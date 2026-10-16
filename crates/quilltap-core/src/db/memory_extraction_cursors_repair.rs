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

/// What [`stamp_settled_chain`] recorded: the counts in the two rows it wrote.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SettledChainStamp {
    /// Chats v4's episode migration would backfill (`N chats backfilled`).
    pub episode_backfilled: usize,
    /// Cursor rows v4's cursor migration would insert (`N carried over`).
    pub cursors_carried: usize,
    /// `add-memory-tiers-v1` stamped too: its `shouldRun` holds on this
    /// instance through `needsChatWatermark` alone (the chat column the cursor
    /// migration dropped), so v4 would re-run it — and re-add the column — on
    /// a v5-provisioned instance (the `83334b9a5` unification's review).
    pub tiers_stamped: bool,
}

/// P4.D288 unit 13 (R-E's stamp rule, §R.13(a)) — the chain on an instance
/// where it is ALREADY SETTLED without v4's ledger: `chats` and the cursor
/// table present, NEITHER legacy watermark column, and NO ledger row for
/// either id. That is a v5-provisioned instance (it is born at the post-chain
/// shape), the one state O-3's gate leaves alone — and the one where v4's
/// runner, finding no rows, re-runs BOTH migrations (measured, the zero-rerun
/// `fresh` mode): it re-adds the episode column, backfills it from each chat's
/// last summary anchor, copies those into `episode-fold` cursors (skipping the
/// pending episodes of every chat that has none yet) and drops the column.
///
/// O-3's gate stands — nothing is run. This writes ONLY the two ledger rows,
/// with the bytes v4's chain would write on this instance, computed read-only
/// (pinned row-for-row against v4's own run on the same instance by
/// `zero_migrations_rerun_equivalence`'s `-unstamped` twins):
/// - episode — `itemsAffected` = 1 (the column) + B, `Episode watermark in
///   place (B chats backfilled)`, B = the chats whose stored anchors v4's
///   `lastSummaryAnchor` accepts (v4's candidate filter, every watermark NULL —
///   the column is new);
/// - cursors — `itemsAffected` = X, `Extraction cursors in place (X carried
///   over from 1 watermark column(s))`, X = those chats WITHOUT an
///   `<chatId>:episode-fold` cursor (v4's `ON CONFLICT DO NOTHING` counts only
///   inserts).
///
/// - **tiers** (`add-memory-tiers-v1`, when its row is absent — the
///   `83334b9a5` unification's review): v4's `shouldRun` holds on this instance
///   through `needsChatWatermark` ALONE (`add-memory-tiers-v1.ts:83,96-105` —
///   the memory columns and the index are provisioned, no NULL tiers), so v4
///   would re-run it, re-add `chats.otherExtractionWatermarkMessageId`, stamp
///   it, then SKIP the stamped cursor migration — and NEITHER app would ever
///   drop the column again (the ping-pong class). Stamped with the row v4
///   would write here: `itemsAffected` 1, `Memory tiers in place (1 changes)`
///   (`:147-156`). With tiers in the chain the cursor row reads `from 2
///   watermark column(s)` — v4's real fresh-boot chain (tiers adds `other`,
///   the episode migration adds `episode`, cursors moves BOTH); `from 1`
///   when tiers was already recorded (an instance tiers ran on before the
///   cursor table existed is the half-migrated arm, never this one).
///
/// `None` when the state is anything else (either id recorded; a legacy column
/// present — the cursor ensure's half-migrated arm; no `chats`; no cursor
/// table — the real chain's). ONE savepoint around the stamps.
pub fn stamp_settled_chain(main: &Connection) -> Result<Option<SettledChainStamp>, DbError> {
    use super::memory_episode_watermark_repair as episode;
    use super::memory_tiers_repair as tiers;
    if ledger_has(main, episode::MIGRATION_ID)?
        || ledger_has(main, MIGRATION_ID)?
        || !table_exists(main, "chats")?
        || !table_exists(main, TABLE)?
        || !legacy_columns_present(main)?.is_empty()
    {
        return Ok(None);
    }
    let anchors: Vec<(String, String)> = if column_exists(main, "chats", "summaryAnchorMessageIds")?
    {
        let mut stmt = main.prepare(
            "SELECT id, summaryAnchorMessageIds FROM chats
              WHERE summaryAnchorMessageIds IS NOT NULL
                AND summaryAnchorMessageIds NOT IN ('', '[]')",
        )?;
        let rows = stmt
            .query_map([], |r| {
                // A non-TEXT cell is not a string to v4's `lastSummaryAnchor`.
                let raw = match r.get_ref(1)? {
                    rusqlite::types::ValueRef::Text(t) => {
                        Some(String::from_utf8_lossy(t).into_owned())
                    }
                    _ => None,
                };
                Ok((r.get::<_, String>(0)?, raw))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        rows.into_iter()
            .filter_map(|(id, raw)| episode::last_summary_anchor(raw.as_deref()).map(|a| (id, a)))
            .collect()
    } else {
        Vec::new()
    };
    let episode_backfilled = anchors.len();
    let mut cursors_carried = 0usize;
    {
        let mut held = main.prepare(&format!(r#"SELECT 1 FROM "{TABLE}" WHERE "id" = ?1"#))?;
        for (chat_id, _) in &anchors {
            if !held.exists([format!("{chat_id}:episode-fold")])? {
                cursors_carried += 1;
            }
        }
    }
    let tiers_absent = !ledger_has(main, tiers::MIGRATION_ID)?;
    let watermark_columns = if tiers_absent { 2 } else { 1 };
    let now = crate::clock::now_iso();
    in_savepoint(main, "memory_extraction_cursors_settled", |conn| {
        if tiers_absent {
            // v4's registration order: tiers before the episode watermark.
            stamp_completed(
                conn,
                tiers::MIGRATION_ID,
                &now,
                1,
                "Memory tiers in place (1 changes)",
            )?;
        }
        stamp_completed(
            conn,
            episode::MIGRATION_ID,
            &now,
            1 + episode_backfilled as i64,
            &format!("Episode watermark in place ({episode_backfilled} chats backfilled)"),
        )?;
        stamp_completed(
            conn,
            MIGRATION_ID,
            &now,
            cursors_carried as i64,
            &format!(
                "Extraction cursors in place ({cursors_carried} carried over from {watermark_columns} watermark column(s))"
            ),
        )?;
        Ok(Some(SettledChainStamp {
            episode_backfilled,
            cursors_carried,
            tiers_stamped: tiers_absent,
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
