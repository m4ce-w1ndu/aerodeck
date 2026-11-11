use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::{sleep, Duration};
use tracing::{debug, info, trace, warn};

use crate::bridge::{SpadLvarBridge, CALC_RET_VOID};
use crate::simconnect::SimConnectClient;
use super::evaluator::ConditionEvaluator;
use super::schema::*;

pub struct ProfileExecutor {
    profile: SpadProfile,
    simconnect: Arc<SimConnectClient>,
    bridge: Arc<SpadLvarBridge>,
    local_vars: Arc<Mutex<HashMap<String, String>>>,
    active_page_idx: Arc<Mutex<usize>>,
    /// Buttons currently held, keyed by (device, input, trigger prefix).
    presses: std::sync::Mutex<HashMap<(String, String, String), PressState>>,
    press_counter: std::sync::atomic::AtomicU64,
    notices: std::sync::Mutex<Vec<String>>,
    noticed: std::sync::Mutex<std::collections::HashSet<String>>,
    /// Axis trigger zones: "device|input|zone" -> currently inside
    zone_state: std::sync::Mutex<HashMap<String, bool>>,
}

#[derive(Debug, Clone, Copy)]
struct PressState {
    generation: u64,
    long_fired: bool,
}

/// Hold time after which `PRESSLONG` fires (and `PRESSSHORT` no longer will).
pub const LONG_PRESS: Duration = Duration::from_millis(500);
/// `HELD` fires on press, then repeats at this interval after `HELD_DELAY`.
pub const HELD_DELAY: Duration = Duration::from_millis(300);
pub const HELD_REPEAT: Duration = Duration::from_millis(100);

impl ProfileExecutor {
    pub fn new(
        profile: SpadProfile,
        simconnect: Arc<SimConnectClient>,
        bridge: Arc<SpadLvarBridge>,
    ) -> Self {
        let mut initial_vars = HashMap::new();
        if let Some(vars) = &profile.variables {
            for v in &vars.items {
                if let Some(val) = &v.value {
                    initial_vars.insert(v.name.clone(), val.clone());
                }
            }
        }

        // Subscribe every SimVar the profile reads or writes, so conditions and displays
        // have values before the first input arrives.
        let simvars = collect_simvar_refs(&profile);
        if uses_pmdg(&profile) {
            if let Ok(rt) = tokio::runtime::Handle::try_current() {
                let sc = simconnect.clone();
                rt.spawn(async move {
                    // First call scans the MSFS packages for SDK headers.
                    let _ = tokio::task::spawn_blocking(crate::pmdg::installed_sdks).await;
                    if crate::pmdg::installed_sdks().is_empty() {
                        warn!("Profile uses PMDG controls but no PMDG SDK header was found in the MSFS packages");
                    } else if let Err(e) = sc.ensure_pmdg().await {
                        warn!("Cannot register PMDG data areas: {}", e);
                    }
                });
            }
        }
        if !simvars.is_empty() {
            if let Ok(rt) = tokio::runtime::Handle::try_current() {
                let sc = simconnect.clone();
                rt.spawn(async move {
                    info!("Subscribing {} SimVars used by the profile", simvars.len());
                    for name in simvars {
                        if let Err(e) = sc.subscribe_simvar(&name).await {
                            warn!("Cannot subscribe SimVar '{}': {}", name, e);
                        }
                    }
                });
            }
        }

        Self {
            profile,
            simconnect,
            bridge,
            local_vars: Arc::new(Mutex::new(initial_vars)),
            active_page_idx: Arc::new(Mutex::new(0)),
            presses: std::sync::Mutex::new(HashMap::new()),
            press_counter: std::sync::atomic::AtomicU64::new(0),
            notices: std::sync::Mutex::new(Vec::new()),
            noticed: std::sync::Mutex::new(std::collections::HashSet::new()),
            zone_state: std::sync::Mutex::new(HashMap::new()),
        }
    }

    /// Physical press or release with SPAD.neXt timing semantics:
    ///
    /// * `PRESS` on press, `RELEASE` on release
    /// * `PRESSSHORT` on release if held less than [`LONG_PRESS`]
    /// * `PRESSLONG` once the button has been held for [`LONG_PRESS`]
    /// * `HELD` on press, then repeatedly while held (trim, POV hats)
    ///
    /// `trigger_prefix` is empty for buttons and `POV_NORTH_`-style for hat directions.
    pub async fn handle_button(self: &Arc<Self>, device_id: &str, bound_to: &str, trigger_prefix: &str, pressed: bool) {
        let key = (device_id.to_string(), bound_to.to_string(), trigger_prefix.to_string());
        let trig = |t: &str| format!("{}{}", trigger_prefix, t);
        let defined = self.defined_triggers(device_id, bound_to).await;
        if defined.is_empty() && pressed {
            self.note_unmapped(device_id, bound_to);
        }
        let has = |t: &str| defined.iter().any(|d| d.eq_ignore_ascii_case(&trig(t)));

        if pressed {
            let generation = self.press_counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
            if let Ok(mut p) = self.presses.lock() {
                p.insert(key.clone(), PressState { generation, long_fired: false });
            }
            let _ = self.dispatch_event(device_id, bound_to, &trig("PRESS")).await;
            let wants_held = has("HELD");
            if wants_held {
                let _ = self.dispatch_event(device_id, bound_to, &trig("HELD")).await;
            }
            if wants_held || has("PRESSLONG") {
                let this = self.clone();
                tokio::spawn(async move { this.run_hold_timer(key, generation, wants_held).await });
            }
        } else {
            let state = self.presses.lock().ok().and_then(|mut p| p.remove(&key));
            if !state.is_some_and(|s| s.long_fired) {
                let _ = self.dispatch_event(device_id, bound_to, &trig("PRESSSHORT")).await;
            }
            let _ = self.dispatch_event(device_id, bound_to, &trig("RELEASE")).await;
        }
    }

    async fn run_hold_timer(self: Arc<Self>, key: (String, String, String), generation: u64, repeat_held: bool) {
        let (device_id, bound_to, prefix) = (&key.0, &key.1, &key.2);
        let start = tokio::time::Instant::now();
        let still_held = |this: &Self| {
            this.presses.lock().ok().and_then(|p| p.get(&key).copied()).is_some_and(|s| s.generation == generation)
        };
        let mut long_done = false;
        let mut next_held = start + HELD_DELAY;
        loop {
            let next_long = start + LONG_PRESS;
            let wake = match (long_done, repeat_held) {
                (false, true) => next_long.min(next_held),
                (false, false) => next_long,
                (true, true) => next_held,
                (true, false) => return,
            };
            tokio::time::sleep_until(wake).await;
            if !still_held(&self) {
                return;
            }
            let now = tokio::time::Instant::now();
            if !long_done && now >= next_long {
                long_done = true;
                if let Ok(mut p) = self.presses.lock() {
                    if let Some(s) = p.get_mut(&key) {
                        s.long_fired = true;
                    }
                }
                let _ = self.dispatch_event(device_id, bound_to, &format!("{}PRESSLONG", prefix)).await;
            }
            if repeat_held && now >= next_held {
                let _ = self.dispatch_event(device_id, bound_to, &format!("{}HELD", prefix)).await;
                next_held = now + HELD_REPEAT;
            }
        }
    }

    /// Current value of a profile-local variable (`LOCAL:name`).
    pub async fn local_var(&self, name: &str) -> Option<String> {
        self.local_vars.lock().await.get(name).cloned()
    }

    /// Every definition bound to an input on the active page, across all profile entries for
    /// the device and all `<Event>`s with that `BoundTo` (merged or imported profiles can split
    /// one input, e.g. hat north and south, over several entries).
    async fn matching_definitions(&self, device_id: &str, bound_to: &str) -> Vec<&EventDefinition> {
        let page_idx = *self.active_page_idx.lock().await;
        self.profile
            .devices
            .iter()
            .flat_map(|d| &d.items)
            .filter(|d| d.matches_device_id(device_id) && !d.pages.is_empty())
            .map(|d| &d.pages[page_idx.min(d.pages.len() - 1)])
            .flat_map(|page| page.events.iter().flat_map(|e| &e.items))
            .filter(|e| bound_to_matches(&e.bound_to, bound_to))
            .flat_map(|e| e.event_definitions.iter().flat_map(|d| &d.items))
            .collect()
    }

    /// Triggers (upper-case) defined for an input on the active page.
    async fn defined_triggers(&self, device_id: &str, bound_to: &str) -> Vec<String> {
        self.matching_definitions(device_id, bound_to)
            .await
            .into_iter()
            .map(|d| d.trigger.as_deref().unwrap_or("PRESS").to_ascii_uppercase())
            .collect()
    }

    pub fn profile(&self) -> &SpadProfile {
        &self.profile
    }

    /// Handles a DeviceEvent directly
    pub async fn handle_device_event(&self, device_id: &str, event: &crate::hardware::device::DeviceEvent) -> Result<bool> {
        use crate::hardware::device::DeviceEvent;
        match event {
            DeviceEvent::ButtonPress { bound_to } => self.dispatch_event(device_id, bound_to, "PRESS").await,
            DeviceEvent::ButtonRelease { bound_to } => self.dispatch_event(device_id, bound_to, "RELEASE").await,
            DeviceEvent::RotaryTurn { bound_to, clockwise } => {
                let trg = if *clockwise { "CLOCKWISE" } else { "COUNTER_CLOCKWISE" };
                self.dispatch_event(device_id, bound_to, trg).await
            }
            DeviceEvent::SwitchChange { bound_to, state } => {
                let trg = if *state { "ON" } else { "OFF" };
                self.dispatch_event(device_id, bound_to, trg).await
            }
        }
    }

    /// Dispatches an analog axis movement to SimConnect based on the profile mapping.
    ///
    /// `raw`, `min` and `max` are the hardware values as reported by the device; the
    /// position is rescaled to the range the target sim event expects (see [`sim_axis_range`]).
    /// Dispatches an analog axis movement to SimConnect based on the profile mapping.
    /// Returns whether anything was sent. The first time an input is unmapped, or first
    /// sends, a notice is queued for the UI (see [`Self::take_notices`]).
    pub async fn dispatch_axis_event(&self, device_id: &str, bound_to: &str, raw: i32, min: i32, max: i32) -> Result<bool> {
        let sent = self.dispatch_axis_inner(device_id, bound_to, raw, min, max).await?;
        if sent.is_empty() {
            // Inputs with only trigger zones send nothing between zone changes.
            if self.matching_definitions(device_id, bound_to).await.is_empty() {
                self.note_unmapped(device_id, bound_to);
            }
        } else {
            self.note_once(format!("sent:{}:{}", device_id, bound_to), || {
                format!("{} on {} -> {}", bound_to, device_label(device_id), sent.join(", "))
            });
        }
        Ok(!sent.is_empty())
    }

    /// Runs one definition if its conditions hold (used by axis trigger zones).
    async fn run_definition(&self, def: &EventDefinition, trigger: &str) -> Result<()> {
        if def.actions.as_ref().is_none_or(|a| a.items.is_empty()) {
            return Ok(());
        }
        let lvars = self.bridge.get_lvar_values().await;
        let simvars = self.simconnect.simvar_snapshot();
        let locals = self.local_vars.lock().await.clone();
        let pmdg = self.simconnect.pmdg_snapshot();
        let ok = ConditionEvaluator::new(&lvars, &locals).with_simvars(&simvars).with_pmdg(&pmdg).evaluate_definition(def);
        if ok {
            info!("Axis zone: Trigger='{}'", trigger);
            self.execute_actions(def, trigger, false).await?;
        }
        Ok(())
    }

    /// Notices for the UI log (unmapped inputs, first successful use), drained by the caller.
    pub fn take_notices(&self) -> Vec<String> {
        self.notices.lock().map(|mut n| std::mem::take(&mut *n)).unwrap_or_default()
    }

    fn note_once(&self, key: String, msg: impl FnOnce() -> String) {
        let first = self.noticed.lock().map(|mut s| s.insert(key)).unwrap_or(false);
        if first {
            if let Ok(mut n) = self.notices.lock() {
                n.push(msg());
            }
        }
    }

    fn note_unmapped(&self, device_id: &str, bound_to: &str) {
        let profile = self.profile.name.clone().unwrap_or_else(|| "unnamed".into());
        self.note_once(format!("unmapped:{}:{}", device_id, bound_to), || {
            format!(
                "{} on {} is not mapped in the active profile '{}' — map it in the Mapping tab or bind this aircraft to another profile",
                bound_to,
                device_label(device_id),
                profile
            )
        });
    }

    async fn dispatch_axis_inner(
        &self,
        device_id: &str,
        bound_to: &str,
        raw: i32,
        min: i32,
        max: i32,
    ) -> Result<Vec<String>> {
        let defs = self.matching_definitions(device_id, bound_to).await;
        if defs.is_empty() {
            return Ok(Vec::new());
        }

        let position = axis_position(raw, min, max);
        let mut sent: Vec<String> = Vec::new();

        // Trigger zones (ZONE_ENTER / ZONE_LEAVE), with hysteresis against chattering.
        let mut zone_defs: Vec<(AxisZone, &EventDefinition)> =
            defs.iter().filter_map(|d| AxisZone::of(d).map(|z| (z, *d))).collect();
        zone_defs.sort_by(|a, b| a.0.min.total_cmp(&b.0.min));
        let mut seen_zone_keys: Vec<String> = Vec::new();
        let mut bands: Vec<(f64, f64)> = Vec::new();
        // (zone key, entered) for every zone whose state changed with this movement
        let mut transitions: Vec<(String, bool)> = Vec::new();
        for (zone, _) in &zone_defs {
            let key = zone.key();
            if seen_zone_keys.contains(&key) {
                continue;
            }
            seen_zone_keys.push(key.clone());
            bands.extend(zone.frozen_band());
            let state_key = format!("{}|{}|{}", device_id, bound_to, key);
            let prev = self.zone_state.lock().ok().and_then(|m| m.get(&state_key).copied());
            let next = zone.next_state(prev.unwrap_or(false), position * 100.0);
            if let Ok(mut m) = self.zone_state.lock() {
                m.insert(state_key, next);
            }
            // First reading: only an entry is reported (syncs the sim to the lever).
            let changed = match prev {
                Some(p) => p != next,
                None => next,
            };
            if changed {
                sent.push(format!("zone {:.0}-{:.0}% {}", zone.min, zone.max, if next { "enter" } else { "leave" }));
                transitions.push((key, next));
            }
        }
        // Leaving runs before the axis moves again (disarm, then extend); entering runs
        // after the axis has been parked at the band edge (retract, then arm).
        self.run_zone_transitions(&zone_defs, &transitions, false).await?;

        // Proportional output skips the frozen bands around zones that have a deadzone:
        // the rest of the travel is stretched to the full range, and inside a band the
        // edge value is sent once and the axis is then left alone.
        let (position, send_axis) = if bands.is_empty() {
            (position, true)
        } else {
            let (p, inside) = axis_outside_bands(position * 100.0, &bands);
            let band_key = format!("{}|{}|band", device_id, bound_to);
            let was_inside = self.zone_state.lock().ok().and_then(|mut m| m.insert(band_key, inside));
            (p, !(inside && was_inside == Some(true)))
        };

        for def in defs {
            // Axis movements only fire VALUE (or trigger-less) definitions; PRESS/RELEASE
            // definitions on the same input belong to its push-button.
            let trig = def.trigger.as_deref().unwrap_or("");
            if !(trig.is_empty() || trig.eq_ignore_ascii_case("VALUE")) {
                continue;
            }
            let Some(actions) = &def.actions else { continue };
            for act in &actions.items {
                if let EventActionItem::RangedAxis(lever) = act {
                    if !send_axis {
                        continue;
                    }
                    let rev_key = format!("{}|{}|reverse|{}", device_id, bound_to, lever.config_id);
                    let was_reverse = self.zone_state.lock().ok().and_then(|m| m.get(&rev_key).copied()).unwrap_or(false);
                    match super::lever::lever_outputs_with_state(lever, raw, min, max, was_reverse) {
                        Some((outputs, in_reverse)) => {
                            if let Ok(mut m) = self.zone_state.lock() {
                                m.insert(rev_key, in_reverse);
                            }
                            for (event_name, value) in outputs {
                                if !sent.contains(&event_name) {
                                    sent.push(event_name.clone());
                                }
                                trace!("Lever {} ({}) -> {} = {}", bound_to, lever.config_id, event_name, value);
                                if let Err(e) = self.simconnect.transmit_event(&event_name, value as u32).await {
                                    warn!("Failed to send lever event {}: {}", event_name, e);
                                }
                            }
                        }
                        None => debug!("Lever type '{}' is not supported yet", lever.config_id),
                    }
                    continue;
                }
                let (target, invert) = match act {
                    EventActionItem::Control(ctrl) => (ctrl.target.as_str(), false),
                    EventActionItem::CustomAxis(caxis) => (caxis.target.as_str(), option_is_true(caxis.options.as_ref(), "INVERTAXIS")),
                    _ => continue,
                };
                if !send_axis {
                    continue;
                }
                let Some(event_name) = sim_event_name(target) else {
                    debug!("Axis target '{}' is not a SimConnect event; skipped", target);
                    continue;
                };
                let p = if invert { 1.0 - position } else { position };
                let value = scale_axis(p, sim_axis_range(event_name));
                trace!("Axis {} -> {} = {}", bound_to, event_name, value);
                if let Err(e) = self.simconnect.transmit_event(event_name, value as u32).await {
                    warn!("Failed to send axis event {}: {}", event_name, e);
                }
                if !sent.iter().any(|e| e == event_name) {
                    sent.push(event_name.to_string());
                }
            }
        }

        self.run_zone_transitions(&zone_defs, &transitions, true).await?;
        Ok(sent)
    }

    /// Runs the `ZONE_ENTER` (`entered`) or `ZONE_LEAVE` definitions of the zones that changed.
    async fn run_zone_transitions(
        &self,
        zone_defs: &[(AxisZone, &EventDefinition)],
        transitions: &[(String, bool)],
        entered: bool,
    ) -> Result<()> {
        let trigger = if entered { "ZONE_ENTER" } else { "ZONE_LEAVE" };
        for (key, _) in transitions.iter().filter(|(_, e)| *e == entered) {
            for (z, def) in zone_defs {
                if &z.key() == key && def.trigger.as_deref().is_some_and(|t| t.eq_ignore_ascii_case(trigger)) {
                    self.run_definition(def, trigger).await?;
                }
            }
        }
        Ok(())
    }

    /// Triggers an input event from a hardware device (e.g. bound_to: "AP_BUTTON", trigger: "PRESSED")
    pub async fn dispatch_event(&self, device_id: &str, bound_to: &str, trigger: &str) -> Result<bool> {
        debug!("Dispatching event on device '{}': bound_to='{}', trigger='{}'", device_id, bound_to, trigger);

        let defs = self.matching_definitions(device_id, bound_to).await;
        if defs.is_empty() {
            return Ok(false);
        }

        // Cached values; reading each LVAR through the WASM bridge here would cost a
        // round-trip per variable on every button press.
        let lvars = self.bridge.get_lvar_values().await;
        let simvars = self.simconnect.simvar_snapshot();
        let locals = self.local_vars.lock().await.clone();
        let pmdg = self.simconnect.pmdg_snapshot();
        let evaluator = ConditionEvaluator::new(&lvars, &locals).with_simvars(&simvars).with_pmdg(&pmdg);

        // If any PMDG action on this input passes an explicit position value (0, 1, 2...),
        // the input drives a switch position: an empty parameter then means position 0,
        // not a mouse click (SPAD.neXt start levers: PRESS = 0, RELEASE = 1).
        let pmdg_positional = defs.iter().flat_map(|d| d.actions.iter().flat_map(|a| &a.items)).any(|a| match a {
            EventActionItem::Control(c) => {
                pmdg_target(&c.target).is_some()
                    && c.effective_value().and_then(|v| v.trim().parse::<u32>().ok()).is_some_and(|v| v < 0x1000)
            }
            _ => false,
        });

        for def in defs {
            // Leftover definitions without actions (e.g. after deleting actions in the editor)
            // must not end processing through IsFinal.
            if def.actions.as_ref().is_none_or(|a| a.items.is_empty()) {
                continue;
            }
            let def_trigger = def.trigger.as_deref().unwrap_or("");
            if !trigger_matches(def_trigger, trigger) {
                continue;
            }

            if evaluator.evaluate_definition(def) {
                info!("Event triggered: BoundTo='{}', Trigger='{}'", bound_to, trigger);
                self.execute_actions(def, trigger, pmdg_positional).await?;
                if def.is_final.unwrap_or(false) {
                    break;
                }
            }
        }

        Ok(true)
    }

    async fn execute_actions(&self, def: &EventDefinition, trigger: &str, pmdg_positional: bool) -> Result<()> {
        let actions = match &def.actions {
            Some(a) => &a.items,
            None => return Ok(()),
        };

        for act in actions {
            match act {
                EventActionItem::Control(ctrl) => {
                    let param = ctrl.effective_value();
                    info!("Action CONTROL -> '{}' (value: {:?})", ctrl.target, param);
                    if let Some((prefix, name)) = pmdg_target(&ctrl.target) {
                        let val = match param.map(str::trim).filter(|p| !p.is_empty()) {
                            Some(p) => parse_event_value(Some(p)),
                            None if pmdg_positional => 0,
                            None => crate::pmdg::default_parameter(trigger),
                        };
                        if let Err(e) = self.simconnect.transmit_pmdg_event(prefix, name, val).await {
                            warn!("Failed to send PMDG event {}: {}", ctrl.target, e);
                        }
                        continue;
                    }
                    match sim_event_name(&ctrl.target) {
                        Some(event_name) => {
                            let val = parse_event_value(param);
                            if let Err(e) = self.simconnect.transmit_event(event_name, val).await {
                                warn!("Failed to send SimConnect event {}: {}", event_name, e);
                            }
                        }
                        None => debug!("Control target '{}' is not supported yet; skipped", ctrl.target),
                    }
                }
                EventActionItem::ChangeValue(chg) => {
                    info!("Action CHANGE_VALUE -> target='{}', op='{:?}', val='{:?}'", chg.target, chg.operation, chg.value);
                    if let Err(e) = self.change_value(chg, trigger).await {
                        warn!("CHANGE_VALUE on '{}' failed: {}", chg.target, e);
                    }
                }
                EventActionItem::Script(script) => {
                    if let Some(code) = &script.script {
                        info!("Action SCRIPT -> executing RPN calculator code: {}", code);
                        let _ = self.bridge.execute_calculator_code(code, CALC_RET_VOID).await;
                    }
                }
                EventActionItem::Delay(del) => {
                    debug!("Action DELAY -> {} ms", del.delay_ms);
                    sleep(Duration::from_millis(del.delay_ms)).await;
                }
                EventActionItem::Command(cmd) => {
                    info!("Action COMMAND -> {}", cmd.command);
                }
                _ => {}
            }
        }
        Ok(())
    }
}

impl ProfileExecutor {
    async fn change_value(&self, chg: &EventActionChangeValue, trigger: &str) -> Result<()> {
        let operand = chg.value.as_deref().and_then(|v| v.trim().parse::<f64>().ok()).unwrap_or(0.0);
        let op = chg.operation.as_deref().unwrap_or("");
        let Some((prefix, name)) = super::evaluator::split_data_prefix(chg.target.trim()) else {
            anyhow::bail!("target has no data provider prefix");
        };
        match prefix.to_ascii_uppercase().as_str() {
            "LVAR" => {
                let name = name.trim_start_matches("L:").trim();
                let current = self.bridge.get_lvar_values().await.get(name).copied().unwrap_or(0.0);
                self.bridge.set_lvar(name, apply_operation(current, op, operand, trigger)).await
            }
            "LOCAL" => {
                let mut locals = self.local_vars.lock().await;
                let current = locals.get(name).and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0);
                locals.insert(name.to_string(), apply_operation(current, op, operand, trigger).to_string());
                Ok(())
            }
            "SIMCONNECT" | "MSFS" => {
                let key = self.simconnect.subscribe_simvar(name).await?;
                let current = self.simconnect.simvar(&key).unwrap_or(0.0);
                self.simconnect.set_simvar(&key, apply_operation(current, op, operand, trigger)).await
            }
            other => anyhow::bail!("data provider '{}' is not supported yet", other),
        }
    }
}

/// Applies a SPAD.neXt `ChangeValue` operation.
///
/// With no explicit operation, a rotary trigger steps the value (clockwise adds the operand,
/// counter-clockwise subtracts it), which is how SPAD.neXt tuner bindings are written.
/// Anything else sets the value.
pub fn apply_operation(current: f64, op: &str, operand: f64, trigger: &str) -> f64 {
    let bit = || 1u64 << (operand as u32).min(63);
    let cur_bits = current.max(0.0) as u64;
    match op.trim().to_ascii_uppercase().as_str() {
        "" | "SET" => {
            let t = trigger.to_ascii_uppercase().replace(['_', ' '], "");
            if t.contains("COUNTERCLOCKWISE") || t.ends_with("CCW") {
                current - operand
            } else if t.contains("CLOCKWISE") || t.ends_with("CW") {
                current + operand
            } else {
                operand
            }
        }
        "INCREMENT" | "INC" | "ADD" => current + operand,
        "DECREMENT" | "DEC" | "SUB" | "SUBTRACT" => current - operand,
        "MULTIPLY" => current * operand,
        "DIVIDE" if operand != 0.0 => current / operand,
        "TOGGLE" => {
            if current != 0.0 { 0.0 } else if operand != 0.0 { operand } else { 1.0 }
        }
        "TOGGLEBIT" => (cur_bits ^ bit()) as f64,
        "SETBIT" => (cur_bits | bit()) as f64,
        "CLEARBIT" => (cur_bits & !bit()) as f64,
        _ => operand,
    }
}

/// `evdev_044f_0407_event7` -> `device 044F:0407`, for log messages.
fn device_label(device_id: &str) -> String {
    let parts: Vec<&str> = device_id.split('_').collect();
    match parts.as_slice() {
        ["evdev", vid, pid, ..] => format!("device {}:{}", vid.to_ascii_uppercase(), pid.to_ascii_uppercase()),
        _ => device_id.to_string(),
    }
}

/// Whether a definition's trigger fires for an incoming trigger. A definition without a
/// trigger fires on press; `PRESSED` / `RELEASED` are accepted spellings.
pub fn trigger_matches(definition: &str, incoming: &str) -> bool {
    let canon = |t: &str| match t.trim().to_ascii_uppercase().as_str() {
        "" | "PRESSED" => "PRESS".to_string(),
        "RELEASED" => "RELEASE".to_string(),
        other => other.to_string(),
    };
    canon(definition) == canon(incoming)
}

/// Splits a `PMDGNG3:EVT_...` / `PMDG777:...` target into (prefix, name).
pub fn pmdg_target(target: &str) -> Option<(&str, &str)> {
    let (prefix, name) = super::evaluator::split_data_prefix(target.trim())?;
    crate::pmdg::PmdgFamily::from_profile_prefix(prefix).map(|_| (prefix, name.trim()))
}

/// True if any action or condition in the profile targets a PMDG data provider.
pub fn uses_pmdg(profile: &SpadProfile) -> bool {
    profile.devices.iter().flat_map(|d| &d.items).flat_map(|d| &d.pages).any(|page| {
        page.events.iter().flat_map(|e| &e.items).flat_map(|e| e.event_definitions.iter().flat_map(|d| &d.items)).any(|def| {
            def.actions.iter().flat_map(|a| &a.items).any(|a| match a {
                EventActionItem::Control(c) => pmdg_target(&c.target).is_some(),
                EventActionItem::DisplayValue(d) => d.target.as_deref().and_then(pmdg_target).is_some(),
                _ => false,
            }) || def.conditions.iter().flat_map(|c| &c.items).any(|c| match c {
                EventConditionItem::Simple(s) => pmdg_target(&s.target).is_some(),
                _ => false,
            })
        })
    })
}

/// Every SimVar referenced by conditions, value changes and displays in the profile.
pub fn collect_simvar_refs(profile: &SpadProfile) -> Vec<String> {
    use crate::simconnect::simvars::is_simvar_name;
    let mut out = std::collections::BTreeSet::new();
    let mut consider = |target: &str| {
        if let Some((prefix, name)) = super::evaluator::split_data_prefix(target.trim()) {
            if (prefix.eq_ignore_ascii_case("SIMCONNECT") || prefix.eq_ignore_ascii_case("MSFS")) && is_simvar_name(name) {
                if let Some(r) = crate::simconnect::simvars::SimVarRef::parse(name) {
                    out.insert(r.key);
                }
            }
        }
    };
    for dev in profile.devices.iter().flat_map(|d| &d.items) {
        for page in &dev.pages {
            for ev in page.events.iter().flat_map(|e| &e.items) {
                for def in ev.event_definitions.iter().flat_map(|d| &d.items) {
                    for cond in def.conditions.iter().flat_map(|c| &c.items) {
                        if let EventConditionItem::Simple(s) = cond {
                            consider(&s.target);
                        }
                    }
                    for act in def.actions.iter().flat_map(|a| &a.items) {
                        match act {
                            EventActionItem::ChangeValue(c) => consider(&c.target),
                            EventActionItem::DisplayValue(d) => {
                                if let Some(t) = &d.target {
                                    consider(t);
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    }
    out.into_iter().collect()
}


/// Extracts the sim event name from a profile target such as `SIMCONNECT:AP_MASTER`.
/// Returns `None` for targets handled elsewhere (LVAR:, LOCAL:, ...).
pub fn sim_event_name(target: &str) -> Option<&str> {
    let target = target.trim();
    if let Some((prefix, rest)) = target.split_once(':') {
        if prefix.eq_ignore_ascii_case("SIMCONNECT") || prefix.eq_ignore_ascii_case("MSFS") || prefix.eq_ignore_ascii_case("K") {
            let rest = rest.trim();
            // `SIMCONNECT:AUTOPILOT MASTER` is a SimVar, not an event.
            return (!rest.is_empty() && !crate::simconnect::simvars::is_simvar_name(rest)).then_some(rest);
        }
        return None;
    }
    (!target.is_empty()).then_some(target)
}

/// Parses an event parameter; accepts signed and fractional values (sent as two's complement).
fn parse_event_value(v: Option<&str>) -> u32 {
    let Some(v) = v.map(str::trim).filter(|v| !v.is_empty()) else { return 0 };
    if let Ok(i) = v.parse::<i64>() {
        return i as u32;
    }
    v.parse::<f64>().map(|f| f.round() as i64 as u32).unwrap_or(0)
}

pub fn option_is_true(opts: Option<&ProfileOptionsList>, key: &str) -> bool {
    opts.map(|o| {
        o.items.iter().any(|opt| {
            opt.key.eq_ignore_ascii_case(key)
                && opt.value.as_deref().map(|v| v.eq_ignore_ascii_case("true")).unwrap_or(false)
        })
    })
    .unwrap_or(false)
}

/// Hardware position in `0.0..=1.0` (min end = 0, max end = 1).
pub fn axis_position(raw: i32, min: i32, max: i32) -> f64 {
    let (lo, hi) = (min.min(max) as f64, min.max(max) as f64);
    if hi <= lo {
        return 0.0;
    }
    ((raw as f64 - lo) / (hi - lo)).clamp(0.0, 1.0)
}

/// Value range a sim event expects.
///
/// All `AXIS_*` / `*_AXIS_SET_EX1` events span -16383..16383 (for throttles, -16383 is idle);
/// the legacy `*_SET` events for centred controls are bipolar too, the rest are 0..16383.
pub fn sim_axis_range(event_name: &str) -> (i32, i32) {
    let e = event_name.to_ascii_uppercase();
    let bipolar_legacy = matches!(
        e.as_str(),
        "AILERON_SET" | "ELEVATOR_SET" | "RUDDER_SET" | "ELEVATOR_TRIM_SET" | "AILERON_TRIM_SET" | "RUDDER_TRIM_SET"
    );
    if e.contains("AXIS") || bipolar_legacy {
        (-16383, 16383)
    } else {
        (0, 16383)
    }
}

pub fn scale_axis(position: f64, (lo, hi): (i32, i32)) -> i32 {
    (lo as f64 + position.clamp(0.0, 1.0) * (hi - lo) as f64).round() as i32
}
