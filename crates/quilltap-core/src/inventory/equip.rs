//! Equip and unequip — the chokepoints a gesture in the Salon (and a tool, and
//! Pascal's effects) calls to change what a character carries in one chat (v4
//! `lib/inventory/equip.ts`, `4fbdad186` at `322a5488e`; P4.D324).
//!
//! Plan, then write (spec §5.6):
//!
//!   1. Load the character's pool and the current equipped set (ids that no
//!      longer resolve are dropped here, and so pruned by this write).
//!   2. Expand kits: an archetype member becomes an instance in the
//!      character's vault (an unequipped instance they already own is reused
//!      rather than a fresh one minted), an item member is itself, and a
//!      member that does not resolve is reported in `skipped`.
//!   3. Refuse archetypes and archived objects named directly
//!      (`NOT_EQUIPPABLE`), and check the cap: `count(equipped kin) + planned
//!      kin + 1 ≤ maxEquipped`, or the whole call fails (`CAP_EXCEEDED`) with
//!      nothing written — instantiations included.
//!   4. Write the instances, then one `set_equipped_inventory`.
//!
//! Unequip never fails on state: ids not carried are reported. A kit
//! unequips every member present (and the instances of its archetype
//! members) and keeps the instances.
//!
//! [`pick_up_items`] is the third gesture (a character's `inventory_pick_up`
//! and a Pascal `pick_up` effect): an instance of each named archetype is made
//! in the character's vault root and, by default, equipped — the cap checked
//! over the whole plan *before* any instance is written, so a breach creates
//! nothing.
//!
//! ## The change is RETURNED, never notified (R-1)
//!
//! v4 ends each gesture with `notifyInventoryChanged` (Pascal's line, the
//! avatar refresh, or the turn's pending set). Here each gesture RETURNS its
//! [`InventoryChange`] to the caller; `services::inventory_change_effects`
//! (P4.D328) notifies, records or flushes it. The four change types are data
//! only and live here so this module's signatures compile.
//!
//! ## The lock (R-2)
//!
//! v4 serialises every write of one chat's `equippedInventory` through
//! `withChatInventoryLock` (`runSerialized('inventory-equip:<chatId>')`), since
//! `setEquippedInventory` is a read-modify-write of the whole map, and its
//! chain is not re-entrant — inside it a caller equips through
//! `equipItemsHoldingLock`. In v5 the single writer task IS the lock: every
//! `Db::write` closure runs alone, so [`with_chat_inventory_lock`] is ONE
//! `db.write` and the `*_holding_lock` forms are the synchronous bodies a
//! caller already inside a write closure (chat create, a seat add, a merge,
//! the tools' turn) calls directly. The re-entrancy hazard has no v5 twin
//! (a closure cannot `await` another write).

use std::collections::HashSet;

use rusqlite::Connection;
use serde::Serialize;
use serde_json::Value;

use super::equipped_set::read_equipped_set;
use super::errors::{InventoryError, InventoryRefusal, NotEquippableReason};
use super::item_mutations::{instantiate, InstantiateOptions};
use super::location::{resolve_inventory_location, ResolveInventoryLocationOptions};
use super::pool::{load_inventory_pool, InventoryPool, LoadInventoryPoolOptions};
use super::resolve::is_kin;
use super::roster_gate::{is_character_gesture, is_on_closed_shelf, project_shelf_open_to};
use super::types::{InventoryFields, InventoryKind, InventoryScope, ResolvedInventoryItem};
use crate::db::chats_inventory::ChatsInventoryRepository;
use crate::db::runtime::Db;
use crate::db::DbError;
use crate::realtime::bus::publish_realtime;
use crate::realtime::types::RealtimeTopic;

// ============================================================================
// The four pure change types (v4 `equip-effects.ts:38-56` +
// `turn-containers.ts:18-30`) — data only.
// ============================================================================

/// Who is changing what a character carries: the operator, the character's
/// own tool call, or a custom tool (v4 `InventoryChangeProvenance`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum InventoryChangeProvenance {
    Operator,
    Character,
    Pascal,
}

impl InventoryChangeProvenance {
    pub fn as_str(self) -> &'static str {
        match self {
            InventoryChangeProvenance::Operator => "operator",
            InventoryChangeProvenance::Character => "character",
            InventoryChangeProvenance::Pascal => "pascal",
        }
    }
}

/// One character's Inventory changes waiting for the turn's single Pascal
/// line: everything taken up, set down and picked up, netted (v4
/// `PendingInventoryChange`).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingInventoryChange {
    pub user_id: String,
    pub chat_id: String,
    pub character_id: String,
    pub equipped: Vec<ResolvedInventoryItem>,
    pub unequipped: Vec<ResolvedInventoryItem>,
    pub picked_up: Vec<ResolvedInventoryItem>,
    /// Who made the changes folded into this entry, in the order they first did.
    pub provenances: Vec<InventoryChangeProvenance>,
}

/// The turn's pending Inventory changes, keyed `chatId:characterId` — an
/// INSERTION-ordered map, as v4's `Map` is (P4.169 I-4): the flush posts in
/// first-record order. Entries are only inserted or edited in place mid-turn
/// (v4 never deletes one before the drain), so no removal can reorder it.
pub type PendingInventoryMap = indexmap::IndexMap<String, PendingInventoryChange>;

/// Who is changing what a character carries, and the turn's pending set if
/// there is one (v4 `InventoryChangeContext`; `pending` keyed
/// `chatId:characterId`).
#[derive(Debug)]
pub struct InventoryChangeContext<'a> {
    pub user_id: &'a str,
    pub provenance: InventoryChangeProvenance,
    /// Post Pascal's line for this gesture. False at arrival.
    pub announce: bool,
    pub pending: Option<&'a mut PendingInventoryMap>,
}

/// A give: the object handed over and the receiver's name.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InventoryGive {
    pub item: ResolvedInventoryItem,
    pub to_name: String,
}

/// One change: what was taken up and what was set down (v4
/// `InventoryChange`).
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InventoryChange {
    pub equipped: Vec<ResolvedInventoryItem>,
    pub unequipped: Vec<ResolvedInventoryItem>,
    /// Instances made from an archetype in this gesture (named "picks up").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub picked_up: Option<Vec<ResolvedInventoryItem>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub given: Option<InventoryGive>,
}

impl InventoryChange {
    /// v4 `notifyInventoryChanged`'s early return: nothing taken up, set
    /// down, picked up or given.
    pub fn is_empty(&self) -> bool {
        self.equipped.is_empty()
            && self.unequipped.is_empty()
            && self.picked_up.as_ref().is_none_or(Vec::is_empty)
            && self.given.is_none()
    }
}

// ============================================================================
// The chat and the results
// ============================================================================

/// One seat as equip reads it.
#[derive(Debug, Clone, Default)]
pub struct EquipParticipant {
    pub kind: Option<String>,
    pub character_id: Option<String>,
    pub status: Option<String>,
}

/// The chat as equip needs it (v4 `EquipChat`).
#[derive(Debug, Clone, Default)]
pub struct EquipChat {
    pub id: String,
    pub project_id: Option<String>,
    pub equipped_inventory: Option<Value>,
    pub participants: Vec<EquipParticipant>,
}

impl EquipChat {
    /// From a chat row (`chats_read`'s marshalled object).
    pub fn from_chat_row(row: &Value) -> Self {
        let s = |v: &Value, k: &str| v.get(k).and_then(Value::as_str).map(str::to_string);
        EquipChat {
            id: s(row, "id").unwrap_or_default(),
            project_id: s(row, "projectId"),
            equipped_inventory: row.get("equippedInventory").cloned(),
            participants: row
                .get("participants")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .map(|p| EquipParticipant {
                    kind: s(p, "type"),
                    character_id: s(p, "characterId"),
                    status: s(p, "status"),
                })
                .collect(),
        }
    }
}

/// Why a kit member was not carried (v4 `SkipReason`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SkipReason {
    Missing,
    Archived,
    IsKit,
}

/// A kit member that could not be carried, and why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkippedMember {
    pub id: String,
    pub kit_id: String,
    pub reason: SkipReason,
}

/// v4 `EquipResult`.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EquipResult {
    /// Everything newly carried (instances created for kits included) —
    /// created first, then the existing objects.
    pub equipped: Vec<ResolvedInventoryItem>,
    /// Instances minted for a kit's archetype members.
    pub created: Vec<ResolvedInventoryItem>,
    /// Kit members that could not be carried, and why.
    pub skipped: Vec<SkippedMember>,
    /// Ids asked for that were already carried.
    pub already_equipped: Vec<String>,
}

/// v4 `UnequipResult`.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnequipResult {
    pub unequipped: Vec<ResolvedInventoryItem>,
    /// Ids asked for that were not carried.
    pub not_equipped: Vec<String>,
}

/// v4 `PickUpResult`.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PickUpResult {
    /// The instances made, in archetype order.
    pub created: Vec<ResolvedInventoryItem>,
    /// Whether they were equipped.
    pub equipped: bool,
}

/// v4 `EquipOptions`.
#[derive(Debug, Clone, Default)]
pub struct EquipOptions {
    /// Arrival: an object that would breach its cap is dropped with a `warn`
    /// instead of failing the call, and a direct id that is not carriable is
    /// skipped.
    pub lenient: bool,
    /// Objects written by this same call that the pool may not see yet,
    /// looked up after the pool.
    pub known: Vec<ResolvedInventoryItem>,
}

/// v4 `pickUpItems`'s opts: a `title` override, and `equip` (default true).
#[derive(Debug, Clone)]
pub struct PickUpOptions {
    pub title: Option<String>,
    pub equip: bool,
}

impl Default for PickUpOptions {
    fn default() -> Self {
        PickUpOptions {
            title: None,
            equip: true,
        }
    }
}

/// What a holding-lock gesture answers: its result and the change to notify
/// (`None` when nothing was written and v4 would not have notified).
pub type GestureOutcome<R> = (R, Option<InventoryChange>);

fn ids_json(items: &[ResolvedInventoryItem]) -> String {
    let ids: Vec<&str> = items.iter().map(|i| i.id()).collect();
    serde_json::to_string(&ids).unwrap_or_default()
}

fn json_text(v: &impl Serialize) -> String {
    serde_json::to_string(v).unwrap_or_default()
}

// ============================================================================
// The guards
// ============================================================================

/// The character is seated in the chat (any seat type, not `removed`); else
/// INFO `[InventoryEquip] Refused` + `NOT_A_PARTICIPANT` (v4
/// `assertParticipant`, R-18 — a USER-type seat carrying a `characterId`
/// counts).
pub fn assert_participant(chat: &EquipChat, character_id: &str) -> Result<(), InventoryError> {
    let seated = chat.participants.iter().any(|p| {
        p.character_id.as_deref() == Some(character_id) && p.status.as_deref() != Some("removed")
    });
    if seated {
        return Ok(());
    }
    tracing::info!(
        code = "NOT_A_PARTICIPANT",
        chatId = %chat.id,
        characterId = %character_id,
        context = "inventory",
        "[InventoryEquip] Refused"
    );
    Err(InventoryRefusal::not_a_participant(character_id).into())
}

/// Refuse a character's own gesture (never the operator's) that takes from
/// the project's shelf while the roster does not admit them (v4
/// `assertShelfReach`). `items` are the objects or archetypes about to be
/// carried, none of them already carried.
pub fn assert_shelf_reach(
    main: &Connection,
    mount: &Connection,
    chat: &EquipChat,
    character_id: &str,
    items: &[&ResolvedInventoryItem],
    ctx: &InventoryChangeContext<'_>,
) -> Result<(), InventoryError> {
    if !is_character_gesture(ctx.provenance) {
        return Ok(());
    }
    let nothing_carried = HashSet::new();
    let from_shelf: Vec<&&ResolvedInventoryItem> = items
        .iter()
        .filter(|item| is_on_closed_shelf(item, &nothing_carried))
        .collect();
    if from_shelf.is_empty()
        || project_shelf_open_to(main, mount, chat.project_id.as_deref(), character_id)
    {
        return Ok(());
    }
    let names: Vec<String> = from_shelf
        .iter()
        .map(|i| i.title_or_name().to_string())
        .collect();
    let error = InventoryRefusal::off_roster(&names);
    let item_ids: Vec<&str> = from_shelf.iter().map(|i| i.id()).collect();
    tracing::info!(
        code = error.code.as_str(),
        chatId = %chat.id,
        characterId = %character_id,
        projectId = chat.project_id.as_deref().unwrap_or("null"),
        itemIdsJson = %json_text(&item_ids),
        provenance = ctx.provenance.as_str(),
        context = "inventory",
        "[InventoryEquip] Refused"
    );
    Err(error.into())
}

/// Re-read the chat so a serialised write sees the set as it is now (v4
/// `freshChat` — the stale one when the read finds nothing).
fn fresh_chat(main: &Connection, chat: &EquipChat) -> EquipChat {
    crate::db::chats_read::find_by_id_or_none(main, &chat.id)
        .map(|row| EquipChat::from_chat_row(&row))
        .unwrap_or_else(|| chat.clone())
}

fn pool_for(
    main: &Connection,
    mount: &Connection,
    chat: &EquipChat,
    character_id: &str,
) -> Result<InventoryPool, DbError> {
    load_inventory_pool(
        main,
        mount,
        LoadInventoryPoolOptions {
            character_id,
            chat_id: Some(&chat.id),
            project_id: chat.project_id.as_deref(),
            own_records: None,
        },
    )
}

fn character_vault(
    main: &Connection,
    mount: &Connection,
    character_id: &str,
    user_id: &str,
    missing: &str,
) -> Result<super::location::InventoryLocation, InventoryError> {
    resolve_inventory_location(
        main,
        mount,
        InventoryScope::Character,
        Some(character_id),
        user_id,
        ResolveInventoryLocationOptions::default(),
    )?
    .ok_or_else(|| {
        InventoryError::Other(format!("Character {character_id} has no vault {missing}"))
    })
}

// ============================================================================
// The lock
// ============================================================================

/// Run `f` holding a chat's carried-set lock (v4 `withChatInventoryLock`) —
/// ONE `Db::write` closure over the main and mount-index writers. A degraded
/// instance with no mount-index writer answers the `PartitionUnavailable`
/// storage error (P4.159's class).
pub async fn with_chat_inventory_lock<T, F>(
    db: &Db,
    _chat_id: &str,
    f: F,
) -> Result<T, InventoryError>
where
    F: FnOnce(&Connection, &Connection) -> Result<T, InventoryError> + Send + 'static,
    T: Send + 'static,
{
    db.write(move |ws| {
        let main = ws.main().connection();
        let mount = ws
            .mount_index()
            .ok_or(DbError::PartitionUnavailable(
                crate::write_partition::WriteDbTarget::MountIndex,
            ))?
            .connection();
        Ok(f(main, mount))
    })
    .await?
}

/// Drop one object from one character's carried set in EVERY chat that lists
/// it — a give (the id is kept on the move, so a later give-back must not
/// reappear as carried) or an archive (a later restore must not quietly
/// re-equip it) (v4 `dropCarriedEverywhere`). `also_chat_ids` names chats to
/// check whatever the read finds (first-seen order, found first). Answers how
/// many chats changed. DEBUG `[InventoryEquip] Dropped an object from every
/// carried set`.
pub fn drop_carried_everywhere(
    main: &Connection,
    character_id: &str,
    item_id: &str,
    also_chat_ids: &[String],
) -> usize {
    let repo = ChatsInventoryRepository::new(main);
    let mut chat_ids: Vec<String> = Vec::new();
    for id in repo
        .find_chat_ids_carrying_inventory_id(character_id, item_id)
        .into_iter()
        .chain(also_chat_ids.iter().cloned())
    {
        if !chat_ids.contains(&id) {
            chat_ids.push(id);
        }
    }
    let changed = chat_ids
        .iter()
        .filter(|chat_id| repo.remove_equipped_inventory_id(chat_id, character_id, item_id))
        .count();
    tracing::debug!(
        characterId = %character_id,
        itemId = %item_id,
        checked = chat_ids.len(),
        changed,
        context = "inventory",
        "[InventoryEquip] Dropped an object from every carried set"
    );
    changed
}

// ============================================================================
// Equip
// ============================================================================

/// One planned object: an existing one, or an archetype to instantiate.
enum Planned {
    Existing(ResolvedInventoryItem),
    Instantiate {
        archetype: ResolvedInventoryItem,
        kit_folder: Option<String>,
    },
}

impl Planned {
    fn subject(&self) -> &ResolvedInventoryItem {
        match self {
            Planned::Existing(item) => item,
            Planned::Instantiate { archetype, .. } => archetype,
        }
    }
}

struct Plan {
    current_ids: HashSet<String>,
    planned: Vec<Planned>,
    planned_ids: HashSet<String>,
    result: EquipResult,
}

impl Plan {
    fn plan_existing(&mut self, item: ResolvedInventoryItem) {
        if self.current_ids.contains(item.id()) {
            self.result.already_equipped.push(item.id().to_string());
            return;
        }
        if !self.planned_ids.insert(item.id().to_string()) {
            return;
        }
        self.planned.push(Planned::Existing(item));
    }
}

/// [`equip_items`] for a caller already holding the lock (inside a write
/// closure) — v4 `equipItemsHoldingLock`: `assert_participant`, then the plan
/// and the writes. Returns the change (`None` when nothing new was carried —
/// no write, no publish).
pub fn equip_items_holding_lock(
    main: &Connection,
    mount: &Connection,
    chat: &EquipChat,
    character_id: &str,
    ids: &[String],
    ctx: &InventoryChangeContext<'_>,
    opts: &EquipOptions,
) -> Result<GestureOutcome<EquipResult>, InventoryError> {
    assert_participant(chat, character_id)?;
    equip_unserialized(main, mount, chat, character_id, ids, ctx, opts)
}

fn equip_unserialized(
    main: &Connection,
    mount: &Connection,
    stale_chat: &EquipChat,
    character_id: &str,
    ids: &[String],
    ctx: &InventoryChangeContext<'_>,
    opts: &EquipOptions,
) -> Result<GestureOutcome<EquipResult>, InventoryError> {
    let chat = fresh_chat(main, stale_chat);
    let pool = pool_for(main, mount, &chat, character_id)?;
    let current = read_equipped_set(
        Some(&chat.id),
        chat.equipped_inventory.as_ref(),
        character_id,
        &pool,
    );
    let carried_parents: HashSet<String> = current
        .iter()
        .filter_map(|item| item.parent_id().map(str::to_string))
        .collect();
    let mut plan = Plan {
        current_ids: current.iter().map(|i| i.id().to_string()).collect(),
        planned: Vec::new(),
        planned_ids: HashSet::new(),
        result: EquipResult::default(),
    };
    let mut planned_archetypes: HashSet<String> = HashSet::new();

    let mut unique_ids: Vec<&String> = Vec::new();
    for id in ids {
        if !unique_ids.contains(&id) {
            unique_ids.push(id);
        }
    }
    for id in unique_ids {
        let item = pool
            .resolve_id(id)
            .or_else(|| opts.known.iter().find(|k| k.id() == id.as_str()).cloned());
        let refusal_reason = match &item {
            None => Some(NotEquippableReason::Missing),
            Some(i) if i.is_archived() => Some(NotEquippableReason::Archived),
            Some(i) if i.is_archetype() => Some(NotEquippableReason::Archetype),
            Some(_) => None,
        };
        if let Some(reason) = refusal_reason {
            if opts.lenient {
                tracing::debug!(
                    chatId = %chat.id,
                    characterId = %character_id,
                    itemId = %id,
                    reason = reason.as_str(),
                    context = "inventory",
                    "[InventoryEquip] Arrival skipped an object that cannot be carried"
                );
                continue;
            }
            tracing::info!(
                code = "NOT_EQUIPPABLE",
                chatId = %chat.id,
                characterId = %character_id,
                itemId = %id,
                reason = reason.as_str(),
                context = "inventory",
                "[InventoryEquip] Refused"
            );
            return Err(InventoryRefusal::not_equippable(id, reason).into());
        }
        let item = item.expect("checked above");
        if item.kind() != InventoryKind::Kit {
            plan.plan_existing(item);
            continue;
        }
        // A kit: expand its members.
        let kit_id = item.id().to_string();
        for member_id in item.item_ids() {
            let skip = |reason| SkippedMember {
                id: member_id.clone(),
                kit_id: kit_id.clone(),
                reason,
            };
            match pool.resolve_id(&member_id) {
                None => plan.result.skipped.push(skip(SkipReason::Missing)),
                Some(member) if member.is_archived() => {
                    plan.result.skipped.push(skip(SkipReason::Archived))
                }
                Some(member) if member.kind() == InventoryKind::Kit => {
                    plan.result.skipped.push(skip(SkipReason::IsKit))
                }
                Some(member) if member.is_archetype() => {
                    if carried_parents.contains(member.id()) {
                        plan.result.already_equipped.push(member.id().to_string());
                    } else if planned_archetypes.insert(member.id().to_string()) {
                        // Own, unarchived, uncarried, unplanned instances of
                        // the archetype, for reuse.
                        let spare = pool
                            .own
                            .iter()
                            .find(|r| {
                                r.str_of("parentId") == Some(member.id())
                                    && !r.is_archived()
                                    && !plan.current_ids.contains(r.id())
                                    && !plan.planned_ids.contains(r.id())
                            })
                            .map(|r| pool.resolve(r));
                        match spare {
                            Some(spare) => plan.plan_existing(spare),
                            None => plan.planned.push(Planned::Instantiate {
                                archetype: member,
                                kit_folder: pool.owns(&kit_id).then(|| item.folder().to_string()),
                            }),
                        }
                    }
                }
                Some(member) => plan.plan_existing(member),
            }
        }
    }

    // A character's own gesture cannot take from the project's shelf when the
    // roster does not admit them; checked before the cap so nothing is
    // written. What they already carry was never planned.
    let subjects: Vec<&ResolvedInventoryItem> = plan.planned.iter().map(Planned::subject).collect();
    assert_shelf_reach(main, mount, &chat, character_id, &subjects, ctx)?;

    // The cap, checked over the whole plan before anything is written. An
    // instance-to-be has its archetype's name and cap.
    let mut counted: Vec<ResolvedInventoryItem> = current.clone();
    let mut accepted: Vec<Planned> = Vec::new();
    for planned in plan.planned {
        let subject = planned.subject();
        let kin = counted
            .iter()
            .filter(|other| is_kin(*other, subject))
            .count() as i64;
        if kin + 1 > subject.max_equipped() {
            if opts.lenient {
                tracing::warn!(
                    chatId = %chat.id,
                    characterId = %character_id,
                    itemId = %subject.id(),
                    cap = subject.max_equipped(),
                    context = "inventory",
                    "[InventoryEquip] Arrival dropped an object over its cap"
                );
                continue;
            }
            let error = InventoryRefusal::cap_exceeded(
                subject.id(),
                subject.title_or_name(),
                subject.max_equipped(),
            );
            tracing::info!(
                code = error.code.as_str(),
                chatId = %chat.id,
                characterId = %character_id,
                itemId = %subject.id(),
                cap = subject.max_equipped(),
                context = "inventory",
                "[InventoryEquip] Refused"
            );
            return Err(error.into());
        }
        counted.push(subject.clone());
        accepted.push(planned);
    }
    let mut result = plan.result;

    if accepted.is_empty() {
        tracing::debug!(
            chatId = %chat.id,
            characterId = %character_id,
            alreadyEquippedJson = %json_text(&result.already_equipped),
            skippedJson = %json_text(&result.skipped),
            context = "inventory",
            "[InventoryEquip] Nothing new to carry"
        );
        return Ok((result, None));
    }

    // Writes: instances first, then the one set write.
    if accepted
        .iter()
        .any(|p| matches!(p, Planned::Instantiate { .. }))
    {
        let vault = character_vault(
            main,
            mount,
            character_id,
            ctx.user_id,
            "to hold a kit's instances",
        )?;
        for planned in &accepted {
            let Planned::Instantiate {
                archetype,
                kit_folder,
            } = planned
            else {
                continue;
            };
            let made = instantiate(
                main,
                mount,
                &vault,
                archetype.id(),
                InstantiateOptions {
                    folder: kit_folder.as_deref(),
                    overrides: None,
                    pool: Some(&pool),
                },
            )?;
            result.created.push(made.item.clone());
            result.equipped.push(made.item);
        }
    }
    for planned in accepted {
        if let Planned::Existing(item) = planned {
            result.equipped.push(item);
        }
    }

    let next: Vec<String> = current
        .iter()
        .chain(result.equipped.iter())
        .map(|i| i.id().to_string())
        .collect();
    ChatsInventoryRepository::new(main).set_equipped_inventory(&chat.id, character_id, Some(&next));
    publish_realtime(RealtimeTopic::Chats, Some(&chat.id));
    tracing::debug!(
        op = "equip",
        chatId = %chat.id,
        characterId = %character_id,
        equippedJson = %ids_json(&result.equipped),
        createdJson = %ids_json(&result.created),
        skippedJson = %json_text(&result.skipped),
        alreadyEquippedJson = %json_text(&result.already_equipped),
        context = "inventory",
        "[InventoryEquip] Equipped"
    );
    let change = InventoryChange {
        equipped: result.equipped.clone(),
        unequipped: Vec::new(),
        picked_up: None,
        given: None,
    };
    Ok((result, Some(change)))
}

/// Equip objects (and kits) for one character in one chat (v4 `equipItems`):
/// `assert_participant` outside the lock, then the plan and the writes under
/// it. See the module header for the plan-then-write contract.
pub async fn equip_items(
    db: &Db,
    chat: &EquipChat,
    character_id: &str,
    ids: &[String],
    ctx: &InventoryChangeContext<'_>,
    opts: EquipOptions,
) -> Result<GestureOutcome<EquipResult>, InventoryError> {
    assert_participant(chat, character_id)?;
    let (chat, character_id, ids) = (chat.clone(), character_id.to_string(), ids.to_vec());
    let (user_id, provenance, announce) = (ctx.user_id.to_string(), ctx.provenance, ctx.announce);
    let chat_id = chat.id.clone();
    with_chat_inventory_lock(db, &chat_id, move |main, mount| {
        let ctx = InventoryChangeContext {
            user_id: &user_id,
            provenance,
            announce,
            pending: None,
        };
        equip_unserialized(main, mount, &chat, &character_id, &ids, &ctx, &opts)
    })
    .await
}

// ============================================================================
// Unequip
// ============================================================================

/// [`unequip_items`] for a caller already holding the lock. Never fails on
/// state; always answers a change (v4 notifies unconditionally — an empty
/// change is the caller's early return).
pub fn unequip_items_holding_lock(
    main: &Connection,
    mount: &Connection,
    chat: &EquipChat,
    character_id: &str,
    ids: &[String],
) -> Result<GestureOutcome<UnequipResult>, InventoryError> {
    // v4 `unequipItems` asserts the seat before any read; a lock-held caller
    // (a Pascal effect) must get the same refusal (§3 review, the union).
    assert_participant(chat, character_id)?;
    let fresh = fresh_chat(main, chat);
    let pool = pool_for(main, mount, &fresh, character_id)?;
    let current = read_equipped_set(
        Some(&fresh.id),
        fresh.equipped_inventory.as_ref(),
        character_id,
        &pool,
    );
    let mut removing: HashSet<String> = HashSet::new();
    let mut not_equipped: Vec<String> = Vec::new();

    let mut unique_ids: Vec<&String> = Vec::new();
    for id in ids {
        if !unique_ids.contains(&id) {
            unique_ids.push(id);
        }
    }
    for id in unique_ids {
        if let Some(kit) = pool
            .resolve_id(id)
            .filter(|i| i.kind() == InventoryKind::Kit)
        {
            for member_id in kit.item_ids() {
                for carried in &current {
                    if carried.id() == member_id || carried.parent_id() == Some(member_id.as_str())
                    {
                        removing.insert(carried.id().to_string());
                    }
                }
            }
            continue;
        }
        if current.iter().any(|c| c.id() == id.as_str()) {
            removing.insert(id.clone());
        } else {
            not_equipped.push(id.clone());
        }
    }

    let unequipped: Vec<ResolvedInventoryItem> = current
        .iter()
        .filter(|i| removing.contains(i.id()))
        .cloned()
        .collect();
    if !unequipped.is_empty() {
        let kept: Vec<String> = current
            .iter()
            .filter(|i| !removing.contains(i.id()))
            .map(|i| i.id().to_string())
            .collect();
        ChatsInventoryRepository::new(main).set_equipped_inventory(
            &fresh.id,
            character_id,
            Some(&kept),
        );
        publish_realtime(RealtimeTopic::Chats, Some(&fresh.id));
    }
    tracing::debug!(
        op = "unequip",
        chatId = %fresh.id,
        characterId = %character_id,
        unequippedJson = %ids_json(&unequipped),
        notEquippedJson = %json_text(&not_equipped),
        context = "inventory",
        "[InventoryEquip] Unequipped"
    );
    let change = InventoryChange {
        equipped: Vec::new(),
        unequipped: unequipped.clone(),
        picked_up: None,
        given: None,
    };
    Ok((
        UnequipResult {
            unequipped,
            not_equipped,
        },
        Some(change),
    ))
}

/// Unequip objects (and kits) for one character in one chat (v4
/// `unequipItems`). Never fails on state.
pub async fn unequip_items(
    db: &Db,
    chat: &EquipChat,
    character_id: &str,
    ids: &[String],
) -> Result<GestureOutcome<UnequipResult>, InventoryError> {
    assert_participant(chat, character_id)?;
    let (chat, character_id, ids) = (chat.clone(), character_id.to_string(), ids.to_vec());
    let chat_id = chat.id.clone();
    with_chat_inventory_lock(db, &chat_id, move |main, mount| {
        unequip_items_holding_lock(main, mount, &chat, &character_id, &ids)
    })
    .await
}

// ============================================================================
// Pick up
// ============================================================================

/// [`pick_up_items`] for a caller already holding the lock. The three
/// `INVALID_PARENT` arms, the shelf, the cap over the archetypes (when
/// equipping), then the instances at the vault ROOT, the `inventory` publish,
/// and (equipping something) the set write + the `chats` publish.
pub fn pick_up_items_holding_lock(
    main: &Connection,
    mount: &Connection,
    chat: &EquipChat,
    character_id: &str,
    archetype_ids: &[String],
    ctx: &InventoryChangeContext<'_>,
    opts: &PickUpOptions,
) -> Result<GestureOutcome<PickUpResult>, InventoryError> {
    // v4 `pickUpItems` asserts the seat first (as above).
    assert_participant(chat, character_id)?;
    let equip = opts.equip;
    let fresh = fresh_chat(main, chat);
    let pool = pool_for(main, mount, &fresh, character_id)?;
    let current = read_equipped_set(
        Some(&fresh.id),
        fresh.equipped_inventory.as_ref(),
        character_id,
        &pool,
    );

    let mut archetypes: Vec<ResolvedInventoryItem> = Vec::new();
    for id in archetype_ids {
        let archetype = pool.resolve_id(id);
        match archetype {
            Some(a) if a.is_archetype() && !a.is_archived() => archetypes.push(a),
            other => {
                let message = match &other {
                    Some(a) if !a.is_archetype() => {
                        format!(
                            "\"{}\" is not an archetype; equip it instead",
                            a.title_or_name()
                        )
                    }
                    _ => "The parent must be an archetype this character can reach".to_string(),
                };
                let mut details = serde_json::Map::new();
                details.insert("archetypeId".into(), serde_json::json!(id));
                let error = InventoryRefusal::invalid_parent(message, details);
                tracing::info!(
                    code = error.code.as_str(),
                    chatId = %fresh.id,
                    characterId = %character_id,
                    archetypeId = %id,
                    context = "inventory",
                    "[InventoryEquip] Refused"
                );
                return Err(error.into());
            }
        }
    }
    let refs: Vec<&ResolvedInventoryItem> = archetypes.iter().collect();
    assert_shelf_reach(main, mount, &fresh, character_id, &refs, ctx)?;

    if equip {
        let mut counted: Vec<&ResolvedInventoryItem> = current.iter().collect();
        for archetype in &archetypes {
            let kin = counted
                .iter()
                .filter(|other| is_kin(**other, archetype))
                .count() as i64;
            if kin + 1 > archetype.max_equipped() {
                let error = InventoryRefusal::cap_exceeded(
                    archetype.id(),
                    archetype.title_or_name(),
                    archetype.max_equipped(),
                );
                tracing::info!(
                    code = error.code.as_str(),
                    op = "pick-up",
                    chatId = %fresh.id,
                    characterId = %character_id,
                    itemId = %archetype.id(),
                    cap = archetype.max_equipped(),
                    context = "inventory",
                    "[InventoryEquip] Refused"
                );
                return Err(error.into());
            }
            counted.push(archetype);
        }
    }

    let vault = character_vault(
        main,
        mount,
        character_id,
        ctx.user_id,
        "to pick an object up into",
    )?;
    let title = opts
        .title
        .as_deref()
        .map(crate::jsstr::js_trim)
        .filter(|t| !t.is_empty());
    let overrides = title.map(|t| {
        let mut m = serde_json::Map::new();
        m.insert("title".into(), serde_json::json!(t));
        m
    });
    let mut created: Vec<ResolvedInventoryItem> = Vec::new();
    for archetype in &archetypes {
        let made = instantiate(
            main,
            mount,
            &vault,
            archetype.id(),
            InstantiateOptions {
                folder: None,
                overrides: overrides.as_ref(),
                pool: Some(&pool),
            },
        )?;
        created.push(made.item);
    }
    publish_realtime(RealtimeTopic::Inventory, None);

    if equip && !created.is_empty() {
        let next: Vec<String> = current
            .iter()
            .chain(created.iter())
            .map(|i| i.id().to_string())
            .collect();
        ChatsInventoryRepository::new(main).set_equipped_inventory(
            &fresh.id,
            character_id,
            Some(&next),
        );
        publish_realtime(RealtimeTopic::Chats, Some(&fresh.id));
    }
    tracing::debug!(
        op = "pick-up",
        chatId = %fresh.id,
        characterId = %character_id,
        archetypesJson = %ids_json(&archetypes),
        createdJson = %ids_json(&created),
        equipped = equip,
        context = "inventory",
        "[InventoryEquip] Picked up"
    );
    let change = InventoryChange {
        equipped: if equip { created.clone() } else { Vec::new() },
        unequipped: Vec::new(),
        picked_up: Some(created.clone()),
        given: None,
    };
    Ok((
        PickUpResult {
            created,
            equipped: equip,
        },
        Some(change),
    ))
}

/// Pick up an instance of each archetype for one character in one chat (v4
/// `pickUpItems`): the instances land at the root of the character's vault
/// (`title` overrides the archetype's, when given) and, unless `equip` is
/// false, are equipped. A breach of the cap creates nothing; a non-archetype
/// is `INVALID_PARENT`.
pub async fn pick_up_items(
    db: &Db,
    chat: &EquipChat,
    character_id: &str,
    archetype_ids: &[String],
    ctx: &InventoryChangeContext<'_>,
    opts: PickUpOptions,
) -> Result<GestureOutcome<PickUpResult>, InventoryError> {
    assert_participant(chat, character_id)?;
    let (chat, character_id, ids) = (
        chat.clone(),
        character_id.to_string(),
        archetype_ids.to_vec(),
    );
    let (user_id, provenance, announce) = (ctx.user_id.to_string(), ctx.provenance, ctx.announce);
    let chat_id = chat.id.clone();
    with_chat_inventory_lock(db, &chat_id, move |main, mount| {
        let ctx = InventoryChangeContext {
            user_id: &user_id,
            provenance,
            announce,
            pending: None,
        };
        pick_up_items_holding_lock(main, mount, &chat, &character_id, &ids, &ctx, &opts)
    })
    .await
}
