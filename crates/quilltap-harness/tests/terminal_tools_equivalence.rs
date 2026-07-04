//! Read-differential test: v4's Prospero terminal tools (`terminal_read` /
//! `terminal_list`, W4.1d batch 1).
//!
//! Both sides READ the SAME baked fixture (three terminal_sessions created by
//! v4's real `repos.terminalSessions.create`, ids + timestamps pinned), run the
//! SAME cases, and the results compare **exactly** — NO normalization, because
//! nothing is mutated so no timestamp is ever minted.
//!
//! ## The scrollback-source seam
//!
//! v4's `executeTerminalReadTool` resolves the raw scrollback from a live PTY OR
//! from `tailFile(session.transcriptPath)`. There is no live PTY in the oracle,
//! so v4 tails the transcript file the fixture builder wrote. The Rust port takes
//! the resolved scrollback as an INJECTED input; this test reproduces v4's
//! resolution by reading the SAME transcript file (via the `transcriptPath` on the
//! row the port's `find_by_id` returns) and handing those bytes to
//! `execute_terminal_read`. Identical bytes on both sides.
//!
//! Generate the oracle output + fixture (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_TERMTOOLS=/tmp/qt-termtools.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-terminal-tools-fixture.ts
//!   QT_FIXTURE_TERMTOOLS=/tmp/qt-termtools.db \
//!     $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/terminal-tools.ts > /tmp/oracle-termtools.ndjson
//! Run:
//!   QT_ORACLE_TERMTOOLS=/tmp/oracle-termtools.ndjson \
//!   QT_FIXTURE_TERMTOOLS=/tmp/qt-termtools.db \
//!     cargo test -p quilltap-harness --test terminal_tools_equivalence
//!
//! (If better-sqlite3 fails to load in the oracle, rebuild its binding for Node 24
//! per `[[oracle-node-abi-gotcha]]`: `node-gyp rebuild` in the
//! `better-sqlite3-multiple-ciphers` package dir.)

use std::path::{Path, PathBuf};

use quilltap_core::db::runtime::Db;
use quilltap_core::db::terminal_sessions;
use quilltap_core::tools::terminal::{
    execute_terminal_list, execute_terminal_read, format_terminal_list, format_terminal_read,
    validate_terminal_list_input, validate_terminal_read_input, TerminalToolError,
};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    #[serde(rename = "readCases")]
    read_cases: Vec<ReadCase>,
    #[serde(rename = "readValidateCases")]
    read_validate_cases: Vec<ReadValidateCase>,
    #[serde(rename = "listCases")]
    list_cases: Vec<ListCase>,
}

#[derive(Deserialize)]
struct ReadCase {
    name: String,
    #[serde(rename = "chatId")]
    chat_id: String,
    args: Value,
    #[serde(default, rename = "expectThrow")]
    expect_throw: Option<String>,
}

#[derive(Deserialize)]
struct ReadValidateCase {
    name: String,
    args: Value,
}

#[derive(Deserialize)]
struct ListCase {
    name: String,
    #[serde(rename = "chatId")]
    chat_id: String,
}

// ---- Oracle shapes ----

#[derive(Deserialize)]
struct Oracle {
    #[serde(rename = "readResults")]
    read_results: Vec<OracleRead>,
    #[serde(rename = "readValidateResults")]
    read_validate_results: Vec<OracleValidate>,
    #[serde(rename = "listResults")]
    list_results: Vec<OracleList>,
}

#[derive(Deserialize)]
struct OracleRead {
    name: String,
    #[serde(default)]
    thrown: Option<String>,
    #[serde(default)]
    output: Option<Value>,
    #[serde(default)]
    formatted: Option<String>,
}

#[derive(Deserialize)]
struct OracleValidate {
    name: String,
    valid: bool,
}

#[derive(Deserialize)]
struct OracleList {
    name: String,
    valid: bool,
    output: Value,
    formatted: String,
}

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/terminal-tools-tier2.json")
}

/// Parse a spec `args` JSON object into the port's `TerminalReadInput` via the
/// port's own validator (the same gate the dispatcher applies). Panics if the
/// case's args are not valid (throw/success cases always carry valid args).
fn parse_read_args(args: &Value) -> quilltap_core::tools::terminal::TerminalReadInput {
    validate_terminal_read_input(args)
        .unwrap_or_else(|| panic!("read case args should validate: {args}"))
}

/// Resolve the raw scrollback the way v4's handler would: read the transcript
/// file recorded on the session row. Mirrors v4's `tailFile(transcriptPath)` for
/// a <1 MB file (whole-file read).
fn resolve_scrollback(db: &Db, session_id: &str) -> String {
    let path = db
        .read_main(|conn| terminal_sessions::find_by_id(conn, session_id))
        .expect("find_by_id")
        .and_then(|row| row.transcript_path);
    match path {
        Some(p) => std::fs::read_to_string(&p).unwrap_or_default(),
        None => String::new(),
    }
}

#[tokio::test]
async fn terminal_tools_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_TERMTOOLS") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_TERMTOOLS to the oracle NDJSON (see header).");
            return;
        }
    };
    let fixture = match std::env::var("QT_FIXTURE_TERMTOOLS") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_FIXTURE_TERMTOOLS to the fixture .db (header).");
            return;
        }
    };

    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(spec_path()).unwrap_or_else(|e| panic!("read spec: {e}")),
    )
    .expect("parse spec");
    let oracle: Oracle = serde_json::from_str(
        std::fs::read_to_string(&oracle_path)
            .unwrap_or_else(|e| panic!("read oracle: {e}"))
            .trim(),
    )
    .expect("parse oracle dump");

    // Open the fixture in place — reads only; the scrollback files sit beside it
    // (built as `<fixture>.scrollback/...` with absolute paths in the rows), so we
    // must NOT copy the DB to a temp dir or the transcript paths would still point
    // at the original files. Reads never mutate, so opening in place is safe.
    let db = Db::open_main(Path::new(&fixture), &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open: {e}"));

    // ---- terminal_read cases ----
    assert_eq!(
        spec.read_cases.len(),
        oracle.read_results.len(),
        "read case count: spec vs oracle"
    );
    for (i, c) in spec.read_cases.iter().enumerate() {
        let oc = &oracle.read_results[i];
        assert_eq!(oc.name, c.name, "read case {i}: name mismatch");

        let input = parse_read_args(&c.args);
        let scrollback = resolve_scrollback(&db, &input.session_id);
        let result = execute_terminal_read(&db, &c.chat_id, &input, &scrollback).await;

        match (&c.expect_throw, result) {
            (Some(expected), Err(err)) => {
                let msg = format!("{err}");
                assert_eq!(
                    &msg, expected,
                    "read case {} ({}): thrown message mismatch",
                    i, c.name
                );
                assert_eq!(
                    oc.thrown.as_deref(),
                    Some(expected.as_str()),
                    "read case {} ({}): oracle should have thrown the same message",
                    i,
                    c.name
                );
            }
            (Some(expected), Ok(_)) => {
                panic!(
                    "read case {} ({}): expected throw {expected:?}, got Ok",
                    i, c.name
                );
            }
            (None, Ok(out)) => {
                let got = serde_json::to_value(&out).expect("serialize output");
                let want = oc.output.as_ref().unwrap_or_else(|| {
                    panic!("read case {} ({}): oracle has no output", i, c.name)
                });
                assert_eq!(
                    &got,
                    want,
                    "read case {} ({}): output diverged\n  rust:   {}\n  oracle: {}",
                    i,
                    c.name,
                    serde_json::to_string(&got).unwrap(),
                    serde_json::to_string(want).unwrap()
                );
                let got_fmt = format_terminal_read(&out);
                let want_fmt = oc.formatted.as_deref().unwrap_or_else(|| {
                    panic!(
                        "read case {} ({}): oracle has no formatted string",
                        i, c.name
                    )
                });
                assert_eq!(
                    got_fmt, want_fmt,
                    "read case {} ({}): formatted string diverged",
                    i, c.name
                );
            }
            (None, Err(err)) => {
                let err: TerminalToolError = err;
                panic!("read case {} ({}): unexpected error {err}", i, c.name);
            }
        }
    }

    // ---- terminal_read validator cases ----
    assert_eq!(
        spec.read_validate_cases.len(),
        oracle.read_validate_results.len(),
        "read validate case count"
    );
    for (i, c) in spec.read_validate_cases.iter().enumerate() {
        let ov = &oracle.read_validate_results[i];
        assert_eq!(ov.name, c.name, "validate case {i}: name mismatch");
        let got = validate_terminal_read_input(&c.args).is_some();
        assert_eq!(
            got, ov.valid,
            "validate case {} ({}): validity diverged (rust {got}, oracle {})",
            i, c.name, ov.valid
        );
    }

    // ---- terminal_list cases ----
    assert_eq!(
        spec.list_cases.len(),
        oracle.list_results.len(),
        "list case count"
    );
    for (i, c) in spec.list_cases.iter().enumerate() {
        let ol = &oracle.list_results[i];
        assert_eq!(ol.name, c.name, "list case {i}: name mismatch");

        // The list validator gates on the empty arguments object.
        let valid = validate_terminal_list_input(&json!({}));
        assert_eq!(valid, ol.valid, "list case {} ({}): validity", i, c.name);

        let out = execute_terminal_list(&db, &c.chat_id)
            .await
            .unwrap_or_else(|e| panic!("list case {} ({}): {e}", i, c.name));
        let got = serde_json::to_value(&out).expect("serialize list output");
        assert_eq!(
            got,
            ol.output,
            "list case {} ({}): output diverged\n  rust:   {}\n  oracle: {}",
            i,
            c.name,
            serde_json::to_string(&got).unwrap(),
            serde_json::to_string(&ol.output).unwrap()
        );
        let got_fmt = format_terminal_list(&out);
        assert_eq!(
            got_fmt, ol.formatted,
            "list case {} ({}): formatted string diverged",
            i, c.name
        );
    }

    eprintln!(
        "OK: terminal tools matched oracle ({} read, {} validate, {} list).",
        spec.read_cases.len(),
        spec.read_validate_cases.len(),
        spec.list_cases.len()
    );
}
