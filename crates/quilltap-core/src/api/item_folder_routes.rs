//! The route preamble every tiered item collection shares — the wardrobe's and
//! the Inventory's, at all four tiers — stated once (v4
//! `lib/mount-index/item-folder-routes.ts`, `a0ed5ecad` + `600c2906f`;
//! P4.D377 item 3, the survey's R-C):
//!
//!   - per-tier labels: the owner's 404 ([`owner_label`], v4 `OWNER_LABELS`)
//!     and the item's (each kind's [`ItemRouteKind::item_label`]), so a missing
//!     project item says "Project wardrobe item" / "Project inventory object";
//!   - one error mapping, [`item_error_response`]: the folder refusals both
//!     kinds share and the archived-character tombstone (INFO `[Item routes]
//!     Refused a ${noun} write to an archived character` + 409 — "a inventory"
//!     is v4's grammar, ported VERBATIM, the survey's R-E), after the kind's own
//!     refusals have had their say;
//!   - the location resolver ([`resolve_location`], with its `No ${noun}
//!     location` DEBUG) and the read probe ([`probe`]), which **never
//!     provisions**: a project or group with no store yet, or an unprovisioned
//!     General, answers empty. **No GET passes `ensure`** — the v4 #135 fix:
//!     no `Project Files:` / `Group Files:` store and no `Wardrobe/` folder is
//!     minted by a read;
//!   - `GET ?action=folders` ([`get_folders`]) and the folder write actions
//!     `create-folder` / `rename-folder` / `delete-folder`
//!     ([`folder_create`] / [`folder_rename`] / [`folder_delete`]): each parses
//!     its body (a Zod failure is the kind's own 400 — outside the refusal
//!     mapping, as v4's middleware), resolves the tier's location
//!     (provisioning a missing store ONLY to create a folder), runs the shared
//!     folder mutation (`inventory::item_folder_location`), logs v4's INFO, and
//!     answers; a refusal answers through the kind's error mapping.
//!
//! Response keys stay per kind (`wardrobeItems` vs `items`) — the factories own
//! their list and item bodies; only the preamble is shared. Item filing
//! (`move-items` / `copy-items`) stays per kind.
//!
//! **Log lines.** v4 keys each tier line by `logIdKey` (`characterId` /
//! `generalId` / `projectId` / `groupId`); tracing field names are static, so
//! [`item_tier_line!`] matches on the tier, under each tier's route target
//! (the targets the factories' `tier_line!`s used). The DEBUGs of
//! [`resolve_location`] / [`probe`] run where the resolve runs — inside a
//! writer closure for a write (their pins use `global_capture`); every INFO
//! runs on the calling thread after the write returns.

use rusqlite::Connection;
use serde_json::{json, Value};

use crate::db::groups::GroupsRepository;
use crate::db::projects::ProjectsRepository;
use crate::db::runtime::Db;
use crate::db::DbError;
use crate::inventory::item_folder_location::{
    create_item_folder, delete_item_folder, rename_item_folder, ItemFolderKind, ItemFolderLocation,
};
use crate::inventory::item_folders::capitalise;
use crate::services::image_job_common::with_both_conns_maybe_for;

use super::types::{db_error_response, ErrorKind, Response};

/// The four tiers an item collection lives at (v4 `ItemTierScope`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemTierScope {
    Character,
    Group,
    Project,
    General,
}

impl ItemTierScope {
    pub fn as_str(self) -> &'static str {
        match self {
            ItemTierScope::Character => "character",
            ItemTierScope::Group => "group",
            ItemTierScope::Project => "project",
            ItemTierScope::General => "general",
        }
    }
}

impl From<crate::db::wardrobe_location::WardrobeScope> for ItemTierScope {
    fn from(s: crate::db::wardrobe_location::WardrobeScope) -> Self {
        use crate::db::wardrobe_location::WardrobeScope as W;
        match s {
            W::Character => ItemTierScope::Character,
            W::Group => ItemTierScope::Group,
            W::Project => ItemTierScope::Project,
            W::General => ItemTierScope::General,
        }
    }
}

impl From<crate::inventory::types::InventoryScope> for ItemTierScope {
    fn from(s: crate::inventory::types::InventoryScope) -> Self {
        use crate::inventory::types::InventoryScope as I;
        match s {
            I::Character => ItemTierScope::Character,
            I::Group => ItemTierScope::Group,
            I::Project => ItemTierScope::Project,
            I::General => ItemTierScope::General,
        }
    }
}

/// Which kind a line belongs to — it picks the General tier's route target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemKindTag {
    Wardrobe,
    Inventory,
}

/// v4 `OWNER_LABELS` — the owner a tier's 404 names, the same for every kind.
pub fn owner_label(scope: ItemTierScope) -> &'static str {
    match scope {
        ItemTierScope::Character => "Character",
        ItemTierScope::Group => "Group",
        ItemTierScope::Project => "Project",
        ItemTierScope::General => "Quilltap General",
    }
}

/// What each tiered item route declares (v4 `ItemRouteConfigBase` — the
/// `logIdKey` follows from the tier).
#[derive(Debug, Clone, Copy)]
pub struct ItemRouteConfig {
    pub scope: ItemTierScope,
    /// Log-message prefix, e.g. `[Projects v1]`.
    pub log_tag: &'static str,
}

/// One tier line keyed by the tier's `logIdKey` under the tier's route target.
/// `$id` is the rendered owner id (`"null"` for General); `$rest` are v4's
/// remaining keys in v4's order.
macro_rules! item_tier_line {
    ($level:ident, $kind:expr, $scope:expr, $log_tag:expr, $id:expr, $message:expr; $($rest:tt)*) => {{
        let kind: $crate::api::item_folder_routes::ItemKindTag = $kind;
        let scope: $crate::api::item_folder_routes::ItemTierScope = $scope;
        let log_tag: &str = $log_tag;
        let id: &str = $id;
        let message: &str = $message;
        use $crate::api::item_folder_routes::{ItemKindTag as __K, ItemTierScope as __S};
        match (kind, scope) {
            (_, __S::Character) => tracing::$level!(
                target: "quilltap_core::api::characters",
                characterId = id, $($rest)* "{} {}", log_tag, message
            ),
            (__K::Wardrobe, __S::General) => tracing::$level!(
                target: "quilltap_core::api::wardrobe",
                generalId = id, $($rest)* "{} {}", log_tag, message
            ),
            (__K::Inventory, __S::General) => tracing::$level!(
                target: "quilltap_core::api::inventory",
                generalId = id, $($rest)* "{} {}", log_tag, message
            ),
            (_, __S::Project) => tracing::$level!(
                target: "quilltap_core::api::projects",
                projectId = id, $($rest)* "{} {}", log_tag, message
            ),
            (_, __S::Group) => tracing::$level!(
                target: "quilltap_core::api::groups",
                groupId = id, $($rest)* "{} {}", log_tag, message
            ),
        }
    }};
}

/// A tier line with NO `logIdKey` (`No ${noun} location`, `General not
/// provisioned; nothing to read`), under the tier's route target.
macro_rules! item_tier_line_bare {
    ($level:ident, $kind:expr, $scope:expr, $log_tag:expr, $message:expr; $($rest:tt)*) => {{
        let kind: $crate::api::item_folder_routes::ItemKindTag = $kind;
        let scope: $crate::api::item_folder_routes::ItemTierScope = $scope;
        let log_tag: &str = $log_tag;
        let message: &str = $message;
        use $crate::api::item_folder_routes::{ItemKindTag as __K, ItemTierScope as __S};
        match (kind, scope) {
            (_, __S::Character) => tracing::$level!(
                target: "quilltap_core::api::characters", $($rest)* "{} {}", log_tag, message
            ),
            (__K::Wardrobe, __S::General) => tracing::$level!(
                target: "quilltap_core::api::wardrobe", $($rest)* "{} {}", log_tag, message
            ),
            (__K::Inventory, __S::General) => tracing::$level!(
                target: "quilltap_core::api::inventory", $($rest)* "{} {}", log_tag, message
            ),
            (_, __S::Project) => tracing::$level!(
                target: "quilltap_core::api::projects", $($rest)* "{} {}", log_tag, message
            ),
            (_, __S::Group) => tracing::$level!(
                target: "quilltap_core::api::groups", $($rest)* "{} {}", log_tag, message
            ),
        }
    }};
}

fn not_found(resource: &str) -> Response {
    // v4 `notFound(resource)` → `{ error: '<resource> not found' }`.
    Response::error(ErrorKind::NotFound, format!("{resource} not found"))
}

// ============================================================================
// One error mapping
// ============================================================================

/// The refusals every kind shares (v4's base classes in `item-folders.ts`,
/// and `CharacterArchivedError`).
#[derive(Debug)]
pub enum SharedRefusal {
    /// `ItemFolderPathError` → 400 with its message.
    FolderPath(String),
    /// `ItemNotInLocationError` → 400 with its message.
    NotInLocation(String),
    /// `ItemFolderNotFoundError` → 404 `${Capitalised noun} folder` (its own
    /// message is not used).
    FolderNotFound,
    /// `ItemFolderConflictError` → 409 with its message.
    FolderConflict(String),
    /// `CharacterArchivedError` → the INFO and 409.
    CharacterArchived,
}

/// v4 `itemErrorResponse(kind, error)` past the kind's own refusals: a bad path
/// or a borrowed id is a 400, a missing folder a 404, a collision a 409, an
/// archived character's vault a 409 with v4's INFO.
pub fn item_error_response(noun: &str, refusal: SharedRefusal) -> Response {
    match refusal {
        SharedRefusal::FolderPath(message) | SharedRefusal::NotInLocation(message) => {
            Response::error(ErrorKind::BadRequest, message)
        }
        SharedRefusal::FolderNotFound => not_found(&format!("{} folder", capitalise(noun))),
        SharedRefusal::FolderConflict(message) => Response::error(ErrorKind::Conflict, message),
        SharedRefusal::CharacterArchived => {
            tracing::info!(
                context = noun,
                "[Item routes] Refused a {noun} write to an archived character"
            );
            Response::error(
                ErrorKind::Conflict,
                format!("An archived character's {noun} cannot be changed"),
            )
        }
    }
}

// ============================================================================
// The kind
// ============================================================================

/// Everything a tiered item kind tells the shared preamble (v4
/// `ItemRouteKind<L>`).
pub trait ItemRouteKind: 'static {
    type Location: ItemFolderLocation + Send + 'static;
    /// The General tier's route target.
    const TAG: ItemKindTag;

    /// The folder kind: its rules (and `rules.noun`) and its path error.
    fn folder_kind() -> &'static ItemFolderKind<<Self::Location as ItemFolderLocation>::Error>;
    /// The kind's noun — `wardrobe` / `inventory`.
    fn noun() -> &'static str {
        Self::folder_kind().rules.noun
    }
    /// The item a tier's 404 names.
    fn item_label(scope: ItemTierScope) -> &'static str;
    /// The kind's tier resolver (`resolveWardrobeLocation` /
    /// `resolveInventoryLocation`).
    fn resolve(
        main: &Connection,
        mount: &Connection,
        scope: ItemTierScope,
        id: Option<&str>,
        user_id: &str,
        ensure: bool,
    ) -> Result<Option<Self::Location>, DbError>;
    /// `location.id` — `None` for General.
    fn location_id(location: &Self::Location) -> Option<&str>;
    /// The kind's error mapping (its own refusals, then
    /// [`item_error_response`], then its rethrow class).
    fn error_response(error: <Self::Location as ItemFolderLocation>::Error) -> Response;
    /// The kind's success envelope for the preamble's answers.
    fn wrap(scope: ItemTierScope, body: Value) -> Response;
    /// `schemas.path` — `{ path }`; a refusal is the kind's own 400.
    fn parse_path_body(body: &Value) -> Result<String, Response>;
    /// `schemas.rename` — `{ path, name }`.
    fn parse_rename_body(body: &Value) -> Result<(String, String), Response>;
}

fn id_of<K: ItemRouteKind>(location: &K::Location) -> &str {
    K::location_id(location).unwrap_or("null")
}

// P4.166 (dogfood #156) gated project / group item writes on the owner's
// overlay read (v4's 503 when the owner was HYDRATED to resolve a location).
// RETIRED by measurement at the `624d0d25c` pin (P4.D377, R-L5): v4's
// locations read owners RAW through `resolveOwnerStore`, so a degraded mount
// index no longer surfaces as the owner's 503 — a write fails at the store as
// the neutral 500 (`degraded_mount_routes`' `project_inventory_create` /
// `group_wardrobe_create`). No tier has an entity gate now.

// ============================================================================
// The resolver and the read probe
// ============================================================================

/// v4 `resolveLocation(ctx, params, ensure)` — the tier's location, or the
/// response to answer. `ensure` provisions a missing project / group store and
/// folder; only a write passes it. A miss logs DEBUG `${logTag} No ${noun}
/// location {scope, ensure, context}`; General → 500 `Quilltap General store is
/// not provisioned yet`; every other tier → 404 owner.
pub fn resolve_location<K: ItemRouteKind>(
    main: &Connection,
    mount: &Connection,
    cfg: &ItemRouteConfig,
    id: Option<&str>,
    user_id: &str,
    ensure: bool,
) -> Result<Result<K::Location, Response>, DbError> {
    if let Some(location) = K::resolve(main, mount, cfg.scope, id, user_id, ensure)? {
        return Ok(Ok(location));
    }
    let noun = K::noun();
    item_tier_line_bare!(debug, K::TAG, cfg.scope, cfg.log_tag, &format!("No {noun} location");
        scope = cfg.scope.as_str(),
        ensure,
        context = noun,
    );
    if cfg.scope == ItemTierScope::General {
        return Ok(Err(Response::error(
            ErrorKind::Internal,
            "Quilltap General store is not provisioned yet",
        )));
    }
    Ok(Err(not_found(owner_label(cfg.scope))))
}

/// What a read probe found (v4 `ProbedLocation<L>`).
pub enum Probed<L> {
    /// The tier's location.
    Found(L),
    /// Nothing provisioned yet — an empty answer, never a store minted by a read.
    Empty,
    /// The 404 to answer.
    Respond(Response),
}

/// v4 `probe(ctx, params)` — the read probe: the tier's location
/// (`ensure: false`); [`Probed::Empty`] when the owner exists but has no store
/// yet (DEBUG `${logTag} No store yet; nothing to read {[logIdKey], scope,
/// context}`) or General is unprovisioned (DEBUG `${logTag} General not
/// provisioned; nothing to read {context}`); else the 404 owner. The owner
/// check is the HYDRATED repository read (`projects.findById` /
/// `groups.findById`), while the location itself resolves raw through reach.
pub fn probe<K: ItemRouteKind>(
    main: &Connection,
    mount: &Connection,
    cfg: &ItemRouteConfig,
    id: Option<&str>,
    user_id: &str,
) -> Result<Probed<K::Location>, DbError> {
    if let Some(location) = K::resolve(main, mount, cfg.scope, id, user_id, false)? {
        return Ok(Probed::Found(location));
    }
    let noun = K::noun();
    if cfg.scope == ItemTierScope::General {
        item_tier_line_bare!(debug, K::TAG, cfg.scope, cfg.log_tag,
            "General not provisioned; nothing to read";
            context = noun,
        );
        return Ok(Probed::Empty);
    }
    if cfg.scope != ItemTierScope::Character {
        if let Some(id) = id.filter(|s| !s.is_empty()) {
            let owner = if cfg.scope == ItemTierScope::Project {
                ProjectsRepository::new(main, mount)
                    .find_by_id(id)
                    .map_err(|e| e.into_db())?
                    .is_some()
            } else {
                GroupsRepository::new(main, mount)
                    .find_by_id(id)
                    .map_err(|e| e.into_db())?
                    .is_some()
            };
            if owner {
                item_tier_line!(debug, K::TAG, cfg.scope, cfg.log_tag, id,
                    "No store yet; nothing to read";
                    scope = cfg.scope.as_str(),
                    context = noun,
                );
                return Ok(Probed::Empty);
            }
        }
    }
    Ok(Probed::Respond(not_found(owner_label(cfg.scope))))
}

/// Run `f` on the read pool (main + mount-index).
fn read_main_mount<T>(
    db: &Db,
    f: impl FnOnce(&Connection, &Connection) -> Result<T, DbError>,
) -> Result<T, DbError> {
    db.read_main(|main| db.read_mount_index(|mount| f(main, mount)))
}

/// v4 `getFolders` — `GET ?action=folders`: the probe; nothing provisioned →
/// `{ folders: [] }`; else the tier's folders with DEBUG `${logTag} Listed
/// ${noun} folders {[logIdKey], mountPointId, count, context}` (the Inventory
/// GAINS `mountPointId`, `600c2906f`).
pub fn get_folders<K: ItemRouteKind>(
    db: &Db,
    cfg: ItemRouteConfig,
    id: Option<&str>,
    user_id: &str,
) -> Response {
    enum Out<L> {
        Listed(L, Vec<crate::inventory::item_folders::ItemFolder>),
        Respond(Response),
    }
    let out = read_main_mount(db, |main, mount| {
        let location = match probe::<K>(main, mount, &cfg, id, user_id)? {
            Probed::Found(l) => l,
            Probed::Empty => return Ok(Out::Respond(K::wrap(cfg.scope, json!({ "folders": [] })))),
            Probed::Respond(r) => return Ok(Out::Respond(r)),
        };
        let folders = list_folders::<K>(mount, &location)?;
        Ok(Out::Listed(location, folders))
    });
    match out {
        Ok(Out::Listed(location, folders)) => {
            let noun = K::noun();
            item_tier_line!(debug, K::TAG, cfg.scope, cfg.log_tag, id_of::<K>(&location),
                &format!("Listed {noun} folders");
                mountPointId = location.mount_point_id(),
                count = folders.len(),
                context = noun,
            );
            K::wrap(cfg.scope, json!({ "folders": folders }))
        }
        Ok(Out::Respond(r)) => r,
        Err(e) => db_error_response(e),
    }
}

/// `location.listFolders()` — the folder layer's read half (a READ, no
/// tombstone).
fn list_folders<K: ItemRouteKind>(
    mount: &Connection,
    location: &K::Location,
) -> Result<Vec<crate::inventory::item_folders::ItemFolder>, DbError> {
    location.list_folders(mount)
}

// ============================================================================
// The folder write actions
// ============================================================================

/// The shape every folder write shares: resolve → the mutation (its refusals
/// through the kind's mapping).
async fn folder_write<K, T>(
    db: &Db,
    cfg: ItemRouteConfig,
    id: Option<&str>,
    user_id: &str,
    ensure: bool,
    op: impl FnOnce(
            &Connection,
            &Connection,
            &K::Location,
        ) -> Result<T, <K::Location as ItemFolderLocation>::Error>
        + Send
        + 'static,
) -> Result<(K::Location, T), Response>
where
    K: ItemRouteKind,
    T: Send + 'static,
{
    let (id, uid) = (id.map(str::to_string), user_id.to_string());
    let entity = None;
    match with_both_conns_maybe_for(db, entity, move |main, mount| {
        let location = match resolve_location::<K>(main, mount, &cfg, id.as_deref(), &uid, ensure)?
        {
            Ok(l) => l,
            Err(r) => return Ok(Err(r)),
        };
        match op(main, mount, &location) {
            Ok(v) => Ok(Ok((location, v))),
            Err(e) => Ok(Err(K::error_response(e))),
        }
    })
    .await
    {
        Ok(out) => out,
        Err(e) => Err(db_error_response(e)),
    }
}

/// v4 `itemFolderActionHandlers`' `create-folder`: parse `{ path }` → resolve
/// (`ensure: true`) → `createItemFolder` → INFO `${logTag} Created ${noun}
/// folder {[logIdKey], userId, mountPointId, folder, context}` → `{ folder }`.
pub async fn folder_create<K: ItemRouteKind>(
    db: &Db,
    cfg: ItemRouteConfig,
    id: Option<&str>,
    user_id: &str,
    body: &Value,
) -> Response {
    let path = match K::parse_path_body(body) {
        Ok(p) => p,
        Err(r) => return r,
    };
    match folder_write::<K, _>(db, cfg, id, user_id, true, move |main, mount, loc| {
        create_item_folder(K::folder_kind(), main, mount, loc, &path)
    })
    .await
    {
        Ok((location, folder)) => {
            let noun = K::noun();
            item_tier_line!(info, K::TAG, cfg.scope, cfg.log_tag, id_of::<K>(&location),
                &format!("Created {noun} folder");
                userId = user_id,
                mountPointId = location.mount_point_id(),
                folder = folder.path.as_str(),
                context = noun,
            );
            K::wrap(cfg.scope, json!({ "folder": folder }))
        }
        Err(r) => r,
    }
}

/// `rename-folder`: parse `{ path, name }` → resolve (no ensure) →
/// `renameItemFolder` → INFO `${logTag} Renamed ${noun} folder {[logIdKey],
/// userId, mountPointId, from: body.path, to, context}` → `{ folder }`.
pub async fn folder_rename<K: ItemRouteKind>(
    db: &Db,
    cfg: ItemRouteConfig,
    id: Option<&str>,
    user_id: &str,
    body: &Value,
) -> Response {
    let (path, name) = match K::parse_rename_body(body) {
        Ok(b) => b,
        Err(r) => return r,
    };
    let from = path.clone();
    match folder_write::<K, _>(db, cfg, id, user_id, false, move |main, mount, loc| {
        rename_item_folder(K::folder_kind(), main, mount, loc, &path, &name)
    })
    .await
    {
        Ok((location, folder)) => {
            let noun = K::noun();
            item_tier_line!(info, K::TAG, cfg.scope, cfg.log_tag, id_of::<K>(&location),
                &format!("Renamed {noun} folder");
                userId = user_id,
                mountPointId = location.mount_point_id(),
                from = from.as_str(),
                to = folder.path.as_str(),
                context = noun,
            );
            K::wrap(cfg.scope, json!({ "folder": folder }))
        }
        Err(r) => r,
    }
}

/// `delete-folder`: parse `{ path }` → resolve (no ensure) →
/// `deleteItemFolder` → INFO `${logTag} Deleted ${noun} folder {[logIdKey],
/// userId, mountPointId, folder: body.path, movedItems, movedFolders, context}`
/// → `{ movedItems, movedFolders }`.
pub async fn folder_delete<K: ItemRouteKind>(
    db: &Db,
    cfg: ItemRouteConfig,
    id: Option<&str>,
    user_id: &str,
    body: &Value,
) -> Response {
    let path = match K::parse_path_body(body) {
        Ok(p) => p,
        Err(r) => return r,
    };
    let requested = path.clone();
    match folder_write::<K, _>(db, cfg, id, user_id, false, move |main, mount, loc| {
        delete_item_folder(K::folder_kind(), main, mount, loc, &path)
    })
    .await
    {
        Ok((location, deleted)) => {
            let noun = K::noun();
            item_tier_line!(info, K::TAG, cfg.scope, cfg.log_tag, id_of::<K>(&location),
                &format!("Deleted {noun} folder");
                userId = user_id,
                mountPointId = location.mount_point_id(),
                folder = requested.as_str(),
                movedItems = deleted.moved_items,
                movedFolders = deleted.moved_folders,
                context = noun,
            );
            K::wrap(
                cfg.scope,
                json!({
                    "movedItems": deleted.moved_items,
                    "movedFolders": deleted.moved_folders,
                }),
            )
        }
        Err(r) => r,
    }
}

#[cfg(test)]
mod preamble_lines {
    //! P4.D377 item 16 — the preamble's lines, capture-pinned byte for byte
    //! (level, target, `<logTag> <message>`, then v4's keys in v4's order), each
    //! with its silence leg. v5-side LITERALS checked against v4's
    //! `item-folder-routes.ts` at the `624d0d25c` pin; the route families
    //! compare bodies and dumps only. Driven over a real provisioned instance
    //! with the process-global capture (the resolver's DEBUGs run inside the
    //! writer closure).
    use super::*;
    use crate::api::inventory_route_factory::{collection_action, CollectionAction};
    use crate::api::types::{InventoryContainer, InventoryScope as WireInventoryScope};
    use crate::api::wardrobe_route_factory::{
        duplicate, folder_create, folders_list, list_store_items, GENERAL_WARDROBE,
    };
    use crate::db::runtime::DbPaths;
    use crate::test_support::global_capture::{capture, capture_async};

    const PEPPER: &str = "cXVpbGx0YXAtdGVzdC1wZXBwZXItMzItYnl0ZXMhIQ==";
    const USER: &str = "u-preamble";
    const MISSING_PROJECT: &str = "00000000-0000-4000-8000-0000000000ff";

    async fn instance(dir: &std::path::Path) -> Db {
        let path = dir.to_path_buf();
        tokio::task::spawn_blocking(move || {
            crate::services::provisioning::provision_fresh_instance(&path, PEPPER).unwrap();
            Db::open(
                DbPaths {
                    main: path.join("quilltap.db"),
                    mount_index: Some(path.join("quilltap-mount-index.db")),
                    llm_logs: None,
                },
                PEPPER,
            )
            .unwrap()
        })
        .await
        .unwrap()
    }

    fn general_mount(db: &Db) -> String {
        db.read_main(|main| {
            Ok(
                crate::db::instance_settings::get_general_mount_point_id(main)?
                    .expect("provisioned"),
            )
        })
        .unwrap()
    }

    /// The lines carrying `needle`, each from its level on.
    fn lines_with(lines: &[String], needle: &str) -> Vec<String> {
        lines
            .iter()
            .filter(|l| l.contains(needle))
            .cloned()
            .collect()
    }

    fn inventory_general() -> InventoryContainer {
        InventoryContainer {
            scope: WireInventoryScope::General,
            id: None,
        }
    }

    #[tokio::test]
    async fn the_inventory_gains_v4s_folder_infos_and_the_listing_mount() {
        crate::test_support::global_capture::install();
        let dir = tempfile::tempdir().unwrap();
        let db = instance(dir.path()).await;
        let mp = general_mount(&db);
        const HEAD: &str = "quilltap_core::api::inventory [Inventory General v1]";
        let general = inventory_general();
        let post = |action: CollectionAction| collection_action(&db, action, &general, USER);

        let (_, lines) = capture_async(post(CollectionAction::FolderCreate {
            body: json!({ "path": "Shelves" }),
        }))
        .await;
        assert_eq!(
            lines_with(&lines, "inventory folder"),
            vec![format!(
                "INFO {HEAD} Created inventory folder generalId=null userId={USER} \
                 mountPointId={mp} folder=Shelves context=inventory"
            )]
        );
        let (_, lines) = capture_async(post(CollectionAction::Folders)).await;
        assert_eq!(
            lines_with(&lines, "Listed inventory folders"),
            vec![format!(
                "DEBUG {HEAD} Listed inventory folders generalId=null mountPointId={mp} \
                 count=1 context=inventory"
            )]
        );
        let (_, lines) = capture_async(post(CollectionAction::FolderRename {
            body: json!({ "path": "shelves", "name": "Racks" }),
        }))
        .await;
        assert_eq!(
            lines_with(&lines, "inventory folder"),
            vec![format!(
                "INFO {HEAD} Renamed inventory folder generalId=null userId={USER} \
                 mountPointId={mp} from=shelves to=Racks context=inventory"
            )]
        );
        let (_, lines) = capture_async(post(CollectionAction::FolderDelete {
            body: json!({ "path": "Racks" }),
        }))
        .await;
        assert_eq!(
            lines_with(&lines, "inventory folder"),
            vec![format!(
                "INFO {HEAD} Deleted inventory folder generalId=null userId={USER} \
                 mountPointId={mp} folder=Racks movedItems=0 movedFolders=0 context=inventory"
            )]
        );
        // Silence: a refused rename (no such folder → 404) logs no route line.
        let (resp, lines) = capture_async(post(CollectionAction::FolderRename {
            body: json!({ "path": "Nowhere", "name": "Elsewhere" }),
        }))
        .await;
        assert!(
            matches!(&resp, Response::Error(e) if e.kind == ErrorKind::NotFound
                && e.message == "Inventory folder not found"),
            "{resp:?}"
        );
        assert!(
            lines_with(&lines, "inventory folder").is_empty(),
            "{lines:?}"
        );
    }

    #[tokio::test]
    async fn the_probe_and_the_resolver_log_their_misses() {
        crate::test_support::global_capture::install();
        let dir = tempfile::tempdir().unwrap();
        let db = instance(dir.path()).await;

        // A write's resolve that finds nothing: the resolver's DEBUG and the
        // 404. Driven on DIRECT connections on the test thread — in production
        // it runs inside the writer closure, where the thread-local capture
        // cannot see it (memory note `capture-rig-writer-thread`).
        {
            let main =
                crate::db::Writer::open_writable(&dir.path().join("quilltap.db"), PEPPER).unwrap();
            let mount = crate::db::Writer::open_writable_in(
                crate::db::table_shape::Partition::MountIndex,
                &dir.path().join("quilltap-mount-index.db"),
                PEPPER,
            )
            .unwrap();
            let cfg = ItemRouteConfig {
                scope: ItemTierScope::Project,
                log_tag: "[Projects v1]",
            };
            let (resp, lines) = capture(|| {
                resolve_location::<crate::api::wardrobe_route_factory::WardrobeKind>(
                    main.connection(),
                    mount.connection(),
                    &cfg,
                    Some(MISSING_PROJECT),
                    USER,
                    true,
                )
                .unwrap()
                .expect_err("a missing project resolves to the 404")
            });
            assert!(
                matches!(&resp, Response::Error(e) if e.message == "Project not found"),
                "{resp:?}"
            );
            assert_eq!(
                lines_with(&lines, "No wardrobe location"),
                vec![
                    "DEBUG quilltap_core::api::projects [Projects v1] No wardrobe location \
                     scope=project ensure=true context=wardrobe"
                        .to_string()
                ]
            );
            // Silence: a resolve that FINDS its location logs no miss.
            let general = ItemRouteConfig {
                scope: ItemTierScope::General,
                log_tag: "[Wardrobe Archetypes v1]",
            };
            let (found, lines) = capture(|| {
                resolve_location::<crate::api::wardrobe_route_factory::WardrobeKind>(
                    main.connection(),
                    mount.connection(),
                    &general,
                    None,
                    USER,
                    false,
                )
                .unwrap()
                .is_ok()
            });
            assert!(found);
            assert!(lines_with(&lines, "No wardrobe location").is_empty());
        }
        let _ = folder_create;

        // An unprovisioned General: every GET answers empty with the probe's
        // DEBUG (both kinds), never the resolver's.
        db.write(|ws| {
            ws.main()
                .connection()
                .execute(
                    "DELETE FROM instance_settings WHERE key = 'generalMountPointId'",
                    [],
                )
                .map(|_| ())
                .map_err(Into::into)
        })
        .await
        .unwrap();
        let (resp, lines) =
            capture_async(list_store_items(&db, GENERAL_WARDROBE, None, USER, false)).await;
        let Response::Wardrobe(body) = resp else {
            panic!("{resp:?}")
        };
        assert_eq!(
            serde_json::to_string(&body).unwrap(),
            r#"{"mountPointId":null,"wardrobeItems":[]}"#
        );
        assert_eq!(
            lines_with(&lines, "nothing to read"),
            vec![
                "DEBUG quilltap_core::api::wardrobe [Wardrobe Archetypes v1] General not \
                 provisioned; nothing to read context=wardrobe"
                    .to_string()
            ]
        );
        assert!(lines_with(&lines, "No wardrobe location").is_empty());
        let (_, lines) = capture(|| folders_list(&db, GENERAL_WARDROBE, None, USER));
        assert_eq!(lines_with(&lines, "nothing to read").len(), 1);
        let (_, lines) = capture_async(collection_action(
            &db,
            CollectionAction::Folders,
            &inventory_general(),
            USER,
        ))
        .await;
        assert_eq!(
            lines_with(&lines, "nothing to read"),
            vec![
                "DEBUG quilltap_core::api::inventory [Inventory General v1] General not \
                 provisioned; nothing to read context=inventory"
                    .to_string()
            ]
        );
    }

    #[test]
    fn the_archived_refusal_logs_v4s_info_per_kind_and_the_others_log_nothing() {
        crate::test_support::global_capture::install();
        for noun in ["wardrobe", "inventory"] {
            let (resp, lines) =
                capture(|| item_error_response(noun, SharedRefusal::CharacterArchived));
            assert!(
                matches!(&resp, Response::Error(e) if e.kind == ErrorKind::Conflict
                    && e.message == format!("An archived character's {noun} cannot be changed")),
                "{resp:?}"
            );
            // "a inventory" — v4's grammar, VERBATIM (R-E).
            assert_eq!(
                lines_with(&lines, "[Item routes]"),
                vec![format!(
                    "INFO quilltap_core::api::item_folder_routes [Item routes] Refused a {noun} \
                     write to an archived character context={noun}"
                )]
            );
            let (resp, lines) =
                capture(|| item_error_response(noun, SharedRefusal::FolderNotFound));
            assert!(
                matches!(&resp, Response::Error(e) if e.message
                    == format!("{} folder not found", capitalise(noun))),
                "{resp:?}"
            );
            assert!(lines_with(&lines, "[Item routes]").is_empty());
        }
    }

    #[tokio::test]
    async fn the_duplicate_logs_its_debug_and_info_and_a_miss_logs_neither() {
        crate::test_support::global_capture::install();
        let dir = tempfile::tempdir().unwrap();
        let db = instance(dir.path()).await;
        let mp = general_mount(&db);
        let made = crate::api::wardrobe_route_factory::create(
            &db,
            GENERAL_WARDROBE,
            None,
            USER,
            &json!({ "title": "Spats", "types": ["footwear"], "folder": "Feet" }),
        )
        .await;
        let Response::Wardrobe(made) = made else {
            panic!("{made:?}")
        };
        let source = made["wardrobeItem"]["id"].as_str().unwrap().to_string();
        let (resp, lines) = capture_async(duplicate(
            &db,
            GENERAL_WARDROBE,
            None,
            USER,
            &json!({ "itemId": source }),
        ))
        .await;
        let Response::Wardrobe(body) = resp else {
            panic!("{resp:?}")
        };
        let copy = body["wardrobeItem"]["id"].as_str().unwrap();
        assert_eq!(body["wardrobeItem"]["title"], "Spats (copy)");
        assert_eq!(body["wardrobeItem"]["folder"], "Feet");
        const HEAD: &str = "quilltap_core::api::wardrobe [Wardrobe Archetypes v1]";
        assert_eq!(
            lines_with(&lines, "uplicat"),
            vec![
                format!(
                    "DEBUG {HEAD} Duplicating wardrobe item generalId=null itemId={source} \
                     title=Spats (copy) folder=Feet context=wardrobe"
                ),
                format!(
                    "INFO {HEAD} Duplicated wardrobe item generalId=null userId={USER} \
                     mountPointId={mp} itemId={source} copyId={copy} title=Spats (copy) \
                     context=wardrobe"
                ),
            ]
        );
        // Silence: a missing item is the per-tier 404 and logs neither line.
        let (resp, lines) = capture_async(duplicate(
            &db,
            GENERAL_WARDROBE,
            None,
            USER,
            &json!({ "itemId": MISSING_PROJECT }),
        ))
        .await;
        assert!(
            matches!(&resp, Response::Error(e) if e.message == "Archetype wardrobe item not found"),
            "{resp:?}"
        );
        assert!(lines_with(&lines, "uplicat").is_empty(), "{lines:?}");
    }
}
