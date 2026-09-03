//! P4.9E4A attach-mount-file differential: `api::chat_media::chat_attach_mount_file`
//! (+ the read-back half in `chat_files_list`) vs v4's REAL
//! `handleAttachMountFile` (`app/api/v1/chats/[id]/files/route.ts:250`). Both
//! sides read a FRESH copy of the committed `attach-file-{main,mount,llmlogs}.db`
//! fixture per case, so every id in the corpus is baked and identical — only the
//! announcement's `id`/`createdAt` are minted, and only those are blanked.
//!
//! ## The tiers in one family
//! Tier 2 for the writes (the Librarian announcement's row, the blob-description
//! cache, the response envelopes, the error arms) and tier 3 for the vision
//! rungs: the SAME canned response is injected on both sides through a
//! [`CannedCompletionProvider`] registered from the oracle's recorded calls
//! ([[tier3-completion-oracle]]), and the resulting `IMAGE_DESCRIPTION` rows are
//! diffed against the committed EMPTY llm-logs partition.
//!
//! The describe runner below is a byte-for-byte restatement of the host's
//! `HostImageDescribeRunner` (`quilltap-host/src/spine.rs`) — the same
//! `FallbackDeps` over the same `generate_image_description`, so the composition
//! under test is production's. The transcoder is
//! [`NotConfiguredTranscoder`]: every corpus image is tiny, so v4's
//! `resizeImageForProvider` early-returns before touching its codec, which is
//! what makes the two sides equivalent here.
//!
//! ## ⚠ [P4.63] The corpus describes through OPENAI, and why that matters
//!
//! This family stood RED from v4 `a14a1811` (2026-08-23) until P4.63 found it,
//! and the failure mode is worth remembering: the corpus's four describer
//! profiles were on `OPENAI_COMPATIBLE`, and bug 91 put
//! `providerCanTransportImages()` in front of every describe attempt — the
//! OpenAI-compatible plugin's shared base declares no attachment support, so
//! v4 began refusing before the provider seam. The oracle then recorded ZERO
//! canned vision calls, `doc_mount_blobs.description` came back `''`, and the
//! `IMAGE_DESCRIPTION` rows stopped being written.
//!
//! **v5 was never wrong here** — it ports the same predicate (P4.D106) and
//! refuses identically, which is exactly why the state diff could not see the
//! problem: both engines went dark together and compared equal. The only thing
//! that noticed was the corpus-shape assert on the canned-call count. The
//! repair is the corpus, not the port: the profiles moved to `OPENAI`, a
//! provider BOTH transport tiers agree on (v4's static mirror, which is what
//! answers under jest where the plugin registry is never initialized, and v5's
//! baked manifest registry). Pick corpus providers that way.
//!
//! Generate the oracle (Node 24, from the v4 checkout — see the .ts header):
//!   … QT_ORACLE_OUT=/tmp/oracle-attach-mount-file.ndjson npx jest -- attach-mount-file
//! Run:
//!   QT_ORACLE_ATTACH_MOUNT_FILE=/tmp/oracle-attach-mount-file.ndjson \
//!     cargo test -p quilltap-harness --test attach_mount_file_equivalence

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use quilltap_core::api::chat_media::{self, ImageDescribeDriver};
use quilltap_core::api::types::{ErrorKind, Response};
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::files::image_processing::NotConfiguredTranscoder;
use quilltap_core::model::completion::{
    CannedCompletionProvider, CompletionAttachment, CompletionMessage, CompletionResponse,
    CompletionUsage,
};
use quilltap_core::services::file_fallback::{
    self, FallbackDeps, FallbackFile, FallbackResult, IMAGE_DESCRIPTION_INSTRUCTION,
};
use serde::Deserialize;
use serde_json::{json, Value};

const CHAT: &str = "c1000000-0000-4000-8000-000000000001";
const MISSING_CHAT: &str = "99999999-9999-4999-8999-999999999999";
/// The wall clock for `logLLMCall`'s `durationMs` — excluded from both dumps.
const NOW_MS: i64 = 1_780_272_000_000;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    test_pepper_base64: String,
    user_id: String,
    reasoning_user_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Meta {
    mount_point_id: String,
}

#[derive(Deserialize)]
struct OracleUsage {
    #[serde(rename = "promptTokens")]
    prompt_tokens: i64,
    #[serde(rename = "completionTokens")]
    completion_tokens: i64,
    #[serde(rename = "totalTokens")]
    total_tokens: i64,
}

#[derive(Deserialize)]
struct CannedRow {
    provider: String,
    model: String,
    temperature: Option<f64>,
    filename: String,
    #[serde(rename = "mimeType")]
    mime_type: String,
    content: String,
    #[serde(rename = "finishReason")]
    finish_reason: Option<String>,
    usage: Option<OracleUsage>,
}

fn spec_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../harness/oracle/fixtures/attach-file.json")
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

// ===========================================================================
// The describe runner (the host's `HostImageDescribeRunner`, restated)
// ===========================================================================

struct TestDescribeRunner {
    db: Db,
    user_id: String,
    completion: Arc<CannedCompletionProvider>,
}

impl ImageDescribeDriver for TestDescribeRunner {
    fn describe<'a>(
        &'a self,
        file: FallbackFile,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = FallbackResult> + Send + 'a>> {
        Box::pin(async move {
            let transcoder = NotConfiguredTranscoder;
            let deps = FallbackDeps {
                db: &self.db,
                completion: &*self.completion,
                transcoder: &transcoder,
                user_id: &self.user_id,
                now_ms: NOW_MS,
            };
            file_fallback::generate_image_description(&deps, &file).await
        })
    }
}

// ===========================================================================
// Normalization
// ===========================================================================

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

/// Blank ONLY what is genuinely minted on this path: every `createdAt`, and the
/// `id` of the announcement envelope (an object whose keys are exactly
/// `{id, createdAt}`). Deliberately narrow — every OTHER id in the corpus (the
/// mount link ids the response and the announcement body carry) is baked into
/// the committed fixture and must be COMPARED, not erased.
fn blank_minted(v: &mut Value) {
    match v {
        Value::Object(o) => {
            let announcement_envelope =
                o.len() == 2 && o.contains_key("id") && o.contains_key("createdAt");
            if announcement_envelope {
                o.insert("id".to_string(), Value::String("<minted-id>".to_string()));
            }
            if o.contains_key("createdAt") {
                o.insert(
                    "createdAt".to_string(),
                    Value::String("<createdAt>".to_string()),
                );
            }
            o.iter_mut().for_each(|(_, x)| blank_minted(x));
        }
        Value::Array(a) => a.iter_mut().for_each(blank_minted),
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
    blank_minted(&mut v);
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

/// The web-edge (status, body) for a Response.
fn status_body(r: &Response) -> (u16, Value) {
    match r {
        Response::ChatMedia(v) => (200, v.clone()),
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

// ===========================================================================
// Fixture + dumps
// ===========================================================================

fn fresh_db(spec: &Spec, tag: &str) -> Db {
    let scratch = std::env::temp_dir().join(format!("qt-af-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    let main = scratch.join("main.db");
    let mount = scratch.join("mount.db");
    let ll = scratch.join("llm-logs.db");
    std::fs::copy(fixtures_dir().join("attach-file-main.db"), &main).unwrap();
    std::fs::copy(fixtures_dir().join("attach-file-mount.db"), &mount).unwrap();
    std::fs::copy(fixtures_dir().join("attach-file-llmlogs.db"), &ll).unwrap();
    Db::open(
        DbPaths {
            main,
            mount_index: Some(mount),
            llm_logs: Some(ll),
        },
        &spec.test_pepper_base64,
    )
    .expect("open db")
}

/// The chat's message events over the oracle's projection.
fn dump_messages(db: &Db, chat_id: &str) -> Value {
    let cid = chat_id.to_string();
    let events = db
        .read_main(move |c| quilltap_core::db::chats_messages_read::get_messages(c, &cid))
        .unwrap();
    Value::Array(
        events
            .into_iter()
            .map(|e| {
                let g = |k: &str| e.get(k).cloned().unwrap_or(Value::Null);
                json!({
                    "type": g("type"),
                    "role": g("role"),
                    "content": g("content"),
                    "opaqueContent": g("opaqueContent"),
                    "attachments": e.get("attachments").cloned().unwrap_or(json!([])),
                    "participantId": g("participantId"),
                    "senderName": g("senderName"),
                    "systemSender": g("systemSender"),
                })
            })
            .collect(),
    )
}

/// The link-side description a describe caches.
fn dump_description(db: &Db, mount_point_id: &str, relative_path: &str) -> Value {
    let (mp, rp) = (mount_point_id.to_string(), relative_path.to_string());
    db.read_mount_index(move |c| {
        quilltap_core::db::doc_mount_blobs::DocMountBlobsRepository::new(c)
            .find_by_mount_point_and_path(&mp, &rp)
    })
    .unwrap()
    .map(|b| json!({ "description": b.description }))
    .unwrap_or(Value::Null)
}

/// llm_logs over stable columns (id / timestamps / durationMs excluded).
fn dump_llm_logs_stable(db: &Db) -> Value {
    db.read_llm_logs(|conn| {
        let mut stmt = conn.prepare(
            "SELECT type, userId, chatId, messageId, characterId, provider, modelName, \
             request, response, usage FROM llm_logs",
        )?;
        let parse = |s: Option<String>| {
            s.and_then(|v| serde_json::from_str::<Value>(&v).ok())
                .unwrap_or(Value::Null)
        };
        let mut rows = stmt
            .query_map([], |r| {
                Ok(json!({
                    "type": r.get::<_, Option<String>>(0)?,
                    "userId": r.get::<_, Option<String>>(1)?,
                    "chatId": r.get::<_, Option<String>>(2)?,
                    "messageId": r.get::<_, Option<String>>(3)?,
                    "characterId": r.get::<_, Option<String>>(4)?,
                    "provider": r.get::<_, Option<String>>(5)?,
                    "modelName": r.get::<_, Option<String>>(6)?,
                    "request": parse(r.get::<_, Option<String>>(7)?),
                    "response": parse(r.get::<_, Option<String>>(8)?),
                    "usage": parse(r.get::<_, Option<String>>(9)?),
                }))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        rows.sort_by_key(|r| serde_json::to_string(r).unwrap());
        Ok::<_, quilltap_core::db::DbError>(Value::Array(rows))
    })
    .unwrap()
}

// ===========================================================================
// The test
// ===========================================================================

#[test]
fn attach_mount_file_matches_oracle() {
    let Some(oracle_path) = env_or_skip("QT_ORACLE_ATTACH_MOUNT_FILE") else {
        return;
    };
    let spec: Spec = serde_json::from_str(&std::fs::read_to_string(spec_path()).unwrap()).unwrap();
    let meta: Meta = serde_json::from_str(
        &std::fs::read_to_string(fixtures_dir().join("attach-file-main.db.meta.json")).unwrap(),
    )
    .unwrap();

    let mut oracle: HashMap<String, Value> = HashMap::new();
    for line in std::fs::read_to_string(&oracle_path)
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
    {
        let v: Value = serde_json::from_str(line).unwrap();
        oracle.insert(v["name"].as_str().unwrap().to_string(), v);
    }

    // The canned vision provider, registered from the calls v4 actually answered.
    let canned_rows: Vec<CannedRow> =
        serde_json::from_value(oracle["canned"]["canned"].clone()).expect("canned rows parse");
    assert!(
        canned_rows.len() == 6,
        "the corpus must carry all six vision calls (primary / refusal / \
         uncensored retry / reasoning / the bug-116 unseen pair); got {}.\n\
         ⚠ Zero recorded calls means v4 refused every describe attempt BEFORE \
         reaching the provider seam, and the whole vision half of this family \
         is measuring nothing while every table still compares equal (both \
         sides go dark together). That is what happened between v4 \
         `a14a1811` (bug 91) and P4.63: the corpus described through an \
         `OPENAI_COMPATIBLE` profile, and bug 91 put \
         `providerCanTransportImages()` in front of `describeImageWithProfile`, \
         where the OpenAI-compatible plugin declares no attachment support. If \
         this fires again, check the corpus provider against BOTH transport \
         tiers (v4's static mirror, which is what answers under jest, and v5's \
         baked manifest registry) before suspecting the port.",
        canned_rows.len()
    );

    // …and the same claim from the other end, so a corpus that stopped
    // exercising the ladder cannot pass on the canned count alone: each vision
    // rung must have LOGGED its call. Both engines writing nothing compares
    // equal, so this is the only side of the diff that can tell.
    for (case, want_rows) in [
        ("attach_vision", 1usize),
        ("attach_refusal_retry", 2),
        ("attach_reasoning", 1),
        // P4.D151 (bug 116): both describers are asked and both are refused by
        // the arrival verdict, so TWO IMAGE_DESCRIPTION rows are logged (v4
        // logs the call before judging it) and nothing is cached.
        ("attach_verdict_unseen", 2),
    ] {
        let logs = oracle[case]["tables"]["llmLogs"]
            .as_array()
            .map(Vec::len)
            .unwrap_or(0);
        assert_eq!(
            logs, want_rows,
            "[{case}] the oracle carries {logs} IMAGE_DESCRIPTION row(s), the \
             rung is defined by {want_rows} — v4 stopped describing, and the \
             arm is vacuous (see the canned-call note above)"
        );
    }
    let mut completion = CannedCompletionProvider::new();
    for row in canned_rows {
        let messages = vec![CompletionMessage::user(IMAGE_DESCRIPTION_INSTRUCTION)];
        let attachments = vec![CompletionAttachment {
            // Off the canned key (P4.21) — the oracle's recorded keys carry no id.
            id: String::new(),
            filename: row.filename,
            mime_type: row.mime_type,
            data: String::new(),
        }];
        completion = completion.with_response_full(
            &row.provider,
            &row.model,
            row.temperature,
            &messages,
            &attachments,
            CompletionResponse {
                content: row.content,
                usage: row.usage.map(|u| CompletionUsage {
                    prompt_tokens: u.prompt_tokens,
                    completion_tokens: u.completion_tokens,
                    total_tokens: u.total_tokens,
                }),
                finish_reason: row.finish_reason,
                attachment_results: None,
                cache_usage: None,
            },
        );
    }
    let completion = Arc::new(completion);

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let mut failed: Vec<String> = Vec::new();

    let mut check = |name: &str, resp: &Response, tables: Option<Value>| {
        let (status, body) = status_body(resp);
        let want = &oracle[name];
        let want_status = want["status"].as_u64().unwrap() as u16;
        if status != want_status {
            eprintln!("[{name}] STATUS {status} != {want_status}");
            failed.push(format!("{name}_status"));
        }
        if norm(&body) != norm(&want["body"]) {
            eprintln!(
                "[{name}] BODY MISMATCH:\n{}",
                first_diff(&norm(&body), &norm(&want["body"]))
            );
            failed.push(name.to_string());
        } else {
            eprintln!("[{name}] body OK.");
        }
        if let Some(got_tables) = tables {
            let want_tables = &want["tables"];
            if norm(&got_tables) != norm(want_tables) {
                eprintln!(
                    "[{name} tables] MISMATCH:\n{}",
                    first_diff(&norm(&got_tables), &norm(want_tables))
                );
                failed.push(format!("{name}_tables"));
            } else {
                eprintln!("[{name} tables] OK.");
            }
        }
    };

    let mp = meta.mount_point_id.as_str();

    // ── The description ladder, rung by rung ────────────────────────────────
    for (name, tag, rel, user) in [
        (
            "attach_cached",
            "cached",
            "library/described.png",
            spec.user_id.as_str(),
        ),
        (
            "attach_kept_image",
            "kept",
            "photos/kept-lantern.webp",
            spec.user_id.as_str(),
        ),
        (
            "attach_vision",
            "vision",
            "library/undescribed.png",
            spec.user_id.as_str(),
        ),
        (
            "attach_refusal_retry",
            "retry",
            "library/refuses.png",
            spec.user_id.as_str(),
        ),
        (
            "attach_reasoning",
            "reason",
            "library/reasoning.png",
            spec.reasoning_user_id.as_str(),
        ),
        // P4.D151 (v4 `0b0617fee`, bug 116): the describer answers confidently
        // about a picture it never received. The arrival verdict runs ahead of
        // every content check, refuses BOTH describers (each billed below the
        // instruction ceiling), and the invention never reaches
        // `doc_mount_blobs.description` — which is the whole of the bug, since
        // a cached description short-circuits every later reader forever.
        (
            "attach_verdict_unseen",
            "unseen",
            "library/unseen.png",
            spec.user_id.as_str(),
        ),
        (
            "attach_non_image",
            "nonimg",
            "library/ledger.txt",
            spec.user_id.as_str(),
        ),
        // Bug 38: a native-text DOCUMENT (no blob) — attaches via the document
        // fallback + posts the same Librarian announcement, where ghost.md 404s.
        (
            "attach_native_text",
            "nativetext",
            "library/notes.md",
            spec.user_id.as_str(),
        ),
    ] {
        let db = fresh_db(&spec, tag);
        let describe: Arc<dyn ImageDescribeDriver> = Arc::new(TestDescribeRunner {
            db: db.clone(),
            user_id: user.to_string(),
            completion: Arc::clone(&completion),
        });
        let r = rt.block_on(chat_media::chat_attach_mount_file(
            &db,
            Some(&describe),
            CHAT,
            mp,
            rel,
        ));
        let tables = json!({
            "messages": dump_messages(&db, CHAT),
            "blob": dump_description(&db, mp, rel),
            "llmLogs": dump_llm_logs_stable(&db),
        });
        check(name, &r, Some(tables));
    }

    // ── The error arms ──────────────────────────────────────────────────────
    for (name, tag, chat, mount_point, rel) in [
        (
            "attach_missing_file",
            "nofile",
            CHAT,
            mp,
            "library/nope.png",
        ),
        (
            "attach_missing_blob",
            "noblob",
            CHAT,
            mp,
            "library/ghost.md",
        ),
        (
            "attach_no_chat",
            "nochat",
            MISSING_CHAT,
            mp,
            "library/described.png",
        ),
        (
            "attach_missing_mount_point_id",
            "nompid",
            CHAT,
            "",
            "library/described.png",
        ),
        ("attach_missing_relative_path", "norel", CHAT, mp, ""),
    ] {
        let db = fresh_db(&spec, tag);
        let describe: Arc<dyn ImageDescribeDriver> = Arc::new(TestDescribeRunner {
            db: db.clone(),
            user_id: spec.user_id.clone(),
            completion: Arc::clone(&completion),
        });
        let r = rt.block_on(chat_media::chat_attach_mount_file(
            &db,
            Some(&describe),
            chat,
            mount_point,
            rel,
        ));
        check(name, &r, None);
    }

    // ── Double attach: two announcements, ONE `files` entry on the read-back ─
    {
        let db = fresh_db(&spec, "twice");
        let describe: Arc<dyn ImageDescribeDriver> = Arc::new(TestDescribeRunner {
            db: db.clone(),
            user_id: spec.user_id.clone(),
            completion: Arc::clone(&completion),
        });
        let rel = "library/described.png";
        let first = rt.block_on(chat_media::chat_attach_mount_file(
            &db,
            Some(&describe),
            CHAT,
            mp,
            rel,
        ));
        let second = rt.block_on(chat_media::chat_attach_mount_file(
            &db,
            Some(&describe),
            CHAT,
            mp,
            rel,
        ));
        let (first_status, first_body) = status_body(&first);
        assert_eq!(first_status, 200, "the first attach must succeed");
        let (list_status, list_body) = status_body(&chat_media::chat_files_list(&db, CHAT));
        let combined = Response::ChatMedia(json!({
            "first": first_body,
            "second": status_body(&second).1,
        }));
        let tables = json!({
            "messages": dump_messages(&db, CHAT),
            "filesList": list_body,
            "filesListStatus": list_status,
        });
        check("attach_twice", &combined, Some(tables));
    }

    assert!(failed.is_empty(), "attach-mount-file FAILED: {failed:?}");
}

/// The v5-only arm with no v4 counterpart: an assembly with NO
/// [`ImageDescribeDriver`] must still ATTACH — the describe rung resolves to
/// `''`, which is v4's own posture for every describe failure — rather than
/// refuse. Runs off the committed fixture alone, so it never SKIPs.
#[test]
fn attach_without_describe_driver_still_succeeds() {
    let spec: Spec = serde_json::from_str(&std::fs::read_to_string(spec_path()).unwrap()).unwrap();
    let meta: Meta = serde_json::from_str(
        &std::fs::read_to_string(fixtures_dir().join("attach-file-main.db.meta.json")).unwrap(),
    )
    .unwrap();
    let db = fresh_db(&spec, "nodriver");
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let r = rt.block_on(chat_media::chat_attach_mount_file(
        &db,
        None,
        CHAT,
        &meta.mount_point_id,
        "library/undescribed.png",
    ));
    let (status, body) = status_body(&r);
    assert_eq!(status, 200, "the attach must succeed: {body}");

    // The announcement landed, and it carries NO description block (the ladder
    // resolved to '' without a driver) — and nothing was cached on the blob.
    let messages = dump_messages(&db, CHAT);
    let announcement = messages
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["role"] == "ASSISTANT")
        .expect("the Librarian announcement landed");
    let content = announcement["content"].as_str().unwrap();
    assert!(
        content.contains("undescribed.png"),
        "the announcement names the file: {content}"
    );
    assert!(
        !content.contains("describes the illustration thus"),
        "no description block without a driver: {content}"
    );
    assert_eq!(
        dump_description(&db, &meta.mount_point_id, "library/undescribed.png"),
        json!({ "description": "" }),
        "nothing is cached on the blob when the describe never ran"
    );
    assert_eq!(
        dump_llm_logs_stable(&db),
        json!([]),
        "and no IMAGE_DESCRIPTION row is written"
    );
}
