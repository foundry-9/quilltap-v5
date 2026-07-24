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

    #[test]
    fn unknown_kind_passes_through() {
        let v: Value = serde_json::from_str(r#"{"b":1,"a":2}"#).unwrap();
        assert_eq!(reorder("not_a_kind", v).to_string(), r#"{"b":1,"a":2}"#);
    }
}
