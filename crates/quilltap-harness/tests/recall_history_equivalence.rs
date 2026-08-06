//! Tier-1 differential test #4: the Commonplace Book recall anti-repetition
//! ring buffer (parseRecallHistory / recentlyWhisperedIdSet / appendRecallTurn).
//!
//! Unlike the earlier cases, the inputs are arbitrary / deliberately-malformed
//! JSON (the module's whole job is coercion), so each oracle row carries BOTH
//! the input `raw` and the expected output. The test feeds the SAME `raw`
//! (parsed to serde_json::Value) through the Rust port and compares — guaranteeing
//! both sides see identical bytes, with no second transcription of subtle inputs.
//!
//! Equivalence shapes: parse/append are exact nested-array structure; the set
//! union is compared as set membership (the TS returns a Set; iteration order is
//! not part of its contract).
//!
//! Generate the oracle output:
//!   cd ~/source/quilltap-server
//!   npx tsx ~/source/quilltap-v5/harness/oracle/cases/recall-history.ts \
//!     > /tmp/oracle-recall-history.ndjson
//! Run:
//!   QT_ORACLE_RECALL_HISTORY=/tmp/oracle-recall-history.ndjson \
//!     cargo test -p quilltap-harness --test recall_history_equivalence

use std::collections::HashSet;

use quilltap_core::recall_history::{
    append_recall_turn, append_retro_signature, parse_recall_history, parse_retro_signatures,
    recently_whispered_id_set,
};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
#[serde(tag = "kind")]
enum OracleRow {
    #[serde(rename = "parse")]
    Parse {
        id: String,
        raw: Value,
        out: Vec<Vec<String>>,
    },
    #[serde(rename = "set")]
    Set {
        id: String,
        raw: Value,
        out: Vec<String>,
    },
    #[serde(rename = "append")]
    Append {
        id: String,
        raw: Value,
        #[serde(rename = "newIds")]
        new_ids: Vec<String>,
        out: AppendOut,
    },
    #[serde(rename = "parseSig")]
    ParseSig {
        id: String,
        raw: Value,
        out: Vec<String>,
    },
    #[serde(rename = "appendSig")]
    AppendSig {
        id: String,
        raw: Value,
        #[serde(rename = "newIds")]
        new_ids: Vec<String>,
        signature: String,
        out: AppendOut,
    },
}

#[derive(Deserialize)]
struct AppendOut {
    turns: Vec<Vec<String>>,
    /// v4 omits the key entirely when the list is empty (it only ever writes a
    /// non-empty one), so an absent key means "no signatures".
    #[serde(rename = "retroSignatures", default)]
    retro_signatures: Vec<String>,
}

#[test]
fn recall_history_matches_oracle() {
    let path = match std::env::var("QT_ORACLE_RECALL_HISTORY") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_RECALL_HISTORY to the oracle NDJSON (see test header).");
            return;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));

    let mut n_parse = 0;
    let mut n_set = 0;
    let mut n_append = 0;
    let mut n_parse_sig = 0;
    let mut n_append_sig = 0;

    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        match serde_json::from_str::<OracleRow>(line).unwrap() {
            OracleRow::Parse { id, raw, out } => {
                let got = parse_recall_history(&raw);
                assert_eq!(got, out, "parse '{id}'");
                n_parse += 1;
            }
            OracleRow::Set { id, raw, out } => {
                let got = recently_whispered_id_set(&raw);
                let want: HashSet<String> = out.into_iter().collect();
                assert_eq!(got, want, "set '{id}' (membership)");
                n_set += 1;
            }
            OracleRow::Append {
                id,
                raw,
                new_ids,
                out,
            } => {
                let got = append_recall_turn(&raw, &new_ids);
                assert_eq!(got.turns, out.turns, "append '{id}'");
                assert_eq!(
                    got.retro_signatures, out.retro_signatures,
                    "append '{id}' (retroSignatures carried through)"
                );
                n_append += 1;
            }
            OracleRow::ParseSig { id, raw, out } => {
                let got = parse_retro_signatures(&raw);
                assert_eq!(got, out, "parseSig '{id}'");
                n_parse_sig += 1;
            }
            OracleRow::AppendSig {
                id,
                raw,
                new_ids,
                signature,
                out,
            } => {
                // Mirrors buildContext's persist: append the turn first, then the
                // signature on the already-appended history.
                let base = append_recall_turn(&raw, &new_ids);
                let got = append_retro_signature(&base, &signature);
                assert_eq!(got.turns, out.turns, "appendSig '{id}' (turns)");
                assert_eq!(
                    got.retro_signatures, out.retro_signatures,
                    "appendSig '{id}' (retroSignatures)"
                );
                n_append_sig += 1;
            }
        }
    }

    assert!(
        n_parse > 0 && n_set > 0 && n_append > 0 && n_parse_sig > 0 && n_append_sig > 0,
        "oracle file looks empty/partial"
    );
    eprintln!(
        "OK: recall-history matched oracle ({n_parse} parse, {n_set} set, {n_append} append, \
         {n_parse_sig} parseSig, {n_append_sig} appendSig)."
    );
}
