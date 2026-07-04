//! `wardrobe_take_off` handler (v4
//! `lib/tools/handlers/wardrobe-take-off-handler.ts` `executeWardrobeTakeOffTool` +
//! `formatWardrobeTakeOffResults`).
//!
//! Applies an ordered array of take-off operations in sequence (fails fast on the
//! first bad op). `remove` filters a named item out of every slot it covers (or a
//! single named slot); `clear_slot` empties one slot. Avatar generation + the
//! pending wardrobe announcement fire ONCE after the loop when at least one op
//! landed (avatar generation is an image seam, out of scope; the announcement id is
//! returned to the executor).

use rusqlite::Connection;
use serde::Serialize;
use serde_json::Value;

use crate::db::doc_mount_documents::DocMountDocumentsRepository;
use crate::db::DbError;
use crate::wardrobe::{describe_wardrobe_effect, normalize_no_item_sentinel, WardrobeEffect};

use super::wardrobe_shared::{
    build_wardrobe_coverage_summary_from_state, empty_equipped_state, load_current_wardrobe_state,
    remove_from_slot, resolve_project_mount_point_ids_for_chat, resolve_wardrobe_item_across_tiers,
};
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
const SLOT_ENUM: [&str; 4] = ["top", "bottom", "footwear", "accessories"];

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

/// Execute `wardrobe_take_off` (v4 `executeWardrobeTakeOffTool`). Returns the
/// output plus the character ids to announce (non-empty when an op landed).
pub fn execute(
    main: &Connection,
    mount: &Connection,
    user_id: &str,
    chat_id: &str,
    character_id: &str,
    args: &Value,
) -> (WardrobeTakeOffToolOutput, Vec<String>) {
    let _ = user_id;
    let Some(ops) = validate(args) else {
        return (failure(VALIDATION_ERROR), Vec::new());
    };
    match run(main, mount, chat_id, character_id, &ops) {
        Ok(pair) => pair,
        Err(e) => (failure(e.to_string()), Vec::new()),
    }
}

struct TakeOffError(String);

fn run(
    main: &Connection,
    mount: &Connection,
    chat_id: &str,
    character_id: &str,
    ops: &[TakeOffOp],
) -> Result<(WardrobeTakeOffToolOutput, Vec<String>), DbError> {
    let docs = DocMountDocumentsRepository::new(mount);
    let project_mount_point_ids = resolve_project_mount_point_ids_for_chat(main, mount, chat_id);

    let mut results: Vec<WardrobeTakeOffOpResult> = Vec::new();
    let mut applied_count = 0usize;
    let mut failed_error: Option<String> = None;

    for op in ops {
        let mode = op.mode.clone().unwrap_or_else(|| "remove".to_string());
        let item_id = normalize_no_item_sentinel(op.item_id.as_deref());
        let item_title = normalize_no_item_sentinel(op.item_title.as_deref());

        let outcome = if mode == "clear_slot" {
            // op.slot is validated present for clear_slot.
            let slot = op.slot.clone().unwrap_or_default();
            remove_from_slot(main, chat_id, character_id, &slot, None)?;
            Ok(WardrobeTakeOffOpResult {
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
            })
        } else {
            apply_remove(
                main,
                &docs,
                chat_id,
                character_id,
                &mode,
                op.slot.as_deref(),
                item_id.as_deref(),
                item_title.as_deref(),
                &project_mount_point_ids,
            )?
        };

        match outcome {
            Ok(res) => {
                results.push(res);
                applied_count += 1;
            }
            Err(TakeOffError(message)) => {
                results.push(WardrobeTakeOffOpResult {
                    mode,
                    effect: "removed".to_string(),
                    effect_summary: String::new(),
                    item: None,
                    slots_affected: Vec::new(),
                    error: Some(message.clone()),
                });
                failed_error = Some(message);
                break;
            }
        }
    }

    let announce = if applied_count > 0 {
        // triggerAvatarGenerationIfEnabled — image subsystem seam (out of scope).
        vec![character_id.to_string()]
    } else {
        Vec::new()
    };

    let current_state = load_current_wardrobe_state(main, chat_id, character_id)?;
    let coverage_summary = build_wardrobe_coverage_summary_from_state(
        main,
        &docs,
        character_id,
        &current_state,
        &project_mount_point_ids,
    )?;

    Ok((
        WardrobeTakeOffToolOutput {
            success: failed_error.is_none(),
            operations: results,
            current_state: current_state.to_value(),
            coverage_summary,
            error: failed_error,
        },
        announce,
    ))
}

#[allow(clippy::too_many_arguments)]
fn apply_remove(
    main: &Connection,
    docs: &DocMountDocumentsRepository,
    chat_id: &str,
    character_id: &str,
    mode: &str,
    slot: Option<&str>,
    item_id: Option<&str>,
    item_title: Option<&str>,
    project_mount_point_ids: &[String],
) -> Result<Result<WardrobeTakeOffOpResult, TakeOffError>, DbError> {
    let item = resolve_wardrobe_item_across_tiers(
        main,
        docs,
        character_id,
        item_id,
        item_title,
        project_mount_point_ids,
    )?;
    let Some(item) = item else {
        return Ok(Err(TakeOffError(not_found_message(item_id, item_title))));
    };

    let title = item
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let id = item
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let types: Vec<String> = item
        .get("types")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|e| e.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();

    // Restrict to one slot if given, else every slot the item covers.
    let slots_affected: Vec<String> = match slot {
        Some(s) => vec![s.to_string()],
        None => types,
    };
    for s in &slots_affected {
        remove_from_slot(main, chat_id, character_id, s, Some(&id))?;
    }

    Ok(Ok(WardrobeTakeOffOpResult {
        mode: mode.to_string(),
        effect: "removed".to_string(),
        effect_summary: describe_wardrobe_effect(
            WardrobeEffect::Removed,
            &slots_affected,
            Some(title.as_str()),
        ),
        item: Some(OpItem { item_id: id, title }),
        slots_affected,
        error: None,
    }))
}

fn not_found_message(item_id: Option<&str>, item_title: Option<&str>) -> String {
    let mut msg = String::from("Wardrobe item not found");
    if let Some(id) = item_id {
        msg.push_str(&format!(" with ID \"{id}\""));
    }
    if let Some(t) = item_title {
        msg.push_str(&format!(" with title \"{t}\""));
    }
    msg
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
