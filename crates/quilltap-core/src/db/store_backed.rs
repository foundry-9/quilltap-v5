//! The generic **store-backed repository** — v4's
//! `AbstractStoreBackedRepository` (`store-backed.repository.ts`) as a Rust
//! generic over a [`StoreEntity`]. Shared by `groups` and `projects` (the two
//! entities whose substantive content lives in their official document store,
//! not in DB columns).
//!
//! It is the chokepoint that hides the split: every read overlays the store
//! ([`super::document_store_overlay`]); every write routes store-resident fields
//! to the store and strips them from the slim DB row; `create` provisions and
//! populates the store before returning so a freshly-created entity is never
//! storeless.
//!
//! The slim row lives in the **main** DB (`E::slim_table()` — `id` / `name` /
//! `officialMountPointId` / timestamps); the store lives in the **mount-index**
//! DB. So the repository holds BOTH connections (mirrors v4's cross-backend
//! `getRepositories()`). Concrete repos (`GroupsRepository`, `ProjectsRepository`)
//! are thin wrappers that build the property bag and add entity-specific methods
//! (a project's character roster, etc.).

use std::marker::PhantomData;

use rusqlite::{params, Connection};
use serde_json::{Map, Value};

use super::document_store_overlay::{self as overlay, ManagedFields, OverlayError, StoreEntity};
use super::ensure_official_store::ensure_official_store;
use super::DbError;

/// Optional pinned id/timestamps (v4 `CreateOptions`). `None` → minted (the
/// remap-form differential mints everything).
#[derive(Default)]
pub struct StoreCreateOptions {
    pub id: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

/// Generic store-backed repository over the main DB connection (slim row) + the
/// mount-index connection (the store).
pub struct StoreBackedRepository<'c, E: StoreEntity> {
    main: &'c Connection,
    mount: &'c Connection,
    _entity: PhantomData<E>,
}

impl<'c, E: StoreEntity> StoreBackedRepository<'c, E> {
    pub fn new(main: &'c Connection, mount: &'c Connection) -> Self {
        Self {
            main,
            mount,
            _entity: PhantomData,
        }
    }

    /// The mount-index connection (used by entity-specific helpers, e.g. provisioning).
    pub fn mount(&self) -> &Connection {
        self.mount
    }

    // ── slim-row internals (store-aware `_create`/`_update` + raw reads) ──────

    /// Read one slim row as a JSON map (v4 `_findById` / `findByIdRaw`), or `None`
    /// when absent. Only the five slim columns are read; the store-resident
    /// columns (present in the table but never written by this repo) are ignored.
    pub fn find_by_id_raw(&self, id: &str) -> Result<Option<Map<String, Value>>, DbError> {
        self.main
            .query_row(
                &format!(
                    "SELECT id, name, officialMountPointId, createdAt, updatedAt \
                     FROM {} WHERE id = ?1",
                    E::slim_table()
                ),
                params![id],
                |row| Ok(slim_row_to_map(row)),
            )
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other.into()),
            })
    }

    /// All slim rows (v4 `findAllRaw`).
    pub fn find_all_raw(&self) -> Result<Vec<Map<String, Value>>, DbError> {
        let mut stmt = self.main.prepare(&format!(
            "SELECT id, name, officialMountPointId, createdAt, updatedAt FROM {}",
            E::slim_table()
        ))?;
        let rows = stmt
            .query_map([], |row| Ok(slim_row_to_map(row)))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Persist ONLY the `officialMountPointId` FK + bump `updatedAt`, bypassing the
    /// overlay (v4 `setOfficialMountPointId`). Used by provisioning before the
    /// store files exist.
    pub fn set_official_mount_point_id(
        &self,
        id: &str,
        mount_point_id: &str,
    ) -> Result<(), DbError> {
        self.main.execute(
            &format!(
                "UPDATE {} SET officialMountPointId = ?1, updatedAt = ?2 WHERE id = ?3",
                E::slim_table()
            ),
            params![mount_point_id, crate::clock::now_iso(), id],
        )?;
        Ok(())
    }

    /// Insert the slim row with a NULL FK (v4 store-aware `_create`). Mints id +
    /// timestamps unless pinned. Returns the created `(id, name)`.
    fn create_slim(
        &self,
        name: &str,
        opts: &StoreCreateOptions,
    ) -> Result<(String, String), DbError> {
        let now = crate::clock::now_iso();
        let id = opts
            .id
            .clone()
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let created_at = opts.created_at.clone().unwrap_or_else(|| now.clone());
        let updated_at = opts.updated_at.clone().unwrap_or(now);
        self.main.execute(
            &format!(
                "INSERT INTO {} (id, name, officialMountPointId, createdAt, updatedAt) \
                 VALUES (?1, ?2, NULL, ?3, ?4)",
                E::slim_table()
            ),
            params![id, name, created_at, updated_at],
        )?;
        Ok((id, name.to_string()))
    }

    /// Apply the DB-only remainder of an update to the slim row (v4 store-aware
    /// `_update`). The only slim non-id/timestamp column is `name`; `updatedAt` is
    /// always bumped. Returns `false` when the row is absent.
    fn update_slim(&self, id: &str, db_patch: &Map<String, Value>) -> Result<bool, DbError> {
        if self.find_by_id_raw(id)?.is_none() {
            return Ok(false);
        }
        if let Some(name) = db_patch.get("name").and_then(Value::as_str) {
            self.main.execute(
                &format!(
                    "UPDATE {} SET name = ?1, updatedAt = ?2 WHERE id = ?3",
                    E::slim_table()
                ),
                params![name, crate::clock::now_iso(), id],
            )?;
        } else {
            self.main.execute(
                &format!(
                    "UPDATE {} SET updatedAt = ?1 WHERE id = ?2",
                    E::slim_table()
                ),
                params![crate::clock::now_iso(), id],
            )?;
        }
        Ok(true)
    }

    // ── public CRUD (document-store overlay applied) ─────────────────────────

    /// Find by id, hydrated; **throws** `Unavailable` if the store is
    /// missing/unreadable (v4 `findById` → `applyOverlayOne`).
    pub fn find_by_id(&self, id: &str) -> Result<Option<Value>, OverlayError> {
        let raw = self.find_by_id_raw(id)?;
        overlay::apply_overlay_one::<E>(self.mount, raw)
    }

    /// Find all, each hydrated; a row whose store is unavailable is **dropped**
    /// (v4 `findAll` → `applyOverlay`).
    pub fn find_all(&self) -> Result<Vec<Value>, OverlayError> {
        let raw = self.find_all_raw()?;
        overlay::apply_overlay::<E>(self.mount, raw)
    }

    /// Create the entity, provision its official store, populate the four overlay
    /// files from `fields`, and return the overlaid entity (v4 `create` — the
    /// 5-step sequence). Fails hard if the store can't be provisioned.
    pub fn create(
        &self,
        name: &str,
        fields: &ManagedFields,
        opts: &StoreCreateOptions,
    ) -> Result<Value, OverlayError> {
        let (id, name) = self.create_slim(name, opts)?;
        let ensured =
            ensure_official_store::<E>(self.main, self.mount, &id, &name)?.ok_or_else(|| {
                OverlayError::Db(DbError::Key(format!(
                    "{} {id} disappeared during store provisioning",
                    E::entity_label()
                )))
            })?;
        overlay::write_managed_fields::<E>(self.mount, &ensured.mount_point_id, fields)?;
        self.find_by_id(&id)?.ok_or_else(|| {
            OverlayError::Db(DbError::Key(format!(
                "{} {id} disappeared immediately after creation",
                E::entity_label()
            )))
        })
    }

    /// Update: store-resident fields routed to the store, the DB-only remainder
    /// written through the slim `_update`; the result is overlaid (v4 `update`).
    pub fn update(
        &self,
        id: &str,
        patch: &Map<String, Value>,
    ) -> Result<Option<Value>, OverlayError> {
        let raw = self.find_by_id_raw(id)?;
        let db_patch = overlay::apply_write_overlay::<E>(self.mount, raw.as_ref(), patch)?;
        if !db_patch.is_empty() {
            self.update_slim(id, &db_patch)?;
        }
        let result = self.find_by_id_raw(id)?;
        overlay::apply_overlay_one::<E>(self.mount, result)
    }

    /// Delete the slim row (the official store is orphaned, per v4 `delete`).
    pub fn delete(&self, id: &str) -> Result<bool, DbError> {
        let affected = self.main.execute(
            &format!("DELETE FROM {} WHERE id = ?1", E::slim_table()),
            params![id],
        )?;
        Ok(affected > 0)
    }
}

/// Build a slim-row JSON map from a `SELECT id,name,officialMountPointId,
/// createdAt,updatedAt` row (the nullable FK → `Value::Null` when absent).
fn slim_row_to_map(row: &rusqlite::Row<'_>) -> Map<String, Value> {
    let mut m = Map::new();
    m.insert("id".into(), text_col(row, 0));
    m.insert("name".into(), text_col(row, 1));
    m.insert("officialMountPointId".into(), text_col(row, 2));
    m.insert("createdAt".into(), text_col(row, 3));
    m.insert("updatedAt".into(), text_col(row, 4));
    m
}

/// A TEXT-or-NULL column → `Value::String` / `Value::Null`.
fn text_col(row: &rusqlite::Row<'_>, idx: usize) -> Value {
    match row.get::<_, Option<String>>(idx) {
        Ok(Some(s)) => Value::String(s),
        _ => Value::Null,
    }
}
