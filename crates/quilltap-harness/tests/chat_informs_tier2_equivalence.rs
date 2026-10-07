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
//! **P4.D249 (v4 `52d6e7ecd`)** grew the corpus with a standing-inform chat
//! (seed rows carry `permanent`; `createBatch` ops may name it): delivery order
//! vs posting order, the batch flag, a standing batch cancelled whole, a
//! consumed one-shot surviving cancel, seat removal taking delivered standing
//! rows. Red-first measured from the oracle at both pins: 16 of 27 read
//! results move between `e5c6bd0c0` and `52d6e7ecd`.
//!
//! **P4.149** grew it twice. (1) The P4.151 survey's B1: two standing rows
//! for p1 inserted LAST — F posted before C, G tied with D on `createdAt` with a
//! smaller id — so the posting-order leg AMONG standing rows is observable here
//! (mutation-proven: `by_delivery_order` answering `Equal` for two standing rows
//! reddens op 18). (2) The repository-fallback arm: a BEFORE DELETE trigger
//! planted mid-corpus (reads still succeed, every row delete fails), then a
//! cancel and a seat removal. v4 answers `0` with its base rethrow line `Error
//! deleting entity` and the 4-argument wrap's own line; ops marked
//! `captureLogs` compare their ERROR/WARN lines as whole strings (field order
//! and the bare `error` included), with v4's backend `SQLite deleteOne error`
//! dropped (unported by standing convention). Red-first on unported `main`: the
//! cancel PROPAGATED `planted delete failure` where v4 answers 0.
//!
//! **P4.156** (the repository-fallback class, round 4) grew it a third time:
//! every READ the routes call is reached through a planted column RENAME
//! (`chatId`, `batchId`, then `id` for `_update`'s own `findById`), each once
//! outside the strict scope (v4's INNER `findByFilter` line, `[]`) and once
//! inside it (R-H — the inner line + the method's OUTER line, both
//! `strictFailures: true`, and the op THROWS, recorded as `{threw}`);
//! `markConsumed` under a BEFORE UPDATE plant (the base `Error updating
//! entity` + its own fallback line → 0), `createBatch` under a BEFORE INSERT
//! plant (the base `Error creating entity`, then it throws — contract C2's
//! strict rendering committed, as the order's R-H asks), and the Inform
//! cancel's record-delete leg (`chats.deleteMessagesByIds`, v4's standalone
//! fallback — dogfood #145). Red-first on unported `main` at `94fbb1ae3`: 16 of
//! the 16 new captured ops logged differently and 8 answered differently; three
//! mutations (the per-row `findById`, the strict sibling's propagation, the
//! create's base line) each red. v4's SQL double-quotes identifiers, so a
//! renamed column's message carries the quotes and a hint v5's does not —
//! [`normalize_v4_sqlite`] maps one onto the other.
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
    /// P4.D249: `createBatch`'s standing flag; absent = v4's default leg.
    #[serde(default)]
    permanent: Option<bool>,
    #[serde(default)]
    ids: Option<Vec<String>>,
    #[serde(default, rename = "messageId")]
    message_id: Option<String>,
    /// P4.149: record the ERROR/WARN lines this op logs (see [`v4_line`]).
    #[serde(default, rename = "captureLogs")]
    capture_logs: bool,
    /// P4.156: the raw plant an `execSql` op runs (a column RENAME, a trigger).
    #[serde(default)]
    sql: Option<String>,
    /// P4.156 (R-H): run the op inside the strict repository scope.
    #[serde(default)]
    strict: bool,
    /// P4.163: ALSO record this op's DEBUG lines (`Informs marked consumed`
    /// and the empty-list silence), compared the same way.
    #[serde(default, rename = "captureDebug")]
    capture_debug: bool,
}

/// P4.149 — v4's backend-only lines (`backends/sqlite/backend.ts`), which v5
/// does not port by standing convention (the search families'
/// `UNPORTED_BACKEND_LINES`): dropped from the oracle's captured lines.
const UNPORTED_BACKEND_LINES: &[&str] = &[
    "SQLite deleteOne error",
    // P4.156: the read / update / insert plants' backend lines.
    "SQLite find error",
    "SQLite updateOne error",
    "SQLite insertOne error",
    "SQLite findOne error",
    // Not a backend line: `_update`'s not-found WARN, unported by P4.149's
    // Ruling R-A (with `Entity created` / `Entity deleted`).
    "Entity not found for update",
];

/// P4.156 — one failure, two SQL texts: v4's query builder double-quotes every
/// identifier, so a renamed column reaches SQLite's "double-quoted string"
/// fallback and its message carries the quotes and a hint (`no such column:
/// "chatId" - should this be a string literal in single-quotes?`); v5's SQL
/// names the column bare (`no such column: chatId`). The v4 text is mapped onto
/// v5's before the compare — the column NAME stays compared.
fn normalize_v4_sqlite(text: &str) -> String {
    const HINT: &str = " - should this be a string literal in single-quotes?";
    match (
        text.strip_prefix("no such column: \""),
        text.strip_suffix(HINT),
    ) {
        (Some(_), Some(head)) => head
            .replacen("no such column: \"", "no such column: ", 1)
            .trim_end_matches('"')
            .to_string(),
        _ => text.to_string(),
    }
}

/// P4.149 — the planted delete failure (the oracle's `PLANT`, byte-identical):
/// a BEFORE DELETE trigger, so the selects inside the bulk deletes still
/// succeed and only the row delete fails.
const PLANT: &str = "CREATE TRIGGER qt_plant_no_delete BEFORE DELETE ON chat_informs \
                     BEGIN SELECT RAISE(ABORT, 'planted delete failure'); END";

/// One v4 line as the oracle recorded it (`{level, message, fields: [[k, v]…]}`,
/// the context in v4's own key order), rendered the way v5's capture rig renders
/// a `quilltap::db` event: `"<LEVEL> quilltap::db <message> k=v …"` — so the two
/// sides compare as whole strings, field ORDER and the bare `error` included.
fn v4_line(line: &Value) -> String {
    let level = line["level"].as_str().expect("level").to_ascii_uppercase();
    let mut out = format!(
        "{level} quilltap::db {}",
        line["message"].as_str().expect("message")
    );
    for pair in line["fields"].as_array().expect("fields") {
        let key = pair[0].as_str().expect("field key");
        let (key, value) = match &pair[1] {
            Value::String(s) if key == "error" => (key.to_string(), normalize_v4_sqlite(s)),
            Value::String(s) => (key.to_string(), s.clone()),
            // P4.156: an array / object field is the file layer's `…Json`
            // convention on v5 (the callsite serializes; the capture rig sees
            // the RAW field, so the suffixed key and the compact JSON).
            other @ (Value::Array(_) | Value::Object(_)) => {
                (format!("{key}Json"), other.to_string())
            }
            other => (key.to_string(), other.to_string()),
        };
        out.push_str(&format!(" {key}={value}"));
    }
    out
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
    // P4.D249: schema order, never omitted — Zod's `.default(false)` always
    // fills it.
    m.insert("permanent".into(), Value::from(r.permanent));
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
    m.insert("permanent".into(), Value::from(r.permanent));
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
    let scratch_dir = tempfile::Builder::new()
        .prefix("qt-chat-informs-rust-")
        .tempdir()
        .expect("tempdir");
    let work = scratch_dir.path().join("qt-chat-informs-rust.db");
    std::fs::copy(&fixture, &work).unwrap_or_else(|e| panic!("copy fixture: {e}"));

    let mut reads: Vec<Value> = Vec::new();
    let writer = Writer::open_writable(&work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open fixture copy: {e}"));
    {
        let repo = writer.chat_informs();
        for op in &spec.ops {
            // P4.156: every op answers `Result<Value, DbError>`; an `Err` is the
            // oracle's `{ threw: message }` (v4's op threw — a strict-scope
            // read, a failed create). `execSql` runs a raw plant.
            let run = || -> Result<Value, quilltap_core::db::DbError> {
                Ok(match op.kind.as_str() {
                    "execSql" => {
                        writer
                            .connection()
                            .execute_batch(op.sql.as_deref().expect("execSql needs sql"))
                            .expect("plant");
                        Value::Null
                    }
                    "plantDeleteFailure" => {
                        writer.connection().execute_batch(PLANT).expect("plant");
                        Value::Null
                    }
                    "dropPlant" => {
                        writer
                            .connection()
                            .execute_batch("DROP TRIGGER qt_plant_no_delete")
                            .expect("drop plant");
                        Value::Null
                    }
                    "findPendingForParticipant" => Value::Array(
                        repo.find_pending_for_participant(
                            op.chat_id.as_deref().unwrap(),
                            op.participant_id.as_deref().unwrap(),
                        )?
                        .iter()
                        .map(row_json)
                        .collect(),
                    ),
                    "findConsumedByMessages" => Value::Array(
                        repo.find_consumed_by_messages(
                            op.chat_id.as_deref().unwrap(),
                            op.participant_id.as_deref().unwrap(),
                            op.message_ids.as_deref().unwrap(),
                        )?
                        .iter()
                        .map(row_json)
                        .collect(),
                    ),
                    "findPendingBatches" => Value::Array(
                        repo.find_pending_batches(op.chat_id.as_deref().unwrap())?
                            .iter()
                            .map(|b| {
                                json!({
                                    "batchId": b.batch_id,
                                    "contentMarkdown": b.content_markdown,
                                    "createdAt": b.created_at,
                                    "recordMessageId": b.record_message_id,
                                    "permanent": b.permanent,
                                    "pendingParticipantIds": b.pending_participant_ids,
                                })
                            })
                            .collect(),
                    ),
                    "findByChatId" => Value::Array(
                        repo.find_by_chat_id(op.chat_id.as_deref().unwrap())?
                            .iter()
                            .map(row_json)
                            .collect(),
                    ),
                    "findByBatchId" => Value::Array(
                        repo.find_by_batch_id(op.batch_id.as_deref().unwrap())?
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
                            // v4 `params.permanent === true`.
                            op.permanent == Some(true),
                        )?
                        .iter()
                        .map(created_row_json)
                        .collect(),
                    ),
                    "markConsumed" => Value::from(repo.mark_consumed(
                        op.ids.as_deref().unwrap(),
                        op.message_id.as_deref().unwrap(),
                    )?),
                    "deletePendingByBatch" => {
                        Value::from(repo.delete_pending_by_batch(op.batch_id.as_deref().unwrap())?)
                    }
                    "deletePendingForParticipant" => {
                        Value::from(repo.delete_pending_for_participant(
                            op.chat_id.as_deref().unwrap(),
                            op.participant_id.as_deref().unwrap(),
                        )?)
                    }
                    // P4.156 (#145): the cancel's record-delete leg, through the
                    // home the route reads it through.
                    "deleteMessagesByIds" => {
                        let chat = op.chat_id.as_deref().unwrap();
                        let ids = op.message_ids.as_deref().unwrap();
                        Value::from(quilltap_core::db::fallback::messages_deleted_or_zero(
                            chat,
                            ids.len(),
                            || writer.chat_messages().delete_messages_by_ids(chat, ids),
                        ))
                    }
                    "deleteByChatId" => {
                        Value::from(repo.delete_by_chat_id(op.chat_id.as_deref().unwrap())?)
                    }
                    other => panic!("unknown op kind: {other}"),
                })
            };
            let (result, lines) = quilltap_core::test_support::captured_with(|| {
                let outcome = if op.strict {
                    quilltap_core::db::fallback::with_strict_repository_failures(run)
                } else {
                    run()
                };
                outcome.unwrap_or_else(
                    |e| json!({ "threw": quilltap_core::db::fallback::error_text(&e) }),
                )
            });
            let mut read = json!({ "kind": op.kind, "label": op.label, "result": result });
            if op.capture_logs || op.capture_debug {
                let logged: Vec<Value> = lines
                    .into_iter()
                    .filter(|l| {
                        l.starts_with("ERROR ")
                            || l.starts_with("WARN ")
                            || (op.capture_debug && l.starts_with("DEBUG quilltap::db "))
                    })
                    .map(Value::from)
                    .collect();
                read["logs"] = Value::Array(logged);
            }
            reads.push(read);
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

    // P4.149 — the captured lines, per op, before the result diff: the planted
    // delete failure's three-line shape (v4's backend line dropped) and the
    // healthy deletes' silence. Taken out of BOTH sides before normalization —
    // every id they carry is a committed spec value.
    let mut log_ops = 0usize;
    let mut arm_lines = 0usize;
    let mut log_fails: Vec<String> = Vec::new();
    for (g, w) in got["reads"]
        .as_array_mut()
        .expect("rust reads")
        .iter_mut()
        .zip(
            want["reads"]
                .as_array_mut()
                .expect("oracle reads")
                .iter_mut(),
        )
    {
        let w_logs = w.as_object_mut().and_then(|o| o.remove("logs"));
        let g_logs = g.as_object_mut().and_then(|o| o.remove("logs"));
        let Some(w_logs) = w_logs else {
            assert!(g_logs.is_none(), "rust captured an op the oracle did not");
            continue;
        };
        log_ops += 1;
        let want_lines: Vec<String> = w_logs
            .as_array()
            .expect("oracle logs")
            .iter()
            .filter(|l| !UNPORTED_BACKEND_LINES.contains(&l["message"].as_str().unwrap_or("")))
            .map(v4_line)
            .collect();
        let got_lines: Vec<String> = g_logs
            .expect("rust logs")
            .as_array()
            .expect("rust logs array")
            .iter()
            .map(|l| l.as_str().expect("line").to_string())
            .collect();
        arm_lines += want_lines.len();
        // P4.156: every diverging op is collected and reported together, so a
        // red run states its whole red count (the order's red-first record).
        if got_lines != want_lines {
            log_fails.push(format!(
                "logged lines diverged for `{}` — {}\n  rust:   {got_lines:?}\n  oracle: {want_lines:?}",
                w["kind"].as_str().unwrap_or("?"),
                w["label"].as_str().unwrap_or("")
            ));
        }
    }
    assert_eq!(
        (log_ops, arm_lines),
        (28, 38),
        "P4.149: six captured ops (four silence legs + the two planted delete arms, two lines \
         each); P4.156: eighteen more — five failed reads (one filter line each), four strict \
         reads (filter + outer), the batch read twice and the strict cancel (1 + 2 + 3), the \
         failed consume twice (2 + 2), the failed create twice (1 + 1); the consume whose \
         per-row read fails (one `Error finding entity by ID` per id); the failed record \
         delete (its fallback line); P4.163: four `captureDebug` consumes (the healthy one's \
         `Informs marked consumed`, the empty list's silence, the unknown id's `count: 0`, the \
         per-row-failure op's `count: 0` DEBUG) and the per-row failure inside the strict \
         scope (entity + update + wrap, all strict)"
    );

    // P4.156: an op that threw on v4 records `{ threw: message }`; the message
    // is the same SQLite failure in v4's quoted-identifier text.
    for w in want["reads"].as_array_mut().expect("oracle reads") {
        if let Some(t) = w["result"].get_mut("threw") {
            let n = normalize_v4_sqlite(t.as_str().expect("threw is a string"));
            *t = Value::String(n);
        }
    }

    // Per-op read diff first: a mismatch names the method and v4's own label.
    let got_reads = got["reads"].as_array().expect("rust reads");
    let want_reads = want["reads"].as_array().expect("oracle reads");
    assert_eq!(
        got_reads.len(),
        want_reads.len(),
        "read-op count diverged (the corpus changed under one side)"
    );
    let mut read_fails: Vec<String> = Vec::new();
    for (g, w) in got_reads.iter().zip(want_reads.iter()) {
        if g["result"] != w["result"] {
            read_fails.push(format!(
                "read result diverged for `{}` — {}\n  rust:   {}\n  oracle: {}",
                w["kind"].as_str().unwrap_or("?"),
                w["label"].as_str().unwrap_or(""),
                g["result"],
                w["result"],
            ));
        }
    }
    assert!(
        log_fails.is_empty() && read_fails.is_empty(),
        "{} op(s) logged differently and {} op(s) answered differently:\n{}\n{}",
        log_fails.len(),
        read_fails.len(),
        log_fails.join("\n"),
        read_fails.join("\n")
    );
    let threw = got["reads"]
        .as_array()
        .expect("rust reads")
        .iter()
        .filter(|r| r["result"].get("threw").is_some())
        .count();
    assert_eq!(
        threw, 10,
        "P4.156: the nine strict / create ops must THROW on v5 as on v4 (the five strict reads, \
         the strict cancel, the strict consume, both creates); P4.163: + the strict consume \
         whose per-row read fails"
    );

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
    // Three P4.D205 rows are created, but `deletePendingForParticipant` later
    // removes the one targeting p2, so TWO of those survive; the P4.D249
    // standing corpus creates two more (p2 standing, p3 default), both of which
    // survive the p1 seat removal — FOUR minted-id rows in the final state.
    assert_eq!(
        tokened, 4,
        "expected the four surviving createBatch rows to carry tokenized minted ids; the \
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
