//! P4.D274 unit 1 tier-1 differential: v4's document-store naming leaf
//! (`lib/mount-index/store-names.ts`, `ed8b15b50`, bug 186) and the shared
//! suffixer `nextUniqueMountPointName` (`lib/mount-index/unique-mount-point-
//! name.ts`) against `quilltap_core::store_names` and
//! `quilltap_core::db::ensure_official_store::next_unique_mount_point_name` —
//! exact string equality, the rename lists compared in order AND key order
//! (`id, from, to, reason` is spread into v4's INFO line), plus each plan's
//! idempotence pass.
//!
//! The suffixer rows also cover restore 22a's free-name pick (C1 §14, P4.D265),
//! which calls the same fn: the `js_trim` convergence moved that caller too.
//!
//! `STORE_STAMP_UNREACHABLE_FORMS` (R-E) is a both-ways table: V8's legacy
//! `Date.parse` forms and its extended years cannot reach the plan (a store
//! whose `createdAt` fails `TimestampSchema` is dropped first), so v5 does not
//! port them. The table pins v4's ACTUAL output and v5's own, so a future
//! reachability (or a v4 change) trips it.
//!
//! Regenerate + run (a pure tsx oracle, the committed corpus):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   rm -f /tmp/oracle-store-names.ndjson
//!   cd ~/source/quilltap-server
//!   TZ=UTC QT_STORE_NAMES_CORPUS=$V5W/harness/oracle/fixtures/store-names-corpus.json $N/npx tsx $V5W/harness/oracle/cases/store-names.ts > /tmp/oracle-store-names.ndjson
//!   cd $V5W
//!   QT_ORACLE_STORE_NAMES=/tmp/oracle-store-names.ndjson \
//!     cargo test -p quilltap-harness --test store_names_equivalence -- --nocapture

use std::collections::HashSet;

use quilltap_core::db::ensure_official_store::next_unique_mount_point_name;
use quilltap_core::store_names::{
    character_vault_name, plan_store_names, retired_vault_name, store_name_timestamp,
    vault_base_name, StoreNameRow, VaultOwnerRow,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

/// v4's output for the V8 date forms no Zod-valid `createdAt` can take, and
/// v5's (the `iso.replace(/:/g, '')` fallback, since v5's parser is the ISO
/// subset only). `(input, v4, v5)`.
const STORE_STAMP_UNREACHABLE_FORMS: &[(&str, &str, &str)] = &[
    ("Oct 9 2026", "2026-10-09T000000Z", "Oct 9 2026"),
    (
        "Fri, 09 Oct 2026 16:45:06 GMT",
        "2026-10-09T164506Z",
        "Fri, 09 Oct 2026 164506 GMT",
    ),
    (
        "+010000-01-01T00:00:00.000Z",
        "+010000-01T010000Z",
        "+010000-01-01T000000.000Z",
    ),
    (
        "-000001-01-01T00:00:00.000Z",
        "-000001-01T010000Z",
        "-000001-01-01T000000.000Z",
    ),
];

fn s(v: &Value) -> String {
    v.as_str().expect("string").to_string()
}

fn store_rows(v: &Value) -> Vec<StoreNameRow> {
    v.as_array()
        .expect("stores")
        .iter()
        .map(|r| StoreNameRow {
            id: s(&r["id"]),
            name: s(&r["name"]),
            store_type: r["storeType"].as_str().map(str::to_string),
            created_at: s(&r["createdAt"]),
        })
        .collect()
}

fn owner_rows(v: &Value) -> Vec<VaultOwnerRow> {
    v.as_array()
        .expect("characters")
        .iter()
        .map(|r| VaultOwnerRow {
            id: s(&r["id"]),
            name: s(&r["name"]),
            character_document_mount_point_id: r["characterDocumentMountPointId"]
                .as_str()
                .map(str::to_string),
            created_at: s(&r["createdAt"]),
        })
        .collect()
}

fn renames_json(renames: &[quilltap_core::store_names::StoreRename]) -> String {
    serde_json::to_string(&Value::Array(renames.iter().map(|r| r.to_json()).collect())).unwrap()
}

#[test]
fn store_names_match_oracle() {
    let Ok(path) = std::env::var("QT_ORACLE_STORE_NAMES") else {
        eprintln!("SKIP: set QT_ORACLE_STORE_NAMES to the oracle NDJSON (see header).");
        return;
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read oracle {path}: {e}"));
    assert!(!text.trim().is_empty(), "the oracle file {path} is EMPTY");
    let oracle: Value = serde_json::from_str(text.trim()).expect("oracle line");

    // The oracle ran over THIS checkout's corpus, not a stale one.
    let corpus = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../harness/oracle/fixtures/store-names-corpus.json"),
    )
    .expect("read committed corpus");
    let sha = format!("{:x}", Sha256::digest(&corpus));
    assert_eq!(
        oracle["corpusSha256"],
        json!(sha),
        "stale oracle: corpus sha differs"
    );

    let mut failures = Vec::new();
    let mut rows = 0usize;

    for r in oracle["vaultNames"].as_array().unwrap() {
        rows += 1;
        let got = character_vault_name(r["in"].as_str().unwrap());
        if json!(got) != r["out"] {
            failures.push(format!(
                "characterVaultName({:?}): v5 {got:?} v4 {}",
                r["in"], r["out"]
            ));
        }
    }
    for r in oracle["baseNames"].as_array().unwrap() {
        rows += 1;
        let got = vault_base_name(r["in"].as_str().unwrap());
        if json!(got) != r["out"] {
            failures.push(format!(
                "vaultBaseName({:?}): v5 {got:?} v4 {}",
                r["in"], r["out"]
            ));
        }
    }
    for r in oracle["stamps"].as_array().unwrap() {
        rows += 1;
        let got = store_name_timestamp(r["in"].as_str().unwrap());
        if json!(got) != r["out"] {
            failures.push(format!(
                "storeNameTimestamp({:?}): v5 {got:?} v4 {}",
                r["in"], r["out"]
            ));
        }
    }
    for r in oracle["retired"].as_array().unwrap() {
        rows += 1;
        let got = retired_vault_name(
            r["base"].as_str().unwrap(),
            r["createdAt"].as_str().unwrap(),
        );
        if json!(got) != r["out"] {
            failures.push(format!("retiredVaultName({r}): v5 {got:?}"));
        }
    }
    for r in oracle["unique"].as_array().unwrap() {
        rows += 1;
        let taken: HashSet<String> = r["taken"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t.as_str().unwrap().to_string())
            .collect();
        let got = next_unique_mount_point_name(&taken, r["desired"].as_str().unwrap());
        if json!(got) != r["out"] {
            failures.push(format!("nextUniqueMountPointName({r}): v5 {got:?}"));
        }
    }

    // The plans: the corpus carries the inputs (the oracle echoes only the
    // label), so both are read and matched by position.
    let corpus_json: Value = serde_json::from_slice(&corpus).unwrap();
    let inputs = corpus_json["plans"].as_array().unwrap();
    let plans = oracle["plans"].as_array().unwrap();
    assert_eq!(inputs.len(), plans.len(), "plan count");
    for (input, p) in inputs.iter().zip(plans) {
        rows += 1;
        assert_eq!(input["label"], p["label"], "plan order");
        let stores = store_rows(&input["stores"]);
        let owners = owner_rows(&input["characters"]);
        let renames = plan_store_names(&stores, &owners);
        let got = renames_json(&renames);
        let want = serde_json::to_string(&p["renames"]).unwrap();
        if got != want {
            failures.push(format!("plan {}:\n  v5 {got}\n  v4 {want}", p["label"]));
            continue;
        }
        // Idempotence: plan the applied result (v4 pins `[]`).
        let after: Vec<StoreNameRow> = stores
            .iter()
            .map(|st| StoreNameRow {
                name: renames
                    .iter()
                    .find(|r| r.id == st.id)
                    .map(|r| r.to.clone())
                    .unwrap_or_else(|| st.name.clone()),
                ..st.clone()
            })
            .collect();
        let again = renames_json(&plan_store_names(&after, &owners));
        let want_again = serde_json::to_string(&p["idempotent"]).unwrap();
        if again != want_again {
            failures.push(format!(
                "plan {} (second pass): v5 {again} v4 {want_again}",
                p["label"]
            ));
        }
    }

    // STORE_STAMP_UNREACHABLE_FORMS, both ways.
    let unreachable = oracle["unreachableStamps"].as_array().unwrap();
    assert_eq!(unreachable.len(), STORE_STAMP_UNREACHABLE_FORMS.len());
    for (r, (input, v4, v5)) in unreachable.iter().zip(STORE_STAMP_UNREACHABLE_FORMS) {
        assert_eq!(r["in"], json!(input), "unreachable-form order");
        assert_eq!(
            r["out"],
            json!(v4),
            "STORE_STAMP_UNREACHABLE_FORMS: v4's output for {input:?} moved — re-measure the table"
        );
        assert_eq!(
            store_name_timestamp(input),
            *v5,
            "STORE_STAMP_UNREACHABLE_FORMS: v5's output for {input:?} moved"
        );
        assert_ne!(
            v4, v5,
            "a row that converged is no longer a divergence: retire it"
        );
    }

    assert!(
        failures.is_empty(),
        "{} mismatches:\n{}",
        failures.len(),
        failures.join("\n")
    );
    assert_eq!(rows, 117, "the corpus row count");
    eprintln!(
        "OK: store names matched oracle ({rows} rows; {} unreachable stamp forms pinned both ways).",
        STORE_STAMP_UNREACHABLE_FORMS.len()
    );
}
