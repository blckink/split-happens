use rand::prelude::*;
use serde_json::{Map, Value, json};
use sha1::{Digest, Sha1};
use std::error::Error;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;

use crate::util::filesystem::copy_dir_recursive;
use crate::util::sha1_file;
use crate::{handler::Handler, paths::*};

/// Generates a random hexadecimal string of the requested length so Nemirtingas
/// receives deterministic-looking IDs instead of regenerating them every boot.
fn generate_hex_id(len: usize) -> String {
    let mut rng = rand::rng();
    (0..len)
        .map(|_| format!("{:x}", rng.random_range(0..16)))
        .collect()
}

/// Creates a deterministic hexadecimal identifier by hashing the provided seed and
/// extending it with a counter if additional entropy is required.
fn deterministic_hex_from_seed(seed: &str, len: usize) -> String {
    let mut output = String::new();
    let mut counter: u32 = 0;

    while output.len() < len {
        let mut hasher = Sha1::new();
        hasher.update(seed.as_bytes());
        if counter > 0 {
            hasher.update(counter.to_le_bytes());
        }

        let digest = hasher.finalize();
        output.push_str(&format!("{:x}", digest));
        counter = counter.saturating_add(1);
    }

    output.truncate(len);
    output
}

/// Validates that the provided identifier is a non-empty hexadecimal string; Nemirtingas
/// rejects any other characters and regenerates the value if required.
fn is_valid_hex(value: &str) -> bool {
    // Allow callers to supply identifiers with an optional 0x/0X prefix before checking the remaining characters.
    let trimmed = value.trim_start_matches("0x").trim_start_matches("0X");
    !trimmed.is_empty() && trimmed.chars().all(|c| c.is_ascii_hexdigit())
}

/// Logs a warning both to stdout and the persistent launch warning log so profile repairs are
/// visible even after the session ends.
fn log_profile_warning(message: &str) {
    println!("[PARTYDECK][WARN] {message}");

    let log_dir = PATH_PARTY.join("logs");
    if let Err(err) = std::fs::create_dir_all(&log_dir) {
        println!(
            "[PARTYDECK][WARN] Failed to prepare launch log directory {}: {}",
            log_dir.display(),
            err
        );
        return;
    }

    let log_path = log_dir.join("launch_warnings.txt");
    if let Err(err) = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .and_then(|mut file| writeln!(file, "[WARN] {message}"))
    {
        println!(
            "[PARTYDECK][WARN] Failed to persist launch warning log {}: {}",
            log_path.display(),
            err
        );
    }
}

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
    let mut existing_username = None;
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
            existing_username = value
                .pointer("/EOSEmu/User/UserName")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .or_else(|| {
                    value
                        .get("username")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                });
        }
    }

    let existing_epicid = existing_epicid.and_then(|id| {
        if is_valid_hex(&id) {
            Some(id)
        } else {
            log_profile_warning(&format!(
                "Profile {name} contained invalid Nemirtingas EpicId {id}; regenerating."
            ));
            None
        }
    });

    let existing_productuserid = existing_productuserid.and_then(|id| {
        if is_valid_hex(&id) {
            Some(id)
        } else {
            log_profile_warning(&format!(
                "Profile {name} contained invalid Nemirtingas ProductUserId {id}; regenerating."
            ));
            None
        }
    });

    let profile_username = existing_username
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| name.to_string());
    let uses_default_username = profile_username == "DefaultName";

    // Persistently assign Nemirtingas IDs when they are missing so invite codes do not
    // depend on the emulator regenerating identifiers on every launch.
    let epic_id = existing_epicid.unwrap_or_else(|| {
        let new_id = if uses_default_username {
            generate_hex_id(32)
        } else {
            deterministic_hex_from_seed(&profile_username, 32)
        };
        println!(
            "[PARTYDECK] Generated Nemirtingas EpicId {} for profile {} using {} mode",
            new_id,
            name,
            if uses_default_username {
                "random"
            } else {
                "deterministic"
            }
        );
        new_id
    });
    let product_user_id = existing_productuserid.unwrap_or_else(|| {
        let seed = format!("{appid}:{epic_id}");
        let new_id = deterministic_hex_from_seed(&seed, 32);
        println!(
            "[PARTYDECK] Generated Nemirtingas ProductUserId {} for profile {} using deterministic seed",
            new_id,
            name
        );
        new_id
    });

    // Build the Nemirtingas configuration with the expected nested layout.
    let mut user_obj = Map::new();
    user_obj.insert("Language".to_string(), json!("en"));
    user_obj.insert("UserName".to_string(), json!(profile_username.clone()));
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
    obj.insert("username".to_string(), json!(profile_username));
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

// Unit tests verifying that profile utility helpers continue to validate identifiers correctly.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_hex_with_optional_prefix() {
        // Accept hexadecimal identifiers without any prefix as a baseline expectation.
        assert!(is_valid_hex("abcdef"));
        assert!(is_valid_hex("ABCDEF123"));

        // Accept hexadecimal identifiers with the optional 0x/0X prefix so existing configs continue to load.
        assert!(is_valid_hex("0xdeadBEEF"));
        assert!(is_valid_hex("0X1234"));

        // Reject invalid identifiers, including empty strings or values with non-hex characters.
        assert!(!is_valid_hex(""));
        assert!(!is_valid_hex("0x"));
        assert!(!is_valid_hex("0xGHI"));
    }
}
