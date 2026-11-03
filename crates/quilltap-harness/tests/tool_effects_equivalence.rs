//! Tier-1 differential (P4.D343): declared tool effects — `crate::tools::effects`
//! and `crate::pascal::tool_effect` vs v4's REAL `lib/tools/effects.ts`,
//! `effect-types.ts`, the four computed specs and `lib/pascal/tool-effect.ts`
//! at the pin `236c04e35` (v4 `b15fe9404`).
//!
//! Covers the 74-row `BUILT_IN_TOOL_EFFECTS` table (every key, every value,
//! `{computed: maxEffect}` for the four computed specs, the count),
//! `effectOfToolCall` over a corpus (each computed tool on valid / invalid /
//! missing input; `run_custom` with and without a `customToolEffects` hit; an
//! unknown name with / without a `pluginToolEffects` entry), `maxEffectOfTool`,
//! `highestToolEffect` (incl. empty), `isWithinCeiling` × 16, `maxToolEffect`
//! × 16, `lowerToolEffect` × 16 + `undefined`, `describeCeilingRefusal` for
//! every (effect, ceiling) pair, `usesAllowedUnder` for the four computed × four
//! ceilings (the exact note or null), `customToolEffect` over parsed
//! definitions, and `8becfc6ce`'s deletion of `isComputedTool` (no v5 twin).
//!
//! Generate the oracle output (Node 24, from the pinned v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   $N/npx tsx $V5W/harness/oracle/cases/tool-effects.ts \
//!     > /tmp/oracle-tool-effects.ndjson
//! Run:
//!   QT_ORACLE_TOOL_EFFECTS=/tmp/oracle-tool-effects.ndjson \
//!     cargo test -p quilltap-harness --test tool_effects_equivalence -- --nocapture

use quilltap_core::pascal::custom_tool_types::safe_parse;
use quilltap_core::pascal::tool_effect::custom_tool_effect;
use quilltap_core::tools::effects::{
    describe_ceiling_refusal, effect_of_tool_call, highest_tool_effect, is_within_ceiling,
    lower_tool_effect, max_effect_of_tool, max_tool_effect, tool_effect_spec_of,
    uses_allowed_under, EffectLookups, ToolEffect, ToolEffectSpec, BUILT_IN_TOOL_EFFECT_NAMES,
};
use serde_json::{json, Map, Value};

fn effect(v: &Value) -> ToolEffect {
    ToolEffect::from_str(v.as_str().expect("effect string")).expect("known effect")
}

fn lookups(ctx: &Value) -> EffectLookups {
    let mut l = EffectLookups::default();
    let pairs = |key: &str| -> Vec<(String, ToolEffect)> {
        ctx.get(key)
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .map(|p| (p[0].as_str().unwrap().to_string(), effect(&p[1])))
                    .collect()
            })
            .unwrap_or_default()
    };
    l.custom_tool_effects = pairs("customToolEffects").into_iter().collect();
    l.plugin_tool_effects = pairs("pluginToolEffects").into_iter().collect();
    l
}

fn s(e: ToolEffect) -> Value {
    Value::String(e.as_str().to_string())
}

#[test]
fn tool_effects_match_v4() {
    let Ok(path) = std::env::var("QT_ORACLE_TOOL_EFFECTS") else {
        eprintln!("SKIP: QT_ORACLE_TOOL_EFFECTS not set");
        return;
    };
    let text = std::fs::read_to_string(&path).expect("read oracle");
    let mut failures = Vec::new();
    let mut rows = 0usize;
    let mut kinds = std::collections::BTreeMap::<String, usize>::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Value = serde_json::from_str(line).expect("row json");
        let kind = row["kind"].as_str().unwrap().to_string();
        let id = row["id"].as_str().unwrap().to_string();
        let want = row["out"].clone();
        let got: Value = match kind.as_str() {
            "table" => {
                let mut m = Map::new();
                for name in BUILT_IN_TOOL_EFFECT_NAMES {
                    let v = match tool_effect_spec_of(name).unwrap() {
                        ToolEffectSpec::Declared(e) => s(e),
                        ToolEffectSpec::Computed(c) => {
                            json!({ "computed": c.max_effect().as_str() })
                        }
                    };
                    m.insert(name.to_string(), v);
                }
                assert_eq!(
                    row["count"].as_u64(),
                    Some(BUILT_IN_TOOL_EFFECT_NAMES.len() as u64),
                    "table count"
                );
                Value::Object(m)
            }
            // `8becfc6ce`: the pin exports no `isComputedTool`, and v5 has no twin
            // (the unit-E source guard pins the absence of a twin).
            "noIsComputedTool" => Value::Bool(false),
            "effectOf" => {
                let name = row["name"].as_str().unwrap();
                let input = if row["inputAbsent"].as_bool() == Some(true) {
                    Value::Null
                } else {
                    row["input"].clone()
                };
                s(effect_of_tool_call(name, &input, &lookups(&row["ctx"])))
            }
            "maxEffect" => s(max_effect_of_tool(
                row["name"].as_str().unwrap(),
                &lookups(&row["ctx"]),
            )),
            "highest" => s(highest_tool_effect(
                row["effects"].as_array().unwrap().iter().map(effect),
            )),
            "within" => Value::Bool(is_within_ceiling(
                effect(&row["effect"]),
                effect(&row["ceiling"]),
            )),
            "maxPair" => s(max_tool_effect(effect(&row["a"]), effect(&row["b"]))),
            "lower" => {
                let ceiling = if row["ceiling"].is_null() {
                    None
                } else {
                    Some(effect(&row["ceiling"]))
                };
                s(lower_tool_effect(ceiling, effect(&row["other"])))
            }
            "refusal" => Value::String(describe_ceiling_refusal(
                row["name"].as_str().unwrap(),
                effect(&row["effect"]),
                effect(&row["ceiling"]),
            )),
            "usesAllowed" => {
                match uses_allowed_under(row["name"].as_str().unwrap(), effect(&row["ceiling"])) {
                    Some(n) => Value::String(n.to_string()),
                    None => Value::Null,
                }
            }
            "customTool" => match safe_parse(&row["def"]) {
                Ok(def) => {
                    assert_eq!(
                        row["parsed"].as_bool(),
                        Some(true),
                        "{id}: v5 parsed, v4 did not"
                    );
                    s(custom_tool_effect(&def))
                }
                Err(_) => {
                    assert_eq!(
                        row["parsed"].as_bool(),
                        Some(false),
                        "{id}: v4 parsed, v5 did not"
                    );
                    Value::Null
                }
            },
            other => panic!("unknown kind {other}"),
        };
        rows += 1;
        *kinds.entry(kind.clone()).or_default() += 1;
        // Byte compare (serde_json's Map `==` ignores key order).
        let (g, w) = (
            serde_json::to_string(&got).unwrap(),
            serde_json::to_string(&want).unwrap(),
        );
        if g != w {
            failures.push(format!("{kind}/{id}:\n  v5: {g}\n  v4: {w}"));
        }
    }
    eprintln!("tool_effects_equivalence: {rows} rows {kinds:?}");
    assert!(
        rows > 100,
        "too few oracle rows ({rows}) — stale or truncated NDJSON"
    );
    assert!(
        failures.is_empty(),
        "{} mismatches:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
