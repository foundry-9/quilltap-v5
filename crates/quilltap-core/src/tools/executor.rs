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

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use serde::Serialize;
use serde_json::{json, Map, Value};

use super::doc_edit::{execute_doc_edit_tool, format_doc_edit_results, DocEditToolContext};
use super::{
    annotations, doc_edit, help, help_search, list_email, photo, project_info, read_conversation,
    request_full_context, rng, run_sql, search, self_inventory, send_mail, state, terminal,
    wardrobe_archive, wardrobe_create, wardrobe_list, wardrobe_read, wardrobe_take_off,
    wardrobe_update, wardrobe_wear, web_search, whisper,
};
use crate::db::runtime::Db;
use crate::db::{characters_read, chats_read};
use crate::model::embedding::{EmbeddingPriority, EmbeddingProvider, ErasedEmbeddingProvider};
use crate::photos::save_image_to_album::{
    FileBytesStore, NoSideEffects, NotConfiguredBytes, SaveImageSideEffects,
};
use crate::services::tool_execution::{ToolCall, ToolExecutionContext, ToolResult, ToolRunner};
use crate::tools::doc_edit::shared::collect_peer_character_ids_for_reads;
use crate::tools::photo::PhotoToolContext;
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
    "wardrobe_list",
    "wardrobe_read",
    "wardrobe_create",
    "wardrobe_update",
    "wardrobe_archive",
    "wardrobe_wear",
    "wardrobe_take_off",
    // W4.1d3b: doc-edit tools (the 23 non-photo `doc_*` tools, dispatched through
    // `execute_doc_edit_tool`). The photo trio (`keep_image`/`list_images`/
    // `attach_image`) is a tracked scoped deferral → the loud fallback.
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
    // W4.1d4: search tools
    "search",
    "project_info",
    "help_search",
    "request_full_context",
    // W4.1d5: host-seamed + remaining tools (state + run_sql + the Post Office).
    // `ask_carina` is ported (`tools::ask_carina`) + differential-verified, but its
    // dispatch needs the W4.5 Carina query engine injected as the `RunCarinaQuery`
    // seam (orchestrator-provided, frozen this round), so it stays on the loud
    // fallback until that seam is wired — the handler + differential are complete.
    "state",
    "run_sql",
    "send_mail",
    "list_email",
    // `search_web` dispatches through the injected `WebSearchProvider` seam
    // (default: NotConfigured — faithful to a no-search-plugin instance; a real
    // provider is host-wired). The handler + differential are complete.
    "search_web",
    // W4.9b: the photo trio. `keep_image` composes the ported
    // `save_image_to_album` behind the injected `FileBytesStore` bytes seam;
    // `list_images` the vault `document_search`; `attach_image` the self-vault link
    // read. Each dispatches inside a both-connections `Db::write` closure. The
    // embedding-enqueue after a keep is a **recorded seam** (mount invalidation +
    // embedding scheduler host-side).
    "keep_image",
    "list_images",
    "attach_image",
];

/// The three photo tools live in `DOC_EDIT_TOOL_NAMES` (v4 groups them under the
/// doc-edit family) but dispatch to the [`photo`] handlers, NOT `run_doc_edit`.
const PHOTO_TOOLS: &[&str] = &["keep_image", "list_images", "attach_image"];

/// Whether `name` is a doc-edit tool this runner dispatches through the shared
/// `execute_doc_edit_tool` (every `doc_*` name except the photo trio, which has its
/// own handler).
fn is_ported_doc_edit_tool(name: &str) -> bool {
    doc_edit::DOC_EDIT_TOOL_NAMES.contains(&name) && !PHOTO_TOOLS.contains(&name)
}

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
    /// The embedding provider the `search` / `help_search` tools use (v4's
    /// `generateEmbeddingForUser`). Defaults to the always-failing
    /// [`ErasedEmbeddingProvider::none`] until a real provider is wired (W4.1g);
    /// with no provider the search branches degrade gracefully (v4's "no embedding
    /// profile" behavior) and `help_search` falls back to keyword search.
    embedding_provider: ErasedEmbeddingProvider,
    /// The web-search boundary the `search_web` tool uses (v4's
    /// `searchProviderRegistry` + API-key lookup + Serper fallback). Defaults to
    /// [`web_search::NotConfiguredWebSearch`] (a no-search-plugin instance returns
    /// v4's "not configured" error); production wires a real provider host-side.
    web_search_provider: Arc<dyn web_search::WebSearchProvider>,
    /// The image-bytes boundary the `keep_image` tool uses (v4's
    /// `fileStorageManager.downloadFile` + `ingestImageBuffer`). Defaults to
    /// [`NotConfiguredBytes`] (no host byte source → `keep_image` reports
    /// `EMPTY_BYTES`); production wires a real store host-side, the differential a
    /// canned in-memory one.
    file_bytes: Arc<dyn FileBytesStore>,
    /// The recorded save side-effects (mount invalidation + embedding enqueue).
    /// Defaults to [`NoSideEffects`]; production wires the mount-chunk cache + the
    /// embedding scheduler host-side (the embedding-enqueue via `queue_service`
    /// EMBEDDING_GENERATE is a **tracked deferral** — a recorded seam this round).
    photo_side_effects: Arc<dyn SaveImageSideEffects + Send + Sync>,
    fallback: F,
}

impl BuiltInToolRunner<LoudFallbackRunner> {
    /// Build a runner with the loud production fallback + the empty scrollback
    /// source + no embedding provider + the not-configured web-search boundary.
    pub fn new(db: Db, self_inventory_env: SelfInventoryEnv) -> Self {
        BuiltInToolRunner {
            db,
            self_inventory_env,
            scrollback: Arc::new(EmptyScrollback),
            embedding_provider: ErasedEmbeddingProvider::none(),
            web_search_provider: Arc::new(web_search::NotConfiguredWebSearch),
            file_bytes: Arc::new(NotConfiguredBytes),
            photo_side_effects: Arc::new(NoSideEffects),
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
            embedding_provider: self.embedding_provider,
            web_search_provider: self.web_search_provider,
            file_bytes: self.file_bytes,
            photo_side_effects: self.photo_side_effects,
            fallback,
        }
    }

    /// Inject the image-bytes store the `keep_image` tool uses (production wires a
    /// real host store; the core default reports missing bytes).
    pub fn with_file_bytes(mut self, store: Arc<dyn FileBytesStore>) -> Self {
        self.file_bytes = store;
        self
    }

    /// Inject the recorded photo save side-effects (mount invalidation + embedding
    /// enqueue). Production wires the host cache/scheduler.
    pub fn with_photo_side_effects(
        mut self,
        side_effects: Arc<dyn SaveImageSideEffects + Send + Sync>,
    ) -> Self {
        self.photo_side_effects = side_effects;
        self
    }

    /// Inject the embedding provider the `search` / `help_search` tools use
    /// (production wires the real one; the core default never succeeds).
    pub fn with_embedding_provider(mut self, provider: ErasedEmbeddingProvider) -> Self {
        self.embedding_provider = provider;
        self
    }

    /// Inject the web-search boundary the `search_web` tool uses (production wires
    /// the real plugin/Serper provider host-side).
    pub fn with_web_search_provider(
        mut self,
        provider: Arc<dyn web_search::WebSearchProvider>,
    ) -> Self {
        self.web_search_provider = provider;
        self
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
            "wardrobe_list" => self.run_wardrobe_list(tc, ctx).await,
            "wardrobe_read" => self.run_wardrobe_read(tc, ctx).await,
            "wardrobe_create" => self.run_wardrobe_create(tc, ctx).await,
            "wardrobe_update" => self.run_wardrobe_update(tc, ctx).await,
            "wardrobe_archive" => self.run_wardrobe_archive(tc, ctx).await,
            "wardrobe_wear" => self.run_wardrobe_wear(tc, ctx).await,
            "wardrobe_take_off" => self.run_wardrobe_take_off(tc, ctx).await,
            // W4.1d4: search tools
            "search" => self.run_search(tc, ctx).await,
            "project_info" => self.run_project_info(tc, ctx),
            "help_search" => self.run_help_search(tc, ctx).await,
            "request_full_context" => self.run_request_full_context(tc, ctx).await,
            // W4.1d5
            "state" => self.run_state(tc, ctx).await,
            "run_sql" => self.run_run_sql(tc, ctx).await,
            "send_mail" => self.run_send_mail(tc, ctx).await,
            "list_email" => self.run_list_email(tc, ctx).await,
            "search_web" => self.run_web_search(tc, ctx),
            // W4.9b: photo trio.
            "keep_image" => self.run_keep_image(tc, ctx).await,
            "list_images" => self.run_list_images(tc, ctx).await,
            "attach_image" => self.run_attach_image(tc, ctx).await,
            // W4.1d3b: every ported doc-edit tool routes through one handler.
            n if is_ported_doc_edit_tool(n) => self.run_doc_edit(tc, ctx).await,
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

    // ── W4.1d4: search tools ──────────────────────────────────────────────

    // -- search (Scriptorium unified search; tool-executor.ts:938) ----------
    async fn run_search(&self, tc: &ToolCall, ctx: &ToolExecutionContext) -> ToolResult {
        // v4's dispatcher guard: a character surface needs a character; the
        // operator surface (Brahma Console) is character-less.
        if ctx.character_id.is_none() && !ctx.operator_surface {
            return fail("search", "Search requires a character context");
        }
        let context = search::SearchContext {
            user_id: ctx.user_id.clone(),
            character_id: ctx.character_id.clone(),
            embedding_profile_id: ctx.embedding_profile_id.clone(),
            project_id: ctx.project_id.clone(),
            operator_surface: ctx.operator_surface,
        };
        let out = search::execute_search_scriptorium(
            &self.db,
            &self.embedding_provider,
            &context,
            &tc.arguments,
            crate::clock::now_unix_ms() as f64,
        )
        .await;
        if out.success {
            // v4: `success && results ? format : error || 'No results found'`.
            let formatted = out
                .results
                .as_ref()
                .map(|r| search::format_search_scriptorium_results(r))
                .unwrap_or_else(|| "No results found".to_string());
            ok(
                "search",
                json!({
                    "formattedText": formatted,
                    "results": out.results,
                    "totalFound": out.total_found,
                    "query": out.query,
                }),
            )
        } else {
            fail("search", out.error.unwrap_or_default())
        }
    }

    // -- project_info (tool-executor.ts:434) --------------------------------
    fn run_project_info(&self, tc: &ToolCall, ctx: &ToolExecutionContext) -> ToolResult {
        // v4's dispatcher guard: project_info needs a project context.
        let Some(project_id) = ctx.project_id.clone() else {
            return fail("project_info", "Project info requires a project context");
        };
        let context = project_info::ProjectInfoContext {
            user_id: ctx.user_id.clone(),
            project_id,
            embedding_profile_id: ctx.embedding_profile_id.clone(),
        };
        let out = project_info::execute_project_info(&self.db, &context, &tc.arguments);
        let formatted = project_info::format_project_info(&out);
        if out.success {
            ok(
                "project_info",
                json!({
                    "formattedText": formatted,
                    "action": out.action,
                    "data": out.data,
                }),
            )
        } else {
            fail("project_info", out.error.unwrap_or_default())
        }
    }

    // -- help_search (tool-executor.ts:491) ---------------------------------
    async fn run_help_search(&self, tc: &ToolCall, ctx: &ToolExecutionContext) -> ToolResult {
        let out = help_search::execute_help_search(
            &self.db,
            &self.embedding_provider,
            &ctx.user_id,
            &tc.arguments,
        )
        .await;
        if out.success {
            // v4: `success && results ? format : error || 'No help documentation found'`.
            let formatted = out
                .results
                .as_ref()
                .map(|r| help_search::format_help_search(r))
                .unwrap_or_else(|| "No help documentation found".to_string());
            ok(
                "help_search",
                json!({
                    "formattedText": formatted,
                    "results": out.results,
                    "totalFound": out.total_found,
                    "query": out.query,
                }),
            )
        } else {
            fail("help_search", out.error.unwrap_or_default())
        }
    }

    // -- request_full_context (tool-executor.ts:470) ------------------------
    async fn run_request_full_context(
        &self,
        tc: &ToolCall,
        ctx: &ToolExecutionContext,
    ) -> ToolResult {
        let out = request_full_context::execute_request_full_context(
            &self.db,
            &ctx.chat_id,
            &tc.arguments,
        )
        .await;
        let formatted = request_full_context::format_request_full_context(&out);
        if out.success {
            ok(
                "request_full_context",
                json!({ "formattedText": formatted }),
            )
        } else {
            // v4 maps the error from `result.message`.
            fail("request_full_context", out.message)
        }
    }

    // -- state (tool-executor.ts:639) ---------------------------------------
    async fn run_state(&self, tc: &ToolCall, ctx: &ToolExecutionContext) -> ToolResult {
        let context = state::StateToolContext {
            user_id: ctx.user_id.clone(),
            chat_id: ctx.chat_id.clone(),
            project_id: ctx.project_id.clone(),
        };
        let out = state::execute_state_tool(&self.db, &context, &tc.arguments).await;
        let formatted = state::format_state_results(&out);
        if out.success {
            // v4: `{ formattedText, operation, context, path, value, previousValue }`
            // — undefined fields dropped by JSON.stringify.
            let mut result = Map::new();
            result.insert("formattedText".into(), json!(formatted));
            result.insert("operation".into(), out.operation.clone());
            if let Some(c) = &out.context {
                result.insert("context".into(), json!(c));
            }
            if let Some(p) = &out.path {
                result.insert("path".into(), json!(p));
            }
            if let Some(v) = &out.value {
                result.insert("value".into(), v.clone());
            }
            if let Some(pv) = &out.previous_value {
                result.insert("previousValue".into(), pv.clone());
            }
            ok("state", Value::Object(result))
        } else {
            fail("state", out.error.clone().unwrap_or_default())
        }
    }

    // -- run_sql (tool-executor.ts:982; operator-gated) ---------------------
    async fn run_run_sql(&self, tc: &ToolCall, ctx: &ToolExecutionContext) -> ToolResult {
        // v4's dispatcher gate: run_sql is only available on the Brahma Console.
        if !ctx.operator_surface {
            return fail(
                "run_sql",
                "run_sql is only available in the Brahma Console.",
            );
        }
        let out = run_sql::execute_run_sql_tool(&self.db, &tc.arguments, &ctx.user_id).await;
        // v4: `result: result.success ? result : null` — the whole `{ success, .. }`
        // envelope is spread; the ToolResult carries it as `result`.
        match &out {
            run_sql::RunSqlResult::Success { .. } => {
                ok("run_sql", serde_json::to_value(&out).unwrap_or(Value::Null))
            }
            run_sql::RunSqlResult::Failure { error } => fail("run_sql", error.clone()),
        }
    }

    // -- search_web (tool-executor.ts:406) ----------------------------------
    fn run_web_search(&self, tc: &ToolCall, ctx: &ToolExecutionContext) -> ToolResult {
        let out =
            web_search::execute_web_search(&*self.web_search_provider, &ctx.user_id, &tc.arguments);
        if out.success {
            // v4: `{ formattedText, results, totalFound, query }`.
            let formatted =
                web_search::format_web_search_results(out.results.as_deref().unwrap_or(&[]));
            ok(
                "search_web",
                json!({
                    "formattedText": formatted,
                    "results": out.results,
                    "totalFound": out.total_found,
                    "query": out.query,
                }),
            )
        } else {
            fail("search_web", out.error.unwrap_or_default())
        }
    }

    // ── W4.9b: photo trio (v4 groups these under isDocEditTool → executeDocEditTool)
    // Each dispatches inside a both-connections `Db::write` closure (the wardrobe /
    // doc-edit precedent) so the handler holds MAIN (`characters`/`chats`/`files`) +
    // mount-index (the store). v4's dispatch row for keep_image/list_images is
    // `{ formattedText: formatDocEditResults(...), ...result.result }`; the
    // photo handlers ALWAYS set `formattedText` on success, so
    // `formatDocEditResults` returns it verbatim. attach_image passes its descriptor
    // ARRAY through unchanged (NOT spread).

    // -- keep_image (photo-handlers.ts:handleKeepImage) ---------------------
    async fn run_keep_image(&self, tc: &ToolCall, ctx: &ToolExecutionContext) -> ToolResult {
        let args = tc.arguments.clone();
        let photo_ctx = self.photo_context(ctx);
        // v4 mints keptAt = new Date().toISOString() inside saveImageToAlbum; here it
        // is the injected clock so the handler stays clock-testable.
        let kept_at = crate::clock::now_iso();
        let bytes = self.file_bytes.clone();
        let side_effects = self.photo_side_effects.clone();
        self.photo_write(move |main, mount| {
            let out = photo::handle_keep_image(
                main,
                mount,
                &args,
                &photo_ctx,
                &*bytes,
                &*side_effects,
                &kept_at,
            );
            photo_result_row("keep_image", out, false)
        })
        .await
    }

    // -- list_images (photo-handlers.ts:handleListImages) -------------------
    async fn run_list_images(&self, tc: &ToolCall, ctx: &ToolExecutionContext) -> ToolResult {
        // The semantic branch needs an embedding (async). Compute it up front (the
        // rest of the handler is a both-connection DB read); a failure feeds the
        // handler's SILENT fall-through to plain listing.
        let query = tc
            .arguments
            .get("query")
            .and_then(Value::as_str)
            .map(|q| crate::jsstr::js_trim(q).to_string())
            .unwrap_or_default();
        let embedding: Option<Result<Vec<f32>, String>> = if query.is_empty() {
            None
        } else {
            Some(
                self.embedding_provider
                    .generate_embedding_for_user(
                        &query,
                        &ctx.user_id,
                        ctx.embedding_profile_id.as_deref(),
                        EmbeddingPriority::Interactive,
                    )
                    .await
                    .map(|r| r.embedding)
                    .map_err(|e| e.message),
            )
        };
        let args = tc.arguments.clone();
        let photo_ctx = self.photo_context(ctx);
        let doc_ctx = self.doc_context(ctx);
        self.photo_write(move |main, mount| {
            // v4 collectPeerCharacterIdsForReads (the Shared-Vaults gate lives on the
            // chat's `allowCrossCharacterVaultReads`).
            let peers = collect_peer_character_ids_for_reads(main, &doc_ctx);
            // Adapt the pre-fetched embedding into the handler's sync embed closure.
            let embed = move |_q: &str| -> Result<Vec<f32>, String> {
                match &embedding {
                    Some(Ok(v)) => Ok(v.clone()),
                    Some(Err(e)) => Err(e.clone()),
                    // Empty query never reaches the semantic branch.
                    None => Err("no embedding (empty query)".to_string()),
                }
            };
            let out = photo::handle_list_images(main, mount, &args, &photo_ctx, &peers, &embed);
            photo_result_row("list_images", out, false)
        })
        .await
    }

    // -- attach_image (photo-handlers.ts:handleAttachImage) -----------------
    async fn run_attach_image(&self, tc: &ToolCall, ctx: &ToolExecutionContext) -> ToolResult {
        let args = tc.arguments.clone();
        let photo_ctx = self.photo_context(ctx);
        self.photo_write(move |main, mount| {
            let out = photo::handle_attach_image(main, mount, &args, &photo_ctx);
            // v4: attach_image passes its descriptor ARRAY through unchanged.
            photo_result_row("attach_image", out, true)
        })
        .await
    }

    /// Snapshot the [`PhotoToolContext`] the handlers need from the exec context.
    fn photo_context(&self, ctx: &ToolExecutionContext) -> PhotoToolContext {
        PhotoToolContext {
            chat_id: ctx.chat_id.clone(),
            user_id: ctx.user_id.clone(),
            project_id: ctx.project_id.clone(),
            character_id: ctx.character_id.clone(),
            embedding_profile_id: ctx.embedding_profile_id.clone(),
        }
    }

    /// The [`DocEditToolContext`] the peer-read gate needs (operator_override is
    /// always false on the tool-call path).
    fn doc_context(&self, ctx: &ToolExecutionContext) -> DocEditToolContext {
        DocEditToolContext {
            chat_id: ctx.chat_id.clone(),
            user_id: ctx.user_id.clone(),
            project_id: ctx.project_id.clone(),
            character_id: ctx.character_id.clone(),
            operator_override: false,
        }
    }

    /// Run a photo handler inside a single write closure with both writer
    /// connections (the wardrobe precedent). A main-only instance (no mount-index)
    /// is a clear failure — photos live in the document store.
    async fn photo_write<G>(&self, build: G) -> ToolResult
    where
        G: FnOnce(&rusqlite::Connection, &rusqlite::Connection) -> ToolResult + Send + 'static,
    {
        let db = self.db.clone();
        db.write(move |writers| {
            let Some(mount_w) = writers.mount_index() else {
                return Ok(fail(
                    "photo",
                    "photo tools require the mount-index database",
                ));
            };
            let mount = mount_w.connection();
            let main = writers.main().connection();
            Ok(build(main, mount))
        })
        .await
        .unwrap_or_else(|e| fail("photo", e.to_string()))
    }

    // -- send_mail (Post Office; tool-executor.ts:1114) ----------------------
    async fn run_send_mail(&self, tc: &ToolCall, ctx: &ToolExecutionContext) -> ToolResult {
        let args = tc.arguments.clone();
        let user_id = ctx.user_id.clone();
        let character_id = ctx.character_id.clone();
        // v4 mints `sentAt = new Date().toISOString()` inside the delivery path;
        // here it is injected so the handler stays clock-testable.
        let now_iso = crate::clock::now_iso();
        self.wardrobe_write(self.db.clone(), move |main, mount| {
            let out = send_mail::execute_send_mail(
                main,
                mount,
                &user_id,
                character_id.as_deref(),
                &args,
                &now_iso,
            );
            let formatted = send_mail::format_send_mail_results(&out);
            // v4 returns `result: { formattedText, path }` even on failure; error
            // set only when !success. `path` (undefined on failure) is dropped.
            let mut result = Map::new();
            result.insert("formattedText".into(), json!(formatted));
            result.insert("path".into(), json!(out.path));
            ToolResult {
                tool_name: "send_mail".into(),
                success: out.success,
                result: Value::Object(result),
                error: if out.success { None } else { out.error },
                message: None,
                metadata: None,
            }
        })
        .await
    }

    // -- list_email (Post Office; tool-executor.ts:1131) ---------------------
    async fn run_list_email(&self, tc: &ToolCall, ctx: &ToolExecutionContext) -> ToolResult {
        let args = tc.arguments.clone();
        let character_id = ctx.character_id.clone();
        self.wardrobe_write(self.db.clone(), move |main, mount| {
            let out = list_email::execute_list_email(main, mount, character_id.as_deref(), &args);
            let formatted = list_email::format_list_email_results(&out);
            // v4 returns `result: { formattedText, count }`; error when !success.
            let result = json!({ "formattedText": formatted, "count": out.count });
            ToolResult {
                tool_name: "list_email".into(),
                success: out.success,
                result,
                error: if out.success { None } else { out.error },
                message: None,
                metadata: None,
            }
        })
        .await
    }

    // ── wardrobe tools (tool-executor.ts:667–826) ─────────────────────────
    // Each dispatches inside a single `Db::write` closure so the sync handler
    // holds BOTH the main + mount-index writer connections (the wardrobe reads
    // span `characters`/`chats` + the vault store, and the mutations write both).
    // The mutating handlers (create/wear/take_off/archive/update) run there too,
    // serialized on the writer thread. `wardrobe_{archive,wear,take_off}` return a
    // pending-announcement id list, folded into the per-turn set inside the closure
    // (v4's `recordPendingWardrobeAnnouncement`).

    async fn run_wardrobe_list(&self, tc: &ToolCall, ctx: &ToolExecutionContext) -> ToolResult {
        let (db, args, user_id, chat_id, character_id) = self.wardrobe_snapshot(tc, ctx);
        self.wardrobe_write(db, move |main, mount| {
            let out = wardrobe_list::execute(main, mount, &user_id, &chat_id, &character_id, &args);
            let formatted = wardrobe_list::format(&out);
            if out.success {
                ok(
                    "wardrobe_list",
                    json!({
                        "formattedText": formatted,
                        "items": out.items,
                        "total_count": out.total_count,
                    }),
                )
            } else {
                fail("wardrobe_list", out.error.clone().unwrap_or_default())
            }
        })
        .await
    }

    async fn run_wardrobe_read(&self, tc: &ToolCall, ctx: &ToolExecutionContext) -> ToolResult {
        let (db, args, user_id, chat_id, character_id) = self.wardrobe_snapshot(tc, ctx);
        self.wardrobe_write(db, move |main, mount| {
            let out = wardrobe_read::execute(main, mount, &user_id, &chat_id, &character_id, &args);
            let formatted = wardrobe_read::format(&out);
            if out.success {
                // v4: `{ formattedText, ...result }`.
                ok("wardrobe_read", spread_output(&formatted, &out))
            } else {
                fail("wardrobe_read", out.error.clone().unwrap_or_default())
            }
        })
        .await
    }

    async fn run_wardrobe_create(&self, tc: &ToolCall, ctx: &ToolExecutionContext) -> ToolResult {
        let (db, args, user_id, chat_id, character_id) = self.wardrobe_snapshot(tc, ctx);
        self.wardrobe_write(db, move |main, mount| {
            let out =
                wardrobe_create::execute(main, mount, &user_id, &chat_id, &character_id, &args);
            let formatted = wardrobe_create::format(&out);
            if out.success {
                // v4 selects a subset: formattedText, item_id, title, equipped,
                // recipient_name?, current_state? (undefined ones omitted).
                let mut result = Map::new();
                result.insert("formattedText".into(), json!(formatted));
                result.insert("item_id".into(), json!(out.item_id));
                result.insert("title".into(), json!(out.title));
                result.insert("equipped".into(), json!(out.equipped));
                if let Some(name) = &out.recipient_name {
                    result.insert("recipient_name".into(), json!(name));
                }
                if let Some(state) = &out.current_state {
                    result.insert("current_state".into(), state.clone());
                }
                ok("wardrobe_create", Value::Object(result))
            } else {
                fail("wardrobe_create", out.error.clone().unwrap_or_default())
            }
        })
        .await
    }

    async fn run_wardrobe_update(&self, tc: &ToolCall, ctx: &ToolExecutionContext) -> ToolResult {
        let (db, args, user_id, chat_id, character_id) = self.wardrobe_snapshot(tc, ctx);
        self.wardrobe_write(db, move |main, mount| {
            let out =
                wardrobe_update::execute(main, mount, &user_id, &chat_id, &character_id, &args);
            let formatted = wardrobe_update::format(&out);
            if out.success {
                ok("wardrobe_update", spread_output(&formatted, &out))
            } else {
                fail("wardrobe_update", out.error.clone().unwrap_or_default())
            }
        })
        .await
    }

    async fn run_wardrobe_archive(&self, tc: &ToolCall, ctx: &ToolExecutionContext) -> ToolResult {
        let (db, args, user_id, chat_id, character_id) = self.wardrobe_snapshot(tc, ctx);
        let announce = ctx.pending_wardrobe_announcements.clone();
        self.wardrobe_write(db, move |main, mount| {
            let (out, ids) =
                wardrobe_archive::execute(main, mount, &user_id, &chat_id, &character_id, &args);
            fold_announcements(&announce, ids);
            let formatted = wardrobe_archive::format(&out);
            if out.success {
                ok(
                    "wardrobe_archive",
                    json!({
                        "formattedText": formatted,
                        "item_id": out.item_id,
                        "title": out.title,
                        "action": out.action,
                    }),
                )
            } else {
                fail("wardrobe_archive", out.error.clone().unwrap_or_default())
            }
        })
        .await
    }

    async fn run_wardrobe_wear(&self, tc: &ToolCall, ctx: &ToolExecutionContext) -> ToolResult {
        let (db, args, user_id, chat_id, character_id) = self.wardrobe_snapshot(tc, ctx);
        let announce = ctx.pending_wardrobe_announcements.clone();
        self.wardrobe_write(db, move |main, mount| {
            let (out, ids) =
                wardrobe_wear::execute(main, mount, &user_id, &chat_id, &character_id, &args);
            fold_announcements(&announce, ids);
            let formatted = wardrobe_wear::format(&out);
            // v4 returns the result object even on failure; error set when !success.
            let result = json!({
                "formattedText": formatted,
                "operations": out.operations,
                "current_state": out.current_state,
                "coverage_summary": out.coverage_summary,
            });
            wear_take_off_result("wardrobe_wear", out.success, result, out.error.clone())
        })
        .await
    }

    async fn run_wardrobe_take_off(&self, tc: &ToolCall, ctx: &ToolExecutionContext) -> ToolResult {
        let (db, args, user_id, chat_id, character_id) = self.wardrobe_snapshot(tc, ctx);
        let announce = ctx.pending_wardrobe_announcements.clone();
        self.wardrobe_write(db, move |main, mount| {
            let (out, ids) =
                wardrobe_take_off::execute(main, mount, &user_id, &chat_id, &character_id, &args);
            fold_announcements(&announce, ids);
            let formatted = wardrobe_take_off::format(&out);
            let result = json!({
                "formattedText": formatted,
                "operations": out.operations,
                "current_state": out.current_state,
                "coverage_summary": out.coverage_summary,
            });
            wear_take_off_result("wardrobe_take_off", out.success, result, out.error.clone())
        })
        .await
    }

    // ── doc-edit tools (W4.1d3b; tool-executor.ts:1060) ───────────────────
    /// Dispatch any ported `doc_*` tool through [`execute_doc_edit_tool`], run
    /// inside a single `Db::write` closure that hands it BOTH writer connections
    /// (the handlers read `chats`/`characters` on main + the store on mount, and
    /// write the store on mount). Builds v4's dispatch-row result shape
    /// (`{ formattedText, ...result.result }` on success; the failure `error`).
    /// `operatorOverride` is always false on the tool-call path (the operator
    /// surfaces — Document Mode / Brahma Console — never route through here).
    async fn run_doc_edit(&self, tc: &ToolCall, ctx: &ToolExecutionContext) -> ToolResult {
        let name = tc.name.clone();
        let args = tc.arguments.clone();
        let doc_ctx = DocEditToolContext {
            chat_id: ctx.chat_id.clone(),
            user_id: ctx.user_id.clone(),
            project_id: ctx.project_id.clone(),
            character_id: ctx.character_id.clone(),
            operator_override: false,
        };
        let db = self.db.clone();
        db.write(move |writers| {
            let Some(mount_w) = writers.mount_index() else {
                return Ok(fail(
                    &name,
                    "doc-edit tools require the mount-index database",
                ));
            };
            let mount = mount_w.connection();
            let main = writers.main().connection();
            let result = execute_doc_edit_tool(main, mount, &name, &args, &doc_ctx);
            Ok(if result.success {
                let mut m = Map::new();
                m.insert(
                    "formattedText".into(),
                    json!(format_doc_edit_results(&result)),
                );
                if let Some(Value::Object(obj)) = result.result {
                    for (k, v) in obj {
                        m.insert(k, v);
                    }
                }
                ok(&name, Value::Object(m))
            } else {
                fail(&name, result.error.unwrap_or_default())
            })
        })
        .await
        .unwrap_or_else(|e| fail(&tc.name, e.to_string()))
    }

    /// Snapshot the ctx fields a wardrobe handler needs (owned, so they can move
    /// into the `'static` writer closure). `characterId || ''` per v4's dispatcher.
    fn wardrobe_snapshot(
        &self,
        tc: &ToolCall,
        ctx: &ToolExecutionContext,
    ) -> (Db, Value, String, String, String) {
        (
            self.db.clone(),
            tc.arguments.clone(),
            ctx.user_id.clone(),
            ctx.chat_id.clone(),
            ctx.character_id.clone().unwrap_or_default(),
        )
    }

    /// Run a wardrobe handler inside a single write closure that hands it both the
    /// main and mount-index writer connections. A main-only instance (no
    /// mount-index) is a clear failure (wardrobe lives in the document store).
    async fn wardrobe_write<G>(&self, db: Db, build: G) -> ToolResult
    where
        G: FnOnce(&rusqlite::Connection, &rusqlite::Connection) -> ToolResult + Send + 'static,
    {
        let result = db
            .write(move |writers| {
                let Some(mount_w) = writers.mount_index() else {
                    return Ok(fail(
                        "wardrobe",
                        "wardrobe tools require the mount-index database",
                    ));
                };
                // Borrow both connections for the handler.
                let mount = mount_w.connection();
                let main = writers.main().connection();
                Ok(build(main, mount))
            })
            .await;
        result.unwrap_or_else(|e| fail("wardrobe", e.to_string()))
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

/// Build the dispatch row for a photo-tool result (v4 `executeDocEditTool` row).
/// For `keep_image` / `list_images` (`pass_through == false`) the success row is
/// `{ formattedText, ...result.result }` (the handler always sets `formattedText`, so
/// `formatDocEditResults` returns it verbatim). For `attach_image`
/// (`pass_through == true`) the success `result` is the handler's `result` VALUE
/// (the descriptor array) passed through unchanged.
fn photo_result_row(name: &str, out: photo::PhotoToolResult, pass_through: bool) -> ToolResult {
    if !out.success {
        return fail(name, out.error.unwrap_or_default());
    }
    if pass_through {
        // attach_image: result is the descriptor array, unchanged (or null).
        return ok(name, out.result.unwrap_or(Value::Null));
    }
    // keep_image / list_images: { formattedText, ...result.result }.
    let mut m = Map::new();
    m.insert(
        "formattedText".into(),
        json!(out.formatted_text.unwrap_or_default()),
    );
    if let Some(Value::Object(obj)) = out.result {
        for (k, v) in obj {
            m.insert(k, v);
        }
    }
    ok(name, Value::Object(m))
}

/// v4's `{ formattedText, ...result }` — a `result` object with `formattedText`
/// prepended, then every field of the serialized handler output (used by
/// `wardrobe_read` / `wardrobe_update`).
fn spread_output<T: Serialize>(formatted: &str, out: &T) -> Value {
    let mut result = Map::new();
    result.insert("formattedText".into(), json!(formatted));
    if let Ok(Value::Object(obj)) = serde_json::to_value(out) {
        for (k, v) in obj {
            result.insert(k, v);
        }
    }
    Value::Object(result)
}

/// Fold a handler's pending-announcement character ids into the per-turn set (v4
/// `recordPendingWardrobeAnnouncement` — add to the context's Set). A poisoned lock
/// is ignored (the set is advisory).
fn fold_announcements(announce: &Arc<Mutex<HashSet<String>>>, ids: Vec<String>) {
    if ids.is_empty() {
        return;
    }
    if let Ok(mut set) = announce.lock() {
        set.extend(ids);
    }
}

/// The `wardrobe_wear` / `wardrobe_take_off` dispatch-row result: v4 returns the
/// `result` object EVEN on failure (unlike the other wardrobe tools), with `error`
/// set only when `!success`.
fn wear_take_off_result(
    name: &str,
    success: bool,
    result: Value,
    error: Option<String>,
) -> ToolResult {
    ToolResult {
        tool_name: name.to_string(),
        success,
        result,
        error: if success { None } else { error },
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

    #[test]
    fn fold_announcements_dedupes_into_shared_set() {
        let set = Arc::new(Mutex::new(HashSet::new()));
        fold_announcements(&set, vec!["char-a".into()]);
        fold_announcements(&set, vec![]); // empty is a no-op
        fold_announcements(&set, vec!["char-a".into(), "char-b".into()]);
        let guard = set.lock().unwrap();
        assert_eq!(guard.len(), 2);
        assert!(guard.contains("char-a"));
        assert!(guard.contains("char-b"));
    }

    #[test]
    fn wardrobe_tools_are_ported() {
        for name in [
            "wardrobe_list",
            "wardrobe_read",
            "wardrobe_create",
            "wardrobe_update",
            "wardrobe_archive",
            "wardrobe_wear",
            "wardrobe_take_off",
        ] {
            assert!(PORTED_TOOLS.contains(&name), "{name} must be dispatched");
            assert!(BuiltInToolRunner::<LoudFallbackRunner>::handles(name));
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
