//! Pure sync planning: decides what to do per file. No I/O here.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RemoteMeta {
    pub name: String,
    pub version: i64,
    pub sha256: String,
    pub size: i64,
    pub pack_version: Option<String>,
    pub updated_by: String,
    pub updated_at: i64,
    pub deleted: bool,
}

/// Last state we successfully synced for a file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Base {
    pub version: i64,
    pub sha256: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Precond {
    /// `If-None-Match: *`
    Absent,
    /// `If-Match: "<n>"`
    Version(i64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Local and remote already match; just record the base.
    Noop,
    Pull,
    Push(Precond),
    Conflict,
    /// Deleted on another machine and unchanged here: back up + remove locally.
    DeletedRemotely,
}

/// `local` is the sha256 (lowercase hex) of the local file, if it exists.
pub fn plan(local: Option<&str>, remote: Option<&RemoteMeta>, base: Option<&Base>) -> Action {
    // A tombstone with no content is "absent" for matching purposes but needs
    // care because pushing over it requires its version.
    match remote {
        None => match local {
            None => Action::Noop,
            Some(_) => Action::Push(Precond::Absent),
        },
        Some(r) if r.deleted => match (local, base) {
            (None, _) => Action::Noop,
            (Some(l), Some(b)) if l == b.sha256 => Action::DeletedRemotely,
            (Some(_), _) => Action::Conflict,
        },
        Some(r) => match local {
            None => Action::Pull, // never auto-delete remotely; restore locally
            Some(l) if l == r.sha256 => Action::Noop,
            Some(l) => match base {
                None => Action::Conflict,
                Some(b) if l == b.sha256 => Action::Pull,
                Some(b) if r.version == b.version => Action::Push(Precond::Version(b.version)),
                Some(_) => Action::Conflict,
            },
        },
    }
}

/// Is `remote` strictly newer than `installed`? Compares dotted numbers when
/// both parse; otherwise any inequality counts as "different" (true).
pub fn pack_version_newer(remote: &str, installed: &str) -> bool {
    fn parse(s: &str) -> Option<Vec<u64>> {
        s.trim_start_matches('v').split('.').map(|p| p.parse().ok()).collect()
    }
    match (parse(remote), parse(installed)) {
        (Some(r), Some(i)) => r > i,
        _ => remote != installed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(version: i64, sha: &str, deleted: bool) -> RemoteMeta {
        RemoteMeta {
            name: "a.player".into(),
            version,
            sha256: sha.into(),
            size: 1,
            pack_version: None,
            updated_by: "m".into(),
            updated_at: 0,
            deleted,
        }
    }
    fn b(version: i64, sha: &str) -> Base {
        Base { version, sha256: sha.into() }
    }

    #[test]
    fn same_sha_is_noop() {
        assert_eq!(plan(Some("x"), Some(&r(3, "x", false)), None), Action::Noop);
        assert_eq!(plan(Some("x"), Some(&r(3, "x", false)), Some(&b(1, "y"))), Action::Noop);
    }
    #[test]
    fn nothing_anywhere() {
        assert_eq!(plan(None, None, None), Action::Noop);
        assert_eq!(plan(None, None, Some(&b(1, "x"))), Action::Noop);
    }
    #[test]
    fn local_only_pushes_new() {
        assert_eq!(plan(Some("x"), None, None), Action::Push(Precond::Absent));
    }
    #[test]
    fn remote_only_pulls() {
        assert_eq!(plan(None, Some(&r(1, "x", false)), None), Action::Pull);
    }
    #[test]
    fn first_sync_both_differ_conflicts() {
        assert_eq!(plan(Some("a"), Some(&r(1, "b", false)), None), Action::Conflict);
    }
    #[test]
    fn local_unchanged_remote_newer_pulls() {
        assert_eq!(plan(Some("a"), Some(&r(2, "b", false)), Some(&b(1, "a"))), Action::Pull);
    }
    #[test]
    fn local_changed_remote_same_pushes() {
        assert_eq!(
            plan(Some("c"), Some(&r(1, "a", false)), Some(&b(1, "a"))),
            Action::Push(Precond::Version(1))
        );
    }
    #[test]
    fn both_changed_conflicts() {
        assert_eq!(plan(Some("c"), Some(&r(2, "b", false)), Some(&b(1, "a"))), Action::Conflict);
    }
    #[test]
    fn local_missing_restores() {
        assert_eq!(plan(None, Some(&r(1, "a", false)), Some(&b(1, "a"))), Action::Pull);
    }
    #[test]
    fn remote_deleted_local_unchanged() {
        assert_eq!(plan(Some("a"), Some(&r(2, "", true)), Some(&b(1, "a"))), Action::DeletedRemotely);
    }
    #[test]
    fn remote_deleted_local_changed_or_unknown() {
        assert_eq!(plan(Some("c"), Some(&r(2, "", true)), Some(&b(1, "a"))), Action::Conflict);
        assert_eq!(plan(Some("c"), Some(&r(2, "", true)), None), Action::Conflict);
    }
    #[test]
    fn remote_deleted_local_missing() {
        assert_eq!(plan(None, Some(&r(2, "", true)), Some(&b(1, "a"))), Action::Noop);
    }
    #[test]
    fn pack_versions() {
        assert!(pack_version_newer("1.10.0", "1.9.0"));
        assert!(!pack_version_newer("1.2.0", "1.2.0"));
        assert!(!pack_version_newer("1.1.0", "1.2.0"));
        assert!(pack_version_newer("abc", "def"));
    }
}
