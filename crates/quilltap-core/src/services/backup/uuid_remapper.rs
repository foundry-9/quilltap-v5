//! v4 `lib/backup/uuid-remapper.ts` — the memoized old→new id map that
//! `new-account` restore runs every entity through.
//!
//! v4's own header: *"Used during restore operations when importing data to a
//! different account (new-account mode). Generates new UUIDs for all entities
//! while maintaining a consistent mapping and updates all foreign key
//! references across entities."*
//!
//! The memo is the whole mechanism: one instance is shared across every entity
//! pass in [`super::uuid_remap::remap_backup_data`], so a chat's `id` and a
//! memory's `chatId` resolve to the **same** new value no matter which pass
//! reaches them first. Reset it (or build a new one) per restore.
//!
//! ## Path divergence (deliberate, recorded)
//!
//! v4 keeps this file at `lib/backup/uuid-remapper.ts` and its only consumer at
//! `lib/backup/restore/uuid-remap.ts`. v5 puts both flat under
//! `services/backup/` — `restore/` is a different lane's directory.
//!
//! ## The keying trap
//!
//! v4 keys a JS `Map`, which compares primitives **by value** and objects **by
//! identity**. Every array v4 remaps is a UUID array per its schemas
//! (`chat.types.ts:158`: `attachments: z.array(UUIDSchema)`), so in practice
//! every key is a string — but `remap` is public and takes `any`, so the port
//! keys by a discriminant-tagged representation that distinguishes `"5"` from
//! `5`, and every OUTPUT is a fresh UUID string regardless of the input's type.
//!
//! Two out-of-contract inputs diverge, both unreachable from
//! [`super::uuid_remap::remap_backup_data`] and both recorded rather than
//! modelled:
//!
//! - An **array or object** key: JS keys those by reference identity (two
//!   structurally identical objects get two different new ids); the port keys
//!   them structurally (one shared id). No call site passes one — `remap_fields`
//!   only rewrites `typeof === 'string'` values, and every direct `remap` call
//!   site in the entity table passes a scalar id.
//! - A **non-object, non-null `obj`** handed to [`UuidRemapper::remap_fields`] /
//!   [`UuidRemapper::remap_array_fields`]: JS's `typeof [] === 'object'`, so v4
//!   would spread an array into `{0: …, 1: …}`. The port returns it unchanged.
//!   v4's own signature (`T extends Record<string, any>`) puts that out of
//!   contract.

use std::collections::HashMap;

use serde_json::{Map, Value};

use crate::pascal::js_value::to_js_string;

/// One memo entry. `display` is the key as v4's [`UuidRemapper::mapping`] would
/// report it — a JS object key is the `String(…)` coercion of the `Map` key, so
/// the number `5.5` reports as `"5.5"` and `null` as `"null"`.
struct Entry {
    display: String,
    new_id: String,
}

/// v4 `lib/backup/uuid-remapper.ts:15` — memoized old→new id mapping. The memo
/// is shared across every entity pass; that is the mechanism by which
/// cross-references stay internally consistent.
pub struct UuidRemapper {
    /// Insertion-ordered, so [`UuidRemapper::mapping`] reproduces the iteration
    /// order of v4's `Map`.
    order: Vec<Entry>,
    /// Tagged key → index into `order`.
    index: HashMap<String, usize>,
    id_source: Box<dyn FnMut() -> String + Send>,
}

/// The discriminant-tagged memo key — see the module header's keying trap.
fn map_key(v: &Value) -> String {
    match v {
        Value::String(s) => format!("s:{s}"),
        // JS `Map` keys numbers by SameValueZero on the numeric value, so `5`
        // and `5.0` are one key; `String(n)` collapses them the same way.
        Value::Number(_) => format!("n:{}", to_js_string(v)),
        Value::Bool(b) => format!("b:{b}"),
        Value::Null => "null".to_string(),
        Value::Array(_) | Value::Object(_) => {
            format!("j:{}", serde_json::to_string(v).unwrap_or_default())
        }
    }
}

impl UuidRemapper {
    /// v4 `createUuidRemapper()` (`:179`) — production ids from
    /// `crypto.randomUUID()`, here `Uuid::new_v4()`.
    pub fn new() -> Self {
        Self::with_id_source(Box::new(|| uuid::Uuid::new_v4().to_string()))
    }

    /// The oracle/test seam: a deterministic id source, so the differential
    /// needs no normalization. NOT used in production.
    pub fn with_id_source(source: Box<dyn FnMut() -> String + Send>) -> Self {
        Self {
            order: Vec::new(),
            index: HashMap::new(),
            id_source: source,
        }
    }

    /// v4 `remap(oldUuid)` (`:34`) — get or create the new id for an old one.
    /// The same input always yields the same output **for the life of the
    /// instance**; that memo is how cross-references stay connected.
    pub fn remap(&mut self, old: &Value) -> String {
        let key = map_key(old);
        if let Some(&i) = self.index.get(&key) {
            return self.order[i].new_id.clone();
        }
        let new_id = (self.id_source)();
        self.index.insert(key, self.order.len());
        self.order.push(Entry {
            display: to_js_string(old),
            new_id: new_id.clone(),
        });
        new_id
    }

    /// [`UuidRemapper::remap`] for the common case — a string id.
    pub fn remap_str(&mut self, old: &str) -> String {
        let key = format!("s:{old}");
        if let Some(&i) = self.index.get(&key) {
            return self.order[i].new_id.clone();
        }
        let new_id = (self.id_source)();
        self.index.insert(key, self.order.len());
        self.order.push(Entry {
            display: old.to_string(),
            new_id: new_id.clone(),
        });
        new_id
    }

    /// v4 `remapArray(uuids)` (`:54`) — each element through
    /// [`UuidRemapper::remap`], order preserved. A non-array warns and yields
    /// `[]`.
    ///
    /// **That `[]` branch is dead in practice:** the only caller is
    /// [`UuidRemapper::remap_array_fields`], which has already checked
    /// `Array.isArray`. Ported anyway — it is public API in v4.
    pub fn remap_array(&mut self, values: &Value) -> Value {
        let Some(arr) = values.as_array() else {
            return Value::Array(Vec::new());
        };
        let mut out = Vec::with_capacity(arr.len());
        for v in arr {
            out.push(Value::String(self.remap(v)));
        }
        Value::Array(out)
    }

    /// v4 `remapFields(obj, fields)` (`:74`) — shallow copy, then rewrite each
    /// named field **only if** `field in obj && typeof obj[field] === 'string'`.
    /// A `null`, a number or an absent key is left exactly as it was.
    ///
    /// The rewrite is an in-place `insert`, so an existing key keeps its
    /// position — object key order is observable here (`serde_json` is built
    /// with `preserve_order`) and this family's differential is byte-level.
    pub fn remap_fields(&mut self, obj: &Value, fields: &[&str]) -> Value {
        let Some(map) = obj.as_object() else {
            return obj.clone();
        };
        let mut out = map.clone();
        for field in fields {
            if let Some(Value::String(s)) = out.get(*field) {
                let old = s.clone();
                let new = self.remap_str(&old);
                out.insert((*field).to_string(), Value::String(new));
            }
        }
        Value::Object(out)
    }

    /// v4 `remapArrayFields(obj, fields)` (`:110`) — the same shape, rewriting
    /// only if `field in obj && Array.isArray(obj[field])`.
    pub fn remap_array_fields(&mut self, obj: &Value, fields: &[&str]) -> Value {
        let Some(map) = obj.as_object() else {
            return obj.clone();
        };
        let mut out = map.clone();
        for field in fields {
            if let Some(v @ Value::Array(_)) = out.get(*field) {
                let arr = v.clone();
                let new = self.remap_array(&arr);
                out.insert((*field).to_string(), new);
            }
        }
        Value::Object(out)
    }

    /// v4 `getMapping()` (`:144`) — insertion-ordered old→new. v4 folds the
    /// `Map` into a plain object, so the key is the `String(…)` coercion of
    /// whatever was remapped.
    pub fn mapping(&self) -> Vec<(String, String)> {
        self.order
            .iter()
            .map(|e| (e.display.clone(), e.new_id.clone()))
            .collect()
    }

    /// v4 `getMapping()` folded exactly as JS folds it into a `Record` — a
    /// later duplicate display key **overwrites the value in place** rather
    /// than appending. (Only reachable when two differently-typed keys coerce
    /// to the same string, e.g. `5.5` and `"5.5"`.)
    pub fn mapping_object(&self) -> Map<String, Value> {
        let mut out = Map::new();
        for e in &self.order {
            out.insert(e.display.clone(), Value::String(e.new_id.clone()));
        }
        out
    }

    /// v4 `clear()` (`:158`) — reset for a new restore operation.
    pub fn clear(&mut self) {
        self.order.clear();
        self.index.clear();
    }

    /// v4 `getSize()` (`:171`) — how many ids have been remapped.
    pub fn size(&self) -> usize {
        self.order.len()
    }
}

impl Default for UuidRemapper {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// The differential's id source: instantly recognizable in a diff.
    fn counting() -> UuidRemapper {
        let mut n = 0u64;
        UuidRemapper::with_id_source(Box::new(move || {
            n += 1;
            format!("00000000-0000-4000-8000-{n:012}")
        }))
    }

    fn id(n: u64) -> String {
        format!("00000000-0000-4000-8000-{n:012}")
    }

    #[test]
    fn remap_memoizes_by_value() {
        let mut r = counting();
        assert_eq!(r.remap_str("a"), id(1));
        assert_eq!(r.remap_str("b"), id(2));
        // The memo — this is what keeps cross-references connected.
        assert_eq!(r.remap_str("a"), id(1));
        assert_eq!(r.size(), 2);
    }

    #[test]
    fn remap_distinguishes_by_discriminant() {
        let mut r = counting();
        assert_eq!(r.remap(&json!("5.5")), id(1));
        assert_eq!(r.remap(&json!(5.5)), id(2));
        assert_eq!(r.remap(&Value::Null), id(3));
        assert_eq!(r.remap(&json!(true)), id(4));
        // …but both `5.5`s report under the one JS-coerced display key, and the
        // `Record` fold keeps the FIRST position with the LAST value.
        let m = r.mapping_object();
        let keys: Vec<&String> = m.keys().collect();
        assert_eq!(keys, vec!["5.5", "null", "true"]);
        assert_eq!(m["5.5"], Value::String(id(2)));
    }

    #[test]
    fn remap_fields_only_rewrites_strings() {
        let mut r = counting();
        let obj = json!({ "id": "x", "projectId": null, "n": 7, "keep": "y" });
        let out = r.remap_fields(&obj, &["id", "projectId", "n", "absent"]);
        assert_eq!(out["id"], Value::String(id(1)));
        assert_eq!(out["projectId"], Value::Null);
        assert_eq!(out["n"], json!(7));
        assert_eq!(out["keep"], json!("y"));
        assert!(out.get("absent").is_none());
        assert_eq!(r.size(), 1);
    }

    #[test]
    fn remap_fields_preserves_key_position() {
        let mut r = counting();
        let obj = json!({ "a": 1, "id": "x", "z": 2 });
        let out = r.remap_fields(&obj, &["id"]);
        let keys: Vec<&String> = out.as_object().unwrap().keys().collect();
        assert_eq!(keys, vec!["a", "id", "z"]);
    }

    #[test]
    fn remap_fields_leaves_non_objects_alone() {
        let mut r = counting();
        assert_eq!(r.remap_fields(&Value::Null, &["id"]), Value::Null);
        assert_eq!(r.remap_fields(&json!("s"), &["id"]), json!("s"));
        assert_eq!(r.size(), 0);
    }

    #[test]
    fn remap_array_fields_guards_on_is_array() {
        let mut r = counting();
        let obj = json!({ "tags": ["t1", "t2"], "linkedTo": "not-an-array", "empty": [] });
        let out = r.remap_array_fields(&obj, &["tags", "linkedTo", "empty", "absent"]);
        assert_eq!(out["tags"], json!([id(1), id(2)]));
        // The `Array.isArray` guard leaves a non-array exactly as it was — it is
        // NOT routed through `remapArray`'s dead `[]` branch.
        assert_eq!(out["linkedTo"], json!("not-an-array"));
        assert_eq!(out["empty"], json!([]));
    }

    #[test]
    fn remap_array_dead_branch_yields_empty() {
        // v4 `remapArray`'s non-array arm: warn and return []. Unreachable via
        // `remapArrayFields`, kept because it is public API.
        let mut r = counting();
        assert_eq!(r.remap_array(&json!("nope")), json!([]));
        assert_eq!(r.size(), 0);
    }

    #[test]
    fn clear_resets_the_memo() {
        let mut r = counting();
        r.remap_str("a");
        r.clear();
        assert_eq!(r.size(), 0);
        // A fresh id for the same input — the counter does not rewind.
        assert_eq!(r.remap_str("a"), id(2));
    }

    #[test]
    fn mapping_is_insertion_ordered() {
        let mut r = counting();
        r.remap_str("zzz");
        r.remap_str("aaa");
        assert_eq!(
            r.mapping(),
            vec![("zzz".into(), id(1)), ("aaa".into(), id(2))]
        );
    }
}
