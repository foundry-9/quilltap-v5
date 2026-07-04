//! `wardrobe_read` handler (v4 `lib/tools/handlers/wardrobe-read-handler.ts`
//! `executeWardrobeReadTool` + `formatWardrobeReadResults` + the shared
//! `buildWardrobeReadOutput` / `buildWardrobeReadFailure`, reused by
//! `wardrobe_update`).
//!
//! Resolves ONE wardrobe item across the (non-group) tiers and returns its full
//! detail — the Portrait Cue, default/replace flags, component list, archived
//! status, ownership, and equipped slots.

use rusqlite::Connection;
use serde::Serialize;
use serde_json::Value;

use crate::db::chats_outfits::ChatOutfitsRepository;
use crate::db::doc_mount_documents::DocMountDocumentsRepository;
use crate::db::wardrobe_read::find_by_ids_for_character;
use crate::db::DbError;
use crate::wardrobe::normalize_no_item_sentinel;

use super::wardrobe_shared::{
    find_equipped_slots, is_own_wardrobe_item, resolve_project_mount_point_ids_for_chat,
    resolve_wardrobe_item_across_tiers,
};

/// v4 `WardrobeReadToolOutput` — also the `wardrobe_update` output (a read-shaped
/// echo). `error` is omitted on success.
#[derive(Debug, Serialize)]
pub struct WardrobeReadToolOutput {
    pub success: bool,
    pub item_id: String,
    pub title: String,
    pub description: Option<String>,
    pub image_prompt: Option<String>,
    pub types: Vec<String>,
    pub appropriateness: Option<String>,
    pub is_default: bool,
    pub replace: bool,
    pub is_composite: bool,
    pub component_item_ids: Vec<String>,
    pub component_titles: Vec<String>,
    pub archived: bool,
    pub is_own: bool,
    pub is_equipped: bool,
    pub equipped_slots: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

fn str_field(item: &Value, key: &str) -> Option<String> {
    item.get(key).and_then(Value::as_str).map(str::to_string)
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

fn component_ids_of(item: &Value) -> Vec<String> {
    item.get("componentItemIds")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|e| e.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// v4 `buildWardrobeReadFailure` — the all-empty read output carrying an error.
pub fn build_read_failure(error: impl Into<String>) -> WardrobeReadToolOutput {
    WardrobeReadToolOutput {
        success: false,
        item_id: String::new(),
        title: String::new(),
        description: None,
        image_prompt: None,
        types: Vec::new(),
        appropriateness: None,
        is_default: false,
        replace: false,
        is_composite: false,
        component_item_ids: Vec::new(),
        component_titles: Vec::new(),
        archived: false,
        is_own: false,
        is_equipped: false,
        equipped_slots: Vec::new(),
        error: Some(error.into()),
    }
}

/// v4 `buildWardrobeReadOutput` — the full read-shaped output for a resolved item.
/// Shared by `wardrobe_read` and `wardrobe_update`.
pub fn build_read_output(
    main: &Connection,
    docs: &DocMountDocumentsRepository,
    character_id: &str,
    chat_id: &str,
    item: &Value,
    project_mount_point_ids: &[String],
) -> Result<WardrobeReadToolOutput, DbError> {
    let component_item_ids = component_ids_of(item);
    let is_composite = !component_item_ids.is_empty();

    let component_titles: Vec<String> = if is_composite {
        let components = find_by_ids_for_character(
            main,
            docs,
            character_id,
            &component_item_ids,
            project_mount_point_ids,
        )?;
        let title_by_id: std::collections::HashMap<String, String> = components
            .iter()
            .filter_map(|c| {
                let id = c.get("id").and_then(Value::as_str)?;
                let title = c.get("title").and_then(Value::as_str)?;
                Some((id.to_string(), title.to_string()))
            })
            .collect();
        component_item_ids
            .iter()
            .filter_map(|cid| title_by_id.get(cid).cloned())
            .collect()
    } else {
        Vec::new()
    };

    let equipped_slots_val = ChatOutfitsRepository::new(main)
        .get_equipped_outfit_for_character(chat_id, character_id)?;
    let id = str_field(item, "id").unwrap_or_default();
    let equipped = find_equipped_slots(&id, equipped_slots_val.as_ref());

    Ok(WardrobeReadToolOutput {
        success: true,
        item_id: id,
        title: str_field(item, "title").unwrap_or_default(),
        description: str_field(item, "description"),
        image_prompt: str_field(item, "imagePrompt"),
        types: types_of(item),
        appropriateness: str_field(item, "appropriateness"),
        is_default: item
            .get("isDefault")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        replace: item
            .get("replace")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        is_composite,
        component_item_ids,
        component_titles,
        archived: matches!(item.get("archivedAt"), Some(v) if !v.is_null()),
        is_own: is_own_wardrobe_item(item, character_id),
        is_equipped: !equipped.is_empty(),
        equipped_slots: equipped,
        error: None,
    })
}

/// Validated `wardrobe_read` / `wardrobe_update`-style locate input (`item_id` /
/// `item_title`, at least one required). `None` ⇒ validation failure.
pub(crate) struct LocateInput {
    pub item_id: Option<String>,
    pub item_title: Option<String>,
}

/// v4 `wardrobeReadToolInputSchema` (also the locate half of update/archive): an
/// object with optional string `item_id`/`item_title`, refined so at least one is
/// present.
pub(crate) fn validate_locate(args: &Value) -> Option<LocateInput> {
    let obj = args.as_object()?;
    let item_id = match obj.get("item_id") {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) => Some(s.clone()),
        Some(_) => return None,
    };
    let item_title = match obj.get("item_title") {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) => Some(s.clone()),
        Some(_) => return None,
    };
    if item_id.is_none() && item_title.is_none() {
        return None; // the `.refine()` — at least one required.
    }
    Some(LocateInput {
        item_id,
        item_title,
    })
}

/// v4 the not-found message: `Wardrobe item not found[ with ID "<id>"][ with title
/// "<title>"]` — using the RAW input values (not the sentinel-normalized ones).
pub(crate) fn not_found_message(item_id: &Option<String>, item_title: &Option<String>) -> String {
    let mut msg = String::from("Wardrobe item not found");
    if let Some(id) = item_id {
        msg.push_str(&format!(" with ID \"{id}\""));
    }
    if let Some(title) = item_title {
        msg.push_str(&format!(" with title \"{title}\""));
    }
    msg
}

/// Execute `wardrobe_read` (v4 `executeWardrobeReadTool`).
pub fn execute(
    main: &Connection,
    mount: &Connection,
    user_id: &str,
    chat_id: &str,
    character_id: &str,
    args: &Value,
) -> WardrobeReadToolOutput {
    let _ = user_id;
    let Some(input) = validate_locate(args) else {
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
    input: &LocateInput,
) -> Result<WardrobeReadToolOutput, DbError> {
    let docs = DocMountDocumentsRepository::new(mount);
    let project_mount_point_ids = resolve_project_mount_point_ids_for_chat(main, mount, chat_id);
    let item = resolve_wardrobe_item_across_tiers(
        main,
        &docs,
        character_id,
        normalize_no_item_sentinel(input.item_id.as_deref()).as_deref(),
        normalize_no_item_sentinel(input.item_title.as_deref()).as_deref(),
        &project_mount_point_ids,
    )?;
    let Some(item) = item else {
        return Ok(build_read_failure(not_found_message(
            &input.item_id,
            &input.item_title,
        )));
    };
    build_read_output(
        main,
        &docs,
        character_id,
        chat_id,
        &item,
        &project_mount_point_ids,
    )
}

/// v4 `formatWardrobeReadResults`.
pub fn format(output: &WardrobeReadToolOutput) -> String {
    if !output.success {
        return format!(
            "Wardrobe Error: {}",
            output.error.as_deref().unwrap_or("Unknown error")
        );
    }
    let mut lines: Vec<String> = vec![format!("{} ({})", output.title, output.item_id)];
    lines.push(format!("  types: {}", output.types.join(", ")));
    if let Some(a) = &output.appropriateness {
        lines.push(format!("  appropriateness: {a}"));
    }
    if let Some(d) = &output.description {
        lines.push(format!("  description: {d}"));
    }
    lines.push(format!(
        "  portrait cue: {}",
        output
            .image_prompt
            .as_deref()
            .unwrap_or("(none — falls back to title)")
    ));
    if output.is_composite {
        let titles = output.component_titles.join(", ");
        let inner = if titles.is_empty() {
            "unresolved components".to_string()
        } else {
            titles
        };
        lines.push(format!(
            "  composite: {} (replace={})",
            inner, output.replace
        ));
    }
    lines.push(format!(
        "  default: {} | own: {}",
        if output.is_default { "yes" } else { "no" },
        if output.is_own {
            "yes"
        } else {
            "no (shared — read-only)"
        }
    ));
    if output.archived {
        lines.push("  archived: yes (hidden from listings, cannot be worn)".to_string());
    }
    lines.push(format!(
        "  equipped: {}",
        if output.is_equipped {
            output.equipped_slots.join(", ")
        } else {
            "no".to_string()
        }
    ));
    lines.join("\n")
}
