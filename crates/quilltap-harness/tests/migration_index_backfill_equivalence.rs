//! P4.160 — the migration-index-family boot backfill against v4.
//!
//! A v5 instance provisioned BEFORE P4.153 lacks v4's migration-created index
//! family (56 of `migration_indexes.json`'s 60 statements after a boot, plus a
//! PLAIN `idx_doc_mount_folders_mp_path`); since P4.160 the host's
//! `seed_built_ins` backfills it (`db::migration_index_family_repair`). Both
//! arms DERIVE such an instance (provision, `DROP` the family, re-plant the
//! plain `mp_path` — no committed pre-round fixture exists) and boot the REAL
//! `Host` over it once.
//!
//!   (A) **The family arm** — the backfilled index set per partition EQUALS a
//!       real v4 first boot's (`QT_ORACLE_PROVISION`, the migrations-first
//!       oracle `build-provision-oracle.ts` builds by driving v4's REAL
//!       `MigrationRunner` over v4's REAL registry), name for name and SQL
//!       byte-equal, modulo the SAME carve-outs `provisioning_equivalence`'s
//!       (1c) names — copied here BY NAME (that file is read-only to this
//!       lane), both-ways. Red-first on unported `main`: 56 absent + the plain
//!       `mp_path` (the lane record). With `QT_V5_BACKFILLED_OUT` set, the
//!       backfilled instance is written there for the order's items 9/10
//!       (`migration-index-backfill.ts` with `QT_V5_BACKFILLED_DIR`, and
//!       `verify-v5-provisioned.ts`).
//!   (B) **The dedupe arm** — v4's REAL `add-connection-profile-unique-name-
//!       index-v1` (the one family script v4 itself would run on such a file),
//!       through `harness/oracle/lib/v4-migrations.ts`, over the same planted
//!       `connection_profiles` rows in five shapes (`migration-index-
//!       backfill.ts`'s header). Per shape: the index's `sqlite_master.sql`
//!       byte-equal (`null` when absent), every row's `(id, userId, name)` and
//!       whether its `updatedAt` was re-stamped, and v4's DEBUG / INFO lines
//!       (its `MigrationLogger.prototype` spy) rendered as v5's rig renders
//!       them, in order. v4's report verdict is asserted per shape, so a shape
//!       cannot silently go vacuous.
//!
//! Every boot logs from the writer and seed threads, so the capture is the
//! process-GLOBAL default and the arms run one at a time behind `SERIAL`.
//!
//! Generate (Node 24, from the pinned v4 worktree):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   QT_ORACLE_PROVISION=/tmp/oracle-provision.json \
//!   QT_V4_FRESH_OUT=/tmp/qt-v4-fresh \
//!     $N/npx tsx $V5W/harness/oracle/provision/build-provision-oracle.ts
//!   QT_FIXTURE_OUT_DIR=/tmp/qt-index-backfill \
//!     $N/npx tsx $V5W/harness/oracle/cases/migration-index-backfill.ts \
//!     > /tmp/oracle-index-backfill.ndjson
//! Run:
//!   QT_ORACLE_PROVISION=/tmp/oracle-provision.json \
//!   QT_ORACLE_INDEX_BACKFILL=/tmp/oracle-index-backfill.ndjson \
//!   QT_V5_BACKFILLED_OUT=/tmp/qt-v5-backfilled \
//!     cargo test -p quilltap-harness --test migration_index_backfill_equivalence -- --nocapture
//! Then the order's items 9/10 over the instance arm A wrote (recorded in the
//! P4.160 survey, not compared here — the last NDJSON line is v4's REAL
//! `MigrationRunner` over a copy of it; the second script opens it with v4's
//! REAL repositories). Both import v4's `@/` alias, so from the v4 checkout:
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT_DIR=/tmp/qt-index-backfill-runner \
//!   QT_V5_BACKFILLED_DIR=/tmp/qt-v5-backfilled \
//!     $N/npx tsx $V5W/harness/oracle/cases/migration-index-backfill.ts \
//!     > /tmp/oracle-index-backfill-runner.ndjson
//!   QT_FIXTURE_V5_PROVISIONED=/tmp/qt-v5-backfilled \
//!     $N/npx tsx $V5W/harness/oracle/provision/verify-v5-provisioned.ts
//!
//! Each arm skips (does not fail) when its env var is unset — the standing
//! gated-differential discipline.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use quilltap_core::db::Writer;
use quilltap_core::services::provisioning::provision_fresh_instance;
use quilltap_core::test_support::CaptureLayer;
use quilltap_host::{Host, HostConfig};
use serde_json::{json, Value};
use tracing_subscriber::layer::SubscriberExt;

/// The oracle's test pepper (`migrations-first.ts`'s `TEST_PEPPER`), so a v4
/// script can open the backfilled instance this test writes out.
const PEPPER: &str = "3q2+796tvu/erb7v3q2+796tvu/erb7v3q2+796tvu8=";
const MAIN: &str = "quilltap.db";
const MOUNT: &str = "quilltap-mount-index.db";
const PARTITIONS: [(&str, &str); 3] = [
    ("main", MAIN),
    ("mountIndex", MOUNT),
    ("llmLogs", "quilltap-llm-logs.db"),
];
const PROFILE_INDEX: &str = "idx_connection_profiles_userId_name";

static SERIAL: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn capture() -> &'static Arc<Mutex<Vec<String>>> {
    static LOGS: OnceLock<Arc<Mutex<Vec<String>>>> = OnceLock::new();
    LOGS.get_or_init(|| {
        let logs = Arc::new(Mutex::new(Vec::new()));
        tracing::subscriber::set_global_default(
            tracing_subscriber::registry().with(CaptureLayer(logs.clone())),
        )
        .expect("this binary owns the global subscriber");
        logs
    })
}

fn opt_env(name: &str) -> Option<PathBuf> {
    std::env::var(name)
        .ok()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
}

fn config(base: &Path) -> HostConfig {
    let mut config = HostConfig::new(base);
    config.instances_path = Some(base.join("instances.json"));
    config.env_pepper = Some(PEPPER.to_string());
    config.autonomous_tick_ms = 3_600_000;
    config.stuck_check_ms = 3_600_000;
    config.terminal = false;
    config.seed_sample_content = false;
    config
}

fn artifact(name: &str, partition: &str) -> Vec<String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../quilltap-core/src/services/provisioning")
        .join(name);
    let v: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    v[partition]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s.as_str().unwrap().to_string())
        .collect()
}

fn index_name(sql: &str) -> Option<String> {
    let rest = sql.strip_prefix("CREATE ")?;
    let rest = rest.strip_prefix("UNIQUE ").unwrap_or(rest);
    let rest = rest.strip_prefix("INDEX ")?;
    Some(
        rest.split([' ', '('])
            .next()
            .unwrap()
            .trim_matches('"')
            .to_string(),
    )
}

fn exec(data: &Path, file: &str, sql: &str) {
    let w = Writer::open_writable(&data.join(file), PEPPER).unwrap();
    w.connection()
        .execute_batch(sql)
        .unwrap_or_else(|e| panic!("plant {sql:?}: {e}"));
}

/// A fresh provision with the migration family taken away again, the plain
/// `mp_path` re-planted — every pre-P4.153 instance's shape.
fn derive_pre_round(data: &Path) {
    provision_fresh_instance(data, PEPPER).expect("provision");
    for (partition, file) in PARTITIONS {
        let drops: String = artifact("migration_indexes.json", partition)
            .iter()
            .map(|sql| format!("DROP INDEX \"{}\";", index_name(sql).unwrap()))
            .collect();
        exec(data, file, &drops);
    }
    let plain = artifact("fresh_schema.json", "mountIndex")
        .into_iter()
        .find(|sql| index_name(sql).as_deref() == Some("idx_doc_mount_folders_mp_path"))
        .unwrap();
    exec(data, MOUNT, &plain);
}

/// Derive, plant, boot ONCE; answer the data dir (kept alive by the TempDir)
/// and the boot's captured lines.
fn boot_pre_round(plants: &[String]) -> (tempfile::TempDir, PathBuf, Vec<String>) {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    derive_pre_round(&data);
    for sql in plants {
        exec(&data, MAIN, sql);
    }
    capture().lock().unwrap().clear();
    let host = Host::start(config(dir.path())).expect("the backfill boot");
    let lines = capture().lock().unwrap().clone();
    drop(host);
    (dir, data, lines)
}

fn index_sql(conn: &rusqlite::Connection) -> BTreeMap<String, String> {
    let mut stmt = conn
        .prepare(
            "SELECT name, sql FROM sqlite_master WHERE type = 'index' AND sql IS NOT NULL \
             AND name NOT LIKE 'sqlite_%'",
        )
        .unwrap();
    let rows = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    rows
}

// ───────────── (A) the family arm ─────────────

/// `provisioning_equivalence`'s `ORACLE_ONLY_INDEXES`, copied by name (that
/// file is read-only to P4.160). Both-ways.
const ORACLE_ONLY_INDEXES: &[(&str, &str, &str)] = &[(
    "main",
    "idx_wardrobe_items_character",
    "on the legacy wardrobe_items table v5 never makes (ORACLE_ONLY_TABLES)",
)];

/// `provisioning_equivalence`'s `SHARED_NAME_SQL`, copied by name: the 16
/// names both v4 families make where v5 keeps generateDDL's `("userId" ASC)`
/// and a real v4 boot the migration's `("userId")` — the same index. Both-ways.
const SHARED_NAME_SQL: &[(&str, &str, &str)] = &[
    ("main", "idx_api_keys_userId", "ASC"),
    ("main", "idx_background_jobs_userId", "ASC"),
    ("main", "idx_characters_userId", "ASC"),
    ("main", "idx_chats_userId", "ASC"),
    ("main", "idx_connection_profiles_userId", "ASC"),
    ("main", "idx_embedding_profiles_userId", "ASC"),
    ("main", "idx_embedding_status_userId", "ASC"),
    ("main", "idx_files_userId", "ASC"),
    ("main", "idx_folders_userId", "ASC"),
    ("main", "idx_image_profiles_userId", "ASC"),
    ("main", "idx_plugin_configs_userId", "ASC"),
    ("main", "idx_prompt_templates_userId", "ASC"),
    ("main", "idx_roleplay_templates_userId", "ASC"),
    ("main", "idx_tags_userId", "ASC"),
    ("main", "idx_tfidf_vocabularies_userId", "ASC"),
    ("llmLogs", "idx_llm_logs_userId", "ASC"),
];

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_backfilled_pre_round_instance_carries_v4s_first_boot_index_set() {
    let Some(oracle_path) = opt_env("QT_ORACLE_PROVISION") else {
        eprintln!("SKIP: set QT_ORACLE_PROVISION (see header).");
        return;
    };
    let _serial = SERIAL.lock().await;
    let oracle: Value =
        serde_json::from_str(&std::fs::read_to_string(&oracle_path).expect("read oracle"))
            .expect("parse oracle");
    let (_dir, data, _lines) = boot_pre_round(&[]);

    let mut problems = Vec::new();
    let mut seen_only = std::collections::BTreeSet::new();
    let mut seen_shared = std::collections::BTreeSet::new();
    for (part, file) in PARTITIONS {
        let w = Writer::open_writable(&data.join(file), PEPPER).unwrap();
        let ours = index_sql(w.connection());
        let theirs: BTreeMap<String, String> = oracle["indexes"][part]
            .as_array()
            .expect("oracle indexes (regenerate build-provision-oracle.ts)")
            .iter()
            .map(|r| {
                (
                    r["name"].as_str().unwrap().to_string(),
                    r["sql"].as_str().unwrap().to_string(),
                )
            })
            .collect();
        assert!(
            !theirs.is_empty(),
            "{part}: the oracle's index set is EMPTY"
        );
        for (name, sql) in &theirs {
            match ours.get(name) {
                None if ORACLE_ONLY_INDEXES
                    .iter()
                    .any(|(p, n, _)| *p == part && n == name) =>
                {
                    seen_only.insert((part, name.clone()));
                }
                None => problems.push(format!("{part}: v4 has index {name}, v5 does not — {sql}")),
                Some(mine) if mine == sql => {}
                Some(mine)
                    if SHARED_NAME_SQL
                        .iter()
                        .any(|(p, n, _)| *p == part && n == name)
                        && mine.replace(" ASC)", ")") == *sql =>
                {
                    seen_shared.insert((part, name.clone()));
                }
                Some(mine) => {
                    problems.push(format!("{part}: {name} differs — v4 {sql} | v5 {mine}"))
                }
            }
        }
        for name in ours.keys() {
            if !theirs.contains_key(name) {
                problems.push(format!(
                    "{part}: v5 has index {name}, v4's first boot does not"
                ));
            }
        }
    }
    for (p, n, why) in ORACLE_ONLY_INDEXES {
        if !seen_only.contains(&(*p, n.to_string())) {
            problems.push(format!("stale ORACLE_ONLY_INDEXES row {p}.{n} ({why})"));
        }
    }
    for (p, n, class) in SHARED_NAME_SQL {
        if !seen_shared.contains(&(*p, n.to_string())) {
            problems.push(format!("stale SHARED_NAME_SQL row {p}.{n} ({class})"));
        }
    }
    assert!(
        problems.is_empty(),
        "the backfilled index set differs from v4's real first boot ({} problem(s)):\n{}",
        problems.len(),
        problems.join("\n")
    );

    if let Some(out) = opt_env("QT_V5_BACKFILLED_OUT") {
        let _ = std::fs::remove_dir_all(&out);
        std::fs::create_dir_all(&out).unwrap();
        for (_, file) in PARTITIONS {
            std::fs::copy(data.join(file), out.join(file)).unwrap();
        }
        eprintln!("wrote the backfilled instance to {}", out.display());
    }
}

// ───────────── (B) the dedupe arm ─────────────

/// A v4 migration-logger line as v5's capture rig renders the port's line:
/// `<LEVEL> quilltap::boot <message> k=v …`, in v4's key order.
fn v4_line(log: &Value) -> String {
    let mut line = format!(
        "{} quilltap::boot {}",
        log["level"].as_str().unwrap().to_uppercase(),
        log["message"].as_str().unwrap()
    );
    for key in log["keys"].as_array().unwrap() {
        let key = key.as_str().unwrap();
        let value = &log["meta"][key];
        let rendered = match value {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        line.push_str(&format!(" {key}={rendered}"));
    }
    line
}

const PROFILE_MESSAGES: [&str; 2] = [
    "Renamed duplicate connection-profile name",
    "Enforced unique connection-profile names",
];

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_profile_rename_dedupe_matches_v4s_migration_in_five_shapes() {
    let Some(oracle_path) = opt_env("QT_ORACLE_INDEX_BACKFILL") else {
        eprintln!("SKIP: set QT_ORACLE_INDEX_BACKFILL (see header).");
        return;
    };
    let _serial = SERIAL.lock().await;
    let text = std::fs::read_to_string(&oracle_path).unwrap();
    let modes: Vec<Value> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str::<Value>(l).expect("NDJSON line"))
        .filter(|v| v["mode"] != "runner-on-backfilled")
        .collect();
    let names: Vec<&str> = modes.iter().map(|m| m["mode"].as_str().unwrap()).collect();
    assert_eq!(
        names,
        [
            "clash",
            "unicode-trim",
            "no-clash",
            "padded-only",
            "indexed"
        ],
        "the oracle's shapes"
    );
    let unique_sql = artifact("migration_indexes.json", "main")
        .into_iter()
        .find(|s| index_name(s).as_deref() == Some(PROFILE_INDEX))
        .unwrap();

    let mut failures = Vec::new();
    for oracle in &modes {
        let mode = oracle["mode"].as_str().unwrap();
        let verdict = oracle["report"][0].as_str().unwrap();
        let ran = verdict.starts_with("+RAN add-connection-profile-unique-name-index-v1");
        assert_eq!(
            ran,
            mode != "indexed",
            "[{mode}] v4's report — a shape that changed verdict is a vacuous diff: {verdict}"
        );

        let mut plants: Vec<String> = Vec::new();
        if oracle["indexPlanted"] == true {
            plants.push(unique_sql.clone());
        }
        plants.extend(
            oracle["plants"]
                .as_array()
                .unwrap()
                .iter()
                .map(|s| s.as_str().unwrap().to_string()),
        );
        let (_dir, data, lines) = boot_pre_round(&plants);
        let w = Writer::open_writable(&data.join(MAIN), PEPPER).unwrap();
        let conn = w.connection();

        let ours_sql: Option<String> = conn
            .query_row(
                "SELECT sql FROM sqlite_master WHERE type = 'index' AND name = ?1",
                [PROFILE_INDEX],
                |r| r.get(0),
            )
            .ok();
        if json!(ours_sql) != oracle["indexSql"] {
            failures.push(format!(
                "[{mode}] index sql: v5 {ours_sql:?} | v4 {}",
                oracle["indexSql"]
            ));
        }

        let seeded = &oracle["seeded"];
        let normalize = |id: &str, updated_at: &str| {
            if seeded[id].as_str() == Some(updated_at) {
                "<seeded>"
            } else {
                "<re-stamped>"
            }
        };
        let mut stmt = conn
            .prepare("SELECT id, userId, name, updatedAt FROM connection_profiles ORDER BY id")
            .unwrap();
        let ours_rows: Vec<Value> = stmt
            .query_map([], |r| {
                let (id, user, name, updated): (String, String, String, String) =
                    (r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?);
                Ok(json!({"id": id, "userId": user, "name": name,
                          "updatedAt": normalize(&id, &updated)}))
            })
            .unwrap()
            .map(Result::unwrap)
            .collect();
        let theirs_rows: Vec<Value> = oracle["rows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| {
                let id = r["id"].as_str().unwrap();
                json!({"id": id, "userId": r["userId"], "name": r["name"],
                       "updatedAt": normalize(id, r["updatedAt"].as_str().unwrap())})
            })
            .collect();
        if ours_rows != theirs_rows {
            failures.push(format!(
                "[{mode}] rows:\n  v5 {ours_rows:#?}\n  v4 {theirs_rows:#?}"
            ));
        }

        let ours_lines: Vec<String> = lines
            .iter()
            .filter(|l| PROFILE_MESSAGES.iter().any(|m| l.contains(m)))
            .cloned()
            .collect();
        let theirs_lines: Vec<String> = oracle["logs"]
            .as_array()
            .unwrap()
            .iter()
            .map(v4_line)
            .collect();
        if ours_lines != theirs_lines {
            failures.push(format!(
                "[{mode}] log lines:\n  v5 {ours_lines:#?}\n  v4 {theirs_lines:#?}"
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} difference(s) against v4's real migration:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
