//! Tier-1 differential (P4.d19, v4 `faab6881` + `6864bf0e`): the Pascal tool
//! VOCABULARY — what a definition quotes, byte-exact against v4's real
//! `collectToolVocabulary`.
//!
//! The whole serialized object is compared rather than field by field, because
//! it IS payload: `/api/v1/chats/{id}/custom-tools` puts `references` on every
//! listing, so key order and "all seven keys always present" are the contract
//! (§1 of the P4.d19 round's shared contract, which the SPA consumes).
//!
//! Generate the oracle output (v4 @ 231be14c, Node 24
//! `~/.nvm/versions/node/v24.13.1/bin`; the pinned detached worktree):
//!   cd ~/source/quilltap-server
//!   TZ=UTC npx tsx \
//!     <V5W>/harness/oracle/cases/pascal-tool-vocabulary.ts \
//!     > /tmp/oracle-pascal-vocabulary.ndjson
//! Run:
//!   QT_ORACLE_PASCAL_VOCABULARY=/tmp/oracle-pascal-vocabulary.ndjson \
//!     cargo test -p quilltap-harness --test pascal_tool_vocabulary_equivalence

use quilltap_core::pascal::custom_tool_types::safe_parse;
use quilltap_core::pascal::tool_vocabulary::{collect_tool_vocabulary, is_empty_vocabulary};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
struct Row {
    id: String,
    /// The definition's BYTES — both sides parse them with their own JSON
    /// parser, exactly as `read_tool_file` does.
    #[serde(rename = "inputJson")]
    input_json: String,
    /// The SERIALIZED vocabulary: seven keys, in v4's declaration order.
    vocabulary: String,
    empty: bool,
}

#[test]
fn pascal_tool_vocabulary_matches_oracle() {
    let path = match std::env::var("QT_ORACLE_PASCAL_VOCABULARY") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_PASCAL_VOCABULARY to the oracle NDJSON (see header).");
            return;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));

    let mut count = 0usize;
    // Coverage, not a magic row count: a corpus that quietly lost its
    // interesting rows would otherwise still pass. Every field must be seen
    // BOTH set and unset, and at least one row must quote nothing at all.
    let mut seen_true: Vec<&str> = Vec::new();
    let mut seen_false: Vec<&str> = Vec::new();
    let mut seen_nonempty_list: Vec<&str> = Vec::new();
    let mut seen_empty_vocabulary = false;
    let mut seen_multi_entry_list = false;

    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Row = serde_json::from_str(line).unwrap();
        let raw: Value = serde_json::from_str(&row.input_json)
            .unwrap_or_else(|e| panic!("case '{}': oracle input is not JSON: {e}", row.id));
        let tool = safe_parse(&raw)
            .unwrap_or_else(|e| panic!("case '{}': definition does not load: {e:?}", row.id));

        let vocabulary = collect_tool_vocabulary(&tool);
        let got = serde_json::to_string(&vocabulary).expect("a vocabulary serializes");
        assert_eq!(
            got, row.vocabulary,
            "case '{}': vocabulary differs (key order and presence included)",
            row.id
        );
        assert_eq!(
            is_empty_vocabulary(&vocabulary),
            row.empty,
            "isEmptyVocabulary '{}'",
            row.id
        );

        for (name, flag) in [
            ("value", vocabulary.value),
            ("roll", vocabulary.roll),
            ("dice", vocabulary.dice),
            ("llm", vocabulary.llm),
        ] {
            let bucket = if flag {
                &mut seen_true
            } else {
                &mut seen_false
            };
            if !bucket.contains(&name) {
                bucket.push(name);
            }
        }
        for (name, list) in [
            ("params", &vocabulary.params),
            ("metadata", &vocabulary.metadata),
            ("state", &vocabulary.state),
        ] {
            if !list.is_empty() && !seen_nonempty_list.contains(&name) {
                seen_nonempty_list.push(name);
            }
            if list.len() > 1 {
                seen_multi_entry_list = true;
            }
        }
        if row.empty {
            seen_empty_vocabulary = true;
        }
        count += 1;
    }

    assert!(count > 0, "oracle file looks empty");
    seen_true.sort();
    seen_false.sort();
    seen_nonempty_list.sort();
    assert_eq!(
        seen_true,
        vec!["dice", "llm", "roll", "value"],
        "every boolean must be seen SET somewhere in the corpus"
    );
    assert_eq!(
        seen_false,
        vec!["dice", "llm", "roll", "value"],
        "every boolean must be seen UNSET somewhere in the corpus"
    );
    assert_eq!(
        seen_nonempty_list,
        vec!["metadata", "params", "state"],
        "every list must be seen non-empty somewhere in the corpus"
    );
    assert!(
        seen_empty_vocabulary,
        "no row exercised the quotes-nothing case (isEmptyVocabulary)"
    );
    assert!(
        seen_multi_entry_list,
        "no row exercised the ORDERING of a multi-entry list"
    );
    eprintln!("OK: pascal tool vocabulary matched oracle ({count} definitions).");
}
