mod app;
mod game;
mod handler;
mod input;
mod instance;
mod launch;
mod paths;
mod util;

use crate::app::*;
use crate::paths::PATH_APP;
use crate::util::*;

fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().collect();

    if std::env::args().any(|arg| arg == "--help") {
        println!("{}", USAGE_TEXT);
        std::process::exit(0);
    }

    if std::env::args().any(|arg| arg == "--kwin") {
        let args: Vec<String> = std::env::args().filter(|arg| arg != "--kwin").collect();

        let (w, h) = get_screen_resolution();
        let mut cmd = std::process::Command::new("kwin_wayland");

        cmd.arg("--xwayland");
        cmd.arg("--width");
        cmd.arg(w.to_string());
        cmd.arg("--height");
        cmd.arg(h.to_string());
        cmd.arg("--exit-with-session");
        let args_string = args
            .iter()
            .map(|arg| format!("\"{}\"", arg))
            .collect::<Vec<String>>()
            .join(" ");
        cmd.arg(args_string);

        println!("[SPLIT HAPPENS] Launching kwin session: {:?}", cmd);

        match cmd.spawn() {
            Ok(_) => std::process::exit(0),
            Err(e) => {
                eprintln!("Failed to start kwin_wayland: {}", e);
                std::process::exit(1);
            }
        }
    }

    let mut exec = String::new();
    let mut execargs = String::new();
    if let Some(exec_index) = args.iter().position(|arg| arg == "--exec") {
        if let Some(next_arg) = args.get(exec_index + 1) {
            exec = next_arg.clone();
        } else {
            eprintln!("{}", USAGE_TEXT);
            std::process::exit(1);
        }
    }
    if let Some(execargs_index) = args.iter().position(|arg| arg == "--args") {
        if let Some(next_arg) = args.get(execargs_index + 1) {
            execargs = next_arg.clone();
        } else {
            eprintln!("{}", USAGE_TEXT);
            std::process::exit(1);
        }
    }

    let fullscreen = std::env::args().any(|arg| arg == "--fullscreen");

    std::fs::create_dir_all(PATH_APP.join("gamesyms"))
        .expect("Failed to create gamesyms directory");
    std::fs::create_dir_all(PATH_APP.join("handlers"))
        .expect("Failed to create handlers directory");
    std::fs::create_dir_all(PATH_APP.join("profiles"))
        .expect("Failed to create profiles directory");

    remove_guest_profiles().unwrap();

    if PATH_APP.join("tmp").exists() {
        std::fs::remove_dir_all(PATH_APP.join("tmp")).unwrap();
    }

    let (_, scrheight) = get_screen_resolution();
    let zoom_factor = recommended_zoom_factor(fullscreen, scrheight);
    let repaint_interval = recommended_repaint_interval(fullscreen, scrheight);
    let steamdeck = is_steam_deck();

    let light = !exec.is_empty();

    let win_width = match light {
        true => 900.0,
        false => 1080.0,
    };

    let mut options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([win_width, 540.0])
            .with_min_inner_size([640.0, 360.0])
            .with_fullscreen(fullscreen)
            .with_icon(
                eframe::icon_data::from_png_bytes(&include_bytes!("../res/icon.png")[..])
                    .expect("Failed to load icon"),
            ),
        ..Default::default()
    };
    options.vsync = true;
    if steamdeck {
        options.hardware_acceleration = eframe::HardwareAcceleration::Required;
    }

    println!("\n[SPLIT HAPPENS] starting...\n");
    if steamdeck {
        println!("[SPLIT HAPPENS] Steam Deck optimizations enabled");
    }

    eframe::run_native(
        "Split Happens",
        options,
        Box::new(move |cc| {
            // This gives us image support:
            egui_extras::install_image_loaders(&cc.egui_ctx);
            cc.egui_ctx.set_zoom_factor(zoom_factor);
            apply_split_happens_theme(&cc.egui_ctx);
            Ok(match light {
                true => Box::<LightPartyApp>::new(LightPartyApp::new_lightapp(
                    exec,
                    execargs,
                    repaint_interval,
                )),
                false => Box::<PartyApp>::new(PartyApp::with_repaint_interval(repaint_interval)),
            })
        }),
    )
}

static USAGE_TEXT: &str = r#"
{}
Usage: split-happens [OPTIONS]

Options:
    --exec <executable>   Execute the specified executable in splitscreen. If this isn't specified, Split Happens will launch in the regular GUI mode.
    --args [args]         Specify arguments for the executable to be launched with. Must be quoted if containing spaces.
    --fullscreen          Start the GUI in fullscreen mode
    --kwin                Launch Split Happens inside of a KWin session
"#;
