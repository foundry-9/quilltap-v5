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
use quilltap_core::generators::external_prompt::build_meta_system_prompt;
use quilltap_core::generators::field_semantics::{
    generator_direction_for, kind_drift_signal_for, ALL_EXPORTS, ASSISTANT_TASK_DIRECTION,
    COMMITTEE_DRIFT_GUARDRAIL, COMPANION_TRUST_DISPOSITION, COMPANION_TRUST_DISPOSITION_GATE,
    EXAMPLE_DIALOGUE_COVERAGE, FLATTENING_DRIFT_SIGNAL, GATED_COMPANION_TRUST_DISPOSITION,
    STAKES_AND_REACTION_DIRECTION, TRUST_SAFEGUARDS_DIRECTION,
};
use quilltap_core::generators::optimizer::{
    get_analysis_prompt, get_new_system_prompts_suggestion_prompt,
    get_system_prompt_suggestion_prompt,
};
use quilltap_core::generators::wizard::wizard_field_max_tokens;
use serde_json::{json, Value};
use quilltap_core::generators::wizard_prompts::{
    build_system_prompt_field_prompt, field_prompt, wizard_field_prompt,
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

// ---- character-wizard-prompts.test.ts (v4 3ae4cf582) ----------------------

#[test]
fn system_prompt_prompt_for_a_roleplay_character_carries_the_stakes_direction_before_the_companion_gate(
) {
    let p = build_system_prompt_field_prompt(CharacterKind::Roleplay);
    assert!(p.contains(STAKES_AND_REACTION_DIRECTION));
    assert!(!p.contains(ASSISTANT_TASK_DIRECTION));
    let stakes = p.find(STAKES_AND_REACTION_DIRECTION).unwrap();
    assert!(stakes > p.find(TRUST_SAFEGUARDS_DIRECTION).unwrap());
    assert!(stakes < p.find(COMPANION_TRUST_DISPOSITION_GATE).unwrap());
}

#[test]
fn system_prompt_prompt_for_an_assistant_carries_the_task_direction_instead() {
    let p = build_system_prompt_field_prompt(CharacterKind::Assistant);
    assert!(p.contains(ASSISTANT_TASK_DIRECTION));
    assert!(!p.contains(STAKES_AND_REACTION_DIRECTION));
}

#[test]
fn wizard_field_prompt_routes_system_prompt_through_the_kind_and_every_other_field_to_field_prompts(
) {
    assert_eq!(
        wizard_field_prompt("systemPrompt", CharacterKind::Assistant),
        Some(build_system_prompt_field_prompt(CharacterKind::Assistant))
    );
    assert_eq!(
        wizard_field_prompt("identity", CharacterKind::Assistant),
        field_prompt("identity")
    );
    assert_eq!(field_prompt("systemPrompt"), None);
}

#[test]
fn gives_the_system_prompt_room_to_finish() {
    // A reasoning model truncated it at the old 1000-token cap.
    assert_eq!(wizard_field_max_tokens("systemPrompt"), 2500);
    assert_eq!(wizard_field_max_tokens("exampleDialogues"), 1000);
    assert_eq!(wizard_field_max_tokens("firstMessage"), 1000);
    assert_eq!(wizard_field_max_tokens("scenarios"), 4000);
    assert_eq!(wizard_field_max_tokens("identity"), 500);
}

// ---- external-prompt-generator.test.ts (v4 3ae4cf582) ---------------------

#[test]
fn external_prompt_follows_the_character_kind_stakes_for_roleplay_the_work_for_an_assistant() {
    let roleplay = build_meta_system_prompt(CharacterKind::Roleplay);
    assert!(roleplay.contains(STAKES_AND_REACTION_DIRECTION));
    assert!(!roleplay.contains(ASSISTANT_TASK_DIRECTION));
    let assistant = build_meta_system_prompt(CharacterKind::Assistant);
    assert!(assistant.contains(ASSISTANT_TASK_DIRECTION));
    assert!(!assistant.contains(STAKES_AND_REACTION_DIRECTION));
}

// ---- character-optimizer-helpers.test.ts, "the character kind in the
// Optimizer" (v4 3ae4cf582) ------------------------------------------------

fn optimizer_fixture() -> (Value, Value) {
    let analysis = json!({ "behavioralPatterns": [], "summary": "steady" });
    let prompt = json!({
        "id": "sp-k",
        "name": "Default",
        "content": "You are calm.",
        "isDefault": true,
        "createdAt": "2026-01-01T00:00:00Z",
        "updatedAt": "2026-01-01T00:00:00Z",
    });
    (analysis, prompt)
}

#[test]
fn analysis_looks_for_flattening_in_a_roleplay_character_not_in_an_assistant() {
    let roleplay = get_analysis_prompt(CharacterKind::Roleplay);
    assert!(roleplay.contains("- Flattening — "));
    assert!(roleplay.contains(FLATTENING_DRIFT_SIGNAL));
    assert!(!get_analysis_prompt(CharacterKind::Assistant).contains("Flattening"));
}

#[test]
fn refining_a_roleplay_prompt_carries_the_stakes_direction_and_the_flattening_signal() {
    let (analysis, prompt) = optimizer_fixture();
    let p = get_system_prompt_suggestion_prompt(&analysis, &prompt, CharacterKind::Roleplay);
    assert!(p.contains(STAKES_AND_REACTION_DIRECTION));
    assert!(p.contains(FLATTENING_DRIFT_SIGNAL));
    assert!(!p.contains(ASSISTANT_TASK_DIRECTION));
}

#[test]
fn refining_an_assistant_prompt_carries_the_task_direction_and_no_flattening_signal() {
    let (analysis, prompt) = optimizer_fixture();
    let p = get_system_prompt_suggestion_prompt(&analysis, &prompt, CharacterKind::Assistant);
    assert!(p.contains(ASSISTANT_TASK_DIRECTION));
    assert!(!p.contains(FLATTENING_DRIFT_SIGNAL));
    assert!(!p.contains(STAKES_AND_REACTION_DIRECTION));
}

#[test]
fn a_new_prompt_follows_the_kind() {
    let (analysis, _) = optimizer_fixture();
    assert!(
        get_new_system_prompts_suggestion_prompt(&analysis, CharacterKind::Roleplay)
            .contains(STAKES_AND_REACTION_DIRECTION)
    );
    assert!(
        get_new_system_prompts_suggestion_prompt(&analysis, CharacterKind::Assistant)
            .contains(ASSISTANT_TASK_DIRECTION)
    );
}
