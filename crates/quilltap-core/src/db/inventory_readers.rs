//! Inventory readers — parsing a tier's `Inventory/**/*.json` files into
//! records, and the one predicate pair that says which files are objects (v4
//! `lib/database/repositories/vault-overlay/inventory-readers.ts`,
//! `4fbdad186`; P4.D324).
//!
//! A file is an object when it sits under `Inventory/` (not `images/`), its
//! JSON parses, its `id` is UUID-shaped and its fields validate
//! ([`parse_inventory_item_file`]). Anything else — a note, a half-written
//! file, a picture — is skipped with a `warn` and is never swept by a
//! projection: [`is_inventory_item_document`] answers false for it, exactly as
//! `is_wardrobe_item_document` does for a note under `Wardrobe/`.
//!
//! Identity is the `id` inside the file, never the path. An instance's file
//! is sparse (only the fields it overrides), and is returned unresolved — the
//! pool resolves it against its archetype ([`crate::inventory::resolve`]).
//!
//! **Fidelity notes.** The text goes through V8's `JSON.parse` twin
//! ([`js_json_parse`] — a duplicate key keeps the LAST value, objects re-keyed
//! in V8's order); a hand-written timestamp through V8's `Date.parse`
//! (`js_date_parse_ms`). The duplicate-id rule compares each file's path
//! case-insensitively against the PROJECTED name of its record: a hand-filed
//! object whose projected name differs only by case from its stored path
//! loses to the first-by-path (ported as-is, recorded).

use serde_json::{Map, Value};

use super::doc_mount_documents::{DocMountDocumentsRepository, VaultFolderDoc};
use super::DbError;
use crate::api::zod_issues::join_path;
use crate::inventory::folders::{
    folder_of_inventory_path, is_inventory_item_document_path, join_folder, INVENTORY_FOLDER,
};
use crate::inventory::types::{
    inventory_shape_issues, js_json_parse, zod_inventory_full_file, zod_inventory_item_file,
    InventoryFields, InventoryRecord, INVENTORY_FILE_KEYS,
};
use crate::vault_overlay::sanitize_file_name;

/// v4 `UUID_RE` — `/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i`.
pub fn is_uuid_shaped(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 36
        && b.iter().enumerate().all(|(i, c)| match i {
            8 | 13 | 18 | 23 => *c == b'-',
            _ => c.is_ascii_hexdigit(),
        })
}

/// The file name (under `Inventory/`) a record is projected to (v4
/// `inventoryFileNameFor`): `title || name || fileStem || object-<id8>`,
/// sanitised, `.json`, inside the record's folder.
pub fn inventory_file_name_for(record: &InventoryRecord) -> String {
    let pick = |v: Option<&str>| v.filter(|s| !s.is_empty()).map(str::to_string);
    let stem = pick(record.title())
        .or_else(|| pick(record.str_of("name")))
        .or_else(|| pick(record.file_stem()))
        .unwrap_or_else(|| {
            let id = record.id();
            format!("object-{}", id.get(..8).unwrap_or(id))
        });
    join_folder(&[
        record.folder(),
        &format!("{}.json", sanitize_file_name(&stem)),
    ])
}

/// The document a parse reads: a stored row, or the predicate's bare
/// `(relativePath, content)` (no mount, no timestamps).
pub struct InventoryDoc<'a> {
    pub relative_path: &'a str,
    pub content: &'a str,
    pub mount_point_id: Option<&'a str>,
    pub created_at: Option<&'a str>,
    pub updated_at: Option<&'a str>,
}

impl<'a> InventoryDoc<'a> {
    pub fn of(doc: &'a VaultFolderDoc) -> Self {
        InventoryDoc {
            relative_path: &doc.relative_path,
            content: &doc.content,
            mount_point_id: Some(&doc.mount_point_id),
            created_at: Some(&doc.created_at),
            updated_at: Some(&doc.updated_at),
        }
    }
}

/// v4 `toIso(value)`: a non-empty string through `new Date(value)` →
/// `toISOString`; NaN → `None`.
pub fn to_iso(value: Option<&str>) -> Option<String> {
    let s = value.filter(|s| !s.is_empty())?;
    crate::episodic::js_date_parse_ms(s).map(crate::clock::iso_from_unix_ms)
}

fn warn(
    doc: &InventoryDoc<'_>,
    quiet: bool,
    message: &str,
    extra: impl FnOnce() -> Vec<(&'static str, String)>,
) {
    if quiet {
        return;
    }
    let extra = extra();
    let field = |k: &str| extra.iter().find(|(n, _)| *n == k).map(|(_, v)| v.as_str());
    // v4's meta: `{mountPointId, relativePath, context, ...meta}`; an
    // undefined `mountPointId` is omitted.
    tracing::warn!(
        target: "quilltap::db",
        mountPointId = doc.mount_point_id,
        relativePath = %doc.relative_path,
        context = "inventory",
        error = field("error"),
        itemId = field("itemId"),
        keysJson = field("keysJson"),
        issues = field("issues"),
        issuesJson = field("issuesJson"),
        "{}",
        message
    );
}

/// Parse one `Inventory/` document into a record, or `None` when it is not
/// an object (v4 `parseInventoryItemFile`). A standalone, archetype or kit is
/// validated whole (defaults filled in); an instance (`parentId` set) is
/// validated sparse, with nothing defaulted, so an omitted field stays
/// omitted and inherits. Unknown top-level keys are dropped with a `warn`;
/// free-form data lives under `metadata`. Missing timestamps fall back to the
/// document's, then the epoch. `quiet` → no logging.
pub fn parse_inventory_item_file(doc: &InventoryDoc<'_>, quiet: bool) -> Option<InventoryRecord> {
    if !is_inventory_item_document_path(doc.relative_path) {
        return None;
    }
    let json = match js_json_parse(doc.content) {
        Ok(v) => v,
        Err(error) => {
            warn(
                doc,
                quiet,
                "Inventory file is not valid JSON; skipped",
                || vec![("error", error)],
            );
            return None;
        }
    };
    let Value::Object(raw) = json else {
        warn(
            doc,
            quiet,
            "Inventory file is not a JSON object; skipped",
            Vec::new,
        );
        return None;
    };
    let raw_id = match raw.get("id") {
        Some(Value::String(id)) if is_uuid_shaped(id) => id.clone(),
        _ => {
            warn(
                doc,
                quiet,
                "Inventory file has no UUID-shaped id; skipped",
                Vec::new,
            );
            return None;
        }
    };

    let mut cleaned = Map::new();
    let mut unknown: Vec<String> = Vec::new();
    for (k, v) in &raw {
        if INVENTORY_FILE_KEYS.contains(&k.as_str()) {
            cleaned.insert(k.clone(), v.clone());
        } else {
            unknown.push(k.clone());
        }
    }
    if !unknown.is_empty() {
        warn(
            doc,
            quiet,
            "Inventory file carries reserved-namespace keys outside metadata; dropped",
            || {
                vec![
                    ("itemId", raw_id.clone()),
                    (
                        "keysJson",
                        serde_json::to_string(&unknown).unwrap_or_default(),
                    ),
                ]
            },
        );
    }
    // `cleaned.createdAt ??= toIso(doc.createdAt)` — `??=` assigns over null
    // and undefined alike (an undefined result leaves the key absent).
    let nullish = |m: &Map<String, Value>, k: &str| m.get(k).is_none_or(Value::is_null);
    if nullish(&cleaned, "createdAt") {
        if let Some(iso) = to_iso(doc.created_at) {
            cleaned.insert("createdAt".into(), Value::String(iso));
        }
    }
    if nullish(&cleaned, "updatedAt") {
        match to_iso(doc.updated_at) {
            Some(iso) => {
                cleaned.insert("updatedAt".into(), Value::String(iso));
            }
            None => {
                if let Some(c) = cleaned.get("createdAt").cloned() {
                    cleaned.insert("updatedAt".into(), c);
                }
            }
        }
    }
    if nullish(&cleaned, "createdAt") {
        cleaned.insert(
            "createdAt".into(),
            Value::String("1970-01-01T00:00:00.000Z".into()),
        );
    }
    if nullish(&cleaned, "updatedAt") {
        let c = cleaned.get("createdAt").cloned().unwrap_or(Value::Null);
        cleaned.insert("updatedAt".into(), c);
    }

    let is_instance = cleaned
        .get("parentId")
        .and_then(Value::as_str)
        .is_some_and(|s| !s.is_empty());
    let input = Value::Object(cleaned);
    let parsed = if is_instance {
        zod_inventory_item_file(&input)
    } else {
        zod_inventory_full_file(&input)
    };
    let file = match parsed {
        Ok(file) => file,
        Err(issues) => {
            warn(
                doc,
                quiet,
                "Inventory file failed validation; skipped",
                || {
                    let joined = issues
                        .iter()
                        .map(|i| format!("{}: {}", join_path(i.path()), i.message()))
                        .collect::<Vec<_>>()
                        .join("; ");
                    vec![("itemId", raw_id.clone()), ("issues", joined)]
                },
            );
            return None;
        }
    };
    let mut issues = inventory_shape_issues(&file);
    if is_instance && crate::inventory::types::js_truthy(file.get("isArchetype")) {
        issues.push("An instance cannot be an archetype".to_string());
    }
    if !issues.is_empty() {
        let item_id = file
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        warn(
            doc,
            quiet,
            "Inventory file breaks the object shape rules; skipped",
            || {
                vec![
                    ("itemId", item_id),
                    (
                        "issuesJson",
                        serde_json::to_string(&issues).unwrap_or_default(),
                    ),
                ]
            },
        );
        return None;
    }

    let file_name = match doc.relative_path.rfind('/') {
        Some(i) => &doc.relative_path[i + 1..],
        None => doc.relative_path,
    };
    // `fileName.replace(/\.json$/i, '')`.
    let cut = file_name.len().saturating_sub(5);
    let file_stem = match (file_name.get(cut..), file_name.get(..cut)) {
        (Some(tail), Some(head)) if file_name.len() >= 5 && tail.eq_ignore_ascii_case(".json") => {
            head
        }
        _ => file_name,
    };
    let mut record = file;
    record.insert(
        "folder".into(),
        Value::String(folder_of_inventory_path(doc.relative_path)),
    );
    record.insert("fileStem".into(), Value::String(file_stem.to_string()));
    Some(InventoryRecord(record))
}

/// True for a document the Inventory owns (v4 `isInventoryItemDocument`): an
/// object path whose content parses into an object. Quiet. The projection
/// sweeps only these, so a note or a broken file kept under `Inventory/` is
/// never deleted by an Inventory write.
pub fn is_inventory_item_document(relative_path: &str, content: Option<&str>) -> bool {
    parse_inventory_item_file(
        &InventoryDoc {
            relative_path,
            content: content.unwrap_or(""),
            mount_point_id: None,
            created_at: None,
            updated_at: None,
        },
        true,
    )
    .is_some()
}

/// True when an Inventory file carries its own UUID `id` (every object does)
/// (v4 `hasStableInventoryItemId`).
pub fn has_stable_inventory_item_id(content: Option<&str>) -> bool {
    match js_json_parse(content.unwrap_or("")) {
        Ok(Value::Object(o)) => o
            .get("id")
            .and_then(Value::as_str)
            .is_some_and(is_uuid_shaped),
        _ => false,
    }
}

/// The folder's records and the duplicate-id files the projection must leave
/// alone.
pub struct InventoryScan {
    pub records: Vec<InventoryRecord>,
    pub duplicate_paths: Vec<String>,
}

/// v4 `scanInventoryFolder`: every `.json` under the mount's `Inventory/` at
/// any depth, object paths only, sorted by path (`localeCompare`, stable),
/// parsed (non-quiet); two files claiming one id → the one at the object's
/// own projected name wins (case-insensitive), failing that the first by
/// path; the rest WARN `Two Inventory files claim one id; this one is skipped
/// and left alone` and are returned as duplicates. DEBUG `Inventory folder
/// read`.
pub fn scan_inventory_folder(
    mount: &rusqlite::Connection,
    mount_point_id: &str,
) -> Result<InventoryScan, DbError> {
    let docs = DocMountDocumentsRepository::new(mount)
        .find_many_by_mount_points_in_folder_opts_or_empty(
            &[mount_point_id.to_string()],
            INVENTORY_FOLDER,
            ".json",
            true,
        )?;
    let mut object_docs: Vec<&VaultFolderDoc> = docs
        .iter()
        .filter(|d| is_inventory_item_document_path(&d.relative_path))
        .collect();
    object_docs
        .sort_by(|a, b| crate::collation::locale_compare(&a.relative_path, &b.relative_path));
    let mut parsed: Vec<(String, InventoryRecord)> = Vec::new();
    for doc in &object_docs {
        if let Some(record) = parse_inventory_item_file(&InventoryDoc::of(doc), false) {
            parsed.push((doc.relative_path.clone(), record));
        }
    }
    let at_own_name = |path: &str, record: &InventoryRecord| {
        path.to_lowercase()
            == format!("{INVENTORY_FOLDER}/{}", inventory_file_name_for(record)).to_lowercase()
    };
    let mut winner: Vec<(String, String)> = Vec::new(); // (id, path)
    let winner_of = |winner: &Vec<(String, String)>, id: &str| -> Option<String> {
        winner.iter().find(|(i, _)| i == id).map(|(_, p)| p.clone())
    };
    for (path, record) in &parsed {
        if winner_of(&winner, record.id()).is_none() && at_own_name(path, record) {
            winner.push((record.id().to_string(), path.clone()));
        }
    }
    for (path, record) in &parsed {
        if winner_of(&winner, record.id()).is_none() {
            winner.push((record.id().to_string(), path.clone()));
        }
    }
    let mut records = Vec::new();
    let mut duplicate_paths: Vec<String> = Vec::new();
    for (path, record) in parsed {
        let kept = winner_of(&winner, record.id());
        if kept.as_deref() == Some(path.as_str()) {
            records.push(record);
            continue;
        }
        tracing::warn!(
            target: "quilltap::db",
            mountPointId = %mount_point_id,
            relativePath = %path,
            keptPath = kept.as_deref(),
            itemId = %record.id(),
            context = "inventory",
            "Two Inventory files claim one id; this one is skipped and left alone"
        );
        if !duplicate_paths.contains(&path) {
            duplicate_paths.push(path);
        }
    }
    tracing::debug!(
        target: "quilltap::db",
        mountPointId = %mount_point_id,
        documentCount = docs.len(),
        objectCount = records.len(),
        duplicateCount = duplicate_paths.len(),
        op = "read",
        context = "inventory",
        "Inventory folder read"
    );
    Ok(InventoryScan {
        records,
        duplicate_paths,
    })
}

/// Every object record in a mount's `Inventory/`, at any depth, archived
/// included, sorted by path (v4 `readInventoryRecords`).
pub fn read_inventory_records(
    mount: &rusqlite::Connection,
    mount_point_id: &str,
) -> Result<Vec<InventoryRecord>, DbError> {
    Ok(scan_inventory_folder(mount, mount_point_id)?.records)
}

/// The paths of the files in a mount's `Inventory/` that parse as objects
/// but were skipped because an earlier file claims the same id (v4
/// `readDuplicateInventoryPaths`).
pub fn read_duplicate_inventory_paths(
    mount: &rusqlite::Connection,
    mount_point_id: &str,
) -> Result<Vec<String>, DbError> {
    Ok(scan_inventory_folder(mount, mount_point_id)?.duplicate_paths)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc<'a>(path: &'a str, content: &'a str) -> InventoryDoc<'a> {
        InventoryDoc {
            relative_path: path,
            content,
            mount_point_id: Some("mp"),
            created_at: Some("2026-10-20T00:00:00.000Z"),
            updated_at: None,
        }
    }

    #[test]
    fn a_standalone_fills_defaults_and_the_timestamp_ladder() {
        let r = parse_inventory_item_file(
            &doc(
                "Inventory/Sub/Lamp.JSON",
                r#"{"id":"a1000000-0000-4000-8000-0000000000a1","name":"Lamp"}"#,
            ),
            true,
        )
        .unwrap();
        assert_eq!(r.folder(), "Sub");
        assert_eq!(r.file_stem(), Some("Lamp"));
        assert_eq!(r.created_at(), "2026-10-20T00:00:00.000Z");
        assert_eq!(r.updated_at(), "2026-10-20T00:00:00.000Z");
        assert_eq!(inventory_file_name_for(&r), "Sub/Lamp.json");
    }

    #[test]
    fn non_objects_are_not_documents() {
        assert!(!is_inventory_item_document(
            "Inventory/notes.json",
            Some("not json")
        ));
        assert!(!is_inventory_item_document(
            "Inventory/notes.json",
            Some("[]")
        ));
        assert!(!is_inventory_item_document(
            "Inventory/a.json",
            Some(r#"{"id":"x"}"#)
        ));
        assert!(has_stable_inventory_item_id(Some(
            r#"{"id":"A1000000-0000-4000-8000-0000000000A1"}"#
        )));
    }
}
