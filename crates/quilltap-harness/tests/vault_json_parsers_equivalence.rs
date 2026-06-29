//! Tier-1 differential test: the vault JSON projection parsers
//! (`parseVaultProperties`, `parseVaultPhysicalPrompts`).
//!
//! Exact-equality against the v4 oracle, reproducing Zod `safeParse`'s
//! fall-back-to-null-on-any-violation semantics + unknown-key stripping. Covers:
//! a full valid `properties.json`, the all-nulls form, unknown-key stripping
//! (top-level and inside `pronouns`), invalid JSON, non-object root, missing
//! required key, `talkativeness` range (low/high/boundary), `aliases`
//! non-array/non-string, `pronouns` missing-field/too-long/empty, wrong-type
//! `title`; and for `physical-prompts.json`: full, headAndShoulders-absent,
//! tier-nulls, invalid JSON, missing tier, wrong-type tier, extra-key stripping.
//!
//! Numbers are canonicalized on both sides (integer-valued floats → integers) so
//! `talkativeness: 1.0` (which v4 emits as `1`) compares equal to the Rust `1.0`.
//!
//! Generate the oracle output:
//!   cd ~/source/quilltap-server
//!   npx tsx ~/source/quilltap-v5/harness/oracle/cases/vault-json-parsers.ts \
//!     > /tmp/oracle-vault-json-parsers.ndjson
//! Run:
//!   QT_ORACLE_VAULT_JSON_PARSERS=/tmp/oracle-vault-json-parsers.ndjson \
//!     cargo test -p quilltap-harness --test vault_json_parsers_equivalence

use quilltap_core::vault_overlay::{parse_vault_physical_prompts, parse_vault_properties};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
#[serde(tag = "kind")]
enum Row {
    #[serde(rename = "properties")]
    Properties { id: String, raw: String, out: Value },
    #[serde(rename = "physical")]
    Physical { id: String, raw: String, out: Value },
}

/// Collapse integer-valued floats to integers, recursively (matches how v4's
/// `JSON.stringify` renders `1.0` as `1`), so the two sides compare structurally.
fn canon_numbers(v: &mut Value) {
    match v {
        Value::Number(n) => {
            if let Some(f) = n.as_f64() {
                if f.is_finite() && f.fract() == 0.0 && f >= i64::MIN as f64 && f <= i64::MAX as f64
                {
                    *v = Value::from(f as i64);
                }
            }
        }
        Value::Array(a) => a.iter_mut().for_each(canon_numbers),
        Value::Object(o) => o.values_mut().for_each(canon_numbers),
        _ => {}
    }
}

fn canon(mut v: Value) -> Value {
    canon_numbers(&mut v);
    v
}

#[test]
fn vault_json_parsers_match_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_VAULT_JSON_PARSERS") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_VAULT_JSON_PARSERS to the oracle NDJSON (see header).");
            return;
        }
    };
    let text =
        std::fs::read_to_string(&oracle_path).unwrap_or_else(|e| panic!("cannot read oracle: {e}"));

    let mut n = 0;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Row = serde_json::from_str(line).expect("parse oracle row");
        match row {
            Row::Properties { id, raw, out } => {
                let got = parse_vault_properties(&raw)
                    .map(|p| serde_json::to_value(p).unwrap())
                    .unwrap_or(Value::Null);
                assert_eq!(canon(got), canon(out), "[{id}] parse_vault_properties");
            }
            Row::Physical { id, raw, out } => {
                let got = parse_vault_physical_prompts(&raw)
                    .map(|p| serde_json::to_value(p).unwrap())
                    .unwrap_or(Value::Null);
                assert_eq!(
                    canon(got),
                    canon(out),
                    "[{id}] parse_vault_physical_prompts"
                );
            }
        }
        n += 1;
    }
    assert!(n >= 24, "expected the full corpus, saw {n} rows");
    eprintln!("OK: vault JSON parsers matched oracle on {n} cases.");
}
