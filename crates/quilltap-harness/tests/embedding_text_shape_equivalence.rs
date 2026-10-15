//! Tier-3 differential — P4.D292: WHICH TEXT the EMBEDDING_GENERATE memory
//! branch hands the embedding seam (v4 `c2795733f` bug 204 + `4ae0c4ca6`, at
//! the `83334b9a5` pin): the canonical ANCHORED text in the memory-calendar
//! zone as `text` (what a neural profile embeds), the anchor-free LEXICAL text
//! as `lexical_text` (what BUILTIN embeds instead), `priority` background; the
//! oversize / blank guard on the ANCHORED text; the row write through the ONE
//! index writer (`memories.embedding` only); the `Built memory embedding text`
//! DEBUG (UTF-16 `textLength` — a non-BMP summary) and the success INFO / the
//! missing-memory WARN with v4's `context` / `jobId` keys.
//!
//! v4 runs its REAL `handleEmbeddingGenerate` with only `generateEmbeddingForUser`
//! replaced by a recorder; v5 runs `handle_embedding_generate` with a
//! recording [`EmbeddingProvider`] that answers the same fixed vector and
//! RECORDS `lexical_text`. The memory calendar is the INSTANCE zone
//! (`Pacific/Auckland`, planted in `chat_settings`), read through each side's
//! real refresh. That the BUILTIN service embeds the lexical text it is handed
//! is `embedding_provider_tier3`'s rows.
//!
//! Compared per job: the provider calls (every field), the captured lines
//! (the three this lane owns, whole; the sibling `[MemoryCalendar]` / guard
//! lines by TAG only, §R.5 — `Upserted memory vectors` fires on the writer
//! thread, out of this rig's sight) and the outcome;
//! then the `memories` vectors (hex) and the `embedding_status` rows.
//!
//! Generate the oracle (Node 24, from the round's pin):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   rm -rf /tmp/qt-ets-oracle /tmp/oracle-embedding-text-shape.ndjson
//!   mkdir -p /tmp/qt-ets-oracle/cases /tmp/qt-ets-oracle/db
//!   cp $V5W/harness/oracle/cases/embedding-text-shape.test.ts /tmp/qt-ets-oracle/cases/
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_ETS_DIR=/tmp/qt-ets-oracle/db QT_ORACLE_OUT=/tmp/oracle-embedding-text-shape.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=180000 \
//!     --roots "$PWD" --roots /tmp/qt-ets-oracle/cases -- embedding-text-shape
//! Run:
//!   cd $V5W
//!   QT_ORACLE_EMBEDDING_TEXT_SHAPE=/tmp/oracle-embedding-text-shape.ndjson \
//!   QT_FIXTURE_ETS_DIR=/tmp/qt-ets-oracle/db \
//!     cargo test -p quilltap-harness --test embedding_text_shape_equivalence

use std::sync::Mutex;

use quilltap_core::db::runtime::Db;
use quilltap_core::model::embedding::{
    EmbeddingCallOptions, EmbeddingError, EmbeddingPriority, EmbeddingProvider, EmbeddingResult,
};
use quilltap_core::services::embedding_generate_job::{
    handle_embedding_generate, EmbeddingGenerateContext, EmbeddingGeneratePayload,
};
use quilltap_core::test_support::global_capture;
use serde_json::{json, Value};

const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
const USER: &str = "e2920000-0000-4000-8000-0000000000aa";
const CHAR: &str = "e2920000-0000-4000-8000-0000000000c1";
const PROFILE: &str = "e2920000-0000-4000-8000-0000000000e2";
const VECTOR: [f32; 4] = [1.0, 0.0, 0.0, 0.0];

/// A recording double: answers the fixed vector, records every call whole.
#[derive(Default)]
struct Recording {
    calls: Mutex<Vec<Value>>,
}

impl EmbeddingProvider for Recording {
    fn generate_embedding_for_user(
        &self,
        text: &str,
        user_id: &str,
        profile_id: Option<&str>,
        opts: EmbeddingCallOptions,
    ) -> impl std::future::Future<Output = Result<EmbeddingResult, EmbeddingError>> + Send {
        self.calls.lock().unwrap().push(json!({
            "text": text,
            "userId": user_id,
            "profileId": profile_id,
            "priority": match opts.priority {
                EmbeddingPriority::Background => "background",
                EmbeddingPriority::Interactive => "interactive",
            },
            "lexicalText": opts.lexical_text,
        }));
        std::future::ready(Ok(EmbeddingResult {
            embedding: VECTOR.to_vec(),
            model: "canned".into(),
            dimensions: VECTOR.len(),
            provider: "canned".into(),
        }))
    }
}

const OWNED: [&str; 3] = [
    "[EmbeddingGenerate] Built memory embedding text",
    "[EmbeddingGenerate] Embedding generated successfully",
    "[EmbeddingGenerate] Memory not found",
];

fn want_lines(lines: &Value) -> Vec<String> {
    lines
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
                    other => out.push_str(&format!(" {k}={other}")),
                }
            }
            out
        })
        .collect()
}

/// v5's owned lines minus their target, and the sibling lines' TAGS.
fn split_got(lines: Vec<String>) -> (Vec<String>, Vec<String>) {
    let mut owned = Vec::new();
    let mut tagged = Vec::new();
    for l in lines {
        let mut parts = l.splitn(3, ' ');
        let level = parts.next().unwrap().to_string();
        let _target = parts.next();
        let rest = parts.next().unwrap_or_default().to_string();
        if OWNED.iter().any(|m| rest.starts_with(m)) {
            owned.push(format!("{level} {rest}"));
        } else if let Some(tag) = [
            "[MemoryCalendar]",
            "[EmbeddingGenerate]",
            "Upserted memory vectors",
        ]
        .iter()
        .find(|t| rest.starts_with(**t))
        {
            tagged.push(tag.to_string());
        }
    }
    (owned, tagged)
}

fn tags_of(messages: &Value) -> Vec<String> {
    messages
        .as_array()
        .unwrap()
        .iter()
        .map(|m| {
            let m = m.as_str().unwrap();
            [
                "[MemoryCalendar]",
                "[EmbeddingGenerate]",
                "Upserted memory vectors",
            ]
            .iter()
            .find(|t| m.starts_with(**t))
            .unwrap()
            .to_string()
        })
        .collect()
}

fn end_state(db: &Db) -> Value {
    db.read_main(|c| {
        let mut stmt = c.prepare("SELECT id, hex(embedding) FROM memories ORDER BY id")?;
        let memories = stmt
            .query_map([], |r| {
                Ok(json!({ "id": r.get::<_, String>(0)?, "embedding": r.get::<_, String>(1)? }))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let mut stmt = c.prepare(
            "SELECT entityType, entityId, profileId, status, error FROM embedding_status ORDER BY entityId",
        )?;
        let status = stmt
            .query_map([], |r| {
                Ok(json!({
                    "entityType": r.get::<_, String>(0)?,
                    "entityId": r.get::<_, String>(1)?,
                    "profileId": r.get::<_, String>(2)?,
                    "status": r.get::<_, String>(3)?,
                    "error": r.get::<_, Option<String>>(4)?,
                }))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(json!({ "memories": memories, "status": status }))
    })
    .expect("end state")
}

#[tokio::test]
async fn the_memory_branch_hands_v4s_texts_to_the_seam() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_EMBEDDING_TEXT_SHAPE") else {
        eprintln!("SKIP: set QT_ORACLE_EMBEDDING_TEXT_SHAPE to the oracle NDJSON (see header).");
        return;
    };
    let Ok(dir) = std::env::var("QT_FIXTURE_ETS_DIR") else {
        eprintln!("SKIP: set QT_FIXTURE_ETS_DIR to the case's dir (see header).");
        return;
    };
    let oracle: Vec<Value> = std::fs::read_to_string(&oracle_path)
        .unwrap_or_else(|e| panic!("read oracle: {e}"))
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("oracle line"))
        .collect();
    assert!(
        oracle.len() >= 7,
        "the oracle carries six jobs and the end state"
    );
    global_capture::install();
    quilltap_core::services::memory_time_zone::reset_memory_time_zone_cache_for_tests();
    let scratch = tempfile::tempdir().unwrap();
    let work = scratch.path().join("main.db");
    std::fs::copy(std::path::Path::new(&dir).join("base.db"), &work).expect("copy base.db");
    let db = Db::open_main(&work, PEPPER).expect("open");

    let recording = Recording::default();
    let mut failures = Vec::new();
    for job in oracle.iter().filter(|l| l["kind"] == "job") {
        let job_id = job["jobId"].as_str().unwrap();
        let payload = EmbeddingGeneratePayload::from_json(&json!({
            "entityType": "MEMORY",
            "entityId": job["entityId"],
            "characterId": CHAR,
            "profileId": PROFILE,
        }));
        recording.calls.lock().unwrap().clear();
        let (outcome, lines) = global_capture::capture_async(handle_embedding_generate(
            &db,
            &recording,
            USER,
            &payload,
            EmbeddingGenerateContext {
                job_id,
                // v4's process zone under jest is UTC; the instance zone wins here.
                host_zone_fallback: "UTC",
                now_ms: 1_800_000_000_000.0,
            },
        ))
        .await;
        if outcome.err().as_deref() != job["error"].as_str() {
            failures.push(format!("{job_id}: outcome diverged (v4 {})", job["error"]));
        }
        let calls = Value::Array(recording.calls.lock().unwrap().clone());
        if serde_json::to_string(&calls).unwrap() != serde_json::to_string(&job["calls"]).unwrap() {
            failures.push(format!(
                "{job_id}: calls\n  v5 {calls}\n  v4 {}",
                job["calls"]
            ));
        }
        let (owned, tagged) = split_got(lines);
        let want = want_lines(&job["lines"]);
        if owned != want {
            failures.push(format!("{job_id}: lines\n  v5 {owned:#?}\n  v4 {want:#?}"));
        }
        // `Upserted memory vectors` is P4.D288's line, emitted INSIDE the writer
        // closure (C1-3's `memory_index::upsert(conn, …)` runs on the writer's
        // connection), so this caller-thread rig cannot see it (memory note
        // `capture-rig-writer-thread`); its presence is P4.D288's to pin.
        let want_tags: Vec<String> = tags_of(&job["tagged"])
            .into_iter()
            .filter(|t| t != "Upserted memory vectors")
            .collect();
        for tag in &want_tags {
            if !tagged.contains(tag) {
                failures.push(format!(
                    "{job_id}: v4's sibling tag {tag} absent from v5 ({tagged:?})"
                ));
            }
        }
    }
    let end = oracle
        .iter()
        .find(|l| l["kind"] == "end")
        .expect("end state");
    let got = end_state(&db);
    for key in ["memories", "status"] {
        if got[key] != end[key] {
            failures.push(format!("{key}\n  v5 {}\n  v4 {}", got[key], end[key]));
        }
    }
    assert!(
        failures.is_empty(),
        "{} divergences:\n{}",
        failures.len(),
        failures.join("\n\n")
    );
    eprintln!("OK: the memory branch's texts, lines and writes matched v4.");
}
