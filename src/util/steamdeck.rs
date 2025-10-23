use std::fs;
use std::sync::OnceLock;
use std::time::Duration;

/// Caches whether the current process is running on an actual Steam Deck so we
/// can branch once and reuse the result for subsequent performance hints.
static IS_STEAM_DECK: OnceLock<bool> = OnceLock::new();

/// Returns `true` when PartyDeck detects a Steam Deck host environment.
///
/// SteamOS exposes several markers that we probe in a best-effort fashion so
/// development builds on desktop Linux keep functioning while real Deck users
/// benefit from the tuned defaults.
pub fn is_steam_deck() -> bool {
    *IS_STEAM_DECK.get_or_init(|| {
        if std::env::var("STEAMDECK").is_ok() || std::env::var("SteamDeck").is_ok() {
            return true;
        }

        if let Ok(contents) = fs::read_to_string("/etc/os-release") {
            let contents = contents.to_ascii_lowercase();
            if contents.contains("steamos") || contents.contains("steam deck") {
                return true;
            }
        }

        if let Ok(product_name) = fs::read_to_string("/sys/devices/virtual/dmi/id/product_name") {
            let product_name = product_name.to_ascii_lowercase();
            if product_name.contains("jupiter") || product_name.contains("galileo") {
                return true;
            }
        }

        false
    })
}

/// Calculates the GUI zoom factor to keep the layout comfortable on TVs and the
/// built-in Steam Deck screen without requiring the user to tweak the slider.
///
/// We intentionally clamp the scale so oversized 4K televisions do not blow up
/// the UI beyond recognition while handheld mode remains readable.
pub fn recommended_zoom_factor(fullscreen: bool, screen_height: u32) -> f32 {
    if fullscreen {
        return (screen_height as f32 / 720.0).clamp(1.1, 2.0);
    }

    if is_steam_deck() {
        return 1.2;
    }

    1.3
}

/// Suggests an egui repaint interval tailored for Steam Deck usage patterns so
/// docked TV play gets the responsive menus it needs without wasting battery in
/// handheld mode.
pub fn recommended_repaint_interval(fullscreen: bool, screen_height: u32) -> Duration {
    if !is_steam_deck() {
        return Duration::from_millis(33);
    }

    if screen_height >= 1440 {
        // Favor a full 60 FPS when the Deck is pushing a large external panel.
        Duration::from_millis(16)
    } else if fullscreen {
        // Handheld fullscreen benefits from a smoother feel without maxing the APU.
        Duration::from_millis(22)
    } else {
        // Windowed helpers can redraw a little slower to save power while docked.
        Duration::from_millis(27)
    }
}
