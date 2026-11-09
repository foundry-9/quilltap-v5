//! The Inventory route factory — ONE implementation of the Inventory endpoints
//! for all four tiers (v4 `lib/inventory/routes/inventory-route-factory.ts` +
//! `responses.ts`, `4fbdad186` at the `322a5488e` pin; P4.D325):
//!
//!   - `/api/v1/characters/[id]/inventory` (+ `/[itemId]`) — a character's vault
//!   - `/api/v1/groups/[id]/inventory`     (+ `/[itemId]`) — a group's store
//!   - `/api/v1/projects/[id]/inventory`   (+ `/[itemId]`) — a project's store
//!   - `/api/v1/inventory`                 (+ `/[itemId]`) — Quilltap General
//!
//! Collection: `GET` lists the tier's objects resolved (`?includeArchived`;
//! the character tier's `?shared=true` lists the whole reachable pool, each
//! object tagged with its tier), `?action=folders`, `?action=archetypes` (the
//! archetypes reachable from this tier, for the instantiate picker; a
//! character takes `?projectId=` / `?chatId=` for the project tier). `POST`
//! creates; `?action=instantiate`, `duplicate`, `create-folder`,
//! `rename-folder`, `delete-folder`, `move-items`, `copy-items`.
//!
//! Item: `GET` one resolved object, `?action=dependents`; `PUT` an edit
//! (`archived` archives), `?action=clear-override`, `?action=detach`;
//! `DELETE` (`?force=true` bakes an archetype's instances first).
//!
//! Every refusal maps through [`inventory_error_response`] (its `code` nested
//! under `details`, a 409 or a 400); an archived character's vault answers
//! 409. The handlers are the keystone's modules by their C1 names — this file
//! holds only v4's ORDER of checks and the bodies:
//!
//!   - a collection POST parses BEFORE it resolves ("Parse before resolving:
//!     an invalid body must never provision a store") — an invalid body on a
//!     storeless project leaves `doc_mount_points` untouched;
//!   - an item route resolves and finds the object FIRST, then parses — a
//!     missing object 404s before a bad body 400s;
//!   - `duplicate` parses `{ itemId }` as `duplicateInventorySchema` before
//!     it resolves — a Zod 400.
//!
//! **Provisioning (v4's own `ensure` flags — they DIFFER from the
//! wardrobe's).** The three GET reads never provision: a project / group that
//! exists without a store, and an unprovisioned General, answer the empty
//! shape, so they run on the READ pool. Only create, instantiate and
//! `create-folder` resolve with `ensure: true` (General unprovisioned → 500
//! `Quilltap General store is not provisioned yet`); everything else never
//! provisions.
//!
//! **The shared preamble (v4 `600c2906f` / `a0ed5ecad`; P4.D377 item 5).**
//! The owner / item labels (a missing object is `Inventory object` / `Group
//! inventory object` / `Project inventory object` / `General inventory
//! object` per tier), the resolver, the read probe (with its `General not
//! provisioned; nothing to read` / `No store yet; nothing to read` DEBUGs),
//! `?action=folders` (`Listed inventory folders` now carries `mountPointId`),
//! the three folder writes with v4's `Created|Renamed|Deleted inventory
//! folder` INFOs, and the shared refusals — the archived-character INFO is
//! `[Item routes] Refused a inventory write to an archived character` ("a
//! inventory", v4's grammar, VERBATIM — R-E) — are
//! [`super::item_folder_routes`]'s; [`InventoryKind`] is what the Inventory
//! tells it.
//!
//! **Log lines.** v4 keys each line by the tier's `logIdKey` (`characterId` /
//! `groupId` / `projectId` / `generalId`); tracing field names are static, so
//! [`tier_line!`] is the match. Each line runs on the CALLING thread after the
//! read / write returns (memory note `capture-rig-writer-thread`). The
//! keystone's lines beneath (`[InventoryLocation]`, `[InventoryPool]`,
//! `[InventoryMutations]`) are the keystone's.

use rusqlite::Connection;
use serde_json::{json, Map, Value};

use crate::db::runtime::Db;
use crate::db::DbError;
use crate::inventory::dependents::find_dependents;
use crate::inventory::errors::InventoryError;
use crate::inventory::folders::InventoryFolderError;
use crate::inventory::item_images::assert_inventory_image_choice;
use crate::inventory::item_mutations::{
    clear_overrides, copy_inventory_items_to_folder, create_inventory_item, delete_inventory_item,
    detach, duplicate_inventory_item, instantiate, move_inventory_items_to_folder,
    update_inventory_item, CreateInventoryItemOptions, InstantiateOptions,
};
use crate::inventory::location::{
    resolve_inventory_location, InventoryLocation, ResolveInventoryLocationOptions,
};
use crate::inventory::pool::{
    load_inventory_pool, load_pool_for_location, InventoryPool, LoadInventoryPoolOptions,
    LoadPoolContext,
};
use crate::inventory::types::{
    parse_clear_inventory_override_body, parse_create_inventory_body,
    parse_file_inventory_items_body, parse_instantiate_inventory_body,
    parse_inventory_folder_path_body, parse_rename_inventory_folder_body,
    parse_update_inventory_body, InventoryFields, InventoryRecord, InventoryScope,
};
use crate::services::image_job_common::with_both_conns_maybe_for;

use super::item_folder_routes::{
    self as preamble, item_error_response, ItemKindTag, ItemRouteConfig, ItemRouteKind,
    ItemTierScope, Probed as Probe, SharedRefusal,
};
use super::types::{db_error_response, ErrorKind, InventoryContainer, Response};
use super::zod_issues::ZodIssue;

/// v4 `InventoryRouteConfig` — the tier, its log prefix and the meta key
/// naming the owner id in its lines (`paramsToId` is the container's `id`).
#[derive(Debug, Clone, Copy)]
pub struct InventoryRouteConfig {
    pub scope: InventoryScope,
    /// Log-message prefix, e.g. `[Projects v1]`.
    pub log_tag: &'static str,
    /// The meta key naming the owner id, e.g. `projectId`.
    pub log_id_key: &'static str,
}

impl InventoryRouteConfig {
    /// v4 `OWNER_LABELS[scope]` — the 404 a missing owner answers.
    pub fn owner_label(&self) -> &'static str {
        preamble::owner_label(self.scope.into())
    }

    /// v4 `INVENTORY_ROUTE_KIND.itemLabels[scope]` — the 404 a missing object
    /// answers, per tier.
    pub fn item_label(&self) -> &'static str {
        InventoryKind::item_label(self.scope.into())
    }

    /// The preamble's config shape.
    fn item_cfg(&self) -> ItemRouteConfig {
        ItemRouteConfig {
            scope: self.scope.into(),
            log_tag: self.log_tag,
        }
    }
}

/// v4 `INVENTORY_ROUTE_KIND` — what the shared preamble needs to know about the
/// Inventory (P4.D377 item 5).
pub struct InventoryKind;

impl ItemRouteKind for InventoryKind {
    type Location = InventoryLocation;
    const TAG: ItemKindTag = ItemKindTag::Inventory;

    fn folder_kind(
    ) -> &'static crate::inventory::item_folder_location::ItemFolderKind<InventoryError> {
        &crate::inventory::item_mutations::INVENTORY_FOLDER_KIND
    }
    fn item_label(scope: ItemTierScope) -> &'static str {
        match scope {
            ItemTierScope::Character => "Inventory object",
            ItemTierScope::Group => "Group inventory object",
            ItemTierScope::Project => "Project inventory object",
            ItemTierScope::General => "General inventory object",
        }
    }
    fn resolve(
        main: &Connection,
        mount: &Connection,
        scope: ItemTierScope,
        id: Option<&str>,
        user_id: &str,
        ensure: bool,
    ) -> Result<Option<InventoryLocation>, DbError> {
        let scope = match scope {
            ItemTierScope::Character => InventoryScope::Character,
            ItemTierScope::Group => InventoryScope::Group,
            ItemTierScope::Project => InventoryScope::Project,
            ItemTierScope::General => InventoryScope::General,
        };
        resolve_inventory_location(
            main,
            mount,
            scope,
            id,
            user_id,
            ResolveInventoryLocationOptions { ensure },
        )
    }
    fn location_id(location: &InventoryLocation) -> Option<&str> {
        location.id.as_deref()
    }
    fn error_response(error: InventoryError) -> Response {
        inventory_error_response(error)
    }
    fn wrap(_scope: ItemTierScope, body: Value) -> Response {
        Response::Inventory(body)
    }
    fn parse_path_body(body: &Value) -> Result<String, Response> {
        parse_inventory_folder_path_body(body)
            .map(|b| str_field(&b, "path"))
            .map_err(|issues| validation_error(&issues))
    }
    fn parse_rename_body(body: &Value) -> Result<(String, String), Response> {
        parse_rename_inventory_folder_body(body)
            .map(|b| (str_field(&b, "path"), str_field(&b, "name")))
            .map_err(|issues| validation_error(&issues))
    }
}

// P4.166 (dogfood #156) gated project / group item writes on the owner's
// overlay read (v4's 503 when the owner was HYDRATED to resolve a location).
// RETIRED by measurement at the `624d0d25c` pin (P4.D377, R-L5): v4's
// locations read owners RAW through `resolveOwnerStore`, so a degraded mount
// index no longer surfaces as the owner's 503 — a write fails at the store as
// the neutral 500 (`degraded_mount_routes`' `project_inventory_create` /
// `group_wardrobe_create`). No tier has an entity gate now.

/// The tier's factory config for a wire scope (the four route files' configs:
/// `characters.rs`, `groups.rs`, `projects.rs`, `inventory.rs`).
pub fn config_for(scope: super::types::InventoryScope) -> InventoryRouteConfig {
    use super::types::InventoryScope as S;
    match scope {
        S::Character => super::characters::CHARACTER_INVENTORY,
        S::Group => super::groups::GROUP_INVENTORY,
        S::Project => super::projects::PROJECT_INVENTORY,
        S::General => super::inventory::GENERAL_INVENTORY,
    }
}

/// One factory line keyed by the tier's `logIdKey`, under the tier's route
/// target. `$id` is the rendered owner id (`"null"` for General); `$rest` are
/// v4's remaining keys in v4's order (each line ends `context = "inventory"`).
macro_rules! tier_line {
    ($level:ident, $cfg:expr, $id:expr, $message:expr; $($rest:tt)*) => {{
        let cfg: &InventoryRouteConfig = &$cfg;
        let id: &str = $id;
        match cfg.scope {
            InventoryScope::Character => tracing::$level!(
                target: "quilltap_core::api::characters",
                characterId = id, $($rest)* "{} {}", cfg.log_tag, $message
            ),
            InventoryScope::General => tracing::$level!(
                target: "quilltap_core::api::inventory",
                generalId = id, $($rest)* "{} {}", cfg.log_tag, $message
            ),
            InventoryScope::Project => tracing::$level!(
                target: "quilltap_core::api::projects",
                projectId = id, $($rest)* "{} {}", cfg.log_tag, $message
            ),
            InventoryScope::Group => tracing::$level!(
                target: "quilltap_core::api::groups",
                groupId = id, $($rest)* "{} {}", cfg.log_tag, $message
            ),
        }
    }};
}

fn bad_request(msg: impl Into<String>) -> Response {
    Response::error(ErrorKind::BadRequest, msg.into())
}

/// v4 `notFound(resource)` → `{ error: '<resource> not found' }`.
pub(crate) fn not_found(resource: &str) -> Response {
    Response::error(ErrorKind::NotFound, format!("{resource} not found"))
}

/// v4's context middleware for anything a handler rethrows.
pub(crate) fn internal_server_error() -> Response {
    Response::error(ErrorKind::Internal, "Internal server error")
}

/// v4's middleware `validationError(zodError)` — `{error: 'Validation error',
/// details: [issues]}` at 400, the issue array in Zod's own bytes.
pub(crate) fn validation_error(issues: &[ZodIssue]) -> Response {
    Response::validation_error(Value::Array(
        issues.iter().map(ZodIssue::to_value).collect(),
    ))
}

/// A storage failure that escapes a handler: a broken store is v4's
/// contextful 503 (the middleware's mapping); anything else its generic 500.
pub(crate) fn escaped(e: DbError) -> Response {
    match e {
        e @ DbError::StoreUnavailable { .. } => db_error_response(e),
        _ => internal_server_error(),
    }
}

/// v4 `inventoryErrorResponse(error)` (`responses.ts`) =
/// `itemErrorResponse(INVENTORY_ITEM_ERRORS, error)` — the Inventory's own
/// refusals first, then the shared folder refusals and the archived-character
/// INFO + 409 ([`item_error_response`]). v4 rethrows anything else into the
/// middleware (its generic 500; a broken store its contextful 503).
///
/// The refusal's `code` is NESTED under `details` (`errorResponse(message,
/// status, { code, ...details })`) and the body is `{error, details}`, so the
/// flat [`CoreError::code`](super::types::CoreError) slot — v4's file-op
/// `{error, code}` wire — stays `None`.
pub fn inventory_error_response(error: InventoryError) -> Response {
    match error {
        InventoryError::Refusal(refusal) => {
            let kind = if refusal.status == 400 {
                ErrorKind::BadRequest
            } else {
                ErrorKind::Conflict
            };
            let details = Value::Object(refusal.code_and_details());
            with_details(Response::error(kind, refusal.message), details)
        }
        // `badRequest(message, { code: 'INVALID_SVG' })`.
        InventoryError::Svg(e) => with_details(bad_request(e.0), json!({ "code": "INVALID_SVG" })),
        InventoryError::Folder(e) => item_error_response(
            "inventory",
            match e {
                InventoryFolderError::Path(message) => SharedRefusal::FolderPath(message),
                e @ InventoryFolderError::NotInLocation(_) => {
                    SharedRefusal::NotInLocation(e.to_string())
                }
                // `notFound('Inventory folder')` — the error's own message is not used.
                InventoryFolderError::NotFound(_) => SharedRefusal::FolderNotFound,
                InventoryFolderError::Conflict(message) => SharedRefusal::FolderConflict(message),
            },
        ),
        InventoryError::ForeignImage { .. } => {
            bad_request("imageFileId must name one of this object's own pictures")
        }
        InventoryError::CharacterArchived { .. } => {
            item_error_response("inventory", SharedRefusal::CharacterArchived)
        }
        InventoryError::Db(e) => escaped(e),
        InventoryError::Other(_) => internal_server_error(),
    }
}

fn with_details(mut response: Response, details: Value) -> Response {
    if let Response::Error(e) = &mut response {
        e.details = Some(Box::new(details));
    }
    response
}

/// v4 `location?.id ?? null` as a log value.
fn id_of(location: &InventoryLocation) -> &str {
    location.id.as_deref().unwrap_or("null")
}

/// The owner id the route's params name (`paramsToId`): General has none.
fn owner_id(cfg: &InventoryRouteConfig, container: &InventoryContainer) -> Option<String> {
    if cfg.scope == InventoryScope::General {
        None
    } else {
        container.id.clone()
    }
}

/// Run `f` on the read pool (main + mount-index).
fn read_main_mount<T>(
    db: &Db,
    f: impl FnOnce(&Connection, &Connection) -> Result<T, DbError>,
) -> Result<T, DbError> {
    db.read_main(|main| db.read_mount_index(|mount| f(main, mount)))
}

/// v4 `resolveLocation(ctx, params, ensure)` — the preamble's resolver for this
/// tier (its `No inventory location` DEBUG; General unprovisioned → 500; every
/// other tier → 404 owner).
fn resolve_location(
    main: &Connection,
    mount: &Connection,
    cfg: &InventoryRouteConfig,
    id: Option<&str>,
    user_id: &str,
    ensure: bool,
) -> Result<Result<InventoryLocation, Response>, DbError> {
    preamble::resolve_location::<InventoryKind>(main, mount, &cfg.item_cfg(), id, user_id, ensure)
}

/// v4 `probe` — the preamble's read probe for this tier.
fn probe(
    main: &Connection,
    mount: &Connection,
    cfg: &InventoryRouteConfig,
    id: Option<&str>,
    user_id: &str,
) -> Result<Probe<InventoryLocation>, DbError> {
    preamble::probe::<InventoryKind>(main, mount, &cfg.item_cfg(), id, user_id)
}

/// v4 `serialize(pool, location, record)` — `pool.resolve({ ...record, origin:
/// location.origin, mountPointId: location.mountPointId })`. The key order is
/// the keystone's (`resolve.rs`, T1): a re-set key keeps its position.
fn serialize(
    pool: &InventoryPool,
    location: &InventoryLocation,
    record: &InventoryRecord,
) -> Value {
    let mut tagged = record.clone();
    tagged.set("origin", location.origin.to_value());
    tagged.set("mountPointId", json!(location.mount_point_id));
    pool.resolve(&tagged).to_value()
}

fn context<'a>(project_id: Option<&'a str>, chat_id: Option<&'a str>) -> LoadPoolContext<'a> {
    LoadPoolContext {
        chat_id,
        project_id,
    }
}

// ===========================================================================
// Collection
// ===========================================================================

/// The collection verbs, as the engine dispatches them (`body` is the raw
/// request body).
#[derive(Debug, Clone)]
pub enum CollectionAction {
    List {
        include_archived: bool,
        shared: bool,
        project_id: Option<String>,
        chat_id: Option<String>,
    },
    Folders,
    Archetypes {
        project_id: Option<String>,
        chat_id: Option<String>,
    },
    Create {
        project_id: Option<String>,
        chat_id: Option<String>,
        body: Value,
    },
    Instantiate {
        body: Value,
    },
    FolderCreate {
        body: Value,
    },
    FolderRename {
        body: Value,
    },
    FolderDelete {
        body: Value,
    },
    ItemsMove {
        body: Value,
    },
    ItemsCopy {
        body: Value,
    },
    Duplicate {
        body: Value,
    },
}

/// Dispatch one collection verb for `container` (C1's
/// `copy_inventory_items_to_folder` carries a copy's pictures by link, with no
/// codec — so no verb here takes one).
pub async fn collection_action(
    db: &Db,
    action: CollectionAction,
    container: &InventoryContainer,
    user_id: &str,
) -> Response {
    let cfg = config_for(container.scope);
    let id = owner_id(&cfg, container);
    match action {
        CollectionAction::List {
            include_archived,
            shared,
            project_id,
            chat_id,
        } => list(
            db,
            cfg,
            id,
            user_id,
            include_archived,
            shared,
            project_id,
            chat_id,
        ),
        CollectionAction::Folders => folders(db, cfg, id, user_id),
        CollectionAction::Archetypes {
            project_id,
            chat_id,
        } => archetypes(db, cfg, id, user_id, project_id, chat_id),
        CollectionAction::Create {
            project_id,
            chat_id,
            body,
        } => create(db, cfg, id, user_id, project_id, chat_id, &body).await,
        CollectionAction::Instantiate { body } => {
            instantiate_action(db, cfg, id, user_id, &body).await
        }
        CollectionAction::FolderCreate { body } => folder_create(db, cfg, id, user_id, &body).await,
        CollectionAction::FolderRename { body } => folder_rename(db, cfg, id, user_id, &body).await,
        CollectionAction::FolderDelete { body } => folder_delete(db, cfg, id, user_id, &body).await,
        CollectionAction::ItemsMove { body } => {
            file_items(db, cfg, id, user_id, FileAction::Move, &body).await
        }
        CollectionAction::ItemsCopy { body } => {
            file_items(db, cfg, id, user_id, FileAction::Copy, &body).await
        }
        CollectionAction::Duplicate { body } => duplicate(db, cfg, id, user_id, &body).await,
    }
}

/// `GET` — `{ mountPointId, items }` (`{ mountPointId: null, items: [] }` with
/// no store). `shared` + `includeArchived` → every pooled record resolved, in
/// the pool's Map order (archived INCLUDED); `shared` alone → the reachable
/// pool resolved; else the tier's own records serialized.
#[allow(clippy::too_many_arguments)]
fn list(
    db: &Db,
    cfg: InventoryRouteConfig,
    id: Option<String>,
    user_id: &str,
    include_archived: bool,
    shared: bool,
    project_id: Option<String>,
    chat_id: Option<String>,
) -> Response {
    // `shared` is honoured for the character tier only.
    let shared = cfg.scope == InventoryScope::Character && shared;
    let out = read_main_mount(db, |main, mount| {
        let location = match probe(main, mount, &cfg, id.as_deref(), user_id)? {
            Probe::Found(l) => l,
            Probe::Empty => {
                return Ok(Err(Response::Inventory(
                    json!({ "mountPointId": null, "items": [] }),
                )))
            }
            Probe::Respond(r) => return Ok(Err(r)),
        };
        let items: Vec<Value> = if shared {
            let character_id = location.character_id.clone().unwrap_or_default();
            let pool = load_inventory_pool(
                main,
                mount,
                LoadInventoryPoolOptions {
                    character_id: &character_id,
                    chat_id: chat_id.as_deref(),
                    project_id: project_id.as_deref(),
                    own_records: None,
                },
            )?;
            if include_archived {
                pool.by_id
                    .values()
                    .map(|r| pool.resolve(r).to_value())
                    .collect()
            } else {
                pool.resolved().iter().map(|i| i.to_value()).collect()
            }
        } else {
            let pool = load_pool_for_location(main, mount, &location, LoadPoolContext::default())?;
            location
                .read_items(mount, include_archived)?
                .iter()
                .map(|r| serialize(&pool, &location, r))
                .collect()
        };
        Ok(Ok((location, items)))
    });
    match out {
        Ok(Ok((location, items))) => {
            tier_line!(debug, cfg, id_of(&location), "Listed inventory";
                mountPointId = location.mount_point_id.as_str(),
                count = items.len(),
                shared = shared,
                includeArchived = include_archived,
                context = "inventory",
            );
            Response::Inventory(json!({ "mountPointId": location.mount_point_id, "items": items }))
        }
        Ok(Err(r)) => r,
        // P4.166 (dogfood #156, measured): on a DEGRADED mount index v4's
        // General list still resolves its location (the pointer lives in
        // `instance_settings`) and reads the folder through the fallback
        // overlay — 200 with no items, as the General wardrobe list.
        Err(DbError::PartitionUnavailable(_)) if cfg.scope == InventoryScope::General => {
            match db.read_main(crate::db::instance_settings::get_general_mount_point_id) {
                Ok(Some(mount_point_id)) => {
                    // The folder's two fallback reads (its documents, the
                    // legacy JSON), one quiet DEBUG apiece.
                    let degraded = DbError::PartitionUnavailable(
                        crate::write_partition::WriteDbTarget::MountIndex,
                    );
                    for _ in 0..2 {
                        crate::db::fallback::log_mount_index_unavailable(
                            "doc_mount_documents",
                            &degraded,
                        );
                    }
                    tier_line!(debug, cfg, "null", "Listed inventory";
                        mountPointId = mount_point_id.as_str(),
                        count = 0usize,
                        shared = shared,
                        includeArchived = include_archived,
                        context = "inventory",
                    );
                    Response::Inventory(json!({ "mountPointId": mount_point_id, "items": [] }))
                }
                Ok(None) => Response::Inventory(json!({ "mountPointId": null, "items": [] })),
                Err(e) => escaped(e),
            }
        }
        Err(e) => escaped(e),
    }
}

/// `GET ?action=folders` → `{ folders }` (`{ folders: [] }` with no store) —
/// the preamble's `getFolders`.
fn folders(db: &Db, cfg: InventoryRouteConfig, id: Option<String>, user_id: &str) -> Response {
    preamble::get_folders::<InventoryKind>(db, cfg.item_cfg(), id.as_deref(), user_id)
}

/// `GET ?action=archetypes` → `{ items }` — the archetypes reachable from this
/// tier (`{ items: [] }` with no store).
fn archetypes(
    db: &Db,
    cfg: InventoryRouteConfig,
    id: Option<String>,
    user_id: &str,
    project_id: Option<String>,
    chat_id: Option<String>,
) -> Response {
    let out = read_main_mount(db, |main, mount| {
        let location = match probe(main, mount, &cfg, id.as_deref(), user_id)? {
            Probe::Found(l) => l,
            Probe::Empty => return Ok(Err(Response::Inventory(json!({ "items": [] })))),
            Probe::Respond(r) => return Ok(Err(r)),
        };
        let pool = load_pool_for_location(
            main,
            mount,
            &location,
            context(project_id.as_deref(), chat_id.as_deref()),
        )?;
        let items: Vec<Value> = pool
            .resolved()
            .into_iter()
            .filter(|item| item.is_archetype())
            .map(|item| item.to_value())
            .collect();
        Ok(Ok((location, items)))
    });
    match out {
        Ok(Ok((location, items))) => {
            tier_line!(debug, cfg, id_of(&location), "Listed reachable archetypes";
                count = items.len(),
                context = "inventory",
            );
            Response::Inventory(json!({ "items": items }))
        }
        Ok(Err(r)) => r,
        Err(e) => escaped(e),
    }
}

/// The shape every collection write shares after its parse: resolve (with
/// `ensure`) → the mutation, whose refusals map through
/// [`inventory_error_response`]. `Err` of the outer result is a storage
/// failure the resolve let escape (v4's middleware).
async fn collection_write<T: Send + 'static>(
    db: &Db,
    cfg: InventoryRouteConfig,
    id: Option<String>,
    user_id: &str,
    ensure: bool,
    op: impl FnOnce(&Connection, &Connection, &InventoryLocation) -> Result<T, InventoryError>
        + Send
        + 'static,
) -> Response2<(InventoryLocation, T)> {
    let uid = user_id.to_string();
    let out = with_both_conns_maybe_for(db, None, move |main, mount| {
        let location = match resolve_location(main, mount, &cfg, id.as_deref(), &uid, ensure)? {
            Ok(l) => l,
            Err(r) => return Ok(Err(Fail::from(r))),
        };
        match op(main, mount, &location) {
            Ok(v) => Ok(Ok((location, v))),
            Err(e) => Ok(Err(Fail::Refused(e))),
        }
    })
    .await;
    match out {
        Ok(r) => r.map_err(Fail::answer),
        Err(e) => Err(escaped(e)),
    }
}

/// A handler's outcome: the value, or the response to answer.
type Response2<T> = Result<T, Response>;

/// What a writer closure answers when it does not succeed: a response decided
/// there, or a refusal mapped AFTER the write returns — on the calling thread,
/// where [`inventory_error_response`]'s archived-character INFO must log (v4
/// logs on the request's thread; a line inside `Db::write` runs on the writer
/// thread — memory note `capture-rig-writer-thread`).
pub(crate) enum Fail {
    Respond(Response),
    Refused(InventoryError),
}

impl From<Response> for Fail {
    fn from(r: Response) -> Self {
        Fail::Respond(r)
    }
}

impl Fail {
    pub(crate) fn answer(self) -> Response {
        match self {
            Fail::Respond(r) => r,
            Fail::Refused(e) => inventory_error_response(e),
        }
    }
}

/// `POST` — the create → 201 `{ item }` (the transport supplies the 201).
async fn create(
    db: &Db,
    cfg: InventoryRouteConfig,
    id: Option<String>,
    user_id: &str,
    project_id: Option<String>,
    chat_id: Option<String>,
    body: &Value,
) -> Response {
    // Parse before resolving: an invalid body must never provision a store.
    let body = match parse_create_inventory_body(body) {
        Ok(b) => b,
        Err(issues) => return validation_error(&issues),
    };
    let is_kit = body.get("kind").and_then(Value::as_str) == Some("kit");
    match collection_write(db, cfg, id, user_id, true, move |main, mount, location| {
        let pool = if is_kit {
            Some(load_pool_for_location(
                main,
                mount,
                location,
                context(project_id.as_deref(), chat_id.as_deref()),
            )?)
        } else {
            None
        };
        let record = create_inventory_item(
            main,
            mount,
            location,
            &body,
            CreateInventoryItemOptions {
                preserve: None,
                pool: pool.as_ref(),
            },
        )?;
        let pool = match pool {
            Some(p) => p,
            None => load_pool_for_location(main, mount, location, LoadPoolContext::default())?,
        };
        let item = serialize(&pool, location, &record);
        Ok((record, item))
    })
    .await
    {
        Ok((location, (record, item))) => {
            tier_line!(info, cfg, id_of(&location), "Created inventory object";
                userId = user_id,
                mountPointId = location.mount_point_id.as_str(),
                itemId = record.id(),
                kind = record.kind().as_str(),
                context = "inventory",
            );
            Response::Inventory(json!({ "item": item }))
        }
        Err(r) => r,
    }
}

/// `POST ?action=instantiate` → 201 `{ item: { ...item, origin } }`.
async fn instantiate_action(
    db: &Db,
    cfg: InventoryRouteConfig,
    id: Option<String>,
    user_id: &str,
    body: &Value,
) -> Response {
    let body = match parse_instantiate_inventory_body(body) {
        Ok(b) => b,
        Err(issues) => return validation_error(&issues),
    };
    let archetype_id = body
        .get("archetypeId")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let aid = archetype_id.clone();
    match collection_write(db, cfg, id, user_id, true, move |main, mount, location| {
        // `{ projectId: body.projectId ?? null, chatId: body.chatId ?? null }`.
        let pool = load_pool_for_location(
            main,
            mount,
            location,
            context(
                body.get("projectId").and_then(Value::as_str),
                body.get("chatId").and_then(Value::as_str),
            ),
        )?;
        let overrides = body.get("overrides").and_then(Value::as_object);
        let made = instantiate(
            main,
            mount,
            location,
            &aid,
            InstantiateOptions {
                folder: body.get("folder").and_then(Value::as_str),
                overrides,
                pool: Some(&pool),
            },
        )?;
        let mut item = made.item;
        item.set("origin", location.origin.to_value());
        Ok((made.record, item.to_value()))
    })
    .await
    {
        Ok((location, (record, item))) => {
            tier_line!(info, cfg, id_of(&location), "Instantiated archetype";
                userId = user_id,
                itemId = record.id(),
                archetypeId = archetype_id.as_str(),
                context = "inventory",
            );
            Response::Inventory(json!({ "item": item }))
        }
        Err(r) => r,
    }
}

/// `POST ?action=create-folder` `{ path }` → `{ folder }` (`ensure: true`) —
/// the preamble's folder action (v4 `600c2906f`: the Inventory GAINS v4's
/// `Created inventory folder` INFO).
async fn folder_create(
    db: &Db,
    cfg: InventoryRouteConfig,
    id: Option<String>,
    user_id: &str,
    body: &Value,
) -> Response {
    preamble::folder_create::<InventoryKind>(db, cfg.item_cfg(), id.as_deref(), user_id, body).await
}

/// `POST ?action=rename-folder` `{ path, name }` → `{ folder }`.
async fn folder_rename(
    db: &Db,
    cfg: InventoryRouteConfig,
    id: Option<String>,
    user_id: &str,
    body: &Value,
) -> Response {
    preamble::folder_rename::<InventoryKind>(db, cfg.item_cfg(), id.as_deref(), user_id, body).await
}

/// `POST ?action=delete-folder` `{ path }` → `{ movedItems, movedFolders }`.
async fn folder_delete(
    db: &Db,
    cfg: InventoryRouteConfig,
    id: Option<String>,
    user_id: &str,
    body: &Value,
) -> Response {
    preamble::folder_delete::<InventoryKind>(db, cfg.item_cfg(), id.as_deref(), user_id, body).await
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FileAction {
    Move,
    Copy,
}

/// `POST ?action=move-items` / `copy-items` `{ itemIds, folder }` → `{ items }`
/// serialized through a FRESH pool.
async fn file_items(
    db: &Db,
    cfg: InventoryRouteConfig,
    id: Option<String>,
    user_id: &str,
    action: FileAction,
    body: &Value,
) -> Response {
    let body = match parse_file_inventory_items_body(body) {
        Ok(b) => b,
        Err(issues) => return validation_error(&issues),
    };
    let item_ids: Vec<String> = body
        .get("itemIds")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    let folder = str_field(&body, "folder");
    let f = folder.clone();
    match collection_write(db, cfg, id, user_id, false, move |main, mount, location| {
        let records = match action {
            FileAction::Move => {
                move_inventory_items_to_folder(main, mount, location, &item_ids, &f)?
            }
            FileAction::Copy => {
                copy_inventory_items_to_folder(main, mount, location, &item_ids, &f)?
            }
        };
        let pool = load_pool_for_location(main, mount, location, LoadPoolContext::default())?;
        Ok(records
            .iter()
            .map(|r| serialize(&pool, location, r))
            .collect::<Vec<_>>())
    })
    .await
    {
        Ok((location, items)) => {
            let message = match action {
                FileAction::Move => "Moved inventory objects to folder",
                FileAction::Copy => "Copied inventory objects to folder",
            };
            tier_line!(info, cfg, id_of(&location), message;
                userId = user_id,
                folder = folder.as_str(),
                count = items.len(),
                context = "inventory",
            );
            Response::Inventory(json!({ "items": items }))
        }
        Err(r) => r,
    }
}

/// `POST ?action=duplicate` `{ itemId }` → 201 `{ item }` — a copy beside the
/// original, titled "(copy)"; refused when not copyable. v4 parses the body as
/// `duplicateInventorySchema` BEFORE it resolves: a missing, non-string or
/// empty `itemId` is the Zod 400 (v4 `0276d10d9`).
async fn duplicate(
    db: &Db,
    cfg: InventoryRouteConfig,
    id: Option<String>,
    user_id: &str,
    body: &Value,
) -> Response {
    let item_id = match crate::inventory::types::parse_duplicate_inventory_body(body) {
        Ok(parsed) => parsed
            .get("itemId")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        Err(issues) => return validation_error(&issues),
    };
    let uid = user_id.to_string();
    let out = with_both_conns_maybe_for(db, None, move |main, mount| {
        let location = match resolve_location(main, mount, &cfg, id.as_deref(), &uid, false)? {
            Ok(l) => l,
            Err(r) => return Ok(Err(Fail::from(r))),
        };
        let Some(current) = location.find_item(mount, &item_id)? else {
            return Ok(Err(Fail::from(not_found(cfg.item_label()))));
        };
        let made = (|| -> Result<(InventoryRecord, Value), InventoryError> {
            let record = duplicate_inventory_item(main, mount, &location, &current)?;
            let pool = load_pool_for_location(main, mount, &location, LoadPoolContext::default())?;
            let item = serialize(&pool, &location, &record);
            Ok((record, item))
        })();
        Ok(match made {
            Ok((record, item)) => Ok((location, current, record, item)),
            Err(e) => Err(Fail::Refused(e)),
        })
    })
    .await;
    match out {
        Ok(Ok((location, current, record, item))) => {
            tier_line!(info, cfg, id_of(&location), "Duplicated inventory object";
                itemId = current.id(),
                copyId = record.id(),
                context = "inventory",
            );
            Response::Inventory(json!({ "item": item }))
        }
        Ok(Err(f)) => f.answer(),
        Err(e) => escaped(e),
    }
}

fn str_field(body: &Map<String, Value>, key: &str) -> String {
    body.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

// ===========================================================================
// Item
// ===========================================================================

/// The item verbs, as the engine dispatches them.
#[derive(Debug, Clone)]
pub enum ItemAction {
    Get,
    Dependents,
    Update {
        project_id: Option<String>,
        chat_id: Option<String>,
        body: Value,
    },
    ClearOverride {
        body: Value,
    },
    Detach,
    Delete {
        force: bool,
    },
}

/// Dispatch one item verb for `container` / `item_id`.
pub async fn item_action(
    db: &Db,
    action: ItemAction,
    container: &InventoryContainer,
    item_id: &str,
    user_id: &str,
) -> Response {
    let cfg = config_for(container.scope);
    let id = owner_id(&cfg, container);
    match action {
        ItemAction::Get => item_get(db, cfg, id, user_id, item_id),
        ItemAction::Dependents => item_dependents(db, cfg, id, user_id, item_id),
        ItemAction::Update {
            project_id,
            chat_id,
            body,
        } => item_update(db, cfg, id, user_id, item_id, project_id, chat_id, body).await,
        ItemAction::ClearOverride { body } => {
            item_clear_override(db, cfg, id, user_id, item_id, body).await
        }
        ItemAction::Detach => item_detach(db, cfg, id, user_id, item_id).await,
        ItemAction::Delete { force } => item_delete(db, cfg, id, user_id, item_id, force).await,
    }
}

/// v4 `findRecord` — the tier's location (never provisioned) and the record in
/// it (archived included), or the 404 to answer.
fn find_record(
    main: &Connection,
    mount: &Connection,
    cfg: &InventoryRouteConfig,
    id: Option<&str>,
    user_id: &str,
    item_id: &str,
) -> Result<Result<(InventoryLocation, InventoryRecord), Response>, DbError> {
    let location = match resolve_location(main, mount, cfg, id, user_id, false)? {
        Ok(l) => l,
        Err(r) => return Ok(Err(r)),
    };
    match location.find_item(mount, item_id)? {
        Some(record) => Ok(Ok((location, record))),
        None => Ok(Err(not_found(cfg.item_label()))),
    }
}

/// v4 `respondWith` — `{ item }` through a fresh pool, or the 404 when the
/// mutation answered no record.
fn respond_with(
    main: &Connection,
    mount: &Connection,
    item_label: &str,
    location: &InventoryLocation,
    record: Option<&InventoryRecord>,
) -> Result<Response, DbError> {
    let Some(record) = record else {
        return Ok(not_found(item_label));
    };
    let pool = load_pool_for_location(main, mount, location, LoadPoolContext::default())?;
    Ok(Response::Inventory(
        json!({ "item": serialize(&pool, location, record) }),
    ))
}

/// The item `GET` → `{ item }`.
fn item_get(
    db: &Db,
    cfg: InventoryRouteConfig,
    id: Option<String>,
    user_id: &str,
    item_id: &str,
) -> Response {
    match read_main_mount(db, |main, mount| {
        let (location, record) =
            match find_record(main, mount, &cfg, id.as_deref(), user_id, item_id)? {
                Ok(found) => found,
                Err(r) => return Ok(r),
            };
        respond_with(main, mount, cfg.item_label(), &location, Some(&record))
    }) {
        Ok(r) => r,
        Err(e) => escaped(e),
    }
}

/// `GET ?action=dependents` → `{ dependents }` — `[]` for a non-archetype
/// WITHOUT reading.
fn item_dependents(
    db: &Db,
    cfg: InventoryRouteConfig,
    id: Option<String>,
    user_id: &str,
    item_id: &str,
) -> Response {
    let out = read_main_mount(db, |main, mount| {
        let (location, record) =
            match find_record(main, mount, &cfg, id.as_deref(), user_id, item_id)? {
                Ok(found) => found,
                Err(r) => return Ok(Err(r)),
            };
        let dependents = if record.is_archetype() {
            find_dependents(main, mount, record.id())?
        } else {
            Vec::new()
        };
        Ok(Ok((location, record, dependents)))
    });
    match out {
        Ok(Ok((location, record, dependents))) => {
            tier_line!(debug, cfg, id_of(&location), "Listed archetype dependents";
                itemId = record.id(),
                count = dependents.len(),
                context = "inventory",
            );
            Response::Inventory(json!({ "dependents": dependents }))
        }
        Ok(Err(r)) => r,
        Err(e) => escaped(e),
    }
}

/// The item `PUT` → `{ item }`: find FIRST, then parse; a picture choice is
/// checked before the edit (a foreign id is the 400 sentence).
#[allow(clippy::too_many_arguments)]
async fn item_update(
    db: &Db,
    cfg: InventoryRouteConfig,
    id: Option<String>,
    user_id: &str,
    item_id: &str,
    project_id: Option<String>,
    chat_id: Option<String>,
    body: Value,
) -> Response {
    let (uid, iid) = (user_id.to_string(), item_id.to_string());
    let out = with_both_conns_maybe_for(db, None, move |main, mount| {
        let (location, current) = match find_record(main, mount, &cfg, id.as_deref(), &uid, &iid)? {
            Ok(found) => found,
            Err(r) => return Ok(Err(Fail::from(r))),
        };
        let body = match parse_update_inventory_body(&body) {
            Ok(b) => b,
            Err(issues) => return Ok(Err(Fail::from(validation_error(&issues)))),
        };
        let edited = (|| -> Result<Option<InventoryRecord>, InventoryError> {
            if let Some(choice) = body.get("imageFileId") {
                assert_inventory_image_choice(main, current.id(), choice.as_str()).map_err(
                    |e| match e {
                        crate::services::wardrobe_item_images::primitives::ItemImageChoiceError::Foreign {
                            item_id,
                            file_id,
                        } => InventoryError::ForeignImage { item_id, file_id },
                        crate::services::wardrobe_item_images::primitives::ItemImageChoiceError::Read(m) => {
                            InventoryError::Other(m)
                        }
                    },
                )?;
            }
            update_inventory_item(
                main,
                mount,
                &location,
                &current,
                &body,
                context(project_id.as_deref(), chat_id.as_deref()),
            )
        })();
        match edited {
            Ok(record) => {
                let response = respond_with(main, mount, cfg.item_label(), &location, record.as_ref())?;
                Ok(Ok((location, current, response)))
            }
            Err(e) => Ok(Err(Fail::Refused(e))),
        }
    })
    .await;
    match out {
        Ok(Ok((location, current, response))) => {
            tier_line!(info, cfg, id_of(&location), "Updated inventory object";
                userId = user_id,
                itemId = current.id(),
                context = "inventory",
            );
            response
        }
        Ok(Err(f)) => f.answer(),
        Err(e) => escaped(e),
    }
}

/// `PUT ?action=clear-override` `{ fields }` → `{ item }` (no route line).
async fn item_clear_override(
    db: &Db,
    cfg: InventoryRouteConfig,
    id: Option<String>,
    user_id: &str,
    item_id: &str,
    body: Value,
) -> Response {
    let (uid, iid) = (user_id.to_string(), item_id.to_string());
    let out = with_both_conns_maybe_for(db, None, move |main, mount| {
        let (location, current) = match find_record(main, mount, &cfg, id.as_deref(), &uid, &iid)? {
            Ok(found) => found,
            Err(r) => return Ok(Fail::from(r)),
        };
        let body = match parse_clear_inventory_override_body(&body) {
            Ok(b) => b,
            Err(issues) => return Ok(Fail::from(validation_error(&issues))),
        };
        let fields: Vec<String> = body
            .get("fields")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        match clear_overrides(main, mount, &location, &current, &fields) {
            Ok(record) => Ok(Fail::from(respond_with(
                main,
                mount,
                cfg.item_label(),
                &location,
                record.as_ref(),
            )?)),
            Err(e) => Ok(Fail::Refused(e)),
        }
    })
    .await;
    match out {
        Ok(f) => f.answer(),
        Err(e) => escaped(e),
    }
}

/// `PUT ?action=detach` → `{ item }` — bake an instance into a standalone.
async fn item_detach(
    db: &Db,
    cfg: InventoryRouteConfig,
    id: Option<String>,
    user_id: &str,
    item_id: &str,
) -> Response {
    let (uid, iid) = (user_id.to_string(), item_id.to_string());
    let out = with_both_conns_maybe_for(db, None, move |main, mount| {
        let (location, current) = match find_record(main, mount, &cfg, id.as_deref(), &uid, &iid)? {
            Ok(found) => found,
            Err(r) => return Ok(Err(Fail::from(r))),
        };
        match detach(main, mount, &location, &current, None) {
            Ok(record) => {
                let response =
                    respond_with(main, mount, cfg.item_label(), &location, record.as_ref())?;
                Ok(Ok((location, current, response)))
            }
            Err(e) => Ok(Err(Fail::Refused(e))),
        }
    })
    .await;
    match out {
        Ok(Ok((location, current, response))) => {
            tier_line!(info, cfg, id_of(&location), "Detached instance";
                itemId = current.id(),
                context = "inventory",
            );
            response
        }
        Ok(Err(f)) => f.answer(),
        Err(e) => escaped(e),
    }
}

/// The item `DELETE` → `{ success: true, detached }`; `!deleted` → 404.
async fn item_delete(
    db: &Db,
    cfg: InventoryRouteConfig,
    id: Option<String>,
    user_id: &str,
    item_id: &str,
    force: bool,
) -> Response {
    let (uid, iid) = (user_id.to_string(), item_id.to_string());
    let out = with_both_conns_maybe_for(db, None, move |main, mount| {
        let (location, current) = match find_record(main, mount, &cfg, id.as_deref(), &uid, &iid)? {
            Ok(found) => found,
            Err(r) => return Ok(Err(Fail::from(r))),
        };
        match delete_inventory_item(main, mount, &location, &current, force) {
            Ok(result) if !result.deleted => Ok(Err(Fail::from(not_found(cfg.item_label())))),
            Ok(result) => Ok(Ok((location, current, result.detached))),
            Err(e) => Ok(Err(Fail::Refused(e))),
        }
    })
    .await;
    match out {
        Ok(Ok((location, current, detached))) => {
            tier_line!(info, cfg, id_of(&location), "Deleted inventory object";
                userId = user_id,
                itemId = current.id(),
                force = force,
                detached = detached,
                context = "inventory",
            );
            Response::Inventory(json!({ "success": true, "detached": detached }))
        }
        Ok(Err(f)) => f.answer(),
        Err(e) => escaped(e),
    }
}
