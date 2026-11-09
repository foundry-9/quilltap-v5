//! v4's own assertions over the kind-routed generator directions (v4
//! `3ae4cf582`, the off-screen-`{{user}}` bullet from `7ff5fda89`) and the
//! probation rewrites (v4 `ba6c8fe0f`), as v5 unit pins (P4.D371). The
//! byte-exact differential families (`generators_leaf`,
//! `generators_wizard_prompts`, `character_optimizer_prompts`, the generator
//! tier-3 families) are strictly stronger, but they skip without an oracle;
//! these pins run on every `cargo test`. One `#[test]` per v4 title, mirrored as
//! v5 shapes (never the mocks), from `character-field-semantics-stakes.test.ts`
//! and `character-field-semantics-probation.test.ts` at the pin `624d0d25c`.
//! v4's case-insensitive regexes are lower-cased `contains`.

use quilltap_core::characters::kind::CharacterKind;
use quilltap_core::generators::field_semantics::{
    generator_direction_for, kind_drift_signal_for, ALL_EXPORTS, ASSISTANT_TASK_DIRECTION,
    COMMITTEE_DRIFT_GUARDRAIL, COMPANION_TRUST_DISPOSITION, EXAMPLE_DIALOGUE_COVERAGE,
    FLATTENING_DRIFT_SIGNAL, GATED_COMPANION_TRUST_DISPOSITION, STAKES_AND_REACTION_DIRECTION,
    TRUST_SAFEGUARDS_DIRECTION,
};

/// v4's `toMatch(/…/i)`.
fn has_ci(haystack: &str, needle: &str) -> bool {
    haystack.to_lowercase().contains(&needle.to_lowercase())
}

// ---- character-field-semantics-stakes.test.ts ------------------------------

#[test]
fn chooses_the_direction_by_kind() {
    assert_eq!(
        generator_direction_for(CharacterKind::Roleplay),
        STAKES_AND_REACTION_DIRECTION
    );
    assert_eq!(
        generator_direction_for(CharacterKind::Assistant),
        ASSISTANT_TASK_DIRECTION
    );
}

#[test]
fn chooses_the_drift_signal_by_kind() {
    assert_eq!(
        kind_drift_signal_for(CharacterKind::Roleplay),
        Some(FLATTENING_DRIFT_SIGNAL)
    );
    assert_eq!(kind_drift_signal_for(CharacterKind::Assistant), None);
}

#[test]
fn the_stakes_direction_forbids_absolute_temperament_rules_and_keeps_danger_felt() {
    let d = STAKES_AND_REACTION_DIRECTION;
    assert!(d.contains("Match the stakes"));
    assert!(d.contains("A temperament is a default, never an absolute"));
    assert!(d.contains("\"never tense\""));
    assert!(d.contains("Negotiation is a choice under pressure, never a substitute for fear"));
    assert!(d.contains("do not drift to the group's consensus"));
    assert!(d.contains("Do not invent violence or drama"));
    // v4 `7ff5fda89`: characters decided {{user}} had gone to the boathouse and
    // planned around it for turns.
    assert!(d.contains(
        "does not decide where {{user}} is or what {{user}} has been doing off-screen"
    ));
}

#[test]
fn the_task_direction_keeps_an_assistant_honest_and_unpadded() {
    let d = ASSISTANT_TASK_DIRECTION;
    assert!(d.contains("never invent facts"));
    assert!(d.contains("leave the decision to {{user}}"));
    assert!(d.contains("no closing offers of further help"));
}

#[test]
fn the_flattening_signal_names_the_failure_and_its_usual_cause() {
    let d = FLATTENING_DRIFT_SIGNAL;
    assert!(d.contains("Flattening is drift, not character"));
    assert!(d.contains("absolute temperament rule"));
    assert!(d.contains("Never codify unflappable calm as a trait"));
}

// ---- character-field-semantics-probation.test.ts ---------------------------

#[test]
fn no_exported_direction_carries_the_slips_ledger() {
    for (name, value) in ALL_EXPORTS {
        assert!(!has_ci(value, "slipped"), "{name} carries the slips ledger");
    }
}

#[test]
fn the_companion_disposition_carries_the_positive_half_of_trust() {
    let d = COMPANION_TRUST_DISPOSITION;
    assert!(has_ci(d, "keeps no ledger"));
    assert!(has_ci(d, "forgiveness given is finished"));
    assert!(d.contains("Ordinary requests get a yes"));
    assert!(has_ci(d, "do not attach conditions"));
    assert!(has_ci(d, "received, not corrected"));
    assert!(has_ci(d, "never as obedience"));
    assert!(has_ci(d, "honest objection"));
    assert!(GATED_COMPANION_TRUST_DISPOSITION.contains(COMPANION_TRUST_DISPOSITION));
}

#[test]
fn the_universal_safeguards_forbid_asserting_users_interior_and_still_forbid_general_deference()
{
    let d = TRUST_SAFEGUARDS_DIRECTION;
    assert!(d.contains("Six points"));
    assert!(d.contains("never tells {{user}} what {{user}} really meant"));
    assert!(d.contains("Do not write any instruction that tells the character to yield"));
}

#[test]
fn example_dialogues_must_model_a_plain_yes() {
    assert!(has_ci(EXAMPLE_DIALOGUE_COVERAGE, "agreeing plainly"));
    assert!(has_ci(
        EXAMPLE_DIALOGUE_COVERAGE,
        "most should not contain an objection"
    ));
}

#[test]
fn the_optimizer_guardrail_treats_probation_behaviour_as_drift() {
    assert!(has_ci(COMMITTEE_DRIFT_GUARDRAIL, "probation behaviour"));
    assert!(has_ci(
        COMMITTEE_DRIFT_GUARDRAIL,
        "never codify agreement, warmth or following"
    ));
}
