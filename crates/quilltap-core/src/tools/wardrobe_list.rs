//! `wardrobe_list` handler (v4 `lib/tools/handlers/wardrobe-list-handler.ts`
//! `executeWardrobeListTool` + `formatWardrobeListResults`).
//!
//! Merges the character's own wardrobe with shared archetypes (project + Quilltap
//! General; character items win on id collision), drops archived items, filters by
//! type/appropriateness, and marks each item's equipped slots + composite metadata.

use rusqlite::Connection;
use serde::Serialize;
use serde_json::Value;

use crate::db::chats_outfits::ChatOutfitsRepository;
use crate::db::doc_mount_documents::DocMountDocumentsRepository;
use crate::db::wardrobe_read::find_wearable_pool_for_character;
use crate::db::DbError;

use super::wardrobe_shared::{find_equipped_slots, resolve_project_mount_point_ids_for_chat};

/// One item in the list result (v4 `WardrobeListItemResult`). The composite fields
/// are omitted for leaf items (v4's conditional spread).
#[derive(Debug, Serialize)]
pub struct WardrobeListItemResult {
    pub item_id: String,
    pub title: String,
    pub description: Option<String>,
    pub image_prompt: Option<String>,
    pub types: Vec<String>,
    pub appropriateness: Option<String>,
    pub is_own: bool,
    pub is_equipped: bool,
    pub equipped_slot: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_composite: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub component_item_ids: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub component_titles: Option<Vec<String>>,
}

/// v4 `WardrobeListToolOutput` (presets omitted — the handler never populates it).
#[derive(Debug, Serialize)]
pub struct WardrobeListToolOutput {
    pub success: bool,
    pub items: Vec<WardrobeListItemResult>,
    pub total_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Validated `wardrobe_list` input (v4 `WardrobeListToolInput`).
struct ListInput {
    type_filter: Option<Vec<String>>,
    appropriateness_filter: Option<String>,
    include_equipped: Option<bool>,
}

const VALIDATION_ERROR: &str = "Invalid input: type_filter must be a string array, appropriateness_filter must be a string, include_equipped must be a boolean";

/// v4 `validateWardrobeListInput` (`wardrobeListToolInputSchema.safeParse`): an
/// object whose optional fields, if present, have the right types. Unknown keys are
/// stripped (Zod non-strict). `None` ⇒ validation failure.
fn validate(args: &Value) -> Option<ListInput> {
    let obj = args.as_object()?;
    let type_filter = match obj.get("type_filter") {
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
    let appropriateness_filter = match obj.get("appropriateness_filter") {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) => Some(s.clone()),
        Some(_) => return None,
    };
    let include_equipped = match obj.get("include_equipped") {
        None | Some(Value::Null) => None,
        Some(Value::Bool(b)) => Some(*b),
        Some(_) => return None,
    };
    // include_presets is accepted (and ignored); a wrong type still fails the parse.
    match obj.get("include_presets") {
        None | Some(Value::Null) | Some(Value::Bool(_)) => {}
        Some(_) => return None,
    }
    Some(ListInput {
        type_filter,
        appropriateness_filter,
        include_equipped,
    })
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

/// Execute `wardrobe_list` (v4 `executeWardrobeListTool`).
pub fn execute(
    main: &Connection,
    mount: &Connection,
    user_id: &str,
    chat_id: &str,
    character_id: &str,
    args: &Value,
) -> WardrobeListToolOutput {
    let _ = user_id; // logging only
    let Some(input) = validate(args) else {
        return WardrobeListToolOutput {
            success: false,
            items: Vec::new(),
            total_count: 0,
            error: Some(VALIDATION_ERROR.to_string()),
        };
    };
    match run(main, mount, chat_id, character_id, &input) {
        Ok(out) => out,
        Err(e) => WardrobeListToolOutput {
            success: false,
            items: Vec::new(),
            total_count: 0,
            error: Some(e.to_string()),
        },
    }
}

fn run(
    main: &Connection,
    mount: &Connection,
    chat_id: &str,
    character_id: &str,
    input: &ListInput,
) -> Result<WardrobeListToolOutput, DbError> {
    let docs = DocMountDocumentsRepository::new(mount);
    // The character's own wardrobe merged under the shared archetypes (project +
    // Quilltap General). Character items win on id collision so a personal
    // override masks the shared item. (The group tier is a tracked follow-up —
    // the repo doesn't accept group mounts yet.)
    let project_mount_point_ids = resolve_project_mount_point_ids_for_chat(main, mount, chat_id);
    let all_items =
        find_wearable_pool_for_character(main, &docs, character_id, &project_mount_point_ids)?;

    let equipped = ChatOutfitsRepository::new(main)
        .get_equipped_outfit_for_character(chat_id, character_id)?;

    // Type filter.
    let type_lower: Option<Vec<String>> = input
        .type_filter
        .as_ref()
        .filter(|t| !t.is_empty())
        .map(|t| t.iter().map(|s| s.to_lowercase()).collect());
    // Appropriateness filter (trimmed non-empty).
    let appr_lower: Option<String> = input
        .appropriateness_filter
        .as_ref()
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.to_lowercase());

    let mut filtered: Vec<&Value> = all_items.iter().collect();
    if let Some(tl) = &type_lower {
        filtered.retain(|item| {
            types_of(item)
                .iter()
                .any(|t| tl.contains(&t.to_lowercase()))
        });
    }
    if let Some(al) = &appr_lower {
        filtered.retain(|item| {
            item.get("appropriateness")
                .and_then(Value::as_str)
                .is_some_and(|a| a.to_lowercase().contains(al.as_str()))
        });
    }

    // Title map for composite component resolution (unfiltered set).
    let titles_by_id: std::collections::HashMap<String, String> = all_items
        .iter()
        .filter_map(|it| {
            let id = it.get("id").and_then(Value::as_str)?;
            let title = it.get("title").and_then(Value::as_str)?;
            Some((id.to_string(), title.to_string()))
        })
        .collect();

    let result_items: Vec<WardrobeListItemResult> = filtered
        .into_iter()
        .map(|item| {
            let id = str_field(item, "id").unwrap_or_default();
            let equipped_slots = find_equipped_slots(&id, equipped.as_ref());
            let component_item_ids = component_ids_of(item);
            let is_composite = !component_item_ids.is_empty();
            let component_titles: Option<Vec<String>> = if is_composite {
                Some(
                    component_item_ids
                        .iter()
                        .filter_map(|cid| titles_by_id.get(cid).cloned())
                        .collect(),
                )
            } else {
                None
            };
            WardrobeListItemResult {
                item_id: id.clone(),
                title: str_field(item, "title").unwrap_or_default(),
                description: str_field(item, "description"),
                image_prompt: str_field(item, "imagePrompt"),
                types: types_of(item),
                appropriateness: str_field(item, "appropriateness"),
                is_own: item.get("characterId").and_then(Value::as_str) == Some(character_id),
                is_equipped: !equipped_slots.is_empty(),
                equipped_slot: equipped_slots.first().cloned(),
                is_composite: if is_composite { Some(true) } else { None },
                component_item_ids: if is_composite {
                    Some(component_item_ids)
                } else {
                    None
                },
                component_titles,
            }
        })
        .collect();

    // include_equipped === false drops equipped items.
    let final_items: Vec<WardrobeListItemResult> = if input.include_equipped == Some(false) {
        result_items
            .into_iter()
            .filter(|i| !i.is_equipped)
            .collect()
    } else {
        result_items
    };

    let total_count = final_items.len();
    Ok(WardrobeListToolOutput {
        success: true,
        items: final_items,
        total_count,
        error: None,
    })
}

/// v4 `formatWardrobeListResults`.
pub fn format(output: &WardrobeListToolOutput) -> String {
    if !output.success {
        return format!(
            "Wardrobe Error: {}",
            output.error.as_deref().unwrap_or("Unknown error")
        );
    }
    if output.items.is_empty() {
        return "Wardrobe: No items found matching the specified filters.".to_string();
    }
    let mut lines: Vec<String> = vec![format!(
        "Wardrobe ({} item{}):",
        output.total_count,
        if output.total_count != 1 { "s" } else { "" }
    )];
    for item in &output.items {
        let type_tags = item
            .types
            .iter()
            .map(|t| format!("[{t}]"))
            .collect::<Vec<_>>()
            .join(" ");
        let equipped_tag = if item.is_equipped {
            format!(
                " (EQUIPPED in {})",
                item.equipped_slot.as_deref().unwrap_or("null")
            )
        } else {
            String::new()
        };
        let shared_tag = if item.is_own {
            ""
        } else {
            " [shared — read-only]"
        };
        let appropriateness_tag = match &item.appropriateness {
            Some(a) => format!(" | {a}"),
            None => String::new(),
        };
        let description = match &item.description {
            Some(d) => format!(" - {d}"),
            None => String::new(),
        };
        let cue_tag = match &item.image_prompt {
            Some(c) => format!(" (cue: {c})"),
            None => String::new(),
        };
        let composite_tag = if item.is_composite == Some(true) {
            let titles = item.component_titles.clone().unwrap_or_default().join(", ");
            let inner = if titles.is_empty() {
                "unresolved components".to_string()
            } else {
                titles
            };
            format!(" [composite: {inner}]")
        } else {
            String::new()
        };
        lines.push(format!(
            "  {} {}{}{}{}{}{}{}",
            type_tags,
            item.title,
            equipped_tag,
            shared_tag,
            appropriateness_tag,
            composite_tag,
            cue_tag,
            description
        ));
    }
    lines.join("\n")
}
