//! JSON-row accessors for the restore orchestrator.
//!
//! Every collection in a backup archive is a `findAll` projection: present keys
//! carry the column's value, and a NULL column is **omitted entirely** rather
//! than written as `null`. So "absent" and "null" mean the same thing here, and
//! every accessor treats them alike — which is also what v4's destructure-then-
//! spread does when it hands the object to the repository's Zod parse.
//!
//! The defaults each accessor takes are v4's Zod `.default(...)`s, quoted at the
//! call site rather than guessed here.

use serde::de::DeserializeOwned;
use serde_json::Value;

/// A required string column; absent/null/non-string → `""` (v4's Zod would
/// throw, and the phase's warn-and-continue would catch it — a `""` here
/// reaches the same per-row warning through the DB's NOT NULL, without the
/// orchestrator having to model Zod's error text).
pub fn s(v: &Value, k: &str) -> String {
    v.get(k)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// A nullable string column.
pub fn os(v: &Value, k: &str) -> Option<String> {
    v.get(k).and_then(Value::as_str).map(str::to_string)
}

/// A boolean column with a Zod default.
pub fn b(v: &Value, k: &str, default: bool) -> bool {
    v.get(k).and_then(Value::as_bool).unwrap_or(default)
}

/// A `.optional()` boolean with NO default — absent stays absent (SQL NULL),
/// which is distinct from an explicit `false`.
pub fn ob(v: &Value, k: &str) -> Option<bool> {
    v.get(k).and_then(Value::as_bool)
}

/// A number column with a Zod default.
pub fn n(v: &Value, k: &str, default: f64) -> f64 {
    v.get(k).and_then(Value::as_f64).unwrap_or(default)
}

/// A nullable number column.
pub fn on(v: &Value, k: &str) -> Option<f64> {
    v.get(k).and_then(Value::as_f64)
}

/// A JSON string-array column; absent → `[]` (every one of these carries
/// `.default([])`).
pub fn sa(v: &Value, k: &str) -> Vec<String> {
    v.get(k)
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// An open-JSON object column with a Zod default.
pub fn obj(v: &Value, k: &str, default: Value) -> Value {
    match v.get(k) {
        Some(Value::Null) | None => default,
        Some(other) => other.clone(),
    }
}

/// A serialized embedding column (`number[]` in the archive, little-endian
/// Float32 bytes on disk). Absent/empty → `None`, i.e. SQL NULL.
pub fn embedding(v: &Value, k: &str) -> Option<Vec<f32>> {
    let arr = v.get(k)?.as_array()?;
    if arr.is_empty() {
        return None;
    }
    Some(
        arr.iter()
            .map(|x| x.as_f64().unwrap_or(0.0) as f32)
            .collect(),
    )
}

/// A RULED DIVERGENCE (the human, 2026-10-07, at the `94fbb1ae3` boot-hardness
/// unification — "fix v5"): v4's FULL backup writes `data.memories` raw
/// (`backup-service.ts:657`, `writeJsonArrayFile` → `JSON.stringify`), and
/// `JSON.stringify(Float32Array)` is an index-keyed OBJECT `{"0":v0,"1":v1,…}`
/// — a shape v4's own restore then refuses in `MemorySchema`'s embedding union
/// (`memory.types.ts:73-84`: Float32Array / number[] / Buffer / string), so v4
/// drops EVERY embedded memory on restore with an `invalid_union` warning. v5
/// decodes exactly that shape — keys `"0"…"n-1"` (canonical decimal, no gaps),
/// every value a number — back into the `number[]` the union's array option
/// takes, so the memory AND its vector restore. Any other object is left
/// untouched (the schema refuses it, as v4's does). Answers whether it decoded.
pub fn decode_index_keyed_embedding(item: &mut serde_json::Map<String, Value>) -> bool {
    let Some(Value::Object(o)) = item.get("embedding") else {
        return false;
    };
    let mut values: Vec<Option<Value>> = vec![None; o.len()];
    for (k, x) in o {
        let Ok(i) = k.parse::<usize>() else {
            return false;
        };
        if i.to_string() != *k || i >= values.len() || !x.is_number() {
            return false;
        }
        values[i] = Some(x.clone());
    }
    let decoded: Vec<Value> = values.into_iter().flatten().collect();
    item.insert("embedding".into(), Value::Array(decoded));
    true
}

/// Deserialize a nested typed value off the row, or the type's `Default`.
pub fn de_or_default<T: DeserializeOwned + Default>(v: &Value, k: &str) -> T {
    v.get(k)
        .cloned()
        .and_then(|x| serde_json::from_value(x).ok())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn an_index_keyed_embedding_decodes_to_its_number_array() {
        let mut m = json!({"embedding": {"1": -0.5, "0": 0.25, "2": 1}})
            .as_object()
            .cloned()
            .unwrap();
        assert!(decode_index_keyed_embedding(&mut m));
        assert_eq!(m["embedding"], json!([0.25, -0.5, 1]));
        // `Float32Array(0)` stringifies to `{}` → `[]` (stored as SQL NULL).
        let mut empty = json!({"embedding": {}}).as_object().cloned().unwrap();
        assert!(decode_index_keyed_embedding(&mut empty));
        assert_eq!(empty["embedding"], json!([]));
    }

    #[test]
    fn any_other_embedding_shape_is_left_for_the_schema() {
        for e in [
            json!({"0": 0.25, "2": 0.5}), // a gap
            json!({"00": 0.25}),          // not canonical decimal
            json!({"0": "0.25"}),         // a non-number value
            json!({"x": 1}),              // not an index
            json!([0.25, -0.5]),          // already the array option
            json!("[0.25]"),              // the string option
            Value::Null,
        ] {
            let mut m = json!({ "embedding": e.clone() })
                .as_object()
                .cloned()
                .unwrap();
            assert!(!decode_index_keyed_embedding(&mut m), "{e}");
            assert_eq!(m["embedding"], e);
        }
        let mut absent = serde_json::Map::new();
        assert!(!decode_index_keyed_embedding(&mut absent));
        assert!(absent.is_empty());
    }

    #[test]
    fn absent_and_null_are_the_same_thing() {
        let row = json!({ "a": null, "n": null, "arr": null });
        assert_eq!(os(&row, "a"), None);
        assert_eq!(os(&row, "missing"), None);
        assert_eq!(on(&row, "n"), None);
        assert_eq!(sa(&row, "arr"), Vec::<String>::new());
        assert_eq!(obj(&row, "arr", json!({})), json!({}));
    }

    #[test]
    fn defaults_only_fire_when_the_key_is_not_there() {
        let row = json!({ "flag": false, "count": 0 });
        assert!(!b(&row, "flag", true));
        assert!(b(&row, "other", true));
        assert_eq!(n(&row, "count", 7.0), 0.0);
        assert_eq!(n(&row, "other", 7.0), 7.0);
        assert_eq!(ob(&row, "flag"), Some(false));
        assert_eq!(ob(&row, "other"), None);
    }

    #[test]
    fn an_empty_embedding_array_is_sql_null() {
        assert_eq!(embedding(&json!({"e": []}), "e"), None);
        assert_eq!(
            embedding(&json!({"e": [1.0, -0.5]}), "e"),
            Some(vec![1.0f32, -0.5f32])
        );
    }
}
