//! Inventory change effects — what follows a gesture that changes what a
//! character carries in a chat (v4 `lib/inventory/equip-effects.ts`,
//! `e1872c94f` + `102a23648`; P4.D328). The one chokepoint is
//! [`notify_inventory_changed`]:
//!
//!   - when `ctx.announce`, Pascal posts one plain line for the gesture
//!     (`systemKind: 'inventory-change'`, no roll record): *"Vivienne takes up
//!     Excalibur and a signet ring."* / *"Vivienne sets down the lantern."*;
//!   - the character's per-chat avatar is refreshed the way an outfit change
//!     refreshes it, so a newly carried sword can reach the portrait.
//!
//! Arrival defaults pass `announce: false` (arrival is not a gesture), and a
//! mutation outside a chat never comes here.
//!
//! Inside a model turn the caller hands the turn's pending map (minted by
//! [`new_wardrobe_turn_containers`](super::outfit_change_effects::new_wardrobe_turn_containers)):
//! the change is recorded there instead, netted with the turn's other changes
//! for that character, and posted once — line and avatar refresh — by
//! [`flush_pending_inventory_announcements`] at turn end, so a character's
//! `inventory_*` calls and a Pascal inventory effect in one turn read as one
//! line. A give (`change.given`) always posts a line of its own, at once.
//!
//! ## The four types and the map (R-1 / R-2)
//!
//! The change types are the keystone's (`inventory::equip` — its gestures
//! RETURN the change) and are re-exported here under their names. C1's
//! `InventoryChangeContext.pending` is a `&mut HashMap`, which a turn's map —
//! shared by every tool call and Pascal run of one turn — cannot lend across
//! the notify's awaits (a `std::sync::MutexGuard` is not `Send`). So the
//! turn's map travels as its own `pending` argument, the shared
//! [`PendingInventoryAnnouncements`], locked only inside the record / strike
//! (never across an `.await`) and drained under the lock BEFORE any await in
//! the flush — the wardrobe set's precedent. C1's `pending` field is never
//! read here; callers pass `None` in it.
//!
//! ## Ordering
//!
//! v4 iterates its `Map` in insertion order; v5's `HashMap` is unordered, so
//! the flush drains in KEY order (`chatId:characterId`). Each entry's post and
//! refresh are independent and keyed per character, so the rows and jobs are
//! the same; only the order of two characters' lines in one turn can differ —
//! a recorded-only class (R-6).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde::Serialize;

use crate::db::runtime::Db;
use crate::inventory::resolve::inventory_label;
use crate::inventory::types::{InventoryFields, ResolvedInventoryItem};
use crate::jsstr::{is_js_ws, js_trim};
use crate::services::outfit_change_effects::refresh_avatar_for_outfit;
use crate::services::pascal_writer::post_pascal_inventory_change;

pub use crate::inventory::equip::{
    InventoryChange, InventoryChangeContext, InventoryChangeProvenance, InventoryGive,
    PendingInventoryChange,
};

/// The turn's pending Inventory changes, keyed `chatId:characterId` (v4's
/// optional `pendingInventoryAnnouncements?: Map<string,
/// PendingInventoryChange>`), shared by reference through every tool call
/// and Pascal run of one turn.
pub type PendingInventoryAnnouncements = Arc<Mutex<HashMap<String, PendingInventoryChange>>>;

/// The `source_context` the avatar refresh carries (v4
/// `refreshAvatarForAppearance(…, 'inventory-change')`). R-5: the refresh is
/// the EXISTING `refresh_avatar_for_outfit` — v4's rename to
/// `refreshAvatarForAppearance` (an alias kept) is P4.D326's.
pub const INVENTORY_CHANGE_SOURCE_CONTEXT: &str = "inventory-change";

/// The `context` field of every line here.
const CONTEXT: &str = "inventory";

// ============================================================================
// Pascal's line
// ============================================================================

/// A plural-looking name ("lockpicks", "gloves") — not "compass", "abacus" or
/// "iris" (v4 `looksPlural`): the LAST whitespace-separated word ends in `s`
/// but not in `ss` / `us` / `is`, case-insensitively.
fn looks_plural(name: &str) -> bool {
    // `name.split(/\s+/).pop() ?? name` — a trailing separator leaves an empty
    // last word, which ends in nothing.
    let last = name.split(is_js_ws).next_back().unwrap_or(name);
    let lower = last.to_ascii_lowercase();
    lower.ends_with('s')
        && !(lower.ends_with("ss") || lower.ends_with("us") || lower.ends_with("is"))
}

/// "an" before a vowel sound, by spelling: "an amulet", but "a unicorn horn",
/// "a one-eyed idol" (v4 `indefiniteArticle`). Both rules test the WHOLE
/// name's first letters (`"old axe"` → `an`), ASCII-case-insensitively — v4's
/// `i`-flagged regexes over ASCII letters; a non-ASCII first letter matches
/// neither (→ `a`).
fn indefinite_article(name: &str) -> &'static str {
    let lower = name
        .chars()
        .take(4)
        .map(|c| c.to_ascii_lowercase())
        .collect::<String>();
    if ["uni", "use", "usu", "uti", "eu", "one", "once"]
        .iter()
        .any(|p| lower.starts_with(p))
    {
        return "a";
    }
    if lower.starts_with(['a', 'e', 'i', 'o', 'u']) {
        "an"
    } else {
        "a"
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Article {
    A,
    The,
}

/// v4 `withArticle`: a trimmed title wins outright; `the` names the thing;
/// a plural-looking name takes `some`; else `a` / `an`.
fn with_article(item: &ResolvedInventoryItem, article: Article) -> String {
    if let Some(title) = item.title().map(js_trim).filter(|t| !t.is_empty()) {
        return title.to_string();
    }
    let name = js_trim(item.name());
    if article == Article::The {
        return format!("the {name}");
    }
    if looks_plural(name) {
        return format!("some {name}");
    }
    format!("{} {name}", indefinite_article(name))
}

/// v4 `joinList`: one → itself; two → `A and B`; more → `A, B, and C`.
fn join_list(parts: &[String]) -> String {
    match parts.len() {
        0 | 1 => parts.join(""),
        2 => format!("{} and {}", parts[0], parts[1]),
        n => format!("{}, and {}", parts[..n - 1].join(", "), parts[n - 1]),
    }
}

/// Pascal's line for a change, or `None` when nothing changed (v4
/// `describeInventoryChange`). Untitled objects take an article ("a signet
/// ring" or "some lockpicks" taken up, "the lantern" set down); titled ones
/// are named ("Excalibur"). An object picked up this gesture reads "picks
/// up", never also "takes up".
pub fn describe_inventory_change(character_name: &str, change: &InventoryChange) -> Option<String> {
    if let Some(given) = &change.given {
        return Some(format!(
            "*{character_name} hands {} to {}.*",
            with_article(&given.item, Article::The),
            given.to_name
        ));
    }
    let picked: Vec<&ResolvedInventoryItem> = change.picked_up.iter().flatten().collect();
    let picked_ids: std::collections::HashSet<&str> = picked.iter().map(|i| i.id()).collect();
    let mut clauses: Vec<String> = Vec::new();
    if !change.unequipped.is_empty() {
        let parts: Vec<String> = change
            .unequipped
            .iter()
            .map(|i| with_article(i, Article::The))
            .collect();
        clauses.push(format!("sets down {}", join_list(&parts)));
    }
    let taken_up: Vec<String> = change
        .equipped
        .iter()
        .filter(|i| !picked_ids.contains(i.id()))
        .map(|i| with_article(i, Article::A))
        .collect();
    if !taken_up.is_empty() {
        clauses.push(format!("takes up {}", join_list(&taken_up)));
    }
    if !picked_ids.is_empty() {
        let parts: Vec<String> = picked.iter().map(|i| with_article(i, Article::A)).collect();
        clauses.push(format!("picks up {}", join_list(&parts)));
    }
    if clauses.is_empty() {
        return None;
    }
    Some(format!("*{character_name} {}.*", clauses.join(" and ")))
}

// ============================================================================
// The turn's pending map (pure — the caller holds the lock)
// ============================================================================

fn push_unique(list: &mut Vec<ResolvedInventoryItem>, item: &ResolvedInventoryItem) {
    if !list.iter().any(|existing| existing.id() == item.id()) {
        list.push(item.clone());
    }
}

fn remove_by_id(list: &mut Vec<ResolvedInventoryItem>, id: &str) {
    if let Some(at) = list.iter().position(|item| item.id() == id) {
        list.remove(at);
    }
}

/// Who a pending entry is for.
#[derive(Clone, Copy, Debug)]
pub struct PendingWho<'a> {
    pub user_id: &'a str,
    pub chat_id: &'a str,
    pub character_id: &'a str,
}

/// The pending map's key (v4 `` `${chatId}:${characterId}` ``).
pub fn pending_inventory_key(chat_id: &str, character_id: &str) -> String {
    format!("{chat_id}:{character_id}")
}

/// Fold a change into the turn's pending entry for the character, netting
/// opposites (v4 `recordPendingInventoryChange`): the provenance once, in
/// first-seen order; an equip undoes a pending unequip of the same object,
/// else joins the equipped list (once); an unequip undoes a pending equip AND
/// removes the object from `pickedUp` (picked up and set down in one turn
/// announces nothing), else joins the unequipped list; `pickedUp` joins once.
pub fn record_pending_inventory_change(
    pending: &mut HashMap<String, PendingInventoryChange>,
    who: PendingWho<'_>,
    change: &InventoryChange,
    provenance: InventoryChangeProvenance,
) {
    let key = pending_inventory_key(who.chat_id, who.character_id);
    let entry = pending
        .entry(key)
        .or_insert_with(|| PendingInventoryChange {
            user_id: who.user_id.to_string(),
            chat_id: who.chat_id.to_string(),
            character_id: who.character_id.to_string(),
            equipped: Vec::new(),
            unequipped: Vec::new(),
            picked_up: Vec::new(),
            provenances: Vec::new(),
        });
    if !entry.provenances.contains(&provenance) {
        entry.provenances.push(provenance);
    }
    for item in &change.equipped {
        match entry
            .unequipped
            .iter()
            .position(|other| other.id() == item.id())
        {
            Some(undone) => {
                entry.unequipped.remove(undone);
            }
            None => push_unique(&mut entry.equipped, item),
        }
    }
    for item in &change.unequipped {
        match entry
            .equipped
            .iter()
            .position(|other| other.id() == item.id())
        {
            Some(undone) => {
                entry.equipped.remove(undone);
                remove_by_id(&mut entry.picked_up, item.id());
            }
            None => push_unique(&mut entry.unequipped, item),
        }
    }
    for item in change.picked_up.iter().flatten() {
        push_unique(&mut entry.picked_up, item);
    }
}

/// Take one object out of a character's pending entry altogether — it was
/// given away this turn (v4 `strikeFromPendingInventoryChange`). An unknown
/// key is a no-op.
pub fn strike_from_pending_inventory_change(
    pending: &mut HashMap<String, PendingInventoryChange>,
    key: &str,
    item_id: &str,
) {
    let Some(entry) = pending.get_mut(key) else {
        return;
    };
    remove_by_id(&mut entry.equipped, item_id);
    remove_by_id(&mut entry.unequipped, item_id);
    remove_by_id(&mut entry.picked_up, item_id);
}

// ============================================================================
// The chokepoint and the drain
// ============================================================================

fn labels_json(items: &[ResolvedInventoryItem]) -> String {
    let labels: Vec<String> = items.iter().map(inventory_label).collect();
    serde_json::to_string(&labels).unwrap_or_default()
}

/// The character's display name for Pascal's line (v4
/// `repos.characters.findById(id)?.name ?? 'Someone'` — the overlaid read).
fn character_name(db: &Db, character_id: &str) -> Result<String, crate::db::DbError> {
    let cid = character_id.to_string();
    let row = db.read_main(|main| {
        db.read_mount_index(|mount| crate::db::characters_read::find_by_id(main, mount, &cid))
    })?;
    Ok(row
        .as_ref()
        .and_then(|r| r.get("name"))
        .and_then(|n| n.as_str())
        .unwrap_or("Someone")
        .to_string())
}

/// The effects of one gesture (v4 `notifyInventoryChanged`). Call once per
/// gesture, after its write committed. Never fails.
///
/// `ctx` is the gesture's context (`user_id`, `provenance`, `announce`); the
/// turn's map, when there is one, is `pending` (module header).
///
/// 1. Nothing taken up, set down, picked up or given → nothing.
/// 2. A map, `announce`, and not a give → the change joins the turn's entry
///    (DEBUG `Inventory change joined the turn`) and NOTHING else happens now
///    — the line and the refresh come at the flush.
/// 3. A map and a give → the object is struck from the giver's entry (the
///    give posts now, so it must not also be announced at turn end).
/// 4. Then: when `announce`, Pascal's line (if any); ALWAYS the avatar
///    refresh; DEBUG `Inventory change effects fired`. A failed name read is
///    v4's caught failure (WARN `Inventory change effects failed`).
pub async fn notify_inventory_changed(
    db: &Db,
    chat_id: &str,
    character_id: &str,
    change: &InventoryChange,
    ctx: &InventoryChangeContext<'_>,
    pending: Option<&PendingInventoryAnnouncements>,
) {
    if change.is_empty() {
        return;
    }
    if let Some(pending) = pending {
        let recorded = {
            let mut map = pending.lock().unwrap_or_else(|e| e.into_inner());
            if ctx.announce && change.given.is_none() {
                record_pending_inventory_change(
                    &mut map,
                    PendingWho {
                        user_id: ctx.user_id,
                        chat_id,
                        character_id,
                    },
                    change,
                    ctx.provenance,
                );
                true
            } else {
                if let Some(given) = &change.given {
                    strike_from_pending_inventory_change(
                        &mut map,
                        &pending_inventory_key(chat_id, character_id),
                        given.item.id(),
                    );
                }
                false
            }
        };
        if recorded {
            tracing::debug!(
                target: "quilltap::inventory",
                chatId = chat_id,
                characterId = character_id,
                provenance = ctx.provenance.as_str(),
                context = CONTEXT,
                "[InventoryEffects] Inventory change joined the turn",
            );
            return;
        }
    }
    if ctx.announce {
        match character_name(db, character_id) {
            Ok(name) => {
                if let Some(content) = describe_inventory_change(&name, change) {
                    post_pascal_inventory_change(db, chat_id, &content).await;
                }
            }
            Err(error) => {
                tracing::warn!(
                    target: "quilltap::inventory",
                    chatId = chat_id,
                    characterId = character_id,
                    context = CONTEXT,
                    error = %crate::db::fallback::error_text(&error),
                    "[InventoryEffects] Inventory change effects failed",
                );
                return;
            }
        }
    }
    refresh_avatar_for_outfit(
        db,
        ctx.user_id,
        chat_id,
        character_id,
        INVENTORY_CHANGE_SOURCE_CONTEXT,
    )
    .await;
    tracing::debug!(
        target: "quilltap::inventory",
        chatId = chat_id,
        characterId = character_id,
        announced = ctx.announce,
        provenance = ctx.provenance.as_str(),
        equippedJson = %labels_json(&change.equipped),
        unequippedJson = %labels_json(&change.unequipped),
        context = CONTEXT,
        "[InventoryEffects] Inventory change effects fired",
    );
}

/// One drained entry's provenances, as the drain's DEBUG reports them.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DrainedProvenances<'a> {
    character_id: &'a str,
    provenances: &'a [InventoryChangeProvenance],
}

/// Drain the turn's pending Inventory changes: one Pascal line and one avatar
/// refresh per character that changed (v4
/// `flushPendingInventoryAnnouncements`). Idempotent: the map is emptied
/// under its lock BEFORE any await. Each entry re-notifies with NO map (so it
/// posts now) under its FIRST provenance (`character` when none). Never
/// fails.
pub async fn flush_pending_inventory_announcements(
    context: Option<&PendingInventoryAnnouncements>,
    db: &Db,
) {
    let Some(pending) = context else {
        return;
    };
    let mut entries: Vec<(String, PendingInventoryChange)> = {
        let mut map = pending.lock().unwrap_or_else(|e| e.into_inner());
        if map.is_empty() {
            return;
        }
        map.drain().collect()
    };
    // R-6: v4's Map is insertion-ordered; drain in key order.
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    for (_, entry) in &entries {
        let change = InventoryChange {
            equipped: entry.equipped.clone(),
            unequipped: entry.unequipped.clone(),
            picked_up: Some(entry.picked_up.clone()),
            given: None,
        };
        let ctx = InventoryChangeContext {
            user_id: &entry.user_id,
            provenance: entry
                .provenances
                .first()
                .copied()
                .unwrap_or(InventoryChangeProvenance::Character),
            announce: true,
            pending: None,
        };
        notify_inventory_changed(db, &entry.chat_id, &entry.character_id, &change, &ctx, None)
            .await;
    }
    let provenances: Vec<DrainedProvenances<'_>> = entries
        .iter()
        .map(|(_, e)| DrainedProvenances {
            character_id: &e.character_id,
            provenances: &e.provenances,
        })
        .collect();
    tracing::debug!(
        target: "quilltap::inventory",
        characters = entries.len(),
        provenancesJson = %serde_json::to_string(&provenances).unwrap_or_default(),
        context = CONTEXT,
        "[InventoryEffects] Drained the turn's Inventory announcements",
    );
}
