//! The Scenario Builder dispatch family (P4.D217, v4 `d1c06cd9d`) — v4
//! `app/api/v1/scenario-builder/route.ts`: `POST ?action=build` (the run,
//! streamed) and `GET ?action=capabilities`, as three verbs:
//!
//! - `scenarioBuilderBuild { runId, body }` — `runId` is a CLIENT-minted uuid
//!   (v5's scope tag; v4 has none — its SSE response IS the run) and `body` is
//!   v4's request object RAW, so absent / `null` / wrong-typed keys reach the
//!   Zod twin exactly as v4's `safeParse(raw)` sees them.
//! - `scenarioBuilderAbort { runId }` → `{ aborted: bool }` (v5-only; v4's
//!   abort is the closed request).
//! - `scenarioBuilderCapabilities` → `{ webSearchConfigured, curlConfigured }`.
//!
//! ## Where each piece lives
//!
//! Everything DB-only and every refusal lives HERE, in v4's order
//! ([`scenario_builder_prepare`]): the Zod twin, the profile (owner-checked),
//! `allowToolUse`, the api-key resolution, the cast scoping (not a refusal),
//! the chat (owner- and type-checked), then the `accepted` DEBUG. The run
//! itself — which needs the streaming provider, the tool runner and the
//! detector only the composing host can build — rides the
//! [`ScenarioBuilderDriver`] seam (the `BrahmaConsoleSendDriver` precedent).
//!
//! **The run registry is ONE place: [`ScenarioBuilderRuns`], owned by the
//! engine.** A build registers its `runId` with a fresh abort token before
//! ANY v4 check (so a duplicate in-flight id answers the v5-only 409 first) and
//! unregisters when the run ends; `scenarioBuilderAbort` trips the token. The
//! token is v4's `AbortSignal`: the shared loop observes it between turns and
//! per chunk (P4.D216), and the service's catch reads it for the
//! "during a throw" line.
//!
//! **The frames** ride the Event channel as
//! `EventPayload::ScenarioBuilderProgress` with `progress_id = runId`; the
//! dispatch REPLY resolves after the run with the terminal frame's object, or
//! `{ aborted: true }`. The REST edge (`quilltap-web`'s
//! `scenario_builder_routes`) re-frames the same events into v4's exact SSE.
//!
//! ## One measured correction to the order's §S.1
//!
//! v4's validation DEBUG is `Scenario Builder request failed validation`
//! `{ issues: parsed.error.issues.length }` — the issue COUNT, not the issue
//! array the order's prose names (`route.ts:48`).

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde_json::{json, Value};

use crate::db::runtime::Db;
use crate::db::{characters_read, chats_read, connection_profiles, DbError};
use crate::services::api_key_service::{
    resolve_connection_profile_api_key, ProfileApiKeyResolution,
};
use crate::services::scenario_builder::request_schema::parse_scenario_build_request;
use crate::services::scenario_builder::{
    ScenarioBuilderChat, ScenarioBuilderInput, ScenarioRunOutcome,
};

use super::types::{CoreError, ErrorKind, Response};

/// The v5-only duplicate-id refusal (409), checked before every v4 check.
pub const DUPLICATE_RUN: &str = "A Scenario Builder run with that id is already in flight.";

/// v4's tool-less-profile refusal (`route.ts:58-60`).
pub const TOOLS_OFF: &str = "This connection profile has tool use switched off, and the Host cannot make enquiries without tools. Choose another profile.";

/// v4's wrong-chat-type refusal (`route.ts:95`).
pub const NOT_A_SALON: &str =
    "The Scenario Builder sets scenes only for Salon chats and autonomous rooms.";

/// One vetted build, handed to the driver: everything the route resolved
/// before `new ReadableStream`, plus v5's scope tag and abort token.
#[derive(Debug, Clone)]
pub struct ScenarioBuilderBuildRequest {
    pub user_id: String,
    /// The client-minted scope tag — the frames' `progressId`.
    pub run_id: String,
    /// The owner-checked connection profile ROW.
    pub connection_profile: Value,
    pub input: ScenarioBuilderInput,
    /// v4 `isWebSearchConfigured()` — the engine's host fact.
    pub web_search_configured: bool,
    /// v4's `req.signal` — tripped by `scenarioBuilderAbort` (or, on the REST
    /// edge, by the client disconnecting).
    pub abort: Arc<AtomicBool>,
}

/// The boxed future a [`ScenarioBuilderDriver`] returns.
pub type ScenarioBuilderFuture<'a> =
    Pin<Box<dyn Future<Output = Result<ScenarioRunOutcome, CoreError>> + Send + 'a>>;

/// One full Scenario Builder run: the service over the host's model
/// boundaries, each frame published on the engine's Event broadcast under
/// `progress_id = run_id`, the outcome returned. Only the composing host can
/// construct the streaming / tool bundle (the `BrahmaConsoleSendDriver`
/// precedent).
pub trait ScenarioBuilderDriver: Send + Sync {
    fn build(&self, req: ScenarioBuilderBuildRequest) -> ScenarioBuilderFuture<'_>;
}

/// The in-flight runs: `runId` → its abort token. The ONE registry.
#[derive(Default)]
pub struct ScenarioBuilderRuns {
    runs: Mutex<HashMap<String, Arc<AtomicBool>>>,
}

/// Unregisters its run when dropped — so a run removes itself however its
/// future ends (completed, refused, or dropped mid-await) — and TRIPS the
/// token as it goes. The run itself executes on the host driver's own thread,
/// which dropping the build future does not stop; without the trip, a caller
/// that went away mid-run (an HTTP client closing `/api/dispatch`, the REST
/// edge's pump ending) would leave that thread running to completion with no
/// registered id left to abort it by. On a run that already finished, the trip
/// is a no-op.
pub struct RunRegistration<'a> {
    runs: &'a ScenarioBuilderRuns,
    run_id: String,
    pub token: Arc<AtomicBool>,
}

impl Drop for RunRegistration<'_> {
    fn drop(&mut self) {
        self.token.store(true, Ordering::SeqCst);
        if let Ok(mut runs) = self.runs.runs.lock() {
            runs.remove(&self.run_id);
        }
    }
}

impl ScenarioBuilderRuns {
    /// Register `run_id`; `None` when a run with that id is already in flight.
    pub fn register(&self, run_id: &str) -> Option<RunRegistration<'_>> {
        let mut runs = self.runs.lock().ok()?;
        if runs.contains_key(run_id) {
            return None;
        }
        let token = Arc::new(AtomicBool::new(false));
        runs.insert(run_id.to_string(), Arc::clone(&token));
        Some(RunRegistration {
            runs: self,
            run_id: run_id.to_string(),
            token,
        })
    }

    /// Trip `run_id`'s token; `true` iff a run with that id was live. Never an
    /// error for an unknown id — a Stop can race the finish.
    pub fn abort(&self, run_id: &str) -> bool {
        match self.runs.lock() {
            Ok(runs) => match runs.get(run_id) {
                Some(token) => {
                    token.store(true, Ordering::SeqCst);
                    true
                }
                None => false,
            },
            Err(_) => false,
        }
    }
}

fn s<'v>(v: &'v Value, key: &str) -> Option<&'v str> {
    v.get(key).and_then(Value::as_str)
}

/// v4 `handleBuild` from `safeParse` to the `accepted` DEBUG
/// (`route.ts:44-107`), in v4's order — every refusal a `Response` before any
/// frame. Returns the vetted request (less the scope tag, token and host
/// fact, which the engine arm adds).
pub fn scenario_builder_prepare(
    db: &Db,
    user_id: &str,
    body: &Value,
) -> Result<(Value, ScenarioBuilderInput), Response> {
    let parsed = match parse_scenario_build_request(body) {
        Ok(p) => p,
        Err(issues) => {
            // `{ issues: parsed.error.issues.length }` — the COUNT (see the
            // module header).
            tracing::debug!(
                issues = issues.len(),
                "Scenario Builder request failed validation"
            );
            return Err(Response::validation_error(
                crate::api::zod_issues::zod_issue_details(&issues),
            ));
        }
    };

    // Profile: must be the user's, and must allow tools — the builder is a
    // tool loop. v4's `findById` is a fallback-mode `safeQuery`, so a failed
    // read is the same `null` as a miss.
    let pid = parsed.connection_profile_id.clone();
    let profile = db
        .read_main(move |c| connection_profiles::find_by_id(c, &pid))
        .ok()
        .flatten()
        .filter(|p| s(p, "userId") == Some(user_id));
    let Some(profile) = profile else {
        tracing::debug!(
            profileId = %parsed.connection_profile_id,
            "Scenario Builder profile not found for user"
        );
        return Err(Response::error(
            ErrorKind::NotFound,
            "Connection profile not found",
        ));
    };
    if profile.get("allowToolUse").and_then(Value::as_bool) == Some(false) {
        return Err(Response::error(ErrorKind::BadRequest, TOOLS_OFF));
    }

    let provider = s(&profile, "provider").unwrap_or_default().to_string();
    let key_id = s(&profile, "apiKeyId").map(str::to_string);
    let resolution = db
        .read_main(move |c| {
            Ok::<_, DbError>(resolve_connection_profile_api_key(
                c,
                &provider,
                key_id.as_deref(),
            ))
        })
        .unwrap_or(ProfileApiKeyResolution::Failed(
            crate::services::api_key_service::ProfileApiKeyFailure::ApiKeyNotFound,
        ));
    if let ProfileApiKeyResolution::Failed(reason) = resolution {
        return Err(Response::error(ErrorKind::BadRequest, reason.describe()));
    }

    // Cast: keep only ids that READ (the OVERLAID read, unlike the mount
    // pool's raw one). v4's comment says "`repos.characters` is user-scoped",
    // but its `findById` does NOT filter on the owner — measured by the route
    // family's `drops cast ids the user cannot read…` case, where v4 KEEPS a
    // planted character owned by another user and drops only the missing id.
    // Ported as measured; the mount pool's raw read still gives a foreign
    // member no vault and no groups (`mount_pool.rs`).
    let mut character_ids: Vec<String> = Vec::new();
    let mut seen: Vec<&str> = Vec::new();
    for id in &parsed.character_ids {
        if seen.contains(&id.as_str()) {
            continue;
        }
        seen.push(id);
        let cid = id.clone();
        let found = db.read_main(|main| {
            db.read_mount_index(|mount| characters_read::find_by_id(main, mount, &cid))
        });
        // `Ok(None)` and a failed read are both a miss, dropped SILENTLY. v4's route wraps
        // the read in a try/catch whose WARN `Scenario Builder dropped an
        // unreadable cast id` is unreachable: `characters.findById` is a
        // fallback-mode `safeQuery`, so a failing read answers `null` and
        // never throws. Measured by the route family's `drops a cast id
        // whose lookup throws` case (the d1c06cd9d unification review) —
        // v4 logs only the `{ requested, kept }` DEBUG below; v5 had
        // emitted the WARN too.
        if let Ok(Some(_)) = found {
            character_ids.push(id.clone());
        }
    }
    if character_ids.len() != parsed.character_ids.len() {
        tracing::debug!(
            requested = parsed.character_ids.len(),
            kept = character_ids.len(),
            "Scenario Builder dropped cast ids the user cannot read"
        );
    }

    // In-chat: the chat must be the user's, and a Salon or autonomous room.
    let mut chat: Option<ScenarioBuilderChat> = None;
    if let Some(chat_id) = parsed.chat_id() {
        let cid = chat_id.to_string();
        let found = db
            .read_main(move |c| chats_read::find_by_id(c, &cid))
            .ok()
            .flatten()
            .filter(|c| s(c, "userId") == Some(user_id));
        let Some(found) = found else {
            return Err(Response::error(ErrorKind::NotFound, "Chat not found"));
        };
        if !matches!(s(&found, "chatType"), Some("salon") | Some("autonomous")) {
            return Err(Response::error(ErrorKind::BadRequest, NOT_A_SALON));
        }
        chat = Some(ScenarioBuilderChat {
            id: s(&found, "id").unwrap_or(chat_id).to_string(),
            scenario_text: s(&found, "scenarioText").map(str::to_string),
            context_summary: s(&found, "contextSummary").map(str::to_string),
        });
    }

    tracing::debug!(
        mode = parsed.mode.as_str(),
        castCount = character_ids.len(),
        hasProject = parsed.project_id().is_some(),
        inChat = chat.is_some(),
        revising = parsed.revision().is_some(),
        profileId = s(&profile, "id").unwrap_or_default(),
        "Scenario Builder request accepted"
    );

    let input = ScenarioBuilderInput {
        mode: parsed.mode,
        location: parsed.location.clone(),
        time: parsed.time.clone(),
        details: parsed.details.clone(),
        project_id: parsed.project_id().map(str::to_string),
        character_ids,
        chat,
        prior_draft: parsed.prior_draft().map(str::to_string),
        revision: parsed.revision().map(str::to_string),
    };
    Ok((profile, input))
}

/// The dispatch reply body for a finished run: the terminal frame's object,
/// or `{ aborted: true }` (v5-only — v4's aborted run answers nothing).
pub fn outcome_body(outcome: ScenarioRunOutcome) -> Value {
    match outcome {
        ScenarioRunOutcome::Done(frame) | ScenarioRunOutcome::Failed(frame) => frame,
        ScenarioRunOutcome::Aborted => json!({ "aborted": true }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_duplicate_run_id_is_refused_until_the_first_ends() {
        let runs = ScenarioBuilderRuns::default();
        let first = runs.register("r1").expect("first registers");
        assert!(runs.register("r1").is_none(), "a live id refuses");
        assert!(runs.abort("r1"));
        assert!(first.token.load(Ordering::SeqCst));
        drop(first);
        assert!(!runs.abort("r1"), "an ended run is unknown");
        assert!(runs.register("r1").is_some(), "the id is free again");
    }

    #[test]
    fn dropping_a_registration_trips_its_token() {
        let runs = ScenarioBuilderRuns::default();
        let reg = runs.register("r2").unwrap();
        let token = Arc::clone(&reg.token);
        assert!(!token.load(Ordering::SeqCst));
        drop(reg);
        assert!(
            token.load(Ordering::SeqCst),
            "a dropped caller aborts its run"
        );
    }

    #[test]
    fn aborting_an_unknown_id_is_false_never_an_error() {
        assert!(!ScenarioBuilderRuns::default().abort("nope"));
    }
}
