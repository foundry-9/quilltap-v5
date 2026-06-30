//! Tier-2 differential test: v4's `ChatsRepository` slim-row marshaling
//! (Phase-2, the conversation capstone, sub-unit 1 — create / update / delete).
//!
//! Both sides run the SAME op sequence (`chats-tier2.json`) on a fresh copy of
//! the empty-table seed fixture, then the `chats` table is dumped canonically and
//! the post-op state is asserted identical. Exercises the ~96-column surface:
//! the typed `participants` array column (incl. an integer-valued `talkativeness`
//! rendered as `1`), the JSON-array columns, the open-JSON `state` (single-key),
//! the number-affinity / boolean / enum / nullable columns, the
//! updatedAt-PRESERVED vs explicit-updatedAt `update` branches, and `delete`.
//!
//! NORMALIZATION: none. `update` never mints `updatedAt` (preserved unless the
//! caller passes one), so every id + timestamp is pinned on both sides.
//!
//! Generate the oracle output + fixture (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-chats-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-chats-fixture.ts
//!   QT_FIXTURE_CHATS=/tmp/qt-chats-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/chats-tier2.ts > /tmp/oracle-chats.ndjson
//! Run:
//!   QT_ORACLE_CHATS=/tmp/oracle-chats.ndjson \
//!   QT_FIXTURE_CHATS=/tmp/qt-chats-fixture.db \
//!     cargo test -p quilltap-harness --test chats_tier2_equivalence

use quilltap_core::db::chats::{ChatCreate, ChatUpdate, CreateOptions};
use quilltap_core::db::Writer;
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    ops: Vec<Op>,
}

#[derive(Deserialize)]
#[serde(tag = "kind")]
enum Op {
    #[serde(rename = "create")]
    Create {
        options: Opts,
        data: Box<ChatCreate>,
    },
    #[serde(rename = "update")]
    Update { id: String, data: PatchData },
    #[serde(rename = "delete")]
    Delete { id: String },
}

#[derive(Deserialize)]
struct Opts {
    id: String,
    #[serde(rename = "createdAt")]
    created_at: String,
    #[serde(rename = "updatedAt")]
    updated_at: String,
}

/// The sub-unit-1 update patch (matches `ChatUpdate`'s representative columns).
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct PatchData {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    context_summary: Option<String>,
    #[serde(default)]
    is_paused: Option<bool>,
    #[serde(default)]
    is_manually_renamed: Option<bool>,
    #[serde(default)]
    message_count: Option<f64>,
    #[serde(default)]
    danger_score: Option<f64>,
    #[serde(default)]
    chat_type: Option<String>,
    #[serde(default)]
    state: Option<Value>,
    #[serde(default)]
    tags: Option<Vec<String>>,
    #[serde(default, rename = "updatedAt")]
    updated_at: Option<String>,
}

impl PatchData {
    fn to_chat_update(&self) -> ChatUpdate {
        ChatUpdate {
            title: self.title.clone(),
            // The corpus only ever SETS these nullable columns (never to null),
            // so `Option<T>` → `Some(Some(T))`.
            context_summary: self.context_summary.clone().map(Some),
            is_paused: self.is_paused,
            is_manually_renamed: self.is_manually_renamed,
            message_count: self.message_count,
            danger_score: self.danger_score.map(Some),
            chat_type: self.chat_type.clone(),
            state: self.state.clone(),
            tags: self.tags.clone(),
            participants: None,
            impersonating_participant_ids: None,
            active_typing_participant_id: None,
            last_message_at: None,
            spoken_this_cycle_participant_ids: None,
            all_llm_pause_turn_count: None,
            total_prompt_tokens: None,
            total_completion_tokens: None,
            estimated_cost_usd: None,
            equipped_outfit: None,
            updated_at: self.updated_at.clone(),
        }
    }
}

#[test]
fn chats_tier2_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_CHATS") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_CHATS to the oracle NDJSON (see header).");
            return;
        }
    };
    let fixture = match std::env::var("QT_FIXTURE_CHATS") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_FIXTURE_CHATS to the seed fixture .db (see header).");
            return;
        }
    };

    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../harness/oracle/fixtures/chats-tier2.json"),
        )
        .unwrap_or_else(|e| panic!("read spec: {e}")),
    )
    .expect("parse spec");

    let oracle: Value = serde_json::from_str(
        std::fs::read_to_string(&oracle_path)
            .unwrap_or_else(|e| panic!("read oracle: {e}"))
            .trim(),
    )
    .expect("parse oracle dump");

    let work = std::env::temp_dir().join(format!("qt-chats-rust-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&work);
    std::fs::copy(&fixture, &work).unwrap_or_else(|e| panic!("copy fixture: {e}"));

    let writer = Writer::open_writable(&work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open fixture copy: {e}"));
    {
        let repo = writer.chats();
        for op in &spec.ops {
            match op {
                Op::Create { options, data } => {
                    repo.create(
                        data,
                        &CreateOptions {
                            id: options.id.clone(),
                            created_at: options.created_at.clone(),
                            updated_at: options.updated_at.clone(),
                        },
                    )
                    .expect("create");
                }
                Op::Update { id, data } => {
                    repo.update(id, &data.to_chat_update()).expect("update");
                }
                Op::Delete { id } => {
                    repo.delete(id).expect("delete");
                }
            }
        }
    }

    let got = writer.dump_table_json("chats", "id").expect("dump chats");
    let _ = std::fs::remove_file(&work);

    assert_eq!(got["table"], oracle["table"], "table name");
    assert_eq!(got["columns"], oracle["columns"], "column set / order");
    assert_eq!(
        got["rows"], oracle["rows"],
        "row state diverged\n  rust:   {}\n  oracle: {}",
        got["rows"], oracle["rows"]
    );

    let n = got["rows"].as_array().map(|a| a.len()).unwrap_or(0);
    assert!(n > 0, "dump looks empty");
    eprintln!("OK: chats tier-2 matched oracle ({n} rows).");
}
