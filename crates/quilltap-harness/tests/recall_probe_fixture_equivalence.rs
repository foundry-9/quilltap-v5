//! Tier-3 differential (P4.174 / C-3 — P4.D291 Tier 2 item 24): v5's
//! `services::recall::search::search_recall` over v4's OWN recall probe set —
//! the whole-pipeline regression over a realistic corpus (158 memories with
//! 64-dimension vectors, nine turns A, B, C, F1, F2, M, O2, O3, O4) — against
//! v4's REAL `searchRecall` run through v4's own `probeWorld` seams
//! (`harness/oracle/cases/recall-probe-fixture.test.ts`).
//!
//! The probe set is read AT TEST TIME from `$QT_V4_ROOT/__tests__/fixtures/
//! recall-probes/probe-set.json` — never vendored (SKIP with the reason when
//! unset). Its non-RFC-UUID ids (`…-ax0000000001` memories, `chat-older` …)
//! are remapped to deterministic RFC UUIDs, ORDER-PRESERVING (the sort a tie
//! falls to is unchanged), because v5's raw reads drop non-UUID rows (memory
//! note `memory-raw-reads-drop-non-uuid-fixtures`); the vectors are planted as
//! Float32 LE blobs on a freshly provisioned scratch instance's
//! `memories.embedding` (v5 has no process cache — every search loads them);
//! the 15-entry embedding table is the query's canned provider (the vectors the
//! turn's query and probes carry). The comparand is v4's CURRENT arithmetic,
//! not the fixture's own pre-Phase-E `expected` rows: per turn the candidates'
//! ids in order, `score` / `salience` / `blendedAfter` at 1e-12, `fired`,
//! `usedEmbedding` — plus the gold-at-head total against the fixture's own
//! `goldAtHead`.
//!
//! Generate the oracle (Node 24, from the v4 checkout pinned at the baseline):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   TMPO=/tmp/qt-recall-probe-oracle
//!   rm -rf "$TMPO"; mkdir -p "$TMPO/cases"
//!   cp "$V5W/harness/oracle/cases/recall-probe-fixture.test.ts" "$TMPO/cases/"
//!   cd ~/source/quilltap-server
//!   TZ=UTC QT_ORACLE_OUT=/tmp/oracle-recall-probe.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=180000 \
//!       --roots "$PWD" --roots "$TMPO/cases" -- recall-probe-fixture
//! Run:
//!   QT_V4_ROOT=<the pin> QT_ORACLE_RECALL_PROBE=/tmp/oracle-recall-probe.ndjson \
//!     cargo test -p quilltap-harness --test recall_probe_fixture_equivalence -- --nocapture

use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::PathBuf;

use quilltap_core::db::runtime::Db;
use quilltap_core::model::embedding::EmbeddingResult;
use quilltap_core::recall_tags::{ContextTag, ScopePolicy, TemporalTag, TimeWindow};
use quilltap_core::services::recall::context::RecallContextInput;
use quilltap_core::services::recall::query::{EmbeddedText, RecallQuery};
use quilltap_core::services::recall::search::{search_recall, RecallSearchOptions};
use quilltap_core::services::recall::types::RecallTrace;
use serde_json::Value;

const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
const EPS: f64 = 1e-12;
const USER: &str = "00000000-0000-4000-8000-0000000000aa";

fn is_rfc_uuid(s: &str) -> bool {
    let b = s.as_bytes();
    if b.len() != 36 {
        return false;
    }
    for (i, c) in b.iter().enumerate() {
        match i {
            8 | 13 | 18 | 23 => {
                if *c != b'-' {
                    return false;
                }
            }
            14 => {
                if !(b'1'..=b'8').contains(c) {
                    return false;
                }
            }
            19 => {
                if !matches!(c, b'8' | b'9' | b'a' | b'b' | b'A' | b'B') {
                    return false;
                }
            }
            _ => {
                if !c.is_ascii_hexdigit() {
                    return false;
                }
            }
        }
    }
    true
}

/// Every id string the fixture carries that v5 would not read as a UUID.
fn collect_ids(f: &Value) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut add = |s: &str| {
        if !is_rfc_uuid(s) {
            out.insert(s.to_string());
        }
    };
    for m in f["memories"].as_array().unwrap() {
        add(m["id"].as_str().unwrap());
        if let Some(c) = m["chatId"].as_str() {
            add(c);
        }
        for r in m["relatedMemoryIds"].as_array().unwrap() {
            add(r.as_str().unwrap());
        }
    }
    for t in f["turns"].as_array().unwrap() {
        for g in t["gold"].as_array().unwrap() {
            add(g.as_str().unwrap());
        }
        let rc = &t["recallContext"];
        add(rc["currentChatId"].as_str().unwrap());
        for w in rc["recentlyWhisperedIds"].as_array().unwrap() {
            add(w.as_str().unwrap());
        }
    }
    out
}

/// Order-preserving remap: sorted originals get ascending RFC UUIDs.
struct IdMap {
    forward: HashMap<String, String>,
    back: HashMap<String, String>,
}

impl IdMap {
    fn new(ids: BTreeSet<String>) -> Self {
        let mut forward = HashMap::new();
        let mut back = HashMap::new();
        for (i, id) in ids.into_iter().enumerate() {
            let uuid = format!("e0000000-0000-4000-8000-{i:012x}");
            back.insert(uuid.clone(), id.clone());
            forward.insert(id, uuid);
        }
        IdMap { forward, back }
    }
    fn to_v5(&self, s: &str) -> String {
        self.forward
            .get(s)
            .cloned()
            .unwrap_or_else(|| s.to_string())
    }
    fn to_v4(&self, s: &str) -> String {
        self.back.get(s).cloned().unwrap_or_else(|| s.to_string())
    }
}

fn strings(v: &Value) -> Vec<String> {
    v.as_array()
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn vec_f32(v: &Value) -> Vec<f32> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_f64().unwrap() as f32)
        .collect()
}

fn blob(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|x| x.to_le_bytes()).collect()
}

fn embedding(v: &Value) -> EmbeddingResult {
    let e = vec_f32(v);
    EmbeddingResult {
        dimensions: e.len(),
        embedding: e,
        model: "probe".to_string(),
        provider: "OPENAI".to_string(),
    }
}

fn feq(got: f64, want: &Value, what: &str) {
    let w = want
        .as_f64()
        .unwrap_or_else(|| panic!("{what}: oracle not a number: {want}"));
    assert!((got - w).abs() <= EPS, "{what}: rust={got} oracle={w}");
}

async fn plant(fixture: &Value, ids: &IdMap) -> (tempfile::TempDir, Db) {
    let dir = tempfile::tempdir().unwrap();
    quilltap_core::services::provisioning::provision_fresh_instance(dir.path(), PEPPER).unwrap();
    let db = Db::open_main(dir.path().join("quilltap.db"), PEPPER).unwrap();
    let memories = fixture["memories"].as_array().unwrap().clone();
    let chats: BTreeSet<String> = memories
        .iter()
        .filter_map(|m| m["chatId"].as_str().map(|c| ids.to_v5(c)))
        .collect();
    let rows: Vec<Vec<rusqlite::types::Value>> = memories
        .iter()
        .map(|m| {
            use rusqlite::types::Value as V;
            let text = |k: &str| match m[k].as_str() {
                Some(s) => V::Text(s.to_string()),
                None => V::Null,
            };
            let id_text = |k: &str| match m[k].as_str() {
                Some(s) => V::Text(ids.to_v5(s)),
                None => V::Null,
            };
            let related: Vec<String> = strings(&m["relatedMemoryIds"])
                .iter()
                .map(|r| ids.to_v5(r))
                .collect();
            vec![
                V::Text(ids.to_v5(m["id"].as_str().unwrap())),
                text("characterId"),
                id_text("aboutCharacterId"),
                id_text("chatId"),
                text("projectId"),
                text("content"),
                text("summary"),
                V::Text(serde_json::to_string(&m["keywords"]).unwrap()),
                V::Text(serde_json::to_string(&m["tags"]).unwrap()),
                V::Real(m["importance"].as_f64().unwrap()),
                V::Blob(blob(&vec_f32(&m["embedding"]))),
                text("source"),
                text("occurredAt"),
                text("narrativeTime"),
                text("kind"),
                text("lastAccessedAt"),
                V::Real(m["reinforcementCount"].as_f64().unwrap()),
                text("lastReinforcedAt"),
                V::Text(serde_json::to_string(&related).unwrap()),
                V::Real(m["reinforcedImportance"].as_f64().unwrap()),
                text("tier"),
                text("createdAt"),
                text("updatedAt"),
            ]
        })
        .collect();
    let character = fixture["characterId"].as_str().unwrap().to_string();
    db.write(move |w| {
        let c = w.main().connection();
        for chat in &chats {
            c.execute(
                "INSERT INTO chats (id, userId, title, createdAt, updatedAt) VALUES \
                 (?1, ?2, 't', '2026-01-01T00:00:00.000Z', '2026-01-01T00:00:00.000Z')",
                rusqlite::params![chat, USER],
            )?;
        }
        let _ = &character;
        for row in &rows {
            c.execute(
                "INSERT INTO memories (id, characterId, aboutCharacterId, chatId, projectId, \
                 content, summary, keywords, tags, importance, embedding, source, occurredAt, \
                 narrativeTime, kind, lastAccessedAt, reinforcementCount, lastReinforcedAt, \
                 relatedMemoryIds, reinforcedImportance, tier, createdAt, updatedAt) VALUES \
                 (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22,?23)",
                rusqlite::params_from_iter(row.iter()),
            )?;
        }
        Ok(())
    })
    .await
    .unwrap();
    (dir, db)
}

fn context(rc: &Value, ids: &IdMap) -> RecallContextInput {
    RecallContextInput {
        current_project_id: rc["currentProjectId"].as_str().map(str::to_string),
        scope_policy: if rc["scopePolicy"] == "exclude" {
            ScopePolicy::Exclude
        } else {
            ScopePolicy::DownWeight
        },
        present_about_character_ids: strings(&rc["presentAboutCharacterIds"]),
        turn_context: rc["turnContext"].as_str().and_then(ContextTag::from_kw),
        turn_temporal: rc["turnTemporal"].as_str().and_then(TemporalTag::from_kw),
        turn_retrospective: rc["turnRetrospective"] == true,
        occurred_within: None,
        expand_related: rc["expandRelated"] == true,
        recently_whispered_ids: Some(
            strings(&rc["recentlyWhisperedIds"])
                .iter()
                .map(|w| ids.to_v5(w))
                .collect::<HashSet<_>>(),
        ),
        current_chat_id: rc["currentChatId"].as_str().map(|c| ids.to_v5(c)),
        now_ms: rc["nowMs"].as_f64(),
        tuning: None,
        present_participant_names: strings(&rc["presentParticipantNames"]),
    }
}

#[tokio::test(flavor = "current_thread")]
async fn recall_probe_fixture_matches_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_RECALL_PROBE") else {
        eprintln!("SKIP: QT_ORACLE_RECALL_PROBE not set (see header).");
        return;
    };
    let Ok(v4_root) = std::env::var("QT_V4_ROOT") else {
        eprintln!(
            "SKIP: QT_V4_ROOT not set — v4's probe set is read at test time, never vendored."
        );
        return;
    };
    let fixture: Value = serde_json::from_str(
        &std::fs::read_to_string(
            PathBuf::from(&v4_root).join("__tests__/fixtures/recall-probes/probe-set.json"),
        )
        .expect("read v4's probe set"),
    )
    .unwrap();
    let oracle: Vec<Value> = std::fs::read_to_string(&oracle_path)
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let turns = fixture["turns"].as_array().unwrap();
    assert_eq!(oracle.len(), turns.len(), "oracle/turn count — regenerate");

    let ids = IdMap::new(collect_ids(&fixture));
    let (_dir, db) = plant(&fixture, &ids).await;
    let embeddings = fixture["embeddings"].as_object().unwrap();
    let character = fixture["characterId"].as_str().unwrap();

    let mut gold_at_head = 0usize;
    let mut compared = 0usize;
    for (turn, want) in turns.iter().zip(&oracle) {
        let slot = turn["slot"].as_str().unwrap();
        assert_eq!(want["slot"], slot, "turn order");
        let embed_of = |text: &str| embedding(&embeddings[text]);
        let query = RecallQuery {
            text: turn["query"].as_str().unwrap().to_string(),
            embedding: Some(embed_of(turn["query"].as_str().unwrap())),
            probes: strings(&turn["extraProbes"])
                .into_iter()
                .map(|t| EmbeddedText {
                    embedding: Some(embed_of(&t)),
                    text: t,
                })
                .collect(),
            signals: None,
            anchors: strings(&turn["entityAnchors"]),
            window: turn["occurredWithin"].as_object().map(|w| TimeWindow {
                from: w["from"].as_str().unwrap().to_string(),
                to: w["to"].as_str().unwrap().to_string(),
            }),
            retrospective: turn["recallContext"]["turnRetrospective"] == true,
            embedding_error: None,
        };
        let options = RecallSearchOptions {
            limit: turn["limit"].as_u64().map(|n| n as usize),
            min_importance: turn["minImportance"].as_f64(),
            head_size: turn["headSize"].as_u64().map(|n| n as usize),
            now_ms: turn["weightClockMs"].as_f64().unwrap(),
            recall_context: Some(context(&turn["recallContext"], &ids)),
            ..Default::default()
        };
        let mut trace: RecallTrace = Vec::new();
        let result = search_recall(&db, character, &query, &options, &mut trace, None)
            .await
            .unwrap_or_else(|e| panic!("slot {slot}: search failed: {e}"));

        let got_ids: Vec<String> = result
            .candidates
            .iter()
            .map(|c| ids.to_v4(c.id()))
            .collect();
        let want_rows = want["candidates"].as_array().unwrap();
        let want_ids: Vec<String> = want_rows
            .iter()
            .map(|r| r["id"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(got_ids, want_ids, "slot {slot}: candidate ids / order");
        for (c, w) in result.candidates.iter().zip(want_rows) {
            let what = format!("slot {slot} {}", ids.to_v4(c.id()));
            feq(c.score, &w["score"], &format!("{what} score"));
            feq(c.salience, &w["salience"], &format!("{what} salience"));
            assert_eq!(
                c.used_embedding,
                w["usedEmbedding"] == true,
                "{what} usedEmbedding"
            );
            match (&c.recall_adjustment, &w["blendedAfter"]) {
                (None, Value::Null) => {}
                (Some(a), v) if !v.is_null() => {
                    feq(a.blended_after, v, &format!("{what} blendedAfter"))
                }
                (g, v) => panic!("{what}: blendedAfter rust={g:?} oracle={v}"),
            }
            let fired = c
                .recall_adjustment
                .as_ref()
                .map(|a| a.fired.clone())
                .unwrap_or_default();
            assert_eq!(fired, strings(&w["fired"]), "{what} fired");
            compared += 1;
        }
        let head: HashSet<&String> = got_ids
            .iter()
            .take(turn["headSize"].as_u64().unwrap() as usize)
            .collect();
        gold_at_head += strings(&turn["gold"])
            .iter()
            .filter(|g| head.contains(g))
            .count();
    }
    assert_eq!(
        gold_at_head as u64,
        fixture["goldAtHead"].as_u64().unwrap(),
        "gold-at-head total (v4's own fixture total)"
    );
    assert!(compared > 0, "no candidate was compared");
    eprintln!(
        "recall_probe_fixture: {} turns, {compared} candidates compared, gold-at-head {gold_at_head}",
        turns.len()
    );
}
