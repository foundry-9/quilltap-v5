//! Inventory context — the rolling character's bag, loaded ONCE per tool call
//! or per custom-tool run (v4 `lib/inventory/context.ts`, `e1872c94f` +
//! `102a23648`'s roster shelf; P4.D328).
//!
//! The pool (`load_inventory_pool`), its resolved form and the character's
//! equipped set for the chat (`read_equipped_set`), handed to every reader
//! through [`InventoryContext`]. Every reader of this context is the
//! character's own reach (a tool call, a custom tool), so the project roster
//! applies: a closed shelf hides what they are not already carrying
//! (`roster_gate`), and keeps it aside ONLY so a reference to it can be
//! refused in words rather than as "not found".
//!
//! Synchronous over the writer's (or a reader's) two connections — the tool
//! handlers load it inside their write, and `inventory_update` loads it a
//! second time in the SAME closure to report what it wrote. v4's job-child
//! read-your-writes overlay (`child-repositories-proxy.ts`) is NO-PORT: v5
//! has no forked child, and a second load on the writer connection already
//! sees the first gesture.

use std::collections::HashSet;
use std::ops::Deref;

use rusqlite::Connection;
use serde_json::Value;

use super::equipped_set::read_equipped_set;
use super::pool::{load_inventory_pool, InventoryPool, LoadInventoryPoolOptions};
use super::refs::{
    inventory_context_from, AggregateObject, AggregateScope, InventoryContext, InventoryState,
    ResolvedInventoryContext, ResolvedInventoryPool,
};
use super::roster_gate::{is_on_closed_shelf, project_shelf_open_to};
use super::types::{InventoryFields, ResolvedInventoryItem};
use crate::db::chats_read;
use crate::db::DbError;

/// A loaded context, with the server pool it was built from (v4
/// `LoadedInventoryContext`). Derefs to the [`ResolvedInventoryContext`]
/// (its `pool`, `equipped_ids`, `resolve`).
#[derive(Debug)]
pub struct LoadedInventoryContext {
    pub context: ResolvedInventoryContext,
    pub server_pool: InventoryPool,
    /// What the project roster hides from this character: project-shelf
    /// objects they do not carry. Never offered to them.
    pub closed_shelf: Vec<ResolvedInventoryItem>,
}

impl Deref for LoadedInventoryContext {
    type Target = ResolvedInventoryContext;
    fn deref(&self) -> &ResolvedInventoryContext {
        &self.context
    }
}

impl InventoryContext for LoadedInventoryContext {
    fn character_id(&self) -> &str {
        self.context.character_id()
    }
    fn chat_id(&self) -> Option<&str> {
        self.context.chat_id()
    }
    fn state(&self, reference: &str) -> InventoryState {
        self.context.state(reference)
    }
    fn equipped_kin_count(&self, reference: &str) -> usize {
        self.context.equipped_kin_count(reference)
    }
    fn item(&self, reference: &str) -> Option<ResolvedInventoryItem> {
        self.context.item(reference)
    }
    fn scope_objects(&self, scope: AggregateScope) -> &[AggregateObject] {
        self.context.scope_objects(scope)
    }
    fn pool(&self) -> Option<&ResolvedInventoryPool> {
        self.context.pool()
    }
    fn equipped_ids(&self) -> Option<&HashSet<String>> {
        self.context.equipped_ids()
    }
}

/// The holder's own ids, for a resolved pool's `owns` (v4 `pool.owns(id)`).
fn owns_of(pool: &InventoryPool) -> Box<dyn Fn(&str) -> bool + Send + Sync> {
    let own: HashSet<String> = pool.own.iter().map(|r| r.id().to_string()).collect();
    Box::new(move |id| own.contains(id))
}

/// The resolved form of a server pool: every reachable object plus the
/// holder's own ARCHIVED records (not already reachable), resolved and
/// appended (v4 `resolvedPoolOf`).
pub fn resolved_pool_of(pool: &InventoryPool) -> ResolvedInventoryPool {
    let mut items = pool.resolved();
    let seen: HashSet<String> = items.iter().map(|i| i.id().to_string()).collect();
    let archived_own: Vec<ResolvedInventoryItem> = pool
        .own
        .iter()
        .filter(|record| record.is_archived() && !seen.contains(record.id()))
        .map(|record| pool.resolve(record))
        .collect();
    items.extend(archived_own);
    ResolvedInventoryPool {
        items,
        owns: owns_of(pool),
    }
}

/// Load the rolling character's inventory context for a chat — no chat:
/// nothing equipped, no project, the shelf open (v4 `loadInventoryContext`).
/// DEBUG `[InventoryContext] Loaded` with the project shelf's tri-state.
pub fn load_inventory_context(
    main: &Connection,
    mount: &Connection,
    character_id: &str,
    chat_id: Option<&str>,
) -> Result<LoadedInventoryContext, DbError> {
    // `repos.chats.findById` — the fallback read (an error answers null).
    let chat = chat_id.and_then(|id| chats_read::find_by_id_or_none(main, id));
    let project_id: Option<String> = chat
        .as_ref()
        .and_then(|c| c.get("projectId"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let server_pool = load_inventory_pool(
        main,
        mount,
        LoadInventoryPoolOptions {
            character_id,
            chat_id,
            project_id: project_id.as_deref(),
            own_records: None,
        },
    )?;
    let equipped: Vec<ResolvedInventoryItem> = match &chat {
        Some(chat) => read_equipped_set(
            chat.get("id").and_then(Value::as_str),
            chat.get("equippedInventory"),
            character_id,
            &server_pool,
        ),
        None => Vec::new(),
    };
    let equipped_ids: HashSet<String> = equipped.iter().map(|i| i.id().to_string()).collect();
    let full = resolved_pool_of(&server_pool);
    let shelf_open = project_shelf_open_to(main, mount, project_id.as_deref(), character_id);
    let (pool, closed_shelf) = if shelf_open {
        (full, Vec::new())
    } else {
        let (hidden, kept): (Vec<_>, Vec<_>) = full
            .items
            .into_iter()
            .partition(|item| is_on_closed_shelf(item, &equipped_ids));
        (
            ResolvedInventoryPool {
                items: kept,
                owns: full.owns,
            },
            hidden,
        )
    };
    tracing::debug!(
        target: "quilltap::inventory",
        characterId = character_id,
        chatId = chat_id.unwrap_or("null"),
        reachable = pool.items.len(),
        equipped = equipped_ids.len(),
        // v4 `projectId ? (shelfOpen ? 'open' : 'closed by roster') : 'none'`.
        projectShelf = match (project_id.as_deref().filter(|p| !p.is_empty()), shelf_open) {
            (None, _) => "none",
            (Some(_), true) => "open",
            (Some(_), false) => "closed by roster",
        },
        context = "inventory",
        "[InventoryContext] Loaded",
    );
    Ok(LoadedInventoryContext {
        context: inventory_context_from(character_id, chat_id, pool, equipped_ids),
        server_pool,
        closed_shelf,
    })
}
