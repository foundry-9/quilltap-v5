//! The `zod_issues` home guard (P4.101) — the ninth `invalid_type` has to be
//! argued for.
//!
//! Zod's issue objects ride v4's wire verbatim (`validationError(err)` →
//! `{error: 'Validation error', details: err.issues}`, and a `ZodError` that
//! escapes carries `JSON.stringify(issues, null, 2)` as its message), so an
//! issue's KEY ORDER is contractual. Before P4.101 the tree carried **eight**
//! `invalid_type` constructors — one per route family as it was ported — in
//! three carrier shapes and three `path` representations. They all happened to
//! agree (measured against the checkout's real `zod` 4.5.4 before the fold),
//! but nothing held them together: a Zod bump that reordered one code's keys
//! would have had to be chased through eight files, and a ninth copy could be
//! added without anyone noticing.
//!
//! This walks `crates/quilltap-core/src` and holds every `fn invalid_type` /
//! `fn invalid_uuid` / `fn invalid_value` / `fn invalid_enum` /
//! `fn invalid_literal` / `fn invalid_int_type` DEFINITION against the census
//! below. **The census IS the arithmetic:** the ONE home plus the Tier-3
//! remainder P4.101 deliberately did not converge.
//!
//! The same walk holds the `parsedType` word — v4 `util.parsedType`, the table
//! that turns a received value into `"undefined"` / `"null"` / `"boolean"` /
//! … — to its recorded copies. P4.98 retired the fourth copy onto
//! `api/settings.rs`; P4.101 moved that home to `api/zod_issues.rs` and
//! retired four more. What is left is named below with WHY, because each
//! remaining one answers a different question (a `&Value` rather than an
//! `Option<&Value>`, i.e. a caller for whom "absent" is unreachable) or
//! belongs to a Tier-3 surface with its own client-side twin.
//!
//! Mutation M1: add a ninth `fn invalid_type` anywhere under
//! `crates/quilltap-core/src` → this test reddens by name.
//!
//! Sibling pins: `api::zod_issues::tests::render_table_matches_real_zod_454`
//! (the per-code key order against real zod 4.5.4) and the families listed in
//! the P4.101 lane record (the neutrality proof).
//!
//! Run standalone:
//!   cargo test -p quilltap-harness --test zod_issues_home_guard

use std::path::{Path, PathBuf};

/// `(repo-relative path, expected constructor definitions, why)`.
///
/// **Arithmetic:** 5 (the home's own `invalid_*` constructors, associated
/// functions on `ZodIssue`) + 1 (`pascal/custom_tool_types.rs`) + 1
/// (`progressions/schema.rs`) = **7 definitions in 3 files**, measured down
/// from **17 in 9 files** at `main` `57fd1680` (`generators_detail` 2,
/// `generators_wizard` 3, `prompt_templates` 1, `settings` 3, `subprompts` 1,
/// `lora_validation` 1, `pascal` 1, `progressions` 1, `chat_create` 4).
/// Everything else in the tree calls `crate::api::zod_issues::ZodIssue::…`.
const CONSTRUCTOR_CENSUS: &[(&str, usize, &str)] = &[
    (
        "crates/quilltap-core/src/api/zod_issues.rs",
        5,
        "THE HOME — `invalid_type`, `invalid_int_type`, `invalid_value`, \
         `invalid_literal` and `invalid_uuid` (their `too_*` siblings are not \
         in the needle list). Every route family's issue bag is built from \
         these.",
    ),
    (
        "crates/quilltap-core/src/pascal/custom_tool_types.rs",
        1,
        "TIER-3 REMAINDER (P4.101): the Pascal custom-tool Zod port renders \
         STRING sentences for a different v4 surface (`customToolSchema`'s \
         own error prose), not issue OBJECTS, and its client-safe TS twin \
         mirrors those sentences. Converging it would change what that \
         surface says, which is a port, not a refactor.",
    ),
    (
        "crates/quilltap-core/src/progressions/schema.rs",
        1,
        "TIER-3 REMAINDER (P4.101): the progressions shim, whose `ZodIssue` \
         struct is mirrored 1:1 by the SPA's `pascal/zod-shim.ts` (P4.D170). \
         Converging it here would desynchronize the twin, which is the one \
         thing that family's differential cannot see.",
    ),
];

/// The `util.parsedType` table. **Arithmetic:** 1 (the home) + 5 (the
/// recorded remainder below) = **6 in 6 files**, measured down from **11 in
/// 11** at `main` `57fd1680` — P4.101 retired `prompt_templates.rs`'s and
/// `subprompts.rs`'s `zod_received`, `chat_create.rs`'s and
/// `lora_validation.rs`'s `parsed_type`, and `run_sql.rs`'s `json_typeof`
/// (the `&Value` form for a caller that had no `undefined` arm), and moved
/// `settings.rs`'s `zod_parsed_type` into the home.
const PARSED_TYPE_CENSUS: &[(&str, usize, &str)] = &[
    (
        "crates/quilltap-core/src/api/zod_issues.rs",
        1,
        "THE HOME — `zod_parsed_type`, the `Option<&Value>` form every issue \
         constructor reads.",
    ),
    (
        "crates/quilltap-core/src/api/chat_outfits.rs",
        1,
        "`received` returns a `String` and is a SENTENCE helper for a \
         hand-written refusal, not an issue field (P4.101 left it; it is not \
         this home's shape).",
    ),
    (
        "crates/quilltap-core/src/api/photos.rs",
        1,
        "`zod_received(&Value)` — a caller for whom `undefined` is \
         unreachable, so the table has no `None` arm to share.",
    ),
    (
        "crates/quilltap-core/src/photos/save_attribution.rs",
        1,
        "the same `&Value` shape as `api/photos.rs`, on a sibling surface.",
    ),
    (
        "crates/quilltap-core/src/progressions/schema.rs",
        1,
        "TIER-3 REMAINDER — the SPA-mirrored shim (see above).",
    ),
    (
        "crates/quilltap-core/src/pascal/custom_tool_types.rs",
        1,
        "TIER-3 REMAINDER — the Pascal sentence port (see above).",
    ),
];

/// The constructor spellings a definition can carry. `fn ` + the name + `(`
/// matches a definition and not a call.
const CONSTRUCTOR_NEEDLES: &[&str] = &[
    "fn invalid_type(",
    "fn invalid_int_type(",
    "fn invalid_uuid(",
    "fn invalid_value(",
    "fn invalid_enum(",
    "fn invalid_literal(",
];

/// The `parsedType` spellings the tree has used.
const PARSED_TYPE_NEEDLES: &[&str] = &[
    "fn zod_parsed_type(",
    "fn parsed_type(",
    "fn zod_received(",
    "fn json_typeof(",
    "fn received(",
];

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("harness crate sits two levels under the repo root")
        .to_path_buf()
}

fn rust_sources(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = std::fs::read_dir(dir).unwrap_or_else(|e| panic!("read {}: {e}", dir.display()));
    for entry in entries {
        let path = entry.expect("dir entry").path();
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        if path.is_dir() {
            if name == "target" || name == "vendor" {
                continue;
            }
            rust_sources(&path, out);
        } else if name.ends_with(".rs") {
            out.push(path);
        }
    }
}

/// Count the needles in `text`, ignoring `#[cfg(test)]` modules — a test may
/// legitimately define a local helper of the same name, and a census that
/// counted those would be a census of the test suite.
/// (`an-in-file-source-census-must-strip-test-modules-by-braces`.)
fn strip_test_modules(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let bytes = text.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        let rest = &text[i..];
        if rest.starts_with("#[cfg(test)]") {
            // Skip to the opening brace of the item, then to its match.
            let Some(open_rel) = rest.find('{') else {
                break;
            };
            let mut depth = 0i32;
            let mut j = i + open_rel;
            let mut in_line_comment = false;
            let mut in_block_comment = false;
            let mut in_string = false;
            let mut escaped = false;
            while j < bytes.len() {
                let c = bytes[j] as char;
                let next = bytes.get(j + 1).map(|b| *b as char).unwrap_or('\0');
                if in_line_comment {
                    if c == '\n' {
                        in_line_comment = false;
                    }
                } else if in_block_comment {
                    if c == '*' && next == '/' {
                        in_block_comment = false;
                        j += 1;
                    }
                } else if in_string {
                    if escaped {
                        escaped = false;
                    } else if c == '\\' {
                        escaped = true;
                    } else if c == '"' {
                        in_string = false;
                    }
                } else if c == '/' && next == '/' {
                    in_line_comment = true;
                    j += 1;
                } else if c == '/' && next == '*' {
                    in_block_comment = true;
                    j += 1;
                } else if c == '"' {
                    in_string = true;
                } else if c == '{' {
                    depth += 1;
                } else if c == '}' {
                    depth -= 1;
                    if depth == 0 {
                        j += 1;
                        break;
                    }
                }
                j += 1;
            }
            i = j;
            continue;
        }
        let ch = text[i..].chars().next().expect("in bounds");
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

fn count(text: &str, needles: &[&str]) -> usize {
    needles.iter().map(|n| text.matches(n).count()).sum()
}

fn walk_and_check(
    census: &[(&str, usize, &str)],
    needles: &[&str],
    label: &str,
    advice: &str,
) -> Vec<String> {
    let root = repo_root();
    let mut files = Vec::new();
    rust_sources(&root.join("crates/quilltap-core/src"), &mut files);
    files.sort();
    assert!(
        files.len() > 100,
        "the walk found only {} rust files under quilltap-core — it is not \
         reaching the tree",
        files.len()
    );

    let mut failures: Vec<String> = Vec::new();
    let mut seen: Vec<(String, usize)> = Vec::new();

    for path in &files {
        let raw = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
        let text = strip_test_modules(&raw);
        let n = count(&text, needles);
        if n == 0 {
            continue;
        }
        let rel = path
            .strip_prefix(&root)
            .expect("under the repo root")
            .to_string_lossy()
            .replace('\\', "/");
        seen.push((rel.clone(), n));
        match census.iter().find(|(p, ..)| *p == rel) {
            None => failures.push(format!(
                "{rel}: {n} {label} definition(s) outside the census. {advice}"
            )),
            Some((_, expected, _)) if n != *expected => failures.push(format!(
                "{rel}: {n} {label} definition(s), census says {expected}. {advice}"
            )),
            Some(_) => {}
        }
    }
    for (rel, expected, why) in census {
        if !seen.iter().any(|(p, _)| p == rel) {
            failures.push(format!(
                "{rel}: census expects {expected} {label} definition(s), found \
                 none. If the home moved, move the census with it ({why})."
            ));
        }
    }
    failures
}

#[test]
fn zod_issue_constructors_live_in_one_home() {
    let failures = walk_and_check(
        CONSTRUCTOR_CENSUS,
        CONSTRUCTOR_NEEDLES,
        "Zod issue constructor",
        "Zod's issue key order is contractual and there is ONE place that \
         owns it: `crate::api::zod_issues::ZodIssue`. Call it rather than \
         growing a ninth copy; if this really is a different v4 surface with \
         a different rendering, say so in the census with the measurement \
         (P4.101).",
    );
    assert!(
        failures.is_empty(),
        "the `invalid_type` copies are regrowing:\n  {}",
        failures.join("\n  ")
    );
}

#[test]
fn the_parsed_type_word_has_one_home_plus_its_recorded_remainder() {
    let failures = walk_and_check(
        PARSED_TYPE_CENSUS,
        PARSED_TYPE_NEEDLES,
        "`util.parsedType`",
        "v4's `util.parsedType` table lives at \
         `crate::api::zod_issues::zod_parsed_type`. A new copy wants a reason \
         in the census (P4.98, P4.101).",
    );
    assert!(
        failures.is_empty(),
        "the `parsedType` word is regrowing:\n  {}",
        failures.join("\n  ")
    );
}
