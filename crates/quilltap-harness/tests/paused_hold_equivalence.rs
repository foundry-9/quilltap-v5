//! Tier-1 differential: the paused-chat hold rule (P4.D186; v4 `31436bae4`
//! bug 137, `lib/services/chat-message/paused-hold.ts`
//! `shouldHoldUserTurnForPause`), ported as
//! `quilltap_core::services::paused_hold::should_hold_user_turn_for_pause`.
//!
//! The oracle drives v4's REAL export over the FULL 2 × 2 × 3 input grid —
//! `isContinueMode` × `chatIsPaused` × `neverPauseForUser` in each of its three
//! states (absent / `false` / `true`; v4's guard is `=== true`, so the absent arm
//! is a distinct input, not a spelling of `false`). Twelve rows, exact booleans.
//! v4's own four unit shapes fall out of the grid and are asserted by name.
//!
//! Regenerate the oracle (from the v4 checkout — a PINNED worktree under the
//! ledger's PIN REQUIRED rule, since the predicate does not exist at the
//! baseline):
//!   cd ~/source/quilltap-server   # (pinned: pass --v4 <pin> to the sweep driver)
//!   V5W=${V5W:-$HOME/source/quilltap-v5}
//!   ~/.nvm/versions/node/v24.13.1/bin/npx tsx \
//!     $V5W/harness/oracle/cases/paused-hold.ts > /tmp/oracle-paused-hold.ndjson
//! Run:
//!   QT_ORACLE_PAUSED_HOLD=/tmp/oracle-paused-hold.ndjson \
//!     cargo test -p quilltap-harness --test paused_hold_equivalence -- --nocapture

use std::collections::HashSet;

use quilltap_core::services::paused_hold::{should_hold_user_turn_for_pause, PausedHoldInput};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Row {
    label: String,
    is_continue_mode: bool,
    chat_is_paused: bool,
    /// `"absent"` / `"false"` / `"true"` — the tri-state tag, not a bool, so the
    /// NDJSON records which of v4's three input shapes the row drove.
    never_pause_for_user: String,
    hold: bool,
}

#[test]
fn paused_hold_matches_v4() {
    let Ok(path) = std::env::var("QT_ORACLE_PAUSED_HOLD") else {
        eprintln!("QT_ORACLE_PAUSED_HOLD not set; skipping");
        return;
    };
    let text = std::fs::read_to_string(&path).expect("read oracle NDJSON");
    let mut rows = 0usize;
    let mut labels: HashSet<String> = HashSet::new();
    let mut held = 0usize;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Row = serde_json::from_str(line).expect("parse oracle row");
        let never_pause_for_user = match row.never_pause_for_user.as_str() {
            "absent" => None,
            "false" => Some(false),
            "true" => Some(true),
            other => panic!("unknown neverPauseForUser tag {other} in {}", row.label),
        };
        let got = should_hold_user_turn_for_pause(PausedHoldInput {
            is_continue_mode: row.is_continue_mode,
            chat_is_paused: row.chat_is_paused,
            never_pause_for_user,
        });
        assert_eq!(got, row.hold, "hold mismatch for {}", row.label);
        if row.hold {
            held += 1;
        }
        labels.insert(row.label);
        rows += 1;
    }
    // The grid is exhaustive: 2 × 2 × 3.
    assert_eq!(rows, 12, "expected the full input grid, got {rows} rows");
    // v4's four unit shapes, by name.
    for required in [
        "v4_holds_a_typed_message_while_paused",
        "v4_lets_a_typed_message_through_when_not_paused",
        "v4_never_holds_a_continue_mode_summons",
        "v4_never_holds_an_autonomous_room_turn",
    ] {
        assert!(labels.contains(required), "missing v4 shape {required}");
    }
    // Non-vacuity: the grid must contain BOTH answers, or an all-`false` port
    // would pass. Exactly two coordinates hold — paused, not continue mode, with
    // `neverPauseForUser` absent or explicitly false.
    assert_eq!(held, 2, "expected exactly two holding rows, got {held}");
    println!("paused_hold_equivalence: {rows} rows OK ({held} held)");
}
