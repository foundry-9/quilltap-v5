//! The fold-time episode watermark boot ensure — P4.D288 (C1-2, R-D / O-3),
//! v4 `c2795733f`'s migration `add-episode-extraction-watermark-v1` (bug 207;
//! `dependsOn: ['add-memory-tiers-v1']`).
//!
//! v4 adds `chats.episodeExtractionWatermarkMessageId` (`TEXT DEFAULT NULL`)
//! and backfills each chat's watermark from the LAST entry of its
//! `summaryAnchorMessageIds` (every fold so far ran the episode pass over its
//! own window, and the anchors list every message the summary folded, in
//! order). Ported LITERALLY, in v4's registration order, because the cursor
//! ensure that runs after it ([`super::memory_extraction_cursors_repair`])
//! copies this column into `episode-fold` cursors and drops it: a pre-round v5
//! instance ends with exactly the cursors v4 would have given it.
//!
//! **O-3 — the exact gate.** [`ensure_memory_episode_watermark`] runs only when
//! v4's ledger lacks the id AND `chats` exists AND the column is ABSENT AND
//! `memory_extraction_cursors` is ABSENT — the one state in which v4's chain
//! meets this migration first. With the cursor table present it is a no-op on
//! every later boot (the cursor migration dropped the column; re-adding it here
//! would be the ping-pong); a half-migrated instance (table AND a legacy
//! column — a crashed v4 boot) is the cursor ensure's to converge.
//!
//! **Stamped (R-D / R-E).** When it runs it records v4's `migrations_state`
//! row (v4's message bytes — `Episode watermark in place (N chats
//! backfilled)`, `itemsAffected` = the column + the backfilled chats):
//! unstamped, v4 HEAD's `shouldRun` (column absent) would re-add the column
//! after the cursor drop and re-insert `episode-fold` cursors at the last
//! summary anchor, skipping pending episodes.
//!
//! **L-8 — ONE transaction** around the ADD, the backfill and the stamp (v4 has
//! none); a failure leaves the instance as it was and is FATAL to boot (the
//! caller `?`s it). v4's migration lines and its progress reports are NO-PORT;
//! the host logs v5's ONE boot INFO when it ran.

use rusqlite::{params, Connection};

use super::migrations_ledger::{ledger_has, stamp_completed};
use super::schema_probe::{add_column_if_missing, column_exists, in_savepoint, table_exists};
use super::DbError;

/// v4's migration id.
pub const MIGRATION_ID: &str = "add-episode-extraction-watermark-v1";

/// v4's `COLUMN` and its DDL text.
pub const COLUMN: &str = "episodeExtractionWatermarkMessageId";
const DECL: &str = "TEXT DEFAULT NULL";

/// v4's candidate SELECT, the bytes copied.
const CANDIDATES_SQL: &str = "SELECT id, summaryAnchorMessageIds FROM chats
              WHERE episodeExtractionWatermarkMessageId IS NULL
                AND summaryAnchorMessageIds IS NOT NULL
                AND summaryAnchorMessageIds NOT IN ('', '[]')";

/// What one run did; `None` from [`ensure_memory_episode_watermark`] is the
/// gate saying no.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EpisodeWatermarkOutcome {
    /// The column was added (v4's `itemsAffected++`).
    pub column_added: bool,
    /// Candidate chats (v4's `candidates`).
    pub candidates: usize,
    /// Chats whose watermark was set (v4's `backfilled`).
    pub backfilled: usize,
}

/// v4 `lastSummaryAnchor(raw)`: the last id of a stored
/// `summaryAnchorMessageIds` value, or `None` (not a non-empty string; not a
/// JSON array; empty; the last element not a non-empty string). `JSON.parse`
/// is serde's here — the anchors are v4's own `JSON.stringify` output.
pub fn last_summary_anchor(raw: Option<&str>) -> Option<String> {
    let raw = raw.filter(|s| !s.is_empty())?;
    let parsed: serde_json::Value = serde_json::from_str(raw).ok()?;
    let last = parsed.as_array()?.last()?;
    last.as_str().filter(|s| !s.is_empty()).map(str::to_string)
}

/// Run v4's `add-episode-extraction-watermark-v1` under O-3's gate; `None`
/// when the gate says no.
pub fn ensure_memory_episode_watermark(
    main: &Connection,
) -> Result<Option<EpisodeWatermarkOutcome>, DbError> {
    if ledger_has(main, MIGRATION_ID)?
        || !table_exists(main, "chats")?
        || column_exists(main, "chats", COLUMN)?
        || table_exists(main, super::memory_extraction_cursors::TABLE)?
    {
        return Ok(None);
    }
    let now = crate::clock::now_iso();
    in_savepoint(main, "memory_episode_watermark", |conn| {
        let column_added = add_column_if_missing(conn, "chats", COLUMN, DECL)?;
        let (mut candidates, mut backfilled) = (0usize, 0usize);
        if column_exists(conn, "chats", "summaryAnchorMessageIds")? {
            let rows: Vec<(String, Option<String>)> = {
                let mut stmt = conn.prepare(CANDIDATES_SQL)?;
                let rows = stmt
                    .query_map([], |r| {
                        // A non-TEXT cell is not a string to v4's
                        // `lastSummaryAnchor` — it reads as no anchor.
                        let raw = match r.get_ref(1)? {
                            rusqlite::types::ValueRef::Text(t) => {
                                Some(String::from_utf8_lossy(t).into_owned())
                            }
                            _ => None,
                        };
                        Ok((r.get::<_, String>(0)?, raw))
                    })?
                    .collect::<Result<Vec<_>, _>>()?;
                rows
            };
            candidates = rows.len();
            for (id, raw) in rows {
                if let Some(watermark) = last_summary_anchor(raw.as_deref()) {
                    conn.execute(
                        "UPDATE chats SET episodeExtractionWatermarkMessageId = ? WHERE id = ?",
                        params![watermark, id],
                    )?;
                    backfilled += 1;
                }
            }
        }
        let items = i64::from(column_added) + backfilled as i64;
        stamp_completed(
            conn,
            MIGRATION_ID,
            &now,
            items,
            &format!("Episode watermark in place ({backfilled} chats backfilled)"),
        )?;
        Ok(Some(EpisodeWatermarkOutcome {
            column_added,
            candidates,
            backfilled,
        }))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn last_summary_anchor_reads_v4s_shapes() {
        assert_eq!(last_summary_anchor(Some(r#"["a","b"]"#)), Some("b".into()));
        assert_eq!(last_summary_anchor(Some("[]")), None);
        assert_eq!(last_summary_anchor(Some("")), None);
        assert_eq!(last_summary_anchor(None), None);
        assert_eq!(last_summary_anchor(Some("not json")), None);
        assert_eq!(last_summary_anchor(Some(r#"["a",""]"#)), None);
        assert_eq!(last_summary_anchor(Some(r#"["a",7]"#)), None);
        assert_eq!(last_summary_anchor(Some(r#"{"0":"a"}"#)), None);
    }
}
