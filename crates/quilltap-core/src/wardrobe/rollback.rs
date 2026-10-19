//! The ONE rollback (v4 `lib/wardrobe/history/rollback.ts`, `ebb480edd`) —
//! return a character to an outfit recorded earlier in a chat's outfit history.
//!
//! Served by both callers: the `wardrobe_rollback` tool (the CHARACTER's pool —
//! the project roster applies — ledger source `tool`) and the operator's `POST
//! ?action=wardrobe-rollback` (the OPERATOR's pool, ledger source `ui`). Rolling
//! back is itself a change of clothes: ONE commit through the chokepoint (no
//! bundles — the leaves were credited when first worn), then the usual effects
//! (the avatar refresh and Aurora's announcement), with a CURSOR intent so the
//! history moves its cursor instead of appending. The next change after a
//! rollback lands past every recorded entry (`max + 1`).
//!
//! v5 crosses its write / after-commit seam in TWO halves (P4.D316 R-1), as the
//! wardrobe tools already do: [`rollback_in_write`] runs inside ONE `Db::write`
//! (the reads it decides on, the stale-item lines and the commit, atomic), and
//! [`finish_rollback`] runs after it (the async effects with the cursor intent,
//! then `Rolled a wardrobe back` — AFTER the notify, v4's order). The route and
//! the tool share both halves; neither re-implements a step.

use rusqlite::Connection;
use serde_json::{json, Value};

use crate::db::chat_wardrobe_history::{ChatWardrobeHistoryRepository, WardrobeHistoryIntent};
use crate::db::chats_outfits::ChatOutfitsRepository;
use crate::db::runtime::Db;
use crate::db::wardrobe_wear_stats::EquipSource;
use crate::db::DbError;
use crate::services::outfit_change_effects::{notify_wardrobe_changed, OutfitChangeContext};
use crate::services::wardrobe_wear_commit::{commit_equipped_outfit, CommitEquippedOutfitInput};
use crate::wardrobe::{Slots, WARDROBE_SLOT_TYPES};
use crate::wearable_pool::WearablePool;

/// v4's default `sourceContext`.
pub const CONTEXT: &str = "wardrobe.history.rollback";

/// Why a stale item was left off (v4 `'archived' | 'missing'`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RollbackSkipReason {
    Archived,
    Missing,
}

impl RollbackSkipReason {
    pub fn as_str(self) -> &'static str {
        match self {
            RollbackSkipReason::Archived => "archived",
            RollbackSkipReason::Missing => "missing",
        }
    }
}

/// v4 `RollbackSkippedItem` — an item the target outfit named that could not
/// be put back on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RollbackSkippedItem {
    pub item_id: String,
    /// `null` for a missing item (nothing left to name).
    pub title: Option<String>,
    pub reason: RollbackSkipReason,
}

impl RollbackSkippedItem {
    /// The route's camelCase record (`{ itemId, title, reason }`).
    pub fn to_route_value(&self) -> Value {
        json!({ "itemId": self.item_id, "title": self.title, "reason": self.reason.as_str() })
    }

    /// The tool's snake_case record (`{ item_id, title, reason }`).
    pub fn to_tool_value(&self) -> Value {
        json!({ "item_id": self.item_id, "title": self.title, "reason": self.reason.as_str() })
    }
}

/// v4's four refusal reasons.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RollbackRefusalReason {
    NoHistory,
    AtFirst,
    OutOfRange,
    AlreadyCurrent,
}

impl RollbackRefusalReason {
    pub fn as_str(self) -> &'static str {
        match self {
            RollbackRefusalReason::NoHistory => "no_history",
            RollbackRefusalReason::AtFirst => "at_first",
            RollbackRefusalReason::OutOfRange => "out_of_range",
            RollbackRefusalReason::AlreadyCurrent => "already_current",
        }
    }
}

/// v4's `{ ok: false, reason, message }`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RollbackRefusal {
    pub reason: RollbackRefusalReason,
    pub message: String,
}

/// What the in-write half committed — v4's `{ ok: true, … }` plus the cursor
/// intent the after-commit half hands the effects.
#[derive(Debug, Clone, PartialEq)]
pub struct RollbackStage {
    pub previous_index: i64,
    pub restored_index: i64,
    /// What was actually WRITTEN (stale items removed), never the entry's.
    pub slots: Slots,
    pub skipped: Vec<RollbackSkippedItem>,
    /// `Cursor { index: restored_index }`.
    pub intent: WardrobeHistoryIntent,
}

/// The in-write half's answer.
#[derive(Debug, Clone, PartialEq)]
pub enum RollbackOutcome {
    Rolled(RollbackStage),
    Refused(RollbackRefusal),
}

/// What the in-write half reads.
#[derive(Debug, Clone, Copy)]
pub struct RollbackInput<'a> {
    pub chat_id: &'a str,
    /// Whose outfit rolls back.
    pub character_id: &'a str,
    /// The caller's pool: the tool's `load_tool_pool`, or the operator's
    /// `load_wearable_pool(…, operator: true)`.
    pub pool: &'a WearablePool,
    /// Absolute entry index; `None` = one before the current entry.
    pub index: Option<i64>,
    /// The ledger's source for the commit (`Ui` or `Tool`).
    pub source: EquipSource,
    /// v4's `sourceContext`, the `context` of the stale-item lines.
    pub source_context: &'a str,
}

/// v4 `characterName` — the RAW row's `name` when a NON-EMPTY string (`||`),
/// else `This character`; a failed read answers the fallback too.
fn character_name(main: &Connection, character_id: &str) -> String {
    crate::db::characters_read::find_by_id_raw_or_none(main, character_id)
        .as_ref()
        .and_then(|c| c.get("name"))
        .and_then(Value::as_str)
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| "This character".to_string())
}

/// JS truthiness of an item's `archivedAt` (`null` / absent / `""` → kept).
fn is_archived(item: &Value) -> bool {
    match item.get("archivedAt") {
        None | Some(Value::Null) => false,
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|f| f != 0.0 && !f.is_nan()),
        Some(_) => true,
    }
}

/// v4 `filterStaleItems(pool, target, live)` — keep each id the pool can still
/// put on; keep an archived id already worn in the SAME slot; drop the rest
/// into `skipped`, ONCE per id in first-seen (slot-major) order. So a
/// multi-slot archived item worn in one of its slots stays there and is
/// dropped (reported once) from the others. Pure. `pool.get` reaches every
/// tier, archived items included.
pub fn filter_stale_items(
    pool: &WearablePool,
    target: &Slots,
    live: &Slots,
) -> (Slots, Vec<RollbackSkippedItem>) {
    let mut skipped: Vec<RollbackSkippedItem> = Vec::new();
    let mut slots = Slots::fresh();
    for slot in WARDROBE_SLOT_TYPES {
        for item_id in target.slot(slot) {
            let keep = match pool.get(item_id) {
                None => {
                    if !skipped.iter().any(|s| &s.item_id == item_id) {
                        skipped.push(RollbackSkippedItem {
                            item_id: item_id.clone(),
                            title: None,
                            reason: RollbackSkipReason::Missing,
                        });
                    }
                    false
                }
                Some(item) if !is_archived(item) => true,
                Some(_) if live.slot(slot).contains(item_id) => true,
                Some(item) => {
                    if !skipped.iter().any(|s| &s.item_id == item_id) {
                        skipped.push(RollbackSkippedItem {
                            item_id: item_id.clone(),
                            title: item
                                .get("title")
                                .and_then(Value::as_str)
                                .map(str::to_string),
                            reason: RollbackSkipReason::Archived,
                        });
                    }
                    false
                }
            };
            if keep {
                slots.slot_mut(slot).push(item_id.clone());
            }
        }
    }
    (slots, skipped)
}

/// v4's four refusal sentences, verbatim.
pub fn no_history_message(name: &str) -> String {
    format!("No outfit history yet for {name} in this chat.")
}
pub fn at_first_message(name: &str) -> String {
    format!(
        "{name} is already in the first outfit of this chat (0); there is nothing earlier to return to."
    )
}
pub fn out_of_range_message(target: i64, max: i64) -> String {
    format!("There is no outfit {target}; this chat's outfits are numbered 0 to {max}.")
}
pub fn already_current_message(name: &str, target: i64) -> String {
    format!("{name} is already wearing outfit {target}.")
}

/// v4 `entries.find(...)!.slots` over a timeline with a GAP at the target
/// index: V8's `TypeError`, whose message each caller's catch reports
/// verbatim (P4.D316 R-11). `settle` never creates a gap; a restored archive
/// could.
pub const GAP_ERROR_TEXT: &str = "Cannot read properties of undefined (reading 'slots')";

/// The IN-WRITE half of v4 `rollbackWardrobe` (`:100-170`), on the writer's
/// MAIN connection: the timeline, the refusal ladder (the name read lazily —
/// never for `out_of_range`), the live slots, [`filter_stale_items`], the INFO
/// `Rollback left a stale item off` per skipped item (BEFORE the commit, so
/// the chokepoint's lines follow), then ONE `commit_equipped_outfit` with no
/// bundles. A failed read or commit is the caller's `Err` (v4's throw into
/// the caller's catch). Rolling FORWARD to any recorded index is allowed.
pub fn rollback_in_write(
    main: &Connection,
    input: RollbackInput<'_>,
) -> Result<RollbackOutcome, DbError> {
    let RollbackInput {
        chat_id,
        character_id,
        pool,
        index,
        source,
        source_context,
    } = input;
    let refused = |reason, message| {
        Ok(RollbackOutcome::Refused(RollbackRefusal {
            reason,
            message,
        }))
    };

    let entries = ChatWardrobeHistoryRepository::new(main).find_entries(chat_id, character_id)?;
    let Some(last) = entries.last() else {
        return refused(
            RollbackRefusalReason::NoHistory,
            no_history_message(&character_name(main, character_id)),
        );
    };
    let current = entries.iter().find(|e| e.is_current).unwrap_or(last);
    let max = last.entry_index;
    let target_index = index.unwrap_or(current.entry_index - 1);

    if index.is_none() && current.entry_index == 0 {
        return refused(
            RollbackRefusalReason::AtFirst,
            at_first_message(&character_name(main, character_id)),
        );
    }
    if target_index < 0 || target_index > max {
        return refused(
            RollbackRefusalReason::OutOfRange,
            out_of_range_message(target_index, max),
        );
    }
    if target_index == current.entry_index {
        return refused(
            RollbackRefusalReason::AlreadyCurrent,
            already_current_message(&character_name(main, character_id), target_index),
        );
    }

    let Some(target) = entries.iter().find(|e| e.entry_index == target_index) else {
        return Err(DbError::Internal(GAP_ERROR_TEXT.to_string()));
    };
    // v4 `getEquippedOutfitForCharacter(…) ?? makeEmptyEquippedSlots()` — the
    // fallback reader.
    let live = Slots::from_value(
        ChatOutfitsRepository::new(main)
            .get_equipped_outfit_for_character(chat_id, character_id)
            .as_ref(),
    );
    let (slots, skipped) = filter_stale_items(pool, &target.slots, &live);

    for item in &skipped {
        tracing::info!(
            context = source_context,
            chatId = chat_id,
            characterId = character_id,
            targetIndex = target_index,
            itemId = item.item_id.as_str(),
            reason = item.reason.as_str(),
            "Rollback left a stale item off"
        );
    }

    commit_equipped_outfit(
        main,
        CommitEquippedOutfitInput {
            chat_id,
            character_id,
            next_slots: &slots,
            worn_bundles: &[],
            source,
            at: None,
        },
    )?;

    Ok(RollbackOutcome::Rolled(RollbackStage {
        previous_index: current.entry_index,
        restored_index: target_index,
        slots,
        skipped,
        intent: WardrobeHistoryIntent::Cursor {
            index: target_index,
        },
    }))
}

/// v4's DEBUG `Rolled a wardrobe back` (`:172-180`) — logged AFTER the notify,
/// by [`finish_rollback`] and by the tool table's after-commit half.
pub fn log_rolled_back(
    source_context: &str,
    ctx: &OutfitChangeContext<'_>,
    previous_index: i64,
    restored_index: i64,
    skipped: usize,
) {
    tracing::debug!(
        context = source_context,
        chatId = ctx.chat_id,
        characterId = ctx.character_id,
        previousIndex = previous_index,
        restoredIndex = restored_index,
        skipped = skipped,
        provenance = ctx.provenance.source.as_str(),
        "Rolled a wardrobe back"
    );
}

/// The AFTER-COMMIT half (v4 `:164-180`): the outfit-change effects with the
/// stage's CURSOR intent (the avatar refresh, then the announcement + the
/// cursor — deferred into the turn's map inside a turn, settled at once
/// outside), THEN `Rolled a wardrobe back`. `ctx` is the caller's — its
/// provenance and containers; its `history_intent` is replaced.
pub async fn finish_rollback(
    db: &Db,
    ctx: &OutfitChangeContext<'_>,
    stage: &RollbackStage,
    source_context: &'static str,
) {
    let with_cursor = OutfitChangeContext {
        history_intent: Some(&stage.intent),
        ..*ctx
    };
    notify_wardrobe_changed(db, &with_cursor, source_context).await;
    log_rolled_back(
        source_context,
        ctx,
        stage.previous_index,
        stage.restored_index,
        stage.skipped.len(),
    );
}
