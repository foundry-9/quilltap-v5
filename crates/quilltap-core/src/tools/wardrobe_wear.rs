//! `wardrobe_wear` handler (v4 `lib/tools/handlers/wardrobe-wear-handler.ts`
//! `executeWardrobeWearTool` + `formatWardrobeWearResults`).
//!
//! Applies an ordered array of put-on operations in sequence (each builds on the
//! last). Per-op mode maps to the equip primitives; fails fast on the first bad
//! op (item not found / archived / slot mismatch). Avatar generation + the pending
//! wardrobe announcement fire ONCE after the loop when at least one op landed
//! (avatar generation is an image seam, out of scope; the announcement id is
//! returned to the executor to fold into the per-turn set).

use rusqlite::Connection;
use serde::Serialize;
use serde_json::Value;

use crate::db::doc_mount_documents::DocMountDocumentsRepository;
use crate::db::DbError;
use crate::wardrobe::{describe_wardrobe_effect, normalize_no_item_sentinel, WardrobeEffect};

use super::wardrobe_shared::{
    add_to_slot, build_wardrobe_coverage_summary_from_state, empty_equipped_state, equip_item,
    load_current_wardrobe_state, replace_item, resolve_wardrobe_item_across_tiers,
};
use crate::wardrobe_tiers::{resolve_shared_wardrobe_tiers_for_chat, SharedWardrobeTiers};

/// The item acted on in an op result (`{ item_id, title } | null`).
#[derive(Debug, Serialize)]
pub struct OpItem {
    pub item_id: String,
    pub title: String,
}

/// v4 `WardrobeWearOpResult`.
#[derive(Debug, Serialize)]
pub struct WardrobeWearOpResult {
    pub mode: String,
    pub effect: String,
    pub effect_summary: String,
    pub item: Option<OpItem>,
    pub slots_affected: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// v4 `WardrobeWearToolOutput`.
#[derive(Debug, Serialize)]
pub struct WardrobeWearToolOutput {
    pub success: bool,
    pub operations: Vec<WardrobeWearOpResult>,
    pub current_state: Value,
    pub coverage_summary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

fn failure(error: impl Into<String>) -> WardrobeWearToolOutput {
    WardrobeWearToolOutput {
        success: false,
        operations: Vec::new(),
        current_state: empty_equipped_state().to_value(),
        coverage_summary: String::new(),
        error: Some(error.into()),
    }
}

const VALIDATION_ERROR: &str = "Invalid input: provide a non-empty \"operations\" array. Each operation needs an item_id or item_title; mode=add_to_slot also needs a slot.";
const MODE_ENUM: [&str; 3] = ["wear", "replace", "add_to_slot"];
const SLOT_ENUM: [&str; 4] = ["top", "bottom", "footwear", "accessories"];

/// One validated wear op.
struct WearOp {
    item_id: Option<String>,
    item_title: Option<String>,
    mode: Option<String>,
    slot: Option<String>,
}

/// v4 `validateWardrobeWearInput` — a non-empty `operations` array whose elements
/// are typed, with the superRefine: `add_to_slot` requires a slot; every op
/// requires `item_id` or `item_title`.
fn validate(args: &Value) -> Option<Vec<WearOp>> {
    let obj = args.as_object()?;
    let ops_val = obj.get("operations")?.as_array()?;
    if ops_val.is_empty() {
        return None; // `.min(1)`
    }
    let mut ops = Vec::with_capacity(ops_val.len());
    for op in ops_val {
        let o = op.as_object()?;
        let item_id = opt_str_field(o, "item_id")?;
        let item_title = opt_str_field(o, "item_title")?;
        let mode = opt_enum_field(o, "mode", &MODE_ENUM)?;
        let slot = opt_enum_field(o, "slot", &SLOT_ENUM)?;
        // superRefine.
        let effective_mode = mode.as_deref().unwrap_or("wear");
        if effective_mode == "add_to_slot" && slot.is_none() {
            return None;
        }
        if item_id.is_none() && item_title.is_none() {
            return None;
        }
        ops.push(WearOp {
            item_id,
            item_title,
            mode,
            slot,
        });
    }
    Some(ops)
}

/// `Ok(None)` = absent; `Ok(Some)` = a valid string; `None` (outer) = wrong type.
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

/// Execute `wardrobe_wear` (v4 `executeWardrobeWearTool`). Returns the output plus
/// the character ids to announce (non-empty when at least one op landed).
pub fn execute(
    main: &Connection,
    mount: &Connection,
    user_id: &str,
    chat_id: &str,
    character_id: &str,
    args: &Value,
) -> (WardrobeWearToolOutput, Vec<String>) {
    let _ = user_id;
    let Some(ops) = validate(args) else {
        return (failure(VALIDATION_ERROR), Vec::new());
    };
    match run(main, mount, chat_id, character_id, &ops) {
        Ok(pair) => pair,
        Err(e) => (failure(e.to_string()), Vec::new()),
    }
}

/// A per-op error (v4's `WardrobeWearError` thrown inside the loop).
struct WearError(String);

fn run(
    main: &Connection,
    mount: &Connection,
    chat_id: &str,
    character_id: &str,
    ops: &[WearOp],
) -> Result<(WardrobeWearToolOutput, Vec<String>), DbError> {
    let docs = DocMountDocumentsRepository::new(mount);
    let tiers = resolve_shared_wardrobe_tiers_for_chat(main, mount, chat_id, character_id);

    let mut results: Vec<WardrobeWearOpResult> = Vec::new();
    let mut applied_count = 0usize;
    let mut failed_error: Option<String> = None;

    for op in ops {
        let mode = op.mode.clone().unwrap_or_else(|| "wear".to_string());
        let item_id = normalize_no_item_sentinel(op.item_id.as_deref());
        let item_title = normalize_no_item_sentinel(op.item_title.as_deref());

        match apply_op(
            main,
            &docs,
            chat_id,
            character_id,
            &mode,
            op.slot.as_deref(),
            item_id.as_deref(),
            item_title.as_deref(),
            &tiers,
        )? {
            Ok(res) => {
                results.push(res);
                applied_count += 1;
            }
            Err(WearError(message)) => {
                results.push(WardrobeWearOpResult {
                    mode,
                    effect: "layered".to_string(),
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

    // Side effects fire ONCE, only if at least one op landed.
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
        &tiers,
    )?;

    Ok((
        WardrobeWearToolOutput {
            success: failed_error.is_none(),
            operations: results,
            current_state: current_state.to_value(),
            coverage_summary,
            error: failed_error,
        },
        announce,
    ))
}

/// Apply one op — the inner `Result` is the v4 per-op try/catch (`Err(WearError)`
/// is a per-op failure); the outer `Result` is a DB error (propagated).
#[allow(clippy::too_many_arguments)]
fn apply_op(
    main: &Connection,
    docs: &DocMountDocumentsRepository,
    chat_id: &str,
    character_id: &str,
    mode: &str,
    slot: Option<&str>,
    item_id: Option<&str>,
    item_title: Option<&str>,
    tiers: &SharedWardrobeTiers,
) -> Result<Result<WardrobeWearOpResult, WearError>, DbError> {
    let item =
        resolve_wardrobe_item_across_tiers(main, docs, character_id, item_id, item_title, tiers)?;
    let Some(item) = item else {
        return Ok(Err(WearError(not_found_message(item_id, item_title))));
    };
    if matches!(item.get("archivedAt"), Some(v) if !v.is_null()) {
        let title = item
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or_default();
        return Ok(Err(WearError(format!(
            "Item \"{title}\" is archived and cannot be worn"
        ))));
    }

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
    let types = types_of(&item);
    let component_item_ids = string_array(&item, "componentItemIds");
    let replace = item
        .get("replace")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let (effect, slots_affected) = match mode {
        "add_to_slot" => {
            let slot = slot.unwrap_or_default();
            if !types.iter().any(|t| t == slot) {
                return Ok(Err(WearError(format!(
                    "Item \"{title}\" (types: {}) cannot be added to the \"{slot}\" slot",
                    types.join(", ")
                ))));
            }
            add_to_slot(
                main,
                docs,
                chat_id,
                character_id,
                slot,
                &id,
                &types,
                &component_item_ids,
                tiers,
            )?;
            (WardrobeEffect::Layered, vec![slot.to_string()])
        }
        "replace" => {
            replace_item(
                main,
                docs,
                chat_id,
                character_id,
                &id,
                &types,
                &component_item_ids,
                tiers,
            )?;
            (WardrobeEffect::Replaced, types.clone())
        }
        _ => {
            // mode === 'wear'
            equip_item(
                main,
                docs,
                chat_id,
                character_id,
                &id,
                &types,
                &component_item_ids,
                replace,
                tiers,
            )?;
            let effect = if replace {
                WardrobeEffect::Replaced
            } else {
                WardrobeEffect::Layered
            };
            (effect, types.clone())
        }
    };

    Ok(Ok(WardrobeWearOpResult {
        mode: mode.to_string(),
        effect: effect.as_str().to_string(),
        effect_summary: describe_wardrobe_effect(effect, &slots_affected, Some(title.as_str())),
        item: Some(OpItem { item_id: id, title }),
        slots_affected,
        error: None,
    }))
}

/// The item's `componentItemIds` (absent reads as empty — v4's `?? []`).
fn string_array(item: &Value, key: &str) -> Vec<String> {
    item.get(key)
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|e| e.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn types_of(item: &Value) -> Vec<String> {
    item.get("types")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|e| e.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// v4 the per-op not-found message using the (normalized) op ids.
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

/// v4 `formatWardrobeWearResults`.
pub fn format(output: &WardrobeWearToolOutput) -> String {
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
