//! The doc-edit tool family — port of v4 `lib/tools/handlers/doc-edit/` +
//! the `doc-edit-handler.ts` dispatcher (`executeDocEditTool` /
//! `formatDocEditResults`).
//!
//! The ~26 `doc_*` tools address database-backed document stores (character
//! vaults, project/group stores) through the d3a foundation
//! ([`crate::doc_edit`] path resolver / qtap codec / markdown parser) + the
//! [`crate::db::database_store`] primitives + this family's [`shared`]
//! access-control helpers. Each handler is **synchronous** and takes both writer
//! connections (`main` + `mount`); the [`BuiltInToolRunner`] dispatches every
//! doc tool inside one [`crate::db::runtime::Db::write`] closure (the wardrobe
//! precedent).
//!
//! ## The dispatcher contract (v4 `executeDocEditTool`)
//!
//! A handler returns [`DocEditToolResult`] on a normal path (success or an
//! explicit validation failure, which may carry its own `formattedText`), or
//! `Err(String)` for a **thrown** error (a `PathResolutionError`, an access-gate
//! denial, or a store error). [`execute_doc_edit_tool`] maps a thrown error to
//! `{ success:false, error, formattedText: "Error: <msg>" }`, exactly like v4's
//! outer try/catch (which treats a `PathResolutionError` and a generic `Error`
//! the same in the result — only the log line differs).

pub mod shared;

pub mod blob;
pub mod document_ui;
pub mod file_management;
pub mod markdown;
pub mod text;

use rusqlite::Connection;
use serde::Serialize;
use serde_json::Value;

pub use shared::DocEditToolContext;

use crate::services::librarian_notifications::LibrarianWriteAnnouncement;

/// Every `doc_*` tool name (v4 `DOC_EDIT_TOOL_NAMES`, `doc-edit-handler.ts:96`).
pub const DOC_EDIT_TOOL_NAMES: &[&str] = &[
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
];

/// The doc-edit tool result shape (v4 `{ success, result?, error?, formattedText? }`).
/// Serializes with `undefined` keys omitted (skip-if-none), matching
/// `JSON.stringify` of v4's return object. Object comparison in the differential
/// is key-order-insensitive (serde_json `Value` map equality), so per-handler
/// literal order need not be reproduced — only the human `formattedText` string
/// (and the `JSON.stringify`-formatted fallback for tools that set none) is
/// order-sensitive, and each such handler builds its `result` in v4's order.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct DocEditToolResult {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(rename = "formattedText", skip_serializing_if = "Option::is_none")]
    pub formatted_text: Option<String>,
    /// The Librarian doc-save (`change:{diff}`/`{created,body}`) announcement a
    /// mutating write handler wants posted (v4 fires it inline; here it is threaded
    /// OUT of the synchronous `Db::write` closure so the async caller can `await`
    /// [`crate::services::librarian_notifications::post_librarian_write_announcement`]
    /// after the closure returns — the wardrobe-drain `pending*` precedent). `None`
    /// for every read/no-op/failed handler and every non-write path. NEVER
    /// serialized (v4 puts `change` only in the announcement call, not the tool
    /// result), so it does not affect the differential's output diff.
    #[serde(skip)]
    pub pending_librarian_announcement: Option<LibrarianWriteAnnouncement>,
}

impl DocEditToolResult {
    /// A success result carrying a `result` object + a `formattedText` string.
    pub fn ok(result: Value, formatted: impl Into<String>) -> Self {
        DocEditToolResult {
            success: true,
            result: Some(result),
            error: None,
            formatted_text: Some(formatted.into()),
            pending_librarian_announcement: None,
        }
    }

    /// An explicit validation failure (`{ success:false, error }`), no
    /// `formattedText` — v4's bare `return { success: false, error: '...' }`.
    pub fn fail(error: impl Into<String>) -> Self {
        DocEditToolResult {
            success: false,
            result: None,
            error: Some(error.into()),
            formatted_text: None,
            pending_librarian_announcement: None,
        }
    }

    /// A validation failure that ALSO carries a `formattedText` (some handlers,
    /// e.g. `doc_str_replace`'s not-found/ambiguous paths, set both).
    pub fn fail_with_formatted(error: impl Into<String>, formatted: impl Into<String>) -> Self {
        DocEditToolResult {
            success: false,
            result: None,
            error: Some(error.into()),
            formatted_text: Some(formatted.into()),
            pending_librarian_announcement: None,
        }
    }

    /// Attach a pending Librarian write announcement to a success result (chainable
    /// after [`DocEditToolResult::ok`]). The caller posts it after the write closure
    /// returns.
    pub fn with_librarian_announcement(mut self, announcement: LibrarianWriteAnnouncement) -> Self {
        self.pending_librarian_announcement = Some(announcement);
        self
    }
}

/// The doc-edit dispatcher — v4 `executeDocEditTool` (`doc-edit-handler.ts:135`).
/// Routes by name to the handler, mapping a thrown `Err(msg)` to the caught-error
/// result shape (`{ success:false, error:msg, formattedText:"Error: msg" }`).
pub fn execute_doc_edit_tool(
    main: &Connection,
    mount: &Connection,
    tool_name: &str,
    args: &Value,
    ctx: &DocEditToolContext,
) -> DocEditToolResult {
    let outcome: Result<DocEditToolResult, String> = match tool_name {
        "doc_read_file" => text::handle_read_file(main, mount, args, ctx),
        "doc_write_file" => text::handle_write_file(main, mount, args, ctx),
        "doc_str_replace" => text::handle_str_replace(main, mount, args, ctx),
        "doc_insert_text" => text::handle_insert_text(main, mount, args, ctx),
        "doc_grep" => text::handle_grep(main, mount, args, ctx),
        "doc_list_files" => text::handle_list_files(main, mount, args, ctx),
        "doc_read_frontmatter" => markdown::handle_read_frontmatter(main, mount, args, ctx),
        "doc_update_frontmatter" => markdown::handle_update_frontmatter(main, mount, args, ctx),
        "doc_read_heading" => markdown::handle_read_heading(main, mount, args, ctx),
        "doc_update_heading" => markdown::handle_update_heading(main, mount, args, ctx),
        "doc_move_file" => file_management::handle_move_file(main, mount, args, ctx),
        "doc_copy_file" => file_management::handle_copy_file(main, mount, args, ctx),
        "doc_delete_file" => file_management::handle_delete_file(main, mount, args, ctx),
        "doc_create_folder" => file_management::handle_create_folder(main, mount, args, ctx),
        "doc_delete_folder" => file_management::handle_delete_folder(main, mount, args, ctx),
        "doc_move_folder" => file_management::handle_move_folder(main, mount, args, ctx),
        "doc_open_document" => document_ui::handle_open_document(main, mount, args, ctx),
        "doc_close_document" => document_ui::handle_close_document(main, mount, args, ctx),
        "doc_focus" => document_ui::handle_doc_focus(main, mount, args, ctx),
        "doc_write_blob" => blob::handle_write_blob(main, mount, args, ctx),
        "doc_read_blob" => blob::handle_read_blob(main, mount, args, ctx),
        "doc_list_blobs" => blob::handle_list_blobs(main, mount, args, ctx),
        "doc_delete_blob" => blob::handle_delete_blob(main, mount, args, ctx),
        // The photo trio (`keep_image`/`list_images`/`attach_image`) is
        // `isDocEditTool` in v4 but needs deps this sync entry point can't carry
        // (the injected `FileBytesStore` bytes seam + the async embedding provider +
        // the minted `keptAt` clock), so the dispatcher (`tools::executor`) routes
        // them to the dedicated `tools::photo` handlers directly — they never reach
        // this function. Guarded here so a stray direct call fails loudly rather than
        // hitting the retired stub.
        "keep_image" | "list_images" | "attach_image" => {
            return DocEditToolResult::fail(format!(
                "{tool_name} is dispatched via tools::photo, not execute_doc_edit_tool"
            ));
        }
        other => {
            return DocEditToolResult::fail(format!("Unknown doc-edit tool: {other}"));
        }
    };

    match outcome {
        Ok(result) => result,
        // v4's outer catch: a thrown PathResolutionError / Error → failure result
        // with `formattedText: "Error: <message>"`.
        Err(message) => DocEditToolResult {
            success: false,
            result: None,
            error: Some(message.clone()),
            formatted_text: Some(format!("Error: {message}")),
            pending_librarian_announcement: None,
        },
    }
}

/// v4 `formatDocEditResults` (`doc-edit-handler.ts:225`): the human string the
/// tool loop shows. Prefers `formattedText`; else `"Error: <error>"` on failure;
/// else the pretty-printed `result` object (`JSON.stringify(result.result, null,
/// 2)`).
pub fn format_doc_edit_results(result: &DocEditToolResult) -> String {
    if let Some(ft) = &result.formatted_text {
        return ft.clone();
    }
    if !result.success {
        return format!(
            "Error: {}",
            result.error.as_deref().unwrap_or("Unknown error")
        );
    }
    match &result.result {
        Some(v) => serde_json::to_string_pretty(v).unwrap_or_default(),
        None => "null".to_string(),
    }
}
