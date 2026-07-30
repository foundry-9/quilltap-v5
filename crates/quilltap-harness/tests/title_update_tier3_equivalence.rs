//! P4.6ao unit-3 TIER-3 (mocked-LLM) differential for the `TITLE_UPDATE` job
//! handler: `services::title_update_job::handle_title_update` vs v4's REAL
//! `handleTitleUpdate`. Both sides run over a FRESH copy of the committed
//! `cost-background-{main,mount}.db` fixture per case, with the SAME canned
//! cheap-LLM reply injected at the model boundary, then diff the writes tier-2
//! style: the chat row's title + checkpoint cursor + `updatedAt`, the
//! TITLE_GENERATION system events, and the `background_jobs` rows.
//!
//! The canned reply is keyed by the SYSTEM prompt on both sides, which also
//! proves the right evaluator prompt was built (the literary
//! `CHAT_TITLE_CONSIDERATION_PROMPT` vs the practical help one).
//!
//! Generate the oracle (Node 24, from the v4 checkout — see the .ts header):
//!   … QT_ORACLE_OUT=/tmp/oracle-title-update.ndjson npx jest -- title-update-tier3
//! Run:
//!   QT_ORACLE_TITLE_UPDATE=/tmp/oracle-title-update.ndjson \
//!     cargo test -p quilltap-harness --test title_update_tier3_equivalence -- --nocapture

use std::collections::HashMap;
use std::path::PathBuf;

use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::model::completion::{
    CompletionError, CompletionParams, CompletionProvider, CompletionResponse, CompletionUsage,
};
use quilltap_core::services::cheap_llm_exec::CheapLlmTaskExecutor;
use quilltap_core::services::cost_estimation::NoMessageCost;
use quilltap_core::services::title_update_job::{handle_title_update, TitleUpdatePayload};
use serde::Deserialize;
use serde_json::Value;

const TEST_PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct CannedTitle {
    content: String,
    prompt_tokens: i64,
    completion_tokens: i64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    frozen_now_ms: i64,
    user_enabled_id: String,
    user_disabled_id: String,
    connection_profile_id: String,
    chat_title_id: String,
    chat_help_id: String,
    chat_autonomous_id: String,
    chat_renamed_id: String,
    chat_regen_id: String,
    missing_id: String,
    canned_titles: HashMap<String, CannedTitle>,
}

fn spec_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/cost-background-web.json")
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

fn fresh_db(tag: &str) -> Db {
    let scratch = std::env::temp_dir().join(format!("qt-tu-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    let main = scratch.join("main.db");
    let mount = scratch.join("mount.db");
    std::fs::copy(fixtures_dir().join("cost-background-main.db"), &main).unwrap();
    std::fs::copy(fixtures_dir().join("cost-background-mount.db"), &mount).unwrap();
    Db::open(
        DbPaths {
            main,
            mount_index: Some(mount),
            llm_logs: None,
        },
        TEST_PEPPER,
    )
    .expect("open db")
}

// ── The model boundary ─────────────────────────────────────────────────────

/// The canned cheap-LLM provider, keyed by the SYSTEM prompt exactly as the
/// oracle's `createLLMProvider` mock is — so a wrong evaluator prompt fails the
/// case rather than silently answering the other arm's reply.
struct CannedTitleProvider {
    canned: HashMap<String, CannedTitle>,
    /// Per-case override (the no-rename / unparseable arms).
    override_reply: Option<CannedTitle>,
    throws: bool,
}

/// v4's `keyForSystemPrompt`: the help evaluator names itself in its first line.
fn key_for_system_prompt(system: &str) -> &'static str {
    if system.contains("help chat title evaluator") {
        "help"
    } else {
        "literary"
    }
}

impl CompletionProvider for CannedTitleProvider {
    async fn send_message(
        &self,
        _provider: &str,
        _base_url: Option<&str>,
        params: &CompletionParams,
    ) -> Result<CompletionResponse, CompletionError> {
        if self.throws {
            return Err(CompletionError::new("canned provider failure"));
        }
        let system = params
            .messages
            .iter()
            .find(|m| m.role.as_str() == "system")
            .map(|m| m.content.clone())
            .unwrap_or_default();
        let key = key_for_system_prompt(&system);
        let canned = self
            .override_reply
            .clone()
            .or_else(|| self.canned.get(key).cloned())
            .unwrap_or_else(|| panic!("no canned reply for key {key}"));
        Ok(CompletionResponse {
            content: canned.content,
            usage: Some(CompletionUsage {
                prompt_tokens: canned.prompt_tokens,
                completion_tokens: canned.completion_tokens,
                total_tokens: canned.prompt_tokens + canned.completion_tokens,
            }),
            finish_reason: None,
            attachment_results: None,
        })
    }
}

// ── The diff surface ───────────────────────────────────────────────────────

fn sorted(v: &Value) -> Value {
    match v {
        Value::Object(o) => {
            let mut m = serde_json::Map::new();
            let mut keys: Vec<&String> = o.keys().collect();
            keys.sort();
            for k in keys {
                m.insert(k.clone(), sorted(&o[k]));
            }
            Value::Object(m)
        }
        Value::Array(a) => Value::Array(a.iter().map(sorted).collect()),
        other => other.clone(),
    }
}
fn norm(v: &Value) -> String {
    serde_json::to_string_pretty(&sorted(v)).unwrap()
}
fn js_num(f: f64) -> Value {
    if f.is_finite() && f.fract() == 0.0 {
        Value::from(f as i64)
    } else {
        Value::from(f)
    }
}

/// What the handler wrote, in the oracle's `state` shape.
fn dump_state(db: &Db, chat_id: &str) -> Value {
    let cid = chat_id.to_string();
    db.read_main(move |conn| {
        let chat = quilltap_core::db::chats_read::find_by_id(conn, &cid)?;
        let chat_json = match &chat {
            Some(c) => serde_json::json!({
                "title": c.get("title").cloned().unwrap_or(Value::Null),
                "lastRenameCheckInterchange": c
                    .get("lastRenameCheckInterchange").cloned().unwrap_or(Value::Null),
                "updatedAt": c.get("updatedAt").cloned().unwrap_or(Value::Null),
            }),
            None => Value::Null,
        };

        let mut events: Vec<Value> = Vec::new();
        if chat.is_some() {
            for m in quilltap_core::db::chats_messages_read::get_messages(conn, &cid)? {
                if m.get("type").and_then(Value::as_str) != Some("system")
                    || m.get("systemEventType").and_then(Value::as_str) != Some("TITLE_GENERATION")
                {
                    continue;
                }
                events.push(serde_json::json!({
                    "systemEventType": m.get("systemEventType").cloned().unwrap_or(Value::Null),
                    "description": m.get("description").cloned().unwrap_or(Value::Null),
                    "promptTokens": m.get("promptTokens").cloned().unwrap_or(Value::Null),
                    "completionTokens": m.get("completionTokens").cloned().unwrap_or(Value::Null),
                    "totalTokens": m.get("totalTokens").cloned().unwrap_or(Value::Null),
                    "provider": m.get("provider").cloned().unwrap_or(Value::Null),
                    "modelName": m.get("modelName").cloned().unwrap_or(Value::Null),
                    // The oracle applies `?? null` — an omitted column reads null.
                    "estimatedCostUSD": m.get("estimatedCostUSD").cloned().unwrap_or(Value::Null),
                }));
            }
        }

        let mut jobs: Vec<Value> =
            quilltap_core::db::background_jobs::BackgroundJobsRepository::new(conn)
                .find_all()?
                .into_iter()
                .map(|j| {
                    serde_json::json!({
                        "type": j.job_type,
                        "status": j.status,
                        "priority": js_num(j.priority),
                        "payload": serde_json::from_str::<Value>(&j.payload).unwrap_or(Value::Null),
                    })
                })
                .collect();
        jobs.sort_by_key(|v| v["type"].as_str().unwrap_or("").to_string());

        Ok(serde_json::json!({
            "chat": chat_json,
            "systemEvents": events,
            "jobs": jobs,
        }))
    })
    .expect("dump state")
}

#[test]
fn title_update_matches_oracle() {
    let Some(oracle_path) = env_or_skip("QT_ORACLE_TITLE_UPDATE") else {
        return;
    };
    let spec: Spec = serde_json::from_str(&std::fs::read_to_string(spec_path()).unwrap()).unwrap();

    let mut oracle: HashMap<String, Value> = HashMap::new();
    for line in std::fs::read_to_string(&oracle_path)
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
    {
        let v: Value = serde_json::from_str(line).unwrap();
        oracle.insert(v["name"].as_str().unwrap().to_string(), v);
    }

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let mut failed: Vec<String> = Vec::new();

    // (name, chat, user, override reply, provider throws)
    let cases: Vec<(&str, &str, &str, Option<CannedTitle>, bool)> = vec![
        (
            "manually_renamed",
            &spec.chat_renamed_id,
            &spec.user_enabled_id,
            None,
            false,
        ),
        (
            "help_chat",
            &spec.chat_help_id,
            &spec.user_enabled_id,
            None,
            false,
        ),
        (
            "normal_renamed",
            &spec.chat_title_id,
            &spec.user_enabled_id,
            None,
            false,
        ),
        (
            "no_rename_needed",
            &spec.chat_title_id,
            &spec.user_enabled_id,
            Some(CannedTitle {
                content: r#"{"needsNewTitle": false, "reason": "the title still fits", "suggestedTitle": null}"#.to_string(),
                prompt_tokens: 40,
                completion_tokens: 8,
            }),
            false,
        ),
        (
            "unparseable_reply",
            &spec.chat_title_id,
            &spec.user_enabled_id,
            Some(CannedTitle {
                content: "I am afraid I cannot do that.".to_string(),
                prompt_tokens: 40,
                completion_tokens: 6,
            }),
            false,
        ),
        (
            "provider_throws",
            &spec.chat_title_id,
            &spec.user_enabled_id,
            None,
            true,
        ),
        (
            "autonomous_no_background",
            &spec.chat_autonomous_id,
            &spec.user_enabled_id,
            None,
            false,
        ),
        (
            "disabled_no_background",
            &spec.chat_title_id,
            &spec.user_disabled_id,
            None,
            false,
        ),
        (
            "empty_conversation",
            &spec.chat_regen_id,
            &spec.user_enabled_id,
            None,
            false,
        ),
        (
            "chat_missing",
            &spec.missing_id,
            &spec.user_enabled_id,
            None,
            false,
        ),
    ];

    for (name, chat_id, user_id, override_reply, throws) in cases {
        let db = fresh_db(name);
        let provider = CannedTitleProvider {
            canned: spec.canned_titles.clone(),
            override_reply,
            throws,
        };
        let executor = CheapLlmTaskExecutor::new();
        let payload = TitleUpdatePayload {
            chat_id: chat_id.to_string(),
            connection_profile_id: spec.connection_profile_id.clone(),
            current_interchange: 5.0,
        };
        let outcome = rt.block_on(handle_title_update(
            &db,
            &provider,
            &executor,
            &NoMessageCost,
            user_id,
            &payload,
            spec.frozen_now_ms,
        ));

        let want = &oracle[name];
        // The throw arms: diff the message v4 threw.
        let want_threw = want["threw"].as_str();
        let got_threw = outcome.as_ref().err().map(String::as_str);
        if got_threw != want_threw {
            eprintln!("[{name}] THROW MISMATCH: got {got_threw:?} / want {want_threw:?}");
            failed.push(name.to_string());
            continue;
        }
        if want_threw.is_some() {
            eprintln!("[{name}] OK (threw).");
            continue;
        }

        let got = dump_state(&db, chat_id);
        if norm(&got) != norm(&want["state"]) {
            eprintln!(
                "[{name}] STATE MISMATCH:\n got {}\n want {}",
                norm(&got),
                norm(&want["state"])
            );
            failed.push(name.to_string());
        } else {
            eprintln!("[{name}] OK.");
        }
    }

    assert!(failed.is_empty(), "cases failed: {failed:?}");
}

// ===========================================================================
// Runner-registration E2E (the W4.8 pattern)
// ===========================================================================

/// enqueue → claim → dispatch → COMPLETED, through the real [`JobRunner`] and a
/// registry carrying the [`TitleUpdateHandler`].
///
/// This is the regression guard for the exact bug P4.6ao unit 3 fixed: the
/// handler FUNCTION being correct is not the same as the job type being
/// REGISTERED. `TITLE_UPDATE` sat in `KNOWN_JOB_TYPES` with no handler, so every
/// enqueued title job failed on the runner's "recognized but not yet available"
/// fallback while the differential above would have been perfectly green. Asserts
/// the job reaches COMPLETED with no `lastError` — an unregistered type would
/// land FAILED with the fallback message.
///
/// Needs no oracle (the writes are pinned above); it runs on the committed
/// fixture alone.
#[test]
fn title_update_runner_registration_e2e() {
    use quilltap_core::db::background_jobs::BackgroundJobsRepository;
    use quilltap_core::services::job_runner::{HandlerRegistry, JobRunner};
    use quilltap_core::services::queue_service::enqueue_title_update;
    use quilltap_core::services::title_update_job::TitleUpdateHandler;

    let spec: Spec = serde_json::from_str(&std::fs::read_to_string(spec_path()).unwrap()).unwrap();
    let db = fresh_db("registration_e2e");
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    let job_id = rt
        .block_on(enqueue_title_update(
            &db,
            &spec.user_enabled_id,
            &spec.chat_title_id,
            serde_json::json!({
                "chatId": spec.chat_title_id,
                "connectionProfileId": spec.connection_profile_id,
                "currentInterchange": 5,
            }),
        ))
        .expect("enqueue");

    let mut reg = HandlerRegistry::new();
    reg.register(
        "TITLE_UPDATE",
        Box::new(TitleUpdateHandler {
            completion: CannedTitleProvider {
                canned: spec.canned_titles.clone(),
                override_reply: None,
                throws: false,
            },
            executor: CheapLlmTaskExecutor::new(),
            cost: NoMessageCost,
            now_ms: spec.frozen_now_ms,
        }),
    );
    let runner = JobRunner::new(db.clone(), reg);

    // NB: the pump dispatches TWO jobs, not one — a successful title rename
    // enqueues a STORY_BACKGROUND_GENERATION job, and the same pump claims it.
    // Only TITLE_UPDATE is registered here, so that second job legitimately
    // lands on the loud fallback; this test's claim is about the FIRST one.
    let out = rt.block_on(runner.pump_claim());
    assert!(out.dispatched >= 1, "runner should dispatch the title job");

    let jid = job_id.clone();
    let (status, last_error): (String, Option<String>) = db
        .read_main(move |c| {
            let job = BackgroundJobsRepository::new(c)
                .find_by_id(&jid)?
                .expect("job row");
            Ok((job.status, job.last_error))
        })
        .expect("read job");
    assert_eq!(
        status, "COMPLETED",
        "TITLE_UPDATE must dispatch to its handler, not the loud fallback (lastError: {last_error:?})"
    );
    assert_eq!(last_error, None);

    // And the handler actually did its work through that path.
    let cid = spec.chat_title_id.clone();
    let title: Option<String> = db
        .read_main(move |c| {
            Ok(quilltap_core::db::chats_read::find_by_id(c, &cid)?
                .and_then(|v| v["title"].as_str().map(String::from)))
        })
        .expect("read chat");
    assert_eq!(title.as_deref(), Some("The Ballonet's Slow Betrayal"));
}
