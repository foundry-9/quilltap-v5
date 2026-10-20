//! Wearable references (v4 `lib/wardrobe/wearable-ref.ts`, `5cb0e0190`) — the
//! one lookup rule for a string that names a garment or outfit.
//!
//! A wearable reference (a Pascal wardrobe effect's `items[]` entry, a
//! `wardrobe_*` tool's title argument) is an id or a case-insensitive title. The
//! rule is id first, then title, in the pool's own precedence order — which the
//! caller has already applied.
//!
//! v4's server `WearablePool.findByTitle` delegates its title half here; v5's
//! [`crate::wearable_pool::WearablePool::find_by_title`] already implements the
//! same trim / lower-case / own-first rule behaviour-for-behaviour, so it is not
//! moved (P4.D319 R-5). This module is the one Rust statement of the rule, for
//! the SPA twin's oracle and any later server caller; it is differentially
//! pinned by `pascal_bench_wardrobe_equivalence`'s `wearable_ref` rows.

use serde_json::Value;

use crate::jsstr::js_trim;
use crate::pascal::placeholders::scan_placeholders;

/// v4 `RefTarget = Pick<WardrobeItem, 'id' | 'title'>` — anything a reference
/// can name.
pub trait RefTarget {
    fn ref_id(&self) -> &str;
    fn ref_title(&self) -> &str;
}

impl RefTarget for Value {
    fn ref_id(&self) -> &str {
        self.get("id").and_then(Value::as_str).unwrap_or_default()
    }
    fn ref_title(&self) -> &str {
        self.get("title")
            .and_then(Value::as_str)
            .unwrap_or_default()
    }
}

/// A borrowed target answers for its referent, so a filtered `Vec<&Value>`
/// (the pool's shared half minus the character's own ids) matches like a slice
/// of owned items.
impl<T: RefTarget> RefTarget for &T {
    fn ref_id(&self) -> &str {
        (**self).ref_id()
    }
    fn ref_title(&self) -> &str {
        (**self).ref_title()
    }
}

/// v4 `matchWearableTitle(pool, title)` — the first item in `pool` whose title
/// matches `title`, the WANTED title trimmed (JS's trim) and lower-cased, each
/// item's title lower-cased only. A blank title names nothing.
pub fn match_wearable_title<'p, T: RefTarget>(pool: &'p [T], title: &str) -> Option<&'p T> {
    let wanted = js_trim(title).to_lowercase();
    if wanted.is_empty() {
        return None;
    }
    pool.iter()
        .find(|item| item.ref_title().to_lowercase() == wanted)
}

/// v4 `isTemplatedWearableRef(ref)` — true when a reference carries a `{{…}}`
/// placeholder and so is chosen at run time.
pub fn is_templated_wearable_ref(r: &str) -> bool {
    !scan_placeholders(r).is_empty()
}

/// v4 `resolveWearableRef(pool, ref)` — id first, then case-insensitive title,
/// the pool's own-items-first order already applied by the caller. A blank or
/// templated reference never resolves — its garment is chosen when the tool
/// runs.
pub fn resolve_wearable_ref<'p, T: RefTarget>(pool: &'p [T], r: &str) -> Option<&'p T> {
    let trimmed = js_trim(r);
    if trimmed.is_empty() || is_templated_wearable_ref(trimmed) {
        return None;
    }
    pool.iter()
        .find(|item| item.ref_id() == trimmed)
        .or_else(|| match_wearable_title(pool, trimmed))
}

#[cfg(test)]
mod tests {
    //! v4's seven `wearable-ref.test.ts` `it`s, ported over v4's own pool.
    use super::*;
    use serde_json::json;

    fn pool() -> Vec<Value> {
        vec![
            json!({ "id": "a", "title": "Cloak of Night" }),
            json!({ "id": "b", "title": "cloak of night" }),
            json!({ "id": "c", "title": "b" }),
            json!({ "id": "d", "title": "Linen Shirt" }),
        ]
    }

    fn id_of(v: Option<&Value>) -> Option<&str> {
        v.map(|v| v.ref_id())
    }

    #[test]
    fn prefers_an_id_over_a_title_that_happens_to_match() {
        // "b" is item c's title and item b's id: the id wins.
        assert_eq!(id_of(resolve_wearable_ref(&pool(), "b")), Some("b"));
    }

    #[test]
    fn matches_a_title_case_insensitively_trimmed() {
        assert_eq!(
            id_of(resolve_wearable_ref(&pool(), "  LINEN shirt ")),
            Some("d")
        );
    }

    #[test]
    fn takes_the_first_title_match_in_pool_order() {
        assert_eq!(
            id_of(resolve_wearable_ref(&pool(), "CLOAK OF NIGHT")),
            Some("a")
        );
    }

    #[test]
    fn never_resolves_a_templated_reference() {
        assert_eq!(
            id_of(resolve_wearable_ref(&pool(), "{{params.garment}}")),
            None
        );
        let templated = vec![json!({ "id": "x", "title": "{{params.garment}}" })];
        assert_eq!(
            id_of(resolve_wearable_ref(&templated, "{{params.garment}}")),
            None
        );
    }

    #[test]
    fn resolves_nothing_for_a_blank_or_unknown_reference() {
        assert_eq!(id_of(resolve_wearable_ref(&pool(), "   ")), None);
        assert_eq!(id_of(resolve_wearable_ref(&pool(), "Bowler Hat")), None);
    }

    #[test]
    fn match_wearable_title_ignores_ids() {
        assert_eq!(id_of(match_wearable_title(&pool(), "a")), None);
    }

    #[test]
    fn is_templated_wearable_ref_spots_a_placeholder_anywhere_in_the_string() {
        assert!(is_templated_wearable_ref("{{params.garment}}"));
        assert!(is_templated_wearable_ref("The {{metadata.hat}} hat"));
        assert!(!is_templated_wearable_ref("Cloak of Night"));
        assert!(!is_templated_wearable_ref("{ not a template }"));
    }
}

#[cfg(test)]
mod find_by_title_regression_tests {
    //! v4's three `wearable-ref-delegation.test.ts` `it`s, as REGRESSION pins on
    //! v5's `find_in_pool` / `WearablePool::find_by_title` — the rule is not
    //! moved (v4's delegation is behaviour-neutral, P4.D319 R-5), so these say
    //! the server's title lookup and [`super::match_wearable_title`] agree.
    use crate::wear_ops::{find_in_pool, WearableRef};
    use crate::wearable_pool::build_wearable_pool;
    use serde_json::{json, Value};

    const STAMP: &str = "2026-01-01T00:00:00.000Z";

    fn item(
        id: &str,
        title: &str,
        character_id: Option<&str>,
        archived: bool,
        origin: Value,
    ) -> Value {
        json!({
            "id": id, "characterId": character_id, "title": title, "types": ["top"],
            "componentItemIds": [], "isDefault": false, "replace": false,
            "archivedAt": if archived { json!(STAMP) } else { Value::Null },
            "createdAt": STAMP, "updatedAt": STAMP, "origin": origin,
        })
    }

    fn pool() -> crate::wearable_pool::WearablePool {
        let own = json!({ "scope": "character", "id": "char-1", "name": "Imogen" });
        let general = json!({ "scope": "general", "id": null, "name": "Quilltap General" });
        build_wearable_pool(
            "char-1",
            Default::default(),
            vec![item("own-coat", "Old Coat", Some("char-1"), true, own)],
            vec![],
            vec![],
            vec![
                item("gen-coat", "Old Coat", None, false, general.clone()),
                item("gen-hat", "Bowler Hat", None, false, general),
            ],
        )
    }

    fn id_of(v: Option<&Value>) -> Option<&str> {
        v.and_then(|v| v.get("id")).and_then(Value::as_str)
    }

    #[test]
    fn the_title_half_matches_case_insensitively() {
        let p = pool();
        let found = find_in_pool(
            &p,
            WearableRef {
                item_id: None,
                item_title: Some("bowler hat"),
            },
        );
        assert_eq!(id_of(found), Some("gen-hat"));
        assert_eq!(
            id_of(super::match_wearable_title(p.wearable(), "bowler hat")),
            Some("gen-hat")
        );
    }

    #[test]
    fn keeps_own_items_first_archived_included() {
        let p = pool();
        let found = find_in_pool(
            &p,
            WearableRef {
                item_id: None,
                item_title: Some("OLD COAT"),
            },
        );
        assert_eq!(id_of(found), Some("own-coat"));
    }

    #[test]
    fn finds_by_id_without_the_title_rule() {
        let p = pool();
        let found = find_in_pool(
            &p,
            WearableRef {
                item_id: Some("gen-coat"),
                item_title: None,
            },
        );
        assert_eq!(id_of(found), Some("gen-coat"));
    }
}
