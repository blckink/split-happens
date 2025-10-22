// Re-export all utility functions from submodules
mod filesystem;
mod hash;
mod lock;
mod profiles;
mod proton;
mod sys;
mod updates;

// Re-export functions from profiles
pub use profiles::{
    create_gamesave, create_profile, ensure_nemirtingas_config, remove_guest_profiles,
    resolve_nemirtingas_ports, scan_profiles, synchronize_goldberg_profiles,
};

// Re-export functions from filesystem
pub use filesystem::{SanitizePath, copy_dir_recursive, get_rootpath, get_rootpath_handler};

pub use hash::sha1_file;

pub use lock::ProfileLock;

// Re-export functions from launcher
pub use sys::{get_screen_resolution, kwin_dbus_start_script, kwin_dbus_unload_script, msg, yesno};

// Re-export functions from updates
pub use updates::check_for_partydeck_update;

// Re-export Proton helpers so the UI and launcher can reference them directly.
pub use proton::{
    ProtonEnvironment, ProtonInstall, ProtonSource, discover_proton_versions,
    resolve_proton_environment,
};
