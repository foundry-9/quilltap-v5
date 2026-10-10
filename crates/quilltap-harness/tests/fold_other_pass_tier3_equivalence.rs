//! Tier-3 differential test: the **fold-grain OTHER pass** (P4.D270 — v4
//! `d58548051`, pin `01a83539d`): `services::fold_other_pass` —
//! `run_fold_other_pass`, `find_fold_other_catchup_candidates`,
//! `enqueue_fold_other_catchups` — and `handle_memory_extraction`'s
//! `foldOtherCatchup` branch (→ `run_fold_other_catchup`), against v4's REAL
//! `runFoldOtherPass` / `findFoldOtherCatchupCandidates` /
//! `enqueueFoldOtherCatchups` / `handleMemoryExtraction` over the SAME fixture
//! pair (`build-fold-other-pass-fixture.ts`).
//!
//! Both sides pin the two model calls identically: the completion by exact
//! call key (the oracle RECORDS every `provider|model|temperature|messages` key
//! it answered — a failing rule records its `fail` — and this test replays
//! exactly those entries through `CannedCompletionProvider`, so a prompt /
//! canon / selection divergence surfaces as a canned-miss), and the embedding
//! by exact text (the oracle's one-hot e_n per distinct text).
//!
//! Ops run sequentially on ONE copy per side (disjoint chats per op; the
//! per-op `memoryExtractionMode` / `memoryExtractionLimits` plants go through
//! each side's REAL setter and are deleted after the op). Compared per op: the
//! result object (the pass / selection / sweep summary; the handler's job
//! error) and every `[FoldOtherPass]` / `[FoldOtherCatchup]` /
//! `[MemoryExtraction] Fold-grain…` / `[Canon]` / sweep line (v4's through a
//! `Logger.prototype` wrap; v5's through `global_capture` — every line here is
//! logged on the caller thread). Then the end state: `memories` (minted ids /
//! stamps tokenized), `chats` (the watermark, `updatedAt` — untouched by the
//! one writer — and `messageCount`; `lastMessageAt` is planted at each side's
//! op time for two handler ops, so it is masked), `background_jobs` (type,
//! payload bytes, status).
//!
//! NOT compared here: `llm_logs` — this oracle keeps jest.setup's
//! `logLLMCall` mock (zero rows); `memory_pipeline_jobs_tier3` compares the
//! fold OTHER call's rows end to end. The two unreachable v4 lines R-G names
//! are pinned NEGATIVE across the whole corpus (`[Canon] … unavailable …`; the
//! fold hook's `[Context Summary] Fold OTHER pass failed:` cannot appear here
//! at all). A v5-only dry-run check closes the file (Tier 2 item 17).
//!
//! Generate the fixture + oracle (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   TMPO=/tmp/qt-fop-oracle
//!   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
//!   cp "$V5W/harness/oracle/cases/fold-other-pass-tier3.test.ts" "$TMPO/cases/"
//!   cp "$V5W/harness/oracle/fixtures/fold-other-pass-tier3.json" "$TMPO/fixtures/"
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-fop-main.db QT_FIXTURE_MOUNT_OUT=/tmp/qt-fop-mount.db \
//!     $N/npx tsx $V5W/harness/oracle/fixtures/build-fold-other-pass-fixture.ts
//!   TZ=UTC QT_FIXTURE_FOP_MAIN=/tmp/qt-fop-main.db QT_FIXTURE_FOP_MOUNT=/tmp/qt-fop-mount.db \
//!   QT_ORACLE_OUT=/tmp/oracle-fold-other-pass.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=180000 --roots "$PWD" --roots "$TMPO/cases" -- "fold-other-pass-tier3\\.test\\.ts$"
//! Run:
//!   QT_ORACLE_FOLD_OTHER_PASS=/tmp/oracle-fold-other-pass.ndjson \
//!   QT_FIXTURE_FOP_MAIN=/tmp/qt-fop-main.db QT_FIXTURE_FOP_MOUNT=/tmp/qt-fop-mount.db \
//!     cargo test -p quilltap-harness --test fold_other_pass_tier3_equivalence

use std::collections::{BTreeMap, HashMap};

use quilltap_core::cheap_llm::{get_cheap_llm_provider, CheapLlmConfig, CheapLlmProfile};
use quilltap_core::context_budget::resolve_max_tokens;
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::db::{chats_messages_read, connection_profiles, instance_settings};
use quilltap_core::model::completion::{
    CannedCompletionProvider, CompletionMessage, CompletionRole, CompletionUsage,
};
use quilltap_core::model::embedding::CannedEmbeddingProvider;
use quilltap_core::services::cheap_llm_exec::CheapLlmTaskExecutor;
use quilltap_core::services::fold_other_pass::{run_fold_other_pass, RunFoldOtherPassInput};
use quilltap_core::test_support::global_capture;
use serde::Deserialize;
use serde_json::Value;

/// Which op kinds this commit's port compares (the family landed with the
/// pass, P4.D270 E5; the catch-up + handler ops with E7; the sweep + the end
/// state with E8).
const COMPARED_KINDS: &[&str] = &["pass"];

const INSTANCE_SETTINGS_DDL: &str =
    "CREATE TABLE IF NOT EXISTS \"instance_settings\" (\n  \"key\" TEXT PRIMARY KEY,\n  \"value\" TEXT NOT NULL\n)";

/// The messages this family compares (the oracle captures the same set).
const MESSAGES: &[&str] = &[
    "[FoldOtherPass] otherPass=turn; fold-grain pass not run",
    "[FoldOtherPass] Applied watermark to window",
    "[FoldOtherPass] Window trimmed to newest messages",
    "[FoldOtherPass] Character unavailable; leaving it out",
    "[FoldOtherPass] Pass set up",
    "[FoldOtherPass] Observer rate-limited; skipping",
    "[FoldOtherPass] OTHER extraction failed",
    "[FoldOtherPass] Candidates for pair",
    "[FoldOtherPass] Passes lost to timeout; watermark not advanced",
    "[FoldOtherPass] Fold-grain OTHER pass complete",
    "[FoldOtherPass] Fold-grain OTHER pass failed (non-fatal)",
    "[FoldOtherPass] Watermark advanced",
    "[FoldOtherPass] Failed to advance watermark",
    "[FoldOtherCatchup] otherPass=turn; no catch-up needed",
    "[FoldOtherCatchup] Could not inspect chat; skipping",
    "[FoldOtherCatchup] No connection profile on any seat; skipping chat",
    "[FoldOtherCatchup] Selection complete",
    "[FoldOtherCatchup] Chat active again since the sweep; leaving it to the fold",
    "[FoldOtherCatchup] Running catch-up pass",
    "[MemoryExtraction] Fold-grain OTHER catch-up processed",
    "[Canon] Commonplace digest loaded into canon",
    "[Canon] Commonplace digest unavailable; canon continues without it",
    "Enqueued fold-grain OTHER catch-up",
    "Failed to enqueue fold-grain OTHER catch-up",
    "Fold-grain OTHER catch-up capped; remaining chats wait for the next sweep",
    "Fold-grain OTHER catch-up sweep done",
];

/// The two v4 catch lines R-G / E2 rule unreachable — pinned negative.
const NEVER: &[&str] = &[
    "[Canon] Commonplace digest unavailable; canon continues without it",
    "[Context Summary] Fold OTHER pass failed:",
];

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
// The catch-up / sweep / handler fields are read once those ops compare
// (P4.D270 E7 / E8).
#[allow(dead_code)]
struct OpW {
    op: String,
    name: String,
    #[serde(default)]
    chat_id: Option<String>,
    #[serde(default)]
    tag: Option<String>,
    #[serde(default)]
    mode: Option<Value>,
    #[serde(default)]
    limits: Option<Value>,
    #[serde(default)]
    in_autonomous_room: Option<bool>,
    #[serde(default)]
    timeline_mode: Option<String>,
    #[serde(default)]
    limit: Option<usize>,
    #[serde(default)]
    prior_turn_job: Option<Value>,
    #[serde(default)]
    fold_other_catchup: Option<Value>,
    #[serde(default)]
    fold_other_ignore_idle: Option<bool>,
    #[serde(default)]
    plant_last_message_at_now: Option<bool>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SpecW {
    test_pepper_base64: String,
    user_id: String,
    fixed_now: String,
    connection_profile_id: String,
    ops: Vec<OpW>,
}

#[derive(Deserialize)]
struct CannedMessageW {
    role: String,
    content: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CannedUsageW {
    prompt_tokens: i64,
    completion_tokens: i64,
    total_tokens: i64,
}

#[derive(Deserialize)]
struct CannedRowW {
    provider: String,
    model: String,
    temperature: Option<f64>,
    messages: Vec<CannedMessageW>,
    response: String,
    usage: CannedUsageW,
    #[serde(default)]
    fail: Option<String>,
}

type Line = (String, String, BTreeMap<String, String>);

/// A rig line (`LEVEL target message k=v …`) → `(level, message, fields)`;
/// `None` for a line outside [`MESSAGES`].
fn rust_line(line: &str) -> Option<Line> {
    let (level, rest) = line.split_once(' ')?;
    let (_target, rest) = rest.split_once(' ')?;
    let rest = rest.trim_start();
    let message = MESSAGES
        .iter()
        .filter(|m| rest.starts_with(**m))
        .max_by_key(|m| m.len())?;
    let tail = rest[message.len()..].trim_start();
    let mut fields = BTreeMap::new();
    // Split on ` <ident>=` boundaries; a value may carry spaces (an error).
    let bytes = tail.as_bytes();
    let mut keys: Vec<(usize, usize)> = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if i == 0 || bytes[i - 1] == b' ' {
            let mut j = i;
            while j < bytes.len() && bytes[j].is_ascii_alphanumeric() {
                j += 1;
            }
            if j > i && j < bytes.len() && bytes[j] == b'=' {
                keys.push((i, j));
                i = j + 1;
                continue;
            }
        }
        i += 1;
    }
    for (n, (start, eq)) in keys.iter().enumerate() {
        let end = keys
            .get(n + 1)
            .map(|(next, _)| next - 1)
            .unwrap_or(tail.len());
        fields.insert(tail[*start..*eq].to_string(), tail[eq + 1..end].to_string());
    }
    Some((level.to_lowercase(), message.to_string(), fields))
}

/// v4's captured line → the same shape (values as the rig renders them).
fn oracle_line(v: &Value) -> Line {
    let fields = v["context"]
        .as_object()
        .map(|o| {
            o.iter()
                .map(|(k, val)| {
                    let rendered = match val {
                        Value::String(s) => s.clone(),
                        other => other.to_string(),
                    };
                    (k.clone(), rendered)
                })
                .collect()
        })
        .unwrap_or_default();
    (
        v["level"].as_str().unwrap().to_string(),
        v["message"].as_str().unwrap().to_string(),
        fields,
    )
}

/// A connection-profile row → the cheap-LLM profile (the core's
/// `image_job_common::cheap_llm_profile_from_value`, which is crate-private).
fn profile_from(v: &Value) -> CheapLlmProfile {
    let s = |k: &str| v.get(k).and_then(Value::as_str).map(str::to_string);
    CheapLlmProfile {
        id: s("id").unwrap_or_default(),
        provider: s("provider").unwrap_or_default(),
        model_name: s("modelName").unwrap_or_default(),
        base_url: s("baseUrl"),
        is_cheap: v.get("isCheap").and_then(Value::as_bool) == Some(true),
        is_dangerous_compatible: v.get("isDangerousCompatible").and_then(Value::as_bool)
            == Some(true),
        parameters: v.get("parameters").cloned(),
        max_tokens: v.get("maxTokens").and_then(Value::as_f64),
        max_context: v.get("maxContext").and_then(Value::as_f64),
        model_class: s("modelClass"),
    }
}

async fn plant(db: &Db, key: &'static str, value: Value) {
    db.write(move |w| {
        let c = w.main().connection();
        c.execute_batch(INSTANCE_SETTINGS_DDL)?;
        if key == "memoryExtractionMode" {
            instance_settings::set_memory_extraction_mode_settings(c, &value).map(|_| ())
        } else {
            instance_settings::set_memory_extraction_limits(c, &value)
        }
    })
    .await
    .unwrap_or_else(|e| panic!("plant {key}: {e:?}"));
}

async fn unplant(db: &Db, key: &'static str) {
    db.write(move |w| {
        w.main()
            .connection()
            .execute(
                "DELETE FROM \"instance_settings\" WHERE \"key\" = ?1",
                [key],
            )
            .map(|_| ())
            .map_err(Into::into)
    })
    .await
    .unwrap_or_else(|e| panic!("unplant {key}: {e:?}"));
}

#[tokio::test]
async fn fold_other_pass_tier3_matches_oracle() {
    global_capture::install();
    let _key = quilltap_core::test_support::CannedCheapLlmKey::install("test-key");
    let (Ok(oracle_path), Ok(fixture_main), Ok(fixture_mount)) = (
        std::env::var("QT_ORACLE_FOLD_OTHER_PASS"),
        std::env::var("QT_FIXTURE_FOP_MAIN"),
        std::env::var("QT_FIXTURE_FOP_MOUNT"),
    ) else {
        eprintln!("SKIP: set QT_ORACLE_FOLD_OTHER_PASS + QT_FIXTURE_FOP_MAIN/MOUNT (see header).");
        return;
    };
    let spec: SpecW = serde_json::from_str(
        &std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../harness/oracle/fixtures/fold-other-pass-tier3.json"
        ))
        .unwrap(),
    )
    .unwrap();
    let oracle_text = std::fs::read_to_string(&oracle_path).unwrap();
    assert!(
        oracle_text.contains("Fold-grain OTHER pass complete"),
        "stale oracle — no completion INFO recorded"
    );

    let mut results: HashMap<String, (Value, Option<String>)> = HashMap::new();
    let mut logs: HashMap<String, Vec<Line>> = HashMap::new();
    let mut canned: Vec<CannedRowW> = Vec::new();
    let mut embeddings: Vec<(String, usize, usize)> = Vec::new();
    let mut tables: HashMap<String, Value> = HashMap::new();
    for line in oracle_text.lines().filter(|l| !l.trim().is_empty()) {
        let v: Value = serde_json::from_str(line).unwrap();
        match v["kind"].as_str().unwrap() {
            "result" => {
                results.insert(
                    v["op"].as_str().unwrap().to_string(),
                    (v["result"].clone(), v["error"].as_str().map(str::to_string)),
                );
            }
            "logs" => {
                logs.insert(
                    v["op"].as_str().unwrap().to_string(),
                    v["lines"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(oracle_line)
                        .collect(),
                );
            }
            "canned" => canned.push(serde_json::from_value(v).unwrap()),
            "cannedEmbedding" => embeddings.push((
                v["text"].as_str().unwrap().to_string(),
                v["index"].as_u64().unwrap() as usize,
                v["dim"].as_u64().unwrap() as usize,
            )),
            "table" => {
                tables.insert(v["table"].as_str().unwrap().to_string(), v["rows"].clone());
            }
            other => panic!("unknown oracle row kind {other}"),
        }
    }

    let mut completion = CannedCompletionProvider::new();
    for row in &canned {
        let messages: Vec<CompletionMessage> = row
            .messages
            .iter()
            .map(|m| CompletionMessage {
                role: CompletionRole::from_v4_wire(&m.role).unwrap(),
                content: m.content.clone(),
            })
            .collect();
        completion = match &row.fail {
            Some(message) => completion.with_failure(
                &row.provider,
                &row.model,
                row.temperature,
                &messages,
                message.clone(),
            ),
            None => completion.with_response(
                &row.provider,
                &row.model,
                row.temperature,
                &messages,
                row.response.clone(),
                Some(CompletionUsage {
                    prompt_tokens: row.usage.prompt_tokens,
                    completion_tokens: row.usage.completion_tokens,
                    total_tokens: row.usage.total_tokens,
                }),
            ),
        };
    }
    let mut embedding = CannedEmbeddingProvider::new();
    for (text, index, dim) in &embeddings {
        let mut vec = vec![0f32; *dim];
        vec[*index] = 1.0;
        embedding = embedding.with_vector(text.clone(), vec);
    }

    let scratch = tempfile::Builder::new()
        .prefix("qt-fop-rust-")
        .tempdir()
        .unwrap();
    let work_main = scratch.path().join("fop-main.db");
    let work_mount = scratch.path().join("fop-mount.db");
    std::fs::copy(&fixture_main, &work_main).unwrap();
    std::fs::copy(&fixture_mount, &work_mount).unwrap();
    let db = Db::open(
        DbPaths {
            main: work_main,
            mount_index: Some(work_mount),
            llm_logs: None,
        },
        &spec.test_pepper_base64,
    )
    .unwrap_or_else(|e| panic!("open fixture copies: {e}"));
    let executor = CheapLlmTaskExecutor::new();

    let uid = spec.user_id.clone();
    let profile_rows = db
        .read_main(move |c| connection_profiles::find_by_user_id(c, &uid))
        .unwrap();
    let profiles: Vec<CheapLlmProfile> = profile_rows.iter().map(profile_from).collect();
    let current = profiles
        .iter()
        .find(|p| p.id == spec.connection_profile_id)
        .unwrap()
        .clone();
    let selection = get_cheap_llm_provider(
        &current,
        &CheapLlmConfig {
            strategy: "PROVIDER_CHEAPEST".into(),
            user_defined_profile_id: None,
            default_cheap_profile_id: None,
            fallback_to_local: false,
        },
        &profiles,
        false,
        None,
    );
    let cheap_max_tokens = resolve_max_tokens(
        current.max_tokens.map(|v| v as i64),
        current.model_class.as_deref(),
    );
    let fixed_now_ms = quilltap_core::clock::iso_to_ms(&spec.fixed_now).expect("fixedNow");

    let mut failed: Vec<String> = Vec::new();
    let mut compared = 0usize;
    for op in &spec.ops {
        if !COMPARED_KINDS.contains(&op.op.as_str()) {
            // Not yet ported at this commit — the corpus orders every pass op
            // first, so skipping the rest moves no compared op's input.
            continue;
        }
        if let Some(mode) = &op.mode {
            plant(&db, "memoryExtractionMode", mode.clone()).await;
        }
        if let Some(limits) = &op.limits {
            plant(&db, "memoryExtractionLimits", limits.clone()).await;
        }
        let run = async {
            match op.op.as_str() {
                "pass" => {
                    let chat_id = op.chat_id.clone().unwrap();
                    let cid = chat_id.clone();
                    let window: Vec<Value> = db
                        .read_main(move |c| chats_messages_read::get_messages(c, &cid))
                        .unwrap()
                        .into_iter()
                        .filter(|m| m.get("type").and_then(Value::as_str) == Some("message"))
                        .collect();
                    let r = run_fold_other_pass(
                        &db,
                        &completion,
                        &embedding,
                        &executor,
                        &selection,
                        None,
                        &RunFoldOtherPassInput {
                            chat_id,
                            user_id: spec.user_id.clone(),
                            window_messages: window,
                            cheap_max_tokens: Some(cheap_max_tokens),
                            timeline_mode: op
                                .timeline_mode
                                .clone()
                                .unwrap_or_else(|| "realtime".into()),
                            project_id: None,
                            in_autonomous_room: op.in_autonomous_room.unwrap_or(false),
                            project_description: None,
                            chat_context_summary: Some(format!(
                                "[op:{}] The household is planning the harbor festival.",
                                op.tag.as_deref().unwrap()
                            )),
                            cover_through_message_id: None,
                            dry_run: false,
                        },
                    )
                    .await;
                    (serde_json::to_value(r).unwrap(), None)
                }
                other => panic!("{}: op kind {other} not compared at this commit", op.name),
            }
        };
        let ((got, got_error), lines) = global_capture::capture_async(run).await;
        if op.mode.is_some() {
            unplant(&db, "memoryExtractionMode").await;
        }
        if op.limits.is_some() {
            unplant(&db, "memoryExtractionLimits").await;
        }
        for l in &lines {
            for never in NEVER {
                assert!(
                    !l.contains(never),
                    "{}: an unreachable v4 line fired: {l}",
                    op.name
                );
            }
        }
        compared += 1;
        let (want, want_error) = results
            .get(&op.name)
            .unwrap_or_else(|| panic!("{}: no oracle result", op.name))
            .clone();
        let got_lines: Vec<Line> = lines.iter().filter_map(|l| rust_line(l)).collect();
        let want_lines = logs.get(&op.name).cloned().unwrap_or_default();
        for (_, m, _) in &want_lines {
            assert!(
                !NEVER.iter().any(|n| m.contains(n)),
                "{}: v4 fired {m}",
                op.name
            );
        }
        let ok = got == want && got_error == want_error && got_lines == want_lines;
        if !ok {
            eprintln!(
                "{}:\n  got  {got}\n  want {want}\n  got error  {got_error:?}\n  want error {want_error:?}\n  got lines  {got_lines:#?}\n  want lines {want_lines:#?}",
                op.name
            );
            failed.push(op.name.clone());
        }
    }
    assert!(
        failed.is_empty(),
        "{} op(s) diverge: {failed:?}",
        failed.len()
    );
    assert!(compared > 0, "no op compared");

    // The end-state tables compare once the sweep + handler ops run (E8).
    let _ = (&tables, fixed_now_ms);
    eprintln!("OK: fold-other-pass matched the oracle ({compared} ops)");
}
