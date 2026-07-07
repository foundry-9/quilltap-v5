//! Tier-1 differential for the W4.6b personified-system WRITER pure builders —
//! byte-exact against v4's REAL EXPORTED functions
//! (`harness/oracle/cases/post-office-concierge-lantern-suparna.ts`).
//!
//! Covers: the Concierge danger content/opaque builders (score/threshold/
//! category-ranking edge cases — crossed vs below threshold, moderation vs llm
//! source, 0/1/2/3+ categories, unknown-category label fallback, stable-sort
//! ties, `toFixed` rounding), the Lantern alert resolver
//! (`is_lantern_image_alert_enabled`, the chat→project→OFF chain), and the
//! Suparṇā mail whisper (`build_suparna_mail_whisper`, 0/1/plural + blank body).
//!
//! The Concierge MANUAL builders and the Lantern BODY builders are module-private
//! in v4 (static per-kind strings) — the Rust port transcribes them verbatim and
//! covers them with unit tests in their own modules; the post paths that consume
//! them are exercised by the parent's central tier-3.
//!
//! Generate (Node 24, from the v4 checkout). `TZ=UTC` is REQUIRED — the Suparṇā
//! whisper renders letter dates via `formatLetterDate` (system-TZ), while the
//! Rust `format_time` is UTC-pinned (the documented harness seam shared with
//! `context-feeders-leaves` / `mail-carina-tools`):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
//!   cd ~/source/quilltap-server
//!   TZ=UTC $N/npx tsx $V5/harness/oracle/cases/post-office-concierge-lantern-suparna.ts \
//!       > /tmp/oracle-po-cls.ndjson
//! Run:
//!   QT_ORACLE_PO_CLS=/tmp/oracle-po-cls.ndjson \
//!     cargo test -p quilltap-harness --test post_office_concierge_lantern_suparna_equivalence

use serde_json::{json, Value};

use quilltap_core::post_office::mailbox::DeliveredLetterSummary;
use quilltap_core::services::concierge_notifications::{
    build_danger_content, build_danger_opaque_content, ConciergeCategory, ConciergeDangerDetails,
};
use quilltap_core::services::lantern_notifications::is_lantern_image_alert_enabled;
use quilltap_core::services::suparna_notifications::build_suparna_mail_whisper;

fn cat(category: &str, score: f64, label: Option<&str>) -> ConciergeCategory {
    ConciergeCategory {
        category: category.into(),
        score,
        label: label.map(|s| s.into()),
    }
}

/// Mirror the oracle's `dangerCases` corpus (must match input-for-input).
fn danger_case(id: &str) -> Option<ConciergeDangerDetails> {
    let d = |score: f64,
             threshold: f64,
             categories: Vec<ConciergeCategory>,
             source: Option<&str>,
             provider: Option<&str>| {
        ConciergeDangerDetails {
            score,
            threshold,
            categories,
            source: source.map(|s| s.into()),
            provider_name: provider.map(|s| s.into()),
        }
    };
    Some(match id {
        "no-details" => return None,
        "crossed-single-moderation" => d(
            0.92,
            0.7,
            vec![cat("nsfw", 0.92, None)],
            Some("moderation"),
            Some("OPENAI"),
        ),
        "crossed-two-llm" => d(
            0.88,
            0.7,
            vec![cat("nsfw", 0.88, None), cat("violence", 0.75, None)],
            Some("llm"),
            Some("DEEPSEEK"),
        ),
        "crossed-three-plus-moderation" => d(
            0.95,
            0.7,
            vec![
                cat("nsfw", 0.95, None),
                cat("violence", 0.9, None),
                cat("hate_speech", 0.8, None),
                cat("self_harm", 0.72, None),
            ],
            Some("moderation"),
            Some("OPENAI"),
        ),
        "crossed-mixed-crossing" => d(
            0.88,
            0.7,
            vec![cat("nsfw", 0.88, None), cat("violence", 0.6, None)],
            Some("moderation"),
            Some("OPENAI"),
        ),
        "crossed-no-provider" => d(0.9, 0.7, vec![cat("nsfw", 0.9, None)], None, None),
        "crossed-zero-categories" => d(0.8, 0.7, vec![], Some("llm"), Some("GROK")),
        "below-flagged-moderation" => d(
            0.5,
            0.7,
            vec![cat("nsfw", 0.5, None), cat("violence", 0.3, None)],
            Some("moderation"),
            Some("OPENAI"),
        ),
        "below-flagged-llm" => d(
            0.6,
            0.7,
            vec![cat("self_harm", 0.6, None)],
            Some("llm"),
            Some("DEEPSEEK"),
        ),
        "below-no-provider" => d(0.4, 0.7, vec![cat("nsfw", 0.4, None)], None, None),
        "below-zero-categories" => d(0.5, 0.7, vec![], Some("moderation"), Some("OPENAI")),
        "unknown-cat-with-label" => d(
            0.9,
            0.7,
            vec![cat("weird_stuff", 0.9, Some("Weird Stuff"))],
            Some("llm"),
            Some("GROK"),
        ),
        "unknown-cat-no-label" => d(
            0.9,
            0.7,
            vec![cat("mystery", 0.9, None)],
            Some("moderation"),
            Some("OPENAI"),
        ),
        "tie-scores-stable" => d(
            0.8,
            0.7,
            vec![
                cat("nsfw", 0.8, None),
                cat("violence", 0.8, None),
                cat("hate_speech", 0.8, None),
                cat("self_harm", 0.8, None),
            ],
            Some("llm"),
            Some("GROK"),
        ),
        "rounding-edge" => d(
            0.925,
            0.005,
            vec![cat("nsfw", 0.125, None), cat("violence", 0.005, None)],
            Some("moderation"),
            Some("OPENAI"),
        ),
        other => panic!("unknown danger case {other}"),
    })
}

fn alert_case(id: &str) -> (Option<Value>, Option<Value>) {
    let chat = |v: Value| Some(json!({ "alertCharactersOfLanternImages": v }));
    let project = |v: Value| Some(json!({ "defaultAlertCharactersOfLanternImages": v }));
    match id {
        "chat-true" => (chat(json!(true)), None),
        "chat-false" => (chat(json!(false)), project(json!(true))),
        "chat-null-project-true" => (chat(Value::Null), project(json!(true))),
        "chat-null-project-false" => (chat(Value::Null), project(json!(false))),
        "chat-null-project-null" => (chat(Value::Null), project(Value::Null)),
        "both-absent" => (None, None),
        "chat-true-project-false" => (chat(json!(true)), project(json!(false))),
        other => panic!("unknown alert case {other}"),
    }
}

fn suparna_case(id: &str) -> Vec<DeliveredLetterSummary> {
    let letter = |path: &str, from: &str, sent: &str, body: &str| DeliveredLetterSummary {
        path: path.into(),
        from: from.into(),
        sent_at: sent.into(),
        body: body.into(),
        alerted: false,
        in_reply_to: None,
    };
    match id {
        "empty" => vec![],
        "single" => vec![letter(
            "Mail/friday-2026-02-01.md",
            "Friday",
            "2026-02-01T09:05:00.000Z",
            "  Hello there.  ",
        )],
        "plural-with-blank" => vec![
            letter(
                "Mail/a.md",
                "Ada",
                "2026-02-02T09:00:00.000Z",
                "First line.\n\nSecond paragraph.",
            ),
            letter("Mail/b.md", "Bob", "2026-02-01T09:00:00.000Z", "   "),
        ],
        other => panic!("unknown suparna case {other}"),
    }
}

fn rust_value(kind: &str, id: &str) -> Value {
    match kind {
        "danger_content" => {
            let details = danger_case(id);
            Value::String(build_danger_content(details.as_ref()))
        }
        "danger_opaque" => {
            let details = danger_case(id);
            Value::String(build_danger_opaque_content(details.as_ref()))
        }
        "lantern_alert" => {
            let (chat, project) = alert_case(id);
            Value::Bool(is_lantern_image_alert_enabled(
                chat.as_ref(),
                project.as_ref(),
            ))
        }
        "suparna_whisper" => Value::String(build_suparna_mail_whisper(&suparna_case(id))),
        other => panic!("unknown oracle kind {other}"),
    }
}

#[test]
fn post_office_concierge_lantern_suparna_matches_oracle() {
    let path = match std::env::var("QT_ORACLE_PO_CLS") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("QT_ORACLE_PO_CLS unset; skipping");
            return;
        }
    };
    let text = std::fs::read_to_string(&path).expect("read oracle ndjson");
    let mut count = 0usize;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Value = serde_json::from_str(line).expect("parse oracle row");
        let kind = row["kind"].as_str().unwrap();
        let id = row["id"].as_str().unwrap();
        let want = &row["value"];
        let got = rust_value(kind, id);
        assert_eq!(&got, want, "post-office writer leaf {kind}/{id} diverges");
        count += 1;
    }
    assert!(count > 0, "oracle had no rows");
    eprintln!("post-office-concierge-lantern-suparna: {count} rows matched");
}
