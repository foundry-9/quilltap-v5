//! The `regexp()` registration census (P4.D331 R-1, Tier 2 item 22).
//!
//! v4 registers its `regexp()` SQL function on ONE connection kind — the
//! LLM-logs client (`llm-logs-client.ts:56`, the only call site in `lib/`) —
//! so `x REGEXP 'y'` fails `no such function: REGEXP` on main and the mount
//! index. v5 keeps that observable by registering it ONLY in the openers'
//! `LlmLogs` arm: `Writer::open_writable_in` (`db/mod.rs`), the read pool's
//! `open_readonly` (`db/runtime.rs`) and `test_support::open_readonly_in`. A
//! new `register_regexp(` call anywhere else in core's production zone is a
//! red here, by name — and the every-connection `register_qt_text(` sites are
//! pinned UNMOVED beside it (the pattern this one must not copy).
//!
//! Run standalone:
//!   cargo test -p quilltap-harness --test regexp_registration_sites

mod source_census;

use source_census::{code_only, core_src_root, production_zone, rust_sources};

/// `(path under crates/quilltap-core/src, definitions, calls)` per identifier.
const REGEXP_CENSUS: &[(&str, usize, usize)] = &[
    ("db/mod.rs", 0, 1),
    ("db/regexp_function.rs", 1, 0),
    ("db/runtime.rs", 0, 1),
    ("test_support.rs", 0, 1),
];

const QT_TEXT_CENSUS: &[(&str, usize, usize)] = &[
    ("db/mod.rs", 0, 1),
    ("db/runtime.rs", 0, 1),
    ("db/text_compression.rs", 1, 0),
    ("test_support.rs", 0, 1),
];

/// `(definitions, calls)` of `ident` in a code-only zone; a match must not
/// continue an identifier on its left, and one preceded by `fn` is the
/// definition.
fn count(code: &str, ident: &str) -> (usize, usize) {
    let (mut defs, mut calls) = (0usize, 0usize);
    let mut from = 0usize;
    while let Some(at) = code[from..].find(ident) {
        let abs = from + at;
        let prev_ok = abs == 0
            || !code.as_bytes()[abs - 1].is_ascii_alphanumeric()
                && code.as_bytes()[abs - 1] != b'_';
        if prev_ok {
            if code[..abs].trim_end().ends_with("fn") {
                defs += 1;
            } else {
                calls += 1;
            }
        }
        from = abs + ident.len();
    }
    (defs, calls)
}

fn census_view(ident: &str) -> Vec<(String, usize, usize)> {
    let root = core_src_root();
    let mut files = Vec::new();
    rust_sources(&root, &mut files);
    files.sort();
    let mut out = Vec::new();
    for path in files {
        let rel = path
            .strip_prefix(&root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        let src = std::fs::read_to_string(&path).unwrap();
        let (defs, calls) = count(&code_only(&production_zone(&src)), ident);
        if defs + calls > 0 {
            out.push((rel, defs, calls));
        }
    }
    out
}

fn assert_census(ident: &str, want: &[(&str, usize, usize)], why: &str) {
    let view = census_view(ident);
    let got: Vec<(&str, usize, usize)> =
        view.iter().map(|(p, d, c)| (p.as_str(), *d, *c)).collect();
    assert_eq!(got, want.to_vec(), "{why}");
}

#[test]
fn regexp_is_registered_only_in_the_llm_logs_openers() {
    assert_census(
        "register_regexp(",
        REGEXP_CENSUS,
        "a `register_regexp(` call moved. v4 registers `regexp()` on its LLM-logs client \
         ONLY; v5 calls it in the openers' `LlmLogs` arm (db/mod.rs, db/runtime.rs, \
         test_support.rs) and nowhere else — a REGEXP that works on main is a divergence.",
    );
}

#[test]
fn the_every_connection_qt_text_sites_are_unmoved() {
    assert_census(
        "register_qt_text(",
        QT_TEXT_CENSUS,
        "the `register_qt_text(` sites moved — this census pins them beside the \
         partition-scoped `regexp()` registration (P4.D331).",
    );
}

/// The counter sees code only: a comment, a string, a `#[cfg(test)]` module
/// and a longer identifier do not count; a call and a definition do.
#[test]
fn the_counter_sees_only_code() {
    let src = r#"
        // register_regexp(&conn) in a comment
        const S: &str = "register_regexp(";
        pub fn register_regexp(conn: &Connection) {}
        fn not_register_regexp(x: u8) {}
        fn site() { register_regexp(&conn).unwrap(); }
        #[cfg(test)]
        mod tests { fn t() { register_regexp(&conn).unwrap(); } }
    "#;
    let code = code_only(&production_zone(src));
    assert_eq!(count(&code, "register_regexp("), (1, 1));
}
