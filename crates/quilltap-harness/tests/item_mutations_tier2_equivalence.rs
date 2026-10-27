//! Tier-2 read-back differential for the wardrobe ITEM + FOLDER MUTATIONS
//! (P4.D279 keystone C1 §11): `quilltap_core::services::wardrobe_item_mutations`
//! and `::wardrobe_folder_mutations` vs v4's REAL `lib/wardrobe/item-mutations.ts`
//! (+ `create-body.ts`) and `lib/wardrobe/folder-mutations.ts`, each side over
//! its REAL `resolveWardrobeLocation`.
//!
//! Both sides start from the SAME baked fixture (a character + vault, Quilltap
//! General, four seeded items — `fixtures/item-mutations-tier2.json`), run the
//! corpus `ops` and after EVERY op record `{ op, result | error, items:
//! readItems(true), folders: listFolders(), logs }` — `logs` being the
//! `[ItemMutations]` / `[FolderMutations]` DEBUG lines (message + fields as
//! strings; v5's are captured with `test_support::captured_with`).
//!
//! NORMALISATION (both sides, here): a create without `preserve` mints a uuid —
//! at most ONE new id per op, remapped to `<minted:N>` in op order wherever it
//! occurs (items, componentItemIds, results, log fields); every ISO timestamp
//! that is not one of the spec's `pinnedStamps` → `<ts>`. Items are sorted by
//! their normalised id. Everything else — the `fields` key order, v4's error
//! sentences, counts — exact.
//!
//! RED-FIRST: on main before this lane the two service modules did not exist
//! (`services::wardrobe_item_mutations` / `::wardrobe_folder_mutations` were
//! stubs); this family could not compile, let alone pass.
//!
//! Generate the fixtures + oracle (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_ITEM_MUTATIONS_MAIN=/tmp/qt-itemmut-main.db \
//!   QT_FIXTURE_ITEM_MUTATIONS_MOUNT=/tmp/qt-itemmut-mount.db \
//!     $N/npx tsx $V5W/harness/oracle/fixtures/build-item-mutations-fixture.ts
//!   QT_FIXTURE_ITEM_MUTATIONS_MAIN=/tmp/qt-itemmut-main.db \
//!   QT_FIXTURE_ITEM_MUTATIONS_MOUNT=/tmp/qt-itemmut-mount.db \
//!     $N/npx tsx $V5W/harness/oracle/cases/item-mutations-tier2.ts > /tmp/oracle-item-mutations.ndjson
//! Run:
//!   QT_ORACLE_ITEM_MUTATIONS=/tmp/oracle-item-mutations.ndjson \
//!   QT_FIXTURE_ITEM_MUTATIONS_MAIN=/tmp/qt-itemmut-main.db \
//!   QT_FIXTURE_ITEM_MUTATIONS_MOUNT=/tmp/qt-itemmut-mount.db \
//!     cargo test -p quilltap-harness --test item_mutations_tier2_equivalence

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use quilltap_core::db::doc_mount_file_links::{sha256_of_string, LinkDocumentInput};
use quilltap_core::db::vault_wardrobe_public::WardrobeScope;
use quilltap_core::db::wardrobe_location::{
    resolve_wardrobe_location, ResolveWardrobeLocationOptions, WardrobeLocation,
};
use quilltap_core::db::Writer;
use quilltap_core::services::wardrobe_folder_mutations::{
    create_wardrobe_folder, delete_wardrobe_folder, rename_wardrobe_folder,
};
use quilltap_core::services::wardrobe_item_mutations::{
    component_lookup_for, create_item, move_items_to_folder, set_item_archived,
    update_item_with_origin, validate_component_refs, CreateItemOptions, ItemPatch, PatchOrigin,
    PreservedIdentity, WardrobeCreateBody,
};
use quilltap_core::test_support::captured_with;
use quilltap_core::vault_overlay::WardrobeItem;
use serde::Deserialize;
use serde_json::{json, Map, Value};

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    #[serde(rename = "userId")]
    user_id: String,
    #[serde(rename = "characterId")]
    character_id: String,
    #[serde(rename = "pinnedStamps")]
    pinned_stamps: Vec<String>,
    ops: Vec<Op>,
}

#[derive(Deserialize)]
struct Op {
    op: String,
    scope: String,
    #[serde(default)]
    body: Option<Value>,
    #[serde(default)]
    lookup: Option<bool>,
    #[serde(default)]
    preserve: Option<Value>,
    #[serde(default)]
    target: Option<String>,
    #[serde(default)]
    patch: Option<Value>,
    #[serde(default)]
    archived: Option<bool>,
    #[serde(default, rename = "itemIds")]
    item_ids: Option<Vec<String>>,
    #[serde(default)]
    folder: Option<String>,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    ids: Option<Vec<String>>,
    #[serde(default, rename = "relativePath")]
    relative_path: Option<String>,
    #[serde(default)]
    content: Option<String>,
}

fn opt_str(v: &Value, k: &str) -> Option<String> {
    v.get(k).and_then(Value::as_str).map(str::to_string)
}
fn str_array(v: &Value, k: &str) -> Option<Vec<String>> {
    v.get(k).and_then(Value::as_array).map(|a| {
        a.iter()
            .filter_map(|e| e.as_str().map(str::to_string))
            .collect()
    })
}
/// A nullable string key: absent → `None`; `null` → `Some(None)`; a string →
/// `Some(Some(s))`.
fn nullable(v: &Value, k: &str) -> Option<Option<String>> {
    match v.get(k) {
        None => None,
        Some(Value::Null) => Some(None),
        Some(other) => Some(other.as_str().map(str::to_string)),
    }
}

fn body_from_json(v: &Value) -> WardrobeCreateBody {
    WardrobeCreateBody {
        title: opt_str(v, "title").unwrap_or_default(),
        description: opt_str(v, "description"),
        image_prompt: opt_str(v, "imagePrompt"),
        types: str_array(v, "types").unwrap_or_default(),
        appropriateness: opt_str(v, "appropriateness"),
        is_default: v.get("isDefault").and_then(Value::as_bool),
        component_item_ids: str_array(v, "componentItemIds"),
        replace: v.get("replace").and_then(Value::as_bool),
        folder: opt_str(v, "folder"),
    }
}

fn preserve_from_json(v: &Value) -> PreservedIdentity {
    PreservedIdentity {
        id: opt_str(v, "id").unwrap_or_default(),
        created_at: opt_str(v, "createdAt").unwrap_or_default(),
        updated_at: opt_str(v, "updatedAt").unwrap_or_default(),
        archived_at: nullable(v, "archivedAt"),
        image_file_id: nullable(v, "imageFileId"),
        migrated_from_clothing_record_id: nullable(v, "migratedFromClothingRecordId"),
    }
}

fn patch_from_json(v: &Value) -> ItemPatch {
    ItemPatch {
        title: opt_str(v, "title"),
        types: str_array(v, "types"),
        component_item_ids: str_array(v, "componentItemIds"),
        description: nullable(v, "description"),
        image_prompt: nullable(v, "imagePrompt"),
        appropriateness: nullable(v, "appropriateness"),
        is_default: v.get("isDefault").and_then(Value::as_bool),
        replace: v.get("replace").and_then(Value::as_bool),
        image_file_id: nullable(v, "imageFileId"),
        migrated_from_clothing_record_id: nullable(v, "migratedFromClothingRecordId"),
        folder: opt_str(v, "folder"),
        archived: v.get("archived").and_then(Value::as_bool),
    }
}

fn scope_of(s: &str) -> WardrobeScope {
    match s {
        "character" => WardrobeScope::Character,
        "general" => WardrobeScope::General,
        "project" => WardrobeScope::Project,
        "group" => WardrobeScope::Group,
        other => panic!("unknown scope {other}"),
    }
}

fn item_json(item: &WardrobeItem) -> Value {
    serde_json::to_value(item).expect("item to json")
}

/// A captured tracing line → `{ message, fields }` when its message carries
/// one of the compared prefixes. The capture renders `LEVEL target message
/// k=v k=v` — the `message` field first (a `format_args!`, whose `Debug`
/// prints UNQUOTED), then the fields in record order. The message holds no
/// `=` and every compared field value is space-free, so the first `k=v` token
/// ends the message and the rest split on spaces.
fn parse_log_line(line: &str) -> Option<Value> {
    let start = ["[ItemMutations]", "[FolderMutations]"]
        .iter()
        .filter_map(|p| line.find(&format!(" {p} ")))
        .min()?;
    let rest = &line[start + 1..];
    let mut message_words: Vec<&str> = Vec::new();
    let mut fields = Map::new();
    for tok in rest.split(' ').filter(|s| !s.is_empty()) {
        match tok.split_once('=') {
            Some((k, v)) if !fields.is_empty() || !message_words.is_empty() => {
                fields.insert(k.to_string(), Value::String(v.to_string()));
            }
            _ => {
                assert!(
                    fields.is_empty(),
                    "a space inside a compared field value broke the split: {line}"
                );
                message_words.push(tok);
            }
        }
    }
    Some(json!({ "message": message_words.join(" "), "fields": Value::Object(fields) }))
}

fn logs_from(lines: Vec<String>) -> Value {
    Value::Array(lines.iter().filter_map(|l| parse_log_line(l)).collect())
}

fn fixture_env(name: &str) -> Option<String> {
    match std::env::var(name) {
        Ok(p) => Some(p),
        Err(_) => {
            eprintln!("SKIP: set {name} (see header).");
            None
        }
    }
}

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/item-mutations-tier2.json")
}

// ── Normalisation ──────────────────────────────────────────────────────────

struct Normalizer {
    pinned_stamps: HashSet<String>,
    minted: HashMap<String, String>,
}

fn looks_like_iso(s: &str) -> bool {
    s.len() == 24
        && s.ends_with('Z')
        && s.as_bytes()[4] == b'-'
        && s.as_bytes()[10] == b'T'
        && s.chars().filter(|c| c.is_ascii_digit()).count() == 17
}

impl Normalizer {
    /// Learn this record's minted ids (at most one new per op) from its items.
    fn learn(&mut self, record: &Value, known: &HashSet<String>, op_index: usize) {
        let mut fresh: Vec<String> = Vec::new();
        if let Some(items) = record.get("items").and_then(Value::as_array) {
            for it in items {
                if let Some(id) = it.get("id").and_then(Value::as_str) {
                    if !known.contains(id)
                        && !self.minted.contains_key(id)
                        && !fresh.contains(&id.to_string())
                    {
                        fresh.push(id.to_string());
                    }
                }
            }
        }
        assert!(
            fresh.len() <= 1,
            "op {op_index}: more than one newly minted id in one record: {fresh:?}"
        );
        for id in fresh {
            let n = self.minted.len() + 1;
            self.minted.insert(id, format!("<minted:{n}>"));
        }
    }

    fn apply(&self, v: &mut Value) {
        match v {
            Value::String(s) => {
                if let Some(p) = self.minted.get(s.as_str()) {
                    *s = p.clone();
                } else if looks_like_iso(s) && !self.pinned_stamps.contains(s.as_str()) {
                    *s = "<ts>".to_string();
                }
            }
            Value::Array(a) => a.iter_mut().for_each(|e| self.apply(e)),
            Value::Object(o) => o.values_mut().for_each(|e| self.apply(e)),
            _ => {}
        }
    }
}

/// Canonicalise object KEY order recursively. v4's `location.readItems` emits
/// an item in the file parser's literal order (`componentItemIds` after
/// `replace`); v5's `WardrobeItem` serialises in Zod-schema order (the shape
/// the API answers with). Both are v4's own orders at different seams and the
/// read path is verified elsewhere, so this family compares VALUES: key order
/// is canonicalised, while array order, null-vs-absent and every value stay
/// byte-exact under the `to_string` compare.
fn canonical_keys(v: &mut Value) {
    match v {
        Value::Array(a) => a.iter_mut().for_each(canonical_keys),
        Value::Object(o) => {
            let mut entries: Vec<(String, Value)> = std::mem::take(o).into_iter().collect();
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            for (k, mut val) in entries {
                canonical_keys(&mut val);
                o.insert(k, val);
            }
        }
        _ => {}
    }
}

fn sort_items(record: &mut Value) {
    if let Some(items) = record.get_mut("items").and_then(Value::as_array_mut) {
        items.sort_by(|a, b| {
            a.get("id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .cmp(b.get("id").and_then(Value::as_str).unwrap_or_default())
        });
    }
}

/// Normalise a whole result list in op order (the minted map grows as it goes;
/// each record is normalised with everything learnt up to and including it).
fn normalize_results(results: &mut [Value], pinned: &[String], known: &HashSet<String>) {
    let mut norm = Normalizer {
        pinned_stamps: pinned.iter().cloned().collect(),
        minted: HashMap::new(),
    };
    for (i, record) in results.iter_mut().enumerate() {
        norm.learn(record, known, i);
        norm.apply(record);
        canonical_keys(record);
        sort_items(record);
    }
}

// ── The run ────────────────────────────────────────────────────────────────

#[test]
fn item_mutations_match_oracle() {
    let (Some(oracle_path), Some(main_fixture), Some(mount_fixture)) = (
        fixture_env("QT_ORACLE_ITEM_MUTATIONS"),
        fixture_env("QT_FIXTURE_ITEM_MUTATIONS_MAIN"),
        fixture_env("QT_FIXTURE_ITEM_MUTATIONS_MOUNT"),
    ) else {
        return;
    };

    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(spec_path()).unwrap_or_else(|e| panic!("read spec: {e}")),
    )
    .expect("parse spec");
    let oracle: Value = serde_json::from_str(
        std::fs::read_to_string(&oracle_path)
            .unwrap_or_else(|e| panic!("read oracle: {e}"))
            .trim(),
    )
    .expect("parse oracle");
    let mut want_results: Vec<Value> = oracle
        .get("results")
        .and_then(Value::as_array)
        .expect("oracle has results array")
        .clone();
    assert_eq!(
        want_results.len(),
        spec.ops.len(),
        "oracle op count != corpus"
    );

    let scratch = tempfile::Builder::new()
        .prefix("qt-itemmut-rust-")
        .tempdir()
        .expect("tempdir");
    let main_work = scratch.path().join("main.db");
    let mount_work = scratch.path().join("mount.db");
    std::fs::copy(&main_fixture, &main_work).unwrap_or_else(|e| panic!("copy main: {e}"));
    std::fs::copy(&mount_fixture, &mount_work).unwrap_or_else(|e| panic!("copy mount: {e}"));

    let main = Writer::open_writable(&main_work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open main: {e}"));
    let mount = Writer::open_writable(&mount_work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open mount: {e}"));
    let (mc, xc) = (main.connection(), mount.connection());

    // Every id the baked fixture already carries — the pinned seed ids and the
    // preserved create's — is "known"; anything else a dump shows was minted.
    let mut known: HashSet<String> = HashSet::new();
    for item in resolve(mc, xc, &spec, "character")
        .read_items(xc, true)
        .expect("seed read")
    {
        known.insert(item.id);
    }
    for item in resolve(mc, xc, &spec, "general")
        .read_items(xc, true)
        .expect("seed read")
    {
        known.insert(item.id);
    }
    for op in &spec.ops {
        if let Some(id) = op.preserve.as_ref().and_then(|p| opt_str(p, "id")) {
            known.insert(id);
        }
    }

    let mut got_results: Vec<Value> = Vec::new();
    for op in &spec.ops {
        let location = resolve(mc, xc, &spec, &op.scope);
        let (result, lines) = captured_with(|| run_op(mc, xc, &location, op));
        let items: Vec<Value> = location
            .read_items(xc, true)
            .expect("read-back")
            .iter()
            .map(item_json)
            .collect();
        let folders = serde_json::to_value(location.list_folders(xc).expect("list folders"))
            .expect("folders to json");
        got_results.push(json!({
            "op": op.op,
            "result": result,
            "items": items,
            "folders": folders,
            "logs": logs_from(lines),
        }));
    }
    drop(main);
    drop(mount);

    normalize_results(&mut got_results, &spec.pinned_stamps, &known);
    normalize_results(&mut want_results, &spec.pinned_stamps, &known);

    for (i, want) in want_results.iter().enumerate() {
        let got = &got_results[i];
        assert_eq!(
            serde_json::to_string(got).unwrap(),
            serde_json::to_string(want).unwrap(),
            "op {i} ({}) diverged\n  rust:   {got}\n  oracle: {want}",
            spec.ops[i].op
        );
    }

    eprintln!(
        "OK: item-mutations-tier2 matched oracle ({} ops).",
        spec.ops.len()
    );
}

fn resolve(
    mc: &rusqlite::Connection,
    xc: &rusqlite::Connection,
    spec: &Spec,
    scope: &str,
) -> WardrobeLocation {
    let id = (scope == "character").then_some(spec.character_id.as_str());
    resolve_wardrobe_location(
        mc,
        xc,
        scope_of(scope),
        id,
        &spec.user_id,
        ResolveWardrobeLocationOptions { ensure: true },
    )
    .unwrap_or_else(|e| panic!("resolve {scope}: {e}"))
    .unwrap_or_else(|| panic!("no {scope} location"))
}

fn find_by_title(
    xc: &rusqlite::Connection,
    location: &WardrobeLocation,
    title: &str,
) -> WardrobeItem {
    location
        .read_items(xc, true)
        .expect("read for target")
        .into_iter()
        .find(|i| i.title == title)
        .unwrap_or_else(|| panic!("target not found by title: {title}"))
}

fn err_json<E: std::fmt::Display>(e: E) -> Value {
    json!({ "kind": "error", "message": e.to_string() })
}

fn run_op(
    mc: &rusqlite::Connection,
    xc: &rusqlite::Connection,
    location: &WardrobeLocation,
    op: &Op,
) -> Value {
    match op.op.as_str() {
        "componentLookup" => match component_lookup_for(mc, xc, location, &[]) {
            Ok(lookup) => {
                let mut ids: Vec<&String> = lookup.keys().collect();
                ids.sort();
                json!({ "kind": "ok", "ids": ids })
            }
            Err(e) => err_json(e),
        },
        "validateRefs" => match component_lookup_for(mc, xc, location, &[]) {
            Ok(lookup) => {
                let (components, missing) =
                    validate_component_refs(&lookup, op.ids.as_deref().unwrap_or(&[]));
                let ids: Vec<Value> = components
                    .iter()
                    .map(|c| c.get("id").cloned().unwrap_or(Value::Null))
                    .collect();
                json!({ "kind": "ok", "components": ids, "missing": missing })
            }
            Err(e) => err_json(e),
        },
        "create" => {
            let body = body_from_json(op.body.as_ref().expect("create body"));
            let lookup = match op.lookup {
                Some(true) => match component_lookup_for(mc, xc, location, &[]) {
                    Ok(l) => Some(l),
                    Err(e) => return err_json(e),
                },
                _ => None,
            };
            let preserve = op.preserve.as_ref().map(preserve_from_json);
            match create_item(
                mc,
                xc,
                location,
                &body,
                CreateItemOptions {
                    lookup: lookup.as_ref(),
                    preserve: preserve.as_ref(),
                },
            ) {
                Ok(item) => json!({ "kind": "ok", "item": item_json(&item) }),
                Err(e) => err_json(e),
            }
        }
        "update" => {
            let current = find_by_title(xc, location, op.target.as_deref().expect("target"));
            let lookup = match op.lookup {
                Some(true) => match component_lookup_for(mc, xc, location, &[]) {
                    Ok(l) => Some(l),
                    Err(e) => return err_json(e),
                },
                _ => None,
            };
            let patch = patch_from_json(op.patch.as_ref().expect("patch"));
            // Every corpus patch's keys sit in the same order under either origin
            // (C3); the route's is v4's direct-`updateItem` caller shape here.
            match update_item_with_origin(
                mc,
                xc,
                location,
                &current,
                &patch,
                lookup.as_ref(),
                PatchOrigin::Route,
            ) {
                Ok(updated) => {
                    let cur_archived = current.archived_at.clone().flatten();
                    let upd_archived = updated
                        .as_ref()
                        .and_then(|u| u.archived_at.clone().flatten());
                    json!({
                        "kind": "ok",
                        "found": updated.is_some(),
                        "item": updated.as_ref().map(item_json).unwrap_or(Value::Null),
                        "archivedAtSame": upd_archived == cur_archived,
                        "updatedAtSame": updated.as_ref().map(|u| u.updated_at == current.updated_at).unwrap_or(false),
                    })
                }
                Err(e) => err_json(e),
            }
        }
        "setArchived" => {
            let current = find_by_title(xc, location, op.target.as_deref().expect("target"));
            match set_item_archived(mc, xc, location, &current, op.archived == Some(true)) {
                Ok(outcome) => json!({
                    "kind": "ok",
                    "changed": outcome.changed,
                    "item": outcome.item.as_ref().map(item_json).unwrap_or(Value::Null),
                    "updatedAtSame": outcome.item.as_ref().map(|u| u.updated_at == current.updated_at).unwrap_or(false),
                }),
                Err(e) => err_json(e),
            }
        }
        "move" => match move_items_to_folder(
            mc,
            xc,
            location,
            op.item_ids.as_deref().unwrap_or(&[]),
            op.folder.as_deref().unwrap_or(""),
        ) {
            Ok(moved) => {
                json!({ "kind": "ok", "moved": moved.iter().map(item_json).collect::<Vec<_>>() })
            }
            Err(e) => err_json(e),
        },
        "createFolder" => {
            match create_wardrobe_folder(mc, xc, location, op.path.as_deref().unwrap_or("")) {
                Ok(folder) => json!({ "kind": "ok", "folder": folder }),
                Err(e) => err_json(e),
            }
        }
        "renameFolder" => match rename_wardrobe_folder(
            mc,
            xc,
            location,
            op.path.as_deref().unwrap_or(""),
            op.name.as_deref().unwrap_or(""),
        ) {
            Ok(folder) => json!({ "kind": "ok", "folder": folder }),
            Err(e) => err_json(e),
        },
        "deleteFolder" => {
            match delete_wardrobe_folder(mc, xc, location, op.path.as_deref().unwrap_or("")) {
                Ok((moved_items, moved_folders)) => {
                    json!({ "kind": "ok", "movedItems": moved_items, "movedFolders": moved_folders })
                }
                Err(e) => err_json(e),
            }
        }
        "plantNote" => {
            let rel = op.relative_path.clone().unwrap_or_default();
            let content = op.content.clone().unwrap_or_default();
            let links =
                quilltap_core::db::doc_mount_file_links::DocMountFileLinksRepository::new(xc);
            match links.link_document_content(&LinkDocumentInput {
                mount_point_id: location.mount_point_id.clone(),
                relative_path: rel.clone(),
                file_name: rel.rsplit('/').next().unwrap_or(&rel).to_string(),
                file_type: "markdown".to_string(),
                content_sha256: sha256_of_string(&content),
                plain_text_length: content.encode_utf16().count() as i64,
                file_size_bytes: content.len() as i64,
                content,
                allow_embed: None,
                allow_character_read: None,
                allow_character_write: None,
                last_modified: None,
                created_at: None,
            }) {
                Ok(_) => json!({ "kind": "ok" }),
                Err(e) => err_json(e),
            }
        }
        other => panic!("unknown op {other}"),
    }
}
