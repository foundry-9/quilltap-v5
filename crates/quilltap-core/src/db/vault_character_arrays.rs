//! Character **array / sub-array ops** (characters sub-unit 4b). Ports the
//! `systemPrompts` / `scenarios` / `partnerLinks` mutators + the favorite /
//! controlled-by / Carina setters of v4's
//! `lib/database/repositories/characters.repository.ts`.
//!
//! ## The shared shape: read-overlay → mutate-in-memory → write-overlay
//!
//! Every sub-array op in v4 follows the same three beats (the private
//! `addToSubArray` / `updateInSubArray` / `removeFromSubArray` helpers):
//!
//!   1. `findById(characterId)` — the **read overlay** ([`find_by_id`]) hydrates
//!      the character, so `systemPrompts` / `scenarios` reflect the current
//!      `Prompts/` / `Scenarios/` vault folders and `partnerLinks` the slim row.
//!   2. mutate the relevant array in memory (push / find-and-replace / filter),
//!      applying the per-op default-normalization (the `onBeforeAdd` /
//!      `onAfterBuild` / `onAfterRemove` callbacks).
//!   3. `update(characterId, { <array>: items })` — the **write overlay**
//!      ([`update_character`]) reprojects the vault folder (for prompts/scenarios)
//!      or writes the slim column (for partnerLinks), sweeping orphaned files.
//!
//! Because step 3 reprojects each prompt/scenario to `<sanitize(name|title)>.md`
//! with content from the verified [`build_system_prompt_file`] /
//! [`build_scenario_file`] leaves, **the minted item `id` / `createdAt` /
//! `updatedAt` never reach disk** — the read side re-derives a prompt's id from its
//! path (`stableUuidFromString`). So the array ops are deterministic in their DB
//! effect even though they mint ids/clocks; the differential still placeholders the
//! file/link/document timestamps `write_database_document` mints.
//!
//! The setters (`setFavorite` / `setControlledBy` / `setCanBeCarina`) are thin
//! `update(id, { … })` wrappers — no read, no vault, just a slim column.
//!
//! ## `find_by_id` is the shared read path
//!
//! These ops read the character through [`super::characters_read::find_by_id`]
//! (re-exported here) — the same full read path the `findBy*` queries use (sub-unit
//! 4c). So the items they mutate (`systemPrompts` / `scenarios` / `partnerLinks`)
//! carry exactly the vault-overlaid values v4's `findById` returns; the 4b
//! write-effect differential and the 4c read-differential together verify it.

use rusqlite::Connection;
use serde_json::{json, Map, Value};

use super::vault_character_update::update_character;
use super::DbError;

// The array ops read the character through the FULL read path (sub-unit 4c) — the
// same `find_by_id` the `findBy*` queries use — so the items they mutate
// (`systemPrompts` / `scenarios` / `partnerLinks`) carry exactly the vault-overlaid
// values v4's `findById` returns.
pub use super::characters_read::find_by_id;

/// Extract a managed/slim array off a hydrated character (`None`/non-array → `[]`).
fn array_of(character: &Value, key: &str) -> Vec<Value> {
    character
        .get(key)
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

/// Build the `{ <key>: items }` patch and route it through [`update_character`].
fn project_array(
    main: &Connection,
    mount: &Connection,
    character_id: &str,
    key: &str,
    items: Vec<Value>,
) -> Result<(), DbError> {
    let mut patch = Map::new();
    patch.insert(key.to_string(), Value::Array(items));
    update_character(main, mount, character_id, &patch)?;
    Ok(())
}

/// Set every item's `isDefault` to `false` (v4 `existing.forEach(p => p.isDefault = false)`).
fn demote_all(items: &mut [Value]) {
    for item in items.iter_mut() {
        item["isDefault"] = Value::Bool(false);
    }
}

// ============================================================================
// SYSTEM PROMPT OPERATIONS
// ============================================================================

/// Add a system prompt (v4 `addSystemPrompt` via `addToSubArray`). Mints the
/// item's id/timestamps (discarded by the projection), and — when `is_default` is
/// set or this is the first prompt — demotes the others and forces this one
/// default. Returns the added item, or `None` when the character is absent.
pub fn add_system_prompt(
    main: &Connection,
    mount: &Connection,
    character_id: &str,
    name: &str,
    content: &str,
    is_default: bool,
) -> Result<Option<Value>, DbError> {
    let Some(character) = find_by_id(main, mount, character_id)? else {
        return Ok(None);
    };
    let mut items = array_of(&character, "systemPrompts");

    let now = crate::clock::now_iso();
    let mut new_item = json!({
        "id": uuid::Uuid::new_v4().to_string(),
        "name": name,
        "content": content,
        "isDefault": is_default,
        "createdAt": now,
        "updatedAt": now,
    });

    // onBeforeAdd: first prompt or explicit default ⇒ sole default.
    if is_default || items.is_empty() {
        demote_all(&mut items);
        new_item["isDefault"] = Value::Bool(true);
    }
    items.push(new_item.clone());

    project_array(main, mount, character_id, "systemPrompts", items)?;
    Ok(Some(new_item))
}

/// Update a system prompt (v4 `updateSystemPrompt` via `updateInSubArray`). Merges
/// `patch` over the existing item (id/createdAt preserved, updatedAt minted), and —
/// when the patch sets `isDefault: true` — demotes the others. Returns the updated
/// item, or `None` when the character or prompt is absent.
pub fn update_system_prompt(
    main: &Connection,
    mount: &Connection,
    character_id: &str,
    prompt_id: &str,
    patch: &Map<String, Value>,
) -> Result<Option<Value>, DbError> {
    let Some(character) = find_by_id(main, mount, character_id)? else {
        return Ok(None);
    };
    let mut items = array_of(&character, "systemPrompts");
    let Some(index) = items
        .iter()
        .position(|i| i.get("id").and_then(Value::as_str) == Some(prompt_id))
    else {
        return Ok(None);
    };

    let existing = items[index].clone();
    let now = crate::clock::now_iso();

    // buildUpdated: { ...existing, ...data, id, createdAt, updatedAt: now }.
    let mut updated = existing.clone();
    for (k, v) in patch {
        updated[k.as_str()] = v.clone();
    }
    updated["id"] = existing["id"].clone();
    updated["createdAt"] = existing["createdAt"].clone();
    updated["updatedAt"] = Value::String(now);

    // onAfterBuild: an explicit default demotes the rest (operates on the array
    // before the index is overwritten, matching v4).
    if patch.get("isDefault").and_then(Value::as_bool) == Some(true) {
        demote_all(&mut items);
        updated["isDefault"] = Value::Bool(true);
    }
    items[index] = updated.clone();

    project_array(main, mount, character_id, "systemPrompts", items)?;
    Ok(Some(updated))
}

/// Delete a system prompt (v4 `deleteSystemPrompt` via `removeFromSubArray`).
/// Returns `false` when the character or prompt is absent. onAfterRemove: if the
/// survivors include no default, the first survivor is promoted.
pub fn delete_system_prompt(
    main: &Connection,
    mount: &Connection,
    character_id: &str,
    prompt_id: &str,
) -> Result<bool, DbError> {
    let Some(character) = find_by_id(main, mount, character_id)? else {
        return Ok(false);
    };
    let items = array_of(&character, "systemPrompts");
    let mut filtered: Vec<Value> = items
        .iter()
        .filter(|i| i.get("id").and_then(Value::as_str) != Some(prompt_id))
        .cloned()
        .collect();
    if filtered.len() == items.len() {
        return Ok(false); // not found
    }

    // onAfterRemove: keep exactly one default among survivors.
    let has_default = filtered
        .iter()
        .any(|p| p.get("isDefault").and_then(Value::as_bool) == Some(true));
    if !filtered.is_empty() && !has_default {
        filtered[0]["isDefault"] = Value::Bool(true);
    }

    project_array(main, mount, character_id, "systemPrompts", filtered)?;
    Ok(true)
}

/// Make a system prompt the sole default (v4 `setDefaultSystemPrompt`). Every
/// prompt's `isDefault` is set to `i == target` and `updatedAt` minted (the latter
/// discarded by the projection). Returns `false` when the character or prompt is
/// absent.
pub fn set_default_system_prompt(
    main: &Connection,
    mount: &Connection,
    character_id: &str,
    prompt_id: &str,
) -> Result<bool, DbError> {
    let Some(character) = find_by_id(main, mount, character_id)? else {
        return Ok(false);
    };
    let mut prompts = array_of(&character, "systemPrompts");
    let Some(target) = prompts
        .iter()
        .position(|p| p.get("id").and_then(Value::as_str) == Some(prompt_id))
    else {
        return Ok(false);
    };

    let now = crate::clock::now_iso();
    for (i, p) in prompts.iter_mut().enumerate() {
        p["isDefault"] = Value::Bool(i == target);
        p["updatedAt"] = Value::String(now.clone());
    }

    project_array(main, mount, character_id, "systemPrompts", prompts)?;
    Ok(true)
}

// ============================================================================
// SCENARIO OPERATIONS
// ============================================================================

/// Add a scenario (v4 `addScenario` via `addToSubArray`; no default logic).
/// Returns the added item, or `None` when the character is absent.
pub fn add_scenario(
    main: &Connection,
    mount: &Connection,
    character_id: &str,
    title: &str,
    content: &str,
) -> Result<Option<Value>, DbError> {
    let Some(character) = find_by_id(main, mount, character_id)? else {
        return Ok(None);
    };
    let mut items = array_of(&character, "scenarios");

    let now = crate::clock::now_iso();
    let new_item = json!({
        "id": uuid::Uuid::new_v4().to_string(),
        "title": title,
        "content": content,
        "createdAt": now,
        "updatedAt": now,
    });
    items.push(new_item.clone());

    project_array(main, mount, character_id, "scenarios", items)?;
    Ok(Some(new_item))
}

/// Update a scenario (v4 `updateScenario` via `updateInSubArray`; no default
/// logic). Returns the updated item, or `None` when the character or scenario is
/// absent.
pub fn update_scenario(
    main: &Connection,
    mount: &Connection,
    character_id: &str,
    scenario_id: &str,
    patch: &Map<String, Value>,
) -> Result<Option<Value>, DbError> {
    let Some(character) = find_by_id(main, mount, character_id)? else {
        return Ok(None);
    };
    let mut items = array_of(&character, "scenarios");
    let Some(index) = items
        .iter()
        .position(|i| i.get("id").and_then(Value::as_str) == Some(scenario_id))
    else {
        return Ok(None);
    };

    let existing = items[index].clone();
    let now = crate::clock::now_iso();
    let mut updated = existing.clone();
    for (k, v) in patch {
        updated[k.as_str()] = v.clone();
    }
    updated["id"] = existing["id"].clone();
    updated["createdAt"] = existing["createdAt"].clone();
    updated["updatedAt"] = Value::String(now);
    items[index] = updated.clone();

    project_array(main, mount, character_id, "scenarios", items)?;
    Ok(Some(updated))
}

/// Remove a scenario (v4 `removeScenario` via `removeFromSubArray`; no default
/// logic). Returns `false` when the character or scenario is absent.
pub fn remove_scenario(
    main: &Connection,
    mount: &Connection,
    character_id: &str,
    scenario_id: &str,
) -> Result<bool, DbError> {
    let Some(character) = find_by_id(main, mount, character_id)? else {
        return Ok(false);
    };
    let items = array_of(&character, "scenarios");
    let filtered: Vec<Value> = items
        .iter()
        .filter(|i| i.get("id").and_then(Value::as_str) != Some(scenario_id))
        .cloned()
        .collect();
    if filtered.len() == items.len() {
        return Ok(false); // not found
    }

    project_array(main, mount, character_id, "scenarios", filtered)?;
    Ok(true)
}

// ============================================================================
// PARTNER LINK OPERATIONS
// ============================================================================

/// Add a partner link (v4 `addPartnerLink`). Idempotent: a link to an existing
/// `partner_id` is left unchanged. `partnerLinks` is a slim column (not vault
/// managed), so the slim `_update` writes it (bumping `updatedAt`). Returns `false`
/// only when the character is absent.
pub fn add_partner_link(
    main: &Connection,
    mount: &Connection,
    character_id: &str,
    partner_id: &str,
    is_default: bool,
) -> Result<bool, DbError> {
    let Some(character) = find_by_id(main, mount, character_id)? else {
        return Ok(false);
    };
    let mut links = array_of(&character, "partnerLinks");
    let exists = links
        .iter()
        .any(|l| l.get("partnerId").and_then(Value::as_str) == Some(partner_id));
    if !exists {
        links.push(json!({ "partnerId": partner_id, "isDefault": is_default }));
        let mut patch = Map::new();
        patch.insert("partnerLinks".to_string(), Value::Array(links));
        update_character(main, mount, character_id, &patch)?;
    }
    Ok(true)
}

/// Remove a partner link (v4 `removePartnerLink`). A no-op when no link matches.
/// Returns `false` only when the character is absent.
pub fn remove_partner_link(
    main: &Connection,
    mount: &Connection,
    character_id: &str,
    partner_id: &str,
) -> Result<bool, DbError> {
    let Some(character) = find_by_id(main, mount, character_id)? else {
        return Ok(false);
    };
    let links = array_of(&character, "partnerLinks");
    let filtered: Vec<Value> = links
        .iter()
        .filter(|l| l.get("partnerId").and_then(Value::as_str) != Some(partner_id))
        .cloned()
        .collect();
    if filtered.len() != links.len() {
        let mut patch = Map::new();
        patch.insert("partnerLinks".to_string(), Value::Array(filtered));
        update_character(main, mount, character_id, &patch)?;
    }
    Ok(true)
}

// ============================================================================
// FAVORITE / CONTROLLED-BY / CARINA SETTERS
// ============================================================================

/// Apply a single slim-column patch via [`update_character`] (the setters' shared
/// body — v4 `this.update(id, { … })`). Returns whether the slim row was updated.
fn set_one(
    main: &Connection,
    mount: &Connection,
    character_id: &str,
    key: &str,
    value: Value,
) -> Result<bool, DbError> {
    let mut patch = Map::new();
    patch.insert(key.to_string(), value);
    update_character(main, mount, character_id, &patch)
}

/// Set favorite status (v4 `setFavorite`).
pub fn set_favorite(
    main: &Connection,
    mount: &Connection,
    character_id: &str,
    is_favorite: bool,
) -> Result<bool, DbError> {
    set_one(
        main,
        mount,
        character_id,
        "isFavorite",
        Value::Bool(is_favorite),
    )
}

/// Set controlled-by (`'llm'` | `'user'`) (v4 `setControlledBy`).
pub fn set_controlled_by(
    main: &Connection,
    mount: &Connection,
    character_id: &str,
    controlled_by: &str,
) -> Result<bool, DbError> {
    set_one(
        main,
        mount,
        character_id,
        "controlledBy",
        Value::String(controlled_by.to_string()),
    )
}

/// Set Carina (inline @-query answerer) eligibility (v4 `setCanBeCarina`).
pub fn set_can_be_carina(
    main: &Connection,
    mount: &Connection,
    character_id: &str,
    can_be_carina: bool,
) -> Result<bool, DbError> {
    set_one(
        main,
        mount,
        character_id,
        "canBeCarina",
        Value::Bool(can_be_carina),
    )
}
