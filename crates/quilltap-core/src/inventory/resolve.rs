//! Inventory resolution — what an object *is* once its archetype is filled in
//! (v4 `lib/inventory/resolve.ts`, `4fbdad186`; P4.D324).
//!
//! Inheritance is sparse overrides resolved at read time: an instance's file
//! stores `parentId` plus only the fields it overrides; the resolved object
//! is the archetype's inheritable fields overlaid with the instance's own,
//! `metadata` merged shallowly key by key. Identity, shape, filing and
//! timestamps are always the instance's. A standalone, archetype or kit
//! resolves to itself. An instance whose archetype cannot be found resolves
//! to its own fields with `parentMissing: true`.
//!
//! KEY ORDER is a byte contract (survey T1): v4 builds each result with
//! object spreads and answers it as-is, so every builder here reproduces V8's
//! insertion order on a `preserve_order` map — a re-set key keeps its first
//! position, a new key appends. Pure.

use serde_json::{json, Map, Value};

use super::types::{
    InventoryFields, InventoryKind, InventoryRecord, KitState, ResolvedInventoryItem,
    INVENTORY_INHERITABLE_FIELDS,
};
use crate::jsstr::js_trim;

/// v4's seven defaults, in its literal's order.
fn defaults() -> Map<String, Value> {
    let mut m = Map::new();
    m.insert("kind".into(), json!("item"));
    m.insert("equippedByDefault".into(), json!(false));
    m.insert("maxEquipped".into(), json!(1));
    m.insert("canCopy".into(), json!(false));
    m.insert("isArchetype".into(), json!(false));
    m.insert("itemIds".into(), json!([]));
    m.insert("metadata".into(), json!({}));
    m
}

/// `a ?? b` over map values (`null` and absent both fall through).
fn non_null(v: Option<&Value>) -> Option<&Value> {
    v.filter(|v| !v.is_null())
}

/// Fill the schema defaults into a (possibly sparse) record (v4
/// `withInventoryDefaults`): the seven defaults first, then the record's own
/// keys in FILE order (`fileStem` dropped), then `name` / `parentId` /
/// `folder` re-set in place or appended. Every other key the record carries
/// (`origin`, `mountPointId` from a pool) rides along at its position.
pub fn with_inventory_defaults(record: &InventoryRecord) -> ResolvedInventoryItem {
    let mut out = defaults();
    for (k, v) in &record.0 {
        if k == "fileStem" {
            continue;
        }
        out.insert(k.clone(), v.clone());
    }
    let name = non_null(record.get("name"))
        .cloned()
        .or_else(|| non_null(record.get("fileStem")).cloned())
        .unwrap_or_else(|| json!("Unnamed object"));
    out.insert("name".into(), name);
    let parent = non_null(record.get("parentId"))
        .cloned()
        .unwrap_or(Value::Null);
    out.insert("parentId".into(), parent);
    let folder = non_null(record.get("folder"))
        .cloned()
        .unwrap_or_else(|| json!(""));
    out.insert("folder".into(), folder);
    ResolvedInventoryItem(out)
}

/// The inheritable fields an instance's own file sets (v4
/// `overriddenFieldsOf`) — an explicit `null` COUNTS.
pub fn overridden_fields_of(record: &InventoryRecord) -> Vec<String> {
    INVENTORY_INHERITABLE_FIELDS
        .iter()
        .filter(|field| record.has(field))
        .map(|field| field.to_string())
        .collect()
}

/// Resolve one record (v4 `resolveItem`). `lookup` finds the archetype by id
/// (a pool walk, or an instance-wide parent map). Pure.
pub fn resolve_item<'a>(
    record: &InventoryRecord,
    lookup: impl Fn(&str) -> Option<&'a InventoryRecord>,
) -> ResolvedInventoryItem {
    let origin = record.get("origin").cloned();
    let Some(parent_id) = record.parent_id() else {
        let mut out = with_inventory_defaults(record);
        if let Some(origin) = origin {
            out.set("origin", origin);
        }
        return out;
    };

    let overridden = overridden_fields_of(record);
    let parent = lookup(parent_id);
    let Some(parent) = parent.filter(|p| p.is_archetype()) else {
        let mut out = with_inventory_defaults(record);
        out.set("kind", json!("item"));
        out.set("isArchetype", json!(false));
        out.set("itemIds", json!([]));
        out.set("overriddenFields", json!(overridden));
        out.set("parentMissing", json!(true));
        out.set("parentName", Value::Null);
        if let Some(origin) = origin {
            out.set("origin", origin);
        }
        return out;
    };

    let base = with_inventory_defaults(parent);
    let mut out = Map::new();
    for field in INVENTORY_INHERITABLE_FIELDS {
        // `own[field] !== undefined ? own[field] : base[field]` — an absent
        // base field is JS `undefined`, which the wire drops.
        if let Some(v) = record.get(field).or_else(|| base.get(field)) {
            out.insert(field.to_string(), v.clone());
        }
    }
    let mut metadata = base.metadata();
    if let Some(Value::Object(own)) = record.get("metadata") {
        for (k, v) in own {
            metadata.insert(k.clone(), v.clone());
        }
    }
    out.insert("metadata".into(), Value::Object(metadata));
    out.insert("id".into(), json!(record.id()));
    out.insert("kind".into(), json!("item"));
    out.insert("isArchetype".into(), json!(false));
    out.insert("parentId".into(), json!(parent_id));
    out.insert("itemIds".into(), json!([]));
    out.insert(
        "folder".into(),
        non_null(record.get("folder"))
            .cloned()
            .unwrap_or_else(|| json!("")),
    );
    out.insert(
        "archivedAt".into(),
        non_null(record.get("archivedAt"))
            .cloned()
            .unwrap_or(Value::Null),
    );
    if let Some(v) = record.get("createdAt") {
        out.insert("createdAt".into(), v.clone());
    }
    if let Some(v) = record.get("updatedAt") {
        out.insert("updatedAt".into(), v.clone());
    }
    out.insert("overriddenFields".into(), json!(overridden));
    out.insert("parentMissing".into(), json!(false));
    // `base.title || base.name`.
    out.insert("parentName".into(), json!(base.title_or_name()));
    if let Some(origin) = origin {
        out.insert("origin".into(), origin);
    }
    ResolvedInventoryItem(out)
}

/// The display name (v4 `inventoryDisplayName`): `Excalibur (sword)` when the
/// trimmed title differs from the trimmed name case-insensitively, else the
/// one there is. The name inside the parentheses is NOT trimmed (v4's).
pub fn inventory_display_name(item: &impl InventoryFields) -> String {
    let title = item.title().map(js_trim).unwrap_or("");
    if !title.is_empty() && title.to_lowercase() != js_trim(item.name()).to_lowercase() {
        return format!("{title} ({})", item.name());
    }
    if title.is_empty() {
        item.name().to_string()
    } else {
        title.to_string()
    }
}

/// The short label (v4 `inventoryLabel`): the trimmed title, else the name.
pub fn inventory_label(item: &impl InventoryFields) -> String {
    match item.title().map(js_trim) {
        Some(t) if !t.is_empty() => t.to_string(),
        _ => item.name().to_string(),
    }
}

/// Kin share a resolved name, case-insensitively (v4 `isKin`).
pub fn is_kin(a: &impl InventoryFields, b: &impl InventoryFields) -> bool {
    js_trim(a.name()).to_lowercase() == js_trim(b.name()).to_lowercase()
}

/// An object that can be carried: not an archetype, not a kit, not archived
/// (v4 `isEquippable`).
pub fn is_equippable(item: &impl InventoryFields) -> bool {
    !item.is_archetype() && item.kind() != InventoryKind::Kit && !item.is_archived()
}

/// A kit's state in a chat (v4 `kitState`): how many of its resolvable
/// members are carried. An archetype member counts as carried when an
/// instance of it is; a missing / kit / archived member is not counted.
pub fn kit_state<R: InventoryFields>(
    kit_item_ids: &[String],
    equipped: &[impl InventoryFields],
    resolvable: impl Fn(&str) -> Option<R>,
) -> KitState {
    let ids: std::collections::HashSet<&str> = equipped.iter().map(|i| i.id()).collect();
    let parents: std::collections::HashSet<&str> =
        equipped.iter().filter_map(|i| i.parent_id()).collect();
    let mut total = 0usize;
    let mut carried = 0usize;
    for member_id in kit_item_ids {
        let Some(member) = resolvable(member_id) else {
            continue;
        };
        if member.kind() == InventoryKind::Kit || member.is_archived() {
            continue;
        }
        total += 1;
        let hit = if member.is_archetype() {
            parents.contains(member_id.as_str())
        } else {
            ids.contains(member_id.as_str())
        };
        if hit {
            carried += 1;
        }
    }
    if total == 0 || carried == 0 {
        KitState::None
    } else if carried == total {
        KitState::All
    } else {
        KitState::Some
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(v: Value) -> InventoryRecord {
        InventoryRecord(v.as_object().unwrap().clone())
    }

    #[test]
    fn defaults_key_order() {
        let r =
            rec(json!({"id": "i", "name": "n", "createdAt": "t", "folder": "f", "fileStem": "s"}));
        assert_eq!(
            serde_json::to_string(&with_inventory_defaults(&r)).unwrap(),
            r#"{"kind":"item","equippedByDefault":false,"maxEquipped":1,"canCopy":false,"isArchetype":false,"itemIds":[],"metadata":{},"id":"i","name":"n","createdAt":"t","folder":"f","parentId":null}"#
        );
    }
}
