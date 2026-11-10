use eframe::egui::{self, Color32, Pos2, Rect, Response, RichText, Sense, Stroke, Ui, Vec2};
use super::theme::AeroTheme;

/// Renders a modern elevated card container with subtle borders and balanced padding
pub fn ui_card<R>(ui: &mut Ui, theme: &AeroTheme, add_contents: impl FnOnce(&mut Ui) -> R) -> R {
    egui::Frame::none()
        .fill(theme.bg_surface)
        .stroke(Stroke::new(1.0_f32, theme.border_subtle))
        .rounding(8.0)
        .inner_margin(12.0)
        .show(ui, add_contents)
        .inner
}

/// Renders a slightly higher elevated card (e.g. for selection drawers or popups)
pub fn ui_elevated_card<R>(
    ui: &mut Ui,
    theme: &AeroTheme,
    add_contents: impl FnOnce(&mut Ui) -> R,
) -> R {
    egui::Frame::none()
        .fill(theme.bg_elevated)
        .stroke(Stroke::new(1.0_f32, theme.border_strong))
        .rounding(8.0)
        .inner_margin(10.0)
        .show(ui, add_contents)
        .inner
}

/// Renders an elevated card with an accent border
pub fn ui_elevated_card_accent<R>(
    ui: &mut Ui,
    theme: &AeroTheme,
    add_contents: impl FnOnce(&mut Ui) -> R,
) -> R {
    egui::Frame::none()
        .fill(theme.bg_elevated)
        .stroke(Stroke::new(1.0_f32, theme.accent))
        .rounding(8.0)
        .inner_margin(10.0)
        .show(ui, add_contents)
        .inner
}

/// Renders a clickable card with hover and selection states
pub fn ui_clickable_card<R>(
    ui: &mut Ui,
    theme: &AeroTheme,
    selected: bool,
    add_contents: impl FnOnce(&mut Ui) -> R,
) -> (R, Response) {
    let (border, bg) = if selected {
        (theme.accent, theme.bg_card_hover)
    } else {
        (theme.border_subtle, theme.bg_elevated)
    };
    let inner_resp = egui::Frame::none()
        .fill(bg)
        .stroke(Stroke::new(1.0_f32, border))
        .rounding(6.0)
        .inner_margin(10.0)
        .show(ui, add_contents);
    let click_resp = ui.interact(inner_resp.response.rect, inner_resp.response.id, Sense::click());
    (inner_resp.inner, click_resp)
}

/// Renders a clickable row card that expands to full available width (ideal for vertical lists)
pub fn ui_clickable_row_card<R>(
    ui: &mut Ui,
    theme: &AeroTheme,
    selected: bool,
    add_contents: impl FnOnce(&mut Ui) -> R,
) -> (R, Response) {
    let (border, bg) = if selected {
        (theme.accent, theme.bg_card_hover)
    } else {
        (theme.border_subtle, theme.bg_elevated)
    };
    let inner_resp = egui::Frame::none()
        .fill(bg)
        .stroke(Stroke::new(1.0_f32, border))
        .rounding(6.0)
        .inner_margin(Vec2::new(10.0, 6.0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add_contents(ui)
        });
    let click_resp = ui.interact(inner_resp.response.rect, inner_resp.response.id, Sense::click());
    (inner_resp.inner, click_resp)
}

/// Renders a pill-shaped status badge with high-contrast text (>4.5:1 WCAG AA)
pub fn ui_status_badge(ui: &mut Ui, label: &str, active: bool, theme: &AeroTheme) {
    let (border, icon_color, text_color, icon) = if active {
        (theme.success, theme.success, theme.text_primary, "⏺")
    } else {
        (theme.border_subtle, theme.text_muted, theme.text_secondary, "○")
    };

    egui::Frame::none()
        .fill(theme.bg_elevated)
        .stroke(Stroke::new(1.0_f32, border))
        .rounding(4.0)
        .inner_margin(Vec2::new(8.0, 3.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(icon).color(icon_color).small().strong());
                ui.label(RichText::new(label).color(text_color).small().strong());
            });
        });
}

/// Renders a high-contrast tag pill (e.g. VID:PID, ICAO, category)
pub fn ui_tag_pill(ui: &mut Ui, label: &str, accent_color: Color32, theme: &AeroTheme) {
    egui::Frame::none()
        .fill(theme.bg_elevated)
        .stroke(Stroke::new(1.0_f32, theme.border_subtle))
        .rounding(4.0)
        .inner_margin(Vec2::new(7.0, 2.5))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("•").color(accent_color).small().strong());
                ui.label(RichText::new(label).color(theme.text_primary).small());
            });
        });
}

/// Renders a smooth animated bar for analog axes (bipolar centered -100%..+100% or unipolar 0%..100%)
pub fn ui_smooth_axis_bar(
    ui: &mut Ui,
    norm: f64,
    is_bipolar: bool,
    theme: &AeroTheme,
) -> Response {
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(ui.available_width().max(160.0), 16.0),
        Sense::hover(),
    );

    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        // Background track
        painter.rect_filled(rect, 4.0, theme.bg_elevated);
        painter.rect_stroke(rect, 4.0, Stroke::new(1.0_f32, theme.border_subtle));

        if is_bipolar {
            // Draw center zero line
            let center_x = rect.center().x;
            painter.line_segment(
                [
                    Pos2::new(center_x, rect.min.y),
                    Pos2::new(center_x, rect.max.y),
                ],
                Stroke::new(1.0_f32, theme.border_strong),
            );

            let half_w = rect.width() / 2.0;
            let offset = (norm.clamp(-1.0, 1.0) as f32) * (half_w - 2.0);
            let fill_rect = if offset >= 0.0 {
                Rect::from_min_max(
                    Pos2::new(center_x, rect.min.y + 2.0),
                    Pos2::new(center_x + offset, rect.max.y - 2.0),
                )
            } else {
                Rect::from_min_max(
                    Pos2::new(center_x + offset, rect.min.y + 2.0),
                    Pos2::new(center_x, rect.max.y - 2.0),
                )
            };
            painter.rect_filled(fill_rect, 2.0, theme.accent);
        } else {
            let fill_w = (norm.clamp(0.0, 1.0) as f32) * (rect.width() - 4.0);
            let fill_rect = Rect::from_min_max(
                Pos2::new(rect.min.x + 2.0, rect.min.y + 2.0),
                Pos2::new(rect.min.x + 2.0 + fill_w, rect.max.y - 2.0),
            );
            painter.rect_filled(fill_rect, 2.0, theme.accent);
        }
    }

    response
}
