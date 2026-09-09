//! The vendored `.qtap` export-schema guard (P4.86, Tier 1 item 2).
//!
//! `crates/quilltap-core/src/generators/qtap-export.schema.json` is a
//! byte-copy of v4's `public/schemas/qtap-export.schema.json`, `include_str!`'d
//! into the binary — the `help/**` arrangement, because v4 reads its copy from
//! `process.cwd()` and a Rust engine has no cwd to read it from. That makes a
//! v4 commit touching the schema a **re-vendor obligation**: this test fails
//! the moment the two files differ, so the obligation cannot be missed.
//!
//! The v4 checkout is optional here — a machine without one skips (the harness
//! convention), and the SELF-consistency half (the embedded copy parses, has
//! the expected `$schema`/`$id`, and compiles) always runs.
//!
//! Run standalone:
//!   QT_V4_ROOT=~/source/quilltap-server \
//!     cargo test -p quilltap-harness --test qtap_schema_embed_guard -- --nocapture

use std::path::PathBuf;

use quilltap_core::generators::qtap_schema::{validate_qtap_export, QTAP_EXPORT_SCHEMA_JSON};
use serde_json::{json, Value};

/// The vendored size at v4 `78b381a96` (P4.D171 — the route-trail message
/// field, `5841a8c62`; was 89,769 at `2f4254b42`).
const VENDORED_BYTES: usize = 92_797;

fn v4_root() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("QT_V4_ROOT") {
        return Some(PathBuf::from(p));
    }
    let default = PathBuf::from(std::env::var("HOME").ok()?).join("source/quilltap-server");
    default.is_dir().then_some(default)
}

#[test]
fn the_embedded_schema_is_self_consistent() {
    assert_eq!(
        QTAP_EXPORT_SCHEMA_JSON.len(),
        VENDORED_BYTES,
        "the vendored schema's size moved — re-vendor deliberately, then update this constant"
    );
    let schema: Value = serde_json::from_str(QTAP_EXPORT_SCHEMA_JSON).expect("the schema parses");
    assert_eq!(
        schema["$schema"],
        json!("https://json-schema.org/draft/2020-12/schema")
    );
    assert_eq!(
        schema["$id"],
        json!("https://quilltap.app/schemas/qtap-export.schema.json")
    );
    // Every `$ref` is LOCAL — the engine is built with no resolver, and a
    // remote `$ref` would fail the compile rather than reach the network.
    let refs: Vec<&str> = collect_refs(&schema);
    assert!(!refs.is_empty(), "the schema carries $refs");
    for r in &refs {
        assert!(
            r.starts_with("#/$defs/"),
            "non-local $ref {r:?} — the engine has no resolver and must never need one"
        );
    }
    // It compiles (a compile failure would otherwise surface only as the
    // `Schema validation error:` refusal on every call).
    let result = validate_qtap_export(&json!({}));
    assert!(
        !result
            .errors
            .iter()
            .any(|e| e.starts_with("Schema validation error:")),
        "the vendored schema failed to compile: {:?}",
        result.errors
    );
}

#[test]
fn the_embedded_schema_equals_the_v4_checkouts() {
    let Some(root) = v4_root() else {
        eprintln!("SKIP: no v4 checkout (set QT_V4_ROOT).");
        return;
    };
    let path = root.join("public/schemas/qtap-export.schema.json");
    let Ok(v4_bytes) = std::fs::read_to_string(&path) else {
        eprintln!("SKIP: {} is unreadable.", path.display());
        return;
    };
    assert!(
        v4_bytes == QTAP_EXPORT_SCHEMA_JSON,
        "the vendored qtap-export schema has DRIFTED from {} — v4 changed it, and the copy under \
         crates/quilltap-core/src/generators/ must be re-vendored (v4 {} bytes, vendored {} bytes)",
        path.display(),
        v4_bytes.len(),
        QTAP_EXPORT_SCHEMA_JSON.len()
    );
}

fn collect_refs(v: &Value) -> Vec<&str> {
    let mut out = Vec::new();
    fn walk<'a>(v: &'a Value, out: &mut Vec<&'a str>) {
        match v {
            Value::Object(o) => {
                for (k, val) in o {
                    if k == "$ref" {
                        if let Some(s) = val.as_str() {
                            out.push(s);
                        }
                    }
                    walk(val, out);
                }
            }
            Value::Array(a) => a.iter().for_each(|x| walk(x, out)),
            _ => {}
        }
    }
    walk(v, &mut out);
    out
}
