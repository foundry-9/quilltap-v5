//! The wardrobe item picture generation — v4 `lib/wardrobe/
//! item-image-generation.ts` (`7c8572869`, #82): generate, store and make
//! current a picture of one wardrobe item, the path the editor's Generate
//! button (`api::wardrobe_item_images::generate`) and the tool-queued job
//! (`services::wardrobe_item_image_job`) both take.
//!
//! v4's order, kept exactly (P4.D263 R-D): the item's mount FIRST (an
//! archived character's tombstone refuses before any spend) → the image
//! profile (override → designated → default; none, or its key row gone, is
//! the ONE 400 sentence) → the owner / the component leaves / the project
//! aesthetic mount → the prompt → the Concierge policy over the global
//! settings AND the chat a tool-queued picture was asked for in (bug 189, v4
//! `7a8d6dacc`: Locked never fails over, Unmoderated routes direct to the
//! uncensored desk, a refusal is ledgered and announced there; the editor's
//! Generate has no chat — `chatId: null` reads as Moderated with no
//! announcement and no ledger, the trail the only record) → ONE provider call
//! (`n: 1`, `style: 'natural'`, the prompt's orientation) through the shared
//! logged attempt (`image_gen::image_attempt`, v4 `9ecebf986`) and the image
//! failover chokepoint (purpose `wardrobe`) → `decode_provider_image` →
//! `add_wardrobe_item_image` (the bridge write, the `files` row — no
//! `description`, bug 132 — the frontmatter pointer, in ONE `Db::write`). Each
//! provider attempt writes ONE `llm_logs` row of type `WARDROBE_ITEM_IMAGE`
//! carrying the chat id, its error shape on a throw; the Almanack counts the
//! type as image spend (bug 196). No preview-avatar bypass, no `n > 1`, no
//! cache.
//!
//! The generation's OWN lines run on the caller's thread; the item-image
//! write's lines run inside the write closure and are pinned by
//! `wardrobe_item_images_tier2_equivalence`.

use std::sync::Arc;

use serde_json::Value;

use crate::db::files::FileRow;
use crate::db::runtime::Db;
use crate::db::DbError;
use crate::image_gen::image_attempt::{
    decode_provider_image, make_logged_image_attempt, ImageAttemptParams,
    LoggedImageAttemptOptions, PixelCodecTranscoder, DEFAULT_IMAGE_MODEL,
};
use crate::image_gen::Orientation;
use crate::model::image::ErasedImageGenerate;
use crate::services::dangerous_content::image_failover::{
    generate_image_with_concierge_failover, FailoverProfile, ImageFailoverContext, ImagePurpose,
    ImageUnderstudySource,
};
use crate::services::dangerous_content::provider_routing::DbApiKeys;
use crate::services::file_storage::PixelCodec;
use crate::services::llm_logging::log_type;
use crate::services::route_trail::{
    RouteAttempt, RouteAttemptOutcome, RouteAttemptVia, RouteProfileKind,
};
use crate::services::wardrobe_container::{scope_str, WardrobeItemHome};
use crate::services::wardrobe_image_bridge::WardrobeImageKind;
use crate::services::wardrobe_item_image_prompt::{
    build_wardrobe_item_image_prompt, WardrobeImageSubject,
};
use crate::services::wardrobe_item_images::service::{
    add_wardrobe_item_image, wardrobe_image_url, AddWardrobeItemImageInput, ImageMint,
    ItemImageError,
};
use crate::vault_overlay::WardrobeItem;

const LOG_CONTEXT: &str = "wardrobe.item-image-generation";

/// v4 `NO_WARDROBE_IMAGE_PROFILE_MESSAGE` — the copy the avatar path uses when
/// nothing is configured. ONE sentence for "no usable profile" AND "its key
/// row is gone" (the resolver already requires `apiKeyId`).
pub const NO_WARDROBE_IMAGE_PROFILE_MESSAGE: &str =
    "No image profile is configured. Set one in Settings → Images before generating wardrobe pictures.";

/// What the generation needs from the composing host (P4.D263 R-F): the image
/// provider and the pixel codec — the same pair `ImagesGenerateSeams` carries
/// (its prompt classifier is not used: this path does no pre-screen). The
/// blob normalization runs through the SAME codec (`PixelCodecWebp`, v4's one
/// `sharp`); the API key, the LLM-log writer and the clock read the instance
/// directly.
#[derive(Clone)]
pub struct WardrobeItemImageSeams {
    pub provider: ErasedImageGenerate,
    pub codec: Arc<dyn PixelCodec>,
}

impl WardrobeItemImageSeams {
    /// The route verb's seams, off the engine's images-generate pair.
    pub fn from_images_generate(seams: &crate::api::images::ImagesGenerateSeams) -> Self {
        WardrobeItemImageSeams {
            provider: seams.provider.clone(),
            codec: Arc::clone(&seams.codec),
        }
    }
}

/// The generation's typed failures (v4's two error classes + whatever the
/// item write lets propagate).
#[derive(Debug)]
pub enum WardrobeImageGenerationFailure {
    /// v4 `NoWardrobeImageProfileError` → the route's 400.
    NoProfile,
    /// v4 `WardrobeImageGenerationError(message, trail, refused)` → 422 when
    /// refused, else 502.
    Generation {
        message: String,
        trail: Option<Vec<RouteAttempt>>,
        refused: bool,
    },
    /// An item-image write failure — `Archived` is v4's `CharacterArchivedError`
    /// (→ 409); the rest propagates (→ 500).
    Item(ItemImageError),
}

impl From<ItemImageError> for WardrobeImageGenerationFailure {
    fn from(e: ItemImageError) -> Self {
        WardrobeImageGenerationFailure::Item(e)
    }
}

impl From<DbError> for WardrobeImageGenerationFailure {
    fn from(e: DbError) -> Self {
        WardrobeImageGenerationFailure::Item(e.into())
    }
}

/// v4 `WardrobeItemImageGenerationResult` (plus the stored `files` row, which
/// v4's route re-reads by id).
#[derive(Debug, Clone)]
pub struct WardrobeItemImageGenerationResult {
    pub file_id: String,
    pub url: String,
    pub prompt: String,
    pub subject: WardrobeImageSubject,
    pub profile_id: String,
    pub profile_name: String,
    pub rerouted: bool,
    pub trail: Option<Vec<RouteAttempt>>,
    pub item: Option<WardrobeItem>,
    pub file: FileRow,
}

/// v4 `generateWardrobeItemImage`'s `args`.
#[derive(Debug, Clone)]
pub struct GenerateWardrobeItemImageArgs<'a> {
    pub user_id: &'a str,
    pub home: &'a WardrobeItemHome,
    /// The container id from the request (the project id, for the aesthetic).
    pub container_id: Option<&'a str>,
    pub image_profile_id: Option<&'a str>,
    /// Bug 189 (v4 `7a8d6dacc`): the chat a tool-queued picture was asked for
    /// in, from the `WARDROBE_ITEM_IMAGE` job payload's `chatId`. Its Concierge
    /// state governs the call and it rides the `llm_logs` row; `None` (the
    /// editor's direct generation) reads as Moderated with no ledger.
    pub chat_id: Option<&'a str>,
}

fn orientation_str(o: Orientation) -> &'static str {
    match o {
        Orientation::Portrait => "portrait",
        Orientation::Landscape => "landscape",
        Orientation::Square => "square",
    }
}

/// v4 `resolveComponentLeaves(repos, home)` — an outfit's leaves, from its
/// container plus the shared archetypes (and, for a character, the
/// character's group / project stores through the component graph). Unknown
/// ids drop out; the item itself never counts.
fn resolve_component_leaves(
    main: &rusqlite::Connection,
    mount: &rusqlite::Connection,
    home: &WardrobeItemHome,
) -> Vec<Value> {
    let component_ids: Vec<String> = home
        .item
        .get("componentItemIds")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    if component_ids.is_empty() {
        return Vec::new();
    }
    let docs = crate::db::doc_mount_documents::DocMountDocumentsRepository::new(mount);
    let mut items_by_id: std::collections::HashMap<String, Value> =
        std::collections::HashMap::new();
    for i in &home.container_items {
        if let Some(id) = i.get("id").and_then(Value::as_str) {
            items_by_id.insert(id.to_string(), i.clone());
        }
    }
    if home.scope != crate::api::types::WardrobeContainerScope::General {
        match crate::db::archetype_wardrobe::read_general_wardrobe(main, &docs, true) {
            Ok(arches) => {
                for a in arches {
                    if let Some(id) = a.get("id").and_then(Value::as_str) {
                        items_by_id
                            .entry(id.to_string())
                            .or_insert_with(|| a.clone());
                    }
                }
            }
            Err(e) => {
                let error = crate::db::fallback::error_text(&e);
                tracing::debug!(
                    context = LOG_CONTEXT,
                    error = %error,
                    "[WardrobeItemImage] Shared archetypes unavailable for component resolution"
                );
            }
        }
    }
    if home.scope == crate::api::types::WardrobeContainerScope::Character {
        if let Some(character_id) = home.character_id.as_deref() {
            // v4 `9ecebf986`: every tier the character can reach (group stores
            // included), walked from this outfit's parts only —
            // `componentGraph(loadWearablePool(repos, characterId, []), ids)`.
            // No project context reads as no project tier.
            let pool = crate::wearable_pool::load_wearable_pool(
                main,
                mount,
                character_id,
                Some(&[]),
                crate::wearable_pool::LoadWearablePoolOptions::default(),
            );
            for part in crate::wearable_pool::component_graph(&pool, &component_ids) {
                if let Some(id) = part.get("id").and_then(Value::as_str) {
                    items_by_id
                        .entry(id.to_string())
                        .or_insert_with(|| part.clone());
                }
            }
        }
    }
    let expanded =
        crate::wardrobe::expand_composites(&[home.item_id().to_string()], &items_by_id, None);
    // v4 `9ecebf986`: a cycle or a depth cut is a DEBUG, not a failure — the
    // leaves found so far still draw.
    if !expanded.cycles.is_empty() || expanded.truncated {
        // (Built outside the macro: `tracing`'s own `Value` trait shadows
        // `serde_json::Value` inside its expansion.)
        let cycles_json = serde_json::to_string(&expanded.cycles).unwrap_or_default();
        tracing::debug!(
            context = LOG_CONTEXT,
            itemId = %home.item_id(),
            cyclesJson = %cycles_json,
            truncated = expanded.truncated,
            "[WardrobeItemImage] Outfit expansion stopped short"
        );
    }
    expanded
        .leaf_ids
        .iter()
        .filter(|id| id.as_str() != home.item_id())
        .filter_map(|id| items_by_id.get(id).cloned())
        .collect()
}

/// v4 `generateWardrobeItemImage(repos, args)` — generate, store and make
/// current a picture of `args.home.item`.
pub async fn generate_wardrobe_item_image(
    db: &Db,
    seams: &WardrobeItemImageSeams,
    args: &GenerateWardrobeItemImageArgs<'_>,
) -> Result<WardrobeItemImageGenerationResult, WardrobeImageGenerationFailure> {
    let home = args.home;
    let user_id = args.user_id;
    let started_at = crate::clock::now_unix_ms();

    // Refuse a tombstone before spending a provider call.
    {
        let h = home.clone();
        db.read_main(move |c| h.resolve_mount(c))?;
    }

    // The profile: override → designated → default. v4's
    // `chatSettings.findByUserId` is a fallback read (contract C2, survey B.5:
    // it logs v4's ERROR before its `null` — the read was swallowed silently).
    let chat_settings: Option<Value> = db
        .read_main(|c| {
            Ok(crate::db::chat_settings::find_by_user_id_or_none(
                c, user_id,
            ))
        })
        .unwrap_or_else(|e| {
            crate::db::chat_settings::log_find_by_user_id_failure(&e);
            None
        });
    let override_id = args.image_profile_id.map(str::to_string);
    let profile = {
        let cs = chat_settings.clone();
        let uid = user_id.to_string();
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
        return Err(WardrobeImageGenerationFailure::NoProfile);
    };
    let Some(api_key_id) = profile
        .get("apiKeyId")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
    else {
        return Err(WardrobeImageGenerationFailure::NoProfile);
    };
    // v4 `repos.connections.findApiKeyByIdAndUserId(profile.apiKeyId, userId)`
    // — the scoped fallback read (P4.139's class); no `key_value` → the same
    // 400.
    let api_key = crate::services::api_key_service::read_api_key_scoped(db, api_key_id, user_id)
        .map(|k| k.key_value)
        .filter(|k| !k.is_empty());
    let Some(api_key) = api_key else {
        return Err(WardrobeImageGenerationFailure::NoProfile);
    };

    // The owner (character scope), the outfit's leaves, the project aesthetic
    // mount (v4's `Promise.all`).
    let owner = resolve_owner(db, home).await;
    // A failed pool read is v4's rejected `Promise.all` — the route's 500 —
    // never a component-less prompt that still spends a provider call (the
    // `f5e953a3f` unification's §3 finding).
    let components = read_both(db, |main, mount| {
        Ok(resolve_component_leaves(main, mount, home))
    })?;
    let project_mount = if home.scope == crate::api::types::WardrobeContainerScope::Project {
        crate::services::aesthetics::get_project_official_mount_point_id(db, args.container_id)
            .await
    } else {
        None
    };
    let aesthetic = crate::services::aesthetics::resolve_aesthetic(
        db,
        crate::services::aesthetics::AestheticKind::Aurora,
        project_mount.as_deref(),
        None,
    )
    .await;

    let built = build_wardrobe_item_image_prompt(
        &home.item,
        &components,
        owner.as_ref(),
        aesthetic.as_deref(),
    );
    let profile_id = profile
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();

    tracing::debug!(
        context = LOG_CONTEXT,
        itemId = %home.item_id(),
        scope = scope_str(home.scope),
        containerId = args.container_id.unwrap_or("null"),
        subject = built.subject.as_str(),
        orientation = orientation_str(built.orientation),
        componentCount = components.len(),
        profileId = %profile_id,
        profileOverride = args.image_profile_id.is_some_and(|s| !s.is_empty()),
        "[WardrobeItemImage] Generating wardrobe item image"
    );

    // Bug 189 (v4 `7a8d6dacc`): the chat a tool-queued picture was asked for
    // in governs the call. `loadChat` is `repos.chats.findById` — a `safeQuery`
    // read, so a failed read logs v4's `Error finding entity by ID` and reads
    // as no chat (the `[WardrobeItemImage] Chat unreadable; the failover will
    // re-read its Concierge state` WARN around it is UNREACHABLE in v4 and is
    // not emitted here — pinned NEGATIVE by the tier-3 family). `null` (the
    // editor's Generate) reads as Moderated with no ledger.
    let chat_id = args.chat_id.filter(|c| !c.is_empty());
    let chat: Option<Value> = match chat_id {
        Some(cid) => {
            let cid = cid.to_string();
            db.read_main(move |c| Ok(crate::db::chats_read::find_by_id_or_none(c, &cid)))
                .ok()
                .flatten()
        }
        None => None,
    };
    let concierge_policy = crate::services::dangerous_content::resolver::resolve_concierge_settings(
        chat_settings.as_ref(),
        chat.as_ref(),
    );
    tracing::debug!(
        context = LOG_CONTEXT,
        itemId = %home.item_id(),
        chatId = chat_id.unwrap_or("null"),
        chatFound = chat.is_some(),
        conciergeState = concierge_policy.state.as_str(),
        routeDirect = concierge_policy.route_direct,
        "[WardrobeItemImage] Concierge policy resolved"
    );

    // An Unmoderated chat goes straight to the uncensored desk, as the avatar
    // job does: the verdict is already in. Logged whether or not it rerouted.
    let api_keys = DbApiKeys(db.clone());
    let mut primary_profile = FailoverProfile::from_row(&profile);
    let mut primary_key = api_key.clone();
    if concierge_policy.route_direct {
        let original = crate::services::dangerous_content::provider_routing::RouteProfile {
            id: primary_profile.id.clone(),
            name: primary_profile.name.clone(),
            provider: primary_profile.provider.clone(),
            model_name: primary_profile.model_name.clone(),
            base_url: profile
                .get("baseUrl")
                .and_then(Value::as_str)
                .map(str::to_string),
            is_dangerous_compatible: profile
                .get("isDangerousCompatible")
                .and_then(Value::as_bool)
                == Some(true),
        };
        let route = {
            let (orig_key, policy, uid, keys) = (
                api_key.clone(),
                concierge_policy.clone(),
                user_id.to_string(),
                DbApiKeys(db.clone()),
            );
            db.read_main(move |conn| {
                Ok(
                    crate::services::dangerous_content::provider_routing::resolve_image_provider_for_dangerous_content(
                        conn, &keys, &original, &orig_key, &policy, &uid,
                    ),
                )
            })?
        };
        if route.rerouted {
            primary_profile = FailoverProfile {
                id: route.image_profile.id.clone(),
                name: route.image_profile.name.clone(),
                provider: route.image_profile.provider.clone(),
                model_name: route.image_profile.model_name.clone(),
                row: Value::Null,
            };
            primary_key = route.api_key.clone();
        }
        tracing::info!(
            context = LOG_CONTEXT,
            itemId = %home.item_id(),
            chatId = chat_id.unwrap_or("null"),
            rerouted = route.rerouted,
            profile = %primary_profile.name,
            "[WardrobeItemImage] Unmoderated chat: routed direct to the uncensored desk"
        );
    }

    // Wardrobe pictures keep their own log type; the Almanack counts it as
    // image spend all the same (bug 196, `IMAGE_SPEND_LOG_TYPES`). v4
    // `9ecebf986`: the shared `makeLoggedImageAttempt` — `primaryProfileId` is
    // the RESOLVED profile (not a direct-routed `primaryProfile`), so a
    // direct-routed Unmoderated call logs its first row as `(Concierge
    // reroute)`, the avatar job's `effectiveImageProfile` semantics; the row
    // carries the chat a tool-queued picture was asked for in (bug 189).
    let orientation = built.orientation;
    let mut attempt = make_logged_image_attempt(
        db,
        &seams.provider,
        LoggedImageAttemptOptions {
            user_id,
            log_type: log_type::WARDROBE_ITEM_IMAGE,
            prompt: &built.prompt,
            primary_profile_id: &profile_id,
            // The filtered id: a payload with no `chatId` (the editor's Generate)
            // writes `llm_logs.chatId` NULL as v4's `undefined` does, never `''`.
            chat_id,
            character_id: home.character_id.as_deref(),
            params: ImageAttemptParams {
                overrides: crate::image_gen::params_builder::ImageGenOverrides {
                    n: Some(1.0),
                    style: Some("natural".to_string()),
                    ..Default::default()
                },
                orientation: Some(orientation),
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
    // Bug 189: the chokepoint receives the chat — Locked never fails over, a
    // refusal is ledgered and announced there — and `primaryVia: 'concierge'`
    // when the direct route swapped the desk.
    let primary_via = if primary_profile.id != profile_id {
        RouteAttemptVia::Concierge
    } else {
        RouteAttemptVia::Primary
    };
    let failover = match generate_image_with_concierge_failover(
        (primary_profile, primary_key),
        |candidate, key| attempt.call(candidate, key),
        &ImageFailoverContext {
            db,
            user_id,
            chat_id,
            chat: chat.as_ref(),
            purpose: ImagePurpose::Wardrobe,
            concierge_policy: &concierge_policy,
            understudy: &understudy,
            profile_kind: RouteProfileKind::Image,
            primary_via,
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
            let concierge_trail = trail.as_ref().map(|t| {
                Value::Array(
                    t.iter()
                        .map(|r| {
                            serde_json::json!({
                                "profileName": r.profile_name,
                                "outcome": r.outcome.as_str(),
                                "detail": r.detail,
                            })
                        })
                        .collect(),
                )
                .to_string()
            });
            tracing::warn!(
                context = LOG_CONTEXT,
                itemId = %home.item_id(),
                error = %failure.error.message,
                refused = refused,
                conciergeTrailJson = concierge_trail.as_deref(),
                "[WardrobeItemImage] Wardrobe item image generation failed"
            );
            return Err(WardrobeImageGenerationFailure::Generation {
                message: failure.error.message,
                trail,
                refused,
            });
        }
    };

    // v4 `decodeProviderImage(failover.result, 'wardrobe')` (`9ecebf986`): the
    // base64 → WebP transcode over the seams' codec, the real dimensions, the
    // sha256, the `wardrobe_<ms>.<ext>` filename (NOT row-visible — the bridge's
    // leaf name becomes `originalFilename`); null → the sentence below.
    let decoded = decode_provider_image(
        &PixelCodecTranscoder(seams.codec.as_ref()),
        Some(&failover.result),
        "wardrobe",
        0,
        crate::clock::now_unix_ms(),
    );
    let Some(decoded) = decoded else {
        return Err(WardrobeImageGenerationFailure::Generation {
            message: "The image provider returned no picture".to_string(),
            trail: (!failover.trail.is_empty()).then(|| failover.trail.clone()),
            refused: false,
        });
    };
    let model_name = failover.profile.model_name.clone();

    let written = {
        let h = home.clone();
        let uid = user_id.to_string();
        let prompt = built.prompt.clone();
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
            Ok(add_wardrobe_item_image(
                main,
                mount,
                &h,
                &AddWardrobeItemImageInput {
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
        itemId = %home.item_id(),
        fileId = %written.file.id,
        bytes = written.file.size,
        subject = built.subject.as_str(),
        profileId = %failover.profile.id,
        rerouted = failover.rerouted,
        durationMs = crate::clock::now_unix_ms() - started_at,
        "[WardrobeItemImage] Wardrobe item image generated"
    );

    Ok(WardrobeItemImageGenerationResult {
        file_id: written.file.id.clone(),
        url: wardrobe_image_url(&written.file.id),
        prompt: built.prompt,
        subject: built.subject,
        profile_id: failover.profile.id.clone(),
        profile_name: failover.profile.name.clone(),
        rerouted: failover.rerouted,
        trail: (!failover.trail.is_empty()).then_some(failover.trail),
        item: written.item,
        file: written.file,
    })
}

/// Both READ connections at once, on the caller's thread (the main pool, then
/// the mount-index pool inside it) — the generation's reads never occupy the
/// writer, and their lines stay visible to the capture rig.
fn read_both<T>(
    db: &Db,
    f: impl FnOnce(&rusqlite::Connection, &rusqlite::Connection) -> Result<T, DbError>,
) -> Result<T, DbError> {
    db.read_main(|main| db.read_mount_index(|mount| f(main, mount)))
}

/// v4 `resolveOwner(repos, home)` — the wearer, for a character's own item. A
/// read failure (a broken vault) costs the figure, not the picture: WARN and
/// a catalogue shot.
async fn resolve_owner(db: &Db, home: &WardrobeItemHome) -> Option<Value> {
    if home.scope != crate::api::types::WardrobeContainerScope::Character {
        return None;
    }
    let character_id = home.character_id.clone()?;
    match read_both(db, |main, mount| {
        crate::db::characters_read::find_by_id(main, mount, &character_id)
    }) {
        Ok(owner) => owner,
        Err(e) => {
            let error = crate::db::fallback::error_text(&e);
            tracing::warn!(
                context = LOG_CONTEXT,
                characterId = %character_id,
                error = %error,
                "[WardrobeItemImage] Owner unreadable; falling back to a catalogue shot"
            );
            None
        }
    }
}
