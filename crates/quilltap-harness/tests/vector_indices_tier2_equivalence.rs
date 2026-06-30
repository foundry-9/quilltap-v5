//! Tier-2 differential test: the standalone two-table `vector_indices` repository
//! (`vector_indices` metadata + `vector_entries` Float32-BLOB rows).
//!
//! Both sides run the SAME op sequence (from the committed spec) against the SAME
//! seed fixture (empty seed — only the two table DDLs, materialized by v4's real
//! `ensureInitialized`), then BOTH tables are structural-diffed:
//!
//!   - `vector_indices` — `id` is PINNED (it equals the input `characterId`,
//!     not generated), so only the minted timestamps (`createdAt`/`updatedAt`)
//!     are placeholdered. `version` / `dimensions` are REAL-affinity numbers that
//!     render integer-valued via the dump's `js_number_to_json` (so `1.0` → `1`,
//!     `8.0` → `8`).
//!   - `vector_entries` — `id` is treated as MINTED (remapped to first-seen tokens
//!     in dump order, which is the deterministic `embedding`-hex order), the
//!     `createdAt` timestamp placeholdered, and the `embedding` BLOB compared as
//!     lowercase hex (bit-exact, mirrors `conversation_chunks` / `help_docs`).
//!     `characterId` is pinned.
//!
//! The corpus banks: saveMeta create (id=characterId, version=1, dimensions),
//! addEntry, addEntries (a single shared `createdAt` across the batch),
//! updateEntryEmbedding (only the embedding column — proven by the new hex),
//! saveMeta update (dimensions bumped + updatedAt re-minted, version/createdAt
//! preserved), removeEntries (the per-id loop), and deleteByCharacterId on a
//! second character (whose meta + entry are wiped, leaving no trace — the combined
//! delete).
//!
//! Generate the oracle output + fixture (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-vector-indices-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-vector-indices-fixture.ts
//!   QT_FIXTURE_VECTOR_INDICES=/tmp/qt-vector-indices-fixture.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/vector-indices.ts \
//!     > /tmp/oracle-vector-indices.ndjson
//! Run:
//!   QT_ORACLE_VECTOR_INDICES=/tmp/oracle-vector-indices.ndjson \
//!   QT_FIXTURE_VECTOR_INDICES=/tmp/qt-vector-indices-fixture.db \
//!     cargo test -p quilltap-harness --test vector_indices_tier2_equivalence

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use quilltap_core::db::vector_indices::VectorEntryInput;
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
struct EntryInput {
    id: String,
    #[serde(rename = "characterId")]
    character_id: String,
    embedding: Vec<f32>,
}

#[derive(Deserialize)]
#[serde(tag = "kind")]
enum Op {
    #[serde(rename = "saveMeta")]
    SaveMeta {
        #[serde(rename = "characterId")]
        character_id: String,
        dimensions: f64,
    },
    #[serde(rename = "deleteMetaByCharacterId")]
    DeleteMetaByCharacterId {
        #[serde(rename = "characterId")]
        character_id: String,
    },
    #[serde(rename = "addEntry")]
    AddEntry {
        id: String,
        #[serde(rename = "characterId")]
        character_id: String,
        embedding: Vec<f32>,
    },
    #[serde(rename = "addEntries")]
    AddEntries { entries: Vec<EntryInput> },
    #[serde(rename = "updateEntryEmbedding")]
    UpdateEntryEmbedding { id: String, embedding: Vec<f32> },
    #[serde(rename = "removeEntry")]
    RemoveEntry { id: String },
    #[serde(rename = "removeEntries")]
    RemoveEntries { ids: Vec<String> },
    #[serde(rename = "removeEntriesByCharacterId")]
    RemoveEntriesByCharacterId {
        #[serde(rename = "characterId")]
        character_id: String,
    },
    #[serde(rename = "deleteByCharacterId")]
    DeleteByCharacterId {
        #[serde(rename = "characterId")]
        character_id: String,
    },
}

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/vector-indices-tier2.json")
}

/// Placeholder minted timestamps in every row of a dump (no id remap).
fn placeholder_timestamps(dump: &mut Value, ts_columns: &[&str]) {
    let rows = dump
        .get_mut("rows")
        .and_then(Value::as_array_mut)
        .expect("dump has rows");
    for row in rows.iter_mut() {
        let obj = row.as_object_mut().expect("row is object");
        for col in ts_columns {
            if obj.get(*col).map(|v| !v.is_null()).unwrap_or(false) {
                obj.insert((*col).to_string(), Value::String("<ts>".into()));
            }
        }
    }
}

/// Remap the minted `id` column (first-seen tokens in dump order) + placeholder
/// timestamps. `characterId` / `embedding` stay literal (pinned / deterministic).
fn remap_ids_and_timestamps(dump: &mut Value, ts_columns: &[&str]) {
    let mut id_map: HashMap<String, String> = HashMap::new();
    let rows = dump
        .get_mut("rows")
        .and_then(Value::as_array_mut)
        .expect("dump has rows");
    for row in rows.iter_mut() {
        let obj = row.as_object_mut().expect("row is object");
        if let Some(Value::String(raw)) = obj.get("id") {
            let next = format!("ID_{}", id_map.len());
            let token = id_map.entry(raw.clone()).or_insert(next).clone();
            obj.insert("id".into(), Value::String(token));
        }
        for col in ts_columns {
            if obj.get(*col).map(|v| !v.is_null()).unwrap_or(false) {
                obj.insert((*col).to_string(), Value::String("<ts>".into()));
            }
        }
    }
}

/// Pick the NDJSON line whose `table` field matches `table` from a multi-line
/// oracle file.
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

#[test]
fn vector_indices_tier2_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_VECTOR_INDICES") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_VECTOR_INDICES to the oracle NDJSON (see header).");
            return;
        }
    };
    let fixture = match std::env::var("QT_FIXTURE_VECTOR_INDICES") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_FIXTURE_VECTOR_INDICES to the seed fixture .db (see header).");
            return;
        }
    };

    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(spec_path()).unwrap_or_else(|e| panic!("read spec: {e}")),
    )
    .expect("parse spec");
    let oracle_text =
        std::fs::read_to_string(&oracle_path).unwrap_or_else(|e| panic!("read oracle: {e}"));

    // Fresh copy so the shared seed fixture stays pristine.
    let work =
        std::env::temp_dir().join(format!("qt-vector-indices-rust-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&work);
    std::fs::copy(&fixture, &work).unwrap_or_else(|e| panic!("copy fixture: {e}"));

    // Run the SAME op sequence through the Rust port, minting our own timestamps.
    let writer = Writer::open_writable(&work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open fixture copy: {e}"));
    {
        let repo = writer.vector_indices();
        for op in &spec.ops {
            match op {
                Op::SaveMeta {
                    character_id,
                    dimensions,
                } => {
                    repo.save_meta(character_id, *dimensions)
                        .expect("save_meta");
                }
                Op::DeleteMetaByCharacterId { character_id } => {
                    repo.delete_meta_by_character_id(character_id)
                        .expect("delete_meta_by_character_id");
                }
                Op::AddEntry {
                    id,
                    character_id,
                    embedding,
                } => {
                    repo.add_entry(&VectorEntryInput {
                        id: id.clone(),
                        character_id: character_id.clone(),
                        embedding: Some(embedding.clone()),
                    })
                    .expect("add_entry");
                }
                Op::AddEntries { entries } => {
                    let inputs: Vec<VectorEntryInput> = entries
                        .iter()
                        .map(|e| VectorEntryInput {
                            id: e.id.clone(),
                            character_id: e.character_id.clone(),
                            embedding: Some(e.embedding.clone()),
                        })
                        .collect();
                    repo.add_entries(&inputs).expect("add_entries");
                }
                Op::UpdateEntryEmbedding { id, embedding } => {
                    repo.update_entry_embedding(id, Some(embedding))
                        .expect("update_entry_embedding");
                }
                Op::RemoveEntry { id } => {
                    repo.remove_entry(id).expect("remove_entry");
                }
                Op::RemoveEntries { ids } => {
                    repo.remove_entries(ids).expect("remove_entries");
                }
                Op::RemoveEntriesByCharacterId { character_id } => {
                    repo.remove_entries_by_character_id(character_id)
                        .expect("remove_entries_by_character_id");
                }
                Op::DeleteByCharacterId { character_id } => {
                    repo.delete_by_character_id(character_id)
                        .expect("delete_by_character_id");
                }
            }
        }
    }

    let mut got_meta = writer
        .dump_table_json("vector_indices", "id")
        .expect("dump vector_indices");
    let mut got_entries = writer
        .dump_table_json("vector_entries", "embedding")
        .expect("dump vector_entries");
    let _ = std::fs::remove_file(&work);

    let mut want_meta = oracle_table(&oracle_text, "vector_indices");
    let mut want_entries = oracle_table(&oracle_text, "vector_entries");

    // vector_indices: id is pinned (== characterId), only timestamps placeholdered.
    placeholder_timestamps(&mut got_meta, &["createdAt", "updatedAt"]);
    placeholder_timestamps(&mut want_meta, &["createdAt", "updatedAt"]);

    // vector_entries: id minted -> remap; createdAt placeholdered.
    remap_ids_and_timestamps(&mut got_entries, &["createdAt"]);
    remap_ids_and_timestamps(&mut want_entries, &["createdAt"]);

    assert_eq!(
        got_meta["columns"], want_meta["columns"],
        "vector_indices column set / order"
    );
    assert_eq!(
        got_meta["rows"], want_meta["rows"],
        "vector_indices row state diverged\n  rust:   {}\n  oracle: {}",
        got_meta["rows"], want_meta["rows"]
    );
    assert_eq!(
        got_entries["columns"], want_entries["columns"],
        "vector_entries column set / order"
    );
    assert_eq!(
        got_entries["rows"], want_entries["rows"],
        "vector_entries row state diverged\n  rust:   {}\n  oracle: {}",
        got_entries["rows"], want_entries["rows"]
    );

    // Sanity: CHAR_B was wiped entirely (combined delete), so only CHAR_A's meta
    // survives, with two surviving entries.
    let meta_rows = got_meta["rows"].as_array().expect("meta rows");
    assert_eq!(
        meta_rows.len(),
        1,
        "only CHAR_A meta survives (CHAR_B wiped)"
    );
    assert_eq!(
        meta_rows[0]["id"],
        Value::String("a0000000-0000-4000-8000-00000000000a".into()),
        "surviving meta is CHAR_A"
    );
    // version stays 1, dimensions bumped to 8 (integer-collapsed REAL).
    assert_eq!(meta_rows[0]["version"], Value::from(1));
    assert_eq!(meta_rows[0]["dimensions"], Value::from(8));

    let entry_rows = got_entries["rows"].as_array().expect("entry rows");
    assert_eq!(
        entry_rows.len(),
        2,
        "two CHAR_A entries survive (e...0003 removed, CHAR_B wiped)"
    );
    // The updated embedding hex is present ([0.25;4] -> 0000803e * 4).
    assert!(
        entry_rows
            .iter()
            .any(|r| r["embedding"] == Value::String("0000803e0000803e0000803e0000803e".into())),
        "updateEntryEmbedding result present"
    );

    eprintln!("OK: vector_indices tier-2 matched oracle (two tables, BLOB entries).");
}
