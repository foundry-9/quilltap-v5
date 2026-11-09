//! Established Fact Memories job handler — port of v4
//! `lib/background-jobs/handlers/established-fact-memories.ts` (P4.D372, v4
//! `db6190b44`, pin `624d0d25c`).
//!
//! v4's why, carried: a machine-delivered established fact (the Salon's
//! established-fact tab) is a Host row with no speaker — there is nothing for
//! LLM extraction to interpret, and every Staff row is skipped by it anyway.
//! So each one is admitted deterministically, verbatim, as one MANUAL memory
//! per LLM seat that is not removed — absent and silent seats included,
//! because everybody knows it.
//!
//! `source: 'MANUAL'` because the operator wrote it: it never folds into an
//! extracted row, and a chat wipe (`retireChatMemories`, `keep: 'manual'`)
//! spares it. Admission runs through the ONE admission path
//! ([`crate::services::admission::admit_candidate`]); its own dedup handles a
//! repeat. One seat's failure (a missing character, an embedding failure) is
//! a WARN and never stops the others.

use serde_json::{json, Value};

use crate::db::background_jobs::BackgroundJob;
use crate::db::runtime::Db;
use crate::model::embedding::EmbeddingProvider;
use crate::services::admission::{admit_candidate, AdmissionKeyScope, AdmitOptions};

/// v4 `ESTABLISHED_FACT_MEMORY_IMPORTANCE` — settled truth the operator vouched for.
pub const ESTABLISHED_FACT_MEMORY_IMPORTANCE: f64 = 0.8;

/// v4 `establishedFactCandidate` (exported for tests) — the candidate a seat's
/// character admits for one fact, as the JSON v4's `MemoryCandidateSchema`
/// parses (so a refusal is the same refusal). `anchorTimeMs` is the fact
/// row's `createdAt` when it parses, else `now_ms` (v4's `Date.now()`).
pub fn established_fact_candidate(
    fact: &str,
    holder_id: &str,
    chat_id: &str,
    project_id: Option<&str>,
    message_id: &str,
    created_at: Option<&str>,
    now_ms: f64,
) -> Value {
    let anchor = created_at
        .filter(|s| !s.is_empty())
        .and_then(crate::episodic::js_date_parse_ms)
        .map_or(now_ms, |ms| ms as f64);
    json!({
        "content": fact,
        "summary": fact,
        "keywords": [],
        "targeting": { "temporal": "present", "scope": "wide", "context": "information" },
        "importance": ESTABLISHED_FACT_MEMORY_IMPORTANCE,
        "kind": "semantic",
        "when": null,
        "entities": [],
        "tags": [],
        "subject": { "holderId": holder_id, "aboutId": null },
        "provenance": {
            "source": "MANUAL",
            "witnessedContext": "manual",
            "chatId": chat_id,
            "projectId": project_id,
            "sourceMessageId": message_id,
            "anchorTimeMs": anchor,
            "pass": "established-fact",
        },
    })
}

/// What the handler needs besides the job (the host supplies them).
pub struct EstablishedFactMemoriesDeps<'a, P> {
    pub provider: &'a P,
    /// The running job attempt's admission key scope (v4's child job scope).
    /// The host passes one per run; `None` drives the handler in-process, as
    /// the differential's oracle does.
    pub key_scope: Option<&'a AdmissionKeyScope>,
    /// The host's memory-calendar fallback zone.
    pub host_zone_fallback: &'a str,
    /// v4 `Date.now()` — read only when the fact's `createdAt` will not parse.
    pub now_ms: f64,
}

/// v4 `handleEstablishedFactMemories`. Never fails the job: every exit is a
/// log line (v4's handler returns `void` on every arm).
pub async fn handle_established_fact_memories<P: EmbeddingProvider + Sync>(
    db: &Db,
    job: &BackgroundJob,
    deps: EstablishedFactMemoriesDeps<'_, P>,
) -> Result<(), String> {
    let payload: Value = serde_json::from_str(&job.payload).unwrap_or(Value::Null);
    let chat_id = payload
        .get("chatId")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let message_id = payload
        .get("messageId")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();

    let cid = chat_id.clone();
    let chat = db
        .read_main(move |c| Ok(crate::db::chats_read::find_by_id_or_none(c, &cid)))
        .ok()
        .flatten();
    let Some(chat) = chat else {
        tracing::warn!(
            jobId = %job.id,
            chatId = %chat_id,
            "[EstablishedFactMemories] Chat not found; skipping"
        );
        return Ok(());
    };

    let cid = chat_id.clone();
    let messages = db
        .read_main(move |c| crate::db::chats_messages_read::get_messages(c, &cid))
        .unwrap_or_default();
    let message = messages.iter().find(|m| {
        m.get("type").and_then(Value::as_str) == Some("message")
            && m.get("id").and_then(Value::as_str) == Some(message_id.as_str())
    });
    let message = match message {
        Some(m)
            if crate::established_fact::is_established_fact_announcement(
                &crate::established_fact::EstablishedFactFields::from_value(m),
            ) =>
        {
            m
        }
        // Deleted before the job ran, or not a fact row at all: nothing to know.
        _ => {
            tracing::warn!(
                jobId = %job.id,
                chatId = %chat_id,
                messageId = %message_id,
                "[EstablishedFactMemories] Fact message not found; skipping"
            );
            return Ok(());
        }
    };
    let fact = crate::jsstr::js_trim(message.get("content").and_then(Value::as_str).unwrap_or(""))
        .to_string();
    if fact.is_empty() {
        return Ok(());
    }
    let fact_id = message
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let created_at = message
        .get("createdAt")
        .and_then(Value::as_str)
        .map(str::to_string);
    let project_id = chat
        .get("projectId")
        .and_then(Value::as_str)
        .map(str::to_string);

    // Every LLM seat that is not removed. Absent and silent seats know it too;
    // a user-controlled seat is the operator's, and forms no memory here.
    let participants: Vec<Value> = chat
        .get("participants")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let s = |p: &Value, k: &str| p.get(k).and_then(Value::as_str).map(str::to_string);
    let seats: Vec<&Value> = participants
        .iter()
        .filter(|p| {
            s(p, "type").as_deref() == Some("CHARACTER")
                && s(p, "controlledBy").as_deref() != Some("user")
                && s(p, "status").as_deref() != Some("removed")
                && s(p, "characterId").is_some_and(|c| !c.is_empty())
        })
        .collect();

    let mut admitted = 0usize;
    let mut not_admitted = 0usize;
    for seat in &seats {
        let participant_id = s(seat, "id").unwrap_or_default();
        let character_id = s(seat, "characterId").unwrap_or_default();
        let cid = character_id.clone();
        let character = db
            .read_main(move |c| Ok(crate::db::characters_read::find_by_id_raw_or_none(c, &cid)))
            .ok()
            .flatten();
        let archived = character
            .as_ref()
            .and_then(|c| c.get("archivedAt"))
            .is_some_and(|v| crate::inventory::types::js_truthy(Some(v)));
        if character.is_none() || archived {
            not_admitted += 1;
            tracing::warn!(
                jobId = %job.id,
                chatId = %chat_id,
                participantId = %participant_id,
                characterId = %character_id,
                "[EstablishedFactMemories] Seat character missing or archived; skipping seat"
            );
            continue;
        }

        let candidate_json = established_fact_candidate(
            &fact,
            &character_id,
            &chat_id,
            project_id.as_deref(),
            &fact_id,
            created_at.as_deref(),
            deps.now_ms,
        );
        let candidate = match crate::memory_candidate::parse_memory_candidate(&candidate_json) {
            Ok(c) => c,
            Err(issues) => {
                // v4's `MemoryCandidateSchema.parse` throws into the seat's
                // catch; `getErrorMessage(zodError)` is its issue JSON.
                not_admitted += 1;
                tracing::warn!(
                    jobId = %job.id,
                    chatId = %chat_id,
                    participantId = %participant_id,
                    characterId = %character_id,
                    error = %serde_json::to_string_pretty(&issues).unwrap_or_default(),
                    "[EstablishedFactMemories] Admission failed for a seat; continuing"
                );
                continue;
            }
        };
        let resolved = crate::memory_candidate::ResolvedCandidateTime {
            occurred_at: None,
            narrative_time: None,
            source_message_timestamp: None,
        };
        let outcome = admit_candidate(
            db,
            deps.provider,
            &candidate,
            &resolved,
            AdmitOptions {
                user_id: &job.user_id,
                dry_run: false,
                backfill: false,
                dry_run_book: None,
                key_scope: deps.key_scope,
                host_zone_fallback: deps.host_zone_fallback,
            },
        )
        .await;
        match outcome {
            Ok(outcome) => match &outcome.memory {
                Some(memory) => {
                    admitted += 1;
                    let memory_id = memory.get("id").and_then(Value::as_str).unwrap_or_default();
                    tracing::debug!(
                        jobId = %job.id,
                        chatId = %chat_id,
                        participantId = %participant_id,
                        characterId = %character_id,
                        memoryId = %memory_id,
                        action = outcome.action.kind().as_str(),
                        "[EstablishedFactMemories] Seat admitted the fact"
                    );
                }
                None => {
                    not_admitted += 1;
                    let reason = match &outcome.action {
                        crate::admission::types::AdmissionAction::Reject { reason } => {
                            reason.as_str()
                        }
                        other => other.kind().as_str(),
                    };
                    tracing::warn!(
                        jobId = %job.id,
                        chatId = %chat_id,
                        participantId = %participant_id,
                        characterId = %character_id,
                        reason = reason,
                        "[EstablishedFactMemories] Seat did not admit the fact"
                    );
                }
            },
            Err(error) => {
                not_admitted += 1;
                tracing::warn!(
                    jobId = %job.id,
                    chatId = %chat_id,
                    participantId = %participant_id,
                    characterId = %character_id,
                    error = %crate::db::fallback::error_text(&error),
                    "[EstablishedFactMemories] Admission failed for a seat; continuing"
                );
            }
        }
    }

    tracing::info!(
        jobId = %job.id,
        chatId = %chat_id,
        messageId = %fact_id,
        seats = seats.len(),
        admitted = admitted,
        notAdmitted = not_admitted,
        "[EstablishedFactMemories] Established fact admitted to the cast"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Item 18: the four order-bearing job-type lists agree — the new type
    /// sits right after `CARINA_MEMORY_EXTRACTION` in each, as in v4's enum,
    /// and shares its `memories` topic arm (scoped by `chatId`).
    #[test]
    fn the_four_job_lists_agree_on_the_insertion_point() {
        fn after<'a>(list: &[&'a str]) -> Option<&'a str> {
            let i = list
                .iter()
                .position(|t| *t == "CARINA_MEMORY_EXTRACTION")
                .unwrap();
            list.get(i + 1).copied()
        }
        assert_eq!(
            after(crate::services::job_runner::KNOWN_JOB_TYPES),
            Some("ESTABLISHED_FACT_MEMORIES")
        );
        assert_eq!(
            after(crate::api::system_data::JOB_TYPES),
            Some("ESTABLISHED_FACT_MEMORIES")
        );
        let kinds: Vec<&str> = crate::services::activity_kinds::JOB_TYPE_ACTIVITY
            .iter()
            .map(|(t, _)| *t)
            .collect();
        assert_eq!(after(&kinds), Some("ESTABLISHED_FACT_MEMORIES"));
        assert_eq!(
            crate::services::activity_kinds::JOB_TYPE_ACTIVITY
                .iter()
                .find(|(t, _)| *t == "ESTABLISHED_FACT_MEMORIES")
                .map(|(_, k)| *k),
            Some(Some(crate::services::activity_kinds::ActivityKind::Memory))
        );
        let payload = json!({ "chatId": "c-1", "messageId": "m-1" });
        let topics = |t: &str| {
            format!(
                "{:?}",
                crate::realtime::job_topics::topics_for_completed_job(Some(t), Some(&payload))
            )
        };
        assert_eq!(
            topics("ESTABLISHED_FACT_MEMORIES"),
            topics("CARINA_MEMORY_EXTRACTION")
        );
    }

    #[test]
    fn the_candidate_falls_back_to_now_on_an_unparseable_stamp() {
        let c = established_fact_candidate("f", "h", "c", None, "m", Some("nope"), 42.0);
        assert_eq!(c["provenance"]["anchorTimeMs"], json!(42.0));
        let c = established_fact_candidate(
            "f",
            "h",
            "c",
            Some("p"),
            "m",
            Some("2026-01-01T00:00:00.000Z"),
            42.0,
        );
        assert_eq!(c["provenance"]["anchorTimeMs"], json!(1_767_225_600_000.0));
        assert_eq!(c["provenance"]["projectId"], json!("p"));
        // The candidate parses — the same refusal v4's schema would make is none.
        assert!(crate::memory_candidate::parse_memory_candidate(&c).is_ok());
    }
}
