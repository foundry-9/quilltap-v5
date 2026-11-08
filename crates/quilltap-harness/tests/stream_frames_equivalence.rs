//! P4.D366 tier-1 differential: the Salon stream frames (v4 `604a082c4`
//! `lib/chat/stream-frames.ts` at the `624d0d25c` pin) vs
//! `quilltap_core::services::stream_frames::render` — the ONE renderer.
//!
//! The oracle (`harness/oracle/cases/stream-frames.ts`) drives v4's REAL
//! `encodeFrame` in its three encoder profiles (a plain `TextEncoder` →
//! frames, `legacyFrameEncoder()`, `seatedFrameEncoder('p-seat')`), plus
//! `toLegacyBag(tagFrame(frame, 'p-seat'))`, over the committed corpus
//! `harness/oracle/fixtures/stream-frames.json`; `done` rows go through v4's
//! REAL `encodeDoneEvent` / `encodeTurnDoneEvent`; and v4's six GOLDEN streams
//! are translated by v4's REAL `fromLegacyBag` (the oracle asserts v4's own
//! golden invariant at the pin and throws if it fails).
//!
//! This side decodes each v4 FRAMES body through a STRICT test-only
//! `chat_event_from_frame` (an unknown key PANICS — it never launders a key)
//! into the `ChatEvent` a v5 producer emits, renders it on each dialect / seat,
//! and compares `serde_json::to_string` byte for byte — never `Value ==`,
//! which is blind to key order (memory note `serde-json-map-eq-ignores-order`).
//!
//! ## Named carves (both ways — each pinned by count; a moved count is red)
//!
//! - `TOOL_RESULT_UNDEFINED_RESULT` — v4's `ToolResult.result` is `unknown`, so a
//!   tool handler's `undefined` result is admissible and `JSON.stringify` drops
//!   the key; v5's `ToolResultPayload.result` is a `Value` (no `undefined`) and
//!   always writes `"result":null`. The corpus row `tool_result_undefined_result`
//!   is the vector: this side asserts v5's bytes equal v4's with `"result":null`
//!   inserted after `success`. Retires when v5's payload grows an omittable
//!   `result`.
//! - `GOLDEN_SCHEMA_ORDER` — v4's `fromLegacyBag` parses each translated frame
//!   through `StreamFrameSchema.parse`, and Zod 4 rebuilds the object in the
//!   SCHEMA's key order (measured at the pin), not any producer's. So a golden
//!   frame whose producer order differs from the schema's (e.g. a finalizer
//!   `done` with `turn` before `provider` vs the schema's `emptyResponseReason`
//!   … order) cannot byte-match a v5 producer. Those rows compare
//!   KEY-SORTED (every key and value still checked); producer order is pinned
//!   by the corpus sections, which never go through Zod. The count of such rows
//!   is pinned.
//!
//! Regenerate + run (a pure tsx oracle; the corpus is committed):
//!   V5W=${V5W:-$HOME/source/quilltap-v5}
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   rm -f /tmp/oracle-stream-frames.ndjson
//!   cd ~/source/quilltap-server
//!   $N/npx tsx $V5W/harness/oracle/cases/stream-frames.ts \
//!     > /tmp/oracle-stream-frames.ndjson
//!   cd $V5W
//!   QT_ORACLE_STREAM_FRAMES=/tmp/oracle-stream-frames.ndjson \
//!     cargo test -p quilltap-harness --test stream_frames_equivalence -- --nocapture

use std::collections::BTreeMap;

use quilltap_core::services::chat_events::{
    ChainCompletePayload, ChatEvent, ConfirmationResultPayload, DoneCacheUsage, DonePayload,
    DoneReasoningSegment, DoneTurn, DoneUsage, FileProcessingEntry, Omittable,
    PendingExternalTurnPayload, StatusPayload, StorytellerBidFrame, ToolResultPayload, TrueBool,
    TurnCompletePayload, TurnDonePayload, TurnStartPayload,
};
use quilltap_core::services::file_fallback::FallbackType;
use quilltap_core::services::route_trail::RouteAttempt;
use quilltap_core::services::stream_frames::{
    render, swipe_done_frame, swipe_status_frame, to_legacy_bag, Dialect,
};
use serde_json::{Map, Value};

const SEAT: &str = "p-seat";
const EXPECTED_TOOL_RESULT_UNDEFINED: usize = 1;

/// A strict key reader: every key the decoder reads is consumed; `finish`
/// panics on any key left over.
struct Keys {
    kind: String,
    m: Map<String, Value>,
}

impl Keys {
    fn take(&mut self, k: &str) -> Option<Value> {
        // `shift_remove`: `remove` is a SWAP-remove under `preserve_order` and
        // would move the last key into the hole (the `done` dispatch reads
        // the key after `outcome`).
        self.m.shift_remove(k)
    }
    fn req(&mut self, k: &str) -> Value {
        self.take(k)
            .unwrap_or_else(|| panic!("{} frame lacks `{k}`", self.kind))
    }
    fn s(&mut self, k: &str) -> String {
        self.req(k).as_str().expect(k).to_string()
    }
    fn opt_s(&mut self, k: &str) -> Option<String> {
        self.take(k).map(|v| v.as_str().expect(k).to_string())
    }
    fn nullable_s(&mut self, k: &str) -> Option<String> {
        match self.req(k) {
            Value::Null => None,
            v => Some(v.as_str().expect(k).to_string()),
        }
    }
    fn b(&mut self, k: &str) -> bool {
        self.req(k).as_bool().expect(k)
    }
    fn opt_b(&mut self, k: &str) -> Option<bool> {
        self.take(k).map(|v| v.as_bool().expect(k))
    }
    fn i(&mut self, k: &str) -> i64 {
        self.req(k).as_i64().expect(k)
    }
    fn finish(self) {
        assert!(
            self.m.is_empty(),
            "{} frame carries keys the decoder does not know: {:?}",
            self.kind,
            self.m.keys().collect::<Vec<_>>()
        );
    }
}

fn status_payload(v: Value) -> StatusPayload {
    let Value::Object(m) = v else {
        panic!("status is not an object")
    };
    let mut k = Keys {
        kind: "status.status".into(),
        m,
    };
    let p = StatusPayload {
        stage: k.s("stage"),
        message: k.s("message"),
        tool_name: k.opt_s("toolName"),
        character_name: k.opt_s("characterName"),
        character_id: k.opt_s("characterId"),
    };
    k.finish();
    p
}

fn usage(v: Value) -> Option<DoneUsage> {
    match v {
        Value::Null => None,
        Value::Object(m) => {
            let mut k = Keys {
                kind: "usage".into(),
                m,
            };
            let u = DoneUsage {
                prompt_tokens: k.take("promptTokens").map(|v| v.as_i64().unwrap()),
                completion_tokens: k.take("completionTokens").map(|v| v.as_i64().unwrap()),
                total_tokens: k.take("totalTokens").map(|v| v.as_i64().unwrap()),
            };
            k.finish();
            Some(u)
        }
        other => panic!("usage {other}"),
    }
}

fn cache_usage(v: Value) -> Option<DoneCacheUsage> {
    match v {
        Value::Null => None,
        Value::Object(m) => {
            let mut k = Keys {
                kind: "cacheUsage".into(),
                m,
            };
            let u = DoneCacheUsage {
                cache_creation_input_tokens: k
                    .take("cacheCreationInputTokens")
                    .map(|v| v.as_i64().unwrap()),
                cache_read_input_tokens: k
                    .take("cacheReadInputTokens")
                    .map(|v| v.as_i64().unwrap()),
            };
            k.finish();
            Some(u)
        }
        other => panic!("cacheUsage {other}"),
    }
}

fn nullable_value(v: Value) -> Option<Value> {
    match v {
        Value::Null => None,
        v => Some(v),
    }
}

/// The outcome's flags as v5's producers set them (a bare yield rides with
/// `skipped`, P4.D335).
fn flags(outcome: &str) -> (Option<bool>, Option<bool>, Option<bool>, Option<bool>) {
    // (stopped, skipped, yielded, empty_response)
    match outcome {
        "finished" => (None, None, None, None),
        "skipped" => (None, Some(true), None, None),
        "yielded" => (None, Some(true), Some(true), None),
        "stopped" => (Some(true), None, None, None),
        "empty" => (None, None, None, Some(true)),
        other => panic!("unknown outcome {other}"),
    }
}

fn done_event(mut k: Keys) -> ChatEvent {
    let outcome = k.s("outcome");
    let (stopped, skipped, yielded, empty_response) = flags(&outcome);
    // `done` maps to `TurnDonePayload` when the key after `outcome` is `usage`
    // (v4 `encodeTurnDoneEvent`'s streaming head), else `DonePayload`.
    let turn_close = k.m.keys().next().map(String::as_str) == Some("usage");
    if turn_close {
        let p = TurnDonePayload {
            usage: usage(k.req("usage")),
            cache_usage: cache_usage(k.req("cacheUsage")),
            attachment_results: nullable_value(k.req("attachmentResults")),
            provider: k.s("provider"),
            model_name: k.s("modelName"),
            message_id: k.nullable_s("messageId"),
            participant_id: k.s("participantId"),
            tools_executed: k.b("toolsExecuted"),
            skipped,
            skipped_participant_id: k.opt_s("skippedParticipantId"),
            yielded,
            stopped,
            empty_response,
            empty_response_reason: k.opt_s("emptyResponseReason"),
        };
        k.finish();
        return ChatEvent::turn_done(p);
    }
    let p = DonePayload {
        message_id: k.nullable_s("messageId"),
        participant_id: match k.take("participantId") {
            None | Some(Value::Null) => None,
            Some(v) => Some(v.as_str().unwrap().to_string()),
        },
        usage: usage(k.req("usage")),
        cache_usage: cache_usage(k.req("cacheUsage")),
        attachment_results: nullable_value(k.req("attachmentResults")),
        tools_executed: k.b("toolsExecuted"),
        stopped,
        skipped,
        skipped_participant_id: k.opt_s("skippedParticipantId"),
        yielded,
        turn: k.take("turn").map(|v| {
            let Value::Object(m) = v else { panic!("turn") };
            let mut t = Keys {
                kind: "turn".into(),
                m,
            };
            let d = DoneTurn {
                next_speaker_id: t.nullable_s("nextSpeakerId"),
                reason: t.s("reason"),
                cycle_complete: t.b("cycleComplete"),
                is_users_turn: t.b("isUsersTurn"),
            };
            t.finish();
            d
        }),
        provider: k.opt_s("provider"),
        model_name: k.opt_s("modelName"),
        pending_external_turn: k.opt_b("pendingExternalTurn"),
        route_trail: k.take("routeTrail").map(|v| match v {
            Value::Null => None,
            Value::Array(a) => Some(
                a.iter()
                    .map(|x| RouteAttempt::from_value(x).expect("a route attempt decodes"))
                    .collect(),
            ),
            other => panic!("routeTrail {other}"),
        }),
        is_silent_message: k.opt_b("isSilentMessage"),
        empty_response,
        empty_response_reason: k.opt_s("emptyResponseReason"),
        reasoning_content: match k.take("reasoningContent") {
            None => Omittable::Absent,
            Some(Value::Null) => Omittable::Null,
            Some(v) => Omittable::Value(v.as_str().unwrap().to_string()),
        },
        reasoning_segments: match k.take("reasoningSegments") {
            None => Omittable::Absent,
            Some(Value::Null) => Omittable::Null,
            Some(Value::Array(a)) => Omittable::Value(
                a.into_iter()
                    .map(|x| {
                        let Value::Object(m) = x else {
                            panic!("segment")
                        };
                        let mut s = Keys {
                            kind: "reasoningSegment".into(),
                            m,
                        };
                        let seg = DoneReasoningSegment {
                            anchor_offset: s.i("anchorOffset") as usize,
                            content: s.s("content"),
                            seq: s.i("seq") as u64,
                        };
                        s.finish();
                        seg
                    })
                    .collect(),
            ),
            Some(other) => panic!("reasoningSegments {other}"),
        },
    };
    k.finish();
    ChatEvent::done(p)
}

fn file_entry(v: Value) -> FileProcessingEntry {
    let Value::Object(m) = v else {
        panic!("fileProcessing entry")
    };
    let mut k = Keys {
        kind: "fileProcessing.info".into(),
        m,
    };
    let e = FileProcessingEntry {
        filename: k.s("filename"),
        type_: match k.s("type").as_str() {
            "text" => FallbackType::Text,
            "image_description" => FallbackType::ImageDescription,
            "unsupported" => FallbackType::Unsupported,
            other => panic!("fallback type {other}"),
        },
        used_image_description_llm: k.b("usedImageDescriptionLLM"),
        error: k.opt_s("error"),
    };
    k.finish();
    e
}

/// A v4 FRAMES body → the `ChatEvent` a v5 producer emits + its seat. Strict:
/// an unknown key panics. Returns `undefined_result = true` when a
/// `toolResult` lacked `result` (the `TOOL_RESULT_UNDEFINED_RESULT` carve).
fn chat_event_from_frame(frame: &Value) -> (ChatEvent, Option<String>, bool) {
    let Value::Object(m) = frame.clone() else {
        panic!("a frame is an object")
    };
    let mut k = Keys {
        kind: String::new(),
        m,
    };
    k.kind = k.s("kind");
    let seat = k.take("seat").map(|s| {
        assert_eq!(s["concurrent"], Value::Bool(true));
        s["participantId"].as_str().unwrap().to_string()
    });
    let mut undefined_result = false;
    let ev = match k.kind.clone().as_str() {
        "content" => ChatEvent::content(k.s("text")),
        "reasoning" => ChatEvent::reasoning(k.s("text")),
        "status" => ChatEvent::status(status_payload(k.req("status"))),
        "fileProcessing" => {
            let Value::Array(a) = k.req("info") else {
                panic!("info")
            };
            ChatEvent::file_processing(a.into_iter().map(file_entry).collect())
        }
        "toolsDetected" => {
            let names = k.req("names");
            let names = names
                .as_array()
                .unwrap()
                .iter()
                .map(|n| n.as_str().unwrap().to_string())
                .collect();
            let Value::Array(args) = k.req("arguments") else {
                panic!("arguments")
            };
            ChatEvent::tools_detected(names, args)
        }
        "toolResult" => {
            let index = k.i("index") as usize;
            let name = k.s("name");
            let success = k.b("success");
            let result = match k.take("result") {
                Some(v) => v,
                None => {
                    undefined_result = true;
                    Value::Null
                }
            };
            ChatEvent::tool_result(ToolResultPayload {
                index,
                name,
                success,
                result,
                error: k.opt_s("error"),
            })
        }
        "turnStart" => ChatEvent::turn_start(TurnStartPayload {
            participant_id: k.s("participantId"),
            character_name: k.s("characterName"),
            chain_depth: k.i("chainDepth"),
        }),
        "turnComplete" => ChatEvent::turn_complete(TurnCompletePayload {
            participant_id: k.s("participantId"),
            message_id: k.s("messageId"),
            chain_depth: k.i("chainDepth"),
            skipped: k.opt_b("skipped"),
        }),
        "chainComplete" => ChatEvent::chain_complete(ChainCompletePayload {
            reason: k.s("reason"),
            next_speaker_id: k.nullable_s("nextSpeakerId"),
            chain_depth: k.i("chainDepth"),
            paused: k.opt_b("paused"),
            held_user_turn: k.opt_b("heldUserTurn"),
        }),
        "error" => {
            let e = ChatEvent::error(k.s("message"), k.s("errorType"), k.s("details"));
            // The shell extras ride `ChatErrorPayload`, not `ChatEvent::Error`
            // — `render_error` is compared on those rows (see `is_shell_error`).
            assert!(
                k.m.is_empty(),
                "an error frame with extras is a shell frame"
            );
            e
        }
        "pendingExternalTurn" => ChatEvent::pending_external_turn(PendingExternalTurnPayload {
            message_id: k.s("messageId"),
            participant_id: k.s("participantId"),
            character_name: k.s("characterName"),
        }),
        "posted" => {
            let what = k.s("what");
            let message = k.req("message");
            match what.as_str() {
                "carinaAnswer" => ChatEvent::carina_answer(message),
                "hostAnnouncement" => ChatEvent::host_announcement(message),
                "pascalResult" => ChatEvent::pascal_result(message),
                other => panic!("posted what {other}"),
            }
        }
        "storytellerRound" => ChatEvent::StorytellerRound {
            storyteller_round: k.req("round"),
        },
        "storytellerBid" => ChatEvent::StorytellerBid {
            storyteller_bid: StorytellerBidFrame {
                round_id: k.s("roundId"),
                bid: k.req("bid"),
            },
        },
        "roundOpen" => ChatEvent::RoundOpen {
            round_open: TrueBool,
            round_id: k.s("roundId"),
        },
        "handAsked" => ChatEvent::hand_asked(&k.s("participantId"), &k.s("characterName")),
        "handAnswered" => {
            let pid = k.s("participantId");
            let name = k.s("characterName");
            ChatEvent::hand_answered(&pid, &name, k.b("yielded"))
        }
        "confirmationResult" => ChatEvent::confirmation_result(ConfirmationResultPayload {
            message_id: k.s("messageId"),
            confirmed: match k.req("confirmed") {
                Value::Null => None,
                v => Some(v.as_bool().unwrap()),
            },
            revised: k.b("revised"),
            notes: k.nullable_s("notes"),
            content: k.opt_s("content"),
        }),
        "done" => return (done_event(k), seat, false),
        other => panic!("unknown frame kind {other}"),
    };
    k.finish();
    (ev, seat, undefined_result)
}

/// A frame `ChatEvent` cannot carry: the transport-shell error (the bug-241
/// extras ride `ChatErrorPayload`) and the swipe's `swipeDone` — built by the
/// `stream_frames` helpers instead.
fn non_event_frame(f: &Value) -> Option<Value> {
    let kind = f["kind"].as_str().unwrap_or("");
    if kind == "swipeDone" {
        return Some(swipe_done_frame(&f["message"]));
    }
    let shell = kind == "error"
        && (f.get("userMessagePersisted").is_some()
            || f.get("pendingToolResultsPersisted").is_some());
    if !shell {
        return None;
    }
    for k in f.as_object().unwrap().keys() {
        assert!(
            [
                "kind",
                "message",
                "errorType",
                "details",
                "userMessagePersisted",
                "pendingToolResultsPersisted"
            ]
            .contains(&k.as_str()),
            "a shell error frame with key {k}"
        );
    }
    Some(quilltap_core::services::stream_frames::render_error(
        &quilltap_core::api::ChatErrorPayload {
            error: f["message"].as_str().unwrap().into(),
            error_type: f["errorType"].as_str().unwrap().into(),
            details: f["details"].as_str().unwrap().into(),
            user_message_persisted: f.get("userMessagePersisted").and_then(Value::as_bool),
            pending_tool_results_persisted: f
                .get("pendingToolResultsPersisted")
                .and_then(Value::as_bool),
        },
        Dialect::Frames,
    ))
}

fn bytes(v: &Value) -> String {
    serde_json::to_string(v).unwrap()
}

/// Recursively key-sorted bytes (the `GOLDEN_SCHEMA_ORDER` comparand).
fn sorted(v: &Value) -> String {
    fn canon(v: &Value) -> Value {
        match v {
            Value::Object(m) => {
                let b: BTreeMap<&String, Value> = m.iter().map(|(k, v)| (k, canon(v))).collect();
                let mut out = Map::new();
                for (k, v) in b {
                    out.insert(k.clone(), v);
                }
                Value::Object(out)
            }
            Value::Array(a) => Value::Array(a.iter().map(canon).collect()),
            v => v.clone(),
        }
    }
    bytes(&canon(v))
}

/// v4's bytes with `"result":null` inserted after `success` (the carve).
fn with_null_result(v4: &Value) -> Value {
    fn ins(m: &Map<String, Value>) -> Map<String, Value> {
        let mut out = Map::new();
        for (k, v) in m {
            out.insert(k.clone(), v.clone());
            if k == "success" {
                out.insert("result".into(), Value::Null);
            }
        }
        out
    }
    let Value::Object(m) = v4 else {
        return v4.clone();
    };
    match m.get("toolResult") {
        // legacy: nested
        Some(Value::Object(inner)) => {
            let mut out = Map::new();
            for (k, v) in m {
                if k == "toolResult" {
                    out.insert(k.clone(), Value::Object(ins(inner)));
                } else {
                    out.insert(k.clone(), v.clone());
                }
            }
            Value::Object(out)
        }
        _ => Value::Object(ins(m)),
    }
}

#[test]
fn stream_frames_match_oracle() {
    let Ok(path) = std::env::var("QT_ORACLE_STREAM_FRAMES") else {
        eprintln!("SKIP: QT_ORACLE_STREAM_FRAMES unset");
        return;
    };
    let text = std::fs::read_to_string(&path).expect("oracle readable");
    let rows: Vec<Value> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("ndjson row"))
        .collect();
    assert!(rows.len() > 300, "the oracle is short: {} rows", rows.len());

    // Group (section, name) → profile → body.
    let mut groups: BTreeMap<(String, String), BTreeMap<String, Value>> = BTreeMap::new();
    let mut order: Vec<(String, String)> = Vec::new();
    for r in &rows {
        let key = (
            r["section"].as_str().unwrap().to_string(),
            r["name"].as_str().unwrap().to_string(),
        );
        let body: Value = serde_json::from_str(r["body"].as_str().unwrap()).expect("body json");
        if !groups.contains_key(&key) {
            order.push(key.clone());
        }
        groups
            .entry(key)
            .or_default()
            .insert(r["profile"].as_str().unwrap().to_string(), body);
    }

    let mut compared = 0usize;
    let mut undefined_result_rows = 0usize;
    let mut golden_schema_order_rows = 0usize;
    let mut by_section: BTreeMap<String, usize> = BTreeMap::new();
    let mut failures: Vec<String> = Vec::new();
    let check = |label: String, got: &Value, want: &Value, failures: &mut Vec<String>| {
        if bytes(got) != bytes(want) {
            failures.push(format!(
                "{label}\n   v5: {}\n   v4: {}",
                bytes(got),
                bytes(want)
            ));
        }
    };

    for key in &order {
        let (section, name) = key;
        let p = &groups[key];
        *by_section.entry(section.clone()).or_default() += 1;
        match section.as_str() {
            "frames" | "done" | "turnDone" => {
                if let Some(got) = non_event_frame(&p["frames"]) {
                    check(
                        format!("{section}/{name}/frames"),
                        &got,
                        &p["frames"],
                        &mut failures,
                    );
                    check(
                        format!("{section}/{name}/legacy"),
                        &to_legacy_bag(&got),
                        &p["legacy"],
                        &mut failures,
                    );
                    compared += 2;
                    continue;
                }
                let (ev, seat, undef) = chat_event_from_frame(&p["frames"]);
                assert!(seat.is_none());
                if undef {
                    undefined_result_rows += 1;
                }
                let adj = |v: &Value| {
                    if undef {
                        with_null_result(v)
                    } else {
                        v.clone()
                    }
                };
                for (profile, dialect, seat) in [
                    ("frames", Dialect::Frames, None),
                    ("legacy", Dialect::Legacy, None),
                    ("seated", Dialect::Frames, Some(SEAT)),
                    ("legacySeated", Dialect::Legacy, Some(SEAT)),
                ] {
                    let got = render(&ev, dialect, seat);
                    check(
                        format!("{section}/{name}/{profile}"),
                        &got,
                        &adj(&p[profile]),
                        &mut failures,
                    );
                    compared += 1;
                }
            }
            "tagged" => {
                // `toLegacyBag(tagFrame(frame, seat))` — decode the frame the
                // corpus wrote (read back from the committed fixture).
                // The `??` arm (`participantId` present / `null` / absent) on
                // v4's `toLegacyBag` ported literally, over the frame VALUE —
                // a `done` with an explicit `participantId: null` is a v4
                // frame no v5 producer builds (`DonePayload.participant_id`
                // omits a `None`), so this section drives `to_legacy_bag`
                // directly, as v4's test drives `toLegacyBag(tagFrame(…))`.
                let mut frame = tagged_frame(name);
                frame.as_object_mut().unwrap().insert(
                    "seat".into(),
                    serde_json::json!({ "participantId": SEAT, "concurrent": true }),
                );
                check(
                    format!("tagged/{name}"),
                    &to_legacy_bag(&frame),
                    &p["legacySeated"],
                    &mut failures,
                );
                compared += 1;
            }
            "swipe" => {
                let f = &p["frames"];
                let got = match f["kind"].as_str().unwrap() {
                    "status" => swipe_status_frame(&f["status"]),
                    "swipeDone" => swipe_done_frame(&f["message"]),
                    "error" => quilltap_core::services::stream_frames::render_error(
                        &quilltap_core::api::ChatErrorPayload {
                            error: f["message"].as_str().unwrap().into(),
                            error_type: f["errorType"].as_str().unwrap().into(),
                            details: f["details"].as_str().unwrap().into(),
                            user_message_persisted: None,
                            pending_tool_results_persisted: None,
                        },
                        Dialect::Frames,
                    ),
                    other => panic!("swipe kind {other}"),
                };
                check(format!("swipe/{name}/frames"), &got, f, &mut failures);
                check(
                    format!("swipe/{name}/legacy"),
                    &to_legacy_bag(&got),
                    &p["legacy"],
                    &mut failures,
                );
                compared += 2;
            }
            "golden" => {
                let f = &p["frames"];
                let (got_frames, got_legacy) = if let Some(g) = non_event_frame(f) {
                    let l = to_legacy_bag(&g);
                    (g, l)
                } else {
                    let (ev, seat, undef) = chat_event_from_frame(f);
                    assert!(!undef, "a golden toolResult without `result`");
                    (
                        render(&ev, Dialect::Frames, seat.as_deref()),
                        render(&ev, Dialect::Legacy, seat.as_deref()),
                    )
                };
                let mut schema_order = false;
                for (profile, got) in [("frames", &got_frames), ("legacy", &got_legacy)] {
                    let want = &p[profile];
                    if bytes(got) != bytes(want) {
                        if sorted(got) == sorted(want) {
                            schema_order = true;
                        } else {
                            failures.push(format!(
                                "golden/{name}/{profile}\n   v5: {}\n   v4: {}",
                                bytes(got),
                                bytes(want)
                            ));
                        }
                    }
                    compared += 1;
                }
                if schema_order {
                    golden_schema_order_rows += 1;
                }
                // v4's own invariant, from this side: our legacy == the recorded bag.
                if sorted(&got_legacy) != sorted(&p["recorded"]) {
                    failures.push(format!(
                        "golden/{name}/recorded\n   v5: {}\n   recorded: {}",
                        bytes(&got_legacy),
                        bytes(&p["recorded"])
                    ));
                }
            }
            other => panic!("unknown section {other}"),
        }
    }

    eprintln!(
        "stream_frames_equivalence: {} groups {by_section:?}, {compared} comparisons, \
         TOOL_RESULT_UNDEFINED_RESULT={undefined_result_rows}, GOLDEN_SCHEMA_ORDER={golden_schema_order_rows}, {} failures",
        order.len(),
        failures.len()
    );
    assert!(
        failures.is_empty(),
        "frame bytes differ:\n{}",
        failures.join("\n")
    );
    assert_eq!(
        undefined_result_rows, EXPECTED_TOOL_RESULT_UNDEFINED,
        "TOOL_RESULT_UNDEFINED_RESULT moved"
    );
    assert_eq!(
        golden_schema_order_rows, EXPECTED_GOLDEN_SCHEMA_ORDER,
        "GOLDEN_SCHEMA_ORDER moved"
    );
}

const EXPECTED_GOLDEN_SCHEMA_ORDER: usize = 0;

fn tagged_frame(name: &str) -> Value {
    let corpus: Value = serde_json::from_str(
        &std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../harness/oracle/fixtures/stream-frames.json"),
        )
        .expect("corpus"),
    )
    .expect("corpus json");
    corpus["tagged"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["name"] == name)
        .unwrap_or_else(|| panic!("tagged row {name}"))["frame"]
        .clone()
}
