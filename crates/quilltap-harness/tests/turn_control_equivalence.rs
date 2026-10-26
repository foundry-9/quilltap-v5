//! Tier-1 differential (P4.D335): the interrupts' registry + pure half —
//! `quilltap_core::services::turn_control`, `quilltap_core::yield_signal`,
//! `skip_signal::matches_sentinel_line` and the watchdog's stop race vs v4's
//! REAL `lib/chat/turn-control.ts`, `turn-manager/yield-signal.ts`,
//! `skip-signal.ts`'s `matchesSentinelLine` and `stream-watchdog.ts`'s
//! `abortSignal` (`2b5e8ee24`, at `322a5488e`).
//!
//! The registry rows are v4's ten `turn-control.test.ts` scenarios (+ two
//! streams on one chat) replayed step for step on a current-thread runtime, so
//! the observation order AND the `[TurnControl]` line order are comparable;
//! every line is compared by (level, message, fields) with the chat id masked
//! to `<chat>` (v5 tests use unique ids — the registry is one process static).
//! The watchdog rows are event-driven (no clock), incl. the measured
//! `stop-vs-buffered-chunk` (v4 abandons a buffered chunk once stopped).
//!
//! Generate the oracle output:
//!   cd ~/source/quilltap-server
//!   npx tsx ~/source/quilltap-v5/harness/oracle/cases/turn-control.ts \
//!     > /tmp/oracle-turn-control.ndjson
//! Run:
//!   QT_ORACLE_TURN_CONTROL=/tmp/oracle-turn-control.ndjson \
//!     cargo test -p quilltap-harness --test turn_control_equivalence

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use quilltap_core::model::stream::{StreamChunk, StreamChunkResult};
use quilltap_core::model::stream_watchdog::{
    watch_stream_with_stop, StallBudgets, StallWatchdogContext,
};
use quilltap_core::services::turn_control::{
    has_active_turn, register_active_turn, set_hand_raised, stop_active_turns, TurnControlHandle,
};
use quilltap_core::skip_signal::matches_sentinel_line;
use quilltap_core::test_support::global_capture;
use quilltap_core::yield_signal::{
    build_turn_yield_instruction, detect_yield_sentinel, TURN_YIELD_SYSTEM_KIND, YIELD_SENTINEL,
};
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::sync::mpsc;

#[derive(Deserialize, Debug, Clone, PartialEq)]
struct LogLine {
    level: String,
    message: String,
    fields: serde_json::Map<String, Value>,
}

#[derive(Deserialize)]
#[serde(tag = "kind")]
enum OracleRow {
    #[serde(rename = "registry")]
    Registry {
        id: String,
        obs: Vec<Value>,
        logs: Vec<LogLine>,
    },
    #[serde(rename = "detect")]
    Detect {
        id: String,
        response: String,
        #[serde(rename = "characterName", default)]
        character_name: Option<String>,
        #[serde(default)]
        aliases: Option<Vec<String>>,
        out: Value,
    },
    #[serde(rename = "note")]
    Note {
        id: String,
        #[serde(rename = "operatorName")]
        operator_name: String,
        out: String,
    },
    #[serde(rename = "constants")]
    Constants { id: String, out: Value },
    #[serde(rename = "sentinelLine")]
    SentinelLine {
        id: String,
        line: String,
        phrase: String,
        out: bool,
    },
    #[serde(rename = "watchdog")]
    Watchdog {
        id: String,
        seen: Vec<String>,
        logs: Vec<LogLine>,
    },
}

fn snake(k: &str) -> String {
    let mut out = String::new();
    for c in k.chars() {
        if c.is_ascii_uppercase() {
            out.push('_');
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// v4's `{level, message, fields}` in the capture rig's line shape (target
/// dropped): `"<LEVEL> <message> k=v …"`, keys snake_cased in v4's order.
fn v4_line(l: &LogLine) -> String {
    let mut s = format!("{} {}", l.level.to_ascii_uppercase(), l.message);
    for (k, v) in &l.fields {
        let v = match v {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        s.push_str(&format!(" {}={v}", snake(k)));
    }
    s
}

/// A captured v5 line with its target dropped, the chat id and `elapsed_ms`
/// masked, restricted to the tag the row compares.
fn v5_lines(lines: &[String], tag: &str, chat: Option<&str>) -> Vec<String> {
    let elapsed = regex::Regex::new(r"elapsed_ms=\d+").unwrap();
    lines
        .iter()
        .filter(|l| l.contains(tag))
        .map(|l| {
            let mut parts = l.splitn(3, ' ');
            let level = parts.next().unwrap_or_default();
            let _target = parts.next();
            let rest = parts.next().unwrap_or_default();
            let mut line = format!("{level} {rest}");
            if let Some(chat) = chat {
                line = line.replace(chat, "<chat>");
            }
            elapsed.replace_all(&line, "elapsed_ms=<ms>").into_owned()
        })
        .collect()
}

fn unique(id: &str) -> String {
    static N: AtomicU64 = AtomicU64::new(0);
    format!(
        "tc-eq-{id}-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    )
}

fn rt() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

/// Replay one v4 registry scenario on this thread; returns (obs, the chat id).
async fn registry_scenario(id: &str) -> (Vec<Value>, String) {
    let chat = unique(id);
    let c = chat.as_str();
    let mut obs: Vec<Value> = Vec::new();
    match id {
        "no-handle" => {
            let none: Option<&TurnControlHandle> = None;
            obs.push(json!(true));
            obs.push(json!(none.is_some_and(|h| h.is_stopped())));
            obs.push(json!(none.is_some_and(|h| h.take_raised_hand())));
        }
        "lifetime" => {
            let reg = register_active_turn(c);
            obs.push(json!(has_active_turn(c)));
            obs.push(json!(reg.handle().stop_signal().stopped()));
            drop(reg);
            obs.push(json!("done"));
            obs.push(json!(has_active_turn(c)));
        }
        "throws" => {
            let c2 = chat.clone();
            let r = std::panic::catch_unwind(move || {
                let _reg = register_active_turn(&c2);
                std::panic::panic_any("boom");
            });
            let msg = r
                .err()
                .and_then(|e| e.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_default();
            obs.push(json!(msg));
            obs.push(json!(has_active_turn(c)));
        }
        "hand-once" => {
            let reg = register_active_turn(c);
            obs.push(json!(set_hand_raised(c, true)));
            let h = reg.handle();
            obs.push(json!(h.take_raised_hand()));
            obs.push(json!(h.take_raised_hand()));
            drop(reg);
        }
        "lower" => {
            let reg = register_active_turn(c);
            obs.push(json!(set_hand_raised(c, true)));
            obs.push(json!(set_hand_raised(c, false)));
            obs.push(json!(reg.handle().take_raised_hand()));
            drop(reg);
        }
        "idle" => {
            obs.push(json!(set_hand_raised(c, true)));
        }
        "stop-settle" => {
            let reg = register_active_turn(c);
            let h = reg.handle();
            let worker = tokio::spawn(async move {
                h.stop_signal().wait().await;
                let seen = json!([h.is_stopped(), h.is_stopped(), h.take_raised_hand()]);
                drop(reg);
                seen
            });
            tokio::task::yield_now().await;
            obs.push(json!(set_hand_raised(c, true)));
            obs.push(serde_json::to_value(stop_active_turns(c, None).await).unwrap());
            obs.push(worker.await.unwrap());
        }
        "fresh" => {
            let first = register_active_turn(c);
            let stopping = {
                let chat = chat.clone();
                tokio::spawn(async move {
                    stop_active_turns(&chat, Some(Duration::from_millis(1000))).await
                })
            };
            tokio::task::yield_now().await;
            let second = register_active_turn(c);
            let second_stopped = second.handle().is_stopped();
            drop(second);
            drop(first);
            obs.push(serde_json::to_value(stopping.await.unwrap()).unwrap());
            obs.push(json!(second_stopped));
        }
        "give-up" => {
            let reg = register_active_turn(c);
            obs.push(
                serde_json::to_value(stop_active_turns(c, Some(Duration::from_millis(20))).await)
                    .unwrap(),
            );
            drop(reg);
        }
        "noop-stop" => {
            obs.push(serde_json::to_value(stop_active_turns(c, None).await).unwrap());
        }
        "two-streams" => {
            let a = register_active_turn(c);
            let b = register_active_turn(c);
            obs.push(json!(has_active_turn(c)));
            obs.push(json!(set_hand_raised(c, true)));
            obs.push(json!(a.handle().take_raised_hand()));
            drop(a);
            obs.push(json!(b.handle().take_raised_hand()));
            drop(b);
            obs.push(json!(has_active_turn(c)));
        }
        other => panic!("no Rust replay for registry scenario {other}"),
    }
    (obs, chat)
}

fn ctx() -> StallWatchdogContext<'static> {
    StallWatchdogContext::streaming_service("DEEPSEEK", "deepseek-v4-flash").with_ids(
        Some("u1"),
        Some("c1"),
        None,
        Some("m1"),
    )
}

const SLOW: StallBudgets = StallBudgets {
    first_chunk_ms: 5000,
    idle_ms: 5000,
};

/// v4's `silentAfter(values)`: the sender is HELD so the channel never closes.
fn silent_after(
    values: &[&str],
) -> (
    mpsc::Receiver<StreamChunkResult>,
    mpsc::Sender<StreamChunkResult>,
) {
    let (tx, rx) = mpsc::channel(values.len().max(1));
    for v in values {
        tx.try_send(Ok(StreamChunk::content(*v))).unwrap();
    }
    (rx, tx)
}

async fn watchdog_scenario(id: &str) -> Vec<String> {
    let chat = unique(id);
    let reg = register_active_turn(&chat);
    let sig = reg.handle().stop_signal();
    let stop = || stop_active_turns(&chat, Some(Duration::ZERO));
    let mut seen = Vec::new();
    match id {
        "mid-stream" | "before-first-chunk" => {
            let values: &[&str] = if id == "mid-stream" { &["a", "b"] } else { &[] };
            let (rx, _held) = silent_after(values);
            let mut s = watch_stream_with_stop(rx, SLOW, ctx(), Some(&sig));
            for _ in 0..values.len() {
                if let Some(Ok(c)) = s.recv().await {
                    seen.push(c.content);
                }
            }
            let next = {
                let waiting = s.recv();
                tokio::pin!(waiting);
                assert!(
                    tokio::time::timeout(Duration::from_millis(15), &mut waiting)
                        .await
                        .is_err(),
                    "{id}: the source must still be silent"
                );
                stop().await;
                waiting.await
            };
            assert!(next.is_none(), "{id}: a stop is a quiet end");
        }
        "already-stopped" => {
            stop().await;
            let (rx, _held) = silent_after(&["a"]);
            let mut s = watch_stream_with_stop(rx, SLOW, ctx(), Some(&sig));
            while let Some(Ok(c)) = s.recv().await {
                seen.push(c.content);
            }
        }
        "stop-vs-buffered-chunk" => {
            let (rx, _held) = silent_after(&["a", "b", "c"]);
            let mut s = watch_stream_with_stop(rx, SLOW, ctx(), Some(&sig));
            while let Some(Ok(c)) = s.recv().await {
                let first = c.content == "a";
                seen.push(c.content);
                if first {
                    stop().await;
                }
            }
        }
        "never-fires" => {
            let (tx, rx) = mpsc::channel(2);
            tx.try_send(Ok(StreamChunk::content("a"))).unwrap();
            tx.try_send(Ok(StreamChunk::content("b"))).unwrap();
            drop(tx);
            let mut s = watch_stream_with_stop(rx, SLOW, ctx(), Some(&sig));
            while let Some(Ok(c)) = s.recv().await {
                seen.push(c.content);
            }
        }
        other => panic!("no Rust replay for watchdog scenario {other}"),
    }
    drop(reg);
    seen
}

#[test]
fn turn_control_equivalence() {
    global_capture::install();
    let Ok(path) = std::env::var("QT_ORACLE_TURN_CONTROL") else {
        eprintln!("QT_ORACLE_TURN_CONTROL not set; skipping");
        return;
    };
    let text = std::fs::read_to_string(&path).expect("read the oracle NDJSON");
    let mut counts = std::collections::BTreeMap::<&str, usize>::new();
    let mut failures = Vec::new();
    let rt = rt();

    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: OracleRow = serde_json::from_str(line).expect("parse an oracle row");
        match row {
            OracleRow::Registry { id, obs, logs } => {
                *counts.entry("registry").or_default() += 1;
                let ((got, chat), lines) =
                    rt.block_on(global_capture::capture_async(registry_scenario(&id)));
                if got != obs {
                    failures.push(format!("registry {id}: obs v5 {got:?} != v4 {obs:?}"));
                }
                let want: Vec<String> = logs.iter().map(v4_line).collect();
                let have = v5_lines(&lines, "[TurnControl]", Some(&chat));
                if want != have {
                    failures.push(format!(
                        "registry {id}: logs\n  v5 {have:#?}\n  v4 {want:#?}"
                    ));
                }
            }
            OracleRow::Detect {
                id,
                response,
                character_name,
                aliases,
                out,
            } => {
                *counts.entry("detect").or_default() += 1;
                let got = serde_json::to_value(detect_yield_sentinel(
                    &response,
                    character_name.as_deref(),
                    aliases.as_deref(),
                ))
                .unwrap();
                if serde_json::to_string(&got).unwrap() != serde_json::to_string(&out).unwrap() {
                    failures.push(format!("detect {id}: v5 {got} != v4 {out}"));
                }
            }
            OracleRow::Note {
                id,
                operator_name,
                out,
            } => {
                *counts.entry("note").or_default() += 1;
                let got = build_turn_yield_instruction(&operator_name);
                if got != out {
                    failures.push(format!("note {id}: v5 {got:?} != v4 {out:?}"));
                }
            }
            OracleRow::Constants { id, out } => {
                *counts.entry("constants").or_default() += 1;
                let got = json!({
                    "YIELD_SENTINEL": YIELD_SENTINEL,
                    "TURN_YIELD_SYSTEM_KIND": TURN_YIELD_SYSTEM_KIND,
                });
                if got != out {
                    failures.push(format!("constants {id}: v5 {got} != v4 {out}"));
                }
            }
            OracleRow::SentinelLine {
                id,
                line,
                phrase,
                out,
            } => {
                *counts.entry("sentinelLine").or_default() += 1;
                let got = matches_sentinel_line(&line, &phrase);
                if got != out {
                    failures.push(format!(
                        "sentinelLine {id} ({line:?}, {phrase:?}): v5 {got} != v4 {out}"
                    ));
                }
            }
            OracleRow::Watchdog { id, seen, logs } => {
                *counts.entry("watchdog").or_default() += 1;
                let (got, lines) =
                    rt.block_on(global_capture::capture_async(watchdog_scenario(&id)));
                if got != seen {
                    failures.push(format!("watchdog {id}: seen v5 {got:?} != v4 {seen:?}"));
                }
                let want: Vec<String> = logs.iter().map(v4_line).collect();
                let have = v5_lines(&lines, "[LLMStream]", None);
                if want != have {
                    failures.push(format!(
                        "watchdog {id}: logs\n  v5 {have:#?}\n  v4 {want:#?}"
                    ));
                }
            }
        }
    }

    eprintln!("turn_control_equivalence row counts: {counts:?}");
    // Vacuity guards: every family arrived, at the sizes the case writes.
    assert_eq!(counts.get("registry"), Some(&11), "{counts:?}");
    assert_eq!(counts.get("watchdog"), Some(&5), "{counts:?}");
    assert!(
        counts.get("detect").copied().unwrap_or(0) >= 40,
        "{counts:?}"
    );
    assert_eq!(counts.get("sentinelLine"), Some(&40), "{counts:?}");
    assert_eq!(counts.get("note"), Some(&4), "{counts:?}");
    assert!(
        failures.is_empty(),
        "{} divergence(s):\n{}",
        failures.len(),
        failures.join("\n")
    );
}
