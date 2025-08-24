use std::fs::OpenOptions;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use crate::app::PartyConfig;
use crate::game::Game;
use crate::game::Game::{ExecRef, HandlerRef};
use crate::handler::*;
use crate::input::*;
use crate::instance::*;
use crate::paths::*;
use crate::util::*;
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
        let dest = run_fs.join(nemirtingas_rel);
        if dest.exists() {
            std::fs::remove_file(&dest)?;
        }
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(src, &dest)?;
    }
    Ok(run_fs)
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

    let run_dir = PATH_PARTY.join("run");
    std::fs::create_dir_all(&run_dir)?;
    let mut locks = Vec::new();
    for instance in instances {
        let lock_path = run_dir.join(format!("{}.lock", instance.profname));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&lock_path)
        {
            Ok(_) => locks.push(lock_path),
            Err(e) if e.kind() == ErrorKind::AlreadyExists => {
                return Err(format!("Instance {} already running", instance.profname).into());
            }
            Err(e) => return Err(e.into()),
        }
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
        let profile_dir = PATH_PARTY.join(format!("profiles/{}", instance.profname));
        let src_json = profile_dir.join("NemirtingasEpicEmu.json");
        if !src_json.exists() || std::fs::metadata(&src_json)?.len() == 0 {
            create_profile(instance.profname.as_str())?;
        }
        if !src_json.exists() || std::fs::metadata(&src_json)?.len() == 0 {
            eprintln!(
                "Nemirtingas config missing for profile {} at {}",
                instance.profname,
                src_json.display()
            );
            return Err("Nemirtingas config missing".into());
        }
        let sha1_nemirtingas = sha1_file(&src_json)?;

        let instance_gamedir = if use_bwrap {
            gamedir.clone()
        } else if let HandlerRef(h) = game {
            prepare_working_tree(
                instance.profname.as_str(),
                &gamedir,
                h.path_nemirtingas.as_str(),
                &src_json,
            )?
            .to_string_lossy()
            .to_string()
        } else {
            gamedir.clone()
        };

        let mut dest_json_opt = None;
        if use_bwrap {
            if let HandlerRef(h) = game {
                if !h.path_nemirtingas.is_empty() {
                    let dest = PathBuf::from(&instance_gamedir).join(&h.path_nemirtingas);
                    if dest.exists() && dest.is_symlink() {
                        std::fs::remove_file(&dest)?;
                    }
                    if let Some(parent) = dest.parent() {
                        std::fs::create_dir_all(parent)?;
                    }
                    if !dest.exists() {
                        std::fs::File::create(&dest)?;
                    }
                    dest_json_opt = Some(dest);
                }
            }
        } else if let HandlerRef(h) = game {
            if !h.path_nemirtingas.is_empty() {
                let dest = PathBuf::from(&instance_gamedir).join(&h.path_nemirtingas);
                println!(
                    "Instance {}: Nemirtingas config {} (SHA1 {}) -> {}",
                    instance.profname,
                    src_json.canonicalize()?.display(),
                    sha1_nemirtingas,
                    dest.canonicalize()?.display()
                );
            }
        }

        if let Some(dest) = &dest_json_opt {
            println!(
                "Instance {}: Nemirtingas config {} (SHA1 {}) -> {}",
                instance.profname,
                src_json.canonicalize()?.display(),
                sha1_nemirtingas,
                dest.canonicalize()?.display()
            );
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
            if cfg.proton_separate_pfxs {
                format!("{party}/pfx{}", i + 1)
            } else {
                format!("{party}/pfx")
            }
        } else {
            String::new()
        };
        if win {
            cmd.env("WINEPREFIX", &pfx);
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
            cmd.arg("--tmpfs").arg("/tmp");

            for (d, dev) in input_devices.iter().enumerate() {
                if !dev.enabled
                    || (!instance.devices.contains(&d) && dev.device_type == DeviceType::Gamepad)
                {
                    cmd.arg("--bind").arg("/dev/null").arg(&dev.path);
                }
            }

            if let HandlerRef(h) = game {
                let path_prof = format!("{party}/profiles/{}", instance.profname);
                let path_save = format!("{path_prof}/saves/{}", h.uid);
                if !h.path_goldberg.is_empty() {
                    cmd.arg("--bind")
                        .arg(format!("{path_prof}/steam"))
                        .arg(format!(
                            "{instance_gamedir}/{}/goldbergsave",
                            h.path_goldberg
                        ));
                }
                if let Some(dest) = &dest_json_opt {
                    cmd.arg("--bind");
                    cmd.arg(&src_json);
                    cmd.arg(dest);
                }
                if h.win {
                    let path_windata = format!("{pfx}/drive_c/users/steamuser");
                    if h.win_unique_appdata {
                        cmd.arg("--bind")
                            .arg(format!("{path_save}/_AppData"))
                            .arg(format!("{path_windata}/AppData"));
                    }
                    if h.win_unique_documents {
                        cmd.arg("--bind")
                            .arg(format!("{path_save}/_Documents"))
                            .arg(format!("{path_windata}/Documents"));
                    }
                } else {
                    if h.linux_unique_localshare {
                        cmd.arg("--bind")
                            .arg(format!("{path_save}/_share"))
                            .arg(format!("{localshare}"));
                    }
                    if h.linux_unique_config {
                        cmd.arg("--bind")
                            .arg(format!("{path_save}/_config"))
                            .arg(format!("{home}/.config"));
                    }
                }
                for subdir in &h.game_unique_paths {
                    cmd.arg("--bind")
                        .arg(format!("{path_save}/{subdir}"))
                        .arg(format!("{instance_gamedir}/{subdir}"));
                }
            }
        }

        if !runtime.is_empty() {
            cmd.arg(&runtime);
        }

        let exec_path = format!("{instance_gamedir}/{exec}");
        cmd.arg(exec_path);

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
        children.push(child);

        if i < instances.len() - 1 {
            if win {
                std::thread::sleep(Duration::from_secs(6));
            } else {
                std::thread::sleep(Duration::from_millis(10));
            }
        }
    }

    for mut child in children {
        let _ = child.wait();
    }

    for lock in locks {
        let _ = std::fs::remove_file(lock);
    }

    if cfg.enable_kwin_script {
        kwin_dbus_unload_script()?;
    }

    remove_guest_profiles()?;

    Ok(())
}
