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
//! - **Both v4 index families** (P4.153, dogfood #149): the generateDDL indexes
//!   above PLUS the ones v4's MIGRATIONS make on every first boot before any
//!   repository runs ([`MIGRATION_INDEXES_JSON`], dumped from v4's real
//!   `MigrationRunner`) — so a fresh instance plans `chat_messages`,
//!   `memories`, `chats`, … reads the way a migrated v4 instance does, and the
//!   UNIQUE ones refuse what v4 refuses. The boot's own index ensures
//!   (`idx_files_generationKey`, `idx_folders_userId_projectId_path`, …) find
//!   theirs already made.
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
///
/// Re-dump register (append, never restructure): `1766701c2` (P4.D135),
/// `e30acf4e3` (P4.D78), `8330d3e79` (P4.D77), `0871733bb` (P4.D73),
/// `0a26dadc5` (P4.D49), `78b381a96` (P4.D171 — `chat_messages.routeTrail` +
/// `chats.cycleOrderParticipantIds`, one dump landing both), `31436bae4`
/// (P4.D182 — `files.generationKey`, the round's ONE re-dump column).
///
/// ## The `31436bae4` round's OTHER column is deliberately absent
///
/// That round also added `chats."transcriptVersion" INTEGER DEFAULT 0` (v4
/// `5029075bb`, migration `add-transcript-version-column-v1`), and it is NOT
/// in this dump — correctly. v4 keeps `transcriptVersion` out of
/// `ChatMetadataSchema`/`ChatMetadataBaseSchema` on purpose (two identical
/// comment blocks say so): a counter carried as a schema field would be
/// rewound by `_update`'s whole-row rewrite, so Zod strips it from every write
/// and `SET v = v + 1` is its only writer. `generateDDL` walks the schema, so
/// it never emits the column, and this dump cannot carry it. It arrives by
/// boot ensure only — `db::chats_transcript_version_repair` — the same
/// shape P4.D145's bug-114 unique index took for the opposite reason
/// (`generateDDL` could not express THAT one either).
///
/// `files.generationKey` IS in v4's `FileEntrySchema`, so it lands here; its
/// index `idx_files_generationKey` is NOT (plain indexes belong to v4's
/// migration), so `db::files_generation_key_repair` creates both.
///
/// `4d370a90f` (P4.D226 — #75, the three Concierge states; `dump-fresh-schema.ts`
/// run FROM the pin): EXACTLY two statements move. (1) `chats` gains
/// `"conciergeMode" TEXT`, `"conciergeModeSetBy" TEXT`, `"conciergeModeReason"
/// TEXT` right after `"conciergeOverride" TEXT` — nullable, NO DEFAULT (the
/// Zod fields carry no `.default()`; the `TEXT DEFAULT 'moderated'` in v4's
/// `add-chat-concierge-mode-v1` is the MIGRATION's, on migrated instances only).
/// (2) `chat_settings."dangerousContentSettings"`'s DDL default gains
/// `"autoSwitchAfterRefusals":2` — P4.D225's `49059fb14` handoff, which this
/// pin carries too. `conciergeOverride` is STILL declared at this pin (#76
/// deletes it — P4.D227's re-dump). The E.1 anomaly (a live
/// `extractSchemaMetadata` probe omitting the declared `conciergeOverride`)
/// MEASURED against the real dumper: it does not reproduce — the dump names the
/// column, as P4.D225 also found at `49059fb14`. The seed is UNMOVED at this pin
/// (`cmp`-identical). The ledger columns stay absent (schema-absent by design —
/// P4.D225's boot ensure is their only source).
///
/// `3b463d6b1` (P4.D227 — #76, the Concierge's own settings; run FROM the
/// pin): EXACTLY three lines move, all as measured. (1) `chats` LOSES
/// `"conciergeOverride" TEXT` (v4 deleted it from both chat schemas; its
/// migration `drop-chat-concierge-override-v1` drops the column on migrated
/// instances — v5 never drops it, it simply stops binding it, so either shape
/// opens). (2) `chat_settings` LOSES `"uncensoredImageDescriptionProfileId"
/// TEXT`. (3) `"dangerousContentSettings" TEXT DEFAULT '…'` is REPLACED IN
/// PLACE by `"conciergeSettings" TEXT DEFAULT '{"enabled":true,
/// "autoSwitchAfterRefusals":2,"newChatsStartAs":"moderated","display":
/// {"mode":"SHOW","showWarningBadges":true},"preScreen":{"enabled":false,
/// "threshold":0.7,"scanTextChat":true,"scanImagePrompts":true,
/// "scanImageGeneration":false,"summaryClassification":false}}'` — the Zod
/// `.default()` literal, WITHOUT the four desk ids and the prompt. #76's own
/// prose ("drops NO old column") holds for MIGRATED instances only: a fresh
/// DDL has neither legacy column. The seed moved with it (below).
///
/// `f7f3d7bf0` (P4.D235 — keep conversation embeddings warm; render
/// transcripts on demand; run FROM the pin): EXACTLY one line moves, as
/// measured. `chats` LOSES `"renderedMarkdown" TEXT` (v4 deleted it from both
/// chat schemas; `drop-chat-rendered-markdown-v1` drops the column on migrated
/// instances — v5 never drops it, it stops binding it, so either shape opens,
/// exactly the `conciergeOverride` treatment). v4's hand-written
/// `sqlite-initial-schema.ts` still CREATES the column, which is why a v4
/// instance born from `SQLITE_TABLES` shows the drop in its migration log; the
/// `generateDDL` surface this module is built from never has it. The seed is
/// UNMOVED at this pin (`cmp`-identical).
///
/// `52d6e7ecd` (P4.D249 — D23 re-dump #4, v4 "Inform: standing (per-chat)
/// informs", migration `add-chat-informs-permanent-v1`; run FROM the pin,
/// 2026-10-03): EXACTLY one line moves, as measured. `chat_informs` GAINS
/// `"permanent" INTEGER DEFAULT 0` between `"recordMessageId"` and
/// `"createdAt"` — generateDDL's spelling of `z.boolean().default(false)`,
/// in schema order and WITHOUT the `NOT NULL` the migration's
/// `ADD COLUMN "permanent" INTEGER NOT NULL DEFAULT 0` carries (the two v4
/// shapes disagree; the migration's is re-homed as
/// `db::chat_informs_permanent_repair`, this one is `CHAT_INFORMS_TABLE_DDL`).
/// The seed is UNMOVED at this pin (`cmp`-identical).
///
/// `07b8f0209` (P4.D251 — D23 re-dump #5, v4 "Impersonated-line voice: three
/// modes, no model call until asked", migration `impersonation-voice-mode-v1`;
/// run FROM the pin, 2026-10-05): EXACTLY one line moves, as measured.
/// `chat_settings`'s `"impersonationVoiceRewrite" INTEGER DEFAULT 0` (the
/// `686954937` boolean, P4.D179) is REPLACED in place by
/// `"impersonationVoiceMode" TEXT DEFAULT 'off'` — generateDDL's spelling of
/// `ImpersonationVoiceModeEnum.default('off')`, at the same position between
/// `composerUnicode` and `textReplacementsEnabled`; the mount-index and
/// llm-logs partitions `cmp`-identical. ONE shape this time: v4's migration
/// (`addColumnIfMissing(…, "TEXT DEFAULT 'off'")`, re-homed as
/// `db::chat_settings_impersonation_voice_mode_repair`, which REPLACES the
/// P4.D179 ensure) spells the same clause, differing only in position
/// (appended). The seed MOVED with it (below).
static FRESH_SCHEMA_JSON: &str = include_str!("fresh_schema.json");

/// The captured `chat_settings` seed row's columns (all but the minted
/// id/userId/timestamps). Regenerate alongside the schema (`QT_SEED_OUT=…`).
///
/// Seed-only re-dump register (append): `49059fb14` (P4.D225 — the seed's
/// `dangerousContentSettings` gains `"autoSwitchAfterRefusals":2`, v4 #74's
/// schema default; the SAME dump's `fresh_schema.json` differs from the
/// committed one on exactly one line — that column's DDL `DEFAULT` carrying
/// the same key — and was deliberately NOT taken: the D23 schema re-dump is
/// P4.D226's this round, at its own pin. `provisioning_equivalence` is red at
/// `49059fb14` on that one line until it lands.)
/// `3b463d6b1` (P4.D227): the seed row loses `uncensoredImageDescriptionProfileId`
/// and trades `dangerousContentSettings` for `conciergeSettings` =
/// `DEFAULT_CONCIERGE_SETTINGS` serialized in its OWN key order — every desk id
/// and `customClassificationPrompt` present as `null` (the repository's default
/// row is the resolver's constant, not the schema's `.default()` literal, so the
/// seeded bytes and the DDL DEFAULT genuinely differ). Measured, not predicted.
/// `07b8f0209` (P4.D251): the seed row's `"impersonationVoiceRewrite": 0`
/// becomes `"impersonationVoiceMode": "off"` (the column entry AND the value
/// move together) — v4's repository default `impersonationVoiceMode: 'off'`
/// (`chat-settings.repository.ts:211`), as captured; nothing else moved.
static CHAT_SETTINGS_SEED_JSON: &str = include_str!("chat_settings_seed.json");

/// P4.153 (dogfood #149): v4's OTHER index family — the one its MIGRATIONS
/// make in PHASE 1 of every first boot, before any repository runs
/// (`idx_chat_messages_chatId`, `idx_chats_projectId`, `idx_memories_*`, the
/// UNIQUE `idx_connection_profiles_userId_name` / `idx_chat_documents_unique` /
/// `idx_group_character_members_group_char` / …). [`FRESH_SCHEMA_JSON`] is the
/// generateDDL surface and carries none of them, so until P4.153 a fresh v5
/// instance scanned `chat_messages` once per restored message (2 h 26 m
/// against 9 m into a migrated copy).
///
/// Dumped — never hand-written (D23) — by
/// `harness/oracle/provision/dump-migration-indexes.ts`, which runs v4's REAL
/// `MigrationRunner` over an empty data dir and keeps every index
/// `fresh_schema.json` does not already name, on a table `fresh_schema.json`
/// creates (the legacy `wardrobe_items` one is left out). A name both families
/// make keeps the generateDDL copy — EXCEPT where v4's migration makes it
/// UNIQUE and generateDDL does not (`idx_doc_mount_folders_mp_path`): the dump
/// carries the UNIQUE one and [`exec_ddl`] skips the plain copy, so a fresh
/// instance refuses a duplicate `(mountPointId, path)` folder row as every v4
/// instance does (ruled 2026-10-06; v5's own `builtin_mounts` ensure already
/// asked for UNIQUE, a silent no-op behind the plain copy until then). The file's `source` block names the v4
/// commit. Statements are `sqlite_master` text — no `IF NOT EXISTS` — so they
/// replay on a FRESH file only, which [`provision_fresh_instance`] enforces.
///
/// Re-dump register (append): `94fbb1ae3` (P4.153 — first dump: main 50 /
/// mount-index 5 (incl. the UNIQUE `idx_doc_mount_folders_mp_path`) /
/// llm-logs 5, cross-checked name-and-SQL against a REAL
/// `tsx server.ts` first boot at the same pin, zero differences).
static MIGRATION_INDEXES_JSON: &str = include_str!("migration_indexes.json");

/// One artifact's statements per partition (`fresh_schema.json` and
/// `migration_indexes.json` share the shape; the latter's `source` block is
/// ignored).
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
    /// P4.46: the data dir already carries instance files, so this is not a
    /// first run. Carries the offending file names, in [`INSTANCE_FILES`] order.
    AlreadyProvisioned(Vec<&'static str>),
}

impl std::fmt::Display for ProvisionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProvisionError::Db(e) => write!(f, "provisioning database error: {e}"),
            ProvisionError::Artifact(m) => write!(f, "provisioning artifact error: {m}"),
            ProvisionError::AlreadyProvisioned(names) => write!(
                f,
                "this instance is already provisioned ({} present) — \
                 refusing to provision over it",
                names.join(", ")
            ),
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

/// Every file first-run setup brings into being, in the order a refusal names
/// them: the key file the engine writes, then the three partitions this module
/// creates.
pub const INSTANCE_FILES: [&str; 4] = [
    "quilltap.dbkey",
    "quilltap.db",
    "quilltap-mount-index.db",
    "quilltap-llm-logs.db",
];

/// Which of [`INSTANCE_FILES`] already exist under `data_dir` — empty means a
/// genuine first run (P4.46).
///
/// The engine's `Setup` arm consults this BEFORE minting a pepper: a retry
/// after a partly-completed setup would otherwise mint a NEW pepper and
/// overwrite `quilltap.dbkey` with it, and the new pepper cannot open the
/// partitions the first attempt already created — the original pepper is gone
/// and the instance is bricked. There is no v4 analog (v4's setup converts an
/// existing plaintext instance rather than creating one), so this is deliberate
/// v5 hardening.
pub fn existing_instance_files(data_dir: &Path) -> Vec<&'static str> {
    INSTANCE_FILES
        .into_iter()
        .filter(|name| data_dir.join(name).exists())
        .collect()
}

/// Create and seed a fresh instance under `data_dir`, keyed by `pepper_b64` —
/// all three partitions encrypted from creation. The caller (the engine's
/// `Setup`) has already minted the pepper and written `quilltap.dbkey`, and has
/// already claimed the instance lock (P4.46: no partition is created before the
/// host holds the lock).
///
/// The three partition files MUST NOT already exist (this is first-run setup) —
/// enforced here, not merely documented, because the DDL uses no
/// `IF NOT EXISTS` and a replay over a live instance would fail halfway through
/// its own transaction. Each partition is created by `Writer::open_writable`
/// and its schema replayed in one transaction. Only the main partition carries
/// seed rows.
pub fn provision_fresh_instance(data_dir: &Path, pepper_b64: &str) -> Result<(), ProvisionError> {
    // `quilltap.dbkey` is EXCLUDED from this check on purpose: the caller wrote
    // it moments ago. The partitions are the ones that must not exist.
    let present: Vec<&'static str> = existing_instance_files(data_dir)
        .into_iter()
        .filter(|name| *name != "quilltap.dbkey")
        .collect();
    if !present.is_empty() {
        return Err(ProvisionError::AlreadyProvisioned(present));
    }

    let schema: FreshSchema = serde_json::from_str(FRESH_SCHEMA_JSON)
        .map_err(|e| ProvisionError::Artifact(format!("fresh_schema.json: {e}")))?;
    let migration_indexes: FreshSchema = serde_json::from_str(MIGRATION_INDEXES_JSON)
        .map_err(|e| ProvisionError::Artifact(format!("migration_indexes.json: {e}")))?;

    // Main partition: schema + seed rows + the built-in roleplay templates
    // (family 1, main-only).
    let main = Writer::open_writable(&data_dir.join("quilltap.db"), pepper_b64)?;
    exec_ddl(main.connection(), &schema.main, &migration_indexes.main)?;
    seed_main(&main)?;
    builtin_templates::seed_built_in_templates(main.connection())?;

    // Mount-index sibling: schema, then the three built-in mount stores (family 2)
    // — which span both partitions (pointers in main, rows/folders here). Kept
    // open alongside `main` so the provisioner can write both.
    let mount_index = Writer::open_writable(&data_dir.join("quilltap-mount-index.db"), pepper_b64)?;
    exec_ddl(
        mount_index.connection(),
        &schema.mount_index,
        &migration_indexes.mount_index,
    )?;
    builtin_mounts::ensure_builtin_mounts(main.connection(), mount_index.connection())?;
    drop(mount_index);
    drop(main);

    // llm-logs sibling: schema only.
    let llm_logs = Writer::open_writable(&data_dir.join("quilltap-llm-logs.db"), pepper_b64)?;
    exec_ddl(
        llm_logs.connection(),
        &schema.llm_logs,
        &migration_indexes.llm_logs,
    )?;
    drop(llm_logs);

    Ok(())
}

/// Replay one partition's DDL in a single transaction: `fresh_schema.json`'s
/// statements (tables then indexes, the order the dumper emits) and then
/// `migration_indexes.json`'s (indexes on those tables).
///
/// A name BOTH artifacts carry is the migration's: the dumper keeps a shared
/// name only where v4's migration makes it UNIQUE and generateDDL does not
/// (`idx_doc_mount_folders_mp_path` — a real v4 boot runs its migrations first,
/// so the UNIQUE index is the one every v4 instance has; ruled 2026-10-06), so
/// the generateDDL copy of that name is skipped here. Every other name is in
/// exactly one artifact, and their relative order is immaterial.
fn exec_ddl(
    conn: &Connection,
    fresh: &[String],
    migration: &[String],
) -> Result<(), ProvisionError> {
    let migration_names: std::collections::HashSet<&str> =
        migration.iter().filter_map(|sql| index_name(sql)).collect();
    let tx = conn.unchecked_transaction()?;
    for sql in fresh
        .iter()
        .filter(|sql| index_name(sql).is_none_or(|n| !migration_names.contains(n)))
        .chain(migration)
    {
        tx.execute_batch(sql)?;
    }
    tx.commit()?;
    Ok(())
}

/// The index a `CREATE [UNIQUE] INDEX [IF NOT EXISTS] name …` statement makes
/// (quotes stripped); `None` for anything else.
fn index_name(sql: &str) -> Option<&str> {
    let rest = sql.strip_prefix("CREATE ")?;
    let rest = rest.strip_prefix("UNIQUE ").unwrap_or(rest);
    let rest = rest.strip_prefix("INDEX ")?;
    let rest = rest.strip_prefix("IF NOT EXISTS ").unwrap_or(rest);
    rest.split([' ', '(']).next().map(|n| n.trim_matches('"'))
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
