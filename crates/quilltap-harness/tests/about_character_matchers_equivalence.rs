//! Tier-1 differential test #22 (Wave 5 / B13): word-boundary about-character
//! name matchers — nameAppears / countNameOccurrences / resolveAboutCharacterId,
//! exact bool / count / structural equality against the v4 oracle. Exercises the
//! Unicode-boundary + lookahead regex reproduced without a backtracking engine.
//!
//! Generate the oracle output:
//!   cd ~/source/quilltap-server
//!   npx tsx ~/source/quilltap-v5/harness/oracle/cases/about-character-matchers.ts \
//!     > /tmp/oracle-about-character-matchers.ndjson
//! Run:
//!   QT_ORACLE_ABOUT_CHARACTER_MATCHERS=/tmp/oracle-about-character-matchers.ndjson \
//!     cargo test -p quilltap-harness

use quilltap_core::about_character::{
    count_name_occurrences, name_appears, resolve_about_character_id, AboutFlipReason,
};
use serde::Deserialize;

#[derive(Deserialize)]
struct WHolder {
    name: String,
    #[serde(default)]
    aliases: Vec<String>,
}

#[derive(Deserialize)]
struct WAbout {
    name: String,
    #[serde(default)]
    aliases: Vec<String>,
    #[serde(rename = "controlledBy")]
    controlled_by: String,
}

#[derive(Deserialize)]
struct WResolveOut {
    #[serde(rename = "aboutCharacterId")]
    about_character_id: Option<String>,
    flipped: bool,
    #[serde(default)]
    reason: Option<String>,
}

#[derive(Deserialize)]
#[serde(tag = "kind")]
enum Row {
    #[serde(rename = "appears")]
    Appears {
        id: String,
        names: Vec<String>,
        haystack: String,
        out: bool,
    },
    #[serde(rename = "count")]
    Count {
        id: String,
        names: Vec<String>,
        haystack: String,
        out: usize,
    },
    #[serde(rename = "resolve")]
    Resolve {
        id: String,
        #[serde(rename = "holderCharacterId")]
        holder_character_id: String,
        #[serde(rename = "holderCharacter")]
        holder_character: Option<WHolder>,
        #[serde(rename = "proposedAboutCharacterId")]
        proposed_about_character_id: Option<String>,
        #[serde(rename = "proposedAboutCharacter")]
        proposed_about_character: Option<WAbout>,
        text: String,
        out: WResolveOut,
    },
}

fn reason_str(r: Option<AboutFlipReason>) -> Option<String> {
    r.map(|r| match r {
        AboutFlipReason::HolderDominates => "holder-dominates".to_string(),
    })
}

#[test]
fn about_character_matchers_match_oracle() {
    let path = match std::env::var("QT_ORACLE_ABOUT_CHARACTER_MATCHERS") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_ABOUT_CHARACTER_MATCHERS to the oracle NDJSON (see test header).");
            return;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));

    let mut count = 0usize;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        match serde_json::from_str::<Row>(line).unwrap() {
            Row::Appears {
                id,
                names,
                haystack,
                out,
            } => {
                assert_eq!(name_appears(&names, &haystack), out, "appears '{id}'");
            }
            Row::Count {
                id,
                names,
                haystack,
                out,
            } => {
                assert_eq!(
                    count_name_occurrences(&names, &haystack),
                    out,
                    "count '{id}'"
                );
            }
            Row::Resolve {
                id,
                holder_character_id,
                holder_character,
                proposed_about_character_id,
                proposed_about_character,
                text: memory_text,
                out,
            } => {
                let holder = holder_character
                    .as_ref()
                    .map(|h| (h.name.as_str(), h.aliases.as_slice()));
                let about = proposed_about_character.as_ref().map(|a| {
                    (
                        a.name.as_str(),
                        a.aliases.as_slice(),
                        a.controlled_by.as_str(),
                    )
                });
                let got = resolve_about_character_id(
                    &holder_character_id,
                    holder,
                    proposed_about_character_id.as_deref(),
                    about,
                    &memory_text,
                );
                assert_eq!(
                    got.about_character_id, out.about_character_id,
                    "resolve '{id}': id"
                );
                assert_eq!(got.flipped, out.flipped, "resolve '{id}': flipped");
                assert_eq!(reason_str(got.reason), out.reason, "resolve '{id}': reason");
            }
        }
        count += 1;
    }

    assert!(count > 0, "oracle file looks empty: {count}");
    eprintln!("OK: about-character-matchers matched oracle ({count} rows).");
}
