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
//! P4.D44 extended the fixture with three roleplay templates plus the two
//! defaults that name them (`chat_settings.defaultRoleplayTemplateId` and the
//! Lantern project's `defaultRoleplayTemplateId`), and added the five `rt_*`
//! cases mirroring v4's `route.roleplay-template.test.ts`. Because the user
//! default is now set, EVERY case bakes a non-null `chats.roleplayTemplateId` —
//! that is the default chain being walked, proven on both sides.
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

mod sampling_capture;

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
    canned_stream_key, CannedStreamingProvider, StreamChunk, StreamChunkResult, StreamUsage,
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
    /// P4.D79: the CUMULATIVE reasoning the greeting stream emits; the last
    /// non-empty one is persisted onto the stored greeting message.
    #[serde(default)]
    greeting_reasoning: Vec<String>,
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
    /// P4.D83: the sampling knobs v4's REAL `autoGenerateFirstMessage` resolved
    /// off the profile's bag for this call (absent keys = knobs left undefined).
    #[serde(default)]
    sampling: Value,
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

/// Sort a table's rows by an id-free key, or leave as-is.
///
/// ⚠ Neither key may be an id. The created chat / participant / message ids are
/// minted at random on each side, so an id sort orders the two dumps
/// differently the moment a table holds more than one row — and P4.D148's
/// fixture bakes a SOURCE chat (the continuation case's previous chapter)
/// alongside the created one. `createdAt` is the stable discriminator: the
/// baked rows carry pinned timestamps that precede the create's wall clock, and
/// within one create the insertion sequence is the same on both sides, so it
/// also breaks the tie between a replayed message and the original it was
/// copied from (identical `role` + `content`, different clock).
fn sort_rows(table: &str, rows: &mut [Value]) {
    let text = |r: &Value, k: &str| -> String {
        r.get(k).and_then(Value::as_str).unwrap_or("").to_string()
    };
    match table {
        "chat_messages" => rows.sort_by_key(|r| {
            (
                text(r, "systemSender"),
                text(r, "role"),
                text(r, "content"),
                text(r, "createdAt"),
            )
        }),
        "chats" => rows.sort_by_key(|r| (text(r, "createdAt"), text(r, "title"))),
        _ => {}
    }
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

/// The LITERAL `roleplayTemplateId` of every `chats` row, in row order, with no
/// normalization whatsoever.
///
/// ⚠ This exists because [`normalize`] would otherwise make the P4.D44 arms
/// unfalsifiable. Every template id is a UUID, and `normalize` remaps UUIDs to
/// first-appearance tokens — so a row carrying the PROJECT default and a row
/// carrying an EXPLICITLY PICKED template both render as the same `<uN>` when
/// the id appears nowhere else in the dump. Mutation testing caught exactly
/// that: swapping the project/user precedence, and making the explicit key stop
/// winning, both left the `chats` section byte-identical. The fixture bakes
/// these ids on BOTH sides (they are not minted), so comparing them literally is
/// exact — the remap only ever needed to cover minted values.
/// (P4.D148: reads the rows through [`table_rows`] so it inherits the same
/// id-free `chats` ordering every other comparand uses — an id sort would put
/// the baked source chat and the freshly minted one in a different order on
/// each side.)
fn chat_template_ids(dump: &Value) -> Value {
    Value::Array(
        table_rows("chats", dump)
            .as_array()
            .map(|rows| {
                rows.iter()
                    .map(|r| r.get("roleplayTemplateId").cloned().unwrap_or(Value::Null))
                    .collect()
            })
            .unwrap_or_default(),
    )
}

/// P4.D148 — the INSERTION-ORDERED message trace, the position proof.
///
/// `tables.chatMessages` is sorted on BOTH sides (by id in the oracle, by
/// `(systemSender, role, content)` here), so it can say WHICH rows exist but
/// never WHERE one landed. The create-time Concierge flip is a claim about
/// position and nothing else: its bubble must sit after the SYSTEM prompt and
/// before the continuation replay / the staff whispers / the greeting. `rowid`
/// is insertion order on both sides (neither ever deletes a message row), so
/// this projection — the same six TEXT columns the oracle reads, in `rowid`
/// order — is what makes that claim falsifiable.
fn dump_message_order(db: &Db) -> Value {
    db.read_main(|conn| {
        let mut stmt = conn.prepare(
            "SELECT \"chatId\", \"type\", \"role\", \"systemSender\", \"systemKind\", \"content\" \
             FROM chat_messages ORDER BY rowid",
        )?;
        let rows: Vec<Value> = stmt
            .query_map([], |row| {
                let mut cells: Vec<Value> = Vec::with_capacity(6);
                for i in 0..6 {
                    cells.push(match row.get::<_, Option<String>>(i)? {
                        Some(s) => Value::String(s),
                        None => Value::Null,
                    });
                }
                Ok(Value::Array(cells))
            })?
            .collect::<Result<_, _>>()?;
        Ok(Value::Array(rows))
    })
    .expect("dump message order")
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
        let mut expected_stream_sampling: HashMap<String, Value> = HashMap::new();
        for rec in &recordings {
            let msgs = to_messages(&rec.messages);
            if rec.kind == "stream" {
                expected_stream_sampling.insert(
                    canned_stream_key(&rec.provider, &rec.model, rec.temperature, &msgs),
                    if rec.sampling.is_object() {
                        rec.sampling.clone()
                    } else {
                        Value::Object(serde_json::Map::new())
                    },
                );
            }
            if rec.kind == "stream" {
                let mut chunks: Vec<StreamChunkResult> = Vec::new();
                for r in &c.greeting_reasoning {
                    chunks.push(Ok(StreamChunk {
                        reasoning_content: Some(r.clone()),
                        ..Default::default()
                    }));
                }
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
        // P4.D83: record what the PORT put on the greeting stream.
        let streaming =
            sampling_capture::SamplingStreamCapture::new(streaming, |provider, params| {
                canned_stream_key(
                    provider,
                    &params.model,
                    params.temperature,
                    &params.messages,
                )
            });
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

        let create_result = rt.block_on(handle_create(
            &db,
            main_w.connection(),
            mount_w.connection(),
            Some(llm_w.connection()),
            &deps,
            &request,
            &emitter,
        ));

        // Dump a MAIN-db table via a fresh read-only pooled connection (sees
        // every committed write from both the Writer conns and the Db writer
        // thread). Hoisted above the reject branch so a 400 can prove it wrote
        // NOTHING rather than only that it answered 400.
        let dump = |t: &str| -> Value {
            let t = t.to_string();
            let t2 = t.clone();
            db.read_main(move |conn| dump_table_json_conn(conn, &t, "id"))
                .unwrap_or_else(|e| panic!("dump {t2}: {e}"))
        };

        // P4.9E3B: the request-level rejection arms (a bad / explicit-null
        // timestampConfig — v4's route-level Zod parse → 400 `Validation
        // error`; P4.D148 adds the `conciergeState` pair). The oracle row's
        // status selects the branch; the Zod `details` array stays the standing
        // error-envelope deferral.
        let want_status = want["status"].as_u64().unwrap_or(201);
        if want_status != 201 {
            let err = match create_result {
                Err(e) => e,
                Ok(_) => panic!(
                    "case {}: v4 answered {want_status} but the port succeeded",
                    c.name
                ),
            };
            let (got_status, got_msg) = match &err {
                quilltap_core::services::chat_create::HandleCreateError::BadRequest(m) => {
                    (400u64, m.clone())
                }
                other => (500u64, other.to_string()),
            };
            assert_eq!(
                got_status, want_status,
                "case {}: status {got_status} != {want_status}",
                c.name
            );
            let want_error = want["body"]["error"].as_str().unwrap_or_default();
            assert_eq!(got_msg, want_error, "case {}: error copy mismatch", c.name);

            // A refusal must also have written nothing: v4's Zod parse rejects
            // before `repos.chats.create` is reached (its own suite asserts
            // exactly that), so the persisted state must still be the baked
            // fixture on both sides. Without this the reject arms proved only
            // the status + sentence, and a port that refused AFTER writing
            // would have passed.
            let reject_norm = |v: &Value| -> String {
                let mut v = v.clone();
                canon_numbers(&mut v);
                normalize(&serde_json::to_string_pretty(&sorted(&v)).unwrap())
            };
            for (name, got_v, want_v) in [
                (
                    "chats",
                    table_rows("chats", &dump("chats")),
                    table_rows("chats", &want["tables"]["chats"]),
                ),
                (
                    "chat_messages",
                    table_rows("chat_messages", &dump("chat_messages")),
                    table_rows("chat_messages", &want["tables"]["chatMessages"]),
                ),
            ] {
                assert_eq!(
                    reject_norm(&got_v),
                    reject_norm(&want_v),
                    "case {}: reject arm wrote a different `{name}` than v4",
                    c.name
                );
            }
            eprintln!(
                "OK: chat-create case {} matched oracle (reject arm + wrote nothing).",
                c.name
            );
            drop(db);
            drop(main_w);
            drop(mount_w);
            drop(llm_w);
            let _ = std::fs::remove_dir_all(&scratch);
            continue;
        }

        let ChatCreateResult {
            mut chat,
            participants,
        } = create_result.unwrap_or_else(|e| panic!("case {}: handle_create failed: {e}", c.name));

        // Build the 201 DTO body: chat.participants := enriched summaries.
        if let Value::Object(map) = &mut chat {
            map.insert(
                "participants".into(),
                serde_json::to_value(&participants).unwrap(),
            );
        }
        let got_dto = json!({ "chat": chat });

        // Dump the four MAIN-db tables (the `dump` closure is defined above the
        // reject branch, which uses it too).
        let got_chats = dump("chats");
        let got_msgs = dump("chat_messages");
        let got_projects = dump("projects");
        let got_bg = dump("background_jobs");

        // P4.D148: the insertion-ordered message trace (position proof).
        let got_message_order = dump_message_order(&db);

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

        let sections: [(&str, Value, Value); 7] = [
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
                "message_order",
                got_message_order.clone(),
                want["messageOrder"].clone(),
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

        // P4.D44: the un-normalized template-id proof (see `chat_template_ids`).
        // Compared BEFORE the normalizing sections so a template-resolution
        // divergence is reported as itself rather than hiding behind `<uN>`.
        let got_tpl = chat_template_ids(&got_chats);
        let want_tpl = chat_template_ids(&want["tables"]["chats"]);
        if got_tpl != want_tpl {
            eprintln!(
                "[{}] section `roleplay_template_id` MISMATCH:\n  GOT : {got_tpl}\n  WANT: {want_tpl}",
                c.name
            );
            failed.push("roleplay_template_id".to_string());
        } else {
            eprintln!(
                "[{}] section `roleplay_template_id` OK ({got_tpl}).",
                c.name
            );
        }

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
        // --- the greeting's SAMPLING knobs AT THE WIRE (P4.D83) ---
        // Only cases that actually stream a greeting have anything to compare;
        // the rest record nothing, and asserting on them would be vacuous.
        if recordings.iter().any(|r| r.kind == "stream") {
            streaming.assert_matches(&expected_stream_sampling, &format!("greeting ({})", c.name));
        }

        eprintln!(
            "OK: chat-create case {} matched oracle (all sections).",
            c.name
        );
    }
}
