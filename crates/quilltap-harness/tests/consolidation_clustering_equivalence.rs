//! P4.D271 tier-1 differential: v4's consolidation clusterer
//! (`lib/memory/consolidation-clustering.ts`, `d58548051`, byte-identical at
//! the `01a83539d` pin) against `quilltap_core::consolidation_clustering` —
//! `cosine`, `clusterBucket`, `effectiveMinClusterSize`,
//! `clusterHasNewMaterial`, `selectClusters` over the oracle's emitted inputs
//! (seeded unit vectors at dims 3 / 8 / 16 / 384, the 0.72 threshold ± 1 f32
//! ulp, digest seeds capped at 2 / 3 / 30, episodic windows, ties at every
//! strict `>`, budgets 0 / 1 / 40 / NaN). Comparands: ids, order, kinds,
//! digest ids exactly; scores and cosines at 1e-12.
//!
//! Regenerate + run (a pure tsx oracle, no fixture):
//!   V5W=${V5W:-$HOME/source/quilltap-v5}
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   rm -f /tmp/qt-consolidation-clustering.ndjson
//!   cd ~/source/quilltap-server
//!   $N/npx tsx $V5W/harness/oracle/cases/consolidation-clustering.ts > /tmp/qt-consolidation-clustering.ndjson
//!   cd $V5W
//!   QT_ORACLE_CONSOLIDATION_CLUSTERING=/tmp/qt-consolidation-clustering.ndjson \
//!     cargo test -p quilltap-harness --test consolidation_clustering_equivalence -- --nocapture

use std::collections::HashMap;

use quilltap_core::consolidation_clustering::{
    cluster_bucket, cluster_has_new_material, cosine, effective_min_cluster_size, select_clusters,
    Cluster, ClusterItem, ClusterKind, ClusterParams, SelectClustersOptions,
};
use serde_json::{json, Value};

fn f32s(v: &Value) -> Vec<f32> {
    v.as_array()
        .expect("embedding array")
        .iter()
        .map(|x| {
            let d = x.as_f64().expect("number");
            let f = d as f32;
            // R-H: every emitted value IS an f32 promoted to a double.
            assert_eq!(f as f64, d, "embedding value {d} is not an exact f32");
            f
        })
        .collect()
}

fn kind(v: &Value) -> ClusterKind {
    match v.as_str() {
        Some("semantic") => ClusterKind::Semantic,
        Some("episodic") => ClusterKind::Episodic,
        other => panic!("kind {other:?}"),
    }
}

fn opt_f64(v: &Value) -> Option<f64> {
    if v.is_null() {
        None
    } else {
        Some(v.as_f64().expect("number"))
    }
}

fn item(v: &Value) -> ClusterItem {
    ClusterItem {
        id: v["id"].as_str().unwrap().to_string(),
        embedding: f32s(&v["embedding"]),
        kind: kind(&v["kind"]),
        is_digest: v["isDigest"].as_bool().unwrap(),
        event_time_ms: opt_f64(&v["eventTimeMs"]),
        created_at_ms: v["createdAtMs"].as_f64().unwrap(),
        considered_at_ms: opt_f64(&v["consideredAtMs"]),
        weight: v["weight"].as_f64().unwrap(),
    }
}

fn cluster_from(v: &Value) -> Cluster {
    Cluster {
        kind: kind(&v["kind"]),
        digest_id: v["digestId"].as_str().map(str::to_string),
        member_ids: v["memberIds"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x.as_str().unwrap().to_string())
            .collect(),
        score: v["score"].as_f64().unwrap(),
    }
}

fn cluster_json(c: &Cluster) -> Value {
    json!({
        "kind": c.kind.as_str(),
        "digestId": c.digest_id,
        "memberIds": c.member_ids,
        "score": c.score,
    })
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
fn consolidation_clustering_matches_oracle() {
    let Ok(path) = std::env::var("QT_ORACLE_CONSOLIDATION_CLUSTERING") else {
        eprintln!(
            "SKIP: set QT_ORACLE_CONSOLIDATION_CLUSTERING to the oracle NDJSON (see header)."
        );
        return;
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read oracle {path}: {e}"));
    let mut failures = Vec::new();
    let mut counts: HashMap<String, usize> = HashMap::new();
    let mut multi_member_clusters = 0usize;
    let mut attracted = 0usize;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let c: Value = serde_json::from_str(line).expect("oracle line");
        let case = c["case"].as_str().unwrap().to_string();
        *counts.entry(case.clone()).or_default() += 1;
        match case.as_str() {
            "cosine" => {
                let got = cosine(&f32s(&c["a"]), &f32s(&c["b"]));
                let want = c["out"].as_f64().unwrap();
                // Exact: the same f64 summation order over the same f32s.
                if got != want {
                    failures.push(format!("cosine: v5 {got} v4 {want}"));
                }
            }
            "clusterBucket" => {
                let items: Vec<ClusterItem> =
                    c["items"].as_array().unwrap().iter().map(item).collect();
                let p = &c["params"];
                let params = ClusterParams {
                    threshold: p["threshold"].as_f64().unwrap(),
                    max_cluster_size: p["maxClusterSize"].as_f64().unwrap(),
                    episodic_window_ms: opt_f64(&p["episodicWindowMs"]),
                };
                let got = Value::Array(
                    cluster_bucket(&items, &params)
                        .iter()
                        .map(cluster_json)
                        .collect(),
                );
                for cl in c["clusters"].as_array().unwrap() {
                    if cl["memberIds"].as_array().unwrap().len() > 1 {
                        multi_member_clusters += 1;
                    }
                    if !cl["digestId"].is_null() {
                        attracted += 1;
                    }
                }
                if !approx_eq(&got, &c["clusters"]) {
                    failures.push(format!(
                        "clusterBucket {}:\n  v5 {got}\n  v4 {}",
                        c["name"], c["clusters"]
                    ));
                }
            }
            "effectiveMinClusterSize" => {
                let created: Vec<f64> = c["created"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|x| x.as_f64().unwrap())
                    .collect();
                let got = effective_min_cluster_size(
                    &created,
                    c["minClusterSize"].as_f64().unwrap(),
                    c["matureAfterDays"].as_f64().unwrap(),
                    c["nowMs"].as_f64().unwrap(),
                );
                if got != c["out"].as_f64().unwrap() {
                    failures.push(format!("effectiveMinClusterSize {c}: v5 {got}"));
                }
            }
            "clusterHasNewMaterial" => {
                let members: Vec<ClusterItem> =
                    c["members"].as_array().unwrap().iter().map(item).collect();
                let refs: Vec<&ClusterItem> = members.iter().collect();
                let got = cluster_has_new_material(&refs);
                if got != c["out"].as_bool().unwrap() {
                    failures.push(format!("clusterHasNewMaterial {c}: v5 {got}"));
                }
            }
            "selectClusters" => {
                let items: HashMap<String, ClusterItem> = c["items"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| {
                        let it = item(v);
                        (it.id.clone(), it)
                    })
                    .collect();
                let clusters: Vec<Cluster> = c["clusters"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(cluster_from)
                    .collect();
                let o = &c["opts"];
                let max_clusters = match &o["maxClusters"] {
                    Value::String(s) if s == "NaN" => f64::NAN,
                    v => v.as_f64().unwrap(),
                };
                let opts = SelectClustersOptions {
                    min_cluster_size: o["minClusterSize"].as_f64().unwrap(),
                    mature_after_days: o["matureAfterDays"].as_f64().unwrap(),
                    max_clusters,
                    now_ms: o["nowMs"].as_f64().unwrap(),
                };
                let sel = select_clusters(&clusters, &items, &opts);
                let arr = |v: &[Cluster]| Value::Array(v.iter().map(cluster_json).collect());
                let got = json!({
                    "selected": arr(&sel.selected),
                    "deferred": arr(&sel.deferred),
                    "belowMin": arr(&sel.below_min),
                    "stale": arr(&sel.stale),
                });
                if !approx_eq(&got, &c["out"]) {
                    failures.push(format!(
                        "selectClusters {o}:\n  v5 {got}\n  v4 {}",
                        c["out"]
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
    let n = |k: &str| counts.get(k).copied().unwrap_or(0);
    assert!(
        n("clusterBucket") >= 60,
        "clusterBucket cases: {}",
        n("clusterBucket")
    );
    assert!(n("selectClusters") >= 21 && n("cosine") >= 8, "{counts:?}");
    assert!(
        n("effectiveMinClusterSize") >= 9 && n("clusterHasNewMaterial") >= 6,
        "{counts:?}"
    );
    // The corpus must actually exercise merging and attraction.
    assert!(
        multi_member_clusters >= 40,
        "multi-member clusters: {multi_member_clusters}"
    );
    assert!(attracted >= 10, "digest-seeded clusters: {attracted}");
    eprintln!(
        "OK: consolidation clustering matched oracle ({counts:?}; {multi_member_clusters} multi-member, {attracted} digest-seeded)."
    );
}
