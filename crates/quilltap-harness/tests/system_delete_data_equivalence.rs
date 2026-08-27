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
//! ## `conversation_annotations` — CONVERGED (bug 10)
//!
//! v5 wiped `conversation_annotations` on delete-all first (dogfood #57); v4 has
//! since adopted it (`3bb664f0`), so this key is now a plain equality like every
//! other. The whole count map, including it, is compared for equality.
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

/// ## `conversation_annotations` — the delete-all wipe (bug 10, v4 CONVERGED)
///
/// v4 used to delete this table on no path at all: it was absent from
/// `clearFormat3Entities`' main list, `deleteUserData` never collected it, and
/// `chats.repository.delete()` swept only the message rows. So "delete all my
/// data" left it standing, and a `replace`-mode restore then re-inserted the
/// archive's annotations on top of the survivors — which on a migration-vintage
/// instance (whose DDL carries `UNIQUE(chatId, messageIndex, characterName)`,
/// something `generateDDL` never emits) failed once per row (dogfood #57, seen
/// eight times on the 2026-08-03 Part F walk).
///
/// v5 truncated the table (`services::delete_all::V5_EXTRA_MAIN_TABLES`) under
/// the standing 2026-08-03 ruling; **v4 has since CONVERGED** (`3bb664f0`, bug
/// 10: `conversation_annotations` added to `clearFormat3Entities`' `mainTables`),
/// so both sides now zero it on delete and the count is a PLAIN equality like
/// every other key.

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
        // `main.conversation_annotations` is a PLAIN equality since bug 10
        // (`3bb664f0`): v4 adopted the wipe this port made first
        // (`clearFormat3Entities` now clears it), so both sides zero it on delete.
        for (k, v) in want {
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
        let (s, b) = outcome(&rt.block_on(system_data::delete_data(&db, USER, "nope", None)));
        check("delete_data_wrong_confirm", s, b, count_all(&db));
    }

    // 4. The full delete.
    {
        let db = fresh_db("delete");
        let (s, b) = outcome(&rt.block_on(system_data::delete_data(&db, USER, CONFIRM, None)));
        check("delete_data", s, b, count_all(&db));
    }

    // 5. Idempotence — the second run summarizes zeros and changes nothing.
    {
        let db = fresh_db("twice");
        let _ = rt.block_on(system_data::delete_data(&db, USER, CONFIRM, None));
        let (s, b) = outcome(&rt.block_on(system_data::delete_data(&db, USER, CONFIRM, None)));
        check("delete_data_twice", s, b, count_all(&db));
    }

    // 6. `instance_settings` survives the wipe (a row is WRITTEN first).
    {
        let db = fresh_db("instset");
        let _ = rt.block_on(system_data::job_concurrency_set(&db, 7));
        let (s, b) = outcome(&rt.block_on(system_data::delete_data(&db, USER, CONFIRM, None)));
        check("delete_data_keeps_instance_settings", s, b, count_all(&db));
    }

    // 7. P4.D65 (the banked P4.D63 unit-8 arm): the archive-bundle SWEEP.
    //
    // The KEPT leg was already covered by every wipe case above — the committed
    // fixture carries one ARCHIVE-category `files` row, which is why
    // `main.files` lands on 1 rather than 0. What no case reached is the
    // explicit opt-in to destroy it, so a port that kept ARCHIVE rows
    // unconditionally (ignoring the option entirely) passed. Here `main.files`
    // must reach ZERO. The explicit-TRUE case beside it is what would catch a
    // port that read the flag but inverted it.
    {
        let db = fresh_db("sweeparch");
        let (s, b) =
            outcome(&rt.block_on(system_data::delete_data(&db, USER, CONFIRM, Some(false))));
        check("delete_data_sweeps_archive_bundles", s, b, count_all(&db));
    }
    {
        let db = fresh_db("keeparch");
        let (s, b) =
            outcome(&rt.block_on(system_data::delete_data(&db, USER, CONFIRM, Some(true))));
        check(
            "delete_data_keeps_archive_bundles_explicitly",
            s,
            b,
            count_all(&db),
        );
    }

    // 8. Preview on an already-wiped instance: all zeros, still no writes.
    {
        let db = fresh_db("previewafter");
        let _ = rt.block_on(system_data::delete_data(&db, USER, CONFIRM, None));
        let (s, b) = outcome(&system_data::delete_data_preview(&db, USER));
        check("delete_data_preview_after_wipe", s, b, count_all(&db));
    }

    assert!(
        failed.is_empty(),
        "{} case(s) failed:\n{}",
        failed.len(),
        failed.join("\n")
    );
    assert_eq!(ran, 9, "expected 9 cases to run, ran {ran}");
}

// ── P4.D126 unit 1: the full-wipe deletion chokepoint (v4 `914b59e13`) ───────

/// v4's full-wipe path used to delete memories with per-row
/// `repos.memories.delete` calls — the last direct bypass of
/// `deleteMemoriesWithUnlinkBatch`. `914b59e13` collects every doomed id and
/// makes ONE batch call; v4's own regression test asserts the direct repository
/// delete is never hit on this path.
///
/// **The count-map differential above is BLIND to this by design.** Both
/// routings delete exactly the same rows, so `main.memories` lands on the same
/// number either way and the oracle can never tell them apart. v4 pins the
/// routing by mocking the repository; Rust has no repository to mock, so the pin
/// here is BEHAVIOURAL, over the one observable the chokepoint adds:
///
/// **the neighbour scrub.** `delete_many_with_unlink` rewrites every SURVIVING
/// row's `relatedMemoryIds` to drop the doomed ids. In the full-wipe case that
/// is normally a no-op (every neighbour is itself doomed — v4's why-comment),
/// but a memory whose `characterId` belongs to no character of this user is not
/// collected, so it survives the wipe *and* is a neighbour. Under the old
/// per-row loop its edge to a deleted memory is left dangling; under the
/// chokepoint it is scrubbed.
///
/// Mutation-proven: reverting `delete_all.rs` to the per-row
/// `memories.delete(&memory_id)` loop leaves the survivor's
/// `relatedMemoryIds` at `["<doomed>"]` and this test fails.
///
/// Runs unconditionally — no env var, so it can never silently skip.
#[test]
fn delete_all_routes_memory_deletion_through_the_chokepoint() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    let db = fresh_db("chokepoint");

    // The fixture's own character (owned by USER) and a memory on it — doomed.
    let doomed = "dd000000-0000-4000-8000-0000000000d1";
    // A memory on a character this user does not own, so the wipe never collects
    // it: it SURVIVES, and it links to the doomed row.
    let survivor = "dd000000-0000-4000-8000-0000000000d2";
    let foreign_character = "dc000000-0000-4000-8000-0000000000c9";

    let owned_character: String = db
        .read_main(|c| {
            c.query_row(
                "SELECT id FROM characters WHERE userId = ?1 LIMIT 1",
                rusqlite::params![USER],
                |r| r.get::<_, String>(0),
            )
            .map_err(DbError::from)
        })
        .expect("the fixture must carry a character for the test user");

    let owned = owned_character.clone();
    db.write_blocking(move |ws| {
        let conn = ws.main().connection();
        let insert = |id: &str, character: &str, related: &str| {
            conn.execute(
                "INSERT INTO memories \
                   (id, characterId, content, summary, relatedMemoryIds, createdAt, updatedAt) \
                 VALUES (?1, ?2, 'x', 'x', ?3, '2026-01-01T00:00:00.000Z', \
                         '2026-01-01T00:00:00.000Z')",
                rusqlite::params![id, character, related],
            )
            .map(|_| ())
        };
        insert(doomed, &owned, "[]")?;
        insert(survivor, foreign_character, &format!("[\"{doomed}\"]"))?;
        Ok(())
    })
    .expect("seed the chokepoint rows");

    let (status, body) = outcome(&rt.block_on(system_data::delete_data(&db, USER, CONFIRM, None)));
    assert_eq!(status, 200, "delete_data refused: {body}");

    let (still_there, related): (i64, String) = db
        .read_main(|c| {
            let n: i64 = c.query_row(
                "SELECT COUNT(*) FROM memories WHERE id = ?1",
                rusqlite::params![survivor],
                |r| r.get(0),
            )?;
            let related: String = c.query_row(
                "SELECT relatedMemoryIds FROM memories WHERE id = ?1",
                rusqlite::params![survivor],
                |r| r.get(0),
            )?;
            Ok::<_, DbError>((n, related))
        })
        .expect("read the survivor back");

    assert_eq!(still_there, 1, "the foreign-character memory must survive");
    let doomed_gone: i64 = db
        .read_main(|c| {
            c.query_row(
                "SELECT COUNT(*) FROM memories WHERE id = ?1",
                rusqlite::params![doomed],
                |r| r.get(0),
            )
            .map_err(DbError::from)
        })
        .expect("count the doomed row");
    assert_eq!(doomed_gone, 0, "the user's own memory must be deleted");

    let parsed: Vec<String> = serde_json::from_str(&related).unwrap_or_default();
    assert!(
        parsed.is_empty(),
        "the wipe must run through delete_many_with_unlink, which scrubs the \
         doomed id out of a surviving neighbour's relatedMemoryIds; found {related}"
    );
}
