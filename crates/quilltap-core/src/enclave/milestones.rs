//! The autonomous-room pacing milestones (U4.1) — a port of the milestone
//! machinery in v4's `lib/background-jobs/handlers/autonomous-room-turn.ts`
//! (v4 `6bf88959`): the bit constants + thresholds (lines 181–185), the
//! `MILESTONE_BINDING_PHRASE` table + `buildMilestoneMessage` composition
//! (lines 267–329), the grace-round bodies (lines 194–197), and the pure
//! decision rules extracted from the handler's inline milestone / grace
//! application sites (`handleAutonomousRoomTurn` lines 536–572 pre-turn,
//! 761–771 post-turn, 810–834 the milestone block).
//!
//! From v4 (the design why, lines 172–179): as a run approaches its budget the
//! Host nudges the room — once at the halfway mark, once at 10% remaining — so
//! the characters can pace themselves and wrap up gracefully before the run
//! stops. Which milestones have fired is tracked with a bitmask on the chat
//! row (reset at run start), so each fires exactly once even though the budget
//! is only sampled at turn boundaries.
//!
//! The message text lives in the generated [`milestone_text`] submodule,
//! extracted mechanically from the v4 source (the constants are module-private
//! in v4 — no oracle import is possible; see the generator
//! `harness/oracle/cases/gen-enclave-milestone-text.mjs`). The Rust
//! composition in [`build_milestone_message`] is unit-tested against the
//! generator's `COMPOSED_MILESTONE_MESSAGES` table — every (binding ×
//! milestone) cell evaluated by v4's OWN template literals — so transcription
//! cannot drift silently. The byte-exact END-TO-END proof (the persisted
//! `chat_messages` rows through the Host announcement writer) lands with
//! U4.4's tier-3 differential, which diffs the announcement rows v4's real
//! `handleAutonomousRoomTurn` posts.

pub mod milestone_text;

use crate::enclave_budget::MilestoneBinding;
pub use milestone_text::{GRACE_CONTENT, GRACE_OPAQUE};

/// Bit 0 of `runMilestonesAnnounced`: the halfway nudge has fired.
pub const MILESTONE_HALFWAY: i64 = 1;
/// Bit 1: the near-end (10%-remaining) nudge has fired.
pub const MILESTONE_NEAR_END: i64 = 2;
/// Bit 2: a one-turn grace round was granted (budget reached without a
/// near-end warning).
pub const MILESTONE_GRACE: i64 = 4;
/// The halfway nudge fires once progress reaches this fraction.
pub const HALFWAY_THRESHOLD: f64 = 0.5;
/// The near-end nudge fires once progress reaches this fraction (10% of the
/// budget remaining).
pub const NEAR_END_THRESHOLD: f64 = 0.9;

/// The `systemKind` of the grace-round announcement (v4 posts it at the
/// `grantGraceTurn` site, `autonomous-room-turn.ts:412`).
pub const GRACE_SYSTEM_KIND: &str = "autonomous-room-grace";

/// Which pacing milestone fires. The string forms match v4's
/// `'halfway' | 'near-end'` literals (used in the fire record + logging).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Milestone {
    Halfway,
    NearEnd,
}

impl Milestone {
    pub fn as_str(self) -> &'static str {
        match self {
            Milestone::Halfway => "halfway",
            Milestone::NearEnd => "near-end",
        }
    }
}

/// A milestone decision: which nudge fires and the new
/// `runMilestonesAnnounced` mask to persist.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MilestoneFire {
    pub milestone: Milestone,
    pub next_mask: i64,
}

/// The post-turn milestone rule, extracted verbatim from the inline block at
/// v4 `autonomous-room-turn.ts:812–820`. Given the binding budget's progress
/// fraction (from [`crate::enclave_budget::compute_budget_progress`] — the
/// caller skips this entirely when that returns `None`) and the current
/// `runMilestonesAnnounced` mask, decide which nudge (if any) fires and what
/// the new mask is. The run is known to be continuing at this point (the
/// budget re-check did not exhaust), so only the two nudges are in play here;
/// the grace round is [`post_turn_exhausted_action`]'s.
///
/// From v4 (lines 814–816): a single long turn can vault straight past the
/// halfway mark; fire only the (more urgent) near-end nudge, but record both
/// bits so the now-moot halfway nudge never fires after it. So a run that
/// vaults past 90% without halfway ever having fired gets NO halfway nudge —
/// one near-end nudge, mask gains `NEAR_END | HALFWAY`.
///
/// Each branch fires at most once per run: near-end when `fraction >= 0.9`
/// and the near-end bit is unset; else halfway when `fraction >= 0.5` and the
/// halfway bit is unset. (Faithful edge: a mask with near-end set but halfway
/// unset — never produced by this rule itself, since firing near-end sets
/// both — would fall through to the halfway branch at any fraction ≥ 0.5,
/// exactly as v4's `else if` does.)
pub fn decide_milestone(fraction: f64, mask: i64) -> Option<MilestoneFire> {
    if fraction >= NEAR_END_THRESHOLD && (mask & MILESTONE_NEAR_END) == 0 {
        Some(MilestoneFire {
            milestone: Milestone::NearEnd,
            next_mask: mask | MILESTONE_NEAR_END | MILESTONE_HALFWAY,
        })
    } else if fraction >= HALFWAY_THRESHOLD && (mask & MILESTONE_HALFWAY) == 0 {
        Some(MilestoneFire {
            milestone: Milestone::Halfway,
            next_mask: mask | MILESTONE_HALFWAY,
        })
    } else {
        None
    }
}

/// The new `runMilestonesAnnounced` mask recorded when a grace turn is
/// granted (v4 `grantGraceTurn`, `autonomous-room-turn.ts:409–411`).
pub fn grant_grace_mask(mask: i64) -> i64 {
    mask | MILESTONE_GRACE
}

/// What the turn handler does when a budget check comes back exhausted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExhaustedAction {
    /// Proceed with the turn anyway — this IS the granted grace turn (the
    /// grace bit is set); its own post-turn check ends the run.
    ProceedGraceTurn,
    /// End (or pause, for the daily cap) the run now.
    End,
    /// Grant one grace turn: record the grace bit
    /// ([`grant_grace_mask`]), post the grace announcement
    /// ([`GRACE_CONTENT`] / [`GRACE_OPAQUE`]), re-enqueue one more turn —
    /// without ending the run.
    GrantGrace,
}

/// The PRE-turn exhausted rule (v4 `autonomous-room-turn.ts:536–572`).
///
/// From v4's why-comments: exactly one grace turn is ever granted per run,
/// since the grace bit is checked before granting. Grace bit set → this is
/// the granted grace turn, let it run one last time even though the budget is
/// spent (the post-turn check sees the grace bit and ends the run cleanly
/// once this final word is delivered). Near-end bit set (no grace) → the
/// company already had their near-end warning, end now. Neither → budget
/// reached without a near-end warning ever firing, grant one grace turn so
/// the company gets a final word before the run closes. (Note: a fired
/// HALFWAY nudge alone does not count as a warning — only the near-end bit
/// suppresses the grace grant.)
pub fn pre_turn_exhausted_action(mask: i64) -> ExhaustedAction {
    if (mask & MILESTONE_GRACE) != 0 {
        ExhaustedAction::ProceedGraceTurn
    } else if (mask & MILESTONE_NEAR_END) != 0 {
        ExhaustedAction::End
    } else {
        ExhaustedAction::GrantGrace
    }
}

/// The POST-turn exhausted rule (v4 `autonomous-room-turn.ts:761–771`).
///
/// From v4: reached budget this turn without a near-end warning ever firing
/// (a single turn vaulted the [90%, 100%) band) → grant one grace turn so the
/// company gets a final word; that turn's own post-turn check ends the run
/// once the grace bit is set. Otherwise — either the grace turn just
/// completed (grace bit set), or the near-end warning already fired earlier
/// in the run — end now. Unlike the pre-turn rule this never proceeds: the
/// turn already ran.
pub fn post_turn_exhausted_action(mask: i64) -> ExhaustedAction {
    if (mask & MILESTONE_GRACE) == 0 && (mask & MILESTONE_NEAR_END) == 0 {
        ExhaustedAction::GrantGrace
    } else {
        ExhaustedAction::End
    }
}

/// A composed milestone announcement: the Host-voiced body, the persona-free
/// body for opaque-anywhere rooms, and the `systemKind` (v4's
/// `AutonomousRoomKind` literal).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MilestoneMessage {
    pub system_kind: &'static str,
    pub content: String,
    pub opaque_content: String,
}

/// Per-binding phrasing for the pacing nudges (v4's
/// `MILESTONE_BINDING_PHRASE`, `autonomous-room-turn.ts:267–297`).
/// `host_half` / `host_end` are the Host-voiced fragments
/// (steampunk-Wodehouse register); `opaque_noun` is the neutral noun used in
/// the persona-free body that steers the characters in opaque-anywhere rooms.
/// `pauses` marks budgets that *pause* the room rather than end the run — the
/// near-end nudge tells those characters they must stop for now and will
/// reconvene, not that the gathering closes for good.
#[derive(Clone, Copy, Debug)]
pub struct BindingPhrase {
    pub host_half: &'static str,
    pub host_end: &'static str,
    pub opaque_noun: &'static str,
    pub pauses: bool,
}

/// The `MILESTONE_BINDING_PHRASE` row for a binding (text from the generated
/// submodule).
pub fn milestone_binding_phrase(binding: MilestoneBinding) -> BindingPhrase {
    match binding {
        MilestoneBinding::Time => BindingPhrase {
            host_half: milestone_text::TIME_HOST_HALF,
            host_end: milestone_text::TIME_HOST_END,
            opaque_noun: milestone_text::TIME_OPAQUE_NOUN,
            pauses: milestone_text::TIME_PAUSES,
        },
        MilestoneBinding::Turns => BindingPhrase {
            host_half: milestone_text::TURNS_HOST_HALF,
            host_end: milestone_text::TURNS_HOST_END,
            opaque_noun: milestone_text::TURNS_OPAQUE_NOUN,
            pauses: milestone_text::TURNS_PAUSES,
        },
        MilestoneBinding::Tokens => BindingPhrase {
            host_half: milestone_text::TOKENS_HOST_HALF,
            host_end: milestone_text::TOKENS_HOST_END,
            opaque_noun: milestone_text::TOKENS_OPAQUE_NOUN,
            pauses: milestone_text::TOKENS_PAUSES,
        },
        MilestoneBinding::Daily => BindingPhrase {
            host_half: milestone_text::DAILY_HOST_HALF,
            host_end: milestone_text::DAILY_HOST_END,
            opaque_noun: milestone_text::DAILY_OPAQUE_NOUN,
            pauses: milestone_text::DAILY_PAUSES,
        },
    }
}

/// Compose the Host-voiced + persona-free bodies (and the `systemKind`) for a
/// pacing milestone, phrased around whichever budget is binding (v4's
/// `buildMilestoneMessage`, `autonomous-room-turn.ts:305–329`). A binding
/// that *pauses* the room (the daily cap) is framed as "finish for now, we
/// shall reconvene" rather than a final close.
pub fn build_milestone_message(
    binding: MilestoneBinding,
    milestone: Milestone,
) -> MilestoneMessage {
    let phrase = milestone_binding_phrase(binding);
    match milestone {
        Milestone::Halfway => MilestoneMessage {
            system_kind: "autonomous-room-halfway",
            content: format!(
                "The Host raps a crystal glass for attention: we have reached the midpoint of {}. There is room yet — but let the conversation begin to find its way toward what matters most.",
                phrase.host_half
            ),
            opaque_content: format!(
                "This conversation is halfway through its {}. Continue naturally, but begin steering toward what matters most before it {}.",
                phrase.opaque_noun,
                if phrase.pauses { "pauses" } else { "ends" }
            ),
        },
        Milestone::NearEnd if phrase.pauses => MilestoneMessage {
            system_kind: "autonomous-room-nearing-end",
            content: format!(
                "The Host consults a pocket-watch and clears their throat: {} is nearly spent, and this gathering must soon pause for the day. Finish now what most needs saying — the company shall reconvene when the allowance comes round again.",
                phrase.host_end
            ),
            opaque_content: format!(
                "This conversation is almost out of its {} and will pause shortly — it will resume later, but not before stopping for now. Say what most needs to be said, and bring the present scene to a close.",
                phrase.opaque_noun
            ),
        },
        Milestone::NearEnd => MilestoneMessage {
            system_kind: "autonomous-room-nearing-end",
            content: format!(
                "The Host consults a pocket-watch and clears their throat: {} is nearly spent, and this gathering must soon draw to a close. Say now what most needs saying, and bring your threads to a graceful rest.",
                phrase.host_end
            ),
            opaque_content: format!(
                "This conversation is almost out of its {} and will end soon. Say what most needs to be said, and begin bringing the scene to a close.",
                phrase.opaque_noun
            ),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn binding_from_str(s: &str) -> MilestoneBinding {
        match s {
            "time" => MilestoneBinding::Time,
            "turns" => MilestoneBinding::Turns,
            "tokens" => MilestoneBinding::Tokens,
            "daily" => MilestoneBinding::Daily,
            other => panic!("unknown binding {other}"),
        }
    }

    fn milestone_from_str(s: &str) -> Milestone {
        match s {
            "halfway" => Milestone::Halfway,
            "near-end" => Milestone::NearEnd,
            other => panic!("unknown milestone {other}"),
        }
    }

    #[test]
    fn constants_match_the_extracted_source_values() {
        assert_eq!(MILESTONE_HALFWAY, milestone_text::SRC_MILESTONE_HALFWAY);
        assert_eq!(MILESTONE_NEAR_END, milestone_text::SRC_MILESTONE_NEAR_END);
        assert_eq!(MILESTONE_GRACE, milestone_text::SRC_MILESTONE_GRACE);
        assert_eq!(HALFWAY_THRESHOLD, milestone_text::SRC_HALFWAY_THRESHOLD);
        assert_eq!(NEAR_END_THRESHOLD, milestone_text::SRC_NEAR_END_THRESHOLD);
    }

    #[test]
    fn composition_matches_every_generated_cell() {
        // The generator evaluated v4's OWN template literals against the
        // extracted phrase table; the Rust composition must reproduce every
        // cell byte-for-byte.
        let table = milestone_text::COMPOSED_MILESTONE_MESSAGES;
        assert_eq!(table.len(), 8, "expected 4 bindings x 2 milestones");
        for &(binding, milestone, kind, content, opaque) in table {
            let got =
                build_milestone_message(binding_from_str(binding), milestone_from_str(milestone));
            assert_eq!(got.system_kind, kind, "{binding}/{milestone} systemKind");
            assert_eq!(got.content, content, "{binding}/{milestone} content");
            assert_eq!(
                got.opaque_content, opaque,
                "{binding}/{milestone} opaqueContent"
            );
        }
    }

    #[test]
    fn milestone_decision_matrix() {
        // Below halfway: nothing fires at any not-yet-fired mask.
        assert_eq!(decide_milestone(0.0, 0), None);
        assert_eq!(decide_milestone(0.499_999_999, 0), None);

        // Halfway band [0.5, 0.9): fires once.
        assert_eq!(
            decide_milestone(0.5, 0),
            Some(MilestoneFire {
                milestone: Milestone::Halfway,
                next_mask: MILESTONE_HALFWAY,
            })
        );
        assert_eq!(
            decide_milestone(0.899_999_999, 0),
            Some(MilestoneFire {
                milestone: Milestone::Halfway,
                next_mask: MILESTONE_HALFWAY,
            })
        );
        // Already fired → silent.
        assert_eq!(decide_milestone(0.6, MILESTONE_HALFWAY), None);

        // Near-end at exactly the threshold, halfway having fired earlier.
        assert_eq!(
            decide_milestone(0.9, MILESTONE_HALFWAY),
            Some(MilestoneFire {
                milestone: Milestone::NearEnd,
                next_mask: MILESTONE_HALFWAY | MILESTONE_NEAR_END,
            })
        );

        // The vault case: past 0.9 with NOTHING fired — the halfway nudge is
        // skipped (no-nudge), only near-end fires, and BOTH bits are recorded
        // so the now-moot halfway nudge never fires after it.
        assert_eq!(
            decide_milestone(0.95, 0),
            Some(MilestoneFire {
                milestone: Milestone::NearEnd,
                next_mask: MILESTONE_HALFWAY | MILESTONE_NEAR_END,
            })
        );
        // Over 1.0 behaves the same (the fraction is not clamped).
        assert_eq!(
            decide_milestone(1.3, 0),
            Some(MilestoneFire {
                milestone: Milestone::NearEnd,
                next_mask: MILESTONE_HALFWAY | MILESTONE_NEAR_END,
            })
        );

        // Both fired → silent forever after.
        assert_eq!(
            decide_milestone(0.95, MILESTONE_HALFWAY | MILESTONE_NEAR_END),
            None
        );
        assert_eq!(
            decide_milestone(0.6, MILESTONE_HALFWAY | MILESTONE_NEAR_END),
            None
        );

        // Faithful v4 edge: near-end set but halfway UNSET (not producible by
        // this rule itself) falls through the `else if` to the halfway branch.
        assert_eq!(
            decide_milestone(0.95, MILESTONE_NEAR_END),
            Some(MilestoneFire {
                milestone: Milestone::Halfway,
                next_mask: MILESTONE_NEAR_END | MILESTONE_HALFWAY,
            })
        );

        // The grace bit does not gate the nudges (v4 masks only the specific
        // bit under test).
        assert_eq!(
            decide_milestone(0.95, MILESTONE_GRACE),
            Some(MilestoneFire {
                milestone: Milestone::NearEnd,
                next_mask: MILESTONE_GRACE | MILESTONE_NEAR_END | MILESTONE_HALFWAY,
            })
        );
    }

    #[test]
    fn pre_turn_exhausted_matrix() {
        // Grace bit set → this IS the grace turn; proceed (checked first,
        // even when near-end is also set).
        assert_eq!(
            pre_turn_exhausted_action(MILESTONE_GRACE),
            ExhaustedAction::ProceedGraceTurn
        );
        assert_eq!(
            pre_turn_exhausted_action(MILESTONE_GRACE | MILESTONE_NEAR_END | MILESTONE_HALFWAY),
            ExhaustedAction::ProceedGraceTurn
        );
        // Near-end fired (no grace) → already warned, end now.
        assert_eq!(
            pre_turn_exhausted_action(MILESTONE_NEAR_END),
            ExhaustedAction::End
        );
        assert_eq!(
            pre_turn_exhausted_action(MILESTONE_NEAR_END | MILESTONE_HALFWAY),
            ExhaustedAction::End
        );
        // No near-end warning ever → grant the grace turn. A halfway nudge
        // alone does NOT count as a warning.
        assert_eq!(pre_turn_exhausted_action(0), ExhaustedAction::GrantGrace);
        assert_eq!(
            pre_turn_exhausted_action(MILESTONE_HALFWAY),
            ExhaustedAction::GrantGrace
        );
    }

    #[test]
    fn post_turn_exhausted_matrix() {
        // Vaulted to exhaustion with no near-end warning → grant grace.
        assert_eq!(post_turn_exhausted_action(0), ExhaustedAction::GrantGrace);
        assert_eq!(
            post_turn_exhausted_action(MILESTONE_HALFWAY),
            ExhaustedAction::GrantGrace
        );
        // Warned earlier, or the grace turn just completed → end.
        assert_eq!(
            post_turn_exhausted_action(MILESTONE_NEAR_END),
            ExhaustedAction::End
        );
        assert_eq!(
            post_turn_exhausted_action(MILESTONE_NEAR_END | MILESTONE_HALFWAY),
            ExhaustedAction::End
        );
        assert_eq!(
            post_turn_exhausted_action(MILESTONE_GRACE),
            ExhaustedAction::End
        );
        assert_eq!(
            post_turn_exhausted_action(MILESTONE_GRACE | MILESTONE_NEAR_END | MILESTONE_HALFWAY),
            ExhaustedAction::End
        );
    }

    #[test]
    fn grace_mask_and_strings() {
        assert_eq!(grant_grace_mask(0), MILESTONE_GRACE);
        assert_eq!(
            grant_grace_mask(MILESTONE_HALFWAY),
            MILESTONE_HALFWAY | MILESTONE_GRACE
        );
        // Idempotent, and never disturbs other bits.
        assert_eq!(
            grant_grace_mask(MILESTONE_HALFWAY | MILESTONE_NEAR_END | MILESTONE_GRACE),
            MILESTONE_HALFWAY | MILESTONE_NEAR_END | MILESTONE_GRACE
        );
        // The grace bodies are re-exported from the generated submodule; pin
        // the UTF-16 lengths the generator recorded (a cheap tamper check —
        // both strings are pure BMP text, so chars == UTF-16 units).
        assert_eq!(GRACE_CONTENT.chars().count(), 234);
        assert_eq!(GRACE_OPAQUE.chars().count(), 173);
        assert_eq!(GRACE_SYSTEM_KIND, "autonomous-room-grace");
    }
}
