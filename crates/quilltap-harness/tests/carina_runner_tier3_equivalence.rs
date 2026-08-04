//! Tier-3 differential test: the **Carina markup runner**
//! (`quilltap_core::services::carina_runner` — v4 `runCarinaMarkupQuery`,
//! lib/services/carina/markup-runner.ts).
//!
//! The isolated query engine (`runCarinaQuery`) is an injected seam (its whole
//! dependency graph — the wave-4 tool subsystem, `findCharactersByName`,
//! `searchMemoriesSemantic`, the commonplace writer, the Brahma console,
//! `enqueueCarinaMemoryExtraction` — is unported; see the module docs). So both
//! sides pin the SAME seam behavior from the corpus `outcome`: on the oracle side
//! `runCarinaQuery` is jest-mocked to post through v4's REAL `postCarinaResponse`
//! (so `chat_messages` fills byte-for-byte) and return a fixed `CarinaResult`;
//! here the Rust `RunCarinaQuery` seam posts through the ported
//! `build_carina_message` + `add_message` and returns the same result. The
//! Prospero error post is a recorder on both sides.
//!
//! Diffed per call:
//!   1. the runner-observable **trace** — the ordered callback fires
//!      (`onConsulting` / `onPublicAnswer`, the latter NOT for whispers) + seam
//!      invocations (query args, Prospero args, seam-throw-swallowed) — with the
//!      minted posted-message id tokenized to `<msg>`;
//!   2. the `chat_messages` dump (ordered by `content`), with the minted `id`
//!      tokenized in natural (content) order and `createdAt` placeholdered to
//!      `<ts>` (the established minted-values remap form).
//!
//! Generate the fixture + oracle output (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   TMPO=/tmp/qt-carina-runner-oracle
//!   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
//!   cp "$V5W/harness/oracle/cases/carina-runner-tier3.test.ts" "$TMPO/cases/"
//!   cp "$V5W/harness/oracle/fixtures/carina-runner-tier3.json" "$TMPO/fixtures/"
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-carina-runner.db \
//!     $N/npx tsx $V5W/harness/oracle/fixtures/build-carina-runner-fixture.ts
//!   QT_FIXTURE_CARINA=/tmp/qt-carina-runner.db \
//!   QT_ORACLE_OUT=/tmp/oracle-carina-runner.ndjson \
//!     $N/npx jest --silent --watchman=false --roots "$PWD" --roots "$TMPO/cases" -- carina-runner-tier3
//! Run:
//!   QT_ORACLE_CARINA=/tmp/oracle-carina-runner.ndjson \
//!   QT_FIXTURE_CARINA=/tmp/qt-carina-runner.db \
//!     cargo test -p quilltap-harness --test carina_runner_tier3_equivalence

use std::collections::HashMap;

use quilltap_core::db::dump_table_json_conn;
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::services::carina_runner::{
    build_carina_message, run_carina_markup_query, CarinaError, CarinaErrorKind,
    CarinaMarkupCallbacks, CarinaMarkupLogLabels, CarinaMarkupOptions, CarinaResult,
    CarinaRunError, CarinaTrace, PostCarinaResponseParams, PostProsperoCarinaError,
    PostedCarinaMessage, ProsperoCarinaErrorArgs, RunCarinaQuery, RunCarinaQueryOptions,
};
use serde::Deserialize;
use serde_json::{json, Value};

// ---------------------------------------------------------------------------
// Spec structures (harness/oracle/fixtures/carina-runner-tier3.json).
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CarinaErrorSpec {
    kind: String,
    #[serde(default)]
    character_name: Option<String>,
    #[serde(default)]
    detail: Option<String>,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
enum OutcomeSpec {
    Ok {
        answer: String,
        #[serde(default)]
        post: bool,
        // `whisper` lives on the parsed markup (the runner derives it), so the
        // outcome's copy is not consulted here — kept in the corpus for clarity.
        #[serde(default, rename = "whisper")]
        _whisper: bool,
    },
    Err {
        error: CarinaErrorSpec,
    },
    Throw {
        message: String,
    },
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LogLabelsSpec {
    detected: String,
    failed: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CallSpec {
    name: String,
    text: String,
    #[serde(default)]
    asker_participant_id: Option<String>,
    operator_initiated: bool,
    log_labels: LogLabelsSpec,
    outcome: OutcomeSpec,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    test_pepper_base64: String,
    user_id: String,
    chat: ChatSpec,
    answerer_id: String,
    answerer_participant_id: String,
    calls: Vec<CallSpec>,
}

#[derive(Deserialize)]
struct ChatSpec {
    id: String,
}

// ---------------------------------------------------------------------------
// Rust-side seam impls.
// ---------------------------------------------------------------------------

struct RecordingCallbacks {
    consulting: Vec<String>,
    public_answers: Vec<String>,
}
impl CarinaMarkupCallbacks for RecordingCallbacks {
    fn on_consulting(&mut self, name: &str) {
        self.consulting.push(name.to_string());
    }
    fn on_public_answer(&mut self, m: &PostedCarinaMessage) {
        self.public_answers.push(m.id.clone());
    }
}

/// The query seam bound to this corpus call. Posts through the ported marshaling
/// for an `ok`+post outcome (via `write_blocking`, exactly as the async
/// `post_carina_response` does via `write`), else returns the fixed error / a
/// throw. `db` is captured to reach the writer.
struct CorpusQuery<'a> {
    db: &'a Db,
    chat_id: String,
    answerer_id: String,
    answerer_participant_id: String,
    outcome: OutcomeKind,
}

enum OutcomeKind {
    OkPost {
        answer: String,
    },
    Err {
        kind: CarinaErrorKind,
        character_name: Option<String>,
        detail: Option<String>,
    },
    Throw {
        message: String,
    },
}

impl RunCarinaQuery for CorpusQuery<'_> {
    #[allow(clippy::manual_async_fn)]
    fn run(
        &mut self,
        opts: RunCarinaQueryOptions,
    ) -> impl std::future::Future<Output = Result<CarinaResult, CarinaRunError>> + Send {
        async move {
            match &self.outcome {
                OutcomeKind::Throw { message } => Err(CarinaRunError(message.clone())),
                OutcomeKind::Err {
                    kind,
                    character_name,
                    detail,
                } => Ok(CarinaResult::Err {
                    error: CarinaError {
                        kind: *kind,
                        detail: detail.clone(),
                        character_name: character_name.clone(),
                    },
                }),
                OutcomeKind::OkPost { answer } => {
                    let params = PostCarinaResponseParams {
                        chat_id: self.chat_id.clone(),
                        answer: answer.clone(),
                        answerer_id: self.answerer_id.clone(),
                        question: opts.question.clone(),
                        participant_id: Some(self.answerer_participant_id.clone()),
                        whisper: opts.whisper,
                        asker_participant_id: opts.asker_participant_id.clone(),
                    };
                    // The ported marshaling — the same bytes `post_carina_response`
                    // writes; posted through the writer thread (async now that the
                    // seam is `-> impl Future`).
                    let (event, posted) = build_carina_message(&params)
                        .ok_or_else(|| CarinaRunError("build_carina_message failed".into()))?;
                    let chat_id = self.chat_id.clone();
                    let write = self
                        .db
                        .write(move |writers| {
                            writers.main().chat_messages().add_message(&chat_id, &event)
                        })
                        .await;
                    match write {
                        Ok(()) => Ok(CarinaResult::Ok {
                            answer: answer.clone(),
                            message_id: posted.id.clone(),
                            message: posted,
                            answerer_id: self.answerer_id.clone(),
                            answerer_name: opts.character_name.clone(),
                        }),
                        // v4's postCarinaResponse returns null on a write failure →
                        // runCarinaQuery maps that to an llm-failed error.
                        Err(_) => Ok(CarinaResult::Err {
                            error: CarinaError {
                                kind: CarinaErrorKind::LlmFailed,
                                detail: Some("failed to persist answer".into()),
                                character_name: Some(opts.character_name.clone()),
                            },
                        }),
                    }
                }
            }
        }
    }
}

struct RecordingProspero {
    calls: Vec<ProsperoCarinaErrorArgs>,
}
impl PostProsperoCarinaError for RecordingProspero {
    fn post(&mut self, args: ProsperoCarinaErrorArgs) -> Result<(), CarinaRunError> {
        self.calls.push(args);
        Ok(())
    }
}

fn kind_from_str(s: &str) -> CarinaErrorKind {
    match s {
        "not-found" => CarinaErrorKind::NotFound,
        "no-profile" => CarinaErrorKind::NoProfile,
        "llm-failed" => CarinaErrorKind::LlmFailed,
        other => panic!("unknown carina error kind {other}"),
    }
}

// ---------------------------------------------------------------------------
// Normalization: build the Rust-side trace value matching the oracle's shape.
// ---------------------------------------------------------------------------

/// Turn the Rust trace + recorded callbacks into the oracle's ordered JSON trace.
/// The minted posted-message id is tokenized to `<msg>` (its byte-identity is
/// proven by the chat_messages dump, not the trace).
fn trace_to_json(trace: &[CarinaTrace]) -> Vec<Value> {
    trace
        .iter()
        .filter_map(|t| match t {
            // NoMarkup is implicit (empty trace) on the oracle side too — a
            // no-markup call emits an empty `trace` array.
            CarinaTrace::NoMarkup => None,
            CarinaTrace::Consulting { character_name } => {
                Some(json!({ "t": "consulting", "characterName": character_name }))
            }
            CarinaTrace::QueryInvoked {
                character_name,
                question,
                whisper,
                asker_participant_id,
                operator_initiated,
            } => Some(json!({
                "t": "query-invoked",
                "characterName": character_name,
                "question": question,
                "whisper": whisper,
                "askerParticipantId": asker_participant_id,
                "operatorInitiated": operator_initiated,
            })),
            CarinaTrace::PublicAnswer { .. } => {
                Some(json!({ "t": "public-answer", "messageId": "<msg>" }))
            }
            // A whisper answer fires NO onPublicAnswer, so the oracle's trace has
            // no entry for it — the Rust WhisperAnswerNotSpliced is trace-only
            // bookkeeping and is dropped to match.
            CarinaTrace::WhisperAnswerNotSpliced { .. } => None,
            CarinaTrace::ProsperoErrorPosted(args) => Some(json!({
                "t": "prospero-error",
                "chatId": args.chat_id,
                "kind": args.kind.as_str(),
                "characterName": args.character_name,
                "detail": args.detail,
                "whisper": args.whisper,
                "askerParticipantId": args.asker_participant_id,
            })),
            // A swallowed seam throw is invisible to the oracle (v4 logs + returns,
            // emitting nothing to the trace) — drop it here too.
            CarinaTrace::SeamThrewAndSwallowed { .. } => None,
        })
        .collect()
}

/// Collapse a `chat_messages` dump: tokenize `id` to `<id-N>` in row order (the
/// dump is already sorted by `content`), placeholder `createdAt` → `<ts>`.
fn normalize_messages(dump: &mut Value) {
    let rows = dump
        .get_mut("rows")
        .and_then(Value::as_array_mut)
        .expect("rows array");
    let mut id_map: HashMap<String, String> = HashMap::new();
    for row in rows.iter_mut() {
        let obj = row.as_object_mut().expect("row object");
        if let Some(Value::String(id)) = obj.get("id") {
            let next = format!("<id-{}>", id_map.len());
            let token = id_map.entry(id.clone()).or_insert(next).clone();
            obj.insert("id".to_string(), Value::String(token));
        }
        if let Some(v) = obj.get_mut("createdAt") {
            if !v.is_null() {
                *v = Value::String("<ts>".to_string());
            }
        }
    }
}

fn assert_dump_eq(got: &Value, oracle: &Value, label: &str) {
    assert_eq!(
        got.get("columns"),
        oracle.get("columns"),
        "{label}: column set differs"
    );
    assert_eq!(
        got.get("rows"),
        oracle.get("rows"),
        "{label}: rows differ\nGOT:    {}\nORACLE: {}",
        got.get("rows").unwrap_or(&Value::Null),
        oracle.get("rows").unwrap_or(&Value::Null)
    );
}

#[test]
fn carina_runner_tier3_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_CARINA") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_CARINA to the oracle NDJSON (see header).");
            return;
        }
    };
    let fixture = match std::env::var("QT_FIXTURE_CARINA") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_FIXTURE_CARINA to the seed fixture .db (see header).");
            return;
        }
    };

    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../harness/oracle/fixtures/carina-runner-tier3.json"),
        )
        .unwrap_or_else(|e| panic!("read spec: {e}")),
    )
    .expect("parse spec");

    // Parse the oracle NDJSON into per-call traces + the chat_messages table.
    let mut oracle_traces: HashMap<String, Vec<Value>> = HashMap::new();
    let mut oracle_messages: Option<Value> = None;
    for line in std::fs::read_to_string(&oracle_path)
        .unwrap_or_else(|e| panic!("read oracle: {e}"))
        .lines()
        .filter(|l| !l.trim().is_empty())
    {
        let v: Value = serde_json::from_str(line).expect("parse oracle line");
        match v.get("kind").and_then(Value::as_str) {
            Some("trace") => {
                let call = v["call"].as_str().unwrap().to_string();
                let mut entries = v["trace"].as_array().cloned().unwrap_or_default();
                // The oracle emits the raw minted message id in a `public-answer`
                // entry; tokenize it to `<msg>` to match the Rust side (the byte
                // identity is proven by the chat_messages dump, not the trace).
                for e in entries.iter_mut() {
                    if e.get("t").and_then(Value::as_str) == Some("public-answer") {
                        if let Some(obj) = e.as_object_mut() {
                            obj.insert("messageId".to_string(), Value::String("<msg>".to_string()));
                        }
                    }
                }
                oracle_traces.insert(call, entries);
            }
            Some("table") => oracle_messages = Some(v),
            _ => {}
        }
    }
    let mut oracle_messages = oracle_messages.expect("oracle chat_messages table");

    // Copy the fixture + open a Db (main-only; the runner never touches siblings).
    let work =
        std::env::temp_dir().join(format!("qt-carina-runner-rust-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&work);
    std::fs::copy(&fixture, &work).unwrap_or_else(|e| panic!("copy fixture: {e}"));

    let db = Db::open(DbPaths::main_only(&work), &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open fixture copy: {e}"));

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("build tokio runtime");

    for call in &spec.calls {
        let mut cb = RecordingCallbacks {
            consulting: Vec::new(),
            public_answers: Vec::new(),
        };
        let outcome = match &call.outcome {
            OutcomeSpec::Ok { answer, post, .. } => {
                assert!(
                    *post || call.name == "no_markup",
                    "ok outcome must post (except no_markup)"
                );
                OutcomeKind::OkPost {
                    answer: answer.clone(),
                }
            }
            OutcomeSpec::Err { error } => OutcomeKind::Err {
                kind: kind_from_str(&error.kind),
                character_name: error.character_name.clone(),
                detail: error.detail.clone(),
            },
            OutcomeSpec::Throw { message } => OutcomeKind::Throw {
                message: message.clone(),
            },
        };
        let mut query = CorpusQuery {
            db: &db,
            chat_id: spec.chat.id.clone(),
            answerer_id: spec.answerer_id.clone(),
            answerer_participant_id: spec.answerer_participant_id.clone(),
            outcome,
        };
        let mut prospero = RecordingProspero { calls: Vec::new() };

        let opts = CarinaMarkupOptions {
            user_id: spec.user_id.clone(),
            chat_id: spec.chat.id.clone(),
            text: call.text.clone(),
            asker_participant_id: call.asker_participant_id.clone(),
            operator_initiated: call.operator_initiated,
            log_labels: CarinaMarkupLogLabels {
                detected: call.log_labels.detected.clone(),
                failed: call.log_labels.failed.clone(),
            },
        };

        // The Carina query seam is async (`-> impl Future`); drive the runner on a
        // current-thread runtime. `CorpusQuery`'s write goes through the writer OS
        // thread, so `db.write().await` completes under block_on.
        let trace = rt.block_on(run_carina_markup_query(
            &opts,
            &mut cb,
            &mut query,
            &mut prospero,
        ));
        let got_trace = trace_to_json(&trace);
        let oracle_trace = oracle_traces
            .get(&call.name)
            .unwrap_or_else(|| panic!("oracle trace for call {}", call.name));
        assert_eq!(
            &got_trace, oracle_trace,
            "trace differs for call {}\nGOT:    {:?}\nORACLE: {:?}",
            call.name, got_trace, oracle_trace
        );
    }

    // Diff chat_messages.
    let mut got_messages = db
        .read_main(|conn| dump_table_json_conn(conn, "chat_messages", "content"))
        .expect("dump chat_messages");
    let _ = std::fs::remove_file(&work);

    normalize_messages(&mut got_messages);
    normalize_messages(&mut oracle_messages);
    assert_dump_eq(&got_messages, &oracle_messages, "chat_messages");

    let nm = got_messages["rows"]
        .as_array()
        .map(|a| a.len())
        .unwrap_or(0);
    assert!(nm > 0, "chat_messages dump looks empty");
    eprintln!("OK: carina runner tier-3 matched oracle ({nm} message rows).");
}
