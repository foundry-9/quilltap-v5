//! `db::fallback` is the ONE home for v4's repository-layer fallback lines
//! (CLAUDE.md §R.6 of every round since the `97b25fc53` follow-ups
//! unification, which found three hand-copies of one v4 line under three
//! tracing targets; the `97b25fc53` smalls unification found seven more of
//! `Error finding entity by ID` under module targets, plus a local twin of
//! `find_all_or_empty`). A twin written by hand lands under the caller's
//! module target, which the differentials — mapping v4's `Repository` logger
//! to `quilltap::db` — cannot see, so a re-aimed test stays green on a line
//! v4 never logs there.
//!
//! This guard scans the production zone of every core source file (test
//! modules stripped by braces, comments dropped, STRING LITERALS KEPT — the
//! message IS a literal) for each home message as a whole literal, and
//! allows it in `db/fallback.rs` alone. A new emitter anywhere else is red:
//! fold it onto the home's fn (or grow the home by one fn for a new v4 shape).
//!
//! Run standalone:
//!   cargo test -p quilltap-harness --test fallback_home_guard

mod source_census;

use source_census::{core_src_root, production_zone, rust_sources, string_literals};

/// Every line the home emits — the message literal, exactly as `tracing!`
/// carries it. Grows only with `db/fallback.rs`.
const HOME_MESSAGES: &[&str] = &[
    "Error finding entity by ID",
    "Error finding all entities",
    "Error finding entities by filter",
    "Error finding entity by filter",
    "Error querying joined file links",
    "Error finding document by mount point and path",
    "Error deleting file link with GC",
];

const HOME: &str = "db/fallback.rs";

#[test]
fn every_home_line_is_emitted_only_by_the_home() {
    let root = core_src_root();
    let mut files = Vec::new();
    rust_sources(&root, &mut files);
    files.sort();
    let mut home_seen = 0usize;
    let mut offenders: Vec<String> = Vec::new();
    for f in &files {
        let rel = f
            .strip_prefix(&root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        let src = std::fs::read_to_string(f).unwrap();
        let zone = production_zone(&src);
        for lit in string_literals(&zone) {
            let body = lit
                .trim_start_matches('r')
                .trim_matches('#')
                .trim_matches('"');
            if !HOME_MESSAGES.contains(&body) {
                continue;
            }
            if rel == HOME {
                home_seen += 1;
            } else {
                offenders.push(format!("{rel}: {body:?}"));
            }
        }
    }
    assert_eq!(
        home_seen,
        HOME_MESSAGES.len(),
        "the home must emit each of its {} messages exactly once (HOME_MESSAGES drifted from `db/fallback.rs`)",
        HOME_MESSAGES.len()
    );
    assert!(
        offenders.is_empty(),
        "v4 repository-layer fallback lines emitted OUTSIDE `db::fallback` (fold each onto the home):\n  {}",
        offenders.join("\n  ")
    );
}

/// The scanner sees a literal in production code and not in a test module —
/// the guard is only as good as its zone.
#[test]
fn the_scanner_sees_production_literals_and_not_test_ones() {
    let src = r#"
fn prod() { tracing::error!(target: "quilltap::db", "Error finding entity by ID"); }
#[cfg(test)]
mod tests {
    #[test]
    fn t() { assert!(l.contains("Error finding all entities")); }
}
"#;
    let zone = production_zone(src);
    let lits: Vec<&str> = string_literals(&zone)
        .into_iter()
        .map(|l| l.trim_matches('"'))
        .collect();
    assert!(lits.contains(&"Error finding entity by ID"), "{lits:?}");
    assert!(!lits.contains(&"Error finding all entities"), "{lits:?}");
}
