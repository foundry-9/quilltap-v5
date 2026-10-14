//! Tier-2 differential test: v4's `MemoriesRepository` write / mutation surface
//! (Phase-2, main DB).
//!
//! Both sides run the SAME op sequence (`memories-tier2.json`) on a fresh copy of
//! the empty-table seed fixture, then the `memories` table is dumped canonically
//! and the post-op state is asserted identical. Exercises `create` (rich +
//! minimal, embedding BLOB + null), `update`, `updateForCharacter` (owned +
//! not-owned no-op), `updateAccessTime{,Bulk}`, `replaceInMemories`,
//! `deleteForCharacter` (not-owned no-op), `bulkDelete`, `delete`,
//! `deleteByChatId`, `deleteBySourceMessageId{,s}`. Since P4.161 the oracle
//! also records five `createExpectError` rows (v4's REAL `create` over rows
//! `MemorySchema` refuses) compared against `parse_create_memory`.
//!
//! P4.D265 (v4 `d58548051` / `70f9b495c`): the oracle also runs a TIER phase
//! after the op sequence and before the dump, and emits it as `tier: [{op,
//! returned, lines}]`; this side REPLAYS that exact op list (creates through
//! `parse_create_memory` — v4's prefill + `MemorySchema` — then
//! `MemoriesRepository::create`; `increment_reinforcement`, `update_tier_bulk`,
//! `mark_considered` on the writer; L-6 (P4.D288): the retired
//! `markMemoriesAccessed` re-aimed at `update_access_time_bulk`) and compares each
//! op's answer and its lines (level, message, fields in v4's key order).
//!
//! P4.D288 (C1-7, the `83334b9a5` pin): the committed op list's retired
//! deleters and `replaceInMemories` are RE-AIMED onto the pin's successors
//! (the oracle header carries the mapping), and a KEYSTONE phase after the
//! tier phase replays every new repository verb from the oracle's own op list
//! — answers and lines compared as the tier phase's are.
//!
//! NORMALIZATION: minted-timestamp placeholder. ids + createdAt + every payload
//! column are pinned; only `updatedAt` (bumped by every mutator) and
//! `lastAccessedAt` (set by updateAccessTime{,Bulk}) are minted, so both are
//! collapsed to `<ts>` on BOTH dumps. The `embedding` BLOB is dumped as lowercase
//! hex on both sides (a text-only update leaves it untouched).
//!
//! Generate the oracle output + fixture (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-mem-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-memories-fixture.ts
//!   QT_FIXTURE_MEM=/tmp/qt-mem-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/memories-tier2.ts > /tmp/oracle-mem.ndjson
//! Run:
//!   QT_ORACLE_MEM=/tmp/oracle-mem.ndjson \
//!   QT_FIXTURE_MEM=/tmp/qt-mem-fixture.db \
//!     cargo test -p quilltap-harness --test memories_tier2_equivalence

use quilltap_core::db::memories::{CreateOptions, MemCreate, MemUpdate};
use quilltap_core::db::Writer;
use quilltap_core::services::memory_retire::{
    retire_chat_memories, retire_memories, RetireOptions, RetireReason,
};
use serde::Deserialize;
use serde_json::Value;

/// The two minted-timestamp columns collapsed to `<ts>` on both dumps.
const TS_COLUMNS: &[&str] = &["updatedAt", "lastAccessedAt"];

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
    Create {
        options: Opts,
        data: Box<CreateData>,
    },
    #[serde(rename = "update")]
    Update { id: String, data: PatchData },
    #[serde(rename = "updateForCharacter")]
    UpdateForCharacter {
        #[serde(rename = "characterId")]
        character_id: String,
        #[serde(rename = "memoryId")]
        memory_id: String,
        data: PatchData,
    },
    #[serde(rename = "updateAccessTime")]
    UpdateAccessTime {
        #[serde(rename = "characterId")]
        character_id: String,
        #[serde(rename = "memoryId")]
        memory_id: String,
    },
    #[serde(rename = "updateAccessTimeBulk")]
    UpdateAccessTimeBulk {
        #[serde(rename = "characterId")]
        character_id: String,
        ids: Vec<String>,
    },
    #[serde(rename = "replaceInMemories")]
    ReplaceInMemories {
        ids: Vec<String>,
        search: String,
        replace: String,
    },
    #[serde(rename = "delete")]
    Delete { id: String },
    #[serde(rename = "deleteForCharacter")]
    DeleteForCharacter {
        #[serde(rename = "characterId")]
        character_id: String,
        #[serde(rename = "memoryId")]
        memory_id: String,
    },
    #[serde(rename = "bulkDelete")]
    BulkDelete {
        #[serde(rename = "characterId")]
        character_id: String,
        ids: Vec<String>,
    },
    #[serde(rename = "deleteByChatId")]
    DeleteByChatId {
        #[serde(rename = "chatId")]
        chat_id: String,
    },
    #[serde(rename = "deleteBySourceMessageId")]
    DeleteBySourceMessageId {
        #[serde(rename = "sourceMessageId")]
        source_message_id: String,
    },
    /// v4 `ad1c4c37f` deletes `deleteBySourceMessageIds` as dead code (v5
    /// never had a production caller either — retired, not ported). The tag
    /// is kept BY NAME as a no-op so the committed fixture's final op still
    /// deserializes unchanged (§R.7/no-fixture-edit) — never as a
    /// `#[serde(other)]` catch-all, which would let a future op the oracle
    /// runs and the Rust side does not know pass as a silent no-op (the
    /// `b0b6656b5` unification review).
    #[serde(rename = "deleteBySourceMessageIds")]
    Retired {
        #[allow(dead_code)]
        ids: Vec<String>,
    },
}

#[test]
fn an_unknown_op_kind_still_refuses_to_deserialize() {
    assert!(serde_json::from_str::<Op>(r#"{"kind":"bogus","ids":[]}"#).is_err());
    assert!(
        serde_json::from_str::<Op>(r#"{"kind":"deleteBySourceMessageIds","ids":["m1"]}"#).is_ok()
    );
}

#[derive(Deserialize)]
struct Opts {
    id: String,
    #[serde(rename = "createdAt")]
    created_at: String,
    #[serde(rename = "updatedAt")]
    updated_at: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateData {
    character_id: String,
    about_character_id: Option<String>,
    chat_id: Option<String>,
    project_id: Option<String>,
    content: String,
    summary: String,
    keywords: Vec<String>,
    tags: Vec<String>,
    importance: f64,
    embedding: Option<Vec<f32>>,
    source: String,
    witnessed_context: Option<String>,
    // Episodic spine (v4 8bf3cb5f): absent in the op spec → the MemorySchema
    // defaults v4's `_create` materializes (NULL / NULL / [] / 'semantic').
    #[serde(default)]
    occurred_at: Option<String>,
    #[serde(default)]
    narrative_time: Option<String>,
    #[serde(default)]
    entities: Vec<String>,
    #[serde(default)]
    kind: Option<String>,
    source_message_id: Option<String>,
    last_accessed_at: Option<String>,
    reinforcement_count: f64,
    last_reinforced_at: Option<String>,
    related_memory_ids: Vec<String>,
    reinforced_importance: f64,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct PatchData {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    summary: Option<String>,
    #[serde(default)]
    importance: Option<f64>,
    #[serde(default)]
    keywords: Option<Vec<String>>,
    #[serde(default)]
    tags: Option<Vec<String>>,
    #[serde(default)]
    source: Option<String>,
}

impl CreateData {
    fn to_mem_create(&self) -> MemCreate {
        MemCreate {
            character_id: self.character_id.clone(),
            about_character_id: self.about_character_id.clone(),
            chat_id: self.chat_id.clone(),
            project_id: self.project_id.clone(),
            content: self.content.clone(),
            summary: self.summary.clone(),
            keywords: self.keywords.clone(),
            tags: self.tags.clone(),
            importance: self.importance,
            embedding: self.embedding.clone(),
            source: self.source.clone(),
            witnessed_context: self.witnessed_context.clone(),
            occurred_at: self.occurred_at.clone(),
            narrative_time: self.narrative_time.clone(),
            entities: self.entities.clone(),
            kind: self.kind.clone().unwrap_or_else(|| "semantic".to_string()),
            source_message_id: self.source_message_id.clone(),
            last_accessed_at: self.last_accessed_at.clone(),
            reinforcement_count: self.reinforcement_count,
            last_reinforced_at: self.last_reinforced_at.clone(),
            related_memory_ids: self.related_memory_ids.clone(),
            reinforced_importance: self.reinforced_importance,
            tier: "hot".to_string(),
            superseded_by_id: None,
            superseded_at: None,
            consolidated_from: Vec::new(),
            consolidated_at: None,
        }
    }
}

impl PatchData {
    fn to_mem_update(&self) -> MemUpdate {
        MemUpdate {
            content: self.content.clone(),
            summary: self.summary.clone(),
            importance: self.importance,
            keywords: self.keywords.clone(),
            tags: self.tags.clone(),
            source: self.source.clone(),
            ..Default::default()
        }
    }
}

/// A JS number as `JSON.stringify` writes it — an integral value without `.0`.
fn js_number(n: f64) -> Value {
    if n.fract() == 0.0 && n.abs() < 9_007_199_254_740_992.0 {
        Value::from(n as i64)
    } else {
        Value::from(n)
    }
}

/// Collapse the two minted-timestamp columns to `<ts>` (non-null only), in place.
fn normalize(dump: &mut Value, label: &str) {
    let rows = dump
        .get_mut("rows")
        .and_then(Value::as_array_mut)
        .unwrap_or_else(|| panic!("{label}: dump has no rows array"));
    for row in rows.iter_mut() {
        let obj = row
            .as_object_mut()
            .unwrap_or_else(|| panic!("{label}: row is not an object"));
        for col in TS_COLUMNS {
            if obj.get(*col).map(|v| !v.is_null()).unwrap_or(false) {
                obj.insert((*col).to_string(), Value::String("<ts>".to_string()));
            }
        }
        // P4.D288: `supersededAt` is PINNED when a create states it (the
        // keystone phase's `2026-05-0…` stamps compare exactly) and MINTED
        // (`now`) when a tier move sets `supersededById` — only the minted
        // ones collapse, so NULL vs set still compares.
        if let Some(Value::String(v)) = obj.get("supersededAt") {
            if !v.starts_with("2026-05-0") {
                obj.insert(
                    "supersededAt".to_string(),
                    Value::String("<ts>".to_string()),
                );
            }
        }
    }
}

/// v4's counted-reinforcement answer — `{ reinforcementCount,
/// reinforcedImportance, lastReinforcedAt }` (P4.D288: `lastReinforcedAt`
/// joined) or `null`.
fn reinforcement_json(r: Option<quilltap_core::db::memories::Reinforcement>) -> Value {
    match r {
        Some(r) => serde_json::json!({
            "reinforcementCount": js_number(r.reinforcement_count),
            "reinforcedImportance": js_number(r.reinforced_importance),
            "lastReinforcedAt": r.last_reinforced_at,
        }),
        None => Value::Null,
    }
}

/// An oracle op's captured lines rendered as the capture rig renders v5's:
/// `LEVEL target message k=v …`, fields in v4's key order.
fn want_lines(t: &Value) -> Vec<String> {
    t["lines"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| {
            let message = l["message"].as_str().unwrap();
            let target = if message.starts_with("[Memory]") {
                "quilltap::memories"
            } else {
                "quilltap::db"
            };
            let mut out = format!(
                "{} {target} {message}",
                l["level"].as_str().unwrap().to_uppercase()
            );
            for (k, v) in l["fields"].as_object().unwrap() {
                let rendered = match v {
                    Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                out.push_str(&format!(" {k}={rendered}"));
            }
            out
        })
        .collect()
}

#[test]
fn memories_tier2_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_MEM") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_MEM to the oracle NDJSON (see header).");
            return;
        }
    };
    let fixture = match std::env::var("QT_FIXTURE_MEM") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_FIXTURE_MEM to the seed fixture .db (see header).");
            return;
        }
    };

    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../harness/oracle/fixtures/memories-tier2.json"),
        )
        .unwrap_or_else(|e| panic!("read spec: {e}")),
    )
    .expect("parse spec");

    let oracle_text =
        std::fs::read_to_string(&oracle_path).unwrap_or_else(|e| panic!("read oracle: {e}"));
    let mut oracle: Value = serde_json::from_str(oracle_text.trim()).expect("parse oracle dump");

    let work_scratch = tempfile::Builder::new()
        .prefix("qt-mem-rust-")
        .tempdir()
        .expect("tempdir");
    let work = work_scratch.path().join("work.db");
    std::fs::copy(&fixture, &work).unwrap_or_else(|e| panic!("copy fixture: {e}"));

    let writer = Writer::open_writable(&work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open fixture copy: {e}"));
    {
        let repo = writer.memories();
        for op in &spec.ops {
            match op {
                Op::Create { options, data } => {
                    repo.create(
                        &data.to_mem_create(),
                        &CreateOptions {
                            id: options.id.clone(),
                            created_at: options.created_at.clone(),
                            updated_at: options.updated_at.clone(),
                        },
                    )
                    .expect("create");
                }
                Op::Update { id, data } => {
                    repo.update(id, &data.to_mem_update()).expect("update");
                }
                Op::UpdateForCharacter {
                    character_id,
                    memory_id,
                    data,
                } => {
                    repo.update_for_character(character_id, memory_id, &data.to_mem_update())
                        .expect("updateForCharacter");
                }
                Op::UpdateAccessTime {
                    character_id,
                    memory_id,
                } => {
                    repo.update_access_time(character_id, memory_id)
                        .expect("updateAccessTime");
                }
                Op::UpdateAccessTimeBulk { character_id, ids } => {
                    repo.update_access_time_bulk(character_id, ids)
                        .expect("updateAccessTimeBulk");
                }
                Op::ReplaceInMemories {
                    ids,
                    search,
                    replace,
                } => {
                    repo.replace_in_memories(ids, search, replace)
                        .expect("replaceInMemories");
                }
                // P4.D288 (C1-7): the retired deleters RE-AIMED onto the pin's
                // successors, the oracle's mapping (see its header).
                Op::Delete { id } => {
                    retire_memories(
                        writer.connection(),
                        std::slice::from_ref(id),
                        RetireOptions::new(RetireReason::ItemDelete),
                    )
                    .expect("delete → retireMemories");
                }
                Op::DeleteForCharacter {
                    character_id,
                    memory_id,
                } => {
                    repo.bulk_delete_unsafe(character_id, std::slice::from_ref(memory_id))
                        .expect("deleteForCharacter → bulkDeleteUnsafe");
                }
                Op::BulkDelete { character_id, ids } => {
                    repo.bulk_delete_unsafe(character_id, ids)
                        .expect("bulkDelete → bulkDeleteUnsafe");
                }
                Op::DeleteByChatId { chat_id } => {
                    retire_chat_memories(writer.connection(), chat_id)
                        .expect("deleteByChatId → retireChatMemories");
                }
                Op::DeleteBySourceMessageId { source_message_id } => {
                    let ids: Vec<String> =
                        quilltap_core::db::memories_read::find_by_source_message_id(
                            writer.connection(),
                            source_message_id,
                        )
                        .expect("findBySourceMessageId")
                        .iter()
                        .filter_map(|m| m["id"].as_str().map(str::to_string))
                        .collect();
                    retire_memories(
                        writer.connection(),
                        &ids,
                        RetireOptions::new(RetireReason::MessageCascade),
                    )
                    .expect("deleteBySourceMessageId → retireMemories");
                }
                Op::Retired { .. } => {}
            }
        }
    }

    // === P4.D265: the tier phase, replayed from the oracle's own op list ===
    let tier_ops = oracle["tier"]
        .as_array()
        .cloned()
        .expect("the oracle carries no `tier` phase — regenerate it (P4.D265)");
    assert!(
        tier_ops.len() >= 20,
        "the tier phase shrank: {}",
        tier_ops.len()
    );
    let mut tier_lines = 0usize;
    for (i, t) in tier_ops.iter().enumerate() {
        let op = &t["op"];
        let kind = op["kind"].as_str().unwrap();
        let s = |k: &str| op[k].as_str().unwrap().to_string();
        let ids = || -> Vec<String> {
            op["ids"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap().to_string())
                .collect()
        };
        let (returned, lines): (Value, Vec<String>) = match kind {
            "create" => {
                let opts = &op["options"];
                let id = opts["id"].as_str().unwrap();
                let data = quilltap_core::db::memories::parse_create_memory(&op["data"], Some(id))
                    .unwrap_or_else(|e| panic!("tier op {i}: v4 accepted, v5 refused: {e}"));
                writer
                    .memories()
                    .create(
                        &data,
                        &CreateOptions {
                            id: id.to_string(),
                            created_at: opts["createdAt"].as_str().unwrap().to_string(),
                            updated_at: opts["updatedAt"].as_str().unwrap().to_string(),
                        },
                    )
                    .expect("tier create");
                (Value::Null, Vec::new())
            }
            "plant" => {
                writer
                    .connection()
                    .execute_batch(op["sql"].as_str().unwrap())
                    .expect("plant");
                (Value::Null, Vec::new())
            }
            "incrementReinforcement" => {
                let (r, lines) = quilltap_core::test_support::captured_with(|| {
                    writer
                        .memories()
                        .increment_reinforcement(&s("characterId"), &s("memoryId"), &s("at"), 1)
                        .expect("incrementReinforcement")
                });
                (reinforcement_json(r), lines)
            }
            "updateTierBulk" => {
                let extra = &op["extra"];
                let tri = |k: &str| match extra.get(k) {
                    None => None,
                    Some(Value::Null) => Some(None),
                    Some(v) => Some(Some(v.as_str().unwrap().to_string())),
                };
                let (n, lines) = quilltap_core::test_support::captured_with(|| {
                    writer
                        .memories()
                        .update_tier_bulk(
                            &s("characterId"),
                            &ids(),
                            &s("tier"),
                            quilltap_core::db::memories::TierExtra {
                                superseded_by_id: tri("supersededById"),
                                consolidated_at: tri("consolidatedAt"),
                            },
                        )
                        .expect("updateTierBulk")
                });
                (Value::from(n), lines)
            }
            "markConsidered" => {
                let (n, lines) = quilltap_core::test_support::captured_with(|| {
                    writer
                        .memories()
                        .mark_considered(&s("characterId"), &ids(), &s("when"))
                        .expect("markConsidered")
                });
                (Value::from(n), lines)
            }
            // L-6 (P4.D288): re-aimed at the repository verb v4's
            // `stampAccessed` calls.
            "updateAccessTimeBulk" => {
                let (n, lines) = quilltap_core::test_support::captured_with(|| {
                    writer
                        .memories()
                        .update_access_time_bulk(&s("characterId"), &ids())
                        .expect("updateAccessTimeBulk")
                });
                (Value::from(n), lines)
            }
            other => panic!("unknown tier op: {other}"),
        };
        assert_eq!(returned, t["returned"], "tier op {i} ({kind}): the answer");
        let want = want_lines(t);
        assert_eq!(lines, want, "tier op {i} ({kind}): the lines");
        tier_lines += want.len();
    }
    assert!(
        tier_lines >= 8,
        "the tier phase compared only {tier_lines} lines"
    );

    // === P4.D288: the keystone phase, replayed from the oracle's own op list ===
    let keystone_ops = oracle["keystone"]
        .as_array()
        .cloned()
        .expect("the oracle carries no `keystone` phase — regenerate it (P4.D288)");
    assert!(
        keystone_ops.len() >= 50,
        "the keystone phase shrank: {}",
        keystone_ops.len()
    );
    let mut keystone_lines = 0usize;
    for (i, t) in keystone_ops.iter().enumerate() {
        let (returned, lines) = replay_keystone_op(&writer, &t["op"], i);
        let kind = t["op"]["kind"].as_str().unwrap();
        assert_eq!(
            returned, t["returned"],
            "keystone op {i} ({kind}): the answer"
        );
        let want = want_lines(t);
        assert_eq!(lines, want, "keystone op {i} ({kind}): the lines");
        keystone_lines += want.len();
    }
    assert!(
        keystone_lines >= 25,
        "the keystone phase compared only {keystone_lines} lines"
    );

    let mut got = writer
        .dump_table_json("memories", "id")
        .expect("dump memories");

    normalize(&mut got, "rust");
    normalize(&mut oracle, "oracle");

    assert_eq!(got["table"], oracle["table"], "table name");
    assert_eq!(got["columns"], oracle["columns"], "column set / order");
    assert_eq!(
        got["rows"], oracle["rows"],
        "row state diverged\n  rust:   {}\n  oracle: {}",
        got["rows"], oracle["rows"]
    );

    let n = got["rows"].as_array().map(|a| a.len()).unwrap_or(0);
    assert!(n > 0, "dump looks empty");

    // [P4.161 — P4.155's R-E twin] v4's REAL `memories.create` over five
    // rows `MemorySchema` refuses (run oracle-side AFTER the op sequence; the
    // dump above is the "wrote nothing" proof): each message must be exactly
    // `parse_create_memory`'s over the same data and claimed id.
    let errors = oracle["errors"]
        .as_array()
        .expect("the oracle carries no `errors` — regenerate it (P4.161)");
    assert_eq!(errors.len(), 5, "the five createExpectError rows");
    for e in errors {
        let label = e["label"].as_str().unwrap_or("?");
        let want = e["message"].as_str();
        assert!(
            want.is_some(),
            "[{label}] v4 ACCEPTED a row it should refuse"
        );
        let got = quilltap_core::db::memories::parse_create_memory(
            &e["data"],
            e.get("id").and_then(Value::as_str),
        )
        .err();
        assert_eq!(
            got.as_deref(),
            want,
            "[{label}] the refusal's ZodError message"
        );
    }
    eprintln!(
        "OK: memories tier-2 matched oracle ({n} rows, {} refusals, {} tier ops / {tier_lines} lines, {} keystone ops / {keystone_lines} lines).",
        errors.len(),
        tier_ops.len(),
        keystone_ops.len()
    );
}

/// Replay one keystone op (P4.D288) on the writer: `(answer, lines)` in the
/// oracle's shapes — a refusal answers `{error}` with the bare message.
fn replay_keystone_op(writer: &Writer, op: &Value, i: usize) -> (Value, Vec<String>) {
    use quilltap_core::db::memories_read as mr;
    let kind = op["kind"].as_str().unwrap();
    let s = |k: &str| op[k].as_str().unwrap().to_string();
    let ids = |k: &str| -> Vec<String> {
        op[k]
            .as_array()
            .map(|a| a.iter().map(|v| v.as_str().unwrap().to_string()).collect())
            .unwrap_or_default()
    };
    let err = |e: quilltap_core::db::DbError| serde_json::json!({ "error": quilltap_core::db::fallback::error_text(&e) });
    let repo = writer.memories();
    let conn = writer.connection();
    let (returned, lines) = quilltap_core::test_support::captured_with(|| -> Value {
        match kind {
            "create" => {
                let opts = &op["options"];
                let id = opts["id"].as_str().unwrap();
                let data = quilltap_core::db::memories::parse_create_memory(&op["data"], Some(id))
                    .unwrap_or_else(|e| panic!("keystone op {i}: v4 accepted, v5 refused: {e}"));
                repo.create(
                    &data,
                    &CreateOptions {
                        id: id.to_string(),
                        created_at: opts["createdAt"].as_str().unwrap().to_string(),
                        updated_at: opts["updatedAt"].as_str().unwrap().to_string(),
                    },
                )
                .expect("keystone create");
                Value::Null
            }
            "plant" => {
                conn.execute_batch(op["sql"].as_str().unwrap())
                    .expect("plant");
                Value::Null
            }
            "incrementReinforcement" => {
                let by = op.get("by").and_then(Value::as_i64).unwrap_or(1);
                match repo.increment_reinforcement(&s("characterId"), &s("memoryId"), &s("at"), by)
                {
                    Ok(r) => reinforcement_json(r),
                    Err(e) => err(e),
                }
            }
            "addRelatedMemoryIds" => serde_json::json!(repo
                .add_related_memory_ids(&s("characterId"), &s("memoryId"), &ids("ids"))
                .expect("addRelatedMemoryIds")),
            "removeRelatedMemoryIds" => serde_json::json!(repo
                .remove_related_memory_ids(&s("characterId"), &s("memoryId"), &ids("ids"))
                .expect("removeRelatedMemoryIds")),
            "removeRelatedMemoryIdsEverywhere" => Value::from(
                repo.remove_related_memory_ids_everywhere(&ids("ids"))
                    .expect("removeRelatedMemoryIdsEverywhere"),
            ),
            "appendReinforcementFootnotes" => match repo
                .append_reinforcement_footnotes(&s("characterId"), &s("memoryId"), &ids("details"))
                .expect("appendReinforcementFootnotes")
            {
                Some(r) => serde_json::json!({ "content": r.content, "appended": r.appended }),
                None => Value::Null,
            },
            "countCapBreakdown" => mr::count_cap_breakdown(conn, &s("characterId"))
                .expect("countCapBreakdown")
                .to_json(),
            "findDanglingLinks" => {
                let scope = op.get("characterIds").map(|_| ids("characterIds"));
                mr::find_dangling_links(conn, scope.as_deref())
                    .expect("findDanglingLinks")
                    .to_json()
            }
            "findDigestsContainingMembers" => Value::Array(
                mr::find_digests_containing_members(conn, &ids("characterIds"), &ids("memberIds"))
                    .expect("findDigestsContainingMembers")
                    .into_iter()
                    .map(|d| {
                        serde_json::json!({
                            "id": d.id,
                            "characterId": d.character_id,
                            "consolidatedFrom": d.consolidated_from,
                        })
                    })
                    .collect(),
            ),
            "findIdsSupersededByAny" => Value::Array(
                mr::find_ids_superseded_by_any(conn, &ids("characterIds"), &ids("digestIds"))
                    .expect("findIdsSupersededByAny")
                    .into_iter()
                    .map(|(id, c)| serde_json::json!({ "id": id, "characterId": c }))
                    .collect(),
            ),
            "findDigestSubjectsByIds" => Value::Array(
                mr::find_digest_subjects_by_ids(conn, &ids("ids"))
                    .expect("findDigestSubjectsByIds")
                    .into_iter()
                    .map(|d| {
                        serde_json::json!({
                            "id": d.id,
                            "characterId": d.character_id,
                            "aboutCharacterId": d.about_character_id,
                        })
                    })
                    .collect(),
            ),
            "removeConsolidatedMembers" => Value::from(
                repo.remove_consolidated_members(&s("characterId"), &s("digestId"), &ids("ids"))
                    .expect("removeConsolidatedMembers"),
            ),
            "updateTierBulk" => {
                let extra = &op["extra"];
                let tri = |k: &str| match extra.get(k) {
                    None => None,
                    Some(Value::Null) => Some(None),
                    Some(v) => Some(Some(v.as_str().unwrap().to_string())),
                };
                Value::from(
                    repo.update_tier_bulk(
                        &s("characterId"),
                        &ids("ids"),
                        &s("tier"),
                        quilltap_core::db::memories::TierExtra {
                            superseded_by_id: tri("supersededById"),
                            consolidated_at: tri("consolidatedAt"),
                        },
                    )
                    .expect("updateTierBulk"),
                )
            }
            "findRecentByImportanceTier" => {
                let limits = &op["limits"];
                let limit = |k: &str, d: i64| limits.get(k).and_then(Value::as_i64).unwrap_or(d);
                let (high, medium, low) = mr::find_recent_by_importance_tier(
                    conn,
                    &s("characterId"),
                    limit("high", 20),
                    limit("medium", 10),
                    limit("low", 5),
                )
                .expect("findRecentByImportanceTier");
                let id_list = |rows: Vec<Value>| -> Vec<Value> {
                    rows.into_iter().map(|m| m["id"].clone()).collect()
                };
                serde_json::json!({ "high": id_list(high), "medium": id_list(medium), "low": id_list(low) })
            }
            "bulkDeleteUnsafe" => Value::from(
                repo.bulk_delete_unsafe(&s("characterId"), &ids("ids"))
                    .expect("bulkDeleteUnsafe"),
            ),
            other => panic!("unknown keystone op: {other}"),
        }
    });
    (returned, lines)
}
