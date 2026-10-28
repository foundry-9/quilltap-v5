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

use crate::db::doc_mount_documents::DocMountDocumentsRepository;
use crate::db::runtime::Db;
use crate::db::DbError;
use crate::image_gen::params_builder::{ImageGenOverrides, ImageParamsLogContext};
use crate::image_gen::Orientation;
use crate::image_gen_data::image_declarations_for;
use crate::model::image::ImageGenParams;
use crate::services::activity_kinds::ActivityKind;
use crate::services::activity_registry::track_activity;
use crate::services::image_job_common::with_both_conns;
use crate::services::wardrobe_transfers::{
    self, ComponentMode, DestinationScope, ExplicitSource, SourceScope, TransferAction,
    TransferError, TransferRequest,
};

use super::types::{ErrorKind, Response};

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
// Quilltap General through the one route factory (v4 `app/api/v1/wardrobe/
// route.ts` + `[itemId]/route.ts` at `9ecebf986` — registrations of
// `lib/wardrobe/routes/wardrobe-route-factory.ts`; P4.D281)
// ===========================================================================

use super::wardrobe_route_factory::{self as factory, GENERAL_WARDROBE};

/// The engine's single user (General has no owner, so the factory's ownership
/// gate never reads it; v4's INFO lines log it as `userId`).
const USER_ID: &str = super::engine::SINGLE_USER_ID;

/// v4 `GET /api/v1/wardrobe` — `{ mountPointId, wardrobeItems }` (origin +
/// wear); an unprovisioned General answers `{ wardrobeItems: [] }`.
pub async fn wardrobe_list(db: &Db, include_archived: bool) -> Response {
    factory::list_store_items(db, GENERAL_WARDROBE, None, USER_ID, include_archived).await
}

/// v4 `POST /api/v1/wardrobe` — 201 `{ wardrobeItem }` (origin + wear); an
/// unprovisioned General is the 500 `Quilltap General store is not
/// provisioned yet`; a component cycle the 400.
pub async fn wardrobe_create(db: &Db, body: Value) -> Response {
    factory::create(db, GENERAL_WARDROBE, None, USER_ID, &body).await
}

/// v4 `GET /api/v1/wardrobe/[itemId]` — `{ wardrobeItem }` (origin + wear) or
/// 404 `Archetype wardrobe item`.
pub fn wardrobe_item_get(db: &Db, item_id: &str) -> Response {
    factory::item_get(db, GENERAL_WARDROBE, None, USER_ID, item_id)
}

/// v4 `GET /api/v1/wardrobe/[itemId]?action=wear-history`.
pub(crate) fn wardrobe_item_wear_history_general(db: &Db, item_id: &str) -> Response {
    factory::item_wear_history(db, GENERAL_WARDROBE, None, USER_ID, item_id)
}

/// v4 `PUT /api/v1/wardrobe/[itemId]` — 404 → Zod → `imageFileId` → update.
pub async fn wardrobe_update(db: &Db, item_id: &str, body: Value) -> Response {
    factory::item_update(db, GENERAL_WARDROBE, None, USER_ID, item_id, body).await
}

/// v4 `DELETE /api/v1/wardrobe/[itemId]` — `{ success: true }`.
pub async fn wardrobe_delete(db: &Db, item_id: &str) -> Response {
    factory::item_delete(db, GENERAL_WARDROBE, None, USER_ID, item_id).await
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
    let failure = match out {
        Ok(Ok(v)) => return Response::Wardrobe(v),
        // P4.166 (dogfood #156): a DEGRADED mount index is not v4's catch —
        // its three list reads are fallback overlays that DROP every entity
        // whose store they cannot read, so every project and group falls away
        // and only a vault-less character survives (measured: 200 with empty
        // lists on a degraded clone).
        Err(crate::db::DbError::PartitionUnavailable(_)) => {
            match degraded_transfer_destinations(db, user_id) {
                Ok(v) => return Response::Wardrobe(v),
                Err(e) => crate::db::fallback::error_text(&e),
            }
        }
        Ok(Err(e)) => transfer_error_text(&e),
        Err(e) => crate::db::fallback::error_text(&e),
    };
    // v4 `transfers/route.ts:203-206` (P4.166, survey C.11): the catch's ERROR
    // (the Error as winston's error argument), then this exact message — the
    // ONE catch for the sound read and the degraded arm alike.
    tracing::error!(
        userId = user_id,
        error = %failure,
        "[WardrobeTransfers v1] Failed to list destinations"
    );
    internal("Failed to load transfer destinations")
}

/// [`wardrobe_transfer_destinations`] on a DEGRADED mount index (P4.166):
/// General stays available, every store-backed project / group drops, and the
/// user's characters are the VAULT-LESS ones (v4's overlay drops a vaulted
/// character whose keystone it cannot read) — named and sorted by the sound
/// enumeration's own [`wardrobe_transfers::destinations_body`]. A failed read
/// is the caller's catch.
fn degraded_transfer_destinations(db: &Db, user_id: &str) -> Result<Value, DbError> {
    let characters = db.read_main(crate::db::characters_read::find_all_raw)?;
    let users: Vec<Value> = characters
        .into_iter()
        .filter(|c| c.get("userId").and_then(Value::as_str) == Some(user_id))
        .filter(|c| {
            c.get("characterDocumentMountPointId")
                .and_then(Value::as_str)
                .is_none_or(str::is_empty)
        })
        .collect();
    Ok(wardrobe_transfers::destinations_body(&[], &[], &users))
}

/// A [`TransferError`]'s text for the catch's ERROR line.
fn transfer_error_text(e: &TransferError) -> String {
    match e {
        TransferError::NotFound => "Wardrobe item not found".to_string(),
        TransferError::BadRequest(m)
        | TransferError::Conflict(m)
        | TransferError::Server(m)
        | TransferError::Internal(m) => m.clone(),
    }
}

// ===========================================================================
// POST /api/v1/wardrobe/transfers — move / copy
// ===========================================================================

/// Zod-message port of v4 `transferRequestSchema` (the route CATCHES the
/// ZodError and joins the issue messages with `'; '`, so these strings are wire
/// payload). Issues collect in shape-key order: `action`, `itemId`,
/// `sourceCharacterId`, `sourceProjectId`, `source`, `destination` (then its
/// keys), `components`. The two `.refine`s run only when the base shape parsed
/// (Zod skips refinements on a failed object parse), and BOTH run when it did.
///
/// `pub` so the transfers tier-2 differential can drive the same body-parse
/// layer the route does.
pub fn parse_transfer_request(body: &Value) -> Result<TransferRequest, String> {
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

    // itemId: z.string().min(1); sourceCharacterId: the same `.optional()`
    // (absent OK, an explicit null is NOT — `.optional()` without
    // `.nullable()`).
    let min1 = |v: Option<&Value>, issues: &mut Vec<String>| -> Option<String> {
        match v {
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
    let item_id = min1(get("itemId"), &mut issues);
    let source_character_id = match get("sourceCharacterId") {
        None => None,
        v => min1(v, &mut issues),
    };

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

    // source: z.object({ scope: WardrobeScopeSchema, id: z.string().optional()
    // }).optional() — the explicit container (d7263f39). Since `9ecebf986`
    // BOTH source and destination take `WardrobeScopeSchema =
    // z.enum(['character','group','project','general'])` (`location.ts:56`) —
    // the issue sentence's ORDER moved on both.
    let mut source: Option<ExplicitSource> = None;
    match get("source") {
        None => {}
        Some(Value::Object(src)) => {
            let scope = match src.get("scope") {
                Some(Value::String(s)) => match s.as_str() {
                    "character" => Some(SourceScope::Character),
                    "project" => Some(SourceScope::Project),
                    "group" => Some(SourceScope::Group),
                    "general" => Some(SourceScope::General),
                    _ => None,
                },
                _ => None,
            };
            if scope.is_none() {
                issues.push(
                    "Invalid option: expected one of \"character\"|\"group\"|\"project\"|\"general\""
                        .to_string(),
                );
            }
            let mut source_id: Option<String> = None;
            match src.get("id") {
                None => {}
                Some(Value::String(s)) => source_id = Some(s.clone()),
                v => {
                    issues.push(format!(
                        "Invalid input: expected string, received {}",
                        received(v)
                    ));
                }
            }
            if let Some(scope) = scope {
                source = Some(ExplicitSource {
                    scope,
                    id: source_id,
                });
            }
        }
        v => {
            issues.push(format!(
                "Invalid input: expected object, received {}",
                received(v)
            ));
        }
    }

    // destination: z.object({ scope: WardrobeScopeSchema, id: z.string().optional() })
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
                                "Invalid option: expected one of \"character\"|\"group\"|\"project\"|\"general\""
                                    .to_string(),
                            );
                            None
                        }
                    };
                }
                _ => {
                    issues.push(
                        "Invalid option: expected one of \"character\"|\"group\"|\"project\"|\"general\""
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

    // components: z.enum(['move', 'copy', 'none']).optional() (f6a10055).
    let components = match get("components") {
        None => ComponentMode::None,
        Some(Value::String(s)) if s == "move" => ComponentMode::Move,
        Some(Value::String(s)) if s == "copy" => ComponentMode::Copy,
        Some(Value::String(s)) if s == "none" => ComponentMode::None,
        _ => {
            issues.push("Invalid option: expected one of \"move\"|\"copy\"|\"none\"".to_string());
            ComponentMode::None
        }
    };

    if !issues.is_empty() {
        return Err(issues.join("; "));
    }

    // The two schema refines — they only run once the base shape parsed, and
    // both run (each failure is its own joined issue).
    let mut refine_issues: Vec<String> = Vec::new();
    if source_character_id.is_none() && source.is_none() {
        refine_issues.push("Either sourceCharacterId or source is required".to_string());
    }
    if action == Some(TransferAction::Copy) && components == ComponentMode::Move {
        refine_issues.push(
            "Copying an outfit cannot move its components — the original outfit still needs them"
                .to_string(),
        );
    }
    if !refine_issues.is_empty() {
        return Err(refine_issues.join("; "));
    }

    Ok(TransferRequest {
        action: action.unwrap(),
        item_id: item_id.unwrap(),
        source_character_id,
        source_project_id,
        source,
        destination_scope: destination_scope.unwrap(),
        destination_id,
        components,
    })
}

/// v4 `POST /api/v1/wardrobe/transfers` — the move/copy over the ported
/// [`wardrobe_transfers::transfer_wardrobe_item`]; success is
/// `successResponse({wardrobeItem, action})` (RAW at the edge). `now` is the
/// minted copy timestamp (injected — v4 `new Date().toISOString()`).
///
/// The carried pictures' blob links write through the
/// [`RefusingWebpTranscoder`](crate::services::mount_index::blob_transcode::RefusingWebpTranscoder)
/// (v4's store-original fallback) — a codec-less caller's form. A carried
/// picture is already a stored WebP, which the normalizer passes through
/// either way. The engine arm calls [`wardrobe_transfer_apply_with_codec`]
/// over `ready_db_and_blob_webp()` (P4.D263's HANDOFF, landed at the
/// `f5e953a3f` unification).
pub async fn wardrobe_transfer_apply(db: &Db, user_id: &str, body: Value, now: &str) -> Response {
    wardrobe_transfer_apply_with_codec(
        db,
        user_id,
        body,
        now,
        std::sync::Arc::new(crate::services::mount_index::blob_transcode::RefusingWebpTranscoder),
    )
    .await
}

/// [`wardrobe_transfer_apply`] with the engine's blob codec for the carried
/// pictures' links (P4.D263).
pub async fn wardrobe_transfer_apply_with_codec(
    db: &Db,
    user_id: &str,
    body: Value,
    now: &str,
    blob_webp: std::sync::Arc<dyn crate::services::mount_index::blob_transcode::WebpTranscoder>,
) -> Response {
    let req = match parse_transfer_request(&body) {
        Ok(r) => r,
        Err(joined) => return bad_request(joined),
    };
    let uid = user_id.to_string();
    let now = now.to_string();

    let out = with_both_conns(db, move |main, mount| {
        Ok(wardrobe_transfers::transfer_wardrobe_item(
            main,
            mount,
            &uid,
            &req,
            &now,
            blob_webp.as_ref(),
        ))
    })
    .await;

    match out {
        Ok(Ok(outcome)) => {
            if let Some(f) = &outcome.read_back_failure {
                tracing::error!(
                    userId = user_id,
                    outfitId = f.outfit_id.as_str(),
                    outfitFoundAtDestination = f.outfit_found_at_destination,
                    plannedComponentIdsJson =
                        %serde_json::to_string(&f.planned_component_ids).unwrap_or_default(),
                    readBackComponentIdsJson =
                        %serde_json::to_string(&f.read_back_component_ids).unwrap_or_default(),
                    unresolvedComponentIdsJson =
                        %serde_json::to_string(&outcome.unresolved_component_ids).unwrap_or_default(),
                    destinationScope = f.destination_scope,
                    destinationMountPointId = f.destination_mount_point_id.as_str(),
                    "[WardrobeTransfers v1] Transferred outfit did not read back with its planned component references"
                );
            }
            // v4 `9ecebf986` + `4a38222e6` (`folder: stored.folder ?? ''`).
            let t = &outcome.transferred;
            tracing::info!(
                userId = user_id,
                action = wardrobe_transfers::action_str(outcome.action),
                componentMode = t.component_mode,
                itemId = t.item_id.as_str(),
                resultItemId = t.result_item_id.as_str(),
                folder = t.folder.as_str(),
                componentsTransferred = outcome.components_transferred,
                sourceScope = t.source_scope,
                destinationScope = t.destination_scope,
                sourceCharacterId = t.source_character_id.as_deref().unwrap_or("null"),
                destinationCharacterId = t.destination_character_id.as_deref().unwrap_or("null"),
                sourceMountPointId = t.source_mount_point_id.as_str(),
                destinationMountPointId = t.destination_mount_point_id.as_str(),
                "[WardrobeTransfers v1] Wardrobe item transferred"
            );
            // v4: `{ wardrobeItem, action, componentsTransferred,
            // ...(unresolvedComponentIds when non-empty) }`.
            let mut body = json!({
                "wardrobeItem": outcome.wardrobe_item,
                "action": wardrobe_transfers::action_str(outcome.action),
                "componentsTransferred": outcome.components_transferred,
            });
            if !outcome.unresolved_component_ids.is_empty() {
                body.as_object_mut().unwrap().insert(
                    "unresolvedComponentIds".to_string(),
                    json!(outcome.unresolved_component_ids),
                );
            }
            Response::Wardrobe(body)
        }
        Ok(Err(TransferError::NotFound)) => not_found("Wardrobe item"),
        Ok(Err(TransferError::BadRequest(m))) => bad_request(m),
        // v4 `conflict(...)` for a `CharacterArchivedError` (P4.D263).
        Ok(Err(TransferError::Conflict(m))) => Response::error(ErrorKind::Conflict, m),
        // v4's explicit serverError arms — the message IS the wire payload.
        Ok(Err(TransferError::Server(m))) => internal(m),
        // v4's catch → its ERROR (P4.166, survey C.11 — `transfers/route.ts:
        // 424`, no context keys, the Error as winston's error argument) and
        // serverError with this exact message. A failure before the transfer
        // ran (a degraded mount index among them) is the same catch in v4.
        Ok(Err(TransferError::Internal(m))) => transfer_failed(&m),
        Err(e) => transfer_failed(&crate::db::fallback::error_text(&e)),
    }
}

/// v4's POST catch (`transfers/route.ts:424-425`).
fn transfer_failed(error: &str) -> Response {
    tracing::error!(error = %error, "[WardrobeTransfers v1] Failed to transfer item");
    internal("Failed to transfer wardrobe item")
}

// ===========================================================================
// POST /api/v1/wardrobe/preview-avatar (tier 2) — the render seam + handler
// ===========================================================================

// === P4.D283: preview-avatar (owned) ===

/// What the preview route needs from the host (P4.D283, round ruling R-G): the
/// image-provider IO and the pixel codec — both host capabilities, riding one
/// erased seam. The CORE threads v4's `9ecebf986` pipeline AROUND them:
/// `resolveAvatarImageProfile` → the shared logged attempt
/// (`image_gen::image_attempt`) inside `generate_image_with_concierge_failover`
/// (so a refusal is retried once on the uncensored understudy and `rerouted`
/// / `trail` are REAL) → `decode_provider_image` over the codec → the vault
/// write → `create_generated_file_row`. The host answers ONE
/// `provider.generateImage(params, apiKey)` per attempt and lends its codec
/// (v4 `sharp`); the harness cans the provider and uses the real codec.
pub trait AvatarPreviewRenderer: Send + Sync {
    /// One image-provider call (v4 `createImageProvider(profile.provider)
    /// .generateImage(params, apiKey)`), as the chokepoint asks it per profile.
    fn generate<'a>(
        &'a self,
        provider: &'a str,
        api_key: &'a str,
        params: &'a ImageGenParams,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<
                    Output = Result<
                        crate::model::image::ImageGenResponse,
                        crate::model::image::ImageGenError,
                    >,
                > + Send
                + 'a,
        >,
    >;

    /// The pixel codec `decodeProviderImage`'s `convertToWebP` runs over.
    fn codec(&self) -> std::sync::Arc<dyn crate::services::file_storage::PixelCodec>;
}

/// A type-erased renderer (an `Arc<dyn …>`).
#[derive(Clone)]
pub struct ErasedAvatarPreview(std::sync::Arc<dyn AvatarPreviewRenderer>);

impl ErasedAvatarPreview {
    pub fn new<R: AvatarPreviewRenderer + 'static>(renderer: R) -> Self {
        Self(std::sync::Arc::new(renderer))
    }

    /// The not-wired default — every provider call answers the loud
    /// not-assembled refusal (as the generation's `ImageGenError`, so it surfaces
    /// through v4's own 502 arm), and the guard arms upstream stay live.
    pub fn none() -> Self {
        struct NotConfigured;
        impl AvatarPreviewRenderer for NotConfigured {
            fn generate<'a>(
                &'a self,
                _provider: &'a str,
                _api_key: &'a str,
                _params: &'a ImageGenParams,
            ) -> std::pin::Pin<
                Box<
                    dyn std::future::Future<
                            Output = Result<
                                crate::model::image::ImageGenResponse,
                                crate::model::image::ImageGenError,
                            >,
                        > + Send
                        + 'a,
                >,
            > {
                Box::pin(async {
                    Err(crate::model::image::ImageGenError::new(
                        NOT_CONFIGURED_MESSAGE,
                    ))
                })
            }
            fn codec(&self) -> std::sync::Arc<dyn crate::services::file_storage::PixelCodec> {
                std::sync::Arc::new(crate::services::file_storage::NotConfiguredPixelCodec)
            }
        }
        Self(std::sync::Arc::new(NotConfigured))
    }

    /// The seam's codec.
    pub fn codec(&self) -> std::sync::Arc<dyn crate::services::file_storage::PixelCodec> {
        self.0.codec()
    }
}

/// The v5-only unwired-seam refusal (loud, typed — the host wire assembles the
/// real provider; the P4.9f1 lane record).
const NOT_CONFIGURED_MESSAGE: &str =
    "wardrobePreviewAvatar: the avatar-preview render seam is not assembled \
     (the host wire is deferred to unification — the P4.9f1 lane record)";

impl crate::model::image::ImageProvider for ErasedAvatarPreview {
    async fn generate_image(
        &self,
        provider: &str,
        api_key: &str,
        params: &ImageGenParams,
    ) -> Result<crate::model::image::ImageGenResponse, crate::model::image::ImageGenError> {
        self.0.generate(provider, api_key, params).await
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
///
/// Avatar previews generate synchronously rather than through the job queue, so
/// the handler registers with the activity registry — the toolbar's "Img" chip
/// stays lit for the whole preview, prompt build and provider wait included. v4
/// `preview-avatar/route.ts:222` (`664cfca84`), which wraps at the route.
pub async fn wardrobe_preview_avatar(
    db: &Db,
    renderer: &ErasedAvatarPreview,
    user_id: &str,
    body: Value,
    now: &str,
    // P4.104: the encoder the `images/history/` vault write normalizes through
    // (`None` → the refusing encoder, v4's `sharp`-threw arm).
    blob_webp: Option<
        std::sync::Arc<dyn crate::services::mount_index::blob_transcode::WebpTranscoder>,
    >,
) -> Response {
    track_activity(
        ActivityKind::Image,
        run_wardrobe_preview_avatar(db, renderer, user_id, body, now, blob_webp),
    )
    .await
}

async fn run_wardrobe_preview_avatar(
    db: &Db,
    renderer: &ErasedAvatarPreview,
    user_id: &str,
    body: Value,
    now: &str,
    blob_webp: Option<
        std::sync::Arc<dyn crate::services::mount_index::blob_transcode::WebpTranscoder>,
    >,
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

    // The one-shot pick, else the user's default — each checked for an API key
    // (v4 `resolveAvatarImageProfile(user.id, repos, { override })`, C1 §8).
    let override_id = parsed.image_profile_id.clone();
    let uid_for_profile = user_id.to_string();
    let profile = match db.read_main(move |conn| {
        Ok(
            crate::services::image_profile_resolution::resolve_avatar_image_profile(
                conn,
                &uid_for_profile,
                override_id.as_deref(),
                None,
            ),
        )
    }) {
        Ok(p) => p,
        Err(e) => return internal(e),
    };
    // `!imageProfile?.apiKeyId` — null/absent/empty are all falsy; one sentence
    // for "no profile" and "a profile with no key" (v4 `9ecebf986`).
    let Some(profile) = profile else {
        return bad_request(NO_AVATAR_IMAGE_PROFILE_MESSAGE);
    };
    let Some(api_key_id) = profile
        .get("apiKeyId")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
    else {
        return bad_request(NO_AVATAR_IMAGE_PROFILE_MESSAGE);
    };
    // v4 `findApiKeyByIdAndUserId` is a fallback read: a read error logs its
    // line and is `null`, so it is this 400 — never a 500 (P4.139).
    let api_key =
        match crate::services::api_key_service::read_api_key_scoped(db, &api_key_id, user_id) {
            Some(k) if !k.key_value.is_empty() => k.key_value,
            _ => return bad_request("API key for image profile is missing or invalid"),
        };
    let profile_id = profile
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();

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
                // v4's preview-avatar route passes no `carrying` (measured).
                carrying: None,
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
    let leaf_counts = &prompt_result.leaf_counts;
    let leaf_counts_json = json!({
        "top": leaf_counts.top,
        "bottom": leaf_counts.bottom,
        "footwear": leaf_counts.footwear,
        "accessories": leaf_counts.accessories,
        "hair": leaf_counts.hair,
    })
    .to_string();

    tracing::debug!(
        context = PREVIEW_LOG_CONTEXT,
        characterId = %parsed.character_id,
        profileId = %profile_id,
        profileOverride = parsed.image_profile_id.is_some(),
        leafCountsJson = %leaf_counts_json,
        "[Avatar Preview] Generating preview"
    );

    // No chat: the global Concierge policy decides whether a refusal may fail
    // over; there is no Locked state to honour and nothing to announce.
    let uid_for_settings = user_id.to_string();
    let chat_settings = db
        .read_main(move |c| {
            Ok(crate::db::chat_settings::find_by_user_id_or_none(
                c,
                &uid_for_settings,
            ))
        })
        .unwrap_or_else(|e| {
            crate::db::chat_settings::log_find_by_user_id_failure(&e);
            None
        });
    let concierge_policy = crate::services::dangerous_content::resolver::resolve_concierge_settings(
        chat_settings.as_ref(),
        None,
    );

    // Portrait through the shared builder, so the preview shows what the profile
    // actually produces — LoRAs, residual options and all — mapped onto the
    // provider's own shape mechanism, exactly as the chat avatar job asks (v4
    // `makeLoggedImageAttempt`, `9ecebf986`: the `IMAGE_GENERATION` row is the
    // attempt's now — the pre-pin preview wrote none).
    let character_id_owned = parsed.character_id.clone();
    let mut attempt = crate::image_gen::image_attempt::make_logged_image_attempt(
        db,
        renderer,
        crate::image_gen::image_attempt::LoggedImageAttemptOptions {
            user_id,
            log_type: crate::services::llm_logging::log_type::IMAGE_GENERATION,
            prompt: &prompt,
            primary_profile_id: &profile_id,
            chat_id: None,
            character_id: Some(&character_id_owned),
            params: crate::image_gen::image_attempt::ImageAttemptParams {
                overrides: ImageGenOverrides {
                    n: Some(1.0),
                    style: Some("natural".to_string()),
                    ..Default::default()
                },
                orientation: Some(Orientation::Portrait),
                log_context: ImageParamsLogContext {
                    context: PREVIEW_LOG_CONTEXT,
                    ..Default::default()
                },
                fallback_model: crate::image_gen::image_attempt::DEFAULT_IMAGE_MODEL,
                declarations_for: &image_declarations_for,
            },
            prebuilt: None,
        },
    );
    let api_keys = crate::services::dangerous_content::provider_routing::DbApiKeys(db.clone());
    let understudy = crate::services::dangerous_content::image_failover::ImageUnderstudySource {
        db,
        api_keys: &api_keys,
        user_id,
        uncensored_image_profile_id: concierge_policy.desk.image_profile_id.as_deref(),
    };
    let failover = match crate::services::dangerous_content::image_failover::generate_image_with_concierge_failover(
        (
            crate::services::dangerous_content::image_failover::FailoverProfile::from_row(&profile),
            api_key,
        ),
        |candidate, key| attempt.call(candidate, key),
        &crate::services::dangerous_content::image_failover::ImageFailoverContext {
            db,
            user_id,
            chat_id: None,
            chat: None,
            purpose: crate::services::dangerous_content::image_failover::ImagePurpose::Avatar,
            concierge_policy: &concierge_policy,
            understudy: &understudy,
            profile_kind: crate::services::route_trail::RouteProfileKind::Image,
            primary_via: crate::services::route_trail::RouteAttemptVia::Primary,
            announce_unresolved_refusal: true,
        },
    )
    .await
    {
        Ok(f) => f,
        Err(failure) => {
            let trail = failure
                .concierge_trail()
                .map(<[crate::services::route_trail::RouteAttempt]>::to_vec);
            let refused = trail.as_ref().is_some_and(|t| {
                t.iter().any(|r| {
                    r.outcome == crate::services::route_trail::RouteAttemptOutcome::Refused
                })
            });
            let concierge_trail = trail.as_ref().map(|t| {
                Value::Array(
                    t.iter()
                        .map(|r| json!({ "profileName": r.profile_name, "outcome": r.outcome.as_str() }))
                        .collect(),
                )
                .to_string()
            });
            tracing::warn!(
                context = PREVIEW_LOG_CONTEXT,
                characterId = %parsed.character_id,
                error = %failure.error.message,
                refused = refused,
                conciergeTrailJson = concierge_trail.as_deref(),
                "[Avatar Preview] Image generation failed"
            );
            // v4 `errorResponse(refused ? 'The image provider declined to draw this
            // portrait' : `Image generation failed: ${message}`, refused ? 422 :
            // 502, { trail, refused })` — the web edge maps the 502 off
            // `details.refused === false` (the images route's idiom).
            let details = json!({
                "trail": trail
                    .as_ref()
                    .map(|t| serde_json::to_value(t).unwrap_or(Value::Null)),
                "refused": refused,
            });
            let kind = if refused {
                ErrorKind::Unprocessable
            } else {
                ErrorKind::Internal
            };
            let message = if refused {
                "The image provider declined to draw this portrait".to_string()
            } else {
                format!("Image generation failed: {}", failure.error.message)
            };
            let mut response = Response::error(kind, message);
            if let Response::Error(e) = &mut response {
                e.details = Some(Box::new(details));
            }
            return response;
        }
    };

    // v4 `decodeProviderImage(failover.result, `avatar_preview_${safeName}`)`:
    // the decoder owns the minted name now (`_<Date.now()>.<ext>`, `.webp` after
    // the transcode).
    let safe_name: String = character_name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    let codec = renderer.codec();
    let decoded = crate::image_gen::image_attempt::decode_provider_image(
        &crate::image_gen::image_attempt::PixelCodecTranscoder(codec.as_ref()),
        Some(&failover.result),
        &format!("avatar_preview_{safe_name}"),
        0,
        crate::clock::now_unix_ms(),
    );
    let Some(decoded) = decoded else {
        return internal("Image provider returned no image data");
    };

    // Persist: the character-vault `images/history/` write + the `files` row.
    // Any failure in this block is v4's `serverError('Failed to save avatar
    // preview')` (its try/catch around exactly this region). Linked to the
    // character so it surfaces in the character's gallery, but NOT to a chat —
    // the caller may have no chat context, and even when they do, this preview
    // is intentionally not bound to it. No `description` label on the link or
    // the row (bug 132): the prompt is the account of record.
    let file_id = uuid::Uuid::new_v4().to_string();
    let model_name = failover.profile.model_name.clone();
    let character_id = parsed.character_id.clone();
    let now_owned = now.to_string();
    let uid_owned = user_id.to_string();
    let file_id_owned = file_id.clone();
    let prompt_owned = prompt.clone();
    let decoded_for_write = decoded.clone();

    let saved = with_both_conns(db, move |main, mount| {
        let written = crate::services::image_job_storage::write_character_avatar_to_vault(
            main,
            mount,
            &character_id,
            &decoded_for_write.filename,
            &decoded_for_write.buffer,
            &decoded_for_write.mime_type,
            None,
            crate::services::mount_index::normalize_blob_image::blob_codec_or_refusing(
                blob_webp.as_deref(),
            ),
        )?;
        // The vault bridge transcodes bitmap uploads; the row records the
        // post-transcode mime/size (v4 `createGeneratedFileRow(repos, { userId,
        // sha256, originalFilename, stored: written, width, height, linkedTo:
        // [characterId], tags: [characterId], generation })`).
        crate::files::generated_file_row::create_generated_file_row(
            main,
            &crate::files::generated_file_row::GeneratedFileRowInput {
                user_id: &uid_owned,
                id: Some(&file_id_owned),
                sha256: &decoded_for_write.sha256,
                original_filename: &decoded_for_write.filename,
                stored: &crate::files::generated_file_row::StoredImageBytes {
                    storage_key: written.storage_key.clone(),
                    stored_mime_type: written.stored_mime_type.clone(),
                    size_bytes: written.size_bytes as i64,
                },
                width: decoded_for_write.width,
                height: decoded_for_write.height,
                linked_to: vec![character_id.clone()],
                tags: vec![character_id.clone()],
                source: None,
                generation: Some(&crate::files::generated_file_row::GenerationRecord {
                    prompt: Some(prompt_owned.clone()),
                    model: Some(model_name.clone()),
                    revised_prompt: decoded_for_write.revised_prompt.clone(),
                }),
                generation_key: None,
                description: None,
                project_id: None,
                folder_path: None,
                now_iso: &now_owned,
            },
        )?;
        Ok(written.stored_mime_type)
    })
    .await;

    match saved {
        Ok(stored_mime_type) => {
            tracing::info!(
                context = PREVIEW_LOG_CONTEXT,
                characterId = %parsed.character_id,
                fileId = %file_id,
                profileId = %failover.profile.id,
                rerouted = failover.rerouted,
                "[Avatar Preview] Preview saved"
            );
            let trail = if failover.trail.is_empty() {
                Value::Null
            } else {
                serde_json::to_value(&failover.trail).unwrap_or(Value::Null)
            };
            Response::Wardrobe(json!({
                "fileId": file_id,
                "url": format!("/api/v1/files/{file_id}?action=download"),
                "mimeType": stored_mime_type,
                "prompt": prompt,
                "rerouted": failover.rerouted,
                "trail": trail,
            }))
        }
        Err(e) => {
            // v4 `logger.error('[Avatar Preview] Failed to save preview image',
            // { context, characterId }, error)` — the error as the record's
            // own `error` object.
            tracing::error!(
                context = PREVIEW_LOG_CONTEXT,
                characterId = %parsed.character_id,
                error = %e,
                "[Avatar Preview] Failed to save preview image"
            );
            internal("Failed to save avatar preview")
        }
    }
}

/// v4 `preview-avatar/route.ts` `LOG_CONTEXT`.
const PREVIEW_LOG_CONTEXT: &str = "api.v1.wardrobe.preview-avatar";

/// v4 `9ecebf986`: ONE sentence for "no usable profile" and "the profile has no
/// key" (`resolveAvatarImageProfile` already requires `apiKeyId`).
pub const NO_AVATAR_IMAGE_PROFILE_MESSAGE: &str =
    "No image profile is configured. Set one in Settings → Images before generating avatars.";

// === end P4.D283 ===

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

// ===========================================================================
// `?action=instructions` — the per-tier dressing instructions (v4 `b86bb1a5`)
// ===========================================================================

/// v4's `instructionsBodySchema`, defined identically in all four route files:
/// `z.object({ instructions: z.string().nullable() })`. REQUIRED and nullable —
/// `{}` FAILS. The routes do not catch the `ZodError`, so a failure surfaces as
/// the middleware's flat `Validation error` 400.
///
/// The tri-state reaches here intact (absent / explicit `null` / value): a
/// hand-built variant would collapse absent into null and make the `{}` arm
/// untestable (memory: `edge-must-decode-through-the-request-enum.md`).
pub(super) fn parse_instructions_body(
    instructions: &Option<Option<Value>>,
) -> Result<Option<String>, Response> {
    match instructions {
        // The key was absent — Zod's `.nullable()` is not `.optional()`.
        None => Err(bad_request("Validation error")),
        Some(None) => Ok(None),
        Some(Some(Value::String(s))) => Ok(Some(s.clone())),
        Some(Some(Value::Null)) => Ok(None),
        Some(Some(_)) => Err(bad_request("Validation error")),
    }
}

/// v4's shared post-write echo: `cleared = !instructions ||
/// instructions.trim().length === 0` (JS truthiness — `''` clears like `null`),
/// and the response carries `cleared ? null : instructions.trim()`.
pub(super) fn instructions_echo(instructions: Option<&str>) -> (bool, Value) {
    let cleared = instructions.is_none_or(|s| crate::jsstr::js_trim(s).is_empty());
    if cleared {
        (true, Value::Null)
    } else {
        (
            false,
            Value::String(crate::jsstr::js_trim(instructions.unwrap()).to_string()),
        )
    }
}

/// v4 `GET /api/v1/wardrobe?action=instructions` — the Quilltap General file
/// through the factory (`ensure: true` — the folder is ensured when General is
/// provisioned); an unprovisioned General reads `{ instructions: null }`.
pub async fn wardrobe_instructions_get(db: &Db) -> Response {
    factory::instructions_get_store(db, GENERAL_WARDROBE, None, USER_ID).await
}

/// v4 `POST /api/v1/wardrobe?action=instructions` — an unprovisioned General
/// clears as a harmless no-op and refuses a write with the fixed 500 sentence.
pub async fn wardrobe_instructions_set(db: &Db, instructions: &Option<Option<Value>>) -> Response {
    factory::instructions_set(db, GENERAL_WARDROBE, None, USER_ID, instructions).await
}

#[cfg(test)]
mod api_key_read_tests {
    //! P4.139: the avatar preview reads the image profile's key through v4's
    //! SCOPED fallback `findApiKeyByIdAndUserId` (`preview-avatar/route.ts:
    //! 78-84`) — a corrupt key is the repository line and `null`, so it is
    //! the route's 400, never the 500 v5 had answered.
    use super::*;
    use crate::services::api_key_service::test_instance::*;

    const CHARACTER: &str = "a1000139-0000-4000-8000-000000000001";
    const IP_BAD: &str = "a6000139-0000-4000-8000-0000000000bd";
    const IP_OK: &str = "a6000139-0000-4000-8000-00000000000c";

    fn preview(db: &Db, image_profile_id: &str) -> (Response, Vec<String>) {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        crate::test_support::captured_with(|| {
            rt.block_on(wardrobe_preview_avatar(
                db,
                &ErasedAvatarPreview::none(),
                USER,
                serde_json::json!({
                    "characterId": CHARACTER,
                    "equippedSlots": {},
                    "imageProfileId": image_profile_id,
                }),
                NOW,
                None,
            ))
        })
    }

    #[test]
    fn a_corrupt_image_profile_key_is_the_scoped_line_and_the_400() {
        let (_dir, db) = provisioned(|c| {
            c.execute(
                "INSERT INTO characters (id, userId, name, createdAt, updatedAt) \
                 VALUES (?1, ?2, 'Probe', ?3, ?3)",
                rusqlite::params![CHARACTER, USER, NOW],
            )
            .unwrap();
            for (id, key) in [(IP_BAD, BAD_KEY), (IP_OK, OK_KEY)] {
                c.execute(
                    "INSERT INTO image_profiles (id, userId, name, provider, modelName, \
                     apiKeyId, createdAt, updatedAt) VALUES (?1, ?2, ?1, 'OPENAI', \
                     'gpt-image-1', ?3, ?4, ?4)",
                    rusqlite::params![id, USER, key, NOW],
                )
                .unwrap();
            }
        });
        let (resp, lines) = preview(&db, IP_BAD);
        match resp {
            Response::Error(e) => {
                assert_eq!(e.kind, crate::api::types::ErrorKind::BadRequest);
                assert_eq!(e.message, "API key for image profile is missing or invalid");
            }
            other => panic!("expected the 400, got {other:?}"),
        }
        assert_eq!(db_lines(&lines), vec![scoped_line(BAD_KEY, USER)]);
        // A healthy key passes the gate (the unwired renderer refuses later)
        // and logs no repository line.
        let (resp, lines) = preview(&db, IP_OK);
        if let Response::Error(e) = &resp {
            assert_ne!(e.message, "API key for image profile is missing or invalid");
        }
        assert!(db_lines(&lines).is_empty(), "{lines:?}");
    }
}
