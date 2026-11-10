//! SPAD.neXt calibrated levers (`EventActionRangedAxis`), e.g. the Thrustmaster TCA quadrant
//! throttles with an idle detent.
//!
//! A lever is described by zones of raw device values:
//!
//! ```text
//!  MAX ── MAXTO        IDLETO ── IDLE          (reverse side, ignored by *_NOREVERSE_*)
//!  └ full zone ┘ ←── linear ──→ └ idle zone ┘
//! ```
//!
//! Either edge order works (on the TCA, full thrust is raw 0 and idle ≈ 49 000). Levers
//! without an idle detent (mixture, flaps) use `MIN`/`MINTO` as the low zone. Positions inside
//! a zone snap to its end; between zones they are linear.

use super::schema::EventActionRangedAxis;

/// Lever kinds this implementation understands, derived from the SPAD `ConfigID`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeverKind {
    Throttle,
    Mixture,
    Propeller,
    Flaps,
    Spoiler,
}

impl LeverKind {
    pub fn from_config_id(config_id: &str) -> Option<Self> {
        let c = config_id.to_ascii_uppercase();
        Some(if c.contains("THROTTLE") {
            LeverKind::Throttle
        } else if c.contains("MIXTURE") {
            LeverKind::Mixture
        } else if c.contains("PROP") {
            LeverKind::Propeller
        } else if c.contains("FLAP") {
            LeverKind::Flaps
        } else if c.contains("SPOILER") || c.contains("SPEEDBRAKE") {
            LeverKind::Spoiler
        } else {
            return None;
        })
    }

    /// Sim events for engine `n` (1-based), or for all engines when `None`.
    /// All take -16383..16383 with -16383 at the low end (idle, lean, coarse, up, retracted).
    fn event(self, engine: Option<u8>) -> Option<String> {
        Some(match (self, engine) {
            (LeverKind::Throttle, Some(n)) => format!("THROTTLE{}_AXIS_SET_EX1", n),
            (LeverKind::Throttle, None) => "AXIS_THROTTLE_SET".to_string(),
            (LeverKind::Mixture, Some(n)) => format!("AXIS_MIXTURE{}_SET", n),
            (LeverKind::Mixture, None) => "AXIS_MIXTURE_SET".to_string(),
            (LeverKind::Propeller, Some(n)) => format!("AXIS_PROPELLER{}_SET", n),
            (LeverKind::Propeller, None) => "AXIS_PROPELLER_SET".to_string(),
            (LeverKind::Flaps, _) => "AXIS_FLAPS_SET".to_string(),
            (LeverKind::Spoiler, _) => "AXIS_SPOILER_SET".to_string(),
        })
    }

    fn per_engine(self) -> bool {
        matches!(self, LeverKind::Throttle | LeverKind::Mixture | LeverKind::Propeller)
    }
}

/// A zone of raw values; `near` is the edge facing the linear range.
#[derive(Debug, Clone, Copy)]
struct Zone {
    lo: f64,
    hi: f64,
}

impl Zone {
    fn new(a: f64, b: Option<f64>) -> Self {
        let b = b.unwrap_or(a);
        Zone { lo: a.min(b), hi: a.max(b) }
    }
}

fn zone(action: &EventActionRangedAxis, key: &str) -> Option<Zone> {
    let a = action.option_f64(key)?;
    Some(Zone::new(a, action.option_f64(&format!("{}TO", key))))
}

/// Lever position in `0.0..=1.0` (0 = idle / low end, 1 = full / high end).
pub fn lever_position(action: &EventActionRangedAxis, raw: i32, dev_min: i32, dev_max: i32) -> f64 {
    let raw = raw as f64;
    let low = zone(action, "IDLE").or_else(|| zone(action, "MIN")).unwrap_or(Zone::new(dev_min as f64, None));
    let high = zone(action, "MAX").unwrap_or(Zone::new(dev_max as f64, None));

    // Which way does the lever run? (TCA: high end has the smaller raw value.)
    let high_is_below = high.hi <= low.lo;
    let (low_edge, high_edge) = if high_is_below { (low.lo, high.hi) } else { (low.hi, high.lo) };
    let span = high_edge - low_edge;
    if span.abs() < f64::EPSILON {
        return 0.0;
    }
    // Past the low zone (reverse side) clamps to 0; inside the high zone clamps to 1.
    ((raw - low_edge) / span).clamp(0.0, 1.0)
}

/// Sim events to send for a lever movement, with values in -16383..16383.
pub fn lever_outputs(action: &EventActionRangedAxis, raw: i32, dev_min: i32, dev_max: i32) -> Option<Vec<(String, i32)>> {
    let kind = LeverKind::from_config_id(&action.config_id)?;
    let pos = lever_position(action, raw, dev_min, dev_max);
    let value = (-16383.0 + pos * 32766.0).round() as i32;

    let engines: Vec<u8> = if kind.per_engine() {
        (1..=4u8).filter(|n| action.option_bool(&format!("OPTION{}", n))).collect()
    } else {
        Vec::new()
    };
    let out = if engines.is_empty() {
        vec![(kind.event(None)?, value)]
    } else {
        engines.into_iter().filter_map(|n| kind.event(Some(n)).map(|e| (e, value))).collect()
    };
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::schema::{ProfileOption, ProfileOptionsList};

    fn lever(config: &str, opts: &[(&str, &str)]) -> EventActionRangedAxis {
        EventActionRangedAxis {
            config_id: config.to_string(),
            options: Some(ProfileOptionsList {
                items: opts
                    .iter()
                    .map(|(k, v)| ProfileOption { key: k.to_string(), value: Some(v.to_string()) })
                    .collect(),
            }),
        }
    }

    #[test]
    fn tca_throttle_from_pmdg_777_profile() {
        // Values from profiles/PMDG 777.xml, X_AXIS of the TCA quadrant (044F:0407)
        let l = lever(
            "LEVER_NOREVERSE_THROTTLE",
            &[("USERAWVALUE", "True"), ("MAX", "0"), ("MAXTO", "0"), ("OPTION1", "True"), ("IDLE", "50934"), ("IDLETO", "48647")],
        );
        let out = |raw| lever_outputs(&l, raw, 0, 65535).unwrap();
        assert_eq!(out(0), vec![("THROTTLE1_AXIS_SET_EX1".to_string(), 16383)]);
        assert_eq!(out(48647)[0].1, -16383, "idle detent");
        assert_eq!(out(50000)[0].1, -16383, "inside idle zone");
        assert_eq!(out(65535)[0].1, -16383, "reverse side ignored");
        let mid = out(48647 / 2)[0].1;
        assert!(mid.abs() <= 1, "halfway between idle and max is centre, got {}", mid);
    }

    #[test]
    fn engine_selection_and_other_levers() {
        let md11_y = lever("LEVER_NOREVERSE_THROTTLE", &[("OPTION3", "True"), ("IDLE", "49171"), ("MAX", "0")]);
        assert_eq!(lever_outputs(&md11_y, 0, 0, 65535).unwrap()[0].0, "THROTTLE3_AXIS_SET_EX1");

        let both = lever("LEVER_NOREVERSE_THROTTLE", &[("OPTION1", "True"), ("OPTION2", "True"), ("OPTION3", "False")]);
        let names: Vec<_> = lever_outputs(&both, 0, 0, 65535).unwrap().into_iter().map(|e| e.0).collect();
        assert_eq!(names, ["THROTTLE1_AXIS_SET_EX1", "THROTTLE2_AXIS_SET_EX1"]);

        // Mixture (Cirrus SR22T): MIN 49211 = cutoff, MAX 0 = full rich, no engine options -> all
        let mix = lever("LEVER_MIXTURE", &[("MIN", "49211"), ("MINTO", "49211"), ("MAX", "0"), ("MAXTO", "0")]);
        assert_eq!(lever_outputs(&mix, 0, 0, 65535).unwrap(), vec![("AXIS_MIXTURE_SET".to_string(), 16383)]);
        assert_eq!(lever_outputs(&mix, 60000, 0, 65535).unwrap()[0].1, -16383);

        // Flaps (Honda Jet): MIN 0 .. MAX 31806, normal direction
        let flaps = lever("LEVER_FLAPS", &[("MIN", "0"), ("MAX", "31806")]);
        assert_eq!(lever_outputs(&flaps, 0, 0, 65535).unwrap(), vec![("AXIS_FLAPS_SET".to_string(), -16383)]);
        assert_eq!(lever_outputs(&flaps, 40000, 0, 65535).unwrap()[0].1, 16383);

        assert!(lever_outputs(&lever("LEVER_SOMETHING_ELSE", &[]), 0, 0, 1).is_none());
    }
}
