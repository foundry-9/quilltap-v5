//! Reading the wardrobe wear ledger for people and characters — v4
//! `lib/wardrobe/wear-history.ts` (`3ee3b1342`).
//!
//! The ledger itself ([`WardrobeWearStatsRepository`]) answers in ids and
//! timestamps. This module is the one place those become something a reader
//! can use:
//!
//! - [`attach_wear`] — the `wear` response annotation every wardrobe
//!   collection GET carries, from **one** `find_summaries` call. Like `origin`
//!   ([`crate::db::archetype_wardrobe::with_origin`]) it is a read-time
//!   annotation, never a field of the stored item and never accepted on write.
//! - [`resolve_wearers`] — wearer ids to names (and avatars, for the editor).
//!   Reads are RAW (v4 `characters.findByIdRaw`), as in the speaker-names
//!   read: a broken vault costs a label, not a 500.
//! - [`build_wear_history_payload`] — the body of `?action=wear-history`.

use rusqlite::Connection;
use serde_json::{json, Value};

use crate::db::characters_read;
use crate::db::chats_read;
use crate::db::wardrobe_wear_stats::{
    never_worn_summary, WardrobeWearStatsRepository, WardrobeWearer,
};
use crate::services::character_enrichment::enrich_with_default_image;

/// The label for a wearer whose character can no longer be read (v4
/// `DEPARTED_WEARER_LABEL`).
pub const DEPARTED_WEARER_LABEL: &str = "a departed character";

/// The label for the ledger's unattributed row — wearer deleted and folded, or
/// unresolvable on import (v4 `UNATTRIBUTED_WEARER_LABEL`).
pub const UNATTRIBUTED_WEARER_LABEL: &str = "unattributed";

/// v4 `attachWear(items, repos)` — tag every item in a collection read with its
/// wear summary, from a SINGLE `find_summaries` call. Order and every other
/// field are preserved; `wear` is appended LAST (after `origin` — v4's object
/// spread over the already-tagged item); an item the ledger has never seen
/// carries the canonical zero summary. `[]` for no items, with no read and no
/// line.
pub fn attach_wear(main: &Connection, items: Vec<Value>) -> Vec<Value> {
    if items.is_empty() {
        return Vec::new();
    }
    let ids: Vec<String> = items
        .iter()
        .map(|item| {
            item.get("id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        })
        .collect();
    let summaries = WardrobeWearStatsRepository::new(main).find_summaries(&ids);
    tracing::debug!(
        itemCount = items.len(),
        wornCount = summaries.values().filter(|s| s.wear_count > 0).count(),
        context = "wardrobe",
        "Attached wear summaries to wardrobe read"
    );
    items
        .into_iter()
        .zip(ids)
        .map(|(mut item, id)| {
            let summary = summaries
                .get(&id)
                .cloned()
                .unwrap_or_else(never_worn_summary);
            if let Value::Object(map) = &mut item {
                map.insert(
                    "wear".to_string(),
                    serde_json::to_value(summary).unwrap_or(Value::Null),
                );
            }
            item
        })
        .collect()
}

/// How a wearer resolved (v4 `WearerKind`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WearerKind {
    Character,
    Departed,
    Unattributed,
}

/// One wearer, resolved for display (v4 `ResolvedWearer`). Index-aligned with
/// the history's `wearers`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedWearer {
    /// `None` for the unattributed row.
    pub character_id: Option<String>,
    pub name: String,
    pub avatar_url: Option<String>,
    pub kind: WearerKind,
}

/// v4 `resolveWearers(wearers, repos, { avatars })` — each wearer to a display
/// name (and, when asked, an avatar URL). Never fails: the null wearer is
/// [`UNATTRIBUTED_WEARER_LABEL`]; a character that cannot be read is
/// [`DEPARTED_WEARER_LABEL`].
///
/// v4 reads `characters.findByIdRaw` inside a try/catch whose WARN `Could not
/// read wearer; labelling as departed` is UNREACHABLE: `findByIdRaw` is a
/// fallback-mode `safeQuery` (`_findById`) that logs `Error finding entity by
/// ID` and answers `null` on any failure (measured in
/// `wardrobe_wear_history_equivalence` — a corrupt `characters` row takes the
/// departed arm with the repository's lines, never the WARN). v5 reads through
/// the same fallback twin ([`characters_read::find_by_id_raw_or_none`]) and so
/// never logs the WARN either. The avatar's `Could not resolve wearer avatar`
/// catch is unreachable too (see the avatar arm) — both pinned NEGATIVE.
pub fn resolve_wearers(
    main: &Connection,
    mount: &Connection,
    wearers: &[WardrobeWearer],
    avatars: bool,
) -> Vec<ResolvedWearer> {
    resolve_wearers_inner(main, avatars.then_some(mount), wearers)
}

/// v4 `resolveWearers(wearers, repos)` with its default `{ avatars: false }`
/// — names and kinds only, so no mount-index connection is needed (the
/// `wardrobe_read` tool's form, v4 `buildWardrobeReadWear`).
pub fn resolve_wearer_names(main: &Connection, wearers: &[WardrobeWearer]) -> Vec<ResolvedWearer> {
    resolve_wearers_inner(main, None, wearers)
}

/// The shared body: `avatar_mount` is `Some` exactly when v4's `avatars` is
/// set.
fn resolve_wearers_inner(
    main: &Connection,
    avatar_mount: Option<&Connection>,
    wearers: &[WardrobeWearer],
) -> Vec<ResolvedWearer> {
    let mut resolved = Vec::with_capacity(wearers.len());
    for wearer in wearers {
        let Some(character_id) = wearer.character_id.as_deref().filter(|id| !id.is_empty()) else {
            resolved.push(ResolvedWearer {
                character_id: None,
                name: UNATTRIBUTED_WEARER_LABEL.to_string(),
                avatar_url: None,
                kind: WearerKind::Unattributed,
            });
            continue;
        };

        let character = characters_read::find_by_id_raw_or_none(main, character_id);
        // v4 `!character?.name` — an absent row OR an empty name is departed.
        let name = character
            .as_ref()
            .and_then(|c| c.get("name"))
            .and_then(Value::as_str)
            .filter(|n| !n.is_empty());
        let Some(name) = name else {
            resolved.push(ResolvedWearer {
                character_id: Some(character_id.to_string()),
                name: DEPARTED_WEARER_LABEL.to_string(),
                avatar_url: None,
                kind: WearerKind::Departed,
            });
            continue;
        };

        let mut avatar_url = None;
        if let Some(mount) = avatar_mount {
            let default_image_id = character
                .as_ref()
                .and_then(|c| c.get("defaultImageId"))
                .and_then(Value::as_str);
            // v4's `Could not resolve wearer avatar` catch is UNREACHABLE:
            // `resolveCharacterAvatar` runs both of its reads in fallback mode
            // and never throws (measured — `resolve_avatar_read_fails` answers
            // `avatarUrl: null` with no WARN). v5's shared resolver still
            // propagates a read error (the known `resolve_character_avatar`
            // gap, outside this module), so an `Err` here takes v4's `null`
            // and the WARN is pinned NEGATIVE.
            avatar_url = enrich_with_default_image(main, mount, default_image_id)
                .ok()
                .flatten()
                .map(|i| i.filepath);
        }

        resolved.push(ResolvedWearer {
            character_id: Some(character_id.to_string()),
            name: name.to_string(),
            avatar_url,
            kind: WearerKind::Character,
        });
    }
    resolved
}

/// v4 `buildWearHistoryPayload(itemId, repos)` — the `?action=wear-history`
/// body for one item: the ledger's history, each wearer named, and the chat it
/// was last worn in when that chat still exists. EXACTLY `{ history: {
/// wearCount, firstWornAt, lastWornAt, lastWornChatId, wearers: [...] },
/// wearers: [{ characterId, name, avatarUrl }], lastWornChat: { id, title } |
/// null }`.
///
/// The CALLER has already established the item exists in its tier — the item
/// routes' 404 runs BEFORE the ledger is read (v4 `wear-ledger-routes.test.ts`
/// asserts `findHistory` is never called for an item outside the tier).
///
/// v4's last-worn-chat read sits in a try/catch (WARN `Could not read
/// last-worn chat { itemId, chatId, error, context }`) that is UNREACHABLE for
/// the same reason as the wearer read's: `chats.findById` is a fallback-mode
/// `_findById`. v5 reads through [`chats_read::find_by_id_or_none`] — a failed
/// read takes the `null` arm with the repository's ERROR, never the WARN.
pub fn build_wear_history_payload(main: &Connection, mount: &Connection, item_id: &str) -> Value {
    let history = WardrobeWearStatsRepository::new(main).find_history(item_id);
    let wearers: Vec<Value> = resolve_wearers(main, mount, &history.wearers, true)
        .into_iter()
        .map(|w| {
            json!({
                "characterId": w.character_id,
                "name": w.name,
                "avatarUrl": w.avatar_url,
            })
        })
        .collect();

    let last_worn_chat = history
        .last_worn_chat_id
        .as_deref()
        .filter(|id| !id.is_empty())
        .and_then(|chat_id| chats_read::find_by_id_or_none(main, chat_id))
        .map(|chat| {
            json!({
                "id": chat.get("id").cloned().unwrap_or(Value::Null),
                "title": chat.get("title").cloned().unwrap_or(Value::Null),
            })
        });

    tracing::debug!(
        itemId = %item_id,
        wearCount = history.wear_count,
        wearerCount = wearers.len(),
        lastWornChatResolved = last_worn_chat.is_some(),
        context = "wardrobe",
        "Built wear history"
    );

    json!({
        "history": serde_json::to_value(&history).unwrap_or(Value::Null),
        "wearers": wearers,
        "lastWornChat": last_worn_chat.unwrap_or(Value::Null),
    })
}
