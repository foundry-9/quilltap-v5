//! Arrival defaults — what a character carries when they arrive in a chat (v4
//! `lib/inventory/apply-defaults.ts`, `4fbdad186` at `322a5488e`; P4.D324).
//!
//! Called beside the outfit selections at each of their call sites (chat
//! start, a seat added or reactivated, a merge) with the same `source`:
//!
//!   - with a source chat (a continuation, or a merge's merged chat) that has
//!     an entry for the character, its list is copied — ids only, VERBATIM;
//!     the reader drops what no longer resolves;
//!   - otherwise, for a character **with no entry yet**, every reachable,
//!     non-archived, non-archetype object flagged `equippedByDefault` is
//!     equipped (a flagged kit equips its members) through the equip
//!     chokepoint, with no announcement (arrival is not a gesture) and
//!     leniently: an object over its cap is dropped with a `warn`, never
//!     failing the arrival;
//!   - nothing equipped → the entry is STAMPED `[]`, so a later re-seat does
//!     not apply defaults again.
//!
//! Defaults apply only at arrival; a chat whose operator has changed things
//! is not revisited. [`reset_inventory_to_defaults`] is the by-hand remedy.
//! Never fails: a failure for one character is logged and the rest go on.
//!
//! v4's equip notifies (`announce: false` → the avatar refresh only); here the
//! change comes back (R-1) and the avatar refresh (R-3) runs on the caller's
//! task AFTER the write closure — [`apply_inventory_defaults`] does it itself;
//! a caller of the sync [`apply_inventory_defaults_holding_lock`] runs it
//! after its own closure over the changes it is handed.

use rusqlite::Connection;

use super::equip::{
    equip_items_holding_lock, with_chat_inventory_lock, EquipChat, EquipOptions, EquipResult,
    GestureOutcome, InventoryChange, InventoryChangeContext, InventoryChangeProvenance,
};
use super::equipped_set::{equipped_ids_of, has_equipped_entry};
use super::errors::InventoryError;
use super::pool::{load_inventory_pool, LoadInventoryPoolOptions};
use super::types::InventoryFields;
use crate::db::chats_inventory::ChatsInventoryRepository;
use crate::db::runtime::Db;

/// v4 `InventoryArrivalSource`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InventoryArrivalSource {
    ChatStart,
    ParticipantAdded,
    Merge,
}

impl InventoryArrivalSource {
    pub fn as_str(self) -> &'static str {
        match self {
            InventoryArrivalSource::ChatStart => "chat-start",
            InventoryArrivalSource::ParticipantAdded => "participant-added",
            InventoryArrivalSource::Merge => "merge",
        }
    }
}

/// v4 `ApplyInventoryDefaultsOptions`.
#[derive(Debug, Clone)]
pub struct ApplyInventoryDefaultsOptions {
    pub source: InventoryArrivalSource,
    pub user_id: String,
    /// A continuation's (or a merge's) source chat: its lists are copied.
    pub source_chat_id: Option<String>,
}

/// The source of the R-3 refresh — v4 `refreshAvatarForAppearance(…,
/// 'inventory-change')`.
pub const INVENTORY_CHANGE_AVATAR_CONTEXT: &str =
    crate::services::inventory_change_effects::INVENTORY_CHANGE_SOURCE_CONTEXT;

fn json_text(v: &impl serde::Serialize) -> String {
    serde_json::to_string(v).unwrap_or_default()
}

fn chat_row(main: &Connection, chat_id: &str) -> Option<EquipChat> {
    crate::db::chats_read::find_by_id_or_none(main, chat_id)
        .map(|row| EquipChat::from_chat_row(&row))
}

/// The ids a character's defaults name, kits included — equip expands them
/// (v4 `defaultIdsFor`).
fn default_ids_for(
    main: &Connection,
    mount: &Connection,
    chat: &EquipChat,
    character_id: &str,
) -> Result<Vec<String>, InventoryError> {
    let pool = load_inventory_pool(
        main,
        mount,
        LoadInventoryPoolOptions {
            character_id,
            chat_id: Some(&chat.id),
            project_id: chat.project_id.as_deref(),
            own_records: None,
        },
    )?;
    Ok(pool
        .resolved()
        .into_iter()
        .filter(|item| item.equipped_by_default() && !item.is_archetype() && !item.is_archived())
        .map(|item| item.id().to_string())
        .collect())
}

/// Equip a character's defaults leniently and silently; the caller holds the
/// lock (v4 `equipDefaults`). `None` when there are no defaults — BEFORE the
/// participant check, so a non-participant with no defaults is stamped `[]`.
fn equip_defaults(
    main: &Connection,
    mount: &Connection,
    chat: &EquipChat,
    character_id: &str,
    user_id: &str,
) -> Result<Option<GestureOutcome<EquipResult>>, InventoryError> {
    let ids = default_ids_for(main, mount, chat, character_id)?;
    if ids.is_empty() {
        return Ok(None);
    }
    let ctx = InventoryChangeContext {
        user_id,
        provenance: InventoryChangeProvenance::Operator,
        announce: false,
        pending: None,
    };
    let opts = EquipOptions {
        lenient: true,
        known: Vec::new(),
    };
    equip_items_holding_lock(main, mount, chat, character_id, &ids, &ctx, &opts).map(Some)
}

/// One character's arrival, holding the chat's lock (v4 `arriveUnderLock`).
/// Answers the equip's change, when there was one.
fn arrive_under_lock(
    main: &Connection,
    mount: &Connection,
    chat: &EquipChat,
    source_chat: Option<&EquipChat>,
    character_id: &str,
    opts: &ApplyInventoryDefaultsOptions,
) -> Result<Option<InventoryChange>, InventoryError> {
    let chat_id = chat.id.as_str();
    let source = opts.source.as_str();
    // Read inside the lock: an earlier character's write, or an equip, changed it.
    let current = chat_row(main, chat_id).unwrap_or_else(|| chat.clone());
    if has_equipped_entry(current.equipped_inventory.as_ref(), character_id) {
        tracing::debug!(
            chatId = %chat_id,
            characterId = %character_id,
            source,
            context = "inventory",
            "[InventoryArrival] Character already has an entry; defaults not revisited"
        );
        return Ok(None);
    }
    let repo = ChatsInventoryRepository::new(main);
    if let Some(source_chat) =
        source_chat.filter(|s| has_equipped_entry(s.equipped_inventory.as_ref(), character_id))
    {
        let ids = equipped_ids_of(source_chat.equipped_inventory.as_ref(), character_id);
        repo.set_equipped_inventory(chat_id, character_id, Some(&ids));
        tracing::debug!(
            op = "arrival",
            chatId = %chat_id,
            sourceChatId = %source_chat.id,
            characterId = %character_id,
            source,
            equippedJson = %json_text(&ids),
            context = "inventory",
            "[InventoryArrival] Copied the source chat's carried objects"
        );
        return Ok(None);
    }
    let outcome = equip_defaults(main, mount, &current, character_id, &opts.user_id)?;
    let equipped_nothing = outcome.as_ref().is_none_or(|(r, _)| r.equipped.is_empty());
    if equipped_nothing {
        // Record the arrival, so a later re-seat does not apply defaults again.
        repo.set_equipped_inventory(chat_id, character_id, Some(&[]));
    }
    let ids = |items: &[super::types::ResolvedInventoryItem]| -> Vec<String> {
        items.iter().map(|i| i.id().to_string()).collect()
    };
    let (equipped, created, skipped) = match &outcome {
        Some((r, _)) => (ids(&r.equipped), ids(&r.created), json_text(&r.skipped)),
        None => (Vec::new(), Vec::new(), "[]".to_string()),
    };
    tracing::debug!(
        op = "arrival",
        chatId = %chat_id,
        characterId = %character_id,
        source,
        equippedJson = %json_text(&equipped),
        createdJson = %json_text(&created),
        skippedJson = %skipped,
        context = "inventory",
        "[InventoryArrival] Applied arrival defaults"
    );
    Ok(outcome.and_then(|(_, change)| change))
}

fn warn_whole(chat_id: &str, opts: &ApplyInventoryDefaultsOptions, error: &InventoryError) {
    tracing::warn!(
        chatId = %chat_id,
        source = opts.source.as_str(),
        context = "inventory",
        error = %error,
        "[InventoryArrival] Failed to apply arrival defaults"
    );
}

fn warn_character(
    chat_id: &str,
    character_id: &str,
    opts: &ApplyInventoryDefaultsOptions,
    error: &InventoryError,
) {
    tracing::warn!(
        chatId = %chat_id,
        characterId = %character_id,
        source = opts.source.as_str(),
        context = "inventory",
        error = %error,
        "[InventoryArrival] Failed to apply arrival defaults for a character"
    );
}

fn unique(ids: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for id in ids {
        if !out.contains(id) {
            out.push(id.clone());
        }
    }
    out
}

/// Apply arrival defaults for each character, for a caller ALREADY inside a
/// write closure (chat create, a merge) — the synchronous body of
/// [`apply_inventory_defaults`]. Never fails. Answers each character's
/// non-empty change, for the caller's R-3 avatar refresh after its closure.
pub fn apply_inventory_defaults_holding_lock(
    main: &Connection,
    mount: &Connection,
    chat_id: &str,
    character_ids: &[String],
    opts: &ApplyInventoryDefaultsOptions,
) -> Vec<(String, InventoryChange)> {
    let Some(chat) = chat_row(main, chat_id) else {
        tracing::debug!(
            chatId = %chat_id,
            source = opts.source.as_str(),
            context = "inventory",
            "[InventoryArrival] No such chat; nothing to apply"
        );
        return Vec::new();
    };
    let source_chat = opts
        .source_chat_id
        .as_deref()
        .filter(|s| !s.is_empty())
        .and_then(|id| chat_row(main, id));
    let mut changes = Vec::new();
    for character_id in unique(character_ids) {
        match arrive_under_lock(
            main,
            mount,
            &chat,
            source_chat.as_ref(),
            &character_id,
            opts,
        ) {
            Ok(Some(change)) if !change.is_empty() => changes.push((character_id, change)),
            Ok(_) => {}
            Err(error) => warn_character(chat_id, &character_id, opts, &error),
        }
    }
    changes
}

/// Run the R-3 after-effect for each character whose arrival carried
/// something: v4's `notifyInventoryChanged` with `announce: false` refreshes
/// the per-chat avatar (`'inventory-change'`). The DEBUG `[InventoryEffects]`
/// line around it is P4.D328's.
pub async fn refresh_avatars_after_arrival(
    db: &Db,
    user_id: &str,
    chat_id: &str,
    changes: &[(String, InventoryChange)],
) {
    for (character_id, change) in changes {
        if !change.is_empty() {
            crate::services::outfit_change_effects::refresh_avatar_for_outfit(
                db,
                user_id,
                chat_id,
                character_id,
                INVENTORY_CHANGE_AVATAR_CONTEXT,
            )
            .await;
        }
    }
}

/// Apply arrival defaults for each character (v4 `applyInventoryDefaults`):
/// one lock (one write) per UNIQUE character, first-seen order; a failure for
/// one is WARNed and the rest go on; the R-3 refresh after each write.
pub async fn apply_inventory_defaults(
    db: &Db,
    chat_id: &str,
    character_ids: &[String],
    opts: &ApplyInventoryDefaultsOptions,
) {
    let read = {
        let (chat_id, source_chat_id) = (chat_id.to_string(), opts.source_chat_id.clone());
        db.read_main(move |main| {
            let chat = chat_row(main, &chat_id);
            let source_chat = source_chat_id
                .as_deref()
                .filter(|s| !s.is_empty())
                .and_then(|id| chat_row(main, id));
            Ok((chat, source_chat))
        })
    };
    let (chat, source_chat) = match read {
        Ok(pair) => pair,
        Err(error) => return warn_whole(chat_id, opts, &InventoryError::from(error)),
    };
    let Some(chat) = chat else {
        tracing::debug!(
            chatId = %chat_id,
            source = opts.source.as_str(),
            context = "inventory",
            "[InventoryArrival] No such chat; nothing to apply"
        );
        return;
    };
    for character_id in unique(character_ids) {
        let (chat_c, source_c, id_c, opts_c) = (
            chat.clone(),
            source_chat.clone(),
            character_id.clone(),
            opts.clone(),
        );
        let outcome = with_chat_inventory_lock(db, &chat.id, move |main, mount| {
            arrive_under_lock(main, mount, &chat_c, source_c.as_ref(), &id_c, &opts_c)
        })
        .await;
        match outcome {
            Ok(Some(change)) if !change.is_empty() => {
                refresh_avatars_after_arrival(
                    db,
                    &opts.user_id,
                    chat_id,
                    &[(character_id, change)],
                )
                .await;
            }
            Ok(_) => {}
            Err(error) => warn_character(chat_id, &character_id, opts, &error),
        }
    }
}

/// Clear a character's entry and apply the defaults again, holding the lock
/// (v4 `resetUnderLock`). The entry is now LAST in the map (a delete then a
/// re-add appends — T2). Answers the ids now carried (the SORTED, de-duped
/// stored list) and the equip's change.
pub fn reset_inventory_to_defaults_holding_lock(
    main: &Connection,
    mount: &Connection,
    chat: &EquipChat,
    character_id: &str,
    user_id: &str,
) -> Result<(Vec<String>, Option<InventoryChange>), InventoryError> {
    let repo = ChatsInventoryRepository::new(main);
    repo.set_equipped_inventory(&chat.id, character_id, None);
    let fresh = chat_row(main, &chat.id).unwrap_or_else(|| chat.clone());
    let outcome = equip_defaults(main, mount, &fresh, character_id, user_id)?;
    if outcome.as_ref().is_none_or(|(r, _)| r.equipped.is_empty()) {
        repo.set_equipped_inventory(&chat.id, character_id, Some(&[]));
    }
    let after = chat_row(main, &chat.id).unwrap_or(fresh);
    let equipped = equipped_ids_of(after.equipped_inventory.as_ref(), character_id);
    tracing::debug!(
        op = "reset-defaults",
        chatId = %chat.id,
        characterId = %character_id,
        equippedJson = %json_text(&equipped),
        context = "inventory",
        "[InventoryArrival] Reset to defaults"
    );
    Ok((equipped, outcome.and_then(|(_, change)| change)))
}

/// The operator's *Reset to defaults* (v4 `resetInventoryToDefaults`): the
/// clear, the defaults and the trailing write in one critical section (one
/// write); silent, as arrival is; the R-3 refresh when something was
/// equipped. Answers the ids now carried.
pub async fn reset_inventory_to_defaults(
    db: &Db,
    chat: &EquipChat,
    character_id: &str,
    user_id: &str,
) -> Result<Vec<String>, InventoryError> {
    let (chat_c, id_c, user_c) = (chat.clone(), character_id.to_string(), user_id.to_string());
    let (ids, change) = with_chat_inventory_lock(db, &chat.id, move |main, mount| {
        reset_inventory_to_defaults_holding_lock(main, mount, &chat_c, &id_c, &user_c)
    })
    .await?;
    if let Some(change) = change {
        refresh_avatars_after_arrival(db, user_id, &chat.id, &[(character_id.to_string(), change)])
            .await;
    }
    Ok(ids)
}
