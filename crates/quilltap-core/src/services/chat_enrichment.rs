//! Chat participant enrichment — the summary/no-preloaded slice of v4
//! `lib/services/chat-enrichment.service.ts`.
//!
//! [`enrich_participant_summary`] + [`get_character_summary`] are the exact
//! functions `handleCreate`'s 201 response uses (`chat.participants.map(p =>
//! enrichParticipantSummary(p, repos))` — the no-`preloaded` branch). The LIST
//! enrichment (`enrichChatsForList` / `enrichParticipantDetail` / the batched
//! `preloaded` path) is the P4.6 Salon-read unit's and is NOT ported here.
//!
//! Character reads go through the vault-overlaid [`characters_read::find_by_id`];
//! the avatar resolves through the ported
//! [`crate::photos::resolve_character_avatar`].

use rusqlite::Connection;
use serde::Serialize;
use serde_json::Value;

use crate::db::{characters_read, DbError};
use crate::photos::resolve_character_avatar::resolve_character_avatar;

/// Image info for enriched entities (v4 `EnrichedImage`). `url` is always null on
/// this path (v4 sets `url: null` and carries the URL in `filepath`).
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnrichedImage {
    pub id: String,
    pub filepath: String,
    pub url: Option<String>,
}

/// Character info for list/summary view (v4 `EnrichedCharacterSummary`).
#[derive(Clone, Debug, PartialEq, Serialize)]
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
#[derive(Clone, Debug, PartialEq, Serialize)]
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
