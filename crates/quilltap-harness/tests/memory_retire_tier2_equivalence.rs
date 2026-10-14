//! Tier-2 differential test — P4.D288 (C1-7): the ONE retire path,
//! `quilltap_core::services::memory_retire`, against v4's REAL
//! `lib/memory/retire.ts` at the `83334b9a5` pin (the retire half of
//! `memory-index-and-retire.test.ts` + `retire-memories-unlink.test.ts`).
//!
//! The oracle builds `base.db` through v4's own `create` (four holders, one
//! per scenario — the chat wipe under bug 201's `keep: 'manual'`, invariant
//! 8's digest cascade incl. `retention` and a three-pass chain, the graph
//! repair's scan + scrub, the 200-neighbour WARN), runs the op list through
//! v4's real module and records each op's answer and lines. This side copies
//! `base.db`, replays the SAME ops on a caller-thread `Writer` (the capture rig
//! is thread-local — memory note `capture-rig-writer-thread`) and compares:
//!
//! - each answer EXACTLY (the ten-field `RetireReport`; `previewChatWipe`'s
//!   sorted ids; the dangling scan; the scrub report);
//! - each op's lines — `LEVEL message k=v …` in v4's key order, an object
//!   field riding as `<key>Json` (the file layer's convention), `durationMs`
//!   (real wall-clock both sides) masked;
//! - the end state of `memories` (minted `updatedAt` / `supersededAt` masked).
//!
//! v4's store is LOADED for every holder before the first op (L-2,
//! `MEMORY_INDEX_ALWAYS_LOADED`), so the index's counts and its `Re-stamped
//! vector tiers` lines meet v5's loaded answers. v4's announcement runs inside
//! `retireMemories`; v5's runs after the commit (R-AF) and is not replayed
//! here — none of its lines is captured on either side.
//!
//! Regenerate (Node 24, from the round's pin):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   rm -rf /tmp/qt-memory-retire-tier2 /tmp/oracle-memory-retire-tier2.ndjson
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_MEMORY_RETIRE_DIR=/tmp/qt-memory-retire-tier2 \
//!     $N/npx tsx $V5W/harness/oracle/cases/memory-retire-tier2.ts \
//!     > /tmp/oracle-memory-retire-tier2.ndjson
//! Run:
//!   QT_ORACLE_MEMORY_RETIRE_TIER2=/tmp/oracle-memory-retire-tier2.ndjson \
//!   QT_FIXTURE_MEMORY_RETIRE_DIR=/tmp/qt-memory-retire-tier2 \
//!     cargo test -p quilltap-harness --test memory_retire_tier2_equivalence

use quilltap_core::db::memories_read::{DanglingMemoryLink, DanglingMemoryLinks};
use quilltap_core::db::vector_store::Tier;
use quilltap_core::db::Writer;
use quilltap_core::services::memory_retire::{
    preview_chat_wipe, retire_chat_memories, retire_memories, scrub_dangling_memory_links,
    RetireOptions, RetireReason,
};
use serde_json::{json, Value};

const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
const T0: &str = "2026-01-01T00:00:00.000Z";

fn reason_of(s: &str) -> RetireReason {
    match s {
        "item-delete" => RetireReason::ItemDelete,
        "chat-wipe" => RetireReason::ChatWipe,
        "message-cascade" => RetireReason::MessageCascade,
        "message-reattribution" => RetireReason::MessageReattribution,
        "swipe-regenerate" => RetireReason::SwipeRegenerate,
        "character-delete" => RetireReason::CharacterDelete,
        "character-archive" => RetireReason::CharacterArchive,
        "restore-replace" => RetireReason::RestoreReplace,
        "retention" => RetireReason::Retention,
        "near-duplicate-fold" => RetireReason::NearDuplicateFold,
        "graph-repair" => RetireReason::GraphRepair,
        other => panic!("unknown reason {other}"),
    }
}

fn strings(v: &Value) -> Vec<String> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_str().unwrap().to_string())
        .collect()
}

/// v4's `DanglingMemoryLinks` JSON back into v5's struct (the scrub replays
/// over the links the PREVIOUS op scanned, as v4's does).
fn links_from_json(v: &Value) -> DanglingMemoryLinks {
    let link = |l: &Value| DanglingMemoryLink {
        id: l["id"].as_str().unwrap().to_string(),
        character_id: l["characterId"].as_str().unwrap().to_string(),
        target: l["target"].as_str().unwrap().to_string(),
    };
    DanglingMemoryLinks {
        related: v["related"].as_array().unwrap().iter().map(link).collect(),
        consolidated_from: v["consolidatedFrom"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| (link(l), l["memberCount"].as_i64().unwrap()))
            .collect(),
        superseded_by: v["supersededBy"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| (link(l), Tier::of_cell(l["tier"].as_str())))
            .collect(),
    }
}

fn mask_duration(l: &str) -> String {
    match l.find(" durationMs=") {
        Some(at) => {
            let rest = &l[at + 1..];
            let tail = rest.find(' ').map(|sp| &rest[sp..]).unwrap_or("");
            format!("{} durationMs=<n>{tail}", &l[..at])
        }
        None => l.to_string(),
    }
}

/// v4's lines as `LEVEL message k=v …`.
fn want_lines(op: &Value) -> Vec<String> {
    op["lines"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| {
            let mut out = format!(
                "{} {}",
                l["level"].as_str().unwrap().to_uppercase(),
                l["message"].as_str().unwrap()
            );
            for (k, v) in l["fields"].as_object().unwrap() {
                match v {
                    Value::String(s) => out.push_str(&format!(" {k}={s}")),
                    Value::Object(_) | Value::Array(_) => out.push_str(&format!(" {k}Json={v}")),
                    other => out.push_str(&format!(" {k}={other}")),
                }
            }
            mask_duration(&out)
        })
        .collect()
}

/// v5's captured lines minus their target.
fn got_lines(lines: Vec<String>) -> Vec<String> {
    lines
        .into_iter()
        .map(|l| {
            let mut parts = l.splitn(3, ' ');
            let level = parts.next().unwrap().to_string();
            let _target = parts.next();
            mask_duration(&format!("{level} {}", parts.next().unwrap_or_default()))
        })
        .collect()
}

/// Replay one op on the writer: `(answer, lines)`.
fn run_op(w: &Writer, op: &Value, last_links: &Value) -> (Value, Vec<String>) {
    let input = &op["input"];
    let conn = w.connection();
    let (answer, lines) = quilltap_core::test_support::captured_with(|| -> Value {
        match op["op"].as_str().unwrap() {
            "retire" => retire_memories(
                conn,
                &strings(&input["ids"]),
                RetireOptions {
                    reason: reason_of(input["reason"].as_str().unwrap()),
                    keep_manual: input["keepManual"].as_bool().unwrap(),
                },
            )
            .expect("retire")
            .to_json(),
            "retireChatMemories" => retire_chat_memories(conn, input["chatId"].as_str().unwrap())
                .expect("retireChatMemories")
                .to_json(),
            "previewChatWipe" => {
                let mut ids: Vec<String> =
                    preview_chat_wipe(conn, input["chatId"].as_str().unwrap())
                        .expect("previewChatWipe")
                        .into_iter()
                        .collect();
                ids.sort();
                json!(ids)
            }
            "findDanglingLinks" => quilltap_core::db::memories_read::find_dangling_links(
                conn,
                Some(&strings(&input["characterIds"])),
            )
            .expect("findDanglingLinks")
            .to_json(),
            "scrubDanglingMemoryLinks" => {
                scrub_dangling_memory_links(conn, &links_from_json(last_links))
                    .expect("scrubDanglingMemoryLinks")
                    .to_json()
            }
            other => panic!("unknown op {other}"),
        }
    });
    (answer, got_lines(lines))
}

/// The end-state dump, as the oracle selects it (every cell as text).
fn end_state(w: &Writer) -> Value {
    let cols = [
        "id",
        "characterId",
        "source",
        "tier",
        "supersededById",
        "supersededAt",
        "consolidatedFrom",
        "relatedMemoryIds",
        "embedding",
        "updatedAt",
    ];
    let mut stmt = w
        .connection()
        .prepare(
            "SELECT id, characterId, source, tier, supersededById, supersededAt, consolidatedFrom, \
                    relatedMemoryIds, quote(embedding) AS embedding, updatedAt FROM memories ORDER BY id",
        )
        .unwrap();
    let rows: Vec<Value> = stmt
        .query_map([], |r| {
            let mut o = serde_json::Map::new();
            for (i, name) in cols.iter().enumerate() {
                o.insert((*name).to_string(), json!(r.get::<_, Option<String>>(i)?));
            }
            Ok(Value::Object(o))
        })
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    Value::Array(rows)
}

/// Minted stamps → `<ts>` (anything but the seed's `T0`).
fn mask(rows: &Value) -> Value {
    Value::Array(
        rows.as_array()
            .unwrap()
            .iter()
            .map(|row| {
                let mut o = row.as_object().unwrap().clone();
                for k in ["supersededAt", "updatedAt"] {
                    if let Some(Value::String(s)) = o.get(k) {
                        if s != T0 {
                            o.insert(k.to_string(), json!("<ts>"));
                        }
                    }
                }
                Value::Object(o)
            })
            .collect(),
    )
}

#[test]
fn memory_retire_matches_v4() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_MEMORY_RETIRE_TIER2") else {
        eprintln!("SKIP: set QT_ORACLE_MEMORY_RETIRE_TIER2 to the oracle NDJSON (see header).");
        return;
    };
    let Ok(dir) = std::env::var("QT_FIXTURE_MEMORY_RETIRE_DIR") else {
        eprintln!("SKIP: set QT_FIXTURE_MEMORY_RETIRE_DIR to the case's dir (see header).");
        return;
    };
    let oracle: Value = serde_json::from_str(
        std::fs::read_to_string(&oracle_path)
            .unwrap_or_else(|e| panic!("read oracle: {e}"))
            .trim(),
    )
    .expect("oracle line");
    let scratch = tempfile::tempdir().unwrap();
    let work = scratch.path().join("main.db");
    std::fs::copy(std::path::Path::new(&dir).join("base.db"), &work).expect("copy base.db");
    let writer = Writer::open_writable(&work, PEPPER).expect("open the caller-thread writer");

    let ops = oracle["ops"].as_array().unwrap();
    assert!(ops.len() >= 13, "the op list shrank: {}", ops.len());
    let mut failures = Vec::new();
    let mut compared_lines = 0usize;
    let mut last_links = Value::Null;
    for (i, op) in ops.iter().enumerate() {
        let kind = op["op"].as_str().unwrap();
        let (got, got_lines) = run_op(&writer, op, &last_links);
        if kind == "findDanglingLinks" {
            last_links = op["result"].clone();
        }
        let want_lines = want_lines(op);
        if got != op["result"] {
            failures.push(format!(
                "op {i} ({kind}): result\n  v5 {got}\n  v4 {}",
                op["result"]
            ));
        }
        if got_lines != want_lines {
            failures.push(format!(
                "op {i} ({kind}): lines\n  v5 {got_lines:#?}\n  v4 {want_lines:#?}"
            ));
        }
        compared_lines += want_lines.len();
    }
    let got_end = mask(&end_state(&writer));
    let want_end = mask(&oracle["memories"]);
    if got_end != want_end {
        failures.push(format!("end state\n  v5 {got_end}\n  v4 {want_end}"));
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
    assert!(compared_lines >= 40, "only {compared_lines} lines compared");
    eprintln!(
        "OK: memory retire matched v4 ({} ops, {compared_lines} lines, {} rows).",
        ops.len(),
        want_end.as_array().unwrap().len()
    );
}
