//! Tier-3 differential: the API-path `EmbeddingProvider` (P4.1a deliverable 4 —
//! `quilltap_core::services::embedding_provider::ApiEmbeddingProvider`, the port
//! of v4 `generateEmbeddingForUser` / `generateEmbedding` / `generateApiEmbedding`
//! / `getApiKeyForProfile`).
//!
//! Both sides run the same case list from the committed spec
//! (`harness/oracle/fixtures/embedding-provider-tier3.json`) over a COPY of the
//! same baked fixture (embedding profiles + api keys + a fitted BUILTIN
//! vocabulary, seeded through v4's REAL repos). The v4 oracle
//! (`harness/oracle/cases/embedding-provider-tier3.test.ts`) drives the REAL
//! embedding service with ONLY `global.fetch` scripted (which also intercepts the
//! bundled OpenRouter SDK), recording every wire request (method / url / raw body
//! string) and the exact response text served. This test registers a
//! `CannedWireTransport` FROM that recorded wire — so a Rust request-building
//! divergence (url, body bytes, a skipped `/api/show`, a wrong `num_ctx`)
//! surfaces as a loud canned miss — runs the same calls through
//! `ApiEmbeddingProvider`, and diffs each `EmbeddingResult` / `EmbeddingError`
//! EXACTLY (the BUILTIN vectors too: the idf is loaded from identical stored
//! bytes, so no `ln` is computed at query time).
//!
//! Generate the fixture + oracle (Node 24, from the v4 checkout; jest cannot see
//! test files under a `.claude/` worktree, so copy the oracle case + spec to a
//! mirror dir first — the `../fixtures` relative spec read must survive):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5=<this checkout>
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_EMBED_MAIN=/tmp/qt-embed-main.db \
//!   QT_FIXTURE_EMBED_MOUNT=/tmp/qt-embed-mount.db \
//!     $N/node --import tsx $V5/harness/oracle/fixtures/build-embedding-provider-fixture.ts
//!   mkdir -p /tmp/qt-embed-oracle/{cases,fixtures}
//!   cp $V5/harness/oracle/cases/embedding-provider-tier3.test.ts /tmp/qt-embed-oracle/cases/
//!   cp $V5/harness/oracle/fixtures/embedding-provider-tier3.json /tmp/qt-embed-oracle/fixtures/
//!   QT_FIXTURE_EMBED_MAIN=/tmp/qt-embed-main.db \
//!   QT_FIXTURE_EMBED_MOUNT=/tmp/qt-embed-mount.db \
//!   QT_ORACLE_OUT=/tmp/oracle-embedding-provider.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=120000 \
//!       --roots "$PWD" --roots /tmp/qt-embed-oracle/cases -- embedding-provider-tier3
//! Run:
//!   QT_ORACLE_EMBEDDING_PROVIDER=/tmp/oracle-embedding-provider.ndjson \
//!   QT_FIXTURE_EMBED_MAIN=/tmp/qt-embed-main.db \
//!     cargo test -p quilltap-harness --test embedding_provider_tier3_equivalence

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::model::embedding::{EmbeddingPriority, EmbeddingProvider};
use quilltap_core::model::wire::{CannedWireTransport, WireResponse};
use quilltap_core::services::embedding_provider::ApiEmbeddingProvider;
use serde_json::{json, Map, Value};

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/embedding-provider-tier3.json")
}

/// Render an f64 the way V8 `JSON.stringify` does, for the compare: an
/// integer-valued float collapses to a JSON integer (`0`, not `0.0`) so the
/// serde_json `Number` variants align with the oracle's parsed text (the
/// `js_number_to_json` rule). The VALUES still compare exactly.
fn js_num(f: f64) -> Value {
    if f.is_finite() && f.fract() == 0.0 && f.abs() <= 9.007_199_254_740_992e15 {
        json!(f as i64)
    } else {
        json!(f)
    }
}

/// One oracle NDJSON row.
struct OracleRow {
    case: String,
    /// The `result` or `error` object (exactly one present).
    outcome: Value,
    /// The recorded wire calls: {method, url, body, status, statusText, responseBody}.
    wire: Vec<Value>,
}

fn load_oracle(path: &str) -> Vec<OracleRow> {
    let text = std::fs::read_to_string(path).expect("read oracle ndjson");
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|line| {
            let v: Value = serde_json::from_str(line).expect("oracle row json");
            let case = v["case"].as_str().expect("case id").to_string();
            let outcome = if let Some(r) = v.get("result") {
                json!({ "result": r })
            } else {
                json!({ "error": v.get("error").cloned().expect("result or error") })
            };
            let wire = v["wire"].as_array().cloned().unwrap_or_default();
            OracleRow {
                case,
                outcome,
                wire,
            }
        })
        .collect()
}

#[test]
fn embedding_provider_matches_v4() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_EMBEDDING_PROVIDER") else {
        eprintln!(
            "skipping embedding_provider_tier3_equivalence: QT_ORACLE_EMBEDDING_PROVIDER unset"
        );
        return;
    };
    let fixture_main = std::env::var("QT_FIXTURE_EMBED_MAIN")
        .expect("QT_FIXTURE_EMBED_MAIN must point at the built main fixture");

    let spec: Value =
        serde_json::from_str(&std::fs::read_to_string(spec_path()).expect("read spec"))
            .expect("spec json");
    let pepper = spec["testPepperBase64"].as_str().expect("pepper");
    let users: HashMap<String, String> = spec["users"]
        .as_object()
        .expect("users")
        .iter()
        .map(|(k, v)| (k.clone(), v.as_str().unwrap().to_string()))
        .collect();

    let oracle = load_oracle(&oracle_path);
    let cases = spec["cases"].as_array().expect("cases");
    assert_eq!(
        cases.len(),
        oracle.len(),
        "spec has {} cases but the oracle recorded {}",
        cases.len(),
        oracle.len()
    );

    // One copy for the whole run — the embedding path only READS.
    let work =
        std::env::temp_dir().join(format!("qt-embed-provider-tier3-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&work);
    std::fs::copy(&fixture_main, &work).expect("copy fixture");
    let db = Db::open(
        DbPaths {
            main: work.clone(),
            mount_index: None,
            llm_logs: None,
        },
        pepper,
    )
    .expect("open fixture copy");

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    for (case, row) in cases.iter().zip(oracle.iter()) {
        let id = case["id"].as_str().unwrap();
        assert_eq!(id, row.case, "case order drift between spec and oracle");
        let user_id = users[case["user"].as_str().unwrap()].clone();
        let profile_id = case["profileId"].as_str().map(str::to_string);
        let text = case["text"].as_str().unwrap();
        let priority = match case.get("priority").and_then(Value::as_str) {
            Some("background") => EmbeddingPriority::Background,
            _ => EmbeddingPriority::Interactive,
        };

        // Register the canned transport FROM the oracle-recorded wire: the raw
        // key is the exact (method, url, body-bytes) v4 sent, the response the
        // exact text v4 was served. A Rust request divergence = canned miss.
        let mut transport = CannedWireTransport::new();
        for w in &row.wire {
            transport = transport.with_response(
                w["method"].as_str().unwrap(),
                w["url"].as_str().unwrap(),
                w["body"].as_str().unwrap(),
                WireResponse {
                    status: u16::try_from(w["status"].as_i64().unwrap()).unwrap(),
                    status_text: w["statusText"].as_str().unwrap().to_string(),
                    body: w["responseBody"].as_str().unwrap().to_string(),
                },
            );
        }

        // A fresh provider per case: v4's module-global numCtxCache is
        // equivalent here because every case uses a distinct (baseUrl, model)
        // pair except the deliberate cache exercises inside one case.
        let provider = ApiEmbeddingProvider::new(db.clone(), transport);

        let got = match rt.block_on(provider.generate_embedding_for_user(
            text,
            &user_id,
            profile_id.as_deref(),
            priority,
        )) {
            Ok(r) => {
                let embedding: Vec<Value> =
                    r.embedding.iter().map(|&x| js_num(f64::from(x))).collect();
                json!({ "result": {
                    "embedding": embedding,
                    "model": r.model,
                    "dimensions": r.dimensions,
                    "provider": r.provider,
                }})
            }
            Err(e) => {
                let mut m = Map::new();
                m.insert("message".into(), json!(e.message));
                m.insert(
                    "provider".into(),
                    e.provider.map(Value::String).unwrap_or(Value::Null),
                );
                json!({ "error": Value::Object(m) })
            }
        };

        assert_eq!(
            got,
            row.outcome,
            "case {id} mismatch\n got: {}\nwant: {}",
            serde_json::to_string_pretty(&got).unwrap(),
            serde_json::to_string_pretty(&row.outcome).unwrap()
        );
    }

    eprintln!(
        "OK: embedding_provider tier-3 — {} case(s) match v4 (results exact, wire canned-key-pinned).",
        cases.len()
    );
    drop(db);
    let _ = std::fs::remove_file(&work);
}
