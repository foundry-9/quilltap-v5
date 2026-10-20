//! P4.D315 Tier 1 item 9 — tier-2 differential: the chat wardrobe history's
//! DATA layer (`quilltap_core::db::chat_wardrobe_history`) against v4's REAL
//! `ChatWardrobeHistoryRepository` over the REAL `ChatsRepository` (the
//! `e8e17b2d9` pin).
//!
//! The oracle case (`harness/oracle/cases/chat-wardrobe-history-tier2.ts`)
//! builds the base DB per run at the pin — the spec's chats through v4's REAL
//! `repos.chats.create`, the table through the round's shared plant
//! `plantChatWardrobeHistoryTable()` — writes it to
//! `QT_FIXTURE_CWH_T2_DIR/base.db`, and runs the committed spec's ops
//! (`harness/oracle/fixtures/chat-wardrobe-history-tier2.json`): every `settle`
//! arm (the three arrivals, the empty-live and dressed-live first touches, the
//! Pascal tool name present / absent / empty / dropped on the one-row arm —
//! R-7, an unchanged re-settle, a live equal to an OLDER entry, the cursor back
//! / forward / onto the current index / missing, max + 1 after a rollback, a
//! corrupt `equippedOutfit` cell read as EMPTY through the fallback live read, a
//! missing chat (the FK), a table-less copy), the typed `upsert_rows` (a round
//! trip, a second current row, a duplicate index, a wrong stored hash, the
//! UPDATE arm keeping `chatId` / `characterId`, an FK miss), the raw
//! `upsert_raw_rows` fed the SAME JSON v4's `upsertRows` takes (JS bind
//! semantics, measured: `isCurrent` truthiness, `source` written as given — a
//! number lands `"5.0"` — `slots` salvaged, a numeric-string `entryIndex`, a
//! missing `id` written as NULL, a missing `createdAt` and a boolean `id`
//! refused), `delete_by_character`, the chat DELETE cascade. This test runs the
//! same ops on a COPY of that base through v5 and compares, per op:
//!
//! - the result (`SettleOutcome` in v4's object bytes; a written count; a
//!   thrown message as `{threw}`);
//! - the four reads (`find_entries` / `find_current` for the op's chat and
//!   character, `find_by_chat_id`, `find_all`) and the raw-table dumps;
//! - the realtime publishes (v4's `Realtime publish queued` lines; v5's
//!   thread-scoped bus, flushed and read back) — `appended` / `cursor-moved`
//!   ONLY;
//! - every repository log line (module `wardrobe-history` →
//!   `quilltap::wardrobe_history`, `module=wardrobe-history` leading; an object
//!   field renders as `<key>Json`, a `null` as `null`). v4's backend `Raw query
//!   failed` (the standing NO-PORT class) and the chats repository's own lines
//!   are outside this family's comparand.
//!
//! Masks: a settle-minted `id` reads `<minted>` (the spec's fixed ids stay), a
//! settle-minted `createdAt` reads `<now>` (one clock per call — masked, never
//! ordered by).
//!
//! **Recorded divergence `NULL_ID_READS_EMPTY`** (pinned both ways): a raw
//! restore row with no `id` is written with a NULL id on BOTH sides (v4's
//! legacy non-INTEGER primary key admits it — measured), but v4's
//! `normalizeRow` reads it back `id: null` where v5's typed row reads `""`. The
//! comparand maps v4's `null` to `""` and asserts it happened.
//!
//! **Recorded divergence `NAN_ENTRY_INDEX_READS_ZERO`** (pinned both ways; the
//! `e8e17b2d9` unification's §3 review — the order's item 3 named the class
//! and no row drove it): a raw restore row whose `entryIndex` is a NON-NUMERIC
//! string (`"abc"`) binds TEXT and INSERTs on both sides (INTEGER affinity
//! cannot coerce it; the table has no CHECK), but v4's `normalizeRow`
//! `Number("abc")` is `NaN`, which JSON renders `null`, where v5's typed
//! `i64` reads `0`. The comparand maps v4's `null` `entryIndex` to `0` and
//! asserts it happened. The consequence is RECORDED, not driven: a later
//! `settle` on such a timeline computes `NaN + 1` in v4 (the INSERT fails →
//! `skipped`) and `0 + 1` in v5 (an entry 1 appends). Reachable only through
//! a hand-edited archive (the `NULL_ID_READS_EMPTY` reach).
//!
//! Generate (Node 24, from the pinned v4 worktree):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   V5W=${V5W:-$HOME/source/quilltap-v5}
//!   rm -rf /tmp/qt-cwh-t2
//!   QT_FIXTURE_OUT_DIR=/tmp/qt-cwh-t2 \
//!     $N/npx tsx $V5W/harness/oracle/cases/chat-wardrobe-history-tier2.ts \
//!     > /tmp/oracle-cwh-t2.ndjson
//! Run:
//!   QT_ORACLE_CWH_T2=/tmp/oracle-cwh-t2.ndjson QT_FIXTURE_CWH_T2_DIR=/tmp/qt-cwh-t2 \
//!     cargo test -p quilltap-harness --test chat_wardrobe_history_tier2_equivalence -- --nocapture

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use quilltap_core::api::types::EventPayload;
use quilltap_core::db::chat_wardrobe_history::{
    ChatWardrobeHistoryRepository, ChatWardrobeHistoryRow, WardrobeHistoryIntent,
    WardrobeHistoryProvenance, WardrobeHistorySource,
};
use quilltap_core::db::chats_outfits::ChatOutfitsRepository;
use quilltap_core::db::fallback::error_text;
use quilltap_core::db::{DbError, Writer};
use quilltap_core::realtime::bus::{
    arm_realtime_bus_for_current_thread, disarm_realtime_bus_for_current_thread, BusSpawner,
};
use quilltap_core::test_support::captured_with;
use serde_json::{json, Map, Value};

const TARGET: &str = "quilltap::wardrobe_history";

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/chat-wardrobe-history-tier2.json")
}

/// The spec's intent JSON as v5's type (v4's object, key for key).
fn intent(v: &Value) -> WardrobeHistoryIntent {
    match v["kind"].as_str().unwrap() {
        "append" => {
            let p = &v["provenance"];
            let source: WardrobeHistorySource =
                serde_json::from_value(p["source"].clone()).unwrap();
            let mut provenance = WardrobeHistoryProvenance::new(source);
            if let Some(a) = p.get("actorCharacterId").and_then(Value::as_str) {
                provenance = provenance.with_actor(a);
            }
            if let Some(t) = p.get("toolName").and_then(Value::as_str) {
                provenance = provenance.with_tool_name(t);
            }
            WardrobeHistoryIntent::Append { provenance }
        }
        "cursor" => WardrobeHistoryIntent::Cursor {
            index: v["index"].as_i64().unwrap(),
        },
        other => panic!("unknown intent kind {other}"),
    }
}

struct Masks {
    ids: HashSet<String>,
    created: HashSet<String>,
}

impl Masks {
    fn row(&self, row: &mut Value) {
        let Some(o) = row.as_object_mut() else {
            return;
        };
        if let Some(Value::String(id)) = o.get("id") {
            if !id.is_empty() && !self.ids.contains(id) {
                o.insert("id".into(), json!("<minted>"));
            }
        }
        if let Some(Value::String(at)) = o.get("createdAt") {
            if !self.created.contains(at) {
                o.insert("createdAt".into(), json!("<now>"));
            }
        }
    }
    fn value(&self, v: &mut Value) {
        match v {
            Value::Array(rows) => rows.iter_mut().for_each(|r| self.row(r)),
            Value::Object(_) => self.row(v),
            _ => {}
        }
    }
}

fn rows(r: Result<Vec<ChatWardrobeHistoryRow>, DbError>) -> Value {
    match r {
        Ok(rows) => serde_json::to_value(rows).unwrap(),
        Err(e) => json!({ "threw": error_text(&e) }),
    }
}

/// v4's log entry as v5's capture rig renders the ported line, for the
/// repository's own messages only.
fn expected_line(entry: &Value) -> Option<String> {
    const MESSAGES: [&str; 8] = [
        "Wardrobe history settle failed; the outfit change stands unrecorded",
        "Recorded wardrobe history entry",
        "Outfit unchanged since the current entry; nothing recorded",
        "Wardrobe history cursor target missing; cursor left where it was",
        "Moved wardrobe history cursor",
        "Skipped a wardrobe history row that would break the timeline",
        "Upserted wardrobe history rows",
        "Dropped wardrobe history for a deleted character",
    ];
    let message = entry["message"].as_str().unwrap();
    if !MESSAGES.contains(&message) {
        return None;
    }
    let fields: Vec<String> = entry["fields"]
        .as_array()
        .unwrap()
        .iter()
        .map(|kv| {
            let k = kv[0].as_str().unwrap();
            match &kv[1] {
                Value::String(s) => format!("{k}={s}"),
                v @ (Value::Object(_) | Value::Array(_)) => format!("{k}Json={v}"),
                other => format!("{k}={other}"),
            }
        })
        .collect();
    Some(format!(
        "{} {TARGET} {message} module=wardrobe-history {}",
        entry["level"].as_str().unwrap().to_uppercase(),
        fields.join(" ")
    ))
}

fn dump(conn: &rusqlite::Connection) -> Value {
    let mut stmt = conn
        .prepare(
            "SELECT * FROM \"chat_wardrobe_history\" ORDER BY \"chatId\", \"characterId\", \"entryIndex\", rowid",
        )
        .unwrap();
    let names: Vec<String> = stmt.column_names().into_iter().map(String::from).collect();
    let out: Vec<Value> = stmt
        .query_map([], |r| {
            let mut o = Map::new();
            for (j, n) in names.iter().enumerate() {
                let v = match r.get_ref(j)? {
                    rusqlite::types::ValueRef::Null => Value::Null,
                    rusqlite::types::ValueRef::Integer(x) => json!(x),
                    rusqlite::types::ValueRef::Real(x) if x.fract() == 0.0 => json!(x as i64),
                    rusqlite::types::ValueRef::Real(x) => json!(x),
                    rusqlite::types::ValueRef::Text(t) => json!(String::from_utf8_lossy(t)),
                    rusqlite::types::ValueRef::Blob(_) => Value::Null,
                };
                o.insert(n.clone(), v);
            }
            Ok(Value::Object(o))
        })
        .unwrap()
        .map(Result::unwrap)
        .collect();
    Value::Array(out)
}

#[test]
fn chat_wardrobe_history_tier2_matches_oracle() {
    let (Some(oracle_path), Some(base_dir)) = (
        std::env::var_os("QT_ORACLE_CWH_T2"),
        std::env::var_os("QT_FIXTURE_CWH_T2_DIR"),
    ) else {
        eprintln!("SKIP: set QT_ORACLE_CWH_T2 and QT_FIXTURE_CWH_T2_DIR (see header).");
        return;
    };
    let spec: Value = serde_json::from_str(&std::fs::read_to_string(spec_path()).unwrap()).unwrap();
    let text = std::fs::read_to_string(&oracle_path).expect("read oracle");
    let oracle: Value = serde_json::from_str(
        text.lines()
            .find(|l| l.starts_with('{'))
            .expect("an NDJSON line"),
    )
    .expect("parse oracle");
    let theirs = oracle["results"].as_array().expect("oracle results");
    let ops = spec["ops"].as_array().unwrap();
    assert_eq!(theirs.len(), ops.len(), "op count — regenerate the oracle");

    // The fixed ids / timestamps the masks keep.
    let mut masks = Masks {
        ids: HashSet::new(),
        created: spec["fixedCreatedAts"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s.as_str().unwrap().to_string())
            .collect(),
    };
    for op in ops {
        for r in op["rows"].as_array().into_iter().flatten() {
            if let Some(id) = r["id"].as_str() {
                masks.ids.insert(id.to_string());
            }
        }
    }

    let scratch = tempfile::tempdir().unwrap();
    let work = scratch.path().join("work.db");
    std::fs::copy(Path::new(&base_dir).join("base.db"), &work).expect("copy base.db");
    let pepper = spec["testPepperBase64"].as_str().unwrap();
    let w = Writer::open_writable(&work, pepper).expect("open the base copy");
    let conn = w.connection();
    let repo = ChatWardrobeHistoryRepository::new(conn);
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .unwrap();

    let mut null_ids_seen = 0usize;
    let mut nan_entry_indexes_seen = 0usize;
    let mut publishes_seen = 0usize;
    for (i, (op, v4)) in ops.iter().zip(theirs).enumerate() {
        let label = op["label"].as_str().unwrap();
        let kind = op["kind"].as_str().unwrap();
        let chat_id = op["chatId"].as_str();
        let character_id = op["characterId"].as_str();

        // A fresh thread-scoped bus per op; every new pending key spawns one
        // flush, which lands on the runtime 250 ms later.
        let (tx, mut rx) = tokio::sync::broadcast::channel(64);
        let spawned = Arc::new(AtomicUsize::new(0));
        let (count, handle) = (spawned.clone(), rt.handle().clone());
        let spawner: BusSpawner = Arc::new(move |fut| {
            count.fetch_add(1, Ordering::SeqCst);
            handle.spawn(fut);
        });
        arm_realtime_bus_for_current_thread(tx, spawner, || 0);
        let (mut mine, lines) = captured_with(|| match kind {
            "setOutfit" => {
                ChatOutfitsRepository::new(conn)
                    .set_equipped_outfit(chat_id.unwrap(), character_id.unwrap(), &op["slots"])
                    .unwrap();
                Value::Null
            }
            "settle" => serde_json::to_value(repo.settle(
                chat_id.unwrap(),
                character_id.unwrap(),
                &intent(&op["intent"]),
            ))
            .unwrap(),
            "execSql" => {
                conn.execute_batch(op["sql"].as_str().unwrap()).unwrap();
                Value::Null
            }
            "dropTable" => {
                conn.execute_batch("DROP TABLE \"chat_wardrobe_history\"")
                    .unwrap();
                Value::Null
            }
            "exportDeleteUpsert" => {
                let exported = repo.find_by_chat_id(chat_id.unwrap()).unwrap();
                conn.execute(
                    "DELETE FROM \"chat_wardrobe_history\" WHERE \"chatId\" = ?1",
                    [chat_id.unwrap()],
                )
                .unwrap();
                json!({ "exported": exported.len(), "written": repo.upsert_rows(&exported) })
            }
            "upsertRows" => {
                let typed: Vec<ChatWardrobeHistoryRow> =
                    serde_json::from_value(op["rows"].clone()).unwrap();
                json!(repo.upsert_rows(&typed))
            }
            "upsertRawRows" => json!(repo.upsert_raw_rows(op["rows"].as_array().unwrap())),
            "deleteByCharacter" => {
                repo.delete_by_character(character_id.unwrap()).unwrap();
                Value::Null
            }
            "dump" => dump(conn),
            other => panic!("unknown op kind {other}"),
        });
        disarm_realtime_bus_for_current_thread();
        let mut hints = Vec::new();
        for _ in 0..spawned.load(Ordering::SeqCst) {
            let ev = rt
                .block_on(async {
                    tokio::time::timeout(std::time::Duration::from_secs(5), rx.recv()).await
                })
                .expect("a flushed hint")
                .expect("the channel open");
            if let EventPayload::Realtime(h) = ev.payload {
                hints.push(json!({ "topic": h.topic, "id": h.id }));
            }
        }

        let mut want = v4["result"].clone();
        if kind == "dump" {
            masks.value(&mut mine);
            masks.value(&mut want);
        }
        assert_eq!(mine, want, "[#{i} {label}] result");

        // The four reads.
        let mut reads = json!({
            "entries": match (chat_id, character_id) {
                (Some(c), Some(ch)) => rows(repo.find_entries(c, ch)),
                _ => Value::Null,
            },
            "current": match (chat_id, character_id) {
                (Some(c), Some(ch)) => match repo.find_current(c, ch) {
                    Ok(row) => serde_json::to_value(row).unwrap(),
                    Err(e) => json!({ "threw": error_text(&e) }),
                },
                _ => Value::Null,
            },
            "byChat": chat_id.map(|c| rows(repo.find_by_chat_id(c))).unwrap_or(Value::Null),
            "all": rows(repo.find_all()),
        });
        let mut want_reads = v4["reads"].clone();
        for key in ["entries", "current", "byChat", "all"] {
            masks.value(&mut reads[key]);
            // NULL_ID_READS_EMPTY: v4's `id: null` is v5's `""`.
            if let Some(list) = want_reads[key].as_array_mut() {
                for r in list.iter_mut() {
                    if r.get("id") == Some(&Value::Null) {
                        r["id"] = json!("");
                        null_ids_seen += 1;
                    }
                    // NAN_ENTRY_INDEX_READS_ZERO: v4's `entryIndex: null`
                    // (`Number("abc")` → NaN → JSON null) is v5's `0`.
                    if r.get("entryIndex") == Some(&Value::Null) {
                        r["entryIndex"] = json!(0);
                        nan_entry_indexes_seen += 1;
                    }
                }
            }
            masks.value(&mut want_reads[key]);
        }
        assert_eq!(reads, want_reads, "[#{i} {label}] reads");

        let want_hints: Vec<Value> = v4["publishes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| json!({ "topic": p["topic"], "id": p["id"] }))
            .collect();
        publishes_seen += want_hints.len();
        assert_eq!(hints, want_hints, "[#{i} {label}] realtime publishes");

        let mine_lines: Vec<String> = lines
            .into_iter()
            .filter(|l| l.split(' ').nth(1) == Some(TARGET))
            .collect();
        let want_lines: Vec<String> = v4["logs"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(expected_line)
            .collect();
        assert_eq!(mine_lines, want_lines, "[#{i} {label}] log lines");
    }
    assert!(
        null_ids_seen > 0,
        "NULL_ID_READS_EMPTY: v4 no longer reads a NULL id back — retire the carve"
    );
    assert!(
        nan_entry_indexes_seen > 0,
        "NAN_ENTRY_INDEX_READS_ZERO: v4 no longer reads a non-numeric entryIndex as null — retire the carve"
    );
    assert!(
        publishes_seen > 0,
        "no op published — the bus capture is vacuous"
    );
    eprintln!(
        "OK: the chat wardrobe history matched v4 on {} ops ({publishes_seen} publishes).",
        ops.len()
    );
}
