use super::app::{MenuPage, PartyApp};
use crate::game::{Game::*, *};
use crate::input::*;
use crate::util::*;

use eframe::egui::RichText;
use eframe::egui::{self, Ui};

macro_rules! cur_game {
    ($self:expr) => {
        &$self.games[$self.selected_game]
    };
}

impl PartyApp {
    pub fn display_panel_top(&mut self, ui: &mut Ui) {
        // Render a wide navigation bar that mirrors Steam's controller-friendly layout.
        egui::Frame::new()
            .fill(ui.visuals().panel_fill)
            .inner_margin(egui::Margin::symmetric(20, 12))
            .show(ui, |bar_ui| {
                bar_ui.set_height(56.0);
                bar_ui.spacing_mut().item_spacing.x = 18.0;

                bar_ui.horizontal(|row| {
                    row.spacing_mut().item_spacing.x = 16.0;
                    row.label(
                        RichText::new("PartyDeck")
                            .heading()
                            .color(row.visuals().strong_text_color()),
                    );

                    row.separator();
                    row.add(
                        egui::Image::new(egui::include_image!("../../res/BTN_EAST.png"))
                            .max_height(16.0),
                    );
                    row.selectable_value(
                        &mut self.cur_page,
                        MenuPage::Home,
                        RichText::new("Home").size(22.0),
                    );
                    row.add(
                        egui::Image::new(egui::include_image!("../../res/BTN_NORTH.png"))
                            .max_height(16.0),
                    );
                    row.selectable_value(
                        &mut self.cur_page,
                        MenuPage::Settings,
                        RichText::new("Settings").size(22.0),
                    );
                    row.add(
                        egui::Image::new(egui::include_image!("../../res/BTN_WEST.png"))
                            .max_height(16.0),
                    );
                    if row
                        .selectable_value(
                            &mut self.cur_page,
                            MenuPage::Profiles,
                            RichText::new("Profiles").size(22.0),
                        )
                        .clicked()
                    {
                        self.profiles = scan_profiles(false);
                        self.cur_page = MenuPage::Profiles;
                    }

                    if row
                        .button(RichText::new("🎮 Rescan Controllers").size(18.0))
                        .clicked()
                    {
                        self.instances.clear();
                        self.input_devices = scan_input_devices(&self.options.pad_filter_type);
                    }

                    row.with_layout(egui::Layout::right_to_left(egui::Align::Center), |right| {
                        if right
                            .button(
                                RichText::new("Quit")
                                    .size(18.0)
                                    .color(right.visuals().strong_text_color()),
                            )
                            .clicked()
                        {
                            right.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                        let version_label = match self.needs_update {
                            true => format!("v{} (Update Available)", env!("CARGO_PKG_VERSION")),
                            false => format!("v{}", env!("CARGO_PKG_VERSION")),
                        };
                        right.hyperlink_to(
                            RichText::new(version_label).size(18.0),
                            "https://github.com/blckink/suckmydeck/releases",
                        );
                        right.add(egui::Separator::default().vertical());
                        right.hyperlink_to(
                            RichText::new("Open Source Licenses").size(18.0),
                            "https://github.com/blckink/suckmydeck/tree/main?tab=License-2-ov-file",
                        );
                    });
                });
            });
    }

    pub fn display_panel_left(&mut self, ui: &mut Ui) {
        ui.add_space(6.0);
        // Surface high-level library controls with larger buttons for couch play.
        ui.horizontal(|ui| {
            ui.heading(RichText::new("Library").size(26.0));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .button(RichText::new("Add").size(18.0))
                    .on_hover_text("Add a new handler or executable")
                    .clicked()
                {
                    self.prompt_add_game();
                }
                if ui
                    .button(RichText::new("Refresh").size(18.0))
                    .on_hover_text("Rescan your library for new content")
                    .clicked()
                {
                    self.reload_games();
                }
            });
        });
        ui.separator();
        egui::ScrollArea::vertical().show(ui, |ui| {
            self.panel_left_game_list(ui);
        });
    }

    pub fn display_panel_bottom(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("info_panel")
            .exact_height(100.0)
            .show(ctx, |ui| {
                if self.task.is_some() {
                    ui.disable();
                }
                match self.cur_page {
                    MenuPage::Game => {
                        match cur_game!(self){
                            Game::ExecRef(e) =>
                                self.infotext = format!("{}", e.path().display()),
                            Game::HandlerRef(h) =>
                                self.infotext = h.info.to_owned(),
                        }
                    }
                    MenuPage::Profiles =>
                        self.infotext = "Create profiles to persistently store game save data, settings, and stats.".to_string(),
                    _ => {}
                }
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.label(&self.infotext);
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

    pub fn panel_left_game_list(&mut self, ui: &mut Ui) {
        let mut refresh_games = false;

        for (i, game) in self.games.iter().enumerate() {
            // Draw each entry as a rich card so the selection reads clearly from the couch.
            let is_selected = self.selected_game == i;
            let (rect, response) = ui
                .allocate_exact_size(egui::vec2(ui.available_width(), 68.0), egui::Sense::click());

            let rounding = egui::CornerRadius::same(12);
            let visuals = ui.visuals();
            let bg_fill = if is_selected {
                visuals.selection.bg_fill
            } else if response.hovered() {
                visuals.widgets.hovered.bg_fill
            } else {
                visuals.widgets.inactive.bg_fill
            };
            let stroke_color = if is_selected {
                visuals.selection.stroke.color
            } else {
                visuals.widgets.inactive.bg_stroke.color
            };

            ui.painter().rect_filled(rect, rounding, bg_fill);
            ui.painter().rect_stroke(
                rect,
                rounding,
                egui::Stroke::new(1.0, stroke_color),
                egui::StrokeKind::Outside,
            );

            let mut row_ui = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(rect.shrink2(egui::vec2(12.0, 10.0)))
                    .layout(egui::Layout::left_to_right(egui::Align::Center)),
            );
            row_ui.add(
                egui::Image::new(game.icon())
                    .max_width(38.0)
                    .maintain_aspect_ratio(true),
            );
            row_ui.add_space(12.0);
            row_ui.vertical(|col| {
                col.spacing_mut().item_spacing.y = 4.0;
                col.label(RichText::new(game.name()).size(20.0).strong());
                match game {
                    HandlerRef(handler) => {
                        let platform = if handler.win { "Proton" } else { "Native" };
                        col.label(
                            RichText::new(format!("{} • by {}", platform, handler.author))
                                .size(16.0)
                                .color(col.visuals().weak_text_color()),
                        );
                    }
                    ExecRef(exec) => {
                        col.label(
                            RichText::new(exec.path().display().to_string())
                                .size(16.0)
                                .color(col.visuals().weak_text_color()),
                        );
                    }
                }
            });

            if response.clicked() {
                self.selected_game = i;
                self.cur_page = MenuPage::Game;
                self.pending_game_list_focus = true;
            }

            if self.pending_game_list_focus && is_selected {
                response.request_focus();
                response.scroll_to_me(Some(egui::Align::Center));
                self.pending_game_list_focus = false;
            }

            let popup_id = ui.make_persistent_id(format!("gamectx{}", i));
            egui::popup::popup_below_widget(
                ui,
                popup_id,
                &response,
                egui::popup::PopupCloseBehavior::CloseOnClick,
                |ui| {
                    if ui.button("Remove").clicked() {
                        if yesno(
                            "Remove game?",
                            &format!("Are you sure you want to remove {}?", game.name()),
                        ) {
                            if let Err(err) = remove_game(&self.games[i]) {
                                println!("Failed to remove game: {}", err);
                                msg("Error", &format!("Failed to remove game: {}", err));
                            }
                        }
                        refresh_games = true;
                    }
                    if let HandlerRef(h) = game {
                        if ui.button("Open Handler Folder").clicked() {
                            if let Err(_) = std::process::Command::new("sh")
                                .arg("-c")
                                .arg(format!("xdg-open {}", h.path_handler.display()))
                                .status()
                            {
                                msg("Error", "Couldn't open handler folder!");
                            }
                        }
                    }
                },
            );

            if response.secondary_clicked() {
                ui.memory_mut(|mem| mem.toggle_popup(popup_id));
            }
        }
        // Hacky workaround to avoid borrowing conflicts from inside the loop
        if refresh_games {
            self.reload_games();
        }
        if self.pending_game_list_focus {
            self.pending_game_list_focus = false;
        }
    }
}
