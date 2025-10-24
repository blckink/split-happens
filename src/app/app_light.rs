use std::collections::HashMap;
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
    /// Mirror the repaint pacing knob from the full UI so both modes behave the
    /// same way on Steam Deck hardware.
    pub repaint_interval: std::time::Duration,
    /// Timestamp of the most recent device scan so Bluetooth pads pop up
    /// automatically without spamming the filesystem.
    pub last_input_scan: std::time::Instant,
}

impl LightPartyApp {
    pub fn new_lightapp(
        exec: String,
        execargs: String,
        repaint_interval: std::time::Duration,
    ) -> Self {
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
            repaint_interval,
            last_input_scan: std::time::Instant::now(),
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
        // Keep the lightweight UI in sync with new controllers just like the
        // full desktop experience.
        self.maybe_refresh_input_devices();

        egui::TopBottomPanel::top("menu_nav_panel").show(ctx, |ui| {
            if self.task.is_some() {
                ui.disable();
            }
            self.display_panel_top(ui);
        });

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
            ctx.request_repaint_after(self.repaint_interval);
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

                    match self.instance_add_dev {
                        Some(inst) => {
                            self.instance_add_dev = None;
                            if !self.instances[inst].devices.contains(&i) {
                                self.instances[inst].devices.push(i);
                            }
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
            self.remove_device_at(instance_index, device_index);
        }
    }

    /// Removes a device from a concrete instance slot so duplicate controllers
    /// can be released without disturbing other joins.
    pub fn remove_device_at(&mut self, instance_index: usize, device_index: usize) {
        if let Some(instance) = self.instances.get_mut(instance_index) {
            if device_index < instance.devices.len() {
                instance.devices.remove(device_index);
            }
        }
        self.prune_empty_instances();
    }

    /// Mirrors the rescan remapping logic from the full UI so controller
    /// indexes remain valid after the background device sync fires.
    fn sync_input_devices(&mut self) {
        let old_paths: Vec<String> = self
            .input_devices
            .iter()
            .map(|device| device.path().to_string())
            .collect();
        let new_devices = scan_input_devices(&self.options.pad_filter_type);
        let new_paths: Vec<String> = new_devices
            .iter()
            .map(|device| device.path().to_string())
            .collect();

        if new_paths == old_paths {
            return;
        }

        let mut path_to_index: HashMap<String, usize> = HashMap::new();
        for (idx, path) in new_paths.iter().enumerate() {
            path_to_index.insert(path.clone(), idx);
        }

        for instance in &mut self.instances {
            let mut remapped: Vec<usize> = Vec::with_capacity(instance.devices.len());
            for &old_index in &instance.devices {
                if let Some(old_path) = old_paths.get(old_index) {
                    if let Some(&new_index) = path_to_index.get(old_path) {
                        if !remapped.contains(&new_index) {
                            remapped.push(new_index);
                        }
                    }
                }
            }
            instance.devices = remapped;
        }

        self.prune_empty_instances();
        self.input_devices = new_devices;
    }

    /// Removes instance entries that no longer have active devices so the
    /// layout always remains tidy.
    fn prune_empty_instances(&mut self) {
        self.instances
            .retain(|instance| !instance.devices.is_empty());
    }

    /// Periodically rescan for controllers so Bluetooth pads appear without the
    /// manual rescan button in the light UI as well.
    fn maybe_refresh_input_devices(&mut self) {
        if self.last_input_scan.elapsed() < std::time::Duration::from_secs(2) {
            return;
        }
        self.last_input_scan = std::time::Instant::now();
        self.sync_input_devices();
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
        // Mirror the streamlined navigation styling from the full launcher.
        egui::Frame::new()
            .fill(ui.visuals().panel_fill)
            .inner_margin(egui::Margin::symmetric(16, 10))
            .show(ui, |bar_ui| {
                bar_ui.set_height(48.0);
                bar_ui.horizontal(|row| {
                    row.spacing_mut().item_spacing.x = 12.0;
                    row.label(
                        RichText::new("PartyDeck")
                            .heading()
                            .color(row.visuals().strong_text_color()),
                    );

                    row.separator();
                    row.selectable_value(
                        &mut self.cur_page,
                        MenuPage::Instances,
                        RichText::new("Play").size(16.0),
                    );
                    row.selectable_value(
                        &mut self.cur_page,
                        MenuPage::Settings,
                        RichText::new("Settings").size(16.0),
                    );

                    row.with_layout(
                        egui::Layout::right_to_left(egui::Align::Center),
                        |actions| {
                            if actions
                                .button(
                                    RichText::new("Quit")
                                        .size(14.0)
                                        .color(actions.visuals().strong_text_color()),
                                )
                                .clicked()
                            {
                                actions
                                    .ctx()
                                    .send_viewport_cmd(egui::ViewportCommand::Close);
                            }
                            actions.hyperlink_to(
                                RichText::new(format!("v{}", env!("CARGO_PKG_VERSION"))).size(14.0),
                                "https://github.com/blckink/suckmydeck/releases",
                            );
                            actions.add(egui::Separator::default().vertical());
                            if actions.button(RichText::new("Rescan").size(14.0)).clicked() {
                                self.instances.clear();
                                self.input_devices =
                                    scan_input_devices(&self.options.pad_filter_type);
                            }
                        },
                    );
                });
            });
    }

    pub fn display_panel_right(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        // Provide a compact device summary identical to the full UI treatment.
        ui.heading("Connected Devices");
        ui.separator();

        if self.input_devices.is_empty() {
            ui.label(RichText::new("No controllers detected.").weak());
        } else {
            for pad in self.input_devices.iter() {
                let mut dev_text = RichText::new(format!(
                    "{} {} ({})",
                    pad.emoji(),
                    pad.fancyname(),
                    pad.path()
                ))
                .size(14.0);

                if !pad.enabled() {
                    dev_text = dev_text.weak();
                } else if pad.has_button_held() {
                    dev_text = dev_text.strong();
                }

                ui.label(dev_text);
            }
        }

        ui.add_space(12.0);
        ui.link("Devices not being detected?")
            .on_hover_ui(|hover_ui| {
                hover_ui.style_mut().interaction.selectable_labels = true;
                hover_ui.label("Try adding your user to the `input` group.");
                hover_ui.label("In a terminal, enter the following command:");
                hover_ui.horizontal(|row| {
                    row.code("sudo usermod -aG input $USER");
                    if row.button("📎").clicked() {
                        ctx.copy_text("sudo usermod -aG input $USER".to_string());
                    }
                });
            });
    }

    pub fn display_page_settings(&mut self, ui: &mut Ui) {
        self.infotext.clear();
        // Keep all lightweight settings accessible in a single scrollable surface.
        egui::ScrollArea::vertical()
            .auto_shrink([false; 2])
            .show(ui, |scroll| {
                scroll.heading("Settings");
                scroll.add_space(12.0);

                // Mirror the consolidated settings structure from the desktop UI.
                scroll.heading("General");
                scroll.add_space(6.0);
                let force_sdl2_check =
                    scroll.checkbox(&mut self.options.force_sdl, "Force Steam Runtime SDL2");

                let enable_kwin_script_check = scroll.checkbox(
                    &mut self.options.enable_kwin_script,
                    "Automatically resize/reposition instances",
                );

                let vertical_two_player_check = scroll.checkbox(
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

                scroll.horizontal(|row| {
                    let filter_label = row.label("Controller filter");
                    let r1 = row.radio_value(
                        &mut self.options.pad_filter_type,
                        PadFilterType::All,
                        "All controllers",
                    );
                    let r2 = row.radio_value(
                        &mut self.options.pad_filter_type,
                        PadFilterType::NoSteamInput,
                        "No Steam Input",
                    );
                    let r3 = row.radio_value(
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

                scroll.add_space(12.0);
                // Group Proton-related options under a dedicated compatibility heading.
                scroll.heading("Compatibility");
                scroll.add_space(6.0);
                scroll.horizontal(|row| {
                    let proton_ver_label = row.label("Proton version");
                    row.vertical(|col| {
                        col.spacing_mut().item_spacing.y = 4.0;
                        col.horizontal(|inner| {
                            let combo_response = egui::ComboBox::from_id_source(
                                "light_settings_proton_combo",
                            )
                            .selected_text(self.proton_dropdown_label())
                            .width(260.0)
                            .show_ui(inner, |combo_ui| {
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

                            let refresh_btn = inner.small_button("Refresh");
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

                        let proton_ver_editbox = col.add(
                            egui::TextEdit::singleline(&mut self.options.proton_version)
                                .hint_text("GE-Proton or /path/to/proton"),
                        );
                        if proton_ver_editbox.hovered() {
                            self.infotext = "Enter a custom Proton identifier or absolute path. Leave empty to auto-select GE-Proton.".to_string();
                        }
                    });
                });

                let proton_separate_pfxs_check = scroll.checkbox(
                    &mut self.options.proton_separate_pfxs,
                    "Run instances in separate Proton prefixes",
                );
                if proton_separate_pfxs_check.hovered() {
                    self.infotext = "Runs each instance in its own Proton prefix. If unsure, leave this unchecked. This option will take up more space on the disk, but may also help with certain Proton-related issues such as only one instance of a game starting.".to_string();
                }

                scroll.add_space(20.0);
                // Follow with the Gamescope tweaks so everything lives on one page.
                scroll.heading("Gamescope");
                scroll.add_space(6.0);
                let gamescope_lowres_fix_check = scroll.checkbox(
                    &mut self.options.gamescope_fix_lowres,
                    "Automatically fix low resolution instances",
                );
                let gamescope_sdl_backend_check = scroll.checkbox(
                    &mut self.options.gamescope_sdl_backend,
                    "Use SDL backend for Gamescope",
                );
                let kbm_support_check = scroll.checkbox(
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

                scroll.add_space(20.0);
                // Allow the lightweight UI to persist changes without leaving the page.
                scroll.horizontal(|actions| {
                    if actions.button("Save Settings").clicked() {
                        if let Err(e) = save_cfg(&self.options) {
                            msg("Error", &format!("Couldn't save settings: {}", e));
                        }
                    }
                    if actions.button("Restore Defaults").clicked() {
                        self.options = PartyConfig::default();
                        self.input_devices = scan_input_devices(&self.options.pad_filter_type);
                    }
                });
                scroll.separator();
            });
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

        // Record precise instance/device pairs flagged for deletion so shared
        // controllers can be detached one slot at a time.
        let mut devices_to_remove: Vec<(usize, usize)> = Vec::new();
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
            for (device_slot, &dev) in instance.devices.iter().enumerate() {
                if let Some(device) = self.input_devices.get(dev) {
                    let mut dev_text =
                        RichText::new(format!("{} {}", device.emoji(), device.fancyname()));

                    if device.has_button_held() {
                        dev_text = dev_text.strong();
                    }

                    ui.horizontal(|ui| {
                        ui.label("  ");
                        ui.label(dev_text);
                        if ui.button("🗑").clicked() {
                            devices_to_remove.push((i, device_slot));
                        }
                    });
                } else {
                    // Remove orphaned indices that may linger during a hotplug.
                    devices_to_remove.push((i, device_slot));
                }
            }
        }

        for (instance_index, device_index) in devices_to_remove.into_iter().rev() {
            self.remove_device_at(instance_index, device_index);
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

        // Mirror the inline device overview from the full UI.
        ui.add_space(20.0);
        let devices_ctx = ui.ctx().clone();
        self.display_panel_right(ui, &devices_ctx);
    }
}
