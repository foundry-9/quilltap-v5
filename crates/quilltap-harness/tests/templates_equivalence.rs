//! Tier-1 differential test: template processing (chat-orchestration wave 1.1,
//! Part A). Covers processTemplate, buildTemplateContext,
//! processCharacterTemplates — pure functions, exact string equality.
//!
//! Generate the oracle output:
//!   cd ~/source/quilltap-server
//!   npx tsx ~/source/quilltap-v5/harness/oracle/cases/templates.ts \
//!     > /tmp/oracle-templates.ndjson
//! Run:
//!   QT_ORACLE_TEMPLATES=/tmp/oracle-templates.ndjson cargo test -p quilltap-harness \
//!     --test templates_equivalence

use std::collections::HashMap;

use quilltap_core::templates::{
    build_template_context, process_character_templates, process_template, TemplateCharacter,
    TemplateContext, TemplateUserCharacter,
};
use serde::Deserialize;

/// A character as the oracle serializes it (both build + char cases). Optional
/// fields are `Option<Option<String>>`: absent (`None`) vs JSON `null`
/// (`Some(None)`) vs a value — but v4 treats absent and null identically
/// (`|| ''`), so we flatten with `serde(default)` + `Option<String>` where the
/// distinction never matters.
#[derive(Deserialize, Default)]
struct WireCharacter {
    name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    manifesto: Option<String>,
    #[serde(default)]
    personality: Option<String>,
    #[serde(rename = "firstMessage", default)]
    first_message: Option<String>,
    #[serde(rename = "exampleDialogues", default)]
    example_dialogues: Option<String>,
}

#[derive(Deserialize)]
struct WireUserCharacter {
    name: String,
    #[serde(default)]
    description: Option<String>,
}

#[derive(Deserialize)]
struct WireProcessed {
    description: String,
    manifesto: String,
    personality: String,
    scenario: String,
    #[serde(rename = "firstMessage")]
    first_message: String,
    #[serde(rename = "exampleDialogues")]
    example_dialogues: String,
    #[serde(rename = "systemPrompt")]
    system_prompt: String,
}

#[derive(Deserialize)]
#[serde(tag = "kind")]
enum Row {
    #[serde(rename = "process")]
    Process {
        id: String,
        template: String,
        context: HashMap<String, String>,
        out: String,
    },
    #[serde(rename = "build")]
    Build {
        id: String,
        character: WireCharacter,
        #[serde(rename = "userCharacter", default)]
        user_character: Option<WireUserCharacter>,
        #[serde(default)]
        scenario: Option<String>,
        #[serde(rename = "systemPrompt", default)]
        system_prompt: Option<String>,
        /// The full emitted context map (16 keys, all strings).
        out: HashMap<String, String>,
    },
    #[serde(rename = "char")]
    Char {
        id: String,
        character: WireCharacter,
        #[serde(rename = "userCharacter", default)]
        user_character: Option<WireUserCharacter>,
        #[serde(default)]
        scenario: Option<String>,
        #[serde(rename = "systemPrompt", default)]
        system_prompt: Option<String>,
        out: WireProcessed,
    },
}

fn to_core_char(w: &WireCharacter) -> TemplateCharacter {
    TemplateCharacter {
        name: w.name.clone(),
        description: w.description.clone(),
        manifesto: w.manifesto.clone(),
        personality: w.personality.clone(),
        first_message: w.first_message.clone(),
        example_dialogues: w.example_dialogues.clone(),
    }
}

fn to_core_user(w: &Option<WireUserCharacter>) -> Option<TemplateUserCharacter> {
    w.as_ref().map(|u| TemplateUserCharacter {
        name: u.name.clone(),
        description: u.description.clone(),
    })
}

/// Turn a name→value map into a `TemplateContext` (the shape
/// `process_template` consumes).
fn context_from_map(m: &HashMap<String, String>) -> TemplateContext {
    let mut c = TemplateContext::default();
    for (k, v) in m {
        c.set(k, v.clone());
    }
    c
}

#[test]
fn templates_match_oracle() {
    let path = match std::env::var("QT_ORACLE_TEMPLATES") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_TEMPLATES to the oracle NDJSON (see test header).");
            return;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));

    let mut count = 0usize;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        match serde_json::from_str::<Row>(line).unwrap() {
            Row::Process {
                id,
                template,
                context,
                out,
            } => {
                let ctx = context_from_map(&context);
                let got = process_template(&template, &ctx);
                assert_eq!(got, out, "process '{id}'");
            }
            Row::Build {
                id,
                character,
                user_character,
                scenario,
                system_prompt,
                out,
            } => {
                let ch = to_core_char(&character);
                let uc = to_core_user(&user_character);
                let ctx = build_template_context(
                    &ch,
                    uc.as_ref(),
                    scenario.as_deref(),
                    system_prompt.as_deref(),
                );
                // Every key v4 emits must be present with the exact value.
                for (k, v) in &out {
                    assert_eq!(ctx.get(k), Some(v.as_str()), "build '{id}' key '{k}'");
                }
            }
            Row::Char {
                id,
                character,
                user_character,
                scenario,
                system_prompt,
                out,
            } => {
                let ch = to_core_char(&character);
                let uc = to_core_user(&user_character);
                let got = process_character_templates(
                    &ch,
                    uc.as_ref(),
                    scenario.as_deref(),
                    system_prompt.as_deref(),
                );
                assert_eq!(got.description, out.description, "char '{id}' description");
                assert_eq!(got.manifesto, out.manifesto, "char '{id}' manifesto");
                assert_eq!(got.personality, out.personality, "char '{id}' personality");
                assert_eq!(got.scenario, out.scenario, "char '{id}' scenario");
                assert_eq!(
                    got.first_message, out.first_message,
                    "char '{id}' firstMessage"
                );
                assert_eq!(
                    got.example_dialogues, out.example_dialogues,
                    "char '{id}' exampleDialogues"
                );
                assert_eq!(
                    got.system_prompt, out.system_prompt,
                    "char '{id}' systemPrompt"
                );
            }
        }
        count += 1;
    }

    assert!(count > 0, "oracle file looks empty: {count}");
    eprintln!("OK: templates matched oracle ({count} rows).");
}
