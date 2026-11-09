//! Tier-3 differential: Storyteller "All at once" end to end (P4.D368, item 6
//! — `storyteller_simultaneous_tier3_equivalence`).
//!
//! v4's REAL `handleStorytellerRun(…, { order: 'simultaneous', keepOrder:
//! true })` (the oracle `harness/oracle/cases/storyteller-simultaneous-
//! tier3.test.ts`) vs v5's REAL host [`ChatSpine`] →
//! [`StorytellerDriver::run`], each on its own COPY of ONE minted fixture trio
//! (`build-storyteller-simultaneous-fixture.ts`): Ada and Bram (Simple,
//! profile A) all at once, then Cora's Act tail (profile B), with a project
//! whose description gives Prospero's cadence context something to post and
//! five planted messages so every turn lands ON the cadence.
//!
//! **The model boundary is canned BY SEAT on both sides** (the responding
//! character's name in the system prompt): `<Name> answers ` + `the storm.` +
//! a done with usage. Prompt bytes are NOT this family's comparand — that is
//! `orchestrator_tier3`'s job; keying by seat keeps each seat's call distinct
//! without a byte-exact prompt match. Every other subsystem runs REAL on both
//! sides (v4: only the provider, embeddings, the pricing cache, the API-key
//! lookups and the job processor are mocked; v5: the production spine over
//! canned providers — `quilltap-web/tests/chat_send_smoke.rs`'s construction).
//!
//! ## Compared (byte for byte through `serde_json::to_string`)
//!
//! - **The frames, PARTITIONED BY SEAT**: each concurrent seat's subsequence
//!   (`seat: {participantId, concurrent: true}` LAST on every frame) in
//!   order, and the UNSEATED subsequence in order — the Act tail's hook
//!   `turnStart` (depth 3) / `turnComplete`, its own turn's frames, and ONE
//!   `chainComplete`. The cross-seat INTERLEAVING is NOT compared — it is
//!   scheduling (v4's microtasks vs v5's cooperative task). The v5 frames are
//!   rendered exactly as `quilltap-web/src/events.rs` writes them, less the
//!   `chatId` scope tag (v4's stream body carries none). Masked: `messageId`
//!   (minted) → `<id>`.
//! - **No hook `turnStart` / `turnComplete` without a seat for a concurrent
//!   seat** (v4 `orchestrator.service.ts:364`, `:372`): the unseated
//!   subsequence names only Cora.
//! - **The rows the round wrote** — every NEW `chat_messages` `message` row
//!   (content decoded through each side's compressed-text codec,
//!   `participantId`, `storytellerBid`, `systemSender`,
//!   `targetParticipantIds`), SORTED (insert order among concurrent turns is
//!   scheduling). This carries item 8's follower gate: exactly ONE public
//!   Prospero context announcement for the group (the follower Bram posts
//!   none) and one group-context whisper per seat, plus the tail's own
//!   cadence announcement. And the `storyteller_rounds` row (`status`,
//!   `selection` with `chosenAt` masked, `closedReason`).
//! - **The captured lines**: `Storyteller round running`, `Storyteller round
//!   closed` (`spoke` compared as a SET — completion order is scheduling), and
//!   one `[TurnPreparation] Prospero context refresh due` per turn
//!   (`messageCount` / `reinjectInterval`; v4's logger `module` metadata is
//!   dropped).
//!
//! ## Rows
//!
//! - `main` — the round above.
//! - `stop` — Stop pressed from inside Ada's stream after her first chunk:
//!   both concurrent turns finish (Ada's partial preserved, Bram whole), the
//!   tail does not run, `chainComplete {reason:"stopped", …, paused:true}`.
//! - `failure` — Bram's provider fails after one chunk (item 7). **Named
//!   carve `CONCURRENT_TURN_FAILURE_CONTAINED`** (ruling R-3, both ways; the
//!   host module doc records it): v4's throw rejects `Promise.all`, so the
//!   round is left `running`, v4's `openTurnStream` catch logs `Streaming
//!   error` and enqueues its UNSEATED `Failed to generate response` frame,
//!   the stream CLOSES (Ada's `done` never arrives) and no `chainComplete`
//!   follows; v5 contains the failure to Bram's turn — Ada's turn and Cora's
//!   tail survive and the round closes `ran` with ONE `chainComplete`. The
//!   row asserts v4's measured shape AND v5's contained shape, and compares
//!   the parts both sides share (Ada's seated frames up to her content, the
//!   two `Storyteller round running` lines).
//!
//! `llm_logs` is NOT compared: v4's run writes no `llm_logs` table into the
//! minted llm-logs partition (measured — the oracle reads `no such table`),
//! so there is nothing to compare the round id against; the round id on every
//! row is `round_log_context_tests`' and `plan_storyteller_run`'s job.
//!
//! Generate the fixture + oracle (Node 24, from the PINNED v4 worktree —
//! §R.3 PIN REQUIRED; jest ignores `.claude/` paths, so the case is STAGED):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   TZ=UTC QT_FIXTURE_STS3_MAIN=/tmp/qt-sts3-main.db QT_FIXTURE_STS3_MOUNT=/tmp/qt-sts3-mount.db \
//!   QT_FIXTURE_STS3_LLMLOGS=/tmp/qt-sts3-llmlogs.db \
//!     $N/npx tsx $V5W/harness/oracle/fixtures/build-storyteller-simultaneous-fixture.ts
//!   mkdir -p /tmp/qt-sts3-jest && cp $V5W/harness/oracle/cases/storyteller-simultaneous-tier3.test.ts /tmp/qt-sts3-jest/
//!   TZ=UTC QT_FIXTURE_STS3_MAIN=/tmp/qt-sts3-main.db QT_FIXTURE_STS3_MOUNT=/tmp/qt-sts3-mount.db \
//!   QT_FIXTURE_STS3_LLMLOGS=/tmp/qt-sts3-llmlogs.db QT_ORACLE_OUT=/tmp/oracle-sts3.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=180000 \
//!       --roots "$PWD" --roots /tmp/qt-sts3-jest -- storyteller-simultaneous-tier3
//! Run:
//!   QT_ORACLE_STS3=/tmp/oracle-sts3.ndjson \
//!   QT_FIXTURE_STS3_MAIN=/tmp/qt-sts3-main.db QT_FIXTURE_STS3_MOUNT=/tmp/qt-sts3-mount.db \
//!   QT_FIXTURE_STS3_LLMLOGS=/tmp/qt-sts3-llmlogs.db \
//!     cargo test -p quilltap-harness --test storyteller_simultaneous_tier3_equivalence -- --nocapture

use std::collections::BTreeMap;
use std::future::Future;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use quilltap_core::api::chat_storyteller::{StorytellerDriver, StorytellerRunRequest};
use quilltap_core::api::Event;
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::model::completion::{
    CompletionError, CompletionParams, CompletionProvider, CompletionResponse,
};
use quilltap_core::model::embedding::CannedEmbeddingProvider;
use quilltap_core::model::stream::{
    StreamChunk, StreamChunkResult, StreamError, StreamMessage, StreamParams, StreamUsage,
    StreamingCompletionProvider,
};
use quilltap_core::services::file_storage::ProductionFileBytes;
use quilltap_core::services::pricing_fetcher::{PricingFetch, PricingFetcher};
use quilltap_core::storyteller::types::StorytellerOrder;
use quilltap_core::tools::self_inventory::{ClientShell, SelfInventoryEnv};
use quilltap_host::spine::ChatSpine;
use quilltap_host::{HostImageCodec, LocalStorageBackend};
use serde_json::{json, Map, Value};

const TEST_PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
const CHAT: &str = "c7000000-0000-4000-8000-000000000001";
const P_ADA: &str = "e7000000-0000-4000-8000-0000000000a1";
const P_BRAM: &str = "e7000000-0000-4000-8000-0000000000b2";
const P_CORA: &str = "e7000000-0000-4000-8000-0000000000c3";
const SEATS: [&str; 3] = ["Ada", "Bram", "Cora"];
/// The planted `message` rows the round does not write.
const PLANTED: usize = 5;
/// `CONCURRENT_TURN_FAILURE_CONTAINED` (header; ruling R-3).
const CONCURRENT_TURN_FAILURE_CONTAINED: &str = "failure";

const PINNED: &[&str] = &[
    "Storyteller round running",
    "Storyteller round closed",
    "[TurnPreparation] Prospero context refresh due",
];

// ---------------------------------------------------------------------------
// The canned model boundary (the oracle's `createLLMProvider`)
// ---------------------------------------------------------------------------

#[derive(Default, Clone)]
struct Plan {
    stop_during: Option<&'static str>,
    fail_seat: Option<&'static str>,
    /// Between chunks (the interleaving test; 0 for the differential rows).
    gap_ms: u64,
}

struct SeatStream {
    plan: Plan,
    stop_fired: AtomicBool,
}

fn seat_of(params: &StreamParams) -> String {
    let system = params
        .messages
        .iter()
        .find_map(|m| match m {
            StreamMessage::System { content } => Some(content.clone()),
            _ => None,
        })
        .unwrap_or_default();
    SEATS
        .iter()
        .find(|s| system.contains(&format!("You are {s}")))
        .or_else(|| SEATS.iter().find(|s| system.contains(*s)))
        .map(|s| s.to_string())
        .unwrap_or_else(|| "?".to_string())
}

impl StreamingCompletionProvider for SeatStream {
    #[allow(clippy::manual_async_fn)]
    fn stream_message(
        &self,
        _provider: &str,
        _base_url: Option<&str>,
        params: &StreamParams,
    ) -> impl Future<Output = tokio::sync::mpsc::Receiver<StreamChunkResult>> + Send {
        let seat = seat_of(params);
        let stop = self.plan.stop_during == Some(seat.as_str())
            && !self.stop_fired.swap(true, Ordering::SeqCst);
        let fail = self.plan.fail_seat == Some(seat.as_str());
        let gap = Duration::from_millis(self.plan.gap_ms);
        async move {
            let (tx, rx) = tokio::sync::mpsc::channel(8);
            // The provider streams from its own thread, so the chunks arrive
            // while the turns run (and a Stop lands between two chunks).
            std::thread::spawn(move || {
                let _ = tx.blocking_send(Ok(StreamChunk::content(format!("{seat} answers "))));
                if stop {
                    // v4: `void stopActiveTurns(chatId)` after the first chunk —
                    // fired once the other concurrent seat has finished (v4's
                    // microtasks run Bram's canned stream to its end first).
                    std::thread::sleep(Duration::from_millis(400));
                    let rt = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .expect("stop runtime");
                    rt.block_on(quilltap_core::services::turn_control::stop_active_turns(
                        CHAT,
                        Some(Duration::ZERO),
                    ));
                    std::thread::sleep(Duration::from_millis(200));
                }
                if fail {
                    let _ = tx.blocking_send(Err(StreamError::new(format!(
                        "{seat}'s provider fell over"
                    ))));
                    return;
                }
                std::thread::sleep(gap);
                let _ = tx.blocking_send(Ok(StreamChunk::content("the storm.".to_string())));
                std::thread::sleep(gap);
                let _ = tx.blocking_send(Ok(StreamChunk::done(Some(StreamUsage {
                    prompt_tokens: 10,
                    completion_tokens: 4,
                    total_tokens: 14,
                }))));
            });
            rx
        }
    }
}

/// Every cheap-LLM call answers `{}` (the oracle's `sendMessage`).
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
        version: "0.0.0-sts3".to_string(),
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

// ---------------------------------------------------------------------------
// The log capture (the round runs on its own thread — process-global)
// ---------------------------------------------------------------------------

fn global_capture() -> Arc<Mutex<Vec<String>>> {
    static BUF: OnceLock<Arc<Mutex<Vec<String>>>> = OnceLock::new();
    BUF.get_or_init(|| {
        use tracing_subscriber::layer::SubscriberExt;
        let buf = Arc::new(Mutex::new(Vec::<String>::new()));
        let subscriber = tracing_subscriber::registry()
            .with(quilltap_core::test_support::CaptureLayer(buf.clone()));
        tracing::subscriber::set_global_default(subscriber).expect("one global capture per binary");
        buf
    })
    .clone()
}

/// Tests in this binary share the global buffer and the turn-control
/// registry: run them one at a time.
fn serial() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|p| p.into_inner())
}

// ---------------------------------------------------------------------------
// One case through v5
// ---------------------------------------------------------------------------

struct Fixture {
    main: String,
    mount: String,
    llmlogs: String,
}

fn fixture() -> Option<Fixture> {
    Some(Fixture {
        main: std::env::var("QT_FIXTURE_STS3_MAIN").ok()?,
        mount: std::env::var("QT_FIXTURE_STS3_MOUNT").ok()?,
        llmlogs: std::env::var("QT_FIXTURE_STS3_LLMLOGS").ok()?,
    })
}

struct Ran {
    frames: Vec<Value>,
    messages: Vec<Value>,
    round: Value,
    lines: Vec<String>,
}

fn run_v5(fx: &Fixture, round_id: &str, plan: Plan) -> Ran {
    let dir = tempfile::Builder::new()
        .prefix("qt-sts3-rust-")
        .tempdir()
        .expect("tempdir");
    let data = dir.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    let (main, mount, ll) = (
        data.join("quilltap.db"),
        data.join("quilltap-mount-index.db"),
        data.join("quilltap-llm-logs.db"),
    );
    std::fs::copy(&fx.main, &main).expect("copy main");
    std::fs::copy(&fx.mount, &mount).expect("copy mount");
    std::fs::copy(&fx.llmlogs, &ll).expect("copy llm-logs");
    let db = Db::open(
        DbPaths {
            main,
            mount_index: Some(mount),
            llm_logs: Some(ll),
        },
        TEST_PEPPER,
    )
    .expect("open fixture copies");

    let (events, mut rx) = tokio::sync::broadcast::channel::<Event>(4096);
    let spine = ChatSpine {
        db: db.clone(),
        events: events.clone(),
        embedding: Arc::new(CannedEmbeddingProvider::new()),
        completion: Arc::new(AnyCompletion),
        streaming: Arc::new(SeatStream {
            plan,
            stop_fired: AtomicBool::new(false),
        }),
        pricing: Arc::new(PricingFetcher::new(NoPricingFetch)),
        tz: "UTC".to_string(),
        display_zone: quilltap_core::host_zone::TimeZone::UTC,
        env: test_env(),
        file_bytes: Arc::new(ProductionFileBytes {
            db: db.clone(),
            backend: Arc::new(LocalStorageBackend::new(dir.path().join("files"))),
            codec: Arc::new(HostImageCodec),
        }),
        image_transcoder: Arc::new(HostImageCodec),
        scrollback: None,
        consult: None,
        web_search: None,
        image_describe: None,
    };

    global_capture().lock().unwrap().clear();
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("runtime");
    let frames = rt.block_on(async {
        spine
            .run(StorytellerRunRequest {
                chat_id: CHAT.to_string(),
                round_id: round_id.to_string(),
                participant_ids: vec![P_ADA.into(), P_BRAM.into(), P_CORA.into()],
                order: StorytellerOrder::Simultaneous,
                keep_order: true,
            })
            .await
            .unwrap_or_else(|e| panic!("the run refused: {e:?}"));
        // Every chat frame of the run, until ONE `chainComplete`.
        let mut frames = Vec::new();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(60);
        loop {
            let left = deadline.saturating_duration_since(tokio::time::Instant::now());
            match tokio::time::timeout(left, rx.recv()).await {
                Ok(Ok(ev)) => {
                    // Exactly `quilltap-web/src/events.rs`'s `data:` bytes, less
                    // the scope tag (v4's stream body carries none).
                    let text = serde_json::to_string(&ev).expect("event json");
                    let tag = format!(r#"{{"chatId":"{CHAT}","#);
                    let Some(rest) = text.strip_prefix(&tag) else {
                        continue;
                    };
                    let text = format!("{{{rest}");
                    let v: Value = serde_json::from_str(&text).expect("frame json");
                    if v.get("kind").is_none() {
                        continue;
                    }
                    let done = v.get("kind").and_then(Value::as_str) == Some("chainComplete");
                    frames.push(v);
                    if done {
                        break;
                    }
                }
                Ok(Err(tokio::sync::broadcast::error::RecvError::Lagged(n))) => {
                    panic!("the broadcast lagged {n} frames")
                }
                Ok(Err(_)) | Err(_) => panic!("no chainComplete; frames so far: {frames:#?}"),
            }
        }
        // The round's writes land before its `chainComplete`; the
        // housekeeping enqueues settle.
        tokio::time::sleep(Duration::from_millis(300)).await;
        frames
    });

    let messages = db
        .read_main(|c| {
            let mut stmt = c.prepare(
                r#"SELECT "role", "content", "participantId", "storytellerBid", "systemSender", "targetParticipantIds"
                     FROM "chat_messages" WHERE "chatId" = ?1 AND "type" = 'message' ORDER BY "createdAt", "rowid""#,
            )?;
            let rows = stmt
                .query_map([CHAT], |r| {
                    let content = match r.get_ref(1)? {
                        rusqlite::types::ValueRef::Blob(b) => {
                            Value::String(quilltap_core::db::text_compression::decode_blob(b))
                        }
                        rusqlite::types::ValueRef::Text(t) => {
                            Value::String(String::from_utf8_lossy(t).into_owned())
                        }
                        _ => Value::Null,
                    };
                    let text = |i: usize| -> rusqlite::Result<Value> {
                        Ok(r.get::<_, Option<String>>(i)?
                            .map(Value::String)
                            .unwrap_or(Value::Null))
                    };
                    let mut m = Map::new();
                    m.insert("role".into(), text(0)?);
                    m.insert("content".into(), content);
                    m.insert("participantId".into(), text(2)?);
                    m.insert("storytellerBid".into(), text(3)?);
                    m.insert("systemSender".into(), text(4)?);
                    m.insert("targetParticipantIds".into(), text(5)?);
                    Ok(Value::Object(m))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .expect("messages");
    let rid = round_id.to_string();
    let round = db
        .read_main(move |c| {
            Ok(c.query_row(
                r#"SELECT "status", "selection", "closedReason" FROM "storyteller_rounds" WHERE "id" = ?1"#,
                [&rid],
                |r| {
                    Ok(json!({
                        "status": r.get::<_, Option<String>>(0)?,
                        "selection": r.get::<_, Option<String>>(1)?,
                        "closedReason": r.get::<_, Option<String>>(2)?,
                    }))
                },
            )?)
        })
        .expect("round");
    let lines = global_capture().lock().unwrap().clone();
    drop(rt);
    Ran {
        frames,
        messages: messages.into_iter().skip(PLANTED).collect(),
        round,
        lines,
    }
}

// ---------------------------------------------------------------------------
// Normalisation (both sides)
// ---------------------------------------------------------------------------

/// `UNPORTED_PRE_SEND_STATUSES` — the four pre-send `status` stages v4's
/// `processMessage` emits that v5's orchestrator does not (a standing gap
/// `orchestrator_tier3_equivalence`'s `shared_status_stage` already filters,
/// unmoved by this lane and by the span): dropped from BOTH sides. Every other
/// `status` stage is compared.
const UNPORTED_PRE_SEND_STATUSES: &[&str] = &[
    "loading_tools",
    "compressing",
    "budget_check",
    "assembling_context",
];

fn compared(f: &Value) -> bool {
    !(f["kind"] == "status"
        && f["status"]["stage"]
            .as_str()
            .is_some_and(|s| UNPORTED_PRE_SEND_STATUSES.contains(&s)))
}

/// `CONCURRENT_DONE_TURN_IS_SCHEDULING` — a concurrent seat's `done.turn`
/// (the next-speaker read off the transcript at its finalize) depends on which
/// seat of the group FINISHED first, which is scheduling (v4's microtasks vs
/// v5's cooperative task): masked on SEATED `done` frames only. The tail's
/// (unseated) `turn` is compared.
fn mask_frame(f: &Value) -> Value {
    let mut f = f.clone();
    let seated = f.get("seat").is_some();
    if let Some(o) = f.as_object_mut() {
        if o.get("messageId")
            .is_some_and(|v| v.as_str().is_some_and(|s| !s.is_empty()))
        {
            o.insert("messageId".into(), Value::String("<id>".into()));
        }
        if seated && o.get("kind").and_then(Value::as_str) == Some("done") && o.contains_key("turn")
        {
            o.insert("turn".into(), Value::String("<scheduling>".into()));
        }
    }
    f
}

/// Frames partitioned by seat (`UNSEATED` for the round's own stream), each
/// subsequence in arrival order.
fn by_seat(frames: &[Value]) -> BTreeMap<String, Vec<String>> {
    let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for f in frames.iter().filter(|f| compared(f)) {
        let seat = f
            .get("seat")
            .and_then(|s| s.get("participantId"))
            .and_then(Value::as_str)
            .unwrap_or("UNSEATED")
            .to_string();
        out.entry(seat)
            .or_default()
            .push(serde_json::to_string(&mask_frame(f)).unwrap());
    }
    out
}

fn mask_selection(round: &Value) -> Value {
    let mut r = round.clone();
    if let Some(sel) = r.get("selection").and_then(Value::as_str) {
        let mut v: Value = serde_json::from_str(sel).expect("selection json");
        v["chosenAt"] = Value::String("<ts>".into());
        r["selection"] = Value::String(serde_json::to_string(&v).unwrap());
    }
    r
}

fn sorted_rows(rows: &[Value]) -> Vec<String> {
    let mut out: Vec<String> = rows
        .iter()
        .map(|m| serde_json::to_string(m).unwrap())
        .collect();
    out.sort();
    out
}

/// A v4 log row → `LEVEL message k=v …` (the capture rig's rendering less its
/// target); `module` (v4's child-logger metadata) dropped; `spoke` sorted.
fn v4_line(log: &Value) -> String {
    let mut line = format!(
        "{} {}",
        log["level"].as_str().unwrap_or("").to_ascii_uppercase(),
        log["message"].as_str().unwrap_or("")
    );
    for (k, v) in log["fields"].as_object().into_iter().flatten() {
        if k == "module" {
            continue;
        }
        match v {
            Value::String(s) => line.push_str(&format!(" {k}={s}")),
            Value::Array(a) => {
                let mut items: Vec<String> = a
                    .iter()
                    .map(|x| {
                        x.as_str()
                            .map(|s| format!("{s:?}"))
                            .unwrap_or(x.to_string())
                    })
                    .collect();
                if k == "spoke" {
                    items.sort();
                }
                line.push_str(&format!(" {k}=[{}]", items.join(", ")));
            }
            other => line.push_str(&format!(" {k}={other}")),
        }
    }
    line
}

/// A v5 capture line → the same shape (target dropped), when pinned; `spoke`
/// sorted.
fn v5_line(line: &str) -> Option<String> {
    let (level, rest) = line.split_once(' ')?;
    let (_target, msg) = rest.split_once(' ')?;
    if !PINNED
        .iter()
        .any(|p| msg == *p || msg.starts_with(&format!("{p} ")))
    {
        return None;
    }
    let msg = match msg.find(" spoke=[") {
        Some(i) => {
            let start = i + " spoke=[".len();
            let end = start + msg[start..].find(']').unwrap_or(0);
            let mut items: Vec<&str> = msg[start..end]
                .split(", ")
                .filter(|s| !s.is_empty())
                .collect();
            items.sort();
            format!("{}{}{}", &msg[..start], items.join(", "), &msg[end..])
        }
        None => msg.to_string(),
    };
    Some(format!("{level} {msg}"))
}

fn oracle_rows() -> Option<Vec<Value>> {
    let path = std::env::var("QT_ORACLE_STS3").ok()?;
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read oracle {path}: {e}"));
    let rows: Vec<Value> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("oracle row"))
        .collect();
    assert_eq!(
        rows.len(),
        3,
        "the oracle carries the three rows — stale or truncated"
    );
    Some(rows)
}

const ROUNDS: [&str; 3] = [
    "57000000-0000-4000-8000-000000000001",
    "57000000-0000-4000-8000-000000000002",
    "57000000-0000-4000-8000-000000000003",
];

fn plan_for(name: &str) -> Plan {
    match name {
        "stop" => Plan {
            stop_during: Some("Ada"),
            ..Plan::default()
        },
        "failure" => Plan {
            fail_seat: Some("Bram"),
            ..Plan::default()
        },
        _ => Plan::default(),
    }
}

#[test]
fn storyteller_simultaneous_matches_oracle() {
    let _serial = serial();
    let (Some(rows), Some(fx)) = (oracle_rows(), fixture()) else {
        eprintln!("SKIP: set QT_ORACLE_STS3 + QT_FIXTURE_STS3_MAIN/MOUNT/LLMLOGS (see header).");
        return;
    };
    let mut mismatches = Vec::new();
    for (i, row) in rows.iter().enumerate() {
        let name = row["name"].as_str().expect("name");
        let got = run_v5(&fx, ROUNDS[i], plan_for(name));
        let want_frames: Vec<Value> = row["frames"].as_array().expect("frames").clone();
        let (gs, ws) = (by_seat(&got.frames), by_seat(&want_frames));
        let want_lines: Vec<String> = row["logs"]
            .as_array()
            .expect("logs")
            .iter()
            .filter(|l| PINNED.contains(&l["message"].as_str().unwrap_or("")))
            .map(v4_line)
            .collect();
        let got_lines: Vec<String> = got.lines.iter().filter_map(|l| v5_line(l)).collect();

        if name == CONCURRENT_TURN_FAILURE_CONTAINED {
            // v4 (measured): the round is left `running`, the unseated error
            // frame is the stream's LAST frame, no `chainComplete`, Ada's
            // `done` never arrives.
            assert!(row["threw"].is_null(), "v4's stream swallowed the throw");
            assert_eq!(
                row["rows"]["round"]["status"], "running",
                "v4 leaves the round running"
            );
            let last = want_frames.last().expect("v4 frames");
            assert_eq!(
                last["kind"], "error",
                "v4's last frame is the shell's error frame"
            );
            assert!(last.get("seat").is_none(), "v4's error frame is UNSEATED");
            assert!(
                !want_frames.iter().any(|f| f["kind"] == "chainComplete"),
                "v4 sends no chainComplete"
            );
            // v5 (contained): the round closes `ran`, Cora's tail runs, ONE
            // chainComplete user_turn.
            assert_eq!(got.round["status"], "closed", "v5 closes the round");
            assert_eq!(got.round["closedReason"], "ran");
            let chain: Vec<&Value> = got
                .frames
                .iter()
                .filter(|f| f["kind"] == "chainComplete")
                .collect();
            assert_eq!(chain.len(), 1);
            assert_eq!(chain[0]["reason"], "user_turn");
            assert!(got
                .frames
                .iter()
                .any(|f| f["kind"] == "done" && f["participantId"] == P_CORA));
            // Shared: Ada's seated frames v4 sent before the stream closed are
            // a prefix of v5's.
            let (ga, wa) = (&gs[P_ADA], &ws[P_ADA]);
            if ga.len() < wa.len() || ga[..wa.len()] != wa[..] {
                mismatches.push(format!(
                    "{name} Ada's seated prefix\n   v5: {ga:#?}\n   v4: {wa:#?}"
                ));
            }
            let running = |ls: &[String]| -> Vec<String> {
                ls.iter()
                    .filter(|l| l.contains("Storyteller round running"))
                    .cloned()
                    .collect()
            };
            if running(&got_lines) != running(&want_lines) {
                mismatches.push(format!(
                    "{name} LOGS (running)\n   v5: {got_lines:#?}\n   v4: {want_lines:#?}"
                ));
            }
            continue;
        }

        if gs != ws {
            for seat in gs
                .keys()
                .chain(ws.keys())
                .collect::<std::collections::BTreeSet<_>>()
            {
                let (g, w) = (gs.get(seat), ws.get(seat));
                if g != w {
                    mismatches.push(format!(
                        "{name} FRAMES seat {seat}\n   v5: {g:#?}\n   v4: {w:#?}"
                    ));
                }
            }
        }
        // No hook frame without a seat names a concurrent seat.
        for f in got.frames.iter().filter(|f| f.get("seat").is_none()) {
            if matches!(f["kind"].as_str(), Some("turnStart" | "turnComplete")) {
                assert_eq!(
                    f["participantId"], P_CORA,
                    "{name}: an unseated hook frame for a concurrent seat: {f}"
                );
            }
        }
        let want_messages: Vec<Value> = row["rows"]["messages"]
            .as_array()
            .expect("messages")
            .clone();
        let (gm, wm) = (sorted_rows(&got.messages), sorted_rows(&want_messages));
        if gm != wm {
            mismatches.push(format!("{name} MESSAGES\n   v5: {gm:#?}\n   v4: {wm:#?}"));
        }
        let (gr, wr) = (
            serde_json::to_string(&mask_selection(&got.round)).unwrap(),
            serde_json::to_string(&mask_selection(&row["rows"]["round"])).unwrap(),
        );
        if gr != wr {
            mismatches.push(format!("{name} ROUND\n   v5: {gr}\n   v4: {wr}"));
        }
        if got_lines != want_lines {
            mismatches.push(format!(
                "{name} LOGS\n   v5: {got_lines:#?}\n   v4: {want_lines:#?}"
            ));
        }
        if name == "main" {
            // Item 8: the group's ONE public briefing (the follower posts
            // none) + the tail's own cadence briefing; a whisper per seat.
            let public = want_messages
                .iter()
                .filter(|m| m["systemSender"] == "prospero" && m["targetParticipantIds"].is_null())
                .count();
            assert_eq!(
                public, 2,
                "v4: one public briefing for the group + one for the tail"
            );
        }
    }
    eprintln!(
        "storyteller_simultaneous_tier3_equivalence: {} rows compared, {} mismatches",
        rows.len(),
        mismatches.len()
    );
    assert!(
        mismatches.is_empty(),
        "{} mismatch(es):\n{}",
        mismatches.len(),
        mismatches.join("\n")
    );
}

/// Item 5's concurrency measurement (v5-only): two concurrent turns over a
/// provider that waits between chunks INTERLEAVE on the run's stream — the
/// seats alternate (A, B, A, B), not A, A, B, B — so the group is truly
/// concurrent on one cooperative task. Item 15 rides it: every frame of a
/// concurrent turn carries `seat: {participantId, concurrent: true}` LAST,
/// and the sequential tail's frames carry none.
#[test]
fn concurrent_turns_interleave_and_every_frame_is_seated() {
    let _serial = serial();
    let Some(fx) = fixture() else {
        eprintln!("SKIP: set QT_FIXTURE_STS3_MAIN/MOUNT/LLMLOGS (see header).");
        return;
    };
    let got = run_v5(
        &fx,
        ROUNDS[0],
        Plan {
            gap_ms: 150,
            ..Plan::default()
        },
    );
    let contents: Vec<&str> = got
        .frames
        .iter()
        .filter(|f| f["kind"] == "content" && f.get("seat").is_some())
        .map(|f| f["seat"]["participantId"].as_str().unwrap())
        .collect();
    assert_eq!(
        contents.len(),
        4,
        "two content chunks per concurrent seat: {contents:?}"
    );
    let first_two: std::collections::BTreeSet<&str> = contents[..2].iter().copied().collect();
    assert_eq!(
        first_two.len(),
        2,
        "both seats stream before either finishes (interleaved), got {contents:?}"
    );
    for f in &got.frames {
        let text = serde_json::to_string(f).unwrap();
        match f.get("seat") {
            Some(seat) => {
                let pid = seat["participantId"].as_str().unwrap();
                assert!(
                    pid == P_ADA || pid == P_BRAM,
                    "only the concurrent seats are seated: {text}"
                );
                assert!(
                    text.ends_with(&format!(
                        r#","seat":{{"participantId":"{pid}","concurrent":true}}}}"#
                    )),
                    "the seat is the LAST key: {text}"
                );
            }
            None => {
                let pid = f.get("participantId").and_then(Value::as_str);
                assert!(
                    pid.is_none() || pid == Some(P_CORA),
                    "an unseated frame names a concurrent seat: {text}"
                );
            }
        }
    }
}
