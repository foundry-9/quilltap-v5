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

use rusqlite::types::ToSql;
use rusqlite::{params, Connection};
use serde_json::{Map, Value};

use super::document_store_overlay::{self as overlay, ManagedFields, OverlayError, StoreEntity};
use super::ensure_official_store::ensure_official_store;
use super::DbError;
use crate::chunk::{chunk_array, SQLITE_VARIABLE_CHUNK_SIZE};

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

    /// The slim rows for a batch of ids (the raw half of v4's `findByIds`,
    /// `store-backed.repository.ts:101` — its `findByFilter({ id: { $in: ids } })`).
    /// Empty input → `[]` without a statement; an id with no row is simply absent.
    /// Order is per-chunk SQL-natural and unobservable (the caller overlays then
    /// maps).
    ///
    /// ⚠ v4 does **not** chunk this read (only its two memories-delete sites feed
    /// `lib/utils/chunk.ts`). The chunking is the P4.65 scale-safety measure taken
    /// after P4.D126 measured a 40,000-id batch failing with "too many SQL
    /// variables"; it is invisible in the output.
    pub fn find_by_ids_raw(&self, ids: &[String]) -> Result<Vec<Map<String, Value>>, DbError> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let mut out = Vec::new();
        for chunk in chunk_array(ids, SQLITE_VARIABLE_CHUNK_SIZE) {
            let placeholders = (0..chunk.len()).map(|_| "?").collect::<Vec<_>>().join(", ");
            let sql = format!(
                "SELECT id, name, officialMountPointId, createdAt, updatedAt \
                 FROM {} WHERE id IN ({placeholders})",
                E::slim_table()
            );
            let params: Vec<&dyn ToSql> = chunk.iter().map(|s| s as &dyn ToSql).collect();
            let mut stmt = self.main.prepare(&sql)?;
            let rows = stmt
                .query_map(params.as_slice(), |row| Ok(slim_row_to_map(row)))?
                .collect::<Result<Vec<_>, _>>()?;
            out.extend(rows);
        }
        Ok(out)
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
        self.insert_slim(name, opts, None)
    }

    /// The slim INSERT itself, its `officialMountPointId` given (`None` →
    /// NULL — every create but the restore's bound one).
    fn insert_slim(
        &self,
        name: &str,
        opts: &StoreCreateOptions,
        official_mount_point_id: Option<&str>,
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
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                E::slim_table()
            ),
            params![id, name, official_mount_point_id, created_at, updated_at],
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

    /// Find a batch by id, each hydrated; a row whose store is unavailable is
    /// **dropped** — the same `applyOverlay` semantics as [`Self::find_all`], not
    /// the throwing `applyOverlayOne` [`Self::find_by_id`] uses (v4 `findByIds`,
    /// `store-backed.repository.ts:101`). So a caller cannot distinguish "no such
    /// id" from "its store is gone"; the chat-list preload wants exactly that —
    /// a broken store must not fail the whole listing.
    pub fn find_by_ids(&self, ids: &[String]) -> Result<Vec<Value>, OverlayError> {
        let raw = self.find_by_ids_raw(ids)?;
        overlay::apply_overlay::<E>(self.mount, raw)
    }

    /// Create the entity, provision its official store, populate the four overlay
    /// files from `fields`, and return the overlaid entity (v4 `create` — the
    /// 5-step sequence). Fails hard if the store can't be provisioned.
    ///
    /// The property bag is validated BEFORE the slim INSERT (P4.158 R-E): v4's
    /// `_create` runs `validate` on the WHOLE entity and only then
    /// `insertOne` (`base.repository.ts:368-370`), so a bag its schema refuses
    /// leaves nothing — no slim row, no provisioned store. v5 parsed the bag
    /// only when writing `properties.json`, after both; the same parse, with
    /// the same error, now runs first (and again at the write, which can no
    /// longer refuse).
    pub fn create(
        &self,
        name: &str,
        fields: &ManagedFields,
        opts: &StoreCreateOptions,
    ) -> Result<Value, OverlayError> {
        E::parse_properties(&fields.properties)
            .map_err(|d| OverlayError::Db(DbError::Internal(d)))?;
        let (id, name) = self.create_slim(name, opts)?;
        self.provision_official_store(&id, &name, fields)?;
        self.find_by_id(&id)?.ok_or_else(|| {
            OverlayError::Db(DbError::Internal(format!(
                "{} {id} disappeared immediately after creation",
                E::entity_label()
            )))
        })
    }

    /// Provision the entity's official store and write its store-resident
    /// fields from `fields` (the in-memory payload, not a re-read) — v4
    /// `provisionOfficialStore` (`store-backed.repository.ts:175-189`,
    /// `ed8b15b50`), factored out of `create`. `ensure_official_store` sets
    /// `officialMountPointId` on the row; a pointer naming a missing store is
    /// healed rather than trusted. Fails hard rather than leave the entity
    /// storeless, which would throw on every read. Returns the store's id.
    ///
    /// Two callers: [`Self::create`], and the restore's 22a-i, which gives a
    /// bound entity whose archived store row failed to restore a fresh store
    /// populated from its backup row.
    pub fn provision_official_store(
        &self,
        id: &str,
        name: &str,
        fields: &ManagedFields,
    ) -> Result<String, OverlayError> {
        let lower = E::entity_label();
        let ensured =
            ensure_official_store::<E>(self.main, self.mount, id, name)?.ok_or_else(|| {
                OverlayError::Db(DbError::Internal(format!(
                    "Failed to provision official document store for {lower} {id}; \
                     refusing to return a storeless {lower}."
                )))
            })?;
        overlay::write_managed_fields::<E>(self.mount, &ensured.mount_point_id, fields)?;
        Ok(ensured.mount_point_id)
    }

    /// Restore an entity row bound to the official store a backup carries for
    /// it — v4 `createBoundToStore` (`store-backed.repository.ts:191-229`,
    /// `ed8b15b50`, bug 185).
    ///
    /// The one sanctioned exception to [`Self::create`]'s fresh-store rule, and
    /// only for the restore: the archive's own store for this entity is being
    /// restored alongside the row, so minting a fresh one would leave the row
    /// pointing at an empty store and the archived one orphaned (bug 185). The
    /// restore's claim map guarantees no two rows are given the same store.
    ///
    /// Writes the slim row with `officialMountPointId` set — ONE insert
    /// carrying the FK — and does nothing else: no provisioning, no link, no
    /// store writes (the archive carries the store's files and its link rows).
    /// The store's rows usually arrive later in the restore than the
    /// entity's, so the row is never read back through the overlay, which
    /// would throw until they do.
    ///
    /// `entity` is the create payload (the archive row minus `id` /
    /// `createdAt` / `updatedAt`), validated WHOLE first as v4's `_create`
    /// does — the project's create seed applied, the group's none
    /// ([`crate::db::projects::parse_create_entity`] /
    /// [`crate::db::groups::parse_create_entity`]). A refusal logs v4's
    /// `Data validation failed` and `Error creating <kind> entity`, then the
    /// bound wrap `Error restoring <kind> bound to its store {collection,
    /// name, mountPointId, error}`; a failed INSERT logs the last two. On
    /// success, INFO `<Label> restored bound to its archived store
    /// {projectId|groupId, name, officialMountPointId}`. Returns the row's id.
    pub fn create_bound_to_store(
        &self,
        entity: &Value,
        claimed_id: Option<&str>,
        mount_point_id: &str,
        opts: &StoreCreateOptions,
    ) -> Result<String, OverlayError> {
        let kind = match E::entity_label() {
            "project" => overlay::StoreKind::Project,
            _ => overlay::StoreKind::Group,
        };
        let validated = match kind {
            overlay::StoreKind::Project => {
                crate::db::projects::parse_create_entity(entity, claimed_id).map(|_| ())
            }
            overlay::StoreKind::Group => {
                crate::db::groups::parse_create_entity(entity, claimed_id).map(|_| ())
            }
        };
        let created = validated
            .inspect_err(|zod| {
                tracing::error!(
                    target: "quilltap::db",
                    collection = E::slim_table(),
                    error = %zod,
                    "Data validation failed"
                );
            })
            .and_then(|()| {
                let name = entity.get("name").and_then(Value::as_str).unwrap_or("");
                self.insert_slim(name, opts, Some(mount_point_id))
                    .map_err(|e| crate::db::fallback::error_text(&e))
            })
            .inspect_err(|error| {
                crate::db::fallback::log_store_entity_create_failure(kind, error);
            });
        match created {
            Ok((id, name)) => {
                bound_create_info(kind, &id, &name, mount_point_id);
                Ok(id)
            }
            Err(error) => {
                bound_create_wrap(kind, entity.get("name"), mount_point_id, &error);
                Err(OverlayError::Db(DbError::Internal(error)))
            }
        }
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

/// [`StoreBackedRepository::create_bound_to_store`]'s success INFO (v4
/// `store-backed.repository.ts:219-223`).
fn bound_create_info(kind: overlay::StoreKind, id: &str, name: &str, mount_point_id: &str) {
    match kind {
        overlay::StoreKind::Project => tracing::info!(
            target: "quilltap::db",
            projectId = %id,
            name = %name,
            officialMountPointId = %mount_point_id,
            "Project restored bound to its archived store"
        ),
        overlay::StoreKind::Group => tracing::info!(
            target: "quilltap::db",
            groupId = %id,
            name = %name,
            officialMountPointId = %mount_point_id,
            "Group restored bound to its archived store"
        ),
    }
}

/// [`StoreBackedRepository::create_bound_to_store`]'s `safeQuery` wrap (v4
/// `store-backed.repository.ts:226-227`): `{collection, name, mountPointId,
/// error}` — `name` the payload's, omitted when absent (winston drops
/// `undefined`), a non-string rendered as its JSON text.
fn bound_create_wrap(
    kind: overlay::StoreKind,
    name: Option<&Value>,
    mount_point_id: &str,
    error: &str,
) {
    let name = name.map(|v| match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    });
    let name = name.as_deref().map(tracing::field::display);
    match kind {
        overlay::StoreKind::Project => tracing::error!(
            target: "quilltap::db",
            collection = "projects",
            name = name,
            mountPointId = %mount_point_id,
            error = %error,
            "Error restoring project bound to its store"
        ),
        overlay::StoreKind::Group => tracing::error!(
            target: "quilltap::db",
            collection = "groups",
            name = name,
            mountPointId = %mount_point_id,
            error = %error,
            "Error restoring group bound to its store"
        ),
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

#[cfg(test)]
mod find_by_ids_tests {
    use super::*;
    use crate::db::projects::ProjectEntity;

    /// The MAIN-db slim table for the entity under test (`projects`) — only the
    /// five slim columns the raw read names.
    fn main_db(rows: &[(&str, &str, Option<&str>)]) -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE projects (id TEXT PRIMARY KEY NOT NULL, name TEXT NOT NULL, \
             officialMountPointId TEXT, createdAt TEXT NOT NULL, updatedAt TEXT NOT NULL);",
        )
        .unwrap();
        for (id, name, mount_point_id) in rows {
            conn.execute(
                "INSERT INTO projects (id, name, officialMountPointId, createdAt, updatedAt) \
                 VALUES (?1, ?2, ?3, '2020-01-01T00:00:00.000Z', '2020-01-01T00:00:00.000Z')",
                params![id, name, mount_point_id],
            )
            .unwrap();
        }
        conn
    }

    /// The MOUNT-INDEX side the overlay reads through: the three-table join
    /// behind `findManyByMountPointsAndPath`, seeded with one `properties.json`
    /// per named mount point.
    fn mount_db(stores: &[&str]) -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE doc_mount_files (id TEXT PRIMARY KEY NOT NULL);
             CREATE TABLE doc_mount_documents (id TEXT PRIMARY KEY NOT NULL, \
                fileId TEXT NOT NULL, content TEXT);
             CREATE TABLE doc_mount_file_links (id TEXT PRIMARY KEY NOT NULL, \
                fileId TEXT NOT NULL, mountPointId TEXT NOT NULL, relativePath TEXT NOT NULL);",
        )
        .unwrap();
        for mp in stores {
            conn.execute(
                "INSERT INTO doc_mount_files (id) VALUES (?1 || '-f')",
                params![mp],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO doc_mount_documents (id, fileId, content) \
                 VALUES (?1 || '-d', ?1 || '-f', '{}')",
                params![mp],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO doc_mount_file_links (id, fileId, mountPointId, relativePath) \
                 VALUES (?1 || '-l', ?1 || '-f', ?1, 'properties.json')",
                params![mp],
            )
            .unwrap();
        }
        conn
    }

    fn ids(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_string()).collect()
    }

    fn sorted_ids(rows: &[Value]) -> Vec<String> {
        let mut out: Vec<String> = rows
            .iter()
            .map(|r| r["id"].as_str().unwrap().to_string())
            .collect();
        out.sort();
        out
    }

    fn sorted_raw_ids(rows: &[Map<String, Value>]) -> Vec<String> {
        let mut out: Vec<String> = rows
            .iter()
            .map(|r| r["id"].as_str().unwrap().to_string())
            .collect();
        out.sort();
        out
    }

    // ── the raw half ────────────────────────────────────────────────────────

    /// The present ids come back as slim maps; an absent id is simply missing.
    #[test]
    fn raw_returns_only_the_ids_that_exist() {
        let main = main_db(&[
            ("p1", "One", Some("mp-1")),
            ("p2", "Two", Some("mp-2")),
            ("p3", "Three", None),
        ]);
        let mount = mount_db(&["mp-1", "mp-2"]);
        let repo = StoreBackedRepository::<ProjectEntity>::new(&main, &mount);

        let rows = repo.find_by_ids_raw(&ids(&["p1", "ghost", "p3"])).unwrap();
        assert_eq!(
            sorted_raw_ids(&rows),
            vec!["p1".to_string(), "p3".to_string()]
        );
        // The same five-column shape `find_by_id_raw` builds, NULL FK included.
        let p3 = rows.iter().find(|r| r["id"] == "p3").unwrap();
        assert_eq!(p3["name"], Value::from("Three"));
        assert_eq!(p3["officialMountPointId"], Value::Null);
        assert_eq!(p3.len(), 5);
    }

    #[test]
    fn raw_empty_input_answers_empty() {
        let main = main_db(&[("p1", "One", Some("mp-1"))]);
        let mount = mount_db(&["mp-1"]);
        assert!(StoreBackedRepository::<ProjectEntity>::new(&main, &mount)
            .find_by_ids_raw(&[])
            .unwrap()
            .is_empty());
    }

    /// The P4.65 chunking, in the P4.D126 idiom: 40,000 ids is past the engine's
    /// 32,766 ceiling, so an un-chunked `IN (…)` fails with "too many SQL
    /// variables". The real ids sit in three different chunk regions, so a port
    /// that only ran the first chunk would miss two of them. One proof at the
    /// raw level covers the hydrated twin too — the overlay above it is
    /// `find_all`'s, already proven, and never re-queries the slim table.
    #[test]
    fn raw_chunks_past_the_variable_limit() {
        let main = main_db(&[
            ("p1", "One", Some("mp-1")),
            ("p2", "Two", Some("mp-1")),
            ("p3", "Three", Some("mp-1")),
        ]);
        let mount = mount_db(&["mp-1"]);
        let repo = StoreBackedRepository::<ProjectEntity>::new(&main, &mount);

        let mut batch: Vec<String> = (0..40_000).map(|i| format!("ghost-{i}")).collect();
        batch[0] = "p1".to_string();
        batch[SQLITE_VARIABLE_CHUNK_SIZE] = "p2".to_string();
        batch[39_999] = "p3".to_string();

        let rows = repo
            .find_by_ids_raw(&batch)
            .expect("a chunked find_by_ids_raw must not hit the variable limit");
        assert_eq!(
            sorted_raw_ids(&rows),
            vec!["p1".to_string(), "p2".to_string(), "p3".to_string()],
            "every chunk's rows are concatenated into one result"
        );
    }

    // ── the hydrated half ───────────────────────────────────────────────────

    /// Each returned row is overlaid with its store, and a row whose store is
    /// unavailable is **dropped** rather than raised — `applyOverlay`, not the
    /// throwing `applyOverlayOne`. Here `p3` has a NULL `officialMountPointId`
    /// and `p4` points at a store with no `properties.json`; both vanish.
    #[test]
    fn hydrates_each_row_and_drops_an_unavailable_store() {
        let main = main_db(&[
            ("p1", "One", Some("mp-1")),
            ("p3", "Three", None),
            ("p4", "Four", Some("mp-missing")),
        ]);
        let mount = mount_db(&["mp-1"]);
        let repo = StoreBackedRepository::<ProjectEntity>::new(&main, &mount);

        let rows = repo
            .find_by_ids(&ids(&["p1", "p3", "p4", "ghost"]))
            .unwrap();
        assert_eq!(sorted_ids(&rows), vec!["p1".to_string()]);
        // …and the survivor carries the overlay's materialized property bag.
        assert_eq!(rows[0]["name"], Value::from("One"));
        assert_eq!(rows[0]["allowAnyCharacter"], Value::from(false));
        assert_eq!(rows[0]["characterRoster"], Value::Array(Vec::new()));
        assert_eq!(rows[0]["backgroundDisplayMode"], Value::from("theme"));
        assert_eq!(rows[0]["description"], Value::Null);
    }

    #[test]
    fn hydrated_empty_input_answers_empty() {
        let main = main_db(&[("p1", "One", Some("mp-1"))]);
        let mount = mount_db(&["mp-1"]);
        assert!(StoreBackedRepository::<ProjectEntity>::new(&main, &mount)
            .find_by_ids(&[])
            .unwrap()
            .is_empty());
    }
}

#[cfg(test)]
mod create_order_tests {
    //! P4.158 R-E + P4.D273 — the create paths leave NOTHING behind when they
    //! fail, over a real provisioned instance (a temp COPY, never live data).
    use super::*;
    use crate::db::projects::ProjectEntity;

    const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";

    fn instance() -> (tempfile::TempDir, crate::db::Writer, crate::db::Writer) {
        let dir = tempfile::tempdir().unwrap();
        crate::services::provisioning::provision_fresh_instance(dir.path(), PEPPER).unwrap();
        let main =
            crate::db::Writer::open_writable(&dir.path().join("quilltap.db"), PEPPER).unwrap();
        let mount =
            crate::db::Writer::open_writable(&dir.path().join("quilltap-mount-index.db"), PEPPER)
                .unwrap();
        (dir, main, mount)
    }

    fn count(conn: &Connection, table: &str) -> i64 {
        conn.query_row(&format!("SELECT COUNT(*) FROM \"{table}\""), [], |r| {
            r.get(0)
        })
        .unwrap()
    }

    fn pinned(id: &str) -> StoreCreateOptions {
        StoreCreateOptions {
            id: Some(id.to_string()),
            created_at: None,
            updated_at: None,
        }
    }

    fn lines_with(lines: &[String], needle: &str) -> Vec<String> {
        lines
            .iter()
            .filter(|l| l.contains(needle))
            .cloned()
            .collect()
    }

    /// P4.D273 — v4 `createBoundToStore`: ONE slim INSERT carrying the FK,
    /// nothing provisioned or linked, v4's INFO — and none of the refusal lines.
    #[test]
    fn create_bound_to_store_writes_one_bound_row_and_its_info() {
        let (_dir, main, mount) = instance();
        let (points, links) = (
            count(mount.connection(), "doc_mount_points"),
            count(mount.connection(), "doc_mount_file_links"),
        );
        let repo =
            StoreBackedRepository::<ProjectEntity>::new(main.connection(), mount.connection());
        let (id, lines) = crate::test_support::captured_with(|| {
            repo.create_bound_to_store(
                &serde_json::json!({ "name": "The Voyage", "color": "#334455" }),
                Some("a3000000-0000-4000-8000-000000000001"),
                "mp-archived",
                &pinned("a3000000-0000-4000-8000-000000000001"),
            )
            .unwrap()
        });
        assert_eq!(id, "a3000000-0000-4000-8000-000000000001");
        let fk: Option<String> = main
            .connection()
            .query_row(
                "SELECT officialMountPointId FROM projects WHERE id = ?1",
                params![id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(fk.as_deref(), Some("mp-archived"));
        assert_eq!(
            count(mount.connection(), "doc_mount_points"),
            points,
            "no store"
        );
        assert_eq!(
            count(mount.connection(), "doc_mount_file_links"),
            links,
            "no link"
        );
        assert_eq!(
            lines_with(&lines, "restored bound to its archived store"),
            vec![
                "INFO quilltap::db Project restored bound to its archived store \
                 projectId=a3000000-0000-4000-8000-000000000001 name=The Voyage \
                 officialMountPointId=mp-archived"
                    .to_string()
            ]
        );
        assert!(
            !lines.iter().any(|l| l.starts_with("ERROR")),
            "the silence leg: {lines:?}"
        );
    }

    /// A refused entity (v4's `_create` validates first): no row, and v4's
    /// three lines in order — `Data validation failed`, `Error creating group
    /// entity`, the bound wrap — with no INFO.
    #[test]
    fn create_bound_to_store_refuses_with_three_lines_and_no_row() {
        let (_dir, main, mount) = instance();
        let repo = StoreBackedRepository::<crate::db::groups::GroupEntity>::new(
            main.connection(),
            mount.connection(),
        );
        let long = "x".repeat(101);
        let (refused, lines) = crate::test_support::captured_with(|| {
            repo.create_bound_to_store(
                &serde_json::json!({ "name": long }),
                Some("a2000000-0000-4000-8000-000000000001"),
                "mp-archived",
                &pinned("a2000000-0000-4000-8000-000000000001"),
            )
        });
        assert!(refused.is_err());
        assert_eq!(count(main.connection(), "groups"), 0, "no slim row");
        let messages: Vec<&str> = lines
            .iter()
            .filter(|l| l.starts_with("ERROR") || l.starts_with("INFO"))
            .map(|l| {
                [
                    "Data validation failed",
                    "Error creating group entity",
                    "Error restoring group bound to its store",
                    "Group restored bound to its archived store",
                ]
                .into_iter()
                .find(|m| l.contains(m))
                .unwrap_or(l.as_str())
            })
            .collect();
        assert_eq!(
            messages,
            vec![
                "Data validation failed",
                "Error creating group entity",
                "Error restoring group bound to its store"
            ]
        );
        let wrap = lines_with(&lines, "Error restoring group bound to its store");
        assert!(
            wrap[0].contains(&format!(
                "collection=groups name={long} mountPointId=mp-archived error="
            )),
            "{wrap:?}"
        );
    }

    /// A failed INSERT (the id already taken) skips validation's line and logs
    /// the rethrow + the wrap, with SQLite's bare message.
    #[test]
    fn create_bound_to_store_insert_failure_logs_the_bare_message() {
        let (_dir, main, mount) = instance();
        let repo =
            StoreBackedRepository::<ProjectEntity>::new(main.connection(), mount.connection());
        const DUP: &str = "a3000000-0000-4000-8000-0000000000dd";
        let entity = serde_json::json!({ "name": "Twice" });
        repo.create_bound_to_store(&entity, Some(DUP), "mp-1", &pinned(DUP))
            .unwrap();
        let (refused, lines) = crate::test_support::captured_with(|| {
            repo.create_bound_to_store(&entity, Some(DUP), "mp-2", &pinned(DUP))
        });
        assert!(refused.is_err());
        assert!(lines_with(&lines, "Data validation failed").is_empty());
        assert_eq!(
            lines_with(&lines, "Error restoring project bound to its store"),
            vec![
                "ERROR quilltap::db Error restoring project bound to its store collection=projects \
                 name=Twice mountPointId=mp-2 error=UNIQUE constraint failed: projects.id"
                    .to_string()
            ]
        );
        assert_eq!(lines_with(&lines, "Error creating project entity").len(), 1);
    }

    /// v4's refusing sentence when the row to provision is gone
    /// (`provisionOfficialStore`, `store-backed.repository.ts:178-182`).
    #[test]
    fn provision_official_store_refuses_a_missing_row_with_v4s_sentence() {
        let (_dir, main, mount) = instance();
        let repo =
            StoreBackedRepository::<ProjectEntity>::new(main.connection(), mount.connection());
        let err = repo
            .provision_official_store(
                "p-gone",
                "Gone",
                &ManagedFields {
                    properties: serde_json::json!({}),
                    description: None,
                    instructions: None,
                    state: serde_json::json!({}),
                },
            )
            .unwrap_err();
        assert_eq!(
            err.to_string(),
            "Failed to provision official document store for project p-gone; \
             refusing to return a storeless project."
        );
    }

    /// R-E: v4's `_create` validates the WHOLE entity before its INSERT
    /// (`base.repository.ts:368-370`), so a bag v4's schema refuses leaves
    /// nothing — no slim row, no provisioned store, no store file.
    #[test]
    fn create_validates_the_bag_before_any_write() {
        let (_dir, main, mount) = instance();
        let (points, links) = (
            count(mount.connection(), "doc_mount_points"),
            count(mount.connection(), "doc_mount_file_links"),
        );
        let repo =
            StoreBackedRepository::<ProjectEntity>::new(main.connection(), mount.connection());
        let refused = repo.create(
            "The Voyage",
            &ManagedFields {
                properties: serde_json::json!({ "allowAnyCharacter": "yes" }),
                description: None,
                instructions: None,
                state: serde_json::json!({}),
            },
            &pinned("p-refused"),
        );
        assert!(refused.is_err(), "the bag must be refused: {refused:?}");
        assert_eq!(count(main.connection(), "projects"), 0, "no slim row");
        assert_eq!(
            count(mount.connection(), "doc_mount_points"),
            points,
            "no store"
        );
        assert_eq!(
            count(mount.connection(), "doc_mount_file_links"),
            links,
            "no store file"
        );
    }
}
