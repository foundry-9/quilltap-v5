//! The `CHARACTER_AVATAR_GENERATION` job handler (v4
//! `lib/background-jobs/handlers/character-avatar.ts`).
//!
//! Looks the configuration up in the avatar cache — a HIT binds the existing
//! image and stops, spending nothing — and otherwise generates a
//! head-and-shoulders portrait for a character from their equipped wardrobe +
//! physical descriptions, runs it through the Concierge, generates the image,
//! and persists it into the character's vault, then updates
//! `chats.characterAvatars` + `characters.avatarOverrides` and posts the Lantern
//! notification.
//!
//! ## Every avatar goes to the character's vault (v4 `7fbf8a55b`)
//!
//! Project context or not. Reading a mount blob is addressed by blob id with no
//! project scoping, so a chat in any project can render a `fileId` whose bytes
//! live in the vault — which is what lets the configuration cache be shared
//! across projects without hard-linking anything. The project-store upload
//! branch and the legacy `folders` find-or-create are GONE from this path (the
//! `folders` table backs the pre-Scriptorium file tree and is only meaningful
//! for disk-backed or project-mount-backed writes), and the vault-missing
//! refusal is now unconditional. `common::ProjectImageUpload` itself stays — the
//! story-background job still uploads into a project store.
//!
//! ## Model boundaries (tier-3 seams — the same ones the v4 oracle mocks)
//!   - [`ImageProvider`] — `provider.generateImage(params, key)`.
//!   - [`CompletionProvider`] + [`ModerationProvider`] — the Concierge pre-scan.
//!   - [`ApiKeyResolver`] — the profile's decrypted API key.
//!   - [`ImageTranscoder`] — the WebP transcode (`convertToWebP`).
//!
//! ## Deferrals (documented seams)
//!   - `logLLMCall` — v4 fire-and-forgets an `IMAGE_GENERATION` llm-logs row
//!     (never awaited); the host logging service is deferred (the `generate_image`
//!     precedent). Not emitted.
//!   - Aesthetics: aurora ONLY, no depiction guidelines (the Ariel Clause
//!     deliberately does not apply to avatars).

use serde_json::{json, Value};

use crate::clock::iso_from_unix_ms;
use crate::db::runtime::Db;
use crate::db::DbError;
use crate::image_gen::Orientation;
use crate::model::completion::CompletionProvider;
use crate::model::image::{ImageProvider, ImageTranscoder, TranscodeInput};
use crate::services::aesthetics::{resolve_aesthetic, AestheticKind};
use crate::services::avatar_prompt::{build_character_avatar_prompt, AvatarPromptOptions};
use crate::services::dangerous_content::gatekeeper::{classify_content, ModerationProvider};
use crate::services::dangerous_content::provider_routing::{
    resolve_image_provider_for_dangerous_content, ApiKeyResolver, RouteProfile,
};
use crate::services::image_job_common as common;
use crate::services::image_job_storage::write_character_avatar_to_vault;
use crate::services::lantern_notifications::{
    post_lantern_image_notification, LanternNotificationKind, LanternPostParams,
};
use crate::wardrobe::Slots;

/// v4's `context` for this handler (`character-avatar.ts` passes it in every one
/// of its nineteen log bags) — byte-exact, because it is what an operator greps
/// `combined.log` for. The two `…concierge-route` / `…concierge-reroute`
/// variants are the IMAGE-PARAMS builder's log contexts, not this one, and stay
/// spelled out at their call sites.
const CONTEXT: &str = "background-jobs.character-avatar";

/// The tracing target the handler's shared-path lines already use
/// (`image_job_common`'s three `RerouteHandler::CharacterAvatar` arms).
const LOG_TARGET: &str = "quilltap::character_avatar";

/// v4's `logger.error('[CharacterAvatar] Prompt classification failed,
/// continuing normally', { context, jobId, error })` — `character-avatar.ts:292`.
///
/// v4's `try` (`:243-297`) wraps BOTH `classifyDangerousContent` AND
/// `resolveImageProviderForDangerousContent`. v5's `classify_content`
/// (`dangerous_content::gatekeeper`) is INFALLIBLE by signature — it returns
/// `DangerClassificationResult` and folds its own inner error into
/// `safe_fallback()` — so the classifier half has no v5 branch. The resolver
/// half does: its `read_main` can fail, and that arm (below, at the routing
/// read) is where the sentence fires in v5, with the error attached as v4
/// attaches the exception. It has a silence leg; a FIRING pin is not
/// plantable through the seed, because the resolver reads `image_profiles`
/// and the job's own profile read reaches that table first (recorded at the
/// `1fefadb9a` round's unification).
///
/// The eighteen other `character-avatar.ts` sites all fire (fourteen landed
/// by P4.93, one by P4.D184, three on the shared `image_job_common` path).
const CLASSIFICATION_FAILED: &str =
    "[CharacterAvatar] Prompt classification failed, continuing normally";

/// The decoded `CHARACTER_AVATAR_GENERATION` payload.
#[derive(Clone, Debug)]
pub struct CharacterAvatarPayload {
    pub chat_id: String,
    pub character_id: String,
    pub image_profile_id: String,
    /// One-shot `{ top, bottom, footwear, accessories, hair }` override.
    pub equipped_slots_override: Option<Value>,
    /// Reroll: bypass the avatar configuration cache and generate
    /// unconditionally. Set by the manual regenerate button; automatic triggers
    /// leave it unset (v4 `7fbf8a55b`).
    pub force: bool,
}

impl CharacterAvatarPayload {
    /// Decode from the raw job payload JSON (missing/invalid → an error the
    /// runner surfaces as a failure).
    pub fn from_json(payload: &Value) -> Result<Self, String> {
        let chat_id = payload
            .get("chatId")
            .and_then(Value::as_str)
            .ok_or("payload missing chatId")?
            .to_string();
        let character_id = payload
            .get("characterId")
            .and_then(Value::as_str)
            .ok_or("payload missing characterId")?
            .to_string();
        let image_profile_id = payload
            .get("imageProfileId")
            .and_then(Value::as_str)
            .ok_or("payload missing imageProfileId")?
            .to_string();
        let equipped_slots_override = payload
            .get("equippedSlotsOverride")
            .filter(|v| !v.is_null())
            .cloned();
        // v4 reads `payload.force` through `if (!payload.force)` — JS truthiness.
        // This read is a STRICT bool: absent, `false`, `null` and `0` leave the
        // cache on in both engines, and the one writer (`queue_service`) only
        // ever writes a literal `true`, so the shapes where the two rules would
        // differ (`1`, `"x"` — truthy in v4, not a bool here) are unreachable.
        // Recorded rather than widened: a truthy-string reroll is not a contract.
        let force = payload
            .get("force")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        Ok(Self {
            chat_id,
            character_id,
            image_profile_id,
            equipped_slots_override,
            force,
        })
    }
}

/// The injected seams the avatar handler needs.
pub struct AvatarJobDeps<'a, I, C, M, A, T> {
    pub image_provider: &'a I,
    pub completion: &'a C,
    pub moderation: &'a M,
    pub api_keys: &'a A,
    pub transcoder: &'a T,
    /// Injected wall clock (v4's frozen `Date`): drives the `avatar_<ts>` filename
    /// AND every handler-minted timestamp (`files.createdAt`, the
    /// `characterAvatars.generatedAt`), so they match v4 byte-for-byte.
    pub now_ms: i64,
    /// The plugin-registry orientation seam (v4's `getImageGenerationModels` +
    /// `getImageProviderConstraints`).
    pub declarations_for: &'a common::ImageDeclarationsFn,
    /// P4.104: the encoder the vault/Lantern blob write normalizes through
    /// (v4 `linkBlobContent`, `186eb09cb`). `None` → the refusing encoder, v4's
    /// `sharp`-threw arm; the host wires its `HostImageCodec`.
    pub blob_webp:
        Option<std::sync::Arc<dyn crate::services::mount_index::blob_transcode::WebpTranscoder>>,
}

/// v4 `handleCharacterAvatarGeneration`. Returns `Ok(())` on success or a benign
/// skip (WARN+RETURN), `Err(msg)` on a throw (the runner marks the job failed).
pub async fn handle_character_avatar_generation<I, C, M, A, T>(
    db: &Db,
    deps: &AvatarJobDeps<'_, I, C, M, A, T>,
    user_id: &str,
    payload: &CharacterAvatarPayload,
    // v4 folds `jobId: job.id` into this handler's image-params log context
    // (`84f33ce94`). Log-only, but the runner has it in hand, so it is threaded
    // rather than dropped.
    job_id: &str,
) -> Result<(), String>
where
    I: ImageProvider,
    C: CompletionProvider,
    M: ModerationProvider,
    A: ApiKeyResolver,
    T: ImageTranscoder,
{
    // v4 `:102` — the handler announces itself before it touches the database,
    // so a job that dies on a missing chat still leaves its own first line.
    tracing::info!(
        target: LOG_TARGET,
        context = CONTEXT,
        job_id = job_id,
        chat_id = %payload.chat_id,
        character_id = %payload.character_id,
        "[CharacterAvatar] Starting avatar generation"
    );

    let chat_id = payload.chat_id.clone();
    let character_id = payload.character_id.clone();

    // 1. Load chat (throw if missing).
    let chat = db
        .read_main(move |conn| crate::db::chats_read::find_by_id(conn, &chat_id))
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Chat not found: {}", payload.chat_id))?;

    // 2. Load character (vault-overlaid; throw if missing).
    let cid = character_id.clone();
    let character = common::with_both_conns(db, move |main, mount| {
        crate::db::characters_read::find_by_id(main, mount, &cid)
    })
    .await
    .map_err(|e| e.to_string())?
    .ok_or_else(|| format!("Character not found: {}", payload.character_id))?;
    let character_name = common::str_field(&character, "name")
        .unwrap_or("")
        .to_string();

    // 3. Load image profile (throw if missing).
    let pid = payload.image_profile_id.clone();
    let image_profile = db
        .read_main(move |conn| crate::db::image_profiles::find_by_id(conn, &pid))
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Image profile not found: {}", payload.image_profile_id))?;

    // apiKeyId / apiKey — WARN+RETURN (a benign skip) when absent. v4 `:128`
    // and `:138`: two DIFFERENT sentences with two different bags, because the
    // operator's next move differs — one profile has no key attached, the other
    // points at a key that no longer resolves.
    let Some(api_key_id) = common::owned_field(&image_profile, "apiKeyId") else {
        tracing::warn!(
            target: LOG_TARGET,
            context = CONTEXT,
            job_id = job_id,
            profile_id = common::str_field(&image_profile, "id").unwrap_or(""),
            "[CharacterAvatar] Image profile has no API key, skipping"
        );
        return Ok(());
    };
    let Some(api_key) = deps
        .api_keys
        .resolve(&api_key_id, user_id)
        .filter(|k| !k.is_empty())
    else {
        // v4's bag here names NO profile — `apiKey?.key_value` is what failed,
        // and the id it was looked up by is already in the line above when that
        // one fired.
        tracing::warn!(
            target: LOG_TARGET,
            context = CONTEXT,
            job_id = job_id,
            "[CharacterAvatar] API key not found or invalid, skipping"
        );
        return Ok(());
    };

    // 4. Aurora character aesthetic (project-over-global). NO depiction guidelines.
    let project_id_opt = common::owned_field(&chat, "projectId");
    let project_official = crate::services::aesthetics::get_project_official_mount_point_id(
        db,
        project_id_opt.as_deref(),
    )
    .await;
    let character_aesthetic =
        resolve_aesthetic(db, AestheticKind::Aurora, project_official.as_deref(), None).await;

    // 5-7. Resolve equipped slots + project mounts + build the portrait prompt.
    let equipped_override = payload.equipped_slots_override.clone();
    let character_for_prompt = character.clone();
    let chat_id_for_prompt = payload.chat_id.clone();
    let character_id_for_prompt = payload.character_id.clone();
    let prompt_result = common::with_both_conns(db, move |main, mount| {
        // equippedSlots = override ?? getEquippedOutfitForCharacter(chat, char).
        let equipped_slots: Option<Slots> = match &equipped_override {
            Some(over) => Some(Slots::from_value(Some(over))),
            None => {
                let stored = crate::db::chats_outfits::ChatOutfitsRepository::new(main)
                    .get_equipped_outfit_for_character(
                        &chat_id_for_prompt,
                        &character_id_for_prompt,
                    )?;
                stored.as_ref().map(|v| Slots::from_value(Some(v)))
            }
        };
        let project_mount_point_ids =
            crate::tools::wardrobe_shared::resolve_project_mount_point_ids_for_chat(
                main,
                mount,
                &chat_id_for_prompt,
            );
        let docs = crate::db::doc_mount_documents::DocMountDocumentsRepository::new(mount);
        build_character_avatar_prompt(
            main,
            mount,
            &docs,
            &character_for_prompt,
            &AvatarPromptOptions {
                equipped_slots,
                project_mount_point_ids,
                character_aesthetic: character_aesthetic.clone(),
            },
        )
    })
    .await
    .map_err(|e| e.to_string())?;

    // Kept for the cache-hit info line (v4 logs `leafCounts` beside the
    // reused file id).
    let leaf_counts = prompt_result.leaf_counts;
    let prompt = prompt_result.prompt;
    if !prompt_result.has_appearance {
        // No appearance data — WARN+RETURN (v4 `:167`).
        tracing::warn!(
            target: LOG_TARGET,
            context = CONTEXT,
            job_id = job_id,
            character_id = %payload.character_id,
            "[CharacterAvatar] No appearance data available, skipping"
        );
        return Ok(());
    }

    // 7b. The avatar configuration cache (v4 `7fbf8a55b`).
    //
    // Built from the ORIGINAL profile, before the Concierge classification
    // below, so a hit skips that LLM call as well as the image call, the WebP
    // transcode and the file write. A Concierge reroute therefore stores its
    // image under the originally-requested key — correct (same inputs, same
    // outcome), though it means a cached row's `generationModel` need not match
    // its key's model.
    //
    // These params are reused verbatim for generation on a miss; only a reroute
    // rebuilds them, since the fallback provider's shape mechanism, LoRA support
    // and stored options are all its own.
    let original_profile_id = common::str_field(&image_profile, "id")
        .unwrap_or("")
        .to_string();
    let original_provider = common::str_field(&image_profile, "provider")
        .unwrap_or("")
        .to_string();
    let avatar_params = common::build_job_image_params(
        &original_provider,
        common::str_field(&image_profile, "modelName").unwrap_or(""),
        &image_profile
            .get("parameters")
            .cloned()
            .unwrap_or(Value::Null),
        &prompt,
        Orientation::Portrait,
        deps.declarations_for,
        "background-jobs.character-avatar",
        Some(&payload.chat_id),
        Some(job_id),
        &original_profile_id,
    );
    let cache_keys = crate::services::avatar_cache::derive_avatar_cache_keys(
        &original_provider,
        &original_profile_id,
        &avatar_params.to_key_value(),
    );

    if !payload.force {
        let keys = cache_keys.clone();
        let cid = payload.character_id.clone();
        let cached = common::with_both_conns(db, move |main, mount| {
            Ok(crate::services::avatar_cache::lookup_cached_avatar(
                main, mount, &keys, &cid,
            ))
        })
        .await
        .map_err(|e| e.to_string())?;

        if let Some(cached) = cached {
            bind_avatar_to_chat(
                db,
                &chat,
                &character,
                &payload.chat_id,
                &payload.character_id,
                &cached.id,
                &iso_from_unix_ms(deps.now_ms),
            )
            .await?;

            // No Lantern notification: nothing was produced. The avatar still
            // reaches the Salon through the normal realtime path.
            // P4.93: onto the shared constants, and its four camelCase field
            // names respelled snake_case. The SENTENCE, level and target are
            // unmoved (the `avatar_job_tier3` pin reads the sentence); this is
            // the field convention the other 347 `tracing::` sites under
            // `services/` use, including `image_job_common`'s three
            // `[CharacterAvatar]` lines, and the fourteen new ones below.
            tracing::info!(
                target: LOG_TARGET,
                context = CONTEXT,
                job_id = job_id,
                chat_id = %payload.chat_id,
                character_id = %payload.character_id,
                file_id = %cached.id,
                // v4 logs `leafCounts` as an OBJECT; through the file layer's
                // `…Json` convention (P4.91) it lands as one, not as a `Debug`
                // string.
                leafCountsJson = %serde_json::json!({
                    "top": leaf_counts.top,
                    "bottom": leaf_counts.bottom,
                    "footwear": leaf_counts.footwear,
                    "accessories": leaf_counts.accessories,
                    "hair": leaf_counts.hair,
                })
                .to_string(),
                "[CharacterAvatar] Reused cached avatar for this configuration"
            );
            return Ok(());
        }
    }

    // 8. Concierge pre-scan (chatSettings → danger settings; classify + reroute).
    let chat_settings = db
        .read_main(move |conn| crate::db::chat_settings::find_by_user_id(conn, user_id))
        .map_err(|e| e.to_string())?;
    let danger_settings = common::resolve_danger_settings_for_chat(chat_settings.as_ref(), &chat);

    // Effective profile (may be rerouted). Track id/provider/model/params/key.
    let mut eff_id = common::str_field(&image_profile, "id")
        .unwrap_or("")
        .to_string();
    let mut eff_provider = common::str_field(&image_profile, "provider")
        .unwrap_or("")
        .to_string();
    let mut eff_model = common::str_field(&image_profile, "modelName")
        .unwrap_or("")
        .to_string();
    let mut eff_params = image_profile
        .get("parameters")
        .cloned()
        .unwrap_or(Value::Null);
    let mut eff_api_key = api_key.clone();

    if danger_settings.mode != "OFF" && danger_settings.scan_image_prompts {
        // Build the cheap-LLM selection for classification. v4 wraps
        // `resolveCheapLLMSelectionForUser` in a try/catch and WARNS on a throw
        // (`:237`), leaving `cheapLLMSelection` null so the classification is
        // skipped. v5's `build_cheap_llm_selection` is infallible, so the only
        // thing that can fail here is the profiles READ — which is the same
        // failure wearing Rust's clothes, and it used to be swallowed whole by
        // `unwrap_or_default()`. A successful read that simply finds no eligible
        // profile answers `None` and stays SILENT, exactly as v4's
        // `resolved?.selection ?? null` does.
        let all_profiles = match db.read_main(crate::db::connection_profiles::find_all) {
            Ok(profiles) => profiles,
            Err(e) => {
                tracing::warn!(
                    target: LOG_TARGET,
                    context = CONTEXT,
                    job_id = job_id,
                    error = %e,
                    "[CharacterAvatar] Failed to build cheap LLM selection for danger classification"
                );
                Vec::new()
            }
        };
        let cheap_settings = chat_settings
            .as_ref()
            .and_then(|cs| cs.get("cheapLLMSettings"));
        if let Some(selection) = common::build_cheap_llm_selection(&all_profiles, cheap_settings) {
            // classify_content never throws (safe_fallback on error).
            let classification = classify_content(
                db,
                deps.moderation,
                deps.completion,
                &prompt,
                &selection,
                user_id,
                &danger_settings,
                Some(&payload.chat_id),
            )
            .await;
            // v4 logs the verdict on `isDangerous` ALONE (`:255`) and only then
            // asks about the mode (`:274`/`:282` live inside the AUTO_ROUTE
            // arm). v5 had collapsed the two into one conjunction, which is
            // routing-equivalent but silent in DETECT_ONLY — where the verdict
            // is the only thing the operator gets.
            if classification.is_dangerous {
                // v4 logs `categories` as an ARRAY of names; a `?`-formatted
                // `Vec` would render Rust's `Debug`, so it rides the file
                // layer's `…Json` convention (P4.91) and lands as structure.
                let category_names: Vec<&str> = classification
                    .categories
                    .iter()
                    .map(|c| c.category.as_str())
                    .collect();
                let categories_json = serde_json::to_string(&category_names).unwrap_or_default();
                tracing::info!(
                    target: LOG_TARGET,
                    context = CONTEXT,
                    job_id = job_id,
                    score = classification.score,
                    categoriesJson = %categories_json,
                    mode = %danger_settings.mode,
                    "[CharacterAvatar] Avatar prompt classified as dangerous"
                );
            }
            if classification.is_dangerous && danger_settings.mode == "AUTO_ROUTE" {
                let original = RouteProfile {
                    id: eff_id.clone(),
                    name: common::str_field(&image_profile, "name")
                        .unwrap_or("")
                        .to_string(),
                    provider: eff_provider.clone(),
                    model_name: eff_model.clone(),
                    base_url: common::owned_field(&image_profile, "baseUrl"),
                };
                let orig_key = api_key.clone();
                let mode = danger_settings.mode.clone();
                let uncensored = danger_settings.uncensored_image_profile_id.clone();
                let uid = user_id.to_string();
                let api_keys = deps.api_keys;
                let route = match db.read_main(move |conn| {
                    Ok(resolve_image_provider_for_dangerous_content(
                        conn,
                        api_keys,
                        &original,
                        &orig_key,
                        &mode,
                        uncensored.as_deref(),
                        &uid,
                    ))
                }) {
                    Ok(route) => Some(route),
                    Err(e) => {
                        // v4 `:292` — the one v5 branch of its catch (see
                        // `CLASSIFICATION_FAILED`): the routing read failed,
                        // the avatar carries on unrouted.
                        tracing::error!(
                            target: LOG_TARGET,
                            context = CONTEXT,
                            job_id = job_id,
                            error = %e,
                            "{}",
                            CLASSIFICATION_FAILED
                        );
                        None
                    }
                };
                if let Some(route) = route {
                    if route.rerouted {
                        // v4 `:274` — the ORIGINAL profile's NAME, not its id
                        // (this pair of lines is what an operator reads to see
                        // which desk the portrait actually went to). Read before
                        // the swap below, which overwrites `eff_*`.
                        tracing::info!(
                            target: LOG_TARGET,
                            context = CONTEXT,
                            job_id = job_id,
                            original_profile =
                                common::str_field(&image_profile, "name").unwrap_or(""),
                            uncensored_profile = %route.image_profile.name,
                            reason = %route.reason,
                            "[CharacterAvatar] Rerouted to uncensored image provider"
                        );
                        eff_id = route.image_profile.id.clone();
                        eff_provider = route.image_profile.provider.clone();
                        eff_model = route.image_profile.model_name.clone();
                        eff_params = common::load_profile_parameters(db, &eff_id).await;
                        eff_api_key = route.api_key.clone();
                    } else {
                        // v4 `:282` — the resolver looked and found nothing, so
                        // the original desk keeps the job. The reason is the
                        // resolver's own sentence.
                        tracing::warn!(
                            target: LOG_TARGET,
                            context = CONTEXT,
                            job_id = job_id,
                            reason = %route.reason,
                            "[CharacterAvatar] No uncensored image provider available, using original"
                        );
                    }
                }
            }
        }
    }

    // 9. Generate the portrait (with the post-hoc moderation reroute).
    //
    // Reuse the params the cache key was derived from. A pre-generation
    // Concierge reroute swaps the profile, and the fallback provider's shape
    // mechanism, LoRA support and stored options are its own — so that case, and
    // only that case, rebuilds (v4's `effectiveImageProfile.id ===
    // imageProfile.id ? avatarParams : buildImageGenParams(...)`). Building once
    // is also what keeps the `[Image LoRA]` lines firing once per attempt.
    let prebuilt_params = if eff_id == original_profile_id {
        Some(avatar_params)
    } else {
        None
    };
    // The INITIAL build's log context. v4 builds once, up front, under
    // `background-jobs.character-avatar` (the params the cache key was derived
    // from, reused verbatim); a PRE-generation Concierge profile swap is the one
    // case that rebuilds, and v4 tags that rebuild `…concierge-route`
    // (`character-avatar.ts:317-326`). `prebuilt_params` is `None` exactly then.
    let initial_build_log_context = if prebuilt_params.is_some() {
        "background-jobs.character-avatar"
    } else {
        "background-jobs.character-avatar.concierge-route"
    };
    let outcome = common::generate_with_reroute(
        db,
        deps.image_provider,
        deps.api_keys,
        &eff_id,
        &eff_provider,
        &eff_model,
        &eff_params,
        &eff_api_key,
        prebuilt_params,
        &prompt,
        Orientation::Portrait,
        deps.declarations_for,
        &danger_settings.mode,
        danger_settings.uncensored_image_profile_id.as_deref(),
        user_id,
        Some(&payload.chat_id),
        Some(&payload.character_id),
        "Avatar image generation failed",
        initial_build_log_context,
        "background-jobs.character-avatar.concierge-reroute",
        Some(job_id),
        // [cc65d6bfc] v4's avatar handler is UNTOUCHED by bug 133: its reroute
        // is gated on the moderation error alone, with no chat-state conjunct.
        common::RerouteHandler::CharacterAvatar,
    )
    .await?;

    // No images / no data — WARN+RETURN. v4: `rawData = imageData.data ||
    // imageData.b64Json; if (!rawData)` — a JS falsy check, so a missing AND an
    // empty-string payload both no-op (W4.7f widened `data` to Option<String>).
    let Some(image_data) = outcome.images.into_iter().next() else {
        // v4 `:479`.
        tracing::warn!(
            target: LOG_TARGET,
            context = CONTEXT,
            job_id = job_id,
            "[CharacterAvatar] No images returned from provider"
        );
        return Ok(());
    };
    let raw_data = match image_data.data.as_deref() {
        Some(d) if !d.is_empty() => d,
        _ => {
            // v4 `:490` — a DIFFERENT sentence from the one above: the provider
            // did answer, the answer just carried no bytes.
            tracing::warn!(
                target: LOG_TARGET,
                context = CONTEXT,
                job_id = job_id,
                "[CharacterAvatar] Generated image has no data"
            );
            return Ok(());
        }
    };
    let generation_model = outcome.active_model;

    // 10. Decode + transcode + persist.
    let raw_buffer = common::decode_base64_node(raw_data);
    let provider_mime = image_data
        .mime_type
        .as_deref()
        .unwrap_or("image/png")
        .to_string();
    let provider_ext = provider_mime.split('/').nth(1).unwrap_or("png");
    let provider_filename = format!(
        "avatar_{}_{}.{provider_ext}",
        sanitize_name_for_filename(&character_name),
        deps.now_ms
    );
    let converted = deps.transcoder.transcode(&TranscodeInput {
        bytes: raw_buffer,
        mime_type: provider_mime.clone(),
        filename: provider_filename,
    });
    let now_iso = iso_from_unix_ms(deps.now_ms);
    let file_id = uuid::Uuid::new_v4().to_string();

    // No storage branch: every avatar goes to the character's vault, project
    // context or not (v4 `7fbf8a55b` — see the module doc).
    let write = AvatarWriteInput {
        user_id: user_id.to_string(),
        character_id: payload.character_id.clone(),
        chat_id: payload.chat_id.clone(),
        file_id: file_id.clone(),
        now_iso: now_iso.clone(),
        converted_bytes: converted.bytes.clone(),
        converted_mime: converted.mime_type.clone(),
        converted_filename: converted.filename.clone(),
        converted_width: converted.width,
        converted_height: converted.height,
        prompt: prompt.clone(),
        generation_model,
        revised_prompt: image_data.revised_prompt.clone(),
        generation_key: cache_keys.key.clone(),
        blob_webp: deps.blob_webp.clone(),
    };
    match common::with_both_conns(db, move |main, mount| {
        write_avatar_file(main, mount, &write)
    })
    .await
    {
        Ok(()) => tracing::info!(
            target: LOG_TARGET,
            context = CONTEXT,
            job_id = job_id,
            file_id = %file_id,
            "[CharacterAvatar] Avatar image saved"
        ),
        Err(e) => {
            // v4 `:589` logs at ERROR with the exception attached (v4's
            // `logger.error(msg, ctx, error)` renders it into the record's
            // `error` object; the visitor maps a tracing field named `error`
            // onto the same shape) and a bag of just `{context, jobId}`; the
            // message also becomes the job's failure text below.
            tracing::error!(
                target: LOG_TARGET,
                context = CONTEXT,
                job_id = job_id,
                error = %e,
                "[CharacterAvatar] Failed to save avatar image"
            );
            return Err(format!("Failed to save avatar image: {e}"));
        }
    }

    // 11-12. Bind the chat (and the character's per-chat override) to the new
    // avatar — the same helper the cache-hit path above used.
    bind_avatar_to_chat(
        db,
        &chat,
        &character,
        &payload.chat_id,
        &payload.character_id,
        &file_id,
        &now_iso,
    )
    .await?;

    // v4 `:604` — after the bind, before the Lantern post.
    tracing::info!(
        target: LOG_TARGET,
        context = CONTEXT,
        job_id = job_id,
        chat_id = %payload.chat_id,
        character_id = %payload.character_id,
        file_id = %file_id,
        "[CharacterAvatar] Avatar generation completed"
    );

    // 13. Lantern notification (avatar → sender aurora; the built prompt as aim).
    let _ = post_lantern_image_notification(
        db,
        LanternPostParams {
            chat_id: payload.chat_id.clone(),
            file_id,
            kind: LanternNotificationKind::Avatar {
                character_name: character_name.clone(),
            },
            prompt: Some(prompt),
        },
    )
    .await;

    Ok(())
}

/// Point a chat (and the character's per-chat override) at an avatar image
/// (v4 `bindAvatarToChat`, `7fbf8a55b`).
///
/// Shared by the generation path and the configuration-cache hit path: a reused
/// avatar must bind exactly the way a freshly drawn one does, or the two paths
/// drift and a cached avatar shows up in one surface but not the other.
///
/// `now_iso` is the handler's injected clock (v4 reads `new Date()` here; under
/// the frozen oracle clock they are the same instant).
async fn bind_avatar_to_chat(
    db: &Db,
    chat: &Value,
    character: &Value,
    chat_id: &str,
    character_id: &str,
    file_id: &str,
    now_iso: &str,
) -> Result<(), String> {
    // chat.characterAvatars — merge this character's entry into whatever is there.
    let message_count = chat
        .get("messageCount")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let existing_avatars = chat
        .get("characterAvatars")
        .filter(|v| v.is_object())
        .cloned()
        .unwrap_or_else(|| json!({}));
    let mut avatars = existing_avatars.as_object().cloned().unwrap_or_default();
    avatars.insert(
        character_id.to_string(),
        json!({
            "imageId": file_id,
            "generatedAt": now_iso,
            "afterMessageCount": message_count,
        }),
    );
    let avatars_value = Value::Object(avatars);
    let chat_id_upd = chat_id.to_string();
    db.write(move |writers| {
        let update = crate::db::chats::ChatUpdate {
            character_avatars: Some(avatars_value),
            ..Default::default()
        };
        writers.main().chats().update(&chat_id_upd, &update)?;
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?;

    // character.avatarOverrides — filter this chat out, push the new one.
    let existing_overrides = character
        .get("avatarOverrides")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut filtered: Vec<Value> = existing_overrides
        .into_iter()
        .filter(|o| o.get("chatId").and_then(Value::as_str) != Some(chat_id))
        .collect();
    filtered.push(json!({ "chatId": chat_id, "imageId": file_id }));
    let char_id_upd = character_id.to_string();
    let patch = {
        let mut m = serde_json::Map::new();
        m.insert("avatarOverrides".into(), Value::Array(filtered));
        m
    };
    common::with_both_conns(db, move |main, mount| {
        // avatarOverrides is a slim key — the P4.22 `Unavailable` refusal is
        // unreachable here; collapse to the closure's DbError.
        crate::db::vault_character_update::update_character(main, mount, &char_id_upd, &patch)
            .map_err(crate::db::document_store_overlay::OverlayError::into_db)
            .map(|_| ())
    })
    .await
    .map_err(|e| e.to_string())?;

    Ok(())
}

// ===========================================================================
// The registered JobHandler (the W4.8 runner pattern; the host wires it with
// real seams, removing CHARACTER_AVATAR_GENERATION from the loud fallback)
// ===========================================================================

/// A `CHARACTER_AVATAR_GENERATION` [`crate::services::job_runner::JobHandler`] over
/// owned seams. The host registers one built with the real image/completion/etc.
/// providers; the differential's runner E2E registers one built with canned seams.
pub struct CharacterAvatarGenerationHandler<I, C, M, A, T, F> {
    pub image_provider: I,
    pub completion: C,
    pub moderation: M,
    pub api_keys: A,
    pub transcoder: T,
    pub now_ms: i64,
    /// `Fn(provider) -> (models, provider-support)` (the plugin-registry seam).
    pub declarations_for: F,
    /// P4.104: the encoder the vault/Lantern blob write normalizes through
    /// (v4 `linkBlobContent`, `186eb09cb`). `None` → the refusing encoder, v4's
    /// `sharp`-threw arm; the host wires its `HostImageCodec`.
    pub blob_webp:
        Option<std::sync::Arc<dyn crate::services::mount_index::blob_transcode::WebpTranscoder>>,
}

impl<I, C, M, A, T, F> crate::services::job_runner::JobHandler
    for CharacterAvatarGenerationHandler<I, C, M, A, T, F>
where
    I: ImageProvider + Send + Sync,
    C: CompletionProvider + Send + Sync,
    M: ModerationProvider + Send + Sync,
    A: ApiKeyResolver + Send + Sync,
    T: ImageTranscoder + Send + Sync,
    F: Fn(&str) -> crate::image_gen::params_builder::ImageDeclarations + Send + Sync + 'static,
{
    fn handle<'a>(
        &'a self,
        db: &'a Db,
        job: &'a crate::db::background_jobs::BackgroundJob,
    ) -> crate::services::job_runner::JobFuture<'a> {
        Box::pin(async move {
            let payload_json: Value = serde_json::from_str(&job.payload).unwrap_or(Value::Null);
            let payload = match CharacterAvatarPayload::from_json(&payload_json) {
                Ok(p) => p,
                Err(e) => return crate::services::job_runner::JobOutcome::Failed(e),
            };
            let deps = AvatarJobDeps {
                image_provider: &self.image_provider,
                completion: &self.completion,
                moderation: &self.moderation,
                api_keys: &self.api_keys,
                transcoder: &self.transcoder,
                now_ms: self.now_ms,
                declarations_for: &self.declarations_for as &common::ImageDeclarationsFn,
                blob_webp: self.blob_webp.clone(),
            };
            match handle_character_avatar_generation(db, &deps, &job.user_id, &payload, &job.id)
                .await
            {
                Ok(()) => crate::services::job_runner::JobOutcome::Completed(None),
                Err(e) => crate::services::job_runner::JobOutcome::Failed(e),
            }
        })
    }
}

/// The owned inputs for the storage write closure.
struct AvatarWriteInput {
    user_id: String,
    character_id: String,
    chat_id: String,
    file_id: String,
    now_iso: String,
    converted_bytes: Vec<u8>,
    converted_mime: String,
    converted_filename: String,
    converted_width: Option<f64>,
    converted_height: Option<f64>,
    prompt: String,
    generation_model: String,
    revised_prompt: Option<String>,
    /// The v1 cache key of the REQUESTED profile — bound to the new row so this
    /// configuration is served from cache next time (v4 `7fbf8a55b`).
    generation_key: String,
    blob_webp:
        Option<std::sync::Arc<dyn crate::services::mount_index::blob_transcode::WebpTranscoder>>,
}

/// The storage half of v4 `handleCharacterAvatarGeneration`: the character vault
/// → the `files` row (linkedTo `[chatId, characterId]`, tags `[characterId]`).
///
/// v4 `7fbf8a55b` deleted the project branch: every avatar goes to the vault,
/// project context or not, so the vault-missing refusal is unconditional and no
/// legacy `folders` row is minted per image any more.
fn write_avatar_file(
    main: &rusqlite::Connection,
    mount: &rusqlite::Connection,
    input: &AvatarWriteInput,
) -> Result<(), DbError> {
    let sha256 = sha256_hex(&input.converted_bytes);

    // The vault is provisioned at character creation and re-asserted by the
    // startup backfill; if it is somehow missing we refuse to write rather than
    // leak bytes into the catch-all `_general/`.
    if crate::services::image_job_storage::resolve_character_vault_mount(
        main,
        mount,
        &input.character_id,
    )
    .is_none()
    {
        return Err(DbError::Internal(format!(
            "Character {} has no linked database-backed vault; cannot persist wardrobe avatar.",
            input.character_id
        )));
    }
    let written = write_character_avatar_to_vault(
        main,
        mount,
        &input.character_id,
        &input.converted_filename,
        &input.converted_bytes,
        &input.converted_mime,
        // Bug 132: no label on the vault link either — see the note on the
        // `files` row below.
        None,
        crate::services::mount_index::normalize_blob_image::blob_codec_or_refusing(
            input.blob_webp.as_deref(),
        ),
    )?;
    let (storage_key, stored_mime, stored_size) = (
        written.storage_key,
        written.stored_mime_type,
        written.size_bytes,
    );

    let files = crate::db::files::FilesRepository::new(main);
    files.create(
        &crate::db::files::FileCreate {
            user_id: input.user_id.clone(),
            sha256,
            original_filename: input.converted_filename.clone(),
            mime_type: stored_mime,
            size: stored_size as f64,
            width: input.converted_width,
            height: input.converted_height,
            is_plain_text: None,
            linked_to: vec![input.chat_id.clone(), input.character_id.clone()],
            source: "GENERATED".to_string(),
            category: "IMAGE".to_string(),
            generation_prompt: Some(input.prompt.clone()),
            generation_model: Some(input.generation_model.clone()),
            generation_revised_prompt: input.revised_prompt.clone(),
            // Bind this configuration's cache key to the new image — last write
            // wins, which is exactly what makes a forced reroll the new
            // canonical portrait for this character in this outfit. Keyed on the
            // REQUESTED profile, not the rerouted one.
            generation_key: Some(input.generation_key.clone()),
            // No label here — see the matching note in `story_background_job.rs`
            // (bug 132).
            description: None,
            tags: vec![input.character_id.clone()],
            // No legacy `folders` row and no project scoping: the bytes live in
            // the vault, addressed by blob id.
            project_id: None,
            folder_path: None,
            storage_key: Some(storage_key),
            file_status: "ok".to_string(),
        },
        &crate::db::files::CreateOptions {
            id: input.file_id.clone(),
            created_at: input.now_iso.clone(),
            updated_at: input.now_iso.clone(),
        },
    )?;
    Ok(())
}

/// v4 `character.name.replace(/[^a-zA-Z0-9]/g, '_')` for the provider filename.
fn sanitize_name_for_filename(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect()
}

/// Lowercase hex SHA-256.
fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[cfg(test)]
mod payload_tests {
    use super::*;

    /// The `force` wire's read half: the manual regenerate writes a literal
    /// `true` (`queue_service::enqueue_character_avatar_generation`), and this
    /// is the only reader. Absent and `false` are one answer, as v4's
    /// `if (!payload.force)` makes them.
    #[test]
    fn force_reads_a_literal_true_and_nothing_else() {
        let base = serde_json::json!({
            "chatId": "c1", "characterId": "ch1", "imageProfileId": "p1"
        });
        let read = |extra: Option<Value>| {
            let mut v = base.clone();
            if let Some(f) = extra {
                v["force"] = f;
            }
            CharacterAvatarPayload::from_json(&v)
                .expect("decodes")
                .force
        };
        assert!(!read(None), "absent leaves the cache on");
        assert!(!read(Some(Value::Bool(false))));
        assert!(!read(Some(Value::Null)));
        assert!(read(Some(Value::Bool(true))), "the manual reroll's payload");
    }
}

// ===========================================================================
// P4.93 — the handler's log surface (v4 `character-avatar.ts`'s nineteen sites)
// ===========================================================================
//
// Log-only work is invisible to every other proof this repo has: the `files`
// row, `chats.characterAvatars`, the Lantern notification and the job's outcome
// are identical whether or not a sentence fires (the finding-#103/#110/#116
// class). So each line gets a capturing layer over the REAL handler, asserting
// the sentence, its LEVEL, and its whole field bag — and each gets a SILENCE
// leg, because a line that fires on every path says nothing.
//
// The instance is a REAL fresh-provisioned one (`provision_fresh_instance` —
// full `fresh_schema.json` DDL, the single user, the built-in mounts), with one
// character (vault and all), one chat and one image profile inserted on top.
// Hand-rolled DDL would have to track every schema move the port absorbs; this
// tracks them for free.
#[cfg(test)]
mod log_line_tests {
    use super::*;

    use crate::db::runtime::{Db, DbPaths};
    use crate::image_gen::params_builder::ImageDeclarations;
    use crate::model::completion::CannedCompletionProvider;
    use crate::model::image::{
        GeneratedImageData, ImageGenError, ImageGenParams, ImageGenResponse, ImageProvider,
        PassthroughTranscoder,
    };
    use crate::services::dangerous_content::gatekeeper::NoModerationProvider;

    /// The pepper `provision_fresh_instance`'s own self-test uses (never a real one).
    const PEPPER: &str = "3q2+796tvu/erb7v3q2+796tvu/erb7v3q2+796tvu8=";
    const USER: &str = "ffffffff-ffff-ffff-ffff-ffffffffffff";
    const CHAT: &str = "11111111-1111-4111-8111-111111111111";
    const CHARACTER: &str = "22222222-2222-4222-8222-222222222222";
    const PROFILE: &str = "33333333-3333-4333-8333-333333333333";
    const JOB: &str = "job-93";
    /// A 1x1 PNG, base64 — the provider's canned answer.
    const PNG_B64: &str =
        "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";

    struct Keys(Option<String>);
    impl crate::services::dangerous_content::provider_routing::ApiKeyResolver for Keys {
        fn resolve(&self, _api_key_id: &str, _user_id: &str) -> Option<String> {
            self.0.clone()
        }
    }

    /// What the one provider attempt answers.
    #[derive(Clone)]
    enum Answer {
        Images(Vec<GeneratedImageData>),
        Fails(String),
    }
    struct StubImages(Answer);
    impl ImageProvider for StubImages {
        fn generate_image(
            &self,
            _provider: &str,
            _api_key: &str,
            _params: &ImageGenParams,
        ) -> impl std::future::Future<Output = Result<ImageGenResponse, ImageGenError>> + Send
        {
            let answer = self.0.clone();
            async move {
                match answer {
                    Answer::Images(images) => Ok(ImageGenResponse { images }),
                    Answer::Fails(m) => Err(ImageGenError::new(m)),
                }
            }
        }
    }

    fn declarations(_provider: &str) -> ImageDeclarations {
        ImageDeclarations::default()
    }

    fn one_png() -> Answer {
        Answer::Images(vec![GeneratedImageData {
            data: Some(PNG_B64.to_string()),
            url: None,
            mime_type: Some("image/png".to_string()),
            revised_prompt: None,
        }])
    }

    /// How the seeded instance is arranged for one test.
    struct Arrangement {
        /// `None` → the profile row carries a NULL `apiKeyId` (v4 `:128`).
        profile_api_key_id: Option<&'static str>,
        /// `None` → the resolver finds no key for that id (v4 `:138`).
        resolved_key: Option<&'static str>,
        /// `false` → the vault carries no physical description (v4 `:167`).
        has_appearance: bool,
        /// The Concierge's `mode`. `"OFF"` (the default) skips the pre-scan
        /// entirely, so the three classification lines cannot fire.
        danger_mode: &'static str,
        /// `true` seeds a SECOND image profile and points the danger settings'
        /// `uncensoredImageProfileId` at it, so the AUTO_ROUTE resolver has
        /// somewhere to go (v4 `:274`); `false` leaves it nowhere (v4 `:282`).
        uncensored_profile: bool,
        /// Drop `connection_profiles` after seeding, so the profiles READ the
        /// cheap-LLM selection is built from FAILS (v4's `:237` catch).
        break_connection_profiles: bool,
        /// Refuse every INSERT into `files` through a BEFORE INSERT trigger, so
        /// the avatar WRITE fails while every read before it (the cache lookup
        /// only SELECTs `files`) still succeeds — the plant for v4's `:589`.
        break_files_insert: bool,
    }

    impl Default for Arrangement {
        fn default() -> Self {
            Self {
                profile_api_key_id: Some("key-1"),
                resolved_key: Some("sk-test"),
                has_appearance: true,
                danger_mode: "OFF",
                uncensored_profile: false,
                break_connection_profiles: false,
                break_files_insert: false,
            }
        }
    }

    const UNCENSORED_PROFILE: &str = "44444444-4444-4444-8444-444444444444";

    /// A moderation provider that flags everything — the moderation-FIRST arm of
    /// `classify_content`, which needs no completion call at all.
    struct FlagsEverything;
    impl crate::services::dangerous_content::gatekeeper::ModerationProvider for FlagsEverything {
        async fn moderate(
            &self,
            _content: &str,
            _user_id: &str,
            _settings: &crate::db::chat_settings::DangerousContentSettings,
            _chat_id: Option<&str>,
        ) -> crate::services::dangerous_content::gatekeeper::ModerationOutcome {
            use crate::services::dangerous_content::gatekeeper::{
                ModerationCategoryScore, ModerationOutcome, ModerationResult,
            };
            ModerationOutcome::Moderated {
                result: ModerationResult {
                    flagged: true,
                    categories: vec![ModerationCategoryScore {
                        category: "violence".to_string(),
                        flagged: true,
                        score: 0.92,
                    }],
                },
                provider_name: "STUB".to_string(),
            }
        }
    }

    /// A fresh provisioned instance with one chat / character / image profile.
    fn instance(a: &Arrangement) -> (tempfile::TempDir, Db) {
        let dir = tempfile::tempdir().expect("tempdir");
        let data = dir.path().join("data");
        std::fs::create_dir_all(&data).expect("data dir");
        crate::services::provisioning::provision_fresh_instance(&data, PEPPER).expect("provision");

        let db = Db::open(
            DbPaths {
                main: data.join("quilltap.db"),
                mount_index: Some(data.join("quilltap-mount-index.db")),
                llm_logs: Some(data.join("quilltap-llm-logs.db")),
            },
            PEPPER,
        )
        .expect("open instance");

        let now = "2020-01-01T00:00:00.000Z".to_string();
        let api_key_id = a.profile_api_key_id.map(str::to_string);
        let danger_mode = a.danger_mode;
        let uncensored_profile = a.uncensored_profile;
        let break_connection_profiles = a.break_connection_profiles;
        let break_files_insert = a.break_files_insert;
        let physical =
            a.has_appearance.then(
                || crate::db::vault_character_write::PhysicalDescriptionWrite {
                    full_description: None,
                    head_and_shoulders_prompt: Some("a head-and-shoulders study".to_string()),
                    short_prompt: None,
                    medium_prompt: None,
                    long_prompt: None,
                    complete_prompt: None,
                },
            );
        let now_w = now.clone();
        db.write_blocking(move |ws| {
            let main = ws.main().connection();
            let mount = ws
                .mount_index()
                .expect("the mount-index partition")
                .connection();

            crate::db::character_vault::create_character_with_options(
                main,
                mount,
                &crate::db::characters::CharacterCreate {
                    user_id: USER.to_string(),
                    name: "Portrait Subject".to_string(),
                    default_image_id: None,
                    default_connection_profile_id: None,
                    default_partner_id: None,
                    default_roleplay_template_id: None,
                    default_image_profile_id: None,
                    silly_tavern_data: None,
                    is_favorite: false,
                    npc: false,
                    controlled_by: "llm".to_string(),
                    default_agent_mode_enabled: None,
                    default_help_tools_enabled: None,
                    default_timestamp_config: None,
                    default_scenario_id: None,
                    default_system_prompt_id: None,
                    character_document_mount_point_id: None,
                    can_dress_themselves: None,
                    can_create_outfits: None,
                    system_transparency: None,
                    core_whisper_enabled: None,
                    can_be_carina: None,
                    partner_links: Vec::new(),
                    tags: Vec::new(),
                    avatar_overrides: Vec::new(),
                },
                &crate::db::vault_character_write::CharacterVaultWriteInput {
                    physical_description: physical,
                    ..Default::default()
                },
                &crate::db::characters::CreateOptions {
                    id: CHARACTER.to_string(),
                    created_at: now_w.clone(),
                    updated_at: now_w.clone(),
                },
            )?;

            crate::db::image_profiles::ImageProfilesRepository::new(main).create(
                &crate::db::image_profiles::IpCreate {
                    user_id: USER.to_string(),
                    name: "Original Desk".to_string(),
                    provider: "OPENAI".to_string(),
                    api_key_id,
                    base_url: None,
                    model_name: "dall-e-3".to_string(),
                    parameters: serde_json::json!({}),
                    is_default: true,
                    is_dangerous_compatible: false,
                    tags: Vec::new(),
                },
                &crate::db::image_profiles::CreateOptions {
                    id: PROFILE.to_string(),
                    created_at: now_w.clone(),
                    updated_at: now_w.clone(),
                },
            )?;

            // `ChatCreate`/`ChatParticipant` are `Deserialize`-only (every
            // optional field carries its Zod default through serde), so the
            // seed goes in as JSON — which is also the shape v4 would write.
            let chat_create: crate::db::chats::ChatCreate =
                serde_json::from_value(serde_json::json!({
                    "userId": USER,
                    "title": "A Sitting",
                    "participants": [{
                        "id": "p1",
                        "type": "character",
                        "characterId": CHARACTER,
                        "createdAt": now_w,
                        "updatedAt": now_w,
                    }],
                }))
                .expect("chat create shape");
            // A cheap-LLM candidate: `build_cheap_llm_selection` answers `None`
            // on an empty list, and then the Concierge pre-scan never runs at
            // all. Raw INSERT of exactly the NOT NULL columns — every other one
            // has a DDL default, which is also what v4's `insertOne` relies on.
            main.execute(
                "INSERT INTO connection_profiles \
                 (id, userId, name, provider, modelName, isDefault, isCheap, createdAt, updatedAt) \
                 VALUES (?1, ?2, 'Cheap Desk', 'OPENAI', 'gpt-cheap', 1, 1, ?3, ?3)",
                rusqlite::params!["55555555-5555-4555-8555-555555555555", USER, now_w],
            )?;

            if danger_mode != "OFF" {
                // The second image profile the AUTO_ROUTE resolver can find.
                if uncensored_profile {
                    crate::db::image_profiles::ImageProfilesRepository::new(main).create(
                        &crate::db::image_profiles::IpCreate {
                            user_id: USER.to_string(),
                            name: "Uncensored Desk".to_string(),
                            provider: "OPENAI".to_string(),
                            api_key_id: Some("key-1".to_string()),
                            base_url: None,
                            model_name: "dall-e-uncensored".to_string(),
                            parameters: serde_json::json!({}),
                            is_default: false,
                            is_dangerous_compatible: true,
                            tags: Vec::new(),
                        },
                        &crate::db::image_profiles::CreateOptions {
                            id: UNCENSORED_PROFILE.to_string(),
                            created_at: now_w.clone(),
                            updated_at: now_w.clone(),
                        },
                    )?;
                }
                // The provisioned instance already HAS the single user's
                // `chat_settings` row, so this UPDATEs the one column the
                // pre-scan reads rather than inserting a second.
                let mut settings = serde_json::json!({
                    "mode": danger_mode,
                    "threshold": 0.7,
                    "scanTextChat": true,
                    "scanImagePrompts": true,
                    "scanImageGeneration": false,
                    "displayMode": "SHOW",
                    "showWarningBadges": true,
                });
                if uncensored_profile {
                    settings["uncensoredImageProfileId"] =
                        serde_json::Value::String(UNCENSORED_PROFILE.to_string());
                }
                let updated = main.execute(
                    "UPDATE chat_settings SET dangerousContentSettings = ?1 WHERE userId = ?2",
                    rusqlite::params![settings.to_string(), USER],
                )?;
                assert_eq!(updated, 1, "the provisioned chat_settings row");
            }

            ws.main().chats().create(
                &chat_create,
                &crate::db::chats::CreateOptions {
                    id: CHAT.to_string(),
                    created_at: now_w.clone(),
                    updated_at: now_w,
                },
            )?;
            // The plant for v4 `:237`: the profiles READ itself fails. v5's
            // `build_cheap_llm_selection` is infallible, so this is the only
            // thing under it that can go wrong — and it used to be swallowed.
            if break_connection_profiles {
                main.execute("DROP TABLE connection_profiles", [])?;
            }
            // The plant for v4 `:589`: the `files` INSERT itself fails. A
            // trigger, not a DROP, because the cache lookup SELECTs `files`
            // first and must still succeed for the job to reach the write.
            if break_files_insert {
                main.execute(
                    "CREATE TRIGGER qt_test_refuse_files_insert BEFORE INSERT ON files \
                     BEGIN SELECT RAISE(ABORT, 'files insert refused by the test plant'); END",
                    [],
                )?;
            }
            Ok(())
        })
        .expect("seed");

        (dir, db)
    }

    /// Run the handler over a freshly arranged instance and hand back its
    /// outcome plus everything it narrated.
    fn run(a: Arrangement, answer: Answer) -> (Result<(), String>, Vec<String>) {
        run_with_moderation(a, answer, &NoModerationProvider)
    }

    fn run_with_moderation<M>(
        a: Arrangement,
        answer: Answer,
        moderation: &M,
    ) -> (Result<(), String>, Vec<String>)
    where
        M: crate::services::dangerous_content::gatekeeper::ModerationProvider,
    {
        let (_dir, db) = instance(&a);
        let images = StubImages(answer);
        let completion = CannedCompletionProvider::new();
        let keys = Keys(a.resolved_key.map(str::to_string));
        let transcoder = PassthroughTranscoder;
        let decl: &common::ImageDeclarationsFn = &declarations;
        let deps = AvatarJobDeps {
            image_provider: &images,
            completion: &completion,
            moderation,
            api_keys: &keys,
            transcoder: &transcoder,
            now_ms: 1_577_836_800_000,
            declarations_for: decl,
            blob_webp: None,
        };
        let payload = CharacterAvatarPayload {
            chat_id: CHAT.to_string(),
            character_id: CHARACTER.to_string(),
            image_profile_id: PROFILE.to_string(),
            equipped_slots_override: None,
            force: false,
        };
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        crate::test_support::captured_with(|| {
            rt.block_on(handle_character_avatar_generation(
                &db, &deps, USER, &payload, JOB,
            ))
        })
    }

    /// Exactly one captured line contains `needle`; hand it back.
    fn one<'a>(lines: &'a [String], needle: &str) -> &'a str {
        let hits: Vec<&String> = lines.iter().filter(|l| l.contains(needle)).collect();
        assert_eq!(
            hits.len(),
            1,
            "expected exactly one line containing {needle:?}, got {hits:?}\nall: {lines:?}"
        );
        hits[0]
    }

    fn none(lines: &[String], needle: &str) {
        assert!(
            !lines.iter().any(|l| l.contains(needle)),
            "expected NO line containing {needle:?}\nall: {lines:?}"
        );
    }

    /// Every line carries v4's `context` and the job id it was handed.
    fn has_common_bag(line: &str) {
        assert!(
            line.contains("context=background-jobs.character-avatar"),
            "v4's context: {line}"
        );
        assert!(
            line.contains(&format!("job_id={JOB}")),
            "the job id: {line}"
        );
    }

    // --- v4 `:102` — the handler announces itself before touching the DB -----

    /// It fires even when the very next step throws, which is the point: a job
    /// that dies on a missing chat still leaves its own first line.
    #[test]
    fn starting_fires_before_the_first_read_and_survives_a_missing_chat() {
        let (_dir, db) = instance(&Arrangement::default());
        let images = StubImages(one_png());
        let completion = CannedCompletionProvider::new();
        let moderation = NoModerationProvider;
        let keys = Keys(Some("sk-test".to_string()));
        let transcoder = PassthroughTranscoder;
        let decl: &common::ImageDeclarationsFn = &declarations;
        let deps = AvatarJobDeps {
            image_provider: &images,
            completion: &completion,
            moderation: &moderation,
            api_keys: &keys,
            transcoder: &transcoder,
            now_ms: 1_577_836_800_000,
            declarations_for: decl,
            blob_webp: None,
        };
        let payload = CharacterAvatarPayload {
            chat_id: "no-such-chat".to_string(),
            character_id: CHARACTER.to_string(),
            image_profile_id: PROFILE.to_string(),
            equipped_slots_override: None,
            force: false,
        };
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        let (out, lines) = crate::test_support::captured_with(|| {
            rt.block_on(handle_character_avatar_generation(
                &db, &deps, USER, &payload, JOB,
            ))
        });
        assert_eq!(out.err().as_deref(), Some("Chat not found: no-such-chat"));

        let line = one(&lines, "[CharacterAvatar] Starting avatar generation");
        assert!(
            line.starts_with("INFO quilltap::character_avatar"),
            "{line}"
        );
        has_common_bag(line);
        assert!(line.contains("chat_id=no-such-chat"), "{line}");
        assert!(
            line.contains(&format!("character_id={CHARACTER}")),
            "{line}"
        );
        // The silence leg: nothing downstream of the failed read fired.
        none(&lines, "[CharacterAvatar] Avatar generation completed");
        none(&lines, "[CharacterAvatar] Avatar image saved");
    }

    // --- v4 `:128` / `:138` — the two key skips, which are NOT the same line --

    #[test]
    fn a_profile_with_no_api_key_id_skips_and_names_the_profile() {
        let (out, lines) = run(
            Arrangement {
                profile_api_key_id: None,
                ..Default::default()
            },
            one_png(),
        );
        assert!(out.is_ok(), "a benign skip, not a failure: {out:?}");

        let line = one(
            &lines,
            "[CharacterAvatar] Image profile has no API key, skipping",
        );
        assert!(
            line.starts_with("WARN quilltap::character_avatar"),
            "{line}"
        );
        has_common_bag(line);
        assert!(line.contains(&format!("profile_id={PROFILE}")), "{line}");
        // The OTHER key line is a different diagnosis and must NOT fire.
        none(
            &lines,
            "[CharacterAvatar] API key not found or invalid, skipping",
        );
        none(
            &lines,
            "[CharacterAvatar] No appearance data available, skipping",
        );
    }

    #[test]
    fn a_dangling_api_key_id_skips_with_the_other_sentence_and_names_no_profile() {
        let (out, lines) = run(
            Arrangement {
                resolved_key: None,
                ..Default::default()
            },
            one_png(),
        );
        assert!(out.is_ok(), "a benign skip: {out:?}");

        let line = one(
            &lines,
            "[CharacterAvatar] API key not found or invalid, skipping",
        );
        assert!(
            line.starts_with("WARN quilltap::character_avatar"),
            "{line}"
        );
        has_common_bag(line);
        // v4's bag here is `{context, jobId}` and nothing else.
        assert!(
            !line.contains("profile_id="),
            "v4 names no profile here: {line}"
        );
        none(
            &lines,
            "[CharacterAvatar] Image profile has no API key, skipping",
        );
    }

    // --- v4 `:167` — no appearance -------------------------------------------

    #[test]
    fn a_character_with_no_physical_description_skips_and_names_the_character() {
        let (out, lines) = run(
            Arrangement {
                has_appearance: false,
                ..Default::default()
            },
            one_png(),
        );
        assert!(out.is_ok(), "a benign skip: {out:?}");

        let line = one(
            &lines,
            "[CharacterAvatar] No appearance data available, skipping",
        );
        assert!(
            line.starts_with("WARN quilltap::character_avatar"),
            "{line}"
        );
        has_common_bag(line);
        assert!(
            line.contains(&format!("character_id={CHARACTER}")),
            "{line}"
        );
        // It got past BOTH key gates to reach this one.
        none(
            &lines,
            "[CharacterAvatar] Image profile has no API key, skipping",
        );
        none(
            &lines,
            "[CharacterAvatar] API key not found or invalid, skipping",
        );
        // And nothing was generated.
        none(&lines, "[CharacterAvatar] Avatar image saved");
    }

    // --- v4 `:479` / `:490` — the two empty-payload skips ---------------------

    #[test]
    fn an_empty_image_list_says_so_and_never_says_the_other_thing() {
        let (out, lines) = run(Arrangement::default(), Answer::Images(Vec::new()));
        assert!(out.is_ok(), "a benign skip: {out:?}");

        let line = one(&lines, "[CharacterAvatar] No images returned from provider");
        assert!(
            line.starts_with("WARN quilltap::character_avatar"),
            "{line}"
        );
        has_common_bag(line);
        none(&lines, "[CharacterAvatar] Generated image has no data");
        none(&lines, "[CharacterAvatar] Avatar image saved");
        none(&lines, "[CharacterAvatar] Avatar generation completed");
    }

    /// v4's `rawData = imageData.data || imageData.b64Json; if (!rawData)` — a
    /// JS falsy test, so an EMPTY string lands here, not just a missing key.
    #[test]
    fn an_image_with_empty_data_says_the_other_thing() {
        let (out, lines) = run(
            Arrangement::default(),
            Answer::Images(vec![GeneratedImageData {
                data: Some(String::new()),
                url: None,
                mime_type: Some("image/png".to_string()),
                revised_prompt: None,
            }]),
        );
        assert!(out.is_ok(), "a benign skip: {out:?}");

        let line = one(&lines, "[CharacterAvatar] Generated image has no data");
        assert!(
            line.starts_with("WARN quilltap::character_avatar"),
            "{line}"
        );
        has_common_bag(line);
        none(&lines, "[CharacterAvatar] No images returned from provider");
        none(&lines, "[CharacterAvatar] Avatar image saved");
    }

    // --- v4 `:583` + `:604` — the happy path's two lines ----------------------

    #[test]
    fn a_saved_avatar_logs_the_write_and_then_the_completion() {
        let (out, lines) = run(Arrangement::default(), one_png());
        assert!(out.is_ok(), "the happy path: {out:?}");

        let saved = one(&lines, "[CharacterAvatar] Avatar image saved");
        assert!(
            saved.starts_with("INFO quilltap::character_avatar"),
            "{saved}"
        );
        has_common_bag(saved);
        assert!(saved.contains("file_id="), "the minted file id: {saved}");

        let done = one(&lines, "[CharacterAvatar] Avatar generation completed");
        assert!(
            done.starts_with("INFO quilltap::character_avatar"),
            "{done}"
        );
        has_common_bag(done);
        assert!(done.contains(&format!("chat_id={CHAT}")), "{done}");
        assert!(
            done.contains(&format!("character_id={CHARACTER}")),
            "{done}"
        );
        assert!(done.contains("file_id="), "{done}");

        // v4 logs the save BEFORE the bind and the completion after it.
        let idx = |needle: &str| lines.iter().position(|l| l.contains(needle)).unwrap();
        assert!(
            idx("Avatar image saved") < idx("Avatar generation completed"),
            "the save precedes the completion: {lines:?}"
        );
        // The failure twin and every skip stayed silent.
        none(&lines, "[CharacterAvatar] Failed to save avatar image");
        none(&lines, "skipping");
        none(&lines, "[CharacterAvatar] No images returned from provider");
    }

    /// v4 `:589` — the write fails: ERROR, the exception on the line, the job
    /// fails with the same sentence, and the completion line never fires.
    /// (The `1fefadb9a` round's §3 review: the line had only a silence leg.)
    #[test]
    fn a_failed_save_logs_the_error_and_fails_the_job() {
        let (out, lines) = run_with_moderation(
            Arrangement {
                break_files_insert: true,
                ..Default::default()
            },
            one_png(),
            &FlagsEverything,
        );
        let err = out.expect_err("the refused INSERT must fail the job");
        assert!(
            err.starts_with("Failed to save avatar image: "),
            "v4 rethrows with its own prefix: {err}"
        );

        let line = one(&lines, "[CharacterAvatar] Failed to save avatar image");
        assert!(
            line.starts_with("ERROR quilltap::character_avatar"),
            "{line}"
        );
        has_common_bag(line);
        assert!(line.contains("error="), "v4 attaches the exception: {line}");
        assert!(
            line.contains("refused by the test plant"),
            "and it names what actually failed: {line}"
        );
        none(&lines, "[CharacterAvatar] Avatar image saved");
        none(&lines, "[CharacterAvatar] Avatar generation completed");
    }

    // --- v4 `:378` (the shared arm) + the completion's silence ---------------

    /// A refused provider ends the job, so neither of the happy path's two
    /// lines may fire — the silence leg for both of them at once.
    #[test]
    fn a_failed_generation_logs_neither_the_save_nor_the_completion() {
        let (out, lines) = run(
            Arrangement::default(),
            Answer::Fails("the provider exploded".to_string()),
        );
        assert_eq!(
            out.err().as_deref(),
            Some("Avatar image generation failed: the provider exploded")
        );
        // `image_job_common`'s shared arm, under this handler's name.
        let failed = one(&lines, "[CharacterAvatar] Image generation failed");
        assert!(
            failed.starts_with("ERROR quilltap::character_avatar"),
            "{failed}"
        );
        none(&lines, "[CharacterAvatar] Avatar image saved");
        none(&lines, "[CharacterAvatar] Avatar generation completed");
        none(
            &lines,
            "[CharacterAvatar] Concierge uncensored reroute succeeded",
        );
    }

    // --- v4 `:255` / `:274` / `:282` — the Concierge pre-scan's three lines ---

    /// v4 logs the VERDICT on `isDangerous` alone and asks about the mode only
    /// afterwards, so DETECT_ONLY — where nothing is rerouted — is exactly the
    /// arm where the verdict is the operator's only signal. v5 had collapsed the
    /// two conditions into one, which routes identically and says nothing here.
    #[test]
    fn a_dangerous_verdict_is_logged_in_detect_only_where_nothing_reroutes() {
        let (out, lines) = run_with_moderation(
            Arrangement {
                danger_mode: "DETECT_ONLY",
                ..Default::default()
            },
            one_png(),
            &FlagsEverything,
        );
        assert!(out.is_ok(), "DETECT_ONLY still draws the portrait: {out:?}");

        let line = one(
            &lines,
            "[CharacterAvatar] Avatar prompt classified as dangerous",
        );
        assert!(
            line.starts_with("INFO quilltap::character_avatar"),
            "{line}"
        );
        has_common_bag(line);
        assert!(
            line.contains("score=0.92"),
            "the classifier's score: {line}"
        );
        assert!(
            line.contains("categoriesJson=[\"violence\"]"),
            "v4 maps to the category NAMES, as an array (the `…Json` convention): {line}"
        );
        assert!(line.contains("mode=DETECT_ONLY"), "{line}");
        // The routing read succeeded, so v4's `:292` catch stayed silent.
        none(&lines, CLASSIFICATION_FAILED);
        // DETECT_ONLY asks no routing question at all.
        none(
            &lines,
            "[CharacterAvatar] Rerouted to uncensored image provider",
        );
        none(
            &lines,
            "[CharacterAvatar] No uncensored image provider available, using original",
        );
        // …and the portrait still lands.
        one(&lines, "[CharacterAvatar] Avatar image saved");
    }

    /// AUTO_ROUTE with a configured uncensored desk: the verdict AND the reroute.
    #[test]
    fn auto_route_with_an_uncensored_desk_names_both_profiles() {
        let (out, lines) = run_with_moderation(
            Arrangement {
                danger_mode: "AUTO_ROUTE",
                uncensored_profile: true,
                ..Default::default()
            },
            one_png(),
            &FlagsEverything,
        );
        assert!(out.is_ok(), "{out:?}");

        let verdict = one(
            &lines,
            "[CharacterAvatar] Avatar prompt classified as dangerous",
        );
        assert!(verdict.contains("mode=AUTO_ROUTE"), "{verdict}");

        let line = one(
            &lines,
            "[CharacterAvatar] Rerouted to uncensored image provider",
        );
        assert!(
            line.starts_with("INFO quilltap::character_avatar"),
            "{line}"
        );
        has_common_bag(line);
        // v4 logs the profiles' NAMES here, not their ids — the operator is
        // reading which desk answered.
        assert!(line.contains("original_profile=Original Desk"), "{line}");
        assert!(
            line.contains("uncensored_profile=Uncensored Desk"),
            "{line}"
        );
        assert!(
            line.contains("reason="),
            "the resolver's own sentence: {line}"
        );
        none(
            &lines,
            "[CharacterAvatar] No uncensored image provider available, using original",
        );
    }

    /// AUTO_ROUTE with nowhere to go: the OTHER sentence, and the job carries on
    /// with the original desk (v4 "using original" — not a failure).
    #[test]
    fn auto_route_with_no_uncensored_desk_says_so_and_keeps_the_original() {
        let (out, lines) = run_with_moderation(
            Arrangement {
                danger_mode: "AUTO_ROUTE",
                uncensored_profile: false,
                ..Default::default()
            },
            one_png(),
            &FlagsEverything,
        );
        assert!(out.is_ok(), "the original desk still draws it: {out:?}");

        one(
            &lines,
            "[CharacterAvatar] Avatar prompt classified as dangerous",
        );
        let line = one(
            &lines,
            "[CharacterAvatar] No uncensored image provider available, using original",
        );
        assert!(
            line.starts_with("WARN quilltap::character_avatar"),
            "{line}"
        );
        has_common_bag(line);
        assert!(line.contains("reason="), "{line}");
        none(
            &lines,
            "[CharacterAvatar] Rerouted to uncensored image provider",
        );
        one(&lines, "[CharacterAvatar] Avatar image saved");
    }

    /// The silence leg for all three: with the Concierge OFF the pre-scan never
    /// runs, so no verdict and no routing line — whatever the moderator says.
    #[test]
    fn the_concierge_off_says_nothing_even_when_the_moderator_flags_everything() {
        let (out, lines) = run_with_moderation(Arrangement::default(), one_png(), &FlagsEverything);
        assert!(out.is_ok(), "{out:?}");
        none(
            &lines,
            "[CharacterAvatar] Avatar prompt classified as dangerous",
        );
        none(
            &lines,
            "[CharacterAvatar] Rerouted to uncensored image provider",
        );
        none(
            &lines,
            "[CharacterAvatar] No uncensored image provider available, using original",
        );
        none(
            &lines,
            "[CharacterAvatar] Failed to build cheap LLM selection for danger classification",
        );
        one(&lines, "[CharacterAvatar] Avatar image saved");
    }

    // --- v4 `:237` — the cheap-LLM selection could not be built --------------

    /// v4 WARNs when `resolveCheapLLMSelectionForUser` throws and carries on with
    /// no classification. v5's builder is infallible, so the analogous failure is
    /// the profiles READ — which `unwrap_or_default()` used to swallow whole.
    #[test]
    fn a_failed_profiles_read_warns_and_the_job_carries_on_unclassified() {
        let (out, lines) = run_with_moderation(
            Arrangement {
                danger_mode: "AUTO_ROUTE",
                uncensored_profile: true,
                break_connection_profiles: true,
                ..Default::default()
            },
            one_png(),
            &FlagsEverything,
        );
        assert!(out.is_ok(), "v4 never blocks an avatar on this: {out:?}");

        let line = one(
            &lines,
            "[CharacterAvatar] Failed to build cheap LLM selection for danger classification",
        );
        assert!(
            line.starts_with("WARN quilltap::character_avatar"),
            "{line}"
        );
        has_common_bag(line);
        assert!(line.contains("error="), "v4 carries the message: {line}");
        assert!(
            line.contains("connection_profiles"),
            "and it names what actually failed: {line}"
        );
        // No selection means no classification — which is v4's behaviour on the
        // throw, not just ours.
        none(
            &lines,
            "[CharacterAvatar] Avatar prompt classified as dangerous",
        );
        none(
            &lines,
            "[CharacterAvatar] Rerouted to uncensored image provider",
        );
        // The portrait still lands on the original desk.
        one(&lines, "[CharacterAvatar] Avatar image saved");
    }

    /// The silence leg for `:237`: a profiles read that SUCCEEDS and simply
    /// finds no eligible profile is v4's `resolved?.selection ?? null`, which
    /// warns about nothing.
    #[test]
    fn a_successful_profiles_read_never_warns() {
        let (out, lines) = run_with_moderation(
            Arrangement {
                danger_mode: "AUTO_ROUTE",
                uncensored_profile: true,
                ..Default::default()
            },
            one_png(),
            &FlagsEverything,
        );
        assert!(out.is_ok(), "{out:?}");
        none(
            &lines,
            "[CharacterAvatar] Failed to build cheap LLM selection for danger classification",
        );
        // Non-vacuity: the pre-scan really did run on this arrangement.
        one(
            &lines,
            "[CharacterAvatar] Avatar prompt classified as dangerous",
        );
    }
}
