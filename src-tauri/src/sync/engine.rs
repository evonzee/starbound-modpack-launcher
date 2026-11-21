//! Scans the player dir, plans against the server manifest, and executes.
use std::{
    collections::{BTreeSet, HashMap},
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::Serialize;
use sha2::{Digest, Sha256};

use super::{
    client::{SyncClient, SyncError},
    plan::{pack_version_newer, plan, Action, Base, Precond, RemoteMeta},
    sbon,
    state::SyncState,
};

const KEEP_BACKUP_SETS: usize = 20;

#[derive(Clone)]
pub struct Ctx {
    pub client: SyncClient,
    /// `<starbound_dir>/base10/storage/player`
    pub dir: PathBuf,
    /// `<starbound_dir>/base10/sync-backups`
    pub backup_root: PathBuf,
    pub state_path: PathBuf,
    /// Installed modpack version (sent as X-Pack-Version, gates pulls).
    pub pack_version: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Pull,
    Push,
    Both,
}

#[derive(Default, Serialize)]
pub struct Summary {
    pub pulled: Vec<String>,
    pub pushed: Vec<String>,
    pub removed: Vec<String>,
    pub conflicts: Vec<String>,
    /// Remote copy needs a newer modpack than is installed.
    pub blocked: Vec<String>,
    pub errors: Vec<String>,
}

impl Summary {
    pub fn is_quiet(&self) -> bool {
        self.pulled.is_empty() && self.pushed.is_empty() && self.removed.is_empty()
    }
}

#[derive(Serialize)]
pub struct FileStatus {
    pub name: String,
    /// synced | ahead | behind | conflict | local-only | remote-only
    /// | deleted-remotely | needs-update
    pub status: String,
    pub local_size: Option<u64>,
    /// Character name (shared by a player's `.player` and `.shipworld`).
    pub character: Option<String>,
    pub remote: Option<RemoteMeta>,
}

fn sha_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

pub fn valid_name(name: &str) -> bool {
    let Some((id, ext)) = name.rsplit_once('.') else { return false };
    (ext == "player" || ext == "shipworld")
        && !id.is_empty()
        && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

struct Local {
    sha: String,
    size: u64,
    /// Character name, for `.player` files.
    character: Option<String>,
}

fn file_id(name: &str) -> &str {
    name.rsplit_once('.').map_or(name, |(id, _)| id)
}

fn scan(dir: &Path) -> HashMap<String, Local> {
    let mut out = HashMap::new();
    let Ok(rd) = fs::read_dir(dir) else { return out };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        if !valid_name(&name) || !e.path().is_file() {
            continue;
        }
        if let Ok(bytes) = fs::read(e.path()) {
            if !bytes.is_empty() {
                let character = name.ends_with(".player").then(|| sbon::player_name(&bytes)).flatten();
                out.insert(name, Local { sha: sha_hex(&bytes), size: bytes.len() as u64, character });
            }
        }
    }
    out
}

fn all_names(local: &HashMap<String, Local>, remote: &HashMap<String, RemoteMeta>) -> BTreeSet<String> {
    local.keys().chain(remote.keys()).cloned().collect()
}

fn blocked(ctx: &Ctx, r: &RemoteMeta) -> bool {
    match (&r.pack_version, &ctx.pack_version) {
        (Some(rv), Some(iv)) => pack_version_newer(rv, iv),
        _ => false,
    }
}

pub async fn status(ctx: &Ctx) -> Result<Vec<FileStatus>, SyncError> {
    let remote: HashMap<_, _> = ctx.client.manifest().await?.into_iter().map(|m| (m.name.clone(), m)).collect();
    let (dir, state_path) = (ctx.dir.clone(), ctx.state_path.clone());
    let local = tauri::async_runtime::spawn_blocking(move || scan(&dir)).await.map_err(|e| SyncError::Other(e.to_string()))?;
    let mut state = SyncState::load(&state_path);
    let mut learned = false;
    for (name, l) in &local {
        if let Some(c) = &l.character {
            if state.name(file_id(name)) != Some(c) {
                state.set_name(file_id(name), c.clone());
                learned = true;
            }
        }
    }
    if learned {
        let _ = state.save(&state_path);
    }

    let mut out = vec![];
    for name in all_names(&local, &remote) {
        let l = local.get(&name);
        let r = remote.get(&name);
        let action = plan(l.map(|l| l.sha.as_str()), r, state.get(&ctx.client.pack, &name));
        let status = match action {
            Action::Noop => "synced",
            Action::Pull if r.is_some_and(|r| blocked(ctx, r)) => "needs-update",
            Action::Pull if l.is_none() => "remote-only",
            Action::Pull => "behind",
            Action::Push(Precond::Absent) => "local-only",
            Action::Push(_) => "ahead",
            Action::Conflict => "conflict",
            Action::DeletedRemotely => "deleted-remotely",
        };
        // Tombstoned remote with no local file: nothing to show.
        if l.is_none() && r.is_some_and(|r| r.deleted) {
            continue;
        }
        let character = state.name(file_id(&name)).cloned();
        out.push(FileStatus {
            name,
            status: status.into(),
            local_size: l.map(|l| l.size),
            character,
            remote: r.cloned(),
        });
    }
    Ok(out)
}

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// Copies `name` from the player dir into this run's backup set (if it exists).
struct Backups {
    set: PathBuf,
    made: bool,
}
impl Backups {
    fn new(ctx: &Ctx) -> Self {
        Self { set: ctx.backup_root.join(format!("{:012}", now_secs())), made: false }
    }
    fn backup(&mut self, dir: &Path, name: &str) -> Result<(), String> {
        let src = dir.join(name);
        if !src.exists() {
            return Ok(());
        }
        fs::create_dir_all(&self.set).map_err(|e| format!("backup dir: {e}"))?;
        fs::copy(&src, self.set.join(name)).map_err(|e| format!("backup {name}: {e}"))?;
        self.made = true;
        Ok(())
    }
    fn prune(&self, root: &Path) {
        let Ok(rd) = fs::read_dir(root) else { return };
        let mut sets: Vec<_> = rd.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
        sets.sort();
        let excess = sets.len().saturating_sub(KEEP_BACKUP_SETS);
        for old in sets.into_iter().take(excess) {
            let _ = fs::remove_dir_all(old);
        }
    }
}

async fn pull_one(ctx: &Ctx, bk: &mut Backups, state: &mut SyncState, r: &RemoteMeta) -> Result<(), SyncError> {
    let d = ctx.client.download(&r.name).await?;
    let actual = sha_hex(&d.bytes);
    // Must match both the header and the manifest entry we planned against
    // (a mismatch means corruption or the head moved mid-sync; retry later).
    if actual != r.sha256 || d.sha256.as_deref().is_some_and(|s| s != actual) {
        return Err(SyncError::Other(format!("{}: downloaded data failed checksum", r.name)));
    }
    bk.backup(&ctx.dir, &r.name).map_err(SyncError::Other)?;
    fs::create_dir_all(&ctx.dir).map_err(|e| SyncError::Other(e.to_string()))?;
    let tmp = ctx.dir.join(format!("{}.tmp", r.name));
    fs::write(&tmp, &d.bytes).map_err(|e| SyncError::Other(e.to_string()))?;
    fs::rename(&tmp, ctx.dir.join(&r.name)).map_err(|e| SyncError::Other(e.to_string()))?;
    if r.name.ends_with(".player") {
        if let Some(c) = sbon::player_name(&d.bytes) {
            state.set_name(file_id(&r.name), c);
        }
    }
    state.set(&ctx.client.pack, &r.name, Base { version: r.version, sha256: actual });
    Ok(())
}

async fn push_one(ctx: &Ctx, state: &mut SyncState, name: &str, pre: Precond) -> Result<(), SyncError> {
    let bytes = fs::read(ctx.dir.join(name)).map_err(|e| SyncError::Other(format!("{name}: {e}")))?;
    let sha = sha_hex(&bytes);
    let meta = ctx.client.upload(name, bytes, &sha, pre, ctx.pack_version.as_deref()).await?;
    state.set(&ctx.client.pack, name, Base { version: meta.version, sha256: sha });
    Ok(())
}

fn remove_one(ctx: &Ctx, bk: &mut Backups, state: &mut SyncState, name: &str) -> Result<(), String> {
    bk.backup(&ctx.dir, name)?;
    let _ = fs::remove_file(ctx.dir.join(name));
    state.clear(&ctx.client.pack, name);
    Ok(())
}

pub async fn run(ctx: &Ctx, mode: Mode) -> Result<Summary, SyncError> {
    let remote: HashMap<_, _> = ctx.client.manifest().await?.into_iter().map(|m| (m.name.clone(), m)).collect();
    let (dir, state_path) = (ctx.dir.clone(), ctx.state_path.clone());
    let local = tauri::async_runtime::spawn_blocking(move || scan(&dir)).await.map_err(|e| SyncError::Other(e.to_string()))?;
    let mut state = SyncState::load(&state_path);
    for (name, l) in &local {
        if let Some(c) = &l.character {
            state.set_name(file_id(name), c.clone());
        }
    }
    let mut bk = Backups::new(ctx);
    let mut sum = Summary::default();
    let pull_ok = mode != Mode::Push;
    let push_ok = mode != Mode::Pull;

    for name in all_names(&local, &remote) {
        let l = local.get(&name);
        let r = remote.get(&name);
        let action = plan(l.map(|l| l.sha.as_str()), r, state.get(&ctx.client.pack, &name));
        let res: Result<(), SyncError> = async {
            match action {
                Action::Noop => {
                    if let Some(r) = r.filter(|r| !r.deleted) {
                        state.set(&ctx.client.pack, &name, Base { version: r.version, sha256: r.sha256.clone() });
                    }
                }
                Action::Pull if pull_ok => {
                    let r = r.expect("pull implies remote");
                    if blocked(ctx, r) {
                        sum.blocked.push(name.clone());
                    } else {
                        pull_one(ctx, &mut bk, &mut state, r).await?;
                        sum.pulled.push(name.clone());
                    }
                }
                Action::DeletedRemotely if pull_ok => {
                    remove_one(ctx, &mut bk, &mut state, &name).map_err(SyncError::Other)?;
                    sum.removed.push(name.clone());
                }
                Action::Push(pre) if push_ok => {
                    push_one(ctx, &mut state, &name, pre).await?;
                    sum.pushed.push(name.clone());
                }
                Action::Conflict => sum.conflicts.push(name.clone()),
                _ => {}
            }
            Ok(())
        }
        .await;
        match res {
            Ok(()) => {}
            Err(SyncError::Conflict(_)) => sum.conflicts.push(name.clone()),
            Err(e @ SyncError::Unauthorized) => return Err(e),
            Err(e) => sum.errors.push(format!("{name}: {e}")),
        }
        // Persist as we go so a crash never loses track of what we synced.
        let _ = state.save(&ctx.state_path);
    }
    if bk.made {
        bk.prune(&ctx.backup_root);
    }
    Ok(sum)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Keep {
    Local,
    Remote,
}

/// Resolve a conflict explicitly. The loser stays in server history / local backup.
pub async fn resolve(ctx: &Ctx, name: &str, keep: Keep) -> Result<(), SyncError> {
    if !valid_name(name) {
        return Err(SyncError::Other("invalid file name".into()));
    }
    let head = ctx.client.manifest().await?.into_iter().find(|m| m.name == name);
    let mut state = SyncState::load(&ctx.state_path);
    let mut bk = Backups::new(ctx);
    match keep {
        Keep::Local => {
            let pre = head.map_or(Precond::Absent, |h| Precond::Version(h.version));
            push_one(ctx, &mut state, name, pre).await?;
        }
        Keep::Remote => match head {
            Some(h) if !h.deleted => pull_one(ctx, &mut bk, &mut state, &h).await?,
            Some(_) => remove_one(ctx, &mut bk, &mut state, name).map_err(SyncError::Other)?,
            None => return Err(SyncError::Other("no copy on the server".into())),
        },
    }
    state.save(&ctx.state_path).map_err(SyncError::Other)?;
    if bk.made {
        bk.prune(&ctx.backup_root);
    }
    Ok(())
}

/// Roll back: make `version` the server head, then pull it (local backed up).
pub async fn restore(ctx: &Ctx, name: &str, version: i64) -> Result<(), SyncError> {
    if !valid_name(name) {
        return Err(SyncError::Other("invalid file name".into()));
    }
    let head = ctx.client.restore(name, version).await?;
    let mut state = SyncState::load(&ctx.state_path);
    let mut bk = Backups::new(ctx);
    pull_one(ctx, &mut bk, &mut state, &head).await?;
    state.save(&ctx.state_path).map_err(SyncError::Other)?;
    if bk.made {
        bk.prune(&ctx.backup_root);
    }
    Ok(())
}
