//! P4.9I2A tier-1 differential — the help-chat system prompt
//! (`quilltap_core::services::help_chat::system_prompt::build_help_chat_system_prompt`
//! vs v4's REAL `buildHelpChatSystemPrompt`, `lib/help-chat/system-prompt-builder.ts`),
//! byte-exact over the committed corpus (`harness/oracle/fixtures/
//! help-system-prompt.json`: 17 option combinations — no personality /
//! personality with `{{char}}`+`{{user}}`+`{{scenario}}`+`{{persona}}` / pronouns
//! full, partial (`undefined` interpolation) and null / tool instructions present,
//! absent and empty (which toggles the reinforcement sentence) / page context /
//! two additional contexts / user character null, present and empty-named /
//! other names 0, 1, 3).
//!
//! Generate (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   $N/node --import tsx $V5W/harness/oracle/cases/help-system-prompt.ts > /tmp/oracle-help-system-prompt.ndjson
//! Run:
//!   QT_ORACLE_HELP_SYSTEM_PROMPT=/tmp/oracle-help-system-prompt.ndjson \
//!     cargo test -p quilltap-harness --test help_system_prompt_equivalence

use std::collections::HashMap;

use quilltap_core::services::help_chat::context_resolver::{HelpPageContext, MatchType};
use quilltap_core::services::help_chat::system_prompt::{
    build_help_chat_system_prompt, HelpSystemPromptOptions, HelpUserCharacter,
};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
struct PcW {
    title: String,
    content: String,
    url: String,
}
#[derive(Deserialize)]
struct UcW {
    name: String,
    description: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CaseW {
    name: String,
    character: Value,
    #[serde(default)]
    user_character: Option<UcW>,
    #[serde(default)]
    page_context: Option<PcW>,
    #[serde(default)]
    additional_page_contexts: Option<Vec<PcW>>,
    #[serde(default)]
    other_character_names: Option<Vec<String>>,
    #[serde(default)]
    tool_instructions: Option<String>,
}
#[derive(Deserialize)]
struct Spec {
    cases: Vec<CaseW>,
}

fn pc(p: &PcW) -> HelpPageContext {
    HelpPageContext {
        title: p.title.clone(),
        content: p.content.clone(),
        url: p.url.clone(),
        match_type: MatchType::Exact,
        doc_id: String::new(),
    }
}

#[test]
fn help_system_prompt_matches_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_HELP_SYSTEM_PROMPT") else {
        eprintln!("SKIP: set QT_ORACLE_HELP_SYSTEM_PROMPT (see test header).");
        return;
    };
    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../harness/oracle/fixtures/help-system-prompt.json"),
        )
        .unwrap(),
    )
    .unwrap();
    let mut oracle: HashMap<String, String> = HashMap::new();
    for line in std::fs::read_to_string(&oracle_path)
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
    {
        let v: Value = serde_json::from_str(line).unwrap();
        oracle.insert(
            v["name"].as_str().unwrap().to_string(),
            v["prompt"].as_str().unwrap().to_string(),
        );
    }
    assert_eq!(oracle.len(), spec.cases.len());
    assert!(
        spec.cases.len() >= 12,
        "the corpus floor (order: ≥ 12 combinations)"
    );

    let mut failed = Vec::new();
    for c in &spec.cases {
        let uc = c.user_character.as_ref().map(|u| HelpUserCharacter {
            name: u.name.clone(),
            description: u.description.clone(),
        });
        let page = c.page_context.as_ref().map(pc);
        let additional: Vec<HelpPageContext> = c
            .additional_page_contexts
            .as_ref()
            .map(|v| v.iter().map(pc).collect())
            .unwrap_or_default();
        let got = build_help_chat_system_prompt(&HelpSystemPromptOptions {
            character: &c.character,
            user_character: uc.as_ref(),
            page_context: page.as_ref(),
            additional_page_contexts: &additional,
            other_character_names: c.other_character_names.as_deref(),
            tool_instructions: c.tool_instructions.as_deref(),
        });
        let want = &oracle[&c.name];
        if &got != want {
            let i = got
                .chars()
                .zip(want.chars())
                .position(|(a, b)| a != b)
                .unwrap_or(got.len().min(want.len()));
            eprintln!(
                "[{}] MISMATCH at char {i}:\n  GOT : {:?}\n  WANT: {:?}",
                c.name,
                got.chars()
                    .skip(i.saturating_sub(40))
                    .take(120)
                    .collect::<String>(),
                want.chars()
                    .skip(i.saturating_sub(40))
                    .take(120)
                    .collect::<String>()
            );
            failed.push(c.name.clone());
        }
    }
    assert!(failed.is_empty(), "help-system-prompt FAILED: {failed:?}");
}
