//! Tier-1 differential test (chat-orchestration wave 1.4): the Aurora Core
//! whisper cadence trigger, `shouldFireCoreWhisper` (and its private helpers
//! `isVisibleConversationalTurn` / `findLatestContextTransitionIndex`,
//! exercised only through the public function). Pure function — exact
//! equality on every field.
//!
//! Generate the oracle output:
//!   cd ~/source/quilltap-server
//!   npx tsx ~/source/quilltap-v5/harness/oracle/cases/core-whisper.ts \
//!     > /tmp/oracle-core-whisper.ndjson
//! Run:
//!   QT_ORACLE_CORE_WHISPER=/tmp/oracle-core-whisper.ndjson cargo test -p quilltap-harness

use quilltap_core::core_whisper::{
    should_fire_core_whisper, CoreWhisperReason, ShouldFireCoreWhisperOptions, WhisperEvent,
};
use serde::Deserialize;

#[derive(Deserialize)]
struct WireEvent {
    #[serde(rename = "type")]
    event_type: String,
    role: Option<String>,
    #[serde(rename = "participantId", default)]
    participant_id: Option<String>,
    content: Option<String>,
    #[serde(rename = "systemSender", default)]
    system_sender: Option<String>,
    #[serde(rename = "systemKind", default)]
    system_kind: Option<String>,
    #[serde(rename = "isSilentMessage", default)]
    is_silent_message: Option<bool>,
    #[serde(rename = "targetParticipantIds", default)]
    target_participant_ids: Option<Vec<String>>,
}

impl WireEvent {
    fn into_event(self) -> WhisperEvent {
        WhisperEvent {
            event_type: self.event_type,
            role: self.role,
            participant_id: self.participant_id,
            content: self.content,
            system_sender: self.system_sender,
            system_kind: self.system_kind,
            is_silent_message: self.is_silent_message,
            target_participant_ids: self.target_participant_ids,
        }
    }
}

#[derive(Deserialize)]
struct WireOptions {
    events: Vec<WireEvent>,
    #[serde(rename = "respondingParticipantId")]
    responding_participant_id: String,
    #[serde(rename = "isContinue")]
    is_continue: bool,
    #[serde(rename = "isNudge")]
    is_nudge: bool,
    interval: i64,
    #[serde(rename = "silenceThreshold")]
    silence_threshold: i64,
    #[serde(rename = "fireOnContextTransition", default)]
    fire_on_context_transition: Option<bool>,
}

#[derive(Deserialize)]
struct WireResult {
    fire: bool,
    reason: Option<String>,
}

#[derive(Deserialize)]
struct Row {
    id: String,
    options: WireOptions,
    out: WireResult,
}

fn reason_str(r: Option<CoreWhisperReason>) -> Option<&'static str> {
    r.map(|r| r.as_str())
}

#[test]
fn core_whisper_matches_oracle() {
    let path = match std::env::var("QT_ORACLE_CORE_WHISPER") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_CORE_WHISPER to the oracle NDJSON (see test header).");
            return;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));

    let mut count = 0usize;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Row = serde_json::from_str(line).unwrap();
        let events: Vec<WhisperEvent> = row
            .options
            .events
            .into_iter()
            .map(WireEvent::into_event)
            .collect();

        // v4 defaults fireOnContextTransition to true when omitted.
        let fire_on_context_transition = row.options.fire_on_context_transition.unwrap_or(true);

        let got = should_fire_core_whisper(ShouldFireCoreWhisperOptions {
            events: &events,
            responding_participant_id: &row.options.responding_participant_id,
            is_continue: row.options.is_continue,
            is_nudge: row.options.is_nudge,
            interval: row.options.interval,
            silence_threshold: row.options.silence_threshold,
            fire_on_context_transition,
        });

        assert_eq!(got.fire, row.out.fire, "'{}' fire", row.id);
        assert_eq!(
            reason_str(got.reason),
            row.out.reason.as_deref(),
            "'{}' reason",
            row.id
        );

        count += 1;
    }

    assert!(count > 0, "oracle file looks empty: {count}");
    eprintln!("OK: core-whisper matched oracle ({count} rows).");
}
