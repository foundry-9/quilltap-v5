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
    chat_mixed_status_id: String,
    chat_all_left_id: String,
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
    /// The message it throws with (v4's `providerThrowMessage`). A
    /// timeout-shaped one drives `is_timeout_failure` → the same-route retry →
    /// `timed_out` → the lost-pass failure (v4 `a1d88aa3a`, bug 107).
    throw_message: String,
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
            return Err(CompletionError::new(self.throw_message.clone()));
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
            cache_usage: None,
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
    /// One driven case: name, chat id, user id, the reply override, whether the
    /// provider throws, and the message it throws with.
    type Case<'a> = (
        &'a str,
        &'a str,
        &'a str,
        Option<CannedTitle>,
        bool,
        &'a str,
    );
    let cases: Vec<Case> = vec![
        (
            "manually_renamed",
            &spec.chat_renamed_id,
            &spec.user_enabled_id,
            None,
            false,
            "",
        ),
        (
            "help_chat",
            &spec.chat_help_id,
            &spec.user_enabled_id,
            None,
            false,
            "",
        ),
        (
            "normal_renamed",
            &spec.chat_title_id,
            &spec.user_enabled_id,
            None,
            false,
            "",
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
            "",
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
            "",
        ),
        (
            "provider_throws",
            &spec.chat_title_id,
            &spec.user_enabled_id,
            None,
            true,
            "canned provider failure",
        ),
        // …unless it TIMED OUT (v4 `a1d88aa3a`, bug 107). Then the check never
        // ran, the cursor must NOT be burned over it, and the handler fails so
        // the job is retried. The throw sits BEFORE the cursor write for
        // exactly that reason — v4's own note in `BACKGROUND_JOBS_CHILD.md`.
        (
            "provider_times_out",
            &spec.chat_title_id,
            &spec.user_enabled_id,
            None,
            true,
            "Request timed out.",
        ),
        (
            "autonomous_no_background",
            &spec.chat_autonomous_id,
            &spec.user_enabled_id,
            None,
            false,
            "",
        ),
        (
            "disabled_no_background",
            &spec.chat_title_id,
            &spec.user_disabled_id,
            None,
            false,
            "",
        ),
        (
            "empty_conversation",
            &spec.chat_regen_id,
            &spec.user_enabled_id,
            None,
            false,
            "",
        ),
        // ── P4.D110 / v4 bug 96: the tolerant title-verdict parser, measured
        // through the handler's WRITES. Each reply is a shape the pre-fix
        // canonical-key-only parser read as "no rename wanted".
        (
            "key_typo_suggest_title",
            &spec.chat_title_id,
            &spec.user_enabled_id,
            Some(CannedTitle {
                content: r#"{"needsNewTitle":true,"reason":"The current title is generic and doesn't reflect the content.","suggestTitle":"The Beast's Hundred Gigajoules"}"#.to_string(),
                prompt_tokens: 44,
                completion_tokens: 12,
            }),
            false,
            "",
        ),
        (
            "fold_key_snake_case",
            &spec.chat_title_id,
            &spec.user_enabled_id,
            Some(CannedTitle {
                content: r#"{"needsNewTitle":true,"reason":"the title is generic","suggested_title":"Amber Lines Above the Table"}"#.to_string(),
                prompt_tokens: 41,
                completion_tokens: 9,
            }),
            false,
            "",
        ),
        (
            "canonical_beats_near_miss",
            &spec.chat_title_id,
            &spec.user_enabled_id,
            Some(CannedTitle {
                content: r#"{"needsNewTitle":true,"reason":"the title is generic","title":"The Wrong One","suggestedTitle":"The Right One"}"#.to_string(),
                prompt_tokens: 42,
                completion_tokens: 11,
            }),
            false,
            "",
        ),
        (
            "rename_with_no_usable_title",
            &spec.chat_title_id,
            &spec.user_enabled_id,
            Some(CannedTitle {
                content: r#"{"needsNewTitle":true,"reason":"the current title says nothing","headline":"Under An Unknown Key"}"#.to_string(),
                prompt_tokens: 40,
                completion_tokens: 10,
            }),
            false,
            "",
        ),
        (
            "quoted_padded_title",
            &spec.chat_title_id,
            &spec.user_enabled_id,
            Some(CannedTitle {
                content: r#"{"needsNewTitle":true,"reason":"the title is generic","suggestedTitle":"  \"  A Padded Quoted Title  \"  "}"#.to_string(),
                prompt_tokens: 43,
                completion_tokens: 10,
            }),
            false,
            "",
        ),
        (
            "overlong_near_miss_title",
            &spec.chat_title_id,
            &spec.user_enabled_id,
            Some(CannedTitle {
                content: r#"{"needsNewTitle":true,"reason":"the title is generic","newTitle":"The Ballonet, the Beast, and the Long Interminable Descent Over Chicago"}"#.to_string(),
                prompt_tokens: 45,
                completion_tokens: 18,
            }),
            false,
            "",
        ),
        (
            "null_canonical_falls_through",
            &spec.chat_title_id,
            &spec.user_enabled_id,
            Some(CannedTitle {
                content: r#"{"needsNewTitle":true,"reason":"the title is generic","suggestedTitle":null,"newTitle":"Recovered From A Null"}"#.to_string(),
                prompt_tokens: 42,
                completion_tokens: 9,
            }),
            false,
            "",
        ),
        // [P4.D146 / v4 70505745a] The story-background enqueue's presence gate.
        // The enqueued job's payload `characterIds` is the comparand: one
        // participant per status must yield exactly [active, silent], and a chat
        // where everyone has left enqueues NO job (the rename still happens).
        (
            "present_participants_only",
            &spec.chat_mixed_status_id,
            &spec.user_enabled_id,
            None,
            false,
            "",
        ),
        (
            "all_participants_left",
            &spec.chat_all_left_id,
            &spec.user_enabled_id,
            None,
            false,
            "",
        ),
        (
            "chat_missing",
            &spec.missing_id,
            &spec.user_enabled_id,
            None,
            false,
            "",
        ),
    ];

    for (name, chat_id, user_id, override_reply, throws, throw_message) in cases {
        let db = fresh_db(name);
        let provider = CannedTitleProvider {
            canned: spec.canned_titles.clone(),
            override_reply,
            throws,
            throw_message: throw_message.to_string(),
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
                throw_message: String::new(),
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

// ===========================================================================
// P4.D110 + P4.61 — the handler's log lines, pinned with a capturing layer
// ===========================================================================
//
// The differential above cannot see any of these: a log line writes no row, and
// several of them ride branches whose DB state is byte-identical to a sibling's
// (the checkpoint-burned warn vs a genuine decline is the extreme case — that
// identity IS bug 96's invisibility). So the sanctioned proof for a log-only
// port is a capturing `tracing::Layer` over the REAL `handle_title_update`, on
// the committed fixture, with no oracle — asserting BOTH that the line fires on
// its own branch and that the sibling branches stay SILENT, because a line
// attached to the wrong branch is exactly the defect a presence-only assertion
// cannot catch.
//
// P4.61 carries v4 `title-update.ts`'s remaining log sites. Two of the eight are
// NO-PORTs with evidence, recorded here so a later reader does not "fix" them
// back in:
//
//   * `:89` `[Title Update] No cheap LLM available` — dead code in v4 itself.
//     `getCheapLLMProvider` is typed `: CheapLLMSelection` and has no `return
//     null` on any path (`cheap-llm.ts:176-278`; priority 5 always yields the
//     current profile), so `if (!cheapLLMSelection)` can never be entered. v5's
//     `get_cheap_llm_provider` returns non-optionally for that reason, so there
//     is no branch to attach the line to on either side.
//   * `:185` `[Title Update] Failed to create system event:` — its `try` cannot
//     throw. `estimateMessageCost` catches its whole body and returns
//     `{cost: null, source: 'unavailable'}` (`cost-estimation.service.ts:124`);
//     `createSystemEvent` catches its whole body, logs its OWN different
//     sentence, and returns null (`system-events.service.ts:73`). v5's seams are
//     `Option`-returning for the same reason.

use quilltap_core::test_support::CaptureLayer;
use std::sync::{Arc, Mutex};
use tracing_subscriber::layer::SubscriberExt;

/// Drive the REAL handler `runs` times over ONE fresh copy of the committed
/// fixture and return every line it logged.
///
/// * `override_reply` / `throws` are the model boundary, as in the differential.
/// * `break_jobs_table` drops `background_jobs` first, so the story-background
///   enqueue really fails — v4's `:303` catch arm has no other way in, since the
///   enqueue's only failure mode is the database refusing it.
/// * `runs > 1` re-drives the same chat on the same DB, which exercises v4's
///   `isNew: false` dedupe arm (the second enqueue finds the first job pending
///   and must say nothing).
#[allow(clippy::too_many_arguments)]
fn capture_runs(
    spec: &Spec,
    tag: &str,
    chat_id: &str,
    user_id: &str,
    override_reply: Option<CannedTitle>,
    throws: bool,
    break_jobs_table: bool,
    runs: usize,
) -> Vec<String> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let db = fresh_db(tag);
    if break_jobs_table {
        rt.block_on(db.write(|w| {
            w.main()
                .connection()
                .execute_batch("DROP TABLE background_jobs;")?;
            Ok(())
        }))
        .expect("drop background_jobs");
    }

    let logs = Arc::new(Mutex::new(Vec::<String>::new()));
    let subscriber = tracing_subscriber::registry().with(CaptureLayer(logs.clone()));
    {
        let _guard = tracing::subscriber::set_default(subscriber);
        for _ in 0..runs {
            let provider = CannedTitleProvider {
                canned: spec.canned_titles.clone(),
                override_reply: override_reply.clone(),
                throws,
                throw_message: "canned provider failure".to_string(),
            };
            let payload = TitleUpdatePayload {
                chat_id: chat_id.to_string(),
                connection_profile_id: spec.connection_profile_id.clone(),
                current_interchange: 5.0,
            };
            rt.block_on(handle_title_update(
                &db,
                &provider,
                &CheapLlmTaskExecutor::new(),
                &NoMessageCost,
                user_id,
                &payload,
                spec.frozen_now_ms,
            ))
            .expect("handler");
        }
    }
    let out = logs.lock().unwrap().clone();
    out
}

fn read_spec() -> Spec {
    serde_json::from_str(&std::fs::read_to_string(spec_path()).unwrap()).unwrap()
}

fn canned(content: &str) -> Option<CannedTitle> {
    Some(CannedTitle {
        content: content.to_string(),
        prompt_tokens: 40,
        completion_tokens: 10,
    })
}

/// The one line in `lines` containing `needle`, or a panic naming everything the
/// handler DID log.
fn one<'a>(lines: &'a [String], needle: &str) -> &'a String {
    let hits: Vec<&String> = lines.iter().filter(|l| l.contains(needle)).collect();
    assert_eq!(
        hits.len(),
        1,
        "expected exactly one {needle:?} line, got {}: {lines:#?}",
        hits.len()
    );
    hits[0]
}

fn none(lines: &[String], needle: &str) {
    assert!(
        !lines.iter().any(|l| l.contains(needle)),
        "expected NO {needle:?} line: {lines:#?}"
    );
}

/// v4 `:196` — when the model asked for a rename and handed back nothing usable,
/// say so instead of burning the checkpoint in silence.
#[test]
fn checkpoint_burned_warn_fires_only_when_a_rename_had_no_usable_title() {
    let spec = read_spec();

    // (a) The bug-96 residue: a rename asked for under a key nothing can read.
    let lines = capture_runs(
        &spec,
        "burned_warn",
        &spec.chat_title_id,
        &spec.user_enabled_id,
        canned(
            r#"{"needsNewTitle":true,"reason":"the current title says nothing","headline":"Under An Unknown Key"}"#,
        ),
        false,
        false,
        1,
    );
    let burned = one(
        &lines,
        "[Title Update] Rename requested with no usable title — checkpoint burned",
    );
    assert!(
        burned.contains("context=background-jobs.title-update"),
        "{burned}"
    );
    assert!(
        burned.contains(&format!("chat_id={}", spec.chat_title_id)),
        "{burned}"
    );
    assert!(burned.contains("current_interchange=5"), "{burned}");
    assert!(
        burned.contains("reason=the current title says nothing"),
        "{burned}"
    );

    // (b) A genuine decline takes the SAME guard and must stay silent.
    let lines = capture_runs(
        &spec,
        "burned_warn_silent",
        &spec.chat_title_id,
        &spec.user_enabled_id,
        canned(r#"{"needsNewTitle":false,"reason":"the title still fits","suggestedTitle":null}"#),
        false,
        false,
        1,
    );
    none(&lines, "checkpoint burned");
}

/// v4 `:152` — `[Title Update] Failed for chat ${chatId}: ${result.error}`, one
/// rendered sentence with no fields bag. The failing cheap-LLM call is the only
/// absorbed failure in the handler, so before P4.61 an exhausted quota burned
/// checkpoints in complete silence.
#[test]
fn failed_call_warn_fires_only_when_the_cheap_llm_call_failed() {
    let spec = read_spec();

    // (a) The provider throws — v4's `!result.success` arm.
    let lines = capture_runs(
        &spec,
        "failed_warn",
        &spec.chat_title_id,
        &spec.user_enabled_id,
        None,
        true,
        false,
        1,
    );
    let failed = one(
        &lines,
        &format!("[Title Update] Failed for chat {}: ", spec.chat_title_id),
    );
    assert!(failed.contains("canned provider failure"), "{failed}");
    // The whole sentence is the message; v4 passes no context object, so nothing
    // but the target rides along.
    assert!(failed.starts_with("WARN "), "{failed}");

    // (b) A successful evaluation must not claim a failure.
    let lines = capture_runs(
        &spec,
        "failed_warn_silent",
        &spec.chat_title_id,
        &spec.user_enabled_id,
        None,
        false,
        false,
        1,
    );
    none(&lines, "[Title Update] Failed for chat");
}

/// v4 `:213` + `:224` — the decided-to-rename line and the wrote-the-title line.
/// Both are single rendered sentences; `:224` quotes the title, and the quotes
/// are v4's own bytes.
#[test]
fn rename_info_lines_fire_only_when_a_title_is_written() {
    let spec = read_spec();

    // (a) The canned literary verdict renames the chat.
    let lines = capture_runs(
        &spec,
        "rename_info",
        &spec.chat_title_id,
        &spec.user_enabled_id,
        None,
        false,
        false,
        1,
    );
    one(
        &lines,
        &format!(
            "[Title Update] Chat {} - needsNewTitle: true, reason: the generic title says nothing of the ballonet",
            spec.chat_title_id
        ),
    );
    one(
        &lines,
        &format!(
            "[Title Update] Updated title for chat {} to: \"The Ballonet's Slow Betrayal\"",
            spec.chat_title_id
        ),
    );

    // (b) A decline writes no title, so neither line may appear.
    let lines = capture_runs(
        &spec,
        "rename_info_silent",
        &spec.chat_title_id,
        &spec.user_enabled_id,
        canned(r#"{"needsNewTitle":false,"reason":"the title still fits","suggestedTitle":null}"#),
        false,
        false,
        1,
    );
    none(&lines, "needsNewTitle: true");
    none(&lines, "[Title Update] Updated title for chat");

    // (c) So does a failed call — the earlier return must not reach either line.
    let lines = capture_runs(
        &spec,
        "rename_info_silent_failed",
        &spec.chat_title_id,
        &spec.user_enabled_id,
        None,
        true,
        false,
        1,
    );
    none(&lines, "needsNewTitle: true");
    none(&lines, "[Title Update] Updated title for chat");
}

/// v4 `:294` — the queued-a-background line, gated on `isNew`, with the fields
/// an operator needs to find the job. The three gates that skip the enqueue
/// (settings disabled, an autonomous room, a dedupe hit) must each stay silent.
#[test]
fn queued_story_background_info_fires_once_per_new_job() {
    let spec = read_spec();

    // (a) A fresh rename on a story-backgrounds-enabled user queues one.
    let lines = capture_runs(
        &spec,
        "queued_info",
        &spec.chat_title_id,
        &spec.user_enabled_id,
        None,
        false,
        false,
        1,
    );
    let queued = one(&lines, "[Title Update] Queued story background generation");
    assert!(
        queued.contains("context=background-jobs.title-update"),
        "{queued}"
    );
    assert!(
        queued.contains(&format!("chat_id={}", spec.chat_title_id)),
        "{queued}"
    );
    assert!(queued.contains("job_id="), "{queued}");
    assert!(queued.contains("image_profile_id="), "{queued}");
    assert!(queued.contains("character_count=2"), "{queued}");

    // (b) The dedupe arm: a second run finds the first job pending, so
    //     `isNew` is false and v4 says nothing. Exactly ONE line across both.
    let lines = capture_runs(
        &spec,
        "queued_info_dedupe",
        &spec.chat_title_id,
        &spec.user_enabled_id,
        None,
        false,
        false,
        2,
    );
    one(&lines, "[Title Update] Queued story background generation");

    // (c) Story backgrounds disabled — renamed, but nothing queued.
    let lines = capture_runs(
        &spec,
        "queued_info_disabled",
        &spec.chat_title_id,
        &spec.user_disabled_id,
        None,
        false,
        false,
        1,
    );
    one(&lines, "[Title Update] Updated title for chat");
    none(&lines, "Queued story background generation");

    // (d) An autonomous room — the Lantern's auto-trigger is off there.
    let lines = capture_runs(
        &spec,
        "queued_info_autonomous",
        &spec.chat_autonomous_id,
        &spec.user_enabled_id,
        None,
        false,
        false,
        1,
    );
    one(&lines, "[Title Update] Updated title for chat");
    none(&lines, "Queued story background generation");
}

/// v4 `:303` — the enqueue's catch arm. The only way in is the database refusing
/// the job row, so the fixture copy loses its `background_jobs` table first.
#[test]
fn failed_to_queue_warn_fires_only_when_the_enqueue_errors() {
    let spec = read_spec();

    // (a) No `background_jobs` table — the enqueue errors and says so.
    let lines = capture_runs(
        &spec,
        "queue_fail_warn",
        &spec.chat_title_id,
        &spec.user_enabled_id,
        None,
        false,
        true,
        1,
    );
    let failed = one(
        &lines,
        "[Title Update] Failed to queue story background generation",
    );
    assert!(
        failed.contains("context=background-jobs.title-update"),
        "{failed}"
    );
    assert!(
        failed.contains(&format!("chat_id={}", spec.chat_title_id)),
        "{failed}"
    );
    assert!(failed.contains("error="), "{failed}");
    // The failure is loud INSTEAD of the success line, not alongside it.
    none(&lines, "Queued story background generation");
    // And the handler still completed — the enqueue stays best-effort.
    one(&lines, "[Title Update] Updated title for chat");

    // (b) A healthy enqueue must not warn.
    let lines = capture_runs(
        &spec,
        "queue_fail_warn_silent",
        &spec.chat_title_id,
        &spec.user_enabled_id,
        None,
        false,
        false,
        1,
    );
    none(&lines, "Failed to queue story background generation");
}
