//! The doc-mount-points repository — the **widest mount-index sibling-DB repo**
//! of Phase 2. Ports v4's
//! `lib/database/repositories/doc-mount-points.repository.ts` (+ the
//! `_create`/`_update`/`_delete` internals of `base.repository.ts`).
//!
//! ## The sibling DB (mirrors `group_character_members`)
//!
//! Like every mount-index repo, v4 overrides `getCollection()` to route all
//! reads/writes to the dedicated mount-index database (`quilltap-mount-index.db`)
//! via `getRawMountIndexDatabase()`, isolating mount-tracking data from the main
//! DB. In the Rust port that routing is **not** a property of the repo — it is
//! the file the [`super::Writer`] was opened against. `Writer::open_writable`
//! opens any ChaCha20 file by path, so a writer opened on the mount-index DB
//! exposes these repos exactly as a main-DB writer exposes `users`/`folders`. The
//! repo code is therefore identical in shape to a plain main-DB repo; only the
//! harness points it at the mount-index fixture (the tier-2 case + builder target
//! `SQLITE_MOUNT_INDEX_PATH` and read back through `getRawMountIndexDatabase()`).
//!
//! ## The runtime `getCollection()` migrations are NO-OPS on a fresh fixture
//!
//! v4's overridden `getCollection()` runs four runtime `ALTER TABLE` "migrations"
//! (`totalSizeBytes`, `conversionStatus`, `conversionError`, `storeType`) on first
//! access — but each is guarded by `if (!columns.some(c => c.name === …))`. On a
//! FRESH fixture the table is created by `generateDDL` from the CURRENT schema,
//! which ALREADY contains all four columns, so every guard is `false` and every
//! migration is a no-op. The fresh fixture therefore has EXACTLY the schema
//! columns, in schema/Zod field order. We bind the INSERT in that exact order.
//!
//! Scope: `create`, `update`, and `delete` (the three abstract methods, each
//! delegating straight to `_create`/`_update`/`_delete`). The custom query
//! helpers — `findEnabled`, `findByName`, `countByName`, `updateLastScanned`,
//! `refreshStats`, `updateConversionStatus`, `updateScanStatus` — are out of
//! scope here.
//!
//! ## What this repo banks for the tier-2 marshaling surface
//!
//! The **widest of the mount-index family** — the first mount-index repo with
//! enums, booleans, JSON arrays, and REAL-int columns:
//!
//!   - **two JSON array columns** — `includePatterns` AND `excludePatterns`, both
//!     `z.array(z.string())` → compact JSON text via `serde_json::to_string` of a
//!     `Vec<String>` (order-preserving, no key-order subtlety). Non-empty and
//!     empty arrays both exercised.
//!   - a **boolean column** (`enabled`, `z.boolean()`) → INTEGER 0/1, bound as
//!     `i64::from(bool)`. Both 0 and 1 banked.
//!   - **three REAL-affinity int columns** (`fileCount`, `chunkCount`,
//!     `totalSizeBytes`, all `z.number().int().default(0)`). Per v4's
//!     `mapToSQLiteType`, a number maps to INTEGER only with BOTH an integer min
//!     AND an integer max; `.int()` alone (no min/max) → REAL. So these are REAL —
//!     bound as `f64`. The canonical dump's [`super::js_number_to_json`] collapses
//!     an integer-valued REAL (e.g. `0.0`) back to a JSON integer (`0`), so it
//!     matches v4 byte-for-byte. (Same idiom as `files.size`.) Both 0 and >0
//!     banked.
//!   - **four enum TEXT columns** (`mountType`, `storeType`, `scanStatus`,
//!     `conversionStatus`) — all stored as plain TEXT.
//!   - **three nullable string/timestamp columns** (`lastScannedAt`,
//!     `lastScanError`, `conversionError`) → `Option<String>` (`None` → SQL NULL).
//!     Both null and non-null banked.
//!   - plain required TEXT (`name`, `basePath`).
//!
//! To sidestep Zod-default-replication subtlety the corpus provides ALL fields
//! explicitly on every create, so v4's `.default(...)` never fires and both sides
//! insert identical bytes. [`DmpCreate`] therefore carries all 16 non-id/timestamp
//! fields.
//!
//! Determinism: the tier-2 case pins the id and timestamps (CreateOptions on
//! create; an explicit `updatedAt` in the update patch), so the persisted rows
//! match v4's byte-for-byte with no normalization — the pinned form the prior
//! Phase-2 repos use.
//!
//! Deferred (not in the corpus, mirroring the other repos): clearing a nullable
//! column **to NULL** via `update` — the patch models a provided field as "set to
//! this value", so a nullable setter lands when an op needs it. The corpus sets
//! nullables to values or leaves them.

use rusqlite::types::ToSql;
use rusqlite::{params, Connection};

use super::DbError;

/// Fields for creating a doc mount point (the
/// `Omit<DocMountPoint,'id'|timestamps>` shape). All 16 non-id/timestamp fields
/// are present so the corpus can set them explicitly (no Zod default fires).
pub struct DmpCreate {
    /// `z.string().min(1)` → required TEXT.
    pub name: String,
    /// `z.string().default('')` → TEXT (corpus always sets it).
    pub base_path: String,
    /// `enum['filesystem','obsidian','database']` → TEXT.
    pub mount_type: String,
    /// `enum['documents','character']` → TEXT.
    pub store_type: String,
    /// `z.array(z.string())` → compact JSON text (`["*.md"]`, `[]` when empty).
    pub include_patterns: Vec<String>,
    /// `z.array(z.string())` → compact JSON text.
    pub exclude_patterns: Vec<String>,
    /// `z.boolean()` → INTEGER 0/1.
    pub enabled: bool,
    /// `TimestampSchema.nullable().optional()` → NULLABLE TEXT (`None` → NULL).
    pub last_scanned_at: Option<String>,
    /// `enum['idle','scanning','error']` → TEXT.
    pub scan_status: String,
    /// `z.string().nullable().optional()` → NULLABLE TEXT.
    pub last_scan_error: Option<String>,
    /// `enum['idle','converting','deconverting','error']` → TEXT.
    pub conversion_status: String,
    /// `z.string().nullable().optional()` → NULLABLE TEXT.
    pub conversion_error: Option<String>,
    /// `z.number().int().default(0)` → REAL → `f64` (integer-valued collapses).
    pub file_count: f64,
    /// `z.number().int().default(0)` → REAL → `f64`.
    pub chunk_count: f64,
    /// `z.number().int().default(0)` → REAL → `f64`.
    pub total_size_bytes: f64,
}

/// Pinned id + timestamps (v4's `CreateOptions`).
pub struct CreateOptions {
    pub id: String,
    pub created_at: String,
    pub updated_at: String,
}

/// A mount-point update patch. Mirrors v4 `update` over `_update`: provided fields
/// overwrite, id and createdAt are preserved, `updatedAt` is set explicitly. Each
/// `Some` field sets that column; clearing a nullable column to NULL is deferred
/// (see header). Covers every settable column.
#[derive(Default)]
pub struct DmpUpdate {
    pub name: Option<String>,
    pub base_path: Option<String>,
    pub mount_type: Option<String>,
    pub store_type: Option<String>,
    /// Re-serialized to compact JSON text when provided.
    pub include_patterns: Option<Vec<String>>,
    /// Re-serialized to compact JSON text when provided.
    pub exclude_patterns: Option<Vec<String>>,
    pub enabled: Option<bool>,
    pub last_scanned_at: Option<String>,
    pub scan_status: Option<String>,
    pub last_scan_error: Option<String>,
    pub conversion_status: Option<String>,
    pub conversion_error: Option<String>,
    pub file_count: Option<f64>,
    pub chunk_count: Option<f64>,
    pub total_size_bytes: Option<f64>,
    pub updated_at: String,
}

/// Repository over a borrowed connection (held by the [`super::Writer`]).
pub struct DocMountPointsRepository<'c> {
    conn: &'c Connection,
}

impl<'c> DocMountPointsRepository<'c> {
    pub fn new(conn: &'c Connection) -> Self {
        Self { conn }
    }

    /// Insert a mount point with the given pinned id + timestamps. The two array
    /// columns → compact JSON array text; `enabled` binds `i64::from(bool)`;
    /// `fileCount`/`chunkCount`/`totalSizeBytes` bind `f64` (REAL); the enum TEXT
    /// columns and nullable strings pass through (`None` → SQL NULL). Columns are
    /// bound in schema/Zod field order (= on-disk order on a fresh fixture).
    pub fn create(&self, data: &DmpCreate, opts: &CreateOptions) -> Result<(), DbError> {
        let include_json = serde_json::to_string(&data.include_patterns)
            .map_err(|e| DbError::Key(format!("includePatterns serialize: {e}")))?;
        let exclude_json = serde_json::to_string(&data.exclude_patterns)
            .map_err(|e| DbError::Key(format!("excludePatterns serialize: {e}")))?;

        self.conn.execute(
            "INSERT INTO doc_mount_points \
               (id, name, basePath, mountType, storeType, includePatterns, excludePatterns, \
                enabled, lastScannedAt, scanStatus, lastScanError, conversionStatus, \
                conversionError, fileCount, chunkCount, totalSizeBytes, createdAt, updatedAt) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, \
                     ?17, ?18)",
            params![
                opts.id,
                data.name,
                data.base_path,
                data.mount_type,
                data.store_type,
                include_json,
                exclude_json,
                i64::from(data.enabled),
                data.last_scanned_at,
                data.scan_status,
                data.last_scan_error,
                data.conversion_status,
                data.conversion_error,
                data.file_count,
                data.chunk_count,
                data.total_size_bytes,
                opts.created_at,
                opts.updated_at,
            ],
        )?;
        Ok(())
    }

    /// Apply an update patch to the mount point `id`. Returns `Ok(false)` when no
    /// row matched (v4's "not found -> null"). id and createdAt are never touched.
    /// Each `Some` field sets that column; `updatedAt` is always set.
    pub fn update(&self, id: &str, patch: &DmpUpdate) -> Result<bool, DbError> {
        // v4 `_update` first `findById`s — the row must exist or it's a no-op.
        if !self.row_exists(id)? {
            return Ok(false);
        }

        let mut assignments: Vec<String> = Vec::new();
        let mut values: Vec<Box<dyn ToSql>> = Vec::new();

        if let Some(name) = &patch.name {
            assignments.push(format!("name = ?{}", values.len() + 1));
            values.push(Box::new(name.clone()));
        }
        if let Some(base_path) = &patch.base_path {
            assignments.push(format!("basePath = ?{}", values.len() + 1));
            values.push(Box::new(base_path.clone()));
        }
        if let Some(mount_type) = &patch.mount_type {
            assignments.push(format!("mountType = ?{}", values.len() + 1));
            values.push(Box::new(mount_type.clone()));
        }
        if let Some(store_type) = &patch.store_type {
            assignments.push(format!("storeType = ?{}", values.len() + 1));
            values.push(Box::new(store_type.clone()));
        }
        if let Some(include_patterns) = &patch.include_patterns {
            let include_json = serde_json::to_string(include_patterns)
                .map_err(|e| DbError::Key(format!("includePatterns serialize: {e}")))?;
            assignments.push(format!("includePatterns = ?{}", values.len() + 1));
            values.push(Box::new(include_json));
        }
        if let Some(exclude_patterns) = &patch.exclude_patterns {
            let exclude_json = serde_json::to_string(exclude_patterns)
                .map_err(|e| DbError::Key(format!("excludePatterns serialize: {e}")))?;
            assignments.push(format!("excludePatterns = ?{}", values.len() + 1));
            values.push(Box::new(exclude_json));
        }
        if let Some(enabled) = patch.enabled {
            assignments.push(format!("enabled = ?{}", values.len() + 1));
            values.push(Box::new(i64::from(enabled)));
        }
        if let Some(last_scanned_at) = &patch.last_scanned_at {
            assignments.push(format!("lastScannedAt = ?{}", values.len() + 1));
            values.push(Box::new(last_scanned_at.clone()));
        }
        if let Some(scan_status) = &patch.scan_status {
            assignments.push(format!("scanStatus = ?{}", values.len() + 1));
            values.push(Box::new(scan_status.clone()));
        }
        if let Some(last_scan_error) = &patch.last_scan_error {
            assignments.push(format!("lastScanError = ?{}", values.len() + 1));
            values.push(Box::new(last_scan_error.clone()));
        }
        if let Some(conversion_status) = &patch.conversion_status {
            assignments.push(format!("conversionStatus = ?{}", values.len() + 1));
            values.push(Box::new(conversion_status.clone()));
        }
        if let Some(conversion_error) = &patch.conversion_error {
            assignments.push(format!("conversionError = ?{}", values.len() + 1));
            values.push(Box::new(conversion_error.clone()));
        }
        if let Some(file_count) = patch.file_count {
            assignments.push(format!("fileCount = ?{}", values.len() + 1));
            values.push(Box::new(file_count));
        }
        if let Some(chunk_count) = patch.chunk_count {
            assignments.push(format!("chunkCount = ?{}", values.len() + 1));
            values.push(Box::new(chunk_count));
        }
        if let Some(total_size_bytes) = patch.total_size_bytes {
            assignments.push(format!("totalSizeBytes = ?{}", values.len() + 1));
            values.push(Box::new(total_size_bytes));
        }
        assignments.push(format!("updatedAt = ?{}", values.len() + 1));
        values.push(Box::new(patch.updated_at.clone()));

        let id_idx = values.len() + 1;
        values.push(Box::new(id.to_string()));

        let sql = format!(
            "UPDATE doc_mount_points SET {} WHERE id = ?{}",
            assignments.join(", "),
            id_idx
        );

        let params_refs: Vec<&dyn ToSql> = values.iter().map(|b| b.as_ref()).collect();
        let affected = self.conn.execute(&sql, params_refs.as_slice())?;
        Ok(affected > 0)
    }

    /// Delete the mount point `id`. Returns `Ok(false)` when no row matched (v4's
    /// `_delete` "deletedCount === 0 -> false").
    pub fn delete(&self, id: &str) -> Result<bool, DbError> {
        let affected = self
            .conn
            .execute("DELETE FROM doc_mount_points WHERE id = ?1", params![id])?;
        Ok(affected > 0)
    }

    /// True iff a row with this id exists — v4's `_update` `findById` precondition
    /// (a missing target makes the update a no-op returning `null`).
    fn row_exists(&self, id: &str) -> Result<bool, DbError> {
        let found: Option<i64> = self
            .conn
            .query_row(
                "SELECT 1 FROM doc_mount_points WHERE id = ?1",
                params![id],
                |row| row.get::<_, i64>(0),
            )
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other),
            })?;
        Ok(found.is_some())
    }

    /// v4 `findById` — true iff a mount point with this id still exists. The
    /// provisioning flow ([`super::ensure_official_store`]) uses it to validate a
    /// stale `officialMountPointId` FK and to confirm a linked store before
    /// adopting it. (The pilot only needs existence; the full row is not yet read
    /// because the adopt branch's name/type checks are not exercised by the
    /// groups corpus, which always provisions fresh.)
    pub fn exists(&self, id: &str) -> Result<bool, DbError> {
        self.row_exists(id)
    }

    /// All mount-point `name`s currently in use — v4 reads `findAll()` then
    /// `new Set(mp.name)` to feed `nextUniqueMountPointName` when minting a fresh
    /// store name. Returns the raw names (uniquification is the caller's job).
    pub fn find_all_names(&self) -> Result<Vec<String>, DbError> {
        let mut stmt = self.conn.prepare("SELECT name FROM doc_mount_points")?;
        let names = stmt
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(names)
    }
}
