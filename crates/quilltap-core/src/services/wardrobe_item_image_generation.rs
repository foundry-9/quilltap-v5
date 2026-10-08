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
//! settings and NO chat → ONE provider call (`n: 1`, `style: 'natural'`, the
//! prompt's orientation) through the image failover chokepoint with `chatId:
//! null` (so no announcement, no Locked state, no ledger row — purpose
//! `wardrobe`) → the first image to WebP → `add_wardrobe_item_image` (the
//! bridge write, the `files` row, the frontmatter pointer, in ONE
//! `Db::write`). Each provider attempt writes ONE `llm_logs` row of type
//! `WARDROBE_ITEM_IMAGE`, its error shape on a throw. No preview-avatar
//! bypass, no `n > 1`, no cache.
//!
//! The generation's OWN lines run on the caller's thread; the item-image
//! write's lines run inside the write closure and are pinned by
//! `wardrobe_item_images_tier2_equivalence`.

use std::sync::Arc;

use serde_json::Value;

use crate::db::files::FileRow;
use crate::db::runtime::Db;
use crate::db::DbError;
use crate::image_gen::Orientation;
use crate::model::image::{ErasedImageGenerate, ImageGenResponse};
use crate::services::dangerous_content::image_failover::{
    generate_image_with_concierge_failover, FailoverProfile, ImageFailoverContext, ImagePurpose,
    ImageUnderstudySource,
};
use crate::services::dangerous_content::provider_routing::DbApiKeys;
use crate::services::file_storage::PixelCodec;
use crate::services::llm_logging::{
    log_llm_call, log_type, LogContext, LogLlmCallParams, LogRequest, LogRequestMessage,
    LogResponse,
};
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
            let tiers = crate::wardrobe_tiers::shared_wardrobe_tiers_for_character(
                main,
                mount,
                character_id,
                &[],
            );
            hydrate_component_graph(main, &docs, character_id, &mut items_by_id, &tiers);
        }
    }
    let expanded =
        crate::wardrobe::expand_composites(&[home.item_id().to_string()], &items_by_id, None);
    expanded
        .leaf_ids
        .iter()
        .filter(|id| id.as_str() != home.item_id())
        .filter_map(|id| items_by_id.get(id).cloned())
        .collect()
}

/// v4 `hydrateComponentGraph` (`lib/wardrobe/hydrate-components.ts`) — fill in
/// every component reachable from the items already in `items_by_id`, a level
/// at a time, bounded by the depth `expand_composites` walks. A failed level
/// WARNs and stops (a component we can't fetch degrades to an unresolvable
/// leaf).
///
/// ⚠ A LOCAL port: `tools/wardrobe_shared.rs`' twin is private and that file
/// is P4.D262's this round — a `pub(crate)` there is a recorded `HANDOFF:`
/// (the lane record), after which this copy folds onto it. This copy logs
/// v4's camelCase keys; the twin still logs snake_case ones (the HANDOFF
/// names that too).
fn hydrate_component_graph(
    main: &rusqlite::Connection,
    docs: &crate::db::doc_mount_documents::DocMountDocumentsRepository,
    character_id: &str,
    items_by_id: &mut std::collections::HashMap<String, Value>,
    tiers: &crate::wardrobe_tiers::SharedWardrobeTiers,
) {
    let mut requested: std::collections::HashSet<String> = std::collections::HashSet::new();
    for depth in 0..crate::wardrobe::COMPOSITE_MAX_DEPTH {
        let mut wanted: Vec<String> = Vec::new();
        for item in items_by_id.values() {
            for component in item
                .get("componentItemIds")
                .and_then(Value::as_array)
                .map(Vec::as_slice)
                .unwrap_or_default()
            {
                let Some(cid) = component.as_str() else {
                    continue;
                };
                if items_by_id.contains_key(cid) || !requested.insert(cid.to_string()) {
                    continue;
                }
                wanted.push(cid.to_string());
            }
        }
        if wanted.is_empty() {
            return;
        }
        match crate::db::wardrobe_read::find_by_ids_for_character(
            main,
            docs,
            character_id,
            &wanted,
            tiers,
        ) {
            Ok(components) => {
                if components.is_empty() {
                    return;
                }
                for c in components {
                    if let Some(id) = c.get("id").and_then(Value::as_str) {
                        items_by_id.insert(id.to_string(), c);
                    }
                }
            }
            Err(e) => {
                let error = crate::db::fallback::error_text(&e);
                tracing::warn!(
                    context = "wardrobe",
                    characterId = %character_id,
                    depth = depth,
                    wantedCount = wanted.len(),
                    error = %error,
                    "[hydrateComponentGraph] Component hydration failed"
                );
                return;
            }
        }
    }
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
    // `chatSettings.findByUserId` is a fallback read.
    let chat_settings: Option<Value> = db
        .read_main(|c| crate::db::chat_settings::find_by_user_id(c, user_id))
        .ok()
        .flatten();
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
    let components = {
        read_both(db, |main, mount| {
            Ok(resolve_component_leaves(main, mount, home))
        })
        .unwrap_or_default()
    };
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

    let concierge_policy = crate::services::dangerous_content::resolver::resolve_concierge_settings(
        chat_settings.as_ref(),
        None,
    );

    let prompt = built.prompt.clone();
    let orientation = built.orientation;
    let character_id = home.character_id.clone();
    let primary_profile_id = profile_id.clone();
    let attempt = |candidate: FailoverProfile, key: String| {
        let prompt = prompt.clone();
        let character_id = character_id.clone();
        let primary_profile_id = primary_profile_id.clone();
        async move {
            let built = crate::image_gen::params_builder::build_image_gen_params(
                crate::image_gen::params_builder::ImageProfileLike {
                    provider: &candidate.provider,
                    model_name: candidate.row.get("modelName").and_then(Value::as_str),
                    parameters: candidate.row.get("parameters"),
                },
                &prompt,
                &crate::image_gen::params_builder::ImageGenOverrides {
                    n: Some(1.0),
                    style: Some("natural".to_string()),
                    ..Default::default()
                },
                Some(orientation),
                "dall-e-3",
                &crate::image_gen_data::image_declarations_for(&candidate.provider),
                &crate::image_gen::params_builder::ImageParamsLogContext {
                    context: LOG_CONTEXT,
                    profile_id: Some(candidate.id.clone()),
                    ..Default::default()
                },
            );
            let call_started = crate::clock::now_unix_ms();
            let result = seams
                .provider
                .generate_image(&candidate.provider, &key, &built.params)
                .await;
            let (content, error) = match &result {
                Ok(r) => (success_content(r, candidate.id != primary_profile_id), None),
                Err(e) => (String::new(), Some(e.message.clone())),
            };
            log_llm_call(
                db,
                LogLlmCallParams {
                    user_id: user_id.to_string(),
                    log_type: log_type::WARDROBE_ITEM_IMAGE.to_string(),
                    message_id: None,
                    chat_id: None,
                    character_id,
                    provider: candidate.provider.clone(),
                    model_name: candidate.model_name.clone(),
                    connection_profile_id: None,
                    image_profile_id: Some(candidate.id.clone()),
                    request: LogRequest {
                        messages: vec![LogRequestMessage {
                            role: "user".to_string(),
                            content: prompt.clone(),
                            attachments: None,
                        }],
                        ..Default::default()
                    },
                    response: LogResponse {
                        content,
                        error,
                        ..Default::default()
                    },
                    usage: None,
                    cache_usage: None,
                    raw_provider_usage: None,
                    request_hashes: None,
                    duration_ms: Some((crate::clock::now_unix_ms() - call_started) as f64),
                },
                &LogContext::none(),
            )
            .await;
            result
        }
    };

    let api_keys = DbApiKeys(db.clone());
    let understudy = ImageUnderstudySource {
        db,
        api_keys: &api_keys,
        user_id,
        uncensored_image_profile_id: concierge_policy.desk.image_profile_id.as_deref(),
    };
    let failover = match generate_image_with_concierge_failover(
        (FailoverProfile::from_row(&profile), api_key),
        attempt,
        &ImageFailoverContext {
            db,
            user_id,
            chat_id: None,
            chat: None,
            purpose: ImagePurpose::Wardrobe,
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

    // v4 `imageData?.data || imageData?.b64Json` — the decoders fold both
    // vendor spellings into `data`.
    let image = failover.result.images.first();
    let Some((image, raw)) = image.and_then(|i| {
        i.data
            .as_deref()
            .filter(|s| !s.is_empty())
            .map(|raw| (i, raw))
    }) else {
        return Err(WardrobeImageGenerationFailure::Generation {
            message: "The image provider returned no picture".to_string(),
            trail: (!failover.trail.is_empty()).then(|| failover.trail.clone()),
            refused: false,
        });
    };

    let provider_mime = image
        .mime_type
        .as_deref()
        .filter(|s| !s.is_empty())
        .unwrap_or("image/png")
        .to_string();
    let subtype = provider_mime
        .split('/')
        .nth(1)
        .filter(|s| !s.is_empty())
        .unwrap_or("png");
    let converted = crate::services::file_storage::convert_to_webp(
        seams.codec.as_ref(),
        &crate::services::image_job_common::decode_base64_node(raw),
        &provider_mime,
        &format!("wardrobe.{subtype}"),
    );
    let revised = image.revised_prompt.clone().filter(|s| !s.is_empty());
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
                    content: &converted.buffer,
                    content_type: &converted.mime_type,
                    width: converted.width,
                    height: converted.height,
                    generation_prompt: Some(&prompt),
                    generation_model: Some(&model_name),
                    generation_revised_prompt: revised.as_deref(),
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

/// v4 `response.images?.[0]?.revisedPrompt || \`Generated ${n} image(s)${rerouted
/// ? ' (Concierge reroute)' : ''}\``.
fn success_content(response: &ImageGenResponse, rerouted: bool) -> String {
    response
        .images
        .first()
        .and_then(|i| i.revised_prompt.clone())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| {
            format!(
                "Generated {} image(s){}",
                response.images.len(),
                if rerouted { " (Concierge reroute)" } else { "" }
            )
        })
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
