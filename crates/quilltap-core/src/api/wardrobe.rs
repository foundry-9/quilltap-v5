//! The wardrobe-server dispatch handlers (P4.9f1) — the GLOBAL archetype CRUD
//! tier (v4 `app/api/v1/wardrobe/route.ts` + `[itemId]/route.ts`), the
//! wardrobe-transfers pair (v4 `app/api/v1/wardrobe/transfers/route.ts`), the
//! synchronous avatar preview (v4 `app/api/v1/wardrobe/preview-avatar/route.ts`),
//! and the `analyze-image` refusal arm.
//!
//! Each handler is a differential port of a v4 route handler (the oracle) and
//! returns a [`Response`] directly (the engine arm is a one-line delegate); the
//! exact bytes are pinned by `wardrobe_routes_equivalence` (quilltap-web tests).
//! `user_id` is a parameter (not hard-coded `SINGLE_USER_ID`) so the harness can
//! drive with the fixture's own user id; the engine passes `SINGLE_USER_ID`.
//!
//! ## The two validation regimes (v4's, preserved)
//!
//! - The **archetype create/update** routes do NOT catch `ZodError` — a parse
//!   failure propagates to v4's middleware and surfaces as the flat 400
//!   `{error: 'Validation error'}` (`validationError`). Only the PASS/FAIL
//!   decision must match; the `details` issue array is the standing
//!   project-wide deferral (the P4.6ay-unit-12 precedent).
//! - The **transfers POST** catches `ZodError` itself and joins the issue
//!   messages with `'; '` — those message BYTES are wire payload, ported in
//!   [`parse_transfer_request`].
//!
//! ## Tiers
//!
//! The global tier reads/writes Quilltap General ONLY (`findArchetypes()` with
//! no project mounts — the route has no project context); the transfers pair
//! spans all four tiers through the already-ported
//! [`crate::services::wardrobe_transfers`] service.

use serde_json::{json, Value};

use crate::db::archetype_wardrobe;
use crate::db::chats_outfits::ChatOutfitsRepository;
use crate::db::doc_mount_documents::DocMountDocumentsRepository;
use crate::db::doc_mount_file_links::DocMountFileLinksRepository;
use crate::db::runtime::Db;
use crate::db::vault_wardrobe_public::{
    create_vault_wardrobe_item, delete_vault_wardrobe_item, update_vault_wardrobe_item,
    WardrobePatch,
};
use crate::db::DbError;
use crate::services::image_job_common::with_both_conns;
use crate::services::wardrobe_transfers::{
    self, DestinationScope, TransferAction, TransferError, TransferRequest,
};
use crate::vault_overlay::WardrobeItem;
use crate::wardrobe_tiers::SharedWardrobeTiers;

use super::types::{ErrorKind, Response};

/// The four coverage slots (v4 `WardrobeItemTypeEnum` / `WARDROBE_SLOT_TYPES`).
const WARDROBE_TYPES: [&str; 4] = ["top", "bottom", "footwear", "accessories"];

fn internal(e: impl std::fmt::Display) -> Response {
    Response::error(ErrorKind::Internal, e.to_string())
}

fn bad_request(msg: impl Into<String>) -> Response {
    Response::error(ErrorKind::BadRequest, msg.into())
}

fn not_found(resource: &str) -> Response {
    // v4 `notFound(resource)` → `{error: '<resource> not found'}`.
    Response::error(ErrorKind::NotFound, format!("{resource} not found"))
}

/// Read helper: run `f` with BOTH a main and a mount-index connection (the vault
/// overlay needs both) — the salon.rs nested-read idiom (separate pools → no
/// deadlock).
fn read_main_mount<T>(
    db: &Db,
    f: impl FnOnce(&rusqlite::Connection, &rusqlite::Connection) -> Result<T, DbError>,
) -> Result<T, DbError> {
    db.read_main(|main| db.read_mount_index(|mount| f(main, mount)))
}

// ===========================================================================
// The archetype body parse (v4 create/updateArchetypeSchema — PASS/FAIL only)
// ===========================================================================

/// The parsed create/update body. For an update every field is optional
/// (`updateArchetypeSchema = the same shape, all keys optional`).
struct ArchetypeBody {
    title: Option<String>,
    description: Option<Option<String>>,
    image_prompt: Option<Option<String>>,
    types: Option<Vec<String>>,
    appropriateness: Option<Option<String>>,
    is_default: Option<bool>,
    component_item_ids: Option<Vec<String>>,
    replace: Option<bool>,
}

/// Zod PASS/FAIL port of v4 `createArchetypeSchema` (`require_all = true`) and
/// `updateArchetypeSchema` (`require_all = false`). The routes do not catch the
/// ZodError, so a failure is the flat middleware `Validation error` — no issue
/// messages to port, only the decision:
///
/// - `title`: string, length ≥ 1 (required on create);
/// - `description`/`imagePrompt`/`appropriateness`: string | null | absent
///   (`.nullable().optional()` — explicit null allowed);
/// - `types`: array of the four slot enums, length ≥ 1 (required on create);
/// - `isDefault`/`replace`: boolean | absent (`.optional()` — null FAILS);
/// - `componentItemIds`: array of strings | absent (null FAILS).
///
/// Unknown keys are stripped (Zod's default object mode).
fn parse_archetype_body(body: &Value, require_all: bool) -> Result<ArchetypeBody, ()> {
    let Some(obj) = body.as_object() else {
        return Err(());
    };

    let title = match obj.get("title") {
        None => {
            if require_all {
                return Err(());
            }
            None
        }
        Some(Value::String(s)) if !s.is_empty() => Some(s.clone()),
        Some(_) => return Err(()), // wrong type, null, or the min(1) failure
    };

    let nullable_str = |key: &str| -> Result<Option<Option<String>>, ()> {
        match obj.get(key) {
            None => Ok(None),
            Some(Value::Null) => Ok(Some(None)),
            Some(Value::String(s)) => Ok(Some(Some(s.clone()))),
            Some(_) => Err(()),
        }
    };
    let description = nullable_str("description")?;
    let image_prompt = nullable_str("imagePrompt")?;
    let appropriateness = nullable_str("appropriateness")?;

    let types = match obj.get("types") {
        None => {
            if require_all {
                return Err(());
            }
            None
        }
        Some(Value::Array(a)) => {
            let mut out = Vec::with_capacity(a.len());
            for v in a {
                match v.as_str() {
                    Some(s) if WARDROBE_TYPES.contains(&s) => out.push(s.to_string()),
                    _ => return Err(()),
                }
            }
            if out.is_empty() {
                return Err(()); // min(1, 'At least one type is required')
            }
            Some(out)
        }
        Some(_) => return Err(()),
    };

    let boolean = |key: &str| -> Result<Option<bool>, ()> {
        match obj.get(key) {
            None => Ok(None),
            Some(Value::Bool(b)) => Ok(Some(*b)),
            Some(_) => Err(()), // null fails `.optional()`
        }
    };
    let is_default = boolean("isDefault")?;
    let replace = boolean("replace")?;

    let component_item_ids = match obj.get("componentItemIds") {
        None => None,
        Some(Value::Array(a)) => {
            let mut out = Vec::with_capacity(a.len());
            for v in a {
                match v.as_str() {
                    Some(s) => out.push(s.to_string()),
                    None => return Err(()),
                }
            }
            Some(out)
        }
        Some(_) => return Err(()),
    };

    Ok(ArchetypeBody {
        title,
        description,
        image_prompt,
        types,
        appropriateness,
        is_default,
        component_item_ids,
        replace,
    })
}

// ===========================================================================
// GET /api/v1/wardrobe — list the archetype tier
// ===========================================================================

/// v4 `GET /api/v1/wardrobe` — `repos.wardrobe.findArchetypes()` (non-archived,
/// General only — the route has no project context) → `{wardrobeItems}`.
pub fn wardrobe_list(db: &Db) -> Response {
    let out = read_main_mount(db, |main, mount| {
        let docs = DocMountDocumentsRepository::new(mount);
        archetype_wardrobe::find_archetypes(main, &docs, false, &SharedWardrobeTiers::none())
    });
    match out {
        Ok(items) => Response::Wardrobe(json!({ "wardrobeItems": items })),
        // v4 catches → serverError with this exact message.
        Err(_) => internal("Failed to fetch archetype wardrobe items"),
    }
}

// ===========================================================================
// POST /api/v1/wardrobe — create an archetype
// ===========================================================================

/// v4 `POST /api/v1/wardrobe` — `createArchetypeSchema.parse` (uncaught → the
/// middleware `Validation error`), then `repos.wardrobe.create({characterId:
/// null, ...})` → 201 `{wardrobeItem}`. A thrown vault error (no General mount,
/// component cycle) propagates to v4's middleware → 500 `Internal server error`.
pub async fn wardrobe_create(db: &Db, body: Value) -> Response {
    let parsed = match parse_archetype_body(&body, true) {
        Ok(p) => p,
        Err(()) => return bad_request("Validation error"),
    };

    let out = with_both_conns(db, move |main, mount| {
        // v4 `repos.wardrobe.create` materializes id/timestamps; the route passes
        // `migratedFromClothingRecordId: null` explicitly (echoed as null) but no
        // `archivedAt` (omitted from the echo) — the characters-create precedent.
        let now = crate::clock::now_iso();
        let item = WardrobeItem {
            id: uuid::Uuid::new_v4().to_string(),
            character_id: Some(None),
            title: parsed.title.clone().unwrap_or_default(),
            description: Some(parsed.description.clone().flatten()),
            image_prompt: Some(parsed.image_prompt.clone().flatten()),
            types: parsed.types.clone().unwrap_or_default(),
            component_item_ids: parsed.component_item_ids.clone().unwrap_or_default(),
            appropriateness: Some(parsed.appropriateness.clone().flatten()),
            is_default: parsed.is_default.unwrap_or(false),
            replace: parsed.replace.unwrap_or(false),
            migrated_from_clothing_record_id: Some(None),
            archived_at: None,
            created_at: now.clone(),
            updated_at: now,
        };
        let links = DocMountFileLinksRepository::new(mount);
        let docs = DocMountDocumentsRepository::new(mount);
        Ok(create_vault_wardrobe_item(main, &links, &docs, &item))
    })
    .await;

    match out {
        // v4's create rethrows (no-mount / cycle) → the middleware's generic 500.
        Ok(Err(_)) => internal("Internal server error"),
        Ok(Ok(stored)) => Response::Wardrobe(json!({
            "wardrobeItem": serde_json::to_value(stored).unwrap_or(Value::Null)
        })),
        Err(e) => internal(e),
    }
}

// ===========================================================================
// GET /api/v1/wardrobe/{itemId}
// ===========================================================================

/// v4 `GET /api/v1/wardrobe/[itemId]` — `findArchetypeById` (archived included)
/// + the `characterId !== null` guard → `{wardrobeItem}` or the 404.
pub fn wardrobe_item_get(db: &Db, item_id: &str) -> Response {
    let iid = item_id.to_string();
    let out = read_main_mount(db, move |main, mount| {
        let docs = DocMountDocumentsRepository::new(mount);
        archetype_wardrobe::find_archetype_by_id(main, &docs, &iid, &SharedWardrobeTiers::none())
    });
    match out {
        Ok(Some(item)) if item.get("characterId").map(Value::is_null) == Some(true) => {
            Response::Wardrobe(json!({ "wardrobeItem": item }))
        }
        Ok(_) => not_found("Archetype wardrobe item"),
        // v4 catches → serverError with this exact message.
        Err(_) => internal("Failed to fetch archetype wardrobe item"),
    }
}

// ===========================================================================
// PUT /api/v1/wardrobe/{itemId}
// ===========================================================================

/// v4 `PUT /api/v1/wardrobe/[itemId]` — existence pre-check (404), then
/// `updateArchetypeSchema.parse` (uncaught → `Validation error`), then
/// `repos.wardrobe.update(itemId, patch, null)` → `{wardrobeItem}`. A vault
/// throw (cycle / no mount) propagates → 500 `Internal server error`; an update
/// that finds no item → the 404.
pub async fn wardrobe_update(db: &Db, item_id: &str, body: Value) -> Response {
    let iid = item_id.to_string();
    let parsed = match parse_archetype_body(&body, false) {
        Ok(p) => p,
        Err(()) => return bad_request("Validation error"),
    };
    let patch = WardrobePatch {
        title: parsed.title,
        types: parsed.types,
        component_item_ids: parsed.component_item_ids,
        description: parsed.description,
        image_prompt: parsed.image_prompt,
        appropriateness: parsed.appropriateness,
        is_default: parsed.is_default,
        replace: parsed.replace,
        archived_at: None,
    };

    let out = with_both_conns(db, move |main, mount| {
        let docs = DocMountDocumentsRepository::new(mount);
        // Pre-check (v4 does it before the body parse, but the parse is pure —
        // the observable order is identical for a valid id).
        let existing = archetype_wardrobe::find_archetype_by_id(
            main,
            &docs,
            &iid,
            &SharedWardrobeTiers::none(),
        )?;
        let exists = matches!(
            &existing,
            Some(item) if item.get("characterId").map(Value::is_null) == Some(true)
        );
        if !exists {
            return Ok(Err(not_found("Archetype wardrobe item")));
        }
        let links = DocMountFileLinksRepository::new(mount);
        match update_vault_wardrobe_item(main, &links, &docs, &iid, &patch, None) {
            // Re-read through the overlay so the echo carries the full
            // null-inclusive shape v4's merged JS object emits (the projects
            // precedent — the struct serialize skips `None` fields).
            Ok(Some(_)) => {
                let item = archetype_wardrobe::find_archetype_by_id(
                    main,
                    &docs,
                    &iid,
                    &SharedWardrobeTiers::none(),
                )?
                .unwrap_or(Value::Null);
                Ok(Ok(item))
            }
            Ok(None) => Ok(Err(not_found("Archetype wardrobe item"))),
            // v4's update rethrows → the middleware's generic 500.
            Err(_) => Ok(Err(internal("Internal server error"))),
        }
    })
    .await;

    match out {
        Ok(Ok(item)) => Response::Wardrobe(json!({ "wardrobeItem": item })),
        Ok(Err(r)) => r,
        Err(e) => internal(e),
    }
}

// ===========================================================================
// DELETE /api/v1/wardrobe/{itemId}
// ===========================================================================

/// v4 `DELETE /api/v1/wardrobe/[itemId]` — existence pre-check (404), the
/// warn-and-proceed `removeEquippedItemFromAllChats` cleanup, then
/// `repos.wardrobe.delete(itemId, null)` → `{success: true}`. A thrown delete →
/// the route's own catch (`Failed to delete archetype wardrobe item`).
pub async fn wardrobe_delete(db: &Db, item_id: &str) -> Response {
    let iid = item_id.to_string();
    let out = with_both_conns(db, move |main, mount| {
        let docs = DocMountDocumentsRepository::new(mount);
        let existing = archetype_wardrobe::find_archetype_by_id(
            main,
            &docs,
            &iid,
            &SharedWardrobeTiers::none(),
        )?;
        let exists = matches!(
            &existing,
            Some(item) if item.get("characterId").map(Value::is_null) == Some(true)
        );
        if !exists {
            return Ok(Err(not_found("Archetype wardrobe item")));
        }
        // warn-and-proceed cleanup (v4 wraps this in its own try/catch → warn).
        let _ = ChatOutfitsRepository::new(main).remove_equipped_item_from_all_chats(&iid);
        let links = DocMountFileLinksRepository::new(mount);
        match delete_vault_wardrobe_item(main, &links, &docs, &iid, None) {
            Ok(true) => Ok(Ok(())),
            Ok(false) => Ok(Err(not_found("Archetype wardrobe item"))),
            // v4's delete rethrows into the route's catch → this exact message.
            Err(_) => Ok(Err(internal("Failed to delete archetype wardrobe item"))),
        }
    })
    .await;

    match out {
        Ok(Ok(())) => Response::Wardrobe(json!({ "success": true })),
        Ok(Err(r)) => r,
        Err(e) => internal(e),
    }
}

// ===========================================================================
// GET /api/v1/wardrobe/transfers — destination enumeration
// ===========================================================================

/// v4 `GET /api/v1/wardrobe/transfers` — the destination options
/// (`successResponse` → the RAW `{destinations}` body at the edge).
pub fn wardrobe_transfer_destinations(db: &Db, user_id: &str) -> Response {
    let uid = user_id.to_string();
    let out = read_main_mount(db, move |main, mount| {
        Ok(wardrobe_transfers::enumerate_destinations(
            main, mount, &uid,
        ))
    });
    match out {
        Ok(Ok(v)) => Response::Wardrobe(v),
        // v4 catches → serverError with this exact message.
        Ok(Err(_)) | Err(_) => internal("Failed to load transfer destinations"),
    }
}

// ===========================================================================
// POST /api/v1/wardrobe/transfers — move / copy
// ===========================================================================

/// Zod-message port of v4 `transferRequestSchema` (the route CATCHES the
/// ZodError and joins the issue messages with `'; '`, so these strings are wire
/// payload). Issues collect in shape-key order: `action`, `itemId`,
/// `sourceCharacterId`, `sourceProjectId`, `destination` (then its keys).
fn parse_transfer_request(body: &Value) -> Result<TransferRequest, String> {
    use super::chat_outfits::received;

    let obj = body.as_object();
    if obj.is_none() {
        return Err(format!(
            "Invalid input: expected object, received {}",
            received(Some(body))
        ));
    }
    let mut issues: Vec<String> = Vec::new();

    let get = |key: &str| obj.and_then(|o| o.get(key));

    // action: z.enum(['move', 'copy']) — Zod 4's enum default carries NO
    // `received …` clause (oracle-confirmed: `tr_zod_bad_action`).
    let action = match get("action") {
        Some(Value::String(s)) if s == "move" => Some(TransferAction::Move),
        Some(Value::String(s)) if s == "copy" => Some(TransferAction::Copy),
        _ => {
            issues.push("Invalid option: expected one of \"move\"|\"copy\"".to_string());
            None
        }
    };

    // itemId / sourceCharacterId: z.string().min(1)
    let min1 = |key: &str, issues: &mut Vec<String>| -> Option<String> {
        match get(key) {
            Some(Value::String(s)) if !s.is_empty() => Some(s.clone()),
            Some(Value::String(_)) => {
                issues.push("Too small: expected string to have >=1 characters".to_string());
                None
            }
            v => {
                issues.push(format!(
                    "Invalid input: expected string, received {}",
                    received(v)
                ));
                None
            }
        }
    };
    let item_id = min1("itemId", &mut issues);
    let source_character_id = min1("sourceCharacterId", &mut issues);

    // sourceProjectId: z.string().nullable().optional()
    let source_project_id = match get("sourceProjectId") {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) => Some(s.clone()),
        v => {
            issues.push(format!(
                "Invalid input: expected string, received {}",
                received(v)
            ));
            None
        }
    };

    // destination: z.object({ scope: z.enum([...]), id: z.string().optional() })
    let mut destination_scope: Option<DestinationScope> = None;
    let mut destination_id: Option<String> = None;
    match get("destination") {
        Some(Value::Object(dest)) => {
            match dest.get("scope") {
                Some(Value::String(s)) => {
                    destination_scope = match s.as_str() {
                        "general" => Some(DestinationScope::General),
                        "project" => Some(DestinationScope::Project),
                        "group" => Some(DestinationScope::Group),
                        "character" => Some(DestinationScope::Character),
                        _ => {
                            issues.push(
                                "Invalid option: expected one of \"general\"|\"project\"|\"group\"|\"character\""
                                    .to_string(),
                            );
                            None
                        }
                    };
                }
                _ => {
                    issues.push(
                        "Invalid option: expected one of \"general\"|\"project\"|\"group\"|\"character\""
                            .to_string(),
                    );
                }
            }
            match dest.get("id") {
                None => {}
                Some(Value::String(s)) => destination_id = Some(s.clone()),
                v => {
                    issues.push(format!(
                        "Invalid input: expected string, received {}",
                        received(v)
                    ));
                }
            }
        }
        v => {
            issues.push(format!(
                "Invalid input: expected object, received {}",
                received(v)
            ));
        }
    }

    if !issues.is_empty() {
        return Err(issues.join("; "));
    }

    Ok(TransferRequest {
        action: action.unwrap(),
        item_id: item_id.unwrap(),
        source_character_id: source_character_id.unwrap(),
        source_project_id,
        destination_scope: destination_scope.unwrap(),
        destination_id,
    })
}

/// v4 `POST /api/v1/wardrobe/transfers` — the move/copy over the ported
/// [`wardrobe_transfers::transfer_wardrobe_item`]; success is
/// `successResponse({wardrobeItem, action})` (RAW at the edge). `now` is the
/// minted copy timestamp (injected — v4 `new Date().toISOString()`).
pub async fn wardrobe_transfer_apply(db: &Db, user_id: &str, body: Value, now: &str) -> Response {
    let req = match parse_transfer_request(&body) {
        Ok(r) => r,
        Err(joined) => return bad_request(joined),
    };
    let uid = user_id.to_string();
    let now = now.to_string();

    let out = with_both_conns(db, move |main, mount| {
        Ok(wardrobe_transfers::transfer_wardrobe_item(
            main, mount, &uid, &req, &now,
        ))
    })
    .await;

    match out {
        Ok(Ok(outcome)) => Response::Wardrobe(json!({
            "wardrobeItem": outcome.wardrobe_item,
            "action": wardrobe_transfers::action_str(outcome.action),
        })),
        Ok(Err(TransferError::NotFound)) => not_found("Wardrobe item"),
        Ok(Err(TransferError::BadRequest(m))) => bad_request(m),
        // v4's catch → serverError with this exact message.
        Ok(Err(TransferError::Internal(_))) => internal("Failed to transfer wardrobe item"),
        Err(e) => internal(e),
    }
}

// ===========================================================================
// POST /api/v1/wardrobe/preview-avatar (tier 2) — the render seam + handler
// ===========================================================================

/// What the preview route needs from the host: ONE raw provider render at
/// v4's fixed portrait geometry (`size: '1024x1792'`, `style: 'natural'`,
/// `n: 1`) ALREADY WebP-transcoded (v4 `createImageProvider(...).generateImage`
/// → `convertToWebP`). Both halves are host capabilities (provider IO + the
/// pixel codec), so the pair rides one erased seam; the harness cans it with
/// the oracle-reported post-transcode bytes (the photos `*_bytes` precedent),
/// so everything below — sha256, the vault write, the `files` row, the
/// response — diffs for real.
#[derive(Clone, Debug)]
pub struct AvatarPreviewRenderRequest {
    pub prompt: String,
    pub provider: String,
    pub model: String,
    /// `parameters.quality` when the profile carries one (`standard` | `hd`).
    pub quality: Option<String>,
    pub api_key: String,
    /// The character's display name (v4 mints the provider filename from it).
    pub character_name: String,
}

/// The transcoded render (v4's `converted` + the provider's revised prompt).
#[derive(Clone, Debug)]
pub struct AvatarPreviewImage {
    pub buffer: Vec<u8>,
    pub mime_type: String,
    /// The post-transcode filename (v4
    /// `avatar_preview_<safeName>_<Date.now()>.webp` — the host mints the
    /// stamp; the harness replays the oracle's).
    pub filename: String,
    pub revised_prompt: Option<String>,
}

/// A render failure, mirroring v4's arms.
#[derive(Clone, Debug)]
pub enum AvatarPreviewError {
    /// The v5-only unwired-seam refusal (loud, typed — the host wire is
    /// DEFERRED to unification; see the P4.9f1 lane record).
    NotConfigured,
    /// v4 `serverError('Image provider returned no image data')`.
    NoImageData,
    /// A thrown provider/transcode failure — v4's middleware generic 500.
    Failed(String),
}

/// The one-shot render contract the host implements at unification.
pub trait AvatarPreviewRenderer: Send + Sync {
    fn render<'a>(
        &'a self,
        req: &'a AvatarPreviewRenderRequest,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = Result<AvatarPreviewImage, AvatarPreviewError>>
                + Send
                + 'a,
        >,
    >;
}

/// A type-erased renderer (an `Arc<dyn …>`).
#[derive(Clone)]
pub struct ErasedAvatarPreview(std::sync::Arc<dyn AvatarPreviewRenderer>);

impl ErasedAvatarPreview {
    pub fn new<R: AvatarPreviewRenderer + 'static>(renderer: R) -> Self {
        Self(std::sync::Arc::new(renderer))
    }

    /// The not-wired default — every render answers
    /// [`AvatarPreviewError::NotConfigured`], so the guard arms upstream stay
    /// live and only the render step refuses.
    pub fn none() -> Self {
        struct NotConfigured;
        impl AvatarPreviewRenderer for NotConfigured {
            fn render<'a>(
                &'a self,
                _req: &'a AvatarPreviewRenderRequest,
            ) -> std::pin::Pin<
                Box<
                    dyn std::future::Future<Output = Result<AvatarPreviewImage, AvatarPreviewError>>
                        + Send
                        + 'a,
                >,
            > {
                Box::pin(async { Err(AvatarPreviewError::NotConfigured) })
            }
        }
        Self(std::sync::Arc::new(NotConfigured))
    }

    pub async fn render(
        &self,
        req: &AvatarPreviewRenderRequest,
    ) -> Result<AvatarPreviewImage, AvatarPreviewError> {
        self.0.render(req).await
    }
}

/// The parsed preview body (v4 `previewAvatarSchema`).
struct PreviewBody {
    character_id: String,
    equipped_slots: Value,
    image_profile_id: Option<String>,
}

/// Message-faithful port of v4 `previewAvatarSchema`
/// (`preview-avatar/route.ts:34-38`) — the route catches its ZodError and joins
/// with `', '`. Shape order: `characterId`, `equippedSlots` (REQUIRED),
/// `imageProfileId`.
fn parse_preview_body(body: &Value) -> Result<PreviewBody, String> {
    use super::chat_outfits::{parse_slots_object, received};

    let obj = body.as_object();
    if obj.is_none() {
        return Err(format!(
            "Invalid input: expected object, received {}",
            received(Some(body))
        ));
    }
    let mut issues: Vec<String> = Vec::new();
    let get = |key: &str| obj.and_then(|o| o.get(key));

    let character_id = match get("characterId") {
        Some(Value::String(s)) if !s.is_empty() => Some(s.clone()),
        Some(Value::String(_)) => {
            issues.push("characterId is required".to_string());
            None
        }
        v => {
            issues.push(format!(
                "Invalid input: expected string, received {}",
                received(v)
            ));
            None
        }
    };

    // equippedSlots: EquippedSlotsSchema — REQUIRED here (no `.optional()`).
    let equipped_slots = match get("equippedSlots") {
        Some(Value::Object(s)) => Some(parse_slots_object(s, &mut issues)),
        v => {
            issues.push(format!(
                "Invalid input: expected object, received {}",
                received(v)
            ));
            None
        }
    };

    let image_profile_id = match get("imageProfileId") {
        None => None,
        Some(Value::String(s)) if !s.is_empty() => Some(s.clone()),
        Some(Value::String(_)) => {
            issues.push("Too small: expected string to have >=1 characters".to_string());
            None
        }
        v => {
            issues.push(format!(
                "Invalid input: expected string, received {}",
                received(v)
            ));
            None
        }
    };

    if !issues.is_empty() {
        return Err(issues.join(", "));
    }
    Ok(PreviewBody {
        character_id: character_id.unwrap(),
        equipped_slots: equipped_slots.unwrap(),
        image_profile_id,
    })
}

/// v4 `POST /api/v1/wardrobe/preview-avatar` — the SYNCHRONOUS one-off render
/// against an arbitrary slot snapshot. Saved as a regular generated image
/// (linked + tagged to the character, `photos`-invisible `images/history/`
/// vault write) but NEVER onto `avatarOverrides` or any chat — v4's header
/// isolation note, preserved. `now` stamps the `files` row (injected);
/// `file_id` is minted here (normalized in the differential).
pub async fn wardrobe_preview_avatar(
    db: &Db,
    renderer: &ErasedAvatarPreview,
    user_id: &str,
    body: Value,
    now: &str,
) -> Response {
    let parsed = match parse_preview_body(&body) {
        Ok(p) => p,
        Err(joined) => return bad_request(joined),
    };

    // Character + ownership (v4 `badRequest('Character not found')` for both).
    let cid = parsed.character_id.clone();
    let character = match with_both_conns(db, move |main, mount| {
        crate::db::characters_read::find_by_id(main, mount, &cid)
    })
    .await
    {
        Ok(Some(c)) => c,
        Ok(None) => return bad_request("Character not found"),
        Err(e) => return internal(e),
    };
    if character.get("userId").and_then(Value::as_str) != Some(user_id) {
        return bad_request("Character not found");
    }
    let character_name = character
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();

    // Image profile: explicit override → the default → the first → none.
    let override_id = parsed.image_profile_id.clone();
    let profile = match db.read_main(move |conn| {
        let mut profile = None;
        if let Some(id) = override_id.as_deref() {
            profile = crate::db::image_profiles::find_by_id(conn, id)?;
        }
        if profile.is_none() {
            let all = crate::db::image_profiles::find_all(conn)?;
            profile = all
                .iter()
                .find(|p| p.get("isDefault").and_then(Value::as_bool) == Some(true))
                .cloned()
                .or_else(|| all.into_iter().next());
        }
        Ok(profile)
    }) {
        Ok(Some(p)) => p,
        Ok(None) => return bad_request("No image profile available for avatar generation"),
        Err(e) => return internal(e),
    };

    // `!imageProfile.apiKeyId` — null/absent/empty are all falsy.
    let Some(api_key_id) = profile
        .get("apiKeyId")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
    else {
        return bad_request("Selected image profile has no API key configured");
    };
    let uid = user_id.to_string();
    let api_key = match db
        .read_main(move |conn| crate::db::api_keys::find_by_id_and_user_id(conn, &api_key_id, &uid))
    {
        Ok(Some(k)) if !k.key_value.is_empty() => k.key_value,
        Ok(_) => return bad_request("API key for image profile is missing or invalid"),
        Err(e) => return internal(e),
    };

    // Aurora character aesthetic — GLOBAL tier (a preview has no project context).
    let character_aesthetic = crate::services::aesthetics::resolve_aesthetic(
        db,
        crate::services::aesthetics::AestheticKind::Aurora,
        None,
        None,
    )
    .await;

    // The portrait prompt over the ported builder (no project mounts).
    let character_for_prompt = character.clone();
    let slots_for_prompt = parsed.equipped_slots.clone();
    let prompt_result = match with_both_conns(db, move |main, mount| {
        let docs = DocMountDocumentsRepository::new(mount);
        crate::services::avatar_prompt::build_character_avatar_prompt(
            main,
            mount,
            &docs,
            &character_for_prompt,
            &crate::services::avatar_prompt::AvatarPromptOptions {
                equipped_slots: Some(crate::wardrobe::Slots::from_value(Some(&slots_for_prompt))),
                project_mount_point_ids: Vec::new(),
                character_aesthetic: character_aesthetic.clone(),
            },
        )
    })
    .await
    {
        Ok(r) => r,
        Err(e) => return internal(e),
    };
    if !prompt_result.has_appearance {
        return bad_request(
            "No appearance data available — add a physical description or equip wardrobe items first",
        );
    }
    let prompt = prompt_result.prompt;

    // The render (provider + transcode — the erased host seam).
    let render_req = AvatarPreviewRenderRequest {
        prompt: prompt.clone(),
        provider: profile
            .get("provider")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        model: profile
            .get("modelName")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        quality: profile
            .get("parameters")
            .and_then(|p| p.get("quality"))
            .and_then(Value::as_str)
            .map(str::to_string),
        api_key,
        character_name: character_name.clone(),
    };
    let image = match renderer.render(&render_req).await {
        Ok(img) => img,
        Err(AvatarPreviewError::NotConfigured) => {
            return Response::error(
                ErrorKind::Internal,
                "wardrobePreviewAvatar: the avatar-preview render seam is not assembled \
                 (the host wire is deferred to unification — the P4.9f1 lane record)",
            )
        }
        Err(AvatarPreviewError::NoImageData) => {
            return internal("Image provider returned no image data")
        }
        // v4: an uncaught provider throw → the middleware's generic 500.
        Err(AvatarPreviewError::Failed(_)) => return internal("Internal server error"),
    };

    // Persist: the character-vault `images/history/` write + the `files` row.
    // Any failure in this block is v4's `serverError('Failed to save avatar
    // preview')` (its try/catch around exactly this region).
    let sha256 = crate::photos::keep_image_markdown::sha256_of_buffer(&image.buffer);
    let file_id = uuid::Uuid::new_v4().to_string();
    let model_name = render_req.model.clone();
    let description = format!("{character_name} — outfit preview");
    let character_id = parsed.character_id.clone();
    let image_for_write = image.clone();
    let now_owned = now.to_string();
    let uid_owned = user_id.to_string();
    let file_id_owned = file_id.clone();
    let prompt_owned = prompt.clone();

    let saved = with_both_conns(db, move |main, mount| {
        let written = crate::services::image_job_storage::write_character_avatar_to_vault(
            main,
            mount,
            &character_id,
            &image_for_write.filename,
            &image_for_write.buffer,
            &image_for_write.mime_type,
            Some(&description),
        )?;
        // The vault bridge transcodes bitmap uploads; the FileEntry records the
        // post-transcode mime/size (v4's comment, preserved).
        crate::db::files::FilesRepository::new(main).create(
            &crate::db::files::FileCreate {
                user_id: uid_owned.clone(),
                sha256: sha256.clone(),
                original_filename: image_for_write.filename.clone(),
                mime_type: written.stored_mime_type.clone(),
                size: written.size_bytes as f64,
                width: Some(1024.0),
                height: Some(1792.0),
                is_plain_text: None,
                // Linked + tagged to the character, NOT to any chat (v4's
                // deliberate isolation).
                linked_to: vec![character_id.clone()],
                source: "GENERATED".to_string(),
                category: "IMAGE".to_string(),
                generation_prompt: Some(prompt_owned.clone()),
                generation_model: Some(model_name.clone()),
                generation_revised_prompt: image_for_write.revised_prompt.clone(),
                description: Some(description.clone()),
                tags: vec![character_id.clone()],
                project_id: None,
                folder_path: None,
                storage_key: Some(written.storage_key.clone()),
                file_status: "ok".to_string(),
            },
            &crate::db::files::CreateOptions {
                id: file_id_owned.clone(),
                created_at: now_owned.clone(),
                updated_at: now_owned.clone(),
            },
        )?;
        Ok(())
    })
    .await;

    match saved {
        Ok(()) => Response::Wardrobe(json!({
            "fileId": file_id,
            "url": format!("/api/v1/files/{file_id}?action=download"),
            "mimeType": image.mime_type,
            "prompt": prompt,
        })),
        Err(_) => internal("Failed to save avatar preview"),
    }
}

// ===========================================================================
// POST /api/v1/wardrobe/analyze-image — REFUSAL-ARMED (tier 3, §4)
// ===========================================================================

/// v4 `POST /api/v1/wardrobe/analyze-image` — a vision-LLM call
/// (`lib/wardrobe/image-analysis.ts`) with no Rust port; §4 forbids adding a
/// provider path this round. The loud typed refusal, matching the SPA
/// sibling's deferral (lane P4.9f2 ships no import-from-image entry point).
pub fn wardrobe_analyze_image() -> Response {
    Response::error(
        ErrorKind::Internal,
        "wardrobeAnalyzeImage is not available in this build: the wardrobe \
         image-analysis subsystem (a vision-LLM path) is not ported — \
         a P4.9f1 tier-3 deferral (§4: no new provider path this round)",
    )
}
