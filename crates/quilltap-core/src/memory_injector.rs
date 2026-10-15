//! Port of v4's lib/chat/context/scene-state.ts — the two pure context
//! formatters that survived v4 Phase E's recall rewrite: the current scene state
//! (with the Commonplace Book's per-character emission cache) and the
//! conversation summary. v4 moved both verbatim out of the deleted
//! `memory-injector.ts`; every memory formatter that lived beside them is now
//! [`crate::services::recall::render`] (R-L14 — no file rename here, so the
//! scene-state callers keep their paths). The memory-row predicates
//! [`memory_is_cold`] / [`memory_is_digest`] stay for their other callers.
//!
//! Every string is byte-stable against the v4 output: whitespace, separators,
//! and the token-budget cut boundaries (via [`crate::token_estimation`]).
//!
//! Token counts follow the no-provider path: v4's `estimateTokens(text)` with an
//! absent provider uses `CHARS_PER_TOKEN.default` (3.5) directly, so this module
//! calls [`crate::token_estimation::estimate_tokens`] with
//! [`crate::token_estimation::DEFAULT_CHARS_PER_TOKEN`].

use crate::token_estimation::{
    estimate_tokens as est_tokens_rate, truncate_to_token_limit, DEFAULT_CHARS_PER_TOKEN,
};
use sha2::{Digest, Sha256};

/// v4 `estimateTokens(text)` with no provider → default chars-per-token.
fn estimate_tokens(text: &str) -> i64 {
    est_tokens_rate(text, DEFAULT_CHARS_PER_TOKEN)
}

/// JS `String.prototype.trim()` — strips Unicode whitespace (the JS `\s` set +
/// the BOM/line/paragraph separators) from both ends. For the corpus we only
/// need the common ASCII + a few Unicode spaces; [`str::trim`] on the Rust side
/// strips `char::is_whitespace`, which matches JS `trim` on the whitespace this
/// module actually sees (ASCII spaces/tabs/newlines and NBSP-class spaces).
fn js_trim(s: &str) -> &str {
    s.trim()
}

const SCENE_HASH_LENGTH: usize = 16;

/// SHA-256 hex, first 16 chars — v4 `sceneHash`.
fn scene_hash(input: &str) -> String {
    let digest = Sha256::digest(input.as_bytes());
    let hex = hex::encode(digest);
    hex[..SCENE_HASH_LENGTH].to_string()
}

/// v4's service-side cold test, `m.tier === 'cold'` (the JS `m.tier !==
/// 'cold'` hot reading, §R.4(b)): NULL / absent / `"hot"` are all hot.
pub fn memory_is_cold(v: &serde_json::Value) -> bool {
    v.get("tier").and_then(serde_json::Value::as_str) == Some("cold")
}

/// v4 `m.source === 'CONSOLIDATED'` — a consolidated digest.
pub fn memory_is_digest(v: &serde_json::Value) -> bool {
    v.get("source").and_then(serde_json::Value::as_str) == Some("CONSOLIDATED")
}

// ---------------------------------------------------------------------------
// formatCurrentSceneState
// ---------------------------------------------------------------------------

/// One scene-state character (the subset `formatCurrentSceneState` reads).
#[derive(Clone)]
pub struct SceneStateCharacter {
    pub character_id: String,
    pub character_name: String,
    pub action: String,
    /// v4's `clothing` is `string | null`; `None` = null.
    pub clothing: Option<String>,
}

/// The scene state (the subset the formatter reads).
#[derive(Clone)]
pub struct SceneState {
    pub location: String,
    pub characters: Vec<SceneStateCharacter>,
}

/// Per-target emission record for a single character's slice of the scene-state
/// whisper (v4 `SceneStateEmissionEntry`).
#[derive(Clone, Debug, PartialEq)]
pub struct SceneStateEmissionEntry {
    pub action_hash: String,
    pub clothing_hash: String,
    pub emitted_at: String,
}

/// Result of [`format_current_scene_state`].
pub struct SceneStateResult {
    pub content: String,
    pub token_count: i64,
    /// Preserves emission order (v4 returns a `Map`; iteration order = insertion
    /// order). A `Vec` of `(characterId, entry)` keeps that order for the
    /// differential.
    pub emitted_by_character: Vec<(String, SceneStateEmissionEntry)>,
}

/// Render the latest scene-state snapshot as the `## Current State` section.
/// Returns empty content when `scene_state` is `None`.
///
/// `time` is the chat's announced timestamp; `None`/blank drops the Time line.
/// `now_iso` is v4's `new Date().toISOString()` — injected here for determinism
/// (v4 computes it inline). `live_clothing_by_character_id` overrides each
/// character's clothing; `prior_emission_by_character` is the per-target cache
/// that collapses an unchanged section to `### Name — _unchanged_`.
pub fn format_current_scene_state(
    scene_state: Option<&SceneState>,
    time: Option<&str>,
    now_iso: &str,
    live_clothing_by_character_id: &dyn Fn(&str) -> Option<String>,
    prior_emission_by_character: &dyn Fn(&str) -> Option<SceneStateEmissionEntry>,
) -> SceneStateResult {
    let scene_state = match scene_state {
        Some(s) => s,
        None => {
            return SceneStateResult {
                content: String::new(),
                token_count: 0,
                emitted_by_character: Vec::new(),
            };
        }
    };

    let names: Vec<&str> = scene_state
        .characters
        .iter()
        .map(|c| c.character_name.as_str())
        .filter(|n| !n.is_empty())
        .collect();

    let mut lines: Vec<String> = vec!["## Current State".to_string(), String::new()];
    lines.push(format!(
        "- **Location**: {}",
        if scene_state.location.is_empty() {
            "Unknown"
        } else {
            &scene_state.location
        }
    ));
    lines.push(format!("- **Characters Present**: {}", names.join(", ")));
    if let Some(t) = time {
        if !js_trim(t).is_empty() {
            lines.push(format!("- **Time**: {}", js_trim(t)));
        }
    }
    lines.push("- **Active Now**: true".to_string());

    let mut emitted_by_character: Vec<(String, SceneStateEmissionEntry)> = Vec::new();

    for c in &scene_state.characters {
        if c.character_name.is_empty() {
            continue;
        }

        let action_text = {
            let t = js_trim(&c.action);
            if t.is_empty() {
                "_unspecified_".to_string()
            } else {
                t.to_string()
            }
        };
        // liveClothing || (c.clothing ?? '').trim() || '_unspecified_'
        let live_clothing = live_clothing_by_character_id(&c.character_id)
            .map(|s| js_trim(&s).to_string())
            .filter(|s| !s.is_empty());
        let clothing_text = live_clothing.unwrap_or_else(|| {
            let cached = c.clothing.as_deref().unwrap_or("");
            let t = js_trim(cached);
            if t.is_empty() {
                "_unspecified_".to_string()
            } else {
                t.to_string()
            }
        });
        let action_hash = scene_hash(&action_text);
        let clothing_hash = scene_hash(&clothing_text);

        let prior = if !c.character_id.is_empty() {
            prior_emission_by_character(&c.character_id)
        } else {
            None
        };
        let unchanged = match &prior {
            Some(p) => p.action_hash == action_hash && p.clothing_hash == clothing_hash,
            None => false,
        };

        lines.push(String::new());
        if unchanged {
            lines.push(format!("### {} — _unchanged_", c.character_name));
        } else {
            lines.push(format!("### {}", c.character_name));
            lines.push(String::new());
            lines.push("#### Action".to_string());
            lines.push(String::new());
            lines.push(action_text.clone());
            lines.push(String::new());
            lines.push("#### Clothing".to_string());
            lines.push(String::new());
            lines.push(clothing_text.clone());
        }

        if !c.character_id.is_empty() {
            let emitted_at = if unchanged {
                match &prior {
                    Some(p) => p.emitted_at.clone(),
                    None => now_iso.to_string(),
                }
            } else {
                now_iso.to_string()
            };
            emitted_by_character.push((
                c.character_id.clone(),
                SceneStateEmissionEntry {
                    action_hash,
                    clothing_hash,
                    emitted_at,
                },
            ));
        }
    }

    let content = lines.join("\n");
    let token_count = estimate_tokens(&format!("{content}\n"));
    SceneStateResult {
        content,
        token_count,
        emitted_by_character,
    }
}

// ---------------------------------------------------------------------------
// formatSummaryForContext
// ---------------------------------------------------------------------------

/// Format conversation summary for context. Returns `(content, token_count)`.
pub fn format_summary_for_context(summary: &str, max_tokens: i64) -> (String, i64) {
    if summary.is_empty() || js_trim(summary).is_empty() {
        return (String::new(), 0);
    }

    let header = "## Previous Conversation Summary";
    let full_content = format!("{header}\n{summary}");
    let full_tokens = estimate_tokens(&full_content);

    if full_tokens <= max_tokens {
        return (full_content, full_tokens);
    }

    let header_tokens = estimate_tokens(&format!("{header}\n"));
    let available_for_summary = max_tokens - header_tokens;
    let truncated_summary = truncate_to_token_limit(
        summary,
        available_for_summary,
        DEFAULT_CHARS_PER_TOKEN,
        "...",
    );

    let content = format!("{header}\n{truncated_summary}");
    let token_count = estimate_tokens(&content);
    (content, token_count)
}
