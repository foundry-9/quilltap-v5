//! Operator post — recording what the operator just sent (v4
//! `lib/services/chat-message/operator-post.service.ts`, `a8214b253` bug 262,
//! at `236c04e35`).
//!
//! Everything an operator's send puts in the transcript, in the order it
//! appears there:
//!
//!   1. staged tool results from the composer, as TOOL rows;
//!   2. dice / coin / bottle patterns auto-detected in the text, rolled and
//!      saved as TOOL rows (they precede the words that asked for them);
//!   3. the USER message itself, its attachment links and the Librarian's note
//!      of any image uploads;
//!   4. Pascal's trigger-phrase outcomes (they follow the words that caused
//!      them);
//!   5. an inline `@Name:` / `@Name?` Carina query and its answer.
//!
//! This is the one place a post is recorded. Every path that takes an operator
//! message — a turn about to be answered, a paused chat holding the post, a
//! storyteller round, the fair-rotation pause for another user-driven seat —
//! records it here, so no path keeps a thinner copy of the same message
//! (bug 262). Rows that the answering character should hear are spliced onto
//! the turn's in-memory context as they land; the database copy covers later
//! turns.
//!
//! ## v5 notes (P4.D348)
//!
//!   - **The danger-flags attach stays RETIRED.** v4's `attachDangerFlags` step
//!     stays in its source guarded on a non-empty list, but after `ce2f1dabf`
//!     (#77) only the classifier PRE-SCREEN writes flags, and v5 has no
//!     pre-screen branch (the classify is an injected seam). The synthesized
//!     Unmoderated flags were v5's ONLY writer, so the attach had no input left
//!     and was deleted rather than kept dead (the note moved here from the
//!     orchestrator).
//!   - **The Librarian upload note's failure WARN is unreachable.** v4 wraps
//!     `postLibrarianUploadAnnouncement` in a `Failed to post Librarian upload
//!     announcement` WARN; v5's writer
//!     ([`super::librarian_notifications::post_librarian_upload_announcement`])
//!     returns `Option` and never fails — the `image_attempt.rs` module-note
//!     class.
//!   - **The Carina markup pass is wired on BOTH paths** (R-1), through the
//!     existing [`super::carina_runner::run_carina_markup_query`] over the
//!     turn's own `carina_query` / `prospero` seams. Generic over them
//!     ([`OperatorPostCarina`]) because [`RunCarinaQuery`] returns an RPITIT
//!     future and is not object-safe.
//!   - **Every line is logged on the CALLER thread**, never inside a `Db::write`
//!     closure (the capture rig cannot see the writer thread).

use std::cell::Cell;

use serde_json::{json, Value};

use super::carina_runner::{
    run_carina_markup_query, CarinaMarkupCallbacks, CarinaMarkupLogLabels, CarinaMarkupOptions,
    PostProsperoCarinaError, PostedCarinaMessage, RunCarinaQuery,
};
use super::chat_events::{ChatEvent, EventSink, StatusPayload};
use super::orchestrator::{OrchestratorChatSettings, PendingToolResult, SubmissionLedger};
use super::turn_control::TurnControlHandle;
use crate::db::runtime::Db;
use crate::db::DbError;
use crate::pascal::llm_consult::ConsultRunner;
use crate::rng_patterns::detect_and_convert_rng_patterns;
use crate::tools::rng::{
    execute_rng_tool, format_rng_results, RandomBytes, RngToolContext, RngType,
};

const TARGET: &str = "quilltap::operator_post";

/// One of the operator's uploads named by a send, as the chat's own file record
/// (v4 `AttachedFile`). Distinct from `recovery::AttachedFile` (a different
/// shape for a different reader).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AttachedFile {
    pub id: String,
    /// `api/files/<id>`.
    pub filepath: String,
    pub filename: String,
    pub mime_type: String,
    pub size: i64,
}

/// The seat a post is attributed to (`participant_id: None` — the bare
/// operator).
#[derive(Clone, Copy, Debug, Default)]
pub struct Poster<'a> {
    pub participant: Option<&'a Value>,
    pub participant_id: Option<&'a str>,
}

/// The character the turn's status lines name (v4 `createTurnStatusEmitter`'s
/// optional `character`). `None` on the fair-rotation pause, which has no
/// responder.
#[derive(Clone, Copy, Debug)]
pub struct StatusCharacter<'a> {
    pub name: &'a str,
    pub id: &'a str,
}

/// The Carina markup pass's two seams (R-1) — the turn's own `carina_query` and
/// `prospero` deps, borrowed for the pass.
pub struct OperatorPostCarina<'a, Q, P> {
    pub query: &'a mut Q,
    pub prospero: &'a mut P,
}

/// Everything [`record_operator_post`] records and where its side effects go
/// (v4 `OperatorPostParams`).
pub struct OperatorPostParams<'a, SNK, Q, P>
where
    SNK: EventSink + Sync,
{
    pub chat_id: &'a str,
    pub user_id: &'a str,
    /// The room's `projectId` (the Pascal trigger site's room).
    pub project_id: Option<&'a str>,
    pub chat_settings: Option<&'a OrchestratorChatSettings>,
    /// What the operator typed (`""` for an attachments- or tools-only send).
    pub content: &'a str,
    /// The file ids the send named; the USER row lists them all.
    pub file_ids: &'a [String],
    /// Those of them that are the chat's own files ([`find_attached_files`]);
    /// only these are linked.
    pub attached_files: &'a [AttachedFile],
    pub pending_tool_results: &'a [PendingToolResult],
    pub target_participant_ids: Option<&'a [String]>,
    /// P4.D372 (v4 `db6190b44`): the line is posted as established fact — the
    /// USER row carries `isEstablishedFact: true` (absent otherwise → NULL).
    pub is_established_fact: bool,
    pub poster: Poster<'a>,
    /// The turn's in-memory context; what the answering character must hear is
    /// pushed onto it.
    pub context: &'a mut Vec<Value>,
    pub ledger: &'a Cell<SubmissionLedger>,
    pub sink: &'a SNK,
    /// The spine's ONE clock reading for the post (the rows' `createdAt`).
    pub now_ms: i64,
    pub rng_bytes: &'a mut dyn RandomBytes,
    /// The custom-tool consult runner the trigger site consults through.
    pub consult: Option<&'a dyn ConsultRunner>,
    /// The Carina markup pass's seams (R-1). `None` skips the pass (a unit
    /// test's shape; both production paths pass `Some`).
    pub carina: Option<OperatorPostCarina<'a, Q, P>>,
    /// The character the `Consulting …` status names (`None`: no responder).
    pub status_character: Option<StatusCharacter<'a>>,
    /// The send's turn-control handle (`TurnStopReach`), handed to the trigger
    /// site.
    pub turn_control: Option<&'a TurnControlHandle>,
}

/// What [`record_operator_post`] wrote (v4 `OperatorPostResult`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OperatorPostResult {
    /// The USER row's id, or `None` when the send carried no words.
    pub user_message_id: Option<String>,
}

/// The TOOL-message `content` JSON for an auto-detected RNG execution (v4's
/// `JSON.stringify({ tool, initiatedBy, success, result, prompt, arguments })`).
/// A typed struct in v4's field order so the stored bytes are byte-identical.
#[derive(serde::Serialize)]
struct RngToolMessageContent<'a> {
    tool: &'static str,
    #[serde(rename = "initiatedBy")]
    initiated_by: &'static str,
    success: bool,
    result: &'a str,
    prompt: &'a str,
    arguments: RngArguments,
}

/// The `arguments` object of an RNG TOOL message (v4
/// `{ type, rolls, modifier: pattern.modifier ?? 0 }`).
///
/// `modifier` is NOT optional here, unlike on the `RngToolCall` it comes from:
/// v4's `?? 0` runs at this site, so a coin-flip pattern — which carries no
/// modifier at all — still persists `modifier: 0` in its TOOL row.
#[derive(serde::Serialize)]
struct RngArguments {
    #[serde(rename = "type")]
    type_: RngType,
    rolls: u32,
    modifier: i64,
}

/// The operator's uploads named by a send, as the chat's own file records (v4
/// `findAttachedFiles`): only ids that are actually linked to this chat count,
/// so a stale or foreign id is dropped rather than linked to the new message.
/// An empty `file_ids` answers `[]` with NO read.
pub async fn find_attached_files(
    db: &Db,
    chat_id: &str,
    file_ids: &[String],
) -> Result<Vec<AttachedFile>, DbError> {
    if file_ids.is_empty() {
        return Ok(Vec::new());
    }
    let chat_id = chat_id.to_string();
    let chat_files = db.read_main(move |c| {
        crate::db::files::FilesRepository::new(c).find_by_linked_to(&chat_id)
    })?;
    Ok(chat_files
        .into_iter()
        .filter(|f| file_ids.contains(&f.id))
        .map(|f| AttachedFile {
            filepath: format!("api/files/{}", f.id),
            id: f.id,
            filename: f.original_filename,
            mime_type: f.mime_type,
            size: f.size,
        })
        .collect())
}

/// One `addMessage` through the writer.
async fn add_message(db: &Db, chat_id: &str, msg: &Value) -> Result<(), DbError> {
    let event: crate::db::chats_messages::ChatEventInput = serde_json::from_value(msg.clone())
        .map_err(|e| DbError::Internal(format!("operator post message marshal: {e}")))?;
    let write_chat_id = chat_id.to_string();
    db.write(move |w| w.main().chat_messages().add_message(&write_chat_id, &event))
        .await
}

/// Record an operator's send (v4 `recordOperatorPost`). Fails only if a TOOL or
/// USER row cannot be written; every side effect after the USER row is
/// best-effort.
pub async fn record_operator_post<SNK, Q, P>(
    db: &Db,
    params: OperatorPostParams<'_, SNK, Q, P>,
) -> Result<OperatorPostResult, DbError>
where
    SNK: EventSink + Sync,
    Q: RunCarinaQuery,
    P: PostProsperoCarinaError,
{
    let OperatorPostParams {
        chat_id,
        user_id,
        project_id,
        chat_settings,
        content,
        file_ids,
        attached_files,
        pending_tool_results,
        target_participant_ids,
        is_established_fact,
        poster,
        context,
        ledger,
        sink,
        now_ms,
        rng_bytes,
        consult,
        carina,
        status_character,
        turn_control,
    } = params;

    save_pending_tool_results(db, chat_id, pending_tool_results, context, ledger).await?;
    roll_detected_rng(
        db,
        chat_id,
        user_id,
        content,
        chat_settings,
        context,
        now_ms,
        rng_bytes,
    )
    .await?;

    let mut user_message_id: Option<String> = None;
    if !content.is_empty() {
        let id = uuid::Uuid::new_v4().to_string();
        // v4 writes `participantId` and `targetParticipantIds: … || null`
        // ALWAYS (R-4) — an absent key and `null` both store SQL NULL.
        let mut msg = json!({
            "id": id,
            "type": "message",
            "role": "USER",
            "content": content,
            "createdAt": crate::clock::iso_from_unix_ms(now_ms),
            "attachments": file_ids,
            "participantId": poster.participant_id,
            "targetParticipantIds": target_participant_ids,
        });
        // P4.D372: v4's `...(isEstablishedFact ? { isEstablishedFact: true } : {})`
        // — the LAST key, only when set.
        if is_established_fact {
            msg["isEstablishedFact"] = json!(true);
        }
        add_message(db, chat_id, &msg).await?;
        ledger.set(SubmissionLedger {
            user_message_persisted: true,
            ..ledger.get()
        });
        tracing::debug!(
            target: TARGET,
            chatId = %chat_id,
            userMessageId = %id,
            isEstablishedFact = is_established_fact,
            "[OperatorPost] User message persisted",
        );
        if is_established_fact {
            tracing::info!(
                target: TARGET,
                chatId = %chat_id,
                userMessageId = %id,
                "[OperatorPost] Operator line posted as established fact",
            );
        }

        link_attachments(db, chat_id, attached_files, &id).await;
        announce_image_uploads(db, chat_id, attached_files).await;
        user_message_id = Some(id);
    }

    // Pascal's announcement must FOLLOW the words that caused it, so it runs
    // after the persist (unlike the dice above). The seat speaks for itself: a
    // user-driven or impersonated character's words are that character's, and
    // its roster is theirs; the bare operator rolls as nobody. Never fails.
    {
        let trigger_settings =
            chat_settings.map(|s| crate::pascal::trigger_runner::TriggerSettings {
                custom_tools: Some(s.custom_tools),
                custom_tool_triggers: Some(s.custom_tool_triggers),
            });
        let mut on_posted = |message: &Value| sink.emit(ChatEvent::pascal_result(message.clone()));
        super::trigger_phrase_sites::run_user_message_triggers(
            db,
            super::trigger_phrase_sites::UserMessageTriggerParams {
                user_id,
                chat_id,
                project_id,
                content,
                is_continue_mode: false,
                user_participant: poster.participant,
                user_participant_id: poster.participant_id,
                chat_settings: trigger_settings,
                now_ms,
                turn_control,
            },
            consult,
            context,
            &mut on_posted,
        )
        .await;
    }

    // Carina: if the operator addressed an answerer with @Name: / @Name?, fire
    // the isolated reference call. A side effect — the responding participant
    // still takes its turn ("both fire"). The answer carries systemSender
    // 'carina', so it is excluded from memory extraction automatically.
    if !content.is_empty() {
        if let Some(OperatorPostCarina { query, prospero }) = carina {
            let opts = CarinaMarkupOptions {
                user_id: user_id.to_string(),
                chat_id: chat_id.to_string(),
                text: content.to_string(),
                asker_participant_id: poster.participant_id.map(str::to_string),
                // The human operator typed this markup — they can reach any
                // character, regardless of whether their persona is itself a
                // Carina answerer.
                operator_initiated: true,
                log_labels: CarinaMarkupLogLabels {
                    detected: "user message".to_string(),
                    failed: "user-message".to_string(),
                },
            };
            // The answer itself is surfaced live (`carinaAnswer`) by the query
            // seam the instant it posts, before any character starts responding.
            let mut callbacks = UserMessageCarina {
                sink,
                status_character,
                context: &mut *context,
            };
            run_carina_markup_query(&opts, &mut callbacks, query, prospero).await;
        }
    }

    Ok(OperatorPostResult { user_message_id })
}

/// The user-message path's two Carina callbacks (v4's `onConsulting` /
/// `onPublicAnswer`).
struct UserMessageCarina<'a, 'c, SNK> {
    sink: &'a SNK,
    status_character: Option<StatusCharacter<'a>>,
    context: &'c mut Vec<Value>,
}

impl<SNK: EventSink + Sync> CarinaMarkupCallbacks for UserMessageCarina<'_, '_, SNK> {
    fn on_consulting(&mut self, character_name: &str) {
        self.sink.emit(ChatEvent::status(StatusPayload {
            stage: "gathering".into(),
            message: format!("Consulting {character_name}..."),
            tool_name: None,
            character_name: self.status_character.map(|c| c.name.to_string()),
            character_id: self.status_character.map(|c| c.id.to_string()),
        }));
    }

    /// A PUBLIC answer joins this turn's context so the first character to
    /// respond in the same cycle hears it.
    fn on_public_answer(&mut self, message: &PostedCarinaMessage) {
        self.context.push(message.message.clone());
    }
}

/// Staged composer tool results become TOOL rows ahead of the message (v4
/// `savePendingToolResults`). The content JSON is byte-exact in v4's field
/// order.
async fn save_pending_tool_results(
    db: &Db,
    chat_id: &str,
    pending: &[PendingToolResult],
    context: &mut Vec<Value>,
    ledger: &Cell<SubmissionLedger>,
) -> Result<(), DbError> {
    if pending.is_empty() {
        return Ok(());
    }
    for tr in pending {
        let content_str = serde_json::to_string(&json!({
            "tool": tr.tool,
            "initiatedBy": "user",
            "success": tr.success,
            "result": tr.result,
            "prompt": tr.prompt,
            "arguments": tr.arguments,
        }))
        .map_err(|e| DbError::Internal(format!("pending tool result marshal: {e}")))?;
        let msg = json!({
            "id": uuid::Uuid::new_v4().to_string(),
            "type": "message",
            "role": "TOOL",
            "content": content_str,
            "createdAt": tr.created_at,
            "attachments": [],
        });
        add_message(db, chat_id, &msg).await?;
        context.push(msg);
    }
    ledger.set(SubmissionLedger {
        pending_tool_results_persisted: true,
        ..ledger.get()
    });
    tracing::debug!(
        target: TARGET,
        chatId = %chat_id,
        count = pending.len(),
        "[OperatorPost] Pending tool results persisted",
    );
    Ok(())
}

/// Dice rolls, coin flips and spin-the-bottle patterns in the text are rolled
/// and saved as TOOL rows (chat setting `autoDetectRng`, default on; v4
/// `rollDetectedRng`).
#[allow(clippy::too_many_arguments)]
async fn roll_detected_rng(
    db: &Db,
    chat_id: &str,
    user_id: &str,
    content: &str,
    chat_settings: Option<&OrchestratorChatSettings>,
    context: &mut Vec<Value>,
    now_ms: i64,
    rng_bytes: &mut dyn RandomBytes,
) -> Result<(), DbError> {
    if content.is_empty() || chat_settings.is_some_and(|s| !s.auto_detect_rng) {
        return Ok(());
    }
    let rng_calls = detect_and_convert_rng_patterns(content);
    if rng_calls.is_empty() {
        return Ok(());
    }

    let patterns: Vec<Value> = rng_calls
        .iter()
        .map(|p| json!({ "type": p.type_, "rolls": p.rolls, "matchText": p.match_text }))
        .collect();
    let patterns = Value::Array(patterns);
    tracing::info!(
        target: TARGET,
        chatId = %chat_id,
        userId = %user_id,
        patternCount = rng_calls.len(),
        patterns = %patterns,
        "Auto-detected RNG patterns in user message",
    );

    for call in &rng_calls {
        let rng_ctx = RngToolContext {
            user_id: user_id.to_string(),
            chat_id: chat_id.to_string(),
        };
        // v4 `{ type, rolls, modifier: pattern.modifier ?? 0 }` — the same shape
        // reaches the executor AND the persisted TOOL row below.
        let call_modifier = call.modifier.unwrap_or(0);
        let rng_input =
            json!({ "type": call.type_, "rolls": call.rolls, "modifier": call_modifier });
        let output = execute_rng_tool(db, &rng_input, &rng_ctx, rng_bytes)?;
        let formatted = format_rng_results(&output);
        let content_str = serde_json::to_string(&RngToolMessageContent {
            tool: "rng",
            initiated_by: "auto-detect",
            success: output.success,
            result: &formatted,
            prompt: &call.match_text,
            arguments: RngArguments {
                type_: call.type_,
                rolls: call.rolls,
                modifier: call_modifier,
            },
        })
        .map_err(|e| DbError::Internal(format!("rng tool content marshal: {e}")))?;
        let msg = json!({
            "id": uuid::Uuid::new_v4().to_string(),
            "type": "message",
            "role": "TOOL",
            "content": content_str,
            "createdAt": crate::clock::iso_from_unix_ms(now_ms),
            "attachments": [],
        });
        add_message(db, chat_id, &msg).await?;
        context.push(msg);
    }
    Ok(())
}

/// Link each of the chat's own uploads to the new USER row (v4
/// `linkAttachments`) — SOFT: a failed link is a WARN, never a failed post.
async fn link_attachments(
    db: &Db,
    chat_id: &str,
    attached_files: &[AttachedFile],
    message_id: &str,
) {
    for file in attached_files {
        let (fid, mid) = (file.id.clone(), message_id.to_string());
        if let Err(error) = db
            .write(move |w| w.main().files().add_link(&fid, &mid).map(|_| ()))
            .await
        {
            tracing::warn!(
                target: TARGET,
                chatId = %chat_id,
                fileId = %file.id,
                error = %error,
                "[OperatorPost] Failed to link attachment to user message",
            );
        }
    }
}

/// Surface uploaded image ids in the transcript so the LLM has the handle that
/// `keep_image` / `attach_image` need (v4 `announceImageUploads`). The message
/// already carries the bytes, so the Librarian's note is text-only.
async fn announce_image_uploads(db: &Db, chat_id: &str, attached_files: &[AttachedFile]) {
    use super::librarian_notifications::{
        post_librarian_upload_announcement, LibrarianUpload, LibrarianUploadAnnouncement,
    };
    let uploads: Vec<LibrarianUpload> = attached_files
        .iter()
        .filter(|f| f.mime_type.to_lowercase().starts_with("image/"))
        .map(|f| LibrarianUpload {
            file_id: f.id.clone(),
            filename: f.filename.clone(),
        })
        .collect();
    if uploads.is_empty() {
        return;
    }
    post_librarian_upload_announcement(
        db,
        &LibrarianUploadAnnouncement {
            chat_id: chat_id.to_string(),
            uploads,
        },
    )
    .await;
}

// === P4.D372 ===
#[cfg(test)]
mod established_fact_post_tests {
    use super::*;
    use crate::services::carina_runner::{
        CarinaResult, CarinaRunError, ProsperoCarinaErrorArgs, RunCarinaQueryOptions,
    };

    struct NoQuery;
    impl RunCarinaQuery for NoQuery {
        #[allow(clippy::manual_async_fn)]
        fn run(
            &mut self,
            _o: RunCarinaQueryOptions,
        ) -> impl std::future::Future<Output = Result<CarinaResult, CarinaRunError>> + Send
        {
            async { Err(CarinaRunError("unused".into())) }
        }
    }
    struct NoProspero;
    impl PostProsperoCarinaError for NoProspero {
        fn post(&mut self, _args: ProsperoCarinaErrorArgs) -> Result<(), CarinaRunError> {
            Ok(())
        }
    }
    struct NoBytes;
    impl crate::pascal::dice::RandomBytes for NoBytes {
        fn random_bytes(&mut self, n: usize) -> Vec<u8> {
            vec![0; n]
        }
    }

    const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
    const CHAT: &str = "c3720000-0000-4000-8000-000000000001";

    /// v4 `recordOperatorPost` (`db6190b44`): the row's `isEstablishedFact`
    /// key LAST and only when set; the DEBUG's third key on EVERY post; the
    /// INFO only when set (its silence leg on the plain post).
    #[test]
    fn the_flag_rides_the_row_and_the_lines() {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("data");
        std::fs::create_dir_all(&data).unwrap();
        crate::services::provisioning::provision_fresh_instance(&data, PEPPER).unwrap();
        let db = Db::open(
            crate::db::runtime::DbPaths {
                main: data.join("quilltap.db"),
                mount_index: Some(data.join("quilltap-mount-index.db")),
                llm_logs: None,
            },
            PEPPER,
        )
        .unwrap();
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let create: crate::db::chats::ChatCreate = serde_json::from_value(json!({
            "userId": crate::api::SINGLE_USER_ID,
            "title": "Deus",
            "participants": [],
        }))
        .unwrap();
        let opts = crate::db::chats::CreateOptions {
            id: CHAT.to_string(),
            created_at: "2026-05-01T00:00:00.000Z".to_string(),
            updated_at: "2026-05-01T00:00:00.000Z".to_string(),
        };
        rt.block_on(db.write(move |w| {
            crate::db::chats::ChatsRepository::new(w.main().connection()).create(&create, &opts)
        }))
        .unwrap();

        let post = |content: &str, fact: bool, now_ms: i64| {
            let ledger = Cell::new(SubmissionLedger::default());
            let sink = super::super::chat_events::DiscardSink;
            let mut context = Vec::new();
            let mut bytes = NoBytes;
            crate::test_support::captured_with(|| {
                rt.block_on(record_operator_post::<_, NoQuery, NoProspero>(
                    &db,
                    OperatorPostParams {
                        chat_id: CHAT,
                        user_id: crate::api::SINGLE_USER_ID,
                        project_id: None,
                        chat_settings: None,
                        content,
                        file_ids: &[],
                        attached_files: &[],
                        pending_tool_results: &[],
                        target_participant_ids: None,
                        is_established_fact: fact,
                        poster: Poster {
                            participant: None,
                            participant_id: None,
                        },
                        context: &mut context,
                        ledger: &ledger,
                        sink: &sink,
                        now_ms,
                        rng_bytes: &mut bytes,
                        consult: None,
                        carina: None,
                        status_character: None,
                        turn_control: None,
                    },
                ))
                .unwrap()
            })
        };
        let (fact, fact_lines) = post("The bridge fell.", true, 1_777_000_000_000);
        let (plain, plain_lines) = post("Nothing special.", false, 1_777_000_001_000);
        let fid = fact.user_message_id.unwrap();
        let pid = plain.user_message_id.unwrap();
        let persisted = |lines: &[String], id: &str, flag: bool| {
            let want = format!(
                "[OperatorPost] User message persisted chatId={CHAT} userMessageId={id} isEstablishedFact={flag}"
            );
            assert_eq!(
                lines.iter().filter(|l| l.ends_with(&want)).count(),
                1,
                "{lines:#?}"
            );
        };
        persisted(&fact_lines, &fid, true);
        persisted(&plain_lines, &pid, false);
        let info = format!(
            "INFO quilltap::operator_post [OperatorPost] Operator line posted as established fact chatId={CHAT} userMessageId={fid}"
        );
        assert!(fact_lines.contains(&info), "{fact_lines:#?}");
        assert!(
            !plain_lines
                .iter()
                .any(|l| l.contains("posted as established fact")),
            "silence leg: {plain_lines:#?}"
        );
        let rows = db
            .read_main(|c| crate::db::chats_messages_read::get_messages(c, CHAT))
            .unwrap();
        let row = |id: &str| rows.iter().find(|r| r["id"] == id).unwrap().clone();
        assert_eq!(row(&fid)["isEstablishedFact"], json!(true));
        assert!(
            row(&pid).get("isEstablishedFact").is_none(),
            "absent → NULL"
        );
    }
}
// === /P4.D372 ===
