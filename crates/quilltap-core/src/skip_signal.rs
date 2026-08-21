//! "Nothing to add" turn-skipping — shared pure logic (v4
//! `lib/chat/turn-manager/skip-signal.ts`, whole, at `b90cd1f5`).
//!
//! In qualifying group scenes an LLM character may pass its turn by replying
//! with the single line `[NOTHING TO ADD]`; the Host posts a `turn-pass` note
//! (never containing the sentinel), the rotation advances, and a stall guard
//! forces speech when everyone else has passed.
//!
//! Composes the ALREADY-PORTED leaves — nothing is ported twice:
//! [`crate::message_formatter::normalize_content_block_format`] +
//! [`crate::message_formatter::strip_character_name_prefix`] (v4 hosts these in
//! the extracted `lib/llm/response-normalizer.ts`, which `message-formatter`
//! re-exports — a pure v4 refactor, so the ported `message_formatter` stays the
//! canonical home) and [`crate::chat_predicates::is_participant_present`]. v4's
//! remaining client-safe import is `escapeRegex` (`lib/utils/regex.ts`); the
//! Rust side uses `regex::escape`, which escapes a superset of v4's metacharacter
//! set — the escaped *source bytes* differ, but every escaped token matches the
//! same literal, and only matching is observable here.
//!
//! A pass is recorded as a Host message (`systemSender: 'host'`,
//! `systemKind: 'turn-pass'`, `hostEvent: { participantId }`) — no new state
//! columns. Every derivation below (last speaker, cycle membership, must-speak
//! guard) recomputes from that history.
//!
//! The history-walking functions take the chat events as raw
//! [`serde_json::Value`] rows (the `getMessages` JSON shape the services layer
//! carries), like the other event-scanning services; participants likewise come
//! in as the chat's `participants` JSON array elements.

use std::collections::HashSet;

use regex::Regex;
use serde_json::Value;

use crate::chat_predicates::{is_participant_present, ParticipantStatus};
use crate::jsstr::{js_trim, utf16_len};
use crate::message_formatter::{normalize_content_block_format, strip_character_name_prefix};

/// The literal sentinel a character emits to pass its turn.
pub const NOTHING_TO_ADD_SENTINEL: &str = "[NOTHING TO ADD]";

/// `systemKind` stamped on the Host message that records a pass.
pub const TURN_PASS_SYSTEM_KIND: &str = "turn-pass";

// ---------------------------------------------------------------------------
// JSON field helpers (JS-truthiness aware where v4 is).
// ---------------------------------------------------------------------------

fn str_field<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.get(key).and_then(Value::as_str)
}

/// v4 `isParticipantPresent(p.status)` over a raw participant object: a missing
/// or unknown status matches neither present state (v4's set-membership check on
/// `undefined` is `false`); there is NO `"active"` defaulting here — the
/// persisted `participants` column always materializes `status` (Zod default),
/// so the field is present on every real row.
fn participant_is_present(p: &Value) -> bool {
    let status = match str_field(p, "status") {
        Some("active") => ParticipantStatus::Active,
        Some("silent") => ParticipantStatus::Silent,
        Some("removed") => ParticipantStatus::Removed,
        Some("absent") => ParticipantStatus::Absent,
        _ => return false,
    };
    is_participant_present(status)
}

/// JS-truthy `p.characterId` (a non-empty string).
fn truthy_character_id(p: &Value) -> bool {
    str_field(p, "characterId").is_some_and(|c| !c.is_empty())
}

/// JS-truthy `m.systemSender` (a non-empty string).
fn truthy_system_sender(m: &Value) -> bool {
    str_field(m, "systemSender").is_some_and(|s| !s.is_empty())
}

/// A whisper: `targetParticipantIds` present as a non-empty array (v4
/// `Array.isArray(t) && t.length > 0`).
fn is_whisper(m: &Value) -> bool {
    m.get("targetParticipantIds")
        .and_then(Value::as_array)
        .is_some_and(|a| !a.is_empty())
}

/// Whether a whisper's targets include `participant_id` (only string elements
/// can match, as in v4's `targets.includes(id)`).
fn targets_include(m: &Value, participant_id: &str) -> bool {
    m.get("targetParticipantIds")
        .and_then(Value::as_array)
        .is_some_and(|a| a.iter().any(|t| t.as_str() == Some(participant_id)))
}

// ---------------------------------------------------------------------------
// isTurnPassMessage
// ---------------------------------------------------------------------------

/// Type guard: is this event a Host turn-pass record? A turn-pass carries the
/// passing participant's id in `hostEvent.participantId` (v4 checks
/// `typeof … === 'string'` — an empty string still qualifies).
pub fn is_turn_pass_message(m: &Value) -> bool {
    turn_pass_participant_id(m).is_some()
}

/// The turn-pass record's `hostEvent.participantId`, when the full
/// `isTurnPassMessage` guard passes.
pub fn turn_pass_participant_id(m: &Value) -> Option<&str> {
    if !m.is_object() {
        return None;
    }
    if str_field(m, "type") != Some("message") {
        return None;
    }
    if str_field(m, "systemSender") != Some("host") {
        return None;
    }
    if str_field(m, "systemKind") != Some(TURN_PASS_SYSTEM_KIND) {
        return None;
    }
    // `!!he && typeof he === 'object' && typeof he.participantId === 'string'`.
    m.get("hostEvent")
        .filter(|he| he.is_object())
        .and_then(|he| he.get("participantId"))
        .and_then(Value::as_str)
}

// ---------------------------------------------------------------------------
// detectSkipSentinel
// ---------------------------------------------------------------------------

/// v4 `DetectSkipResult`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DetectSkipResult {
    /// `{ skip: true }` — a bare sentinel (nothing but whitespace after it).
    Skip,
    /// `{ skip: false, cleaned? }` — `cleaned` is `Some` when a sentinel line
    /// was followed by real prose (the sentinel line removed, prose kept).
    NoSkip { cleaned: Option<String> },
}

/// Decide whether a raw model response is a turn-pass.
///
/// The response is normalized (`normalizeContentBlockFormat`), stripped of any
/// leading own-name prefix (`stripCharacterNamePrefix`), and its FIRST
/// non-empty line is examined. That line — with surrounding markdown/quote
/// wrappers (`* _ ~ " ' \``), optional square brackets, and trailing
/// punctuation shed, case-insensitively — must equal `NOTHING TO ADD`.
///
///   - Bare sentinel (nothing but whitespace after it) → `Skip`.
///   - Sentinel line followed by real prose → NOT a skip; returns
///     `NoSkip { cleaned: Some(…) }` with the sentinel line removed so the
///     caller can keep the prose.
///   - Real prose that merely ENDS with a lone sentinel line → NOT a skip;
///     returns `NoSkip { cleaned: Some(…) }` with that trailing line removed.
///     Weak models often narrate, then tack `[NOTHING TO ADD]` on the end; the
///     narration is a genuine contribution and must be kept, but the dangling
///     sentinel line should never reach display / persistence / memory.
///   - No sentinel → `NoSkip { cleaned: None }`.
pub fn detect_skip_sentinel(
    response: &str,
    character_name: Option<&str>,
    aliases: Option<&[String]>,
) -> DetectSkipResult {
    if response.is_empty() {
        return DetectSkipResult::NoSkip { cleaned: None };
    }

    // Only strip a name prefix when we actually have a name/aliases to target.
    // With none, stripCharacterNamePrefix falls back to a generic bracketed-name
    // pattern that would eat the sentinel's own `[NOTHING TO ADD]` brackets.
    let normalized_raw = normalize_content_block_format(response);
    let have_name =
        character_name.is_some_and(|n| !n.is_empty()) || aliases.is_some_and(|a| !a.is_empty());
    let normalized = if have_name {
        strip_character_name_prefix(&normalized_raw, character_name, aliases)
    } else {
        normalized_raw
    };

    let lines: Vec<&str> = normalized.split('\n').collect();

    // Locate the first non-empty line.
    let Some(first_idx) = lines.iter().position(|l| !js_trim(l).is_empty()) else {
        return DetectSkipResult::NoSkip { cleaned: None };
    };

    if is_sentinel_line(lines[first_idx]) {
        // Sentinel matched on the first line. Is there any non-whitespace after it?
        let trailing = lines[first_idx + 1..].join("\n");
        if js_trim(&trailing).is_empty() {
            return DetectSkipResult::Skip;
        }

        // Sentinel + prose: drop the sentinel line, keep the rest.
        let mut kept: Vec<&str> = Vec::with_capacity(lines.len() - 1);
        kept.extend_from_slice(&lines[..first_idx]);
        kept.extend_from_slice(&lines[first_idx + 1..]);
        let cleaned = js_trim(&kept.join("\n")).to_string();
        return DetectSkipResult::NoSkip {
            cleaned: Some(cleaned),
        };
    }

    // The first line is real content, not the sentinel. But weak models often
    // narrate a genuine turn and then dangle a lone `[NOTHING TO ADD]` line at
    // the end. That is NOT a pass — there's real communication above it — but
    // the dangling line must be stripped from what is displayed, saved, and
    // remembered. Locate the LAST non-empty line and, if it is a sentinel line
    // on its own, drop it and keep the prose above.
    let mut last_idx: Option<usize> = None;
    for i in (first_idx + 1..lines.len()).rev() {
        if !js_trim(lines[i]).is_empty() {
            last_idx = Some(i);
            break;
        }
    }
    if let Some(last_idx) = last_idx {
        if is_sentinel_line(lines[last_idx]) {
            let mut kept: Vec<&str> = Vec::with_capacity(lines.len() - 1);
            kept.extend_from_slice(&lines[..last_idx]);
            kept.extend_from_slice(&lines[last_idx + 1..]);
            let cleaned = js_trim(&kept.join("\n")).to_string();
            return DetectSkipResult::NoSkip {
                cleaned: Some(cleaned),
            };
        }
    }

    DetectSkipResult::NoSkip { cleaned: None }
}

/// Does a single line, once its wrapping and trailing punctuation are shed,
/// equal the sentinel phrase? Brackets are optional; matching is
/// case-insensitive.
fn is_sentinel_line(line: &str) -> bool {
    let mut s = js_trim(line).to_string();
    // Shed matched wrapping markdown / quote characters, repeatedly.
    // (e.g. **[NOTHING TO ADD]**, "_[nothing to add]_", `[NOTHING TO ADD]`)
    // v4 trims one wrapper char from EACH end per pass when the first char is a
    // wrapper AND (first === last OR the last is any wrapper), re-trimming; the
    // `s.length > 1` guard is UTF-16 units (JS `String.length`).
    const WRAPPERS: [char; 6] = ['*', '_', '~', '"', '\'', '`'];
    let is_wrapper = |c: char| WRAPPERS.contains(&c);
    let mut changed = true;
    while changed && utf16_len(&s) > 1 {
        changed = false;
        let first = s.chars().next();
        let last = s.chars().next_back();
        if let (Some(first), Some(last)) = (first, last) {
            if is_wrapper(first) && (first == last || is_wrapper(last)) {
                // Trim one wrapper char from each end when both ends are
                // wrapper chars (both are ASCII here, so one char == one
                // UTF-16 unit — v4's `s.slice(1, -1)`).
                let inner = &s[first.len_utf8()..s.len() - last.len_utf8()];
                s = js_trim(inner).to_string();
                changed = true;
            }
        }
    }
    // Drop optional square brackets (v4 `replace(/^\[/, '')` /
    // `replace(/\]$/, '')` — ONE from each end).
    let mut t = s.as_str();
    t = t.strip_prefix('[').unwrap_or(t);
    t = t.strip_suffix(']').unwrap_or(t);
    t = js_trim(t);
    // Drop trailing punctuation (`/[.!?,;:]+$/`).
    let t = t.trim_end_matches(['.', '!', '?', ',', ';', ':']);
    let t = js_trim(t);
    t.to_lowercase() == "nothing to add"
}

// ---------------------------------------------------------------------------
// findSkippedSinceLastSubstantive
// ---------------------------------------------------------------------------

/// Backward-walk the event history collecting the participant ids of every
/// turn-pass record posted since the most recent substantive message. A
/// "substantive" message is a non-whisper USER/ASSISTANT message with a
/// participantId (same predicate `calculateTurnStateFromHistory` uses for
/// `lastSpeakerId`); turn-pass records carry a null participantId so they are
/// never mistaken for substantive.
pub fn find_skipped_since_last_substantive(events: &[Value]) -> HashSet<String> {
    let mut skipped: HashSet<String> = HashSet::new();
    for m in events.iter().rev() {
        if str_field(m, "type") != Some("message") {
            continue;
        }
        if let Some(pid) = turn_pass_participant_id(m) {
            skipped.insert(pid.to_string());
            continue;
        }
        let role = str_field(m, "role");
        let has_pid = str_field(m, "participantId").is_some_and(|p| !p.is_empty());
        if (role == Some("USER") || role == Some("ASSISTANT")) && has_pid && !is_whisper(m) {
            break;
        }
    }
    skipped
}

// ---------------------------------------------------------------------------
// qualifiesForTurnSkipping
// ---------------------------------------------------------------------------

/// Whether a chat is large/busy enough for turn-skipping to apply at all.
///
/// The feature is meant for genuine group scenes, not a one-on-one. It applies
/// only when either:
///   - more than two active character participants are present, OR
///   - at least two of them are LLM-driven.
///
/// So a lone human + a single LLM (a duet) is excluded, while two-or-more LLMs,
/// or any three-plus-participant scene, qualifies.
pub fn qualifies_for_turn_skipping(participants: &[Value]) -> bool {
    let active_chars: Vec<&Value> = participants
        .iter()
        .filter(|p| {
            str_field(p, "type") == Some("CHARACTER")
                && participant_is_present(p)
                && truthy_character_id(p)
        })
        .collect();
    if active_chars.len() > 2 {
        return true;
    }
    let llm_chars = active_chars
        .iter()
        .filter(|p| str_field(p, "controlledBy") != Some("user"))
        .count();
    llm_chars >= 2
}

// ---------------------------------------------------------------------------
// isFirstCharacterTurn
// ---------------------------------------------------------------------------

/// True when no character has yet taken an LLM turn in this chat — i.e. there
/// is no ASSISTANT message carrying a non-null participantId. Greetings count as
/// turns (they carry a participantId); Staff messages do not (participantId is
/// null). The very first character turn of the whole chat is skip-exempt.
pub fn is_first_character_turn(events: &[Value]) -> bool {
    for m in events {
        if str_field(m, "type") != Some("message") {
            continue;
        }
        if str_field(m, "role") == Some("ASSISTANT")
            && str_field(m, "participantId").is_some_and(|p| !p.is_empty())
        {
            return false;
        }
    }
    true
}

// ---------------------------------------------------------------------------
// isRecentlyAddressed
// ---------------------------------------------------------------------------

/// A conversational turn visible to the responding character: a real
/// (non-Staff) message with content, not silent, and not a whisper targeted
/// away from this character. Mirrors the notion in
/// `lib/chat/context/core-whisper-trigger.ts`. (Faithful to v4: no role check —
/// a content-bearing TOOL row without a systemSender is included.)
fn is_visible_conversational_turn(m: &Value, responding_participant_id: &str) -> bool {
    if truthy_system_sender(m) {
        return false;
    }
    if m.get("isSilentMessage").and_then(Value::as_bool) == Some(true) {
        return false;
    }
    let Some(content) = str_field(m, "content") else {
        return false;
    };
    if js_trim(content).is_empty() {
        return false;
    }
    if is_whisper(m) && !targets_include(m, responding_participant_id) {
        return false;
    }
    true
}

/// How many recent visible turns to scan for a "recently addressed" signal.
const RECENTLY_ADDRESSED_LOOKBACK: usize = 10;

/// The responder's name/alias set for the direct-address scan (the fields v4's
/// `buildDirectAddressRegex(respondingCharacter)` consumes).
#[derive(Clone, Debug)]
pub struct RespondingCharacter {
    pub id: String,
    pub name: String,
    pub aliases: Vec<String>,
}

/// Short interjections that commonly lead into a vocative ("Hey Marion, ...").
/// Deliberately small — the goal is catching real address openers, not every
/// word that could precede a name. (v4 `e22f7b36`, transcribed.)
const VOCATIVE_LEAD_INS: &str =
    "(?:hey|hi|hello|oh|no|yes|well|so|and|but|now|listen|please|right|ok|okay|thanks|sorry|merci)";

/// Characters that can end the clause *before* a vocative: sentence punctuation,
/// quotes, brackets, markdown emphasis, newlines, dashes. `-` sits last so the
/// class needs no escaping. (v4's regex source bytes, transcribed.)
const VOCATIVE_PRE_BOUNDARY: &str = "[.!?;:…\"“”'()\\[\\]*_~\\n—–-]";

/// JS `\s`, written out. ECMAScript's `\s` is WhiteSpace ∪ LineTerminator:
/// TAB/VT/FF/SP, every `Zs`, U+FEFF, and LF/CR/LS/PS. Rust's `\s` is
/// `\p{White_Space}`, which **excludes U+FEFF and includes U+0085 (NEL)** —
/// both divergences are reachable from real message content, so the class is
/// spelled rather than borrowed. Pinned by the `bom-*` / `nel-*` oracle vectors.
const JS_SPACE: &str = "[\\t\\n\\x0B\\x0C\\r \\u{a0}\\u{1680}\\u{2000}-\\u{200a}\\u{2028}\\u{2029}\\u{202f}\\u{205f}\\u{3000}\\u{feff}]";

/// The line terminators JS's `m`-flag `^`/`$` honour that Rust's `(?m)` does
/// not (Rust anchors on `\n` alone). *Consuming* one of these next to the token
/// is equivalent — for the *existence* of a match, which is all `.test()` asks
/// — to JS's zero-width anchor at the adjacent position. Pinned by the `cr-*`
/// oracle vectors; without it a lone CR (no LF) diverges.
const JS_ONLY_LINE_TERMINATOR: &str = "[\\r\\u{2028}\\u{2029}]";

/// Build a regex matching the character's name or an alias in a
/// direct-address (vocative) position: preceded by the start of the text, a
/// clause boundary, a comma, an `@`, or a lead-in interjection — and followed
/// by address punctuation (`Marion,` / `Greg?` / `Amy —` / `Al.`) or the end of
/// a line. A name flowing mid-sentence ("if Greg is ready", "Friday's block",
/// "I glance at Amy over the bench") deliberately does NOT match: narrating or
/// citing someone is not speaking *to* them.
///
/// Returns `None` when the character has no usable name tokens.
///
/// **Case folding is a recorded divergence.** v4 uses the JS `i` flag *without*
/// `u`, i.e. ECMAScript Canonicalize (simple `toUppercase`, with the
/// non-ASCII→ASCII guard); Rust's `(?i)` is Unicode simple case folding, whose
/// equivalence classes are a superset on a handful of exotic code points
/// (U+212A KELVIN SIGN vs `k`, U+1E9E capital sharp s vs `ß`, U+017F long s vs
/// `s`). v5 therefore may see "directly addressed" where v4 would not — the
/// safe direction (the answer-rather-than-pass caution appears). This matches
/// the precedent already shipped in [`crate::mentioned_characters`], and the
/// divergence was RATIFIED by the human on 2026-08-21 (round record:
/// `status-log.md` → the `b8449b3e` round); the oracle
/// corpus pins the agreeing non-ASCII vectors.
fn build_direct_address_regex(character: &RespondingCharacter) -> Option<Regex> {
    let mut tokens: Vec<String> = std::iter::once(character.name.as_str())
        .chain(character.aliases.iter().map(String::as_str))
        .map(|t| js_trim(t).to_string())
        .filter(|t| !t.is_empty())
        .collect();
    if tokens.is_empty() {
        return None;
    }

    // Longer tokens first so "John Smith" wins over "John". v4's comparator is
    // `b.length - a.length` — UTF-16 code units — over V8's stable sort.
    tokens.sort_by_key(|t| std::cmp::Reverse(utf16_len(t)));
    let names = tokens
        .iter()
        .map(|t| regex::escape(t))
        .collect::<Vec<_>>()
        .join("|");

    // Only the compiled-size limit can reject a fully-escaped pattern; degrade
    // to "not addressed" rather than v4's `new RegExp` throw.
    Regex::new(&format!(
        "(?im)\
         (?:^|{JS_ONLY_LINE_TERMINATOR}|{VOCATIVE_PRE_BOUNDARY}{JS_SPACE}*|,{JS_SPACE}+|@|(?-u:\\b){VOCATIVE_LEAD_INS},?{JS_SPACE}+)\
         (?:{names}){JS_SPACE}*(?:[,.!?…—–:;]|{JS_ONLY_LINE_TERMINATOR}|$)"
    ))
    .ok()
}

/// Has the responding character been DIRECTLY addressed since they last spoke?
/// Scans the visible conversational turns after the responder's own most recent
/// non-whisper ASSISTANT message (capped at the last
/// [`RECENTLY_ADDRESSED_LOOKBACK`]). A hit is either the responder's name/alias
/// in a vocative position ([`build_direct_address_regex`]), or a whisper
/// targeted at the responder.
///
/// Direct address, not mere mention, on purpose: in a chorus-prone group scene
/// every turn's roll-call recap names most of the cast, so a mention-based
/// signal marked everyone as addressed forever and the "answer rather than
/// pass" caution fired for every character on every turn — nobody ever passed.
pub fn is_recently_addressed(
    events: &[Value],
    responding_participant_id: &str,
    responding_character: &RespondingCharacter,
) -> bool {
    // Find the responder's own last non-whisper ASSISTANT message.
    let mut last_own_idx: Option<usize> = None;
    for (i, m) in events.iter().enumerate().rev() {
        if str_field(m, "type") != Some("message") {
            continue;
        }
        if str_field(m, "role") != Some("ASSISTANT") {
            continue;
        }
        if str_field(m, "participantId") != Some(responding_participant_id) {
            continue;
        }
        if truthy_system_sender(m) {
            continue;
        }
        if is_whisper(m) {
            continue;
        }
        last_own_idx = Some(i);
        break;
    }

    // Collect visible conversational turns after that boundary.
    let start = last_own_idx.map(|i| i + 1).unwrap_or(0);
    let visible: Vec<&Value> = events[start..]
        .iter()
        .filter(|m| {
            str_field(m, "type") == Some("message")
                && is_visible_conversational_turn(m, responding_participant_id)
        })
        .collect();
    let window = &visible[visible.len().saturating_sub(RECENTLY_ADDRESSED_LOOKBACK)..];
    if window.is_empty() {
        return false;
    }

    // A whisper targeted at the responder counts as addressed regardless of text.
    for m in window {
        if is_whisper(m) && targets_include(m, responding_participant_id) {
            return true;
        }
    }

    let Some(regex) = build_direct_address_regex(responding_character) else {
        return false;
    };
    let corpus = window
        .iter()
        .map(|m| str_field(m, "content").unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n");
    regex.is_match(&corpus)
}

// ---------------------------------------------------------------------------
// computeSkipEligibility
// ---------------------------------------------------------------------------

/// v4 `MustSpeakReason` (the non-null members; `None` on the outer `Option`
/// means the skip may be offered).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MustSpeakReason {
    NotMultiCharacter,
    FeatureDisabled,
    FirstCharacterTurn,
    Summoned,
    AlreadySkipped,
    AllOthersSkipped,
}

impl MustSpeakReason {
    /// The v4 string form.
    pub fn as_str(self) -> &'static str {
        match self {
            MustSpeakReason::NotMultiCharacter => "not-multi-character",
            MustSpeakReason::FeatureDisabled => "feature-disabled",
            MustSpeakReason::FirstCharacterTurn => "first-character-turn",
            MustSpeakReason::Summoned => "summoned",
            MustSpeakReason::AlreadySkipped => "already-skipped",
            MustSpeakReason::AllOthersSkipped => "all-others-skipped",
        }
    }
}

/// v4 `ComputeSkipEligibilityOptions`.
pub struct ComputeSkipEligibilityOptions<'a> {
    pub events: &'a [Value],
    pub participants: &'a [Value],
    pub responding_participant_id: &'a str,
    pub responding_character: &'a RespondingCharacter,
    /// Nudge / queue-popped turn — the operator explicitly summoned this voice.
    pub summoned: bool,
    /// Per-chat toggle; NULL/true = enabled. Pass
    /// `chat.turnSkippingEnabled !== false`.
    pub turn_skipping_enabled: bool,
}

/// v4 `SkipEligibility`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SkipEligibility {
    pub offer_skip: bool,
    pub must_speak_reason: Option<MustSpeakReason>,
    pub recently_addressed: bool,
}

/// Decide whether the responding character may be offered the skip option this
/// turn, and (for logging / the human Skip-button guard) why not.
///
/// The single must-speak rule: a responder must speak when every OTHER active
/// CHARACTER participant (LLM or user-controlled) has a turn-pass record since
/// the last substantive message. With no other participants the `.every()` is
/// vacuously true — skipping into an empty room is intentionally forbidden.
///
/// Precedence of withhold reasons: feature-disabled → first-character-turn →
/// summoned → already-skipped → all-others-skipped. (`recentlyAddressed` is
/// computed UNCONDITIONALLY first, before any withhold branch.)
pub fn compute_skip_eligibility(options: &ComputeSkipEligibilityOptions<'_>) -> SkipEligibility {
    let recently_addressed = is_recently_addressed(
        options.events,
        options.responding_participant_id,
        options.responding_character,
    );

    let mut must_speak_reason: Option<MustSpeakReason> = None;

    if !qualifies_for_turn_skipping(options.participants) {
        // A one-on-one (or single-character) chat is out of scope entirely.
        must_speak_reason = Some(MustSpeakReason::NotMultiCharacter);
    } else if !options.turn_skipping_enabled {
        must_speak_reason = Some(MustSpeakReason::FeatureDisabled);
    } else if is_first_character_turn(options.events) {
        must_speak_reason = Some(MustSpeakReason::FirstCharacterTurn);
    } else if options.summoned {
        must_speak_reason = Some(MustSpeakReason::Summoned);
    } else {
        let skipped = find_skipped_since_last_substantive(options.events);
        if skipped.contains(options.responding_participant_id) {
            must_speak_reason = Some(MustSpeakReason::AlreadySkipped);
        } else {
            let other_active_characters: Vec<&Value> = options
                .participants
                .iter()
                .filter(|p| {
                    str_field(p, "type") == Some("CHARACTER")
                        && participant_is_present(p)
                        && truthy_character_id(p)
                        && str_field(p, "id") != Some(options.responding_participant_id)
                })
                .collect();
            let all_others_skipped = other_active_characters
                .iter()
                .all(|p| str_field(p, "id").is_some_and(|id| skipped.contains(id)));
            if all_others_skipped {
                must_speak_reason = Some(MustSpeakReason::AllOthersSkipped);
            }
        }
    }

    SkipEligibility {
        offer_skip: must_speak_reason.is_none(),
        must_speak_reason,
        recently_addressed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn sentinel_line_shedding() {
        assert_eq!(
            detect_skip_sentinel("[NOTHING TO ADD]", None, None),
            DetectSkipResult::Skip
        );
        assert_eq!(
            detect_skip_sentinel("**[NOTHING TO ADD]**", None, None),
            DetectSkipResult::Skip
        );
        assert_eq!(
            detect_skip_sentinel("\"_[nothing to add]_\"", None, None),
            DetectSkipResult::Skip
        );
        assert_eq!(
            detect_skip_sentinel("nothing to add.", None, None),
            DetectSkipResult::Skip
        );
        assert_eq!(
            detect_skip_sentinel("Something to add!", None, None),
            DetectSkipResult::NoSkip { cleaned: None }
        );
    }

    #[test]
    fn sentinel_with_prose_cleans() {
        let got = detect_skip_sentinel("[NOTHING TO ADD]\nActually, one thing.", None, None);
        assert_eq!(
            got,
            DetectSkipResult::NoSkip {
                cleaned: Some("Actually, one thing.".to_string())
            }
        );
    }

    #[test]
    fn narration_then_trailing_sentinel_strips_the_last_line() {
        // Weak models narrate a genuine turn, then dangle a lone sentinel line.
        // The prose is kept; the trailing sentinel line is stripped.
        let got = detect_skip_sentinel(
            "*I stay where I am, my hand on his arm.*\n\n*There is nothing I need to add.*\n\n[NOTHING TO ADD]",
            None,
            None,
        );
        assert_eq!(
            got,
            DetectSkipResult::NoSkip {
                cleaned: Some(
                    "*I stay where I am, my hand on his arm.*\n\n*There is nothing I need to add.*"
                        .to_string()
                )
            }
        );
    }

    #[test]
    fn trailing_sentinel_stripped_even_when_markdown_wrapped() {
        // `is_sentinel_line` sheds the markdown wrappers, so a wrapped trailing
        // sentinel is stripped too.
        let got = detect_skip_sentinel(
            "*She nods once and says nothing more.*\n\n**[nothing to add]**",
            None,
            None,
        );
        assert_eq!(
            got,
            DetectSkipResult::NoSkip {
                cleaned: Some("*She nods once and says nothing more.*".to_string())
            }
        );
    }

    #[test]
    fn mid_line_sentinel_phrase_is_not_stripped() {
        // The phrase mid-line (not a lone final line) is left alone.
        assert_eq!(
            detect_skip_sentinel(
                "There is nothing to add here, but I will speak anyway.",
                None,
                None,
            ),
            DetectSkipResult::NoSkip { cleaned: None }
        );
    }

    #[test]
    fn sentinel_not_on_final_line_is_not_stripped() {
        // A lone sentinel line between two prose lines is NOT the final
        // non-empty line, so nothing is stripped.
        assert_eq!(
            detect_skip_sentinel("Well.\n\n[NOTHING TO ADD]\n\nMore.", None, None),
            DetectSkipResult::NoSkip { cleaned: None }
        );
    }

    #[test]
    fn no_name_guard_keeps_the_sentinel_brackets() {
        // With no name/aliases, the generic bracketed-name pattern must NOT run
        // (it would eat `[NOTHING TO ADD]`'s own brackets before detection —
        // harmless — but v4's guard skips the strip entirely; both paths must
        // still detect the bare sentinel).
        assert_eq!(
            detect_skip_sentinel("[NOTHING TO ADD]", None, None),
            DetectSkipResult::Skip
        );
        // A name prefix IS stripped when the name is known.
        assert_eq!(
            detect_skip_sentinel("[Ada]\n[NOTHING TO ADD]", Some("Ada"), None),
            DetectSkipResult::Skip
        );
    }

    #[test]
    fn turn_pass_guard_shape() {
        let good = json!({
            "type": "message", "systemSender": "host", "systemKind": "turn-pass",
            "hostEvent": { "participantId": "p1" }
        });
        assert!(is_turn_pass_message(&good));
        assert_eq!(turn_pass_participant_id(&good), Some("p1"));
        let bad = json!({
            "type": "message", "systemSender": "host", "systemKind": "turn-pass",
            "hostEvent": { "participantId": 7 }
        });
        assert!(!is_turn_pass_message(&bad));
    }

    #[test]
    fn stall_guard_vacuous_every() {
        // Sole active character, feature on, not first turn, not summoned →
        // all-others-skipped (the `.every()` over zero others is vacuously true).
        let participants = vec![json!({
            "id": "p1", "type": "CHARACTER", "status": "active",
            "characterId": "c1", "controlledBy": "llm"
        })];
        // Doesn't qualify (one active char, one LLM) → not-multi-character wins
        // first; use three chars with two removed?? Simpler: two LLM chars, one
        // responding, the other has a pass since last substantive.
        let participants2 = vec![
            json!({ "id": "p1", "type": "CHARACTER", "status": "active", "characterId": "c1", "controlledBy": "llm" }),
            json!({ "id": "p2", "type": "CHARACTER", "status": "active", "characterId": "c2", "controlledBy": "llm" }),
        ];
        let events = vec![
            json!({ "type": "message", "role": "ASSISTANT", "participantId": "p2", "content": "hi" }),
            json!({ "type": "message", "role": "ASSISTANT", "systemSender": "host",
                    "systemKind": "turn-pass", "participantId": null, "content": "note",
                    "hostEvent": { "participantId": "p2" } }),
        ];
        let rc = RespondingCharacter {
            id: "c1".into(),
            name: "Ada".into(),
            aliases: vec![],
        };
        let el = compute_skip_eligibility(&ComputeSkipEligibilityOptions {
            events: &events,
            participants: &participants2,
            responding_participant_id: "p1",
            responding_character: &rc,
            summoned: false,
            turn_skipping_enabled: true,
        });
        assert_eq!(
            el.must_speak_reason,
            Some(MustSpeakReason::AllOthersSkipped)
        );
        assert!(!el.offer_skip);
        // And the not-multi-character precedence for the singleton.
        let el2 = compute_skip_eligibility(&ComputeSkipEligibilityOptions {
            events: &events,
            participants: &participants,
            responding_participant_id: "p1",
            responding_character: &rc,
            summoned: false,
            turn_skipping_enabled: true,
        });
        assert_eq!(
            el2.must_speak_reason,
            Some(MustSpeakReason::NotMultiCharacter)
        );
    }
}
