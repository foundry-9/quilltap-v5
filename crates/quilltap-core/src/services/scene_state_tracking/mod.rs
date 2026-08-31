//! Scene-state tracking (v4 `lib/memory/cheap-llm-tasks/image-scene-tasks.ts`
//! `updateSceneState` + `capClothingSummary`; the enclosing
//! `handleSceneStateTracking` job handler in
//! `lib/background-jobs/handlers/scene-state-tracking.ts`).
//!
//! Derives a structured scene snapshot (location, per-character action /
//! appearance / clothing) from the recent conversation via the cheap LLM. This
//! module ports the cheap-LLM TASK — the byte-exact prompt bodies, the input
//! marshaling, and the response parser (incl. the ≤200-char clothing cap). It is
//! the in-scope service function (W4.6a); its runner dispatch row is W4.8.
//!
//! The full `handleSceneStateTracking` job wrapper (danger pre-classification +
//! wardrobe baselines + the clothing-hash cache reconciliation + `createSystemEvent`
//! token tracking + the `chats.update({ sceneState })` persist) composes this task
//! over several already-ported subsystems (danger resolver/gatekeeper, cheap-LLM
//! selection, wardrobe resolve, the scene-state `ChatUpdate` setter added this
//! round); it lands with the W4.8 runner-dispatch wiring — see the module's
//! tracked deferral in the CHANGELOG.
//!
//! ## ⚠ DEFERRED with that wrapper (P4.D136, v4 `a1d88aa3a`, bug 107)
//!
//! v4 added `throwIfLostToTimeout(result, 'scene-state-tracking')` to the
//! handler's failure arm — the sixth of the six handlers that fail their job
//! rather than report a clean finish over a pass that never ran. It is v4's
//! headline example: *99 `SCENE_STATE_TRACKING` jobs came back COMPLETED over
//! 12 losses.* v5 has no scene-state handler to attach it to, so the guard
//! rides the deferred wrapper. The machinery it needs is already here and
//! proven — [`crate::services::cheap_llm_exec::throw_if_lost_to_timeout`] and
//! [`crate::services::cheap_llm_exec::CheapLlmTaskResult::timed_out`] — so
//! landing the wrapper is the only remaining work. The other five handlers took
//! their guards this round.

mod prompt_text;

use serde_json::Value;

use crate::cheap_llm::{CheapLlmSelection, UncensoredFallbackOptions};
use crate::memory_tasks::strip_code_fences;
use crate::model::completion::{CompletionMessage, CompletionProvider};
use crate::services::cheap_llm_exec::{
    CheapLlmTaskExecutor, CheapLlmTaskOptions, CheapLlmTaskResult,
};

use prompt_text::{SCENE_STATE_FIRST_TURN_PROMPT, SCENE_STATE_UPDATE_PROMPT};

/// v4 `SCENE_CLOTHING_MAX_CHARS`.
const SCENE_CLOTHING_MAX_CHARS: usize = 200;

/// A conversation message the scene-state task renders (v4 `ChatMessage`).
#[derive(Clone, Debug)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

/// One character baseline (v4 `SceneStateInput.characters[]`).
#[derive(Clone, Debug)]
pub struct CharacterBaseline {
    pub character_id: String,
    pub character_name: String,
    pub physical_description: String,
    pub clothing_description: String,
    pub scenario: Option<String>,
}

/// v4 `SceneStateInput`.
#[derive(Clone, Debug)]
pub struct SceneStateInput {
    /// Previous scene state JSON (`None` for the first turn).
    pub previous_scene_state: Option<Value>,
    pub characters: Vec<CharacterBaseline>,
    pub recent_messages: Vec<ChatMessage>,
    pub chat_scenario: Option<String>,
}

/// v4 `capClothingSummary`: coerce to a trimmed string, cap at
/// `SCENE_CLOTHING_MAX_CHARS` UTF-16 units (over-length → first 199 + `trimEnd` +
/// `…`). A non-string value → `""`.
pub fn cap_clothing_summary(value: Option<&Value>) -> String {
    let text = match value {
        Some(Value::String(s)) => crate::jsstr::js_trim(s).to_string(),
        _ => String::new(),
    };
    if crate::jsstr::utf16_len(&text) <= SCENE_CLOTHING_MAX_CHARS {
        return text;
    }
    let sliced = crate::jsstr::utf16_truncate(&text, SCENE_CLOTHING_MAX_CHARS - 1);
    format!("{}…", crate::jsstr::js_trim_end(&sliced))
}

/// v4 `updateSceneState`: format the recent messages + character baselines, call
/// the cheap LLM (first-turn vs update prompt), then parse the JSON result and cap
/// each character's clothing summary. Returns the parsed scene-state object.
pub async fn update_scene_state<C: CompletionProvider>(
    executor: &CheapLlmTaskExecutor,
    completion: &C,
    input: &SceneStateInput,
    selection: &CheapLlmSelection,
    uncensored_fallback: Option<&UncensoredFallbackOptions<'_>>,
) -> CheapLlmTaskResult<Value> {
    // Recent messages — no truncation (clothing details often deep in messages).
    let message_text = input
        .recent_messages
        .iter()
        .map(|m| {
            let speaker = match m.role.as_str() {
                "user" => "User",
                "assistant" => "Character",
                _ => "System",
            };
            format!("{speaker}: {}", m.content)
        })
        .collect::<Vec<_>>()
        .join("\n\n");

    // Character baseline section.
    let character_baselines = input
        .characters
        .iter()
        .map(|char| {
            let scenario = char
                .scenario
                .as_ref()
                .map(|s| format!("\n  - Scenario context: {s}"))
                .unwrap_or_default();
            format!(
                "{} (ID: {}):\n  - Appearance: {}\n  - Clothing: {}{scenario}",
                char.character_name,
                char.character_id,
                char.physical_description,
                char.clothing_description
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n");

    // Optional scenario section.
    let scenario_section = match &input.chat_scenario {
        Some(s) => format!("\nScenario / Opening Setup:\n{s}\n"),
        None => String::new(),
    };

    let messages: Vec<CompletionMessage> = if let Some(prev) = &input.previous_scene_state {
        // Subsequent turns: update with new messages + baselines for recovery.
        let prev_json = serde_json::to_string_pretty(prev).unwrap_or_else(|_| "null".to_string());
        vec![
            CompletionMessage::system(SCENE_STATE_UPDATE_PROMPT),
            CompletionMessage::user(format!(
                "Character Baselines (defaults only — use these to fill in gaps where the previous state has null or missing fields):\n{character_baselines}\n\nPrevious Scene State:\n{prev_json}\n\nNew Messages (the primary source of truth — these override previous state where applicable):\n{message_text}\n\nUpdate the scene state based on these new messages:"
            )),
        ]
    } else {
        // First turn: establish initial scene state.
        vec![
            CompletionMessage::system(SCENE_STATE_FIRST_TURN_PROMPT),
            CompletionMessage::user(format!(
                "Character Baselines (defaults only — the conversation and scenario override these):\n{character_baselines}\n{scenario_section}\nConversation (the primary source of truth):\n{message_text}\n\nBased on the scenario and conversation above, what is the current scene state?"
            )),
        ]
    };

    executor
        .execute(
            completion,
            selection,
            messages,
            parse_scene_state_response,
            uncensored_fallback,
            None,
            None,
            Some("scene-state-tracking"),
            CheapLlmTaskOptions::default(),
        )
        .await
}

/// v4's `parseResponse` for `updateSceneState`: reject an empty / near-empty
/// response (<10 chars, a likely content refusal — surfaced as an error so the
/// task result is `{ success: false }`), strip code fences, parse JSON, then cap
/// each character's clothing.
fn parse_scene_state_response(content: &str) -> Value {
    let raw = crate::jsstr::js_trim(content);
    // v4 THROWS on an empty/near-empty (<10-char) response or a JSON parse error,
    // which the executor wraps into `{ success: false }`. The ported executor's
    // parse closure returns `T` and cannot throw (the recap/distill precedent), so
    // those failure paths yield `Value::Null` here — and the (deferred) job wrapper
    // treats a `Null` result as v4's `!result.result` failure. A valid parse gets
    // v4's per-character clothing cap.
    if raw.is_empty() || crate::jsstr::utf16_len(raw) < 10 {
        return Value::Null;
    }
    let clean = strip_code_fences(content);
    let mut parsed: Value = match serde_json::from_str(&clean) {
        Ok(v) => v,
        Err(_) => return Value::Null,
    };
    // Cap each character's clothing summary.
    if let Some(chars) = parsed.get_mut("characters").and_then(Value::as_array_mut) {
        for ch in chars.iter_mut() {
            if let Some(obj) = ch.as_object_mut() {
                if obj.contains_key("clothing") {
                    let capped = cap_clothing_summary(obj.get("clothing"));
                    obj.insert("clothing".to_string(), Value::String(capped));
                }
            }
        }
    }
    parsed
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn cap_clothing_summary_variants() {
        assert_eq!(cap_clothing_summary(Some(&json!("  a shirt  "))), "a shirt");
        assert_eq!(cap_clothing_summary(Some(&json!(null))), "");
        assert_eq!(cap_clothing_summary(None), "");
        assert_eq!(cap_clothing_summary(Some(&json!(42))), "");
        let long = "x".repeat(250);
        let out = cap_clothing_summary(Some(&json!(long)));
        assert_eq!(out, format!("{}…", "x".repeat(199)));
    }

    #[test]
    fn parse_rejects_short_and_caps_clothing() {
        assert_eq!(parse_scene_state_response("   "), Value::Null);
        assert_eq!(parse_scene_state_response("short"), Value::Null);
        let long_clothing = "y".repeat(250);
        let resp = format!(
            "{{\"location\":\"a room\",\"characters\":[{{\"characterId\":\"c1\",\"clothing\":\"{long_clothing}\"}}]}}"
        );
        let parsed = parse_scene_state_response(&resp);
        let clothing = parsed["characters"][0]["clothing"].as_str().unwrap();
        assert_eq!(clothing, format!("{}…", "y".repeat(199)));
    }

    #[test]
    fn parse_strips_code_fences() {
        let parsed =
            parse_scene_state_response("```json\n{\"location\":\"here\",\"characters\":[]}\n```");
        assert_eq!(parsed["location"], "here");
    }
}
