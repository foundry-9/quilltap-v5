//! The chat's Inventory actions — v4 `app/api/v1/chats/[id]/actions/
//! inventory.ts` (`4fbdad186` at the `322a5488e` pin; P4.D325):
//!
//!   - `GET  ?action=inventory` — what every cast member carries, resolved,
//!     with each reachable kit's state (and the chat's `projectId`)
//!   - `POST ?action=inventory-equip` / `inventory-unequip` — `{ characterId,
//!     ids }`: take up / set down objects or kits
//!   - `POST ?action=inventory-reset-defaults` — `{ characterId }`: back to the
//!     equipped-by-default set
//!
//! The operator is changing what a character carries: every gesture goes
//! through the keystone's `equip_items` / `unequip_items`
//! (`crate::inventory::equip`), which plan before they write and refuse a cap
//! breach whole. Refusals answer through [`inventory_error_response`].
//!
//! **The effects are a SEAM (R-14).** C1's gestures RETURN their change and
//! never notify; v4's route fires `notifyInventoryChanged` (Pascal's line +
//! the avatar refresh) after the write, before the response. That function is
//! P4.D328's (`services::inventory_change_effects`), so the handlers take an
//! [`InventoryChangeEffects`] and the engine passes
//! [`ProductionInventoryEffects`] — the one `notify_inventory_changed` call
//! (wired at the `322a5488e` union). The reset needs no seam: C1's
//! `reset_inventory_to_defaults` already runs v4's `announce: false` refresh.

use std::future::Future;
use std::pin::Pin;

use serde_json::{json, Map, Value};

use super::inventory_route_factory::{
    escaped, inventory_error_response, not_found, validation_error,
};
use super::types::Response;
use crate::db::runtime::Db;
use crate::inventory::apply_defaults::reset_inventory_to_defaults;
use crate::inventory::equip::{
    equip_items, unequip_items, EquipChat, EquipOptions, InventoryChange, InventoryChangeContext,
    InventoryChangeProvenance,
};
use crate::inventory::equipped_set::read_equipped_set;
use crate::inventory::errors::InventoryRefusal;
use crate::inventory::pool::load_cast_inventory_pools;
use crate::inventory::resolve::kit_state;
use crate::inventory::types::{
    parse_chat_inventory_equip_body, parse_chat_inventory_reset_body, InventoryFields,
    InventoryKind,
};

/// v4 `OPERATOR(userId)` — `{ userId, provenance: 'operator', announce: true }`.
#[allow(non_snake_case)]
pub fn OPERATOR(user_id: &str) -> InventoryChangeContext<'_> {
    InventoryChangeContext {
        user_id,
        provenance: InventoryChangeProvenance::Operator,
        announce: true,
        pending: None,
    }
}

/// The after-write effects of an operator gesture (v4 `notifyInventoryChanged`
/// inside `equipItems` / `unequipItems`). Never fails — v4's never throws.
pub trait InventoryChangeEffects: Send + Sync {
    fn fire<'a>(
        &'a self,
        db: &'a Db,
        chat: &'a EquipChat,
        character_id: &'a str,
        change: InventoryChange,
        ctx: &'a InventoryChangeContext<'a>,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>>;
}

/// The production effects: v4's `notifyInventoryChanged` — Pascal's
/// `inventory-change` line and the per-chat avatar refresh (💸 when the chat's
/// avatar auto-update is on: a real image job, as in v4). An operator gesture
/// holds no turn, so there is no pending map.
pub struct ProductionInventoryEffects;

impl InventoryChangeEffects for ProductionInventoryEffects {
    fn fire<'a>(
        &'a self,
        db: &'a Db,
        chat: &'a EquipChat,
        character_id: &'a str,
        change: InventoryChange,
        ctx: &'a InventoryChangeContext<'a>,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        Box::pin(async move {
            crate::services::inventory_change_effects::notify_inventory_changed(
                db,
                &chat.id,
                character_id,
                &change,
                ctx,
                None,
            )
            .await;
        })
    }
}

/// v4 `castOf(chat)` — the cast whose Inventory the chat shows: every seated
/// CHARACTER, unique, in participant order. (NB the keystone's
/// `assert_participant` ignores `type` — v4's two checks, as written.)
pub fn cast_of(chat: &EquipChat) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for p in &chat.participants {
        let Some(character_id) = p.character_id.as_deref().filter(|s| !s.is_empty()) else {
            continue;
        };
        if p.kind.as_deref() == Some("CHARACTER")
            && p.status.as_deref() != Some("removed")
            && !out.iter().any(|c| c == character_id)
        {
            out.push(character_id.to_string());
        }
    }
    out
}

/// The chat as the gestures see it (`repos.chats.findById`), or `None`.
fn chat_of(db: &Db, chat_id: &str) -> Result<Option<EquipChat>, crate::db::DbError> {
    db.read_main(|main| {
        Ok(crate::db::chats_read::find_by_id_or_none(main, chat_id)
            .map(|row| EquipChat::from_chat_row(&row)))
    })
}

/// `GET ?action=inventory` → `{ inventory: { <characterId>: { equipped, kits } },
/// projectId }` — cast order; `kits` keyed by kit id in the pool's resolved
/// order. A cast member whose pool did not load is skipped; one failed load
/// fails the whole read (v4's `Promise.all` — the middleware 500).
pub fn chat_inventory_get(db: &Db, chat_id: &str) -> Response {
    let chat = match chat_of(db, chat_id) {
        Ok(Some(chat)) => chat,
        Ok(None) => return not_found("Chat"),
        Err(e) => return escaped(e),
    };
    let cast = cast_of(&chat);
    let built = db.read_main(|main| {
        db.read_mount_index(|mount| {
            let pools = load_cast_inventory_pools(
                main,
                mount,
                &chat.id,
                chat.project_id.as_deref(),
                &cast,
            )?;
            let mut inventory = Map::new();
            let mut carried = 0usize;
            for character_id in &cast {
                let Some((_, pool)) = pools.iter().find(|(k, _)| k == character_id) else {
                    continue;
                };
                let equipped = read_equipped_set(
                    Some(&chat.id),
                    chat.equipped_inventory.as_ref(),
                    character_id,
                    pool,
                );
                let mut kits = Map::new();
                for item in pool.resolved() {
                    if item.kind() == InventoryKind::Kit {
                        let state =
                            kit_state(&item.item_ids(), &equipped, |id| pool.resolve_id(id));
                        kits.insert(item.id().to_string(), json!(state.as_str()));
                    }
                }
                carried += equipped.len();
                inventory.insert(
                    character_id.clone(),
                    json!({
                        "equipped": equipped.iter().map(|i| i.to_value()).collect::<Vec<_>>(),
                        "kits": kits,
                    }),
                );
            }
            Ok((inventory, carried))
        })
    });
    match built {
        Ok((inventory, carried)) => {
            tracing::debug!(
                chatId = %chat_id,
                characters = cast.len(),
                carried = carried,
                context = "inventory",
                "[Chats v1] Read the cast inventory"
            );
            Response::Inventory(json!({ "inventory": inventory, "projectId": chat.project_id }))
        }
        Err(e) => escaped(e),
    }
}

/// Which way a gesture goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EquipMode {
    Equip,
    Unequip,
}

impl EquipMode {
    fn as_str(self) -> &'static str {
        match self {
            EquipMode::Equip => "equip",
            EquipMode::Unequip => "unequip",
        }
    }
}

/// `POST ?action=inventory-equip` / `inventory-unequip` → the raw
/// `EquipResult` / `UnequipResult`: parse (the Zod 400) → the chat (404) →
/// the gesture (refusals through the mapper) → the effects → INFO.
pub async fn chat_inventory_equip(
    db: &Db,
    user_id: &str,
    chat_id: &str,
    body: &Value,
    mode: EquipMode,
    effects: &dyn InventoryChangeEffects,
) -> Response {
    let body = match parse_chat_inventory_equip_body(body) {
        Ok(b) => b,
        Err(issues) => return validation_error(&issues),
    };
    let character_id = body
        .get("characterId")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let ids: Vec<String> = body
        .get("ids")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    let chat = match chat_of(db, chat_id) {
        Ok(Some(chat)) => chat,
        Ok(None) => return not_found("Chat"),
        Err(e) => return escaped(e),
    };
    let ctx = OPERATOR(user_id);
    let outcome = match mode {
        EquipMode::Equip => equip_items(
            db,
            &chat,
            &character_id,
            &ids,
            &ctx,
            EquipOptions::default(),
        )
        .await
        .map(|(result, change)| (serde_json::to_value(result).unwrap_or(Value::Null), change)),
        EquipMode::Unequip => unequip_items(db, &chat, &character_id, &ids)
            .await
            .map(|(result, change)| (serde_json::to_value(result).unwrap_or(Value::Null), change)),
    };
    match outcome {
        Ok((result, change)) => {
            if let Some(change) = change {
                effects.fire(db, &chat, &character_id, change, &ctx).await;
            }
            tracing::info!(
                chatId = %chat_id,
                characterId = %character_id,
                idsJson = %serde_json::to_string(&ids).unwrap_or_default(),
                context = "inventory",
                "[Chats v1] Inventory {}",
                mode.as_str()
            );
            Response::Inventory(result)
        }
        Err(e) => inventory_error_response(e),
    }
}

/// `POST ?action=inventory-reset-defaults` → `{ equipped }` (the sorted,
/// de-duplicated stored ids): parse → the chat (404) → the cast gate
/// (`NOT_A_PARTICIPANT` through the mapper) → the reset. v4 calls the reset
/// OUTSIDE any try, so a failure there is the middleware's (500 / 503).
pub async fn chat_inventory_reset_defaults(
    db: &Db,
    user_id: &str,
    chat_id: &str,
    body: &Value,
) -> Response {
    let body = match parse_chat_inventory_reset_body(body) {
        Ok(b) => b,
        Err(issues) => return validation_error(&issues),
    };
    let character_id = body
        .get("characterId")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let chat = match chat_of(db, chat_id) {
        Ok(Some(chat)) => chat,
        Ok(None) => return not_found("Chat"),
        Err(e) => return escaped(e),
    };
    if !cast_of(&chat).contains(&character_id) {
        return inventory_error_response(InventoryRefusal::not_a_participant(&character_id).into());
    }
    match reset_inventory_to_defaults(db, &chat, &character_id, user_id).await {
        Ok(equipped) => {
            tracing::info!(
                chatId = %chat_id,
                characterId = %character_id,
                count = equipped.len(),
                context = "inventory",
                "[Chats v1] Inventory reset to defaults"
            );
            Response::Inventory(json!({ "equipped": equipped }))
        }
        Err(crate::inventory::errors::InventoryError::Db(e)) => escaped(e),
        Err(_) => super::inventory_route_factory::internal_server_error(),
    }
}
