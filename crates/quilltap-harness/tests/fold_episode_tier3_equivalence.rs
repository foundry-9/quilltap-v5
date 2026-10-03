//! Tier-3 differential test: the **fold-time episode pass**
//! (`quilltap_core::services::fold_episode_pass` — v4 `runFoldEpisodePass`),
//! the episodic spine's creation-side keystone, verified tier-3 → tier-2.
//!
//! Both sides pin BOTH model calls identically: the completion by exact call
//! key (the jest oracle RECORDS the exact `provider|model|temperature|messages`
//! entry it answered; this test replays those recorded entries through
//! `CannedCompletionProvider`, so a prompt-byte divergence surfaces as a
//! canned-miss), and the embedding by the exact `buildMemoryEmbeddingText`
//! text. Then the per-run `FoldEpisodePassResult` is compared field-for-field
//! and the three affected tables (`memories`, `vector_entries`,
//! `vector_indices`) are diffed in the shared-cross-table id-map remap form.
//!
//! The corpus covers: the 0–2 episode cap (a third episode is dropped), the
//! summary fallback to `narrative.slice(0, 60)`, the entities coercion, the
//! importance clamp, the `when`-phrase resolution vs the window-start fallback,
//! narrative-mode `narrativeTime`, per-present-character writes (active AND
//! silent; the absent participant gets nothing), fragment linking in both
//! directions, an unstamped trailing message, an unknown participantId (the
//! `Character` speaker fallback), and an empty-episode run that must write
//! nothing.
//!
//! P4.D212 (bug 161, v4 `e7821606f`): the pass now resolves speakers through
//! the SHARED `services::speaker_names` (v4 deleted its private loop). The one
//! behaviour delta is an EMPTY-NAME character — the old gate was `if
//! (character)` (an empty label), the shared one `if (character?.name)` (the
//! `Character` fallback). The `episode_pass` window carries a line from an
//! ABSENT seat on such a character (`Dell`, blanked by the builder — absent,
//! so the pass writes it no memory): v4 at the baseline pin renders `: Dell
//! hums…`, v4 at `e7821606f` and the port `Character: Dell hums…`. Measured
//! red-first by restoring `main`'s pre-port `fold_episode_pass.rs` whole (the
//! canned key misses, 0 episodes). ⚠ The line lives in `episode_pass`, not
//! `no_episodes`: there a miss and a hit both write nothing, and the arm was
//! invisible (the first placement survived the pre-port file). Dropping the
//! resolver's empty-name filter ALONE stays green here — v4's `speakerLabel`
//! also falls through on an empty resolved name, so the label is guarded
//! twice; `speaker_names_equivalence` and the context-summary debug line's
//! `resolvedCount` are what see the map. The rest of the corpus is neutral
//! across the two pins.
//!
//! P4.144: the pass's own LOG LINES are a comparand. v4's three reachable
//! `[FoldEpisodePass]` lines — `Episode extraction failed` (WARN, the
//! `!success` arm only), `Failed to write episode for character` (WARN, the
//! per-character catch — which sits OUTSIDE both link loops, so a failed write
//! skips the rest of that character), and `Episode pass complete` (INFO, after
//! the episode loop) — are captured by the oracle from v4's OWN logger
//! (`Logger.prototype` wrapped in the same `resetModules` generation) as
//! `{kind:'logs', run, lines:[{level, message, context}]}`. This side runs each
//! pass under `global_capture::capture_async`, keeps the lines containing
//! `[FoldEpisodePass]`, and compares each against the oracle's line rendered
//! the way the capture renders it — level, the default module target,
//! message, then every context field in v4's ORDER (`key=value`, strings
//! unquoted) — so a swapped field, a wrong level, a `sqlite error:` prefix or
//! an extra/missing line each reds. Three runs exist for this, each over a NEW
//! chat with Aria + Bram present: `episode_fail` (the completion THROWS —
//! registered here through `with_failure` from the oracle's `fail` row),
//! `episode_write_fail` (a BEFORE INSERT trigger plant aborts Aria's episode)
//! and `episode_link_fail` (a BEFORE UPDATE plant that fires only when
//! `relatedMemoryIds` CHANGES aborts Aria's episode LINK update — her two
//! window fragments must stay unlinked — and a third plant refuses Bram's one
//! fragment BACK-link, so both link-write sites reach the catch and
//! `fragmentsLinked` must count neither; v4 at the pin: `memoriesWritten: 2,
//! fragmentsLinked: 0`, two WARNs then the INFO). Measured red-first on
//! unported `main` against the fresh oracle: 4 of 5 runs carry lines (every
//! run but the `no_episodes` silence leg); with the line assert disabled,
//! `main` also reds `episode_link_fail`'s result (`fragmentsLinked` 3 against
//! v4's 0 — its `let _ =` link writes kept linking Aria's two fragments and
//! counted Bram's refused back-link) and the `memories` table. The one
//! `#[test]` fail-fasts on `episode_pass`.
//!
//! Generate the fixtures + oracle output (Node 24, from the v4 checkout — the
//! CASES run from a `/tmp` mirror because jest ignores `.claude/` paths):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; W=<v5 worktree> ; M=/tmp/qt-d14-oracle
//!   rm -rf $M && mkdir -p $M && cp -R $W/harness/oracle/* $M/
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-fold-episode-main.db \
//!   QT_FIXTURE_MOUNT_OUT=/tmp/qt-fold-episode-mount.db \
//!     $N/npx tsx $W/harness/oracle/fixtures/build-fold-episode-fixture.ts
//!   QT_FIXTURE_FOLD_EPISODE_MAIN=/tmp/qt-fold-episode-main.db \
//!   QT_FIXTURE_FOLD_EPISODE_MOUNT=/tmp/qt-fold-episode-mount.db \
//!   QT_ORACLE_OUT=/tmp/oracle-fold-episode.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=120000 \
//!       --roots "$PWD" --roots "$M/cases" -- fold-episode-tier3
//! Run:
//!   QT_ORACLE_FOLD_EPISODE=/tmp/oracle-fold-episode.ndjson \
//!   QT_FIXTURE_FOLD_EPISODE_MAIN=/tmp/qt-fold-episode-main.db \
//!   QT_FIXTURE_FOLD_EPISODE_MOUNT=/tmp/qt-fold-episode-mount.db \
//!     cargo test -p quilltap-harness --test fold_episode_tier3_equivalence

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use quilltap_core::cheap_llm::CheapLlmSelection;
use quilltap_core::db::dump_table_json_conn;
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::model::completion::{
    CannedCompletionProvider, CompletionMessage, CompletionRole, CompletionUsage,
};
use quilltap_core::model::embedding::CannedEmbeddingProvider;
use quilltap_core::services::cheap_llm_exec::CheapLlmTaskExecutor;
use quilltap_core::services::fold_episode_pass::{
    run_fold_episode_pass, FoldWindowMessage, RunFoldEpisodePassInput,
};
use serde::Deserialize;
use serde_json::{json, Value};

// ---------------------------------------------------------------------------
// Spec structures (harness/oracle/fixtures/fold-episode-tier3.json).
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WindowMessageW {
    id: String,
    role: String,
    content: Option<String>,
    participant_id: Option<String>,
    created_at: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RunW {
    name: String,
    /// P4.144: the completion THROWS this (the oracle records it on the canned
    /// row too; only the canned row is read here).
    #[allow(dead_code)]
    fail: Option<String>,
    chat_id: String,
    timeline_mode: String,
    project_id: Option<String>,
    in_autonomous_room: bool,
    window_messages: Vec<WindowMessageW>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProfileW {
    id: String,
    provider: String,
    model_name: String,
    base_url: Option<String>,
    is_local: bool,
    parameters: Option<Value>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    test_pepper_base64: String,
    user_id: String,
    runs: Vec<RunW>,
    profile: ProfileW,
    canned_embeddings: HashMap<String, Vec<f32>>,
    canned_failures: Vec<String>,
}

#[derive(Deserialize)]
struct CannedMessageW {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct CannedUsageW {
    #[serde(rename = "promptTokens")]
    prompt_tokens: i64,
    #[serde(rename = "completionTokens")]
    completion_tokens: i64,
    #[serde(rename = "totalTokens")]
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
    /// P4.144: a failing call key — register a failure, not a response.
    fail: Option<String>,
}

// ---------------------------------------------------------------------------
// P4.144: the `[FoldEpisodePass]` log-line comparand.
// ---------------------------------------------------------------------------

use quilltap_core::test_support::global_capture;

const LINE_MARK: &str = "[FoldEpisodePass]";
const PASS_TARGET: &str = "quilltap_core::services::fold_episode_pass";

/// One oracle line rendered as `global_capture` renders a v5 event: `<LEVEL>
/// <target> <message>` then ` key=value` per context field in v4's key order
/// (the oracle's JSON keeps insertion order; `serde_json`'s `preserve_order`
/// keeps it here). Strings render unquoted (the `%` sigil), numbers bare.
fn render_oracle_line(line: &Value) -> String {
    let level = line["level"]
        .as_str()
        .expect("oracle log level")
        .to_uppercase();
    let message = line["message"].as_str().expect("oracle log message");
    let mut out = format!("{level} {PASS_TARGET} {message}");
    if let Some(ctx) = line["context"].as_object() {
        for (k, v) in ctx {
            let rendered = match v {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            out.push_str(&format!(" {k}={rendered}"));
        }
    }
    out
}

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/fold-episode-tier3.json")
}

// ---------------------------------------------------------------------------
// Table normalization — the memory-gate shared-cross-table id-map remap form.
// ---------------------------------------------------------------------------

struct TableSpec {
    table: &'static str,
    order_by: &'static str,
    id_columns: &'static [&'static str],
    id_array_columns: &'static [&'static str],
    ts_columns: &'static [&'static str],
}

const TABLES: &[TableSpec] = &[
    TableSpec {
        table: "memories",
        order_by: "content",
        id_columns: &["id"],
        id_array_columns: &["relatedMemoryIds"],
        ts_columns: &[
            "createdAt",
            "updatedAt",
            "lastReinforcedAt",
            "lastAccessedAt",
        ],
    },
    TableSpec {
        table: "vector_entries",
        order_by: "embedding",
        id_columns: &["id"],
        id_array_columns: &[],
        ts_columns: &["createdAt"],
    },
    TableSpec {
        table: "vector_indices",
        order_by: "id",
        // id == characterId (pinned) — left literal.
        id_columns: &[],
        id_array_columns: &[],
        ts_columns: &["createdAt", "updatedAt"],
    },
];

fn token_for(id_map: &mut HashMap<String, String>, raw: &str) -> String {
    let next = format!("ID_{}", id_map.len());
    id_map.entry(raw.to_string()).or_insert(next).clone()
}

fn normalize_table(dump: &mut Value, spec: &TableSpec, id_map: &mut HashMap<String, String>) {
    let rows = dump
        .get_mut("rows")
        .and_then(Value::as_array_mut)
        .unwrap_or_else(|| panic!("{}: dump has no rows array", spec.table));

    for row in rows.iter_mut() {
        let obj = row
            .as_object_mut()
            .unwrap_or_else(|| panic!("{}: row is not an object", spec.table));

        for col in spec.id_columns {
            if let Some(Value::String(raw)) = obj.get(*col) {
                let token = token_for(id_map, raw);
                obj.insert((*col).to_string(), Value::String(token));
            }
        }
        for col in spec.id_array_columns {
            if let Some(Value::String(raw)) = obj.get(*col) {
                if let Ok(Value::Array(items)) = serde_json::from_str::<Value>(raw) {
                    let mapped: Vec<Value> = items
                        .iter()
                        .map(|v| match v.as_str() {
                            Some(s) => Value::String(token_for(id_map, s)),
                            None => v.clone(),
                        })
                        .collect();
                    let re = serde_json::to_string(&Value::Array(mapped)).unwrap();
                    obj.insert((*col).to_string(), Value::String(re));
                }
            }
        }
        for col in spec.ts_columns {
            if obj.get(*col).map(|v| !v.is_null()).unwrap_or(false) {
                obj.insert((*col).to_string(), Value::String("<ts>".to_string()));
            }
        }
    }
}

fn normalize_all(dumps: &mut [Value]) {
    let mut id_map: HashMap<String, String> = HashMap::new();
    for (i, spec) in TABLES.iter().enumerate() {
        normalize_table(&mut dumps[i], spec, &mut id_map);
    }
}

#[tokio::test]
async fn fold_episode_tier3_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_FOLD_EPISODE") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_FOLD_EPISODE to the oracle NDJSON (see header).");
            return;
        }
    };
    let fixture_main = match std::env::var("QT_FIXTURE_FOLD_EPISODE_MAIN") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_FIXTURE_FOLD_EPISODE_MAIN to the seed main .db (see header).");
            return;
        }
    };
    let fixture_mount = match std::env::var("QT_FIXTURE_FOLD_EPISODE_MOUNT") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_FIXTURE_FOLD_EPISODE_MOUNT to the seed mount .db (see header)."
            );
            return;
        }
    };

    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(spec_path()).unwrap_or_else(|e| panic!("read spec: {e}")),
    )
    .expect("parse spec");
    let oracle_text =
        std::fs::read_to_string(&oracle_path).unwrap_or_else(|e| panic!("read oracle: {e}"));

    let mut oracle_results: Vec<(String, Value)> = Vec::new();
    let mut oracle_logs: HashMap<String, Vec<String>> = HashMap::new();
    let mut oracle_canned: Vec<CannedRowW> = Vec::new();
    let mut oracle_tables: HashMap<String, Value> = HashMap::new();
    for line in oracle_text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let v: Value = serde_json::from_str(line).expect("parse oracle line");
        match v.get("kind").and_then(Value::as_str) {
            Some("result") => {
                oracle_results.push((v["run"].as_str().unwrap().to_string(), v["result"].clone()))
            }
            Some("logs") => {
                let rendered = v["lines"]
                    .as_array()
                    .expect("oracle logs row has a lines array")
                    .iter()
                    .map(render_oracle_line)
                    .collect();
                oracle_logs.insert(v["run"].as_str().unwrap().to_string(), rendered);
            }
            Some("canned") => {
                oracle_canned.push(serde_json::from_value(v).expect("parse canned row"))
            }
            Some("table") => {
                oracle_tables.insert(v["table"].as_str().unwrap().to_string(), v);
            }
            other => panic!("unknown oracle row kind {other:?}"),
        }
    }
    assert_eq!(
        spec.runs.len(),
        oracle_results.len(),
        "run-count mismatch — regenerate the oracle NDJSON"
    );

    let pid = std::process::id();
    let work_main = std::env::temp_dir().join(format!("qt-fold-episode-main-rust-{pid}.db"));
    let work_mount = std::env::temp_dir().join(format!("qt-fold-episode-mount-rust-{pid}.db"));
    let _ = std::fs::remove_file(&work_main);
    let _ = std::fs::remove_file(&work_mount);
    std::fs::copy(&fixture_main, &work_main).unwrap_or_else(|e| panic!("copy main: {e}"));
    std::fs::copy(&fixture_mount, &work_mount).unwrap_or_else(|e| panic!("copy mount: {e}"));

    // The completion provider replays EXACTLY the entries the oracle recorded.
    let mut completion = CannedCompletionProvider::new();
    for row in &oracle_canned {
        let messages: Vec<CompletionMessage> = row
            .messages
            .iter()
            .map(|m| CompletionMessage {
                role: CompletionRole::from_v4_wire(m.role.as_str())
                    .unwrap_or_else(|| panic!("unexpected role {}", m.role.as_str())),
                content: m.content.clone(),
            })
            .collect();
        if let Some(fail) = &row.fail {
            completion = completion.with_failure(
                &row.provider,
                &row.model,
                row.temperature,
                &messages,
                fail,
            );
            continue;
        }
        completion = completion.with_response(
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
        );
    }

    let mut embedding = CannedEmbeddingProvider::new();
    for (text, vec) in &spec.canned_embeddings {
        embedding = embedding.with_vector(text.clone(), vec.clone());
    }
    for text in &spec.canned_failures {
        embedding = embedding.with_failure(text.clone());
    }

    let db = Db::open(
        DbPaths {
            main: work_main.clone(),
            mount_index: Some(work_mount.clone()),
            llm_logs: None,
        },
        &spec.test_pepper_base64,
    )
    .unwrap_or_else(|e| panic!("open fixture copies: {e}"));

    let selection = CheapLlmSelection {
        provider: spec.profile.provider.clone(),
        model_name: spec.profile.model_name.clone(),
        base_url: spec.profile.base_url.clone(),
        connection_profile_id: Some(spec.profile.id.clone()),
        is_local: spec.profile.is_local,
        profile_parameters: spec.profile.parameters.clone(),
    };
    // No llm-logs partition here: v4's `executeCheapLLMTask` logging is the
    // pre-existing `cheap_llm_exec` deferral, and the oracle's fold pass writes
    // no llm_logs rows this test diffs.
    let executor = CheapLlmTaskExecutor::new();
    // Before the first callsite is reached (the capture rig's contract).
    global_capture::install();

    for (run, (oracle_name, oracle_result)) in spec.runs.iter().zip(oracle_results) {
        assert_eq!(run.name, oracle_name, "run order mismatch");
        let input = RunFoldEpisodePassInput {
            chat_id: run.chat_id.clone(),
            user_id: spec.user_id.clone(),
            window_messages: run
                .window_messages
                .iter()
                .map(|m| FoldWindowMessage {
                    id: m.id.clone(),
                    role: m.role.clone(),
                    content: m.content.clone(),
                    participant_id: m.participant_id.clone(),
                    created_at: m.created_at.clone(),
                })
                .collect(),
            timeline_mode: run.timeline_mode.clone(),
            project_id: run.project_id.clone(),
            in_autonomous_room: run.in_autonomous_room,
        };
        let (result, captured) = global_capture::capture_async(run_fold_episode_pass(
            &db,
            &completion,
            &embedding,
            &executor,
            &selection,
            &input,
        ))
        .await;
        let got = json!({
            "episodesExtracted": result.episodes_extracted,
            "memoriesWritten": result.memories_written,
            "fragmentsLinked": result.fragments_linked,
        });
        assert_eq!(got, oracle_result, "{}: result object diverges", run.name);

        let got_lines: Vec<String> = captured
            .into_iter()
            .filter(|l| l.contains(LINE_MARK))
            .collect();
        let want_lines = oracle_logs
            .get(&run.name)
            .unwrap_or_else(|| panic!("{}: oracle ndjson has no logs row", run.name));
        assert_eq!(
            &got_lines, want_lines,
            "{}: the [FoldEpisodePass] lines diverge (level, target, message, field order)",
            run.name
        );
    }
    assert_eq!(
        oracle_logs.len(),
        spec.runs.len(),
        "one logs row per run — regenerate the oracle NDJSON"
    );

    let mut got: Vec<Value> = TABLES
        .iter()
        .map(|t| {
            db.read_main(|conn| dump_table_json_conn(conn, t.table, t.order_by))
                .unwrap_or_else(|e| panic!("dump {}: {e:?}", t.table))
        })
        .collect();
    drop(db);
    let _ = std::fs::remove_file(&work_main);
    let _ = std::fs::remove_file(&work_mount);

    let mut want: Vec<Value> = TABLES
        .iter()
        .map(|t| {
            oracle_tables
                .get(t.table)
                .unwrap_or_else(|| panic!("oracle ndjson missing table {}", t.table))
                .clone()
        })
        .collect();

    normalize_all(&mut got);
    normalize_all(&mut want);

    for (i, t) in TABLES.iter().enumerate() {
        assert_eq!(
            got[i]["columns"], want[i]["columns"],
            "{} column set / order",
            t.table
        );
        assert_eq!(
            got[i]["rows"], want[i]["rows"],
            "{} row state diverged\n  rust:   {}\n  oracle: {}",
            t.table, got[i]["rows"], want[i]["rows"]
        );
    }

    // Sanity: the 7 seeded fragments + 1 embedded Bug-26 seed + 4
    // `episode_pass` episode memories (2 episodes × 2 present characters; the
    // absent participant gets none) + 1 `episode_write_fail` episode (Bram's;
    // Aria's INSERT is planted away) + 2 `episode_link_fail` episodes (both
    // written; Aria's LINK update is planted away) = 15, and 8 vector entries
    // (one per episode write plus the embedded seed's — the fragments were
    // seeded without).
    let mem_rows = got[0]["rows"].as_array().expect("memory rows");
    assert_eq!(mem_rows.len(), 15, "expected 15 memory rows");
    let entry_rows = got[1]["rows"].as_array().expect("entry rows");
    assert_eq!(entry_rows.len(), 8, "expected 8 vector entries");

    eprintln!(
        "OK: fold-episode tier-3 matched oracle (results + [FoldEpisodePass] lines + 3 tables)."
    );
}
