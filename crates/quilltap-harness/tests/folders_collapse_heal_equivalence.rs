//! P4.D145 collapse family — v5's `ensure_folders_unique_path_index` boot repair
//! vs v4's REAL `collapse-duplicate-folders-v1` migration (`a5df98b3f`, bug
//! 114). Modelled on `chat_activity_heal_equivalence`.
//!
//! Both sides build the same migration-vintage `folders` table from the shared
//! committed spec (`harness/oracle/fixtures/folders-collapse-heal.json`) — v4's
//! own `collapse-duplicate-folders.test.ts` DDL, with NO unique index, which is
//! also what `generateDDL` emits on both sides (an expression index is
//! inexpressible there, so no D23 re-dump is owed) — run their pass, and the
//! diff covers:
//!
//!   - the whole post-pass `folders` table: the survivor per
//!     `(userId, COALESCE(projectId,''), path)` group (oldest wins, `id ASC`
//!     breaking ties), the repoint of a child whose parent was discarded, and
//!     the deletes. `updatedAt` is normalized to `<repointed>` where it differs
//!     from the row's seeded value (the two apps stamp their own clocks), which
//!     still discriminates: a dropped repoint stamp leaves the seeded value.
//!   - `indexSql` — the `sqlite_master` text of the created index, byte-exact.
//!     **This is the cross-app once-only proof.** v4's `shouldRun()` is
//!     `!indexExists()`, so a v5-collapsed database carrying a byte-identical
//!     index is precisely what makes a later v4 boot skip the migration.
//!   - the `MigrationResult.message` and `itemsAffected`, byte-exact, through
//!     `CollapseOutcome::{message, items_affected}`.
//!   - the second run: v4's `shouldRun()` is false afterwards and a forced
//!     re-run affects nothing; v5 answers `AlreadyIndexed` and touches nothing.
//!   - the probes: inserts attempted after the pass. v4's "the index then
//!     rejects a duplicate insert" case, generalized — including the COALESCE
//!     arm (a NULL `projectId` is ONE value) and the different-project arm that
//!     must still be allowed.
//!   - **the `migrations_state` ledger — a RECORDED DIVERGENCE.** v4's runner
//!     writes a `collapse-duplicate-folders-v1` row because the pass succeeded;
//!     v5 writes NOTHING, because for THIS migration v4's own guard never reads
//!     the ledger (`shouldRun()` is `!indexExists()`). A v5 stamp would claim a
//!     completion v5's guard cannot check, and the index — proven byte-identical
//!     above — is what both apps actually agree on. The oracle's row is asserted
//!     to exist so the divergence stays measured rather than assumed.
//!
//! Generate the oracle (Node 24; jest ignores `.claude/` paths, so the case is
//! staged in a /tmp mirror):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   TMPO=/tmp/qt-folders-collapse-heal-oracle
//!   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
//!   cp "$V5W/harness/oracle/cases/folders-collapse-heal.test.ts" "$TMPO/cases/"
//!   cp "$V5W/harness/oracle/fixtures/folders-collapse-heal.json" "$TMPO/fixtures/"
//!   cd ~/source/quilltap-server
//!   QT_ORACLE_OUT=/tmp/oracle-folders-collapse-heal.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=120000 \
//!       --roots "$PWD" --roots "$TMPO/cases" -- "folders-collapse-heal\.test\.ts$"
//! Run:
//!   QT_ORACLE_FOLDERS_COLLAPSE=/tmp/oracle-folders-collapse-heal.ndjson \
//!     cargo test -p quilltap-harness --test folders_collapse_heal_equivalence

use std::path::PathBuf;

use quilltap_core::db::folders_unique_path_repair::{
    ensure_folders_unique_path_index, CollapseOutcome, FOLDERS_UNIQUE_PATH_INDEX,
};
use rusqlite::Connection;
use serde_json::{json, Value};

/// v5's stamp for the repointed rows. Never equal to a seeded `createdAt`, so
/// the normalizer can tell a stamped row from an untouched one.
const FIXED_NOW: &str = "2026-09-02T12:00:00.000Z";

struct Spec {
    user_id: String,
    scenarios: Vec<Value>,
}

fn spec() -> Spec {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/folders-collapse-heal.json");
    let spec: Value = serde_json::from_str(&std::fs::read_to_string(p).expect("spec")).unwrap();
    Spec {
        user_id: spec["userId"].as_str().expect("userId").to_string(),
        scenarios: spec["scenarios"].as_array().expect("scenarios").clone(),
    }
}

/// v4's own test derivation: the path's last segment, or `Root`.
fn folder_name(path: &str) -> String {
    let trimmed = path.strip_suffix('/').unwrap_or(path);
    match trimmed.rsplit('/').next() {
        Some(last) if !last.is_empty() => last.to_string(),
        _ => "Root".to_string(),
    }
}

/// The migration-vintage table the oracle case and v4's own test build.
fn build_db(scenario: &Value, default_user: &str) -> Connection {
    let db = Connection::open_in_memory().expect("open");
    db.execute_batch(
        "CREATE TABLE \"folders\" (\
           \"id\" TEXT PRIMARY KEY,\
           \"userId\" TEXT NOT NULL,\
           \"path\" TEXT NOT NULL,\
           \"name\" TEXT NOT NULL,\
           \"parentFolderId\" TEXT,\
           \"projectId\" TEXT,\
           \"createdAt\" TEXT NOT NULL,\
           \"updatedAt\" TEXT NOT NULL\
         );",
    )
    .expect("ddl");
    for row in scenario["folders"].as_array().expect("folders") {
        let created = row["createdAt"].as_str().expect("createdAt");
        db.execute(
            "INSERT INTO folders (id, userId, path, name, parentFolderId, projectId, \
               createdAt, updatedAt) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![
                row["id"].as_str().expect("id"),
                row.get("userId")
                    .and_then(Value::as_str)
                    .unwrap_or(default_user),
                row["path"].as_str().expect("path"),
                folder_name(row["path"].as_str().unwrap()),
                row.get("parentFolderId").and_then(Value::as_str),
                row["projectId"].as_str(),
                created,
                created,
            ],
        )
        .expect("seed folder");
    }
    db
}

/// Every `folders` row, `updatedAt` normalized to `<repointed>` when the pass
/// stamped it (the two apps use their own clocks; the seed makes `updatedAt`
/// equal `createdAt`, so any difference IS the stamp).
fn dump_folders(db: &Connection) -> Vec<Value> {
    let mut stmt = db
        .prepare(
            "SELECT id, userId, projectId, path, parentFolderId, createdAt, updatedAt \
             FROM folders ORDER BY id",
        )
        .expect("prep");
    stmt.query_map([], |r| {
        let created: String = r.get(5)?;
        let updated: String = r.get(6)?;
        Ok(json!({
            "id": r.get::<_, String>(0)?,
            "userId": r.get::<_, String>(1)?,
            "projectId": r.get::<_, Option<String>>(2)?,
            "path": r.get::<_, String>(3)?,
            "parentFolderId": r.get::<_, Option<String>>(4)?,
            "createdAt": created.clone(),
            "updatedAt": if updated == created { created } else { "<repointed>".to_string() },
        }))
    })
    .expect("query")
    .collect::<Result<Vec<_>, _>>()
    .expect("rows")
}

/// The oracle's rows through the same normalizer.
fn normalize_oracle_folders(rows: &Value) -> Vec<Value> {
    rows.as_array()
        .expect("folders array")
        .iter()
        .map(|r| {
            let created = r["createdAt"].as_str().expect("createdAt");
            let updated = r["updatedAt"].as_str().expect("updatedAt");
            json!({
                "id": r["id"],
                "userId": r["userId"],
                "projectId": r["projectId"],
                "path": r["path"],
                "parentFolderId": r["parentFolderId"],
                "createdAt": created,
                "updatedAt": if updated == created { created } else { "<repointed>" },
            })
        })
        .collect()
}

fn index_sql(db: &Connection) -> Option<String> {
    db.query_row(
        "SELECT sql FROM sqlite_master WHERE type = 'index' AND name = ?1",
        rusqlite::params![FOLDERS_UNIQUE_PATH_INDEX],
        |r| r.get::<_, Option<String>>(0),
    )
    .ok()
    .flatten()
}

fn table_exists(db: &Connection, name: &str) -> bool {
    db.prepare("SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1")
        .and_then(|mut s| s.exists([name]))
        .unwrap_or(false)
}

#[test]
fn folders_collapse_heal_matches_v4() {
    let Ok(path) = std::env::var("QT_ORACLE_FOLDERS_COLLAPSE") else {
        eprintln!("SKIP: set QT_ORACLE_FOLDERS_COLLAPSE (see the header).");
        return;
    };
    let oracle: Vec<Value> = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read {path}: {e}"))
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("oracle row"))
        .collect();
    let spec = spec();
    assert_eq!(
        oracle.len(),
        spec.scenarios.len(),
        "the oracle and the spec disagree on the scenario count"
    );
    assert!(
        spec.scenarios.len() >= 10,
        "the spec must keep v4's own five cases plus the widened arms and the \
         Friday-shaped one — a shrunk corpus is a stale-oracle tell"
    );

    let mut saw_repoint = false;
    let mut saw_rejecting_probe = false;
    let mut saw_allowed_probe = false;
    let mut saw_collapse = false;

    for (scenario, want) in spec.scenarios.iter().zip(&oracle) {
        let name = scenario["name"].as_str().expect("name");
        assert_eq!(name, want["scenario"].as_str().unwrap(), "scenario order");
        assert_eq!(
            want["completedBefore"].as_bool(),
            Some(false),
            "[{name}] the oracle instance must start unmigrated"
        );
        assert_eq!(
            want["shouldRun"].as_bool(),
            Some(true),
            "[{name}] every scenario starts index-less, so v4 must run"
        );

        let db = build_db(scenario, &spec.user_id);
        let outcome = ensure_folders_unique_path_index(&db, FIXED_NOW).expect("collapse");

        let CollapseOutcome::Ran {
            scanned,
            surviving,
            deleted,
            repointed,
        } = outcome.clone()
        else {
            panic!("[{name}] v4 ran the pass; v5 answered {outcome:?}");
        };
        assert_eq!(
            scanned,
            scenario["folders"].as_array().unwrap().len(),
            "[{name}] every seeded row scanned"
        );
        if deleted > 0 {
            saw_collapse = true;
        }
        if repointed > 0 {
            saw_repoint = true;
        }

        // --- the MigrationResult ---
        let result = &want["result"];
        assert_eq!(
            outcome.items_affected() as u64,
            result["itemsAffected"].as_u64().expect("itemsAffected"),
            "[{name}] itemsAffected"
        );
        assert_eq!(
            outcome.message().as_deref(),
            result["message"].as_str(),
            "[{name}] MigrationResult message, byte-exact"
        );
        assert_eq!(
            surviving + deleted,
            scanned,
            "[{name}] survivors + discards"
        );

        // --- the table ---
        assert_eq!(
            dump_folders(&db),
            normalize_oracle_folders(&want["folders"]),
            "[{name}] folders after the pass"
        );

        // --- the index: the cross-app once-only marker ---
        assert_eq!(
            index_sql(&db).as_deref(),
            want["indexSql"].as_str(),
            "[{name}] the created index, byte-exact — v4's shouldRun() is \
             !indexExists(), so this is what makes a later v4 boot skip"
        );

        // --- the second run ---
        assert_eq!(
            want["shouldRunAfter"].as_bool(),
            Some(false),
            "[{name}] v4 must not want to run again"
        );
        let second = ensure_folders_unique_path_index(&db, FIXED_NOW).expect("second run");
        assert_eq!(
            second,
            CollapseOutcome::AlreadyIndexed,
            "[{name}] v5's guard is the index"
        );
        assert_eq!(
            want["secondRun"]["itemsAffected"].as_u64(),
            Some(0),
            "[{name}] v4's forced re-run affects nothing"
        );
        assert_eq!(
            dump_folders(&db),
            normalize_oracle_folders(&want["foldersAfterSecondRun"]),
            "[{name}] folders after the second run"
        );

        // --- the probes: what the index now rejects ---
        for probe in want["probes"].as_array().expect("probes") {
            let id = probe["id"].as_str().expect("probe id");
            let spec_probe = scenario["probes"]
                .as_array()
                .unwrap()
                .iter()
                .find(|p| p["id"].as_str() == Some(id))
                .unwrap_or_else(|| panic!("[{name}] probe {id} missing from the spec"));
            let probe_path = spec_probe["path"].as_str().expect("path");
            let attempt = db.execute(
                "INSERT INTO folders (id, userId, path, name, parentFolderId, projectId, \
                   createdAt, updatedAt) \
                 VALUES (?1, ?2, ?3, ?4, NULL, ?5, '2026-05-01T00:00:00.000Z', \
                   '2026-05-01T00:00:00.000Z')",
                rusqlite::params![
                    id,
                    spec_probe
                        .get("userId")
                        .and_then(Value::as_str)
                        .unwrap_or(&spec.user_id),
                    probe_path,
                    folder_name(probe_path),
                    spec_probe["projectId"].as_str(),
                ],
            );
            let want_threw = probe["threw"].as_bool().expect("threw");
            assert_eq!(
                attempt.is_err(),
                want_threw,
                "[{name}] probe {id}: v4 {} — v5 {:?}",
                if want_threw {
                    "was rejected"
                } else {
                    "was allowed"
                },
                attempt
            );
            if want_threw {
                saw_rejecting_probe = true;
                let err = attempt.unwrap_err();
                assert!(
                    quilltap_core::db::sqlite_errors::message_names_unique_constraint(
                        &err.to_string()
                    ),
                    "[{name}] probe {id}: a UNIQUE violation, not {err}"
                );
                assert!(
                    want["probes"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|p| p["message"]
                            .as_str()
                            .is_some_and(|m| m.contains("UNIQUE constraint failed"))),
                    "[{name}] v4 reported a UNIQUE violation too"
                );
            } else {
                saw_allowed_probe = true;
            }
        }

        // --- the ledger: a RECORDED DIVERGENCE, measured on both sides ---
        let v4_ledger = want["ledger"].as_array().expect("ledger");
        assert_eq!(
            v4_ledger.len(),
            1,
            "[{name}] v4's runner records the completed migration"
        );
        assert_eq!(
            v4_ledger[0]["id"].as_str(),
            Some("collapse-duplicate-folders-v1"),
            "[{name}] under v4's migration id"
        );
        assert!(
            !table_exists(&db, "migrations_state"),
            "[{name}] v5 stamps NOTHING. v4's own guard for this migration never \
             reads the ledger (shouldRun() is !indexExists()), so the row is \
             informational; a v5 stamp would claim a completion v5's guard \
             cannot check. The index — proven byte-identical above — is what \
             both apps agree on."
        );
        assert!(
            !table_exists(&db, "migrations_metadata"),
            "[{name}] and no metadata either"
        );
    }

    assert!(saw_collapse, "no scenario actually collapsed a duplicate");
    assert!(saw_repoint, "no scenario exercised the parent repoint");
    assert!(
        saw_rejecting_probe && saw_allowed_probe,
        "the probes must cover BOTH a rejected duplicate and an allowed sibling"
    );
    println!(
        "OK: folders-collapse-heal matched v4 across {} scenarios.",
        oracle.len()
    );
}
