//! Answer-confirmation service (v4
//! `lib/services/chat-message/answer-confirmation.service.ts`).
//!
//! Before a character's tool-using Salon reply "lands", a cheap-LLM consistency
//! check compares the reply against what the character was told this turn (the
//! Commonplace Book whisper) and what it looked up (in-scope read-tool results).
//! If the reply looks inconsistent, the character's OWN model is invited to
//! affirm or rewrite it.
//!
//! ```text
//!   consistent             → confirmed:true
//!   inconsistent, stood by → confirmed:false  (notes = discrepancies)
//!   inconsistent, rewrote  → confirmed:true, revised:true  (original kept for logs)
//!   check errored/timeout  → confirmed:null   (could-not-verify)
//! ```
//!
//! The whole block is a no-op when the feature is inactive or there is nothing
//! to check (no whisper AND no in-scope read tool this turn) — those gates live
//! in the finalizer ([`crate::services::message_finalizer`]), which composes
//! this service's leaves + [`run_answer_confirmation`].
//!
//! ## Seams / deferrals (tracked)
//!
//!   - **Model boundary** — the check + re-affirmation go through
//!     [`CheapLlmTaskExecutor`](crate::services::cheap_llm_exec::CheapLlmTaskExecutor)
//!     ([`CompletionProvider`]), exactly where the v4 oracle mocks
//!     `createLLMProvider`. API-key acquisition + `logLLMCall` stay host-side (the
//!     `cheap_llm_exec` boundary).
//!   - **Timeouts** — v4 wraps each call in a 25 s / 60 s `withTimeout` that
//!     rejects a hung promise to the error outcome. Timing is host-side (no tokio
//!     timers in the core; the same treatment as the gate's 500 ms retry delay).
//!     The failure → `confirmed: null` mapping IS ported; only the timer is
//!     omitted.

use serde_json::Value;

use crate::cheap_llm::{
    resolve_uncensored_cheap_llm_selection, CheapLlmProfile, CheapLlmSelection,
    DangerousContentSettings, UncensoredFallbackOptions,
};
use crate::jsstr::{js_trim, utf16_len, utf16_slice_from};
use crate::model::completion::{CompletionMessage, CompletionProvider};
use crate::services::cheap_llm_exec::CheapLlmTaskExecutor;
use crate::services::tool_execution::ToolMessage;

pub mod prompt_text;

/// Read tools whose results are worth checking a reply against (v4
/// `CONFIRMATION_READ_TOOLS`). Content reads only — web search, prior-conversation
/// reads, and the Scriptorium `doc_*` content-read family. Listing / binary-blob
/// reads are intentionally excluded. A **static** name set — no runtime tool
/// introspection.
pub const CONFIRMATION_READ_TOOLS: &[&str] = &[
    "search",
    "read_conversation",
    "doc_read_file",
    "doc_grep",
    "doc_read_heading",
    "doc_read_frontmatter",
    "doc_open_document",
];

fn is_confirmation_read_tool(tool_name: &str) -> bool {
    CONFIRMATION_READ_TOOLS.contains(&tool_name)
}

/// Char budget for the assembled reference block handed to the cheap LLM (v4
/// `REFERENCE_CHAR_BUDGET`). UTF-16 code units.
const REFERENCE_CHAR_BUDGET: usize = 24_000;

/// v4 `AnswerConfirmationOverride` — a per-chat / per-project override string:
/// `"ON"` / `"OFF"` / absent.
pub type AnswerConfirmationOverride<'a> = Option<&'a str>;

/// Resolve whether the answer-confirmation check is active for a chat, given the
/// three-level gate: per-chat override wins, then the per-project override, then
/// the global default (v4 `isAnswerConfirmationActive`). `'OFF'` always beats an
/// inherited `'ON'` at its own level because it is checked first.
pub fn is_answer_confirmation_active(
    chat_override: AnswerConfirmationOverride<'_>,
    project_override: AnswerConfirmationOverride<'_>,
    global_enabled: bool,
) -> bool {
    if chat_override == Some("ON") {
        return true;
    }
    if chat_override == Some("OFF") {
        return false;
    }
    if project_override == Some("ON") {
        return true;
    }
    if project_override == Some("OFF") {
        return false;
    }
    global_enabled
}

/// True when there is something to check this turn: a Commonplace Book whisper
/// was found for this character AND/OR at least one in-scope read-tool result ran
/// (v4 `hasCheckableInputs`). Plain turns with neither are skipped entirely.
pub fn has_checkable_inputs(whisper: Option<&str>, tool_messages: &[ToolMessage]) -> bool {
    if whisper.map(|w| !js_trim(w).is_empty()) == Some(true) {
        return true;
    }
    tool_messages
        .iter()
        .any(|tm| is_confirmation_read_tool(&tm.tool_name))
}

/// The most-recent Commonplace Book whisper targeted at this character this turn
/// (v4 `findLatestCommonplaceWhisper`). Whispers are written (targeted, private)
/// just before generation, so the last `commonplaceBook` message addressed to
/// this participant is the one the character saw. Returns its raw `content`, or
/// `None` if none.
///
/// Operates on the raw `chat_messages` rows ([`crate::db::chats_messages_read`]'s
/// JSON): each must be `type: "message"`, `systemSender: "commonplaceBook"`, its
/// `targetParticipantIds` array include `participant_id`, and its `content` be a
/// non-blank string.
pub fn find_latest_commonplace_whisper(messages: &[Value], participant_id: &str) -> Option<String> {
    for m in messages.iter().rev() {
        if m.get("type").and_then(Value::as_str) != Some("message") {
            continue;
        }
        if m.get("systemSender").and_then(Value::as_str) != Some("commonplaceBook") {
            continue;
        }
        let targets_include = m
            .get("targetParticipantIds")
            .and_then(Value::as_array)
            .is_some_and(|arr| arr.iter().any(|v| v.as_str() == Some(participant_id)));
        if !targets_include {
            continue;
        }
        if let Some(content) = m.get("content").and_then(Value::as_str) {
            if !js_trim(content).is_empty() {
                return Some(content.to_string());
            }
        }
    }
    None
}

/// True when this turn was authored by a user-controlled participant
/// (impersonation) (v4 `isUserDrivenTurn`). Such replies are never checked — the
/// human may have sourced facts out of band — and are persisted with an explicit
/// `confirmed: null`.
///
/// `participants` are the raw chat participant JSON objects; a participant matches
/// on `id`, and is user-driven when its `controlledBy` is `"user"`.
pub fn is_user_driven_turn(
    impersonating_participant_ids: &[String],
    participants: &[Value],
    participant_id: &str,
) -> bool {
    if impersonating_participant_ids
        .iter()
        .any(|id| id == participant_id)
    {
        return true;
    }
    participants
        .iter()
        .find(|p| p.get("id").and_then(Value::as_str) == Some(participant_id))
        .and_then(|p| p.get("controlledBy").and_then(Value::as_str))
        == Some("user")
}

/// Compactly serialize an in-scope tool result for the reference block (v4
/// `serializeToolResult`).
fn serialize_tool_result(tm: &ToolMessage) -> String {
    let args = match &tm.arguments {
        Some(Value::Object(map)) if !map.is_empty() => format!(
            "\nArguments: {}",
            serde_json::to_string(&tm.arguments).unwrap_or_default()
        ),
        // v4 `Object.keys(tm.arguments).length > 0` — a non-object arguments value
        // (never produced by the tool path) throws in JS; here it is treated as
        // "no arguments" (the corpus only carries object arguments).
        _ => String::new(),
    };
    let failed = if tm.success { "" } else { " (failed)" };
    format!(
        "[Tool: {}{}]{}\nResult: {}",
        tm.tool_name, failed, args, tm.content
    )
}

/// Assemble the reference block (whisper + in-scope tool results) the cheap LLM
/// checks the reply against (v4 `gatherConfirmationInputs`). Returns `None` when
/// there is nothing to check. Truncates **oldest-first** to keep within the char
/// budget (UTF-16 semantics).
pub fn gather_confirmation_inputs(
    whisper: Option<&str>,
    tool_messages: &[ToolMessage],
) -> Option<String> {
    if !has_checkable_inputs(whisper, tool_messages) {
        return None;
    }

    let mut sections: Vec<String> = Vec::new();
    if let Some(w) = whisper {
        let trimmed = js_trim(w);
        if !trimmed.is_empty() {
            sections.push(format!(
                "=== What the character recalled this turn (Commonplace Book) ===\n{trimmed}"
            ));
        }
    }
    for tm in tool_messages
        .iter()
        .filter(|tm| is_confirmation_read_tool(&tm.tool_name))
    {
        sections.push(format!(
            "=== Lookup result ===\n{}",
            serialize_tool_result(tm)
        ));
    }

    let mut reference = sections.join("\n\n");
    if utf16_len(&reference) > REFERENCE_CHAR_BUDGET {
        // Truncate oldest-first: drop from the front until within budget, keeping
        // the most-recent lookups (usually the ones the reply leans on).
        let start = utf16_len(&reference) - REFERENCE_CHAR_BUDGET;
        let tail = utf16_slice_from(&reference, start);
        reference = format!("[…earlier reference material truncated…]\n{tail}");
    }
    Some(reference)
}

/// The resolved outcome the finalizer applies to the persisted message (v4
/// `AnswerConfirmationOutcome`). `confirmed === undefined` never occurs here — the
/// finalizer only calls [`run_answer_confirmation`] when the feature is active and
/// there are checkable inputs.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AnswerConfirmationOutcome {
    pub confirmed: Option<bool>,
    pub revised: bool,
    pub notes: Option<String>,
    /// Present only when `revised` — the replacement reply text.
    pub revised_content: Option<String>,
}

/// The character's own connection-profile fields the re-affirmation pass reads
/// verbatim (v4 `RunAnswerConfirmationOptions.connectionProfile` subset).
#[derive(Clone, Debug, Default)]
pub struct ReaffirmationProfile {
    pub id: String,
    pub provider: String,
    pub model_name: String,
    pub base_url: Option<String>,
    /// The profile's provider parameters (open JSON) — forwarded via the JS
    /// `parameters && typeof === 'object'` guard.
    pub parameters: Option<Value>,
}

/// Everything [`run_answer_confirmation`] needs (v4 `RunAnswerConfirmationOptions`
/// minus the ids used only for `logLLMCall`, which is host-side).
pub struct RunAnswerConfirmationOptions<'a> {
    pub reply: &'a str,
    pub reference: &'a str,
    pub user_id: &'a str,
    pub chat_id: &'a str,
    pub message_id: Option<&'a str>,
    pub character_id: Option<&'a str>,
    /// Already-resolved cheap-LLM selection (compression.cheapLLMSelection).
    /// `None` → the whole block is skipped → `confirmed: null`.
    pub cheap_llm_selection: Option<&'a CheapLlmSelection>,
    /// The character's own model, used verbatim for the re-affirmation pass.
    pub connection_profile: &'a ReaffirmationProfile,
    pub is_dangerous_chat: bool,
    /// The uncensored-fallback options for the consistency check's cheap selection
    /// (v4 `uncensoredFallback`).
    pub uncensored_fallback: Option<&'a UncensoredFallbackOptions<'a>>,
}

/// Extract a JSON object from a possibly fenced/wrapped LLM response (v4
/// `extractJson`). Fence-strip via `/```(?:json)?\s*([\s\S]*?)```/i`, then the
/// first `{...}` span, then a strict `JSON.parse`. **Distinct** from
/// [`crate::memory_tasks::strip_code_fences`] (that one only strips a leading /
/// trailing fence; this matches a fence anywhere, non-greedy, case-insensitive).
/// Returns the parse error on failure (v4 `JSON.parse` throws).
fn extract_json(content: &str) -> Result<Value, ()> {
    let trimmed = js_trim(content);
    // Strip ```json / ``` fences if present, matching anywhere (case-insensitive),
    // non-greedy to the FIRST closing fence — reproduce JS's regex without a
    // backtracking engine over the bounded shape v4 uses.
    let body = extract_fenced_body(trimmed).unwrap_or_else(|| trimmed.to_string());
    let body = js_trim(&body);

    // Find the first {...} span if there's leading/trailing prose. v4 uses
    // `indexOf('{')` / `lastIndexOf('}')` on UTF-16 units; the byte-index find is
    // equivalent here because we then slice a contiguous ASCII-delimited span and
    // hand the bytes to a JSON parser that agrees with V8 on the object subset.
    let start = body.find('{');
    let end = body.rfind('}');
    let json_text: &str = match (start, end) {
        (Some(s), Some(e)) if e > s => &body[s..=e],
        _ => body,
    };
    serde_json::from_str(json_text).map_err(|_| ())
}

/// Reproduce v4's `trimmed.match(/```(?:json)?\s*([\s\S]*?)```/i)` capture group:
/// find the first ``` fence (optionally followed by `json`), skip the `\s*` run,
/// then the shortest content up to the next ```. Case-insensitive on the `json`
/// literal. Returns `None` when no complete fence pair is present (v4's `null`
/// match → the whole `trimmed` is used).
fn extract_fenced_body(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let mut i = 0;
    while i + 3 <= bytes.len() {
        if &bytes[i..i + 3] == b"```" {
            let mut j = i + 3;
            // Optional case-insensitive `json` literal.
            if j + 4 <= bytes.len() && s[j..j + 4].eq_ignore_ascii_case("json") {
                j += 4;
            }
            // `\s*` — the JS whitespace set (matches the `regex`/JS `\s`). Skip a
            // run of ASCII + common Unicode whitespace via `char::is_whitespace`
            // over the remaining slice (the bounded corpus uses ASCII whitespace).
            let after_ws = skip_leading_ws(&s[j..]);
            let content_start = s.len() - after_ws.len();
            // Non-greedy content up to the next ```.
            if let Some(rel) = s[content_start..].find("```") {
                return Some(s[content_start..content_start + rel].to_string());
            }
            // Fence opened but never closed — no match at this position; advance.
        }
        i += 1;
    }
    None
}

fn skip_leading_ws(s: &str) -> &str {
    s.trim_start_matches(|c: char| c.is_whitespace())
}

/// v4 `parseConsistencyVerdict` — requires a boolean `consistent`; `discrepancies`
/// defaults to `""` when absent / not a string. Returns `(consistent,
/// discrepancies)`. Errors when the boolean is missing (v4 throws).
fn parse_consistency_verdict(content: &str) -> Result<(bool, String), ()> {
    let obj = extract_json(content)?;
    let consistent = obj.get("consistent").and_then(Value::as_bool).ok_or(())?;
    let discrepancies = obj
        .get("discrepancies")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    Ok((consistent, discrepancies))
}

/// v4 `parseReaffirmationVerdict` — requires a boolean `revise`; `reply` is
/// `Some` only when a string. Returns `(revise, reply)`. Errors when the boolean
/// is missing (v4 throws).
fn parse_reaffirmation_verdict(content: &str) -> Result<(bool, Option<String>), ()> {
    let obj = extract_json(content)?;
    let revise = obj.get("revise").and_then(Value::as_bool).ok_or(())?;
    let reply = obj.get("reply").and_then(Value::as_str).map(str::to_string);
    Ok((revise, reply))
}

/// The re-affirmation status-frame callback (v4 `onAffirming`). Fired just before
/// the re-affirmation pass, only when a rewrite is invited. `Sync` so a
/// `&dyn OnAffirming` can be captured by the finalizer's `Send` future.
pub trait OnAffirming: Sync {
    fn affirming(&self);
}

impl<F: Fn() + Sync> OnAffirming for F {
    fn affirming(&self) {
        self()
    }
}

/// Build the re-affirmation user message (v4's `reaffUser` `[...].join('\n')`).
/// Factored out so the differential + self-tests reconstruct it byte-identically.
fn build_reaffirmation_user(reply: &str, discrepancies: &str, reference: &str) -> String {
    [
        "You drafted this reply this turn:",
        "\"\"\"",
        reply,
        "\"\"\"",
        "",
        "A consistency check flagged these apparent conflicts with what you know or looked up this turn:",
        discrepancies,
        "",
        "For reference, here is what you actually knew and looked up this turn:",
        reference,
        "",
        "If, on reflection, you stand by your reply exactly as written, respond with strict JSON {\"revise\": false}. If you want to correct it, respond with {\"revise\": true, \"reply\": \"<your corrected reply>\"} — the corrected reply replaces what you send, so write it in full and in your own voice.",
    ]
    .join("\n")
}

/// Run the consistency check and, if needed, the re-affirmation pass (v4
/// `runAnswerConfirmation`). **Never throws** — any failure resolves to
/// `confirmed: null`. Generic over the [`CompletionProvider`] boundary; the
/// executor holds the session no-custom-temperature cache.
pub async fn run_answer_confirmation<C: CompletionProvider>(
    completion: &C,
    executor: &CheapLlmTaskExecutor,
    opts: &RunAnswerConfirmationOptions<'_>,
    on_affirming: &dyn OnAffirming,
) -> AnswerConfirmationOutcome {
    let Some(cheap_selection) = opts.cheap_llm_selection else {
        // No cheap-LLM selection available → cannot verify.
        return AnswerConfirmationOutcome {
            confirmed: None,
            revised: false,
            notes: None,
            revised_content: None,
        };
    };

    // Upgrade to the uncensored cheap profile iff the Concierge flagged this chat.
    let (danger_settings, available_profiles): (
        Option<&DangerousContentSettings>,
        &[CheapLlmProfile],
    ) = match opts.uncensored_fallback {
        Some(uf) => (Some(uf.danger_settings), uf.available_profiles),
        None => (None, &[]),
    };
    let selection = resolve_uncensored_cheap_llm_selection(
        cheap_selection.clone(),
        opts.is_dangerous_chat,
        danger_settings,
        available_profiles,
    );

    let check_messages = vec![
        CompletionMessage::system(prompt_text::CONSISTENCY_SYSTEM_PROMPT),
        CompletionMessage::user(format!(
            "--- REFERENCE INFORMATION ---\n{}\n\n--- REPLY TO CHECK ---\n{}",
            opts.reference, opts.reply
        )),
    ];

    // v4 wraps in `withTimeout(25s)`; the timer is host-side (see module docs).
    let check = executor
        .execute(
            completion,
            &selection,
            check_messages,
            parse_consistency_verdict,
            opts.uncensored_fallback,
            Some(512.0),
            opts.character_id,
        )
        .await;

    let verdict = match (check.success, check.result) {
        // Provider ok AND parser ok.
        (true, Some(Ok((consistent, discrepancies)))) => Some((consistent, discrepancies)),
        // Provider ok but parse failed (v4: `extractJson`/`parseConsistencyVerdict`
        // throw → the outer try/catch → `confirmed: null`).
        (true, Some(Err(()))) | (true, None) => None,
        // Provider failed (v4 `!check.success || !check.result`).
        (false, _) => None,
    };

    let Some((consistent, discrepancies)) = verdict else {
        return AnswerConfirmationOutcome {
            confirmed: None,
            revised: false,
            notes: None,
            revised_content: None,
        };
    };

    if consistent {
        return AnswerConfirmationOutcome {
            confirmed: Some(true),
            revised: false,
            notes: None,
            revised_content: None,
        };
    }

    let discrepancies = if discrepancies.is_empty() {
        "The reply appears inconsistent with the reference information.".to_string()
    } else {
        discrepancies
    };
    on_affirming.affirming();

    // Re-affirmation: the character's OWN model (NOT escalated), given the
    // discrepancies, chooses to stand by the reply or rewrite it. Reuse the
    // cheap-task harness with a selection built from the character's connection
    // profile.
    let reaff_selection = CheapLlmSelection {
        provider: opts.connection_profile.provider.clone(),
        model_name: opts.connection_profile.model_name.clone(),
        base_url: opts
            .connection_profile
            .base_url
            .clone()
            .filter(|s| !s.is_empty()),
        connection_profile_id: Some(opts.connection_profile.id.clone()),
        // v4 hardcodes `isLocal: false` on this path.
        is_local: false,
        profile_parameters: match &opts.connection_profile.parameters {
            Some(v @ (Value::Object(_) | Value::Array(_))) => Some(v.clone()),
            _ => None,
        },
    };

    let reaff_messages = vec![
        CompletionMessage::system(prompt_text::REAFFIRMATION_SYSTEM_PROMPT),
        CompletionMessage::user(build_reaffirmation_user(
            opts.reply,
            &discrepancies,
            opts.reference,
        )),
    ];

    // v4 wraps in `withTimeout(60s)` with NO uncensored fallback.
    let reaff = executor
        .execute(
            completion,
            &reaff_selection,
            reaff_messages,
            parse_reaffirmation_verdict,
            None,
            Some(4096.0),
            opts.character_id,
        )
        .await;

    let reaff_verdict = match (reaff.success, reaff.result) {
        (true, Some(Ok((revise, reply)))) => Some((revise, reply)),
        (true, Some(Err(()))) | (true, None) => None,
        (false, _) => None,
    };

    let Some((revise, reply)) = reaff_verdict else {
        // Errored / parse-failed → unverified, but keep the discrepancies note.
        return AnswerConfirmationOutcome {
            confirmed: None,
            revised: false,
            notes: Some(discrepancies),
            revised_content: None,
        };
    };

    if revise {
        if let Some(reply_text) = reply.as_ref().filter(|r| !js_trim(r).is_empty()) {
            // Revised with usable text.
            return AnswerConfirmationOutcome {
                confirmed: Some(true),
                revised: true,
                notes: Some(discrepancies),
                revised_content: Some(reply_text.clone()),
            };
        }
        // Wanted to revise but gave no usable text — don't gamble on a broken
        // rewrite.
        return AnswerConfirmationOutcome {
            confirmed: None,
            revised: false,
            notes: Some(discrepancies),
            revised_content: None,
        };
    }

    // Stood by the flagged reply.
    AnswerConfirmationOutcome {
        confirmed: Some(false),
        revised: false,
        notes: Some(discrepancies),
        revised_content: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn tool(name: &str, success: bool, content: &str, args: Option<Value>) -> ToolMessage {
        ToolMessage {
            tool_name: name.to_string(),
            success,
            content: content.to_string(),
            arguments: args,
            ..Default::default()
        }
    }

    #[test]
    fn is_active_three_level_gate() {
        // Chat OFF beats project ON and global true.
        assert!(!is_answer_confirmation_active(
            Some("OFF"),
            Some("ON"),
            true
        ));
        assert!(is_answer_confirmation_active(
            Some("ON"),
            Some("OFF"),
            false
        ));
        // Project ON with global false.
        assert!(is_answer_confirmation_active(None, Some("ON"), false));
        assert!(!is_answer_confirmation_active(None, Some("OFF"), true));
        // Global fallback.
        assert!(is_answer_confirmation_active(None, None, true));
        assert!(!is_answer_confirmation_active(None, None, false));
    }

    #[test]
    fn checkable_inputs_whisper_or_in_scope_tool() {
        assert!(has_checkable_inputs(Some("recalled"), &[]));
        assert!(!has_checkable_inputs(Some("   "), &[]));
        assert!(!has_checkable_inputs(None, &[]));
        assert!(has_checkable_inputs(
            None,
            &[tool("search", true, "r", None)]
        ));
        // Out-of-scope tool alone → not checkable.
        assert!(!has_checkable_inputs(
            None,
            &[tool("doc_list_files", true, "r", None)]
        ));
    }

    #[test]
    fn commonplace_whisper_backward_scan() {
        let msgs = vec![
            json!({"type":"message","systemSender":"commonplaceBook","targetParticipantIds":["p1"],"content":"older"}),
            json!({"type":"message","systemSender":"host","targetParticipantIds":["p1"],"content":"not a whisper"}),
            json!({"type":"message","systemSender":"commonplaceBook","targetParticipantIds":["p2"],"content":"other target"}),
            json!({"type":"message","systemSender":"commonplaceBook","targetParticipantIds":["p1"],"content":"newest"}),
        ];
        assert_eq!(
            find_latest_commonplace_whisper(&msgs, "p1").as_deref(),
            Some("newest")
        );
        assert_eq!(find_latest_commonplace_whisper(&msgs, "p3"), None);
        // Blank content is skipped.
        let blank = vec![
            json!({"type":"message","systemSender":"commonplaceBook","targetParticipantIds":["p1"],"content":"   "}),
        ];
        assert_eq!(find_latest_commonplace_whisper(&blank, "p1"), None);
    }

    #[test]
    fn user_driven_turn_impersonation_and_controlled_by() {
        let participants = vec![
            json!({"id":"p1","controlledBy":"llm"}),
            json!({"id":"p2","controlledBy":"user"}),
        ];
        assert!(is_user_driven_turn(
            &["p1".to_string()],
            &participants,
            "p1"
        ));
        assert!(is_user_driven_turn(&[], &participants, "p2"));
        assert!(!is_user_driven_turn(&[], &participants, "p1"));
    }

    #[test]
    fn gather_inputs_shape_and_ordering() {
        let tools = vec![
            tool("search", true, "hit1", Some(json!({"query":"x"}))),
            tool("doc_list_files", true, "ignored", None), // out of scope
            tool("read_conversation", false, "hit2", Some(json!({}))),
        ];
        let r = gather_confirmation_inputs(Some("recalled fact"), &tools).unwrap();
        let expected = concat!(
            "=== What the character recalled this turn (Commonplace Book) ===\nrecalled fact",
            "\n\n=== Lookup result ===\n[Tool: search]\nArguments: {\"query\":\"x\"}\nResult: hit1",
            "\n\n=== Lookup result ===\n[Tool: read_conversation (failed)]\nResult: hit2",
        );
        assert_eq!(r, expected);
        // Nothing checkable → None.
        assert_eq!(gather_confirmation_inputs(None, &[]), None);
    }

    #[test]
    fn gather_inputs_truncates_oldest_first() {
        // Build a reference well over the budget via a single big whisper.
        let big = "A".repeat(REFERENCE_CHAR_BUDGET + 500);
        let r = gather_confirmation_inputs(Some(&big), &[]).unwrap();
        assert!(r.starts_with("[…earlier reference material truncated…]\n"));
        // The tail is exactly REFERENCE_CHAR_BUDGET code units after the prefix.
        let prefix = "[…earlier reference material truncated…]\n";
        let tail = r.strip_prefix(prefix).unwrap();
        assert_eq!(utf16_len(tail), REFERENCE_CHAR_BUDGET);
    }

    #[test]
    fn extract_json_fenced_and_bare() {
        assert_eq!(
            extract_json("```json\n{\"consistent\":true,\"discrepancies\":\"\"}\n```")
                .unwrap()
                .get("consistent")
                .and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            extract_json("prose {\"revise\":false} trailing")
                .unwrap()
                .get("revise")
                .and_then(Value::as_bool),
            Some(false)
        );
        assert!(extract_json("not json at all").is_err());
    }

    #[test]
    fn parse_verdicts() {
        assert_eq!(
            parse_consistency_verdict("{\"consistent\":false,\"discrepancies\":\"bad\"}").unwrap(),
            (false, "bad".to_string())
        );
        // discrepancies default.
        assert_eq!(
            parse_consistency_verdict("{\"consistent\":true}").unwrap(),
            (true, String::new())
        );
        // missing boolean → error.
        assert!(parse_consistency_verdict("{\"discrepancies\":\"x\"}").is_err());
        assert_eq!(
            parse_reaffirmation_verdict("{\"revise\":true,\"reply\":\"fixed\"}").unwrap(),
            (true, Some("fixed".to_string()))
        );
        assert!(parse_reaffirmation_verdict("{\"reply\":\"x\"}").is_err());
    }

    // --- run_answer_confirmation orchestration (async, canned model) ------------

    use crate::model::completion::CannedCompletionProvider;

    const REPLY: &str = "The tower is 300 meters tall.";
    const REFERENCE: &str = "=== Lookup result ===\n[Tool: search]\nResult: 324m";

    fn cheap_selection() -> CheapLlmSelection {
        CheapLlmSelection {
            provider: "ANTHROPIC".into(),
            model_name: "cheap".into(),
            base_url: None,
            connection_profile_id: Some("cur".into()),
            is_local: false,
            profile_parameters: None,
        }
    }

    fn own_profile() -> ReaffirmationProfile {
        ReaffirmationProfile {
            id: "own".into(),
            provider: "ANTHROPIC".into(),
            model_name: "big".into(),
            base_url: None,
            parameters: None,
        }
    }

    fn check_messages() -> Vec<CompletionMessage> {
        vec![
            CompletionMessage::system(prompt_text::CONSISTENCY_SYSTEM_PROMPT),
            CompletionMessage::user(format!(
                "--- REFERENCE INFORMATION ---\n{REFERENCE}\n\n--- REPLY TO CHECK ---\n{REPLY}"
            )),
        ]
    }

    fn reaff_messages(discrepancies: &str) -> Vec<CompletionMessage> {
        vec![
            CompletionMessage::system(prompt_text::REAFFIRMATION_SYSTEM_PROMPT),
            CompletionMessage::user(build_reaffirmation_user(REPLY, discrepancies, REFERENCE)),
        ]
    }

    async fn run<C: CompletionProvider>(completion: &C) -> AnswerConfirmationOutcome {
        let executor = CheapLlmTaskExecutor::new();
        let sel = cheap_selection();
        let prof = own_profile();
        let opts = RunAnswerConfirmationOptions {
            reply: REPLY,
            reference: REFERENCE,
            user_id: "u",
            chat_id: "c",
            message_id: Some("m"),
            character_id: Some("char"),
            cheap_llm_selection: Some(&sel),
            connection_profile: &prof,
            is_dangerous_chat: false,
            uncensored_fallback: None,
        };
        let noop = || {};
        run_answer_confirmation(completion, &executor, &opts, &noop).await
    }

    #[tokio::test]
    async fn consistent_verdict_confirms() {
        let provider = CannedCompletionProvider::new().with_response(
            "ANTHROPIC",
            "cheap",
            Some(0.3),
            &check_messages(),
            "{\"consistent\":true,\"discrepancies\":\"\"}",
            None,
        );
        let out = run(&provider).await;
        assert_eq!(
            out,
            AnswerConfirmationOutcome {
                confirmed: Some(true),
                revised: false,
                notes: None,
                revised_content: None,
            }
        );
    }

    #[tokio::test]
    async fn inconsistent_then_stands_by_is_not_confirmed() {
        let mut provider = CannedCompletionProvider::new().with_response(
            "ANTHROPIC",
            "cheap",
            Some(0.3),
            &check_messages(),
            "{\"consistent\":false,\"discrepancies\":\"reply says 300m, lookup says 324m\"}",
            None,
        );
        // Re-affirmation runs on the character's OWN model (provider/model of
        // `own_profile`). Register a stands-by verdict for the exact reaff messages.
        provider = provider.with_response(
            "ANTHROPIC",
            "big",
            Some(0.3),
            &reaff_messages("reply says 300m, lookup says 324m"),
            "{\"revise\":false}",
            None,
        );
        let out = run(&provider).await;
        assert_eq!(out.confirmed, Some(false));
        assert!(!out.revised);
        assert_eq!(
            out.notes.as_deref(),
            Some("reply says 300m, lookup says 324m")
        );
    }

    #[tokio::test]
    async fn inconsistent_then_revises_confirms_with_content() {
        let mut provider = CannedCompletionProvider::new().with_response(
            "ANTHROPIC",
            "cheap",
            Some(0.3),
            &check_messages(),
            "{\"consistent\":false,\"discrepancies\":\"height\"}",
            None,
        );
        provider = provider.with_response(
            "ANTHROPIC",
            "big",
            Some(0.3),
            &reaff_messages("height"),
            "{\"revise\":true,\"reply\":\"The tower is 324 meters tall.\"}",
            None,
        );
        let out = run(&provider).await;
        assert_eq!(out.confirmed, Some(true));
        assert!(out.revised);
        assert_eq!(
            out.revised_content.as_deref(),
            Some("The tower is 324 meters tall.")
        );
        assert_eq!(out.notes.as_deref(), Some("height"));
    }

    #[tokio::test]
    async fn inconsistent_revise_without_text_is_unverified() {
        let mut provider = CannedCompletionProvider::new().with_response(
            "ANTHROPIC",
            "cheap",
            Some(0.3),
            &check_messages(),
            "{\"consistent\":false,\"discrepancies\":\"height\"}",
            None,
        );
        provider = provider.with_response(
            "ANTHROPIC",
            "big",
            Some(0.3),
            &reaff_messages("height"),
            "{\"revise\":true}",
            None,
        );
        let out = run(&provider).await;
        assert_eq!(out.confirmed, None);
        assert!(!out.revised);
        assert_eq!(out.notes.as_deref(), Some("height"));
    }

    #[tokio::test]
    async fn check_parse_failure_is_unverified() {
        let provider = CannedCompletionProvider::new().with_response(
            "ANTHROPIC",
            "cheap",
            Some(0.3),
            &check_messages(),
            "not json at all",
            None,
        );
        let out = run(&provider).await;
        assert_eq!(out.confirmed, None);
        assert!(out.notes.is_none());
    }

    #[tokio::test]
    async fn no_cheap_selection_is_unverified() {
        let provider = CannedCompletionProvider::new();
        let executor = CheapLlmTaskExecutor::new();
        let prof = own_profile();
        let opts = RunAnswerConfirmationOptions {
            reply: REPLY,
            reference: REFERENCE,
            user_id: "u",
            chat_id: "c",
            message_id: None,
            character_id: None,
            cheap_llm_selection: None,
            connection_profile: &prof,
            is_dangerous_chat: false,
            uncensored_fallback: None,
        };
        let noop = || {};
        let out = run_answer_confirmation(&provider, &executor, &opts, &noop).await;
        assert_eq!(out.confirmed, None);
    }

    #[tokio::test]
    async fn affirming_callback_fires_only_on_inconsistency() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let fired = AtomicUsize::new(0);
        // Consistent → callback NOT fired.
        {
            let provider = CannedCompletionProvider::new().with_response(
                "ANTHROPIC",
                "cheap",
                Some(0.3),
                &check_messages(),
                "{\"consistent\":true,\"discrepancies\":\"\"}",
                None,
            );
            let executor = CheapLlmTaskExecutor::new();
            let sel = cheap_selection();
            let prof = own_profile();
            let opts = RunAnswerConfirmationOptions {
                reply: REPLY,
                reference: REFERENCE,
                user_id: "u",
                chat_id: "c",
                message_id: None,
                character_id: None,
                cheap_llm_selection: Some(&sel),
                connection_profile: &prof,
                is_dangerous_chat: false,
                uncensored_fallback: None,
            };
            let cb = || {
                fired.fetch_add(1, Ordering::SeqCst);
            };
            run_answer_confirmation(&provider, &executor, &opts, &cb).await;
        }
        assert_eq!(fired.load(Ordering::SeqCst), 0);
    }
}
