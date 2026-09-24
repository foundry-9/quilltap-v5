//! The d1c06cd9d unification review (S3) — **a driver failure AFTER the stream
//! has committed ends it with v4's error frame and ERROR line.**
//!
//! v4 `app/api/v1/scenario-builder/route.ts:156-163`: when `runScenarioBuilder`
//! throws, the route logs ERROR `Scenario Builder stream failed` with
//! `{ error: error.message }` and enqueues
//! `data: {"error":"The Host could not complete the enquiry."}\n\n` before
//! closing. v5's equivalent throw is the driver resolving `Err` (the spine's
//! panicked-thread arm). Before the first frame the re-framer answers that as
//! a JSON 500; AFTER one, the committed pump used to drop the `Err` and end the
//! SSE with no terminal frame and nothing logged.
//!
//! A canned driver publishes ONE frame, lets the pump commit, then fails.
//!
//! Run:
//!   cargo test -p quilltap-web --test scenario_builder_midstream_failure

mod common;
mod scenario_builder_spine;

use std::sync::Arc;
use std::time::Duration;

use quilltap_core::api::scenario_builder::{
    ScenarioBuilderBuildRequest, ScenarioBuilderDriver, ScenarioBuilderFuture,
};
use quilltap_core::api::types::{CoreError, ErrorKind, Event};
use quilltap_core::test_support::global_capture;
use serde_json::json;

use scenario_builder_spine::{Canned, ScenarioBuilderSpineFactory};

/// The driver's failure message (the spine's panicked-thread arm's text).
const FAILURE: &str = "scenario builder thread panicked";

/// Publishes one frame, waits long enough for the pump to commit, then fails.
struct FailAfterFrame {
    events: tokio::sync::broadcast::Sender<Event>,
}

impl ScenarioBuilderDriver for FailAfterFrame {
    fn build(&self, req: ScenarioBuilderBuildRequest) -> ScenarioBuilderFuture<'_> {
        Box::pin(async move {
            let _ = self.events.send(Event::scenario_builder_progress(
                &req.run_id,
                json!({ "reasoning": "weighing the quay" }),
            ));
            tokio::time::sleep(Duration::from_millis(200)).await;
            Err(CoreError {
                kind: ErrorKind::Internal,
                message: FAILURE.to_string(),
                pepper_state: None,
                code: None,
                associations: None,
                character_id: None,
                entity: None,
                details: None,
                already_saved: None,
            })
        })
    }
}

#[tokio::test]
async fn a_driver_failure_after_the_first_frame_ends_with_v4s_error_frame() {
    global_capture::install();
    let (body, lines) = global_capture::capture_async(async {
        let base = common::materialize_fixture_instance();
        {
            // One tools-on OLLAMA profile (no key), cloned from the fixture's
            // own row on this per-run copy (the disconnect test's plant).
            let w = quilltap_core::db::Writer::open_writable(
                &base.path().join("data").join("quilltap.db"),
                common::TEST_PEPPER,
            )
            .unwrap();
            w.connection()
                .execute_batch(
                    "CREATE TEMP TABLE \"s3_clone\" AS SELECT * FROM \"connection_profiles\" ORDER BY rowid LIMIT 1;
                     UPDATE \"s3_clone\" SET \"id\" = '5b170000-0000-4000-8000-0000000000a1', \"name\" = 'Host OK', \"provider\" = 'OLLAMA', \"apiKeyId\" = NULL, \"allowToolUse\" = 1, \"isDefault\" = 0;
                     INSERT INTO \"connection_profiles\" SELECT * FROM \"s3_clone\";
                     DROP TABLE \"s3_clone\";",
                )
                .unwrap();
        }
        let base_dir = base.path().to_path_buf();
        let (addr, _state) = common::serve_instance(base.path(), move |mut c| {
            c.terminal = false;
            c.spine = Some(Arc::new(ScenarioBuilderSpineFactory {
                base_dir,
                canned: Canned::Scene,
                driver: Some(Arc::new(|events: &tokio::sync::broadcast::Sender<Event>| {
                    Arc::new(FailAfterFrame {
                        events: events.clone(),
                    }) as Arc<dyn ScenarioBuilderDriver>
                })),
            }));
            c
        })
        .await;

        let resp = reqwest::Client::new()
            .post(format!("http://{addr}/api/v1/scenario-builder?action=build"))
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
        assert_eq!(resp.status(), 200, "the stream committed on the first frame");
        assert_eq!(
            resp.headers().get("content-type").unwrap(),
            "text/event-stream"
        );
        let body = tokio::time::timeout(Duration::from_secs(15), resp.text())
            .await
            .expect("the stream ends")
            .unwrap();
        drop(base);
        body
    })
    .await;

    // (a) v4's bytes, and the error frame is LAST.
    assert_eq!(
        body,
        "data: {\"reasoning\":\"weighing the quay\"}\n\n\
         data: {\"error\":\"The Host could not complete the enquiry.\"}\n\n",
        "the committed stream must end with v4's error frame"
    );
    // (b) v4's route-level ERROR line, with the failure's message.
    let line = lines
        .iter()
        .find(|l| l.contains("Scenario Builder stream failed"))
        .unwrap_or_else(|| panic!("no `Scenario Builder stream failed` line: {lines:#?}"));
    assert!(line.starts_with("ERROR "), "{line}");
    assert!(line.contains(&format!("error={FAILURE}")), "{line}");
}
