//! Inventory effects — where a custom tool's `inventory` gestures land (v4
//! `lib/pascal/inventory-effects.ts`, NEW at `e1872c94f`).
//!
//! The third applier beside [`super::side_effects`] (value writes) and
//! [`super::wardrobe_effects`] (outfit gestures), and shaped like the second: a
//! sequence of gestures through the equip chokepoints, each of which loads the
//! current set and commits on its own.
//!
//! ## Doctrine (v4's header, spec inventory-tools-and-pascal §5.6)
//!
//! 1. **No character, no gesture.** A run nobody made carries nothing.
//! 2. **One context per run** (`load_inventory_context`), re-read after a
//!    gesture lands so the next one resolves against it; unloadable → warn,
//!    nothing.
//! 3. **Gestures in file order, items in list order.** References resolve
//!    through `resolve_inventory_ref` — `equip` and `unequip` by
//!    `pick_for_gesture`, `pick_up` to archetypes only — and then ONE
//!    `equip_items` / `unequip_items` / `pick_up_items` call per gesture with
//!    the `pascal` provenance. A cap breach fails THAT gesture before any write
//!    and is recorded as skipped with the refusal's own words; the next gesture
//!    proceeds. Fail-soft per gesture, never per run; an unresolved reference
//!    is skipped with a debug log.
//! 4. **Sequential** — and in v5 the writer thread IS the read-your-writes
//!    guarantee: the chokepoints and the re-read run on the writer's own
//!    connections inside ONE write, so v4's per-job `equippedInventory`
//!    overlay (`child-repositories-proxy.ts`) has nothing to do here (NO-PORT).
//! 5. **Audit entries**, one per landed object: `inventory.equip`,
//!    `inventory.unequip`, `inventory.pick_up`, with the equipped ids before
//!    and after.
//! 6. **Never fails.**
//!
//! ## The two halves (v5)
//!
//! v4's chokepoints notify inside each call. v5's (`inventory::equip`, C1)
//! RETURN the change instead, because the notify takes the [`Db`] and cannot
//! run inside the write — the wardrobe applier's two-halves rule.
//! [`apply_inventory_effects`] is the in-write half and collects the changes;
//! [`finish_inventory_effects`] is the after-commit half: ONE
//! `notify_inventory_changed` per landed gesture, in order (with the turn's
//! pending map each nets into the turn's entry; without one each posts its own
//! line), then the `applied` DEBUG. The in-write lines are logged on the
//! WRITER thread; the after-commit DEBUG on the caller's.

use std::collections::HashSet;

use rusqlite::Connection;
use serde::ser::SerializeMap;
use serde::{Serialize, Serializer};
use serde_json::{json, Map, Value};

use super::custom_tool_types::InventoryEffectAction;
use super::side_effects::AppliedEffect;
use super::wardrobe_effects::IndexedAppliedEffect;
use crate::db::runtime::Db;
use crate::inventory::context::{load_inventory_context, LoadedInventoryContext};
use crate::inventory::equip::{
    equip_items_holding_lock, pick_up_items_holding_lock, unequip_items_holding_lock, EquipChat,
    EquipOptions, InventoryChange, InventoryChangeContext, InventoryChangeProvenance,
    PickUpOptions,
};
use crate::inventory::refs::{
    pick_for_gesture, resolve_inventory_ref, Gesture, InventoryContext, ResolveInventoryRefOptions,
    ResolvedInventoryPool,
};
use crate::inventory::resolve::inventory_label;
use crate::inventory::types::{InventoryFields, InventoryKind, ResolvedInventoryItem};
use crate::services::inventory_change_effects::{
    notify_inventory_changed, PendingInventoryAnnouncements,
};

pub use super::custom_tools::ResolvedInventoryGesture;

/// The module's log context (v4 `CONTEXT`).
pub const CONTEXT: &str = "pascal.inventory-effects";

/// The `target` an applied inventory gesture records in `pascalMeta.effects`
/// (v4 `InventoryEffectTarget`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InventoryEffectTarget {
    Equip,
    Unequip,
    PickUp,
}

impl InventoryEffectTarget {
    pub fn as_str(self) -> &'static str {
        match self {
            InventoryEffectTarget::Equip => "inventory.equip",
            InventoryEffectTarget::Unequip => "inventory.unequip",
            InventoryEffectTarget::PickUp => "inventory.pick_up",
        }
    }
}

/// What one landed object records as its `next` (v4 `InventoryEffectRecord`).
#[derive(Debug, Clone, PartialEq)]
pub struct InventoryEffectRecord {
    /// `item.id`.
    pub item_id: String,
    /// `item.title` — `inventory_label` (title, else name).
    pub item_title: String,
    /// True for an instance this gesture made (a pick-up, or a kit's
    /// archetype member). Absent otherwise.
    pub created: bool,
    /// The character's equipped ids after this gesture.
    pub equipped: Vec<String>,
}

impl Serialize for InventoryEffectRecord {
    /// v4's literal: `{ item: { id, title }, created?: true, equipped }`.
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut m = s.serialize_map(None)?;
        let mut item = Map::new();
        item.insert("id".into(), Value::String(self.item_id.clone()));
        item.insert("title".into(), Value::String(self.item_title.clone()));
        m.serialize_entry("item", &Value::Object(item))?;
        if self.created {
            m.serialize_entry("created", &true)?;
        }
        m.serialize_entry("equipped", &self.equipped)?;
        m.end()
    }
}

/// v4 `ApplyInventoryEffectsParams`, minus the turn's pending map — that is
/// the after-commit half's ([`finish_inventory_effects`]).
pub struct ApplyInventoryEffectsParams<'a> {
    pub user_id: &'a str,
    pub chat_id: &'a str,
    pub tool_name: &'a str,
    /// The rolling character. `None` (or empty) → every gesture skips.
    pub character_id: Option<&'a str>,
    /// The applicable gestures, each with its definition effect's index.
    pub gestures: &'a [(usize, &'a ResolvedInventoryGesture)],
    /// The run's context, when the entrance already loaded it.
    pub inventory: Option<&'a dyn InventoryContext>,
}

/// What the in-write half produced.
#[derive(Debug, Default)]
pub struct InventoryEffectsOutcome {
    /// One entry per object that landed, in effect order.
    pub applied: Vec<IndexedAppliedEffect>,
    /// The changes the chokepoints returned, one per landed gesture, in order
    /// — what the after-commit half notifies.
    pub changes: Vec<InventoryChange>,
    /// True when the pass reached its end. v4's `applied` DEBUG is the tail of
    /// the function: an early return (no character, unreadable bag, no chat)
    /// never logs it.
    pub finished: bool,
}

/// The bag the pass resolves against: the entrance's, or one it loaded.
enum Bag<'a> {
    Given(&'a dyn InventoryContext),
    Loaded(Box<LoadedInventoryContext>),
}

impl Bag<'_> {
    fn pool(&self) -> &ResolvedInventoryPool {
        match self {
            Bag::Given(ctx) => ctx.pool().expect("a given bag is used only when loaded"),
            Bag::Loaded(loaded) => &loaded.pool,
        }
    }
    fn equipped_ids(&self) -> &HashSet<String> {
        match self {
            Bag::Given(ctx) => ctx
                .equipped_ids()
                .expect("a given bag is used only when loaded"),
            Bag::Loaded(loaded) => &loaded.equipped_ids,
        }
    }
}

/// v4 `isLoaded(ctx)` — a context with a real pool AND equipped ids (a mock
/// is never loaded).
fn is_loaded(ctx: Option<&dyn InventoryContext>) -> bool {
    ctx.is_some_and(|c| c.pool().is_some() && c.equipped_ids().is_some())
}

/// JS `[...ids].sort()` — UTF-16 code-unit order; ids are ASCII, so a byte
/// sort is the same order.
fn sorted_ids(ids: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut out: Vec<String> = ids.into_iter().collect();
    out.sort();
    out
}

/// One `not applied` DEBUG (v4's `skip`), its detail spread as fields.
fn skip(
    chat_id: &str,
    tool_name: &str,
    character_id: &str,
    index: usize,
    reason: &str,
    detail: &Map<String, Value>,
) {
    match (detail.get("reference"), detail.get("action")) {
        (Some(reference), _) => tracing::debug!(
            target: "quilltap::pascal",
            context = CONTEXT,
            chatId = chat_id,
            tool = tool_name,
            characterId = character_id,
            effectIndex = index,
            reason,
            reference = reference.as_str().unwrap_or_default(),
            "Custom tool inventory effect not applied",
        ),
        (None, Some(action)) => tracing::debug!(
            target: "quilltap::pascal",
            context = CONTEXT,
            chatId = chat_id,
            tool = tool_name,
            characterId = character_id,
            effectIndex = index,
            reason,
            action = action.as_str().unwrap_or_default(),
            itemsJson = %detail.get("items").cloned().unwrap_or_default(),
            "Custom tool inventory effect not applied",
        ),
        (None, None) => tracing::debug!(
            target: "quilltap::pascal",
            context = CONTEXT,
            chatId = chat_id,
            tool = tool_name,
            characterId = character_id,
            effectIndex = index,
            reason,
            "Custom tool inventory effect not applied",
        ),
    }
}

/// v4's `land` — one applied entry per object.
fn land(
    applied: &mut Vec<IndexedAppliedEffect>,
    index: usize,
    target: InventoryEffectTarget,
    previous: &[String],
    items: &[ResolvedInventoryItem],
    after: &[String],
    created: &HashSet<String>,
) {
    for item in items {
        let next = InventoryEffectRecord {
            item_id: item.id().to_string(),
            item_title: inventory_label(item),
            created: created.contains(item.id()),
            equipped: after.to_vec(),
        };
        applied.push(IndexedAppliedEffect {
            index,
            applied: AppliedEffect {
                target: target.as_str().to_string(),
                previous: Some(json!(previous)),
                next: serde_json::to_value(&next).unwrap_or(Value::Null),
                tier: None,
            },
        });
    }
}

/// Apply a run's inventory gestures to the rolling character — the IN-WRITE
/// half (v4 `applyInventoryEffects` up to its tail DEBUG). Never fails.
///
/// `mount` is `None` on a degraded open: the bag cannot be read, which lands
/// on v4's `could not be read` arm.
pub fn apply_inventory_effects(
    main: &Connection,
    mount: Option<&Connection>,
    params: ApplyInventoryEffectsParams<'_>,
) -> InventoryEffectsOutcome {
    let ApplyInventoryEffectsParams {
        user_id,
        chat_id,
        tool_name,
        character_id,
        gestures,
        inventory,
    } = params;
    if gestures.is_empty() {
        return InventoryEffectsOutcome::default();
    }

    let Some(character_id) = character_id.filter(|c| !c.is_empty()) else {
        tracing::debug!(
            target: "quilltap::pascal",
            context = CONTEXT,
            chatId = chat_id,
            tool = tool_name,
            gestures = gestures.len(),
            "Custom tool: no rolling character, inventory effects skipped",
        );
        return InventoryEffectsOutcome::default();
    };

    let unreadable = |error: String| {
        tracing::warn!(
            target: "quilltap::pascal",
            context = CONTEXT,
            chatId = chat_id,
            tool = tool_name,
            characterId = character_id,
            error,
            "Custom tool inventory effects skipped — the inventory could not be read",
        );
        InventoryEffectsOutcome::default()
    };

    // v4's one try: the context (given, or loaded), then the chat.
    let Some(mount) = mount else {
        return unreadable("the mount index is unavailable".to_string());
    };
    let mut bag = match inventory {
        Some(given) if is_loaded(inventory) => Bag::Given(given),
        _ => match load_inventory_context(main, mount, character_id, Some(chat_id)) {
            Ok(loaded) => Bag::Loaded(Box::new(loaded)),
            Err(e) => return unreadable(e.to_string()),
        },
    };
    let chat = match crate::db::chats_read::find_by_id(main, chat_id) {
        Ok(Some(row)) => EquipChat::from_chat_row(&row),
        Ok(None) => {
            tracing::warn!(
                target: "quilltap::pascal",
                context = CONTEXT,
                chatId = chat_id,
                tool = tool_name,
                "Custom tool inventory effects skipped — no such chat",
            );
            return InventoryEffectsOutcome::default();
        }
        Err(e) => return unreadable(crate::db::fallback::error_text(&e)),
    };

    // The notify (and its pending map) is the after-commit half's; the
    // chokepoints read only who and why.
    let change_ctx = InventoryChangeContext {
        user_id,
        provenance: InventoryChangeProvenance::Pascal,
        announce: true,
        pending: None,
    };
    let skip = |index: usize, reason: &str, detail: &Map<String, Value>| {
        skip(chat_id, tool_name, character_id, index, reason, detail)
    };

    let mut outcome = InventoryEffectsOutcome::default();
    for (index, gesture) in gestures {
        let index = *index;
        let before = sorted_ids(bag.equipped_ids().iter().cloned());
        let landed: Result<Option<InventoryChange>, String> = match gesture.action {
            InventoryEffectAction::PickUp => {
                let mut archetype_ids: Vec<String> = Vec::new();
                for reference in &gesture.items {
                    let archetype = resolve_inventory_ref(
                        reference,
                        bag.pool(),
                        bag.equipped_ids(),
                        ResolveInventoryRefOptions {
                            include_archetypes: true,
                            include_archived: false,
                        },
                    )
                    .into_iter()
                    .find(|m| m.item.is_archetype())
                    .map(|m| m.item);
                    match archetype {
                        Some(item) => archetype_ids.push(item.id().to_string()),
                        None => {
                            let mut detail = Map::new();
                            detail.insert("reference".into(), json!(reference));
                            skip(index, "no reachable archetype by that reference", &detail);
                        }
                    }
                }
                if archetype_ids.is_empty() {
                    continue;
                }
                let equip = gesture.equip != Some(false);
                match pick_up_items_holding_lock(
                    main,
                    mount,
                    &chat,
                    character_id,
                    &archetype_ids,
                    &change_ctx,
                    &PickUpOptions { title: None, equip },
                ) {
                    Ok((result, change)) => {
                        let created: HashSet<String> =
                            result.created.iter().map(|i| i.id().to_string()).collect();
                        let after = if equip {
                            sorted_ids(
                                before
                                    .iter()
                                    .cloned()
                                    .chain(result.created.iter().map(|i| i.id().to_string())),
                            )
                        } else {
                            before.clone()
                        };
                        land(
                            &mut outcome.applied,
                            index,
                            InventoryEffectTarget::PickUp,
                            &before,
                            &result.created,
                            &after,
                            &created,
                        );
                        Ok(change)
                    }
                    Err(e) => Err(e.to_string()),
                }
            }
            action @ (InventoryEffectAction::Equip | InventoryEffectAction::Unequip) => {
                let mut ids: Vec<String> = Vec::new();
                for reference in &gesture.items {
                    let matches = resolve_inventory_ref(
                        reference,
                        bag.pool(),
                        bag.equipped_ids(),
                        ResolveInventoryRefOptions::default(),
                    );
                    let pick = if action == InventoryEffectAction::Equip {
                        pick_for_gesture(&matches, Gesture::Equip)
                    } else {
                        pick_for_gesture(&matches, Gesture::Unequip).or_else(|| {
                            matches.iter().find(|m| m.item.kind() == InventoryKind::Kit)
                        })
                    };
                    match pick {
                        Some(m) => {
                            let id = m.item.id().to_string();
                            if !ids.contains(&id) {
                                ids.push(id);
                            }
                        }
                        None => {
                            let reason = if matches.is_empty() {
                                "no object by that reference".to_string()
                            } else {
                                format!("nothing to {}", action.as_str())
                            };
                            let mut detail = Map::new();
                            detail.insert("reference".into(), json!(reference));
                            skip(index, &reason, &detail);
                        }
                    }
                }
                if ids.is_empty() {
                    continue;
                }
                if action == InventoryEffectAction::Equip {
                    match equip_items_holding_lock(
                        main,
                        mount,
                        &chat,
                        character_id,
                        &ids,
                        &change_ctx,
                        &EquipOptions::default(),
                    ) {
                        Ok((result, change)) => {
                            let after = sorted_ids(
                                before
                                    .iter()
                                    .cloned()
                                    .chain(result.equipped.iter().map(|i| i.id().to_string())),
                            );
                            let created: HashSet<String> =
                                result.created.iter().map(|i| i.id().to_string()).collect();
                            land(
                                &mut outcome.applied,
                                index,
                                InventoryEffectTarget::Equip,
                                &before,
                                &result.equipped,
                                &after,
                                &created,
                            );
                            Ok(change)
                        }
                        Err(e) => Err(e.to_string()),
                    }
                } else {
                    match unequip_items_holding_lock(main, mount, &chat, character_id, &ids) {
                        Ok((result, change)) => {
                            let gone: HashSet<&str> =
                                result.unequipped.iter().map(|i| i.id()).collect();
                            let after: Vec<String> = before
                                .iter()
                                .filter(|id| !gone.contains(id.as_str()))
                                .cloned()
                                .collect();
                            land(
                                &mut outcome.applied,
                                index,
                                InventoryEffectTarget::Unequip,
                                &before,
                                &result.unequipped,
                                &after,
                                &HashSet::new(),
                            );
                            Ok(change)
                        }
                        Err(e) => Err(e.to_string()),
                    }
                }
            }
        };

        match landed {
            Ok(change) => outcome.changes.extend(change),
            Err(reason) => {
                // A refusal (the cap, an archetype named for equip) fails THIS
                // gesture before anything was written; the next one proceeds.
                let mut detail = Map::new();
                detail.insert("action".into(), json!(gesture.action.as_str()));
                detail.insert("items".into(), json!(gesture.items));
                skip(index, &reason, &detail);
                continue;
            }
        }

        // The next gesture resolves against what this one left.
        match load_inventory_context(main, mount, character_id, Some(chat_id)) {
            Ok(loaded) => bag = Bag::Loaded(Box::new(loaded)),
            Err(e) => {
                tracing::warn!(
                    target: "quilltap::pascal",
                    context = CONTEXT,
                    chatId = chat_id,
                    tool = tool_name,
                    error = %e,
                    "Custom tool inventory effects stopped — the inventory could not be re-read",
                );
                break;
            }
        }
    }

    outcome.finished = true;
    outcome
}

/// The AFTER-COMMIT half (v4 `applyInventoryEffects`' notifies + tail): ONE
/// `notify_inventory_changed` per landed gesture, in order — with the turn's
/// pending map each nets into the turn's entry (v4 `joined the turn`), without
/// one each posts its own line (`posted now`) — then the `applied` DEBUG. Call
/// only when the in-write half [`finished`](InventoryEffectsOutcome::finished).
#[allow(clippy::too_many_arguments)]
pub async fn finish_inventory_effects(
    db: &Db,
    user_id: &str,
    chat_id: &str,
    tool_name: &str,
    character_id: &str,
    changes: &[InventoryChange],
    applied_targets: &[&str],
    pending: Option<&PendingInventoryAnnouncements>,
) {
    for change in changes {
        // v4's notify never throws; with the turn's map it nets the change
        // into the turn's entry, without one it posts now.
        let ctx = InventoryChangeContext {
            user_id,
            provenance: InventoryChangeProvenance::Pascal,
            announce: true,
            pending: None,
        };
        notify_inventory_changed(db, chat_id, character_id, change, &ctx, pending).await;
    }
    tracing::debug!(
        target: "quilltap::pascal",
        context = CONTEXT,
        chatId = chat_id,
        tool = tool_name,
        characterId = character_id,
        appliedJson = %serde_json::to_string(applied_targets).unwrap_or_default(),
        announcement = if pending.is_some() { "joined the turn" } else { "posted now" },
        "Custom tool inventory effects applied",
    );
}

#[cfg(test)]
mod tests {
    //! The applier's capture pins (R-12), each with its silence leg, over a
    //! real provisioned `Db` (the pass runs on the caller thread here — the
    //! seam's writer-thread lines are `pascal_inventory_effects_equivalence`'s).
    //! The bag loader is the real `load_inventory_context` (v4's
    //! `loadInventoryContext`), whose reads are v4's fallbacks: on a fresh
    //! instance it loads an empty context, and the missing chat is v4's
    //! `no such chat` arm. The `could not be read` arm is a degraded mount
    //! index (the read itself fails).
    use super::*;
    use crate::pascal::custom_tools::ResolvedInventoryGesture;
    use crate::test_support::global_capture;

    const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
    const CHAR: &str = "a1000000-0000-4000-8000-00000000000a";

    fn db() -> (Db, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("data");
        std::fs::create_dir_all(&data).unwrap();
        crate::services::provisioning::provision_fresh_instance(&data, PEPPER).unwrap();
        let db = Db::open(
            crate::db::runtime::DbPaths {
                main: data.join("quilltap.db"),
                mount_index: Some(data.join("quilltap-mount-index.db")),
                llm_logs: None,
            },
            PEPPER,
        )
        .unwrap();
        (db, dir)
    }

    fn equip(items: &[&str]) -> ResolvedInventoryGesture {
        ResolvedInventoryGesture {
            action: InventoryEffectAction::Equip,
            items: items.iter().map(|s| s.to_string()).collect(),
            equip: None,
        }
    }

    /// Run the in-write half against the instance's own connections.
    fn apply(
        db: &Db,
        with_mount: bool,
        character_id: Option<&str>,
        gestures: &[(usize, &ResolvedInventoryGesture)],
    ) -> (InventoryEffectsOutcome, Vec<String>) {
        global_capture::install();
        let params = ApplyInventoryEffectsParams {
            user_id: "u1",
            chat_id: "chat-1",
            tool_name: "satchel",
            character_id,
            gestures,
            inventory: None,
        };
        let (outcome, lines) = global_capture::capture(|| {
            db.read_main(|main| {
                if with_mount {
                    db.read_mount_index(|mount| {
                        Ok(apply_inventory_effects(main, Some(mount), params))
                    })
                } else {
                    Ok(apply_inventory_effects(main, None, params))
                }
            })
            .unwrap()
        });
        let lines = lines
            .into_iter()
            .filter(|l| l.contains(" quilltap::pascal "))
            .collect();
        (outcome, lines)
    }

    #[test]
    fn no_gestures_is_silent() {
        let (db, _dir) = db();
        let (out, lines) = apply(&db, true, Some(CHAR), &[]);
        assert!(out.applied.is_empty() && out.changes.is_empty() && !out.finished);
        assert!(lines.is_empty(), "no gestures logs nothing: {lines:?}");
    }

    #[test]
    fn nobody_rolled_logs_the_skip() {
        let (db, _dir) = db();
        let g = equip(&["coin"]);
        let (out, lines) = apply(&db, true, None, &[(0, &g)]);
        assert!(out.applied.is_empty() && !out.finished);
        assert_eq!(
            lines,
            vec![
                "DEBUG quilltap::pascal Custom tool: no rolling character, inventory effects skipped context=pascal.inventory-effects chatId=chat-1 tool=satchel gestures=1"
            ]
        );
        // An empty id is v4's falsy `!characterId` too.
        let (_, lines) = apply(&db, true, Some(""), &[(0, &g)]);
        assert_eq!(lines.len(), 1);
        assert!(lines[0].contains("no rolling character"));
    }

    #[test]
    fn a_missing_chat_warns_no_such_chat() {
        // v4: the context loads (its reads are fallbacks), then
        // `repos.chats.findById` answers null — `skipped — no such chat`.
        let (db, _dir) = db();
        let g = equip(&["coin"]);
        let (out, lines) = apply(&db, true, Some(CHAR), &[(0, &g)]);
        assert!(out.applied.is_empty() && out.changes.is_empty() && !out.finished);
        assert_eq!(
            lines,
            vec![
                "WARN quilltap::pascal Custom tool inventory effects skipped — no such chat context=pascal.inventory-effects chatId=chat-1 tool=satchel"
                    .to_string()
            ]
        );
    }

    #[test]
    fn a_degraded_mount_index_is_the_unreadable_arm() {
        let (db, _dir) = db();
        let g = equip(&["coin"]);
        let (out, lines) = apply(&db, false, Some(CHAR), &[(0, &g)]);
        assert!(out.applied.is_empty() && !out.finished);
        assert_eq!(
            lines,
            vec![format!(
                "WARN quilltap::pascal Custom tool inventory effects skipped — the inventory could not be read context=pascal.inventory-effects chatId=chat-1 tool=satchel characterId={CHAR} error=the mount index is unavailable"
            )]
        );
    }
}
