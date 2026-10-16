//! `wardrobe_take_off` handler (v4
//! `lib/tools/handlers/wardrobe-take-off-handler.ts` `executeWardrobeTakeOffTool` +
//! `formatWardrobeTakeOffResults`).
//!
//! Applies an ordered array of take-off operations in sequence (fails fast on the
//! first bad op), over the tool call's ONE wearable pool (v4 `9ecebf986`).
//! `remove` finds the item in the pool (`find_in_pool` — archived garments CAN
//! be taken off) and takes it off every slot it covers, or the one named
//! (`wear_ops::take_off_wearable`). Works for single garments and composites:
//! a composite comes off by its leaves, since worn composites are stored
//! dissolved (v4 `f0db8cbb8`). `clear_slot` empties one slot (ONE
//! `apply_displacement` in `clear_slot` mode). The outfit-change effects (v4
//! `notifyWardrobeChanged`) fire ONCE when at least one op landed: the returned
//! ids are the characters whose outfit changed, fired by the executor once the
//! write commits.

use rusqlite::Connection;
use serde::Serialize;
use serde_json::Value;

use crate::db::wardrobe_wear_stats::EquipSource;
use crate::db::DbError;
use crate::services::outfit_displacement::apply_displacement;
use crate::slot_ops::{ComputeDisplacedOptions, DisplacementMode};
use crate::wardrobe::{describe_wardrobe_effect, normalize_no_item_sentinel, WardrobeEffect};
use crate::wear_ops::{
    find_in_pool, take_off_wearable, wardrobe_item_not_found_message, WearableRef,
};

use super::wardrobe_shared::{empty_equipped_state, load_current_wardrobe_state};
use super::wardrobe_tool_table::load_tool_pool;
use super::wardrobe_wear::OpItem;

/// v4 `WardrobeTakeOffOpResult`.
#[derive(Debug, Serialize)]
pub struct WardrobeTakeOffOpResult {
    pub mode: String,
    pub effect: String,
    pub effect_summary: String,
    pub item: Option<OpItem>,
    pub slots_affected: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// v4 `WardrobeTakeOffToolOutput`.
#[derive(Debug, Serialize)]
pub struct WardrobeTakeOffToolOutput {
    pub success: bool,
    pub operations: Vec<WardrobeTakeOffOpResult>,
    pub current_state: Value,
    pub coverage_summary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

fn failure(error: impl Into<String>) -> WardrobeTakeOffToolOutput {
    WardrobeTakeOffToolOutput {
        success: false,
        operations: Vec::new(),
        current_state: empty_equipped_state().to_value(),
        coverage_summary: String::new(),
        error: Some(error.into()),
    }
}

const VALIDATION_ERROR: &str = "Invalid input: provide a non-empty \"operations\" array. mode=remove needs an item_id or item_title; mode=clear_slot needs a slot.";
const MODE_ENUM: [&str; 2] = ["remove", "clear_slot"];
const SLOT_ENUM: [&str; 5] = crate::wardrobe::WARDROBE_SLOT_TYPES;

struct TakeOffOp {
    item_id: Option<String>,
    item_title: Option<String>,
    mode: Option<String>,
    slot: Option<String>,
}

/// v4 `validateWardrobeTakeOffInput` — a non-empty `operations` array, superRefine:
/// `clear_slot` requires a slot; `remove` requires `item_id`/`item_title`.
fn validate(args: &Value) -> Option<Vec<TakeOffOp>> {
    let obj = args.as_object()?;
    let ops_val = obj.get("operations")?.as_array()?;
    if ops_val.is_empty() {
        return None;
    }
    let mut ops = Vec::with_capacity(ops_val.len());
    for op in ops_val {
        let o = op.as_object()?;
        let item_id = opt_str_field(o, "item_id")?;
        let item_title = opt_str_field(o, "item_title")?;
        let mode = opt_enum_field(o, "mode", &MODE_ENUM)?;
        let slot = opt_enum_field(o, "slot", &SLOT_ENUM)?;
        let effective_mode = mode.as_deref().unwrap_or("remove");
        if effective_mode == "clear_slot" && slot.is_none() {
            return None;
        }
        if effective_mode == "remove" && item_id.is_none() && item_title.is_none() {
            return None;
        }
        ops.push(TakeOffOp {
            item_id,
            item_title,
            mode,
            slot,
        });
    }
    Some(ops)
}

fn opt_str_field(o: &serde_json::Map<String, Value>, key: &str) -> Option<Option<String>> {
    match o.get(key) {
        None | Some(Value::Null) => Some(None),
        Some(Value::String(s)) => Some(Some(s.clone())),
        Some(_) => None,
    }
}

fn opt_enum_field(
    o: &serde_json::Map<String, Value>,
    key: &str,
    allowed: &[&str],
) -> Option<Option<String>> {
    match o.get(key) {
        None | Some(Value::Null) => Some(None),
        Some(Value::String(s)) if allowed.contains(&s.as_str()) => Some(Some(s.clone())),
        Some(_) => None,
    }
}

/// v4's `context` on every line the handler logs.
const LOG_CONTEXT: &str = "wardrobe-take-off-handler";

/// Execute `wardrobe_take_off` (v4 `executeWardrobeTakeOffTool`). Returns the
/// output plus the characters whose outfit changed (non-empty when an op landed).
pub fn execute(
    main: &Connection,
    mount: &Connection,
    user_id: &str,
    chat_id: &str,
    character_id: &str,
    args: &Value,
) -> (WardrobeTakeOffToolOutput, Vec<String>) {
    let Some(ops) = validate(args) else {
        tracing::warn!(
            target: "quilltap::tools",
            context = LOG_CONTEXT,
            userId = user_id,
            chatId = chat_id,
            characterId = character_id,
            inputJson = %args,
            "Wardrobe take off tool validation failed",
        );
        return (failure(VALIDATION_ERROR), Vec::new());
    };
    // v4 has no outer catch here: the per-op catch takes every op failure, and
    // only the closing state read can throw past it.
    match run(main, mount, user_id, chat_id, character_id, &ops) {
        Ok(pair) => pair,
        Err(e) => (failure(crate::db::fallback::error_text(&e)), Vec::new()),
    }
}

fn run(
    main: &Connection,
    mount: &Connection,
    user_id: &str,
    chat_id: &str,
    character_id: &str,
    ops: &[TakeOffOp],
) -> Result<(WardrobeTakeOffToolOutput, Vec<String>), DbError> {
    let pool = load_tool_pool(main, mount, chat_id, character_id);

    let mut results: Vec<WardrobeTakeOffOpResult> = Vec::new();
    let mut applied_count = 0usize;
    let mut failed_error: Option<String> = None;

    for op in ops {
        let mode = op.mode.clone().unwrap_or_else(|| "remove".to_string());
        let item_id = normalize_no_item_sentinel(op.item_id.as_deref());
        let item_title = normalize_no_item_sentinel(op.item_title.as_deref());

        // v4's per-op `try` — a miss or a failed write is this op's error.
        let attempt: Result<WardrobeTakeOffOpResult, String> = (|| {
            if mode == "clear_slot" {
                // `op.slot` is validated present for clear_slot.
                let slot = op.slot.clone().unwrap_or_default();
                apply_displacement(
                    main,
                    chat_id,
                    character_id,
                    &ComputeDisplacedOptions {
                        mode: DisplacementMode::ClearSlot,
                        item: None,
                        slot: Some(&slot),
                        item_id: None,
                        items_by_id: None,
                    },
                    EquipSource::Ui,
                )
                .map_err(|e| crate::db::fallback::error_text(&e))?;
                tracing::info!(
                    target: "quilltap::tools",
                    context = LOG_CONTEXT,
                    userId = user_id,
                    chatId = chat_id,
                    characterId = character_id,
                    slot = %slot,
                    "Wardrobe slot cleared",
                );
                return Ok(WardrobeTakeOffOpResult {
                    mode: mode.clone(),
                    effect: "cleared".to_string(),
                    effect_summary: describe_wardrobe_effect(
                        WardrobeEffect::Cleared,
                        std::slice::from_ref(&slot),
                        None,
                    ),
                    item: None,
                    slots_affected: vec![slot],
                    error: None,
                });
            }

            // mode === 'remove' — the SENTINEL-normalised values name a miss.
            let Some(item) = find_in_pool(
                &pool,
                WearableRef {
                    item_id: item_id.as_deref(),
                    item_title: item_title.as_deref(),
                },
            ) else {
                return Err(wardrobe_item_not_found_message(
                    item_id.as_deref(),
                    item_title.as_deref(),
                ));
            };
            let id = item
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let title = item
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            // Restrict to one slot if given, else take it off every slot it
            // covers (a composite: every slot one of its leaves occupies).
            // `slots_affected` is what was COMMITTED AGAINST.
            let slots_affected = take_off_wearable(
                main,
                chat_id,
                &pool,
                item,
                op.slot.as_deref().filter(|s| !s.is_empty()),
            )
            .map_err(|e| crate::db::fallback::error_text(&e))?
            .slots_affected;
            tracing::info!(
                target: "quilltap::tools",
                context = LOG_CONTEXT,
                userId = user_id,
                chatId = chat_id,
                characterId = character_id,
                itemId = %id,
                itemTitle = %title,
                slotsAffectedJson = %serde_json::to_string(&slots_affected).unwrap_or_default(),
                "Wardrobe item taken off",
            );
            Ok(WardrobeTakeOffOpResult {
                mode: mode.clone(),
                effect: "removed".to_string(),
                effect_summary: describe_wardrobe_effect(
                    WardrobeEffect::Removed,
                    &slots_affected,
                    Some(title.as_str()),
                ),
                item: Some(OpItem { item_id: id, title }),
                slots_affected,
                error: None,
            })
        })();

        match attempt {
            Ok(res) => {
                results.push(res);
                applied_count += 1;
            }
            Err(message) => {
                tracing::warn!(
                    target: "quilltap::tools",
                    context = LOG_CONTEXT,
                    userId = user_id,
                    chatId = chat_id,
                    characterId = character_id,
                    mode = %mode,
                    // v4's `message` key, renamed (a tracing `message` field
                    // replaces the file record's envelope text — recorded).
                    errorMessage = %message,
                    "Wardrobe take off operation failed",
                );
                results.push(WardrobeTakeOffOpResult {
                    mode,
                    effect: "removed".to_string(),
                    effect_summary: String::new(),
                    item: None,
                    slots_affected: Vec::new(),
                    error: Some(message.clone()),
                });
                failed_error = Some(message);
                break; // fail-fast
            }
        }
    }

    // v4 `finalizeWardrobeMutation`: the effects fire ONCE, only if at least
    // one op landed (the executor fires them once the write commits).
    let changed = if applied_count > 0 {
        vec![character_id.to_string()]
    } else {
        Vec::new()
    };

    let current_state = load_current_wardrobe_state(main, chat_id, character_id)?;
    // v4 `buildWardrobeCoverageSummaryFromState(pool, slots)` =
    // `describeOutfit(resolveEquippedOutfitForCharacter(pool, slots).outfitValues)`
    // — pure over the pool the tool already loaded (no second read).
    let coverage_summary = crate::wardrobe::describe_outfit(
        &crate::services::resolve_equipped::resolve_equipped_outfit_for_character(
            &pool,
            &current_state,
        )
        .outfit_values,
    );

    Ok((
        WardrobeTakeOffToolOutput {
            success: failed_error.is_none(),
            operations: results,
            current_state: current_state.to_value(),
            coverage_summary,
            error: failed_error,
        },
        changed,
    ))
}

/// v4 `formatWardrobeTakeOffResults`.
pub fn format(output: &WardrobeTakeOffToolOutput) -> String {
    if !output.success && output.operations.is_empty() {
        return format!(
            "Wardrobe Error: {}",
            output.error.as_deref().unwrap_or("Unknown error")
        );
    }
    let mut lines: Vec<String> = Vec::new();
    for op in &output.operations {
        if let Some(err) = &op.error {
            lines.push(format!("Failed: {err}"));
        } else if !op.effect_summary.is_empty() {
            lines.push(op.effect_summary.clone());
        }
    }
    lines.push(String::new());
    lines.push("Current outfit:".to_string());
    for slot in crate::wardrobe::WARDROBE_SLOT_TYPES {
        let ids = output
            .current_state
            .get(slot)
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|e| e.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default();
        // An unreported-if-blank slot (hair) is omitted entirely when empty
        // rather than listed as "(empty)" — the model must never read that as
        // baldness.
        if ids.is_empty() && !crate::wardrobe::is_slot_reported_when_empty(slot) {
            continue;
        }
        let label = if ids.is_empty() {
            "(empty)".to_string()
        } else {
            ids
        };
        lines.push(format!("  {slot}: {label}"));
    }
    lines.push(String::new());
    lines.push(format!("Summary: {}", output.coverage_summary));
    lines.join("\n")
}
