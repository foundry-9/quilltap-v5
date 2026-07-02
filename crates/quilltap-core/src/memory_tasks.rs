//! The pure leaves of v4's memory-extraction cheap-LLM tasks
//! (`lib/memory/cheap-llm-tasks/memory-tasks.ts`): the extraction prompt
//! builders (SELF / OTHER, with the orienting-context footer and the
//! autonomous-room / first-person-user preambles), the shared turn-context
//! renderer, and the candidate response parsers (JSON parsing, targeting-tag
//! validation, candidate shaping, the hard cap). Everything here is
//! deterministic string/JSON shaping — the LLM call itself sits behind
//! [`crate::model::completion::CompletionProvider`] and is orchestrated by the
//! memory-processor service.
//!
//! Also hosts [`strip_code_fences`], which v4 keeps in
//! `lib/services/ai-import.service.ts` — it is the single fence-stripper every
//! cheap-LLM JSON parser shares; move it out when a second Rust consumer lands.
//!
//! The big prompt bodies live in the generated [`prompt_text`] submodule
//! (extracted mechanically from the v4 source, never hand-transcribed).

mod prompt_text;

use serde::Serialize;
use serde_json::Value;

use crate::jsstr::{js_trim, js_trim_end, js_trim_start, utf16_len, utf16_truncate};
use crate::memory_format::{format_name_with_pronouns, Pronouns};
use crate::model::completion::CompletionMessage;
use crate::recall_tags::{ContextTag, ScopeTag, TemporalTag};

/// Hard ceiling on candidates returned from a single extraction call, regardless
/// of the cheap-LLM profile's output-token budget. Applied both in the prompt
/// (so the model is told the cap) and after parsing (as defense-in-depth when
/// the model ignores the instruction).
///
/// SELF: per-call cap (one observer, one subject — themselves).
/// OTHER: per-subject cap inside a single multi-subject call; the call's
///        total cap is `HARD_CANDIDATE_CAP × number-of-subjects`.
pub const HARD_CANDIDATE_CAP: usize = 2;

/// Resolves the per-call maxMemories from the token budget, clamped to the hard
/// cap (v4 `resolveMaxMemories`: `ceil((resolvedMaxTokens ?? 8000) / 8000)`
/// clamped to `[1, HARD_CANDIDATE_CAP]`).
pub fn resolve_max_memories(resolved_max_tokens: Option<f64>) -> i64 {
    let budget_derived = (resolved_max_tokens.unwrap_or(8000.0) / 8000.0).ceil() as i64;
    (HARD_CANDIDATE_CAP as i64).min(budget_derived.max(1))
}

/// Candidate memory extracted from a conversation (v4 `MemoryCandidate` after
/// `coerceMemoryCandidate` — content/summary may be absent, keywords and
/// importance are always materialized). `importance` is kept as the raw JSON
/// number so an integer-valued model emission (`1`) re-serializes as `1`, not
/// `1.0`, matching `JSON.stringify`.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct MemoryCandidate {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    pub keywords: Vec<String>,
    pub importance: serde_json::Number,
}

impl MemoryCandidate {
    /// The importance as an `f64` (the arithmetic consumers' view).
    pub fn importance_f64(&self) -> f64 {
        self.importance.as_f64().unwrap_or(0.5)
    }
}

/// Non-canonical orienting context fed into the extraction footer (v4
/// `OrientingContext`). Background only — never a source of memories; held in
/// the variable footer so cheap-LLM prefix caching still hits.
#[derive(Clone, Debug, Default)]
pub struct OrientingContext {
    pub project_description: Option<String>,
    pub chat_context_summary: Option<String>,
}

/// Char budget per orienting line — UTF-16 code units, not tokens (no tokenizer
/// on the cheap path).
const ORIENTING_TRUNCATE: usize = 1500;

fn truncate_for_orienting(value: &str) -> String {
    if utf16_len(value) <= ORIENTING_TRUNCATE {
        value.to_string()
    } else {
        format!("{}…", utf16_truncate(value, ORIENTING_TRUNCATE))
    }
}

/// Render the ORIENTING CONTEXT footer block (v4 `renderOrientingContext`).
/// Each line appears only when its source is non-empty; the whole block is
/// omitted when both are empty (so the prompt stays byte-identical to the
/// no-context form).
pub fn render_orienting_context(orienting: Option<&OrientingContext>) -> String {
    let Some(orienting) = orienting else {
        return String::new();
    };
    let mut lines: Vec<String> = Vec::new();
    if let Some(project) = orienting.project_description.as_deref().map(js_trim) {
        if !project.is_empty() {
            lines.push(format!("PROJECT: {}", truncate_for_orienting(project)));
        }
    }
    if let Some(summary) = orienting.chat_context_summary.as_deref().map(js_trim) {
        if !summary.is_empty() {
            lines.push(format!("STORY SO FAR: {}", truncate_for_orienting(summary)));
        }
    }
    if lines.is_empty() {
        return String::new();
    }
    format!(
        "ORIENTING CONTEXT — background only, never a source of memories\n{}\n\n",
        lines.join("\n")
    )
}

/// 4.6 Private Character Rooms — user-absence clause prepended to the SELF and
/// OTHER prompts when the source chat is autonomous, so resulting memories
/// don't falsely imply the user was present, agreed to, or was informed.
const AUTONOMOUS_ROOM_USER_ABSENCE_CLAUSE: &str =
    "IMPORTANT: This exchange occurred in an autonomous character-to-character \
room with no user present. Do not produce memories that imply the user \
witnessed, agreed to, or was informed of any part of this exchange. \
Memories about the participants' shared experience are allowed; memories \
that name or address the user are not.\n\n";

/// Prepended to the SELF prompt when the subject is a character a human is
/// driving directly — binds first-person pronouns in the subject's own lines to
/// the subject. A *prepended* preamble (like the autonomous clause) so the
/// cached body prefix stays byte-stable for the common AI path.
const FIRST_PERSON_USER_CLAUSE: &str =
    "IMPORTANT: The SUBJECT below is a character a human is playing directly, so \
the SUBJECT's lines in the transcript may be written in the first person. \
Read every \"I\", \"me\", \"my\", and \"myself\" in the SUBJECT's own lines as \
referring to the SUBJECT — attribute those decisions, realizations, and \
actions to the SUBJECT, not to anyone else in the exchange.\n\n";

fn self_body_for_cap(max_memories: i64) -> String {
    format!(
        "{}{max_memories}{}",
        prompt_text::SELF_BODY_BEFORE_CAP,
        prompt_text::SELF_BODY_AFTER_CAP
    )
}

fn other_body_for_cap(per_subject_cap: i64) -> String {
    format!(
        "{}{per_subject_cap}{}",
        prompt_text::OTHER_BODY_BEFORE_CAP,
        prompt_text::OTHER_BODY_AFTER_CAP
    )
}

/// v4 `getSelfMemoryExtractionPrompt`: the SELF extraction system prompt — the
/// byte-stable body plus the CONTEXT footer naming the subject and carrying the
/// canon block.
pub fn get_self_memory_extraction_prompt(
    max_memories: i64,
    observer_name: &str,
    canon_block: &str,
    in_autonomous_room: bool,
    orienting: Option<&OrientingContext>,
    is_user_controlled: bool,
) -> String {
    let mut preamble = String::new();
    if is_user_controlled {
        preamble.push_str(FIRST_PERSON_USER_CLAUSE);
    }
    if in_autonomous_room {
        preamble.push_str(AUTONOMOUS_ROOM_USER_ABSENCE_CLAUSE);
    }
    let orienting_block = render_orienting_context(orienting);
    format!(
        "{preamble}{}\n\n{orienting_block}CONTEXT\nSUBJECT: {observer_name}\n\n{canon_block}",
        self_body_for_cap(max_memories)
    )
}

/// One subject of a multi-subject OTHER extraction call (v4 `OtherSubjectInput`).
#[derive(Clone, Debug)]
pub struct OtherSubjectInput {
    /// Stable identifier the caller will use to route returned candidates.
    pub id: String,
    pub name: String,
    pub pronouns: Option<Pronouns>,
    /// True for the user-controlled character; false for AI characters.
    pub is_user: bool,
    /// Pre-rendered "ALREADY ESTABLISHED about <name>" block for this subject.
    pub canon_block: String,
}

/// v4 `getOtherMemoryExtractionPrompt`: the OTHER extraction system prompt —
/// the byte-stable body plus the CONTEXT footer naming the observer and every
/// numbered subject with its canon block.
pub fn get_other_memory_extraction_prompt(
    per_subject_cap: i64,
    observer_name: &str,
    subjects: &[OtherSubjectInput],
    in_autonomous_room: bool,
    orienting: Option<&OrientingContext>,
) -> String {
    let subjects_block = subjects
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let label = format_name_with_pronouns(&s.name, s.pronouns.as_ref());
            let user_tag = if s.is_user {
                " (the user-controlled character)"
            } else {
                ""
            };
            format!("SUBJECT {}: {label}{user_tag}\n{}", i + 1, s.canon_block)
        })
        .collect::<Vec<_>>()
        .join("\n\n");

    let preamble = if in_autonomous_room {
        AUTONOMOUS_ROOM_USER_ABSENCE_CLAUSE
    } else {
        ""
    };
    let orienting_block = render_orienting_context(orienting);
    format!(
        "{preamble}{}\n\n{orienting_block}CONTEXT\nOBSERVER: {observer_name}\n\n{subjects_block}",
        other_body_for_cap(per_subject_cap)
    )
}

/// One character's joined contribution to a turn (v4 `TurnCharacterSlice`).
/// `is_user_controlled` is `?? false` at every v4 use site, so the optional
/// boolean collapses to a plain `bool`.
#[derive(Clone, Debug, Default)]
pub struct TurnCharacterSlice {
    pub character_id: String,
    pub character_name: String,
    pub character_pronouns: Option<Pronouns>,
    /// Joined text from every assistant message this character contributed
    /// during the turn, in chronological order.
    pub text: String,
    /// Every assistant message ID that contributed to this slice.
    pub contributing_message_ids: Vec<String>,
    /// True when this slice's text was authored by the human driving a
    /// user-controlled character.
    pub is_user_controlled: bool,
}

/// The joined per-turn transcript (v4 `TurnTranscript`). `user_message` is
/// `string | null` in v4 — `None` here means null (an empty string is a real,
/// present opener).
#[derive(Clone, Debug, Default)]
pub struct TurnTranscript {
    /// ID of the USER message that opened the turn, or `None` for
    /// greeting/continue turns with no fresh user input.
    pub turn_opener_message_id: Option<String>,
    /// Verbatim user-message text, or `None` when the turn has no user opener.
    pub user_message: Option<String>,
    pub user_character_id: Option<String>,
    pub user_character_name: Option<String>,
    pub user_character_pronouns: Option<Pronouns>,
    /// ASSISTANT participants that spoke during the turn, ordered by first
    /// contribution.
    pub character_slices: Vec<TurnCharacterSlice>,
    /// The most recent ASSISTANT message ID in the turn — used as
    /// sourceMessageId on derived memories.
    pub latest_assistant_message_id: Option<String>,
}

/// Render the participant roster + joined transcript for inclusion in
/// extraction prompts (v4 `renderTurnContext`). Single shared formatter so the
/// SELF and OTHER passes see byte-identical input prefixes.
pub fn render_turn_context(transcript: &TurnTranscript) -> String {
    // A user-controlled character arrives as a slice (built from the turn
    // opener). It is the human participant *and* a memory-forming character, so
    // it is listed on the USER line and its lines render once in the body
    // labeled "(the user-controlled character)" — never under the AI-character
    // roster and never duplicated as a standalone opener.
    let ai_slices: Vec<&TurnCharacterSlice> = transcript
        .character_slices
        .iter()
        .filter(|s| !s.is_user_controlled)
        .collect();
    let user_slices: Vec<&TurnCharacterSlice> = transcript
        .character_slices
        .iter()
        .filter(|s| s.is_user_controlled)
        .collect();
    let has_user_slice = !user_slices.is_empty();

    let mut roster: Vec<String> = vec!["PARTICIPANTS IN THIS TURN:".to_string()];
    let user_display_name = transcript
        .user_character_name
        .as_deref()
        .or_else(|| user_slices.first().map(|s| s.character_name.as_str()));
    if let Some(name) = user_display_name {
        roster.push(format!("- USER: {name} (the human participant)"));
    } else if transcript.user_message.is_some() {
        roster.push("- USER: The human participant".to_string());
    }

    if ai_slices.len() == 1 {
        let slice = ai_slices[0];
        roster.push(format!(
            "- CHARACTER: {} (an AI character)",
            format_name_with_pronouns(&slice.character_name, slice.character_pronouns.as_ref())
        ));
    } else if ai_slices.len() > 1 {
        roster.push("- CHARACTERS (AI characters in this chat):".to_string());
        for slice in &ai_slices {
            roster.push(format!(
                "  * {}",
                format_name_with_pronouns(&slice.character_name, slice.character_pronouns.as_ref())
            ));
        }
    }

    let mut transcript_sections: Vec<String> = Vec::new();
    // The standalone opener renders only when no user slice carries it — i.e. a
    // plain human with no character. When a user slice exists, its body line
    // below is the single rendering of that text.
    if let Some(user_message) = transcript.user_message.as_deref() {
        if !has_user_slice {
            let user_label = match transcript.user_character_name.as_deref() {
                Some(name) => format!("{name} (the user)"),
                None => "The user".to_string(),
            };
            transcript_sections.push(format!("{user_label} says:\n\"{user_message}\""));
        }
    }
    for slice in &transcript.character_slices {
        let role = if slice.is_user_controlled {
            "the user-controlled character"
        } else {
            "the character"
        };
        let character_label = format!(
            "{} ({role})",
            format_name_with_pronouns(&slice.character_name, slice.character_pronouns.as_ref())
        );
        transcript_sections.push(format!("{character_label} says:\n\"{}\"", slice.text));
    }

    format!(
        "{}\n\nTURN TRANSCRIPT:\n\n{}",
        roster.join("\n"),
        transcript_sections.join("\n\n")
    )
}

/// Build the SELF extraction call's messages (the pure half of v4
/// `extractSelfMemoriesFromTurn`, up to the `executeCheapLLMTask` call).
/// Returns `None` when the target character contributed no slice this turn —
/// v4's early `{ success: true, result: [] }` return, no call made.
pub fn build_self_extraction_messages(
    transcript: &TurnTranscript,
    target_character_id: &str,
    canon_block: &str,
    resolved_max_tokens: Option<f64>,
    in_autonomous_room: bool,
    orienting: Option<&OrientingContext>,
) -> Option<Vec<CompletionMessage>> {
    let target = transcript
        .character_slices
        .iter()
        .find(|s| s.character_id == target_character_id)?;

    let max_memories = resolve_max_memories(resolved_max_tokens);
    let target_label =
        format_name_with_pronouns(&target.character_name, target.character_pronouns.as_ref());

    Some(vec![
        CompletionMessage::system(get_self_memory_extraction_prompt(
            max_memories,
            &target_label,
            canon_block,
            in_autonomous_room,
            orienting,
            target.is_user_controlled,
        )),
        CompletionMessage::user(render_turn_context(transcript)),
    ])
}

/// Build the OTHER extraction call's messages (the pure half of v4
/// `extractOtherMemoriesFromTurn`). Returns `None` when the observer
/// contributed no slice this turn or there are no subjects — v4's early
/// success-with-empty-map return, no call made.
pub fn build_other_extraction_messages(
    transcript: &TurnTranscript,
    observer_character_id: &str,
    subjects: &[OtherSubjectInput],
    resolved_max_tokens: Option<f64>,
    in_autonomous_room: bool,
    orienting: Option<&OrientingContext>,
) -> Option<Vec<CompletionMessage>> {
    let observer = transcript
        .character_slices
        .iter()
        .find(|s| s.character_id == observer_character_id)?;
    if subjects.is_empty() {
        return None;
    }

    let per_subject_cap = resolve_max_memories(resolved_max_tokens);
    let observer_label = format_name_with_pronouns(
        &observer.character_name,
        observer.character_pronouns.as_ref(),
    );

    Some(vec![
        CompletionMessage::system(get_other_memory_extraction_prompt(
            per_subject_cap,
            &observer_label,
            subjects,
            in_autonomous_room,
            orienting,
        )),
        CompletionMessage::user(render_turn_context(transcript)),
    ])
}

/// Strip markdown code fences from LLM output before JSON parsing (v4
/// `stripCodeFences`, `lib/services/ai-import.service.ts` — the single source
/// for every cheap-LLM JSON parser). `^```json\s*` / `^```\s*` then
/// `\s*```$`, with JS `trim` / `\s` semantics.
pub fn strip_code_fences(text: &str) -> String {
    fn strip_trailing_fence(s: &str) -> &str {
        // `/\s*```$/` — removes the terminal fence plus the whitespace run
        // before it, but only when the string actually ends with a fence.
        match s.strip_suffix("```") {
            Some(rest) => js_trim_end(rest),
            None => s,
        }
    }

    let cleaned = js_trim(text);
    let cleaned = if let Some(rest) = cleaned.strip_prefix("```json") {
        strip_trailing_fence(js_trim_start(rest))
    } else if let Some(rest) = cleaned.strip_prefix("```") {
        strip_trailing_fence(js_trim_start(rest))
    } else {
        cleaned
    };
    js_trim(cleaned).to_string()
}

/// Validate the three model-emitted targeting tags against their closed
/// vocabularies, default any invalid/missing value (present / wide /
/// information), and materialize them into the keyword array: `temporal` and
/// `context` as bare words, `scope` as `scope: <value>` (v4
/// `applyTargetingTags`). The tags never persist as top-level memory fields.
fn apply_targeting_tags(free_keywords: Vec<String>, item: &Value) -> Vec<String> {
    fn normalized(item: &Value, key: &str) -> String {
        match item.get(key) {
            Some(Value::String(s)) => js_trim(s).to_lowercase(),
            _ => String::new(),
        }
    }

    let temporal = TemporalTag::from_kw(&normalized(item, "temporal"))
        .unwrap_or(TemporalTag::Present)
        .as_str();
    let scope = ScopeTag::from_kw(&normalized(item, "scope"))
        .unwrap_or(ScopeTag::Wide)
        .as_str();
    let context = ContextTag::from_kw(&normalized(item, "context"))
        .unwrap_or(ContextTag::Information)
        .as_str();

    let mut keywords = free_keywords;
    keywords.push(temporal.to_string());
    keywords.push(format!("scope: {scope}"));
    keywords.push(context.to_string());
    keywords
}

/// Whether a JSON value is truthy under JS coercion (the `item.content ?`
/// check in `coerceMemoryCandidate`). `undefined` never reaches here — an
/// absent key is `None` at the call site.
fn is_truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().map(|f| f != 0.0).unwrap_or(true),
        Value::String(s) => !s.is_empty(),
        // Arrays and objects are always truthy in JS (even empty ones).
        Value::Array(_) | Value::Object(_) => true,
    }
}

/// Coerce a raw parsed item into a [`MemoryCandidate`], or `None` when the row
/// is empty (no content and no summary) — v4 `coerceMemoryCandidate`.
///
/// `apply_tags` — when true the three targeting-tag axes are validated and
/// merged into the keyword array via [`apply_targeting_tags`]; when false the
/// raw keyword array is used as-is (batch extraction path).
///
/// v4 dereferences `item.keywords` etc. directly, so a `null` item **throws**
/// (`Err` here — the array parser's catch turns that into `[]`); any other
/// non-object item property-accesses to `undefined` and coerces to an empty
/// candidate (dropped).
fn coerce_memory_candidate(item: &Value, apply_tags: bool) -> Result<Option<MemoryCandidate>, ()> {
    if item.is_null() {
        // JS: `null.keywords` → TypeError.
        return Err(());
    }

    let free_keywords: Vec<String> = match item.get("keywords") {
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(|k| k.as_str().map(str::to_string))
            .collect(),
        _ => Vec::new(),
    };

    // `typeof x === 'string' ? x : (x ? JSON.stringify(x) : undefined)` —
    // non-string truthy values are stringified, falsy ones drop.
    fn coerce_text(v: Option<&Value>) -> Option<String> {
        match v {
            Some(Value::String(s)) => Some(s.clone()),
            Some(other) if is_truthy(other) => {
                Some(serde_json::to_string(other).expect("JSON value re-serializes"))
            }
            _ => None,
        }
    }

    let content = coerce_text(item.get("content"));
    let summary = coerce_text(item.get("summary"));
    let keywords = if apply_tags {
        apply_targeting_tags(free_keywords, item)
    } else {
        free_keywords
    };
    let importance = match item.get("importance") {
        Some(Value::Number(n)) => n.clone(),
        _ => serde_json::Number::from_f64(0.5).expect("0.5 is finite"),
    };

    let content_empty = content.as_deref().map(str::is_empty).unwrap_or(true);
    let summary_empty = summary.as_deref().map(str::is_empty).unwrap_or(true);
    if content_empty && summary_empty {
        return Ok(None);
    }

    Ok(Some(MemoryCandidate {
        content,
        summary,
        keywords,
        importance,
    }))
}

/// Shared parser for SELF memory extraction responses (v4
/// `parseMemoryCandidateArray`): strip fences, parse, coerce each item, drop
/// empties, cap at [`HARD_CANDIDATE_CAP`]. Any parse error — including the
/// TypeError a `null` item raises inside the map — yields `[]` (v4's
/// whole-function try/catch).
pub fn parse_memory_candidate_array(content: &str) -> Vec<MemoryCandidate> {
    let clean_content = strip_code_fences(content);
    let Ok(parsed) = serde_json::from_str::<Value>(&clean_content) else {
        return Vec::new();
    };
    let items: Vec<Value> = match parsed {
        Value::Array(items) => items,
        other => vec![other],
    };

    let mut out: Vec<MemoryCandidate> = Vec::new();
    for item in &items {
        match coerce_memory_candidate(item, true) {
            Err(()) => return Vec::new(), // the thrown TypeError → catch → []
            Ok(Some(candidate)) => out.push(candidate),
            Ok(None) => {}
        }
    }
    out.truncate(HARD_CANDIDATE_CAP);
    out
}

/// Multi-subject parser (v4 `parseOtherCandidatesBySubject`): routes each item
/// back to its subject by 1-based `subjectIndex`. Items with a
/// missing/invalid/out-of-range subjectIndex are dropped, items beyond the
/// per-subject cap are dropped, items beyond the total cap
/// (`per_subject_cap × subjects.len()`) are dropped. Returns the buckets in
/// subject order (v4's `Map` insertion order); every subject appears, empty or
/// not.
pub fn parse_other_candidates_by_subject(
    content: &str,
    subjects: &[OtherSubjectInput],
    per_subject_cap: i64,
) -> Vec<(String, Vec<MemoryCandidate>)> {
    let mut result: Vec<(String, Vec<MemoryCandidate>)> = subjects
        .iter()
        .map(|s| (s.id.clone(), Vec::new()))
        .collect();

    if subjects.is_empty() {
        return result;
    }
    let total_cap = per_subject_cap * subjects.len() as i64;

    let clean_content = strip_code_fences(content);
    let Ok(parsed) = serde_json::from_str::<Value>(&clean_content) else {
        return result;
    };
    let items: Vec<Value> = match parsed {
        Value::Array(items) => items,
        other => vec![other],
    };

    let mut total: i64 = 0;
    for raw in &items {
        if total >= total_cap {
            break;
        }
        // `!raw || typeof raw !== 'object'` — null and primitives skip; JS
        // arrays are typeof 'object' so they pass (and then drop at the index
        // check, their `subjectIndex` being undefined).
        if !matches!(raw, Value::Object(_) | Value::Array(_)) {
            continue;
        }

        // `typeof item.subjectIndex === 'number' ? … : NaN`, then
        // `Number.isInteger(idx)` — a JSON `2.0` IS an integer to JS.
        let idx = match raw.get("subjectIndex") {
            Some(Value::Number(n)) => match n.as_f64() {
                Some(f) if f.fract() == 0.0 && f.is_finite() => f as i64,
                _ => continue,
            },
            _ => continue,
        };
        if idx < 1 || idx > subjects.len() as i64 {
            continue;
        }
        let bucket = &mut result[(idx - 1) as usize].1;
        if bucket.len() as i64 >= per_subject_cap {
            continue;
        }

        // Object items can't be null here, so the coerce can't throw.
        let Ok(Some(candidate)) = coerce_memory_candidate(raw, true) else {
            continue;
        };

        bucket.push(candidate);
        total += 1;
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_max_memories_bands() {
        assert_eq!(resolve_max_memories(None), 1);
        assert_eq!(resolve_max_memories(Some(8000.0)), 1);
        assert_eq!(resolve_max_memories(Some(8001.0)), 2);
        assert_eq!(resolve_max_memories(Some(64000.0)), 2); // hard cap
        assert_eq!(resolve_max_memories(Some(100.0)), 1);
        assert_eq!(resolve_max_memories(Some(0.0)), 1); // max(1, 0)
    }

    #[test]
    fn strip_code_fences_variants() {
        assert_eq!(strip_code_fences("```json\n[1]\n```"), "[1]");
        assert_eq!(strip_code_fences("``` [1] ```"), "[1]");
        assert_eq!(strip_code_fences("  [1]  "), "[1]");
        assert_eq!(strip_code_fences("```json[1]"), "[1]"); // no closing fence
        assert_eq!(strip_code_fences("``````"), "");
    }

    #[test]
    fn null_item_poisons_the_whole_self_array() {
        // v4: the TypeError from `null.keywords` is caught by the outer
        // try/catch, discarding even the valid items.
        let out = parse_memory_candidate_array(
            r#"[{"content":"a real fact","summary":"real","importance":0.5}, null]"#,
        );
        assert!(out.is_empty());
    }

    #[test]
    fn other_parser_routes_by_subject_index() {
        let subjects = vec![
            OtherSubjectInput {
                id: "s1".into(),
                name: "Amy".into(),
                pronouns: None,
                is_user: false,
                canon_block: String::new(),
            },
            OtherSubjectInput {
                id: "s2".into(),
                name: "Charlie".into(),
                pronouns: None,
                is_user: true,
                canon_block: String::new(),
            },
        ];
        let out = parse_other_candidates_by_subject(
            r#"[{"subjectIndex":2,"content":"c","importance":1},
                {"subjectIndex":1.0,"content":"a","importance":0.5},
                {"subjectIndex":"1","content":"dropped"},
                null,
                {"subjectIndex":3,"content":"out of range"}]"#,
            &subjects,
            2,
        );
        assert_eq!(out[0].0, "s1");
        assert_eq!(out[0].1.len(), 1); // the 1.0 float-integer routes
        assert_eq!(out[1].1.len(), 1);
        // Integer-valued importance survives as the bare JSON number.
        assert_eq!(serde_json::to_value(&out[1].1[0]).unwrap()["importance"], 1);
    }
}
