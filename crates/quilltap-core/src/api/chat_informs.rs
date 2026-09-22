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
/// **This function walks the two fields and nothing else — it has no root-path
/// arm, and cannot have one.** Zod 4's `z.object` refuses a non-object body
/// BEFORE any field check, with ONE issue at the root path (the rule
/// `chat_delete::parse_stop_impersonate` does carry, because that verb keeps
/// the raw body). This verb does not: the wire carries the two keys already
/// LIFTED into the tri-state, so the body's SHAPE is not visible at this seam.
/// On `POST /api/dispatch` the envelope is an object by construction, so the
/// root arm is unreachable there; at the REST edge `request_envelope` folds a
/// non-object body to all-absent (see its header), and the two per-field
/// `invalid_type … received undefined` issues below are what v5 answers where
/// v4 would answer the single root issue. A **recorded divergence on a
/// non-object body only**, not a silent one — inventing a root arm here would
/// mean guessing which of `null` / `[]` / `42` the edge had folded, which the
/// lifted shape cannot tell us.
fn parse_inform(
    content_markdown: &Option<Option<Value>>,
    target_participant_ids: &Option<Option<Value>>,
) -> Result<(String, Option<Vec<String>>), Value> {
    let mut issues: Vec<Value> = Vec::new();

    // `contentMarkdown: z.string().min(1)`.
    //
    // `Some(None)` is an explicit JSON `null`, and Zod's `util.parsedType` calls
    // that **"null"**, never "undefined" (`zod_issues::zod_parsed_type`): only an
    // ABSENT key reads as `undefined`. So the explicit null is handed to the
    // issue as `Value::Null` rather than as "nothing".
    let explicit_null = Value::Null;
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
                        Some(inner.as_ref().unwrap_or(&explicit_null)),
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
///
/// Same lifted-tri-state shape as [`parse_inform`], and the same
/// `parsedType` rule: an ABSENT key is `undefined`, an explicit JSON `null` is
/// **`null`** (`zod_issues::zod_parsed_type`).
fn parse_cancel(batch_id: &Option<Option<Value>>) -> Result<String, Value> {
    let explicit_null = Value::Null;
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
                Some(inner.as_ref().unwrap_or(&explicit_null))
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

    // v4 logs `{ chatId, batchId, targetCount, audience, recordMessageId }`, and
    // BOTH ids are `… ?? null` — a JSON string or a JSON null. `%batch_id` on a
    // `serde_json::Value` renders the quotes INTO the field (`"\"uuid\""`), and
    // `unwrap_or("null")` logs the four-character string `null` where v4 logs a
    // real null. Both go through the file layer's `…Json` convention instead
    // (`quilltap_web::log_file::JSON_FIELD_SUFFIX`): the callsite serializes and
    // the layer re-parses under the un-suffixed name, so the record reads
    // `"batchId": "…"` / `"recordMessageId": null` exactly as v4's does.
    let batch_id_json = batch_id.to_string();
    let record_message_id_json = match &record_message_id {
        Some(id) => Value::String(id.clone()).to_string(),
        None => "null".to_string(),
    };
    tracing::info!(
        chat_id = %chat_id,
        batchIdJson = batch_id_json.as_str(),
        target_count = participant_ids.len(),
        audience = if record_targets.is_some() { "whisper" } else { "public" },
        recordMessageIdJson = record_message_id_json.as_str(),
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
    // v4 wraps `findByBatchId` in `safeQuery(..., [])`
    // (`chat-informs.repository.ts:163-170`): a failed read LOGS and answers the
    // EMPTY fallback, it never throws. So a broken read falls into the
    // `rows.is_empty()` arm below and the operator sees v4's 404 — not a 500.
    // (`safeQuery`'s rethrow leg only arms inside
    // `withStrictRepositoryFailures`, which only the importer enters.)
    let rows = match db.read_main(move |c| {
        crate::db::chat_informs::ChatInformsRepository::new(c).find_by_batch_id(&b1)
    }) {
        Ok(rows) => rows,
        Err(e) => {
            tracing::error!(
                batch_id = %batch,
                error = %e,
                "Error finding informs by batch ID",
            );
            Vec::new()
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
    // `deletePendingByBatch` is `safeQuery(..., 0)` too
    // (`chat-informs.repository.ts:250-270`): a failed delete LOGS and answers
    // 0, and v4's handler carries on to its 200 with `removed: 0`.
    let removed = match db
        .write(move |writers| writers.main().chat_informs().delete_pending_by_batch(&b2))
        .await
    {
        Ok(n) => n,
        Err(e) => {
            tracing::error!(
                batch_id = %batch,
                error = %e,
                "Error deleting pending informs by batch",
            );
            0
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

#[cfg(test)]
mod zod_rendering_tests {
    //! The `parsedType` pin for the two lifted tri-states (the `f45a517a9`
    //! unification §3, finding 3).
    //!
    //! `double_option` decodes an ABSENT key to `None` and an explicit JSON
    //! `null` to `Some(None)`, and the two must NOT render the same: Zod's
    //! `util.parsedType` calls a missing key `undefined` and a null `null`
    //! (`zod_issues::zod_parsed_type`). Handing `Some(None)` to
    //! `ZodIssue::invalid_type` as "nothing" — which the first port did — made
    //! `{"contentMarkdown": null}` answer v4's sentence for a key that was never
    //! sent. Both legs are asserted, so a fix in either direction that collapses
    //! them again reddens.

    use super::*;

    fn message(issues: &Value, index: usize) -> String {
        issues[index]["message"]
            .as_str()
            .unwrap_or_default()
            .to_string()
    }

    #[test]
    fn an_absent_content_markdown_is_received_undefined() {
        let err = parse_inform(&None, &Some(None)).expect_err("an absent key is an issue");
        assert_eq!(
            message(&err, 0),
            "Invalid input: expected string, received undefined"
        );
        assert_eq!(err[0]["path"], json!(["contentMarkdown"]));
    }

    #[test]
    fn an_explicit_null_content_markdown_is_received_null() {
        let err = parse_inform(&Some(None), &Some(None)).expect_err("null is not a string");
        assert_eq!(
            message(&err, 0),
            "Invalid input: expected string, received null"
        );
    }

    #[test]
    fn a_non_string_content_markdown_still_names_its_own_type() {
        let err = parse_inform(&Some(Some(json!(42))), &Some(None))
            .expect_err("a number is not a string");
        assert_eq!(
            message(&err, 0),
            "Invalid input: expected string, received number"
        );
    }

    #[test]
    fn an_absent_batch_id_is_received_undefined_and_an_explicit_null_is_received_null() {
        assert_eq!(
            message(&parse_cancel(&None).expect_err("absent"), 0),
            "Invalid input: expected string, received undefined"
        );
        assert_eq!(
            message(&parse_cancel(&Some(None)).expect_err("explicit null"), 0),
            "Invalid input: expected string, received null"
        );
        // The non-string leg keeps naming the type it actually saw.
        assert_eq!(
            message(
                &parse_cancel(&Some(Some(json!([])))).expect_err("an array"),
                0
            ),
            "Invalid input: expected string, received array"
        );
    }

    #[test]
    fn an_absent_targets_key_is_received_undefined_and_an_explicit_null_is_legal() {
        // `targetParticipantIds` IS `.nullable()`, so `Some(None)` is the
        // "Everyone" spelling and must produce NO issue — the other half of the
        // same tri-state, and the reason the two keys cannot share one arm.
        let ok = parse_inform(&Some(Some(json!("a passage"))), &Some(None))
            .expect("an explicit null means every eligible seat");
        assert_eq!(ok, ("a passage".to_string(), None));

        let err = parse_inform(&Some(Some(json!("a passage"))), &None)
            .expect_err("an ABSENT targets key is still an issue");
        assert_eq!(
            message(&err, 0),
            "Invalid input: expected array, received undefined"
        );
    }
}

#[cfg(test)]
mod safe_query_and_log_tests {
    //! The `safeQuery` fidelity pins (the `f45a517a9` unification §3, finding 7)
    //! and the posted-inform log line (nit 11).
    //!
    //! v4 wraps `findByBatchId` and `deletePendingByBatch` in
    //! `safeQuery(op, message, context, fallback)`
    //! (`chat-informs.repository.ts:163-170, 250-270`), which logs at ERROR and
    //! returns the fallback — it only rethrows inside
    //! `withStrictRepositoryFailures`, a scope the importer enters and no route
    //! does. So on v4 a broken read answers **404** and a broken delete answers
    //! **200 with `removed: 0`**; v5 answered 500 for both. Both are induced
    //! here rather than argued: a MISSING table for the read, and a
    //! `BEFORE DELETE … RAISE(ABORT)` trigger for the delete — which fails the
    //! DELETE while leaving the SELECT that precedes it working, the one shape
    //! that separates the two arms.

    use super::*;
    use crate::db::runtime::{Db, DbPaths};
    use crate::test_support::captured_with;

    const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
    const CHAT: &str = "c1000000-0000-4000-8000-000000000001";
    const BATCH: &str = "bbbbbbbb-0000-4000-8000-000000000001";
    const SEAT_A: &str = "e1000000-0000-4000-8000-000000000001";
    const SEAT_B: &str = "e1000000-0000-4000-8000-000000000002";

    fn line<'a>(lines: &'a [String], needle: &str) -> &'a str {
        lines
            .iter()
            .find(|l| l.contains(needle))
            .unwrap_or_else(|| panic!("no line containing {needle:?} in {lines:#?}"))
    }

    /// A full fresh instance — the real DDL, so `chats` / `chat_messages` /
    /// `chat_informs` are exactly what a provisioned instance carries.
    fn provisioned(tag: &str) -> (tempfile::TempDir, Db) {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join(tag);
        std::fs::create_dir_all(&data).unwrap();
        crate::services::provisioning::provision_fresh_instance(&data, PEPPER).unwrap();
        let db = Db::open(
            DbPaths {
                main: data.join("quilltap.db"),
                mount_index: Some(data.join("quilltap-mount-index.db")),
                llm_logs: None,
            },
            PEPPER,
        )
        .unwrap();
        (dir, db)
    }

    /// A two-LLM-seat room, so the eligibility gate passes and the coverage rule
    /// has something to be true about.
    async fn seed_chat(db: &Db) {
        let create: crate::db::chats::ChatCreate = serde_json::from_value(json!({
            "userId": crate::api::SINGLE_USER_ID,
            "title": "The Inform Room",
            "participants": [
                { "id": SEAT_A, "type": "CHARACTER", "controlledBy": "llm",
                  "characterId": "a1000000-0000-4000-8000-000000000001",
                  "createdAt": "2026-05-01T00:00:00.000Z",
                  "updatedAt": "2026-05-01T00:00:00.000Z" },
                { "id": SEAT_B, "type": "CHARACTER", "controlledBy": "llm",
                  "characterId": "a1000000-0000-4000-8000-000000000002",
                  "createdAt": "2026-05-01T00:00:00.000Z",
                  "updatedAt": "2026-05-01T00:00:00.000Z" }
            ],
        }))
        .expect("a ChatCreate");
        let opts = crate::db::chats::CreateOptions {
            id: CHAT.to_string(),
            created_at: "2026-05-01T00:00:00.000Z".to_string(),
            updated_at: "2026-05-01T00:00:00.000Z".to_string(),
        };
        db.write(move |w| {
            crate::db::chats::ChatsRepository::new(w.main().connection()).create(&create, &opts)
        })
        .await
        .expect("seed the chat");
    }

    #[test]
    fn a_broken_batch_read_answers_v4s_404_not_a_500() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let (_dir, db) = provisioned("a");
        // Drop the table: every `find_by_batch_id` now errors, which is exactly
        // the state v4's `safeQuery` answers `[]` for.
        rt.block_on(db.write(|w| {
            w.main()
                .connection()
                .execute_batch("DROP TABLE chat_informs")?;
            Ok(())
        }))
        .unwrap();

        let (resp, lines) =
            captured_with(|| rt.block_on(chat_inform_cancel(&db, CHAT, &Some(Some(json!(BATCH))))));
        match resp {
            Response::Error(e) => {
                assert!(
                    matches!(e.kind, ErrorKind::NotFound),
                    "v4's safeQuery fallback is `[]`, so the handler reaches its \
                     `rows.length === 0` arm: {e:?}"
                );
                assert_eq!(e.message, "Inform batch not found");
            }
            other => panic!("expected v4's 404, got {other:?}"),
        }
        let l = line(&lines, "Error finding informs by batch ID");
        assert!(l.starts_with("ERROR"), "v4's safeQuery logs at error: {l}");
        assert!(l.contains(&format!("batch_id={BATCH}")), "{l}");
    }

    #[test]
    fn a_broken_pending_delete_still_answers_200_with_removed_zero() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let (_dir, db) = provisioned("b");

        rt.block_on(db.write(|w| {
            let c = w.main().connection();
            crate::db::chat_informs::ChatInformsRepository::new(c).create(
                &crate::db::chat_informs::ChatInformCreate {
                    id: "11110000-0000-4000-8000-00000000aaa1".to_string(),
                    chat_id: CHAT.to_string(),
                    batch_id: BATCH.to_string(),
                    participant_id: SEAT_A.to_string(),
                    content_markdown: "The clock has stopped.".to_string(),
                    // NULL, so the record-delete leg is not entered and this
                    // case measures the delete arm alone.
                    record_message_id: None,
                    created_at: "2026-05-01T00:00:00.000Z".to_string(),
                    updated_at: "2026-05-01T00:00:00.000Z".to_string(),
                    consumed_at: None,
                    consumed_by_message_id: None,
                },
            )?;
            // Fails the DELETE while leaving the SELECT before it working.
            c.execute_batch(
                "CREATE TRIGGER no_delete BEFORE DELETE ON chat_informs \
                 BEGIN SELECT RAISE(ABORT, 'the delete is refused'); END",
            )?;
            Ok(())
        }))
        .unwrap();

        let (resp, lines) =
            captured_with(|| rt.block_on(chat_inform_cancel(&db, CHAT, &Some(Some(json!(BATCH))))));
        match resp {
            Response::ChatInformCancelled(v) => {
                assert_eq!(v["success"], json!(true));
                assert_eq!(
                    v["removed"],
                    json!(0),
                    "v4's safeQuery fallback for `deletePendingByBatch` is 0, and \
                     the handler carries on to its 200: {v}"
                );
                assert_eq!(v["recordDeleted"], json!(false));
            }
            other => panic!("expected v4's 200, got {other:?}"),
        }
        let l = line(&lines, "Error deleting pending informs by batch");
        assert!(l.starts_with("ERROR"), "{l}");
    }

    #[test]
    fn the_posted_inform_line_renders_both_ids_as_json() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let (_dir, db) = provisioned("c");
        rt.block_on(seed_chat(&db));

        let (resp, lines) = captured_with(|| {
            rt.block_on(chat_inform(
                &db,
                CHAT,
                &Some(Some(json!("The clock in the hall has stopped."))),
                &Some(None),
            ))
        });
        let body = match resp {
            Response::ChatInform(v) => v,
            other => panic!("the post must succeed for this pin to mean anything: {other:?}"),
        };
        let batch_id = body["batchId"].as_str().expect("a batch id");
        let record_id = body["message"]["id"].as_str().expect("a record message id");

        let l = line(&lines, "[Chats v1] Inform posted");
        assert!(l.starts_with("INFO"), "v4 logs this one at info: {l}");
        // The `…Json` convention: the callsite serializes, and the file layer
        // re-parses under the un-suffixed name. The thread-scoped capture sees
        // the RAW field, so the quotes are the proof the value is JSON and not
        // a bare interpolation of a `serde_json::Value`'s Display.
        assert!(
            l.contains(&format!("batchIdJson=\"{batch_id}\"")),
            "the batch id must render as a JSON string, once: {l}"
        );
        assert!(
            l.contains(&format!("recordMessageIdJson=\"{record_id}\"")),
            "{l}"
        );
        // `audience` is a plain `&str` field, so the fmt layer renders it
        // UNQUOTED — unlike the two `…Json` fields above, whose quotes are the
        // JSON's own and are the point of the assertions.
        assert!(
            l.contains("audience=public") && l.contains("target_count=2"),
            "both eligible seats were covered, so the record is public: {l}"
        );
        // The silence leg: the two ids never reach the wire under their old
        // spellings, which rendered `"\"uuid\""` and the literal string `null`.
        assert!(
            !l.contains("batch_id=") && !l.contains("record_message_id="),
            "the pre-fix field names must be gone: {l}"
        );
    }
}
