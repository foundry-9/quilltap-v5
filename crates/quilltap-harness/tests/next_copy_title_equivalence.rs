//! Tier-1 differential (P4.D309 — v4 `bbbcd2388`, copy to folder): the
//! `(copy)` title helper folder filing calls server-side.
//!
//! Diffs [`quilltap_core::wardrobe::next_copy_title`] against v4's REAL
//! `lib/wardrobe/next-copy-title.ts` (`nextCopyTitle`), exact strings. v4 has no
//! unit suite for the helper; the corpus is authored (the order's §4.1 list):
//! the `(copy)` chain, the suffix strip over JS `\s` / ASCII `\d` / ASCII `/i`,
//! only-the-last-suffix, the blank fallback, padded / cased / non-ASCII
//! existing titles, astral characters, a long title, a long chain.
//!
//! Generate the oracle output (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   $N/node --import tsx $V5W/harness/oracle/cases/next-copy-title.ts \
//!     > /tmp/oracle-next-copy-title.ndjson
//! Run:
//!   QT_ORACLE_NEXT_COPY_TITLE=/tmp/oracle-next-copy-title.ndjson \
//!     cargo test -p quilltap-harness --test next_copy_title_equivalence -- --nocapture

use quilltap_core::wardrobe::next_copy_title;
use serde::Deserialize;

/// The corpus's own marker — a stale NDJSON cannot pass quietly.
const BASELINE: &str = "p4.d309-next-copy-title";

#[derive(Deserialize)]
#[serde(tag = "kind")]
enum Row {
    #[serde(rename = "meta")]
    Meta { baseline: String },
    #[serde(rename = "case")]
    Case {
        id: String,
        source: String,
        existing: Vec<String>,
        out: String,
    },
}

#[test]
fn next_copy_title_matches_oracle() {
    let Ok(path) = std::env::var("QT_ORACLE_NEXT_COPY_TITLE") else {
        eprintln!("SKIP: set QT_ORACLE_NEXT_COPY_TITLE to the oracle NDJSON (see test header).");
        return;
    };
    let text = std::fs::read_to_string(&path).expect("read oracle NDJSON");
    let mut saw_meta = false;
    let mut cases = 0usize;
    let mut failures: Vec<String> = Vec::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        match serde_json::from_str::<Row>(line).expect("parse oracle row") {
            Row::Meta { baseline } => {
                assert_eq!(baseline, BASELINE, "oracle NDJSON is from another corpus");
                saw_meta = true;
            }
            Row::Case {
                id,
                source,
                existing,
                out,
            } => {
                cases += 1;
                let got = next_copy_title(&source, &existing);
                if got != out {
                    failures.push(format!("'{id}': v5 {got:?} != v4 {out:?}"));
                }
            }
        }
    }
    assert!(saw_meta, "no meta row — not this corpus");
    assert!(cases >= 40, "only {cases} cases — a truncated NDJSON?");
    assert!(
        failures.is_empty(),
        "{} of {cases} rows diverge:\n{}",
        failures.len(),
        failures.join("\n")
    );
    eprintln!("next_copy_title: {cases} rows equal");
}
