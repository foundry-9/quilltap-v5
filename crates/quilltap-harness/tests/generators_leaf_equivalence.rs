//! Tier-1 differential: the four character-generator leaf modules (P4.9K0) —
//! `lib/llm/llm-json.ts`, `lib/services/character-field-semantics.ts`,
//! `lib/characters/generated-properties.ts` and
//! `lib/characters/sanitize-pronouns.ts` — exact against v4's REAL exports.
//!
//! Comparands are byte-exact strings throughout; parsed JSON values are compared
//! as SERIALIZED bytes, so key order is part of the assertion (v4's object
//! insertion order vs serde_json's `preserve_order`). Integer-valued floats are
//! collapsed on both sides by `normalize_js_numbers` — a JS number does not
//! distinguish `3` from `3.0` and `JSON.stringify` emits `3` for both.
//!
//! Coverage is asserted by SHAPE in both directions: the oracle's final
//! `coverage` row names every corpus id, and this test checks that the ids it
//! actually drove are exactly that set (a corpus case the oracle skipped, or an
//! oracle row with no corpus entry, both fail).
//!
//! Regenerate the oracle (the corpus is committed — nothing to build):
//!   cd ~/source/quilltap-server
//!   npx tsx $V5W/harness/oracle/cases/generators-leaf.ts > /tmp/oracle-generators-leaf.ndjson
//! Run:
//!   QT_ORACLE_GENERATORS_LEAF=/tmp/oracle-generators-leaf.ndjson cargo test -p quilltap-harness --test generators_leaf_equivalence
//!
//! While the v4 checkout sits on `bugfix` (drift-ledger §1), the regen needs a
//! detached worktree pinned at the baseline — that is the sweep driver's job
//! (`recipe_sweep.py --run generators_leaf_equivalence --v4 <pin>`), never a
//! path baked into this header.

use quilltap_core::generators::field_semantics::ALL_EXPORTS;
use quilltap_core::generators::generated_properties::{
    describe_generated_properties, parse_generated_properties, GeneratedProperties,
};
use quilltap_core::generators::llm_json::{
    escape_control_chars_in_strings, parse_llm_json, parse_llm_json_object, repair_truncated_json,
    strip_code_fences,
};
use quilltap_core::generators::sanitize_pronouns::{sanitize_pronouns, Pronouns};
use quilltap_core::model::tool_wire::normalize_js_numbers;
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
struct Attempt {
    ok: bool,
    /// `JSON.parse` can only ever answer a JSON value, never `undefined`, so an
    /// absent key and an explicit `null` mean the same thing here — a top-level
    /// `null` answer (the `bug119-top-level-null` row) is exactly that.
    #[serde(default)]
    value: Value,
}

#[derive(Deserialize)]
struct WProps {
    pronouns: Option<Pronouns>,
    aliases: Vec<String>,
}

#[derive(Deserialize)]
#[serde(tag = "kind")]
enum Row {
    #[serde(rename = "llm_json", rename_all = "camelCase")]
    LlmJson {
        id: String,
        text: String,
        strip_code_fences: String,
        escape_control_chars_in_strings: String,
        repair_truncated_json: String,
        #[serde(rename = "parseLLMJson")]
        parse_llm_json: Attempt,
        #[serde(rename = "parseLLMJsonObject")]
        parse_llm_json_object: Attempt,
    },
    #[serde(rename = "pronouns")]
    Pronouns {
        id: String,
        raw: Value,
        out: Option<Pronouns>,
    },
    #[serde(rename = "properties")]
    Properties {
        id: String,
        text: String,
        ok: bool,
        props: Option<WProps>,
        describe: Option<String>,
    },
    #[serde(rename = "field_semantics")]
    FieldSemantics { id: String, value: String },
    #[serde(rename = "coverage", rename_all = "camelCase")]
    Coverage {
        llm_json: Vec<String>,
        pronouns: Vec<String>,
        properties: Vec<String>,
        field_semantics: Vec<String>,
        missing_pronouns_label: String,
    },
}

/// Serialize a parsed value the way the comparison needs it: integer-valued
/// floats collapsed (JS numbers), key order preserved.
fn shape(v: &Value) -> String {
    serde_json::to_string(&normalize_js_numbers(v.clone())).unwrap()
}

fn check_attempt(label: &str, got: Result<Value, impl std::fmt::Debug>, want: &Attempt) {
    match (got, want.ok) {
        (Ok(v), true) => {
            assert_eq!(shape(&v), shape(&want.value), "{label}: parsed value");
        }
        (Err(e), true) => panic!("{label}: v5 refused where v4 parsed ({e:?})"),
        (Ok(v), false) => panic!("{label}: v5 parsed {v} where v4 threw"),
        (Err(_), false) => {}
    }
}

#[test]
fn generators_leaf_matches_oracle() {
    let path = match std::env::var("QT_ORACLE_GENERATORS_LEAF") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_ORACLE_GENERATORS_LEAF to the oracle NDJSON (see test header)."
            );
            return;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));

    let mut seen_llm_json: Vec<String> = Vec::new();
    let mut seen_pronouns: Vec<String> = Vec::new();
    let mut seen_properties: Vec<String> = Vec::new();
    let mut seen_semantics: Vec<String> = Vec::new();
    let mut coverage: Option<Row> = None;

    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Row = serde_json::from_str(line)
            .unwrap_or_else(|e| panic!("bad oracle row: {e}\n{}", &line[..line.len().min(200)]));
        match row {
            Row::LlmJson {
                id,
                text,
                strip_code_fences: want_strip,
                escape_control_chars_in_strings: want_escape,
                repair_truncated_json: want_repair,
                parse_llm_json: want_parse,
                parse_llm_json_object: want_parse_object,
            } => {
                assert_eq!(
                    strip_code_fences(&text),
                    want_strip,
                    "stripCodeFences '{id}'"
                );
                assert_eq!(
                    escape_control_chars_in_strings(&text),
                    want_escape,
                    "escapeControlCharsInStrings '{id}'"
                );
                assert_eq!(
                    repair_truncated_json(&text),
                    want_repair,
                    "repairTruncatedJson '{id}'"
                );
                check_attempt(
                    &format!("parseLLMJson '{id}'"),
                    parse_llm_json(&text),
                    &want_parse,
                );
                check_attempt(
                    &format!("parseLLMJsonObject '{id}'"),
                    parse_llm_json_object(&text),
                    &want_parse_object,
                );
                seen_llm_json.push(id);
            }
            Row::Pronouns { id, raw, out } => {
                assert_eq!(sanitize_pronouns(&raw), out, "sanitizePronouns '{id}'");
                seen_pronouns.push(id);
            }
            Row::Properties {
                id,
                text,
                ok,
                props,
                describe,
            } => {
                match (parse_generated_properties(&text), ok) {
                    (Ok(got), true) => {
                        let want = props.expect("ok row carries props");
                        let want = GeneratedProperties {
                            pronouns: want.pronouns,
                            aliases: want.aliases,
                        };
                        assert_eq!(got, want, "parseGeneratedProperties '{id}'");
                        let label = describe.expect("ok row carries describe");
                        // The label is the corpus's `missingPronounsLabel`; the
                        // coverage row carries it and the assert below pins it.
                        assert_eq!(
                            describe_generated_properties(&got, MISSING_PRONOUNS_LABEL),
                            label,
                            "describeGeneratedProperties '{id}'"
                        );
                    }
                    (Err(e), true) => panic!("properties '{id}': v5 refused where v4 parsed ({e})"),
                    (Ok(_), false) => panic!("properties '{id}': v5 parsed where v4 threw"),
                    (Err(_), false) => {
                        assert!(props.is_none() && describe.is_none(), "properties '{id}'");
                    }
                }
                seen_properties.push(id);
            }
            Row::FieldSemantics { id, value } => {
                let ours = ALL_EXPORTS
                    .iter()
                    .find(|(name, _)| *name == id)
                    .unwrap_or_else(|| panic!("v5 has no field-semantics export '{id}'"));
                assert_eq!(ours.1, value, "field semantics '{id}'");
                seen_semantics.push(id);
            }
            r @ Row::Coverage { .. } => coverage = Some(r),
        }
    }

    let Some(Row::Coverage {
        llm_json,
        pronouns,
        properties,
        field_semantics,
        missing_pronouns_label,
    }) = coverage
    else {
        panic!("the oracle emitted no coverage row");
    };

    // Both directions: the ids driven are exactly the corpus's.
    assert_eq!(seen_llm_json, llm_json, "llm_json coverage");
    assert_eq!(seen_pronouns, pronouns, "pronouns coverage");
    assert_eq!(seen_properties, properties, "properties coverage");
    assert_eq!(seen_semantics, field_semantics, "field-semantics coverage");
    // And v5 declares no export the oracle did not drive.
    let ours: Vec<String> = ALL_EXPORTS.iter().map(|(n, _)| n.to_string()).collect();
    assert_eq!(ours, field_semantics, "v5 field-semantics export set");
    assert_eq!(
        missing_pronouns_label, MISSING_PRONOUNS_LABEL,
        "the corpus's missingPronounsLabel moved; update the test constant"
    );

    // The work order's corpus floors.
    assert!(
        llm_json.len() >= 25,
        "llm_json corpus shrank below the floor: {}",
        llm_json.len()
    );
    assert!(pronouns.len() >= 8, "pronouns corpus: {}", pronouns.len());
    assert!(
        properties.len() >= 8,
        "properties corpus: {}",
        properties.len()
    );

    eprintln!(
        "OK: generators-leaf matched oracle ({} llm_json / {} pronouns / {} properties / {} field-semantics exports).",
        llm_json.len(),
        pronouns.len(),
        properties.len(),
        field_semantics.len()
    );
}

/// The corpus's `missingPronounsLabel`, pinned here and cross-checked against
/// the coverage row so the two cannot drift apart silently.
const MISSING_PRONOUNS_LABEL: &str = "no pronouns found";
