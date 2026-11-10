use eframe::egui::{self, Color32, Margin, Rounding, Stroke, Vec2, Visuals};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThemeMode {
    Dark,
    Light,
}

#[derive(Clone, Copy, Debug)]
pub struct AeroTheme {
    pub mode: ThemeMode,
    pub bg_base: Color32,
    pub bg_surface: Color32,
    pub bg_elevated: Color32,
    pub bg_card_hover: Color32,
    pub border_subtle: Color32,
    pub border_strong: Color32,
    pub accent: Color32,
    pub accent_hover: Color32,
    pub accent_muted: Color32,
    pub success: Color32,
    pub success_subtle: Color32,
    pub warning: Color32,
    pub warning_subtle: Color32,
    pub danger: Color32,
    pub danger_subtle: Color32,
    pub text_primary: Color32,
    pub text_secondary: Color32,
    pub text_muted: Color32,
}

impl AeroTheme {
    pub fn dark() -> Self {
        Self {
            mode: ThemeMode::Dark,
            bg_base: Color32::from_rgb(13, 17, 23),
            bg_surface: Color32::from_rgb(22, 27, 34),
            bg_elevated: Color32::from_rgb(33, 38, 45),
            bg_card_hover: Color32::from_rgb(44, 51, 60),
            border_subtle: Color32::from_rgb(48, 54, 61),
            border_strong: Color32::from_rgb(70, 78, 88),
            accent: Color32::from_rgb(56, 189, 248),        // Sky 400
            accent_hover: Color32::from_rgb(125, 211, 252),  // Sky 300
            accent_muted: Color32::from_rgba_premultiplied(56, 189, 248, 35),
            success: Color32::from_rgb(34, 197, 94),        // Emerald 500
            success_subtle: Color32::from_rgba_premultiplied(34, 197, 94, 30),
            warning: Color32::from_rgb(245, 158, 11),       // Amber 500
            warning_subtle: Color32::from_rgba_premultiplied(245, 158, 11, 30),
            danger: Color32::from_rgb(239, 68, 68),         // Rose 500
            danger_subtle: Color32::from_rgba_premultiplied(239, 68, 68, 30),
            text_primary: Color32::from_rgb(240, 246, 252),
            text_secondary: Color32::from_rgb(205, 215, 225),
            text_muted: Color32::from_rgb(155, 165, 175),
        }
    }

    pub fn light() -> Self {
        Self {
            mode: ThemeMode::Light,
            bg_base: Color32::from_rgb(248, 250, 252),      // Slate 50
            bg_surface: Color32::from_rgb(255, 255, 255),    // White
            bg_elevated: Color32::from_rgb(241, 245, 249),   // Slate 100
            bg_card_hover: Color32::from_rgb(226, 232, 240), // Slate 200
            border_subtle: Color32::from_rgb(226, 232, 240),
            border_strong: Color32::from_rgb(203, 213, 225),
            accent: Color32::from_rgb(2, 132, 199),         // Sky 600
            accent_hover: Color32::from_rgb(3, 105, 161),   // Sky 700
            accent_muted: Color32::from_rgba_premultiplied(2, 132, 199, 25),
            success: Color32::from_rgb(22, 163, 74),        // Emerald 600
            success_subtle: Color32::from_rgba_premultiplied(22, 163, 74, 25),
            warning: Color32::from_rgb(217, 119, 6),        // Amber 600
            warning_subtle: Color32::from_rgba_premultiplied(217, 119, 6, 25),
            danger: Color32::from_rgb(220, 38, 38),         // Red 600
            danger_subtle: Color32::from_rgba_premultiplied(220, 38, 38, 25),
            text_primary: Color32::from_rgb(15, 23, 42),     // Slate 900
            text_secondary: Color32::from_rgb(51, 65, 85),   // Slate 700
            text_muted: Color32::from_rgb(100, 116, 139),    // Slate 500
        }
    }

    pub fn apply(&self, ctx: &egui::Context) {
        let mut visuals = match self.mode {
            ThemeMode::Dark => Visuals::dark(),
            ThemeMode::Light => Visuals::light(),
        };

        visuals.window_fill = self.bg_base;
        visuals.panel_fill = self.bg_surface;
        visuals.extreme_bg_color = self.bg_base;
        visuals.faint_bg_color = self.bg_surface;
        visuals.code_bg_color = self.bg_elevated;
        visuals.hyperlink_color = self.accent;
        visuals.warn_fg_color = self.warning;
        visuals.error_fg_color = self.danger;

        // Leave override_text_color None so egui uses proper widget text colors
        visuals.override_text_color = None;

        visuals.window_rounding = Rounding::same(8.0);
        visuals.menu_rounding = Rounding::same(6.0);

        // Non-interactive surfaces (labels, plain text)
        visuals.widgets.noninteractive.bg_fill = self.bg_surface;
        visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, self.border_subtle);
        visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, self.text_primary);
        visuals.widgets.noninteractive.rounding = Rounding::same(4.0);

        // Inactive widgets (buttons, inputs)
        visuals.widgets.inactive.bg_fill = self.bg_elevated;
        visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, self.border_subtle);
        visuals.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, self.text_primary);
        visuals.widgets.inactive.rounding = Rounding::same(6.0);

        // Hovered widgets
        visuals.widgets.hovered.bg_fill = self.bg_card_hover;
        visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, self.accent);
        visuals.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, self.text_primary);
        visuals.widgets.hovered.rounding = Rounding::same(6.0);

        // Active / clicked / selected widgets
        // In dark mode, fg_stroke must ALWAYS be bright white, never black!
        visuals.widgets.active.bg_fill = match self.mode {
            ThemeMode::Dark => Color32::from_rgb(28, 64, 102),
            ThemeMode::Light => self.accent,
        };
        visuals.widgets.active.bg_stroke = Stroke::new(1.0_f32, self.accent_hover);
        visuals.widgets.active.fg_stroke = Stroke::new(
            1.0_f32,
            match self.mode {
                ThemeMode::Dark => Color32::WHITE,
                ThemeMode::Light => Color32::WHITE,
            },
        );
        visuals.widgets.active.rounding = Rounding::same(6.0);

        // Open state (dropdowns, combos)
        visuals.widgets.open.bg_fill = self.bg_elevated;
        visuals.widgets.open.bg_stroke = Stroke::new(1.0_f32, self.accent);
        visuals.widgets.open.fg_stroke = Stroke::new(1.0_f32, self.text_primary);
        visuals.widgets.open.rounding = Rounding::same(6.0);

        // Selections (selectable labels, highlighted text)
        visuals.selection.bg_fill = match self.mode {
            ThemeMode::Dark => Color32::from_rgb(30, 64, 110),
            ThemeMode::Light => Color32::from_rgb(186, 230, 253),
        };
        visuals.selection.stroke = Stroke::new(1.0_f32, self.accent);

        ctx.set_visuals(visuals);

        // Refine global spacing and typography scale (minimum 13px for body, 12px for small/badges)
        let mut style = (*ctx.style()).clone();
        style.spacing.item_spacing = Vec2::new(8.0, 8.0);
        style.spacing.window_margin = Margin::same(12.0);
        style.spacing.button_padding = Vec2::new(10.0, 6.0);
        style.spacing.combo_width = 240.0;
        style.spacing.indent = 16.0;

        style.text_styles.insert(
            egui::TextStyle::Body,
            egui::FontId::new(13.5, egui::FontFamily::Proportional),
        );
        style.text_styles.insert(
            egui::TextStyle::Button,
            egui::FontId::new(13.0, egui::FontFamily::Proportional),
        );
        style.text_styles.insert(
            egui::TextStyle::Small,
            egui::FontId::new(12.0, egui::FontFamily::Proportional),
        );
        style.text_styles.insert(
            egui::TextStyle::Heading,
            egui::FontId::new(16.5, egui::FontFamily::Proportional),
        );
        style.text_styles.insert(
            egui::TextStyle::Monospace,
            egui::FontId::new(12.5, egui::FontFamily::Monospace),
        );

        ctx.set_style(style);
    }
}

/// Font setup shared by the app and tests. Keeps egui's bundled fonts (Ubuntu Light,
/// Hack, Noto Emoji, emoji-icon-font); GUI text must only use glyphs they contain,
/// which `tests/gui_glyph_tests.rs` checks.
pub fn install_fonts(ctx: &egui::Context) {
    ctx.set_fonts(egui::FontDefinitions::default());
}
