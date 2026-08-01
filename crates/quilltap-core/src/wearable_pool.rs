//! Wearable pool merge — v4 `lib/wardrobe/wearable-pool.ts`.
//!
//! The one rule for folding the shared wardrobe tiers (Quilltap General +
//! project stores) together with a character's own vault into the single pool
//! of items that character can wear.
//!
//! Pure — no I/O. Callers fetch the tiers; this decides who wins.

use serde_json::Value;

/// v4's `!item.archivedAt` — an item is archived when `archivedAt` is *truthy*:
/// a non-empty string, `true`, or a non-zero number. `null`, absent, `false`,
/// `0` and `""` all read as active.
///
/// The canonical copy: [`crate::db::wardrobe_read`] and
/// [`crate::db::archetype_wardrobe`] both filter with it, and so does
/// [`merge_wearable_pool`], so the three tiers agree on what "archived" means.
pub fn is_archived_truthy(item: &Value) -> bool {
    match item.get("archivedAt") {
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|f| f != 0.0),
        _ => false,
    }
}

/// v4 `mergeWearablePool(shared, own)` — merge the shared tiers under a
/// character's own wardrobe.
///
/// Precedence is character > shared: a personal item with the same id as a
/// shared one shadows it entirely (that's how a character keeps a private
/// variant of a house garment, and how they opt *out* of a shared default by
/// holding a copy with `isDefault: false`).
///
/// Archived items are dropped from the result — including archived personal
/// overrides, which is what lets the shared item resurface once a character
/// archives their own copy. `wardrobe_list` has always behaved this way.
///
/// **Callers who need archived items (the equip path, which wants an archived
/// item's `types` for display) must not use this helper.**
///
/// Order is v4's insertion-ordered `Map`: shared items in read order, then
/// own-only items appended — an own item that shadows a shared one replaces the
/// value *at the shared item's position*, it does not move to the end.
pub fn merge_wearable_pool(shared: &[Value], own: &[Value]) -> Vec<Value> {
    let mut order: Vec<Value> = Vec::with_capacity(shared.len() + own.len());
    let mut index_by_id: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();

    let mut upsert = |item: &Value| {
        let id = item
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        if let Some(&i) = index_by_id.get(&id) {
            order[i] = item.clone();
        } else {
            index_by_id.insert(id, order.len());
            order.push(item.clone());
        }
    };

    for item in shared {
        upsert(item);
    }
    // Character overrides shared.
    for item in own {
        upsert(item);
    }

    order.retain(|item| !is_archived_truthy(item));
    order
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn item(id: &str, extra: Value) -> Value {
        let mut v = json!({ "id": id, "title": id, "types": ["top"], "archivedAt": null });
        if let (Some(obj), Some(ex)) = (v.as_object_mut(), extra.as_object()) {
            for (k, val) in ex {
                obj.insert(k.clone(), val.clone());
            }
        }
        v
    }

    fn ids(items: &[Value]) -> Vec<&str> {
        items
            .iter()
            .filter_map(|i| i.get("id").and_then(Value::as_str))
            .collect()
    }

    /// v4 pool test: "applies precedence character > project > general".
    /// (The shared list arrives already merged project-over-general by
    /// `find_archetypes`; this helper only settles character vs shared.)
    #[test]
    fn character_shadows_shared_in_place() {
        let shared = vec![
            item("livery", json!({ "title": "shared livery" })),
            item("hat", json!({})),
        ];
        let own = vec![
            item("livery", json!({ "title": "own livery" })),
            item("boots", json!({})),
        ];
        let pool = merge_wearable_pool(&shared, &own);
        // The shadowing item keeps the SHARED item's position (v4's `Map.set`).
        assert_eq!(ids(&pool), vec!["livery", "hat", "boots"]);
        assert_eq!(
            pool[0].get("title").and_then(Value::as_str),
            Some("own livery")
        );
    }

    /// v4 pool test: "drops archived shared items from the pool".
    #[test]
    fn drops_archived_shared_items() {
        let shared = vec![
            item("cloak", json!({ "archivedAt": "2026-02-02T00:00:00.000Z" })),
            item("coat", json!({})),
        ];
        let pool = merge_wearable_pool(&shared, &[]);
        assert_eq!(ids(&pool), vec!["coat"]);
    }

    /// v4 pool test: "lets a shared item resurface when the character archives
    /// their own copy" — the archived filter runs LAST, so an archived personal
    /// override removes the merged entry entirely rather than unmasking the
    /// shared row. (v4's own assertion: the id is absent from the pool.)
    #[test]
    fn an_archived_personal_override_removes_the_entry() {
        let shared = vec![item("livery", json!({ "title": "shared" }))];
        let own = vec![item(
            "livery",
            json!({ "title": "own", "archivedAt": "2026-02-02T00:00:00.000Z" }),
        )];
        let pool = merge_wearable_pool(&shared, &own);
        assert!(pool.is_empty());
    }

    /// The opt-out: a personal `isDefault: false` copy shadows the shared
    /// default. Only reachable because the merge sees the FULL pools — filtering
    /// `isDefault` first would leave the personal copy out and nothing to shadow
    /// with.
    #[test]
    fn a_personal_is_default_false_copy_shadows_the_shared_default() {
        let shared = vec![item("livery", json!({ "isDefault": true }))];
        let own = vec![item("livery", json!({ "isDefault": false }))];
        let pool = merge_wearable_pool(&shared, &own);
        assert_eq!(pool.len(), 1);
        assert_eq!(pool[0].get("isDefault"), Some(&Value::Bool(false)));
    }

    /// `archivedAt` truthiness matches v4's falsy check: `null`, absent and `""`
    /// are all active.
    #[test]
    fn empty_string_archived_at_is_active() {
        let shared = vec![
            item("a", json!({ "archivedAt": "" })),
            item("b", json!({ "archivedAt": Value::Null })),
        ];
        let mut bare = json!({ "id": "c" });
        bare.as_object_mut().unwrap();
        let pool = merge_wearable_pool(&shared, &[bare]);
        assert_eq!(ids(&pool), vec!["a", "b", "c"]);
    }
}
