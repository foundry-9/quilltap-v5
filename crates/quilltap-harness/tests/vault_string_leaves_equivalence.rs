//! Tier-1 differential test: the vault write-projection string leaves
//! (`slugifyWardrobeTitle`, `buildSlugByItemIdMap`, `sanitizeFileName`,
//! `buildSystemPromptFile` + the private `escapeYaml`, `buildScenarioFile`).
//!
//! Exact-equality against the v4 oracle. Decision-free vault (Family B) string
//! leaves — no eemeli/yaml (the prompt frontmatter is hand-built via `escapeYaml`
//! = `JSON.stringify`, the wardrobe YAML emitter is a separate step-7 slice), and
//! no `localeCompare` (slug case-folding is collation-safe — the `[^a-z0-9]→-`
//! filter neutralizes case-mapping divergence). Covers: slugification (caps,
//! spaces, dashes, unicode→dash, punctuation, empty), the first-wins slug map
//! (collision + empty-slug skip), filename sanitization (special chars,
//! whitespace-collapse, empty→`untitled`), prompt-file frontmatter (plain /
//! isDefault / the `escapeYaml` quote triggers `:`/`#`/`"`/`'`/`\n`), and the
//! frontmatter-less scenario file.
//!
//! Generate the oracle output:
//!   cd ~/source/quilltap-server
//!   npx tsx ~/source/quilltap-v5/harness/oracle/cases/vault-string-leaves.ts \
//!     > /tmp/oracle-vault-string-leaves.ndjson
//! Run:
//!   QT_ORACLE_VAULT_STRING_LEAVES=/tmp/oracle-vault-string-leaves.ndjson \
//!     cargo test -p quilltap-harness --test vault_string_leaves_equivalence

use quilltap_core::vault_overlay::{
    build_scenario_file, build_slug_by_item_id_map, build_system_prompt_file, sanitize_file_name,
    slugify_wardrobe_title,
};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(tag = "kind")]
enum Row {
    #[serde(rename = "slug")]
    Slug {
        id: String,
        title: String,
        out: String,
    },
    #[serde(rename = "slugMap")]
    SlugMap {
        id: String,
        items: Vec<(String, String)>,
        out: Vec<(String, String)>,
    },
    #[serde(rename = "sanitize")]
    Sanitize {
        id: String,
        name: String,
        out: String,
    },
    #[serde(rename = "promptFile")]
    PromptFile {
        id: String,
        name: String,
        #[serde(rename = "isDefault")]
        is_default: bool,
        content: String,
        out: String,
    },
    #[serde(rename = "scenarioFile")]
    ScenarioFile {
        id: String,
        title: String,
        content: String,
        out: String,
    },
}

#[test]
fn vault_string_leaves_match_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_VAULT_STRING_LEAVES") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_VAULT_STRING_LEAVES to the oracle NDJSON (see header).");
            return;
        }
    };
    let text =
        std::fs::read_to_string(&oracle_path).unwrap_or_else(|e| panic!("cannot read oracle: {e}"));

    let mut n = 0;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Row = serde_json::from_str(line).expect("parse oracle row");
        match row {
            Row::Slug { id, title, out } => {
                assert_eq!(slugify_wardrobe_title(&title), out, "[{id}] slugify");
            }
            Row::SlugMap { id, items, out } => {
                assert_eq!(build_slug_by_item_id_map(&items), out, "[{id}] slug map");
            }
            Row::Sanitize { id, name, out } => {
                assert_eq!(sanitize_file_name(&name), out, "[{id}] sanitize");
            }
            Row::PromptFile {
                id,
                name,
                is_default,
                content,
                out,
            } => {
                assert_eq!(
                    build_system_prompt_file(&name, is_default, &content),
                    out,
                    "[{id}] prompt file"
                );
            }
            Row::ScenarioFile {
                id,
                title,
                content,
                out,
            } => {
                assert_eq!(
                    build_scenario_file(&title, &content),
                    out,
                    "[{id}] scenario file"
                );
            }
        }
        n += 1;
    }
    assert!(n >= 24, "expected the full corpus, saw {n} rows");
    eprintln!("OK: vault string leaves matched oracle on {n} cases.");
}
