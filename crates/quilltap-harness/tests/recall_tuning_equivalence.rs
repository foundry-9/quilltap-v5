//! Tier-1 differential (P4.D267): recall tuning — v4 `lib/memory/recall-tuning.ts`
//! (`f7d8064be`, the recall multiplier retuning; pinned at `01a83539d`).
//!
//! Every oracle row carries its INPUT; this side recomputes from the same bytes
//! through v5's REAL `quilltap_core::recall_tuning` and compares:
//! - `default` — `DEFAULT_RECALL_TUNING` field for field;
//! - `resolve` — the input validated (as the route does), resolved, described:
//!   every resolved field (floats 1e-12) and the `describe` string exactly;
//! - `select` — `selectSpecificAnchors`' chosen list exactly;
//! - `tuningParse` / `signalsParse` — `RecallTuningInputSchema.optional()` /
//!   `RecallReplaySignalsSchema.optional()` `.safeParse`: success, and the
//!   parsed `data` or the `issues` as JSON STRINGS — byte-compared, so key
//!   order is a comparand (serde_json's `Map` equality is order-blind).
//!
//! Generate the oracle (from the v4 checkout, or a worktree pinned at the
//! baseline):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   $N/npx tsx $V5W/harness/oracle/cases/recall-tuning.ts \
//!     > /tmp/oracle-recall-tuning.ndjson
//! Run:
//!   QT_ORACLE_RECALL_TUNING=/tmp/oracle-recall-tuning.ndjson \
//!     cargo test -p quilltap-harness --test recall_tuning_equivalence -- --nocapture

use quilltap_core::recall_tags::{RecallMultiplierTable, RECALL_MULTIPLIER_KEYS};
use quilltap_core::recall_tuning::{
    describe_recall_tuning, resolve_recall_tuning, select_specific_anchors,
    validate_recall_replay_signals, validate_recall_tuning_input, AnchorOrder,
    ResolvedRecallTuning, DEFAULT_RECALL_TUNING,
};
use serde_json::Value;

const EPS: f64 = 1e-12;

fn feq(a: f64, b: f64, what: &str) {
    assert!((a - b).abs() <= EPS, "{what}: rust={a} oracle={b}");
}

fn assert_table(got: &RecallMultiplierTable, want: &Value, id: &str) {
    let obj = want.as_object().expect("multipliers object");
    let keys: Vec<&str> = obj.keys().map(String::as_str).collect();
    assert_eq!(keys, RECALL_MULTIPLIER_KEYS, "{id}: table key order");
    for k in RECALL_MULTIPLIER_KEYS {
        feq(
            got.get(k).unwrap(),
            obj[k].as_f64().unwrap(),
            &format!("{id}: multipliers.{k}"),
        );
    }
}

fn assert_tuning(got: &ResolvedRecallTuning, want: &Value, id: &str) {
    assert_table(&got.multipliers, &want["multipliers"], id);
    match (&got.boost_gate, &want["boostGate"]) {
        (None, Value::Null) => {}
        (Some(g), Value::Object(w)) => {
            let opt = |v: &Value| v.as_f64();
            assert_eq!(
                g.abs.is_some(),
                opt(&w["abs"]).is_some(),
                "{id}: gate.abs null"
            );
            assert_eq!(
                g.margin.is_some(),
                opt(&w["margin"]).is_some(),
                "{id}: gate.margin null"
            );
            if let (Some(a), Some(b)) = (g.abs, opt(&w["abs"])) {
                feq(a, b, &format!("{id}: gate.abs"));
            }
            if let (Some(a), Some(b)) = (g.margin, opt(&w["margin"])) {
                feq(a, b, &format!("{id}: gate.margin"));
            }
            feq(
                g.ramp,
                w["ramp"].as_f64().unwrap(),
                &format!("{id}: gate.ramp"),
            );
        }
        (g, w) => panic!("{id}: boostGate rust={g:?} oracle={w}"),
    }
    feq(
        got.boost_cap,
        want["boostCap"].as_f64().unwrap(),
        &format!("{id}: boostCap"),
    );
    assert_eq!(
        Value::Bool(got.fresh_bypasses_gate),
        want["freshBypassesGate"],
        "{id}"
    );
    assert_eq!(
        Value::Bool(got.window_bypasses_gate),
        want["windowBypassesGate"],
        "{id}"
    );
    assert_eq!(
        Value::Bool(got.specific_anchors),
        want["specificAnchors"],
        "{id}"
    );
    assert_eq!(
        got.anchor_min_hits,
        want["anchorMinHits"].as_i64().unwrap(),
        "{id}: anchorMinHits"
    );
    assert_eq!(
        got.anchor_order.as_str(),
        want["anchorOrder"],
        "{id}: anchorOrder"
    );
    feq(
        got.background_reserve,
        want["backgroundReserve"].as_f64().unwrap(),
        &format!("{id}: backgroundReserve"),
    );
}

fn order_of(v: &Value) -> Option<AnchorOrder> {
    match v.as_str() {
        Some("rarest") => Some(AnchorOrder::Rarest),
        Some("distiller") => Some(AnchorOrder::Distiller),
        _ => None,
    }
}

#[test]
fn recall_tuning_matches_oracle() {
    let Ok(path) = std::env::var("QT_ORACLE_RECALL_TUNING") else {
        eprintln!("SKIP: set QT_ORACLE_RECALL_TUNING to the oracle NDJSON (see test header).");
        return;
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));
    let rows: Vec<Value> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert!(!rows.is_empty(), "empty oracle — regenerate it");
    let (mut n_default, mut n_resolve, mut n_select, mut n_tparse, mut n_sparse) = (0, 0, 0, 0, 0);
    let to_string = |v: &Value| serde_json::to_string(v).unwrap();

    for row in &rows {
        let id = row["id"].as_str().unwrap_or("default");
        match row["kind"].as_str().unwrap() {
            "default" => {
                assert_tuning(
                    &DEFAULT_RECALL_TUNING,
                    &row["tuning"],
                    "DEFAULT_RECALL_TUNING",
                );
                n_default += 1;
            }
            "resolve" => {
                let input = row.get("input").map(|raw| {
                    validate_recall_tuning_input(raw)
                        .unwrap_or_else(|e| panic!("{id}: rust refused a v4-valid input: {e:?}"))
                        .0
                });
                let resolved = resolve_recall_tuning(input.as_ref());
                assert_tuning(&resolved, &row["resolved"], id);
                assert_eq!(
                    describe_recall_tuning(&resolved),
                    row["described"].as_str().unwrap(),
                    "{id}: describe"
                );
                n_resolve += 1;
            }
            "select" => {
                let candidates: Vec<(String, usize)> = row["candidates"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|c| {
                        (
                            c["phrase"].as_str().unwrap().to_string(),
                            c["count"].as_u64().unwrap() as usize,
                        )
                    })
                    .collect();
                let present: Vec<String> = row["present"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|p| p.as_str().unwrap().to_string())
                    .collect();
                let max = row["max"].as_u64().map(|m| m as usize).unwrap_or(3);
                let got = select_specific_anchors(
                    &candidates,
                    &present,
                    max,
                    row["minHits"].as_i64(),
                    order_of(&row["order"]),
                );
                let want: Vec<String> = row["chosen"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|p| p.as_str().unwrap().to_string())
                    .collect();
                assert_eq!(got, want, "{id}: chosen");
                n_select += 1;
            }
            kind @ ("tuningParse" | "signalsParse") => {
                // An absent `input` is v4's `undefined`: `.optional()` passes it
                // through (`data: undefined` → our `null` stand-in), and the
                // route never calls the validator for it.
                let got: Result<String, String> = match row.get("input") {
                    None => Ok("null".into()),
                    Some(raw) if kind == "tuningParse" => validate_recall_tuning_input(raw)
                        .map(|(_, data)| to_string(&data))
                        .map_err(|issues| serde_json::to_string(&issues).unwrap()),
                    Some(raw) => validate_recall_replay_signals(raw)
                        .map(|(_, data)| to_string(&data))
                        .map_err(|issues| serde_json::to_string(&issues).unwrap()),
                };
                if row["success"].as_bool().unwrap() {
                    let want = row["data"].as_str().unwrap();
                    assert_eq!(got, Ok(want.to_string()), "{kind} {id}: parsed data");
                } else {
                    let want = row["issues"].as_str().unwrap();
                    assert_eq!(got, Err(want.to_string()), "{kind} {id}: issues");
                }
                if kind == "tuningParse" {
                    n_tparse += 1;
                } else {
                    n_sparse += 1;
                }
            }
            other => panic!("unexpected row kind {other}"),
        }
    }
    assert!(n_default == 1 && n_resolve > 0 && n_select > 0 && n_tparse > 0 && n_sparse > 0);
    eprintln!(
        "OK: recall-tuning matched oracle ({n_default} default, {n_resolve} resolve, \
         {n_select} select, {n_tparse} tuningParse, {n_sparse} signalsParse)."
    );
}
