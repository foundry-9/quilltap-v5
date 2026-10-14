//! Tier-2 differential (mocked embedding): the HAND EDIT —
//! `quilltap_core::services::admission::update_memory` against v4's REAL
//! `updateMemory` (`lib/memory/admission/update-memory.ts`, `af48f6cc2` Phase
//! B, bug 203) at the `83334b9a5` pin (P4.D290).
//!
//! Both sides run the corpus's `edits` (`harness/oracle/fixtures/
//! admission-tier3.json`) in order on ONE copy of the admission seed pair;
//! the embedding answers come from the oracle's emitted `canned` / `failure`
//! lines (the exact texts v4's real `embeddingTextFor` produced). Per edit:
//! the result (`{ ok, memory }` / `not-found` / `invalid-related` /
//! `embedding-failed`), the embedding calls (text, lexical text, priority —
//! the edit is INTERACTIVE), and the lines `memory:update-memory` owns. After
//! the run: the `memories` table and the holder vaults' `Commonplace/`
//! projection (LR-6: the hot digest's bucket, both buckets when the subject
//! moves; a cold digest and a plain row mirror nothing).
//!
//! Recipe (canonical; the fixture pair is the admission family's):
//!   N=$HOME/.nvm/versions/node/v24.13.1/bin
//!   V5W=${V5W:-$HOME/source/quilltap-v5}
//!   TMPO=/tmp/qt-update-memory-tier2-oracle
//!   rm -rf "$TMPO"; mkdir -p "$TMPO/harness/oracle/cases" "$TMPO/harness/oracle/fixtures"
//!   cp "$V5W/harness/oracle/cases/update-memory-tier2.test.ts" "$TMPO/harness/oracle/cases/"
//!   cp "$V5W/harness/oracle/fixtures/admission-tier3.json" "$TMPO/harness/oracle/fixtures/"
//!   cd ~/source/quilltap-server
//!   rm -f /tmp/qt-admission-main.db /tmp/qt-admission-mount.db
//!   QT_FIXTURE_OUT=/tmp/qt-admission-main.db QT_FIXTURE_MOUNT_OUT=/tmp/qt-admission-mount.db \
//!     PATH=$N:$PATH npx tsx "$V5W/harness/oracle/fixtures/build-admission-fixture.ts"
//!   rm -f /tmp/oracle-update-memory.ndjson
//!   TZ=UTC QT_FIXTURE_ADMISSION_MAIN=/tmp/qt-admission-main.db QT_FIXTURE_ADMISSION_MOUNT=/tmp/qt-admission-mount.db \
//!     QT_ORACLE_OUT=/tmp/oracle-update-memory.ndjson \
//!     PATH=$N:$PATH npx jest --silent --testTimeout=180000 --roots "$PWD" --roots "$TMPO/harness/oracle/cases" -- update-memory-tier2
//! Run:
//!   QT_ORACLE_UPDATE_MEMORY=/tmp/oracle-update-memory.ndjson \
//!   QT_FIXTURE_ADMISSION_MAIN=/tmp/qt-admission-main.db \
//!   QT_FIXTURE_ADMISSION_MOUNT=/tmp/qt-admission-mount.db \
//!     cargo test -p quilltap-harness --test update_memory_tier2_equivalence

mod admission_support;

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use admission_support::*;
use quilltap_core::db::runtime::Db;
use quilltap_core::services::admission::{update_memory, MemoryEditPatch, UpdateMemoryResult};
use quilltap_core::services::memory_time_zone::reset_memory_time_zone_cache_for_tests;
use serde_json::{json, Value};

const HOST_ZONE: &str = "UTC";

/// The corpus patch as v4's `MemoryEditPatch` (an explicit `null` is
/// `Some(None)` on the two nullable ids).
fn patch_of(v: &Value) -> MemoryEditPatch {
    let nullable = |key: &str| match v.get(key) {
        None => None,
        Some(Value::Null) => Some(None),
        Some(x) => Some(x.as_str().map(str::to_string)),
    };
    MemoryEditPatch {
        content: opt_str(v, "content"),
        summary: opt_str(v, "summary"),
        keywords: opt_strings(v, "keywords"),
        tags: opt_strings(v, "tags"),
        importance: v.get("importance").and_then(Value::as_f64),
        about_character_id: nullable("aboutCharacterId"),
        chat_id: nullable("chatId"),
        related_memory_ids: opt_strings(v, "relatedMemoryIds"),
    }
}

/// v4's `UpdateMemoryResult` object.
fn result_json(r: &UpdateMemoryResult) -> Value {
    match r {
        UpdateMemoryResult::Ok { memory } => json!({ "ok": true, "memory": memory }),
        UpdateMemoryResult::NotFound => json!({ "ok": false, "reason": "not-found" }),
        UpdateMemoryResult::InvalidRelated { invalid_ids } => {
            json!({ "ok": false, "reason": "invalid-related", "invalidIds": invalid_ids })
        }
        UpdateMemoryResult::EmbeddingFailed { message } => {
            json!({ "ok": false, "reason": "embedding-failed", "message": message })
        }
    }
}

/// An ISO stamp inside a projected document → `<ts>` (the mirror stamps
/// `updatedAt` with now when the edit passes none, as v4's default does).
fn mask_stamps(s: &str) -> String {
    let re = regex::Regex::new(r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(\.\d{3})?Z").unwrap();
    re.replace_all(s, "<ts>").into_owned()
}

fn project(db: &Db, sql: &str) -> Value {
    db.read_mount_index(|c| {
        let mut stmt = c.prepare(sql)?;
        let n = stmt.column_count();
        let rows: Vec<Value> = stmt
            .query_map([], |r| {
                let mut row = Vec::with_capacity(n);
                for i in 0..n {
                    row.push(match r.get_ref(i)? {
                        rusqlite::types::ValueRef::Null => Value::Null,
                        rusqlite::types::ValueRef::Integer(x) => json!(x),
                        rusqlite::types::ValueRef::Real(x) => json!(x),
                        rusqlite::types::ValueRef::Text(t) => {
                            Value::String(String::from_utf8_lossy(t).into_owned())
                        }
                        rusqlite::types::ValueRef::Blob(b) => Value::String(hex::encode(b)),
                    });
                }
                Ok(Value::Array(row))
            })?
            .collect::<Result<_, _>>()?;
        Ok(Value::Array(rows))
    })
    .expect("project the mount index")
}

const PROJECTIONS: &[(&str, &str)] = &[
    (
        "links",
        "SELECT l.mountPointId, l.relativePath, l.fileName, f.fileType, f.source, d.content, d.plainTextLength
           FROM doc_mount_file_links l JOIN doc_mount_files f ON f.id = l.fileId
           LEFT JOIN doc_mount_documents d ON d.fileId = f.id
          WHERE l.relativePath LIKE 'Commonplace/%' ORDER BY l.relativePath, l.mountPointId",
    ),
    (
        "folders",
        "SELECT f.mountPointId, f.name, f.path FROM doc_mount_folders f WHERE f.path LIKE 'Commonplace%' ORDER BY f.path, f.mountPointId",
    ),
];

#[tokio::test(flavor = "current_thread")]
async fn update_memory_tier2_matches_oracle() {
    let env = |k: &str| std::env::var(k).ok();
    let (Some(oracle_path), Some(main), Some(mount)) = (
        env("QT_ORACLE_UPDATE_MEMORY"),
        env("QT_FIXTURE_ADMISSION_MAIN"),
        env("QT_FIXTURE_ADMISSION_MOUNT"),
    ) else {
        eprintln!("SKIP: set QT_ORACLE_UPDATE_MEMORY and QT_FIXTURE_ADMISSION_MAIN / _MOUNT (see header).");
        return;
    };
    let spec: Value =
        serde_json::from_str(&std::fs::read_to_string(spec_path()).expect("read spec")).unwrap();
    let oracle = oracle_lines(&oracle_path);
    assert!(
        oracle.iter().any(|l| l["kind"] == "dump"),
        "{oracle_path}: no dump — a stale or partial oracle?"
    );

    use tracing_subscriber::layer::SubscriberExt;
    let logs = Arc::new(Mutex::new(Vec::new()));
    let subscriber = tracing_subscriber::registry().with(Capture {
        lines: logs.clone(),
        owned: &["memory:update-memory"],
        prefixes: &[],
    });
    let _guard = tracing::subscriber::set_default(subscriber);

    let dir = tempfile::Builder::new()
        .prefix("qt-update-memory-rust-")
        .tempdir()
        .unwrap();
    let db = Db::open(
        copy_pair(&main, &mount, dir.path()),
        spec["testPepperBase64"].as_str().unwrap(),
    )
    .expect("open fixture copy");
    reset_memory_time_zone_cache_for_tests();
    let provider = provider_from(&oracle);
    let user_id = spec["userId"].as_str().unwrap();

    let mut planted = HashSet::new();
    planted_stamps(&spec, &mut planted);
    let norm = Norm {
        minted: Vec::new(),
        planted,
    };
    let want_edits: Vec<&Value> = oracle.iter().filter(|l| l["kind"] == "edit").collect();
    let edits = spec["edits"].as_array().unwrap();
    assert_eq!(want_edits.len(), edits.len(), "edit count");
    for (edit, want) in edits.iter().zip(want_edits) {
        let name = edit["name"].as_str().unwrap();
        provider.calls.lock().unwrap().clear();
        logs.lock().unwrap().clear();
        let result = update_memory(
            &db,
            &provider,
            edit["characterId"].as_str().unwrap(),
            edit["memoryId"].as_str().unwrap(),
            patch_of(&edit["patch"]),
            user_id,
            HOST_ZONE,
        )
        .await
        .unwrap_or_else(|e| panic!("{name}: {e:?}"));
        let got = norm.value(&result_json(&result), None);
        let wanted = norm.value(&want["result"], None);
        assert!(
            same(&got, &wanted),
            "{name}: result diverged\n  rust:   {got}\n  oracle: {wanted}"
        );
        let want_calls: Vec<Value> = want["embedCalls"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| {
                let mut c = c.clone();
                if c["priority"].is_null() {
                    c["priority"] = json!("interactive");
                }
                c
            })
            .collect();
        assert_eq!(
            *provider.calls.lock().unwrap(),
            want_calls,
            "{name}: embedding calls"
        );
        let want_lines: Vec<String> = want["lines"]
            .as_array()
            .unwrap()
            .iter()
            .map(render_v4)
            .collect();
        assert_eq!(*logs.lock().unwrap(), want_lines, "{name}: lines");
    }

    // The memories table (no minted ids: an edit mints nothing).
    let dump = db
        .read_main(|c| {
            let mut stmt = c.prepare("SELECT * FROM memories ORDER BY content, id")?;
            let cols: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
            let rows: Vec<Value> = stmt
                .query_map([], |r| {
                    let mut m = serde_json::Map::new();
                    for (i, col) in cols.iter().enumerate() {
                        m.insert(
                            col.clone(),
                            match r.get_ref(i)? {
                                rusqlite::types::ValueRef::Null => Value::Null,
                                rusqlite::types::ValueRef::Integer(x) => json!(x),
                                rusqlite::types::ValueRef::Real(x) => json!(x),
                                rusqlite::types::ValueRef::Text(t) => {
                                    Value::String(String::from_utf8_lossy(t).into_owned())
                                }
                                rusqlite::types::ValueRef::Blob(b) => Value::String(hex::encode(b)),
                            },
                        );
                    }
                    Ok(Value::Object(m))
                })?
                .collect::<Result<_, _>>()?;
            Ok(json!({ "columns": cols, "rows": rows }))
        })
        .expect("dump memories");
    let want_dump = oracle.iter().find(|l| l["kind"] == "dump").unwrap();
    assert_eq!(dump["columns"], want_dump["columns"], "memories columns");
    let (got_rows, want_rows) = (
        dump["rows"].as_array().unwrap(),
        want_dump["rows"].as_array().unwrap(),
    );
    assert_eq!(got_rows.len(), want_rows.len(), "memories row count");
    for (g, w) in got_rows.iter().zip(want_rows) {
        let (g, w) = (norm.row(g), norm.row(w));
        assert!(
            same(&g, &w),
            "memories row diverged\n  rust:   {g}\n  oracle: {w}"
        );
    }

    // The Commonplace mirror (LR-6).
    for (name, sql) in PROJECTIONS {
        let mask =
            |v: &Value| -> Value { serde_json::from_str(&mask_stamps(&v.to_string())).unwrap() };
        let got = mask(&project(&db, sql));
        let want = mask(
            &oracle
                .iter()
                .find(|l| l["kind"] == "projection" && l["name"] == *name)
                .unwrap()["rows"],
        );
        assert!(
            same(&got, &want),
            "Commonplace projection {name}\n  rust:   {got}\n  oracle: {want}"
        );
        assert!(
            !got.as_array().unwrap().is_empty(),
            "{name}: the projection is empty — the mirror never ran?"
        );
    }
    drop(db);
    eprintln!(
        "OK: update_memory tier-2 matched the oracle ({} edits).",
        edits.len()
    );
}
