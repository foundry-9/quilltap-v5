//! Port of v4's `ask_carina` tool (`lib/tools/handlers/ask-carina-handler.ts` +
//! `lib/tools/ask-carina-tool.ts`).
//!
//! Routes a quick isolated question to the named answerer via Carina. On success
//! the Carina service (the seam) already posts the answer as a chat message; this
//! handler surfaces the answer text to the calling LLM. On failure it posts a
//! Prospero error announcement (the seam) and returns a short human-readable error.
//!
//! ## Seams (reused from `services::carina_runner`)
//!
//! - [`RunCarinaQuery`] — v4's `runCarinaQuery` (the whole Carina query engine,
//!   still unported; W4.5). It OWNS the answer-posting + the live `onPosted` emit.
//! - [`PostProsperoCarinaError`] — v4's `postProsperoCarinaError` (post-office /
//!   Prospero writer). Recorded by the harness.
//!
//! ## Tracked deferral: `emitCarinaAnswer` (onPosted)
//!
//! v4's handler forwards `context.emitCarinaAnswer` as `runCarinaQuery`'s
//! `onPosted` (the live SSE callback). The [`ToolExecutionContext`] carries that
//! as a documented-deferral slot (it reaches the orchestrator's live stream, owned
//! by a unit frozen this round), so this port does not forward it — the seam owns
//! posting and the live emit lands when the slot is closed. The differential's
//! canned seam does not emit, so nothing is unverified here.
//!
//! [`ToolExecutionContext`]: crate::services::tool_execution::ToolExecutionContext

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use serde::ser::SerializeStruct;
use serde::{Serialize, Serializer};
use serde_json::Value;

use crate::db::runtime::Db;
use crate::model::embedding::EmbeddingProvider;
use crate::model::stream::StreamingCompletionProvider;
use crate::services::carina_query::{CarinaQueryDeps, RealCarinaQuery, RunBrahmaConsole};
use crate::services::carina_runner::{
    CarinaErrorKind, CarinaResult, PostProsperoCarinaError, ProsperoCarinaErrorArgs,
    RunCarinaQuery, RunCarinaQueryOptions,
};
use crate::services::chat_events::EventSink;
use crate::services::native_tool_loop::ToolCallDetector;
use crate::services::tool_execution::ToolRunner;

/// v4 `AskCarinaToolOutput` — `{ success, answer, error? }`. Success omits `error`.
#[derive(Debug, Clone)]
pub struct AskCarinaOutput {
    pub success: bool,
    pub answer: String,
    pub error: Option<String>,
}

impl Serialize for AskCarinaOutput {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let len = if self.error.is_some() { 3 } else { 2 };
        let mut st = s.serialize_struct("AskCarinaOutput", len)?;
        st.serialize_field("success", &self.success)?;
        st.serialize_field("answer", &self.answer)?;
        if let Some(e) = &self.error {
            st.serialize_field("error", e)?;
        }
        st.end()
    }
}

/// v4 `errorMessage(kind, detail)` — a short human-readable error string.
fn error_message(kind: CarinaErrorKind, detail: Option<&str>) -> String {
    match kind {
        CarinaErrorKind::NotFound => "No answerer by that name is on duty.".to_string(),
        CarinaErrorKind::NoProfile => {
            "The answerer has no connection to an LLM provider.".to_string()
        }
        CarinaErrorKind::LlmFailed => match detail {
            Some(d) => format!("The answerer was unable to respond — {d}."),
            None => "The answerer was unable to respond.".to_string(),
        },
    }
}

/// v4 `validateAskCarinaInput`: `character` + `question` non-empty strings;
/// `whisper` optional boolean (default false).
fn validate(args: &Value) -> Option<(String, String, bool)> {
    let obj = args.as_object()?;
    let character = match obj.get("character") {
        Some(Value::String(s)) if !s.is_empty() => s.clone(),
        _ => return None,
    };
    let question = match obj.get("question") {
        Some(Value::String(s)) if !s.is_empty() => s.clone(),
        _ => return None,
    };
    let whisper = match obj.get("whisper") {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        _ => return None,
    };
    Some((character, question, whisper))
}

/// Execute the `ask_carina` tool (v4 `executeAskCarinaTool`). The two Carina seams
/// are injected (both mocked identically on the oracle side).
pub async fn execute_ask_carina<Q, P>(
    run_query: &mut Q,
    prospero: &mut P,
    user_id: &str,
    chat_id: &str,
    calling_participant_id: Option<&str>,
    args: &Value,
) -> AskCarinaOutput
where
    Q: RunCarinaQuery,
    P: PostProsperoCarinaError,
{
    let Some((character, question, whisper)) = validate(args) else {
        return AskCarinaOutput {
            success: false,
            answer: String::new(),
            error: Some("Invalid input: character and question are required".to_string()),
        };
    };

    let opts = RunCarinaQueryOptions {
        user_id: user_id.to_string(),
        chat_id: chat_id.to_string(),
        character_name: character,
        question,
        whisper,
        asker_participant_id: calling_participant_id.map(str::to_string),
        // A character asking via the tool is not the operator.
        operator_initiated: false,
    };

    match run_query.run(opts).await {
        Ok(CarinaResult::Ok { answer, .. }) => AskCarinaOutput {
            success: true,
            answer,
            error: None,
        },
        Ok(CarinaResult::Err { error }) => {
            // Post a Prospero error announcement (fire-and-forget seam).
            let _ = prospero.post(ProsperoCarinaErrorArgs {
                chat_id: chat_id.to_string(),
                kind: error.kind,
                character_name: error.character_name.clone(),
                detail: error.detail.clone(),
                whisper,
                asker_participant_id: calling_participant_id.map(str::to_string),
            });
            AskCarinaOutput {
                success: false,
                answer: String::new(),
                error: Some(error_message(error.kind, error.detail.as_deref())),
            }
        }
        // v4's outer catch: `error.message`.
        Err(e) => AskCarinaOutput {
            success: false,
            answer: String::new(),
            error: Some(e.0),
        },
    }
}

/// v4 `formatAskCarinaResults`: `success ? answer : (error || 'Carina was unable
/// to answer.')`.
pub fn format_ask_carina_results(out: &AskCarinaOutput) -> String {
    if out.success {
        out.answer.clone()
    } else {
        out.error
            .clone()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "Carina was unable to answer.".to_string())
    }
}

// ===========================================================================
// The erased ask_carina dispatch seam (W4.10a)
// ===========================================================================

/// Re-satisfies `EventSink` for a `&dyn EventSink` (the engine's `SNK` bound is a
/// concrete generic; a trait object is not itself the trait).
struct SinkRef<'s>(&'s (dyn EventSink + Sync));
impl EventSink for SinkRef<'_> {
    fn emit(&self, event: crate::services::chat_events::ChatEvent) {
        self.0.emit(event);
    }
}

/// The object-safe ask_carina boundary the tool dispatcher holds behind an
/// `Arc<dyn …>`, so `ask_carina` routes without threading the Carina engine's
/// six generics through [`BuiltInToolRunner`](crate::tools::executor::BuiltInToolRunner)
/// (the [`ErasedImageGeneration`](crate::tools::generate_image::ErasedImageGeneration)
/// precedent). The per-turn live SSE sink (v4's `context.emitCarinaAnswer` →
/// `onPosted`) is passed as a parameter, since the engine owns the post + emit.
pub trait AskCarinaRunner: Send + Sync {
    fn run<'a>(
        &'a self,
        sink: &'a (dyn EventSink + Sync),
        user_id: &'a str,
        chat_id: &'a str,
        calling_participant_id: Option<&'a str>,
        args: &'a Value,
    ) -> Pin<Box<dyn Future<Output = AskCarinaOutput> + Send + 'a>>;
}

/// A type-erased ask_carina runner (an `Arc<dyn …>`).
#[derive(Clone)]
pub struct ErasedAskCarina(Arc<dyn AskCarinaRunner>);

impl ErasedAskCarina {
    /// Wrap a concrete runner.
    pub fn new<R: AskCarinaRunner + 'static>(runner: R) -> Self {
        Self(Arc::new(runner))
    }

    /// The not-available default (no Carina engine wired) — reproduces the loud
    /// fallback [`BuiltInToolRunner`](crate::tools::executor::BuiltInToolRunner)
    /// returned for `ask_carina` before this seam existed, so every existing
    /// construction site behaves unchanged.
    pub fn not_available() -> Self {
        Self(Arc::new(NotAvailableAskCarina))
    }

    /// Run the handler.
    pub async fn run(
        &self,
        sink: &(dyn EventSink + Sync),
        user_id: &str,
        chat_id: &str,
        calling_participant_id: Option<&str>,
        args: &Value,
    ) -> AskCarinaOutput {
        self.0
            .run(sink, user_id, chat_id, calling_participant_id, args)
            .await
    }
}

impl Default for ErasedAskCarina {
    fn default() -> Self {
        Self::not_available()
    }
}

/// The default runner: no Carina engine is wired into this build. Returns the
/// exact loud-fallback message the [`LoudFallbackRunner`] produced for a
/// recognized-but-unported built-in, so moving `ask_carina` into the dispatched
/// set is behaviorally inert until a real engine is injected.
///
/// [`LoudFallbackRunner`]: crate::tools::executor::LoudFallbackRunner
struct NotAvailableAskCarina;
impl AskCarinaRunner for NotAvailableAskCarina {
    fn run<'a>(
        &'a self,
        _sink: &'a (dyn EventSink + Sync),
        _user_id: &'a str,
        _chat_id: &'a str,
        _calling_participant_id: Option<&'a str>,
        _args: &'a Value,
    ) -> Pin<Box<dyn Future<Output = AskCarinaOutput> + Send + 'a>> {
        Box::pin(async move {
            AskCarinaOutput {
                success: false,
                answer: String::new(),
                error: Some(
                    "Tool 'ask_carina' is recognized but not yet available in this build"
                        .to_string(),
                ),
            }
        })
    }
}

/// A concrete [`AskCarinaRunner`] over owned Carina-engine seams — the production
/// host + the differential each build one and erase it into [`ErasedAskCarina`]
/// (the [`TypedImageGeneration`](crate::tools::generate_image::TypedImageGeneration)
/// precedent). Owns everything the [`CarinaQueryDeps`] needs EXCEPT the per-turn
/// SSE `sink` (a `run` parameter). `PostProsperoCarinaError` is cloned per call
/// (its writer is stateless over the [`Db`]), avoiding a `!Send` mutex guard
/// across the engine's await.
///
/// `tool_runner` must NOT be the same [`BuiltInToolRunner`](crate::tools::executor::BuiltInToolRunner)
/// that holds this seam (a type-level cycle); Carina filters `ask_carina` from its
/// own slate, so a separate runner suffices.
pub struct TypedAskCarina<EMB, STR, TR, TD, BRA, P>
where
    EMB: EmbeddingProvider,
    STR: StreamingCompletionProvider,
    TR: ToolRunner,
    TD: ToolCallDetector,
    BRA: RunBrahmaConsole,
    P: PostProsperoCarinaError + Clone,
{
    pub db: Db,
    pub embedding: EMB,
    pub streaming: STR,
    pub tool_runner: TR,
    pub tool_detector: TD,
    pub brahma: BRA,
    pub prospero: P,
    pub model_supports_native_tools: bool,
    pub now_ms: f64,
}

impl<EMB, STR, TR, TD, BRA, P> AskCarinaRunner for TypedAskCarina<EMB, STR, TR, TD, BRA, P>
where
    EMB: EmbeddingProvider + Send + Sync,
    STR: StreamingCompletionProvider + Send + Sync,
    TR: ToolRunner + Send + Sync,
    TD: ToolCallDetector + Send + Sync,
    BRA: RunBrahmaConsole + Send + Sync,
    P: PostProsperoCarinaError + Clone + Send + Sync,
{
    fn run<'a>(
        &'a self,
        sink: &'a (dyn EventSink + Sync),
        user_id: &'a str,
        chat_id: &'a str,
        calling_participant_id: Option<&'a str>,
        args: &'a Value,
    ) -> Pin<Box<dyn Future<Output = AskCarinaOutput> + Send + 'a>> {
        Box::pin(async move {
            // The engine's `SNK: EventSink` is a concrete generic; the per-turn
            // sink arrives as `&dyn EventSink`, so a thin newtype re-satisfies the
            // bound (`dyn EventSink` is not itself `EventSink`).
            let sink_ref = SinkRef(sink);
            let deps = CarinaQueryDeps {
                db: &self.db,
                embedding: &self.embedding,
                streaming: &self.streaming,
                tool_runner: &self.tool_runner,
                tool_detector: &self.tool_detector,
                sink: &sink_ref,
                brahma: &self.brahma,
                model_supports_native_tools: self.model_supports_native_tools,
                now_ms: self.now_ms,
            };
            let mut query = RealCarinaQuery::new(deps);
            let mut prospero = self.prospero.clone();
            execute_ask_carina(
                &mut query,
                &mut prospero,
                user_id,
                chat_id,
                calling_participant_id,
                args,
            )
            .await
        })
    }
}
