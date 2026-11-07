use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::{sleep, Duration};
use tracing::{debug, info};

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
}

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

        Self {
            profile,
            simconnect,
            bridge,
            local_vars: Arc::new(Mutex::new(initial_vars)),
            active_page_idx: Arc::new(Mutex::new(0)),
        }
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

    /// Triggers an input event from a hardware device (e.g. bound_to: "AP_BUTTON", trigger: "PRESSED")
    pub async fn dispatch_event(&self, device_id: &str, bound_to: &str, trigger: &str) -> Result<bool> {
        debug!("Dispatching event on device '{}': bound_to='{}', trigger='{}'", device_id, bound_to, trigger);

        let devices = match &self.profile.devices {
            Some(d) => &d.items,
            None => return Ok(false),
        };

        // Find device profile
        let device = devices.iter().find(|d| {
            d.id.as_deref() == Some(device_id)
                || d.product_id.as_deref() == Some(device_id)
                || d.name.as_deref() == Some(device_id)
        });

        let device = match device {
            Some(d) => d,
            None => {
                debug!("Device '{}' not configured in profile", device_id);
                return Ok(false);
            }
        };

        if device.pages.is_empty() {
            return Ok(false);
        }

        let page_idx = *self.active_page_idx.lock().await;
        let page = &device.pages[page_idx.min(device.pages.len() - 1)];

        let events = match &page.events {
            Some(e) => &e.items,
            None => return Ok(false),
        };

        let spad_event = events.iter().find(|e| e.bound_to.eq_ignore_ascii_case(bound_to));
        let spad_event = match spad_event {
            Some(e) => e,
            None => {
                debug!("No event binding for '{}' on page '{}'", bound_to, page.name.as_deref().unwrap_or(""));
                return Ok(false);
            }
        };

        let defs = match &spad_event.event_definitions {
            Some(d) => &d.items,
            None => return Ok(false),
        };

        let lvars = {
            // Snapshot of LVARs
            let known = self.bridge.get_known_lvars().await;
            let mut map = HashMap::new();
            for k in known {
                if let Ok(val) = self.bridge.get_lvar(&k).await {
                    map.insert(k, val);
                }
            }
            map
        };

        let locals = self.local_vars.lock().await.clone();
        let evaluator = ConditionEvaluator::new(&lvars, &locals);

        for def in defs {
            let def_trigger = def.trigger.as_deref().unwrap_or("");
            if !def_trigger.is_empty() && !def_trigger.eq_ignore_ascii_case(trigger) {
                continue;
            }

            if evaluator.evaluate_definition(def) {
                info!("Event triggered: BoundTo='{}', Trigger='{}'", bound_to, trigger);
                self.execute_actions(def).await?;
                if def.is_final.unwrap_or(false) {
                    break;
                }
            }
        }

        Ok(true)
    }

    async fn execute_actions(&self, def: &EventDefinition) -> Result<()> {
        let actions = match &def.actions {
            Some(a) => &a.items,
            None => return Ok(()),
        };

        for act in actions {
            match act {
                EventActionItem::Control(ctrl) => {
                    info!("Action CONTROL -> '{}' (value: {:?})", ctrl.target, ctrl.value);
                    let _event_name = ctrl.target.strip_prefix("SIMCONNECT:").unwrap_or(&ctrl.target);
                    let val = ctrl.value.as_deref().and_then(|v| v.parse::<u32>().ok()).unwrap_or(0);
                    // Transmit client event
                    let _ = self.simconnect.transmit_client_event(0, 1, val).await;
                }
                EventActionItem::ChangeValue(chg) => {
                    info!("Action CHANGE_VALUE -> target='{}', op='{:?}', val='{:?}'", chg.target, chg.operation, chg.value);
                    let val = chg.value.as_deref().and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0);
                    if chg.target.starts_prefix_ci("LVAR:") {
                        let lvar = chg.target.split(':').nth(1).unwrap_or(&chg.target);
                        self.bridge.set_lvar(lvar, val).await?;
                    } else if chg.target.starts_prefix_ci("LOCAL:") {
                        let local_key = chg.target.split(':').nth(1).unwrap_or(&chg.target);
                        let mut l = self.local_vars.lock().await;
                        l.insert(local_key.to_string(), val.to_string());
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

trait CaseInsensitivePrefix {
    fn starts_prefix_ci(&self, prefix: &str) -> bool;
}

impl CaseInsensitivePrefix for str {
    fn starts_prefix_ci(&self, prefix: &str) -> bool {
        self.len() >= prefix.len() && self[..prefix.len()].eq_ignore_ascii_case(prefix)
    }
}
