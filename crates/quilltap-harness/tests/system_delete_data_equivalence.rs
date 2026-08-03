//! P4.9G3 delete-all-data differential (tier-2 DB-state): direct-drives the Rust
//! `api::system_data::delete_data_preview` / `delete_data` over a FRESH copy of
//! the committed three-partition `system-data-*` fixture family per case, and
//! diffs BOTH
//!
//!   (a) the returned `{success, summary}` body (v4's `DeleteSummary` verbatim),
//!   (b) a row-count map over EVERY table in all three partitions
//!       (`main.<t>` / `mount.<t>` / `llm.<t>`, enumerated from `sqlite_master`),
//!
//! against v4's REAL `system/tools?action=delete-data-preview|delete-data` route
//! handlers (which compose the REAL `lib/backup/restore/delete-service.ts`) — the
//! `system-delete-data` oracle.
//!
//! The full count map is the point: it pins the tables the wipe must NOT touch
//! (`instance_settings` — the `delete_data_keeps_instance_settings` case writes a
//! row first so the assertion is a survivor, not a coincidental 0 == 0;
//! `background_jobs`, `users`) as tightly as the ones it must clear, and it is
//! what would catch a v5 repo whose delete cascades differently from v4's.
//!
//! ## One ruled divergence
//!
//! [`ANNOTATION_DIVERGENCE_KEY`] — v5 wipes `conversation_annotations`, v4 wipes
//! it nowhere (dogfood #57). Asserted in both directions on the four cases that
//! actually run the wipe; every other key, and every other case, is compared for
//! equality exactly as before.
//!
//! Generate the oracle (see the .test.ts header), then run:
//!   QT_ORACLE_SYSTEM_DELETE_DATA=/tmp/oracle-system-delete-data.ndjson \
//!     cargo test -p quilltap-harness --test system_delete_data_equivalence -- --nocapture

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

use quilltap_core::api::system_data;
use quilltap_core::api::types::{ErrorKind, Response};
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::db::DbError;
use rusqlite::Connection;
use serde_json::{json, Value};

const TEST_PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
const USER: &str = "e18e05bc-63e8-4539-8a85-719b7a508850";

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../quilltap-web/tests/fixtures")
}

/// A fresh copy of all THREE partitions (the wipe reaches the llm-logs sibling).
fn fresh_db(tag: &str) -> Db {
    let scratch = std::env::temp_dir().join(format!("qt-sysdel-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    let main = scratch.join("main.db");
    let mount = scratch.join("mount.db");
    let llm = scratch.join("llm.db");
    std::fs::copy(fixtures_dir().join("system-data-main.db"), &main).unwrap();
    std::fs::copy(fixtures_dir().join("system-data-mount.db"), &mount).unwrap();
    std::fs::copy(fixtures_dir().join("system-data-llmlogs.db"), &llm).unwrap();
    Db::open(
        DbPaths {
            main,
            mount_index: Some(mount),
            llm_logs: Some(llm),
        },
        TEST_PEPPER,
    )
    .expect("open db")
}

fn status_of(kind: ErrorKind) -> u16 {
    match kind {
        ErrorKind::BadRequest | ErrorKind::Unprocessable => 400,
        ErrorKind::Unauthorized => 401,
        ErrorKind::Forbidden => 403,
        ErrorKind::NotFound => 404,
        ErrorKind::Conflict => 409,
        ErrorKind::Locked => 503,
        // The store-unavailable refusal (P4.23) — also 503 (context.ts:176-205).
        ErrorKind::Unavailable => 503,
        ErrorKind::Internal => 500,
    }
}

fn outcome(resp: &Response) -> (u16, Value) {
    match resp {
        Response::System(v) => (200, v.clone()),
        Response::Error(e) => (status_of(e.kind), json!({ "error": e.message })),
        other => panic!("unexpected response variant: {other:?}"),
    }
}

/// Row counts for every table in one partition, prefixed `<tag>.<table>` — the
/// same enumeration the oracle runs.
fn count_partition(conn: &Connection, tag: &str, out: &mut BTreeMap<String, i64>) {
    let mut stmt = conn
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
        .expect("prepare sqlite_master");
    let names: Vec<String> = stmt
        .query_map([], |r| r.get::<_, String>(0))
        .expect("query tables")
        .map(|r| r.expect("table name"))
        .collect();
    for name in names {
        let n: i64 = conn
            .query_row(&format!("SELECT COUNT(*) FROM \"{name}\""), [], |r| {
                r.get(0)
            })
            .unwrap_or_else(|e| panic!("count {tag}.{name}: {e}"));
        out.insert(format!("{tag}.{name}"), n);
    }
}

fn count_all(db: &Db) -> Value {
    let mut out: BTreeMap<String, i64> = BTreeMap::new();
    db.read_main(|c| {
        count_partition(c, "main", &mut out);
        Ok::<(), DbError>(())
    })
    .expect("count main");
    db.read_mount_index(|c| {
        count_partition(c, "mount", &mut out);
        Ok::<(), DbError>(())
    })
    .expect("count mount");
    db.read_llm_logs(|c| {
        count_partition(c, "llm", &mut out);
        Ok::<(), DbError>(())
    })
    .expect("count llm");
    serde_json::to_value(out).expect("counts to json")
}

const CONFIRM: &str = "DELETE_ALL_MY_DATA";

/// ## ⚠ RULED DIVERGENCE (dogfood #57, 2026-08-03) — v5 wipes `conversation_annotations`
///
/// v4 deletes this table on no path at all: it is absent from
/// `clearFormat3Entities`' main list, `deleteUserData` never collects it, and
/// `chats.repository.delete()` sweeps only the message rows. So "delete all my
/// data" leaves it standing, and a `replace`-mode restore then re-inserts the
/// archive's annotations on top of the survivors — which on a migration-vintage
/// instance (whose DDL carries `UNIQUE(chatId, messageIndex, characterName)`,
/// something `generateDDL` never emits) fails once per row. The 2026-08-03 Part
/// F walk saw it eight times.
///
/// Under the standing 2026-08-03 ruling — in backup/restore/import/export v5
/// FIXES v4's bugs rather than reproducing them — v5 truncates the table
/// (`services::delete_all::V5_EXTRA_MAIN_TABLES`). The v4-side repair is queued
/// on the post-5.0 v4-first list.
///
/// **Asserted in BOTH directions** by [`check_annotation_divergence`]: on every
/// case that actually runs the wipe, rust must be 0 AND the oracle must be
/// non-zero. If v4 ever converges, this test fails and tells the next lane to
/// delete the entry rather than silently agreeing.
///
/// The key is excluded from the general count-map comparison below; nothing else
/// is.
const ANNOTATION_DIVERGENCE_KEY: &str = "main.conversation_annotations";

/// The cases whose wipe actually runs, i.e. where the divergence must be live.
/// The other three (`pristine_counts`, `delete_data_preview`,
/// `delete_data_wrong_confirm`) write nothing, so both sides must AGREE there —
/// and they are checked for equality like any other key.
const WIPING_CASES: &[&str] = &[
    "delete_data",
    "delete_data_twice",
    "delete_data_keeps_instance_settings",
    "delete_data_preview_after_wipe",
];

#[test]
fn system_delete_data_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_SYSTEM_DELETE_DATA") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_ORACLE_SYSTEM_DELETE_DATA to the oracle NDJSON (see test header)."
            );
            return;
        }
    };
    let mut oracle: HashMap<String, Value> = HashMap::new();
    for line in std::fs::read_to_string(&oracle_path)
        .unwrap_or_else(|e| panic!("read oracle: {e}"))
        .lines()
        .filter(|l| !l.trim().is_empty())
    {
        let v: Value = serde_json::from_str(line).expect("parse oracle line");
        oracle.insert(v["name"].as_str().unwrap().to_string(), v);
    }

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    let mut failed: Vec<String> = Vec::new();
    let mut ran = 0usize;

    let mut check = |name: &str, status: u16, body: Value, counts: Value| {
        let Some(exp) = oracle.get(name) else {
            failed.push(format!("{name}: MISSING from oracle"));
            return;
        };
        let exp_status = exp["status"].as_u64().unwrap() as u16;
        if status != exp_status {
            failed.push(format!(
                "{name}: status {status} != oracle {exp_status}\n  rust body: {body}"
            ));
            return;
        }
        if body != exp["body"] {
            failed.push(format!(
                "{name}: body mismatch\n  rust:   {body}\n  oracle: {}",
                exp["body"]
            ));
            return;
        }
        // Compare over the UNION of both key sets, an absent table counting as 0.
        // v4 lazily `ensureCollection`s a repo's table the first time it is
        // touched, so its wipe SIDE-EFFECT-CREATES empty `prompt_templates` /
        // `wardrobe_items` tables the committed fixture never had; v5 never
        // creates schema outside provisioning. An empty table that exists on one
        // side only is that artifact, not state — a table with ROWS on one side
        // still fails (which is what `delete_data_keeps_instance_settings`
        // deliberately arranges for `instance_settings`).
        let got = counts.as_object().unwrap();
        let want = exp["counts"].as_object().unwrap();
        let zero = json!(0);
        let mut diffs: Vec<String> = Vec::new();
        // The one ruled divergence, asserted in both directions instead of for
        // equality — see `ANNOTATION_DIVERGENCE_KEY`.
        if WIPING_CASES.contains(&name) {
            let rust = got.get(ANNOTATION_DIVERGENCE_KEY).unwrap_or(&zero);
            let oracle = want.get(ANNOTATION_DIVERGENCE_KEY).unwrap_or(&zero);
            if rust != &zero {
                diffs.push(format!(
                    "    {ANNOTATION_DIVERGENCE_KEY}: v5 must WIPE it (rust {rust}, expected 0) \
                     — see ANNOTATION_DIVERGENCE_KEY"
                ));
            }
            if oracle == &zero {
                diffs.push(format!(
                    "    {ANNOTATION_DIVERGENCE_KEY}: v4 has CONVERGED (oracle 0) — the ruled \
                     divergence is over; delete ANNOTATION_DIVERGENCE_KEY and let this key be \
                     compared for equality again"
                ));
            }
        }
        for (k, v) in want {
            if WIPING_CASES.contains(&name) && k == ANNOTATION_DIVERGENCE_KEY {
                continue;
            }
            let g = got.get(k).unwrap_or(&zero);
            if g != v {
                diffs.push(format!("    {k}: rust {g} != oracle {v}"));
            }
        }
        for (k, g) in got {
            if !want.contains_key(k) && g != &zero {
                diffs.push(format!("    {k}: EXTRA in rust ({g})"));
            }
        }
        if !diffs.is_empty() {
            failed.push(format!("{name}: count map mismatch\n{}", diffs.join("\n")));
            return;
        }
        ran += 1;
        eprintln!("OK {name}");
    };

    // 1. The pristine fixture's counts (the preview baseline).
    {
        let db = fresh_db("pristine");
        check("pristine_counts", 0, Value::Null, count_all(&db));
    }

    // 2. Preview — counts only, and the DB must be untouched.
    {
        let db = fresh_db("preview");
        let (s, b) = outcome(&system_data::delete_data_preview(&db, USER));
        check("delete_data_preview", s, b, count_all(&db));
    }

    // 3. The wrong-sentinel refusal — 400, and nothing written.
    {
        let db = fresh_db("badconfirm");
        let (s, b) = outcome(&rt.block_on(system_data::delete_data(&db, USER, "nope")));
        check("delete_data_wrong_confirm", s, b, count_all(&db));
    }

    // 4. The full delete.
    {
        let db = fresh_db("delete");
        let (s, b) = outcome(&rt.block_on(system_data::delete_data(&db, USER, CONFIRM)));
        check("delete_data", s, b, count_all(&db));
    }

    // 5. Idempotence — the second run summarizes zeros and changes nothing.
    {
        let db = fresh_db("twice");
        let _ = rt.block_on(system_data::delete_data(&db, USER, CONFIRM));
        let (s, b) = outcome(&rt.block_on(system_data::delete_data(&db, USER, CONFIRM)));
        check("delete_data_twice", s, b, count_all(&db));
    }

    // 6. `instance_settings` survives the wipe (a row is WRITTEN first).
    {
        let db = fresh_db("instset");
        let _ = rt.block_on(system_data::job_concurrency_set(&db, 7));
        let (s, b) = outcome(&rt.block_on(system_data::delete_data(&db, USER, CONFIRM)));
        check("delete_data_keeps_instance_settings", s, b, count_all(&db));
    }

    // 7. Preview on an already-wiped instance: all zeros, still no writes.
    {
        let db = fresh_db("previewafter");
        let _ = rt.block_on(system_data::delete_data(&db, USER, CONFIRM));
        let (s, b) = outcome(&system_data::delete_data_preview(&db, USER));
        check("delete_data_preview_after_wipe", s, b, count_all(&db));
    }

    assert!(
        failed.is_empty(),
        "{} case(s) failed:\n{}",
        failed.len(),
        failed.join("\n")
    );
    assert_eq!(ran, 7, "expected 7 cases to run, ran {ran}");
}
