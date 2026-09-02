//! One `folders` row per path — the port of v4's `collapse-duplicate-folders-v1`
//! migration (`migrations/scripts/collapse-duplicate-folders.ts`, v4
//! `a5df98b3f`, bug 114) as a v5 boot repair.
//!
//! The legacy `folders` table (the pre-Scriptorium file-tree UI) had no
//! uniqueness constraint on a folder's identity, and every writer hand-rolled
//! its own `findByPath` -> `create` guard. Those guards are neither atomic (two
//! concurrent image jobs both read "absent" and both insert) nor able to fail
//! loudly (v4's `findByPath` swallows read errors and returns null), so the
//! machine-written paths — `/character-avatars/` and `/story-backgrounds/`,
//! driven by the avatar and Lantern pipelines — accumulated one row per
//! generated image: 607 rows describing 24 folders on the real instance.
//! Hand-created folders, written once by a human through the API, never
//! duplicated.
//!
//! Three steps, v4's exactly:
//!   1. Group rows by `(userId, COALESCE(projectId, ''), path)`. The oldest row
//!      in each group survives (`ORDER BY createdAt ASC, id ASC` — id breaks
//!      ties so the choice is deterministic across re-runs); the rest are
//!      discarded.
//!   2. Repoint every `folders.parentFolderId` that named a discarded row at
//!      its group's survivor, THEN delete the discarded rows.
//!      `folders.parentFolderId` is the only column in the main database that
//!      references `folders.id` — `files` locates its folder by `folderPath` +
//!      `projectId`, not by id.
//!   3. Create the UNIQUE INDEX that keeps it from happening again.
//!
//! ## Why this is NOT in the `migrations_state` ledger idiom
//!
//! v4's `shouldRun()` for this migration is `!indexExists()` — a bare
//! `sqlite_master` lookup by NAME. It never reads `migrations_state`, and v4's
//! runner records nothing for a migration whose `shouldRun()` was false. So the
//! index itself is the cross-app once-only marker in BOTH directions: v4 first
//! => this ensure no-ops; v5 first => v4's `shouldRun()` is false and its runner
//! stamps nothing. **Writing a `collapse-duplicate-folders-v1` ledger row from
//! v5 would be wrong** — it would claim a completion v4 never claims, and the
//! differential asserts `migrations_state` stays empty on both sides. This is
//! the [`super::mount_index_case_repair`] idiom (collapse-then-index, guarded by
//! the index, no ledger row), not the P4.D97 / P4.D140 ledger idiom.
//!
//! The guard is v4's: index presence by NAME, not a check that the index is
//! *honest*. `mount_index_case_repair` is stricter because its invariant has no
//! v4 migration behind it; here a stricter guard would make v5 re-collapse an
//! instance v4 considers done.
//!
//! ## Fresh instances — the boot chain ONLY, measured
//!
//! v4's `generateDDL` builds indexes from a plain column list
//! (`schema-translator.ts` `generateCreateIndexes`) and cannot express
//! `COALESCE(...)`, so this index never reaches the fresh generateDDL surface on
//! EITHER side — v5's `fresh_schema.json` correctly does not carry it and no D23
//! re-dump is owed (confirmed by regenerating the provision oracle at the
//! `a5df98b3f` pin: v4's fresh `main` schema carries `idx_folders_createdAt` and
//! `idx_folders_userId`, and nothing else on `folders`).
//!
//! The work order's follow-on suggestion — call this from
//! `services::provisioning` as well, "the same hook the boot uses" — was
//! **REFUTED by measurement**. `provisioning_equivalence` compares v5's
//! provisioned `sqlite_master` byte-for-byte against v4's fresh generateDDL
//! surface, so creating the index there makes v5's dump carry an index v4's does
//! not, and the family reddens on `schema mismatch in partition main`
//! (measured, then reverted). It is also unnecessary: `Host::assemble` runs
//! `seed_built_ins` — and therefore this ensure — on EVERY open, including the
//! first one after Setup provisions. That is the same placement every other
//! re-homed v4 migration in v5 uses (P4.D63, P4.D73, P4.D77, P4.D79, P4.D97,
//! P4.D135, P4.D140), and it is where v4 gets the index too: from the migration
//! runner on first boot, not from `ensureCollection`.
//!
//! Deliberately WITHOUT v4's pretty progress label (`lib/startup/prettify.ts`'s
//! "Sweeping up the duplicate folders that crept into the filing cabinet..."):
//! v5 has no migration-runner progress screen — the standing non-port recorded
//! at [`super::chat_activity_recompute_heal`].

use std::collections::HashMap;

use rusqlite::{params, Connection, OptionalExtension};

use super::DbError;

/// v4's `INDEX_NAME`.
pub const FOLDERS_UNIQUE_PATH_INDEX: &str = "idx_folders_userId_projectId_path";

/// v4's `CREATE UNIQUE INDEX`, byte-for-byte (both the migration's and the one
/// `sqlite-initial-schema`'s `SQLITE_TABLES` now creates for fresh instances).
const CREATE_INDEX_SQL: &str =
    "CREATE UNIQUE INDEX IF NOT EXISTS \"idx_folders_userId_projectId_path\" \
     ON \"folders\" (\"userId\", COALESCE(\"projectId\", ''), \"path\")";

/// What the ensure did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CollapseOutcome {
    /// No `folders` table — a bare or loose-typed database. v4's `shouldRun()`
    /// is false; nothing is stamped and the next boot re-checks.
    NoTable,
    /// The index is already there, so v4's `shouldRun()` is false. The
    /// cross-app marker, in both directions.
    AlreadyIndexed,
    /// The pass ran. The counts are v4's `logger.info` fields.
    Ran {
        scanned: usize,
        surviving: usize,
        deleted: usize,
        repointed: usize,
    },
}

impl CollapseOutcome {
    /// v4's `MigrationResult.message`, byte-exact:
    /// `Collapsed ${n} duplicate folder row${s} into ${m} folder${s}`.
    pub fn message(&self) -> Option<String> {
        let CollapseOutcome::Ran {
            surviving, deleted, ..
        } = self
        else {
            return None;
        };
        Some(format!(
            "Collapsed {deleted} duplicate folder row{} into {surviving} folder{}",
            if *deleted == 1 { "" } else { "s" },
            if *surviving == 1 { "" } else { "s" },
        ))
    }

    /// v4's `MigrationResult.itemsAffected` — the discarded rows.
    pub fn items_affected(&self) -> usize {
        match self {
            CollapseOutcome::Ran { deleted, .. } => *deleted,
            _ => 0,
        }
    }
}

struct FolderRow {
    id: String,
    user_id: String,
    project_id: Option<String>,
    path: String,
    parent_folder_id: Option<String>,
}

fn table_exists(db: &Connection, name: &str) -> Result<bool, DbError> {
    Ok(db
        .query_row(
            "SELECT name FROM sqlite_master WHERE type = 'table' AND name = ?1",
            params![name],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .is_some())
}

/// v4's `indexExists()` — presence by name, nothing more.
pub fn index_exists(db: &Connection) -> Result<bool, DbError> {
    Ok(db
        .query_row(
            "SELECT name FROM sqlite_master WHERE type = 'index' AND name = ?1",
            params![FOLDERS_UNIQUE_PATH_INDEX],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .is_some())
}

/// Collapse duplicate `folders` rows and create the unique index.
///
/// `now_iso` stamps the repointed rows' `updatedAt` — v4 computes ONE
/// `new Date().toISOString()` before the repoint loop and reuses it for every
/// row, so the caller passes [`crate::clock::now_iso`] once.
pub fn ensure_folders_unique_path_index(
    db: &Connection,
    now_iso: &str,
) -> Result<CollapseOutcome, DbError> {
    // v4 `shouldRun()`: SQLite backend (always, here) AND the table exists AND
    // the index does NOT.
    if !table_exists(db, "folders")? {
        return Ok(CollapseOutcome::NoTable);
    }
    if index_exists(db)? {
        return Ok(CollapseOutcome::AlreadyIndexed);
    }

    // 1. Decide a survivor per identity group. Oldest row wins; id breaks ties
    //    so the choice is deterministic across re-runs.
    let folders: Vec<FolderRow> = {
        let mut stmt = db.prepare(
            "SELECT id, userId, projectId, path, parentFolderId FROM folders \
             ORDER BY createdAt ASC, id ASC",
        )?;
        let mapped = stmt.query_map([], |row| {
            Ok(FolderRow {
                id: row.get(0)?,
                user_id: row.get(1)?,
                project_id: row.get(2)?,
                path: row.get(3)?,
                parent_folder_id: row.get(4)?,
            })
        })?;
        mapped.collect::<Result<Vec<_>, _>>()?
    };

    let mut survivor_by_group: HashMap<String, String> = HashMap::new();
    // Discarded folder id -> the survivor it should be replaced with. Insertion
    // order is kept alongside so the deletes run in v4's `Map` order.
    let mut superseded_by: HashMap<String, String> = HashMap::new();
    let mut discarded_ids: Vec<String> = Vec::new();

    for folder in &folders {
        // v4 separates the key's three parts with NUL: it cannot appear in a
        // uuid or a path, so no two distinct identities can collide on one key
        // the way a space or a slash could.
        let group_key = format!(
            "{}\u{0}{}\u{0}{}",
            folder.user_id,
            folder.project_id.as_deref().unwrap_or(""),
            folder.path
        );
        match survivor_by_group.get(&group_key) {
            None => {
                survivor_by_group.insert(group_key, folder.id.clone());
            }
            Some(survivor) => {
                superseded_by.insert(folder.id.clone(), survivor.clone());
                discarded_ids.push(folder.id.clone());
            }
        }
    }

    // 2a. Repoint children of a discarded row at that row's survivor, so no
    //     `parentFolderId` is left naming a folder we are about to delete.
    let needing_repoint: Vec<&FolderRow> = folders
        .iter()
        .filter(|f| {
            f.parent_folder_id
                .as_deref()
                .is_some_and(|p| superseded_by.contains_key(p))
        })
        .collect();

    // v4 issues these as separate statements with no explicit transaction; v5
    // wraps the whole pass in ONE, so a crash midway cannot leave children
    // pointing at rows that are already gone. The index-presence guard means a
    // rolled-back pass is simply re-run on the next boot — the same recovery v4
    // relies on, with a smaller window (the P4.31 chokepoint precedent).
    let tx = db.unchecked_transaction()?;
    {
        let mut repoint =
            tx.prepare("UPDATE folders SET parentFolderId = ?1, updatedAt = ?2 WHERE id = ?3")?;
        for folder in &needing_repoint {
            let old_parent = folder.parent_folder_id.as_deref().unwrap_or_default();
            let new_parent = superseded_by
                .get(old_parent)
                .expect("filtered on membership")
                .clone();
            repoint.execute(params![new_parent, now_iso, folder.id])?;
            tracing::debug!(
                target: "quilltap::boot",
                context = "migration.collapse-duplicate-folders",
                folder_id = %folder.id,
                from = %old_parent,
                to = %new_parent,
                "Repointed folder parent to surviving duplicate",
            );
        }

        // 2b. Delete the discarded rows.
        let mut delete = tx.prepare("DELETE FROM folders WHERE id = ?1")?;
        for id in &discarded_ids {
            delete.execute(params![id])?;
        }
    }

    // 3. Create the unique index that makes a repeat impossible.
    tx.execute_batch(CREATE_INDEX_SQL)?;
    tx.commit()?;

    if !index_exists(db)? {
        // v4: `throw new Error('Unique index was not created')`, caught by its
        // own try/catch into a failed MigrationResult. v5 has no result row to
        // fail into, so it surfaces.
        return Err(DbError::Internal(
            "Unique index was not created".to_string(),
        ));
    }

    Ok(CollapseOutcome::Ran {
        scanned: folders.len(),
        surviving: survivor_by_group.len(),
        deleted: discarded_ids.len(),
        repointed: needing_repoint.len(),
    })
}
