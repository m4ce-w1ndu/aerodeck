//! Calibrated lever editor (SPAD.neXt `EventActionRangedAxis`), for throttle quadrants with an
//! idle detent such as the Thrustmaster TCA: move the lever, click "Set full" / "Set idle",
//! pick the engines, save.

use eframe::egui::{self, Color32, Pos2, Rect, RichText, Sense, Stroke, Ui, Vec2};

use crate::gui::app::LiveAxisState;
use crate::gui::theme::AeroTheme;
use crate::gui::widgets::{ui_card, ui_tag_pill};
use crate::profile::lever::{lever_outputs, lever_position};
use crate::profile::schema::{EventActionItem, EventActionRangedAxis, SpadProfile};

pub const LEVER_KINDS: [(&str, &str); 5] = [
    ("LEVER_NOREVERSE_THROTTLE", "Throttle"),
    ("LEVER_MIXTURE", "Mixture"),
    ("LEVER_PROPELLER", "Propeller"),
    ("LEVER_FLAPS", "Flaps"),
    ("LEVER_SPOILER", "Speedbrake"),
];

/// Lever being edited for one input.
#[derive(Debug, Clone)]
pub struct LeverDraft {
    pub input: String,
    pub action: EventActionRangedAxis,
}

impl LeverDraft {
    fn new(input: &str, existing: Option<EventActionRangedAxis>, live: Option<&LiveAxisState>) -> Self {
        let action = existing.unwrap_or_else(|| {
            // Thrustmaster TCA convention (as in SPAD.neXt TCA profiles): lever forward reads
            // the minimum raw value, the idle detent sits about 3/4 of the way back.
            let (lo, hi) = live.map(|l| (l.min, l.max)).unwrap_or((0, 65535));
            let idle = lo + (hi - lo) * 3 / 4;
            let mut a = EventActionRangedAxis { config_id: LEVER_KINDS[0].0.to_string(), options: None };
            a.set_option("USERAWVALUE", "True");
            a.set_option("MAX", lo.to_string());
            a.set_option("MAXTO", lo.to_string());
            a.set_option("IDLE", idle.to_string());
            a.set_option("IDLETO", idle.to_string());
            a
        });
        Self { input: input.to_string(), action }
    }

    fn uses_idle(&self) -> bool {
        self.action.config_id.to_ascii_uppercase().contains("THROTTLE")
    }

    /// Low-end key for this lever kind: `IDLE` for throttles, `MIN` otherwise.
    fn low_key(&self) -> &'static str {
        if self.uses_idle() { "IDLE" } else { "MIN" }
    }

    fn get(&self, key: &str) -> i32 {
        self.action.option_f64(key).unwrap_or(0.0) as i32
    }

    fn set(&mut self, key: &str, v: i32) {
        self.action.set_option(key, v.to_string());
    }

    /// Switches kind, carrying the low zone over between IDLE and MIN.
    fn set_kind(&mut self, config_id: &str) {
        let (from, from_to) = (self.get(self.low_key()), self.get(&format!("{}TO", self.low_key())));
        self.action.config_id = config_id.to_string();
        let k = self.low_key();
        self.set(k, from);
        self.set(&format!("{}TO", k), from_to);
    }
}

/// Finds the first RangedAxis action bound to `input` on the device.
pub fn existing_lever(profile: &Option<SpadProfile>, vid: u16, pid: u16, dev_id: &str, input: &str) -> Option<EventActionRangedAxis> {
    let dev = profile.as_ref()?.find_device_for_connected(vid, pid, dev_id)?;
    dev.pages
        .iter()
        .flat_map(|p| p.events.iter().flat_map(|e| &e.items))
        .filter(|e| crate::profile::schema::bound_to_matches(&e.bound_to, input))
        .flat_map(|e| e.event_definitions.iter().flat_map(|d| &d.items))
        .flat_map(|d| d.actions.iter().flat_map(|a| &a.items))
        .find_map(|a| match a {
            EventActionItem::RangedAxis(r) => Some(r.clone()),
            _ => None,
        })
}

/// Renders the lever card. Returns the action to save when the user clicks "Save lever".
pub fn render_lever_card(
    ui: &mut Ui,
    draft: &mut Option<LeverDraft>,
    input: &str,
    existing: Option<EventActionRangedAxis>,
    live: Option<&LiveAxisState>,
    theme: &AeroTheme,
) -> Option<EventActionRangedAxis> {
    if draft.as_ref().map(|d| d.input != input).unwrap_or(true) {
        *draft = Some(LeverDraft::new(input, existing.clone(), live));
    }
    let d = draft.as_mut().unwrap();
    let mut save = None;
    let mut reset = false;

    ui_card(ui, theme, |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new("CALIBRATED LEVER").color(theme.text_secondary).strong().small());
            if existing.is_some() {
                ui_tag_pill(ui, "saved in profile", theme.success, theme);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    RichText::new("Move the lever, then set its full and idle positions")
                        .color(theme.text_muted)
                        .small(),
                );
            });
        });
        ui.add_space(6.0);

        // Lever kind
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("Lever").color(theme.text_secondary).small());
            for (id, label) in LEVER_KINDS {
                let selected = d.action.config_id.eq_ignore_ascii_case(id);
                if ui.selectable_label(selected, label).clicked() && !selected {
                    d.set_kind(id);
                }
            }
        });

        // Engines (only for per-engine levers)
        let kind_upper = d.action.config_id.to_ascii_uppercase();
        if kind_upper.contains("THROTTLE") || kind_upper.contains("MIXTURE") || kind_upper.contains("PROP") {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Engines").color(theme.text_secondary).small());
                for n in 1..=4 {
                    let key = format!("OPTION{}", n);
                    let mut on = d.action.option_bool(&key);
                    if ui.checkbox(&mut on, format!("{}", n)).changed() {
                        d.action.set_option(&key, if on { "True" } else { "False" });
                    }
                }
                ui.label(RichText::new("(none = all engines)").color(theme.text_muted).small());
            });
        }

        ui.add_space(8.0);
        let (dev_min, dev_max) = live.map(|l| (l.min, l.max)).unwrap_or((0, 65535));
        let raw = live.map(|l| l.raw);
        paint_lever_bar(ui, d, raw, dev_min, dev_max, theme);
        ui.add_space(8.0);

        // Output readout
        let pos = raw.map(|r| lever_position(&d.action, r, dev_min, dev_max));
        ui.horizontal(|ui| {
            let pct = pos.map(|p| format!("{:.0}%", p * 100.0)).unwrap_or_else(|| "--".into());
            ui.label(RichText::new(pct).color(theme.accent).size(26.0).strong());
            ui.vertical(|ui| {
                ui.label(
                    RichText::new(match raw {
                        Some(r) => format!("raw {}", r),
                        None => "move the lever to see it live".into(),
                    })
                    .color(theme.text_muted)
                    .small()
                    .monospace(),
                );
                if let Some(r) = raw {
                    if let Some(outs) = lever_outputs(&d.action, r, dev_min, dev_max) {
                        let names: Vec<String> = outs.iter().map(|(e, _)| e.clone()).collect();
                        ui.label(RichText::new(format!("sends {}", names.join(", "))).color(theme.text_secondary).small());
                    }
                }
            });
        });

        ui.add_space(6.0);
        // Zone editing
        let low = d.low_key();
        egui::Grid::new("lever_zone_grid").num_columns(4).spacing([10.0, 6.0]).show(ui, |ui| {
            for (key, label, color) in [("MAX", "Full", theme.success), (low, if low == "IDLE" { "Idle" } else { "Minimum" }, theme.warning)] {
                ui.label(RichText::new(label).color(color).strong());
                let mut a = d.get(key);
                let mut b = d.get(&format!("{}TO", key));
                let range = dev_min.min(dev_max)..=dev_min.max(dev_max);
                let ca = ui.add(egui::DragValue::new(&mut a).range(range.clone()).speed(50.0).prefix("from "));
                let cb = ui.add(egui::DragValue::new(&mut b).range(range).speed(50.0).prefix("to "));
                if ca.changed() {
                    d.set(key, a);
                }
                if cb.changed() {
                    d.set(&format!("{}TO", key), b);
                }
                let btn = ui.add_enabled(raw.is_some(), egui::Button::new(RichText::new(format!("Set {} here", label.to_lowercase())).small()));
                if btn.clicked() {
                    if let Some(r) = raw {
                        // A small zone around the detent so it is easy to hit.
                        let pad = ((dev_max - dev_min).abs() / 100).max(1);
                        let toward_full = if key == "MAX" { 0 } else { (d.get("MAX") - r).signum() * pad * 2 };
                        d.set(key, r);
                        d.set(&format!("{}TO", key), r + toward_full);
                    }
                }
                ui.end_row();
            }
        });

        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui
                .button(RichText::new("✔ Save lever").color(theme.success).strong())
                .on_hover_text("Saves this lever to the active profile (replaces other axis actions on this input)")
                .clicked()
            {
                save = Some(d.action.clone());
            }
            if ui.button(RichText::new("Reset").small()).clicked() {
                reset = true;
            }
        });
    });
    if reset {
        *draft = Some(LeverDraft::new(input, existing, live));
    }
    save
}

/// Raw-range bar with the full and low zones shaded and the live position marked.
fn paint_lever_bar(ui: &mut Ui, d: &LeverDraft, raw: Option<i32>, dev_min: i32, dev_max: i32, theme: &AeroTheme) {
    let (area, _) = ui.allocate_exact_size(Vec2::new(ui.available_width().max(200.0), 46.0), Sense::hover());
    let rect = Rect::from_min_size(area.min + Vec2::new(0.0, 4.0), Vec2::new(area.width(), 26.0));
    let painter = ui.painter_at(area);
    let span = (dev_max - dev_min) as f32;
    let x_of = |v: i32| rect.min.x + if span.abs() < 1.0 { 0.0 } else { ((v - dev_min) as f32 / span).clamp(0.0, 1.0) * rect.width() };
    let label_x = |v: i32| x_of(v).clamp(rect.min.x + 16.0, rect.max.x - 16.0);

    painter.rect_filled(rect, 6.0, theme.bg_elevated);
    painter.rect_stroke(rect, 6.0, Stroke::new(1.0_f32, theme.border_subtle));

    let zone = |a: i32, b: i32, color: Color32| {
        let (x0, x1) = (x_of(a.min(b)), x_of(a.max(b)).max(x_of(a.min(b)) + 3.0));
        painter.rect_filled(Rect::from_min_max(Pos2::new(x0, rect.min.y + 3.0), Pos2::new(x1, rect.max.y - 3.0)), 3.0, color);
    };
    let low = d.low_key();
    let tint = |c: Color32| Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), 90);
    zone(d.get("MAX"), d.get("MAXTO"), tint(theme.success));
    zone(d.get(low), d.get(&format!("{}TO", low)), tint(theme.warning));

    // Linear range between the zones
    let (full_edge, low_edge) = (d.get("MAXTO"), d.get(&format!("{}TO", low)));
    let y = rect.center().y;
    painter.line_segment([Pos2::new(x_of(full_edge), y), Pos2::new(x_of(low_edge), y)], Stroke::new(2.0_f32, theme.accent_muted));

    if let Some(r) = raw {
        let x = x_of(r);
        painter.line_segment([Pos2::new(x, rect.min.y - 2.0), Pos2::new(x, rect.max.y + 2.0)], Stroke::new(3.0_f32, theme.accent));
    }
    let font = egui::FontId::proportional(11.0);
    painter.text(Pos2::new(label_x(d.get("MAX")), rect.max.y + 1.0), egui::Align2::CENTER_TOP, "FULL", font.clone(), theme.success);
    let low_label = if low == "IDLE" { "IDLE" } else { "MIN" };
    painter.text(Pos2::new(label_x(d.get(low)), rect.max.y + 1.0), egui::Align2::CENTER_TOP, low_label, font, theme.warning);
}
