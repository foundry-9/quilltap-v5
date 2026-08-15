//! Tier-3 differential: the Ollama retry-without-`think` salvage (P4.D78, v4
//! `d9c5a1c7`), on BOTH the streaming and the non-streaming path.
//!
//! The oracle (`harness/oracle/cases/ollama-think-retry-tier3.test.ts`) drives
//! v4's REAL `OllamaProvider.streamMessage` / `.sendMessage` with ONLY
//! `global.fetch` mocked below it, so v4's own `!response.ok` →
//! `isThinkRejection` → `delete requestBody.think` → re-send path runs end to
//! end. It records, per arm, the **attempt fingerprint** — how many requests v4
//! issued and whether each body still carried `think` (and its value) — plus the
//! content it recovered or the failure it raised.
//!
//! This test drives the REAL v5 compositions (`WireStreamingProvider::
//! stream_message` and `execute_completion`) over a scripted transport serving
//! the SAME outcomes, and diffs that fingerprint. A byte-level proof of the
//! retry body lives in `model::ollama_think_retry`'s unit tests (key order
//! preserved, exactly one key removed) and in the two compositions' quartets;
//! this arm proves turn-level parity against v4's REAL salvage.
//!
//! **One documented, pre-existing divergence: the error TEXT.** v4 raises
//! `Ollama API error: <status> <body>`; v5's one reqwest transport renders every
//! provider's non-2xx as `HTTP <status>: <body>` (the nine-SDK collapse recorded
//! in `model::transport`'s header). So the comparison asserts v4 and v5 agree on
//! *whether* the call failed and that v5's message carries the same status and
//! body text — not the sentence, which no lane in this port has ever matched for
//! Ollama.
//!
//! Regenerate the oracle (Node 24; the /tmp mirror is because jest ignores
//! `.claude/` paths):
//!   rm -rf /tmp/qt-ollama-think-retry-oracle && mkdir -p /tmp/qt-ollama-think-retry-oracle/cases
//!   cp ~/source/quilltap-v5/harness/oracle/cases/ollama-think-retry-tier3.test.ts /tmp/qt-ollama-think-retry-oracle/cases/
//!   cd ~/source/quilltap-server
//!   QT_ORACLE_OUT=/tmp/oracle-ollama-think-retry.ndjson ~/.nvm/versions/node/v24.13.1/bin/npx jest --silent --watchman=false --roots "$PWD" --roots /tmp/qt-ollama-think-retry-oracle/cases -- "ollama-think-retry-tier3\.test\.ts$"
//! Run:
//!   QT_ORACLE_OLLAMA_THINK_RETRY=/tmp/oracle-ollama-think-retry.ndjson cargo test -p quilltap-harness --test ollama_think_retry_tier3_equivalence

use std::collections::VecDeque;
use std::sync::Mutex;

use quilltap_core::model::completion::{CompletionMessage, CompletionParams};
use quilltap_core::model::completion_provider::execute_completion;
use quilltap_core::model::stream::{StreamMessage, StreamParams, StreamingCompletionProvider};
use quilltap_core::model::streaming_provider::{SingleKey, WireStreamingProvider};
use quilltap_core::model::transport::{
    BoxFuture, ProviderTransport, StreamBytes, TransportError, TransportPolicy, TransportRequest,
    TransportResponse,
};
use serde::Deserialize;

#[derive(Deserialize, Debug, PartialEq)]
struct Attempt {
    #[serde(rename = "hasThink")]
    has_think: bool,
    think: serde_json::Value,
}

#[derive(Deserialize)]
struct Arm {
    arm: String,
    method: String,
    attempts: Vec<Attempt>,
    content: Option<String>,
    error: Option<String>,
}

/// The canned outcome for one transport attempt.
#[derive(Clone)]
enum Outcome {
    Ok,
    Fail(u16, String),
}

const THINK_ERROR: &str = r#"{"error":"\"qwen3:8b\" does not support disabling thinking"}"#;
const OTHER_ERROR: &str = r#"{"error":"model \"nope\" not found"}"#;

const OK_STREAM: &str = concat!(
    r#"{"model":"qwen3:8b","message":{"role":"assistant","content":"ok"},"done":false}"#,
    "\n",
    r#"{"model":"qwen3:8b","message":{"role":"assistant","content":""},"done":true,"prompt_eval_count":1,"eval_count":1}"#,
    "\n"
);
const OK_SEND: &str = r#"{"model":"qwen3:8b","message":{"role":"assistant","content":"ok"},"done":true,"prompt_eval_count":1,"eval_count":1}"#;

/// A transport with a queued outcome per attempt (the LAST outcome repeats, as
/// the oracle's mock does), recording every request body it saw.
struct ScriptedTransport {
    outcomes: Mutex<VecDeque<Outcome>>,
    last: Mutex<Outcome>,
    seen: Mutex<Vec<Vec<u8>>>,
}

impl ScriptedTransport {
    fn new(outcomes: Vec<Outcome>) -> Self {
        let last = outcomes.last().cloned().expect("at least one outcome");
        Self {
            outcomes: Mutex::new(outcomes.into()),
            last: Mutex::new(last),
            seen: Mutex::new(Vec::new()),
        }
    }

    fn next(&self, request: &TransportRequest) -> Outcome {
        self.seen.lock().unwrap().push(request.body.clone());
        self.outcomes
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or_else(|| self.last.lock().unwrap().clone())
    }

    /// The (has_think, think) fingerprint of every body the transport received.
    fn attempts(&self) -> Vec<Attempt> {
        self.seen
            .lock()
            .unwrap()
            .iter()
            .map(|b| {
                let v: serde_json::Value = serde_json::from_slice(b).expect("body is JSON");
                Attempt {
                    has_think: v.get("think").is_some(),
                    // The oracle records `body.think ?? null`.
                    think: v.get("think").cloned().unwrap_or(serde_json::Value::Null),
                }
            })
            .collect()
    }
}

impl ProviderTransport for ScriptedTransport {
    fn execute<'a>(
        &'a self,
        request: &'a TransportRequest,
        _policy: &'a TransportPolicy,
    ) -> BoxFuture<'a, Result<TransportResponse, TransportError>> {
        let outcome = self.next(request);
        Box::pin(async move {
            match outcome {
                Outcome::Ok => Ok(TransportResponse {
                    status: 200,
                    body: OK_SEND.as_bytes().to_vec(),
                }),
                Outcome::Fail(status, text) => Err(TransportError {
                    message: format!("HTTP {status}: {text}"),
                    status: Some(status),
                }),
            }
        })
    }

    fn execute_stream<'a>(
        &'a self,
        request: &'a TransportRequest,
        _policy: &'a TransportPolicy,
    ) -> BoxFuture<'a, Result<tokio::sync::mpsc::Receiver<StreamBytes>, TransportError>> {
        let outcome = self.next(request);
        Box::pin(async move {
            match outcome {
                Outcome::Ok => {
                    let (tx, rx) = tokio::sync::mpsc::channel(2);
                    let _ = tx.send(Ok(OK_STREAM.as_bytes().to_vec())).await;
                    Ok(rx)
                }
                Outcome::Fail(status, text) => Err(TransportError {
                    message: format!("HTTP {status}: {text}"),
                    status: Some(status),
                }),
            }
        })
    }
}

fn outcomes_for(arm: &str) -> Vec<Outcome> {
    match arm {
        "rejected-then-succeeded" => vec![Outcome::Fail(400, THINK_ERROR.into()), Outcome::Ok],
        "rejected-twice" => vec![
            Outcome::Fail(400, THINK_ERROR.into()),
            Outcome::Fail(500, "still thinking about it".into()),
        ],
        "non-think-error" => vec![Outcome::Fail(404, OTHER_ERROR.into()), Outcome::Ok],
        other => panic!("unknown oracle arm {other}"),
    }
}

fn stream_params() -> StreamParams {
    StreamParams {
        messages: vec![StreamMessage::user("hi")],
        model: "qwen3:8b".into(),
        temperature: Some(0.7),
        max_tokens: Some(1024),
        top_p: None,
        tools: None,
        web_search_enabled: false,
        profile_parameters: None,
        cache_key: None,
        previous_response_id: None,
        stop: Vec::new(),
    }
}

fn completion_params() -> CompletionParams {
    CompletionParams {
        messages: vec![CompletionMessage::user("hi")],
        model: "qwen3:8b".into(),
        temperature: Some(0.7),
        max_tokens: 1024,
        strict_max_tokens: false,
        cache_key: None,
        profile_parameters: None,
        attachments: Vec::new(),
        request_timeout_ms: None,
    }
}

/// v4's `Ollama API error: <status> <text>` vs v5's `HTTP <status>: <text>` —
/// the pre-existing transport collapse. Assert the STATUS and the body text
/// survive, so the check is not vacuous.
fn assert_same_failure(arm: &str, method: &str, v4: &str, v5: &str) {
    let status = v4
        .strip_prefix("Ollama API error: ")
        .and_then(|rest| rest.split_once(' '))
        .map(|(s, _)| s.to_string())
        .unwrap_or_else(|| panic!("{arm}/{method}: unrecognized v4 error shape: {v4}"));
    let body = v4
        .strip_prefix(&format!("Ollama API error: {status} "))
        .unwrap();
    assert_eq!(
        v5,
        format!("HTTP {status}: {body}"),
        "{arm}/{method}: v5's error must carry v4's status and body text"
    );
}

#[test]
fn ollama_think_retry_tier3_matches_oracle() {
    let path = match std::env::var("QT_ORACLE_OLLAMA_THINK_RETRY") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_ORACLE_OLLAMA_THINK_RETRY to the oracle NDJSON (see test header)."
            );
            return;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));
    let rows: Vec<Arm> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("oracle row parses"))
        .collect();
    assert_eq!(
        rows.len(),
        6,
        "expected 3 arms × 2 methods; regenerate the oracle"
    );

    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let mut checked = 0usize;
    let mut retried = 0usize;

    for row in &rows {
        let transport = ScriptedTransport::new(outcomes_for(&row.arm));
        let (content, error): (Option<String>, Option<String>) = match row.method.as_str() {
            "stream" => {
                let provider = WireStreamingProvider::new(
                    transport,
                    SingleKey(String::new()),
                    TransportPolicy::default(),
                    "Quilltap/test".to_string(),
                );
                let items = rt.block_on(async {
                    let mut rx = provider
                        .stream_message("OLLAMA", None, &stream_params())
                        .await;
                    let mut out = Vec::new();
                    while let Some(item) = rx.recv().await {
                        out.push(item);
                    }
                    out
                });
                let err = items.iter().find_map(|i| i.as_ref().err().cloned());
                let got = match err {
                    Some(e) => (None, Some(e.message)),
                    None => (
                        Some(
                            items
                                .iter()
                                .map(|i| i.as_ref().unwrap().content.as_str())
                                .collect::<String>(),
                        ),
                        None,
                    ),
                };
                let attempts = provider.transport_ref().attempts();
                assert_eq!(
                    attempts, row.attempts,
                    "{}/{}: attempt fingerprint",
                    row.arm, row.method
                );
                retried += usize::from(attempts.len() > 1);
                got
            }
            "send" => {
                let params = completion_params();
                let result = rt.block_on(execute_completion(
                    &transport,
                    "OLLAMA",
                    None,
                    "",
                    &params,
                    &TransportPolicy::default(),
                    "Quilltap/test",
                    None,
                ));
                let attempts = transport.attempts();
                assert_eq!(
                    attempts, row.attempts,
                    "{}/{}: attempt fingerprint",
                    row.arm, row.method
                );
                retried += usize::from(attempts.len() > 1);
                match result {
                    Ok(r) => (Some(r.content), None),
                    Err(e) => (None, Some(e.message)),
                }
            }
            other => panic!("unknown oracle method {other}"),
        };

        assert_eq!(
            content, row.content,
            "{}/{}: recovered content",
            row.arm, row.method
        );
        match (&row.error, &error) {
            (Some(v4), Some(v5)) => assert_same_failure(&row.arm, &row.method, v4, v5),
            (None, None) => {}
            (Some(v4), None) => panic!(
                "{}/{}: v4 failed ({v4}) but v5 did not",
                row.arm, row.method
            ),
            (None, Some(v5)) => panic!(
                "{}/{}: v5 failed ({v5}) but v4 did not",
                row.arm, row.method
            ),
        }
        checked += 1;
    }

    // Shape, not a hand count: an oracle that lost every retrying arm would
    // otherwise leave this family green with the salvage untested.
    assert!(
        retried >= 4,
        "only {retried} arm(s) issued a second attempt — the retrying arms are \
         missing from the oracle, or the salvage stopped firing"
    );
    eprintln!("OK: ollama think-retry tier 3 — {checked} arm(s) matched v4 ({retried} retried).");
}
