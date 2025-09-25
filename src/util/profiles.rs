use rand::prelude::*;
use serde_json::{Map, Value, json};
use std::error::Error;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;

use crate::util::sha1_file;

use crate::util::filesystem::copy_dir_recursive;

/// Generates a random hexadecimal string of the requested length so Nemirtingas
/// receives deterministic-looking IDs instead of regenerating them every boot.
fn generate_hex_id(len: usize) -> String {
    let mut rng = rand::rng();
    (0..len)
        .map(|_| format!("{:x}", rng.random_range(0..16)))
        .collect()
}
use crate::{handler::Handler, paths::*};

// Makes a folder and sets up Goldberg Steam Emu profile for Steam games
pub fn create_profile(name: &str) -> Result<(), std::io::Error> {
    let profile_dir = PATH_PARTY.join(format!("profiles/{name}"));

    if !profile_dir.exists() {
        println!("Creating profile {name}");
        let path_steam = profile_dir.join("steam/settings");
        std::fs::create_dir_all(&path_steam)?;

        let steam_id = format!("{:017}", rand::rng().random_range(u32::MIN..u32::MAX));
        let usersettings = format!(
            "[user::general]\naccount_name={name}\naccount_steamid={steam_id}\nlanguage=english\nip_country=US"
        );
        std::fs::write(path_steam.join("configs.user.ini"), usersettings)?;

        println!("Created successfully");
    }

    std::fs::create_dir_all(profile_dir.join("nepice_settings"))?;

    Ok(())
}

pub fn ensure_nemirtingas_config(
    name: &str,
    appid: &str,
) -> Result<(PathBuf, PathBuf, PathBuf, String), Box<dyn Error>> {
    let profile_dir = PATH_PARTY.join(format!("profiles/{name}"));
    std::fs::create_dir_all(&profile_dir)?;
    create_profile(name)?;

    let nepice_dir = profile_dir.join("nepice_settings");
    std::fs::create_dir_all(&nepice_dir)?;
    let path = nepice_dir.join("NemirtingasEpicEmu.json");

    let mut existing_epicid = None;
    let mut existing_productuserid = None;
    if let Ok(file) = std::fs::File::open(&path) {
        if let Ok(value) = serde_json::from_reader::<_, Value>(file) {
            // Support both the new nested structure and the legacy flat structure so that
            // previously generated profiles keep their IDs without interruption.
            existing_epicid = value
                .pointer("/EOSEmu/User/EpicId")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .or_else(|| {
                    value
                        .get("epicid")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                });
            existing_productuserid = value
                .pointer("/EOSEmu/User/ProductUserId")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .or_else(|| {
                    value
                        .get("productuserid")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                });
        }
    }

    if let Some(ref epicid) = existing_epicid {
        if !epicid.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err("Invalid epicid".into());
        }
    }
    if let Some(ref productuserid) = existing_productuserid {
        if !productuserid.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err("Invalid productuserid".into());
        }
    }

    // Persistently assign Nemirtingas IDs when they are missing so invite codes do not
    // depend on the emulator regenerating identifiers on every launch.
    let epic_id = existing_epicid.unwrap_or_else(|| {
        let new_id = generate_hex_id(32);
        println!(
            "[PARTYDECK] Generated Nemirtingas EpicId {} for profile {}",
            new_id, name
        );
        new_id
    });
    let product_user_id = existing_productuserid.unwrap_or_else(|| {
        let new_id = generate_hex_id(32);
        println!(
            "[PARTYDECK] Generated Nemirtingas ProductUserId {} for profile {}",
            new_id, name
        );
        new_id
    });

    // Build the Nemirtingas configuration with the expected nested layout.
    let mut user_obj = Map::new();
    user_obj.insert("Language".to_string(), json!("en"));
    user_obj.insert("UserName".to_string(), json!(name));
    user_obj.insert("EpicId".to_string(), json!(epic_id.clone()));
    user_obj.insert("ProductUserId".to_string(), json!(product_user_id.clone()));

    let mut obj = Map::new();
    obj.insert(
        "EOSEmu".to_string(),
        json!({
            "Achievements": {
                "OnlineDatabase": ""
            },
            "Application": {
                "AppId": appid,
                "DisableCrashDump": false,
                "DisableOnlineNetworking": false,
                // Keep Nemirtingas at debug verbosity so cross-profile issues remain visible during invite debugging.
                "LogLevel": "Debug",
                "SavePath": "appdata"
            },
            "Ecom": {
                "UnlockDlcs": true
            },
            "Plugins": {
                "Overlay": {
                    "DelayDetection": "5s",
                    "Enabled": true
                }
            },
            "User": user_obj
        }),
    );
    // Enable the broadcast plugin so Nemirtingas advertises the lobby over LAN, allowing
    // other players on the local network to discover the host via invite codes.
    obj.insert(
        "Network".to_string(),
        json!({
            "IceServers": [],
            "Plugins": {
                "Broadcast": {
                    "EnableLog": false,
                    "Enabled": true,
                    "LocalhostOnly": false
                },
                "WebSocket": {
                    "EnableLog": false,
                    "Enabled": false,
                    "SignalingServers": []
                }
            }
        }),
    );
    obj.insert("appid".to_string(), json!(appid));
    obj.insert("language".to_string(), json!("en"));
    // Mirror the nested debug verbosity in the legacy flat configuration for tools still reading the flat keys.
    obj.insert("log_level".to_string(), json!("DEBUG"));
    obj.insert("username".to_string(), json!(name));
    // Surface the generated IDs in the flat layout as well so legacy Nemirtingas builds read them consistently.
    obj.insert("epicid".to_string(), json!(epic_id));
    obj.insert("productuserid".to_string(), json!(product_user_id));

    let data = serde_json::to_string_pretty(&Value::Object(obj))?;
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&path)?;
    file.write_all(data.as_bytes())?;
    file.sync_all()?;

    // Surface the per-profile Nemirtingas log location and ensure the file exists so
    // users immediately know where to inspect debug output after switching log levels.
    let log_path = nepice_dir.join("NemirtingasEpicEmu.log");
    match OpenOptions::new().create(true).append(true).open(&log_path) {
        Ok(_) => println!(
            "[PARTYDECK] Nemirtingas log for profile {} will be written to {}",
            name,
            log_path.display()
        ),
        Err(err) => println!(
            "[PARTYDECK][WARN] Failed to prepare Nemirtingas log for profile {} at {}: {}",
            name,
            log_path.display(),
            err
        ),
    }

    let sha1 = sha1_file(&path)?;
    Ok((nepice_dir, path, log_path, sha1))
}

// Creates the "game save" folder for per-profile game data to go into
pub fn create_gamesave(name: &str, h: &Handler) -> Result<(), Box<dyn Error>> {
    let path_gamesave = PATH_PARTY
        .join("profiles")
        .join(name)
        .join("saves")
        .join(&h.uid);

    if path_gamesave.exists() {
        println!("{} already has save for {}, continuing...", name, h.uid);
        return Ok(());
    }
    println!("Creating game save {} for {}", h.uid, name);

    if h.win_unique_appdata {
        std::fs::create_dir_all(path_gamesave.join("_AppData/Local"))?;
        std::fs::create_dir_all(path_gamesave.join("_AppData/LocalLow"))?;
        std::fs::create_dir_all(path_gamesave.join("_AppData/Roaming"))?;
    }
    if h.win_unique_documents {
        std::fs::create_dir_all(path_gamesave.join("_Documents"))?;
    }
    if h.linux_unique_localshare {
        std::fs::create_dir_all(path_gamesave.join("_share"))?;
    }
    if h.linux_unique_config {
        std::fs::create_dir_all(path_gamesave.join("_config"))?;
    }

    for path in &h.game_unique_paths {
        if path.is_empty() {
            continue;
        }
        // If the path contains a dot, we assume it to be a file, and don't create a directory,
        // hoping that the handler uses copy_to_profilesave to get the relevant file in there.
        // Kind of a hacky solution since folders can technically have dots in their names.
        if path.contains('.') {
            continue;
        }
        println!("Creating subdirectory /{path}");
        let path = path_gamesave.join(path);
        if !path.exists() {
            std::fs::create_dir_all(path)?;
        }
    }

    let copy_save_src = PathBuf::from(&h.path_handler).join("copy_to_profilesave");
    if copy_save_src.exists() {
        println!("{} handler has built-in save data, copying...", h.uid);
        copy_dir_recursive(&copy_save_src, &path_gamesave, false, true, None)?;
    }

    println!("Save data directories created successfully");
    Ok(())
}

// Gets a vector of all available profiles.
// include_guest true for building the profile selector dropdown, false for the profile viewer.
pub fn scan_profiles(include_guest: bool) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();

    if let Ok(entries) = std::fs::read_dir(PATH_PARTY.join("profiles")) {
        for entry in entries {
            if let Ok(entry) = entry {
                if entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false) {
                    if let Some(name) = entry.file_name().to_str() {
                        out.push(name.to_string());
                    }
                }
            }
        }
    }

    out.sort();

    if include_guest {
        out.insert(0, "Guest".to_string());
    }

    out
}

pub fn remove_guest_profiles() -> Result<(), Box<dyn Error>> {
    let path_profiles = PATH_PARTY.join("profiles");
    let entries = std::fs::read_dir(&path_profiles)?;
    for entry in entries.flatten() {
        if !entry.file_type()?.is_dir() {
            continue;
        }

        let name = entry.file_name();
        let name_str = name.to_string_lossy();

        if name_str.starts_with(".") {
            std::fs::remove_dir_all(entry.path())?;
        }
    }
    Ok(())
}

pub static GUEST_NAMES: [&str; 31] = [
    "Blinky", "Pinky", "Inky", "Clyde", "Beatrice", "Battler", "Miyao", "Rena", "Ellie", "Joel",
    "Leon", "Ada", "Madeline", "Theo", "Yokatta", "Wyrm", "Brodiee", "Supreme", "Conk", "Gort",
    "Lich", "Smores", "Canary", "Trico", "Yorda", "Wander", "Agro", "Jak", "Daxter", "Soap",
    "Ghost",
];
