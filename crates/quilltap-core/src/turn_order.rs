//! Port of v4's lib/chat/turn-manager/turn-order.ts — the predicted turn order
//! shown in the participant sidebar. Display-only: it does NOT drive actual turn
//! selection.
//!
//! Ordering priority: (1) currently generating, (2) next speaker, (3) queue,
//! (4) eligible (not spoken, not last, not user, LLM) by talkativeness desc,
//! (5) the user character, (6) already-spoken by talkativeness desc, (7)
//! inactive (no position).

use std::collections::HashSet;

/// A participant as the sidebar reads it. `status` defaults to "active" when
/// absent/empty (v4's `p.status || 'active'`); `talkativeness` defaults to 0.5.
#[derive(Clone, Debug)]
pub struct TurnOrderParticipant {
    pub id: String,
    pub status: Option<String>,
    pub controlled_by: Option<String>,
    pub talkativeness: Option<f64>,
}

impl TurnOrderParticipant {
    fn is_present(&self) -> bool {
        let s = self
            .status
            .as_deref()
            .filter(|x| !x.is_empty())
            .unwrap_or("active");
        s == "active" || s == "silent"
    }
    fn talkativeness(&self) -> f64 {
        self.talkativeness.unwrap_or(0.5)
    }
}

/// One entry in the predicted order. `position` is 1-based, or `None` for the
/// inactive tail.
#[derive(Clone, Debug, PartialEq)]
pub struct TurnOrderEntry {
    pub participant_id: String,
    pub position: Option<i64>,
    pub status: String,
}

fn add_entry(
    entries: &mut Vec<TurnOrderEntry>,
    placed: &mut HashSet<String>,
    participants: &[TurnOrderParticipant],
    participant_id: &str,
    status: &str,
) {
    if placed.contains(participant_id) {
        return;
    }
    if !participants.iter().any(|p| p.id == participant_id) {
        return;
    }
    placed.insert(participant_id.to_string());
    let position = if status == "inactive" {
        None
    } else {
        // Count of non-inactive entries placed so far, +1. (Steps 1–6 only ever
        // add non-inactive statuses, so this is effectively sequential.)
        Some(entries.iter().filter(|e| e.status != "inactive").count() as i64 + 1)
    };
    entries.push(TurnOrderEntry {
        participant_id: participant_id.to_string(),
        position,
        status: status.to_string(),
    });
}

/// Compute the predicted turn order for display.
///
/// `next_speaker_id` should already fold v4's `turnSelectionResult?.nextSpeakerId`
/// guard: pass the truthy next-speaker id, or `None` when there is no selection
/// result or its `nextSpeakerId` is null. `responding_participant_id` is honored
/// only when `is_generating` is true.
#[allow(clippy::too_many_arguments)]
pub fn compute_predicted_turn_order(
    participants: &[TurnOrderParticipant],
    queue: &[String],
    spoken_since_user_turn: &[String],
    last_speaker_id: Option<&str>,
    next_speaker_id: Option<&str>,
    is_generating: bool,
    responding_participant_id: Option<&str>,
    user_participant_id: Option<&str>,
) -> Vec<TurnOrderEntry> {
    let mut entries: Vec<TurnOrderEntry> = Vec::new();
    let mut placed: HashSet<String> = HashSet::new();

    // 1. Currently generating.
    if is_generating {
        if let Some(r) = responding_participant_id.filter(|s| !s.is_empty()) {
            add_entry(&mut entries, &mut placed, participants, r, "generating");
        }
    }

    // 2. Next speaker.
    if let Some(ns) = next_speaker_id.filter(|s| !s.is_empty()) {
        if !placed.contains(ns) {
            add_entry(&mut entries, &mut placed, participants, ns, "next");
        }
    }

    // 3. Queue (in order).
    for q in queue {
        add_entry(&mut entries, &mut placed, participants, q, "queued");
    }

    let active: Vec<&TurnOrderParticipant> =
        participants.iter().filter(|p| p.is_present()).collect();
    let inactive: Vec<&TurnOrderParticipant> =
        participants.iter().filter(|p| !p.is_present()).collect();

    // 4. Eligible: active, not placed, not the user, not spoken, not last
    //    speaker, not user-controlled — sorted by talkativeness descending.
    let mut eligible: Vec<&TurnOrderParticipant> = active
        .iter()
        .copied()
        .filter(|p| {
            !placed.contains(&p.id)
                && Some(p.id.as_str()) != user_participant_id
                && !spoken_since_user_turn.iter().any(|s| s == &p.id)
                && Some(p.id.as_str()) != last_speaker_id
                && p.controlled_by.as_deref() != Some("user")
        })
        .collect();
    eligible.sort_by(|a, b| {
        b.talkativeness()
            .partial_cmp(&a.talkativeness())
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    for p in &eligible {
        add_entry(&mut entries, &mut placed, participants, &p.id, "eligible");
    }

    // 5. The user character at their cycle slot.
    if let Some(uid) = user_participant_id {
        if !placed.contains(uid) {
            if let Some(up) = participants.iter().find(|p| p.id == uid) {
                if up.is_present() {
                    add_entry(&mut entries, &mut placed, participants, uid, "user-turn");
                }
            }
        }
    }

    // 6. Already-spoken: remaining active, by talkativeness descending.
    let mut spoken: Vec<&TurnOrderParticipant> = active
        .iter()
        .copied()
        .filter(|p| !placed.contains(&p.id))
        .collect();
    spoken.sort_by(|a, b| {
        b.talkativeness()
            .partial_cmp(&a.talkativeness())
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    for p in &spoken {
        add_entry(&mut entries, &mut placed, participants, &p.id, "spoken");
    }

    // 7. Inactive tail (no position). Maps raw status absent/silent, else inactive.
    for p in &inactive {
        if !placed.contains(&p.id) {
            placed.insert(p.id.clone());
            let status = match p.status.as_deref() {
                Some("absent") => "absent",
                Some("silent") => "silent",
                _ => "inactive",
            };
            entries.push(TurnOrderEntry {
                participant_id: p.id.clone(),
                position: None,
                status: status.to_string(),
            });
        }
    }

    entries
}
