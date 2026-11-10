//! Trigger zones on an axis (`ZONE_ENTER` / `ZONE_LEAVE` definitions), e.g. arming the
//! spoilers at the bottom of the speedbrake lever. A zone can have a deadzone around it where
//! the input's proportional output is frozen, so the detent does not drag the axis along.

use eframe::egui::{self, Color32, Pos2, RichText, Sense, Stroke, Ui, Vec2};

use crate::gui::app::LiveAxisState;
use crate::gui::theme::AeroTheme;
use crate::gui::views::mapping_view::input_definitions_mut;
use crate::gui::widgets::{ui_card, ui_elevated_card, ui_tag_pill};
use crate::hardware::enumerator::ConnectedDevice;
use crate::profile::executor::axis_position;
use crate::profile::schema::{
    axis_outside_bands, AxisZone, EventActionControl, EventActionItem, EventActionsList, EventDefinition, SpadProfile,
};

/// Ready-made zone actions: (label, enter event, leave event).
const ZONE_PRESETS: [(&str, &str, &str); 3] = [
    ("Spoilers ARM", "SPOILERS_ARM_ON", "SPOILERS_ARM_OFF"),
    ("Spoilers ARM (toggle)", "SPOILERS_ARM_TOGGLE", "SPOILERS_ARM_TOGGLE"),
    ("Custom", "", ""),
];

/// Zone being added in the editor (percent of the raw device travel).
#[derive(Debug, Clone)]
pub struct ZoneDraft {
    pub min: f64,
    pub max: f64,
    pub deadzone: f64,
    pub preset: usize,
    pub enter_event: String,
    pub leave_event: String,
}

impl Default for ZoneDraft {
    fn default() -> Self {
        Self {
            min: 0.0,
            max: 8.0,
            deadzone: 4.0,
            preset: 0,
            enter_event: ZONE_PRESETS[0].1.to_string(),
            leave_event: ZONE_PRESETS[0].2.to_string(),
        }
    }
}

impl ZoneDraft {
    /// Hysteresis is kept below the deadzone so the zone is left before the axis moves.
    fn zone(&self) -> AxisZone {
        let (min, max) = (self.min.min(self.max), self.min.max(self.max));
        AxisZone { min, max, hysteresis: (self.deadzone * 0.75).min(AxisZone::DEFAULT_HYSTERESIS), deadzone: Some(self.deadzone) }
    }
}

/// A zone configured on the input, with a short description of its actions.
pub struct ZoneSummary {
    pub zone: AxisZone,
    pub enter: Vec<String>,
    pub leave: Vec<String>,
}

fn describe(act: &EventActionItem) -> String {
    match act {
        EventActionItem::Control(c) => c.target.strip_prefix("SIMCONNECT:").unwrap_or(&c.target).to_string(),
        EventActionItem::ChangeValue(cv) => format!("{} = {}", cv.target, cv.value.as_deref().unwrap_or("")),
        EventActionItem::Script(_) => "RPN script".to_string(),
        _ => "action".to_string(),
    }
}

/// Zones on `input`, grouped by range, in travel order.
pub fn zones_for_input(profile: &Option<SpadProfile>, dev: &ConnectedDevice, input: &str) -> Vec<ZoneSummary> {
    let mut out: Vec<ZoneSummary> = Vec::new();
    let Some(dev_prof) = profile.as_ref().and_then(|p| p.find_device_for_connected(dev.vendor_id, dev.product_id, &dev.id)) else {
        return out;
    };
    let defs = dev_prof
        .pages
        .iter()
        .filter_map(|p| p.events.as_ref())
        .flat_map(|e| &e.items)
        .filter(|e| crate::profile::schema::bound_to_matches(&e.bound_to, input))
        .filter_map(|e| e.event_definitions.as_ref())
        .flat_map(|d| &d.items);
    for def in defs {
        let Some(zone) = AxisZone::of(def) else { continue };
        let entering = def.trigger.as_deref().is_some_and(|t| t.eq_ignore_ascii_case("ZONE_ENTER"));
        let idx = match out.iter().position(|s| s.zone.key() == zone.key()) {
            Some(i) => i,
            None => {
                out.push(ZoneSummary { zone, enter: Vec::new(), leave: Vec::new() });
                out.len() - 1
            }
        };
        let s = &mut out[idx];
        // The deadzone of either definition applies (the executor freezes the union).
        if s.zone.deadzone.is_none() {
            s.zone.deadzone = zone.deadzone;
        }
        let list = if entering { &mut s.enter } else { &mut s.leave };
        list.extend(def.actions.iter().flat_map(|a| &a.items).map(describe));
    }
    out.sort_by(|a, b| a.zone.min.total_cmp(&b.zone.min));
    out
}

fn zone_definition(trigger: &str, zone: AxisZone, event: &str) -> EventDefinition {
    let target = if crate::profile::evaluator::split_data_prefix(event).is_some() {
        event.to_string()
    } else {
        format!("SIMCONNECT:{}", event)
    };
    EventDefinition {
        trigger: Some(trigger.to_string()),
        priority: None,
        is_final: Some(false),
        options: Some(zone.to_options()),
        condition_binding: None,
        conditions: None,
        actions: Some(EventActionsList {
            items: vec![EventActionItem::Control(EventActionControl { target, value: Some("0".to_string()), parameters: Vec::new() })],
        }),
    }
}

/// Adds a zone with a SimConnect event on enter and/or leave (empty = none).
pub fn add_zone(profile: &mut Option<SpadProfile>, dev: &ConnectedDevice, input: &str, draft: &ZoneDraft) {
    let Some(defs) = input_definitions_mut(profile, dev, input) else { return };
    let zone = draft.zone();
    for (trigger, event) in [("ZONE_ENTER", draft.enter_event.trim()), ("ZONE_LEAVE", draft.leave_event.trim())] {
        if !event.is_empty() {
            defs.items.push(zone_definition(trigger, zone, event));
        }
    }
}

/// Removes every definition of the zone with `key` from `input`.
pub fn remove_zone(profile: &mut Option<SpadProfile>, dev: &ConnectedDevice, input: &str, key: &str) {
    if let Some(defs) = input_definitions_mut(profile, dev, input) {
        defs.items.retain(|d| AxisZone::of(d).is_none_or(|z| z.key() != key));
    }
}

/// Live position in percent of the raw travel.
fn live_pct(live: Option<&LiveAxisState>) -> Option<f64> {
    live.map(|l| axis_position(l.raw, l.min, l.max) * 100.0)
}

/// Editor card. Returns true when the profile changed and should be saved.
pub fn render_zone_card(
    ui: &mut Ui,
    draft: &mut ZoneDraft,
    profile: &mut Option<SpadProfile>,
    dev: Option<&ConnectedDevice>,
    input: &str,
    live: Option<&LiveAxisState>,
    theme: &AeroTheme,
) -> bool {
    let mut changed = false;
    let zones = dev.map(|d| zones_for_input(profile, d, input)).unwrap_or_default();
    let pos = live_pct(live);

    ui_card(ui, theme, |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new("TRIGGER ZONES").color(theme.text_secondary).strong().small());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(RichText::new("Fire events when the lever enters or leaves a range").color(theme.text_muted).small());
            });
        });
        ui.add_space(4.0);

        let saved: Vec<AxisZone> = zones.iter().map(|z| z.zone).collect();
        paint_zone_bar(ui, &saved, Some(draft.zone()), pos, theme);
        if let Some(p) = pos {
            let bands: Vec<(f64, f64)> = saved.iter().filter_map(|z| z.frozen_band()).collect();
            let (out, frozen) = axis_outside_bands(p, &bands);
            let inside: Vec<String> = saved
                .iter()
                .filter(|z| p >= z.min && p <= z.max)
                .map(|z| format!("{:.0}–{:.0}%", z.min, z.max))
                .collect();
            let mut text = format!("lever {:.1}%", p);
            if !bands.is_empty() {
                text += &format!("   axis output {:.1}%{}", out * 100.0, if frozen { " (frozen)" } else { "" });
            }
            if !inside.is_empty() {
                text += &format!("   in zone {}", inside.join(", "));
            }
            ui.label(RichText::new(text).color(theme.text_primary).monospace());
        }
        ui.add_space(6.0);

        for z in &zones {
            ui_elevated_card(ui, theme, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui_tag_pill(ui, &format!("{:.1}–{:.1}%", z.zone.min, z.zone.max), theme.accent, theme);
                    match z.zone.deadzone {
                        Some(d) => ui_tag_pill(ui, &format!("deadzone {:.1}%", d), theme.warning, theme),
                        None => ui_tag_pill(ui, "axis not frozen", theme.text_muted, theme),
                    };
                    let list = |v: &Vec<String>| if v.is_empty() { "—".to_string() } else { v.join(", ") };
                    ui.label(RichText::new(format!("on enter: {}   on leave: {}", list(&z.enter), list(&z.leave))).color(theme.text_primary));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(RichText::new("🗑 Delete").color(theme.danger).small()).clicked() {
                            if let Some(d) = dev {
                                remove_zone(profile, d, input, &z.zone.key());
                                changed = true;
                            }
                        }
                    });
                });
            });
            ui.add_space(2.0);
        }

        ui.add_space(4.0);
        ui.label(RichText::new("New zone").color(theme.accent).strong());
        egui::Grid::new("zone_draft_grid").num_columns(4).spacing([10.0, 6.0]).show(ui, |ui| {
            for (label, value) in [("From", &mut draft.min), ("To", &mut draft.max)] {
                ui.label(RichText::new(label).color(theme.text_secondary).small());
                ui.add(egui::DragValue::new(value).range(0.0..=100.0).speed(0.2).suffix("%").max_decimals(1));
                let here = ui.add_enabled(pos.is_some(), egui::Button::new(RichText::new("Lever here").small()));
                if here.on_hover_text("Use the current lever position").clicked() {
                    *value = (pos.unwrap_or(0.0) * 10.0).round() / 10.0;
                }
                ui.label("");
                ui.end_row();
            }
            ui.label(RichText::new("Deadzone").color(theme.text_secondary).small());
            ui.add(egui::Slider::new(&mut draft.deadzone, 0.0..=15.0).suffix("%").max_decimals(1));
            ui.label(
                RichText::new("axis output frozen this far around the zone; the rest of the travel is stretched to the full range")
                    .color(theme.text_muted)
                    .small(),
            );
            ui.end_row();
        });

        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("Actions:").color(theme.text_secondary).small());
            let prev = draft.preset;
            egui::ComboBox::from_id_salt("zone_preset_combo")
                .width(170.0)
                .selected_text(ZONE_PRESETS[draft.preset].0)
                .show_ui(ui, |ui| {
                    for (i, p) in ZONE_PRESETS.iter().enumerate() {
                        ui.selectable_value(&mut draft.preset, i, p.0);
                    }
                });
            if draft.preset != prev && !ZONE_PRESETS[draft.preset].1.is_empty() {
                draft.enter_event = ZONE_PRESETS[draft.preset].1.to_string();
                draft.leave_event = ZONE_PRESETS[draft.preset].2.to_string();
            }
            ui.label(RichText::new("on enter").color(theme.text_secondary).small());
            ui.add(egui::TextEdit::singleline(&mut draft.enter_event).hint_text("SIM EVENT").desired_width(170.0));
            ui.label(RichText::new("on leave").color(theme.text_secondary).small());
            ui.add(egui::TextEdit::singleline(&mut draft.leave_event).hint_text("SIM EVENT").desired_width(170.0));
        });
        ui.label(
            RichText::new("For PMDG events, LVARs or scripts, add the zone here and then more actions with Add Action, trigger Zone enter / Zone leave.")
                .color(theme.text_muted)
                .small(),
        );
        ui.add_space(4.0);
        let valid = (draft.max - draft.min).abs() > f64::EPSILON;
        let overlaps = zones.iter().any(|z| z.zone.key() == draft.zone().key());
        ui.horizontal(|ui| {
            let add = ui.add_enabled(valid && !overlaps, egui::Button::new(RichText::new("➕ Add zone").color(theme.success).strong()));
            if add.clicked() {
                if let Some(d) = dev {
                    add_zone(profile, d, input, draft);
                    changed = true;
                }
            }
            if overlaps {
                ui.label(RichText::new("a zone with this range already exists").color(theme.warning).small());
            }
        });
    });
    changed
}

/// Bar over the raw travel: saved zones, their frozen bands, the draft outline and the lever.
fn paint_zone_bar(ui: &mut Ui, zones: &[AxisZone], draft: Option<AxisZone>, pos: Option<f64>, theme: &AeroTheme) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width().max(160.0), 22.0), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let painter = ui.painter();
    painter.rect_filled(rect, 4.0, theme.bg_elevated);
    painter.rect_stroke(rect, 4.0, Stroke::new(1.0_f32, theme.border_subtle));
    let x = |p: f64| rect.min.x + (p.clamp(0.0, 100.0) / 100.0) as f32 * rect.width();
    let span = |a: f64, b: f64, c: Color32| {
        let r = egui::Rect::from_min_max(Pos2::new(x(a), rect.min.y + 1.0), Pos2::new(x(b).max(x(a) + 2.0), rect.max.y - 1.0));
        painter.rect_filled(r, 2.0, c);
    };
    let tint = |c: Color32, a: u8| Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), a);
    for z in zones {
        if let Some((a, b)) = z.frozen_band() {
            span(a, b, tint(theme.warning, 50));
        }
        span(z.min, z.max, tint(theme.accent, 140));
    }
    if let Some(d) = draft {
        let r = egui::Rect::from_min_max(Pos2::new(x(d.min), rect.min.y + 1.0), Pos2::new(x(d.max).max(x(d.min) + 2.0), rect.max.y - 1.0));
        painter.rect_stroke(r, 2.0, Stroke::new(1.0_f32, theme.text_secondary));
    }
    if let Some(p) = pos {
        let px = x(p);
        painter.line_segment([Pos2::new(px, rect.min.y - 2.0), Pos2::new(px, rect.max.y + 2.0)], Stroke::new(2.0_f32, theme.text_primary));
    }
}
