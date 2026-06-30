//! Tier-2 differential test: v4's `ChatTokenTrackingOps` (Phase-2, the
//! conversation capstone, sub-unit 6 — `incrementTokenAggregates` /
//! `resetTokenAggregates`).
//!
//! Both sides run the SAME op sequence (`chats-tokens-tier2.json`) on a fresh
//! copy of the seed fixture (two chats seeded via v4's real `repos.chats.create`
//! — one starting at zero counters, one with pinned non-zero
//! totals/cost/priceSource for the reset), then the `chats` table is dumped
//! canonically and the post-op state asserted identical.
//!
//! Exercises: an increment WITH cost (sets `estimatedCostUSD` + `priceSource`,
//! bumps `updatedAt`, accumulates the counters); a SECOND increment with cost
//! (accumulates onto the first — `current + cost`, counters add, priceSource
//! overwritten); an increment with `estimatedCost = null` (only counters +
//! `updatedAt`, no cost/priceSource change); a reset (counters → 0,
//! `estimatedCostUSD` → null, `updatedAt` PRESERVED); and an increment on a
//! missing chat (no-op).
//!
//! NORMALIZATION (applied identically to both dumps): `updatedAt` is collapsed to
//! `<ts>` ONLY when it differs from the seed sentinel — so the reset chat (which
//! preserves `updatedAt`) stays pinned and is diffed exactly (proving it did NOT
//! mint), while an incremented chat's bumped `updatedAt` is normalized away. Ids,
//! `createdAt`, and the token columns (`totalPromptTokens` /
//! `totalCompletionTokens` / `estimatedCostUSD` / `priceSource`) are diffed
//! EXACTLY — that's the increment math + the cost accumulation + the
//! reset-to-null.
//!
//! Generate the oracle output + fixture (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-chtok-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-chats-tokens-fixture.ts
//!   QT_FIXTURE_CHTOK=/tmp/qt-chtok-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/chats-tokens-tier2.ts > /tmp/oracle-chtok.ndjson
//! Run:
//!   QT_ORACLE_CHTOK=/tmp/oracle-chtok.ndjson \
//!   QT_FIXTURE_CHTOK=/tmp/qt-chtok-fixture.db \
//!     cargo test -p quilltap-harness --test chats_tokens_tier2_equivalence

use quilltap_core::db::Writer;
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    #[serde(rename = "seedTimestamp")]
    seed_timestamp: String,
    ops: Vec<Op>,
}

#[derive(Deserialize)]
#[serde(tag = "kind")]
enum Op {
    #[serde(rename = "increment")]
    Increment {
        #[serde(rename = "chatId")]
        chat_id: String,
        #[serde(rename = "promptTokens")]
        prompt_tokens: f64,
        #[serde(rename = "completionTokens")]
        completion_tokens: f64,
        #[serde(rename = "estimatedCost")]
        estimated_cost: Option<f64>,
        #[serde(default, rename = "priceSource")]
        price_source: Option<String>,
    },
    #[serde(rename = "reset")]
    Reset {
        #[serde(rename = "chatId")]
        chat_id: String,
    },
}

/// Collapse `updatedAt` to `<ts>` (non-null, non-sentinel only). A reset
/// preserves the sentinel, so it stays pinned and is diffed exactly.
fn normalize(dump: &mut Value, sentinel: &str) {
    let Some(rows) = dump.get_mut("rows").and_then(Value::as_array_mut) else {
        return;
    };
    for row in rows.iter_mut() {
        let Some(obj) = row.as_object_mut() else {
            continue;
        };
        let mint = obj
            .get("updatedAt")
            .and_then(Value::as_str)
            .is_some_and(|s| s != sentinel);
        if mint {
            obj.insert("updatedAt".to_string(), Value::String("<ts>".to_string()));
        }
    }
}

#[test]
fn chats_tokens_tier2_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_CHTOK") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_CHTOK to the oracle NDJSON (see header).");
            return;
        }
    };
    let fixture = match std::env::var("QT_FIXTURE_CHTOK") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_FIXTURE_CHTOK to the seed fixture .db (see header).");
            return;
        }
    };

    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../harness/oracle/fixtures/chats-tokens-tier2.json"),
        )
        .unwrap_or_else(|e| panic!("read spec: {e}")),
    )
    .expect("parse spec");

    let mut oracle: Value = serde_json::from_str(
        std::fs::read_to_string(&oracle_path)
            .unwrap_or_else(|e| panic!("read oracle: {e}"))
            .trim(),
    )
    .expect("parse oracle dump");

    let work = std::env::temp_dir().join(format!("qt-chtok-rust-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&work);
    std::fs::copy(&fixture, &work).unwrap_or_else(|e| panic!("copy fixture: {e}"));

    let writer = Writer::open_writable(&work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open fixture copy: {e}"));
    {
        let repo = writer.chat_tokens();
        for op in &spec.ops {
            match op {
                Op::Increment {
                    chat_id,
                    prompt_tokens,
                    completion_tokens,
                    estimated_cost,
                    price_source,
                } => {
                    repo.increment_token_aggregates(
                        chat_id,
                        *prompt_tokens,
                        *completion_tokens,
                        *estimated_cost,
                        price_source.as_deref(),
                    )
                    .expect("increment_token_aggregates");
                }
                Op::Reset { chat_id } => {
                    repo.reset_token_aggregates(chat_id)
                        .expect("reset_token_aggregates");
                }
            }
        }
    }

    let mut got = writer.dump_table_json("chats", "id").expect("dump chats");
    let _ = std::fs::remove_file(&work);

    normalize(&mut got, &spec.seed_timestamp);
    normalize(&mut oracle["chats"], &spec.seed_timestamp);

    assert_eq!(got["table"], oracle["chats"]["table"], "chats: table name");
    assert_eq!(
        got["columns"], oracle["chats"]["columns"],
        "chats: column set / order"
    );
    assert_eq!(
        got["rows"], oracle["chats"]["rows"],
        "chats: row state diverged\n  rust:   {}\n  oracle: {}",
        got["rows"], oracle["chats"]["rows"]
    );

    // Guard against an empty-vs-empty false pass.
    assert_ne!(got["rows"], json!([]), "expected non-empty chats dump");

    eprintln!("OK: chats tokens tier-2 matched oracle.");
}
