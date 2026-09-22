//! The Salon's **Inform** verbs (P4.D205, porting v4 `e7d77bb60`'s
//! `app/api/v1/chats/[id]/actions/inform.ts` + `schemas.ts:249-258`).
//!
//! An out-of-character passage the operator hands to one or more LLM-controlled
//! seats. Each target receives it verbatim as its own system block on their next
//! generation, and it is then consumed for them.
//!
//! The transcript keeps a record — a Host message carrying exactly what was
//! typed, public when everyone was targeted and whispered to the targets
//! otherwise. **The record is for the operator; it never reaches a model** (see
//! the three strips: `message_context`'s record-only pass,
//! `courier_transport`'s skip, and `chat_tasks::extract_visible_conversation`'s).
//!
//!   - `POST /api/v1/chats/{id}?action=inform`
//!   - `POST /api/v1/chats/{id}?action=cancel-inform`
//!   - `GET  /api/v1/chats/{id}?action=informs`

use serde_json::{json, Value};

use crate::db::runtime::Db;

use super::types::{ErrorKind, Response};
use super::zod_issues::{key, zod_uuid_ok, ZodIssue};

/// A seat that can be informed: a character still in the room, whose turns an
/// LLM takes (v4 `isEligibleSeat`).
///
/// A user-controlled seat never generates, so it could never collect what it was
/// handed. Impersonation is irrelevant here — it is an overlay, not a column
/// write, so an impersonated seat is still `controlledBy: 'llm'` and delivery
/// simply waits for that seat's next *LLM* generation.
fn is_eligible_seat(p: &Value) -> bool {
    let s = |k: &str| p.get(k).and_then(Value::as_str);
    s("type") == Some("CHARACTER")
        && s("controlledBy") == Some("llm")
        && s("status") != Some("removed")
        && p.get("removedAt").and_then(Value::as_str).is_none()
}

/// A seat still in the room (v4 `!p.removedAt && p.status !== 'removed'`) — the
/// GET's defensive filter.
fn is_current_seat(p: &Value) -> bool {
    p.get("removedAt").and_then(Value::as_str).is_none()
        && p.get("status").and_then(Value::as_str) != Some("removed")
}

fn participants_of(chat: &Value) -> Vec<Value> {
    chat.get("participants")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

/// v4 `informSchema` = `{ contentMarkdown: z.string().min(1),
/// targetParticipantIds: z.array(z.uuid()).min(1).nullable() }`.
///
/// Both keys are REQUIRED — an absent one is an `invalid_type … received
/// undefined`, which is why the wire carries them as the `double_option`
/// tri-state rather than a serde-typed field. `null` on the second means
/// Everyone; `null` on the first does NOT (the schema is not `.nullable()`).
///
/// Zod 4's `z.object` refuses a non-object body BEFORE any field check, with ONE
/// issue at the root path — the same rule `chat_delete::parse_stop_impersonate`
/// carries, and for the same reason: a per-field walk over `null` / `[]` / `42`
/// would invent field issues v4 never emits.
fn parse_inform(
    content_markdown: &Option<Option<Value>>,
    target_participant_ids: &Option<Option<Value>>,
) -> Result<(String, Option<Vec<String>>), Value> {
    let mut issues: Vec<Value> = Vec::new();

    // `contentMarkdown: z.string().min(1)`.
    let raw_content: Option<&Value> = content_markdown.as_ref().map(|v| v.as_ref()).and_then(
        |v: Option<&Value>| v, // Some(None) => absent-with-null; handled below
    );
    let content = match content_markdown {
        // The key was absent entirely.
        None => {
            issues.push(
                ZodIssue::invalid_type("string", vec![key("contentMarkdown")], None).to_value(),
            );
            String::new()
        }
        // Present. `Some(None)` is an explicit JSON `null`.
        Some(inner) => match inner.as_ref().and_then(Value::as_str) {
            Some(s) if !s.is_empty() => s.to_string(),
            Some(_) => {
                // `.min(1)` — an empty string is `too_small`, not `invalid_type`.
                issues.push(
                    ZodIssue::too_small_string(json!(1), vec![key("contentMarkdown")]).to_value(),
                );
                String::new()
            }
            None => {
                issues.push(
                    ZodIssue::invalid_type(
                        "string",
                        vec![key("contentMarkdown")],
                        inner.as_ref().or(raw_content),
                    )
                    .to_value(),
                );
                String::new()
            }
        },
    };

    // `targetParticipantIds: z.array(z.uuid()).min(1).nullable()`.
    let targets: Option<Vec<String>> = match target_participant_ids {
        None => {
            issues.push(
                ZodIssue::invalid_type("array", vec![key("targetParticipantIds")], None).to_value(),
            );
            None
        }
        // An explicit `null` is legal and means "every eligible seat".
        Some(None) => None,
        Some(Some(v)) => match v.as_array() {
            Some(arr) if arr.is_empty() => {
                issues.push(
                    ZodIssue::too_small_string(json!(1), vec![key("targetParticipantIds")])
                        .to_value(),
                );
                None
            }
            Some(arr) => {
                let mut out = Vec::with_capacity(arr.len());
                for (i, item) in arr.iter().enumerate() {
                    match item.as_str() {
                        Some(s) if zod_uuid_ok(s) => out.push(s.to_string()),
                        Some(_) => issues.push(
                            ZodIssue::invalid_uuid(vec![key("targetParticipantIds"), json!(i)])
                                .to_value(),
                        ),
                        None => issues.push(
                            ZodIssue::invalid_type(
                                "string",
                                vec![key("targetParticipantIds"), json!(i)],
                                Some(item),
                            )
                            .to_value(),
                        ),
                    }
                }
                Some(out)
            }
            None => {
                issues.push(
                    ZodIssue::invalid_type("array", vec![key("targetParticipantIds")], Some(v))
                        .to_value(),
                );
                None
            }
        },
    };

    if issues.is_empty() {
        Ok((content, targets))
    } else {
        Err(Value::Array(issues))
    }
}

/// v4 `cancelInformSchema` = `{ batchId: z.uuid() }`.
fn parse_cancel(batch_id: &Option<Option<Value>>) -> Result<String, Value> {
    match batch_id {
        None => Err(json!([ZodIssue::invalid_type(
            "string",
            vec![key("batchId")],
            None
        )
        .to_value()])),
        Some(inner) => match inner.as_ref().and_then(Value::as_str) {
            Some(s) if zod_uuid_ok(s) => Ok(s.to_string()),
            Some(_) => Err(json!([
                ZodIssue::invalid_uuid(vec![key("batchId")]).to_value()
            ])),
            None => Err(json!([ZodIssue::invalid_type(
                "string",
                vec![key("batchId")],
                inner.as_ref()
            )
            .to_value()])),
        },
    }
}

/// `POST ?action=inform` — post one passage to one, several, or every eligible
/// seat (v4 `handleInform`).
///
/// The guard ORDER is v4's and is load-bearing: 404 before eligibility, then
/// membership before eligibility on an explicit list — so an id that names a
/// real seat the operator simply may not inform is reported as such, rather than
/// as an unknown id.
pub async fn chat_inform(
    db: &Db,
    chat_id: &str,
    content_markdown: &Option<Option<Value>>,
    target_participant_ids: &Option<Option<Value>>,
) -> Response {
    // v4 `informSchema.parse(body)` runs BEFORE the chat read.
    let (content, requested_targets) = match parse_inform(content_markdown, target_participant_ids)
    {
        Ok(v) => v,
        Err(issues) => return Response::validation_error(issues),
    };

    let cid = chat_id.to_string();
    let chat = match db.read_main(move |c| crate::db::chats_read::find_by_id(c, &cid)) {
        Ok(Some(chat)) => chat,
        Ok(None) => return Response::error(ErrorKind::NotFound, "Chat not found"),
        Err(e) => {
            tracing::error!(chat_id = %chat_id, error = %e, "[Chats v1] Error posting inform");
            return Response::error(ErrorKind::Internal, "Failed to post inform");
        }
    };

    let participants = participants_of(&chat);
    let eligible_ids: Vec<String> = participants
        .iter()
        .filter(|p| is_eligible_seat(p))
        .filter_map(|p| p.get("id").and_then(Value::as_str).map(str::to_string))
        .collect();
    if eligible_ids.is_empty() {
        return Response::error(ErrorKind::BadRequest, "No LLM-controlled seat to inform.");
    }

    let participant_ids: Vec<String> = match requested_targets {
        None => eligible_ids.clone(),
        Some(requested) => {
            // Membership first (the same gate the whisper audience uses), then
            // eligibility.
            let audience = crate::services::announcer::audience::resolve_announcement_audience(
                db,
                chat_id,
                Some(&requested),
            );
            if !audience.unknown_ids.is_empty() {
                return Response::error(
                    ErrorKind::BadRequest,
                    format!(
                        "Unknown inform target(s) for this chat: {}",
                        audience.unknown_ids.join(", ")
                    ),
                );
            }
            let resolved = audience.target_participant_ids.unwrap_or_default();
            let eligible: std::collections::HashSet<&str> =
                eligible_ids.iter().map(String::as_str).collect();
            let ineligible: Vec<&str> = resolved
                .iter()
                .map(String::as_str)
                .filter(|id| !eligible.contains(id))
                .collect();
            if !ineligible.is_empty() {
                return Response::error(
                    ErrorKind::BadRequest,
                    format!(
                        "Not an LLM-controlled seat in this chat: {}",
                        ineligible.join(", ")
                    ),
                );
            }
            resolved
        }
    };

    if participant_ids.is_empty() {
        return Response::error(ErrorKind::BadRequest, "No LLM-controlled seat to inform.");
    }

    // Coverage, not clicks, decides whether the record is public: an operator who
    // ticks every seat by hand has informed the whole company, and the transcript
    // should say so.
    let covers_everyone = participant_ids.len() == eligible_ids.len();
    let record_targets: Option<Vec<String>> = if covers_everyone {
        None
    } else {
        Some(participant_ids.clone())
    };

    let message = crate::services::announcer::writer::post_inform_record(
        db,
        &crate::services::announcer::writer::InformRecordParams {
            chat_id: chat_id.to_string(),
            content_markdown: content.clone(),
            target_participant_ids: record_targets.clone(),
        },
    )
    .await;

    if message.is_none() {
        // A lost record must not cost the operator the batch — the informs still
        // deliver; only the transcript's note of them is missing.
        tracing::warn!(
            chat_id = %chat_id,
            target_count = participant_ids.len(),
            "[Chats v1] Inform record could not be posted — continuing",
        );
    }

    let record_message_id: Option<String> = message
        .as_ref()
        .and_then(|m| m.get("id").and_then(Value::as_str))
        .map(str::to_string);

    // v4 trims the body for the ROWS (`contentMarkdown.trim()`) — the record
    // writer trims independently.
    let trimmed = crate::jsstr::js_trim(&content).to_string();
    let chat_owned = chat_id.to_string();
    let ids_owned = participant_ids.clone();
    let record_owned = record_message_id.clone();
    let rows = match db
        .write(move |writers| {
            writers.main().chat_informs().create_batch(
                &chat_owned,
                &trimmed,
                &ids_owned,
                record_owned.as_deref(),
            )
        })
        .await
    {
        Ok(rows) => rows,
        Err(e) => {
            tracing::error!(chat_id = %chat_id, error = %e, "[Chats v1] Error posting inform");
            return Response::error(ErrorKind::Internal, "Failed to post inform");
        }
    };

    let batch_id: Value = rows
        .first()
        .map(|r| Value::String(r.batch_id.clone()))
        .unwrap_or(Value::Null);

    tracing::info!(
        chat_id = %chat_id,
        batch_id = %batch_id,
        target_count = participant_ids.len(),
        audience = if record_targets.is_some() { "whisper" } else { "public" },
        record_message_id = record_message_id.as_deref().unwrap_or("null"),
        "[Chats v1] Inform posted",
    );

    Response::ChatInform(json!({
        "success": true,
        "batchId": batch_id,
        "targetParticipantIds": match &record_targets {
            Some(ids) => json!(ids),
            None => Value::Null,
        },
        "message": message.unwrap_or(Value::Null),
    }))
}

/// `GET ?action=informs` — the pending batches, for the composer's chip (v4
/// `handleGetInforms`).
///
/// Rows whose seat has left the chat are filtered out defensively; the
/// remove-participant path deletes them, so this only ever catches a row that
/// outlived its seat some other way. A batch left with no seats is dropped.
///
/// v4 answers with a plain `NextResponse.json({ batches })` — NOT its `success`
/// envelope.
pub async fn chat_informs_list(db: &Db, chat_id: &str) -> Response {
    let cid = chat_id.to_string();
    let chat = match db.read_main(move |c| crate::db::chats_read::find_by_id(c, &cid)) {
        Ok(Some(chat)) => chat,
        Ok(None) => return Response::error(ErrorKind::NotFound, "Chat not found"),
        Err(e) => {
            tracing::error!(chat_id = %chat_id, error = %e, "[Chats v1] Error listing informs");
            return Response::error(ErrorKind::Internal, "Failed to list informs");
        }
    };

    let current: std::collections::HashSet<String> = participants_of(&chat)
        .iter()
        .filter(|p| is_current_seat(p))
        .filter_map(|p| p.get("id").and_then(Value::as_str).map(str::to_string))
        .collect();

    let cid2 = chat_id.to_string();
    let all = db
        .read_main(move |c| {
            crate::db::chat_informs::ChatInformsRepository::new(c).find_pending_batches(&cid2)
        })
        .unwrap_or_default();
    let total = all.len();

    let batches: Vec<Value> = all
        .into_iter()
        .filter_map(|b| {
            let pending: Vec<String> = b
                .pending_participant_ids
                .into_iter()
                .filter(|id| current.contains(id))
                .collect();
            if pending.is_empty() {
                return None;
            }
            Some(json!({
                "batchId": b.batch_id,
                "contentMarkdown": b.content_markdown,
                "createdAt": b.created_at,
                // v4 coalesces explicitly (`row.recordMessageId ?? null`) —
                // without it the key would be absent, because the backend turns
                // every NULL cell into `undefined` on read.
                "recordMessageId": match b.record_message_id {
                    Some(id) => Value::String(id),
                    None => Value::Null,
                },
                "pendingParticipantIds": pending,
            }))
        })
        .collect();

    tracing::debug!(
        chat_id = %chat_id,
        batches = batches.len(),
        dropped = total - batches.len(),
        "[Chats v1] Pending informs listed",
    );

    Response::ChatInforms(json!({ "batches": batches }))
}

/// `POST ?action=cancel-inform` — withdraw a batch's still-pending targets (v4
/// `handleCancelInform`).
///
/// A seat that already read the passage keeps its consumed row (a later swipe of
/// that turn must still re-apply it), and the record stays with it. When nothing
/// was consumed the record goes too: it would otherwise document something that
/// never happened.
pub async fn chat_inform_cancel(
    db: &Db,
    chat_id: &str,
    batch_id: &Option<Option<Value>>,
) -> Response {
    let batch = match parse_cancel(batch_id) {
        Ok(b) => b,
        Err(issues) => return Response::validation_error(issues),
    };

    let b1 = batch.clone();
    let rows = match db.read_main(move |c| {
        crate::db::chat_informs::ChatInformsRepository::new(c).find_by_batch_id(&b1)
    }) {
        Ok(rows) => rows,
        Err(e) => {
            tracing::error!(chat_id = %chat_id, error = %e, "[Chats v1] Error cancelling inform");
            return Response::error(ErrorKind::Internal, "Failed to cancel inform");
        }
    };
    if rows.is_empty() {
        return Response::error(ErrorKind::NotFound, "Inform batch not found");
    }
    if rows.iter().any(|r| r.chat_id != chat_id) {
        return Response::error(
            ErrorKind::BadRequest,
            "That inform belongs to another conversation.",
        );
    }

    let any_consumed = rows.iter().any(|r| r.consumed_at.is_some());
    let record_message_id: Option<String> = rows
        .iter()
        .find(|r| r.record_message_id.is_some())
        .and_then(|r| r.record_message_id.clone());

    let b2 = batch.clone();
    let removed = match db
        .write(move |writers| writers.main().chat_informs().delete_pending_by_batch(&b2))
        .await
    {
        Ok(n) => n,
        Err(e) => {
            tracing::error!(chat_id = %chat_id, error = %e, "[Chats v1] Error cancelling inform");
            return Response::error(ErrorKind::Internal, "Failed to cancel inform");
        }
    };

    let mut record_deleted = false;
    if !any_consumed {
        if let Some(record_id) = &record_message_id {
            let cid = chat_id.to_string();
            let rid = record_id.clone();
            match db
                .write(move |writers| {
                    writers
                        .main()
                        .chat_messages()
                        .delete_messages_by_ids(&cid, &[rid])
                })
                .await
            {
                Ok(deleted) => record_deleted = deleted > 0,
                Err(e) => {
                    // The rows are already gone; a surviving record is untidy,
                    // not broken.
                    tracing::warn!(
                        chat_id = %chat_id,
                        batch_id = %batch,
                        record_message_id = %record_id,
                        error = %e,
                        "[Chats v1] Could not delete inform record message",
                    );
                }
            }
        }
    }

    // Deleting pending rows touches no message row, so nothing else fires the
    // hint that refreshes the composer's chip.
    crate::realtime::bus::publish_realtime(
        crate::realtime::types::RealtimeTopic::Chats,
        Some(chat_id),
    );

    tracing::debug!(
        chat_id = %chat_id,
        batch_id = %batch,
        removed,
        any_consumed,
        record_deleted,
        "[Chats v1] Inform cancelled",
    );

    Response::ChatInformCancelled(json!({
        "success": true,
        "removed": removed,
        "recordDeleted": record_deleted,
    }))
}
