//! The vector-mirror retire — P4.D288 (C1-2, O-1, the human's 2026-10-13
//! RETIRE ruling), v4 `af48f6cc2`'s migration `retire-memory-vector-entries-v1`
//! (memory refactor Phase B, decision 1).
//!
//! A memory's vector used to live twice: on its row (`memories.embedding`) and
//! in `vector_entries`, with per-character metadata in `vector_indices`. The
//! mirror had no foreign key, so a delete that forgot it left an orphan vector
//! that blinded the memory gate (bug 200), and a tier move it never saw left
//! cold rows in hot-only recall (bug 205). From Phase B the row is the ONLY
//! copy. Before dropping the mirror v4 walks every `vector_entries` row by
//! rowid, in pages of 500, and — where the memory exists and has no embedding
//! of its own — copies the mirror's vector onto the row (a BLOB byte for byte;
//! a JSON-text vector, a development hot-reload artefact, through
//! [`parse_legacy_embedding_text`] → [`float32_to_blob`], the QUANTIZED form);
//! then `DROP TABLE IF EXISTS` `vector_entries`, then `vector_indices`.
//!
//! [`retire_memory_vector_mirror`] follows v4's `shouldRun` (either table
//! exists) and `run`, counting as v4 counts: no memory row → `orphaned`; the
//! row's `embedding IS NOT NULL` (an EMPTY blob included) → `already_on_row`;
//! no usable mirror vector → `unusable`; else `copied += changes` of `UPDATE …
//! WHERE id = ? AND embedding IS NULL`. **L-8:** v4's per-PAGE transactions
//! (one savepoint per 500 rows), the DROPs after — a failure mid-walk keeps
//! the pages already copied and leaves the mirror for the next boot.
//! Structure-gated, UNSTAMPED (a second run finds neither table), FATAL on
//! failure. v4's lines and progress reports are NO-PORT; the host logs v5's ONE
//! boot INFO when it ran.

use rusqlite::types::ValueRef;
use rusqlite::{params, Connection, OptionalExtension};

use super::schema_probe::{in_savepoint, table_exists};
use super::DbError;
use crate::embedding_blob::{float32_to_blob, parse_legacy_embedding_text};

/// v4's migration id.
pub const MIGRATION_ID: &str = "retire-memory-vector-entries-v1";

/// v4's `PAGE_SIZE`.
pub const PAGE_SIZE: i64 = 500;

/// What one run did; `None` from [`retire_memory_vector_mirror`] is v4's
/// `shouldRun` saying no.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct VectorMirrorRetireOutcome {
    /// `vector_entries` rows at the start (v4's `mirrorRows`; 0 when the walk
    /// did not run).
    pub mirror_rows: usize,
    pub copied: usize,
    pub already_on_row: usize,
    pub orphaned: usize,
    pub unusable: usize,
    /// Tables dropped (v4's `tablesDropped`).
    pub tables_dropped: usize,
}

/// v4 `mirrorBlob(value)`: the mirror's vector as a stored embedding BLOB, or
/// `None` when it has none usable.
pub fn mirror_blob(cell: ValueRef<'_>) -> Option<Vec<u8>> {
    match cell {
        ValueRef::Blob(b) => (!b.is_empty()).then(|| b.to_vec()),
        ValueRef::Text(t) => {
            let parsed = parse_legacy_embedding_text(&String::from_utf8_lossy(t))?;
            if parsed.is_empty() {
                return None;
            }
            let v: Vec<f32> = parsed.iter().map(|x| *x as f32).collect();
            Some(float32_to_blob(&v))
        }
        _ => None,
    }
}

/// Run v4's `retire-memory-vector-entries-v1` when its `shouldRun` would.
pub fn retire_memory_vector_mirror(
    main: &Connection,
) -> Result<Option<VectorMirrorRetireOutcome>, DbError> {
    if !table_exists(main, "vector_entries")? && !table_exists(main, "vector_indices")? {
        return Ok(None);
    }
    let mut out = VectorMirrorRetireOutcome::default();
    if table_exists(main, "vector_entries")? && table_exists(main, "memories")? {
        let total: i64 =
            main.query_row("SELECT COUNT(*) AS n FROM vector_entries", [], |r| r.get(0))?;
        out.mirror_rows = total as usize;
        let mut cursor: i64 = 0;
        loop {
            let page: Vec<(i64, String, Option<Vec<u8>>)> = {
                let mut stmt = main.prepare(
                    "SELECT rowid AS rid, id, embedding FROM vector_entries WHERE rowid > ? ORDER BY rowid LIMIT ?",
                )?;
                let rows = stmt
                    .query_map(params![cursor, PAGE_SIZE], |r| {
                        Ok((
                            r.get::<_, i64>(0)?,
                            r.get::<_, String>(1)?,
                            mirror_blob(r.get_ref(2)?),
                        ))
                    })?
                    .collect::<Result<Vec<_>, _>>()?;
                rows
            };
            let Some(last) = page.last().map(|(rid, _, _)| *rid) else {
                break;
            };
            in_savepoint(main, "memory_vector_mirror_retire", |conn| {
                for (_, id, blob) in &page {
                    let has: Option<i64> = conn
                        .query_row(
                            "SELECT embedding IS NOT NULL AS has FROM memories WHERE id = ?",
                            params![id],
                            |r| r.get(0),
                        )
                        .optional()?;
                    match has {
                        None => out.orphaned += 1,
                        Some(h) if h != 0 => out.already_on_row += 1,
                        Some(_) => match blob {
                            None => out.unusable += 1,
                            Some(b) => {
                                out.copied += conn.execute(
                                    "UPDATE memories SET embedding = ? WHERE id = ? AND embedding IS NULL",
                                    params![b, id],
                                )?;
                            }
                        },
                    }
                }
                Ok(())
            })?;
            cursor = last;
        }
    }
    for table in ["vector_entries", "vector_indices"] {
        if table_exists(main, table)? {
            main.execute_batch(&format!("DROP TABLE IF EXISTS \"{table}\""))?;
            out.tables_dropped += 1;
        }
    }
    Ok(Some(out))
}
