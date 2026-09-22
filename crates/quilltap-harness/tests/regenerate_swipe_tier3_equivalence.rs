//! Tier-3 differential test: **regenerate-swipe** (v4
//! `lib/services/chat-message/regenerate-swipe.service.ts` `regenerateMessageAsSwipe`,
//! ported as `quilltap_core::services::regenerate_swipe`), the sibling entry
//! point to `processMessage`.
//!
//! Both sides copy the same two-DB seed fixture and run the same four-call
//! sequence. The oracle drives v4's REAL `regenerateMessageAsSwipe` with ONLY the
//! model boundaries + buildContext feeders mocked to match the Rust seams; this
//! test composes the ported services through `regenerate_message_as_swipe`,
//! replaying the recorded canned STREAM. Then the five touched tables (`chats`
//! / `chat_messages` / `memories` / `vector_indices` / `vector_entries`) are
//! diffed:
//!   - `chat_messages` — the new swipe's minted id is remapped to a first-appearance
//!     token (seeded ids identical both sides → same token); every other cell
//!     (`createdAt` = the target's SEEDED stamp, the token counts from the fixed
//!     usage, the swipe-group columns) is pinned;
//!   - `chats` — the minted metadata bump (`updatedAt` / `lastMessageAt`) is
//!     placeholdered (v4's frozen+tick vs the Rust real clock);
//!   - `memories` / `vector_entries` — the DELETE cascade's survivors are pinned;
//!   - `vector_indices` — a flush that removed entries mints `updatedAt` (sentinel-
//!     aware collapse).
//!
//! Each call's throw/no-throw is also asserted (the `not_assistant` error path).
//!
//! ## P4.D207 (v4 `f564b0de3`): the generation is a STREAM, and the frames are
//! a comparand
//!
//! Two things moved with `f564b0de3`, and both are measured here:
//!
//! 1. **The persisted row grew three columns.** v5 wrote `rawResponse` /
//!    `reasoningContent` / `thoughtSignature` as NULL under a tracked deferral;
//!    they now ride the chunks. The corpus's canned stream carries all four
//!    chunk fields on purpose — with a STALE `rawResponse` + `reasoningContent`
//!    on the middle chunk, so last-wins is measurable in both, and a NON-EMPTY
//!    terminal `rawResponse`, because an empty object is JS-truthy too and so
//!    could not tell a truthiness bug from a stored-verbatim bug. **This was a
//!    DESIGNED red at the target pin** (three NULLs vs v4's values); it is
//!    re-recorded, never "fixed" back.
//! 2. **An ordered `progress` comparand.** The oracle runs every v4
//!    `onProgress` event through v4's OWN SSE encoders (imported, never
//!    transcribed) and records the decoded frame objects per call; the port's
//!    frames are drained off a real `Event` subscription. So the diff is over
//!    the WIRE bytes the Salon consumes — which is what settles that the
//!    `status` frame carries `kind` (it does).
//!
//! The terminal `{done:true,message}` and `error` frames are deliberately NOT
//! in this family: v4 builds both in the ROUTE, and this oracle drives the
//! SERVICE. They belong to `salon_swipe_generate` and the dispatch wire test.
//!
//! [`a_silent_regeneration_writes_exactly_what_a_narrated_one_writes`] is v4's
//! own invariant as a pin: the same corpus with an INERT emitter writes what
//! v4's narrated run wrote, byte for byte.
//!
//! **TZ=UTC is REQUIRED since P4.d26** (the distill TODAY line renders in the
//! SERVER-LOCAL zone, and the harness pins `server_tz` to "UTC"), so the pin below
//! is load-bearing, not decoration.
//!
//! Generate the fixture + oracle (Node 24, from the v4 checkout; jest ignores
//! `.claude/` paths, so the case is staged in a /tmp mirror). ⚠ The FIXTURE and
//! the ORACLE must come from the SAME tree: the fixture builder runs v4's real
//! repos, so at a pin whose repo layer registers compressed columns the fixture
//! carries brotli cells, and a v5 tree without the codec cannot read them.
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   TMPO=/tmp/qt-regen-oracle
//!   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures" "$TMPO/lib"
//!   cp "$V5W/harness/oracle/cases/regenerate-swipe-tier3.test.ts" "$TMPO/cases/"
//!   cp "$V5W/harness/oracle/lib/pinned-draws.ts" "$TMPO/lib/"
//!   cp "$V5W/harness/oracle/fixtures/regenerate-swipe-tier3.json" "$TMPO/fixtures/"
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-regen-main.db QT_FIXTURE_MOUNT_OUT=/tmp/qt-regen-mount.db \
//!     $N/npx tsx $V5W/harness/oracle/fixtures/build-regenerate-swipe-fixture.ts
//!   QT_FIXTURE_REGEN_MAIN=/tmp/qt-regen-main.db QT_FIXTURE_REGEN_MOUNT=/tmp/qt-regen-mount.db \
//!   TZ=UTC QT_ORACLE_OUT=/tmp/oracle-regenerate-swipe.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=120000 \
//!       --roots "$PWD" --roots "$TMPO/cases" -- regenerate-swipe-tier3
//! Run (`TZ=UTC` is load-bearing — see above; `--test-threads=1` because the
//! neutrality test runs the corpus twice):
//!   QT_ORACLE_REGEN=/tmp/oracle-regenerate-swipe.ndjson \
//!   QT_FIXTURE_REGEN_MAIN=/tmp/qt-regen-main.db QT_FIXTURE_REGEN_MOUNT=/tmp/qt-regen-mount.db \
//!   TZ=UTC cargo test -p quilltap-harness --test regenerate_swipe_tier3_equivalence \
//!     -- --test-threads=1

mod sampling_capture;

use std::collections::HashMap;

use quilltap_core::api::types::{Event, EventPayload};
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::db::{chats_messages_read, chats_read, dump_table_json_conn};
use quilltap_core::model::completion::{
    canned_completion_key, CannedCompletionProvider, CompletionMessage, CompletionRole,
};
use quilltap_core::model::embedding::CannedEmbeddingProvider;
use quilltap_core::model::stream::{
    CannedStreamingProvider, StreamChunk, StreamChunkResult, StreamUsage,
};
use quilltap_core::services::build_context::NoopSeams as BcNoopSeams;
use quilltap_core::services::cheap_llm_exec::CheapLlmTaskExecutor;
use quilltap_core::services::message_context::NoopMessageContextSeams;
use quilltap_core::services::regenerate_swipe::SwipeProgressEmitter;
use quilltap_core::services::regenerate_swipe::{
    regenerate_message_as_swipe, RegenError, RegenerateSwipeOptions,
};
use quilltap_core::weighted_random::DrawSource;
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CallW {
    name: String,
    chat_id: String,
    user_id: String,
    target_message_id: String,
    #[serde(default)]
    expect_throw: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    test_pepper_base64: String,
    seed_timestamp: String,
    frozen_now_ms: i64,
    #[serde(default)]
    local_offset_minutes: i64,
    calls: Vec<CallW>,
}

#[derive(Deserialize)]
struct CannedMsgW {
    role: String,
    content: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UsageW {
    prompt_tokens: i64,
    completion_tokens: i64,
    total_tokens: i64,
}
/// One canned CHUNK, as v4's own generator yields it (P4.D207). Every field
/// bar `content` is optional, exactly as [`StreamChunk`]'s are.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CannedChunkW {
    #[serde(default)]
    content: String,
    #[serde(default)]
    done: bool,
    #[serde(default)]
    usage: Option<UsageW>,
    #[serde(default)]
    raw_response: Option<Value>,
    #[serde(default)]
    reasoning_content: Option<String>,
    #[serde(default)]
    thought_signature: Option<String>,
}

#[derive(Deserialize)]
struct CannedStreamW {
    provider: String,
    model: String,
    temperature: Option<f64>,
    messages: Vec<CannedMsgW>,
    /// P4.D83: the sampling knobs v4's REAL `regenerateMessageAsSwipe` resolved
    /// off the profile bag for this call (absent keys = knobs v4 left undefined).
    #[serde(default)]
    sampling: Value,
    /// The chunk sequence v4's mocked `streamMessage` yielded.
    chunks: Vec<CannedChunkW>,
}

/// The oracle's ordered per-call WIRE frames (P4.D207) — v4's own encoders'
/// output, decoded back to objects. `call` → the frame list.
fn oracle_progress(oracle_text: &str) -> HashMap<String, Vec<Value>> {
    let mut out = HashMap::new();
    for line in oracle_text.lines().filter(|l| !l.trim().is_empty()) {
        let v: Value = serde_json::from_str(line).expect("parse oracle line");
        if v.get("kind").and_then(Value::as_str) == Some("progress") {
            out.insert(
                v.get("call").and_then(Value::as_str).unwrap().to_string(),
                v.get("frames")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default(),
            );
        }
    }
    out
}

/// Drain every [`EventPayload::SwipeProgress`] frame the port published for
/// `progress_id`, in order.
fn drain_frames(rx: &mut tokio::sync::broadcast::Receiver<Event>, progress_id: &str) -> Vec<Value> {
    let mut out = Vec::new();
    while let Ok(ev) = rx.try_recv() {
        if ev.progress_id.as_deref() != Some(progress_id) {
            continue;
        }
        if let EventPayload::SwipeProgress(p) = &ev.payload {
            out.push(p.frame.clone());
        }
    }
    out
}

fn to_completion_messages(m: &[CannedMsgW]) -> Vec<CompletionMessage> {
    m.iter()
        .map(|m| CompletionMessage {
            role: CompletionRole::from_v4_wire(m.role.as_str()).unwrap_or(CompletionRole::User),
            content: m.content.clone(),
        })
        .collect()
}

fn spec_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/regenerate-swipe-tier3.json")
}

/// The oracle NDJSON line for a dumped table.
fn oracle_table(oracle_text: &str, table: &str) -> Value {
    for line in oracle_text.lines().filter(|l| !l.trim().is_empty()) {
        let v: Value = serde_json::from_str(line).expect("parse oracle line");
        if v.get("kind").and_then(Value::as_str) == Some("table")
            && v.get("table").and_then(Value::as_str) == Some(table)
        {
            return v;
        }
    }
    panic!("oracle ndjson missing table {table}");
}

/// The oracle's recorded call throws (name → threw).
fn oracle_calls(oracle_text: &str) -> HashMap<String, bool> {
    let mut out = HashMap::new();
    for line in oracle_text.lines().filter(|l| !l.trim().is_empty()) {
        let v: Value = serde_json::from_str(line).expect("parse oracle line");
        if v.get("kind").and_then(Value::as_str) == Some("call") {
            out.insert(
                v.get("call").and_then(Value::as_str).unwrap().to_string(),
                v.get("threw").and_then(Value::as_bool).unwrap_or(false),
            );
        }
    }
    out
}

/// Sort chat_messages by a stable id-free key, then remap every `id` to a
/// first-appearance token (seeded ids identical → same token; minted swipe ids →
/// matching tokens). `createdAt` is the target's SEEDED stamp both sides, so it
/// stays literal.
fn normalize_messages(dump: &mut Value, idmap: &mut HashMap<String, String>) {
    if let Some(rows) = dump.get_mut("rows").and_then(Value::as_array_mut) {
        rows.sort_by_key(|r| {
            (
                str_of(r, "chatId"),
                str_of(r, "createdAt"),
                r.get("swipeIndex").and_then(Value::as_i64).unwrap_or(-1),
                str_of(r, "role"),
                str_of(r, "content"),
            )
        });
        for row in rows {
            if let Some(obj) = row.as_object_mut() {
                if let Some(id) = obj.get("id").and_then(Value::as_str) {
                    let n = idmap.len();
                    let tok = idmap
                        .entry(id.to_string())
                        .or_insert_with(|| format!("<m{n}>"))
                        .clone();
                    obj.insert("id".into(), Value::String(tok));
                }
            }
        }
    }
}

/// chats: placeholder the minted metadata bump columns.
fn normalize_chats(dump: &mut Value) {
    if let Some(rows) = dump.get_mut("rows").and_then(Value::as_array_mut) {
        rows.sort_by_key(|r| str_of(r, "id"));
        for row in rows {
            if let Some(obj) = row.as_object_mut() {
                for c in ["updatedAt", "lastMessageAt"] {
                    if let Some(v) = obj.get(c) {
                        if !v.is_null() {
                            obj.insert(c.to_string(), Value::String(format!("<{c}>")));
                        }
                    }
                }
            }
        }
    }
}

/// memories / vector_indices / vector_entries: sort by id; sentinel-aware
/// `updatedAt` collapse (a flush that removed entries mints via `saveMeta`).
fn normalize_id_table(dump: &mut Value, sentinel: &str) {
    if let Some(rows) = dump.get_mut("rows").and_then(Value::as_array_mut) {
        rows.sort_by_key(|r| str_of(r, "id"));
        for row in rows {
            if let Some(obj) = row.as_object_mut() {
                let bump = matches!(obj.get("updatedAt"), Some(Value::String(s)) if s != sentinel);
                if bump {
                    obj.insert("updatedAt".into(), Value::String("<ts>".into()));
                }
            }
        }
    }
}

fn str_of(r: &Value, k: &str) -> String {
    r.get(k).and_then(Value::as_str).unwrap_or("").to_string()
}

fn assert_rows_eq(name: &str, got: &Value, want: &Value) {
    if got.get("rows") != want.get("rows") {
        let g = serde_json::to_string_pretty(got.get("rows").unwrap()).unwrap();
        let w = serde_json::to_string_pretty(want.get("rows").unwrap()).unwrap();
        panic!("table {name} mismatch\n--- got ---\n{g}\n--- want ---\n{w}");
    }
}

/// The whole corpus, once. `emit_progress` chooses v4's narrated leg (an ACTIVE
/// [`SwipeProgressEmitter`], the default `stream: true` shape) or its silent one
/// (an inert emitter — v4's absent `onProgress`).
///
/// Returns the five normalized dumps, so the two legs can be compared to each
/// other as well as to the oracle.
fn run_corpus(label: &str, emit_progress: bool) -> Option<[Value; 5]> {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_REGEN") else {
        eprintln!("SKIP: QT_ORACLE_REGEN not set");
        return None;
    };
    let Ok(fixture_main) = std::env::var("QT_FIXTURE_REGEN_MAIN") else {
        eprintln!("SKIP: QT_FIXTURE_REGEN_MAIN not set");
        return None;
    };
    let Ok(fixture_mount) = std::env::var("QT_FIXTURE_REGEN_MOUNT") else {
        eprintln!("SKIP: QT_FIXTURE_REGEN_MOUNT not set");
        return None;
    };

    let spec: Spec =
        serde_json::from_str(&std::fs::read_to_string(spec_path()).expect("spec readable"))
            .expect("spec parses");
    let oracle_text = std::fs::read_to_string(&oracle_path).expect("oracle readable");
    let want_calls = oracle_calls(&oracle_text);

    let want_progress = oracle_progress(&oracle_text);

    // Canned streams (P4.D207 — the generation is read as a stream).
    let mut canned_streams: Vec<CannedStreamW> = Vec::new();
    for line in oracle_text.lines().filter(|l| !l.trim().is_empty()) {
        let v: Value = serde_json::from_str(line).unwrap();
        if v.get("kind").and_then(Value::as_str) == Some("cannedStream") {
            canned_streams.push(serde_json::from_value(v).expect("cannedStream"));
        }
    }
    assert!(
        !canned_streams.is_empty(),
        "the oracle recorded no `cannedStream` line — it is stale (pre-P4.D207) \
         or the mocked provider was never called; re-record it"
    );

    let scratch =
        std::env::temp_dir().join(format!("qt-regen-harness-{}-{label}", std::process::id()));
    std::fs::create_dir_all(&scratch).expect("scratch dir");
    let work_main = scratch.join("regen-main.db");
    let work_mount = scratch.join("regen-mount.db");
    let _ = std::fs::remove_file(&work_main);
    let _ = std::fs::remove_file(&work_mount);
    std::fs::copy(&fixture_main, &work_main).expect("copy main");
    std::fs::copy(&fixture_mount, &work_mount).expect("copy mount");

    let db = Db::open(
        DbPaths {
            main: work_main.clone(),
            mount_index: Some(work_mount.clone()),
            llm_logs: None,
        },
        &spec.test_pepper_base64,
    )
    .expect("open fixture");

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");

    let mut expected_sampling: HashMap<String, Value> = HashMap::new();
    let mut streaming = CannedStreamingProvider::new();
    for row in &canned_streams {
        let messages = to_completion_messages(&row.messages);
        let chunks: Vec<StreamChunkResult> = row
            .chunks
            .iter()
            .map(|c| {
                Ok(StreamChunk {
                    content: c.content.clone(),
                    done: c.done,
                    usage: c.usage.as_ref().map(|u| StreamUsage {
                        prompt_tokens: u.prompt_tokens,
                        completion_tokens: u.completion_tokens,
                        total_tokens: u.total_tokens,
                    }),
                    raw_response: c.raw_response.clone(),
                    reasoning_content: c.reasoning_content.clone(),
                    thought_signature: c.thought_signature.clone(),
                    ..Default::default()
                })
            })
            .collect();
        streaming = streaming.with_stream(
            &row.provider,
            &row.model,
            row.temperature,
            &messages,
            chunks,
        );
        expected_sampling.insert(
            canned_completion_key(&row.provider, &row.model, row.temperature, &messages),
            if row.sampling.is_object() {
                row.sampling.clone()
            } else {
                Value::Object(serde_json::Map::new())
            },
        );
    }
    // P4.D83: record what the PORT put on each call, so the three knobs are a
    // comparand rather than an unmeasured field. The STREAM twin now — the one
    // visible generation moved to the streaming seam (P4.D207).
    let streaming = sampling_capture::SamplingStreamCapture::new(streaming, |provider, params| {
        quilltap_core::model::stream::canned_stream_key(
            provider,
            &params.model,
            params.temperature,
            &params.messages,
        )
    });
    // The completion half stays wired for the context build's cheap-LLM feeders
    // (recall extraction, the distill). This corpus reaches none of them — the
    // oracle mocks them to no-ops and the Rust seams are the `Noop*` pair — so
    // an EMPTY canned provider is the honest instrument: a v5 path that started
    // calling it would answer a corpus-omission `Err` rather than pass quietly.
    let completion = CannedCompletionProvider::new();
    let embedding = CannedEmbeddingProvider::new();
    let executor = CheapLlmTaskExecutor::new();
    let bc_seams = BcNoopSeams;
    let mc_seams = NoopMessageContextSeams;

    // P4.D207: the port's frames ride the Event channel, so the comparand is
    // read off a real subscription — the same bytes a client sees.
    let (events_tx, mut events_rx) = tokio::sync::broadcast::channel::<Event>(512);

    for call in &spec.calls {
        let chat_id = call.chat_id.clone();
        let chat = db
            .read_main(move |c| chats_read::find_by_id(c, &chat_id))
            .expect("chat read")
            .expect("chat present");
        let chat_id2 = call.chat_id.clone();
        let all_messages = db
            .read_main(move |c| chats_messages_read::get_messages(c, &chat_id2))
            .expect("messages read");
        let target = all_messages
            .iter()
            .find(|m| m.get("id").and_then(Value::as_str) == Some(call.target_message_id.as_str()))
            .expect("target present")
            .clone();

        let result = rt.block_on(regenerate_message_as_swipe(
            &db,
            &embedding,
            &completion,
            &streaming,
            &executor,
            &bc_seams,
            &mc_seams,
            RegenerateSwipeOptions {
                user_id: call.user_id.clone(),
                chat,
                target_message: target,
                all_messages,
                active_user_participant_id: None,
                model_context_limit: 200_000,
                timestamp_config: None,
                timezone: Some("UTC".to_string()),
                server_tz: Some("UTC".to_string()),
                now_ms: spec.frozen_now_ms,
                local_offset_minutes: spec.local_offset_minutes,
                // P4.D172: `[0]` reproduces the frozen-zero pin.
                random01: DrawSource::sequence(vec![0.0]),
                // P4.D207: ACTIVE on the narrated leg, so the frame sequence
                // is a comparand on every call — including `not_assistant`,
                // where v4's guard throws before any beat and the list must be
                // EMPTY on both sides. INERT on the silent leg (v4's absent
                // `onProgress`).
                progress: if emit_progress {
                    SwipeProgressEmitter::active(call.target_message_id.clone(), events_tx.clone())
                } else {
                    SwipeProgressEmitter::inert()
                },
            },
        ));

        let threw = match &result {
            Ok(_) => false,
            Err(RegenError::NotAssistant) | Err(RegenError::StaffMessage) => true,
            Err(e) => panic!("regenerate {} unexpected error: {e}", call.name),
        };
        let want = *want_calls
            .get(&call.name)
            .unwrap_or_else(|| panic!("oracle missing call {}", call.name));
        // The oracle's recorded throw must match the spec's declared expectation
        // (a corpus-consistency check) AND the Rust side.
        assert_eq!(
            want, call.expect_throw,
            "spec/oracle throw disagree for {}",
            call.name
        );
        assert_eq!(threw, want, "call {} throw mismatch", call.name);

        // P4.D207: the ORDERED wire frames, diffed against v4's own encoders'
        // output. This is the arm the mutation proofs target — firing
        // `regenerating` on every content chunk changes this list.
        let got_frames = drain_frames(&mut events_rx, &call.target_message_id);
        let want_frames = want_progress.get(&call.name).unwrap_or_else(|| {
            panic!(
                "oracle missing a `progress` line for call {} — re-record it",
                call.name
            )
        });
        if emit_progress {
            assert_eq!(
                &got_frames,
                want_frames,
                "call {} progress frames mismatch\n--- got ---\n{}\n--- want ---\n{}",
                call.name,
                serde_json::to_string_pretty(&got_frames).unwrap(),
                serde_json::to_string_pretty(want_frames).unwrap()
            );
        } else {
            // The silent leg: v4's absent `onProgress` emits NOTHING.
            assert!(
                got_frames.is_empty(),
                "call {} published {} frame(s) with an INERT emitter",
                call.name,
                got_frames.len()
            );
        }
    }
    // A floor, so a silent emitter (or a drained-too-early subscription) cannot
    // pass the whole family vacuously.
    assert!(
        want_progress.values().any(|f| !f.is_empty()),
        "no call recorded a single progress frame — the comparand is vacuous"
    );

    // Dump + diff.
    let dump = |t: &str| -> Value {
        let t = t.to_string();
        db.read_main(move |c| dump_table_json_conn(c, &t, "id"))
            .expect("dump")
    };
    let mut got_chats = dump("chats");
    let mut got_msgs = dump("chat_messages");
    let mut got_mem = dump("memories");
    let mut got_vi = dump("vector_indices");
    let mut got_ve = dump("vector_entries");
    drop(db);
    let _ = std::fs::remove_dir_all(&scratch);

    let mut want_chats = oracle_table(&oracle_text, "chats");
    let mut want_msgs = oracle_table(&oracle_text, "chat_messages");
    let mut want_mem = oracle_table(&oracle_text, "memories");
    let mut want_vi = oracle_table(&oracle_text, "vector_indices");
    let mut want_ve = oracle_table(&oracle_text, "vector_entries");

    normalize_chats(&mut got_chats);
    normalize_chats(&mut want_chats);
    let mut idmap_g = HashMap::new();
    let mut idmap_w = HashMap::new();
    normalize_messages(&mut got_msgs, &mut idmap_g);
    normalize_messages(&mut want_msgs, &mut idmap_w);
    for (g, w) in [
        (&mut got_mem, &mut want_mem),
        (&mut got_vi, &mut want_vi),
        (&mut got_ve, &mut want_ve),
    ] {
        normalize_id_table(g, &spec.seed_timestamp);
        normalize_id_table(w, &spec.seed_timestamp);
    }

    // --- the SAMPLING knobs AT THE WIRE (P4.D83, v4 `d89babc4`) ---
    streaming.assert_matches(&expected_sampling, "regenerate-swipe");

    assert_rows_eq("chats", &got_chats, &want_chats);
    assert_rows_eq("chat_messages", &got_msgs, &want_msgs);
    assert_rows_eq("memories", &got_mem, &want_mem);
    assert_rows_eq("vector_indices", &got_vi, &want_vi);
    assert_rows_eq("vector_entries", &got_ve, &want_ve);

    // Sanity: two memory survivors (targetC's), two vector entries.
    assert_eq!(
        got_mem["rows"].as_array().unwrap().len(),
        2,
        "memory survivors"
    );
    assert_eq!(
        got_ve["rows"].as_array().unwrap().len(),
        2,
        "vector survivors"
    );

    eprintln!(
        "OK: regenerate-swipe tier-3 [{label}] matched oracle (5 tables + call throws + \
         {} progress frame set(s)).",
        want_progress.len()
    );
    Some([got_chats, got_msgs, got_mem, got_vi, got_ve])
}

/// The narrated leg — v4's `stream: true`, every frame a comparand.
#[test]
fn regenerate_swipe_tier3_matches_oracle() {
    run_corpus("streamed", true);
}

/// **v4's own invariant: "with no callback the generation is identical, just
/// silent."** (P4.D207 Tier-1 item 2's neutrality pin.)
///
/// This is not a paraphrase of the narrated leg — it is the same corpus run
/// with an INERT emitter against a FRESH copy of the fixture, and its five
/// dumps are diffed against **v4's own NARRATED record**. So a match proves
/// two things at once: that the silent v5 run writes what v4's streamed run
/// wrote, and that switching the narration on changes nothing but the frames.
///
/// The negative half rides inside `run_corpus`: on this leg every call must
/// publish ZERO frames, so an emitter that ignored its inert state would redden
/// here rather than passing quietly.
#[test]
fn a_silent_regeneration_writes_exactly_what_a_narrated_one_writes() {
    let Some(silent) = run_corpus("silent", false) else {
        return;
    };
    let Some(streamed) = run_corpus("streamed-twin", true) else {
        return;
    };
    // Both legs already matched the oracle table-by-table inside `run_corpus`;
    // this is the direct leg-to-leg diff, which is what "identical" means.
    for (name, (a, b)) in [
        "chats",
        "chat_messages",
        "memories",
        "vector_indices",
        "vector_entries",
    ]
    .into_iter()
    .zip(silent.iter().zip(streamed.iter()))
    {
        assert_eq!(
            a.get("rows"),
            b.get("rows"),
            "table {name}: a SILENT regeneration wrote something a narrated one did not"
        );
    }
    eprintln!("OK: regenerate-swipe silent/narrated neutrality (5 tables, both legs).");
}
