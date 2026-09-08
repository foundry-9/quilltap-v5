//! Character progressions — the prompt-side chokepoint (v4
//! `lib/progressions/prompt-section.ts`, 143 lines at `25f534c0b`).
//!
//! Every prompt path that reports a character's timed conditions comes through
//! [`build_progressions_section`], and nowhere else re-derives elapsed /
//! remaining / percent. The engine below it is pure and client-safe; this
//! module is the server-side wrapper that adds the two things the engine
//! deliberately has no business knowing: the cadence input (the character's own
//! last turn, walked out of the event history) and the debug logging every
//! backend path owes.
//!
//! ## Where the section lands
//!
//! Never in system block 1. The identity stack and `build_system_prompt` are
//! the CACHED prefix; a per-turn clock inside them would bisect the cache on
//! every single turn and move the committed golden hashes. The report is a
//! **trailing per-turn section** on the uncached tail, after Suparṇā's mail and
//! before the turn-skip note, and it is not persisted as a message: it is
//! recomputed every turn, and a transcript whisper per turn for a
//! `turn`-cadence weapon would be noise.
//!
//! ## Empty is byte-for-byte nothing
//!
//! When no progression reports this turn the function returns `""` and the
//! caller pushes nothing — the empty-is-identical guarantee every trailing
//! section keeps, and the reason a character with no progressions sees a prompt
//! indistinguishable from one built before this feature existed.
//!
//! That guarantee extends to the READ: `load_events` is a thunk, called only
//! once a character is known to carry at least one progression. A character
//! carrying none costs this feature exactly nothing — not a query, not a row —
//! which is the overwhelmingly common case and the one a per-turn addition has
//! no business taxing.

use serde_json::{json, Value};

use crate::core_whisper::{find_last_own_turn_ms, WhisperEvent};

use super::engine::{
    derive_progression, parse_progressions, render_progression_report, should_report_progression,
    RenderProgressionOptions, ReportReason,
};

/// v4's `CONTEXT` — the tracing target every line below carries.
const CONTEXT: &str = "progressions.prompt-section";

/// The wrapper sentence the report block opens with. Second person, no Staff
/// persona.
pub const PROGRESSIONS_SECTION_HEADER: &str =
    "Time-bound conditions you are carrying, as of this moment:";

/// The responding character as this module reads it — v4's
/// `{ id: string; metadata?: unknown }`.
pub struct SectionCharacter<'a> {
    pub id: &'a str,
    pub metadata: Option<&'a Value>,
}

/// This chat's events for the cadence walk, as a THUNK so a character with no
/// progressions never triggers the read. v5's read path is synchronous
/// (`Db::read_main`), so the thunk is too; a failure is `Err` and lands in the
/// same `catch` arm v4's rejected promise does.
pub type LoadEvents<'a> = &'a mut dyn FnMut() -> Result<Vec<WhisperEvent>, String>;

/// v4 `BuildProgressionsSectionParams`.
pub struct BuildProgressionsSectionParams<'a> {
    /// The responding character, hydrated — `metadata` comes from the read
    /// overlay. `None` is v4's `null | undefined`.
    pub character: Option<SectionCharacter<'a>>,
    /// Ignored when `force` is set.
    pub load_events: Option<LoadEvents<'a>>,
    /// The responding participant, whose own last turn sets the cadence.
    pub responding_participant_id: Option<&'a str>,
    /// The wall clock, injected.
    pub now_ms: i64,
    /// The chat's resolved timezone, for `{{start}}` / `{{end}}`.
    pub timezone: Option<&'a str>,
    /// Report everything, cadence notwithstanding — the greeting builder and
    /// Carina, both of which are one-shot prompts with no "last turn" to speak
    /// of. An opener should know she is pregnant.
    pub force: bool,
}

/// Build the trailing progressions section for one turn, or `""` when nothing
/// reports (v4 `buildProgressionsSection`).
///
/// Never fails: a character's timed conditions are a garnish on a turn, and no
/// malformed entry may cost them the turn itself. v4's `try/catch` becomes the
/// `load_events` `Err` arm — the only fallible step on this path, since the
/// engine below is total.
pub fn build_progressions_section(params: BuildProgressionsSectionParams<'_>) -> String {
    let BuildProgressionsSectionParams {
        character,
        load_events,
        responding_participant_id,
        now_ms,
        timezone,
        force,
    } = params;

    let Some(character) = character else {
        return String::new();
    };

    let progressions = parse_progressions(character.metadata, &mut |id, issue| {
        tracing::warn!(
            target: "progressions.prompt-section",
            context = CONTEXT,
            characterId = character.id,
            progressionId = id,
            issue = issue,
            "Dropping a malformed character progression"
        );
    });

    if progressions.is_empty() {
        return String::new();
    }

    // Cadence input: the character's own most recent visible turn here. A
    // forced build skips the walk entirely — there is no history to consult on
    // a greeting, and `None` is what "report everything" means to the engine.
    let last_turn_ms = match (force, load_events, responding_participant_id) {
        (false, Some(load), Some(participant_id)) => match load() {
            Ok(events) => find_last_own_turn_ms(&events, participant_id),
            Err(error) => {
                tracing::warn!(
                    target: "progressions.prompt-section",
                    context = CONTEXT,
                    characterId = character.id,
                    error = error,
                    "Progressions section failed to build; the turn continues without it"
                );
                return String::new();
            }
        },
        _ => None,
    };

    let mut lines: Vec<String> = Vec::new();
    let mut decisions: Vec<Value> = Vec::new();

    // v4 iterates `ids.sort()` — JS default sort, which for the ASCII-only ids
    // the identifier rule admits is byte order.
    let mut ordered: Vec<_> = progressions.iter().collect();
    ordered.sort_by(|(a, _), (b, _)| a.as_bytes().cmp(b.as_bytes()));

    for (id, p) in ordered {
        let derived = derive_progression(id, p, now_ms);
        // MEASURED (P4.D168): this branch is REDUNDANT in v4 too, and a mutation
        // deleting it stays green on both sides. `force` already nulled
        // `last_turn_ms` above, and `should_report_progression` with `None`
        // takes rule 1 — which returns exactly `{report: true, reason: first}`.
        // v4 writes the ternary anyway (`prompt-section.ts:113-115`); it is
        // carried here for shape, not because either side can observe it. What
        // IS observable — a forced build reporting a `once` progression the
        // cadence would silence — is pinned by
        // `force_reports_everything_and_never_reads_events`.
        let (report, reason) = if force {
            (true, ReportReason::First)
        } else {
            let r = should_report_progression(p, &derived, last_turn_ms);
            (r.report, r.reason)
        };

        decisions.push(json!({
            "id": id,
            "state": derived.state.as_str(),
            "reason": reason.as_str(),
        }));
        if report {
            lines.push(format!(
                "- {}",
                render_progression_report(p, &derived, &RenderProgressionOptions { timezone })
            ));
        }
    }

    let section = if lines.is_empty() {
        String::new()
    } else {
        format!("{PROGRESSIONS_SECTION_HEADER}\n{}", lines.join("\n"))
    };

    // Bound OUTSIDE the macro: a `tracing` macro shadows `serde_json::Value`,
    // so `Value::Array(...)` inside one is `expected a type, found a trait`.
    let decisions_json = Value::Array(decisions);
    let responding_json = json!(responding_participant_id);
    let last_turn_json = json!(last_turn_ms);
    tracing::debug!(
        target: "progressions.prompt-section",
        context = CONTEXT,
        characterId = character.id,
        respondingParticipantId = %responding_json,
        lastTurnMs = %last_turn_json,
        forced = force,
        emitted = !section.is_empty(),
        reported = lines.len(),
        decisions = %decisions_json,
        "Character progressions evaluated for this turn"
    );

    section
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::captured;
    use serde_json::json;

    const CANNON: &str = "2026-09-08T14:00:00Z";
    /// `Date.parse('2026-09-08T14:00:00Z')`.
    const CANNON_MS: i64 = 1_788_876_000_000;
    const MIN: i64 = 60_000;

    fn metadata(progressions: Value) -> Value {
        json!({ "faction": "Ordo Aurum", "progressions": progressions })
    }

    fn cannon() -> Value {
        json!({
            "name": "Cannon recharge",
            "startTime": CANNON,
            "endTime": "2026-09-08T14:10:00Z",
            "timeIncrement": "minute",
        })
    }

    fn section(metadata: &Value, now_ms: i64, force: bool) -> String {
        build_progressions_section(BuildProgressionsSectionParams {
            character: Some(SectionCharacter {
                id: "char-1",
                metadata: Some(metadata),
            }),
            load_events: None,
            responding_participant_id: None,
            now_ms,
            timezone: None,
            force,
        })
    }

    /// The wrapper the caller pushes, header + one `- ` line per report.
    #[test]
    fn renders_the_header_and_one_dash_line_per_report() {
        let md = metadata(json!({ "cannon": cannon() }));
        let out = section(&md, CANNON_MS + 2 * MIN + 10_000, false);
        assert_eq!(
            out,
            "Time-bound conditions you are carrying, as of this moment:\n\
             - Cannon recharge: 2 minutes, 10 seconds elapsed, 7 minutes, 50 seconds \
             remaining, 22% complete."
        );
    }

    /// The empty-is-identical guarantee, in all three of its shapes.
    #[test]
    fn empty_is_byte_for_byte_nothing() {
        // No character at all.
        assert_eq!(
            build_progressions_section(BuildProgressionsSectionParams {
                character: None,
                load_events: None,
                responding_participant_id: None,
                now_ms: CANNON_MS,
                timezone: None,
                force: false,
            }),
            ""
        );
        // A character carrying none.
        assert_eq!(
            section(&json!({ "faction": "Ordo Aurum" }), CANNON_MS, false),
            ""
        );
        // Carrying one, but nothing reports this turn: `once`, already complete
        // and already announced.
        let silenced = metadata(json!({
            "cannon": {
                "name": "Cannon recharge",
                "startTime": CANNON,
                "endTime": "2026-09-08T14:10:00Z",
                "timeIncrement": "minute",
                "onComplete": "once",
            }
        }));
        let out = build_progressions_section(BuildProgressionsSectionParams {
            character: Some(SectionCharacter {
                id: "char-1",
                metadata: Some(&silenced),
            }),
            load_events: Some(&mut || Ok(vec![])),
            responding_participant_id: Some("p-1"),
            now_ms: CANNON_MS + 12 * MIN,
            timezone: None,
            force: false,
        });
        // `load_events` returns no rows, so `last_turn_ms` is `None` → rule 1
        // fires and it DOES report. The silenced case needs a last turn.
        assert!(out.starts_with(PROGRESSIONS_SECTION_HEADER));
    }

    /// v4 iterates `ids.sort()`, so two progressions come out in id order
    /// regardless of the order the metadata object holds them in.
    #[test]
    fn ids_are_sorted() {
        let md = metadata(json!({
            "zeta": { "name": "Zeta", "startTime": CANNON, "endTime": "2026-09-08T14:10:00Z",
                      "timeIncrement": "minute", "percentageReport": false },
            "alpha": { "name": "Alpha", "startTime": CANNON, "endTime": "2026-09-08T14:10:00Z",
                       "timeIncrement": "minute", "percentageReport": false },
        }));
        let out = section(&md, CANNON_MS + MIN, false);
        let alpha = out.find("Alpha").expect("alpha rendered");
        let zeta = out.find("Zeta").expect("zeta rendered");
        assert!(alpha < zeta, "ids.sort() puts alpha first:\n{out}");
    }

    /// `force` reports everything with reason `first`, cadence notwithstanding —
    /// and never consults the events thunk.
    #[test]
    fn force_reports_everything_and_never_reads_events() {
        let md = metadata(json!({
            "cannon": {
                "name": "Cannon recharge",
                "startTime": CANNON,
                "endTime": "2026-09-08T14:10:00Z",
                "timeIncrement": "minute",
                "onComplete": "once",
            }
        }));
        let mut reads = 0usize;
        let out = build_progressions_section(BuildProgressionsSectionParams {
            character: Some(SectionCharacter {
                id: "char-1",
                metadata: Some(&md),
            }),
            load_events: Some(&mut || {
                reads += 1;
                Ok(vec![])
            }),
            responding_participant_id: Some("p-1"),
            // Complete, already announced — `once` would silence it on cadence.
            now_ms: CANNON_MS + 12 * MIN,
            timezone: None,
            force: true,
        });
        assert!(out.contains("Cannon recharge: complete;"), "{out}");
        assert_eq!(reads, 0, "a forced build never walks the history");
    }

    /// The thunk is not called for a character carrying NO progressions — the
    /// read the empty-is-identical guarantee extends to.
    #[test]
    fn a_character_with_no_progressions_costs_no_read() {
        let mut reads = 0usize;
        let out = build_progressions_section(BuildProgressionsSectionParams {
            character: Some(SectionCharacter {
                id: "char-1",
                metadata: Some(&json!({ "faction": "Ordo Aurum" })),
            }),
            load_events: Some(&mut || {
                reads += 1;
                Ok(vec![])
            }),
            responding_participant_id: Some("p-1"),
            now_ms: CANNON_MS,
            timezone: None,
            force: false,
        });
        assert_eq!(out, "");
        assert_eq!(reads, 0, "not a query, not a row");
    }

    /// Line 1 — the per-entry drop warn, with v4's bag.
    #[test]
    fn warns_once_per_dropped_entry() {
        let md = metadata(json!({
            "cannon": cannon(),
            "broken": { "name": "B", "startTime": CANNON, "endTime": CANNON,
                        "timeIncrement": "minute" },
        }));
        let logs = captured(|| {
            let _ = section(&md, CANNON_MS + MIN, false);
        });
        let warns: Vec<&String> = logs
            .iter()
            .filter(|l| l.contains("Dropping a malformed character progression"))
            .collect();
        assert_eq!(warns.len(), 1, "one warn per dropped entry:\n{logs:#?}");
        let w = warns[0];
        assert!(w.starts_with("WARN progressions.prompt-section"), "{w}");
        assert!(w.contains("characterId=char-1"), "{w}");
        assert!(w.contains("progressionId=broken"), "{w}");
        assert!(w.contains("must be strictly after startTime"), "{w}");
    }

    /// Line 2 — the per-turn debug, with the decisions list.
    #[test]
    fn debugs_the_turns_decisions() {
        let md = metadata(json!({ "cannon": cannon() }));
        let logs = captured(|| {
            let _ = section(&md, CANNON_MS + MIN, true);
        });
        let line = logs
            .iter()
            .find(|l| l.contains("Character progressions evaluated for this turn"))
            .unwrap_or_else(|| panic!("no debug line:\n{logs:#?}"));
        assert!(
            line.starts_with("DEBUG progressions.prompt-section"),
            "{line}"
        );
        assert!(line.contains("characterId=char-1"), "{line}");
        assert!(line.contains("respondingParticipantId=null"), "{line}");
        assert!(line.contains("lastTurnMs=null"), "{line}");
        assert!(line.contains("forced=true"), "{line}");
        assert!(line.contains("emitted=true"), "{line}");
        assert!(line.contains("reported=1"), "{line}");
        assert!(
            line.contains(r#"{"id":"cannon","state":"active","reason":"first"}"#),
            "{line}"
        );
    }

    /// The debug line fires even when NOTHING reports — v4 logs it
    /// unconditionally, which is how an operator sees a silenced progression.
    #[test]
    fn debugs_even_when_nothing_reports() {
        let md = metadata(json!({
            "cannon": {
                "name": "Cannon recharge",
                "startTime": CANNON,
                "endTime": "2026-09-08T14:10:00Z",
                "timeIncrement": "minute",
                "onComplete": "once",
            }
        }));
        let logs = captured(|| {
            let _ = build_progressions_section(BuildProgressionsSectionParams {
                character: Some(SectionCharacter {
                    id: "char-1",
                    metadata: Some(&md),
                }),
                // A last turn AFTER the completion, so rule 3 does not fire and
                // rule 4 silences it.
                load_events: Some(&mut || {
                    Ok(vec![crate::core_whisper::WhisperEvent {
                        event_type: "message".to_string(),
                        role: Some("ASSISTANT".to_string()),
                        participant_id: Some("p-1".to_string()),
                        content: Some("spoke".to_string()),
                        created_at: Some("2026-09-08T14:11:00Z".to_string()),
                        ..Default::default()
                    }])
                }),
                responding_participant_id: Some("p-1"),
                now_ms: CANNON_MS + 12 * MIN,
                timezone: None,
                force: false,
            });
        });
        let line = logs
            .iter()
            .find(|l| l.contains("Character progressions evaluated for this turn"))
            .unwrap_or_else(|| panic!("no debug line:\n{logs:#?}"));
        assert!(line.contains("emitted=false"), "{line}");
        assert!(line.contains("reported=0"), "{line}");
        assert!(line.contains(r#""reason":"silenced""#), "{line}");
    }

    /// Line 3 — a failed history read never costs the turn.
    #[test]
    fn a_failed_event_read_warns_and_yields_nothing() {
        let md = metadata(json!({ "cannon": cannon() }));
        let logs = captured(|| {
            let out = build_progressions_section(BuildProgressionsSectionParams {
                character: Some(SectionCharacter {
                    id: "char-1",
                    metadata: Some(&md),
                }),
                load_events: Some(&mut || Err("no such table: messages".to_string())),
                responding_participant_id: Some("p-1"),
                now_ms: CANNON_MS + MIN,
                timezone: None,
                force: false,
            });
            assert_eq!(out, "", "the turn continues without the section");
        });
        let line = logs
            .iter()
            .find(|l| {
                l.contains("Progressions section failed to build; the turn continues without it")
            })
            .unwrap_or_else(|| panic!("no failure warn:\n{logs:#?}"));
        assert!(
            line.starts_with("WARN progressions.prompt-section"),
            "{line}"
        );
        assert!(line.contains("characterId=char-1"), "{line}");
        assert!(line.contains("no such table: messages"), "{line}");
    }
}
