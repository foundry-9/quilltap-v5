//! The wardrobe route factory — ONE implementation of the wardrobe item
//! endpoints for all four tiers (v4 `lib/wardrobe/routes/wardrobe-route-factory.ts`,
//! `9ecebf986` + `4a38222e6`; P4.D281):
//!
//!   - `/api/v1/characters/[id]/wardrobe` (+ `/[itemId]`) — a character's vault
//!   - `/api/v1/groups/[id]/wardrobe`     (+ `/[itemId]`) — a group's store
//!   - `/api/v1/projects/[id]/wardrobe`   (+ `/[itemId]`) — a project's store
//!   - `/api/v1/wardrobe`                 (+ `/[itemId]`) — Quilltap General
//!
//! Every tier is a location ([`resolve_wardrobe_location`]), so the handler
//! bodies are written once: list / create (each with `?action=instructions`),
//! the folder actions (`GET ?action=folders`; `POST ?action=create-folder`,
//! `rename-folder`, `delete-folder`, `move-items`), and get (with
//! `?action=wear-history`) / update / delete. Each tier's file is a config
//! ([`WardrobeRouteConfig`]) and one-line delegates. Rules that hold everywhere:
//!
//!   - an item read or written is serialized the same way — `origin` and
//!     `wear` attached — on GET, POST and PUT (`serializeWardrobeItems`);
//!   - a component cycle is a 400, and so is a folder path that breaks the
//!     rules or a move naming an item filed elsewhere; a missing folder is a
//!     404 and a folder collision a 409 ([`write_error_response`]);
//!   - a PUT checks the item exists before it checks the picture choice;
//!   - a character must be the requesting user's (the location checks — R-E);
//!   - a write to an archived character's vault is a 409.
//!
//! **Provisioning (v4's own `ensure` flags, ported as written).** The list GET
//! and the instructions GET resolve with `ensure: scope !== 'character'` — a
//! project / group list provisions its store and `Wardrobe/` folder, and
//! General ensures the folder — so those two run on the WRITER for the three
//! store tiers (the read pool is read-only); the character tier stays a read.
//! Create, the instructions POST and `create-folder` ensure; every item route,
//! `?action=folders`, `rename-folder`, `delete-folder` and `move-items` never
//! provision.
//!
//! **Log lines.** v4 keys each line by the tier's `logIdKey` (`characterId` /
//! `generalId` / `projectId` / `groupId`); tracing field names are static, so
//! [`tier_line!`] is the match. Each line is emitted under its tier's ROUTE
//! target (the target the tier's handler lived under before the factory), and
//! on the CALLING thread — never inside a `Db::write` closure, where the
//! capture rig cannot see it (memory note `capture-rig-writer-thread`). The
//! keystone's own lines beneath (`[WardrobeLocation]`, `[ItemMutations]`,
//! `[FolderMutations]`, the folder-writer DEBUGs) are the keystone's (R-H).

use rusqlite::Connection;
use serde_json::{json, Map, Value};

use crate::db::archetype_wardrobe::{with_origin, WardrobeOrigin, WardrobeOriginScope};
use crate::db::doc_mount_documents::DocMountDocumentsRepository;
use crate::db::doc_mount_file_links::DocMountFileLinksRepository;
use crate::db::groups::GroupsRepository;
use crate::db::projects::ProjectsRepository;
use crate::db::runtime::Db;
use crate::db::tiered_mount_pool::resolve_group_mounts_for_character;
use crate::db::wardrobe_location::{
    resolve_wardrobe_location, ResolveWardrobeLocationOptions, WardrobeLocation,
    WardrobeLocationError, WardrobeScope,
};
use crate::db::wardrobe_read::read_shared_tiers;
use crate::db::DbError;
use crate::services::image_job_common::with_both_conns;
use crate::services::wardrobe_folder_mutations::{
    create_wardrobe_folder, delete_wardrobe_folder, rename_wardrobe_folder,
};
use crate::services::wardrobe_item_images::primitives::{
    cleanup_item_images, ItemImageCleanupMeta,
};
use crate::services::wardrobe_item_mutations::{
    component_lookup_for, create_item, move_items_to_folder, update_item, CreateItemOptions,
    ItemPatch, WardrobeCreateBody,
};
use crate::services::wardrobe_item_route_steps::{
    image_choice_error, log_cleanup_equipped_refs, parse_image_file_id, run_cleanup_equipped_refs,
    ItemRouteMeta,
};
use crate::services::wardrobe_wear_history::{attach_wear, build_wear_history_payload};
use crate::vault_overlay::WardrobeItem;
use crate::wardrobe_instructions::{
    read_wardrobe_instructions_file, write_wardrobe_instructions_file,
};

use super::types::{db_error_response, ErrorKind, Response};

/// v4 `WardrobeRouteConfig` — the tier, and its log prefix (the `logIdKey`
/// follows from the tier; [`tier_line!`]).
#[derive(Debug, Clone, Copy)]
pub struct WardrobeRouteConfig {
    pub scope: WardrobeScope,
    /// Log-message prefix, e.g. `[Projects v1]`.
    pub log_tag: &'static str,
}

/// `characters/[id]/wardrobe` — `{ scope: 'character', logTag: '[Wardrobe v1]',
/// logIdKey: 'characterId' }`.
pub const CHARACTER_WARDROBE: WardrobeRouteConfig = WardrobeRouteConfig {
    scope: WardrobeScope::Character,
    log_tag: "[Wardrobe v1]",
};
/// `wardrobe` (Quilltap General) — `{ scope: 'general', logTag: '[Wardrobe
/// Archetypes v1]', logIdKey: 'generalId' }`.
pub const GENERAL_WARDROBE: WardrobeRouteConfig = WardrobeRouteConfig {
    scope: WardrobeScope::General,
    log_tag: "[Wardrobe Archetypes v1]",
};
/// `projects/[id]/wardrobe` — `{ scope: 'project', logTag: '[Projects v1]',
/// logIdKey: 'projectId' }`.
pub const PROJECT_WARDROBE: WardrobeRouteConfig = WardrobeRouteConfig {
    scope: WardrobeScope::Project,
    log_tag: "[Projects v1]",
};
/// `groups/[id]/wardrobe` — `{ scope: 'group', logTag: '[Groups v1]',
/// logIdKey: 'groupId' }`.
pub const GROUP_WARDROBE: WardrobeRouteConfig = WardrobeRouteConfig {
    scope: WardrobeScope::Group,
    log_tag: "[Groups v1]",
};

impl WardrobeRouteConfig {
    /// v4 `LABELS[scope].owner` — the 404 a missing owner answers.
    pub fn owner_label(&self) -> &'static str {
        match self.scope {
            WardrobeScope::Character => "Character",
            WardrobeScope::Group => "Group",
            WardrobeScope::Project => "Project",
            WardrobeScope::General => "Quilltap General",
        }
    }

    /// v4 `LABELS[scope].item` — the 404 a missing item answers.
    pub fn item_label(&self) -> &'static str {
        match self.scope {
            WardrobeScope::Character => "Wardrobe item",
            WardrobeScope::Group => "Group wardrobe item",
            WardrobeScope::Project => "Project wardrobe item",
            WardrobeScope::General => "Archetype wardrobe item",
        }
    }

    /// The tier's success envelope (each tier's handler answered its own
    /// `Response` variant before the factory; the body is the same).
    fn wrap(&self, body: Value) -> Response {
        match self.scope {
            WardrobeScope::Character => Response::Character(body),
            WardrobeScope::General => Response::Wardrobe(body),
            WardrobeScope::Project => Response::Project(body),
            WardrobeScope::Group => Response::Group(body),
        }
    }

    /// The meta v4 hands `cleanupEquippedRefs`: `{ [logIdKey]: location.id,
    /// itemId, context }`.
    fn route_meta<'a>(&self, id: &'a str) -> ItemRouteMeta<'a> {
        ItemRouteMeta::Location {
            scope: self.scope,
            id,
        }
    }
}

/// One factory line keyed by the tier's `logIdKey`, under the tier's route
/// target. `$id` is the rendered owner id (`"null"` for General); `$rest` are
/// v4's remaining keys in v4's order (each line ends `context = "wardrobe"`).
macro_rules! tier_line {
    ($level:ident, $cfg:expr, $id:expr, $message:expr; $($rest:tt)*) => {{
        let cfg: &WardrobeRouteConfig = &$cfg;
        let id: &str = $id;
        match cfg.scope {
            WardrobeScope::Character => tracing::$level!(
                target: "quilltap_core::api::characters",
                characterId = id, $($rest)* "{} {}", cfg.log_tag, $message
            ),
            WardrobeScope::General => tracing::$level!(
                target: "quilltap_core::api::wardrobe",
                generalId = id, $($rest)* "{} {}", cfg.log_tag, $message
            ),
            WardrobeScope::Project => tracing::$level!(
                target: "quilltap_core::api::projects",
                projectId = id, $($rest)* "{} {}", cfg.log_tag, $message
            ),
            WardrobeScope::Group => tracing::$level!(
                target: "quilltap_core::api::groups",
                groupId = id, $($rest)* "{} {}", cfg.log_tag, $message
            ),
        }
    }};
}

fn bad_request(msg: impl Into<String>) -> Response {
    Response::error(ErrorKind::BadRequest, msg.into())
}

fn not_found(resource: &str) -> Response {
    // v4 `notFound(resource)` → `{ error: '<resource> not found' }`.
    Response::error(ErrorKind::NotFound, format!("{resource} not found"))
}

/// v4's context middleware for anything a handler rethrows.
fn internal_server_error() -> Response {
    Response::error(ErrorKind::Internal, "Internal server error")
}

/// The flat 400 v4's middleware answers for an uncaught `ZodError` (the
/// `details` array is the standing project-wide deferral).
fn validation_error() -> Response {
    bad_request("Validation error")
}

/// v4 `location?.id ?? null` as a log value.
fn id_of(location: &WardrobeLocation) -> &str {
    location.id.as_deref().unwrap_or("null")
}

/// Run `f` on the read pool (main + mount-index).
fn read_main_mount<T>(
    db: &Db,
    f: impl FnOnce(&Connection, &Connection) -> Result<T, DbError>,
) -> Result<T, DbError> {
    db.read_main(|main| db.read_mount_index(|mount| f(main, mount)))
}

/// v4 `writeErrorResponse` — the write-side refusals every tier shares; any
/// other error is v4's rethrow into the middleware's generic 500.
pub fn write_error_response(error: WardrobeLocationError) -> Response {
    match error {
        WardrobeLocationError::Cycle(m) => bad_request(m),
        WardrobeLocationError::FolderPath(e) => bad_request(e.0),
        e @ WardrobeLocationError::ItemNotInLocation { .. } => bad_request(e.message()),
        // `notFound('Wardrobe folder')` — the error's own message is not used.
        WardrobeLocationError::FolderNotFound { .. } => not_found("Wardrobe folder"),
        WardrobeLocationError::FolderConflict { message, .. } => {
            Response::error(ErrorKind::Conflict, message)
        }
        WardrobeLocationError::CharacterArchived { .. } => Response::error(
            ErrorKind::Conflict,
            "An archived character's wardrobe cannot be changed",
        ),
        // A broken store is v4's contextful 503 (the middleware's mapping).
        WardrobeLocationError::Db(e @ DbError::StoreUnavailable { .. }) => db_error_response(e),
        WardrobeLocationError::NoLinkedVault { .. } | WardrobeLocationError::Db(_) => {
            internal_server_error()
        }
    }
}

/// v4 `makeResolver(config)(ctx, params, ensure)` — the tier's location, or
/// the response to answer: General unprovisioned → 500 `Quilltap General store
/// is not provisioned yet`; every other tier → 404 owner.
fn resolve_location(
    main: &Connection,
    mount: &Connection,
    cfg: &WardrobeRouteConfig,
    id: Option<&str>,
    user_id: &str,
    ensure: bool,
) -> Result<Result<WardrobeLocation, Response>, DbError> {
    Ok(
        match resolve_wardrobe_location(
            main,
            mount,
            cfg.scope,
            id,
            user_id,
            ResolveWardrobeLocationOptions { ensure },
        )? {
            Some(location) => Ok(location),
            None if cfg.scope == WardrobeScope::General => Err(Response::error(
                ErrorKind::Internal,
                "Quilltap General store is not provisioned yet",
            )),
            None => Err(not_found(cfg.owner_label())),
        },
    )
}

// ===========================================================================
// Serialization — `origin` then `wear` on EVERY item body
// ===========================================================================

fn nullable(v: &Option<Option<String>>) -> Value {
    match v {
        Some(Some(s)) => Value::String(s.clone()),
        _ => Value::Null,
    }
}

/// An item as v4's READ path builds it (`parseWardrobeItemFile`'s object
/// order — `vault-overlay/parsers.ts:445-462`): what a list, a GET, a PUT
/// (`{ ...before, ...patch }` keeps the read order) and `move-items` answer.
pub fn item_read_value(item: &WardrobeItem) -> Value {
    let mut m = Map::new();
    m.insert("id".into(), json!(item.id));
    m.insert("characterId".into(), nullable(&item.character_id));
    m.insert("title".into(), json!(item.title));
    m.insert("description".into(), nullable(&item.description));
    m.insert("imagePrompt".into(), nullable(&item.image_prompt));
    m.insert("types".into(), json!(item.types));
    m.insert("appropriateness".into(), nullable(&item.appropriateness));
    m.insert("isDefault".into(), json!(item.is_default));
    m.insert("replace".into(), json!(item.replace));
    m.insert("componentItemIds".into(), json!(item.component_item_ids));
    m.insert(
        "migratedFromClothingRecordId".into(),
        nullable(&item.migrated_from_clothing_record_id),
    );
    m.insert("imageFileId".into(), nullable(&item.image_file_id));
    m.insert("archivedAt".into(), nullable(&item.archived_at));
    m.insert("folder".into(), json!(item.folder));
    m.insert("createdAt".into(), json!(item.created_at));
    m.insert("updatedAt".into(), json!(item.updated_at));
    Value::Object(m)
}

/// An item as v4's `createItem` builds it (`{ ...wardrobeItemFromCreateBody(…),
/// id, archivedAt, imageFileId, migratedFromClothingRecordId, createdAt,
/// updatedAt }` — the spread's keys first, the identity after): what a create
/// answers.
pub fn item_create_value(item: &WardrobeItem) -> Value {
    let mut m = Map::new();
    m.insert("characterId".into(), nullable(&item.character_id));
    m.insert("title".into(), json!(item.title));
    m.insert("description".into(), nullable(&item.description));
    m.insert("imagePrompt".into(), nullable(&item.image_prompt));
    m.insert("types".into(), json!(item.types));
    m.insert("componentItemIds".into(), json!(item.component_item_ids));
    m.insert("appropriateness".into(), nullable(&item.appropriateness));
    m.insert("isDefault".into(), json!(item.is_default));
    m.insert("replace".into(), json!(item.replace));
    m.insert(
        "migratedFromClothingRecordId".into(),
        nullable(&item.migrated_from_clothing_record_id),
    );
    m.insert("imageFileId".into(), nullable(&item.image_file_id));
    m.insert("folder".into(), json!(item.folder));
    m.insert("id".into(), json!(item.id));
    m.insert("archivedAt".into(), nullable(&item.archived_at));
    m.insert("createdAt".into(), json!(item.created_at));
    m.insert("updatedAt".into(), json!(item.updated_at));
    Value::Object(m)
}

/// v4 `serializeWardrobeItems(items, origin, repos)` = `attachWear(withOrigin(…))`.
pub fn serialize_wardrobe_items(
    main: &Connection,
    items: Vec<Value>,
    origin: &WardrobeOrigin,
) -> Vec<Value> {
    attach_wear(main, with_origin(items, origin))
}

fn serialize_one(main: &Connection, item: Value, origin: &WardrobeOrigin) -> Value {
    serialize_wardrobe_items(main, vec![item], origin)
        .pop()
        .unwrap_or(Value::Null)
}

// ===========================================================================
// The Zod ports (`lib/schemas/wardrobe.types.ts` at `4a38222e6`)
// ===========================================================================

/// `wardrobeItemFieldsSchema` (+ `updateWardrobeSchema`'s `archived` /
/// `imageFileId` when `partial`). PASS/FAIL only — the routes do not catch the
/// `ZodError`. `z.object` strips unknown keys.
struct ItemFields {
    title: Option<String>,
    description: Option<Option<String>>,
    image_prompt: Option<Option<String>>,
    types: Option<Vec<String>>,
    appropriateness: Option<Option<String>>,
    is_default: Option<bool>,
    component_item_ids: Option<Vec<String>>,
    replace: Option<bool>,
    folder: Option<String>,
    archived: Option<bool>,
    image_file_id: Option<Option<String>>,
}

fn parse_item_fields(body: &Value, partial: bool) -> Result<ItemFields, ()> {
    let obj = body.as_object().ok_or(())?;
    let get = |k: &str| obj.get(k);

    // title: z.string().min(1, 'Title is required').
    let title = match get("title") {
        None if partial => None,
        Some(Value::String(s)) if crate::jsstr::zod_len_min_ok(s, 1) => Some(s.clone()),
        _ => return Err(()),
    };
    let nullable_str = |k: &str| -> Result<Option<Option<String>>, ()> {
        match get(k) {
            None => Ok(None),
            Some(Value::Null) => Ok(Some(None)),
            Some(Value::String(s)) => Ok(Some(Some(s.clone()))),
            _ => Err(()),
        }
    };
    let description = nullable_str("description")?;
    let image_prompt = nullable_str("imagePrompt")?;
    // types: z.array(WardrobeItemTypeEnum).min(1).
    let types = match get("types") {
        None if partial => None,
        Some(Value::Array(a)) if !a.is_empty() => {
            let mut out = Vec::with_capacity(a.len());
            for v in a {
                match v.as_str() {
                    Some(s) if crate::wardrobe::WARDROBE_SLOT_TYPES.contains(&s) => {
                        out.push(s.to_string())
                    }
                    _ => return Err(()),
                }
            }
            Some(out)
        }
        _ => return Err(()),
    };
    let appropriateness = nullable_str("appropriateness")?;
    let boolean = |k: &str| -> Result<Option<bool>, ()> {
        match get(k) {
            None => Ok(None),
            Some(Value::Bool(b)) => Ok(Some(*b)),
            _ => Err(()),
        }
    };
    let is_default = boolean("isDefault")?;
    // componentItemIds: z.array(z.string()).optional().
    let component_item_ids = match get("componentItemIds") {
        None => None,
        Some(Value::Array(a)) => {
            let mut out = Vec::with_capacity(a.len());
            for v in a {
                out.push(v.as_str().ok_or(())?.to_string());
            }
            Some(out)
        }
        _ => return Err(()),
    };
    let replace = boolean("replace")?;
    // folder: WardrobeFolderInputSchema.optional() = z.string().max(900).
    let folder = match get("folder") {
        None => None,
        Some(Value::String(s)) if crate::jsstr::zod_len_max_ok(s, 900) => Some(s.clone()),
        _ => return Err(()),
    };
    let (archived, image_file_id) = if partial {
        (boolean("archived")?, parse_image_file_id(body)?)
    } else {
        (None, None)
    };
    Ok(ItemFields {
        title,
        description,
        image_prompt,
        types,
        appropriateness,
        is_default,
        component_item_ids,
        replace,
        folder,
        archived,
        image_file_id,
    })
}

/// `WardrobeFolderInputSchema.min(1, …)` — a string of 1..=900 (UTF-16) units.
fn folder_input(obj: &Map<String, Value>, key: &str, min1: bool) -> Result<String, ()> {
    match obj.get(key) {
        Some(Value::String(s))
            if crate::jsstr::zod_len_max_ok(s, 900)
                && (!min1 || crate::jsstr::zod_len_min_ok(s, 1)) =>
        {
            Ok(s.clone())
        }
        _ => Err(()),
    }
}

/// `createWardrobeFolderSchema` / `deleteWardrobeFolderSchema` — `{ path }`.
fn parse_path_body(body: &Value) -> Result<String, ()> {
    folder_input(body.as_object().ok_or(())?, "path", true)
}

/// `renameWardrobeFolderSchema` — `{ path, name: z.string().min(1).max(200) }`.
fn parse_rename_body(body: &Value) -> Result<(String, String), ()> {
    let obj = body.as_object().ok_or(())?;
    let path = folder_input(obj, "path", true)?;
    let name = match obj.get("name") {
        Some(Value::String(s))
            if crate::jsstr::zod_len_min_ok(s, 1) && crate::jsstr::zod_len_max_ok(s, 200) =>
        {
            s.clone()
        }
        _ => return Err(()),
    };
    Ok((path, name))
}

/// `moveWardrobeItemsSchema` — `{ itemIds: z.array(z.string().min(1)).min(1).max(500),
/// folder: WardrobeFolderInputSchema }`.
fn parse_move_body(body: &Value) -> Result<(Vec<String>, String), ()> {
    let obj = body.as_object().ok_or(())?;
    let Some(Value::Array(ids)) = obj.get("itemIds") else {
        return Err(());
    };
    if ids.is_empty() || ids.len() > 500 {
        return Err(());
    }
    let mut item_ids = Vec::with_capacity(ids.len());
    for id in ids {
        match id {
            Value::String(s) if crate::jsstr::zod_len_min_ok(s, 1) => item_ids.push(s.clone()),
            _ => return Err(()),
        }
    }
    let folder = folder_input(obj, "folder", false)?;
    Ok((item_ids, folder))
}

// ===========================================================================
// Collection GET — list (+ the character tier's `?scope=group`)
// ===========================================================================

/// What a list read answers, and the one line it logs (on the caller thread).
enum Listed {
    Items {
        location: WardrobeLocation,
        items: Vec<Value>,
    },
    GroupTier {
        character_id: String,
        group_count: usize,
        group_mount_count: usize,
        items: Vec<Value>,
    },
}

fn list_in(
    main: &Connection,
    mount: &Connection,
    cfg: &WardrobeRouteConfig,
    id: Option<&str>,
    user_id: &str,
    group_scope: bool,
    include_archived: bool,
) -> Result<Result<Listed, Response>, DbError> {
    let Some(location) = resolve_wardrobe_location(
        main,
        mount,
        cfg.scope,
        id,
        user_id,
        ResolveWardrobeLocationOptions {
            ensure: cfg.scope != WardrobeScope::Character,
        },
    )?
    else {
        // An unprovisioned General simply has nothing in it yet.
        if cfg.scope == WardrobeScope::General {
            return Ok(Err(cfg.wrap(json!({ "wardrobeItems": [] }))));
        }
        return Ok(Err(not_found(cfg.owner_label())));
    };
    if cfg.scope == WardrobeScope::Character && group_scope {
        // v4 `listGroupTier`: the group tier, each item tagged with its group.
        // `originByMount` is a Map — a mount keeps its FIRST position, a later
        // group overwrites its origin.
        let character_id = location.character_id.clone().unwrap_or_default();
        let groups = resolve_group_mounts_for_character(main, mount, &character_id);
        let mut mounts: Vec<String> = Vec::new();
        let mut origin_by_mount: std::collections::HashMap<String, WardrobeOrigin> =
            std::collections::HashMap::new();
        for g in &groups {
            for mp in &g.mount_point_ids {
                if !origin_by_mount.contains_key(mp) {
                    mounts.push(mp.clone());
                }
                origin_by_mount.insert(
                    mp.clone(),
                    WardrobeOrigin::new(WardrobeOriginScope::Group, &g.group.id, &g.group.name),
                );
            }
        }
        let docs = DocMountDocumentsRepository::new(mount);
        let items = read_shared_tiers(&docs, &mounts, include_archived, &|mp: &str| {
            origin_by_mount
                .get(mp)
                .cloned()
                .expect("every read mount has an origin")
        });
        let items = attach_wear(main, items);
        return Ok(Ok(Listed::GroupTier {
            character_id,
            group_count: groups.len(),
            group_mount_count: origin_by_mount.len(),
            items,
        }));
    }
    let items: Vec<Value> = location
        .read_items(mount, include_archived)?
        .iter()
        .map(item_read_value)
        .collect();
    let items = serialize_wardrobe_items(main, items, &location.origin);
    Ok(Ok(Listed::Items { location, items }))
}

fn listed_response(cfg: &WardrobeRouteConfig, listed: Listed) -> Response {
    match listed {
        Listed::GroupTier {
            character_id,
            group_count,
            group_mount_count,
            items,
        } => {
            tracing::debug!(
                target: "quilltap_core::api::characters",
                characterId = character_id.as_str(),
                groupCount = group_count,
                groupMountCount = group_mount_count,
                itemCount = items.len(),
                context = "wardrobe",
                "{} Group-tier wardrobe read",
                cfg.log_tag
            );
            cfg.wrap(json!({ "wardrobeItems": items }))
        }
        Listed::Items { location, items } => {
            tier_line!(debug, cfg, id_of(&location), "Listed wardrobe items";
                mountPointId = location.mount_point_id.as_str(),
                count = items.len(),
                context = "wardrobe",
            );
            cfg.wrap(json!({
                "mountPointId": location.mount_point_id,
                "wardrobeItems": items,
            }))
        }
    }
}

/// The character tier's collection GET — a READ (`ensure: false`).
/// `scope = Some("group")` serves the group tier; any other value falls
/// through to the vault list, as v4's `=== 'group'` check does.
pub fn list_character_items(
    db: &Db,
    user_id: &str,
    character_id: &str,
    scope: Option<&str>,
    include_archived: bool,
) -> Response {
    let cfg = CHARACTER_WARDROBE;
    let (cid, uid, group_scope) = (
        character_id.to_string(),
        user_id.to_string(),
        scope == Some("group"),
    );
    match read_main_mount(db, move |main, mount| {
        list_in(
            main,
            mount,
            &cfg,
            Some(&cid),
            &uid,
            group_scope,
            include_archived,
        )
    }) {
        Ok(Ok(listed)) => listed_response(&cfg, listed),
        Ok(Err(r)) => r,
        Err(e) => db_error_response(e),
    }
}

/// A store tier's collection GET (General / project / group) — `ensure:
/// true`, so it runs on the writer.
pub async fn list_store_items(
    db: &Db,
    cfg: WardrobeRouteConfig,
    id: Option<&str>,
    user_id: &str,
    include_archived: bool,
) -> Response {
    let (id, uid) = (id.map(str::to_string), user_id.to_string());
    match with_both_conns(db, move |main, mount| {
        list_in(
            main,
            mount,
            &cfg,
            id.as_deref(),
            &uid,
            false,
            include_archived,
        )
    })
    .await
    {
        Ok(Ok(listed)) => listed_response(&cfg, listed),
        Ok(Err(r)) => r,
        Err(e) => db_error_response(e),
    }
}

// ===========================================================================
// `?action=instructions`
// ===========================================================================

/// The location an instructions read resolved (General may have none) and
/// the file it read.
type InstructionsRead = (Option<WardrobeLocation>, Option<String>);

fn instructions_get_in(
    main: &Connection,
    mount: &Connection,
    cfg: &WardrobeRouteConfig,
    id: Option<&str>,
    user_id: &str,
) -> Result<Result<InstructionsRead, Response>, DbError> {
    let location = resolve_wardrobe_location(
        main,
        mount,
        cfg.scope,
        id,
        user_id,
        ResolveWardrobeLocationOptions {
            ensure: cfg.scope != WardrobeScope::Character,
        },
    )?;
    if location.is_none() && cfg.scope != WardrobeScope::General {
        return Ok(Err(not_found(cfg.owner_label())));
    }
    let instructions = location
        .as_ref()
        .and_then(|l| read_wardrobe_instructions_file(mount, &l.mount_point_id));
    Ok(Ok((location, instructions)))
}

fn instructions_read_response(
    cfg: &WardrobeRouteConfig,
    location: Option<WardrobeLocation>,
    instructions: Option<String>,
) -> Response {
    tier_line!(debug, cfg, location.as_ref().map(id_of).unwrap_or("null"), "Read dressing instructions";
        mountPointId = location.as_ref().map(|l| l.mount_point_id.as_str()).unwrap_or("null"),
        present = instructions.is_some(),
        context = "wardrobe",
    );
    cfg.wrap(json!({ "instructions": instructions }))
}

/// The character tier's `GET ?action=instructions` — a READ. An archived
/// character's instructions still answer (the location reads a tombstone).
pub fn instructions_get_character(db: &Db, user_id: &str, character_id: &str) -> Response {
    let cfg = CHARACTER_WARDROBE;
    let (cid, uid) = (character_id.to_string(), user_id.to_string());
    match read_main_mount(db, move |main, mount| {
        instructions_get_in(main, mount, &cfg, Some(&cid), &uid)
    }) {
        Ok(Ok((location, instructions))) => {
            instructions_read_response(&cfg, location, instructions)
        }
        Ok(Err(r)) => r,
        Err(e) => db_error_response(e),
    }
}

/// A store tier's `GET ?action=instructions` (`ensure: true` — the writer).
pub async fn instructions_get_store(
    db: &Db,
    cfg: WardrobeRouteConfig,
    id: Option<&str>,
    user_id: &str,
) -> Response {
    let (id, uid) = (id.map(str::to_string), user_id.to_string());
    match with_both_conns(db, move |main, mount| {
        instructions_get_in(main, mount, &cfg, id.as_deref(), &uid)
    })
    .await
    {
        Ok(Ok((location, instructions))) => {
            instructions_read_response(&cfg, location, instructions)
        }
        Ok(Err(r)) => r,
        Err(e) => db_error_response(e),
    }
}

/// `POST ?action=instructions` — parse FIRST, then resolve (`ensure: true`);
/// clearing what can't exist yet is a harmless no-op on General only; an
/// archived character is the route's OWN 409 sentence.
pub async fn instructions_set(
    db: &Db,
    cfg: WardrobeRouteConfig,
    id: Option<&str>,
    user_id: &str,
    instructions: &Option<Option<Value>>,
) -> Response {
    let instructions = match super::wardrobe::parse_instructions_body(instructions) {
        Ok(v) => v,
        Err(r) => return r,
    };
    let (cleared, echo) = super::wardrobe::instructions_echo(instructions.as_deref());
    let (id, uid) = (id.map(str::to_string), user_id.to_string());
    let out = with_both_conns(db, move |main, mount| {
        let location = match resolve_location(main, mount, &cfg, id.as_deref(), &uid, true)? {
            Ok(l) => l,
            Err(r) => {
                if cleared && cfg.scope == WardrobeScope::General {
                    return Ok(Err(cfg.wrap(json!({ "instructions": null }))));
                }
                return Ok(Err(r));
            }
        };
        let mount_point_id = match location.writable_mount_point_id(main) {
            Ok(mp) => mp,
            Err(WardrobeLocationError::CharacterArchived { .. }) => {
                return Ok(Err(Response::error(
                    ErrorKind::Conflict,
                    "Character is archived; dressing instructions cannot be edited",
                )))
            }
            Err(WardrobeLocationError::Db(e)) => return Err(e),
            Err(_) => return Ok(Err(internal_server_error())),
        };
        let links = DocMountFileLinksRepository::new(mount);
        write_wardrobe_instructions_file(&links, &mount_point_id, instructions.as_deref())?;
        Ok(Ok((location, mount_point_id)))
    })
    .await;
    match out {
        Ok(Ok((location, mount_point_id))) => {
            tier_line!(info, cfg, id_of(&location), "Dressing instructions updated";
                userId = user_id,
                mountPointId = mount_point_id.as_str(),
                cleared = cleared,
                context = "wardrobe",
            );
            cfg.wrap(json!({ "instructions": echo }))
        }
        Ok(Err(r)) => r,
        Err(e) => db_error_response(e),
    }
}

// ===========================================================================
// Folders — the operator's filing under this tier's `Wardrobe/`
// ===========================================================================

/// `GET ?action=folders` — a read probe that NEVER provisions: a project or
/// group with no store yet (or an unprovisioned General) simply has no
/// folders. Answers `Response::Wardrobe` (C2).
pub fn folders_list(
    db: &Db,
    cfg: WardrobeRouteConfig,
    id: Option<&str>,
    user_id: &str,
) -> Response {
    enum Out {
        Listed(
            WardrobeLocation,
            Vec<crate::wardrobe_folders::WardrobeFolder>,
        ),
        NoStore(String),
        Respond(Response),
    }
    let (id, uid) = (id.map(str::to_string), user_id.to_string());
    let out = read_main_mount(db, move |main, mount| {
        let Some(location) = resolve_wardrobe_location(
            main,
            mount,
            cfg.scope,
            id.as_deref(),
            &uid,
            ResolveWardrobeLocationOptions { ensure: false },
        )?
        else {
            if cfg.scope == WardrobeScope::General {
                return Ok(Out::Respond(Response::Wardrobe(json!({ "folders": [] }))));
            }
            if let Some(id) = id.as_deref().filter(|s| !s.is_empty()) {
                let owner_exists = match cfg.scope {
                    WardrobeScope::Project => ProjectsRepository::new(main, mount)
                        .find_by_id(id)
                        .map_err(|e| e.into_db())?
                        .is_some(),
                    WardrobeScope::Group => GroupsRepository::new(main, mount)
                        .find_by_id(id)
                        .map_err(|e| e.into_db())?
                        .is_some(),
                    _ => false,
                };
                if owner_exists {
                    return Ok(Out::NoStore(id.to_string()));
                }
            }
            return Ok(Out::Respond(not_found(cfg.owner_label())));
        };
        let folders = location.list_folders(mount)?;
        Ok(Out::Listed(location, folders))
    });
    match out {
        Ok(Out::Listed(location, folders)) => {
            tier_line!(debug, cfg, id_of(&location), "Listed wardrobe folders";
                mountPointId = location.mount_point_id.as_str(),
                count = folders.len(),
                context = "wardrobe",
            );
            Response::Wardrobe(json!({ "folders": folders }))
        }
        Ok(Out::NoStore(id)) => {
            tier_line!(debug, cfg, id.as_str(), "No store yet; no wardrobe folders";
                context = "wardrobe",
            );
            Response::Wardrobe(json!({ "folders": [] }))
        }
        Ok(Out::Respond(r)) => r,
        Err(e) => db_error_response(e),
    }
}

/// The shape every folder write shares: parse (before resolve) → resolve →
/// the mutation (its refusals through [`write_error_response`]).
async fn folder_write<T: Send + 'static>(
    db: &Db,
    cfg: WardrobeRouteConfig,
    id: Option<&str>,
    user_id: &str,
    ensure: bool,
    op: impl FnOnce(&Connection, &Connection, &WardrobeLocation) -> Result<T, WardrobeLocationError>
        + Send
        + 'static,
) -> Result<Result<(WardrobeLocation, T), Response>, DbError> {
    let (id, uid) = (id.map(str::to_string), user_id.to_string());
    with_both_conns(db, move |main, mount| {
        let location = match resolve_location(main, mount, &cfg, id.as_deref(), &uid, ensure)? {
            Ok(l) => l,
            Err(r) => return Ok(Err(r)),
        };
        match op(main, mount, &location) {
            Ok(v) => Ok(Ok((location, v))),
            Err(e) => Ok(Err(write_error_response(e))),
        }
    })
    .await
}

/// `POST ?action=create-folder` `{ path }` → `{ folder }` (`ensure: true`).
pub async fn folder_create(
    db: &Db,
    cfg: WardrobeRouteConfig,
    id: Option<&str>,
    user_id: &str,
    body: &Value,
) -> Response {
    let Ok(path) = parse_path_body(body) else {
        return validation_error();
    };
    match folder_write(db, cfg, id, user_id, true, move |main, mount, loc| {
        create_wardrobe_folder(main, mount, loc, &path)
    })
    .await
    {
        Ok(Ok((location, folder))) => {
            tier_line!(info, cfg, id_of(&location), "Created wardrobe folder";
                userId = user_id,
                mountPointId = location.mount_point_id.as_str(),
                folder = folder.path.as_str(),
                context = "wardrobe",
            );
            Response::Wardrobe(json!({ "folder": folder }))
        }
        Ok(Err(r)) => r,
        Err(e) => db_error_response(e),
    }
}

/// `POST ?action=rename-folder` `{ path, name }` → `{ folder }`.
pub async fn folder_rename(
    db: &Db,
    cfg: WardrobeRouteConfig,
    id: Option<&str>,
    user_id: &str,
    body: &Value,
) -> Response {
    let Ok((path, name)) = parse_rename_body(body) else {
        return validation_error();
    };
    let from = path.clone();
    match folder_write(db, cfg, id, user_id, false, move |main, mount, loc| {
        rename_wardrobe_folder(main, mount, loc, &path, &name)
    })
    .await
    {
        Ok(Ok((location, folder))) => {
            tier_line!(info, cfg, id_of(&location), "Renamed wardrobe folder";
                userId = user_id,
                mountPointId = location.mount_point_id.as_str(),
                from = from.as_str(),
                to = folder.path.as_str(),
                context = "wardrobe",
            );
            Response::Wardrobe(json!({ "folder": folder }))
        }
        Ok(Err(r)) => r,
        Err(e) => db_error_response(e),
    }
}

/// `POST ?action=delete-folder` `{ path }` → `{ movedItems, movedFolders }`
/// (the parent is the mutation's to log, not the route's to answer).
pub async fn folder_delete(
    db: &Db,
    cfg: WardrobeRouteConfig,
    id: Option<&str>,
    user_id: &str,
    body: &Value,
) -> Response {
    let Ok(path) = parse_path_body(body) else {
        return validation_error();
    };
    let requested = path.clone();
    match folder_write(db, cfg, id, user_id, false, move |main, mount, loc| {
        delete_wardrobe_folder(main, mount, loc, &path)
    })
    .await
    {
        Ok(Ok((location, (moved_items, moved_folders)))) => {
            tier_line!(info, cfg, id_of(&location), "Deleted wardrobe folder";
                userId = user_id,
                mountPointId = location.mount_point_id.as_str(),
                folder = requested.as_str(),
                movedItems = moved_items,
                movedFolders = moved_folders,
                context = "wardrobe",
            );
            Response::Wardrobe(json!({
                "movedItems": moved_items,
                "movedFolders": moved_folders,
            }))
        }
        Ok(Err(r)) => r,
        Err(e) => db_error_response(e),
    }
}

/// `POST ?action=move-items` `{ itemIds, folder }` → `{ items }` serialized.
/// The INFO logs the folder AS REQUESTED (v4 `folder: body.folder`) where
/// `create-folder` logs the resolved path — v4's asymmetry, ported as written
/// (R-I).
pub async fn items_move(
    db: &Db,
    cfg: WardrobeRouteConfig,
    id: Option<&str>,
    user_id: &str,
    body: &Value,
) -> Response {
    let Ok((item_ids, folder)) = parse_move_body(body) else {
        return validation_error();
    };
    let requested = folder.clone();
    let out = folder_write(db, cfg, id, user_id, false, move |main, mount, loc| {
        let moved = move_items_to_folder(main, mount, loc, &item_ids, &folder)?;
        let items: Vec<Value> = moved.iter().map(item_read_value).collect();
        Ok((
            moved.len(),
            serialize_wardrobe_items(main, items, &loc.origin),
        ))
    })
    .await;
    match out {
        Ok(Ok((location, (count, items)))) => {
            tier_line!(info, cfg, id_of(&location), "Moved wardrobe items to folder";
                userId = user_id,
                mountPointId = location.mount_point_id.as_str(),
                folder = requested.as_str(),
                count = count,
                context = "wardrobe",
            );
            Response::Wardrobe(json!({ "items": items }))
        }
        Ok(Err(r)) => r,
        Err(e) => db_error_response(e),
    }
}

// ===========================================================================
// Collection POST — create
// ===========================================================================

/// The collection POST: parse BEFORE resolving (an invalid body never
/// provisions a store) → resolve (`ensure: true`) → `createItem` → 201
/// `{ wardrobeItem }` serialized.
pub async fn create(
    db: &Db,
    cfg: WardrobeRouteConfig,
    id: Option<&str>,
    user_id: &str,
    body: &Value,
) -> Response {
    let Ok(fields) = parse_item_fields(body, false) else {
        return validation_error();
    };
    let create_body = WardrobeCreateBody {
        title: fields.title.unwrap_or_default(),
        description: fields.description.flatten(),
        image_prompt: fields.image_prompt.flatten(),
        types: fields.types.unwrap_or_default(),
        appropriateness: fields.appropriateness.flatten(),
        is_default: fields.is_default,
        component_item_ids: fields.component_item_ids,
        replace: fields.replace,
        folder: fields.folder,
    };
    let (id, uid) = (id.map(str::to_string), user_id.to_string());
    let out = with_both_conns(db, move |main, mount| {
        let location = match resolve_location(main, mount, &cfg, id.as_deref(), &uid, true)? {
            Ok(l) => l,
            Err(r) => return Ok(Err(r)),
        };
        let has_components = create_body
            .component_item_ids
            .as_ref()
            .is_some_and(|c| !c.is_empty());
        let lookup = if has_components {
            Some(component_lookup_for(main, mount, &location, &[])?)
        } else {
            None
        };
        let stored = match create_item(
            main,
            mount,
            &location,
            &create_body,
            CreateItemOptions {
                lookup: lookup.as_ref(),
                preserve: None,
            },
        ) {
            Ok(s) => s,
            Err(e) => return Ok(Err(write_error_response(e))),
        };
        let echo = serialize_one(main, item_create_value(&stored), &location.origin);
        Ok(Ok((location, stored, echo)))
    })
    .await;
    match out {
        Ok(Ok((location, stored, echo))) => {
            tier_line!(info, cfg, id_of(&location), "Created wardrobe item";
                userId = user_id,
                mountPointId = location.mount_point_id.as_str(),
                itemId = stored.id.as_str(),
                title = stored.title.as_str(),
                context = "wardrobe",
            );
            cfg.wrap(json!({ "wardrobeItem": echo }))
        }
        Ok(Err(r)) => r,
        Err(e) => db_error_response(e),
    }
}

// ===========================================================================
// Item — GET (with `?action=wear-history`) / PUT / DELETE
// ===========================================================================

/// v4 `findItem` — the tier's location (never provisioned) and the item in it
/// (archived included), or the 404 to answer.
fn find_item(
    main: &Connection,
    mount: &Connection,
    cfg: &WardrobeRouteConfig,
    id: Option<&str>,
    user_id: &str,
    item_id: &str,
) -> Result<Result<(WardrobeLocation, WardrobeItem), Response>, DbError> {
    let location = match resolve_location(main, mount, cfg, id, user_id, false)? {
        Ok(l) => l,
        Err(r) => return Ok(Err(r)),
    };
    match location.find_item(mount, item_id)? {
        Some(item) => Ok(Ok((location, item))),
        None => Ok(Err(not_found(cfg.item_label()))),
    }
}

/// The item GET → `{ wardrobeItem }` serialized.
pub fn item_get(
    db: &Db,
    cfg: WardrobeRouteConfig,
    id: Option<&str>,
    user_id: &str,
    item_id: &str,
) -> Response {
    let (id, uid, iid) = (
        id.map(str::to_string),
        user_id.to_string(),
        item_id.to_string(),
    );
    match read_main_mount(db, move |main, mount| {
        let (location, item) = match find_item(main, mount, &cfg, id.as_deref(), &uid, &iid)? {
            Ok(found) => found,
            Err(r) => return Ok(Err(r)),
        };
        Ok(Ok(serialize_one(
            main,
            item_read_value(&item),
            &location.origin,
        )))
    }) {
        Ok(Ok(item)) => cfg.wrap(json!({ "wardrobeItem": item })),
        Ok(Err(r)) => r,
        Err(e) => db_error_response(e),
    }
}

/// `GET …/[itemId]?action=wear-history` — `findItem` first (the 404s run
/// before the ledger read), then `{ history, wearers, lastWornChat }`.
pub fn item_wear_history(
    db: &Db,
    cfg: WardrobeRouteConfig,
    id: Option<&str>,
    user_id: &str,
    item_id: &str,
) -> Response {
    let (id, uid, iid) = (
        id.map(str::to_string),
        user_id.to_string(),
        item_id.to_string(),
    );
    match read_main_mount(db, move |main, mount| {
        let (location, _) = match find_item(main, mount, &cfg, id.as_deref(), &uid, &iid)? {
            Ok(found) => found,
            Err(r) => return Ok(Err(r)),
        };
        Ok(Ok((
            location,
            build_wear_history_payload(main, mount, &iid),
        )))
    }) {
        Ok(Ok((location, payload))) => {
            tier_line!(debug, cfg, id_of(&location), "Read wardrobe item wear history";
                itemId = item_id,
                wearCount = payload["history"]["wearCount"].as_i64().unwrap_or(0),
                context = "wardrobe",
            );
            Response::WardrobeWearHistory(payload)
        }
        Ok(Err(r)) => r,
        Err(e) => db_error_response(e),
    }
}

/// The item PUT: `findItem` (404) → `updateWardrobeSchema` (uncaught →
/// `Validation error`) → `imageChoiceError` (400) → `updateItem` → `{
/// wardrobeItem }` serialized. The 404 runs BEFORE the picture check.
pub async fn item_update(
    db: &Db,
    cfg: WardrobeRouteConfig,
    id: Option<&str>,
    user_id: &str,
    item_id: &str,
    body: Value,
) -> Response {
    let (id, uid, iid) = (
        id.map(str::to_string),
        user_id.to_string(),
        item_id.to_string(),
    );
    let out = with_both_conns(db, move |main, mount| {
        let (location, current) = match find_item(main, mount, &cfg, id.as_deref(), &uid, &iid)? {
            Ok(found) => found,
            Err(r) => return Ok(Err(r)),
        };
        let Ok(fields) = parse_item_fields(&body, true) else {
            return Ok(Err(validation_error()));
        };
        match image_choice_error(
            main,
            &current.id,
            fields.image_file_id.as_ref().and_then(|o| o.as_deref()),
        ) {
            Ok(None) => {}
            Ok(Some(message)) => return Ok(Err(bad_request(message))),
            Err(_) => return Ok(Err(internal_server_error())),
        }
        // Only a composite whose composition is being edited needs its parts.
        let composite = !fields
            .component_item_ids
            .as_deref()
            .unwrap_or(&current.component_item_ids)
            .is_empty();
        let touches_composition = fields.component_item_ids.is_some() || fields.types.is_some();
        let lookup = if composite && touches_composition {
            Some(component_lookup_for(main, mount, &location, &[])?)
        } else {
            None
        };
        let archived = fields.archived;
        let patch = ItemPatch {
            title: fields.title,
            types: fields.types,
            component_item_ids: fields.component_item_ids,
            description: fields.description,
            image_prompt: fields.image_prompt,
            appropriateness: fields.appropriateness,
            is_default: fields.is_default,
            replace: fields.replace,
            image_file_id: fields.image_file_id,
            migrated_from_clothing_record_id: None,
            folder: fields.folder,
            archived,
        };
        let item = match update_item(main, mount, &location, &current, &patch, lookup.as_ref()) {
            Ok(Some(item)) => item,
            Ok(None) => return Ok(Err(not_found(cfg.item_label()))),
            Err(e) => return Ok(Err(write_error_response(e))),
        };
        let echo = serialize_one(main, item_read_value(&item), &location.origin);
        Ok(Ok((location, item, archived, echo)))
    })
    .await;
    match out {
        Ok(Ok((location, item, archived, echo))) => {
            let mp = location.mount_point_id.as_str();
            if archived.is_some() {
                let archived_at = item
                    .archived_at
                    .as_ref()
                    .and_then(|a| a.as_deref())
                    .unwrap_or("null");
                tier_line!(info, cfg, id_of(&location), "Updated wardrobe item";
                    userId = user_id,
                    mountPointId = mp,
                    itemId = item.id.as_str(),
                    context = "wardrobe",
                    archivedAt = archived_at,
                );
            } else {
                tier_line!(info, cfg, id_of(&location), "Updated wardrobe item";
                    userId = user_id,
                    mountPointId = mp,
                    itemId = item.id.as_str(),
                    context = "wardrobe",
                );
            }
            cfg.wrap(json!({ "wardrobeItem": echo }))
        }
        Ok(Err(r)) => r,
        Err(e) => db_error_response(e),
    }
}

/// The item DELETE: `findItem` (404) → the writable gate FIRST (an archived
/// character's vault is refused before any chat is touched) →
/// `cleanupEquippedRefs` → delete (404 when it vanished) → the item's
/// pictures → `{ success: true }`.
///
/// Two writes, as v4's separate awaits: the clean-up's outcome is logged on
/// the CALLING thread between them.
pub async fn item_delete(
    db: &Db,
    cfg: WardrobeRouteConfig,
    id: Option<&str>,
    user_id: &str,
    item_id: &str,
) -> Response {
    let (id, uid, iid) = (
        id.map(str::to_string),
        user_id.to_string(),
        item_id.to_string(),
    );
    let cleaned = with_both_conns(db, {
        let iid = iid.clone();
        move |main, mount| {
            let (location, _) = match find_item(main, mount, &cfg, id.as_deref(), &uid, &iid)? {
                Ok(found) => found,
                Err(r) => return Ok(Err(r)),
            };
            if let Err(e) = location.writable_mount_point_id(main) {
                return Ok(Err(write_error_response(e)));
            }
            Ok(Ok((location, run_cleanup_equipped_refs(main, &iid))))
        }
    })
    .await;
    let location = match cleaned {
        Ok(Ok((location, outcome))) => {
            log_cleanup_equipped_refs(
                &outcome,
                &iid,
                cfg.log_tag,
                cfg.route_meta(id_of(&location)),
            );
            location
        }
        Ok(Err(r)) => return r,
        Err(e) => return db_error_response(e),
    };

    let out = with_both_conns(db, {
        let (location, iid) = (location.clone(), iid.clone());
        move |main, mount| match location.delete(main, mount, &iid) {
            Ok(true) => {
                cleanup_item_images(
                    main,
                    mount,
                    &iid,
                    cfg.log_tag,
                    ItemImageCleanupMeta::Location {
                        scope: cfg.scope,
                        id: id_of(&location),
                        mount_point_id: &location.mount_point_id,
                    },
                );
                Ok(Ok(()))
            }
            Ok(false) => Ok(Err(not_found(cfg.item_label()))),
            Err(e) => Ok(Err(write_error_response(e))),
        }
    })
    .await;
    match out {
        Ok(Ok(())) => {
            tier_line!(info, cfg, id_of(&location), "Deleted wardrobe item";
                itemId = item_id,
                context = "wardrobe",
                mountPointId = location.mount_point_id.as_str(),
            );
            cfg.wrap(json!({ "success": true }))
        }
        Ok(Err(r)) => r,
        Err(e) => db_error_response(e),
    }
}
