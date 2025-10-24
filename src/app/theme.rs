use eframe::egui::{self, Color32, FontFamily, FontId, TextStyle};

/// Applies a Steam-inspired dark theme with larger typography so the UI feels at
/// home on televisions and docked Steam Deck sessions.
pub fn apply_split_happens_theme(ctx: &egui::Context) {
    // Embrace deep blues and muted panels to mirror Steam's visual language.
    let mut visuals = egui::Visuals::dark();
    visuals.window_fill = Color32::from_rgb(15, 22, 33);
    visuals.panel_fill = Color32::from_rgb(18, 26, 39);
    visuals.extreme_bg_color = Color32::from_rgb(13, 20, 30);
    visuals.hyperlink_color = Color32::from_rgb(102, 188, 255);

    // Accent interactive states with a saturated blue highlight that pops on TVs.
    let accent = Color32::from_rgb(54, 119, 201);
    visuals.widgets.inactive.bg_fill = Color32::from_rgb(26, 38, 55);
    visuals.widgets.inactive.bg_stroke.color = Color32::from_rgba_premultiplied(54, 119, 201, 64);
    visuals.widgets.hovered.bg_fill = Color32::from_rgb(34, 48, 69);
    visuals.widgets.hovered.bg_stroke.color = Color32::from_rgba_premultiplied(102, 188, 255, 160);
    visuals.widgets.active.bg_fill = Color32::from_rgb(47, 82, 129);
    visuals.selection.bg_fill = accent;
    visuals.selection.stroke.color = Color32::from_rgb(143, 202, 255);

    // Start with the current style so spacing tweaks build on upstream defaults.
    let mut style = (*ctx.style()).clone();
    style.visuals = visuals;
    style.spacing.item_spacing = egui::vec2(10.0, 10.0);
    style.spacing.button_padding = egui::vec2(10.0, 8.0);
    style.spacing.interact_size = egui::vec2(48.0, 26.0);

    // Size typography relative to the viewport width so the UI remains legible
    // when the window shrinks without looking oversized on larger monitors.
    let screen_width = ctx.screen_rect().width().max(640.0);
    let scale = (screen_width / 1280.0).clamp(0.85, 1.05);
    style.text_styles.insert(
        TextStyle::Heading,
        FontId::new(26.0 * scale, FontFamily::Proportional),
    );
    style.text_styles.insert(
        TextStyle::Body,
        FontId::new(18.0 * scale, FontFamily::Proportional),
    );
    style.text_styles.insert(
        TextStyle::Button,
        FontId::new(16.0 * scale, FontFamily::Proportional),
    );
    style.text_styles.insert(
        TextStyle::Small,
        FontId::new(14.0 * scale, FontFamily::Proportional),
    );
    style.text_styles.insert(
        TextStyle::Monospace,
        FontId::new(16.0 * scale, FontFamily::Monospace),
    );

    ctx.set_style(style);
}
