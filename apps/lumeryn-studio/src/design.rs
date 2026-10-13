use egui::{Color32, FontId, Stroke, TextStyle};

pub const BACKGROUND: Color32 = Color32::from_rgb(14, 19, 25);
pub const PANEL: Color32 = Color32::from_rgb(23, 30, 38);
pub const RAISED: Color32 = Color32::from_rgb(32, 41, 51);
pub const BORDER: Color32 = Color32::from_rgb(46, 59, 71);
pub const TEXT: Color32 = Color32::from_rgb(241, 244, 249);
pub const MUTED: Color32 = Color32::from_rgb(173, 188, 205);
pub const ACCENT: Color32 = Color32::from_rgb(106, 247, 203);
pub const WARNING: Color32 = Color32::from_rgb(255, 193, 116);

pub fn configure(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "Lumeryn Inter".into(),
        egui::FontData::from_static(include_bytes!(
            "../../../assets/library/packs/inter.v4.1/Inter[opsz,wght].ttf"
        ))
        .into(),
    );
    fonts
        .families
        .entry(egui::FontFamily::Proportional)
        .or_default()
        .insert(0, "Lumeryn Inter".into());
    ctx.set_fonts(fonts);
    let mut style = egui::Style {
        visuals: egui::Visuals::dark(),
        ..Default::default()
    };
    style.visuals.handle_shape = egui::style::HandleShape::Circle;
    style.visuals.widgets.inactive.corner_radius = egui::CornerRadius::same(5);
    style.visuals.widgets.hovered.corner_radius = egui::CornerRadius::same(5);
    style.visuals.widgets.active.corner_radius = egui::CornerRadius::same(5);
    style.visuals.panel_fill = BACKGROUND;
    style.visuals.window_fill = PANEL;
    style.visuals.extreme_bg_color = BACKGROUND;
    style.visuals.faint_bg_color = RAISED;
    style.visuals.override_text_color = Some(TEXT);
    style.visuals.selection.bg_fill = ACCENT.gamma_multiply(0.2);
    style.visuals.selection.stroke = Stroke::new(1.5, ACCENT);
    style.visuals.widgets.noninteractive.bg_fill = PANEL;
    style.visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
    style.visuals.widgets.inactive.bg_fill = RAISED;
    style.visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, BORDER);
    style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(44, 60, 69);
    style.visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, ACCENT);
    style.visuals.widgets.active.bg_fill = Color32::from_rgb(39, 84, 74);
    style.visuals.widgets.active.bg_stroke = Stroke::new(1.0, ACCENT);
    style.spacing.item_spacing = egui::vec2(10.0, 8.0);
    style.spacing.button_padding = egui::vec2(14.0, 8.0);
    style.spacing.interact_size.y = 36.0;
    style.drag_value_text_style = TextStyle::Body;
    style
        .text_styles
        .insert(TextStyle::Heading, FontId::proportional(22.0));
    style
        .text_styles
        .insert(TextStyle::Body, FontId::proportional(15.0));
    style
        .text_styles
        .insert(TextStyle::Button, FontId::proportional(15.0));
    style
        .text_styles
        .insert(TextStyle::Small, FontId::proportional(12.0));
    style
        .text_styles
        .insert(TextStyle::Monospace, FontId::monospace(13.0));
    ctx.set_global_style(style);
}

pub fn section(ui: &mut egui::Ui, title: &str, subtitle: &str) {
    ui.heading(title);
    ui.label(egui::RichText::new(subtitle).color(MUTED).size(13.0));
    ui.add_space(10.0);
}
pub fn primary(ui: &mut egui::Ui, label: &str, enabled: bool, width: f32) -> egui::Response {
    let response = ui.add_enabled(
        enabled,
        egui::Button::new("")
            .fill(ACCENT)
            .corner_radius(5)
            .min_size(egui::vec2(width, 42.0)),
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, label));
    ui.painter().text(
        response.rect.center(),
        egui::Align2::CENTER_CENTER,
        label,
        FontId::proportional(15.0),
        if enabled { BACKGROUND } else { MUTED },
    );
    response
}

#[derive(Clone, Copy)]
pub enum Icon {
    Play,
    Pause,
    First,
    Previous,
    Next,
    Speaker,
    Muted,
    Expand,
    BackFive,
    ForwardFive,
    Media,
    Clip,
    Project,
    Library,
    Text,
    Undo,
    Redo,
    Close,
}
pub fn icon_button(ui: &mut egui::Ui, icon: Icon, label: &str) -> egui::Response {
    let response = ui.add(
        egui::Button::new("")
            .frame(false)
            .min_size(egui::vec2(36.0, 36.0)),
    );
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), label)
    });
    paint_icon(
        ui.painter(),
        response.rect.center(),
        icon,
        if !ui.is_enabled() {
            MUTED
        } else if response.hovered() {
            ACCENT
        } else {
            TEXT
        },
    );
    response.on_hover_text(label)
}

pub fn rail_button(ui: &mut egui::Ui, icon: Icon, label: &str, selected: bool) -> egui::Response {
    let response = ui.add(
        egui::Button::new("")
            .frame(false)
            .selected(selected)
            .min_size(egui::vec2(58.0, 66.0)),
    );
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), label)
    });
    if selected {
        ui.painter().rect_filled(response.rect, 5.0, RAISED);
        ui.painter().rect_filled(
            egui::Rect::from_min_size(
                response.rect.left_top() + egui::vec2(0.0, 10.0),
                egui::vec2(3.0, 40.0),
            ),
            2.0,
            ACCENT,
        );
    }
    paint_icon(
        ui.painter(),
        response.rect.center_top() + egui::vec2(0.0, 22.0),
        icon,
        if selected || response.hovered() {
            ACCENT
        } else {
            MUTED
        },
    );
    ui.painter().text(
        response.rect.center_bottom() - egui::vec2(0.0, 13.0),
        egui::Align2::CENTER_CENTER,
        label,
        FontId::proportional(11.0),
        if selected { TEXT } else { MUTED },
    );
    response.on_hover_text(label)
}

fn paint_icon(p: &egui::Painter, center: egui::Pos2, icon: Icon, color: Color32) {
    let r = egui::Rect::from_center_size(center, egui::vec2(18.0, 18.0));
    let stroke = Stroke::new(1.8, color);
    let triangle = |x: f32, sign: f32| {
        p.add(egui::Shape::convex_polygon(
            vec![
                egui::pos2(x, r.top()),
                egui::pos2(x + sign * 12.0, r.center().y),
                egui::pos2(x, r.bottom()),
            ],
            stroke.color,
            Stroke::NONE,
        ))
    };
    match icon {
        Icon::Play => {
            triangle(r.left() + 3.0, 1.0);
        }
        Icon::Pause => {
            for x in [r.left() + 3.0, r.right() - 6.0] {
                p.rect_filled(
                    egui::Rect::from_min_size(egui::pos2(x, r.top()), egui::vec2(4.0, 18.0)),
                    1.0,
                    stroke.color,
                );
            }
        }
        Icon::First => {
            p.line_segment([r.left_top(), r.left_bottom()], stroke);
            for offset in [8.0, 17.0] {
                p.add(egui::Shape::convex_polygon(
                    vec![
                        r.left_top() + egui::vec2(offset, 2.0),
                        r.left_center() + egui::vec2(offset - 6.0, 0.0),
                        r.left_bottom() + egui::vec2(offset, -2.0),
                    ],
                    stroke.color,
                    Stroke::NONE,
                ));
            }
        }
        Icon::Previous => {
            triangle(r.right() - 2.0, -1.0);
            p.line_segment([r.left_top(), r.left_bottom()], stroke);
        }
        Icon::Next => {
            triangle(r.left() + 2.0, 1.0);
            p.line_segment([r.right_top(), r.right_bottom()], stroke);
        }
        Icon::Speaker | Icon::Muted => {
            p.add(egui::Shape::convex_polygon(
                vec![
                    r.left_center() + egui::vec2(0.0, -3.0),
                    r.center() + egui::vec2(-3.0, -3.0),
                    r.center() + egui::vec2(2.0, -8.0),
                    r.center() + egui::vec2(2.0, 8.0),
                    r.center() + egui::vec2(-3.0, 3.0),
                    r.left_center() + egui::vec2(0.0, 3.0),
                ],
                stroke.color,
                Stroke::NONE,
            ));
            if matches!(icon, Icon::Muted) {
                p.line_segment([r.left_bottom(), r.right_top()], stroke);
            } else {
                p.line_segment(
                    [r.center() + egui::vec2(6.0, -6.0), r.right_center()],
                    stroke,
                );
                p.line_segment(
                    [r.right_center(), r.center() + egui::vec2(6.0, 6.0)],
                    stroke,
                );
            }
        }
        Icon::Expand => {
            for (corner, dx, dy) in [
                (r.left_top(), 1.0, 1.0),
                (r.right_top(), -1.0, 1.0),
                (r.left_bottom(), 1.0, -1.0),
                (r.right_bottom(), -1.0, -1.0),
            ] {
                p.line_segment([corner, corner + egui::vec2(dx * 6.0, 0.0)], stroke);
                p.line_segment([corner, corner + egui::vec2(0.0, dy * 6.0)], stroke);
            }
        }
        Icon::BackFive | Icon::ForwardFive => {
            let sign = if matches!(icon, Icon::BackFive) {
                -1.0
            } else {
                1.0
            };
            let points = (0..=16)
                .map(|step| {
                    let angle = -std::f32::consts::FRAC_PI_2
                        + step as f32 * std::f32::consts::PI * 1.65 / 16.0;
                    center + egui::vec2(sign * angle.cos() * 9.0, angle.sin() * 9.0)
                })
                .collect();
            p.add(egui::Shape::line(points, stroke));
            p.add(egui::Shape::convex_polygon(
                vec![
                    center + egui::vec2(-sign * 4.0, -9.0),
                    center + egui::vec2(sign * 2.0, -13.0),
                    center + egui::vec2(sign * 2.0, -5.0),
                ],
                color,
                Stroke::NONE,
            ));
            p.text(
                center,
                egui::Align2::CENTER_CENTER,
                "5",
                FontId::proportional(10.0),
                color,
            );
        }
        Icon::Media => {
            p.add(egui::Shape::line(
                vec![
                    r.left_bottom(),
                    r.left_top(),
                    r.center_top(),
                    r.center_top() + egui::vec2(2.0, 4.0),
                    r.right_top() + egui::vec2(0.0, 4.0),
                    r.right_bottom(),
                    r.left_bottom(),
                ],
                stroke,
            ));
        }
        Icon::Clip => {
            p.rect_stroke(r, 2.0, stroke, egui::StrokeKind::Inside);
            p.line_segment(
                [
                    r.left_top() + egui::vec2(4.0, 0.0),
                    r.left_bottom() + egui::vec2(4.0, 0.0),
                ],
                stroke,
            );
            p.line_segment(
                [
                    r.right_top() - egui::vec2(4.0, 0.0),
                    r.right_bottom() - egui::vec2(4.0, 0.0),
                ],
                stroke,
            );
            for y in [r.top() + 5.0, r.bottom() - 5.0] {
                p.line_segment([egui::pos2(r.left(), y), egui::pos2(r.right(), y)], stroke);
            }
        }
        Icon::Undo | Icon::Redo => {
            let sign = if matches!(icon, Icon::Undo) {
                1.0
            } else {
                -1.0
            };
            let point = |x: f32, y: f32| center + egui::vec2(sign * x, y);
            p.add(egui::Shape::line(
                vec![
                    point(-7.0, -3.0),
                    point(1.0, -3.0),
                    point(6.0, 0.0),
                    point(6.0, 6.0),
                ],
                stroke,
            ));
            p.line_segment([point(-7.0, -3.0), point(-2.0, -8.0)], stroke);
            p.line_segment([point(-7.0, -3.0), point(-2.0, 2.0)], stroke);
        }
        Icon::Library => {
            for x in [r.left(), r.left() + 10.0] {
                for y in [r.top(), r.top() + 10.0] {
                    p.rect_stroke(
                        egui::Rect::from_min_size(egui::pos2(x, y), egui::vec2(7.0, 7.0)),
                        1.0,
                        stroke,
                        egui::StrokeKind::Inside,
                    );
                }
            }
        }
        Icon::Project => {
            for (y, offset) in [
                (r.top() + 2.0, 5.0),
                (r.center().y, 13.0),
                (r.bottom() - 2.0, 8.0),
            ] {
                p.line_segment([egui::pos2(r.left(), y), egui::pos2(r.right(), y)], stroke);
                p.circle_filled(egui::pos2(r.left() + offset, y), 2.5, color);
            }
        }
        Icon::Close => {
            p.line_segment([r.left_top(), r.right_bottom()], stroke);
            p.line_segment([r.right_top(), r.left_bottom()], stroke);
        }
        Icon::Text => {
            p.line_segment([r.left_top(), r.right_top()], stroke);
            p.line_segment([r.center_top(), r.center_bottom()], stroke);
            p.line_segment(
                [
                    r.center_bottom() - egui::vec2(4.0, 0.0),
                    r.center_bottom() + egui::vec2(4.0, 0.0),
                ],
                stroke,
            );
        }
    }
}

pub fn logo(ui: &mut egui::Ui) {
    let (r, _) = ui.allocate_exact_size(egui::vec2(26.0, 28.0), egui::Sense::hover());
    ui.painter().add(egui::Shape::convex_polygon(
        vec![
            r.left_bottom(),
            r.center_top(),
            r.center() + egui::vec2(3.0, -4.0),
            r.left_bottom() + egui::vec2(10.0, 0.0),
        ],
        ACCENT,
        Stroke::NONE,
    ));
    ui.painter().add(egui::Shape::convex_polygon(
        vec![
            r.center() + egui::vec2(4.0, 1.0),
            r.right_bottom(),
            r.center_bottom(),
        ],
        ACCENT,
        Stroke::NONE,
    ));
}
