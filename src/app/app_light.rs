use std::thread::sleep;

use super::config::*;
use crate::game::*;
use crate::input::*;
use crate::instance::*;
use crate::launch::launch_game;
use crate::util::*;

use std::path::PathBuf;

use eframe::egui::RichText;
use eframe::egui::{self, Ui};

#[derive(Eq, PartialEq)]
pub enum MenuPage {
    Settings,
    Instances,
}

pub struct LightPartyApp {
    pub options: PartyConfig,
    pub cur_page: MenuPage,
    pub infotext: String,

    pub input_devices: Vec<InputDevice>,
    pub instances: Vec<Instance>,
    pub instance_add_dev: Option<usize>,
    pub game: Game,
    pub proton_versions: Vec<ProtonInstall>,

    pub loading_msg: Option<String>,
    pub loading_since: Option<std::time::Instant>,
    #[allow(dead_code)]
    pub task: Option<std::thread::JoinHandle<()>>,
}

impl LightPartyApp {
    pub fn new_lightapp(exec: String, execargs: String) -> Self {
        let options = load_cfg();
        let input_devices = scan_input_devices(&options.pad_filter_type);
        // placeholder, user should define this
        Self {
            options,
            cur_page: MenuPage::Instances,
            infotext: String::new(),
            input_devices,
            instances: Vec::new(),
            instance_add_dev: None,
            // Placeholder, user should define this with program args
            game: Game::ExecRef(Executable::new(PathBuf::from(exec), execargs)),
            proton_versions: discover_proton_versions(),
            loading_msg: None,
            loading_since: None,
            task: None,
        }
    }
}

impl eframe::App for LightPartyApp {
    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        if !raw_input.focused || self.task.is_some() {
            return;
        }
        if self.cur_page == MenuPage::Instances {
            self.handle_devices_instance_menu();
        }
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::TopBottomPanel::top("menu_nav_panel").show(ctx, |ui| {
            if self.task.is_some() {
                ui.disable();
            }
            self.display_panel_top(ui);
        });

        if self.cur_page == MenuPage::Instances {
            egui::SidePanel::right("devices_panel")
                .resizable(false)
                .exact_width(180.0)
                .show(ctx, |ui| {
                    if self.task.is_some() {
                        ui.disable();
                    }
                    self.display_panel_right(ui, ctx);
                });
        }

        if self.cur_page == MenuPage::Settings {
            self.display_panel_bottom(ctx);
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            if self.task.is_some() {
                ui.disable();
            }
            match self.cur_page {
                MenuPage::Settings => self.display_page_settings(ui),
                MenuPage::Instances => self.display_page_instances(ui),
            }
        });

        if let Some(handle) = self.task.take() {
            if handle.is_finished() {
                let _ = handle.join();
                self.loading_since = None;
                self.loading_msg = None;
            } else {
                self.task = Some(handle);
            }
        }
        if let Some(start) = self.loading_since {
            if start.elapsed() > std::time::Duration::from_secs(60) {
                // Give up waiting after one minute
                self.loading_msg = Some("Operation timed out".to_string());
            }
        }
        if let Some(msg) = &self.loading_msg {
            egui::Area::new("loading".into())
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .interactable(false)
                .show(ctx, |ui| {
                    egui::Frame::NONE
                        .fill(egui::Color32::from_rgba_premultiplied(0, 0, 0, 192))
                        .corner_radius(6.0)
                        .inner_margin(egui::Margin::symmetric(16, 12))
                        .show(ui, |ui| {
                            ui.vertical_centered(|ui| {
                                ui.add(egui::widgets::Spinner::new().size(40.0));
                                ui.add_space(8.0);
                                ui.label(msg);
                            });
                        });
                });
        }
        if ctx.input(|input| input.focused) {
            ctx.request_repaint_after(std::time::Duration::from_millis(33)); // 30 fps
        }
    }
}

impl LightPartyApp {
    /// Refreshes the cached Proton installation list in the lightweight UI so
    /// users can pick newly installed compatibility tools without restarting.
    pub fn refresh_proton_versions(&mut self) {
        self.proton_versions = discover_proton_versions();
    }

    /// Mirrors the launcher Proton resolution used in the full UI so the light
    /// experience remains feature parity.
    pub fn selected_proton_install(&self) -> Option<&ProtonInstall> {
        let trimmed = self.options.proton_version.trim();
        if trimmed.is_empty() {
            return self
                .proton_versions
                .iter()
                .find(|install| install.matches("GE-Proton"));
        }

        self.proton_versions
            .iter()
            .find(|install| install.matches(trimmed))
    }

    /// Returns the label shown in the Proton combo box for the light UI.
    pub fn proton_dropdown_label(&self) -> String {
        if let Some(install) = self.selected_proton_install() {
            return install.display_label();
        }

        let trimmed = self.options.proton_version.trim();
        if trimmed.is_empty() {
            if self
                .proton_versions
                .iter()
                .any(|install| install.matches("GE-Proton"))
            {
                "Auto (GE-Proton)".to_string()
            } else {
                "Auto (GE-Proton missing)".to_string()
            }
        } else {
            format!("Custom: {trimmed}")
        }
    }

    pub fn spawn_task<F>(&mut self, msg: &str, f: F)
    where
        F: FnOnce() + Send + 'static,
    {
        self.loading_msg = Some(msg.to_string());
        self.loading_since = Some(std::time::Instant::now());
        self.task = Some(std::thread::spawn(f));
    }

    fn handle_devices_instance_menu(&mut self) {
        let mut i = 0;
        while i < self.input_devices.len() {
            if !self.input_devices[i].enabled() {
                i += 1;
                continue;
            }
            match self.input_devices[i].poll() {
                Some(PadButton::ABtn) | Some(PadButton::ZKey) | Some(PadButton::RightClick) => {
                    if self.input_devices[i].device_type() != DeviceType::Gamepad
                        && !self.options.kbm_support
                    {
                        continue;
                    }
                    if self.is_device_in_any_instance(i) {
                        continue;
                    }

                    match self.instance_add_dev {
                        Some(inst) => {
                            self.instance_add_dev = None;
                            self.instances[inst].devices.push(i);
                        }
                        None => {
                            self.instances.push(Instance {
                                devices: vec![i],
                                profname: String::new(),
                                profselection: 0,
                                width: 0,
                                height: 0,
                            });
                        }
                    }
                }
                Some(PadButton::BBtn) | Some(PadButton::XKey) => {
                    if self.instance_add_dev != None {
                        self.instance_add_dev = None;
                    } else if self.is_device_in_any_instance(i) {
                        self.remove_device(i);
                    }
                }
                Some(PadButton::YBtn) | Some(PadButton::AKey) => {
                    if self.instance_add_dev == None {
                        if let Some((instance, _)) = self.find_device_in_instance(i) {
                            self.instance_add_dev = Some(instance);
                        }
                    }
                }
                Some(PadButton::StartBtn) => {
                    if self.instances.len() > 0 && self.is_device_in_any_instance(i) {
                        self.prepare_game_launch();
                    }
                }
                _ => {}
            }
            i += 1;
        }
    }

    fn is_device_in_any_instance(&mut self, dev: usize) -> bool {
        for instance in &self.instances {
            if instance.devices.contains(&dev) {
                return true;
            }
        }
        false
    }

    fn find_device_in_instance(&mut self, dev: usize) -> Option<(usize, usize)> {
        for (i, instance) in self.instances.iter().enumerate() {
            for (d, device) in instance.devices.iter().enumerate() {
                if device == &dev {
                    return Some((i, d));
                }
            }
        }
        None
    }

    pub fn remove_device(&mut self, dev: usize) {
        if let Some((instance_index, device_index)) = self.find_device_in_instance(dev) {
            self.instances[instance_index].devices.remove(device_index);
            if self.instances[instance_index].devices.is_empty() {
                self.instances.remove(instance_index);
            }
        }
    }

    pub fn prepare_game_launch(&mut self) {
        set_instance_resolutions(&mut self.instances, &self.options);

        let game = self.game.to_owned();
        let instances = self.instances.clone();
        let dev_infos: Vec<DeviceInfo> = self.input_devices.iter().map(|p| p.info()).collect();

        let cfg = self.options.clone();
        let _ = save_cfg(&cfg);

        self.spawn_task(
            "Launching...\n\nDon't press any buttons or move any analog sticks or mice.",
            move || {
                sleep(std::time::Duration::from_secs(2));
                if let Err(err) = launch_game(&game, &dev_infos, &instances, &cfg) {
                    println!("{}", err);
                    msg("Launch Error", &format!("{err}"));
                }
                std::process::exit(0);
            },
        );
    }
}

impl LightPartyApp {
    pub fn display_panel_top(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.cur_page, MenuPage::Instances, "Play");
            ui.selectable_value(&mut self.cur_page, MenuPage::Settings, "Settings");

            if ui.button("🎮 Rescan").clicked() {
                self.instances.clear();
                self.input_devices = scan_input_devices(&self.options.pad_filter_type);
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("❌ Quit").clicked() {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
                ui.hyperlink_to(
                    format!("PartyDeck v{}", env!("CARGO_PKG_VERSION")),
                    "https://github.com/blckink/suckmydeck/releases",
                );
                ui.add(egui::Separator::default().vertical());
                ui.hyperlink_to(
                    "Open Source Licenses",
                    "https://github.com/blckink/suckmydeck/tree/main?tab=License-2-ov-file",
                );
            });
        });
    }

    pub fn display_panel_right(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        ui.add_space(6.0);

        ui.heading("Devices");
        ui.separator();

        for pad in self.input_devices.iter() {
            let mut dev_text = RichText::new(format!(
                "{} {} ({})",
                pad.emoji(),
                pad.fancyname(),
                pad.path()
            ))
            .small();

            if !pad.enabled() {
                dev_text = dev_text.weak();
            } else if pad.has_button_held() {
                dev_text = dev_text.strong();
            }

            ui.label(dev_text);
        }

        ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
            ui.link("Devices not being detected?").on_hover_ui(|ui| {
                ui.style_mut().interaction.selectable_labels = true;
                ui.label("Try adding your user to the `input` group.");
                ui.label("In a terminal, enter the following command:");
                ui.horizontal(|ui| {
                    ui.code("sudo usermod -aG input $USER");
                    if ui.button("📎").clicked() {
                        ctx.copy_text("sudo usermod -aG input $USER".to_string());
                    }
                });
            });
        });
    }

    pub fn display_panel_bottom(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("info_panel")
            .exact_height(50.0)
            .show(ctx, |ui| {
                if self.task.is_some() {
                    ui.disable();
                }
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.label(&self.infotext);
                });
            });
    }
}

impl LightPartyApp {
    pub fn display_page_settings(&mut self, ui: &mut Ui) {
        let force_sdl2_check = ui.checkbox(&mut self.options.force_sdl, "Force Steam Runtime SDL2");

        let enable_kwin_script_check = ui.checkbox(
            &mut self.options.enable_kwin_script,
            "Automatically resize/reposition instances",
        );

        let vertical_two_player_check = ui.checkbox(
            &mut self.options.vertical_two_player,
            "Vertical split for 2 players",
        );

        if force_sdl2_check.hovered() {
            self.infotext = "Forces games to use the version of SDL2 included in the Steam Runtime. Only works on native Linux games, may fix problematic game controller support (incorrect mappings) in some games, may break others. If unsure, leave this unchecked.".to_string();
        }

        if enable_kwin_script_check.hovered() {
            self.infotext = "Resizes/repositions instances to fit the screen using a KWin script. If unsure, leave this checked. If using a desktop environment or window manager other than KDE Plasma, uncheck this; note that you will need to manually resize and reposition the windows.".to_string();
        }

        if vertical_two_player_check.hovered() {
            self.infotext =
                "Splits two-player games vertically (side by side) instead of horizontally."
                    .to_string();
        }

        ui.horizontal(|ui| {
            let filter_label = ui.label("Controller filter");
            let r1 = ui.radio_value(
                &mut self.options.pad_filter_type,
                PadFilterType::All,
                "All controllers",
            );
            let r2 = ui.radio_value(
                &mut self.options.pad_filter_type,
                PadFilterType::NoSteamInput,
                "No Steam Input",
            );
            let r3 = ui.radio_value(
                &mut self.options.pad_filter_type,
                PadFilterType::OnlySteamInput,
                "Only Steam Input",
            );

            if filter_label.hovered() || r1.hovered() || r2.hovered() || r3.hovered() {
                self.infotext = "Select which controllers to filter out. If unsure, set this to \"No Steam Input\". If you use Steam Input to remap controllers, you may want to select \"Only Steam Input\", but be warned that this option is experimental and is known to break certain Proton games.".to_string();
            }

            if r1.clicked() || r2.clicked() || r3.clicked() {
                self.input_devices = scan_input_devices(&self.options.pad_filter_type);
            }
        });

        // Mirror the enhanced Proton selector introduced in the full UI.
        ui.horizontal(|ui| {
            let proton_ver_label = ui.label("Proton version");
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 4.0;
                ui.horizontal(|ui| {
                    let combo_response = egui::ComboBox::from_id_source("light_settings_proton_combo")
                        .selected_text(self.proton_dropdown_label())
                        .width(260.0)
                        .show_ui(ui, |combo_ui| {
                            combo_ui.selectable_value(
                                &mut self.options.proton_version,
                                String::new(),
                                "Auto (GE-Proton)",
                            );

                            if self.proton_versions.is_empty() {
                                combo_ui.label("No Proton builds detected");
                            } else {
                                for install in &self.proton_versions {
                                    combo_ui.selectable_value(
                                        &mut self.options.proton_version,
                                        install.id.clone(),
                                        install.display_label(),
                                    );
                                }
                            }

                            combo_ui.separator();
                            combo_ui.label(
                                "Select a build above or keep using the custom path below.",
                            );
                        })
                        .response;

                    let refresh_btn = ui.small_button("Refresh");
                    if refresh_btn.clicked() {
                        self.refresh_proton_versions();
                    }

                    if proton_ver_label.hovered()
                        || combo_response.hovered()
                        || refresh_btn.hovered()
                    {
                        self.infotext = "Choose an installed Proton build or refresh the list after installing a new compatibility tool. Keep the field below blank for the default GE-Proton.".to_string();
                    }
                });

                let proton_ver_editbox = ui.add(
                    egui::TextEdit::singleline(&mut self.options.proton_version)
                        .hint_text("GE-Proton or /path/to/proton"),
                );
                if proton_ver_editbox.hovered() {
                    self.infotext = "Enter a custom Proton identifier or absolute path. Leave empty to auto-select GE-Proton.".to_string();
                }
            });
        });

        let proton_separate_pfxs_check = ui.checkbox(
            &mut self.options.proton_separate_pfxs,
            "Run instances in separate Proton prefixes",
        );
        if proton_separate_pfxs_check.hovered() {
            self.infotext = "Runs each instance in its own Proton prefix. If unsure, leave this unchecked. This option will take up more space on the disk, but may also help with certain Proton-related issues such as only one instance of a game starting.".to_string();
        }

        let gamescope_lowres_fix_check = ui.checkbox(
            &mut self.options.gamescope_fix_lowres,
            "Automatically fix low resolution instances",
        );
        let gamescope_sdl_backend_check = ui.checkbox(
            &mut self.options.gamescope_sdl_backend,
            "Use SDL backend for Gamescope",
        );
        let kbm_support_check = ui.checkbox(
            &mut self.options.kbm_support,
            "Enable keyboard and mouse support through custom Gamescope",
        );

        if gamescope_lowres_fix_check.hovered() {
            self.infotext = "Many games have graphical problems or even crash when running at resolutions below 600p. If this is enabled, any instances below 600p will automatically be resized before launching.".to_string();
        }
        if gamescope_sdl_backend_check.hovered() {
            self.infotext = "Runs gamescope sessions using the SDL backend. If unsure, leave this checked. If gamescope sessions only show a black screen or give an error (especially on Nvidia + Wayland), try disabling this.".to_string();
        }
        if kbm_support_check.hovered() {
            self.infotext = "Runs a custom Gamescope build with support for holding keyboards and mice. If you want to use your own Gamescope installation, uncheck this.".to_string();
        }
    }

    pub fn display_page_instances(&mut self, ui: &mut Ui) {
        ui.heading("Players");
        ui.separator();

        ui.horizontal(|ui| {
            ui.add(
                egui::Image::new(egui::include_image!("../../res/BTN_SOUTH.png")).max_height(12.0),
            );
            ui.label("[Z]");
            ui.add(
                egui::Image::new(egui::include_image!("../../res/MOUSE_RIGHT.png"))
                    .max_height(12.0),
            );
            let add_text = match self.instance_add_dev {
                None => "Add New Instance",
                Some(i) => &format!("Add to Instance {}", i + 1),
            };
            ui.label(add_text);

            ui.add(egui::Separator::default().vertical());

            ui.add(
                egui::Image::new(egui::include_image!("../../res/BTN_EAST.png")).max_height(12.0),
            );
            ui.label("[X]");
            let remove_text = match self.instance_add_dev {
                None => "Remove",
                Some(_) => "Cancel",
            };
            ui.label(remove_text);

            ui.add(egui::Separator::default().vertical());

            if self.instances.len() > 0 && self.instance_add_dev == None {
                ui.add(
                    egui::Image::new(egui::include_image!("../../res/BTN_NORTH.png"))
                        .max_height(12.0),
                );
                ui.label("[A]");
                ui.label("Invite to Instance");
            }
        });

        ui.separator();

        let mut devices_to_remove = Vec::new();
        for (i, instance) in &mut self.instances.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                ui.label(format!("Instance {}", i + 1));

                if self.instance_add_dev == None {
                    if ui.button("➕ Invite New Device").clicked() {
                        self.instance_add_dev = Some(i);
                    }
                } else if self.instance_add_dev == Some(i) {
                    if ui.button("🗙 Cancel").clicked() {
                        self.instance_add_dev = None;
                    }
                    ui.label("Adding new device...");
                }
            });
            for &dev in instance.devices.iter() {
                let mut dev_text = RichText::new(format!(
                    "{} {}",
                    self.input_devices[dev].emoji(),
                    self.input_devices[dev].fancyname()
                ));

                if self.input_devices[dev].has_button_held() {
                    dev_text = dev_text.strong();
                }

                ui.horizontal(|ui| {
                    ui.label("  ");
                    ui.label(dev_text);
                    if ui.button("🗑").clicked() {
                        devices_to_remove.push(dev);
                    }
                });
            }
        }

        for d in devices_to_remove {
            self.remove_device(d);
        }

        if self.instances.len() > 0 {
            ui.separator();
            ui.horizontal(|ui| {
                ui.add(
                    egui::Image::new(egui::include_image!("../../res/BTN_START.png"))
                        .max_height(16.0),
                );
                ui.add(
                    egui::Image::new(egui::include_image!("../../res/BTN_START_PS5.png"))
                        .max_height(16.0),
                );
                if ui.button("Start").clicked() {
                    self.prepare_game_launch();
                }
            });
        }
    }
}
