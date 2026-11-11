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

/// True for throttles that drive reverse thrust past the idle detent
/// (`LEVER_THROTTLE`, `LEVER_REVERSE_THROTTLE`; not `*_NOREVERSE_*`).
pub fn has_reverse(action: &EventActionRangedAxis) -> bool {
    let c = action.config_id.to_ascii_uppercase();
    c.contains("THROTTLE") && !c.contains("NOREVERSE")
}

/// Default reverse gate, in percent of the travel between the idle detent and the `REVERSE` zone.
pub const DEFAULT_REVERSE_GATE: f64 = 20.0;
/// How far (percent of the same travel) the lever must come back below the gate to stow.
pub const REVERSE_GATE_HYSTERESIS: f64 = 4.0;

/// Reverse gate of a throttle in percent (`REVERSEGATE` option, default [`DEFAULT_REVERSE_GATE`]).
/// Just past the idle detent the lever still means forward idle: resting a hand on a lever
/// sitting in the detent pushes it a little way back, which must not deploy the reversers.
pub fn reverse_gate(action: &EventActionRangedAxis) -> f64 {
    action.option_f64("REVERSEGATE").unwrap_or(DEFAULT_REVERSE_GATE).clamp(0.0, 90.0)
}

/// Fraction of the travel from the far edge of the idle zone (0) to the `REVERSE` zone (1)
/// when the lever is past the idle zone on the reverse side; `None` in the forward range.
/// Without a `REVERSE` zone the far end of the device range is full reverse.
pub fn reverse_travel(action: &EventActionRangedAxis, raw: i32, dev_min: i32, dev_max: i32) -> Option<f64> {
    if !has_reverse(action) {
        return None;
    }
    let raw = raw as f64;
    let idle = zone(action, "IDLE")?;
    let high = zone(action, "MAX").unwrap_or(Zone::new(dev_max as f64, None));
    let high_is_below = high.hi <= idle.lo;
    // The reverse side is the side of the idle zone away from full thrust.
    let (idle_far, past_idle) = if high_is_below { (idle.hi, raw > idle.hi) } else { (idle.lo, raw < idle.lo) };
    if !past_idle {
        return None;
    }
    let far_end = if high_is_below { dev_max.max(dev_min) as f64 } else { dev_min.min(dev_max) as f64 };
    let rev = zone(action, "REVERSE").unwrap_or(Zone::new(far_end, None));
    let rev_near = if high_is_below { rev.lo } else { rev.hi };
    let span = rev_near - idle_far;
    if span.abs() < f64::EPSILON {
        return Some(1.0);
    }
    Some(((raw - idle_far) / span).clamp(0.0, 1.0))
}

/// Reverse amount in `0.0..=1.0` (0 = reverse idle at the gate, 1 inside the `REVERSE` zone),
/// or `None` while the lever is forward of the reverse gate.
///
/// `was_reverse` is the previous state: once engaged, reverse stays engaged until the lever
/// comes back [`REVERSE_GATE_HYSTERESIS`] below the gate, so jitter at the gate does not
/// deploy and stow the reversers repeatedly.
pub fn reverse_amount(action: &EventActionRangedAxis, raw: i32, dev_min: i32, dev_max: i32, was_reverse: bool) -> Option<f64> {
    let t = reverse_travel(action, raw, dev_min, dev_max)?;
    let gate = reverse_gate(action) / 100.0;
    let threshold = if was_reverse { (gate - REVERSE_GATE_HYSTERESIS / 100.0).max(0.0) } else { gate };
    if t < threshold {
        return None;
    }
    if gate >= 1.0 {
        return Some(1.0);
    }
    Some(((t - gate) / (1.0 - gate)).clamp(0.0, 1.0))
}

/// Sim events to send for a lever movement (stateless: reverse engages at the gate).
pub fn lever_outputs(action: &EventActionRangedAxis, raw: i32, dev_min: i32, dev_max: i32) -> Option<Vec<(String, i32)>> {
    lever_outputs_with_state(action, raw, dev_min, dev_max, false).map(|(out, _)| out)
}

/// Sim events to send for a lever movement, plus whether the lever is now in reverse.
///
/// Forward travel uses the `*_AXIS_SET_EX1` / `AXIS_*_SET` events (-16383 idle .. 16383 full).
/// Past the reverse gate of a reverse-capable throttle, `THROTTLEn_SET` with a negative value
/// (0 .. -16383) is sent instead, which MSFS interprets as reverse thrust.
pub fn lever_outputs_with_state(
    action: &EventActionRangedAxis,
    raw: i32,
    dev_min: i32,
    dev_max: i32,
    was_reverse: bool,
) -> Option<(Vec<(String, i32)>, bool)> {
    let kind = LeverKind::from_config_id(&action.config_id)?;
    let engines: Vec<u8> = if kind.per_engine() {
        (1..=4u8).filter(|n| action.option_bool(&format!("OPTION{}", n))).collect()
    } else {
        Vec::new()
    };
    if let Some(rev) = reverse_amount(action, raw, dev_min, dev_max, was_reverse) {
        let value = -(rev * 16383.0).round() as i32;
        let out = if engines.is_empty() {
            vec![("THROTTLE_SET".to_string(), value)]
        } else {
            engines.iter().map(|n| (format!("THROTTLE{}_SET", n), value)).collect()
        };
        return Some((out, true));
    }
    let pos = lever_position(action, raw, dev_min, dev_max);
    let value = (-16383.0 + pos * 32766.0).round() as i32;

    let out = if engines.is_empty() {
        vec![(kind.event(None)?, value)]
    } else {
        engines.into_iter().filter_map(|n| kind.event(Some(n)).map(|e| (e, value))).collect()
    };
    Some((out, false))
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
    fn tca_reverse_past_idle_detent() {
        // TCA: full = raw 0, idle detent ~49 000, lever pulled back to 65535 = full reverse
        let l = lever(
            "LEVER_THROTTLE",
            &[("MAX", "0"), ("MAXTO", "0"), ("IDLE", "50934"), ("IDLETO", "48647"), ("OPTION1", "True")],
        );
        let out = |raw| lever_outputs(&l, raw, 0, 65535).unwrap();
        assert_eq!(out(0), vec![("THROTTLE1_AXIS_SET_EX1".to_string(), 16383)]);
        assert_eq!(out(49500), vec![("THROTTLE1_AXIS_SET_EX1".to_string(), -16383)], "inside detent = idle");
        assert_eq!(out(65535), vec![("THROTTLE1_SET".to_string(), -16383)], "full reverse");
        // Just past the detent (hand resting on the lever) is still forward idle
        assert_eq!(out(51500), vec![("THROTTLE1_AXIS_SET_EX1".to_string(), -16383)], "inside reverse gate");
        // Halfway between the gate (20%) and full reverse
        let gate = 50934.0 + (65535.0 - 50934.0) * 0.2;
        let half = out(((gate + 65535.0) / 2.0) as i32)[0].1;
        assert!((-8300..=-8000).contains(&half), "half reverse, got {}", half);

        // Hysteresis: once in reverse, stays engaged slightly below the gate
        let just_below = (gate - 300.0) as i32;
        assert!(!lever_outputs_with_state(&l, just_below, 0, 65535, false).unwrap().1);
        let (o, rev) = lever_outputs_with_state(&l, just_below, 0, 65535, true).unwrap();
        assert!(rev);
        assert_eq!(o[0], ("THROTTLE1_SET".to_string(), 0));
        let back = (50934.0 + (65535.0 - 50934.0) * 0.1) as i32;
        assert!(!lever_outputs_with_state(&l, back, 0, 65535, true).unwrap().1, "stows below the hysteresis");

        // Gate 0 = old behaviour, reverse right past the detent
        let mut g0 = l.clone();
        g0.set_option("REVERSEGATE", "0");
        assert_eq!(lever_outputs(&g0, 51500, 0, 65535).unwrap()[0].0, "THROTTLE1_SET");

        // explicit reverse zone: everything beyond 60000 is full reverse
        let mut z = l.clone();
        z.set_option("REVERSE", "60000");
        z.set_option("REVERSETO", "65535");
        assert_eq!(lever_outputs(&z, 61000, 0, 65535).unwrap()[0].1, -16383);

        // NOREVERSE keeps clamping to idle
        let nr = lever("LEVER_NOREVERSE_THROTTLE", &[("MAX", "0"), ("IDLE", "50934"), ("IDLETO", "48647")]);
        assert_eq!(lever_outputs(&nr, 65535, 0, 65535).unwrap(), vec![("AXIS_THROTTLE_SET".to_string(), -16383)]);
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
