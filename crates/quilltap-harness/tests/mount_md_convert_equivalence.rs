//! P4.6y markdown-converter differential (tier-1 exact):
//! `services::mount_index::converters::markdown_to_text` vs v4's REAL
//! `convertMarkdownToText` over the oracle-carried corpus (each row holds the
//! input AND v4's output, so the corpus is single-sourced in the oracle case).
//!
//! Generate (Node 24, from the v4 checkout):
//!   V5W=<the v5 checkout>
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   $N/node --import tsx $V5W/harness/oracle/cases/mount-md-convert.ts \
//!     > /tmp/oracle-mount-md-convert.ndjson
//! Run:
//!   QT_ORACLE_MOUNT_MD_CONVERT=/tmp/oracle-mount-md-convert.ndjson \
//!     cargo test -p quilltap-harness --test mount_md_convert_equivalence

use quilltap_core::services::mount_index::converters::markdown_to_text;

#[test]
fn markdown_converter_matches_oracle() {
    let path = match std::env::var("QT_ORACLE_MOUNT_MD_CONVERT") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_MOUNT_MD_CONVERT to the oracle NDJSON.");
            return;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));
    let mut checked = 0usize;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: serde_json::Value = serde_json::from_str(line).unwrap();
        let id = row["id"].as_str().unwrap();
        let input = row["input"].as_str().unwrap();
        let want = row["out"].as_str().unwrap();
        let got = markdown_to_text(input);
        assert_eq!(
            got, want,
            "markdown_to_text mismatch for case {id}\ninput: {input:?}"
        );
        checked += 1;
    }
    assert!(checked >= 25, "expected the full corpus, got {checked}");
    eprintln!("OK: mount-md-convert matched oracle ({checked} cases).");
}
