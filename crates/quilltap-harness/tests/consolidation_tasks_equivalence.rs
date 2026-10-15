//! P4.D271 tier-1 differential: v4's consolidation task family
//! (`lib/memory/cheap-llm-tasks/consolidation-tasks.ts`, `d58548051`,
//! byte-identical at the `01a83539d` pin) against
//! `quilltap_core::consolidation_tasks` — the task type, the GENERATED system
//! prompt's bytes, `buildConsolidationUserMessage` (three buckets × two kinds ×
//! canon / digest present / whitespace-only / absent, the 1,200-unit cut,
//! `toFixed(2)` edges, `undated`, newline-run flattening), and
//! `parseConsolidationResponse` / `validateConsolidationOutput` over every reason
//! string, each Zod 4 first-issue shape, code fences, prose-wrapped JSON,
//! non-object roots and unparseable text (V8's `JSON.parse` wording). Every
//! comparand is exact.
//!
//! **Recorded lossy class:** a 1,200-unit cut through a surrogate pair leaves v4
//! a lone surrogate (the oracle marks the row `lossy`); v5 renders U+FFFD there.
//! The row is pinned to that shape, not skipped.
//!
//! Regenerate + run (a pure tsx oracle, no fixture):
//!   V5W=${V5W:-$HOME/source/quilltap-v5}
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   rm -f /tmp/qt-consolidation-tasks.ndjson
//!   cd ~/source/quilltap-server
//!   $N/npx tsx $V5W/harness/oracle/cases/consolidation-tasks.ts > /tmp/qt-consolidation-tasks.ndjson
//!   cd $V5W
//!   QT_ORACLE_CONSOLIDATION_TASKS=/tmp/qt-consolidation-tasks.ndjson \
//!     cargo test -p quilltap-harness --test consolidation_tasks_equivalence -- --nocapture

use quilltap_core::consolidation_tasks::{
    build_consolidation_user_message, parse_consolidation_response, validate_consolidation_output,
    ConsolidationBucketKind, ConsolidationCallInput, ConsolidationMemberInput,
    CONSOLIDATION_SYSTEM_PROMPT, CONSOLIDATION_TASK_TYPE,
};
use serde_json::Value;

fn opt_str(v: &Value) -> Option<String> {
    v.as_str().map(str::to_string)
}

fn call_input(v: &Value) -> ConsolidationCallInput {
    ConsolidationCallInput {
        holder_name: v["holderName"].as_str().unwrap().to_string(),
        subject_name: v["subjectName"].as_str().unwrap().to_string(),
        bucket: match v["bucket"].as_str().unwrap() {
            "self" => ConsolidationBucketKind::SelfBucket,
            "other" => ConsolidationBucketKind::Other,
            _ => ConsolidationBucketKind::None,
        },
        cluster_kind: v["clusterKind"].as_str().unwrap().to_string(),
        canon_block: opt_str(&v["canonBlock"]),
        existing_digest: opt_str(&v["existingDigest"]),
        members: v["members"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| ConsolidationMemberInput {
                handle: m["handle"].as_str().unwrap().to_string(),
                when: opt_str(&m["when"]),
                importance: m["importance"].as_f64().unwrap(),
                reinforcement_count: m["reinforcementCount"].as_f64().unwrap(),
                content: m["content"].as_str().unwrap().to_string(),
            })
            .collect(),
    }
}

fn handles(v: &Value) -> Vec<String> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|h| h.as_str().unwrap().to_string())
        .collect()
}

/// v4's `JSON.stringify` of a string with a lone surrogate escapes the unit
/// as `\udXXX`; v5's U+FFFD stands in for exactly that unit.
fn v5_shape_of_lossy(escaped: &str) -> String {
    let mut out = String::new();
    let mut rest = escaped;
    while let Some(i) = rest.find("\\ud") {
        out.push_str(&rest[..i]);
        out.push_str("\\ufffd");
        rest = &rest[i + 6..];
    }
    out.push_str(rest);
    let decoded: String = serde_json::from_str(&out).expect("escaped JSON string");
    decoded
}

#[test]
fn consolidation_tasks_match_oracle() {
    let Ok(path) = std::env::var("QT_ORACLE_CONSOLIDATION_TASKS") else {
        eprintln!("SKIP: set QT_ORACLE_CONSOLIDATION_TASKS to the oracle NDJSON (see header).");
        return;
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read oracle {path}: {e}"));
    let mut failures = Vec::new();
    let mut empty_answers = 0usize;
    let (mut messages, mut parses, mut validates, mut lossy, mut invalid, mut unparseable) =
        (0, 0, 0, 0, 0, 0);
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let c: Value = serde_json::from_str(line).expect("oracle line");
        match c["case"].as_str().unwrap() {
            "constants" => {
                assert_eq!(c["taskType"], CONSOLIDATION_TASK_TYPE);
                assert_eq!(
                    c["systemPrompt"].as_str().unwrap(),
                    CONSOLIDATION_SYSTEM_PROMPT,
                    "the GENERATED system prompt bytes"
                );
            }
            "userMessage" => {
                messages += 1;
                let got = build_consolidation_user_message(&call_input(&c["input"]));
                let want = if c["lossy"].as_bool().unwrap() {
                    lossy += 1;
                    v5_shape_of_lossy(c["escaped"].as_str().expect("lossy rows carry `escaped`"))
                } else {
                    c["out"].as_str().unwrap().to_string()
                };
                if got != want {
                    failures.push(format!(
                        "userMessage {}:\n--- v5\n{got}\n--- v4\n{want}",
                        c["input"]
                    ));
                }
            }
            "parse" => {
                parses += 1;
                let got = parse_consolidation_response(
                    c["content"].as_str().unwrap(),
                    &handles(&c["handles"]),
                )
                .to_json();
                if c["out"]["ok"] == false {
                    invalid += 1;
                    let reason = c["out"]["reason"].as_str().unwrap();
                    if reason.starts_with("unparseable JSON") {
                        unparseable += 1;
                    }
                    // Bug 246 (P4.D297): an empty / whitespace-only answer is
                    // the empty-answer verdict, checked BEFORE the parse.
                    if reason
                        == quilltap_core::consolidation_tasks::CONSOLIDATION_EMPTY_ANSWER_REASON
                    {
                        empty_answers += 1;
                    }
                }
                if serde_json::to_string(&got).unwrap() != serde_json::to_string(&c["out"]).unwrap()
                {
                    failures.push(format!(
                        "parse {:?}:\n  v5 {got}\n  v4 {}",
                        c["content"], c["out"]
                    ));
                }
            }
            "validate" => {
                validates += 1;
                let got =
                    validate_consolidation_output(&c["raw"], &handles(&c["handles"])).to_json();
                if serde_json::to_string(&got).unwrap() != serde_json::to_string(&c["out"]).unwrap()
                {
                    failures.push(format!(
                        "validate {}:\n  v5 {got}\n  v4 {}",
                        c["raw"], c["out"]
                    ));
                }
            }
            other => panic!("unknown case {other}"),
        }
    }
    assert!(
        failures.is_empty(),
        "{} failure(s):\n{}",
        failures.len(),
        failures.join("\n")
    );
    assert!(
        messages >= 31 && lossy >= 1,
        "messages {messages} lossy {lossy}"
    );
    assert!(
        parses >= 70 && validates >= 6,
        "parses {parses} validates {validates}"
    );
    // Bug 246 moved the `''` / `'   '` rows from V8's `unparseable JSON` to
    // the empty-answer verdict (12 → 11 unparseable at the `83334b9a5` pin).
    assert!(
        invalid >= 50 && unparseable >= 11 && empty_answers >= 2,
        "invalid {invalid} unparseable {unparseable} empty answers {empty_answers}"
    );
    eprintln!(
        "OK: consolidation tasks matched oracle ({messages} user messages incl. {lossy} lossy, {parses} parses ({invalid} invalid, {unparseable} unparseable), {validates} raw validations)."
    );
}
