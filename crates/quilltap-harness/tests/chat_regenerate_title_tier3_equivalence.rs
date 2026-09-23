//! P4.9E3A TIER-3 (mocked-LLM) differential for the MANUAL
//! `?action=regenerate-title`: `services::chat_admin::chat_regenerate_title` vs
//! v4's REAL `handleRegenerateTitle` through its real chat route. Both sides run
//! over a FRESH copy of the committed `chat-admin-{main,mount}.db` fixture per
//! case, with the SAME canned cheap-LLM reply injected at the model boundary,
//! then diff the response body AND the chat row's `title` /
//! `isManuallyRenamed` / `updatedAt`, AND (P4.D215) every `background_jobs`
//! row.
//!
//! ## P4.D215 — through the auto-title chokepoint (v4 `00c290c9a`)
//!
//! The verb now writes through `apply_auto_title(…, clear_manual_rename: true)`.
//! Six arms only the chokepoint distinguishes: an overruled hand rename, an
//! unchanged title (only the extra patch lands), the story background a CHANGED
//! title queues (bug 163; before the commit this verb never queued one) with
//! and without an overruled rename, an unchanged title queueing nothing, and a
//! help chat queueing nothing. Their state is planted by the ORACLE through v4's
//! real repositories and saved beside the NDJSON as `<oracle>.<case>.{main,
//! mount}.db`; this side opens that seed, so neither side hand-writes v4's row
//! shapes.
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
    /// P4.D215: SQL run on this DB INSIDE the canned call — after the verb's
    /// own reads, before the chokepoint's re-read (the log pins' fault plant).
    mid_call_sql: Option<(Db, &'static str)>,
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
        if let Some((db, sql)) = &self.mid_call_sql {
            let sql = *sql;
            db.write(move |w| {
                w.main().connection().execute_batch(sql)?;
                Ok(())
            })
            .await
            .expect("mid-call plant");
        }
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
            cache_usage: None,
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

/// A fresh copy per case: of the committed pair, or — for a P4.D215 planted
/// case — of the planted seed the oracle saved beside its NDJSON
/// (`<oracle>.<case>.{main,mount}.db`; see the oracle's `CaseSpec.plant`).
fn fresh_db(spec: &Spec, tag: &str, seed: Option<(PathBuf, PathBuf)>) -> Db {
    let scratch = std::env::temp_dir().join(format!("qt-rt-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    let main = scratch.join("main.db");
    let mount = scratch.join("mount.db");
    let (src_main, src_mount) = seed.unwrap_or_else(|| {
        (
            fixtures_dir().join("chat-admin-main.db"),
            fixtures_dir().join("chat-admin-mount.db"),
        )
    });
    std::fs::copy(&src_main, &main)
        .unwrap_or_else(|e| panic!("copy seed {}: {e}", src_main.display()));
    std::fs::copy(&src_mount, &mount).unwrap();
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

/// Every background job, in the oracle's `readJobs` shape (P4.D215 — the
/// story background a changed title now queues, bug 163).
fn dump_jobs(db: &Db) -> Value {
    let mut jobs: Vec<Value> = db
        .read_main(|c| {
            Ok(
                quilltap_core::db::background_jobs::BackgroundJobsRepository::new(c)
                    .find_all()?
                    .into_iter()
                    .map(|j| {
                        let priority = if j.priority.fract() == 0.0 {
                            json!(j.priority as i64)
                        } else {
                            json!(j.priority)
                        };
                        json!({
                            "type": j.job_type,
                            "status": j.status,
                            "priority": priority,
                            "payload": serde_json::from_str::<Value>(&j.payload).unwrap_or(Value::Null),
                        })
                    })
                    .collect(),
            )
        })
        .expect("dump jobs");
    jobs.sort_by_key(|v| v["type"].as_str().unwrap_or("").to_string());
    Value::Array(jobs)
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
    // Before any callsite is touched — see `auto_title_capture`'s module doc.
    auto_title_capture::install();
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
    // Padding INSIDE the quotes — the `dcab791c2` second-trim arm (see the
    // oracle case's comment): the stored title must carry no padding.
    let padded_inside = Canned {
        content: "\" A Padded Title \"".to_string(),
        prompt_tokens: 10,
        completion_tokens: 6,
    };
    let empty_reply = Canned {
        content: "   ".to_string(),
        prompt_tokens: 10,
        completion_tokens: 1,
    };
    // P4.D215: `CHAT`'s committed title, so the chokepoint reads it unchanged.
    let same_title = Canned {
        content: "The Evening Post".to_string(),
        prompt_tokens: 10,
        completion_tokens: 4,
    };
    // The cases whose state the oracle PLANTED through v4's real repositories.
    let planted = [
        "regen_title_overrules_manual",
        "regen_title_queues_background",
        "regen_title_overrules_manual_queues_background",
        "regen_title_unchanged_no_background",
        "regen_title_help_no_background",
    ];

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
            "regen_title_quoted_padded_inside",
            CHAT,
            Some(padded_inside.clone()),
            false,
        ),
        (
            "regen_title_empty_reply",
            CHAT,
            Some(empty_reply.clone()),
            false,
        ),
        ("regen_title_provider_throws", CHAT, None, true),
        ("regen_title_no_messages", EMPTY_CHAT, None, false),
        ("regen_title_chat_missing", MISSING_ID, None, false),
        // ── P4.D215 (v4 `00c290c9a`, bugs 163/164) — the chokepoint arms.
        ("regen_title_overrules_manual", CHAT, None, false),
        (
            "regen_title_unchanged",
            CHAT,
            Some(same_title.clone()),
            false,
        ),
        ("regen_title_queues_background", CHAT, None, false),
        (
            "regen_title_overrules_manual_queues_background",
            CHAT,
            None,
            false,
        ),
        (
            "regen_title_unchanged_no_background",
            CHAT,
            Some(same_title.clone()),
            false,
        ),
        ("regen_title_help_no_background", HELP_CHAT, None, false),
    ] {
        driven.insert(name.to_string());
        let seed = planted.contains(&name).then(|| {
            (
                PathBuf::from(format!("{oracle_path}.{name}.main.db")),
                PathBuf::from(format!("{oracle_path}.{name}.mount.db")),
            )
        });
        let db = fresh_db(&spec, name, seed);
        let provider = CannedTitleProvider {
            canned: spec.canned_titles.clone(),
            override_reply,
            throws,
            seen: Mutex::new(Vec::new()),
            mid_call_sql: None,
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
            ("jobs", dump_jobs(&db), want["jobs"].clone()),
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

// ===========================================================================
// P4.D215 — the verb's three log lines, pinned with a capturing layer
// ===========================================================================
//
// v4 `title.ts` logs three lines the port never carried before this lane:
// `[Chats v1] Title generation failed` `{chatId, error}` (ERROR, `:72`),
// `[Chats v1] Title regenerated` `{chatId, newTitle, outcome}` (INFO, `:89` —
// `outcome` is NEW at `00c290c9a`), and `[Chats v1] Error regenerating title`
// `{chatId}` + the error (ERROR, `:93`). A log line writes no row, so the proof
// is a capture over the REAL verb on the committed fixture, each line on its
// own branch and silent on the siblings.

// The capture rig (see its module doc for why not `CaptureLayer`).
mod auto_title_capture;

fn capture_regen(
    tag: &str,
    chat_id: &str,
    reply: Option<Canned>,
    throws: bool,
    mid_call_sql: Option<&'static str>,
) -> (Vec<String>, u16) {
    let (lines, status, _) = capture_regen_with(tag, chat_id, reply, throws, None, mid_call_sql);
    (lines, status)
}

/// [`capture_regen`] with a `pre_sql` plant (run before the verb's own reads)
/// and the response body (the `00c290c9a` unification's review arms).
fn capture_regen_with(
    tag: &str,
    chat_id: &str,
    reply: Option<Canned>,
    throws: bool,
    pre_sql: Option<&'static str>,
    mid_call_sql: Option<&'static str>,
) -> (Vec<String>, u16, Value) {
    let spec: Spec = serde_json::from_str(&std::fs::read_to_string(spec_path()).unwrap()).unwrap();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let db = fresh_db(&spec, tag, None);
    if let Some(sql) = pre_sql {
        rt.block_on(db.write(move |w| {
            w.main().connection().execute_batch(sql)?;
            Ok(())
        }))
        .expect("pre-call plant");
    }
    let provider = CannedTitleProvider {
        canned: spec.canned_titles.clone(),
        override_reply: reply,
        throws,
        seen: Mutex::new(Vec::new()),
        mid_call_sql: mid_call_sql.map(|sql| (db.clone(), sql)),
    };
    let now_iso = quilltap_core::clock::iso_from_unix_ms(spec.frozen_now_ms);
    let ((status, body), out) = auto_title_capture::capture(|| {
        let r = rt.block_on(chat_admin::chat_regenerate_title(
            &db,
            &spec.user_id,
            chat_id,
            &provider,
            &CheapLlmTaskExecutor::new(),
            &now_iso,
        ));
        status_body(&r)
    });
    (out, status, body)
}

fn one<'a>(lines: &'a [String], needle: &str) -> &'a String {
    let hits: Vec<&String> = lines.iter().filter(|l| l.contains(needle)).collect();
    assert_eq!(
        hits.len(),
        1,
        "expected exactly one {needle:?} line: {lines:#?}"
    );
    hits[0]
}
fn none(lines: &[String], needle: &str) {
    assert!(
        !lines.iter().any(|l| l.contains(needle)),
        "expected NO {needle:?} line: {lines:#?}"
    );
}

#[test]
fn regenerate_title_log_lines_fire_on_their_own_branches() {
    auto_title_capture::install();
    // (a) A changed title: INFO with the new `outcome` field.
    let (lines, status) = capture_regen("log_applied", CHAT, None, false, None);
    assert_eq!(status, 200);
    let done = one(&lines, "[Chats v1] Title regenerated");
    assert!(done.starts_with("INFO "), "{done}");
    assert!(done.contains(&format!("chatId={CHAT}")), "{done}");
    assert!(done.contains("outcome=applied"), "{done}");
    assert!(done.contains("newTitle="), "{done}");
    one(&lines, "[Auto Title] Chat retitled");
    let retitled = one(&lines, "[Auto Title] Chat retitled");
    assert!(retitled.contains("source=regenerate"), "{retitled}");
    none(&lines, "Title generation failed");
    none(&lines, "Error regenerating title");

    // (b) The same title: still 200, still logged, `outcome=unchanged`.
    let (lines, status) = capture_regen(
        "log_unchanged",
        CHAT,
        Some(Canned {
            content: "The Evening Post".to_string(),
            prompt_tokens: 10,
            completion_tokens: 4,
        }),
        false,
        None,
    );
    assert_eq!(status, 200);
    let done = one(&lines, "[Chats v1] Title regenerated");
    assert!(done.contains("outcome=unchanged"), "{done}");
    assert!(done.contains("newTitle=The Evening Post"), "{done}");
    none(&lines, "[Auto Title] Chat retitled");

    // (c) The provider throws: the ERROR carries the task's error, and neither
    //     success line fires.
    let (lines, status) = capture_regen("log_failed", CHAT, None, true, None);
    assert_eq!(status, 500);
    let failed = one(&lines, "[Chats v1] Title generation failed");
    assert!(failed.starts_with("ERROR "), "{failed}");
    assert!(failed.contains(&format!("chatId={CHAT}")), "{failed}");
    assert!(failed.contains("error="), "{failed}");
    none(&lines, "[Chats v1] Title regenerated");
    none(&lines, "Error regenerating title");

    // (d) The chokepoint's re-read fails (the `chats` table is gone by the time
    //     the LLM answers): v4's outer catch — the ERROR, the 500, and no
    //     success line.
    let (lines, status, body) = capture_regen_with(
        "log_error",
        CHAT,
        None,
        false,
        None,
        Some("DROP TABLE chats;"),
    );
    assert_eq!(status, 500);
    assert_eq!(body, json!({ "error": "Failed to regenerate title" }));
    let err = one(&lines, "[Chats v1] Error regenerating title");
    assert!(err.starts_with("ERROR "), "{err}");
    assert!(err.contains(&format!("chatId={CHAT}")), "{err}");
    assert!(err.contains("error="), "{err}");
    none(&lines, "[Chats v1] Title regenerated");
    none(&lines, "Title generation failed");

    // (e) The `00c290c9a` unification's review: a repository read BEFORE the
    //     LLM call fails — v4's ONE outer catch (`title.ts:92-94`) spans it
    //     too, so it is the same ERROR and the same 500 body, not the raw
    //     `DbError` text unlogged (red-first: 500 with `no such table` in the
    //     body and no line).
    let (lines, status, body) = capture_regen_with(
        "log_read_error",
        CHAT,
        None,
        false,
        Some(r#"ALTER TABLE "connection_profiles" RENAME TO "connection_profiles_poisoned";"#),
        None,
    );
    assert_eq!(status, 500);
    assert_eq!(body, json!({ "error": "Failed to regenerate title" }));
    let err = one(&lines, "[Chats v1] Error regenerating title");
    assert!(err.starts_with("ERROR "), "{err}");
    assert!(err.contains("no such table"), "{err}");
    none(&lines, "[Chats v1] Title regenerated");

    // (f) The `missing` outcome (the review: the lane had no arm for it) — the
    //     chat is deleted while the LLM answers, so the chokepoint's re-read
    //     finds nothing: DEBUG `Chat vanished…`, nothing written, and the body
    //     is STILL v4's `{success, title}` with `outcome=missing` on the INFO.
    let (lines, status, body) = capture_regen_with(
        "log_missing",
        CHAT,
        None,
        false,
        None,
        // `CHAT`, spelled out: the plant is a `&'static str`.
        Some("DELETE FROM chats WHERE id = 'c1000000-0000-4000-8000-000000000001';"),
    );
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["success"], json!(true), "{body}");
    assert!(
        body["title"].as_str().is_some_and(|t| !t.is_empty()),
        "{body}"
    );
    let gone = one(
        &lines,
        "[Auto Title] Chat vanished before title could be applied",
    );
    assert!(gone.starts_with("DEBUG "), "{gone}");
    assert!(gone.contains("source=regenerate"), "{gone}");
    let done = one(&lines, "[Chats v1] Title regenerated");
    assert!(done.contains("outcome=missing"), "{done}");
    none(&lines, "[Auto Title] Chat retitled");
    none(&lines, "Error regenerating title");
}
