//! Tier-1 differential: `resolveFloorSeatId` — which seat the "your turn"
//! banner speaks for, and whose turn its Skip passes (v4 `2075242f9`, bug 146).
//!
//! Exact on the resolved id (a string or null). The corpus carries v4's own
//! seven `__tests__/unit/lib/chat/turn-manager/floor-seat.test.ts` cases (plus
//! the eighth v4 wrote on 2026-09-16, still uncommitted at the `2075242f9`
//! unification) plus
//! the shapes that suite does not ask — the verbatim un-validated fallback, the
//! JS-falsy empty-string floor id, and `find`'s first-match on duplicate ids.
//!
//! ⚠ PIN REQUIRED at the TARGET `2075242f9`: `resolveFloorSeatId` does not
//! exist at the `ffb6b3119` baseline, so a baseline-pinned run fails to IMPORT
//! (`does not provide an export named 'resolveFloorSeatId'`) — that failure is
//! the pin verification. Regen from the checkout once the baseline has moved
//! past `2075242f9`; until then point the driver at a pin (`--v4 "$PIN"`).
//!
//! Regenerate + run (self-contained; a pure tsx oracle, no fixture):
//!   V5W=${V5W:-$HOME/source/quilltap-v5}
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   rm -f /tmp/oracle-floor-seat.ndjson
//!   cd ~/source/quilltap-server
//!   $N/npx tsx $V5W/harness/oracle/cases/floor-seat.ts \
//!     > /tmp/oracle-floor-seat.ndjson
//!   cd $V5W
//!   QT_ORACLE_FLOOR_SEAT=/tmp/oracle-floor-seat.ndjson \
//!     cargo test -p quilltap-harness --test floor_seat_equivalence -- --nocapture

use quilltap_core::chat_predicates::participant_status_from_str;
use quilltap_core::participant_filters::{resolve_floor_seat_id, ParticipantView};
use serde::Deserialize;

#[derive(Deserialize, Clone)]
struct WirePart {
    id: String,
    #[serde(rename = "controlledBy")]
    controlled_by: String,
    status: String,
}

fn parts_to_core(ps: &[WirePart]) -> Vec<ParticipantView> {
    ps.iter()
        .map(|p| ParticipantView {
            id: p.id.clone(),
            // This corpus carries no `type`; `resolveFloorSeatId` reads only
            // id/status/controlledBy, and an empty literal keeps a future reader
            // that starts consulting the field from being masked.
            participant_type: String::new(),
            // §C: participant-status parsing has ONE home. (The older
            // `turn_pause_filters_equivalence` hand-rolls this match; it predates
            // the census and is carried there, not copied here.) That parser maps
            // anything unknown to `absent` rather than failing, so the corpus's
            // own spelling is checked first — a typo'd status would otherwise
            // become a silently-not-present seat and make its row vacuous.
            status: {
                assert!(
                    matches!(
                        p.status.as_str(),
                        "active" | "silent" | "absent" | "removed"
                    ),
                    "unknown status {} in the corpus",
                    p.status
                );
                participant_status_from_str(Some(p.status.as_str()))
            },
            controlled_by: p.controlled_by.clone(),
            character_id: Some(format!("char-{}", p.id)),
        })
        .collect()
}

/// How the v4 call spelled an argument — JS tells `null` from `undefined`, and
/// Rust's `Option` cannot, so the form rides the row and the mapping is pinned.
#[derive(Deserialize, PartialEq, Eq, Clone, Copy, Debug)]
#[serde(rename_all = "lowercase")]
enum Form {
    Value,
    Null,
    Undefined,
}

#[derive(Deserialize)]
#[serde(tag = "kind")]
enum OracleRow {
    #[serde(rename = "floor")]
    Floor {
        id: String,
        #[serde(rename = "nextSpeakerId")]
        next_speaker_id: Option<String>,
        #[serde(rename = "nextSpeakerForm")]
        next_speaker_form: Form,
        participants: Vec<WirePart>,
        impersonating: Option<Vec<String>>,
        #[serde(rename = "impersonatingForm")]
        impersonating_form: Form,
        #[serde(rename = "speakingSeatId")]
        speaking_seat_id: Option<String>,
        #[serde(rename = "speakingSeatForm")]
        speaking_seat_form: Form,
        out: Option<String>,
    },
    #[serde(rename = "reexport")]
    Reexport { id: String, out: bool },
}

#[test]
fn floor_seat_matches_oracle() {
    let path = match std::env::var("QT_ORACLE_FLOOR_SEAT") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_FLOOR_SEAT to the oracle NDJSON (see test header).");
            return;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));

    let (mut floors, mut reexports) = (0usize, 0usize);
    // The shapes the corpus must actually contain, so a future trimmed regen
    // cannot go green having stopped asking the questions.
    let (mut saw_undefined_form, mut saw_empty_string_floor, mut saw_null_out) =
        (false, false, false);

    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        match serde_json::from_str::<OracleRow>(line).unwrap() {
            OracleRow::Floor {
                id,
                next_speaker_id,
                next_speaker_form,
                participants,
                impersonating,
                impersonating_form,
                speaking_seat_id,
                speaking_seat_form,
                out,
            } => {
                // `null` and an omitted argument are the same input to the Rust
                // twin (both `None`), which is exactly how v4 reads them — the
                // form is carried so the corpus stays legible and so a row that
                // claims `undefined` is never silently a `value`.
                for (form, value) in [
                    (next_speaker_form, next_speaker_id.is_some()),
                    (impersonating_form, impersonating.is_some()),
                    (speaking_seat_form, speaking_seat_id.is_some()),
                ] {
                    assert_eq!(
                        form == Form::Value,
                        value,
                        "row '{id}': form {form:?} disagrees with the emitted value"
                    );
                }
                if next_speaker_form == Form::Undefined
                    || impersonating_form == Form::Undefined
                    || speaking_seat_form == Form::Undefined
                {
                    saw_undefined_form = true;
                }
                if next_speaker_id.as_deref() == Some("") {
                    saw_empty_string_floor = true;
                }
                if out.is_none() {
                    saw_null_out = true;
                }

                let core = parts_to_core(&participants);
                let got = resolve_floor_seat_id(
                    next_speaker_id.as_deref(),
                    &core,
                    impersonating.as_deref(),
                    speaking_seat_id.as_deref(),
                );
                assert_eq!(got, out, "floor '{id}'");
                floors += 1;
            }
            // v4 `index.ts:83`: the utils re-export block gained the function.
            // The oracle compares the barrel's binding against the one it drove.
            OracleRow::Reexport { id, out } => {
                assert!(
                    out,
                    "reexport '{id}': the barrel does not re-export the same function"
                );
                reexports += 1;
            }
        }
    }

    assert!(floors >= 40, "corpus too small: {floors} floor rows");
    assert_eq!(reexports, 1, "expected exactly one reexport row");
    assert!(
        saw_undefined_form,
        "corpus asks no `undefined`-spelled argument"
    );
    assert!(
        saw_empty_string_floor,
        "corpus asks no empty-string nextSpeakerId (the JS-falsy shape)"
    );
    assert!(saw_null_out, "corpus has no row resolving to null");
    eprintln!("OK: floor-seat matched oracle ({floors} floor rows, {reexports} reexport).");
}
