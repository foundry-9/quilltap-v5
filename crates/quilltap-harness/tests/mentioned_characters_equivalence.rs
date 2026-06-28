//! Tier-1 differential test #23 (Wave 5 / B14): findMentionedCharacterIds —
//! exact set equality against the v4 oracle. Exercises ASCII-`\b` boundaries,
//! longest-token-first alternation, shared aliases, case-insensitivity, the
//! non-ASCII-trailing-boundary quirk, and cross-candidate dedup.
//!
//! Generate the oracle output:
//!   cd ~/source/quilltap-server
//!   npx tsx ~/source/quilltap-v5/harness/oracle/cases/mentioned-characters.ts \
//!     > /tmp/oracle-mentioned-characters.ndjson
//! Run:
//!   QT_ORACLE_MENTIONED_CHARACTERS=/tmp/oracle-mentioned-characters.ndjson \
//!     cargo test -p quilltap-harness

use quilltap_core::mentioned_characters::{find_mentioned_character_ids, MentionCandidate};
use serde::Deserialize;
use std::collections::BTreeSet;

#[derive(Deserialize)]
struct WCand {
    id: String,
    name: String,
    #[serde(default)]
    aliases: Vec<String>,
}

#[derive(Deserialize)]
struct Row {
    id: String,
    corpus: String,
    candidates: Vec<WCand>,
    out: Vec<String>,
}

#[test]
fn mentioned_characters_match_oracle() {
    let path = match std::env::var("QT_ORACLE_MENTIONED_CHARACTERS") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_ORACLE_MENTIONED_CHARACTERS to the oracle NDJSON (see test header)."
            );
            return;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));

    let mut count = 0usize;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Row = serde_json::from_str(line).unwrap();
        let candidates: Vec<MentionCandidate> = row
            .candidates
            .into_iter()
            .map(|c| MentionCandidate {
                id: c.id,
                name: c.name,
                aliases: c.aliases,
            })
            .collect();
        let got = find_mentioned_character_ids(&row.corpus, &candidates);
        let expected: BTreeSet<String> = row.out.into_iter().collect();
        assert_eq!(got, expected, "mentioned '{}'", row.id);
        count += 1;
    }

    assert!(count > 0, "oracle file looks empty: {count}");
    eprintln!("OK: mentioned-characters matched oracle ({count} rows).");
}
