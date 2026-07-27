//! P4.9E3B llm_choose TIER-3 differential: the out-of-create `llm_choose`
//! outfit pick — `chat_cast::chat_add_participant` and
//! `chat_merge::chat_merge_conversation` over the [`OutfitLlmChooseRunner`]
//! seam — vs v4's real routes with the model boundary mocked to the same
//! canned reply on both sides (`tier3-completion-oracle`).
//!
//! The test runner delegates to `run_llm_choose_via_db`, the EXACT composition
//! the production `HostOutfitLlmChooseRunner` uses, so the differential proves
//! the shipped path, not a test-only one.
//!
//! Compared per case: the status (v4 answers **201** on add — the standing
//! created-status difference, asserted directionally), the REDUCED body claim,
//! the LLM request MESSAGES the provider saw (the outfit prompt's bytes —
//! character fields, wardrobe listing, scenario — pinned end-to-end), and the
//! target chat's `equippedOutfit` + reduced cast. Minted ids/timestamps are
//! deliberately outside this family (the chat-cast / chat-admin bytes).
//!
//! Generate the oracle (Node 24, from the v4 checkout — see the .ts header):
//!   … QT_ORACLE_OUT=/tmp/oracle-llm-choose-tier3.ndjson npx jest -- chat-dialogs-llm-choose-tier3
//! Run:
//!   QT_ORACLE_LLM_CHOOSE=/tmp/oracle-llm-choose-tier3.ndjson \
//!     cargo test -p quilltap-harness --test outfit_llm_choose_tier3_equivalence -- --nocapture

use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use quilltap_core::api::types::Response;
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::model::completion::{
    CompletionError, CompletionParams, CompletionProvider, CompletionResponse, CompletionUsage,
};
use quilltap_core::services::chat_participants::ParticipantAddData;
use quilltap_core::services::cheap_llm_exec::CheapLlmTaskExecutor;
use quilltap_core::services::outfit_selections::{
    run_llm_choose_via_db, OutfitLlmChooseRequest, OutfitLlmChooseRunner,
};
use quilltap_core::wardrobe::Slots;
use serde::Deserialize;
use serde_json::{json, Value};

const PIP: &str = "a2000000-0000-4000-8000-000000000002";
const MERGE_TARGET: &str = "c2000000-0000-4000-8000-000000000007";
const MERGE_SOURCE: &str = "c2000000-0000-4000-8000-000000000008";

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Canned {
    content: String,
    prompt_tokens: i64,
    completion_tokens: i64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    test_pepper_base64: String,
    user_id: String,
    canned_outfits: HashMap<String, Canned>,
}

/// The mocked model boundary — records every message list, then answers the
/// canned reply or fails (both sides share the reply via the spec).
struct CannedOutfitProvider {
    reply: Option<Canned>,
    throws: bool,
    seen: Mutex<Vec<Value>>,
}

impl CompletionProvider for CannedOutfitProvider {
    async fn send_message(
        &self,
        _provider: &str,
        _base_url: Option<&str>,
        params: &CompletionParams,
    ) -> Result<CompletionResponse, CompletionError> {
        self.seen.lock().unwrap().push(Value::Array(
            params
                .messages
                .iter()
                .map(|m| json!({ "role": m.role.as_str(), "content": m.content }))
                .collect(),
        ));
        if self.throws {
            return Err(CompletionError::new("canned provider failure"));
        }
        let canned = self.reply.clone().expect("no canned reply for this case");
        Ok(CompletionResponse {
            content: canned.content,
            usage: Some(CompletionUsage {
                prompt_tokens: canned.prompt_tokens,
                completion_tokens: canned.completion_tokens,
                total_tokens: canned.prompt_tokens + canned.completion_tokens,
            }),
            finish_reason: None,
        })
    }
}

/// The test runner — the same composition as `HostOutfitLlmChooseRunner`
/// (db reads + executor + `run_llm_choose_via_db`), over the canned provider.
struct CannedRunner {
    db: Db,
    provider: Arc<CannedOutfitProvider>,
}

impl OutfitLlmChooseRunner for CannedRunner {
    fn choose(
        &self,
        req: OutfitLlmChooseRequest,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Option<Slots>> + Send + '_>> {
        Box::pin(async move {
            let executor = CheapLlmTaskExecutor::new();
            run_llm_choose_via_db(
                &self.db,
                &*self.provider,
                &executor,
                &req.character_id,
                req.scenario_text.as_deref(),
                req.cheap_settings.as_ref(),
            )
            .await
        })
    }
}

fn spec_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/chat-dialogs-web.json")
}
fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../quilltap-web/tests/fixtures")
}
fn env_or_skip(key: &str) -> Option<String> {
    match std::env::var(key) {
        Ok(v) => Some(v),
        Err(_) => {
            eprintln!("SKIP: set {key} (see test header).");
            None
        }
    }
}

fn fresh_db(spec: &Spec, tag: &str) -> Db {
    let scratch = std::env::temp_dir().join(format!("qt-lc-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    let main = scratch.join("main.db");
    let mount = scratch.join("mount.db");
    std::fs::copy(fixtures_dir().join("chat-dialogs-main.db"), &main).unwrap();
    std::fs::copy(fixtures_dir().join("chat-dialogs-mount.db"), &mount).unwrap();
    Db::open(
        DbPaths {
            main,
            mount_index: Some(mount),
            llm_logs: None,
        },
        &spec.test_pepper_base64,
    )
    .expect("open db")
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
fn norm(v: &Value) -> String {
    serde_json::to_string_pretty(&sorted(v)).unwrap()
}
fn first_diff(got: &str, want: &str) -> String {
    let g: Vec<&str> = got.lines().collect();
    let w: Vec<&str> = want.lines().collect();
    for i in 0..g.len().max(w.len()) {
        let gi = g.get(i).copied().unwrap_or("<none>");
        let wi = w.get(i).copied().unwrap_or("<none>");
        if gi != wi {
            return format!("  line {i}\n  GOT : {gi}\n  WANT: {wi}");
        }
    }
    "(identical line-by-line)".to_string()
}

/// The oracle's `readTables` mirror.
fn dump_tables(db: &Db, chat_id: &str) -> Value {
    let cid = chat_id.to_string();
    let chat = db
        .read_main(move |c| quilltap_core::db::chats_read::find_by_id(c, &cid))
        .unwrap()
        .unwrap_or(Value::Null);
    let equipped = db
        .read_main(|c| {
            quilltap_core::db::chats_outfits::ChatOutfitsRepository::new(c)
                .get_equipped_outfit(chat_id)
        })
        .unwrap()
        .unwrap_or_else(|| json!({}));
    let participants: Vec<Value> = chat
        .get("participants")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .map(|p| {
                    json!({
                        "characterId": p.get("characterId").cloned().unwrap_or(Value::Null),
                        "controlledBy": p.get("controlledBy").cloned().unwrap_or(Value::Null),
                        "status": p.get("status").cloned().unwrap_or(Value::Null),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    json!({ "equippedOutfit": equipped, "participants": participants })
}

#[test]
fn outfit_llm_choose_matches_oracle() {
    let Some(oracle_path) = env_or_skip("QT_ORACLE_LLM_CHOOSE") else {
        return;
    };
    let spec: Spec = serde_json::from_str(&std::fs::read_to_string(spec_path()).unwrap()).unwrap();

    let mut oracle: HashMap<String, Value> = HashMap::new();
    for line in std::fs::read_to_string(&oracle_path)
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
    {
        let v: Value = serde_json::from_str(line).unwrap();
        oracle.insert(v["name"].as_str().unwrap().to_string(), v);
    }
    assert!(
        !oracle.is_empty(),
        "the oracle NDJSON is empty — regenerate it (an erroring builder leaves a stale file)"
    );

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let mut failed: Vec<String> = Vec::new();
    let mut driven: BTreeSet<String> = BTreeSet::new();

    struct Case<'a> {
        name: &'a str,
        add: bool,
        reply: Option<&'a str>,
        throws: bool,
    }
    let cases = [
        Case {
            name: "add_llm_choose_pick",
            add: true,
            reply: Some("pipPick"),
            throws: false,
        },
        Case {
            name: "add_llm_choose_invalid_ids",
            add: true,
            reply: Some("pipInvalidIds"),
            throws: false,
        },
        Case {
            name: "add_llm_choose_provider_fails",
            add: true,
            reply: None,
            throws: true,
        },
        Case {
            name: "merge_llm_choose_pick",
            add: false,
            reply: Some("pipPick"),
            throws: false,
        },
        Case {
            name: "merge_llm_choose_provider_fails",
            add: false,
            reply: None,
            throws: true,
        },
    ];

    for c in cases {
        let db = fresh_db(&spec, c.name);
        let provider = Arc::new(CannedOutfitProvider {
            reply: c.reply.map(|k| spec.canned_outfits[k].clone()),
            throws: c.throws,
            seen: Mutex::new(Vec::new()),
        });
        let runner: Arc<dyn OutfitLlmChooseRunner> = Arc::new(CannedRunner {
            db: db.clone(),
            provider: Arc::clone(&provider),
        });

        let (resp, is_add) = if c.add {
            let data = ParticipantAddData {
                character_id: PIP.to_string(),
                outfit_selection: Some(json!({ "characterId": PIP, "mode": "llm_choose" })),
                ..Default::default()
            };
            (
                rt.block_on(quilltap_core::api::chat_cast::chat_add_participant(
                    &db,
                    &spec.user_id,
                    MERGE_TARGET,
                    &data,
                    Some(&runner),
                )),
                true,
            )
        } else {
            let selections = vec![json!({ "characterId": PIP, "mode": "llm_choose" })];
            (
                rt.block_on(
                    quilltap_core::services::chat_merge::chat_merge_conversation(
                        &db,
                        &spec.user_id,
                        MERGE_TARGET,
                        MERGE_SOURCE,
                        None,
                        Some(&selections),
                        Some(&runner),
                    ),
                ),
                false,
            )
        };
        driven.insert(c.name.to_string());
        let Some(want) = oracle.get(c.name) else {
            failed.push(format!("{}_MISSING_FROM_ORACLE", c.name));
            continue;
        };

        // Status: v4 answers 201 on add (the created-status precedent) and 200
        // on merge; every v5 success is 200 at the dispatch boundary.
        let want_status = want["status"].as_u64().unwrap_or(0);
        let v5_success = !matches!(resp, Response::Error(_));
        let expected_v4 = if is_add { 201 } else { 200 };
        if want_status != expected_v4 || !v5_success {
            eprintln!(
                "[{}] status drift (v4={want_status}, expected {expected_v4}; v5 success={v5_success})",
                c.name
            );
            failed.push(format!("{}_status", c.name));
        }

        // The reduced body claim.
        let body = match &resp {
            Response::ChatCast(v) | Response::ChatAdmin(v) => v.clone(),
            other => {
                eprintln!("[{}] unexpected response: {other:?}", c.name);
                failed.push(format!("{}_shape", c.name));
                continue;
            }
        };
        let reduced = if is_add {
            json!({
                "participantCharacterId":
                    body.get("participant").and_then(|p| p.get("characterId")).cloned().unwrap_or(Value::Null),
                "error": Value::Null,
            })
        } else {
            json!({
                "success": body.get("success").cloned().unwrap_or(Value::Null),
                "mergedCharacterIds":
                    body.get("merge").and_then(|m| m.get("mergedCharacterIds")).cloned().unwrap_or(Value::Null),
                "error": Value::Null,
            })
        };
        if norm(&reduced) != norm(&want["body"]) {
            eprintln!(
                "[{}] BODY MISMATCH:\n{}",
                c.name,
                first_diff(&norm(&reduced), &norm(&want["body"]))
            );
            failed.push(c.name.to_string());
        }

        // The LLM request messages — the outfit prompt's bytes, end-to-end.
        let seen = Value::Array(provider.seen.lock().unwrap().clone());
        if norm(&seen) != norm(&want["llmMessages"]) {
            eprintln!(
                "[{}] LLM MESSAGES MISMATCH:\n{}",
                c.name,
                first_diff(&norm(&seen), &norm(&want["llmMessages"]))
            );
            failed.push(format!("{}_llm_messages", c.name));
        } else {
            eprintln!("[{}] llm messages OK.", c.name);
        }

        // The outfit + reduced cast.
        let got_tables = dump_tables(&db, MERGE_TARGET);
        if norm(&got_tables) != norm(&want["tables"]) {
            eprintln!(
                "[{} tables] MISMATCH:\n{}",
                c.name,
                first_diff(&norm(&got_tables), &norm(&want["tables"]))
            );
            failed.push(format!("{}_tables", c.name));
        } else {
            eprintln!("[{} tables] OK.", c.name);
        }
    }

    let expected: BTreeSet<String> = oracle.keys().cloned().collect();
    let missing: Vec<&String> = expected.difference(&driven).collect();
    let extra: Vec<&String> = driven.difference(&expected).collect();
    assert!(
        missing.is_empty() && extra.is_empty(),
        "case-set drift — oracle-only: {missing:?}; driven-only: {extra:?}"
    );
    assert!(
        failed.is_empty(),
        "llm-choose tier-3 mismatches: {failed:?}"
    );
}
