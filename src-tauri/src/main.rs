#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

mod modinfo;
mod prefs;

use core::fmt;
use std::{
    cmp::min,
    collections::HashMap,
    env,
    error::Error,
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
};

use futures_util::StreamExt;
use modinfo::ModpackConfig;
use reqwest::Client;
use rfd::FileDialog;

// Learn more about Tauri commands at https://tauri.app/v1/guides/features/command
#[tauri::command]
fn load_install_location() -> Result<String, String> {
    prefs::get_starbound_dir()
}

#[tauri::command]
async fn get_available_version(window: tauri::Window) -> String {
    let res = download_file_to_mods(
        &window,
        "https://www.grayles.com/modpack/mods.json",
        "mods.json.new",
    )
    .await;

    match res {
        Ok(()) => get_modpack_version("mods.json.new"),
        Err(val) => val,
    }
}

#[tauri::command]
fn get_installed_version() -> String {
    get_modpack_version("mods.json")
}

#[tauri::command]
fn change_starbound_location(window: tauri::Window) -> Result<String, String> {
    let initial_dir = match load_install_location() {
        Ok(loc) => loc,
        Err(_) => "/".to_string(),
    };

    if let Some(folder) = FileDialog::new().set_directory(initial_dir).pick_folder() {
        match folder.to_str() {
            None => (),
            Some(value) => {
                match prefs::set_starbound_dir(value.into()) {
                    Ok(()) => (),
                    Err(_) => {
                        log(&window, "Couldn't find starbound in the selected location!");
                        return Err("Failed to save preferences file!".into());
                    }
                }

                match scan_and_write_config_file() {
                    Ok(()) => (),
                    Err(err) => {
                        log(
                            &window,
                            format!(
                                "Couldn't write modpack config file in the selected location! {}",
                                err
                            )
                            .as_str(),
                        );
                        return Err("Failed to write Starbound config file!".into());
                    }
                }
            }
        }
    };

    load_install_location()
}

#[tauri::command]
async fn update(window: tauri::Window) {
    log(&window, "Starting update process");

    let config = get_modpack_config("mods.json.new").unwrap();
    let oldconfig = get_modpack_config("mods.json").ok();

    remove_old_mods(&window, &oldconfig, &config);
    let res = download_new_mods(&window, &oldconfig, &config).await.unwrap_or(false);

    if res {
        download_file_to_mods(
            &window,
            "https://www.grayles.com/modpack/mods.json",
            "mods.json",
        )
        .await.unwrap();
    }
}

fn remove_old_mods(window: &tauri::Window, oldconfig: &Option<ModpackConfig>, config: &ModpackConfig) {
    if let Some(old) = oldconfig {
        for modinfo in old.mods.iter() {
            if !config.mods.iter().any(|newmod| {
                modinfo.name == newmod.name && modinfo.last_change == newmod.last_change && modinfo.checksum == newmod.checksum
            }) {
                // remove mod
                log(window, format!("Removing old mod {}", modinfo.name).as_str());
                if remove_file_from_mods(format!("{}.pak", modinfo.name).as_str()).is_err() {
                    log(window, "Failed to remove file!");
                }
            }
        }
    }
    log(window, "Finished removing old mods");
}

async fn download_new_mods(window: &tauri::Window, oldconfig: &Option<ModpackConfig>, config: &ModpackConfig) -> Result<bool, Box<dyn Error>> {
    let mods = config.mods.iter()
        .filter(|newmod| {
            if let Some(old) = oldconfig {
                return !old.mods.iter().any(|modinfo| {
                    modinfo.name == newmod.name && modinfo.last_change == newmod.last_change && modinfo.checksum == newmod.checksum
                })
            }
            true
        });
    
    for modinfo in mods {
        log(window, format!("Downloading new mod {}", modinfo.name).as_str());
        let modfile = format!("{}.pak", modinfo.name);
        let url = format!("https://www.grayles.com/modpack/files/{}.pak", modinfo.name);
        download_file_to_mods(window, url.as_str(), modfile.as_str()).await?;
    }

    Ok(true)
}

#[tauri::command]
async fn launch(window: tauri::Window) {
    let initial_dir = match load_install_location() {
        Ok(loc) => Path::new(&loc).to_path_buf(),
        Err(_) => {
            log(&window, "Starbound location is not set; cannot launch");
            return;
        },
    };
    log(&window, "Launching starbound...");
    log(&window, launch_starbound(initial_dir).await.as_str());
}

#[cfg(target_os = "macos")]
async fn launch_starbound(mut path: PathBuf) ->  String {
    path.push("osx");
    let executable = "Starbound.app/Contents/MacOS/starbound";
    let mut env = HashMap::new();

    run_starbound(path, executable, env)
}
#[cfg(target_os = "linux")]
async fn launch_starbound(mut path: PathBuf) -> String  {
    path.push("linux");
    let executable = "starbound";
    let mut env = HashMap::new();
    env.insert("LD_LIBRARY_PATH", format!("./:{}", env::var("LD_LIBRARY_PATH").unwrap_or("".to_string()))); 

    run_starbound(path, executable, env)
}
#[cfg(target_os = "windows")]
async fn launch_starbound(mut path: PathBuf) ->  String  {
    path.push("win64");
    let executable = "starbound.exe";
    let mut env = HashMap::new();

    run_starbound(path, executable, env)
}

fn run_starbound(mut path: PathBuf, executable: &str, env: std::collections::HashMap<&str, String>) -> String {
    let cwd = path.clone();

    path.push(executable);
    let mut command = std::process::Command::new(path);
    command.current_dir(cwd);
    command.arg("-bootconfig");
    command.arg("grayles-modpack.config");

    for ele in env {
        command.env(ele.0, ele.1);    
    }

    let exitstatus = match command.spawn() {
        Ok(mut child) => child.wait(),
        Err(_) => return "Error starting starbound!".to_string()
    };

    match exitstatus {
        Ok(status) => if status.success() { "Starbound exited OK" } else { "Starbound exited abnormally!" }
        Err(_) => "Error getting starbound exit code!",
    }.to_string()
}

#[derive(Debug)]
struct StarboundNotFound(String);
impl Error for StarboundNotFound {}
impl fmt::Display for StarboundNotFound {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "There is an error: {}", self.0)
    }
}
fn remove_file_from_mods(filename: &str) -> Result<(), String> {
    let mut path = get_mods_dir();
    path.push(filename);
    let path = path.to_str().unwrap_or("nofilename");

    fs::remove_file(path).or(Err("Failed to remove file!".to_string()))
}

async fn download_file_to_mods(
    window: &tauri::Window,
    url: &str,
    filename: &str,
) -> Result<(), String> {
    let client = Client::new();

    // Reqwest setup
    let res = client
        .get(url)
        .send()
        .await
        .or(Err(format!("Failed to GET from '{}'", &url)))?;
    if !res.status().is_success() {
        log(window, format!("Failed to download {}!", filename).as_str());
        return Err("Error downloading file!".to_string());
    }

    let total_size = res
        .content_length()
        .ok_or(format!("Failed to get content length from '{}'", &url))?;

    // download chunks

    let mut path = get_mods_dir();
    path.push(filename);
    let path = path.to_str().unwrap_or("nofilename");

    let mut file = File::create(path).or(Err(format!("Failed to create file '{}'", path)))?;
    let mut downloaded: u64 = 0;
    let mut stream = res.bytes_stream();

    while let Some(item) = stream.next().await {
        let chunk = item.or(Err("Error while downloading file"))?;
        file.write_all(&chunk)
            .or(Err("Error while writing to file"))?;
        let new = min(downloaded + (chunk.len() as u64), total_size);
        downloaded = new;
        set_status(
            window,
            format!(
                "Downloading {}: {} of {} bytes",
                filename, downloaded, total_size
            )
            .as_str(),
        );
    }

    // update statusbar pb.finish_with_message(&format!("Downloaded {} to {}", url, path));
    log(
        window,
        format!("Finished Downloading {}", filename).as_str(),
    );

    Ok(())
}

fn get_modpack_version(filename: &str) -> String {
    let maybe_config = get_modpack_config(filename);

    match maybe_config {
        Ok(config) => config
            .modpack_version()
            .unwrap_or("Mod config file is missing version metadata!".to_string()),
        Err(_) => "No modpack config found, or file is corrupted".to_string(),
    }
}

fn get_modpack_config(filename: &str) -> Result<ModpackConfig, String> {
    let mut path = get_mods_dir();
    path.push(filename);
    if let Some(configpath) = path.to_str() {
        return match modinfo::read_mods(configpath) {
            Ok(value) => Ok(value),
            Err(err) => Err(err.to_string()),
        };
    }

    Err("Starbound path is not configured!".to_string())
}

fn get_mods_dir() -> PathBuf {
    let loc = prefs::get_starbound_dir().unwrap_or(String::new());
    let mut path = Path::new(&loc).to_path_buf();
    path.push("grayles/mods/");

    path
}

fn scan_and_write_config_file() -> Result<(), Box<dyn Error>> {
    let loc = prefs::get_starbound_dir()?;
    let path = Path::new(&loc);

    for entry in path.read_dir()? {
        let subpath = entry?.path();
        if subpath.file_name().ok_or("")?.eq_ignore_ascii_case("linux") {
            return write_config_file_to_dir(subpath);
        }
        if subpath.file_name().ok_or("")?.eq_ignore_ascii_case("win64") {
            return write_config_file_to_dir(subpath);
        }
        if subpath.file_name().ok_or("")?.eq_ignore_ascii_case("osx") {
            return write_config_file_to_dir(subpath);
        }
    }

    Err(Box::new(StarboundNotFound(
        "No linux or windows subdirectory found in selected folder!".into(),
    )))
}

fn write_config_file_to_dir(path: PathBuf) -> Result<(), Box<dyn Error>> {
    println!("Found config dir {:?}", path.to_str());
    let config = include_str!("modpack.config");
    let mut filepath = path.clone();
    filepath.push("grayles-modpack.config");
    println!("Writing config file to {:?}", filepath.to_str());
    fs::write(filepath, config)?;

    let mut grayles_dir = path.clone();
    grayles_dir.push("../grayles");
    if !grayles_dir.exists() {
        fs::create_dir(grayles_dir)?;
    }

    let mut grayles_dir = path.clone();
    grayles_dir.push("../grayles/storage");
    if !grayles_dir.exists() {
        fs::create_dir(grayles_dir)?;
    }

    let mut grayles_dir = path;
    grayles_dir.push("../grayles/mods");
    if !grayles_dir.exists() {
        fs::create_dir(grayles_dir)?;
    }

    Ok(())
}

// the payload type must implement `Serialize` and `Clone`.
#[derive(Clone, serde::Serialize)]
struct StatusMessage {
    message: String,
}

fn set_status(window: &tauri::Window, message: &str) {
    let result = window.emit(
        "status",
        StatusMessage {
            message: message.into(),
        },
    );
    match result {
        Ok(()) => true,
        Err(_) => false,
    };
}

fn log(window: &tauri::Window, message: &str) {
    let result = window.emit(
        "log",
        StatusMessage {
            message: message.into(),
        },
    );
    match result {
        Ok(()) => true,
        Err(_) => false,
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
