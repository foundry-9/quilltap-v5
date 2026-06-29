//! The `groups` repository — the **store-backed pilot** of the document-store
//! overlay slice. Ports v4's `GroupsRepository` +
//! `AbstractStoreBackedRepository` (`store-backed.repository.ts`) bound to the
//! group overlay (`lib/groups/group-store/*`).
//!
//! A group's substantive content does NOT live in `groups` columns. The DB row
//! is slim — `id` / `name` / `officialMountPointId` / timestamps — and everything
//! else (`description` / `instructions` / `state` + the `properties.json` bag:
//! `color` / `icon`) lives in the group's official document store as the four
//! overlay files. The slim row lives in the **main** DB; the store (mount point,
//! links, file/document/folder rows) lives in the **mount-index** DB — so this
//! repo holds BOTH connections.
//!
//! Reads overlay the store ([`super::document_store_overlay`]); writes route
//! store-resident fields to the store and strip them from the slim patch; create
//! provisions + populates the store before returning so a freshly-created group
//! is never storeless (the 5-step sequence: insert slim row → provision official
//! store → set FK raw → write the four files → overlay re-read).
//!
//! Groups is the smallest store-backed surface (a 2-key property bag, no roster,
//! no subclass methods), so it proves the whole engine with the least incidental
//! marshaling; `projects` reuses the same engine with a larger bag + roster ops.

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::document_store_overlay::{self as overlay, ManagedFields, OverlayError, StoreEntity};
use super::ensure_official_store::ensure_group_official_store;
use super::DbError;

/// The `properties.json` bag (v4 `GroupPropertiesSchema`). Both keys are
/// `.nullable().optional()`; serialized in schema-declaration order with
/// `skip_serializing_if` so an absent key stays absent — matching
/// `JSON.stringify(parse(x), null, 2)` byte-for-byte (the dedup sha depends on
/// it). NB an explicit `null` value is treated as absent here (serde `Option`
/// folds `null`→`None`); the corpus keeps `color`/`icon` to present-or-absent, so
/// the null-vs-absent distinction (the open-JSON insertion/null seam) is not
/// exercised — a tracked deferral, same family as `state` multi-key order.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct GroupProperties {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
}

/// The group's [`StoreEntity`] binding for the generic overlay engine.
pub struct GroupEntity;

impl StoreEntity for GroupEntity {
    type Properties = GroupProperties;

    fn entity_label() -> &'static str {
        "group"
    }

    fn property_keys() -> &'static [&'static str] {
        &["color", "icon"]
    }

    fn parse_properties(value: &Value) -> Result<GroupProperties, String> {
        if !value.is_object() {
            return Err(format!("expected a JSON object, got: {value}"));
        }
        serde_json::from_value(value.clone()).map_err(|e| e.to_string())
    }
}

/// Create payload for a group — the hydrated, app-facing fields. The store-bound
/// fields (`description`/`instructions`/`state`/`color`/`icon`) are written to
/// the store, not the slim row.
pub struct GroupCreateInput {
    pub name: String,
    pub description: Option<String>,
    pub instructions: Option<String>,
    /// Arbitrary JSON (`null`/absent → `{}`). Kept `{}`/single-key in the corpus
    /// (the open-JSON multi-key order seam).
    pub state: Value,
    pub color: Option<String>,
    pub icon: Option<String>,
}

/// Optional pinned id/timestamps (v4 `CreateOptions`). `None` → minted (the
/// remap-form differential mints everything).
#[derive(Default)]
pub struct GroupCreateOptions {
    pub id: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

/// Repository over the main DB connection (slim `groups` row) + the mount-index
/// connection (the store). Mirrors v4's cross-backend `getRepositories()`.
pub struct GroupsRepository<'c> {
    main: &'c Connection,
    mount: &'c Connection,
}

impl<'c> GroupsRepository<'c> {
    pub fn new(main: &'c Connection, mount: &'c Connection) -> Self {
        Self { main, mount }
    }

    // ========================================================================
    // Slim-row internals (the store-aware `_create`/`_update`/raw reads)
    // ========================================================================

    /// Read one slim row as a JSON map (v4 `_findById` / `findByIdRaw`), or `None`
    /// when absent. Only the five slim columns are read; the store-resident
    /// columns (present in the table but never written by this repo) are ignored.
    pub fn find_by_id_raw(&self, id: &str) -> Result<Option<Map<String, Value>>, DbError> {
        self.main
            .query_row(
                "SELECT id, name, officialMountPointId, createdAt, updatedAt \
                 FROM groups WHERE id = ?1",
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
        let mut stmt = self
            .main
            .prepare("SELECT id, name, officialMountPointId, createdAt, updatedAt FROM groups")?;
        let rows = stmt
            .query_map([], |row| Ok(slim_row_to_map(row)))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Persist ONLY the `officialMountPointId` FK + bump `updatedAt`, bypassing the
    /// overlay (v4 `setOfficialMountPointId` → `_update({officialMountPointId})`,
    /// which sets `updatedAt = now`). Used by provisioning before the store files
    /// exist.
    pub fn set_official_mount_point_id(
        &self,
        id: &str,
        mount_point_id: &str,
    ) -> Result<(), DbError> {
        self.main.execute(
            "UPDATE groups SET officialMountPointId = ?1, updatedAt = ?2 WHERE id = ?3",
            params![mount_point_id, crate::clock::now_iso(), id],
        )?;
        Ok(())
    }

    /// Insert the slim row with a NULL FK (v4 store-aware `_create`: validate the
    /// full entity in memory, write only the slim columns). Mints id + timestamps
    /// unless pinned. Returns the created `(id, name)`.
    fn create_slim(
        &self,
        name: &str,
        opts: &GroupCreateOptions,
    ) -> Result<(String, String), DbError> {
        let now = crate::clock::now_iso();
        let id = opts
            .id
            .clone()
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let created_at = opts.created_at.clone().unwrap_or_else(|| now.clone());
        let updated_at = opts.updated_at.clone().unwrap_or(now);
        self.main.execute(
            "INSERT INTO groups (id, name, officialMountPointId, createdAt, updatedAt) \
             VALUES (?1, ?2, NULL, ?3, ?4)",
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
        // `name` is the lone DB-only column; ignore any other remainder key
        // (none exists for groups).
        if let Some(name) = db_patch.get("name").and_then(Value::as_str) {
            self.main.execute(
                "UPDATE groups SET name = ?1, updatedAt = ?2 WHERE id = ?3",
                params![name, crate::clock::now_iso(), id],
            )?;
        } else {
            self.main.execute(
                "UPDATE groups SET updatedAt = ?1 WHERE id = ?2",
                params![crate::clock::now_iso(), id],
            )?;
        }
        Ok(true)
    }

    // ========================================================================
    // Public CRUD (document-store overlay applied)
    // ========================================================================

    /// Find by id, hydrated from the store; **throws** `Unavailable` if the store
    /// is missing/unreadable (v4 `findById` → `applyOverlayOne`).
    pub fn find_by_id(&self, id: &str) -> Result<Option<Value>, OverlayError> {
        let raw = self.find_by_id_raw(id)?;
        overlay::apply_overlay_one::<GroupEntity>(self.mount, raw)
    }

    /// Find all, each hydrated; a row whose store is unavailable is **dropped**
    /// (v4 `findAll` → `applyOverlay`).
    pub fn find_all(&self) -> Result<Vec<Value>, OverlayError> {
        let raw = self.find_all_raw()?;
        overlay::apply_overlay::<GroupEntity>(self.mount, raw)
    }

    /// Create a group, provision its official store, populate the four files, and
    /// return the overlaid entity (v4 `create`). The 5-step sequence; fails hard
    /// if the store can't be provisioned.
    pub fn create(
        &self,
        input: &GroupCreateInput,
        opts: &GroupCreateOptions,
    ) -> Result<Value, OverlayError> {
        // 1. Slim row (FK null).
        let (id, name) = self.create_slim(&input.name, opts)?;

        // 2. Provision the official store (sets the FK raw).
        let ensured =
            ensure_group_official_store(self.main, self.mount, &id, &name)?.ok_or_else(|| {
                OverlayError::Db(DbError::Key(format!(
                    "group {id} disappeared during store provisioning"
                )))
            })?;

        // 3-4. Write the four overlay files from the create payload.
        let properties = serde_json::to_value(GroupProperties {
            color: input.color.clone(),
            icon: input.icon.clone(),
        })
        .map_err(|e| OverlayError::Db(DbError::Key(format!("properties build: {e}"))))?;
        overlay::write_managed_fields::<GroupEntity>(
            self.mount,
            &ensured.mount_point_id,
            &ManagedFields {
                properties,
                description: input.description.clone(),
                instructions: input.instructions.clone(),
                state: input.state.clone(),
            },
        )?;

        // 5. Overlay re-read (reflects the freshly-set mount + store state).
        self.find_by_id(&id)?.ok_or_else(|| {
            OverlayError::Db(DbError::Key(format!(
                "group {id} disappeared immediately after creation"
            )))
        })
    }

    /// Update a group: store-resident fields routed to the store, the DB-only
    /// remainder written through the slim `_update`; the result is overlaid (v4
    /// `update`). `patch` is the partial entity as a JSON map.
    pub fn update(
        &self,
        id: &str,
        patch: &Map<String, Value>,
    ) -> Result<Option<Value>, OverlayError> {
        let raw = self.find_by_id_raw(id)?;
        let db_patch =
            overlay::apply_write_overlay::<GroupEntity>(self.mount, raw.as_ref(), patch)?;
        let has_db_work = !db_patch.is_empty();
        let result = if has_db_work {
            self.update_slim(id, &db_patch)?;
            self.find_by_id_raw(id)?
        } else {
            self.find_by_id_raw(id)?
        };
        overlay::apply_overlay_one::<GroupEntity>(self.mount, result)
    }

    /// Delete the slim row (the official store is orphaned, per v4 `delete`).
    pub fn delete(&self, id: &str) -> Result<bool, DbError> {
        let affected = self
            .main
            .execute("DELETE FROM groups WHERE id = ?1", params![id])?;
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
