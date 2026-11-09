//! Tier-1 differential: the AI Wizard's PURE arms (P4.9K2, unit 1) — the
//! exported prompt constants and `buildContextPrompt` — byte-exact against v4's
//! REAL exports.
//!
//! Every comparand is a whole string: these prompts reach a paid model
//! verbatim. The `FIELD_PROMPTS` table is compared BOTH ways — every v4 key
//! present in v5 with identical bytes, and no v5 key v4 does not have — and its
//! ORDER is pinned, because the generated table records v4's insertion order
//! rather than sorting.
//!
//! ⚠ **Measured:** `FIELD_PROMPTS` has **9** keys at v4 `3ae4cf582` (10
//! before — `systemPrompt` moved to the kind-routed
//! `buildSystemPromptFieldPrompt`, P4.D371). Thirteen is the size of
//! `WizardRequest.fieldsToGenerate`'s union; `properties`,
//! `physicalDescription` and `wardrobeItems` are served by dedicated generators
//! rather than by this table. The floor below is 9 and the coverage row is
//! diffed exactly, so a future drift in either direction is loud.
//!
//! P4.D371 row kinds: `system_prompt_field` (one per generator direction key,
//! v4's `buildSystemPromptFieldPrompt(kind)` bytes), `wizard_field_prompt`
//! (every union member + `bogus`, × both kinds — v4's `undefined` is JSON
//! `null` ↔ `None`) and `wizard_field_max_tokens` (the same fourteen names).
//! The module is GENERATED (`harness/oracle/tools/gen-wizard-prompts.mjs`, its
//! header has the recipe, then `cargo fmt -p quilltap-core`).
//!
//! Regenerate the oracle (the corpus is committed — nothing to build):
//!   cd ~/source/quilltap-server
//!   npx tsx $V5W/harness/oracle/cases/generators-wizard-prompts.ts > /tmp/oracle-generators-wizard-prompts.ndjson
//! Run:
//!   QT_ORACLE_GENERATORS_WIZARD_PROMPTS=/tmp/oracle-generators-wizard-prompts.ndjson cargo test -p quilltap-harness --test generators_wizard_prompts_equivalence
//!
//! While the v4 checkout sits on `bugfix` (drift-ledger §1) the regen needs a
//! worktree pinned at the baseline — the sweep driver's job, never a path baked
//! into this header.

use quilltap_core::characters::kind::CharacterKind;
use quilltap_core::generators::wizard::{build_context_prompt, wizard_field_max_tokens};
use quilltap_core::generators::wizard_prompts::{
    build_system_prompt_field_prompt, field_prompt, wizard_field_prompt, FIELD_PROMPTS,
    HEAD_AND_SHOULDERS_PHYSICAL_PROMPT, PROPERTIES_PROMPT,
};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
#[serde(tag = "kind")]
enum Row {
    #[serde(rename = "field_prompt")]
    FieldPrompt { name: String, value: String },
    #[serde(rename = "constant")]
    Constant { name: String, value: String },
    #[serde(rename = "context_prompt", rename_all = "camelCase")]
    ContextPrompt {
        id: String,
        character_name: String,
        background: String,
        existing_data: Value,
        image_description: Option<String>,
        document_content: Option<String>,
        out: String,
    },
    /// P4.D371 — v4 `buildSystemPromptFieldPrompt(characterKind)`.
    #[serde(rename = "system_prompt_field", rename_all = "camelCase")]
    SystemPromptField {
        direction: String,
        character_kind: CharacterKind,
        value: String,
    },
    /// P4.D371 — v4 `wizardFieldPrompt(field, characterKind) ?? null`.
    #[serde(rename = "wizard_field_prompt", rename_all = "camelCase")]
    WizardFieldPrompt {
        field: String,
        character_kind: CharacterKind,
        value: Option<String>,
    },
    /// P4.D371 — v4 `wizardFieldMaxTokens(field)`.
    #[serde(rename = "wizard_field_max_tokens")]
    WizardFieldMaxTokens { field: String, value: i64 },
    #[serde(rename = "coverage", rename_all = "camelCase")]
    Coverage {
        field_prompts: Vec<String>,
        system_prompt_fields: Vec<String>,
        wizard_fields: Vec<String>,
        contexts: Vec<String>,
    },
}

fn diff_report(label: &str, got: &str, want: &str) -> String {
    let mut g = got.lines();
    let mut w = want.lines();
    let mut n = 1;
    loop {
        match (g.next(), w.next()) {
            (Some(a), Some(b)) if a == b => n += 1,
            (a, b) => {
                return format!(
                    "{label}: first difference at line {n}\n  v5:   {:?}\n  v4:   {:?}\n  (lengths {} vs {})",
                    a.unwrap_or("<eof>"),
                    b.unwrap_or("<eof>"),
                    got.len(),
                    want.len()
                )
            }
        }
    }
}

fn eq(label: &str, got: &str, want: &str) {
    if got != want {
        panic!("{}", diff_report(label, got, want));
    }
}

#[test]
fn generators_wizard_prompts_match_oracle() {
    let path = match std::env::var("QT_ORACLE_GENERATORS_WIZARD_PROMPTS") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_GENERATORS_WIZARD_PROMPTS to the oracle NDJSON (see test header).");
            return;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));

    let mut seen_fields: Vec<String> = Vec::new();
    let mut seen_contexts: Vec<String> = Vec::new();
    let mut seen_constants: Vec<String> = Vec::new();
    let mut seen_directions: Vec<String> = Vec::new();
    let mut seen_wizard_fields: Vec<(String, CharacterKind)> = Vec::new();
    let mut seen_max_tokens: Vec<String> = Vec::new();
    let mut coverage: Option<Row> = None;

    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Row = serde_json::from_str(line)
            .unwrap_or_else(|e| panic!("bad oracle row: {e}\n{}", &line[..line.len().min(200)]));
        match row {
            Row::FieldPrompt { name, value } => {
                let ours = field_prompt(&name)
                    .unwrap_or_else(|| panic!("v5 has no FIELD_PROMPTS entry '{name}'"));
                eq(&format!("FIELD_PROMPTS.{name}"), ours, &value);
                seen_fields.push(name);
            }
            Row::Constant { name, value } => {
                let ours = match name.as_str() {
                    "PROPERTIES_PROMPT" => PROPERTIES_PROMPT,
                    "HEAD_AND_SHOULDERS_PHYSICAL_PROMPT" => HEAD_AND_SHOULDERS_PHYSICAL_PROMPT,
                    other => panic!("v5 has no wizard constant '{other}'"),
                };
                eq(&name, ours, &value);
                seen_constants.push(name);
            }
            Row::ContextPrompt {
                id,
                character_name,
                background,
                existing_data,
                image_description,
                document_content,
                out,
            } => {
                let got = build_context_prompt(
                    &character_name,
                    &background,
                    Some(&existing_data),
                    image_description.as_deref(),
                    document_content.as_deref(),
                );
                eq(&format!("buildContextPrompt '{id}'"), &got, &out);
                seen_contexts.push(id);
            }
            Row::SystemPromptField {
                direction,
                character_kind,
                value,
            } => {
                eq(
                    &format!("buildSystemPromptFieldPrompt('{}') [{direction}]", character_kind.as_str()),
                    build_system_prompt_field_prompt(character_kind),
                    &value,
                );
                seen_directions.push(direction);
            }
            Row::WizardFieldPrompt {
                field,
                character_kind,
                value,
            } => {
                let label = format!("wizardFieldPrompt('{field}', '{}')", character_kind.as_str());
                match (wizard_field_prompt(&field, character_kind), value.as_deref()) {
                    (Some(got), Some(want)) => eq(&label, got, want),
                    (None, None) => {}
                    (got, want) => panic!(
                        "{label}: v5 {} where v4 {}",
                        if got.is_some() { "answered" } else { "had none" },
                        if want.is_some() { "answered" } else { "had none" }
                    ),
                }
                seen_wizard_fields.push((field, character_kind));
            }
            Row::WizardFieldMaxTokens { field, value } => {
                assert_eq!(
                    wizard_field_max_tokens(&field),
                    value,
                    "wizardFieldMaxTokens('{field}')"
                );
                seen_max_tokens.push(field);
            }
            r @ Row::Coverage { .. } => coverage = Some(r),
        }
    }

    let Some(Row::Coverage {
        field_prompts,
        system_prompt_fields,
        wizard_fields,
        contexts,
    }) = coverage
    else {
        panic!("the oracle emitted no coverage row");
    };

    // Both directions, ORDER included: v5's generated table IS v4's insertion
    // order, so a reordering or a dropped key is a red.
    assert_eq!(seen_fields, field_prompts, "FIELD_PROMPTS coverage");
    let ours: Vec<String> = FIELD_PROMPTS.iter().map(|(k, _)| k.to_string()).collect();
    assert_eq!(ours, field_prompts, "v5's FIELD_PROMPTS key set and order");
    assert_eq!(seen_contexts, contexts, "buildContextPrompt coverage");
    assert_eq!(
        seen_constants,
        vec!["PROPERTIES_PROMPT", "HEAD_AND_SHOULDERS_PHYSICAL_PROMPT"],
        "constant coverage"
    );

    // P4.D371: the direction rows, and every (field, kind) pair, both ways.
    assert_eq!(seen_directions, system_prompt_fields, "system-prompt field coverage");
    let kinds = [CharacterKind::Roleplay, CharacterKind::Assistant];
    let want_pairs: Vec<(String, CharacterKind)> = wizard_fields
        .iter()
        .flat_map(|f| kinds.iter().map(move |k| (f.clone(), *k)))
        .collect();
    assert_eq!(seen_wizard_fields, want_pairs, "wizardFieldPrompt coverage");
    assert_eq!(seen_max_tokens, wizard_fields, "wizardFieldMaxTokens coverage");
    assert!(
        wizard_fields.len() >= 14,
        "wizard field corpus: {}",
        wizard_fields.len()
    );

    // Floors. NINE, measured at v4 `3ae4cf582` (ten before — see the header).
    assert!(
        field_prompts.len() >= 9,
        "FIELD_PROMPTS shrank below the measured floor: {}",
        field_prompts.len()
    );
    assert!(contexts.len() >= 12, "context corpus: {}", contexts.len());

    eprintln!(
        "OK: wizard prompts matched oracle ({} field prompts / {} constants / {} context prompts).",
        field_prompts.len(),
        seen_constants.len(),
        contexts.len()
    );
}
