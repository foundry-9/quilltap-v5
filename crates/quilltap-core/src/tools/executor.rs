//! The dispatching tool runner — port of v4 `executeToolCallWithContext`'s
//! built-in tool if-chain (`lib/chat/tool-executor.ts:275+`).
//!
//! [`BuiltInToolRunner`] is the real [`ToolRunner`](crate::services::tool_execution::ToolRunner)
//! implementation that `process_tool_calls` dispatches through. It routes a
//! [`ToolCall`] by name to the ported W4.1d-batch-1 handlers (+ the already-ported
//! `rng` handler), reproducing v4's per-tool dispatch row: call the handler's
//! `execute_*`, `format_*`, and build the exact [`ToolResult`] shape v4 returns
//! (the `{ formattedText, … }` object, the failure `null`/`error` mapping).
//!
//! ## The injected fallback (batches 2–5 extend the map without touching callers)
//!
//! Names this batch has not ported yet route to an **injected inner
//! [`ToolRunner`] fallback**. Its production default, [`LoudFallbackRunner`], is
//! loud — never a silent wrong answer:
//!   - a name v4 itself doesn't know (not in [`BUILT_IN_TOOLS`], no plugin) →
//!     v4's exact `Unknown tool: <name>` failure result;
//!   - a name that IS a v4 built-in but is not yet ported (image gen, search,
//!     wardrobe, doc-edit, state, ask_carina, …) → a loud "recognized but not yet
//!     available in this build" failure naming the tool (a documented deferral;
//!     batches 2–5 move it into the ported set).
//!
//! The differential injects a [`CannedToolRunner`](crate::services::tool_execution::CannedToolRunner)
//! as the fallback instead, so unported names replay canned results.
//!
//! ## Deferrals (documented boundaries)
//!
//! - **Plugin-vs-built-in routing.** v4 first checks `BUILT_IN_TOOLS.has(name)`;
//!   a NON-built-in name is routed to the plugin registry
//!   (`toolRegistry.hasPlugin` / multi-tool plugins) BEFORE the unknown-tool
//!   fallthrough. The plugin registry is unported, so this runner reproduces v4's
//!   **no-plugin world**: built-in names dispatch here; every other name reaches
//!   the fallback (→ `Unknown tool: <name>` by default). Mirroring the real
//!   plugin precedence waits for the plugin subsystem (later wave). This is the
//!   only divergence from v4's dispatch, and only for installed-plugin setups.
//! - **`terminal_read` scrollback source.** v4's handler reads the raw scrollback
//!   from the live PTY ring buffer or by tailing the transcript file — both
//!   host-side I/O. The port lifted that to an injected `full_content` (see
//!   [`crate::tools::terminal`]); here it is the [`TerminalScrollbackSource`]
//!   seam (default [`EmptyScrollback`], the "no live session / no transcript"
//!   case). Production wires ptyManager / transcript tailing (host-side).
//! - **`rng` byte source.** The tool-loop `rng` path uses OS randomness here
//!   ([`OsRandomBytes`](crate::tools::rng::OsRandomBytes)); an injectable byte
//!   source for the response-path RNG lands with the tool loops (W4.1e). The
//!   deterministic auto-detect RNG path (W4.1a) already injects its bytes.
//! - **Guard clauses** that v4 keeps in the dispatcher (not the handler) are
//!   reproduced here: `self_inventory` requires a character; `whisper` requires a
//!   calling participant; the annotation tools resolve the calling character's
//!   name from the chat participants.

use std::sync::Arc;

use serde_json::{json, Map, Value};

use super::{annotations, help, read_conversation, rng, self_inventory, terminal, whisper};
use crate::db::runtime::Db;
use crate::db::{characters_read, chats_read};
use crate::services::tool_execution::{ToolCall, ToolExecutionContext, ToolResult, ToolRunner};
use crate::tools::self_inventory::SelfInventoryEnv;

// ===========================================================================
// v4's BUILT_IN_TOOLS set (tool-executor.ts:235) + the names ported this batch
// ===========================================================================

/// Every built-in tool name v4 knows (`BUILT_IN_TOOLS`, tool-executor.ts:235,
/// including the `DOC_EDIT_TOOL_NAMES` members). Used by [`LoudFallbackRunner`]
/// to tell an unported-but-recognized tool from a genuinely unknown one.
pub const BUILT_IN_TOOLS: &[&str] = &[
    "generate_image",
    "search_web",
    "project_info",
    "request_full_context",
    "help_search",
    "help_settings",
    "help_navigate",
    "rng",
    "state",
    "self_inventory",
    "submit_final_response",
    "whisper",
    "read_conversation",
    "upsert_annotation",
    "delete_annotation",
    "search",
    "run_sql",
    "wardrobe_list",
    "wardrobe_read",
    "wardrobe_create",
    "wardrobe_update",
    "wardrobe_archive",
    "wardrobe_wear",
    "wardrobe_take_off",
    // DOC_EDIT_TOOL_NAMES (doc-edit-handler.ts:96)
    "doc_read_file",
    "doc_write_file",
    "doc_str_replace",
    "doc_insert_text",
    "doc_grep",
    "doc_list_files",
    "doc_read_frontmatter",
    "doc_update_frontmatter",
    "doc_read_heading",
    "doc_update_heading",
    "doc_move_file",
    "doc_copy_file",
    "doc_delete_file",
    "doc_create_folder",
    "doc_delete_folder",
    "doc_move_folder",
    "doc_open_document",
    "doc_close_document",
    "doc_focus",
    "doc_write_blob",
    "doc_read_blob",
    "doc_list_blobs",
    "doc_delete_blob",
    "keep_image",
    "list_images",
    "attach_image",
    // Terminal tools
    "terminal_read",
    "terminal_list",
    // Carina
    "ask_carina",
    // Post Office
    "send_mail",
    "list_email",
];

/// The built-in tools [`BuiltInToolRunner`] dispatches to a real handler as of
/// W4.1d batch 1 (this batch's nine + the already-ported `rng`). Everything else
/// in [`BUILT_IN_TOOLS`] routes to the fallback until its batch lands.
pub const PORTED_TOOLS: &[&str] = &[
    "rng",
    "read_conversation",
    "upsert_annotation",
    "delete_annotation",
    "terminal_read",
    "terminal_list",
    "whisper",
    "help_settings",
    "help_navigate",
    "submit_final_response",
    "self_inventory",
];

// ===========================================================================
// The terminal-scrollback seam
// ===========================================================================

/// Resolve the raw scrollback bytes for a terminal session (v4's live-PTY /
/// transcript-tail read, host-side). Injected into [`BuiltInToolRunner`] so the
/// core stays free of PTY/fs I/O.
pub trait TerminalScrollbackSource: Send + Sync {
    /// The raw scrollback for `session_id` (empty when no live buffer / no
    /// transcript is available — v4's `fullContent = ''` default).
    fn scrollback(&self, session_id: &str) -> String;
}

/// The default source: always empty (no live session, no transcript). Faithful to
/// v4 when neither a live PTY nor a transcript file exists.
#[derive(Debug, Clone, Copy, Default)]
pub struct EmptyScrollback;

impl TerminalScrollbackSource for EmptyScrollback {
    fn scrollback(&self, _session_id: &str) -> String {
        String::new()
    }
}

// ===========================================================================
// The loud production fallback
// ===========================================================================

/// The default fallback for names [`BuiltInToolRunner`] does not dispatch itself.
/// Loud — never a silent wrong answer. Reproduces v4's `Unknown tool: <name>` for
/// names v4 doesn't know; for a recognized-but-unported built-in it returns a
/// clear "not yet available in this build" failure naming the tool.
#[derive(Debug, Clone, Copy, Default)]
pub struct LoudFallbackRunner;

impl ToolRunner for LoudFallbackRunner {
    // The trait fixes `run`'s signature to `-> impl Future + Send`, so the body is
    // a returned `async move` block, not an `async fn`.
    #[allow(clippy::manual_async_fn)]
    fn run(
        &self,
        tool_call: &ToolCall,
        _ctx: &ToolExecutionContext,
    ) -> impl std::future::Future<Output = ToolResult> + Send {
        let name = tool_call.name.clone();
        async move {
            let error = if BUILT_IN_TOOLS.contains(&name.as_str()) {
                // Recognized by v4 but not yet ported into v5 (batches 2–5).
                format!("Tool '{name}' is recognized but not yet available in this build")
            } else {
                // v4's exact unknown-tool result (the no-plugin branch).
                format!("Unknown tool: {name}")
            };
            ToolResult {
                tool_name: name,
                success: false,
                result: Value::Null,
                error: Some(error),
                message: None,
                metadata: None,
            }
        }
    }
}

// ===========================================================================
// BuiltInToolRunner
// ===========================================================================

/// The real dispatching [`ToolRunner`] (v4 `executeToolCallWithContext`'s
/// built-in if-chain). Holds the [`Db`] handle, the [`SelfInventoryEnv`] the
/// `self_inventory` tool's `quilltap` section needs, the terminal-scrollback
/// seam, and the injected fallback for unported names.
pub struct BuiltInToolRunner<F: ToolRunner = LoudFallbackRunner> {
    db: Db,
    self_inventory_env: SelfInventoryEnv,
    scrollback: Arc<dyn TerminalScrollbackSource>,
    fallback: F,
}

impl BuiltInToolRunner<LoudFallbackRunner> {
    /// Build a runner with the loud production fallback + the empty scrollback
    /// source.
    pub fn new(db: Db, self_inventory_env: SelfInventoryEnv) -> Self {
        BuiltInToolRunner {
            db,
            self_inventory_env,
            scrollback: Arc::new(EmptyScrollback),
            fallback: LoudFallbackRunner,
        }
    }
}

impl<F: ToolRunner> BuiltInToolRunner<F> {
    /// Replace the fallback runner (unported names route here). Used by the
    /// differential to inject a `CannedToolRunner`.
    pub fn with_fallback<G: ToolRunner>(self, fallback: G) -> BuiltInToolRunner<G> {
        BuiltInToolRunner {
            db: self.db,
            self_inventory_env: self.self_inventory_env,
            scrollback: self.scrollback,
            fallback,
        }
    }

    /// Replace the terminal-scrollback source (production wires PTY/transcript
    /// reads here).
    pub fn with_scrollback(mut self, scrollback: Arc<dyn TerminalScrollbackSource>) -> Self {
        self.scrollback = scrollback;
        self
    }

    /// Whether this runner dispatches `name` to a real handler (vs. the fallback).
    fn handles(name: &str) -> bool {
        PORTED_TOOLS.contains(&name)
    }

    /// Dispatch a ported tool. `None` ⇒ not handled here (route to the fallback).
    async fn dispatch(&self, tc: &ToolCall, ctx: &ToolExecutionContext) -> Option<ToolResult> {
        if !Self::handles(&tc.name) {
            return None;
        }
        Some(match tc.name.as_str() {
            "rng" => self.run_rng(tc, ctx).await,
            "read_conversation" => self.run_read_conversation(tc, ctx).await,
            "upsert_annotation" => self.run_upsert_annotation(tc, ctx).await,
            "delete_annotation" => self.run_delete_annotation(tc, ctx).await,
            "terminal_read" => self.run_terminal_read(tc, ctx).await,
            "terminal_list" => self.run_terminal_list(tc, ctx).await,
            "whisper" => self.run_whisper(tc, ctx).await,
            "help_settings" => self.run_help_settings(tc, ctx).await,
            "help_navigate" => self.run_help_navigate(tc, ctx).await,
            "submit_final_response" => self.run_submit_final_response(tc, ctx).await,
            "self_inventory" => self.run_self_inventory(tc, ctx).await,
            // `handles` guards the set, so this is unreachable.
            other => fail(other, format!("Unknown tool: {other}")),
        })
    }

    // -- rng (already ported handler; tool-executor.ts:582) -----------------
    async fn run_rng(&self, tc: &ToolCall, ctx: &ToolExecutionContext) -> ToolResult {
        let context = rng::RngToolContext {
            user_id: ctx.user_id.clone(),
            chat_id: ctx.chat_id.clone(),
        };
        let mut source = rng::OsRandomBytes;
        match rng::execute_rng_tool(&self.db, &tc.arguments, &context, &mut source) {
            Ok(out) => {
                let formatted = rng::format_rng_results(&out);
                if out.success {
                    ok(
                        "rng",
                        json!({
                            "formattedText": formatted,
                            "type": out.type_,
                            "rollCount": out.roll_count,
                            "results": out.results,
                            "sum": out.sum,
                        }),
                    )
                } else {
                    fail("rng", out.error.unwrap_or_default())
                }
            }
            // v4's outer try/catch maps a thrown DB error to a failure result.
            Err(e) => fail("rng", e.to_string()),
        }
    }

    // -- read_conversation (tool-executor.ts:836) ---------------------------
    async fn run_read_conversation(&self, tc: &ToolCall, ctx: &ToolExecutionContext) -> ToolResult {
        let out = read_conversation::execute_read_conversation(
            &self.db,
            &ctx.user_id,
            &ctx.chat_id,
            ctx.character_id.as_deref(),
            &tc.arguments,
        )
        .await;
        let formatted = read_conversation::format_read_conversation(&out);
        if out.success {
            ok(
                "read_conversation",
                json!({
                    "formattedText": formatted,
                    "messageCount": out.message_count,
                    "interchangeCount": out.interchange_count,
                }),
            )
        } else {
            fail("read_conversation", out.error.unwrap_or_default())
        }
    }

    // -- upsert_annotation (tool-executor.ts:859) ---------------------------
    async fn run_upsert_annotation(&self, tc: &ToolCall, ctx: &ToolExecutionContext) -> ToolResult {
        let character_name = match self.resolve_calling_character_name(ctx) {
            Ok(n) => n,
            Err(e) => return fail("upsert_annotation", e),
        };
        let out = annotations::execute_upsert_annotation(
            &self.db,
            &ctx.user_id,
            &ctx.chat_id,
            &character_name,
            &tc.arguments,
        )
        .await;
        let formatted = annotations::format_upsert_annotation(&out);
        if out.success {
            ok(
                "upsert_annotation",
                json!({
                    "formattedText": formatted,
                    "message_index": out.message_index,
                    "character_name": out.character_name,
                    "action": out.action,
                }),
            )
        } else {
            fail("upsert_annotation", out.error.unwrap_or_default())
        }
    }

    // -- delete_annotation (tool-executor.ts:899) ---------------------------
    async fn run_delete_annotation(&self, tc: &ToolCall, ctx: &ToolExecutionContext) -> ToolResult {
        let character_name = match self.resolve_calling_character_name(ctx) {
            Ok(n) => n,
            Err(e) => return fail("delete_annotation", e),
        };
        let out = annotations::execute_delete_annotation(
            &self.db,
            &ctx.user_id,
            &ctx.chat_id,
            &character_name,
            &tc.arguments,
        )
        .await;
        let formatted = annotations::format_delete_annotation(&out);
        if out.success {
            ok(
                "delete_annotation",
                json!({
                    "formattedText": formatted,
                    "message_index": out.message_index,
                    "character_name": out.character_name,
                }),
            )
        } else {
            fail("delete_annotation", out.error.unwrap_or_default())
        }
    }

    // -- terminal_read (tool-executor.ts:1147) ------------------------------
    async fn run_terminal_read(&self, tc: &ToolCall, ctx: &ToolExecutionContext) -> ToolResult {
        // Validate first (v4's `validateTerminalReadInput` gate in the dispatcher).
        let Some(input) = terminal::validate_terminal_read_input(&tc.arguments) else {
            return fail(
                "terminal_read",
                "Invalid arguments for terminal_read: sessionId is required",
            );
        };
        let full_content = self.scrollback.scrollback(&input.session_id);
        match terminal::execute_terminal_read(&self.db, &ctx.chat_id, &input, &full_content).await {
            Ok(out) => {
                let formatted = terminal::format_terminal_read(&out);
                let mut result = Map::new();
                result.insert("formattedText".into(), json!(formatted));
                result.insert("sessionId".into(), json!(out.session_id));
                result.insert("shell".into(), json!(out.shell));
                result.insert("cwd".into(), json!(out.cwd));
                result.insert("status".into(), json!(out.status));
                result.insert("exitCode".into(), json!(out.exit_code));
                result.insert("lines".into(), json!(out.lines));
                result.insert("totalLines".into(), json!(out.total_lines));
                result.insert("startLine".into(), json!(out.start_line));
                result.insert("endLine".into(), json!(out.end_line));
                result.insert("truncated".into(), json!(out.truncated));
                result.insert("scrollback".into(), json!(out.scrollback));
                if let Some(raw) = &out.raw_scrollback {
                    result.insert("rawScrollback".into(), json!(raw));
                }
                ok("terminal_read", Value::Object(result))
            }
            // v4 catches the throw → failure result with err.message.
            Err(e) => fail("terminal_read", e.to_string()),
        }
    }

    // -- terminal_list (tool-executor.ts:1202) ------------------------------
    async fn run_terminal_list(&self, tc: &ToolCall, ctx: &ToolExecutionContext) -> ToolResult {
        if !terminal::validate_terminal_list_input(&tc.arguments) {
            return fail("terminal_list", "Invalid arguments for terminal_list");
        }
        match terminal::execute_terminal_list(&self.db, &ctx.chat_id).await {
            Ok(out) => {
                let formatted = terminal::format_terminal_list(&out);
                ok(
                    "terminal_list",
                    json!({ "formattedText": formatted, "sessions": out.sessions }),
                )
            }
            Err(e) => fail("terminal_list", e.to_string()),
        }
    }

    // -- whisper (tool-executor.ts:1028) ------------------------------------
    async fn run_whisper(&self, tc: &ToolCall, ctx: &ToolExecutionContext) -> ToolResult {
        // v4's dispatcher guard: whisper requires a calling participant.
        let Some(calling) = ctx.calling_participant_id.as_deref() else {
            return fail("whisper", "Whisper requires a multi-character chat context");
        };
        let out =
            whisper::execute_whisper(&self.db, &ctx.user_id, &ctx.chat_id, calling, &tc.arguments)
                .await;
        let formatted = whisper::format_whisper(&out);
        if out.success {
            ok(
                "whisper",
                json!({
                    "formattedText": formatted,
                    "targetName": out.target_name,
                    "targetParticipantId": out.target_participant_id,
                }),
            )
        } else {
            fail("whisper", out.error.unwrap_or_default())
        }
    }

    // -- help_settings (tool-executor.ts:490) -------------------------------
    async fn run_help_settings(&self, tc: &ToolCall, ctx: &ToolExecutionContext) -> ToolResult {
        let out = help::execute_help_settings(&self.db, &ctx.user_id, &tc.arguments).await;
        let formatted = help::format_help_settings(&out);
        if out.success {
            ok(
                "help_settings",
                json!({
                    "formattedText": formatted,
                    "category": out.category,
                    "data": out.data,
                }),
            )
        } else {
            fail("help_settings", out.error.unwrap_or_default())
        }
    }

    // -- help_navigate (tool-executor.ts:518) -------------------------------
    async fn run_help_navigate(&self, tc: &ToolCall, ctx: &ToolExecutionContext) -> ToolResult {
        let out = help::execute_help_navigate(&ctx.user_id, &tc.arguments);
        let formatted = help::format_help_navigate(&out);
        if out.success {
            ok(
                "help_navigate",
                json!({ "formattedText": formatted, "navigationUrl": out.url }),
            )
        } else {
            fail("help_navigate", out.error.unwrap_or_default())
        }
    }

    // -- submit_final_response (tool-executor.ts:1003) ----------------------
    async fn run_submit_final_response(
        &self,
        tc: &ToolCall,
        ctx: &ToolExecutionContext,
    ) -> ToolResult {
        let out = help::execute_submit_final_response(&ctx.chat_id, &tc.arguments);
        let formatted = help::format_submit_final_response(&out);
        if out.success {
            ok(
                "submit_final_response",
                json!({
                    "formattedText": formatted,
                    "finalResponse": out.final_response,
                    "summary": out.summary,
                    // JS number: an integer-valued confidence renders bare.
                    "confidence": out.confidence.map(crate::db::js_number_to_json),
                }),
            )
        } else {
            // v4 maps the error from `result.message`, not `result.error`.
            fail("submit_final_response", out.message)
        }
    }

    // -- self_inventory (tool-executor.ts:625) ------------------------------
    async fn run_self_inventory(&self, tc: &ToolCall, ctx: &ToolExecutionContext) -> ToolResult {
        // v4's dispatcher guard: self_inventory requires a character context.
        let Some(character_id) = ctx.character_id.as_deref() else {
            return fail(
                "self_inventory",
                "self_inventory requires a character context",
            );
        };
        let out = self_inventory::execute_self_inventory(
            &self.db,
            &self.self_inventory_env,
            &ctx.user_id,
            &ctx.chat_id,
            Some(character_id),
            ctx.project_id.as_deref(),
            ctx.calling_participant_id.as_deref(),
            ctx.loaded_memories.as_ref(),
            &tc.arguments,
        )
        .await;
        let formatted = self_inventory::format_self_inventory(&out);
        if !out.success {
            return fail("self_inventory", out.error.clone().unwrap_or_default());
        }
        // Build v4's `structuredResult`: formattedText + the three id fields +
        // each present section, in fixed order — carina is DELIBERATELY omitted
        // (v4's asymmetry: carina appears only inside formattedText).
        let ov = serde_json::to_value(&out).unwrap_or(Value::Null);
        let mut result = Map::new();
        result.insert("formattedText".into(), json!(formatted));
        for key in ["quilltapVersion", "characterId", "characterName"] {
            result.insert(key.into(), ov.get(key).cloned().unwrap_or(Value::Null));
        }
        for key in [
            "vault",
            "vaultAccess",
            "memory",
            "loadedMemories",
            "chats",
            "prompt",
            "lastTurn",
            "quilltap",
            "context",
        ] {
            if let Some(v) = ov.get(key) {
                result.insert(key.into(), v.clone());
            }
        }
        ok("self_inventory", Value::Object(result))
    }

    // -- shared: resolve the calling character's name (tool-executor.ts:861) --
    /// v4's per-dispatch name resolution: from `callingParticipantId`, find the
    /// chat participant → its `characterId` → the character's `name`. Defaults to
    /// `"Unknown"` when anything is missing (exactly v4's fallback). A DB error
    /// surfaces to the caller (v4's outer try/catch → failure result).
    fn resolve_calling_character_name(&self, ctx: &ToolExecutionContext) -> Result<String, String> {
        let Some(calling) = ctx.calling_participant_id.as_deref() else {
            return Ok("Unknown".to_string());
        };
        let chat = self
            .db
            .read_main(|conn| chats_read::find_by_id(conn, &ctx.chat_id))
            .map_err(|e| e.to_string())?;
        let Some(chat) = chat else {
            return Ok("Unknown".to_string());
        };
        let participant_char_id = chat
            .get("participants")
            .and_then(Value::as_array)
            .and_then(|ps| {
                ps.iter()
                    .find(|p| p.get("id").and_then(Value::as_str) == Some(calling))
            })
            .and_then(|p| p.get("characterId"))
            .and_then(Value::as_str)
            .map(str::to_string);
        let Some(cid) = participant_char_id else {
            return Ok("Unknown".to_string());
        };
        let character = self
            .db
            .read_main(|main| {
                self.db
                    .read_mount_index(|mount| characters_read::find_by_id(main, mount, &cid))
            })
            .map_err(|e| e.to_string())?;
        Ok(character
            .as_ref()
            .and_then(|c| c.get("name"))
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| "Unknown".to_string()))
    }
}

impl<F: ToolRunner + Sync> ToolRunner for BuiltInToolRunner<F> {
    // Genuinely awaits `dispatch` inside, so the returned future is an `async
    // move` block (the trait forbids an `async fn` signature).
    #[allow(clippy::manual_async_fn)]
    fn run(
        &self,
        tool_call: &ToolCall,
        ctx: &ToolExecutionContext,
    ) -> impl std::future::Future<Output = ToolResult> + Send {
        async move {
            match self.dispatch(tool_call, ctx).await {
                Some(result) => result,
                // Not handled here → the injected fallback (loud by default).
                None => self.fallback.run(tool_call, ctx).await,
            }
        }
    }
}

// ===========================================================================
// ToolResult builders (v4's dispatch-row shape)
// ===========================================================================

/// A success [`ToolResult`] (`result` present, `error`/`message` absent).
fn ok(tool_name: &str, result: Value) -> ToolResult {
    ToolResult {
        tool_name: tool_name.to_string(),
        success: true,
        result,
        error: None,
        message: None,
        metadata: None,
    }
}

/// A failure [`ToolResult`] (`result: null`, `error` set) — v4's `result:
/// result.success ? {…} : null, error: result.success ? undefined : result.error`.
fn fail(tool_name: &str, error: impl Into<String>) -> ToolResult {
    ToolResult {
        tool_name: tool_name.to_string(),
        success: false,
        result: Value::Null,
        error: Some(error.into()),
        message: None,
        metadata: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::tool_execution::create_tool_context;

    fn ctx() -> ToolExecutionContext {
        create_tool_context(
            "chat-1", "user-1", "char-1", "pp-1", None, None, None, None, None,
        )
    }

    #[test]
    fn ported_set_is_a_subset_of_built_in_tools() {
        for name in PORTED_TOOLS {
            assert!(
                BUILT_IN_TOOLS.contains(name),
                "ported tool `{name}` must be a v4 built-in"
            );
        }
    }

    #[tokio::test]
    async fn loud_fallback_unknown_vs_unported() {
        let runner = LoudFallbackRunner;
        // A genuinely unknown name → v4's exact `Unknown tool: <name>`.
        let unknown = ToolCall {
            name: "totally_made_up".into(),
            arguments: json!({}),
            call_id: None,
        };
        let r = runner.run(&unknown, &ctx()).await;
        assert!(!r.success);
        assert_eq!(r.error.as_deref(), Some("Unknown tool: totally_made_up"));
        assert_eq!(r.result, Value::Null);

        // A recognized-but-unported built-in → loud "not yet available".
        let unported = ToolCall {
            name: "generate_image".into(),
            arguments: json!({}),
            call_id: None,
        };
        let r = runner.run(&unported, &ctx()).await;
        assert!(!r.success);
        assert_eq!(
            r.error.as_deref(),
            Some("Tool 'generate_image' is recognized but not yet available in this build")
        );
    }
}
