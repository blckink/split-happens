use crate::paths::PATH_STEAM;

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

/// Enumerates the different sources a Proton installation can originate from so
/// the UI can provide a readable badge next to each option.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProtonSource {
    CompatibilityTool,
    SteamRuntime,
}

/// Captures metadata about a Proton installation that PartyDeck can expose to
/// the user or use internally to prepare the launcher environment.
#[derive(Clone, Debug)]
pub struct ProtonInstall {
    pub id: String,
    pub display_name: String,
    pub root_path: PathBuf,
    pub source: ProtonSource,
}

impl ProtonInstall {
    /// Returns the label that should be shown inside selection widgets.
    pub fn display_label(&self) -> String {
        let badge = match self.source {
            ProtonSource::CompatibilityTool => "Custom",
            ProtonSource::SteamRuntime => "Steam",
        };
        format!("{} ({badge})", self.display_name)
    }

    /// Checks if the stored installation matches a given settings value.
    pub fn matches(&self, value: &str) -> bool {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return false;
        }
        self.id.eq_ignore_ascii_case(trimmed)
            || self.display_name.eq_ignore_ascii_case(trimmed)
            || self
                .root_path
                .to_string_lossy()
                .eq_ignore_ascii_case(trimmed)
    }
}

/// Describes the Proton runtime configuration derived from the settings file
/// so the launcher can hydrate environment variables and optional helpers.
#[derive(Clone, Debug)]
pub struct ProtonEnvironment {
    /// String assigned to PROTONPATH so Proton knows which runtime to load.
    pub env_value: String,
    /// Human-readable label surfaced to logs and diagnostics.
    pub display_name: String,
    /// Canonical Proton installation directory when it exists on disk.
    pub root_path: Option<PathBuf>,
}

/// Discovers Proton installations in the user's Steam directory so the
/// settings screen can offer a curated drop-down.
pub fn discover_proton_versions() -> Vec<ProtonInstall> {
    let mut installs: Vec<ProtonInstall> = Vec::new();

    // Collect custom compatibility tools that ship as Proton builds.
    collect_proton_under(
        &PATH_STEAM.join("compatibilitytools.d"),
        ProtonSource::CompatibilityTool,
        &mut installs,
    );

    // Collect the official Steam-distributed Proton builds.
    collect_proton_under(
        &PATH_STEAM.join("steamapps/common"),
        ProtonSource::SteamRuntime,
        &mut installs,
    );

    // Deduplicate installations that may appear twice because of symlinks and
    // keep the list sorted for deterministic UI ordering.
    let mut seen: HashSet<PathBuf> = HashSet::new();
    installs.retain(|install| {
        let canonical = install
            .root_path
            .canonicalize()
            .unwrap_or_else(|_| install.root_path.clone());
        if seen.contains(&canonical) {
            return false;
        }
        seen.insert(canonical);
        true
    });

    installs.sort_by(|a, b| a.display_name.cmp(&b.display_name));
    installs
}

/// Resolves a Proton environment configuration from a textual settings value.
pub fn resolve_proton_environment(value: &str) -> ProtonEnvironment {
    let trimmed = value.trim();
    let installs = discover_proton_versions();

    // Fall back to the default GE-Proton build whenever the user left the
    // field empty, keeping compatibility with previous PartyDeck releases.
    if trimmed.is_empty() {
        if let Some(install) = installs.iter().find(|install| install.matches("GE-Proton")) {
            let path = install.root_path.clone();
            return ProtonEnvironment {
                env_value: path.to_string_lossy().to_string(),
                display_name: install.display_name.clone(),
                root_path: Some(path),
            };
        }
        return ProtonEnvironment {
            env_value: "GE-Proton".to_string(),
            display_name: "GE-Proton".to_string(),
            root_path: None,
        };
    }

    let candidate = PathBuf::from(trimmed);
    if candidate.exists() {
        let root = if candidate.is_dir() {
            candidate
        } else {
            candidate
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or(candidate)
        };
        return ProtonEnvironment {
            env_value: root.to_string_lossy().to_string(),
            display_name: trimmed.to_string(),
            root_path: Some(root),
        };
    }

    if let Some(install) = installs.iter().find(|install| install.matches(trimmed)) {
        let path = install.root_path.clone();
        return ProtonEnvironment {
            env_value: path.to_string_lossy().to_string(),
            display_name: install.display_name.clone(),
            root_path: Some(path),
        };
    }

    ProtonEnvironment {
        env_value: trimmed.to_string(),
        display_name: trimmed.to_string(),
        root_path: None,
    }
}

/// Enumerates Proton-like directories under a root and stores any valid
/// installations inside the provided vector.
fn collect_proton_under(root: &Path, source: ProtonSource, installs: &mut Vec<ProtonInstall>) {
    if !root.exists() {
        return;
    }

    let Ok(entries) = fs::read_dir(root) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        if !is_valid_proton_root(&path) {
            continue;
        }

        let name = entry.file_name().to_string_lossy().trim().to_string();
        installs.push(ProtonInstall {
            id: name.clone(),
            display_name: name,
            root_path: path,
            source,
        });
    }
}

/// Detects whether a directory contains a Proton distribution by checking for
/// the canonical launcher script and the Wine binaries folder.
fn is_valid_proton_root(path: &Path) -> bool {
    let script = path.join("proton");
    if script.exists() {
        return true;
    }

    let dist_wine = path.join("dist/bin/wine");
    let files_wine = path.join("files/bin/wine");
    dist_wine.exists() || files_wine.exists()
}
