use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::app::PartyConfig;
use crate::game::Game;
use crate::game::Game::{ExecRef, HandlerRef};
use crate::handler::*;
use crate::input::*;
use crate::instance::*;
use crate::paths::*;
use crate::util::*;

use ctrlc;
use nix::sys::signal::{Signal, kill};
use nix::unistd::Pid;
use std::process::{Child, Command};
use std::time::Duration;

fn prepare_working_tree(
    profname: &str,
    gamedir: &str,
    nemirtingas_rel: &str,
    src: &Path,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let run_fs = PATH_PARTY.join(format!("run/{profname}/fs"));
    if run_fs.exists() {
        std::fs::remove_dir_all(&run_fs)?;
    }
    std::fs::create_dir_all(&run_fs)?;
    let status = std::process::Command::new("cp")
        .arg("-r")
        .arg("-s")
        .arg(format!("{gamedir}/."))
        .arg(run_fs.to_string_lossy().to_string())
        .status()?;
    if !status.success() {
        return Err("cp failed".into());
    }
    if !nemirtingas_rel.is_empty() {
        let dest_dir = run_fs.join(Path::new(nemirtingas_rel).parent().unwrap());
        if dest_dir.exists() {
            if dest_dir.is_file() || dest_dir.is_symlink() {
                std::fs::remove_file(&dest_dir)?;
            } else {
                std::fs::remove_dir_all(&dest_dir)?;
            }
        }
        if let Some(parent) = dest_dir.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::os::unix::fs::symlink(src, &dest_dir)?;
    }
    Ok(run_fs)
}

/// Appends launch diagnostics to a persistent log so users can inspect warnings after the game exits.
fn append_launch_log(level: &str, message: &str) {
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
        .and_then(|mut file| writeln!(file, "[{}] {}", level, message))
    {
        println!(
            "[PARTYDECK][WARN] Failed to persist launch warning log {}: {}",
            log_path.display(),
            err
        );
    }
}

/// Prints and persists a launch warning so it appears both on stdout and in the log file.
fn log_launch_warning(message: &str) {
    println!("[PARTYDECK][WARN] {message}");
    append_launch_log("WARN", message);
}

/// Logs diagnostic information for handlers so users can verify their assets before launch.
fn log_handler_resource_state(handler: &Handler, gamedir: &str) {
    // Report the resolved executable path so the user can confirm the handler layout.
    let exec_path = PathBuf::from(gamedir).join(&handler.exec);
    println!(
        "[PARTYDECK] Handler {} uses executable {}",
        handler.uid,
        exec_path.display()
    );

    if handler.path_nemirtingas.is_empty() {
        return;
    }

    // Expose the resolved Nemirtingas config target to make missing path issues obvious.
    let nemirtingas_target = PathBuf::from(gamedir).join(&handler.path_nemirtingas);
    println!(
        "[PARTYDECK] Handler {} expects Nemirtingas config at {}",
        handler.uid,
        nemirtingas_target.display()
    );

    let parent_rel = Path::new(&handler.path_nemirtingas).parent();
    let Some(parent_rel) = parent_rel else {
        log_launch_warning(&format!(
            "Nemirtingas path for handler {} has no parent directory; check handler JSON.",
            handler.uid
        ));

        return;
    };

    // Validate the directory next to the Nemirtingas config contains patched EOSSDK files.
    let parent_path = PathBuf::from(gamedir).join(parent_rel);
    if !parent_path.exists() {
        log_launch_warning(&format!(
            "Nemirtingas directory {} is missing. Ensure the handler copied patched EOSSDK files there.",
            parent_path.display()
        ));
        return;
    }

    let mut eos_paths = Vec::new();
    if let Ok(entries) = fs::read_dir(&parent_path) {
        for entry in entries.flatten() {
            if entry
                .file_type()
                .map(|file_type| file_type.is_file())
                .unwrap_or(false)
            {
                let name_lower = entry.file_name().to_string_lossy().to_lowercase();
                if name_lower.contains("eossdk") {
                    eos_paths.push(entry.path());
                }
            }
        }
    } else {
        log_launch_warning(&format!(
            "Failed to scan {} for EOSSDK files. Verify directory permissions.",
            parent_path.display()
        ));
        return;
    }

    if eos_paths.is_empty() {
        log_launch_warning(&format!(
            "No EOSSDK files were found next to {}. Nemirtingas may fail to initialize.",
            nemirtingas_target.display()
        ));
    } else {
        // List the discovered EOSSDK assets to help verify the patched binaries are available.
        for path in eos_paths {
            println!(
                "[PARTYDECK] Found EOS-related file for Nemirtingas: {}",
                path.display()
            );
        }
    }
}

pub fn launch_game(
    game: &Game,
    input_devices: &[DeviceInfo],
    instances: &Vec<Instance>,
    cfg: &PartyConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    if let HandlerRef(h) = game {
        for instance in instances {
            create_profile(instance.profname.as_str())?;
            create_gamesave(instance.profname.as_str(), &h)?;
        }
        if h.symlink_dir {
            create_symlink_folder(&h)?;
        }
    }

    let game_id = match game {
        ExecRef(e) => e.filename().to_string(),
        HandlerRef(h) => h.uid.clone(),
    };
    let mut locks_vec = Vec::new();
    for instance in instances {
        let lock = ProfileLock::acquire(&game_id, &instance.profname)?;
        locks_vec.push(lock);
    }
    let locks = Arc::new(Mutex::new(locks_vec));
    let child_pids: Arc<Mutex<Vec<u32>>> = Arc::new(Mutex::new(Vec::new()));
    {
        let child_pids = Arc::clone(&child_pids);
        let locks = Arc::clone(&locks);
        ctrlc::set_handler(move || {
            if let Ok(pids) = child_pids.lock() {
                for pid in pids.iter() {
                    let _ = kill(Pid::from_raw(-(*pid as i32)), Signal::SIGTERM);
                }
            }
            if let Ok(locks) = locks.lock() {
                for lock in locks.iter() {
                    lock.cleanup();
                }
            }
        })?;
    }

    let home = PATH_HOME.display();
    let localshare = PATH_LOCAL_SHARE.display();
    let party = PATH_PARTY.display();
    let steam = PATH_STEAM.display();

    let gamedir = match game {
        ExecRef(e) => e
            .path()
            .parent()
            .ok_or_else(|| "Invalid path")?
            .to_string_lossy()
            .to_string(),
        HandlerRef(h) => match h.symlink_dir {
            true => format!("{party}/gamesyms/{}", h.uid),
            false => get_rootpath_handler(&h)?,
        },
    };

    let win = match game {
        ExecRef(e) => e.path().extension().unwrap_or_default() == "exe",
        HandlerRef(h) => h.win,
    };

    let exec = match game {
        ExecRef(e) => e.filename().to_string(),
        HandlerRef(h) => h.exec.clone(),
    };

    let runtime = if win {
        BIN_UMU_RUN.to_string_lossy().to_string()
    } else if let HandlerRef(h) = game {
        match h.runtime.as_str() {
            "scout" => format!("{steam}/ubuntu12_32/steam-runtime/run.sh"),
            "soldier" => {
                format!("{steam}/steamapps/common/SteamLinuxRuntime_soldier/_v2-entry-point")
            }
            _ => String::new(),
        }
    } else {
        String::new()
    };

    if !PathBuf::from(&gamedir).join(&exec).exists() {
        return Err(format!("Executable not found: {gamedir}/{exec}").into());
    }

    if let HandlerRef(h) = game {
        if h.runtime == "scout" && !PATH_STEAM.join("ubuntu12_32/steam-runtime/run.sh").exists() {
            return Err("Steam Scout Runtime not found".into());
        } else if h.runtime == "soldier"
            && !PATH_STEAM
                .join("steamapps/common/SteamLinuxRuntime_soldier")
                .exists()
        {
            return Err("Steam Soldier Runtime not found".into());
        }

        // Surface handler-specific resource information so users can debug launch issues quickly.
        log_handler_resource_state(h, &gamedir);
    }

    let use_bwrap = Command::new("bwrap").arg("--version").status().is_ok();

    if cfg.enable_kwin_script {
        let script = if instances.len() == 2 && cfg.vertical_two_player {
            "splitscreen_kwin_vertical.js"
        } else {
            "splitscreen_kwin.js"
        };
        kwin_dbus_start_script(PATH_RES.join(script))?;
    }

    let mut children: Vec<Child> = Vec::new();
    for (i, instance) in instances.iter().enumerate() {
        let (nepice_dir, json_path, sha1_nemirtingas) =
            ensure_nemirtingas_config(&instance.profname, &game_id)?;
        let json_real = json_path.canonicalize()?;

        let instance_gamedir = if use_bwrap {
            gamedir.clone()
        } else if let HandlerRef(h) = game {
            prepare_working_tree(
                instance.profname.as_str(),
                &gamedir,
                h.path_nemirtingas.as_str(),
                &nepice_dir,
            )?
            .to_string_lossy()
            .to_string()
        } else {
            gamedir.clone()
        };

        // Track the optional Nemirtingas bind mount as a tuple of source and destination.
        let mut nemirtingas_bind: Option<(PathBuf, PathBuf)> = None;
        if let HandlerRef(h) = game {
            if !h.path_nemirtingas.is_empty() {
                let nemirtingas_rel = Path::new(&h.path_nemirtingas);
                let dest_parent = nemirtingas_rel
                    .parent()
                    .map(|parent| PathBuf::from(&instance_gamedir).join(parent))
                    .unwrap_or_else(|| PathBuf::from(&instance_gamedir));
                if dest_parent.exists() && !dest_parent.is_dir() {
                    std::fs::remove_file(&dest_parent)?;
                }
                std::fs::create_dir_all(&dest_parent)?;
                let dest_path = PathBuf::from(&instance_gamedir).join(nemirtingas_rel);
                if dest_path.exists() && dest_path.is_dir() {
                    std::fs::remove_dir_all(&dest_path)?;
                }
                if !dest_path.exists() {
                    // Ensure the destination file exists so that bubblewrap can bind over it.
                    std::fs::File::create(&dest_path)?;
                }
                println!(
                    "Instance {}: Nemirtingas config {} (SHA1 {}) -> {} (user {} appid {})",
                    instance.profname,
                    json_real.display(),
                    sha1_nemirtingas,
                    dest_path.display(),
                    instance.profname,
                    game_id
                );
                if use_bwrap {
                    // Bind the per-profile JSON directly onto the handler's expected location.
                    nemirtingas_bind = Some((json_path.clone(), dest_path));
                }
            }
        }

        let mut cmd = Command::new(match cfg.kbm_support {
            true => BIN_GSC_KBM.to_string_lossy().to_string(),
            false => "gamescope".to_string(),
        });

        cmd.current_dir(&instance_gamedir);
        cmd.env("SDL_JOYSTICK_HIDAPI", "0");
        cmd.env("ENABLE_GAMESCOPE_WSI", "0");
        cmd.env("PROTON_DISABLE_HIDRAW", "1");
        if cfg.force_sdl && !win {
            let mut path_sdl =
                "ubuntu12_32/steam-runtime/usr/lib/x86_64-linux-gnu/libSDL2-2.0.so.0";
            if let HandlerRef(h) = game {
                if h.is32bit {
                    path_sdl = "ubuntu12_32/steam-runtime/usr/lib/i386-linux-gnu/libSDL2-2.0.so.0";
                }
            }
            cmd.env("SDL_DYNAMIC_API", format!("{steam}/{path_sdl}"));
        }
        if win {
            let protonpath = if cfg.proton_version.is_empty() {
                "GE-Proton".to_string()
            } else {
                cfg.proton_version.clone()
            };
            cmd.env("PROTON_VERB", "run");
            cmd.env("PROTONPATH", protonpath);
            if let HandlerRef(h) = game {
                if !h.dll_overrides.is_empty() {
                    let mut overrides = String::new();
                    for dll in &h.dll_overrides {
                        overrides.push_str(&format!("{dll},"));
                    }
                    overrides.push_str("=n,b");
                    cmd.env("WINEDLLOVERRIDES", overrides);
                }
                if h.coldclient {
                    cmd.env("PROTON_DISABLE_LSTEAMCLIENT", "1");
                }
            }
        }

        let pfx = if win {
            let mut pfx = format!("{party}/pfx/{}", instance.profname);
            if cfg.proton_separate_pfxs {
                pfx = format!("{}_{}", pfx, i + 1);
            }
            pfx
        } else {
            String::new()
        };
        if win {
            std::fs::create_dir_all(&pfx)?;
            cmd.env("WINEPREFIX", &pfx);
            cmd.env("STEAM_COMPAT_DATA_PATH", &pfx);
        }

        cmd.arg("-W").arg(instance.width.to_string());
        cmd.arg("-H").arg(instance.height.to_string());
        if cfg.gamescope_sdl_backend {
            cmd.arg("--backend=sdl");
        }

        if cfg.kbm_support {
            let mut has_keyboard = false;
            let mut has_mouse = false;
            let mut kbms: Vec<String> = Vec::new();
            for d in &instance.devices {
                match input_devices[*d].device_type {
                    DeviceType::Keyboard => {
                        has_keyboard = true;
                        kbms.push(input_devices[*d].path.clone());
                    }
                    DeviceType::Mouse => {
                        has_mouse = true;
                        kbms.push(input_devices[*d].path.clone());
                    }
                    _ => {}
                }
            }
            if has_keyboard {
                cmd.arg("--backend-disable-keyboard");
            }
            if has_mouse {
                cmd.arg("--backend-disable-mouse");
            }
            if !kbms.is_empty() {
                cmd.arg("--libinput-hold-dev");
                cmd.arg(kbms.join(","));
            }
        }

        cmd.arg("--");
        if use_bwrap {
            cmd.arg("bwrap");
            cmd.arg("--die-with-parent");
            cmd.arg("--dev-bind").arg("/").arg("/");
            cmd.arg("--bind").arg("/tmp").arg("/tmp");
            if let Ok(runtime_dir) = std::env::var("XDG_RUNTIME_DIR") {
                cmd.arg("--bind").arg(&runtime_dir).arg(&runtime_dir);
            }

            for (d, dev) in input_devices.iter().enumerate() {
                if !dev.enabled
                    || (!instance.devices.contains(&d) && dev.device_type == DeviceType::Gamepad)
                {
                    cmd.args(["--bind", "/dev/null", dev.path.as_str()]);
                }
            }

            if let HandlerRef(h) = game {
                let path_prof = format!("{party}/profiles/{}", instance.profname);
                let path_save = format!("{path_prof}/saves/{}", h.uid);
                if !h.path_goldberg.is_empty() {
                    let src = format!("{path_prof}/steam");
                    let dst = format!("{instance_gamedir}/{}/goldbergsave", h.path_goldberg);
                    cmd.args(["--bind", src.as_str(), dst.as_str()]);
                }
                if let Some((src, dest)) = &nemirtingas_bind {
                    // Bind the single Nemirtingas JSON file into the game directory.
                    cmd.arg("--bind").arg(src).arg(dest);
                }
                if h.win {
                    let path_windata = format!("{pfx}/drive_c/users/steamuser");
                    if h.win_unique_appdata {
                        let src = format!("{path_save}/_AppData");
                        let dst = format!("{path_windata}/AppData");
                        cmd.args(["--bind", src.as_str(), dst.as_str()]);
                    }
                    if h.win_unique_documents {
                        let src = format!("{path_save}/_Documents");
                        let dst = format!("{path_windata}/Documents");
                        cmd.args(["--bind", src.as_str(), dst.as_str()]);
                    }
                } else {
                    if h.linux_unique_localshare {
                        let src = format!("{path_save}/_share");
                        let dst = format!("{localshare}");
                        cmd.args(["--bind", src.as_str(), dst.as_str()]);
                    }
                    if h.linux_unique_config {
                        let src = format!("{path_save}/_config");
                        let dst = format!("{home}/.config");
                        cmd.args(["--bind", src.as_str(), dst.as_str()]);
                    }
                }
                for subdir in &h.game_unique_paths {
                    let src = format!("{path_save}/{subdir}");
                    let dst = format!("{instance_gamedir}/{subdir}");
                    cmd.args(["--bind", src.as_str(), dst.as_str()]);
                }
            }
        }

        if !runtime.is_empty() {
            cmd.arg(&runtime);
        }

        // Resolve the executable path and canonicalize it for Windows builds so Proton receives
        // the real filesystem target instead of a symlink path that certain games refuse to open.
        let exec_path = PathBuf::from(&instance_gamedir).join(&exec);
        let exec_arg = if win {
            exec_path
                .canonicalize()
                .unwrap_or_else(|_| exec_path.clone())
        } else {
            exec_path.clone()
        };
        cmd.arg(exec_arg.to_string_lossy().to_string());

        let args: Vec<String> = match game {
            HandlerRef(h) => h
                .args
                .iter()
                .map(|arg| match arg.as_str() {
                    "$GAMEDIR" => instance_gamedir.clone(),
                    "$PROFILE" => instance.profname.clone(),
                    "$WIDTH" => instance.width.to_string(),
                    "$HEIGHT" => instance.height.to_string(),
                    "$WIDTHXHEIGHT" => format!("{}x{}", instance.width, instance.height),
                    _ => arg.to_string(),
                })
                .collect(),
            ExecRef(e) => e.args.split_whitespace().map(|s| s.to_string()).collect(),
        };
        for a in args {
            cmd.arg(a);
        }

        let child = cmd.spawn()?;
        child_pids.lock().unwrap().push(child.id());
        children.push(child);

        if i < instances.len() - 1 {
            std::thread::sleep(Duration::from_secs(6));
        }
    }

    for mut child in children {
        let _ = child.wait();
    }
    if let Ok(pids) = child_pids.lock() {
        for pid in pids.iter() {
            let _ = kill(Pid::from_raw(-(*pid as i32)), Signal::SIGTERM);
        }
    }
    locks.lock().unwrap().clear();

    if cfg.enable_kwin_script {
        kwin_dbus_unload_script()?;
    }

    remove_guest_profiles()?;

    Ok(())
}
