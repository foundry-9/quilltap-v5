//! Inventory references — the one rule that turns a string into objects (v4
//! `lib/inventory/refs.ts`, `e1872c94f` + `a53024d3c`'s `scopeObjects`;
//! P4.D328).
//!
//! A reference names an object by its id, else its title, else its name,
//! case-insensitively (spec inventory-tools-and-pascal §5.1). Every
//! `inventory_*` tool, Pascal's `inventory` / `inventoryCount` subjects, the
//! `{{inventory.*}}` placeholders, the `$inventory` operand and the
//! `inventory` effect resolve through here; nothing else re-derives
//! id-or-title-or-name. A reference that resolves to nothing is `absent`,
//! never an error.
//!
//! Also the [`InventoryContext`]: what one tool call or one custom-tool run
//! knows about the rolling character's bag, built once and handed to every
//! reader. The server builds it from a pool and a chat
//! (`inventory::context`); the Proving Bench builds a mock one (P4.D329's
//! `pascal::bench_inventory`).
//!
//! Pure: no connection, no logging. v4's `byIdCache` (a `WeakMap` keyed by the
//! pool's `items` array) has no twin — the by-id map is built per call, which
//! no caller can observe.

use std::collections::{HashMap, HashSet};

use serde::Serialize;
use serde_json::{Map, Value};

use super::resolve::{is_kin, kit_state};
use super::types::{InventoryFields, KitState, ResolvedInventoryItem};
use crate::jsstr::js_trim;

/// Where an object stands for the rolling character in this chat (v4
/// `InventoryState`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum InventoryState {
    Equipped,
    Carried,
    Reachable,
    Absent,
}

/// The four states, best first (v4 `INVENTORY_STATES`).
pub const INVENTORY_STATES: [InventoryState; 4] = [
    InventoryState::Equipped,
    InventoryState::Carried,
    InventoryState::Reachable,
    InventoryState::Absent,
];

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

/// The resolved objects a character can reach, and which of them are their
/// own (v4 `ResolvedInventoryPool`). `owns` is v4's `owns(id)` closure.
pub struct ResolvedInventoryPool {
    /// Every reachable object, resolved; the holder's own archived ones included.
    pub items: Vec<ResolvedInventoryItem>,
    /// True when the object lives in the holder's own tier.
    pub owns: Box<dyn Fn(&str) -> bool + Send + Sync>,
}

impl std::fmt::Debug for ResolvedInventoryPool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResolvedInventoryPool")
            .field("items", &self.items)
            .finish_non_exhaustive()
    }
}

impl ResolvedInventoryPool {
    pub fn owns(&self, id: &str) -> bool {
        (self.owns)(id)
    }
}

/// One object a reference matched (v4 `InventoryRefMatch`).
#[derive(Clone, Debug, PartialEq)]
pub struct InventoryRefMatch {
    pub item: ResolvedInventoryItem,
    pub equipped: bool,
    pub own: bool,
}

/// v4 `ResolveInventoryRefOptions` — both default false.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ResolveInventoryRefOptions {
    /// Archetypes match too (a pick-up names one).
    pub include_archetypes: bool,
    /// The holder's own archived objects match too (a restore names one).
    pub include_archived: bool,
}

/// v4 `norm` — `(value ?? '').trim().toLowerCase()` (Rust's `to_lowercase`
/// is byte-identical to JS's, the Phase-1 finding).
fn norm(value: &str) -> String {
    js_trim(value).to_lowercase()
}

fn by_id_of(pool: &ResolvedInventoryPool) -> HashMap<&str, &ResolvedInventoryItem> {
    // v4 `new Map(items.map(...))`: a later duplicate id overwrites the value.
    let mut map = HashMap::new();
    for item in &pool.items {
        map.insert(item.id(), item);
    }
    map
}

/// Is this object carried? A kit is carried when every member is (v4
/// `isEquippedIn`).
fn is_equipped_in(
    item: &ResolvedInventoryItem,
    pool: &ResolvedInventoryPool,
    equipped_ids: &HashSet<String>,
) -> bool {
    if !item.is_kit() {
        return equipped_ids.contains(item.id());
    }
    let by_id = by_id_of(pool);
    // `Array.from(equippedIds)` — the set's insertion order; `kit_state`
    // reads membership only, so the order cannot show.
    let equipped: Vec<ResolvedInventoryItem> = equipped_ids
        .iter()
        .filter_map(|id| by_id.get(id.as_str()).map(|i| (*i).clone()))
        .collect();
    kit_state(&item.item_ids(), &equipped, |id| {
        by_id.get(id).map(|i| (*i).clone())
    }) == KitState::All
}

/// id → title → name, case-insensitive, over the resolved pool (v4
/// `resolveInventoryRef`). The first RULE that matches anything wins
/// POOL-WIDE (an untitled instance named by its kind's name is found by the
/// name round even beside a titled sibling — the title round is skipped only
/// when no item's title matches); matches come own tier first, each half in
/// pool order. Archetypes and archived objects are excluded unless asked.
pub fn resolve_inventory_ref(
    reference: &str,
    pool: &ResolvedInventoryPool,
    equipped_ids: &HashSet<String>,
    opts: ResolveInventoryRefOptions,
) -> Vec<InventoryRefMatch> {
    let wanted = norm(reference);
    if wanted.is_empty() {
        return Vec::new();
    }
    let candidates: Vec<&ResolvedInventoryItem> = pool
        .items
        .iter()
        .filter(|item| {
            (opts.include_archetypes || !item.is_archetype())
                && (!item.is_archived() || (opts.include_archived && pool.owns(item.id())))
        })
        .collect();
    let rules: [&dyn Fn(&ResolvedInventoryItem) -> bool; 3] = [
        &|item| norm(item.id()) == wanted,
        &|item| {
            item.title()
                .is_some_and(|t| !t.is_empty() && norm(t) == wanted)
        },
        &|item| norm(item.name()) == wanted,
    ];
    for rule in rules {
        let hits: Vec<&ResolvedInventoryItem> = candidates
            .iter()
            .copied()
            .filter(|item| rule(item))
            .collect();
        if hits.is_empty() {
            continue;
        }
        let matches: Vec<InventoryRefMatch> = hits
            .into_iter()
            .map(|item| InventoryRefMatch {
                item: item.clone(),
                equipped: is_equipped_in(item, pool, equipped_ids),
                own: pool.owns(item.id()),
            })
            .collect();
        // Own tier before shared; otherwise the pool's order.
        let (own, shared): (Vec<_>, Vec<_>) = matches.into_iter().partition(|m| m.own);
        return own.into_iter().chain(shared).collect();
    }
    Vec::new()
}

/// The best state among the matches: equipped > carried > reachable > absent
/// (v4 `stateOf`). Archetypes and archived objects never count, even when the
/// matches include them.
pub fn state_of(matches: &[InventoryRefMatch]) -> InventoryState {
    let carriable: Vec<&InventoryRefMatch> = matches
        .iter()
        .filter(|m| !m.item.is_archetype() && !m.item.is_archived())
        .collect();
    if carriable.iter().any(|m| m.equipped) {
        InventoryState::Equipped
    } else if carriable.iter().any(|m| m.own) {
        InventoryState::Carried
    } else if !carriable.is_empty() {
        InventoryState::Reachable
    } else {
        InventoryState::Absent
    }
}

/// A bare name for [`is_kin`] (it reads only `name`).
struct NameOnly(Map<String, Value>);

impl InventoryFields for NameOnly {
    fn fields(&self) -> &Map<String, Value> {
        &self.0
    }
}

/// Equipped kin (same resolved name, case-insensitive) of `name` (v4
/// `equippedKinCountByName`): equipped, not archived, not a kit.
pub fn equipped_kin_count_by_name(
    name: &str,
    pool: &ResolvedInventoryPool,
    equipped_ids: &HashSet<String>,
) -> usize {
    let mut probe = Map::new();
    probe.insert("name".into(), Value::String(name.to_string()));
    let probe = NameOnly(probe);
    pool.items
        .iter()
        .filter(|item| {
            equipped_ids.contains(item.id())
                && !item.is_archived()
                && !item.is_kit()
                && is_kin(*item, &probe)
        })
        .count()
}

/// Equipped kin of the FIRST match's resolved name, archetypes included in
/// the match (an archetype reference counts its instances); no match → 0
/// (v4 `equippedKinCount`).
pub fn equipped_kin_count(
    reference: &str,
    pool: &ResolvedInventoryPool,
    equipped_ids: &HashSet<String>,
) -> usize {
    let first = resolve_inventory_ref(
        reference,
        pool,
        equipped_ids,
        ResolveInventoryRefOptions {
            include_archetypes: true,
            include_archived: false,
        },
    )
    .into_iter()
    .next();
    first
        .map(|m| equipped_kin_count_by_name(m.item.name(), pool, equipped_ids))
        .unwrap_or(0)
}

/// The two gestures [`pick_for_gesture`] picks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gesture {
    Equip,
    Unequip,
}

/// For a gesture: the match to act on — the first equipped one for unequip,
/// the first unequipped, carriable one for equip (kits allowed), own tier
/// before shared (v4 `pickForGesture`). `None` when none qualifies.
pub fn pick_for_gesture(
    matches: &[InventoryRefMatch],
    gesture: Gesture,
) -> Option<&InventoryRefMatch> {
    match gesture {
        Gesture::Unequip => matches.iter().find(|m| m.equipped),
        Gesture::Equip => matches
            .iter()
            .find(|m| !m.equipped && !m.item.is_archetype() && !m.item.is_archived()),
    }
}

// ============================================================================
// The aggregate sheet's scopes (R-O / R-7 — the types live here; the folds are
// P4.D329's `pascal::inventory_aggregate`, which re-exports these)
// ============================================================================

/// An `inventoryAggregate` scope: `equipped` ⊆ `owned` ⊆ `reachable` (v4
/// `AggregateScope`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AggregateScope {
    Equipped,
    Owned,
    Reachable,
}

/// The three scopes in v4's order.
pub const AGGREGATE_SCOPES: [AggregateScope; 3] = [
    AggregateScope::Equipped,
    AggregateScope::Owned,
    AggregateScope::Reachable,
];

impl AggregateScope {
    pub fn as_str(self) -> &'static str {
        match self {
            AggregateScope::Equipped => "equipped",
            AggregateScope::Owned => "owned",
            AggregateScope::Reachable => "reachable",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        AGGREGATE_SCOPES
            .into_iter()
            .find(|scope| scope.as_str() == s)
    }
}

/// One object as the aggregate sheet reads it (v4 `AggregateObject`):
/// `title` is `title || name` (no trim — a whitespace-only title is kept),
/// `metadata` always present (`metadata ?? {}`).
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AggregateObject {
    pub name: String,
    pub title: String,
    pub metadata: Map<String, Value>,
}

/// The carriable objects in a scope (v4 `scopeObjectsOf`): the
/// `resolveInventoryRef` rule (no archetypes, no archived objects) and no
/// kits (a kit's members stand for it). Objects are already resolved, so an
/// instance carries its archetype's metadata.
pub fn scope_objects_of(
    scope: AggregateScope,
    pool: &ResolvedInventoryPool,
    equipped_ids: &HashSet<String>,
) -> Vec<AggregateObject> {
    pool.items
        .iter()
        .filter(|item| !item.is_archetype() && !item.is_kit() && !item.is_archived())
        .filter(|item| {
            if scope == AggregateScope::Reachable {
                return true;
            }
            let equipped = equipped_ids.contains(item.id());
            if scope == AggregateScope::Equipped {
                equipped
            } else {
                equipped || pool.owns(item.id())
            }
        })
        .map(|item| AggregateObject {
            name: item.name().to_string(),
            title: item.title_or_name().to_string(),
            metadata: item.metadata(),
        })
        .collect()
}

// ============================================================================
// The context one call or one run reads
// ============================================================================

/// What one tool call or one custom-tool run knows about the rolling
/// character's bag (v4 `InventoryContext`). A trait so the Proving Bench's
/// mock (P4.D329, R-U: `pool()` / `equipped_ids()` `None`, `item()` always
/// `None`) and the real pool's context answer the same readers.
pub trait InventoryContext: Send + Sync {
    fn character_id(&self) -> &str;
    fn chat_id(&self) -> Option<&str>;
    /// The best state of a reference.
    fn state(&self, reference: &str) -> InventoryState;
    /// Equipped kin of a reference.
    fn equipped_kin_count(&self, reference: &str) -> usize;
    /// The object a reference reads as (equipped first, then own, then
    /// shared), or `None`. An OWNED clone, so no trait-object lifetime is
    /// pinned across a reader's render.
    fn item(&self, reference: &str) -> Option<ResolvedInventoryItem>;
    /// The objects in an `inventoryAggregate` scope. Pure; never fails.
    fn scope_objects(&self, scope: AggregateScope) -> &[AggregateObject];
    /// The real pool; `None` on a mock (the bench).
    fn pool(&self) -> Option<&ResolvedInventoryPool>;
    /// The real equipped ids; `None` on a mock.
    fn equipped_ids(&self) -> Option<&HashSet<String>>;
}

/// The context over a real resolved pool (v4 `inventoryContextFrom`'s
/// object). The three scopes are computed ONCE at construction — v4 memoises
/// them lazily; both are pure, so the difference cannot show.
#[derive(Debug)]
pub struct ResolvedInventoryContext {
    pub character_id: String,
    pub chat_id: Option<String>,
    pub pool: ResolvedInventoryPool,
    pub equipped_ids: HashSet<String>,
    scopes: [Vec<AggregateObject>; 3],
}

/// Build a context over a resolved pool and the character's equipped ids
/// (v4 `inventoryContextFrom`). Pure.
pub fn inventory_context_from(
    character_id: &str,
    chat_id: Option<&str>,
    pool: ResolvedInventoryPool,
    equipped_ids: HashSet<String>,
) -> ResolvedInventoryContext {
    let scopes = AGGREGATE_SCOPES.map(|scope| scope_objects_of(scope, &pool, &equipped_ids));
    ResolvedInventoryContext {
        character_id: character_id.to_string(),
        chat_id: chat_id.map(str::to_string),
        pool,
        equipped_ids,
        scopes,
    }
}

impl ResolvedInventoryContext {
    /// Resolve a ref over this context (the handlers' `resolveToolRef`).
    pub fn resolve(
        &self,
        reference: &str,
        opts: ResolveInventoryRefOptions,
    ) -> Vec<InventoryRefMatch> {
        resolve_inventory_ref(reference, &self.pool, &self.equipped_ids, opts)
    }
}

impl InventoryContext for ResolvedInventoryContext {
    fn character_id(&self) -> &str {
        &self.character_id
    }
    fn chat_id(&self) -> Option<&str> {
        self.chat_id.as_deref()
    }
    fn state(&self, reference: &str) -> InventoryState {
        state_of(&self.resolve(reference, ResolveInventoryRefOptions::default()))
    }
    fn equipped_kin_count(&self, reference: &str) -> usize {
        equipped_kin_count(reference, &self.pool, &self.equipped_ids)
    }
    fn item(&self, reference: &str) -> Option<ResolvedInventoryItem> {
        // DEFAULT options — no archetype ever renders.
        let matches = self.resolve(reference, ResolveInventoryRefOptions::default());
        matches
            .iter()
            .find(|m| m.equipped)
            .or_else(|| matches.iter().find(|m| m.own))
            .or_else(|| matches.first())
            .map(|m| m.item.clone())
    }
    fn scope_objects(&self, scope: AggregateScope) -> &[AggregateObject] {
        let at = AGGREGATE_SCOPES
            .iter()
            .position(|s| *s == scope)
            .expect("every scope is in AGGREGATE_SCOPES");
        &self.scopes[at]
    }
    fn pool(&self) -> Option<&ResolvedInventoryPool> {
        Some(&self.pool)
    }
    fn equipped_ids(&self) -> Option<&HashSet<String>> {
        Some(&self.equipped_ids)
    }
}
