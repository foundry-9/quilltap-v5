//! P4.9K2 tier-3 differential: `api::generators_wizard::ai_import_stream` over
//! `generators::ai_import::run_ai_import_streaming` (v4 `system/tools/route.ts`
//! `ai-import-stream` → `lib/services/ai-import.service.ts`) vs v4's REAL
//! route, the model boundary canned on both sides and the clock frozen.
//!
//! Both sides run every corpus case on a FRESH copy of the committed
//! `character-generators-{main,mount}.db` pair and diff the route's two 400s,
//! the WHOLE frame trace (the `done` frame's assembled `QuilltapExport` and
//! `stepResults` included — uuids remapped `<minted-N>` in first-seen order,
//! the manifest's `appVersion` normalized), every recorded model call (the
//! source context and the step prompts as bytes), and the `[AIImport]` log
//! lines.
//!
//! ## The validation pin — a RECORDED DIVERGENCE, both directions
//!
//! v4 validates the assembled export through ajv and answers
//! `step_complete validation` (`Validation passed`) for every well-formed
//! export; v5 carries no JSON-Schema engine and answers a NAMED
//! `step_error validation` with `errors.validation` set (the runner's module
//! header). The comparison therefore (a) asserts v4's `[step_start,
//! step_complete{Validation passed}]` pair and v5's `[step_start,
//! step_error{VALIDATION_UNAVAILABLE}]` pair explicitly on every successful
//! run, then (b) strips both pairs and v5's `errors.validation` before the
//! byte diff. When an engine lands, (a) trips and the pin retires.
//!
//! `appVersion`: v4 stamps its `package.json` version, v5 the engine's; both
//! are asserted and then normalized.
//!
//! Regenerate the oracle (Node 24, from the v4 checkout — see the .ts header):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   V5W=${V5W:-$HOME/source/quilltap-v5}
//!   TMPO=/tmp/qt-ai-import-oracle
//!   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
//!   cp "$V5W/harness/oracle/cases/ai-import-tier3.test.ts" "$TMPO/cases/"
//!   cp "$V5W/harness/oracle/fixtures/character-generators.json" "$TMPO/fixtures/"
//!   cp "$V5W/harness/oracle/fixtures/ai-import-tier3.json" "$TMPO/fixtures/"
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_CG_MAIN=$V5W/crates/quilltap-web/tests/fixtures/character-generators-main.db \
//!   QT_FIXTURE_CG_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/character-generators-mount.db \
//!   QT_ORACLE_OUT=/tmp/oracle-ai-import.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=300000 \
//!       --roots "$PWD" --roots "$TMPO/cases" -- ai-import-tier3
//! Run:
//!   QT_ORACLE_AI_IMPORT=/tmp/oracle-ai-import.ndjson \
//!     cargo test -p quilltap-harness --test ai_import_tier3_equivalence -- --nocapture
//!
//! While v4 HEAD is past the baseline the regen needs a worktree pinned at
//! the baseline — the sweep driver's job (`recipe_sweep.py --run
//! ai_import_tier3_equivalence --v4 <pin>`), never a path baked in here.

use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use quilltap_core::api::generators_wizard::{
    ai_import_stream, AiImportDriverRequest, GeneratorsWizardDriver, GeneratorsWizardFuture,
    WizardDriverRequest,
};
use quilltap_core::api::types::{ErrorKind, Event, Response};
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::generators::ai_import::{run_ai_import_streaming, VALIDATION_UNAVAILABLE};
use quilltap_core::generators::wizard::{OnProgress, WizardResult};
use quilltap_core::model::completion::{
    CannedCompletionProvider, CompletionError, CompletionParams, CompletionProvider,
    CompletionResponse,
};
use quilltap_core::services::file_storage::StorageBackend;
use regex::Regex;
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::sync::broadcast;

/// The version stamp this harness hands the runner (the engine passes its own
/// in production); v4's oracle stamps the pin's `package.json` version.
const V5_APP_VERSION: &str = "0.0.0-harness";
const V4_APP_VERSION: &str = "4.10.0-dev.0";

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
    body: Value,
    calls: Vec<CallSpec>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Corpus {
    frozen_now_ms: i64,
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

struct NoDiskBackend;
impl StorageBackend for NoDiskBackend {
    fn upload(&self, _: &str, _: &[u8], _: &str) -> Result<(), String> {
        Err("no disk backend in the import differential".into())
    }
    fn download(&self, key: &str) -> Result<Vec<u8>, String> {
        Err(format!(
            "no disk backend in the import differential (key {key})"
        ))
    }
    fn delete(&self, _: &str) -> Result<(), String> {
        Ok(())
    }
    fn exists(&self, _: &str) -> Result<bool, String> {
        Ok(false)
    }
}

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
        self.calls.lock().unwrap().push(json!({
            "provider": provider,
            "baseUrl": base_url,
            "model": params.model,
            "temperature": params.temperature,
            "maxTokens": params.max_tokens,
            "profileParameters": params.profile_parameters,
            "messages": params.messages.iter().map(|m| json!({"role": m.role.as_str(), "content": m.content})).collect::<Vec<_>>(),
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
                Some(msg) => CannedCompletionProvider::new().with_failure(
                    &provider,
                    &params.model,
                    params.temperature,
                    &params.messages,
                    msg,
                ),
                None => CannedCompletionProvider::new().with_response(
                    &provider,
                    &params.model,
                    params.temperature,
                    &params.messages,
                    content.unwrap_or_default(),
                    None,
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
        _req: WizardDriverRequest,
    ) -> GeneratorsWizardFuture<'a, Result<WizardResult, String>> {
        Box::pin(async { panic!("the import differential never runs the wizard") })
    }
    fn wizard_stream<'a>(
        &'a self,
        _req: WizardDriverRequest,
        _on_progress: OnProgress<'a>,
    ) -> GeneratorsWizardFuture<'a, ()> {
        Box::pin(async { panic!("the import differential never runs the wizard") })
    }
    fn ai_import_stream<'a>(
        &'a self,
        req: AiImportDriverRequest,
        on_progress: OnProgress<'a>,
    ) -> GeneratorsWizardFuture<'a, ()> {
        Box::pin(async move {
            run_ai_import_streaming(
                &self.db,
                &*self.completion,
                &NoDiskBackend,
                &req.request,
                &req.user_id,
                on_progress,
                V5_APP_VERSION,
                req.now_ms,
            )
            .await
        })
    }
}

fn fresh_pair(spec: &Spec, tag: &str) -> (Db, PathBuf) {
    let scratch = std::env::temp_dir().join(format!("qt-imp-{}-{}", tag, std::process::id()));
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

/// One captured `[AIImport]` line → `{level, message, context}` (a line with
/// no `context=` field is v4's context-less call: `null`).
fn parse_log_line(line: &str) -> Value {
    let mut head = line.splitn(3, ' ');
    let level = head.next().unwrap().to_lowercase();
    let _target = head.next().unwrap();
    let rest = head.next().unwrap();
    match rest.find(" context=") {
        Some(pos) => {
            let message = &rest[..pos];
            let ctx_text = &rest[pos + " context=".len()..];
            let context: Value = serde_json::Deserializer::from_str(ctx_text)
                .into_iter::<Value>()
                .next()
                .expect("a context value")
                .expect("valid context JSON");
            json!({"level": level, "message": message, "context": context})
        }
        None => json!({"level": level, "message": rest, "context": Value::Null}),
    }
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

fn remap_minted(serialized: &str) -> String {
    let re =
        Regex::new(r"[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}")
            .unwrap();
    // The fixture's PINNED ids (profiles, files, the user) must stay
    // themselves — only ids the assembler minted are remapped, and those are
    // random on both sides. A pinned id has the fixture's `-0000-4000-8000-`
    // spine; a minted v4 uuid never does.
    let mut map: HashMap<String, String> = HashMap::new();
    let mut next = 1usize;
    re.replace_all(serialized, |caps: &regex::Captures<'_>| {
        let key = caps[0].to_string();
        if key.contains("-0000-4000-8000-") {
            return key;
        }
        map.entry(key)
            .or_insert_with(|| {
                let p = format!("<minted-{next}>");
                next += 1;
                p
            })
            .clone()
    })
    .into_owned()
}

/// The validation pin (module header): assert each side's own pair, then
/// strip it — and v5's `errors.validation` — so the rest diffs as bytes.
fn strip_validation(events: &mut Vec<Value>, side: &str) -> bool {
    let idx = events
        .iter()
        .position(|e| e["type"] == json!("step_start") && e["step"] == json!("validation"));
    let Some(i) = idx else {
        return false;
    };
    let outcome = events.get(i + 1).cloned().unwrap_or(Value::Null);
    match side {
        "v4" => assert_eq!(
            outcome,
            json!({"type": "step_complete", "step": "validation", "snippet": "Validation passed"}),
            "v4's validation outcome moved — the pin retires when v5 gains an engine"
        ),
        _ => assert_eq!(
            outcome,
            json!({"type": "step_error", "step": "validation", "error": VALIDATION_UNAVAILABLE}),
            "v5's validation refusal moved"
        ),
    }
    events.drain(i..=i + 1);
    if side == "v5" {
        if let Some(done) = events.last_mut() {
            if let Some(errors) = done.get_mut("errors").and_then(Value::as_object_mut) {
                let removed = errors.remove("validation");
                assert!(removed.is_some(), "v5's done carried no errors.validation");
                if errors.is_empty() {
                    done.as_object_mut().unwrap().remove("errors");
                }
            }
        }
    }
    true
}

/// The refusal's own log line (v5-only, beside the frame pair): present
/// exactly once on every run that reached validation, never otherwise; then
/// stripped so the remaining lines diff as bytes.
fn strip_validation_log_line(log_lines: &mut Vec<Value>, side: &str, reached_validation: bool) {
    let is_refusal = |l: &Value| {
        l["message"]
            .as_str()
            .is_some_and(|m| m.contains("Validation unavailable"))
    };
    let count = log_lines.iter().filter(|l| is_refusal(l)).count();
    match side {
        "v4" => assert_eq!(count, 0, "v4 has no validation-unavailable line"),
        _ => assert_eq!(
            count,
            usize::from(reached_validation),
            "v5's validation-unavailable line must accompany the frame pair exactly once"
        ),
    }
    log_lines.retain(|l| !is_refusal(l));
    // …and the refusal's `errors.validation` entry is one of the
    // `stepsWithErrors` v5's completion line counts (v4's validation passes
    // and adds none) — discounted here, beside the frame-side strip.
    if side == "v5" && reached_validation {
        let complete = log_lines
            .iter_mut()
            .find(|l| l["message"] == json!("[AIImport] AI character import complete"))
            .expect("v5's completion line accompanies every validated run");
        let n = complete["context"]["stepsWithErrors"]
            .as_u64()
            .expect("stepsWithErrors is a count");
        assert!(n >= 1, "v5's stepsWithErrors must count errors.validation");
        complete["context"]["stepsWithErrors"] = json!(n - 1);
    }
}

fn normalize(mut v: Value, side: &str) -> (String, bool) {
    canon_numbers(&mut v);
    let mut reached_validation = false;
    if let Some(events) = v.get_mut("events").and_then(Value::as_array_mut) {
        reached_validation = strip_validation(events, side);
        if let Some(done) = events.last_mut() {
            if let Some(app) = done.pointer_mut("/result/manifest/appVersion") {
                let want = if side == "v4" {
                    V4_APP_VERSION
                } else {
                    V5_APP_VERSION
                };
                assert_eq!(app, &json!(want), "{side}'s appVersion stamp moved");
                *app = Value::String("<app-version>".into());
            }
        }
    }
    if let Some(log_lines) = v.get_mut("logLines").and_then(Value::as_array_mut) {
        strip_validation_log_line(log_lines, side, reached_validation);
    }
    let serialized = serde_json::to_string_pretty(&sorted(&v)).unwrap();
    (remap_minted(&serialized), reached_validation)
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

fn run_case(spec: &Spec, corpus: &Corpus, c: &Case) -> Value {
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
    let (response, lines) = quilltap_core::test_support::captured_with(|| {
        rt.block_on(ai_import_stream(
            Some(&driver),
            &events_tx,
            &spec.user_id,
            Some(&progress_id),
            &c.body,
            corpus.frozen_now_ms,
        ))
    });
    let mut events: Vec<Value> = Vec::new();
    while let Ok(ev) = events_rx.try_recv() {
        let v = serde_json::to_value(&ev).unwrap();
        if v["progressId"] == json!(progress_id) && v["type"] == json!("generatorProgress") {
            assert_eq!(v["generator"], json!("aiImport"));
            events.push(v["event"].clone());
        }
    }
    let (status, body) = to_status_body(response);
    let body = if status == 200 {
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
        .filter(|l| l.contains("[AIImport]"))
        .map(|l| parse_log_line(l))
        .collect();
    let calls = completion.calls.lock().unwrap().clone();

    // The cross-family importability proof (the order's tier-2 item 8): the
    // assembled `result` is a body `systemImportExecute` accepts — feed the
    // text-only run's export to v5's REAL `import_execute` on the same pair
    // and read the character back by name.
    if c.name == "source_text_only" {
        let result = events
            .last()
            .and_then(|d| d.get("result"))
            .cloned()
            .expect("the text-only run assembles an export");
        let imported = rt.block_on(quilltap_core::api::system_qtap::import_execute(
            &db,
            &spec.user_id,
            &result,
            &json!({"conflictStrategy": "duplicate", "importMemories": true}),
            None,
        ));
        assert!(
            !matches!(imported, Response::Error(_)),
            "the assembled export was refused by import_execute: {imported:?}"
        );
        let landed: i64 = db
            .read_main(|c| {
                c.query_row(
                    "SELECT COUNT(*) FROM characters WHERE name = 'Mira Lanternwright'",
                    [],
                    |r| r.get(0),
                )
                .map_err(Into::into)
            })
            .unwrap();
        assert_eq!(landed, 1, "the imported character did not land");
    }

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
fn ai_import_matches_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_AI_IMPORT") else {
        eprintln!("SKIP: set QT_ORACLE_AI_IMPORT (see the test header).");
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
        &std::fs::read_to_string(oracle_dir().join("ai-import-tier3.json")).unwrap(),
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
    let (mut model_calls, mut frames, mut validated, mut refusals, mut fatal) =
        (0usize, 0usize, 0usize, 0usize, 0usize);
    for c in &corpus.cases {
        let want_row = &oracle[&c.name];
        let got = run_case(&spec, &corpus, c);
        model_calls += want_row.calls.len();
        frames += want_row.events.len();
        if want_row.status == 400 {
            refusals += 1;
        }
        if want_row
            .events
            .iter()
            .any(|e| e["type"] == json!("step_complete") && e["step"] == json!("validation"))
        {
            validated += 1;
        }
        if want_row
            .events
            .last()
            .is_some_and(|e| e["type"] == json!("done") && e.get("error").is_some())
        {
            fatal += 1;
        }
        let want = json!({
            "name": want_row.name,
            "status": want_row.status,
            "body": want_row.body,
            "events": want_row.events,
            "calls": want_row.calls,
            "logLines": want_row.log_lines,
        });
        let ((g, g_reached), (w, w_reached)) = (normalize(got, "v5"), normalize(want, "v4"));
        // The validation pin must see the refusal DISAPPEAR too: both sides
        // reach (or skip) the validation step together, per case (the §3
        // review's catch at the `2f4254b42` unification — a v5 that stopped
        // emitting its pair used to compare EQUAL after the strip).
        assert_eq!(
            g_reached, w_reached,
            "case {}: reached validation on one side only (v5={g_reached}, v4={w_reached})",
            c.name
        );
        if g != w {
            failed.push(format!("{}:\n{}", c.name, first_diff(&g, &w)));
        }
    }
    assert!(
        model_calls >= 100,
        "the oracle recorded only {model_calls} model calls"
    );
    assert!(
        frames >= 300,
        "the oracle recorded only {frames} progress frames"
    );
    assert!(
        validated >= 12,
        "the validation pin was exercised only {validated} times"
    );
    assert!(
        refusals >= 2,
        "the oracle recorded only {refusals} route refusals"
    );
    assert!(fatal >= 4, "the oracle recorded only {fatal} fatal runs");
    eprintln!(
        "ai_import_tier3_equivalence: {} cases, {model_calls} model calls, {frames} frames, {validated} validation pins, {refusals} refusals, {fatal} fatal runs",
        corpus.cases.len()
    );
    assert!(
        failed.is_empty(),
        "{} case(s) DIFFER:\n{}",
        failed.len(),
        failed.join("\n")
    );
}
