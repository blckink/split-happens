use crate::paths::*;

use std::collections::HashMap;
use std::error::Error;
use std::fs::File;
use std::io::BufReader;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, PartialEq)]
pub enum PadFilterType {
    All,
    NoSteamInput,
    OnlySteamInput,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct PartyConfig {
    pub force_sdl: bool,
    pub enable_kwin_script: bool,
    pub gamescope_fix_lowres: bool,
    pub gamescope_sdl_backend: bool,
    pub kbm_support: bool,
    pub proton_version: String,
    pub proton_separate_pfxs: bool,
    #[serde(default)]
    pub vertical_two_player: bool,
    pub pad_filter_type: PadFilterType,
    #[serde(default)]
    pub last_profile_assignments: HashMap<String, Vec<String>>,
    // Performance toggles that gate optional Steam Deck optimizations.
    #[serde(default)]
    pub performance_limit_40fps: bool,
    #[serde(default)]
    pub performance_gamescope_rt: bool,
    #[serde(default)]
    pub performance_enable_proton_fsr: bool,
}

impl Default for PartyConfig {
    fn default() -> Self {
        PartyConfig {
            force_sdl: false,
            enable_kwin_script: true,
            gamescope_fix_lowres: true,
            gamescope_sdl_backend: true,
            kbm_support: true,
            proton_version: "".to_string(),
            proton_separate_pfxs: false,
            vertical_two_player: false,
            pad_filter_type: PadFilterType::NoSteamInput,
            last_profile_assignments: HashMap::new(),
            performance_limit_40fps: false,
            performance_gamescope_rt: false,
            performance_enable_proton_fsr: false,
        }
    }
}

pub fn load_cfg() -> PartyConfig {
    let path = PATH_APP.join("settings.json");

    if let Ok(file) = File::open(path) {
        if let Ok(config) = serde_json::from_reader::<_, PartyConfig>(BufReader::new(file)) {
            return config;
        }
    }

    // Return default settings if file doesn't exist or has error
    return PartyConfig::default();
}

pub fn save_cfg(config: &PartyConfig) -> Result<(), Box<dyn Error>> {
    let path = PATH_APP.join("settings.json");
    let file = File::create(path)?;
    serde_json::to_writer_pretty(file, config)?;
    Ok(())
}
