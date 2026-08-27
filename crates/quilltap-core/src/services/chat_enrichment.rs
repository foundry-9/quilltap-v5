//! Chat enrichment — a port of v4 `lib/services/chat-enrichment.service.ts`.
//!
//! [`enrich_participant_summary`] + [`get_character_summary`] are the summary
//! (no-`preloaded`) slice `handleCreate`'s 201 response uses. The **LIST**
//! orchestration ([`enrich_chats_for_list`] / [`enrich_chat_for_list`] /
//! [`enrich_tags`] / [`filter_chats_by_excluded_tags`]) and the **DETAIL**
//! participant path ([`enrich_participant_detail`] / [`get_character_detail`] /
//! [`get_connection_profile`] / [`get_image_profile`]) are the P4.6a Salon-read
//! unit's. `cleanEnrichedChats` (strip `_allTagIds`) is a `#[serde(skip)]`
//! field on [`EnrichedChatSummary`].
//!
//! **P4.65:** the LIST path now carries v4's [`ChatListPreloaded`] batching —
//! [`enrich_chats_for_list`] collects every character/project/story-background
//! id up front, issues ONE batched read per repository, and threads the maps
//! through the per-chat helpers, exactly per v4 `:620-687`. (The P4.6a port
//! had dropped the preload entirely — payload-equivalent but structurally
//! N+1; P4.64 measured it at 97% of the 8.6–12.2 s salon list / dashboard
//! cost on the real instance.) Each helper keeps v4's no-`preloaded` fallback
//! branch for the single-row callers.
//!
//! Character reads go through the vault-overlaid [`characters_read::find_by_id`]
//! (per-row) or [`characters_read::find_by_ids`] (the batch); the per-row
//! avatar resolves through [`crate::photos::resolve_character_avatar`].

use std::collections::{HashMap, HashSet};

use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::db::doc_mount_file_links::{DocMountFileLinksRepository, LinkWithContent};
use crate::db::files::FileEntry;
use crate::db::projects::ProjectsRepository;
use crate::db::{
    api_keys, chats_messages_read, connection_profiles, conversation_chunks, image_profiles,
    memories_read, tags, DbError,
};
use crate::db::{characters_read, files};
use crate::photos::resolve_character_avatar::{
    build_legacy_file_url, build_mount_file_url, resolve_character_avatar,
};

/// Pre-loaded data for batched list enrichment (v4 `ChatListPreloaded`,
/// `chat-enrichment.service.ts:38-56`). Populated once by
/// [`enrich_chats_for_list`] and threaded through the per-chat / per-participant
/// helpers so they skip per-row `findById` calls. Without this, v4's 287 chats ×
/// N participants turned into ~500+ `characters.findById` calls, each of which
/// triggered the 8-query `applyDocumentStoreOverlay` block — a 4000+ query stall
/// right after startup. (v5 re-measured the same shape at P4.64: ~2,000 vault
/// lookups and 97% of an 8.6–12.2 s list on the real instance.)
pub struct ChatListPreloaded {
    /// Overlaid character rows by id ([`characters_read::find_by_ids`] — a
    /// character whose vault is unavailable is DROPPED from the batch, so a map
    /// miss here answers `character: null` where the per-row fallback errors).
    pub characters: HashMap<String, Value>,
    /// Story-background `files` rows by id.
    pub files: HashMap<String, FileEntry>,
    /// Vault link ids resolved up front for character avatars. Post-Phase-3
    /// `defaultImageId` carries `doc_mount_file_links.id`; we look them up
    /// alongside `files` so the per-character enrichment hot path stays one map
    /// lookup (v4's comment, carried).
    pub doc_mount_file_links: HashMap<String, LinkWithContent>,
    /// Hydrated store-backed projects by id (unavailable stores dropped, like
    /// characters).
    pub projects: HashMap<String, Value>,
    /// Memory counts per chatId (zero when absent).
    pub memory_counts: HashMap<String, i64>,
    /// Conversation-chunk `(total, embedded)` per chatId (absent when no chunks
    /// exist). Used together with `chat.renderedMarkdown` to derive
    /// `scriptoriumStatus`.
    pub conversation_chunk_counts: HashMap<String, (i64, i64)>,
}

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

/// v4 `getCharacterSummary(characterId, repos)` — the no-`preloaded` call shape
/// (`handleCreate`'s 201 response). Kept as a thin wrapper so the single-row
/// callers don't carry a `None` literal.
pub fn get_character_summary(
    main: &Connection,
    mount: &Connection,
    character_id: &str,
) -> Result<Option<EnrichedCharacterSummary>, DbError> {
    get_character_summary_preloaded(main, mount, character_id, None)
}

/// v4 `getCharacterSummary(characterId, repos, preloaded?)`: the overlaid
/// character + its resolved avatar → the summary shape. `None` when the
/// character row is absent.
///
/// When `preloaded` is supplied, both the character and its defaultImage are
/// read from the pre-fetched maps instead of hitting the repository — this is
/// the batched list path (v4's comment, carried). A map MISS is `None`, never a
/// fallback read: v4's preloaded branch is `preloaded.characters.get(id) ??
/// null`, so an unavailable-vault character (dropped from the batch) answers
/// `character: null` here where the per-row branch would error. The avatar
/// tries the vault-link map first, then the `files` map — which holds only
/// story backgrounds, so a legacy-file avatar resolves in the list only when
/// its id doubles as some chat's story background (v4 `:252-264`, faithfully).
fn get_character_summary_preloaded(
    main: &Connection,
    mount: &Connection,
    character_id: &str,
    preloaded: Option<&ChatListPreloaded>,
) -> Result<Option<EnrichedCharacterSummary>, DbError> {
    let character = match preloaded {
        Some(p) => p.characters.get(character_id).cloned(),
        None => characters_read::find_by_id(main, mount, character_id)?,
    };
    let Some(character) = character else {
        return Ok(None);
    };

    let mut default_image: Option<EnrichedImage> = None;
    let default_image_id = s(&character, "defaultImageId");
    if let Some(did) = default_image_id.as_deref() {
        match preloaded {
            Some(p) => {
                if let Some(link) = p.doc_mount_file_links.get(did) {
                    default_image = Some(EnrichedImage {
                        id: did.to_string(),
                        filepath: build_mount_file_url(&link.mount_point_id, &link.relative_path),
                        url: None,
                    });
                } else if let Some(file_entry) = p.files.get(did) {
                    default_image = Some(EnrichedImage {
                        id: file_entry.id.clone(),
                        filepath: build_legacy_file_url(&file_entry.id),
                        url: None,
                    });
                }
            }
            None => {
                if let Some(resolved) = resolve_character_avatar(main, mount, Some(did))? {
                    default_image = Some(EnrichedImage {
                        id: resolved.id,
                        filepath: resolved.url,
                        url: None,
                    });
                }
            }
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

/// v4 `enrichParticipantSummary(participant, repos)` — the no-`preloaded` call
/// shape (`handleCreate`'s 201 response).
pub fn enrich_participant_summary(
    main: &Connection,
    mount: &Connection,
    participant: &Value,
) -> Result<EnrichedParticipantSummary, DbError> {
    enrich_participant_summary_preloaded(main, mount, participant, None)
}

/// v4 `enrichParticipantSummary(participant, repos, preloaded?)`: the
/// participant Value (an element of the chat's `participants` array) → the
/// summary shape, threading `preloaded` down to the character read.
fn enrich_participant_summary_preloaded(
    main: &Connection,
    mount: &Connection,
    participant: &Value,
    preloaded: Option<&ChatListPreloaded>,
) -> Result<EnrichedParticipantSummary, DbError> {
    let kind = s(participant, "type").unwrap_or_else(|| "CHARACTER".to_string());
    let character_id = s(participant, "characterId");
    let character = match (kind.as_str(), character_id.as_deref()) {
        ("CHARACTER", Some(cid)) => get_character_summary_preloaded(main, mount, cid, preloaded)?,
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
    /// Archive tombstone timestamp — participant chips badge archived seats
    /// (character-archive spec §5.2). Mirrors the enrichment in
    /// `app/api/v1/chats/[id]/helpers.ts` (v5:
    /// [`crate::services::chat_participants::EnrichedParticipantCharacter::archived_at`]).
    /// v4 bug 66, `aa464abf`.
    pub archived_at: Option<String>,
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
    // v4 `character.archivedAt ?? null` — read once, carried on BOTH returns.
    let archived_at = s(&character, "archivedAt");

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
                            archived_at,
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
        // Bug 66: the chat GET renders the sidebar, so the archive badge needs
        // this projection here too — not only in the participants/PUT enrichment.
        archived_at,
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
// `filterChatsByExcludedTags` / `cleanEnrichedChats`) — since P4.65, the real
// `ChatListPreloaded` batched orchestration, with v4's per-row fallback arms.
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

/// v4 `enrichChatForList(chat, repos, preloaded?)`: the per-chat
/// `EnrichedChatSummary`. On the list path `preloaded` is always supplied by
/// [`enrich_chats_for_list`]; the `None` arms are v4's per-row fallback
/// branches, kept for shape fidelity (v5 has no single-chat caller today).
///
/// Until P4.65 the list path approximated v4's preloaded avatar semantics with
/// a per-row vault-link-only lookup; the real preload now carries v4's exact
/// two-step (the `docMountFileLinks` map, then the story-background `files`
/// map) inside [`get_character_summary_preloaded`].
pub fn enrich_chat_for_list(
    main: &Connection,
    mount: &Connection,
    chat: &Value,
    preloaded: Option<&ChatListPreloaded>,
) -> Result<EnrichedChatSummary, DbError> {
    let chat_id = s(chat, "id").unwrap_or_default();

    let raw_participants = chat
        .get("participants")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut participants = Vec::with_capacity(raw_participants.len());
    for p in &raw_participants {
        participants.push(enrich_participant_summary_preloaded(
            main, mount, p, preloaded,
        )?);
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

    // project (v4: `preloaded ? preloaded.projects.get(id) ?? null : findById`
    // — a map miss is null, never a fallback read; an unavailable project store
    // was dropped from the batch, where the per-row `findById` throws).
    let project = match s(chat, "projectId") {
        Some(pid) => match preloaded {
            Some(p) => p.projects.get(&pid).cloned(),
            None => ProjectsRepository::new(main, mount)
                .find_by_id(&pid)
                .map_err(|e| DbError::Internal(format!("project overlay: {e}")))?,
        }
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
            let bg_file = match preloaded {
                Some(p) => p.files.get(&bg_id).cloned(),
                None => files::FilesRepository::new(main).find_by_id(&bg_id)?,
            };
            bg_file.map(|f| EnrichedStoryBackground {
                filepath: build_legacy_file_url(&f.id),
                id: f.id,
            })
        }
        None => None,
    };

    // _count
    let messages = chats_messages_read::get_message_count(main, &chat_id)?;
    // Memory count: prefer the bulk-preloaded value (`?? 0`); fall back to the
    // per-chat read when the single-chat path calls in without preload (v4's
    // comment, carried).
    let memories = match preloaded {
        Some(p) => p.memory_counts.get(&chat_id).copied().unwrap_or(0),
        None => memories_read::count_by_chat_id(main, &chat_id)?,
    };

    // scriptorium status. With preload the map is consulted for every chat but
    // only holds rendered chats' rows (v4 reads it unconditionally too); the
    // fallback queries only when renderedMarkdown is truthy, as v4's does.
    let has_rendered = chat
        .get("renderedMarkdown")
        .and_then(Value::as_str)
        .map(|s| !s.is_empty())
        .unwrap_or(false);
    let chunk_stats: Option<(i64, i64)> = match preloaded {
        Some(p) => p.conversation_chunk_counts.get(&chat_id).copied(),
        None if has_rendered => Some(
            conversation_chunks::ConversationChunksRepository::new(main)
                .count_stats_by_chat_id(&chat_id)?,
        ),
        None => None,
    };
    let scriptorium_status = if !has_rendered {
        "none".to_string()
    } else {
        match chunk_stats {
            Some((total, embedded)) if total > 0 && embedded >= total => "embedded".to_string(),
            _ => "rendered".to_string(),
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

/// v4 `enrichChatsForList`'s ordering pass on its own — `lastMessageAt ??
/// updatedAt`, descending, stable (V8's sort is stable too, so ties keep input
/// order).
///
/// Exposed because the key is a **raw chat field**: nothing the enrichment
/// produces feeds it. A caller that renders only the first N may therefore sort
/// and truncate BEFORE enriching and get byte-identical rows without paying for
/// the enrichment of the ones it discards (P4.64 — the home dashboard). Kept as
/// the single home of the comparator so the two spellings cannot drift.
pub fn sort_chats_for_list(chats: &mut [Value]) {
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
}

/// v4 `enrichChatsForList(chats, repos)` (`:620-687`): sort descending by
/// `lastMessageAt ?? updatedAt` (stable — ties keep the input order), build the
/// [`ChatListPreloaded`] maps with ONE batched read per repository, then
/// per-chat enrich on the preload.
///
/// v4's build order, kept exactly: one collection pass over the sorted chats;
/// `characters.findByIds` FIRST (its results seed the avatar-id set from the
/// returned characters' `defaultImageId`); then the five remaining batched
/// reads (v4 runs them in one `Promise.all` — concurrency only, same
/// connection underneath; sequential here).
pub fn enrich_chats_for_list(
    main: &Connection,
    mount: &Connection,
    chats: Vec<Value>,
) -> Result<Vec<EnrichedChatSummary>, DbError> {
    let mut chats = chats;
    sort_chats_for_list(&mut chats);

    // ONE pass collecting the cross-chat id sets (v4 `:631-640`). Insertion
    // order with a seen-set — the order feeds only `IN` queries whose results
    // land in maps, but determinism is free. JS-truthiness: v4's `if
    // (chat.projectId)` / `p.characterId` guards skip empty strings.
    let mut character_ids: Vec<String> = Vec::new();
    let mut project_ids: Vec<String> = Vec::new();
    let mut file_ids: Vec<String> = Vec::new();
    let mut seen_characters = HashSet::new();
    let mut seen_projects = HashSet::new();
    let mut seen_files = HashSet::new();
    for chat in &chats {
        if let Some(pid) = s(chat, "projectId").filter(|v| !v.is_empty()) {
            if seen_projects.insert(pid.clone()) {
                project_ids.push(pid);
            }
        }
        if let Some(fid) = s(chat, "storyBackgroundImageId").filter(|v| !v.is_empty()) {
            if seen_files.insert(fid.clone()) {
                file_ids.push(fid);
            }
        }
        for p in chat
            .get("participants")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or_default()
        {
            // The same type-defaulting spelling `enrich_participant_summary`
            // uses (a missing `type` reads as CHARACTER), so the collection
            // pass and the lookup pass can never disagree about a seat.
            let kind = s(p, "type").unwrap_or_else(|| "CHARACTER".to_string());
            if kind == "CHARACTER" {
                if let Some(cid) = s(p, "characterId").filter(|v| !v.is_empty()) {
                    if seen_characters.insert(cid.clone()) {
                        character_ids.push(cid);
                    }
                }
            }
        }
    }

    // Characters first (v4 `:641-652`): the batch drops unavailable-vault rows,
    // and the RETURNED characters seed the avatar-id set.
    let characters = characters_read::find_by_ids(main, mount, &character_ids)?;
    let mut character_avatar_ids: Vec<String> = Vec::new();
    let mut seen_avatars = HashSet::new();
    for character in &characters {
        if let Some(did) = s(character, "defaultImageId").filter(|v| !v.is_empty()) {
            if seen_avatars.insert(did.clone()) {
                character_avatar_ids.push(did);
            }
        }
    }
    let characters_map: HashMap<String, Value> = characters
        .into_iter()
        .filter_map(|c| s(&c, "id").map(|id| (id, c)))
        .collect();

    let chat_ids: Vec<String> = chats.iter().filter_map(|c| s(c, "id")).collect();
    // Restrict the chunk-count query to chats that actually have rendered
    // markdown — every other chat is unambiguously 'none' and querying for it
    // is wasted work. Memory counts run over every chat ID since a chat can
    // accrue memories without ever being rendered. (v4 `:655-661`.)
    let rendered_chat_ids: Vec<String> = chats
        .iter()
        .filter(|c| {
            c.get("renderedMarkdown")
                .and_then(Value::as_str)
                .map(|s| !s.is_empty())
                .unwrap_or(false)
        })
        .filter_map(|c| s(c, "id"))
        .collect();

    // The five remaining batched reads (v4's `Promise.all`, `:663-675`).
    // Story backgrounds still resolve through the legacy `files` table (they
    // live in the Lantern Backgrounds mount but `chat.storyBackgroundImageId`
    // continues to point at a `files` row by design — v4's comment, carried).
    let files_rows = files::FilesRepository::new(main).find_by_ids(&file_ids)?;
    let links =
        DocMountFileLinksRepository::new(mount).find_by_ids_with_content(&character_avatar_ids)?;
    let projects_rows = ProjectsRepository::new(main, mount)
        .find_by_ids(&project_ids)
        .map_err(|e| DbError::Internal(format!("project overlay: {e}")))?;
    let memory_counts_json = memories_read::count_by_chat_ids(main, &chat_ids)?;
    let conversation_chunk_counts = conversation_chunks::ConversationChunksRepository::new(main)
        .count_by_chat_ids(&rendered_chat_ids)?;

    let preloaded = ChatListPreloaded {
        characters: characters_map,
        files: files_rows.into_iter().map(|f| (f.id.clone(), f)).collect(),
        doc_mount_file_links: links.into_iter().map(|l| (l.id.clone(), l)).collect(),
        projects: projects_rows
            .into_iter()
            .filter_map(|p| s(&p, "id").map(|id| (id, p)))
            .collect(),
        memory_counts: memory_counts_json
            .as_object()
            .map(|m| {
                m.iter()
                    .filter_map(|(k, v)| v.as_i64().map(|n| (k.clone(), n)))
                    .collect()
            })
            .unwrap_or_default(),
        conversation_chunk_counts,
    };

    let mut out = Vec::with_capacity(chats.len());
    for c in &chats {
        out.push(enrich_chat_for_list(main, mount, c, Some(&preloaded))?);
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
