//! Tier-2 differential for the wardrobe TRANSFERS service:
//! `quilltap-core::services::wardrobe_transfers::transfer_wardrobe_item` vs v4's
//! REAL `app/api/v1/wardrobe/transfers/route.ts` POST handler.
//!
//! Both sides start from the SAME baked two-DB fixture (a users row; a source
//! character with one wardrobe item in its vault; a destination character holding
//! a collision-seed item; a project + group each with a provisioned official
//! store; a Quilltap General store with one archetype item). Per scenario, both
//! run the same transfer against a fresh COPY of the fixture, then the SEVEN
//! mount-index tables are structural-diffed (doc_mount_points / _files /
//! _documents / _file_links / _folders + project_doc_mount_links +
//! group_doc_mount_links), plus the outcome (`wardrobe_item` + action).
//!
//! REMAP FORM (shared cross-table/cross-db id-map, extended for content-addressed
//! bytes). A COPY writes the item's MINTED uuid + `now` timestamps INTO the
//! wardrobe `.md` frontmatter — so `content`, `contentSha256`, `sha256`, `fileId`
//! all embed values a flat id/timestamp column remap can't align. The normalizer:
//!   1. placeholders every ISO-8601 timestamp (dedicated ts columns AND embedded
//!      in `content`) to `<ts>` — makes a copy's content identical across sides;
//!   2. sorts each table's rows by a post-ts canonical key;
//!   3. walks the tables in a fixed order, remapping every UUID (id columns AND
//!      UUIDs embedded in `content`) through ONE shared first-seen id-map — so the
//!      minted copy id (which is NOT a table-id column, only inside `.md` bytes)
//!      still aligns by first appearance;
//!   4. recomputes `contentSha256` (documents) from the fully-normalized content
//!      and propagates it to the owning file's `sha256` via `fileId` — so the
//!      content-addressed shas agree once the minted bytes are normalized;
//!   5. pins the link `chunkCount` to `<cc>` (a v4-only reindex artifact the Rust
//!      storage primitive does not rebuild) and EXCLUDES `doc_mount_chunks`.
//!
//! FROZEN_NOW is arbitrary: copy timestamps are minted, so they placeholder either
//! way (proving the diff is insensitive to `now`).
//!
//! Generate the fixtures + oracle (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   V5=~/source/quilltap-v5
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_WTR_MAIN=/tmp/qt-wtr-main.db QT_FIXTURE_WTR_MOUNT=/tmp/qt-wtr-mount.db \
//!     $N/node --import tsx $V5/harness/oracle/fixtures/build-wardrobe-transfers-fixture.ts
//!   QT_FIXTURE_WTR_MAIN=/tmp/qt-wtr-main.db QT_FIXTURE_WTR_MOUNT=/tmp/qt-wtr-mount.db \
//!   QT_ORACLE_OUT=/tmp/oracle-wtr.ndjson \
//!     $N/npx jest --silent --watchman=false --roots "$PWD" --roots "$V5/harness/oracle/cases" -- wardrobe-transfers
//! Run:
//!   QT_ORACLE_WTR=/tmp/oracle-wtr.ndjson \
//!   QT_FIXTURE_WTR_MAIN=/tmp/qt-wtr-main.db QT_FIXTURE_WTR_MOUNT=/tmp/qt-wtr-mount.db \
//!     cargo test -p quilltap-harness --test wardrobe_transfers_tier2_equivalence

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use quilltap_core::db::Writer;
use quilltap_core::services::wardrobe_transfers::{
    action_str, transfer_wardrobe_item, DestinationScope, TransferAction, TransferError,
    TransferRequest,
};
use regex::Regex;
use serde::Deserialize;
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

/// Any fixed ISO string — a copy's minted timestamps are placeholdered by the
/// normalizer, so the value is irrelevant to the diff.
const FROZEN_NOW: &str = "2026-05-05T05:05:05.005Z";

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    #[serde(rename = "userId")]
    user_id: String,
    scenarios: Vec<Scenario>,
}

#[derive(Deserialize)]
struct Scenario {
    name: String,
    action: String,
    #[serde(rename = "itemId")]
    item_id: String,
    #[serde(rename = "sourceCharacterId")]
    source_character_id: String,
    #[serde(default, rename = "sourceProjectId")]
    source_project_id: Option<String>,
    destination: Destination,
    expect: String,
    #[serde(default, rename = "expectMessage")]
    expect_message: Option<String>,
}

#[derive(Deserialize)]
struct Destination {
    scope: String,
    #[serde(default)]
    id: Option<String>,
}

/// The mount-index tables dumped after each scenario, in the canonical walk order
/// for the shared id-remap. The order is a DEPENDENCY order: a table whose sort +
/// token identity depends on another's tokens comes after it. `points` first (its
/// tokens key every store), then `folders` and `file_links` (keyed by
/// `mountPointId` + path), then `files`/`documents` (their `fileId` tokens are
/// assigned by the `file_links` walk — NOT by their own content-addressed sha,
/// which a copy's minted id/timestamps legitimately perturb).
struct TableSpec {
    table: &'static str,
    oracle_key: &'static str,
    /// The raw-dump SQL `ORDER BY` column (any stable column — normalization
    /// re-sorts by [`norm_sort_key`], so this only shapes the pre-normalization dump).
    order_by: &'static str,
    id_columns: &'static [&'static str],
    /// Referenced id columns whose ALREADY-ASSIGNED tokens prefix the
    /// normalization sort key (so a referencer sorts by its parent's stable token,
    /// never by a raw uuid). Requires the referenced table to appear earlier.
    sort_ref_cols: &'static [&'static str],
    ts_columns: &'static [&'static str],
    content_column: Option<&'static str>,
    sha_column: Option<&'static str>,
    pin_chunk_count: bool,
}

const TABLES: &[TableSpec] = &[
    TableSpec {
        table: "doc_mount_points",
        oracle_key: "points",
        order_by: "name",
        id_columns: &["id"],
        sort_ref_cols: &[],
        ts_columns: &["createdAt", "updatedAt", "lastScannedAt"],
        content_column: None,
        sha_column: None,
        pin_chunk_count: false,
    },
    TableSpec {
        table: "doc_mount_folders",
        oracle_key: "folders",
        order_by: "path",
        id_columns: &["id", "parentId", "mountPointId"],
        sort_ref_cols: &["mountPointId"],
        ts_columns: &["createdAt", "updatedAt"],
        content_column: None,
        sha_column: None,
        pin_chunk_count: false,
    },
    TableSpec {
        table: "doc_mount_file_links",
        oracle_key: "links",
        order_by: "relativePath",
        id_columns: &["id", "fileId", "folderId", "mountPointId"],
        // Store + path is the stable link identity, so `fileId` tokens (assigned
        // here) survive a copy's sha perturbation.
        sort_ref_cols: &["mountPointId"],
        ts_columns: &[
            "lastModified",
            "descriptionUpdatedAt",
            "createdAt",
            "updatedAt",
        ],
        content_column: None,
        sha_column: None,
        pin_chunk_count: true,
    },
    TableSpec {
        table: "doc_mount_files",
        oracle_key: "files",
        order_by: "sha256",
        id_columns: &["id"],
        // `id` == `fileId`, already tokenized by the `file_links` walk above.
        sort_ref_cols: &["id"],
        ts_columns: &["createdAt", "updatedAt"],
        content_column: None,
        sha_column: Some("sha256"),
        pin_chunk_count: false,
    },
    TableSpec {
        table: "doc_mount_documents",
        oracle_key: "documents",
        order_by: "contentSha256",
        id_columns: &["id", "fileId"],
        sort_ref_cols: &["fileId"],
        ts_columns: &["createdAt", "updatedAt"],
        content_column: Some("content"),
        sha_column: Some("contentSha256"),
        pin_chunk_count: false,
    },
    TableSpec {
        table: "project_doc_mount_links",
        oracle_key: "projectLinks",
        order_by: "createdAt",
        id_columns: &["id", "projectId", "mountPointId"],
        sort_ref_cols: &["mountPointId"],
        ts_columns: &["createdAt", "updatedAt"],
        content_column: None,
        sha_column: None,
        pin_chunk_count: false,
    },
    TableSpec {
        table: "group_doc_mount_links",
        oracle_key: "groupLinks",
        order_by: "createdAt",
        id_columns: &["id", "groupId", "mountPointId"],
        sort_ref_cols: &["mountPointId"],
        ts_columns: &["createdAt", "updatedAt"],
        content_column: None,
        sha_column: None,
        pin_chunk_count: false,
    },
];

fn uuid_re() -> &'static Regex {
    use std::sync::OnceLock;
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}")
            .unwrap()
    })
}

fn iso_re() -> &'static Regex {
    use std::sync::OnceLock;
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z").unwrap())
}

/// Replace every ISO-8601 timestamp anywhere in a string with `<ts>`.
fn placeholder_ts_in(s: &str) -> String {
    iso_re().replace_all(s, "<ts>").into_owned()
}

/// Replace every UUID in a string with a CONSTANT `<uuid>` placeholder. Used for
/// UUIDs embedded inside content-addressed `.md` bytes: a copy echoes its own
/// minted id into the frontmatter, but that id is not a relational FK — and its
/// numbered id-map token would depend on the (per-side-different) raw-sha row
/// order, so a stable constant keeps the normalized content byte-identical across
/// sides (and the recomputed sha with it).
fn blank_uuids_in(s: &str) -> String {
    uuid_re().replace_all(s, "<uuid>").into_owned()
}

fn sha256_hex(s: &str) -> String {
    let mut h = Sha256::new();
    h.update(s.as_bytes());
    hex::encode(h.finalize())
}

// Tiny inline hex encoder (avoid pulling a crate).
mod hex {
    pub fn encode(bytes: impl AsRef<[u8]>) -> String {
        let mut out = String::with_capacity(bytes.as_ref().len() * 2);
        for b in bytes.as_ref() {
            out.push_str(&format!("{b:02x}"));
        }
        out
    }
}

/// Sort rows by the given column's (post-ts-normalization) string value; falls
/// back to the full row's JSON when the column value ties.
/// The normalization sort key: the ALREADY-ASSIGNED tokens of `sort_ref_cols`
/// (falling back to the raw value when not yet mapped), followed by an
/// id-stripped canonical rendering of the row. It never depends on a raw uuid in
/// an id column, so it aligns across sides even when a copied item's minted
/// id/timestamps give two same-path files an identical normalized sha (they'd tie
/// on content and the old `a.to_string()` tie-break leaked the raw id).
fn norm_sort_key(row: &Value, spec: &TableSpec, id_map: &HashMap<String, String>) -> String {
    let empty = serde_json::Map::new();
    let obj = row.as_object().unwrap_or(&empty);
    let mut key = String::new();
    for col in spec.sort_ref_cols {
        let raw = obj.get(*col).and_then(Value::as_str).unwrap_or("");
        key.push_str(id_map.get(raw).map(String::as_str).unwrap_or(raw));
        key.push('\u{1f}');
    }
    let mut rest: Vec<(&String, String)> = obj
        .iter()
        .filter(|(k, _)| !spec.id_columns.contains(&k.as_str()))
        .map(|(k, v)| (k, v.to_string()))
        .collect();
    rest.sort_by(|a, b| a.0.cmp(b.0));
    for (k, v) in rest {
        key.push_str(k);
        key.push('\u{1e}');
        key.push_str(&v);
        key.push('\u{1f}');
    }
    key
}

/// Normalize all seven dumps in place using ONE shared id-map. Phases:
///   1. placeholder ISO timestamps in EVERY string cell (incl. `content`);
///   2. sort each table's rows by its `order_by` (post-ts key);
///   3. walk tables in order, remap UUIDs in id columns AND `content`;
///   4. recompute `contentSha256` from normalized content, propagate to files;
///   5. pin `chunkCount`.
fn normalize_all(dumps: &mut [Value]) {
    // Phase 1: placeholder every ISO timestamp everywhere; blank embedded UUIDs
    // inside content columns to a CONSTANT `<uuid>` (so a copy's frontmatter is
    // byte-identical across sides regardless of the minted id / raw-sha order).
    // Pin `chunkCount` HERE (not last): v4's reindex sets a real count while the
    // Rust port leaves the DDL default 0, and `chunkCount` sorts before `fileName`
    // in the id-stripped sort key — so a live value would perturb which of two
    // same-store files claims the lower `fileId` token.
    for (i, spec) in TABLES.iter().enumerate() {
        if let Some(rows) = dumps[i].get_mut("rows").and_then(Value::as_array_mut) {
            for row in rows.iter_mut() {
                if let Some(obj) = row.as_object_mut() {
                    for (k, v) in obj.iter_mut() {
                        if let Value::String(s) = v {
                            *s = placeholder_ts_in(s);
                            if spec.content_column == Some(k.as_str()) {
                                *s = blank_uuids_in(s);
                            }
                        }
                    }
                    if spec.pin_chunk_count {
                        obj.insert("chunkCount".to_string(), Value::String("<cc>".to_string()));
                    }
                }
            }
        }
    }

    // Phase 2: recompute contentSha256 from the (now fully normalized) content;
    // propagate to the owning file's sha256 via fileId. Do this BEFORE the id
    // remap, keyed on the RAW fileId (still a real uuid here), so both the
    // document's contentSha256 and the file's sha256 line up.
    let mut file_sha_by_raw_id: HashMap<String, String> = HashMap::new();
    for (i, spec) in TABLES.iter().enumerate() {
        let Some(cc) = spec.content_column else {
            continue;
        };
        let sc = spec.sha_column.unwrap();
        let rows = dumps[i]
            .get_mut("rows")
            .and_then(Value::as_array_mut)
            .unwrap();
        for row in rows.iter_mut() {
            let obj = row.as_object_mut().unwrap();
            let content = obj
                .get(cc)
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let recomputed = format!("SHA_{}", sha256_hex(&content));
            if let Some(Value::String(fid)) = obj.get("fileId") {
                file_sha_by_raw_id.insert(fid.clone(), recomputed.clone());
            }
            obj.insert(sc.to_string(), Value::String(recomputed));
        }
    }
    for (i, spec) in TABLES.iter().enumerate() {
        if spec.table != "doc_mount_files" {
            continue;
        }
        let rows = dumps[i]
            .get_mut("rows")
            .and_then(Value::as_array_mut)
            .unwrap();
        for row in rows.iter_mut() {
            let obj = row.as_object_mut().unwrap();
            if let Some(Value::String(id)) = obj.get("id") {
                if let Some(sha) = file_sha_by_raw_id.get(id) {
                    obj.insert("sha256".to_string(), Value::String(sha.clone()));
                }
            }
        }
    }

    // Phase 3+4: interleaved sort-then-remap over the ID COLUMNS only (embedded
    // content UUIDs are already blanked to a constant). Walk tables in DEPENDENCY
    // order so each table's referenced tokens (mountPointId / folderId / fileId)
    // are assigned before it is sorted and remapped. The sort key
    // ([`norm_sort_key`]) never uses a raw id, so first-seen tokens align across
    // sides.
    let mut id_map: HashMap<String, String> = HashMap::new();
    for (i, spec) in TABLES.iter().enumerate() {
        let rows = dumps[i]
            .get_mut("rows")
            .and_then(Value::as_array_mut)
            .unwrap_or_else(|| panic!("{}: no rows array", spec.table));
        rows.sort_by_key(|a| norm_sort_key(a, spec, &id_map));
        for row in rows.iter_mut() {
            let obj = row
                .as_object_mut()
                .unwrap_or_else(|| panic!("{}: row not an object", spec.table));
            for col in spec.id_columns {
                if let Some(Value::String(raw)) = obj.get(*col) {
                    let next = format!("ID_{}", id_map.len());
                    let token = id_map.entry(raw.clone()).or_insert(next).clone();
                    obj.insert((*col).to_string(), Value::String(token));
                }
            }
        }
    }

    // Phase 5: placeholder dedicated ts columns (belt-and-braces — already
    // covered by Phase 1) + pin chunkCount.
    for (i, spec) in TABLES.iter().enumerate() {
        let rows = dumps[i]
            .get_mut("rows")
            .and_then(Value::as_array_mut)
            .unwrap();
        for row in rows.iter_mut() {
            let obj = row.as_object_mut().unwrap();
            for col in spec.ts_columns {
                if obj.get(*col).map(|v| !v.is_null()).unwrap_or(false) {
                    obj.insert((*col).to_string(), Value::String("<ts>".to_string()));
                }
            }
            if spec.pin_chunk_count {
                obj.insert("chunkCount".to_string(), Value::String("<cc>".to_string()));
            }
        }
    }

    // Final re-sort after remap: all referenced ids are now tokens, so
    // `norm_sort_key` falls through to them directly and yields the same stable
    // order on both sides.
    let final_map: HashMap<String, String> = HashMap::new();
    for (i, spec) in TABLES.iter().enumerate() {
        if let Some(rows) = dumps[i].get_mut("rows").and_then(Value::as_array_mut) {
            rows.sort_by(|a, b| {
                norm_sort_key(a, spec, &final_map).cmp(&norm_sort_key(b, spec, &final_map))
            });
        }
    }
}

/// Normalize a `wardrobe_item` object for cross-side comparison: drop null-valued
/// keys (v4's Zod object drops `undefined`; the Rust struct serializes them as
/// explicit `null`), blank UUIDs to a constant `<uuid>` and timestamps to `<ts>`
/// (the item's id/characterId carry no cross-table meaning HERE — the table dumps
/// verify relational identity — and a copy mints them), then SORT keys (v4's
/// field-spread order differs from the Rust struct order; equivalence is
/// order-insensitive).
fn normalize_item(item: &Value) -> Value {
    let Some(obj) = item.as_object() else {
        return item.clone();
    };
    let mut pairs: Vec<(String, Value)> = Vec::new();
    for (k, v) in obj {
        match v {
            Value::Null => continue, // v4 drops undefined optionals
            Value::String(s) => {
                let s = blank_uuids_in(&placeholder_ts_in(s));
                pairs.push((k.clone(), Value::String(s)));
            }
            other => pairs.push((k.clone(), other.clone())),
        }
    }
    pairs.sort_by(|a, b| a.0.cmp(&b.0));
    Value::Object(pairs.into_iter().collect::<Map<String, Value>>())
}

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/wardrobe-transfers-tier2.json")
}

fn action_from_str(s: &str) -> TransferAction {
    match s {
        "move" => TransferAction::Move,
        "copy" => TransferAction::Copy,
        other => panic!("unknown action {other}"),
    }
}

fn scope_from_str(s: &str) -> DestinationScope {
    match s {
        "general" => DestinationScope::General,
        "project" => DestinationScope::Project,
        "group" => DestinationScope::Group,
        "character" => DestinationScope::Character,
        other => panic!("unknown scope {other}"),
    }
}

#[test]
fn wardrobe_transfers_tier2_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_WTR") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_WTR to the oracle NDJSON (see header).");
            return;
        }
    };
    let main_fixture = match std::env::var("QT_FIXTURE_WTR_MAIN") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_FIXTURE_WTR_MAIN to the main fixture .db (see header).");
            return;
        }
    };
    let mount_fixture = match std::env::var("QT_FIXTURE_WTR_MOUNT") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_FIXTURE_WTR_MOUNT to the mount fixture .db (see header).");
            return;
        }
    };

    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(spec_path()).unwrap_or_else(|e| panic!("read spec: {e}")),
    )
    .expect("parse spec");

    // Oracle: one NDJSON line per scenario, keyed by name.
    let oracle_by_name: HashMap<String, Value> = std::fs::read_to_string(&oracle_path)
        .unwrap_or_else(|e| panic!("read oracle: {e}"))
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let v: Value = serde_json::from_str(l).expect("parse oracle line");
            (
                v.get("name").and_then(Value::as_str).unwrap().to_string(),
                v,
            )
        })
        .collect();
    assert_eq!(
        oracle_by_name.len(),
        spec.scenarios.len(),
        "oracle scenario count != corpus"
    );

    for scenario in &spec.scenarios {
        let want = oracle_by_name
            .get(&scenario.name)
            .unwrap_or_else(|| panic!("oracle missing scenario {}", scenario.name));

        // Fresh copies so the shared seed fixtures stay pristine.
        let pid = std::process::id();
        let main_work =
            std::env::temp_dir().join(format!("qt-wtr-main-rust-{pid}-{}.db", scenario.name));
        let mount_work =
            std::env::temp_dir().join(format!("qt-wtr-mount-rust-{pid}-{}.db", scenario.name));
        let _ = std::fs::remove_file(&main_work);
        let _ = std::fs::remove_file(&mount_work);
        std::fs::copy(&main_fixture, &main_work).unwrap_or_else(|e| panic!("copy main: {e}"));
        std::fs::copy(&mount_fixture, &mount_work).unwrap_or_else(|e| panic!("copy mount: {e}"));

        let main = Writer::open_writable(&main_work, &spec.test_pepper_base64)
            .unwrap_or_else(|e| panic!("open main: {e}"));
        let mount = Writer::open_writable(&mount_work, &spec.test_pepper_base64)
            .unwrap_or_else(|e| panic!("open mount: {e}"));

        let req = TransferRequest {
            action: action_from_str(&scenario.action),
            item_id: scenario.item_id.clone(),
            source_character_id: scenario.source_character_id.clone(),
            source_project_id: scenario.source_project_id.clone(),
            destination_scope: scope_from_str(&scenario.destination.scope),
            destination_id: scenario.destination.id.clone(),
        };

        let result = transfer_wardrobe_item(
            main.connection(),
            mount.connection(),
            &spec.user_id,
            &req,
            FROZEN_NOW,
        );

        // Compare the outcome/error tag against the oracle body.
        let want_ok = want.get("ok").and_then(Value::as_bool).unwrap_or(false);
        match (&result, want_ok) {
            (Ok(outcome), true) => {
                let want_body = want.get("body").expect("oracle body");
                let got_item = normalize_item(&outcome.wardrobe_item);
                let want_item =
                    normalize_item(want_body.get("wardrobeItem").expect("oracle wardrobeItem"));
                assert_eq!(
                    got_item, want_item,
                    "scenario {}: wardrobe_item diverged\n  rust:   {got_item}\n  oracle: {want_item}",
                    scenario.name
                );
                assert_eq!(
                    action_str(outcome.action),
                    want_body.get("action").and_then(Value::as_str).unwrap(),
                    "scenario {}: action",
                    scenario.name
                );
                assert_eq!(
                    scenario.expect, "ok",
                    "scenario {} expected ok",
                    scenario.name
                );
            }
            (Err(e), false) => {
                assert_eq!(
                    scenario.expect, "error",
                    "scenario {}: rust errored but corpus expected ok ({e:?})",
                    scenario.name
                );
                if let Some(msg) = &scenario.expect_message {
                    let got_msg = match e {
                        TransferError::BadRequest(m) => m.clone(),
                        TransferError::NotFound => "Wardrobe item".to_string(),
                        TransferError::Internal(m) => m.clone(),
                    };
                    assert_eq!(&got_msg, msg, "scenario {}: error message", scenario.name);
                    // The oracle body carries v4's error text.
                    assert_eq!(
                        want.get("body")
                            .and_then(|b| b.get("error"))
                            .and_then(Value::as_str)
                            .unwrap(),
                        msg,
                        "scenario {}: oracle error text",
                        scenario.name
                    );
                }
            }
            (Ok(_), false) => panic!(
                "scenario {}: rust succeeded but oracle expected an error ({:?})",
                scenario.name,
                want.get("body")
            ),
            (Err(e), true) => panic!(
                "scenario {}: rust errored ({e:?}) but oracle succeeded",
                scenario.name
            ),
        }

        // Dump + diff the seven mount-index tables.
        let mut got: Vec<Value> = TABLES
            .iter()
            .map(|s| {
                mount
                    .dump_table_json(s.table, s.order_by)
                    .unwrap_or_else(|e| panic!("dump {}: {e}", s.table))
            })
            .collect();
        drop(main);
        drop(mount);
        let _ = std::fs::remove_file(&main_work);
        let _ = std::fs::remove_file(&mount_work);

        let mut wanted: Vec<Value> = TABLES
            .iter()
            .map(|s| {
                want.get("tables")
                    .and_then(|t| t.get(s.oracle_key))
                    .cloned()
                    .unwrap_or_else(|| panic!("oracle missing table {}", s.oracle_key))
            })
            .collect();

        normalize_all(&mut got);
        normalize_all(&mut wanted);

        for (i, s) in TABLES.iter().enumerate() {
            assert_eq!(
                got[i]["columns"], wanted[i]["columns"],
                "scenario {} / {}: column set",
                scenario.name, s.table
            );
            assert_eq!(
                got[i]["rows"], wanted[i]["rows"],
                "scenario {} / {}: remapped rows diverged\n  rust:   {}\n  oracle: {}",
                scenario.name, s.table, got[i]["rows"], wanted[i]["rows"]
            );
        }
    }

    eprintln!(
        "OK: wardrobe-transfers tier-2 matched oracle ({} scenarios, 7 tables each).",
        spec.scenarios.len()
    );
}
