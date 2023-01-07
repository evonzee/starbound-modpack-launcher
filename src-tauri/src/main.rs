#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

// Learn more about Tauri commands at https://tauri.app/v1/guides/features/command
#[tauri::command]
fn load_install_location() -> String {
    return "Some installation location!".to_string();
}

#[tauri::command]
fn get_available_version() -> String {
    return "v1.0.2".to_string();
}

#[tauri::command]
fn get_installed_version() -> String {
    return "v1.0.1".to_string();
}

#[tauri::command]
fn change_starbound_location() -> String {
    return "somewhere else".to_string();
}

#[tauri::command]
async fn update(window: tauri::Window) {
    // do something
    set_status(window, "Starting update process");
    ();
}

#[tauri::command]
fn launch() -> () {
    // do something
}

// the payload type must implement `Serialize` and `Clone`.
#[derive(Clone, serde::Serialize)]
struct StatusMessage {
  message: String,
}

fn set_status(window: tauri::Window, message: &str) {
    let result = window.emit("status", StatusMessage { message: message.into() });
    match result {
        Ok(()) => true,
        Err(_) => false
    };
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            load_install_location, 
            get_available_version, 
            get_installed_version,
            change_starbound_location,
            update,
            launch
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
