//! P4.164 — self-tests for the shared `common` helpers that parse or
//! normalize text before a differential compares it. A helper living in
//! `common` is compiled into every binary that declares `mod common;`, so its
//! unit tests live HERE (one binary) rather than as `#[cfg(test)]` inside
//! `common` (which would re-run them in every family).
//!
//! No oracle, no fixture, no env var — run:
//!   cargo test -p quilltap-harness --test common_helpers_selftest

mod common;

use common::{capture_fields, normalize_v4_sqlite, pdf_lines_from_capture};
use serde_json::json;

// ─────────────────────────── normalize_v4_sqlite ───────────────────────────

const HINT: &str = " - should this be a string literal in single-quotes?";

/// Rule 1: v4's quoted name + SQLite's hint → v5's bare name.
#[test]
fn normalize_maps_the_quoted_hinted_column_onto_the_bare_one() {
    assert_eq!(
        normalize_v4_sqlite(&format!("no such column: \"chatId\"{HINT}")),
        "no such column: chatId"
    );
}

/// Rule 2: the hint alone is not enough — an unquoted name passes through.
#[test]
fn normalize_requires_the_quoted_prefix() {
    let text = format!("no such column: chatId{HINT}");
    assert_eq!(normalize_v4_sqlite(&text), text);
}

/// Rule 3: the quoted name alone is not enough — no hint, no mapping.
#[test]
fn normalize_requires_the_hint() {
    let text = "no such column: \"chatId\"";
    assert_eq!(normalize_v4_sqlite(text), text);
}

/// Rule 4: any other SQLite text passes through byte-identical (the name
/// stays compared — the helper never masks it).
#[test]
fn normalize_passes_other_text_through() {
    for text in [
        "no such column: chatId",
        "no such table: chat_informs",
        "UNIQUE constraint failed: chat_informs.id",
        "",
    ] {
        assert_eq!(normalize_v4_sqlite(text), text);
    }
}

/// The two copies P4.164 folded (`fold_episode_tier3`, `chat_informs_tier2`)
/// were written differently; this is the second one's logic, kept as a
/// both-ways witness that the `common` home answers what it answered on
/// every shape above and on the edge shapes below.
#[test]
fn normalize_agrees_with_the_retired_chat_informs_copy() {
    fn chat_informs_copy(text: &str) -> String {
        match (
            text.strip_prefix("no such column: \""),
            text.strip_suffix(HINT),
        ) {
            (Some(_), Some(head)) => head
                .replacen("no such column: \"", "no such column: ", 1)
                .trim_end_matches('"')
                .to_string(),
            _ => text.to_string(),
        }
    }
    for text in [
        format!("no such column: \"chatId\"{HINT}"),
        format!("no such column: \"a\"\"b\"{HINT}"),
        format!("no such column: \"\"{HINT}"),
        format!("no such column: \"{HINT}"),
        format!("no such column: chatId{HINT}"),
        "no such column: \"chatId\"".to_string(),
        HINT.to_string(),
        "no such table: x".to_string(),
    ] {
        assert_eq!(
            normalize_v4_sqlite(&text),
            chat_informs_copy(&text),
            "{text:?}"
        );
    }
}

// ───────────────────────────── capture_fields ──────────────────────────────

#[test]
fn capture_fields_splits_bare_integer_fields() {
    assert_eq!(
        capture_fields(" size=812 chars=40"),
        vec![("size", "812"), ("chars", "40")]
    );
}

/// The P4.157 OPEN item: a bare string value with spaces stays ONE value.
#[test]
fn capture_fields_keeps_a_spaced_value_whole() {
    assert_eq!(
        capture_fields(" error=file is not a database"),
        vec![("error", "file is not a database")]
    );
    assert_eq!(
        capture_fields(" size=3 error=file is not a database chars=0"),
        vec![
            ("size", "3"),
            ("error", "file is not a database"),
            ("chars", "0")
        ]
    );
}

/// A value whose words hold `=` but no identifier before it is not a boundary.
#[test]
fn capture_fields_only_breaks_before_an_identifier_key() {
    assert_eq!(
        capture_fields(" error=a = b == c"),
        vec![("error", "a = b == c")]
    );
}

#[test]
fn capture_fields_on_nothing_is_empty() {
    assert!(capture_fields("").is_empty());
    assert!(capture_fields("   ").is_empty());
}

// ────────────────────────── pdf_lines_from_capture ─────────────────────────

/// A synthetic catch line (`Error extracting PDF content`, v4's
/// `file-content-extractor.ts:204` — unreachable in v5 today, so no family row
/// poses it): the `error` value carries spaces and must arrive whole.
#[test]
fn pdf_lines_parse_a_spaced_error_value() {
    let lines = vec![
        "ERROR quilltap::file_content_extractor Error extracting PDF content \
         error=file is not a database"
            .to_string(),
    ];
    assert_eq!(
        pdf_lines_from_capture(&lines),
        vec![json!({
            "level": "error",
            "message": "Error extracting PDF content",
            "context": {"error": "file is not a database"},
        })]
    );
}

/// The two shapes the generator families DO pose, unchanged by the parser
/// rewrite (integers as numbers, field order kept, no fields → null).
#[test]
fn pdf_lines_parse_the_posed_shapes_unchanged() {
    let lines = vec![
        "WARN quilltap::file_content_extractor pdf-parse found no text, using \
         native fallback extraction size=812"
            .to_string(),
        "DEBUG quilltap::file_content_extractor Extracted PDF content size=812 chars=40"
            .to_string(),
        "DEBUG quilltap::file_content_extractor Extracted PDF content".to_string(),
        "DEBUG quilltap::other Extracted PDF content size=1".to_string(),
    ];
    let got = pdf_lines_from_capture(&lines);
    assert_eq!(
        got,
        vec![
            json!({
                "level": "warn",
                "message": "pdf-parse found no text, using native fallback extraction",
                "context": {"size": 812},
            }),
            json!({
                "level": "debug",
                "message": "Extracted PDF content",
                "context": {"size": 812, "chars": 40},
            }),
            json!({
                "level": "debug",
                "message": "Extracted PDF content",
                "context": null,
            }),
        ]
    );
    let keys: Vec<_> = got[1]["context"].as_object().unwrap().keys().collect();
    assert_eq!(keys, ["size", "chars"]);
}
