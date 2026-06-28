//! Tier-1 differential test #19 (Wave 3 / B10): pure memory name-resolution
//! leaves — calculateReinforcedImportance (float within 1e-12),
//! formatNameWithPronouns, namesForAboutCharacter, namesForHolder (exact
//! string/array equality).
//!
//! Generate the oracle output:
//!   cd ~/source/quilltap-server
//!   npx tsx ~/source/quilltap-v5/harness/oracle/cases/memory-name-helpers.ts \
//!     > /tmp/oracle-memory-name-helpers.ndjson
//! Run:
//!   QT_ORACLE_MEMORY_NAME_HELPERS=/tmp/oracle-memory-name-helpers.ndjson \
//!     cargo test -p quilltap-harness

use quilltap_core::about_character::{names_for_about_character, names_for_holder};
use quilltap_core::memory_format::{format_name_with_pronouns, Pronouns};
use quilltap_core::memory_gate::calculate_reinforced_importance;
use serde::Deserialize;

#[derive(Deserialize)]
struct WPronouns {
    subject: String,
    object: String,
    possessive: String,
}

#[derive(Deserialize)]
struct WChar {
    name: String,
    #[serde(default)]
    aliases: Vec<String>,
    #[serde(rename = "controlledBy", default)]
    controlled_by: Option<String>,
}

#[derive(Deserialize)]
#[serde(tag = "kind")]
enum Row {
    #[serde(rename = "reinforced")]
    Reinforced {
        id: String,
        base: f64,
        count: f64,
        out: f64,
    },
    #[serde(rename = "format")]
    Format {
        id: String,
        name: String,
        pronouns: Option<WPronouns>,
        out: String,
    },
    #[serde(rename = "about")]
    About {
        id: String,
        character: WChar,
        out: Vec<String>,
    },
    #[serde(rename = "holder")]
    Holder {
        id: String,
        character: WChar,
        out: Vec<String>,
    },
}

#[test]
fn memory_name_helpers_match_oracle() {
    let path = match std::env::var("QT_ORACLE_MEMORY_NAME_HELPERS") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_ORACLE_MEMORY_NAME_HELPERS to the oracle NDJSON (see test header)."
            );
            return;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));

    let mut count = 0usize;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        match serde_json::from_str::<Row>(line).unwrap() {
            Row::Reinforced {
                id,
                base,
                count: c,
                out,
            } => {
                let got = calculate_reinforced_importance(base, c);
                assert!(
                    (got - out).abs() < 1e-12,
                    "reinforced '{id}': rust={got} oracle={out}"
                );
            }
            Row::Format {
                id,
                name,
                pronouns,
                out,
            } => {
                let p = pronouns.map(|w| Pronouns {
                    subject: w.subject,
                    object: w.object,
                    possessive: w.possessive,
                });
                let got = format_name_with_pronouns(&name, p.as_ref());
                assert_eq!(got, out, "format '{id}'");
            }
            Row::About { id, character, out } => {
                let got = names_for_about_character(
                    &character.name,
                    &character.aliases,
                    character.controlled_by.as_deref().unwrap_or(""),
                );
                assert_eq!(got, out, "about '{id}'");
            }
            Row::Holder { id, character, out } => {
                let got = names_for_holder(&character.name, &character.aliases);
                assert_eq!(got, out, "holder '{id}'");
            }
        }
        count += 1;
    }

    assert!(count > 0, "oracle file looks empty: {count}");
    eprintln!("OK: memory-name-helpers matched oracle ({count} rows).");
}
