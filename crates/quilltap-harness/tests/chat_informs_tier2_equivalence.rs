//! Tier-2 differential: the `chat_informs` repository (P4.D205, v4 `e7d77bb60`).
//!
//! Drives the SAME op sequence through v4's real `ChatInformsRepository` (the
//! oracle case) and the Rust port, then diffs two comparands:
//!
//!   - **`reads`** — every read op's RESULT, in order. Final state cannot see
//!     this repo's contract: three methods sort (`createdAt` ascending, tie-broken
//!     by `id.localeCompare`), one folds rows into batches, and four return
//!     counts. The corpus is built so the sort is observable — `gamma` is seeded
//!     AFTER `alpha` with the same `createdAt` and a smaller `id`, so rowid order
//!     and sorted order disagree.
//!   - **`dump`** — the final table state.
//!
//! ## The normalization (one implementation, applied to BOTH sides)
//!
//! `createBatch` mints a `batchId`, an `id` per target and the timestamps;
//! `markConsumed` mints `consumedAt`/`updatedAt`. Nothing is pinned on those ops,
//! so both sides mint independently. Rather than placeholder every id and
//! timestamp — which would throw away the seed's pinned `createdAt` values, i.e.
//! the entire ordering proof — the normalization is keyed on the COMMITTED SPEC:
//!
//!   - a UUID that appears anywhere in `chat-informs-tier2.json` is a
//!     deterministic input and is compared **exactly**;
//!   - any other UUID was minted at run time and becomes a first-seen token
//!     (`ID_0`, `ID_1`, …) in traversal order;
//!   - an ISO timestamp in the spec is compared **exactly**; any other becomes
//!     `<ts>`.
//!
//! Traversal order is identical on both sides: the `reads` in op order, then the
//! dump rows re-sorted by the deterministic natural key
//! `(contentMarkdown, participantId)` — NOT by `id`, which is minted on the
//! created rows. The test asserts that key is unique across the final rows, so
//! the sort is total.
//!
//! One consequence worth naming: v4's `markConsumed` mints `consumedAt` and then
//! lets `_update` mint its own `updatedAt`, so the two can land a millisecond
//! apart; v5 writes one `now` to both. Both are `<ts>` here, so the diff cannot
//! see it — recorded rather than hidden.
//!
//! Generate the oracle output + fixture (Node 24, from the TARGET-pinned v4
//! worktree — §R.3 PIN REQUIRED):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-chat-informs-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-chat-informs-fixture.ts
//!   QT_FIXTURE_CHAT_INFORMS=/tmp/qt-chat-informs-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/chat-informs-tier2.ts \
//!     > /tmp/oracle-chat-informs.ndjson
//! Run:
//!   QT_ORACLE_CHAT_INFORMS=/tmp/oracle-chat-informs.ndjson \
//!   QT_FIXTURE_CHAT_INFORMS=/tmp/qt-chat-informs-fixture.db \
//!     cargo test -p quilltap-harness --test chat_informs_tier2_equivalence -- --nocapture

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use quilltap_core::db::chat_informs::ChatInformRow;
use quilltap_core::db::Writer;
use serde::Deserialize;
use serde_json::{json, Map, Value};

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    ops: Vec<Op>,
}

#[derive(Deserialize)]
struct Op {
    kind: String,
    label: String,
    #[serde(default, rename = "chatId")]
    chat_id: Option<String>,
    #[serde(default, rename = "participantId")]
    participant_id: Option<String>,
    #[serde(default, rename = "batchId")]
    batch_id: Option<String>,
    #[serde(default, rename = "messageIds")]
    message_ids: Option<Vec<String>>,
    #[serde(default, rename = "contentMarkdown")]
    content_markdown: Option<String>,
    #[serde(default, rename = "participantIds")]
    participant_ids: Option<Vec<String>>,
    #[serde(default, rename = "recordMessageId")]
    record_message_id: Option<String>,
    #[serde(default)]
    ids: Option<Vec<String>>,
    #[serde(default, rename = "messageId")]
    message_id: Option<String>,
}

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/chat-informs-tier2.json")
}

/// One row rendered exactly as v4's repository returns it, so the `reads`
/// comparand lines up field for field.
///
/// ⚠ **The three nullable columns are OMITTED when NULL, not rendered `null`.**
/// Measured, not assumed: v4's SQLite backend deserializer converts every NULL
/// cell to `undefined` before Zod sees it
/// (`lib/database/backends/sqlite/backend.ts:477-479`, "Convert null to
/// undefined for Zod .optional() compatibility") — so a row object read back
/// from the database never carries a null-valued key, and `JSON.stringify` drops
/// it. This is a whole-backend rule, not a `chat_informs` quirk, and it reaches
/// further than this test: **the export NDJSON record and the backup JSON both
/// serialize raw rows, so they omit these keys too.** Where v4 wants an explicit
/// null it writes one — `findPendingBatches` coalesces with
/// `row.recordMessageId ?? null`, which is exactly why that coalesce exists.
///
/// ⚠ **The shape depends on PROVENANCE.** A row returned by `_create` never went
/// through the deserializer: it is the input object plus the minted fields, and
/// `createBatch` passes `consumedAt: null`, `consumedByMessageId: null` and
/// `recordMessageId: params.recordMessageId ?? null` explicitly — so every
/// CREATED row carries all three keys, nulls included. Measured at the pin.
/// [`created_row_json`] renders that shape; this one renders the read shape.
fn row_json(r: &ChatInformRow) -> Value {
    let mut m = Map::new();
    m.insert("id".into(), Value::from(r.id.clone()));
    m.insert("chatId".into(), Value::from(r.chat_id.clone()));
    m.insert("batchId".into(), Value::from(r.batch_id.clone()));
    m.insert(
        "participantId".into(),
        Value::from(r.participant_id.clone()),
    );
    m.insert(
        "contentMarkdown".into(),
        Value::from(r.content_markdown.clone()),
    );
    if let Some(v) = &r.record_message_id {
        m.insert("recordMessageId".into(), Value::from(v.clone()));
    }
    m.insert("createdAt".into(), Value::from(r.created_at.clone()));
    m.insert("updatedAt".into(), Value::from(r.updated_at.clone()));
    if let Some(v) = &r.consumed_at {
        m.insert("consumedAt".into(), Value::from(v.clone()));
    }
    if let Some(v) = &r.consumed_by_message_id {
        m.insert("consumedByMessageId".into(), Value::from(v.clone()));
    }
    Value::Object(m)
}

/// The CREATE shape: every column present, the three nullable ones rendered
/// `null` rather than omitted (see [`row_json`] on why the two differ).
fn created_row_json(r: &ChatInformRow) -> Value {
    let mut m = Map::new();
    m.insert("id".into(), Value::from(r.id.clone()));
    m.insert("chatId".into(), Value::from(r.chat_id.clone()));
    m.insert("batchId".into(), Value::from(r.batch_id.clone()));
    m.insert(
        "participantId".into(),
        Value::from(r.participant_id.clone()),
    );
    m.insert(
        "contentMarkdown".into(),
        Value::from(r.content_markdown.clone()),
    );
    m.insert(
        "recordMessageId".into(),
        r.record_message_id.clone().map_or(Value::Null, Value::from),
    );
    m.insert("createdAt".into(), Value::from(r.created_at.clone()));
    m.insert("updatedAt".into(), Value::from(r.updated_at.clone()));
    m.insert(
        "consumedAt".into(),
        r.consumed_at.clone().map_or(Value::Null, Value::from),
    );
    m.insert(
        "consumedByMessageId".into(),
        r.consumed_by_message_id
            .clone()
            .map_or(Value::Null, Value::from),
    );
    Value::Object(m)
}

fn looks_like_uuid(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 36
        && b.iter().enumerate().all(|(i, c)| match i {
            8 | 13 | 18 | 23 => *c == b'-',
            _ => c.is_ascii_hexdigit(),
        })
}

fn looks_like_iso_ts(s: &str) -> bool {
    // `2026-01-02T00:00:00.000Z` — the only timestamp shape either side writes.
    s.len() >= 20 && s.as_bytes()[4] == b'-' && s.as_bytes()[10] == b'T' && s.ends_with('Z')
}

/// Collect every UUID-shaped and timestamp-shaped string in the committed spec.
/// Those are the deterministic inputs; everything else in a dump was minted.
fn known_values(spec_json: &Value) -> (HashSet<String>, HashSet<String>) {
    let mut ids = HashSet::new();
    let mut timestamps = HashSet::new();
    fn walk(v: &Value, ids: &mut HashSet<String>, ts: &mut HashSet<String>) {
        match v {
            Value::String(s) => {
                if looks_like_uuid(s) {
                    ids.insert(s.clone());
                } else if looks_like_iso_ts(s) {
                    ts.insert(s.clone());
                }
            }
            Value::Array(a) => a.iter().for_each(|x| walk(x, ids, ts)),
            Value::Object(o) => o.values().for_each(|x| walk(x, ids, ts)),
            _ => {}
        }
    }
    walk(spec_json, &mut ids, &mut timestamps);
    (ids, timestamps)
}

struct Normalizer {
    known_ids: HashSet<String>,
    known_ts: HashSet<String>,
    minted: HashMap<String, String>,
}

impl Normalizer {
    /// Rewrite minted values in place, in traversal order (objects walk in their
    /// serde_json insertion order, which both sides build identically).
    fn walk(&mut self, v: &mut Value) {
        match v {
            Value::String(s) => {
                if looks_like_uuid(s) && !self.known_ids.contains(s.as_str()) {
                    let next = format!("ID_{}", self.minted.len());
                    let token = self.minted.entry(s.clone()).or_insert(next).clone();
                    *v = Value::String(token);
                } else if looks_like_iso_ts(s) && !self.known_ts.contains(s.as_str()) {
                    *v = Value::String("<ts>".to_string());
                }
            }
            Value::Array(a) => a.iter_mut().for_each(|x| self.walk(x)),
            Value::Object(o) => o.values_mut().for_each(|x| self.walk(x)),
            _ => {}
        }
    }
}

/// The deterministic natural key the final dump is sorted by on both sides.
fn natural_key(row: &Value) -> (String, String) {
    let g = |k: &str| {
        row.get(k)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    (g("contentMarkdown"), g("participantId"))
}

fn sort_dump_rows(dump: &mut Value, label: &str) {
    let rows = dump
        .get_mut("rows")
        .and_then(Value::as_array_mut)
        .unwrap_or_else(|| panic!("{label}: dump has no rows array"));
    rows.sort_by_key(natural_key);
    let mut seen = HashSet::new();
    for r in rows.iter() {
        assert!(
            seen.insert(natural_key(r)),
            "{label}: the natural key (contentMarkdown, participantId) is not unique across the \
             final rows — the corpus must keep it total or the sort is not deterministic"
        );
    }
}

#[test]
fn chat_informs_tier2_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_CHAT_INFORMS") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_CHAT_INFORMS to the oracle NDJSON (see header).");
            return;
        }
    };
    let fixture = match std::env::var("QT_FIXTURE_CHAT_INFORMS") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_FIXTURE_CHAT_INFORMS to the seed fixture .db (see header).");
            return;
        }
    };

    let spec_text =
        std::fs::read_to_string(spec_path()).unwrap_or_else(|e| panic!("cannot read spec: {e}"));
    let spec_json: Value = serde_json::from_str(&spec_text).expect("parse spec json");
    let spec: Spec = serde_json::from_str(&spec_text).expect("parse spec");
    let (known_ids, known_ts) = known_values(&spec_json);

    let oracle_text =
        std::fs::read_to_string(&oracle_path).unwrap_or_else(|e| panic!("cannot read oracle: {e}"));
    let mut oracle: Value =
        serde_json::from_str(oracle_text.trim()).expect("parse oracle NDJSON row");

    // Fresh copy so the shared seed fixture stays pristine.
    let work = std::env::temp_dir().join(format!("qt-chat-informs-rust-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&work);
    std::fs::copy(&fixture, &work).unwrap_or_else(|e| panic!("copy fixture: {e}"));

    let mut reads: Vec<Value> = Vec::new();
    let writer = Writer::open_writable(&work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open fixture copy: {e}"));
    {
        let repo = writer.chat_informs();
        for op in &spec.ops {
            let result: Value = match op.kind.as_str() {
                "findPendingForParticipant" => Value::Array(
                    repo.find_pending_for_participant(
                        op.chat_id.as_deref().unwrap(),
                        op.participant_id.as_deref().unwrap(),
                    )
                    .expect("find_pending_for_participant")
                    .iter()
                    .map(row_json)
                    .collect(),
                ),
                "findConsumedByMessages" => Value::Array(
                    repo.find_consumed_by_messages(
                        op.chat_id.as_deref().unwrap(),
                        op.participant_id.as_deref().unwrap(),
                        op.message_ids.as_deref().unwrap(),
                    )
                    .expect("find_consumed_by_messages")
                    .iter()
                    .map(row_json)
                    .collect(),
                ),
                "findPendingBatches" => Value::Array(
                    repo.find_pending_batches(op.chat_id.as_deref().unwrap())
                        .expect("find_pending_batches")
                        .iter()
                        .map(|b| {
                            json!({
                                "batchId": b.batch_id,
                                "contentMarkdown": b.content_markdown,
                                "createdAt": b.created_at,
                                "recordMessageId": b.record_message_id,
                                "pendingParticipantIds": b.pending_participant_ids,
                            })
                        })
                        .collect(),
                ),
                "findByChatId" => Value::Array(
                    repo.find_by_chat_id(op.chat_id.as_deref().unwrap())
                        .expect("find_by_chat_id")
                        .iter()
                        .map(row_json)
                        .collect(),
                ),
                "findByBatchId" => Value::Array(
                    repo.find_by_batch_id(op.batch_id.as_deref().unwrap())
                        .expect("find_by_batch_id")
                        .iter()
                        .map(row_json)
                        .collect(),
                ),
                "createBatch" => Value::Array(
                    repo.create_batch(
                        op.chat_id.as_deref().unwrap(),
                        op.content_markdown.as_deref().unwrap(),
                        op.participant_ids.as_deref().unwrap(),
                        op.record_message_id.as_deref(),
                    )
                    .expect("create_batch")
                    .iter()
                    .map(created_row_json)
                    .collect(),
                ),
                "markConsumed" => Value::from(
                    repo.mark_consumed(
                        op.ids.as_deref().unwrap(),
                        op.message_id.as_deref().unwrap(),
                    )
                    .expect("mark_consumed"),
                ),
                "deletePendingByBatch" => Value::from(
                    repo.delete_pending_by_batch(op.batch_id.as_deref().unwrap())
                        .expect("delete_pending_by_batch"),
                ),
                "deletePendingForParticipant" => Value::from(
                    repo.delete_pending_for_participant(
                        op.chat_id.as_deref().unwrap(),
                        op.participant_id.as_deref().unwrap(),
                    )
                    .expect("delete_pending_for_participant"),
                ),
                "deleteByChatId" => Value::from(
                    repo.delete_by_chat_id(op.chat_id.as_deref().unwrap())
                        .expect("delete_by_chat_id"),
                ),
                other => panic!("unknown op kind: {other}"),
            };
            reads.push(json!({ "kind": op.kind, "label": op.label, "result": result }));
        }
    }

    let mut dump = writer
        .dump_table_json("chat_informs", "id")
        .expect("dump chat_informs");
    let _ = std::fs::remove_file(&work);

    let mut got = json!({ "reads": Value::Array(reads), "dump": dump.take() });
    let mut want = json!({
        "reads": oracle["reads"].take(),
        "dump": oracle["dump"].take(),
    });

    // Same total order on both sides BEFORE tokenizing, so first-seen tokens line up.
    sort_dump_rows(&mut got["dump"], "rust");
    sort_dump_rows(&mut want["dump"], "oracle");

    let mut n_got = Normalizer {
        known_ids: known_ids.clone(),
        known_ts: known_ts.clone(),
        minted: HashMap::new(),
    };
    n_got.walk(&mut got);
    let mut n_want = Normalizer {
        known_ids,
        known_ts,
        minted: HashMap::new(),
    };
    n_want.walk(&mut want);

    // Per-op read diff first: a mismatch names the method and v4's own label.
    let got_reads = got["reads"].as_array().expect("rust reads");
    let want_reads = want["reads"].as_array().expect("oracle reads");
    assert_eq!(
        got_reads.len(),
        want_reads.len(),
        "read-op count diverged (the corpus changed under one side)"
    );
    for (g, w) in got_reads.iter().zip(want_reads.iter()) {
        assert_eq!(
            g["result"],
            w["result"],
            "read result diverged for `{}` — {}\n  rust:   {}\n  oracle: {}",
            w["kind"].as_str().unwrap_or("?"),
            w["label"].as_str().unwrap_or(""),
            g["result"],
            w["result"],
        );
    }

    assert_eq!(got["dump"]["table"], want["dump"]["table"], "table name");
    assert_eq!(
        got["dump"]["columns"], want["dump"]["columns"],
        "column set / order"
    );
    assert_eq!(
        got["dump"]["rows"], want["dump"]["rows"],
        "final row state diverged\n  rust:   {}\n  oracle: {}",
        got["dump"]["rows"], want["dump"]["rows"]
    );

    // Guard against a vacuous normalization: the created rows' minted ids MUST
    // have become tokens, or the comparison proved nothing about them.
    let rows = got["dump"]["rows"].as_array().expect("rows array");
    let tokened = rows
        .iter()
        .filter(|r| {
            r.get("id")
                .and_then(Value::as_str)
                .is_some_and(|s| s.starts_with("ID_"))
        })
        .count();
    // Three rows are created, but `deletePendingForParticipant` later removes the
    // one targeting p2, so TWO minted-id rows survive into the final state.
    assert_eq!(
        tokened, 2,
        "expected the two surviving createBatch rows to carry tokenized minted ids; the \
         normalization tokenized {tokened}"
    );
    // And a guard the other way: the seed's pinned timestamps must NOT have been
    // placeholdered, or the ordering proof compared `<ts>` against `<ts>`.
    let kept_ts = rows
        .iter()
        .filter(|r| {
            r.get("createdAt")
                .and_then(Value::as_str)
                .is_some_and(|s| s.starts_with("2026-01-"))
        })
        .count();
    assert!(
        kept_ts >= 1,
        "every createdAt was placeholdered — the seed's pinned timestamps must survive \
         normalization or the sort order is untested"
    );

    let map: &Map<String, Value> = rows[0].as_object().expect("row object");
    assert!(map.contains_key("consumedByMessageId"), "column set shrank");

    eprintln!(
        "OK: chat_informs tier-2 matched oracle ({} read ops, {} final rows).",
        got_reads.len(),
        rows.len()
    );
}
