//! Document-UI doc-edit handlers — v4
//! `lib/tools/handlers/doc-edit/document-ui-handlers.ts`: `doc_open_document`,
//! `doc_close_document`, `doc_focus`.
//!
//! These control the split-panel editor pane in the Salon: they write to the
//! `chat_documents` table and set `chats.documentMode`, returning a structured
//! response the frontend interprets to drive the UI. Neither the `chat_documents`
//! write (`openDocument`/`closeDocumentById`) nor the `documentMode` update bumps
//! the chat's own `updatedAt` (v4 `chats.update` preserves it unless a caller
//! passes `updatedAt`, which these handlers never do).
//!
//! ## The open announcement (W4.6c — now live)
//!
//! `postLibrarianOpenAnnouncement` is built inside the open handler (with its
//! bespoke `characters.findById` origin resolution + `documentHiddenFromCharacters`
//! suppression gate) and threaded out via
//! [`DocEditToolResult::pending_librarian_announcement`]
//! ([`super::PendingLibrarianAnnouncement::Open`]); the executor posts it after the
//! write closure returns (the G6 write-announcement precedent).
//!
//! ## The new-blank-document path (a tracked FsSeam deferral)
//!
//! v4's `doc_open_document` supports opening WITHOUT a `filePath`: it mints a
//! `crypto.randomUUID()` name and `fs.writeFile`s an empty file to the resolved
//! **on-disk** path. That branch is real host-filesystem I/O (the d3a
//! [`crate::doc_edit::path_resolver::ResolveError::FsSeam`]) — deferred to the
//! Phase-4 host. Every corpus open passes `filePath`, so this port returns a clear
//! deferral error when it is absent.

use rusqlite::Connection;
use serde_json::{Map, Value};

use super::shared::{
    apply_qtap_uri, arg_bool, arg_str, arg_str_ref, assert_character_may_read, basename,
    build_read_resolution_context, document_hidden_from_characters, librarian_scope_from,
    resolve_error_message, scope_from_str, DocEditToolContext,
};
use super::DocEditToolResult;
use crate::db::characters_read;
use crate::db::chat_documents::{ChatDocumentsRepository, OpenChatDocument, OpenDocumentInput};
use crate::db::chats::{ChatUpdate, ChatsRepository};
use crate::db::database_store::database_document_exists;
use crate::doc_edit::path_resolver::resolve_doc_edit_path;
use crate::doc_edit::uri_producers::uri_for_resolved_path;
use crate::doc_edit::DocEditScope;
use crate::services::librarian_notifications::{LibrarianOpenAnnouncement, LibrarianOpenKind};
use crate::tools::llm_number::llm_number;

/// v4 `resolveOpenDocTarget` (`document-ui-handlers.ts:45`): pick which open
/// document a per-document UI tool (`doc_focus` / `doc_close`) targets. With
/// several documents open, the caller names one by `path` (optionally narrowed by
/// `scope` / `mount_point`); when no path is given, default to the **most recently
/// opened** (`findOpenForChat` is oldest-first, so the last entry). Returns `None`
/// when nothing is open or the named document isn't currently open.
fn resolve_open_doc_target(
    main: &Connection,
    chat_id: &str,
    sel_path: Option<&str>,
    sel_scope: Option<&str>,
    sel_mount_point: Option<&str>,
) -> Result<Option<OpenChatDocument>, String> {
    let open = ChatDocumentsRepository::new(main)
        .find_open_for_chat(chat_id)
        .map_err(|e| e.to_string())?;
    if open.is_empty() {
        return Ok(None);
    }
    let Some(path) = sel_path else {
        // Newest open = last entry (oldest-first ordering).
        return Ok(open.into_iter().last());
    };
    let matched = open.into_iter().find(|d| {
        d.file_path == path
            && (sel_scope.is_none() || Some(d.scope.as_str()) == sel_scope)
            // v4: (d.mountPoint ?? undefined) === selector.mount_point — a narrowing
            // mount_point matches only when the row's mountPoint equals it (a NULL
            // row's mountPoint is `undefined`, so it never matches a named filter).
            && (sel_mount_point.is_none() || d.mount_point.as_deref() == sel_mount_point)
    });
    Ok(matched)
}

// --- doc_open_document ---

pub fn handle_open_document(
    main: &Connection,
    mount: &Connection,
    args: &Value,
    ctx: &DocEditToolContext,
) -> Result<DocEditToolResult, String> {
    let addressing = apply_qtap_uri(
        arg_str(args, "scope"),
        arg_str(args, "mount_point"),
        arg_str(args, "path"),
        arg_str_ref(args, "uri"),
    )?;
    // v4: scope = (input.scope || 'project'); an unrecognized string is not
    // re-defaulted here (the cast is loose), but the corpus always passes a valid
    // scope or none — `scope_from_str(_, Project)` reproduces the `|| 'project'`.
    let scope = scope_from_str(addressing.scope.as_deref(), DocEditScope::Project);
    // The enum → the raw string v4 stores/returns (`scope` in the output +
    // `openDocument`'s scope column).
    let scope_str = scope.as_str();
    // v4: mode = input.mode || 'split'.
    let mode = arg_str(args, "mode").unwrap_or_else(|| "split".to_string());

    let Some(file_path) = addressing.path.clone() else {
        // The new-blank-document path is the FsSeam (see the module header).
        return Err(
            "Creating a blank document (no `path`) is not available here (host-filesystem seam)."
                .to_string(),
        );
    };

    // v4: displayTitle = input.title || (fallback below). `input.title` is the
    // ORIGINAL title arg, unaffected by the URI (v4 reads `input.title` on the
    // post-URI input, but the URI codec never sets `title`).
    let title_arg = arg_str(args, "title");

    // Opening an existing file — resolve as a READ (peer vaults reachable when the
    // chat's cross-character read flag is enabled), matching v4. A thrown
    // PathResolutionError surfaces `{ success:false, error }` (no formattedText),
    // via the outer dispatcher on `Err`; but v4 catches HERE and returns the bare
    // failure for BOTH the resolution error and the not-found — so we do too.
    let read_context = build_read_resolution_context(main, &addressing, ctx);
    let resolved = match resolve_doc_edit_path(
        main,
        mount,
        scope,
        Some(&file_path),
        &read_context,
        ctx.files_dir.as_deref(),
    ) {
        Ok(r) => r,
        // v4: `if (error instanceof PathResolutionError) return { success:false,
        // error: error.message }` — a bare failure (no formattedText).
        Err(e) => return Ok(DocEditToolResult::fail(resolve_error_message(e))),
    };
    // character_read:false → not-found for characters (operator unaffected). v4's
    // catch turns any thrown error into `File not found: <filePath>`.
    if let Err(_msg) = assert_character_may_read(mount, &resolved, ctx) {
        return Ok(DocEditToolResult::fail(format!(
            "File not found: {file_path}"
        )));
    }
    // A character_read:false document must not be announced to characters (the read
    // gate already blocks characters; this covers the operator-override path).
    let hidden_from_characters = document_hidden_from_characters(
        mount,
        resolved.mount_point_id.as_deref(),
        &resolved.relative_path,
    );
    let doc_uri = uri_for_resolved_path(
        main,
        mount,
        &resolved,
        ctx.character_id.as_deref(),
        None,
        None,
    );

    // Existence check + mtime. Only the database-mount branch is live; a
    // non-database mount is the FsSeam (`fs.stat`) — never on the corpus.
    let mtime: i64 = if resolved.mount_type.as_deref() == Some("database") {
        // v4's catch turns a missing document / store error into `File not found`.
        let exists = resolved
            .mount_point_id
            .as_deref()
            .and_then(|mp_id| database_document_exists(mount, mp_id, &resolved.relative_path).ok());
        if exists != Some(true) {
            return Ok(DocEditToolResult::fail(format!(
                "File not found: {file_path}"
            )));
        }
        // v4: mtime = Date.now(). Minted → placeholdered in the differential.
        crate::clock::now_unix_ms()
    } else {
        // Non-database mount = FsSeam; v4 would `fs.stat`.
        return Err(
            "This location is served from the host filesystem, which is unavailable here."
                .to_string(),
        );
    };

    // v4: displayTitle = input.title || path.basename(filePath). (The literal
    // 'Untitled document' only survives the new-blank path, which we never reach.)
    let display_title = title_arg.unwrap_or_else(|| basename(&file_path).to_string());

    // Persist: chat_documents.openDocument + chats.update({ documentMode: mode }).
    // A DB error becomes v4's `{ success:false, error: "Failed to open document:
    // <msg>" }` (no formattedText).
    let persist = (|| -> Result<(), String> {
        ChatDocumentsRepository::new(main)
            .open_document(
                &ctx.chat_id,
                &OpenDocumentInput {
                    file_path: file_path.clone(),
                    scope: scope_str.to_string(),
                    // v4 passes input.mount_point (the ORIGINAL/URI mount_point).
                    mount_point: addressing.mount_point.clone(),
                    display_title: Some(display_title.clone()),
                },
            )
            .map_err(|e| e.to_string())?;
        ChatsRepository::new(main)
            .update(
                &ctx.chat_id,
                &ChatUpdate {
                    document_mode: Some(mode.clone()),
                    ..Default::default()
                },
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    })();
    if let Err(msg) = persist {
        return Ok(DocEditToolResult::fail(format!(
            "Failed to open document: {msg}"
        )));
    }

    // Build the result object in v4's exact key order (DocOpenDocumentOutput:
    // success, filePath, uri, scope, mountPoint, displayTitle, mode, isNew, mtime).
    // `mountPoint` / `uri` are dropped when absent (JSON.stringify drops
    // `undefined`); here `uri` is always a string and `mountPoint` = input.mount_point
    // (absent unless a URI/mount_point arg supplied it).
    let mut result = Map::new();
    result.insert("success".into(), Value::Bool(true));
    result.insert("filePath".into(), Value::String(file_path.clone()));
    result.insert("uri".into(), Value::String(doc_uri));
    result.insert("scope".into(), Value::String(scope_str.to_string()));
    if let Some(mp) = &addressing.mount_point {
        result.insert("mountPoint".into(), Value::String(mp.clone()));
    }
    result.insert("displayTitle".into(), Value::String(display_title.clone()));
    result.insert("mode".into(), Value::String(mode.clone()));
    result.insert("isNew".into(), Value::Bool(false));
    result.insert("mtime".into(), Value::Number(mtime.into()));

    // v4's bespoke open-origin resolution (NOT the shared `resolveActorOrigin`):
    // `repos.characters.findById` (name is a slim column, so the raw read matches
    // the overlaid `.name`), swallowing errors, mapping a non-empty name to
    // `opened-by-character` else `opened-by-user`.
    let origin = {
        let mut character_name: Option<String> = None;
        if let Some(cid) = ctx.character_id.as_deref() {
            if let Ok(Some(character)) = characters_read::find_by_id_raw(main, cid) {
                if let Some(name) = character.get("name").and_then(Value::as_str) {
                    if !name.is_empty() {
                        character_name = Some(name.to_string());
                    }
                }
            }
        }
        match character_name {
            Some(character_name) => LibrarianOpenKind::ByCharacter { character_name },
            None => LibrarianOpenKind::ByUser,
        }
    };
    let announcement = LibrarianOpenAnnouncement {
        chat_id: ctx.chat_id.clone(),
        display_title: display_title.clone(),
        file_path: file_path.clone(),
        scope: librarian_scope_from(scope),
        mount_point: addressing.mount_point.clone(),
        is_new: false,
        origin,
        hidden_from_characters,
    };

    let formatted = format!("Opened document \"{display_title}\" ({file_path}) in {mode} mode.");
    Ok(DocEditToolResult::ok(Value::Object(result), formatted)
        .with_librarian_announcement(announcement))
}

// --- doc_close_document ---

pub fn handle_close_document(
    main: &Connection,
    _mount: &Connection,
    args: &Value,
    ctx: &DocEditToolContext,
) -> Result<DocEditToolResult, String> {
    let sel_path = arg_str_ref(args, "path");
    let sel_scope = arg_str_ref(args, "scope");
    let sel_mount_point = arg_str_ref(args, "mount_point");

    // v4 wraps the whole resolve/close in one try/catch → `{ success:false, error:
    // "Failed to close document: <msg>" }` on a thrown DB error.
    let outcome = (|| -> Result<Option<()>, String> {
        let target =
            resolve_open_doc_target(main, &ctx.chat_id, sel_path, sel_scope, sel_mount_point)?;

        let Some(target) = target else {
            // Nothing open (or nothing matched) — keep documentMode consistent.
            ChatsRepository::new(main)
                .update(
                    &ctx.chat_id,
                    &ChatUpdate {
                        document_mode: Some("normal".to_string()),
                        ..Default::default()
                    },
                )
                .map_err(|e| e.to_string())?;
            return Ok(None);
        };

        ChatDocumentsRepository::new(main)
            .close_document_by_id(&ctx.chat_id, &target.id)
            .map_err(|e| e.to_string())?;

        // Recompute documentMode: only return to 'normal' once the last open
        // document closes.
        let remaining = ChatDocumentsRepository::new(main)
            .find_open_for_chat(&ctx.chat_id)
            .map_err(|e| e.to_string())?;
        if remaining.is_empty() {
            ChatsRepository::new(main)
                .update(
                    &ctx.chat_id,
                    &ChatUpdate {
                        document_mode: Some("normal".to_string()),
                        ..Default::default()
                    },
                )
                .map_err(|e| e.to_string())?;
        }
        Ok(Some(()))
    })();

    let closed = match outcome {
        Ok(v) => v,
        Err(msg) => {
            return Ok(DocEditToolResult::fail(format!(
                "Failed to close document: {msg}"
            )));
        }
    };

    if closed.is_none() {
        // v4: `{ success:true, result: { success:true, message:'No document was
        // open.' }, formattedText: <named? "No open document matches ..." : "No
        // document was open to close.">}`.
        let mut inner = Map::new();
        inner.insert("success".into(), Value::Bool(true));
        inner.insert(
            "message".into(),
            Value::String("No document was open.".to_string()),
        );
        let formatted = match sel_path {
            Some(p) => format!("No open document matches \"{p}\"."),
            None => "No document was open to close.".to_string(),
        };
        return Ok(DocEditToolResult::ok(Value::Object(inner), formatted));
    }

    // v4: message = reason ? `Closed the document. ${reason}` : 'Closed the
    // document and returned to chat view.'; result = { success, message }.
    let message = match arg_str_ref(args, "reason") {
        Some(reason) => format!("Closed the document. {reason}"),
        None => "Closed the document and returned to chat view.".to_string(),
    };
    let mut inner = Map::new();
    inner.insert("success".into(), Value::Bool(true));
    inner.insert("message".into(), Value::String(message.clone()));
    Ok(DocEditToolResult::ok(Value::Object(inner), message))
}

// --- doc_focus ---

pub fn handle_doc_focus(
    main: &Connection,
    _mount: &Connection,
    args: &Value,
    ctx: &DocEditToolContext,
) -> Result<DocEditToolResult, String> {
    let sel_path = arg_str_ref(args, "path");
    let sel_scope = arg_str_ref(args, "scope");
    let sel_mount_point = arg_str_ref(args, "mount_point");

    let target = resolve_open_doc_target(main, &ctx.chat_id, sel_path, sel_scope, sel_mount_point)?;

    // v4 `targetIdentity` = target ? { chatDocumentId, filePath, scope, mountPoint }
    // : {}. `mountPoint` is spread even when null (a JSON `null` — NOT dropped),
    // matching v4's `mountPoint: target.mountPoint` (a nullable column value).
    //
    // doc_focus sets NO formattedText, so `format_doc_edit_results` falls back to
    // JSON.stringify(result, null, 2) — KEY-ORDER-SENSITIVE. Build the result map
    // in v4's exact insertion order.

    // clear_focus: return immediately with the (best-effort) identity.
    if arg_bool(args, "clear_focus") == Some(true) {
        let mut result = Map::new();
        result.insert("success".into(), Value::Bool(true));
        result.insert("clear_focus".into(), Value::Bool(true));
        insert_target_identity(&mut result, target.as_ref());
        return Ok(result_no_formatted(Value::Object(result)));
    }

    // No document matches → error (bare failure, no formattedText).
    let Some(target) = target else {
        let error = match sel_path {
            Some(p) => format!("No open document matches \"{p}\"."),
            None => "No document is open in Document Mode.".to_string(),
        };
        return Ok(DocEditToolResult::fail(error));
    };

    // Success: { success, ...targetIdentity, anchor?, highlight?, line? }. The
    // optional focus params are present only when the arg is supplied
    // (JSON.stringify drops `undefined`).
    let mut result = Map::new();
    result.insert("success".into(), Value::Bool(true));
    insert_target_identity(&mut result, Some(&target));
    if let Some(anchor) = args.get("anchor") {
        if !anchor.is_null() {
            result.insert("anchor".into(), anchor.clone());
        }
    }
    if let Some(highlight) = args.get("highlight") {
        if !highlight.is_null() {
            result.insert("highlight".into(), highlight.clone());
        }
    }
    if let Some(line) = args.get("line") {
        if !line.is_null() {
            // `line` is an llmNumber(...) field, and the Zod parse REPLACES the
            // value — so v4 echoes the CONVERTED number here. Echoing the raw arg
            // would put the model's quoted `"42"` in the output where v4 has 42.
            result.insert("line".into(), llm_number(line).into_owned());
        }
    }
    Ok(result_no_formatted(Value::Object(result)))
}

/// Insert v4's `targetIdentity` keys in order (`chatDocumentId`, `filePath`,
/// `scope`, `mountPoint`). Absent target → nothing (the `{}` spread). `mountPoint`
/// is always present as a JSON value (its own column value — `null` when the row's
/// column is NULL), matching v4's `mountPoint: target.mountPoint`.
fn insert_target_identity(map: &mut Map<String, Value>, target: Option<&OpenChatDocument>) {
    let Some(t) = target else {
        return;
    };
    map.insert("chatDocumentId".into(), Value::String(t.id.clone()));
    map.insert("filePath".into(), Value::String(t.file_path.clone()));
    map.insert("scope".into(), Value::String(t.scope.clone()));
    map.insert(
        "mountPoint".into(),
        match &t.mount_point {
            Some(mp) => Value::String(mp.clone()),
            None => Value::Null,
        },
    );
}

/// A success result carrying a `result` object but NO `formattedText` (doc_focus)
/// — `format_doc_edit_results` then renders `JSON.stringify(result, null, 2)`.
fn result_no_formatted(result: Value) -> DocEditToolResult {
    DocEditToolResult {
        success: true,
        result: Some(result),
        error: None,
        formatted_text: None,
        pending_librarian_announcement: None,
    }
}
