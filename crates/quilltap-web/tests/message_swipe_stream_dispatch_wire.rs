//! P4.D207 (v4 `f564b0de3`) — the streamed swipe's DISPATCH WIRE.
//!
//! `messageSwipe` gained one optional flag, `stream: bool`, and a new Event
//! family rides it. Three things about that wire are invisible to every
//! core-side family in the repo, because each of them lives above the boundary:
//!
//! 1. **Does the flag survive serde?** `#[serde(default)]` on a `bool` means an
//!    absent key decodes as `false`, and a JSON `true` must reach the engine as
//!    `true` — the one instrument that sees a serde collapse is a real
//!    `POST /api/dispatch` (the `dispatch_wrong_type_census` discipline).
//! 2. **Do the frames reach `/api/events`, scope-tagged by the TARGET message
//!    id?** §S.2 fixes the tag as the swiped message's id, and the SPA narrows
//!    on it. A core capture test cannot see the transport.
//! 3. **Does a SWITCH ignore the flag?** v4 reads `?stream=1` only AFTER the
//!    switch branch has returned (`route.ts:260-270`), so `swipeIndex` present
//!    plus `stream: true` must narrate NOTHING. That ordering is a fact about
//!    v4's route, and the engine arm is where v5 keeps it.
//!
//! **The generation is expected to FAIL here, and that is the point.** The test
//! venue has no reachable provider, so the run gets as far as `gathering` and
//! `sending` and then the provider call fails — which exercises the `error`
//! frame end to end over the real transport, the one leg no canned stream can
//! reach. The frames before it are v4's bytes either way.
//!
//! Run:
//!   cargo test -p quilltap-web --test message_swipe_stream_dispatch_wire

mod common;

use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use futures_util::StreamExt;
use quilltap_core::api::types::Event;
use quilltap_core::db::runtime::Db;
use quilltap_core::model::completion::{
    CompletionError, CompletionParams, CompletionProvider, CompletionResponse,
};
use quilltap_core::model::embedding::CannedEmbeddingProvider;
use quilltap_core::model::stream::{
    StreamChunk, StreamChunkResult, StreamParams, StreamUsage, StreamingCompletionProvider,
};
use quilltap_core::services::file_storage::ProductionFileBytes;
use quilltap_core::services::pricing_fetcher::{PricingFetch, PricingFetcher};
use quilltap_core::tools::self_inventory::{ClientShell, SelfInventoryEnv};
use quilltap_host::spine::{ChatCreateSpine, ChatSpine, SpineBundle, SpineFactory};
use quilltap_host::{HostImageCodec, LocalStorageBackend};
use serde_json::{json, Value};

/// The two prose deltas the canned stream yields, so the wire sees the
/// `regenerating` beat fire ONCE and two `{"content": …}` frames follow.
const SWIPE_DELTAS: [&str; 2] = ["A re-rolled ", "line, at last."];

/// A streaming provider that answers EVERY call with one fixed sequence. The
/// swipe's one generation is the only stream this venue opens.
struct AnyStream;
impl StreamingCompletionProvider for AnyStream {
    #[allow(clippy::manual_async_fn)]
    fn stream_message(
        &self,
        _provider: &str,
        _base_url: Option<&str>,
        _params: &StreamParams,
    ) -> impl Future<Output = tokio::sync::mpsc::Receiver<StreamChunkResult>> + Send {
        async move {
            let (tx, rx) = tokio::sync::mpsc::channel(8);
            for d in SWIPE_DELTAS {
                let _ = tx.send(Ok(StreamChunk::content(d.to_string()))).await;
            }
            let _ = tx
                .send(Ok(StreamChunk::done(Some(StreamUsage {
                    prompt_tokens: 11,
                    completion_tokens: 7,
                    total_tokens: 18,
                }))))
                .await;
            rx
        }
    }
}

/// `{}` for every cheap-LLM call (the ported parsers resolve it to
/// nothing-found and the paths degrade gracefully).
struct AnyCompletion;
impl CompletionProvider for AnyCompletion {
    #[allow(clippy::manual_async_fn)]
    fn send_message(
        &self,
        _provider: &str,
        _base_url: Option<&str>,
        _params: &CompletionParams,
    ) -> impl Future<Output = Result<CompletionResponse, CompletionError>> + Send {
        async move {
            Ok(CompletionResponse {
                content: "{}".to_string(),
                usage: None,
                finish_reason: None,
                attachment_results: None,
                cache_usage: None,
            })
        }
    }
}

struct NoPricingFetch;
impl PricingFetch for NoPricingFetch {
    fn openrouter_public_models(&self) -> Option<Value> {
        None
    }
    fn openrouter_sdk_models(&self, _api_key: &str) -> Option<Value> {
        None
    }
    fn ollama_tags(&self, _base_url: &str) -> Option<Value> {
        None
    }
}

fn test_env() -> SelfInventoryEnv {
    SelfInventoryEnv {
        version: "0.0.0-swipe-wire".to_string(),
        runtime_mode: "local-dev".to_string(),
        client_shell: ClientShell::Browser,
        mount_index_degraded: false,
        release_notes: None,
        changelog: None,
        model_info: Vec::new(),
        fallback_pricing: Vec::new(),
        registry_default_context: 8192,
    }
}

/// ⚠ **`swipe_generate` must be `Some` here**, unlike the `chat_send_smoke`
/// factory this is modelled on. With it `None` the engine's readiness gate
/// answers `"swipe generation not assembled"` (a 500) BEFORE
/// `message_swipe_generate` runs at all — so the guards never execute, no frame
/// is ever published, and every assertion in this file would be measuring the
/// refusal instead of the wire. That is exactly what the first run of this test
/// measured, and it is why the bundle below wires the spine into both slots.
struct SwipeSpineFactory {
    base_dir: std::path::PathBuf,
}

impl SpineFactory for SwipeSpineFactory {
    fn build(
        &self,
        db: &Db,
        events: &tokio::sync::broadcast::Sender<Event>,
        _terminal: Option<Arc<quilltap_host::terminal::TerminalManager>>,
        pepper: &str,
        data_dir: &std::path::Path,
        bus: &Arc<quilltap_core::services::creation_progress::CreationProgressBus>,
    ) -> SpineBundle {
        let embedding = Arc::new(CannedEmbeddingProvider::new());
        let completion = Arc::new(AnyCompletion);
        let streaming = Arc::new(AnyStream);
        let spine = Arc::new(ChatSpine {
            db: db.clone(),
            events: events.clone(),
            embedding: Arc::clone(&embedding),
            completion: Arc::clone(&completion),
            streaming: Arc::clone(&streaming),
            pricing: Arc::new(PricingFetcher::new(NoPricingFetch)),
            tz: "UTC".to_string(),
            env: test_env(),
            file_bytes: Arc::new(ProductionFileBytes {
                db: db.clone(),
                backend: Arc::new(LocalStorageBackend::new(self.base_dir.join("files"))),
                codec: Arc::new(HostImageCodec),
            }),
            image_transcoder: Arc::new(HostImageCodec),
            scrollback: None,
            consult: None,
            web_search: None,
            image_describe: None,
        });
        let chat_create = Arc::new(ChatCreateSpine {
            db: db.clone(),
            events: events.clone(),
            bus: Arc::clone(bus),
            pepper: pepper.to_string(),
            data_dir: data_dir.to_path_buf(),
            embedding,
            completion,
            streaming,
            tz: "UTC".to_string(),
        });
        SpineBundle {
            chat_send: Arc::clone(&spine) as _,
            chat_create,
            swipe_generate: Some(Arc::clone(&spine) as _),
            provider_actions: None,
            memory_embedding: None,
            courier_resolve: None,
            save_image_bytes: None,
            image_generation: None,
            consult: None,
            brahma_console_send: None,
            help_chat_send: None,
            recall_replay: None,
            announcement_preview: None,
            in_scene_voice: None,
            operator_tool_runner: None,
            regenerate_title: None,
            outfit_llm_choose: None,
            image_describe: None,
            web_search: None,
            search_providers: Vec::new(),
            job_handlers: Vec::new(),
            generators_detail: None,
            generators_wizard: None,
        }
    }
}

/// The chat-send fixture's populated chat. ⚠ NOT the smoke chat
/// `9fe3f87b-…` that `chat_send_smoke` uses: that one is EMPTY until the smoke
/// test sends into it, so discovering a target there finds nothing at all
/// (measured — the first draft of this test failed on "0 events"). This chat
/// carries ten USER/ASSISTANT pairs, none of them staff-authored.
const POPULATED_CHAT_ID: &str = "c860cf74-128f-4a81-9a5c-6c2275f24302";

/// Collect every `data:` frame that arrives within `window`, then give up.
/// Deliberately time-bounded rather than count-bounded: a test that waits for
/// N frames cannot prove that a SWITCH published ZERO
/// (`e2e-tohavecount-resolves-on-first-matching-poll`, the same hazard).
async fn collect_frames(resp: reqwest::Response, window: Duration) -> Vec<Value> {
    let mut stream = resp.bytes_stream();
    let mut buf = String::new();
    let mut out = Vec::new();
    let deadline = tokio::time::Instant::now() + window;
    while tokio::time::Instant::now() < deadline {
        let remaining = deadline - tokio::time::Instant::now();
        let chunk = match tokio::time::timeout(remaining, stream.next()).await {
            Ok(Some(Ok(bytes))) => bytes,
            _ => break,
        };
        buf.push_str(&String::from_utf8_lossy(&chunk));
        while let Some(end) = buf.find("\n\n") {
            let frame = buf[..end].to_string();
            buf.drain(..end + 2);
            for line in frame.lines() {
                if let Some(payload) = line.strip_prefix("data: ") {
                    if let Ok(v) = serde_json::from_str::<Value>(payload) {
                        out.push(v);
                    }
                }
            }
        }
    }
    out
}

/// The swipe frames for one `progressId`, in order.
fn swipe_frames(frames: &[Value], progress_id: &str) -> Vec<Value> {
    frames
        .iter()
        .filter(|f| {
            f.get("type").and_then(Value::as_str) == Some("swipeProgress")
                && f.get("progressId").and_then(Value::as_str) == Some(progress_id)
        })
        .map(|f| f.get("frame").cloned().unwrap_or(Value::Null))
        .collect()
}

/// The dispatch error envelope's sentence: `{"type":"error","data":{"kind":…,
/// "message":…}}`.
fn err_message(v: &Value) -> &str {
    v["data"]["message"].as_str().unwrap_or_default()
}

struct Wire {
    client: reqwest::Client,
    url: String,
}
impl Wire {
    async fn post(&self, body: Value) -> (u16, Value) {
        let r = self
            .client
            .post(&self.url)
            .json(&body)
            .send()
            .await
            .unwrap();
        let status = r.status().as_u16();
        (status, r.json::<Value>().await.unwrap_or(Value::Null))
    }
}

/// Every `type: 'message'` event in the populated chat.
async fn messages(wire: &Wire) -> Vec<Value> {
    let (status, v) = wire
        .post(json!({ "type": "chatMessageEvents", "chatId": POPULATED_CHAT_ID }))
        .await;
    assert_eq!(status, 200, "chatMessageEvents: {v}");
    let msgs = v["data"]["messages"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(
        !msgs.is_empty(),
        "the fixture chat is EMPTY — this test would be vacuous: {v}"
    );
    msgs
}

#[tokio::test(flavor = "multi_thread")]
async fn the_stream_flag_narrates_a_generate_and_is_ignored_by_a_switch() {
    let base = common::materialize_fixture_instance();
    let base_dir = base.path().to_path_buf();
    let (addr, _state) = common::serve_instance(base.path(), move |mut c| {
        c.terminal = false;
        c.spine = Some(Arc::new(SwipeSpineFactory { base_dir }));
        c
    })
    .await;
    let wire = Wire {
        client: reqwest::Client::new(),
        url: format!("http://{addr}/api/dispatch"),
    };

    // The target: a character-authored ASSISTANT message (no `systemSender`),
    // discovered rather than hard-coded so a fixture regen cannot silently
    // make this test vacuous.
    let all = messages(&wire).await;
    let target = all
        .iter()
        .find(|m| {
            m.get("role").and_then(Value::as_str) == Some("ASSISTANT")
                && m.get("systemSender")
                    .and_then(Value::as_str)
                    .is_none_or(str::is_empty)
        })
        .unwrap_or_else(|| {
            panic!(
                "the chat-send fixture has no swipe-able ASSISTANT message among {} events",
                all.len()
            )
        });
    let target_id = target["id"].as_str().expect("an id").to_string();

    // ---- 1. GENERATE with `stream: true` → the frames arrive, tagged with the
    // TARGET message id.
    let events = wire
        .client
        .get(format!("http://{addr}/api/events"))
        .send()
        .await
        .unwrap();
    assert_eq!(events.status(), 200);
    tokio::time::sleep(Duration::from_millis(200)).await;

    let dispatch = {
        let client = wire.client.clone();
        let url = wire.url.clone();
        let id = target_id.clone();
        tokio::spawn(async move {
            client
                .post(&url)
                .json(&json!({ "type": "messageSwipe", "messageId": id, "stream": true }))
                .send()
                .await
                .unwrap()
                .json::<Value>()
                .await
                .unwrap_or(Value::Null)
        })
    };

    let frames = collect_frames(events, Duration::from_secs(20)).await;
    let _ = dispatch.await;
    let mine = swipe_frames(&frames, &target_id);
    assert!(
        !mine.is_empty(),
        "no `swipeProgress` frame reached /api/events for the target — does the \
         engine build the emitter from the `stream` flag? (all frames: {})",
        frames.len()
    );

    // The WHOLE sequence, in v4's order. The canned stream yields two prose
    // deltas and a terminal usage chunk, so this is the full shape the Salon
    // consumes: four beats, `regenerating` ONCE and only once, both deltas, and
    // the route's terminal `done` carrying the persisted row.
    let stages: Vec<&str> = mine
        .iter()
        .filter_map(|f| f.get("status"))
        .filter_map(|s| s.get("stage").and_then(Value::as_str))
        .collect();
    assert_eq!(
        stages,
        vec!["gathering", "sending", "regenerating", "saving"],
        "the four beats, in v4's order and ONCE each: {mine:?}"
    );
    let deltas: Vec<&str> = mine
        .iter()
        .filter_map(|f| f.get("content").and_then(Value::as_str))
        .collect();
    assert_eq!(
        deltas,
        SWIPE_DELTAS.to_vec(),
        "each content chunk is its own DELTA frame, in order"
    );
    let done = mine.last().expect("a terminal frame");
    assert_eq!(
        done.get("done").and_then(Value::as_bool),
        Some(true),
        "the LAST frame is the route's `done`: {done:?}"
    );
    assert_eq!(
        done.pointer("/message/content").and_then(Value::as_str),
        Some(SWIPE_DELTAS.concat().as_str()),
        "…carrying the persisted swipe, whose content is the CONCATENATION of \
         the deltas: {done:?}"
    );
    // The three columns that used to be NULL ride the chunks now; the canned
    // stream carries usage, so the token triple is real on the persisted row.
    assert_eq!(
        done.pointer("/message/tokenCount").and_then(Value::as_i64),
        Some(18),
        "the usage rode the terminal chunk onto the row: {done:?}"
    );
    let first = mine[0].get("status").expect("a status frame");
    assert_eq!(
        first.get("kind").and_then(Value::as_str),
        Some("status"),
        "the `status` object carries `kind` — v4's route stringifies `{{ status }}` \
         over the WHOLE progress event"
    );
    assert!(
        first
            .get("message")
            .and_then(Value::as_str)
            .is_some_and(|m| m.starts_with("Regenerating — gathering")),
        "v4's sentence: {first:?}"
    );
    assert!(
        first.get("characterName").is_some() && first.get("characterId").is_some(),
        "both character fields ride every beat: {first:?}"
    );

    // ---- 2. A SWITCH ignores the flag. v4 reads `?stream=1` only after the
    // switch branch has returned, so this must narrate NOTHING.
    let events2 = wire
        .client
        .get(format!("http://{addr}/api/events"))
        .send()
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;
    let (status, body) = wire
        .post(json!({
            "type": "messageSwipe",
            "messageId": target_id,
            "swipeIndex": 0,
            "stream": true,
        }))
        .await;
    // Whatever the switch answers (200 with the variant, or v4's 400 for a
    // message that is not in a swipe group), it must not have narrated.
    let frames2 = collect_frames(events2, Duration::from_secs(3)).await;
    assert!(
        swipe_frames(&frames2, &target_id).is_empty(),
        "a SWITCH published swipe frames (status {status}, body {body}) — v4 reads \
         the flag only on the generate branch"
    );
}

/// The serde envelope, on its own: the flag is a plain `bool` with
/// `#[serde(default)]`, so an ABSENT key must decode as `false` and must not
/// refuse the request. (The wrong-TYPE question is adjudicated in
/// `dispatch_wrong_type_census`'s `MessageSwipe.stream` row — v4 reads this
/// from the query string, so no wrong-JSON-type behaviour exists to reproduce.)
#[tokio::test(flavor = "multi_thread")]
async fn the_flag_is_optional_and_a_refusal_never_narrates() {
    let base = common::materialize_fixture_instance();
    let base_dir = base.path().to_path_buf();
    let (addr, _state) = common::serve_instance(base.path(), move |mut c| {
        c.terminal = false;
        c.spine = Some(Arc::new(SwipeSpineFactory { base_dir }));
        c
    })
    .await;
    let wire = Wire {
        client: reqwest::Client::new(),
        url: format!("http://{addr}/api/dispatch"),
    };

    let events = wire
        .client
        .get(format!("http://{addr}/api/events"))
        .send()
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;

    // v4's three refusals, each with `stream: true` so a frame emitted before
    // the guards would show up here. A USER message takes the ASSISTANT-only
    // guard; a made-up id takes the 404.
    let all = messages(&wire).await;
    let user_msg = all
        .iter()
        .find(|m| m.get("role").and_then(Value::as_str) == Some("USER"))
        .and_then(|m| m["id"].as_str())
        .expect("a USER message in the fixture")
        .to_string();

    let (s1, v1) = wire
        .post(json!({
            "type": "messageSwipe",
            "messageId": user_msg,
            "stream": true,
        }))
        .await;
    assert_eq!(s1, 400, "the ASSISTANT-only guard: {v1}");
    assert_eq!(err_message(&v1), "Only assistant messages can be swiped");

    let missing = "00000000-0000-4000-8000-0000000000ff";
    let (s2, v2) = wire
        .post(json!({
            "type": "messageSwipe",
            "messageId": missing,
            "stream": true,
        }))
        .await;
    assert_eq!(s2, 404, "the 404: {v2}");

    // An ABSENT `stream` key must decode (a `#[serde(default)] bool`), not
    // refuse with a decode error — the same refusal, reached the same way.
    let (s3, v3) = wire
        .post(json!({ "type": "messageSwipe", "messageId": user_msg }))
        .await;
    assert_eq!(
        s3, 400,
        "an absent `stream` must decode as false and reach the SAME guard: {v3}"
    );
    assert_eq!(
        err_message(&v3),
        "Only assistant messages can be swiped",
        "…with the same sentence, not a serde complaint"
    );

    let frames = collect_frames(events, Duration::from_secs(2)).await;
    let any_swipe: Vec<&Value> = frames
        .iter()
        .filter(|f| f.get("type").and_then(Value::as_str) == Some("swipeProgress"))
        .collect();
    assert!(
        any_swipe.is_empty(),
        "a REFUSAL narrated: {any_swipe:?} — v4's rule is that a refusal before \
         the stream opens stays an ordinary JSON error"
    );
}
