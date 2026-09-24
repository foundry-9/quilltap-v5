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
//! PROCESS-GLOBAL subscriber (this binary holds exactly this one test).
//!
//! Run:
//!   cargo test -p quilltap-web --test scenario_builder_disconnect -- --nocapture

mod common;
mod scenario_builder_spine;

use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use futures_util::StreamExt;
use serde_json::{json, Value};
use tracing_subscriber::layer::SubscriberExt;

use scenario_builder_spine::{Canned, ScenarioBuilderSpineFactory, SLOW_REASONING};

static LINES: OnceLock<Arc<Mutex<Vec<String>>>> = OnceLock::new();

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

#[tokio::test(flavor = "multi_thread")]
async fn dropping_the_sse_response_aborts_the_live_run() {
    let store = Arc::new(Mutex::new(Vec::new()));
    LINES.set(Arc::clone(&store)).unwrap();
    tracing::subscriber::set_global_default(
        tracing_subscriber::registry().with(Global(Arc::clone(&store))),
    )
    .expect("the one global subscriber of this binary");

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

    let resp = reqwest::Client::new()
        .post(format!(
            "http://{addr}/api/v1/scenario-builder?action=build"
        ))
        .header("content-type", "application/json")
        .body(
            json!({
                "mode": "in-world",
                "location": "The quay",
                "time": "dusk",
                "connectionProfileId": "5b170000-0000-4000-8000-0000000000a1",
            })
            .to_string(),
        )
        .send()
        .await
        .unwrap();
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
        "Scenario Builder client disconnected; aborting the run",
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
