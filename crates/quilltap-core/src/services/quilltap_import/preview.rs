//! v4 `previewImport` (`lib/import/quilltap-import/preview.ts:20`) — count what
//! each entity type would import and flag conflicts, **without writing
//! anything**. This is what the Import dialog's step 2 renders.
//!
//! ## The two conflict tests
//!
//! Every kind except characters tests existence **by id only** — the id IS the
//! match key. Characters additionally fall back to a **case-insensitive name
//! match** (the cross-instance import path), and when the name matched, the
//! entity carries `matchedExistingId`. That extra key is absent on an id match
//! and absent on a miss (v4 builds the object literal three different ways).
//!
//! ## The shapes v4's `&&` spreads produce
//!
//! `entities.<kind>` is present only when that kind's array is NON-EMPTY, so a
//! characters-only export yields `entities: { characters: [...] }` and nothing
//! else. `entities.memories` is `{count}` and appears whenever `data.memories`
//! EXISTS — including when it is an empty array (v4 tests `data.memories &&`,
//! and `[]` is truthy in JS). `conflictCounts` only carries kinds with ≥1
//! conflict.

use rusqlite::Connection;
use serde_json::{Map, Value};

use super::QuilltapExport;
use crate::db::{
    characters_read, chats_read, connection_profiles, embedding_profiles, groups, image_profiles,
    projects, roleplay_templates, tags, DbError,
};

/// One `ImportPreviewEntity` (v4 `types.ts:63`).
fn entity(id: &str, name: &str, exists: bool, matched_existing_id: Option<&str>) -> Value {
    let mut m = Map::new();
    m.insert("id".into(), Value::String(id.to_string()));
    m.insert("name".into(), Value::String(name.to_string()));
    m.insert("exists".into(), Value::Bool(exists));
    if let Some(mid) = matched_existing_id {
        m.insert("matchedExistingId".into(), Value::String(mid.to_string()));
    }
    Value::Object(m)
}

/// v4's `checkExists` name resolution: `'name' in item ? item.name : 'title' in
/// item ? item.title : 'Unknown'`.
fn display_name(item: &Value) -> String {
    if let Some(n) = item.get("name").and_then(Value::as_str) {
        return n.to_string();
    }
    if let Some(t) = item.get("title").and_then(Value::as_str) {
        return t.to_string();
    }
    "Unknown".to_string()
}

fn id_of(item: &Value) -> String {
    item.get("id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// v4 `previewImport(userId, exportData)`.
pub fn preview_import(
    main: &Connection,
    mount: &Connection,
    user_id: &str,
    export: &QuilltapExport,
) -> Result<Value, DbError> {
    let empty = Map::new();
    let data = export.data.as_object().unwrap_or(&empty);
    let mut conflict_counts = Map::new();

    // The generic id-only existence check, shared by every non-character kind.
    let mut check = |items: Option<&Vec<Value>>,
                     entity_type: &str,
                     exists: &mut dyn FnMut(&str) -> Result<bool, DbError>|
     -> Result<Vec<Value>, DbError> {
        let Some(items) = items else {
            return Ok(Vec::new());
        };
        let mut results = Vec::new();
        let mut conflicts = 0i64;
        for item in items {
            let id = id_of(item);
            let found = exists(&id)?;
            if found {
                conflicts += 1;
            }
            results.push(entity(&id, &display_name(item), found, None));
        }
        if conflicts > 0 {
            conflict_counts.insert(entity_type.to_string(), Value::from(conflicts));
        }
        Ok(results)
    };

    let arr = |key: &str| data.get(key).and_then(Value::as_array);

    let chats = check(arr("chats"), "chats", &mut |id| {
        Ok(chats_read::find_by_id(main, id)?.is_some())
    })?;
    let tags_out = check(arr("tags"), "tags", &mut |id| {
        Ok(tags::find_full_by_id(main, id)?.is_some())
    })?;
    let connection_profiles_out =
        check(arr("connectionProfiles"), "connectionProfiles", &mut |id| {
            Ok(connection_profiles::find_by_id(main, id)?.is_some())
        })?;
    let image_profiles_out = check(arr("imageProfiles"), "imageProfiles", &mut |id| {
        Ok(image_profiles::find_by_id(main, id)?.is_some())
    })?;
    let embedding_profiles_out = check(arr("embeddingProfiles"), "embeddingProfiles", &mut |id| {
        Ok(embedding_profiles::find_by_id(main, id)?.is_some())
    })?;
    let roleplay_templates_out = check(arr("roleplayTemplates"), "roleplayTemplates", &mut |id| {
        Ok(roleplay_templates::find_full_json_by_id(main, id)?.is_some())
    })?;
    let projects_out = {
        let repo = projects::ProjectsRepository::new(main, mount);
        check(arr("projects"), "projects", &mut |id| {
            // The overlay read throws when a project's store is missing; v4's
            // `findById` swallows to `null`, so treat any overlay failure as
            // "not found" rather than sinking the whole preview.
            Ok(repo.find_by_id(id).ok().flatten().is_some())
        })?
    };
    let groups_out = {
        let repo = groups::GroupsRepository::new(main, mount);
        check(arr("groups"), "groups", &mut |id| {
            Ok(repo.find_by_id(id).ok().flatten().is_some())
        })?
    };

    // Characters: id first, then the case-insensitive name fallback.
    let characters_out = check_characters(main, arr("characters"), &mut conflict_counts)?;

    let mut entities = Map::new();
    let mut put = |key: &str, v: Vec<Value>| {
        if !v.is_empty() {
            entities.insert(key.to_string(), Value::Array(v));
        }
    };
    // v4's spread order inside the `entities` literal (:161-172).
    put("characters", characters_out);
    put("chats", chats);
    put("tags", tags_out);
    put("connectionProfiles", connection_profiles_out);
    put("imageProfiles", image_profiles_out);
    put("embeddingProfiles", embedding_profiles_out);
    put("roleplayTemplates", roleplay_templates_out);
    put("projects", projects_out);
    put("groups", groups_out);
    // ⚠ `data.memories && {…}` — an EMPTY array is truthy in JS, so a
    // `"memories": []` key still emits `{count: 0}`. Only an ABSENT key omits it.
    if let Some(memories) = data.get("memories").and_then(Value::as_array) {
        let mut c = Map::new();
        c.insert("count".into(), Value::from(memories.len()));
        entities.insert("memories".into(), Value::Object(c));
    }

    let mut out = Map::new();
    out.insert("manifest".into(), export.manifest.clone());
    out.insert("entities".into(), Value::Object(entities));
    out.insert("conflictCounts".into(), Value::Object(conflict_counts));
    // v4's user id is only used for logging + the repo scope; single-user v5
    // scopes by construction.
    let _ = user_id;
    Ok(Value::Object(out))
}

/// v4 `checkCharacterExists` (`preview.ts:62`) — the id-then-name conflict test.
/// The existing-by-name map is built ONCE from `characters.findAll()` and keyed
/// on the lowercased name; a later duplicate name in the same map overwrites the
/// earlier entry (JS `Map.set`), so the LAST character with a given name wins.
fn check_characters(
    main: &Connection,
    items: Option<&Vec<Value>>,
    conflict_counts: &mut Map<String, Value>,
) -> Result<Vec<Value>, DbError> {
    let Some(items) = items else {
        return Ok(Vec::new());
    };

    // `find_all_raw` (no vault overlay): the preview needs only id + name, both
    // slim columns, and one broken vault must not sink the whole preview.
    let existing = characters_read::find_all_raw(main)?;
    let mut by_name: Vec<(String, String)> = Vec::new();
    for c in &existing {
        let (Some(id), Some(name)) = (
            c.get("id").and_then(Value::as_str),
            c.get("name").and_then(Value::as_str),
        ) else {
            continue;
        };
        let key = name.to_lowercase();
        match by_name.iter_mut().find(|(k, _)| *k == key) {
            Some(slot) => slot.1 = id.to_string(),
            None => by_name.push((key, id.to_string())),
        }
    }

    let mut results = Vec::new();
    let mut conflicts = 0i64;
    for item in items {
        let id = id_of(item);
        let name = item
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();

        if characters_read::find_by_id_raw(main, &id)?.is_some() {
            conflicts += 1;
            results.push(entity(&id, &name, true, None));
            continue;
        }
        let lowered = name.to_lowercase();
        if let Some((_, existing_id)) = by_name.iter().find(|(k, _)| *k == lowered) {
            conflicts += 1;
            results.push(entity(&id, &name, true, Some(existing_id)));
            continue;
        }
        results.push(entity(&id, &name, false, None));
    }

    if conflicts > 0 {
        conflict_counts.insert("characters".to_string(), Value::from(conflicts));
    }
    Ok(results)
}
