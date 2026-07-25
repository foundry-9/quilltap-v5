//! Tier-2 differential test: W4.6b — the conversation-summary vault mirror + the
//! `chats.delete` summary sweep (v4
//! `lib/file-storage/conversation-summary-vault-bridge.ts`; the sweep is driven
//! through `repos.chats.delete(id, { syncVaults })`).
//!
//! Both sides COPY the SAME two-DB seed fixture (a MAIN db with two vault-provisioned
//! `characters` + two `chats`, and a MOUNT-INDEX db with the store tables) and run
//! the SAME op sequence:
//!   1-3. `write_conversation_summary_to_vaults` (create, rename-in-place, distinct);
//!   4.   `delete_conversation_with_vault_sweep(chat1, sync_vaults = true)`  — sweep;
//!   5.   `delete_conversation_with_vault_sweep(chat2, sync_vaults = false)` — leave.
//! Then FIVE mount-index store tables are structural-diffed (doc_mount_points /
//! _folders / _files / _documents / _file_links). The Rust port drives the ported
//! bridge functions over a `Db`; v4 drives the REAL bridge + `repos.chats.delete`.
//!
//! Minted-values remap with ONE shared id-map across the five tables (points ->
//! folders -> files -> documents -> links, rows in natural-key order). Every
//! mount-index row id is minted (vault provisioning + the summary writes), so every
//! FK verifies by RELATIONSHIP. Timestamps -> `<ts>`; the link `chunkCount` diffs
//! EXACTLY since P4.6BK (v5 chunks on write); `doc_mount_chunks` is excluded.
//! Character/chat ids are PINNED in the spec, so they appear literally (identical
//! both sides) inside the `doc_mount_documents.content` frontmatter and are NOT
//! remapped — `content` is diffed byte-for-byte.
//!
//! Generate the oracle output + fixtures (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_VSM_MAIN=/tmp/qt-vsm-main.db \
//!   QT_FIXTURE_VSM_MOUNT=/tmp/qt-vsm-mount.db \
//!     $N/npx tsx $V5/harness/oracle/fixtures/build-vault-summary-mirror-fixture.ts
//!   QT_FIXTURE_VSM_MAIN=/tmp/qt-vsm-main.db \
//!   QT_FIXTURE_VSM_MOUNT=/tmp/qt-vsm-mount.db \
//!     $N/npx tsx $V5/harness/oracle/cases/vault-summary-mirror-tier2.ts > /tmp/oracle-vault-summary-mirror.ndjson
//! Run:
//!   QT_ORACLE_VSM=/tmp/oracle-vault-summary-mirror.ndjson \
//!   QT_FIXTURE_VSM_MAIN=/tmp/qt-vsm-main.db \
//!   QT_FIXTURE_VSM_MOUNT=/tmp/qt-vsm-mount.db \
//!     cargo test -p quilltap-harness --test vault_summary_mirror_tier2_equivalence

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use quilltap_core::db::dump_table_json_conn;
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::services::conversation_summary_vault_bridge::{
    delete_conversation_with_vault_sweep, write_conversation_summary_to_vaults,
    WriteConversationSummaryInput,
};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    writes: Vec<WriteSpec>,
    deletes: Vec<DeleteSpec>,
}

#[derive(Deserialize)]
struct WriteSpec {
    #[serde(rename = "chatId")]
    chat_id: String,
    #[serde(rename = "chatTitle")]
    chat_title: String,
    summary: String,
    #[serde(rename = "summaryGeneration")]
    summary_generation: i64,
    #[serde(rename = "participantCharacterIds")]
    participant_character_ids: Vec<String>,
    #[serde(rename = "messageCount")]
    message_count: i64,
    #[serde(rename = "firstMessageAt")]
    first_message_at: Option<String>,
    #[serde(rename = "lastMessageAt")]
    last_message_at: Option<String>,
    #[serde(rename = "updatedAt")]
    updated_at: String,
}

impl WriteSpec {
    fn into_input(self) -> WriteConversationSummaryInput {
        WriteConversationSummaryInput {
            chat_id: self.chat_id,
            chat_title: self.chat_title,
            summary: self.summary,
            summary_generation: self.summary_generation,
            participant_character_ids: self.participant_character_ids,
            message_count: self.message_count,
            first_message_at: self.first_message_at,
            last_message_at: self.last_message_at,
            updated_at: self.updated_at,
        }
    }
}

#[derive(Deserialize)]
struct DeleteSpec {
    #[serde(rename = "chatId")]
    chat_id: String,
    #[serde(rename = "syncVaults")]
    sync_vaults: bool,
}

/// Per-table normalization spec. The slice order is the canonical walk order for the
/// shared cross-table id-remap.
struct TableSpec {
    table: &'static str,
    oracle_key: &'static str,
    order_by: &'static str,
    id_columns: &'static [&'static str],
    ts_columns: &'static [&'static str],
    pin_chunk_count: bool,
}

const TABLES: &[TableSpec] = &[
    TableSpec {
        table: "doc_mount_points",
        oracle_key: "points",
        order_by: "name",
        id_columns: &["id"],
        ts_columns: &["createdAt", "updatedAt", "lastScannedAt"],
        pin_chunk_count: false,
    },
    TableSpec {
        table: "doc_mount_folders",
        oracle_key: "folders",
        order_by: "path",
        id_columns: &["id", "parentId", "mountPointId"],
        ts_columns: &["createdAt", "updatedAt"],
        pin_chunk_count: false,
    },
    TableSpec {
        table: "doc_mount_files",
        oracle_key: "files",
        order_by: "sha256",
        id_columns: &["id"],
        ts_columns: &["createdAt", "updatedAt"],
        pin_chunk_count: false,
    },
    TableSpec {
        table: "doc_mount_documents",
        oracle_key: "documents",
        order_by: "contentSha256",
        id_columns: &["id", "fileId"],
        ts_columns: &["createdAt", "updatedAt"],
        pin_chunk_count: false,
    },
    TableSpec {
        table: "doc_mount_file_links",
        oracle_key: "links",
        order_by: "relativePath",
        id_columns: &["id", "fileId", "folderId", "mountPointId"],
        ts_columns: &[
            "lastModified",
            "descriptionUpdatedAt",
            "createdAt",
            "updatedAt",
        ],
        pin_chunk_count: false, // P4.6BK: v5 chunks on write — chunkCount now diffs exactly
    },
];

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/vault-summary-mirror-tier2.json")
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
                let next = format!("ID_{}", id_map.len());
                let token = id_map.entry(raw.clone()).or_insert(next).clone();
                obj.insert((*col).to_string(), Value::String(token));
            }
        }
        for col in spec.ts_columns {
            if obj.get(*col).map(|v| !v.is_null()).unwrap_or(false) {
                obj.insert((*col).to_string(), Value::String("<ts>".to_string()));
            }
        }
        if spec.pin_chunk_count {
            obj.insert("chunkCount".to_string(), Value::String("<cc>".to_string()));
        }
    }
}

fn normalize_all(dumps: &mut [Value]) {
    let mut id_map: HashMap<String, String> = HashMap::new();
    for (i, spec) in TABLES.iter().enumerate() {
        normalize_table(&mut dumps[i], spec, &mut id_map);
    }
}

#[test]
fn vault_summary_mirror_tier2_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_VSM") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_VSM to the oracle NDJSON (see header).");
            return;
        }
    };
    let main_fixture = match std::env::var("QT_FIXTURE_VSM_MAIN") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_FIXTURE_VSM_MAIN to the main fixture .db (header).");
            return;
        }
    };
    let mount_fixture = match std::env::var("QT_FIXTURE_VSM_MOUNT") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_FIXTURE_VSM_MOUNT to the mount fixture .db (header).");
            return;
        }
    };

    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(spec_path()).unwrap_or_else(|e| panic!("read spec: {e}")),
    )
    .expect("parse spec");

    let oracle: Value = serde_json::from_str(
        std::fs::read_to_string(&oracle_path)
            .unwrap_or_else(|e| panic!("read oracle: {e}"))
            .trim(),
    )
    .expect("parse oracle dump");

    // Fresh copies so the shared seed fixtures stay pristine.
    let pid = std::process::id();
    let main_work = std::env::temp_dir().join(format!("qt-vsm-main-rust-{pid}.db"));
    let mount_work = std::env::temp_dir().join(format!("qt-vsm-mount-rust-{pid}.db"));
    let _ = std::fs::remove_file(&main_work);
    let _ = std::fs::remove_file(&mount_work);
    std::fs::copy(&main_fixture, &main_work).unwrap_or_else(|e| panic!("copy main: {e}"));
    std::fs::copy(&mount_fixture, &mount_work).unwrap_or_else(|e| panic!("copy mount: {e}"));

    let db = Db::open(
        DbPaths {
            main: main_work.clone(),
            mount_index: Some(mount_work.clone()),
            llm_logs: None,
        },
        &spec.test_pepper_base64,
    )
    .unwrap_or_else(|e| panic!("open db: {e}"));

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    // The op sequence under test.
    rt.block_on(async {
        for w in spec.writes {
            let input = w.into_input();
            write_conversation_summary_to_vaults(&db, &input).await;
        }
        for d in &spec.deletes {
            delete_conversation_with_vault_sweep(&db, &d.chat_id, d.sync_vaults)
                .await
                .unwrap_or_else(|e| panic!("delete {}: {e}", d.chat_id));
        }
    });

    // Dump the five mount-index store tables off a read-only pooled connection.
    let mut got: Vec<Value> = TABLES
        .iter()
        .map(|s| {
            let table = s.table;
            let order_by = s.order_by;
            db.read_mount_index(move |c| dump_table_json_conn(c, table, order_by))
                .unwrap_or_else(|e| panic!("dump {}: {e}", s.table))
        })
        .collect();

    drop(db);
    let _ = std::fs::remove_file(&main_work);
    let _ = std::fs::remove_file(&mount_work);

    let mut want: Vec<Value> = TABLES
        .iter()
        .map(|s| {
            oracle
                .get(s.oracle_key)
                .cloned()
                .unwrap_or_else(|| panic!("oracle missing dump for {}", s.oracle_key))
        })
        .collect();

    normalize_all(&mut got);
    normalize_all(&mut want);

    for (i, s) in TABLES.iter().enumerate() {
        assert_eq!(got[i]["table"], want[i]["table"], "{}: table name", s.table);
        assert_eq!(
            got[i]["columns"], want[i]["columns"],
            "{}: column set / order",
            s.table
        );
        assert_eq!(
            got[i]["rows"], want[i]["rows"],
            "{}: remapped row state diverged\n  rust:   {}\n  oracle: {}",
            s.table, got[i]["rows"], want[i]["rows"]
        );
    }

    // Sanity: the corpus produced the expected shape.
    let rows = |key: &str| {
        let i = TABLES.iter().position(|t| t.oracle_key == key).unwrap();
        got[i]["rows"].as_array().unwrap().clone()
    };
    assert_eq!(rows("points").len(), 2, "2 vault mount points");
    // Each vault: one surviving summary link ("Other.md" from chat2, sync off).
    let link_paths: Vec<String> = rows("links")
        .iter()
        .filter_map(|r| {
            r.get("relativePath")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .collect();
    let surviving_summaries = link_paths
        .iter()
        .filter(|p| p.starts_with("Conversation Summaries/"))
        .count();
    assert_eq!(
        surviving_summaries, 2,
        "exactly 2 surviving summary links (chat2 'Other.md' in each vault); chat1 swept"
    );

    eprintln!(
        "OK: vault-summary-mirror tier-2 matched oracle (5 mount-index tables, {} link rows).",
        rows("links").len()
    );
}
