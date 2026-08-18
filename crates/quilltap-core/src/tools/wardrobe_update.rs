//! `wardrobe_update` handler (v4 `lib/tools/handlers/wardrobe-update-handler.ts`
//! `executeWardrobeUpdateTool` + `formatWardrobeUpdateResults`).
//!
//! Edits the stored fields of an existing OWN item (shared archetypes are
//! read-only). Only supplied fields change; when the component list changes and
//! `types` wasn't given, the coverage union is recomputed. Echoes back the updated
//! item in `wardrobe_read` shape. Does NOT equip.

use rusqlite::Connection;
use serde_json::Value;

use crate::db::doc_mount_documents::DocMountDocumentsRepository;
use crate::db::doc_mount_file_links::DocMountFileLinksRepository;
use crate::db::vault_wardrobe_public::{update_vault_wardrobe_item, WardrobePatch};
use crate::db::wardrobe_read::find_by_ids_for_character;
use crate::wardrobe::{normalize_no_item_sentinel, union_types};

use super::wardrobe_read::{
    build_read_failure, build_read_output, not_found_message, WardrobeReadToolOutput,
};
use super::wardrobe_shared::{
    is_own_wardrobe_item, public_error_message, resolve_wardrobe_item_across_tiers,
};
use crate::wardrobe_tiers::resolve_shared_wardrobe_tiers_for_chat;

/// The `wardrobe_update` mutation fields (v4 `WardrobeUpdateToolInput`). `Some` =
/// present. The locate fields (`item_id`/`item_title`) are kept alongside.
struct UpdateInput {
    item_id: Option<String>,
    item_title: Option<String>,
    title: Option<String>,
    description: Option<String>,
    image_prompt: Option<String>,
    appropriateness: Option<String>,
    types: Option<Vec<String>>,
    is_default: Option<bool>,
    replace: Option<bool>,
    component_item_ids: Option<Vec<String>>,
}

const SLOT_ENUM: [&str; 5] = crate::wardrobe::WARDROBE_SLOT_TYPES;

fn opt_string(obj: &serde_json::Map<String, Value>, key: &str) -> Result<Option<String>, ()> {
    match obj.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.clone())),
        Some(_) => Err(()),
    }
}

fn opt_bool(obj: &serde_json::Map<String, Value>, key: &str) -> Result<Option<bool>, ()> {
    match obj.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Bool(b)) => Ok(Some(*b)),
        Some(_) => Err(()),
    }
}

/// v4 `validateWardrobeUpdateInput` (`wardrobeUpdateToolInputSchema.safeParse`):
/// typed optional fields (`title` min-1, `types` a non-empty slot-enum array,
/// `component_item_ids` a string array) refined so `item_id`/`item_title` — at
/// least one — is present. `None` ⇒ validation failure.
fn validate(args: &Value) -> Option<UpdateInput> {
    let obj = args.as_object()?;
    let item_id = opt_string(obj, "item_id").ok()?;
    let item_title = opt_string(obj, "item_title").ok()?;
    if item_id.is_none() && item_title.is_none() {
        return None;
    }
    // title: string min length 1.
    let title = opt_string(obj, "title").ok()?;
    if let Some(t) = &title {
        if t.is_empty() {
            return None;
        }
    }
    let description = opt_string(obj, "description").ok()?;
    let image_prompt = opt_string(obj, "image_prompt").ok()?;
    let appropriateness = opt_string(obj, "appropriateness").ok()?;
    // types: non-empty array of the slot enum.
    let types = match obj.get("types") {
        None | Some(Value::Null) => None,
        Some(Value::Array(a)) => {
            if a.is_empty() {
                return None; // `.nonempty()`
            }
            let mut v = Vec::with_capacity(a.len());
            for e in a {
                let s = e.as_str()?;
                if !SLOT_ENUM.contains(&s) {
                    return None; // z.enum
                }
                v.push(s.to_string());
            }
            Some(v)
        }
        Some(_) => return None,
    };
    let is_default = opt_bool(obj, "is_default").ok()?;
    let replace = opt_bool(obj, "replace").ok()?;
    // component_item_ids: array of string.
    let component_item_ids = match obj.get("component_item_ids") {
        None | Some(Value::Null) => None,
        Some(Value::Array(a)) => {
            let mut v = Vec::with_capacity(a.len());
            for e in a {
                v.push(e.as_str()?.to_string());
            }
            Some(v)
        }
        Some(_) => return None,
    };
    Some(UpdateInput {
        item_id,
        item_title,
        title,
        description,
        image_prompt,
        appropriateness,
        types,
        is_default,
        replace,
        component_item_ids,
    })
}

/// Execute `wardrobe_update` (v4 `executeWardrobeUpdateTool`).
pub fn execute(
    main: &Connection,
    mount: &Connection,
    user_id: &str,
    chat_id: &str,
    character_id: &str,
    args: &Value,
) -> WardrobeReadToolOutput {
    let _ = user_id;
    let Some(input) = validate(args) else {
        return build_read_failure("Invalid input: item_id or item_title is required.");
    };
    match run(main, mount, chat_id, character_id, &input) {
        Ok(out) => out,
        Err(e) => build_read_failure(e.to_string()),
    }
}

fn run(
    main: &Connection,
    mount: &Connection,
    chat_id: &str,
    character_id: &str,
    input: &UpdateInput,
) -> Result<WardrobeReadToolOutput, crate::db::DbError> {
    let docs = DocMountDocumentsRepository::new(mount);
    let links = DocMountFileLinksRepository::new(mount);
    let tiers = resolve_shared_wardrobe_tiers_for_chat(main, mount, chat_id, character_id);

    let item = resolve_wardrobe_item_across_tiers(
        main,
        &docs,
        character_id,
        normalize_no_item_sentinel(input.item_id.as_deref()).as_deref(),
        normalize_no_item_sentinel(input.item_title.as_deref()).as_deref(),
        &tiers,
    )?;
    let Some(item) = item else {
        return Ok(build_read_failure(not_found_message(
            &input.item_id,
            &input.item_title,
        )));
    };

    let title = item
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();

    if !is_own_wardrobe_item(&item, character_id) {
        return Ok(build_read_failure(format!(
            "\"{title}\" is a shared wardrobe item — you can wear it but not edit or retire it. \
Only items in your own wardrobe can be changed."
        )));
    }

    let id = item
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();

    // Build the patch (only supplied fields).
    let mut patch = WardrobePatch {
        title: input.title.clone(),
        description: input.description.clone().map(Some),
        image_prompt: input.image_prompt.clone().map(Some),
        appropriateness: input.appropriateness.clone().map(Some),
        types: input.types.clone(),
        is_default: input.is_default,
        replace: input.replace,
        component_item_ids: input.component_item_ids.clone(),
        archived_at: None,
    };

    // When components changed and types wasn't supplied, recompute the coverage
    // union from the new components (across tiers).
    if let Some(comp_ids) = &input.component_item_ids {
        if input.types.is_none() && !comp_ids.is_empty() {
            let comps = find_by_ids_for_character(main, &docs, character_id, comp_ids, &tiers)?;
            let comp_types: Vec<Vec<String>> = comps
                .iter()
                .map(|c| {
                    c.get("types")
                        .and_then(Value::as_array)
                        .map(|a| {
                            a.iter()
                                .filter_map(|e| e.as_str().map(str::to_string))
                                .collect()
                        })
                        .unwrap_or_default()
                })
                .collect();
            let union = union_types(comp_types.iter().map(Vec::as_slice));
            if !union.is_empty() {
                patch.types = Some(union);
            }
        }
    }

    let updated =
        match update_vault_wardrobe_item(main, &links, &docs, &id, &patch, Some(character_id)) {
            Ok(u) => u,
            Err(e) => return Ok(build_read_failure(public_error_message(e, "update"))),
        };
    let Some(updated) = updated else {
        return Ok(build_read_failure(format!(
            "Failed to update wardrobe item \"{title}\""
        )));
    };

    let updated_value = serde_json::to_value(&updated).unwrap_or(Value::Null);
    build_read_output(main, &docs, character_id, chat_id, &updated_value, &tiers)
}

/// v4 `formatWardrobeUpdateResults`.
pub fn format(output: &WardrobeReadToolOutput) -> String {
    if !output.success {
        return format!(
            "Wardrobe Error: {}",
            output.error.as_deref().unwrap_or("Unknown error")
        );
    }
    format!("Updated \"{}\" ({}).", output.title, output.item_id)
}
