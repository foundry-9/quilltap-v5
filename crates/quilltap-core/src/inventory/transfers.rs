//! Inventory transfers — move or copy one object between tiers (v4
//! `lib/inventory/transfers.ts`, `4fbdad186` at `322a5488e`; P4.D324, R-M /
//! R-T: the PIN's shape — the route-level refusal moves of the later
//! extraction are P4.D328's).
//!
//! A move keeps the object's id (every chat's carried set and every kit still
//! name it); a copy mints one and is refused when the object is not copyable.
//! A kit travels whole: its members must be reachable from the destination
//! (in context — a project object named by a character's kit), and moving a
//! member away from a kit that could no longer reach it is `KIT_STRANDED`.
//! Pictures are linked at the destination before the object lands, so a
//! landed object's `imageFileId` never dangles. The object is created with
//! the location's direct `create` — a transfer is a move, not a create, so no
//! title lint runs (v4's `preserve` semantics).

use rusqlite::Connection;
use serde_json::{json, Value};

use super::dependents::{
    find_kits_stranded_by_move, find_records_anywhere, is_reachable_from_kit_tier, mount_scope_of,
};
use super::errors::{InventoryError, InventoryRefusal};
use super::folders::normalize_inventory_folder_path_or_root;
use super::item_images::{
    carry_inventory_item_images, commit_moved_inventory_images, CarryInventoryImagesArgs,
};
use super::item_mutations::{assert_copyable, assert_kit_members, MutationMeta};
use super::location::{location_key, InventoryLocation};
use super::pool::{load_pool_for_location, LoadPoolContext};
use super::types::{InventoryFields, InventoryKind, InventoryRecord, ResolvedInventoryItem};
use crate::services::mount_index::blob_transcode::WebpTranscoder;
use crate::services::wardrobe_item_images::carry::CarryMode;

/// v4 `InventoryTransferAction`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InventoryTransferAction {
    Move,
    Copy,
}

impl InventoryTransferAction {
    pub fn as_str(self) -> &'static str {
        match self {
            InventoryTransferAction::Move => "move",
            InventoryTransferAction::Copy => "copy",
        }
    }

    fn carry_mode(self) -> CarryMode {
        match self {
            InventoryTransferAction::Move => CarryMode::Move,
            InventoryTransferAction::Copy => CarryMode::Copy,
        }
    }
}

/// v4 `TransferInventoryItemParams`.
#[derive(Debug, Clone, Copy)]
pub struct TransferInventoryItemParams<'a> {
    pub action: InventoryTransferAction,
    pub source: &'a InventoryLocation,
    pub destination: &'a InventoryLocation,
    pub record: &'a InventoryRecord,
    pub user_id: &'a str,
}

/// What landed, resolved at the destination (v4 `{ stored, item }`).
#[derive(Debug, Clone)]
pub struct TransferredInventoryItem {
    pub stored: InventoryRecord,
    pub item: ResolvedInventoryItem,
}

/// Move or copy one object (v4 `transferInventoryItem`). Answers what landed,
/// resolved at the destination (a fresh pool load); every refusal comes back
/// as `Err`. `blob_webp` is the bridge's normalisation encoder.
pub fn transfer_inventory_item(
    main: &Connection,
    mount: &Connection,
    params: TransferInventoryItemParams<'_>,
    blob_webp: &dyn WebpTranscoder,
) -> Result<TransferredInventoryItem, InventoryError> {
    let TransferInventoryItemParams {
        action,
        source,
        destination,
        record,
        user_id,
    } = params;
    if location_key(source) == location_key(destination) {
        return Err(InventoryRefusal::transfer_same_location().into());
    }

    let op = format!("transfer-{}", action.as_str());
    let meta = MutationMeta::OpItem {
        op: &op,
        item_id: record.id(),
    };
    if action == InventoryTransferAction::Copy {
        let pool = load_pool_for_location(main, mount, source, LoadPoolContext::default())?;
        assert_copyable(&[pool.resolve(record)], meta)?;
    } else {
        // A move writes the source too: an archived source character
        // refuses here, before anything lands at the destination.
        source.writable_mount_point_id(main)?;
    }

    let members = record.item_ids();
    if record.kind() == InventoryKind::Kit && !members.is_empty() {
        // Members the destination's own pool cannot see may still be in
        // reach from its tier in context (a project object, for a
        // character's kit).
        let destination_pool =
            load_pool_for_location(main, mount, destination, LoadPoolContext::default())?;
        let unseen: Vec<String> = members
            .iter()
            .filter(|id| destination_pool.get(id).is_none())
            .cloned()
            .collect();
        let mut contextual: Vec<String> = Vec::new();
        if !unseen.is_empty() {
            for (id, home) in find_records_anywhere(mount, &unseen) {
                let Some(scope) = mount_scope_of(main, mount, &home.mount_point_id)? else {
                    continue;
                };
                if is_reachable_from_kit_tier(
                    main,
                    mount,
                    &destination.mount_point_id,
                    &home.mount_point_id,
                    scope,
                )? {
                    contextual.push(id);
                }
            }
        }
        assert_kit_members(&destination_pool, &members, meta, &contextual)?;
    }
    if action == InventoryTransferAction::Move && record.kind() != InventoryKind::Kit {
        let stranded: Vec<_> = find_kits_stranded_by_move(
            main,
            mount,
            record.id(),
            &destination.mount_point_id,
            destination.scope,
        )?
        .into_iter()
        .filter(|kit| kit.id != record.id())
        .collect();
        if !stranded.is_empty() {
            let kits: Vec<&str> = stranded.iter().map(|k| k.id.as_str()).collect();
            tracing::info!(
                op = %op,
                itemId = %record.id(),
                kitsJson = %serde_json::to_string(&kits).unwrap_or_default(),
                context = "inventory",
                "[InventoryTransfers] Move refused: it would strand kits"
            );
            return Err(InventoryRefusal::kit_stranded(record.id(), &stranded).into());
        }
    }

    let now = crate::clock::now_iso();
    let copy = action == InventoryTransferAction::Copy;
    let mut planned = record.clone();
    planned.set(
        "id",
        json!(if copy {
            uuid::Uuid::new_v4().to_string()
        } else {
            record.id().to_string()
        }),
    );
    planned.set(
        "folder",
        json!(normalize_inventory_folder_path_or_root(
            record.str_of("folder")
        )),
    );
    planned.set(
        "archivedAt",
        if copy {
            Value::Null
        } else {
            record
                .get("archivedAt")
                .filter(|v| !v.is_null())
                .cloned()
                .unwrap_or(Value::Null)
        },
    );
    let stamp = |key: &str| -> Value {
        if copy {
            json!(now)
        } else {
            record.get(key).cloned().unwrap_or(Value::Null)
        }
    };
    let created_at = stamp("createdAt");
    let updated_at = stamp("updatedAt");
    planned.set("createdAt", created_at);
    planned.set("updatedAt", updated_at);
    let planned_id = planned.id().to_string();
    if destination
        .read_items(mount, true)?
        .iter()
        .any(|existing| existing.id() == planned_id)
    {
        let mut details = serde_json::Map::new();
        details.insert("itemId".into(), json!(planned_id));
        return Err(InventoryRefusal::shape(
            "An object with that id already exists at the destination",
            details,
        )
        .into());
    }

    // Pictures are linked at the destination before the object lands, so a
    // landed object's `imageFileId` never dangles.
    let destination_mount_point_id = destination.writable_mount_point_id(main)?;
    let mut mint_file_id = || uuid::Uuid::new_v4().to_string();
    let carried = carry_inventory_item_images(
        main,
        mount,
        CarryInventoryImagesArgs {
            mode: action.carry_mode(),
            source_item_id: record.id(),
            destination_item_id: &planned_id,
            destination_mount_point_id: &destination_mount_point_id,
            user_id,
        },
        &mut mint_file_id,
        &now,
        blob_webp,
    )?;
    // `record.imageFileId ? fileIdMap.get(it) ?? null : record.imageFileId` —
    // a falsy pointer is set back as it was (an absent key stays absent).
    match record.get("imageFileId") {
        Some(Value::String(file_id)) if !file_id.is_empty() => {
            planned.set(
                "imageFileId",
                carried.mapped(file_id).map_or(Value::Null, |f| json!(f)),
            );
        }
        _ => {}
    }

    let stored = destination.create(main, mount, &planned)?;
    if action == InventoryTransferAction::Move {
        let removed = source.delete(main, mount, &[record.id().to_string()], |_| Vec::new())?;
        if removed.is_empty() {
            return Err(InventoryError::Other(
                "Failed to remove the object from its source after the move".to_string(),
            ));
        }
        commit_moved_inventory_images(main, mount, record.id(), &carried.pending_move, &now);
    }

    tracing::info!(
        userId = %user_id,
        action = action.as_str(),
        itemId = %record.id(),
        resultItemId = %stored.id(),
        from = source.scope.as_str(),
        to = destination.scope.as_str(),
        folder = %stored.folder(),
        context = "inventory",
        "[InventoryTransfers] Inventory object transferred"
    );
    let pool = load_pool_for_location(main, mount, destination, LoadPoolContext::default())?;
    let mut tagged = stored.clone();
    tagged.set("origin", destination.origin.to_value());
    tagged.set("mountPointId", json!(destination.mount_point_id));
    let item = pool.resolve(&tagged);
    Ok(TransferredInventoryItem { stored, item })
}
