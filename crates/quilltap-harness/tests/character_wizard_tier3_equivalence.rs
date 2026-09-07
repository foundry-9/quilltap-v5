//! P4.9K2 tier-3 differential: `api::generators_wizard::{character_wizard,
//! character_wizard_stream}` over `generators::wizard::{run_character_wizard,
//! run_character_wizard_streaming}` (v4 `characters/handlers/post.ts`
//! `ai-wizard` / `ai-wizard-stream` → `lib/services/character-wizard.service.ts`)
//! vs v4's REAL route, the model boundary canned on both sides.
//!
//! Both sides run every corpus case on a FRESH copy of the committed
//! `character-generators-{main,mount}.db` pair and diff:
//!   * the status + body — the non-streaming twin's `WizardResult` raw, the
//!     Zod `details`, v4's generic 500 on a runner throw;
//!   * the WHOLE progress-event trace of the streaming twin (v4's SSE frames
//!     decoded, v5's `Event::GeneratorProgress` frames drained off the engine
//!     broadcast);
//!   * every recorded model call — provider / baseUrl / model / temperature /
//!     maxTokens / profileParameters / the whole message list INCLUDING the
//!     vision call's base64 attachment (`{id, filename, mimeType, data}`) — the
//!     context prompt, the field prompts and the image bytes are the unit;
//!   * the `[CharacterWizard]` log lines (level + message + context).
//!
//! Not compared: the api key v4 hands `sendMessage` and the `CHARACTER_WIZARD`
//! `llm_logs` rows (no llm-logs partition on either side). The dispatch's
//! `{ terminal }` payload is asserted against the last drained frame.
//!
//! Regenerate the oracle (Node 24, from the v4 checkout — see the .ts header):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   V5W=${V5W:-$HOME/source/quilltap-v5}
//!   TMPO=/tmp/qt-character-wizard-oracle
//!   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
//!   cp "$V5W/harness/oracle/cases/character-wizard-tier3.test.ts" "$TMPO/cases/"
//!   cp "$V5W/harness/oracle/fixtures/character-generators.json" "$TMPO/fixtures/"
//!   cp "$V5W/harness/oracle/fixtures/character-wizard-tier3.json" "$TMPO/fixtures/"
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_CG_MAIN=$V5W/crates/quilltap-web/tests/fixtures/character-generators-main.db \
//!   QT_FIXTURE_CG_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/character-generators-mount.db \
//!   QT_ORACLE_OUT=/tmp/oracle-character-wizard.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=300000 \
//!       --roots "$PWD" --roots "$TMPO/cases" -- character-wizard-tier3
//! Run:
//!   QT_ORACLE_CHARACTER_WIZARD=/tmp/oracle-character-wizard.ndjson \
//!     cargo test -p quilltap-harness --test character_wizard_tier3_equivalence -- --nocapture
//!
//! While v4 HEAD is past the baseline the regen needs a worktree pinned at
//! the baseline — the sweep driver's job (`recipe_sweep.py --run
//! character_wizard_tier3_equivalence --v4 <pin>`), never a path baked in here.

use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use quilltap_core::api::generators_wizard::{
    character_wizard, character_wizard_stream, AiImportDriverRequest, GeneratorsWizardDriver,
    GeneratorsWizardFuture, WizardDriverRequest,
};
use quilltap_core::api::types::{ErrorKind, Event, Response};
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::generators::wizard::{
    run_character_wizard, run_character_wizard_streaming, OnProgress, WizardResult,
};
use quilltap_core::model::completion::{
    CannedCompletionProvider, CompletionError, CompletionParams, CompletionProvider,
    CompletionResponse,
};
use quilltap_core::services::file_storage::StorageBackend;
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::sync::broadcast;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    test_pepper_base64: String,
    user_id: String,
}

#[derive(Deserialize, Clone)]
#[serde(untagged)]
enum CallSpec {
    Content(String),
    Shaped {
        #[serde(default)]
        content: Option<String>,
        #[serde(default)]
        throws: Option<String>,
    },
}

#[derive(Deserialize)]
struct Case {
    name: String,
    action: String,
    body: Value,
    calls: Vec<CallSpec>,
}

#[derive(Deserialize)]
struct Corpus {
    cases: Vec<Case>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OracleRow {
    name: String,
    status: i64,
    body: Value,
    events: Vec<Value>,
    calls: Vec<Value>,
    log_lines: Vec<Value>,
}

fn oracle_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../harness/oracle/fixtures")
}
fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../quilltap-web/tests/fixtures")
}

fn status_of(kind: &ErrorKind) -> i64 {
    match kind {
        ErrorKind::BadRequest => 400,
        ErrorKind::Unauthorized => 401,
        ErrorKind::Forbidden => 403,
        ErrorKind::NotFound => 404,
        ErrorKind::Conflict => 409,
        ErrorKind::Unprocessable => 422,
        ErrorKind::Locked | ErrorKind::Unavailable => 503,
        ErrorKind::Internal => 500,
    }
}

fn to_status_body(r: Response) -> (i64, Value) {
    match r {
        Response::Character(v) => (200, v),
        Response::Error(e) => {
            let status = status_of(&e.kind);
            if let Some(body) = e.unavailable_wire_body() {
                return (status, body);
            }
            let mut body = json!({ "error": e.message });
            if let Some(d) = e.details {
                body["details"] = *d;
            }
            (status, body)
        }
        other => panic!("unexpected response variant: {other:?}"),
    }
}

/// The mount-blob-only backend: the fixture's files are stored under
/// `mount-blob:` keys, which `download_file` reads from the mount DB before
/// ever consulting the backend — so a disk read here is a corpus defect.
struct NoDiskBackend;
impl StorageBackend for NoDiskBackend {
    fn upload(&self, _: &str, _: &[u8], _: &str) -> Result<(), String> {
        Err("no disk backend in the wizard differential".into())
    }
    fn download(&self, key: &str) -> Result<Vec<u8>, String> {
        Err(format!(
            "no disk backend in the wizard differential (key {key})"
        ))
    }
    fn delete(&self, _: &str) -> Result<(), String> {
        Ok(())
    }
    fn exists(&self, _: &str) -> Result<bool, String> {
        Ok(false)
    }
}

/// The model seam: scripted BY CALL INDEX (the oracle's shape), recording
/// every request as the oracle records it — attachments as
/// `{id, filename, mimeType, data}`.
struct ScriptedProvider {
    script: Vec<(Option<String>, Option<String>)>,
    next: Mutex<usize>,
    calls: Mutex<Vec<Value>>,
    case: String,
}

impl CompletionProvider for ScriptedProvider {
    fn send_message(
        &self,
        provider: &str,
        base_url: Option<&str>,
        params: &CompletionParams,
    ) -> impl std::future::Future<Output = Result<CompletionResponse, CompletionError>> + Send {
        let messages: Vec<Value> = params
            .messages
            .iter()
            .map(|m| json!({"role": m.role.as_str(), "content": m.content}))
            .collect();
        let mut messages = messages;
        // v4 attaches on the message; v5 carries attachments on the params
        // (one user message on the vision call) — recorded on the last
        // message, which is where v4's recorder finds them.
        if !params.attachments.is_empty() {
            if let Some(last) = messages.last_mut() {
                last["attachments"] = Value::Array(
                    params
                        .attachments
                        .iter()
                        .map(|a| json!({"id": a.id, "filename": a.filename, "mimeType": a.mime_type, "data": a.data}))
                        .collect(),
                );
            }
        }
        self.calls.lock().unwrap().push(json!({
            "provider": provider,
            "baseUrl": base_url,
            "model": params.model,
            "temperature": params.temperature,
            "maxTokens": params.max_tokens,
            "profileParameters": params.profile_parameters,
            "messages": messages,
        }));
        let index = {
            let mut n = self.next.lock().unwrap();
            let i = *n;
            *n += 1;
            i
        };
        let scripted = self.script.get(index).cloned();
        let case = self.case.clone();
        let provider = provider.to_string();
        let base_url = base_url.map(str::to_string);
        let params = params.clone();
        async move {
            let (content, throws) =
                scripted.unwrap_or_else(|| panic!("no scripted call #{index} for case {case}"));
            let canned = match throws {
                Some(msg) => CannedCompletionProvider::new().with_failure_full(
                    &provider,
                    &params.model,
                    params.temperature,
                    &params.messages,
                    &params.attachments,
                    msg,
                ),
                None => CannedCompletionProvider::new().with_response_full(
                    &provider,
                    &params.model,
                    params.temperature,
                    &params.messages,
                    &params.attachments,
                    CompletionResponse {
                        content: content.unwrap_or_default(),
                        usage: None,
                        finish_reason: None,
                        attachment_results: None,
                        cache_usage: None,
                    },
                ),
            };
            canned
                .send_message(&provider, base_url.as_deref(), &params)
                .await
        }
    }
}

struct TestDriver {
    db: Db,
    completion: Arc<ScriptedProvider>,
}

impl GeneratorsWizardDriver for TestDriver {
    fn wizard<'a>(
        &'a self,
        req: WizardDriverRequest,
    ) -> GeneratorsWizardFuture<'a, Result<WizardResult, String>> {
        Box::pin(async move {
            run_character_wizard(
                &self.db,
                &*self.completion,
                &NoDiskBackend,
                &req.request,
                &req.user_id,
            )
            .await
        })
    }
    fn wizard_stream<'a>(
        &'a self,
        req: WizardDriverRequest,
        on_progress: OnProgress<'a>,
    ) -> GeneratorsWizardFuture<'a, ()> {
        Box::pin(async move {
            run_character_wizard_streaming(
                &self.db,
                &*self.completion,
                &NoDiskBackend,
                &req.request,
                &req.user_id,
                on_progress,
            )
            .await
        })
    }
    fn ai_import_stream<'a>(
        &'a self,
        _req: AiImportDriverRequest,
        _on_progress: OnProgress<'a>,
    ) -> GeneratorsWizardFuture<'a, ()> {
        Box::pin(async { panic!("the wizard differential never runs the AI import") })
    }
}

fn fresh_pair(spec: &Spec, tag: &str) -> (Db, PathBuf) {
    let scratch = std::env::temp_dir().join(format!("qt-wiz-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    let main = scratch.join("main.db");
    let mount = scratch.join("mount.db");
    std::fs::copy(fixtures_dir().join("character-generators-main.db"), &main).unwrap();
    std::fs::copy(fixtures_dir().join("character-generators-mount.db"), &mount).unwrap();
    let db = Db::open(
        DbPaths {
            main,
            mount_index: Some(mount),
            llm_logs: None,
        },
        &spec.test_pepper_base64,
    )
    .expect("open fixture pair");
    (db, scratch)
}

/// One captured `[CharacterWizard]` line → the oracle's `{level, message,
/// context}` row (the rig renders `LEVEL target message context=<json>`).
fn parse_log_line(line: &str) -> Value {
    let mut head = line.splitn(3, ' ');
    let level = head.next().unwrap().to_lowercase();
    let _target = head.next().unwrap();
    let rest_from_message = head.next().unwrap();
    let ctx_rel = rest_from_message
        .find(" context=")
        .expect("a context field");
    let message = &rest_from_message[..ctx_rel];
    let ctx_pos = line.find(" context=").unwrap();
    let rest = &line[ctx_pos + " context=".len()..];
    let context: Value = serde_json::Deserializer::from_str(rest)
        .into_iter::<Value>()
        .next()
        .expect("a context value")
        .expect("valid context JSON");
    json!({"level": level, "message": message, "context": context})
}

fn sorted(v: &Value) -> Value {
    match v {
        Value::Array(a) => Value::Array(a.iter().map(sorted).collect()),
        Value::Object(o) => {
            let mut keys: Vec<&String> = o.keys().collect();
            keys.sort();
            let mut m = serde_json::Map::new();
            for k in keys {
                m.insert(k.clone(), sorted(&o[k]));
            }
            Value::Object(m)
        }
        _ => v.clone(),
    }
}

fn canon_numbers(v: &mut Value) {
    match v {
        Value::Number(n) => {
            if let Some(f) = n.as_f64() {
                if f.is_finite() && f.fract() == 0.0 && f.abs() < 9.007_199_254_740_992e15 {
                    *v = Value::Number((f as i64).into());
                }
            }
        }
        Value::Array(a) => a.iter_mut().for_each(canon_numbers),
        Value::Object(o) => o.iter_mut().for_each(|(_, x)| canon_numbers(x)),
        _ => {}
    }
}

fn normalize(mut v: Value) -> String {
    canon_numbers(&mut v);
    if let Some(calls) = v.get_mut("calls").and_then(Value::as_array_mut) {
        for c in calls {
            if let Some(o) = c.as_object_mut() {
                o.remove("apiKey");
            }
        }
    }
    serde_json::to_string_pretty(&sorted(&v)).unwrap()
}

fn first_diff(got: &str, want: &str) -> String {
    let g: Vec<&str> = got.lines().collect();
    let w: Vec<&str> = want.lines().collect();
    for i in 0..g.len().max(w.len()) {
        let gi = g.get(i).copied().unwrap_or("<none>");
        let wi = w.get(i).copied().unwrap_or("<none>");
        if gi != wi {
            let mut ctx = String::new();
            for j in i.saturating_sub(4)..i {
                ctx.push_str(&format!("   = {}\n", g.get(j).copied().unwrap_or("")));
            }
            ctx.push_str(&format!("  GOT : {gi}\n  WANT: {wi}\n"));
            return ctx;
        }
    }
    "(identical line-by-line)".to_string()
}

fn run_case(spec: &Spec, c: &Case) -> Value {
    let (db, scratch) = fresh_pair(spec, &c.name);
    let completion = Arc::new(ScriptedProvider {
        script: c
            .calls
            .iter()
            .map(|s| match s {
                CallSpec::Content(s) => (Some(s.clone()), None),
                CallSpec::Shaped { content, throws } => (content.clone(), throws.clone()),
            })
            .collect(),
        next: Mutex::new(0),
        calls: Mutex::new(Vec::new()),
        case: c.name.clone(),
    });
    let driver: Arc<dyn GeneratorsWizardDriver> = Arc::new(TestDriver {
        db: db.clone(),
        completion: completion.clone(),
    });
    let (events_tx, mut events_rx) = broadcast::channel::<Event>(4096);
    let progress_id = format!("p-{}", c.name);
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let streaming = c.action == "ai-wizard-stream";
    let (response, lines) = quilltap_core::test_support::captured_with(|| {
        rt.block_on(async {
            if streaming {
                character_wizard_stream(
                    Some(&driver),
                    &events_tx,
                    &spec.user_id,
                    Some(&progress_id),
                    &c.body,
                )
                .await
            } else {
                character_wizard(Some(&driver), &spec.user_id, &c.body).await
            }
        })
    });
    let mut events: Vec<Value> = Vec::new();
    while let Ok(ev) = events_rx.try_recv() {
        let v = serde_json::to_value(&ev).unwrap();
        if v["progressId"] == json!(progress_id) && v["type"] == json!("generatorProgress") {
            assert_eq!(v["generator"], json!("wizard"));
            events.push(v["event"].clone());
        }
    }
    let (status, body) = to_status_body(response);
    let body = if streaming && status == 200 {
        assert_eq!(
            body,
            json!({ "terminal": events.last() }),
            "the dispatch terminal is the last frame ({})",
            c.name
        );
        Value::Null
    } else {
        body
    };
    let log_lines: Vec<Value> = lines
        .iter()
        .filter(|l| l.contains("[CharacterWizard]"))
        .map(|l| parse_log_line(l))
        .collect();
    let calls = completion.calls.lock().unwrap().clone();
    drop(driver);
    drop(db);
    let _ = std::fs::remove_dir_all(&scratch);
    json!({
        "name": c.name,
        "status": status,
        "body": body,
        "events": events,
        "calls": calls,
        "logLines": log_lines,
    })
}

#[test]
fn character_wizard_matches_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_CHARACTER_WIZARD") else {
        eprintln!("SKIP: set QT_ORACLE_CHARACTER_WIZARD (see the test header).");
        return;
    };
    let text = std::fs::read_to_string(&oracle_path).unwrap();
    assert!(
        !text.trim().is_empty(),
        "{oracle_path} is EMPTY — the regen truncated it before failing (ledger §5.1)"
    );
    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(oracle_dir().join("character-generators.json")).unwrap(),
    )
    .unwrap();
    let corpus: Corpus = serde_json::from_str(
        &std::fs::read_to_string(oracle_dir().join("character-wizard-tier3.json")).unwrap(),
    )
    .unwrap();
    let oracle: HashMap<String, OracleRow> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let row: OracleRow = serde_json::from_str(l).unwrap();
            (row.name.clone(), row)
        })
        .collect();
    let corpus_names: BTreeSet<&str> = corpus.cases.iter().map(|c| c.name.as_str()).collect();
    let oracle_names: BTreeSet<&str> = oracle.keys().map(String::as_str).collect();
    assert_eq!(
        corpus_names, oracle_names,
        "the corpus and the oracle disagree on the case set"
    );

    let mut failed: Vec<String> = Vec::new();
    let (mut model_calls, mut frames, mut vision_calls, mut zod_rows, mut json_rows) =
        (0usize, 0usize, 0usize, 0usize, 0usize);
    for c in &corpus.cases {
        let want_row = &oracle[&c.name];
        let got = run_case(&spec, c);
        model_calls += want_row.calls.len();
        frames += want_row.events.len();
        vision_calls += want_row
            .calls
            .iter()
            .filter(|call| {
                call["messages"]
                    .as_array()
                    .is_some_and(|m| m.iter().any(|x| x.get("attachments").is_some()))
            })
            .count();
        if want_row.status == 400 {
            zod_rows += 1;
        }
        if c.action == "ai-wizard" && want_row.status == 200 {
            json_rows += 1;
        }
        let want = json!({
            "name": want_row.name,
            "status": want_row.status,
            "body": want_row.body,
            "events": want_row.events,
            "calls": want_row.calls,
            "logLines": want_row.log_lines,
        });
        let (g, w) = (normalize(got), normalize(want));
        if g != w {
            failed.push(format!("{}:\n{}", c.name, first_diff(&g, &w)));
        }
    }
    assert!(
        model_calls >= 60,
        "the oracle recorded only {model_calls} model calls"
    );
    assert!(
        frames >= 100,
        "the oracle recorded only {frames} progress frames"
    );
    assert!(
        vision_calls >= 3,
        "the oracle recorded only {vision_calls} vision calls"
    );
    assert!(
        zod_rows >= 3,
        "the oracle recorded only {zod_rows} Zod refusals"
    );
    assert!(
        json_rows >= 3,
        "the oracle recorded only {json_rows} non-streaming results"
    );
    eprintln!(
        "character_wizard_tier3_equivalence: {} cases, {model_calls} model calls ({vision_calls} vision), {frames} frames, {zod_rows} Zod refusals",
        corpus.cases.len()
    );
    assert!(
        failed.is_empty(),
        "{} case(s) DIFFER:\n{}",
        failed.len(),
        failed.join("\n")
    );
}
