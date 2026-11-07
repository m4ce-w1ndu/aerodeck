use anyhow::Result;
use std::collections::HashMap;
use tracing::debug;

use super::schema::*;

pub struct ConditionEvaluator<'a> {
    pub lvars: &'a HashMap<String, f64>,
    pub local_vars: &'a HashMap<String, String>,
}

impl<'a> ConditionEvaluator<'a> {
    pub fn new(lvars: &'a HashMap<String, f64>, local_vars: &'a HashMap<String, String>) -> Self {
        Self { lvars, local_vars }
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
        if let Some(lvar_name) = t.strip_prefix("LVAR:") {
            let clean = lvar_name.trim_start_matches("L:").trim();
            self.lvars.get(clean).map(|v| v.to_string())
        } else if let Some(local_name) = t.strip_prefix("LOCAL:") {
            self.local_vars.get(local_name).cloned()
        } else {
            // Direct literal or fallback
            Some(t.to_string())
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
