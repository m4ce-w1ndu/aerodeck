//! SimVar (simulation variable) subscriptions.
//!
//! Each SimVar gets its own single-datum data definition, requested every visual frame
//! with `SIMCONNECT_DATA_REQUEST_FLAG_CHANGED`, so the sim only sends a value when it
//! changes. One definition per variable keeps a misspelled name from breaking others:
//! SimConnect rejects only that definition.
//!
//! Names follow SPAD.neXt / MSFS conventions: `NAV STANDBY FREQUENCY:2`, optionally with an
//! `A:` prefix and an explicit unit after a comma (`A:INDICATED ALTITUDE, meters`).

use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::RwLock;
use tokio::sync::broadcast;
use tracing::debug;

use super::client::SimConnectClient;
use super::protocol::{RecvSimObjectData, SIMCONNECT_OBJECT_ID_USER};

/// `SIMCONNECT_DATATYPE_FLOAT64`
const DATATYPE_FLOAT64: u32 = 4;
/// `SIMCONNECT_PERIOD_VISUAL_FRAME`
const PERIOD_VISUAL_FRAME: u32 = 2;
/// `SIMCONNECT_DATA_REQUEST_FLAG_CHANGED`
const REQUEST_FLAG_CHANGED: u32 = 1;
/// Data definition / request IDs for SimVars start here, away from the bridge's IDs.
const FIRST_SIMVAR_DEFINE_ID: u32 = 0x0002_0000;

/// A parsed SimVar reference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimVarRef {
    /// Canonical key: upper-case name with optional `:index`, no `A:` prefix, no unit.
    pub key: String,
    pub unit: String,
}

impl SimVarRef {
    pub fn parse(raw: &str) -> Option<Self> {
        let raw = raw.trim();
        let (name, unit) = match raw.split_once(',') {
            Some((n, u)) if !u.trim().is_empty() => (n.trim(), Some(u.trim().to_string())),
            _ => (raw, None),
        };
        let name = name.strip_prefix("A:").or_else(|| name.strip_prefix("a:")).unwrap_or(name).trim();
        if name.is_empty() {
            return None;
        }
        let key = name.split_whitespace().collect::<Vec<_>>().join(" ").to_ascii_uppercase();
        let unit = unit.unwrap_or_else(|| default_unit(&key).to_string());
        Some(Self { key, unit })
    }
}

/// True if a `SIMCONNECT:` target names a SimVar rather than a key event.
/// SimVars are written with spaces (`AUTOPILOT MASTER`), events with underscores (`AP_MASTER`).
pub fn is_simvar_name(name: &str) -> bool {
    let n = name.trim();
    let n = n.strip_prefix("A:").unwrap_or(n);
    n.contains(' ') || n.contains(',')
}

/// Unit SPAD.neXt-style profiles expect when none is given. Values are what pilots see on
/// panels (MHz, feet, degrees, knots), not the sim's internal units (Hz, meters, radians).
pub fn default_unit(key: &str) -> &'static str {
    let base = key.split(':').next().unwrap_or(key);
    let has = |s: &str| base.contains(s);

    if has("ADF") && (has("FREQUENCY") || has("STANDBY") || has("ACTIVE")) {
        "KHz"
    } else if has("FREQUENCY") {
        "MHz"
    } else if has("TRANSPONDER CODE") {
        "number"
    } else if has("VERTICAL SPEED") || has("VERTICAL HOLD VAR") {
        "feet per minute"
    } else if has("ALTITUDE") || has("ALT LOCK VAR") || has("DECISION HEIGHT") {
        "feet"
    } else if has("MACH") {
        "mach"
    } else if has("AIRSPEED") || has("GROUND VELOCITY") {
        "knots"
    } else if has("HEADING") || has("BEARING") || has("COURSE") || has("OBS") || has("RADIAL")
        || has("PLANE PITCH") || has("PLANE BANK") || has("LATITUDE") || has("LONGITUDE")
    {
        "degrees"
    } else if has("KOHLSMAN") {
        "inHg"
    } else if has("PERCENT") || (has("POSITION") && (has("THROTTLE") || has("FLAPS") || has("SPOILER"))) {
        "percent"
    } else {
        "number"
    }
}

/// Many SimVars SPAD.neXt profiles write to are read-only through `SetDataOnSimObject`
/// (radio frequencies, autopilot references, ...). For those, the matching key event is
/// sent instead. Returns `(event name, event data)` for `value` in [`default_unit`] units.
pub fn setter_event(key: &str, value: f64) -> Option<(String, u32)> {
    let (base, index) = match key.rsplit_once(':') {
        Some((b, i)) if i.chars().all(|c| c.is_ascii_digit()) => (b, i.parse::<u32>().unwrap_or(1)),
        _ => (key, 1),
    };
    let numbered = |first: &str, other: String| if index <= 1 { first.to_string() } else { other };
    let hz = |mhz: f64| (mhz * 1_000_000.0).round() as u32;

    let ev = match base {
        "NAV STANDBY FREQUENCY" => (format!("NAV{}_STBY_SET_HZ", index), hz(value)),
        "NAV ACTIVE FREQUENCY" => (format!("NAV{}_RADIO_SET_HZ", index), hz(value)),
        "COM STANDBY FREQUENCY" => (numbered("COM_STBY_RADIO_SET_HZ", format!("COM{}_STBY_RADIO_SET_HZ", index)), hz(value)),
        "COM ACTIVE FREQUENCY" => (numbered("COM_RADIO_SET_HZ", format!("COM{}_RADIO_SET_HZ", index)), hz(value)),
        "ADF STANDBY FREQUENCY" => (numbered("ADF_STBY_SET", format!("ADF{}_STBY_SET", index)), (value * 1000.0).round() as u32),
        "AUTOPILOT ALTITUDE LOCK VAR" => ("AP_ALT_VAR_SET_ENGLISH".to_string(), value.round() as i32 as u32),
        "AUTOPILOT HEADING LOCK DIR" => ("HEADING_BUG_SET".to_string(), value.round() as u32),
        "AUTOPILOT VERTICAL HOLD VAR" => ("AP_VS_VAR_SET_ENGLISH".to_string(), value.round() as i32 as u32),
        "AUTOPILOT AIRSPEED HOLD VAR" => ("AP_SPD_VAR_SET".to_string(), value.round() as u32),
        "NAV OBS" => (format!("VOR{}_SET", index), value.round() as u32),
        "KOHLSMAN SETTING MB" => ("KOHLSMAN_SET".to_string(), (value * 16.0).round() as u32),
        "TRANSPONDER CODE" => ("XPNDR_SET".to_string(), to_bcd16(value.round() as u32)),
        _ => return None,
    };
    Some(ev)
}

/// Keeps values inside the range the instrument can show, wrapping like a real knob.
pub fn wrap_value(key: &str, value: f64) -> f64 {
    let base = key.split(':').next().unwrap_or(key);
    let wrap = |v: f64, lo: f64, hi: f64, step: f64| {
        if v > hi + step / 2.0 {
            lo + (v - hi - step)
        } else if v < lo - step / 2.0 {
            hi - (lo - step - v)
        } else {
            v
        }
    };
    match base {
        "AUTOPILOT HEADING LOCK DIR" | "NAV OBS" => value.rem_euclid(360.0),
        "NAV STANDBY FREQUENCY" | "NAV ACTIVE FREQUENCY" => wrap(value, 108.0, 117.95, 0.05),
        "COM STANDBY FREQUENCY" | "COM ACTIVE FREQUENCY" => wrap(value, 118.0, 136.99, 0.005),
        "ADF STANDBY FREQUENCY" => wrap(value, 190.0, 1799.0, 1.0),
        "AUTOPILOT ALTITUDE LOCK VAR" => value.max(0.0),
        _ => value,
    }
}

fn to_bcd16(code: u32) -> u32 {
    let d = |n: u32| (code / n) % 10;
    (d(1000) << 12) | (d(100) << 8) | (d(10) << 4) | d(1)
}

/// Live values shared between the SimConnect reader task and everyone else.
pub struct SimVarStore {
    inner: RwLock<StoreInner>,
    changes: broadcast::Sender<(String, f64)>,
}

#[derive(Default)]
struct StoreInner {
    /// SimVar key -> define ID (also used as request ID)
    defines: HashMap<String, u32>,
    by_define: HashMap<u32, String>,
    values: HashMap<String, f64>,
}

impl Default for SimVarStore {
    fn default() -> Self {
        Self {
            inner: RwLock::new(StoreInner::default()),
            changes: broadcast::channel(1024).0,
        }
    }
}

static NEXT_DEFINE_ID: AtomicU32 = AtomicU32::new(FIRST_SIMVAR_DEFINE_ID);

impl SimVarStore {
    /// Called by the reader task for every `SIMOBJECT_DATA` packet.
    pub fn apply(&self, d: &RecvSimObjectData) {
        if d.data.len() < 8 {
            return;
        }
        let value = f64::from_le_bytes(d.data[0..8].try_into().unwrap());
        let key = {
            let mut inner = match self.inner.write() {
                Ok(g) => g,
                Err(_) => return,
            };
            let Some(key) = inner.by_define.get(&d.define_id).cloned() else { return };
            inner.values.insert(key.clone(), value);
            key
        };
        let _ = self.changes.send((key, value));
    }

    pub fn get(&self, key: &str) -> Option<f64> {
        self.inner.read().ok()?.values.get(key).copied()
    }

    pub fn snapshot(&self) -> HashMap<String, f64> {
        self.inner.read().map(|i| i.values.clone()).unwrap_or_default()
    }

    pub fn subscribed(&self) -> Vec<String> {
        let mut v: Vec<String> = self.inner.read().map(|i| i.defines.keys().cloned().collect()).unwrap_or_default();
        v.sort();
        v
    }

    fn define_for(&self, key: &str) -> Option<u32> {
        self.inner.read().ok()?.defines.get(key).copied()
    }

    fn register(&self, key: &str) -> u32 {
        let id = NEXT_DEFINE_ID.fetch_add(1, Ordering::SeqCst);
        if let Ok(mut inner) = self.inner.write() {
            inner.defines.insert(key.to_string(), id);
            inner.by_define.insert(id, key.to_string());
        }
        id
    }

    /// Records a value written locally so readers see it before the sim echoes it back.
    fn set_local(&self, key: &str, value: f64) {
        if let Ok(mut inner) = self.inner.write() {
            inner.values.insert(key.to_string(), value);
        }
    }
}

impl SimConnectClient {
    /// Subscribes to a SimVar (idempotent) and returns its canonical key.
    pub async fn subscribe_simvar(&self, name: &str) -> Result<String> {
        let r = SimVarRef::parse(name).ok_or_else(|| anyhow!("invalid SimVar name '{}'", name))?;
        let _guard = self.simvar_subscribe_lock.lock().await;
        if self.simvar_store.define_for(&r.key).is_some() {
            return Ok(r.key);
        }
        let id = self.simvar_store.register(&r.key);
        debug!("Subscribing SimVar '{}' [{}] as define {}", r.key, r.unit, id);
        self.add_to_data_definition(id, &r.key, &r.unit, DATATYPE_FLOAT64, 0.0).await?;
        self.request_data_on_sim_object(id, id, SIMCONNECT_OBJECT_ID_USER, PERIOD_VISUAL_FRAME, REQUEST_FLAG_CHANGED)
            .await?;
        Ok(r.key)
    }

    /// Last known value of a SimVar (None until the sim has sent one).
    pub fn simvar(&self, name: &str) -> Option<f64> {
        SimVarRef::parse(name).and_then(|r| self.simvar_store.get(&r.key))
    }

    pub fn simvar_snapshot(&self) -> HashMap<String, f64> {
        self.simvar_store.snapshot()
    }

    pub fn subscribed_simvars(&self) -> Vec<String> {
        self.simvar_store.subscribed()
    }

    /// Stream of `(key, value)` for every SimVar change.
    pub fn simvar_changes(&self) -> broadcast::Receiver<(String, f64)> {
        self.simvar_store.changes.subscribe()
    }

    /// Writes a SimVar, through its key event when it has one (see [`setter_event`]),
    /// otherwise with `SetDataOnSimObject` (subscribing to it first if needed).
    pub async fn set_simvar(&self, name: &str, value: f64) -> Result<()> {
        let key = self.subscribe_simvar(name).await?;
        let value = wrap_value(&key, value);
        if let Some((event, data)) = setter_event(&key, value) {
            self.simvar_store.set_local(&key, value);
            return self.transmit_event(&event, data).await;
        }
        let id = self
            .simvar_store
            .define_for(&key)
            .ok_or_else(|| anyhow!("SimVar '{}' has no definition", key))?;
        self.simvar_store.set_local(&key, value);
        self.set_data_on_sim_object(id, SIMCONNECT_OBJECT_ID_USER, &value.to_le_bytes()).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_names_units_and_indices() {
        let r = SimVarRef::parse("A:nav standby  frequency:2").unwrap();
        assert_eq!(r.key, "NAV STANDBY FREQUENCY:2");
        assert_eq!(r.unit, "MHz");

        let r = SimVarRef::parse("INDICATED ALTITUDE, meters").unwrap();
        assert_eq!(r.key, "INDICATED ALTITUDE");
        assert_eq!(r.unit, "meters");

        assert_eq!(SimVarRef::parse("ADF ACTIVE FREQUENCY:1").unwrap().unit, "KHz");
        assert_eq!(SimVarRef::parse("AUTOPILOT HEADING LOCK DIR").unwrap().unit, "degrees");
        assert_eq!(SimVarRef::parse("AUTOPILOT MASTER").unwrap().unit, "number");
    }

    #[test]
    fn writes_go_through_key_events() {
        assert_eq!(setter_event("NAV STANDBY FREQUENCY:2", 110.55), Some(("NAV2_STBY_SET_HZ".into(), 110_550_000)));
        assert_eq!(setter_event("COM STANDBY FREQUENCY:1", 121.5).unwrap().0, "COM_STBY_RADIO_SET_HZ");
        assert_eq!(setter_event("TRANSPONDER CODE:1", 7000.0), Some(("XPNDR_SET".into(), 0x7000)));
        assert_eq!(setter_event("AUTOPILOT MASTER", 1.0), None);

        assert_eq!(wrap_value("AUTOPILOT HEADING LOCK DIR", 361.0), 1.0);
        assert!((wrap_value("NAV STANDBY FREQUENCY:1", 118.0) - 108.0).abs() < 1e-9);
        assert!((wrap_value("NAV STANDBY FREQUENCY:1", 107.95) - 117.95).abs() < 1e-9);
    }

    #[test]
    fn distinguishes_simvars_from_events() {
        assert!(is_simvar_name("AUTOPILOT MASTER"));
        assert!(is_simvar_name("NAV STANDBY FREQUENCY:2"));
        assert!(!is_simvar_name("AP_MASTER"));
        assert!(!is_simvar_name("THROTTLE1_AXIS_SET_EX1"));
    }
}
