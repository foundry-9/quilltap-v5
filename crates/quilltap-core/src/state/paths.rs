//! Pure path helpers for the persistent state system — v4
//! `lib/state/state-paths.ts` (NEW at `f48f34dc`, extracted verbatim from the
//! old state handler).
//!
//! `parse_path`/`get_at_path`/`set_at_path`/`delete_at_path` navigate and
//! mutate the plain JSON objects that back chat / project / group / general
//! state. They carry no I/O, no repository access, and no logging, so they can
//! be shared by the state tool handler, the cascade resolver, and Pascal's
//! `$state` resolution without dragging any of those subsystems in.
//!
//! Extracted from `tools::state` (which now re-uses them, the way v4's handler
//! re-exports them for backwards compatibility). One deliberate v4 difference
//! carried over: the root-set guard here returns a plain error string rather
//! than the handler's typed `StateError` — the handler re-wraps it.
//!
//! Known limitation (pre-existing, documented alongside Pascal's `$state`): the
//! `\w+` segment pattern means keys containing spaces or dots are unreachable
//! via a path string.

use serde_json::{Map, Value};

/// A single key in a parsed path — a property name or an array index (v4's
/// `(string | number)[]`).
#[derive(Debug, Clone, PartialEq)]
pub enum PathKey {
    Prop(String),
    Index(usize),
}

/// v4 `parsePath`: split `"player.inventory[0].name"` into keys. Regex
/// `(\w+)|\[(\d+)\]` — JS `\w`/`\d` are ASCII, so `[A-Za-z0-9_]+` / `[0-9]+`.
/// Empty / whitespace / absent → `[]`.
pub fn parse_path(path: Option<&str>) -> Vec<PathKey> {
    let Some(path) = path else {
        return Vec::new();
    };
    if path.trim().is_empty() {
        return Vec::new();
    }
    let bytes = path.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if c.is_ascii_alphanumeric() || c == b'_' {
            // A property-name run: [A-Za-z0-9_]+.
            let start = i;
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            out.push(PathKey::Prop(path[start..i].to_string()));
        } else if c == b'[' {
            // An array index: \[(\d+)\]. Only matches when digits + `]` follow.
            let mut j = i + 1;
            let dstart = j;
            while j < bytes.len() && bytes[j].is_ascii_digit() {
                j += 1;
            }
            if j > dstart && j < bytes.len() && bytes[j] == b']' {
                // parseInt(_, 10). JS numbers are f64; a path index in range fits usize.
                if let Ok(idx) = path[dstart..j].parse::<usize>() {
                    out.push(PathKey::Index(idx));
                }
                i = j + 1;
            } else {
                // `[` not opening a valid index — the regex would skip this char.
                i += 1;
            }
        } else {
            // Any other char is between tokens; the global regex skips it.
            i += 1;
        }
    }
    out
}

/// v4 `getAtPath`: walk `path`, returning `None` (JS `undefined`) when a segment
/// is missing or the current node is not indexable, and `Some(Value::Null)` when
/// the resolved value is an explicit stored `null` (v4's `getAtPath` returns a
/// final null as-is; an INTERMEDIATE null short-circuits to undefined). Empty path
/// → the whole object.
pub fn get_at_path(obj: &Value, path: &[PathKey]) -> Option<Value> {
    // `Option<Value>`: `None` = JS undefined, `Some(Null)` = a null value.
    let mut current: Option<Value> = Some(obj.clone());
    for key in path {
        match current {
            // v4 top-of-loop guard: `current === null || undefined → undefined`.
            None => return None,
            Some(Value::Null) => return None,
            // typeof current !== 'object' → undefined (arrays are objects in JS).
            Some(ref v) if !v.is_object() && !v.is_array() => return None,
            Some(container) => {
                current = index_value(&container, key);
            }
        }
    }
    current
}

/// Index `container[key]` the JS way: `None` for a missing key/out-of-range index
/// (undefined), else `Some(value)` (possibly `Some(Null)`).
fn index_value(container: &Value, key: &PathKey) -> Option<Value> {
    match container {
        Value::Object(map) => {
            let k = match key {
                PathKey::Prop(s) => s.clone(),
                // JS coerces a numeric object key to string.
                PathKey::Index(n) => n.to_string(),
            };
            map.get(&k).cloned()
        }
        Value::Array(arr) => match key {
            PathKey::Index(n) => arr.get(*n).cloned(),
            // JS `array["foo"]` → undefined (non-index property).
            PathKey::Prop(_) => None,
        },
        _ => None,
    }
}

/// v4 `setAtPath`: set `value` at `path`, creating intermediate arrays/objects
/// based on the next key's type. Mutates `obj` in place. An empty path replaces the
/// root (must be a plain object). Returns `Err` with v4's plain-`Error` message for
/// the "root to non-object" case — callers that need a typed error wrap it.
pub fn set_at_path(obj: &mut Value, path: &[PathKey], value: Value) -> Result<(), String> {
    if path.is_empty() {
        // Setting root — value must be a plain object.
        if value.is_object() {
            *obj = value;
            return Ok(());
        }
        return Err("Cannot set root state to non-object value".to_string());
    }
    let mut current = obj;
    for i in 0..path.len() - 1 {
        let key = &path[i];
        let next_is_index = matches!(path[i + 1], PathKey::Index(_));
        current = descend_or_create(current, key, next_is_index);
    }
    let last = &path[path.len() - 1];
    assign(current, last, value);
    Ok(())
}

/// Ensure `current[key]` is an object/array (creating it, or overwriting a
/// primitive/null) based on whether the NEXT key is an index, then return `&mut`
/// to it — v4's intermediate-structure creation. `current` is always a container
/// (the root object, or the slot created by a prior step).
fn descend_or_create<'a>(
    current: &'a mut Value,
    key: &PathKey,
    next_is_index: bool,
) -> &'a mut Value {
    let fresh = if next_is_index {
        Value::Array(Vec::new())
    } else {
        Value::Object(Map::new())
    };
    match current {
        Value::Object(map) => {
            let k = match key {
                PathKey::Prop(s) => s.clone(),
                PathKey::Index(n) => n.to_string(),
            };
            let entry = map.entry(k).or_insert(Value::Null);
            // Create/overwrite when absent, null, or a primitive.
            if !entry.is_object() && !entry.is_array() {
                *entry = fresh;
            }
            entry
        }
        Value::Array(arr) => {
            let idx = match key {
                PathKey::Index(n) => *n,
                PathKey::Prop(s) => s.parse::<usize>().unwrap_or(0),
            };
            if idx >= arr.len() {
                arr.resize(idx + 1, Value::Null);
            }
            let entry = &mut arr[idx];
            if !entry.is_object() && !entry.is_array() {
                *entry = fresh;
            }
            entry
        }
        // Reached only with a container (root or a created slot).
        _ => current,
    }
}

/// Assign `value` at `current[last]` (v4's final `current[lastKey] = value`).
fn assign(current: &mut Value, last: &PathKey, value: Value) {
    match current {
        Value::Object(map) => {
            let k = match last {
                PathKey::Prop(s) => s.clone(),
                PathKey::Index(n) => n.to_string(),
            };
            map.insert(k, value);
        }
        Value::Array(arr) => {
            let idx = match last {
                PathKey::Index(n) => *n,
                // JS assigns a string key onto an array as a property; the corpus
                // never mixes shapes here, but keep it faithful by coercing to 0.
                PathKey::Prop(_) => 0,
            };
            if idx >= arr.len() {
                arr.resize(idx + 1, Value::Null);
            }
            arr[idx] = value;
        }
        _ => {}
    }
}

/// v4 `deleteAtPath`: remove the value at `path`. Returns whether anything was
/// deleted. Empty path (cannot delete root) → false. Array + numeric last key →
/// splice (reindexes); otherwise `delete`.
pub fn delete_at_path(obj: &mut Value, path: &[PathKey]) -> bool {
    if path.is_empty() {
        return false;
    }
    // Walk to the parent of the last key.
    let mut current = obj;
    for key in &path[..path.len() - 1] {
        let next = match current {
            Value::Object(map) => {
                let k = match key {
                    PathKey::Prop(s) => s.clone(),
                    PathKey::Index(n) => n.to_string(),
                };
                match map.get_mut(&k) {
                    Some(v) if v.is_object() || v.is_array() => v,
                    Some(_) => return false, // primitive → not indexable
                    None => return false,    // undefined/null
                }
            }
            Value::Array(arr) => match key {
                PathKey::Index(n) => match arr.get_mut(*n) {
                    Some(v) if v.is_object() || v.is_array() => v,
                    Some(_) => return false,
                    None => return false,
                },
                PathKey::Prop(_) => return false,
            },
            _ => return false,
        };
        current = next;
    }
    let last = &path[path.len() - 1];
    match current {
        Value::Object(map) => {
            let k = match last {
                PathKey::Prop(s) => s.clone(),
                PathKey::Index(n) => n.to_string(),
            };
            if !map.contains_key(&k) {
                return false;
            }
            // JS `delete obj[key]` preserves the remaining keys' order. Under
            // `preserve_order`, `Map::remove` is a swap_remove (the LAST key
            // moves into the hole) — `shift_remove` is the order-preserving one.
            map.shift_remove(&k);
            true
        }
        Value::Array(arr) => match last {
            PathKey::Index(n) => {
                if *n >= arr.len() {
                    return false;
                }
                arr.remove(*n); // Array.splice(n, 1)
                true
            }
            // `!(lastKey in current)` for a string key on an array → false.
            PathKey::Prop(_) => false,
        },
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_path_dot_and_index() {
        assert_eq!(parse_path(None), vec![]);
        assert_eq!(parse_path(Some("")), vec![]);
        assert_eq!(parse_path(Some("   ")), vec![]);
        assert_eq!(
            parse_path(Some("player.inventory[0].name")),
            vec![
                PathKey::Prop("player".into()),
                PathKey::Prop("inventory".into()),
                PathKey::Index(0),
                PathKey::Prop("name".into()),
            ]
        );
        assert_eq!(
            parse_path(Some("a[2][3]")),
            vec![
                PathKey::Prop("a".into()),
                PathKey::Index(2),
                PathKey::Index(3)
            ]
        );
    }

    #[test]
    fn get_at_path_walks() {
        let obj = json!({ "player": { "health": 5, "inv": [ { "name": "sword" } ] } });
        assert_eq!(
            get_at_path(&obj, &parse_path(Some("player.health"))),
            Some(json!(5))
        );
        assert_eq!(
            get_at_path(&obj, &parse_path(Some("player.inv[0].name"))),
            Some(json!("sword"))
        );
        assert_eq!(get_at_path(&obj, &parse_path(Some("player.missing"))), None);
        // Primitive mid-path → undefined.
        assert_eq!(
            get_at_path(&obj, &parse_path(Some("player.health.nope"))),
            None
        );
        assert_eq!(get_at_path(&obj, &[]), Some(obj.clone()));
    }

    #[test]
    fn set_at_path_creates_intermediate() {
        let mut obj = json!({});
        set_at_path(&mut obj, &parse_path(Some("a.b.c")), json!(1)).unwrap();
        assert_eq!(obj, json!({ "a": { "b": { "c": 1 } } }));

        let mut obj2 = json!({});
        set_at_path(&mut obj2, &parse_path(Some("list[2]")), json!("x")).unwrap();
        assert_eq!(obj2, json!({ "list": [null, null, "x"] }));

        // Root set to non-object → error.
        let mut obj3 = json!({ "k": 1 });
        assert_eq!(
            set_at_path(&mut obj3, &[], json!(5)).unwrap_err(),
            "Cannot set root state to non-object value"
        );
    }

    #[test]
    fn delete_at_path_object_and_array() {
        let mut obj = json!({ "a": { "b": 1, "c": 2 } });
        assert!(delete_at_path(&mut obj, &parse_path(Some("a.b"))));
        assert_eq!(obj, json!({ "a": { "c": 2 } }));
        // Missing → false.
        assert!(!delete_at_path(&mut obj, &parse_path(Some("a.z"))));
        // Root → false.
        assert!(!delete_at_path(&mut obj, &[]));

        let mut arr = json!({ "l": [10, 20, 30] });
        assert!(delete_at_path(&mut arr, &parse_path(Some("l[1]"))));
        assert_eq!(arr, json!({ "l": [10, 30] }));
    }
}
