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
//! ## The validation / repair steps (P4.86)
//!
//! Both are ported, so the frames, the five `[AIImport]` log lines and the
//! whole repair loop compare as PLAIN EQUALITIES. P4.9K2's
//! `VALIDATION_UNAVAILABLE` pins are retired. What survives is the engine
//! divergence underneath, and it is carried rather than hidden:
//!
//! * ajv reports one extra root error (`/: must match "then" schema`) per
//!   failed `allOf[i].then` branch, which the `jsonschema` crate does not.
//!   So every ERROR COUNT differs by that much. The counts reach five places
//!   — the `step_error validation` frame, `errors.validation`, and the
//!   `errorCount` / `remainingErrors` bags of three log lines — and each is
//!   rewritten to `<n>` for the byte diff while BOTH sides' raw values are
//!   collected and asserted against the committed
//!   [`V4_VALIDATION_COUNTS`] / [`V5_VALIDATION_COUNTS`] tables. A count
//!   moving on either side reddens.
//! * the error STRINGS themselves (the two log bags' `slice(0, 5)` and the
//!   repair prompt's `errors.join('\n')`) differ in wording as well, so they
//!   are canonicalized to the sorted SET of instance PATHS with ajv's wrapper
//!   struck — the comparand `qtap_schema_validate_equivalence` proves equal
//!   row by row. The rest of the repair prompt (v4's template, the
//!   `JSON.stringify(sectionsToRepair, null, 2)` block, the section-key list)
//!   is compared as BYTES.
//!
//! Both sides must reach (or skip) the validation step together — the
//! `2f4254b42` §3 review's assert, kept.
//!
//! ## The SSE bytes and the `logLLMCall` calls (P4.86, K2's tier-2 items 9/10)
//!
//! * **`rawSse`** — v4's route answers `data: ${JSON.stringify(event)}\n\n`
//!   per frame and closes. The oracle now emits those exact bytes, and this
//!   family re-frames v5's OWN event stream the same way and compares them
//!   BYTE FOR BYTE (after the shared `<minted-N>` remap and the `appVersion`
//!   normalization, which the `done` frame carries into the stream). What that
//!   does NOT cover is the three response HEADERS on the `ai-import-stream`
//!   edge, which live in `quilltap-web` — see the lane record's deferral.
//! * **`llmLogCalls`** — v4 gates its `logLLMCall` on
//!   `options.userId && options.profileProvider` and passes `type:
//!   'AI_IMPORT'`. The oracle records every call's `{type, provider,
//!   modelName}`; this family asserts v5's `LOG_TYPE_AI_IMPORT` constant
//!   equals the one `type` v4 uses and that the per-case COUNT equals the
//!   number of v5 model calls that returned content. Comparing the ROWS
//!   themselves would need an llm-logs partition this family's fixture set
//!   does not carry — deferred loudly in the lane record.
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

mod common;

use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use quilltap_core::api::generators_wizard::{
    ai_import_stream, AiImportDriverRequest, GeneratorsWizardDriver, GeneratorsWizardFuture,
    WizardDriverRequest,
};
use quilltap_core::api::types::{ErrorKind, Event, Response};
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::generators::ai_import::run_ai_import_streaming;
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
/// v4's ROOT `package.json` version, which its `ai-import.service.ts` stamps
/// into the manifest. v4 bumps it on EVERY commit, so this constant moves with
/// every regen — that is by design: the assert proves the stamp reached the
/// manifest on both sides before the value is normalized away.
const V4_APP_VERSION: &str = "4.10.0-dev.5";

/// Every engine-dependent count v4 (ajv) puts on the wire, in encounter order.
/// See the module header: the entries that differ from [`V5_VALIDATION_COUNTS`]
/// differ by ajv's `if/then` root wrapper, and by nothing else.
const V4_VALIDATION_COUNTS: &[&str] = &[
    "validation_repair_succeeds/frame = 3",
    "validation_repair_succeeds/warnErrorCount = 3",
    "validation_repair_fails_then_succeeds/frame = 3",
    "validation_repair_fails_then_succeeds/warnErrorCount = 3",
    "validation_repair_fails_then_succeeds/remaining#1 = 11",
    "validation_repair_fails_twice/frame = 3",
    "validation_repair_fails_twice/errorsValidation = 3",
    "validation_repair_fails_twice/warnErrorCount = 3",
    "validation_repair_fails_twice/remaining#1 = 11",
    "validation_repair_fails_twice/remaining#2 = 11",
    "validation_repair_fails_twice/couldNotRepairCount = 3",
    "validation_no_repairable_sections/frame = 1",
    "validation_no_repairable_sections/errorsValidation = 1",
    "validation_no_repairable_sections/warnErrorCount = 1",
    "validation_no_repairable_sections/couldNotRepairCount = 1",
    "validation_repair_call_throws/frame = 3",
    "validation_repair_call_throws/errorsValidation = 3",
    "validation_repair_call_throws/warnErrorCount = 3",
    "validation_repair_call_throws/couldNotRepairCount = 3",
    "validation_repair_reply_unparseable/frame = 3",
    "validation_repair_reply_unparseable/warnErrorCount = 3",
    "validation_repair_reply_omits_section/frame = 3",
    "validation_repair_reply_omits_section/errorsValidation = 3",
    "validation_repair_reply_omits_section/warnErrorCount = 3",
    "validation_repair_reply_omits_section/remaining#1 = 3",
    "validation_repair_reply_omits_section/remaining#2 = 3",
    "validation_repair_reply_omits_section/couldNotRepairCount = 3",
];

/// The same slots as v5 (the `jsonschema` crate) reports them.
const V5_VALIDATION_COUNTS: &[&str] = &[
    "validation_repair_succeeds/frame = 2",
    "validation_repair_succeeds/warnErrorCount = 2",
    "validation_repair_fails_then_succeeds/frame = 2",
    "validation_repair_fails_then_succeeds/warnErrorCount = 2",
    "validation_repair_fails_then_succeeds/remaining#1 = 10",
    "validation_repair_fails_twice/frame = 2",
    "validation_repair_fails_twice/errorsValidation = 2",
    "validation_repair_fails_twice/warnErrorCount = 2",
    "validation_repair_fails_twice/remaining#1 = 10",
    "validation_repair_fails_twice/remaining#2 = 10",
    "validation_repair_fails_twice/couldNotRepairCount = 2",
    "validation_no_repairable_sections/frame = 1",
    "validation_no_repairable_sections/errorsValidation = 1",
    "validation_no_repairable_sections/warnErrorCount = 1",
    "validation_no_repairable_sections/couldNotRepairCount = 1",
    "validation_repair_call_throws/frame = 2",
    "validation_repair_call_throws/errorsValidation = 2",
    "validation_repair_call_throws/warnErrorCount = 2",
    "validation_repair_call_throws/couldNotRepairCount = 2",
    "validation_repair_reply_unparseable/frame = 2",
    "validation_repair_reply_unparseable/warnErrorCount = 2",
    "validation_repair_reply_omits_section/frame = 2",
    "validation_repair_reply_omits_section/errorsValidation = 2",
    "validation_repair_reply_omits_section/warnErrorCount = 2",
    "validation_repair_reply_omits_section/remaining#1 = 2",
    "validation_repair_reply_omits_section/remaining#2 = 2",
    "validation_repair_reply_omits_section/couldNotRepairCount = 2",
];

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
    /// v4's EXACT response bytes for a `text/event-stream` answer (`null` for
    /// the two 400s) — P4.86 tier-2 item 9's framing half.
    raw_sse: Option<String>,
    events: Vec<Value>,
    calls: Vec<Value>,
    log_lines: Vec<Value>,
    /// v4's REAL `logLLMCall` arguments, recorded (P4.86 tier-2 item 10).
    llm_log_calls: Vec<Value>,
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
    /// Whether each recorded call answered non-empty content — v4 logs an
    /// `AI_IMPORT` row only AFTER its `if (!response?.content) throw` (P4.86).
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
    // A fresh llm-logs partition per case (the §3 unification review of the
    // generator follow-ups round): item 10's `llmLogCalls` compares the rows
    // v5's OWN `log_llm_call` wrote against v4's recorded `logLLMCall`
    // arguments — which is impossible with the `None` this family used to
    // pass (the v5 leg was a harness-side derivation from the scripted
    // provider, so it never observed the port). P4.85's shape.
    let llm_logs = scratch.join("llmlogs.db");
    common::materialize_llm_logs(&llm_logs, &spec.test_pepper_base64);
    let db = Db::open(
        DbPaths {
            main,
            mount_index: Some(mount),
            llm_logs: Some(llm_logs),
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

/// ajv's root wrapper for a failed `allOf[i].then` branch (the whole measured
/// engine divergence — `qtap_schema_validate_equivalence` grades it).
const AJV_THEN_WRAPPER: &str = "must match \"then\" schema";

/// Whether the run reached step 9 at all (the `2f4254b42` §3 assert: a v5 that
/// stopped emitting the pair must not compare equal to a v4 that emitted it).
fn reached_validation(events: &[Value]) -> bool {
    events
        .iter()
        .any(|e| e["type"] == json!("step_start") && e["step"] == json!("validation"))
}

/// An error list (`errors.slice(0, 5)`, or the repair prompt's
/// `errors.join('\n')`) canonicalized to the comparand the two engines agree
/// on: ajv's `if/then` wrapper struck, each remaining error reduced to its
/// instance PATH, sorted and deduplicated.
fn canon_error_paths(errors: &[String]) -> Vec<String> {
    let mut paths: Vec<String> = errors
        .iter()
        .filter_map(|e| match e.split_once(": ") {
            Some((path, message)) if message != AJV_THEN_WRAPPER => Some(path.to_string()),
            Some(_) => None,
            None => Some(e.clone()),
        })
        .collect();
    paths.sort();
    paths.dedup();
    paths
}

fn canon_error_paths_value(v: &Value) -> Value {
    let list: Vec<String> = v
        .as_array()
        .map(|a| a.iter().map(to_plain_string).collect())
        .unwrap_or_default();
    json!(canon_error_paths(&list))
}

fn to_plain_string(v: &Value) -> String {
    v.as_str()
        .map(str::to_string)
        .unwrap_or_else(|| v.to_string())
}

/// The repair prompt's error block canonicalized in place; everything else in
/// v4's template (the `JSON.stringify(sectionsToRepair, null, 2)` block and
/// the section-key list) is left as BYTES.
fn canon_repair_prompt(text: &str) -> Option<String> {
    const HEAD: &str = "Validation errors:\n";
    const TAIL: &str = "\n\nCurrent data (sections with errors only):";
    if !text.contains("The following .qtap export data has validation errors.") {
        return None;
    }
    let head_at = text.find(HEAD)? + HEAD.len();
    let tail_at = text[head_at..].find(TAIL)? + head_at;
    let block: Vec<String> = text[head_at..tail_at]
        .split('\n')
        .map(str::to_string)
        .collect();
    let canon = canon_error_paths(&block).join("\n");
    Some(format!("{}{canon}{}", &text[..head_at], &text[tail_at..]))
}

/// One observed count, as `<case>/<slot> = <n>` — collected per side and
/// asserted against the committed tables (the module header).
fn count_row(case: &str, slot: &str, n: u64) -> String {
    format!("{case}/{slot} = {n}")
}

/// Rewrite every place an engine-dependent COUNT or error-string list reaches
/// the wire, collecting the raw values as it goes.
fn canon_validation_counts(v: &mut Value, case: &str, counts: &mut Vec<String>) {
    let frame_re = Regex::new(r"(\d+) validation error\(s\)").unwrap();
    let sentence_re =
        Regex::new(r"Validation has (\d+) error\(s\) that could not be auto-repaired").unwrap();

    if let Some(events) = v.get_mut("events").and_then(Value::as_array_mut) {
        for e in events.iter_mut() {
            if e["type"] == json!("step_error") && e["step"] == json!("validation") {
                let text = to_plain_string(&e["error"]);
                // The frame's error is EXACTLY that sentence; the regex is
                // un-anchored so it can also rewrite `rawSse` below.
                if let Some(c) = frame_re.captures(&text) {
                    assert_eq!(text, format!("{} validation error(s)", &c[1]), "{case}");
                    counts.push(count_row(case, "frame", c[1].parse().unwrap()));
                    e["error"] = json!("<n> validation error(s)");
                }
            }
        }
        if let Some(done) = events.last_mut() {
            if let Some(text) = done.pointer("/errors/validation").map(to_plain_string) {
                if let Some(c) = sentence_re.captures(&text) {
                    assert_eq!(
                        text,
                        format!(
                            "Validation has {} error(s) that could not be auto-repaired",
                            &c[1]
                        ),
                        "{case}"
                    );
                    counts.push(count_row(case, "errorsValidation", c[1].parse().unwrap()));
                    done["errors"]["validation"] =
                        json!("Validation has <n> error(s) that could not be auto-repaired");
                }
            }
        }
    }

    if let Some(lines) = v.get_mut("logLines").and_then(Value::as_array_mut) {
        let mut attempt = 0usize;
        for l in lines.iter_mut() {
            let message = to_plain_string(&l["message"]);
            match message.as_str() {
                "[AIImport] Validation failed, attempting repair" => {
                    let n = l["context"]["errorCount"].as_u64().expect("errorCount");
                    counts.push(count_row(case, "warnErrorCount", n));
                    l["context"]["errorCount"] = json!("<n>");
                    l["context"]["errors"] = canon_error_paths_value(&l["context"]["errors"]);
                }
                "[AIImport] No repairable sections identified from error paths" => {
                    l["context"]["errors"] = canon_error_paths_value(&l["context"]["errors"]);
                }
                "[AIImport] Repair attempt failed" => {
                    attempt += 1;
                    let n = l["context"]["remainingErrors"]
                        .as_u64()
                        .expect("remainingErrors");
                    counts.push(count_row(case, &format!("remaining#{attempt}"), n));
                    l["context"]["remainingErrors"] = json!("<n>");
                }
                "[AIImport] Could not fully repair validation errors" => {
                    let n = l["context"]["errorCount"].as_u64().expect("errorCount");
                    counts.push(count_row(case, "couldNotRepairCount", n));
                    l["context"]["errorCount"] = json!("<n>");
                }
                _ => {}
            }
        }
    }

    // The SAME rewrites inside `rawSse`, which carries the frames as TEXT and
    // so is not reached by the walks above.
    if let Some(Value::String(raw)) = v.get_mut("rawSse") {
        let rewritten = frame_re
            .replace_all(raw, "<n> validation error(s)")
            .into_owned();
        let rewritten = sentence_re
            .replace_all(
                &rewritten,
                "Validation has <n> error(s) that could not be auto-repaired",
            )
            .into_owned();
        *raw = rewritten;
    }

    if let Some(calls) = v.get_mut("calls").and_then(Value::as_array_mut) {
        for call in calls.iter_mut() {
            let Some(messages) = call.get_mut("messages").and_then(Value::as_array_mut) else {
                continue;
            };
            for m in messages.iter_mut() {
                let text = to_plain_string(&m["content"]);
                if let Some(canon) = canon_repair_prompt(&text) {
                    m["content"] = json!(canon);
                }
            }
        }
    }
}

fn normalize(mut v: Value, side: &str, case: &str, counts: &mut Vec<String>) -> (String, bool) {
    let app_version = if side == "v4" {
        V4_APP_VERSION
    } else {
        V5_APP_VERSION
    };
    canon_numbers(&mut v);
    canon_validation_counts(&mut v, case, counts);
    // `rawSse` carries the `done` frame as TEXT, so the manifest's version
    // stamp is inside the string too.
    if let Some(Value::String(raw)) = v.get_mut("rawSse") {
        *raw = raw.replace(app_version, "<app-version>");
    }
    let mut reached = false;
    if let Some(events) = v.get_mut("events").and_then(Value::as_array_mut) {
        reached = reached_validation(events);
        if let Some(done) = events.last_mut() {
            if let Some(app) = done.pointer_mut("/result/manifest/appVersion") {
                assert_eq!(app, &json!(app_version), "{side}'s appVersion stamp moved");
                *app = Value::String("<app-version>".into());
            }
        }
    }
    let serialized = serde_json::to_string_pretty(&sorted(&v)).unwrap();
    (remap_minted(&serialized), reached)
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
    // v4's SSE framing, re-created from v5's OWN event stream: `data:
    // ${JSON.stringify(event)}\n\n` per frame, nothing else, nothing after
    // the last (the route closes the controller). A refusal answers no stream.
    let raw_sse: Value = if status == 200 {
        Value::String(
            events
                .iter()
                .map(|e| format!("data: {}\n\n", serde_json::to_string(e).unwrap()))
                .collect::<String>(),
        )
    } else {
        Value::Null
    };
    // v4 `callLLM`: one `logLLMCall({type: 'AI_IMPORT', provider, modelName})`
    // per call that RETURNED content, gated on a truthy userId + provider.
    // The v5 leg is what `log_llm_call` actually WROTE to this case's fresh
    // llm-logs partition, in insert order — never a derivation from the
    // scripted provider (the §3 unification review's catch: a derived leg
    // measured the harness's model of v4's gate, not the port's).
    let llm_log_calls: Vec<Value> = db
        .read_llm_logs(|conn| {
            let mut stmt =
                conn.prepare("SELECT type, provider, modelName FROM llm_logs ORDER BY rowid")?;
            let rows = stmt.query_map([], |r| {
                Ok(json!({
                    "type": r.get::<_, String>(0)?,
                    "provider": r.get::<_, String>(1)?,
                    "modelName": r.get::<_, String>(2)?,
                }))
            })?;
            rows.collect::<Result<Vec<Value>, _>>().map_err(Into::into)
        })
        .expect("read the case's llm_logs rows");

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
        "rawSse": raw_sse,
        "events": events,
        "calls": calls,
        "logLines": log_lines,
        "llmLogCalls": llm_log_calls,
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
    let (mut v4_counts, mut v5_counts): (Vec<String>, Vec<String>) = (Vec::new(), Vec::new());
    let (mut model_calls, mut frames, mut validated, mut refusals, mut fatal) =
        (0usize, 0usize, 0usize, 0usize, 0usize);
    let (mut repair_attempts, mut repair_successes) = (0usize, 0usize);
    let (mut streamed_rows, mut logged_calls) = (0usize, 0usize);
    let mut v4_log_types: BTreeSet<&str> = BTreeSet::new();
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
        streamed_rows += usize::from(want_row.raw_sse.is_some());
        logged_calls += want_row.llm_log_calls.len();
        for call in &want_row.llm_log_calls {
            v4_log_types.insert(call["type"].as_str().expect("a `type` string"));
        }
        repair_attempts += want_row
            .events
            .iter()
            .filter(|e| e["type"] == json!("step_start") && e["step"] == json!("repair"))
            .count();
        repair_successes += want_row
            .events
            .iter()
            .filter(|e| e["type"] == json!("step_complete") && e["step"] == json!("repair"))
            .count();
        let want = json!({
            "name": want_row.name,
            "status": want_row.status,
            "body": want_row.body,
            "rawSse": want_row.raw_sse,
            "events": want_row.events,
            "calls": want_row.calls,
            "logLines": want_row.log_lines,
            "llmLogCalls": want_row.llm_log_calls,
        });
        let ((g, g_reached), (w, w_reached)) = (
            normalize(got, "v5", &c.name, &mut v5_counts),
            normalize(want, "v4", &c.name, &mut v4_counts),
        );
        // Both sides reach (or skip) the validation step together, per case
        // (the §3 review's catch at the `2f4254b42` unification — a v5 that
        // stopped emitting its frames used to compare EQUAL after the strip).
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
        "only {validated} runs reached `step_complete validation`"
    );
    assert!(
        repair_attempts >= 12,
        "the repair loop was entered only {repair_attempts} times"
    );
    assert!(
        repair_successes >= 3,
        "only {repair_successes} runs reached `Repair successful`"
    );
    assert!(
        streamed_rows >= 30,
        "only {streamed_rows} rows carried v4's raw SSE bytes"
    );
    assert!(
        logged_calls >= 150,
        "the oracle recorded only {logged_calls} `logLLMCall` calls"
    );
    assert_eq!(
        v4_log_types,
        BTreeSet::from(["AI_IMPORT"]),
        "v4's `logLLMCall` type for the import path moved"
    );
    assert!(
        refusals >= 2,
        "the oracle recorded only {refusals} route refusals"
    );
    assert!(fatal >= 4, "the oracle recorded only {fatal} fatal runs");
    eprintln!(
        "ai_import_tier3_equivalence: {} cases, {model_calls} model calls, {frames} frames, \
         {validated} validations passed, {repair_attempts} repair attempts \
         ({repair_successes} successful), {refusals} refusals, {fatal} fatal runs, \
         {streamed_rows} streamed rows, {logged_calls} AI_IMPORT log calls",
        corpus.cases.len()
    );
    // The RECORDED engine-count divergence, printed on every run.
    eprintln!("  validation counts — v4 (ajv) vs v5 (jsonschema):");
    for (a, b) in v4_counts.iter().zip(v5_counts.iter()) {
        let mark = if a == b { " " } else { "≠" };
        eprintln!("   {mark} v4 {a:<58} v5 {b}");
    }
    assert!(
        failed.is_empty(),
        "{} case(s) DIFFER:\n{}",
        failed.len(),
        failed.join("\n")
    );

    assert_eq!(
        v4_counts, V4_VALIDATION_COUNTS,
        "v4's validation counts moved — regenerate, re-measure, and update the table"
    );
    assert_eq!(
        v5_counts, V5_VALIDATION_COUNTS,
        "v5's validation counts moved — the engine or the port changed"
    );
}
