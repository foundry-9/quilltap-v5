//! Storyteller mode — who is asked for a bid (v4 `lib/storyteller/invite.ts`,
//! P4.D343).
//!
//! Every present, non-archived, LLM-driven CHARACTER seat is invited. Silent
//! seats are invited too (their bid prompt says they may only act or emote).
//! User-driven seats and impersonated ones are not — the operator speaks for
//! them. With the chat's talkativeness filter on (off by default), each seat is
//! invited with probability `0.25 + 0.75 × talkativeness`; the rest are "kept
//! quiet" and listed on the board, where the operator can ask anyway.
//!
//! Pure: the caller supplies the random source.

use std::collections::HashMap;

use crate::chat_predicates::ParticipantStatus;
use crate::cycle_order::cycle_candidates;
use crate::participant_filters::is_user_driven_seat;
use crate::select_speaker::{SpeakerCharacter, SpeakerParticipant};

/// The character facts a seat reads (v4's `Map<string, Character>` narrowed).
#[derive(Debug, Clone, PartialEq)]
pub struct SeatCharacter {
    pub name: String,
    pub talkativeness: Option<f64>,
    pub archived: bool,
}

/// v4 `StorytellerSeat`.
#[derive(Debug, Clone)]
pub struct StorytellerSeat {
    pub participant: SpeakerParticipant,
    /// `character?.name` — `None` when the character was not loaded.
    pub character_name: Option<String>,
    pub talkativeness: f64,
    pub silent: bool,
}

/// The probability a seat is invited under the talkativeness filter (v4
/// `inviteProbability`): `clamp(0.25 + 0.75 × t, 0, 1)`.
pub fn invite_probability(talkativeness: f64) -> f64 {
    // JS `Math.min(1, Math.max(0, x))`; a NaN stays NaN in both.
    let x = 0.25 + 0.75 * talkativeness;
    if x.is_nan() {
        return x;
    }
    x.clamp(0.0, 1.0)
}

/// Every seat a round may ask for a bid, before the filter (v4
/// `biddableSeats`): the rotation's candidates minus the user-driven seats.
pub fn biddable_seats(
    participants: &[SpeakerParticipant],
    characters: &HashMap<String, SeatCharacter>,
    impersonating_participant_ids: Option<&[String]>,
) -> Vec<StorytellerSeat> {
    let speaker_chars: HashMap<String, SpeakerCharacter> = characters
        .iter()
        .map(|(id, c)| {
            (
                id.clone(),
                SpeakerCharacter {
                    talkativeness: c.talkativeness,
                    archived: c.archived,
                },
            )
        })
        .collect();
    let impersonating = impersonating_participant_ids.unwrap_or(&[]);
    cycle_candidates(participants, &speaker_chars)
        .into_iter()
        .filter(|p| !is_user_driven_seat(&p.id, &p.controlled_by, Some(impersonating)))
        .map(|p| {
            let character = p
                .character_id
                .as_deref()
                .and_then(|cid| characters.get(cid));
            StorytellerSeat {
                participant: p.clone(),
                character_name: character.map(|c| c.name.clone()),
                talkativeness: p
                    .talkativeness
                    .or_else(|| character.and_then(|c| c.talkativeness))
                    .unwrap_or(0.5),
                silent: p.status == ParticipantStatus::Silent,
            }
        })
        .collect()
}

/// v4 `chooseInvitees(seats, {filter, random})`: filter off → everyone; on →
/// ONE draw per seat, in order, `random() < inviteProbability(t)`.
pub fn choose_invitees(
    seats: Vec<StorytellerSeat>,
    filter: bool,
    random: &mut dyn FnMut() -> f64,
) -> (Vec<StorytellerSeat>, Vec<StorytellerSeat>) {
    if !filter {
        return (seats, Vec::new());
    }
    let mut invited = Vec::new();
    let mut kept_quiet = Vec::new();
    for seat in seats {
        if random() < invite_probability(seat.talkativeness) {
            invited.push(seat);
        } else {
            kept_quiet.push(seat);
        }
    }
    (invited, kept_quiet)
}
