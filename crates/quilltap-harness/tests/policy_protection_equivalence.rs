//! Tier-1 differential test — P4.D288 (C1-7): `quilltap_core::policy_protection`
//! against v4's REAL `lib/memory/policy-protection.ts` (bug 201) at the
//! `83334b9a5` pin: `policy_protection_of` over every `(source, tier)` pair the
//! oracle enumerates (absent and `null` both reach v5 as `None`), and
//! `partition_by_policy_protection` over mixed corpora — both halves in input
//! order, `sparedBy` byte-equal in v4's key order.
//!
//! Regenerate (Node 24, from the round's pin):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   $N/npx tsx $V5W/harness/oracle/cases/policy-protection.ts > /tmp/oracle-policy-protection.ndjson
//! Run:
//!   QT_ORACLE_POLICY_PROTECTION=/tmp/oracle-policy-protection.ndjson \
//!     cargo test -p quilltap-harness --test policy_protection_equivalence

use quilltap_core::policy_protection::{partition_by_policy_protection, policy_protection_of};
use serde_json::{json, Value};

fn field<'a>(row: &'a Value, key: &str) -> Option<&'a str> {
    row.get(key).and_then(Value::as_str)
}

#[test]
fn policy_protection_matches_v4() {
    let Ok(path) = std::env::var("QT_ORACLE_POLICY_PROTECTION") else {
        eprintln!("SKIP: set QT_ORACLE_POLICY_PROTECTION to the oracle NDJSON (see header).");
        return;
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read oracle: {e}"));
    let (mut ofs, mut partitions) = (0usize, 0usize);
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let case: Value = serde_json::from_str(line).expect("oracle line");
        match case["case"].as_str().unwrap() {
            "of" => {
                let row = &case["row"];
                let got = policy_protection_of(field(row, "source"), field(row, "tier"))
                    .map(|p| json!(p.as_str()))
                    .unwrap_or(Value::Null);
                assert_eq!(got, case["result"], "policyProtectionOf({row})");
                ofs += 1;
            }
            "partition" => {
                let rows: Vec<Value> = case["rows"].as_array().unwrap().clone();
                let (actionable, spared, spared_by) = partition_by_policy_protection(&rows, |r| {
                    (field(r, "source"), field(r, "tier"))
                });
                let ids = |rs: Vec<Value>| -> Value {
                    Value::Array(rs.into_iter().map(|r| r["id"].clone()).collect())
                };
                assert_eq!(
                    ids(actionable),
                    case["actionable"],
                    "actionable for {}",
                    case["rows"]
                );
                assert_eq!(ids(spared), case["spared"], "spared for {}", case["rows"]);
                assert_eq!(
                    spared_by.to_json().to_string(),
                    case["sparedBy"].to_string(),
                    "sparedBy (bytes, key order) for {}",
                    case["rows"]
                );
                partitions += 1;
            }
            other => panic!("unknown case {other}"),
        }
    }
    assert_eq!((ofs, partitions), (42, 3), "the oracle's case counts");
    eprintln!("OK: policy protection matched v4 ({ofs} rows, {partitions} partitions).");
}
