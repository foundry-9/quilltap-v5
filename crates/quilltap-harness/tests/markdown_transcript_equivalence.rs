//! Tier-1 differential: the Markdown transcript renderer (P4.d28, v4
//! `b3ee00f1`).
//!
//! Drives `services::markdown_transcript::{build_markdown_transcript,
//! transcript_filename}` and `content_disposition::build_content_disposition`
//! against v4's REAL `lib/export/markdown-transcript.ts` +
//! `lib/api/content-disposition.ts` over a pure corpus. Byte-for-byte on the
//! rendered document — the bytes ARE the deliverable — with no normalization
//! anywhere.
//!
//! Why a corpus and not the `chat-dialogs` fixture: the committed fixture has no
//! whispers, no Pascal/Carina rows, no `customAnnouncer`, no Host link notices,
//! no fictional clock and an ASCII title, so a renderer with any of those arms
//! wrong would pass a fixture-tier diff first try. Each row here carries the
//! exact chat/event JSON v4 consumed, so the arms are observable without
//! churning a fixture five other families read. The route tier (headers, the
//! 404/500 arms, the character-id collection) is `chat_export_equivalence`'s.
//!
//! Generate the oracle (Node 24, TZ=UTC REQUIRED — the zone-less formatting path
//! reads the host TZ):
//!   cd ~/source/quilltap-server
//!   TZ=UTC ~/.nvm/versions/node/v24.13.1/bin/npx tsx \
//!     <v5>/harness/oracle/cases/markdown-transcript.ts \
//!     > /tmp/oracle-markdown-transcript.ndjson
//! Run:
//!   QT_ORACLE_MARKDOWN_TRANSCRIPT=/tmp/oracle-markdown-transcript.ndjson \
//!     cargo test -p quilltap-harness --test markdown_transcript_equivalence -- --nocapture

use std::collections::{BTreeMap, HashMap};

use quilltap_core::content_disposition::{build_content_disposition, Disposition as CdDisposition};
use quilltap_core::services::markdown_transcript::{
    build_markdown_transcript, transcript_filename, LocalOffset, MarkdownTranscriptInput,
};
use serde::Deserialize;
use serde_json::Value;

// The oracle's row union: the transcript variant carries whole chats and event
// lists, so it dwarfs the two string variants (clippy::large_enum_variant) — the
// house pattern for a corpus row enum.
#[allow(clippy::large_enum_variant)]
#[derive(Deserialize)]
#[serde(tag = "kind")]
enum Row {
    #[serde(rename = "transcript")]
    Transcript {
        id: String,
        chat: Value,
        events: Vec<Value>,
        #[serde(rename = "characterNames")]
        character_names: HashMap<String, String>,
        #[serde(rename = "userName")]
        user_name: String,
        #[serde(rename = "defaultTimestampConfig")]
        default_timestamp_config: Value,
        #[serde(rename = "chatSettingsTimezone")]
        chat_settings_timezone: Option<String>,
        #[serde(rename = "localOffsetMin")]
        local_offset_min: i64,
        #[serde(default)]
        out: Option<String>,
        #[serde(default)]
        threw: bool,
    },
    #[serde(rename = "filename")]
    Filename {
        id: String,
        chat: Value,
        out: String,
    },
    #[serde(rename = "disposition")]
    Disposition {
        id: String,
        filename: String,
        disposition: Option<String>,
        out: String,
    },
}

/// First differing line, with the two sides' bytes — a rendered document is many
/// lines and a whole-document dump buries the disagreement.
fn first_diff(got: &str, want: &str) -> String {
    let g: Vec<&str> = got.lines().collect();
    let w: Vec<&str> = want.lines().collect();
    for i in 0..g.len().max(w.len()) {
        let gi = g.get(i).copied().unwrap_or("<none>");
        let wi = w.get(i).copied().unwrap_or("<none>");
        if gi != wi {
            return format!("  line {i}\n  GOT : {gi:?}\n  WANT: {wi:?}");
        }
    }
    // Same lines but different bytes ⇒ the trailing-newline discipline.
    format!(
        "(identical line-by-line; trailing bytes differ — got {:?}, want {:?})",
        &got[got.len().saturating_sub(4)..],
        &want[want.len().saturating_sub(4)..]
    )
}

#[test]
fn markdown_transcript_matches_oracle() {
    let path = match std::env::var("QT_ORACLE_MARKDOWN_TRANSCRIPT") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_ORACLE_MARKDOWN_TRANSCRIPT to the oracle NDJSON (see test header)."
            );
            return;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));

    let mut kinds: BTreeMap<String, usize> = Default::default();
    let mut threw_rows = 0usize;
    let mut astral_disposition_rows = 0usize;
    let mut failed: Vec<String> = Vec::new();
    let mut count = 0usize;

    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let raw: Value = serde_json::from_str(line).unwrap();
        *kinds
            .entry(raw["kind"].as_str().unwrap().to_string())
            .or_default() += 1;
        if raw["kind"] == "transcript" && raw["threw"] == Value::Bool(true) {
            threw_rows += 1;
        }
        if raw["kind"] == "disposition" && !raw["filename"].as_str().unwrap().is_ascii() {
            astral_disposition_rows += 1;
        }

        match serde_json::from_str::<Row>(line).unwrap() {
            Row::Transcript {
                id,
                chat,
                events,
                character_names,
                user_name,
                default_timestamp_config,
                chat_settings_timezone,
                local_offset_min,
                out,
                threw,
            } => {
                let input = MarkdownTranscriptInput {
                    chat: &chat,
                    events: &events,
                    character_names_by_id: &character_names,
                    user_name: &user_name,
                    default_timestamp_config: Some(&default_timestamp_config),
                    chat_settings_timezone: chat_settings_timezone.as_deref(),
                    local_offset: LocalOffset::Fixed(local_offset_min),
                };
                let got = build_markdown_transcript(&input);
                if threw {
                    if got.is_ok() {
                        eprintln!("[{id}] expected the Intl throw, got Ok");
                        failed.push(format!("{id}_expected_throw"));
                    } else {
                        eprintln!("[{id}] throw OK.");
                    }
                } else {
                    let want = out.expect("a non-threw row must carry out");
                    match got {
                        Err(e) => {
                            eprintln!("[{id}] unexpected error: {e}");
                            failed.push(format!("{id}_unexpected_error"));
                        }
                        Ok(got) => {
                            if got != want {
                                eprintln!(
                                    "[{id}] TRANSCRIPT MISMATCH:\n{}",
                                    first_diff(&got, &want)
                                );
                                failed.push(id.clone());
                            } else {
                                eprintln!("[{id}] transcript bytes OK ({} bytes).", got.len());
                            }
                        }
                    }
                }
            }
            Row::Filename { id, chat, out } => {
                let got = transcript_filename(&chat);
                if got != out {
                    eprintln!("[filename {id}] GOT {got:?} WANT {out:?}");
                    failed.push(format!("filename_{id}"));
                }
            }
            Row::Disposition {
                id,
                filename,
                disposition,
                out,
            } => {
                // `null` in the corpus is v4's defaulted parameter (inline).
                let d = match disposition.as_deref() {
                    Some("attachment") => CdDisposition::Attachment,
                    _ => CdDisposition::Inline,
                };
                let got = build_content_disposition(&filename, d);
                if got != out {
                    eprintln!("[disposition {id}] GOT {got:?} WANT {out:?}");
                    failed.push(format!("disposition_{id}"));
                }
            }
        }
        count += 1;
    }

    assert!(
        count > 0,
        "the oracle NDJSON is empty — regenerate it (an erroring builder leaves a stale file)"
    );

    // Shape assertions, not row-count constants. The three families must all be
    // present, the Intl-throw row must survive regeneration (it is the route's
    // 500 arm), and so must the non-ASCII disposition rows — they are the only
    // ones that reach the RFC 5987 branch at all, and the UTF-16-code-unit
    // fallback the lift fixed is invisible without them.
    for family in ["transcript", "filename", "disposition"] {
        assert!(
            kinds.get(family).copied().unwrap_or(0) > 0,
            "oracle carries no '{family}' rows — regeneration lost a family (kinds: {kinds:?})"
        );
    }
    assert!(
        threw_rows >= 1,
        "expected the unresolvable-timezone throw row, got {threw_rows}"
    );
    assert!(
        astral_disposition_rows >= 2,
        "expected the non-ASCII disposition rows (>= 2), got {astral_disposition_rows}"
    );

    assert!(
        failed.is_empty(),
        "markdown-transcript mismatches: {failed:?}"
    );
    eprintln!("OK: markdown-transcript matched oracle ({count} rows, kinds: {kinds:?}).");
}
