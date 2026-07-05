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

use serde::ser::SerializeStruct;
use serde::{Serialize, Serializer};
use serde_json::Value;

use crate::services::carina_runner::{
    CarinaErrorKind, CarinaResult, PostProsperoCarinaError, ProsperoCarinaErrorArgs,
    RunCarinaQuery, RunCarinaQueryOptions,
};

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
pub fn execute_ask_carina(
    run_query: &mut dyn RunCarinaQuery,
    prospero: &mut dyn PostProsperoCarinaError,
    user_id: &str,
    chat_id: &str,
    calling_participant_id: Option<&str>,
    args: &Value,
) -> AskCarinaOutput {
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

    match run_query.run(opts) {
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
