//! Query distillation (v4 `extractMemorySearchKeywords`, `memory-tasks.ts`) — the
//! cheap-LLM task that turns the recent conversation window into a natural-language
//! paraphrase + keyword bag + a turn-level temporal/context guess, used to drive
//! the dynamic-head memory search.
//!
//! Closes the `BuildContextSeams::distill_memory_search` seam (W4.6a).
//! build_context consumes `paraphrase` (preferred) then `keywords`; `temporal` /
//! `context` are parsed for fidelity (the search-leg recall re-rank that consumes
//! them is a tracked memory-service deferral). The prompt body is the byte-exact
//! `MEMORY_KEYWORD_EXTRACTION_PROMPT` in the generated `prompt_text` submodule.

use serde_json::Value;

use crate::cheap_llm::CheapLlmSelection;
use crate::memory_tasks::strip_code_fences;
use crate::model::completion::{CompletionMessage, CompletionProvider};
use crate::recall_tags::{ContextTag, TemporalTag};
use crate::services::cheap_llm_exec::CheapLlmTaskExecutor;
use crate::services::cheap_llm_exec::CheapLlmTaskOptions;

use super::prompt_text::MEMORY_KEYWORD_EXTRACTION_PROMPT;

/// v4 `ExtractionClock` (`extraction/prompts/shared.ts`) — the subset the
/// SEARCH extraction consumes: the TODAY line resolves relative time phrases
/// ("last week") into absolute dates. The full type (with `narrative_now`, for
/// the creation prompts' CLOCK block) lives at
/// [`crate::memory_tasks::ExtractionClock`].
#[derive(Clone, Debug)]
pub struct ExtractionClock {
    /// ISO wall-clock timestamp of the source turn (the turn's message time).
    pub now_iso: String,
    /// Which clock the chat's story runs on: `"realtime" | "narrative"`.
    pub timeline_mode: String,
    /// v4 `clock.timeZone` — the instance's MEMORY CALENDAR zone (bug 212:
    /// instance timezone → `QUILLTAP_TIMEZONE` → the host's own), the same
    /// calendar the write side stamps with. v4 reads `clock?.timeZone ?? await
    /// refreshMemoryTimeZone()`; every v5 production caller resolves it through
    /// [`crate::services::memory_time_zone::refresh_memory_time_zone`] and hands
    /// it in (core never reads the process zone). `None` reads `"UTC"` — reached
    /// only by unit tests.
    pub time_zone: Option<String>,
}

impl ExtractionClock {
    fn zone(&self) -> &str {
        self.time_zone.as_deref().unwrap_or("UTC")
    }
}

/// v4 `WEEKDAYS` (`recall/query.ts`) — indexed by the memory calendar's
/// weekday (0 = Sunday; bug 212's `zonedParts(nowMs, zone).weekday`).
const WEEKDAYS: [&str; 7] = [
    "Sunday",
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
];

/// v4 `MemorySearchExtraction.timeRange` — an absolute ISO window the turn
/// references, resolved by the model against the TODAY line. The same
/// structural `{from, to}` shape flows into the recall context's
/// `occurredWithin`, so the type is shared with `recall_tags`.
pub use crate::recall_tags::TimeWindow as DistillTimeRange;

/// v4 `MemorySearchExtraction` (the subset build_context consumes). `paraphrase`
/// wins over `keywords` as the embedding query. The episodic signals
/// (`retrospective` / `time_range` / `entities`) all default to inert values on
/// garbling so recall degrades to exactly today's behavior, never blocks.
#[derive(Clone, Debug, Default)]
pub struct DistilledSearch {
    pub keywords: Vec<String>,
    pub temporal: Option<TemporalTag>,
    pub context: Option<ContextTag>,
    pub paraphrase: Option<String>,
    /// True when the turn references past shared events (strict `=== true`).
    pub retrospective: bool,
    pub time_range: Option<DistillTimeRange>,
    pub entities: Vec<String>,
    /// True for v4's catch arm (`return { keywords: [] }`): the JSON shape then
    /// carries ONLY `keywords` — every other field is absent, not null/false.
    /// Consumers treat both arms identically; [`DistilledSearch::signals_json`]
    /// needs the distinction to reproduce v4's wire shape byte-for-byte.
    pub parse_failed: bool,
}

impl DistilledSearch {
    /// The v4 `MemorySearchExtraction` JSON shape with v4's field-presence
    /// semantics: `JSON.stringify` drops `undefined` (`temporal` / `context` /
    /// `paraphrase` when omitted) but keeps `retrospective: false`,
    /// `timeRange: null`, `entities: []`; the catch arm is `{"keywords":[]}`.
    pub fn signals_json(&self) -> Value {
        let mut obj = serde_json::Map::new();
        obj.insert(
            "keywords".to_string(),
            Value::Array(self.keywords.iter().cloned().map(Value::String).collect()),
        );
        if self.parse_failed {
            return Value::Object(obj);
        }
        if let Some(t) = self.temporal {
            obj.insert(
                "temporal".to_string(),
                Value::String(t.as_str().to_string()),
            );
        }
        if let Some(c) = self.context {
            obj.insert("context".to_string(), Value::String(c.as_str().to_string()));
        }
        if let Some(p) = &self.paraphrase {
            obj.insert("paraphrase".to_string(), Value::String(p.clone()));
        }
        obj.insert("retrospective".to_string(), Value::Bool(self.retrospective));
        obj.insert(
            "timeRange".to_string(),
            match &self.time_range {
                Some(tr) => {
                    let mut w = serde_json::Map::new();
                    w.insert("from".to_string(), Value::String(tr.from.clone()));
                    w.insert("to".to_string(), Value::String(tr.to.clone()));
                    Value::Object(w)
                }
                None => Value::Null,
            },
        );
        obj.insert(
            "entities".to_string(),
            Value::Array(self.entities.iter().cloned().map(Value::String).collect()),
        );
        Value::Object(obj)
    }
}

/// A conversation message for the distill window (already role-lowercased + the
/// assistant slice tool-stripped by the caller, matching v4's `recentForDistill`).
#[derive(Clone, Debug)]
pub struct DistillMessage {
    pub role: String,
    pub content: String,
}

/// How many of the most recent messages the deterministic day-reference scanner
/// reads. Deliberately far tighter than the 20 the prompt sees: an override that
/// fires on a stale "yesterday" from three topics ago is worse than no override.
const DAY_REFERENCE_SCAN_MESSAGES: usize = 4;

/// Resolve the turn's deterministic day reference exactly as v4's
/// `extractMemorySearchKeywords` does (`505dcb1f`; bug 212 moved it onto the
/// memory calendar, `resolveReferenceWindow(text, { timeZone, nowMs })`):
/// realtime timelines with a finite clock only, scanning the RAW content of the last
/// [`DAY_REFERENCE_SCAN_MESSAGES`] of the capped-20 window joined with a single
/// newline — a different rendering from the speaker-prefixed, 500-unit-truncated
/// text the prompt sees.
///
/// Public because the tier-1 differential drives the production pipeline
/// (resolve → parse → merge) rather than re-deriving it.
pub fn resolve_distill_day_reference(
    recent_messages: &[DistillMessage],
    clock: Option<&ExtractionClock>,
) -> Option<crate::memory_time::clock::DayReferenceResolution> {
    let now_iso = match clock {
        Some(c) => c.now_iso.clone(),
        None => crate::clock::now_iso(),
    };
    let timeline_mode = clock
        .map(|c| c.timeline_mode.as_str())
        .unwrap_or("realtime");
    if timeline_mode != "realtime" {
        return None;
    }
    let now_ms = crate::episodic::event_time_ms(Some(&now_iso))?;

    // v4: `cappedMessages.slice(-DAY_REFERENCE_SCAN_MESSAGES)` over the same
    // `recentMessages.slice(-20)` the prompt uses.
    let start = recent_messages.len().saturating_sub(20);
    let capped = &recent_messages[start..];
    let scan_start = capped.len().saturating_sub(DAY_REFERENCE_SCAN_MESSAGES);
    let text = capped[scan_start..]
        .iter()
        .map(|m| m.content.as_str())
        .collect::<Vec<_>>()
        .join("\n");

    crate::memory_time::clock::resolve_reference_window(
        &text,
        clock.map(ExtractionClock::zone).unwrap_or("UTC"),
        now_ms,
    )
}

/// Merge a deterministically-resolved day reference into the LLM's parsed
/// signals (v4 `mergeDayReference`, `505dcb1f` fix 1).
///
/// - Past-pointing reference → the resolved window OVERRIDES any LLM range and
///   forces `retrospective: true`. The override is the point: the model's own
///   ranges are UTC-day-biased, and that bias is what made a same-day "the
///   mission today" resolve to a window containing none of the mission.
/// - Future-only reference ("tomorrow") → nothing applied; whatever the model
///   said stands, since the lexicon covers less than the model does.
/// - No reference → the parsed result passes through unchanged.
///
/// Accepted cost (v4's own): a purely forward-looking "let's go to the pool
/// today" marks the turn retrospective. The consequences (a temporal flip toward
/// today's memories, one turn without the anti-repetition penalty) are benign,
/// and intent heuristics to dodge it would reintroduce exactly the brittleness
/// this resolver exists to remove.
///
/// v4 applies this INSIDE the parse closure's success arm only; its catch arm
/// returns a bare `{ keywords: [] }` because "callers bail on empty keywords, so
/// there is nothing for the deterministic window to ride along with". The
/// `parse_failed` guard is that arm.
pub fn merge_day_reference(
    parsed: DistilledSearch,
    day_reference: Option<&crate::memory_time::clock::DayReferenceResolution>,
) -> DistilledSearch {
    if parsed.parse_failed {
        return parsed;
    }
    let Some(day_reference) = day_reference else {
        return parsed;
    };
    if !day_reference.past_pointing {
        // v4 `logger.debug('[MemorySearchKeywords] Day reference is
        // future-pointing; leaving LLM signals alone')`. Log output is outside
        // the differential contract (P4.18); v5's line carries no chat/character
        // id because this function, unlike v4's, is not handed them.
        tracing::debug!(
            target: "quilltap::memory",
            matched = %day_reference.matched,
            "Day reference is future-pointing; leaving LLM signals alone",
        );
        return parsed;
    }
    tracing::debug!(
        target: "quilltap::memory",
        matched = %day_reference.matched,
        from = %day_reference.time_range.from,
        to = %day_reference.time_range.to,
        overrode_llm_time_range = parsed.time_range.is_some(),
        llm_retrospective = parsed.retrospective,
        "Deterministic day reference resolved",
    );
    DistilledSearch {
        retrospective: true,
        time_range: Some(day_reference.time_range.clone()),
        ..parsed
    }
}

/// Build the exact `LLMMessage[]` v4's `extractMemorySearchKeywords` sends:
/// the system prompt plus the user content with the conversation window (last
/// 20 messages, each capped at 500 UTF-16 units) and the TODAY line (variable —
/// user content, never the cached system prompt) so the model can resolve
/// "last week" into an absolute timeRange. Pure — the tier-1 differential
/// drives it directly.
pub fn build_distill_messages(
    recent_messages: &[DistillMessage],
    character_name: &str,
    clock: Option<&ExtractionClock>,
) -> Vec<CompletionMessage> {
    // v4: `recentMessages.slice(-20)`.
    let start = recent_messages.len().saturating_sub(20);
    let capped = &recent_messages[start..];
    let conversation_text = capped
        .iter()
        .map(|m| {
            let speaker = match m.role.as_str() {
                "user" => "User",
                "assistant" => "Character",
                _ => "System",
            };
            // v4: `content.length > 500 ? content.substring(0, 500) + '...' : content`
            // (`.length`/`.substring` are UTF-16 code-unit ops).
            let content = if crate::jsstr::utf16_len(&m.content) > 500 {
                format!("{}...", crate::jsstr::utf16_truncate(&m.content, 500))
            } else {
                m.content.clone()
            };
            format!("{speaker}: {content}")
        })
        .collect::<Vec<_>>()
        .join("\n\n");

    // v4: `clock?.nowIso ?? new Date().toISOString()` / `clock?.timelineMode ??
    // 'realtime'`. The non-finite arm keeps the raw string (no date/weekday).
    let now_iso = match clock {
        Some(c) => c.now_iso.clone(),
        None => crate::clock::now_iso(),
    };
    let timeline_mode = clock
        .map(|c| c.timeline_mode.as_str())
        .unwrap_or("realtime");
    // Rendered from the instance's MEMORY CALENDAR (bug 212 — instance
    // timezone → `QUILLTAP_TIMEZONE` → the host's own), never UTC: a UTC TODAY
    // line tells a user talking at 21:44 CDT that it is already tomorrow, and
    // every "today" the model resolves then lands on a day containing none of
    // the memories it means. The non-finite arm keeps the raw string unchanged.
    let zone = clock.map(ExtractionClock::zone).unwrap_or("UTC");
    let today_line = match crate::episodic::event_time_ms(Some(&now_iso)) {
        Some(ms) => {
            let date = crate::memory_time::calendar::zoned_date_stamp(ms, zone);
            let weekday =
                WEEKDAYS[crate::memory_time::calendar::zoned_parts(ms, zone).weekday as usize];
            format!("TODAY: {date} ({weekday}); timeline mode: {timeline_mode}")
        }
        None => format!("TODAY: {now_iso}; timeline mode: {timeline_mode}"),
    };

    let user_content = format!(
        "Character: {character_name}\n{today_line}\n\nRecent conversation:\n{conversation_text}\n\nExtract keywords for searching {character_name}'s memories:"
    );

    vec![
        CompletionMessage::system(MEMORY_KEYWORD_EXTRACTION_PROMPT),
        CompletionMessage::user(user_content),
    ]
}

/// v4 `extractMemorySearchKeywords`: build the conversation text (last 20 messages,
/// each capped at 500 chars), call the cheap LLM, and parse the response. Returns
/// `None` when the task fails (v4 `{ success: false }`) so the caller falls back to
/// the raw recent-window query.
///
/// `options` carries v4's trailing `latency: CheapLLMLatencyClass = 'background'`
/// (`02d4efa1b`, bug 115). One function serves two consumers with opposite answers
/// to "is anyone waiting?": the proactive pre-compute pass runs after delivery and
/// takes the background default, while the dynamic-head fallback in
/// [`crate::services::build_context`] is awaited inline with the composer empty and
/// must pass [`CheapLlmTaskOptions::interactive`]. Rust has no default arguments, so
/// every caller names its class — see [`crate::services::cheap_llm_exec::CheapLlmLatencyClass`].
#[allow(clippy::too_many_arguments)]
pub async fn distill_memory_search<C: CompletionProvider>(
    executor: &CheapLlmTaskExecutor,
    completion: &C,
    recent_messages: &[DistillMessage],
    character_name: &str,
    selection: &CheapLlmSelection,
    character_id: &str,
    clock: Option<&ExtractionClock>,
    options: CheapLlmTaskOptions,
) -> Option<DistilledSearch> {
    let messages = build_distill_messages(recent_messages, character_name, clock);
    let zone = clock.map(ExtractionClock::zone).unwrap_or("UTC");
    // Resolved BEFORE the call, exactly where v4 resolves it, and applied inside
    // the parse closure so the catch arm's bare `{ keywords: [] }` stays bare.
    let day_reference = resolve_distill_day_reference(recent_messages, clock);

    let result = executor
        .execute(
            completion,
            selection,
            messages,
            |content: &str| {
                merge_day_reference(parse_extraction(content, zone), day_reference.as_ref())
            },
            None,
            None,
            Some(character_id),
            Some("memory-keyword-extraction"),
            options,
        )
        .await;

    if !result.success {
        return None;
    }
    result.result
}

/// v4's `parseResponse` closure for `extractMemorySearchKeywords`. Accepts either
/// the object shape `{ keywords, temporal, context, paraphrase, retrospective,
/// timeRange, entities }` or a bare keyword array; a parse failure →
/// `{ keywords: [] }` (still a success value, with every other field ABSENT —
/// see [`DistilledSearch::parse_failed`]). A date-only `timeRange` bound widens
/// to the LOCAL day on the memory calendar `time_zone` (bug 212,
/// `normaliseTimeRange`), never a UTC day.
pub fn parse_extraction(content: &str, time_zone: &str) -> DistilledSearch {
    let clean = strip_code_fences(content);
    let parsed: Value = match serde_json::from_str(&clean) {
        Ok(v) => v,
        Err(_) => {
            return DistilledSearch {
                parse_failed: true,
                ..DistilledSearch::default()
            }
        }
    };

    // `Array.isArray(parsed) ? parsed : parsed?.keywords`.
    let raw_keywords: Option<&Vec<Value>> = if parsed.is_array() {
        parsed.as_array()
    } else {
        parsed.get("keywords").and_then(Value::as_array)
    };
    let keywords = raw_keywords
        .map(|arr| {
            arr.iter()
                .filter_map(|item| item.as_str())
                // `typeof item === 'string' && item.trim().length > 0`.
                .filter(|s| !crate::jsstr::js_trim(s).is_empty())
                .map(str::to_string)
                .take(10)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    // temporal / context only exist on the object shape.
    let obj = if parsed.is_array() {
        None
    } else {
        parsed.as_object()
    };
    let raw_temporal = obj
        .and_then(|o| o.get("temporal"))
        .and_then(Value::as_str)
        .map(|s| crate::jsstr::js_trim(s).to_lowercase())
        .unwrap_or_default();
    let raw_context = obj
        .and_then(|o| o.get("context"))
        .and_then(Value::as_str)
        .map(|s| crate::jsstr::js_trim(s).to_lowercase())
        .unwrap_or_default();
    // v4 validates against the closed vocabularies; an invalid value → undefined.
    let temporal = TemporalTag::from_kw(&raw_temporal);
    let context = ContextTag::from_kw(&raw_context);

    let raw_paraphrase = obj
        .and_then(|o| o.get("paraphrase"))
        .and_then(Value::as_str)
        .map(|s| crate::jsstr::js_trim(s).to_string())
        .unwrap_or_default();
    let paraphrase = if raw_paraphrase.is_empty() {
        None
    } else {
        Some(raw_paraphrase)
    };

    // Episodic signals — every one defaults to inert on garbling so recall
    // degrades to exactly today's behavior, never blocks.
    // v4: `!Array.isArray(parsed) && parsed?.retrospective === true` (strict).
    let retrospective = obj
        .and_then(|o| o.get("retrospective"))
        .and_then(Value::as_bool)
        == Some(true);

    // v4: `parsed?.timeRange && typeof parsed.timeRange === 'object'` (null is
    // falsy), then `normaliseTimeRange({ from, to }, { timeZone })` over the
    // string bounds (a non-string bound is `null`): both bounds trimmed and
    // `YYYY-MM-DD`-prefixed, a date-only bound widened to its LOCAL day on the
    // memory calendar (bug 212), `from <= to`, else no window.
    let time_range: Option<DistillTimeRange> = obj
        .and_then(|o| o.get("timeRange"))
        .filter(|t| t.is_object())
        .and_then(|tr| {
            crate::memory_time::clock::normalise_time_range(
                tr.get("from").and_then(Value::as_str),
                tr.get("to").and_then(Value::as_str),
                time_zone,
            )
        });

    // v4: filter (typeof string && trim().length > 0) → map(trim) → slice(0, 5).
    let entities = obj
        .and_then(|o| o.get("entities"))
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|e| e.as_str())
                .filter(|s| !crate::jsstr::js_trim(s).is_empty())
                .map(|s| crate::jsstr::js_trim(s).to_string())
                .take(5)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    DistilledSearch {
        keywords,
        temporal,
        context,
        paraphrase,
        retrospective,
        time_range,
        entities,
        parse_failed: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_object_shape() {
        let out = parse_extraction(
            r#"{"keywords":["a","b"," ","c"],"temporal":"Present","context":"relationships","paraphrase":"They talk."}"#,
            "UTC",
        );
        assert_eq!(out.keywords, vec!["a", "b", "c"]);
        assert_eq!(out.temporal, Some(TemporalTag::Present));
        assert_eq!(out.context, Some(ContextTag::Relationships));
        assert_eq!(out.paraphrase.as_deref(), Some("They talk."));
    }

    #[test]
    fn parse_bare_array_shape() {
        let out = parse_extraction(r#"["x","y"]"#, "UTC");
        assert_eq!(out.keywords, vec!["x", "y"]);
        assert_eq!(out.temporal, None);
        assert_eq!(out.context, None);
        assert_eq!(out.paraphrase, None);
    }

    #[test]
    fn parse_failure_yields_empty() {
        let out = parse_extraction("not json at all", "UTC");
        assert!(out.keywords.is_empty());
        assert!(out.paraphrase.is_none());
        assert!(out.parse_failed);
        // The catch arm's wire shape carries ONLY `keywords`.
        assert_eq!(out.signals_json().to_string(), r#"{"keywords":[]}"#);
    }

    #[test]
    fn parse_retrospective_strict_true_only() {
        let out = parse_extraction(r#"{"keywords":["k"],"retrospective":true}"#, "UTC");
        assert!(out.retrospective);
        let out = parse_extraction(r#"{"keywords":["k"],"retrospective":"true"}"#, "UTC");
        assert!(!out.retrospective);
        let out = parse_extraction(r#"{"keywords":["k"],"retrospective":1}"#, "UTC");
        assert!(!out.retrospective);
    }

    #[test]
    fn parse_time_range_date_only_normalizes_full_day() {
        let out = parse_extraction(
            r#"{"keywords":[],"timeRange":{"from":"2024-06-01","to":"2024-06-07"}}"#,
            "UTC",
        );
        let tr = out.time_range.expect("window");
        assert_eq!(tr.from, "2024-06-01T00:00:00.000Z");
        assert_eq!(tr.to, "2024-06-07T23:59:59.999Z");
    }

    #[test]
    fn parse_time_range_rejects_bad_shapes() {
        // from > to.
        assert!(parse_extraction(
            r#"{"keywords":[],"timeRange":{"from":"2024-06-08","to":"2024-06-07"}}"#,
            "UTC"
        )
        .time_range
        .is_none());
        // Regex miss.
        assert!(parse_extraction(
            r#"{"keywords":[],"timeRange":{"from":"June 1","to":"2024-06-07"}}"#,
            "UTC"
        )
        .time_range
        .is_none());
        // Passes the regex, non-finite Date.parse (month 13).
        assert!(parse_extraction(
            r#"{"keywords":[],"timeRange":{"from":"2024-13-01","to":"2024-12-31"}}"#,
            "UTC"
        )
        .time_range
        .is_none());
        // Null / non-object.
        assert!(
            parse_extraction(r#"{"keywords":[],"timeRange":null}"#, "UTC")
                .time_range
                .is_none()
        );
    }

    #[test]
    fn parse_entities_trimmed_capped_at_five() {
        let out = parse_extraction(
            r#"{"keywords":[],"entities":[" Amy ","","Lighthouse Point",3,"b","c","d","e","f"]}"#,
            "UTC",
        );
        assert_eq!(out.entities, vec!["Amy", "Lighthouse Point", "b", "c", "d"]);
    }

    #[test]
    fn today_line_renders_weekday_and_mode() {
        let clock = ExtractionClock {
            now_iso: "2024-06-15T12:00:00.000Z".to_string(),
            timeline_mode: "narrative".to_string(),
            time_zone: None,
        };
        let msgs = build_distill_messages(
            &[DistillMessage {
                role: "user".to_string(),
                content: "hi".to_string(),
            }],
            "Aria",
            Some(&clock),
        );
        assert!(msgs[1]
            .content
            .contains("TODAY: 2024-06-15 (Saturday); timeline mode: narrative"));
    }

    #[test]
    fn today_line_non_finite_falls_back_raw() {
        let clock = ExtractionClock {
            now_iso: "sometime".to_string(),
            timeline_mode: "realtime".to_string(),
            time_zone: None,
        };
        let msgs = build_distill_messages(&[], "Aria", Some(&clock));
        assert!(msgs[1]
            .content
            .contains("TODAY: sometime; timeline mode: realtime"));
    }

    #[test]
    fn keywords_capped_at_ten() {
        let arr: Vec<String> = (0..15).map(|i| format!("\"k{i}\"")).collect();
        let json = format!("{{\"keywords\":[{}]}}", arr.join(","));
        let out = parse_extraction(&json, "UTC");
        assert_eq!(out.keywords.len(), 10);
    }

    #[test]
    fn code_fences_stripped() {
        let out = parse_extraction("```json\n{\"keywords\":[\"z\"]}\n```", "UTC");
        assert_eq!(out.keywords, vec!["z"]);
    }
}
