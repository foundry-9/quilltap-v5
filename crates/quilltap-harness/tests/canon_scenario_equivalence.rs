//! Tier-1 differential test #21 (Wave 4 / B12): canon-block renderers and the
//! scenario-text combiner — exact string / structural equality against the v4
//! oracle.
//!
//! Generate the oracle output:
//!   cd ~/source/quilltap-server
//!   npx tsx ~/source/quilltap-v5/harness/oracle/cases/canon-scenario.ts \
//!     > /tmp/oracle-canon-scenario.ndjson
//! Run:
//!   QT_ORACLE_CANON_SCENARIO=/tmp/oracle-canon-scenario.ndjson \
//!     cargo test -p quilltap-harness

use quilltap_core::canon::{
    load_canon_for_self, render_other_canon_block, render_self_canon_block, CanonSource,
    CanonSourceKind, SelfCanon, NO_CANON_FALLBACK,
};
use quilltap_core::scenario_text::combine_scenario_text;
use serde::Deserialize;

#[derive(Deserialize)]
struct WSelfCanon {
    #[serde(rename = "characterId")]
    character_id: String,
    #[serde(rename = "characterName")]
    character_name: String,
    manifesto: Option<String>,
    personality: Option<String>,
    description: Option<String>,
    identity: Option<String>,
}

impl WSelfCanon {
    fn to_core(&self) -> SelfCanon {
        SelfCanon {
            character_id: self.character_id.clone(),
            character_name: self.character_name.clone(),
            manifesto: self.manifesto.clone(),
            personality: self.personality.clone(),
            description: self.description.clone(),
            identity: self.identity.clone(),
        }
    }
}

#[derive(Deserialize)]
struct WCanonSource {
    #[serde(rename = "characterId")]
    character_id: String,
    #[serde(rename = "characterName")]
    character_name: String,
    body: Option<String>,
    source: String,
}

#[derive(Deserialize)]
struct WLoadChar {
    id: String,
    name: String,
    manifesto: Option<String>,
    personality: Option<String>,
    description: Option<String>,
    identity: Option<String>,
}

#[derive(Deserialize)]
#[serde(tag = "kind")]
enum Row {
    #[serde(rename = "selfBlock")]
    SelfBlock {
        id: String,
        canon: WSelfCanon,
        out: String,
    },
    #[serde(rename = "otherBlock")]
    OtherBlock {
        id: String,
        canon: WCanonSource,
        out: String,
    },
    #[serde(rename = "loadSelf")]
    LoadSelf {
        id: String,
        character: WLoadChar,
        out: WSelfCanon,
    },
    #[serde(rename = "scenario")]
    Scenario {
        id: String,
        #[serde(rename = "presetBody")]
        preset_body: Option<String>,
        #[serde(rename = "freeText")]
        free_text: Option<String>,
        out: Option<String>,
    },
    #[serde(rename = "fallbackConst")]
    FallbackConst { value: String },
}

fn parse_source(s: &str) -> CanonSourceKind {
    match s {
        "vault" => CanonSourceKind::Vault,
        "identity" => CanonSourceKind::Identity,
        "description" => CanonSourceKind::Description,
        "none" => CanonSourceKind::None,
        other => panic!("unknown canon source: {other}"),
    }
}

#[test]
fn canon_scenario_matches_oracle() {
    let path = match std::env::var("QT_ORACLE_CANON_SCENARIO") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_CANON_SCENARIO to the oracle NDJSON (see test header).");
            return;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));

    let mut count = 0usize;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        match serde_json::from_str::<Row>(line).unwrap() {
            Row::SelfBlock { id, canon, out } => {
                let got = render_self_canon_block(&canon.to_core());
                assert_eq!(got, out, "selfBlock '{id}'");
            }
            Row::OtherBlock { id, canon, out } => {
                let core = CanonSource {
                    character_id: canon.character_id,
                    character_name: canon.character_name,
                    body: canon.body,
                    source: parse_source(&canon.source),
                };
                let got = render_other_canon_block(&core);
                assert_eq!(got, out, "otherBlock '{id}'");
            }
            Row::LoadSelf { id, character, out } => {
                let got = load_canon_for_self(
                    &character.id,
                    &character.name,
                    character.manifesto.as_deref(),
                    character.personality.as_deref(),
                    character.description.as_deref(),
                    character.identity.as_deref(),
                );
                assert_eq!(got, out.to_core(), "loadSelf '{id}'");
            }
            Row::Scenario {
                id,
                preset_body,
                free_text,
                out,
            } => {
                let got = combine_scenario_text(preset_body.as_deref(), free_text.as_deref());
                assert_eq!(got, out, "scenario '{id}'");
            }
            Row::FallbackConst { value } => {
                assert_eq!(NO_CANON_FALLBACK, value, "fallbackConst");
            }
        }
        count += 1;
    }

    assert!(count > 0, "oracle file looks empty: {count}");
    eprintln!("OK: canon-scenario matched oracle ({count} rows).");
}
