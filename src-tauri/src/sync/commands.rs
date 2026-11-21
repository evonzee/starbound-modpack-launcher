//! Tauri commands and the launch-time hooks for player sync.
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
use tauri_plugin_notification::NotificationExt;

use super::{
    client::{Lease, SyncClient, SyncError},
    engine::{self, Ctx, FileStatus, Keep, Mode, Summary},
    plan::RemoteMeta,
};
use crate::{get_modpack_config, log, prefs};

/// The pack all data is filed under. Single pack for now; pass it down rather
/// than scattering the literal, since a switchable pack is planned.
pub const PACK_ID: &str = "base10";

#[derive(Serialize, Deserialize)]
pub struct Settings {
    pub url: String,
    pub token: String,
    pub enabled: bool,
    pub machine: String,
}

fn default_machine() -> String {
    std::env::var("HOSTNAME")
        .or_else(|_| std::env::var("COMPUTERNAME"))
        .ok()
        .or_else(|| std::fs::read_to_string("/etc/hostname").ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".into())
}

fn load_settings() -> Settings {
    Settings {
        url: prefs::get_string("sync_url").unwrap_or_default(),
        token: prefs::get_string("sync_token").unwrap_or_default(),
        enabled: prefs::get_string("sync_enabled").as_deref() == Some("true"),
        machine: prefs::get_string("machine_name")
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(default_machine),
    }
}

#[tauri::command]
pub fn sync_get_settings() -> Settings {
    load_settings()
}

#[tauri::command]
pub fn sync_set_settings(settings: Settings) -> Result<(), String> {
    let e = |e: preferences::PreferencesError| e.to_string();
    prefs::set_string("sync_url", settings.url.trim().to_string()).map_err(e)?;
    prefs::set_string("sync_token", settings.token.trim().to_string()).map_err(e)?;
    prefs::set_string("sync_enabled", settings.enabled.to_string()).map_err(e)?;
    prefs::set_string("machine_name", settings.machine.trim().chars().take(64).collect()).map_err(e)
}

/// `Err` means "not configured": callers treat that as sync silently off.
fn build_ctx(app: &AppHandle, require_enabled: bool) -> Result<Ctx, String> {
    let s = load_settings();
    if require_enabled && !s.enabled {
        return Err("Sync is disabled".into());
    }
    if s.url.is_empty() || s.token.is_empty() {
        return Err("Sync server URL and token are not set".into());
    }
    let sb = prefs::get_starbound_dir()?;
    let base10 = std::path::Path::new(&sb).join("base10");
    let state_path = app
        .path()
        .app_config_dir()
        .map_err(|e| e.to_string())?
        .join("sync_state.json");
    let pack_version = get_modpack_config("mods.json").ok().and_then(|c| c.modpack_version());
    let client = SyncClient::new(&s.url, &s.token, PACK_ID, &s.machine).map_err(|e| e.to_string())?;
    Ok(Ctx {
        client,
        dir: base10.join("storage").join("player"),
        backup_root: base10.join("sync-backups"),
        state_path,
        pack_version,
    })
}

#[tauri::command]
pub async fn sync_test_connection(settings: Settings) -> Result<String, String> {
    let client = SyncClient::new(&settings.url, &settings.token, PACK_ID, &settings.machine)
        .map_err(|e| e.to_string())?;
    client.test().await.map_err(|e| e.to_string())?;
    Ok("Connection OK".into())
}

#[derive(Serialize)]
pub struct StatusReport {
    pub files: Vec<FileStatus>,
    pub lease: Option<Lease>,
    pub machine: String,
}

#[tauri::command]
pub async fn sync_status(app: AppHandle) -> Result<StatusReport, String> {
    let ctx = build_ctx(&app, false)?;
    let files = engine::status(&ctx).await.map_err(|e| e.to_string())?;
    let lease = ctx.client.lease_get().await.ok().flatten();
    Ok(StatusReport { files, lease, machine: ctx.client.machine.clone() })
}

#[tauri::command]
pub async fn sync_now(app: AppHandle) -> Result<Summary, String> {
    let ctx = build_ctx(&app, false)?;
    engine::run(&ctx, Mode::Both).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn sync_resolve(app: AppHandle, name: String, keep: Keep) -> Result<(), String> {
    let ctx = build_ctx(&app, false)?;
    engine::resolve(&ctx, &name, keep).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn sync_history(app: AppHandle, name: String) -> Result<Vec<RemoteMeta>, String> {
    if !engine::valid_name(&name) {
        return Err("invalid file name".into());
    }
    let ctx = build_ctx(&app, false)?;
    ctx.client.history(&name).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn sync_restore(app: AppHandle, name: String, version: i64) -> Result<(), String> {
    let ctx = build_ctx(&app, false)?;
    engine::restore(&ctx, &name, version).await.map_err(|e| e.to_string())
}

// ---------- launch integration ----------

fn notify(app: &AppHandle, body: &str) {
    let _ = app.notification().builder().title("Starbound sync").body(body).show();
}

fn report(app: &AppHandle, what: &str, sum: &Summary) {
    if !sum.is_quiet() {
        log(
            app,
            &format!(
                "Sync {what}: {} pulled, {} pushed, {} removed",
                sum.pulled.len(),
                sum.pushed.len(),
                sum.removed.len()
            ),
        );
    }
    for e in &sum.errors {
        log(app, &format!("Sync error: {e}"));
    }
    for b in &sum.blocked {
        log(app, &format!("Sync: {b} was saved with a newer modpack; update the modpack first"));
    }
}

/// Active sync for one game session. Created by `begin`; `None` means sync is
/// off or unreachable and the launch proceeds without it.
pub struct Session {
    ctx: Ctx,
}

/// Pre-launch: take the lease, pull. `Ok(None)` = continue without sync,
/// `Err(msg)` = abort the launch (user declined, or conflicts need resolving).
pub async fn begin(app: &AppHandle) -> Result<Option<Session>, String> {
    let ctx = match build_ctx(app, true) {
        Ok(c) => c,
        Err(_) => return Ok(None),
    };

    match ctx.client.lease_acquire(false).await {
        Ok(()) => {}
        Err(SyncError::LeaseHeld(h)) => {
            let msg = format!(
                "Starbound appears to be running on \"{}\". Playing on two machines at once can lose progress.\n\nContinue anyway?",
                h.machine
            );
            let app2 = app.clone();
            let yes = tauri::async_runtime::spawn_blocking(move || {
                app2.dialog()
                    .message(msg)
                    .title("Already playing elsewhere")
                    .kind(MessageDialogKind::Warning)
                    .buttons(MessageDialogButtons::OkCancelCustom("Continue anyway".into(), "Cancel".into()))
                    .blocking_show()
            })
            .await
            .unwrap_or(false);
            if !yes {
                return Err("Launch cancelled".into());
            }
            if let Err(e) = ctx.client.lease_acquire(true).await {
                log(app, &format!("Sync disabled for this launch: {e}"));
                return Ok(None);
            }
        }
        Err(e) => {
            log(app, &format!("Sync unavailable, launching without it: {e}"));
            if matches!(e, SyncError::Unauthorized) {
                notify(app, "Sync token was rejected; check your sync settings.");
            }
            return Ok(None);
        }
    }

    log(app, "Syncing player data...");
    match engine::run(&ctx, Mode::Pull).await {
        Ok(sum) => {
            report(app, "pull", &sum);
            if !sum.conflicts.is_empty() {
                let _ = ctx.client.lease_release().await;
                let _ = app.emit("sync-conflicts", sum.conflicts.clone());
                notify(app, "Sync conflicts must be resolved before launching.");
                return Err(format!(
                    "Sync conflict in: {}. Resolve in the Sync window, then launch again.",
                    sum.conflicts.join(", ")
                ));
            }
            Ok(Some(Session { ctx }))
        }
        Err(e) => {
            log(app, &format!("Sync failed, launching without it: {e}"));
            let _ = ctx.client.lease_release().await;
            Ok(None)
        }
    }
}

impl Session {
    /// Runs the lease-renew timer until the returned guard is dropped.
    pub fn keep_alive(&self) -> KeepAlive {
        let client = self.ctx.client.clone();
        KeepAlive(tauri::async_runtime::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_secs(300));
            tick.tick().await; // first tick fires immediately
            loop {
                tick.tick().await;
                let _ = client.lease_acquire(false).await;
            }
        }))
    }

    /// Post-exit: push, then always release the lease.
    pub async fn finish(self, app: &AppHandle) {
        log(app, "Pushing player data...");
        match engine::run(&self.ctx, Mode::Push).await {
            Ok(sum) => {
                report(app, "push", &sum);
                if !sum.conflicts.is_empty() {
                    let _ = app.emit("sync-conflicts", sum.conflicts.clone());
                    notify(app, "Sync conflict: your changes were NOT uploaded. Open the Sync window to resolve.");
                    log(app, &format!("Sync conflict in: {}", sum.conflicts.join(", ")));
                } else if !sum.errors.is_empty() {
                    notify(app, "Some player files failed to upload; they will retry next launch.");
                }
            }
            Err(e) => {
                notify(app, &format!("Couldn't upload player data: {e}. It will retry next launch."));
                log(app, &format!("Sync push failed: {e}"));
            }
        }
        let _ = self.ctx.client.lease_release().await;
    }
}

pub struct KeepAlive(tauri::async_runtime::JoinHandle<()>);
impl Drop for KeepAlive {
    fn drop(&mut self) {
        self.0.abort();
    }
}
