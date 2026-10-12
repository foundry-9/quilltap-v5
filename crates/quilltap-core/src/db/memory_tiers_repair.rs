//! The memory-tiers boot ensure (v4 `d58548051`, migration
//! `add-memory-tiers-v1` — "the Commonplace Book gets a cellar";
//! `dependsOn: ['add-episodic-memory-fields-v1']`, registered LAST in v4's
//! registry, NOT `resumable`).
//!
//! v4 adds four `memories` columns (`tier`, `supersededById`,
//! `consolidatedFrom`, `consolidatedAt`), the `(characterId, tier)` index and
//! `chats.otherExtractionWatermarkMessageId` through its migration runner, and
//! heals any row whose `tier` / `consolidatedFrom` is NULL. v5's migration
//! runner is a locked deferral, so — the P4.d7 / P4.D249 / P4.D251 sibling
//! column ensures' shape — the migration is re-homed as a boot ensure over the
//! main partition, wired in the host's `seed_built_ins` AFTER the P4.D255
//! wardrobe ensures (v4's registration order) and BEFORE the P4.160
//! migration-index backfill (which would otherwise try
//! `idx_memories_character_tier` on a table without `tier` and log
//! `Failed to backfill a migration-created index`).
//!
//! [`ensure_memory_tiers`] follows v4's `assessWork` / `shouldRun` / `run`
//! exactly (`migrations/scripts/add-memory-tiers-v1.ts:53-176`):
//!
//!   1. each missing `memories` column, in v4's `MEMORY_COLUMNS` order, with
//!      the MIGRATION's DDL text ([`MEMORY_COLUMNS`], `addColumnIfMissing`'s
//!      `ALTER TABLE "memories" ADD COLUMN "<col>" <ddl>`);
//!   2. the index when absent — its text from `migration_indexes.json`
//!      ([`crate::services::provisioning::migration_main_index`], `IF NOT
//!      EXISTS` spliced after `INDEX `, the P4.160 splice) — never spelled
//!      here (D23);
//!   3. whenever `memories.tier` exists, the heal UPDATE, byte for byte;
//!   4. the chat column when absent, `TEXT DEFAULT NULL`.
//!
//! Every step is table-gated (`sqliteTableExists`). **No transaction wraps
//! the steps** — v4's has none, so a mid-run failure leaves the earlier ALTERs
//! in place, as v4's does. A failure is FATAL to boot (the caller `?`s it):
//! v4's migration is not `resumable`, so its failure stops v4's startup
//! (`migrations/index.ts:200-217`) — the sibling column ensures' shape.
//!
//! ## The two v4 shapes (R-B)
//!
//! Measured at the D23 re-dump #7 from the `01a83539d` pin: **generateDDL**
//! (`fresh_schema.json`) puts the five columns in their schema slots with bare
//! `TEXT` on the three nullable ones; **the migration** APPENDS them with
//! `TEXT DEFAULT NULL` (`supersededById` / `consolidatedAt` cids 25–28 on a
//! real first boot, the chat column LAST). The difference is cosmetic
//! (`DEFAULT NULL` ≡ no default; the defaults that matter — `'hot'`, `'[]'` —
//! agree; `notnull` 0 everywhere). A fresh v5 instance keeps the generateDDL
//! surface (the P4.153 / 2026-10-08 ruling) and this ensure is then a no-op;
//! a pre-round instance gets the migration's shape, as a v4 upgrade does.
//! v5 names its columns explicitly on every read and write, so column ORDER is
//! harmless.
//!
//! v4's header comment (`:73-74`, "a fresh database whose table came from the
//! Zod DDL has no column default") is FALSE — generateDDL gives `tier` /
//! `consolidatedFrom` their defaults (measured). NULLs arise only from writes
//! that bind NULL explicitly.
//!
//! ## No ledger stamp (R-A) — and the recorded consequence
//!
//! v4's migration is idempotent (column ALTERs gated, `CREATE INDEX IF NOT
//! EXISTS`, a COALESCE heal that is a no-op on healed rows), so a v4 boot over
//! a v5-ensured instance finds `shouldRun` false (or runs a no-op heal) and
//! records its own row — no double-application hazard, unlike P4.D255's seed.
//! This ensure therefore neither reads nor writes `migrations_state`. The
//! consequence: v4's runner consults its ledger BEFORE `shouldRun`
//! (`migrations/index.ts`), so once v4 has recorded the migration (live Friday
//! did, 2026-10-08 22:46Z) v4 never heals a NULL-tier row again, while this
//! unledgered ensure re-heals on every boot whenever the NULL count is
//! non-zero. Convergent and harmless — the heal writes v4's own defaults, and
//! no v4 output differs on any state v4 itself produces.
//!
//! ## NO-PORT (R-J)
//!
//! v4's migration lines — the INFO `Added memories.<col> column`, `Created
//! idx_memories_character_tier`, `Stamped NULL-tier memories hot` `{rows}`,
//! `Added chats.otherExtractionWatermarkMessageId column`, the ERROR
//! `Memory-tiers migration failed`, `database-utils.ts`'s DEBUG `Adding
//! column`, the RUNNER's `Migration completed successfully` / `Migration
//! failed`, its `migrations_state` row and its pretty label (`Fitting the
//! Commonplace Book with a cellar…`, `lib/startup/prettify.ts:159`) — belong to
//! the deferred migration runner and have no v5 analog (the sixth such
//! recording: P4.D63 / P4.D73 / P4.D79 / P4.D249 / P4.D251 / P4.D255). This
//! module emits NO line; the host's fence logs v5's own ONE boot INFO when the
//! outcome is not all-false (the P4.D255 host-block idiom).

use rusqlite::Connection;

use super::DbError;

/// v4's migration id (`add-memory-tiers-v1.ts:32`).
pub const MIGRATION_ID: &str = "add-memory-tiers-v1";

/// v4's `INDEX_NAME` (`:34`) — the statement itself is read from
/// `migration_indexes.json`.
pub const INDEX_NAME: &str = "idx_memories_character_tier";

/// v4's `MEMORY_COLUMNS` (`:37-42`), in the order they are added, with the
/// MIGRATION's DDL text verbatim (never `fresh_schema.json`'s).
pub const MEMORY_COLUMNS: [(&str, &str); 4] = [
    ("tier", "TEXT DEFAULT 'hot'"),
    ("supersededById", "TEXT DEFAULT NULL"),
    ("consolidatedFrom", "TEXT DEFAULT '[]'"),
    ("consolidatedAt", "TEXT DEFAULT NULL"),
];

/// v4's `CHAT_WATERMARK_COLUMN` (`:44`) and its DDL (`:147`).
pub const CHAT_WATERMARK_COLUMN: &str = "otherExtractionWatermarkMessageId";
const CHAT_WATERMARK_DECL: &str = "TEXT DEFAULT NULL";

/// v4's heal (`:132-135`), the bytes copied — line breaks and indentation
/// included.
const HEAL_SQL: &str = "UPDATE memories
                SET tier = COALESCE(tier, 'hot'),
                    consolidatedFrom = COALESCE(consolidatedFrom, '[]')
              WHERE tier IS NULL OR consolidatedFrom IS NULL";

/// What one [`ensure_memory_tiers`] run did. All-false / empty / zero means
/// v4's `shouldRun` answered false and nothing was touched.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MemoryTiersEnsureOutcome {
    /// The `memories` columns added, in add order (v4's `MEMORY_COLUMNS`).
    pub columns_added: Vec<&'static str>,
    /// `idx_memories_character_tier` was created.
    pub index_created: bool,
    /// Rows the heal UPDATE changed (v4's `healed.changes`).
    pub healed_rows: usize,
    /// `chats.otherExtractionWatermarkMessageId` was added.
    pub chat_column_added: bool,
}

impl MemoryTiersEnsureOutcome {
    /// True when the run changed nothing — the boot logs no line then.
    pub fn is_noop(&self) -> bool {
        self == &Self::default()
    }

    /// v4's `itemsAffected` (columns + index + healed rows + chat column).
    pub fn items_affected(&self) -> usize {
        self.columns_added.len()
            + usize::from(self.index_created)
            + self.healed_rows
            + usize::from(self.chat_column_added)
    }
}

/// v4's `WorkNeeded` (`:46-51`).
struct WorkNeeded {
    memory_columns: Vec<&'static str>,
    needs_index: bool,
    needs_chat_watermark: bool,
    null_tier_rows: i64,
}

/// v4's `assessWork()` (`:53-87`).
fn assess_work(main: &Connection) -> Result<WorkNeeded, DbError> {
    let mut work = WorkNeeded {
        memory_columns: Vec::new(),
        needs_index: false,
        needs_chat_watermark: false,
        null_tier_rows: 0,
    };
    if table_exists(main, "memories")? {
        let columns = column_names(main, "memories")?;
        for (name, _) in MEMORY_COLUMNS {
            if !columns.iter().any(|c| c == name) {
                work.memory_columns.push(name);
            }
        }
        work.needs_index = !index_exists(main, INDEX_NAME)?;
        if columns.iter().any(|c| c == "tier") {
            work.null_tier_rows = main.query_row(
                "SELECT COUNT(*) AS n FROM memories WHERE tier IS NULL OR consolidatedFrom IS NULL",
                [],
                |r| r.get(0),
            )?;
        }
    }
    if table_exists(main, "chats")? {
        work.needs_chat_watermark = !column_names(main, "chats")?
            .iter()
            .any(|c| c == CHAT_WATERMARK_COLUMN);
    }
    Ok(work)
}

/// Run v4's `add-memory-tiers-v1` against the main partition when its
/// `shouldRun` would (`:96-105`) — idempotent, table-gated, unstamped, silent
/// (see the module doc). A fully-migrated instance (live Friday, a fresh v5
/// provision) answers the all-false outcome after four read-only probes.
pub fn ensure_memory_tiers(main: &Connection) -> Result<MemoryTiersEnsureOutcome, DbError> {
    let work = assess_work(main)?;
    let should_run = !work.memory_columns.is_empty()
        || work.needs_index
        || work.needs_chat_watermark
        || work.null_tier_rows > 0;
    let mut outcome = MemoryTiersEnsureOutcome::default();
    if !should_run {
        return Ok(outcome);
    }

    // (1) Each missing `memories` column, through `addColumnIfMissing`.
    for (name, decl) in MEMORY_COLUMNS {
        if !work.memory_columns.contains(&name) {
            continue;
        }
        if add_column_if_missing(main, "memories", name, decl)? {
            outcome.columns_added.push(name);
        }
    }

    // (2) The index — `work.needsIndex && sqliteTableExists('memories')`.
    if work.needs_index && table_exists(main, "memories")? {
        let sql = crate::services::provisioning::migration_main_index(INDEX_NAME)
            .map_err(|e| DbError::Internal(e.to_string()))?
            .ok_or_else(|| {
                DbError::Internal(format!("migration_indexes.json carries no {INDEX_NAME}"))
            })?;
        main.execute_batch(&sql.replacen("INDEX ", "INDEX IF NOT EXISTS ", 1))?;
        outcome.index_created = true;
    }

    // (3) The heal, whenever `memories.tier` exists (re-read: step 1 may have
    // just added it, as v4's `sqliteColumnExists` re-checks).
    if table_exists(main, "memories")?
        && column_names(main, "memories")?.iter().any(|c| c == "tier")
    {
        outcome.healed_rows = main.execute(HEAL_SQL, [])?;
    }

    // (4) The chat watermark column.
    if work.needs_chat_watermark
        && add_column_if_missing(main, "chats", CHAT_WATERMARK_COLUMN, CHAT_WATERMARK_DECL)?
    {
        outcome.chat_column_added = true;
    }

    Ok(outcome)
}

/// v4's `addColumnIfMissing` (`migrations/lib/database-utils.ts:347-360`):
/// `ALTER TABLE "<table>" ADD COLUMN "<column>" <ddl>` unless the column is
/// already there; `true` when it added one. (Its DEBUG `Adding column` is
/// NO-PORT — the module doc.)
fn add_column_if_missing(
    conn: &Connection,
    table: &str,
    column: &str,
    decl: &str,
) -> Result<bool, DbError> {
    if column_names(conn, table)?.iter().any(|c| c == column) {
        return Ok(false);
    }
    conn.execute_batch(&format!(
        "ALTER TABLE \"{table}\" ADD COLUMN \"{column}\" {decl}"
    ))?;
    Ok(true)
}

fn table_exists(conn: &Connection, name: &str) -> Result<bool, DbError> {
    let mut stmt =
        conn.prepare("SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1")?;
    Ok(stmt.exists([name])?)
}

/// v4's `SELECT name FROM sqlite_master WHERE type = 'index' AND name = ?`.
fn index_exists(conn: &Connection, name: &str) -> Result<bool, DbError> {
    let mut stmt =
        conn.prepare("SELECT name FROM sqlite_master WHERE type = 'index' AND name = ?1")?;
    Ok(stmt.exists([name])?)
}

fn column_names(conn: &Connection, table: &str) -> Result<Vec<String>, DbError> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info(\"{table}\")"))?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::captured_with;

    /// The pre-round (`f5e953a3f` generateDDL) `memories` — `fresh_schema.json`'s
    /// statement minus exactly the four moved lines.
    fn baseline_memories_ddl() -> String {
        let current = fresh_statement("CREATE TABLE \"memories\"");
        let mut out = current.clone();
        for line in [
            "  \"tier\" TEXT DEFAULT 'hot',\n",
            "  \"supersededById\" TEXT,\n",
            "  \"consolidatedFrom\" TEXT DEFAULT '[]',\n",
            "  \"consolidatedAt\" TEXT,\n",
        ] {
            assert_eq!(out.matches(line).count(), 1, "{line:?} in {current}");
            out = out.replacen(line, "", 1);
        }
        out
    }

    /// The pre-round `chats` — minus the one moved line.
    fn baseline_chats_ddl() -> String {
        let current = fresh_statement("CREATE TABLE \"chats\"");
        let line = "  \"otherExtractionWatermarkMessageId\" TEXT,\n";
        assert_eq!(current.matches(line).count(), 1);
        current.replacen(line, "", 1)
    }

    fn fresh_statement(prefix: &str) -> String {
        let schema: serde_json::Value =
            serde_json::from_str(include_str!("../services/provisioning/fresh_schema.json"))
                .unwrap();
        schema["main"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|v| v.as_str())
            .find(|s| s.starts_with(prefix))
            .unwrap()
            .to_string()
    }

    fn pre_round() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(&baseline_memories_ddl()).unwrap();
        conn.execute_batch(&baseline_chats_ddl()).unwrap();
        for id in ["m1", "m2"] {
            conn.execute(
                "INSERT INTO memories (id, characterId, content, summary, createdAt, updatedAt) \
                 VALUES (?1, 'c1', 'x', 'y', 't', 't')",
                [id],
            )
            .unwrap();
        }
        conn.execute(
            "INSERT INTO chats (id, userId, title, createdAt, updatedAt) \
             VALUES ('chat1', 'u1', 't', 't', 't')",
            [],
        )
        .unwrap();
        conn
    }

    fn columns(conn: &Connection, table: &str) -> Vec<(String, String, Option<String>)> {
        conn.prepare(&format!("PRAGMA table_info(\"{table}\")"))
            .unwrap()
            .query_map([], |r| Ok((r.get(1)?, r.get(2)?, r.get(4)?)))
            .unwrap()
            .map(Result::unwrap)
            .collect()
    }

    fn master(conn: &Connection) -> Vec<(String, Option<String>)> {
        conn.prepare("SELECT name, sql FROM sqlite_master ORDER BY name")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .map(Result::unwrap)
            .collect()
    }

    fn tiers(conn: &Connection) -> Vec<(String, Option<String>, Option<String>)> {
        conn.prepare("SELECT id, tier, consolidatedFrom FROM memories ORDER BY id")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap()
            .map(Result::unwrap)
            .collect()
    }

    #[test]
    fn a_pre_round_instance_gains_the_migration_shape() {
        let conn = pre_round();
        let outcome = ensure_memory_tiers(&conn).unwrap();
        assert_eq!(
            outcome,
            MemoryTiersEnsureOutcome {
                columns_added: vec![
                    "tier",
                    "supersededById",
                    "consolidatedFrom",
                    "consolidatedAt"
                ],
                index_created: true,
                healed_rows: 0,
                chat_column_added: true,
            }
        );
        assert_eq!(
            outcome.items_affected(),
            6,
            "v4's real first boot: 6 changes"
        );
        let mem = columns(&conn, "memories");
        // APPENDED, in the migration's text (not generateDDL's slot).
        assert_eq!(
            mem[mem.len() - 4..],
            [
                ("tier".into(), "TEXT".into(), Some("'hot'".into())),
                ("supersededById".into(), "TEXT".into(), Some("NULL".into())),
                (
                    "consolidatedFrom".into(),
                    "TEXT".into(),
                    Some("'[]'".into())
                ),
                ("consolidatedAt".into(), "TEXT".into(), Some("NULL".into())),
            ]
        );
        let chats = columns(&conn, "chats");
        assert_eq!(
            chats.last().unwrap(),
            &(
                CHAT_WATERMARK_COLUMN.to_string(),
                "TEXT".to_string(),
                Some("NULL".to_string())
            )
        );
        assert_eq!(
            tiers(&conn),
            vec![
                ("m1".into(), Some("hot".into()), Some("[]".into())),
                ("m2".into(), Some("hot".into()), Some("[]".into())),
            ]
        );
        let index: String = conn
            .query_row(
                "SELECT sql FROM sqlite_master WHERE name = ?1",
                [INDEX_NAME],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            index,
            "CREATE INDEX \"idx_memories_character_tier\" ON \"memories\" (\"characterId\", \"tier\")"
        );
    }

    #[test]
    fn a_planted_null_tier_is_healed() {
        let conn = pre_round();
        ensure_memory_tiers(&conn).unwrap();
        conn.execute("UPDATE memories SET tier = NULL WHERE id = 'm1'", [])
            .unwrap();
        conn.execute(
            "UPDATE memories SET consolidatedFrom = NULL WHERE id = 'm2'",
            [],
        )
        .unwrap();
        let outcome = ensure_memory_tiers(&conn).unwrap();
        assert_eq!(
            outcome,
            MemoryTiersEnsureOutcome {
                healed_rows: 2,
                ..Default::default()
            }
        );
        assert_eq!(
            tiers(&conn),
            vec![
                ("m1".into(), Some("hot".into()), Some("[]".into())),
                ("m2".into(), Some("hot".into()), Some("[]".into())),
            ]
        );
    }

    #[test]
    fn columns_present_index_absent_creates_only_the_index() {
        let conn = pre_round();
        ensure_memory_tiers(&conn).unwrap();
        conn.execute_batch(&format!("DROP INDEX \"{INDEX_NAME}\""))
            .unwrap();
        assert_eq!(
            ensure_memory_tiers(&conn).unwrap(),
            MemoryTiersEnsureOutcome {
                index_created: true,
                ..Default::default()
            }
        );
    }

    #[test]
    fn absent_tables_are_table_gated() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE \"unrelated\" (\"id\" TEXT PRIMARY KEY NOT NULL)")
            .unwrap();
        let before = master(&conn);
        assert!(ensure_memory_tiers(&conn).unwrap().is_noop());
        assert_eq!(master(&conn), before);
    }

    #[test]
    fn only_chats_lacking_the_watermark_gains_only_the_chat_column() {
        let conn = pre_round();
        ensure_memory_tiers(&conn).unwrap();
        // Rebuild `chats` without the column.
        conn.execute_batch("DROP TABLE chats").unwrap();
        conn.execute_batch(&baseline_chats_ddl()).unwrap();
        assert_eq!(
            ensure_memory_tiers(&conn).unwrap(),
            MemoryTiersEnsureOutcome {
                chat_column_added: true,
                ..Default::default()
            }
        );
    }

    #[test]
    fn a_second_run_and_a_fresh_generate_ddl_instance_are_exact_no_ops() {
        let conn = pre_round();
        ensure_memory_tiers(&conn).unwrap();
        let before = master(&conn);
        let outcome = ensure_memory_tiers(&conn).unwrap();
        assert!(outcome.is_noop(), "{outcome:?}");
        assert_eq!(master(&conn), before, "sqlite_master byte-equal");

        // generateDDL-current tables + the artifact index: shouldRun false.
        let fresh = Connection::open_in_memory().unwrap();
        fresh
            .execute_batch(&fresh_statement("CREATE TABLE \"memories\""))
            .unwrap();
        fresh
            .execute_batch(&fresh_statement("CREATE TABLE \"chats\""))
            .unwrap();
        fresh
            .execute_batch(
                &crate::services::provisioning::migration_main_index(INDEX_NAME)
                    .unwrap()
                    .unwrap(),
            )
            .unwrap();
        let before = master(&fresh);
        assert!(ensure_memory_tiers(&fresh).unwrap().is_noop());
        assert_eq!(master(&fresh), before);
    }

    /// A v4-migrated instance (the appended shape + the index + v4's own
    /// ledger row) is a no-op — and the ensure never reads or writes the
    /// ledger (R-A).
    #[test]
    fn a_v4_migrated_instance_is_a_no_op_and_the_ledger_is_untouched() {
        let conn = pre_round();
        ensure_memory_tiers(&conn).unwrap();
        conn.execute_batch(
            "CREATE TABLE \"migrations_state\" (\"id\" TEXT PRIMARY KEY, \"completedAt\" TEXT NOT NULL, \
             \"quilltapVersion\" TEXT NOT NULL, \"itemsAffected\" INTEGER NOT NULL DEFAULT 0, \"message\" TEXT);\
             INSERT INTO migrations_state VALUES ('add-memory-tiers-v1', 't', '4.10.0', 6, 'Memory tiers in place (6 changes)');",
        )
        .unwrap();
        assert!(ensure_memory_tiers(&conn).unwrap().is_noop());
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM migrations_state", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1);
    }

    /// R-J: v4's migration lines are NO-PORT — the ensure itself is SILENT on
    /// the run that ALTERs (the host logs v5's one line), and on a no-op run.
    #[test]
    fn the_ensure_emits_no_line_of_its_own() {
        let conn = pre_round();
        let (outcome, lines) = captured_with(|| ensure_memory_tiers(&conn).unwrap());
        assert!(!outcome.is_noop());
        assert!(lines.is_empty(), "{lines:?}");
        let (outcome, lines) = captured_with(|| ensure_memory_tiers(&conn).unwrap());
        assert!(outcome.is_noop());
        assert!(lines.is_empty(), "{lines:?}");
    }
}
