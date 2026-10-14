//! Port of v4 `lib/memory/admission/describe.ts` (`8af764514`, Phase C of the
//! memory system refactor) — one debug-log line for an admission, built from
//! its outcome and trace; the extraction debug log stores this rather than
//! strings each writer assembled. Pure; safe anywhere.
//!
//! JS fidelity: cosines render through [`to_fixed`] (`toFixed(3)`; a NaN
//! cosine reads `NaN`); ids and `via`s are cut with a UTF-16 `slice(0, 8)`
//! ([`utf16_truncate`] — a non-BMP id keeps fewer than eight characters; a
//! cut through a surrogate pair decodes lossily, the helper's recorded
//! convention, where JS would keep a lone surrogate); a memory's
//! `reinforcementCount` renders as JS `String(number)`
//! ([`number_to_string`]), so a fractional count reads `2.5`.

use serde_json::Value;

use super::types::{AdmissionAction, AdmissionOutcome, NeighbourAssessment, RejectReason, Verdict};
use crate::jsnum::to_fixed;
use crate::jsstr::utf16_truncate;
use crate::pascal::js_value::number_to_string;

/// JS `String(value)` for a JSON value inside a template literal.
fn js_string(v: &Value) -> String {
    match v {
        Value::Null => "null".to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => number_to_string(n.as_f64().unwrap_or(f64::NAN)),
        Value::String(s) => s.clone(),
        Value::Array(items) => items
            .iter()
            .map(|item| match item {
                Value::Null => String::new(),
                other => js_string(other),
            })
            .collect::<Vec<_>>()
            .join(","),
        Value::Object(_) => "[object Object]".to_string(),
    }
}

/// v4's `x ?? fallback` over an optional row field.
fn present(v: Option<&Value>) -> Option<&Value> {
    v.filter(|v| !v.is_null())
}

/// v4 `neighbourNote`.
fn neighbour_note(a: &NeighbourAssessment) -> String {
    let verdict = if a.verdict == Verdict::Excluded {
        format!(
            "excluded:{}",
            a.excluded_by.map_or("undefined", |e| e.as_str())
        )
    } else {
        a.verdict.as_str().to_string()
    };
    let via = match a.via.as_deref().filter(|v| !v.is_empty()) {
        Some(via) => format!(" via {}", utf16_truncate(via, 8)),
        None => String::new(),
    };
    format!(
        "{} {} {verdict}{via}",
        utf16_truncate(&a.id, 8),
        to_fixed(a.cosine, 3)
    )
}

/// `[Memory] <what happened> <label> … — <summary>[ · <neighbours>]` (v4
/// `describeAdmission`). The neighbours are the non-`below` assessments.
pub fn describe_admission(outcome: &AdmissionOutcome, label: &str, summary: &str) -> String {
    let action = &outcome.action;
    let memory = outcome.memory.as_ref();
    let trace = &outcome.trace;
    // `memory?.id ?? (('into' in action && action.into) || 'unknown')`.
    let id = match memory.and_then(|m| present(m.get("id"))) {
        Some(id) => js_string(id),
        None => action
            .into_id()
            .filter(|into| !into.is_empty())
            .unwrap_or("unknown")
            .to_string(),
    };
    // `memory?.reinforcementCount ?? '?'`.
    let count = memory
        .and_then(|m| present(m.get("reinforcementCount")))
        .map_or_else(|| "?".to_string(), js_string);
    let head = match action {
        AdmissionAction::Insert => format!("Created {label}"),
        AdmissionAction::InsertLinked { link_to, reason } => {
            // `outcome.linkedIds.length || action.linkTo.length`.
            let linked = if outcome.linked_ids.is_empty() {
                link_to.len()
            } else {
                outcome.linked_ids.len()
            };
            format!(
                "Created {label} (linked to {linked} related, {})",
                reason.as_str()
            )
        }
        AdmissionAction::Reinforce { .. } => format!(
            "REINFORCED {label}: {id} (count {count}, +{} details)",
            outcome.novel_details.len()
        ),
        AdmissionAction::PromoteAndReinforce { .. } => {
            format!("REINFORCED {label}: {id} (promoted from cold, count {count})")
        }
        AdmissionAction::Absorb {
            into,
            redirected_from,
        } => {
            let best = trace.assessments.iter().find(|a| &a.id == into);
            let similarity = best.map_or_else(|| "n/a".to_string(), |b| to_fixed(b.cosine, 3));
            let via = match redirected_from.as_deref().filter(|v| !v.is_empty()) {
                Some(from) => format!(", via {from}"),
                None => String::new(),
            };
            format!(
                "SKIPPED near-duplicate {label}: absorbed into {id} (similarity {similarity}{via})"
            )
        }
        AdmissionAction::Reject { reason } => match reason {
            RejectReason::EmbeddingFailed => format!(
                "SKIPPED {label} — embedding generation failed: {}",
                trace.notes.last().map_or("unknown", String::as_str)
            ),
            RejectReason::Empty => format!("SKIPPED {label} — no usable text"),
        },
    };
    let neighbours: Vec<String> = trace
        .assessments
        .iter()
        .filter(|a| a.verdict != Verdict::Below)
        .map(neighbour_note)
        .collect();
    if neighbours.is_empty() {
        format!("[Memory] {head} — {summary}")
    } else {
        format!("[Memory] {head} — {summary} · {}", neighbours.join("; "))
    }
}
