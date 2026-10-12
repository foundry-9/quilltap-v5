//! P4.D273 tier-1 differential: the restore's store claim map (v4
//! `lib/backup/restore/store-claims.ts` `makeStoreClaimMap`, `ed8b15b50` bug
//! 185) vs `quilltap_core::services::backup::restore::store_claims`.
//!
//! Both sides read the SAME corpus (`harness/oracle/fixtures/store-claims-
//! corpus.json`): each case builds ONE claim map from `carried` and runs its
//! `claims` in order, so a holder recorded by one claim is seen by the next.
//! Every outcome is rendered in v4's object shape (`{bound: true,
//! mountPointId}` / `{bound: false, reason, claimedBy?}`) and compared
//! EXACTLY — key order included.
//!
//! ## ⚠ `DUP_STORE_ID_FIRST_ROW` — the one ruled divergence (P4.D273 R-A)
//!
//! v4's `new Map(carriedStores.map(s => [s.id, s]))` keeps the LAST row of a
//! duplicated id; v5 keeps the FIRST, as both engines' 22a primary key does.
//! The cases in [`DUP_STORE_ID_FIRST_ROW`] are asserted BOTH ways: v5 must
//! answer the first row's verdict and v4 the last row's. If v4 adopts the
//! first row, the v4 leg fires — retire the table then.
//!
//! Generate the oracle output (Node 24, from the v4 checkout — pinned at or
//! after `ed8b15b50` until the baseline moves):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/store-claims.ts \
//!     > /tmp/oracle-store-claims.ndjson
//! Run:
//!   QT_ORACLE_STORE_CLAIMS=/tmp/oracle-store-claims.ndjson \
//!     cargo test -p quilltap-harness --test store_claims_equivalence -- --nocapture

use std::collections::HashMap;
use std::path::PathBuf;

use quilltap_core::services::backup::restore::store_claims::{
    make_store_claim_map, StoreClaim, StoreOwnerKind,
};
use serde_json::{json, Value};

/// `(case, v5's expected outcomes, v4's expected outcomes)` — the ruled
/// divergence, both directions, compact JSON per outcome.
const DUP_STORE_ID_FIRST_ROW: &[(&str, &[&str], &[&str])] = &[
    (
        // vault row FIRST, a documents row LAST under the same id.
        "dup_store_id_vault_first",
        &[
            r#"{"bound":true,"mountPointId":"vault-1"}"#,
            r#"{"bound":false,"reason":"wrong-kind"}"#,
        ],
        &[
            r#"{"bound":false,"reason":"wrong-kind"}"#,
            r#"{"bound":true,"mountPointId":"vault-1"}"#,
        ],
    ),
    (
        // documents row FIRST, the vault row LAST.
        "dup_store_id_documents_first",
        &[
            r#"{"bound":false,"reason":"wrong-kind"}"#,
            r#"{"bound":true,"mountPointId":"vault-1"}"#,
        ],
        &[
            r#"{"bound":true,"mountPointId":"vault-1"}"#,
            r#"{"bound":false,"reason":"wrong-kind"}"#,
        ],
    ),
];

fn corpus() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/store-claims-corpus.json");
    serde_json::from_str(&std::fs::read_to_string(&path).expect("read the corpus"))
        .expect("parse the corpus")
}

fn kind_of(s: &str) -> StoreOwnerKind {
    match s {
        "character" => StoreOwnerKind::Character,
        "project" => StoreOwnerKind::Project,
        "group" => StoreOwnerKind::Group,
        other => panic!("unknown kind {other:?}"),
    }
}

/// v4's `StoreClaim` object, in its literal key order.
fn render(claim: &StoreClaim) -> Value {
    match claim {
        StoreClaim::Bound { mount_point_id } => {
            json!({"bound": true, "mountPointId": mount_point_id})
        }
        StoreClaim::Fallback { reason, claimed_by } => {
            let mut o = serde_json::Map::new();
            o.insert("bound".into(), json!(false));
            o.insert("reason".into(), json!(reason.as_str()));
            if let Some((kind, id)) = claimed_by {
                o.insert("claimedBy".into(), json!({"kind": kind.as_str(), "id": id}));
            }
            Value::Object(o)
        }
    }
}

fn v5_outcomes(case: &Value) -> Vec<String> {
    let carried = case["carried"].as_array().cloned().unwrap_or_default();
    let mut map = make_store_claim_map(&carried);
    case["claims"]
        .as_array()
        .unwrap()
        .iter()
        .map(|call| {
            let pointer = match &call["pointer"] {
                Value::String(s) if s == "__ABSENT__" => None,
                v => Some(v),
            };
            let claim = map.claim_value(
                kind_of(call["kind"].as_str().unwrap()),
                call["entityId"].as_str().unwrap(),
                pointer,
            );
            serde_json::to_string(&render(&claim)).unwrap()
        })
        .collect()
}

#[test]
fn store_claims_equivalence() {
    let Ok(path) = std::env::var("QT_ORACLE_STORE_CLAIMS") else {
        eprintln!("SKIP: QT_ORACLE_STORE_CLAIMS unset");
        return;
    };
    let oracle: HashMap<String, Vec<String>> = std::fs::read_to_string(&path)
        .expect("read the oracle")
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let v: Value = serde_json::from_str(l).expect("oracle line");
            let outcomes = v["outcomes"]
                .as_array()
                .unwrap()
                .iter()
                .map(|o| serde_json::to_string(o).unwrap())
                .collect();
            (v["name"].as_str().unwrap().to_string(), outcomes)
        })
        .collect();

    let corpus = corpus();
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(
        oracle.len(),
        cases.len(),
        "the oracle and the corpus disagree on the case count — regenerate"
    );
    let mut failures = Vec::new();
    let mut divergence_cases = 0;
    for case in cases {
        let name = case["name"].as_str().unwrap();
        let want = oracle
            .get(name)
            .unwrap_or_else(|| panic!("[{name}] missing from the oracle"));
        let got = v5_outcomes(case);
        if let Some((_, v5, v4)) = DUP_STORE_ID_FIRST_ROW.iter().find(|(c, _, _)| *c == name) {
            divergence_cases += 1;
            if got != *v5 {
                failures.push(format!(
                    "[{name}] DUP_STORE_ID_FIRST_ROW (v5): got {got:?}, ruled {v5:?}"
                ));
            }
            if want != v4 {
                failures.push(format!(
                    "[{name}] DUP_STORE_ID_FIRST_ROW (v4): v4 now answers {want:?} — if it keeps \
                     the FIRST row, v4 converged: retire the table"
                ));
            }
            continue;
        }
        if &got != want {
            failures.push(format!("[{name}]\n  rust:   {got:?}\n  oracle: {want:?}"));
        }
    }
    assert_eq!(
        divergence_cases,
        DUP_STORE_ID_FIRST_ROW.len(),
        "every DUP_STORE_ID_FIRST_ROW case is in the corpus"
    );
    assert!(
        failures.is_empty(),
        "{} store-claim difference(s):\n{}",
        failures.len(),
        failures.join("\n")
    );
    eprintln!("OK: {} store-claim cases", cases.len());
}
