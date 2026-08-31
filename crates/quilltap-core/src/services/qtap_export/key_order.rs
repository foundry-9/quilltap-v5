//! v4's Zod **schema declaration key order** for every entity the `.qtap`
//! export emits, and the reorder applied at the export boundary.
//!
//! ## Why the export reorders and the read paths don't
//!
//! v4 emits `schema.parse(row)`, and Zod's object parse walks the shape in
//! declaration order — so every `.qtap` line's key order is the SCHEMA's. v5's
//! read paths marshal in COLUMN order (see `db/characters_read.rs`'s header:
//! every other differential in the port compares over key-order-independent
//! `Value`s, so it never mattered). Reordering those landed read paths now would
//! be a wide, unrelated change; instead the writer reorders here, at the one
//! surface where the byte order is part of the artifact.
//!
//! [`reorder`] is **non-lossy**: a key absent from the template is kept, in its
//! original relative position after the templated ones. A future v4 column that
//! this table has not learned about therefore still ships — it just lands at the
//! end instead of its schema slot, which the differential would catch.
//!
//! The table is a byte-exact dump of v4's real schemas — regenerate it with
//! `harness/oracle/fixtures/dump-export-key-order.ts` (recipe in that file's
//! header) whenever a v4 schema drifts.

use serde_json::{Map, Value};
use std::sync::OnceLock;

/// The committed dump (`schema-key-order.json`), keyed by record kind.
const SCHEMA_KEY_ORDER_JSON: &str = include_str!("schema-key-order.json");

fn table() -> &'static Map<String, Value> {
    static T: OnceLock<Map<String, Value>> = OnceLock::new();
    T.get_or_init(|| {
        serde_json::from_str::<Value>(SCHEMA_KEY_ORDER_JSON)
            .expect("schema-key-order.json parses")
            .as_object()
            .expect("schema-key-order.json is an object")
            .clone()
    })
}

/// The template for one record kind (`character`, `chat_message`, …), or `None`
/// when the kind's shape is built by the writer itself (the doc-store records).
fn template(kind: &str) -> Option<&'static Vec<Value>> {
    table().get(kind).and_then(Value::as_array)
}

/// Rebuild `value`'s object in v4's schema declaration order for `kind`. Keys the
/// template doesn't name keep their original order, appended after. A non-object
/// value (or an unknown kind) is returned unchanged.
pub(crate) fn reorder(kind: &str, value: Value) -> Value {
    let Some(keys) = template(kind) else {
        return value;
    };
    let Value::Object(obj) = value else {
        return value;
    };
    let mut out = Map::with_capacity(obj.len());
    for k in keys {
        let Some(k) = k.as_str() else { continue };
        if let Some(v) = obj.get(k) {
            out.insert(k.to_string(), v.clone());
        }
    }
    for (k, v) in obj {
        if !out.contains_key(&k) {
            out.insert(k, v);
        }
    }
    Value::Object(out)
}

/// [`reorder`] over an already-destructured map (the `_tagNames` / `_*` spread
/// sites, whose synthetic keys are absent from every template and so land last —
/// exactly where v4's object spread puts them).
pub(crate) fn reorder_map(kind: &str, obj: Map<String, Value>) -> Map<String, Value> {
    match reorder(kind, Value::Object(obj)) {
        Value::Object(m) => m,
        other => other.as_object().cloned().unwrap_or_default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_expected_kind_has_a_template() {
        for kind in [
            "character",
            "chat",
            "chat_message",
            "memory",
            "roleplay_template",
            "tag",
            "project",
            "group",
            "connection_profile",
            "image_profile",
            "embedding_profile",
        ] {
            assert!(template(kind).is_some(), "missing template for {kind}");
        }
        // The doc-store records are assembled key-by-key by the writer itself,
        // and wardrobe items come off the character vault in the vault document's
        // own order (never through a Zod parse) — neither is templated.
        assert!(template("doc_mount_blob").is_none());
        assert!(template("wardrobe_item").is_none());
    }

    /// [P4.D120 / v4 `d25dacc1`] The archived flags survive the export
    /// projection.
    ///
    /// v4 changed NO writer or reader code for this: both fields already rode
    /// along structurally, and `d25dacc1` only DECLARED them in
    /// `public/schemas/qtap-export.schema.json` (which v5 has never shipped —
    /// a named standalone deferral). What could still lose them on v5's side is
    /// this reorder: a scenario's `archived` lives INSIDE the `scenarios` array
    /// value (untouched by key ordering), and a wardrobe item is not templated
    /// at all. Both facts are pinned here, because the P4.D63 lesson is that a
    /// stale `schema-key-order.json` silently relocates — or drops — exactly
    /// this kind of key.
    #[test]
    fn the_archived_flags_survive_the_export_projection() {
        let character: Value = serde_json::from_str(
            r#"{
                 "name": "Bertie",
                 "id": "c-1",
                 "scenarios": [
                   {"id": "s-1", "title": "Active", "content": "A."},
                   {"id": "s-2", "title": "Mothballed", "content": "B.", "archived": true}
                 ]
               }"#,
        )
        .unwrap();
        let out = reorder("character", character.clone());
        assert_eq!(
            out.get("scenarios"),
            character.get("scenarios"),
            "the scenarios array — `archived` included — must pass through verbatim"
        );

        // A wardrobe item has no template, so `reorder` is the identity on it and
        // `archivedAt` cannot be relocated or dropped.
        let item: Value = serde_json::from_str(
            r#"{"id":"w-1","title":"Retired breeches","archivedAt":"2026-02-01T00:00:00.000Z"}"#,
        )
        .unwrap();
        assert_eq!(reorder("wardrobe_item", item.clone()), item);
    }

    #[test]
    fn reorder_is_non_lossy_and_appends_unknown_keys() {
        let v: Value =
            serde_json::from_str(r#"{"_tagNames":["x"],"name":"N","zzz":1,"id":"i"}"#).unwrap();
        let out = reorder("tag", v);
        // id/name lead (schema order); the two non-schema keys keep their
        // relative order after.
        assert_eq!(
            out.to_string(),
            r#"{"id":"i","name":"N","_tagNames":["x"],"zzz":1}"#
        );
    }

    /// P4.D79: the regenerated table must place `multiCharacterPrefill` in v4's
    /// schema slot — immediately after `pseudoToolMode`, NOT appended at the
    /// end. The D65 lesson: `schema-key-order.json` stales on every D23 re-dump
    /// and the non-lossy `reorder` would happily ship a new key at the tail,
    /// which is a silent byte divergence in the `.qtap` artifact.
    #[test]
    fn connection_profile_carries_the_prefill_in_its_schema_slot() {
        let t = template("connection_profile").expect("template");
        let idx = t
            .iter()
            .position(|k| k == "multiCharacterPrefill")
            .expect("multiCharacterPrefill is in the regenerated table");
        assert_eq!(
            t[idx - 1],
            "pseudoToolMode",
            "the prefill must follow pseudoToolMode"
        );
        assert_eq!(t[idx + 1], "modelClass", "and precede modelClass");
        // P4.D135: the 4.10 fallback pair sits between `modelClass` and
        // `maxContext` — the Zod declaration order, which is ALSO where the
        // generateDDL re-dump puts the columns (and NOT where
        // `sqlite-initial-schema.ts`'s hand-written base table puts them).
        assert_eq!(
            &t[idx + 1..idx + 5],
            &[
                Value::from("modelClass"),
                Value::from("fallbackProfileId"),
                Value::from("allowTierFallback"),
                Value::from("maxContext"),
            ],
            "the fallback pair follows modelClass and precedes maxContext"
        );

        // And the reorder actually applies it: a net-read object in COLUMN order
        // comes out in SCHEMA order with the key in place.
        let v: Value = serde_json::from_str(
            r#"{"id":"i","pseudoToolMode":"auto","allowTierFallback":true,"modelClass":null,"multiCharacterPrefill":false,"fallbackProfileId":"u"}"#,
        )
        .unwrap();
        assert_eq!(
            reorder("connection_profile", v).to_string(),
            r#"{"id":"i","pseudoToolMode":"auto","multiCharacterPrefill":false,"modelClass":null,"fallbackProfileId":"u","allowTierFallback":true}"#
        );
    }

    #[test]
    fn unknown_kind_passes_through() {
        let v: Value = serde_json::from_str(r#"{"b":1,"a":2}"#).unwrap();
        assert_eq!(reorder("not_a_kind", v).to_string(), r#"{"b":1,"a":2}"#);
    }
}
