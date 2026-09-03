//! The realign-`files.sha256` boot heal (v4 migration
//! `realign-file-entry-sha256-v1`, `0b0617fee` — bug 117's data pass).
//!
//! `services/chat_files.rs` hashed the *input* buffer and let the storage bridge
//! transcode afterwards, so a chat upload that arrived as PNG or JPEG was stored
//! as WebP under a row whose `sha256` named bytes that exist nowhere. The sibling
//! path (`services/image_job_storage.rs`) transcodes first and hashes second,
//! which is why every generated image joined cleanly and half the uploads did not
//! — in the instance that surfaced this, 118 of 239 uploaded images, all of them
//! converted WebP.
//!
//! Nothing was corrupted; the joins simply stopped meeting. `files.sha256` was
//! speaking input-hash and the mount index was speaking stored-hash, so every
//! cross-domain lookup returned an empty result its caller read as "no such
//! file". **The readers this column exists for, and the reason it matters:**
//!
//!   - [`crate::photos::auto_describe_attachment`] — a generated description
//!     never reached `doc_mount_file_links.description`/`extractedText`, so it
//!     was never chunked or embedded and the image was unsearchable
//!   - [`crate::tools::photo`] — `describe_image` and `attach_image` could not
//!     resolve a mount-link uuid to its FileEntry
//!   - [`crate::photos::photo_link_summary`] and its five callers — link
//!     summaries reported zero linkers
//!
//! The forward fix runs the bridge's own transcode before anything is hashed, so
//! one hash serves both dedup and the join; this repairs the rows written before
//! that. It walks every `files` row whose `storageKey` is a `mount-blob:` key,
//! reads the blob's own hash out of the MOUNT-INDEX partition, and writes it to
//! `files.sha256` where the two disagree. No bytes are touched and no blob is
//! re-hashed — `doc_mount_blobs.sha256` is recomputed from the actual bytes at
//! write time (`link_blob_content`), so it is already the trustworthy side of the
//! join.
//!
//! Idempotent: rows already in agreement are skipped, and a row whose blob has
//! gone missing is logged and left as it is rather than guessed at.
//!
//! ## The once-only mechanism — the P4.D140 ledger shape, chosen deliberately
//!
//! The pass is DATA-only with no schema delta to key off. Two established shapes
//! were available:
//!
//!   - **P4.D97's always-stamp** ([`super::thinking_prefill_retire_heal`]) writes
//!     the ledger row on every completed pass.
//!   - **P4.D140's honour-and-write-on-effect**
//!     ([`super::chat_activity_recompute_heal`]) — **chosen** (work order
//!     P4.D152 §D, binding). An existing `migrations_state` row from EITHER app
//!     is honoured (v4 running on the same instance will already have run its own
//!     migration — the free cross-app proof the 2026-09-02 dogfood pass got for
//!     bug 112), and the row is written only on a pass that realigned at least
//!     one row.
//!
//! ### ⚠ RECORDED DIVERGENCE — v4 stamps a zero-affected pass; v5 does not
//!
//! **MEASURED, not assumed** (v4 `migrations/index.ts:157-163` +
//! `realign-file-entry-sha256.ts:shouldRun`): v4's `shouldRun()` for THIS
//! migration is `COUNT(*) FROM files WHERE storageKey LIKE 'mount-blob:%' > 0` —
//! **presence, not drift**. So on an instance that has mount-blob FileEntries
//! which all AGREE, v4 runs, returns `itemsAffected: 0`, and its runner records
//! the ledger row anyway. (This is unlike the P4.D140 migration, whose own
//! `shouldRun()` tests for drift — the shape is borrowed, the v4 semantics are
//! not identical, and the difference is here rather than hidden.)
//!
//! v5 deliberately does NOT stamp there, because the risk is asymmetric. A v5
//! stamp on a pass that changed nothing tells a LATER v4 boot to skip a migration
//! that has never run on that instance — and a pre-4.9.0 v4 sharing the instance
//! is still writing drifted rows, so the drift can arrive after the stamp. Not
//! stamping costs only a cheap re-check on the next v5 boot: one indexed scan.
//! Both directions are pinned by `files_sha256_realign_heal_equivalence`'s ledger
//! comparand; when v4's own guard changes, that pin trips by design.
//!
//! A zero-mount-blob-row instance matches exactly: v4 skips without recording,
//! v5 returns [`RealignOutcome::NoDrift`] with `scanned: 0` and records nothing.
//!
//! ## Non-ports, named
//!
//! v4's runner UI has no v5 surface: the per-row `reportProgress(scanned, total,
//! 'files')` throttle and the `PRETTY_LABELS` loading-screen sentence
//! `'Matching each picture to its fingerprint, so nothing goes missing on the
//! shelves…'` are carried here for the day a loading screen exists. The v5 boot
//! log line carries the four counts instead.
//!
//! v4's `dependsOn` (`relink-files-to-mount-blobs-v1`,
//! `repair-mount-blob-sha256-from-bytes-v1`) is runner ordering with no v5
//! counterpart — v5 has no migration runner, and both predecessors are v4-era
//! passes whose effects a v5-written instance never needed.

use rusqlite::Connection;

use super::DbError;

/// v4's migration id — the ledger key both apps honour.
const MIGRATION_ID: &str = "realign-file-entry-sha256-v1";

/// v4 `BATCH_SIZE`. Batching is a memory bound, not a behaviour: the walk is a
/// keyset scan by `id` and the result is identical at any size (recorded in the
/// lane report — the >500-row case is a coverage floor, not a discriminator).
const BATCH_SIZE: usize = 500;

/// What one pass did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RealignOutcome {
    /// The ledger already carries the row (either app completed the pass).
    AlreadyCompleted,
    /// No `files` table, or the mount partition is not open — retried next boot,
    /// nothing stamped (v4's `shouldRun() === false` arm; its
    /// `fs.existsSync(getMountIndexDatabasePath())` gate maps to "the mount
    /// partition is open").
    NotApplicable,
    /// The pass ran and realigned nothing. v5 records NOTHING here and re-checks
    /// on the next boot. With `scanned: 0` that matches v4 exactly (its
    /// `shouldRun()` is false, so its runner never records); with `scanned > 0`
    /// it is the recorded divergence in the module header — v4's runner DOES
    /// write a zero-`itemsAffected` ledger row there.
    NoDrift {
        scanned: usize,
        orphaned: usize,
        malformed_key: usize,
    },
    /// The pass realigned at least one row and wrote the ledger row.
    Ran {
        scanned: usize,
        realigned: usize,
        orphaned: usize,
        malformed_key: usize,
    },
}

impl RealignOutcome {
    /// v4's summary sentence, byte-for-byte (`run()`'s `message`, and the
    /// `logger.info` it carries). Rendered for every pass that scanned rows.
    pub fn message(&self) -> Option<String> {
        let (scanned, realigned, orphaned, malformed_key) = match self {
            RealignOutcome::NoDrift {
                scanned,
                orphaned,
                malformed_key,
            } => (*scanned, 0usize, *orphaned, *malformed_key),
            RealignOutcome::Ran {
                scanned,
                realigned,
                orphaned,
                malformed_key,
            } => (*scanned, *realigned, *orphaned, *malformed_key),
            _ => return None,
        };
        Some(format!(
            "Scanned {scanned} mount-blob FileEntries; realigned {realigned} sha256 values; \
             {orphaned} orphaned (no matching blob), {malformed_key} malformed storage keys"
        ))
    }
}

struct FileRow {
    id: String,
    sha256: String,
    storage_key: String,
}

/// Run the realign once per instance, guarded by v4's own migration ledger.
/// `now_iso` stamps the rewritten `files.updatedAt` and the ledger's
/// `completedAt`/`lastChecked` (the caller passes [`crate::clock::now_iso`]).
///
/// `mount` is the mount-index partition; `None` is v4's "no mount-index database
/// present" arm.
pub fn realign_file_entry_sha256(
    main: &Connection,
    mount: Option<&Connection>,
    now_iso: &str,
) -> Result<RealignOutcome, DbError> {
    // The completed check comes FIRST, exactly as v4's runner orders it
    // (`isMigrationCompleted` before `shouldRun`).
    if table_exists(main, "migrations_state")? {
        let mut stmt = main.prepare("SELECT 1 FROM \"migrations_state\" WHERE \"id\" = ?1")?;
        if stmt.exists([MIGRATION_ID])? {
            return Ok(RealignOutcome::AlreadyCompleted);
        }
    }

    // v4 `shouldRun`: the `files` table, and the mount-index database present.
    if !table_exists(main, "files")? {
        return Ok(RealignOutcome::NotApplicable);
    }
    let Some(mount) = mount else {
        return Ok(RealignOutcome::NotApplicable);
    };
    // v4's `run()` opens the mount DB and would find no `doc_mount_blobs` on a
    // partition that has never had one; a missing table reads as "nothing to
    // realign", not as an abort.
    if !table_exists(mount, "doc_mount_blobs")? {
        return Ok(RealignOutcome::NotApplicable);
    }

    let mut scanned = 0usize;
    let mut orphaned = 0usize;
    let mut malformed_key = 0usize;
    let mut realigned = 0usize;

    let mut last_id = String::new();
    loop {
        let batch: Vec<FileRow> = {
            let mut stmt = main.prepare(
                "SELECT id, sha256, storageKey \
                   FROM \"files\" \
                  WHERE storageKey LIKE 'mount-blob:%' \
                    AND id > ?1 \
                  ORDER BY id \
                  LIMIT ?2",
            )?;
            let rows = stmt
                .query_map(rusqlite::params![last_id, BATCH_SIZE as i64], |r| {
                    Ok(FileRow {
                        id: r.get(0)?,
                        sha256: r.get(1)?,
                        storage_key: r.get(2)?,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;
            rows
        };
        if batch.is_empty() {
            break;
        }

        // Read the blob hashes up-front, apply the writes in one transaction
        // afterwards: a row whose blob has vanished must not abort the batch
        // around it.
        let mut updates: Vec<(String, String)> = Vec::new();
        for row in &batch {
            scanned += 1;
            // v4's private `parseBlobId` is `parseMountBlobStorageKey`'s rule
            // spelled a second time — same prefix test, same `sep < 1 || sep ==
            // len - 1` refusal — so the shared parser is what runs here.
            let blob_id =
                crate::services::file_storage::parse_mount_blob_storage_key(&row.storage_key)
                    .map(|(_, blob)| blob);
            let Some(blob_id) = blob_id else {
                malformed_key += 1;
                tracing::warn!(
                    context = %format!("migration.{MIGRATION_ID}"),
                    file_id = %row.id,
                    storage_key = %row.storage_key,
                    "Malformed mount-blob storage key; sha256 left untouched"
                );
                continue;
            };
            let blob_sha: Option<String> = mount
                .query_row(
                    "SELECT sha256 FROM \"doc_mount_blobs\" WHERE id = ?1",
                    rusqlite::params![blob_id],
                    |r| r.get::<_, String>(0),
                )
                .ok();
            let Some(blob_sha) = blob_sha else {
                orphaned += 1;
                tracing::warn!(
                    context = %format!("migration.{MIGRATION_ID}"),
                    file_id = %row.id,
                    blob_id = %blob_id,
                    "Mount blob missing for FileEntry; sha256 left untouched"
                );
                continue;
            };
            if blob_sha == row.sha256 {
                continue;
            }
            updates.push((row.id.clone(), blob_sha));
        }

        if !updates.is_empty() {
            let tx = main.unchecked_transaction()?;
            {
                let mut stmt =
                    tx.prepare("UPDATE \"files\" SET sha256 = ?1, updatedAt = ?2 WHERE id = ?3")?;
                for (id, sha) in &updates {
                    stmt.execute(rusqlite::params![sha, now_iso, id])?;
                    realigned += 1;
                }
            }
            tx.commit()?;
        }

        last_id = batch[batch.len() - 1].id.clone();
    }

    if realigned == 0 {
        // No ledger row. With `scanned == 0` this matches v4 (its `shouldRun()`
        // is false and its runner records nothing); with `scanned > 0` it is the
        // RECORDED DIVERGENCE in the module header — v4 stamps, v5 re-checks.
        return Ok(RealignOutcome::NoDrift {
            scanned,
            orphaned,
            malformed_key,
        });
    }

    let outcome = RealignOutcome::Ran {
        scanned,
        realigned,
        orphaned,
        malformed_key,
    };

    // The ledger write — v4's `migrations/state.ts` shapes verbatim (the P4.D140
    // heal's shapes, unchanged).
    if !table_exists(main, "migrations_state")? {
        main.execute_batch(
            "CREATE TABLE IF NOT EXISTS \"migrations_state\" (\n        \"id\" TEXT PRIMARY KEY,\n        \"completedAt\" TEXT NOT NULL,\n        \"quilltapVersion\" TEXT NOT NULL,\n        \"itemsAffected\" INTEGER NOT NULL DEFAULT 0,\n        \"message\" TEXT\n      );\n      CREATE TABLE IF NOT EXISTS \"migrations_metadata\" (\n        \"key\" TEXT PRIMARY KEY,\n        \"value\" TEXT NOT NULL\n      );",
        )?;
    }
    main.execute(
        "INSERT INTO \"migrations_state\" (id, completedAt, quilltapVersion, itemsAffected, message)\n         VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![
            MIGRATION_ID,
            now_iso,
            env!("CARGO_PKG_VERSION"),
            realigned as i64,
            outcome.message().expect("Ran renders a message")
        ],
    )?;
    for (k, v) in [
        ("lastChecked", now_iso),
        ("quilltapVersion", env!("CARGO_PKG_VERSION")),
    ] {
        main.execute(
            "INSERT INTO migrations_metadata (key, value) VALUES (?1, ?2)\n             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            rusqlite::params![k, v],
        )?;
    }

    Ok(outcome)
}

fn table_exists(conn: &Connection, name: &str) -> Result<bool, DbError> {
    let mut stmt =
        conn.prepare("SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1")?;
    Ok(stmt.exists([name])?)
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: &str = "2026-09-03T00:00:00.000Z";

    fn main_db() -> Connection {
        let db = Connection::open_in_memory().expect("open");
        db.execute_batch(
            "CREATE TABLE files (\n               id TEXT PRIMARY KEY,\n               sha256 TEXT NOT NULL,\n               storageKey TEXT,\n               updatedAt TEXT NOT NULL\n             );",
        )
        .expect("ddl");
        db
    }

    fn mount_db() -> Connection {
        let db = Connection::open_in_memory().expect("open");
        db.execute_batch(
            "CREATE TABLE doc_mount_blobs (\n               id TEXT PRIMARY KEY,\n               sha256 TEXT NOT NULL\n             );",
        )
        .expect("ddl");
        db
    }

    fn add_file(db: &Connection, id: &str, sha: &str, key: Option<&str>) {
        db.execute(
            "INSERT INTO files (id, sha256, storageKey, updatedAt) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![id, sha, key, "2020-01-01T00:00:00.000Z"],
        )
        .expect("file");
    }

    fn add_blob(db: &Connection, id: &str, sha: &str) {
        db.execute(
            "INSERT INTO doc_mount_blobs (id, sha256) VALUES (?1, ?2)",
            rusqlite::params![id, sha],
        )
        .expect("blob");
    }

    fn read(db: &Connection, id: &str) -> (String, String) {
        db.query_row(
            "SELECT sha256, updatedAt FROM files WHERE id = ?1",
            rusqlite::params![id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .expect("row")
    }

    #[test]
    fn a_drifted_row_is_realigned_and_an_agreeing_one_is_untouched() {
        let main = main_db();
        let mount = mount_db();
        add_blob(&mount, "b1", "stored-1");
        add_blob(&mount, "b2", "stored-2");
        add_file(&main, "f1", "input-1", Some("mount-blob:mp1:b1"));
        add_file(&main, "f2", "stored-2", Some("mount-blob:mp1:b2"));

        let out = realign_file_entry_sha256(&main, Some(&mount), NOW).expect("heal");
        assert_eq!(
            out,
            RealignOutcome::Ran {
                scanned: 2,
                realigned: 1,
                orphaned: 0,
                malformed_key: 0
            }
        );
        assert_eq!(read(&main, "f1"), ("stored-1".into(), NOW.into()));
        // v4 rewrites `updatedAt` only on the rows it changes.
        assert_eq!(
            read(&main, "f2"),
            ("stored-2".into(), "2020-01-01T00:00:00.000Z".into())
        );
    }

    #[test]
    fn a_missing_blob_and_a_malformed_key_are_counted_and_left_alone() {
        let main = main_db();
        let mount = mount_db();
        add_blob(&mount, "b1", "stored-1");
        add_file(&main, "f1", "input-1", Some("mount-blob:mp1:b1"));
        add_file(&main, "f2", "input-2", Some("mount-blob:mp1:gone"));
        // `sep == rest.len() - 1` → refused.
        add_file(&main, "f3", "input-3", Some("mount-blob:mp1:"));
        // Not a mount-blob key at all: the LIKE never selects it.
        add_file(&main, "f4", "input-4", Some("user/portrait.png"));

        let out = realign_file_entry_sha256(&main, Some(&mount), NOW).expect("heal");
        assert_eq!(
            out,
            RealignOutcome::Ran {
                scanned: 3,
                realigned: 1,
                orphaned: 1,
                malformed_key: 1
            }
        );
        assert_eq!(read(&main, "f2").0, "input-2");
        assert_eq!(read(&main, "f3").0, "input-3");
        assert_eq!(read(&main, "f4").0, "input-4");
        assert_eq!(
            out.message().unwrap(),
            "Scanned 3 mount-blob FileEntries; realigned 1 sha256 values; 1 orphaned \
             (no matching blob), 1 malformed storage keys"
        );
    }

    #[test]
    fn the_ledger_row_makes_the_pass_once_only() {
        let main = main_db();
        let mount = mount_db();
        add_blob(&mount, "b1", "stored-1");
        add_file(&main, "f1", "input-1", Some("mount-blob:mp1:b1"));

        assert!(matches!(
            realign_file_entry_sha256(&main, Some(&mount), NOW).expect("heal"),
            RealignOutcome::Ran { realigned: 1, .. }
        ));
        assert_eq!(
            realign_file_entry_sha256(&main, Some(&mount), "2026-09-04T00:00:00.000Z")
                .expect("again"),
            RealignOutcome::AlreadyCompleted
        );
    }

    #[test]
    fn a_no_drift_pass_stamps_nothing_so_a_later_v4_boot_still_runs_it() {
        let main = main_db();
        let mount = mount_db();
        add_blob(&mount, "b1", "stored-1");
        add_file(&main, "f1", "stored-1", Some("mount-blob:mp1:b1"));

        let out = realign_file_entry_sha256(&main, Some(&mount), NOW).expect("heal");
        assert_eq!(
            out,
            RealignOutcome::NoDrift {
                scanned: 1,
                orphaned: 0,
                malformed_key: 0
            }
        );
        assert!(!table_exists(&main, "migrations_state").unwrap());
    }

    #[test]
    fn no_files_table_or_no_mount_partition_is_not_applicable() {
        let bare = Connection::open_in_memory().expect("open");
        let mount = mount_db();
        assert_eq!(
            realign_file_entry_sha256(&bare, Some(&mount), NOW).expect("heal"),
            RealignOutcome::NotApplicable
        );

        let main = main_db();
        assert_eq!(
            realign_file_entry_sha256(&main, None, NOW).expect("heal"),
            RealignOutcome::NotApplicable
        );
        assert!(!table_exists(&main, "migrations_state").unwrap());
    }

    #[test]
    fn the_walk_passes_the_first_batch() {
        let main = main_db();
        let mount = mount_db();
        let n = BATCH_SIZE + 7;
        for i in 0..n {
            let id = format!("f{i:04}");
            let blob = format!("b{i:04}");
            add_blob(&mount, &blob, &format!("stored-{i}"));
            add_file(
                &main,
                &id,
                &format!("input-{i}"),
                Some(&format!("mount-blob:mp1:{blob}")),
            );
        }
        let out = realign_file_entry_sha256(&main, Some(&mount), NOW).expect("heal");
        assert_eq!(
            out,
            RealignOutcome::Ran {
                scanned: n,
                realigned: n,
                orphaned: 0,
                malformed_key: 0
            }
        );
        assert_eq!(
            read(&main, &format!("f{:04}", n - 1)).0,
            format!("stored-{}", n - 1)
        );
    }
}
