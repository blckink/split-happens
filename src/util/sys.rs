use dialog::{Choice, DialogBox};
use std::error::Error;
use std::io::{Error as IoError, ErrorKind};
use std::ops::Deref;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, OnceLock};
use x11rb::connection::Connection;
use zbus::Error as ZbusError;
use zbus::zvariant::{OwnedValue, Value};

use super::steamdeck::is_steam_deck;

/// Tracks the active KWin script identifier so we can cleanly stop it after the
/// last Split Happens instance terminates.
/// Persists the raw identifier returned by KWin when loading the helper script so
/// we can later stop the exact runtime instance regardless of the concrete type
/// (some platforms report a string name, others an integer handle).
static KWIN_SCRIPT_ID: OnceLock<Mutex<Option<OwnedValue>>> = OnceLock::new();

/// Convenience helper that provides access to the script identifier storage.
fn kwin_script_slot() -> &'static Mutex<Option<OwnedValue>> {
    KWIN_SCRIPT_ID.get_or_init(|| Mutex::new(None))
}

/// Locks the script identifier storage and maps poisoning into a descriptive IO
/// error so callers can bubble the failure up uniformly.
fn lock_kwin_script_slot() -> Result<MutexGuard<'static, Option<OwnedValue>>, Box<dyn Error>> {
    kwin_script_slot().lock().map_err(|_| {
        Box::new(IoError::new(
            ErrorKind::Other,
            "Failed to lock KWin script storage",
        )) as Box<dyn Error>
    })
}

/// Formats the dynamically typed DBus identifier into a human readable label so
/// launch logs stay understandable even when KWin reports numeric handles.
fn describe_kwin_id(id: &OwnedValue) -> String {
    match id.deref() {
        Value::Str(text) => text.to_string(),
        Value::I32(num) => num.to_string(),
        Value::I64(num) => num.to_string(),
        Value::U32(num) => num.to_string(),
        Value::U64(num) => num.to_string(),
        other => format!("{other:?}"),
    }
}

/// Detects when KWin refuses a DBus call because the argument signature
/// mismatched its expectations so we can fall back to a string-based API.
fn kwin_signature_mismatch(err: &ZbusError) -> bool {
    err.to_string().contains("Signature mismatch")
}

pub fn msg(title: &str, contents: &str) {
    let _ = dialog::Message::new(contents).title(title).show();
}

pub fn yesno(title: &str, contents: &str) -> bool {
    if let Ok(prompt) = dialog::Question::new(contents).title(title).show() {
        if prompt == Choice::Yes {
            return true;
        }
    }
    false
}

pub fn get_screen_resolution() -> (u32, u32) {
    if let Ok(conn) = x11rb::connect(None) {
        let screen = &conn.0.setup().roots[0];
        println!(
            "Got screen resolution: {}x{}",
            screen.width_in_pixels, screen.height_in_pixels
        );
        return (
            screen.width_in_pixels as u32,
            screen.height_in_pixels as u32,
        );
    }
    // Fallback to a common resolution if detection fails
    println!("Failed to detect screen resolution, using Steam Deck friendly fallback");
    if is_steam_deck() {
        (1280, 800)
    } else {
        (1920, 1080)
    }
}

// Sends the splitscreen script to the active KWin session through DBus
pub fn kwin_dbus_start_script(file: PathBuf) -> Result<(), Box<dyn Error>> {
    println!("Loading script {}...", file.display());
    if !file.exists() {
        return Err("Script file doesn't exist!".into());
    }

    let conn = zbus::blocking::Connection::session()?;
    let proxy = zbus::blocking::Proxy::new(
        &conn,
        "org.kde.KWin",
        "/Scripting",
        "org.kde.kwin.Scripting",
    )?;

    // Ask KWin to load the script and capture the concrete runtime identifier so
    // we can start and later unload the exact instance that was registered.
    let script_id: OwnedValue = proxy.call(
        "loadScript",
        &(file.to_string_lossy().into_owned(), "splitscreen"),
    )?;
    println!(
        "Script loaded as id {}. Starting...",
        describe_kwin_id(&script_id)
    );

    // Launch the freshly registered script so all future game windows are
    // immediately snapped into their target positions, regardless of the
    // identifier type reported by the compositor.
    // Prefer the identifier returned by KWin so we stop the exact runtime instance,
    // but gracefully fall back to the legacy string-based API when the compositor
    // rejects numeric handles on newer Plasma builds.
    if let Err(err) = proxy.call::<_, _, ()>("start", &(script_id.clone(),)) {
        if kwin_signature_mismatch(&err) {
            println!(
                "KWin rejected script id {}; retrying with string fallback...",
                describe_kwin_id(&script_id)
            );
            proxy.call::<_, _, ()>("start", &("splitscreen",))?;
        } else {
            return Err(Box::new(err));
        }
    }

    // Remember which script instance we activated to avoid leaving stray
    // registrations behind when the session terminates.
    let mut slot = lock_kwin_script_slot()?;
    *slot = Some(script_id);

    println!("KWin script started.");
    Ok(())
}

pub fn kwin_dbus_unload_script() -> Result<(), Box<dyn Error>> {
    println!("Unloading splitscreen script...");
    let conn = zbus::blocking::Connection::session()?;
    let proxy = zbus::blocking::Proxy::new(
        &conn,
        "org.kde.KWin",
        "/Scripting",
        "org.kde.kwin.Scripting",
    )?;

    // Attempt to unload the exact script instance we started earlier and fall
    // back to the legacy name-based call when no identifier was recorded.
    let script_id = lock_kwin_script_slot()?.take();

    if let Some(id) = script_id {
        // Attempt to unload by identifier first and gracefully fall back to the
        // string API when the compositor expects a name-only signature.
        let label = describe_kwin_id(&id);
        if let Err(err) = proxy.call::<_, _, bool>("unloadScript", &(id,)) {
            if kwin_signature_mismatch(&err) {
                println!(
                    "KWin rejected script id {}; unloading via name fallback...",
                    label
                );
                proxy.call::<_, _, bool>("unloadScript", &("splitscreen",))?;
            } else {
                return Err(Box::new(err));
            }
        }
    } else {
        let _: bool = proxy.call("unloadScript", &("splitscreen"))?;
    }

    println!("Script unloaded.");
    Ok(())
}
