//! Route-family differential for `/api/v1/scenario-builder` (P4.D217, v4
//! `d1c06cd9d` `app/api/v1/scenario-builder/route.ts`): the REAL axum edge
//! (`scenario_builder_routes`) over the REAL engine refusals, against v4's REAL
//! route driven by `harness/oracle/cases/scenario-builder-routes.test.ts`.
//!
//! **What is canned, on BOTH sides, is the run and nothing else** — v4's
//! `runScenarioBuilder` (mocked in the oracle to enqueue the spec's `frames`,
//! recording the `input` it received and whether its `signal` aborted) and v5's
//! `ScenarioBuilderDriver` (a test double here publishing the same frames,
//! recording the same input and watching the same abort token). The route's
//! contract is what it does AROUND the run, which is exactly what is compared:
//! per case the status, the `content-type` (plus, on a stream, `cache-control`
//! and `connection`), the JSON body or the SSE bytes, and the runs the route
//! started — the cast ids it kept, the chat it vetted, the revise pair, and
//! whether a client disconnect reached the run (`aborted`). Both sides read a
//! per-run copy of the committed chat-send fixture pair with the same PLANTS
//! (`scenario-builder-routes.json`): tools-on / tools-off / keyless / foreign
//! profiles, Salon / autonomous / help / foreign chats, a readable, a foreign
//! and an unreadable (BLOB-named) character.
//!
//! **Pinned both ways (the `EXPECTED_DIVERGENCES` shape):**
//! - `500s when the capability lookup throws` — v4 reaches its 500 arm only by
//!   a MOCKED throwing probe (its real `resolveScenarioBuilderCapabilities`
//!   cannot throw), and v5's probe is infallible; v4's recorded body is
//!   asserted as recorded, v5's answer is asserted to be the 200 capabilities
//!   body, and the family fails "VANISHED" if they ever agree.
//! - `returns the resolved capabilities` — `webSearchConfigured` compared equal
//!   (neither side has a search provider or `SERPER_API_KEY`); `curlConfigured`
//!   is asserted `false` on v5 (the §R.4(k) recorded divergence: no curl
//!   plugin exists on v5) with v4's value RECORDED — v4 answers `false` too
//!   here only because the jest env registers no plugins.
//!
//! **The log lines (the d1c06cd9d unification review).** Per case, the lines
//! v4 logged on a `ScenarioBuilder` logger (the route's `context` child and
//! `resolveScenarioBuilderCapabilities`'s `service` logger) are compared —
//! level, message, and the context's keys IN ORDER with their JSON values —
//! against every event v5 emitted on the matching targets ([`LOG_TARGETS`]).
//! It is an equality over the whole per-case list, so every case is also a
//! silence leg: a line v4 did not log must not appear on v5. v5's route runs
//! on the server's worker threads, not the test thread, so the thread-scoped
//! `test_support::global_capture` cannot see it; this binary (ONE test)
//! installs its own process-global [`StructuredCapture`] layer instead, which
//! records typed field values (a count stays a number, a flag a bool) where
//! `FieldVisitor` would flatten them to text. The WARN `Scenario Builder
//! dropped an unreadable cast id` is unreachable through v4's real code (its
//! `characters.findById` is a fallback-mode `safeQuery`, so a failing read is
//! `null`, never a throw) — the unreadable-character case pins its ABSENCE on
//! both sides rather than inventing a trigger.
//!
//! Regenerate (Node 24; stage outside `.claude/`):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; W=${V5W:-$(git rev-parse --show-toplevel)}
//!   STAGE=/tmp/qt-oracle-stage-sb-routes
//!   rm -rf $STAGE && mkdir -p $STAGE/harness/oracle/cases $STAGE/harness/oracle/fixtures
//!   cp $W/harness/oracle/cases/scenario-builder-routes.test.ts $STAGE/harness/oracle/cases/
//!   cp $W/harness/oracle/fixtures/scenario-builder-routes.json $STAGE/harness/oracle/fixtures/
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_SBR_MAIN=$W/crates/quilltap-web/tests/fixtures/chat-send-main.db \
//!   QT_FIXTURE_SBR_MOUNT=$W/crates/quilltap-web/tests/fixtures/chat-send-mount.db \
//!   QT_ORACLE_OUT=/tmp/oracle-scenario-builder-routes.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=240000 \
//!       --roots "$PWD" --roots "$STAGE/harness/oracle/cases" -- "scenario-builder-routes\.test\.ts$"
//! Run:
//!   QT_ORACLE_SB_ROUTES=/tmp/oracle-scenario-builder-routes.ndjson \
//!     cargo test -p quilltap-web --test scenario_builder_routes_equivalence -- --nocapture

mod common;
mod scenario_builder_spine;

use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::StreamExt;
use quilltap_core::api::scenario_builder::{
    ScenarioBuilderBuildRequest, ScenarioBuilderDriver, ScenarioBuilderFuture,
};
use quilltap_core::api::types::Event;
use quilltap_core::services::scenario_builder::ScenarioRunOutcome;
use serde::Deserialize;
use serde_json::{json, Value};

use scenario_builder_spine::{Canned, DriverMaker, ScenarioBuilderSpineFactory};

/// Cases whose v4 row is reachable only through a mock (see the header).
const V4_MOCK_ONLY: &[&str] = &["500s when the capability lookup throws"];

/// The v5 homes of v4's `ScenarioBuilder`-logger lines this family compares:
/// the route's prepare, the capability probe, and the SSE edge's disconnect.
const LOG_TARGETS: &[&str] = &[
    "quilltap_core::api::scenario_builder",
    "quilltap_core::services::scenario_builder::capabilities",
    "quilltap_web::scenario_builder_routes",
];

/// Every event on [`LOG_TARGETS`], as `{ level, message, context }` with the
/// context as ordered `[key, value]` entries (`null` when the event carries no
/// field besides its message) — the oracle's own row shape.
static LOGGED: Mutex<Vec<Value>> = Mutex::new(Vec::new());

struct StructuredCapture;

struct JsonFields {
    message: Option<String>,
    fields: Vec<Value>,
}

impl tracing::field::Visit for JsonFields {
    fn record_str(&mut self, f: &tracing::field::Field, v: &str) {
        self.fields.push(json!([f.name(), v]));
    }
    fn record_u64(&mut self, f: &tracing::field::Field, v: u64) {
        self.fields.push(json!([f.name(), v]));
    }
    fn record_i64(&mut self, f: &tracing::field::Field, v: i64) {
        self.fields.push(json!([f.name(), v]));
    }
    fn record_bool(&mut self, f: &tracing::field::Field, v: bool) {
        self.fields.push(json!([f.name(), v]));
    }
    fn record_debug(&mut self, f: &tracing::field::Field, v: &dyn std::fmt::Debug) {
        // `%` fields arrive here as a Display wrapper, so `{:?}` is their
        // Display text — a string, as v4's context value is.
        if f.name() == "message" {
            self.message = Some(format!("{v:?}"));
        } else {
            self.fields.push(json!([f.name(), format!("{v:?}")]));
        }
    }
}

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for StructuredCapture {
    fn on_event(&self, event: &tracing::Event<'_>, _: tracing_subscriber::layer::Context<'_, S>) {
        let meta = event.metadata();
        if !LOG_TARGETS.contains(&meta.target()) {
            return;
        }
        let mut v = JsonFields {
            message: None,
            fields: Vec::new(),
        };
        event.record(&mut v);
        LOGGED.lock().unwrap().push(json!({
            "level": meta.level().as_str().to_ascii_lowercase(),
            "message": v.message,
            "context": if v.fields.is_empty() { Value::Null } else { Value::Array(v.fields) },
        }));
    }
}

fn install_capture() {
    use tracing_subscriber::layer::SubscriberExt;
    // LOUD: a second global subscriber would make every silence leg vacuous.
    tracing::subscriber::set_global_default(tracing_subscriber::registry().with(StructuredCapture))
        .expect("this binary's one test owns the global subscriber");
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct CaseW {
    name: String,
    method: String,
    query: String,
    #[serde(default)]
    body: Option<Value>,
    #[serde(default)]
    raw_body: bool,
    #[serde(default)]
    await_abort: bool,
}

#[derive(Deserialize)]
struct Plant {
    sql: String,
    params: Vec<Option<String>>,
}

#[derive(Deserialize)]
struct Spec {
    plants: Vec<Plant>,
    frames: Vec<Value>,
    cases: Vec<CaseW>,
}

/// The canned driver's shared state: whether the NEXT run waits for its abort,
/// and every run's recorded input.
#[derive(Default)]
struct Recorder {
    await_abort: bool,
    runs: Vec<Value>,
}

struct CannedDriver {
    events: tokio::sync::broadcast::Sender<Event>,
    frames: Vec<Value>,
    recorder: Arc<Mutex<Recorder>>,
}

impl ScenarioBuilderDriver for CannedDriver {
    fn build(&self, req: ScenarioBuilderBuildRequest) -> ScenarioBuilderFuture<'_> {
        Box::pin(async move {
            let mut chat = serde_json::Map::new();
            let chat_value = req.input.chat.as_ref().map(|c| {
                chat.insert("id".into(), json!(c.id));
                // v4 builds `{ id, scenarioText, contextSummary }` from a read
                // that turns NULL cells into `undefined`, which JSON omits.
                if let Some(t) = &c.scenario_text {
                    chat.insert("scenarioText".into(), json!(t));
                }
                if let Some(t) = &c.context_summary {
                    chat.insert("contextSummary".into(), json!(t));
                }
                Value::Object(chat.clone())
            });
            let (await_abort, index) = {
                let mut r = self.recorder.lock().unwrap();
                r.runs.push(json!({
                    "mode": req.input.mode.as_str(),
                    "characterIds": req.input.character_ids,
                    "chat": chat_value,
                    "priorDraft": req.input.prior_draft,
                    "revision": req.input.revision,
                    "aborted": false,
                }));
                (r.await_abort, r.runs.len() - 1)
            };
            let publish = |f: &Value| {
                let _ = self
                    .events
                    .send(Event::scenario_builder_progress(&req.run_id, f.clone()));
            };
            if await_abort {
                publish(&self.frames[0]);
                for _ in 0..200 {
                    if req.abort.load(Ordering::SeqCst) {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(25)).await;
                }
                let aborted = req.abort.load(Ordering::SeqCst);
                self.recorder.lock().unwrap().runs[index]["aborted"] = json!(aborted);
                return Ok(ScenarioRunOutcome::Aborted);
            }
            for f in &self.frames {
                publish(f);
            }
            Ok(ScenarioRunOutcome::Done(
                self.frames.last().cloned().unwrap_or(Value::Null),
            ))
        })
    }
}

fn planted_instance(plants: &[Plant]) -> tempfile::TempDir {
    let base = common::materialize_fixture_instance();
    let w = quilltap_core::db::Writer::open_writable(
        &base.path().join("data").join("quilltap.db"),
        common::TEST_PEPPER,
    )
    .unwrap();
    for p in plants {
        w.connection()
            .execute(&p.sql, rusqlite::params_from_iter(p.params.iter()))
            .unwrap_or_else(|e| panic!("plant `{}`: {e}", p.sql));
    }
    drop(w);
    base
}

#[tokio::test(flavor = "multi_thread")]
async fn scenario_builder_routes_match_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_SB_ROUTES") else {
        eprintln!("SKIP: set QT_ORACLE_SB_ROUTES to the oracle NDJSON (see header).");
        return;
    };
    install_capture();
    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../harness/oracle/fixtures/scenario-builder-routes.json"),
        )
        .expect("read spec"),
    )
    .expect("parse spec");
    let text = std::fs::read_to_string(&oracle_path).expect("read oracle");
    assert!(!text.trim().is_empty(), "{oracle_path} is EMPTY");
    let oracle: HashMap<String, Value> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let v: Value = serde_json::from_str(l).expect("row");
            (v["name"].as_str().unwrap().to_string(), v)
        })
        .collect();
    assert_eq!(oracle.len(), spec.cases.len(), "oracle case count");
    assert!(
        spec.cases.len() >= 18,
        "v4's route.test.ts cases must all be armed"
    );

    let recorder = Arc::new(Mutex::new(Recorder::default()));
    let base = planted_instance(&spec.plants);
    let base_dir = base.path().to_path_buf();
    let maker: DriverMaker = {
        let recorder = Arc::clone(&recorder);
        let frames = spec.frames.clone();
        Arc::new(move |events| {
            Arc::new(CannedDriver {
                events: events.clone(),
                frames: frames.clone(),
                recorder: Arc::clone(&recorder),
            }) as _
        })
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
    let client = reqwest::Client::new();

    let mut failures: Vec<String> = Vec::new();
    let mut divergences_exercised = 0usize;
    let mut lines_seen = 0usize;
    for case in &spec.cases {
        let want = &oracle[&case.name];
        {
            let mut r = recorder.lock().unwrap();
            r.runs.clear();
            r.await_abort = case.await_abort;
        }
        LOGGED.lock().unwrap().clear();
        let url = if case.query.is_empty() {
            format!("http://{addr}/api/v1/scenario-builder")
        } else {
            format!("http://{addr}/api/v1/scenario-builder?{}", case.query)
        };
        let resp = if case.method == "GET" {
            client.get(&url).send().await.unwrap()
        } else {
            let body = match (&case.body, case.raw_body) {
                (Some(Value::String(raw)), true) => raw.clone(),
                (Some(v), _) => v.to_string(),
                (None, _) => String::new(),
            };
            client
                .post(&url)
                .header("content-type", "application/json")
                .body(body)
                .send()
                .await
                .unwrap()
        };
        let status = resp.status().as_u16();
        let header = |k: &str| {
            resp.headers()
                .get(k)
                .and_then(|v| v.to_str().ok())
                .map(str::to_string)
        };
        let content_type = header("content-type");
        let mut got = json!({ "status": status, "contentType": content_type });
        if content_type.as_deref() == Some("text/event-stream") {
            got["headers"] = json!({
                "cache-control": header("cache-control"),
                "connection": header("connection"),
            });
            if case.await_abort {
                // Read the first frame, then HANG UP — the disconnect guard
                // must reach the run's abort token.
                let mut stream = resp.bytes_stream();
                let first = tokio::time::timeout(Duration::from_secs(10), stream.next())
                    .await
                    .expect("the first frame arrives")
                    .expect("a chunk")
                    .expect("bytes");
                got["sse"] = json!(String::from_utf8_lossy(&first).to_string());
                drop(stream);
                for _ in 0..200 {
                    let done = recorder
                        .lock()
                        .unwrap()
                        .runs
                        .first()
                        .is_some_and(|r| r["aborted"] == json!(true));
                    if done {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(25)).await;
                }
                // The disconnect DEBUG is logged by a spawned task once the
                // abort dispatch answers — let it land (bounded) before the
                // case's lines are read.
                let want_len = want["lines"].as_array().map_or(0, Vec::len);
                for _ in 0..80 {
                    if LOGGED.lock().unwrap().len() >= want_len {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(25)).await;
                }
            } else {
                got["sse"] = json!(resp.text().await.unwrap());
            }
        } else {
            got["body"] = resp.json::<Value>().await.unwrap_or(Value::Null);
        }
        got["runs"] = json!(recorder.lock().unwrap().runs.clone());
        got["lines"] = json!(LOGGED.lock().unwrap().clone());

        if V4_MOCK_ONLY.contains(&case.name.as_str()) {
            divergences_exercised += 1;
            let v4_shape = want["status"] == json!(500)
                && want["body"]
                    == json!({ "error": "Failed to read Scenario Builder capabilities" });
            let v5_shape = got["status"] == json!(200)
                && got["body"] == json!({ "webSearchConfigured": false, "curlConfigured": false });
            if !v4_shape || !v5_shape {
                failures.push(format!(
                    "{}: WRONG SHAPE for the mock-only divergence\n  v4: {want}\n  v5: {got}",
                    case.name
                ));
            }
            continue;
        }
        if case.name == "returns the resolved capabilities" {
            // curl: v5 asserted false; v4's value recorded, not compared.
            if got["body"]["curlConfigured"] != json!(false) {
                failures.push(format!("{}: v5 curlConfigured must be false", case.name));
            }
            eprintln!(
                "recorded: v4 curlConfigured = {} (no plugins in the jest env)",
                want["body"]["curlConfigured"]
            );
            got["body"]["curlConfigured"] = want["body"]["curlConfigured"].clone();
        }

        let want_lines = want.get("lines").expect("the oracle row carries `lines`");
        lines_seen += want_lines.as_array().map_or(0, Vec::len);

        // `content-type` is compared only where v4 SETS it — the SSE response's
        // explicit headers. v4's JSON answers go through the jest env's
        // `NextResponse.json`, which carries no `content-type` at all (real
        // Next sends `application/json`, as v5's edge does), so on a JSON row
        // v4 has nothing to compare against.
        if want["contentType"].is_null() {
            got["contentType"] = Value::Null;
        }
        for key in [
            "status",
            "contentType",
            "headers",
            "body",
            "sse",
            "runs",
            "lines",
        ] {
            let (g, w) = (got.get(key), want.get(key));
            let (g, w) = (g.filter(|v| !v.is_null()), w.filter(|v| !v.is_null()));
            if g.map(Value::to_string) != w.map(Value::to_string) {
                failures.push(format!(
                    "{}: `{key}` differs\n  v4: {}\n  v5: {}",
                    case.name,
                    w.map(Value::to_string).unwrap_or_default(),
                    g.map(Value::to_string).unwrap_or_default()
                ));
            }
        }
    }
    drop(base);
    eprintln!(
        "scenario_builder_routes: {} cases, {lines_seen} v4 log lines compared",
        spec.cases.len()
    );
    assert!(
        lines_seen > 0,
        "the oracle recorded no ScenarioBuilder log line"
    );
    assert_eq!(divergences_exercised, V4_MOCK_ONLY.len());
    assert!(
        failures.is_empty(),
        "{} failure(s):\n{}",
        failures.len(),
        failures.join("\n")
    );
}
