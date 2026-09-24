//! P4.D222 — the help-section SIZE proof (v4 `492771aff`,
//! `__tests__/unit/lib/help/help-doc-size.test.ts`) without a tokenizer.
//!
//! v4's test holds every `help/*.md` section's embedding text — the only help
//! text ever sent to a provider since bug 168 — to
//! `HELP_SECTION_EMBEDDING_MAX_TOKENS` real `cl100k_base` tokens. v5 has no
//! `cl100k` tokenizer and does not take one (the order's Tier 3 item 11), so
//! the proof is split in two:
//!
//!   1. the oracle (`harness/oracle/cases/help-section-size.ts`) runs v4's REAL
//!      chunker and `js-tiktoken` over v4's tree at the pin and records every
//!      section's composed text and token count;
//!   2. this test slices the EMBEDDED tree (`quilltap-host`'s build-time copy)
//!      through the PRODUCTION sync into an in-memory table, composes each
//!      section with v5's `help_chunk_embedding_text`, and asserts the texts
//!      are v4's byte for byte — so v4's counts are the counts of v5's texts —
//!      then holds every count to the v5 constant with v4's failure message.
//!
//! Generate (from the v4 checkout, or the pinned worktree; needs `js-tiktoken`
//! in its `node_modules`):
//!   cd ~/source/quilltap-server
//!   npx tsx ~/source/quilltap-v5/harness/oracle/cases/help-section-size.ts \
//!     > /tmp/oracle-help-section-size.ndjson
//! Run:
//!   QT_ORACLE_HELP_SECTION_SIZE=/tmp/oracle-help-section-size.ndjson \
//!     cargo test -p quilltap-harness --test help_section_size_equivalence

use std::collections::BTreeMap;

use rusqlite::Connection;
use serde::Deserialize;

use quilltap_core::db::help_doc_chunks_repair::{HELP_DOCS_TABLE_DDL, HELP_DOC_CHUNKS_TABLE_DDL};
use quilltap_core::services::help_doc_chunking::{
    help_chunk_embedding_text, HELP_SECTION_EMBEDDING_MAX_TOKENS,
};
use quilltap_core::services::help_doc_sync::sync_help_docs;
use quilltap_host::files_store::embedded_help_source_files;

#[derive(Deserialize)]
#[serde(tag = "kind")]
enum Line {
    #[serde(rename = "meta")]
    Meta {
        #[serde(rename = "maxTokens")]
        max_tokens: usize,
        files: usize,
        sections: usize,
    },
    #[serde(rename = "section")]
    Section {
        file: String,
        #[serde(rename = "chunkIndex")]
        chunk_index: i64,
        heading: Option<String>,
        text: String,
        tokens: usize,
    },
}

#[test]
fn every_help_section_is_v4s_text_and_within_the_token_ceiling() {
    let Ok(path) = std::env::var("QT_ORACLE_HELP_SECTION_SIZE") else {
        eprintln!("SKIP: QT_ORACLE_HELP_SECTION_SIZE unset");
        return;
    };
    let text = std::fs::read_to_string(&path).expect("read oracle");
    let mut meta = None;
    let mut want: BTreeMap<(String, i64), (Option<String>, String, usize)> = BTreeMap::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        match serde_json::from_str::<Line>(line).expect("oracle line") {
            Line::Meta {
                max_tokens,
                files,
                sections,
            } => meta = Some((max_tokens, files, sections)),
            Line::Section {
                file,
                chunk_index,
                heading,
                text,
                tokens,
            } => {
                want.insert((file, chunk_index), (heading, text, tokens));
            }
        }
    }
    let (max_tokens, files, sections) = meta.expect("the oracle's meta line");
    assert_eq!(
        max_tokens, HELP_SECTION_EMBEDDING_MAX_TOKENS,
        "v5's ceiling is v4's"
    );
    assert!(
        files > 50,
        "v4's own guard: more than 50 help files measured"
    );
    assert_eq!(want.len(), sections, "a truncated oracle must not pass");

    // The production sync, into an in-memory table.
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(HELP_DOCS_TABLE_DDL).unwrap();
    conn.execute_batch(HELP_DOC_CHUNKS_TABLE_DDL).unwrap();
    conn.execute_batch(
        "CREATE TABLE embedding_status (id TEXT PRIMARY KEY, entityType TEXT, \
         entityId TEXT, profileId TEXT, status TEXT)",
    )
    .unwrap();
    let result = sync_help_docs(&conn, &embedded_help_source_files()).expect("sync");
    assert_eq!(result.failed, 0);
    assert_eq!(
        result.created, files,
        "every measured file is one synced doc"
    );

    let mut stmt = conn
        .prepare(
            "SELECT d.path, d.title, c.chunkIndex, c.heading, c.content \
             FROM help_doc_chunks c JOIN help_docs d ON d.id = c.docId",
        )
        .unwrap();
    let got: BTreeMap<(String, i64), (Option<String>, String)> = stmt
        .query_map([], |r| {
            let path: String = r.get(0)?;
            let title: String = r.get(1)?;
            let index: f64 = r.get(2)?;
            let heading: Option<String> = r.get(3)?;
            let content: String = r.get(4)?;
            let text = help_chunk_embedding_text(&title, heading.as_deref(), &content);
            let file = path.strip_prefix("help/").unwrap_or(&path).to_string();
            Ok(((file, index as i64), (heading, text)))
        })
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();

    assert_eq!(
        got.keys().collect::<Vec<_>>(),
        want.keys().collect::<Vec<_>>(),
        "the (file, section) set differs from v4's"
    );
    let mut text_diffs = Vec::new();
    let mut oversize = Vec::new();
    for (key, (heading, v4_text, tokens)) in &want {
        let (got_heading, v5_text) = &got[key];
        if got_heading != heading || v5_text != v4_text {
            text_diffs.push(format!("help/{} section {}", key.0, key.1));
        }
        if *tokens > HELP_SECTION_EMBEDDING_MAX_TOKENS {
            // v4's message, verbatim in shape.
            oversize.push(format!(
                "help/{} section {} ({}): {} tokens",
                key.0,
                key.1,
                heading.as_deref().unwrap_or("no heading"),
                tokens
            ));
        }
    }
    assert!(
        text_diffs.is_empty(),
        "v5 composes different section texts than v4 counted:\n{}",
        text_diffs.join("\n")
    );
    assert_eq!(oversize, Vec::<String>::new());
}
