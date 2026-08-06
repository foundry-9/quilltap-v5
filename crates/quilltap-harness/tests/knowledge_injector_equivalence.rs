//! Read-differential test: the **per-turn knowledge injector**
//! (`quilltap_core::services::knowledge_injector::retrieve_knowledge_for_turn` — v4
//! `retrieveKnowledgeForTurn`).
//!
//! Both sides READ the same pre-seeded mount-index fixture (four tier stores +
//! Knowledge/ files via `linkDocumentContent` + chunks with embeddings) and run the
//! same query corpus with the SAME canned query embedding injected (the oracle via a
//! `jest.mock` of `generateEmbeddingForUser`, the Rust side via a
//! `CannedEmbeddingProvider`). Read-only → **zero normalization**: the
//! `{ content, tokenCount, debug }` result is compared byte-for-byte.
//!
//! Corpus banks: a multi-tier query hitting character (inline + file-level dedup) +
//! group + project (character_read:false DROP) + global (pdf → pointer-only), a
//! tight-budget query that demotes the inline entry to a pointer, empty-query /
//! zero-budget early exits, an embedding-failure early exit, and a minScore that
//! filters everything.
//!
//! Generate the oracle output + fixture (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
//!   TMPO=/tmp/qt-knowledge-injector-oracle
//!   rm -rf "$TMPO"; mkdir -p "$TMPO/harness/oracle/cases" "$TMPO/harness/oracle/fixtures"
//!   cp "$V5/harness/oracle/cases/knowledge-injector.test.ts" "$TMPO/harness/oracle/cases/"
//!   cp "$V5/harness/oracle/fixtures/knowledge-injector.json" "$TMPO/harness/oracle/fixtures/"
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-knowledge-injector-fixture.db \
//!     $N/npx tsx $V5/harness/oracle/fixtures/build-knowledge-injector-fixture.ts
//!   QT_FIXTURE_KNOWLEDGE=/tmp/qt-knowledge-injector-fixture.db \
//!   QT_ORACLE_OUT=/tmp/oracle-knowledge-injector.ndjson \
//!     $N/npx jest --silent --watchman=false --roots "$PWD" --roots "$TMPO/harness/oracle/cases" \
//!       -- "knowledge-injector\.test\.ts$"
//! Run:
//!   QT_ORACLE_KNOWLEDGE=/tmp/oracle-knowledge-injector.ndjson \
//!   QT_FIXTURE_KNOWLEDGE=/tmp/qt-knowledge-injector-fixture.db \
//!     cargo test -p quilltap-harness --test knowledge_injector_equivalence

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::model::embedding::CannedEmbeddingProvider;
use quilltap_core::services::knowledge_injector::{
    retrieve_knowledge_for_turn, KnowledgeRetrievalParams,
};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    #[serde(rename = "userId")]
    user_id: String,
    #[serde(rename = "cannedEmbeddings")]
    canned_embeddings: HashMap<String, Vec<f32>>,
    #[serde(rename = "cannedFailures")]
    canned_failures: Vec<String>,
    queries: Vec<Query>,
}

#[derive(Deserialize)]
struct Query {
    label: String,
    query: String,
    #[serde(rename = "characterMountPointId")]
    character_mount_point_id: Option<String>,
    #[serde(rename = "groupMountPointIds")]
    group_mount_point_ids: Vec<String>,
    #[serde(rename = "projectMountPointIds")]
    project_mount_point_ids: Vec<String>,
    #[serde(rename = "globalMountPointId")]
    global_mount_point_id: Option<String>,
    #[serde(rename = "budgetTokens")]
    budget_tokens: i64,
    #[serde(rename = "charsPerToken")]
    chars_per_token: f64,
    #[serde(default, rename = "minScore")]
    min_score: Option<f64>,
}

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/knowledge-injector.json")
}

/// Render an f64 the way `JSON.stringify` does: an integer-valued float becomes a
/// JSON integer (`1`, not `1.0`); otherwise a JSON float.
fn js_number(f: f64) -> Value {
    if f.is_finite() && f.fract() == 0.0 && f.abs() < 9e15 {
        Value::from(f as i64)
    } else {
        Value::from(f)
    }
}

fn oracle_line(oracle_text: &str, label: &str) -> Value {
    for line in oracle_text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let v: Value = serde_json::from_str(line).expect("parse oracle ndjson line");
        if v.get("label").and_then(Value::as_str) == Some(label) {
            return v;
        }
    }
    panic!("oracle ndjson missing label {label}");
}

#[tokio::test]
async fn knowledge_injector_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_KNOWLEDGE") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_KNOWLEDGE to the oracle NDJSON (see header).");
            return;
        }
    };
    let fixture = match std::env::var("QT_FIXTURE_KNOWLEDGE") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_FIXTURE_KNOWLEDGE to the seed fixture .db (see header).");
            return;
        }
    };

    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(spec_path()).unwrap_or_else(|e| panic!("read spec: {e}")),
    )
    .expect("parse spec");
    let oracle_text =
        std::fs::read_to_string(&oracle_path).unwrap_or_else(|e| panic!("read oracle: {e}"));

    // Fresh copy so the shared fixture stays pristine.
    let work = std::env::temp_dir().join(format!(
        "qt-knowledge-injector-rust-{}.db",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&work);
    std::fs::copy(&fixture, &work).unwrap_or_else(|e| panic!("copy fixture: {e}"));

    let mut provider = CannedEmbeddingProvider::new();
    for (text, vec) in &spec.canned_embeddings {
        provider = provider.with_vector(text.clone(), vec.clone());
    }
    for text in &spec.canned_failures {
        provider = provider.with_failure(text.clone());
    }

    // The injector only reads the mount-index partition; main is required by the
    // handle but never read here, so point it at the same file (read-only pool).
    let db = Db::open(
        DbPaths {
            main: work.clone(),
            mount_index: Some(work.clone()),
            llm_logs: None,
        },
        &spec.test_pepper_base64,
    )
    .unwrap_or_else(|e| panic!("open fixture copy: {e}"));

    for q in &spec.queries {
        let params = KnowledgeRetrievalParams {
            character_id: "char-ada".to_string(),
            user_id: spec.user_id.clone(),
            embedding_profile_id: None,
            query: q.query.clone(),
            character_mount_point_id: q.character_mount_point_id.clone(),
            group_mount_point_ids: q.group_mount_point_ids.clone(),
            project_mount_point_ids: q.project_mount_point_ids.clone(),
            global_mount_point_id: q.global_mount_point_id.clone(),
            budget_tokens: q.budget_tokens,
            chars_per_token: q.chars_per_token,
            min_score: q.min_score,
            candidate_limit: None,
            inline_token_threshold: None,
        };
        let result = retrieve_knowledge_for_turn(&db, &provider, &params)
            .await
            .unwrap_or_else(|e| panic!("query {}: {e:?}", q.label));

        // Serialize the debug array the same way the oracle emits it. `score` is a
        // JS number → an integer-valued float renders as `1`, not `1.0`
        // (JSON.stringify), so encode it via `js_number`.
        let debug: Vec<Value> = result
            .debug
            .iter()
            .map(|d| {
                json!({
                    "filePath": d.file_path,
                    "tier": d.tier.debug_key(),
                    "score": js_number(d.score),
                    "inline": d.inline,
                    "tokenCount": d.token_count,
                })
            })
            .collect();

        let want = oracle_line(&oracle_text, &q.label);
        assert_eq!(
            result.content,
            want.get("content").and_then(Value::as_str).unwrap_or(""),
            "query {} content diverged",
            q.label
        );
        assert_eq!(
            result.token_count,
            want.get("tokenCount").and_then(Value::as_i64).unwrap_or(-1),
            "query {} tokenCount diverged",
            q.label
        );
        assert_eq!(
            Value::Array(debug),
            *want.get("debug").unwrap_or(&Value::Null),
            "query {} debug diverged",
            q.label
        );
    }

    drop(db);
    let _ = std::fs::remove_file(&work);
    eprintln!("OK: knowledge-injector read-differential matched oracle.");
}
