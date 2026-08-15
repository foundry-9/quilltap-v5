//! P4.D79 unit 7 — `profileParams()` as widened by v4 `d9c5a1c7` (the Ollama
//! `num_ctx` injection). Tier-1 exact: 15 parameter-bag shapes × 6 providers ×
//! 10 `maxContext` values = 900 cases, compared BOTH structurally and as the
//! literal `JSON.stringify` text, so **key order** is a comparand.
//!
//! Key order is not decoration here. Two of v4's edges are order facts:
//!
//!   - spreading an ARRAY base yields index keys in order
//!     (`{"0":"a","1":"b","num_ctx":…}`);
//!   - overwriting an existing `num_ctx: null` keeps the key at its ORIGINAL
//!     position — `{temperature, num_ctx, top_p}` stays in that order, it does
//!     not move to the end. serde_json's `preserve_order` `IndexMap` reproduces
//!     both, which is exactly what the `outJson` comparison proves.
//!
//! The other edges the corpus pins: `null` parameters drop out despite
//! `typeof null === 'object'`; the provider test is case-SENSITIVE (`'ollama'`
//! gets nothing); a NON-object bag with an injecting provider still yields
//! `{num_ctx}` because v4 spreads `base ?? {}`; `num_ctx: 0` and `num_ctx: false`
//! suppress the injection (loose `== null`) while `num_ctx: null` does not;
//! `Infinity` is a number that passes `> 0` and then stringifies to `null`;
//! `NaN` fails `> 0`; a STRING `maxContext` fails `typeof === 'number'`.
//!
//! Generate the oracle output (from the v4 checkout; pin a detached worktree
//! via recipe_sweep.py --v4 when v4 HEAD has moved past the baseline):
//!   cd ~/source/quilltap-server
//!   ~/.nvm/versions/node/v24.13.1/bin/npx tsx \
//!     ~/source/quilltap-v5/harness/oracle/cases/profile-params.ts \
//!     > /tmp/oracle-profile-params.ndjson
//! Run:
//!   QT_ORACLE_PROFILE_PARAMS=/tmp/oracle-profile-params.ndjson \
//!     cargo test -p quilltap-harness --test profile_params_equivalence

use quilltap_core::cheap_llm::profile_params_parts;
use serde::Deserialize;
use serde_json::{json, Value};

const UNDEFINED: &str = "<undefined>";

#[derive(Deserialize)]
struct Row {
    id: String,
    provider: String,
    bag: String,
    #[serde(rename = "maxContext")]
    max_context: String,
    out: Value,
    #[serde(rename = "outJson")]
    out_json: String,
}

/// The bag shapes, keyed exactly as `profile-params.ts` names them. Kept in
/// Rust rather than serialized through the NDJSON so the ARRAY and non-object
/// shapes reach the function as themselves.
fn bag(name: &str) -> Option<Value> {
    match name {
        "absent" => None,
        "null" => Some(Value::Null),
        "empty-object" => Some(json!({})),
        "single-key" => Some(json!({ "temperature": 0.4 })),
        "multi-key" => Some(json!({
            "temperature": 0.4, "enable_thinking": true, "max_tokens": 900
        })),
        "num_ctx-set" => Some(json!({ "temperature": 0.4, "num_ctx": 8192 })),
        "num_ctx-null" => Some(json!({
            "temperature": 0.4, "num_ctx": Value::Null, "top_p": 0.9
        })),
        "num_ctx-zero" => Some(json!({ "num_ctx": 0 })),
        "num_ctx-false" => Some(json!({ "num_ctx": false })),
        "num_ctx-string" => Some(json!({ "num_ctx": "4096" })),
        "array-empty" => Some(json!([])),
        "array-two" => Some(json!(["a", "b"])),
        "number" => Some(json!(7)),
        "string" => Some(json!("nope")),
        "boolean" => Some(json!(true)),
        other => panic!("unknown bag shape '{other}' — the oracle case and this table disagree"),
    }
}

/// The `maxContext` values, likewise by name. A non-number (`null`, a string,
/// absent) reaches the Rust helper as `None`, which is what v5's `as_f64` read
/// off the REAL column produces — and what v4's `typeof … === 'number'` guard
/// makes of them.
fn max_context(name: &str) -> Option<f64> {
    match name {
        "absent" | "null" | "string" => None,
        "positive" => Some(32768.0),
        "one" => Some(1.0),
        "zero" => Some(0.0),
        "negative" => Some(-5.0),
        "float" => Some(4096.5),
        "infinity" => Some(f64::INFINITY),
        "nan" => Some(f64::NAN),
        other => panic!("unknown maxContext '{other}' — the oracle case and this table disagree"),
    }
}

#[test]
fn profile_params_matches_oracle() {
    let path = match std::env::var("QT_ORACLE_PROFILE_PARAMS") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_PROFILE_PARAMS to the oracle NDJSON (see test header).");
            return;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));

    let mut n = 0usize;
    let mut injected = 0usize;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Row = serde_json::from_str(line).unwrap();
        let params = bag(&row.bag);
        let got = profile_params_parts(
            &row.provider,
            params.as_ref(),
            max_context(&row.max_context),
        );

        // Structural equality first (the oracle writes `<undefined>` for a JS
        // `undefined` return).
        let want_absent = row.out.as_str() == Some(UNDEFINED);
        match (&got, want_absent) {
            (None, true) => {}
            (Some(g), false) => assert_eq!(g, &row.out, "value for '{}'", row.id),
            (None, false) => panic!(
                "'{}': v5 returned undefined, v4 returned {}",
                row.id, row.out
            ),
            (Some(g), true) => panic!("'{}': v5 returned {g}, v4 returned undefined", row.id),
        }

        // Then the literal serialization — this is what makes key ORDER a
        // comparand, and what catches an integral `num_ctx` rendering as
        // `32768.0` instead of JS's `32768`.
        let got_json = match &got {
            None => UNDEFINED.to_string(),
            Some(g) => serde_json::to_string(g).expect("serialize"),
        };
        assert_eq!(got_json, row.out_json, "JSON text for '{}'", row.id);

        if got
            .as_ref()
            .and_then(|g| g.get("num_ctx"))
            .is_some_and(|_| params.as_ref().and_then(|p| p.get("num_ctx")).is_none())
        {
            injected += 1;
        }
        n += 1;
    }

    // Shape assertions rather than a hand count
    // (`harness-corpus-shape-constants-rot`): the grid is 15 bags × 6 providers
    // × 10 maxContexts, and the injection must actually have FIRED — a corpus
    // where no row injects would pass while proving nothing.
    assert_eq!(n, 15 * 6 * 10, "the corpus must be the full grid");
    assert!(
        injected > 0,
        "no case exercised the num_ctx injection — the corpus is vacuous"
    );
    eprintln!("profileParams: {n} cases matched ({injected} of them injecting num_ctx)");
}
