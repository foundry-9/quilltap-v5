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

    // apiKeyId / apiKey — WARN+RETURN (a benign skip) when absent.
    let Some(api_key_id) = common::owned_field(&image_profile, "apiKeyId") else {
        return Ok(());
    };
    let Some(api_key) = deps
        .api_keys
        .resolve(&api_key_id, user_id)
        .filter(|k| !k.is_empty())
    else {
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
        // No appearance data — WARN+RETURN.
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
            tracing::info!(
                target: "quilltap::character_avatar",
                context = "background-jobs.character-avatar",
                job_id = job_id,
                chatId = %payload.chat_id,
                characterId = %payload.character_id,
                fileId = %cached.id,
                leafCounts = ?leaf_counts,
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
        // Build the cheap-LLM selection for classification (errors swallowed).
        let all_profiles = db
            .read_main(crate::db::connection_profiles::find_all)
            .unwrap_or_default();
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
                let route = db
                    .read_main(move |conn| {
                        Ok(resolve_image_provider_for_dangerous_content(
                            conn,
                            api_keys,
                            &original,
                            &orig_key,
                            &mode,
                            uncensored.as_deref(),
                            &uid,
                        ))
                    })
                    .ok();
                if let Some(route) = route {
                    if route.rerouted {
                        eff_id = route.image_profile.id.clone();
                        eff_provider = route.image_profile.provider.clone();
                        eff_model = route.image_profile.model_name.clone();
                        eff_params = common::load_profile_parameters(db, &eff_id).await;
                        eff_api_key = route.api_key.clone();
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
        return Ok(());
    };
    let raw_data = match image_data.data.as_deref() {
        Some(d) if !d.is_empty() => d,
        _ => return Ok(()),
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
    };
    common::with_both_conns(db, move |main, mount| {
        write_avatar_file(main, mount, &write)
    })
    .await
    .map_err(|e| format!("Failed to save avatar image: {e}"))?;

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
