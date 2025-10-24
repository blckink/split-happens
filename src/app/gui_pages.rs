use super::app::PartyApp;
use super::config::*;
use crate::game::{Game::*, remove_game};
use crate::input::*;
use crate::paths::*;
use crate::util::*;

use dialog::DialogBox;
use eframe::egui::RichText;
use eframe::egui::{self, Ui};
use egui_extras::{Size, StripBuilder};

macro_rules! cur_game {
    ($self:expr) => {
        &$self.games[$self.selected_game]
    };
}

impl PartyApp {
    pub fn display_page_main(&mut self, ui: &mut Ui) {
        // Provide gentle breathing room between the navigation bar and the tile grid.
        ui.add_space(8.0);

        if self.games.is_empty() {
            ui.vertical_centered(|ui| {
                ui.add_space(48.0);
                ui.label("No games found yet. Use \"Add Game\" to import a handler or executable.");
            });
            return;
        }

        // Arrange the responsive tile grid with generous spacing so artwork
        // stays prominent on both desktop and Steam Deck screens.
        let mut refresh_games = false;
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
                // Cache the responsive column count so D-pad input snaps between rows.
                self.home_grid_columns = columns;

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
                            let game = self.games[index].to_owned();
                            let removal_game = game.to_owned();
                            let image_height = (tile_width * 9.0 / 16.0).clamp(160.0, 320.0);
                            let letterbox_pad = (image_height * 0.12).clamp(12.0, 24.0);
                            let hero_total_height = image_height + 2.0 * letterbox_pad;
                            let tile_height = hero_total_height + 72.0;

                            let (rect, response) = row_ui.allocate_exact_size(
                                egui::vec2(tile_width, tile_height),
                                egui::Sense::click(),
                            );

                            let is_selected = index == self.selected_game;
                            let visuals = row_ui.visuals();
                            let fill_color = if is_selected {
                                visuals.selection.bg_fill
                            } else if response.hovered() {
                                visuals.widgets.hovered.bg_fill
                            } else {
                                visuals.widgets.inactive.bg_fill
                            };
                            let stroke = if is_selected {
                                egui::Stroke::new(2.0, visuals.selection.stroke.color)
                            } else if response.hovered() {
                                visuals.widgets.hovered.bg_stroke
                            } else {
                                visuals.widgets.inactive.bg_stroke
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
                                .inner_margin(egui::Margin::symmetric(12, 12))
                                .show(&mut tile_ui, |tile_ui| {
                                    tile_ui.spacing_mut().item_spacing.y = 10.0;

                                    // Paint a letterboxed hero area so artwork never overlaps
                                    // neighboring tiles even when we shrink the window.
                                    let image_width = tile_ui.available_width();
                                    let hero_size = egui::vec2(image_width, hero_total_height);
                                    let (hero_rect, _) = tile_ui
                                        .allocate_exact_size(hero_size, egui::Sense::hover());

                                    let rounding = egui::CornerRadius::same(10);
                                    let letterbox_color = tile_ui.visuals().extreme_bg_color;
                                    tile_ui.painter().rect_filled(
                                        hero_rect,
                                        rounding,
                                        letterbox_color,
                                    );

                                    let image_rect =
                                        hero_rect.shrink2(egui::vec2(0.0, letterbox_pad));
                                    if let Some(hero_path) = game.hero_image_path() {
                                        let hero_widget = egui::Image::new(format!(
                                            "file://{}",
                                            hero_path.display()
                                        ))
                                        .fit_to_exact_size(image_rect.size())
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

                                    tile_ui.add_space(8.0);
                                    tile_ui.label(
                                        egui::RichText::new(game.name()).size(20.0).strong(),
                                    );
                                });

                            if response.clicked() {
                                self.open_instances_for(index);
                            }

                            // Offer a lightweight context menu so games can still be removed from the grid.
                            let popup_id =
                                row_ui.make_persistent_id(format!("home_tile_context_{index}"));
                            egui::popup::popup_below_widget(
                                row_ui,
                                popup_id,
                                &response,
                                egui::popup::PopupCloseBehavior::CloseOnClick,
                                |menu_ui| {
                                    if menu_ui.button("Remove").clicked() {
                                        if yesno(
                                            "Remove game?",
                                            &format!(
                                                "Are you sure you want to remove {}?",
                                                removal_game.name()
                                            ),
                                        ) {
                                            if let Err(err) = remove_game(&removal_game) {
                                                println!("Failed to remove game: {}", err);
                                                msg(
                                                    "Error",
                                                    &format!("Failed to remove game: {}", err),
                                                );
                                            }
                                            refresh_games = true;
                                        }
                                        menu_ui.close_menu();
                                    }
                                },
                            );
                            if response.secondary_clicked() {
                                row_ui.memory_mut(|mem| mem.toggle_popup(popup_id));
                            }

                            if self.pending_home_focus && is_selected {
                                // Pull focus to the active tile so controller actions work immediately.
                                response.request_focus();
                                response.scroll_to_me(Some(egui::Align::Center));
                                self.pending_home_focus = false;
                            }
                        }
                    });

                    if row + 1 < total_rows {
                        scroll_ui.add_space(tile_spacing);
                    }
                }
                if self.pending_home_focus {
                    // If no tile consumed the focus request, clear the flag to avoid repeated pings.
                    self.pending_home_focus = false;
                }
            });

        if refresh_games {
            self.reload_games();
        }
    }

    pub fn display_page_settings(&mut self, ui: &mut Ui) {
        self.infotext.clear();
        // Wrap the complete settings stack in a scroll area so long forms remain accessible.
        egui::ScrollArea::vertical()
            .auto_shrink([false; 2])
            .show(ui, |scroll| {
                scroll.heading("Settings");
                scroll.add_space(10.0);

                // Split the settings into two responsive columns so labels and
                // controls remain tidy even on narrower windows.
                StripBuilder::new(scroll)
                    .size(Size::relative(0.5).at_least(260.0))
                    .size(Size::remainder().at_least(260.0))
                    .horizontal(|mut strip| {
                        strip.cell(|left| {
                            left.spacing_mut().item_spacing.y = 10.0;
                            left.heading("General");
                            left.add_space(6.0);
                            self.display_settings_general(left);
                        });

                        strip.cell(|right| {
                            right.spacing_mut().item_spacing.y = 10.0;
                            right.heading("Gamescope");
                            right.add_space(6.0);
                            self.display_settings_gamescope(right);
                        });
                    });

                scroll.add_space(18.0);
                scroll.heading("Performance");
                scroll.add_space(6.0);
                self.display_settings_performance(scroll);

                scroll.add_space(16.0);
                // Keep persistence controls anchored at the bottom with a
                // consistent compact layout.
                scroll.with_layout(
                    egui::Layout::right_to_left(egui::Align::Center),
                    |actions| {
                        actions.spacing_mut().item_spacing.x = 10.0;
                        if actions.button("Restore Defaults").clicked() {
                            self.options = PartyConfig::default();
                            self.input_devices = scan_input_devices(&self.options.pad_filter_type);
                        }
                        if actions.button("Save Settings").clicked() {
                            if let Err(e) = save_cfg(&self.options) {
                                msg("Error", &format!("Couldn't save settings: {}", e));
                            }
                        }
                    },
                );
                scroll.separator();
            });
    }

    pub fn display_page_profiles(&mut self, ui: &mut Ui) {
        ui.heading("Profiles");
        ui.separator();
        egui::ScrollArea::vertical()
            .max_height(ui.available_height() - 16.0)
            .auto_shrink(false)
            .show(ui, |ui| {
                // Present each profile as a card with inline actions for controller clarity.
                let profile_names = self.profiles.clone();
                for profile in profile_names {
                    let frame = egui::Frame::new()
                        .fill(ui.visuals().widgets.inactive.bg_fill)
                        .stroke(egui::Stroke::new(
                            1.0,
                            ui.visuals().widgets.inactive.bg_stroke.color,
                        ))
                        .corner_radius(egui::CornerRadius::same(12))
                        .inner_margin(egui::Margin::symmetric(18, 12));

                    frame.show(ui, |row_ui| {
                        row_ui.horizontal(|row| {
                            let profile_name = profile.as_str();
                            row.label(RichText::new(profile_name).size(22.0).strong());
                            row.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |actions| {
                                    if actions.button(RichText::new("Open").size(18.0)).clicked() {
                                        if let Err(_) = std::process::Command::new("sh")
                                            .arg("-c")
                                            .arg(format!(
                                                "xdg-open {}/profiles/{}",
                                                PATH_PARTY.display(),
                                                profile_name
                                            ))
                                            .status()
                                        {
                                            msg("Error", "Couldn't open profile directory!");
                                        }
                                    }

                                    if actions.button(RichText::new("Rename").size(18.0)).clicked()
                                    {
                                        if let Some(new_name) =
                                            dialog::Input::new("Enter new name (alphanumeric)")
                                                .title("Rename Profile")
                                                .show()
                                                .expect("Could not display dialog box")
                                        {
                                            let trimmed = new_name.trim();
                                            if trimmed.is_empty()
                                                || !trimmed.chars().all(char::is_alphanumeric)
                                            {
                                                msg("Error", "Invalid name");
                                            } else if let Err(err) =
                                                rename_profile(profile_name, trimmed)
                                            {
                                                msg(
                                                    "Error",
                                                    &format!("Couldn't rename profile: {err}"),
                                                );
                                            } else {
                                                self.apply_local_profile_rename(
                                                    profile_name,
                                                    trimmed,
                                                );
                                                if let Err(err) = save_cfg(&self.options) {
                                                    msg(
                                                        "Error",
                                                        &format!(
                                                            "Couldn't persist profile settings: {}",
                                                            err
                                                        ),
                                                    );
                                                }
                                                self.profiles = scan_profiles(false);
                                            }
                                        }
                                    }
                                },
                            );
                        });
                    });

                    ui.add_space(8.0);
                }
            });
        if ui.button(RichText::new("New Profile").size(20.0)).clicked() {
            if let Some(name) = dialog::Input::new("Enter name (must be alphanumeric):")
                .title("New Profile")
                .show()
                .expect("Could not display dialog box")
            {
                if !name.is_empty() && name.chars().all(char::is_alphanumeric) {
                    if let Err(err) = create_profile(&name) {
                        msg("Error", &format!("Couldn't create profile: {err}"));
                    }
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

        // Track the exact instance/device pairs flagged for removal so shared
        // controllers can be detached cleanly from a single slot.
        let mut devices_to_remove: Vec<(usize, usize)> = Vec::new();
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
                    // Queue phantom entries that reference stale device indices
                    // produced while a pad disconnects mid-session.
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

        // Surface the connected device overview inline now that the sidebar is gone.
        ui.add_space(20.0);
        let devices_ctx = ui.ctx().clone();
        self.display_panel_right(ui, &devices_ctx);
    }

    pub fn display_settings_general(&mut self, ui: &mut Ui) {
        // Normalize spacing so each control lines up cleanly in the two-column layout.
        ui.spacing_mut().item_spacing.y = 12.0;
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

        // Group the controller filter radios so they wrap neatly on narrow windows.
        ui.group(|group| {
            group.spacing_mut().item_spacing.y = 6.0;
            let filter_label = group.label("Controller filter");
            group.horizontal_wrapped(|radios| {
                let r1 = radios.radio_value(
                    &mut self.options.pad_filter_type,
                    PadFilterType::All,
                    "All controllers",
                );
                let r2 = radios.radio_value(
                    &mut self.options.pad_filter_type,
                    PadFilterType::NoSteamInput,
                    "No Steam Input",
                );
                let r3 = radios.radio_value(
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
        });

        // Present the Proton selector as a combo box backed by the discovered
        // installations, followed by a manual override text field.
        // Wrap the Proton selector and manual override into a tidy stack for clarity.
        ui.group(|group| {
            group.spacing_mut().item_spacing.y = 8.0;
            let proton_ver_label = group.label("Proton version");
            let combo_response = egui::ComboBox::from_id_salt("settings_proton_combo")
                .selected_text(self.proton_dropdown_label())
                .width(220.0)
                .show_ui(group, |combo_ui| {
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
                    combo_ui.label("Select a build above or keep using the custom path below.");
                })
                .response;

            let refresh_btn = group.small_button("Refresh");
            if refresh_btn.clicked() {
                self.refresh_proton_versions();
            }

            if proton_ver_label.hovered() || combo_response.hovered() || refresh_btn.hovered() {
                self.infotext = "Choose an installed Proton build or refresh the list after installing a new compatibility tool. Keep the field below blank for the default GE-Proton.".to_string();
            }

            let proton_ver_editbox = group.add(
                egui::TextEdit::singleline(&mut self.options.proton_version)
                    .hint_text("GE-Proton or /path/to/proton"),
            );
            if proton_ver_editbox.hovered() {
                self.infotext = "Enter a custom Proton identifier or absolute path. Leave empty to auto-select GE-Proton.".to_string();
            }
        });

        let proton_separate_pfxs_check = ui.checkbox(
            &mut self.options.proton_separate_pfxs,
            "Run instances in separate Proton prefixes",
        );
        if proton_separate_pfxs_check.hovered() {
            self.infotext = "Runs each instance in its own Proton prefix. If unsure, leave this unchecked. This option will take up more space on the disk, but may also help with certain Proton-related issues such as only one instance of a game starting.".to_string();
        }

        ui.separator();

        // Keep destructive maintenance actions in a single row to avoid tall gaps.
        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |actions| {
            actions.spacing_mut().item_spacing.x = 10.0;
            if actions.button("Erase Proton Prefix").clicked() {
                if yesno(
                    "Erase Prefix?",
                    "This will erase the Wine prefix used by PartyDeck. This shouldn't erase profile/game-specific data, but exercise caution. Are you sure?",
                ) && PATH_PARTY.join("gamesyms").exists()
                {
                    if let Err(err) = std::fs::remove_dir_all(PATH_PARTY.join("pfx")) {
                        msg("Error", &format!("Couldn't erase pfx data: {}", err));
                    } else if let Err(err) = std::fs::create_dir_all(PATH_PARTY.join("pfx")) {
                        msg("Error", &format!("Couldn't re-create pfx directory: {}", err));
                    } else {
                        msg("Data Erased", "Proton prefix data successfully erased.");
                    }
                }
            }

            if actions.button("Erase Symlink Data").clicked() {
                if yesno(
                    "Erase Symlink Data?",
                    "This will erase all game symlink data. This shouldn't erase profile/game-specific data, but exercise caution. Are you sure?",
                ) && PATH_PARTY.join("gamesyms").exists()
                {
                    if let Err(err) = std::fs::remove_dir_all(PATH_PARTY.join("gamesyms")) {
                        msg("Error", &format!("Couldn't erase symlink data: {}", err));
                    } else if let Err(err) = std::fs::create_dir_all(PATH_PARTY.join("gamesyms")) {
                        msg("Error", &format!("Couldn't re-create symlink directory: {}", err));
                    } else {
                        msg("Data Erased", "Game symlink data successfully erased.");
                    }
                }
            }
        });

        // Surface shortcuts to important data locations with compact spacing.
        ui.with_layout(
            egui::Layout::left_to_right(egui::Align::Center),
            |actions| {
                actions.spacing_mut().item_spacing.x = 10.0;
                if actions.button("Open PartyDeck Data Folder").clicked() {
                    if let Err(_) = std::process::Command::new("sh")
                        .arg("-c")
                        .arg(format!("xdg-open {}/", PATH_PARTY.display()))
                        .status()
                    {
                        msg("Error", "Couldn't open PartyDeck Data Folder!");
                    }
                }
                if actions.button("Edit game paths").clicked() {
                    if let Err(_) = std::process::Command::new("sh")
                        .arg("-c")
                        .arg(format!("xdg-open {}/paths.json", PATH_PARTY.display(),))
                        .status()
                    {
                        msg("Error", "Couldn't open paths.json!");
                    }
                }
            },
        );
    }

    pub fn display_settings_performance(&mut self, ui: &mut Ui) {
        // Lay out the Steam Deck performance assists with ample spacing for readability.
        ui.spacing_mut().item_spacing.y = 12.0;

        let realtime_toggle = ui.checkbox(
            &mut self.options.performance_gamescope_rt,
            "Real-time scheduling for Gamescope",
        );
        if realtime_toggle.hovered() {
            self.infotext = "Requests gamescope's real-time compositor mode to reduce frame pacing spikes when two sessions share the GPU.".to_string();
        }

        let fps_limit_toggle = ui.checkbox(
            &mut self.options.performance_limit_40fps,
            "Limit Gamescope output to 40 FPS",
        );
        if fps_limit_toggle.hovered() {
            self.infotext = "Caps each window to 40 frames per second so both players stay within the Deck's thermal and power envelope.".to_string();
        }

        let proton_fsr_toggle = ui.checkbox(
            &mut self.options.performance_enable_proton_fsr,
            "Enable Proton FSR upscaling",
        );
        if proton_fsr_toggle.hovered() {
            self.infotext = "Turns on Proton's fullscreen FSR so Windows titles can render at lower resolutions while gamescope upscales the result.".to_string();
        }
    }

    pub fn display_settings_gamescope(&mut self, ui: &mut Ui) {
        ui.spacing_mut().item_spacing.y = 12.0;
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
