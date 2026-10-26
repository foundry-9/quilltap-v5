//! Generate a picture of an Inventory object — synchronously, in the calling
//! route, through the Concierge's image failover chokepoint, as the
//! wardrobe's item pictures are (v4 `lib/inventory/item-image-generation.ts`,
//! `4fbdad186`; P4.D324).
//!
//! The subject is the object's `imageDescription`, then its `description`,
//! then its title, then its name (spec §2): an object-icon still life — one
//! object, centred, plain ground, no person. The editor has no chat, so there
//! is no Locked state, no announcement and no ledger; the trail is returned
//! for the editor. Log rows are `INVENTORY_ITEM_IMAGE`, counted as image
//! spend.
//!
//! The wardrobe twin's differences (`services::wardrobe_item_image_
//! generation`): NO component leaves, NO chat, purpose `dialog`, `square`,
//! `natural`, the LANTERN aesthetic. The generation's own lines run on the
//! caller's thread; the picture write's lines run inside the write closure.

use std::sync::Arc;

use serde_json::Value;

use super::errors::InventoryError;
use super::item_images::{add_inventory_item_image, AddInventoryItemImageInput, InventoryItemHome};
use super::types::{InventoryFields, InventoryScope};
use crate::db::runtime::Db;
use crate::db::DbError;
use crate::image_gen::image_attempt::{
    decode_provider_image, make_logged_image_attempt, ImageAttemptParams,
    LoggedImageAttemptOptions, PixelCodecTranscoder, DEFAULT_IMAGE_MODEL,
};
use crate::image_gen::Orientation;
use crate::jsstr::{js_trim, utf16_len, utf16_truncate};
use crate::services::dangerous_content::image_failover::{
    generate_image_with_concierge_failover, FailoverProfile, ImageFailoverContext, ImagePurpose,
    ImageUnderstudySource,
};
use crate::services::dangerous_content::provider_routing::DbApiKeys;
use crate::services::llm_logging::log_type;
use crate::services::route_trail::{
    RouteAttempt, RouteAttemptOutcome, RouteAttemptVia, RouteProfileKind,
};
use crate::services::wardrobe_image_bridge::WardrobeImageKind;
use crate::services::wardrobe_item_image_generation::WardrobeItemImageSeams;
use crate::services::wardrobe_item_images::service::ImageMint;

const LOG_CONTEXT: &str = "inventory.item-image-generation";

/// v4 `FIGURE_AESTHETIC_MAX_CHARS` (`avatar-prompt.ts:119`).
const FIGURE_AESTHETIC_MAX_CHARS: usize = 600;

/// v4 `NoInventoryImageProfileError`'s message (a 400).
pub const NO_INVENTORY_IMAGE_PROFILE_MESSAGE: &str =
    "No image profile is configured. Set one in Settings → Images before generating pictures of objects.";

/// The generation's typed failures (v4's two error classes + whatever the
/// picture write lets propagate).
#[derive(Debug)]
pub enum InventoryImageGenerationFailure {
    /// v4 `NoInventoryImageProfileError` → the route's 400.
    NoProfile,
    /// v4 `InventoryImageGenerationError(message, trail, refused)`.
    Generation {
        message: String,
        trail: Option<Vec<RouteAttempt>>,
        refused: bool,
    },
    /// A write failure (the tombstone's `CharacterArchived` among them).
    Item(InventoryError),
}

impl From<InventoryError> for InventoryImageGenerationFailure {
    fn from(e: InventoryError) -> Self {
        InventoryImageGenerationFailure::Item(e)
    }
}

impl From<DbError> for InventoryImageGenerationFailure {
    fn from(e: DbError) -> Self {
        InventoryImageGenerationFailure::Item(e.into())
    }
}

/// What the picture shows (v4 `inventoryImageSubject`): `imageDescription ||
/// description || title` (each trimmed) `|| name`, trailing `[.!?]+`
/// stripped.
pub fn inventory_image_subject(item: &impl InventoryFields) -> String {
    let pick = |v: Option<&str>| v.map(js_trim).filter(|s| !s.is_empty()).map(str::to_string);
    let subject = pick(item.image_description())
        .or_else(|| pick(item.description()))
        .or_else(|| pick(item.title()))
        .unwrap_or_else(|| item.name().to_string());
    subject.trim_end_matches(['.', '!', '?']).to_string()
}

/// The object-icon prompt: one object, centred, plain ground, no person (v4
/// `buildInventoryItemImagePrompt`). Pure.
pub fn build_inventory_item_image_prompt(item: &impl InventoryFields) -> String {
    format!(
        "Still-life product photograph of {}. A single object, centred, shown whole on a plain neutral ground, even soft light, no person and no hands. The object is the subject.",
        inventory_image_subject(item)
    )
}

/// v4 `withArtDirection(prompt, aesthetic)` — the capped art-direction
/// preamble; a blank aesthetic leaves the prompt untouched.
fn with_art_direction(prompt: String, aesthetic: Option<&str>) -> String {
    let Some(trimmed) = aesthetic.map(js_trim).filter(|s| !s.is_empty()) else {
        return prompt;
    };
    let capped = if utf16_len(trimmed) > FIGURE_AESTHETIC_MAX_CHARS {
        utf16_truncate(trimmed, FIGURE_AESTHETIC_MAX_CHARS)
    } else {
        trimmed.to_string()
    };
    format!("Art direction (apply this overall style): {capped}\n\n{prompt}")
}

/// v4 `InventoryItemImageGenerationResult`.
#[derive(Debug, Clone)]
pub struct InventoryItemImageGenerationResult {
    pub file_id: String,
    pub prompt: String,
    pub profile_id: String,
    pub profile_name: String,
    pub rerouted: bool,
    pub trail: Option<Vec<RouteAttempt>>,
}

/// v4 `generateInventoryItemImage`'s args.
#[derive(Debug, Clone, Copy)]
pub struct GenerateInventoryItemImageParams<'a> {
    pub user_id: &'a str,
    pub home: &'a InventoryItemHome,
    pub image_profile_id: Option<&'a str>,
}

/// Generate, store and make current a picture of `home.item` (v4
/// `generateInventoryItemImage`): the tombstone FIRST (before any spend), the
/// profile (none / no key → the ONE 400 sentence), the Lantern aesthetic on
/// a project's store, ONE provider call through the logged attempt and the
/// failover chokepoint (purpose `dialog`, no chat), the decode, then
/// [`add_inventory_item_image`] in ONE `Db::write`.
pub async fn generate_inventory_item_image(
    db: &Db,
    seams: &WardrobeItemImageSeams,
    params: GenerateInventoryItemImageParams<'_>,
) -> Result<InventoryItemImageGenerationResult, InventoryImageGenerationFailure> {
    let GenerateInventoryItemImageParams {
        user_id,
        home,
        image_profile_id,
    } = params;

    // Refuse a tombstone before spending a provider call.
    {
        let location = home.location.clone();
        db.read_main(move |c| {
            location.writable_mount_point_id(c).map_err(|e| match e {
                InventoryError::Db(db) => db,
                InventoryError::CharacterArchived { character_id } => {
                    DbError::CharacterArchived { character_id }
                }
                other => DbError::Internal(other.to_string()),
            })
        })?;
    }

    let chat_settings: Option<Value> = db
        .read_main(|c| crate::db::chat_settings::find_by_user_id(c, user_id))
        .ok()
        .flatten();
    let profile = {
        let cs = chat_settings.clone();
        let uid = user_id.to_string();
        let override_id = image_profile_id.map(str::to_string);
        db.read_main(move |c| {
            Ok(
                crate::services::image_profile_resolution::resolve_wardrobe_image_profile(
                    c,
                    &uid,
                    cs.as_ref(),
                    override_id.as_deref(),
                ),
            )
        })?
    };
    let Some(profile) = profile else {
        return Err(InventoryImageGenerationFailure::NoProfile);
    };
    let Some(api_key_id) = profile
        .get("apiKeyId")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
    else {
        return Err(InventoryImageGenerationFailure::NoProfile);
    };
    let Some(api_key) =
        crate::services::api_key_service::read_api_key_scoped(db, api_key_id, user_id)
            .map(|k| k.key_value)
            .filter(|k| !k.is_empty())
    else {
        return Err(InventoryImageGenerationFailure::NoProfile);
    };

    let project_mount = if home.location.scope == InventoryScope::Project {
        crate::services::aesthetics::get_project_official_mount_point_id(
            db,
            home.location.id.as_deref(),
        )
        .await
    } else {
        None
    };
    let aesthetic = crate::services::aesthetics::resolve_aesthetic(
        db,
        crate::services::aesthetics::AestheticKind::Lantern,
        project_mount.as_deref(),
        None,
    )
    .await;
    let prompt = with_art_direction(
        build_inventory_item_image_prompt(&home.item),
        aesthetic.as_deref(),
    );
    let profile_id = profile
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    tracing::debug!(
        context = LOG_CONTEXT,
        itemId = %home.record.id(),
        scope = home.location.scope.as_str(),
        profileId = %profile_id,
        "[InventoryItemImage] Generating inventory object image"
    );

    let concierge_policy = crate::services::dangerous_content::resolver::resolve_concierge_settings(
        chat_settings.as_ref(),
        None,
    );
    let api_keys = DbApiKeys(db.clone());
    let item_id = home.record.id().to_string();
    let mut attempt = make_logged_image_attempt(
        db,
        &seams.provider,
        LoggedImageAttemptOptions {
            user_id,
            log_type: log_type::INVENTORY_ITEM_IMAGE,
            prompt: &prompt,
            primary_profile_id: &profile_id,
            chat_id: None,
            character_id: home.location.character_id.as_deref(),
            params: ImageAttemptParams {
                overrides: crate::image_gen::params_builder::ImageGenOverrides {
                    n: Some(1.0),
                    style: Some("natural".to_string()),
                    ..Default::default()
                },
                orientation: Some(Orientation::Square),
                // v4 `{ context, itemId }`: the builder's log context carries no
                // item key in v5 — the wardrobe twin's shape (its `itemId`
                // rides only the params builder's DEBUG).
                log_context: crate::image_gen::params_builder::ImageParamsLogContext {
                    context: LOG_CONTEXT,
                    ..Default::default()
                },
                fallback_model: DEFAULT_IMAGE_MODEL,
                declarations_for: &crate::image_gen_data::image_declarations_for,
            },
            prebuilt: None,
        },
    );
    let understudy = ImageUnderstudySource {
        db,
        api_keys: &api_keys,
        user_id,
        uncensored_image_profile_id: concierge_policy.desk.image_profile_id.as_deref(),
    };
    let failover = match generate_image_with_concierge_failover(
        (FailoverProfile::from_row(&profile), api_key),
        |candidate, key| attempt.call(candidate, key),
        &ImageFailoverContext {
            db,
            user_id,
            chat_id: None,
            chat: None,
            purpose: ImagePurpose::Dialog,
            concierge_policy: &concierge_policy,
            understudy: &understudy,
            profile_kind: RouteProfileKind::Image,
            primary_via: RouteAttemptVia::Primary,
            announce_unresolved_refusal: true,
        },
    )
    .await
    {
        Ok(f) => f,
        Err(failure) => {
            let trail = failure.concierge_trail().map(<[RouteAttempt]>::to_vec);
            let refused = trail
                .as_ref()
                .is_some_and(|t| t.iter().any(|r| r.outcome == RouteAttemptOutcome::Refused));
            tracing::warn!(
                context = LOG_CONTEXT,
                itemId = %item_id,
                error = %failure.error.message,
                refused = refused,
                "[InventoryItemImage] Inventory object image generation failed"
            );
            return Err(InventoryImageGenerationFailure::Generation {
                message: failure.error.message,
                trail,
                refused,
            });
        }
    };

    let decoded = decode_provider_image(
        &PixelCodecTranscoder(seams.codec.as_ref()),
        Some(&failover.result),
        "inventory",
        0,
        crate::clock::now_unix_ms(),
    );
    let Some(decoded) = decoded else {
        return Err(InventoryImageGenerationFailure::Generation {
            message: "The image provider returned no picture".to_string(),
            trail: (!failover.trail.is_empty()).then(|| failover.trail.clone()),
            refused: false,
        });
    };
    let model_name = failover.profile.model_name.clone();
    let written = {
        let location = home.location.clone();
        let record = home.record.clone();
        let item = home.item.clone();
        let uid = user_id.to_string();
        let prompt = prompt.clone();
        let codec = Arc::clone(&seams.codec);
        let mint = ImageMint::now();
        db.write(move |ws| {
            let main = ws.main().connection();
            let mount = ws
                .mount_index()
                .ok_or(DbError::PartitionUnavailable(
                    crate::write_partition::WriteDbTarget::MountIndex,
                ))?
                .connection();
            let blob_webp =
                crate::services::mount_index::normalize_blob_image::PixelCodecWebp(codec);
            let home = InventoryItemHome {
                location,
                record,
                item,
            };
            Ok(add_inventory_item_image(
                main,
                mount,
                &home,
                &AddInventoryItemImageInput {
                    user_id: &uid,
                    kind: WardrobeImageKind::Generated,
                    content: &decoded.buffer,
                    content_type: &decoded.mime_type,
                    width: decoded.width.map(|w| w as i64),
                    height: decoded.height.map(|h| h as i64),
                    generation_prompt: Some(&prompt),
                    generation_model: Some(&model_name),
                    generation_revised_prompt: decoded.revised_prompt.as_deref(),
                },
                &mint,
                &blob_webp,
            ))
        })
        .await?
    }?;
    tracing::info!(
        context = LOG_CONTEXT,
        itemId = %item_id,
        fileId = %written.file.id,
        profileId = %failover.profile.id,
        rerouted = failover.rerouted,
        "[InventoryItemImage] Inventory object image generated"
    );
    Ok(InventoryItemImageGenerationResult {
        file_id: written.file.id.clone(),
        prompt,
        profile_id: failover.profile.id.clone(),
        profile_name: failover.profile.name.clone(),
        rerouted: failover.rerouted,
        trail: (!failover.trail.is_empty()).then_some(failover.trail),
    })
}
