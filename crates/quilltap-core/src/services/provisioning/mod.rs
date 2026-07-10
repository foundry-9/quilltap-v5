//! Fresh-instance provisioning (P4.4 unit 1) — the CORE creates a brand-new,
//! **encrypted-from-byte-zero** instance at `Setup` time.
//!
//! v4 has no such thing: on a fresh boot its migrations create the databases in
//! PLAINTEXT (before the pepper exists), the app seeds them, and only then does
//! `?action=setup` mint the pepper and encrypt the files in place — a plaintext
//! window this port eliminates by having the pepper in hand *before* any
//! partition is created (`Writer::open_writable` keys the file on creation).
//!
//! What a fresh v5 instance gets:
//!
//! - **Schema** across all three partitions (main / mount-index / llm-logs) —
//!   the **generateDDL surface**, exactly what v4's real repositories create on
//!   first access via `ensureCollection`/`getCollection` (the same mechanism
//!   every tier-2 fixture uses, proven v4-compatible by every tier-2/tier-3
//!   differential). The DDL is captured verbatim from v4 by
//!   `harness/oracle/provision/dump-fresh-schema.ts` into [`FRESH_SCHEMA_JSON`]
//!   and replayed here. (A long-lived migration-accumulated v4 instance has the
//!   SAME column set but a different column ORDER and DDL text; v4's repos are
//!   column-name-addressed so the generateDDL surface is a valid, cross-compatible
//!   schema — a byte-for-byte match with a migration-accumulated instance would
//!   require porting the migration runner, a tracked deferral, unnecessary for
//!   correctness.)
//! - **Seed rows** (v4's deterministic first-boot seed, minus the deferred
//!   sample-content import): the single user (v4 `getOrCreateSingleUser`), its
//!   default chat settings, and the default `Built-in TF-IDF` embedding profile
//!   (v4 `seedEmbeddingProfiles`) — so zero-config semantic search works without
//!   an API key.
//! - **The built-in roleplay templates** (`Standard` / `Quilltap RP`, v4
//!   `seedBuiltInTemplates`) via [`builtin_templates`] (P4.4u3, family 1).
//! - **The three built-in mount stores** (`Lantern Backgrounds` / `Quilltap
//!   Uploads` / `Quilltap General`) + their `instance_settings` pointers, via
//!   [`builtin_mounts`] (P4.4u3, family 2).
//!
//! ## Tracked deferrals (named in the P4.4 report)
//!
//! - **The sample-content seed import** (`first-startup/imports/
//!   lorian-and-riya.qtap` → 2 characters + 42 memories + avatars) drags in the
//!   unported import service; a fresh instance boots and the SPA is fully usable
//!   with zero characters (you create your own).
//!
//! ## The chat_settings seam
//!
//! v4's `updateForUser` OMITS optional nested keys (Zod omits keys the input
//! doesn't supply), but the ported `ChatSettings` nested structs serialize
//! optionals as explicit `null` (built for the always-present tier-2 corpus), so
//! composing the ported `create` would not be byte-exact for the seed. The seed
//! row is therefore captured verbatim from v4 into [`CHAT_SETTINGS_SEED_JSON`] and
//! replayed with a raw INSERT (the byte-exact static-transcription precedent);
//! the differential proves it matches. `users`/`embedding_profiles` have no such
//! omission and compose the ported repos directly.

use std::path::Path;

use rusqlite::types::Value as SqlValue;
use rusqlite::{params_from_iter, Connection};
use serde::Deserialize;

use crate::clock;
use crate::db::{embedding_profiles, users, DbError, Writer};
use crate::services::{builtin_mounts, builtin_templates};

/// v4 `SINGLE_USER_ID` (`lib/auth/single-user.ts`) — the fixed UUID every row
/// belongs to in single-user mode.
pub const SINGLE_USER_ID: &str = "ffffffff-ffff-ffff-ffff-ffffffffffff";

/// The captured fresh-instance DDL (per partition), verbatim from v4's real
/// `ensureCollection`/`getCollection`. Regenerate with
/// `harness/oracle/provision/dump-fresh-schema.ts` (recipe in its header).
static FRESH_SCHEMA_JSON: &str = include_str!("fresh_schema.json");

/// The captured `chat_settings` seed row's columns (all but the minted
/// id/userId/timestamps). Regenerate alongside the schema (`QT_SEED_OUT=…`).
static CHAT_SETTINGS_SEED_JSON: &str = include_str!("chat_settings_seed.json");

#[derive(Deserialize)]
struct FreshSchema {
    main: Vec<String>,
    #[serde(rename = "mountIndex")]
    mount_index: Vec<String>,
    #[serde(rename = "llmLogs")]
    llm_logs: Vec<String>,
}

#[derive(Deserialize)]
struct ChatSettingsSeed {
    columns: Vec<String>,
    values: serde_json::Map<String, serde_json::Value>,
}

/// A provisioning failure.
#[derive(Debug)]
pub enum ProvisionError {
    /// A database open / DDL / seed write failed.
    Db(DbError),
    /// The embedded schema/seed artifact failed to parse (a build/regen bug).
    Artifact(String),
}

impl std::fmt::Display for ProvisionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProvisionError::Db(e) => write!(f, "provisioning database error: {e}"),
            ProvisionError::Artifact(m) => write!(f, "provisioning artifact error: {m}"),
        }
    }
}
impl std::error::Error for ProvisionError {}

impl From<DbError> for ProvisionError {
    fn from(e: DbError) -> Self {
        ProvisionError::Db(e)
    }
}
impl From<rusqlite::Error> for ProvisionError {
    fn from(e: rusqlite::Error) -> Self {
        ProvisionError::Db(DbError::Sqlite(e))
    }
}

/// Create and seed a fresh instance under `data_dir`, keyed by `pepper_b64` —
/// all three partitions encrypted from creation. The caller (the engine's
/// `Setup`) has already minted the pepper and written `quilltap.dbkey`.
///
/// The three partition files MUST NOT already exist (this is first-run setup);
/// each is created by `Writer::open_writable` and its schema replayed in one
/// transaction. Only the main partition carries seed rows.
pub fn provision_fresh_instance(data_dir: &Path, pepper_b64: &str) -> Result<(), ProvisionError> {
    let schema: FreshSchema = serde_json::from_str(FRESH_SCHEMA_JSON)
        .map_err(|e| ProvisionError::Artifact(format!("fresh_schema.json: {e}")))?;

    // Main partition: schema + seed rows + the built-in roleplay templates
    // (family 1, main-only).
    let main = Writer::open_writable(&data_dir.join("quilltap.db"), pepper_b64)?;
    exec_ddl(main.connection(), &schema.main)?;
    seed_main(&main)?;
    builtin_templates::seed_built_in_templates(main.connection())?;

    // Mount-index sibling: schema, then the three built-in mount stores (family 2)
    // — which span both partitions (pointers in main, rows/folders here). Kept
    // open alongside `main` so the provisioner can write both.
    let mount_index = Writer::open_writable(&data_dir.join("quilltap-mount-index.db"), pepper_b64)?;
    exec_ddl(mount_index.connection(), &schema.mount_index)?;
    builtin_mounts::ensure_builtin_mounts(main.connection(), mount_index.connection())?;
    drop(mount_index);
    drop(main);

    // llm-logs sibling: schema only.
    let llm_logs = Writer::open_writable(&data_dir.join("quilltap-llm-logs.db"), pepper_b64)?;
    exec_ddl(llm_logs.connection(), &schema.llm_logs)?;
    drop(llm_logs);

    Ok(())
}

/// Replay one partition's DDL in a single transaction (tables then indexes, the
/// order the dumper emits — always valid).
fn exec_ddl(conn: &Connection, statements: &[String]) -> Result<(), ProvisionError> {
    let tx = conn.unchecked_transaction()?;
    for sql in statements {
        tx.execute_batch(sql)?;
    }
    tx.commit()?;
    Ok(())
}

/// Seed the main partition: the single user, its chat settings, and the default
/// embedding profile — v4's deterministic first-boot seed.
fn seed_main(writer: &Writer) -> Result<(), ProvisionError> {
    let now = clock::now_iso();

    // The single user (v4 getOrCreateSingleUser: fixed id, localUser/Local User).
    writer.users().create(
        &users::UserCreate {
            username: "localUser".to_string(),
            email: Some("user@localhost.localdomain".to_string()),
            name: Some("Local User".to_string()),
            image: None,
            email_verified: None,
            password_hash: None,
        },
        &users::CreateOptions {
            id: SINGLE_USER_ID.to_string(),
            created_at: now.clone(),
            updated_at: now.clone(),
        },
    )?;

    // The default TF-IDF embedding profile (v4 seedEmbeddingProfiles).
    writer.embedding_profiles().create(
        &embedding_profiles::EpCreate {
            user_id: SINGLE_USER_ID.to_string(),
            name: "Built-in TF-IDF".to_string(),
            provider: "BUILTIN".to_string(),
            api_key_id: None,
            base_url: None,
            model_name: "tfidf-bm25-v1".to_string(),
            dimensions: None,
            truncate_to_dimensions: None,
            normalize_l2: true,
            is_default: true,
            tags: Vec::new(),
        },
        &embedding_profiles::CreateOptions {
            id: uuid::Uuid::new_v4().to_string(),
            created_at: now.clone(),
            updated_at: now.clone(),
        },
    )?;

    // The default chat settings — replayed byte-exact from v4's captured row
    // (the omission seam above); mint id + timestamps, fix userId.
    seed_chat_settings(writer.connection(), &now)?;

    Ok(())
}

/// Raw INSERT of the captured `chat_settings` seed row (see the module doc's
/// chat_settings seam). Binds each captured column value by SQL affinity.
fn seed_chat_settings(conn: &Connection, now: &str) -> Result<(), ProvisionError> {
    let seed: ChatSettingsSeed = serde_json::from_str(CHAT_SETTINGS_SEED_JSON)
        .map_err(|e| ProvisionError::Artifact(format!("chat_settings_seed.json: {e}")))?;

    let quoted: Vec<String> = seed.columns.iter().map(|c| format!("\"{c}\"")).collect();
    let col_list = format!(
        "\"id\", \"userId\", {}, \"createdAt\", \"updatedAt\"",
        quoted.join(", ")
    );
    let n = seed.columns.len() + 4;
    let placeholders = (1..=n)
        .map(|i| format!("?{i}"))
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!("INSERT INTO chat_settings ({col_list}) VALUES ({placeholders})");

    let mut values: Vec<SqlValue> = Vec::with_capacity(n);
    values.push(SqlValue::Text(uuid::Uuid::new_v4().to_string()));
    values.push(SqlValue::Text(SINGLE_USER_ID.to_string()));
    for col in &seed.columns {
        let v = seed.values.get(col).ok_or_else(|| {
            ProvisionError::Artifact(format!("chat_settings_seed.json missing value for {col}"))
        })?;
        values.push(json_to_sql(v));
    }
    values.push(SqlValue::Text(now.to_string()));
    values.push(SqlValue::Text(now.to_string()));

    conn.execute(&sql, params_from_iter(values.iter()))?;
    Ok(())
}

/// Bind a captured JSON scalar to a SQLite value by its natural affinity. The
/// captured columns are already SQL-shaped: TEXT/JSON columns are strings,
/// INTEGER columns (`sidebarWidth`, the 0/1 booleans) are integers, absent
/// nullables are `null`.
fn json_to_sql(v: &serde_json::Value) -> SqlValue {
    match v {
        serde_json::Value::Null => SqlValue::Null,
        serde_json::Value::Bool(b) => SqlValue::Integer(i64::from(*b)),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                SqlValue::Integer(i)
            } else {
                SqlValue::Real(n.as_f64().unwrap_or(0.0))
            }
        }
        serde_json::Value::String(s) => SqlValue::Text(s.clone()),
        // JSON columns are captured as their serialized STRING, never a nested
        // array/object — but be total: re-serialize if one ever appears.
        other => SqlValue::Text(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    const PEPPER: &str = "3q2+796tvu/erb7v3q2+796tvu/erb7v3q2+796tvu8=";

    #[test]
    fn provisions_a_bootable_seeded_instance() {
        let dir = tempdir().unwrap();
        let data = dir.path().join("data");
        std::fs::create_dir_all(&data).unwrap();

        provision_fresh_instance(&data, PEPPER).unwrap();

        // All three partition files exist.
        assert!(data.join("quilltap.db").exists());
        assert!(data.join("quilltap-mount-index.db").exists());
        assert!(data.join("quilltap-llm-logs.db").exists());

        // Reopen the main partition read-write and check the schema + seed.
        let main = Writer::open_writable(&data.join("quilltap.db"), PEPPER).unwrap();
        let conn = main.connection();

        // A representative sample of tables exists.
        for t in [
            "users",
            "chats",
            "chat_messages",
            "chat_settings",
            "embedding_profiles",
        ] {
            let n: i64 = conn
                .query_row(
                    "SELECT count(*) FROM sqlite_master WHERE type='table' AND name=?1",
                    [t],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(n, 1, "table {t} missing");
        }

        // The single user + its chat settings + the default embedding profile.
        let users: i64 = conn
            .query_row(
                "SELECT count(*) FROM users WHERE id=?1",
                [SINGLE_USER_ID],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(users, 1);
        let settings: i64 = conn
            .query_row(
                "SELECT count(*) FROM chat_settings WHERE userId=?1",
                [SINGLE_USER_ID],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(settings, 1);
        let ep: i64 = conn
            .query_row(
                "SELECT count(*) FROM embedding_profiles WHERE provider='BUILTIN' AND isDefault=1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(ep, 1);
        // chats is empty (a fresh instance: listChats -> []).
        let chats: i64 = conn
            .query_row("SELECT count(*) FROM chats", [], |r| r.get(0))
            .unwrap();
        assert_eq!(chats, 0);

        // The mount-index + llm-logs partitions carry their schema.
        let mi = Writer::open_writable(&data.join("quilltap-mount-index.db"), PEPPER).unwrap();
        let midoc: i64 = mi
            .connection()
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='doc_mount_points'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(midoc, 1);
        let ll = Writer::open_writable(&data.join("quilltap-llm-logs.db"), PEPPER).unwrap();
        let lltab: i64 = ll
            .connection()
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='llm_logs'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(lltab, 1);
    }
}
