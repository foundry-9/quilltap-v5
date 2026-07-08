//! Tier-3 differential test: the **context-summary service half**
//! (`quilltap_core::services::context_summary` — v4 `generateContextSummary` /
//! `invalidateContextSummaryIfMessageCovered` / `checkAndGenerateSummaryIfNeeded`).
//!
//! Both sides pin the model boundary identically: the fold / title / help-title
//! completions by exact call key (the jest oracle resolves each real call to a
//! corpus rule and RECORDS the exact `provider|model|temperature|messages` entry
//! it answered — including the deliberate title-failure entry, marked `fail`;
//! this test replays those recorded entries through `CannedCompletionProvider`,
//! so a prompt/selection/temperature divergence surfaces as a canned-miss).
//! Then:
//!
//!   1. each op's result object is compared structurally (built to the same
//!      reconstructed shape on both sides — the `check` ops reconstruct their
//!      observable outcome from the resulting chat + `background_jobs` state,
//!      since v4 `checkAndGenerateSummaryIfNeeded` returns void);
//!   2. the three affected tables (`chats`, `chat_messages`, `background_jobs`)
//!      are diffed in the minted-values normalization form (minted ids remapped
//!      to first-seen tokens; minted timestamps sentinel-placeholdered — a value
//!      equal to the seed sentinel stays pinned, catching a stray mint).
//!
//! W4.6b: the `generate` ops drive `generate_context_summary_with_seams` with
//! [`RealContextSummarySeams`], so the fold now ALSO writes the Librarian summary
//! whisper + the CONTEXT_SUMMARY / TITLE_GENERATION cost events into
//! `chat_messages` (and bumps the chat's token aggregates); the oracle un-mocks
//! the matching v4 writers for those ops. The `check`-op internal fold keeps the
//! `NoopSeams` (the spine caller is unchanged), so it posts none of those — the
//! oracle gates its mocks the same way.
//!
//! W4.6b + Round-3 Group 7: the `generate` ops drive `RealContextSummarySeams`, so
//! the fold now ALSO writes the Librarian summary whisper + the CONTEXT_SUMMARY /
//! TITLE_GENERATION cost events, MIRRORS the summary into the provisioned character's
//! vault (`writeConversationSummaryToVaults`), and REFRESHES relevant conversations
//! (`refreshRelevantConversationsOnFold`) — the latter surfacing a pre-seeded prior
//! summary (canned unit embedding) as a `relevant-conversations` whisper in
//! `chat_messages`. The oracle runs those seams REAL; the fixture is now two DBs
//! (main + mount-index with a provisioned vault + seeded embedded summary). The
//! mirror is diffed by the `doc_mount_file_links` path set; the refresh whisper by
//! the `chat_messages` dump. The `check`-op internal fold keeps `NoopSeams`.
//!
//! Generate the fixture + oracle (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-ctxsum-main.db QT_FIXTURE_MOUNT_OUT=/tmp/qt-ctxsum-mount.db \
//!     $N/npx tsx $V5/harness/oracle/fixtures/build-context-summary-service-fixture.ts
//!   QT_FIXTURE_CTXSUM=/tmp/qt-ctxsum-main.db QT_FIXTURE_CTXSUM_MOUNT=/tmp/qt-ctxsum-mount.db \
//!   QT_ORACLE_OUT=/tmp/oracle-context-summary-service.ndjson \
//!     $N/npx jest --silent --watchman=false --roots "$PWD" --roots "$V5/harness/oracle/cases" -- context-summary-service-tier3
//! Run:
//!   QT_ORACLE_CTXSUM=/tmp/oracle-context-summary-service.ndjson \
//!   QT_FIXTURE_CTXSUM=/tmp/qt-ctxsum-main.db QT_FIXTURE_CTXSUM_MOUNT=/tmp/qt-ctxsum-mount.db \
//!     cargo test -p quilltap-harness --test context_summary_service_tier3_equivalence

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use quilltap_core::cheap_llm::CheapLlmProfile;
use quilltap_core::db::dump_table_json_conn;
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::model::completion::{
    CannedCompletionProvider, CompletionMessage, CompletionRole, CompletionUsage,
};
use quilltap_core::services::cheap_llm_exec::{CheapLlmLogConfig, CheapLlmTaskExecutor};
use quilltap_core::services::context_summary::{
    check_and_generate_summary_if_needed, generate_context_summary_with_seams,
    invalidate_context_summary_if_message_covered, CheapLlmSettings, GenerateSummaryOptions,
    RealContextSummarySeams,
};
use quilltap_core::services::llm_logging::LogContext;
use serde::Deserialize;
use serde_json::{json, Value};

mod common;

const SEED_SENTINEL: &str = "2026-03-01T00:00:00.000Z";

// ---------------------------------------------------------------------------
// Ops spec (harness/oracle/fixtures/context-summary-service-ops.json).
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProfileW {
    id: String,
    provider: String,
    model_name: String,
    #[serde(default)]
    base_url: Option<String>,
    is_cheap: bool,
    is_dangerous_compatible: bool,
    #[serde(default)]
    parameters: Option<Value>,
    #[serde(default)]
    max_tokens: Option<f64>,
    #[serde(default)]
    model_class: Option<String>,
}

impl ProfileW {
    fn into_core(self) -> CheapLlmProfile {
        CheapLlmProfile {
            id: self.id,
            provider: self.provider,
            model_name: self.model_name,
            base_url: self.base_url,
            is_cheap: self.is_cheap,
            is_dangerous_compatible: self.is_dangerous_compatible,
            parameters: self.parameters,
            max_tokens: self.max_tokens,
            model_class: self.model_class,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OpW {
    name: String,
    kind: String,
    chat_id: String,
    #[serde(default)]
    force_regenerate: bool,
    #[serde(default)]
    message_ids: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OpsSpec {
    test_pepper_base64: String,
    user_id: String,
    profiles: Vec<ProfileW>,
    current_profile_id: String,
    ops: Vec<OpW>,
}

// ---------------------------------------------------------------------------
// Oracle rows.
// ---------------------------------------------------------------------------

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
    #[serde(default)]
    fail: Option<String>,
}

fn ops_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/context-summary-service-ops.json")
}

// ---------------------------------------------------------------------------
// Table normalization — minted-values remap + sentinel-aware timestamps.
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
        table: "chats",
        order_by: "id",
        // ids are pinned seed chat ids — left literal.
        id_columns: &[],
        id_array_columns: &["summaryAnchorMessageIds"],
        ts_columns: &["updatedAt", "lastMessageAt"],
    },
    TableSpec {
        table: "chat_messages",
        order_by: "id",
        // message ids: seed ids are pinned; the minted context-summary event id
        // is remapped (a value not seen among the seed ids becomes a token).
        id_columns: &[],
        id_array_columns: &[],
        // seed rows carry the pinned sentinel; the minted context-summary event's
        // createdAt is a real mint → collapsed to <ts>.
        ts_columns: &["createdAt"],
    },
    TableSpec {
        table: "background_jobs",
        order_by: "payload",
        id_columns: &["id"],
        id_array_columns: &[],
        ts_columns: &["createdAt", "updatedAt", "scheduledAt"],
    },
];

fn token_for(id_map: &mut HashMap<String, String>, raw: &str) -> String {
    let next = format!("ID_{}", id_map.len());
    id_map.entry(raw.to_string()).or_insert(next).clone()
}

/// Sentinel-aware timestamp: a value equal to the seed sentinel stays pinned
/// (proving no stray mint); anything else collapses to `<ts>`.
fn norm_ts(v: &Value) -> Value {
    match v.as_str() {
        Some(SEED_SENTINEL) => v.clone(),
        Some(_) => Value::String("<ts>".to_string()),
        None => v.clone(),
    }
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
                if let Ok(v @ Value::Array(_)) = serde_json::from_str::<Value>(raw) {
                    // Re-serialize so ordering is canonical (the anchor arrays are
                    // pinned seed ids, compared verbatim across sides).
                    let re = serde_json::to_string(&v).unwrap();
                    obj.insert((*col).to_string(), Value::String(re));
                }
            }
        }

        for col in spec.ts_columns {
            if let Some(v) = obj.get(*col) {
                let nv = norm_ts(v);
                obj.insert((*col).to_string(), nv);
            }
        }
    }
}

/// Collapse the minted `chat_messages` ids (the context-summary event ids — any
/// message id not present in the seed set) to a single constant placeholder on
/// both sides. Each such row is uniquely identified by its (chatId, context)
/// content, so the id itself carries no diffable information; a per-side token
/// number would just be arbitrary. Seed rows keep their pinned ids.
fn remap_minted_message_ids(dump: &mut Value, seed_ids: &std::collections::HashSet<String>) {
    let rows = dump.get_mut("rows").and_then(Value::as_array_mut).unwrap();
    for row in rows.iter_mut() {
        let obj = row.as_object_mut().unwrap();
        if let Some(Value::String(id)) = obj.get("id") {
            if !seed_ids.contains(id) {
                obj.insert("id".to_string(), Value::String("<minted>".to_string()));
            }
        }
    }
}

fn seed_ids_of(dump: &Value) -> Vec<String> {
    dump.get("rows")
        .and_then(Value::as_array)
        .map(|rows| {
            rows.iter()
                .filter_map(|r| r.get("id").and_then(Value::as_str).map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

#[tokio::test]
async fn context_summary_service_tier3_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_CTXSUM") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_CTXSUM to the oracle NDJSON (see header).");
            return;
        }
    };
    let fixture_main = match std::env::var("QT_FIXTURE_CTXSUM") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_FIXTURE_CTXSUM to the seed main .db (see header).");
            return;
        }
    };
    let fixture_mount = match std::env::var("QT_FIXTURE_CTXSUM_MOUNT") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_FIXTURE_CTXSUM_MOUNT to the seed mount-index .db (see header)."
            );
            return;
        }
    };

    let ops_spec: OpsSpec = serde_json::from_str(
        &std::fs::read_to_string(ops_path()).unwrap_or_else(|e| panic!("read ops: {e}")),
    )
    .expect("parse ops spec");
    let oracle_text =
        std::fs::read_to_string(&oracle_path).unwrap_or_else(|e| panic!("read oracle: {e}"));

    let mut oracle_results: Vec<(String, Value)> = Vec::new();
    let mut oracle_canned: Vec<CannedRowW> = Vec::new();
    let mut oracle_tables: HashMap<String, Value> = HashMap::new();
    let mut oracle_mount_links: Vec<String> = Vec::new();
    let mut oracle_llm_logs: Option<Vec<Value>> = None;
    for line in oracle_text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let v: Value = serde_json::from_str(line).expect("parse oracle line");
        match v.get("kind").and_then(Value::as_str) {
            Some("result") => {
                oracle_results.push((v["op"].as_str().unwrap().to_string(), v["result"].clone()))
            }
            Some("canned") => oracle_canned.push(serde_json::from_value(v).expect("parse canned")),
            Some("table") => {
                oracle_tables.insert(v["table"].as_str().unwrap().to_string(), v);
            }
            Some("llmlogs") => oracle_llm_logs = Some(common::oracle_llm_logs(&v)),
            Some("mountLinks") => {
                oracle_mount_links = v["paths"]
                    .as_array()
                    .expect("mountLinks.paths array")
                    .iter()
                    .map(|p| p.as_str().unwrap().to_string())
                    .collect();
            }
            other => panic!("unknown oracle row kind {other:?}"),
        }
    }

    // Fresh copies so the shared seed fixtures stay pristine.
    let pid = std::process::id();
    let work_main = std::env::temp_dir().join(format!("qt-ctxsum-rust-{pid}.db"));
    let work_mount = std::env::temp_dir().join(format!("qt-ctxsum-rust-mount-{pid}.db"));
    let _ = std::fs::remove_file(&work_main);
    let _ = std::fs::remove_file(&work_mount);
    std::fs::copy(&fixture_main, &work_main).unwrap_or_else(|e| panic!("copy main: {e}"));
    std::fs::copy(&fixture_mount, &work_mount).unwrap_or_else(|e| panic!("copy mount: {e}"));

    // Round-3 Group 7: the fold's refresh embeds the fold summary and searches the
    // vault. The `fold_regular` character (`aa…0001`) is the only provisioned vault,
    // so it is the only op whose refresh reaches the embed; canned to the same unit
    // vector the fixture seeded into the prior summary's chunk (cosine 1.0 → match).
    // The other generate chats' characters are unprovisioned → their refresh returns
    // early (no vault) before embedding, so no registration is needed for them.
    const FOLD_REGULAR_SUMMARY: &str = "Active threads: heist continues. Resolved decisions: split the take. Emotional state: wary. Open questions: escape route.";
    let embedding = quilltap_core::model::embedding::CannedEmbeddingProvider::new().with_vector(
        FOLD_REGULAR_SUMMARY,
        vec![1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
    );

    // Replay EXACTLY the recorded entries (responses + the recorded failure key).
    let mut completion = CannedCompletionProvider::new();
    for row in &oracle_canned {
        let messages: Vec<CompletionMessage> = row
            .messages
            .iter()
            .map(|m| CompletionMessage {
                role: match m.role.as_str() {
                    "system" => CompletionRole::System,
                    "user" => CompletionRole::User,
                    "assistant" => CompletionRole::Assistant,
                    other => panic!("unexpected role {other}"),
                },
                content: m.content.clone(),
            })
            .collect();
        if let Some(fail) = &row.fail {
            completion = completion.with_failure(
                &row.provider,
                &row.model,
                row.temperature,
                &messages,
                fail.clone(),
            );
        } else {
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
    }

    // W4.10b: a fresh llm-logs partition for the fold/title cheap-task rows
    // (SUMMARIZATION [fold] + TITLE_GENERATION [title]).
    let work_ll = work_main.with_file_name(format!("cs-ll-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&work_ll);
    common::materialize_llm_logs(&work_ll, &ops_spec.test_pepper_base64);

    let db = Db::open(
        DbPaths {
            main: work_main.clone(),
            mount_index: Some(work_mount.clone()),
            llm_logs: Some(work_ll.clone()),
        },
        &ops_spec.test_pepper_base64,
    )
    .unwrap_or_else(|e| panic!("open fixture copy: {e}"));

    let profiles: Vec<CheapLlmProfile> = ops_spec
        .profiles
        .into_iter()
        .map(ProfileW::into_core)
        .collect();
    let current_profile = profiles
        .iter()
        .find(|p| p.id == ops_spec.current_profile_id)
        .expect("current profile in spec")
        .clone();
    let settings = CheapLlmSettings {
        strategy: "PROVIDER_CHEAPEST".to_string(),
        user_defined_profile_id: None,
        default_cheap_profile_id: None,
        fallback_to_local: false,
    };

    assert_eq!(
        ops_spec.ops.len(),
        oracle_results.len(),
        "op-count mismatch — regenerate the oracle NDJSON"
    );

    for (op, (oracle_op, oracle_result)) in ops_spec.ops.iter().zip(&oracle_results) {
        assert_eq!(&op.name, oracle_op, "op order mismatch");
        // Per-op executor with logging: the fold / title cheap calls write
        // SUMMARIZATION / TITLE_GENERATION rows carrying this op's chatId (v4's
        // context-summary tasks send no messageId/characterId). A fresh executor
        // per op is equivalent to v4's module-global no-temp cache (no temperature
        // rejection in this corpus).
        let executor = CheapLlmTaskExecutor::with_logging(CheapLlmLogConfig {
            db: db.clone(),
            user_id: ops_spec.user_id.clone(),
            chat_id: Some(op.chat_id.clone()),
            message_id: None,
            ctx: LogContext::none(),
        });
        let got: Value = match op.kind.as_str() {
            "generate" => {
                let options = GenerateSummaryOptions {
                    user_id: ops_spec.user_id.clone(),
                    chat_id: op.chat_id.clone(),
                    connection_profile: current_profile.clone(),
                    cheap_llm_settings: settings.clone(),
                    available_profiles: profiles.clone(),
                    force_regenerate: op.force_regenerate,
                    danger_settings: None,
                    registry_cheapest_for_current: None,
                    // v4 `connectionProfile.maxContext ?? null` — the oracle's profile
                    // carries no maxContext, so the refresh list size is the max.
                    connection_max_context: None,
                };
                // W4.6b + Round-3 Group 7: drive the real Librarian re-post,
                // cost-event, vault MIRROR, and relevant-conversations REFRESH seams
                // (the `check`-op internal folds still use `NoopSeams` inside
                // `check_and_generate_summary_if_needed`, matching the oracle: the
                // real mirror/refresh no-op for its unprovisioned characters).
                let seams = RealContextSummarySeams {
                    db: &db,
                    embedding: &embedding,
                };
                let r = generate_context_summary_with_seams(
                    &db,
                    &completion,
                    &executor,
                    &options,
                    &seams,
                )
                .await;
                // Match v4's result shape: undefined keys are dropped by
                // JSON.stringify, so `summary`/`error`/`usage` only appear when set.
                let mut obj = serde_json::Map::new();
                obj.insert("success".into(), Value::from(r.success));
                if let Some(s) = &r.summary {
                    obj.insert("summary".into(), Value::from(s.clone()));
                }
                if let Some(e) = &r.error {
                    obj.insert("error".into(), Value::from(e.clone()));
                }
                obj.insert("wasGenerated".into(), Value::from(r.was_generated));
                if let Some(u) = r.usage {
                    obj.insert(
                        "usage".into(),
                        json!({
                            "promptTokens": u.prompt_tokens,
                            "completionTokens": u.completion_tokens,
                            "totalTokens": u.total_tokens,
                        }),
                    );
                }
                Value::Object(obj)
            }
            "invalidate" => {
                let r = invalidate_context_summary_if_message_covered(
                    &db,
                    &op.chat_id,
                    &op.message_ids,
                )
                .await;
                json!(r)
            }
            "check" => {
                let outcome = check_and_generate_summary_if_needed(
                    &db,
                    &completion,
                    &executor,
                    &op.chat_id,
                    &current_profile,
                    &settings,
                    &profiles,
                    &ops_spec.user_id,
                    None,
                    None,
                    true,
                )
                .await
                .unwrap_or_else(|e| panic!("{}: check failed: {e:?}", op.name));
                // Reconstruct the same observable shape as the oracle: enqueue
                // presence + the title job's interchange (else null) + the
                // resulting chat's contextSummary/title/lastSummaryTurn.
                let outcome = outcome.expect("check on existing chat");
                let enqueued = outcome.enqueued_title_job_id.is_some();
                let chat_id = op.chat_id.clone();
                let chat = db
                    .read_main(|conn| quilltap_core::db::chats_read::find_by_id(conn, &chat_id))
                    .unwrap()
                    .expect("chat exists");
                json!({
                    "enqueuedTitleUpdate": enqueued,
                    "currentInterchange": if enqueued { Value::from(outcome.current_interchange) } else { Value::Null },
                    "chatContextSummary": chat.get("contextSummary").cloned().unwrap_or(Value::Null),
                    "chatTitle": chat.get("title").cloned().unwrap_or(Value::Null),
                    "chatLastSummaryTurn": chat.get("lastSummaryTurn").cloned().unwrap_or(Value::Null),
                })
            }
            other => panic!("unknown op kind {other}"),
        };
        assert_eq!(&got, oracle_result, "{}: result object diverges", op.name);
    }

    // Dump + diff the three tables.
    let mut got: Vec<Value> = TABLES
        .iter()
        .map(|t| {
            db.read_main(|conn| dump_table_json_conn(conn, t.table, t.order_by))
                .unwrap_or_else(|e| panic!("dump {}: {e:?}", t.table))
        })
        .collect();

    // Mirror proof (Round-3 Group 7): the SET of `<mountPointId>|<relativePath>` in
    // the vault after the fold. The mirror wrote `Conversation Summaries/Old Title
    // A.md` into the `fold_regular` character's vault on both sides; the deterministic
    // relativePath is compared (minted ids / fileIds / timestamps / chunkCount — the
    // v4-reindex-vs-Rust-no-reindex divergence — are all ignored).
    let mut got_mount_links: Vec<String> = db
        .read_mount_index(|conn| {
            let mut stmt =
                conn.prepare("SELECT mountPointId, relativePath FROM doc_mount_file_links")?;
            let rows = stmt.query_map([], |r| {
                Ok(format!(
                    "{}|{}",
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?
                ))
            })?;
            let mut out = Vec::new();
            for row in rows {
                out.push(row?);
            }
            Ok(out)
        })
        .expect("dump mount links");
    got_mount_links.sort();
    let mut want_mount_links = oracle_mount_links.clone();
    want_mount_links.sort();
    assert_eq!(
        got_mount_links, want_mount_links,
        "vault doc_mount_file_links path set diverges (the mirror write)"
    );

    // W4.10b: diff the fold/title `llm_logs` rows (SUMMARIZATION + TITLE_GENERATION).
    let got_logs = common::dump_llm_logs(&db);
    let want_logs = oracle_llm_logs.expect("oracle emitted no llmlogs row");
    assert_eq!(
        got_logs,
        want_logs,
        "llm_logs rows diverge (got {} vs oracle {})",
        got_logs.len(),
        want_logs.len()
    );

    drop(db);
    let _ = std::fs::remove_file(&work_main);
    let _ = std::fs::remove_file(&work_mount);
    let _ = std::fs::remove_file(&work_ll);

    let mut want: Vec<Value> = TABLES
        .iter()
        .map(|t| {
            let mut v = oracle_tables
                .remove(t.table)
                .unwrap_or_else(|| panic!("oracle missing table {}", t.table));
            v.as_object_mut().unwrap().remove("kind");
            v
        })
        .collect();

    // Remap the minted context-summary message id (shared token space per side).
    // Compute the seed message id set from the intersection of both dumps' ids so
    // that only the truly-minted rows tokenize.
    let got_msg_ids: std::collections::HashSet<String> = seed_ids_of(&got[1]).into_iter().collect();
    let want_msg_ids: std::collections::HashSet<String> =
        seed_ids_of(&want[1]).into_iter().collect();
    let seed_msg_ids: std::collections::HashSet<String> =
        got_msg_ids.intersection(&want_msg_ids).cloned().collect();
    remap_minted_message_ids(&mut got[1], &seed_msg_ids);
    remap_minted_message_ids(&mut want[1], &seed_msg_ids);

    for dumps in [&mut got, &mut want] {
        let mut id_map: HashMap<String, String> = HashMap::new();
        for (i, spec) in TABLES.iter().enumerate() {
            normalize_table(&mut dumps[i], spec, &mut id_map);
            // Re-sort rows by a side-stable composite key. The minted
            // context-summary event id differs per side; its `MINTED_n` token
            // (assigned in raw-id sort order) is NOT stable, so ordering on it
            // misaligns the arrays. For `chat_messages` sort by (chatId, type,
            // context, content, id) — the minted rows are unique by their fold
            // `context` text (identical both sides). Other tables keep order_by.
            if let Some(rows) = dumps[i].get_mut("rows").and_then(Value::as_array_mut) {
                if spec.table == "chat_messages" {
                    // The minted rows now include the Librarian summary whisper
                    // (systemSender='librarian') + the CONTEXT_SUMMARY / TITLE_GENERATION
                    // SYSTEM cost events (distinguished by systemEventType/description),
                    // all sharing the `<minted>` id token, so the sort key spans
                    // every discriminating column.
                    rows.sort_by(|a, b| {
                        let key = |r: &Value| {
                            let f = |k: &str| {
                                r.get(k).and_then(Value::as_str).unwrap_or("").to_string()
                            };
                            (
                                f("chatId"),
                                f("type"),
                                f("systemEventType"),
                                f("systemSender"),
                                f("systemKind"),
                                f("description"),
                                f("context"),
                                f("content"),
                                f("id"),
                            )
                        };
                        key(a).cmp(&key(b))
                    });
                } else {
                    rows.sort_by(|a, b| {
                        let av = a.get(spec.order_by).and_then(Value::as_str).unwrap_or("");
                        let bv = b.get(spec.order_by).and_then(Value::as_str).unwrap_or("");
                        av.cmp(bv)
                    });
                }
            }
        }
    }

    for (i, t) in TABLES.iter().enumerate() {
        assert_eq!(
            got[i]["columns"], want[i]["columns"],
            "{} column set / order",
            t.table
        );
        assert_eq!(got[i]["rows"], want[i]["rows"], "{} rows diverge", t.table);
    }
}
