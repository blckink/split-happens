use super::app::{MenuPage, PartyApp};
use crate::input::*;
use crate::util::*;

use eframe::egui::output::OpenUrl;
use eframe::egui::RichText;
use eframe::egui::{self, TextWrapMode, Ui};
use egui_extras::{Size, StripBuilder};

impl PartyApp {
    pub fn display_panel_top(&mut self, ui: &mut Ui) {
        // Render a condensed navigation bar with primary sections on the left and
        // contextual actions (add game, rescan, quit) aligned to the right. The
        // layout uses a strip builder so both halves can wrap responsively when
        // the window becomes narrow.
        egui::Frame::new()
            .fill(ui.visuals().panel_fill)
            .inner_margin(egui::Margin::symmetric(12, 8))
            .show(ui, |bar_ui| {
                bar_ui.set_height(44.0);

                // Shared helper to render consistently styled navigation buttons.
                fn styled_nav_button(
                    ui: &mut Ui,
                    label: impl Into<String>,
                    selected: bool,
                ) -> egui::Response {
                    let text = RichText::new(label.into()).size(15.0);
                    let visuals = ui.visuals().clone();
                    let mut button = egui::Button::new(text)
                        .min_size(egui::vec2(0.0, 28.0))
                        .corner_radius(egui::CornerRadius::same(6));

                    if selected {
                        button = button
                            .fill(visuals.selection.bg_fill)
                            .stroke(visuals.selection.stroke);
                    } else {
                        button = button
                            .fill(visuals.widgets.inactive.bg_fill)
                            .stroke(visuals.widgets.inactive.bg_stroke);
                    }

                    ui.add(button)
                }

                StripBuilder::new(bar_ui)
                    .size(Size::relative(0.55).at_least(220.0))
                    .size(Size::remainder().at_least(200.0))
                    .horizontal(|mut strip| {
                        strip.cell(|left| {
                            left.set_height(36.0);
                            left.spacing_mut().item_spacing.x = 8.0;
                            left.horizontal_wrapped(|nav| {
                                nav.label(
                                    RichText::new("Split Happens")
                                        .heading()
                                        .size(20.0)
                                        .color(nav.visuals().strong_text_color()),
                                );
                                nav.separator();

                                let home_button =
                                    styled_nav_button(nav, "Home", self.cur_page == MenuPage::Home);
                                if self.pending_nav_focus && self.cur_page == MenuPage::Home {
                                    // Hand focus back to the highlighted header button so
                                    // controller presses activate it immediately.
                                    home_button.request_focus();
                                    self.pending_nav_focus = false;
                                }
                                if home_button.clicked() {
                                    self.cur_page = MenuPage::Home;
                                    self.nav_in_focus = false;
                                    self.pending_nav_focus = false;
                                }

                                let settings_button = styled_nav_button(
                                    nav,
                                    "Settings",
                                    self.cur_page == MenuPage::Settings,
                                );
                                if self.pending_nav_focus && self.cur_page == MenuPage::Settings {
                                    settings_button.request_focus();
                                    self.pending_nav_focus = false;
                                }
                                if settings_button.clicked() {
                                    self.cur_page = MenuPage::Settings;
                                    self.nav_in_focus = false;
                                    self.pending_nav_focus = false;
                                }

                                let profiles_button = styled_nav_button(
                                    nav,
                                    "Profiles",
                                    self.cur_page == MenuPage::Profiles,
                                );
                                if self.pending_nav_focus && self.cur_page == MenuPage::Profiles {
                                    profiles_button.request_focus();
                                    self.pending_nav_focus = false;
                                }
                                if profiles_button.clicked() {
                                    self.profiles = scan_profiles(false);
                                    self.cur_page = MenuPage::Profiles;
                                    self.nav_in_focus = false;
                                    self.pending_nav_focus = false;
                                }
                            });
                        });

                        strip.cell(|right| {
                            right.set_height(36.0);
                            right.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |actions| {
                                    actions.spacing_mut().item_spacing.x = 8.0;
                                    actions.scope(|scope| {
                                        let ui = scope;
                                        ui.style_mut().wrap_mode = Some(TextWrapMode::Extend);

                                        if styled_nav_button(ui, "Quit", false).clicked() {
                                            ui.ctx()
                                                .send_viewport_cmd(egui::ViewportCommand::Close);
                                        }

                                        let version_label = if self.needs_update {
                                            format!("v{} • Update", env!("CARGO_PKG_VERSION"))
                                        } else {
                                            format!("v{}", env!("CARGO_PKG_VERSION"))
                                        };
                                        if styled_nav_button(ui, version_label, false).clicked() {
                                            ui.ctx().open_url(OpenUrl::new_tab(
                                                "https://github.com/blckink/suckmydeck/releases",
                                            ));
                                        }

                                        if styled_nav_button(ui, "Add Game", false).clicked() {
                                            self.prompt_add_game();
                                        }
                                        if styled_nav_button(ui, "Rescan Controllers", false)
                                            .clicked()
                                        {
                                            self.instances.clear();
                                            self.input_devices =
                                                scan_input_devices(&self.options.pad_filter_type);
                                        }
                                    });
                                },
                            );
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
