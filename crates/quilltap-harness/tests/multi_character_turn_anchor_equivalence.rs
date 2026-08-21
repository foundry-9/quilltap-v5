//! Tier-1 differential (P4.D96): `applyMultiCharacterTurnAnchor` +
//! `GROUP_SCENE_DISCIPLINE` — `crate::services::message_context` vs v4's REAL
//! `lib/services/chat-message/context-builder.service.ts` exports (`e22f7b36`).
//!
//! v4's `e22f7b36` restructured the anchor: it computes `systemIdx` first,
//! builds a `systemAdditions: string[]` (prose route pushes the identity
//! instruction FIRST, prefill route does not), ALWAYS pushes the anti-chorus
//! `GROUP_SCENE_DISCIPLINE` block, appends `'\n\n' + additions.join('\n\n')` to
//! the system message when one exists, and THEN — prefill route only — pushes
//! the assistant `[Name]` message. Net deltas: the prefill route now edits the
//! system message too (it used to return before any system read), and a prose
//! turn appends TWO blocks where a prefill turn appends ONE.
//!
//! No oracle drove this function before this round; the corpus covers both
//! routes × {system present, absent}, system-message placement (not first,
//! two systems → the first wins), an empty system content, an empty message
//! array, and four interpolated-name shapes (apostrophes, brackets,
//! non-ASCII, empty), each row diffed as the FULL post-call message array.
//!
//! Generate the oracle output:
//!   cd ~/source/quilltap-server
//!   npx tsx ~/source/quilltap-v5/harness/oracle/cases/multi-character-turn-anchor.ts \
//!     > /tmp/oracle-multi-character-turn-anchor.ndjson
//! Run:
//!   QT_ORACLE_TURN_ANCHOR=/tmp/oracle-multi-character-turn-anchor.ndjson \
//!     cargo test -p quilltap-harness --test multi_character_turn_anchor_equivalence

use quilltap_core::services::message_context::{
    apply_multi_character_turn_anchor, FormattedMsg, GROUP_SCENE_DISCIPLINE,
};
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Clone, PartialEq, Eq, Debug)]
struct WireMsg {
    role: String,
    content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(
        rename = "thoughtSignature",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    thought_signature: Option<String>,
}

impl WireMsg {
    fn to_core(&self) -> FormattedMsg {
        FormattedMsg {
            role: self.role.clone(),
            content: self.content.clone(),
            name: self.name.clone(),
            thought_signature: self.thought_signature.clone(),
            attachments: None,
        }
    }

    fn from_core(m: &FormattedMsg) -> Self {
        WireMsg {
            role: m.role.clone(),
            content: m.content.clone(),
            name: m.name.clone(),
            thought_signature: m.thought_signature.clone(),
        }
    }
}

#[derive(Deserialize)]
#[serde(tag = "kind")]
enum OracleRow {
    #[serde(rename = "discipline")]
    Discipline { id: String, text: String },
    #[serde(rename = "anchor")]
    Anchor {
        id: String,
        messages: Vec<WireMsg>,
        #[serde(rename = "characterName")]
        character_name: String,
        #[serde(rename = "usePrefill")]
        use_prefill: bool,
        out: Vec<WireMsg>,
    },
}

#[test]
fn multi_character_turn_anchor_equivalence() {
    let Ok(path) = std::env::var("QT_ORACLE_TURN_ANCHOR") else {
        eprintln!("QT_ORACLE_TURN_ANCHOR not set; skipping");
        return;
    };
    let data = std::fs::read_to_string(&path).expect("read oracle ndjson");
    let mut disciplines = 0usize;
    let mut anchors = 0usize;

    for line in data.lines().filter(|l| !l.trim().is_empty()) {
        let row: OracleRow = serde_json::from_str(line).expect("parse oracle row");
        match row {
            OracleRow::Discipline { id, text } => {
                assert_eq!(
                    GROUP_SCENE_DISCIPLINE, text,
                    "GROUP_SCENE_DISCIPLINE '{id}'"
                );
                disciplines += 1;
            }
            OracleRow::Anchor {
                id,
                messages,
                character_name,
                use_prefill,
                out,
            } => {
                let mut working: Vec<FormattedMsg> =
                    messages.iter().map(WireMsg::to_core).collect();
                apply_multi_character_turn_anchor(&mut working, &character_name, use_prefill);
                let got: Vec<WireMsg> = working.iter().map(WireMsg::from_core).collect();
                assert_eq!(got, out, "anchor '{id}'");
                anchors += 1;
            }
        }
    }

    // Shape assertions, not hand counts: every combination the corpus is built
    // to carry must actually be present.
    assert_eq!(disciplines, 1, "exactly one discipline-constant row");
    assert!(anchors >= 13, "anchor rows: {anchors}");
    eprintln!("multi-character-turn-anchor: {disciplines} discipline + {anchors} anchor rows");
}
