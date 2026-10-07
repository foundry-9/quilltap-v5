//! The three built-in mount stores (P4.4u3, family 2).
//!
//! Ports v4's three provisioning MIGRATIONS —
//! `provision-lantern-backgrounds-mount.ts` / `provision-user-uploads-mount.ts` /
//! `provision-general-mount.ts` — which are identical bar the store name, the
//! `instance_settings` pointer key, and the scaffolded subfolders:
//!
//! | Store | pointer key | subfolders |
//! |---|---|---|
//! | Lantern Backgrounds | `lanternBackgroundsMountPointId` | `generated`, `tool` |
//! | Quilltap Uploads | `userUploadsMountPointId` | `chat`, `images`, `shell`, `diagnostics`, `restored`, `uploads` |
//! | Quilltap General | `generalMountPointId` | `Scenarios` |
//!
//! Each store is a `mountType='database'`, `storeType='documents'` GLOBAL mount
//! (no project link). v4's migration `run()` is a **provision-or-adopt** unit,
//! idempotent by the settings POINTER (not by name):
//!
//!   - the pointer is empty, OR points at a mount row that no longer exists → mint
//!     a fresh mount (new UUID), INSERT the row, upsert the pointer;
//!   - the pointer resolves to a live row → ADOPT it (no new row, pointer kept);
//!   - either way, ensure the subfolders (separately idempotent).
//!
//! This runs on every host assemble/unlock and in fresh-instance provisioning —
//! matching v4's every-startup migration-runner semantics. A real, pre-existing
//! (migration-vintage) instance ADOPTS its existing stores, never duplicating them.
//!
//! The `instance_settings` pointers live in the MAIN db; the mount rows +
//! subfolders live in the MOUNT-INDEX sibling db — so both connections are needed.

use rusqlite::{params, Connection, OptionalExtension};

use crate::clock;
use crate::db::doc_mount_file_links::DocMountFileLinksRepository;
use crate::db::doc_mount_points::DocMountPointsRepository;
use crate::db::mount_index_case_repair;
use crate::db::table_shape::EnsureFailures;
use crate::db::{instance_settings, DbError};

/// One built-in store's identity: its name, its pointer getter/setter, and its
/// scaffolded top-level subfolders. Registration order is Lantern → Uploads →
/// General (not load-bearing on a fresh instance, kept for determinism, matching
/// v4's instrumentation order).
struct MountSpec {
    name: &'static str,
    get_pointer: fn(&Connection) -> Result<Option<String>, DbError>,
    set_pointer: fn(&Connection, &str) -> Result<(), DbError>,
    subfolders: &'static [&'static str],
}

const MOUNTS: &[MountSpec] = &[
    MountSpec {
        name: "Lantern Backgrounds",
        get_pointer: instance_settings::get_lantern_backgrounds_mount_point_id,
        set_pointer: instance_settings::set_lantern_backgrounds_mount_point_id,
        subfolders: &["generated", "tool"],
    },
    MountSpec {
        name: "Quilltap Uploads",
        get_pointer: instance_settings::get_user_uploads_mount_point_id,
        set_pointer: instance_settings::set_user_uploads_mount_point_id,
        subfolders: &[
            "chat",
            "images",
            "shell",
            "diagnostics",
            "restored",
            "uploads",
        ],
    },
    MountSpec {
        name: "Quilltap General",
        get_pointer: instance_settings::get_general_mount_point_id,
        set_pointer: instance_settings::set_general_mount_point_id,
        subfolders: &["Scenarios"],
    },
];

/// v4 `GENERAL_SCENARIOS_FOLDER` — the folder `ensure_general_scenarios_folder`
/// re-creates at startup.
const GENERAL_SCENARIOS_FOLDER: &str = "Scenarios";

/// How [`ensure_builtin_mounts_with`] answers a failure in one of the five
/// sub-steps whose v4 home is NOT a migration (P4.134, dogfood #134(b)): the
/// three case repairs and the link-group column (v4's lazy repository
/// `ensureTable`, per access) and the orphaned store-children reap (v4's boot
/// phase 3.3b, a fallback read). Every other sub-step — the mount-index DDL,
/// the link-content backlog sweep (a ledger-gated v4 migration step) and the
/// three store provisions (v4 migrations) — PROPAGATES in both modes, because
/// v4's migration runner exits the process on a failed migration. That exit is
/// v4-fatal only on an instance whose ledger LACKS the migration: a
/// ledger-complete instance skips it before `shouldRun`
/// (`migrations/index.ts:145-148`) and then reaches these tables only through
/// the repositories — so there v5 is HARDER than v4 (the ledger-gate divergence
/// P4.134 named; ruled KEPT 2026-10-01, filed upstream as v4 bug 176). v4's
/// `e5c6bd0c0` fix for that bug did not change the ledger gate: it added a
/// read-only PHASE 3.1 pass that checks each structural table once per boot and
/// reports damage through a `structure` service in `/api/health`
/// (`lib/startup/verify-structural-tables.ts`). v5 ports it as
/// `db::table_shape::verify_structural_tables`, which REUSES the failure text
/// these lazy sub-steps record (the [`EnsureFailures`] this function returns)
/// instead of re-running them — so a damaged table here now degrades LOUDLY
/// behind a `degraded` `/health`, as on v4 (P4.D248). P4.135 pins v5's
/// every-boot cadence in `host_boot_hardness`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LazyRepairFailures {
    /// Every sub-step's `Err` propagates — fresh-instance provisioning, and
    /// every caller that wants a hard failure.
    Propagate,
    /// The five lazy-home sub-steps log v4's line (`db::fallback`) and the
    /// pass continues — the host boot (`seed_built_ins`).
    LogAndContinue,
}

/// Provision (or adopt) all three built-in mount stores — the three v4 migrations,
/// run as one idempotent unit. `main` holds the `instance_settings` pointers;
/// `mount_index` holds the mount rows and their folders. Every failure
/// propagates ([`LazyRepairFailures::Propagate`]).
pub fn ensure_builtin_mounts(main: &Connection, mount_index: &Connection) -> Result<(), DbError> {
    ensure_builtin_mounts_with(main, mount_index, LazyRepairFailures::Propagate).map(|_| ())
}

/// [`ensure_builtin_mounts`] with the five lazy-home sub-steps' failure mode
/// chosen by the caller (see [`LazyRepairFailures`]). One function with a mode,
/// not five public entry points, so the sub-step ORDER stays in one place.
///
/// Answers the lazy-home ensure failures it logged, by collection (the FIRST
/// per collection — v4's `ensureTable` stops at its first throw): the text the
/// boot's structural pass reports in v4's ensure form (P4.D248). Always empty
/// under [`LazyRepairFailures::Propagate`], where a failure is the `Err`.
pub fn ensure_builtin_mounts_with(
    main: &Connection,
    mount_index: &Connection,
    failures: LazyRepairFailures,
) -> Result<EnsureFailures, DbError> {
    // v4's migration `shouldRun` guards on `sqliteTableExists('instance_settings')`
    // — skip entirely on a bare / not-yet-provisioned db (e.g. a loose-typed test
    // fixture).
    let has_settings = main
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'instance_settings'",
            [],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    if !has_settings {
        return Ok(EnsureFailures::default());
    }

    // v4's migration `run()` calls `ensureMountIndexTables` first — create the
    // mount-index tables when absent. A no-op on a generateDDL-provisioned instance
    // (the tables already exist, in REAL-affinity form).
    let collected = ensure_mount_index_tables(mount_index, failures)?;

    for spec in MOUNTS {
        ensure_one_mount(main, mount_index, spec)?;
    }
    Ok(collected)
}

/// v4's label for the mount-index partition in `ensureTable`'s line
/// (`dedicated-db.repository.ts:54-57`, `DB_LABELS.mountIndex`).
const MOUNT_INDEX_LABEL: &str = "mount index";

/// One lazy-home sub-step under `failures`: `LogAndContinue` answers an `Err`
/// with v4's `ensureTable` line for `collections[0]` (once) and moves on,
/// recording the failure's text in `collected` for EVERY collection in
/// `collections` — the v4 repositories whose `ensureTable` runs this step
/// (P4.D248: the boot's structural pass reports it in v4's ensure form).
fn lazy_ensure(
    failures: LazyRepairFailures,
    collections: &[&'static str],
    collected: &mut EnsureFailures,
    ensure: impl FnOnce() -> Result<(), DbError>,
) -> Result<(), DbError> {
    match failures {
        LazyRepairFailures::Propagate => ensure(),
        LazyRepairFailures::LogAndContinue => {
            let mut text = None;
            crate::db::fallback::ensure_table_or_log(collections[0], MOUNT_INDEX_LABEL, || {
                ensure().inspect_err(|e| text = Some(crate::db::fallback::error_text(e)))
            });
            if let Some(text) = text {
                for collection in collections {
                    collected.record(collection, text.clone());
                }
            }
            Ok(())
        }
    }
}

/// v4's four `doc_mount_points` column self-heals
/// (`doc-mount-points.repository.ts:37-62`, `onTableEnsured`, run on every
/// boot since `e5c6bd0c0`'s PHASE 3.1 pass ensures each table): ONE
/// `table_info` read, then each absent column ADDed in v4's order with v4's
/// INFO line (root logger, no fields). A table predating any of the four is
/// healed and then reported SOUND, where v5 used to report it `degraded`.
///
/// NOT ported (P4.150 R-A, recorded): `alignDocMountPointsSchema`'s silent
/// 14-column set and `alignDocMountFileLinksSchema`'s three policy columns
/// (`migrations/lib/mount-index-schema.ts`) — ledger-gated provisioning
/// migrations every v4-booted instance has already run, not per-boot code.
fn ensure_doc_mount_points_columns(db: &Connection) -> Result<(), DbError> {
    let columns: Vec<String> = {
        let mut stmt = db.prepare("PRAGMA table_info(\"doc_mount_points\")")?;
        let names = stmt.query_map([], |row| row.get::<_, String>(1))?;
        names.collect::<Result<_, _>>()?
    };
    let has = |name: &str| columns.iter().any(|c| c == name);
    for (column, definition) in [
        ("totalSizeBytes", "INTEGER NOT NULL DEFAULT 0"),
        ("conversionStatus", "TEXT NOT NULL DEFAULT 'idle'"),
        ("conversionError", "TEXT DEFAULT NULL"),
        ("storeType", "TEXT NOT NULL DEFAULT 'documents'"),
    ] {
        if !has(column) {
            db.execute_batch(&format!(
                "ALTER TABLE \"doc_mount_points\" ADD COLUMN \"{column}\" {definition}"
            ))?;
            tracing::info!(
                target: "quilltap::db",
                "Migrated doc_mount_points: added {column} column"
            );
        }
    }
    Ok(())
}

/// v4 migration `ensureMountIndexTables` — `CREATE TABLE IF NOT EXISTS` the two
/// tables the provisioner writes, plus the folder-path index. Verbatim from the
/// migrations' `TABLE_DDL`. A no-op whenever the tables already exist (the
/// generateDDL provisioning path, and every re-run). The DDL itself always
/// propagates; the repairs after it follow `failures`.
///
/// The folder-path index here asks for UNIQUE, as v4's migrations make it. On
/// an instance provisioned before P4.153 a PLAIN `idx_doc_mount_folders_mp_path`
/// (`fresh_schema.json`'s copy) already holds the name, so this `IF NOT EXISTS`
/// is a silent no-op behind it; the conversion is P4.160's
/// `db::migration_index_family_repair`, which runs AFTER this in the same boot
/// (the last step of the host's `seed_built_ins`) and drops the plain copy and
/// creates the UNIQUE one in ONE transaction, after a duplicate pre-check. Were
/// the index ever ABSENT here, this statement would create it UNIQUE and fail
/// the boot on a duplicate path — which is why the backfill never leaves the
/// name dropped between boots.
fn ensure_mount_index_tables(
    mount_index: &Connection,
    failures: LazyRepairFailures,
) -> Result<EnsureFailures, DbError> {
    let mut collected = EnsureFailures::default();
    mount_index.execute_batch(
        "CREATE TABLE IF NOT EXISTS \"doc_mount_points\" (\
           \"id\" TEXT PRIMARY KEY, \"name\" TEXT NOT NULL, \
           \"basePath\" TEXT NOT NULL DEFAULT '', \
           \"mountType\" TEXT NOT NULL DEFAULT 'filesystem', \
           \"storeType\" TEXT NOT NULL DEFAULT 'documents', \
           \"includePatterns\" TEXT NOT NULL DEFAULT '[]', \
           \"excludePatterns\" TEXT NOT NULL DEFAULT '[]', \
           \"enabled\" INTEGER NOT NULL DEFAULT 1, \"lastScannedAt\" TEXT, \
           \"scanStatus\" TEXT NOT NULL DEFAULT 'idle', \"lastScanError\" TEXT, \
           \"conversionStatus\" TEXT NOT NULL DEFAULT 'idle', \"conversionError\" TEXT, \
           \"fileCount\" INTEGER NOT NULL DEFAULT 0, \"chunkCount\" INTEGER NOT NULL DEFAULT 0, \
           \"totalSizeBytes\" INTEGER NOT NULL DEFAULT 0, \
           \"createdAt\" TEXT NOT NULL, \"updatedAt\" TEXT NOT NULL);\
         CREATE TABLE IF NOT EXISTS \"doc_mount_folders\" (\
           \"id\" TEXT PRIMARY KEY, \"mountPointId\" TEXT NOT NULL, \"parentId\" TEXT, \
           \"name\" TEXT NOT NULL, \"path\" TEXT NOT NULL, \
           \"createdAt\" TEXT NOT NULL, \"updatedAt\" TEXT NOT NULL);\
         CREATE UNIQUE INDEX IF NOT EXISTS \"idx_doc_mount_folders_mp_path\" \
           ON \"doc_mount_folders\" (\"mountPointId\", \"path\");",
    )?;
    // v4 `0a0419f5`: the case-insensitive mount-namespace invariant. v4 calls the
    // three `ensure*NocaseUniqueIndex` / `repairMountPointNameCollisions` helpers
    // from each repo's lazy `getCollection()` table-init block (folders /
    // file-links / mount-points). v5 has no per-repo lazy init; this boot hook is
    // the single once-per-startup cadence, so the three v4 call sites collapse to
    // here. On CADENCE that is a non-divergence. On FAILURE it was not (dogfood
    // #134(b), fixed by P4.134): v4's `ensureTable` logs `Failed to ensure
    // <table> table in mount index database` and rethrows into the caller's
    // fallback read, per access — a damaged column there never stops v4's boot.
    // So under `LogAndContinue` each logs that line once and the pass goes on
    // (v4 per access, v5 once per boot — recorded). Each runs an unconditional
    // case-collision repair (renaming ` (N)` losers, keep-oldest), then
    // trusts-or-recreates the genuine NOCASE unique index. A no-op on a fresh
    // generateDDL-provisioned instance (the NOCASE indexes exist and no rows
    // collide); an existing pre-`0a0419f5` instance is migrated here.
    lazy_ensure(failures, &["doc_mount_folders"], &mut collected, || {
        mount_index_case_repair::ensure_folder_nocase_unique_index(mount_index)
    })?;
    // v4 `40319484` (migration `add-doc-mount-link-groups-v1`): deliberate
    // hard-link groups. Step 1 — the `linkGroupId` column + its partial index.
    // v4 calls this from the two repositories that name the column (the
    // file-links repo's `onTableEnsured`, `doc-mount-file-links.repository.ts:
    // 397-408`, and the documents repo's), because its `safeQuery` would
    // otherwise turn "no such column" into "every document silently does not
    // exist"; the two call sites collapse to this one boot hook, exactly as the
    // case-repair helpers do. It runs BEFORE the links NOCASE repair, as the
    // file-links repo's `onTableEnsured` orders them, and a failure is that
    // repo's `ensureTable` line. v4's one `try` logs ONE line per access for
    // the pair; v5 logs one per sub-step (TWO on a plant that fails both, the
    // column's error first — pinned below, recorded as the cadence
    // divergence's other half). Its failure is BOTH repositories' ensure error
    // (P4.D248, measured through v4's real pass: a squatted index name reports
    // `doc_mount_file_links …` AND `doc_mount_documents in mount index
    // database: …`) — one line, two recorded problems.
    lazy_ensure(
        failures,
        &["doc_mount_file_links", "doc_mount_documents"],
        &mut collected,
        || mount_index_case_repair::ensure_link_group_column(mount_index),
    )?;
    lazy_ensure(failures, &["doc_mount_file_links"], &mut collected, || {
        mount_index_case_repair::ensure_link_nocase_unique_index(mount_index)
    })?;
    // v4 `doc-mount-points.repository.ts:37-68`'s ONE `onTableEnsured`: the
    // four guarded ALTER self-heals (P4.150), THEN the name-collision repair —
    // one closure, as v4's one method, so a failed ALTER skips the repair (v4
    // never renames collision rows on a table it could not heal) and logs ONE
    // ensure line. The `07b8f0209` follow-ups unification folded the lane's
    // two sub-steps into this shape.
    lazy_ensure(failures, &["doc_mount_points"], &mut collected, || {
        ensure_doc_mount_points_columns(mount_index)?;
        mount_index_case_repair::repair_mount_point_name_collisions(mount_index).map(|_| ())
    })?;
    // Step 2 — collect the backlog of content rows abandoned by
    // content-addressed rewrites before the write path started reaping them.
    // v5 has no migration runner, so this is the boot-repair analogue: cheap and
    // idempotent once the backlog is gone. ALWAYS propagates: in v4 it is a
    // migration step, and a failed migration exits v4's process (on an
    // instance whose ledger lacks it — see `LazyRepairFailures`).
    crate::db::doc_mount_file_links::sweep_orphaned_link_content(mount_index)?;
    // P4.31 (dogfood finding #58): reap the links / folders / chunks whose mount
    // point is gone, and the content they were the last reference to. Since
    // v4's bug 9 v4 runs its own reaper at boot too (`instrumentation.ts`
    // phase 3.3b, `sweepOrphanedStoreChildren` — links / folders / documents),
    // so P4.31's "v4 has no such pass" no longer holds; what v4 reaps and what
    // v5 reaps still differ (chunks vs documents), recorded at the reaper.
    // v4's reaper is a FALLBACK read: a failure logs ERROR `Error sweeping
    // orphaned store children` and answers zeros, so under `LogAndContinue`
    // that is this site's line and the pass goes on (3.3b's own WARN is
    // unreachable behind it). Deliberately AFTER the backlog sweep above: that
    // one collects content with no link at all, this one collects links with
    // no store, and running the cheaper, older pass first leaves this one
    // strictly less to do.
    match failures {
        LazyRepairFailures::Propagate => {
            crate::db::doc_mount_file_links::sweep_orphaned_store_children(mount_index)?;
        }
        LazyRepairFailures::LogAndContinue => {
            crate::db::fallback::sweep_orphaned_store_children_or_default(|| {
                crate::db::doc_mount_file_links::sweep_orphaned_store_children(mount_index)
            });
        }
    }
    Ok(collected)
}

/// One store's provision-or-adopt (v4 migration `run()`).
fn ensure_one_mount(
    main: &Connection,
    mount_index: &Connection,
    spec: &MountSpec,
) -> Result<(), DbError> {
    // Resolve the pointer, then decide adopt-vs-mint (v4's run() re-check, which
    // is also what its shouldRun() gate tests).
    let mut mount_point_id = (spec.get_pointer)(main)?;
    if let Some(id) = &mount_point_id {
        let row_exists = DocMountPointsRepository::new(mount_index).exists(id)?;
        if !row_exists {
            // The pointer dangles (row manually deleted) → re-provision fresh.
            mount_point_id = None;
        }
    }

    let mount_point_id = match mount_point_id {
        Some(id) => id, // adopt the existing store
        None => {
            let id = uuid::Uuid::new_v4().to_string();
            insert_mount_row(mount_index, &id, spec.name)?;
            (spec.set_pointer)(main, &id)?;
            id
        }
    };

    // Always ensure the subfolders (separately idempotent, v4 does this on both
    // the mint and adopt paths).
    let links = DocMountFileLinksRepository::new(mount_index);
    for sub in spec.subfolders {
        links.ensure_folder_path(&mount_point_id, sub)?;
    }
    Ok(())
}

/// The verbatim `doc_mount_points` INSERT from v4's migrations (general :210-234).
/// Every column is fully specified — `mountType='database'`,
/// `storeType='documents'`, empty `basePath`, `enabled=1`, `includePatterns='[]'`,
/// the four fixed `excludePatterns`, zeroed counts, `idle` statuses, no project
/// link. The count literals bind as integers (as v4's `0` does); on the
/// generateDDL REAL-affinity columns SQLite stores them as `0.0` either way.
fn insert_mount_row(
    mount_index: &Connection,
    mount_point_id: &str,
    mount_name: &str,
) -> Result<(), DbError> {
    let now = clock::now_iso();
    let include_patterns = serde_json::to_string(&Vec::<String>::new())
        .map_err(|e| DbError::Internal(format!("includePatterns serialize: {e}")))?;
    let exclude_patterns = serde_json::to_string(&[".git", "node_modules", ".obsidian", ".trash"])
        .map_err(|e| DbError::Internal(format!("excludePatterns serialize: {e}")))?;

    mount_index.execute(
        "INSERT INTO \"doc_mount_points\" \
           (id, name, basePath, mountType, storeType, includePatterns, excludePatterns, \
            enabled, lastScannedAt, scanStatus, lastScanError, conversionStatus, conversionError, \
            fileCount, chunkCount, totalSizeBytes, createdAt, updatedAt) \
         VALUES (?1, ?2, '', 'database', 'documents', ?3, ?4, 1, NULL, 'idle', NULL, \
                 'idle', NULL, 0, 0, 0, ?5, ?6)",
        params![
            mount_point_id,
            mount_name,
            include_patterns,
            exclude_patterns,
            now,
            now
        ],
    )?;
    Ok(())
}

/// v4 `ensureGeneralScenariosFolder()` (`lib/mount-index/general-scenarios.ts:44`)
/// — the runtime re-ensure of the Quilltap General `Scenarios/` folder at startup.
/// A no-op (returns without error) when the General store is not yet provisioned,
/// matching v4's degrade-gracefully-during-the-race behavior.
///
/// P4.134: v4's function catches its own `ensureFolderPath` throw and logs WARN
/// `[GeneralScenarios] Failed to ensure Scenarios folder` `{mountPointId, error}`
/// (`:52-59`), so a damaged folder table never reaches `instrumentation.ts`'s
/// own WARN (`Error ensuring general scenarios folder, continuing startup`,
/// which the host keeps for the residual). The pointer read is already v4's
/// fallback `readSetting` (infallible here too), so the `Result` can no longer
/// be `Err` — unreachable in BOTH stacks; kept as the shape of v4's catch, which
/// the host's residual WARN mirrors as dead code on both sides.
pub fn ensure_general_scenarios_folder(
    main: &Connection,
    mount_index: &Connection,
) -> Result<(), DbError> {
    let Some(mount_point_id) = instance_settings::get_general_mount_point_id(main)? else {
        return Ok(());
    };
    if let Err(error) = DocMountFileLinksRepository::new(mount_index)
        .ensure_folder_path(&mount_point_id, GENERAL_SCENARIOS_FOLDER)
    {
        tracing::warn!(
            mountPointId = %mount_point_id,
            error = %crate::db::fallback::error_text(&error),
            "[GeneralScenarios] Failed to ensure Scenarios folder",
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Migration-vintage regression (v4 `0a0419f5`): a mount-index built with the
    /// LEGACY case-sensitive indexes and carrying planted case-collisions (a shape
    /// only a pre-`0a0419f5` instance or a raw restore can hold) is repaired by the
    /// boot hook `ensure_mount_index_tables` — the colliding rows get ` (N)`-suffixed
    /// and the unique NOCASE indexes replace the legacy ones. Proves the WIRING
    /// (the repair behavior itself is v4-differentialed by `mount_case_repair`).
    #[test]
    fn boot_hook_repairs_legacy_vintage_collisions() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE doc_mount_points (\
               id TEXT PRIMARY KEY, name TEXT NOT NULL, basePath TEXT NOT NULL DEFAULT '', \
               mountType TEXT NOT NULL DEFAULT 'filesystem', storeType TEXT NOT NULL DEFAULT 'documents', \
               includePatterns TEXT NOT NULL DEFAULT '[]', excludePatterns TEXT NOT NULL DEFAULT '[]', \
               enabled INTEGER NOT NULL DEFAULT 1, lastScannedAt TEXT, scanStatus TEXT NOT NULL DEFAULT 'idle', \
               lastScanError TEXT, conversionStatus TEXT NOT NULL DEFAULT 'idle', conversionError TEXT, \
               fileCount INTEGER NOT NULL DEFAULT 0, chunkCount INTEGER NOT NULL DEFAULT 0, \
               totalSizeBytes INTEGER NOT NULL DEFAULT 0, createdAt TEXT NOT NULL, updatedAt TEXT NOT NULL);\
             CREATE TABLE doc_mount_folders (\
               id TEXT PRIMARY KEY, mountPointId TEXT NOT NULL, parentId TEXT, name TEXT NOT NULL, \
               path TEXT NOT NULL, createdAt TEXT NOT NULL, updatedAt TEXT NOT NULL);\
             CREATE TABLE doc_mount_file_links (\
               id TEXT PRIMARY KEY, mountPointId TEXT NOT NULL, relativePath TEXT NOT NULL, \
               fileName TEXT NOT NULL, folderId TEXT, createdAt TEXT NOT NULL);\
             -- the legacy case-SENSITIVE indexes the repair must replace:
             CREATE UNIQUE INDEX \"idx_doc_mount_folders_mp_parent_name\" \
               ON \"doc_mount_folders\" (\"mountPointId\", COALESCE(\"parentId\", ''), \"name\");\
             CREATE UNIQUE INDEX \"idx_doc_mount_file_links_mp_path\" \
               ON \"doc_mount_file_links\" (\"mountPointId\", \"relativePath\");",
        )
        .unwrap();
        // Planted collisions (case-distinct → pass the legacy indexes cleanly).
        conn.execute_batch(
            "INSERT INTO doc_mount_folders (id, mountPointId, parentId, name, path, createdAt, updatedAt) VALUES \
               ('keep','mp',NULL,'Lore','Lore','2024-01-01T00:00:00.000Z','2024-01-01T00:00:00.000Z'),\
               ('lose','mp',NULL,'lore','lore','2024-06-01T00:00:00.000Z','2024-06-01T00:00:00.000Z');\
             INSERT INTO doc_mount_file_links (id, mountPointId, relativePath, fileName, folderId, createdAt) VALUES \
               ('la','mp','Notes.md','Notes.md',NULL,'2024-01-01T00:00:00.000Z'),\
               ('lb','mp','notes.md','notes.md',NULL,'2024-06-01T00:00:00.000Z');\
             INSERT INTO doc_mount_points (id, name, createdAt, updatedAt) VALUES \
               ('pa','My Vault','2024-01-01T00:00:00.000Z','2024-01-01T00:00:00.000Z'),\
               ('pb','my vault','2024-06-01T00:00:00.000Z','2024-06-01T00:00:00.000Z');",
        )
        .unwrap();

        ensure_mount_index_tables(&conn, LazyRepairFailures::Propagate).unwrap();

        let folder_name: String = conn
            .query_row(
                "SELECT name FROM doc_mount_folders WHERE id = 'lose'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(folder_name, "lore (2)", "colliding folder suffixed");
        let link_rel: String = conn
            .query_row(
                "SELECT relativePath FROM doc_mount_file_links WHERE id = 'lb'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            link_rel, "notes (2).md",
            "colliding link suffixed before ext"
        );
        let store_name: String = conn
            .query_row(
                "SELECT name FROM doc_mount_points WHERE id = 'pb'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(store_name, "my vault (2)", "colliding store name suffixed");

        // The unique NOCASE indexes replaced the legacy ones.
        let has = |name: &str| -> bool {
            conn.query_row(
                "SELECT 1 FROM sqlite_master WHERE type='index' AND name=?1",
                [name],
                |_| Ok(()),
            )
            .is_ok()
        };
        assert!(
            has("idx_doc_mount_folders_mp_parent_name_nocase"),
            "folder nocase index created"
        );
        assert!(
            has("idx_doc_mount_file_links_mp_path_nocase"),
            "link nocase index created"
        );
        assert!(
            !has("idx_doc_mount_folders_mp_parent_name"),
            "legacy folder index dropped"
        );
        assert!(
            !has("idx_doc_mount_file_links_mp_path"),
            "legacy link index dropped"
        );
    }

    /// P4.134 (dogfood #134(b)): the mode is the whole difference. The #134
    /// plant (a renamed `relativePath`) fails the file-links case repair;
    /// `Propagate` hands the bare `Err` up (provisioning's contract), while
    /// `LogAndContinue` logs v4's lazy `ensureTable` line and finishes the pass —
    /// the three stores still provisioned after it.
    #[test]
    fn the_failure_mode_decides_whether_a_lazy_repair_failure_stops_the_pass() {
        let plant = || {
            let main = Connection::open_in_memory().unwrap();
            main.execute_batch(
                "CREATE TABLE instance_settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);",
            )
            .unwrap();
            let mount = Connection::open_in_memory().unwrap();
            mount
                .execute_batch(
                    "CREATE TABLE doc_mount_file_links (\
                       id TEXT PRIMARY KEY, fileId TEXT NOT NULL, mountPointId TEXT NOT NULL, \
                       relativePath_x TEXT NOT NULL, fileName TEXT NOT NULL, folderId TEXT, \
                       createdAt TEXT NOT NULL, updatedAt TEXT NOT NULL);\
                     CREATE TABLE doc_mount_chunks (id TEXT PRIMARY KEY, mountPointId TEXT NOT NULL);",
                )
                .unwrap();
            (main, mount)
        };

        let (main, mount) = plant();
        let err = ensure_builtin_mounts(&main, &mount).unwrap_err();
        assert_eq!(
            crate::db::fallback::error_text(&err),
            "no such column: relativePath"
        );

        let (main, mount) = plant();
        let (result, lines) = crate::test_support::captured_with(|| {
            ensure_builtin_mounts_with(&main, &mount, LazyRepairFailures::LogAndContinue)
        });
        // P4.D248: the logged failure's text is what the boot's structural
        // pass reports (v4's ensure form) — recorded once, for the links
        // repository alone (the link-group column passed here).
        let collected = result.unwrap();
        assert_eq!(
            collected.get("doc_mount_file_links"),
            Some("no such column: relativePath")
        );
        assert_eq!(collected.get("doc_mount_documents"), None);
        assert_eq!(
            lines
                .iter()
                .filter(|l| l.starts_with("ERROR"))
                .collect::<Vec<_>>(),
            vec!["ERROR quilltap::db Failed to ensure doc_mount_file_links table in mount index database error=no such column: relativePath"],
        );
        let stores: i64 = mount
            .query_row("SELECT COUNT(*) FROM doc_mount_points", [], |r| r.get(0))
            .unwrap();
        assert_eq!(
            stores, 3,
            "the store provisions after the failed repair ran"
        );
    }

    /// P4.134 (the `ca363178d` unification's §3 catch): the two
    /// `doc_mount_file_links` lazy ensures run in v4's order — the link-group
    /// column (`onTableEnsured` `:400`) BEFORE the NOCASE repair (`:408`) — and
    /// a plant that fails both logs two lines in that order (v4's one `try`
    /// logs one per access; the cadence divergence's other half). A TABLE
    /// squatting on the partial index's name fails the column ensure (`CREATE
    /// INDEX IF NOT EXISTS` does not excuse a same-named table) and the renamed
    /// `relativePath` fails the repair's SELECT.
    #[test]
    fn the_link_group_column_ensure_runs_before_the_nocase_repair_as_v4_orders_them() {
        let main = Connection::open_in_memory().unwrap();
        main.execute_batch(
            "CREATE TABLE instance_settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);",
        )
        .unwrap();
        let mount = Connection::open_in_memory().unwrap();
        mount
            .execute_batch(&format!(
                "CREATE TABLE doc_mount_file_links (\
                   id TEXT PRIMARY KEY, fileId TEXT NOT NULL, mountPointId TEXT NOT NULL, \
                   relativePath_x TEXT NOT NULL, fileName TEXT NOT NULL, folderId TEXT, \
                   createdAt TEXT NOT NULL, updatedAt TEXT NOT NULL);\
                 CREATE TABLE \"{}\" (id TEXT PRIMARY KEY);\
                 CREATE TABLE doc_mount_chunks (id TEXT PRIMARY KEY, mountPointId TEXT NOT NULL);",
                mount_index_case_repair::LINK_GROUP_INDEX
            ))
            .unwrap();
        let (result, lines) = crate::test_support::captured_with(|| {
            ensure_builtin_mounts_with(&main, &mount, LazyRepairFailures::LogAndContinue)
        });
        // P4.D248: the FIRST failure per collection is the one recorded (v4's
        // `ensureTable` stops at its first throw), and the link-group column's
        // failure is BOTH repositories' — v4's documents repository runs the
        // same `ensureLinkGroupColumn` in its own `onTableEnsured`.
        let collected = result.unwrap();
        let squatted = format!(
            "there is already a table named {}",
            mount_index_case_repair::LINK_GROUP_INDEX
        );
        assert_eq!(
            collected.get("doc_mount_file_links"),
            Some(squatted.as_str())
        );
        assert_eq!(
            collected.get("doc_mount_documents"),
            Some(squatted.as_str())
        );
        let errors: Vec<&String> = lines.iter().filter(|l| l.starts_with("ERROR")).collect();
        assert_eq!(
            errors,
            vec![
                &format!(
                    "ERROR quilltap::db Failed to ensure doc_mount_file_links table in mount index database error=there is already a table named {}",
                    mount_index_case_repair::LINK_GROUP_INDEX
                ),
                &"ERROR quilltap::db Failed to ensure doc_mount_file_links table in mount index database error=no such column: relativePath".to_string(),
            ],
            "the column ensure's line comes first, then the repair's: {lines:?}"
        );
    }

    /// P4.134: v4's `ensureGeneralScenariosFolder` catches its own
    /// `ensureFolderPath` throw — WARN `[GeneralScenarios] Failed to ensure
    /// Scenarios folder` `{mountPointId, error}` (`general-scenarios.ts:52-59`)
    /// — and answers; the bare driver message, never `sqlite error: `. Not
    /// plantable through a real boot: the store provisions run the same
    /// `ensure_folder_path` first and stay fatal.
    #[test]
    fn a_failed_scenarios_folder_logs_v4s_own_warn_and_answers() {
        let main = Connection::open_in_memory().unwrap();
        main.execute_batch(
            "CREATE TABLE instance_settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);\
             INSERT INTO instance_settings VALUES ('generalMountPointId', 'gen-1');",
        )
        .unwrap();
        let mount = Connection::open_in_memory().unwrap();
        mount
            .execute_batch("CREATE TABLE doc_mount_folders (id TEXT PRIMARY KEY);")
            .unwrap();
        let (result, lines) =
            crate::test_support::captured_with(|| ensure_general_scenarios_folder(&main, &mount));
        result.unwrap();
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(
            lines[0].starts_with(
                "WARN quilltap_core::services::builtin_mounts [GeneralScenarios] Failed to ensure Scenarios folder mountPointId=gen-1 error=no such column: "
            ),
            "{}",
            lines[0]
        );
    }

    /// P4.31 WIRING pin (dogfood finding #58). `store_delete_equivalence`'s
    /// `reap_orphans` arm proves what the reaper DOES; it calls the sweep
    /// directly, because running the real boot hook against the fixture would
    /// mint the three built-in stores and move the census out from under the
    /// oracle. This proves the other half — that the boot hook actually calls
    /// it. Delete the `sweep_orphaned_store_children` line from
    /// `ensure_mount_index_tables` and this goes red; a semantics test would
    /// not (the P4.D41 lesson).
    #[test]
    fn boot_hook_reaps_parentless_store_children() {
        let main = Connection::open_in_memory().unwrap();
        main.execute_batch(
            "CREATE TABLE instance_settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);",
        )
        .unwrap();

        let mount = Connection::open_in_memory().unwrap();
        mount
            .execute_batch(
                "CREATE TABLE doc_mount_files (id TEXT PRIMARY KEY, sha256 TEXT NOT NULL);\
                 CREATE TABLE doc_mount_documents (id TEXT PRIMARY KEY, fileId TEXT NOT NULL);\
                 CREATE TABLE doc_mount_blobs (id TEXT PRIMARY KEY, fileId TEXT NOT NULL);\
                 CREATE TABLE doc_mount_file_links (\
                   id TEXT PRIMARY KEY, fileId TEXT NOT NULL, mountPointId TEXT NOT NULL, \
                   relativePath TEXT NOT NULL, fileName TEXT NOT NULL, folderId TEXT, \
                   createdAt TEXT NOT NULL, updatedAt TEXT NOT NULL);\
                 CREATE TABLE doc_mount_chunks (id TEXT PRIMARY KEY, mountPointId TEXT NOT NULL);\
                 -- The store this all belonged to is GONE, exactly as on the real
                 -- instance: 43 links + 118 folders across 21 vanished vaults.
                 INSERT INTO doc_mount_files VALUES ('f1','aa');\
                 INSERT INTO doc_mount_documents VALUES ('d1','f1');\
                 INSERT INTO doc_mount_blobs VALUES ('b1','f1');\
                 INSERT INTO doc_mount_file_links VALUES \
                   ('l1','f1','vanished','notes.md','notes.md',NULL,'2024-01-01T00:00:00.000Z',\
                    '2024-01-01T00:00:00.000Z');\
                 INSERT INTO doc_mount_chunks VALUES ('c1','vanished');",
            )
            .unwrap();
        // `doc_mount_points` + `doc_mount_folders` are created by the boot hook
        // itself; plant the orphaned folder once they exist.
        ensure_mount_index_tables(&mount, LazyRepairFailures::Propagate).unwrap();
        mount
            .execute_batch(
                "INSERT INTO doc_mount_folders (id, mountPointId, parentId, name, path, createdAt, updatedAt) \
                 VALUES ('fo1','vanished',NULL,'notes','notes','2024-01-01T00:00:00.000Z',\
                         '2024-01-01T00:00:00.000Z');",
            )
            .unwrap();

        let count = |sql: &str| -> i64 { mount.query_row(sql, [], |r| r.get(0)).unwrap() };
        assert_eq!(
            count("SELECT COUNT(*) FROM doc_mount_folders WHERE mountPointId='vanished'"),
            1
        );

        ensure_builtin_mounts(&main, &mount).unwrap();

        assert_eq!(
            count("SELECT COUNT(*) FROM doc_mount_file_links WHERE mountPointId='vanished'"),
            0
        );
        assert_eq!(
            count("SELECT COUNT(*) FROM doc_mount_folders WHERE mountPointId='vanished'"),
            0
        );
        assert_eq!(
            count("SELECT COUNT(*) FROM doc_mount_chunks WHERE mountPointId='vanished'"),
            0
        );
        assert_eq!(
            count("SELECT COUNT(*) FROM doc_mount_files"),
            0,
            "content collected"
        );
        assert_eq!(count("SELECT COUNT(*) FROM doc_mount_documents"), 0);
        assert_eq!(count("SELECT COUNT(*) FROM doc_mount_blobs"), 0);
        // The three built-in stores were still provisioned, each with folders of
        // its own — the reaper must not have taken those.
        assert_eq!(
            count("SELECT COUNT(*) FROM doc_mount_points"),
            3,
            "built-ins provisioned"
        );
        assert!(
            count("SELECT COUNT(*) FROM doc_mount_folders") > 0,
            "built-in folders kept"
        );
    }
}
