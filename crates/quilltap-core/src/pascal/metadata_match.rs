//! Fail-soft metadata comparison — one semantics table, two callers (v4
//! `lib/pascal/metadata-match.ts`, `6864bf0e`).
//!
//! A `metadata` comparator names a key on a character the definition has never
//! met, so nothing about the subject is knowable at load time: the key may be
//! absent, may hold a list, may hold a string where the table wanted a number.
//! Each of those simply fails to match. That rule is load-bearing in two
//! places — the outcome table ([`super::custom_tools`], at roll time) and the
//! availability gate ([`super::tool_gate`], before a tool is ever offered) — and
//! a second implementation of it would drift the moment one side gained a
//! comparator.
//!
//! CLIENT-SAFE in v4's sense: no logging, no repositories, no IO. What the
//! caller cannot know is injected — how to resolve an operand, and where to log
//! a declined row.
//!
//! # Why [`ResolvedValue`] lives here
//!
//! v4's `Primitive` (`number | string | boolean`) is declared in this module and
//! imported by the execution core; v5's equivalent is `ResolvedValue`, which
//! predates the extraction and lived in `custom_tools.rs`. It moved here with
//! the table so the dependency runs the same direction it does in v4 — the
//! shared semantics know nothing about the run. `custom_tools` re-exports it, so
//! every existing `pascal::custom_tools::ResolvedValue` path still resolves.

use serde_json::{Map, Value};

use super::custom_tool_types::MetadataComparator;
use super::js_value::json_stringify;

/// One resolved value: v4's `Primitive` — `number | string | boolean`, the
/// value types a comparator can actually compare.
#[derive(Debug, Clone, PartialEq)]
pub enum ResolvedValue {
    Number(f64),
    String(String),
    Bool(bool),
}

impl ResolvedValue {
    /// JS `===` across the three types a parameter can hold.
    pub(super) fn strict_eq(&self, other: &ResolvedValue) -> bool {
        match (self, other) {
            (ResolvedValue::Number(a), ResolvedValue::Number(b)) => a == b,
            (ResolvedValue::String(a), ResolvedValue::String(b)) => a == b,
            (ResolvedValue::Bool(a), ResolvedValue::Bool(b)) => a == b,
            _ => false,
        }
    }

    /// As a JSON [`Value`], the way v4 stores a resolved param / tested-metadata
    /// value: a number is stored JS-bare (an integer without a fractional part),
    /// a string as a string, a boolean as a boolean.
    pub fn to_value(&self) -> Value {
        match self {
            ResolvedValue::Number(n) => crate::db::js_number_to_json(*n),
            ResolvedValue::String(s) => Value::String(s.clone()),
            ResolvedValue::Bool(b) => Value::Bool(*b),
        }
    }

    /// The `JSON.stringify(value)` several error sentences interpolate.
    pub(super) fn stringify(&self) -> String {
        json_stringify(&self.to_value())
    }
}

/// v4 `isPrimitive` folded with the JSON-value coercion: the value types a
/// comparator can compare, as a [`ResolvedValue`]. `None` for null/array/object.
pub fn js_primitive(v: &Value) -> Option<ResolvedValue> {
    match v {
        Value::Number(n) => Some(ResolvedValue::Number(n.as_f64().unwrap_or(f64::NAN))),
        Value::String(s) => Some(ResolvedValue::String(s.clone())),
        Value::Bool(b) => Some(ResolvedValue::Bool(*b)),
        _ => None,
    }
}

/// The comparator keys this table walks, in v4's own evaluation order: the four
/// orderings, then `eq`, then `neq`, then the two containments.
const WALK_ORDER: [&str; 8] = [
    "gt",
    "gte",
    "lt",
    "lte",
    "eq",
    "neq",
    "contains",
    "ncontains",
];

/// Evaluate one comparator against one metadata key. Keys AND together (v4
/// `metadataComparatorHolds`).
///
/// Everything a declared-parameter comparison would treat as a regression is,
/// here, an ordinary fact about a character — so it declines the row rather than
/// failing. Failing would punish an author whose table is working exactly as
/// designed: a lockpicking table branches on the key its author invented, and
/// must still deal sensibly to the character who has never heard of it.
///
/// Operand resolution is the caller's business, and may still fail: a `$param`
/// reference IS load-validated, so its failure is a regression rather than a
/// fact about the character. The gate, whose operands are literals by
/// construction, passes a resolver that cannot fail.
///
/// `on_decline` receives the reason a row declined, for a caller that logs it.
pub fn metadata_comparator_holds<E>(
    comparator: &MetadataComparator,
    key: &str,
    metadata: &Map<String, Value>,
    resolve_operand: &mut dyn FnMut(&str) -> Result<ResolvedValue, E>,
    on_decline: &mut dyn FnMut(String),
) -> Result<bool, E> {
    let mut decline = |reason: String| -> Result<bool, E> {
        on_decline(reason);
        Ok(false)
    };

    // The character has no such metadata key — decline before touching operands,
    // so a `$param` operand is never even resolved (let alone failed over).
    let Some(raw) = metadata.get(key) else {
        return decline("the character has no such metadata key".into());
    };

    let Some(subject) = js_primitive(raw) else {
        let held = match raw {
            Value::Null => "null",
            Value::Array(_) => "an array",
            _ => "an object",
        };
        return decline(format!("the key holds {held}, which cannot be compared"));
    };

    for ck in WALK_ORDER {
        if !comparator.has(ck) {
            continue;
        }
        // v4 resolves the operand FIRST (so a `$param` still fails), THEN
        // declines when the two sides cannot be compared.
        let operand = resolve_operand(ck)?;

        if matches!(ck, "gt" | "gte" | "lt" | "lte") {
            let (ResolvedValue::Number(s), ResolvedValue::Number(o)) = (&subject, &operand) else {
                return decline(format!(
                    "{ck} orders {}, and only numbers can be ordered",
                    subject.stringify()
                ));
            };
            let held = match ck {
                "gt" => s > o,
                "gte" => s >= o,
                "lt" => s < o,
                _ => s <= o,
            };
            if !held {
                return Ok(false);
            }
            continue;
        }

        // Containment follows the same fail-soft rule as ordering: a key holding
        // anything but a string cannot be searched, so the row declines —
        // including under ncontains, where (as with neq) absence-of-a-string is
        // not a miss.
        if matches!(ck, "contains" | "ncontains") {
            let (ResolvedValue::String(hay), ResolvedValue::String(needle)) = (&subject, &operand)
            else {
                return decline(format!(
                    "{ck} searches {}, and only a string can contain a substring",
                    subject.stringify()
                ));
            };
            let held = hay.contains(needle.as_str());
            if if ck == "contains" { !held } else { held } {
                return Ok(false);
            }
            continue;
        }

        let held = if ck == "eq" {
            subject.strict_eq(&operand)
        } else {
            !subject.strict_eq(&operand)
        };
        if !held {
            return Ok(false);
        }
    }

    Ok(true)
}
