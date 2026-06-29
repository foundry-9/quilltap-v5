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
use super::store_backed::StoreBackedRepository;
use super::DbError;

pub use super::store_backed::StoreCreateOptions as GroupCreateOptions;

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

    fn parse_properties(value: &Value) -> Result<GroupProperties, String> {
        if !value.is_object() {
            return Err(format!("expected a JSON object, got: {value}"));
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
    pub color: Option<String>,
    pub icon: Option<String>,
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

    /// Create a group, provision its store, and return the overlaid entity.
    pub fn create(
        &self,
        input: &GroupCreateInput,
        opts: &GroupCreateOptions,
    ) -> Result<Value, OverlayError> {
        let properties = serde_json::to_value(GroupProperties {
            color: input.color.clone(),
            icon: input.icon.clone(),
        })
        .map_err(|e| OverlayError::Db(DbError::Key(format!("properties build: {e}"))))?;
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
