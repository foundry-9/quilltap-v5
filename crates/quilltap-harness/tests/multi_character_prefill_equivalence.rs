//! P4.D79 unit 2 — the multi-character `[Name]` prefill resolver (v4
//! `23af7146`, `lib/llm/multi-character-prefill.ts`). Tier-1 exact: both
//! functions over providers × {true, false, null, absent, non-boolean}.
//!
//! What the corpus is really pinning, beyond the obvious Anthropic split:
//!
//!   - `!provider` is JS falsiness, so the **empty string** takes the same
//!     "true" branch as null/undefined (v5 models that with
//!     `.filter(|s| !s.is_empty())`, not a bare `Option` test);
//!   - the hostile test is `provider.toUpperCase()` — casing folds, whitespace
//!     does NOT (`' ANTHROPIC'` is not hostile), and the dotless-i row proves
//!     Rust's `to_uppercase` agrees with JS on a non-ASCII fold that reaches
//!     `'ANTHROPIC'`;
//!   - the stored-choice test is `typeof … === 'boolean'`, so `1`/`0`/`'true'`
//!     fall THROUGH to the provider default instead of coercing — the reason
//!     the v5 helper uses `Value::as_bool()` and not a truthiness test.
//!
//! Generate the oracle output (from the pinned v4 worktree):
//!   cd /tmp/qt-v4-pin-p4d79-aa464abf
//!   ~/.nvm/versions/node/v24.13.1/bin/npx tsx \
//!     ~/source/quilltap-v5/harness/oracle/cases/multi-character-prefill.ts \
//!     > /tmp/oracle-multi-character-prefill.ndjson
//! Run:
//!   QT_ORACLE_MULTI_CHAR_PREFILL=/tmp/oracle-multi-character-prefill.ndjson \
//!     cargo test -p quilltap-harness --test multi_character_prefill_equivalence

use quilltap_core::services::multi_character_prefill::{
    default_multi_character_prefill, profile_uses_name_prefill_value,
};
use serde::Deserialize;
use serde_json::{Map, Value};

/// The oracle's stand-in for a JS `undefined` provider (NDJSON has no such
/// value) and for a key the profile object never carried.
const UNDEFINED: &str = "<undefined>";
const ABSENT: &str = "<absent>";

#[derive(Deserialize)]
#[serde(tag = "kind")]
enum Row {
    #[serde(rename = "default")]
    Default {
        id: String,
        provider: Value,
        out: bool,
    },
    #[serde(rename = "resolve")]
    Resolve {
        id: String,
        provider: Value,
        stored: Value,
        out: bool,
    },
}

/// The oracle's `provider` cell → the argument shape: a JSON `null` and the
/// `<undefined>` sentinel both mean "no provider".
fn provider_arg(cell: &Value) -> Option<&str> {
    match cell {
        Value::String(s) if s == UNDEFINED => None,
        Value::String(s) => Some(s.as_str()),
        _ => None,
    }
}

#[test]
fn multi_character_prefill_matches_oracle() {
    let path = match std::env::var("QT_ORACLE_MULTI_CHAR_PREFILL") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_ORACLE_MULTI_CHAR_PREFILL to the oracle NDJSON (see test header)."
            );
            return;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));

    let mut defaults = 0usize;
    let mut resolves = 0usize;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        match serde_json::from_str::<Row>(line).unwrap() {
            Row::Default { id, provider, out } => {
                assert_eq!(
                    default_multi_character_prefill(provider_arg(&provider)),
                    out,
                    "default '{id}'"
                );
                defaults += 1;
            }
            Row::Resolve {
                id,
                provider,
                stored,
                out,
            } => {
                // Rebuild the exact profile object the oracle handed v4: an
                // absent key stays absent, so the `typeof` guard is exercised
                // on the same shapes on both sides.
                let mut profile = Map::new();
                if provider.as_str() != Some(UNDEFINED) {
                    profile.insert("provider".into(), provider.clone());
                }
                if stored.as_str() != Some(ABSENT) {
                    profile.insert("multiCharacterPrefill".into(), stored.clone());
                }
                assert_eq!(
                    profile_uses_name_prefill_value(&Value::Object(profile)),
                    out,
                    "resolve '{id}'"
                );
                resolves += 1;
            }
        }
    }

    // Shape assertions, not hand counts (`harness-corpus-shape-constants-rot`):
    // every provider row must appear once per `default`, and the resolve grid
    // must be a clean product of the two axes.
    assert!(
        defaults >= 16,
        "expected the full provider table, got {defaults}"
    );
    assert_eq!(
        resolves,
        defaults * 8,
        "the resolve grid must be providers × the 8 stored states"
    );
    eprintln!("multi-character prefill: {defaults} default + {resolves} resolve cases matched");
}
