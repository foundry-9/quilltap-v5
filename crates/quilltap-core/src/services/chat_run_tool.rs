//! The operator-initiated tool run (P4.9E3A) — a differential port of v4
//! `app/api/v1/chats/[id]/actions/run-tool.ts` (`handleRunTool` :41), the Run
//! Tool modal's server half.
//!
//! ## The deny-list
//!
//! `submit_final_response` and `request_full_context` are the agent loop's own
//! machinery and cannot be invoked by a person (`run-tool.ts:19-23`). The
//! rejection is a **400** naming the tool.
//!
//! The arm ORDER is the route's, not the handler's: v4's dispatcher 404s on a
//! missing chat (`handlers/post.ts:115`) before the body is parsed, so the
//! sequence is **404 → Zod 400 → deny-list 400**. The handler's own
//! `badRequest('Chat not found')` at :62 sits behind a SECOND read and is
//! unreachable in practice; one read stands in for both here.
//!
//! ## A private run is a whisper to a non-participant
//!
//! `private: true` sets `targetParticipantIds: [userId]` — the OPERATOR's user
//! id, which is a UUID but not a participant id. That is deliberate (v4's
//! comment): the salon visibility filter and `filterWhisperMessages` therefore
//! exclude it from every character's context, while the "show all whispers"
//! toggle still reveals it to the operator. Reproduced exactly, including that
//! the id written is the USER id and not a participant's.
//!
//! ## The execution context is derived, never trusted
//!
//! `characterId` (when sent) selects an ACTIVE CHARACTER participant with that
//! character; otherwise the first active CHARACTER participant is used. Either
//! way the resulting participant supplies `characterId` /
//! `callingParticipantId` / a fallback `imageProfileId`, and the chat supplies
//! `imageProfileId` / `projectId`. A `characterId` matching nobody yields NO
//! participant — and the tool still runs, with an empty context. That is v4's
//! behavior, not an oversight.
//!
//! ## The seam
//!
//! v4 calls `executeToolCallWithContext` directly; v5's equivalent
//! ([`ToolRunner`]) is assembled by the HOST — it needs the host's
//! `SelfInventoryEnv`, terminal scrollback and consult runner. So the runner
//! arrives through the erased [`OperatorToolRunner`] seam
//! (`EngineAssembly.operator_tool_runner`, the `announcement_preview`
//! precedent). An unassembled runner answers a loud refusal AFTER the deny-list
//! and chat arms, so the modal can render the reason.
//!
//! Pinned by `chat_admin_routes_equivalence`.

use std::future::Future;
use std::pin::Pin;

use serde_json::{json, Map, Value};

use crate::api::types::{ErrorKind, Response};
use crate::db::chats_messages::ChatEventInput;
use crate::db::runtime::Db;
use crate::db::{chats_read, DbError};
use crate::services::chat_admin::{bad_request, internal, ok};
use crate::services::tool_execution::{ToolCall, ToolExecutionContext, ToolResult, ToolRunner};

/// v4's `user.name || user.username` — the name in the Prospero attribution
/// line ("Charles ran rng"). `None` when the user row is absent or carries
/// neither (v4's `undefined`, which the content JSON materializes as `null`).
pub fn operator_display_name(db: &Db, user_id: &str) -> Option<String> {
    let uid = user_id.to_string();
    db.read_main(move |c| {
        Ok::<_, DbError>(
            c.query_row(
                "SELECT name, username FROM users WHERE id = ?1",
                rusqlite::params![uid],
                |r| {
                    Ok((
                        r.get::<_, Option<String>>(0)?,
                        r.get::<_, Option<String>>(1)?,
                    ))
                },
            )
            .ok(),
        )
    })
    .ok()
    .flatten()
    // `||` is JS truthiness: an empty name falls through to the username.
    .and_then(|(name, username)| name.filter(|s| !s.is_empty()).or(username))
}

/// v4 `NON_USER_INVOCABLE_TOOLS` (`run-tool.ts:20-23`) — the agent loop's own
/// machinery, refused when a person asks for it.
pub const NON_USER_INVOCABLE_TOOLS: &[&str] = &["submit_final_response", "request_full_context"];

/// The host seam for one operator-initiated tool execution (v4
/// `executeToolCallWithContext`). Erased so [`EngineAssembly`] can hold it: the
/// production runner is the host's `BuiltInToolRunner`, which carries the
/// `SelfInventoryEnv`, the terminal scrollback source and the consult runner.
///
/// [`EngineAssembly`]: crate::api::engine::EngineAssembly
pub trait OperatorToolRunner: Send + Sync {
    fn run<'a>(
        &'a self,
        call: ToolCall,
        ctx: ToolExecutionContext,
    ) -> Pin<Box<dyn Future<Output = ToolResult> + Send + 'a>>;
}

/// Adapter: any in-process [`ToolRunner`] becomes an [`OperatorToolRunner`].
pub struct ErasedToolRunner<R>(pub R);

impl<R: ToolRunner + Send + Sync> OperatorToolRunner for ErasedToolRunner<R> {
    fn run<'a>(
        &'a self,
        call: ToolCall,
        ctx: ToolExecutionContext,
    ) -> Pin<Box<dyn Future<Output = ToolResult> + Send + 'a>> {
        Box::pin(async move { self.0.run(&call, &ctx).await })
    }
}

/// v4's `requestPrompt` — `name(k: v, k2: v2)`, or the bare name when there are
/// no arguments. String values print raw; everything else is JSON.
fn request_prompt(tool_name: &str, arguments: &Map<String, Value>) -> String {
    if arguments.is_empty() {
        return tool_name.to_string();
    }
    let parts: Vec<String> = arguments
        .iter()
        .map(|(k, v)| match v {
            Value::String(s) => format!("{k}: {s}"),
            other => format!("{k}: {}", serde_json::to_string(other).unwrap_or_default()),
        })
        .collect();
    format!("{tool_name}({})", parts.join(", "))
}

/// v4 `run-tool.ts:67-73` — the active CHARACTER participant the context is
/// derived from.
fn pick_participant<'a>(chat: &'a Value, character_id: Option<&str>) -> Option<&'a Value> {
    let participants = chat.get("participants").and_then(Value::as_array)?;
    participants.iter().find(|p| {
        let is_char = p.get("type").and_then(Value::as_str) == Some("CHARACTER");
        // v4 tests `p.isActive` for truthiness.
        let active = p.get("isActive").and_then(Value::as_bool) == Some(true);
        match character_id {
            Some(want) => {
                is_char && active && p.get("characterId").and_then(Value::as_str) == Some(want)
            }
            None => is_char && active,
        }
    })
}

/// A non-empty string field (`|| undefined` semantics: `""` counts as absent).
fn truthy_str(v: Option<&Value>) -> Option<String> {
    v.and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// v4 `?action=run-tool`. `runner` is the assembled host seam; `None` answers a
/// loud refusal AFTER the deny-list + chat arms. `message_id` / `now_iso` are the
/// two minted values, injected so the differential can pin them.
/// `operator_name` is v4's `user.name || user.username`.
#[allow(clippy::too_many_arguments)]
pub async fn chat_run_tool(
    db: &Db,
    user_id: &str,
    chat_id: &str,
    tool_name: &str,
    arguments: Option<&Value>,
    character_id: Option<&str>,
    private: Option<bool>,
    runner: Option<&dyn OperatorToolRunner>,
    operator_name: Option<String>,
    message_id: String,
    now_iso: String,
) -> Response {
    // v4's ROUTE gate runs first (`handlers/post.ts:115`): a missing chat is a
    // 404 before the body is even parsed. The handler's OWN
    // `badRequest('Chat not found')` (`run-tool.ts:62`) sits behind a second
    // read and is therefore unreachable in practice; the one read below stands
    // in for both.
    let cid = chat_id.to_string();
    let chat = match db.read_main(move |c| chats_read::find_by_id(c, &cid)) {
        Ok(Some(c)) => c,
        Ok(None) => return crate::services::chat_admin::not_found("Chat"),
        Err(e) => return internal(e),
    };

    // `runToolRequestSchema`: toolName min(1), arguments record default {},
    // characterId optional, private default false.
    if tool_name.is_empty() {
        return crate::services::chat_admin::validation_error();
    }
    let arguments: Map<String, Value> = match arguments {
        None | Some(Value::Null) => Map::new(),
        Some(Value::Object(o)) => o.clone(),
        // `z.record(z.string(), z.unknown())` rejects a non-object.
        Some(_) => return crate::services::chat_admin::validation_error(),
    };
    let private = private.unwrap_or(false);

    // The deny-list is checked BEFORE the chat is loaded.
    if NON_USER_INVOCABLE_TOOLS.contains(&tool_name) {
        return bad_request(format!("Tool '{tool_name}' cannot be invoked directly"));
    }

    let Some(runner) = runner else {
        return Response::error(
            ErrorKind::Internal,
            "run-tool is not available in this build: no operator tool runner is assembled",
        );
    };

    let participant = pick_participant(&chat, character_id);
    let ctx = ToolExecutionContext {
        chat_id: chat_id.to_string(),
        user_id: user_id.to_string(),
        // v4 `chat.imageProfileId || characterParticipant?.imageProfileId || undefined`.
        image_profile_id: truthy_str(chat.get("imageProfileId"))
            .or_else(|| participant.and_then(|p| truthy_str(p.get("imageProfileId")))),
        character_id: participant.and_then(|p| truthy_str(p.get("characterId"))),
        calling_participant_id: participant.and_then(|p| truthy_str(p.get("id"))),
        project_id: truthy_str(chat.get("projectId")),
        ..Default::default()
    };

    let result = runner
        .run(
            ToolCall {
                name: tool_name.to_string(),
                arguments: Value::Object(arguments.clone()),
                call_id: None,
            },
            ctx,
        )
        .await;

    let prompt = request_prompt(tool_name, &arguments);

    // The persisted TOOL bubble, authored by Prospero. `content` is a JSON
    // string; its key order is part of the wire.
    let mut content = Map::new();
    content.insert("tool".into(), json!(tool_name));
    content.insert("toolName".into(), json!(tool_name));
    content.insert("initiatedBy".into(), json!("user"));
    content.insert(
        "operatorName".into(),
        operator_name.map(Value::String).unwrap_or(Value::Null),
    );
    content.insert("private".into(), json!(private));
    content.insert("success".into(), json!(result.success));
    // v4 keeps the STRUCTURED result (its comment: ToolMessage renders rich
    // fields, so do not flatten objects into strings here).
    content.insert("result".into(), result.result.clone());
    // `error: result.error || undefined` — an empty or absent error drops the key.
    if let Some(err) = result.error.as_deref().filter(|e| !e.is_empty()) {
        content.insert("error".into(), json!(err));
    }
    content.insert("prompt".into(), json!(prompt));
    content.insert("arguments".into(), Value::Object(arguments));

    let targets = if private {
        Value::Array(vec![Value::String(user_id.to_string())])
    } else {
        Value::Null
    };
    let message = json!({
        "type": "message",
        "id": message_id,
        "role": "TOOL",
        "systemSender": "prospero",
        "systemKind": "tool-run",
        "targetParticipantIds": targets,
        "content": serde_json::to_string(&Value::Object(content)).unwrap_or_default(),
        "createdAt": now_iso,
        "attachments": [],
    });

    let event: ChatEventInput = match serde_json::from_value(message.clone()) {
        Ok(e) => e,
        Err(e) => return internal(DbError::Internal(format!("run-tool message marshal: {e}"))),
    };
    let cid = chat_id.to_string();
    if let Err(e) = db
        .write(move |w| w.main().chat_messages().add_message(&cid, &event))
        .await
    {
        return internal(e);
    }

    let mut out = Map::new();
    out.insert("toolName".into(), json!(result.tool_name));
    out.insert("success".into(), json!(result.success));
    out.insert("result".into(), result.result.clone());
    // v4 spells `error: result.error` here — present but `undefined` when absent,
    // which `NextResponse.json` drops.
    if let Some(err) = result.error.as_deref() {
        out.insert("error".into(), json!(err));
    }

    ok(json!({
        "success": true,
        "message": message,
        "result": Value::Object(out),
    }))
}
