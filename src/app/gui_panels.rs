use super::app::{MenuPage, PartyApp};
use crate::input::*;
use crate::util::*;

use eframe::egui::RichText;
use eframe::egui::{self, Ui};

impl PartyApp {
    pub fn display_panel_top(&mut self, ui: &mut Ui) {
        // Render a condensed navigation bar with the primary sections on the left and
        // contextual actions (add game, rescan, quit) aligned to the right.
        egui::Frame::new()
            .fill(ui.visuals().panel_fill)
            .inner_margin(egui::Margin::symmetric(20, 12))
            .show(ui, |bar_ui| {
                bar_ui.set_height(52.0);
                bar_ui.spacing_mut().item_spacing.x = 16.0;

                bar_ui.horizontal(|row| {
                    row.spacing_mut().item_spacing.x = 14.0;
                    row.label(
                        RichText::new("PartyDeck")
                            .heading()
                            .color(row.visuals().strong_text_color()),
                    );

                    row.separator();
                    row.selectable_value(
                        &mut self.cur_page,
                        MenuPage::Home,
                        RichText::new("Home").size(18.0),
                    );
                    row.selectable_value(
                        &mut self.cur_page,
                        MenuPage::Settings,
                        RichText::new("Settings").size(18.0),
                    );
                    if row
                        .selectable_value(
                            &mut self.cur_page,
                            MenuPage::Profiles,
                            RichText::new("Profiles").size(18.0),
                        )
                        .clicked()
                    {
                        self.profiles = scan_profiles(false);
                        self.cur_page = MenuPage::Profiles;
                    }

                    row.with_layout(egui::Layout::right_to_left(egui::Align::Center), |right| {
                        if right
                            .button(
                                RichText::new("Quit")
                                    .size(16.0)
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
                            RichText::new(version_label).size(16.0),
                            "https://github.com/blckink/suckmydeck/releases",
                        );
                        right.add(egui::Separator::default().vertical());
                        if right.button(RichText::new("Add Game").size(16.0)).clicked() {
                            self.prompt_add_game();
                        }
                        if right
                            .button(RichText::new("Rescan Controllers").size(16.0))
                            .clicked()
                        {
                            self.instances.clear();
                            self.input_devices = scan_input_devices(&self.options.pad_filter_type);
                        }
                    });
                });
            });
    }

    pub fn display_panel_right(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        // Present the live device list inline so users can double-check controllers at a glance.
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
}
