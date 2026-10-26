//! Quilltap General's Inventory routes + the transfers verb (P4.D325).
//!
//! - `app/api/v1/inventory/route.ts` + `[itemId]/route.ts` — a config and
//!   delegates over the factory ([`GENERAL_INVENTORY`]).
//! - `app/api/v1/inventory/transfers/route.ts` — `POST { action: 'move' |
//!   'copy', itemId, from: { scope, id }, to: { scope, id } }` moves or copies
//!   one object between tiers. The rules (id kept on a move, `canCopy` on a
//!   copy, kits kept whole, pictures carried) live in the keystone's
//!   `crate::inventory::transfers::transfer_inventory_item`.

use std::sync::Arc;

use serde_json::{json, Value};

use super::inventory_route_factory::{
    escaped, not_found, validation_error, Fail, InventoryRouteConfig,
};
use super::types::{ErrorKind, Response};
use crate::db::runtime::Db;
use crate::inventory::errors::InventoryError;
use crate::inventory::location::{resolve_inventory_location, ResolveInventoryLocationOptions};
use crate::inventory::transfers::{
    transfer_inventory_item, InventoryTransferAction, TransferInventoryItemParams,
};
use crate::inventory::types::{parse_inventory_transfer_body, InventoryScope};
use crate::services::image_job_common::with_both_conns;
use crate::services::mount_index::blob_transcode::WebpTranscoder;

/// `inventory` (Quilltap General) — `{ scope: 'general', logTag: '[Inventory
/// General v1]', logIdKey: 'generalId' }`.
pub const GENERAL_INVENTORY: InventoryRouteConfig = InventoryRouteConfig {
    scope: InventoryScope::General,
    log_tag: "[Inventory General v1]",
    log_id_key: "generalId",
};

/// `POST /api/v1/inventory/transfers` with the store-original refusal for the
/// carried pictures' blob links (no codec wired).
pub async fn inventory_transfer_apply(db: &Db, user_id: &str, body: Value) -> Response {
    inventory_transfer_apply_with_codec(
        db,
        user_id,
        body,
        Arc::new(crate::services::mount_index::blob_transcode::RefusingWebpTranscoder),
    )
    .await
}

/// `POST /api/v1/inventory/transfers` → `{ item }`, in v4's order: the body
/// parse (the Zod 400 — the scope enum in `InventoryScopeSchema` ORDER) →
/// the source (never provisioned) and the object in it (404 `Inventory object
/// not found`) → the destination (`ensure: true`; none → 400 `Invalid
/// destination`) → the transfer. Everything after the parse is inside v4's
/// try: a refusal (or an archived character) maps through
/// [`inventory_error_response`].
pub async fn inventory_transfer_apply_with_codec(
    db: &Db,
    user_id: &str,
    body: Value,
    blob_webp: Arc<dyn WebpTranscoder>,
) -> Response {
    let body = match parse_inventory_transfer_body(&body) {
        Ok(b) => b,
        Err(issues) => return validation_error(&issues),
    };
    let side = |key: &str| -> (InventoryScope, Option<String>) {
        let side = body.get(key);
        let scope = side
            .and_then(|s| s.get("scope"))
            .and_then(Value::as_str)
            .and_then(InventoryScope::parse)
            .unwrap_or(InventoryScope::General);
        let id = side
            .and_then(|s| s.get("id"))
            .and_then(Value::as_str)
            .map(str::to_string);
        (scope, id)
    };
    let (from_scope, from_id) = side("from");
    let (to_scope, to_id) = side("to");
    let action = match body.get("action").and_then(Value::as_str) {
        Some("copy") => InventoryTransferAction::Copy,
        _ => InventoryTransferAction::Move,
    };
    let item_id = body
        .get("itemId")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let uid = user_id.to_string();
    let out = with_both_conns(db, move |main, mount| {
        let attempt = || -> Result<Result<Value, Response>, InventoryError> {
            let source = resolve_inventory_location(
                main,
                mount,
                from_scope,
                from_id.as_deref(),
                &uid,
                ResolveInventoryLocationOptions { ensure: false },
            )?;
            let record = match &source {
                Some(source) => source.find_item(mount, &item_id)?,
                None => None,
            };
            let (Some(source), Some(record)) = (source, record) else {
                return Ok(Err(not_found("Inventory object")));
            };
            let Some(destination) = resolve_inventory_location(
                main,
                mount,
                to_scope,
                to_id.as_deref(),
                &uid,
                ResolveInventoryLocationOptions { ensure: true },
            )?
            else {
                return Ok(Err(Response::error(
                    ErrorKind::BadRequest,
                    "Invalid destination",
                )));
            };
            let moved = transfer_inventory_item(
                main,
                mount,
                TransferInventoryItemParams {
                    action,
                    source: &source,
                    destination: &destination,
                    record: &record,
                    user_id: &uid,
                },
                blob_webp.as_ref(),
            )?;
            Ok(Ok(json!({ "item": moved.item.to_value() })))
        };
        // A refusal leaves the writer closure unmapped: its archived-character
        // INFO logs on the calling thread (`Fail::answer`).
        Ok(match attempt() {
            Ok(Ok(body)) => Fail::from(Response::Inventory(body)),
            Ok(Err(r)) => Fail::from(r),
            Err(e) => Fail::Refused(e),
        })
    })
    .await;
    match out {
        Ok(f) => f.answer(),
        Err(e) => escaped(e),
    }
}
