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
//! ## [P4.48] A failed existence read sinks the preview
//!
//! Every check below used to be `.ok().flatten()` — a read error read as
//! "doesn't exist", which reports a confident `exists: false` the instance
//! cannot actually support. They now propagate. `previewImport` carries no
//! `try`, so v4's own throwing legs (an unavailable project/group store, which
//! `applyOverlayOne` raises un-wrapped) sink its preview identically; the
//! swallowing legs (`safeQuery(…, null)` inside `_findById`) are the ruled
//! divergence recorded on the preflight in `mod.rs`.
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
            // [P4.48] The old comment here claimed v4's `findById` swallows the
            // overlay failure to `null`. The fresh survey at `aa464abf` says
            // otherwise: `store-backed.findById` is
            // `applyOverlayOne(await this._findById(id))` and only the INNER
            // `_findById` sits in a `safeQuery(…, null)`. `applyOverlayOne`
            // throws, un-wrapped, and `previewImport` has no `try` — so an
            // unavailable store sinks v4's whole preview too.
            Ok(repo.find_by_id(id).map_err(|e| e.into_db())?.is_some())
        })?
    };
    let groups_out = {
        let repo = groups::GroupsRepository::new(main, mount);
        check(arr("groups"), "groups", &mut |id| {
            Ok(repo.find_by_id(id).map_err(|e| e.into_db())?.is_some())
        })?
    };

    // Characters: id first, then the case-insensitive name fallback.
    let characters_out = check_characters(main, arr("characters"), &mut conflict_counts)?;

    // v4 `7189a968`: document stores (and the configuration-shaped types, which
    // land with the five new export types) are previewed AFTER the parallel
    // block, each against the key its importer dedupes on — for stores that is
    // the id (`globalRepos.docMountPoints.findById`, error-swallowed to null).
    // Their conflictCounts entries therefore insert after `characters`.
    let document_stores_out: Vec<Value> = {
        let repo = crate::db::doc_mount_points::DocMountPointsRepository::new(mount);
        let mut out = Vec::new();
        let mut conflicts = 0i64;
        for mp in arr("mountPoints").map(|v| v.as_slice()).unwrap_or(&[]) {
            let id = id_of(mp);
            let exists = repo.find_full_json_by_id(&id)?.is_some();
            if exists {
                conflicts += 1;
            }
            out.push(entity(
                &id,
                mp.get("name").and_then(Value::as_str).unwrap_or_default(),
                exists,
                None,
            ));
        }
        if conflicts > 0 {
            conflict_counts.insert("documentStores".to_string(), Value::from(conflicts));
        }
        out
    };

    // files — by id, plus the bytes-missing `detail` (`7189a968`).
    let files_out: Vec<Value> = {
        let repo = crate::db::files::FilesRepository::new(main);
        let mut out = Vec::new();
        let mut conflicts = 0i64;
        for file in arr("files").map(|v| v.as_slice()).unwrap_or(&[]) {
            let id = id_of(file);
            let exists = repo.find_by_id(&id)?.is_some();
            if exists {
                conflicts += 1;
            }
            let mut m = Map::new();
            m.insert("id".into(), Value::String(id));
            m.insert(
                "name".into(),
                file.get("originalFilename").cloned().unwrap_or(Value::Null),
            );
            m.insert("exists".into(), Value::Bool(exists));
            if file
                .get("_bytesMissing")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                m.insert(
                    "detail".into(),
                    Value::String("contents missing — will be skipped".into()),
                );
            }
            out.push(Value::Object(m));
        }
        if conflicts > 0 {
            conflict_counts.insert("files".to_string(), Value::from(conflicts));
        }
        out
    };

    // prompt templates — by NAME (the importer's dedupe key), carrying
    // `matchedExistingId` on a hit.
    let prompt_templates_out: Vec<Value> = {
        let mut out = Vec::new();
        let mut conflicts = 0i64;
        for template in arr("promptTemplates").map(|v| v.as_slice()).unwrap_or(&[]) {
            let name = template
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let existing = crate::db::prompt_templates::find_by_name(main, user_id, name)?;
            if existing.is_some() {
                conflicts += 1;
            }
            out.push(entity(
                &id_of(template),
                name,
                existing.is_some(),
                existing.as_deref(),
            ));
        }
        if conflicts > 0 {
            conflict_counts.insert("promptTemplates".to_string(), Value::from(conflicts));
        }
        out
    };

    // provider models — always an upsert by (provider, modelId), so "exists"
    // would be noise (`exists: false` always).
    let provider_models_out: Vec<Value> = arr("providerModels")
        .map(|v| v.as_slice())
        .unwrap_or(&[])
        .iter()
        .map(|model| {
            let mut m = Map::new();
            m.insert("id".into(), model.get("id").cloned().unwrap_or(Value::Null));
            m.insert(
                "name".into(),
                Value::String(format!(
                    "{} / {}",
                    model.get("provider").and_then(Value::as_str).unwrap_or(""),
                    model.get("modelId").and_then(Value::as_str).unwrap_or("")
                )),
            );
            m.insert("exists".into(), Value::Bool(false));
            Value::Object(m)
        })
        .collect();

    // plugin configs — by (user, plugin); the redaction `detail` tells the
    // user exactly what they will have to type back in.
    let plugin_configs_out: Vec<Value> = {
        let mut out = Vec::new();
        let mut conflicts = 0i64;
        for config in arr("pluginConfigs").map(|v| v.as_slice()).unwrap_or(&[]) {
            let plugin_name = config
                .get("pluginName")
                .and_then(Value::as_str)
                .unwrap_or_default();
            // [P4.48] `.is_ok()` used to stand here, which folded a read FAILURE
            // into the same `false` as a genuine miss. v4's
            // `pluginConfigs.findByUserAndPlugin` is another `safeQuery(…, null)`
            // — the same swallow — so this is the ruled divergence leg, not a
            // fidelity fix. Only `QueryReturnedNoRows` means "absent".
            let exists = match main.query_row(
                "SELECT 1 FROM plugin_configs WHERE userId = ?1 AND pluginName = ?2",
                rusqlite::params![user_id, plugin_name],
                |_| Ok(()),
            ) {
                Ok(()) => true,
                Err(rusqlite::Error::QueryReturnedNoRows) => false,
                Err(e) => return Err(e.into()),
            };
            if exists {
                conflicts += 1;
            }
            let redacted: Vec<&str> = config
                .get("_redactedKeys")
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(Value::as_str).collect())
                .unwrap_or_default();
            let mut m = Map::new();
            m.insert("id".into(), Value::String(id_of(config)));
            m.insert("name".into(), Value::String(plugin_name.to_string()));
            m.insert("exists".into(), Value::Bool(exists));
            if !redacted.is_empty() {
                m.insert(
                    "detail".into(),
                    Value::String(if redacted.contains(&"*") {
                        "all settings withheld — re-enter them here".to_string()
                    } else {
                        format!("secrets withheld: {}", redacted.join(", "))
                    }),
                );
            }
            out.push(Value::Object(m));
        }
        if conflicts > 0 {
            conflict_counts.insert("pluginConfigs".to_string(), Value::from(conflicts));
        }
        out
    };

    // instance settings — always overwrite (that's the point of the type).
    let instance_settings_out: Vec<Value> = arr("instanceSettings")
        .map(|v| v.as_slice())
        .unwrap_or(&[])
        .iter()
        .map(|setting| {
            let key = setting
                .get("key")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let mut m = Map::new();
            m.insert("id".into(), Value::String(key.to_string()));
            m.insert("name".into(), Value::String(key.to_string()));
            m.insert("exists".into(), Value::Bool(false));
            Value::Object(m)
        })
        .collect();

    let mut entities = Map::new();
    let mut put = |key: &str, v: Vec<Value>| {
        if !v.is_empty() {
            entities.insert(key.to_string(), Value::Array(v));
        }
    };
    // v4's spread order inside the `entities` literal (`7189a968` :232-247):
    // the configuration-shaped kinds lead, then the entity kinds.
    put("documentStores", document_stores_out);
    put("files", files_out);
    put("promptTemplates", prompt_templates_out);
    put("providerModels", provider_models_out);
    put("pluginConfigs", plugin_configs_out);
    put("instanceSettings", instance_settings_out);
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// [P4.48] The preview's existence checks propagate a failed read instead of
    /// reporting a confident `exists: false` the instance cannot support.
    ///
    /// v4's `previewImport` carries no `try`, so its own throwing legs sink the
    /// preview identically; the legs v4 swallows (`safeQuery(…, null)`) are the
    /// ruled divergence recorded on the preflight in `mod.rs`.
    fn preview_of(data: Value, main: &Connection, mount: &Connection) -> Result<Value, DbError> {
        let export = super::super::QuilltapExport {
            manifest: json!({"format": "quilltap-export", "version": "1.0"}),
            data,
        };
        preview_import(main, mount, "user", &export)
    }

    /// Every kind whose existence test reads a repository, against a database
    /// with no tables at all.
    #[test]
    fn preview_propagates_a_failed_existence_read() {
        let cases: Vec<(&str, Value)> = vec![
            ("chats", json!({"id": "ch1", "title": "C"})),
            ("tags", json!({"id": "t1", "name": "T"})),
            ("connectionProfiles", json!({"id": "cp1", "name": "P"})),
            ("imageProfiles", json!({"id": "ip1", "name": "P"})),
            ("embeddingProfiles", json!({"id": "ep1", "name": "P"})),
            ("roleplayTemplates", json!({"id": "rt1", "name": "R"})),
            ("projects", json!({"id": "pr1", "name": "P"})),
            ("groups", json!({"id": "g1", "name": "G"})),
            ("mountPoints", json!({"id": "mp1", "name": "S"})),
            ("files", json!({"id": "f1", "originalFilename": "a.png"})),
            ("promptTemplates", json!({"id": "pt1", "name": "T"})),
            ("pluginConfigs", json!({"id": "pc1", "pluginName": "p"})),
        ];
        for (key, item) in cases {
            let main = Connection::open_in_memory().unwrap();
            let mount = Connection::open_in_memory().unwrap();
            let out = preview_of(json!({ key: [item] }), &main, &mount);
            assert!(
                out.is_err(),
                "{key}: an unreadable table must sink the preview, not read as \
                 `exists: false`"
            );
        }
    }

    /// The overlay leg — v4 throws here too (`applyOverlayOne`, un-wrapped).
    #[test]
    fn preview_propagates_an_unavailable_project_store() {
        let main = Connection::open_in_memory().unwrap();
        main.execute_batch(
            "CREATE TABLE projects (
                 id TEXT PRIMARY KEY,
                 name TEXT,
                 officialMountPointId TEXT,
                 createdAt TEXT,
                 updatedAt TEXT
             );
             INSERT INTO projects (id, name, officialMountPointId, createdAt, updatedAt)
             VALUES ('pr1', 'Planted', NULL, '2026-01-01T00:00:00.000Z', '2026-01-01T00:00:00.000Z');",
        )
        .unwrap();
        let mount = Connection::open_in_memory().unwrap();
        let err = preview_of(
            json!({"projects": [{"id": "pr1", "name": "P"}]}),
            &main,
            &mount,
        )
        .expect_err("an unavailable store sinks the preview");
        assert!(
            matches!(err, DbError::StoreUnavailable { .. }),
            "the `Unavailable` arm must survive structurally so the api layer \
             can answer v4's contextful 503 (P4.23) — got {err:?}"
        );
    }

    /// A genuine miss still reads as `exists: false` — only the ERROR leg moved.
    #[test]
    fn a_genuine_miss_still_reads_as_free() {
        let main = Connection::open_in_memory().unwrap();
        main.execute_batch(
            "CREATE TABLE tags (
                 id TEXT PRIMARY KEY,
                 userId TEXT,
                 name TEXT,
                 nameLower TEXT,
                 quickHide INTEGER,
                 visualStyle TEXT,
                 createdAt TEXT,
                 updatedAt TEXT
             );",
        )
        .unwrap();
        let mount = Connection::open_in_memory().unwrap();
        let out = preview_of(json!({"tags": [{"id": "t1", "name": "T"}]}), &main, &mount)
            .expect("an empty but readable table is a clean miss");
        assert_eq!(out["entities"]["tags"][0]["exists"], json!(false));
        assert_eq!(out["conflictCounts"], json!({}));
    }
}
