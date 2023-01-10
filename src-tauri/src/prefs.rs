use preferences::{AppInfo, Preferences, PreferencesMap, PreferencesError};

const APP_INFO: AppInfo = AppInfo {
    name: "starbound-modpack-launcher",
    author: "base10",
};
const SETTINGS_KEY: &str = "settings";

fn load_settings() -> PreferencesMap {
	return match PreferencesMap::<String>::load(&APP_INFO, SETTINGS_KEY) {
		Ok(value) => value,
		Err(_) => PreferencesMap::new()
	};
} 

pub fn get_starbound_dir() -> Result<String, String> {
	let settings = load_settings();
	return match settings.get("starbound_dir") {
		Some(value) => Ok(value.to_string()),
		None => Err("Not Configured".into())
	};
}

pub fn set_starbound_dir(value: String) -> Result<(), PreferencesError> {
	let mut settings = load_settings();
	settings.insert("starbound_dir".into(), value);
	
	return settings.save(&APP_INFO, SETTINGS_KEY);
}