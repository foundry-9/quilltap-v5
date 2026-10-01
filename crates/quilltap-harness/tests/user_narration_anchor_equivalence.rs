//! Tier-1 differential (P4.D243): the chained-turn scene note —
//! `quilltap_core::user_narration_anchor` vs v4's REAL
//! `lib/chat/context/user-narration-anchor.ts` (`ca363178d`).
//!
//! Every `build` row diffs the returned string byte-for-byte AND the module's
//! one `logger.debug` line: the oracle wraps v4's real logger instance and
//! records each call's message and context object (key order preserved), and
//! the Rust side runs the same input under the thread-scoped capture rig, so
//! the line's message, field values AND field order are all compared — the
//! applying rows pin the line, every other row pins its silence.
//!
//! The module does not exist at the `97b25fc53` baseline (the case fails to
//! import there — the both-directions marker). Generate at the pin:
//!   cd <v4 checkout at ca363178d>
//!   PATH=$HOME/.nvm/versions/node/v24.13.1/bin:$PATH \
//!     npx tsx ~/source/quilltap-v5/harness/oracle/cases/user-narration-anchor.ts \
//!     > /tmp/oracle-user-narration-anchor.ndjson
//! Run:
//!   QT_ORACLE_USER_NARRATION_ANCHOR=/tmp/oracle-user-narration-anchor.ndjson \
//!     cargo test -p quilltap-harness --test user_narration_anchor_equivalence

use std::collections::{HashMap, HashSet};

use quilltap_core::user_narration_anchor::{
    build_user_narration_anchor, render_user_narration_anchor, BuildUserNarrationAnchorInput,
    NarrationWindowRow,
};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
struct WinRow {
    role: String,
    #[serde(default)]
    id: Option<String>,
    #[serde(default, rename = "participantId")]
    participant_id: Option<String>,
}

#[derive(Deserialize)]
struct Input {
    #[serde(rename = "isMultiCharacter")]
    is_multi_character: bool,
    #[serde(rename = "hasNewUserMessage")]
    has_new_user_message: bool,
    #[serde(rename = "historyWindow")]
    history_window: Vec<WinRow>,
    /// Absent and `null` are both `None` — v4 treats them (and an empty set)
    /// identically.
    #[serde(default, rename = "humanTurnMessageIds")]
    human_turn_message_ids: Option<Vec<String>>,
    #[serde(rename = "userName")]
    user_name: String,
    names: Option<HashMap<String, String>>,
}

#[derive(Deserialize)]
struct Log {
    message: String,
    context: serde_json::Map<String, Value>,
}

#[derive(Deserialize)]
#[serde(tag = "kind")]
enum Row {
    #[serde(rename = "render")]
    Render {
        id: String,
        #[serde(rename = "userName")]
        user_name: String,
        text: String,
    },
    #[serde(rename = "build")]
    Build {
        id: String,
        input: Input,
        text: String,
        logs: Vec<Log>,
    },
}

/// v4's debug call rendered the way the capture rig renders the Rust line (the
/// message first, as `tracing` records it), then the fields in v4's own key
/// order.
fn expected_line(log: &Log) -> String {
    let mut line = format!("DEBUG chat.context.user-narration-anchor {}", log.message);
    for (k, v) in &log.context {
        let rendered = match v {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        line.push_str(&format!(" {k}={rendered}"));
    }
    line
}

#[test]
fn user_narration_anchor_equivalence() {
    let Ok(path) = std::env::var("QT_ORACLE_USER_NARRATION_ANCHOR") else {
        eprintln!("SKIP: QT_ORACLE_USER_NARRATION_ANCHOR not set");
        return;
    };
    let data = std::fs::read_to_string(&path).expect("read oracle ndjson");
    assert!(!data.trim().is_empty(), "oracle file {path} is empty");

    let (mut renders, mut builds, mut applying, mut silent) = (0usize, 0usize, 0usize, 0usize);
    for line in data.lines().filter(|l| !l.trim().is_empty()) {
        match serde_json::from_str::<Row>(line).expect("parse oracle row") {
            Row::Render {
                id,
                user_name,
                text,
            } => {
                assert_eq!(
                    render_user_narration_anchor(&user_name),
                    text,
                    "render '{id}'"
                );
                renders += 1;
            }
            Row::Build {
                id,
                input,
                text,
                logs,
            } => {
                let window: Vec<NarrationWindowRow<'_>> = input
                    .history_window
                    .iter()
                    .map(|r| NarrationWindowRow {
                        role: &r.role,
                        id: r.id.as_deref(),
                        participant_id: r.participant_id.as_deref(),
                    })
                    .collect();
                let ids: Option<HashSet<String>> = input
                    .human_turn_message_ids
                    .as_ref()
                    .map(|v| v.iter().cloned().collect());
                let names = input.names.clone();
                let namer = move |pid: &str| names.as_ref().and_then(|m| m.get(pid).cloned());
                let namer_dyn: &dyn Fn(&str) -> Option<String> = &namer;
                let (got, lines) = quilltap_core::test_support::captured_with(|| {
                    build_user_narration_anchor(&BuildUserNarrationAnchorInput {
                        is_multi_character: input.is_multi_character,
                        has_new_user_message: input.has_new_user_message,
                        history_window: &window,
                        human_turn_message_ids: ids.as_ref(),
                        user_name: &input.user_name,
                        name_for_participant: input.names.as_ref().map(|_| namer_dyn),
                    })
                });
                assert_eq!(got, text, "build '{id}'");
                let want: Vec<String> = logs.iter().map(expected_line).collect();
                assert_eq!(lines, want, "debug line(s) for '{id}'");
                if text.is_empty() {
                    silent += 1;
                } else {
                    applying += 1;
                }
                builds += 1;
            }
        }
    }

    assert_eq!(renders, 3, "render rows");
    assert_eq!(
        builds, 30,
        "build rows (the committed corpus; a short NDJSON must refuse)"
    );
    assert_eq!(
        (applying, silent),
        (15, 15),
        "applying / silent split of the committed corpus"
    );
    eprintln!("user-narration-anchor: {renders} render + {builds} build rows ({applying} applying, {silent} silent)");
}
