//! Storage-type coercion for mount-index rows read back out of a backup —
//! v4 `lib/backup/restore/mount-index-coercion.ts` (NEW in `c1507f47`).
//!
//! `dumpMountIndexTable` in the backup service is a raw `SELECT *`, so the
//! archive carries SQLite storage types rather than the domain shapes the
//! repository schemas demand: JSON-text for the pattern arrays on
//! `doc_mount_points`, and INTEGER 0/1 for every boolean. In v4 those rows went
//! straight to Zod-validating `create()`s and were rejected wholesale — the
//! archive held the stores and links and the restore dropped them on the floor.
//! These helpers put the rows back into domain shape immediately before the
//! creates.
//!
//! ## Why v5 needs this even though its `create`s never rejected the rows
//!
//! v5's types are permissive where v4's Zod was strict, so v5 always created the
//! rows — but with the WRONG VALUES, which is the quieter half of the same bug:
//!
//! - `rows::sa` reads a JSON **array**, so a JSON-**text** pattern column read as
//!   `[]`. Every restored character vault, project store and group store came
//!   back with empty include/exclude patterns instead of the archived ones.
//! - `rows::b` / `create_from_row`'s `ob` read a JSON **bool**, so INTEGER `0`
//!   fell through to the column DEFAULT — `true`. A store the user had disabled,
//!   or a document whose `allowCharacterWrite` was `0`, came back **permissive**.
//!   (The committed archives carry `1` everywhere, so no differential could see
//!   that arm; the tier-1 corpus below covers it deliberately.)
//!
//! Both are fixed by porting v4's coercion at v4's own two call sites, 22a and
//! 22d. Deliberately tolerant of already-correct input, exactly as v4 is: a JSON
//! column is parsed only when it arrives as a string, a boolean coerced only when
//! it arrives as a number. That keeps it correct for a backup path that later
//! learns to normalize at the source, and for rows that never went through
//! SQLite at all.

use serde_json::Value;

/// v4's `coerceDocMountPointRow` default for `includePatterns`.
pub const DEFAULT_INCLUDE_PATTERNS: [&str; 4] = ["*.md", "*.txt", "*.pdf", "*.docx"];

/// v4's `coerceDocMountPointRow` default for `excludePatterns`.
pub const DEFAULT_EXCLUDE_PATTERNS: [&str; 4] = [".git", "node_modules", ".obsidian", ".trash"];

/// Coerce one column that should be `string[]` but may have arrived as the JSON
/// text SQLite stored. Anything unparseable — or parseable into something that
/// isn't an array — falls back to the caller's default rather than failing the
/// row (v4 `coerceStringArray`).
///
/// Note the asymmetry v4 has and this keeps: an ARRAY input is filtered to its
/// string members (so `[]` stays `[]`, and a mixed array loses its non-strings),
/// but a STRING input that parses to a non-array takes the fallback. An empty
/// string never parses and takes the fallback too.
pub fn coerce_string_array(value: Option<&Value>, fallback: &[&str]) -> Vec<String> {
    let fb = || fallback.iter().map(|s| (*s).to_string()).collect();
    match value {
        Some(Value::Array(a)) => a
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect(),
        Some(Value::String(s)) if !s.is_empty() => match serde_json::from_str::<Value>(s) {
            Ok(Value::Array(a)) => a
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect(),
            _ => fb(),
        },
        _ => fb(),
    }
}

/// Coerce one column that should be `boolean` but may have arrived as SQLite's
/// INTEGER 0/1. Null/undefined take the caller's default, matching the column
/// defaults in the schema (v4 `coerceBoolean`).
pub fn coerce_boolean(value: Option<&Value>, fallback: bool) -> bool {
    match value {
        Some(Value::Bool(b)) => *b,
        // v4 tests `typeof value === 'number'`, which is true for any JSON
        // number including a float; `!== 0` is the whole rule.
        Some(v) if v.is_number() => v.as_f64().is_some_and(|n| n != 0.0),
        _ => fallback,
    }
}

/// Put a backed-up `doc_mount_points` row back into domain shape: the two
/// pattern columns become real arrays, `enabled` a real boolean
/// (v4 `coerceDocMountPointRow`).
///
/// Whole-row like v4's, not field-by-field, so the key ORDER v4's
/// spread-then-overwrite produces is reproduced too — `serde_json`'s
/// `preserve_order` `insert` keeps an existing key in place and appends a new
/// one, which is exactly what a JS object literal does.
pub fn coerce_doc_mount_point_row(row: &Value) -> Value {
    let mut out = row.clone();
    if let Some(m) = out.as_object_mut() {
        let include = coerce_string_array(m.get("includePatterns"), &DEFAULT_INCLUDE_PATTERNS);
        let exclude = coerce_string_array(m.get("excludePatterns"), &DEFAULT_EXCLUDE_PATTERNS);
        let enabled = coerce_boolean(m.get("enabled"), true);
        m.insert("includePatterns".into(), Value::from(include));
        m.insert("excludePatterns".into(), Value::from(exclude));
        m.insert("enabled".into(), Value::Bool(enabled));
    }
    out
}

/// The three per-document policy flags on a `doc_mount_file_links` row, in v4's
/// order. They default permissive, matching the frontmatter default and the
/// column defaults (v4 `coerceDocMountFileLinkRow`).
pub const LINK_POLICY_FLAGS: [&str; 3] =
    ["allowEmbed", "allowCharacterRead", "allowCharacterWrite"];

/// Put a backed-up `doc_mount_file_links` row back into domain shape: the three
/// policy flags become real booleans in place. Returns a copy; the caller hands
/// it to `create_from_row`, whose own reader wants `Value::Bool`.
pub fn coerce_doc_mount_file_link_row(row: &Value) -> Value {
    let mut out = row.clone();
    if let Some(m) = out.as_object_mut() {
        for key in LINK_POLICY_FLAGS {
            let coerced = coerce_boolean(m.get(key), true);
            m.insert(key.to_string(), Value::Bool(coerced));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn json_text_patterns_are_parsed_not_defaulted() {
        let row = json!({ "includePatterns": "[\"*.md\",\"*.txt\"]" });
        assert_eq!(
            coerce_string_array(row.get("includePatterns"), &DEFAULT_INCLUDE_PATTERNS),
            vec!["*.md".to_string(), "*.txt".to_string()]
        );
    }

    #[test]
    fn an_empty_json_array_stays_empty_in_both_shapes() {
        // The Project/Group stores archive `includePatterns` as the TEXT "[]",
        // and it must not pick up the four-extension default.
        assert_eq!(
            coerce_string_array(Some(&json!("[]")), &DEFAULT_INCLUDE_PATTERNS),
            Vec::<String>::new()
        );
        assert_eq!(
            coerce_string_array(Some(&json!([])), &DEFAULT_INCLUDE_PATTERNS),
            Vec::<String>::new()
        );
    }

    #[test]
    fn unusable_input_takes_the_fallback() {
        for v in [
            json!(""),
            json!(null),
            json!("not json"),
            json!("{\"a\":1}"),
        ] {
            assert_eq!(
                coerce_string_array(Some(&v), &DEFAULT_EXCLUDE_PATTERNS),
                DEFAULT_EXCLUDE_PATTERNS
                    .iter()
                    .map(|s| s.to_string())
                    .collect::<Vec<_>>(),
                "{v}"
            );
        }
        assert_eq!(
            coerce_string_array(None, &DEFAULT_EXCLUDE_PATTERNS),
            DEFAULT_EXCLUDE_PATTERNS
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn sqlite_integers_become_booleans_and_zero_means_false() {
        assert!(!coerce_boolean(Some(&json!(0)), true));
        assert!(coerce_boolean(Some(&json!(1)), false));
        assert!(!coerce_boolean(Some(&json!(false)), true));
        assert!(coerce_boolean(None, true));
        assert!(!coerce_boolean(Some(&json!(null)), false));
        assert!(
            !coerce_boolean(Some(&json!("1")), false),
            "a string is not a number"
        );
    }

    #[test]
    fn a_link_rows_three_flags_are_coerced_in_place() {
        let coerced = coerce_doc_mount_file_link_row(&json!({
            "relativePath": "a.md",
            "allowEmbed": 0,
            "allowCharacterRead": 1,
        }));
        assert_eq!(coerced["allowEmbed"], json!(false));
        assert_eq!(coerced["allowCharacterRead"], json!(true));
        // Absent → the permissive default.
        assert_eq!(coerced["allowCharacterWrite"], json!(true));
        // Everything else is untouched.
        assert_eq!(coerced["relativePath"], json!("a.md"));
    }
}
