//! P4.D180 — the in-scene voice rehearsal over the host's LIVE assembly.
//!
//! The handler and the service are differential-proven
//! (`in_scene_voiced_tier3_equivalence`, 27 cases). What needs proving HERE is
//! the plumbing the differential structurally cannot see:
//!
//!   1. **The driver is actually WIRED.** A spine-less host answers the named
//!      not-assembled refusal, and that refusal is indistinguishable from a
//!      wired path by the dispatch envelope alone — so the test boots
//!      `ProductionSpineFactory`, the assembly that holds it
//!      (`web-test-venue-has-no-spine-factory`). P4.9E2A's sibling seam shipped
//!      `None` for a whole round without anyone noticing.
//!   2. **The rehearsal lands on an `llm_logs` row.** The tier-3 oracle cannot
//!      see this at all: `jest.setup` no-ops `logLLMCall` wholesale
//!      (`jest-setup-llm-logging-service-mocked`), so ZERO rows are written on
//!      v4's side and the log type is not a tier-3 comparand. The host's
//!      `HostInSceneVoiceRunner` rebuilds a LOGGING cheap executor per call for
//!      exactly this reason, and this is the only place that claim is tested.
//!   3. **The verb resolves over `/api/dispatch`** — there is no REST edge for
//!      this action (measured: `impersonation-voice-preview` appears nowhere in
//!      `crates/quilltap-web/src`, and its `announcement-preview` sibling is
//!      dispatch-only too).
//!
//! **No spend.** The fixture's seat profile points at `http://127.0.0.1:1/v1`,
//! where nothing listens, so the provider call fails at the socket. That is
//! enough for both claims: a socket failure proves the driver RAN (the refusal
//! sentence would have come back instead), and `logLLMCall` writes its row for
//! the failed call the same as for a successful one.
//!
//! Run:
//!   cargo test -p quilltap-web --test impersonation_voice_preview_wire

mod common;

use serde_json::{json, Value};

const CHAT_STACK: &str = "c1000000-0000-4000-8000-000000000001";
const P_VESPER: &str = "e1000000-0000-4000-8000-000000000001";
/// NOT in the chat's `impersonatingParticipantIds` — the 400 through the wire.
const P_BRAM: &str = "e1000000-0000-4000-8000-000000000002";

/// ⚠ **ACTIVATE AT UNIFY.** P4.D179 adds the `VOICE_REWRITE` log type and maps
/// BOTH voice task types to it; until both lanes are on one branch this lane's
/// tree still carries v4's pre-fix silent default, under which
/// `impersonation-voice-rewrite` files as `SUMMARIZATION`
/// (`services/llm_logging.rs`, `map_task_type_to_log_type`). **The unifier flips
/// this to `"VOICE_REWRITE"`** and the assertion below must go green on the
/// flip — red-first on the unify branch if it does not.
const EXPECTED_REHEARSAL_LOG_TYPE: &str = "VOICE_REWRITE";

async fn dispatch(
    client: &reqwest::Client,
    addr: &std::net::SocketAddr,
    body: Value,
) -> (u16, Value) {
    let resp = client
        .post(format!("http://{addr}/api/dispatch"))
        .json(&body)
        .send()
        .await
        .unwrap();
    let status = resp.status().as_u16();
    (status, resp.json().await.unwrap())
}

#[tokio::test(flavor = "multi_thread")]
async fn the_rehearsal_runs_over_the_live_assembly_and_logs_its_call() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("quilltap=warn")
        .try_init();
    let base = common::materialize_in_scene_voiced_instance();
    let base_dir = base.path().to_path_buf();
    let (addr, _state) = common::serve_instance(base.path(), move |mut c| {
        c.terminal = false;
        // The PRODUCTION spine, as every deployment shell boots it — without
        // this the driver arrives `None` and the arm answers its refusal.
        c.spine = Some(std::sync::Arc::new(
            quilltap_host::ProductionSpineFactory::new(base_dir, c.version.clone(), c.tz.clone()),
        ));
        c
    })
    .await;
    let client = reqwest::Client::new();

    // 1. The seat that IS impersonated. The provider is unreachable, so the
    //    rehearsal fails — but it fails at the SOCKET, which is the proof the
    //    driver ran rather than refusing.
    let (status, body) = dispatch(
        &client,
        &addr,
        json!({
            "type": "chatImpersonationVoicePreview",
            "chatId": CHAT_STACK,
            "participantId": P_VESPER,
            "seedMarkdown": "Tell them the lamps are out and I am seeing to it.",
        }),
    )
    .await;
    // ⚠ The dispatch envelope nests the sentence under `data.message`; reading
    // `body["error"]` here yields `None` and every `contains` check on it passes
    // VACUOUSLY. The first run of this test did exactly that.
    let err = err_message(&body).unwrap_or_else(|| panic!("expected an error envelope: {body}"));
    assert!(
        !err.contains("is not available: the host has not assembled"),
        "the driver was NOT wired — the arm answered its not-assembled refusal: {err}"
    );
    assert_eq!(
        status, 400,
        "an unreachable provider is v4's preview failure, not a transport error: {body}"
    );

    // 2. …and the call landed on an `llm_logs` row. THE point of this file:
    //    nothing else in the tree can see it.
    let logs = read_rehearsal_logs(base.path());
    assert_eq!(
        logs.len(),
        1,
        "the rehearsal must write exactly one llm_logs row, got {logs:?}"
    );
    let row = &logs[0];
    assert_eq!(
        row.0, EXPECTED_REHEARSAL_LOG_TYPE,
        "see EXPECTED_REHEARSAL_LOG_TYPE — flipped at the `f4ad2c8d1` unification with P4.D179's map"
    );
    assert_eq!(
        row.1.as_deref(),
        Some(CHAT_STACK),
        "the per-call logging executor must carry the request's own chat"
    );
    assert!(
        row.2.is_none(),
        "v4 passes no messageId for a rehearsal (`message_id: None`)"
    );

    // 3. A seat that is NOT being impersonated refuses at v4's sentence, through
    //    the same wire — the overlay on the chat row is the authority.
    let (status, body) = dispatch(
        &client,
        &addr,
        json!({
            "type": "chatImpersonationVoicePreview",
            "chatId": CHAT_STACK,
            "participantId": P_BRAM,
            "seedMarkdown": "The lamps are out.",
        }),
    )
    .await;
    assert_eq!(status, 400, "{body}");
    assert_eq!(
        err_message(&body).as_deref(),
        Some("That seat is not being impersonated."),
        "{body}"
    );
    // …and it wrote nothing: the refusal precedes the driver.
    assert_eq!(read_rehearsal_logs(base.path()).len(), 1);
}

/// The sentence out of the dispatch error envelope
/// (`{"type":"error","data":{"kind","message"}}`) — NOT `body["error"]`, which
/// is always absent here and makes every assertion on it vacuous.
fn err_message(body: &Value) -> Option<String> {
    body.get("data")?
        .get("message")?
        .as_str()
        .map(str::to_string)
}

/// `(type, chatId, messageId)` for every row the rehearsal's task type wrote.
fn read_rehearsal_logs(base: &std::path::Path) -> Vec<(String, Option<String>, Option<String>)> {
    let db = base.path_llm_logs();
    let w = quilltap_core::db::Writer::open_writable(&db, common::TEST_PEPPER).unwrap();
    let mut stmt = w
        .connection()
        .prepare("SELECT type, chatId, messageId FROM llm_logs ORDER BY createdAt")
        .unwrap();
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, Option<String>>(2)?,
            ))
        })
        .unwrap()
        .filter_map(Result::ok)
        .collect();
    rows
}

/// Tiny path helper so the assertion above reads as prose.
trait LlmLogsPath {
    fn path_llm_logs(&self) -> std::path::PathBuf;
}
impl LlmLogsPath for std::path::Path {
    fn path_llm_logs(&self) -> std::path::PathBuf {
        self.join("data").join("quilltap-llm-logs.db")
    }
}
