//! The `STORY_BACKGROUND_GENERATION` job handler (v4
//! `lib/background-jobs/handlers/story-background.ts`).
//!
//! Generates an atmospheric landscape background for a chat from its scene
//! context + characters: derive/reuse the scene, resolve + sanitize character
//! appearances, craft the prompt (with the empty-refusal uncensored retry),
//! back-fill mentioned non-participants, generate the image (with the post-hoc
//! moderation reroute), persist it into the Lantern Backgrounds store (or the
//! project store), then update `chats` + the project `latest_chat` reference and
//! post the Lantern notification.
//!
//! ## Model boundaries (tier-3 seams)
//!   - [`ImageProvider`] / [`CompletionProvider`] / [`ModerationProvider`] /
//!     [`ApiKeyResolver`] / [`ImageTranscoder`] / [`common::ProjectImageUpload`].
//!   - The four cheap-LLM tasks (derive-scene / craft / appearance-resolve /
//!     sanitize) run over the ported [`CheapLlmTaskExecutor`].
//!
//! ## Deferrals: `logLLMCall` (the `generate_image` precedent — not emitted).
//! Aesthetics: **lantern + aurora + the Ariel Clause** (depiction guidelines DO
//! apply here).

use serde_json::Value;

use crate::cheap_llm::{resolve_uncensored_cheap_llm_selection, CheapLlmSelection};
use crate::clock::iso_from_unix_ms;
use crate::db::runtime::Db;
use crate::db::DbError;
use crate::image_gen::Orientation;
use crate::model::completion::CompletionProvider;
use crate::model::image::{ImageProvider, ImageTranscoder, TranscodeInput};
use crate::pronoun_gender::gender_prefix_from_pronouns;
use crate::services::aesthetics::{
    get_project_official_mount_point_id, resolve_aesthetic, resolve_depiction_guidelines,
    AestheticKind, DepictedCharacter,
};
use crate::services::appearance_resolution::{
    resolve_character_appearances, sanitize_appearances_if_needed, AppearanceResolutionInput,
    PhysicalDescription, ResolvedCharacterAppearance, SceneStateCharacter, WardrobeItemInput,
};
use crate::services::cheap_llm_exec::CheapLlmTaskExecutor;
use crate::services::dangerous_content::chat_override::is_chat_active_dangerous;
use crate::services::dangerous_content::gatekeeper::ModerationProvider;
use crate::services::dangerous_content::provider_routing::ApiKeyResolver;
use crate::services::image_job_common as common;
use crate::services::image_job_storage::write_lantern_background_to_mount_store;
use crate::services::image_scene_tasks::{
    craft_story_background_prompt, derive_scene_context, ChatMessage, DeriveSceneContextInput,
    StoryBackgroundCharacter, StoryBackgroundPromptContext,
};
use crate::services::lantern_notifications::{
    post_lantern_image_notification, LanternNotificationKind, LanternPostParams,
};

/// The decoded `STORY_BACKGROUND_GENERATION` payload.
#[derive(Clone, Debug)]
pub struct StoryBackgroundPayload {
    pub chat_id: String,
    pub image_profile_id: String,
    pub character_ids: Vec<String>,
    pub scene_context: Option<String>,
    pub project_id: Option<String>,
}

impl StoryBackgroundPayload {
    /// Decode from the raw job payload JSON.
    pub fn from_json(payload: &Value) -> Result<Self, String> {
        let chat_id = payload
            .get("chatId")
            .and_then(Value::as_str)
            .ok_or("payload missing chatId")?
            .to_string();
        let image_profile_id = payload
            .get("imageProfileId")
            .and_then(Value::as_str)
            .ok_or("payload missing imageProfileId")?
            .to_string();
        let character_ids = payload
            .get("characterIds")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        let scene_context = payload
            .get("sceneContext")
            .and_then(Value::as_str)
            .map(str::to_string);
        let project_id = payload
            .get("projectId")
            .and_then(Value::as_str)
            .map(str::to_string);
        Ok(Self {
            chat_id,
            image_profile_id,
            character_ids,
            scene_context,
            project_id,
        })
    }
}

/// The injected seams the story-background handler needs.
pub struct StoryJobDeps<'a, I, C, M, A, T, U> {
    pub image_provider: &'a I,
    pub completion: &'a C,
    pub moderation: &'a M,
    pub api_keys: &'a A,
    pub transcoder: &'a T,
    pub upload: &'a U,
    /// The cheap-LLM task executor (holds the no-custom-temperature cache).
    pub executor: &'a CheapLlmTaskExecutor,
    pub now_ms: i64,
    pub orientation_data_for: &'a common::OrientationDataFn,
}

/// The subset of a scene state the story handler reads.
struct StorySceneState {
    updated_at_message_count: i64,
    location: String,
    characters: Vec<StorySceneChar>,
}

struct StorySceneChar {
    character_id: String,
    character_name: String,
    action: String,
    appearance: Option<String>,
    clothing: Option<String>,
}

/// Parse + validate the chat's `sceneState` (v4 `SceneStateSchema.safeParse`).
/// `None` on absent / parse failure / invalid shape. Mirrors v4's Zod schema:
/// `SceneStateSchema` requires `location` (string), `characters` (array),
/// `updatedAt` (TimestampSchema = a present string), and `updatedAtMessageCount`
/// (number); each `SceneStateCharacterSchema` requires `characterId` /
/// `characterName` / `action` (strings) plus `appearance` and `clothing` as
/// `z.string().nullable()` — the KEY must be present and be a string or `null`
/// (an absent key or a non-string/non-null value fails the whole parse, matching
/// `safeParse`).
fn parse_scene_state(chat: &Value) -> Option<StorySceneState> {
    let raw = chat.get("sceneState")?;
    let parsed: Value = match raw {
        Value::String(s) => serde_json::from_str(s).ok()?,
        other => other.clone(),
    };
    let location = parsed.get("location").and_then(Value::as_str)?.to_string();
    // TimestampSchema — required present string (value unused by the handler).
    parsed.get("updatedAt").and_then(Value::as_str)?;
    let updated_at_message_count = parsed
        .get("updatedAtMessageCount")
        .and_then(Value::as_i64)?;
    let chars_val = parsed.get("characters").and_then(Value::as_array)?;
    let mut characters = Vec::new();
    for c in chars_val {
        let character_id = c.get("characterId").and_then(Value::as_str)?.to_string();
        let character_name = c.get("characterName").and_then(Value::as_str)?.to_string();
        let action = c.get("action").and_then(Value::as_str)?.to_string();
        // `z.string().nullable()` — the key must be present and string-or-null.
        let appearance = nullable_string(c, "appearance")?;
        let clothing = nullable_string(c, "clothing")?;
        characters.push(StorySceneChar {
            character_id,
            character_name,
            action,
            appearance,
            clothing,
        });
    }
    Some(StorySceneState {
        updated_at_message_count,
        location,
        characters,
    })
}

/// Zod `z.string().nullable()` on a JSON object field: `Some(Some(s))` for a
/// string, `Some(None)` for an explicit `null`, and `None` (invalid → whole parse
/// fails) when the key is absent or the value is neither a string nor `null`.
fn nullable_string(v: &Value, key: &str) -> Option<Option<String>> {
    match v.get(key) {
        Some(Value::String(s)) => Some(Some(s.clone())),
        Some(Value::Null) => Some(None),
        _ => None,
    }
}

/// v4 `handleStoryBackgroundGeneration`.
pub async fn handle_story_background_generation<I, C, M, A, T, U>(
    db: &Db,
    deps: &StoryJobDeps<'_, I, C, M, A, T, U>,
    user_id: &str,
    payload: &StoryBackgroundPayload,
) -> Result<(), String>
where
    I: ImageProvider,
    C: CompletionProvider,
    M: ModerationProvider,
    A: ApiKeyResolver,
    T: ImageTranscoder,
    U: common::ProjectImageUpload,
{
    // 1. Load chat (throw if missing).
    let chat_id = payload.chat_id.clone();
    let chat = db
        .read_main(move |conn| crate::db::chats_read::find_by_id(conn, &chat_id))
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Chat not found: {}", payload.chat_id))?;
    let chat_title = common::str_field(&chat, "title").unwrap_or("").to_string();

    // 2-3. Image profile + API key.
    let pid = payload.image_profile_id.clone();
    let image_profile = db
        .read_main(move |conn| crate::db::image_profiles::find_by_id(conn, &pid))
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Image profile not found: {}", payload.image_profile_id))?;
    let image_provider_name = common::str_field(&image_profile, "provider")
        .unwrap_or("")
        .to_string();
    let image_model = common::str_field(&image_profile, "modelName")
        .unwrap_or("")
        .to_string();
    let image_params = image_profile
        .get("parameters")
        .cloned()
        .unwrap_or(Value::Null);
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

    // 4. Load characters (vault-overlaid; drop missing).
    let char_ids = payload.character_ids.clone();
    let valid_characters = common::with_both_conns(db, move |main, mount| {
        let mut out = Vec::new();
        for id in &char_ids {
            if let Some(c) = crate::db::characters_read::find_by_id(main, mount, id)? {
                out.push(c);
            }
        }
        Ok(out)
    })
    .await
    .map_err(|e| e.to_string())?;

    // Scene state freshness gate (<=5 messages).
    let message_count = chat
        .get("messageCount")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let scene_state = parse_scene_state(&chat).filter(|ss| {
        let gap = message_count - ss.updated_at_message_count;
        gap <= 5
    });

    // 5-6. Chat settings + connection profiles + cheap-LLM selection.
    let chat_settings = db
        .read_main(move |conn| crate::db::chat_settings::find_by_user_id(conn, user_id))
        .map_err(|e| e.to_string())?;
    let all_profiles = db
        .read_main(crate::db::connection_profiles::find_all)
        .map_err(|e| e.to_string())?;
    let cheap_settings = chat_settings
        .as_ref()
        .and_then(|cs| cs.get("cheapLLMSettings"));
    let Some(mut cheap_selection) =
        common::build_cheap_llm_selection(&all_profiles, cheap_settings)
    else {
        // No connection profiles — WARN+RETURN.
        return Ok(());
    };

    // Concierge settings.
    let danger_settings = common::resolve_danger_settings_for_chat(chat_settings.as_ref(), &chat);
    let is_dangerous_chat = is_chat_active_dangerous(Some(&chat));
    let has_uncensored_image_provider = danger_settings
        .uncensored_image_profile_id
        .as_deref()
        .map(|s| !s.is_empty())
        .unwrap_or(false);

    // Dangerous chats: upgrade the cheap selection for all cheap tasks.
    if is_dangerous_chat {
        let profiles: Vec<crate::cheap_llm::CheapLlmProfile> = all_profiles
            .iter()
            .map(common::cheap_llm_profile_from_value)
            .collect();
        let cheap_danger = crate::cheap_llm::DangerousContentSettings {
            mode: danger_settings.mode.clone(),
            uncensored_text_profile_id: danger_settings.uncensored_text_profile_id.clone(),
        };
        cheap_selection = resolve_uncensored_cheap_llm_selection(
            cheap_selection,
            true,
            Some(&cheap_danger),
            &profiles,
        );
    }

    // 7. Recent messages (last 20 visible).
    let chat_id_msgs = payload.chat_id.clone();
    let events = db
        .read_main(move |conn| crate::db::chats_messages_read::get_messages(conn, &chat_id_msgs))
        .map_err(|e| e.to_string())?;
    let recent_messages = extract_recent_messages(&events, 20);

    // 8. Appearance inputs (per-character equipped wardrobe → resolution input).
    let default_scene_context = payload
        .scene_context
        .clone()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| chat_title.clone());
    let mut scene_context = default_scene_context.clone();
    let scene_prompt_for_appearance = default_scene_context.clone();

    let chars_for_inputs = valid_characters.clone();
    let chat_id_inputs = payload.chat_id.clone();
    let appearance_inputs = common::with_both_conns(db, move |main, mount| {
        build_appearance_inputs(main, mount, &chat_id_inputs, &chars_for_inputs)
    })
    .await
    .map_err(|e| e.to_string())?;

    // Uncensored cheap selection (from `imagePromptProfileId`).
    let uncensored_selection = build_uncensored_selection(&all_profiles, cheap_settings);

    // Appearance selection: uncensored for a dangerous chat if available, else cheap.
    let used_uncensored_for_appearance = is_dangerous_chat && uncensored_selection.is_some();
    let appearance_selection = if used_uncensored_for_appearance {
        uncensored_selection.clone().unwrap()
    } else {
        cheap_selection.clone()
    };

    // Scene-state characters for the appearance resolver.
    let scene_state_chars: Option<Vec<SceneStateCharacter>> = scene_state.as_ref().map(|ss| {
        ss.characters
            .iter()
            .map(|c| SceneStateCharacter {
                character_id: c.character_id.clone(),
                character_name: c.character_name.clone(),
                appearance: c.appearance.clone(),
                clothing: c.clothing.clone(),
            })
            .collect()
    });

    // Derive scene context (skipped entirely when scene state is fresh).
    let scene_result = if !recent_messages.is_empty() && scene_state.is_none() {
        Some(
            derive_scene_context(
                deps.executor,
                deps.completion,
                &DeriveSceneContextInput {
                    chat_title: chat_title.clone(),
                    context_summary: common::owned_field(&chat, "contextSummary"),
                    recent_messages: recent_messages.clone(),
                    character_names: valid_characters
                        .iter()
                        .map(|c| common::str_field(c, "name").unwrap_or("").to_string())
                        .collect(),
                },
                &cheap_selection,
                user_id,
                Some(&payload.chat_id),
            )
            .await,
        )
    } else {
        None
    };

    // Appearance resolution (errors swallowed → None).
    let mut appearance_result = if !appearance_inputs.is_empty() {
        Some(
            resolve_character_appearances(
                deps.executor,
                deps.completion,
                &appearance_inputs,
                &recent_messages,
                &scene_prompt_for_appearance,
                &appearance_selection,
                user_id,
                Some(&payload.chat_id),
                scene_state_chars.as_deref(),
            )
            .await,
        )
    } else {
        None
    };

    // Scene context resolution precedence.
    if let Some(ss) = &scene_state {
        let char_actions = ss
            .characters
            .iter()
            .map(|c| format!("{}: {}", c.character_name, c.action))
            .collect::<Vec<_>>()
            .join("; ");
        scene_context = format!("{}. {char_actions}", ss.location);
    } else if let Some(res) = &scene_result {
        if res.success {
            if let Some(result) = &res.result {
                scene_context = result.clone();
            }
        }
    }

    // Appearance uncensored retry (only when the first ran on the cheap selection).
    let should_retry_appearance = appearance_result
        .as_ref()
        .map(|res| !res.llm_resolved)
        .unwrap_or(false)
        && !appearance_inputs.is_empty()
        && !used_uncensored_for_appearance;
    if should_retry_appearance {
        if let Some(uncensored) = &uncensored_selection {
            let retry = resolve_character_appearances(
                deps.executor,
                deps.completion,
                &appearance_inputs,
                &recent_messages,
                &scene_prompt_for_appearance,
                uncensored,
                user_id,
                Some(&payload.chat_id),
                scene_state_chars.as_deref(),
            )
            .await;
            if retry.llm_resolved {
                appearance_result = Some(retry);
            }
        }
    }

    // Sanitize gate (failure swallowed → unsanitized).
    let mut resolved_appearances: Option<Vec<ResolvedCharacterAppearance>> =
        appearance_result.map(|r| r.appearances);
    if let Some(appearances) = resolved_appearances.take() {
        if !appearances.is_empty() {
            let sanitized = sanitize_appearances_if_needed(
                deps.executor,
                deps.moderation,
                deps.completion,
                appearances.clone(),
                &danger_settings,
                is_dangerous_chat,
                has_uncensored_image_provider,
                &cheap_selection,
                user_id,
                Some(&payload.chat_id),
            )
            .await;
            resolved_appearances = Some(sanitized);
        } else {
            resolved_appearances = Some(appearances);
        }
    }

    // Character descriptions (from resolved appearances, or the fallback chain).
    let character_descriptions: Vec<StoryBackgroundCharacter> = valid_characters
        .iter()
        .map(|char| {
            let char_id = common::str_field(char, "id").unwrap_or("");
            let name = common::str_field(char, "name").unwrap_or("").to_string();
            let gender_prefix = gender_prefix_from_pronouns(pronoun_subject(char));
            let resolved = resolved_appearances
                .as_ref()
                .and_then(|list| list.iter().find(|a| a.character_id == char_id));
            let description = match resolved {
                Some(r) => {
                    let mut parts = vec![format!("{gender_prefix}{}", r.physical_description)];
                    if !r.clothing_description.is_empty() {
                        parts.push(format!("Wearing: {}", r.clothing_description));
                    }
                    parts.join(". ")
                }
                None => {
                    let primary = char.get("physicalDescription");
                    let fallback = primary
                        .and_then(|p| {
                            p.get("mediumPrompt")
                                .and_then(Value::as_str)
                                .filter(|s| !s.is_empty())
                                .or(p
                                    .get("shortPrompt")
                                    .and_then(Value::as_str)
                                    .filter(|s| !s.is_empty()))
                        })
                        .unwrap_or(&name);
                    format!("{gender_prefix}{fallback}")
                }
            };
            StoryBackgroundCharacter { name, description }
        })
        .collect();

    // 9. Aesthetics: lantern + aurora + the Ariel Clause (depiction guidelines).
    let project_official =
        get_project_official_mount_point_id(db, common::str_field(&chat, "projectId")).await;
    let scene_aesthetic = resolve_aesthetic(
        db,
        AestheticKind::Lantern,
        project_official.as_deref(),
        None,
    )
    .await;
    let character_aesthetic =
        resolve_aesthetic(db, AestheticKind::Aurora, project_official.as_deref(), None).await;
    let depicted: Vec<DepictedCharacter> = valid_characters
        .iter()
        .map(|c| DepictedCharacter {
            id: common::str_field(c, "id").unwrap_or("").to_string(),
            name: common::str_field(c, "name").unwrap_or("").to_string(),
            character_document_mount_point_id: common::owned_field(
                c,
                "characterDocumentMountPointId",
            ),
        })
        .collect();
    let depiction_guidelines = resolve_depiction_guidelines(db, &depicted, None).await;
    let depiction_for_task: Vec<crate::services::image_scene_tasks::DepictionGuideline> =
        depiction_guidelines
            .iter()
            .map(|g| crate::services::image_scene_tasks::DepictionGuideline {
                character_name: g.character_name.clone(),
                content: g.content.clone(),
            })
            .collect();

    // 10. Craft the background prompt.
    let craft_ctx = StoryBackgroundPromptContext {
        scene_context: scene_context.clone(),
        characters: character_descriptions.clone(),
        provider: image_provider_name.clone(),
        scene_aesthetic: scene_aesthetic.clone(),
        character_aesthetic: character_aesthetic.clone(),
        depiction_guidelines: depiction_for_task.clone(),
    };
    let craft_result = craft_story_background_prompt(
        deps.executor,
        deps.completion,
        &craft_ctx,
        &cheap_selection,
        user_id,
        Some(&payload.chat_id),
    )
    .await;

    if !craft_result.success {
        // Actual cheap-LLM error — WARN+RETURN.
        return Ok(());
    }
    let mut final_prompt = match craft_result.result.filter(|s| !s.is_empty()) {
        Some(p) => p,
        None => {
            // Empty-but-SUCCESS: a silent content refusal → uncensored retry.
            let Some(uncensored) = &uncensored_selection else {
                return Ok(());
            };
            let retry = craft_story_background_prompt(
                deps.executor,
                deps.completion,
                &craft_ctx,
                uncensored,
                user_id,
                Some(&payload.chat_id),
            )
            .await;
            match retry
                .result
                .filter(|_| retry.success)
                .filter(|s| !s.is_empty())
            {
                Some(p) => p,
                None => return Ok(()),
            }
        }
    };

    // 9b. Back-fill mentioned non-participant characters (errors swallowed).
    let participant_ids: std::collections::HashSet<String> =
        payload.character_ids.iter().cloned().collect();
    let uid_for_chars = user_id.to_string();
    let user_characters = common::with_both_conns(db, move |main, mount| {
        crate::db::characters_read::find_by_user_id(main, mount, &uid_for_chars)
    })
    .await
    .unwrap_or_default();
    let non_participant: Vec<Value> = user_characters
        .into_iter()
        .filter(|c| {
            common::str_field(c, "id")
                .map(|id| !participant_ids.contains(id))
                .unwrap_or(true)
        })
        .collect();
    final_prompt = append_missing_character_enumerations(&final_prompt, &non_participant);

    // 10. Generate the image (landscape, with post-hoc reroute).
    let outcome = common::generate_with_reroute(
        db,
        deps.image_provider,
        deps.api_keys,
        common::str_field(&image_profile, "id").unwrap_or(""),
        &image_provider_name,
        &image_model,
        &image_params,
        &api_key,
        &final_prompt,
        Orientation::Landscape,
        deps.orientation_data_for,
        &danger_settings.mode,
        danger_settings.uncensored_image_profile_id.as_deref(),
        user_id,
        "Image generation failed",
    )
    .await?;

    // v4: `rawData = imageData.data || imageData.b64Json; if (!rawData)` — a JS
    // falsy check, so a missing AND an empty-string payload both no-op (W4.7f
    // widened `data` to Option<String>).
    let Some(image_data) = outcome.images.into_iter().next() else {
        return Ok(());
    };
    let raw_data = match image_data.data.as_deref() {
        Some(d) if !d.is_empty() => d,
        _ => return Ok(()),
    };
    let generation_model = outcome.active_model;

    // 11. Decode + transcode + persist.
    let raw_buffer = common::decode_base64_node(raw_data);
    let provider_mime = image_data
        .mime_type
        .as_deref()
        .unwrap_or("image/png")
        .to_string();
    let provider_ext = provider_mime.split('/').nth(1).unwrap_or("png");
    let provider_filename = format!("story_background_{}.{provider_ext}", deps.now_ms);
    let converted = deps.transcoder.transcode(&TranscodeInput {
        bytes: raw_buffer,
        mime_type: provider_mime,
        filename: provider_filename,
    });
    let now_iso = iso_from_unix_ms(deps.now_ms);
    let file_id = uuid::Uuid::new_v4().to_string();
    // v4 description: `Story background for: ${payload.sceneContext || chat.title}`.
    let description = format!(
        "Story background for: {}",
        payload
            .scene_context
            .clone()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| chat_title.clone())
    );

    // Storage branch key: payload.projectId (NOT chat.projectId — v4 wrinkle).
    let project_upload = if let Some(project_id) = &payload.project_id {
        Some(
            deps.upload
                .upload(
                    &converted.filename,
                    &converted.bytes,
                    &converted.mime_type,
                    project_id,
                    "/story-backgrounds/",
                )
                .await,
        )
    } else {
        None
    };

    let mut linked_to = vec![payload.chat_id.clone()];
    linked_to.extend(payload.character_ids.iter().cloned());
    let write = StoryWriteInput {
        user_id: user_id.to_string(),
        file_id: file_id.clone(),
        now_iso: now_iso.clone(),
        converted_bytes: converted.bytes.clone(),
        converted_mime: converted.mime_type.clone(),
        converted_filename: converted.filename.clone(),
        converted_width: converted.width,
        converted_height: converted.height,
        final_prompt: final_prompt.clone(),
        generation_model,
        revised_prompt: image_data.revised_prompt.clone(),
        description: description.clone(),
        linked_to,
        project_id: payload.project_id.clone(),
        project_upload,
    };
    common::with_both_conns(db, move |main, mount| write_story_file(main, mount, &write))
        .await
        .map_err(|e| format!("Failed to save generated image: {e}"))?;

    // 12. Update chat with the new background image id + timestamp.
    let chat_id_upd = payload.chat_id.clone();
    let file_id_upd = file_id.clone();
    let now_upd = now_iso.clone();
    db.write(move |writers| {
        let update = crate::db::chats::ChatUpdate {
            story_background_image_id: Some(Some(file_id_upd)),
            last_background_generated_at: Some(Some(now_upd)),
            ..Default::default()
        };
        writers.main().chats().update(&chat_id_upd, &update)?;
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?;

    // 13. Project `latest_chat` propagation.
    if let Some(project_id) = &payload.project_id {
        let pid = project_id.clone();
        let file_id_proj = file_id.clone();
        common::with_both_conns(db, move |main, mount| {
            let repo = crate::db::projects::ProjectsRepository::new(main, mount);
            if let Some(project) = repo
                .find_by_id(&pid)
                .map_err(|e| DbError::Key(e.to_string()))?
            {
                if project.get("backgroundDisplayMode").and_then(Value::as_str)
                    == Some("latest_chat")
                {
                    let mut patch = serde_json::Map::new();
                    patch.insert(
                        "storyBackgroundImageId".into(),
                        Value::String(file_id_proj.clone()),
                    );
                    repo.update(&pid, &patch)
                        .map_err(|e| DbError::Key(e.to_string()))?;
                }
            }
            Ok(())
        })
        .await
        .map_err(|e| e.to_string())?;
    }

    // 14. Lantern notification (background → sender lantern; finalPrompt as aim).
    let _ = post_lantern_image_notification(
        db,
        LanternPostParams {
            chat_id: payload.chat_id.clone(),
            file_id,
            kind: LanternNotificationKind::Background,
            prompt: Some(final_prompt),
        },
    )
    .await;

    Ok(())
}

// ===========================================================================
// The registered JobHandler (the W4.8 runner pattern)
// ===========================================================================

/// A `STORY_BACKGROUND_GENERATION` [`crate::services::job_runner::JobHandler`] over
/// owned seams. The host wires it with real providers (removing the type from the
/// loud fallback); the differential's runner E2E wires it with canned seams.
pub struct StoryBackgroundGenerationHandler<I, C, M, A, T, U, F> {
    pub image_provider: I,
    pub completion: C,
    pub moderation: M,
    pub api_keys: A,
    pub transcoder: T,
    pub upload: U,
    pub executor: CheapLlmTaskExecutor,
    pub now_ms: i64,
    pub orientation_data_for: F,
}

impl<I, C, M, A, T, U, F> crate::services::job_runner::JobHandler
    for StoryBackgroundGenerationHandler<I, C, M, A, T, U, F>
where
    I: ImageProvider + Send + Sync,
    C: CompletionProvider + Send + Sync,
    M: ModerationProvider + Send + Sync,
    A: ApiKeyResolver + Send + Sync,
    T: ImageTranscoder + Send + Sync,
    U: common::ProjectImageUpload + Send + Sync,
    F: Fn(
            &str,
        ) -> (
            Vec<crate::image_gen::ModelInfo>,
            Option<crate::image_gen::OrientationSupport>,
        ) + Send
        + Sync
        + 'static,
{
    fn handle<'a>(
        &'a self,
        db: &'a Db,
        job: &'a crate::db::background_jobs::BackgroundJob,
    ) -> crate::services::job_runner::JobFuture<'a> {
        Box::pin(async move {
            let payload_json: Value = serde_json::from_str(&job.payload).unwrap_or(Value::Null);
            let payload = match StoryBackgroundPayload::from_json(&payload_json) {
                Ok(p) => p,
                Err(e) => return crate::services::job_runner::JobOutcome::Failed(e),
            };
            let deps = StoryJobDeps {
                image_provider: &self.image_provider,
                completion: &self.completion,
                moderation: &self.moderation,
                api_keys: &self.api_keys,
                transcoder: &self.transcoder,
                upload: &self.upload,
                executor: &self.executor,
                now_ms: self.now_ms,
                orientation_data_for: &self.orientation_data_for as &common::OrientationDataFn,
            };
            match handle_story_background_generation(db, &deps, &job.user_id, &payload).await {
                Ok(()) => crate::services::job_runner::JobOutcome::Completed(None),
                Err(e) => crate::services::job_runner::JobOutcome::Failed(e),
            }
        })
    }
}

/// The owned inputs for the storage write closure.
struct StoryWriteInput {
    user_id: String,
    file_id: String,
    now_iso: String,
    converted_bytes: Vec<u8>,
    converted_mime: String,
    converted_filename: String,
    converted_width: Option<f64>,
    converted_height: Option<f64>,
    final_prompt: String,
    generation_model: String,
    revised_prompt: Option<String>,
    description: String,
    linked_to: Vec<String>,
    project_id: Option<String>,
    project_upload: Option<common::ProjectUploadResult>,
}

/// The storage half: the Lantern-vs-project branch (branch key: `payload.projectId`)
/// → the `files` row (linkedTo `[chatId, ...characterIds]`, tags `[]`).
fn write_story_file(
    main: &rusqlite::Connection,
    mount: &rusqlite::Connection,
    input: &StoryWriteInput,
) -> Result<(), DbError> {
    let sha256 = sha256_hex(&input.converted_bytes);
    let (storage_key, stored_mime, stored_size, file_project_id, file_folder_path) = match (
        &input.project_id,
        &input.project_upload,
    ) {
        // Project → the FsSeam upload result.
        (Some(project_id), Some(upload)) => {
            ensure_legacy_folder(
                main,
                &input.user_id,
                "/story-backgrounds/",
                "story-backgrounds",
                Some(project_id),
            )?;
            (
                upload.storage_key.clone(),
                upload.stored_mime_type.clone(),
                upload.size_bytes,
                Some(project_id.clone()),
                Some("/story-backgrounds/".to_string()),
            )
        }
        (Some(_), None) => {
            return Err(DbError::Key(
                "project upload result missing for project-scoped background".to_string(),
            ))
        }
        // No project → the Lantern Backgrounds store (refuse if unprovisioned).
        (None, _) => {
            if crate::services::image_job_storage::resolve_lantern_backgrounds_mount(main, mount)
                .is_none()
            {
                return Err(DbError::Key(
                        "Lantern Backgrounds mount is not provisioned; cannot persist project-less story background.".to_string(),
                    ));
            }
            let written = write_lantern_background_to_mount_store(
                main,
                mount,
                &input.converted_filename,
                &input.converted_bytes,
                &input.converted_mime,
                "generated",
                Some(&input.description),
            )?;
            (
                written.storage_key,
                written.stored_mime_type,
                written.size_bytes,
                None,
                None,
            )
        }
    };

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
            linked_to: input.linked_to.clone(),
            source: "GENERATED".to_string(),
            category: "IMAGE".to_string(),
            generation_prompt: Some(input.final_prompt.clone()),
            generation_model: Some(input.generation_model.clone()),
            generation_revised_prompt: input.revised_prompt.clone(),
            description: Some(input.description.clone()),
            tags: Vec::new(),
            project_id: file_project_id,
            folder_path: file_folder_path,
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

/// v4 `extractVisibleConversation(events).slice(-20)`, mapped to the scene-task
/// `ChatMessage` shape.
fn extract_recent_messages(events: &[Value], last_n: usize) -> Vec<ChatMessage> {
    let raw: Vec<crate::chat_tasks::RawMessage> = events
        .iter()
        .map(|e| crate::chat_tasks::RawMessage {
            type_: e.get("type").and_then(Value::as_str).map(str::to_string),
            role: e.get("role").and_then(Value::as_str).map(str::to_string),
            content: e.get("content").and_then(Value::as_str).map(str::to_string),
        })
        .collect();
    let visible = crate::chat_tasks::extract_visible_conversation(&raw);
    let start = visible.len().saturating_sub(last_n);
    visible[start..]
        .iter()
        .map(|m| ChatMessage {
            role: m.role.clone(),
            content: m.content.clone(),
        })
        .collect()
}

/// Build the per-character appearance inputs (equipped wardrobe → resolution
/// input), v4's fail-safe try/catch per character.
fn build_appearance_inputs(
    main: &rusqlite::Connection,
    mount: &rusqlite::Connection,
    chat_id: &str,
    characters: &[Value],
) -> Result<Vec<AppearanceResolutionInput>, DbError> {
    let project_mount_point_ids =
        crate::tools::wardrobe_shared::resolve_project_mount_point_ids_for_chat(
            main, mount, chat_id,
        );
    let docs = crate::db::doc_mount_documents::DocMountDocumentsRepository::new(mount);
    let mut inputs = Vec::new();
    for char in characters {
        let char_id = char
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let name = char
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let mut equipped: Vec<WardrobeItemInput> = Vec::new();
        // v4 catches per-character equipped-load failures (uses no items).
        if let Ok(Some(state)) = crate::db::chats_outfits::ChatOutfitsRepository::new(main)
            .get_equipped_outfit_for_character(chat_id, &char_id)
        {
            let slots = crate::wardrobe::Slots::from_value(Some(&state));
            if let Ok(leaves) = crate::tools::wardrobe_shared::resolve_equipped_leaf_items_by_slot(
                main,
                &docs,
                &char_id,
                &slots,
                &project_mount_point_ids,
            ) {
                equipped = leaves
                    .into_iter()
                    .map(|l| WardrobeItemInput {
                        slot: l.slot,
                        title: l.title,
                        description: l.description,
                        image_prompt: l.image_prompt,
                    })
                    .collect();
            }
        }
        inputs.push(AppearanceResolutionInput {
            character_id: char_id,
            character_name: name,
            physical_description: char
                .get("physicalDescription")
                .filter(|v| !v.is_null())
                .map(physical_description_from_value),
            equipped_wardrobe_items: equipped,
        });
    }
    Ok(inputs)
}

/// v4 the uncensored `imagePromptProfileId` selection (`OLLAMA` → local, default
/// baseUrl `http://localhost:11434`).
fn build_uncensored_selection(
    all_profiles: &[Value],
    cheap_settings: Option<&Value>,
) -> Option<CheapLlmSelection> {
    let profile_id = cheap_settings?
        .get("imagePromptProfileId")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())?;
    let profile = all_profiles
        .iter()
        .find(|p| p.get("id").and_then(Value::as_str) == Some(profile_id))?;
    let provider = profile
        .get("provider")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let is_local = provider == "OLLAMA";
    let base_url = if is_local {
        Some(
            profile
                .get("baseUrl")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .unwrap_or("http://localhost:11434")
                .to_string(),
        )
    } else {
        None
    };
    Some(CheapLlmSelection {
        provider,
        model_name: profile
            .get("modelName")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        connection_profile_id: profile
            .get("id")
            .and_then(Value::as_str)
            .map(str::to_string),
        base_url,
        is_local,
        profile_parameters: profile.get("parameters").cloned(),
    })
}

/// v4 `appendMissingCharacterEnumerations`: scan the prompt for user-workspace
/// character names mentioned but not enumerated, and append canonical `Name: desc`
/// entries (longest-name-first, `\b`-bounded, case-insensitive; `escapeRegex`).
fn append_missing_character_enumerations(prompt: &str, user_characters: &[Value]) -> String {
    // Longest names first so "Lady Catherine" wins over "Catherine".
    let mut ordered: Vec<&Value> = user_characters.iter().collect();
    ordered.sort_by(|a, b| {
        let an = a.get("name").and_then(Value::as_str).unwrap_or("").len();
        let bn = b.get("name").and_then(Value::as_str).unwrap_or("").len();
        bn.cmp(&an)
    });

    let mut result = prompt.to_string();
    for char in ordered {
        let name = char.get("name").and_then(Value::as_str).unwrap_or("");
        if name.len() < 2 {
            continue;
        }
        // `\b<name>\b`, case-insensitive.
        if !word_bounded_contains_ci(&result, name) {
            continue;
        }
        // Already enumerated as `Name:` somewhere — leave it.
        if enum_present_ci(&result, name) {
            continue;
        }
        let desc = build_basic_enumeration(char);
        if desc.is_empty() {
            continue;
        }
        // `/[.!?]\s*$/` → ' ' else '. '.
        let trailing = if ends_with_terminal_then_ws(&result) {
            " "
        } else {
            ". "
        };
        // `desc.replace(/\s*\.?\s*$/, '')` — strip trailing ws + optional dot + ws.
        let desc_trimmed = strip_trailing_ws_dot(&desc);
        result = format!(
            "{}{trailing}{name}: {desc_trimmed}.",
            crate::jsstr::js_trim_end(&result)
        );
    }
    result
}

/// v4 `buildBasicEnumeration`: `${genderPrefix}${mediumPrompt || shortPrompt ||
/// longPrompt || fullDescription || name}`.trim().
fn build_basic_enumeration(char: &Value) -> String {
    let name = char.get("name").and_then(Value::as_str).unwrap_or("");
    let primary = char.get("physicalDescription");
    let desc = primary
        .and_then(|p| {
            [
                "mediumPrompt",
                "shortPrompt",
                "longPrompt",
                "fullDescription",
            ]
            .iter()
            .find_map(|k| p.get(*k).and_then(Value::as_str).filter(|s| !s.is_empty()))
        })
        .unwrap_or(name);
    let prefix = gender_prefix_from_pronouns(pronoun_subject(char));
    crate::jsstr::js_trim(&format!("{prefix}{desc}")).to_string()
}

/// The character's subject pronoun (`character.pronouns.subject`).
fn pronoun_subject(character: &Value) -> Option<&str> {
    character
        .get("pronouns")
        .and_then(|p| p.get("subject"))
        .and_then(Value::as_str)
}

/// v4 `physical_description_from_value` (the primary physical-description object →
/// the resolver's tier bundle).
fn physical_description_from_value(v: &Value) -> PhysicalDescription {
    let s = |k: &str| v.get(k).and_then(Value::as_str).map(str::to_string);
    PhysicalDescription {
        id: s("id").unwrap_or_default(),
        name: s("name").unwrap_or_default(),
        usage_context: s("usageContext"),
        short_prompt: s("shortPrompt"),
        medium_prompt: s("mediumPrompt"),
        long_prompt: s("longPrompt"),
        complete_prompt: s("completePrompt"),
    }
}

/// `\b<name>\b` case-insensitive (v4 `new RegExp('\\b' + escapeRegex(name) +
/// '\\b', 'i')`). A word boundary is the ASCII `\w` transition (v4 `\b`).
fn word_bounded_contains_ci(haystack: &str, name: &str) -> bool {
    let hay = haystack.to_lowercase();
    let needle = name.to_lowercase();
    if needle.is_empty() {
        return false;
    }
    let hay_bytes = hay.as_bytes();
    let mut start = 0;
    while let Some(pos) = hay[start..].find(&needle) {
        let idx = start + pos;
        let before_ok = idx == 0 || !is_word_byte(hay_bytes[idx - 1]);
        let after_idx = idx + needle.len();
        let after_ok = after_idx >= hay_bytes.len() || !is_word_byte(hay_bytes[after_idx]);
        if before_ok && after_ok {
            return true;
        }
        start = idx + 1;
        if start >= hay.len() {
            break;
        }
    }
    false
}

/// v4 `new RegExp('(?:^|[.!?]\\s+)' + escapeRegex(name) + '\\s*:\\s', 'i')` —
/// whether the name is already enumerated as a sentence-leading `Name :`.
fn enum_present_ci(haystack: &str, name: &str) -> bool {
    let hay = haystack.to_lowercase();
    let needle = name.to_lowercase();
    let hay_bytes = hay.as_bytes();
    let mut start = 0;
    while let Some(pos) = hay[start..].find(&needle) {
        let idx = start + pos;
        // Prefix must be start-of-string OR `[.!?]\s+`.
        let prefix_ok = idx == 0 || {
            // Find the run of whitespace immediately before idx, then a `.!?`.
            let mut i = idx;
            let mut saw_ws = false;
            while i > 0 && is_ascii_ws(hay_bytes[i - 1]) {
                i -= 1;
                saw_ws = true;
            }
            saw_ws && i > 0 && matches!(hay_bytes[i - 1], b'.' | b'!' | b'?')
        };
        if prefix_ok {
            // After the name: `\s*:\s`.
            let mut j = idx + needle.len();
            while j < hay_bytes.len() && is_ascii_ws(hay_bytes[j]) {
                j += 1;
            }
            if j < hay_bytes.len() && hay_bytes[j] == b':' {
                let k = j + 1;
                if k < hay_bytes.len() && is_ascii_ws(hay_bytes[k]) {
                    return true;
                }
            }
        }
        start = idx + 1;
        if start >= hay.len() {
            break;
        }
    }
    false
}

/// `/[.!?]\s*$/` — the string ends with a terminal punctuation then optional ws.
fn ends_with_terminal_then_ws(s: &str) -> bool {
    let bytes = s.as_bytes();
    let mut i = bytes.len();
    while i > 0 && is_ascii_ws(bytes[i - 1]) {
        i -= 1;
    }
    i > 0 && matches!(bytes[i - 1], b'.' | b'!' | b'?')
}

/// `desc.replace(/\s*\.?\s*$/, '')` — strip a trailing (ws, optional single dot,
/// ws) run.
fn strip_trailing_ws_dot(desc: &str) -> String {
    let trimmed = desc.trim_end_matches(is_char_ws);
    let trimmed = trimmed.strip_suffix('.').unwrap_or(trimmed);
    trimmed.trim_end_matches(is_char_ws).to_string()
}

fn is_word_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}
fn is_ascii_ws(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c)
}
fn is_char_ws(c: char) -> bool {
    crate::jsstr::is_js_ws(c)
}

/// v4 `repos.folders.findByPath` + create — the legacy folder row.
fn ensure_legacy_folder(
    main: &rusqlite::Connection,
    user_id: &str,
    path: &str,
    name: &str,
    project_id: Option<&str>,
) -> Result<(), DbError> {
    let existing = find_folder_by_path(main, user_id, path, project_id)?;
    if existing.is_none() {
        let repo = crate::db::folders::FoldersRepository::new(main);
        repo.create(
            &crate::db::folders::FolderCreate {
                user_id: user_id.to_string(),
                path: path.to_string(),
                name: name.to_string(),
                parent_folder_id: None,
                project_id: project_id.map(str::to_string),
            },
            &crate::db::folders::CreateOptions::default(),
        )?;
    }
    Ok(())
}

fn find_folder_by_path(
    main: &rusqlite::Connection,
    user_id: &str,
    path: &str,
    project_id: Option<&str>,
) -> Result<Option<String>, DbError> {
    let result = match project_id {
        Some(pid) => main.query_row(
            "SELECT id FROM folders WHERE userId = ?1 AND path = ?2 AND projectId = ?3 LIMIT 1",
            rusqlite::params![user_id, path, pid],
            |r| r.get::<_, String>(0),
        ),
        None => main.query_row(
            "SELECT id FROM folders WHERE userId = ?1 AND path = ?2 AND projectId IS NULL LIMIT 1",
            rusqlite::params![user_id, path],
            |r| r.get::<_, String>(0),
        ),
    };
    result.map(Some).or_else(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => Ok(None),
        other => Err(other.into()),
    })
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
