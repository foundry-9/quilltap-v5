//! P4.9E3A TIER-3 (mocked-LLM) differential for the MANUAL
//! `?action=regenerate-title`: `services::chat_admin::chat_regenerate_title` vs
//! v4's REAL `handleRegenerateTitle` through its real chat route. Both sides run
//! over a FRESH copy of the committed `chat-admin-{main,mount}.db` fixture per
//! case, with the SAME canned cheap-LLM reply injected at the model boundary,
//! then diff the response body AND the chat row's `title` /
//! `isManuallyRenamed` / `updatedAt`.
//!
//! ## The system prompt is part of the diff
//!
//! The canned reply is keyed by the SYSTEM prompt on both sides, and the prompts
//! the provider actually saw are compared VERBATIM — SYSTEM **and** USER, since
//! the transcript is the user entry. That pins three things a body-only diff
//! would miss: which generator ran (the literary
//! `CHAT_TITLE_PROMPT` vs the practical `HELP_CHAT_TITLE_PROMPT` — these are the
//! MANUAL generators, not the `TITLE_UPDATE` job's evaluators), `titleChat`'s
//! transcript weighting (last 100 messages, the last ten in full to 500 chars
//! and everything earlier truncated to 150), and the fact that v4 passes
//! `undefined` for `existingTitle`, so the "Current title / update only if…"
//! rider is NEVER appended from this entrance.
//!
//! Generate the oracle (Node 24, from the v4 checkout — see the .ts header):
//!   … QT_ORACLE_OUT=/tmp/oracle-chat-regenerate-title.ndjson \
//!     npx jest -- chat-regenerate-title-tier3
//! Run:
//!   QT_ORACLE_REGENERATE_TITLE=/tmp/oracle-chat-regenerate-title.ndjson \
//!     cargo test -p quilltap-harness --test chat_regenerate_title_tier3_equivalence \
//!       -- --nocapture

use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;
use std::sync::Mutex;

use quilltap_core::api::types::{ErrorKind, Response};
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::model::completion::{
    CompletionError, CompletionParams, CompletionProvider, CompletionResponse, CompletionUsage,
};
use quilltap_core::services::chat_admin;
use quilltap_core::services::cheap_llm_exec::CheapLlmTaskExecutor;
use serde::Deserialize;
use serde_json::{json, Value};

const CHAT: &str = "c1000000-0000-4000-8000-000000000001";
const EMPTY_CHAT: &str = "c1000000-0000-4000-8000-000000000003";
const HELP_CHAT: &str = "c1000000-0000-4000-8000-000000000004";
const MISSING_ID: &str = "99999999-9999-4999-8999-999999999999";

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Canned {
    content: String,
    prompt_tokens: i64,
    completion_tokens: i64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    test_pepper_base64: String,
    user_id: String,
    frozen_now_ms: i64,
    canned_titles: HashMap<String, Canned>,
}

/// The oracle's `keyForSystemPrompt`: the practical generator names itself in
/// its first line.
fn key_for_system_prompt(system: &str) -> &'static str {
    if system.starts_with("Generate a short, practical title") {
        "help"
    } else {
        "literary"
    }
}

/// The mocked model boundary, recording every MESSAGE LIST it is shown — the
/// user entry is where the transcript weighting lands, so a system-only record
/// would leave the whole 100/10/500/150 rendering unchecked.
struct CannedTitleProvider {
    canned: HashMap<String, Canned>,
    override_reply: Option<Canned>,
    throws: bool,
    seen: Mutex<Vec<Value>>,
}

impl CompletionProvider for CannedTitleProvider {
    async fn send_message(
        &self,
        _provider: &str,
        _base_url: Option<&str>,
        params: &CompletionParams,
    ) -> Result<CompletionResponse, CompletionError> {
        let system = params
            .messages
            .iter()
            .find(|m| m.role.as_str() == "system")
            .map(|m| m.content.clone())
            .unwrap_or_default();
        self.seen.lock().unwrap().push(Value::Array(
            params
                .messages
                .iter()
                .map(|m| json!({ "role": m.role.as_str(), "content": m.content }))
                .collect(),
        ));
        if self.throws {
            return Err(CompletionError::new("canned provider failure"));
        }
        let canned = self
            .override_reply
            .clone()
            .or_else(|| self.canned.get(key_for_system_prompt(&system)).cloned())
            .unwrap_or_else(|| panic!("no canned reply"));
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

fn spec_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/chat-admin-web.json")
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
    let scratch = std::env::temp_dir().join(format!("qt-rt-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    let main = scratch.join("main.db");
    let mount = scratch.join("mount.db");
    std::fs::copy(fixtures_dir().join("chat-admin-main.db"), &main).unwrap();
    std::fs::copy(fixtures_dir().join("chat-admin-mount.db"), &mount).unwrap();
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
    serde_json::to_string_pretty(&sorted(v)).unwrap()
}
fn first_diff(got: &str, want: &str) -> String {
    let g: Vec<&str> = got.lines().collect();
    let w: Vec<&str> = want.lines().collect();
    for i in 0..g.len().max(w.len()) {
        let gi = g.get(i).copied().unwrap_or("<none>");
        let wi = w.get(i).copied().unwrap_or("<none>");
        if gi != wi {
            return format!("  GOT : {gi}\n  WANT: {wi}\n");
        }
    }
    "(identical line-by-line)".to_string()
}

fn status_body(r: &Response) -> (u16, Value) {
    match r {
        Response::ChatAdmin(v) => (200, v.clone()),
        Response::Error(e) => {
            let status = match e.kind {
                ErrorKind::BadRequest => 400,
                ErrorKind::NotFound => 404,
                ErrorKind::Internal => 500,
                _ => 500,
            };
            (status, json!({ "error": e.message }))
        }
        other => (500, serde_json::to_value(other).unwrap()),
    }
}

/// The chat row's title-bearing columns (the oracle's `readChat`).
fn dump_chat(db: &Db, chat_id: &str) -> Value {
    let cid = chat_id.to_string();
    match db
        .read_main(move |c| quilltap_core::db::chats_read::find_by_id(c, &cid))
        .unwrap()
    {
        Some(chat) => json!({
            "id": chat.get("id").cloned().unwrap_or(Value::Null),
            "title": chat.get("title").cloned().unwrap_or(Value::Null),
            "isManuallyRenamed": chat.get("isManuallyRenamed").cloned().unwrap_or(Value::Null),
            "updatedAt": chat.get("updatedAt").cloned().unwrap_or(Value::Null),
        }),
        None => Value::Null,
    }
}

#[test]
fn chat_regenerate_title_matches_oracle() {
    let Some(oracle_path) = env_or_skip("QT_ORACLE_REGENERATE_TITLE") else {
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
    let now_iso = quilltap_core::clock::iso_from_unix_ms(spec.frozen_now_ms);

    let quoted = Canned {
        content: "  'The Ledger and the Lamp'  ".to_string(),
        prompt_tokens: 10,
        completion_tokens: 6,
    };
    let empty_reply = Canned {
        content: "   ".to_string(),
        prompt_tokens: 10,
        completion_tokens: 1,
    };

    for (name, chat_id, override_reply, throws) in [
        ("regen_title_normal", CHAT, None, false),
        ("regen_title_help", HELP_CHAT, None, false),
        (
            "regen_title_clamped",
            CHAT,
            spec.canned_titles.get("long").cloned(),
            false,
        ),
        ("regen_title_quoted", CHAT, Some(quoted.clone()), false),
        (
            "regen_title_empty_reply",
            CHAT,
            Some(empty_reply.clone()),
            false,
        ),
        ("regen_title_provider_throws", CHAT, None, true),
        ("regen_title_no_messages", EMPTY_CHAT, None, false),
        ("regen_title_chat_missing", MISSING_ID, None, false),
    ] {
        driven.insert(name.to_string());
        let db = fresh_db(&spec, name);
        let provider = CannedTitleProvider {
            canned: spec.canned_titles.clone(),
            override_reply,
            throws,
            seen: Mutex::new(Vec::new()),
        };
        // The executor is the NON-logging one: the fixture has no llm-logs
        // partition and v4's own logging is best-effort, so neither side writes
        // a row and there is nothing to diff there.
        let executor = CheapLlmTaskExecutor::new();
        let r = rt.block_on(chat_admin::chat_regenerate_title(
            &db,
            &spec.user_id,
            chat_id,
            &provider,
            &executor,
            &now_iso,
        ));

        let Some(want) = oracle.get(name) else {
            failed.push(format!("{name}_MISSING_FROM_ORACLE"));
            continue;
        };
        let (status, body) = status_body(&r);
        let want_status = want["status"].as_u64().unwrap() as u16;
        if status != want_status {
            eprintln!("[{name}] STATUS {status} != {want_status}");
            failed.push(format!("{name}_status"));
        }
        for (label, got, wanted) in [
            ("body", body, want["body"].clone()),
            (
                "llmMessages",
                Value::Array(provider.seen.lock().unwrap().clone()),
                want["llmMessages"].clone(),
            ),
            ("chat", dump_chat(&db, chat_id), want["chat"].clone()),
        ] {
            if norm(&got) != norm(&wanted) {
                eprintln!(
                    "[{name} {label}] MISMATCH:\n{}",
                    first_diff(&norm(&got), &norm(&wanted))
                );
                failed.push(format!("{name}_{label}"));
            } else {
                eprintln!("[{name} {label}] OK.");
            }
        }
    }

    // Shape, not a hand-written count.
    let expected: BTreeSet<String> = oracle.keys().cloned().collect();
    let missing: Vec<&String> = expected.difference(&driven).collect();
    let extra: Vec<&String> = driven.difference(&expected).collect();
    assert!(
        missing.is_empty() && extra.is_empty(),
        "case-set drift — oracle-only: {missing:?}; driven-only: {extra:?}"
    );
    assert!(failed.is_empty(), "regenerate-title mismatches: {failed:?}");
}
