//! P4.4 unit 1 — the provisioning differential.
//!
//! Proves that a v5 `Setup`-provisioned instance matches a v4 fresh instance
//! (the deterministic first-boot seed; the sample-content import + roleplay
//! templates + built-in mount stores are the P4.4 named deferrals) AND that the
//! two engines are cross-compatible:
//!
//!   1. **Schema byte-exact** — v5's `sqlite_master` (per partition) equals v4's
//!      LIVE generateDDL schema (so the diff catches v4 drift, not just v5 replay
//!      fidelity).
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
//!
//! Run the differential:
//!   QT_ORACLE_PROVISION=/tmp/oracle-provision.json \
//!   QT_FIXTURE_V4_FRESH=/tmp/qt-v4-fresh \
//!   QT_V5_PROVISION_OUT=/tmp/qt-v5-provisioned \
//!   QT_DBKEY_V4_FIXTURE=/tmp/qt-v4-dbkey \
//!   QT_DBKEY_V5_OUT=/tmp/qt-v5-dbkey \
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

    // --- (1) schema byte-exact, per partition ---
    let want_schema = &oracle["schema"];
    for (part, conn) in [
        ("main", main.connection()),
        ("mountIndex", mi.connection()),
        ("llmLogs", ll.connection()),
    ] {
        let got = dump_schema(conn);
        let expected: Vec<String> = want_schema[part]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect();
        assert_eq!(got, expected, "schema mismatch in partition {part}");
    }

    // --- (2) seed rows ---
    let want = &oracle["seed"];
    // users: keep id (fixed SINGLE_USER_ID), strip timestamps.
    let got_user = row_to_json(
        main.connection(),
        &format!("SELECT * FROM users WHERE id = '{SINGLE_USER_ID}'"),
        &["createdAt", "updatedAt"],
    );
    assert_eq!(got_user, want["users"], "users seed mismatch");

    // chat_settings: strip id/userId/timestamps (the deterministic remainder).
    let got_settings = row_to_json(
        main.connection(),
        &format!("SELECT * FROM chat_settings WHERE userId = '{SINGLE_USER_ID}'"),
        &["id", "userId", "createdAt", "updatedAt"],
    );
    assert_eq!(
        got_settings, want["chatSettings"],
        "chat_settings seed mismatch"
    );

    // embedding_profiles: strip id/timestamps (keep userId).
    let got_ep = row_to_json(
        main.connection(),
        "SELECT * FROM embedding_profiles WHERE provider = 'BUILTIN'",
        &["id", "createdAt", "updatedAt"],
    );
    assert_eq!(got_ep, want["embeddingProfile"], "embedding seed mismatch");

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
