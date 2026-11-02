//! Tier-1 differential (P4.D354, v4 `40fab1c54`, bug 261): `doc_list_files`'
//! glob — `quilltap_core::doc_edit::path_glob::PathGlob` vs v4's REAL
//! `compilePathGlob` (`lib/doc-edit/path-glob.ts`).
//!
//! Two groups, exact booleans: `suite` — every `expect` pair of v4's own
//! `path-glob.test.ts`; `grid` — the 40-pattern × 30-path edge grid (the
//! `**/` / brace / class / metacharacter / malformed / astral / non-ASCII
//! corners). The grid's patterns that fall to the literal arm (a source the
//! RegExp engine refuses) are counted and printed — the same count on both
//! sides is implied by every row matching.
//!
//! Generate the oracle output:
//!   cd ~/source/quilltap-server
//!   npx tsx ~/source/quilltap-v5/harness/oracle/cases/path-glob.ts \
//!     > /tmp/oracle-path-glob.ndjson
//! Run:
//!   QT_ORACLE_PATH_GLOB=/tmp/oracle-path-glob.ndjson \
//!     cargo test -p quilltap-harness --test path_glob_equivalence

use quilltap_core::doc_edit::path_glob::PathGlob;
use serde::Deserialize;

#[derive(Deserialize)]
struct Row {
    group: String,
    pattern: String,
    path: String,
    matches: bool,
}

#[test]
fn path_glob_matches_oracle() {
    let Ok(path) = std::env::var("QT_ORACLE_PATH_GLOB") else {
        eprintln!("SKIP: QT_ORACLE_PATH_GLOB not set");
        return;
    };
    let text = std::fs::read_to_string(&path).expect("read oracle");
    let rows: Vec<Row> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("parse row"))
        .collect();
    let mut red = Vec::new();
    let mut literal_patterns = std::collections::BTreeSet::new();
    let (mut suite, mut grid) = (0usize, 0usize);
    for r in &rows {
        let glob = PathGlob::compile(&r.pattern);
        if glob.is_literal() {
            literal_patterns.insert(r.pattern.clone());
        }
        let got = glob.matches(&r.path);
        if got != r.matches {
            red.push(format!(
                "[{}] {:?} vs {:?}: rust {got}, v4 {}",
                r.group, r.pattern, r.path, r.matches
            ));
        }
        match r.group.as_str() {
            "suite" => suite += 1,
            _ => grid += 1,
        }
    }
    assert!(
        red.is_empty(),
        "{} row(s) red:\n{}",
        red.len(),
        red.join("\n")
    );
    assert_eq!(suite, 28, "the suite group lost a row");
    assert_eq!(grid, 40 * 30, "the grid lost a row");
    eprintln!(
        "OK: path-glob matched oracle ({suite} suite + {grid} grid rows; {} pattern(s) on the literal arm: {:?}).",
        literal_patterns.len(),
        literal_patterns
    );
}
