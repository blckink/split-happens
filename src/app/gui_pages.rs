use super::app::{PartyApp, SettingsPage};
use super::config::*;
use crate::game::Game::*;
use crate::input::*;
use crate::paths::*;
use crate::util::*;

use dialog::DialogBox;
use eframe::egui::RichText;
use eframe::egui::{self, Ui};

macro_rules! cur_game {
    ($self:expr) => {
        &$self.games[$self.selected_game]
    };
}

impl PartyApp {
    pub fn display_page_main(&mut self, ui: &mut Ui) {
        // Surface quick actions above the grid so users can immediately add or
        // rescan handlers without diving into secondary menus.
        ui.horizontal(|ui| {
            ui.heading("Library");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Add Game").clicked() {
                    self.prompt_add_game();
                }
                if ui.button("Refresh").clicked() {
                    self.reload_games();
                }
            });
        });
        ui.separator();

        if self.games.is_empty() {
            ui.vertical_centered(|ui| {
                ui.add_space(48.0);
                ui.label("No games found yet. Use \"Add Game\" to import a handler or executable.");
            });
            return;
        }

        // Arrange the responsive tile grid with generous spacing so artwork
        // stays prominent on both desktop and Steam Deck screens.
        let tile_spacing = 16.0;
        let min_tile_width = 320.0;

        egui::ScrollArea::vertical()
            .auto_shrink([false; 2])
            .show(ui, |scroll_ui| {
                let mut available_width = scroll_ui.available_width();
                if available_width <= 0.0 {
                    available_width = min_tile_width;
                }

                let mut columns = ((available_width + tile_spacing)
                    / (min_tile_width + tile_spacing))
                    .floor() as usize;
                if columns == 0 {
                    columns = 1;
                }

                let tile_width = if columns == 1 {
                    available_width
                } else {
                    (available_width - tile_spacing * (columns as f32 - 1.0)) / columns as f32
                };

                let total_rows = (self.games.len() + columns - 1) / columns;

                for row in 0..total_rows {
                    let start = row * columns;
                    let end = usize::min(start + columns, self.games.len());

                    scroll_ui.horizontal(|row_ui| {
                        row_ui.set_width(available_width);
                        row_ui.spacing_mut().item_spacing.x = tile_spacing;

                        for index in start..end {
                            let game = &self.games[index];
                            let image_height = (tile_width * 9.0 / 16.0).clamp(160.0, 320.0);
                            let tile_height = image_height + 96.0;

                            let (rect, response) = row_ui.allocate_exact_size(
                                egui::vec2(tile_width, tile_height),
                                egui::Sense::click(),
                            );

                            let fill_color = if response.hovered() {
                                row_ui.visuals().widgets.hovered.bg_fill
                            } else {
                                row_ui.visuals().extreme_bg_color
                            };
                            let stroke = if response.hovered() {
                                row_ui.visuals().widgets.hovered.bg_stroke
                            } else {
                                row_ui.visuals().widgets.inactive.bg_stroke
                            };

                            let mut tile_ui = row_ui.new_child(
                                egui::UiBuilder::new()
                                    .max_rect(rect)
                                    .layout(egui::Layout::top_down(egui::Align::Center)),
                            );
                            egui::Frame::new()
                                .fill(fill_color)
                                .stroke(stroke)
                                .corner_radius(egui::CornerRadius::same(12))
                                .inner_margin(egui::Margin::symmetric(14, 14))
                                .show(&mut tile_ui, |tile_ui| {
                                    let image_width = tile_ui.available_width();
                                    let image_area = egui::vec2(image_width, image_height);
                                    let (image_rect, _) = tile_ui
                                        .allocate_exact_size(image_area, egui::Sense::hover());

                                    let rounding = egui::CornerRadius::same(10);
                                    tile_ui.painter().rect_filled(
                                        image_rect,
                                        rounding,
                                        tile_ui.visuals().widgets.inactive.bg_fill,
                                    );

                                    if let Some(hero_path) = game.hero_image_path() {
                                        let hero_widget = egui::Image::new(format!(
                                            "file://{}",
                                            hero_path.display()
                                        ))
                                        .fit_to_exact_size(image_area)
                                        .maintain_aspect_ratio(true);
                                        tile_ui.put(image_rect, hero_widget);
                                    } else {
                                        let icon_size = image_height.min(128.0);
                                        let icon_rect = egui::Rect::from_center_size(
                                            image_rect.center(),
                                            egui::vec2(icon_size, icon_size),
                                        );
                                        let icon_widget = egui::Image::new(game.icon())
                                            .fit_to_exact_size(icon_rect.size());
                                        tile_ui.put(icon_rect, icon_widget);
                                    }

                                    tile_ui.add_space(12.0);
                                    tile_ui.label(
                                        egui::RichText::new(game.name()).size(20.0).strong(),
                                    );

                                    match game {
                                        HandlerRef(handler) => {
                                            let platform =
                                                if handler.win { "Proton" } else { "Native" };
                                            tile_ui.label(
                                                egui::RichText::new(format!(
                                                    "{} • by {}",
                                                    platform, handler.author
                                                ))
                                                .color(tile_ui.visuals().weak_text_color()),
                                            );
                                            tile_ui.label(
                                                egui::RichText::new(format!(
                                                    "Version {}",
                                                    handler.version
                                                ))
                                                .small()
                                                .color(tile_ui.visuals().weak_text_color()),
                                            );
                                        }
                                        ExecRef(exec) => {
                                            tile_ui.label(
                                                egui::RichText::new(
                                                    exec.path().display().to_string(),
                                                )
                                                .small()
                                                .color(tile_ui.visuals().weak_text_color()),
                                            );
                                        }
                                    }
                                });

                            if response.clicked() {
                                self.open_instances_for(index);
                            }
                        }
                    });

                    if row + 1 < total_rows {
                        scroll_ui.add_space(tile_spacing);
                    }
                }
            });
    }

    pub fn display_page_settings(&mut self, ui: &mut Ui) {
        self.infotext.clear();
        ui.horizontal(|ui| {
            ui.heading("Settings");
            ui.selectable_value(&mut self.settings_page, SettingsPage::General, "General");
            ui.selectable_value(
                &mut self.settings_page,
                SettingsPage::Gamescope,
                "Gamescope",
            );
        });
        ui.separator();

        match self.settings_page {
            SettingsPage::General => self.display_settings_general(ui),
            SettingsPage::Gamescope => self.display_settings_gamescope(ui),
        }

        ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
            ui.horizontal(|ui| {
                if ui.button("Save Settings").clicked() {
                    if let Err(e) = save_cfg(&self.options) {
                        msg("Error", &format!("Couldn't save settings: {}", e));
                    }
                }
                if ui.button("Restore Defaults").clicked() {
                    self.options = PartyConfig::default();
                    self.input_devices = scan_input_devices(&self.options.pad_filter_type);
                }
            });
            ui.separator();
        });
    }

    pub fn display_page_profiles(&mut self, ui: &mut Ui) {
        ui.heading("Profiles");
        ui.separator();
        egui::ScrollArea::vertical()
            .max_height(ui.available_height() - 16.0)
            .auto_shrink(false)
            .show(ui, |ui| {
                for profile in &self.profiles {
                    if ui.selectable_value(&mut 0, 0, profile).clicked() {
                        if let Err(_) = std::process::Command::new("sh")
                            .arg("-c")
                            .arg(format!(
                                "xdg-open {}/profiles/{}",
                                PATH_PARTY.display(),
                                profile
                            ))
                            .status()
                        {
                            msg("Error", "Couldn't open profile directory!");
                        }
                    };
                }
            });
        if ui.button("New").clicked() {
            if let Some(name) = dialog::Input::new("Enter name (must be alphanumeric):")
                .title("New Profile")
                .show()
                .expect("Could not display dialog box")
            {
                if !name.is_empty() && name.chars().all(char::is_alphanumeric) {
                    create_profile(&name).unwrap();
                } else {
                    msg("Error", "Invalid name");
                }
            }
            self.profiles = scan_profiles(false);
        }
    }

    pub fn display_page_game(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.image(cur_game!(self).icon());
            ui.heading(cur_game!(self).name());
        });

        ui.separator();

        ui.horizontal(|ui| {
            ui.add(
                egui::Image::new(egui::include_image!("../../res/BTN_START.png")).max_height(16.0),
            );
            ui.add(
                egui::Image::new(egui::include_image!("../../res/BTN_START_PS5.png"))
                    .max_height(16.0),
            );
            if ui.button("Play").clicked() {
                self.open_instances_for(self.selected_game);
            }
            if let HandlerRef(h) = cur_game!(self) {
                ui.add(egui::Separator::default().vertical());
                if h.win {
                    ui.label(" Proton");
                } else {
                    ui.label("🐧 Native");
                }
                ui.add(egui::Separator::default().vertical());
                ui.label(format!("Author: {}", h.author));
                ui.add(egui::Separator::default().vertical());
                ui.label(format!("Version: {}", h.version));
            }
        });

        if let HandlerRef(h) = cur_game!(self) {
            egui::ScrollArea::horizontal()
                .max_width(f32::INFINITY)
                .show(ui, |ui| {
                    let available_height = ui.available_height();
                    ui.horizontal(|ui| {
                        for img in h.img_paths.iter() {
                            ui.add(
                                egui::Image::new(format!("file://{}", img.display()))
                                    .fit_to_exact_size(egui::vec2(
                                        available_height * 1.77,
                                        available_height,
                                    ))
                                    .maintain_aspect_ratio(true),
                            );
                        }
                    });
                });
        }
    }

    pub fn display_page_instances(&mut self, ui: &mut Ui) {
        ui.heading("Instances");
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

                if let HandlerRef(_) = cur_game!(self) {
                    ui.label("👤");
                    egui::ComboBox::from_id_salt(format!("{i}")).show_index(
                        ui,
                        &mut instance.profselection,
                        self.profiles.len(),
                        |i| self.profiles[i].clone(),
                    );
                }

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

    pub fn display_settings_general(&mut self, ui: &mut Ui) {
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

        // Present the Proton selector as a combo box backed by the discovered
        // installations, followed by a manual override text field.
        ui.horizontal(|ui| {
            let proton_ver_label = ui.label("Proton version");
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 4.0;
                ui.horizontal(|ui| {
                    let combo_response = egui::ComboBox::from_id_source("settings_proton_combo")
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

        ui.separator();

        ui.horizontal(|ui| {
        if ui.button("Erase Proton Prefix").clicked() {
            if yesno("Erase Prefix?", "This will erase the Wine prefix used by PartyDeck. This shouldn't erase profile/game-specific data, but exercise caution. Are you sure?") && PATH_PARTY.join("gamesyms").exists() {
                if let Err(err) = std::fs::remove_dir_all(PATH_PARTY.join("pfx")) {
                    msg("Error", &format!("Couldn't erase pfx data: {}", err));
                }
                else if let Err(err) = std::fs::create_dir_all(PATH_PARTY.join("pfx")) {
                    msg("Error", &format!("Couldn't re-create pfx directory: {}", err));
                }
                else {
                    msg("Data Erased", "Proton prefix data successfully erased.");
                }
            }
        }

        if ui.button("Erase Symlink Data").clicked() {
            if yesno("Erase Symlink Data?", "This will erase all game symlink data. This shouldn't erase profile/game-specific data, but exercise caution. Are you sure?") && PATH_PARTY.join("gamesyms").exists() {
                if let Err(err) = std::fs::remove_dir_all(PATH_PARTY.join("gamesyms")) {
                    msg("Error", &format!("Couldn't erase symlink data: {}", err));
                }
                else if let Err(err) = std::fs::create_dir_all(PATH_PARTY.join("gamesyms")) {
                    msg("Error", &format!("Couldn't re-create symlink directory: {}", err));
                }
                else {
                    msg("Data Erased", "Game symlink data successfully erased.");
                }
            }
        }
        });

        ui.horizontal(|ui| {
            if ui.button("Open PartyDeck Data Folder").clicked() {
                if let Err(_) = std::process::Command::new("sh")
                    .arg("-c")
                    .arg(format!("xdg-open {}/", PATH_PARTY.display()))
                    .status()
                {
                    msg("Error", "Couldn't open PartyDeck Data Folder!");
                }
            }
            if ui.button("Edit game paths").clicked() {
                if let Err(_) = std::process::Command::new("sh")
                    .arg("-c")
                    .arg(format!("xdg-open {}/paths.json", PATH_PARTY.display(),))
                    .status()
                {
                    msg("Error", "Couldn't open paths.json!");
                }
            }
        });
    }

    pub fn display_settings_gamescope(&mut self, ui: &mut Ui) {
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
}
