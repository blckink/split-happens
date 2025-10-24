use std::collections::{HashMap, HashSet};
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use crate::app::PartyConfig;
use crate::game::Game;
use crate::game::Game::{ExecRef, HandlerRef};
use crate::handler::*;
use crate::input::*;
use crate::instance::*;
use crate::paths::*;
use crate::util::*;

use ctrlc;
use nix::libc;
use nix::sched::{CpuSet, sched_setaffinity};
use nix::sys::signal::{Signal, kill};
use nix::unistd::Pid;
use std::process::{Child, Command, Stdio};
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

/// Tracks Nemirtingas logging metadata for an instance so we can surface the
/// persisted emulator output once the Proton processes terminate.
#[derive(Clone)]
struct NemirtingasLogContext {
    profile_log: PathBuf,
    appdata_root: Option<PathBuf>,
}

/// Scans the Proton AppData roots for Nemirtingas log files and copies their
/// contents into the PartyDeck profile log so the advertised path always
/// contains the most recent emulator errors for the user.
fn collect_nemirtingas_logs(contexts: &[NemirtingasLogContext]) {
    for context in contexts {
        let mut sources: Vec<PathBuf> = Vec::new();

        if let Some(appdata_root) = &context.appdata_root {
            let mut search_roots = vec![appdata_root.clone()];
            if let Some(local_root) = appdata_root
                .parent()
                .and_then(|roaming| roaming.parent())
                .map(|appdata| appdata.join("Local").join("NemirtingasEpicEmu"))
            {
                search_roots.push(local_root);
            }

            let mut stack = search_roots;
            while let Some(path) = stack.pop() {
                if !path.exists() {
                    continue;
                }
                if path.is_dir() {
                    match fs::read_dir(&path) {
                        Ok(entries) => {
                            for entry in entries.flatten() {
                                let child = entry.path();
                                if child.is_dir() {
                                    stack.push(child);
                                    continue;
                                }
                                if let Some(name) = child.file_name().and_then(|n| n.to_str()) {
                                    let lower = name.to_ascii_lowercase();
                                    let is_log = lower.ends_with(".log") || lower.ends_with(".txt");
                                    let matches_prefix =
                                        lower.contains("nemirtingas") || lower.contains("applog");
                                    if is_log && matches_prefix {
                                        sources.push(child);
                                    }
                                }
                            }
                        }
                        Err(err) => {
                            println!(
                                "[PARTYDECK][WARN] Failed to enumerate Nemirtingas logs under {}: {}",
                                path.display(),
                                err
                            );
                        }
                    }
                }
            }
        }

        sources.sort();
        sources.dedup();

        let mut aggregated: Vec<u8> = Vec::new();
        for source in sources {
            match fs::read(&source) {
                Ok(data) => {
                    let header = format!("===== {} =====\n", source.display());
                    aggregated.extend_from_slice(header.as_bytes());
                    aggregated.extend_from_slice(&data);
                    if !data.ends_with(b"\n") {
                        aggregated.push(b'\n');
                    }
                    aggregated.push(b'\n');
                }
                Err(err) => {
                    println!(
                        "[PARTYDECK][WARN] Failed to read Nemirtingas log {}: {}",
                        source.display(),
                        err
                    );
                }
            }
        }

        if aggregated.is_empty() {
            continue;
        }

        if let Some(parent) = context.profile_log.parent() {
            if let Err(err) = fs::create_dir_all(parent) {
                println!(
                    "[PARTYDECK][WARN] Failed to prepare Nemirtingas log directory {}: {}",
                    parent.display(),
                    err
                );
                continue;
            }
        }

        match OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&context.profile_log)
        {
            Ok(mut dest) => {
                if let Err(err) = dest.write_all(&aggregated) {
                    println!(
                        "[PARTYDECK][WARN] Failed to persist Nemirtingas log {}: {}",
                        context.profile_log.display(),
                        err
                    );
                }
            }
            Err(err) => {
                println!(
                    "[PARTYDECK][WARN] Failed to open Nemirtingas log {}: {}",
                    context.profile_log.display(),
                    err
                );
            }
        }
    }
}

/// Captures the reusable artifacts from launching a single instance so crashes can be
/// recovered without rebuilding the entire session state.
struct SpawnOutcome {
    child: Child,
    log_context: NemirtingasLogContext,
    proton_prefix: Option<String>,
}

/// Spawns a single Gamescope instance for the provided player slot while preparing all
/// emulator mounts and controller bindings required by the handler. The returned
/// [`SpawnOutcome`] keeps enough context for the caller to re-launch the same slot later
/// when a crash occurs.
fn spawn_instance_child(
    index: usize,
    instance: &Instance,
    game: &Game,
    game_id: &str,
    gamedir: &str,
    exec: &str,
    runtime: &str,
    win: bool,
    use_bwrap: bool,
    cfg: &PartyConfig,
    input_devices: &[DeviceInfo],
    proton_env: Option<&ProtonEnvironment>,
    nemirtingas_ports: &HashMap<String, u16>,
    drained_prefixes: &mut HashSet<String>,
    party: &str,
    steam: &str,
    home: &str,
    localshare: &str,
) -> Result<SpawnOutcome, Box<dyn std::error::Error>> {
    let profile_port = nemirtingas_ports.get(&instance.profname).copied();

    let (nepice_dir, json_path, log_path, sha1_nemirtingas) =
        ensure_nemirtingas_config(&instance.profname, game_id, profile_port)?;
    let json_real = json_path.canonicalize()?;
    let mut log_context = NemirtingasLogContext {
        profile_log: log_path.clone(),
        appdata_root: None,
    };

    reset_nemirtingas_session_state(&nepice_dir);

    let instance_gamedir = if use_bwrap {
        gamedir.to_string()
    } else if let HandlerRef(h) = game {
        prepare_working_tree(
            instance.profname.as_str(),
            gamedir,
            h.path_nemirtingas.as_str(),
            &nepice_dir,
        )?
        .to_string_lossy()
        .to_string()
    } else {
        gamedir.to_string()
    };

    let mut nemirtingas_binds: Vec<(PathBuf, PathBuf)> = Vec::new();
    if let HandlerRef(h) = game {
        if !h.path_nemirtingas.is_empty() {
            let nemirtingas_rel = Path::new(&h.path_nemirtingas);
            let Some(parent_rel) = nemirtingas_rel.parent() else {
                return Err(format!(
                    "Nemirtingas path {} has no parent directory; update the handler configuration.",
                    h.path_nemirtingas
                )
                .into());
            };

            let dest_dir = PathBuf::from(&instance_gamedir).join(parent_rel);
            if dest_dir.exists() && !dest_dir.is_dir() {
                fs::remove_file(&dest_dir)?;
            }
            fs::create_dir_all(&dest_dir)?;

            let dest_config = dest_dir.join(
                nemirtingas_rel
                    .file_name()
                    .unwrap_or_else(|| std::ffi::OsStr::new("NemirtingasEpicEmu.json")),
            );

            println!(
                "Instance {}: Nemirtingas config {} (SHA1 {}) -> {} (user {} appid {})",
                instance.profname,
                json_real.display(),
                sha1_nemirtingas,
                dest_config.display(),
                instance.profname,
                game_id
            );

            if use_bwrap {
                nemirtingas_binds.push((nepice_dir.clone(), dest_dir.clone()));
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
        let mut path_sdl = "ubuntu12_32/steam-runtime/usr/lib/x86_64-linux-gnu/libSDL2-2.0.so.0";
        if let HandlerRef(h) = game {
            if h.is32bit {
                path_sdl = "ubuntu12_32/steam-runtime/usr/lib/i386-linux-gnu/libSDL2-2.0.so.0";
            }
        }
        cmd.env("SDL_DYNAMIC_API", format!("{steam}/{path_sdl}"));
    }
    if let Some(port) = profile_port {
        cmd.env("EOS_OVERRIDE_LAN_PORT", port.to_string());
    }
    if win {
        if let Some(env) = proton_env {
            cmd.env("PROTON_VERB", "run");
            cmd.env("PROTONPATH", env.env_value.clone());
        }
        if cfg.performance_enable_proton_fsr {
            // Enable Proton's built-in FSR scaling so Windows games can render below native resolution without severe blur.
            cmd.env("WINE_FULLSCREEN_FSR", "1");
            cmd.env("WINE_FULLSCREEN_FSR_MODE", "1");
            cmd.env("WINE_FULLSCREEN_FSR_STRENGTH", "2");
        }
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

    let mut proton_prefix: Option<String> = None;
    if win {
        let mut pfx = format!("{party}/pfx/{}", instance.profname);
        if cfg.proton_separate_pfxs {
            pfx = format!("{}_{}", pfx, index + 1);
        }
        std::fs::create_dir_all(&pfx)?;
        cmd.env("WINEPREFIX", &pfx);
        cmd.env("STEAM_COMPAT_DATA_PATH", &pfx);
        if let Some(env) = proton_env {
            if env.root_path.is_some() && drained_prefixes.insert(pfx.clone()) {
                drain_stale_proton_session(&pfx, env);
            }
        }
        log_context.appdata_root = Some(
            PathBuf::from(&pfx)
                .join("drive_c")
                .join("users")
                .join("steamuser")
                .join("AppData")
                .join("Roaming")
                .join("NemirtingasEpicEmu"),
        );
        proton_prefix = Some(pfx);
    }

    cmd.arg("-W").arg(instance.width.to_string());
    cmd.arg("-H").arg(instance.height.to_string());
    if cfg.gamescope_sdl_backend {
        cmd.arg("--backend=sdl");
    }

    if cfg.performance_gamescope_rt {
        // Promote gamescope to its real-time scheduling mode to smooth frame pacing on the Deck.
        cmd.arg("--rt");
    }
    if cfg.performance_limit_40fps {
        // Clamp both active and unfocused windows to 40 FPS to keep dual sessions within the Deck's power budget.
        cmd.arg("--fps-limit=40");
        cmd.arg("--secondary-no-focus-fps-limit=40");
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
            for (src, dest) in &nemirtingas_binds {
                cmd.arg("--bind").arg(src).arg(dest);
            }
            if h.win {
                let Some(prefix_value) = &proton_prefix else {
                    return Err("Missing Proton prefix for Windows handler".into());
                };
                let path_windata = format!("{prefix_value}/drive_c/users/steamuser");
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
                    cmd.args(["--bind", src.as_str(), localshare]);
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
        cmd.arg(runtime);
    }

    let exec_path = PathBuf::from(&instance_gamedir).join(exec);
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

    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let child = cmd.spawn()?;

    Ok(SpawnOutcome {
        child,
        log_context,
        proton_prefix,
    })
}

/// Tracks the runtime state of a launched instance so crashes can trigger targeted
/// restarts without disturbing other players.
struct RuntimeInstance {
    index: usize,
    profile_name: String,
    instance: Instance,
    child: Option<Child>,
    last_pid: Option<u32>,
    log_context: NemirtingasLogContext,
    proton_prefix: Option<String>,
    finished: bool,
}

/// Removes a PID from the shared cleanup list once the corresponding process exits so the
/// Ctrl+C handler stops signalling stale process groups.
fn unregister_child_pid(child_pids: &Arc<Mutex<Vec<u32>>>, pid: u32) {
    if let Ok(mut pids) = child_pids.lock() {
        if let Some(pos) = pids.iter().position(|existing| *existing == pid) {
            pids.swap_remove(pos);
        }
    }
}

/// Raises the niceness of a spawned instance slightly so CPU scheduling stays balanced when
/// multiple Gamescope sessions render simultaneously.
fn promote_instance_priority(pid: u32, index: usize, total_instances: usize) {
    let result = unsafe { libc::setpriority(libc::PRIO_PROCESS, pid as libc::id_t, -5) };
    if result == 0 {
        println!(
            "[PARTYDECK] Elevated scheduling priority for instance {}/{} (PID {}).",
            index + 1,
            total_instances,
            pid
        );
    } else {
        let err = std::io::Error::last_os_error();
        println!(
            "[PARTYDECK][WARN] Unable to boost priority for instance {} (PID {}): {}",
            index + 1,
            pid,
            err
        );
    }
}

/// Tracks the shared cleanup handles referenced by the global Ctrl+C handler so
/// subsequent launches can reuse the same signal hook without tripping the
/// multiple-handler guard in the ctrlc crate.
struct CtrlcCleanup {
    child_pids: Arc<Mutex<Vec<u32>>>,
    locks: Arc<Mutex<Vec<ProfileLock>>>,
}

static CTRL_C_STATE: OnceLock<Mutex<Option<CtrlcCleanup>>> = OnceLock::new();
static CTRL_C_HANDLER: OnceLock<()> = OnceLock::new();

/// Installs (or refreshes) the Ctrl+C cleanup handler so repeated launches keep
/// terminating Gamescope descendants and releasing profile locks without
/// requiring the application to restart between sessions.
fn register_ctrlc_cleanup(
    child_pids: Arc<Mutex<Vec<u32>>>,
    locks: Arc<Mutex<Vec<ProfileLock>>>,
) -> Result<(), ctrlc::Error> {
    let state = CTRL_C_STATE.get_or_init(|| Mutex::new(None));
    {
        let mut guard = state.lock().unwrap();
        *guard = Some(CtrlcCleanup {
            child_pids: Arc::clone(&child_pids),
            locks: Arc::clone(&locks),
        });
    }

    if CTRL_C_HANDLER.get().is_none() {
        let state_ref = CTRL_C_STATE
            .get()
            .expect("Ctrl+C state should be initialized before handler registration");
        ctrlc::set_handler(move || {
            if let Ok(mut guard) = state_ref.lock() {
                if let Some(shared) = guard.as_mut() {
                    if let Ok(pids) = shared.child_pids.lock() {
                        for pid in pids.iter() {
                            let _ = kill(Pid::from_raw(-(*pid as i32)), Signal::SIGTERM);
                        }
                    }
                    if let Ok(mut locks_guard) = shared.locks.lock() {
                        for lock in locks_guard.iter() {
                            lock.cleanup();
                        }
                        locks_guard.clear();
                    }
                }
            }
        })?;
        let _ = CTRL_C_HANDLER.set(());
    }

    Ok(())
}

/// Clears the shared Ctrl+C cleanup state after a launch finishes so the next
/// session starts from a clean slate while reusing the original handler.
fn clear_ctrlc_cleanup() {
    if let Some(state) = CTRL_C_STATE.get() {
        if let Ok(mut guard) = state.lock() {
            *guard = None;
        }
    }
}

/// Removes stale Nemirtingas command cache data so games that rely on the EOS
/// emulator do not trip assertions when multiple instances bootstrap in quick
/// succession.
fn reset_nemirtingas_session_state(nepice_dir: &Path) {
    let appdata = nepice_dir.join("appdata");
    if !appdata.exists() {
        return;
    }

    // Aggressively purge any stale EOS command artifacts before each launch so the emulator
    // never replays partially-submitted commands that crash with the COMMAND_STATE_SUBMITTED
    // assertion reported by players. We preserve the log directory so historical diagnostics
    // stay intact between sessions.
    let mut cleared_state = false;
    let logs_dir = appdata.join("Logs");
    match fs::read_dir(&appdata) {
        Ok(entries) => {
            for entry in entries.flatten() {
                let path = entry.path();
                if path == logs_dir {
                    continue;
                }

                let result = if entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false) {
                    fs::remove_dir_all(&path)
                } else {
                    fs::remove_file(&path)
                };

                match result {
                    Ok(_) => cleared_state = true,
                    Err(err) => println!(
                        "[PARTYDECK][WARN] Failed to remove stale Nemirtingas appdata {}: {}",
                        path.display(),
                        err
                    ),
                }
            }
        }
        Err(err) => println!(
            "[PARTYDECK][WARN] Failed to enumerate Nemirtingas appdata {}: {}",
            appdata.display(),
            err
        ),
    }

    if cleared_state {
        if let Err(err) = fs::create_dir_all(appdata.join("Commands")) {
            println!(
                "[PARTYDECK][WARN] Failed to recreate Nemirtingas command directory {}: {}",
                appdata.join("Commands").display(),
                err
            );
        }
    }
}

/// Ensures the targeted Proton prefix is not held by lingering Wine processes
/// by issuing a graceful shutdown and waiting for cleanup.
fn drain_stale_proton_session(prefix: &str, proton_env: &ProtonEnvironment) {
    let prefix_path = Path::new(prefix);
    if !prefix_path.exists() {
        return;
    }

    let actions = [("-k", "terminate"), ("-w", "wait for cleanup")];
    for (flag, description) in actions {
        let mut helper = Command::new(&*BIN_UMU_RUN);
        helper.env("PROTON_VERB", "run");
        helper.env("PROTONPATH", proton_env.env_value.clone());
        helper.env("WINEPREFIX", prefix);
        helper.env("STEAM_COMPAT_DATA_PATH", prefix);
        helper.env("SDL_JOYSTICK_HIDAPI", "0");
        helper.env("ENABLE_GAMESCOPE_WSI", "0");
        helper.env("PROTON_DISABLE_HIDRAW", "1");
        helper.arg("--");
        helper.arg("wineserver");
        helper.arg(flag);

        match helper.status() {
            Ok(status) => {
                if !status.success() {
                    log_launch_warning(&format!(
                        "wineserver {flag} failed to {description} prefix {} (status: {status})",
                        prefix_path.display(),
                    ));
                }
            }
            Err(err) => {
                log_launch_warning(&format!(
                    "Failed to run wineserver {flag} while preparing prefix {}: {}",
                    prefix_path.display(),
                    err
                ));
                break;
            }
        }
    }
}

/// Distributes CPU cores across running instances while keeping the affinity sets
/// as balanced as possible. The first few players (host included) receive a single
/// extra logical core whenever the CPU count is not perfectly divisible so hosting
/// retains a light advantage without starving other instances.
fn apply_instance_cpu_affinity(pid: u32, instance_index: usize, total_instances: usize) {
    if total_instances <= 1 {
        return;
    }

    let Ok(cpu_count) = std::thread::available_parallelism() else {
        println!(
            "[PARTYDECK][WARN] Unable to query CPU core count for affinity; leaving instance {} unpinned.",
            instance_index + 1
        );
        return;
    };
    let cpu_count = cpu_count.get();

    if cpu_count == 0 {
        println!(
            "[PARTYDECK][WARN] Reported CPU core count was zero; skipping affinity for instance {}.",
            instance_index + 1
        );
        return;
    }

    if cpu_count < total_instances {
        println!(
            "[PARTYDECK][WARN] Only {} CPU cores available for {} instances; skipping affinity to avoid starving players.",
            cpu_count, total_instances
        );
        return;
    }

    let base = cpu_count / total_instances;
    if base == 0 {
        return;
    }
    let remainder = cpu_count % total_instances;
    let extra = if instance_index < remainder { 1 } else { 0 };
    let target_width = base + extra;

    if target_width == 0 {
        println!(
            "[PARTYDECK][WARN] Calculated empty CPU set for instance {}; affinity skipped.",
            instance_index + 1
        );
        return;
    }

    // `CpuSet::new` zero-initializes an affinity mask for us on glibc-based
    // targets, so there is no failure path to handle here while targeting the
    // Steam Deck runtime.
    let mut cpuset = CpuSet::new();

    // Assign logical cores in a round-robin pattern so each instance stays close in size
    // while the first few players receive the leftover cores.
    let mut assigned: Vec<usize> = Vec::with_capacity(target_width);
    for core in (instance_index..cpu_count).step_by(total_instances) {
        assigned.push(core);
        if assigned.len() == target_width {
            break;
        }
    }

    if assigned.is_empty() {
        println!(
            "[PARTYDECK][WARN] No CPU cores mapped to instance {}; affinity skipped.",
            instance_index + 1
        );
        return;
    }

    for &core in &assigned {
        if let Err(err) = cpuset.set(core) {
            println!(
                "[PARTYDECK][WARN] Unable to add core {} to affinity set for instance {}: {}",
                core,
                instance_index + 1,
                err
            );
            return;
        }
    }

    let target_pid = Pid::from_raw(pid as i32);
    match sched_setaffinity(target_pid, &cpuset) {
        Ok(_) => {
            let core_list = assigned
                .iter()
                .map(|core| core.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            println!(
                "[PARTYDECK] Bound instance {}/{} (PID {}) to CPU cores [{}]",
                instance_index + 1,
                total_instances,
                pid,
                core_list
            );
        }
        Err(err) => {
            println!(
                "[PARTYDECK][WARN] Failed to set CPU affinity for instance {}: {}",
                instance_index + 1,
                err
            );
        }
    }
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

/// Gamescope repeats this benign warning endlessly; capture the invariant suffix so we can filter
/// both the standard and `gamescope-kbm` variants without hard-coding each prefix.
const GAMESCOPE_DUP_BUFFER_WARNING_SUFFIX: &str =
    "[Warn]  xwm: got the same buffer committed twice, ignoring.";

/// Streams child output on a background thread while suppressing the noisy duplicate-buffer warning.
fn forward_child_output<R>(reader: R)
where
    R: Read + Send + 'static,
{
    std::thread::spawn(move || {
        let reader = BufReader::new(reader);
        for line in reader.lines() {
            match line {
                Ok(line) => {
                    let trimmed = line.trim();
                    if trimmed.starts_with("[gamescope")
                        && trimmed.ends_with(GAMESCOPE_DUP_BUFFER_WARNING_SUFFIX)
                    {
                        continue;
                    }
                    println!("{line}");
                }
                Err(err) => {
                    println!("[PARTYDECK][WARN] Failed to read child output: {err}");
                    break;
                }
            }
        }
    });
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

    if !handler.path_nemirtingas.is_empty() {
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

        // Walk upward from the Nemirtingas config directory so we also catch EOSSDK files
        // that sit next to the executable instead of inside the nepice_settings folder.
        let gamedir_path = PathBuf::from(gamedir);
        let mut eos_paths = Vec::new();
        let mut scanned_dirs = Vec::new();
        let mut search_dir = parent_path.clone();
        while search_dir.starts_with(&gamedir_path) {
            scanned_dirs.push(search_dir.clone());

            match fs::read_dir(&search_dir) {
                Ok(entries) => {
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
                }
                Err(err) => {
                    log_launch_warning(&format!(
                        "Failed to scan {} for EOSSDK files: {}. Verify directory permissions.",
                        search_dir.display(),
                        err
                    ));
                    return;
                }
            }

            if !eos_paths.is_empty() {
                break;
            }

            let Some(parent) = search_dir.parent() else {
                break;
            };
            if parent == search_dir || !parent.starts_with(&gamedir_path) {
                break;
            }
            search_dir = parent.to_path_buf();
        }

        if eos_paths.is_empty() {
            let scanned_display = if scanned_dirs.is_empty() {
                String::from("<none>")
            } else {
                scanned_dirs
                    .iter()
                    .map(|dir| dir.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            log_launch_warning(&format!(
                "No EOSSDK files were found near {} (searched: {}). Nemirtingas may fail to initialize.",
                nemirtingas_target.display(),
                scanned_display
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

    if handler.path_goldberg.is_empty() {
        return;
    }

    // Surface the resolved Goldberg override directory so the user can spot missing assets.
    let goldberg_dir = PathBuf::from(gamedir).join(&handler.path_goldberg);
    println!(
        "[PARTYDECK] Handler {} expects Goldberg assets at {}",
        handler.uid,
        goldberg_dir.display()
    );

    if !goldberg_dir.exists() {
        log_launch_warning(&format!(
            "Goldberg directory {} is missing. Ensure the handler copied Goldberg files there.",
            goldberg_dir.display()
        ));
        return;
    }

    // Validate the presence of the per-game steam_settings folder and critical config files.
    let steam_settings = goldberg_dir.join("steam_settings");
    if !steam_settings.exists() {
        log_launch_warning(&format!(
            "Goldberg path {} lacks a steam_settings directory. Multiplayer emulation will likely fail.",
            goldberg_dir.display()
        ));
        return;
    }

    for (filename, description) in [
        ("steam_appid.txt", "Steam App ID"),
        ("configs.user.ini", "user configuration"),
        ("steam_interfaces.txt", "interface list"),
    ] {
        let file_path = steam_settings.join(filename);
        if !file_path.exists() {
            log_launch_warning(&format!(
                "steam_settings at {} is missing {} ({}).",
                steam_settings.display(),
                filename,
                description
            ));
            continue;
        }

        // Emit light diagnostics so we can inspect mismatched App IDs at a glance.
        if filename == "steam_appid.txt" {
            match fs::read_to_string(&file_path) {
                Ok(contents) => {
                    let trimmed = contents.trim();
                    if let Some(expected_appid) = &handler.steam_appid {
                        if trimmed != expected_appid {
                            log_launch_warning(&format!(
                                "steam_appid.txt at {} contains {} but handler expects {}.",
                                file_path.display(),
                                trimmed,
                                expected_appid
                            ));
                        }
                    }
                    println!(
                        "[PARTYDECK] Detected steam_appid.txt at {} with value {}",
                        file_path.display(),
                        trimmed
                    );
                }
                Err(err) => {
                    log_launch_warning(&format!("Failed to read {}: {}", file_path.display(), err));
                }
            }
        } else {
            println!(
                "[PARTYDECK] Found Goldberg config file: {}",
                file_path.display()
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

    let profile_names: Vec<String> = instances
        .iter()
        .map(|instance| instance.profname.clone())
        .collect();

    let mut synchronized_goldberg_port: Option<u16> = None;
    if let HandlerRef(h) = game {
        if !h.path_goldberg.is_empty() && !profile_names.is_empty() {
            // Normalize Goldberg LAN metadata so every running instance advertises the
            // same listen port and exposes required identity files for lobby discovery.
            synchronized_goldberg_port =
                synchronize_goldberg_profiles(&profile_names, &game_id, None)?;
        }
    }

    if let Some(port) = synchronized_goldberg_port {
        println!(
            "[PARTYDECK] Goldberg listen_port for {} synchronized to {}",
            game_id, port
        );
    }
    let mut nemirtingas_ports: HashMap<String, u16> = HashMap::new();
    if let HandlerRef(h) = game {
        if !h.path_nemirtingas.is_empty() && !profile_names.is_empty() {
            // Resolve deterministic Nemirtingas LAN ports per profile so each instance binds a
            // unique UDP socket without fighting for the same override on the same machine.
            nemirtingas_ports =
                resolve_nemirtingas_ports(&profile_names, &game_id, synchronized_goldberg_port);

            for profile in &profile_names {
                if let Some(port) = nemirtingas_ports.get(profile) {
                    println!(
                        "[PARTYDECK] Nemirtingas LAN port for profile {} on {} resolved to {}",
                        profile, game_id, port
                    );
                }
            }
        }
    }
    let mut locks_vec = Vec::new();
    for instance in instances {
        let lock = ProfileLock::acquire(&game_id, &instance.profname)?;
        locks_vec.push(lock);
    }
    let locks = Arc::new(Mutex::new(locks_vec));
    let child_pids: Arc<Mutex<Vec<u32>>> = Arc::new(Mutex::new(Vec::new()));
    register_ctrlc_cleanup(Arc::clone(&child_pids), Arc::clone(&locks))?;

    let home = PATH_HOME.to_string_lossy().to_string();
    let localshare = PATH_LOCAL_SHARE.to_string_lossy().to_string();
    let party = PATH_PARTY.to_string_lossy().to_string();
    let steam = PATH_STEAM.to_string_lossy().to_string();

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

    let proton_env = if win {
        let resolved = resolve_proton_environment(cfg.proton_version.as_str());
        if resolved.root_path.is_none() {
            log_launch_warning(&format!(
                "Unable to verify Proton build '{}' on disk; continuing with the provided hint.",
                resolved.display_name
            ));
        } else if let Some(path) = &resolved.root_path {
            println!(
                "[PARTYDECK] Using Proton build {} at {}",
                resolved.display_name,
                path.display()
            );
        }
        Some(resolved)
    } else {
        None
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

    let mut drained_prefixes: HashSet<String> = HashSet::new();
    let mut runtime_instances: Vec<RuntimeInstance> = Vec::new();
    for (i, instance) in instances.iter().enumerate() {
        let outcome = spawn_instance_child(
            i,
            instance,
            game,
            &game_id,
            &gamedir,
            &exec,
            &runtime,
            win,
            use_bwrap,
            cfg,
            input_devices,
            proton_env.as_ref(),
            &nemirtingas_ports,
            &mut drained_prefixes,
            &party,
            &steam,
            &home,
            &localshare,
        )?;

        let mut child = outcome.child;
        let raw_pid = child.id();
        child_pids.lock().unwrap().push(raw_pid);
        apply_instance_cpu_affinity(raw_pid, i, instances.len());
        promote_instance_priority(raw_pid, i, instances.len());

        if let Some(stdout) = child.stdout.take() {
            forward_child_output(stdout);
        }
        if let Some(stderr) = child.stderr.take() {
            forward_child_output(stderr);
        }

        runtime_instances.push(RuntimeInstance {
            index: i,
            profile_name: instance.profname.clone(),
            instance: instance.clone(),
            child: Some(child),
            last_pid: Some(raw_pid),
            log_context: outcome.log_context,
            proton_prefix: outcome.proton_prefix,
            finished: false,
        });

        if i < instances.len() - 1 {
            std::thread::sleep(Duration::from_secs(6));
        }
    }

    while runtime_instances.iter().any(|state| !state.finished) {
        let mut made_progress = false;
        for state in runtime_instances.iter_mut() {
            let Some(child) = state.child.as_mut() else {
                continue;
            };

            match child.try_wait() {
                Ok(Some(status)) => {
                    if let Some(pid) = state.last_pid.take() {
                        unregister_child_pid(&child_pids, pid);
                    }
                    state.child = None;

                    let mut restart_requested = false;
                    if !status.success() {
                        println!(
                            "[PARTYDECK][WARN] Instance {} exited unexpectedly (status: {:?}).",
                            state.profile_name, status
                        );
                        let prompt = format!(
                            "Profile {} closed unexpectedly. Restart it in the reserved slot?",
                            state.profile_name
                        );
                        restart_requested = yesno("Restart crashed instance?", &prompt);
                    }

                    if restart_requested {
                        if let Some(prefix) = state.proton_prefix.clone() {
                            drained_prefixes.remove(&prefix);
                        }
                        std::thread::sleep(Duration::from_secs(2));
                        match spawn_instance_child(
                            state.index,
                            &state.instance,
                            game,
                            &game_id,
                            &gamedir,
                            &exec,
                            &runtime,
                            win,
                            use_bwrap,
                            cfg,
                            input_devices,
                            proton_env.as_ref(),
                            &nemirtingas_ports,
                            &mut drained_prefixes,
                            &party,
                            &steam,
                            &home,
                            &localshare,
                        ) {
                            Ok(mut respawn) => {
                                let new_pid = respawn.child.id();
                                child_pids.lock().unwrap().push(new_pid);
                                apply_instance_cpu_affinity(new_pid, state.index, instances.len());
                                promote_instance_priority(new_pid, state.index, instances.len());

                                if let Some(stdout) = respawn.child.stdout.take() {
                                    forward_child_output(stdout);
                                }
                                if let Some(stderr) = respawn.child.stderr.take() {
                                    forward_child_output(stderr);
                                }

                                state.child = Some(respawn.child);
                                state.last_pid = Some(new_pid);
                                state.log_context = respawn.log_context;
                                state.proton_prefix = respawn.proton_prefix;
                                state.finished = false;
                                println!(
                                    "[PARTYDECK] Restarted profile {} in slot {}.",
                                    state.profile_name,
                                    state.index + 1
                                );
                            }
                            Err(err) => {
                                println!(
                                    "[PARTYDECK][WARN] Failed to restart instance {}: {}",
                                    state.profile_name, err
                                );
                                state.finished = true;
                            }
                        }
                    } else {
                        state.finished = true;
                    }

                    made_progress = true;
                }
                Ok(None) => {}
                Err(err) => {
                    println!(
                        "[PARTYDECK][WARN] Failed to poll instance {}: {}",
                        state.profile_name, err
                    );
                }
            }
        }

        if !made_progress {
            std::thread::sleep(Duration::from_millis(250));
        }
    }

    let nemirtingas_logs: Vec<NemirtingasLogContext> = runtime_instances
        .iter()
        .map(|state| state.log_context.clone())
        .collect();

    collect_nemirtingas_logs(&nemirtingas_logs);

    if let Ok(pids) = child_pids.lock() {
        for pid in pids.iter() {
            let _ = kill(Pid::from_raw(-(*pid as i32)), Signal::SIGTERM);
        }
    }
    locks.lock().unwrap().clear();
    clear_ctrlc_cleanup();

    if cfg.enable_kwin_script {
        kwin_dbus_unload_script()?;
    }

    remove_guest_profiles()?;

    Ok(())
}
