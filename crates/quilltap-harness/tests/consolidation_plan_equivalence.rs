//! P4.D271 tier-1 differential: v4's consolidation write planner
//! (`lib/memory/consolidation-plan.ts`, `d58548051`, byte-identical at the
//! `01a83539d` pin) against `quilltap_core::consolidation_plan` — the WHOLE plan
//! of `planConsolidationWrites` (creates with `data` in v4's literal key order,
//! updates with `patch` in v4's order, tier moves, considered, link rewrites,
//! the `supersededBy` Map as ordered entries, the per-cluster digests incl.
//! their f32 embeddings) under a deterministic `newId` (`d1`, `d2`, …), plus
//! `sharedValue`, `deriveWitnessedContext`, `clampDigestImportance` and
//! `normalizeDigestKeywords`. Key order is compared exactly; numbers at 1e-12.
//!
//! Regenerate + run (a pure tsx oracle, no fixture):
//!   V5W=${V5W:-$HOME/source/quilltap-v5}
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   rm -f /tmp/qt-consolidation-plan.ndjson
//!   cd ~/source/quilltap-server
//!   $N/npx tsx $V5W/harness/oracle/cases/consolidation-plan.ts > /tmp/qt-consolidation-plan.ndjson
//!   cd $V5W
//!   QT_ORACLE_CONSOLIDATION_PLAN=/tmp/qt-consolidation-plan.ndjson \
//!     cargo test -p quilltap-harness --test consolidation_plan_equivalence -- --nocapture

use quilltap_core::consolidation_clustering::ClusterKind;
use quilltap_core::consolidation_plan::{
    clamp_digest_importance, derive_witnessed_context, normalize_digest_keywords,
    plan_consolidation_writes, shared_value, LinkIndexRow, OrderedMap, PlanInput,
    ResolvedClusterOutcome, DIGEST_REINFORCEMENT_CAP,
};
use quilltap_core::consolidation_tasks::{ConsolidationContradiction, ConsolidationDigestOutput};
use serde_json::{json, Value};

fn strs(v: &Value) -> Vec<String> {
    v.as_array()
        .map(|a| a.iter().map(|x| x.as_str().unwrap().to_string()).collect())
        .unwrap_or_default()
}

fn opt_strs(v: &Value) -> Vec<Option<String>> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_str().map(str::to_string))
        .collect()
}

fn digest_out(v: &Value) -> ConsolidationDigestOutput {
    ConsolidationDigestOutput {
        content: v["content"].as_str().unwrap().to_string(),
        summary: v["summary"].as_str().unwrap().to_string(),
        keywords: strs(&v["keywords"]),
        importance: v["importance"].as_f64().unwrap(),
        kind: v["kind"].as_str().unwrap().to_string(),
        occurred_at: v.get("occurredAt").map(|o| o.as_str().map(str::to_string)),
        member_ids: strs(&v["memberIds"]),
    }
}

fn cluster(v: &Value) -> ResolvedClusterOutcome {
    ResolvedClusterOutcome {
        about_character_id: v["aboutCharacterId"].as_str().map(str::to_string),
        cluster_kind: if v["clusterKind"] == "episodic" {
            ClusterKind::Episodic
        } else {
            ClusterKind::Semantic
        },
        existing_digest: if v["existingDigest"].is_null() {
            None
        } else {
            Some(v["existingDigest"].clone())
        },
        members: v["members"].as_array().unwrap().clone(),
        digests: v["digests"]
            .as_array()
            .unwrap()
            .iter()
            .map(digest_out)
            .collect(),
        keep_standalone: strs(&v["keepStandalone"]),
        contradictions: v["contradictions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| ConsolidationContradiction {
                older_id: c["olderId"].as_str().unwrap().to_string(),
                newer_id: c["newerId"].as_str().unwrap().to_string(),
                note: c["note"].as_str().unwrap_or_default().to_string(),
            })
            .collect(),
        embeddings: v["embeddings"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| {
                e.as_array()
                    .map(|a| a.iter().map(|x| x.as_f64().unwrap() as f32).collect())
            })
            .collect(),
    }
}

/// Structural equality with numbers compared at 1e-12 (key order included).
fn approx_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => {
            let (x, y) = (x.as_f64().unwrap(), y.as_f64().unwrap());
            x == y || (x - y).abs() <= 1e-12
        }
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(p, q)| approx_eq(p, q))
        }
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len()
                && x.iter()
                    .zip(y.iter())
                    .all(|((k1, v1), (k2, v2))| k1 == k2 && approx_eq(v1, v2))
        }
        _ => a == b,
    }
}

#[test]
fn consolidation_plan_matches_oracle() {
    let Ok(path) = std::env::var("QT_ORACLE_CONSOLIDATION_PLAN") else {
        eprintln!("SKIP: set QT_ORACLE_CONSOLIDATION_PLAN to the oracle NDJSON (see header).");
        return;
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read oracle {path}: {e}"));
    let mut failures = Vec::new();
    let (mut plans, mut helpers, mut creates, mut updates, mut moves, mut rewrites) =
        (0, 0, 0, 0, 0, 0);
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let c: Value = serde_json::from_str(line).expect("oracle line");
        match c["case"].as_str().unwrap() {
            "constants" => {
                assert_eq!(
                    c["DIGEST_REINFORCEMENT_CAP"].as_f64().unwrap(),
                    DIGEST_REINFORCEMENT_CAP
                );
            }
            "plan" => {
                plans += 1;
                let clusters: Vec<ResolvedClusterOutcome> = c["clusters"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(cluster)
                    .collect();
                let mut row_index: OrderedMap<LinkIndexRow> = OrderedMap::new();
                for r in c["rowIndex"].as_array().unwrap() {
                    let id = r["id"].as_str().unwrap().to_string();
                    row_index.set(
                        &id,
                        LinkIndexRow {
                            id: id.clone(),
                            related_memory_ids: strs(&r["relatedMemoryIds"]),
                        },
                    );
                }
                let mut n = 0;
                let mut new_id = || {
                    n += 1;
                    format!("d{n}")
                };
                let plan = plan_consolidation_writes(PlanInput {
                    character_id: c["characterId"].as_str().unwrap(),
                    clusters: &clusters,
                    row_index: &row_index,
                    now_iso: c["nowIso"].as_str().unwrap(),
                    new_id: &mut new_id,
                });
                creates += plan.creates.len();
                updates += plan.updates.len();
                moves += plan.tier_moves.len();
                rewrites += plan.link_rewrites.len();
                let got = json!({
                    "creates": plan.creates.iter().map(|p| json!({"digest": p.digest.to_json(), "data": p.data_json()})).collect::<Vec<_>>(),
                    "updates": plan.updates.iter().map(|p| json!({"digest": p.digest.to_json(), "patch": p.patch_json})).collect::<Vec<_>>(),
                    "tierMoves": plan.tier_moves.iter().map(|m| json!({"supersededById": m.superseded_by_id, "ids": m.ids})).collect::<Vec<_>>(),
                    "considered": plan.considered,
                    "linkRewrites": plan.link_rewrites.iter().map(|r| json!({"id": r.id, "relatedMemoryIds": r.related_memory_ids})).collect::<Vec<_>>(),
                    "supersededBy": plan.superseded_by.iter().map(|(k, v)| json!([k, v])).collect::<Vec<_>>(),
                    "digestsByCluster": plan.digests_by_cluster.iter().map(|ds| ds.iter().map(|d| d.to_json()).collect::<Vec<_>>()).collect::<Vec<_>>(),
                });
                if !approx_eq(&got, &c["plan"]) {
                    failures.push(format!(
                        "plan {}:\n  v5 {got}\n  v4 {}",
                        c["name"], c["plan"]
                    ));
                }
            }
            "sharedValue" => {
                helpers += 1;
                let got = shared_value(&opt_strs(&c["values"]));
                if json!(got) != c["out"] {
                    failures.push(format!(
                        "sharedValue {}: v5 {got:?} v4 {}",
                        c["values"], c["out"]
                    ));
                }
            }
            "deriveWitnessedContext" => {
                helpers += 1;
                let got = derive_witnessed_context(&opt_strs(&c["values"]));
                if json!(got) != c["out"] {
                    failures.push(format!(
                        "deriveWitnessedContext {}: v5 {got:?} v4 {}",
                        c["values"], c["out"]
                    ));
                }
            }
            "clampDigestImportance" => {
                helpers += 1;
                let rows: Vec<Value> = c["rows"].as_array().unwrap().clone();
                let refs: Vec<&Value> = rows.iter().collect();
                let got = clamp_digest_importance(c["model"].as_f64().unwrap(), &refs);
                let want = c["out"].as_f64().unwrap();
                if (got - want).abs() > 1e-12 {
                    failures.push(format!(
                        "clampDigestImportance {}: v5 {got} v4 {want}",
                        c["model"]
                    ));
                }
            }
            "normalizeDigestKeywords" => {
                helpers += 1;
                let members: Vec<Vec<String>> =
                    c["members"].as_array().unwrap().iter().map(strs).collect();
                let got = normalize_digest_keywords(
                    &strs(&c["model"]),
                    &members,
                    c["kind"].as_str().unwrap(),
                );
                if json!(got) != c["out"] {
                    failures.push(format!(
                        "normalizeDigestKeywords {}: v5 {got:?} v4 {}",
                        c["model"], c["out"]
                    ));
                }
            }
            other => panic!("unknown case {other}"),
        }
    }
    assert!(
        failures.is_empty(),
        "{} failure(s):\n{}",
        failures.len(),
        failures.join("\n")
    );
    assert!(
        plans >= 16 && helpers >= 30,
        "plans {plans} helpers {helpers}"
    );
    assert!(
        creates >= 12 && updates >= 1 && moves >= 10 && rewrites >= 5,
        "c {creates} u {updates} m {moves} r {rewrites}"
    );
    eprintln!(
        "OK: consolidation plan matched oracle ({plans} plans: {creates} creates, {updates} updates, {moves} tier moves, {rewrites} link rewrites; {helpers} helper rows)."
    );
}
