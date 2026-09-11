//! Tier-1 differential (P4.D179, v4 `686954937`): `map_task_type_to_log_type` —
//! the cheap-LLM task type → `llm_logs.type` allowlist, exact against v4's REAL
//! `mapTaskTypeToLogType` export.
//!
//! Nothing drove this map before. That is precisely how the port came to carry
//! v4's own pre-fix defect: the allowlist is CLOSED with a `SUMMARIZATION`
//! default, so an unmapped task type does not throw — it files itself among the
//! chat summaries in the Wire Records and the LLM inspector, and only a
//! comparison against v4's table can see it. v4 exported the function in
//! `686954937` for the same reason.
//!
//! Three things are asserted per run:
//!   1. **Exactness** — every corpus row's v5 answer equals v4's.
//!   2. **Admittance** — v4's own `LLMLogTypeEnum` verdict rides each row
//!      (`admitted`), so a v5 output v4's enum would reject is a red.
//!   3. **Coverage, by census** — every task-type string literal the v5 match
//!      names must appear in the corpus. Without this a future arm could be
//!      added to v5 (or ported from v4) and go unmeasured while the family
//!      stayed green — the same silence the map itself is prone to.
//!
//! Generate the oracle output:
//!   cd ~/source/quilltap-server
//!   npx tsx ~/source/quilltap-v5/harness/oracle/cases/task-type-log-mapping.ts \
//!     > /tmp/oracle-task-type-log-mapping.ndjson
//! Run:
//!   QT_ORACLE_TASK_TYPE_LOG_MAPPING=/tmp/oracle-task-type-log-mapping.ndjson \
//!     cargo test -p quilltap-harness --test task_type_log_mapping_equivalence

use std::collections::HashSet;
use std::path::PathBuf;

use quilltap_core::services::llm_logging::map_task_type_to_log_type;
use serde::Deserialize;

#[derive(Deserialize)]
struct Row {
    /// `None` is the ABSENT argument (v4's `taskType?: string`), distinct from
    /// the empty string even though both take the `|| ''` collapse.
    #[serde(rename = "taskType")]
    task_type: Option<String>,
    #[serde(rename = "logType")]
    log_type: String,
    admitted: bool,
}

/// The v5 source under census.
fn llm_logging_source() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../quilltap-core/src/services/llm_logging.rs");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
}

/// Every string literal naming a task type inside `map_task_type_to_log_type`'s
/// match — i.e. the arm patterns. The arms' RESULTS are `log_type::*` constants
/// rather than literals, so the patterns are the only quoted strings in the
/// code… but not in the text: v4 carries a prose comment inside the match whose
/// own quotation marks scanned as an arm and failed this census on its first
/// run. Comment lines are dropped before the scan for that reason.
fn v5_arm_strings(src: &str) -> Vec<String> {
    let start = src
        .find("pub fn map_task_type_to_log_type")
        .expect("map_task_type_to_log_type not found in llm_logging.rs");
    let body = &src[start..];
    let end = body
        .find("_ => log_type::")
        .expect("the catch-all arm `_ => log_type::…` not found — did the match's shape change?");
    let code_only: String = body[..end]
        .lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");

    let mut out = Vec::new();
    let bytes: Vec<char> = code_only.chars().collect();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == '"' {
            let mut j = i + 1;
            let mut lit = String::new();
            while j < bytes.len() && bytes[j] != '"' {
                lit.push(bytes[j]);
                j += 1;
            }
            out.push(lit);
            i = j + 1;
        } else {
            i += 1;
        }
    }
    out
}

#[test]
fn task_type_log_mapping_matches_v4() {
    let path = match std::env::var("QT_ORACLE_TASK_TYPE_LOG_MAPPING") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "SKIP: set QT_ORACLE_TASK_TYPE_LOG_MAPPING to the oracle NDJSON (see header)."
            );
            return;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));

    let mut corpus_task_types: HashSet<String> = HashSet::new();
    let mut count = 0usize;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Row = serde_json::from_str(line).unwrap();
        let got = map_task_type_to_log_type(row.task_type.as_deref());
        assert_eq!(
            got, row.log_type,
            "mapTaskTypeToLogType({:?})",
            row.task_type
        );
        assert!(
            row.admitted,
            "v4's LLMLogTypeEnum does not admit {:?} (mapped from {:?})",
            row.log_type, row.task_type
        );
        if let Some(t) = row.task_type {
            corpus_task_types.insert(t);
        }
        count += 1;
    }
    assert!(count > 0, "oracle file looks empty: {count}");

    // Coverage by census — see the header's item 3.
    let arms = v5_arm_strings(&llm_logging_source());
    assert!(
        arms.len() >= 25,
        "the arm census found only {} task-type literals — the slice is probably wrong, \
         not the match ({arms:?})",
        arms.len()
    );
    let uncovered: Vec<&String> = arms
        .iter()
        .filter(|a| !corpus_task_types.contains(*a))
        .collect();
    assert!(
        uncovered.is_empty(),
        "v5 names task types the corpus never asks v4 about: {uncovered:?} — \
         add them to harness/oracle/cases/task-type-log-mapping.ts and regenerate"
    );

    eprintln!(
        "OK: task-type→log-type matched oracle ({count} rows; {} v5 arms all covered).",
        arms.len()
    );
}
