//! P4.9E2A ANNOUNCER tier-3 (mocked-LLM) differential:
//! `services::announcer::character_voiced::generate_character_voiced_announcement`
//! vs v4's REAL `generateCharacterVoicedAnnouncement`, over a FRESH copy of the
//! committed `post-office-{main,mount}.db` fixture per case.
//!
//! The same canned completion is injected on both sides, keyed by the oracle's
//! RECORDED call (`provider|model|temperature|messages`) — so **the assembled
//! system prompt and user message ARE the diffed evidence**. A recording wrapper
//! sits in front of the canned provider so a prompt mismatch surfaces as a
//! readable diff instead of a bare lookup miss (memory note
//! `tier3-completion-oracle`).
//!
//! Then a **tier-2 diff on the writes**: the service "persists nothing", but the
//! Commonplace recall it runs bumps `lastAccessedAt` on the memories it returned.
//! The dump is clock-free (`bumped` = `lastAccessedAt` past the fixture's seed
//! timestamp) so it diffs with no normalization.
//!
//! Coverage is asserted by shape — every oracle case name must be driven here and
//! vice versa (`harness-corpus-shape-constants-rot`).
//!
//! Generate the oracle (Node 24, from the v4 checkout, **TZ=UTC** — see the .ts
//! header):
//!   … QT_ORACLE_OUT=/tmp/oracle-announcer-tier3.ndjson TZ=UTC npx jest -- announcer-tier3
//! Run:
//!   QT_ORACLE_ANNOUNCER_TIER3=/tmp/oracle-announcer-tier3.ndjson \
//!     cargo test -p quilltap-harness --test announcer_tier3_equivalence -- --nocapture

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use quilltap_core::api::chat_post_office::{
    chat_announcement_preview, AnnouncementPreviewDriver, AnnouncementPreviewRunner,
};
use quilltap_core::api::types::{ErrorKind, Response};
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::model::completion::{
    CannedCompletionProvider, CompletionError, CompletionMessage, CompletionParams,
    CompletionProvider, CompletionResponse, CompletionRole,
};
use quilltap_core::model::embedding::CannedEmbeddingProvider;
use quilltap_core::services::announcer::character_voiced::{
    generate_character_voiced_announcement, CharacterVoicedAnnouncementParams,
};
use quilltap_core::services::cheap_llm_exec::CheapLlmTaskExecutor;
use serde::Deserialize;
use serde_json::{json, Value};

const USER: &str = "e18e05bc-63e8-4539-8a85-719b7a508850";
const SEED_TS: &str = "2026-05-01T00:00:00.000Z";
const NOW_MS: i64 = 1_777_939_200_000; // 2026-05-05T00:00:00.000Z

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    test_pepper_base64: String,
}

fn spec_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/post-office-web.json")
}
fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../quilltap-web/tests/fixtures")
}
fn env_or_skip(key: &str) -> Option<String> {
    match std::env::var(key) {
        Ok(v) => Some(v),
        Err(_) => {
            eprintln!("SKIP: set {key} (see test header).");
            None
        }
    }
}

fn fresh_db(spec: &Spec, tag: &str) -> Db {
    let scratch = std::env::temp_dir().join(format!("qt-ann-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    let main = scratch.join("main.db");
    let mount = scratch.join("mount.db");
    std::fs::copy(fixtures_dir().join("post-office-main.db"), &main).unwrap();
    std::fs::copy(fixtures_dir().join("post-office-mount.db"), &mount).unwrap();
    Db::open(
        DbPaths {
            main,
            mount_index: Some(mount),
            llm_logs: None,
        },
        &spec.test_pepper_base64,
    )
    .expect("open db")
}

/// Wraps the canned provider and records each call the way the oracle's mock
/// records it, so a prompt drift is a readable diff rather than a lookup miss.
struct RecordingProvider {
    inner: CannedCompletionProvider,
    calls: Mutex<Vec<Value>>,
}

impl CompletionProvider for RecordingProvider {
    async fn send_message(
        &self,
        provider: &str,
        base_url: Option<&str>,
        params: &CompletionParams,
    ) -> Result<CompletionResponse, CompletionError> {
        let messages: Vec<Value> = params
            .messages
            .iter()
            .map(|m| json!({ "role": m.role.as_str(), "content": m.content }))
            .collect();
        let result = self.inner.send_message(provider, base_url, params).await;
        self.calls.lock().unwrap().push(json!({
            "provider": provider,
            "model": params.model,
            "temperature": params.temperature,
            "messages": messages,
            "response": result.as_ref().map(|r| r.content.clone()).unwrap_or_default(),
        }));
        result
    }
}

/// The memories table's access-bump evidence, clock-free — mirrors the oracle's
/// `readMemoryBumps`.
fn dump_memory_bumps(db: &Db) -> Value {
    db.read_main(|conn| {
        let mut stmt = conn.prepare(
            "SELECT id, characterId, \
             CASE WHEN lastAccessedAt IS NOT NULL AND lastAccessedAt > ?1 THEN 1 ELSE 0 END AS bumped \
             FROM memories ORDER BY id",
        )?;
        let rows = stmt
            .query_map(rusqlite::params![SEED_TS], |r| {
                Ok(json!({
                    "id": r.get::<_, String>(0)?,
                    "characterId": r.get::<_, Option<String>>(1)?,
                    "bumped": r.get::<_, i64>(2)?,
                }))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok::<_, quilltap_core::db::DbError>(Value::Array(rows))
    })
    .unwrap()
}

fn canon_numbers(v: &mut Value) {
    match v {
        Value::Number(n) => {
            if let Some(f) = n.as_f64() {
                if f.is_finite() && f.fract() == 0.0 && f.abs() < 9.007_199_254_740_992e15 {
                    *v = Value::Number((f as i64).into());
                }
            }
        }
        Value::Array(a) => a.iter_mut().for_each(canon_numbers),
        Value::Object(o) => o.iter_mut().for_each(|(_, x)| canon_numbers(x)),
        _ => {}
    }
}
fn sorted(v: &Value) -> Value {
    match v {
        Value::Array(a) => Value::Array(a.iter().map(sorted).collect()),
        Value::Object(o) => {
            let mut keys: Vec<&String> = o.keys().collect();
            keys.sort();
            let mut m = serde_json::Map::new();
            for k in keys {
                m.insert(k.clone(), sorted(&o[k]));
            }
            Value::Object(m)
        }
        _ => v.clone(),
    }
}
fn norm(v: &Value) -> String {
    let mut v = v.clone();
    canon_numbers(&mut v);
    serde_json::to_string_pretty(&sorted(&v)).unwrap()
}
fn first_diff(got: &str, want: &str) -> String {
    let g: Vec<&str> = got.lines().collect();
    let w: Vec<&str> = want.lines().collect();
    for i in 0..g.len().max(w.len()) {
        let gi = g.get(i).copied().unwrap_or("<none>");
        let wi = w.get(i).copied().unwrap_or("<none>");
        if gi != wi {
            let mut ctx = String::new();
            for j in i.saturating_sub(3)..i {
                ctx.push_str(&format!("   = {}\n", g.get(j).copied().unwrap_or("")));
            }
            ctx.push_str(&format!("  GOT : {gi}\n  WANT: {wi}\n"));
            return ctx;
        }
    }
    "(identical line-by-line)".to_string()
}

/// The oracle's per-case inputs, mirrored here (the oracle's `CaseSpec`).
struct Case {
    name: &'static str,
    /// Drive the dispatch handler over the [`AnnouncementPreviewRunner`] instead
    /// of the service directly — the route envelope arms.
    via_route: bool,
    chat_id: &'static str,
    character_id: &'static str,
    profile_id: &'static str,
    seed_markdown: &'static str,
}

const CONN: &str = "c0000001-0000-4000-8000-000000000001";
const CONN_B: &str = "c0000002-0000-4000-8000-000000000002";
const ARIA: &str = "a1000000-0000-4000-8000-000000000001";
const DORIAN: &str = "a1000000-0000-4000-8000-000000000004";
const CHAT: &str = "c1000000-0000-4000-8000-000000000001";
const MISSING_CHAT: &str = "99999999-9999-4999-8999-999999999999";

const CASES: &[Case] = &[
    Case {
        name: "rewrite_offscene_with_recall",
        via_route: false,
        chat_id: CHAT,
        character_id: DORIAN,
        profile_id: CONN,
        seed_markdown: "  Tell them the lamps on the eastern quay are out and I am seeing to it.  ",
    },
    Case {
        name: "rewrite_no_recall_hits",
        via_route: false,
        chat_id: CHAT,
        character_id: DORIAN,
        profile_id: CONN,
        seed_markdown: "Zzyzx qwertyuiop.",
    },
    Case {
        name: "rewrite_unknown_chat_no_roster",
        via_route: false,
        chat_id: MISSING_CHAT,
        character_id: DORIAN,
        profile_id: CONN,
        seed_markdown: "The lamps are out.",
    },
    Case {
        name: "rewrite_speaker_is_participant",
        via_route: false,
        chat_id: CHAT,
        character_id: ARIA,
        profile_id: CONN_B,
        seed_markdown: "The lamps are out on the eastern quay.",
    },
    Case {
        name: "rewrite_empty_completion",
        via_route: false,
        chat_id: CHAT,
        character_id: DORIAN,
        profile_id: CONN,
        seed_markdown: "The lamps are out.",
    },
    Case {
        name: "route_preview_ok",
        via_route: true,
        chat_id: CHAT,
        character_id: DORIAN,
        profile_id: CONN,
        seed_markdown: "Tell them the lamps on the eastern quay are out and I am seeing to it.",
    },
    Case {
        name: "route_preview_failure",
        via_route: true,
        chat_id: CHAT,
        character_id: DORIAN,
        profile_id: CONN,
        seed_markdown: "Tell them the lamps on the eastern quay are out and I am seeing to it.",
    },
];

/// The web-edge (status, body) for a Response — the routes-family mapping.
fn status_body(r: &Response) -> (u16, Value) {
    match r {
        Response::ChatPostOffice(v) => (200, v.clone()),
        Response::Error(e) => {
            let status = match e.kind {
                ErrorKind::BadRequest => 400,
                ErrorKind::Unauthorized => 401,
                ErrorKind::Forbidden => 403,
                ErrorKind::NotFound => 404,
                ErrorKind::Conflict => 409,
                ErrorKind::Unprocessable => 422,
                ErrorKind::Locked => 423,
                // The store-unavailable refusal (P4.23) — v4's deliberate
                // contextful 503 (context.ts:176-205).
                ErrorKind::Unavailable => 503,
                ErrorKind::Internal => 500,
            };
            (status, json!({ "error": e.message }))
        }
        other => (500, serde_json::to_value(other).unwrap()),
    }
}

#[test]
fn announcer_tier3_matches_oracle() {
    let Some(oracle_path) = env_or_skip("QT_ORACLE_ANNOUNCER_TIER3") else {
        return;
    };
    let spec: Spec = serde_json::from_str(&std::fs::read_to_string(spec_path()).unwrap()).unwrap();

    let mut oracle: std::collections::HashMap<String, Value> = std::collections::HashMap::new();
    for line in std::fs::read_to_string(&oracle_path)
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
    {
        let v: Value = serde_json::from_str(line).unwrap();
        oracle.insert(v["name"].as_str().unwrap().to_string(), v);
    }
    assert!(
        !oracle.is_empty(),
        "the oracle NDJSON is empty — regenerate it (an erroring builder leaves a stale file)"
    );

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let mut failed: Vec<String> = Vec::new();
    let mut driven: BTreeSet<String> = BTreeSet::new();

    for case in CASES {
        driven.insert(case.name.to_string());
        let Some(want) = oracle.get(case.name) else {
            failed.push(format!("{}_MISSING_FROM_ORACLE", case.name));
            continue;
        };

        // Build the canned provider from the oracle's RECORDED calls.
        let mut canned = CannedCompletionProvider::new();
        for call in want["canned"].as_array().expect("canned array") {
            let messages: Vec<CompletionMessage> = call["messages"]
                .as_array()
                .expect("messages array")
                .iter()
                .map(|m| CompletionMessage {
                    role: match m["role"].as_str().unwrap() {
                        "system" => CompletionRole::System,
                        "assistant" => CompletionRole::Assistant,
                        _ => CompletionRole::User,
                    },
                    content: m["content"].as_str().unwrap().to_string(),
                })
                .collect();
            canned = canned.with_response(
                call["provider"].as_str().unwrap(),
                call["model"].as_str().unwrap(),
                call["temperature"].as_f64(),
                &messages,
                call["response"].as_str().unwrap().to_string(),
                None,
            );
        }
        let provider = RecordingProvider {
            inner: canned,
            calls: Mutex::new(Vec::new()),
        };
        // The vector store is empty on both sides, so the search falls through to
        // the text path either way; the seed vector keeps the embed step canned.
        let embedding = CannedEmbeddingProvider::new()
            .with_vector(case.seed_markdown.to_string(), vec![1.0, 0.0, 0.0, 0.0]);
        // No logging: the fixture family carries no llm-logs partition, and the
        // oracle mocks `logLLMCall` away to match.
        let executor = CheapLlmTaskExecutor::new();

        let db = fresh_db(&spec, case.name);
        let character = db
            .read_main(|main| {
                db.read_mount_index(|mount| {
                    quilltap_core::db::characters_read::find_by_id(main, mount, case.character_id)
                })
            })
            .unwrap()
            .expect("fixture character");
        let profile = db
            .read_main(|main| {
                quilltap_core::db::connection_profiles::find_by_id(main, case.profile_id)
            })
            .unwrap()
            .expect("fixture profile");

        // The route arms go through the dispatch handler over the ready-made
        // runner — the same seam the host will construct — so the response
        // envelope and the `badRequest(result.error)` arm are DIFFED, not
        // asserted. `provider` moves into the runner there, so the recorded
        // calls come back out of the shared Arc.
        let provider = Arc::new(provider);
        if case.via_route {
            let driver: Arc<dyn AnnouncementPreviewDriver> = Arc::new(AnnouncementPreviewRunner {
                db: db.clone(),
                completion: provider.clone(),
                embedding,
                executor: Arc::new(executor),
                now_ms: Some(NOW_MS as f64),
            });
            let resp = rt.block_on(chat_announcement_preview(
                &db,
                Some(&driver),
                USER,
                case.chat_id,
                case.seed_markdown,
                case.character_id,
                case.profile_id,
                None,
            ));
            let (status, body) = status_body(&resp);
            let want_status = want["status"].as_u64().unwrap() as u16;
            if status != want_status {
                eprintln!("[{}] STATUS {status} != {want_status}", case.name);
                failed.push(format!("{}_status", case.name));
            }
            if norm(&body) != norm(&want["body"]) {
                eprintln!(
                    "[{}] BODY MISMATCH:\n{}",
                    case.name,
                    first_diff(&norm(&body), &norm(&want["body"]))
                );
                failed.push(format!("{}_body", case.name));
            } else {
                eprintln!("[{}] body OK.", case.name);
            }
        } else {
            let result = rt.block_on(generate_character_voiced_announcement(
                &db,
                provider.as_ref(),
                &embedding,
                &executor,
                &CharacterVoicedAnnouncementParams {
                    chat_id: case.chat_id,
                    character: &character,
                    profile: &profile,
                    seed_markdown: case.seed_markdown,
                    system_prompt_id: None,
                    user_id: USER,
                    now_ms: NOW_MS as f64,
                },
            ));

            let got_result = json!({
                "success": result.success,
                "proposedMarkdown": result.proposed_markdown,
                "error": result.error,
            });
            // v4's result object omits `error` on success (`{success,
            // proposedMarkdown}`); compare over the union so an absent key and an
            // explicit null agree.
            let mut want_result = want["result"].clone();
            if want_result.get("error").is_none() {
                want_result
                    .as_object_mut()
                    .unwrap()
                    .insert("error".into(), Value::Null);
            }
            if norm(&got_result) != norm(&want_result) {
                eprintln!(
                    "[{}] RESULT MISMATCH:\n{}",
                    case.name,
                    first_diff(&norm(&got_result), &norm(&want_result))
                );
                failed.push(format!("{}_result", case.name));
            } else {
                eprintln!("[{}] result OK.", case.name);
            }
        }

        // The assembled prompt — the substance of this tier.
        let got_calls = Value::Array(provider.calls.lock().unwrap().clone());
        if norm(&got_calls) != norm(&want["canned"]) {
            eprintln!(
                "[{} prompt] MISMATCH:\n{}",
                case.name,
                first_diff(&norm(&got_calls), &norm(&want["canned"]))
            );
            failed.push(format!("{}_prompt", case.name));
        } else {
            eprintln!("[{} prompt] OK.", case.name);
        }

        // The tier-2 diff on the one thing this "persists nothing" path writes.
        let got_tables = json!({ "memories": dump_memory_bumps(&db) });
        if norm(&got_tables) != norm(&want["tables"]) {
            eprintln!(
                "[{} tables] MISMATCH:\n{}",
                case.name,
                first_diff(&norm(&got_tables), &norm(&want["tables"]))
            );
            failed.push(format!("{}_tables", case.name));
        } else {
            eprintln!("[{} tables] OK.", case.name);
        }
    }

    let recorded: BTreeSet<String> = oracle.keys().cloned().collect();
    let missing: Vec<&String> = recorded.difference(&driven).collect();
    let extra: Vec<&String> = driven.difference(&recorded).collect();
    assert!(
        missing.is_empty() && extra.is_empty(),
        "case coverage drifted — oracle-only: {missing:?}, rust-only: {extra:?}"
    );
    eprintln!("announcer-tier3: {} cases driven.", driven.len());

    assert!(failed.is_empty(), "announcer-tier3 FAILED: {failed:?}");
}
