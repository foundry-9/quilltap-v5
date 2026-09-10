//! Tier-1 differential (P4.D172): the cycle rotation — drawn once, then followed.
//!
//! Drives v4's REAL `lib/chat/turn-manager/{cycle-order,state,queue,turn-order,
//! selection}.ts` at the `78b381a96` pin. `Math.random` is pinned to an ORDERED
//! ARRAY per row, because `drawCycleOrder` calls it once per remaining candidate;
//! the oracle emits the draws it CONSUMED and this side replays the same array
//! through a counting source, so a port that draws where v4 does not (or skips a
//! draw) diverges even when the outcome happens to agree.
//!
//! Every case name in v4's own `cycle-order.test.ts` is a row here. v4's two
//! WEIGHTING cases are statistical (400 draws asserting `p1First > 240`; 300 loud
//! vs 300 quiet asserting `loud > quiet * 5 && quiet < 15`) and cannot be
//! transcribed as differential rows: the pinned-sequence rows cover the same
//! ground deterministically, and `weighting_matches_v4_statistical_bounds` below
//! mirrors v4's own bounds Rust-side.
//!
//! Generate the oracle output (tsx imports the WORKTREE case file — point it at
//! this lane's copy, not main):
//!   cd ~/source/quilltap-server
//!   npx tsx <worktree>/harness/oracle/cases/cycle-order.ts \
//!     > /tmp/oracle-cycle-order.ndjson
//! Run:
//!   QT_ORACLE_CYCLE_ORDER=/tmp/oracle-cycle-order.ndjson \
//!     cargo test -p quilltap-harness --test cycle_order_equivalence

use std::collections::HashMap;

use quilltap_core::chat_predicates::{participant_status_from_str, ParticipantStatus};
use quilltap_core::cycle_order::{
    cycle_candidates, draw_cycle_order, parse_cycle_order, pick_from_cycle_order,
    resolve_cycle_order_pure, stringify_cycle_order,
};
use quilltap_core::select_speaker::{
    get_selection_explanation, select_next_speaker, SpeakerCharacter, SpeakerParticipant,
};
use quilltap_core::turn_order::{compute_predicted_turn_order, TurnOrderParticipant};
use quilltap_core::turn_state::{
    compute_cycle_order_after_message, compute_cycle_order_after_skip, create_initial_turn_state,
    reset_cycle_for_user_skip, update_turn_state_after_message, MessageView, TurnState,
};
use quilltap_core::weighted_random::DrawSource;
use serde::Deserialize;
use serde_json::Value;

// --------------------------------------------------------------------------
// wire shapes
// --------------------------------------------------------------------------

#[derive(Deserialize, Clone)]
struct WirePart {
    id: String,
    #[serde(rename = "type")]
    participant_type: String,
    status: String,
    #[serde(rename = "characterId")]
    character_id: Option<String>,
    #[serde(rename = "controlledBy")]
    controlled_by: String,
    talkativeness: Option<f64>,
}

#[derive(Deserialize, Clone, Default)]
struct WireChar {
    #[serde(default)]
    talkativeness: Option<f64>,
    #[serde(rename = "archivedAt", default)]
    archived_at: Option<String>,
}

type WireChars = HashMap<String, WireChar>;

/// Deserialize + map the row's `participants` in one step.
fn speakers_of(row: &Value) -> Vec<SpeakerParticipant> {
    let parts: Vec<WirePart> =
        serde_json::from_value(row["participants"].clone()).expect("row participants");
    to_speakers(&parts)
}

/// Deserialize + map the row's `characters` in one step.
fn characters_of(row: &Value) -> HashMap<String, SpeakerCharacter> {
    let chars: WireChars =
        serde_json::from_value(row["characters"].clone()).expect("row characters");
    to_characters(&chars)
}

fn to_speakers(parts: &[WirePart]) -> Vec<SpeakerParticipant> {
    parts
        .iter()
        .map(|p| SpeakerParticipant {
            id: p.id.clone(),
            participant_type: p.participant_type.clone(),
            // §C: participant-status parsing has ONE home.
            status: participant_status_from_str(Some(p.status.as_str())),
            character_id: p.character_id.clone(),
            controlled_by: p.controlled_by.clone(),
            talkativeness: p.talkativeness,
        })
        .collect()
}

fn to_characters(chars: &WireChars) -> HashMap<String, SpeakerCharacter> {
    chars
        .iter()
        .map(|(cid, c)| {
            (
                cid.clone(),
                SpeakerCharacter {
                    talkativeness: c.talkativeness,
                    // v4 reads `!character?.archivedAt` — JS truthiness, so an
                    // EMPTY STRING is not a tombstone.
                    archived: c.archived_at.as_deref().is_some_and(|s| !s.is_empty()),
                },
            )
        })
        .collect()
}

/// Replays a pinned draw sequence and RECORDS what the port consumed. Mirrors the
/// oracle's `withRandom`: the last value repeats once the array is spent.
fn replay(pinned: &[f64]) -> (DrawSource, std::sync::Arc<std::sync::Mutex<Vec<f64>>>) {
    let log = std::sync::Arc::new(std::sync::Mutex::new(Vec::<f64>::new()));
    let sink = log.clone();
    let values = pinned.to_vec();
    let cursor = std::sync::atomic::AtomicUsize::new(0);
    let src = DrawSource::from_fn(move || {
        let v = if values.is_empty() {
            0.0
        } else {
            let i = cursor.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            values[i.min(values.len() - 1)]
        };
        sink.lock().unwrap().push(v);
        v
    });
    (src, log)
}

fn assert_draws(id: &str, got: &[f64], oracle: &[f64]) {
    assert_eq!(
        got.len(),
        oracle.len(),
        "{id}: draw COUNT rust={got:?} oracle={oracle:?}"
    );
    for (i, (g, o)) in got.iter().zip(oracle.iter()).enumerate() {
        assert!((g - o).abs() < 1e-12, "{id}: draw[{i}] rust={g} oracle={o}");
    }
}

#[test]
fn cycle_order_matches_oracle() {
    let path = match std::env::var("QT_ORACLE_CYCLE_ORDER") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_CYCLE_ORDER to the oracle NDJSON (see test header).");
            return;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));

    let mut counts: HashMap<String, usize> = HashMap::new();

    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Value = serde_json::from_str(line).expect("oracle row");
        let kind = row["kind"].as_str().expect("kind").to_string();
        let id = row["id"].as_str().expect("id").to_string();
        *counts.entry(kind.clone()).or_default() += 1;

        match kind.as_str() {
            "parse" => {
                let json = row["json"].as_str();
                let got = parse_cycle_order(json);
                let want: Vec<String> = serde_json::from_value(row["out"].clone()).unwrap();
                assert_eq!(got, want, "parse '{id}'");
            }
            "candidates" => {
                let parts = speakers_of(&row);
                let chars = characters_of(&row);
                let got: Vec<String> = cycle_candidates(&parts, &chars)
                    .iter()
                    .map(|p| p.id.clone())
                    .collect();
                let want: Vec<String> = serde_json::from_value(row["out"].clone()).unwrap();
                assert_eq!(got, want, "candidates '{id}'");
            }
            "draw" => {
                let parts = speakers_of(&row);
                let chars = characters_of(&row);
                let pinned: Vec<f64> = serde_json::from_value(row["draws"].clone()).unwrap();
                let exclude = row["excludeFirst"].as_str();
                let (draws, log) = replay(&pinned);
                let got = draw_cycle_order(&parts, &chars, exclude, &draws);
                let want: Vec<String> = serde_json::from_value(row["out"].clone()).unwrap();
                assert_eq!(got, want, "draw '{id}'");
                let want_draws: Vec<f64> =
                    serde_json::from_value(row["consumedDraws"].clone()).unwrap();
                assert_draws(&format!("draw '{id}'"), &log.lock().unwrap(), &want_draws);
            }
            "pick-from" => {
                let parts = speakers_of(&row);
                let chars = characters_of(&row);
                let order: Vec<String> = match &row["order"] {
                    Value::Null => Vec::new(),
                    v => serde_json::from_value(v.clone()).unwrap(),
                };
                let spoken: Vec<String> = serde_json::from_value(row["spoken"].clone()).unwrap();
                let got = pick_from_cycle_order(
                    &order,
                    &parts,
                    &chars,
                    &spoken,
                    row["lastSpeakerId"].as_str(),
                );
                let want = row["out"].as_str().map(String::from);
                assert_eq!(got, want, "pick-from '{id}'");
            }
            "resolve" => {
                let parts = speakers_of(&row);
                let chars = characters_of(&row);
                let stored: Vec<String> = serde_json::from_value(row["stored"].clone()).unwrap();
                let spoken: Vec<String> = serde_json::from_value(row["spoken"].clone()).unwrap();
                let pinned: Vec<f64> = serde_json::from_value(row["draws"].clone()).unwrap();
                let fail_write = row["failWrite"].as_bool().unwrap_or(false);
                let (draws, log) = replay(&pinned);
                let got = resolve_cycle_order_pure(
                    &parts,
                    &chars,
                    &stored,
                    &spoken,
                    row["lastSpeakerId"].as_str(),
                    &draws,
                );
                let want: Vec<String> = serde_json::from_value(row["out"].clone()).unwrap();
                assert_eq!(got.order, want, "resolve '{id}' order");

                // v4's `writes` is what reached `repos.chats.update`. The pure
                // resolver reports the SAME decision as `persist: Some(_)`; the
                // failing-write row proves the swallow — v4 records no write and
                // still returns the rotation, and the port must still say it
                // MEANT to write (the async shell is what swallows).
                let writes = row["writes"].as_array().cloned().unwrap_or_default();
                if fail_write {
                    assert!(
                        writes.is_empty(),
                        "resolve '{id}': a failed write records nothing"
                    );
                    assert!(
                        got.persist.is_some(),
                        "resolve '{id}': the decision to write survives the failure"
                    );
                } else {
                    assert_eq!(
                        got.persist.is_some(),
                        !writes.is_empty(),
                        "resolve '{id}': write-or-not disagreed (rust persist={:?}, v4 writes={})",
                        got.persist,
                        writes.len()
                    );
                    if let Some(w) = writes.first() {
                        assert_eq!(
                            w["cycleOrderParticipantIds"].as_str().unwrap(),
                            stringify_cycle_order(&got.order),
                            "resolve '{id}': persisted bytes"
                        );
                        assert_eq!(
                            w["chatId"].as_str(),
                            Some("chat-1"),
                            "resolve '{id}': chat id"
                        );
                    }
                }
                let want_draws: Vec<f64> =
                    serde_json::from_value(row["consumedDraws"].clone()).unwrap();
                assert_draws(
                    &format!("resolve '{id}'"),
                    &log.lock().unwrap(),
                    &want_draws,
                );
            }
            "after-message" => {
                let msg = &row["message"];
                let view = MessageView {
                    msg_type: msg["type"].as_str().map(String::from),
                    role: msg["role"].as_str().unwrap_or_default().to_string(),
                    participant_id: msg["participantId"].as_str().map(String::from),
                    target_participant_ids: msg
                        .get("targetParticipantIds")
                        .and_then(|v| serde_json::from_value(v.clone()).ok()),
                };
                let got = compute_cycle_order_after_message(&view, row["json"].as_str());
                let want = row["out"].as_str().map(String::from);
                assert_eq!(got, want, "after-message '{id}'");
            }
            "after-skip" => {
                let got = compute_cycle_order_after_skip(
                    row["participantId"].as_str().unwrap(),
                    row["json"].as_str(),
                );
                let want = row["out"].as_str().map(String::from);
                assert_eq!(got, want, "after-skip '{id}'");
            }
            "initial-state" => {
                let got = create_initial_turn_state();
                assert!(got.cycle_order.is_empty(), "initial-state cycleOrder");
                let want = &row["out"];
                assert_eq!(
                    want["cycleOrder"].as_array().map(|a| a.len()),
                    Some(0),
                    "initial-state: v4 seeds an EMPTY rotation"
                );
            }
            "history" => {
                let got = quilltap_core::turn_state::calculate_turn_state_from_history_with_cycle(
                    &[],
                    Some("[]"),
                    row["cycleOrderParticipantIds"].as_str(),
                );
                let want: Vec<String> =
                    serde_json::from_value(row["out"]["cycleOrder"].clone()).unwrap();
                assert_eq!(got.cycle_order, want, "history '{id}'");
            }
            "update-after-message" => {
                let state: TurnState = state_from_wire(&row["state"]);
                let extra = &row["extra"];
                let view = MessageView {
                    msg_type: Some("message".to_string()),
                    role: row["role"].as_str().unwrap().to_string(),
                    participant_id: row["participantId"].as_str().map(String::from),
                    target_participant_ids: extra
                        .get("targetParticipantIds")
                        .and_then(|v| serde_json::from_value(v.clone()).ok()),
                };
                let got = update_turn_state_after_message(&state, &view);
                let want: Vec<String> =
                    serde_json::from_value(row["out"]["cycleOrder"].clone()).unwrap();
                assert_eq!(got.cycle_order, want, "update-after-message '{id}'");
            }
            "reset-cycle-for-user-skip" => {
                let state: TurnState = state_from_wire(&row["state"]);
                let got = reset_cycle_for_user_skip(&state);
                let want: Vec<String> =
                    serde_json::from_value(row["out"]["cycleOrder"].clone()).unwrap();
                assert_eq!(got.cycle_order, want, "reset '{id}'");
                assert!(got.cycle_order.is_empty(), "reset '{id}': v4 empties it");
            }
            "turn-order" => {
                let parts: Vec<TurnOrderParticipant> = row["participants"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|p| TurnOrderParticipant {
                        id: p["id"].as_str().unwrap().to_string(),
                        status: p["status"].as_str().map(String::from),
                        controlled_by: p["controlledBy"].as_str().map(String::from),
                        talkativeness: p["character"]["talkativeness"].as_f64(),
                    })
                    .collect();
                let queue: Vec<String> = serde_json::from_value(row["queue"].clone()).unwrap();
                let spoken: Vec<String> = serde_json::from_value(row["spoken"].clone()).unwrap();
                let cycle: Vec<String> = serde_json::from_value(row["cycleOrder"].clone()).unwrap();
                let got = compute_predicted_turn_order(
                    &parts,
                    &queue,
                    &spoken,
                    row["lastSpeakerId"].as_str(),
                    row["nextSpeakerId"].as_str(),
                    false,
                    None,
                    row["userParticipantId"].as_str(),
                    &cycle,
                );
                let want = row["out"].as_array().unwrap();
                assert_eq!(got.len(), want.len(), "turn-order '{id}' length");
                for (g, w) in got.iter().zip(want.iter()) {
                    assert_eq!(
                        g.participant_id,
                        w["participantId"].as_str().unwrap(),
                        "turn-order '{id}' id"
                    );
                    assert_eq!(
                        g.position,
                        w["position"].as_i64(),
                        "turn-order '{id}' position"
                    );
                    assert_eq!(
                        g.status,
                        w["status"].as_str().unwrap(),
                        "turn-order '{id}' status"
                    );
                }
            }
            "select" => {
                let parts = speakers_of(&row);
                let chars = characters_of(&row);
                let queue: Vec<String> = serde_json::from_value(row["queue"].clone()).unwrap();
                let spoken: Vec<String> = serde_json::from_value(row["spoken"].clone()).unwrap();
                let cycle: Vec<String> = serde_json::from_value(row["cycleOrder"].clone()).unwrap();
                let impersonating: Option<Vec<String>> =
                    serde_json::from_value(row["impersonating"].clone()).unwrap_or(None);
                let pinned: Vec<f64> = serde_json::from_value(row["draws"].clone()).unwrap();
                let (draws, log) = replay(&pinned);
                let got = select_next_speaker(
                    &parts,
                    &chars,
                    &queue,
                    &spoken,
                    row["lastSpeakerId"].as_str(),
                    &draws,
                    impersonating.as_deref(),
                    &cycle,
                );
                let out = &row["out"];
                assert_eq!(
                    got.next_speaker_id.as_deref(),
                    out["nextSpeakerId"].as_str(),
                    "select '{id}' nextSpeakerId"
                );
                assert_eq!(
                    got.reason,
                    out["reason"].as_str().unwrap(),
                    "select '{id}' reason"
                );
                assert_eq!(
                    got.cycle_complete,
                    out["cycleComplete"].as_bool().unwrap(),
                    "select '{id}' cycleComplete"
                );
                assert_eq!(
                    get_selection_explanation(&got),
                    row["explanation"].as_str().unwrap(),
                    "select '{id}' explanation"
                );
                // The rotation arm's debug block: v4 emits `weights: {}` and NO
                // `randomValue` key at all.
                if got.reason == "cycle_order" || out["reason"] == "cycle_order" {
                    let d = &out["debug"];
                    assert!(
                        d["randomValue"].is_null(),
                        "select '{id}': v4 emits no randomValue on the rotation arm"
                    );
                    assert_eq!(
                        d["weights"].as_object().map(|m| m.len()),
                        Some(0),
                        "select '{id}': v4 emits an EMPTY weights map on the rotation arm"
                    );
                    let dbg = got
                        .debug
                        .as_ref()
                        .expect("rotation arm carries a debug block");
                    assert!(
                        dbg.random_value.is_none(),
                        "select '{id}': rust randomValue"
                    );
                    assert!(dbg.weights.is_empty(), "select '{id}': rust weights");
                    let want_eligible: Vec<String> =
                        serde_json::from_value(d["eligibleSpeakers"].clone()).unwrap();
                    assert_eq!(
                        dbg.eligible_speakers, want_eligible,
                        "select '{id}' eligible"
                    );
                }
                let want_draws: Vec<f64> =
                    serde_json::from_value(row["consumedDraws"].clone()).unwrap();
                assert_draws(&format!("select '{id}'"), &log.lock().unwrap(), &want_draws);
            }
            other => panic!("unknown oracle row kind '{other}'"),
        }
    }

    // Floors: a corpus that silently loses a whole family of rows must fail.
    for (kind, floor) in [
        ("parse", 9usize),
        ("candidates", 6),
        ("draw", 13),
        ("pick-from", 9),
        ("resolve", 10),
        ("after-message", 11),
        ("after-skip", 4),
        ("history", 4),
        ("update-after-message", 3),
        ("turn-order", 8),
        ("select", 11),
    ] {
        let seen = counts.get(kind).copied().unwrap_or(0);
        assert!(
            seen >= floor,
            "oracle carries {seen} '{kind}' rows, expected >= {floor}"
        );
    }
    let total: usize = counts.values().sum();
    eprintln!("OK: cycle-order matched oracle ({total} rows: {counts:?}).");
}

fn state_from_wire(v: &Value) -> TurnState {
    TurnState {
        spoken_since_user_turn: serde_json::from_value(v["spokenSinceUserTurn"].clone())
            .unwrap_or_default(),
        current_turn_participant_id: v["currentTurnParticipantId"].as_str().map(String::from),
        queue: serde_json::from_value(v["queue"].clone()).unwrap_or_default(),
        last_speaker_id: v["lastSpeakerId"].as_str().map(String::from),
        cycle_order: serde_json::from_value(v["cycleOrder"].clone()).unwrap_or_default(),
    }
}

/// v4's two WEIGHTING cases are statistical and cannot be corpus rows. This
/// mirrors their bounds Rust-side, over the same room shapes, with a fixed-seed
/// LCG standing in for `Math.random` so the assertion is deterministic.
#[test]
fn weighting_matches_v4_statistical_bounds() {
    fn seat(id: &str, talk: f64) -> SpeakerParticipant {
        SpeakerParticipant {
            id: id.to_string(),
            participant_type: "CHARACTER".to_string(),
            status: ParticipantStatus::Active,
            character_id: Some(format!("char-{id}")),
            controlled_by: "llm".to_string(),
            talkativeness: Some(talk),
        }
    }
    // A deterministic uniform stream (a 48-bit LCG), so the bound is a real
    // assertion rather than a flake.
    let state = std::sync::atomic::AtomicU64::new(0x2545_F491_4F6C_DD1D);
    let draws = DrawSource::from_fn(move || {
        let mut x = state.load(std::sync::atomic::Ordering::Relaxed);
        x = x
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        state.store(x, std::sync::atomic::Ordering::Relaxed);
        ((x >> 11) as f64) / ((1u64 << 53) as f64)
    });

    // v4: "weights the draw by talkativeness, per-chat override winning" — 400
    // draws, p1 first more than 240 times (p1 at 0.9 against two at 0.1).
    let room = vec![seat("p1", 0.9), seat("p2", 0.1), seat("p3", 0.1)];
    let chars = HashMap::new();
    let mut p1_first = 0;
    for _ in 0..400 {
        if draw_cycle_order(&room, &chars, None, &draws)
            .first()
            .map(String::as_str)
            == Some("p1")
        {
            p1_first += 1;
        }
    }
    assert!(
        p1_first > 240,
        "p1 led {p1_first}/400 cycles, expected > 240"
    );

    // v4: 300 loud vs 300 quiet — the loud seat leads more than five times as
    // often, and the quiet one fewer than fifteen times.
    let pair = vec![seat("loud", 1.0), seat("quiet", 0.02)];
    let mut loud = 0usize;
    let mut quiet = 0usize;
    for _ in 0..300 {
        match draw_cycle_order(&pair, &chars, None, &draws)
            .first()
            .map(String::as_str)
        {
            Some("loud") => loud += 1,
            Some("quiet") => quiet += 1,
            other => panic!("unexpected head {other:?}"),
        }
    }
    assert!(loud > quiet * 5, "loud {loud} vs quiet {quiet}");
    assert!(quiet < 15, "quiet led {quiet}/300, expected < 15");
}
