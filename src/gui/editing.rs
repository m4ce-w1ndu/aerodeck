//! UI-independent editing of a profile: inputs, actions, calibrated levers and trigger zones,
//! plus the labels the interface shows for them.

use crate::hardware::enumerator::ConnectedDevice;
use crate::profile::schema::{
    AxisZone, DevicePage, EventActionChangeValue, EventActionControl, EventActionCustomAxis, EventActionItem,
    EventActionRangedAxis, EventActionScript, EventActionsList, EventConditionItem, EventConditionSimple,
    EventConditionsList, EventDefinition, EventDefinitionsList, EventList, ProfileOption, ProfileOptionsList, SpadEvent,
    SpadProfile,
};

/// Calibrated lever kinds: (SPAD ConfigID, label).
pub const LEVER_KINDS: [(&str, &str); 6] = [
    ("LEVER_NOREVERSE_THROTTLE", "Throttle"),
    ("LEVER_THROTTLE", "Throttle + reverse"),
    ("LEVER_MIXTURE", "Mixture"),
    ("LEVER_PROPELLER", "Propeller"),
    ("LEVER_FLAPS", "Flaps"),
    ("LEVER_SPOILER", "Speedbrake"),
];

/// Trigger kinds offered for buttons: (id, label, tooltip).
pub const TRIGGER_KINDS: [(&str, &str, &str); 5] = [
    ("PRESS", "Press", "Fires as soon as the button goes down"),
    ("RELEASE", "Release", "Fires when the button is let go"),
    ("PRESSSHORT", "Short press", "Fires on release if held less than half a second"),
    ("PRESSLONG", "Long press", "Fires once after holding for half a second"),
    ("HELD", "Held (repeat)", "Fires on press, then repeats every 100 ms while held"),
];

pub const HAT_DIRECTIONS: [&str; 8] = ["NORTH", "NORTHEAST", "EAST", "SOUTHEAST", "SOUTH", "SOUTHWEST", "WEST", "NORTHWEST"];

/// PMDG event parameters: (label, value). Values are PMDG mouse flags.
pub const PMDG_PARAMS: [(&str, u32); 4] = [
    ("Click", crate::pmdg::MOUSE_FLAG_LEFTSINGLE),
    ("Wheel up", crate::pmdg::MOUSE_FLAG_WHEEL_UP),
    ("Wheel down", crate::pmdg::MOUSE_FLAG_WHEEL_DOWN),
    ("Release", crate::pmdg::MOUSE_FLAG_LEFTRELEASE),
];

pub const COMPARATORS: [&str; 4] = ["Equals", "Unequal", "Less", "Greater"];

/// Ready-made zone actions: (label, enter event, leave event).
pub const ZONE_PRESETS: [(&str, &str, &str); 3] = [
    ("Spoilers ARM", "SPOILERS_ARM_ON", "SPOILERS_ARM_OFF"),
    ("Spoilers ARM (toggle)", "SPOILERS_ARM_TOGGLE", "SPOILERS_ARM_TOGGLE"),
    ("Custom", "", ""),
];

pub fn title_case(s: &str) -> String {
    let lower = s.to_ascii_lowercase();
    let mut c = lower.chars();
    c.next().map(|f| f.to_ascii_uppercase().to_string() + c.as_str()).unwrap_or_default()
}

/// Human label for a trigger id: `PRESSLONG` -> `Long press`, `POV_NORTH_HELD` -> `Hat North · Held (repeat)`.
pub fn trigger_label(trigger: &str) -> String {
    let t = trigger.to_ascii_uppercase();
    if let Some(rest) = t.strip_prefix("POV_") {
        if let Some((dir, kind)) = rest.split_once('_') {
            return format!("Hat {} · {}", title_case(dir), trigger_label(kind));
        }
    }
    match t.as_str() {
        "VALUE" => "Axis value".to_string(),
        "ZONE_ENTER" => "Zone enter".to_string(),
        "ZONE_LEAVE" => "Zone leave".to_string(),
        _ => TRIGGER_KINDS
            .iter()
            .find(|(id, _, _)| *id == t)
            .map(|(_, l, _)| l.to_string())
            .unwrap_or_else(|| title_case(&t.replace('_', " "))),
    }
}

/// Human label for an input id: `BUTTON_5` -> `Button 5`, `ABS_RZ` / `RZ_AXIS` -> `Rz axis`.
pub fn input_label(id: &str) -> String {
    let u = id.to_ascii_uppercase();
    if let Some(n) = u.strip_prefix("BUTTON_") {
        return format!("Button {}", n);
    }
    if let Some(n) = u.strip_prefix("HATSWITCH_") {
        return format!("Hat switch {}", n);
    }
    let axis = u.strip_prefix("ABS_").or_else(|| u.strip_suffix("_AXIS")).or_else(|| u.strip_prefix("AXIS_"));
    match axis {
        Some("THROTTLE") => "Throttle axis".to_string(),
        Some("RUDDER") => "Rudder axis".to_string(),
        Some(a) if a.len() <= 2 => format!("{} axis", title_case(a)),
        _ => id.to_string(),
    }
}

/// True for analog axis names: Linux evdev (`ABS_X`, `ABS_RZ`, ...) and SPAD.neXt (`X_AXIS`, `SLIDER_0`, ...).
pub fn is_axis_input_name(name: &str) -> bool {
    let n = name.to_ascii_uppercase();
    (n.starts_with("ABS_") && !n.starts_with("ABS_HAT"))
        || n.contains("AXIS")
        || n.starts_with("SLIDER")
        || n.contains("THROTTLE")
        || n.contains("RUDDER")
}

pub fn is_hat_input_name(name: &str) -> bool {
    name.to_ascii_uppercase().starts_with("HATSWITCH")
}

pub fn lever_kind_label(config_id: &str) -> &str {
    LEVER_KINDS.iter().find(|(id, _)| id.eq_ignore_ascii_case(config_id)).map(|(_, l)| *l).unwrap_or(config_id)
}

/// Short description of an action, for lists.
pub fn describe_action(act: &EventActionItem) -> (u8, String, String) {
    // (kind, title, detail) — kind: 0 SimConnect, 1 LVAR, 2 RPN, 3 axis, 4 lever, 5 PMDG, 6 other
    match act {
        EventActionItem::Control(c) => {
            let value = c.effective_value().map(str::trim).filter(|v| !v.is_empty()).map(|v| format!("value {}", v)).unwrap_or_default();
            if let Some((prefix, name)) = crate::profile::executor::pmdg_target(&c.target) {
                (5, name.to_string(), format!("{} · {}", prefix, value))
            } else {
                (0, c.target.strip_prefix("SIMCONNECT:").unwrap_or(&c.target).to_string(), value)
            }
        }
        EventActionItem::ChangeValue(cv) => (
            1,
            cv.target.strip_prefix("LVAR:").unwrap_or(&cv.target).to_string(),
            format!("{} {}", cv.operation.as_deref().unwrap_or("Set"), cv.value.as_deref().unwrap_or("")),
        ),
        EventActionItem::Script(sc) => (2, "RPN script".to_string(), sc.script.clone().unwrap_or_default()),
        EventActionItem::CustomAxis(ca) => {
            let inv = crate::profile::executor::option_is_true(ca.options.as_ref(), "INVERTAXIS");
            (3, ca.target.strip_prefix("SIMCONNECT:").unwrap_or(&ca.target).to_string(), if inv { "inverted".into() } else { String::new() })
        }
        EventActionItem::RangedAxis(ra) => {
            let engines: Vec<String> = (1..=4).filter(|n| ra.option_bool(&format!("OPTION{}", n))).map(|n| n.to_string()).collect();
            let eng = if engines.is_empty() { "all engines".to_string() } else { format!("engine {}", engines.join(" + ")) };
            (4, format!("Lever · {}", lever_kind_label(&ra.config_id)), eng)
        }
        _ => (6, "Action".to_string(), String::new()),
    }
}

/// Short summary of the first action of an input, for the input list.
pub fn summary_of(act: &EventActionItem) -> String {
    match act {
        EventActionItem::Control(c) => crate::profile::executor::pmdg_target(&c.target)
            .map(|(_, n)| n.to_string())
            .unwrap_or_else(|| c.target.strip_prefix("SIMCONNECT:").unwrap_or(&c.target).to_string()),
        EventActionItem::ChangeValue(cv) => format!("LVAR {}", cv.target.strip_prefix("LVAR:").unwrap_or(&cv.target)),
        EventActionItem::Script(_) => "RPN script".to_string(),
        EventActionItem::CustomAxis(ca) => ca.target.strip_prefix("SIMCONNECT:").unwrap_or(&ca.target).to_string(),
        EventActionItem::RangedAxis(ra) => format!("Lever {}", lever_kind_label(&ra.config_id).to_lowercase()),
        _ => "Action".to_string(),
    }
}

/// Command that fires an action once in the sim, if it can be tested.
pub fn test_command(act: &EventActionItem, trigger: &str) -> Option<crate::gui::backend::GuiCommand> {
    use crate::gui::backend::GuiCommand;
    match act {
        EventActionItem::Control(c) => {
            let param = c.effective_value().map(str::trim).filter(|v| !v.is_empty());
            if let Some((prefix, name)) = crate::profile::executor::pmdg_target(&c.target) {
                let parameter = param
                    .and_then(|v| v.parse::<i64>().ok())
                    .map(|v| v as u32)
                    .unwrap_or_else(|| crate::pmdg::default_parameter(trigger));
                Some(GuiCommand::FirePmdgEvent { prefix: prefix.to_string(), name: name.to_string(), parameter })
            } else {
                let value = param.and_then(|v| v.parse::<i64>().ok()).map(|v| v as u32).unwrap_or(1);
                Some(GuiCommand::FireSimConnectEvent { event_name: c.target.strip_prefix("SIMCONNECT:").unwrap_or(&c.target).to_string(), value })
            }
        }
        EventActionItem::ChangeValue(cv) => Some(GuiCommand::SetLvar {
            name: cv.target.strip_prefix("LVAR:").unwrap_or(&cv.target).to_string(),
            value: cv.value.as_deref().and_then(|v| v.parse::<f64>().ok()).unwrap_or(1.0),
        }),
        EventActionItem::Script(sc) => Some(GuiCommand::ExecCalculatorCode { code: sc.script.clone().unwrap_or_default() }),
        _ => None,
    }
}

/// All inputs of a device: hardware report plus any extra input the profile binds. (id, is_axis)
pub fn device_inputs(profile: &Option<SpadProfile>, dev: &ConnectedDevice) -> Vec<(String, bool)> {
    let mut all: Vec<(String, bool)> = dev.buttons.iter().map(|b| (b.clone(), false)).collect();
    all.extend(dev.axes.iter().map(|a| (a.clone(), true)));
    if let Some(dp) = profile.as_ref().and_then(|p| p.find_device_for_connected(dev.vendor_id, dev.product_id, &dev.id)) {
        for ev in dp.pages.iter().filter_map(|p| p.events.as_ref()).flat_map(|e| &e.items) {
            if !all.iter().any(|(id, _)| id.eq_ignore_ascii_case(&ev.bound_to)) {
                all.push((ev.bound_to.clone(), is_axis_input_name(&ev.bound_to)));
            }
        }
    }
    all
}

/// Event definitions bound to `input` on the device (all pages).
pub fn input_definitions<'a>(profile: &'a Option<SpadProfile>, dev: &ConnectedDevice, input: &str) -> Vec<&'a EventDefinition> {
    let Some(dp) = profile.as_ref().and_then(|p| p.find_device_for_connected(dev.vendor_id, dev.product_id, &dev.id)) else {
        return Vec::new();
    };
    dp.pages
        .iter()
        .filter_map(|p| p.events.as_ref())
        .flat_map(|e| &e.items)
        .filter(|e| crate::profile::schema::bound_to_matches(&e.bound_to, input))
        .filter_map(|e| e.event_definitions.as_ref())
        .flat_map(|d| &d.items)
        .collect()
}

/// (action count, summary of the first action) for an input.
pub fn input_summary(profile: &Option<SpadProfile>, dev: &ConnectedDevice, input: &str) -> (usize, Option<String>) {
    let Some(dp) = profile.as_ref().and_then(|p| p.find_device_for_connected(dev.vendor_id, dev.product_id, &dev.id)) else {
        return (0, None);
    };
    let ev = dp
        .pages
        .iter()
        .filter_map(|p| p.events.as_ref())
        .flat_map(|e| &e.items)
        .find(|e| crate::profile::schema::bound_to_matches(&e.bound_to, input));
    let Some(defs) = ev.and_then(|e| e.event_definitions.as_ref()) else { return (0, None) };
    let count = defs.items.iter().map(|d| d.actions.as_ref().map(|a| a.items.len()).unwrap_or(0)).sum();
    let summary = defs.items.iter().find_map(|d| d.actions.as_ref().and_then(|a| a.items.first())).map(summary_of);
    (count, summary)
}

/// Event definitions of `bound_to` on the connected device's first page, created if missing.
pub fn input_definitions_mut<'a>(
    profile: &'a mut Option<SpadProfile>,
    conn_dev: &ConnectedDevice,
    bound_to: &str,
) -> Option<&'a mut EventDefinitionsList> {
    let prof = profile.as_mut()?;
    let dev_prof = prof.ensure_device_for_connected(conn_dev.vendor_id, conn_dev.product_id, &conn_dev.id, &conn_dev.name, None);
    if dev_prof.pages.is_empty() {
        dev_prof.pages.push(DevicePage {
            id: Some("Default".to_string()),
            is_default: Some(true),
            name: Some("Default".to_string()),
            events: Some(EventList { items: Vec::new() }),
        });
    }
    let events = dev_prof.pages[0].events.get_or_insert_with(|| EventList { items: Vec::new() });
    let idx = match events.items.iter().position(|e| crate::profile::schema::bound_to_matches(&e.bound_to, bound_to)) {
        Some(i) => i,
        None => {
            events.items.push(SpadEvent {
                bound_to: bound_to.to_string(),
                config_id: None,
                event_definitions: Some(EventDefinitionsList { items: Vec::new() }),
            });
            events.items.len() - 1
        }
    };
    Some(events.items[idx].event_definitions.get_or_insert_with(|| EventDefinitionsList { items: Vec::new() }))
}

/// Replaces the axis (`VALUE`) definitions of an input with one holding `actions`, keeping
/// button-style definitions and trigger zones bound to the same input.
pub fn set_value_definition(profile: &mut Option<SpadProfile>, conn_dev: &ConnectedDevice, bound_to: &str, actions: Vec<EventActionItem>) {
    let Some(defs) = input_definitions_mut(profile, conn_dev, bound_to) else { return };
    defs.items.retain(|d| !d.trigger.as_deref().unwrap_or("").eq_ignore_ascii_case("VALUE"));
    defs.items.insert(
        0,
        EventDefinition {
            trigger: Some("VALUE".to_string()),
            priority: None,
            is_final: Some(true),
            options: None,
            condition_binding: None,
            conditions: None,
            actions: Some(EventActionsList { items: actions }),
        },
    );
}

pub fn set_axis_action(profile: &mut Option<SpadProfile>, conn_dev: &ConnectedDevice, bound_to: &str, event_name: &str, invert: bool) {
    let action = EventActionItem::CustomAxis(EventActionCustomAxis {
        target: format!("SIMCONNECT:{}", event_name),
        value: None,
        options: invert.then(|| ProfileOptionsList { items: vec![ProfileOption { key: "INVERTAXIS".to_string(), value: Some("True".to_string()) }] }),
    });
    set_value_definition(profile, conn_dev, bound_to, vec![action]);
}

/// Removes one action; definitions left without actions are dropped.
pub fn remove_action(profile: &mut Option<SpadProfile>, conn_dev: &ConnectedDevice, input: &str, def_idx: usize, act_idx: usize) {
    let Some(prof) = profile.as_mut() else { return };
    let Some(dev_prof) = prof.devices.as_mut().and_then(|d| d.items.iter_mut().find(|dev| dev.matches_vid_pid(conn_dev.vendor_id, conn_dev.product_id))) else {
        return;
    };
    for ev in dev_prof.pages.iter_mut().filter_map(|p| p.events.as_mut()).flat_map(|e| e.items.iter_mut()) {
        if !crate::profile::schema::bound_to_matches(&ev.bound_to, input) {
            continue;
        }
        let Some(defs) = ev.event_definitions.as_mut() else { continue };
        if let Some(acts) = defs.items.get_mut(def_idx).and_then(|d| d.actions.as_mut()) {
            if act_idx < acts.items.len() {
                acts.items.remove(act_idx);
            }
        }
        if defs.items.get(def_idx).is_some_and(|d| d.actions.as_ref().is_none_or(|a| a.items.is_empty())) {
            defs.items.remove(def_idx);
        }
    }
}

/// Kind of a new action in the add-action form.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NewActionKind {
    SimConnect,
    Pmdg,
    Lvar,
    Rpn,
}

/// Everything the add-action form collects.
#[derive(Debug, Clone)]
pub struct NewAction {
    pub kind: NewActionKind,
    /// Full trigger id (`PRESS`, `POV_NORTH_HELD`, `ZONE_ENTER`, ...)
    pub trigger: String,
    /// Zone key for ZONE_ENTER / ZONE_LEAVE
    pub zone: Option<String>,
    /// SimConnect event, LVAR name, RPN code or PMDG target (`PMDGNG3:EVT_...`)
    pub target: String,
    pub value: String,
    pub condition: Option<(String, String, String)>,
}

impl NewAction {
    fn item(&self) -> EventActionItem {
        match self.kind {
            NewActionKind::Pmdg => EventActionItem::Control(EventActionControl { target: self.target.clone(), value: Some(self.value.clone()), parameters: Vec::new() }),
            NewActionKind::SimConnect => {
                // Keep explicit providers (SIMCONNECT:, PMDGNG3:, ...) as typed.
                let t = self.target.trim();
                let target = if crate::profile::evaluator::split_data_prefix(t).is_some() { t.to_string() } else { format!("SIMCONNECT:{}", t) };
                EventActionItem::Control(EventActionControl { target, value: Some(self.value.clone()), parameters: Vec::new() })
            }
            NewActionKind::Lvar => {
                let t = self.target.trim();
                let target = if t.starts_with("LVAR:") { t.to_string() } else { format!("LVAR:{}", t) };
                EventActionItem::ChangeValue(EventActionChangeValue { target, operation: Some("Set".to_string()), value: Some(self.value.clone()) })
            }
            NewActionKind::Rpn => EventActionItem::Script(EventActionScript { script: Some(self.target.clone()), language: Some("RPN".to_string()) }),
        }
    }

    /// Fires the action once in the sim.
    pub fn test_command(&self) -> Option<crate::gui::backend::GuiCommand> {
        use crate::gui::backend::GuiCommand;
        Some(match self.kind {
            NewActionKind::Pmdg => {
                let (prefix, name) = self.target.split_once(':')?;
                GuiCommand::FirePmdgEvent { prefix: prefix.to_string(), name: name.to_string(), parameter: self.value.parse().unwrap_or(crate::pmdg::MOUSE_FLAG_LEFTSINGLE) }
            }
            NewActionKind::SimConnect => GuiCommand::FireSimConnectEvent {
                event_name: self.target.trim().strip_prefix("SIMCONNECT:").unwrap_or(self.target.trim()).to_string(),
                value: self.value.trim().parse::<u32>().unwrap_or(1),
            },
            NewActionKind::Lvar => GuiCommand::SetLvar {
                name: self.target.trim().strip_prefix("LVAR:").unwrap_or(self.target.trim()).to_string(),
                value: self.value.trim().parse::<f64>().unwrap_or(1.0),
            },
            NewActionKind::Rpn => GuiCommand::ExecCalculatorCode { code: self.target.clone() },
        })
    }
}

/// Adds an action to an input. Returns false when nothing could be added.
pub fn add_action(profile: &mut Option<SpadProfile>, conn_dev: &ConnectedDevice, input: &str, new: &NewAction) -> bool {
    if new.target.trim().is_empty() {
        return false;
    }
    let Some(defs) = input_definitions_mut(profile, conn_dev, input) else { return false };
    let action_item = new.item();
    let conditions = new.condition.as_ref().map(|(t, c, v)| EventConditionsList {
        items: vec![EventConditionItem::Simple(EventConditionSimple { target: t.clone(), comparator: Some(c.clone()), value: Some(v.clone()) })],
    });

    // Zone triggers carry the zone of the definition they belong to.
    let zone_trigger = new.trigger.starts_with("ZONE_");
    let zone_options = if zone_trigger {
        let Some(zone) = new.zone.as_ref().and_then(|key| defs.items.iter().filter_map(AxisZone::of).find(|z| &z.key() == key)) else {
            return false;
        };
        Some(zone.to_options())
    } else {
        None
    };
    let same_zone = |d: &EventDefinition| match &new.zone {
        Some(key) if zone_trigger => AxisZone::of(d).is_some_and(|z| &z.key() == key),
        _ => true,
    };

    // Same trigger, no condition: add to the existing definition (one definition per trigger)
    // instead of stacking IsFinal definitions where only the first would run.
    defs.items.retain(|d| d.actions.as_ref().is_some_and(|a| !a.items.is_empty()));
    if conditions.is_none() {
        if let Some(existing) = defs.items.iter_mut().find(|d| {
            d.conditions.as_ref().is_none_or(|c| c.items.is_empty())
                && same_zone(d)
                && crate::profile::executor::trigger_matches(d.trigger.as_deref().unwrap_or(""), &new.trigger)
        }) {
            existing.actions.get_or_insert_with(|| EventActionsList { items: Vec::new() }).items.push(action_item);
            return true;
        }
    }
    defs.items.push(EventDefinition {
        trigger: Some(new.trigger.clone()),
        priority: None,
        is_final: Some(zone_options.is_none()),
        options: zone_options,
        condition_binding: None,
        conditions,
        actions: Some(EventActionsList { items: vec![action_item] }),
    });
    true
}

/// First calibrated lever bound to `input` on the device.
pub fn existing_lever(profile: &Option<SpadProfile>, dev: &ConnectedDevice, input: &str) -> Option<EventActionRangedAxis> {
    input_definitions(profile, dev, input).into_iter().flat_map(|d| d.actions.iter().flat_map(|a| &a.items)).find_map(|a| match a {
        EventActionItem::RangedAxis(r) => Some(r.clone()),
        _ => None,
    })
}

/// First custom axis bound to `input`: (event, inverted).
pub fn existing_axis(profile: &Option<SpadProfile>, dev: &ConnectedDevice, input: &str) -> Option<(String, bool)> {
    input_definitions(profile, dev, input).into_iter().flat_map(|d| d.actions.iter().flat_map(|a| &a.items)).find_map(|a| match a {
        EventActionItem::CustomAxis(c) => Some((
            c.target.strip_prefix("SIMCONNECT:").unwrap_or(&c.target).to_string(),
            crate::profile::executor::option_is_true(c.options.as_ref(), "INVERTAXIS"),
        )),
        _ => None,
    })
}

/// Default lever for a fresh input (Thrustmaster TCA convention: forward reads the minimum raw
/// value, the idle detent sits about 3/4 of the way back).
pub fn default_lever(dev_min: i32, dev_max: i32) -> EventActionRangedAxis {
    let idle = dev_min + (dev_max - dev_min) * 3 / 4;
    let mut a = EventActionRangedAxis { config_id: LEVER_KINDS[0].0.to_string(), options: None };
    a.set_option("USERAWVALUE", "True");
    a.set_option("MAX", dev_min.to_string());
    a.set_option("MAXTO", dev_min.to_string());
    a.set_option("IDLE", idle.to_string());
    a.set_option("IDLETO", idle.to_string());
    a
}

pub fn lever_uses_idle(lever: &EventActionRangedAxis) -> bool {
    lever.config_id.to_ascii_uppercase().contains("THROTTLE")
}

/// Low-end key for a lever kind: `IDLE` for throttles, `MIN` otherwise.
pub fn lever_low_key(lever: &EventActionRangedAxis) -> &'static str {
    if lever_uses_idle(lever) {
        "IDLE"
    } else {
        "MIN"
    }
}

pub fn lever_get(lever: &EventActionRangedAxis, key: &str) -> i32 {
    lever.option_f64(key).unwrap_or(0.0) as i32
}

/// Switches lever kind, carrying the low zone over between IDLE and MIN.
pub fn lever_set_kind(lever: &mut EventActionRangedAxis, config_id: &str) {
    let low = lever_low_key(lever);
    let (from, to) = (lever_get(lever, low), lever_get(lever, &format!("{}TO", low)));
    lever.config_id = config_id.to_string();
    let k = lever_low_key(lever);
    lever.set_option(k, from.to_string());
    lever.set_option(&format!("{}TO", k), to.to_string());
}

/// Far end of the travel past the idle detent (where full reverse defaults to).
pub fn lever_far_end(lever: &EventActionRangedAxis, dev_min: i32, dev_max: i32) -> i32 {
    if lever_get(lever, "MAX") <= lever_get(lever, "IDLE") {
        dev_min.max(dev_max)
    } else {
        dev_min.min(dev_max)
    }
}

/// "Set here" on a lever zone row.
pub fn lever_set_here(lever: &mut EventActionRangedAxis, key: &str, raw: i32, dev_min: i32, dev_max: i32) {
    // A small zone around the detent so it is easy to hit.
    let pad = ((dev_max - dev_min).abs() / 100).max(1);
    if key == "REVERSE" {
        let far = lever_far_end(lever, dev_min, dev_max);
        lever.set_option("REVERSE", raw.to_string());
        lever.set_option("REVERSETO", far.to_string());
    } else {
        let toward_full = if key == "MAX" { 0 } else { (lever_get(lever, "MAX") - raw).signum() * pad * 2 };
        lever.set_option(key, raw.to_string());
        lever.set_option(&format!("{}TO", key), (raw + toward_full).to_string());
    }
}

/// A zone configured on an input, with a short description of its actions.
pub struct ZoneSummary {
    pub zone: AxisZone,
    pub enter: Vec<String>,
    pub leave: Vec<String>,
}

/// Zones on `input`, grouped by range, in travel order.
pub fn zones_for_input(profile: &Option<SpadProfile>, dev: &ConnectedDevice, input: &str) -> Vec<ZoneSummary> {
    let mut out: Vec<ZoneSummary> = Vec::new();
    for def in input_definitions(profile, dev, input) {
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
        list.extend(def.actions.iter().flat_map(|a| &a.items).map(|a| describe_action(a).1));
    }
    out.sort_by(|a, b| a.zone.min.total_cmp(&b.zone.min));
    out
}

/// Zone being drafted (percent of the raw device travel).
#[derive(Debug, Clone)]
pub struct ZoneDraft {
    pub min: f64,
    pub max: f64,
    pub deadzone: f64,
    pub enter_event: String,
    pub leave_event: String,
}

impl Default for ZoneDraft {
    fn default() -> Self {
        Self { min: 0.0, max: 8.0, deadzone: 4.0, enter_event: ZONE_PRESETS[0].1.to_string(), leave_event: ZONE_PRESETS[0].2.to_string() }
    }
}

impl ZoneDraft {
    /// Hysteresis is kept below the deadzone so the zone is left before the axis moves.
    pub fn zone(&self) -> AxisZone {
        let (min, max) = (self.min.min(self.max), self.min.max(self.max));
        AxisZone { min, max, hysteresis: (self.deadzone * 0.75).min(AxisZone::DEFAULT_HYSTERESIS), deadzone: Some(self.deadzone) }
    }
}

fn zone_definition(trigger: &str, zone: AxisZone, event: &str) -> EventDefinition {
    let target = if crate::profile::evaluator::split_data_prefix(event).is_some() { event.to_string() } else { format!("SIMCONNECT:{}", event) };
    EventDefinition {
        trigger: Some(trigger.to_string()),
        priority: None,
        is_final: Some(false),
        options: Some(zone.to_options()),
        condition_binding: None,
        conditions: None,
        actions: Some(EventActionsList { items: vec![EventActionItem::Control(EventActionControl { target, value: Some("0".to_string()), parameters: Vec::new() })] }),
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

/// File name for a profile name: unsafe characters become `_`.
pub fn profile_file_for(name: &str) -> std::path::PathBuf {
    let filename: String = name.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == ' ' { c } else { '_' }).collect();
    std::path::PathBuf::from(format!("profiles/{}.xml", filename.trim()))
}

pub fn format_value(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e12 {
        format!("{}", v as i64)
    } else {
        format!("{:.3}", v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels() {
        assert_eq!(input_label("BUTTON_5"), "Button 5");
        assert_eq!(input_label("ABS_RZ"), "Rz axis");
        assert_eq!(input_label("X_AXIS"), "X axis");
        assert_eq!(trigger_label("POV_NORTH_HELD"), "Hat North · Held (repeat)");
        assert_eq!(trigger_label("PRESSLONG"), "Long press");
        assert!(is_axis_input_name("ABS_THROTTLE"));
        assert!(!is_axis_input_name("BUTTON_1"));
    }

    #[test]
    fn lever_kind_switch_keeps_low_zone() {
        let mut l = default_lever(0, 65535);
        let idle = lever_get(&l, "IDLE");
        lever_set_kind(&mut l, "LEVER_MIXTURE");
        assert_eq!(lever_get(&l, "MIN"), idle);
        lever_set_kind(&mut l, "LEVER_THROTTLE");
        assert_eq!(lever_get(&l, "IDLE"), idle);
    }
}
