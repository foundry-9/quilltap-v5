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
//! P4.139 grows three channels the dump cannot see. (1) `readIsActive` — six
//! seeded rows whose `isActive` cell is NULL / 2 / 'x' / '' / 1.5 / x'00', read
//! by literal id through `findApiKeyById` and recorded `{id, isActive}`: v4's
//! hydrate (`backend.ts:412-450`) answers `true, false, true, false, false,
//! true`; v5's old `i64 != 0` marshal refused five and read `2` as `true`.
//! (2) A user-A row whose `key_value` is a BLOB: v4 DROPS it from
//! `getApiKeysByUserId` and lists the rest; v5 had failed the whole list. (3)
//! The drop's WARN, `API key validation failed {keyId, userId, error}`, per
//! `getByUser` op — byte-exact for the empty-provider row (v5 renders Zod's
//! own message through `api::zod_issues`), with `error` normalised for the
//! BLOB row (v4 a ZodError over the Float32 decode, v5 rusqlite's sentence —
//! pinned exactly below as v5's bytes). Every mismatch is collected before
//! the assert, so a red names its whole count.
//!
//! Generate the oracle output + fixture (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-api-keys-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-api-keys-fixture.ts
//!   QT_FIXTURE_API_KEYS=/tmp/qt-api-keys-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/api-keys.ts \
//!     > /tmp/oracle-api-keys.ndjson
//!   (the oracle must carry six `readIsActive` rows — `grep -c readIsActive`)
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
    /// P4.139: read a SEEDED row by literal id and record its `isActive`.
    #[serde(rename = "readIsActive")]
    ReadIsActive { id: String },
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

/// The seeded user-A row whose `key_value` is a BLOB (`api-keys-tier2.json`).
const CORRUPT_KEY_ID: &str = "0badbad0-0000-4000-8000-000000000b10";
/// v5's `error` bytes on that row's drop WARN — rusqlite's bare sentence for
/// the marshal's type check on column 4.
const V5_BLOB_KEY_VALUE_ERROR: &str = "Invalid column type Blob at index: 4, name: key_value";

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
    // NDJSON: the `readIsActive` / `warn` rows, then the dump (P4.139).
    let mut oracle_dump: Option<Value> = None;
    let mut oracle_reads: Vec<Value> = Vec::new();
    let mut oracle_warns: Vec<Value> = Vec::new();
    for line in oracle_text.lines().filter(|l| !l.trim().is_empty()) {
        let v: Value = serde_json::from_str(line).expect("parse oracle line");
        match v["case"].as_str() {
            Some("readIsActive") => oracle_reads.push(v),
            Some("warn") => oracle_warns.push(v),
            Some("api-keys-tier2") => oracle_dump = Some(v),
            other => panic!("unexpected oracle row kind {other:?}"),
        }
    }
    let mut oracle = oracle_dump.expect("the oracle carries the api-keys-tier2 dump");
    assert_eq!(
        oracle_reads.len(),
        6,
        "a stale oracle: regenerate it (six readIsActive rows expected)"
    );
    let mut reds: Vec<String> = Vec::new();
    let mut warns: Vec<String> = Vec::new();

    let scratch_dir = tempfile::Builder::new()
        .prefix("qt-api-keys-rust-")
        .tempdir()
        .expect("tempdir");
    let work = scratch_dir.path().join("qt-api-keys-rust.db");
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
                let (keys, lines) = quilltap_core::test_support::captured_with(|| {
                    api_keys::get_api_keys_by_user_id(writer.connection(), user_id)
                });
                warns.extend(lines.into_iter().map(|l| format!("op {i}: {l}")));
                let keys = match keys {
                    Ok(keys) => keys,
                    Err(e) => {
                        reds.push(format!("getByUser op {i}: the whole list failed: {e}"));
                        continue;
                    }
                };
                if let Some(expected) = expect_labels {
                    let mut labels: Vec<String> = keys.iter().map(|k| k.label.clone()).collect();
                    labels.sort();
                    let mut exp = expected.clone();
                    exp.sort();
                    if labels != exp {
                        reds.push(format!(
                            "getByUser op {i}: labels {labels:?}, expected {exp:?}"
                        ));
                    }
                }
            }
            Op::ReadIsActive { id } => {
                let got = match api_keys::find_by_id(writer.connection(), id) {
                    Ok(Some(k)) => Value::Bool(k.is_active),
                    Ok(None) => Value::Null,
                    Err(e) => Value::String(format!("<read error: {e}>")),
                };
                let want = oracle_reads
                    .iter()
                    .find(|r| r["id"] == Value::String(id.clone()))
                    .unwrap_or_else(|| panic!("the oracle has no readIsActive row for {id}"));
                if got != want["isActive"] {
                    reds.push(format!(
                        "readIsActive {id}: v5 {got}, v4 {}",
                        want["isActive"]
                    ));
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

    // The drop's WARN lines, op by op, in v4's order. The BLOB row's `error`
    // is v5's own sentence (pinned exactly); v4's is a ZodError over the
    // Float32 decode — the recorded byte divergence of a marshal-failure drop.
    let want_warns: Vec<String> = oracle_warns
        .iter()
        .map(|w| {
            let c = &w["context"];
            let key_id = c["keyId"].as_str().expect("keyId");
            let error = if key_id == CORRUPT_KEY_ID {
                V5_BLOB_KEY_VALUE_ERROR.to_string()
            } else {
                c["error"].as_str().expect("error").to_string()
            };
            format!(
                "op {}: WARN quilltap::db {} keyId={key_id} userId={} error={error}",
                w["op"],
                w["message"].as_str().expect("message"),
                c["userId"].as_str().expect("userId"),
            )
        })
        .collect();
    if warns != want_warns {
        reds.push(format!(
            "the drop WARN lines diverged\n  v5: {warns:#?}\n  v4: {want_warns:#?}"
        ));
    }
    assert!(
        reds.is_empty(),
        "{} red(s):\n  {}",
        reds.len(),
        reds.join("\n  ")
    );

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
