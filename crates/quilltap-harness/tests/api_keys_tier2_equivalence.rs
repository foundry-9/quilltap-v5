//! Tier-2 differential test: the `api_keys` repo (W4.7d — the LAST unported repo).
//!
//! Structural DB diff, minted-values remap form. Both sides start from the SAME
//! seed fixture (built by build-api-keys-fixture.ts — an empty `api_keys` table
//! plus ONE malformed raw row whose empty `provider` fails `ProviderEnum`), run
//! the SAME op sequence from the committed spec (create ×3 / recordUsage / update
//! / delete + the reads), and dump the `api_keys` table.
//!
//! `createApiKey` mints id + timestamps (v4 takes no CreateOptions), so nothing
//! is pinned on create. The dump is reconciled by:
//!   - **id remap** — rows are dumped in natural-key (`label`) order (labels are
//!     inputs, not generated); walking that order, each `id` gets a first-seen
//!     `ID_N` token.
//!   - **timestamp placeholder** — `createdAt` / `updatedAt` / `lastUsed` collapse
//!     to `<ts>` when non-null (both sides).
//!
//! Every other column (`userId`, `label`, `provider`, `key_value`, `isActive`) is
//! diffed EXACT. The read outcomes are pinned in the spec and asserted on BOTH
//! ports (a mismatch aborts).
//!
//! Banked: the boolean `isActive` INTEGER 0/1 (beta stays `false`), the nullable
//! `lastUsed` (null on a never-used key, `<ts>` after `recordApiKeyUsage` — the
//! CONTRAST proves recordUsage set it AND — riding `updateApiKey` — bumped
//! `updatedAt`), the free-form `provider` string, and the safeParse DROP of the
//! malformed row from `getApiKeysByUserId`.
//!
//! Generate the oracle output + fixture (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-api-keys-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-api-keys-fixture.ts
//!   QT_FIXTURE_API_KEYS=/tmp/qt-api-keys-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/api-keys.ts \
//!     > /tmp/oracle-api-keys.ndjson
//! Run:
//!   QT_ORACLE_API_KEYS=/tmp/oracle-api-keys.ndjson \
//!   QT_FIXTURE_API_KEYS=/tmp/qt-api-keys-fixture.db \
//!     cargo test -p quilltap-harness --test api_keys_tier2_equivalence

use std::path::{Path, PathBuf};

use quilltap_core::db::api_keys::{self, AkCreate, AkUpdate};
use quilltap_core::db::Writer;
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    ops: Vec<Op>,
}

#[derive(Deserialize)]
#[serde(tag = "kind")]
enum Op {
    #[serde(rename = "create")]
    Create { data: CreateData },
    #[serde(rename = "recordUsage")]
    RecordUsage {
        #[serde(rename = "idFromOp")]
        id_from_op: usize,
    },
    #[serde(rename = "update")]
    Update {
        #[serde(rename = "idFromOp")]
        id_from_op: usize,
        data: UpdateData,
    },
    #[serde(rename = "delete")]
    Delete {
        #[serde(rename = "idFromOp")]
        id_from_op: usize,
        #[serde(default, rename = "expectDeleted")]
        expect_deleted: Option<bool>,
    },
    #[serde(rename = "getByUser")]
    GetByUser {
        #[serde(rename = "userId")]
        user_id: String,
        #[serde(default, rename = "expectLabels")]
        expect_labels: Option<Vec<String>>,
    },
    #[serde(rename = "findById")]
    FindById {
        #[serde(rename = "idFromOp")]
        id_from_op: usize,
        #[serde(default, rename = "expectFound")]
        expect_found: Option<bool>,
    },
    #[serde(rename = "findByIdAndUser")]
    FindByIdAndUser {
        #[serde(rename = "idFromOp")]
        id_from_op: usize,
        #[serde(rename = "userId")]
        user_id: String,
        #[serde(default, rename = "expectFound")]
        expect_found: Option<bool>,
    },
}

#[derive(Deserialize)]
struct CreateData {
    #[serde(rename = "userId")]
    user_id: String,
    label: String,
    provider: String,
    #[serde(rename = "keyValue")]
    key_value: String,
    #[serde(default, rename = "isActive")]
    is_active: Option<bool>,
}

#[derive(Deserialize)]
struct UpdateData {
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    provider: Option<String>,
    #[serde(default, rename = "keyValue")]
    key_value: Option<String>,
    #[serde(default, rename = "isActive")]
    is_active: Option<bool>,
}

const ID_COLUMNS: &[&str] = &["id"];
const TS_COLUMNS: &[&str] = &["createdAt", "updatedAt", "lastUsed"];

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../harness/oracle/fixtures/api-keys-tier2.json")
}

/// First-seen id remap (rows already in `label` order) + minted-timestamp
/// placeholder, applied to both dumps.
fn normalize(dump: &mut Value, label: &str) {
    let rows = dump
        .get_mut("rows")
        .and_then(Value::as_array_mut)
        .unwrap_or_else(|| panic!("{label}: dump has no rows array"));
    let mut id_map: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for row in rows.iter_mut() {
        let obj = row
            .as_object_mut()
            .unwrap_or_else(|| panic!("{label}: row is not an object"));
        for col in ID_COLUMNS {
            if let Some(Value::String(raw)) = obj.get(*col) {
                let next = format!("ID_{}", id_map.len());
                let token = id_map.entry(raw.clone()).or_insert(next).clone();
                obj.insert((*col).to_string(), Value::String(token));
            }
        }
        for col in TS_COLUMNS {
            if obj.get(*col).map(|v| !v.is_null()).unwrap_or(false) {
                obj.insert((*col).to_string(), Value::String("<ts>".to_string()));
            }
        }
    }
}

#[test]
fn api_keys_tier2_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_API_KEYS") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_API_KEYS to the oracle NDJSON (see header).");
            return;
        }
    };
    let fixture = match std::env::var("QT_FIXTURE_API_KEYS") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_FIXTURE_API_KEYS to the seed fixture .db (see header).");
            return;
        }
    };

    let spec_text = std::fs::read_to_string(spec_path())
        .unwrap_or_else(|e| panic!("cannot read fixture spec: {e}"));
    let spec: Spec = serde_json::from_str(&spec_text).expect("parse fixture spec");

    let oracle_text =
        std::fs::read_to_string(&oracle_path).unwrap_or_else(|e| panic!("cannot read oracle: {e}"));
    let mut oracle: Value = serde_json::from_str(oracle_text.trim()).expect("parse oracle dump");

    let work = std::env::temp_dir().join(format!("qt-api-keys-rust-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&work);
    std::fs::copy(&fixture, &work).unwrap_or_else(|e| panic!("copy fixture: {e}"));

    let writer = Writer::open_writable(&work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open fixture copy: {e}"));

    // Minted key id per CREATE op index.
    let mut minted_by_op: Vec<Option<String>> = vec![None; spec.ops.len()];
    let resolve = |minted: &[Option<String>], n: usize| -> String {
        minted
            .get(n)
            .and_then(|o| o.clone())
            .unwrap_or_else(|| panic!("op references idFromOp {n}, not a create result"))
    };

    for (i, op) in spec.ops.iter().enumerate() {
        let repo = writer.api_keys();
        match op {
            Op::Create { data } => {
                let created = repo
                    .create(&AkCreate {
                        user_id: data.user_id.clone(),
                        label: data.label.clone(),
                        provider: data.provider.clone(),
                        key_value: data.key_value.clone(),
                        is_active: data.is_active,
                        last_used: None,
                    })
                    .expect("api_keys.create");
                minted_by_op[i] = Some(created.id);
            }
            Op::RecordUsage { id_from_op } => {
                let id = resolve(&minted_by_op, *id_from_op);
                let res = repo.record_usage(&id).expect("api_keys.record_usage");
                assert!(res.is_some(), "recordUsage target missing (op {i})");
            }
            Op::Update { id_from_op, data } => {
                let id = resolve(&minted_by_op, *id_from_op);
                let res = repo
                    .update(
                        &id,
                        &AkUpdate {
                            label: data.label.clone(),
                            provider: data.provider.clone(),
                            key_value: data.key_value.clone(),
                            is_active: data.is_active,
                            last_used: None,
                        },
                    )
                    .expect("api_keys.update");
                assert!(res.is_some(), "update target missing (op {i})");
            }
            Op::Delete {
                id_from_op,
                expect_deleted,
            } => {
                let id = resolve(&minted_by_op, *id_from_op);
                let deleted = repo.delete(&id).expect("api_keys.delete");
                if let Some(expected) = expect_deleted {
                    assert_eq!(deleted, *expected, "delete flag diverged (op {i})");
                }
            }
            Op::GetByUser {
                user_id,
                expect_labels,
            } => {
                let keys = api_keys::get_api_keys_by_user_id(writer.connection(), user_id)
                    .expect("get_api_keys_by_user_id");
                if let Some(expected) = expect_labels {
                    let mut labels: Vec<String> = keys.iter().map(|k| k.label.clone()).collect();
                    labels.sort();
                    let mut exp = expected.clone();
                    exp.sort();
                    assert_eq!(labels, exp, "getByUser labels diverged (op {i})");
                }
            }
            Op::FindById {
                id_from_op,
                expect_found,
            } => {
                let id = resolve(&minted_by_op, *id_from_op);
                let key = api_keys::find_by_id(writer.connection(), &id).expect("find_by_id");
                if let Some(expected) = expect_found {
                    assert_eq!(key.is_some(), *expected, "findById found diverged (op {i})");
                }
            }
            Op::FindByIdAndUser {
                id_from_op,
                user_id,
                expect_found,
            } => {
                let id = resolve(&minted_by_op, *id_from_op);
                let key = api_keys::find_by_id_and_user_id(writer.connection(), &id, user_id)
                    .expect("find_by_id_and_user_id");
                if let Some(expected) = expect_found {
                    assert_eq!(
                        key.is_some(),
                        *expected,
                        "findByIdAndUser found diverged (op {i})"
                    );
                }
            }
        }
    }

    let mut got = writer
        .dump_table_json("api_keys", "label")
        .expect("dump api_keys");

    let _ = std::fs::remove_file(&work);

    normalize(&mut got, "rust");
    normalize(&mut oracle, "oracle");

    assert_eq!(got["table"], oracle["table"], "table name");
    assert_eq!(got["columns"], oracle["columns"], "column set / order");
    assert_eq!(
        got["rows"], oracle["rows"],
        "row state diverged\n  rust:   {}\n  oracle: {}",
        got["rows"], oracle["rows"]
    );

    // Sanity: the recordUsage contrast is banked (alpha has lastUsed <ts>, the
    // never-used beta-renamed has lastUsed null).
    let rows = got["rows"].as_array().expect("rows array");
    let alpha = rows
        .iter()
        .find(|r| r["label"] == Value::String("alpha".into()))
        .expect("alpha row");
    assert_eq!(
        alpha["lastUsed"],
        Value::String("<ts>".into()),
        "recordApiKeyUsage should have set alpha.lastUsed"
    );
    let beta = rows
        .iter()
        .find(|r| r["label"] == Value::String("beta-renamed".into()))
        .expect("beta-renamed row");
    assert_eq!(beta["lastUsed"], Value::Null, "beta was never used");
    assert_eq!(
        beta["isActive"],
        Value::from(0),
        "beta stays isActive=false through the label/key update"
    );

    eprintln!("OK: api_keys tier-2 matched oracle ({} rows).", rows.len());
}
