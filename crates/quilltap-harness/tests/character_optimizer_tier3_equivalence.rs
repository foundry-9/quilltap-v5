//! P4.9K1 tier-3 differential: `api::generators_detail::character_optimize`
//! over `generators::optimizer::run_character_optimizer` (v4
//! `characters/[id]/handlers/post.ts` `optimize-stream` →
//! `lib/services/character-optimizer.service.ts`, the POST-bug-119 shape) vs
//! v4's REAL route, the model + embedding boundaries canned on both sides and
//! the clock frozen at the corpus `frozenNowMs`.
//!
//! Both sides run every corpus case on a FRESH copy of the committed
//! `character-generators-{main,mount}.db` pair and diff:
//!   * the refusal arms' status + body (the 404, the Zod `details`);
//!   * the WHOLE progress-event trace — v4's SSE frames decoded, v5's
//!     `Event::GeneratorProgress` frames drained off the engine broadcast —
//!     with every suggestion `id` normalized to `<id>` (both sides mint
//!     random UUIDs);
//!   * every recorded model call (provider / baseUrl / model / temperature /
//!     maxTokens / cacheKey / profileParameters / the whole message list —
//!     the assembled prompts are the unit, compared as bytes);
//!   * the vault's `Suggestions/` files after the run (path + content, the
//!     frozen stamp included);
//!   * the `[CharacterOptimizer]` log lines — level, message and the whole
//!     context bag — captured by the shared tracing rig on this side and by a
//!     `logger` spy on v4's; the two bug-119 containment lines among them.
//!
//! Not compared: the api key v4 hands `sendMessage` and the two
//! `CHARACTER_OPTIMIZER` `llm_logs` rows (the committed pair carries no
//! llm-logs partition on either side) — recorded in the lane record. The
//! dispatch's `{ terminal }` payload is asserted against the last drained
//! frame instead of v4 (v4's route streams and carries no body).
//!
//! Regenerate the oracle (Node 24, from the v4 checkout — see the .ts header):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   V5W=${V5W:-$HOME/source/quilltap-v5}
//!   TMPO=/tmp/qt-character-optimizer-oracle
//!   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
//!   cp "$V5W/harness/oracle/cases/character-optimizer-tier3.test.ts" "$TMPO/cases/"
//!   cp "$V5W/harness/oracle/fixtures/character-generators.json" "$TMPO/fixtures/"
//!   cp "$V5W/harness/oracle/fixtures/character-optimizer-tier3.json" "$TMPO/fixtures/"
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_CG_MAIN=$V5W/crates/quilltap-web/tests/fixtures/character-generators-main.db \
//!   QT_FIXTURE_CG_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/character-generators-mount.db \
//!   QT_ORACLE_OUT=/tmp/oracle-character-optimizer.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=300000 \
//!       --roots "$PWD" --roots "$TMPO/cases" -- character-optimizer-tier3
//! Run:
//!   QT_ORACLE_CHARACTER_OPTIMIZER=/tmp/oracle-character-optimizer.ndjson \
//!     cargo test -p quilltap-harness --test character_optimizer_tier3_equivalence -- --nocapture
//!
//! While v4 HEAD is past the baseline the regen needs a worktree pinned at
//! the baseline — the sweep driver's job (`recipe_sweep.py --run
//! character_optimizer_tier3_equivalence --v4 <pin>`), never a path baked in here.

use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use quilltap_core::api::generators_detail::{
    character_optimize, ExternalPromptDriverRequest, GeneratorsDetailDriver,
    GeneratorsDetailFuture, OptimizeDriverRequest,
};
use quilltap_core::api::types::{ErrorKind, Event, Response};
use quilltap_core::db::characters_read;
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::db::DbError;
use quilltap_core::generators::external_prompt::ExternalPromptResult;
use quilltap_core::generators::optimizer::{run_character_optimizer, OnProgress};
use quilltap_core::model::completion::{
    CannedCompletionProvider, CompletionError, CompletionParams, CompletionProvider,
    CompletionResponse,
};
use quilltap_core::model::embedding::{
    EmbeddingError, EmbeddingPriority, EmbeddingProvider, EmbeddingResult,
};
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::sync::broadcast;

// P4.85 item 10: `materialize_llm_logs` — an llm-logs partition beside the
// fixture pair, so the runners' `logLLMCall` twin has somewhere to write.
mod common;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    test_pepper_base64: String,
    user_id: String,
}

/// A scripted call: a corpus block name / literal content, or an object
/// carrying `content` or `throws`.
#[derive(Deserialize, Clone)]
#[serde(untagged)]
enum CallSpec {
    Named(String),
    Shaped {
        #[serde(default)]
        content: Option<String>,
        #[serde(default)]
        throws: Option<String>,
    },
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Case {
    name: String,
    character_id: String,
    body: Value,
    calls: Vec<CallSpec>,
    #[serde(default)]
    embeddings: HashMap<String, Vec<f32>>,
    #[serde(default)]
    embedding_throws: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Corpus {
    frozen_now_ms: i64,
    analysis: String,
    sub_steps: HashMap<String, String>,
    cases: Vec<Case>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OracleRow {
    name: String,
    status: i64,
    body: Value,
    /// P4.85 item 9 — the response body VERBATIM (`null` when v4 answered a
    /// JSON refusal rather than a stream).
    raw_sse: Option<String>,
    events: Vec<Value>,
    calls: Vec<Value>,
    suggestions_files: Vec<Value>,
    log_lines: Vec<Value>,
    /// P4.85 item 10 — `{type: count}`, zeroes dropped.
    llm_log_counts: Value,
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

/// Resolve a scripted call the way the oracle does: a named corpus block, a
/// literal JSON string, or an object.
fn resolve_call(corpus: &Corpus, c: &CallSpec) -> (Option<String>, Option<String>) {
    match c {
        CallSpec::Named(s) => {
            if s == "analysis" {
                (Some(corpus.analysis.clone()), None)
            } else if let Some(block) = corpus.sub_steps.get(s) {
                (Some(block.clone()), None)
            } else {
                (Some(s.clone()), None)
            }
        }
        CallSpec::Shaped { content, throws } => (content.clone(), throws.clone()),
    }
}

/// The model seam: scripted BY CALL INDEX (the oracle's shape), recording
/// every request as the oracle records it. The answer is served through a
/// one-entry `CannedCompletionProvider` keyed on the incoming call so the
/// response type stays the production one.
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
            "cacheKey": params.cache_key,
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

/// The embedding seam: the case's canned vector by exact query text, a
/// case-wide throw, or the oracle's own "no canned embedding" failure.
struct ScriptedEmbedding {
    vectors: HashMap<String, Vec<f32>>,
    throws: bool,
}

impl EmbeddingProvider for ScriptedEmbedding {
    fn generate_embedding_for_user(
        &self,
        text: &str,
        _user_id: &str,
        _profile_id: Option<&str>,
        _priority: EmbeddingPriority,
    ) -> impl std::future::Future<Output = Result<EmbeddingResult, EmbeddingError>> + Send {
        let result = if self.throws {
            Err(EmbeddingError::new("canned embedding failure"))
        } else {
            match self.vectors.get(text) {
                Some(v) => Ok(EmbeddingResult {
                    embedding: v.clone(),
                    model: "canned".to_string(),
                    dimensions: v.len(),
                    provider: "canned".to_string(),
                }),
                None => Err(EmbeddingError::new(format!(
                    "no canned embedding registered for input ({} chars)",
                    text.encode_utf16().count()
                ))),
            }
        };
        async move { result }
    }
}

/// The test's driver: the same composition the host builds — the runner over
/// the `Db` + the two providers.
struct TestDriver {
    db: Db,
    completion: Arc<ScriptedProvider>,
    embedding: Arc<ScriptedEmbedding>,
}

impl GeneratorsDetailDriver for TestDriver {
    fn external_prompt<'a>(
        &'a self,
        _req: ExternalPromptDriverRequest,
    ) -> GeneratorsDetailFuture<'a, Result<ExternalPromptResult, DbError>> {
        Box::pin(async { panic!("the optimizer differential never generates an external prompt") })
    }

    fn optimize<'a>(
        &'a self,
        req: OptimizeDriverRequest,
        on_progress: OnProgress<'a>,
    ) -> GeneratorsDetailFuture<'a, ()> {
        Box::pin(async move {
            run_character_optimizer(
                &self.db,
                &*self.completion,
                &*self.embedding,
                &req.character_id,
                &req.connection_profile_id,
                &req.user_id,
                on_progress,
                &req.options,
                req.now_ms,
            )
            .await
        })
    }
}

fn fresh_pair(spec: &Spec, tag: &str) -> (Db, PathBuf) {
    let scratch = std::env::temp_dir().join(format!("qt-opt-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    let main = scratch.join("main.db");
    let mount = scratch.join("mount.db");
    std::fs::copy(fixtures_dir().join("character-generators-main.db"), &main).unwrap();
    std::fs::copy(fixtures_dir().join("character-generators-mount.db"), &mount).unwrap();
    // A fresh llm-logs partition per case: item 10 compares what each runner's
    // `log_llm_call` wrote against the oracle's own per-case delta, which is
    // impossible with the `None` this family used to pass.
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

/// The oracle's `SUGGESTIONS_SQL` over the mount partition.
/// **P4.85 item 10 — the `llm_logs` rows this case wrote.**
///
/// The oracle dumps `SELECT type, COUNT(*) … GROUP BY type` off its scratch
/// llm-logs partition as a per-case DELTA; v5 opens a FRESH one per case, so
/// the totals ARE the delta. Zero counts are dropped on both sides, and v4
/// creates the table lazily on its first write, so a case that logged nothing
/// answers `{}` either way.
fn llm_log_counts(db: &Db) -> Value {
    db.read_llm_logs(|c| {
        let present: i64 = c.query_row(
            "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='llm_logs'",
            [],
            |r| r.get(0),
        )?;
        if present == 0 {
            return Ok(json!({}));
        }
        let mut stmt =
            c.prepare("SELECT type, COUNT(*) AS n FROM llm_logs GROUP BY type ORDER BY type")?;
        let mut out = serde_json::Map::new();
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?;
        for row in rows {
            let (t, n) = row?;
            if n != 0 {
                out.insert(t, json!(n));
            }
        }
        Ok(Value::Object(out))
    })
    .expect("read the llm-logs counts")
}

fn suggestions_files(db: &Db, character_id: &str) -> Vec<Value> {
    let cid = character_id.to_string();
    let mount_id = db
        .read_main(move |c| characters_read::find_by_id_raw(c, &cid))
        .unwrap()
        .and_then(|c| {
            c.get("characterDocumentMountPointId")
                .and_then(Value::as_str)
                .map(str::to_string)
        });
    let Some(mount_id) = mount_id else {
        return Vec::new();
    };
    db.read_mount_index(move |c| {
        let mut stmt = c.prepare(
            "SELECT l.relativePath AS relativePath, d.content AS content FROM doc_mount_file_links l \
             JOIN doc_mount_documents d ON d.fileId = l.fileId \
             WHERE l.mountPointId = ?1 AND l.relativePath LIKE 'Suggestions/%' ORDER BY l.relativePath",
        )?;
        let rows = stmt.query_map([&mount_id], |r| {
            Ok(json!({
                "relativePath": r.get::<_, String>(0)?,
                "content": r.get::<_, String>(1)?,
            }))
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    })
    .unwrap()
}

/// One captured `[CharacterOptimizer]` line → the oracle's `{level, message,
/// context}` row. The rig renders `LEVEL target message context=<json>…` (a
/// literal message is `format_args!`, whose Debug is its Display — no
/// quotes); the context JSON is read as the first complete value after
/// `context=`.
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

/// Both sides mint a random UUID per surviving suggestion.
fn mask_suggestion_ids(v: &mut Value) {
    match v {
        Value::Array(a) => a.iter_mut().for_each(mask_suggestion_ids),
        Value::Object(o) => {
            for (k, x) in o.iter_mut() {
                if (k == "suggestions" || k == "partialSuggestions") && x.is_array() {
                    for s in x.as_array_mut().unwrap() {
                        if let Some(so) = s.as_object_mut() {
                            if so.contains_key("id") {
                                so.insert("id".to_string(), Value::String("<id>".into()));
                            }
                        }
                    }
                }
                mask_suggestion_ids(x);
            }
        }
        _ => {}
    }
}

fn normalize(mut v: Value) -> String {
    canon_numbers(&mut v);
    mask_suggestion_ids(&mut v);
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

/// Run one case: the handler over a fresh pair, the events drained off the
/// broadcast, the log lines captured on this thread (a current-thread runtime
/// so the thread-scoped rig sees every line).
fn run_case(spec: &Spec, corpus: &Corpus, c: &Case) -> Value {
    let (db, scratch) = fresh_pair(spec, &c.name);
    let completion = Arc::new(ScriptedProvider {
        script: c.calls.iter().map(|s| resolve_call(corpus, s)).collect(),
        next: Mutex::new(0),
        calls: Mutex::new(Vec::new()),
        case: c.name.clone(),
    });
    let embedding = Arc::new(ScriptedEmbedding {
        vectors: c.embeddings.clone(),
        throws: c.embedding_throws,
    });
    let driver: Arc<dyn GeneratorsDetailDriver> = Arc::new(TestDriver {
        db: db.clone(),
        completion: completion.clone(),
        embedding,
    });
    let (events_tx, mut events_rx) = broadcast::channel::<Event>(4096);
    let progress_id = format!("p-{}", c.name);
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    let field = |k: &str| c.body.get(k);
    let (response, lines) = quilltap_core::test_support::captured_with(|| {
        rt.block_on(character_optimize(
            &db,
            Some(&driver),
            &events_tx,
            &spec.user_id,
            &c.character_id,
            Some(&progress_id),
            field("connectionProfileId"),
            field("maxMemories"),
            field("searchQuery"),
            field("useSemanticSearch"),
            field("sinceDate"),
            field("beforeDate"),
            field("outputMode"),
            corpus.frozen_now_ms,
        ))
    });

    // Drain the frames published under this run's progress id.
    let mut events: Vec<Value> = Vec::new();
    while let Ok(ev) = events_rx.try_recv() {
        let v = serde_json::to_value(&ev).unwrap();
        if v["progressId"] == json!(progress_id) && v["type"] == json!("generatorProgress") {
            assert_eq!(v["generator"], json!("optimizer"));
            events.push(v["event"].clone());
        }
    }

    let (status, body) = to_status_body(response);
    // The 200 arm's dispatch body is `{ terminal }` — v4 streams instead, so
    // the terminal is asserted against the last frame and the comparand body
    // is `null` on both sides.
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

    let files = suggestions_files(&db, &c.character_id);
    let counts = llm_log_counts(&db);
    let log_lines: Vec<Value> = lines
        .iter()
        .filter(|l| l.contains("[CharacterOptimizer]"))
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
        "suggestionsFiles": files,
        "logLines": log_lines,
        "llmLogCounts": counts,
    })
}

#[test]
fn character_optimizer_matches_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_CHARACTER_OPTIMIZER") else {
        eprintln!("SKIP: set QT_ORACLE_CHARACTER_OPTIMIZER (see the test header).");
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
        &std::fs::read_to_string(oracle_dir().join("character-optimizer-tier3.json")).unwrap(),
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
    let mut model_calls = 0usize;
    let mut frames = 0usize;
    let mut files = 0usize;
    let mut log_rows = 0usize;
    for c in &corpus.cases {
        let want_row = &oracle[&c.name];
        let got = run_case(&spec, &corpus, c);
        model_calls += want_row.calls.len();
        frames += want_row.events.len();
        files += want_row.suggestions_files.len();
        log_rows += want_row.log_lines.len();
        let want = json!({
            "name": want_row.name,
            "status": want_row.status,
            "body": want_row.body,
            "events": want_row.events,
            "calls": want_row.calls,
            "suggestionsFiles": want_row.suggestions_files,
            "logLines": want_row.log_lines,
            "llmLogCounts": want_row.llm_log_counts,
        });
        let (g, w) = (normalize(got), normalize(want));
        if g != w {
            failed.push(format!("{}:\n{}", c.name, first_diff(&g, &w)));
        }
    }
    // Coverage by shape: the corpus must exercise every arm the family exists for.
    assert!(
        model_calls >= 150,
        "the oracle recorded only {model_calls} model calls"
    );
    assert!(
        frames >= 300,
        "the oracle recorded only {frames} progress frames"
    );
    assert!(
        files >= 2,
        "the oracle recorded only {files} suggestions files"
    );
    assert!(
        log_rows >= 50,
        "the oracle recorded only {log_rows} log lines"
    );
    let bug119 = oracle
        .values()
        .flat_map(|r| r.log_lines.iter())
        .filter(|l| {
            l["message"]
                == json!("[CharacterOptimizer] Sub-step answered with a non-array; coerced")
        })
        .count();
    assert!(
        bug119 >= 5,
        "bug 119's coerced arm was driven only {bug119} times"
    );
    eprintln!(
        "character_optimizer_tier3_equivalence: {} cases, {model_calls} model calls, {frames} frames, {files} files, {log_rows} log rows",
        corpus.cases.len()
    );
    assert!(
        failed.is_empty(),
        "{} case(s) DIFFER:\n{}",
        failed.len(),
        failed.join("\n")
    );
}

/// **P4.85 item 4 — the route-level `[Characters v1] Character optimizer
/// starting (streaming)` line.** v4 logs it at
/// `app/api/v1/characters/[id]/handlers/post.ts:101`, between
/// `optimizeStreamSchema.parse(body)` and the `ReadableStream` it then returns,
/// with nine keys: `userId`, `characterId`, `connectionProfileId`,
/// `maxMemories`, `searchQuery` (v4's `searchQuery || '(none)'`),
/// `useSemanticSearch`, `sinceDate`, `beforeDate`, `outputMode`.
///
/// The differential above cannot see it: it filters `[CharacterOptimizer]`,
/// which is the SERVICE's prefix, and this is the ROUTE's. The round record
/// called it "compared by nothing"; this is the pin.
///
/// No model call and no driver: with `driver: None` the handler logs, then
/// answers the named not-assembled refusal — so the line's position (after the
/// 404 and the Zod parse, before the driver) is what the arms below fix.
///
/// Mutation: drop any one field → its `contains` fails; move the line above the
/// Zod parse → the `bad body` silence arm fails; above `require_character` →
/// the 404 arm fails.
#[test]
fn the_optimizer_route_starting_line_carries_v4s_bag() {
    let Ok(spec_raw) = std::fs::read_to_string(oracle_dir().join("character-generators.json"))
    else {
        eprintln!("SKIP: the character-generators spec is missing.");
        return;
    };
    let spec: Spec = serde_json::from_str(&spec_raw).unwrap();
    const SENTENCE: &str = "[Characters v1] Character optimizer starting (streaming)";
    const MIRA: &str = "a2000002-0000-4000-8000-000000000001";
    const PROFILE: &str = "c0000002-0000-4000-8000-000000000001";
    const MISSING: &str = "00000000-0000-4000-8000-00000000dead";
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let (events_tx, _rx) = broadcast::channel::<Event>(16);

    // Every option left to its Zod default, so the line's `(none)` / `null`
    // renderings are exercised as v4 renders them.
    let (db, scratch) = fresh_pair(&spec, "route_line_defaults");
    let lines = quilltap_core::test_support::captured(|| {
        rt.block_on(character_optimize(
            &db,
            None,
            &events_tx,
            &spec.user_id,
            MIRA,
            Some("p-route-line"),
            Some(&json!(PROFILE)),
            None,
            None,
            None,
            None,
            None,
            None,
            1_700_000_000_000,
        ));
    });
    let line = lines
        .iter()
        .find(|l| l.contains(SENTENCE))
        .unwrap_or_else(|| panic!("the optimizer route line is SILENT: {lines:#?}"));
    assert!(line.starts_with("INFO "), "v4 logs at info: {line}");
    for field in [
        format!("user_id={}", spec.user_id),
        format!("character_id={MIRA}"),
        format!("connection_profile_id={PROFILE}"),
        // `maxMemories: z.number().int().min(5).max(200).optional().default(30)`
        "max_memories=30".to_string(),
        // v4's `searchQuery || '(none)'` — the default is `''`, which is falsy.
        "search_query=(none)".to_string(),
        "use_semantic_search=true".to_string(),
        // v4's bag carries the nulls; `logger` renders them, so v5 does too.
        "since_date=null".to_string(),
        "before_date=null".to_string(),
        "output_mode=apply".to_string(),
    ] {
        assert!(line.contains(&field), "missing {field} in {line}");
    }
    drop(db);
    let _ = std::fs::remove_dir_all(&scratch);

    // A body that DOES set them: the same line, carrying the real values.
    let (db, scratch) = fresh_pair(&spec, "route_line_explicit");
    let lines = quilltap_core::test_support::captured(|| {
        rt.block_on(character_optimize(
            &db,
            None,
            &events_tx,
            &spec.user_id,
            MIRA,
            Some("p-route-line-2"),
            Some(&json!(PROFILE)),
            Some(&json!(12)),
            Some(&json!("harbour")),
            Some(&json!(false)),
            Some(&json!("2026-01-01")),
            Some(&json!("2026-02-01")),
            Some(&json!("suggestions-file")),
            1_700_000_000_000,
        ));
    });
    let line = lines.iter().find(|l| l.contains(SENTENCE)).unwrap();
    for field in [
        "max_memories=12",
        "search_query=harbour",
        "use_semantic_search=false",
        "since_date=2026-01-01",
        "before_date=2026-02-01",
        "output_mode=suggestions-file",
    ] {
        assert!(line.contains(field), "missing {field} in {line}");
    }
    drop(db);
    let _ = std::fs::remove_dir_all(&scratch);

    // --- the SILENCE half: v4's two gates both come BEFORE the line. ---
    for (tag, character, profile) in [
        ("route_line_404", MISSING, json!(PROFILE)),
        ("route_line_badbody", MIRA, json!(7)),
    ] {
        let (db, scratch) = fresh_pair(&spec, tag);
        let lines = quilltap_core::test_support::captured(|| {
            rt.block_on(character_optimize(
                &db,
                None,
                &events_tx,
                &spec.user_id,
                character,
                Some("p-route-line-x"),
                Some(&profile),
                None,
                None,
                None,
                None,
                None,
                None,
                1_700_000_000_000,
            ));
        });
        assert!(
            !lines.iter().any(|l| l.contains(SENTENCE)),
            "{tag}: v4 refuses before it announces: {lines:#?}"
        );
        drop(db);
        let _ = std::fs::remove_dir_all(&scratch);
    }
}
