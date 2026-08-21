//! Tier-1 differential (P4.d2): the "nothing to add" turn-skipping pure module
//! — `crate::skip_signal` vs v4's REAL `lib/chat/turn-manager/skip-signal.ts`
//! exports (b90cd1f5).
//!
//! Covers `detectSkipSentinel` (the sentinel-line shedding matrix — wrappers
//! incl. mixed, bracketless, trailing punctuation, case, name-prefix + aliases,
//! content-block-wrapped, the no-name strip guard, sentinel+prose → cleaned,
//! sentinel-not-first-line → no skip), the `isTurnPassMessage` shape matrix,
//! `findSkippedSinceLastSubstantive` (whisper doesn't terminate; substantive
//! does; the set compared sorted), the `qualifiesForTurnSkipping` matrix,
//! `isFirstCharacterTurn` (greeting counts, Staff doesn't),
//! `isRecentlyAddressed` (v4 `e22f7b36`'s DIRECT-address rewrite: v4's own
//! eight new suite shapes 1:1, the vocative alternation's arms, the
//! no-usable-name null return — and its whisper-arm-still-wins twin — plus the
//! JS-regex fidelity vectors this port had to decide by measurement:
//! non-ASCII case folding, a regex-metacharacter name, and the `m`-flag / `\s`
//! edges reachable from message content [CRLF, lone CR at both ends, U+2028,
//! U+FEFF, U+0085, NBSP]), `buildTurnSkipInstruction`'s exact note bytes (the
//! grown base paragraph + the reworded caution — no differential covered them
//! before this round; `build_context_tier3` passes `turn_skip: None` on every
//! op), and the full `computeSkipEligibility`
//! withhold-precedence matrix incl. `summoned` and the stall guard. (The
//! literal vacuous-`.every()` over ZERO other active characters is unreachable
//! through `computeSkipEligibility` with a consistent roster — qualification
//! guarantees ≥2 active characters; the all-others-skipped rows exercise the
//! `.every()` non-trivially.)
//!
//! Generate the oracle output:
//!   cd ~/source/quilltap-server
//!   npx tsx ~/source/quilltap-v5/harness/oracle/cases/skip-signal.ts \
//!     > /tmp/oracle-skip-signal.ndjson
//! Run:
//!   QT_ORACLE_SKIP_SIGNAL=/tmp/oracle-skip-signal.ndjson \
//!     cargo test -p quilltap-harness --test skip_signal_equivalence

use quilltap_core::services::build_context::build_turn_skip_instruction;
use quilltap_core::skip_signal::{
    compute_skip_eligibility, detect_skip_sentinel, find_skipped_since_last_substantive,
    is_first_character_turn, is_recently_addressed, is_turn_pass_message,
    qualifies_for_turn_skipping, ComputeSkipEligibilityOptions, DetectSkipResult,
    RespondingCharacter,
};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize, Clone)]
struct WireCharacter {
    id: String,
    name: String,
    #[serde(default)]
    aliases: Option<Vec<String>>,
}

impl WireCharacter {
    fn to_core(&self) -> RespondingCharacter {
        RespondingCharacter {
            id: self.id.clone(),
            name: self.name.clone(),
            aliases: self.aliases.clone().unwrap_or_default(),
        }
    }
}

#[derive(Deserialize)]
struct WireDetect {
    skip: bool,
    #[serde(default)]
    cleaned: Option<String>,
}

#[derive(Deserialize)]
struct WireEligibility {
    #[serde(rename = "offerSkip")]
    offer_skip: bool,
    #[serde(rename = "mustSpeakReason")]
    must_speak_reason: Option<String>,
    #[serde(rename = "recentlyAddressed")]
    recently_addressed: bool,
}

#[derive(Deserialize)]
#[serde(tag = "kind")]
#[allow(clippy::enum_variant_names)]
enum OracleRow {
    #[serde(rename = "detect")]
    Detect {
        id: String,
        response: String,
        #[serde(rename = "characterName", default)]
        character_name: Option<String>,
        #[serde(default)]
        aliases: Option<Vec<String>>,
        out: WireDetect,
    },
    #[serde(rename = "turnPass")]
    TurnPass { id: String, event: Value, out: bool },
    #[serde(rename = "skippedSince")]
    SkippedSince {
        id: String,
        events: Vec<Value>,
        out: Vec<String>,
    },
    #[serde(rename = "qualifies")]
    Qualifies {
        id: String,
        participants: Vec<Value>,
        out: bool,
    },
    #[serde(rename = "firstTurn")]
    FirstTurn {
        id: String,
        events: Vec<Value>,
        out: bool,
    },
    #[serde(rename = "recentlyAddressed")]
    RecentlyAddressed {
        id: String,
        events: Vec<Value>,
        #[serde(rename = "respondingParticipantId")]
        responding_participant_id: String,
        character: WireCharacter,
        out: bool,
    },
    #[serde(rename = "turnSkipNote")]
    TurnSkipNote {
        id: String,
        #[serde(rename = "characterName")]
        character_name: String,
        #[serde(rename = "recentlyAddressed")]
        recently_addressed: bool,
        out: String,
    },
    #[serde(rename = "eligibility")]
    Eligibility {
        id: String,
        events: Vec<Value>,
        participants: Vec<Value>,
        #[serde(rename = "respondingParticipantId")]
        responding_participant_id: String,
        character: WireCharacter,
        summoned: bool,
        #[serde(rename = "turnSkippingEnabled")]
        turn_skipping_enabled: bool,
        out: WireEligibility,
    },
}

#[test]
fn skip_signal_equivalence() {
    let Ok(path) = std::env::var("QT_ORACLE_SKIP_SIGNAL") else {
        eprintln!("QT_ORACLE_SKIP_SIGNAL not set; skipping");
        return;
    };
    let data = std::fs::read_to_string(&path).expect("read oracle ndjson");
    let mut counts = [0usize; 8];
    for line in data.lines().filter(|l| !l.trim().is_empty()) {
        let row: OracleRow = serde_json::from_str(line).expect("parse oracle row");
        match row {
            OracleRow::Detect {
                id,
                response,
                character_name,
                aliases,
                out,
            } => {
                let got =
                    detect_skip_sentinel(&response, character_name.as_deref(), aliases.as_deref());
                let (got_skip, got_cleaned) = match got {
                    DetectSkipResult::Skip => (true, None),
                    DetectSkipResult::NoSkip { cleaned } => (false, cleaned),
                };
                assert_eq!(got_skip, out.skip, "detect '{id}' skip");
                assert_eq!(got_cleaned, out.cleaned, "detect '{id}' cleaned");
                counts[0] += 1;
            }
            OracleRow::TurnPass { id, event, out } => {
                assert_eq!(is_turn_pass_message(&event), out, "turnPass '{id}'");
                counts[1] += 1;
            }
            OracleRow::SkippedSince { id, events, out } => {
                let mut got: Vec<String> = find_skipped_since_last_substantive(&events)
                    .into_iter()
                    .collect();
                got.sort();
                assert_eq!(got, out, "skippedSince '{id}'");
                counts[2] += 1;
            }
            OracleRow::Qualifies {
                id,
                participants,
                out,
            } => {
                assert_eq!(
                    qualifies_for_turn_skipping(&participants),
                    out,
                    "qualifies '{id}'"
                );
                counts[3] += 1;
            }
            OracleRow::FirstTurn { id, events, out } => {
                assert_eq!(is_first_character_turn(&events), out, "firstTurn '{id}'");
                counts[4] += 1;
            }
            OracleRow::RecentlyAddressed {
                id,
                events,
                responding_participant_id,
                character,
                out,
            } => {
                assert_eq!(
                    is_recently_addressed(
                        &events,
                        &responding_participant_id,
                        &character.to_core()
                    ),
                    out,
                    "recentlyAddressed '{id}'"
                );
                counts[5] += 1;
            }
            OracleRow::TurnSkipNote {
                id,
                character_name,
                recently_addressed,
                out,
            } => {
                assert_eq!(
                    build_turn_skip_instruction(&character_name, recently_addressed),
                    out,
                    "turnSkipNote '{id}'"
                );
                counts[7] += 1;
            }
            OracleRow::Eligibility {
                id,
                events,
                participants,
                responding_participant_id,
                character,
                summoned,
                turn_skipping_enabled,
                out,
            } => {
                let got = compute_skip_eligibility(&ComputeSkipEligibilityOptions {
                    events: &events,
                    participants: &participants,
                    responding_participant_id: &responding_participant_id,
                    responding_character: &character.to_core(),
                    summoned,
                    turn_skipping_enabled,
                });
                assert_eq!(
                    got.offer_skip, out.offer_skip,
                    "eligibility '{id}' offerSkip"
                );
                assert_eq!(
                    got.must_speak_reason.map(|r| r.as_str().to_string()),
                    out.must_speak_reason,
                    "eligibility '{id}' mustSpeakReason"
                );
                assert_eq!(
                    got.recently_addressed, out.recently_addressed,
                    "eligibility '{id}' recentlyAddressed"
                );
                counts[6] += 1;
            }
        }
    }
    let total: usize = counts.iter().sum();
    assert!(
        counts.iter().all(|&c| c > 0),
        "every kind exercised: {counts:?}"
    );
    println!("skip_signal_equivalence: {total} rows green ({counts:?})");
}
