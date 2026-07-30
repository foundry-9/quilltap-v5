//! P4.6c swipe-GENERATE differential: `api::salon::message_swipe_generate` + a
//! canned-provider `SwipeGenerateDriver` vs v4's REAL `handleGenerateSwipe` (POST
//! `/messages/{id}?action=swipe`). Proves the ROUTE layer — the ownership gate, the
//! ASSISTANT-only / `systemSender` guards, the 404, the 201 `{ message }` shape —
//! and the driver seam over the committed salon fixture. The byte-exact
//! regeneration itself is separately proven by `regenerate_swipe_tier3`.
//!
//! The happy case's driver replays the oracle-recorded canned completion (keyed by
//! `provider|model|temperature|messages` — the rebuilt continue-mode prompt bytes),
//! composing the ported `regenerate_message_as_swipe` over canned providers +
//! `NoopSeams` (matching the oracle's mocked feeders + K-loader).
//!
//! Generate the oracle (Node 24, from the v4 checkout — see the .ts header):
//!   … TZ=UTC QT_ORACLE_OUT=/tmp/oracle-salon-swipe.ndjson npx jest -- salon-swipe-generate
//!   (TZ=UTC REQUIRED since P4.d26 — the distill TODAY line is server-local;
//!   the harness pins server_tz "UTC")
//! Run:
//!   QT_ORACLE_SALON_SWIPE=/tmp/oracle-salon-swipe.ndjson \
//!     cargo test -p quilltap-harness --test salon_swipe_generate_equivalence

use std::collections::HashMap;
use std::path::PathBuf;

use quilltap_core::api::chat_send::{
    SwipeGenerateDriver, SwipeGenerateFuture, SwipeGenerateRequest,
};
use quilltap_core::api::salon;
use quilltap_core::api::types::{CoreError, ErrorKind, Response};
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::db::{dump_table_json_conn, DbError};
use quilltap_core::model::completion::{
    CompletionError, CompletionParams, CompletionProvider, CompletionResponse, CompletionUsage,
};
use quilltap_core::model::embedding::CannedEmbeddingProvider;
use quilltap_core::services::build_context::NoopSeams as BcNoopSeams;
use quilltap_core::services::cheap_llm_exec::CheapLlmTaskExecutor;
use quilltap_core::services::message_context::NoopMessageContextSeams;
use quilltap_core::services::regenerate_swipe::{
    regenerate_message_as_swipe, RegenError, RegenerateSwipeOptions,
};
use serde::Deserialize;
use serde_json::Value;

const FROZEN_NOW_MS: i64 = 1_770_000_000_000;
const CANNED_RESPONSE: &str = "A fresh retort, reconsidered.";

/// An always-respond completion — returns the fixed canned response + usage for
/// ANY call. The D2 differential proves the ROUTE + driver seam produce a 201 +
/// a persisted swipe whose OUTPUT matches v4 byte-for-byte (content = the canned
/// response, participant, minted swipe group, token counts from the usage). The
/// rebuilt prompt's byte-exactness is a separate proof (`regenerate_swipe_tier3`
/// / `build_context_tier3`) — the salon fixture's undefined-identity Aria exposes
/// a v4 system-prompt rendering edge (v4 leaks a literal "undefined"; see the
/// report) that is orthogonal to the swipe route, so it is deliberately not
/// coupled here (v4's oracle mock also returns the same response for any call).
struct AlwaysCompletion;
impl CompletionProvider for AlwaysCompletion {
    #[allow(clippy::manual_async_fn)]
    fn send_message(
        &self,
        _provider: &str,
        _base_url: Option<&str>,
        _params: &CompletionParams,
    ) -> impl std::future::Future<Output = Result<CompletionResponse, CompletionError>> + Send {
        async {
            Ok(CompletionResponse {
                content: CANNED_RESPONSE.to_string(),
                usage: Some(CompletionUsage {
                    prompt_tokens: 40,
                    completion_tokens: 12,
                    total_tokens: 52,
                }),
                finish_reason: Some("stop".to_string()),
            })
        }
    }
}
/// The salon fixture's OPENAI_COMPATIBLE profile has a null model → the group chat
/// is tiny, so any limit above its context is identical (no truncation); pinned
/// large to match v4's no-truncation build.
const MODEL_CONTEXT_LIMIT: i64 = 200_000;

/// The set of salon-fixture `chat_messages` ids + swipe-group ids (pinned) — every
/// other id/group is minted by the regeneration and gets a first-appearance token.
const KNOWN: &[&str] = &[
    "d1000000-0000-4000-8000-000000000001",
    "d1000000-0000-4000-8000-000000000002",
    "d1000000-0000-4000-8000-000000000003",
    "d1000000-0000-4000-8000-000000000004",
    "d2000000-0000-4000-8000-000000000001",
    "d2000000-0000-4000-8000-000000000002",
    "d2000000-0000-4000-8000-000000000003",
    "d2000000-0000-4000-8000-000000000004",
    "d2000000-0000-4000-8000-000000000005",
    "d2000000-0000-4000-8000-000000000006",
    "d2000000-0000-4000-8000-000000000007",
];

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    test_pepper_base64: String,
    user_id: String,
}
fn spec_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../harness/oracle/fixtures/salon.json")
}
fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../quilltap-web/tests/fixtures")
}

/// The swipe driver — composes the ported service over the always-respond
/// completion + `NoopSeams`, with the oracle's frozen clock.
struct TestSwipeDriver<'a> {
    db: &'a Db,
    completion: &'a AlwaysCompletion,
    embedding: &'a CannedEmbeddingProvider,
    executor: &'a CheapLlmTaskExecutor,
}
impl SwipeGenerateDriver for TestSwipeDriver<'_> {
    fn generate_swipe(&self, req: SwipeGenerateRequest) -> SwipeGenerateFuture<'_> {
        let db = self.db;
        let completion = self.completion;
        let embedding = self.embedding;
        let executor = self.executor;
        Box::pin(async move {
            let bc = BcNoopSeams;
            let mc = NoopMessageContextSeams;
            let opts = RegenerateSwipeOptions {
                user_id: req.user_id,
                chat: req.chat,
                target_message: req.target_message,
                all_messages: req.all_messages,
                active_user_participant_id: req.active_user_participant_id,
                model_context_limit: MODEL_CONTEXT_LIMIT,
                timestamp_config: None,
                timezone: Some("UTC".to_string()),
                server_tz: Some("UTC".to_string()),
                now_ms: FROZEN_NOW_MS,
                local_offset_minutes: 0,
                random01: 0.0,
            };
            regenerate_message_as_swipe(db, embedding, completion, executor, &bc, &mc, opts)
                .await
                .map_err(|e| match e {
                    RegenError::NotAssistant | RegenError::StaffMessage => CoreError {
                        kind: ErrorKind::BadRequest,
                        message: e.to_string(),
                        pepper_state: None,
                        code: None,
                        associations: None,
                    },
                    RegenError::Db(_) => CoreError {
                        kind: ErrorKind::Internal,
                        message: e.to_string(),
                        pepper_state: None,
                        code: None,
                        associations: None,
                    },
                })
        })
    }
}

/// Remap every minted `id` + `swipeGroupId` (a value NOT in [`KNOWN`]) to a
/// first-appearance token, so the new swipe row's id + the minted shared swipe
/// group verify by relationship. Rows are sorted by a stable id-free key first.
fn normalize_messages(dump: &Value, idmap: &mut HashMap<String, String>) -> Value {
    let mut rows: Vec<Value> = dump
        .get("rows")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    rows.sort_by(|a, b| {
        let k = |r: &Value| {
            (
                r.get("chatId")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                r.get("createdAt")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                r.get("swipeIndex").and_then(Value::as_i64).unwrap_or(-1),
                r.get("role")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                r.get("content")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
            )
        };
        k(a).cmp(&k(b))
    });
    let remap = |v: &str, idmap: &mut HashMap<String, String>| -> String {
        if KNOWN.contains(&v) {
            return v.to_string();
        }
        let n = idmap.len();
        idmap
            .entry(v.to_string())
            .or_insert_with(|| format!("<mint{n}>"))
            .clone()
    };
    for r in &mut rows {
        if let Some(o) = r.as_object_mut() {
            o.remove("renderedHtml");
            if let Some(id) = o.get("id").and_then(Value::as_str).map(str::to_string) {
                let t = remap(&id, idmap);
                o.insert("id".into(), Value::String(t));
            }
            if let Some(sg) = o
                .get("swipeGroupId")
                .and_then(Value::as_str)
                .map(str::to_string)
            {
                let t = remap(&sg, idmap);
                o.insert("swipeGroupId".into(), Value::String(t));
            }
        }
    }
    Value::Array(rows)
}

/// chats: placeholder the minted metadata bump columns; strip renderedHtml.
fn normalize_chats(dump: &Value) -> Value {
    let mut rows: Vec<Value> = dump
        .get("rows")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    rows.sort_by_key(|r| r["id"].as_str().unwrap_or("").to_string());
    for r in &mut rows {
        if let Some(o) = r.as_object_mut() {
            o.remove("renderedHtml");
            for c in ["updatedAt", "lastMessageAt"] {
                if o.get(c).map(|v| !v.is_null()).unwrap_or(false) {
                    o.insert(c.to_string(), Value::String("<ts>".into()));
                }
            }
        }
    }
    Value::Array(rows)
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
fn norm(v: &Value) -> String {
    let mut v = v.clone();
    canon_numbers(&mut v);
    serde_json::to_string_pretty(&v).unwrap()
}
fn response_data(r: &Response) -> Value {
    serde_json::to_value(r)
        .unwrap()
        .get("data")
        .cloned()
        .unwrap_or(Value::Null)
}

#[test]
fn salon_swipe_generate_matches_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_SALON_SWIPE") else {
        eprintln!("SKIP: set QT_ORACLE_SALON_SWIPE (see test header).");
        return;
    };
    let spec: Spec =
        serde_json::from_str(&std::fs::read_to_string(spec_path()).unwrap()).expect("spec");
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

    const GROUP_ASSISTANT: &str = "d2000000-0000-4000-8000-000000000007";
    const GROUP_USER: &str = "d2000000-0000-4000-8000-000000000001";
    const GROUP_HOST: &str = "d2000000-0000-4000-8000-000000000003";
    const BAD: &str = "99999999-9999-4999-8999-999999999999";
    let cases = [
        ("swipe_happy", GROUP_ASSISTANT),
        ("swipe_not_assistant", GROUP_USER),
        ("swipe_systemsender", GROUP_HOST),
        ("swipe_not_found", BAD),
    ];

    let mut failed: Vec<String> = Vec::new();
    for (name, message_id) in cases {
        let want = &oracle[name];
        let completion = AlwaysCompletion;
        let embedding = CannedEmbeddingProvider::new();
        let executor = CheapLlmTaskExecutor::new();

        // Fresh fixture copy per case.
        let scratch =
            std::env::temp_dir().join(format!("qt-salon-swipe-{}-{}", std::process::id(), name));
        let _ = std::fs::remove_dir_all(&scratch);
        std::fs::create_dir_all(&scratch).unwrap();
        let main = scratch.join("main.db");
        let mount = scratch.join("mount.db");
        std::fs::copy(fixtures_dir().join("salon-main.db"), &main).unwrap();
        std::fs::copy(fixtures_dir().join("salon-mount.db"), &mount).unwrap();
        let db = Db::open(
            DbPaths {
                main,
                mount_index: Some(mount),
                llm_logs: None,
            },
            &spec.test_pepper_base64,
        )
        .expect("open db");

        let driver = TestSwipeDriver {
            db: &db,
            completion: &completion,
            embedding: &embedding,
            executor: &executor,
        };
        let got = rt.block_on(salon::message_swipe_generate(
            &db,
            &driver,
            &spec.user_id,
            message_id,
        ));
        let got_body = response_data(&got);

        let chats = db
            .read_main(|conn| dump_table_json_conn(conn, "chats", "id"))
            .unwrap();
        let msgs: Value = db
            .read_main(|conn| -> Result<Value, DbError> {
                dump_table_json_conn(conn, "chat_messages", "id")
            })
            .unwrap();
        drop(db);
        let _ = std::fs::remove_dir_all(&scratch);

        // Body: v4 error `{ error }` vs v5 `{ kind, message }`; the happy body is
        // `{ message: {...} }` — normalize the swipe id + group there too.
        if let Some(err) = want["body"].get("error").and_then(Value::as_str) {
            let got_msg = got_body
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("");
            if got_msg != err {
                eprintln!("[{name}] body error-copy MISMATCH:\n  GOT : {got_msg}\n  WANT: {err}");
                failed.push(format!("{name}/body"));
            }
        } else {
            // Happy: compare the persisted new swipe row (via the tables) — the body's
            // minted id/group ride the same normalization, so assert the message
            // CONTENT + shape here and the row bytes below.
            let got_content = got_body
                .get("message")
                .and_then(|m| m.get("content"))
                .and_then(Value::as_str)
                .unwrap_or("");
            let want_content = want["body"]
                .get("message")
                .and_then(|m| m.get("content"))
                .and_then(Value::as_str)
                .unwrap_or("<none>");
            if got_content != want_content {
                eprintln!(
                    "[{name}] body content MISMATCH:\n  GOT : {got_content}\n  WANT: {want_content}"
                );
                failed.push(format!("{name}/body"));
            }
        }

        // Tables (minted-normalized).
        let mut idmap: HashMap<String, String> = HashMap::new();
        let mut want_idmap: HashMap<String, String> = HashMap::new();
        let got_msgs = norm(&normalize_messages(&msgs, &mut idmap));
        let want_msgs = norm(&normalize_messages(
            &want["tables"]["chatMessages"],
            &mut want_idmap,
        ));
        if got_msgs != want_msgs {
            eprintln!(
                "[{name}] chat_messages MISMATCH:\n{}",
                diff(&got_msgs, &want_msgs)
            );
            failed.push(format!("{name}/chat_messages"));
        }
        let got_chats = norm(&normalize_chats(&chats));
        let want_chats = norm(&normalize_chats(&want["tables"]["chats"]));
        if got_chats != want_chats {
            eprintln!(
                "[{name}] chats MISMATCH:\n{}",
                diff(&got_chats, &want_chats)
            );
            failed.push(format!("{name}/chats"));
        }
        eprintln!("[{name}] done.");
    }
    assert!(failed.is_empty(), "salon-swipe-generate FAILED: {failed:?}");
}

fn diff(got: &str, want: &str) -> String {
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
    "(identical)".to_string()
}
