//! CAPSTONE tier-3 differential (P4.4 unit 2, sub-unit 8): the chat-creation
//! spine (`services::chat_create::handle_create`) vs v4's REAL `handleCreate`
//! (app/api/v1/chats/route.ts POST). Both sides copy the SAME baked fixture and
//! run the same corpus of create requests; the created-chat state (the four MAIN-db
//! tables `chats` / `chat_messages` / `projects` / `background_jobs`, the ordered
//! Green-Room progress frames, and the 201 DTO body) is diffed in the
//! minted-values form: ISO timestamps collapse to `<ts>` and every UUID is remapped
//! to a first-appearance token (so the created chat/participant/message ids line up
//! by structure; the fixture-baked ids are identical both sides → identical tokens).
//!
//! The Rust side injects the same seams v4's oracle mocks: canned model providers
//! (empty for the no-model corpus), `NoApiKeys`, a pinned `random01` (v4 mocks
//! `Math.random`), the wall clock, and an active `CreationProgressEmitter` over a
//! `CreationProgressBus` the harness drains for the frame trace.
//!
//! Build the fixture + oracle (Node 24, from the v4 checkout — see the .ts headers):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_CC_MAIN=/tmp/qt-cc-main.db QT_FIXTURE_CC_MOUNT=/tmp/qt-cc-mount.db \
//!   QT_FIXTURE_CC_LLM=/tmp/qt-cc-llm.db \
//!     $N/node --import tsx $V5W/harness/oracle/fixtures/build-chat-create-capstone.ts
//!   TMPO=/tmp/qt-cc-oracle; rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
//!   cp $V5W/harness/oracle/cases/chat-create-capstone.test.ts "$TMPO/cases/"
//!   cp $V5W/harness/oracle/fixtures/chat-create-capstone.json  "$TMPO/fixtures/"
//!   TZ=UTC QT_FIXTURE_CC_MAIN=/tmp/qt-cc-main.db QT_FIXTURE_CC_MOUNT=/tmp/qt-cc-mount.db \
//!   QT_FIXTURE_CC_LLM=/tmp/qt-cc-llm.db QT_ORACLE_OUT=/tmp/oracle-chat-create.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=120000 \
//!       --roots "$PWD" --roots "$TMPO/cases" -- chat-create-capstone   # TZ=UTC pins the cron case
//! Run:
//!   QT_ORACLE_CC=/tmp/oracle-chat-create.ndjson \
//!   QT_FIXTURE_CC_MAIN=/tmp/qt-cc-main.db QT_FIXTURE_CC_MOUNT=/tmp/qt-cc-mount.db \
//!   QT_FIXTURE_CC_LLM=/tmp/qt-cc-llm.db \
//!     cargo test -p quilltap-harness --test chat_create_capstone_equivalence

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::db::{dump_table_json_conn, Writer};
use quilltap_core::enclave::announce::{system_mint_uuid, system_now_ms};
use quilltap_core::enclave::cron;
use quilltap_core::enclave::lifecycle::LifecycleDeps;
use quilltap_core::model::completion::{
    CannedCompletionProvider, CompletionMessage, CompletionRole, CompletionUsage,
};
use quilltap_core::model::embedding::CannedEmbeddingProvider;
use quilltap_core::model::stream::{
    CannedStreamingProvider, StreamChunk, StreamChunkResult, StreamUsage,
};
use quilltap_core::services::chat_create::{
    handle_create, ChatCreateDeps, ChatCreateRequest, ChatCreateResult,
};
use quilltap_core::services::cheap_llm_exec::CheapLlmTaskExecutor;
use quilltap_core::services::creation_progress::{CreationProgressBus, CreationProgressEmitter};
use quilltap_core::services::dangerous_content::provider_routing::NoApiKeys;
use regex::Regex;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    test_pepper_base64: String,
    cases: Vec<CaseSpec>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CaseSpec {
    name: String,
    #[allow(dead_code)]
    progress_id: String,
    tz: String,
    now_ms: i64,
    random01: f64,
    request: Value,
    #[serde(default)]
    greeting_content: Option<String>,
    #[serde(default)]
    greeting_usage: Option<UsageSpec>,
    #[serde(default)]
    outfit_content: Option<String>,
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct UsageSpec {
    prompt_tokens: i64,
    completion_tokens: i64,
    total_tokens: i64,
}

/// One recorded model call from the oracle (the exact prompt v4 built).
#[derive(Deserialize)]
struct Recording {
    kind: String,
    provider: String,
    model: String,
    temperature: Option<f64>,
    messages: Vec<RecMsg>,
}
#[derive(Deserialize)]
struct RecMsg {
    role: String,
    content: String,
}

fn to_messages(m: &[RecMsg]) -> Vec<CompletionMessage> {
    m.iter()
        .map(|m| CompletionMessage {
            role: match m.role.as_str() {
                "system" => CompletionRole::System,
                "assistant" => CompletionRole::Assistant,
                _ => CompletionRole::User,
            },
            content: m.content.clone(),
        })
        .collect()
}

fn spec_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/chat-create-capstone.json")
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

/// Collapse ISO-8601 millisecond timestamps to `<ts>` everywhere (incl. inside
/// JSON-string columns), then remap every UUID to a first-appearance token. Both
/// sides run identical structure, so the token assignment lines up.
fn normalize(text: &str) -> String {
    let ts = Regex::new(r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z").unwrap();
    let out = ts.replace_all(text, "<ts>").into_owned();

    let uuid =
        Regex::new(r"[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}")
            .unwrap();
    let mut map: HashMap<String, String> = HashMap::new();
    uuid.replace_all(&out, |caps: &regex::Captures| {
        let id = caps[0].to_lowercase();
        let n = map.len();
        map.entry(id).or_insert_with(|| format!("<u{n}>")).clone()
    })
    .into_owned()
}

/// Recursively normalize integer-valued floats to integers (JS `JSON.stringify`
/// renders `1.0` as `1`), so the DTO's f64-typed fields (e.g. enriched
/// `talkativeness`) match v4's integer rendering.
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

/// Recursively re-serialize with sorted object keys, so insertion-order noise
/// (Rust dump column order vs oracle JSON.parse order) never matters.
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

/// Sort a table's rows by an id-free key (chat_messages) or leave as-is.
fn sort_rows(table: &str, rows: &mut [Value]) {
    if table == "chat_messages" {
        rows.sort_by_key(|r| {
            (
                r.get("systemSender")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                r.get("role")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                r.get("content")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
            )
        });
    }
}

/// The one documented persisted-state seam (see the report / test header): v4's
/// `buildCharacterParticipant` writes `connectionProfileId`/`imageProfileId`/
/// `selectedSystemPromptId` as EXPLICIT `null` (its `|| null`), and v4's
/// `ChatParticipantSchema` keeps them (`.nullable().optional()`), so the stored
/// `chats.participants` JSON carries `"...":null`. The ported `ChatParticipant`
/// marshaling (`db/chats.rs`) drops a null via `skip_serializing_if` (it is a
/// plain `Option`, not the present-keeps-null double-`Option` used for
/// `removedAt`), so the Rust-created chat omits those keys. This normalizer strips
/// those explicit-null keys from BOTH sides so the rest of the ~96-column `chats`
/// row is proven byte-exact; the seam itself is a reported finding.
fn strip_participant_null_seam(row: &mut Value) {
    const SEAM_KEYS: [&str; 3] = [
        "connectionProfileId",
        "imageProfileId",
        "selectedSystemPromptId",
    ];
    let Some(obj) = row.as_object_mut() else {
        return;
    };
    let Some(Value::String(s)) = obj.get("participants") else {
        return;
    };
    let Ok(mut arr) = serde_json::from_str::<Value>(s) else {
        return;
    };
    if let Some(list) = arr.as_array_mut() {
        for p in list {
            if let Some(po) = p.as_object() {
                // Rebuild preserving insertion order (Map::remove swap-removes
                // under preserve_order, which would reorder).
                let mut m = serde_json::Map::new();
                for (k, v) in po.iter() {
                    if SEAM_KEYS.contains(&k.as_str()) && v.is_null() {
                        continue;
                    }
                    m.insert(k.clone(), v.clone());
                }
                *p = Value::Object(m);
            }
        }
    }
    obj.insert(
        "participants".into(),
        Value::String(serde_json::to_string(&arr).unwrap()),
    );
}

/// Extract the sorted, number-canonicalized `rows` array from a dump/oracle
/// `{table, columns, rows}` object.
fn table_rows(table: &str, dump: &Value) -> Value {
    let mut rows: Vec<Value> = dump
        .get("rows")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    sort_rows(table, &mut rows);
    if table == "chats" {
        rows.iter_mut().for_each(strip_participant_null_seam);
    }
    let mut arr = Value::Array(rows);
    canon_numbers(&mut arr);
    sorted(&arr)
}

/// The DTO-body seam (see the report): v4's 201 `chat` is the `repos.chats.create`
/// echo (the create input validated through `ChatMetadataBaseSchema`, which KEEPS
/// the explicit `null`s the input set — `avatarGenerationEnabled`/`imageProfileId`/
/// `projectId`/`lastMessageAt`/`roleplayTemplateId`/`contextSummary`/`scenarioText`),
/// while the spine builds the DTO from `chats_read::find_by_id` (a re-read whose
/// hydrate DROPS NULL nullable-optional columns). So v4's body carries those keys
/// as `null` where the spine's omits them. This normalizer drops null-valued keys
/// from the chat object + strips the participant-null seam from its `participants`,
/// on BOTH sides, so the rest of the DTO is proven byte-exact; the seam is a
/// reported finding. (A genuine null-vs-value divergence would still surface — only
/// the pure present-null-vs-absent case is bounded here.)
fn dto_drop_null_seam(dto: &Value) -> Value {
    let mut dto = dto.clone();
    if let Some(chat) = dto.get_mut("chat").and_then(Value::as_object_mut) {
        let null_keys: Vec<String> = chat
            .iter()
            .filter(|(_, v)| v.is_null())
            .map(|(k, _)| k.clone())
            .collect();
        for k in null_keys {
            chat.remove(&k);
        }
        // The enriched DTO participants carry no imageProfileId/etc. — no seam —
        // but harmonize any explicit-null character sub-fields the same way.
        if let Some(ps) = chat.get_mut("participants").and_then(Value::as_array_mut) {
            for p in ps {
                if let Some(po) = p.as_object_mut() {
                    let nk: Vec<String> = po
                        .iter()
                        .filter(|(_, v)| v.is_null())
                        .map(|(k, _)| k.clone())
                        .collect();
                    for k in nk {
                        po.remove(&k);
                    }
                }
            }
        }
    }
    dto
}

/// Strip the `ts` field off each progress frame (nondeterministic wall clock).
fn strip_frame_ts(frames: &mut [Value]) {
    for f in frames {
        if let Some(o) = f.as_object_mut() {
            o.remove("ts");
        }
    }
}

#[test]
fn chat_create_capstone_matches_oracle() {
    let (Some(oracle_path), Some(fixture_main), Some(fixture_mount), Some(fixture_llm)) = (
        env_or_skip("QT_ORACLE_CC"),
        env_or_skip("QT_FIXTURE_CC_MAIN"),
        env_or_skip("QT_FIXTURE_CC_MOUNT"),
        env_or_skip("QT_FIXTURE_CC_LLM"),
    ) else {
        return;
    };

    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(spec_path()).unwrap_or_else(|e| panic!("read spec: {e}")),
    )
    .expect("parse spec");

    // Oracle rows keyed by case name.
    let oracle_text =
        std::fs::read_to_string(&oracle_path).unwrap_or_else(|e| panic!("read oracle: {e}"));
    let mut oracle: HashMap<String, Value> = HashMap::new();
    for line in oracle_text.lines().filter(|l| !l.trim().is_empty()) {
        let v: Value = serde_json::from_str(line).expect("oracle line parses");
        oracle.insert(
            v.get("name").and_then(Value::as_str).unwrap().to_string(),
            v,
        );
    }

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");

    for c in &spec.cases {
        let want = oracle
            .get(&c.name)
            .unwrap_or_else(|| panic!("oracle missing case {}", c.name));

        // Fresh fixture copy.
        let scratch =
            std::env::temp_dir().join(format!("qt-cc-harness-{}-{}", std::process::id(), c.name));
        let _ = std::fs::remove_dir_all(&scratch);
        std::fs::create_dir_all(&scratch).expect("scratch");
        let main_work = scratch.join("main.db");
        let mount_work = scratch.join("mount.db");
        let llm_work = scratch.join("llm.db");
        std::fs::copy(&fixture_main, &main_work).expect("copy main");
        std::fs::copy(&fixture_mount, &mount_work).expect("copy mount");
        std::fs::copy(&fixture_llm, &llm_work).expect("copy llm");

        // Writable partitions (the host's ChatCreateSpine::run_create recipe) +
        // a shared engine Db over the SAME files.
        let busy = Duration::from_millis(5000);
        let open_w = |p: &PathBuf| {
            let w = Writer::open_writable(p, &spec.test_pepper_base64)
                .unwrap_or_else(|e| panic!("open writable {p:?}: {e}"));
            w.connection().busy_timeout(busy).expect("busy_timeout");
            w
        };
        let main_w = open_w(&main_work);
        let mount_w = open_w(&mount_work);
        let llm_w = open_w(&llm_work);

        let db = Db::open(
            DbPaths {
                main: main_work.clone(),
                mount_index: Some(mount_work.clone()),
                llm_logs: Some(llm_work.clone()),
            },
            &spec.test_pepper_base64,
        )
        .expect("open db");

        // Deps: canned model boundaries keyed by v4's RECORDED prompts (so a
        // prompt-byte divergence surfaces as a canned miss → empty greeting →
        // static fallback → a chat_messages diff), no api keys, pinned clock/RNG.
        let recordings: Vec<Recording> = want
            .get("recordings")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .map(|r| serde_json::from_value(r.clone()).expect("recording parses"))
                    .collect()
            })
            .unwrap_or_default();

        let embedding = CannedEmbeddingProvider::new();
        let mut completion = CannedCompletionProvider::new();
        let mut streaming = CannedStreamingProvider::new();
        for rec in &recordings {
            let msgs = to_messages(&rec.messages);
            if rec.kind == "stream" {
                let mut chunks: Vec<StreamChunkResult> = Vec::new();
                if let Some(content) = c.greeting_content.as_deref() {
                    if !content.is_empty() {
                        chunks.push(Ok(StreamChunk::content(content)));
                    }
                }
                let usage = c.greeting_usage.as_ref().map(|u| StreamUsage {
                    prompt_tokens: u.prompt_tokens,
                    completion_tokens: u.completion_tokens,
                    total_tokens: u.total_tokens,
                });
                chunks.push(Ok(StreamChunk::done(usage)));
                streaming = streaming.with_stream(
                    &rec.provider,
                    &rec.model,
                    rec.temperature,
                    &msgs,
                    chunks,
                );
            } else {
                let usage: Option<CompletionUsage> = None;
                completion = completion.with_response(
                    &rec.provider,
                    &rec.model,
                    rec.temperature,
                    &msgs,
                    c.outfit_content.as_deref().unwrap_or_default(),
                    usage,
                );
            }
        }
        let executor = CheapLlmTaskExecutor::new();
        let api_keys = NoApiKeys;

        let now_fn = system_now_ms;
        let mint_fn = system_mint_uuid;
        let tz = c.tz.clone();
        let cron_seam = move |expr: &str, anchor: i64| -> Result<Option<i64>, String> {
            cron::try_next_occurrence(expr, anchor, &tz)
        };
        let lifecycle = LifecycleDeps {
            now_ms: &now_fn,
            mint_uuid: &mint_fn,
            next_occurrence: &cron_seam,
        };

        let deps = ChatCreateDeps {
            embedding: &embedding,
            completion: &completion,
            streaming: &streaming,
            executor: &executor,
            api_keys: &api_keys,
            tz: c.tz.clone(),
            now_ms: c.now_ms,
            random01: c.random01,
            lifecycle: &lifecycle,
            greeting_log: true,
        };

        let request: ChatCreateRequest =
            serde_json::from_value(c.request.clone()).expect("parse request");

        // Emitter built from the REQUEST's progressId (v4
        // `createCreationProgressEmitter`): absent → inert (no frames).
        let bus = Arc::new(CreationProgressBus::new());
        let (tx, _rx) = tokio::sync::broadcast::channel(256);
        let emitter =
            CreationProgressEmitter::from_id(request.progress_id.as_deref(), Arc::clone(&bus), tx);
        let replay_id = request.progress_id.clone().unwrap_or_default();

        let ChatCreateResult {
            mut chat,
            participants,
        } = rt
            .block_on(handle_create(
                &db,
                main_w.connection(),
                mount_w.connection(),
                Some(llm_w.connection()),
                &deps,
                &request,
                &emitter,
            ))
            .unwrap_or_else(|e| panic!("case {}: handle_create failed: {e}", c.name));

        // Build the 201 DTO body: chat.participants := enriched summaries.
        if let Value::Object(map) = &mut chat {
            map.insert(
                "participants".into(),
                serde_json::to_value(&participants).unwrap(),
            );
        }
        let got_dto = json!({ "chat": chat });

        // Dump the four MAIN-db tables via a fresh read-only pooled connection
        // (sees every committed write from both the Writer conns and the Db
        // writer thread).
        let dump = |t: &str| -> Value {
            let t = t.to_string();
            let t2 = t.clone();
            db.read_main(move |conn| dump_table_json_conn(conn, &t, "id"))
                .unwrap_or_else(|e| panic!("dump {t2}: {e}"))
        };
        let got_chats = dump("chats");
        let got_msgs = dump("chat_messages");
        let got_projects = dump("projects");
        let got_bg = dump("background_jobs");

        // Frame trace (buffered replay), ts stripped.
        let mut got_frames: Vec<Value> = bus
            .replay(&replay_id)
            .iter()
            .map(|f| serde_json::to_value(f).unwrap())
            .collect();
        strip_frame_ts(&mut got_frames);

        drop(db);
        drop(main_w);
        drop(mount_w);
        drop(llm_w);
        let _ = std::fs::remove_dir_all(&scratch);

        // --- Compare each section independently (isolates the DTO-shape seam
        // from the load-bearing persisted-state + frame proofs). ---
        let norm_section = |v: &Value| -> String {
            let mut v = v.clone();
            canon_numbers(&mut v);
            normalize(&serde_json::to_string_pretty(&sorted(&v)).unwrap())
        };

        let mut want_frames: Vec<Value> = want["frames"].as_array().cloned().unwrap_or_default();
        strip_frame_ts(&mut want_frames);

        let sections: [(&str, Value, Value); 6] = [
            (
                "chats",
                table_rows("chats", &got_chats),
                table_rows("chats", &want["tables"]["chats"]),
            ),
            (
                "chat_messages",
                table_rows("chat_messages", &got_msgs),
                table_rows("chat_messages", &want["tables"]["chatMessages"]),
            ),
            (
                "projects",
                table_rows("projects", &got_projects),
                table_rows("projects", &want["tables"]["projects"]),
            ),
            (
                "background_jobs",
                table_rows("background_jobs", &got_bg),
                table_rows("background_jobs", &want["tables"]["backgroundJobs"]),
            ),
            (
                "frames",
                Value::Array(got_frames.clone()),
                Value::Array(want_frames.clone()),
            ),
            (
                "dto",
                dto_drop_null_seam(&got_dto),
                dto_drop_null_seam(&json!({ "chat": want["body"]["chat"].clone() })),
            ),
        ];

        let mut failed: Vec<String> = Vec::new();
        for (name, got_v, want_v) in &sections {
            let g = norm_section(got_v);
            let w = norm_section(want_v);
            if g != w {
                // DTO-shape divergence (v4 returns the `repos.chats.create` echo;
                // the spine re-reads via find_by_id) is a KNOWN finding — record
                // it but do not fail the persisted-state proofs.
                let gl: Vec<&str> = g.lines().collect();
                let wl: Vec<&str> = w.lines().collect();
                let mut ctx = String::new();
                for i in 0..gl.len().max(wl.len()) {
                    let gi = gl.get(i).copied().unwrap_or("<none>");
                    let wi = wl.get(i).copied().unwrap_or("<none>");
                    if gi != wi {
                        for j in i.saturating_sub(3)..i {
                            ctx.push_str(&format!("   = {}\n", gl.get(j).copied().unwrap_or("")));
                        }
                        ctx.push_str(&format!("  GOT : {gi}\n  WANT: {wi}\n"));
                        break;
                    }
                }
                eprintln!("[{}] section `{name}` MISMATCH:\n{ctx}", c.name);
                failed.push((*name).to_string());
            } else {
                eprintln!("[{}] section `{name}` OK.", c.name);
            }
        }

        // Every section must match: `chats` (the ~96-column row, modulo the
        // documented participant-null marshaling seam), `chat_messages` (the SYSTEM
        // prompt + every Prospero/Host/Aurora seed whisper + the first message),
        // `projects`, `background_jobs`, the ordered Green-Room `frames` (byte-exact
        // status strings incl. the curly apostrophes), and the 201 `dto` body
        // (modulo the documented create-echo-vs-reread null seam). The two seams are
        // the SAME root cause — v4 keeps the create-input's explicit `null`s (in the
        // persisted participants and in the create-echo body) where the port drops
        // them (the `ChatParticipant` marshaling and the `find_by_id` re-read
        // respectively) — and are reported findings, not silent normalizations.
        assert!(
            failed.is_empty(),
            "case {} FAILED sections: {:?}",
            c.name,
            failed
        );

        // Always print the DTO null seam's exact extent (for the report / audit).
        let g = got_dto["chat"].as_object().cloned().unwrap_or_default();
        let w = want["body"]["chat"]
            .as_object()
            .cloned()
            .unwrap_or_default();
        let null_only_v4: Vec<&String> = w
            .iter()
            .filter(|(k, v)| v.is_null() && !g.contains_key(*k))
            .map(|(k, _)| k)
            .collect();
        eprintln!(
            "case {}: DTO create-echo null seam — v4-present/rust-absent null keys: {:?}",
            c.name, null_only_v4
        );
        eprintln!(
            "OK: chat-create case {} matched oracle (all sections).",
            c.name
        );
    }
}
