//! The serde THREE-state decoder — the ONE neutral home for `double_option`
//! (P4.148, the `07b8f0209` follow-ups round; before it the `db` layer's
//! `groups` / `projects` bags imported it from `services::mount_index::sync::
//! types`, a `db → services` dependency).
//!
//! A Zod `.nullable().optional()` key has three states v4 keeps apart — absent
//! (omitted from the parse output), `null` (kept), and a value — and serde's
//! default `Option<T>` collapses the first two. On an `Option<Option<T>>` field
//! this decodes an ABSENT key to `None` (with `#[serde(default)]`), an explicit
//! `null` to `Some(None)`, and a value to `Some(Some(v))`.
//!
//! The sync family re-exports it (`services::mount_index::sync::types` keeps a
//! `pub use`); `api::types` and `api::memories` still carry private twins (a
//! Tier-3 repoint, P4.148 item 21 — both files were frozen that round).

use serde::Deserialize;

/// See the module docs: absent → `None`, `null` → `Some(None)`, a value →
/// `Some(Some(v))`. Pair with `#[serde(default)]` so the absent arm is reached.
pub fn double_option<'de, T, D>(de: D) -> Result<Option<Option<T>>, D::Error>
where
    T: Deserialize<'de>,
    D: serde::Deserializer<'de>,
{
    Deserialize::deserialize(de).map(Some)
}

#[cfg(test)]
mod tests {
    use super::double_option;
    use serde::Deserialize;

    #[derive(Debug, Deserialize)]
    struct Bag {
        #[serde(default, deserialize_with = "double_option")]
        k: Option<Option<String>>,
    }

    #[test]
    fn absent_null_and_value_stay_three_states() {
        let absent: Bag = serde_json::from_str("{}").unwrap();
        let null: Bag = serde_json::from_str(r#"{"k":null}"#).unwrap();
        let value: Bag = serde_json::from_str(r#"{"k":"v"}"#).unwrap();
        assert_eq!(absent.k, None);
        assert_eq!(null.k, Some(None));
        assert_eq!(value.k, Some(Some("v".to_string())));
    }
}
