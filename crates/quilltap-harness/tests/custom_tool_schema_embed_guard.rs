//! The vendored custom-tool definition format guard (P4.D332, R-E).
//!
//! `pascal/specimens/` holds byte-copies of v4's
//! `public/schemas/qtap-custom-tool.schema.json`,
//! `docs/developer/CUSTOM_TOOL_SPEC.json` and
//! `docs/developer/CUSTOM_TOOL_SPEC_DICE.json` at `322a5488e`, `include_str!`'d
//! so `edit_custom_tool`'s `action: "schema"` can serve them (v4 imports the
//! three at runtime; a Rust engine has no cwd to read them from). The same
//! bytes ship in two other places — the SPA's
//! `apps/web/public/schemas/qtap-custom-tool.schema.json` (P4.D333) and the
//! `docs/v4/developer/` mirrors (P4.D339) — and the three-way equality below is
//! the union's tripwire: on this lane's own branch the SPA-copy leg and the
//! range-specimen mirror leg are RED BY DESIGN until those lanes land.
//!
//! The v4 checkout is optional — a machine without one skips (the harness
//! convention); the self-consistency halves always run.
//!
//! Run standalone:
//!   QT_V4_CHECKOUT=<the 322a5488e pin> QT_V4_ROOT=<the 322a5488e pin> \
//!     cargo test -p quilltap-harness --test custom_tool_schema_embed_guard -- --nocapture

use quilltap_core::pascal::custom_tool_schema::{
    CUSTOM_TOOL_SPEC_DICE_JSON, CUSTOM_TOOL_SPEC_JSON, QTAP_CUSTOM_TOOL_SCHEMA_JSON,
};
use serde_json::{json, Value};
use std::path::PathBuf;

mod common;

const SCHEMA_BYTES: usize = 63_595;
const SPEC_BYTES: usize = 12_660;
const DICE_BYTES: usize = 1_951;

fn v5_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn the_embedded_schema_is_self_consistent() {
    assert_eq!(
        QTAP_CUSTOM_TOOL_SCHEMA_JSON.len(),
        SCHEMA_BYTES,
        "the vendored schema's size moved — re-vendor deliberately, then update this constant"
    );
    let schema: Value =
        serde_json::from_str(QTAP_CUSTOM_TOOL_SCHEMA_JSON).expect("the schema parses");
    assert_eq!(
        schema["$schema"],
        json!("https://json-schema.org/draft/2020-12/schema")
    );
    assert_eq!(
        schema["$id"],
        json!("https://quilltap.ai/schemas/qtap-custom-tool.schema.json")
    );
    let refs = collect_refs(&schema);
    for r in &refs {
        assert!(
            r.starts_with('#'),
            "non-local $ref {r:?} — the engine has no resolver"
        );
    }
}

#[test]
fn the_embedded_specimens_parse() {
    assert_eq!(CUSTOM_TOOL_SPEC_JSON.len(), SPEC_BYTES);
    assert_eq!(CUSTOM_TOOL_SPEC_DICE_JSON.len(), DICE_BYTES);
    for (label, text) in [
        ("CUSTOM_TOOL_SPEC.json", CUSTOM_TOOL_SPEC_JSON),
        ("CUSTOM_TOOL_SPEC_DICE.json", CUSTOM_TOOL_SPEC_DICE_JSON),
    ] {
        let v: Value = serde_json::from_str(text).unwrap_or_else(|e| panic!("{label}: {e}"));
        assert!(v.is_object(), "{label} is an object");
        assert!(v["name"].is_string(), "{label} carries a name");
    }
}

/// RED on this lane's own branch until P4.D333 re-vendors the SPA's copy.
#[test]
fn the_embedded_schema_equals_the_spa_copy() {
    let path = v5_root().join("apps/web/public/schemas/qtap-custom-tool.schema.json");
    let spa = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    assert!(
        spa == QTAP_CUSTOM_TOOL_SCHEMA_JSON,
        "the SPA's schema copy ({} bytes) differs from the engine's embed ({} bytes) — the two \
         must be re-vendored together (P4.D333 owns the SPA copy)",
        spa.len(),
        QTAP_CUSTOM_TOOL_SCHEMA_JSON.len()
    );
}

/// The range leg is RED on this lane's own branch until P4.D339 re-vendors the
/// mirror; the dice leg is unchanged in the span and GREEN.
#[test]
fn the_embedded_specimens_equal_the_docs_mirror() {
    for (file, embedded) in [
        ("CUSTOM_TOOL_SPEC.json", CUSTOM_TOOL_SPEC_JSON),
        ("CUSTOM_TOOL_SPEC_DICE.json", CUSTOM_TOOL_SPEC_DICE_JSON),
    ] {
        let path = v5_root().join("docs/v4/developer").join(file);
        let mirror =
            std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        assert!(
            mirror == embedded,
            "docs/v4/developer/{file} ({} bytes) differs from the engine's embed ({} bytes) — \
             P4.D339 owns the mirror",
            mirror.len(),
            embedded.len()
        );
    }
}

#[test]
fn the_embeds_equal_the_v4_checkouts() {
    let Some(root) = common::v4_root() else {
        eprintln!("SKIP: no v4 checkout (set QT_V4_CHECKOUT).");
        return;
    };
    for (rel, embedded) in [
        (
            "public/schemas/qtap-custom-tool.schema.json",
            QTAP_CUSTOM_TOOL_SCHEMA_JSON,
        ),
        (
            "docs/developer/CUSTOM_TOOL_SPEC.json",
            CUSTOM_TOOL_SPEC_JSON,
        ),
        (
            "docs/developer/CUSTOM_TOOL_SPEC_DICE.json",
            CUSTOM_TOOL_SPEC_DICE_JSON,
        ),
    ] {
        let path = root.join(rel);
        let Ok(v4) = std::fs::read_to_string(&path) else {
            eprintln!("SKIP: {} is unreadable.", path.display());
            continue;
        };
        assert!(
            v4 == embedded,
            "the vendored {rel} has DRIFTED from {} (v4 {} bytes, vendored {} bytes)",
            path.display(),
            v4.len(),
            embedded.len()
        );
    }
}

fn collect_refs(v: &Value) -> Vec<&str> {
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
    let mut out = Vec::new();
    walk(v, &mut out);
    out
}
