//! v4 `ca363178d`'s order and containment assertions over the trust-safeguard
//! directions, as v5 unit pins (P4.D241). The byte-exact differential families
//! (`generators_leaf`, `generators_wizard_prompts`, `character_optimizer_prompts`,
//! `ai_import_tier3`, `external_prompt_tier3`) are strictly stronger, but they
//! skip without an oracle; these pins run on every `cargo test` and name the
//! invariant each site exists to keep. Mirrors (as v5 shapes, never the mocks):
//! `ai-import-system-prompts.test.ts`, `character-wizard-prompts.test.ts`,
//! `character-optimizer-helpers.test.ts`, `external-prompt-generator.test.ts`.

use quilltap_core::generators::ai_import::system_prompts_prompt;
use quilltap_core::generators::external_prompt::META_SYSTEM_PROMPT;
use quilltap_core::generators::field_semantics::{
    COMMITTEE_DRIFT_GUARDRAIL, COMPANION_TRUST_DISPOSITION, COMPANION_TRUST_DISPOSITION_GATE,
    CONVERSATIONAL_VOICE_DIRECTION, GATED_COMPANION_TRUST_DISPOSITION, TRUST_SAFEGUARDS_DIRECTION,
};
use quilltap_core::generators::optimizer::{
    get_analysis_prompt, get_general_fields_suggestions_prompt,
    get_new_system_prompts_suggestion_prompt, get_system_prompt_suggestion_prompt,
    SUGGESTION_SCHEMA_PREAMBLE,
};
use quilltap_core::generators::wizard_prompts::FIELD_PROMPT_SYSTEM_PROMPT;
use serde_json::json;

/// `indexOf(a)` of the first occurrence, panicking (with the needle) when absent.
fn at(haystack: &str, needle: &str) -> usize {
    haystack
        .find(needle)
        .unwrap_or_else(|| panic!("missing: {}", &needle[..needle.len().min(60)]))
}

/// The gate comes before the disposition — the order v4's tests pin at every
/// GATED / framing-gate site.
fn gate_before_disposition(prompt: &str, gate: &str) {
    assert!(at(prompt, gate) < at(prompt, COMPANION_TRUST_DISPOSITION));
}

#[test]
fn gated_disposition_is_the_gate_one_newline_then_the_disposition() {
    assert_eq!(
        GATED_COMPANION_TRUST_DISPOSITION,
        format!("{COMPANION_TRUST_DISPOSITION_GATE}\n{COMPANION_TRUST_DISPOSITION}")
    );
}

#[test]
fn ai_import_system_prompts_prompt_carries_the_directions_in_order() {
    let p = system_prompts_prompt();
    assert!(p.contains(CONVERSATIONAL_VOICE_DIRECTION));
    assert!(p.contains(TRUST_SAFEGUARDS_DIRECTION));
    assert!(p.contains("300-600 words"));
    gate_before_disposition(&p, COMPANION_TRUST_DISPOSITION_GATE);
    // The relationships sentence closes the gated paragraph (ONE newline).
    assert!(p.ends_with(&format!(
        "{GATED_COMPANION_TRUST_DISPOSITION}\nUse the relationships array in the Prior Analysis, where one is given, as evidence for that decision."
    )));
}

#[test]
fn wizard_system_prompt_field_carries_the_directions_in_order() {
    let p = FIELD_PROMPT_SYSTEM_PROMPT;
    assert!(p.contains(TRUST_SAFEGUARDS_DIRECTION));
    assert!(p.to_lowercase().contains("not a committee"));
    gate_before_disposition(p, COMPANION_TRUST_DISPOSITION_GATE);
    assert!(p.contains("under 600 words"));
}

#[test]
fn optimizer_preamble_rule_keeps_the_literal_placeholder() {
    assert!(SUGGESTION_SCHEMA_PREAMBLE.contains(
        "Never propose a trait, rule, or condition that constrains what {{user}}'s persona may do"
    ));
    assert!(SUGGESTION_SCHEMA_PREAMBLE.contains("drift to correct, not behaviour to capture"));
    let analysis = json!({ "behavioralPatterns": [], "summary": "s" });
    assert!(get_general_fields_suggestions_prompt(&analysis).contains(SUGGESTION_SCHEMA_PREAMBLE));
    assert!(
        get_new_system_prompts_suggestion_prompt(&analysis).contains(SUGGESTION_SCHEMA_PREAMBLE)
    );
}

#[test]
fn optimizer_analysis_flags_committee_drift() {
    let p = get_analysis_prompt();
    assert!(p.contains("Committee drift — the character governing {{user}}'s persona"));
    assert!(p.contains(COMMITTEE_DRIFT_GUARDRAIL));
}

#[test]
fn optimizer_refine_pass_keeps_every_never_weaken_rule_and_its_own_gate() {
    let analysis = json!({ "behavioralPatterns": [], "summary": "s" });
    let prompt = json!({ "id": "p1", "name": "Main", "isDefault": true, "content": "c" });
    let p = get_system_prompt_suggestion_prompt(&analysis, &prompt);
    assert!(p.contains("Do NOT change the prompt's evident interaction style"));
    assert!(p.contains("Never remove or weaken the prompt's direction about listening"));
    assert!(p.contains("Do NOT codify repetition"));
    assert!(p.contains("Never remove or weaken the prompt's trust safeguards"));
    assert!(p.contains(TRUST_SAFEGUARDS_DIRECTION));
    assert!(p.contains(&format!("- {COMMITTEE_DRIFT_GUARDRAIL}")));
    gate_before_disposition(
        &p,
        "If the prompt under review frames the character as {{user}}'s companion",
    );
    // The optimizer uses its OWN gate, never the shared one.
    assert!(!p.contains(COMPANION_TRUST_DISPOSITION_GATE));
}

#[test]
fn optimizer_new_prompts_pass_has_trust_and_its_own_gate_but_no_committee() {
    let analysis = json!({ "behavioralPatterns": [], "summary": "s" });
    let p = get_new_system_prompts_suggestion_prompt(&analysis);
    assert!(p.contains(&format!("- {TRUST_SAFEGUARDS_DIRECTION}")));
    gate_before_disposition(
        &p,
        "If the existing prompts frame the character as {{user}}'s companion",
    );
    assert!(!p.contains(COMMITTEE_DRIFT_GUARDRAIL));
    assert!(!p.contains(COMPANION_TRUST_DISPOSITION_GATE));
}

#[test]
fn external_meta_prompt_carries_the_directions_and_forbids_the_literal_token() {
    let p = META_SYSTEM_PROMPT.as_str();
    assert!(p.contains(CONVERSATIONAL_VOICE_DIRECTION));
    assert!(p.contains(TRUST_SAFEGUARDS_DIRECTION));
    gate_before_disposition(p, COMPANION_TRUST_DISPOSITION_GATE);
    assert!(p.contains("never the literal {{user}} token"));
    // Every block `\n\n`-separated: the placeholder sentence is its OWN paragraph.
    assert!(p.contains(&format!(
        "{GATED_COMPANION_TRUST_DISPOSITION}\n\nThe external tool will not substitute placeholders"
    )));
    assert_eq!(p.encode_utf16().count(), 5664);
}
