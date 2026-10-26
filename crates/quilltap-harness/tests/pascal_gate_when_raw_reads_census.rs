//! The "no raw reads of a gate or a when" census (P4.D333 — the v5 twin of v4
//! `a53024d3c`'s source census in `custom-tool-definition.test.ts`).
//!
//! **Every reader of a gate, an outcome `when` or an effect condition goes
//! through its group accessor** — `ToolGate::gate_groups`, `When::when_groups`,
//! `EffectCondition::effect_when_groups` — so a reader written before any-of
//! existed cannot quietly treat an array as nothing (v4's census greps `lib/`,
//! `app/`, `components/` for `availableWhen?.metadata`-style reads and
//! sanctions only the schema module and the Workbench draft).
//!
//! v4's regex does not transfer: measured against v5 `main`, its Rust spelling
//! (`(available_when|withheld_when|\.when)\b[^;]*\.(metadata|…)\b` on a line
//! with no accessor) finds ZERO offenders, because v5's raw reads span lines
//! (`let When::Object(w) = when else {…}` and a later `w.metadata`). In Rust
//! the bypass is a PATTERN on the one-group variant — `When::Object`,
//! `ToolGate::Group`, `EffectCondition::Group` — that leaves the `AnyOf` arm
//! unhandled. So this census counts, per production file of
//! `crates/quilltap-core/src` (comments, strings and `#[cfg(test)]` items
//! stripped by the shared lexer; the schema module exempt), every spelling of
//! a one-group variant and every spelling of its `AnyOf` twin, and sanctions
//! exactly the three exhaustive dispatches that handle both: `matches_when`,
//! `matches_effect_when` and `describe_when`. Measured on `main` the census
//! named `collect_metadata_tested`, `tool_vocabulary.rs`'s outcome loop and
//! `describe_when` as one-arm readers — the red this lane turned green.
//!
//! Run standalone:
//!   cargo test -p quilltap-harness --test pascal_gate_when_raw_reads_census

mod source_census;

use source_census::{code_only, core_src_root, production_zone, rust_sources};

/// `(one-group variant, its any-of twin)` per group-bearing type.
const VARIANTS: [(&str, &str); 3] = [
    ("When::Object", "When::AnyOf"),
    ("ToolGate::Group", "ToolGate::AnyOf"),
    ("EffectCondition::Group", "EffectCondition::AnyOf"),
];

/// The schema module declares the types and builds them; it is v4's exempt
/// `custom-tool.types.ts`.
const EXEMPT: &[&str] = &["pascal/custom_tool_types.rs"];

/// `(path, variant, one-group spellings, any-of spellings, why)`.
const CENSUS: &[(&str, &str, usize, usize, &str)] = &[
    (
        "pascal/custom_tools.rs",
        "When::Object",
        1,
        1,
        "`matches_when` — the outcome dispatch: catch-all, one group, any-of.",
    ),
    (
        "pascal/custom_tools.rs",
        "EffectCondition::Group",
        1,
        1,
        "`matches_effect_when` — the effect-condition dispatch.",
    ),
    (
        "tools/run_custom.rs",
        "When::Object",
        1,
        1,
        "`describe_when` — `a and b` for one group, `(a and b) or c` for an any-of.",
    ),
];

/// Spellings of `ident` not continuing an identifier on either side.
fn count(code: &str, ident: &str) -> usize {
    let bytes = code.as_bytes();
    let mut n = 0usize;
    let mut from = 0usize;
    while let Some(at) = code[from..].find(ident) {
        let abs = from + at;
        let end = abs + ident.len();
        let left_ok =
            abs == 0 || !(bytes[abs - 1].is_ascii_alphanumeric() || bytes[abs - 1] == b'_');
        let right_ok =
            end >= bytes.len() || !(bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_');
        if left_ok && right_ok {
            n += 1;
        }
        from = end;
    }
    n
}

fn census_view(sources: &[(String, String)]) -> Vec<(String, &'static str, usize, usize)> {
    let mut out = Vec::new();
    for (rel, src) in sources {
        if EXEMPT.contains(&rel.as_str()) {
            continue;
        }
        let code = code_only(&production_zone(src));
        for (one, any) in VARIANTS {
            let (o, a) = (count(&code, one), count(&code, any));
            if o + a > 0 {
                out.push((rel.clone(), one, o, a));
            }
        }
    }
    out
}

#[test]
fn every_gate_and_when_reader_goes_through_the_group_accessors() {
    let root = core_src_root();
    let mut files = Vec::new();
    rust_sources(&root, &mut files);
    files.sort();
    let sources: Vec<(String, String)> = files
        .iter()
        .map(|path| {
            let rel = path
                .strip_prefix(&root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            (rel, std::fs::read_to_string(path).unwrap())
        })
        .collect();
    let view = census_view(&sources);
    let got: Vec<(&str, &str, usize, usize)> = view
        .iter()
        .map(|(p, v, o, a)| (p.as_str(), *v, *o, *a))
        .collect();
    let want: Vec<(&str, &str, usize, usize)> = CENSUS
        .iter()
        .map(|(p, v, o, a, _)| (*p, *v, *o, *a))
        .collect();
    assert_eq!(
        got, want,
        "a production reader pattern-matches a gate / when / effect condition. Read it through \
         `gate_groups()` / `when_groups()` / `effect_when_groups()` (v4 `a53024d3c`'s \
         `gateGroups` / `whenGroups` / `effectWhenGroups`) so an any-of array is never read as \
         nothing — rather than adding a row here."
    );
}

/// The counter sees code only, and a one-arm reader is a red.
#[test]
fn the_census_sees_a_one_arm_reader() {
    let one_arm = r#"
        // let When::Object(w) = when else { return None; } — a comment
        const S: &str = "When::Object";
        fn read(when: &When) -> usize {
            let When::Object(w) = when else { return 0 };
            w.metadata.iter().count()
        }
        #[cfg(test)]
        mod tests { fn t() { let _ = When::Object(Box::default()); } }
    "#;
    let view = census_view(&[("pascal/somewhere.rs".to_string(), one_arm.to_string())]);
    assert_eq!(
        view,
        vec![("pascal/somewhere.rs".to_string(), "When::Object", 1, 0)]
    );

    let exhaustive = r#"
        fn read(when: &When) -> bool {
            match when { When::CatchAll(_) => true, When::Object(w) => go(w), When::AnyOf(v) => any(v) }
        }
    "#;
    let view = census_view(&[("pascal/somewhere.rs".to_string(), exhaustive.to_string())]);
    assert_eq!(
        view,
        vec![("pascal/somewhere.rs".to_string(), "When::Object", 1, 1)]
    );
    assert_eq!(
        count("MyWhen::Object(x)", "When::Object"),
        0,
        "a longer name is not ours"
    );
    assert_eq!(
        count("custom_tool_types::When::Object(x)", "When::Object"),
        1,
        "a qualified path is"
    );
}
