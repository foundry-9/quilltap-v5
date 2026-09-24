//! P4.D217 Tier 1 item 8 — **a client disconnect aborts the REAL run.**
//!
//! v4 passes `req.signal` into `runScenarioBuilder`, so closing the tab aborts
//! the loop. v5's run executes on the host driver's OWN thread, which dropping
//! a future does not stop, so the REST edge carries a disconnect guard in its
//! SSE body (`scenario_builder_routes::DisconnectGuard`). This drives the whole
//! chain with the REAL service over a slow canned stream: `POST
//! /api/v1/scenario-builder?action=build`, read the first frame (the stream's
//! leading `{"reasoning": …}`), HANG UP — then the run must end ABORTED:
//!
//! - the edge's DEBUG `Scenario Builder client disconnected; aborting the run`
//!   (v4's `onAbort` line — emitted only because a LIVE run was cut short);
//! - the loop's DEBUG `Scenario Builder: aborted mid-stream` (v4 breaks its
//!   `for await` on the next chunk — the loop's `{ ok: false, detail:
//!   'aborted' }`);
//! - the service's DEBUG `Scenario Builder run aborted by the client`;
//! - and NO `Scenario Builder run complete` / terminal frame.
//!
//! The run's lines are emitted on the driver's thread, so the capture is a
//! PROCESS-GLOBAL subscriber, installed once; the binary's tests take
//! [`SERIAL`] so each reads only its own lines.
//!
//! **P4.115 item 3 — the SECOND test, a leave BEFORE any frame.** v4's
//! `onAbort` is registered as soon as the stream exists (right after the
//! refusals), so it logs on a pre-frame abort too. A canned driver holds its
//! first frame until the token trips; the client gives up on the request
//! before any frame arrives. Before P4.115 the edge's guard was attached only
//! after the first frame committed the stream, so this leave dropped the
//! handler with no guard at all: the run aborted (its registration's `Drop`
//! trips the token) but the edge's DEBUG was lost.
//!
//! Run:
//!   cargo test -p quilltap-web --test scenario_builder_disconnect -- --nocapture

mod common;
mod scenario_builder_spine;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use futures_util::StreamExt;
use quilltap_core::api::scenario_builder::{
    ScenarioBuilderBuildRequest, ScenarioBuilderDriver, ScenarioBuilderFuture,
};
use quilltap_core::services::scenario_builder::ScenarioRunOutcome;
use serde_json::{json, Value};
use tracing_subscriber::layer::SubscriberExt;

use scenario_builder_spine::{Canned, DriverMaker, ScenarioBuilderSpineFactory, SLOW_REASONING};

static LINES: OnceLock<Arc<Mutex<Vec<String>>>> = OnceLock::new();

/// One test at a time: both read the one global line store.
static SERIAL: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// v4's `onAbort` line.
const DISCONNECTED: &str = "Scenario Builder client disconnected; aborting the run";

/// The tools-on OLLAMA profile both tests build with.
const PROFILE_ID: &str = "5b170000-0000-4000-8000-0000000000a1";

struct Global(Arc<Mutex<Vec<String>>>);
impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for Global {
    fn on_event(&self, e: &tracing::Event<'_>, _c: tracing_subscriber::layer::Context<'_, S>) {
        struct V(String);
        impl tracing::field::Visit for V {
            fn record_debug(&mut self, f: &tracing::field::Field, v: &dyn std::fmt::Debug) {
                if f.name() == "message" {
                    self.0 = format!("{v:?}");
                }
            }
        }
        let mut v = V(String::new());
        e.record(&mut v);
        if v.0.contains("Scenario Builder") {
            self.0.lock().unwrap().push(v.0);
        }
    }
}

fn lines() -> Vec<String> {
    LINES.get().unwrap().lock().unwrap().clone()
}

/// Install the one global subscriber (first caller) and clear the store.
fn fresh_capture() {
    let store = LINES.get_or_init(|| {
        let store = Arc::new(Mutex::new(Vec::new()));
        tracing::subscriber::set_global_default(
            tracing_subscriber::registry().with(Global(Arc::clone(&store))),
        )
        .expect("the one global subscriber of this binary");
        store
    });
    store.lock().unwrap().clear();
}

/// A per-run instance copy with one tools-on OLLAMA profile.
fn instance() -> tempfile::TempDir {
    let base = common::materialize_fixture_instance();
    {
        // One tools-on OLLAMA profile (a provider that takes no key), cloned
        // from the fixture's own row on this per-run copy.
        let w = quilltap_core::db::Writer::open_writable(
            &base.path().join("data").join("quilltap.db"),
            common::TEST_PEPPER,
        )
        .unwrap();
        w.connection()
            .execute_batch(
                "CREATE TEMP TABLE \"p4d217_clone\" AS SELECT * FROM \"connection_profiles\" ORDER BY rowid LIMIT 1;
                 UPDATE \"p4d217_clone\" SET \"id\" = '5b170000-0000-4000-8000-0000000000a1', \"name\" = 'Host OK', \"provider\" = 'OLLAMA', \"apiKeyId\" = NULL, \"allowToolUse\" = 1, \"isDefault\" = 0;
                 INSERT INTO \"connection_profiles\" SELECT * FROM \"p4d217_clone\";
                 DROP TABLE \"p4d217_clone\";",
            )
            .unwrap();
    }
    base
}

fn build_request(addr: std::net::SocketAddr) -> reqwest::RequestBuilder {
    reqwest::Client::new()
        .post(format!(
            "http://{addr}/api/v1/scenario-builder?action=build"
        ))
        .header("content-type", "application/json")
        .body(
            json!({
                "mode": "in-world",
                "location": "The quay",
                "time": "dusk",
                "connectionProfileId": PROFILE_ID,
            })
            .to_string(),
        )
}

#[tokio::test(flavor = "multi_thread")]
async fn dropping_the_sse_response_aborts_the_live_run() {
    let _serial = SERIAL.lock().await;
    fresh_capture();
    let base = instance();
    let base_dir = base.path().to_path_buf();
    let (addr, _state) = common::serve_instance(base.path(), move |mut c| {
        c.terminal = false;
        c.spine = Some(Arc::new(ScenarioBuilderSpineFactory {
            base_dir,
            canned: Canned::Slow,
            driver: None,
        }));
        c
    })
    .await;

    let resp = build_request(addr).send().await.unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers().get("content-type").unwrap(),
        "text/event-stream"
    );
    let mut stream = resp.bytes_stream();
    let first = tokio::time::timeout(Duration::from_secs(15), stream.next())
        .await
        .expect("the first frame arrives")
        .expect("a chunk")
        .expect("bytes");
    let first = String::from_utf8_lossy(&first).to_string();
    let frame: Value = serde_json::from_str(
        first
            .strip_prefix("data: ")
            .and_then(|s| s.strip_suffix("\n\n"))
            .expect("one v4 SSE frame"),
    )
    .unwrap();
    assert_eq!(frame, json!({ "reasoning": SLOW_REASONING }));

    // HANG UP mid-run.
    drop(stream);

    let want = [
        DISCONNECTED,
        "Scenario Builder: aborted mid-stream",
        "Scenario Builder run aborted by the client",
    ];
    for _ in 0..200 {
        let seen = lines();
        if want.iter().all(|w| seen.iter().any(|l| l == w)) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let seen = lines();
    for w in want {
        assert!(
            seen.iter().any(|l| l == w),
            "missing `{w}` after the disconnect; saw {seen:#?}"
        );
    }
    assert!(
        !seen.iter().any(|l| l == "Scenario Builder run complete"),
        "an aborted run must not complete: {seen:#?}"
    );
}

/// A driver that publishes NOTHING until its abort token trips, handing the
/// run's token out so the test reads what a real driver's thread would see.
/// (This double runs INLINE in the dispatch future, so a dropped handler
/// drops it too — only the token outlives it, which is exactly what the real
/// spine's detached thread watches.)
struct HoldsItsFirstFrame {
    tokens: Arc<Mutex<Vec<Arc<AtomicBool>>>>,
}

impl ScenarioBuilderDriver for HoldsItsFirstFrame {
    fn build(&self, req: ScenarioBuilderBuildRequest) -> ScenarioBuilderFuture<'_> {
        self.tokens.lock().unwrap().push(Arc::clone(&req.abort));
        Box::pin(async move {
            for _ in 0..400 {
                if req.abort.load(Ordering::SeqCst) {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
            Ok(ScenarioRunOutcome::Aborted)
        })
    }
}

/// P4.115 item 3 — **a leave BEFORE the first frame logs v4's disconnect
/// DEBUG exactly once and aborts the run** (module header).
///
/// Mutation M3: arm the guard only after the pre-commit race again (the
/// P4.D217 shape) → the DEBUG is never logged and this is RED.
#[tokio::test(flavor = "multi_thread")]
async fn leaving_before_the_first_frame_logs_the_disconnect_and_aborts_the_run() {
    let _serial = SERIAL.lock().await;
    fresh_capture();
    let base = instance();
    let base_dir = base.path().to_path_buf();
    let tokens: Arc<Mutex<Vec<Arc<AtomicBool>>>> = Arc::new(Mutex::new(Vec::new()));
    let maker: DriverMaker = {
        let tokens = Arc::clone(&tokens);
        Arc::new(move |_events| {
            Arc::new(HoldsItsFirstFrame {
                tokens: Arc::clone(&tokens),
            }) as _
        })
    };
    let started = || !tokens.lock().unwrap().is_empty();
    let aborted = || {
        tokens
            .lock()
            .unwrap()
            .iter()
            .filter(|t| t.load(Ordering::SeqCst))
            .count()
    };
    let (addr, _state) = common::serve_instance(base.path(), move |mut c| {
        c.terminal = false;
        c.spine = Some(Arc::new(ScenarioBuilderSpineFactory {
            base_dir,
            canned: Canned::Scene,
            driver: Some(maker),
        }));
        c
    })
    .await;

    // Give up on the request while the run holds its first frame: whether the
    // response head has arrived or not, no frame has, and dropping the future
    // (or the response) closes the connection.
    let pending = tokio::spawn(async move {
        let resp = build_request(addr).send().await.ok();
        // Hold the response (if any) without reading a byte, then leave.
        tokio::time::sleep(Duration::from_millis(300)).await;
        drop(resp);
    });
    for _ in 0..200 {
        if started() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(started(), "the run started");
    tokio::time::sleep(Duration::from_millis(150)).await;
    pending.abort();
    let _ = pending.await;

    for _ in 0..200 {
        if aborted() == 1 && lines().iter().any(|l| l == DISCONNECTED) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    // Let any late duplicate land before counting.
    tokio::time::sleep(Duration::from_millis(200)).await;
    let seen = lines();
    assert_eq!(aborted(), 1, "the client's leave aborts the run: {seen:#?}");
    assert_eq!(
        seen.iter().filter(|l| *l == DISCONNECTED).count(),
        1,
        "v4's disconnect DEBUG, exactly once, on a pre-frame leave: {seen:#?}"
    );
}
