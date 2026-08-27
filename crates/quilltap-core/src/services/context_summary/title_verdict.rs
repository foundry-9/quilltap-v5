//! Title-verdict parsing for the two title-consideration cheap-LLM tasks (v4
//! `lib/memory/cheap-llm-tasks/title-verdict.ts`, extracted by `3c041e46` for
//! bug 96).
//!
//! Both [`super::tasks::consider_title_update`] and
//! [`super::tasks::consider_help_chat_title_update`] ask a cheap model the same
//! question and get back the same JSON object. Parsing it is one job, so it
//! lives in one place — v5 already shared one parser where v4 carried two
//! copies, so this port is the BODY change plus the per-site task label.
//!
//! The tolerance here is load-bearing, not decorative. **Bug 96:** a cheap model
//! answered `needsNewTitle: true` with the title under `suggestTitle` — two
//! letters short of the key the prompt asked for. Reading the canonical key
//! alone yielded `undefined`, the caller read that as "no rename wanted", and
//! the chat kept its generic title while the story background that hangs off a
//! successful rename never queued. A one-key typo should not be a silent no.
//!
//! The four `logger.warn` arms are ported as `tracing::warn!` (P4.49 puts them
//! in `combined.log`, where an operator looks): the MESSAGE strings and the
//! `context` values are byte-exact to v4; the metadata field NAMES follow this
//! tree's snake_case tracing convention (`chats.rs:1309`, `llm_logs.rs:1008`)
//! rather than v4's camelCase JSON keys.

use std::collections::HashMap;

use serde_json::{Map, Value};

use crate::jsstr::{js_trim, utf16_len, utf16_truncate};
use crate::memory_tasks::strip_code_fences;

/// v4's `context: 'cheap-llm-tasks.title-verdict'` — byte-exact, because the
/// operator greps `combined.log` for it.
const CONTEXT: &str = "cheap-llm-tasks.title-verdict";

/// Longest title we will store; longer ones are truncated with an ellipsis
/// (v4 `MAX_TITLE_LENGTH`).
pub const MAX_TITLE_LENGTH: usize = 60;

/// Keys a model might put the title under, **canonical first** (v4
/// `TITLE_KEYS`).
///
/// Kept to near-misses of the asked-for key plus the obvious plain synonyms —
/// every entry has to be unambiguously "the new title" in a response object
/// whose only subject is the new title. Matching is also tried case- and
/// separator-insensitively (see [`read_title_key`]), which is what catches
/// `suggested_title` and friends without listing each casing by hand.
///
/// The ORDER is the precedence rule: both passes walk this list, so the
/// canonical key wins whenever a model emits several.
const TITLE_KEYS: [&str; 5] = [
    "suggestedTitle",
    "suggestTitle",
    "newTitle",
    "proposedTitle",
    "title",
];

/// The verdict shape both title tasks resolve to (v4 `TitleVerdict`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TitleVerdict {
    pub needs_new_title: bool,
    pub reason: String,
    pub suggested_title: Option<String>,
}

/// `Suggested_Title` / `suggested-title` / `SUGGESTEDTITLE` all fold to
/// `suggestedtitle` — v4 `key.toLowerCase().replace(/[^a-z0-9]/g, '')`.
///
/// `str::to_lowercase` is byte-identical to JS `toLowerCase` (the Phase-1
/// ICU/Unicode finding), and the surviving class is ASCII-only by construction:
/// every non-`[a-z0-9]` unit — including every non-ASCII letter the lowercase
/// pass produced — is dropped.
fn fold_key(key: &str) -> String {
    key.to_lowercase()
        .chars()
        .filter(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        .collect()
}

/// Pull the title out of a parsed response object, tolerating near-miss keys
/// (v4 `readTitleKey`).
///
/// Returns the raw value plus the key it came from, so the caller can report a
/// non-canonical read rather than absorbing it silently. An explicit `null`
/// never counts as found — pass 1 falls THROUGH a null canonical key to the
/// near-misses, exactly as v4's `!== undefined && !== null` does.
///
/// Pass 2 iterates `Object.keys(parsed)` insertion order (serde_json's
/// `preserve_order` feature, on for this crate) so **the first occurrence wins**
/// on a fold collision. JS would hoist integer-like keys to the front of
/// `Object.keys`, which serde_json does not — immaterial here, since a
/// digit-only key folds to digits and can never collide with any
/// [`TITLE_KEYS`] fold.
fn read_title_key(record: &Map<String, Value>) -> Option<(&Value, String)> {
    for key in TITLE_KEYS {
        if let Some(v) = record.get(key).filter(|v| !v.is_null()) {
            return Some((v, key.to_string()));
        }
    }

    // Second pass: fold both sides, so casing and separators stop mattering.
    let mut folded: HashMap<String, &String> = HashMap::new();
    for actual_key in record.keys() {
        folded.entry(fold_key(actual_key)).or_insert(actual_key);
    }
    for key in TITLE_KEYS {
        let Some(actual_key) = folded.get(&fold_key(key)) else {
            continue;
        };
        if let Some(v) = record.get(*actual_key).filter(|v| !v.is_null()) {
            return Some((v, (*actual_key).clone()));
        }
    }

    None
}

/// Trim, strip a wrapping quote pair, and cap the length (v4 `normalizeTitle`).
///
/// v4 spells it `raw.trim().replace(/^["']/, '').replace(/["']$/, '').trim()` —
/// note the **second** trim, which the inline generator parsers lacked until
/// v4 `dcab791c2` collapsed them onto this spelling (`cleanTitle`); v5's
/// `tasks.rs` cleaners carry the second trim since the same round.
pub(super) fn normalize_title(raw: &str) -> Option<String> {
    let cleaned = js_trim(&strip_edge_quotes(js_trim(raw))).to_string();
    if cleaned.is_empty() {
        return None;
    }
    Some(if utf16_len(&cleaned) > MAX_TITLE_LENGTH {
        format!("{}...", utf16_truncate(&cleaned, MAX_TITLE_LENGTH - 3))
    } else {
        cleaned
    })
}

/// Remove one leading quote char if present AND one trailing quote char if
/// present.
///
/// v4 spells this two ways for the same effect: `normalizeTitle`'s pair of
/// anchored replaces (`/^["']/` then `/["']$/`) and the manual title path's
/// single `/^["']|["']$/g`. The anchored alternation with `/g` can match at
/// most twice — once at the start, once at the end — so the two spellings agree
/// on every input, including a one-character `"` (both yield the empty string).
pub(super) fn strip_edge_quotes(s: &str) -> String {
    let mut chars: Vec<char> = s.chars().collect();
    if matches!(chars.first(), Some('"' | '\'')) {
        chars.remove(0);
    }
    if matches!(chars.last(), Some('"' | '\'')) {
        chars.pop();
    }
    chars.into_iter().collect()
}

/// Parse a title-consideration response into a verdict (v4
/// `parseTitleVerdict`).
///
/// Never fails: an unreadable response resolves to "no new title", which is the
/// safe direction — the chat keeps the title it has. Guessing would rename a
/// chat to something the model never said.
///
/// `task_label` names the asking task in the log lines (v4 passes
/// `'consider-title-update'` / `'consider-help-chat-title-update'`); `chat_id`
/// is the chat under consideration.
pub fn parse_title_verdict(content: &str, task_label: &str, chat_id: Option<&str>) -> TitleVerdict {
    // v4's `logger.warn` meta drops an `undefined` `chatId` at JSON-stringify
    // time; a `tracing` field cannot be omitted per call, so an absent chat
    // renders as the empty string. Cosmetic, and only in the log line.
    let chat_id = chat_id.unwrap_or_default();

    let Ok(parsed) = serde_json::from_str::<Value>(&strip_code_fences(content)) else {
        tracing::warn!(
            context = CONTEXT,
            task_label,
            chat_id,
            // v4 `content.slice(0, 200)` — UTF-16 units.
            content_preview = %utf16_truncate(content, 200),
            "[Title Verdict] Response was not JSON — keeping the current title"
        );
        return TitleVerdict {
            needs_new_title: false,
            reason: "Failed to parse response".to_string(),
            suggested_title: None,
        };
    };

    // v4: `typeof parsed !== 'object' || parsed === null || Array.isArray(parsed)`.
    let Some(record) = parsed.as_object() else {
        tracing::warn!(
            context = CONTEXT,
            task_label,
            chat_id,
            "[Title Verdict] Response JSON was not an object — keeping the current title"
        );
        return TitleVerdict {
            needs_new_title: false,
            reason: "Failed to parse response".to_string(),
            suggested_title: None,
        };
    };

    // A STRICT `=== true`: the string "true" is not enough.
    let needs_new_title = record.get("needsNewTitle") == Some(&Value::Bool(true));

    // `typeof record.reason === 'string' && record.reason.trim()` — a string
    // whose TRIMMED form is truthy, returned UNTRIMMED. Stricter than the
    // pre-fix `||` chain, which accepted any truthy value at all.
    let reason = match record.get("reason").and_then(Value::as_str) {
        Some(r) if !js_trim(r).is_empty() => r.to_string(),
        _ => "No reason provided".to_string(),
    };

    let found = read_title_key(record);
    let mut suggested_title: Option<String> = None;

    if let Some((value, actual_key)) = &found {
        // Only a STRING can be a title; every other type is dropped.
        if let Some(raw) = value.as_str() {
            suggested_title = normalize_title(raw);
            if suggested_title.is_some() && actual_key != "suggestedTitle" {
                tracing::warn!(
                    context = CONTEXT,
                    task_label,
                    chat_id,
                    actual_key = %actual_key,
                    expected_key = "suggestedTitle",
                    "[Title Verdict] Title arrived under a non-canonical key"
                );
            }
        }
    }

    // The case bug 96 turned on: the model asked for a rename and we cannot find
    // the title it meant. Say so — this used to pass for a quiet "no".
    if needs_new_title && suggested_title.is_none() {
        tracing::warn!(
            context = CONTEXT,
            task_label,
            chat_id,
            response_keys = ?record.keys().collect::<Vec<_>>(),
            "[Title Verdict] Model asked for a rename but supplied no usable title"
        );
    }

    TitleVerdict {
        needs_new_title,
        reason,
        suggested_title,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    const LABEL: &str = "consider-title-update";

    fn parse(json: &str) -> TitleVerdict {
        parse_title_verdict(json, LABEL, None)
    }

    // ── The captured-warn rig (the `cheap_llm_exec.rs` / `build_context.rs`
    // idiom): the three warn arms and the ONE case that must stay silent are
    // v4 test cases too, so they are pinned the same way. ──

    struct CaptureLayer(Arc<Mutex<Vec<String>>>);

    struct FieldVisitor(String);
    impl tracing::field::Visit for FieldVisitor {
        fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
            self.0.push_str(&format!(" {}={}", field.name(), value));
        }
        fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
            if field.name() == "message" {
                self.0.push_str(&format!(" {value:?}"));
            } else {
                self.0.push_str(&format!(" {}={value:?}", field.name()));
            }
        }
    }

    impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for CaptureLayer {
        fn on_event(
            &self,
            event: &tracing::Event<'_>,
            _ctx: tracing_subscriber::layer::Context<'_, S>,
        ) {
            let meta = event.metadata();
            let mut visitor = FieldVisitor(format!("{} {}", meta.level(), meta.target()));
            event.record(&mut visitor);
            self.0.lock().unwrap().push(visitor.0);
        }
    }

    /// Run `f` with a capturing subscriber installed and hand back the lines.
    fn captured(f: impl FnOnce()) -> Vec<String> {
        use tracing_subscriber::layer::SubscriberExt;
        let logs = Arc::new(Mutex::new(Vec::<String>::new()));
        let subscriber = tracing_subscriber::registry().with(CaptureLayer(logs.clone()));
        {
            let _guard = tracing::subscriber::set_default(subscriber);
            f();
        }
        let out = logs.lock().unwrap().clone();
        out
    }

    // ── v4 `__tests__/title-verdict.test.ts`, mirrored 1:1 ──

    #[test]
    fn reads_the_canonical_key() {
        let v = parse(
            r#"{"needsNewTitle":true,"reason":"generic","suggestedTitle":"A Quiet Reckoning"}"#,
        );
        assert_eq!(
            v,
            TitleVerdict {
                needs_new_title: true,
                reason: "generic".to_string(),
                suggested_title: Some("A Quiet Reckoning".to_string()),
            }
        );
    }

    #[test]
    fn recovers_the_exact_live_payload_that_caused_bug_96() {
        // Friday, chat 745e8a5e, 2026-08-23 22:18:10 UTC: deepseek-v4-flash
        // answered `needsNewTitle: true` with the title under `suggestTitle`.
        let v = parse(
            r#"{"needsNewTitle":true,"reason":"The current title is generic and doesn't reflect the content.","suggestTitle":"The Beast's Hundred Gigajoules"}"#,
        );
        assert!(v.needs_new_title);
        assert_eq!(
            v.suggested_title.as_deref(),
            Some("The Beast's Hundred Gigajoules")
        );
    }

    #[test]
    fn accepts_a_title_under_every_tolerated_key() {
        for key in [
            "suggestTitle",
            "newTitle",
            "proposedTitle",
            "title",
            // Reachable only through the fold pass.
            "suggested_title",
            "suggested-title",
            "SuggestedTitle",
            "SUGGESTED_TITLE",
        ] {
            let v = parse(&format!(
                r#"{{"needsNewTitle":true,"reason":"r","{key}":"Amber Lines Above the Table"}}"#
            ));
            assert_eq!(
                v.suggested_title.as_deref(),
                Some("Amber Lines Above the Table"),
                "key {key}"
            );
        }
    }

    #[test]
    fn prefers_the_canonical_key_when_a_model_emits_several() {
        // The near-miss comes FIRST in insertion order — precedence is
        // TITLE_KEYS order, not object order.
        let v = parse(
            r#"{"needsNewTitle":true,"reason":"r","title":"Wrong One","suggestedTitle":"Right One"}"#,
        );
        assert_eq!(v.suggested_title.as_deref(), Some("Right One"));
    }

    #[test]
    fn honours_a_genuine_decline_even_with_a_title_present() {
        let v = parse(
            r#"{"needsNewTitle":false,"reason":"already descriptive","suggestedTitle":"Nope"}"#,
        );
        assert!(!v.needs_new_title);
    }

    #[test]
    fn strips_code_fences() {
        let v = parse_title_verdict(
            "```json\n{\"needsNewTitle\": true, \"reason\": \"r\", \"suggestedTitle\": \"Fenced\"}\n```",
            LABEL,
            None,
        );
        assert_eq!(v.suggested_title.as_deref(), Some("Fenced"));
    }

    #[test]
    fn unwraps_a_quoted_title() {
        let v = parse(r#"{"needsNewTitle":true,"reason":"r","suggestedTitle":"\"Quoted Title\""}"#);
        assert_eq!(v.suggested_title.as_deref(), Some("Quoted Title"));
    }

    #[test]
    fn trims_again_after_unwrapping_the_quotes() {
        // v4's SECOND trim — the pre-fix inline parsers stopped after the quote
        // strip, so this padding used to reach the chat row.
        let v = parse(
            r#"{"needsNewTitle":true,"reason":"r","suggestedTitle":"  \"  A Padded Quoted Title  \"  "}"#,
        );
        assert_eq!(v.suggested_title.as_deref(), Some("A Padded Quoted Title"));
    }

    #[test]
    fn truncates_an_overlong_title_to_the_cap() {
        let long = "x".repeat(200);
        let v = parse(&format!(
            r#"{{"needsNewTitle":true,"reason":"r","suggestedTitle":"{long}"}}"#
        ));
        let t = v.suggested_title.unwrap();
        assert_eq!(utf16_len(&t), MAX_TITLE_LENGTH);
        assert!(t.ends_with("..."));
        assert_eq!(
            &t[..MAX_TITLE_LENGTH - 3],
            &"x".repeat(MAX_TITLE_LENGTH - 3)
        );
    }

    #[test]
    fn treats_a_whitespace_only_title_as_absent() {
        let v = parse(r#"{"needsNewTitle":true,"reason":"r","suggestedTitle":"   "}"#);
        assert_eq!(v.suggested_title, None);
    }

    #[test]
    fn declines_rather_than_failing_on_unparseable_output() {
        let v = parse_title_verdict("I think the title is fine, actually.", LABEL, None);
        assert_eq!(
            v,
            TitleVerdict {
                needs_new_title: false,
                reason: "Failed to parse response".to_string(),
                suggested_title: None,
            }
        );
    }

    #[test]
    fn declines_on_json_that_is_not_an_object() {
        assert!(!parse_title_verdict(r#"["a title"]"#, LABEL, None).needs_new_title);
        assert!(!parse_title_verdict(r#""a title""#, LABEL, None).needs_new_title);
        // v4's guard names `null` explicitly; `JSON.parse('null')` is an object
        // by `typeof` and only the null check rejects it.
        assert_eq!(
            parse_title_verdict("null", LABEL, None).reason,
            "Failed to parse response"
        );
    }

    #[test]
    fn ignores_a_non_string_title_value() {
        let v = parse(r#"{"needsNewTitle":true,"reason":"r","suggestedTitle":42}"#);
        assert_eq!(v.suggested_title, None);
    }

    #[test]
    fn warns_when_a_rename_is_requested_with_no_readable_title() {
        let lines = captured(|| {
            parse_title_verdict(
                r#"{"needsNewTitle":true,"reason":"r","headline":"Under An Unknown Key"}"#,
                LABEL,
                Some("chat-1"),
            );
        });
        let line = lines
            .iter()
            .find(|l| l.contains("supplied no usable title"))
            .expect("the no-usable-title warn");
        assert!(line.contains("chat_id=chat-1"), "{line}");
        assert!(
            line.contains("context=cheap-llm-tasks.title-verdict"),
            "{line}"
        );
        assert!(line.contains("task_label=consider-title-update"), "{line}");
        // v4 logs `responseKeys: Object.keys(record)`.
        assert!(line.contains("headline"), "{line}");
    }

    #[test]
    fn warns_when_the_title_arrives_under_a_non_canonical_key() {
        let lines = captured(|| {
            parse_title_verdict(
                r#"{"needsNewTitle":true,"reason":"r","suggestTitle":"Recovered"}"#,
                LABEL,
                Some("chat-2"),
            );
        });
        let line = lines
            .iter()
            .find(|l| l.contains("non-canonical key"))
            .expect("the non-canonical-key warn");
        assert!(line.contains("actual_key=suggestTitle"), "{line}");
        assert!(line.contains("expected_key=suggestedTitle"), "{line}");
    }

    #[test]
    fn does_not_warn_on_a_clean_canonical_response() {
        let lines = captured(|| {
            parse_title_verdict(
                r#"{"needsNewTitle":true,"reason":"r","suggestedTitle":"Clean"}"#,
                LABEL,
                None,
            );
        });
        assert!(lines.is_empty(), "expected silence, got {lines:?}");
    }

    #[test]
    fn defaults_a_missing_reason() {
        assert_eq!(
            parse(r#"{"needsNewTitle":false}"#).reason,
            "No reason provided"
        );
    }

    // ── The arms v4's suite leaves to `title-verdict.ts` itself ──

    #[test]
    fn the_reason_must_be_a_string_whose_trim_is_truthy() {
        // Stricter than the pre-fix `parsed.reason || …` chain: a whitespace-only
        // string, and any non-string value at all, fall back.
        assert_eq!(parse(r#"{"reason":"   "}"#).reason, "No reason provided");
        assert_eq!(parse(r#"{"reason":42}"#).reason, "No reason provided");
        assert_eq!(parse(r#"{"reason":true}"#).reason, "No reason provided");
        // …and a truthy reason comes back UNTRIMMED.
        assert_eq!(parse(r#"{"reason":"  padded  "}"#).reason, "  padded  ");
    }

    #[test]
    fn needs_new_title_is_a_strict_equality() {
        assert!(!parse(r#"{"needsNewTitle":"true"}"#).needs_new_title);
        assert!(!parse(r#"{"needsNewTitle":1}"#).needs_new_title);
        assert!(parse(r#"{"needsNewTitle":true}"#).needs_new_title);
    }

    #[test]
    fn an_explicit_null_canonical_key_falls_through_to_a_near_miss() {
        let v = parse(
            r#"{"needsNewTitle":true,"reason":"r","suggestedTitle":null,"newTitle":"Recovered From A Null"}"#,
        );
        assert_eq!(v.suggested_title.as_deref(), Some("Recovered From A Null"));
    }

    #[test]
    fn a_fold_collision_takes_the_first_occurrence() {
        // Two keys folding to `suggestedtitle`; the fold map keeps the FIRST.
        let v = parse(
            r#"{"needsNewTitle":true,"reason":"r","suggested_title":"First","Suggested-Title":"Second"}"#,
        );
        assert_eq!(v.suggested_title.as_deref(), Some("First"));
    }

    #[test]
    fn fold_key_drops_every_non_ascii_alphanumeric() {
        assert_eq!(fold_key("Suggested_Title"), "suggestedtitle");
        assert_eq!(fold_key("suggested-title"), "suggestedtitle");
        assert_eq!(fold_key("SUGGESTEDTITLE"), "suggestedtitle");
        assert_eq!(fold_key(" suggested title 2 "), "suggestedtitle2");
        // Non-ASCII survives `toLowerCase` and is then dropped by `[^a-z0-9]`.
        assert_eq!(fold_key("SÜGGESTED"), "sggested");
    }

    #[test]
    fn strip_edge_quotes_removes_at_most_one_at_each_end() {
        assert_eq!(strip_edge_quotes("\"\"double\"\""), "\"double\"");
        assert_eq!(strip_edge_quotes("'single'"), "single");
        // A lone quote character strips to nothing in v4's two-replace spelling
        // as well: the leading replace consumes it, the trailing one finds an
        // empty string.
        assert_eq!(strip_edge_quotes("\""), "");
    }

    #[test]
    fn normalize_title_reports_an_emptied_title_as_absent() {
        assert_eq!(normalize_title("  \"  \"  "), None);
        assert_eq!(normalize_title(""), None);
    }
}
