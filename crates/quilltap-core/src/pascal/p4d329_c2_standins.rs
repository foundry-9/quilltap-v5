//! LANE-REMOVE (P4.D329): the C2 surface this lane consumes, as stand-ins.
//!
//! P4.D328 owns `inventory::refs`, `inventory::context` and
//! `services::inventory_change_effects` (the round's C2 contract) and cuts from
//! the same `INVENTORY` commit as this lane, in parallel — so none of it exists
//! on this branch, and this lane may not create it. Everything here carries
//! EXACTLY C2's names, and nothing of this lane's own:
//!
//! - the TYPES are declared for real (the trait, the state and scope enums, the
//!   aggregate object), so the Pascal grammar, the bench's mock bag and the
//!   harness's recorded bag compile and run against the real contract;
//! - the LOADED shapes (`ResolvedInventoryPool`, `InventoryRefMatch`,
//!   `LoadedInventoryContext`) are UNINHABITED here — each carries a private
//!   [`Infallible`] — so no pool and no loaded bag can exist on this branch,
//!   and the pure functions over them are provably unreachable rather than
//!   quietly empty;
//! - every fallible entry answers the ONE typed refusal
//!   [`InventoryContextError::NotAvailable`] (the `not_available` idiom —
//!   never a fake pool, never a panic).
//!
//! The consequence is named, not hidden: on this branch the roster's lazy
//! load, the run service's bag read and the applier's load take v4's own
//! `catch` arms with this refusal's text. The unifier deletes this file at
//! §S.5 wire 1 and rewrites each consumer's ONE fenced `use` line to
//! `crate::inventory::refs::…` / `crate::inventory::context::…` /
//! `crate::services::inventory_change_effects::…`.

use std::collections::{HashMap, HashSet};
use std::convert::Infallible;
use std::sync::{Arc, Mutex};

use rusqlite::Connection;
use serde_json::{Map, Value};

use crate::db::runtime::Db;
use crate::inventory::equip::{InventoryChange, InventoryChangeContext, PendingInventoryChange};
use crate::inventory::types::ResolvedInventoryItem;

/// The refusal text every stand-in answers.
pub const NOT_AVAILABLE: &str =
    "P4.D328's inventory::refs / inventory::context have not landed on this branch";

/// v4 `InventoryState` — where an object stands for the rolling character in
/// this chat.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum InventoryState {
    Equipped,
    Carried,
    Reachable,
    Absent,
}

impl InventoryState {
    pub fn as_str(self) -> &'static str {
        match self {
            InventoryState::Equipped => "equipped",
            InventoryState::Carried => "carried",
            InventoryState::Reachable => "reachable",
            InventoryState::Absent => "absent",
        }
    }
}

/// v4 `AggregateScope` — which objects a fold runs over. Cumulative:
/// equipped ⊆ owned ⊆ reachable (R-O: homed in `inventory::refs`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AggregateScope {
    Equipped,
    Owned,
    Reachable,
}

impl AggregateScope {
    pub fn as_str(self) -> &'static str {
        match self {
            AggregateScope::Equipped => "equipped",
            AggregateScope::Owned => "owned",
            AggregateScope::Reachable => "reachable",
        }
    }
}

/// v4 `AggregateObject` — one object as the aggregate sheet reads it. `title`
/// is already `title || name`, the `{{inventory.<ref>.title}}` rule.
#[derive(Debug, Clone, PartialEq)]
pub struct AggregateObject {
    pub name: String,
    pub title: String,
    pub metadata: Map<String, Value>,
}

/// v4 `ResolvedInventoryPool` — uninhabited on this branch (see the module
/// note): only a real load makes one.
#[derive(Debug)]
pub struct ResolvedInventoryPool {
    pub items: Vec<ResolvedInventoryItem>,
    never: Infallible,
}

impl ResolvedInventoryPool {
    /// True when the object lives in the holder's own tier.
    pub fn owns(&self, _id: &str) -> bool {
        match self.never {}
    }
}

/// v4 `InventoryRefMatch` — uninhabited on this branch.
#[derive(Debug)]
pub struct InventoryRefMatch {
    pub item: ResolvedInventoryItem,
    pub equipped: bool,
    pub own: bool,
    never: Infallible,
}

/// v4 `ResolveInventoryRefOptions`.
#[derive(Debug, Clone, Copy, Default)]
pub struct ResolveInventoryRefOptions {
    pub include_archetypes: bool,
    pub include_archived: bool,
}

/// v4 `InventoryContext` (`refs.ts:169-187`), method for method. `pool` /
/// `equipped_ids` are absent on a mock (the bench).
///
/// `Send + Sync`: the deal (`execute_custom_tool`) is async and holds the bag
/// across the consult's await inside the engine's `Send` dispatch future — a
/// Rust-only obligation v4's single thread never states (`HANDOFF:` P4.D328
/// spells its trait the same way).
pub trait InventoryContext: Send + Sync {
    fn character_id(&self) -> &str;
    fn chat_id(&self) -> Option<&str>;
    /// The best state of a reference.
    fn state(&self, reference: &str) -> InventoryState;
    /// Equipped kin of a reference — a JS number.
    fn equipped_kin_count(&self, reference: &str) -> f64;
    /// The object a reference reads as (equipped first, then own, then
    /// shared), or `None`.
    fn item(&self, reference: &str) -> Option<ResolvedInventoryItem>;
    /// The objects in an `inventoryAggregate` scope. Pure; never fails.
    fn scope_objects(&self, scope: AggregateScope) -> Vec<AggregateObject>;
    fn pool(&self) -> Option<&ResolvedInventoryPool> {
        None
    }
    fn equipped_ids(&self) -> Option<&HashSet<String>> {
        None
    }
}

/// The one refusal (the `not_available` idiom).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InventoryContextError {
    NotAvailable(&'static str),
}

impl std::fmt::Display for InventoryContextError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InventoryContextError::NotAvailable(text) => f.write_str(text),
        }
    }
}

impl std::error::Error for InventoryContextError {}

/// v4 `resolveInventoryRef` — unreachable here: no pool can exist.
pub fn resolve_inventory_ref(
    _reference: &str,
    pool: &ResolvedInventoryPool,
    _equipped: &HashSet<String>,
    _opts: ResolveInventoryRefOptions,
) -> Vec<InventoryRefMatch> {
    match pool.never {}
}

/// v4 `pickForGesture` — unreachable past an empty list: no match can exist.
pub fn pick_for_gesture<'m>(
    matches: &'m [InventoryRefMatch],
    _action: &str,
) -> Option<&'m InventoryRefMatch> {
    match matches.first() {
        None => None,
        Some(m) => match m.never {},
    }
}

/// v4 `LoadedInventoryContext` — uninhabited on this branch.
#[derive(Debug)]
pub struct LoadedInventoryContext {
    pub character_id: String,
    pub chat_id: Option<String>,
    pub pool: ResolvedInventoryPool,
    pub equipped_ids: HashSet<String>,
    pub closed_shelf: Vec<ResolvedInventoryItem>,
    never: Infallible,
}

impl InventoryContext for LoadedInventoryContext {
    fn character_id(&self) -> &str {
        match self.never {}
    }
    fn chat_id(&self) -> Option<&str> {
        match self.never {}
    }
    fn state(&self, _reference: &str) -> InventoryState {
        match self.never {}
    }
    fn equipped_kin_count(&self, _reference: &str) -> f64 {
        match self.never {}
    }
    fn item(&self, _reference: &str) -> Option<ResolvedInventoryItem> {
        match self.never {}
    }
    fn scope_objects(&self, _scope: AggregateScope) -> Vec<AggregateObject> {
        match self.never {}
    }
    fn pool(&self) -> Option<&ResolvedInventoryPool> {
        match self.never {}
    }
    fn equipped_ids(&self) -> Option<&HashSet<String>> {
        match self.never {}
    }
}

/// v4 `loadInventoryContext`, at the CONNECTION level so both a roster read
/// and the applier's in-write re-read can call it (open question 1 — a
/// `HANDOFF:` to P4.D328). Refuses on this branch.
pub fn load_inventory_context(
    _main: &Connection,
    _mount: &Connection,
    _character_id: &str,
    _chat_id: Option<&str>,
) -> Result<LoadedInventoryContext, InventoryContextError> {
    Err(InventoryContextError::NotAvailable(NOT_AVAILABLE))
}

/// The turn's pending inventory announcements (keyed `chatId:characterId`).
pub type PendingInventoryAnnouncements = Arc<Mutex<HashMap<String, PendingInventoryChange>>>;

/// v4 `notifyInventoryChanged` — refuses on this branch.
pub async fn notify_inventory_changed(
    _db: &Db,
    _chat_id: &str,
    _character_id: &str,
    _change: &InventoryChange,
    _ctx: &mut InventoryChangeContext<'_>,
) -> Result<(), InventoryContextError> {
    Err(InventoryContextError::NotAvailable(NOT_AVAILABLE))
}

/// v4 `recordPendingInventoryChange` — refuses on this branch.
pub fn record_pending_inventory_change(
    _pending: &mut HashMap<String, PendingInventoryChange>,
    _chat_id: &str,
    _character_id: &str,
    _change: &InventoryChange,
    _ctx: &InventoryChangeContext<'_>,
) -> Result<(), InventoryContextError> {
    Err(InventoryContextError::NotAvailable(NOT_AVAILABLE))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every fallible stand-in answers the ONE refusal, so the unifier's
    /// deletion cannot leave a silent copy behind (a stand-in that started
    /// answering anything else would be a port in disguise).
    #[test]
    fn every_stand_in_refuses_not_available() {
        let main = Connection::open_in_memory().unwrap();
        let mount = Connection::open_in_memory().unwrap();
        let refused = InventoryContextError::NotAvailable(NOT_AVAILABLE);
        assert_eq!(
            load_inventory_context(&main, &mount, "c", Some("chat")).unwrap_err(),
            refused
        );
        assert_eq!(refused.to_string(), NOT_AVAILABLE);

        let change = InventoryChange {
            equipped: vec![],
            unequipped: vec![],
            picked_up: None,
            given: None,
        };
        let ctx = InventoryChangeContext {
            user_id: "u",
            provenance: crate::inventory::equip::InventoryChangeProvenance::Pascal,
            announce: true,
            pending: None,
        };
        let mut map = HashMap::new();
        assert_eq!(
            record_pending_inventory_change(&mut map, "chat", "c", &change, &ctx).unwrap_err(),
            refused
        );
        assert!(pick_for_gesture(&[], "equip").is_none());
    }
}
