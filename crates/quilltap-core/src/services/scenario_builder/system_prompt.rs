//! Scenario Builder — the system prompt and the user message (v4
//! `lib/scenario-builder/system-prompt.ts`, `d1c06cd9d`).
//!
//! v4's own header: both are for a model, not a person, so they are plain and
//! short — no steampunk. The builder prompt never enters a chat's system
//! prompt; the scene it produces enters a chat exactly as a hand-typed custom
//! scenario does. Not user-editable in v1.
//!
//! Every literal below is transcribed from the file and pinned byte-for-byte
//! by `scenario_builder_prompts_equivalence` (v4's REAL builders, a fixed
//! instant, the zone set per row).
//!
//! **The clock is injected** (`now: &jiff::Zoned`): v4 passes `new Date()` and
//! renders it in the SERVER's local zone; the service passes
//! [`jiff::Zoned::now`] (the host's system zone) and the tier-1 family passes
//! a fixed instant in a named zone, so the zone → offset resolution is compared
//! too, not only the formatter.
//!
//! **A correction to the P4.D217 order (§R.4(d)), measured:** the order says
//! `buildScenarioBuilderUserMessage` tests `currentScenario?.trim()` for
//! truthiness but interpolates the UNTRIMMED value. It does not — v4 binds
//! `const currentScenario = input.currentScenario?.trim()` and interpolates
//! THAT local, exactly as it does `contextSummary`; the oracle's padded row
//! (`current-scenario-padded`) prints the trimmed scene. There is no asymmetry
//! to reproduce and no v4 filing candidate. The values that DO ride untrimmed
//! are `priorDraft` and `revision` (the revise pair — the schema already
//! trimmed `revision`, never `priorDraft`).

use jiff::Zoned;

use super::request_schema::ScenarioBuilderMode;

/// v4 `SCENARIO_TARGET_TOKENS` — the target length stated to the model. The
/// builder never truncates.
pub const SCENARIO_TARGET_TOKENS: u32 = 1000;

/// `SCENARIO_TARGET_TOKENS.toLocaleString('en-US')` — `1,000`. A const-sized
/// grouping of a const, rendered once rather than through a locale library.
fn target_tokens_en_us() -> String {
    let digits = SCENARIO_TARGET_TOKENS.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// v4 `buildScenarioBuilderSystemPrompt({ mode, webAvailable, toolInstructions,
/// now })` — sections joined by a blank line.
pub fn build_scenario_builder_system_prompt(
    mode: ScenarioBuilderMode,
    web_available: bool,
    tool_instructions: &str,
    now: &Zoned,
) -> String {
    let mut sections: Vec<String> = Vec::new();

    sections.push(
        "You are The Host of Quilltap, setting the opening scene for a conversation. Produce ONLY the scene, as Markdown, and deliver it by calling `submit_final_response`. No preamble, no notes, no sources, no title unless the scene itself wants one."
            .to_string(),
    );

    sections.push(
        "## The scene must leave the people out\n\nThis is non-negotiable. Never name, count, or describe the people who will be present. Do not use `{{char}}`, `{{user}}`, or any other placeholder. Write in the present tense, addressed to no one. Describe the place, the time, the weather, the light, the sounds, what is happening around, what has just happened, and what is about to. The stores you read may describe characters; use them for the world only, and leave the people out of the scene."
            .to_string(),
    );

    sections.push(format!(
        "## Length\n\nAim for {} tokens or fewer — a paragraph or two is usually right. If the details ask for a particular tone or length, follow them.",
        target_tokens_en_us()
    ));

    match mode {
        ScenarioBuilderMode::Real => {
            let mut lines = vec![
                "## Research: a real place",
                "",
                "The location is a real place. Use `search_web` and `curl` to establish what it is actually like: its geography, its period details for the given time, and anything the details ask for. The document stores are open to you too (`search` and the `doc_*` read tools) — check them for anything the user has already written about this place. Cite nothing; write the scene.",
            ];
            if !web_available {
                lines.push("");
                lines.push(
                    "The web is unavailable in this run. Rely on what you know; where you are unsure of a fact, keep the scene general rather than invent specifics.",
                );
            }
            sections.push(lines.join("\n"));
        }
        ScenarioBuilderMode::InWorld => {
            sections.push(
                "## Research: the user's world\n\nThe location is fictional and belongs to the user's world. Everything you need is in the document stores: use `search` (documents and knowledge) and the `doc_*` read tools to find the place, its history, its customs, and what the time means there. Do not invent lore that contradicts what you find; where the stores are silent, stay consistent with their tone. You have no access to the web."
                    .to_string(),
            );
        }
    }

    sections.push(format!(
        "## Now\n\nThe current date and time is {}. Use it as the reference for \"now\", \"tonight\", \"this morning\", and the like.",
        format_iso_with_offset(now)
    ));

    // `if (toolInstructions)` — JS truthiness: only the empty string is skipped.
    if !tool_instructions.is_empty() {
        sections.push(tool_instructions.to_string());
    }

    sections.join("\n\n")
}

/// v4 `ScenarioBuilderUserMessageInput`. `null` and absent are the same to
/// every reader here (`?.trim()` and `!= null`), so plain `Option`s.
#[derive(Debug, Clone)]
pub struct ScenarioBuilderUserMessageInput<'a> {
    pub mode: ScenarioBuilderMode,
    pub location: &'a str,
    pub time: &'a str,
    pub details: &'a str,
    /// In-chat only: the scene being replaced.
    pub current_scenario: Option<&'a str>,
    /// In-chat only: the chat's context summary.
    pub context_summary: Option<&'a str>,
    /// Revise: the draft as currently edited. Travels with `revision`.
    pub prior_draft: Option<&'a str>,
    /// Revise: the instruction. Travels with `prior_draft`.
    pub revision: Option<&'a str>,
}

/// v4 `buildScenarioBuilderUserMessage(input)` — blocks joined by a blank
/// line.
pub fn build_scenario_builder_user_message(input: &ScenarioBuilderUserMessageInput<'_>) -> String {
    use crate::jsstr::js_trim;

    let details = js_trim(input.details);
    let mode = input.mode.as_str();
    let mut blocks: Vec<String> = vec![[
        format!("Mode: {mode}"),
        format!("Location: {}", input.location),
        format!("Time: {}", input.time),
        format!(
            "Details: {}",
            if details.is_empty() {
                "(none)"
            } else {
                details
            }
        ),
    ]
    .join("\n")];

    // Both tested AND interpolated trimmed (see the module header — the order
    // said otherwise; the code and the oracle agree it is trimmed).
    let current_scenario = input.current_scenario.map(js_trim).unwrap_or("");
    let context_summary = input.context_summary.map(js_trim).unwrap_or("");
    if !current_scenario.is_empty() {
        blocks.push(format!(
            "Current scene (being replaced):\n{current_scenario}"
        ));
    }
    if !context_summary.is_empty() {
        blocks.push(format!(
            "Where the conversation stands:\n{context_summary}\n\n(This summary may name people. The new scene must not.)"
        ));
    }

    // `priorDraft != null && revision != null` — an EMPTY string travels.
    if let (Some(draft), Some(revision)) = (input.prior_draft, input.revision) {
        blocks.push(format!("Current draft:\n{draft}"));
        blocks.push(format!(
            "Revision requested:\n{revision}\n\nReturn the whole revised scene."
        ));
    }

    blocks.join("\n\n")
}

/// An instant in a named IANA zone — how a caller without its own `jiff`
/// dependency (the tier-1 harness) builds the `now` v4 gets from `new Date()`
/// under `TZ=<zone>`. `None` for an out-of-range instant or an unknown zone.
pub fn zoned_at(epoch_ms: i64, tz: &str) -> Option<Zoned> {
    jiff::Timestamp::from_millisecond(epoch_ms)
        .ok()?
        .in_tz(tz)
        .ok()
}

/// v4's private `formatIsoWithOffset(date)` — ISO-8601 in the zone's local
/// time with a `±HH:MM` offset, NEVER `Z` (UTC renders `+00:00`):
///
/// ```text
/// const pad = (n) => String(Math.abs(n)).padStart(2, '0')
/// const offsetMinutes = -date.getTimezoneOffset()
/// const sign = offsetMinutes >= 0 ? '+' : '-'
/// `${sign}${pad(Math.trunc(offsetMinutes / 60))}:${pad(offsetMinutes % 60)}`
/// ```
///
/// JS `%` keeps the dividend's sign and `pad` takes `Math.abs`, so a negative
/// half-hour offset (St John's, −150 minutes) prints `-02:30`. The year is not
/// padded (`getFullYear()` verbatim).
pub fn format_iso_with_offset(now: &Zoned) -> String {
    let pad = |n: i64| format!("{:02}", n.abs());
    // `getTimezoneOffset()` is whole minutes (sub-minute historical offsets
    // are truncated toward zero by the engine).
    let offset_minutes = i64::from(now.offset().seconds()) / 60;
    let sign = if offset_minutes >= 0 { '+' } else { '-' };
    format!(
        "{}-{}-{}T{}:{}:{}{sign}{}:{}",
        now.year(),
        pad(i64::from(now.month())),
        pad(i64::from(now.day())),
        pad(i64::from(now.hour())),
        pad(i64::from(now.minute())),
        pad(i64::from(now.second())),
        pad(offset_minutes / 60),
        pad(offset_minutes % 60),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn zoned(ms: i64, tz: &str) -> Zoned {
        jiff::Timestamp::from_millisecond(ms)
            .unwrap()
            .in_tz(tz)
            .unwrap()
    }

    #[test]
    fn utc_renders_plus_zero_never_z() {
        assert_eq!(
            format_iso_with_offset(&zoned(1_790_190_309_000, "UTC")),
            "2026-09-23T19:05:09+00:00"
        );
    }

    #[test]
    fn a_negative_half_hour_offset_keeps_its_minutes() {
        assert_eq!(
            format_iso_with_offset(&zoned(1_790_190_309_000, "America/St_Johns")),
            "2026-09-23T16:35:09-02:30"
        );
    }

    #[test]
    fn target_tokens_group_as_en_us() {
        assert_eq!(target_tokens_en_us(), "1,000");
    }
}
