//! P4.D135 — the cheap-LLM fallback chain builder (v4 `65f5021c8`,
//! `lib/memory/cheap-llm-tasks/fallback.ts`), tier-2 over a real DB.
//!
//! Drives v4's REAL `buildCheapFallbackSelections` and diffs
//! `quilltap_core::services::cheap_llm_fallback::build_cheap_fallback_selections`
//! against it, comparing the SELECTIONS — provider, model, baseUrl,
//! connectionProfileId, isLocal and the resolved `profileParams`. That last one
//! is the point: the chain speaks connection profiles and the cheap path speaks
//! selections, and the conversion is where the two currencies meet.
//!
//! Both sides run against a copy of the same fixture and apply the SAME wiring
//! statements first (the committed settings fixture has no chain, no cheap
//! profile, and `allowCheapFallback` unset). The Rust side replays them through
//! its own repositories so the world under test is built the same way, not
//! assumed.
//!
//! ⚠ **Two vacuity traps this case had to walk around, both measured:**
//!
//! 1. The profile-less arm stands in a SYNTHETIC failed profile with no model
//!    class, and `tierMatches` reads unknown-vs-KNOWN as a non-match in both
//!    directions — so a *classified* cheap profile is never drafted and the
//!    switch's two positions both answer `[]`. The cheap profile's `modelClass`
//!    is cleared before those arms.
//! 2. v4's `jest.setup.ts` mocks the WHOLE DB stack, and `safeQuery` swallows
//!    the resulting failure: every seeding write quietly no-ops and the case
//!    goes green having measured a world it never built. The oracle un-mocks
//!    the manager, the repositories and the factory.
//!
//! Generate the fixture + oracle (Node 24, from the v4 checkout; jest ignores
//! `.claude/` paths, so the case is staged in a /tmp mirror):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   TMPO=/tmp/qt-cheapfallback-oracle
//!   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
//!   cp "$V5W/harness/oracle/cases/cheap-llm-fallback.test.ts" "$TMPO/cases/"
//!   cp "$V5W/harness/oracle/fixtures/settings.json"           "$TMPO/fixtures/"
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_SETTINGS_MAIN=/tmp/qt-cheapfallback-fixture.db \
//!     $N/node --import tsx $V5W/harness/oracle/fixtures/build-settings-fixture.ts
//!   QT_FIXTURE_CHEAP_FALLBACK=/tmp/qt-cheapfallback-fixture.db \
//!   QT_ORACLE_OUT=/tmp/oracle-cheap-llm-fallback.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=120000 \
//!       --roots "$PWD" --roots "$TMPO/cases" -- "cheap-llm-fallback\.test\.ts$"
//!   cp "$V5W/harness/oracle/cases/cheap-llm-refusal.test.ts" "$TMPO/cases/"
//!   cp "$V5W/harness/oracle/fixtures/cheap-llm-refusal.json"   "$TMPO/fixtures/"
//!   QT_REFUSAL_LEDGER_SPEC=cheap-llm-refusal.json QT_FIXTURE_OUT=/tmp/qt-cheap-refusal.db \
//!     $N/npx tsx $V5W/harness/oracle/fixtures/build-refusal-ledger-fixture.ts
//!   QT_FIXTURE_CHEAP_REFUSAL=/tmp/qt-cheap-refusal.db \
//!   QT_ORACLE_OUT=/tmp/oracle-cheap-llm-refusal.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=120000 \
//!       --roots "$PWD" --roots "$TMPO/cases" -- "cases/cheap-llm-refusal\.test\.ts$"
//! Run:
//!   QT_ORACLE_CHEAP_FALLBACK=/tmp/oracle-cheap-llm-fallback.ndjson \
//!   QT_FIXTURE_CHEAP_FALLBACK=/tmp/qt-cheapfallback-fixture.db \
//!   QT_ORACLE_CHEAP_REFUSAL=/tmp/oracle-cheap-llm-refusal.ndjson \
//!   QT_FIXTURE_CHEAP_REFUSAL=/tmp/qt-cheap-refusal.db \
//!     cargo test -p quilltap-harness --test cheap_llm_fallback_equivalence -- --nocapture
//!
//! ## P4.D225 — the cheap-LLM refusal record (`cheap_llm_refusal_matches_oracle`)
//!
//! v4 `49059fb14` puts a STATED empty-body refusal from a cheap task on the
//! chat's refusal ledger at four placements (the uncensored retry throws /
//! answers empty / answers; no retry at all). The second test drives v4's REAL
//! `executeCheapLLMTask` (`cheap-llm-refusal.test.ts`, only the provider and the
//! API-key step canned) against v5's `CheapLlmTaskExecutor::execute` with the
//! same script, on the refusal-ledger fixture shape built from
//! `cheap-llm-refusal.json`: per case the task result and every
//! `ConciergeRefusal` / `ConciergeRefusalLedger` line, then the `chats` +
//! `chat_messages` dumps (one arm earns the auto-switch). Arms: the four
//! placements, an UNSTATED empty body (classified, never recorded), a non-empty
//! body with a safety reason (never classified), no chat, no profile id, and a
//! profile id absent from the available list (the `${provider} ${modelName}`
//! name fallback).

use std::path::PathBuf;
use std::sync::Mutex;

mod common;

use quilltap_core::cheap_llm::CheapLlmSelection;
use quilltap_core::cheap_llm::{CheapLlmProfile, UncensoredFallbackOptions};
use quilltap_core::db::chat_settings::zod_parse_concierge_settings;
use quilltap_core::db::dump_table_json_conn;
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::db::Writer;
use quilltap_core::model::completion::{
    CompletionError, CompletionMessage, CompletionParams, CompletionProvider, CompletionResponse,
};
use quilltap_core::model::completion_provider::execute_completion;
use quilltap_core::model::provider_error::text_http_refusal;
use quilltap_core::model::transport::{
    BoxFuture, ProviderTransport, StreamBytes, TransportError, TransportPolicy, TransportRequest,
    TransportResponse,
};
use quilltap_core::services::cheap_llm_exec::{
    CheapLlmLogConfig, CheapLlmTaskExecutor, CheapLlmTaskOptions,
};
use quilltap_core::services::cheap_llm_fallback::{
    build_cheap_fallback_selections, CheapFallbackRequest,
};
use quilltap_core::services::dangerous_content::resolver::resolve_stored_concierge_settings;
use quilltap_core::services::llm_logging::LogContext;
use quilltap_core::test_support::captured_with;
use serde_json::{json, Value};

/// The settings family's pepper — this case reuses its fixture, and that family
/// keys with a DIFFERENT throwaway key from the restore family's.
const TEST_PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";

fn spec_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../harness/oracle/fixtures/settings.json")
}

fn selection_json(s: &CheapLlmSelection) -> Value {
    json!({
        "provider": s.provider,
        "modelName": s.model_name,
        "baseUrl": s.base_url,
        "connectionProfileId": s.connection_profile_id,
        "isLocal": s.is_local,
        "profileParameters": s.profile_parameters,
    })
}

/// v4 emits the selection object as `JSON.stringify` renders it: an `undefined`
/// key is DROPPED, and `profileParameters` is absent when `profileParams`
/// returns nothing. v5's `Option`s render as `null`, so the comparison folds
/// null and absent together on BOTH sides rather than on one.
fn normalize(mut v: Value) -> Value {
    if let Some(obj) = v.as_object_mut() {
        obj.retain(|_, val| !val.is_null());
    }
    v
}

fn selection_from_oracle(v: &Value) -> CheapLlmSelection {
    CheapLlmSelection {
        provider: v["provider"].as_str().unwrap_or("").to_string(),
        model_name: v["modelName"].as_str().unwrap_or("").to_string(),
        base_url: v["baseUrl"].as_str().map(str::to_string),
        connection_profile_id: v["connectionProfileId"].as_str().map(str::to_string),
        is_local: v["isLocal"].as_bool().unwrap_or(false),
        profile_parameters: v.get("profileParameters").cloned().filter(|p| !p.is_null()),
    }
}

#[test]
fn cheap_llm_fallback_matches_oracle() {
    let (Ok(oracle_path), Ok(fixture)) = (
        std::env::var("QT_ORACLE_CHEAP_FALLBACK"),
        std::env::var("QT_FIXTURE_CHEAP_FALLBACK"),
    ) else {
        eprintln!(
            "SKIP: set QT_ORACLE_CHEAP_FALLBACK + QT_FIXTURE_CHEAP_FALLBACK (see the header)."
        );
        return;
    };
    let raw = std::fs::read_to_string(&oracle_path).unwrap_or_else(|e| panic!("read oracle: {e}"));
    let cases: Vec<Value> = raw
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("oracle line is JSON"))
        .collect();
    assert!(!cases.is_empty(), "oracle produced no cases");

    let spec: Value =
        serde_json::from_str(&std::fs::read_to_string(spec_path()).expect("read settings.json"))
            .expect("settings.json parses");
    let user_a = spec["userA"].as_str().expect("userA").to_string();
    let gpt = spec["profiles"]["gpt"].as_str().expect("gpt").to_string();
    let claude = spec["profiles"]["claude"]
        .as_str()
        .expect("claude")
        .to_string();

    let work = std::env::temp_dir().join(format!("qt-cheap-fallback-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&work);
    std::fs::copy(&fixture, &work).unwrap_or_else(|e| panic!("copy fixture: {e}"));

    // The SAME wiring statements the oracle applied, in the same order.
    {
        let w = Writer::open_writable(&work, TEST_PEPPER).expect("open fixture copy");
        let repo = quilltap_core::db::connection_profiles::ConnectionProfilesRepository::new(
            w.connection(),
        );
        repo.update(
            &gpt,
            &quilltap_core::db::connection_profiles::CpUpdate {
                fallback_profile_id: Some(Some(claude.clone())),
                allow_tier_fallback: Some(true),
                model_class: Some("Standard".into()),
                updated_at: "2020-01-01T00:00:00.000Z".into(),
                ..Default::default()
            },
        )
        .expect("wire the primary's chain");
        repo.update(
            &claude,
            &quilltap_core::db::connection_profiles::CpUpdate {
                is_cheap: Some(true),
                model_class: Some("Standard".into()),
                updated_at: "2020-01-01T00:00:00.000Z".into(),
                ..Default::default()
            },
        )
        .expect("make the understudy cheap");
    }

    let db = Db::open(
        DbPaths {
            main: work.clone(),
            mount_index: None,
            llm_logs: None,
        },
        TEST_PEPPER,
    )
    .unwrap_or_else(|e| panic!("open fixture copy: {e}"));

    let mut failed: Vec<String> = Vec::new();
    let mut nonempty = 0usize;

    for case in &cases {
        let name = case["name"].as_str().expect("case name");

        // The two statements the oracle interleaves between arms, replayed at
        // the same points: the switch, and the modelClass clear.
        if let Some(allow) = case["allowCheapFallback"].as_bool() {
            set_allow_cheap_fallback(&work, &user_a, allow);
        }
        if name == "profileless_switch_off" {
            let w = Writer::open_writable(&work, TEST_PEPPER).expect("open fixture copy");
            quilltap_core::db::connection_profiles::ConnectionProfilesRepository::new(
                w.connection(),
            )
            .update(
                &claude,
                &quilltap_core::db::connection_profiles::CpUpdate {
                    clear_model_class: true,
                    updated_at: "2020-01-01T00:00:00.000Z".into(),
                    ..Default::default()
                },
            )
            .expect("clear the cheap profile's model class");
        }

        let selection = selection_from_oracle(&case["selection"]);
        let already_tried: Vec<String> = case["alreadyTried"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();

        let got = build_cheap_fallback_selections(
            &db,
            CheapFallbackRequest {
                selection: &selection,
                user_id: &user_a,
                dangerous: case["dangerous"].as_bool().unwrap_or(false),
                already_tried,
                task_type: Some("oracle"),
            },
        );

        let got_json: Vec<Value> = got.iter().map(|s| normalize(selection_json(s))).collect();
        let want: Vec<Value> = case["selections"]
            .as_array()
            .expect("selections")
            .iter()
            .cloned()
            .map(normalize)
            .collect();
        if !want.is_empty() {
            nonempty += 1;
        }
        if got_json != want {
            failed.push(format!(
                "{name}:\n    rust   {}\n    oracle {}",
                Value::Array(got_json),
                Value::Array(want)
            ));
        }
    }

    let _ = std::fs::remove_file(&work);

    assert!(
        failed.is_empty(),
        "{} of {} case(s) failed:\n{}",
        failed.len(),
        cases.len(),
        failed.join("\n")
    );
    // Shape assertion: an all-empty corpus proves only that both sides refuse.
    assert!(
        nonempty >= 3,
        "only {nonempty} case(s) produced a stand-in — the corpus has gone vacuous \
         (see the two traps in the header)"
    );
    eprintln!(
        "OK cheap-llm fallback: {} cases ({nonempty} producing a stand-in)",
        cases.len()
    );
}

/// The oracle sets the switch through v4's repository; v5 writes the same cell.
fn set_allow_cheap_fallback(main: &std::path::Path, user_id: &str, value: bool) {
    let w = Writer::open_writable(main, TEST_PEPPER).expect("open fixture copy");
    let conn = w.connection();
    let row = quilltap_core::db::chat_settings::find_by_user_id(conn, user_id)
        .expect("read chat settings")
        .expect("the settings fixture seeds userA's chat_settings row");
    let mut bag = row
        .get("cheapLLMSettings")
        .cloned()
        .unwrap_or_else(|| json!({}));
    bag.as_object_mut()
        .expect("cheapLLMSettings is an object")
        .insert("allowCheapFallback".into(), json!(value));
    let id = row["id"].as_str().expect("settings id");
    conn.execute(
        "UPDATE chat_settings SET cheapLLMSettings = ?1 WHERE id = ?2",
        rusqlite::params![serde_json::to_string(&bag).unwrap(), id],
    )
    .expect("write the switch");
}

// ---------------------------------------------------------------------------
// P4.D225 — the cheap-LLM refusal record
// ---------------------------------------------------------------------------

const REFUSAL_SEED_TS: &str = "2020-01-01T00:00:00.000Z";

/// P4.118: the cases whose task FAILS on a `throwsHttp` step, so v4's and v5's
/// error strings differ by the §S.5 message bytes (see the loop).
const MESSAGE_BYTES_CASES: &[&str] = &["uncoded-400-not-fallback-eligible"];

/// The case's script, one step per provider call — v4's canned
/// `createLLMProvider` twin.
struct ScriptedProvider {
    steps: Mutex<Vec<Value>>,
}

/// P4.118: a transport answering one posed non-2xx exactly as
/// `ReqwestTransport` renders it — so a `throwsHttp` step's error is built by
/// the REAL `execute_completion` composition (the refusal side included), not
/// by the test.
struct PosedHttpFailure {
    status: u16,
    body: String,
}

impl PosedHttpFailure {
    fn error(&self) -> TransportError {
        TransportError {
            message: format!("HTTP {}: {}", self.status, self.body),
            status: Some(self.status),
        }
    }
}

impl ProviderTransport for PosedHttpFailure {
    fn execute<'a>(
        &'a self,
        _request: &'a TransportRequest,
        _policy: &'a TransportPolicy,
    ) -> BoxFuture<'a, Result<TransportResponse, TransportError>> {
        let e = self.error();
        Box::pin(async move { Err(e) })
    }

    fn execute_stream<'a>(
        &'a self,
        _request: &'a TransportRequest,
        _policy: &'a TransportPolicy,
    ) -> BoxFuture<'a, Result<tokio::sync::mpsc::Receiver<StreamBytes>, TransportError>> {
        let e = self.error();
        Box::pin(async move { Err(e) })
    }
}

impl CompletionProvider for ScriptedProvider {
    async fn send_message(
        &self,
        provider: &str,
        _base_url: Option<&str>,
        params: &CompletionParams,
    ) -> Result<CompletionResponse, CompletionError> {
        let step = {
            let mut steps = self.steps.lock().unwrap();
            assert!(!steps.is_empty(), "the case script ran out");
            steps.remove(0)
        };
        if let Some(msg) = step.get("throws").and_then(Value::as_str) {
            return Err(CompletionError::new(msg));
        }
        if let Some(http) = step.get("throwsHttp") {
            let transport = PosedHttpFailure {
                status: http["status"].as_u64().unwrap() as u16,
                body: http["body"].as_str().unwrap().to_string(),
            };
            return match execute_completion(
                &transport,
                provider,
                None,
                "canned-test-key",
                params,
                &TransportPolicy::default(),
                "Quilltap/test",
                None,
                None,
            )
            .await
            {
                Ok(_) => panic!("a posed non-2xx must fail"),
                Err(e) => Err(e),
            };
        }
        Ok(CompletionResponse {
            content: step["content"].as_str().unwrap().to_string(),
            finish_reason: step
                .get("finishReason")
                .and_then(Value::as_str)
                .map(str::to_string),
            usage: None,
            attachment_results: None,
            cache_usage: None,
        })
    }
}

fn snake(k: &str) -> String {
    let mut out = String::new();
    for c in k.chars() {
        if c.is_ascii_uppercase() {
            out.push('_');
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

fn render_v4_refusal_log(log: &Value) -> String {
    let target = match log["service"].as_str().unwrap() {
        "ConciergeRefusal" => "quilltap::concierge_refusal",
        "ConciergeRefusalLedger" => "quilltap::concierge_refusal_ledger",
        other => panic!("unexpected service {other}"),
    };
    let level = log["level"].as_str().unwrap().to_uppercase();
    let mut line = format!("{level} {target} {}", log["message"].as_str().unwrap());
    for (k, v) in log["bag"].as_object().unwrap() {
        let rendered = match v {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        line.push_str(&format!(" {}={rendered}", snake(k)));
    }
    line
}

fn refusal_rows(dump: &Value) -> Vec<Value> {
    let mut rows = dump["rows"].as_array().expect("dump rows").clone();
    for row in rows.iter_mut() {
        let obj = row.as_object_mut().unwrap();
        for col in [
            "updatedAt",
            "lastMessageAt",
            "dangerClassifiedAt",
            "lastModerationRefusalAt",
            "createdAt",
        ] {
            if obj
                .get(col)
                .and_then(Value::as_str)
                .is_some_and(|s| s != REFUSAL_SEED_TS)
            {
                obj.insert(col.into(), json!("<ts>"));
            }
        }
        if obj.contains_key("chatId") {
            obj.insert("id".into(), json!("<id>"));
        }
        for v in obj.values_mut() {
            if let Some(f) = v.as_f64() {
                if v.is_f64() && f.fract() == 0.0 {
                    *v = json!(f as i64);
                }
            }
        }
    }
    rows
}

#[test]
fn cheap_llm_refusal_matches_oracle() {
    let (Ok(oracle_path), Ok(fixture)) = (
        std::env::var("QT_ORACLE_CHEAP_REFUSAL"),
        std::env::var("QT_FIXTURE_CHEAP_REFUSAL"),
    ) else {
        eprintln!("SKIP: set QT_ORACLE_CHEAP_REFUSAL + QT_FIXTURE_CHEAP_REFUSAL (see the header).");
        return;
    };
    let spec: Value = serde_json::from_str(
        &std::fs::read_to_string(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../harness/oracle/fixtures/cheap-llm-refusal.json"),
        )
        .expect("read spec"),
    )
    .expect("parse spec");
    let oracle: Vec<Value> = std::fs::read_to_string(&oracle_path)
        .expect("read oracle")
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("oracle line is JSON"))
        .collect();
    let cases = spec["cases"].as_array().unwrap();
    assert_eq!(
        oracle.len(),
        cases.len() + 3,
        "the policy + one row per case + two dumps"
    );

    // v4 `3b463d6b1` (#76): the fallback's Concierge policy, resolved from the
    // spec's stored `conciergeSettings` (no chat, as the task executor is
    // handed it) — the whole policy compared against v4's recorded bytes.
    let (stored, issues) = zod_parse_concierge_settings(&spec["concierge"]);
    assert!(issues.is_empty(), "concierge settings parse");
    let policy = resolve_stored_concierge_settings(Some(&stored), None);
    assert_eq!(
        serde_json::to_value(&policy).unwrap(),
        oracle[0]["conciergePolicy"],
        "the resolved Concierge policy"
    );
    let profiles: Vec<CheapLlmProfile> = spec["connectionProfiles"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| CheapLlmProfile {
            id: p["id"].as_str().unwrap().into(),
            provider: p["provider"].as_str().unwrap().into(),
            model_name: p["modelName"].as_str().unwrap().into(),
            is_dangerous_compatible: p["isDangerousCompatible"].as_bool() == Some(true),
            ..Default::default()
        })
        .collect();

    let dir = tempfile::tempdir().expect("tempdir");
    let main = dir.path().join("main.db");
    let ll = dir.path().join("llm-logs.db");
    std::fs::copy(&fixture, &main).expect("copy fixture");
    let pepper = spec["testPepperBase64"].as_str().unwrap().to_string();
    common::materialize_llm_logs(&ll, &pepper);

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let db = rt.block_on(async {
        Db::open(
            DbPaths {
                main: main.clone(),
                mount_index: None,
                llm_logs: Some(ll.clone()),
            },
            &pepper,
        )
        .expect("open fixture copy")
    });
    let plants: Vec<(String, i64, String)> = spec["ledgerPlants"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            (
                p["chatId"].as_str().unwrap().into(),
                p["count"].as_i64().unwrap(),
                p["lastAt"].as_str().unwrap().into(),
            )
        })
        .collect();
    rt.block_on(db.write(move |w| {
        quilltap_core::test_support::ensure_p4d225_columns(w.main().connection());
        for (id, count, at) in &plants {
            w.main().connection().execute(
                "UPDATE chats SET \"moderationRefusalCount\" = ?1, \"lastModerationRefusalAt\" = ?2 WHERE id = ?3",
                rusqlite::params![count, at, id],
            )?;
        }
        Ok(())
    }))
    .expect("plant the ledgers");

    let user_id = spec["userId"].as_str().unwrap().to_string();
    let mut failed: Vec<String> = Vec::new();
    for (case, want) in cases.iter().zip(&oracle[1..]) {
        let name = case["name"].as_str().unwrap();
        assert_eq!(want["name"], case["name"], "oracle rows in spec order");
        let sel = &case["selection"];
        let selection = CheapLlmSelection {
            provider: sel["provider"].as_str().unwrap().into(),
            model_name: sel["modelName"].as_str().unwrap().into(),
            connection_profile_id: sel
                .get("connectionProfileId")
                .and_then(Value::as_str)
                .map(str::to_string),
            base_url: None,
            is_local: false,
            profile_parameters: None,
        };
        let provider = ScriptedProvider {
            steps: Mutex::new(case["script"].as_array().unwrap().clone()),
        };
        let executor = CheapLlmTaskExecutor::with_logging(CheapLlmLogConfig {
            db: db.clone(),
            user_id: user_id.clone(),
            chat_id: case["chatId"].as_str().map(str::to_string),
            message_id: None,
            ctx: LogContext::none(),
        });
        let uncensored = UncensoredFallbackOptions {
            concierge_policy: &policy,
            available_profiles: &profiles,
            is_dangerous_chat: Some(false),
        };
        let (result, lines) = captured_with(|| {
            rt.block_on(executor.execute(
                &provider,
                &selection,
                vec![CompletionMessage::user("Summarize the scene.")],
                |content: &str| content.to_string(),
                (case["uncensored"].as_bool() == Some(true)).then_some(&uncensored),
                None,
                None,
                Some("oracle-cheap-refusal"),
                CheapLlmTaskOptions::default(),
            ))
        });
        assert!(
            provider.steps.lock().unwrap().is_empty(),
            "{name}: scripted step(s) unused"
        );
        let mut got_result = json!({
            "success": result.success,
            "result": result.result,
            "error": result.error,
        });
        // P4.118 (§S.5): a failed `throwsHttp` task surfaces v5's transport
        // bytes (`HTTP {status}: {body}`) where v4 surfaces its SDK error's
        // rendering — the one message the order leaves unchanged. The refusal
        // SIDE carries v4's rendering, so v4's string must be exactly the side's
        // message; pinned both ways by case name.
        let first_http = case["script"][0].get("throwsHttp");
        let bytes_diverge = MESSAGE_BYTES_CASES.contains(&name);
        if let Some(http) = first_http.filter(|_| bytes_diverge) {
            let status = http["status"].as_u64().unwrap() as u16;
            let body = http["body"].as_str().unwrap();
            let v5_bytes = format!("HTTP {status}: {body}");
            let v4_rendering = text_http_refusal(&selection.provider, status, body)
                .expect("known provider")
                .message;
            assert_eq!(
                got_result["error"],
                json!(v5_bytes),
                "{name}: v5's task error is the transport's bytes"
            );
            assert_eq!(
                want["result"]["error"],
                json!(v4_rendering),
                "{name}: v4's task error is the side's rendering"
            );
            got_result["error"] = want["result"]["error"].clone();
        }
        let got_lines: Vec<String> = lines
            .into_iter()
            .filter(|l| {
                matches!(
                    l.split(' ').nth(1),
                    Some("quilltap::concierge_refusal" | "quilltap::concierge_refusal_ledger")
                )
            })
            .collect();
        let want_lines: Vec<String> = want["logs"]
            .as_array()
            .unwrap()
            .iter()
            .map(render_v4_refusal_log)
            .collect();
        if got_result != want["result"] || got_lines != want_lines {
            failed.push(format!(
                "{name}:\n    rust   {got_result} {got_lines:#?}\n    oracle {} {want_lines:#?}",
                want["result"]
            ));
        }
    }

    let got_chats = db
        .read_main(|c| dump_table_json_conn(c, "chats", "id"))
        .expect("dump chats");
    let got_msgs = db
        .read_main(|c| dump_table_json_conn(c, "chat_messages", "chatId"))
        .expect("dump chat_messages");
    drop(db);
    drop(rt);

    assert!(
        failed.is_empty(),
        "{} of {} case(s) failed:\n{}",
        failed.len(),
        cases.len(),
        failed.join("\n")
    );
    let tables = &oracle[cases.len() + 1..];
    assert_eq!(
        refusal_rows(&got_chats),
        refusal_rows(&tables[0]),
        "chats rows diverge"
    );
    let want_msgs = refusal_rows(&tables[1]);
    assert_eq!(
        want_msgs.len(),
        1,
        "the switch arm posts exactly one bubble"
    );
    assert_eq!(
        refusal_rows(&got_msgs),
        want_msgs,
        "chat_messages rows diverge"
    );
    eprintln!("OK cheap-llm refusal: {} cases", cases.len());
}
