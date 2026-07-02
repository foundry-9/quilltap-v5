//! Tier-1 differential test: the memory-extraction pure leaves
//! (`quilltap_core::memory_tasks` vs v4 `lib/memory/cheap-llm-tasks/
//! memory-tasks.ts`) — the SELF/OTHER extraction prompt builders (preambles,
//! orienting footer, turn-context renderer) and the candidate response parsers
//! (fence stripping, targeting-tag validation, candidate shaping, caps). Exact
//! string / structural equality against the v4 oracle (which drives v4's REAL
//! extractors with only `executeCheapLLMTask` mocked, capturing the built
//! messages and feeding the corpus response into the real parser).
//!
//! Generate the oracle output (jest — the seam needs `jest.mock`):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   V5=~/source/quilltap-v5
//!   cd ~/source/quilltap-server
//!   QT_ORACLE_OUT=/tmp/oracle-memory-tasks.ndjson \
//!     $N/npx jest --silent --roots "$PWD" --roots "$V5/harness/oracle/cases" -- memory-tasks-tier1
//! Run:
//!   QT_ORACLE_MEMORY_TASKS=/tmp/oracle-memory-tasks.ndjson \
//!     cargo test -p quilltap-harness memory_tasks

use quilltap_core::memory_format::Pronouns;
use quilltap_core::memory_tasks::{
    build_other_extraction_messages, build_self_extraction_messages, parse_memory_candidate_array,
    parse_other_candidates_by_subject, resolve_max_memories, OrientingContext, OtherSubjectInput,
    TurnCharacterSlice, TurnTranscript,
};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
struct PronounsW {
    subject: String,
    object: String,
    possessive: String,
}

impl PronounsW {
    fn into_core(self) -> Pronouns {
        Pronouns {
            subject: self.subject,
            object: self.object,
            possessive: self.possessive,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SliceW {
    character_id: String,
    character_name: String,
    #[serde(default)]
    character_pronouns: Option<PronounsW>,
    text: String,
    contributing_message_ids: Vec<String>,
    #[serde(default)]
    is_user_controlled: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TranscriptW {
    #[serde(default)]
    turn_opener_message_id: Option<String>,
    #[serde(default)]
    user_message: Option<String>,
    #[serde(default)]
    user_character_id: Option<String>,
    #[serde(default)]
    user_character_name: Option<String>,
    #[serde(default)]
    user_character_pronouns: Option<PronounsW>,
    character_slices: Vec<SliceW>,
    #[serde(default)]
    latest_assistant_message_id: Option<String>,
}

impl TranscriptW {
    fn into_core(self) -> TurnTranscript {
        TurnTranscript {
            turn_opener_message_id: self.turn_opener_message_id,
            user_message: self.user_message,
            user_character_id: self.user_character_id,
            user_character_name: self.user_character_name,
            user_character_pronouns: self.user_character_pronouns.map(PronounsW::into_core),
            character_slices: self
                .character_slices
                .into_iter()
                .map(|s| TurnCharacterSlice {
                    character_id: s.character_id,
                    character_name: s.character_name,
                    character_pronouns: s.character_pronouns.map(PronounsW::into_core),
                    text: s.text,
                    contributing_message_ids: s.contributing_message_ids,
                    is_user_controlled: s.is_user_controlled,
                })
                .collect(),
            latest_assistant_message_id: self.latest_assistant_message_id,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SubjectW {
    id: String,
    name: String,
    #[serde(default)]
    pronouns: Option<PronounsW>,
    is_user: bool,
    canon_block: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OrientingW {
    #[serde(default)]
    project_description: Option<String>,
    #[serde(default)]
    chat_context_summary: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CaseW {
    kind: String,
    name: String,
    transcript: TranscriptW,
    #[serde(default)]
    target_character_id: Option<String>,
    #[serde(default)]
    canon_block: Option<String>,
    #[serde(default)]
    observer_character_id: Option<String>,
    #[serde(default)]
    subjects: Vec<SubjectW>,
    #[serde(default)]
    resolved_max_tokens: Option<f64>,
    in_autonomous_room: bool,
    #[serde(default)]
    orienting: Option<OrientingW>,
    response_text: String,
}

#[derive(Deserialize)]
struct OracleMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct OracleCall {
    messages: Vec<OracleMessage>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OracleRow {
    name: String,
    calls: Vec<OracleCall>,
    success: bool,
    has_usage: bool,
    result: Value,
}

#[test]
fn memory_tasks_equivalence() {
    let oracle_path = match std::env::var("QT_ORACLE_MEMORY_TASKS") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("QT_ORACLE_MEMORY_TASKS not set — skipping memory-tasks differential");
            return;
        }
    };
    let oracle_raw = std::fs::read_to_string(&oracle_path).expect("read oracle NDJSON");
    let oracle_rows: Vec<OracleRow> = oracle_raw
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("parse oracle row"))
        .collect();

    let corpus_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../harness/oracle/fixtures/memory-tasks-tier1.json"
    );
    let corpus_raw = std::fs::read_to_string(corpus_path).expect("read corpus fixture");
    #[derive(Deserialize)]
    struct Spec {
        cases: Vec<CaseW>,
    }
    let spec: Spec = serde_json::from_str(&corpus_raw).expect("parse corpus");

    assert_eq!(
        spec.cases.len(),
        oracle_rows.len(),
        "corpus/oracle case-count mismatch — regenerate the oracle NDJSON"
    );

    for (case, oracle) in spec.cases.into_iter().zip(oracle_rows) {
        assert_eq!(case.name, oracle.name, "case order mismatch");
        let name = &case.name;

        let transcript = case.transcript.into_core();
        let orienting = case.orienting.map(|o| OrientingContext {
            project_description: o.project_description,
            chat_context_summary: o.chat_context_summary,
        });

        let (messages, result): (Option<Vec<_>>, Value) = match case.kind.as_str() {
            "self" => {
                let messages = build_self_extraction_messages(
                    &transcript,
                    case.target_character_id
                        .as_deref()
                        .expect("targetCharacterId"),
                    case.canon_block.as_deref().expect("canonBlock"),
                    case.resolved_max_tokens,
                    case.in_autonomous_room,
                    orienting.as_ref(),
                );
                let result = match &messages {
                    // The call was made — the real parser consumes the canned
                    // response.
                    Some(_) => {
                        serde_json::to_value(parse_memory_candidate_array(&case.response_text))
                            .unwrap()
                    }
                    // v4's early return: no slice for the target → `[]`.
                    None => json!([]),
                };
                (messages, result)
            }
            "other" => {
                let subjects: Vec<OtherSubjectInput> = case
                    .subjects
                    .into_iter()
                    .map(|s| OtherSubjectInput {
                        id: s.id,
                        name: s.name,
                        pronouns: s.pronouns.map(PronounsW::into_core),
                        is_user: s.is_user,
                        canon_block: s.canon_block,
                    })
                    .collect();
                let messages = build_other_extraction_messages(
                    &transcript,
                    case.observer_character_id
                        .as_deref()
                        .expect("observerCharacterId"),
                    &subjects,
                    case.resolved_max_tokens,
                    case.in_autonomous_room,
                    orienting.as_ref(),
                );
                let buckets = match &messages {
                    Some(_) => {
                        let cap = resolve_max_memories(case.resolved_max_tokens);
                        parse_other_candidates_by_subject(&case.response_text, &subjects, cap)
                    }
                    // v4's early return: an empty bucket per subject, no call.
                    None => subjects
                        .iter()
                        .map(|s| (s.id.clone(), Vec::new()))
                        .collect(),
                };
                let mut obj = serde_json::Map::new();
                for (id, candidates) in buckets {
                    obj.insert(id, serde_json::to_value(candidates).unwrap());
                }
                (messages, Value::Object(obj))
            }
            other => panic!("unknown case kind {other}"),
        };

        // The mocked executor always succeeds, and usage is present exactly
        // when a call was made (the early returns carry `usage: undefined`).
        assert!(oracle.success, "{name}: oracle reported failure");
        assert_eq!(
            oracle.has_usage,
            messages.is_some(),
            "{name}: usage-presence (call-made) mismatch"
        );

        // Calls: zero (early return) or one, with byte-exact messages.
        let rust_calls: Vec<Vec<(String, String)>> = messages
            .into_iter()
            .map(|msgs| {
                msgs.into_iter()
                    .map(|m| (m.role.as_str().to_string(), m.content))
                    .collect()
            })
            .collect();
        let oracle_calls: Vec<Vec<(String, String)>> = oracle
            .calls
            .into_iter()
            .map(|c| {
                c.messages
                    .into_iter()
                    .map(|m| (m.role, m.content))
                    .collect()
            })
            .collect();
        assert_eq!(
            rust_calls.len(),
            oracle_calls.len(),
            "{name}: call-count mismatch"
        );
        for (ci, (rust_msgs, oracle_msgs)) in rust_calls.iter().zip(&oracle_calls).enumerate() {
            assert_eq!(
                rust_msgs.len(),
                oracle_msgs.len(),
                "{name}: call {ci} message-count mismatch"
            );
            for (mi, ((r_role, r_content), (o_role, o_content))) in
                rust_msgs.iter().zip(oracle_msgs).enumerate()
            {
                assert_eq!(r_role, o_role, "{name}: call {ci} message {mi} role");
                assert_eq!(
                    r_content, o_content,
                    "{name}: call {ci} message {mi} content diverges"
                );
            }
        }

        assert_eq!(result, oracle.result, "{name}: parsed result diverges");
    }
}
