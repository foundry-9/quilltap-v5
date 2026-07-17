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

/// Provision (or adopt) all three built-in mount stores — the three v4 migrations,
/// run as one idempotent unit. `main` holds the `instance_settings` pointers;
/// `mount_index` holds the mount rows and their folders.
pub fn ensure_builtin_mounts(main: &Connection, mount_index: &Connection) -> Result<(), DbError> {
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
        return Ok(());
    }

    // v4's migration `run()` calls `ensureMountIndexTables` first — create the
    // mount-index tables when absent. A no-op on a generateDDL-provisioned instance
    // (the tables already exist, in REAL-affinity form).
    ensure_mount_index_tables(mount_index)?;

    for spec in MOUNTS {
        ensure_one_mount(main, mount_index, spec)?;
    }
    Ok(())
}

/// v4 migration `ensureMountIndexTables` — `CREATE TABLE IF NOT EXISTS` the two
/// tables the provisioner writes, plus the folder-path index. Verbatim from the
/// migrations' `TABLE_DDL`. A no-op whenever the tables already exist (the
/// generateDDL provisioning path, and every re-run).
fn ensure_mount_index_tables(mount_index: &Connection) -> Result<(), DbError> {
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
    // file-links / mount-points), effectively once per process. v5 has no
    // per-repo lazy init; this boot hook is the single once-per-startup cadence,
    // so the three v4 call sites collapse to here (a documented non-divergence).
    // Each runs an unconditional case-collision repair (renaming ` (N)` losers,
    // keep-oldest), then trusts-or-recreates the genuine NOCASE unique index. A
    // no-op on a fresh generateDDL-provisioned instance (the NOCASE indexes exist
    // and no rows collide); an existing pre-`0a0419f5` instance is migrated here.
    mount_index_case_repair::ensure_folder_nocase_unique_index(mount_index)?;
    mount_index_case_repair::ensure_link_nocase_unique_index(mount_index)?;
    mount_index_case_repair::repair_mount_point_name_collisions(mount_index)?;
    Ok(())
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
        .map_err(|e| DbError::Key(format!("includePatterns serialize: {e}")))?;
    let exclude_patterns = serde_json::to_string(&[".git", "node_modules", ".obsidian", ".trash"])
        .map_err(|e| DbError::Key(format!("excludePatterns serialize: {e}")))?;

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
pub fn ensure_general_scenarios_folder(
    main: &Connection,
    mount_index: &Connection,
) -> Result<(), DbError> {
    let Some(mount_point_id) = instance_settings::get_general_mount_point_id(main)? else {
        return Ok(());
    };
    DocMountFileLinksRepository::new(mount_index)
        .ensure_folder_path(&mount_point_id, GENERAL_SCENARIOS_FOLDER)?;
    Ok(())
}
