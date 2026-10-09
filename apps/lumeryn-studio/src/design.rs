use egui::{Color32, FontId, Stroke, TextStyle};

pub const BACKGROUND: Color32 = Color32::from_rgb(14, 17, 23);
pub const PANEL: Color32 = Color32::from_rgb(22, 27, 35);
pub const RAISED: Color32 = Color32::from_rgb(32, 40, 51);
pub const BORDER: Color32 = Color32::from_rgb(49, 60, 74);
pub const TEXT: Color32 = Color32::from_rgb(227, 234, 244);
pub const MUTED: Color32 = Color32::from_rgb(146, 160, 181);
pub const ACCENT: Color32 = Color32::from_rgb(124, 202, 208);
pub const CLIP: Color32 = Color32::from_rgb(46, 82, 112);
pub const WARNING: Color32 = Color32::from_rgb(241, 179, 91);

pub fn configure(ctx: &egui::Context) {
    let mut style = egui::Style {
        visuals: egui::Visuals::dark(),
        ..Default::default()
    };
    style.visuals.panel_fill = BACKGROUND;
    style.visuals.window_fill = PANEL;
    style.visuals.extreme_bg_color = BACKGROUND;
    style.visuals.faint_bg_color = RAISED;
    style.visuals.override_text_color = Some(TEXT);
    style.visuals.selection.bg_fill = ACCENT.gamma_multiply(0.28);
    style.visuals.selection.stroke = Stroke::new(1.0, ACCENT);
    style.visuals.widgets.noninteractive.bg_fill = PANEL;
    style.visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
    style.visuals.widgets.inactive.bg_fill = RAISED;
    style.visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, BORDER);
    style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(43, 61, 74);
    style.visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, ACCENT);
    style.visuals.widgets.active.bg_fill = Color32::from_rgb(44, 74, 89);
    style.visuals.widgets.active.bg_stroke = Stroke::new(1.0, ACCENT);
    style.spacing.item_spacing = egui::vec2(10.0, 8.0);
    style.spacing.button_padding = egui::vec2(12.0, 6.0);
    style.spacing.interact_size.y = 30.0;
    style
        .text_styles
        .insert(TextStyle::Heading, FontId::proportional(21.0));
    style
        .text_styles
        .insert(TextStyle::Body, FontId::proportional(14.0));
    style
        .text_styles
        .insert(TextStyle::Button, FontId::proportional(13.0));
    style
        .text_styles
        .insert(TextStyle::Small, FontId::proportional(11.0));
    style
        .text_styles
        .insert(TextStyle::Monospace, FontId::monospace(12.0));
    ctx.set_global_style(style);
}

pub fn section(ui: &mut egui::Ui, title: &str, subtitle: &str) {
    ui.add_space(6.0);
    ui.label(egui::RichText::new(title).strong().size(15.0));
    ui.label(egui::RichText::new(subtitle).color(MUTED).size(11.0));
    ui.add_space(8.0);
}
