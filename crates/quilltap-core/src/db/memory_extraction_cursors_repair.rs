//! The extraction-cursor boot ensure — P4.D288 (C1-2, R-C / R-D / L-8), v4
//! `ac6348c5f`'s migration `add-memory-extraction-cursors-v1` (memory refactor
//! Phase D; `dependsOn: ['add-memory-tiers-v1',
//! 'add-episode-extraction-watermark-v1']`).
//!
//! v4 creates `memory_extraction_cursors` — one row per (chat, fold-grain
//! pass), the last message that pass has read — in the MIGRATION's text
//! ([`MIGRATION_TABLE_DDL`], R-C: `UNIQUE ("chatId", "pass")` + a cascading
//! FK), then MOVES the two per-chat watermark columns into it, in v4's
//! `LEGACY_WATERMARK_COLUMNS` order —
//!
//!   `chats.otherExtractionWatermarkMessageId`   → pass `other-fold`
//!   `chats.episodeExtractionWatermarkMessageId` → pass `episode-fold`
//!
//! — copying each non-NULL, non-empty cell as id `<chatId>:<pass>` with ONE
//! `now` (`ON CONFLICT("id") DO NOTHING`), and then drops every present legacy
//! column (`ALTER TABLE "chats" DROP COLUMN`). It runs after
//! [`super::memory_episode_watermark_repair`] (R-D, v4's registration order),
//! so a pre-round instance ends with the cursors v4 would have given it.
//!
//! [`ensure_memory_extraction_cursors`] follows v4's runner (the ledger first)
//! and `shouldRun`: `chats` exists, and the table is absent OR a legacy column
//! is present — so it also converges a HALF-migrated instance (table present,
//! a legacy column left by a crashed v4 boot).
//!
//! **Stamped (R-D / R-E)** with v4's message bytes (`Extraction cursors in
//! place (N carried over from M watermark column(s))`, `itemsAffected` = the
//! rows copied). **L-8 — ONE transaction** around the create, the copies, the
//! drops and the stamp (v4 has one per column copy and none around the DROPs —
//! the P4.D251 recorded divergence). FATAL on failure. v4's lines and progress
//! reports are NO-PORT; the host logs v5's ONE boot INFO when it ran.
//!
//! The repository's first-touch index (`idx_memory_extraction_cursors_
//! createdAt`, generateDDL's — v4's `ensureCollection` adds it lazily on the
//! repository's first use) is [`ensure_cursor_created_at_index`], run on every
//! boot whenever the table exists (the `wardrobe_wear_stats_repair` shape).

use rusqlite::{params, Connection};

use super::memory_extraction_cursors::{CREATED_AT_INDEX_DDL, MIGRATION_TABLE_DDL, TABLE};
use super::migrations_ledger::{ledger_has, stamp_completed};
use super::schema_probe::{column_exists, in_savepoint, table_exists};
use super::DbError;

/// v4's migration id.
pub const MIGRATION_ID: &str = "add-memory-extraction-cursors-v1";

/// v4's `LEGACY_WATERMARK_COLUMNS`: legacy column → the pass whose cursor it
/// becomes, in v4's order.
pub const LEGACY_WATERMARK_COLUMNS: [(&str, &str); 2] = [
    ("otherExtractionWatermarkMessageId", "other-fold"),
    ("episodeExtractionWatermarkMessageId", "episode-fold"),
];

/// What one run did; `None` from [`ensure_memory_extraction_cursors`] is the
/// runner saying no.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractionCursorsOutcome {
    /// The table was created by this run.
    pub table_created: bool,
    /// Rows copied per present legacy column, in v4's order (`(column, pass,
    /// rows selected)` — v4's `rows.length` on its INFO).
    pub copies: Vec<(&'static str, &'static str, usize)>,
    /// Cursor rows actually inserted (v4's `backfilled` — `changes` summed).
    pub backfilled: usize,
    /// The legacy columns dropped, in v4's order.
    pub dropped_columns: Vec<&'static str>,
}

fn legacy_columns_present(main: &Connection) -> Result<Vec<(&'static str, &'static str)>, DbError> {
    let mut present = Vec::new();
    for (column, pass) in LEGACY_WATERMARK_COLUMNS {
        if column_exists(main, "chats", column)? {
            present.push((column, pass));
        }
    }
    Ok(present)
}

/// The migration's statement — `MIGRATION_TABLE_DDL` with v4's `IF NOT
/// EXISTS` (SQLite stores the text without it, which is the constant).
fn create_table_sql() -> String {
    MIGRATION_TABLE_DDL.replacen("CREATE TABLE ", "CREATE TABLE IF NOT EXISTS ", 1)
}

/// Run v4's `add-memory-extraction-cursors-v1` when its runner would.
pub fn ensure_memory_extraction_cursors(
    main: &Connection,
) -> Result<Option<ExtractionCursorsOutcome>, DbError> {
    if ledger_has(main, MIGRATION_ID)? || !table_exists(main, "chats")? {
        return Ok(None);
    }
    let table_absent = !table_exists(main, TABLE)?;
    if !table_absent && legacy_columns_present(main)?.is_empty() {
        return Ok(None);
    }
    let now = crate::clock::now_iso();
    in_savepoint(main, "memory_extraction_cursors", |conn| {
        conn.execute_batch(&create_table_sql())?;
        let present = legacy_columns_present(conn)?;
        let mut copies = Vec::new();
        let mut backfilled = 0usize;
        for (column, pass) in &present {
            let rows: Vec<(String, String)> = {
                let mut stmt = conn.prepare(&format!(
                    r#"SELECT "id", "{column}" AS "messageId" FROM "chats" WHERE "{column}" IS NOT NULL AND "{column}" <> ''"#
                ))?;
                let rows = stmt
                    .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
                    .collect::<Result<Vec<_>, _>>()?;
                rows
            };
            for (chat_id, message_id) in &rows {
                backfilled += conn.execute(
                    r#"INSERT INTO "memory_extraction_cursors" ("id", "chatId", "pass", "messageId", "createdAt", "updatedAt")
         VALUES (?, ?, ?, ?, ?, ?)
         ON CONFLICT("id") DO NOTHING"#,
                    params![
                        format!("{chat_id}:{pass}"),
                        chat_id,
                        pass,
                        message_id,
                        now,
                        now
                    ],
                )?;
            }
            copies.push((*column, *pass, rows.len()));
        }
        let mut dropped_columns = Vec::new();
        for (column, _) in &present {
            conn.execute_batch(&format!(r#"ALTER TABLE "chats" DROP COLUMN "{column}""#))?;
            dropped_columns.push(*column);
        }
        stamp_completed(
            conn,
            MIGRATION_ID,
            &now,
            backfilled as i64,
            &format!(
                "Extraction cursors in place ({backfilled} carried over from {} watermark column(s))",
                present.len()
            ),
        )?;
        Ok(Some(ExtractionCursorsOutcome {
            table_created: table_absent,
            copies,
            backfilled,
            dropped_columns,
        }))
    })
}

/// v4's repository first touch: generateDDL's `createdAt` index on the cursor
/// table, `IF NOT EXISTS`, whenever the table exists. `true` when it was
/// created by this call.
pub fn ensure_cursor_created_at_index(main: &Connection) -> Result<bool, DbError> {
    if !table_exists(main, TABLE)? {
        return Ok(false);
    }
    let existed =
        super::schema_probe::index_exists(main, "idx_memory_extraction_cursors_createdAt")?;
    main.execute_batch(&CREATED_AT_INDEX_DDL.replacen("INDEX ", "INDEX IF NOT EXISTS ", 1))?;
    Ok(!existed)
}
