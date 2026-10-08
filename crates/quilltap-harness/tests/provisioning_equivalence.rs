//! P4.4 unit 1 — the provisioning differential.
//!
//! Proves that a v5 `Setup`-provisioned instance matches a v4 fresh instance
//! (the deterministic first-boot seed; the sample-content import + roleplay
//! templates + built-in mount stores are the P4.4 named deferrals) AND that the
//! two engines are cross-compatible:
//!
//!   1. **Schema** (P4.153 — the oracle builds v4's instance the way its REAL
//!      first boot does: v4's `MigrationRunner` FIRST, then the repositories'
//!      `ensureCollection` pass — `harness/oracle/provision/migrations-first.ts`):
//!      (a) v5's TABLE text is byte-exact against the committed generateDDL dump
//!      `fresh_schema.json` (the migration-built text is the asymmetry the
//!      order's R-A keeps); (b) every table's column SET equals v4's, modulo the
//!      named both-ways `ORACLE_ONLY_TABLES` / `TABLE_COLUMN_ASYMMETRY` rows;
//!      (c) v5's index set per partition EQUALS v4's real first boot's — BOTH
//!      families, name for name, SQL byte-equal — modulo `ORACLE_ONLY_INDEXES`
//!      and the `SHARED_NAME_SQL` classes. Red-first on `main` before P4.153:
//!      (c) failed by exactly the migration family's 59 names. (d) — the D23
//!      tripwire (the `94fbb1ae3` smalls unification): the committed
//!      `fresh_schema.json` equals a LIVE `dump-fresh-schema.ts` run from the
//!      v4 tree (`QT_FRESH_SCHEMA_LIVE`, required), so v4 moving a table's
//!      generateDDL text reddens this family as D23 promises.
//!   2. **Seed rows** — the single user, its chat settings, and the default
//!      embedding profile match (minted id/timestamps normalized).
//!   3. **Cross-compat, v5 reads v4** — a real v4-built instance opens under the
//!      v5 ported reads.
//!   4. **Cross-compat, v4 reads v5** — the v5-provisioned instance is written to
//!      `QT_V5_PROVISION_OUT` for `verify-v5-provisioned.ts` to open with v4's
//!      REAL repositories.
//!
//! Generate the oracle + fixtures (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   QT_ORACLE_PROVISION=/tmp/oracle-provision.json \
//!   QT_V4_FRESH_OUT=/tmp/qt-v4-fresh \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/provision/build-provision-oracle.ts
//!   QT_DBKEY_V4_OUT=/tmp/qt-v4-dbkey \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/provision/verify-dbkey-crosscompat.ts
//!   QT_SCHEMA_OUT=/tmp/qt-fresh-schema-live.json \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/provision/dump-fresh-schema.ts
//!
//! Run the differential:
//!   QT_ORACLE_PROVISION=/tmp/oracle-provision.json \
//!   QT_FIXTURE_V4_FRESH=/tmp/qt-v4-fresh \
//!   QT_V5_PROVISION_OUT=/tmp/qt-v5-provisioned \
//!   QT_DBKEY_V4_FIXTURE=/tmp/qt-v4-dbkey \
//!   QT_DBKEY_V5_OUT=/tmp/qt-v5-dbkey \
//!   QT_FRESH_SCHEMA_LIVE=/tmp/qt-fresh-schema-live.json \
//!     cargo test -p quilltap-harness --test provisioning_equivalence -- --nocapture
//!
//! Then prove v4 reads the v5 outputs. These two legs MUST run from the v4
//! checkout (their scripts import v4's `@/lib` alias, which only resolves
//! under v4's tsconfig — from any other cwd they die with ERR_MODULE_NOT_FOUND,
//! which the 4.8.2-round unify sweep hit):
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_V5_PROVISIONED=/tmp/qt-v5-provisioned \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/provision/verify-v5-provisioned.ts
//!   QT_DBKEY_V5_FIXTURE=/tmp/qt-v5-dbkey \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/provision/verify-dbkey-crosscompat.ts
//!
//! The provisioner's second artifact, `migration_indexes.json`, is NOT
//! regenerated here (it is committed, D23); re-dump it with
//! `harness/oracle/provision/dump-migration-indexes.ts` (recipe in its header,
//! `QT_REAL_BOOT_MASTER` REQUIRED — a real `tsx server.ts` first boot's
//! `sqlite_master`, and any real-boot FINDING exits non-zero, P4.160 R-F)
//! when v4 moves the migration family — this family then goes red on the moved
//! names first.
//!
//! Skips (does not fail) when `QT_ORACLE_PROVISION` is unset — the standing
//! gated-differential discipline.

use std::path::{Path, PathBuf};

use quilltap_core::db::Writer;
use quilltap_core::services::provisioning::{provision_fresh_instance, SINGLE_USER_ID};
use rusqlite::types::ValueRef;
use rusqlite::Connection;
use serde_json::{json, Map, Value};

/// The test pepper the oracle keys its instance with — the v5-provisioned
/// instance must use the same one so v4 can open it (cross-compat #4).
const TEST_PEPPER: &str = "3q2+796tvu/erb7v3q2+796tvu/erb7v3q2+796tvu8=";

/// The two committed provisioning artifacts (the core replays both): the
/// generateDDL surface and — P4.153 — the migration-created index family.
const FRESH_SCHEMA_JSON: &str =
    include_str!("../../quilltap-core/src/services/provisioning/fresh_schema.json");
const MIGRATION_INDEXES_JSON: &str =
    include_str!("../../quilltap-core/src/services/provisioning/migration_indexes.json");

/// Dump one partition's `sqlite_master` the way the oracle does: CREATE TABLE
/// statements (by name) then CREATE INDEX statements (by name).
fn dump_schema(conn: &Connection) -> Vec<String> {
    let mut stmt = conn
        .prepare(
            "SELECT type, name, sql FROM sqlite_master \
             WHERE sql IS NOT NULL AND name NOT LIKE 'sqlite_%'",
        )
        .unwrap();
    let mut rows: Vec<(String, String, String)> = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })
        .unwrap()
        .map(Result::unwrap)
        .collect();
    rows.sort_by(|a, b| a.1.cmp(&b.1));
    let mut out: Vec<String> = rows
        .iter()
        .filter(|(t, _, _)| t == "table")
        .map(|(_, _, sql)| sql.clone())
        .collect();
    out.extend(
        rows.iter()
            .filter(|(t, _, _)| t == "index")
            .map(|(_, _, sql)| sql.clone()),
    );
    out
}

/// Read one row's columns into a JSON object (by SQLite affinity), stripping the
/// given keys (minted id/timestamps).
fn row_to_json(conn: &Connection, sql: &str, strip: &[&str]) -> Value {
    let mut stmt = conn.prepare(sql).unwrap();
    let cols: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
    let mut rows = stmt.query([]).unwrap();
    let row = rows.next().unwrap().expect("seed row present");
    let mut m = Map::new();
    for (i, name) in cols.iter().enumerate() {
        if strip.contains(&name.as_str()) {
            continue;
        }
        let v = match row.get_ref(i).unwrap() {
            ValueRef::Null => Value::Null,
            ValueRef::Integer(n) => json!(n),
            ValueRef::Real(f) => json!(f),
            ValueRef::Text(t) => json!(String::from_utf8_lossy(t)),
            // The seed rows carry no BLOB columns; represent defensively.
            ValueRef::Blob(b) => json!(format!("<blob {} bytes>", b.len())),
        };
        m.insert(name.clone(), v);
    }
    Value::Object(m)
}

fn opt_env(name: &str) -> Option<PathBuf> {
    std::env::var(name).ok().map(PathBuf::from)
}

/// Normalize a `roleplay_templates` dump: minted id/createdAt/updatedAt →
/// placeholders, rows sorted by (name, isBuiltIn).
fn normalize_templates(dump: &Value) -> Value {
    let mut rows: Vec<Value> = dump["rows"].as_array().unwrap().clone();
    for row in &mut rows {
        let obj = row.as_object_mut().unwrap();
        obj.insert("id".into(), Value::from("<id>"));
        obj.insert("createdAt".into(), Value::from("<ts>"));
        obj.insert("updatedAt".into(), Value::from("<ts>"));
    }
    rows.sort_by_key(|r| format!("{} {}", r["name"].as_str().unwrap_or(""), r["isBuiltIn"]));
    json!({ "columns": dump["columns"], "rows": rows })
}

/// Collapse an integer-valued REAL to an integer (v5's generateDDL REAL count
/// columns store `0.0`; v4's migration INTEGER columns store `0`).
fn collapse_int(v: &Value) -> Value {
    if v.is_f64() {
        if let Some(f) = v.as_f64() {
            if f.fract() == 0.0 && f.is_finite() {
                return json!(f as i64);
            }
        }
    }
    v.clone()
}

/// Remap mount dumps to the shared id map (mount id → name, folder id → path,
/// settings value → store name), dropping minted ids/timestamps.
fn remap_mounts(dmp: &Value, dmf: &Value, isettings: &Value) -> Value {
    use std::collections::HashMap;
    const MOUNT_KEYS: [&str; 3] = [
        "lanternBackgroundsMountPointId",
        "userUploadsMountPointId",
        "generalMountPointId",
    ];
    let mut id_to_name: HashMap<String, String> = HashMap::new();
    for row in dmp["rows"].as_array().unwrap() {
        id_to_name.insert(
            row["id"].as_str().unwrap().to_string(),
            row["name"].as_str().unwrap().to_string(),
        );
    }
    let mut fid_to_path: HashMap<String, String> = HashMap::new();
    for row in dmf["rows"].as_array().unwrap() {
        fid_to_path.insert(
            row["id"].as_str().unwrap().to_string(),
            row["path"].as_str().unwrap().to_string(),
        );
    }
    let count_cols = ["fileCount", "chunkCount", "totalSizeBytes"];
    let mut points: Vec<Value> = dmp["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| {
            let mut o = row.as_object().unwrap().clone();
            o.remove("id");
            o.remove("createdAt");
            o.remove("updatedAt");
            for c in count_cols {
                if let Some(v) = o.get(c) {
                    let cv = collapse_int(v);
                    o.insert(c.to_string(), cv);
                }
            }
            Value::Object(o)
        })
        .collect();
    points.sort_by_key(|r| r["name"].as_str().unwrap_or("").to_string());

    let mut folders: Vec<Value> = dmf["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| {
            let mount = id_to_name
                .get(row["mountPointId"].as_str().unwrap())
                .cloned()
                .unwrap_or_default();
            let parent = match row["parentId"].as_str() {
                Some(pid) => Value::from(fid_to_path.get(pid).cloned()),
                None => Value::Null,
            };
            json!({ "mount": mount, "name": row["name"], "path": row["path"], "parent": parent })
        })
        .collect();
    folders.sort_by_key(|r| {
        format!(
            "{}|{}",
            r["mount"].as_str().unwrap_or(""),
            r["path"].as_str().unwrap_or("")
        )
    });

    let mut settings: Vec<Value> = isettings["rows"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| MOUNT_KEYS.contains(&row["key"].as_str().unwrap_or("")))
        .map(|row| {
            let points_to = id_to_name
                .get(row["value"].as_str().unwrap())
                .cloned()
                .unwrap_or_default();
            json!({ "key": row["key"], "points_to": points_to })
        })
        .collect();
    settings.sort_by_key(|r| r["key"].as_str().unwrap_or("").to_string());

    json!({ "points": points, "folders": folders, "settings": settings })
}

#[test]
fn provisioning_matches_v4_fresh_instance() {
    let Some(oracle_path) = opt_env("QT_ORACLE_PROVISION") else {
        eprintln!("SKIP: set QT_ORACLE_PROVISION to the oracle JSON (see header).");
        return;
    };
    let oracle: Value =
        serde_json::from_str(&std::fs::read_to_string(&oracle_path).expect("read oracle"))
            .expect("parse oracle");

    // --- provision a fresh v5 instance ---
    let scratch = tempfile::tempdir().unwrap();
    let data = scratch.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    provision_fresh_instance(&data, TEST_PEPPER).expect("v5 provision");

    let main = Writer::open_writable(&data.join("quilltap.db"), TEST_PEPPER).unwrap();
    let mi = Writer::open_writable(&data.join("quilltap-mount-index.db"), TEST_PEPPER).unwrap();
    let ll = Writer::open_writable(&data.join("quilltap-llm-logs.db"), TEST_PEPPER).unwrap();

    // --- (1d) THE D23 TRIPWIRE: the committed generateDDL dump IS v4's live one ---
    // The `94fbb1ae3` smalls unification (review of P4.153): once (1a) was
    // re-aimed at the COMMITTED `fresh_schema.json` and (1b) compared column
    // SETS only, nothing compared v5's table surface with v4's LIVE
    // `generateDDL` any more — a v4 change to a column's type, DEFAULT, NOT
    // NULL or position would have passed green, and D23's "a red
    // `provisioning_equivalence` after a drift check" could never fire. The
    // recipe now re-runs `dump-fresh-schema.ts` from the v4 tree into
    // `QT_FRESH_SCHEMA_LIVE`; this arm requires it (no silent skip) and
    // demands the committed artifact equal it statement for statement. A red
    // here is v4 drift: RE-DUMP `fresh_schema.json` (never hand-edit, never
    // "fix" v5 back).
    let live_path = opt_env("QT_FRESH_SCHEMA_LIVE").unwrap_or_else(|| {
        panic!(
            "QT_FRESH_SCHEMA_LIVE is required with QT_ORACLE_PROVISION: run \
             harness/oracle/provision/dump-fresh-schema.ts from the v4 tree into it \
             (see header) — it is the D23 generateDDL tripwire"
        )
    });
    let live: Value =
        serde_json::from_str(&std::fs::read_to_string(&live_path).expect("read live dump"))
            .expect("parse live dump");
    let committed: Value = serde_json::from_str(FRESH_SCHEMA_JSON).unwrap();
    for part in ["main", "mountIndex", "llmLogs"] {
        let want = live[part].as_array().expect("live partition");
        assert!(
            !want.is_empty(),
            "(1d) live {part} dump is EMPTY — a failed regen"
        );
        let got = committed[part].as_array().expect("committed partition");
        let only_live: Vec<&Value> = want.iter().filter(|s| !got.contains(s)).collect();
        let only_committed: Vec<&Value> = got.iter().filter(|s| !want.contains(s)).collect();
        assert!(
            only_live.is_empty() && only_committed.is_empty() && want == got,
            "(1d) {part}: the committed fresh_schema.json differs from v4's LIVE generateDDL \
             (v4 drift — RE-DUMP it, D23).\n  only in v4's live dump: {only_live:#?}\n  \
             only in the committed dump: {only_committed:#?}"
        );
    }

    // --- (1a) TABLES: v5 replays the generateDDL surface verbatim ---
    // P4.153 R-A: the oracle now builds v4's instance migrations-first, so its
    // TABLE text is the MIGRATION's (`SQLITE_TABLES` + ALTERs) — a different
    // `CREATE TABLE` for the same columns, which v5 deliberately does not
    // reproduce (every tier-2 fixture shares v5's shape). The text arm is
    // re-aimed at the committed `fresh_schema.json`; the column SETS are
    // compared against the oracle below.
    let fresh: Value = serde_json::from_str(FRESH_SCHEMA_JSON).unwrap();
    for (part, conn) in [
        ("main", main.connection()),
        ("mountIndex", mi.connection()),
        ("llmLogs", ll.connection()),
    ] {
        let got: Vec<String> = dump_schema(conn)
            .into_iter()
            .filter(|sql| sql.starts_with("CREATE TABLE"))
            .collect();
        let expected: Vec<String> = fresh[part]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .filter(|sql| sql.starts_with("CREATE TABLE"))
            .collect();
        assert_eq!(got, expected, "table DDL mismatch in partition {part}");
    }

    // --- (1b) TABLES: the column SET per table equals v4's real first boot ---
    assert_table_columns(
        &oracle,
        &[
            ("main", main.connection()),
            ("mountIndex", mi.connection()),
            ("llmLogs", ll.connection()),
        ],
    );

    // --- (1c) INDEXES: both v4 families, by name + SQL ---
    assert_index_families(
        &oracle,
        &[
            ("main", main.connection()),
            ("mountIndex", mi.connection()),
            ("llmLogs", ll.connection()),
        ],
    );

    // --- (1e) P4.D255: the wear ledger's ON CONFLICT target, BOTH instances ---
    // The human's 2026-10-08 ruling over the order's R-A: a fresh v5 instance
    // keeps generateDDL's `wardrobe_wear_stats` text (REAL, no default) where
    // v4's migrations-first boot has the migration's (INTEGER DEFAULT 0) — 40
    // of the 45 tables differ that way, measured — so the arm that matters is
    // FUNCTIONAL: the same increments through v5's ledger on a COPY of each
    // fresh instance land the same rows (the UNIQUE `COALESCE` index is the
    // conflict target on both; `wearCount` reads type-tolerantly on v5's).
    if let Some(v4_fresh) = opt_env("QT_FIXTURE_V4_FRESH") {
        let ledger_rows = |db: &Path| -> Value {
            use quilltap_core::db::wardrobe_wear_stats::{
                WardrobeWearIncrement, WardrobeWearStatsRepository,
            };
            let copy = tempfile::tempdir().unwrap();
            let file = copy.path().join("quilltap.db");
            std::fs::copy(db, &file).unwrap();
            let w = Writer::open_writable(&file, TEST_PEPPER).unwrap();
            let repo = WardrobeWearStatsRepository::new(w.connection());
            for (wearer, chat, at) in [
                (Some("A"), "c1", "2026-01-01T00:00:00.000Z"),
                (Some("A"), "c2", "2026-02-01T00:00:00.000Z"),
                (None, "c3", "2026-01-15T00:00:00.000Z"),
                (None, "c4", "2026-01-15T00:00:00.000Z"),
            ] {
                repo.increment_wears(&[WardrobeWearIncrement {
                    item_id: "coat".into(),
                    wearer_character_id: wearer.map(str::to_string),
                    chat_id: Some(chat.into()),
                    at: at.into(),
                }])
                .expect("(1e) the increment upserts");
            }
            let rows: Vec<Value> = repo
                .find_all()
                .into_iter()
                .map(|r| {
                    json!([
                        r.item_id,
                        r.wearer_character_id,
                        r.wear_count,
                        r.first_worn_at,
                        r.last_worn_at,
                        r.last_worn_chat_id
                    ])
                })
                .collect();
            json!(rows)
        };
        let v5_rows = ledger_rows(&data.join("quilltap.db"));
        let v4_rows = ledger_rows(&v4_fresh.join("quilltap.db"));
        assert_eq!(v5_rows.as_array().map(Vec::len), Some(2), "(1e) {v5_rows}");
        assert_eq!(
            v5_rows, v4_rows,
            "(1e) the ledger diverged between the two fresh instances"
        );
    } else {
        eprintln!("SKIP (1e): set QT_FIXTURE_V4_FRESH (the recipe's v4-fresh instance).");
    }

    // --- (2) seed rows ---
    // P4.153: v4's rows now live in its MIGRATION-built tables, so each side
    // drops the columns only it has — exactly the TABLE_COLUMN_ASYMMETRY rows
    // the column arm above already pinned both ways — before the compare.
    let want = &oracle["seed"];
    let v4_row = |table: &str, v: &Value| without_asymmetric(table, "v4", v);
    let v5_row = |table: &str, v: Value| without_asymmetric(table, "v5", &v);
    // users: keep id (fixed SINGLE_USER_ID), strip timestamps.
    let got_user = row_to_json(
        main.connection(),
        &format!("SELECT * FROM users WHERE id = '{SINGLE_USER_ID}'"),
        &["createdAt", "updatedAt"],
    );
    assert_eq!(
        v5_row("users", got_user),
        v4_row("users", &want["users"]),
        "users seed mismatch"
    );

    // chat_settings: strip id/userId/timestamps (the deterministic remainder).
    let got_settings = row_to_json(
        main.connection(),
        &format!("SELECT * FROM chat_settings WHERE userId = '{SINGLE_USER_ID}'"),
        &["id", "userId", "createdAt", "updatedAt"],
    );
    assert_eq!(
        v5_row("chat_settings", got_settings),
        v4_row("chat_settings", &want["chatSettings"]),
        "chat_settings seed mismatch"
    );

    // embedding_profiles: strip id/timestamps (keep userId).
    let got_ep = row_to_json(
        main.connection(),
        "SELECT * FROM embedding_profiles WHERE provider = 'BUILTIN'",
        &["id", "createdAt", "updatedAt"],
    );
    assert_eq!(
        v5_row("embedding_profiles", got_ep),
        v4_row("embedding_profiles", &want["embeddingProfile"]),
        "embedding seed mismatch"
    );

    // --- (2b) P4.4u3 seeded tables: built-in roleplay templates + mount stores ---
    // A fresh v5 instance must carry the SAME seeds as a fresh-v4-with-migrations
    // instance. Minted ids/timestamps are normalized (templates: placeholdered +
    // keyed by name+isBuiltIn; mounts: remapped to the shared id map).
    if let Some(seeded) = oracle.get("seeded") {
        let got_templates = normalize_templates(
            &quilltap_core::db::dump_table_json_conn(main.connection(), "roleplay_templates", "id")
                .unwrap(),
        );
        assert_eq!(
            got_templates,
            normalize_templates(&seeded["roleplayTemplates"]),
            "roleplay_templates seed mismatch"
        );

        let got_mounts = remap_mounts(
            &quilltap_core::db::dump_table_json_conn(mi.connection(), "doc_mount_points", "id")
                .unwrap(),
            &quilltap_core::db::dump_table_json_conn(mi.connection(), "doc_mount_folders", "id")
                .unwrap(),
            &quilltap_core::db::dump_table_json_conn(main.connection(), "instance_settings", "key")
                .unwrap(),
        );
        assert_eq!(
            got_mounts,
            remap_mounts(
                &seeded["docMountPoints"],
                &seeded["docMountFolders"],
                &seeded["instanceSettings"],
            ),
            "mount-store seed mismatch"
        );
        eprintln!("OK: P4.4u3 seeded templates + mount stores match v4-fresh.");
    } else {
        eprintln!("note: oracle has no `seeded` block — regenerate build-provision-oracle.ts.");
    }

    drop((main, mi, ll));

    // --- (3) cross-compat: a real v4-built instance opens under v5 ---
    if let Some(v4) = opt_env("QT_FIXTURE_V4_FRESH") {
        assert_v5_reads_v4(&v4);
    } else {
        eprintln!("note: QT_FIXTURE_V4_FRESH unset — skipping the v5-reads-v4 check.");
    }

    // --- (4) cross-compat: write the v5 instance for v4 to read ---
    if let Some(out) = opt_env("QT_V5_PROVISION_OUT") {
        std::fs::create_dir_all(&out).unwrap();
        for name in [
            "quilltap.db",
            "quilltap-mount-index.db",
            "quilltap-llm-logs.db",
        ] {
            std::fs::copy(data.join(name), out.join(name)).unwrap();
        }
        eprintln!(
            "wrote v5-provisioned instance → {} (verify-v5-provisioned.ts reads it under v4)",
            out.display()
        );
    }
}

/// List the `.dbkey` files in a data dir, sorted — the bug-60 one-file
/// comparand (v4 4.8.1): a passphrase change must leave exactly
/// `quilltap.dbkey`, never the phantom `quilltap-llm-logs.dbkey`.
fn dbkey_files(data: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(data)
        .unwrap()
        .filter_map(|e| {
            let name = e.unwrap().file_name().into_string().unwrap();
            name.ends_with(".dbkey").then_some(name)
        })
        .collect();
    names.sort();
    names
}

/// The version floor v4's guard writes into `.dbkey` — the same value the
/// oracle plants (`verify-dbkey-crosscompat.ts`).
const MIN_SERVER_VERSION: &str = "4.9.0-dev.0";

/// Plant `minServerVersion` the way v4's version guard does: parse, assign,
/// 2-space-pretty back (`lib/startup/version-guard.ts:243-252`).
fn plant_min_server_version(dbkey_path: &Path) {
    let mut data: Map<String, Value> =
        serde_json::from_str(&std::fs::read_to_string(dbkey_path).unwrap()).unwrap();
    data.insert("minServerVersion".into(), json!(MIN_SERVER_VERSION));
    std::fs::write(dbkey_path, serde_json::to_string_pretty(&data).unwrap()).unwrap();
}

fn min_server_version(dbkey_path: &Path) -> Option<String> {
    let data: Map<String, Value> =
        serde_json::from_str(&std::fs::read_to_string(dbkey_path).unwrap()).unwrap();
    data.get("minServerVersion")
        .and_then(|v| v.as_str())
        .map(str::to_string)
}

/// changePassphrase cross-compat (v5 → v4): write a `.dbkey` that v5 minted and
/// re-wrapped via `change_passphrase`, for `verify-dbkey-crosscompat.ts` to
/// unlock with v4's REAL dbkey code. The known TEST_PEPPER lets the oracle assert
/// the exact recovered value. (The reverse direction is
/// [`reads_v4_changed_passphrase_dbkey`] over the oracle's `QT_DBKEY_V4_OUT`
/// leg.) `change_passphrase` emits the byte-identical format `save_dbkey` does.
#[test]
fn writes_v5_changed_passphrase_dbkey_for_v4() {
    use quilltap_core::dbkey;
    let Some(out) = opt_env("QT_DBKEY_V5_OUT") else {
        eprintln!("SKIP: set QT_DBKEY_V5_OUT to write the v5 change-passphrase .dbkey.");
        return;
    };
    // v4's `getDataDir()` is `<QUILLTAP_DATA_DIR>/data`, so write under `data/`
    // and let the oracle point QUILLTAP_DATA_DIR at `out`.
    let data = out.join("data");
    std::fs::create_dir_all(&data).unwrap();
    // Mint the .dbkey under "alpha", then re-wrap to "beta" — the file now wraps
    // TEST_PEPPER under the passphrase "beta".
    dbkey::save_dbkey(&data, TEST_PEPPER, "alpha").unwrap();
    // P4.46: plant the field v4's version guard writes, so the re-wrap below is
    // exercised over a file carrying a key v5 does not model. The oracle's
    // `QT_DBKEY_V5_FIXTURE` leg asserts BOTH that it survived and that v4's real
    // loader still opens the result.
    plant_min_server_version(&data.join("quilltap.dbkey"));
    dbkey::change_passphrase(&data, "alpha", "beta").unwrap();
    assert_eq!(
        min_server_version(&data.join("quilltap.dbkey")),
        Some(MIN_SERVER_VERSION.to_string()),
        "v5's re-wrap dropped the v4 version floor"
    );
    // Sanity: v5 reads its own output back.
    assert_eq!(
        dbkey::load_pepper(&data, Some("beta")).unwrap(),
        TEST_PEPPER
    );
    // One pepper, one file (bug 60): the rewrap wrote exactly quilltap.dbkey.
    // The oracle re-asserts this on its side of the fence.
    assert_eq!(dbkey_files(&data), ["quilltap.dbkey"]);
    eprintln!(
        "wrote v5 change-passphrase .dbkey → {} (verify-dbkey-crosscompat.ts unlocks it under v4)",
        out.display()
    );
}

/// changePassphrase cross-compat (v4 → v5): unlock a `.dbkey` that v4's REAL
/// `changePassphrase` re-wrapped at the pinned baseline
/// (`verify-dbkey-crosscompat.ts`, the `QT_DBKEY_V4_OUT` direction — run it
/// FIRST). Asserts the recovered pepper, the wrong-passphrase refusal, and the
/// bug-60 one-file outcome on v4's own output.
#[test]
fn reads_v4_changed_passphrase_dbkey() {
    use quilltap_core::dbkey;
    let Some(dir) = opt_env("QT_DBKEY_V4_FIXTURE") else {
        eprintln!(
            "SKIP: set QT_DBKEY_V4_FIXTURE to the dir verify-dbkey-crosscompat.ts \
             (QT_DBKEY_V4_OUT) wrote."
        );
        return;
    };
    let data = dir.join("data");
    // v4 at 4.8.1 wrote exactly one .dbkey file (bug 60).
    assert_eq!(dbkey_files(&data), ["quilltap.dbkey"]);
    // The wrong passphrase refuses; "beta" (v4's re-wrap) recovers TEST_PEPPER.
    assert!(dbkey::load_pepper(&data, Some("wrong")).is_err());
    assert_eq!(
        dbkey::load_pepper(&data, Some("beta")).unwrap(),
        TEST_PEPPER
    );

    // ── P4.46: unknown-field preservation over a REAL v4-authored file ───────
    // The oracle re-planted `minServerVersion` after v4's own re-wrap dropped
    // it (that drop is measured and pinned oracle-side — the deliberate
    // divergence). v5's `change_passphrase` must carry the field through, since
    // v5 has no version guard to rewrite it and the floor would otherwise be
    // gone for good. Work on a COPY so the fixture stays as the oracle left it.
    assert_eq!(
        min_server_version(&data.join("quilltap.dbkey")),
        Some(MIN_SERVER_VERSION.to_string()),
        "the oracle fixture should carry a v4-written version floor"
    );
    let scratch = tempfile::tempdir().unwrap();
    std::fs::copy(
        data.join("quilltap.dbkey"),
        scratch.path().join("quilltap.dbkey"),
    )
    .unwrap();
    assert_eq!(
        dbkey::change_passphrase(scratch.path(), "beta", "gamma").unwrap(),
        TEST_PEPPER
    );
    assert_eq!(
        min_server_version(&scratch.path().join("quilltap.dbkey")),
        Some(MIN_SERVER_VERSION.to_string()),
        "v5's re-wrap dropped a v4-written version floor"
    );
    assert_eq!(
        dbkey::load_pepper(scratch.path(), Some("gamma")).unwrap(),
        TEST_PEPPER
    );

    eprintln!(
        "v5 unlocked the v4 change-passphrase .dbkey (pepper matches, one file) and preserved \
         minServerVersion through its own re-wrap"
    );
}

/// Open the committed v4-fresh instance with v5's ported reads: the schema is
/// v4-generated (generateDDL), so the port marshals it correctly — the single
/// user reads back, chats is empty, the default embedding profile is present.
fn assert_v5_reads_v4(v4_dir: &Path) {
    use quilltap_core::db::{chats_read, embedding_profiles, users};

    let main = Writer::open_writable(&v4_dir.join("quilltap.db"), TEST_PEPPER)
        .expect("v5 opens the v4-built main DB");
    let conn = main.connection();

    // The single user reads back with the fixed id (the `name` column is
    // nullable, so `find_name_by_id` returns row-exists → name-value).
    let name = users::find_name_by_id(conn, SINGLE_USER_ID)
        .unwrap()
        .expect("v4 user reads under v5");
    assert_eq!(name.as_deref(), Some("Local User"));

    // chats is empty (a fresh instance).
    let chats = chats_read::find_by_user_id(conn, SINGLE_USER_ID).unwrap();
    assert!(chats.is_empty(), "fresh v4 instance has no chats");

    // The default BUILTIN embedding profile is present + default.
    let ep = embedding_profiles::find_default(conn, SINGLE_USER_ID)
        .unwrap()
        .expect("v4 default embedding profile reads under v5");
    assert_eq!(ep.provider, "BUILTIN");
}

// ───────────────────────── P4.153: the index + column arms ─────────────────────────

/// Tables v4's real first boot makes that a freshly PROVISIONED v5 instance
/// (no boot yet) does not (P4.153 R-A: v5's TABLE surface is the generateDDL
/// one). Both-ways: a row whose table v5 starts provisioning, or v4 stops
/// creating, trips the arm.
const ORACLE_ONLY_TABLES: &[(&str, &str, &str)] = &[
    (
        "main",
        "chat_messages_fts",
        "FTS5 — v5 makes it at boot (db::chat_message_fts)",
    ),
    ("main", "chat_messages_fts_config", "FTS5 shadow table"),
    ("main", "chat_messages_fts_data", "FTS5 shadow table"),
    ("main", "chat_messages_fts_docsize", "FTS5 shadow table"),
    ("main", "chat_messages_fts_idx", "FTS5 shadow table"),
    (
        "main",
        "chat_messages_fts_map",
        "FTS5 rowid map — v5 makes it at boot",
    ),
    (
        "main",
        "migrations_state",
        "v4's migration ledger — the runner stays deferred",
    ),
    (
        "main",
        "migrations_metadata",
        "v4's migration ledger metadata — deferred with it",
    ),
    (
        "main",
        "wardrobe_items",
        "legacy table only sqlite-initial-schema-v1 still makes",
    ),
];

/// Per-table column asymmetries between v4's MIGRATION-built table (a real
/// first boot) and v5's generateDDL one: `(partition, table, side, columns,
/// why)`, `side` naming the side that HAS the columns. P4.153 measured that the
/// order's R-A premise ("the SAME column set") is FALSE on a real first boot —
/// recorded here, not closed (R-A keeps the generateDDL TABLE surface; v4's
/// repositories are column-name-addressed and every v5 read already runs on
/// the migration shape of the Friday copy). Both-ways, per column.
const TABLE_COLUMN_ASYMMETRY: &[(&str, &str, &str, &[&str], &str)] = &[
    (
        "main",
        "characters",
        "v5",
        &[
            "aliases",
            "canChooseOutfit",
            "description",
            "exampleDialogues",
            "firstMessage",
            "identity",
            "manifesto",
            "metadata",
            "personality",
            "physicalDescription",
            "pronouns",
            "scenarios",
            "systemPrompts",
            "talkativeness",
            "title",
        ],
        "v4 `cutover-characters-to-vault` slims the row to the vault-backed shape",
    ),
    (
        "main",
        "projects",
        "v5",
        &[
            "allowAnyCharacter",
            "answerConfirmationOverride",
            "backgroundDisplayMode",
            "characterRoster",
            "color",
            "defaultAgentModeEnabled",
            "defaultAlertCharactersOfLanternImages",
            "defaultAvatarGenerationEnabled",
            "defaultDisabledToolGroups",
            "defaultDisabledTools",
            "defaultImageProfileId",
            "defaultRoleplayTemplateId",
            "description",
            "icon",
            "instructions",
            "state",
            "staticBackgroundImageId",
            "storyBackgroundImageId",
            "storyBackgroundsEnabled",
        ],
        "v4 `cutover-projects-to-store` slims the row to the store-backed shape",
    ),
    (
        "main",
        "groups",
        "v5",
        &["color", "description", "icon", "instructions", "state"],
        "v4 `create-groups-table` makes the store-backed slim row",
    ),
    (
        "main",
        "chat_settings",
        "v5",
        &["timezone"],
        "a generateDDL column no v4 migration adds",
    ),
    (
        "main",
        "chat_settings",
        "v4",
        &[
            "dangerousContentSettings",
            "memoryExtractionConcurrency",
            "uncensoredImageDescriptionProfileId",
        ],
        "migration columns v4 dropped from its schema but never from the table",
    ),
    (
        "main",
        "chats",
        "v4",
        &[
            "lastModerationRefusalAt",
            "moderationRefusalCount",
            "transcriptVersion",
        ],
        "schema-absent migration columns — v5 adds them at boot (repairs)",
    ),
    (
        "main",
        "users",
        "v4",
        &["backupCodes", "totp", "totpAttempts", "trustedDevices"],
        "legacy auth columns sqlite-initial-schema-v1 still makes",
    ),
];

/// Index names v4's real first boot carries that a v5 instance does not, with
/// the reason. Both-ways.
const ORACLE_ONLY_INDEXES: &[(&str, &str, &str)] = &[(
    "main",
    "idx_wardrobe_items_character",
    "on the legacy wardrobe_items table v5 never makes (ORACLE_ONLY_TABLES)",
)];

/// Names BOTH v4 families create, where the MIGRATION's text (which a real v4
/// boot keeps — it runs first, and generateDDL's `IF NOT EXISTS` is then a
/// no-op) differs from the generateDDL copy v5 keeps (the order's R-D).
/// `(partition, name, class)`; the class names the ONE textual difference.
/// Both-ways: a converged row trips the arm.
///
/// `ASC` (16): generateDDL writes `("userId" ASC)`, the migration `("userId")`
/// — the same index (ASC is SQLite's default), so v5 keeps the generateDDL
/// copy. The one shared name whose difference is SEMANTIC is NOT here:
/// `idx_doc_mount_folders_mp_path` is UNIQUE in v4's migrations
/// (`provision-*-mount`, `convert-project-files-to-document-stores`) and plain
/// in the repository's `onTableEnsured`; a real v4 boot keeps the migration's,
/// and since the human's 2026-10-06 ruling so does v5 (`migration_indexes.json`
/// carries it, the provisioner skips the plain copy), so it must match
/// byte-for-byte above. Before that ruling it sat here as a `UNIQUE` row.
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

/// `row` without the main-partition columns [`TABLE_COLUMN_ASYMMETRY`] says only
/// `side` has on `table` (a seed row compared across the two table shapes).
fn without_asymmetric(table: &str, side: &str, row: &Value) -> Value {
    let mut out = row.as_object().expect("seed row object").clone();
    for (p, t, s, cols, _) in TABLE_COLUMN_ASYMMETRY {
        if *p == "main" && *t == table && *s == side {
            for c in *cols {
                out.remove(*c);
            }
        }
    }
    Value::Object(out)
}

fn table_columns(conn: &Connection) -> std::collections::BTreeMap<String, Vec<String>> {
    let mut tables: Vec<String> = conn
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    tables.sort();
    tables
        .into_iter()
        .map(|t| {
            let mut cols: Vec<String> = conn
                .prepare(&format!("PRAGMA table_info(\"{t}\")"))
                .unwrap()
                .query_map([], |r| r.get::<_, String>(1))
                .unwrap()
                .map(Result::unwrap)
                .collect();
            cols.sort();
            (t, cols)
        })
        .collect()
}

/// The column SET of every table equals v4's real first boot's, modulo the two
/// named tables above.
fn assert_table_columns(oracle: &Value, parts: &[(&str, &Connection)]) {
    let mut problems = Vec::new();
    let mut seen_tables = std::collections::BTreeSet::new();
    let mut seen_cols = std::collections::BTreeSet::new();
    for (part, conn) in parts {
        let ours = table_columns(conn);
        let theirs = oracle["columns"][*part]
            .as_object()
            .expect("oracle columns");
        for (table, cols) in theirs {
            let Some(mine) = ours.get(table) else {
                if ORACLE_ONLY_TABLES
                    .iter()
                    .any(|(p, t, _)| p == part && t == table)
                {
                    seen_tables.insert((part.to_string(), table.clone()));
                } else {
                    problems.push(format!("{part}: v4 has table {table}, v5 does not"));
                }
                continue;
            };
            let theirs: std::collections::BTreeSet<&str> = cols
                .as_array()
                .unwrap()
                .iter()
                .map(|c| c.as_str().unwrap())
                .collect();
            let mine: std::collections::BTreeSet<&str> = mine.iter().map(String::as_str).collect();
            for (col, side, missing) in theirs
                .difference(&mine)
                .map(|c| (*c, "v4", "v5"))
                .chain(mine.difference(&theirs).map(|c| (*c, "v5", "v4")))
            {
                if TABLE_COLUMN_ASYMMETRY.iter().any(|(p, t, s, cs, _)| {
                    p == part && t == table && *s == side && cs.contains(&col)
                }) {
                    seen_cols.insert((part.to_string(), table.clone(), col.to_string()));
                } else {
                    problems.push(format!(
                        "{part}.{table}.{col}: {side} has it, {missing} does not"
                    ));
                }
            }
        }
        for table in ours.keys() {
            if !theirs.contains_key(table) {
                problems.push(format!("{part}: v5 has table {table}, v4 does not"));
            }
        }
    }
    for (p, t, why) in ORACLE_ONLY_TABLES {
        if !seen_tables.contains(&(p.to_string(), t.to_string())) {
            problems.push(format!("stale ORACLE_ONLY_TABLES row {p}.{t} ({why})"));
        }
    }
    for (p, t, s, cs, why) in TABLE_COLUMN_ASYMMETRY {
        for c in *cs {
            if !seen_cols.contains(&(p.to_string(), t.to_string(), c.to_string())) {
                problems.push(format!(
                    "stale TABLE_COLUMN_ASYMMETRY row {p}.{t}.{c} ({s}: {why})"
                ));
            }
        }
    }
    assert!(
        problems.is_empty(),
        "column-set arm:\n{}",
        problems.join("\n")
    );
}

/// `(name, sql)` of every named index in one partition.
fn index_sql(conn: &Connection) -> std::collections::BTreeMap<String, String> {
    conn.prepare(
        "SELECT name, sql FROM sqlite_master \
         WHERE type = 'index' AND sql IS NOT NULL AND name NOT LIKE 'sqlite_%'",
    )
    .unwrap()
    .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
    .unwrap()
    .map(Result::unwrap)
    .collect()
}

/// v5's index set per partition EQUALS a real v4 first boot's (both families),
/// name for name, and each statement is byte-equal — modulo the named tables.
fn assert_index_families(oracle: &Value, parts: &[(&str, &Connection)]) {
    let mut problems = Vec::new();
    let mut seen_only = std::collections::BTreeSet::new();
    let mut seen_shared = std::collections::BTreeSet::new();
    let mut migration_family = 0usize;
    let artifact: Value = serde_json::from_str(MIGRATION_INDEXES_JSON).unwrap();
    for (part, conn) in parts {
        let ours = index_sql(conn);
        let theirs: std::collections::BTreeMap<String, String> = oracle["indexes"][*part]
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
        migration_family += artifact[*part].as_array().unwrap().len();
        for (name, sql) in &theirs {
            match ours.get(name) {
                None if ORACLE_ONLY_INDEXES
                    .iter()
                    .any(|(p, n, _)| p == part && n == name) =>
                {
                    seen_only.insert((part.to_string(), name.clone()));
                }
                None => problems.push(format!("{part}: v4 has index {name}, v5 does not — {sql}")),
                Some(mine) if mine == sql => {}
                Some(mine) => match SHARED_NAME_SQL
                    .iter()
                    .find(|(p, n, _)| p == part && n == name)
                {
                    Some((_, _, class)) => {
                        let explained = match *class {
                            "ASC" => mine.replace(" ASC)", ")") == *sql,
                            other => panic!("unknown SHARED_NAME_SQL class {other}"),
                        };
                        if explained {
                            seen_shared.insert((part.to_string(), name.clone()));
                        } else {
                            problems.push(format!(
                                "{part}: {name} differs beyond its {class} class — v4 {sql} | v5 {mine}"
                            ));
                        }
                    }
                    None => problems.push(format!("{part}: {name} differs — v4 {sql} | v5 {mine}")),
                },
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
        if !seen_only.contains(&(p.to_string(), n.to_string())) {
            problems.push(format!("stale ORACLE_ONLY_INDEXES row {p}.{n} ({why})"));
        }
    }
    for (p, n, class) in SHARED_NAME_SQL {
        if !seen_shared.contains(&(p.to_string(), n.to_string())) {
            problems.push(format!("stale SHARED_NAME_SQL row {p}.{n} ({class})"));
        }
    }
    assert!(
        problems.is_empty(),
        "index arm ({} problem(s); the migration family is {migration_family}):\n{}",
        problems.len(),
        problems.join("\n")
    );
}
