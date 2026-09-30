//! Tier-1 differential: the `ZodError.message` bytes v4's repositories log and
//! throw when a row fails its schema (P4.130) — v4's REAL `GroupSchema`,
//! `GroupDocMountLinkSchema` and `ChatMetadataBaseSchema` under zod 4.6.5
//! against v5's `quilltap_core::api::zod_issues::{zod_group_issues,
//! zod_group_doc_mount_link_issues}` and `quilltap_core::services::
//! dangerous_content::chat_override::concierge_columns_zod_error`.
//!
//! Exact, per row: the rendered `error.message` (`JSON.stringify(issues, null,
//! 2)` — v4's `Data validation failed` / `Safe validation failed` `error` and
//! the restore/import warning tail) AND the compact issue list, both byte for
//! byte; a row v4 accepts must render no issue on v5. The corpus asks every
//! column absent / `null` / a number / a hydrated BLOB (`{"$float32": n}` — a
//! `Float32Array` on v4's side, as its SQLite collection decodes a BLOB in a
//! non-BLOB column; [`zod_float32_array_cell`] here) / at its boundary ±1,
//! including the two shapes the home could not render before P4.130: the
//! `invalid_union` a `TimestampSchema` reports over a non-string cell, and the
//! datetime `invalid_format` whose `pattern` is zod's JS-form regex. The five
//! chat messages P4.124 pinned by hand (`chat_override.rs`'s unit test) are
//! rows here, proven mechanically.
//!
//! Regenerate + run (self-contained; a pure tsx oracle, no fixture):
//!   V5W=${V5W:-$HOME/source/quilltap-v5}
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   rm -f /tmp/oracle-repository-zod-messages.ndjson
//!   cd ~/source/quilltap-server
//!   $N/npx tsx $V5W/harness/oracle/cases/repository-zod-messages.ts \
//!     > /tmp/oracle-repository-zod-messages.ndjson
//!   cd $V5W
//!   QT_ORACLE_REPOSITORY_ZOD_MESSAGES=/tmp/oracle-repository-zod-messages.ndjson \
//!     cargo test -p quilltap-harness --test repository_zod_messages_equivalence -- --nocapture

use quilltap_core::api::zod_issues::{
    zod_error_message, zod_float32_array_cell, zod_group_doc_mount_link_issues, zod_group_issues,
    ZodIssue,
};
use quilltap_core::services::dangerous_content::chat_override::concierge_columns_zod_error;
use serde_json::{Map, Value};

/// The corpus's `Float32Array` marker → this crate's.
fn materialize(row: &Map<String, Value>) -> Map<String, Value> {
    row.iter()
        .map(|(k, v)| {
            let cell = match v.get("$float32").and_then(Value::as_u64) {
                Some(n) if v.as_object().is_some_and(|o| o.len() == 1) => {
                    zod_float32_array_cell(n as usize)
                }
                _ => v.clone(),
            };
            (k.clone(), cell)
        })
        .collect()
}

#[test]
fn repository_zod_messages_match_oracle() {
    let Ok(path) = std::env::var("QT_ORACLE_REPOSITORY_ZOD_MESSAGES") else {
        eprintln!(
            "SKIP: set QT_ORACLE_REPOSITORY_ZOD_MESSAGES to the oracle NDJSON (see test header)."
        );
        return;
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));
    assert!(
        !text.trim().is_empty(),
        "{path} is EMPTY — a failed regen truncates its redirect"
    );

    let mut failures: Vec<String> = Vec::new();
    let (mut rows, mut ok_rows) = (0usize, 0usize);
    // The shapes the corpus must still ask, so a trimmed regen cannot go green.
    let (mut saw_union, mut saw_datetime, mut saw_float32, mut saw_astral_ok) =
        (false, false, false, false);
    let mut chat_messages = 0usize;

    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        rows += 1;
        let want: Value = serde_json::from_str(line).expect("oracle row is JSON");
        let id = want["id"].as_str().expect("id");
        let schema = want["schema"].as_str().expect("schema");
        let row = materialize(want["row"].as_object().expect("row"));
        let want_message = want.get("message").and_then(Value::as_str);
        if want["ok"] == Value::Bool(true) {
            ok_rows += 1;
            if id == "group-name-99x-astral" {
                saw_astral_ok = true;
            }
        }
        if let Some(m) = want_message {
            saw_union |= m.contains("\"code\": \"invalid_union\"");
            saw_datetime |= m.contains("\"format\": \"datetime\"");
            saw_float32 |=
                m.contains("received Float32Array") && m.contains("expected unknown to be <=100");
        }

        let (got_message, got_issues): (Option<String>, Option<String>) = match schema {
            "group" | "groupDocMountLink" => {
                let issues: Vec<ZodIssue> = if schema == "group" {
                    zod_group_issues(&row)
                } else {
                    zod_group_doc_mount_link_issues(&row)
                };
                if issues.is_empty() {
                    (None, None)
                } else {
                    (
                        Some(zod_error_message(&issues)),
                        Some(serde_json::to_string(&issues).unwrap()),
                    )
                }
            }
            "chatMetadataBase" => {
                if want_message.is_some() {
                    chat_messages += 1;
                }
                // The concierge twin renders only the message; the issue list
                // is that message re-parsed (key order kept — preserve_order).
                let m = concierge_columns_zod_error(&Value::Object(row));
                let issues = m
                    .as_deref()
                    .map(|m| serde_json::from_str::<Value>(m).unwrap().to_string());
                (m, issues)
            }
            other => panic!("{id}: unknown schema {other}"),
        };
        if got_message.as_deref() != want_message {
            failures.push(format!(
                "{id}: MESSAGE differs\n  v4: {want_message:?}\n  v5: {got_message:?}"
            ));
            continue;
        }
        let want_issues = want.get("issues").map(Value::to_string);
        if got_issues != want_issues {
            failures.push(format!(
                "{id}: ISSUES differ\n  v4: {want_issues:?}\n  v5: {got_issues:?}"
            ));
        }
    }
    eprintln!("repository_zod_messages: {rows} rows ({ok_rows} accepted)");
    assert!(rows >= 51, "the corpus shrank ({rows} rows)");
    assert!(
        saw_union && saw_datetime && saw_float32 && saw_astral_ok,
        "the corpus must still ask the union ({saw_union}), datetime ({saw_datetime}), \
         Float32Array ({saw_float32}) and astral ({saw_astral_ok}) shapes"
    );
    assert_eq!(
        chat_messages, 6,
        "the six refused Concierge chat rows (P4.124's five + a numeric reason)"
    );
    assert!(
        failures.is_empty(),
        "{} row(s) differ:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
