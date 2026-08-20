//! Differential: the vault conversation-summary search's timeRange staging
//! (P4.d13 unit 4 — v4 `searchVaultConversationSummaries` vs
//! `quilltap_core::services::memory_recap::search_vault_conversation_summaries`).
//!
//! Both sides run the SAME case table over fresh copies of the committed
//! episodic-recall fixture (dated summaries + TF-IDF-embedded chunks). No
//! mocks: embeddings run through the fitted BUILTIN profile on both sides.
//! Covers the hard-slice arm (window hits ≥ limit), the starved ×1.3 boost arm
//! (the boost VISIBLY reorders/rescores), unparsable/inverted windows (plain
//! slice), limit 0, and the exclude filter.
//!
//! P4.D95 (v4 `870a57fa`) adds the **precomputed-embedding** arms — the reuse
//! seam that makes the per-turn conversation-summary cadence affordable. Three
//! shapes, each run on BOTH sides with the SAME vector (each side embeds it
//! through its own real builtin TF-IDF profile first, so the vector itself is a
//! transitive comparand):
//!   * `precomputed_same_query` — the vector for THIS query text: the result
//!     must equal the embed-it-yourself arm byte for byte.
//!   * `precomputed_other_query` — the vector for a DIFFERENT sentence while
//!     the `query` string stays put. The list must follow the VECTOR, not the
//!     text; a port that quietly re-embeds `query` scores the other ranking and
//!     the arm goes red.
//!   * `precomputed_wrong_dimension` — a 2-wide vector against a 3-wide corpus:
//!     the document-search guard yields an EMPTY list, not an error.
//!
//! Generate (Node 24, from the v4 checkout), against /tmp COPIES of the
//! COMMITTED episodic-recall fixture — never point a regen at the committed
//! `.db` files themselves, and never rebuild them: a rebuild mints fresh
//! UUIDs and invalidates every episodic family (P4.D32's sweep hazard):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   mkdir -p /tmp/qt-vault-conv-db
//!   cp $V5W/crates/quilltap-web/tests/fixtures/episodic-recall-main.db  /tmp/qt-vault-conv-db/
//!   cp $V5W/crates/quilltap-web/tests/fixtures/episodic-recall-mount.db /tmp/qt-vault-conv-db/
//!   cd ~/source/quilltap-server
//!   TZ=UTC QT_FIXTURE_ER_MAIN=/tmp/qt-vault-conv-db/episodic-recall-main.db \
//!   QT_FIXTURE_ER_MOUNT=/tmp/qt-vault-conv-db/episodic-recall-mount.db \
//!     $N/npx tsx $V5W/harness/oracle/cases/vault-conv-search.ts \
//!     > /tmp/oracle-vault-conv.ndjson
//! Run:
//!   QT_ORACLE_VAULT_CONV=/tmp/oracle-vault-conv.ndjson \
//!     cargo test -p quilltap-harness --test vault_conv_search_equivalence

use std::path::PathBuf;

use quilltap_core::db::characters_read;
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::model::embedding::{
    EmbeddingPriority, EmbeddingProvider, ErasedEmbeddingProvider,
};
use quilltap_core::model::wire::CannedWireTransport;
use quilltap_core::recall_tags::TimeWindow;
use quilltap_core::services::embedding_provider::ApiEmbeddingProvider;
use quilltap_core::services::memory_recap::search_vault_conversation_summaries;
use serde::Deserialize;
use serde_json::Value;

const EPS: f64 = 1e-12;

#[derive(Deserialize)]
struct FixtureSpec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    #[serde(rename = "userId")]
    user_id: String,
    #[serde(rename = "elowenId")]
    elowen_id: String,
}

#[derive(Deserialize)]
struct OracleRow {
    name: String,
    matches: Vec<Value>,
}

struct Case {
    name: &'static str,
    query: &'static str,
    limit: usize,
    time_range: Option<(&'static str, &'static str)>,
}

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../quilltap-web/tests/fixtures")
}

#[tokio::test]
async fn vault_conv_search_matches_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_VAULT_CONV") else {
        eprintln!("QT_ORACLE_VAULT_CONV not set — skipping vault-conv-search differential");
        return;
    };
    let oracle_rows: Vec<OracleRow> = std::fs::read_to_string(&oracle_path)
        .expect("read oracle NDJSON")
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("parse oracle row"))
        .collect();

    let fixture_spec: FixtureSpec = serde_json::from_str(
        &std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../harness/oracle/fixtures/episodic-recall.json"
        ))
        .expect("read fixture spec"),
    )
    .expect("parse fixture spec");

    // MIRRORS the oracle case table (vault-conv-search.ts CASES).
    let cases = vec![
        Case {
            name: "no_window",
            query: "the quay market at dawn",
            limit: 3,
            time_range: None,
        },
        Case {
            name: "window_hard",
            query: "the quay market at dawn",
            limit: 1,
            time_range: Some(("2026-06-01T00:00:00.000Z", "2026-06-03T23:59:59.999Z")),
        },
        Case {
            name: "window_starved_boost",
            query: "mending the sail lockers by the quay market at dawn",
            limit: 3,
            time_range: Some(("2026-06-01T00:00:00.000Z", "2026-06-07T23:59:59.999Z")),
        },
        Case {
            name: "window_unparsable",
            query: "the quay market at dawn",
            limit: 3,
            time_range: Some(("whenever", "2026-06-07T23:59:59.999Z")),
        },
        Case {
            name: "window_inverted",
            query: "the quay market at dawn",
            limit: 3,
            time_range: Some(("2026-06-08T00:00:00.000Z", "2026-06-01T00:00:00.000Z")),
        },
        Case {
            name: "limit_zero",
            query: "the quay market at dawn",
            limit: 0,
            time_range: None,
        },
    ];

    // One shared fresh copy (the search is read-only on this surface).
    let pid = std::process::id();
    let main = std::env::temp_dir().join(format!("qt-vconv-{pid}-main.db"));
    let mount = std::env::temp_dir().join(format!("qt-vconv-{pid}-mount.db"));
    let _ = std::fs::remove_file(&main);
    let _ = std::fs::remove_file(&mount);
    std::fs::copy(fixtures_dir().join("episodic-recall-main.db"), &main).unwrap();
    std::fs::copy(fixtures_dir().join("episodic-recall-mount.db"), &mount).unwrap();
    let db = Db::open(
        DbPaths {
            main: main.clone(),
            mount_index: Some(mount.clone()),
            llm_logs: None,
        },
        &fixture_spec.test_pepper_base64,
    )
    .expect("open fixture copy");

    let embedding = ErasedEmbeddingProvider::new(ApiEmbeddingProvider::new(
        db.clone(),
        CannedWireTransport::new(),
    ));
    let mount_id = {
        let cid = fixture_spec.elowen_id.clone();
        db.read_main(move |c| characters_read::find_by_id_raw(c, &cid))
            .expect("read character")
            .and_then(|c| {
                c.get("characterDocumentMountPointId")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .expect("elowen vault mount id")
    };

    let run_with = |query: &'static str,
                    limit: usize,
                    exclude: Option<String>,
                    window: Option<TimeWindow>,
                    precomputed: Option<Vec<f32>>| {
        let db = db.clone();
        let embedding = embedding.clone();
        let mount_id = mount_id.clone();
        let user_id = fixture_spec.user_id.clone();
        let elowen = fixture_spec.elowen_id.clone();
        async move {
            search_vault_conversation_summaries(
                &db,
                &embedding,
                &elowen,
                Some(&mount_id),
                query,
                &user_id,
                None,
                limit,
                exclude.as_deref(),
                window.as_ref(),
                precomputed.as_deref(),
            )
            .await
        }
    };
    let run =
        |query: &'static str, limit: usize, exclude: Option<String>, window: Option<TimeWindow>| {
            run_with(query, limit, exclude, window, None)
        };

    let to_value = |matches: &[quilltap_core::services::memory_recap::VaultConversationMatch]| {
        matches
            .iter()
            .map(|m| {
                serde_json::json!({
                    "conversationId": m.conversation_id,
                    "conversationTitle": m.conversation_title,
                    "relativePath": m.relative_path,
                    "score": m.score,
                    "firstMessageAt": m.first_message_at,
                    "lastMessageAt": m.last_message_at,
                })
            })
            .collect::<Vec<Value>>()
    };

    let assert_matches = |name: &str, got: &[Value], want: &[Value]| {
        assert_eq!(got.len(), want.len(), "{name}: match count");
        for (i, (g, w)) in got.iter().zip(want).enumerate() {
            for key in [
                "conversationId",
                "conversationTitle",
                "relativePath",
                "firstMessageAt",
                "lastMessageAt",
            ] {
                assert_eq!(g.get(key), w.get(key), "{name}[{i}].{key}");
            }
            let gs = g.get("score").and_then(Value::as_f64).unwrap_or(f64::NAN);
            let ws = w.get("score").and_then(Value::as_f64).unwrap_or(f64::NAN);
            assert!(
                (gs - ws).abs() <= EPS,
                "{name}[{i}].score diverges: rust={gs} oracle={ws}"
            );
        }
    };

    // The baseline (for the exclude arm's id) mirrors the oracle's sequence.
    let baseline = run("the quay market at dawn", 3, None, None).await;
    let exclude_id = baseline
        .first()
        .map(|m| m.conversation_id.clone())
        .unwrap_or_else(|| "none".to_string());

    let mut oracle_iter = oracle_rows.iter();
    for case in &cases {
        let window = case.time_range.map(|(from, to)| TimeWindow {
            from: from.to_string(),
            to: to.to_string(),
        });
        let got = run(case.query, case.limit, None, window).await;
        let oracle = oracle_iter.next().expect("oracle row missing");
        assert_eq!(case.name, oracle.name, "case order mismatch");
        assert_matches(case.name, &to_value(&got), &oracle.matches);
    }
    // The exclude arm.
    let got = run("the quay market at dawn", 3, Some(exclude_id), None).await;
    let oracle = oracle_iter.next().expect("exclude oracle row missing");
    assert_eq!(oracle.name, "exclude_top");
    assert_matches("exclude_top", &to_value(&got), &oracle.matches);

    // ---------------------------------------------------------------------
    // P4.D95: the precomputed-embedding arms.
    // ---------------------------------------------------------------------
    let embed = |text: &'static str| {
        let embedding = embedding.clone();
        let user_id = fixture_spec.user_id.clone();
        async move {
            embedding
                .generate_embedding_for_user(text, &user_id, None, EmbeddingPriority::Interactive)
                .await
                .expect("embed the precomputed query")
                .embedding
        }
    };

    // (1) the vector for THIS query — identical to embedding it inside.
    let same_vec = embed("the quay market at dawn").await;
    let got = run_with(
        "the quay market at dawn",
        3,
        None,
        None,
        Some(same_vec.clone()),
    )
    .await;
    let oracle = oracle_iter
        .next()
        .expect("precomputed_same_query oracle row missing");
    assert_eq!(oracle.name, "precomputed_same_query");
    assert_matches("precomputed_same_query", &to_value(&got), &oracle.matches);

    // (2) the vector for a DIFFERENT sentence: the list follows the VECTOR.
    let other_vec = embed("mending the sail lockers").await;
    assert_ne!(
        same_vec, other_vec,
        "the two probe sentences must embed differently or the arm proves nothing"
    );
    let got = run_with("the quay market at dawn", 3, None, None, Some(other_vec)).await;
    let oracle = oracle_iter
        .next()
        .expect("precomputed_other_query oracle row missing");
    assert_eq!(oracle.name, "precomputed_other_query");
    assert_matches("precomputed_other_query", &to_value(&got), &oracle.matches);

    // (3) a mismatched width — the document-search guard empties the list.
    let got = run_with(
        "the quay market at dawn",
        3,
        None,
        None,
        Some(vec![0.1_f32, 0.2]),
    )
    .await;
    let oracle = oracle_iter
        .next()
        .expect("precomputed_wrong_dimension oracle row missing");
    assert_eq!(oracle.name, "precomputed_wrong_dimension");
    assert_matches(
        "precomputed_wrong_dimension",
        &to_value(&got),
        &oracle.matches,
    );
    assert!(
        got.is_empty(),
        "a mismatched-width vector must yield an EMPTY list (v4's document-search guard)"
    );

    assert!(
        oracle_iter.next().is_none(),
        "unconsumed oracle rows — the NDJSON is newer than this case table"
    );

    drop(db);
    let _ = std::fs::remove_file(&main);
    let _ = std::fs::remove_file(&mount);
    println!(
        "vault_conv_search_equivalence: {} cases green",
        cases.len() + 4
    );
}
