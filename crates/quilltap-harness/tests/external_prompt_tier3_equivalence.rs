//! P4.9K1 tier-3 differential: `api::generators_detail::
//! character_generate_external_prompt` over `generators::external_prompt` (v4
//! `characters/[id]/handlers/post.ts` `generate-external-prompt` →
//! `lib/services/external-prompt-generator.service.ts`) vs v4's REAL route,
//! the model boundary canned on both sides.
//!
//! Both sides run every corpus case on a FRESH copy of the committed
//! `characters-{main,mount}.db` pair with the case's seeds applied through the
//! ported repository twins, then diff the response (status + body, Zod
//! `details` included) AND the recorded model call(s): provider, baseUrl,
//! model, temperature, maxTokens, cacheKey, profileParameters and the whole
//! message list — the assembled USER MESSAGE is the unit, so it is compared as
//! bytes. The canned reply is registered on the Rust side from the ORACLE's
//! recorded key, so a v5 message that differs by one byte misses the canned
//! table and fails loudly rather than answering.
//!
//! Not compared: the api key v4 hands `sendMessage` (v5's provider seam
//! resolves its own — the tree-wide host key scan, P4.D93) and the
//! `EXTERNAL_PROMPT` `llm_logs` row (the committed pair carries no llm-logs
//! partition; v4 writes it to its scratch data dir) — both recorded in the
//! lane record.
//!
//! Regenerate the oracle (Node 24, from the v4 checkout — see the .ts header):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   V5W=${V5W:-$HOME/source/quilltap-v5}
//!   TMPO=/tmp/qt-external-prompt-oracle
//!   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
//!   cp "$V5W/harness/oracle/cases/external-prompt-tier3.test.ts" "$TMPO/cases/"
//!   cp "$V5W/harness/oracle/fixtures/characters.json" "$TMPO/fixtures/"
//!   cp "$V5W/harness/oracle/fixtures/external-prompt-tier3.json" "$TMPO/fixtures/"
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_CHARACTERS_MAIN=$V5W/crates/quilltap-web/tests/fixtures/characters-main.db \
//!   QT_FIXTURE_CHARACTERS_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/characters-mount.db \
//!   QT_ORACLE_OUT=/tmp/oracle-external-prompt.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=300000 \
//!       --roots "$PWD" --roots "$TMPO/cases" -- external-prompt-tier3
//! Run:
//!   QT_ORACLE_EXTERNAL_PROMPT=/tmp/oracle-external-prompt.ndjson \
//!     cargo test -p quilltap-harness --test external_prompt_tier3_equivalence -- --nocapture
//!
//! While v4 HEAD is past the baseline the regen needs a worktree pinned at
//! the baseline — the sweep driver's job (`recipe_sweep.py --run
//! external_prompt_tier3_equivalence --v4 <pin>`), never a path baked in here.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use quilltap_core::api::generators_detail::{
    character_generate_external_prompt, ExternalPromptDriverRequest, GeneratorsDetailDriver,
    GeneratorsDetailFuture, OptimizeDriverRequest,
};
use quilltap_core::api::types::{ErrorKind, Request, Response};
use quilltap_core::db::connection_profiles::{ConnectionProfilesRepository, CpUpdate};
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::db::vault_character_update::update_character;
use quilltap_core::db::DbError;
use quilltap_core::generators::external_prompt::{generate_external_prompt, ExternalPromptResult};
use quilltap_core::model::completion::{
    CannedCompletionProvider, CompletionError, CompletionMessage, CompletionParams,
    CompletionProvider, CompletionResponse, CompletionRole, CompletionUsage,
};
use serde::Deserialize;
use serde_json::{json, Value};

// P4.85 item 10: `materialize_llm_logs` — an llm-logs partition beside the
// fixture pair, so the runners' `logLLMCall` twin has somewhere to write.
mod common;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    test_pepper_base64: String,
    user_id: String,
}

#[derive(Deserialize, Clone)]
#[serde(tag = "kind")]
enum Seed {
    #[serde(rename = "characterPatch", rename_all = "camelCase")]
    CharacterPatch {
        character_id: String,
        patch: serde_json::Map<String, Value>,
    },
    #[serde(rename = "profilePatch", rename_all = "camelCase")]
    ProfilePatch {
        profile_id: String,
        patch: serde_json::Map<String, Value>,
    },
}

#[derive(Deserialize)]
#[serde(untagged)]
enum SeedRef {
    Named(String),
    Inline(Seed),
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Reply {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    usage: Option<Usage>,
    #[serde(default)]
    throws: Option<String>,
}

#[derive(Deserialize, Clone, Copy)]
#[serde(rename_all = "camelCase")]
struct Usage {
    prompt_tokens: i64,
    completion_tokens: i64,
    total_tokens: i64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Case {
    name: String,
    character_id: String,
    body: Value,
    #[serde(default)]
    seeds: Vec<SeedRef>,
    reply: Reply,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Corpus {
    rich_aria: Seed,
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct OracleRow {
    // (this row is NOT `rename_all = "camelCase"` — the new field says so itself)
    name: String,
    status: i64,
    body: Value,
    calls: Vec<Value>,
    /// P4.85 item 10 — `{type: count}`, zeroes dropped.
    #[serde(rename = "llmLogCounts")]
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

/// The recording seam: every call's params as the oracle records them, then
/// the canned answer keyed on the exact call.
struct RecordingProvider {
    inner: CannedCompletionProvider,
    calls: Mutex<Vec<Value>>,
}

impl CompletionProvider for RecordingProvider {
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
        let provider = provider.to_string();
        let base_url = base_url.map(str::to_string);
        let params = params.clone();
        let inner = self.inner.clone();
        async move {
            inner
                .send_message(&provider, base_url.as_deref(), &params)
                .await
        }
    }
}

/// The test's driver: the same composition the host builds — the runner over
/// the `Db` + a completion provider.
struct TestDriver {
    db: Db,
    provider: Arc<RecordingProvider>,
}

impl GeneratorsDetailDriver for TestDriver {
    fn external_prompt<'a>(
        &'a self,
        req: ExternalPromptDriverRequest,
    ) -> GeneratorsDetailFuture<'a, Result<ExternalPromptResult, DbError>> {
        Box::pin(async move {
            generate_external_prompt(
                &self.db,
                &*self.provider,
                &req.character_id,
                &req.request,
                &req.user_id,
            )
            .await
        })
    }

    fn optimize<'a>(
        &'a self,
        _req: OptimizeDriverRequest,
        _on_progress: quilltap_core::generators::optimizer::OnProgress<'a>,
    ) -> GeneratorsDetailFuture<'a, ()> {
        Box::pin(async { panic!("the external-prompt differential never runs the optimizer") })
    }
}

fn messages_from(call: &Value) -> Vec<CompletionMessage> {
    call["messages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| {
            let role = match m["role"].as_str().unwrap() {
                "system" => CompletionRole::System,
                "user" => CompletionRole::User,
                "assistant" => CompletionRole::Assistant,
                _ => CompletionRole::Tool,
            };
            CompletionMessage {
                role,
                content: m["content"].as_str().unwrap().to_string(),
            }
        })
        .collect()
}

/// Register the case's canned reply under the ORACLE's recorded call key.
fn canned_for(oracle: &OracleRow, reply: &Reply) -> CannedCompletionProvider {
    let mut canned = CannedCompletionProvider::new();
    for call in &oracle.calls {
        let provider = call["provider"].as_str().unwrap();
        let model = call["model"].as_str().unwrap();
        let temperature = call["temperature"].as_f64();
        let messages = messages_from(call);
        canned = match &reply.throws {
            Some(msg) => canned.with_failure(provider, model, temperature, &messages, msg.clone()),
            None => canned.with_response(
                provider,
                model,
                temperature,
                &messages,
                reply.content.clone().unwrap_or_default(),
                reply.usage.map(|u| CompletionUsage {
                    prompt_tokens: u.prompt_tokens,
                    completion_tokens: u.completion_tokens,
                    total_tokens: u.total_tokens,
                }),
            ),
        };
    }
    canned
}

fn fresh_pair(tag: &str) -> (Db, PathBuf, String) {
    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(oracle_dir().join("characters.json")).unwrap(),
    )
    .unwrap();
    let scratch = std::env::temp_dir().join(format!("qt-ep-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    let main = scratch.join("main.db");
    let mount = scratch.join("mount.db");
    std::fs::copy(fixtures_dir().join("characters-main.db"), &main).unwrap();
    std::fs::copy(fixtures_dir().join("characters-mount.db"), &mount).unwrap();
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
    (db, scratch, spec.user_id)
}

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

async fn apply_seed(db: &Db, seed: &Seed) {
    match seed {
        Seed::CharacterPatch {
            character_id,
            patch,
        } => {
            let cid = character_id.clone();
            let patch = patch.clone();
            db.write(move |w| {
                let mount = w
                    .mount_index()
                    .ok_or_else(|| DbError::Internal("no mount".into()))?
                    .connection();
                let main = w.main().connection();
                update_character(main, mount, &cid, &patch).map_err(|e| e.into_db())?;
                Ok(())
            })
            .await
            .expect("seed character patch");
        }
        Seed::ProfilePatch { profile_id, patch } => {
            let pid = profile_id.clone();
            let max_context = patch.get("maxContext").and_then(Value::as_f64);
            assert!(
                patch.len() == 1 && max_context.is_some(),
                "the profile seed carries maxContext only"
            );
            db.write(move |w| {
                ConnectionProfilesRepository::new(w.main().connection()).update(
                    &pid,
                    &CpUpdate {
                        max_context,
                        ..Default::default()
                    },
                )?;
                Ok(())
            })
            .await
            .expect("seed profile patch");
        }
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

fn normalize(mut v: Value) -> String {
    canon_numbers(&mut v);
    // The api key is not a comparand (module doc).
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn external_prompt_matches_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_EXTERNAL_PROMPT") else {
        eprintln!("SKIP: set QT_ORACLE_EXTERNAL_PROMPT (see the test header).");
        return;
    };
    let text = std::fs::read_to_string(&oracle_path).unwrap();
    assert!(
        !text.trim().is_empty(),
        "{oracle_path} is EMPTY — the regen truncated it before failing (ledger §5.1)"
    );
    let corpus: Corpus = serde_json::from_str(
        &std::fs::read_to_string(oracle_dir().join("external-prompt-tier3.json")).unwrap(),
    )
    .unwrap();
    let mut oracle: BTreeMap<String, OracleRow> = BTreeMap::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: OracleRow = serde_json::from_str(line).unwrap();
        oracle.insert(row.name.clone(), row);
    }
    let corpus_names: BTreeSet<&str> = corpus.cases.iter().map(|c| c.name.as_str()).collect();
    let oracle_names: BTreeSet<&str> = oracle.keys().map(String::as_str).collect();
    assert_eq!(
        corpus_names, oracle_names,
        "the oracle's case set must equal the corpus's"
    );

    let mut failed: Vec<String> = Vec::new();
    let mut calls_total = 0usize;
    for case in &corpus.cases {
        let o = &oracle[&case.name];
        let (db, scratch, user_id) = fresh_pair(&case.name);
        for seed in &case.seeds {
            let seed = match seed {
                SeedRef::Named(name) => {
                    assert_eq!(name, "richAria", "the only named seed is richAria");
                    corpus.rich_aria.clone()
                }
                SeedRef::Inline(s) => s.clone(),
            };
            apply_seed(&db, &seed).await;
        }
        let provider = Arc::new(RecordingProvider {
            inner: canned_for(o, &case.reply),
            calls: Mutex::new(Vec::new()),
        });
        let driver: Arc<dyn GeneratorsDetailDriver> = Arc::new(TestDriver {
            db: db.clone(),
            provider: Arc::clone(&provider),
        });

        // Decode through the `Request` enum — the edge's exact decode.
        let mut wire =
            json!({ "type": "characterGenerateExternalPrompt", "characterId": case.character_id });
        if let Some(obj) = case.body.as_object() {
            for (k, v) in obj {
                wire[k] = v.clone();
            }
        }
        let req: Request = serde_json::from_value(wire).expect("decodes through Request");
        let response = match req {
            Request::CharacterGenerateExternalPrompt {
                character_id,
                connection_profile_id,
                system_prompt_id,
                scenario_id,
                max_tokens,
            } => {
                let tri = |o: Option<Option<Value>>| o.map(|x| x.unwrap_or(Value::Null));
                let (cp, sp, sc, mt) = (
                    tri(connection_profile_id),
                    tri(system_prompt_id),
                    tri(scenario_id),
                    tri(max_tokens),
                );
                character_generate_external_prompt(
                    &db,
                    Some(&driver),
                    &user_id,
                    &character_id,
                    cp.as_ref(),
                    sp.as_ref(),
                    sc.as_ref(),
                    mt.as_ref(),
                )
                .await
            }
            other => panic!("decoded the wrong variant: {other:?}"),
        };
        let (status, body) = to_status_body(response);
        let calls = provider.calls.lock().unwrap().clone();
        calls_total += calls.len();
        let counts = llm_log_counts(&db);
        drop(driver);
        drop(db);
        let _ = std::fs::remove_dir_all(&scratch);

        let got = normalize(json!({
            "name": case.name, "status": status, "body": body,
            "calls": calls, "llmLogCounts": counts,
        }));
        let want = normalize(json!({
            "name": o.name, "status": o.status, "body": o.body,
            "calls": o.calls, "llmLogCounts": o.llm_log_counts,
        }));
        if got != want {
            failed.push(format!("{}:\n{}", case.name, first_diff(&got, &want)));
        }
    }
    eprintln!(
        "external_prompt_tier3_equivalence: {} cases driven, {} model calls recorded",
        corpus.cases.len(),
        calls_total
    );
    assert!(
        failed.is_empty(),
        "{} case(s) DIFFER:\n{}",
        failed.len(),
        failed.join("\n")
    );
}

/// **P4.85 item 4 — the route-level `[Characters v1] External prompt
/// generation starting` line.** v4 logs it at
/// `app/api/v1/characters/[id]/handlers/post.ts:320`, between
/// `generateExternalPromptSchema.parse(body)` and `generateExternalPrompt(…)`,
/// with four keys: `userId`, `characterId`, `connectionProfileId`,
/// `maxTokens`. Note what is NOT in v4's bag — `systemPromptId` and
/// `scenarioId` are parsed on the line above and deliberately not logged.
///
/// The differential above cannot see it (it compares the response envelope and
/// the model calls, and this line moves neither); the round record called it
/// "compared by nothing".
///
/// Driven with `driver: None`, so the handler logs and then answers the named
/// not-assembled refusal — no model call, and the line's position between v4's
/// two gates and the generation is what the silence arms fix.
///
/// Mutation: drop any field → its `contains` fails; add `system_prompt_id` →
/// the not-in-v4's-bag assert fails; move the line above the parse or the 404
/// → the matching silence arm fails.
#[test]
fn the_external_prompt_route_starting_line_carries_v4s_bag() {
    const SENTENCE: &str = "[Characters v1] External prompt generation starting";
    const ARIA: &str = "a1000000-0000-4000-8000-000000000001";
    const PROFILE: &str = "c0000001-0000-4000-8000-000000000001";
    const PROMPT: &str = "c32fabae-25ab-8e3d-a1dc-8dbacd3f5265";
    const MISSING: &str = "00000000-0000-4000-8000-00000000dead";
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    let (db, scratch, user_id) = fresh_pair("route_line");
    let lines = quilltap_core::test_support::captured(|| {
        rt.block_on(character_generate_external_prompt(
            &db,
            None,
            &user_id,
            ARIA,
            Some(&json!(PROFILE)),
            Some(&json!(PROMPT)),
            None,
            Some(&json!(4000)),
        ));
    });
    let line = lines
        .iter()
        .find(|l| l.contains(SENTENCE))
        .unwrap_or_else(|| panic!("the external-prompt route line is SILENT: {lines:#?}"));
    assert!(line.starts_with("INFO "), "v4 logs at info: {line}");
    for field in [
        format!("user_id={user_id}"),
        format!("character_id={ARIA}"),
        format!("connection_profile_id={PROFILE}"),
        "max_tokens=4000".to_string(),
    ] {
        assert!(line.contains(&field), "missing {field} in {line}");
    }
    // v4's bag is exactly those four: the two ids parsed on the line above are
    // NOT logged, and a port that helpfully added them would diverge.
    assert!(
        !line.contains("system_prompt_id") && !line.contains("scenario_id"),
        "v4's bag carries neither id: {line}"
    );
    drop(db);
    let _ = std::fs::remove_dir_all(&scratch);

    // --- the SILENCE half: the 404 and the Zod parse both precede the line. ---
    for (tag, character, max_tokens) in [
        ("route_line_404", MISSING, json!(4000)),
        // `maxTokens: z.number().int().min(1000).max(20000)` — under the floor.
        ("route_line_badbody", ARIA, json!(10)),
    ] {
        let (db, scratch, user_id) = fresh_pair(tag);
        let lines = quilltap_core::test_support::captured(|| {
            rt.block_on(character_generate_external_prompt(
                &db,
                None,
                &user_id,
                character,
                Some(&json!(PROFILE)),
                Some(&json!(PROMPT)),
                None,
                Some(&max_tokens),
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
