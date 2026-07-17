//! P4.d6 differential — the help-document slug (v4 `lib/help/help-doc-slug.ts`
//! `helpDocSlug` vs `quilltap_core::help_doc_slug::help_doc_slug`).
//!
//! Tier 1: exact string equality over the oracle's fixed corpus. The corpus is
//! the oracle case's (`harness/oracle/cases/help-doc-slug.ts`); this side only
//! replays `input` and compares `output` byte-for-byte.
//!
//! Generate (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5=<this tree>
//!   cd ~/source/quilltap-server
//!   $N/node --import tsx $V5/harness/oracle/cases/help-doc-slug.ts \
//!     > /tmp/oracle-help-doc-slug.ndjson
//! Run:
//!   QT_ORACLE_HELP_DOC_SLUG=/tmp/oracle-help-doc-slug.ndjson \
//!     cargo test -p quilltap-harness --test help_doc_slug_equivalence

use serde::Deserialize;

use quilltap_core::help_doc_slug::help_doc_slug;

#[derive(Deserialize)]
struct SlugCase {
    input: String,
    output: String,
}

#[test]
fn help_doc_slug_matches_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_HELP_DOC_SLUG") else {
        eprintln!(
            "skipping help_doc_slug_matches_oracle: set QT_ORACLE_HELP_DOC_SLUG to run the \
             differential"
        );
        return;
    };

    let text = std::fs::read_to_string(&oracle_path).expect("oracle ndjson");
    let cases: Vec<SlugCase> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("oracle line"))
        .collect();

    assert!(
        !cases.is_empty(),
        "oracle corpus is empty — regenerate {oracle_path}"
    );

    let mut diverged = Vec::new();
    for case in &cases {
        let got = help_doc_slug(&case.input);
        if got != case.output {
            diverged.push(format!(
                "  input {:?}: rust {:?} vs oracle {:?}",
                case.input, got, case.output
            ));
        }
    }

    assert!(
        diverged.is_empty(),
        "help_doc_slug diverged from v4 on {}/{} cases:\n{}",
        diverged.len(),
        cases.len(),
        diverged.join("\n")
    );

    eprintln!("help_doc_slug: {} cases matched", cases.len());
}
