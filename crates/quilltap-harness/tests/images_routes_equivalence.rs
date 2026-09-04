//! P4.73 — the `/api/v1/images` COLLECTION route differential: `api::images::*`
//! vs v4's REAL `app/api/v1/images/route.ts` (+ the `[id]` DELETE arm).
//!
//! Both sides run each case on a FRESH copy of the committed images fixture
//! (every seeded id pinned, so reads diff with no remap). Read cases diff
//! status + body with KEY ORDER preserved — the list projection's key sequence
//! is the whole comparand for the omit-null rule (v4 hydrates a NULL column to
//! `undefined` and `JSON.stringify` drops the key, so `width` / `height` /
//! `generationPrompt` / `generationModel` are ABSENT, not null, while `url` —
//! which comes from the route's own ternary — stays an explicit null). Write
//! cases additionally diff the post-mutation `files` + `characters` dumps, so a
//! refusal proves it wrote NOTHING rather than only that it answered 400.
//!
//! Coverage is asserted by SHAPE: every case name the oracle emitted must have
//! been driven here and vice versa (`harness-corpus-shape-constants-rot`).
//!
//! Generate the fixture copy + oracle (Node 24, from the v4 checkout — the full
//! recipe lives in the .test.ts header):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   V5W=${V5W:-$HOME/source/quilltap-v5}
//!   TMPO=/tmp/qt-images-routes-oracle
//!   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
//!   cp "$V5W/harness/oracle/cases/images-routes.test.ts" "$TMPO/cases/"
//!   cp "$V5W/harness/oracle/fixtures/images-collection.json" "$TMPO/fixtures/"
//!   cp "$V5W/crates/quilltap-web/tests/fixtures/images-main.db" /tmp/qt-imgcol-main.db
//!   cp "$V5W/crates/quilltap-web/tests/fixtures/images-mount.db" /tmp/qt-imgcol-mount.db
//!   cp "$V5W/crates/quilltap-web/tests/fixtures/images-main.db.meta.json" /tmp/qt-imgcol-main.db.meta.json
//!   cd ~/source/quilltap-server
//!   TZ=UTC QT_FIXTURE_IMGCOL_MAIN=/tmp/qt-imgcol-main.db QT_FIXTURE_IMGCOL_MOUNT=/tmp/qt-imgcol-mount.db QT_ORACLE_OUT=/tmp/oracle-images-routes.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=180000 \
//!       --roots "$PWD" --roots "$TMPO/cases" -- images-routes
//! Run:
//!   QT_ORACLE_IMAGES_ROUTES=/tmp/oracle-images-routes.ndjson \
//!     cargo test -p quilltap-harness --test images_routes_equivalence -- --nocapture

use std::collections::HashMap;
use std::path::PathBuf;

use serde_json::Value;

use quilltap_core::api::types::{ErrorKind, Response};
use quilltap_core::db::runtime::{Db, DbPaths};

/// v5's `SINGLE_USER_ID` — the fixture's owner.
const USER_A: &str = "11111111-1111-4111-8111-111111111111";

// The pinned ids the fixture bakes (lockstep with the builder + the .test.ts).
const CHAR_TAG: &str = "c1000000-0000-4000-8000-000000000003";
const THEME_TAG: &str = "ee000000-0000-4000-8000-000000000001";
const F_TAGGED: &str = "f0000000-0000-4000-8000-000000000001";
const F_INUSE: &str = "f0000000-0000-4000-8000-000000000005";
const F_ORPHAN: &str = "f0000000-0000-4000-8000-000000000006";
const F_PLAIN: &str = "f0000000-0000-4000-8000-000000000007";
const F_NOKEY_INUSE: &str = "f0000000-0000-4000-8000-00000000000a";
const F_DOC: &str = "f0000000-0000-4000-8000-000000000009";
const MISSING: &str = "f0000000-0000-4000-8000-00000000dead";

#[derive(serde::Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
}

fn spec_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/images-collection.json")
}

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../quilltap-web/tests/fixtures")
}

fn fresh_db(spec: &Spec, tag: &str) -> Db {
    let scratch = std::env::temp_dir().join(format!("qt-imgcol-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    let main = scratch.join("main.db");
    let mount = scratch.join("mount.db");
    std::fs::copy(fixtures_dir().join("images-main.db"), &main).unwrap();
    std::fs::copy(fixtures_dir().join("images-mount.db"), &mount).unwrap();
    Db::open(
        DbPaths {
            main,
            mount_index: Some(mount),
            llm_logs: None,
        },
        &spec.test_pepper_base64,
    )
    .expect("open db")
}

fn status_of(kind: ErrorKind) -> u16 {
    match kind {
        ErrorKind::BadRequest => 400,
        ErrorKind::Unauthorized => 401,
        ErrorKind::Forbidden => 403,
        ErrorKind::NotFound => 404,
        ErrorKind::Conflict => 409,
        ErrorKind::Unprocessable => 422,
        ErrorKind::Locked => 503,
        ErrorKind::Unavailable => 503,
        ErrorKind::Internal => 500,
    }
}

/// Integer-valued floats → integers (JS-number parity). Applied WITHOUT
/// re-sorting keys: this family's whole point is that the key SEQUENCE matches.
fn canon_numbers(v: &mut Value) {
    match v {
        Value::Number(n) => {
            if let Some(f) = n.as_f64() {
                if f.is_finite() && f.fract() == 0.0 && f.abs() < 9.007_199_254_740_992e15 {
                    *v = Value::Number((f as i64).into());
                }
            }
        }
        Value::Array(a) => a.iter_mut().for_each(canon_numbers),
        Value::Object(o) => o.iter_mut().for_each(|(_, x)| canon_numbers(x)),
        _ => {}
    }
}

fn canon(v: &Value) -> Value {
    let mut c = v.clone();
    canon_numbers(&mut c);
    c
}

/// The ordered key sequence of every object in a value, depth-first. A plain
/// value equality would pass on a re-ordered projection because `serde_json`
/// is built with `preserve_order` — this makes the ordering an EXPLICIT
/// assertion rather than an implicit one, so a future map-collecting rewrite
/// cannot silently lose it.
fn key_paths(v: &Value, at: String, out: &mut Vec<(String, Vec<String>)>) {
    match v {
        Value::Object(o) => {
            out.push((at.clone(), o.keys().cloned().collect()));
            for (k, x) in o {
                key_paths(x, format!("{at}.{k}"), out);
            }
        }
        Value::Array(a) => {
            for (i, x) in a.iter().enumerate() {
                key_paths(x, format!("{at}[{i}]"), out);
            }
        }
        _ => {}
    }
}

/// Dump the two tables the write cases mutate, matching the oracle's
/// `dumpTables()` column-for-column.
fn dump_tables(db: &Db) -> Value {
    let files = db
        .read_main(|c| {
            dump_query(
                c,
                "SELECT id, userId, sha256, originalFilename, mimeType, size, width, height, \
                 source, category, linkedTo, tags, description, generationPrompt, \
                 generationModel, generationRevisedPrompt, storageKey, fileStatus \
                 FROM files ORDER BY id",
            )
        })
        .expect("dump files");
    let characters = db
        .read_main(|c| {
            dump_query(
                c,
                "SELECT id, defaultImageId, avatarOverrides FROM characters ORDER BY id",
            )
        })
        .expect("dump characters");
    serde_json::json!({ "files": files, "characters": characters })
}

fn dump_query(conn: &rusqlite::Connection, sql: &str) -> Result<Value, quilltap_core::db::DbError> {
    let mut stmt = conn.prepare(sql)?;
    let cols: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
    let rows = stmt.query_map([], |row| {
        let mut m = serde_json::Map::new();
        for (i, name) in cols.iter().enumerate() {
            let v = match row.get_ref(i)? {
                rusqlite::types::ValueRef::Null => Value::Null,
                rusqlite::types::ValueRef::Integer(x) => Value::from(x),
                // A REAL cell is a JS Number on v4's side; `canon_numbers`
                // collapses the integer-valued ones on BOTH sides afterwards.
                rusqlite::types::ValueRef::Real(f) => Value::from(f),
                rusqlite::types::ValueRef::Text(t) => {
                    Value::String(String::from_utf8_lossy(t).to_string())
                }
                rusqlite::types::ValueRef::Blob(_) => Value::String("<blob>".to_string()),
            };
            m.insert(name.clone(), v);
        }
        Ok(Value::Object(m))
    })?;
    Ok(Value::Array(rows.collect::<Result<Vec<_>, _>>()?))
}

/// What a case produced on the v5 side.
struct Got {
    status: u16,
    body: Value,
    tables: Option<Value>,
}

fn from_response(resp: Response, tables: Option<Value>) -> Got {
    match resp {
        Response::Images(v) => Got {
            status: 200,
            body: v,
            tables,
        },
        // v4's `badRequest(message, details)` puts BOTH keys on the wire; the
        // details-bearing refusals render through `validation_wire_body`
        // exactly as `images_routes.rs`'s edge does. Rendering only `{error}`
        // here would have made the `Image is in use` bag unmeasurable.
        Response::Error(e) => Got {
            status: status_of(e.kind),
            body: e
                .validation_wire_body()
                .unwrap_or_else(|| serde_json::json!({ "error": e.message })),
            tables,
        },
        other => panic!("unexpected response variant: {other:?}"),
    }
}

#[test]
fn images_routes_match_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_IMAGES_ROUTES") else {
        eprintln!("SKIP: set QT_ORACLE_IMAGES_ROUTES (see the test header).");
        return;
    };
    let text = std::fs::read_to_string(&oracle_path).unwrap();
    assert!(
        !text.trim().is_empty(),
        "{oracle_path} is EMPTY — the regen truncated it before failing (ledger §5.1)"
    );
    let mut oracle: HashMap<String, Value> = HashMap::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let v: Value = serde_json::from_str(line).unwrap();
        oracle.insert(v["name"].as_str().unwrap().to_string(), v);
    }

    let spec: Spec =
        serde_json::from_str(&std::fs::read_to_string(spec_path()).unwrap()).expect("spec");

    let mut driven: Vec<String> = Vec::new();
    let mut failed: Vec<String> = Vec::new();

    // ── GET: the list projection ────────────────────────────────────────────
    let list_cases: &[(&str, Option<&str>)] = &[
        ("list_all", None),
        // v4's `if (tagId)` is JS-falsy, so `?tagId=` never filters — the web
        // edge folds the empty value to absent, which is what this drives.
        ("list_tag_empty", None),
        ("list_tag_character", Some(CHAR_TAG)),
        ("list_tag_theme", Some(THEME_TAG)),
        ("list_tag_unmatched", Some(F_TAGGED)),
        // v4 reads `searchParams.get` — FIRST wins, so the edge passes CHAR_TAG.
        ("list_tag_duplicated", Some(CHAR_TAG)),
    ];
    for (name, tag) in list_cases {
        driven.push((*name).to_string());
        let db = fresh_db(&spec, name);
        let got = from_response(
            quilltap_core::api::images::images_list(&db, USER_A, *tag),
            None,
        );
        compare(name, &got, &oracle, &mut failed);
    }

    // ── DELETE ──────────────────────────────────────────────────────────────
    // The storage backend is deliberately the not-configured one: every
    // fixture row is mount-blob keyed, so the existence probe reads
    // `doc_mount_blobs` and never the disk backend — the same dispatch v4's
    // `fileStorageManager` makes. F_INUSE's bytes are really there and
    // F_ORPHAN's key dangles, which is the whole discriminator between the
    // refusal and the cleanup.
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let backend: std::sync::Arc<dyn quilltap_core::services::file_storage::StorageBackend> =
        std::sync::Arc::new(quilltap_core::services::file_storage::NotConfiguredStorageBackend);

    let delete_cases: &[(&str, &str, bool)] = &[
        ("delete_missing", MISSING, false),
        ("delete_wrong_category", F_DOC, false),
        ("delete_in_use", F_INUSE, true),
        ("delete_orphaned_cleanup", F_ORPHAN, true),
        ("delete_ok", F_PLAIN, true),
        // NO storageKey and referenced: v4 skips the probe, so `fileExists`
        // stays FALSE and the orphan branch runs. Without this row a mutation
        // flipping the key-less arm to "bytes present" stayed GREEN.
        ("delete_nokey_in_use", F_NOKEY_INUSE, true),
    ];
    for (name, id, dump) in delete_cases {
        driven.push((*name).to_string());
        let db = fresh_db(&spec, name);
        let resp = rt.block_on(quilltap_core::api::images::image_delete(
            &db,
            backend.clone(),
            USER_A,
            id,
        ));
        // The dump is taken AFTER the write in every case, so a refusal proves
        // it wrote NOTHING rather than only that it answered 400.
        let tables = if *dump { Some(dump_tables(&db)) } else { None };
        let got = from_response(resp, tables);
        compare(name, &got, &oracle, &mut failed);
    }

    assert!(
        failed.is_empty(),
        "images routes FAILED ({}):\n{}",
        failed.len(),
        failed.join("\n")
    );

    // Coverage by SHAPE, both directions — a case added on one side only
    // cannot pass silently (`harness-corpus-shape-constants-rot`).
    let mut driven_sorted = driven.clone();
    driven_sorted.sort();
    let mut oracle_names: Vec<String> = oracle.keys().cloned().collect();
    oracle_names.sort();
    assert_eq!(
        driven_sorted, oracle_names,
        "the driven case list and the oracle's disagree"
    );
}

fn compare(name: &str, got: &Got, oracle: &HashMap<String, Value>, failed: &mut Vec<String>) {
    let Some(want) = oracle.get(name) else {
        failed.push(format!("{name}: MISSING from the oracle"));
        return;
    };
    let want_status = want["status"].as_u64().unwrap_or(200) as u16;
    if got.status != want_status {
        failed.push(format!(
            "{name}: status {} != {want_status} (body {})",
            got.status, got.body
        ));
        return;
    }
    let want_body = canon(&want["body"]);
    let got_body = canon(&got.body);
    if got_body != want_body {
        failed.push(format!(
            "{name}: body\n  want {want_body}\n  got  {got_body}"
        ));
        return;
    }
    // The key SEQUENCE, asserted explicitly (see `key_paths`).
    let (mut wk, mut gk) = (Vec::new(), Vec::new());
    key_paths(&want_body, "$".into(), &mut wk);
    key_paths(&got_body, "$".into(), &mut gk);
    if wk != gk {
        failed.push(format!("{name}: KEY ORDER\n  want {wk:?}\n  got  {gk:?}"));
        return;
    }
    if let Some(tables) = &got.tables {
        let want_tables = canon(&want["tables"]);
        let got_tables = canon(tables);
        if got_tables != want_tables {
            failed.push(format!(
                "{name}: tables\n  want {want_tables}\n  got  {got_tables}"
            ));
        }
    }
}
