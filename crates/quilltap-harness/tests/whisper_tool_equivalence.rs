//! Tier-2 differential test: the whisper tool handler (v4
//! `lib/tools/handlers/whisper-handler.ts` `executeWhisperTool` +
//! `formatWhisperResults`), ported as `quilltap_core::tools::whisper`.
//!
//! Both sides run the SAME whisper op sequence (`whisper-tool.json`) on a fresh
//! copy of the pre-seeded two-database fixture (seven real characters+vaults — so
//! `characters_read::find_by_id` overlays each name AND its aliases — and four
//! multi-character chats with pinned participant ids at mixed active/silent/absent
//! statuses to exercise `canReceiveWhisper`). Each op's Output JSON + the
//! `format_whisper` string are compared against v4's, and the `chat_messages`
//! table is dumped and diffed.
//!
//! What it proves: success-by-name and success-by-alias (case-insensitive)
//! resolution restricted to whisper-receivable (`active`/`silent`) participants;
//! a `silent` participant CAN receive; an `absent` participant is excluded from
//! both the match set and the "Available characters" list; the ambiguous-match,
//! not-found (with the available list), self-whisper, and invalid-input failures
//! (none of which write a row); and the `[[WHISPER ...]]` double-encoding
//! sanitizer. The written row's `participantId` (caller) + `targetParticipantIds`
//! (resolved target) verify the routing exactly.
//!
//! STOP-RULE: the whisper write is ONLY a `chat_messages` row (the ported
//! `chats_messages::add_message` path) — no post-office / SSE / broadcast side
//! effect. So this is a plain tier-2 write.
//!
//! NORMALIZATION: each successful whisper mints the message `id` + `createdAt`;
//! the dump (ordered by the pinned `chatId`, one whisper per success chat) remaps
//! `id` to a per-row token and collapses `createdAt` to `<ts>`. Everything else —
//! `chatId`, `participantId`, `targetParticipantIds`, `role`, `content`,
//! `attachments` — is pinned, so it diffs exactly.
//!
//! Generate the oracle output + fixture (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-whisper-main.db \
//!   QT_FIXTURE_MOUNT_OUT=/tmp/qt-whisper-mount.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-whisper-tool-fixture.ts
//!   QT_FIXTURE_WHISPER=/tmp/qt-whisper-main.db \
//!   QT_FIXTURE_WHISPER_MOUNT=/tmp/qt-whisper-mount.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/whisper-tool.ts > /tmp/oracle-whisper.ndjson
//! Run:
//!   QT_ORACLE_WHISPER=/tmp/oracle-whisper.ndjson \
//!   QT_FIXTURE_WHISPER=/tmp/qt-whisper-main.db \
//!   QT_FIXTURE_WHISPER_MOUNT=/tmp/qt-whisper-mount.db \
//!     cargo test -p quilltap-harness --test whisper_tool_equivalence

use quilltap_core::db::dump_table_json_conn;
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::tools::whisper::{execute_whisper, format_whisper};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    #[serde(rename = "userId")]
    user_id: String,
    #[serde(rename = "callingParticipantId")]
    calling_participant_id: String,
    ops: Vec<Op>,
}

#[derive(Deserialize)]
struct Op {
    name: String,
    #[serde(rename = "chatId")]
    chat_id: String,
    args: Value,
    expect: Value,
    #[serde(default)]
    formatted: Option<String>,
}

/// Pick the NDJSON line whose `table` field matches.
fn oracle_table(oracle_text: &str, table: &str) -> Value {
    for line in oracle_text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let v: Value = serde_json::from_str(line).expect("parse oracle ndjson line");
        if v.get("table").and_then(Value::as_str) == Some(table) {
            return v;
        }
    }
    panic!("oracle ndjson missing table {table}");
}

/// Remap the minted `id` to `<msg-N>` in row order (rows are already sorted by the
/// pinned `chatId`, at most one whisper per success chat) and collapse the minted
/// `createdAt` to `<ts>`. Every other column is pinned by the fixture.
fn normalize(dump: &mut Value, label: &str) {
    let rows = dump
        .get_mut("rows")
        .and_then(Value::as_array_mut)
        .unwrap_or_else(|| panic!("{label}: dump has no rows array"));
    for (i, row) in rows.iter_mut().enumerate() {
        let obj = row
            .as_object_mut()
            .unwrap_or_else(|| panic!("{label}: row is not an object"));
        if obj.contains_key("id") {
            obj.insert("id".to_string(), Value::String(format!("<msg-{i}>")));
        }
        if matches!(obj.get("createdAt"), Some(Value::String(_))) {
            obj.insert("createdAt".to_string(), Value::String("<ts>".to_string()));
        }
    }
}

#[tokio::test]
async fn whisper_tool_matches_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_WHISPER") else {
        eprintln!("SKIP: set QT_ORACLE_WHISPER to the oracle NDJSON (see header).");
        return;
    };
    let Ok(fixture_main) = std::env::var("QT_FIXTURE_WHISPER") else {
        eprintln!("SKIP: set QT_FIXTURE_WHISPER to the seed main .db (see header).");
        return;
    };
    let Ok(fixture_mount) = std::env::var("QT_FIXTURE_WHISPER_MOUNT") else {
        eprintln!("SKIP: set QT_FIXTURE_WHISPER_MOUNT to the seed mount-index .db (see header).");
        return;
    };

    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../harness/oracle/fixtures/whisper-tool.json"),
        )
        .unwrap_or_else(|e| panic!("read spec: {e}")),
    )
    .expect("parse spec");

    let oracle_text =
        std::fs::read_to_string(&oracle_path).unwrap_or_else(|e| panic!("read oracle: {e}"));

    // Extract the oracle's per-op returns for the Output/formatted diff.
    let returns = {
        let mut found = None;
        for line in oracle_text.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let v: Value = serde_json::from_str(line).expect("parse oracle ndjson line");
            if let Some(r) = v.get("returns") {
                found = Some(r.clone());
                break;
            }
        }
        found.expect("oracle ndjson missing returns line")
    };
    let returns = returns.as_array().expect("returns is array");

    let scratch = std::env::temp_dir().join(format!("qt-whisper-harness-{}", std::process::id()));
    std::fs::create_dir_all(&scratch).expect("scratch dir");
    let work_main = scratch.join("whisper-main.db");
    let work_mount = scratch.join("whisper-mount.db");
    let _ = std::fs::remove_file(&work_main);
    let _ = std::fs::remove_file(&work_mount);
    std::fs::copy(&fixture_main, &work_main).expect("copy main fixture");
    std::fs::copy(&fixture_mount, &work_mount).expect("copy mount fixture");

    let db = Db::open(
        DbPaths {
            main: work_main.clone(),
            mount_index: Some(work_mount.clone()),
            llm_logs: None,
        },
        &spec.test_pepper_base64,
    )
    .expect("open fixture instance");

    for (i, op) in spec.ops.iter().enumerate() {
        let output = execute_whisper(
            &db,
            &spec.user_id,
            &op.chat_id,
            &spec.calling_participant_id,
            &op.args,
        )
        .await;

        let output_json = serde_json::to_value(&output).expect("serialize output");
        assert_eq!(output_json, op.expect, "op[{i}] {} output", op.name);

        let formatted = format_whisper(&output);

        // Cross-check against the oracle's recorded return for this op.
        let orc = &returns[i];
        assert_eq!(
            orc.get("output"),
            Some(&op.expect),
            "op[{i}] {} oracle output vs spec",
            op.name
        );
        assert_eq!(
            orc.get("formatted").and_then(Value::as_str),
            Some(formatted.as_str()),
            "op[{i}] {} formatted vs oracle",
            op.name
        );
        if let Some(f) = &op.formatted {
            assert_eq!(&formatted, f, "op[{i}] {} formatted vs spec", op.name);
        }
    }

    let mut got = db
        .read_main(|conn| dump_table_json_conn(conn, "chat_messages", "chatId"))
        .expect("dump chat_messages");
    drop(db);
    let _ = std::fs::remove_file(&work_main);
    let _ = std::fs::remove_file(&work_mount);

    let mut want = oracle_table(&oracle_text, "chat_messages");

    normalize(&mut got, "rust chat_messages");
    normalize(&mut want, "oracle chat_messages");

    assert_eq!(
        got["columns"], want["columns"],
        "chat_messages column set / order"
    );
    assert_eq!(
        got["rows"], want["rows"],
        "chat_messages row state diverged\n  rust:   {}\n  oracle: {}",
        got["rows"], want["rows"]
    );

    // Sanity: exactly three whisper rows (the three success ops); the four failure
    // ops wrote nothing.
    assert_eq!(
        got["rows"].as_array().unwrap().len(),
        3,
        "expected three whisper rows"
    );

    eprintln!("OK: whisper tool tier-2 matched oracle (chat_messages + per-op outputs).");
}
