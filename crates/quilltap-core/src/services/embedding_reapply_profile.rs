//! The `EMBEDDING_REAPPLY_PROFILE` job — a port of v4's
//! `lib/embedding/reapply-profile.ts` (+ the thin `embedding-reapply-profile.ts`
//! handler). Walks every embedding-bearing table and rewrites stored Float32
//! BLOBs to match the active profile's Matryoshka `truncateToDimensions` + L2
//! normalization — a pure-local slice+renormalise, no provider call.
//!
//! Tables walked (v4's allowlist): `memories`, `vector_entries`,
//! `conversation_chunks`, `help_docs` (main DB) + `doc_mount_chunks` (mount
//! index). The whole pass runs inside ONE `db.write` closure on the writer's
//! connections (both partitions), sequentially, so it needs no path storage on
//! [`Db`] — each connection self-reports its file via `pragma_database_list` for
//! the `VACUUM INTO` backup.
//!
//! Invariants preserved from v4:
//!   - Dimensions come from the DECODED vector ([`blob_to_float32`]`.len()`),
//!     never the byte length (legacy raw Float32 and quantized blobs coexist).
//!   - A `VACUUM INTO` backup of each affected DB is taken BEFORE any write;
//!     backup failure is FATAL (both partitions). Empty corpus (observedMax==0)
//!     → clean zero result, no backup.
//!   - Refuses to GROW: if the widest stored vector is shorter than the target,
//!     abort with v4's exact two-sentence error (vectors can't grow without
//!     re-embedding — the reindex flow's job).
//!   - Degenerate vectors (post-slice magnitude < `ZERO_MAG`) are counted and
//!     skipped, never written.
//!   - Reads/computes happen before the UPDATEs; each table's writes run in a
//!     transaction. Post-write `VACUUM` is non-fatal.
//!   - v4's `invalidateMountChunkCacheAll` / `unloadAll` are documented no-ops in
//!     v5 (no in-memory vector stores / mount-chunk cache), per the P4.d27 bank.
//!
//! ⚠ CRYPTO PATH: `VACUUM INTO` produces an encrypted backup (the destination
//! inherits the source connection's SQLite3MC key context). The differential
//! proves the in-place slice is byte-correct and that the backup call does not
//! throw; it does NOT open the backup. That the backup is correctly encrypted /
//! reopenable awaits the real-instance proof (a Friday-copy reapply).

use rusqlite::{params, Connection};

use crate::db::runtime::Db;
use crate::db::DbError;
use crate::embedding_blob::{blob_to_float32, float32_to_blob, EMBEDDING_DIM_SQL};

/// v4 `FLUSH_BATCH` — kept for parity; a single per-table transaction here makes
/// the flush count moot for the final state.
const FLUSH_BATCH: usize = 500;
/// v4 `ZERO_MAG` — post-slice magnitudes below this are degenerate.
const ZERO_MAG: f64 = 1e-10;

/// v4 `MAIN_DB_TABLES`.
const MAIN_DB_TABLES: [&str; 4] = [
    "memories",
    "vector_entries",
    "conversation_chunks",
    "help_docs",
];

/// Per-table outcome (v4 `TableReapplyResult`).
#[derive(Debug, Clone, Default)]
pub struct TableReapplyResult {
    pub table: String,
    pub truncated: usize,
    pub already_at_target: usize,
    pub shorter_than_target: usize,
    pub degenerate: usize,
}

/// Aggregate report (v4 `ReapplyEmbeddingProfileResult`).
#[derive(Debug, Clone, Default)]
pub struct ReapplyResult {
    pub profile_id: String,
    pub target_dimensions: i64,
    pub per_table: Vec<TableReapplyResult>,
    pub total_truncated: usize,
    pub backup_path: Option<String>,
    pub mount_backup_path: Option<String>,
}

fn table_exists(conn: &Connection, name: &str) -> bool {
    conn.query_row(
        "SELECT name FROM sqlite_master WHERE type='table' AND name = ?",
        params![name],
        |r| r.get::<_, String>(0),
    )
    .is_ok()
}

/// v4 `sliceAndNormalize` — slice to `target_dim`, L2-normalise. Returns the
/// pre-normalisation magnitude (for the degenerate check). Reproduces v4's JS
/// float math: `v = src[i]` (an f32-value promoted to f64) accumulates `sumSq` in
/// f64; `dst[i] = v` stores the f32 slice value; `dst[i] *= inv` computes
/// `(dst[i] as f64) * inv` and stores back to f32.
fn slice_and_normalize(src: &[f32], target_dim: usize, normalize: bool) -> (Vec<f32>, f64) {
    let mut dst: Vec<f32> = Vec::with_capacity(target_dim);
    let mut sum_sq: f64 = 0.0;
    for &e in src.iter().take(target_dim) {
        let v = e as f64;
        dst.push(e);
        sum_sq += v * v;
    }
    let magnitude = sum_sq.sqrt();
    if normalize && magnitude >= ZERO_MAG {
        let inv = 1.0 / magnitude;
        for d in dst.iter_mut() {
            *d = ((*d as f64) * inv) as f32;
        }
    }
    (dst, magnitude)
}

/// v4 `maxStoredDimension` for one table — `MAX(EMBEDDING_DIM_SQL)` over
/// non-null embeddings (0 when the table is absent / empty).
fn max_dim_in_table(conn: &Connection, table: &str) -> i64 {
    if !table_exists(conn, table) {
        return 0;
    }
    conn.query_row(
        &format!("SELECT MAX({EMBEDDING_DIM_SQL}) FROM \"{table}\" WHERE embedding IS NOT NULL"),
        [],
        |r| r.get::<_, Option<i64>>(0),
    )
    .ok()
    .flatten()
    .unwrap_or(0)
}

/// v4 `reapplyTable` — read every non-null embedding, slice+normalise those
/// longer than `target_dim`, and UPDATE them in one transaction.
fn reapply_table(
    conn: &Connection,
    table: &str,
    target_dim: usize,
    normalize: bool,
) -> Result<TableReapplyResult, DbError> {
    let mut result = TableReapplyResult {
        table: table.to_string(),
        ..Default::default()
    };

    // Read all rows first (v4 reads outside the txn).
    let rows: Vec<(String, Vec<u8>)> = {
        let mut stmt = conn.prepare(&format!(
            "SELECT id, embedding FROM \"{table}\" WHERE embedding IS NOT NULL"
        ))?;
        let mapped = stmt.query_map([], |r| {
            let id: String = r.get(0)?;
            // v4 skips `typeof row.embedding === 'string'` (legacy text) — a BLOB
            // column reads as bytes here; a TEXT cell would error on Vec<u8> get,
            // so tolerate it by skipping (matches v4's continue).
            let blob: Option<Vec<u8>> = r.get::<_, Option<Vec<u8>>>(1).ok().flatten();
            Ok((id, blob))
        })?;
        let mut out = Vec::new();
        for row in mapped {
            let (id, blob) = row?;
            if let Some(b) = blob {
                out.push((id, b));
            }
        }
        out
    };

    let mut batch: Vec<(String, Vec<u8>)> = Vec::new();
    let flush = |conn: &Connection, batch: &mut Vec<(String, Vec<u8>)>| -> Result<usize, DbError> {
        let n = batch.len();
        if n == 0 {
            return Ok(0);
        }
        let tx = conn.unchecked_transaction()?;
        {
            let mut upd = tx.prepare(&format!(
                "UPDATE \"{table}\" SET embedding = ? WHERE id = ?"
            ))?;
            for (id, blob) in batch.iter() {
                upd.execute(params![blob, id])?;
            }
        }
        tx.commit()?;
        batch.clear();
        Ok(n)
    };

    for (id, blob) in &rows {
        let src = blob_to_float32(blob);
        if src.len() == target_dim {
            result.already_at_target += 1;
            continue;
        }
        if src.len() < target_dim {
            result.shorter_than_target += 1;
            continue;
        }
        let (dst, magnitude) = slice_and_normalize(&src, target_dim, normalize);
        if magnitude < ZERO_MAG {
            result.degenerate += 1;
            continue;
        }
        batch.push((id.clone(), float32_to_blob(&dst)));
        if batch.len() >= FLUSH_BATCH {
            result.truncated += flush(conn, &mut batch)?;
        }
    }
    result.truncated += flush(conn, &mut batch)?;

    Ok(result)
}

/// The source file path of a connection's `main` schema (v4 gets it from
/// `getSQLiteDatabasePath` / `getMountIndexDatabasePath`; here each connection
/// self-reports via `pragma_database_list`).
fn db_file_path(conn: &Connection) -> Option<String> {
    conn.query_row(
        "SELECT file FROM pragma_database_list WHERE name = 'main'",
        [],
        |r| r.get::<_, String>(0),
    )
    .ok()
    .filter(|s| !s.is_empty())
}

/// v4 `vacuumIntoBackup` — hot-copy the DB via `VACUUM INTO`, name
/// `<base>.bak-pre-truncation-<YYYY-MM-DD>.db` (+ `-<millis>` on a same-day
/// collision). `date_stamp` is `now.split('T')[0]`; `millis` disambiguates.
fn vacuum_into_backup(
    conn: &Connection,
    src_path: &str,
    date_stamp: &str,
    millis: u128,
) -> Result<String, DbError> {
    let src = std::path::Path::new(src_path);
    let dir = src.parent().unwrap_or_else(|| std::path::Path::new("."));
    let base = src
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("database");
    let primary = dir.join(format!("{base}.bak-pre-truncation-{date_stamp}.db"));
    let dst = if primary.exists() {
        dir.join(format!(
            "{base}.bak-pre-truncation-{date_stamp}-{millis}.db"
        ))
    } else {
        primary
    };
    let dst_str = dst.to_string_lossy().replace('\'', "''");
    conn.execute_batch(&format!("VACUUM INTO '{dst_str}'"))?;
    Ok(dst.to_string_lossy().to_string())
}

/// Run the whole re-apply pass. `target_dim` = the profile's
/// `truncateToDimensions` (already validated > 0 by the caller); `normalize` =
/// `normalizeL2 !== false`. `now` is the ISO stamp (for the backup date) and
/// `millis` disambiguates a same-day backup collision.
pub async fn reapply_embedding_profile(
    db: &Db,
    profile_id: &str,
    target_dim: i64,
    normalize: bool,
    now: &str,
    millis: u128,
) -> Result<ReapplyResult, String> {
    let profile_id = profile_id.to_string();
    // v4's in-service defensive throw (`reapply-profile.ts:288-293`) — every
    // current caller pre-validates, but a future direct caller must hit v4's
    // sentence rather than back up and count every row degenerate (§3 review).
    if target_dim <= 0 {
        return Err(format!(
            "Embedding profile {profile_id} has no truncateToDimensions set — nothing to re-apply."
        ));
    }
    let date_stamp = now.split('T').next().unwrap_or(now).to_string();
    let target = target_dim as usize;

    // The closure returns `Result<Result<ReapplyResult, String>, DbError>`: the
    // OUTER `Err` is a genuine DB error (propagated via `?`); the INNER `Err` is a
    // domain abort (refuse-to-grow, backup-failure — v4's thrown Errors).
    #[allow(clippy::type_complexity)]
    let out: Result<Result<ReapplyResult, String>, DbError> = db
        .write(
            move |ws: &mut crate::db::runtime::WriterSet| -> Result<Result<ReapplyResult, String>, DbError> {
                let main = ws.main().connection();

                // observedMax across main tables + mount doc_mount_chunks.
                let mut observed_max: i64 = 0;
                for table in MAIN_DB_TABLES {
                    observed_max = observed_max.max(max_dim_in_table(main, table));
                }
                if let Some(mw) = ws.mount_index() {
                    observed_max =
                        observed_max.max(max_dim_in_table(mw.connection(), "doc_mount_chunks"));
                }

                let mut result = ReapplyResult {
                    profile_id: profile_id.clone(),
                    target_dimensions: target_dim,
                    ..Default::default()
                };

                // Empty corpus → clean zero result, no backup (v4 :298-312).
                if observed_max == 0 {
                    return Ok(Ok(result));
                }
                // Refuse to grow (v4 :314-319, the exact two sentences).
                if observed_max < target_dim {
                    return Ok(Err(format!(
                        "Stored corpus has {observed_max}-d vectors but profile requests {target_dim}-d. \
Vectors cannot grow without re-embedding — use the reindex flow instead."
                    )));
                }

                // Backup main BEFORE any write (FATAL on failure).
                let Some(main_path) = db_file_path(main) else {
                    return Ok(Err("Main SQLite database path unavailable".to_string()));
                };
                match vacuum_into_backup(main, &main_path, &date_stamp, millis) {
                    Ok(p) => result.backup_path = Some(p),
                    // v4 LOGS "…backup failed — aborting" and rethrows the RAW
                    // error, so the persisted job error is the bare SQLite
                    // message (§3 review — the context stays in tracing).
                    Err(e) => {
                        tracing::error!(error = %e, "Main DB backup failed — aborting");
                        return Ok(Err(e.to_string()));
                    }
                }

                // Walk main tables.
                for table in MAIN_DB_TABLES {
                    if !table_exists(main, table) {
                        continue;
                    }
                    result
                        .per_table
                        .push(reapply_table(main, table, target, normalize)?);
                }
                // VACUUM main (non-fatal).
                if let Err(e) = main.execute_batch("VACUUM") {
                    tracing::warn!(error = %e, "VACUUM on main DB failed (non-fatal)");
                }

                // Mount doc_mount_chunks.
                if let Some(mw) = ws.mount_index() {
                    let mount = mw.connection();
                    if table_exists(mount, "doc_mount_chunks") {
                        let Some(mount_path) = db_file_path(mount) else {
                            return Ok(Err("Mount index DB path unavailable".to_string()));
                        };
                        match vacuum_into_backup(mount, &mount_path, &date_stamp, millis) {
                            Ok(p) => result.mount_backup_path = Some(p),
                            // Raw error persisted, context in tracing (see the
                            // main-partition arm — §3 review).
                            Err(e) => {
                                tracing::error!(error = %e, "Mount index DB backup failed — aborting");
                                return Ok(Err(e.to_string()));
                            }
                        }
                        result
                            .per_table
                            .push(reapply_table(mount, "doc_mount_chunks", target, normalize)?);
                        if let Err(e) = mount.execute_batch("VACUUM") {
                            tracing::warn!(error = %e, "VACUUM on mount DB failed (non-fatal)");
                        }
                    }
                }

                result.total_truncated = result.per_table.iter().map(|r| r.truncated).sum();
                Ok(Ok(result))
            },
        )
        .await;
    out.map_err(|e: DbError| e.to_string())?
}

// ===========================================================================
// The thin job handler (v4 `handleEmbeddingReapplyProfile`)
// ===========================================================================

/// v4 `handleEmbeddingReapplyProfile`: resolve the profile, throw
/// `Embedding profile not found: {id}` on miss, return silently when the profile
/// has no `truncateToDimensions`, else run the re-apply.
pub async fn handle_embedding_reapply_profile(
    db: &Db,
    payload: &serde_json::Value,
    now: &str,
    millis: u128,
) -> Result<(), String> {
    let profile_id = payload
        .get("profileId")
        .and_then(|v| v.as_str())
        .ok_or("payload missing profileId")?
        .to_string();

    let profile = db
        .read_main(|conn| crate::db::embedding_profiles::find_by_id(conn, &profile_id))
        .map_err(|e| e.to_string())?;
    let Some(profile) = profile else {
        return Err(format!("Embedding profile not found: {profile_id}"));
    };

    // `!profile.truncateToDimensions` — a null OR a falsy 0 both return silently.
    let target = match profile.truncate_to_dimensions {
        Some(t) if t != 0.0 => t as i64,
        _ => return Ok(()),
    };
    let normalize = profile.normalize_l2;

    reapply_embedding_profile(db, &profile_id, target, normalize, now, millis).await?;
    Ok(())
}

/// The `EMBEDDING_REAPPLY_PROFILE` [`JobHandler`](crate::services::job_runner::JobHandler)
/// — seam-free (DB only). `now_iso`/`millis` pin the backup name in tests.
pub struct EmbeddingReapplyProfileHandler {
    /// `None` reads the real clock at handle time; `Some(s)` pins it.
    pub now_iso: Option<String>,
    /// `None` reads the real wall-clock millis; `Some(m)` pins it (tests).
    pub millis: Option<u128>,
}

impl crate::services::job_runner::JobHandler for EmbeddingReapplyProfileHandler {
    fn handle<'a>(
        &'a self,
        db: &'a Db,
        job: &'a crate::db::background_jobs::BackgroundJob,
    ) -> crate::services::job_runner::JobFuture<'a> {
        Box::pin(async move {
            let payload: serde_json::Value =
                serde_json::from_str(&job.payload).unwrap_or(serde_json::Value::Null);
            let now = self.now_iso.clone().unwrap_or_else(crate::clock::now_iso);
            let millis = self.millis.unwrap_or_else(|| {
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis())
                    .unwrap_or(0)
            });
            match handle_embedding_reapply_profile(db, &payload, &now, millis).await {
                Ok(()) => crate::services::job_runner::JobOutcome::Completed(None),
                Err(e) => crate::services::job_runner::JobOutcome::Failed(e),
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::embedding_blob::{float32_to_blob, float32_to_blob_raw};

    /// A single embedding-bearing table in an in-memory DB — proves the core
    /// slice/normalise/dim-detect logic directly (the differential's mount quirk
    /// aside; `reapply_table` is the SAME code the main-DB tables use).
    fn scratch() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE memories (id TEXT PRIMARY KEY, embedding BLOB);")
            .unwrap();
        conn
    }

    fn insert(conn: &Connection, id: &str, blob: &[u8]) {
        conn.execute(
            "INSERT INTO memories (id, embedding) VALUES (?1, ?2)",
            params![id, blob],
        )
        .unwrap();
    }

    #[test]
    fn slices_longer_normalizes_and_counts() {
        let conn = scratch();
        // 8-dim quantized → slice to 4-dim.
        insert(
            &conn,
            "long-q",
            &float32_to_blob(&[3.0, 4.0, 0.0, 0.0, 9.0, 9.0, 9.0, 9.0]),
        );
        // 8-dim legacy raw → also sliced (dim from the decoded vector, not bytes).
        insert(
            &conn,
            "long-raw",
            &float32_to_blob_raw(&[1.0, 0.0, 0.0, 0.0, 5.0, 5.0, 5.0, 5.0]),
        );
        // Already 4-dim → left alone.
        insert(&conn, "at-target", &float32_to_blob(&[0.5, 0.5, 0.5, 0.5]));
        // Shorter than 4 → cannot grow, left alone.
        insert(&conn, "short", &float32_to_blob(&[1.0, 0.0]));
        // 8-dim but the first 4 are all zero → degenerate, skipped.
        insert(
            &conn,
            "degenerate",
            &float32_to_blob(&[0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0]),
        );

        let r = reapply_table(&conn, "memories", 4, true).unwrap();
        assert_eq!(r.truncated, 2, "two 8-dim rows sliced");
        assert_eq!(r.already_at_target, 1);
        assert_eq!(r.shorter_than_target, 1);
        assert_eq!(r.degenerate, 1);

        // The sliced rows are now 4-dim unit vectors.
        for id in ["long-q", "long-raw"] {
            let blob: Vec<u8> = conn
                .query_row(
                    "SELECT embedding FROM memories WHERE id = ?1",
                    params![id],
                    |row| row.get(0),
                )
                .unwrap();
            let v = blob_to_float32(&blob);
            assert_eq!(v.len(), 4, "{id} sliced to 4-dim");
            let mag: f64 = v
                .iter()
                .map(|&x| (x as f64) * (x as f64))
                .sum::<f64>()
                .sqrt();
            assert!((mag - 1.0).abs() < 0.05, "{id} renormalised (mag {mag})");
        }
        // The degenerate + short + at-target rows are untouched dims.
        assert_eq!(
            blob_to_float32(
                &conn
                    .query_row(
                        "SELECT embedding FROM memories WHERE id = 'degenerate'",
                        [],
                        |r| r.get::<_, Vec<u8>>(0)
                    )
                    .unwrap()
            )
            .len(),
            8
        );
    }

    #[test]
    fn slice_and_normalize_without_normalize_keeps_raw_slice() {
        let (dst, mag) = slice_and_normalize(&[3.0, 4.0, 1.0, 1.0, 9.0], 4, false);
        assert_eq!(dst, vec![3.0f32, 4.0, 1.0, 1.0]);
        assert!((mag - (9.0f64 + 16.0 + 1.0 + 1.0).sqrt()).abs() < 1e-6);
    }
}
