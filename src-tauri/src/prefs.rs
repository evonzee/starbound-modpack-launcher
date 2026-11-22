use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::OnceLock,
};

use tauri::{AppHandle, Manager};

const SETTINGS_FILE: &str = "settings.json";

static CONFIG_DIR: OnceLock<PathBuf> = OnceLock::new();

type Settings = BTreeMap<String, String>;

/// Resolve the app config dir (Tauri's `app_config_dir`) and migrate settings
/// from the legacy `preferences`-crate location. Call once from `setup`.
pub fn init(app: &AppHandle) -> Result<(), String> {
    let dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

    let new_file = dir.join(SETTINGS_FILE);
    if !new_file.exists() {
        if let Some(old) = legacy_settings_file(app) {
            if old.is_file() {
                let _ = fs::copy(&old, &new_file);
            }
        }
    }

    let _ = CONFIG_DIR.set(dir);
    Ok(())
}

/// Where the `preferences` crate used to store `settings.prefs.json`.
fn legacy_settings_file(app: &AppHandle) -> Option<PathBuf> {
    const NAME: &str = "starbound-modpack-launcher";
    const FILE: &str = "settings.prefs.json";
    let p = app.path();

    #[cfg(target_os = "linux")]
    let dir = p.config_dir().ok()?.join(NAME);
    #[cfg(target_os = "macos")]
    let dir = p.home_dir().ok()?.join("Library/Preferences").join(NAME);
    #[cfg(target_os = "windows")]
    let dir = p.data_dir().ok()?.join("base10").join(NAME).join("config");
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    let dir = p.config_dir().ok()?.join(NAME);

    Some(dir.join(FILE))
}

fn settings_path() -> Result<&'static Path, String> {
    CONFIG_DIR
        .get()
        .map(|d| d.as_path())
        .ok_or_else(|| "config directory not initialised".to_string())
}

fn load_settings() -> Settings {
    settings_path()
        .ok()
        .and_then(|d| fs::read(d.join(SETTINGS_FILE)).ok())
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

fn save_settings(settings: &Settings) -> Result<(), String> {
    let dir = settings_path()?;
    let json = serde_json::to_vec_pretty(settings).map_err(|e| e.to_string())?;
    let tmp = dir.join(format!("{SETTINGS_FILE}.tmp"));
    fs::write(&tmp, json).map_err(|e| e.to_string())?;
    fs::rename(&tmp, dir.join(SETTINGS_FILE)).map_err(|e| e.to_string())
}

pub fn get_starbound_dir() -> Result<String, String> {
    get_string("starbound_dir").ok_or_else(|| "Not Configured".into())
}

pub fn set_starbound_dir(value: String) -> Result<(), String> {
    set_string("starbound_dir", value)
}

pub fn get_string(key: &str) -> Option<String> {
    load_settings().get(key).cloned()
}

pub fn set_string(key: &str, value: String) -> Result<(), String> {
    let mut settings = load_settings();
    settings.insert(key.into(), value);
    save_settings(&settings)
}
