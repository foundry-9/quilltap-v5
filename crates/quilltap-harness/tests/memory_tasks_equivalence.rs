//! Tier-1 differential test: the pure leaves of v4's ONE memory-extraction
//! pipeline — `quilltap_core::memory_extraction::{prompts, parse, window}`
//! (v4 `lib/memory/extraction/{prompts/*,parse,window}.ts`, pin `83334b9a5`;
//! work order P4.D296 Tier 1 #1–#3). The family keeps its pre-refactor name
//! (the order's L-20); its old subject (`cheap-llm-tasks/memory-tasks.ts`)
//! was deleted by `ac6348c5f`, so the case was rewritten.
//!
//! Both sides read the SAME committed corpus
//! (`harness/oracle/fixtures/extraction-pure-tier1.json`); the oracle drives
//! v4's REAL builders / renderers / parsers / fold rules and records one
//! `{ section, name, out }` row per op; this test drives the Rust port and
//! compares every row, exact (strings byte-for-byte; drafts and fold results
//! as JSON values).
//!
//! The two `fallback_env_*` clock rows carry no `timeZone`: v4 renders them in
//! `getMemoryTimeZone()`'s fallback (`QUILLTAP_TIMEZONE`, set per row). v5's
//! clock REQUIRES its zone (core never reads an ambient zone — P4.D289
//! LR-12); the caller resolves it through
//! `services::memory_time_zone::cached_memory_time_zone(host_fallback)`, which
//! with a cold cache answers the same host fallback — so those rows render
//! with the zone that resolution yields, pinning that the two agree.
//!
//! Generate the oracle (from the v4 checkout, or a pin):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   $N/npx tsx $V5W/harness/oracle/cases/extraction-pure-tier1.ts > /tmp/oracle-extraction-pure.ndjson
//! Run:
//!   QT_ORACLE_EXTRACTION_PURE=/tmp/oracle-extraction-pure.ndjson \
//!     cargo test -p quilltap-harness --test memory_tasks_equivalence

use std::collections::HashMap;
use std::path::PathBuf;

use quilltap_core::memory_extraction::parse::{
    parse_episode_drafts, parse_other_drafts_by_subject, parse_self_drafts, pass_cap, MemoryDraft,
};
use quilltap_core::memory_extraction::prompts::episode::build_episode_fold_messages;
use quilltap_core::memory_extraction::prompts::other::{
    build_other_fold_messages, build_other_turn_messages, OtherPromptInput, OtherSubjectInput,
};
use quilltap_core::memory_extraction::prompts::self_turn::{
    build_self_turn_messages, SelfTurnPromptInput,
};
use quilltap_core::memory_extraction::prompts::shared::{
    render_clock_block, render_clock_stamp, ExtractionClock, OrientingContext,
};
use quilltap_core::memory_extraction::window::{
    first_valid_iso, fold_pass_range, is_fold_eligible_message, message_id, messages_past_cursor,
    render_stretch, render_turn_context, slice_fold_window, stretch_window, turn_window,
    window_past_cursor, StretchMessage, TranscriptWindow,
};
use quilltap_core::memory_format::Pronouns;
use quilltap_core::model::completion::CompletionMessage;
use quilltap_core::services::memory_time_zone::{
    cached_memory_time_zone, reset_memory_time_zone_cache_for_tests,
};
use quilltap_core::services::speaker_names::SpeakerNames;
use quilltap_core::services::turn_transcript::{TurnCharacterSlice, TurnTranscript};
use serde_json::{json, Value};

fn corpus_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/extraction-pure-tier1.json")
}

fn s(v: &Value, k: &str) -> Option<String> {
    v.get(k).and_then(Value::as_str).map(String::from)
}

fn pronouns(v: Option<&Value>) -> Option<Pronouns> {
    let p = v.filter(|p| !p.is_null())?;
    Some(Pronouns {
        subject: s(p, "subject")?,
        object: s(p, "object")?,
        possessive: s(p, "possessive")?,
    })
}

fn slice(v: &Value) -> TurnCharacterSlice {
    TurnCharacterSlice {
        character_id: s(v, "characterId").unwrap_or_default(),
        character_name: s(v, "characterName").unwrap_or_default(),
        character_pronouns: pronouns(v.get("characterPronouns")),
        text: s(v, "text").unwrap_or_default(),
        contributing_message_ids: v["contributingMessageIds"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .map(String::from)
                    .collect()
            })
            .unwrap_or_default(),
        is_user_controlled: v.get("isUserControlled").and_then(Value::as_bool) == Some(true),
        last_message_created_at: s(v, "lastMessageCreatedAt"),
    }
}

fn transcript(v: &Value) -> TurnTranscript {
    TurnTranscript {
        turn_opener_message_id: s(v, "turnOpenerMessageId"),
        user_message: s(v, "userMessage"),
        user_character_id: s(v, "userCharacterId"),
        user_character_name: s(v, "userCharacterName"),
        user_character_pronouns: pronouns(v.get("userCharacterPronouns")),
        character_slices: v["characterSlices"]
            .as_array()
            .map(|a| a.iter().map(slice).collect())
            .unwrap_or_default(),
        prior_slices: v
            .get("priorSlices")
            .and_then(Value::as_array)
            .map(|a| a.iter().map(slice).collect()),
        latest_assistant_message_id: s(v, "latestAssistantMessageId"),
        turn_timestamp: s(v, "turnTimestamp"),
    }
}

fn clock(v: &Value) -> ExtractionClock {
    ExtractionClock {
        now_iso: s(v, "nowIso").unwrap_or_default(),
        timeline_mode: s(v, "timelineMode").unwrap_or_default(),
        time_zone: s(v, "timeZone").expect("a zoned clock"),
    }
}

fn orienting(v: Option<&Value>) -> Option<OrientingContext> {
    let o = v.filter(|o| !o.is_null())?;
    Some(OrientingContext {
        project_description: s(o, "projectDescription"),
        chat_context_summary: s(o, "chatContextSummary"),
    })
}

fn subjects(v: &Value) -> Vec<OtherSubjectInput> {
    v.as_array()
        .map(|a| {
            a.iter()
                .map(|x| OtherSubjectInput {
                    id: s(x, "id").unwrap_or_default(),
                    name: s(x, "name").unwrap_or_default(),
                    pronouns: pronouns(x.get("pronouns")),
                    is_user: x["isUser"].as_bool().unwrap_or(false),
                    canon_block: s(x, "canonBlock").unwrap_or_default(),
                })
                .collect()
        })
        .unwrap_or_default()
}

fn stretch(v: &Value) -> Vec<StretchMessage> {
    v.as_array()
        .map(|a| {
            a.iter()
                .map(|m| StretchMessage {
                    speaker: s(m, "speaker").unwrap_or_default(),
                    content: s(m, "content").unwrap_or_default(),
                    created_at: s(m, "createdAt"),
                })
                .collect()
        })
        .unwrap_or_default()
}

fn msgs_value(ms: Option<Vec<CompletionMessage>>) -> Value {
    match ms {
        None => Value::Null,
        Some(ms) => Value::from(
            ms.iter()
                .map(|m| json!({ "role": m.role.as_str(), "content": m.content }))
                .collect::<Vec<_>>(),
        ),
    }
}

/// `JSON.stringify` of a JS number: an integral value has no fraction.
fn num(f: f64) -> Value {
    if f.is_finite() && f.fract() == 0.0 {
        Value::from(f as i64)
    } else {
        Value::from(f)
    }
}

/// v4's `JSON.stringify(draft)` presence shape (undefined keys dropped).
fn draft_value(d: &MemoryDraft) -> Value {
    let mut o = serde_json::Map::new();
    if let Some(c) = &d.content {
        o.insert("content".into(), c.clone().into());
    }
    if let Some(c) = &d.summary {
        o.insert("summary".into(), c.clone().into());
    }
    o.insert("keywords".into(), d.keywords.clone().into());
    o.insert("importance".into(), num(d.importance));
    o.insert(
        "targeting".into(),
        json!({
            "temporal": d.targeting.temporal.as_str(),
            "scope": d.targeting.scope.as_str(),
            "context": d.targeting.context.as_str(),
        }),
    );
    o.insert("kind".into(), d.kind.as_str().into());
    if let Some(w) = &d.when {
        o.insert("when".into(), w.clone().into());
    }
    if let Some(n) = &d.narrative_time {
        o.insert("narrativeTime".into(), n.clone().into());
    }
    if let Some(e) = &d.entities {
        o.insert("entities".into(), e.clone().into());
    }
    Value::Object(o)
}

fn ids(ms: &[Value]) -> Value {
    Value::from(
        ms.iter()
            .map(|m| message_id(m).to_string())
            .collect::<Vec<_>>(),
    )
}

fn opt(v: Option<&str>) -> Value {
    v.map(Value::from).unwrap_or(Value::Null)
}

fn arr(v: &Value) -> Vec<Value> {
    v.as_array().cloned().unwrap_or_default()
}

#[test]
fn extraction_pure_leaves_match_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_EXTRACTION_PURE") else {
        eprintln!("QT_ORACLE_EXTRACTION_PURE not set — SKIP");
        return;
    };
    let raw = std::fs::read_to_string(&oracle_path).expect("read oracle NDJSON");
    let mut want: HashMap<(String, String), Value> = HashMap::new();
    for line in raw.lines().filter(|l| !l.trim().is_empty()) {
        let row: Value = serde_json::from_str(line).expect("oracle line JSON");
        let key = (
            row["section"].as_str().unwrap().to_string(),
            row["name"].as_str().unwrap().to_string(),
        );
        assert!(
            want.insert(key, row["out"].clone()).is_none(),
            "duplicate oracle row"
        );
    }
    let corpus: Value =
        serde_json::from_str(&std::fs::read_to_string(corpus_path()).expect("read corpus"))
            .expect("corpus JSON");
    let transcripts: HashMap<String, TurnTranscript> = corpus["transcripts"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(k, v)| (k.clone(), transcript(v)))
        .collect();

    let mut got: Vec<((String, String), Value)> = Vec::new();
    let mut push = |section: &str, name: &str, out: Value| {
        got.push(((section.to_string(), name.to_string()), out));
    };

    // ---- clocks ------------------------------------------------------------
    for c in arr(&corpus["clocks"]) {
        let name = c["name"].as_str().unwrap();
        let mut ck_json = c["clock"].clone();
        if let Some(env) = c.get("envZone").and_then(Value::as_str) {
            // v4's zoneless clock reads `getMemoryTimeZone()`; v5's caller
            // resolves the same value and passes it.
            reset_memory_time_zone_cache_for_tests();
            ck_json["timeZone"] = Value::from(cached_memory_time_zone(env));
        }
        let ck = clock(&ck_json);
        push(
            "clocks",
            name,
            json!({ "stamp": render_clock_stamp(&ck), "block": render_clock_block(Some(&ck)) }),
        );
    }
    push(
        "clocks",
        "no_clock",
        json!({ "stamp": null, "block": render_clock_block(None) }),
    );

    // ---- prompts -----------------------------------------------------------
    for p in arr(&corpus["prompts"]) {
        let name = p["name"].as_str().unwrap();
        let ck = p.get("clock").filter(|c| !c.is_null()).map(clock);
        let or = orienting(p.get("orienting"));
        let cap = p["cap"].as_u64().unwrap_or(0) as usize;
        let auto = p["inAutonomousRoom"].as_bool().unwrap_or(false);
        let out = match p["op"].as_str().unwrap() {
            "self" => build_self_turn_messages(&SelfTurnPromptInput {
                transcript: &transcripts[p["transcript"].as_str().unwrap()],
                holder_id: p["holderId"].as_str().unwrap(),
                canon_block: p["canonBlock"].as_str().unwrap(),
                cap,
                in_autonomous_room: auto,
                orienting: or.as_ref(),
                clock: ck.as_ref(),
            }),
            op @ ("other-turn" | "other-fold") => {
                let subs = subjects(&p["subjects"]);
                let obs_pronouns = pronouns(p["observer"].get("pronouns"));
                let input = OtherPromptInput {
                    observer_name: p["observer"]["name"].as_str().unwrap(),
                    observer_pronouns: obs_pronouns.as_ref(),
                    subjects: &subs,
                    cap,
                    in_autonomous_room: auto,
                    orienting: or.as_ref(),
                    clock: ck.as_ref(),
                };
                Some(if op == "other-turn" {
                    build_other_turn_messages(
                        &input,
                        &transcripts[p["transcript"].as_str().unwrap()],
                    )
                } else {
                    build_other_fold_messages(&input, &stretch(&p["stretch"]))
                })
            }
            _ => Some(build_episode_fold_messages(
                &stretch(&p["stretch"]),
                ck.as_ref().expect("episode clock"),
            )),
        };
        push("prompts", name, msgs_value(out));
    }

    // ---- passCap -----------------------------------------------------------
    for r in arr(&corpus["passCap"]) {
        let base = r["base"].as_f64().unwrap();
        let max = r["maxTokens"].as_f64();
        let name = format!(
            "base_{}_max_{}",
            num_label(&r["base"]),
            r.get("maxTokens")
                .filter(|m| !m.is_null())
                .map(num_label)
                .unwrap_or_else(|| "null".to_string())
        );
        push("passCap", &name, Value::from(pass_cap(base, max) as u64));
    }

    // ---- parse -------------------------------------------------------------
    for p in arr(&corpus["parse"]) {
        let name = p["name"].as_str().unwrap();
        let raw = p["raw"].as_str().unwrap();
        let cap = p["cap"].as_u64().unwrap() as usize;
        let out = match p["op"].as_str().unwrap() {
            "self" => Value::from(
                parse_self_drafts(raw, cap)
                    .iter()
                    .map(draft_value)
                    .collect::<Vec<_>>(),
            ),
            "episode" => Value::from(
                parse_episode_drafts(raw, cap)
                    .iter()
                    .map(draft_value)
                    .collect::<Vec<_>>(),
            ),
            _ => {
                let ids: Vec<String> = arr(&p["subjectIds"])
                    .iter()
                    .filter_map(Value::as_str)
                    .map(String::from)
                    .collect();
                Value::from(
                    parse_other_drafts_by_subject(raw, &ids, cap)
                        .into_iter()
                        .map(|(id, ds)| json!([id, ds.iter().map(draft_value).collect::<Vec<_>>()]))
                        .collect::<Vec<_>>(),
                )
            }
        };
        push("parse", name, out);
    }

    // ---- window ------------------------------------------------------------
    let no_names = SpeakerNames::default();
    for w in arr(&corpus["window"]) {
        let name = w["name"].as_str().unwrap();
        let cursor = w.get("cursor").and_then(Value::as_str);
        let out = match w["op"].as_str().unwrap() {
            "firstValidIso" => {
                let isos: Vec<Option<String>> = arr(&w["isos"])
                    .iter()
                    .map(|v| v.as_str().map(String::from))
                    .collect();
                let refs: Vec<Option<&str>> = isos.iter().map(|o| o.as_deref()).collect();
                opt(first_valid_iso(&refs).as_deref())
            }
            "eligible" => Value::from(
                arr(&w["messages"])
                    .iter()
                    .map(is_fold_eligible_message)
                    .collect::<Vec<_>>(),
            ),
            "pastCursor" => ids(&messages_past_cursor(&arr(&w["messages"]), cursor)),
            "windowPastCursor" => {
                let r = window_past_cursor(&arr(&w["window"]), &arr(&w["all"]), cursor);
                json!({ "window": ids(&r.window), "applied": r.applied })
            }
            "slice" => Value::from(
                slice_fold_window(
                    &arr(&w["eligible"]),
                    w["max"].as_f64().unwrap(),
                    w.get("final").and_then(Value::as_str),
                )
                .iter()
                .map(|s| {
                    json!({
                        "messages": ids(&s.messages),
                        "coverThroughMessageId": opt(s.cover_through_message_id.as_deref()),
                    })
                })
                .collect::<Vec<_>>(),
            ),
            "range" => {
                let r = fold_pass_range(&arr(&w["window"]), &arr(&w["all"]), cursor);
                json!({ "messages": ids(&r.messages), "applied": r.applied, "gap": r.gap })
            }
            "stretchWindow" => {
                let sw = stretch_window(
                    arr(&w["messages"]),
                    &no_names,
                    w.get("cover").and_then(Value::as_str).map(String::from),
                    w["nowMs"].as_f64().unwrap(),
                );
                json!({
                    "messages": ids(&sw.messages),
                    "rendered": sw.rendered.iter().map(|r| json!({
                        "speaker": r.speaker, "content": r.content, "createdAt": opt(r.created_at.as_deref()),
                    })).collect::<Vec<_>>(),
                    "nowIso": sw.now_iso,
                    "startIso": sw.start_iso,
                    "coverThroughMessageId": opt(sw.cover_through_message_id.as_deref()),
                })
            }
            "turnWindow" => {
                let t = transcripts[w["transcript"].as_str().unwrap()].clone();
                match turn_window(t, w["nowMs"].as_f64().unwrap()) {
                    TranscriptWindow::Turn { now_iso, .. } => json!({ "nowIso": now_iso }),
                    TranscriptWindow::Stretch(_) => unreachable!(),
                }
            }
            other => panic!("unknown window op {other}"),
        };
        push("window", name, out);
    }

    // ---- render ------------------------------------------------------------
    for r in arr(&corpus["render"]) {
        let name = r["name"].as_str().unwrap();
        let out = if r["op"] == "stretch" {
            render_stretch(&stretch(&r["stretch"]))
        } else {
            render_turn_context(&transcripts[r["transcript"].as_str().unwrap()])
        };
        push("render", name, Value::from(out));
    }

    assert_eq!(
        got.len(),
        want.len(),
        "row-count mismatch (Rust {} vs oracle {}) — regenerate the oracle",
        got.len(),
        want.len()
    );
    let mut failures: Vec<String> = Vec::new();
    for (key, out) in &got {
        match want.get(key) {
            None => failures.push(format!("{key:?}: missing from oracle")),
            Some(w) if w != out => {
                failures.push(format!("{key:?} diverges\n got: {out:#}\nwant: {w:#}"))
            }
            _ => {}
        }
    }
    assert!(
        failures.is_empty(),
        "{} divergence(s):\n{}",
        failures.len(),
        failures.join("\n")
    );
    println!(
        "memory_tasks (extraction pure leaves): {} rows OK",
        got.len()
    );
}

/// v4's template-literal rendering of a corpus number (`${7.9}`, `${-1}`).
fn num_label(v: &Value) -> String {
    match v.as_f64() {
        Some(f) if f.fract() == 0.0 => format!("{}", f as i64),
        Some(f) => format!("{f}"),
        None => "undefined".to_string(),
    }
}
