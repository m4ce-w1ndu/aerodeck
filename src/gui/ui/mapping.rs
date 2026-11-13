//! Controls page: device and input lists, sim axis, calibrated lever, trigger zones, actions.

use std::rc::Rc;
use std::time::Duration;

use slint::{ComponentHandle, Model};

use super::*;
use crate::gui::editing::{self as ed, NewAction, NewActionKind, ZoneDraft};
use crate::gui::presets::{COMMON_CALCULATOR_CODES, SIMCONNECT_AXIS_PRESETS};
use crate::gui::sim_events::{categories, SIM_EVENTS};
use crate::hardware::enumerator::DeviceCategory;
use crate::profile::executor::axis_position;
use crate::profile::lever;
use crate::profile::schema::{axis_outside_bands, EventActionItem, EventActionRangedAxis, EventConditionItem};

#[derive(Default)]
pub(crate) struct MappingUi {
    pub device_idx: usize,
    pub selected_input: String,
    /// Input ids currently in the list model, in order
    pub listed: Vec<String>,
    /// Lever being edited: (input, action)
    pub lever: Option<(String, EventActionRangedAxis)>,
    /// The lever draft has edits that are not saved to the profile yet
    pub lever_touched: bool,
    /// Unsaved lever drafts of inputs you moved away from, by `device id|input`
    pub lever_drafts: HashMap<String, EventActionRangedAxis>,
    pub zone: ZoneDraft,
    /// Trigger ids behind the trigger segmented control
    pub triggers: Vec<String>,
    /// Zone keys behind the zone segmented control
    pub zone_keys: Vec<String>,
    /// Action kinds behind the action-type control
    pub kinds: Vec<NewActionKind>,
    pub pmdg_event: String,
}

/// After an input moves, axes cannot take the selection away from it for this long.
const AXIS_FOLLOW_HOLD: Duration = Duration::from_millis(1500);

fn draft_key(dev: &ConnectedDevice, input: &str) -> String {
    format!("{}|{}", dev.id, input)
}

impl MappingUi {
    /// Keeps the lever being edited, if it has unsaved changes, so switching to another
    /// input and back does not throw the calibration away.
    fn stash_lever_draft(&mut self, dev: Option<&ConnectedDevice>) {
        let touched = std::mem::take(&mut self.lever_touched);
        if let (Some((input, lever)), Some(dev)) = (self.lever.take(), dev) {
            if touched {
                self.lever_drafts.insert(draft_key(dev, &input), lever);
            }
        }
    }
}

fn dev_range(live: Option<&LiveAxis>) -> (i32, i32) {
    live.map(|l| (l.min, l.max)).unwrap_or((0, 65535))
}

fn frac(v: i32, lo: i32, hi: i32) -> f32 {
    let span = (hi - lo) as f32;
    if span.abs() < 1.0 {
        0.0
    } else {
        ((v - lo) as f32 / span).clamp(0.0, 1.0)
    }
}

fn seg(a: f32, b: f32, tone: i32, outline: bool) -> BarSeg {
    BarSeg { start: a.min(b), end: a.max(b), tone, outline }
}

fn mark(pos: f32, tone: i32, label: &str) -> BarMark {
    BarMark { pos, tone, label: ss(label) }
}

impl App {
    pub(crate) fn current_device(&self) -> Option<ConnectedDevice> {
        let st = self.st.borrow();
        st.devices.get(st.m.device_idx).cloned()
    }

    fn live_for(&self, dev: &ConnectedDevice, input: &str) -> Option<LiveAxis> {
        let st = self.st.borrow();
        st.live_axes.get(&format!("{}:{}", dev.name, input)).or_else(|| st.live_axes.get(input)).cloned()
    }

    /// Selects the input touched on the hardware, switching device if needed.
    /// Returns true when the whole page was refreshed.
    ///
    /// Axes are followed more cautiously than buttons: an axis never takes the selection
    /// away from an input that is being worked on (moved in the last moments, or a lever
    /// with unsaved calibration), so moving twin throttle levers together, or nudging the
    /// stick while pressing one of its buttons, does not bounce the page between inputs.
    pub(crate) fn follow_input(self: &Rc<Self>, device: &str, bound_to: &str, is_axis: bool) -> bool {
        let Some(ui) = self.ui.upgrade() else { return false };
        let ms = ui.global::<MappingState>();
        if !ms.get_follow() || ui.global::<AppState>().get_page() != 1 || ms.get_show_add() {
            return false;
        }
        let idx = {
            let st = self.st.borrow();
            let matches = |d: &ConnectedDevice| d.name.eq_ignore_ascii_case(device) || device.contains(&d.name) || d.name.contains(device);
            if st.devices.get(st.m.device_idx).is_some_and(matches) {
                Some(st.m.device_idx)
            } else {
                st.devices.iter().position(matches)
            }
        };
        let Some(idx) = idx else { return false };
        {
            let st = self.st.borrow();
            if st.m.device_idx == idx && st.m.selected_input == bound_to {
                return false;
            }
            if is_axis && !st.m.selected_input.is_empty() {
                if st.m.lever_touched {
                    return false;
                }
                let busy = st.devices.get(st.m.device_idx).is_some_and(|d| {
                    st.input_activity
                        .get(&format!("{}|{}", d.name, st.m.selected_input))
                        .is_some_and(|t| t.elapsed() < AXIS_FOLLOW_HOLD)
                });
                if busy {
                    return false;
                }
            }
        }
        {
            let mut st = self.st.borrow_mut();
            st.m.device_idx = idx;
        }
        self.select_input(bound_to);
        true
    }

    pub(crate) fn select_input(self: &Rc<Self>, id: &str) {
        let dev = self.current_device();
        {
            let mut st = self.st.borrow_mut();
            st.m.stash_lever_draft(dev.as_ref());
            st.m.selected_input = id.to_string();
            st.m.pmdg_event.clear();
        }
        if let (Some(ui), Some(dev)) = (self.ui.upgrade(), dev.as_ref()) {
            let ms = ui.global::<MappingState>();
            ms.set_show_add(false);
            // Lever editor for quadrants and inputs that already have a lever.
            let existing = ed::existing_lever(&self.st.borrow().profile, dev, id);
            let quadrant = dev.category == DeviceCategory::ThrottleQuadrant;
            ms.set_axis_mode(if existing.is_some() || quadrant { 1 } else { 0 });
            let axis = ed::existing_axis(&self.st.borrow().profile, dev, id);
            let (preset, invert) = match &axis {
                Some((ev, inv)) => (SIMCONNECT_AXIS_PRESETS.iter().position(|p| p.name == ev).unwrap_or(0), *inv),
                None => (0, false),
            };
            ms.set_axis_preset(preset as i32);
            ms.set_axis_invert(invert);
            self.ensure_lever_draft(dev, id);
        }
        self.refresh_mapping_all();
    }

    fn ensure_lever_draft(&self, dev: &ConnectedDevice, input: &str) {
        if self.st.borrow().m.lever.as_ref().is_some_and(|(i, _)| i == input) {
            return;
        }
        let live = self.live_for(dev, input);
        let (lo, hi) = dev_range(live.as_ref());
        let mut st = self.st.borrow_mut();
        // Resume the unsaved calibration left on this input, if any.
        if let Some(draft) = st.m.lever_drafts.remove(&draft_key(dev, input)) {
            st.m.lever = Some((input.to_string(), draft));
            st.m.lever_touched = true;
            return;
        }
        let lever = ed::existing_lever(&st.profile, dev, input).unwrap_or_else(|| ed::default_lever(lo, hi));
        st.m.lever = Some((input.to_string(), lever));
        st.m.lever_touched = false;
    }

    pub(crate) fn refresh_mapping_all(self: &Rc<Self>) {
        self.refresh_devices_selection();
        self.refresh_inputs();
        self.refresh_selected();
        self.refresh_mapping_live();
        self.refresh_add_form();
    }

    fn refresh_devices_selection(&self) {
        let Some(ui) = self.ui.upgrade() else { return };
        let ms = ui.global::<MappingState>();
        let mut st = self.st.borrow_mut();
        if st.m.device_idx >= st.devices.len() {
            st.m.device_idx = 0;
        }
        ms.set_no_devices(st.devices.is_empty());
        ms.set_device_index(st.m.device_idx as i32);
        if let Some(d) = st.devices.get(st.m.device_idx) {
            ms.set_device_name(ss(&d.name));
            ms.set_device_meta(ss(format!("{:04X}:{:04X} · {} buttons · {} axes", d.vendor_id, d.product_id, d.buttons.len(), d.axes.len())));
        }
    }

    pub(crate) fn refresh_inputs(&self) {
        let Some(ui) = self.ui.upgrade() else { return };
        let ms = ui.global::<MappingState>();
        let Some(dev) = self.current_device() else {
            self.inputs_model.set_vec(Vec::new());
            return;
        };
        let query = ms.get_query().trim().to_lowercase();
        let filter = ms.get_filter();
        let st = self.st.borrow();
        let all = ed::device_inputs(&st.profile, &dev);
        let total = all.len();
        let mut items = Vec::new();
        let mut listed = Vec::new();
        for (id, is_axis) in all {
            let (count, summary) = ed::input_summary(&st.profile, &dev, &id);
            match filter {
                1 if is_axis => continue,
                2 if !is_axis => continue,
                3 if count == 0 => continue,
                _ => {}
            }
            let label = ed::input_label(&id);
            if !query.is_empty() {
                let hit = id.to_lowercase().contains(&query)
                    || label.to_lowercase().contains(&query)
                    || summary.as_ref().is_some_and(|s| s.to_lowercase().contains(&query));
                if !hit {
                    continue;
                }
            }
            let kind = if is_axis { 1 } else if ed::is_hat_input_name(&id) { 2 } else { 0 };
            let live = st.input_activity.get(&format!("{}|{}", dev.name, id)).is_some_and(|t| t.elapsed() < Duration::from_millis(800));
            items.push(InputItem { id: ss(&id), label: ss(label), kind, summary: ss(summary.unwrap_or_default()), count: count as i32, live });
            listed.push(id);
        }
        drop(st);
        ms.set_input_total(total as i32);
        self.inputs_model.set_vec(items);
        self.st.borrow_mut().m.listed = listed;
    }

    /// Updates the live flash of the input rows.
    pub(crate) fn refresh_input_activity(&self) {
        let Some(dev) = self.current_device() else { return };
        let st = self.st.borrow();
        for (i, id) in st.m.listed.iter().enumerate() {
            let live = st.input_activity.get(&format!("{}|{}", dev.name, id)).is_some_and(|t| t.elapsed() < Duration::from_millis(800));
            if let Some(mut row) = self.inputs_model.row_data(i) {
                if row.live != live {
                    row.live = live;
                    self.inputs_model.set_row_data(i, row);
                }
            }
        }
    }

    /// Header, actions, lever rows and zones of the selected input.
    fn refresh_selected(&self) {
        let Some(ui) = self.ui.upgrade() else { return };
        let ms = ui.global::<MappingState>();
        let dev = self.current_device();
        let input = self.st.borrow().m.selected_input.clone();
        ms.set_selected_input(ss(&input));
        ms.set_selected_label(ss(ed::input_label(&input)));
        let Some(dev) = dev.filter(|_| !input.is_empty()) else {
            ms.set_is_axis(false);
            ms.set_is_hat(false);
            ms.set_actions(model(Vec::new()));
            return;
        };
        let is_axis = dev.axes.iter().any(|a| a.eq_ignore_ascii_case(&input)) || ed::is_axis_input_name(&input);
        ms.set_is_axis(is_axis);
        ms.set_is_hat(ed::is_hat_input_name(&input));

        // Configured actions
        let st = self.st.borrow();
        let mut actions = Vec::new();
        if let Some(dp) = st.profile.as_ref().and_then(|p| p.find_device_for_connected(dev.vendor_id, dev.product_id, &dev.id)) {
            for ev in dp.pages.iter().filter_map(|p| p.events.as_ref()).flat_map(|e| &e.items) {
                if !crate::profile::schema::bound_to_matches(&ev.bound_to, &input) {
                    continue;
                }
                let Some(defs) = ev.event_definitions.as_ref() else { continue };
                for (di, def) in defs.items.iter().enumerate() {
                    let trigger = def.trigger.as_deref().unwrap_or("PRESS");
                    let label = match crate::profile::schema::AxisZone::of(def) {
                        Some(z) => format!("{} {:.0}–{:.0}%", ed::trigger_label(trigger), z.min, z.max),
                        None => ed::trigger_label(trigger),
                    };
                    let condition = def
                        .conditions
                        .as_ref()
                        .and_then(|c| c.items.first())
                        .map(|ci| match ci {
                            EventConditionItem::Simple(s) => format!("if {} {} {}", s.target, s.comparator.as_deref().unwrap_or("=="), s.value.as_deref().unwrap_or("")),
                            EventConditionItem::Expression(e) => format!("if {}", e.expression),
                        })
                        .unwrap_or_default();
                    for (ai, act) in def.actions.iter().flat_map(|a| a.items.iter()).enumerate() {
                        let (kind, title, detail) = ed::describe_action(act);
                        actions.push(ActionItem {
                            trigger: ss(&label),
                            title: ss(title),
                            detail: ss(detail),
                            kind: kind as i32,
                            condition: ss(&condition),
                            def_index: di as i32,
                            act_index: ai as i32,
                            testable: ed::test_command(act, trigger).is_some(),
                        });
                    }
                }
            }
        }
        // Axis summary
        let axis_current = ed::existing_axis(&st.profile, &dev, &input)
            .map(|(e, inv)| if inv { format!("{} · inverted", e) } else { e })
            .unwrap_or_default();
        let saved_lever = ed::existing_lever(&st.profile, &dev, &input);
        drop(st);
        ms.set_actions(model(actions));
        ms.set_axis_current(ss(axis_current));
        ms.set_axis_presets(strings(SIMCONNECT_AXIS_PRESETS.iter().map(|p| format!("{}  ·  {}", p.description, p.name))));

        if is_axis {
            self.ensure_lever_draft(&dev, &input);
            let st = self.st.borrow();
            if let Some((_, lv)) = &st.m.lever {
                let kind_idx = ed::LEVER_KINDS.iter().position(|(id, _)| id.eq_ignore_ascii_case(&lv.config_id)).unwrap_or(0);
                ms.set_lever_kinds(strings(ed::LEVER_KINDS.iter().map(|(_, l)| *l)));
                ms.set_lever_kind(kind_idx as i32);
                let per_engine = lever::LeverKind::from_config_id(&lv.config_id)
                    .is_some_and(|k| matches!(k, lever::LeverKind::Throttle | lever::LeverKind::Mixture | lever::LeverKind::Propeller));
                ms.set_lever_per_engine(per_engine);
                ms.set_engines(model((1..=4).map(|n| lv.option_bool(&format!("OPTION{}", n))).collect()));
                ms.set_lever_saved(saved_lever.as_ref().is_some_and(|s| s.options == lv.options && s.config_id == lv.config_id));
                ms.set_lever_reverse(lever::has_reverse(lv));
                ms.set_lever_gate(lever::reverse_gate(lv) as f32);
                let low = ed::lever_low_key(lv);
                let mut rows = vec![
                    LeverRow { key: ss("MAX"), label: ss("Full"), tone: 2, from: ss(ed::lever_get(lv, "MAX").to_string()), to: ss(ed::lever_get(lv, "MAXTO").to_string()) },
                    LeverRow {
                        key: ss(low),
                        label: ss(if low == "IDLE" { "Idle" } else { "Minimum" }),
                        tone: 3,
                        from: ss(ed::lever_get(lv, low).to_string()),
                        to: ss(ed::lever_get(lv, &format!("{}TO", low)).to_string()),
                    },
                ];
                if lever::has_reverse(lv) {
                    rows.push(LeverRow {
                        key: ss("REVERSE"),
                        label: ss("Reverse"),
                        tone: 4,
                        from: ss(ed::lever_get(lv, "REVERSE").to_string()),
                        to: ss(ed::lever_get(lv, "REVERSETO").to_string()),
                    });
                }
                ms.set_lever_rows(model(rows));
            }
            drop(st);
            self.refresh_zones(&dev, &input);
        }
    }

    fn refresh_zones(&self, dev: &ConnectedDevice, input: &str) {
        let Some(ui) = self.ui.upgrade() else { return };
        let ms = ui.global::<MappingState>();
        let st = self.st.borrow();
        let zones = ed::zones_for_input(&st.profile, dev, input);
        let list = |v: &Vec<String>| if v.is_empty() { "—".to_string() } else { v.join(", ") };
        ms.set_zones(model(
            zones
                .iter()
                .map(|z| ZoneItem {
                    range: ss(format!("{:.1}–{:.1}%", z.zone.min, z.zone.max)),
                    deadzone: ss(match z.zone.deadzone {
                        Some(d) => format!("deadzone {:.1}%", d),
                        None => "axis not frozen".to_string(),
                    }),
                    frozen: z.zone.deadzone.is_some(),
                    enter: ss(list(&z.enter)),
                    leave: ss(list(&z.leave)),
                    key: ss(z.zone.key()),
                })
                .collect(),
        ));
        ms.set_zone_presets(strings(ed::ZONE_PRESETS.iter().map(|p| p.0)));
        // Draft validation
        let draft = &st.m.zone;
        let error = if (draft.max - draft.min).abs() <= f64::EPSILON {
            "The zone is empty".to_string()
        } else if zones.iter().any(|z| z.zone.key() == draft.zone().key()) {
            "A zone with this range already exists".to_string()
        } else if draft.enter_event.trim().is_empty() && draft.leave_event.trim().is_empty() {
            "Set an event for enter or leave".to_string()
        } else {
            String::new()
        };
        ms.set_zone_error(ss(error));
    }

    /// Everything that follows the live axis position.
    pub(crate) fn refresh_mapping_live(&self) {
        let Some(ui) = self.ui.upgrade() else { return };
        let ms = ui.global::<MappingState>();
        let Some(dev) = self.current_device() else { return };
        let input = self.st.borrow().m.selected_input.clone();
        if input.is_empty() || !ms.get_is_axis() {
            return;
        }
        let live = self.live_for(&dev, &input);
        ms.set_has_live(live.is_some());
        let (lo, hi) = dev_range(live.as_ref());

        // Sim axis bar
        let norm = live.as_ref().map(|l| l.normalized).unwrap_or(0.0);
        let raw = live.as_ref().map(|l| l.raw).unwrap_or(0);
        let bipolar = live.as_ref().map(|l| l.min < 0).unwrap_or(!input.to_ascii_uppercase().contains("THROTTLE"));
        let inv = ms.get_axis_invert();
        let eff = match (inv, bipolar) {
            (false, _) => norm,
            (true, true) => -norm,
            (true, false) => 1.0 - norm,
        };
        let pos = if bipolar { ((eff + 1.0) / 2.0) as f32 } else { eff as f32 };
        let base = if bipolar { 0.5 } else { 0.0 };
        ms.set_axis_segments(model(vec![seg(base, pos, 1, false)]));
        ms.set_axis_marks(model(if bipolar { vec![mark(0.5, 0, "")] } else { Vec::new() }));
        ms.set_axis_live(pos);
        ms.set_axis_readout(ss(format!("{:+.1}%", eff * 100.0)));
        ms.set_axis_raw(ss(if live.is_some() { format!("raw {}", raw) } else { "move the axis to see it live".to_string() }));

        // Lever
        let st = self.st.borrow();
        if let Some((_, lv)) = &st.m.lever {
            let f = |v: i32| frac(v, lo, hi);
            let low = ed::lever_low_key(lv);
            let mut segs = vec![
                seg(f(ed::lever_get(lv, "MAX")), f(ed::lever_get(lv, "MAXTO")), 2, false),
                seg(f(ed::lever_get(lv, low)), f(ed::lever_get(lv, &format!("{}TO", low))), 3, false),
                seg(f(ed::lever_get(lv, "MAXTO")), f(ed::lever_get(lv, &format!("{}TO", low))), 1, true),
            ];
            let mut marks = vec![
                mark(f(ed::lever_get(lv, "MAX")), 2, "FULL"),
                mark(f(ed::lever_get(lv, low)), 3, if low == "IDLE" { "IDLE" } else { "MIN" }),
            ];
            if lever::has_reverse(lv) && lv.option("REVERSE").is_some() {
                segs.push(seg(f(ed::lever_get(lv, "REVERSE")), f(ed::lever_get(lv, "REVERSETO")), 4, false));
                marks.push(mark(f(ed::lever_get(lv, "REVERSE")), 4, "REV"));
                // Reverse gate between the idle zone and the reverse zone
                let max_below = ed::lever_get(lv, "MAX") <= ed::lever_get(lv, "IDLE");
                let (i1, i2) = (ed::lever_get(lv, "IDLE"), ed::lever_get(lv, "IDLETO"));
                let (r1, r2) = (ed::lever_get(lv, "REVERSE"), ed::lever_get(lv, "REVERSETO"));
                let idle_far = if max_below { i1.max(i2) } else { i1.min(i2) };
                let rev_near = if max_below { r1.min(r2) } else { r1.max(r2) };
                let g = idle_far as f64 + (rev_near - idle_far) as f64 * lever::reverse_gate(lv) / 100.0;
                marks.push(mark(f(g.round() as i32), 4, "GATE"));
            }
            ms.set_lever_segments(model(segs));
            ms.set_lever_marks(model(marks));
            ms.set_lever_live(f(raw));
            match &live {
                Some(l) => {
                    let outs = lever::lever_outputs(lv, l.raw, l.min, l.max).unwrap_or_default();
                    let in_reverse = outs.first().is_some_and(|(e, _)| e.starts_with("THROTTLE") && !e.contains("AXIS"));
                    let pos = lever::lever_position(lv, l.raw, l.min, l.max);
                    if in_reverse {
                        let v = outs.first().map(|o| o.1).unwrap_or(0);
                        ms.set_lever_percent(ss(format!("R {:.0}%", (v.abs() as f64 / 16383.0) * 100.0)));
                    } else {
                        ms.set_lever_percent(ss(format!("{:.0}%", pos * 100.0)));
                    }
                    ms.set_lever_raw(ss(format!("raw {}", l.raw)));
                    ms.set_lever_sends(ss(match outs.first() {
                        Some((e, v)) if outs.len() == 1 => format!("sends {} = {}", e, v),
                        Some((_, v)) => format!("sends {} = {}", outs.iter().map(|o| o.0.as_str()).collect::<Vec<_>>().join(", "), v),
                        None => String::new(),
                    }));
                    let gate = lever::reverse_gate(lv);
                    let (state, tone) = match lever::reverse_travel(lv, l.raw, l.min, l.max) {
                        None => (String::new(), 0),
                        Some(t) if !in_reverse => (format!("Past the idle detent {:.0}% · gate at {:.0}% · still idle", t * 100.0, gate), 3),
                        Some(t) => (format!("REVERSE · {:.0}% past the detent", t * 100.0), 4),
                    };
                    ms.set_lever_state(ss(state));
                    ms.set_lever_state_tone(tone);
                }
                None => {
                    ms.set_lever_percent(ss("--"));
                    ms.set_lever_raw(ss("move the lever to see it live"));
                    ms.set_lever_sends(ss(""));
                    ms.set_lever_state(ss(""));
                }
            }
        }

        // Zones bar
        let zones = ed::zones_for_input(&st.profile, &dev, &input);
        let p = live.as_ref().map(|l| axis_position(l.raw, l.min, l.max) * 100.0);
        let mut zsegs = Vec::new();
        for z in &zones {
            if let Some((a, b)) = z.zone.frozen_band() {
                zsegs.push(seg(a as f32 / 100.0, b as f32 / 100.0, 3, false));
            }
            zsegs.push(seg(z.zone.min as f32 / 100.0, z.zone.max as f32 / 100.0, 5, false));
        }
        let d = st.m.zone.zone();
        zsegs.push(seg(d.min as f32 / 100.0, d.max as f32 / 100.0, 0, true));
        ms.set_zone_segments(model(zsegs));
        ms.set_zone_live(p.unwrap_or(0.0) as f32 / 100.0);
        ms.set_zone_readout(ss(match p {
            Some(p) => {
                let bands: Vec<(f64, f64)> = zones.iter().filter_map(|z| z.zone.frozen_band()).collect();
                let (out, frozen) = axis_outside_bands(p, &bands);
                let inside: Vec<String> = zones.iter().filter(|z| p >= z.zone.min && p <= z.zone.max).map(|z| format!("{:.0}–{:.0}%", z.zone.min, z.zone.max)).collect();
                let mut t = format!("lever {:.1}%", p);
                if !bands.is_empty() {
                    t += &format!("   axis out {:.1}%{}", out * 100.0, if frozen { " (frozen)" } else { "" });
                }
                if !inside.is_empty() {
                    t += &format!("   in zone {}", inside.join(", "));
                }
                t
            }
            None => String::new(),
        }));
    }

    /// Add-action form lists (triggers, zone choices, action kinds, pickers).
    fn refresh_add_form(&self) {
        let Some(ui) = self.ui.upgrade() else { return };
        let ms = ui.global::<MappingState>();
        let dev = self.current_device();
        let input = self.st.borrow().m.selected_input.clone();
        let zones = dev.as_ref().map(|d| ed::zones_for_input(&self.st.borrow().profile, d, &input)).unwrap_or_default();

        let mut triggers: Vec<String> = ed::TRIGGER_KINDS.iter().map(|t| t.0.to_string()).collect();
        let mut labels: Vec<String> = ed::TRIGGER_KINDS.iter().map(|t| t.1.to_string()).collect();
        if !zones.is_empty() {
            triggers.extend(["ZONE_ENTER".to_string(), "ZONE_LEAVE".to_string()]);
            labels.extend(["Zone enter".to_string(), "Zone leave".to_string()]);
        }
        let ti = (ms.get_trigger_index().max(0) as usize).min(triggers.len() - 1);
        ms.set_trigger_labels(strings(&labels));
        ms.set_trigger_index(ti as i32);
        ms.set_zone_trigger(triggers[ti].starts_with("ZONE_"));
        ms.set_hat_labels(strings(["N", "NE", "E", "SE", "S", "SW", "W", "NW"]));
        ms.set_zone_options(strings(zones.iter().map(|z| format!("{:.1}–{:.1}%", z.zone.min, z.zone.max))));
        if ms.get_zone_option() as usize >= zones.len() {
            ms.set_zone_option(0);
        }

        let mut kinds = vec![NewActionKind::SimConnect];
        let mut kind_labels = vec!["SimConnect event"];
        if !crate::pmdg::installed_sdks().is_empty() {
            kinds.push(NewActionKind::Pmdg);
            kind_labels.push("PMDG event");
        }
        kinds.extend([NewActionKind::Lvar, NewActionKind::Rpn]);
        kind_labels.extend(["LVAR", "RPN code"]);
        let ai = (ms.get_action_type().max(0) as usize).min(kinds.len() - 1);
        ms.set_action_types(strings(kind_labels));
        ms.set_action_type(ai as i32);
        ms.set_action_kind(match kinds[ai] {
            NewActionKind::SimConnect => 0,
            NewActionKind::Pmdg => 1,
            NewActionKind::Lvar => 2,
            NewActionKind::Rpn => 3,
        });
        ms.set_comparators(strings(ed::COMPARATORS));
        ms.set_rpn_templates(strings(COMMON_CALCULATOR_CODES.iter().map(|(_, label)| *label)));
        ms.set_pmdg_families(strings(crate::pmdg::installed_sdks().iter().map(|s| s.family.label())));
        ms.set_pmdg_params(strings(ed::PMDG_PARAMS.iter().map(|p| p.0)));
        let mut cats = vec!["All categories"];
        cats.extend(categories());
        ms.set_event_categories(strings(cats));
        {
            let mut st = self.st.borrow_mut();
            st.m.triggers = triggers;
            st.m.zone_keys = zones.iter().map(|z| z.zone.key()).collect();
            st.m.kinds = kinds;
        }
        self.refresh_event_list();
        self.refresh_pmdg_list();
        self.refresh_lvar_suggestions();
    }

    fn refresh_event_list(&self) {
        let Some(ui) = self.ui.upgrade() else { return };
        let ms = ui.global::<MappingState>();
        let query = ms.get_event_query().trim().trim_start_matches("SIMCONNECT:").to_ascii_uppercase();
        let cats = categories();
        let cat = (ms.get_event_category() > 0).then(|| cats.get(ms.get_event_category() as usize - 1).copied()).flatten();
        let words: Vec<&str> = query.split(|c: char| c.is_whitespace() || c == '_').filter(|w| !w.is_empty()).collect();
        let exact = SIM_EVENTS.iter().any(|e| e.name == query);
        let list: Vec<PickItem> = SIM_EVENTS
            .iter()
            .filter(|e| cat.is_none_or(|c| c == e.category))
            .filter(|e| exact || words.iter().all(|w| e.name.contains(w)))
            .take(600)
            .map(|e| PickItem { name: ss(e.name), detail: ss(e.category) })
            .collect();
        ms.set_event_hint(ss(if query.is_empty() { "Type an event name." } else { "Not in the catalog: the name you typed is sent as-is." }));
        ms.set_event_list(model(list));
    }

    fn refresh_pmdg_list(&self) {
        let Some(ui) = self.ui.upgrade() else { return };
        let ms = ui.global::<MappingState>();
        let sdks = crate::pmdg::installed_sdks();
        let Some(sdk) = sdks.get(ms.get_pmdg_family().max(0) as usize) else {
            ms.set_pmdg_events(strings(Vec::<String>::new()));
            ms.set_pmdg_count(ss(""));
            return;
        };
        let words: Vec<String> = ms.get_pmdg_query().split_whitespace().map(|w| w.to_ascii_uppercase()).collect();
        let mut names: Vec<&String> = sdk.events.keys().filter(|n| words.iter().all(|w| n.contains(w.as_str()))).collect();
        names.sort();
        ms.set_pmdg_count(ss(format!("{} events", names.len())));
        ms.set_pmdg_events(strings(names));
        ms.set_pmdg_selected(ss(&self.st.borrow().m.pmdg_event));
    }

    fn refresh_lvar_suggestions(&self) {
        let Some(ui) = self.ui.upgrade() else { return };
        let ms = ui.global::<MappingState>();
        let q = ms.get_lvar_target().trim().trim_start_matches("LVAR:").to_lowercase();
        let st = self.st.borrow();
        let list: Vec<&String> = st.lvars.iter().filter(|l| q.is_empty() || l.to_lowercase().contains(&q)).take(300).collect();
        ms.set_lvar_suggestions(strings(list));
    }

    /// The action described by the add-action form.
    fn form_action(&self) -> Option<NewAction> {
        let ui = self.ui.upgrade()?;
        let ms = ui.global::<MappingState>();
        let st = self.st.borrow();
        let kind = *st.m.kinds.get(ms.get_action_type().max(0) as usize)?;
        let base = st.m.triggers.get(ms.get_trigger_index().max(0) as usize).cloned().unwrap_or_else(|| "PRESS".to_string());
        let zone_trigger = base.starts_with("ZONE_");
        let trigger = if ed::is_hat_input_name(&st.m.selected_input) && !zone_trigger {
            let dir = ed::HAT_DIRECTIONS.get(ms.get_hat_index().max(0) as usize).copied().unwrap_or("NORTH");
            format!("POV_{}_{}", dir, base)
        } else {
            base
        };
        let zone = if zone_trigger { st.m.zone_keys.get(ms.get_zone_option().max(0) as usize).cloned() } else { None };
        let (target, value) = match kind {
            NewActionKind::SimConnect => (ms.get_event_query().to_string(), ms.get_action_value().to_string()),
            NewActionKind::Lvar => (ms.get_lvar_target().to_string(), ms.get_action_value().to_string()),
            NewActionKind::Rpn => (ms.get_rpn_code().to_string(), String::new()),
            NewActionKind::Pmdg => {
                let sdk = crate::pmdg::installed_sdks().get(ms.get_pmdg_family().max(0) as usize)?;
                if st.m.pmdg_event.is_empty() {
                    return None;
                }
                let param = ed::PMDG_PARAMS.get(ms.get_pmdg_param().max(0) as usize).map(|p| p.1).unwrap_or(crate::pmdg::MOUSE_FLAG_LEFTSINGLE);
                (format!("{}:{}", sdk.family.profile_prefixes()[0], st.m.pmdg_event), param.to_string())
            }
        };
        let condition = ms.get_use_condition().then(|| {
            (
                ms.get_cond_target().to_string(),
                ed::COMPARATORS.get(ms.get_cond_comp().max(0) as usize).unwrap_or(&"Equals").to_string(),
                ms.get_cond_value().to_string(),
            )
        });
        Some(NewAction { kind, trigger, zone, target, value, condition })
    }

    fn with_lever(&self, f: impl FnOnce(&mut EventActionRangedAxis, i32, i32, Option<&LiveAxis>)) {
        let Some(dev) = self.current_device() else { return };
        let input = self.st.borrow().m.selected_input.clone();
        let live = self.live_for(&dev, &input);
        let (lo, hi) = dev_range(live.as_ref());
        let mut st = self.st.borrow_mut();
        if let Some((_, lv)) = st.m.lever.as_mut() {
            f(lv, lo, hi, live.as_ref());
            st.m.lever_touched = true;
        }
    }

    /// Applies a profile edit to the selected input and saves.
    fn edit_profile(self: &Rc<Self>, f: impl FnOnce(&mut Option<SpadProfile>, &ConnectedDevice, &str) -> bool) {
        let Some(dev) = self.current_device() else { return };
        let input = self.st.borrow().m.selected_input.clone();
        if self.st.borrow().profile.is_none() {
            self.toast("Load or create a profile first", 3);
            return;
        }
        let changed = {
            let mut st = self.st.borrow_mut();
            f(&mut st.profile, &dev, &input)
        };
        if changed {
            self.save_active_profile();
            self.refresh_mapping_all();
            self.refresh_devices();
            self.refresh_shell();
        }
    }
}

pub(crate) fn wire(app: &Rc<App>, ui: &AppWindow) {
    let ms = ui.global::<MappingState>();
    ms.set_zone_enter(ss(ed::ZONE_PRESETS[0].1));
    ms.set_zone_leave(ss(ed::ZONE_PRESETS[0].2));

    let a = app.clone();
    ms.on_select_device(move |i| {
        {
            let mut st = a.st.borrow_mut();
            let dev = st.devices.get(st.m.device_idx).cloned();
            st.m.stash_lever_draft(dev.as_ref());
            st.m.device_idx = i.max(0) as usize;
            st.m.selected_input.clear();
        }
        a.refresh_mapping_all();
    });
    let a = app.clone();
    ms.on_refilter(move || a.refresh_inputs());
    let a = app.clone();
    ms.on_select_input(move |id| a.select_input(id.as_str()));
    let a = app.clone();
    ms.on_deselect(move || {
        a.st.borrow_mut().m.selected_input.clear();
        a.refresh_mapping_all();
    });
    let a = app.clone();
    ms.on_axis_mode_changed(move |_| a.refresh_mapping_all());
    let a = app.clone();
    ms.on_invert_changed(move || a.refresh_mapping_live());
    let a = app.clone();
    ms.on_apply_axis(move || {
        let Some(ui) = a.ui.upgrade() else { return };
        let ms = ui.global::<MappingState>();
        let preset = SIMCONNECT_AXIS_PRESETS.get(ms.get_axis_preset().max(0) as usize).map(|p| p.name).unwrap_or("AXIS_AILERONS_SET");
        let invert = ms.get_axis_invert();
        a.edit_profile(|p, d, i| {
            ed::set_axis_action(p, d, i, preset, invert);
            true
        });
        a.toast(format!("{} bound", preset), 2);
    });

    // Lever
    let a = app.clone();
    ms.on_lever_kind_changed(move |i| {
        if let Some((id, _)) = ed::LEVER_KINDS.get(i.max(0) as usize) {
            a.with_lever(|lv, lo, hi, _| {
                ed::lever_set_kind(lv, id);
                if lever::has_reverse(lv) && lv.option("REVERSE").is_none() {
                    let far = ed::lever_far_end(lv, lo, hi);
                    lv.set_option("REVERSE", far.to_string());
                    lv.set_option("REVERSETO", far.to_string());
                }
            });
        }
        a.refresh_selected();
        a.refresh_mapping_live();
    });
    let a = app.clone();
    ms.on_toggle_engine(move |i| {
        a.with_lever(|lv, _, _, _| {
            let key = format!("OPTION{}", i + 1);
            let on = lv.option_bool(&key);
            lv.set_option(&key, if on { "False" } else { "True" });
        });
        a.refresh_selected();
        a.refresh_mapping_live();
    });
    let a = app.clone();
    ms.on_lever_edit(move |key, to, text| {
        if let Ok(v) = text.trim().parse::<i32>() {
            let k = if to { format!("{}TO", key) } else { key.to_string() };
            a.with_lever(|lv, _, _, _| lv.set_option(&k, v.to_string()));
            a.refresh_mapping_live();
            if let Some(ui) = a.ui.upgrade() {
                ui.global::<MappingState>().set_lever_saved(false);
            }
        }
    });
    let a = app.clone();
    ms.on_lever_set_here(move |key| {
        a.with_lever(|lv, lo, hi, live| {
            if let Some(l) = live {
                ed::lever_set_here(lv, key.as_str(), l.raw, lo, hi);
            }
        });
        a.refresh_selected();
        a.refresh_mapping_live();
    });
    let a = app.clone();
    ms.on_gate_changed(move |v| {
        a.with_lever(|lv, _, _, _| lv.set_option("REVERSEGATE", format!("{:.0}", v)));
        a.refresh_mapping_live();
        if let Some(ui) = a.ui.upgrade() {
            ui.global::<MappingState>().set_lever_saved(false);
        }
    });
    let a = app.clone();
    ms.on_save_lever(move || {
        let action = a.st.borrow().m.lever.as_ref().map(|(_, l)| l.clone());
        if let Some(action) = action {
            a.st.borrow_mut().m.lever_touched = false;
            a.edit_profile(|p, d, i| {
                ed::set_value_definition(p, d, i, vec![EventActionItem::RangedAxis(action)]);
                true
            });
            a.toast("Lever saved to the profile", 2);
        }
    });
    let a = app.clone();
    ms.on_reset_lever(move || {
        {
            let mut st = a.st.borrow_mut();
            st.m.lever = None;
            st.m.lever_touched = false;
        }
        a.refresh_selected();
        a.refresh_mapping_live();
    });

    // Zones
    let a = app.clone();
    ms.on_delete_zone(move |key| {
        a.edit_profile(|p, d, i| {
            ed::remove_zone(p, d, i, key.as_str());
            true
        });
    });
    let a = app.clone();
    ms.on_zone_draft_changed(move || {
        a.sync_zone_draft();
        a.refresh_mapping_all();
    });
    let a = app.clone();
    ms.on_zone_here(move |to| {
        let Some(dev) = a.current_device() else { return };
        let input = a.st.borrow().m.selected_input.clone();
        if let (Some(l), Some(ui)) = (a.live_for(&dev, &input), a.ui.upgrade()) {
            let p = (axis_position(l.raw, l.min, l.max) * 1000.0).round() / 10.0;
            let ms = ui.global::<MappingState>();
            if to {
                ms.set_zone_to(ss(format!("{}", p)));
            } else {
                ms.set_zone_from(ss(format!("{}", p)));
            }
            a.sync_zone_draft();
            a.refresh_mapping_all();
        }
    });
    let a = app.clone();
    ms.on_zone_preset_changed(move |i| {
        if let (Some(p), Some(ui)) = (ed::ZONE_PRESETS.get(i.max(0) as usize), a.ui.upgrade()) {
            if !p.1.is_empty() {
                let ms = ui.global::<MappingState>();
                ms.set_zone_enter(ss(p.1));
                ms.set_zone_leave(ss(p.2));
            }
        }
        a.sync_zone_draft();
        a.refresh_mapping_all();
    });
    let a = app.clone();
    ms.on_add_zone(move || {
        a.sync_zone_draft();
        let draft = a.st.borrow().m.zone.clone();
        a.edit_profile(|p, d, i| {
            ed::add_zone(p, d, i, &draft);
            true
        });
        a.toast("Trigger zone added", 2);
    });

    // Actions
    let a = app.clone();
    ms.on_delete_action(move |di, ai| {
        a.edit_profile(|p, d, i| {
            ed::remove_action(p, d, i, di.max(0) as usize, ai.max(0) as usize);
            true
        });
    });
    let a = app.clone();
    ms.on_test_action(move |di, ai| {
        let Some(dev) = a.current_device() else { return };
        let input = a.st.borrow().m.selected_input.clone();
        let cmd = {
            let st = a.st.borrow();
            st.profile
                .as_ref()
                .and_then(|p| p.find_device_for_connected(dev.vendor_id, dev.product_id, &dev.id))
                .and_then(|dp| {
                    dp.pages
                        .iter()
                        .filter_map(|p| p.events.as_ref())
                        .flat_map(|e| &e.items)
                        .find(|e| crate::profile::schema::bound_to_matches(&e.bound_to, &input))
                })
                .and_then(|e| e.event_definitions.as_ref())
                .and_then(|defs| defs.items.get(di.max(0) as usize))
                .and_then(|def| {
                    let act = def.actions.as_ref()?.items.get(ai.max(0) as usize)?;
                    ed::test_command(act, def.trigger.as_deref().unwrap_or("PRESS"))
                })
        };
        if let Some(cmd) = cmd {
            a.send(cmd);
            a.toast("Sent to the simulator", 2);
        }
    });
    let a = app.clone();
    ms.on_open_add(move || {
        if let Some(ui) = a.ui.upgrade() {
            ui.global::<MappingState>().set_show_add(true);
        }
        a.refresh_add_form();
    });
    let a = app.clone();
    ms.on_trigger_changed(move |_| a.refresh_add_form());
    let a = app.clone();
    ms.on_action_type_changed(move |_| a.refresh_add_form());
    let a = app.clone();
    ms.on_event_filter_changed(move || a.refresh_event_list());
    let a = app.clone();
    ms.on_pick_event(move |name| {
        if let Some(ui) = a.ui.upgrade() {
            ui.global::<MappingState>().set_event_query(name);
        }
        a.refresh_event_list();
    });
    let a = app.clone();
    ms.on_pmdg_filter_changed(move || a.refresh_pmdg_list());
    let a = app.clone();
    ms.on_pick_pmdg(move |name| {
        a.st.borrow_mut().m.pmdg_event = name.to_string();
        if let Some(ui) = a.ui.upgrade() {
            ui.global::<MappingState>().set_pmdg_selected(name);
        }
    });
    let a = app.clone();
    ms.on_lvar_filter_changed(move || a.refresh_lvar_suggestions());
    let a = app.clone();
    ms.on_pick_lvar(move |name| {
        if let Some(ui) = a.ui.upgrade() {
            ui.global::<MappingState>().set_lvar_target(ss(format!("LVAR:{}", name)));
        }
    });
    let a = app.clone();
    ms.on_pick_template(move |i| {
        if let (Some((code, _)), Some(ui)) = (COMMON_CALCULATOR_CODES.get(i.max(0) as usize), a.ui.upgrade()) {
            ui.global::<MappingState>().set_rpn_code(ss(*code));
        }
    });
    let a = app.clone();
    ms.on_save_action(move || {
        let Some(new) = a.form_action() else {
            a.toast("Pick an event first", 3);
            return;
        };
        let mut added = false;
        a.edit_profile(|p, d, i| {
            added = ed::add_action(p, d, i, &new);
            added
        });
        if added {
            if let Some(ui) = a.ui.upgrade() {
                ui.global::<MappingState>().set_show_add(false);
            }
            a.toast(format!("{} → {}", ed::trigger_label(&new.trigger), new.target.trim()), 2);
        } else {
            a.toast("Nothing to save: fill in the action", 3);
        }
    });
    let a = app.clone();
    ms.on_test_new_action(move || {
        if let Some(cmd) = a.form_action().and_then(|n| n.test_command()) {
            a.send(cmd);
            a.toast("Sent to the simulator", 2);
        }
    });
}

impl App {
    fn sync_zone_draft(&self) {
        let Some(ui) = self.ui.upgrade() else { return };
        let ms = ui.global::<MappingState>();
        let mut st = self.st.borrow_mut();
        let z = &mut st.m.zone;
        if let Ok(v) = ms.get_zone_from().trim().trim_end_matches('%').parse::<f64>() {
            z.min = v.clamp(0.0, 100.0);
        }
        if let Ok(v) = ms.get_zone_to().trim().trim_end_matches('%').parse::<f64>() {
            z.max = v.clamp(0.0, 100.0);
        }
        z.deadzone = (ms.get_zone_deadzone() as f64 * 2.0).round() / 2.0;
        z.enter_event = ms.get_zone_enter().to_string();
        z.leave_event = ms.get_zone_leave().to_string();
    }
}
