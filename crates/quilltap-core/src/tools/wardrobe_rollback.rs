//! `wardrobe_rollback` handler (v4 `lib/tools/handlers/wardrobe-rollback-handler.ts`
//! `executeWardrobeRollbackTool` + `formatWardrobeRollbackResults`, `ebb480edd`).
//!
//! Return to an outfit worn earlier in this chat, by its number in the outfit
//! history (default: one step back). The tool loads the CHARACTER's pool (the
//! project roster applies — a character tool call is never the operator) and
//! runs the ONE rollback ([`crate::wardrobe::rollback`]) with ledger source
//! `tool`. Inside the write it answers the output plus the [`RollbackStage`];
//! the table's after-commit half fires the effects with the stage's CURSOR
//! intent and then logs `Rolled a wardrobe back` (v4's order).

use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::db::wardrobe_wear_stats::EquipSource;
use crate::wardrobe::rollback::{
    rollback_in_write, RollbackInput, RollbackOutcome, RollbackSkipReason, RollbackStage,
};

use super::wardrobe_shared::{
    coverage_summary_from_state, format_equipped_slot_lines, load_current_wardrobe_state,
    zod_safe_integer,
};
use super::wardrobe_tool_table::load_tool_pool;

/// v4's handler `CONTEXT`.
pub const CONTEXT: &str = "wardrobe-rollback-handler";

const VALIDATION_ERROR: &str =
    "Invalid input: \"index\" must be a whole number of 0 or more, or omitted.";

/// v4 `WardrobeRollbackSkippedItem` (the tool's snake_case record).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WardrobeRollbackSkippedItem {
    pub item_id: String,
    pub title: Option<String>,
    pub reason: String,
}

/// v4 `WardrobeRollbackToolOutput` — a success carries every field in v4's
/// order; a failure only `success` + `error`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WardrobeRollbackToolOutput {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_index: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub restored_index: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_state: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coverage_summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skipped: Option<Vec<WardrobeRollbackSkippedItem>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

fn failure(error: impl Into<String>) -> WardrobeRollbackToolOutput {
    WardrobeRollbackToolOutput {
        success: false,
        previous_index: None,
        restored_index: None,
        current_state: None,
        coverage_summary: None,
        skipped: None,
        error: Some(error.into()),
    }
}

/// v4 `validateWardrobeRollbackInput` — `wardrobeRollbackToolInputSchema
/// .safeParse(input ?? {})` over `{ index: z.number().int().nonnegative()
/// .optional() }`: `null` input is `{}` (no index); `{ index: null }` is
/// refused (`.optional()` admits `undefined` only); a string `"2"` is refused
/// (only the text-block converter numbers attributes); `.int()` is
/// SAFE-integer; extra keys are stripped; an array or a scalar is refused.
/// `Some(None)` = valid with no index.
pub fn validate(input: &Value) -> Option<Option<i64>> {
    let obj = match input {
        Value::Null => return Some(None),
        Value::Object(obj) => obj,
        _ => return None,
    };
    match obj.get("index") {
        None => Some(None),
        Some(v) => zod_safe_integer(v).filter(|i| *i >= 0).map(Some),
    }
}

/// v4 `executeWardrobeRollbackTool`, inside the write (both connections). The
/// stage is `Some` only when a rollback landed.
pub fn execute(
    main: &Connection,
    mount: &Connection,
    chat_id: &str,
    character_id: &str,
    input: &Value,
) -> (WardrobeRollbackToolOutput, Option<RollbackStage>) {
    let Some(index) = validate(input) else {
        tracing::warn!(
            context = CONTEXT,
            chatId = chat_id,
            characterId = character_id,
            inputJson = %serde_json::to_string(input).unwrap_or_default(),
            "Wardrobe rollback tool validation failed"
        );
        return (failure(VALIDATION_ERROR), None);
    };
    if character_id.is_empty() {
        return (
            failure("wardrobe_rollback needs a character to dress."),
            None,
        );
    }

    let pool = load_tool_pool(main, mount, chat_id, character_id);
    let outcome = rollback_in_write(
        main,
        RollbackInput {
            chat_id,
            character_id,
            pool: &pool,
            index,
            source: EquipSource::Tool,
            source_context: CONTEXT,
        },
    );
    let stage = match outcome {
        Ok(RollbackOutcome::Rolled(stage)) => stage,
        Ok(RollbackOutcome::Refused(refusal)) => {
            tracing::info!(
                context = CONTEXT,
                chatId = chat_id,
                characterId = character_id,
                reason = refusal.reason.as_str(),
                "Wardrobe rollback refused"
            );
            return (failure(refusal.message), None);
        }
        Err(error) => return (failed(chat_id, character_id, &error), None),
    };

    // v4 re-reads the slots AFTER the notify; the notify writes no slots, so
    // reading them here, after the commit, inside the same write, is the same.
    // The outfit is COMMITTED by now and v4's notify + cursor settle have
    // already fired inside `rollbackWardrobe`, so a failed re-read (today
    // unreachable — `load_current_wardrobe_state` is infallible) must still
    // hand the stage out: the announcement and the cursor intent belong to
    // the landed change, not to this read (the `e8e17b2d9` unification's §3
    // review).
    let current_state = match load_current_wardrobe_state(main, chat_id, character_id) {
        Ok(state) => state,
        Err(error) => return (failed(chat_id, character_id, &error), Some(stage)),
    };
    let output = WardrobeRollbackToolOutput {
        success: true,
        previous_index: Some(stage.previous_index),
        restored_index: Some(stage.restored_index),
        current_state: Some(current_state.to_value()),
        coverage_summary: Some(coverage_summary_from_state(&pool, &current_state)),
        skipped: Some(
            stage
                .skipped
                .iter()
                .map(|item| WardrobeRollbackSkippedItem {
                    item_id: item.item_id.clone(),
                    title: item.title.clone(),
                    reason: item.reason.as_str().to_string(),
                })
                .collect(),
        ),
        error: None,
    };
    (output, Some(stage))
}

/// v4's catch — WARN `Wardrobe rollback failed` + `Could not return to that
/// outfit: <message>`.
fn failed(
    chat_id: &str,
    character_id: &str,
    error: &crate::db::DbError,
) -> WardrobeRollbackToolOutput {
    let message = crate::db::fallback::error_text(error);
    tracing::warn!(
        context = CONTEXT,
        chatId = chat_id,
        characterId = character_id,
        error = %message,
        "Wardrobe rollback failed"
    );
    failure(format!("Could not return to that outfit: {message}"))
}

/// v4 `formatWardrobeRollbackResults`. The skipped item's name rule is TRUTHY
/// (`item.title ? the … : item …` — an empty title reads `item <id>`), and the
/// last line says `Now wearing:`, NOT the wear / take-off formatters' `Summary:`.
pub fn format(output: &WardrobeRollbackToolOutput) -> String {
    if !output.success {
        let error = output.error.as_deref().filter(|e| !e.is_empty());
        return format!("Wardrobe Error: {}", error.unwrap_or("Unknown error"));
    }
    let js = |n: Option<i64>| n.map_or_else(|| "undefined".to_string(), |n| n.to_string());
    let mut lines = vec![format!(
        "Returned to outfit {} (from {}).",
        js(output.restored_index),
        js(output.previous_index)
    )];
    for item in output.skipped.iter().flatten() {
        let name = match item.title.as_deref().filter(|t| !t.is_empty()) {
            Some(title) => format!("the {title}"),
            None => format!("item {}", item.item_id),
        };
        lines.push(if item.reason == RollbackSkipReason::Archived.as_str() {
            format!("{name} could not be put back on: it has been archived.")
        } else {
            format!("{name} could not be put back on: it no longer exists.")
        });
    }
    if let Some(state) = &output.current_state {
        lines.push(String::new());
        lines.push("Current outfit:".to_string());
        lines.extend(format_equipped_slot_lines(state));
    }
    lines.push(String::new());
    lines.push(format!(
        "Now wearing: {}",
        output.coverage_summary.as_deref().unwrap_or("")
    ));
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    //! v4 `wardrobe-history-rollback-handlers.test.ts`'s rollback block (the
    //! pure halves) + the validation table the order names (T13).
    use super::*;
    use serde_json::json;

    #[test]
    fn validation_follows_the_zod_schema() {
        assert_eq!(validate(&json!(null)), Some(None));
        assert_eq!(validate(&json!({})), Some(None));
        assert_eq!(
            validate(&json!({ "index": 2, "extra": true })),
            Some(Some(2))
        );
        assert_eq!(validate(&json!({ "index": 0 })), Some(Some(0)));
        assert_eq!(validate(&json!({ "index": 2.0 })), Some(Some(2)));
        for bad in [
            json!({ "index": null }),
            json!({ "index": "2" }),
            json!({ "index": -1 }),
            json!({ "index": 1.5 }),
            json!({ "index": 9_007_199_254_740_992u64 }),
            json!([]),
            json!("index"),
            json!(3),
        ] {
            assert_eq!(validate(&bad), None, "{bad}");
        }
    }

    fn ok(
        skipped: Vec<WardrobeRollbackSkippedItem>,
        state: Option<Value>,
    ) -> WardrobeRollbackToolOutput {
        WardrobeRollbackToolOutput {
            success: true,
            previous_index: Some(3),
            restored_index: Some(2),
            current_state: state,
            coverage_summary: Some("a Greatcoat".into()),
            skipped: Some(skipped),
            error: None,
        }
    }

    #[test]
    fn formats_the_restored_outfit_and_the_stale_items() {
        let out = ok(
            vec![
                WardrobeRollbackSkippedItem {
                    item_id: "i-coat".into(),
                    title: Some("Greatcoat".into()),
                    reason: "archived".into(),
                },
                WardrobeRollbackSkippedItem {
                    item_id: "i-gone".into(),
                    title: None,
                    reason: "missing".into(),
                },
                WardrobeRollbackSkippedItem {
                    item_id: "i-blank".into(),
                    title: Some(String::new()),
                    reason: "archived".into(),
                },
            ],
            Some(
                json!({ "top": ["a", "b"], "bottom": [], "footwear": [], "accessories": [], "hair": [] }),
            ),
        );
        assert_eq!(
            format(&out),
            "Returned to outfit 2 (from 3).\n\
the Greatcoat could not be put back on: it has been archived.\n\
item i-gone could not be put back on: it no longer exists.\n\
item i-blank could not be put back on: it has been archived.\n\
\n\
Current outfit:\n  top: a, b\n  bottom: (empty)\n  footwear: (empty)\n  accessories: (empty)\n\
\n\
Now wearing: a Greatcoat"
        );
    }

    #[test]
    fn formats_a_failure_and_a_missing_state() {
        assert_eq!(format(&failure("nope")), "Wardrobe Error: nope");
        assert_eq!(format(&failure("")), "Wardrobe Error: Unknown error");
        let mut out = ok(Vec::new(), None);
        out.coverage_summary = None;
        assert_eq!(
            format(&out),
            "Returned to outfit 2 (from 3).\n\nNow wearing: "
        );
    }

    #[test]
    fn the_output_keeps_v4s_key_order() {
        let out = ok(Vec::new(), Some(json!({})));
        assert_eq!(
            serde_json::to_string(&out).unwrap(),
            r#"{"success":true,"previous_index":3,"restored_index":2,"current_state":{},"coverage_summary":"a Greatcoat","skipped":[]}"#
        );
        assert_eq!(
            serde_json::to_string(&failure("x")).unwrap(),
            r#"{"success":false,"error":"x"}"#
        );
    }
}
