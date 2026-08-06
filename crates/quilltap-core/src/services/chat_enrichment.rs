//! Chat enrichment — a port of v4 `lib/services/chat-enrichment.service.ts`.
//!
//! [`enrich_participant_summary`] + [`get_character_summary`] are the summary
//! (no-`preloaded`) slice `handleCreate`'s 201 response uses. The **LIST**
//! orchestration ([`enrich_chats_for_list`] / [`enrich_chat_for_list`] /
//! [`enrich_tags`] / [`filter_chats_by_excluded_tags`]) and the **DETAIL**
//! participant path ([`enrich_participant_detail`] / [`get_character_detail`] /
//! [`get_connection_profile`] / [`get_image_profile`]) are the P4.6a Salon-read
//! unit's — ported here as the no-preloaded path (byte-identical output to v4's
//! batched `preloaded` fast path). `cleanEnrichedChats` (strip `_allTagIds`) is a
//! `#[serde(skip)]` field on [`EnrichedChatSummary`].
//!
//! Character reads go through the vault-overlaid [`characters_read::find_by_id`];
//! the avatar resolves through the ported
//! [`crate::photos::resolve_character_avatar`].

use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::db::projects::ProjectsRepository;
use crate::db::{
    api_keys, chats_messages_read, connection_profiles, conversation_chunks, image_profiles,
    memories_read, tags, DbError,
};
use crate::db::{characters_read, files};
use crate::photos::resolve_character_avatar::{build_legacy_file_url, resolve_character_avatar};

/// Image info for enriched entities (v4 `EnrichedImage`). `url` is always null on
/// this path (v4 sets `url: null` and carries the URL in `filepath`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnrichedImage {
    pub id: String,
    pub filepath: String,
    pub url: Option<String>,
}

/// Character info for list/summary view (v4 `EnrichedCharacterSummary`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnrichedCharacterSummary {
    pub id: String,
    pub name: String,
    pub title: Option<String>,
    pub avatar_url: Option<String>,
    pub default_image_id: Option<String>,
    pub default_image: Option<EnrichedImage>,
    pub talkativeness: f64,
    pub tags: Vec<String>,
}

/// Participant info for list/summary view (v4 `EnrichedParticipantSummary`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnrichedParticipantSummary {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub display_order: i64,
    pub is_active: bool,
    pub status: String,
    pub removed_at: Option<String>,
    pub character: Option<EnrichedCharacterSummary>,
}

fn s(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(Value::as_str).map(str::to_string)
}

/// v4 `getCharacterSummary` (no-`preloaded` branch): the overlaid character +
/// its resolved avatar → the summary shape. `None` when the character row is
/// absent.
pub fn get_character_summary(
    main: &Connection,
    mount: &Connection,
    character_id: &str,
) -> Result<Option<EnrichedCharacterSummary>, DbError> {
    let Some(character) = characters_read::find_by_id(main, mount, character_id)? else {
        return Ok(None);
    };

    let mut default_image: Option<EnrichedImage> = None;
    let default_image_id = s(&character, "defaultImageId");
    if let Some(did) = default_image_id.as_deref() {
        if let Some(resolved) = resolve_character_avatar(main, mount, Some(did))? {
            default_image = Some(EnrichedImage {
                id: resolved.id,
                filepath: resolved.url,
                url: None,
            });
        }
    }

    let avatar_url = default_image.as_ref().map(|i| i.filepath.clone());

    Ok(Some(EnrichedCharacterSummary {
        id: s(&character, "id").unwrap_or_default(),
        name: s(&character, "name").unwrap_or_default(),
        title: s(&character, "title"),
        avatar_url,
        default_image_id,
        default_image,
        talkativeness: character
            .get("talkativeness")
            .and_then(Value::as_f64)
            .unwrap_or(0.5),
        tags: character
            .get("tags")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default(),
    }))
}

/// v4 `enrichParticipantSummary` (no-`preloaded` branch): the participant Value
/// (an element of the chat's `participants` array) → the summary shape.
pub fn enrich_participant_summary(
    main: &Connection,
    mount: &Connection,
    participant: &Value,
) -> Result<EnrichedParticipantSummary, DbError> {
    let kind = s(participant, "type").unwrap_or_else(|| "CHARACTER".to_string());
    let character_id = s(participant, "characterId");
    let character = match (kind.as_str(), character_id.as_deref()) {
        ("CHARACTER", Some(cid)) => get_character_summary(main, mount, cid)?,
        _ => None,
    };

    Ok(EnrichedParticipantSummary {
        id: s(participant, "id").unwrap_or_default(),
        kind,
        display_order: participant
            .get("displayOrder")
            .and_then(Value::as_i64)
            .unwrap_or(0),
        is_active: participant
            .get("isActive")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        status: s(participant, "status").unwrap_or_else(|| "active".to_string()),
        removed_at: s(participant, "removedAt"),
        character,
    })
}

// ===========================================================================
// The DETAIL participant path (v4 `enrichParticipantDetail` / `getCharacterDetail`
// / `getConnectionProfile` / `getImageProfile`) — the FULLER shape the single-chat
// GET + the chat PUT response use. Byte-exact field order per v4.
// ===========================================================================

/// v4 `EnrichedCharacterSystemPrompt` — `{ id, name, isDefault }`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnrichedCharacterSystemPrompt {
    pub id: String,
    pub name: String,
    pub is_default: bool,
}

/// v4 `EnrichedCharacterDetail` — the base summary shape PLUS `systemPrompts`
/// (and NO `tags`, which is the summary-only field).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnrichedCharacterDetail {
    pub id: String,
    pub name: String,
    pub title: Option<String>,
    pub avatar_url: Option<String>,
    pub default_image_id: Option<String>,
    pub default_image: Option<EnrichedImage>,
    pub talkativeness: f64,
    pub system_prompts: Vec<EnrichedCharacterSystemPrompt>,
}

/// v4 `getConnectionProfile`'s `apiKey` summary — `{ id, provider, label }` (no
/// secret material).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiKeySummary {
    pub id: String,
    pub provider: String,
    pub label: String,
}

/// v4 `EnrichedConnectionProfile` — `{ id, name, provider, modelName,
/// allowToolUse, apiKey }` (bug 36, `bd419ae9`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnrichedConnectionProfile {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub model_name: String,
    /// Whether this profile permits tool use — surfaced so the tool-settings
    /// dialog can warn that per-chat tool toggles are moot when it's false.
    /// Between `modelName` and `apiKey` to match v4's key order.
    pub allow_tool_use: bool,
    pub api_key: Option<ApiKeySummary>,
}

/// v4 `EnrichedImageProfile` — `{ id, name, provider, modelName }`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnrichedImageProfile {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub model_name: String,
}

/// v4 `EnrichedParticipantDetail` — the fuller participant view for the
/// single-chat GET + PUT response. Field order matches v4's object literal.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnrichedParticipantDetail {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub controlled_by: String,
    pub display_order: i64,
    pub is_active: bool,
    pub status: String,
    pub removed_at: Option<String>,
    pub character: Option<EnrichedCharacterDetail>,
    pub connection_profile: Option<EnrichedConnectionProfile>,
    pub image_profile: Option<EnrichedImageProfile>,
    pub selected_system_prompt_id: Option<String>,
    pub talkativeness: Option<f64>,
    pub created_at: String,
    pub updated_at: String,
}

/// v4 `getCharacterDetail(characterId, repos, chatId?)`: the overlaid character +
/// `systemPrompts` + the avatar-override (chat-specific) early-return branch.
pub fn get_character_detail(
    main: &Connection,
    mount: &Connection,
    character_id: &str,
    chat_id: Option<&str>,
) -> Result<Option<EnrichedCharacterDetail>, DbError> {
    let Some(character) = characters_read::find_by_id(main, mount, character_id)? else {
        return Ok(None);
    };

    let system_prompts: Vec<EnrichedCharacterSystemPrompt> = character
        .get("systemPrompts")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .map(|p| EnrichedCharacterSystemPrompt {
                    id: s(p, "id").unwrap_or_default(),
                    name: s(p, "name").unwrap_or_default(),
                    is_default: p.get("isDefault").and_then(Value::as_bool).unwrap_or(false),
                })
                .collect()
        })
        .unwrap_or_default();

    let title = s(&character, "title");
    let talkativeness = character
        .get("talkativeness")
        .and_then(Value::as_f64)
        .unwrap_or(0.5);

    // Avatar-override branch: a chat-specific avatar wins over the default image.
    if let Some(cid) = chat_id {
        if let Some(overrides) = character.get("avatarOverrides").and_then(Value::as_array) {
            if let Some(over) = overrides
                .iter()
                .find(|o| o.get("chatId").and_then(Value::as_str) == Some(cid))
            {
                if let Some(image_id) = s(over, "imageId") {
                    if let Some(resolved) = resolve_character_avatar(main, mount, Some(&image_id))?
                    {
                        return Ok(Some(EnrichedCharacterDetail {
                            id: s(&character, "id").unwrap_or_default(),
                            name: s(&character, "name").unwrap_or_default(),
                            title,
                            avatar_url: Some(resolved.url.clone()),
                            default_image_id: Some(image_id),
                            default_image: Some(EnrichedImage {
                                id: resolved.id,
                                filepath: resolved.url,
                                url: None,
                            }),
                            talkativeness,
                            system_prompts,
                        }));
                    }
                }
            }
        }
    }

    let mut default_image: Option<EnrichedImage> = None;
    let default_image_id = s(&character, "defaultImageId");
    if let Some(did) = default_image_id.as_deref() {
        if let Some(resolved) = resolve_character_avatar(main, mount, Some(did))? {
            default_image = Some(EnrichedImage {
                id: resolved.id,
                filepath: resolved.url,
                url: None,
            });
        }
    }
    let avatar_url = default_image.as_ref().map(|i| i.filepath.clone());

    Ok(Some(EnrichedCharacterDetail {
        id: s(&character, "id").unwrap_or_default(),
        name: s(&character, "name").unwrap_or_default(),
        title,
        avatar_url,
        default_image_id,
        default_image,
        talkativeness,
        system_prompts,
    }))
}

/// v4 `getConnectionProfile` — the profile + its `{id, provider, label}` api-key
/// summary (both MAIN-db reads).
pub fn get_connection_profile(
    conn: &Connection,
    profile_id: &str,
) -> Result<Option<EnrichedConnectionProfile>, DbError> {
    let Some(profile) = connection_profiles::find_by_id(conn, profile_id)? else {
        return Ok(None);
    };
    let mut api_key: Option<ApiKeySummary> = None;
    if let Some(api_key_id) = s(&profile, "apiKeyId") {
        if let Some(key) = api_keys::find_by_id(conn, &api_key_id)? {
            api_key = Some(ApiKeySummary {
                id: key.id,
                provider: key.provider,
                label: key.label,
            });
        }
    }
    Ok(Some(EnrichedConnectionProfile {
        id: s(&profile, "id").unwrap_or_default(),
        name: s(&profile, "name").unwrap_or_default(),
        provider: s(&profile, "provider").unwrap_or_default(),
        model_name: s(&profile, "modelName").unwrap_or_default(),
        // v4 `profile.allowToolUse ?? true` — `find_by_id` renders it as a bool
        // (NOT NULL, default 1), so absent can only be the true default.
        allow_tool_use: profile
            .get("allowToolUse")
            .and_then(Value::as_bool)
            .unwrap_or(true),
        api_key,
    }))
}

/// v4 `getImageProfile` — `{ id, name, provider, modelName }`.
pub fn get_image_profile(
    conn: &Connection,
    profile_id: &str,
) -> Result<Option<EnrichedImageProfile>, DbError> {
    let Some(profile) = image_profiles::find_by_id(conn, profile_id)? else {
        return Ok(None);
    };
    Ok(Some(EnrichedImageProfile {
        id: s(&profile, "id").unwrap_or_default(),
        name: s(&profile, "name").unwrap_or_default(),
        provider: s(&profile, "provider").unwrap_or_default(),
        model_name: s(&profile, "modelName").unwrap_or_default(),
    }))
}

/// v4 `enrichParticipantDetail(participant, repos, chatId)`.
pub fn enrich_participant_detail(
    main: &Connection,
    mount: &Connection,
    participant: &Value,
    chat_id: &str,
) -> Result<EnrichedParticipantDetail, DbError> {
    let kind = s(participant, "type").unwrap_or_else(|| "CHARACTER".to_string());
    let character_id = s(participant, "characterId");
    let character = match (kind.as_str(), character_id.as_deref()) {
        ("CHARACTER", Some(cid)) => get_character_detail(main, mount, cid, Some(chat_id))?,
        _ => None,
    };
    let connection_profile = match s(participant, "connectionProfileId") {
        Some(pid) => get_connection_profile(main, &pid)?,
        None => None,
    };
    let image_profile = match s(participant, "imageProfileId") {
        Some(pid) => get_image_profile(main, &pid)?,
        None => None,
    };

    Ok(EnrichedParticipantDetail {
        id: s(participant, "id").unwrap_or_default(),
        kind,
        controlled_by: s(participant, "controlledBy").unwrap_or_else(|| "llm".to_string()),
        display_order: participant
            .get("displayOrder")
            .and_then(Value::as_i64)
            .unwrap_or(0),
        is_active: participant
            .get("isActive")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        status: s(participant, "status").unwrap_or_else(|| "active".to_string()),
        removed_at: s(participant, "removedAt"),
        character,
        connection_profile,
        image_profile,
        selected_system_prompt_id: s(participant, "selectedSystemPromptId"),
        talkativeness: participant.get("talkativeness").and_then(Value::as_f64),
        created_at: s(participant, "createdAt").unwrap_or_default(),
        updated_at: s(participant, "updatedAt").unwrap_or_default(),
    })
}

// ===========================================================================
// The LIST path (v4 `enrichChatsForList` / `enrichChatForList` / `enrichTags` /
// `filterChatsByExcludedTags` / `cleanEnrichedChats`) — the no-preloaded
// orchestration. Byte-identical output to v4's batched `preloaded` fast path.
// ===========================================================================

/// v4 `EnrichedTag` — `{ tag: { id, name } }`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EnrichedTag {
    pub tag: TagIdName,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TagIdName {
    pub id: String,
    pub name: String,
}

/// v4 `EnrichedProject` — `{ id, name, color }`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnrichedProject {
    pub id: String,
    pub name: String,
    pub color: Option<String>,
}

/// v4 `EnrichedStoryBackground` — `{ id, filepath }`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EnrichedStoryBackground {
    pub id: String,
    pub filepath: String,
}

/// v4's per-chat `_count` — `{ messages, memories }`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ChatCountDto {
    pub messages: i64,
    pub memories: i64,
}

/// v4 `EnrichedChatSummary` minus `_allTagIds` (which `cleanEnrichedChats`
/// strips — reproduced here as the `#[serde(skip)]` `all_tag_ids` field). Field
/// order matches v4's object literal.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnrichedChatSummary {
    pub id: String,
    pub title: String,
    pub context_summary: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub last_message_at: Option<String>,
    pub participants: Vec<EnrichedParticipantSummary>,
    pub tags: Vec<EnrichedTag>,
    pub project: Option<EnrichedProject>,
    pub story_background: Option<EnrichedStoryBackground>,
    pub is_dangerous_chat: bool,
    pub concierge_override: Option<String>,
    pub chat_type: String,
    pub scriptorium_status: String,
    #[serde(rename = "_count")]
    pub count: ChatCountDto,
    /// v4 `_allTagIds` — used only by `filterChatsByExcludedTags`, stripped from
    /// the response by `cleanEnrichedChats`. `#[serde(skip)]` reproduces the strip.
    #[serde(skip)]
    pub all_tag_ids: Vec<String>,
}

/// v4 `enrichTags(tagIds)` — the `{ tag: { id, name } }` rows in input order,
/// missing dropped.
pub fn enrich_tags(conn: &Connection, tag_ids: &[String]) -> Result<Vec<EnrichedTag>, DbError> {
    let rows = tags::find_by_ids(conn, tag_ids)?;
    Ok(rows
        .into_iter()
        .map(|r| EnrichedTag {
            tag: TagIdName {
                id: r.id,
                name: r.name,
            },
        })
        .collect())
}

/// The LIST-path avatar resolution: v4's batched `enrichChatsForList` preloads
/// character avatars ONLY as vault links (`docMountFileLinks`) — `preloaded.files`
/// holds story backgrounds, not avatar files — so a **legacy-file** avatar resolves
/// to `null` in the list (unlike the no-preloaded GET/create path, which falls back
/// to the legacy `files` table). Reproduce that vault-only behavior here.
fn resolve_list_avatar(
    mount: &Connection,
    image_id: &str,
) -> Result<Option<EnrichedImage>, DbError> {
    let links = crate::db::doc_mount_file_links::DocMountFileLinksRepository::new(mount);
    if let Some(link) = links.find_by_id_with_content(image_id)? {
        return Ok(Some(EnrichedImage {
            id: image_id.to_string(),
            filepath: crate::photos::resolve_character_avatar::build_mount_file_url(
                &link.mount_point_id,
                &link.relative_path,
            ),
            url: None,
        }));
    }
    Ok(None)
}

/// v4 `getCharacterSummary` with the LIST (batched) avatar semantics (vault-only).
fn get_character_summary_for_list(
    main: &Connection,
    mount: &Connection,
    character_id: &str,
) -> Result<Option<EnrichedCharacterSummary>, DbError> {
    let Some(character) = characters_read::find_by_id(main, mount, character_id)? else {
        return Ok(None);
    };
    let default_image_id = s(&character, "defaultImageId");
    let default_image = match default_image_id.as_deref() {
        Some(did) => resolve_list_avatar(mount, did)?,
        None => None,
    };
    let avatar_url = default_image.as_ref().map(|i| i.filepath.clone());
    Ok(Some(EnrichedCharacterSummary {
        id: s(&character, "id").unwrap_or_default(),
        name: s(&character, "name").unwrap_or_default(),
        title: s(&character, "title"),
        avatar_url,
        default_image_id,
        default_image,
        talkativeness: character
            .get("talkativeness")
            .and_then(Value::as_f64)
            .unwrap_or(0.5),
        tags: character
            .get("tags")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default(),
    }))
}

/// v4 `enrichParticipantSummary` with the LIST avatar semantics.
fn enrich_participant_summary_for_list(
    main: &Connection,
    mount: &Connection,
    participant: &Value,
) -> Result<EnrichedParticipantSummary, DbError> {
    let kind = s(participant, "type").unwrap_or_else(|| "CHARACTER".to_string());
    let character = match (kind.as_str(), s(participant, "characterId").as_deref()) {
        ("CHARACTER", Some(cid)) => get_character_summary_for_list(main, mount, cid)?,
        _ => None,
    };
    Ok(EnrichedParticipantSummary {
        id: s(participant, "id").unwrap_or_default(),
        kind,
        display_order: participant
            .get("displayOrder")
            .and_then(Value::as_i64)
            .unwrap_or(0),
        is_active: participant
            .get("isActive")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        status: s(participant, "status").unwrap_or_else(|| "active".to_string()),
        removed_at: s(participant, "removedAt"),
        character,
    })
}

/// v4 `enrichChatForList(chat, repos)` (no-preloaded): the per-chat
/// `EnrichedChatSummary`.
pub fn enrich_chat_for_list(
    main: &Connection,
    mount: &Connection,
    chat: &Value,
) -> Result<EnrichedChatSummary, DbError> {
    let chat_id = s(chat, "id").unwrap_or_default();

    let raw_participants = chat
        .get("participants")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut participants = Vec::with_capacity(raw_participants.len());
    for p in &raw_participants {
        participants.push(enrich_participant_summary_for_list(main, mount, p)?);
    }

    let chat_tag_ids: Vec<String> = chat
        .get("tags")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    let tags = enrich_tags(main, &chat_tag_ids)?;

    // project
    let project = match s(chat, "projectId") {
        Some(pid) => ProjectsRepository::new(main, mount)
            .find_by_id(&pid)
            .map_err(|e| DbError::Key(format!("project overlay: {e}")))?
            .map(|p| EnrichedProject {
                id: s(&p, "id").unwrap_or_default(),
                name: s(&p, "name").unwrap_or_default(),
                color: s(&p, "color"),
            }),
        None => None,
    };

    // story background (always resolves through the legacy `files` table)
    let story_background = match s(chat, "storyBackgroundImageId") {
        Some(bg_id) => {
            let repo = files::FilesRepository::new(main);
            match repo.find_by_id(&bg_id)? {
                Some(f) => Some(EnrichedStoryBackground {
                    filepath: build_legacy_file_url(&f.id),
                    id: f.id,
                }),
                None => None,
            }
        }
        None => None,
    };

    // _count
    let messages = chats_messages_read::get_message_count(main, &chat_id)?;
    let memories = memories_read::count_by_chat_id(main, &chat_id)?;

    // scriptorium status
    let has_rendered = chat
        .get("renderedMarkdown")
        .and_then(Value::as_str)
        .map(|s| !s.is_empty())
        .unwrap_or(false);
    let scriptorium_status = if !has_rendered {
        "none".to_string()
    } else {
        let (total, embedded) = conversation_chunks::ConversationChunksRepository::new(main)
            .count_stats_by_chat_id(&chat_id)?;
        if total > 0 && embedded >= total {
            "embedded".to_string()
        } else {
            "rendered".to_string()
        }
    };

    // _allTagIds = chat.tags + every participant.character.tags
    let mut all_tag_ids = chat_tag_ids.clone();
    for p in &participants {
        if let Some(c) = &p.character {
            all_tag_ids.extend(c.tags.iter().cloned());
        }
    }

    Ok(EnrichedChatSummary {
        id: chat_id,
        title: s(chat, "title").unwrap_or_default(),
        context_summary: s(chat, "contextSummary"),
        created_at: s(chat, "createdAt").unwrap_or_default(),
        updated_at: s(chat, "updatedAt").unwrap_or_default(),
        last_message_at: s(chat, "lastMessageAt"),
        participants,
        tags,
        project,
        story_background,
        is_dangerous_chat: chat
            .get("isDangerousChat")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        concierge_override: s(chat, "conciergeOverride"),
        chat_type: s(chat, "chatType").unwrap_or_else(|| "salon".to_string()),
        scriptorium_status,
        count: ChatCountDto { messages, memories },
        all_tag_ids,
    })
}

/// v4 `enrichChatsForList(chats, repos)`: sort descending by
/// `lastMessageAt ?? updatedAt`, then per-chat enrich. Stable sort (V8's is
/// stable) — ties keep the input order.
pub fn enrich_chats_for_list(
    main: &Connection,
    mount: &Connection,
    chats: Vec<Value>,
) -> Result<Vec<EnrichedChatSummary>, DbError> {
    let mut chats = chats;
    chats.sort_by(|a, b| {
        let key = |c: &Value| -> i64 {
            let ts = c
                .get("lastMessageAt")
                .and_then(Value::as_str)
                .or_else(|| c.get("updatedAt").and_then(Value::as_str));
            ts.and_then(crate::clock::iso_to_ms).unwrap_or(0)
        };
        // Descending.
        key(b).cmp(&key(a))
    });
    let mut out = Vec::with_capacity(chats.len());
    for c in &chats {
        out.push(enrich_chat_for_list(main, mount, c)?);
    }
    Ok(out)
}

/// v4 `filterChatsByExcludedTags(chats, excludeTagIds)` — drop a chat if ANY of
/// its `_allTagIds` is in the excluded set. Empty exclude → unchanged.
pub fn filter_chats_by_excluded_tags(
    chats: Vec<EnrichedChatSummary>,
    exclude_tag_ids: &[String],
) -> Vec<EnrichedChatSummary> {
    if exclude_tag_ids.is_empty() {
        return chats;
    }
    let excluded: std::collections::HashSet<&str> =
        exclude_tag_ids.iter().map(String::as_str).collect();
    chats
        .into_iter()
        .filter(|c| !c.all_tag_ids.iter().any(|t| excluded.contains(t.as_str())))
        .collect()
}
