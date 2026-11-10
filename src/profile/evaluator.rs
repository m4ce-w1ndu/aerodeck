use std::collections::HashMap;
use tracing::debug;

use super::schema::*;

pub struct ConditionEvaluator<'a> {
    pub lvars: &'a HashMap<String, f64>,
    pub local_vars: &'a HashMap<String, String>,
    /// SimVar values keyed by canonical name (see `simconnect::simvars::SimVarRef`).
    pub simvars: Option<&'a HashMap<String, f64>>,
    /// PMDG SDK values keyed as in `SimConnectClient::pmdg_snapshot`.
    pub pmdg: Option<&'a HashMap<String, String>>,
}

impl<'a> ConditionEvaluator<'a> {
    pub fn new(lvars: &'a HashMap<String, f64>, local_vars: &'a HashMap<String, String>) -> Self {
        Self { lvars, local_vars, simvars: None, pmdg: None }
    }

    pub fn with_pmdg(mut self, pmdg: &'a HashMap<String, String>) -> Self {
        self.pmdg = Some(pmdg);
        self
    }

    pub fn with_simvars(mut self, simvars: &'a HashMap<String, f64>) -> Self {
        self.simvars = Some(simvars);
        self
    }

    pub fn evaluate_definition(&self, def: &EventDefinition) -> bool {
        let conditions = match &def.conditions {
            Some(c) if !c.items.is_empty() => &c.items,
            _ => return true, // No conditions means unconditionally true
        };

        let is_any = def.condition_binding.as_deref().unwrap_or("All").eq_ignore_ascii_case("Any");

        for item in conditions {
            let res = match item {
                EventConditionItem::Simple(simple) => self.evaluate_simple(simple),
                EventConditionItem::Expression(expr) => self.evaluate_expression(expr),
            };

            if is_any && res {
                return true;
            } else if !is_any && !res {
                return false;
            }
        }

        !is_any
    }

    pub fn resolve_value(&self, target: &str) -> Option<String> {
        let t = target.trim();
        let Some((prefix, name)) = split_data_prefix(t) else {
            // Direct literal
            return Some(t.to_string());
        };
        match prefix.to_ascii_uppercase().as_str() {
            "LVAR" => {
                let clean = name.trim_start_matches("L:").trim();
                self.lvars.get(clean).map(|v| v.to_string())
            }
            "LOCAL" => self.local_vars.get(name).cloned(),
            "SIMCONNECT" | "MSFS" => {
                let key = crate::simconnect::simvars::SimVarRef::parse(name)?.key;
                self.simvars?.get(&key).map(|v| v.to_string())
            }
            other => {
                if let Some(key) = crate::pmdg::snapshot_key(other, name) {
                    return self.pmdg?.get(&key).cloned();
                }
                debug!("Data source '{}' is not supported yet", t);
                None
            }
        }
    }

    pub fn evaluate_simple(&self, cond: &EventConditionSimple) -> bool {
        let current_val = match self.resolve_value(&cond.target) {
            Some(v) => v,
            None => {
                debug!("Condition target '{}' not resolved, evaluating to false", cond.target);
                return false;
            }
        };

        let target_val = cond.value.as_deref().unwrap_or("");
        let comparator = cond.comparator.as_deref().unwrap_or("Equals");

        match comparator {
            "Equals" => current_val == target_val || (self.as_f64(&current_val) == self.as_f64(target_val) && self.as_f64(&current_val).is_some()),
            "Unequal" | "NotEquals" => current_val != target_val,
            "Less" => match (self.as_f64(&current_val), self.as_f64(target_val)) {
                (Some(a), Some(b)) => a < b,
                _ => false,
            },
            "LessOrEqual" => match (self.as_f64(&current_val), self.as_f64(target_val)) {
                (Some(a), Some(b)) => a <= b,
                _ => false,
            },
            "Greater" => match (self.as_f64(&current_val), self.as_f64(target_val)) {
                (Some(a), Some(b)) => a > b,
                _ => false,
            },
            "GreaterOrEqual" => match (self.as_f64(&current_val), self.as_f64(target_val)) {
                (Some(a), Some(b)) => a >= b,
                _ => false,
            },
            "Mask" | "AnyBitSet" => match (self.as_u64(&current_val), self.as_u64(target_val)) {
                (Some(a), Some(mask)) => (a & mask) != 0,
                _ => false,
            },
            "IsBitSet" => match (self.as_u64(&current_val), self.as_u64(target_val)) {
                (Some(a), Some(bit)) => (a & (1 << bit)) != 0,
                _ => false,
            },
            "IsBitNotSet" => match (self.as_u64(&current_val), self.as_u64(target_val)) {
                (Some(a), Some(bit)) => (a & (1 << bit)) == 0,
                _ => false,
            },
            "StrContains" => current_val.contains(target_val),
            "Always" => true,
            "Ignore" | "None" => true,
            _ => {
                debug!("Unsupported comparator '{}', default to equality", comparator);
                current_val == target_val
            }
        }
    }

    pub fn evaluate_expression(&self, expr: &EventConditionExpression) -> bool {
        // Basic expression evaluation support
        debug!("Evaluating expression condition: {}", expr.expression);
        true
    }

    fn as_f64(&self, s: &str) -> Option<f64> {
        s.trim().parse::<f64>().ok()
    }

    fn as_u64(&self, s: &str) -> Option<u64> {
        let t = s.trim();
        if let Some(hex) = t.strip_prefix("0x") {
            u64::from_str_radix(hex, 16).ok()
        } else {
            t.parse::<u64>().ok()
        }
    }
}

/// Splits `PREFIX:rest` when `PREFIX` looks like a SPAD.neXt data provider
/// (`LVAR`, `LOCAL`, `SIMCONNECT`, `PMDGNG3`, ...). Returns `None` for plain literals.
pub fn split_data_prefix(t: &str) -> Option<(&str, &str)> {
    let (prefix, rest) = t.split_once(':')?;
    let is_provider = prefix.len() >= 2
        && prefix.chars().all(|c| c.is_ascii_alphanumeric())
        && prefix.chars().next().is_some_and(|c| c.is_ascii_alphabetic());
    is_provider.then_some((prefix, rest))
}
