//! `wardrobe_create` handler (v4 `lib/tools/handlers/wardrobe-create-handler.ts`
//! `executeWardrobeCreateTool` + `formatWardrobeCreateResults`).
//!
//! Creates a leaf or composite wardrobe item and optionally equips it. Supports
//! gifting to another chat participant (`recipient`). Since v4 `9ecebf986` the
//! components resolve against the TARGET's one wearable pool (their vault,
//! their groups, the project behind the roster, Quilltap General — archived
//! parts included by id), the item is written through the keystone's
//! `create_item` at the target's own location (`resolve_wardrobe_location`,
//! keyed on the CALLING user — `userId` is load-bearing: a character the user
//! does not own, or one with no vault, has no wardrobe to hang this in), and a
//! composite's `types` are `build_composite_types` (widen, never narrow). Blank
//! prose stores `null`; `replace` is stored for a leaf too. Cycles are the
//! writer's to refuse.
//!
//! `equip_now` wears the new item at once (`wear_ops::wear_item` over the pool
//! plus the item it predates) and — bug 193 — fires BOTH outfit-change effects
//! for the target (v4 `notifyWardrobeChanged`: the avatar refresh AND the turn's
//! announcement set; the avatar had fired alone). This sync handler cannot run
//! the async effects inside the writer closure, so it surfaces the target in
//! [`WardrobeCreateToolOutput::target_character_id`] and the executor fires
//! them once the write commits.

use rusqlite::Connection;
use serde::Serialize;
use serde_json::Value;

use crate::db::archetype_wardrobe::with_origin;
use crate::db::characters_read;
use crate::db::chats_read;
use crate::db::vault_wardrobe_public::WardrobeScope;
use crate::db::wardrobe_location::{resolve_wardrobe_location, ResolveWardrobeLocationOptions};
use crate::db::wardrobe_wear_stats::EquipSource;
use crate::services::wardrobe_item_mutations::{
    create_item, CreateItemOptions, WardrobeCreateBody,
};
use crate::wardrobe::{
    describe_wardrobe_effect, is_composite, Slots, WardrobeEffect, WARDROBE_SLOT_TYPES,
};
use crate::wear_ops::{wear_item, PutOnMode, WearEffect};
use crate::wearable_pool::{build_wearable_pool, WearablePool};

use super::wardrobe_tool_table::load_tool_pool;
use crate::services::tool_image_generation::{
    format_wardrobe_tool_image_line, WardrobeToolImageResult,
};

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
    /// What became of the picture (v4 `b3f937076`) — present only when one was
    /// wanted. Set by the executor AFTER the write commits
    /// ([`crate::services::tool_image_generation::maybe_queue_wardrobe_tool_image`]).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_generation: Option<WardrobeToolImageResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// The character the item was equipped on (the RECIPIENT of a gift, else
    /// the caller) — set ONLY on the equipped branch. Never serialized: it
    /// carries v4's `targetCharacterId` to the executor's outfit-change
    /// effects (v4 `notifyWardrobeChanged` — bug 193: the avatar refresh AND
    /// the turn's announcement set) without touching the tool's JSON result.
    #[serde(skip)]
    pub target_character_id: Option<String>,
    /// The character whose wardrobe received the item (the RECIPIENT of a
    /// gift, else the caller) — set on EVERY success, equipped or not (v4
    /// draws a new garment by default even when `equip_now` is false). Never
    /// serialized: the executor's picture call reads it (`b3f937076`).
    #[serde(skip)]
    pub picture_character_id: Option<String>,
    /// The model's `generate_image` input (`None` = the tool's default),
    /// carried to the executor's picture call. Never serialized.
    #[serde(skip)]
    pub generate_image: Option<bool>,
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
        image_generation: None,
        error: Some(error.into()),
        target_character_id: None,
        picture_character_id: None,
        generate_image: None,
    }
}

/// v4's `context` on every line the handler logs.
const LOG_CONTEXT: &str = "wardrobe-create-handler";

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
    /// v4 `generate_image: z.boolean().optional()` (`b3f937076`).
    generate_image: Option<bool>,
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
    let generate_image = match obj.get("generate_image") {
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
        generate_image,
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
    let Some(input) = validate(args) else {
        tracing::warn!(
            target: "quilltap::tools",
            context = LOG_CONTEXT,
            userId = user_id,
            chatId = chat_id,
            characterId = character_id,
            inputJson = %args,
            "Wardrobe create tool validation failed",
        );
        return failure(VALIDATION_ERROR);
    };
    match run(main, mount, user_id, chat_id, character_id, &input) {
        Ok(out) => out,
        // v4's `WardrobeCreateError` arm: a WARN with its code, the message
        // to the model.
        Err(CreateError::Coded { code, message }) => {
            tracing::warn!(
                target: "quilltap::tools",
                context = LOG_CONTEXT,
                userId = user_id,
                chatId = chat_id,
                characterId = character_id,
                code = code,
                // v4's `message` key: a tracing field named `message` would
                // replace the file record's envelope text — renamed (recorded).
                errorMessage = %message,
                "Wardrobe create error",
            );
            failure(message)
        }
        // v4's outer catch: a thrown writer error (a composite cycle, an
        // archived owner) or a failed read.
        Err(CreateError::Thrown(message)) => {
            tracing::error!(
                target: "quilltap::tools",
                context = LOG_CONTEXT,
                userId = user_id,
                chatId = chat_id,
                characterId = character_id,
                error = %message,
                "Wardrobe create tool execution failed",
            );
            failure(message)
        }
    }
}

enum CreateError {
    /// v4's `WardrobeCreateError(message, code)`.
    Coded { code: &'static str, message: String },
    /// Anything else thrown inside v4's `try` (its `error.message`).
    Thrown(String),
}

impl From<crate::db::DbError> for CreateError {
    fn from(e: crate::db::DbError) -> Self {
        CreateError::Thrown(crate::db::fallback::error_text(&e))
    }
}

fn run(
    main: &Connection,
    mount: &Connection,
    user_id: &str,
    chat_id: &str,
    character_id: &str,
    input: &CreateInput,
) -> Result<WardrobeCreateToolOutput, CreateError> {
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
                tracing::warn!(
                    target: "quilltap::tools",
                    context = LOG_CONTEXT,
                    userId = user_id,
                    chatId = chat_id,
                    recipientName = %recipient,
                    "Wardrobe create recipient not found in chat",
                );
                return Ok(failure(format!(
                    "Could not find a character named \"{recipient}\" in this chat"
                )));
            }
        }
    }

    // Keyed on the *target* character, not the caller: a gift is assembled from
    // what the recipient can reach, and the group tier is per-character.
    let pool = load_tool_pool(main, mount, chat_id, &target_character_id);
    let components = resolve_component_items(
        &pool,
        input.component_item_ids.as_deref(),
        input.component_titles.as_deref(),
    )?;
    let component_item_ids: Vec<String> = components
        .iter()
        .map(|c| {
            c.get("id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        })
        .collect();

    // Reachable only past the schema's refine (`types: []` fails `.nonempty()`
    // first) — ported for fidelity, unreachable from a model's input on BOTH
    // sides. (v4 also DELETED the old `Composite components do not cover any
    // slots — this should not happen` arm here, unreachable both sides.)
    if component_item_ids.is_empty() && input.types.as_ref().is_none_or(|t| t.is_empty()) {
        return Err(CreateError::Coded {
            code: "VALIDATION_ERROR",
            message: "A wardrobe item must cover at least one slot".to_string(),
        });
    }

    let location = resolve_wardrobe_location(
        main,
        mount,
        WardrobeScope::Character,
        Some(&target_character_id),
        user_id,
        ResolveWardrobeLocationOptions::default(),
    )?;
    let Some(location) = location else {
        return Err(CreateError::Coded {
            code: "NOT_FOUND",
            message: "That character has no wardrobe to hang this in".to_string(),
        });
    };

    let new_item = create_item(
        main,
        mount,
        &location,
        &WardrobeCreateBody {
            title: input.title.clone(),
            description: input.description.clone(),
            image_prompt: input.image_prompt.clone(),
            types: input.types.clone().unwrap_or_default(),
            appropriateness: input.appropriateness.clone(),
            is_default: Some(false),
            component_item_ids: Some(component_item_ids.clone()),
            replace: Some(input.replace.unwrap_or(false)),
            folder: None,
        },
        CreateItemOptions {
            lookup: Some(pool.by_id()),
            preserve: None,
        },
    )
    .map_err(|e| CreateError::Thrown(e.message()))?;
    let composite = is_composite(&new_item.component_item_ids);
    let resolved_types = new_item.types.clone();

    let mut equipped = false;
    let mut effect: Option<WardrobeEffect> = None;
    let mut current_state: Option<Value> = None;

    if input.equip_now {
        // The pool predates the item; wear it against a lookup that knows it
        // (v4 `withNew = { ...pool, byId: new Map([...pool.byId, [id, item]]) }`).
        let new_value = with_origin(
            vec![serde_json::to_value(&new_item).unwrap_or(Value::Null)],
            &location.origin,
        )
        .remove(0);
        let with_new = pool_with_item(&pool, new_value.clone());
        let outcome = wear_item(
            main,
            chat_id,
            &with_new,
            &new_value,
            PutOnMode::Wear,
            None,
            EquipSource::Tool,
        )?;
        equipped = true;
        effect = Some(match outcome.effect {
            WearEffect::Replaced => WardrobeEffect::Replaced,
            WearEffect::Layered => WardrobeEffect::Layered,
        });

        // v4 `275cd7bc` (bug 78): read through the repository so the stored bag
        // is normalized to all five slots — a chat row written before a slot
        // existed omits its key.
        current_state = Some(
            crate::db::chats_outfits::ChatOutfitsRepository::new(main)
                .get_equipped_outfit_for_character(chat_id, &target_character_id)
                .unwrap_or_else(|| Slots::fresh().to_value()),
        );
        // v4 fires `notifyWardrobeChanged` for `targetCharacterId` here; the
        // executor fires it once the write commits (see the module note).
    }

    let effect_str = effect.map(|e| e.as_str().to_string());
    let effect_summary =
        effect.map(|e| describe_wardrobe_effect(e, &resolved_types, Some(new_item.title.as_str())));

    Ok(WardrobeCreateToolOutput {
        success: true,
        item_id: new_item.id.clone(),
        title: new_item.title.clone(),
        equipped,
        effect: effect_str,
        effect_summary,
        is_composite: Some(composite),
        resolved_types: Some(resolved_types),
        resolved_component_item_ids: if component_item_ids.is_empty() {
            None
        } else {
            Some(component_item_ids)
        },
        recipient_name,
        current_state,
        image_generation: None,
        error: None,
        target_character_id: if equipped {
            Some(target_character_id.clone())
        } else {
            None
        },
        picture_character_id: Some(target_character_id),
        generate_image: input.generate_image,
    })
}

/// v4's `withNew` — the pool's `byId` (in its order) plus one more item, for
/// the slot math of an item the pool predates. Built through
/// [`build_wearable_pool`]: the pool's items as ONE layer (already shadowed,
/// so their order and values stand) and the new item over them.
fn pool_with_item(pool: &WearablePool, item: Value) -> WearablePool {
    build_wearable_pool(
        &pool.character_id,
        pool.tiers.clone(),
        vec![item],
        Vec::new(),
        Vec::new(),
        pool.by_id_ordered().cloned().collect(),
    )
}

/// v4's INFO `Wardrobe create completed` — logged once the picture has been
/// decided (it carries the outcome's `status`), so the executor (and the
/// differential, composing as the executor does) call it after
/// `maybe_queue_wardrobe_tool_image`. Only a successful create logs it.
pub fn log_create_completed(
    user_id: &str,
    chat_id: &str,
    character_id: &str,
    out: &WardrobeCreateToolOutput,
) {
    if !out.success {
        return;
    }
    let target = out.picture_character_id.as_deref().unwrap_or(character_id);
    let component_count = out.resolved_component_item_ids.as_ref().map_or(0, Vec::len);
    let composite = out.is_composite == Some(true);
    // v4 omits an `undefined` field (`recipientName`, `effect`,
    // `imageGeneration`), so each combination is its own callsite.
    let image = out.image_generation.as_ref().map(|i| i.status.as_str());
    macro_rules! completed {
        ($($extra:tt)*) => {
            tracing::info!(
                target: "quilltap::tools",
                context = LOG_CONTEXT,
                userId = user_id,
                chatId = chat_id,
                characterId = character_id,
                targetCharacterId = target,
                $($extra)*
                "Wardrobe create completed",
            )
        };
    }
    match (&out.recipient_name, &out.effect, image) {
        (None, None, None) => {
            completed!(itemId = %out.item_id, title = %out.title, isComposite = composite, componentCount = component_count, equipped = out.equipped,)
        }
        (None, None, Some(i)) => {
            completed!(itemId = %out.item_id, title = %out.title, isComposite = composite, componentCount = component_count, equipped = out.equipped, imageGeneration = i,)
        }
        (None, Some(e), None) => {
            completed!(itemId = %out.item_id, title = %out.title, isComposite = composite, componentCount = component_count, equipped = out.equipped, effect = %e,)
        }
        (None, Some(e), Some(i)) => {
            completed!(itemId = %out.item_id, title = %out.title, isComposite = composite, componentCount = component_count, equipped = out.equipped, effect = %e, imageGeneration = i,)
        }
        (Some(r), None, None) => {
            completed!(recipientName = %r, itemId = %out.item_id, title = %out.title, isComposite = composite, componentCount = component_count, equipped = out.equipped,)
        }
        (Some(r), None, Some(i)) => {
            completed!(recipientName = %r, itemId = %out.item_id, title = %out.title, isComposite = composite, componentCount = component_count, equipped = out.equipped, imageGeneration = i,)
        }
        (Some(r), Some(e), None) => {
            completed!(recipientName = %r, itemId = %out.item_id, title = %out.title, isComposite = composite, componentCount = component_count, equipped = out.equipped, effect = %e,)
        }
        (Some(r), Some(e), Some(i)) => {
            completed!(recipientName = %r, itemId = %out.item_id, title = %out.title, isComposite = composite, componentCount = component_count, equipped = out.equipped, effect = %e, imageGeneration = i,)
        }
    }
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

/// v4 `resolveComponentItems(pool, componentIds, componentTitles)` — resolve
/// component references against the target's pool into a deduplicated,
/// ordered list: ids first (`pool.get` — the shadowed `byId`, ARCHIVED parts
/// included), then titles (`pool.findByTitle` — the character's own items win
/// a title collision). An unknown reference is v4's `NOT_FOUND`.
fn resolve_component_items<'p>(
    pool: &'p WearablePool,
    component_ids: Option<&[String]>,
    component_titles: Option<&[String]>,
) -> Result<Vec<&'p Value>, CreateError> {
    let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
    let mut resolved: Vec<&'p Value> = Vec::new();
    let mut add = |item: &'p Value| {
        let id = item.get("id").and_then(Value::as_str).unwrap_or_default();
        if seen.insert(id) {
            resolved.push(item);
        }
    };
    for id in component_ids.unwrap_or(&[]) {
        let Some(item) = pool.get(id) else {
            return Err(CreateError::Coded {
                code: "NOT_FOUND",
                message: format!(
                    "Component item with ID \"{id}\" was not found in this character's wardrobe, their groups, the project, or Quilltap General"
                ),
            });
        };
        add(item);
    }
    for title in component_titles.unwrap_or(&[]) {
        let Some(item) = pool.find_by_title(title) else {
            return Err(CreateError::Coded {
                code: "NOT_FOUND",
                message: format!(
                    "Component item titled \"{title}\" was not found in this character's wardrobe, their groups, the project, or Quilltap General"
                ),
            });
        };
        add(item);
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

    // v4 `b3f937076`: the picture line LAST.
    if let Some(line) = format_wardrobe_tool_image_line(output.image_generation.as_ref()) {
        parts.push(line);
    }

    parts.join("\n")
}
