//! `wardrobe_update` handler (v4 `lib/tools/handlers/wardrobe-update-handler.ts`
//! `executeWardrobeUpdateTool` + `formatWardrobeUpdateResults`).
//!
//! Edits the stored fields of an existing OWN item (`pool.owns` — shared items
//! are read-only). Only supplied fields change, through the keystone's ONE
//! `update_item` (v4 `9ecebf986`), which owns the composite `types` rule:
//! WIDEN, never narrow (bug 195) — a components-only edit keeps every slot the
//! composite already designated, a `types`-only edit on a composite is widened
//! over its components. (v5's pre-`9ecebf986` block that replaced `types` with
//! the new components' union was v4's own narrowing, retired with it.) Echoes
//! back the updated item in `wardrobe_read` shape. Does NOT equip.

use rusqlite::Connection;
use serde_json::Value;

use crate::db::vault_wardrobe_public::{WardrobePatch, WardrobeScope};
use crate::db::wardrobe_location::{resolve_wardrobe_location, ResolveWardrobeLocationOptions};
use crate::services::wardrobe_item_mutations::{update_item_with_origin, ItemPatch, PatchOrigin};
use crate::vault_overlay::WardrobeItem;
use crate::wardrobe::normalize_no_item_sentinel;
use crate::wear_ops::{find_in_pool, WearableRef};

use super::wardrobe_read::{
    build_read_failure, build_read_output, not_found_message, WardrobeReadToolOutput,
};
use super::wardrobe_tool_table::{load_tool_pool, shared_wardrobe_item_read_only_message};
use crate::services::tool_image_generation::{
    format_wardrobe_tool_image_line, patch_changes_look, WardrobeToolImageResult,
};

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
    /// v4 `generate_image: z.boolean().optional()` (`b3f937076`).
    generate_image: Option<bool>,
}

/// What a successful update hands the executor's picture call (v4's
/// `maybeQueueWardrobeToolImage` arguments the handler computes): the item, the
/// model's `generate_image`, and whether the patch changed how the item looks
/// (the default when the operator allows tool pictures) — plus the patch's
/// keys, for v4's `Wardrobe update completed` INFO, which follows the picture
/// ([`log_update_completed`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdatePictureClaim {
    pub item_id: String,
    pub requested: Option<bool>,
    pub changes_look: bool,
    /// v4 `Object.keys(patch)` — the HANDLER's patch (never a recomputed
    /// `types`: since `9ecebf986` the widening happens inside `update_item`).
    pub fields: Vec<&'static str>,
}

/// v4's `context` on every line the handler logs.
const LOG_CONTEXT: &str = "wardrobe-update-handler";

const VALIDATION_ERROR: &str = "Invalid input: item_id or item_title is required.";

const SLOT_ENUM: [&str; 5] = crate::wardrobe::WARDROBE_SLOT_TYPES;

// P4.170 C.9: v4's tool input schema marks NO key `.nullable()` — every one
// is `.optional()` (`.default(false)` for `equip_now`), so an explicit `null`
// is a Zod refusal, never "absent". Each typed read below answers `None` only
// for a MISSING key.
fn opt_string(obj: &serde_json::Map<String, Value>, key: &str) -> Result<Option<String>, ()> {
    match obj.get(key) {
        None => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.clone())),
        Some(_) => Err(()),
    }
}

fn opt_bool(obj: &serde_json::Map<String, Value>, key: &str) -> Result<Option<bool>, ()> {
    match obj.get(key) {
        None => Ok(None),
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
        None => None,
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
    let generate_image = opt_bool(obj, "generate_image").ok()?;
    // component_item_ids: array of string.
    let component_item_ids = match obj.get("component_item_ids") {
        None => None,
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
        generate_image,
    })
}

/// Execute `wardrobe_update` (v4 `executeWardrobeUpdateTool`) — the write and
/// the read-shaped echo; the picture is the executor's (see
/// [`execute_with_picture_claim`]).
pub fn execute(
    main: &Connection,
    mount: &Connection,
    user_id: &str,
    chat_id: &str,
    character_id: &str,
    args: &Value,
) -> WardrobeReadToolOutput {
    execute_with_picture_claim(main, mount, user_id, chat_id, character_id, args).0
}

/// [`execute`], plus — on success — the [`UpdatePictureClaim`] the executor
/// hands `maybe_queue_wardrobe_tool_image` once the write commits (v4 queues
/// it inside the handler; the queue is async and v5's write closure is not).
pub fn execute_with_picture_claim(
    main: &Connection,
    mount: &Connection,
    user_id: &str,
    chat_id: &str,
    character_id: &str,
    args: &Value,
) -> (WardrobeReadToolOutput, Option<UpdatePictureClaim>) {
    let Some(input) = validate(args) else {
        tracing::warn!(
            target: "quilltap::tools",
            context = LOG_CONTEXT,
            userId = user_id,
            chatId = chat_id,
            characterId = character_id,
            inputJson = %args,
            "Wardrobe update tool validation failed",
        );
        return (build_read_failure(VALIDATION_ERROR), None);
    };
    match run(main, mount, user_id, chat_id, character_id, &input) {
        Ok(out) => out,
        // v4's catch: a thrown writer error (a composite cycle, an archived
        // owner) or a failed location read.
        Err(message) => {
            tracing::error!(
                target: "quilltap::tools",
                context = LOG_CONTEXT,
                userId = user_id,
                chatId = chat_id,
                characterId = character_id,
                error = %message,
                "Wardrobe update tool execution failed",
            );
            (build_read_failure(message), None)
        }
    }
}

fn run(
    main: &Connection,
    mount: &Connection,
    user_id: &str,
    chat_id: &str,
    character_id: &str,
    input: &UpdateInput,
) -> Result<(WardrobeReadToolOutput, Option<UpdatePictureClaim>), String> {
    let pool = load_tool_pool(main, mount, chat_id, character_id);
    let item_id = normalize_no_item_sentinel(input.item_id.as_deref());
    let item_title = normalize_no_item_sentinel(input.item_title.as_deref());
    let Some(item) = find_in_pool(
        &pool,
        WearableRef {
            item_id: item_id.as_deref(),
            item_title: item_title.as_deref(),
        },
    ) else {
        return Ok((
            build_read_failure(not_found_message(&input.item_id, &input.item_title)),
            None,
        ));
    };
    let title = item
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();

    if !pool.owns(item) {
        return Ok((
            build_read_failure(shared_wardrobe_item_read_only_message(&title, "changed")),
            None,
        ));
    }

    // The patch: only supplied fields, in v4's `if` order (= `Object.keys`).
    let mut fields: Vec<&'static str> = Vec::new();
    let mut note = |present: bool, key: &'static str| {
        if present {
            fields.push(key);
        }
    };
    note(input.title.is_some(), "title");
    note(input.description.is_some(), "description");
    note(input.image_prompt.is_some(), "imagePrompt");
    note(input.appropriateness.is_some(), "appropriateness");
    note(input.types.is_some(), "types");
    note(input.is_default.is_some(), "isDefault");
    note(input.replace.is_some(), "replace");
    note(input.component_item_ids.is_some(), "componentItemIds");
    let patch = ItemPatch {
        title: input.title.clone(),
        description: input.description.clone().map(Some),
        image_prompt: input.image_prompt.clone().map(Some),
        appropriateness: input.appropriateness.clone().map(Some),
        types: input.types.clone(),
        is_default: input.is_default,
        replace: input.replace,
        component_item_ids: input.component_item_ids.clone(),
        ..ItemPatch::default()
    };

    let location = resolve_wardrobe_location(
        main,
        mount,
        WardrobeScope::Character,
        Some(character_id),
        user_id,
        ResolveWardrobeLocationOptions::default(),
    )
    .map_err(|e| crate::db::fallback::error_text(&e))?;
    let updated = match &location {
        // C3: the tool's own patch order (`wardrobe-update-handler.ts:99-107`).
        Some(location) => update_item_with_origin(
            main,
            mount,
            location,
            &WardrobeItem::from_read_value(item),
            &patch,
            Some(pool.by_id()),
            PatchOrigin::Tool,
        )
        .map_err(|e| e.message())?,
        None => None,
    };
    let Some(updated) = updated else {
        return Ok((
            build_read_failure(format!("Failed to update wardrobe item \"{title}\"")),
            None,
        ));
    };

    // An edit redraws by default only when it changes how the item looks
    // (v4 `patchChangesLook(item, patch)` — the PRE-update item against the
    // HANDLER's patch).
    let look_patch = WardrobePatch {
        title: patch.title.clone(),
        image_prompt: patch.image_prompt.clone(),
        types: patch.types.clone(),
        component_item_ids: patch.component_item_ids.clone(),
        ..WardrobePatch::default()
    };
    let claim = UpdatePictureClaim {
        item_id: updated.id.clone(),
        requested: input.generate_image,
        changes_look: patch_changes_look(item, &look_patch),
        fields,
    };
    let updated_value = serde_json::to_value(&updated).unwrap_or(Value::Null);
    let out = build_read_output(main, &pool, chat_id, &updated_value);
    Ok((out, Some(claim)))
}

/// v4's INFO `Wardrobe update completed` — logged once the picture has been
/// decided (it carries the outcome's `status`), so the executor (and the
/// differential, composing as the executor does) call it after
/// `maybe_queue_wardrobe_tool_image`.
pub fn log_update_completed(
    user_id: &str,
    chat_id: &str,
    character_id: &str,
    claim: &UpdatePictureClaim,
    image_generation: Option<&WardrobeToolImageResult>,
) {
    let fields = serde_json::to_string(&claim.fields).unwrap_or_default();
    match image_generation {
        Some(image) => tracing::info!(
            target: "quilltap::tools",
            context = LOG_CONTEXT,
            userId = user_id,
            chatId = chat_id,
            characterId = character_id,
            itemId = %claim.item_id,
            fieldsJson = %fields,
            changesLook = claim.changes_look,
            imageGeneration = image.status.as_str(),
            "Wardrobe update completed",
        ),
        None => tracing::info!(
            target: "quilltap::tools",
            context = LOG_CONTEXT,
            userId = user_id,
            chatId = chat_id,
            characterId = character_id,
            itemId = %claim.item_id,
            fieldsJson = %fields,
            changesLook = claim.changes_look,
            "Wardrobe update completed",
        ),
    }
}

/// v4 `formatWardrobeUpdateResults`.
pub fn format(output: &WardrobeReadToolOutput) -> String {
    if !output.success {
        return format!(
            "Wardrobe Error: {}",
            output.error.as_deref().unwrap_or("Unknown error")
        );
    }
    // v4 `b3f937076`: the picture line under the one-line echo, when a
    // picture was wanted. (The `wear` the echo carries is NOT printed.)
    match format_wardrobe_tool_image_line(output.image_generation.as_ref()) {
        Some(line) => format!("Updated \"{}\" ({}).\n{line}", output.title, output.item_id),
        None => format!("Updated \"{}\" ({}).", output.title, output.item_id),
    }
}
