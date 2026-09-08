//! The vendored `public/schemas/` guard (P4.D170, Tier 1 item 5).
//!
//! `apps/web/public/schemas/{qtap-custom-tool,qtap-progression}.schema.json`
//! are byte-copies of v4's files of the same names, SERVED by the SPA — the
//! `$schema` URL `pascal/tool-draft.ts` writes into every tool draft resolves
//! to the first of them, and the progressions help page points a hand-editor at
//! the second. That makes a v4 commit touching either a **re-vendor
//! obligation**, and until this file existed nothing went red when one was
//! missed: the drift ledger's standing hazard (10) recorded the custom-tool
//! schema as the fifth vendored artifact and the only UNGUARDED one, which is
//! exactly how it sat 529 lines behind v4 through the `0587d1e96` drift.
//!
//! Shaped after `qtap_schema_embed_guard.rs`, with one difference: these two
//! are files the SPA serves rather than strings a crate embeds, so the guard
//! reads them off disk. The v4 checkout is optional — a machine without one
//! skips (the harness convention) — and the SELF-consistency half (each parses,
//! carries the expected `$schema`/`$id`, and matches a pinned byte size) always
//! runs.
//!
//! Run standalone:
//!   QT_V4_ROOT=/tmp/qt-v4-pin-p4d170-25f534c0b \
//!     cargo test -p quilltap-harness --test public_schemas_vendor_guard -- --nocapture
//!
//! ⚠ Against the LIVE checkout this is green only while v4 sits at the file
//! contents this lane vendored. It goes red the moment v4 moves either file —
//! by design, and that redness is the obligation, not a bug.

use std::path::{Path, PathBuf};

use serde_json::{json, Value};

/// One vendored schema: where v5 serves it, where v4 keeps it, and its facts.
struct Vendored {
    /// Path under the repo root.
    v5: &'static str,
    /// Path under the v4 checkout root.
    v4: &'static str,
    /// The byte size at v4 `25f534c0b`.
    bytes: usize,
    /// The `$id` the file must carry.
    id: &'static str,
}

const SCHEMAS: &[Vendored] = &[
    Vendored {
        v5: "apps/web/public/schemas/qtap-custom-tool.schema.json",
        v4: "public/schemas/qtap-custom-tool.schema.json",
        bytes: 39_471,
        id: "https://quilltap.ai/schemas/qtap-custom-tool.schema.json",
    },
    Vendored {
        v5: "apps/web/public/schemas/qtap-progression.schema.json",
        v4: "public/schemas/qtap-progression.schema.json",
        bytes: 5_963,
        id: "https://quilltap.ai/schemas/qtap-progression.schema.json",
    },
];

/// The repo root, from this crate's manifest directory.
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("crates/<crate>/ sits two levels under the repo root")
        .to_path_buf()
}

fn v4_root() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("QT_V4_ROOT") {
        return Some(PathBuf::from(p));
    }
    let default = PathBuf::from(std::env::var("HOME").ok()?).join("source/quilltap-server");
    default.is_dir().then_some(default)
}

#[test]
fn the_vendored_schemas_are_self_consistent() {
    let root = repo_root();
    for s in SCHEMAS {
        let path = root.join(s.v5);
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("{} is unreadable: {e}", path.display()));

        assert_eq!(
            text.len(),
            s.bytes,
            "{}'s size moved — re-vendor deliberately, then update this constant",
            s.v5
        );

        let schema: Value = serde_json::from_str(&text)
            .unwrap_or_else(|e| panic!("{} does not parse: {e}", path.display()));
        assert_eq!(
            schema["$schema"],
            json!("https://json-schema.org/draft/2020-12/schema"),
            "{} names an unexpected meta-schema",
            s.v5
        );
        assert_eq!(
            schema["$id"],
            json!(s.id),
            "{} names an unexpected $id",
            s.v5
        );

        // Every `$ref` is LOCAL. These files are served to a browser, and a
        // remote `$ref` would put a validator on the network — the same rule
        // `qtap_schema_embed_guard` holds for the embedded export schema.
        for r in collect_refs(&schema) {
            assert!(
                r.starts_with("#/$defs/") || r.starts_with("#/"),
                "non-local $ref {r:?} in {}",
                s.v5
            );
        }
    }
}

#[test]
fn the_vendored_schemas_equal_the_v4_checkouts() {
    let Some(v4) = v4_root() else {
        eprintln!("SKIP: no v4 checkout (set QT_V4_ROOT).");
        return;
    };
    let root = repo_root();
    for s in SCHEMAS {
        let v4_path = v4.join(s.v4);
        let Ok(v4_bytes) = std::fs::read_to_string(&v4_path) else {
            eprintln!("SKIP: {} is unreadable.", v4_path.display());
            continue;
        };
        let ours = std::fs::read_to_string(root.join(s.v5)).expect("the vendored copy is readable");
        assert!(
            v4_bytes == ours,
            "the vendored {} has DRIFTED from {} — v4 changed it, and the copy the SPA serves must \
             be re-vendored (v4 {} bytes, vendored {} bytes)",
            s.v5,
            v4_path.display(),
            v4_bytes.len(),
            ours.len()
        );
    }
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
