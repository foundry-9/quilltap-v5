//! Tier-1 differential (P4.D295 unit 1): the pure half of v4's
//! scenarios-from-anywhere slice (`545e299bb`, pinned at `83334b9a5`), exact
//! against v4's REAL exports —
//!
//! - V8's `decodeURIComponent` ↔ `scenarios::decode_uri::js_decode_uri_component`;
//! - `normalizeScenarioPath` / `characterScenarioIdForPath` ↔
//!   `scenarios::location::{normalize_scenario_path, character_scenario_id_for_path}`;
//! - `scenarioExcerpt` ↔ `scenarios::everywhere::scenario_excerpt`, compared
//!   in UTF-16 code units under the ruled table [`EXCERPT_LONE_SURROGATE`];
//! - `hashScenarioBody` / `sameResolvedScenario` ↔
//!   `services::scenario_selection::{hash_scenario_body, same_resolved_scenario}`;
//! - `combineScenarioText` / `recoverPresetBody` ↔ `scenario_text::*`;
//! - `keepCurrentScene` ↔ `services::chat_scenario::keep_current_scene`.
//!
//! Generate the oracle output (Node 24), from the v4 checkout (a pin worktree
//! when v4 HEAD is past the round's baseline):
//!   cd ~/source/quilltap-server
//!   TZ=UTC npx tsx <V5>/harness/oracle/cases/scenario-pure.ts \
//!     > /tmp/oracle-scenario-pure.ndjson
//! Run:
//!   QT_ORACLE_SCENARIO_PURE=/tmp/oracle-scenario-pure.ndjson \
//!     cargo test -p quilltap-harness --test scenario_pure_equivalence

use std::collections::BTreeMap;

use quilltap_core::db::chats::ScenarioSource;
use quilltap_core::db::scenarios::ScenarioPathResolution;
use quilltap_core::scenario_text::{combine_scenario_text, recover_preset_body};
use quilltap_core::scenarios::decode_uri::js_decode_uri_component;
use quilltap_core::scenarios::everywhere::scenario_excerpt;
use quilltap_core::scenarios::location::{character_scenario_id_for_path, normalize_scenario_path};
use quilltap_core::services::chat_scenario::keep_current_scene;
use quilltap_core::services::scenario_selection::{
    hash_scenario_body, same_resolved_scenario, ResolvedScenario,
};
use serde_json::Value;

/// Ruled divergence `EXCERPT_LONE_SURROGATE` (R-AA). v4's
/// `collapsed.slice(0, 240)` counts UTF-16 units and can cut a surrogate pair
/// in half, so its excerpt carries a LONE high surrogate at unit 239 — a value
/// a Rust `String` cannot hold. v5 emits U+FFFD at that unit instead; every
/// other unit is v4's. Both ways: a row named here MUST show v4's lone
/// surrogate (so a v4 convergence — cutting before the pair, say — trips this
/// table), and a row NOT named here must be unit-identical.
const EXCERPT_LONE_SURROGATE: &[&str] = &["emoji_straddles_239_240"];

/// The corpus's per-kind floor (a truncated oracle must not pass).
const KIND_FLOORS: &[(&str, usize)] = &[
    ("decode", 57),
    ("normalize", 42),
    ("idForPath", 6),
    ("excerpt", 23),
    ("hash", 6),
    ("combine", 15),
    ("recover", 16),
    ("same", 10),
    ("keep", 13),
];

fn s(v: &Value, k: &str) -> Option<String> {
    v.get(k).and_then(Value::as_str).map(str::to_string)
}

fn source(v: &Value) -> Option<ScenarioSource> {
    if v.is_null() {
        return None;
    }
    Some(ScenarioSource {
        mount_point_id: s(v, "mountPointId").unwrap(),
        path: s(v, "path").unwrap(),
        body_sha256: s(v, "bodySha256").unwrap(),
    })
}

fn recipe(v: &Value) -> ResolvedScenario {
    ResolvedScenario {
        scenario_text: s(v, "scenarioText"),
        scenario_source: source(&v["scenarioSource"]),
        scenario_notes: s(v, "scenarioNotes"),
    }
}

/// The recipe in v4's key order, as JSON — what `keepCurrentScene` returns.
fn recipe_json(r: &ResolvedScenario) -> Value {
    let mut m = serde_json::Map::new();
    m.insert(
        "scenarioText".into(),
        r.scenario_text.clone().map_or(Value::Null, Value::String),
    );
    m.insert(
        "scenarioSource".into(),
        r.scenario_source
            .as_ref()
            .map_or(Value::Null, |src| serde_json::to_value(src).unwrap()),
    );
    m.insert(
        "scenarioNotes".into(),
        r.scenario_notes.clone().map_or(Value::Null, Value::String),
    );
    Value::Object(m)
}

/// v4's lone high surrogate, if its excerpt carries one: the index.
fn lone_high_surrogate(units: &[u16]) -> Option<usize> {
    units.iter().enumerate().find_map(|(i, &u)| {
        let next_is_low = units
            .get(i + 1)
            .is_some_and(|n| (0xDC00..=0xDFFF).contains(n));
        ((0xD800..=0xDBFF).contains(&u) && !next_is_low).then_some(i)
    })
}

#[test]
fn scenario_pure_matches_v4() {
    let path = match std::env::var("QT_ORACLE_SCENARIO_PURE") {
        Ok(p) if !p.is_empty() => p,
        _ => {
            println!("SKIP: set QT_ORACLE_SCENARIO_PURE to the oracle NDJSON (see header).");
            return;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path}: {e}"));
    let rows: Vec<Value> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap_or_else(|e| panic!("parse row {l}: {e}")))
        .collect();

    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut failures: Vec<String> = Vec::new();
    let mut ruled_seen: Vec<String> = Vec::new();

    for row in &rows {
        let kind = s(row, "kind").expect("kind");
        *counts.entry(kind.clone()).or_default() += 1;
        match kind.as_str() {
            "decode" => {
                let input = s(row, "input").unwrap();
                let got = js_decode_uri_component(&input);
                let ok = row["ok"].as_bool().unwrap();
                match (&got, ok) {
                    (Ok(out), true) if Some(out.clone()) == s(row, "output") => {}
                    (Err(e), false) => {
                        // V8's whole error, `URIError: URI malformed`.
                        if s(row, "error").as_deref() != Some(&format!("URIError: {e}")) {
                            failures.push(format!(
                                "decode {input:?}: error text {:?} vs {e}",
                                s(row, "error")
                            ));
                        }
                    }
                    _ => failures.push(format!("decode {input:?}: v4 {row} vs v5 {got:?}")),
                }
            }
            "normalize" => {
                let input = s(row, "input").unwrap();
                let got = normalize_scenario_path(&input);
                let want_threw = row["threw"].as_bool().unwrap();
                let agrees = match &got {
                    Err(e) => want_threw && s(row, "error") == Some(format!("URIError: {e}")),
                    Ok(ScenarioPathResolution::Ok(p)) => {
                        !want_threw && row["ok"] == true && s(row, "path").as_deref() == Some(p)
                    }
                    Ok(ScenarioPathResolution::Err(e)) => {
                        !want_threw && row["ok"] == false && s(row, "error").as_deref() == Some(e)
                    }
                };
                if !agrees {
                    failures.push(format!("normalize {input:?}: v4 {row} vs v5 {got:?}"));
                }
            }
            "idForPath" => {
                let got = character_scenario_id_for_path(
                    &s(row, "mountPointId").unwrap(),
                    &s(row, "path").unwrap(),
                );
                if Some(got.clone()) != s(row, "id") {
                    failures.push(format!("idForPath {row} vs {got}"));
                }
            }
            "excerpt" => {
                let name = s(row, "name").unwrap();
                let want: Vec<u16> = row["excerptUnits"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|u| u.as_u64().unwrap() as u16)
                    .collect();
                let got: Vec<u16> = scenario_excerpt(&s(row, "body").unwrap())
                    .encode_utf16()
                    .collect();
                let ruled = EXCERPT_LONE_SURROGATE.contains(&name.as_str());
                match (lone_high_surrogate(&want), ruled) {
                    (Some(k), true) => {
                        ruled_seen.push(name.clone());
                        let mut expected = want.clone();
                        expected[k] = 0xFFFD;
                        if got != expected {
                            failures.push(format!("excerpt {name}: v5 is not v4 with U+FFFD at unit {k}"));
                        }
                    }
                    (None, true) => failures.push(format!(
                        "excerpt {name}: ruled EXCERPT_LONE_SURROGATE but v4 no longer leaves a lone surrogate — retire the row"
                    )),
                    (Some(k), false) => failures.push(format!(
                        "excerpt {name}: v4 leaves a lone surrogate at unit {k} but the row is not in EXCERPT_LONE_SURROGATE"
                    )),
                    (None, false) => {
                        if got != want {
                            failures.push(format!(
                                "excerpt {name}: v4 {:?} vs v5 {:?}",
                                String::from_utf16_lossy(&want),
                                String::from_utf16_lossy(&got)
                            ));
                        }
                    }
                }
            }
            "hash" => {
                let got = hash_scenario_body(&s(row, "body").unwrap());
                if Some(got.clone()) != s(row, "hex") {
                    failures.push(format!("hash {row} vs {got}"));
                }
            }
            "combine" => {
                let got =
                    combine_scenario_text(s(row, "preset").as_deref(), s(row, "free").as_deref());
                if got != s(row, "output") {
                    failures.push(format!("combine {row} vs {got:?}"));
                }
            }
            "recover" => {
                let got =
                    recover_preset_body(s(row, "text").as_deref(), s(row, "notes").as_deref());
                if got != s(row, "output") {
                    failures.push(format!("recover {row} vs {got:?}"));
                }
            }
            "same" => {
                let got = same_resolved_scenario(&recipe(&row["a"]), &recipe(&row["b"]));
                if Some(got) != row["output"].as_bool() {
                    failures.push(format!(
                        "same {}: v4 {} vs v5 {got}",
                        row["name"], row["output"]
                    ));
                }
            }
            "keep" => {
                let got = keep_current_scene(&recipe(&row["next"]), &recipe(&row["current"]));
                let got_json = got.as_ref().map_or(Value::Null, recipe_json);
                // Byte-compare (serde_json Map equality ignores key order).
                if serde_json::to_string(&got_json).unwrap()
                    != serde_json::to_string(&row["output"]).unwrap()
                {
                    failures.push(format!(
                        "keep {}: v4 {} vs v5 {got_json}",
                        row["name"], row["output"]
                    ));
                }
            }
            other => failures.push(format!("unknown row kind {other}")),
        }
    }

    for (kind, floor) in KIND_FLOORS {
        let n = counts.get(*kind).copied().unwrap_or(0);
        assert!(
            n >= *floor,
            "kind {kind}: {n} rows, floor {floor} — truncated oracle?"
        );
    }
    for name in EXCERPT_LONE_SURROGATE {
        assert!(
            ruled_seen.iter().any(|n| n == name),
            "EXCERPT_LONE_SURROGATE names {name}, which the corpus no longer carries"
        );
    }
    assert!(
        failures.is_empty(),
        "{} mismatch(es):\n{}",
        failures.len(),
        failures.join("\n")
    );
    println!("scenario_pure: {} rows match v4 ({counts:?})", rows.len());
}
