//! Wardrobe effects — where a custom tool's `wardrobe` gestures land (v4
//! `lib/pascal/wardrobe-effects.ts`, NEW at `f0db8cbb8`).
//!
//! A separate applier from [`super::side_effects`] on purpose. That one is a
//! read-once, batch-commit over whole-object stores; this one is a sequence of
//! gestures through the outfit chokepoint, each of which loads the current
//! slots and commits on its own. Different doctrines, different files.
//!
//! ## Doctrine
//!
//! - **No new writer.** Every change goes through [`wear_item`] /
//!   [`take_off_wearable`] (`wear_ops`) or [`apply_displacement`] itself, and
//!   the side effects of a change (avatar refresh, Aurora's announcement)
//!   through [`notify_wardrobe_changed`] — once, after the loop, and only when
//!   something actually changed.
//! - **The rolling character wears it**, and nobody else. A run nobody made
//!   dresses nobody.
//! - **One pool per run**, the same one a `wardrobe_*` tool call reads
//!   ([`load_tool_pool`]). References resolve by id, then case-insensitive
//!   title.
//! - **Sequential.** Gestures go one by one so each sees the last: the slots
//!   are threaded gesture to gesture from each commit's own return. Never batch
//!   the slot math locally — the wear ledger credits one wear per committed
//!   transition.
//! - **Source is `tool`.** A Pascal effect is a tool wearing something. (The
//!   take-off modes commit as `take-off` whatever is passed — the chokepoint
//!   forces it.)
//! - **Fail-soft per item, never per run.** An item that cannot be resolved,
//!   or whose commit fails, is skipped with a log; the rest proceed. Never
//!   fails.
//!
//! ## The two halves (v5)
//!
//! v4 awaits `notifyWardrobeChanged` inside `applyWardrobeEffects`. v5's
//! effect applier runs SYNC inside ONE [`Db::write`] closure, and the notify
//! takes the [`Db`] (it enqueues, and refreshes the avatar), so it cannot run
//! there — the wardrobe tool table's two-halves rule. [`apply_wardrobe_effects`]
//! is the in-write half and answers whether anything landed;
//! [`finish_wardrobe_effects`] is the after-commit half
//! (`side_effects::apply_effects_for_run` calls it once the write returns).
//! The in-write lines are logged on the WRITER thread; the after-commit DEBUG
//! on the caller's.

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use rusqlite::Connection;
use serde::ser::SerializeMap;
use serde::{Serialize, Serializer};
use serde_json::Value;

use super::custom_tool_types::{PutOnMode, WardrobeEffectAction};
use super::side_effects::AppliedEffect;
use crate::db::fallback::error_text;
use crate::db::runtime::Db;
use crate::db::wardrobe_wear_stats::EquipSource;
use crate::services::outfit_change_effects::{notify_wardrobe_changed, OutfitChangeContext};
use crate::services::outfit_displacement::apply_displacement;
use crate::slot_ops::{ComputeDisplacedOptions, DisplacementMode};
use crate::tools::wardrobe_tool_table::load_tool_pool;
use crate::wardrobe::Slots;
use crate::wear_ops::{
    find_in_pool, load_current_slots, resolve_wearable, take_off_wearable,
    wardrobe_item_not_found_message, wear_item, WearableRef, WearableResolution,
};

pub use super::custom_tools::ResolvedWardrobeGesture;

/// The module's log context (v4 `CONTEXT`) — and the `sourceContext` the ONE
/// notify hands down, so the avatar trigger's and the scheduler's own lines
/// carry it on the no-set path.
pub const CONTEXT: &str = "pascal.wardrobe-effects";

/// The `target` an applied wardrobe gesture records in `pascalMeta.effects`
/// (v4 `WardrobeEffectTarget`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WardrobeEffectTarget {
    Wear,
    TakeOff,
    ClearSlot,
}

impl WardrobeEffectTarget {
    pub fn as_str(self) -> &'static str {
        match self {
            WardrobeEffectTarget::Wear => "wardrobe.wear",
            WardrobeEffectTarget::TakeOff => "wardrobe.take_off",
            WardrobeEffectTarget::ClearSlot => "wardrobe.clear_slot",
        }
    }
}

/// The item that went on or came off.
#[derive(Debug, Clone, PartialEq)]
pub struct WardrobeEffectItem {
    pub id: String,
    pub title: String,
}

/// What one landed wardrobe gesture records as its `next` (v4
/// `WardrobeEffectRecord`).
#[derive(Debug, Clone, PartialEq)]
pub struct WardrobeEffectRecord {
    /// Absent on `clear_slot`.
    pub item: Option<WardrobeEffectItem>,
    /// The one slot named: `add_to_slot`, `clear_slot`, or a restricted
    /// `take_off`.
    pub slot: Option<String>,
    pub slots_affected: Vec<String>,
    /// `layered` / `replaced` / `removed` / `cleared`.
    pub effect: &'static str,
    /// The character's slots after this change.
    pub slots: Slots,
}

impl Serialize for WardrobeEffectRecord {
    /// v4's object literals, per site: `{ item, slot?, slotsAffected, effect,
    /// slots }` for a wear or a take-off, `{ slot, slotsAffected, effect, slots
    /// }` for a clear — `item` and `slot` only when present.
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut m = s.serialize_map(None)?;
        if let Some(item) = &self.item {
            let mut i = serde_json::Map::new();
            i.insert("id".into(), Value::String(item.id.clone()));
            i.insert("title".into(), Value::String(item.title.clone()));
            m.serialize_entry("item", &Value::Object(i))?;
        }
        if let Some(slot) = &self.slot {
            m.serialize_entry("slot", slot)?;
        }
        m.serialize_entry("slotsAffected", &self.slots_affected)?;
        m.serialize_entry("effect", self.effect)?;
        m.serialize_entry("slots", &self.slots.to_value())?;
        m.end()
    }
}

/// An applied effect with the index of the definition effect that produced it,
/// for ordering (v4 `IndexedAppliedEffect`). The write pass yields these too.
#[derive(Debug, Clone, PartialEq)]
pub struct IndexedAppliedEffect {
    pub index: usize,
    pub applied: AppliedEffect,
}

/// v4 `ApplyWardrobeEffectsParams`, minus `userId` and the turn's set — both
/// are the after-commit half's ([`finish_wardrobe_effects`]); nothing in the
/// write reads them.
pub struct ApplyWardrobeEffectsParams<'a> {
    pub chat_id: &'a str,
    pub tool_name: &'a str,
    /// The rolling character. `None` (or empty) → every gesture skips
    /// fail-soft.
    pub character_id: Option<&'a str>,
    /// The applicable gestures, each with its definition effect's index.
    pub gestures: &'a [(usize, &'a ResolvedWardrobeGesture)],
}

/// Apply a run's wardrobe gestures to the rolling character — the IN-WRITE
/// half (v4 `applyWardrobeEffects` up to its notify). Returns one entry per
/// change that landed (an item put on or taken off, a slot cleared), in effect
/// order, each tagged with its effect's index, and whether anything landed —
/// the caller then runs [`finish_wardrobe_effects`] once the write commits.
/// Never fails.
///
/// `mount` is `None` on a degraded open (no mount-index writer): the wardrobe
/// cannot be read, which is v5's home for v4's `loadToolPool` catch — v5's
/// pool and slot reads are themselves fallbacks, as v4's are.
pub fn apply_wardrobe_effects(
    main: &Connection,
    mount: Option<&Connection>,
    params: ApplyWardrobeEffectsParams<'_>,
) -> (Vec<IndexedAppliedEffect>, bool) {
    let ApplyWardrobeEffectsParams {
        chat_id,
        tool_name,
        character_id,
        gestures,
    } = params;
    if gestures.is_empty() {
        return (Vec::new(), false);
    }

    let Some(character_id) = character_id.filter(|c| !c.is_empty()) else {
        // A run nobody made dresses nobody — the asymmetry a metadata write
        // keeps.
        tracing::debug!(
            target: "quilltap::pascal",
            context = CONTEXT,
            chatId = chat_id,
            tool = tool_name,
            gestures = gestures.len(),
            "Custom tool: no rolling character, wardrobe effects skipped",
        );
        return (Vec::new(), false);
    };

    let Some(mount) = mount else {
        tracing::warn!(
            target: "quilltap::pascal",
            context = CONTEXT,
            chatId = chat_id,
            tool = tool_name,
            characterId = character_id,
            error = "wardrobe effects require the mount-index database",
            "Custom tool wardrobe effects skipped — the wardrobe could not be read",
        );
        return (Vec::new(), false);
    };

    // ONE pool and ONE slots read for the whole run.
    let pool = load_tool_pool(main, mount, chat_id, character_id);
    let mut slots = load_current_slots(main, chat_id, character_id);

    let skip = |index: usize,
                reason: &str,
                reference: Option<&str>,
                item: Option<&str>,
                slot: Option<&str>| {
        // v4's `...detail` — `{ reference }`, `{ item, slot }` or nothing; an
        // absent key is omitted (v4's `undefined` is dropped on serialisation).
        tracing::debug!(
            target: "quilltap::pascal",
            context = CONTEXT,
            chatId = chat_id,
            tool = tool_name,
            characterId = character_id,
            effectIndex = index,
            reason,
            reference,
            item,
            slot,
            "Custom tool wardrobe effect not applied",
        );
    };
    let warn_failed =
        |index: usize, item: Option<&str>, slot: Option<&str>, error: &crate::db::DbError| {
            tracing::warn!(
                target: "quilltap::pascal",
                context = CONTEXT,
                chatId = chat_id,
                tool = tool_name,
                effectIndex = index,
                item,
                slot,
                error = %error_text(error),
                "Custom tool wardrobe effect failed; that change was dropped",
            );
        };

    let mut applied: Vec<IndexedAppliedEffect> = Vec::new();
    // Record a change when it changed something; a no-op is logged and
    // dropped. `previous` is ALWAYS present (even all-empty) and there is no
    // `tier`.
    let mut land = |index: usize,
                    target: WardrobeEffectTarget,
                    previous: &Slots,
                    record: WardrobeEffectRecord| {
        if *previous == record.slots {
            skip(
                index,
                "the outfit was already so",
                None,
                record.item.as_ref().map(|i| i.title.as_str()),
                record.slot.as_deref(),
            );
            return;
        }
        applied.push(IndexedAppliedEffect {
            index,
            applied: AppliedEffect {
                target: target.as_str().to_string(),
                previous: Some(previous.to_value()),
                next: serde_json::to_value(&record).unwrap_or(Value::Null),
                tier: None,
            },
        });
    };

    for &(index, gesture) in gestures {
        if gesture.action == WardrobeEffectAction::ClearSlot {
            // `items` are ignored on `clear_slot`.
            let Some(slot) = gesture.slot.as_deref() else {
                skip(index, "clear_slot names no slot", None, None, None);
                continue;
            };
            let previous = slots.clone();
            // No source: the chokepoint stamps `take-off` on a take-off mode.
            match apply_displacement(
                main,
                chat_id,
                character_id,
                &ComputeDisplacedOptions {
                    mode: DisplacementMode::ClearSlot,
                    item: None,
                    slot: Some(slot),
                    item_id: None,
                    items_by_id: None,
                },
                EquipSource::Ui,
            ) {
                Ok(next) => slots = next,
                Err(error) => {
                    warn_failed(index, None, Some(slot), &error);
                    continue;
                }
            }
            land(
                index,
                WardrobeEffectTarget::ClearSlot,
                &previous,
                WardrobeEffectRecord {
                    item: None,
                    slot: Some(slot.to_string()),
                    slots_affected: vec![slot.to_string()],
                    effect: "cleared",
                    slots: slots.clone(),
                },
            );
            continue;
        }

        for reference in &gesture.items {
            // The SAME string is tried as an id, then as a title.
            let r = WearableRef {
                item_id: Some(reference),
                item_title: Some(reference),
            };
            let previous = slots.clone();

            if gesture.action == WardrobeEffectAction::Wear {
                let mode = gesture.mode.unwrap_or(PutOnMode::Wear);
                let item = match resolve_wearable(&pool, r, mode, gesture.slot.as_deref()) {
                    WearableResolution::Ok(item) => item,
                    WearableResolution::Refused { message, .. } => {
                        skip(index, &message, Some(reference), None, None);
                        continue;
                    }
                };
                let (id, title) = item_ref(item);
                match wear_item(
                    main,
                    chat_id,
                    &pool,
                    item,
                    mode,
                    gesture.slot.as_deref(),
                    EquipSource::Tool,
                ) {
                    Ok(outcome) => {
                        slots = outcome.slots.clone();
                        land(
                            index,
                            WardrobeEffectTarget::Wear,
                            &previous,
                            WardrobeEffectRecord {
                                item: Some(WardrobeEffectItem {
                                    id,
                                    title: title.clone(),
                                }),
                                slot: gesture
                                    .slot
                                    .clone()
                                    .filter(|_| mode == PutOnMode::AddToSlot),
                                slots_affected: outcome.slots_affected,
                                effect: outcome.effect.as_str(),
                                slots: outcome.slots,
                            },
                        );
                    }
                    Err(error) => warn_failed(index, Some(&title), None, &error),
                }
                continue;
            }

            // take_off — archived items may come off; archiving never
            // undresses anyone.
            let Some(item) = find_in_pool(&pool, r) else {
                skip(
                    index,
                    &wardrobe_item_not_found_message(Some(reference), Some(reference)),
                    Some(reference),
                    None,
                    None,
                );
                continue;
            };
            let (id, title) = item_ref(item);
            match take_off_wearable(main, chat_id, &pool, item, gesture.slot.as_deref()) {
                Ok(outcome) => {
                    slots = outcome.slots.clone();
                    land(
                        index,
                        WardrobeEffectTarget::TakeOff,
                        &previous,
                        WardrobeEffectRecord {
                            item: Some(WardrobeEffectItem {
                                id,
                                title: title.clone(),
                            }),
                            slot: gesture.slot.clone(),
                            slots_affected: outcome.slots_affected,
                            effect: "removed",
                            slots: outcome.slots,
                        },
                    );
                }
                Err(error) => warn_failed(index, Some(&title), None, &error),
            }
        }
    }

    let changed = !applied.is_empty();
    (applied, changed)
}

fn item_ref(item: &Value) -> (String, String) {
    let field = |k: &str| {
        item.get(k)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    (field("id"), field("title"))
}

/// The AFTER-COMMIT half (v4 `applyWardrobeEffects`' tail): the ONE
/// [`notify_wardrobe_changed`] with `sourceContext` [`CONTEXT`] — joining the
/// turn's containers when they were passed, enqueuing + settling at once
/// otherwise — then the `applied` DEBUG. Call only when the in-write half
/// reported a change. The change is recorded as Pascal's, naming the ROLLING
/// character and the custom tool (v4 `provenance: { source: 'pascal',
/// actorCharacterId: characterId, toolName }`, `05005c562` + `4ae20977f`). The
/// `announcement` word is decided by whether a SET was PASSED, never by
/// whether an enqueue happened, and never by the history map (v4's own test;
/// `wardrobe-effects.ts:237` still keys on the set alone).
#[allow(clippy::too_many_arguments)]
pub async fn finish_wardrobe_effects(
    db: &Db,
    user_id: &str,
    chat_id: &str,
    tool_name: &str,
    character_id: &str,
    applied_targets: &[&str],
    pending: Option<&Arc<Mutex<HashSet<String>>>>,
    pending_history: Option<&crate::services::outfit_change_effects::PendingWardrobeHistory>,
) {
    use crate::db::chat_wardrobe_history::{WardrobeHistoryProvenance, WardrobeHistorySource};
    let provenance = WardrobeHistoryProvenance::new(WardrobeHistorySource::Pascal)
        .with_actor(character_id)
        .with_tool_name(tool_name);
    notify_wardrobe_changed(
        db,
        &OutfitChangeContext {
            user_id,
            chat_id,
            character_id,
            provenance: &provenance,
            history_intent: None,
            pending_wardrobe_announcements: pending,
            pending_wardrobe_history: pending_history,
        },
        CONTEXT,
    )
    .await;
    tracing::debug!(
        target: "quilltap::pascal",
        context = CONTEXT,
        chatId = chat_id,
        tool = tool_name,
        characterId = character_id,
        appliedJson = %serde_json::to_string(applied_targets).unwrap_or_default(),
        announcement = if pending.is_some() { "joined the turn" } else { "enqueued now" },
        "Custom tool wardrobe effects applied",
    );
}
