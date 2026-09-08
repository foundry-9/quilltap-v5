//! Character progressions — the engine (v4 `lib/progressions/engine.ts`, 506
//! lines at `25f534c0b`).
//!
//! Everything else in the feature is plumbing around the five functions here:
//! parse a character's `metadata.progressions`, derive one progression's state
//! against a clock, decide whether this turn should mention it, render the line
//! the character reads, and flatten the lot into the primitive sheet Pascal's
//! comparators already know how to read.
//!
//! ## The clock is the wall clock
//!
//! `now_ms` is `Date.now()`, injected everywhere. Deliberately NOT the chat's
//! fictional timestamp: a progression belongs to a character across every chat
//! they sit in, while fictional time is per chat, so a story clock would give
//! the same pregnancy two different ages in two rooms.
//!
//! ## Month and year are fixed-length
//!
//! 30.436875 and 365.2425 days — the mean Gregorian month and year. Elapsed
//! time is then deterministic and calendar-free.
//!
//! ## The `{{start}}` / `{{end}}` formatting seam (MEASURED)
//!
//! v4 renders those two through `Intl.DateTimeFormat('en-US', { dateStyle:
//! 'medium', timeStyle: 'short' })`. Measured on Node 24.13.1 / ICU 78.2
//! (2026-09-08): `Sep 8, 2026, 2:02 PM` and, date-only, `Sep 8, 2026` — day and
//! 12-hour hour with **no** leading zero, minutes two-digit, and the separator
//! before `AM`/`PM` is **U+0020, not the U+202F narrow no-break space** the
//! order predicted. That prediction is REFUTED by measurement; the corpus pins
//! the bytes.
//!
//! An unresolvable timezone falls back to the host's in v4. v5 pins that
//! fallback to **UTC** and the family's oracle runs under `TZ=UTC` — the same
//! documented harness seam `context_feeders_leaves_equivalence` and
//! `mail_carina_tools_equivalence` already keep for v4's system-TZ
//! `toLocaleDateString`. It keeps the port deterministic instead of
//! reading the build machine's clock settings.
//!
//! CLIENT-SAFE in v4's sense: pure, no DB, no tracing, no I/O. The one logging
//! reader is P4.D168's `prompt_section.rs`, which passes an `on_issue` sink
//! down.

use serde_json::{Map, Value};

use crate::jsnum::to_fixed;

use super::schema::{
    is_progression_id, join_issues, parse_iso_instant_str, parse_progression, OnComplete,
    Progression, TimeIncrement, MAX_PROGRESSIONS_PER_CHARACTER, PROGRESSIONS_METADATA_KEY,
};

/// Milliseconds in one of each increment, indexed by [`TimeIncrement::ALL`] —
/// v4's `UNIT_MS` record, which Rust cannot spell as a keyed const. Month and
/// year are the mean Gregorian lengths (30.436875 and 365.2425 days), so
/// elapsed time is deterministic and calendar-free. Read it through
/// [`unit_ms`].
pub const UNIT_MS: [i64; 7] = [
    1_000,          // second
    60_000,         // minute
    3_600_000,      // hour
    86_400_000,     // day
    604_800_000,    // week
    2_629_746_000,  // month — 30.436875 days
    31_556_952_000, // year  — 365.2425 days
];

/// Milliseconds in one of `unit` — [`UNIT_MS`] read by name.
pub fn unit_ms(unit: TimeIncrement) -> i64 {
    UNIT_MS[unit as usize]
}

/// The next finer unit a remainder is spoken in. `second` has none (v4
/// `FINER_UNIT`).
fn finer_unit(unit: TimeIncrement) -> Option<TimeIncrement> {
    match unit {
        TimeIncrement::Year => Some(TimeIncrement::Month),
        TimeIncrement::Month => Some(TimeIncrement::Day),
        TimeIncrement::Week => Some(TimeIncrement::Day),
        TimeIncrement::Day => Some(TimeIncrement::Hour),
        TimeIncrement::Hour => Some(TimeIncrement::Minute),
        TimeIncrement::Minute => Some(TimeIncrement::Second),
        TimeIncrement::Second => None,
    }
}

/// The period suffixes the `<n><unit>` cadence grammar accepts (v4
/// `PERIOD_UNIT_MS`).
fn period_unit_ms(suffix: u8) -> Option<i64> {
    Some(match suffix {
        b's' => unit_ms(TimeIncrement::Second),
        b'm' => unit_ms(TimeIncrement::Minute),
        b'h' => unit_ms(TimeIncrement::Hour),
        b'd' => unit_ms(TimeIncrement::Day),
        b'w' => unit_ms(TimeIncrement::Week),
        _ => return None,
    })
}

/// Where a progression sits relative to its own span at a given instant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgressionState {
    Pending,
    Active,
    Complete,
}

impl ProgressionState {
    pub fn as_str(self) -> &'static str {
        match self {
            ProgressionState::Pending => "pending",
            ProgressionState::Active => "active",
            ProgressionState::Complete => "complete",
        }
    }
}

/// Everything computed from one progression and one instant (v4
/// `DerivedProgression`).
///
/// `start_ms` / `end_ms` are `Option` where v4 holds a JS number that may be
/// `NaN`: an entry can only reach here through the schema, which refuses an
/// unparseable instant, but the engine is total in v4 and stays total here.
#[derive(Debug, Clone, PartialEq)]
pub struct DerivedProgression {
    pub id: String,
    pub name: String,
    pub start_ms: Option<i64>,
    pub end_ms: Option<i64>,
    pub now_ms: i64,
    /// `clamp(now − start, 0, end − start)` — what has actually run.
    pub elapsed_ms: f64,
    /// `end − now`, negative when overdue. Uncapped.
    pub remaining_ms: f64,
    /// `(now − start) / (end − start) × 100`, UNCAPPED — this is what Pascal
    /// sees.
    pub percent: f64,
    /// The same, clamped to 0–100, for display.
    pub percent_clamped: f64,
    pub state: ProgressionState,
    pub started: bool,
    pub complete: bool,
    /// `total × percent_clamped / 100`, absent when the progression carries no
    /// quantity.
    pub quantity_current: Option<f64>,
    pub elapsed: String,
    pub elapsed_whole: String,
    pub remaining: String,
    pub remaining_whole: String,
}

/// Why [`should_report_progression`] decided the way it did. Logged verbatim.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportReason {
    First,
    Updated,
    Transition,
    Silenced,
    Turn,
    Increment,
    Period,
    Skip,
}

impl ReportReason {
    pub fn as_str(self) -> &'static str {
        match self {
            ReportReason::First => "first",
            ReportReason::Updated => "updated",
            ReportReason::Transition => "transition",
            ReportReason::Silenced => "silenced",
            ReportReason::Turn => "turn",
            ReportReason::Increment => "increment",
            ReportReason::Period => "period",
            ReportReason::Skip => "skip",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShouldReportResult {
    pub report: bool,
    pub reason: ReportReason,
}

/// Read the reserved `progressions` key off a character's metadata (v4
/// `parseProgressions`).
///
/// Total and fail-soft: a missing key, a key holding something other than an
/// object, an entry with a malformed id or a body the schema refuses — all of
/// those drop quietly through `on_issue` and the survivors are returned, in
/// `Object.entries` order. A broken entry must never hollow the character or
/// fail a turn, so this never errors.
///
/// Over-count is trimmed rather than rejected wholesale: a 33rd progression
/// should cost the user their 33rd progression, not their other 32. **The
/// ceiling is checked BEFORE the schema parse**, so a 33rd entry gets the
/// ceiling sentence whether or not its body is valid.
pub fn parse_progressions(
    metadata: Option<&Value>,
    on_issue: &mut dyn FnMut(&str, &str),
) -> Vec<(String, Progression)> {
    let Some(Value::Object(metadata)) = metadata else {
        return Vec::new();
    };

    let raw = match metadata.get(PROGRESSIONS_METADATA_KEY) {
        Some(Value::Object(map)) => map,
        Some(_) => {
            on_issue("*", "the progressions key does not hold an object");
            return Vec::new();
        }
        // Absent: v4's `raw !== undefined` guard means silence, not an issue.
        None => return Vec::new(),
    };

    let mut parsed: Vec<(String, Progression)> = Vec::new();
    let mut kept = 0usize;

    for (id, entry) in raw {
        if !is_progression_id(id) {
            on_issue(
                id,
                "the id is not a lowercase identifier (a-z, 0-9, _ and -, starting with a letter)",
            );
            continue;
        }
        if kept >= MAX_PROGRESSIONS_PER_CHARACTER {
            on_issue(
                id,
                &format!("over the ceiling of {MAX_PROGRESSIONS_PER_CHARACTER} progressions per character"),
            );
            continue;
        }
        match parse_progression(entry) {
            Err(issues) => {
                on_issue(id, &join_issues(&issues));
                continue;
            }
            Ok(progression) => {
                parsed.push((id.clone(), progression));
                kept += 1;
            }
        }
    }

    parsed
}

/// en-US abbreviated month names (`Intl` `dateStyle: 'medium'`). Duplicated
/// from `format_time::MONTHS_SHORT`, which is private and belongs to another
/// lane's file under this round's ownership fence — a three-word table is a
/// cheaper duplication than a cross-lane edit.
const MONTHS_SHORT: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// JS `${n}` for a value `Math.floor` produced from a finite magnitude — always
/// an integer, so JS never prints an exponent or a decimal point.
fn js_integer_string(x: f64) -> String {
    if x.abs() < 9_007_199_254_740_992.0 {
        format!("{}", x as i64)
    } else {
        format!("{x}")
    }
}

/// `${count} ${unit}${count === 1 ? '' : 's'}` (v4 `plural`).
fn plural(count: f64, unit: TimeIncrement) -> String {
    let suffix = if count == 1.0 { "" } else { "s" };
    format!("{} {}{suffix}", js_integer_string(count), unit.as_str())
}

/// Format a span as whole units of `unit` plus a remainder in the next finer
/// one — "20 weeks, 3 days", "2 minutes, 10 seconds", "45 seconds" (v4
/// `formatSpan`).
///
/// The single home of that rule. Negative input never reaches it (callers pass
/// magnitudes) and is defended against anyway by taking the absolute value. A
/// span shorter than one whole unit renders as "0 <units>" only when there is
/// no finer unit to spend it in — otherwise "0 minutes, 12 seconds" would be
/// noise where "12 seconds" is the answer.
pub fn format_span(ms: f64, unit: TimeIncrement) -> String {
    let magnitude = if ms.is_finite() { ms.abs() } else { 0.0 };
    let unit_len = unit_ms(unit) as f64;
    let whole = (magnitude / unit_len).floor();

    let Some(finer) = finer_unit(unit) else {
        return plural((magnitude / unit_len).floor(), unit);
    };

    let remainder_ms = magnitude - whole * unit_len;
    let finer_whole = (remainder_ms / unit_ms(finer) as f64).floor();

    if whole == 0.0 && finer_whole == 0.0 {
        return plural(0.0, unit);
    }
    if whole == 0.0 {
        return plural(finer_whole, finer);
    }
    if finer_whole == 0.0 {
        return plural(whole, unit);
    }
    format!("{}, {}", plural(whole, unit), plural(finer_whole, finer))
}

/// Whole units of `unit` only — what `{{elapsedWhole}}` and `{{remainingWhole}}`
/// render (v4 `formatSpanWhole`).
pub fn format_span_whole(ms: f64, unit: TimeIncrement) -> String {
    let magnitude = if ms.is_finite() { ms.abs() } else { 0.0 };
    plural((magnitude / unit_ms(unit) as f64).floor(), unit)
}

/// Derive one progression's state at an instant (v4 `deriveProgression`).
///
/// `percent` is uncapped on purpose — Pascal tests it, and "118% of the way to
/// a due date" is a fact a tool may reasonably branch on. Display goes through
/// `percent_clamped`. Before the start the spans mean something different from
/// during: `elapsed` is 0 and `remaining` counts down to the START; after the
/// end, `elapsed` counts up from the END, which is what "3 days past due" needs.
pub fn derive_progression(id: &str, p: &Progression, now_ms: i64) -> DerivedProgression {
    // v4 holds these as JS numbers that may be `NaN`; `None` is that `NaN`, and
    // every comparison below is written to fall the way a `NaN` comparison does.
    let start = parse_iso_instant_str(&p.start_time);
    let end = parse_iso_instant_str(&p.end_time);
    let start_f = start.map(|v| v as f64).unwrap_or(f64::NAN);
    let end_f = end.map(|v| v as f64).unwrap_or(f64::NAN);
    let now_f = now_ms as f64;
    let span_ms = end_f - start_f;

    let started = now_f >= start_f;
    let complete = now_f >= end_f;
    let state = if complete {
        ProgressionState::Complete
    } else if started {
        ProgressionState::Active
    } else {
        ProgressionState::Pending
    };

    // JS `Math.min(Math.max(x, 0), span)`: `Math.max` returns NaN if either is
    // NaN, and so does `Math.min` — reproduced by `js_min`/`js_max`.
    let elapsed_ms = js_min(js_max(now_f - start_f, 0.0), span_ms);
    let remaining_ms = end_f - now_f;

    let percent = if span_ms > 0.0 {
        ((now_f - start_f) / span_ms) * 100.0
    } else {
        100.0
    };
    let percent_clamped = js_min(js_max(percent, 0.0), 100.0);

    // What the rendered spans actually count, which differs by state: before the
    // start there is nothing elapsed and the countdown is to the start; after the
    // end the "elapsed" a report wants is the overrun.
    let elapsed_for_display = if state == ProgressionState::Complete {
        now_f - end_f
    } else {
        elapsed_ms
    };
    let remaining_for_display = if state == ProgressionState::Pending {
        start_f - now_f
    } else {
        js_max(remaining_ms, 0.0)
    };

    let unit = p.time_increment;

    DerivedProgression {
        id: id.to_string(),
        name: p.name.clone(),
        start_ms: start,
        end_ms: end,
        now_ms,
        elapsed_ms,
        remaining_ms,
        percent,
        percent_clamped,
        state,
        started,
        complete,
        quantity_current: p
            .quantity
            .as_ref()
            .map(|q| (q.total * percent_clamped) / 100.0),
        elapsed: format_span(elapsed_for_display, unit),
        elapsed_whole: format_span_whole(elapsed_for_display, unit),
        remaining: format_span(remaining_for_display, unit),
        remaining_whole: format_span_whole(remaining_for_display, unit),
    }
}

/// JS `Math.max(a, b)` — NaN-propagating, unlike Rust's `f64::max`.
fn js_max(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a > b {
        a
    } else {
        b
    }
}

/// JS `Math.min(a, b)` — NaN-propagating, unlike Rust's `f64::min`.
fn js_min(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a < b {
        a
    } else {
        b
    }
}

/// The state a progression was in at an arbitrary earlier instant (v4
/// `stateAt`).
fn state_at(p: &Progression, at_ms: f64) -> ProgressionState {
    let start = parse_iso_instant_str(&p.start_time)
        .map(|v| v as f64)
        .unwrap_or(f64::NAN);
    let end = parse_iso_instant_str(&p.end_time)
        .map(|v| v as f64)
        .unwrap_or(f64::NAN);
    if at_ms >= end {
        ProgressionState::Complete
    } else if at_ms >= start {
        ProgressionState::Active
    } else {
        ProgressionState::Pending
    }
}

/// Elapsed-into-span at an arbitrary instant, clamped the way `elapsed_ms` is
/// (v4 `elapsedAt`).
fn elapsed_at(p: &Progression, at_ms: f64) -> f64 {
    let start = parse_iso_instant_str(&p.start_time)
        .map(|v| v as f64)
        .unwrap_or(f64::NAN);
    let end = parse_iso_instant_str(&p.end_time)
        .map(|v| v as f64)
        .unwrap_or(f64::NAN);
    js_min(js_max(at_ms - start, 0.0), end - start)
}

/// `<n><unit>` → period length in ms, or `None` when the string isn't one (v4
/// `parseReportPeriodMs`, `/^([1-9]\d{0,4})([smhdw])$/`).
pub fn parse_report_period_ms(report_frequency: &str) -> Option<i64> {
    let b = report_frequency.as_bytes();
    if b.len() < 2 || b.len() > 6 {
        return None;
    }
    if !(b'1'..=b'9').contains(&b[0]) {
        return None;
    }
    if !b[1..b.len() - 1].iter().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let count: i64 = report_frequency[..b.len() - 1].parse().ok()?;
    Some(count * period_unit_ms(b[b.len() - 1])?)
}

/// Decide whether THIS turn mentions this progression (v4
/// `shouldReportProgression`).
///
/// The one input the caller supplies is `last_turn_ms`: the `createdAt` of the
/// responding character's own most recent visible assistant message in this
/// chat, or `None` if they have never spoken. Cadence is derived from history
/// rather than stored — a stored "last reported at" would be a write on every
/// prompted turn for a `turn`-cadence progression, would race the
/// participant-JSON whole-column replace, and could not be read back within a
/// single job in the forked child.
///
/// The known approximation, documented in the help page: a character who is
/// prompted but does not speak has no new own message, so a period-cadence
/// progression can be mentioned again on their next prompt inside the same
/// period. That is over-reporting by at most one turn per silent turn, and it is
/// the honest trade for zero writes.
///
/// Rules in order; the first that applies wins.
pub fn should_report_progression(
    p: &Progression,
    d: &DerivedProgression,
    last_turn_ms: Option<i64>,
) -> ShouldReportResult {
    // 1. Never spoken here — say everything once, so an opener knows what she
    //    carries. (v4 also takes the `!Number.isFinite` arm; a `None` here is
    //    both of v4's `null` and its NaN.)
    let Some(last_turn_ms) = last_turn_ms else {
        return ShouldReportResult {
            report: true,
            reason: ReportReason::First,
        };
    };
    let last_turn = last_turn_ms as f64;

    // 2. A writer Quilltap controls touched this since the character last spoke.
    //    A re-armed cannon is announced immediately, cadence notwithstanding.
    //    An unparseable `updatedAt` is `NaN` in v4 and never fires this.
    if let Some(updated_at) = p.updated_at.as_deref().and_then(parse_iso_instant_str) {
        if (updated_at as f64) > last_turn {
            return ShouldReportResult {
                report: true,
                reason: ReportReason::Updated,
            };
        }
    }

    // 3. The span crossed one of its own boundaries since then. This is what
    //    makes `onComplete: 'once'` work without storing a flag: the completion
    //    turn is the one where the state flipped.
    if state_at(p, last_turn) != d.state {
        return ShouldReportResult {
            report: true,
            reason: ReportReason::Transition,
        };
    }

    // 4. Finished, already announced, and asked to go quiet afterwards.
    if d.state == ProgressionState::Complete && p.on_complete == OnComplete::Once {
        return ShouldReportResult {
            report: false,
            reason: ReportReason::Silenced,
        };
    }

    if p.report_frequency == "turn" {
        return ShouldReportResult {
            report: true,
            reason: ReportReason::Turn,
        };
    }

    if p.report_frequency == "increment" {
        let unit_len = unit_ms(p.time_increment) as f64;
        let ticked = (elapsed_at(p, d.now_ms as f64) / unit_len).floor()
            != (elapsed_at(p, last_turn) / unit_len).floor();
        return if ticked {
            ShouldReportResult {
                report: true,
                reason: ReportReason::Increment,
            }
        } else {
            ShouldReportResult {
                report: false,
                reason: ReportReason::Skip,
            }
        };
    }

    let Some(period_ms) = parse_report_period_ms(&p.report_frequency) else {
        // The schema's regex admits nothing else, so this is unreachable from a
        // parsed progression; treat an impossible cadence as "every turn" rather
        // than silently muting a condition the user authored.
        return ShouldReportResult {
            report: true,
            reason: ReportReason::Turn,
        };
    };

    // Epoch-anchored wall-clock buckets: `1h` means at most once per clock hour,
    // not "an hour since the last mention".
    let period = period_ms as f64;
    let bucketed = (d.now_ms as f64 / period).floor() != (last_turn / period).floor();
    ShouldReportResult {
        report: bucketed,
        reason: if bucketed {
            ReportReason::Period
        } else {
            ReportReason::Skip
        },
    }
}

/// v4 `RenderProgressionOptions` — the chat's resolved timezone, for
/// `{{start}}` / `{{end}}`. `None` = the host's (pinned to UTC here; see the
/// module doc).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RenderProgressionOptions<'a> {
    pub timezone: Option<&'a str>,
}

/// Render the line the character reads (v4 `renderProgressionReport`).
///
/// `report_template` overrides the IN-PROGRESS wording only: the other two
/// states are short, fixed and structurally different, and an author's
/// in-progress sentence would read as nonsense in either.
pub fn render_progression_report(
    p: &Progression,
    d: &DerivedProgression,
    opts: &RenderProgressionOptions<'_>,
) -> String {
    let values = progression_placeholders(p, d, opts);

    if d.state == ProgressionState::Pending {
        return format!("{}: begins in {}.", p.name, d.remaining);
    }
    if d.state == ProgressionState::Complete {
        return format!("{}: complete; {} since it finished.", p.name, d.elapsed);
    }

    let template = p
        .report_template
        .clone()
        .unwrap_or_else(|| default_in_progress_template(p));
    substitute(&template, &values)
}

/// The default in-progress wording, composed from the flags so a plain entry
/// reads well with nothing else set (v4 `defaultInProgressTemplate`).
pub fn default_in_progress_template(p: &Progression) -> String {
    let mut parts = vec!["{{name}}: {{elapsed}} elapsed, {{remaining}} remaining".to_string()];
    if p.percentage_report {
        parts.push(", {{percent}}% complete".to_string());
    }
    if p.quantity.is_some() {
        parts.push(" ({{quantity}})".to_string());
    }
    let sentence = format!("{}.", parts.join(""));
    match &p.description {
        // v4 tests `p.description ? …` — JS truthiness, so an EMPTY description
        // is not prefixed. (The schema admits `""`: `.max(500)` with no `.min`.)
        Some(description) if !description.is_empty() => format!("{description} {sentence}"),
        _ => sentence,
    }
}

/// Every placeholder a report template may name, already rendered to text (v4
/// `progressionPlaceholders`). Insertion order is v4's declaration order.
pub fn progression_placeholders(
    p: &Progression,
    d: &DerivedProgression,
    opts: &RenderProgressionOptions<'_>,
) -> Vec<(String, String)> {
    let date_only = matches!(
        p.time_increment,
        TimeIncrement::Day | TimeIncrement::Week | TimeIncrement::Month | TimeIncrement::Year
    );
    vec![
        ("name".to_string(), p.name.clone()),
        (
            "description".to_string(),
            p.description.clone().unwrap_or_default(),
        ),
        ("elapsed".to_string(), d.elapsed.clone()),
        ("elapsedWhole".to_string(), d.elapsed_whole.clone()),
        ("remaining".to_string(), d.remaining.clone()),
        ("remainingWhole".to_string(), d.remaining_whole.clone()),
        (
            "percent".to_string(),
            js_integer_string(js_round(d.percent_clamped)),
        ),
        (
            "quantity".to_string(),
            match &p.quantity {
                Some(q) => format!(
                    "{}/{} {}",
                    to_fixed(d.quantity_current.unwrap_or(0.0), q.precision),
                    to_fixed(q.total, q.precision),
                    q.unit
                ),
                None => String::new(),
            },
        ),
        (
            "start".to_string(),
            format_instant_en_us(d.start_ms, date_only, opts.timezone),
        ),
        (
            "end".to_string(),
            format_instant_en_us(d.end_ms, date_only, opts.timezone),
        ),
        (
            "increment".to_string(),
            p.time_increment.as_str().to_string(),
        ),
    ]
}

/// JS `Math.round` — half toward **+Infinity**, not Rust's half-away-from-zero.
/// `percent_clamped` is always in `[0, 100]` so the two agree there; the
/// difference is written down because the input is only clamped by convention.
fn js_round(x: f64) -> f64 {
    if !x.is_finite() {
        return x;
    }
    (x + 0.5).floor()
}

/// Plain scan-and-replace; an unknown placeholder stays exactly as written (v4
/// `substitute`, `/\{\{([^}]+)\}\}/g` with the key `trim()`ed).
fn substitute(template: &str, values: &[(String, String)]) -> String {
    let bytes = template.as_bytes();
    let mut out = String::with_capacity(template.len());
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == b'{' && bytes.get(i + 1) == Some(&b'{') {
            // `[^}]+` — one or more non-`}` characters, then `}}`.
            if let Some(close) = template[i + 2..].find('}') {
                let key_end = i + 2 + close;
                let raw_key = &template[i + 2..key_end];
                if !raw_key.is_empty() && template.as_bytes().get(key_end + 1) == Some(&b'}') {
                    let key = crate::jsstr::js_trim(raw_key);
                    match values.iter().find(|(k, _)| k == key) {
                        Some((_, v)) => out.push_str(v),
                        None => out.push_str(&template[i..key_end + 2]),
                    }
                    i = key_end + 2;
                    continue;
                }
            }
        }
        let ch_len = template[i..]
            .chars()
            .next()
            .map(char::len_utf8)
            .unwrap_or(1);
        out.push_str(&template[i..i + ch_len]);
        i += ch_len;
    }
    out
}

/// v4 `formatInstant` — `Intl.DateTimeFormat('en-US', { dateStyle: 'medium',
/// timeStyle: 'short' unless date-only, timeZone })`. See the module doc for the
/// measured bytes and the UTC host-fallback seam. A non-finite instant renders
/// as the empty string, as v4's `Number.isFinite` guard does.
fn format_instant_en_us(ms: Option<i64>, date_only: bool, timezone: Option<&str>) -> String {
    let Some(ms) = ms else {
        return String::new();
    };
    let Ok(timestamp) = jiff::Timestamp::from_millisecond(ms) else {
        return String::new();
    };
    // An unresolvable zone must not sink a turn; v4 falls back to the host's,
    // v5 to UTC (the documented harness seam — the oracle runs under TZ=UTC).
    let zone = timezone
        .and_then(|name| jiff::tz::TimeZone::get(name).ok())
        .unwrap_or(jiff::tz::TimeZone::UTC);
    let zoned = timestamp.to_zoned(zone);

    let month = MONTHS_SHORT[(zoned.month() as usize) - 1];
    let date = format!("{month} {}, {}", zoned.day(), zoned.year());
    if date_only {
        return date;
    }
    let hour24 = zoned.hour();
    let meridiem = if hour24 < 12 { "AM" } else { "PM" };
    let hour12 = match hour24 % 12 {
        0 => 12,
        h => h,
    };
    // The separator before AM/PM is U+0020 on Node 24.13.1 / ICU 78.2 — see the
    // module doc; it is measured, not assumed.
    format!("{date}, {hour12}:{:02} {meridiem}", zoned.minute())
}

/// The primitive value types a flattened progress sheet holds — Pascal's
/// comparator vocabulary (v4 `ProgressPrimitive`).
pub type ProgressPrimitive = Value;

/// Flatten every progression on a character into the sheet Pascal reads:
/// `"<id>.<field>"` → primitive (v4 `flattenProgressions`). All primitives, so
/// the existing fail-soft comparison table applies unchanged and an absent id or
/// field is simply a comparator that does not hold.
///
/// Times come out as **epoch milliseconds**, not ISO strings, so ordering
/// comparators and `{{now}} + 600000` arithmetic work on them.
pub fn flatten_progressions(
    metadata: Option<&Value>,
    now_ms: i64,
    on_issue: &mut dyn FnMut(&str, &str),
) -> Map<String, Value> {
    let mut sheet = Map::new();

    for (id, p) in parse_progressions(metadata, on_issue) {
        let d = derive_progression(&id, &p, now_ms);
        sheet.insert(format!("{id}.name"), Value::String(d.name.clone()));
        sheet.insert(format!("{id}.percent"), number(d.percent));
        sheet.insert(format!("{id}.elapsedMs"), number(d.elapsed_ms));
        sheet.insert(format!("{id}.remainingMs"), number(d.remaining_ms));
        sheet.insert(
            format!("{id}.startTime"),
            d.start_ms.map(Value::from).unwrap_or(Value::Null),
        );
        sheet.insert(
            format!("{id}.endTime"),
            d.end_ms.map(Value::from).unwrap_or(Value::Null),
        );
        sheet.insert(format!("{id}.started"), Value::Bool(d.started));
        sheet.insert(format!("{id}.complete"), Value::Bool(d.complete));
        sheet.insert(
            format!("{id}.state"),
            Value::String(d.state.as_str().to_string()),
        );
        sheet.insert(format!("{id}.elapsed"), Value::String(d.elapsed.clone()));
        sheet.insert(
            format!("{id}.remaining"),
            Value::String(d.remaining.clone()),
        );
        if let Some(current) = d.quantity_current {
            sheet.insert(format!("{id}.quantity"), number(current));
        }
    }

    sheet
}

/// A JS number as a `Value`, printed the way `JSON.stringify` prints one: an
/// integral value carries no decimal point (so it compares equal to the
/// oracle's integer), and a `NaN`/`Infinity` — only reachable from an entry
/// whose instants the schema would have refused — becomes `null`.
fn number(x: f64) -> Value {
    if !x.is_finite() {
        return Value::Null;
    }
    if x.fract() == 0.0 && x.abs() < 9_007_199_254_740_992.0 {
        // `-0` stringifies as `0` in JS, and `as i64` agrees.
        return Value::Number(serde_json::Number::from(x as i64));
    }
    serde_json::Number::from_f64(x)
        .map(Value::Number)
        .unwrap_or(Value::Null)
}

/// The increment a span of this length is best spoken in (v4 `inferIncrement`)
/// — used when a Pascal effect creates a progression nobody pre-authored and
/// there is no author to ask. Thresholds chosen so the whole-unit part of a
/// report is never zero and never absurd: a ten-minute recharge speaks in
/// minutes, a nine-month gestation in months.
pub fn infer_increment(span_ms: f64) -> TimeIncrement {
    let span = span_ms.abs();
    if span < 2.0 * unit_ms(TimeIncrement::Minute) as f64 {
        return TimeIncrement::Second;
    }
    if span < 2.0 * unit_ms(TimeIncrement::Hour) as f64 {
        return TimeIncrement::Minute;
    }
    if span < 2.0 * unit_ms(TimeIncrement::Day) as f64 {
        return TimeIncrement::Hour;
    }
    if span < 2.0 * unit_ms(TimeIncrement::Week) as f64 {
        return TimeIncrement::Day;
    }
    if span < 8.0 * unit_ms(TimeIncrement::Week) as f64 {
        return TimeIncrement::Week;
    }
    if span < 2.0 * unit_ms(TimeIncrement::Year) as f64 {
        return TimeIncrement::Month;
    }
    TimeIncrement::Year
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::progressions::schema::parse_progression;
    use serde_json::json;

    fn cannon(extra: Value) -> Progression {
        let mut entry = json!({
            "name": "Cannon recharge",
            "startTime": "2026-09-08T14:00:00Z",
            "endTime": "2026-09-08T14:10:00Z",
            "timeIncrement": "minute",
        });
        for (k, v) in extra.as_object().expect("an object of overrides") {
            entry[k] = v.clone();
        }
        parse_progression(&entry).expect("the fixture parses")
    }

    /// `TimeIncrement::ALL` is what indexes [`UNIT_MS`]; a reordered enum would
    /// silently give every unit the wrong length.
    #[test]
    fn unit_ms_table_is_indexed_by_the_enum_order() {
        for (i, unit) in TimeIncrement::ALL.into_iter().enumerate() {
            assert_eq!(unit_ms(unit), UNIT_MS[i], "{}", unit.as_str());
        }
        assert_eq!(unit_ms(TimeIncrement::Month), 2_629_746_000);
        assert_eq!(unit_ms(TimeIncrement::Year), 31_556_952_000);
    }

    /// The measured `Intl` bytes, pinned where the differential cannot reach
    /// without its oracle: a plain U+0020 before `AM`/`PM`, no leading zero on
    /// the day or the 12-hour hour, and date-only for a day-or-coarser unit.
    #[test]
    fn format_instant_matches_the_measured_intl_bytes() {
        let ms = 1_788_876_130_000; // 2026-09-08T14:02:10Z
        assert_eq!(
            format_instant_en_us(Some(ms), false, Some("UTC")),
            "Sep 8, 2026, 2:02\u{20}PM"
        );
        assert!(!format_instant_en_us(Some(ms), false, Some("UTC")).contains('\u{202F}'));
        assert_eq!(
            format_instant_en_us(Some(ms), false, Some("America/Chicago")),
            "Sep 8, 2026, 9:02 AM"
        );
        assert_eq!(
            format_instant_en_us(Some(ms), true, Some("UTC")),
            "Sep 8, 2026"
        );
        // Midnight is 12 AM, noon is 12 PM.
        assert_eq!(
            format_instant_en_us(Some(1_798_761_600_000), false, Some("UTC")),
            "Jan 1, 2027, 12:00 AM"
        );
        // An unresolvable zone falls back to UTC (the recorded harness seam).
        assert_eq!(
            format_instant_en_us(Some(ms), false, Some("Mars/Olympus_Mons")),
            format_instant_en_us(Some(ms), false, None)
        );
        assert_eq!(format_instant_en_us(None, false, Some("UTC")), "");
    }

    /// v4's `plural` singularises on `count === 1` only.
    #[test]
    fn format_span_pluralises_on_exactly_one() {
        assert_eq!(
            format_span(61_000.0, TimeIncrement::Minute),
            "1 minute, 1 second"
        );
        assert_eq!(format_span(120_000.0, TimeIncrement::Minute), "2 minutes");
        assert_eq!(format_span(0.0, TimeIncrement::Minute), "0 minutes");
        assert_eq!(format_span(999.0, TimeIncrement::Second), "0 seconds");
        assert_eq!(
            format_span(-3.0 * 3_600_000.0, TimeIncrement::Hour),
            "3 hours"
        );
        assert_eq!(format_span(f64::NAN, TimeIncrement::Minute), "0 minutes");
    }

    /// The default wording is composed from the flags, and an EMPTY description
    /// is not prefixed — v4 tests `p.description ?`, JS truthiness.
    #[test]
    fn default_template_follows_js_truthiness_on_the_description() {
        assert_eq!(
            default_in_progress_template(&cannon(json!({ "description": "" }))),
            "{{name}}: {{elapsed}} elapsed, {{remaining}} remaining, {{percent}}% complete."
        );
        assert_eq!(
            default_in_progress_template(&cannon(json!({ "description": "D" }))),
            "D {{name}}: {{elapsed}} elapsed, {{remaining}} remaining, {{percent}}% complete."
        );
    }

    /// An unknown placeholder stays verbatim; the key is trimmed.
    #[test]
    fn substitute_leaves_an_unknown_placeholder_alone() {
        let values = vec![("name".to_string(), "Cannon".to_string())];
        assert_eq!(
            substitute("{{ name }}/{{nope}}", &values),
            "Cannon/{{nope}}"
        );
        assert_eq!(substitute("{{}}", &values), "{{}}");
        assert_eq!(substitute("{{{name}}}", &values), "{{{name}}}");
    }
}
