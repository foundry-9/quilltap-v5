//! The character-generator substrate (`p4.9k`, P4.9K0).
//!
//! v4's three character generators — the optimizer
//! (`lib/services/character-optimizer.service.ts`), the AI Wizard
//! (`lib/services/character-wizard.service.ts`) and Summon From Lore
//! (`lib/services/ai-import.service.ts`) — share four pure leaf modules. This
//! module is their one v5 home, ported ahead of the generators themselves so
//! neither server lane ports a shared leaf twice under two names.
//!
//! | v5 module | v4 file |
//! | --- | --- |
//! | [`llm_json`] | `lib/llm/llm-json.ts` |
//! | [`field_semantics`] | `lib/services/character-field-semantics.ts` |
//! | [`generated_properties`] | `lib/characters/generated-properties.ts` |
//! | [`sanitize_pronouns`] | `lib/characters/sanitize-pronouns.ts` |
//!
//! **Deferred, loudly (P4.9K0 tier 3).** [`llm_json`] is a NEW home for v4's
//! module, *not* a consolidation of the five per-caller JSON extractors v5
//! already carries — each of those is oracle-pinned where it sits and must not
//! be retargeted onto this one:
//!
//! * `services::answer_confirmation::extract_json` (private)
//! * `memory_tasks`
//! * `services::image_scene_tasks`
//! * `services::outfit_selections`
//! * `services::context_summary::title_verdict`
//!
//! The consolidation is RECORDED as a candidate and deliberately NOT performed.

// P4.9K1 and P4.9K2 stack on this lane and add their generator modules here
// (`optimizer` / `external_prompt` / `rename` / `refresh_archive` and
// `wizard` / `ai_import`), each inside its own `// === P4.9K<n> ===` fence.

// === P4.9K1 ===
pub mod optimizer;
pub mod refresh_archive;
pub mod rename;
// === end P4.9K1 ===

// === P4.9K2 ===
pub mod wizard;
pub mod wizard_prompts;
// === end P4.9K2 ===

pub mod field_semantics;
pub mod generated_properties;
pub mod llm_json;
pub mod sanitize_pronouns;
