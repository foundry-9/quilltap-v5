//! Tier-1 differential test: the vault frontmatter READ parsers
//! (`parsePromptFile`, `parseScenarioFile`).
//!
//! Exact-equality against the v4 oracle. Exercises the frontmatter
//! `name`/`isDefault`/`description` reads, the body slice + `trimStart`, the
//! `# heading` / filename title fallbacks, the UTF-16 `.trim().slice(0, n)` caps
//! (name ≤100, title ≤200, description ≤500), the `isDefault === true` strictness,
//! the skip conditions (no frontmatter / no name / empty body / no usable title),
//! and the `stableUuidFromString` ids — over a corpus that includes multibyte
//! content to cover the UTF-16 body offset.
//!
//! Generate the oracle output:
//!   cd ~/source/quilltap-server
//!   npx tsx ~/source/quilltap-v5/harness/oracle/cases/vault-frontmatter-parsers.ts \
//!     > /tmp/oracle-vault-frontmatter-parsers.ndjson
//! Run:
//!   QT_ORACLE_VAULT_FRONTMATTER_PARSERS=/tmp/oracle-vault-frontmatter-parsers.ndjson \
//!     cargo test -p quilltap-harness --test vault_frontmatter_parsers_equivalence

use quilltap_core::vault_overlay::{parse_prompt_file, parse_scenario_file, VaultDoc};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
struct Doc {
    content: String,
    #[serde(rename = "mountPointId")]
    mount_point_id: String,
    #[serde(rename = "relativePath")]
    relative_path: String,
    #[serde(rename = "fileName")]
    file_name: String,
    #[serde(rename = "createdAt")]
    created_at: String,
    #[serde(rename = "updatedAt")]
    updated_at: String,
}

#[derive(Deserialize)]
#[serde(tag = "kind")]
enum Row {
    #[serde(rename = "prompt")]
    Prompt { id: String, doc: Doc, out: Value },
    #[serde(rename = "scenario")]
    Scenario { id: String, doc: Doc, out: Value },
}

fn as_doc(d: &Doc) -> VaultDoc<'_> {
    VaultDoc {
        content: &d.content,
        mount_point_id: &d.mount_point_id,
        relative_path: &d.relative_path,
        file_name: &d.file_name,
        created_at: &d.created_at,
        updated_at: &d.updated_at,
    }
}

#[test]
fn frontmatter_parsers_match_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_VAULT_FRONTMATTER_PARSERS") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_ORACLE_VAULT_FRONTMATTER_PARSERS to the oracle NDJSON (see header)."
            );
            return;
        }
    };
    let text =
        std::fs::read_to_string(&oracle_path).unwrap_or_else(|e| panic!("cannot read oracle: {e}"));

    let mut n = 0;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Row = serde_json::from_str(line).expect("parse oracle row");
        match row {
            Row::Prompt { id, doc, out } => {
                let got = parse_prompt_file(&as_doc(&doc))
                    .map(|p| serde_json::to_value(p).unwrap())
                    .unwrap_or(Value::Null);
                assert_eq!(got, out, "[{id}] parse_prompt_file");
            }
            Row::Scenario { id, doc, out } => {
                let got = parse_scenario_file(&as_doc(&doc))
                    .map(|s| serde_json::to_value(s).unwrap())
                    .unwrap_or(Value::Null);
                assert_eq!(got, out, "[{id}] parse_scenario_file");
            }
        }
        n += 1;
    }
    assert!(n >= 26, "expected the full corpus, saw {n} rows");
    eprintln!("OK: vault frontmatter parsers matched oracle on {n} cases.");
}
