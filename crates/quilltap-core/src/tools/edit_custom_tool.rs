//! The `edit_custom_tool` LLM tool — v4 `lib/tools/edit-custom-tool-tool.ts`
//! (the input schema + the formatter's output type) +
//! `lib/tools/handlers/edit-custom-tool-handler.ts`, at the post-`c56fe278c`
//! shape (the `schema` action; the earlier `ea2cb60ab` input never lands).
//!
//! Saves or checks one of Pascal's custom-tool definitions on a model's behalf.
//! The file is always `Tools/<name>.tool.json` at the top level of the named
//! store — the only place the roster looks.
//!
//! A save is judged first by [`validate_tool_definition_text`], the same
//! judgement discovery applies at load time, plus discovery's same-store rule
//! (no two definitions in one store may share a `name`). Only a definition that
//! passes both is written, and the write itself is `doc_write_file`'s handler
//! ([`handle_write_file`]) — so store access, the per-document `character_write`
//! policy, the Librarian's announcement and the reindex are exactly a document
//! write's, with no second write path into stores. A call without content
//! writes nothing: it reads the file as it stands (under the read policy) and
//! passes the same judgement. `action: "schema"` touches no store: it hands back
//! the published JSON Schema and the two reference specimens, which is
//! everything a model needs to write a definition from scratch.
//!
//! # NO-PORT records
//!
//! v4's `contentToText` has two throw arms — `content must be a JSON string or
//! a JSON object.` (`JSON.stringify` returned `undefined`: a function or a
//! symbol) and `content could not be serialized as JSON: …` (a cyclic value or a
//! BigInt). Neither is reachable from a JSON wire: a [`serde_json::Value`] is
//! never a function, a symbol, a cycle or a BigInt, so [`content_to_text`] is
//! total and the two sentences have no arm.
//!
//! v4's "job-child safe" line is a Node worker-process concern (the child
//! buffers the doc-edit write path's DB writes over IPC). The Rust write path
//! is the single writer's closure itself, so nothing is carried.
//!
//! # Where it runs
//!
//! [`execute_edit_custom_tool`] is SYNCHRONOUS over the two writer connections —
//! the shape every doc-edit handler has, because [`handle_write_file`] mutates
//! inside the writer's closure. The Librarian's announcement it threads out is
//! posted by the async caller after the closure returns (the `run_doc_edit`
//! precedent), so a custom-tool save announces exactly as `doc_write_file` does.

use rusqlite::Connection;
use serde::Serialize;
use serde_json::{json, Value};

use crate::db::tiered_mount_pool::MountTier;
use crate::doc_edit::path_resolver::resolve_doc_edit_path;
use crate::doc_edit::uri_producers::uri_for_resolved_path;
use crate::pascal::custom_tool_schema::{
    CUSTOM_TOOL_SPEC_DICE_JSON, CUSTOM_TOOL_SPEC_JSON, QTAP_CUSTOM_TOOL_SCHEMA_JSON,
};
use crate::pascal::custom_tool_types::TOOLS_FOLDER;
use crate::pascal::js_value::{json_stringify, json_stringify_pretty};
use crate::pascal::roster::{
    load_tools_from_mount, tool_definition_path_for, validate_tool_definition_text,
    ToolDefinitionStage, ToolDefinitionValidation, ValidationWhere,
};
use crate::tools::doc_edit::shared::{
    assert_character_may_read, build_read_resolution_context, build_write_resolution_context,
    read_file_with_mtime, resolve_error_message, Addressing, DocEditToolContext,
};
use crate::tools::doc_edit::text::handle_write_file;
use crate::tools::doc_edit::PendingLibrarianAnnouncement;

/// The `edit_custom_tool` tool's wire name.
pub const EDIT_CUSTOM_TOOL_NAME: &str = "edit_custom_tool";

// ===========================================================================
// Input schema + validation (v4 `editCustomToolInputSchema` /
// `validateEditCustomToolInput`)
// ===========================================================================

/// v4's `action: z.enum(['schema'])`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditCustomToolAction {
    Schema,
}

/// The validated `edit_custom_tool` input, after unknown keys are stripped.
#[derive(Debug, Clone, PartialEq)]
pub struct EditCustomToolInput {
    pub action: Option<EditCustomToolAction>,
    pub mount_point: Option<String>,
    pub file: Option<String>,
    /// `z.unknown().optional()`: ABSENT is `None`; a JSON `null` is PRESENT
    /// (`Some(Value::Null)`) — the handler treats it as omitted, exactly as v4's
    /// `checkOnly` does, but the parse keeps the distinction.
    pub content: Option<Value>,
}

/// The sentence both the Zod `.refine` and the handler's narrowing guard carry.
const REQUIRED_UNLESS_SCHEMA: &str =
    "Invalid input: `mount_point` and `file` are required unless `action` is \"schema\".";

/// Validate input the way v4's `z.object({...}).refine(...).safeParse` does:
/// an object (the schema is not strict — unknown keys are stripped);
/// `action` the string `"schema"` if present; `mount_point` / `file` non-empty
/// strings if present; `content` anything; then the refine — `action` is
/// `"schema"` or both store fields are present. `None` when the shape does not
/// parse.
pub fn validate_edit_custom_tool_input(input: &Value) -> Option<EditCustomToolInput> {
    let obj = input.as_object()?;
    let action = match obj.get("action") {
        None => None,
        Some(Value::String(s)) if s == "schema" => Some(EditCustomToolAction::Schema),
        Some(_) => return None,
    };
    let min1 = |key: &str| -> Result<Option<String>, ()> {
        match obj.get(key) {
            None => Ok(None),
            Some(Value::String(s)) if !s.is_empty() => Ok(Some(s.clone())),
            Some(_) => Err(()),
        }
    };
    let mount_point = min1("mount_point").ok()?;
    let file = min1("file").ok()?;
    let content = obj.get("content").cloned();
    if action != Some(EditCustomToolAction::Schema) && (mount_point.is_none() || file.is_none()) {
        return None;
    }
    Some(EditCustomToolInput {
        action,
        mount_point,
        file,
        content,
    })
}

// ===========================================================================
// Output (v4 `EditCustomToolOutput`) + the model-facing formatter
// ===========================================================================

/// v4's `action?: 'saved' | 'validated' | 'schema'`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum EditCustomToolOutcome {
    Saved,
    Validated,
    Schema,
}

/// What the handler hands the executor. Field order is v4's literal order
/// (`success, action, path, uri, name, schema, examples, error`); an absent
/// member is omitted (`undefined`), so a failure serialises as
/// `{"success":false,"path":…,"error":…}`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EditCustomToolOutput {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<EditCustomToolOutcome>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub examples: Option<Vec<Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// v4 `failure(error, path?)` — `{ success: false, ...(path ? { path } : {}),
/// error }`. An empty `path` is falsy in JS and is omitted too.
pub fn failure(error: impl Into<String>, path: Option<&str>) -> EditCustomToolOutput {
    EditCustomToolOutput {
        success: false,
        action: None,
        path: path.filter(|p| !p.is_empty()).map(str::to_string),
        uri: None,
        name: None,
        schema: None,
        examples: None,
        error: Some(error.into()),
    }
}

/// Format an `edit_custom_tool` result for the model's context (v4
/// `formatEditCustomToolResults`).
pub fn format_edit_custom_tool_results(output: &EditCustomToolOutput) -> String {
    if !output.success {
        let error = output.error.as_deref().filter(|e| !e.is_empty());
        return format!(
            "Custom tool edit error: {}",
            error.unwrap_or("Unknown error")
        );
    }
    if output.action == Some(EditCustomToolOutcome::Schema) {
        return [
            "The custom-tool definition format: `schema` is the JSON Schema every Tools/<name>.tool.json file must satisfy, and `examples` are reference definitions that pass it (between them they exercise every feature, so read their descriptions and messages as commentary).",
            "`roll` is either a range object or a dice string, never both. Write the definition, then save it with edit_custom_tool (`mount_point`, `file`, `content`); a failed save says exactly what to fix.",
        ]
        .join(" ");
    }
    let where_ = output
        .uri
        .as_deref()
        .or(output.path.as_deref())
        .unwrap_or("undefined");
    let name = output.name.as_deref().unwrap_or("undefined");
    if output.action == Some(EditCustomToolOutcome::Saved) {
        format!("Saved \"{name}\" to {where_}. It passed schema validation.")
    } else {
        format!("{where_} is a valid custom tool definition (\"{name}\").")
    }
}

// ===========================================================================
// Handler
// ===========================================================================

/// The validation verdict, worded for the model (v4 `describeInvalid`).
fn describe_invalid(path: &str, stage: ToolDefinitionStage, reason: &str) -> String {
    match stage {
        ToolDefinitionStage::Json => format!("{path} {reason}"),
        ToolDefinitionStage::Schema => format!("{path} fails the custom-tool schema: {reason}"),
    }
}

/// Another definition in the same store that already claims `name`, or `None`.
/// Read through discovery itself, so "clash" means what the roster means by it —
/// including a file that discovery has already set aside as a duplicate.
///
/// v4 filters `stage === 'duplicate' && name !== undefined`; v5's
/// `CustomToolLoadError` carries no `stage` and sets `name` ONLY on the
/// duplicate entry, so `name.is_some()` is the same predicate. The tier is a
/// label only (v4 passes `'global'`).
fn find_same_store_clash(
    mount: &Connection,
    mount_point_id: &str,
    definition_path: &str,
    name: &str,
) -> Option<String> {
    let (found, errors) = load_tools_from_mount(mount, mount_point_id, MountTier::Global);
    let own = definition_path.to_lowercase();
    let claims = found
        .into_iter()
        .map(|entry| (entry.definition_path, entry.definition.name))
        .chain(
            errors
                .into_iter()
                .filter_map(|e| e.name.map(|n| (e.definition_path, n))),
        );
    claims
        .into_iter()
        .find(|(path, claimed)| claimed == name && path.to_lowercase() != own)
        .map(|(path, _)| path)
}

/// The content a caller handed over, as the exact text that would be saved:
/// a string VERBATIM, anything else through `JSON.stringify(content, null, 2)`.
fn content_to_text(content: &Value) -> String {
    match content {
        Value::String(s) => s.clone(),
        other => json_stringify_pretty(other),
    }
}

/// One handler log line in v4's field order: `context`, `chatId`,
/// `characterId` (OMITTED when absent — v4's `undefined`), then the line's own.
macro_rules! log_with_character {
    ($level:ident, $ctx:expr, $message:literal $(, $($field:tt)*)?) => {
        match $ctx.character_id.as_deref() {
            Some(c) => tracing::$level!(
                target: "quilltap::tools::edit_custom_tool",
                context = "edit-custom-tool-handler",
                chatId = %$ctx.chat_id,
                characterId = %c,
                $($($field)*,)?
                $message
            ),
            None => tracing::$level!(
                target: "quilltap::tools::edit_custom_tool",
                context = "edit-custom-tool-handler",
                chatId = %$ctx.chat_id,
                $($($field)*,)?
                $message
            ),
        }
    };
}

/// Execute `edit_custom_tool` (v4 `executeEditCustomTool`).
///
/// Returns the output AND the Librarian announcement the save wants posted — the
/// caller posts it after the writer closure returns.
pub fn execute_edit_custom_tool(
    main: &Connection,
    mount: &Connection,
    input: &Value,
    ctx: &DocEditToolContext,
) -> (EditCustomToolOutput, Option<PendingLibrarianAnnouncement>) {
    let Some(parsed) = validate_edit_custom_tool_input(input) else {
        tracing::warn!(
            target: "quilltap::tools::edit_custom_tool",
            context = "edit-custom-tool-handler",
            chatId = %ctx.chat_id,
            inputJson = %json_stringify(input),
            "edit_custom_tool validation failed",
        );
        return (failure(REQUIRED_UNLESS_SCHEMA, None), None);
    };

    if parsed.action == Some(EditCustomToolAction::Schema) {
        log_with_character!(debug, ctx, "edit_custom_tool: schema requested");
        return (schema_output(), None);
    }

    // The input schema's refine already demands both; this narrows the types.
    let (Some(mount_point), Some(file)) = (parsed.mount_point.as_deref(), parsed.file.as_deref())
    else {
        return (failure(REQUIRED_UNLESS_SCHEMA, None), None);
    };
    let Some(definition_path) = tool_definition_path_for(file) else {
        return (
            failure(
                format!(
                    "\"{file}\" is not a custom-tool file name. Give the bare name (\"unlock\") or the file name (\"unlock.tool.json\"); definitions live directly inside the store's top-level {TOOLS_FOLDER}/ folder, never in a subfolder."
                ),
                None,
            ),
            None,
        );
    };

    // `null` is how some models spell "omitted".
    let check_only = matches!(parsed.content, None | Some(Value::Null));

    log_with_character!(
        debug,
        ctx,
        "edit_custom_tool called",
        mountPoint = %mount_point,
        definitionPath = %definition_path,
        checkOnly = check_only
    );

    let result = if check_only {
        check_arm(main, mount, ctx, mount_point, &definition_path)
    } else {
        // `check_only` is false, so `content` is present and not null.
        let content = parsed.content.as_ref().unwrap_or(&Value::Null);
        save_arm(main, mount, ctx, mount_point, &definition_path, content)
    };

    match result {
        Ok(done) => done,
        Err(error) => {
            tracing::warn!(
                target: "quilltap::tools::edit_custom_tool",
                context = "edit-custom-tool-handler",
                chatId = %ctx.chat_id,
                mountPoint = %mount_point,
                definitionPath = %definition_path,
                error = %error,
                "edit_custom_tool failed",
            );
            (failure(error, Some(&definition_path)), None)
        }
    }
}

/// `action: "schema"` — touches no store.
fn schema_output() -> EditCustomToolOutput {
    // The embeds are byte-guarded to parse (`custom_tool_schema_embed_guard`);
    // preserve-order serde keeps v4's key order through the round trip.
    let parse = |text: &str| serde_json::from_str::<Value>(text).unwrap_or(Value::Null);
    EditCustomToolOutput {
        success: true,
        action: Some(EditCustomToolOutcome::Schema),
        path: None,
        uri: None,
        name: None,
        schema: Some(parse(QTAP_CUSTOM_TOOL_SCHEMA_JSON)),
        examples: Some(vec![
            parse(CUSTOM_TOOL_SPEC_JSON),
            parse(CUSTOM_TOOL_SPEC_DICE_JSON),
        ]),
        error: None,
    }
}

type ArmResult = Result<(EditCustomToolOutput, Option<PendingLibrarianAnnouncement>), String>;

fn store_addressing(mount_point: &str, definition_path: &str) -> Addressing {
    Addressing {
        scope: Some("document_store".into()),
        mount_point: Some(mount_point.to_string()),
        path: Some(definition_path.to_string()),
        uri_heading: None,
        uri_level: None,
    }
}

/// No `content`: validate the file as it stands, writing nothing.
fn check_arm(
    main: &Connection,
    mount: &Connection,
    ctx: &DocEditToolContext,
    mount_point: &str,
    definition_path: &str,
) -> ArmResult {
    use crate::doc_edit::DocEditScope;

    let addressing = store_addressing(mount_point, definition_path);
    let read_context = build_read_resolution_context(main, &addressing, ctx);
    let resolved = resolve_doc_edit_path(
        main,
        mount,
        DocEditScope::DocumentStore,
        Some(definition_path),
        &read_context,
        ctx.files_dir.as_deref(),
    )
    .map_err(resolve_error_message)?;
    assert_character_may_read(mount, &resolved, ctx)?;

    let text = match read_file_with_mtime(mount, &resolved) {
        Ok(doc) => doc.content,
        // v4 tests `error.code === 'ENOENT'`; v5's error type on this seam IS the
        // string (the database arm renders `ENOENT: database document not found
        // at …`, the fs arm `ENOENT: no such file or directory, …`).
        Err(e) if e.starts_with("ENOENT") => {
            return Ok((
                failure(
                    format!("There is no {definition_path} in \"{mount_point}\" to validate."),
                    Some(definition_path),
                ),
                None,
            ))
        }
        Err(e) => return Err(e),
    };

    let mount_point_id = resolved
        .mount_point_id
        .clone()
        .ok_or("The resolved path carries no mount point.")?;
    let definition = match validate_tool_definition_text(
        &text,
        &ValidationWhere {
            mount_point_id: Some(mount_point_id.clone()),
            definition_path: Some(definition_path.to_string()),
        },
    ) {
        ToolDefinitionValidation::Ok { definition } => *definition,
        ToolDefinitionValidation::Invalid { stage, reason } => {
            tracing::debug!(
                target: "quilltap::tools::edit_custom_tool",
                context = "edit-custom-tool-handler",
                definitionPath = %definition_path,
                stage = stage.as_str(),
                "edit_custom_tool: existing definition is invalid",
            );
            return Ok((
                failure(
                    describe_invalid(definition_path, stage, &reason),
                    Some(definition_path),
                ),
                None,
            ));
        }
    };
    if let Some(clash) =
        find_same_store_clash(mount, &mount_point_id, definition_path, &definition.name)
    {
        return Ok((
            failure(
                format!(
                    "{definition_path} is valid on its own, but {clash} in the same store also defines \"{}\", so the roster loads only one of them. Rename one.",
                    definition.name
                ),
                Some(definition_path),
            ),
            None,
        ));
    }

    Ok((
        EditCustomToolOutput {
            success: true,
            action: Some(EditCustomToolOutcome::Validated),
            path: Some(definition_path.to_string()),
            uri: Some(uri_for_resolved_path(mount, &resolved, None, None)),
            name: Some(definition.name),
            schema: None,
            examples: None,
            error: None,
        },
        None,
    ))
}

/// With `content`: validate, refuse a clash, then write through
/// `doc_write_file`'s handler.
fn save_arm(
    main: &Connection,
    mount: &Connection,
    ctx: &DocEditToolContext,
    mount_point: &str,
    definition_path: &str,
    content: &Value,
) -> ArmResult {
    use crate::doc_edit::DocEditScope;

    let text = content_to_text(content);
    // No `mountPointId` on this call (v4 `:188`) — nothing is resolved yet.
    let definition = match validate_tool_definition_text(
        &text,
        &ValidationWhere {
            mount_point_id: None,
            definition_path: Some(definition_path.to_string()),
        },
    ) {
        ToolDefinitionValidation::Ok { definition } => *definition,
        ToolDefinitionValidation::Invalid { stage, reason } => {
            tracing::debug!(
                target: "quilltap::tools::edit_custom_tool",
                context = "edit-custom-tool-handler",
                definitionPath = %definition_path,
                stage = stage.as_str(),
                "edit_custom_tool: refused an invalid definition",
            );
            return Ok((
                failure(
                    format!(
                        "Not saved. {}",
                        describe_invalid(definition_path, stage, &reason)
                    ),
                    Some(definition_path),
                ),
                None,
            ));
        }
    };

    // Resolve through the write context before reading the store's roster, so a
    // store the caller may not write to fails as a write would.
    let addressing = store_addressing(mount_point, definition_path);
    let write_context = build_write_resolution_context(main, mount, &addressing, ctx)?;
    let resolved = resolve_doc_edit_path(
        main,
        mount,
        DocEditScope::DocumentStore,
        Some(definition_path),
        &write_context,
        ctx.files_dir.as_deref(),
    )
    .map_err(resolve_error_message)?;
    let mount_point_id = resolved
        .mount_point_id
        .clone()
        .ok_or("The resolved path carries no mount point.")?;
    if let Some(clash) =
        find_same_store_clash(mount, &mount_point_id, definition_path, &definition.name)
    {
        return Ok((
            failure(
                format!(
                    "Not saved. {clash} in \"{mount_point}\" already defines \"{}\", and one store may not hold two definitions of the same name. Edit that file instead, or choose another name.",
                    definition.name
                ),
                Some(definition_path),
            ),
            None,
        ));
    }

    let mut written = handle_write_file(
        main,
        mount,
        &json!({
            "scope": "document_store",
            "mount_point": mount_point,
            "path": definition_path,
            "content": text,
        }),
        ctx,
    )?;
    if !written.success {
        let reason = written.error.take();
        return Ok((
            failure(
                format!(
                    "Not saved. {}",
                    reason.as_deref().unwrap_or("The write failed.")
                ),
                Some(definition_path),
            ),
            None,
        ));
    }

    log_with_character!(
        info,
        ctx,
        "Custom tool definition saved",
        mountPointId = %mount_point_id,
        definitionPath = %definition_path,
        name = %definition.name
    );

    let uri = written
        .result
        .as_ref()
        .and_then(|r| r.get("uri"))
        .and_then(Value::as_str)
        .map(str::to_string);
    Ok((
        EditCustomToolOutput {
            success: true,
            action: Some(EditCustomToolOutcome::Saved),
            path: Some(definition_path.to_string()),
            uri,
            name: Some(definition.name),
            schema: None,
            examples: None,
            error: None,
        },
        written.pending_librarian_announcement.take(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// v4's `failure` key order: `success, path?, error` — pinned by bytes.
    #[test]
    fn failure_serialises_in_v4_key_order() {
        assert_eq!(
            serde_json::to_string(&failure("x", Some("p"))).unwrap(),
            r#"{"success":false,"path":"p","error":"x"}"#
        );
        assert_eq!(
            serde_json::to_string(&failure("x", None)).unwrap(),
            r#"{"success":false,"error":"x"}"#
        );
        // An empty path is falsy in JS.
        assert_eq!(
            serde_json::to_string(&failure("x", Some(""))).unwrap(),
            r#"{"success":false,"error":"x"}"#
        );
    }

    #[test]
    fn input_validation_matches_the_zod_object() {
        let ok = |v: Value| validate_edit_custom_tool_input(&v);
        assert!(ok(json!({ "action": "schema" })).is_some());
        // The refine.
        assert!(ok(json!({ "file": "unlock" })).is_none());
        assert!(ok(json!({ "mount_point": "V" })).is_none());
        // `min(1)`.
        assert!(ok(json!({ "mount_point": "", "file": "x" })).is_none());
        // The enum.
        assert!(ok(json!({ "action": "save", "mount_point": "V", "file": "x" })).is_none());
        assert!(ok(json!({ "action": null, "mount_point": "V", "file": "x" })).is_none());
        // `null` content is PRESENT.
        let with_null = ok(json!({ "mount_point": "V", "file": "x", "content": null })).unwrap();
        assert_eq!(with_null.content, Some(Value::Null));
        // Unknown keys are stripped, not refused.
        assert!(ok(json!({ "mount_point": "V", "file": "x", "extra": 1 })).is_some());
        // The `schema` action ignores every store field.
        assert!(
            ok(json!({ "action": "schema", "mount_point": "V", "file": "x", "content": {} }))
                .is_some()
        );
        // Not an object.
        assert!(ok(json!("schema")).is_none());
    }

    #[test]
    fn content_text_is_verbatim_for_strings_and_pretty_otherwise() {
        assert_eq!(content_to_text(&json!("raw {")), "raw {");
        assert_eq!(
            content_to_text(&json!({ "a": [], "b": {}, "c": [1, 2.5] })),
            "{\n  \"a\": [],\n  \"b\": {},\n  \"c\": [\n    1,\n    2.5\n  ]\n}"
        );
    }

    #[test]
    fn the_schema_action_returns_the_vendored_bytes_in_order() {
        let out = schema_output();
        assert_eq!(out.action, Some(EditCustomToolOutcome::Schema));
        let examples = out.examples.unwrap();
        assert_eq!(examples.len(), 2);
        assert_eq!(
            out.schema.unwrap()["$id"],
            "https://quilltap.ai/schemas/qtap-custom-tool.schema.json"
        );
    }

    #[test]
    fn formatter_shapes() {
        assert_eq!(
            format_edit_custom_tool_results(&failure("", None)),
            "Custom tool edit error: Unknown error"
        );
        let saved = EditCustomToolOutput {
            success: true,
            action: Some(EditCustomToolOutcome::Saved),
            path: Some("Tools/u.tool.json".into()),
            uri: Some("qtap://S/Tools/u.tool.json".into()),
            name: Some("u".into()),
            schema: None,
            examples: None,
            error: None,
        };
        assert_eq!(
            format_edit_custom_tool_results(&saved),
            "Saved \"u\" to qtap://S/Tools/u.tool.json. It passed schema validation."
        );
        let validated = EditCustomToolOutput {
            action: Some(EditCustomToolOutcome::Validated),
            uri: None,
            ..saved
        };
        assert_eq!(
            format_edit_custom_tool_results(&validated),
            "Tools/u.tool.json is a valid custom tool definition (\"u\")."
        );
    }
}
