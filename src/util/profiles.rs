use rand::prelude::*;
use serde_json::{Map, Value, json};
use sha1::{Digest, Sha1};
use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

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

/// Normalizes optional Nemirtingas identifiers by trimming whitespace and removing the
/// optional `0x`/`0X` prefix. Returns `None` when the payload still contains invalid
/// characters after normalization.
fn normalize_hex(value: &str) -> Option<String> {
    let trimmed = value.trim();
    let normalized = trimmed.trim_start_matches("0x").trim_start_matches("0X");

    if normalized.is_empty() {
        return None;
    }

    if normalized.chars().all(|c| c.is_ascii_hexdigit()) {
        Some(normalized.to_string())
    } else {
        None
    }
}

/// Logs a warning both to stdout and the persistent launch warning log so profile repairs are
/// visible even after the session ends.
fn log_profile_warning(message: &str) {
    println!("[PARTYDECK][WARN] {message}");

    let log_dir = PATH_PARTY.join("logs");
    if let Err(err) = fs::create_dir_all(&log_dir) {
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
        fs::create_dir_all(&path_steam)?;

        let steam_id = format!("{:017}", rand::rng().random_range(u32::MIN..u32::MAX));
        let usersettings = format!(
            "[user::general]\naccount_name={name}\naccount_steamid={steam_id}\nlanguage=english\nip_country=US"
        );
        fs::write(path_steam.join("configs.user.ini"), usersettings)?;

        println!("Created successfully");
    }

    fs::create_dir_all(profile_dir.join("nepice_settings"))?;

    Ok(())
}

/// Writes a Goldberg configuration helper file only when the trimmed contents differ so
/// we avoid spamming disk writes every launch while still guaranteeing consistent values.
fn write_setting_if_changed(path: &Path, value: &str) -> io::Result<()> {
    if let Ok(existing) = fs::read_to_string(path) {
        if value.is_empty() {
            if existing.is_empty() {
                return Ok(());
            }
        } else if existing.trim() == value {
            return Ok(());
        }
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    // Maintain genuinely empty files (for helpers like auto_accept_invite.txt) when the
    // desired value is blank; otherwise mirror Goldberg's newline-terminated format.
    let contents = if value.is_empty() {
        String::new()
    } else {
        format!("{value}\n")
    };

    fs::write(path, contents)
}

/// Ensures a Goldberg INI file exposes a specific `key=value` pair inside the target
/// section without disturbing the rest of the configuration. The helper either updates
/// an existing entry or appends it at the end of the section if missing, creating the
/// section on demand when necessary.
fn ensure_ini_setting(path: &Path, section: &str, key: &str, value: &str) -> io::Result<()> {
    let desired_section = section;
    let desired_key = format!("{key}={value}");
    let key_prefix = format!("{key}=");
    let existing_contents = fs::read_to_string(path).ok();

    let mut lines: Vec<String> = existing_contents
        .as_deref()
        .map(|contents| contents.lines().map(|line| line.to_string()).collect())
        .unwrap_or_else(Vec::new);

    let mut section_found = false;
    let mut key_updated = false;
    let mut in_desired_section = false;

    for line in lines.iter_mut() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            in_desired_section = trimmed == desired_section;
            if in_desired_section {
                section_found = true;
            }
            continue;
        }

        if in_desired_section && trimmed.starts_with(&key_prefix) {
            if trimmed != desired_key {
                *line = desired_key.clone();
            }
            key_updated = true;
            break;
        }
    }

    if !section_found {
        if !lines.is_empty() && !lines.last().unwrap().is_empty() {
            lines.push(String::new());
        }
        lines.push(desired_section.to_string());
        lines.push(desired_key.clone());
    } else if !key_updated {
        let mut insert_index = lines.len();
        let mut current_section = None;
        for (idx, line) in lines.iter().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with('[') && trimmed.ends_with(']') {
                current_section = Some(trimmed.to_string());
                if trimmed == desired_section {
                    insert_index = idx + 1;
                } else if insert_index != lines.len() {
                    insert_index = idx;
                    break;
                }
            } else if insert_index != lines.len()
                && current_section.as_deref() == Some(desired_section)
            {
                insert_index = idx + 1;
            }
        }
        if insert_index > lines.len() {
            insert_index = lines.len();
        }
        lines.insert(insert_index, desired_key.clone());
    }

    if !lines.is_empty() && lines.last().map(|line| !line.is_empty()).unwrap_or(false) {
        lines.push(String::new());
    }

    let mut new_contents = lines.join("\n");
    if !new_contents.ends_with('\n') {
        new_contents.push('\n');
    }

    if existing_contents.as_deref() == Some(new_contents.as_str()) {
        return Ok(());
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    fs::write(path, new_contents)
}

/// Convenience wrapper that keeps the public API focused on the listen port while the
/// underlying helper remains flexible for other Goldberg flags we might need in future
/// updates.
fn ensure_ini_listen_port(path: &Path, port: u16) -> io::Result<()> {
    ensure_ini_setting(
        path,
        "[main::connectivity]",
        "listen_port",
        &port.to_string(),
    )
}

/// Extracts a `key=value` pair from Goldberg's `configs.user.ini`, returning `None` when
/// the file cannot be read or the key was absent.
fn read_config_value(config_path: &Path, key: &str) -> Option<String> {
    let Ok(contents) = fs::read_to_string(config_path) else {
        return None;
    };

    contents
        .lines()
        .find_map(|line| line.strip_prefix(&format!("{key}=")))
        .map(|value| value.trim().to_string())
}

/// Computes a deterministic Goldberg listen port derived from the game identifier so all
/// instances share a stable LAN discovery socket without clashing across different games.
fn deterministic_goldberg_port(game_id: &str) -> u16 {
    let mut hasher = Sha1::new();
    hasher.update(format!("partydeck-goldberg-port:{game_id}").as_bytes());
    let digest = hasher.finalize();

    let raw = u16::from_be_bytes([digest[0], digest[1]]);
    20000 + (raw % 20000)
}

/// Computes a deterministic Nemirtingas LAN port based on the game, profile, and attempt
/// counter so each player receives a stable yet unique UDP socket when multiple instances
/// run on the same device.
fn deterministic_nemirtingas_port(game_id: &str, profile: &str, attempt: u32) -> u16 {
    let mut hasher = Sha1::new();
    hasher.update(format!("partydeck-nemirtingas-port:{game_id}:{profile}:{attempt}").as_bytes());
    let digest = hasher.finalize();

    let raw = u16::from_be_bytes([digest[2], digest[3]]);
    40000 + (raw % 20000)
}

/// Resolves stable Nemirtingas LAN ports for every provided profile while avoiding
/// collisions with the Goldberg discovery socket and between different PartyDeck players.
/// Each assigned port stays deterministic across launches so join codes remain valid.
pub fn resolve_nemirtingas_ports(
    profiles: &[String],
    game_id: &str,
    goldberg_port: Option<u16>,
) -> HashMap<String, u16> {
    let mut assignments = HashMap::new();
    let mut used_ports: HashSet<u16> = HashSet::new();

    if let Some(port) = goldberg_port {
        used_ports.insert(port);
    }

    let mut sorted_profiles: Vec<String> = profiles.to_vec();
    sorted_profiles.sort();

    for profile in sorted_profiles {
        let mut attempt: u32 = 0;
        loop {
            let port = deterministic_nemirtingas_port(game_id, &profile, attempt);

            if used_ports.contains(&port) {
                attempt = attempt.saturating_add(1);
                continue;
            }

            used_ports.insert(port);
            assignments.insert(profile.clone(), port);
            break;
        }
    }

    assignments
}

/// Ensures all active profiles expose the Goldberg LAN identity files expected by Coral
/// Island (account name, SteamID, language, invite toggles) and normalizes the shared
/// `listen_port.txt` so every instance binds the same UDP socket during discovery.
pub fn synchronize_goldberg_profiles(
    profiles: &[String],
    game_id: &str,
    port_override: Option<u16>,
) -> Result<Option<u16>, Box<dyn Error>> {
    if profiles.is_empty() {
        return Ok(None);
    }

    // Resolve the Goldberg listen port shared across every profile. Handlers that bundle
    // Nemirtingas request the fixed LAN port so EOS beacons and Goldberg discovery stay on
    // the same socket, while other titles fall back to a deterministic hash of the game ID
    // so multiple games do not collide yet every instance of the same game advertises the
    // identical UDP endpoint.
    let port = port_override.unwrap_or_else(|| deterministic_goldberg_port(game_id));
    let port_source = if port_override.is_some() {
        "handler override"
    } else {
        "deterministic default"
    };
    let mut seen_profiles: HashSet<String> = HashSet::new();

    for name in profiles {
        if !seen_profiles.insert(name.clone()) {
            continue;
        }

        let profile_dir = PATH_PARTY.join(format!("profiles/{name}"));
        fs::create_dir_all(&profile_dir)?;

        let steam_settings = profile_dir.join("steam/settings");
        fs::create_dir_all(&steam_settings)?;

        let config_path = steam_settings.join("configs.user.ini");

        // Prefer existing Goldberg overrides before falling back to deterministic defaults
        // so custom LAN identities remain intact between launches.
        let account_name = fs::read_to_string(steam_settings.join("account_name.txt"))
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .or_else(|| read_config_value(&config_path, "account_name"))
            .unwrap_or_else(|| name.clone());

        let user_steam_id = fs::read_to_string(steam_settings.join("user_steam_id.txt"))
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .or_else(|| read_config_value(&config_path, "account_steamid"))
            .unwrap_or_else(|| {
                let mut hasher = Sha1::new();
                hasher.update(format!("partydeck-goldberg-steamid:{name}").as_bytes());
                let digest = hasher.finalize();
                let mut value = u128::from_be_bytes([
                    digest[0], digest[1], digest[2], digest[3], digest[4], digest[5], digest[6],
                    digest[7], digest[8], digest[9], digest[10], digest[11], digest[12],
                    digest[13], digest[14], digest[15],
                ])
                .to_string();
                if value.len() < 17 {
                    value = format!("{value:0>17}");
                }
                value.truncate(17);
                value
            });

        // Persist the individual identity files so Goldberg can resolve the LAN persona.
        write_setting_if_changed(&steam_settings.join("account_name.txt"), &account_name)?;
        write_setting_if_changed(&steam_settings.join("user_steam_id.txt"), &user_steam_id)?;
        write_setting_if_changed(&steam_settings.join("language.txt"), "english")?;

        // Toggle LAN discovery helpers to avoid requiring the Steam overlay for invites.
        write_setting_if_changed(&steam_settings.join("auto_accept_invite.txt"), "")?;
        write_setting_if_changed(&steam_settings.join("disable_lan_only.txt"), "1")?;
        write_setting_if_changed(&steam_settings.join("gc_token.txt"), "1")?;
        write_setting_if_changed(&steam_settings.join("new_app_ticket.txt"), "1")?;

        // Ensure the Goldberg INI toggles mirror the one-shot helpers so the emulator's
        // internal logic also generates the new auth ticket and GC token expected by the
        // experimental build now bundled with PartyDeck.
        ensure_ini_setting(
            &steam_settings.join("configs.main.ini"),
            "[main::general]",
            "new_app_ticket",
            "1",
        )?;
        ensure_ini_setting(
            &steam_settings.join("configs.main.ini"),
            "[main::general]",
            "gc_token",
            "1",
        )?;

        ensure_ini_setting(
            &steam_settings.join("configs.user.ini"),
            "[main::general]",
            "new_app_ticket",
            "1",
        )?;
        ensure_ini_setting(
            &steam_settings.join("configs.user.ini"),
            "[main::general]",
            "gc_token",
            "1",
        )?;

        // Synchronize the listen port across every profile so Goldberg advertises/joins
        // lobbies via the same UDP endpoint. Persist the port in both helper text files and
        // the INI toggles so legacy builds that only inspect one location remain in sync and
        // Nemirtingas can mirror the same socket when generating its JSON later.
        write_setting_if_changed(&steam_settings.join("listen_port.txt"), &port.to_string())?;
        ensure_ini_listen_port(&steam_settings.join("configs.main.ini"), port)?;
        ensure_ini_listen_port(&steam_settings.join("configs.user.ini"), port)?;

        println!(
            "[PARTYDECK] Goldberg LAN identity for profile {} set to {} / {} on port {} ({})",
            name, account_name, user_steam_id, port, port_source
        );
    }

    // Expose the synchronized port so launch routines can mirror it into Nemirtingas configs
    // and environment variables whenever required.
    Ok(Some(port))
}

pub fn ensure_nemirtingas_config(
    name: &str,
    appid: &str,
    lan_port: Option<u16>,
) -> Result<(PathBuf, PathBuf, PathBuf, String), Box<dyn Error>> {
    let profile_dir = PATH_PARTY.join(format!("profiles/{name}"));
    fs::create_dir_all(&profile_dir)?;
    create_profile(name)?;

    let nepice_dir = profile_dir.join("nepice_settings");
    fs::create_dir_all(&nepice_dir)?;
    let path = nepice_dir.join("NemirtingasEpicEmu.json");

    // Track whether a Nemirtingas config already existed so we can surface missing-ID repairs
    // to the persistent launch log instead of silently regenerating values.
    let had_existing_config = path.exists();

    let mut existing_epicid = None;
    let mut existing_productuserid = None;
    let mut existing_accountid_raw = None;

    let mut existing_username = None;
    if let Ok(file) = fs::File::open(&path) {
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
            existing_accountid_raw = value
                .pointer("/EOSEmu/User/AccountId")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .or_else(|| {
                    value
                        .get("accountid")
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
        if let Some(clean) = normalize_hex(&id) {
            Some(clean)
        } else {
            log_profile_warning(&format!(
                "Profile {name} contained invalid Nemirtingas EpicId {id}; regenerating."
            ));
            None
        }
    });

    let existing_productuserid = existing_productuserid.and_then(|id| {
        if let Some(clean) = normalize_hex(&id) {
            Some(clean)
        } else {
            log_profile_warning(&format!(
                "Profile {name} contained invalid Nemirtingas ProductUserId {id}; regenerating."
            ));
            None
        }
    });

    let existing_accountid = existing_accountid_raw.clone().and_then(|id| {
        if let Some(clean) = normalize_hex(&id) {
            Some(clean)
        } else {
            log_profile_warning(&format!(
                "Profile {name} contained invalid Nemirtingas AccountId {id}; regenerating."
            ));
            None
        }
    });

    if existing_accountid.is_none() && had_existing_config && existing_accountid_raw.is_none() {
        log_profile_warning(&format!(
            "Profile {name} was missing a Nemirtingas AccountId; generating a new value."
        ));
    }

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

    // Ensure the Nemirtingas AccountId exists alongside EpicId/ProductUserId so the emulator
    // can satisfy EOS auth requests without falling back to 0x0 placeholders.
    let account_id = existing_accountid.unwrap_or_else(|| {
        let new_id = if uses_default_username {
            generate_hex_id(32)
        } else {
            let seed = format!("account:{profile_username}");
            deterministic_hex_from_seed(&seed, 32)
        };
        println!(
            "[PARTYDECK] Generated Nemirtingas AccountId {} for profile {} using {} mode",
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

    // Build the Nemirtingas configuration with the expected nested layout.
    let mut user_obj = Map::new();
    user_obj.insert("Language".to_string(), json!("en"));
    user_obj.insert("UserName".to_string(), json!(profile_username.clone()));
    user_obj.insert("EpicId".to_string(), json!(epic_id.clone()));
    user_obj.insert("ProductUserId".to_string(), json!(product_user_id.clone()));
    user_obj.insert("AccountId".to_string(), json!(account_id.clone()));

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
    // other players on the local network to discover the host via invite codes. When a
    // synchronized Goldberg listen port is available, also override the LAN beacon to the
    // same UDP socket so EOS discovery and Goldberg stay aligned.
    let mut network_plugins = Map::new();
    network_plugins.insert(
        "Broadcast".to_string(),
        json!({
            "EnableLog": false,
            "Enabled": true,
            "LocalhostOnly": false
        }),
    );
    network_plugins.insert(
        "WebSocket".to_string(),
        json!({
            "EnableLog": false,
            "Enabled": false,
            "SignalingServers": []
        }),
    );

    let mut network_obj = Map::new();
    network_obj.insert("IceServers".to_string(), json!([]));
    network_obj.insert("Plugins".to_string(), Value::Object(network_plugins));
    if let Some(port) = lan_port {
        network_obj.insert(
            "Lan".to_string(),
            json!({
                "Enabled": true,
                "OverridePort": port
            }),
        );
    }
    obj.insert("Network".to_string(), Value::Object(network_obj));
    obj.insert("appid".to_string(), json!(appid));
    obj.insert("language".to_string(), json!("en"));
    // Mirror the nested debug verbosity in the legacy flat configuration for tools still reading the flat keys.
    obj.insert("log_level".to_string(), json!("DEBUG"));
    obj.insert("username".to_string(), json!(profile_username));
    // Surface the generated IDs in the flat layout as well so legacy Nemirtingas builds read them consistently.
    obj.insert("epicid".to_string(), json!(epic_id));
    obj.insert("productuserid".to_string(), json!(product_user_id));
    obj.insert("accountid".to_string(), json!(account_id));
    if let Some(port) = lan_port {
        // Surface the synchronized LAN port in the flat schema so older Nemirtingas builds
        // that only inspect top-level keys can still reuse Goldberg's UDP socket.
        obj.insert("lan_port".to_string(), json!(port));
    }

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

    // Guarantee each profile exposes a dedicated Nemirtingas AppData root so concurrent
    // instances cannot clobber shared EOS state while the emulator resolves LAN lobbies.
    let appdata_dir = nepice_dir.join("appdata");
    fs::create_dir_all(&appdata_dir)?;

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

// Unit tests to guarantee that Nemirtingas identifier parsing continues accepting both
// raw hexadecimal IDs and variants prefixed with `0x`.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_hex_accepts_plain_hex() {
        assert_eq!(normalize_hex("deadbeef"), Some("deadbeef".to_string()));
    }

    #[test]
    fn normalize_hex_accepts_prefixed_hex() {
        assert_eq!(normalize_hex("0xABC123"), Some("ABC123".to_string()));
        assert_eq!(normalize_hex("0Xff"), Some("ff".to_string()));
    }

    #[test]
    fn normalize_hex_rejects_invalid_values() {
        assert_eq!(normalize_hex(""), None);
        assert_eq!(normalize_hex("0xg"), None);
    }
}
