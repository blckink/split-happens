use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::fs::{File, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::paths::PATH_PARTY;
use crate::util::SanitizePath;

#[derive(Serialize, Deserialize)]
struct LockInfo {
    pid: u32,
    profile: String,
    game: String,
    started_at: u64,
}

pub struct ProfileLock {
    file: File,
    pub path: PathBuf,
}

impl ProfileLock {
    pub fn acquire(game: &str, profile: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let dir = PATH_PARTY.join("run/locks");
        std::fs::create_dir_all(&dir)?;
        let game = game.to_string().sanitize_path();
        let path = dir.join(format!("{}_{}.lock", game, profile));
        loop {
            let mut file = OpenOptions::new().read(true).write(true).create(true).open(&path)?;
            match file.try_lock_exclusive() {
                Ok(()) => {
                    let info = LockInfo {
                        pid: std::process::id(),
                        profile: profile.to_string(),
                        game: game.clone(),
                        started_at: SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs(),
                    };
                    file.set_len(0)?;
                    file.write_all(serde_json::to_string(&info)?.as_bytes())?;
                    file.sync_all()?;
                    return Ok(ProfileLock { file, path });
                }
                Err(e) if e.kind() == ErrorKind::WouldBlock => {
                    drop(file);
                    if Self::stale(&path, profile) {
                        println!("Removing stale lock {}", path.display());
                        std::fs::remove_file(&path)?;
                        continue;
                    } else {
                        if let Ok(content) = std::fs::read_to_string(&path) {
                            if let Ok(info) = serde_json::from_str::<LockInfo>(&content) {
                                println!("Instance {} already running with PID {}", info.profile, info.pid);
                            }
                        }
                        return Err("Instance already running".into());
                    }
                }
                Err(e) => return Err(e.into()),
            }
        }
    }

    fn stale(path: &Path, profile: &str) -> bool {
        if let Ok(content) = std::fs::read_to_string(path) {
            if let Ok(info) = serde_json::from_str::<LockInfo>(&content) {
                return !Self::process_matches(info.pid, profile);
            }
        }
        true
    }

    fn process_matches(pid: u32, profile: &str) -> bool {
        let cmdline_path = format!("/proc/{pid}/cmdline");
        if let Ok(cmdline) = std::fs::read_to_string(cmdline_path) {
            let cmdline = cmdline.replace('\0', " ");
            return cmdline.contains(profile)
                && (cmdline.contains("gamescope")
                    || cmdline.contains("gsc-kbm")
                    || cmdline.contains("bwrap")
                    || cmdline.contains("umu-run"));
        }
        false
    }

    pub fn cleanup(&self) {
        let _ = self.file.unlock();
        let _ = std::fs::remove_file(&self.path);
    }
}

impl Drop for ProfileLock {
    fn drop(&mut self) {
        self.cleanup();
    }
}

