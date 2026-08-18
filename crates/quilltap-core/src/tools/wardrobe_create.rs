//! `wardrobe_create` handler (v4 `lib/tools/handlers/wardrobe-create-handler.ts`
//! `executeWardrobeCreateTool` + `formatWardrobeCreateResults`).
//!
//! Creates a leaf or composite wardrobe item and optionally equips it. Supports
//! gifting to another chat participant (`recipient`). Composite `types` are the
//! union of the components' slots widened by any supplied `types`. Cycles are
//! rejected by the public create path. Avatar generation on equip is an image
//! subsystem seam (out of scope): the corpus keeps `avatarGenerationEnabled`
//! false so v4's `triggerAvatarGenerationIfEnabled` is a no-op, matching the port
//! (which omits it).

use rusqlite::Connection;
use serde::Serialize;
use serde_json::Value;

use crate::clock;
use crate::db::archetype_wardrobe::find_archetypes;
use crate::db::characters_read;
use crate::db::chats_read;
use crate::db::doc_mount_documents::DocMountDocumentsRepository;
use crate::db::doc_mount_file_links::DocMountFileLinksRepository;
use crate::db::vault_wardrobe_public::create_vault_wardrobe_item;
use crate::db::wardrobe_read::find_by_character_id;
use crate::vault_overlay::WardrobeItem;
use crate::wardrobe::{
    describe_wardrobe_effect, union_types, Slots, WardrobeEffect, WARDROBE_SLOT_TYPES,
};

use super::wardrobe_shared::public_error_message;
use crate::wardrobe_tiers::{resolve_shared_wardrobe_tiers_for_chat, SharedWardrobeTiers};

/// v4 `WardrobeCreateToolOutput` (fields conditionally present per v4's object
/// spread; `error` only on failure).
#[derive(Debug, Serialize)]
pub struct WardrobeCreateToolOutput {
    pub success: bool,
    pub item_id: String,
    pub title: String,
    pub equipped: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effect: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effect_summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_composite: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved_types: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved_component_item_ids: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recipient_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_state: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

fn failure(error: impl Into<String>) -> WardrobeCreateToolOutput {
    WardrobeCreateToolOutput {
        success: false,
        item_id: String::new(),
        title: String::new(),
        equipped: false,
        effect: None,
        effect_summary: None,
        is_composite: None,
        resolved_types: None,
        resolved_component_item_ids: None,
        recipient_name: None,
        current_state: None,
        error: Some(error.into()),
    }
}

const VALIDATION_ERROR: &str = "Invalid input: title (string) is required. Either supply types (non-empty array of valid slot types) for a leaf item, or component_item_ids / component_titles for a composite item.";

const SLOT_ENUM: [&str; 5] = crate::wardrobe::WARDROBE_SLOT_TYPES;

/// Validated `wardrobe_create` input (v4 `WardrobeCreateToolInput`).
struct CreateInput {
    title: String,
    description: Option<String>,
    image_prompt: Option<String>,
    types: Option<Vec<String>>,
    appropriateness: Option<String>,
    equip_now: bool,
    recipient: Option<String>,
    component_item_ids: Option<Vec<String>>,
    component_titles: Option<Vec<String>>,
    replace: Option<bool>,
}

fn opt_string(obj: &serde_json::Map<String, Value>, key: &str) -> Result<Option<String>, ()> {
    match obj.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.clone())),
        Some(_) => Err(()),
    }
}

/// An array of non-empty (trim>0) strings (v4 `z.array(z.string().refine(trim>0))`).
fn opt_nonempty_string_array(
    obj: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<Option<Vec<String>>, ()> {
    match obj.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Array(a)) => {
            let mut v = Vec::with_capacity(a.len());
            for e in a {
                let s = e.as_str().ok_or(())?;
                if s.trim().is_empty() {
                    return Err(());
                }
                v.push(s.to_string());
            }
            Ok(Some(v))
        }
        Some(_) => Err(()),
    }
}

/// v4 `validateWardrobeCreateInput`. `title` is required (min-1, trim>0); `types`
/// (if present) a non-empty slot-enum array; `recipient`/component entries trim>0;
/// `equip_now` defaults to false. The top-level refine requires `types` OR
/// non-empty components.
fn validate(args: &Value) -> Option<CreateInput> {
    let obj = args.as_object()?;
    // title: required string, min1, trim>0.
    let title = match obj.get("title") {
        Some(Value::String(s)) if !s.is_empty() && !s.trim().is_empty() => s.clone(),
        _ => return None,
    };
    let description = opt_string(obj, "description").ok()?;
    let image_prompt = opt_string(obj, "image_prompt").ok()?;
    let types = match obj.get("types") {
        None | Some(Value::Null) => None,
        Some(Value::Array(a)) => {
            if a.is_empty() {
                return None;
            }
            let mut v = Vec::with_capacity(a.len());
            for e in a {
                let s = e.as_str()?;
                if !SLOT_ENUM.contains(&s) {
                    return None;
                }
                v.push(s.to_string());
            }
            Some(v)
        }
        Some(_) => return None,
    };
    let appropriateness = opt_string(obj, "appropriateness").ok()?;
    let equip_now = match obj.get("equip_now") {
        None | Some(Value::Null) => false, // .default(false)
        Some(Value::Bool(b)) => *b,
        Some(_) => return None,
    };
    // recipient: string, trim>0 (when present).
    let recipient = match obj.get("recipient") {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) => {
            if s.trim().is_empty() {
                return None;
            }
            Some(s.clone())
        }
        Some(_) => return None,
    };
    let component_item_ids = opt_nonempty_string_array(obj, "component_item_ids").ok()?;
    let component_titles = opt_nonempty_string_array(obj, "component_titles").ok()?;
    let replace = match obj.get("replace") {
        None | Some(Value::Null) => None,
        Some(Value::Bool(b)) => Some(*b),
        Some(_) => return None,
    };
    // Top-level refine: types OR non-empty components.
    let has_components = component_item_ids.as_ref().is_some_and(|v| !v.is_empty())
        || component_titles.as_ref().is_some_and(|v| !v.is_empty());
    if types.is_none() && !has_components {
        return None;
    }
    Some(CreateInput {
        title,
        description,
        image_prompt,
        types,
        appropriateness,
        equip_now,
        recipient,
        component_item_ids,
        component_titles,
        replace,
    })
}

/// Execute `wardrobe_create` (v4 `executeWardrobeCreateTool`).
pub fn execute(
    main: &Connection,
    mount: &Connection,
    user_id: &str,
    chat_id: &str,
    character_id: &str,
    args: &Value,
) -> WardrobeCreateToolOutput {
    let _ = user_id;
    let Some(input) = validate(args) else {
        return failure(VALIDATION_ERROR);
    };
    match run(main, mount, chat_id, character_id, &input) {
        Ok(out) => out,
        Err(CreateError::Message(m)) => failure(m),
        Err(CreateError::Db(e)) => failure(e.to_string()),
    }
}

enum CreateError {
    /// A `WardrobeCreateError` / thrown-message failure (v4 → `error.message`).
    Message(String),
    Db(crate::db::DbError),
}

impl From<crate::db::DbError> for CreateError {
    fn from(e: crate::db::DbError) -> Self {
        CreateError::Db(e)
    }
}

fn run(
    main: &Connection,
    mount: &Connection,
    chat_id: &str,
    character_id: &str,
    input: &CreateInput,
) -> Result<WardrobeCreateToolOutput, CreateError> {
    let docs = DocMountDocumentsRepository::new(mount);
    let links = DocMountFileLinksRepository::new(mount);

    // Resolve the target character (default the caller; else a chat participant).
    let mut target_character_id = character_id.to_string();
    let mut recipient_name: Option<String> = None;
    if let Some(recipient) = &input.recipient {
        match resolve_recipient_from_chat(main, mount, chat_id, recipient)? {
            Some((cid, name)) => {
                target_character_id = cid;
                recipient_name = Some(name);
            }
            None => {
                return Ok(failure(format!(
                    "Could not find a character named \"{recipient}\" in this chat"
                )));
            }
        }
    }

    // Keyed on the *target* character, not the caller: a gift is assembled from
    // what the recipient can reach, and the group tier is per-character
    // (`8600c83f` — one of the two real fixes riding that commit).
    let tiers = resolve_shared_wardrobe_tiers_for_chat(main, mount, chat_id, &target_character_id);

    // Resolve components against the target character's wardrobe + shared archetypes.
    let components = resolve_component_items(
        main,
        &docs,
        &target_character_id,
        input.component_item_ids.as_deref(),
        input.component_titles.as_deref(),
        &tiers,
    )?;

    let is_composite = !components.is_empty();
    let resolved_types: Vec<String> = if is_composite {
        let comp_types: Vec<Vec<String>> = components.iter().map(types_of).collect();
        let union = union_types(comp_types.iter().map(Vec::as_slice));
        if union.is_empty() {
            return Err(CreateError::Message(
                "Composite components do not cover any slots — this should not happen".to_string(),
            ));
        }
        // designated = union ∪ supplied types, then canonical slot order.
        let mut designated: std::collections::HashSet<String> = union.into_iter().collect();
        if let Some(t) = &input.types {
            for s in t {
                designated.insert(s.clone());
            }
        }
        WARDROBE_SLOT_TYPES
            .iter()
            .filter(|s| designated.contains(**s))
            .map(|s| s.to_string())
            .collect()
    } else {
        input.types.clone().unwrap_or_default()
    };

    let component_item_ids: Vec<String> = components
        .iter()
        .map(|c| {
            c.get("id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        })
        .collect();

    // Mint + build the item (v4 `repos.wardrobe.create` materializes id/timestamps).
    let now = clock::now_iso();
    let new_item = WardrobeItem {
        id: uuid::Uuid::new_v4().to_string(),
        character_id: Some(Some(target_character_id.clone())),
        title: input.title.clone(),
        description: Some(input.description.clone().filter(|s| !s.is_empty())),
        image_prompt: Some(input.image_prompt.clone().filter(|s| !s.is_empty())),
        types: resolved_types.clone(),
        component_item_ids: component_item_ids.clone(),
        appropriateness: Some(input.appropriateness.clone().filter(|s| !s.is_empty())),
        is_default: false,
        replace: if is_composite {
            input.replace.unwrap_or(false)
        } else {
            false
        },
        migrated_from_clothing_record_id: None,
        archived_at: None,
        created_at: now.clone(),
        updated_at: now,
    };

    let created = create_vault_wardrobe_item(main, &links, &docs, &new_item)
        .map_err(|e| CreateError::Message(public_error_message(e, "create")))?;

    let mut equipped = false;
    let mut effect: Option<WardrobeEffect> = None;
    let mut current_state: Option<Value> = None;

    if input.equip_now {
        // v4 `8600c83f` fix #2: the equip-now path passed NO tiers at all, so a
        // new bundle's shared components did not resolve.
        super::wardrobe_shared::equip_item(
            main,
            &docs,
            chat_id,
            &target_character_id,
            &created.id,
            &created.types,
            &created.component_item_ids,
            created.replace,
            &tiers,
        )?;
        equipped = true;
        effect = Some(if created.replace {
            WardrobeEffect::Replaced
        } else {
            WardrobeEffect::Layered
        });

        // v4: chat.equippedOutfit?.[targetCharacterId] || {...EMPTY_EQUIPPED_SLOTS}.
        let chat = chats_read::find_by_id(main, chat_id)?;
        let stored = chat
            .as_ref()
            .and_then(|c| c.get("equippedOutfit"))
            .and_then(|eo| eo.get(&target_character_id));
        current_state = Some(Slots::from_value(stored).to_value());

        // triggerAvatarGenerationIfEnabled — image subsystem seam (out of scope).
    }

    let effect_str = effect.map(|e| e.as_str().to_string());
    let effect_summary =
        effect.map(|e| describe_wardrobe_effect(e, &resolved_types, Some(created.title.as_str())));

    Ok(WardrobeCreateToolOutput {
        success: true,
        item_id: created.id.clone(),
        title: created.title.clone(),
        equipped,
        effect: effect_str,
        effect_summary,
        is_composite: Some(is_composite),
        resolved_types: Some(resolved_types),
        resolved_component_item_ids: if component_item_ids.is_empty() {
            None
        } else {
            Some(component_item_ids)
        },
        recipient_name,
        current_state,
        error: None,
    })
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

/// v4 `resolveRecipientFromChat` — find a chat participant character by name
/// (case-insensitive, skipping `removed` participants). Returns `(characterId,
/// characterName)` (the raw name) or `None`.
fn resolve_recipient_from_chat(
    main: &Connection,
    _mount: &Connection,
    chat_id: &str,
    recipient_name: &str,
) -> Result<Option<(String, String)>, crate::db::DbError> {
    let Some(chat) = chats_read::find_by_id(main, chat_id)? else {
        return Ok(None);
    };
    let Some(participants) = chat.get("participants").and_then(Value::as_array) else {
        return Ok(None);
    };
    let normalized = recipient_name.trim().to_lowercase();
    for participant in participants {
        if participant.get("status").and_then(Value::as_str) == Some("removed") {
            continue;
        }
        let Some(char_id) = participant.get("characterId").and_then(Value::as_str) else {
            continue;
        };
        let Some(character) = characters_read::find_by_id_raw(main, char_id)? else {
            continue;
        };
        let name = character
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if name.trim().to_lowercase() == normalized {
            return Ok(Some((char_id.to_string(), name.to_string())));
        }
    }
    Ok(None)
}

/// v4 `resolveComponentItems` — resolve component refs (ids preferred, then
/// titles) across the character's own wardrobe + shared archetypes (own wins on
/// collision), deduped in resolution order. Unknown refs are a NOT_FOUND failure.
fn resolve_component_items(
    main: &Connection,
    docs: &DocMountDocumentsRepository,
    character_id: &str,
    component_ids: Option<&[String]>,
    component_titles: Option<&[String]>,
    tiers: &SharedWardrobeTiers,
) -> Result<Vec<Value>, CreateError> {
    let ids = component_ids.unwrap_or(&[]);
    let titles = component_titles.unwrap_or(&[]);
    if ids.is_empty() && titles.is_empty() {
        return Ok(Vec::new());
    }

    let own_items = find_by_character_id(main, docs, character_id, true)?;
    let archetypes = find_archetypes(main, docs, false, tiers)?;

    // Own items win: insert archetypes first, then own (overrides).
    let mut items_by_id: std::collections::HashMap<String, Value> =
        std::collections::HashMap::new();
    let mut items_by_title: std::collections::HashMap<String, Value> =
        std::collections::HashMap::new();
    for i in archetypes.iter().chain(own_items.iter()) {
        if let Some(id) = i.get("id").and_then(Value::as_str) {
            items_by_id.insert(id.to_string(), i.clone());
        }
        if let Some(title) = i.get("title").and_then(Value::as_str) {
            items_by_title.insert(title.trim().to_lowercase(), i.clone());
        }
    }

    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut resolved: Vec<Value> = Vec::new();

    for id in ids {
        let Some(item) = items_by_id.get(id) else {
            return Err(CreateError::Message(format!(
                "Component item with ID \"{id}\" was not found in this character's wardrobe, the project, or Quilltap General"
            )));
        };
        let item_id = item.get("id").and_then(Value::as_str).unwrap_or_default();
        if seen.insert(item_id.to_string()) {
            resolved.push(item.clone());
        }
    }
    for title in titles {
        let key = title.trim().to_lowercase();
        let Some(item) = items_by_title.get(&key) else {
            return Err(CreateError::Message(format!(
                "Component item titled \"{title}\" was not found in this character's wardrobe, the project, or Quilltap General"
            )));
        };
        let item_id = item.get("id").and_then(Value::as_str).unwrap_or_default();
        if seen.insert(item_id.to_string()) {
            resolved.push(item.clone());
        }
    }

    Ok(resolved)
}

/// v4 `formatWardrobeCreateResults`.
pub fn format(output: &WardrobeCreateToolOutput) -> String {
    if !output.success {
        return format!(
            "Wardrobe Error: {}",
            output.error.as_deref().unwrap_or("Unknown error")
        );
    }
    let recipient_note = output
        .recipient_name
        .as_ref()
        .map(|n| format!(" for {n}"))
        .unwrap_or_default();
    let is_composite = output.is_composite == Some(true);
    let kind_label = if is_composite {
        "composite outfit"
    } else {
        "wardrobe item"
    };
    let mut parts: Vec<String> = vec![format!(
        "Created {} \"{}\" ({}){}",
        kind_label, output.title, output.item_id, recipient_note
    )];

    if is_composite {
        if let Some(ids) = &output.resolved_component_item_ids {
            if !ids.is_empty() {
                let types = output.resolved_types.clone().unwrap_or_default().join(", ");
                parts.push(format!(
                    "- Bundles {} item{}; covers {}",
                    ids.len(),
                    if ids.len() == 1 { "" } else { "s" },
                    types
                ));
            }
        }
    }

    if output.equipped {
        let on_recipient = output
            .recipient_name
            .as_ref()
            .map(|n| format!(" on {n}"))
            .unwrap_or_default();
        parts.push(format!("- Equipped immediately{on_recipient}"));

        if let Some(state) = &output.current_state {
            let slot_summary = WARDROBE_SLOT_TYPES
                .iter()
                // An unreported-if-blank slot (hair) drops out when empty —
                // never surfaced as "(empty)", which reads as "has no hair".
                .filter(|slot| {
                    let occupied = state
                        .get(**slot)
                        .and_then(Value::as_array)
                        .map(|a| !a.is_empty())
                        .unwrap_or(false);
                    occupied || crate::wardrobe::is_slot_reported_when_empty(slot)
                })
                .map(|slot| {
                    let ids = state
                        .get(*slot)
                        .and_then(Value::as_array)
                        .map(|a| {
                            a.iter()
                                .filter_map(|e| e.as_str())
                                .collect::<Vec<_>>()
                                .join(", ")
                        })
                        .unwrap_or_default();
                    let label = if ids.is_empty() { "(empty)" } else { &ids };
                    format!("  {slot}: {label}")
                })
                .collect::<Vec<_>>()
                .join("\n");
            parts.push(format!("- Current outfit:\n{slot_summary}"));
        }
    } else {
        let of_recipient = output
            .recipient_name
            .as_ref()
            .map(|n| format!(" of {n}"))
            .unwrap_or_default();
        parts.push(format!(
            "- Not equipped (added to wardrobe{of_recipient} only)"
        ));
    }

    parts.join("\n")
}
