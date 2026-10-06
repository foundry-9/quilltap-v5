//! The `groups` repository — the first **store-backed pilot** of the
//! document-store overlay slice. A thin wrapper over the generic
//! [`super::store_backed::StoreBackedRepository`] bound to [`GroupEntity`]
//! (v4's `GroupsRepository`, which adds nothing to the shared base beyond its
//! overlay binding — groups has no roster ops). The engine, the slim-row
//! plumbing, and provisioning all live in the generic base; this module supplies
//! only the typed `properties.json` bag + the entity wiring.
//!
//! A group's substantive content does NOT live in `groups` columns. The slim row
//! (id/name/officialMountPointId/timestamps) lives in the MAIN db; the store
//! (`color`/`icon` in `properties.json`, `description`/`instructions`/`state`)
//! lives in the MOUNT-INDEX db. Groups is the smallest store-backed surface (a
//! 2-key bag, no roster), so it proved the whole engine with the least incidental
//! marshaling; `projects` reuses the same base with a 16-key bag + roster ops.

use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::document_store_overlay::{ManagedFields, OverlayError, StoreEntity};
use super::group_doc_mount_links::GroupDocMountLinksRepository;
use super::serde_tristate::double_option;
use super::store_backed::StoreBackedRepository;
use super::DbError;

pub use super::store_backed::StoreCreateOptions as GroupCreateOptions;

/// The `properties.json` bag (v4 `GroupPropertiesSchema`). Both keys are
/// `.nullable().optional()` and THREE-state here (`Option<Option<String>>` over
/// [`double_option`]): `None` = absent and omitted on write (`skip_serializing_if`),
/// `Some(None)` = an explicit `null`, written back as `null`, `Some(Some(v))` = a
/// value. v4's Zod parse keeps an explicit `null` — its create route stores
/// `color: … || null`, and its group editor saves `color: this.color() || null` —
/// so a v5 read-modify-write must not fold it into absent (dogfood #136).
/// Serialized in schema-declaration order, matching `JSON.stringify(parse(x),
/// null, 2)` byte-for-byte (the dedup sha depends on it).
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct GroupProperties {
    #[serde(
        default,
        deserialize_with = "double_option",
        skip_serializing_if = "Option::is_none"
    )]
    pub color: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "double_option",
        skip_serializing_if = "Option::is_none"
    )]
    pub icon: Option<Option<String>>,
}

/// The group's [`StoreEntity`] binding for the generic engine + base repository.
pub struct GroupEntity;

impl StoreEntity for GroupEntity {
    type Properties = GroupProperties;

    fn entity_label() -> &'static str {
        "group"
    }

    fn property_keys() -> &'static [&'static str] {
        &["color", "icon"]
    }

    /// v4 `GroupPropertiesSchema.parse` — the ONE chokepoint every group bag
    /// passes through (overlay read, write overlay, create, import, restore).
    /// [P4.148] v4's rules run FIRST and a refusal is the `ZodError.message`
    /// bytes (`JSON.stringify(issues, null, 2)`) — the hex colour, the 50
    /// code-point icon, a wrong type — where v5 used to accept `"red"` and
    /// answer a wrong type with serde's sentence. The typed decode after it
    /// cannot fail on a bag the rules passed.
    fn parse_properties(value: &Value) -> Result<GroupProperties, String> {
        let issues = crate::api::zod_issues::zod_group_properties_issues(value);
        if !issues.is_empty() {
            return Err(crate::api::zod_issues::zod_error_message(&issues));
        }
        serde_json::from_value(value.clone()).map_err(|e| e.to_string())
    }

    fn slim_table() -> &'static str {
        "groups"
    }

    fn store_name_prefix() -> &'static str {
        "Group Files: "
    }

    fn find_store_links(mount: &Connection, entity_id: &str) -> Result<Vec<String>, DbError> {
        GroupDocMountLinksRepository::new(mount).find_by_group_id(entity_id)
    }

    fn link_store(
        mount: &Connection,
        entity_id: &str,
        mount_point_id: &str,
    ) -> Result<(), DbError> {
        GroupDocMountLinksRepository::new(mount).link(entity_id, mount_point_id)
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
}

/// The groups repository — a thin wrapper over the generic store-backed base.
pub struct GroupsRepository<'c> {
    inner: StoreBackedRepository<'c, GroupEntity>,
}

impl<'c> GroupsRepository<'c> {
    pub fn new(main: &'c Connection, mount: &'c Connection) -> Self {
        Self {
            inner: StoreBackedRepository::new(main, mount),
        }
    }

    /// Create a group, provision its store, and return the overlaid entity. The
    /// `properties.json` bag is given WHOLE, so an explicit `null` survives (v4
    /// `writeManagedFields(parseProperties(entity))`) and an absent key stays
    /// absent. The ONE create since the `07b8f0209` follow-ups unification:
    /// the value-or-absent `create` and `GroupCreateInput`'s `color`/`icon`
    /// fields were deleted once the restore (P4.147) joined the group route and
    /// the `.qtap` import on this fn (P4.148 2(c), §S.7).
    pub fn create_with_properties(
        &self,
        input: &GroupCreateInput,
        properties: &GroupProperties,
        opts: &GroupCreateOptions,
    ) -> Result<Value, OverlayError> {
        let properties = serde_json::to_value(properties)
            .map_err(|e| OverlayError::Db(DbError::Internal(format!("properties build: {e}"))))?;
        self.inner.create(
            &input.name,
            &ManagedFields {
                properties,
                description: input.description.clone(),
                instructions: input.instructions.clone(),
                state: input.state.clone(),
            },
            opts,
        )
    }

    /// Update a group (store-resident fields routed to the store; the DB-only
    /// remainder written to the slim row). `patch` is the partial entity as a map.
    pub fn update(
        &self,
        id: &str,
        patch: &Map<String, Value>,
    ) -> Result<Option<Value>, OverlayError> {
        self.inner.update(id, patch)
    }

    /// Find by id, hydrated (throws `Unavailable` if the store is missing).
    pub fn find_by_id(&self, id: &str) -> Result<Option<Value>, OverlayError> {
        self.inner.find_by_id(id)
    }

    /// Find all, each hydrated (drops a row whose store is unavailable).
    pub fn find_all(&self) -> Result<Vec<Value>, OverlayError> {
        self.inner.find_all()
    }

    /// Delete the slim row (the official store is orphaned).
    pub fn delete(&self, id: &str) -> Result<bool, DbError> {
        self.inner.delete(id)
    }
}

/// Read a group's `name` + `officialMountPointId` WITHOUT the store overlay (v4
/// `groups.findByIdRaw(...)`). Used by the Core-whisper packet assembly (W4.6a)
/// — it needs the group name (heading label) and the official mount pointer, but
/// must survive a broken store overlay. Returns `Ok(None)` when the group row is
/// absent. The `official_mount_point_id` is `None` when the group has no store yet.
pub fn find_name_and_official_mount_point_id_raw(
    main: &Connection,
    id: &str,
) -> Result<Option<(String, Option<String>)>, DbError> {
    let row = main
        .query_row(
            "SELECT name, officialMountPointId FROM groups WHERE id = ?1",
            rusqlite::params![id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
        )
        .map(Some)
        .or_else(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            other => Err(other),
        })?;
    Ok(row)
}

/// Read a group's `name` + `officialMountPointId` WITHOUT the store overlay, as
/// v4's `groups.findByIdRaw` sees the row: `_findById` VALIDATES it against
/// `GroupSchema` ([`crate::api::zod_issues::zod_group_issues`]), and a row
/// that fails is not found — v4's `validate` logs ERROR `Data validation
/// failed {collection, error}`, then its fallback `safeQuery` logs `Error
/// finding entity by ID` and answers `null` (P4.124, P4.D231). This fn logs the
/// first line and answers `Err`, so each caller's existing fallback arm logs
/// the second at its own target. `error` on both is v4's
/// `ZodError.message` — `JSON.stringify(issues, null, 2)` (P4.130; P4.124 had
/// logged a v5 sentence) — carried as the `Err`'s `Display`. A row whose cells merely DECODE (an empty or
/// 101-character name, a non-uuid id or pointer) is refused here where
/// [`find_name_and_official_mount_point_id_raw`] would return it. `Ok(None)`
/// when the row is absent.
pub fn find_validated_name_and_official_mount_point_id_raw(
    main: &Connection,
    id: &str,
) -> Result<Option<(String, Option<String>)>, DbError> {
    let row = main
        .query_row(
            "SELECT id, name, officialMountPointId, createdAt, updatedAt FROM groups WHERE id = ?1",
            rusqlite::params![id],
            |row| {
                let mut obj = serde_json::Map::new();
                for (i, key) in [
                    "id",
                    "name",
                    "officialMountPointId",
                    "createdAt",
                    "updatedAt",
                ]
                .into_iter()
                .enumerate()
                {
                    // v4 turns a NULL cell into `undefined` — the key is absent.
                    if let Some(v) = zod_row_cell(row.get_ref(i)?) {
                        obj.insert(key.to_string(), v);
                    }
                }
                Ok(obj)
            },
        )
        .map(Some)
        .or_else(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            other => Err(other),
        })?;
    let Some(row) = row else {
        return Ok(None);
    };
    let issues = crate::api::zod_issues::zod_group_issues(&row);
    if !issues.is_empty() {
        let error = crate::api::zod_issues::zod_error_message(&issues);
        tracing::error!(
            target: "quilltap::db",
            collection = "groups",
            error = %error,
            "Data validation failed"
        );
        return Err(DbError::Internal(error));
    }
    let text = |k: &str| {
        row.get(k)
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
    };
    Ok(Some((
        text("name").unwrap_or_default(),
        text("officialMountPointId"),
    )))
}

/// One SQLite cell as better-sqlite3 hands it to v4's Zod: `None` for NULL
/// (v4 reads a NULL cell as `undefined` — the key is absent), a number for an
/// INTEGER/REAL, a string for TEXT, and — for a BLOB — the `Float32Array`
/// v4's collection hydrates it to ([`crate::api::zod_issues::
/// zod_float32_array_cell`] over the header-aware element count — `received
/// Float32Array`, never a string). Shared by the two validated raw reads
/// (P4.130).
pub(crate) fn zod_row_cell(v: rusqlite::types::ValueRef<'_>) -> Option<serde_json::Value> {
    use rusqlite::types::ValueRef;
    match v {
        ValueRef::Null => None,
        ValueRef::Integer(i) => Some(serde_json::json!(i)),
        ValueRef::Real(f) => Some(serde_json::json!(f)),
        ValueRef::Text(t) => Some(serde_json::json!(String::from_utf8_lossy(t))),
        ValueRef::Blob(b) => Some(crate::api::zod_issues::zod_float32_array_cell(
            crate::embedding_blob::blob_to_float32(b).len(),
        )),
    }
}

/// Read a group's `officialMountPointId` pointer WITHOUT the store overlay (v4
/// `groups.findByIdRaw(...).officialMountPointId`). The tiered-mount-pool group
/// resolver uses this on its hot path — it only needs the pointer, not the
/// group's hydrated content. Returns `Ok(None)` when the group is absent, and
/// `Ok(Some(None))` when the group exists but has no official store yet.
pub fn find_official_mount_point_id_raw(
    main: &Connection,
    id: &str,
) -> Result<Option<Option<String>>, DbError> {
    let row = main
        .query_row(
            "SELECT officialMountPointId FROM groups WHERE id = ?1",
            rusqlite::params![id],
            |row| row.get::<_, Option<String>>(0),
        )
        .map(Some)
        .or_else(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            other => Err(other),
        })?;
    Ok(row)
}

/// P4.124: the validated raw read refuses a Zod-invalid row with v4's
/// `Data validation failed` and returns a valid one — silent.
#[cfg(test)]
mod validated_raw_read_tests {
    use super::*;

    #[test]
    fn a_zod_invalid_group_row_is_refused_with_v4s_line() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE groups (id TEXT, name, officialMountPointId TEXT, \
             createdAt TEXT, updatedAt TEXT); \
             INSERT INTO groups VALUES ('d2310000-0000-4000-8000-0000000000c1', 'Loners', \
               NULL, '2026-01-02T03:04:05.000Z', '2026-01-02T03:04:05.000Z'); \
             INSERT INTO groups VALUES ('d2310000-0000-4000-8000-0000000000c3', '', \
               NULL, '2026-01-02T03:04:05.000Z', '2026-01-02T03:04:05.000Z'); \
             INSERT INTO groups VALUES ('d2310000-0000-4000-8000-0000000000c2', X'00', \
               NULL, '2026-01-02T03:04:05.000Z', '2026-01-02T03:04:05.000Z');",
        )
        .unwrap();
        let (found, lines) = crate::test_support::captured_with(|| {
            find_validated_name_and_official_mount_point_id_raw(
                &conn,
                "d2310000-0000-4000-8000-0000000000c1",
            )
        });
        assert_eq!(found.unwrap(), Some(("Loners".to_string(), None)));
        assert!(lines.is_empty(), "{lines:?}");
        // P4.130: `error` is v4's `ZodError.message` — the rendered issue
        // list (the bytes themselves are `repository_zod_messages`'s, over
        // v4's real `GroupSchema`), and the `Err` carries the same string.
        for (bad, marker) in [
            (
                "d2310000-0000-4000-8000-0000000000c3",
                "Too small: expected string to have >=1 characters",
            ),
            (
                "d2310000-0000-4000-8000-0000000000c2",
                "Invalid input: expected string, received Float32Array",
            ),
        ] {
            let (found, lines) = crate::test_support::captured_with(|| {
                find_validated_name_and_official_mount_point_id_raw(&conn, bad)
            });
            let err = found.expect_err(bad).to_string();
            assert!(err.starts_with("[\n  {\n") && err.contains(marker), "{err}");
            assert_eq!(lines.len(), 1, "{lines:?}");
            assert!(
                lines[0].starts_with("ERROR quilltap::db Data validation failed")
                    && lines[0].contains("collection=groups")
                    && lines[0].contains(marker),
                "{}",
                lines[0]
            );
        }
        let (found, _) = crate::test_support::captured_with(|| {
            find_validated_name_and_official_mount_point_id_raw(&conn, "absent")
        });
        assert_eq!(found.unwrap(), None);
    }
}

/// P4.146 (dogfood #136): the group bag's three states through the parse +
/// 2-space serialize every write takes.
#[cfg(test)]
mod three_state_tests {
    use super::*;
    use serde_json::json;

    fn round_trip(bag: &Value) -> String {
        serde_json::to_string_pretty(&GroupEntity::parse_properties(bag).unwrap()).unwrap()
    }

    #[test]
    fn null_absent_value_and_mixed() {
        let both_null = "{\n  \"color\": null,\n  \"icon\": null\n}";
        assert_eq!(
            round_trip(&serde_json::from_str(both_null).unwrap()),
            both_null
        );
        assert_eq!(round_trip(&json!({})), "{}");
        assert_eq!(
            round_trip(&json!({ "icon": "gear", "color": "#abcdef" })),
            "{\n  \"color\": \"#abcdef\",\n  \"icon\": \"gear\"\n}"
        );
        assert_eq!(
            round_trip(&json!({ "icon": null })),
            "{\n  \"icon\": null\n}"
        );
    }
}
