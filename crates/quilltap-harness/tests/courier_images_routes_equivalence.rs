//! P4.6ab COURIER + CHAT-IMAGES route-surface differential: `api::chat_media::*`
//! vs v4's REAL Salon courier/image route handlers. Both sides read a FRESH copy
//! of the committed `courier-images-{main,mount}.db` fixture per case (baked ids
//! identical → no remap; the save-image `linkId` and the add-tool-result message
//! `id`/`createdAt` are minted → blanked).
//!
//! The resolve case uses a [`CannedCompletionProvider`] + a fresh
//! [`CheapLlmTaskExecutor`]: the fixture's pending placeholder resolves NO
//! connection profile, so the three post-turn triggers are skipped (v4
//! `if (connectionProfile)`) and the completion/executor are never invoked — the
//! route's synchronous contract (the settle writes + the envelope) is what's pinned.
//!
//! The AT-CADENCE resolve case (`resolve_cadence`) is the courier-path
//! fold-episode pin: `CHAT_CADENCE`'s placeholder resolves the fixture's
//! connection profile at post-resolve interchange 11, so the three triggers
//! FIRE — the memory-extraction enqueue lands in `background_jobs`, the danger
//! trigger bails (mode defaults OFF), and the summary check folds turns 1–5,
//! running the summary fold AND the fold-episode pass through
//! [`FoldEpisodePassSeams`](quilltap_core::services::context_summary::FoldEpisodePassSeams)
//! — the fold + episode SUMMARIZATION rows plus the fold tail's
//! TITLE_GENERATION row (and its title write), diffed against the oracle over
//! the committed EMPTY `courier-images-llmlogs.db` partition. The completions are
//! canned from the oracle's recorded keys; the logging executor mirrors the
//! host spine's construction (chat-scoped, UNtagged). The four cross-subsystem
//! fold arms stay no-ops on both sides (the oracle jest-mocks exactly those —
//! the standing orchestrator-family deferral).
//! Save-image reads the ingested image via a canned [`FileBytesStore`] returning the
//! fixture's recorded STORED bytes (the write internals are separately proven by
//! `photo_tools_equivalence`; here we diff the route wrapper's response envelope).
//!
//! Generate the oracle (Node 24, from the v4 checkout — see the .ts header):
//!   … QT_ORACLE_OUT=/tmp/oracle-courier-images.ndjson npx jest -- courier-images-routes
//! Run:
//!   QT_ORACLE_COURIER_IMAGES=/tmp/oracle-courier-images.ndjson \
//!     cargo test -p quilltap-harness --test courier_images_routes_equivalence

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use quilltap_core::api::chat_media;
use quilltap_core::api::types::{ErrorKind, Response};
use quilltap_core::db::files::FileEntry;
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::model::completion::{
    CannedCompletionProvider, CompletionMessage, CompletionRole, CompletionUsage,
};
use quilltap_core::model::embedding::CannedEmbeddingProvider;
use quilltap_core::photos::save_image_to_album::{
    FileBytesStore, IngestImageRequest, NoSideEffects, SaveImageSideEffects,
};
use quilltap_core::services::cheap_llm_exec::CheapLlmLogConfig;
use quilltap_core::services::cheap_llm_exec::CheapLlmTaskExecutor;
use quilltap_core::services::llm_logging::LogContext;
use serde::Deserialize;
use serde_json::{json, Value};

const USER: &str = "e18e05bc-63e8-4539-8a85-719b7a508850";
const GENERAL_MP: &str = "b0000000-0000-4000-8000-0000000000a0";
const CHAT_PENDING: &str = "c1000000-0000-4000-8000-000000000001";
const CHAT_IMG: &str = "c1000000-0000-4000-8000-000000000002";
const MSG_PENDING: &str = "d1000000-0000-4000-8000-000000000001";
const MSG_SETTLED: &str = "d1000000-0000-4000-8000-000000000002";
const MSG_IMG: &str = "d2000000-0000-4000-8000-000000000001";
const CF_NOTES: &str = "f1000000-0000-4000-8000-000000000001";
const CHAT_CADENCE: &str = "c1000000-0000-4000-8000-000000000004";
const MSG_CAD_PENDING: &str = "d4000000-0000-4000-8000-000000000022";
const NOW_MS: i64 = 1_775_779_200_000; // 2026-04-10T00:00:00.000Z

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    test_pepper_base64: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Meta {
    #[allow(dead_code)]
    lora_vault: String,
    riya_vault: String,
    #[allow(dead_code)]
    proj_mp: String,
    img1_file_id: String,
    img1_bytes: Vec<u8>,
}

fn spec_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/courier-images-web.json")
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

/// A canned bytes store: returns the fixture's recorded stored bytes for the
/// ingested image, and refuses ingest (the file exists → no fallback).
struct CannedBytes {
    file_id: String,
    bytes: Vec<u8>,
}
impl FileBytesStore for CannedBytes {
    fn read_image_buffer(&self, entry: &FileEntry) -> Result<Option<Vec<u8>>, String> {
        if entry.id == self.file_id {
            Ok(Some(self.bytes.clone()))
        } else {
            Ok(None)
        }
    }
    fn ingest_image_buffer(&self, _req: &IngestImageRequest) -> Result<FileEntry, String> {
        Err("ingest not available in the differential".to_string())
    }
}

/// Minimal standard-alphabet base64 decode (the oracle emits the image bytes b64).
fn base64_decode(s: &str) -> Vec<u8> {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut lut = [255u8; 256];
    for (i, &c) in T.iter().enumerate() {
        lut[c as usize] = i as u8;
    }
    let mut out = Vec::new();
    let mut buf = 0u32;
    let mut bits = 0;
    for &c in s.as_bytes() {
        if c == b'=' || c == b'\n' || c == b'\r' {
            continue;
        }
        let v = lut[c as usize];
        if v == 255 {
            continue;
        }
        buf = (buf << 6) | v as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
        }
    }
    out
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
/// Blank the minted keys (save-image linkId; add-tool-result message id/createdAt).
fn blank_minted(v: &mut Value) {
    if let Value::Object(o) = v {
        for k in ["linkId", "id", "createdAt"] {
            if o.contains_key(k) {
                o.insert(k.to_string(), Value::String(format!("<{k}>")));
            }
        }
        o.iter_mut().for_each(|(_, x)| blank_minted(x));
    } else if let Value::Array(a) = v {
        a.iter_mut().for_each(blank_minted);
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

/// The web-edge (status, body) for a Response (matches the HTTP transport mapping).
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

fn fresh_db(spec: &Spec, tag: &str) -> Db {
    fresh_db_inner(spec, tag, false)
}

/// The at-cadence resolve's variant: also copies the committed EMPTY llm-logs
/// partition so the fold + episode SUMMARIZATION rows land somewhere diffable.
fn fresh_db_with_llm_logs(spec: &Spec, tag: &str) -> Db {
    fresh_db_inner(spec, tag, true)
}

fn fresh_db_inner(spec: &Spec, tag: &str, llm_logs: bool) -> Db {
    let scratch = std::env::temp_dir().join(format!("qt-ci-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    let main = scratch.join("main.db");
    let mount = scratch.join("mount.db");
    std::fs::copy(fixtures_dir().join("courier-images-main.db"), &main).unwrap();
    std::fs::copy(fixtures_dir().join("courier-images-mount.db"), &mount).unwrap();
    let ll = if llm_logs {
        let ll = scratch.join("llm-logs.db");
        std::fs::copy(fixtures_dir().join("courier-images-llmlogs.db"), &ll).unwrap();
        Some(ll)
    } else {
        None
    };
    Db::open(
        DbPaths {
            main,
            mount_index: Some(mount),
            llm_logs: ll,
        },
        &spec.test_pepper_base64,
    )
    .expect("open db")
}

/// The persisted message row (or null) + the chat's settle flags, mirroring the
/// oracle's `tables` dump.
fn dump_message_and_chat(db: &Db, chat_id: &str, message_id: &str) -> Value {
    let cid = chat_id.to_string();
    let messages = db
        .read_main(move |c| quilltap_core::db::chats_messages_read::get_messages(c, &cid))
        .unwrap();
    let message = messages
        .into_iter()
        .find(|m| m.get("id").and_then(Value::as_str) == Some(message_id))
        .unwrap_or(Value::Null);
    let cid = chat_id.to_string();
    let chat = db
        .read_main(move |c| quilltap_core::db::chats_read::find_by_id(c, &cid))
        .unwrap();
    let flags = chat
        .map(|c| {
            json!({
                "isPaused": c.get("isPaused").and_then(Value::as_bool).unwrap_or(false),
                "courierCheckpoints": c.get("courierCheckpoints").cloned().unwrap_or(Value::Null),
                "lastMessageAt": c.get("lastMessageAt").cloned().unwrap_or(Value::Null),
            })
        })
        .unwrap_or(Value::Null);
    json!({ "message": message, "chat": flags })
}

/// The at-cadence chat's summary-bearing columns — mirrors the oracle's
/// `readCadenceChat` (volatile `updatedAt` deliberately omitted; the fold
/// stamps it from the wall clock here, from the frozen clock in v4).
fn dump_cadence_chat(db: &Db, chat_id: &str) -> Value {
    let cid = chat_id.to_string();
    let chat = db
        .read_main(move |c| quilltap_core::db::chats_read::find_by_id(c, &cid))
        .unwrap();
    chat.map(|c| {
        let g = |k: &str| c.get(k).cloned().unwrap_or(Value::Null);
        let gn = |k: &str, d: i64| {
            c.get(k)
                .filter(|v| !v.is_null())
                .cloned()
                .unwrap_or_else(|| Value::Number(d.into()))
        };
        json!({
            "title": g("title"),
            "isPaused": c.get("isPaused").and_then(Value::as_bool).unwrap_or(false),
            "courierCheckpoints": g("courierCheckpoints"),
            "lastMessageAt": g("lastMessageAt"),
            "contextSummary": g("contextSummary"),
            "compactionGeneration": gn("compactionGeneration", 0),
            "lastSummaryTurn": gn("lastSummaryTurn", 0),
            "lastFullRebuildTurn": gn("lastFullRebuildTurn", 0),
            "lastRenameCheckInterchange": gn("lastRenameCheckInterchange", 0),
            "summaryAnchorMessageIds": c.get("summaryAnchorMessageIds").cloned().unwrap_or(json!([])),
        })
    })
    .unwrap_or(Value::Null)
}

/// The context-summary chat event(s) the fold appended (minted id/createdAt
/// blanked by the shared normalization).
fn dump_context_summary_events(db: &Db, chat_id: &str) -> Value {
    let cid = chat_id.to_string();
    let messages = db
        .read_main(move |c| quilltap_core::db::chats_messages_read::get_messages(c, &cid))
        .unwrap();
    Value::Array(
        messages
            .into_iter()
            .filter(|m| m.get("type").and_then(Value::as_str) == Some("context-summary"))
            .collect(),
    )
}

/// background_jobs rows over stable columns (payload parsed) — the oracle's
/// `readBackgroundJobsStable`.
fn dump_background_jobs_stable(db: &Db) -> Value {
    db.read_main(|conn| {
        let mut stmt = conn
            .prepare("SELECT type, status, userId, payload FROM background_jobs ORDER BY type")?;
        let rows = stmt
            .query_map([], |r| {
                let payload: Option<String> = r.get(3)?;
                Ok(json!({
                    "type": r.get::<_, String>(0)?,
                    "status": r.get::<_, String>(1)?,
                    "userId": r.get::<_, String>(2)?,
                    "payload": payload
                        .and_then(|p| serde_json::from_str::<Value>(&p).ok())
                        .unwrap_or(Value::Null),
                }))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok::<_, quilltap_core::db::DbError>(Value::Array(rows))
    })
    .unwrap()
}

/// llm_logs rows over stable columns (id / timestamps / durationMs excluded;
/// JSON columns parsed), sorted canonically — the oracle's `readLlmLogsStable`.
fn dump_llm_logs_stable(db: &Db) -> Value {
    db.read_llm_logs(|conn| {
        let mut stmt = conn.prepare(
            "SELECT type, userId, chatId, messageId, characterId, autonomousRunId, \
             provider, modelName, request, response, usage FROM llm_logs",
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
                    "autonomousRunId": r.get::<_, Option<String>>(5)?,
                    "provider": r.get::<_, Option<String>>(6)?,
                    "modelName": r.get::<_, Option<String>>(7)?,
                    "request": parse(r.get::<_, Option<String>>(8)?),
                    "response": parse(r.get::<_, Option<String>>(9)?),
                    "usage": parse(r.get::<_, Option<String>>(10)?),
                }))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        rows.sort_by_key(|r| serde_json::to_string(r).unwrap());
        Ok::<_, quilltap_core::db::DbError>(Value::Array(rows))
    })
    .unwrap()
}

/// The chat's linked files (id, originalFilename) ordered by id — the oracle's dump.
fn dump_chat_files(db: &Db, chat_id: &str) -> Value {
    let cid = chat_id.to_string();
    db.read_main(move |conn| {
        let mut stmt = conn.prepare(
            "SELECT id, originalFilename FROM files \
             WHERE EXISTS (SELECT 1 FROM json_each(files.linkedTo) WHERE value = ?1) ORDER BY id",
        )?;
        let rows = stmt
            .query_map(rusqlite::params![cid], |r| {
                Ok(json!({ "id": r.get::<_, String>(0)?, "originalFilename": r.get::<_, String>(1)? }))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok::<_, quilltap_core::db::DbError>(Value::Array(rows))
    })
    .unwrap()
}

#[test]
fn courier_images_routes_match_oracle() {
    let Some(oracle_path) = env_or_skip("QT_ORACLE_COURIER_IMAGES") else {
        return;
    };
    let spec: Spec = serde_json::from_str(&std::fs::read_to_string(spec_path()).unwrap()).unwrap();
    let meta: Meta = serde_json::from_str(
        &std::fs::read_to_string(fixtures_dir().join("courier-images-main.db.meta.json")).unwrap(),
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

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let mut failed: Vec<String> = Vec::new();
    let kept_at = quilltap_core::clock::iso_from_unix_ms(NOW_MS);

    // status + body checker (errors compared as {error} + status).
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

    // The image bytes saveImageToAlbum reads at case time (emitted by the oracle —
    // the build-time recording can diverge from the case-time readImageBuffer).
    let img_b64 = oracle["image_bytes"]["body"]["base64"].as_str().unwrap();
    let img_bytes = base64_decode(img_b64);
    let _ = &meta.img1_bytes; // (recorded for reference; the oracle bytes are authoritative)
    let bytes: Arc<dyn FileBytesStore> = Arc::new(CannedBytes {
        file_id: meta.img1_file_id.clone(),
        bytes: img_bytes,
    });
    let no_fx: Arc<dyn SaveImageSideEffects + Send + Sync> = Arc::new(NoSideEffects);

    // --- Reads ---
    {
        let db = fresh_db(&spec, "albums");
        check(
            "photo_albums",
            &chat_media::chat_photo_albums(&db, CHAT_IMG),
            None,
        );
    }
    {
        let db = fresh_db(&spec, "flist");
        check(
            "files_list",
            &chat_media::chat_files_list(&db, CHAT_IMG),
            None,
        );
    }

    // --- Save image (character-vault attribution) ---
    {
        let db = fresh_db(&spec, "saveriya");
        let body = json!({ "fileId": meta.img1_file_id, "mountPointId": meta.riya_vault, "caption": "A quiet scene" });
        let r = rt.block_on(chat_media::message_save_image(
            &db,
            USER,
            CHAT_IMG,
            MSG_IMG,
            &body,
            bytes.clone(),
            no_fx.clone(),
            &kept_at,
        ));
        check("save_image_riya", &r, None);
    }
    // --- Save image (user-persona attribution → General) ---
    {
        let db = fresh_db(&spec, "savegen");
        let body = json!({ "fileId": meta.img1_file_id, "mountPointId": GENERAL_MP });
        let r = rt.block_on(chat_media::message_save_image(
            &db,
            USER,
            CHAT_IMG,
            MSG_IMG,
            &body,
            bytes.clone(),
            no_fx.clone(),
            &kept_at,
        ));
        check("save_image_general", &r, None);
    }
    // --- Save image error: not attached ---
    {
        let db = fresh_db(&spec, "savena");
        let body = json!({ "fileId": "99999999-9999-4999-8999-999999999999", "mountPointId": meta.riya_vault });
        let r = rt.block_on(chat_media::message_save_image(
            &db,
            USER,
            CHAT_IMG,
            MSG_IMG,
            &body,
            bytes.clone(),
            no_fx.clone(),
            &kept_at,
        ));
        check("save_image_not_attached", &r, None);
    }

    // --- Add tool result (user + character) ---
    {
        let db = fresh_db(&spec, "atru");
        let body = json!({
            "tool": "generate_image",
            "initiatedBy": "user",
            "prompt": "a lantern-lit alley",
            "images": [{ "id": meta.img1_file_id, "filename": "scene.png" }],
        });
        let r = rt.block_on(chat_media::chat_add_tool_result(&db, USER, CHAT_IMG, &body));
        check("add_tool_result_user", &r, None);
    }
    {
        let db = fresh_db(&spec, "atrc");
        let body = json!({
            "tool": "generate_image",
            "initiatedBy": "character",
            "prompt": "a foggy harbor",
            "result": { "success": true },
            "images": [{ "id": meta.img1_file_id, "filename": "scene.png" }],
        });
        let r = rt.block_on(chat_media::chat_add_tool_result(&db, USER, CHAT_IMG, &body));
        check("add_tool_result_character", &r, None);
    }

    // --- Resolve external turn (no profile → triggers skipped) ---
    {
        let db = fresh_db(&spec, "resolve");
        let completion = CannedCompletionProvider::new();
        let embedding = CannedEmbeddingProvider::new();
        let executor = CheapLlmTaskExecutor::new();
        let r = rt.block_on(chat_media::message_resolve_external_turn(
            &db,
            &completion,
            &embedding,
            &executor,
            USER,
            CHAT_PENDING,
            MSG_PENDING,
            "Lora nods and steps forward.",
            NOW_MS,
        ));
        let tables = dump_message_and_chat(&db, CHAT_PENDING, MSG_PENDING);
        check("resolve", &r, Some(tables));
    }
    {
        let db = fresh_db(&spec, "resolvenp");
        let completion = CannedCompletionProvider::new();
        let embedding = CannedEmbeddingProvider::new();
        let executor = CheapLlmTaskExecutor::new();
        let r = rt.block_on(chat_media::message_resolve_external_turn(
            &db,
            &completion,
            &embedding,
            &executor,
            USER,
            CHAT_PENDING,
            MSG_SETTLED,
            "noop",
            NOW_MS,
        ));
        check("resolve_not_pending", &r, None);
    }

    // --- Resolve external turn AT CADENCE (profile resolves → triggers fire;
    //     the summary gate folds turns 1–5 and runs the fold-episode pass +
    //     the fold tail's title generation: three llm_logs rows + the
    //     memory-extraction enqueue + the title write) ---
    {
        #[derive(Deserialize)]
        struct CannedMsg {
            role: String,
            content: String,
        }
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct CannedUsage {
            prompt_tokens: i64,
            completion_tokens: i64,
            total_tokens: i64,
        }
        #[derive(Deserialize)]
        struct CannedEntry {
            provider: String,
            model: String,
            temperature: Option<f64>,
            messages: Vec<CannedMsg>,
            response: String,
            #[serde(default)]
            usage: Option<CannedUsage>,
        }
        let canned: Vec<CannedEntry> =
            serde_json::from_value(oracle["resolve_cadence"]["canned"].clone())
                .expect("resolve_cadence canned entries");
        let mut completion = CannedCompletionProvider::new();
        for entry in &canned {
            let messages: Vec<CompletionMessage> = entry
                .messages
                .iter()
                .map(|m| CompletionMessage {
                    role: match m.role.as_str() {
                        "system" => CompletionRole::System,
                        "assistant" => CompletionRole::Assistant,
                        _ => CompletionRole::User,
                    },
                    content: m.content.clone(),
                })
                .collect();
            completion = completion.with_response(
                &entry.provider,
                &entry.model,
                entry.temperature,
                &messages,
                &entry.response,
                entry.usage.as_ref().map(|u| CompletionUsage {
                    prompt_tokens: u.prompt_tokens,
                    completion_tokens: u.completion_tokens,
                    total_tokens: u.total_tokens,
                }),
            );
        }

        let db = fresh_db_with_llm_logs(&spec, "resolvecad");
        let embedding = CannedEmbeddingProvider::new();
        // The logging executor mirrors the host spine's construction for this
        // route (chat-scoped, message-less, UNtagged LogContext) — the fold +
        // episode rows it writes are the diffed evidence.
        let executor = CheapLlmTaskExecutor::with_logging(CheapLlmLogConfig {
            db: db.clone(),
            user_id: USER.to_string(),
            chat_id: Some(CHAT_CADENCE.to_string()),
            message_id: None,
            ctx: LogContext::none(),
        });
        let r = rt.block_on(chat_media::message_resolve_external_turn(
            &db,
            &completion,
            &embedding,
            &executor,
            USER,
            CHAT_CADENCE,
            MSG_CAD_PENDING,
            "Lora pins the final lantern code to the board.",
            NOW_MS,
        ));
        let tables = json!({
            "message": dump_message_and_chat(&db, CHAT_CADENCE, MSG_CAD_PENDING)["message"],
            "chat": dump_cadence_chat(&db, CHAT_CADENCE),
            "contextSummaryEvents": dump_context_summary_events(&db, CHAT_CADENCE),
            "backgroundJobs": dump_background_jobs_stable(&db),
            "llmLogs": dump_llm_logs_stable(&db),
        });
        check("resolve_cadence", &r, Some(tables));
    }

    // --- Cancel external turn ---
    {
        let db = fresh_db(&spec, "cancel");
        let r = rt.block_on(chat_media::message_cancel_external_turn(
            &db,
            CHAT_PENDING,
            MSG_PENDING,
        ));
        let tables = dump_message_and_chat(&db, CHAT_PENDING, MSG_PENDING);
        check("cancel", &r, Some(tables));
    }

    // --- Chat-file delete ---
    {
        let db = fresh_db(&spec, "fdel");
        let r = rt.block_on(chat_media::chat_file_delete(&db, USER, CF_NOTES));
        let tables = json!({ "files": dump_chat_files(&db, CHAT_IMG) });
        check("file_delete", &r, Some(tables));
    }
    {
        let db = fresh_db(&spec, "fdelm");
        let r = rt.block_on(chat_media::chat_file_delete(
            &db,
            USER,
            "99999999-9999-4999-8999-999999999999",
        ));
        check("file_delete_missing", &r, None);
    }

    assert!(
        failed.is_empty(),
        "courier-images-routes FAILED: {failed:?}"
    );
}
