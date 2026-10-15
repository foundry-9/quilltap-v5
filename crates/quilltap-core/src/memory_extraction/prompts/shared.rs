//! Blocks every extraction prompt shares — port of v4
//! `lib/memory/extraction/prompts/shared.ts` (pin `83334b9a5`): the
//! ORIENTING CONTEXT and CLOCK footers and the autonomous-room clause. The
//! EVENT / AGREEMENTS / TAGS instruction blocks and the skip bullet are
//! substituted into the generated bodies ([`super::prompt_text`]) — they
//! never appear in a prompt on their own.
//!
//! Changed in the span (bug 212 / `c2795733f`, Phase A / `e0ac7493a`): the
//! CLOCK renders its date and weekday on the MEMORY CALENDAR (the zone the
//! `when` resolver uses), and its `Current in-story time` line is gone (v4
//! removed `ExtractionClock.narrativeNow`).
//!
//! v5 shape: the clock's zone is REQUIRED. v4's `timeZone?` falls back to
//! `getMemoryTimeZone()` when absent, but at the pin every production clock
//! comes from `clockAt(ctx, …)`, which always stamps the context's zone, and
//! v4's `withMemoryTimeZone` (the one helper that fills an absent zone) has
//! no production caller (work order P4.D296 L-8 — not ported). Core never
//! reads an ambient zone (P4.D289 LR-12's principle): the caller passes the
//! memory-calendar zone it resolved, which is exactly what v4's fallback
//! would have read ([`crate::services::memory_time_zone`]).

use crate::jsstr::{js_trim, utf16_len, utf16_truncate};
use crate::memory_time::calendar::{zoned_date_stamp, zoned_parts};

/// Non-canonical orienting context fed into the extraction footer (v4
/// `OrientingContext`). Project description and the rolling chat summary
/// help the model judge a memory's temporal frame, scope, and context tag —
/// they are NEVER a source of memories themselves. Held in the variable
/// footer (never the cached body prefix) so cheap-LLM prefix caching still
/// hits across a long extraction run.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OrientingContext {
    pub project_description: Option<String>,
    pub chat_context_summary: Option<String>,
}

/// Char budget per orienting line — UTF-16 code units, not tokens (no
/// tokenizer on the cheap path).
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
/// no-context form). Placed after the stable body and before the CONTEXT
/// block by the prompt builders.
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

/// The extractor's clock (episodic spine, v4 `ExtractionClock`). Rendered
/// into the variable CONTEXT footer — never the cached body prefix — so the
/// model can resolve "yesterday" / "last spring" while extracting, and so
/// `when` phrases come back already anchorable.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExtractionClock {
    /// ISO wall-clock timestamp of the source turn (the turn's message time).
    pub now_iso: String,
    /// Which clock the chat's story runs on: `"realtime" | "narrative"`.
    pub timeline_mode: String,
    /// The memory-calendar zone this clock is read in (bug 212). The caller
    /// resolves it once and hands the same value to the `when` resolver, so
    /// the date the model is shown and the date its phrases resolve against
    /// cannot disagree.
    pub time_zone: String,
}

const WEEKDAYS: [&str; 7] = [
    "Sunday",
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
];

/// `2026-07-14 (Tuesday), <nowIso>` — the clock's local date on the memory
/// calendar (v4 `renderClockStamp`). A `nowIso` `Date.parse` cannot read
/// keeps the raw string.
pub fn render_clock_stamp(clock: &ExtractionClock) -> String {
    let Some(ms) = crate::episodic::js_date_parse_ms(&clock.now_iso) else {
        return clock.now_iso.clone();
    };
    let ms = ms as f64;
    let weekday = WEEKDAYS[zoned_parts(ms, &clock.time_zone).weekday as usize];
    format!(
        "{} ({weekday}), {}",
        zoned_date_stamp(ms, &clock.time_zone),
        clock.now_iso
    )
}

/// Render the CLOCK footer block (v4 `renderClockBlock`); `""` when no clock
/// was supplied.
pub fn render_clock_block(clock: Option<&ExtractionClock>) -> String {
    let Some(clock) = clock else {
        return String::new();
    };
    let lines = [
        "CLOCK".to_string(),
        format!("Current date/time: {}", render_clock_stamp(clock)),
        format!("Timeline mode: {}", clock.timeline_mode),
        "Resolve relative time phrases against this clock; emit \"when\" as an absolute date (YYYY-MM-DD) whenever you can.".to_string(),
    ];
    format!("{}\n\n", lines.join("\n"))
}

/// 4.6 Private Character Rooms — user-absence clause for memory extraction
/// (v4 `AUTONOMOUS_ROOM_USER_ABSENCE_CLAUSE`). Prepended to the SELF and
/// OTHER prompts when the source chat is autonomous (`chatType =
/// 'autonomous'`). Constrains the extractor so the resulting memories don't
/// falsely imply the user was present, agreed to, or was informed of the
/// exchange. Memories about the participants' shared experience remain in
/// scope; user-attributed memories are out of scope.
pub const AUTONOMOUS_ROOM_USER_ABSENCE_CLAUSE: &str =
    "IMPORTANT: This exchange occurred in an autonomous character-to-character \
room with no user present. Do not produce memories that imply the user \
witnessed, agreed to, or was informed of any part of this exchange. \
Memories about the participants' shared experience are allowed; memories \
that name or address the user are not.\n\n";
