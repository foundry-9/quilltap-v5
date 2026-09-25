//! Tier-1 differential: v4's REAL `classifyRefusal` / `isModerationRefusal`
//! (`lib/services/dangerous-content/refusal.ts`, NEW at `8bd080267`, #73)
//! against `quilltap_core::services::dangerous_content::refusal` (P4.D225).
//!
//! ## Comparands
//!
//! Per corpus row: the verdict (`refused`, `evidence`, `detail` — key PRESENCE
//! included: v4's `{ refused: false }` carries neither), `isModerationRefusal`,
//! and **every line the `ConciergeRefusal` logger wrote** — the DEBUG line on
//! every classification and the INFO line on every refusal, level + message +
//! every bag field in v4's order. v4's bag drops an `undefined` value
//! (`JSON.stringify`), and `tracing` omits a `None` `Option` field, so key
//! presence is compared too (the `emptyBody`/`contentWasFlagged` rows that pass
//! no flag carry no such field on either side). Field names are the repo's
//! standing snake_case mapping of v4's camelCase bag keys.
//!
//! ## The input mapping
//!
//! v4 classifies a thrown JS value; v5 classifies a `RefusalError` (the ruled
//! structured input — see the module doc on `refusal.rs`). Each row carries
//! the JS value's description, and [`error_of`] builds the struct the way a v5
//! provider site would: an `Error`'s own props through
//! `RefusalError::from_record` (its `name` defaulting to `"Error"`, as the JS
//! prototype supplies), a plain object the same with `messageOf`'s
//! string-only rule, a thrown string as message-only, a thrown number as an
//! empty record-less error. A `{"$num": "NaN"}` marker stands for the
//! non-finite number JSON cannot carry and is decoded through
//! `code_string_number` — the same arm `code_string` delegates to.
//!
//! Regenerate the oracle (Node 24, from the v4 checkout; stage the case OUTSIDE
//! any `.claude/` path — v4's jest ignores `/\.claude/`). While v4 HEAD is past
//! the oracle baseline this needs a PINNED worktree; that is the sweep driver's
//! `--v4`, never a path in this header.
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5} ; TMPO=/tmp/qt-oracle-run-refusal-classify
//!   cd ~/source/quilltap-server
//!   rm -rf "$TMPO"; mkdir -p "$TMPO/cases"
//!   cp "$V5W/harness/oracle/cases/refusal-classify.test.ts" "$TMPO/cases/"
//!   rm -f /tmp/oracle-refusal-classify.ndjson
//!   QT_ORACLE_OUT=/tmp/oracle-refusal-classify.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=120000 \
//!       --roots "$PWD" --roots "$TMPO/cases" -- "cases/refusal-classify\.test\.ts$"
//! Run:
//!   QT_ORACLE_REFUSAL_CLASSIFY=/tmp/oracle-refusal-classify.ndjson \
//!     cargo test -p quilltap-harness --test refusal_classify_equivalence -- --nocapture

use quilltap_core::services::dangerous_content::refusal::{
    classify_refusal, code_string, code_string_number, is_moderation_refusal, RefusalError,
    RefusalInput,
};
use quilltap_core::test_support::captured_with;
use serde_json::{Map, Value};

/// A code slot's value: a `{"$num": "…"}` marker is a non-finite JS number.
fn code_of(v: Option<&Value>) -> Option<String> {
    let v = v?;
    if let Some(obj) = v.as_object() {
        if obj.len() == 1 {
            if let Some(n) = obj.get("$num").and_then(Value::as_str) {
                return code_string_number(n.parse::<f64>().unwrap_or(f64::NAN));
            }
        }
    }
    code_string(v)
}

fn from_record(message: String, record: &Map<String, Value>) -> RefusalError {
    let mut e = RefusalError::from_record(message, record);
    // Re-read both code slots through the marker-aware reader.
    e.code = code_of(record.get("code"));
    e.nested_code = record
        .get("error")
        .and_then(Value::as_object)
        .and_then(|n| code_of(n.get("code")));
    e
}

/// Build the `RefusalError` a v5 provider site would hand the classifier for
/// the JS value this row describes (`None` = v4's `error` absent).
fn error_of(spec: &Value) -> Option<RefusalError> {
    if spec.is_null() {
        return None;
    }
    let shape = spec["shape"].as_str().expect("error shape");
    Some(match shape {
        "error" => {
            let mut record = spec
                .get("props")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            // An `Error` instance's `name` comes off its prototype.
            record
                .entry("name")
                .or_insert_with(|| Value::String("Error".into()));
            from_record(spec["message"].as_str().unwrap().to_string(), &record)
        }
        "object" => {
            let record = spec["value"].as_object().unwrap();
            // v4 `messageOf`: a non-Error object's message counts only when it
            // is a STRING.
            let message = record
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            from_record(message, record)
        }
        "string" => RefusalError::message_only(spec["value"].as_str().unwrap()),
        // A thrown number: present, record-less, message `''`.
        "number" => RefusalError::default(),
        other => panic!("unknown error shape {other}"),
    })
}

fn snake(k: &str) -> String {
    let mut out = String::new();
    for c in k.chars() {
        if c.is_ascii_uppercase() {
            out.push('_');
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// v4's recorded line in the capture rig's rendering: `LEVEL target message
/// field=value …`.
fn render_v4(log: &Value) -> String {
    let level = log["level"].as_str().unwrap().to_uppercase();
    let mut line = format!(
        "{level} quilltap::concierge_refusal {}",
        log["message"].as_str().unwrap()
    );
    for (k, v) in log["bag"].as_object().unwrap() {
        let rendered = match v {
            Value::String(s) => s.clone(),
            Value::Bool(b) => b.to_string(),
            other => other.to_string(),
        };
        line.push_str(&format!(" {}={rendered}", snake(k)));
    }
    line
}

#[test]
fn refusal_classify_matches_v4() {
    let Ok(path) = std::env::var("QT_ORACLE_REFUSAL_CLASSIFY") else {
        eprintln!("SKIP: QT_ORACLE_REFUSAL_CLASSIFY not set");
        return;
    };
    let text = std::fs::read_to_string(&path).expect("read oracle NDJSON");
    let mut rows = 0usize;
    let mut by_evidence = std::collections::BTreeMap::<String, usize>::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Value = serde_json::from_str(line).expect("parse oracle row");
        let label = row["label"].as_str().unwrap();
        let input = &row["input"];
        let error = error_of(&input["error"]);
        let finish_reason = match input.get("finishReason") {
            Some(Value::String(s)) => Some(s.clone()),
            _ => None,
        };
        let empty_body = input.get("emptyBody").and_then(Value::as_bool);
        let content_was_flagged = input.get("contentWasFlagged").and_then(Value::as_bool);

        let (verdict, lines) = captured_with(|| {
            classify_refusal(RefusalInput {
                error: error.as_ref(),
                finish_reason: finish_reason.as_deref(),
                empty_body,
                content_was_flagged,
            })
        });

        // The verdict, key presence included.
        let expect = &row["verdict"];
        assert_eq!(
            verdict.refused,
            expect["refused"].as_bool().unwrap(),
            "{label}: refused"
        );
        assert_eq!(
            verdict.evidence.map(|e| e.as_str().to_string()),
            expect
                .get("evidence")
                .and_then(Value::as_str)
                .map(str::to_string),
            "{label}: evidence"
        );
        assert_eq!(
            verdict.detail,
            expect
                .get("detail")
                .and_then(Value::as_str)
                .map(str::to_string),
            "{label}: detail"
        );
        if !verdict.refused {
            assert!(
                expect.get("evidence").is_none() && expect.get("detail").is_none(),
                "{label}: v4's refused:false carries no other key"
            );
        }

        // isModerationRefusal (asked only where v4 had an error).
        if let Some(err) = &error {
            let got = captured_with(|| is_moderation_refusal(err)).0;
            assert_eq!(
                Some(got),
                row["isModerationRefusal"].as_bool(),
                "{label}: isModerationRefusal"
            );
        } else {
            assert!(
                row["isModerationRefusal"].is_null(),
                "{label}: no error, no shorthand"
            );
        }

        // Every line, in order.
        let expected: Vec<String> = row["logs"]
            .as_array()
            .unwrap()
            .iter()
            .map(render_v4)
            .collect();
        assert_eq!(lines, expected, "{label}: the ConciergeRefusal lines");

        rows += 1;
        *by_evidence
            .entry(
                expect
                    .get("evidence")
                    .and_then(Value::as_str)
                    .unwrap_or("none")
                    .to_string(),
            )
            .or_default() += 1;
    }
    eprintln!("refusal_classify: {rows} rows, by evidence {by_evidence:?}");
    // Corpus floors: a truncated oracle must not pass vacuously, and all six
    // exits must be exercised.
    assert!(rows >= 70, "expected the whole corpus, got {rows}");
    for exit in [
        "typed-error",
        "provider-code",
        "finish-reason",
        "message-pattern",
        "inferred",
        "none",
    ] {
        assert!(
            by_evidence.get(exit).copied().unwrap_or(0) >= 3,
            "exit {exit} under-exercised: {by_evidence:?}"
        );
    }
}

/// The silence leg: a non-refusal writes the DEBUG line and NOTHING at INFO.
#[test]
fn a_non_refusal_writes_no_info_line() {
    let err = RefusalError::message_only("ETIMEDOUT");
    let (v, lines) = captured_with(|| {
        classify_refusal(RefusalInput {
            error: Some(&err),
            ..Default::default()
        })
    });
    assert!(!v.refused);
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(lines[0].starts_with("DEBUG quilltap::concierge_refusal"));
    assert!(!lines
        .iter()
        .any(|l| l.contains("Provider refused on content-moderation grounds")));
}
