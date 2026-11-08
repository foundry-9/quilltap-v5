//! Salon stream frames — the ONE renderer of what a chat stream carries
//! (P4.D366, v4 `604a082c4` `lib/chat/stream-frames.ts`).
//!
//! v4's module header, carried: a Salon turn (send, continue, nudge,
//! Storyteller run, swipe regeneration) streams to the browser as Server-Sent
//! Events. Each `data:` line is one **frame**: a member of a tagged union
//! discriminated on `kind`. The server produces bytes only through
//! `encodeFrame`; the client reads them only through `parseFrame`, and
//! dispatches with an exhaustive `switch (frame.kind)`, so a new kind is a
//! compile error until every consumer handles or ignores it, and no ordering
//! of tests decides what a frame means. Two consolidations over the old
//! untagged bag:
//!
//! - `done.outcome` is one value — `finished`, `skipped` (a "nothing to add"
//!   pass), `yielded` (a bare yield to the raised hand), `stopped` (the
//!   operator hard-stopped the turn before any prose) or `empty` (the provider
//!   returned nothing) — instead of five boolean side flags.
//! - `posted` is the one frame for "here is a full message the server has
//!   already saved" (a Carina answer, a Host announcement, a Pascal outcome),
//!   which the client inserts identically; `what` says which.
//!
//! Storyteller "all at once" turns share one stream; each of their frames
//! carries a `seat` (v4 `tagFrame`). The tag is put on the frame before it is
//! encoded — never by re-reading bytes. The other SSE routes (help chat, the
//! Brahma console, the Scenario Builder, the one-shot tool loop) keep the
//! older untagged-bag wire shape; they render through [`Dialect::Legacy`],
//! which is v4's `toLegacyBag` applied to the frame.
//!
//! ## How v5 maps it (the human's ruling R-G)
//!
//! [`ChatEvent`] STAYS the producer vocabulary — no emit site and no
//! variant's fields were re-shaped for the frames. The dialect is a property
//! of the SINK: the host's `BroadcastSink` carries [`Dialect::Frames`] for the
//! Salon send / continue / nudge, the enclave step and the Storyteller run,
//! and [`Dialect::Legacy`] for Help chat and the Brahma console; the Scenario
//! Builder's sink (and through it the one-shot loop) renders `Legacy`.
//! `ChatEvent` derives no `Serialize`: [`render`] is the only way an event
//! reaches bytes, and its ONE exhaustive `match` (no `_` arm) is v4's
//! `assertNever` — a new variant is a compile error here.
//!
//! Key order is the PRODUCER's literal (v4 encodes the unparsed object): the
//! payload structs' serde order, with `kind` first and a `seat` LAST.
//!
//! ## NO-PORT (planner ruling R2)
//!
//! v4's `encodeFrame` validates every frame against `StreamFrameSchema` only
//! when `NODE_ENV !== 'production'` and logs `[StreamFrames] Malformed frame
//! emitted` `{kind, issues}` at ERROR before sending it anyway. v5 ships
//! production behaviour: there is no runtime validator and no
//! `debug_assert!`. The exhaustive [`render`] match and the tier-1
//! `stream_frames_equivalence` family (v4's REAL `encodeFrame` in all three
//! encoder profiles) are the guarantee.

use serde_json::{Map, Value};

use crate::api::types::ChatErrorPayload;
use crate::services::chat_events::ChatEvent;

/// Which wire shape a sink renders (v4's `WireDialect`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dialect {
    /// The 4.12 `kind`-tagged union (v4's `DEFAULT_DIALECT` — what a plain
    /// `TextEncoder` renders).
    Frames,
    /// The pre-4.12 untagged bag (v4 `legacyFrameEncoder()`).
    Legacy,
}

/// How a `done` frame closed its turn (v4 `DoneOutcomeSchema`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DoneOutcome {
    Finished,
    Skipped,
    Yielded,
    Stopped,
    Empty,
}

impl DoneOutcome {
    /// The wire string.
    pub fn as_str(self) -> &'static str {
        match self {
            DoneOutcome::Finished => "finished",
            DoneOutcome::Skipped => "skipped",
            DoneOutcome::Yielded => "yielded",
            DoneOutcome::Stopped => "stopped",
            DoneOutcome::Empty => "empty",
        }
    }
}

/// The four boolean side flags the producer payloads still carry; the frame
/// folds them into `outcome`.
const DONE_FLAG_KEYS: [&str; 4] = ["stopped", "skipped", "yielded", "emptyResponse"];

fn outcome_from_flags(
    stopped: Option<bool>,
    yielded: Option<bool>,
    skipped: Option<bool>,
    empty: Option<bool>,
) -> DoneOutcome {
    // v4's own precedence (`fromLegacyBag`, `salon-stream-golden.ts:40-46`):
    // stopped, then yielded, then skipped, then empty, else finished.
    if stopped == Some(true) {
        DoneOutcome::Stopped
    } else if yielded == Some(true) {
        DoneOutcome::Yielded
    } else if skipped == Some(true) {
        DoneOutcome::Skipped
    } else if empty == Some(true) {
        DoneOutcome::Empty
    } else {
        DoneOutcome::Finished
    }
}

/// The `done` outcome of an event, or `None` for a non-done event.
pub fn outcome_of(event: &ChatEvent) -> Option<DoneOutcome> {
    match event {
        ChatEvent::Done { payload, .. } => Some(outcome_from_flags(
            payload.stopped,
            payload.yielded,
            payload.skipped,
            payload.empty_response,
        )),
        ChatEvent::TurnDone { payload, .. } => Some(outcome_from_flags(
            payload.stopped,
            payload.yielded,
            payload.skipped,
            payload.empty_response,
        )),
        _ => None,
    }
}

fn to_value<T: serde::Serialize>(v: &T) -> Value {
    serde_json::to_value(v).expect("a stream-frame payload serializes")
}

/// `{kind, …entries}` — `kind` first, then the given entries in order.
fn kinded(kind: &str, entries: impl IntoIterator<Item = (&'static str, Value)>) -> Value {
    let mut m = Map::new();
    m.insert("kind".into(), Value::String(kind.into()));
    for (k, v) in entries {
        m.insert(k.into(), v);
    }
    Value::Object(m)
}

/// `{kind, …payload's own keys in serde order}` (v4's `{ kind, ...data }`).
fn kinded_spread<T: serde::Serialize>(kind: &str, payload: &T) -> Value {
    let mut m = Map::new();
    m.insert("kind".into(), Value::String(kind.into()));
    match to_value(payload) {
        Value::Object(p) => m.extend(p),
        other => panic!("a spread stream-frame payload must be an object, got {other}"),
    }
    Value::Object(m)
}

/// `{kind:"done", outcome, …payload minus the four flags}` — v4's
/// `encodeDoneEvent` destructures `outcome` out of `data` and writes it
/// SECOND; `skippedParticipantId` / `emptyResponseReason` stay where the
/// payload declares them.
fn done_frame<T: serde::Serialize>(outcome: DoneOutcome, payload: &T) -> Value {
    let mut m = Map::new();
    m.insert("kind".into(), Value::String("done".into()));
    m.insert("outcome".into(), Value::String(outcome.as_str().into()));
    match to_value(payload) {
        Value::Object(p) => {
            for (k, v) in p {
                if !DONE_FLAG_KEYS.contains(&k.as_str()) {
                    m.insert(k, v);
                }
            }
        }
        other => panic!("a done payload must be an object, got {other}"),
    }
    Value::Object(m)
}

/// The frames-dialect object for one event (v4's producer literals).
fn frame_of(event: &ChatEvent) -> Value {
    match event {
        ChatEvent::Status { status } => kinded("status", [("status", to_value(status))]),
        ChatEvent::Content { content } => {
            kinded("content", [("text", Value::String(content.clone()))])
        }
        ChatEvent::Reasoning { reasoning } => {
            kinded("reasoning", [("text", Value::String(reasoning.clone()))])
        }
        ChatEvent::CarinaAnswer { carina_answer } => kinded(
            "posted",
            [
                ("what", Value::String("carinaAnswer".into())),
                ("message", carina_answer.clone()),
            ],
        ),
        ChatEvent::HostAnnouncement { host_announcement } => kinded(
            "posted",
            [
                ("what", Value::String("hostAnnouncement".into())),
                ("message", host_announcement.clone()),
            ],
        ),
        ChatEvent::PascalResult { pascal_result } => kinded(
            "posted",
            [
                ("what", Value::String("pascalResult".into())),
                ("message", pascal_result.clone()),
            ],
        ),
        ChatEvent::ConfirmationResult {
            confirmation_result,
        } => kinded_spread("confirmationResult", confirmation_result),
        ChatEvent::TurnStart { payload, .. } => kinded_spread("turnStart", payload),
        ChatEvent::TurnComplete { payload, .. } => kinded_spread("turnComplete", payload),
        ChatEvent::HandAsked { payload, .. } => kinded_spread("handAsked", payload),
        ChatEvent::HandAnswered { payload, .. } => kinded_spread("handAnswered", payload),
        ChatEvent::ChainComplete { payload, .. } => kinded_spread("chainComplete", payload),
        // The old count key is GONE in frames (`tool-execution.service.ts:95-99`).
        ChatEvent::ToolsDetected {
            tool_names,
            tool_arguments,
            ..
        } => kinded(
            "toolsDetected",
            [
                ("names", to_value(tool_names)),
                ("arguments", Value::Array(tool_arguments.clone())),
            ],
        ),
        ChatEvent::ToolResult { tool_result } => kinded_spread("toolResult", tool_result),
        ChatEvent::PendingExternalTurn { payload, .. } => {
            kinded_spread("pendingExternalTurn", payload)
        }
        ChatEvent::FileProcessing { file_processing } => {
            kinded("fileProcessing", [("info", to_value(file_processing))])
        }
        // `encodeErrorEvent`: `{ kind:'error', message: error, errorType, details }`.
        ChatEvent::Error {
            error,
            error_type,
            details,
        } => kinded(
            "error",
            [
                ("message", Value::String(error.clone())),
                ("errorType", Value::String(error_type.clone())),
                ("details", Value::String(details.clone())),
            ],
        ),
        ChatEvent::Done { .. } | ChatEvent::TurnDone { .. } => {
            let outcome = outcome_of(event).expect("a done event has an outcome");
            match event {
                ChatEvent::Done { payload, .. } => done_frame(outcome, payload.as_ref()),
                ChatEvent::TurnDone { payload, .. } => done_frame(outcome, payload.as_ref()),
                _ => unreachable!("matched a done variant above"),
            }
        }
        ChatEvent::StorytellerRound { storyteller_round } => {
            kinded("storytellerRound", [("round", storyteller_round.clone())])
        }
        // FLAT in frames (was nested under `storytellerBid`).
        ChatEvent::StorytellerBid { storyteller_bid } => kinded(
            "storytellerBid",
            [
                ("roundId", Value::String(storyteller_bid.round_id.clone())),
                ("bid", storyteller_bid.bid.clone()),
            ],
        ),
        ChatEvent::RoundOpen { round_id, .. } => {
            kinded("roundOpen", [("roundId", Value::String(round_id.clone()))])
        }
    }
}

/// v4 `tagFrame(frame, participantId)` = `{ ...frame, seat: { participantId,
/// concurrent: true } }` — `seat` ALWAYS last.
fn tag_frame(frame: Value, participant_id: &str) -> Value {
    let mut m = match frame {
        Value::Object(m) => m,
        other => return other,
    };
    let mut seat = Map::new();
    seat.insert("participantId".into(), Value::String(participant_id.into()));
    seat.insert("concurrent".into(), Value::Bool(true));
    m.insert("seat".into(), Value::Object(seat));
    Value::Object(m)
}

/// Render one event on a dialect, tagged with a concurrent seat when one is
/// given (v4 `encodeFrame`'s payload, before `JSON.stringify`).
pub fn render(event: &ChatEvent, dialect: Dialect, seat: Option<&str>) -> Value {
    let frame = frame_of(event);
    let tagged = match seat {
        Some(pid) => tag_frame(frame, pid),
        None => frame,
    };
    match dialect {
        Dialect::Frames => tagged,
        Dialect::Legacy => to_legacy_bag(&tagged),
    }
}

/// The transport-shell error frame (v4 `encodeErrorEvent` with the bug-241
/// extras). Frames: `{kind:"error", message, errorType, details,
/// userMessagePersisted?, pendingToolResultsPersisted?}`; legacy: the old
/// `{error, errorType, details, …}` bag.
pub fn render_error(payload: &ChatErrorPayload, dialect: Dialect) -> Value {
    let mut m = Map::new();
    m.insert("kind".into(), Value::String("error".into()));
    m.insert("message".into(), Value::String(payload.error.clone()));
    m.insert(
        "errorType".into(),
        Value::String(payload.error_type.clone()),
    );
    m.insert("details".into(), Value::String(payload.details.clone()));
    if let Some(b) = payload.user_message_persisted {
        m.insert("userMessagePersisted".into(), Value::Bool(b));
    }
    if let Some(b) = payload.pending_tool_results_persisted {
        m.insert("pendingToolResultsPersisted".into(), Value::Bool(b));
    }
    let frame = Value::Object(m);
    match dialect {
        Dialect::Frames => frame,
        Dialect::Legacy => to_legacy_bag(&frame),
    }
}

/// The swipe / "Try uncensored" status frame (v4 `regenerate-swipe-stream.ts`:
/// `encodeStatusEvent(encoder, event)` with the WHOLE progress event, so the
/// nested object keeps its own `kind` — nothing strips it; the schema is
/// non-strict).
pub fn swipe_status_frame(status: &Value) -> Value {
    kinded("status", [("status", status.clone())])
}

/// The swipe stream's terminal frame (v4 `{ kind: 'swipeDone', message }`).
pub fn swipe_done_frame(message: &Value) -> Value {
    kinded("swipeDone", [("message", message.clone())])
}

/// `{ <flag>: true, ...rest }` where `rest` is the frame minus `kind` / `seat`
/// (v4's `const { kind, seat, ...rest } = frame`).
fn flag_then_rest(flag: &str, frame: &Map<String, Value>) -> Map<String, Value> {
    let mut m = Map::new();
    m.insert(flag.into(), Value::Bool(true));
    for (k, v) in frame {
        if k != "kind" && k != "seat" {
            m.insert(k.clone(), v.clone());
        }
    }
    m
}

/// `{ <key>: { ...rest } }` (the nested `toolResult` / `confirmationResult`).
fn nested_rest(key: &str, frame: &Map<String, Value>) -> Map<String, Value> {
    let mut inner = Map::new();
    for (k, v) in frame {
        if k != "kind" && k != "seat" {
            inner.insert(k.clone(), v.clone());
        }
    }
    let mut m = Map::new();
    m.insert(key.into(), Value::Object(inner));
    m
}

fn single(key: &str, value: Value) -> Map<String, Value> {
    let mut m = Map::new();
    m.insert(key.into(), value);
    m
}

fn get(frame: &Map<String, Value>, key: &str) -> Value {
    frame.get(key).cloned().unwrap_or(Value::Null)
}

/// v4 `legacyBagOf` — each frame rebuilt as its old single-key / flag bag.
fn legacy_bag_of(frame: &Map<String, Value>) -> Map<String, Value> {
    let kind = frame.get("kind").and_then(Value::as_str).unwrap_or("");
    match kind {
        "content" => single("content", get(frame, "text")),
        "reasoning" => single("reasoning", get(frame, "text")),
        "status" => single("status", get(frame, "status")),
        "fileProcessing" => single("fileProcessing", get(frame, "info")),
        "toolsDetected" => {
            let names = get(frame, "names");
            let count = names.as_array().map(Vec::len).unwrap_or(0);
            let mut m = Map::new();
            m.insert("toolsDetected".into(), Value::from(count));
            m.insert("toolNames".into(), names);
            m.insert("toolArguments".into(), get(frame, "arguments"));
            m
        }
        "toolResult" => nested_rest("toolResult", frame),
        "turnStart" => flag_then_rest("turnStart", frame),
        "done" => {
            // `{ done: true, ...rest, ...legacyOutcomeFlags(outcome) }` — the
            // flags at the END (a re-assigned key would keep its place, as in JS).
            let mut m = Map::new();
            m.insert("done".into(), Value::Bool(true));
            for (k, v) in frame {
                if k != "kind" && k != "seat" && k != "outcome" {
                    m.insert(k.clone(), v.clone());
                }
            }
            match frame.get("outcome").and_then(Value::as_str) {
                Some("skipped") => {
                    m.insert("skipped".into(), Value::Bool(true));
                }
                Some("yielded") => {
                    m.insert("skipped".into(), Value::Bool(true));
                    m.insert("yielded".into(), Value::Bool(true));
                }
                Some("stopped") => {
                    m.insert("stopped".into(), Value::Bool(true));
                }
                Some("empty") => {
                    m.insert("emptyResponse".into(), Value::Bool(true));
                }
                Some("finished") => {}
                other => panic!("Unhandled stream frame outcome: {other:?}"),
            }
            m
        }
        "turnComplete" => flag_then_rest("turnComplete", frame),
        "chainComplete" => flag_then_rest("chainComplete", frame),
        "error" => {
            // `{ error: message, ...rest }` (rest = minus kind / seat / message).
            let mut m = Map::new();
            m.insert("error".into(), get(frame, "message"));
            for (k, v) in frame {
                if k != "kind" && k != "seat" && k != "message" {
                    m.insert(k.clone(), v.clone());
                }
            }
            m
        }
        "pendingExternalTurn" => flag_then_rest("pendingExternalTurn", frame),
        "posted" => {
            let what = frame
                .get("what")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            single(&what, get(frame, "message"))
        }
        "storytellerRound" => single("storytellerRound", get(frame, "round")),
        "storytellerBid" => {
            let mut inner = Map::new();
            inner.insert("roundId".into(), get(frame, "roundId"));
            inner.insert("bid".into(), get(frame, "bid"));
            single("storytellerBid", Value::Object(inner))
        }
        "roundOpen" => {
            let mut m = Map::new();
            m.insert("roundOpen".into(), Value::Bool(true));
            m.insert("roundId".into(), get(frame, "roundId"));
            m
        }
        "handAsked" => flag_then_rest("handAsked", frame),
        "handAnswered" => flag_then_rest("handAnswered", frame),
        "confirmationResult" => nested_rest("confirmationResult", frame),
        "swipeDone" => {
            let mut m = Map::new();
            m.insert("done".into(), Value::Bool(true));
            m.insert("message".into(), get(frame, "message"));
            m
        }
        // v4 `assertNever` — `render` never builds another kind.
        other => panic!("Unhandled stream frame variant: {other:?}"),
    }
}

/// v4 `toLegacyBag(frame)`: the bag, then — when seated — `{ ...bag,
/// participantId: bag.participantId ?? seat.participantId, concurrent: true }`.
///
/// JS `??` replaces a `null` too, and re-assigning an existing key KEEPS its
/// position — so a non-null `participantId` stays in place, a `null` one is
/// overwritten in place, an absent one is appended; `concurrent: true` lands
/// after it. (This is the merge the pre-4.12 `tagSseChunk` / v5's retired
/// `storyteller::sse_tag::tag_frame` did on bytes.)
pub fn to_legacy_bag(frame: &Value) -> Value {
    let Value::Object(f) = frame else {
        return frame.clone();
    };
    let mut bag = legacy_bag_of(f);
    if let Some(seat_pid) = f.get("seat").and_then(|s| s.get("participantId")).cloned() {
        let pid = match bag.get("participantId") {
            Some(v) if !v.is_null() => v.clone(),
            _ => seat_pid,
        };
        bag.insert("participantId".into(), pid);
        bag.insert("concurrent".into(), Value::Bool(true));
    }
    Value::Object(bag)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::chat_events::*;
    use serde_json::json;

    fn s(v: &Value) -> String {
        serde_json::to_string(v).unwrap()
    }

    fn both(ev: &ChatEvent) -> (String, String) {
        (
            s(&render(ev, Dialect::Frames, None)),
            s(&render(ev, Dialect::Legacy, None)),
        )
    }

    #[test]
    fn content_reasoning_status_bytes() {
        assert_eq!(
            both(&ChatEvent::content("hi")),
            (
                r#"{"kind":"content","text":"hi"}"#.into(),
                r#"{"content":"hi"}"#.into()
            )
        );
        assert_eq!(
            both(&ChatEvent::reasoning("hm")),
            (
                r#"{"kind":"reasoning","text":"hm"}"#.into(),
                r#"{"reasoning":"hm"}"#.into()
            )
        );
        let st = ChatEvent::status(StatusPayload {
            stage: "streaming".into(),
            message: "Ada is typing".into(),
            tool_name: None,
            character_name: Some("Ada".into()),
            character_id: None,
        });
        assert_eq!(
            both(&st),
            (
                r#"{"kind":"status","status":{"stage":"streaming","message":"Ada is typing","characterName":"Ada"}}"#.into(),
                r#"{"status":{"stage":"streaming","message":"Ada is typing","characterName":"Ada"}}"#.into()
            )
        );
    }

    #[test]
    fn tools_frames_bytes() {
        let td = ChatEvent::tools_detected(
            vec!["a".into(), "b".into()],
            vec![json!({"x":1}), json!({})],
        );
        assert_eq!(
            both(&td),
            (
                r#"{"kind":"toolsDetected","names":["a","b"],"arguments":[{"x":1},{}]}"#.into(),
                r#"{"toolsDetected":2,"toolNames":["a","b"],"toolArguments":[{"x":1},{}]}"#.into()
            )
        );
        let fail = ChatEvent::tool_result(ToolResultPayload {
            index: 0,
            name: "a".into(),
            success: false,
            result: Value::Null,
            error: Some("boom".into()),
        });
        assert_eq!(
            both(&fail),
            (
                r#"{"kind":"toolResult","index":0,"name":"a","success":false,"result":null,"error":"boom"}"#.into(),
                r#"{"toolResult":{"index":0,"name":"a","success":false,"result":null,"error":"boom"}}"#.into()
            )
        );
    }

    #[test]
    fn hand_and_turn_frames_bytes() {
        assert_eq!(
            both(&ChatEvent::hand_asked("p9", "Ada")),
            (
                r#"{"kind":"handAsked","participantId":"p9","characterName":"Ada"}"#.into(),
                r#"{"handAsked":true,"participantId":"p9","characterName":"Ada"}"#.into()
            )
        );
        assert_eq!(
            both(&ChatEvent::hand_answered("p9", "Ada", true)),
            (
                r#"{"kind":"handAnswered","participantId":"p9","characterName":"Ada","yielded":true}"#.into(),
                r#"{"handAnswered":true,"participantId":"p9","characterName":"Ada","yielded":true}"#.into()
            )
        );
        let cc = ChatEvent::chain_complete(ChainCompletePayload {
            reason: "paused".into(),
            next_speaker_id: None,
            chain_depth: 2,
            paused: Some(true),
            held_user_turn: Some(true),
        });
        assert_eq!(
            both(&cc),
            (
                r#"{"kind":"chainComplete","reason":"paused","nextSpeakerId":null,"chainDepth":2,"paused":true,"heldUserTurn":true}"#.into(),
                r#"{"chainComplete":true,"reason":"paused","nextSpeakerId":null,"chainDepth":2,"paused":true,"heldUserTurn":true}"#.into()
            )
        );
    }

    #[test]
    fn posted_error_storyteller_confirmation_bytes() {
        assert_eq!(
            both(&ChatEvent::host_announcement(json!({"id":"m1"}))),
            (
                r#"{"kind":"posted","what":"hostAnnouncement","message":{"id":"m1"}}"#.into(),
                r#"{"hostAnnouncement":{"id":"m1"}}"#.into()
            )
        );
        assert_eq!(
            both(&ChatEvent::error("Oops", "processing_error", "d")),
            (
                r#"{"kind":"error","message":"Oops","errorType":"processing_error","details":"d"}"#
                    .into(),
                r#"{"error":"Oops","errorType":"processing_error","details":"d"}"#.into()
            )
        );
        let bid = ChatEvent::StorytellerBid {
            storyteller_bid: StorytellerBidFrame {
                round_id: "r1".into(),
                bid: json!({"participantId":"p1"}),
            },
        };
        assert_eq!(
            both(&bid),
            (
                r#"{"kind":"storytellerBid","roundId":"r1","bid":{"participantId":"p1"}}"#.into(),
                r#"{"storytellerBid":{"roundId":"r1","bid":{"participantId":"p1"}}}"#.into()
            )
        );
        let open = ChatEvent::RoundOpen {
            round_open: TrueBool,
            round_id: "r1".into(),
        };
        assert_eq!(
            both(&open),
            (
                r#"{"kind":"roundOpen","roundId":"r1"}"#.into(),
                r#"{"roundOpen":true,"roundId":"r1"}"#.into()
            )
        );
        let conf = ChatEvent::confirmation_result(ConfirmationResultPayload {
            message_id: "m1".into(),
            confirmed: None,
            revised: false,
            notes: None,
            content: None,
        });
        assert_eq!(
            both(&conf),
            (
                r#"{"kind":"confirmationResult","messageId":"m1","confirmed":null,"revised":false,"notes":null}"#.into(),
                r#"{"confirmationResult":{"messageId":"m1","confirmed":null,"revised":false,"notes":null}}"#.into()
            )
        );
    }

    fn turn_done(f: impl FnOnce(&mut TurnDonePayload)) -> ChatEvent {
        let mut p = TurnDonePayload {
            usage: None,
            cache_usage: None,
            attachment_results: None,
            provider: "OPENAI".into(),
            model_name: "m".into(),
            message_id: None,
            participant_id: "p1".into(),
            tools_executed: false,
            ..Default::default()
        };
        f(&mut p);
        ChatEvent::turn_done(p)
    }

    #[test]
    fn the_five_outcomes_on_both_dialects() {
        const HEAD: &str = r#""usage":null,"cacheUsage":null,"attachmentResults":null,"provider":"OPENAI","modelName":"m","messageId":null,"participantId":"p1","toolsExecuted":false"#;
        let cases: Vec<(ChatEvent, &str, &str, &str)> = vec![
            (turn_done(|_| {}), "finished", "", ""),
            (
                turn_done(|p| {
                    p.skipped = Some(true);
                    p.skipped_participant_id = Some("p1".into());
                }),
                "skipped",
                r#","skippedParticipantId":"p1""#,
                r#","skipped":true"#,
            ),
            (
                turn_done(|p| {
                    p.skipped = Some(true);
                    p.skipped_participant_id = Some("p1".into());
                    p.yielded = Some(true);
                }),
                "yielded",
                r#","skippedParticipantId":"p1""#,
                r#","skipped":true,"yielded":true"#,
            ),
            (
                turn_done(|p| p.stopped = Some(true)),
                "stopped",
                "",
                r#","stopped":true"#,
            ),
            (
                turn_done(|p| {
                    p.empty_response = Some(true);
                    p.empty_response_reason = Some("nothing".into());
                }),
                "empty",
                r#","emptyResponseReason":"nothing""#,
                r#","emptyResponse":true"#,
            ),
        ];
        for (ev, outcome, tail, flags) in cases {
            let (frames, legacy) = both(&ev);
            assert_eq!(
                frames,
                format!(r#"{{"kind":"done","outcome":"{outcome}",{HEAD}{tail}}}"#)
            );
            assert_eq!(legacy, format!(r#"{{"done":true,{HEAD}{tail}{flags}}}"#));
        }
    }

    #[test]
    fn outcome_precedence_stopped_beats_skipped() {
        let ev = turn_done(|p| {
            p.skipped = Some(true);
            p.stopped = Some(true);
        });
        assert_eq!(outcome_of(&ev), Some(DoneOutcome::Stopped));
        let ev = turn_done(|p| {
            p.skipped = Some(true);
            p.yielded = Some(true);
        });
        assert_eq!(outcome_of(&ev), Some(DoneOutcome::Yielded));
        assert_eq!(outcome_of(&ChatEvent::content("x")), None);
        let rec = ChatEvent::done(DonePayload {
            message_id: Some("m1".into()),
            ..Default::default()
        });
        assert_eq!(outcome_of(&rec), Some(DoneOutcome::Finished));
        assert_eq!(
            s(&render(&rec, Dialect::Frames, None)),
            r#"{"kind":"done","outcome":"finished","messageId":"m1","usage":null,"cacheUsage":null,"attachmentResults":null,"toolsExecuted":false}"#
        );
    }

    #[test]
    fn the_seat_on_each_dialect() {
        let ev = ChatEvent::content("hi");
        assert_eq!(
            s(&render(&ev, Dialect::Frames, Some("p-seat"))),
            r#"{"kind":"content","text":"hi","seat":{"participantId":"p-seat","concurrent":true}}"#
        );
        assert_eq!(
            s(&render(&ev, Dialect::Legacy, Some("p-seat"))),
            r#"{"content":"hi","participantId":"p-seat","concurrent":true}"#
        );
        // A frame that already names a participant keeps it, in place.
        let ts = ChatEvent::turn_start(TurnStartPayload {
            participant_id: "p0".into(),
            character_name: "Ada".into(),
            chain_depth: 0,
        });
        assert_eq!(
            s(&render(&ts, Dialect::Legacy, Some("p1"))),
            r#"{"turnStart":true,"participantId":"p0","characterName":"Ada","chainDepth":0,"concurrent":true}"#
        );
        // A `null` participant is replaced in place (JS `??`).
        let done = ChatEvent::done(DonePayload::default());
        let legacy = s(&render(&done, Dialect::Legacy, Some("p1")));
        assert_eq!(
            legacy,
            r#"{"done":true,"messageId":null,"usage":null,"cacheUsage":null,"attachmentResults":null,"toolsExecuted":false,"participantId":"p1","concurrent":true}"#
        );
    }

    #[test]
    fn error_and_swipe_frames() {
        let p = ChatErrorPayload {
            error: "Failed to generate response".into(),
            error_type: "Error".into(),
            details: "boom".into(),
            user_message_persisted: Some(true),
            pending_tool_results_persisted: Some(false),
        };
        assert_eq!(
            s(&render_error(&p, Dialect::Frames)),
            r#"{"kind":"error","message":"Failed to generate response","errorType":"Error","details":"boom","userMessagePersisted":true,"pendingToolResultsPersisted":false}"#
        );
        assert_eq!(
            s(&render_error(&p, Dialect::Legacy)),
            r#"{"error":"Failed to generate response","errorType":"Error","details":"boom","userMessagePersisted":true,"pendingToolResultsPersisted":false}"#
        );
        assert_eq!(
            s(&swipe_status_frame(
                &json!({"kind":"status","stage":"s","message":"m"})
            )),
            r#"{"kind":"status","status":{"kind":"status","stage":"s","message":"m"}}"#
        );
        let done = swipe_done_frame(&json!({"id":"m1"}));
        assert_eq!(s(&done), r#"{"kind":"swipeDone","message":{"id":"m1"}}"#);
        assert_eq!(
            s(&to_legacy_bag(&done)),
            r#"{"done":true,"message":{"id":"m1"}}"#
        );
    }
}
