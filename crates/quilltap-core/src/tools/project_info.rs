//! Port of v4's `project_info` tool
//! (`lib/tools/handlers/project-info-handler.ts` + `lib/tools/project-info-tool.ts`).
//!
//! Two actions over the current chat's project: `get_info` (overview, roster,
//! item counts, and the linked Scriptorium document-store summary) and
//! `get_instructions` (the full project instructions). Read-only.
//!
//! The store summary composes the ported [`pick_primary_project_store`] leaf
//! ([`crate::db::project_store_naming`]) over the project's linked mount points.

use serde::Serialize;
use serde_json::{json, Map, Value};

use crate::db::project_store_naming::{pick_primary_project_store, StoreLike};
use crate::db::runtime::Db;
use crate::db::{
    characters_read, chats_read, doc_mount_blobs, doc_mount_files, doc_mount_points, files,
    memories_read, project_doc_mount_links, projects, DbError,
};

/// v4 `ProjectInfoToolContext`.
pub struct ProjectInfoContext {
    pub user_id: String,
    pub project_id: String,
    #[allow(dead_code)]
    pub embedding_profile_id: Option<String>,
}

/// v4 `ProjectInfoToolOutput` — `{ success, action, data?, error? }`. `data` is the
/// `ProjectInfoResult` / `ProjectInstructionsResult` built in v4 field order (a
/// `Value`); the optionals are dropped when absent (v4 omits `data` on failure and
/// `error` on success).
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ProjectInfoOutput {
    pub success: bool,
    pub action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl ProjectInfoOutput {
    fn err(action: &str, message: impl Into<String>) -> Self {
        ProjectInfoOutput {
            success: false,
            action: action.to_string(),
            data: None,
            error: Some(message.into()),
        }
    }
    fn ok(action: &str, data: Value) -> Self {
        ProjectInfoOutput {
            success: true,
            action: action.to_string(),
            data: Some(data),
            error: None,
        }
    }
}

/// v4 `validateProjectInfoInput` — `{ action: z.enum(['get_info','get_instructions']) }`
/// `safeParse`. Valid iff `args` is an object whose `action` is one of the two.
fn validate_action(args: &Value) -> Option<&str> {
    match args.get("action").and_then(Value::as_str) {
        Some(a @ ("get_info" | "get_instructions")) => Some(a),
        _ => None,
    }
}

/// Execute the `project_info` tool (v4 `executeProjectInfoTool`).
pub fn execute_project_info(
    db: &Db,
    context: &ProjectInfoContext,
    args: &Value,
) -> ProjectInfoOutput {
    let Some(action) = validate_action(args) else {
        // v4's validation-fail path fixes `action: 'get_info'`.
        return ProjectInfoOutput::err(
            "get_info",
            "Invalid input: action is required and must be a valid action type",
        );
    };

    // The outer try/catch: any read error → failure with the error message and the
    // action resolved from the input (else 'get_info', matching v4's catch).
    let run = db.read_main(|main| {
        db.read_mount_index(|mount| match action {
            "get_info" => execute_get_info(main, mount, context),
            _ => execute_get_instructions(main, mount, context),
        })
    });

    match run {
        Ok(out) => out,
        Err(e) => {
            let action = args
                .get("action")
                .and_then(Value::as_str)
                .unwrap_or("get_info");
            ProjectInfoOutput::err(action, e.to_string())
        }
    }
}

/// v4 `executeGetInfo`.
fn execute_get_info(
    main: &rusqlite::Connection,
    mount: &rusqlite::Connection,
    context: &ProjectInfoContext,
) -> Result<ProjectInfoOutput, DbError> {
    let project = match projects::ProjectsRepository::new(main, mount)
        .find_by_id(&context.project_id)
        .map_err(overlay_to_db)?
    {
        Some(p) => p,
        None => return Ok(ProjectInfoOutput::err("get_info", "Project not found")),
    };

    // Character roster details — resolve each id, keep the ones that exist.
    let mut roster: Vec<Value> = Vec::new();
    if let Some(ids) = project.get("characterRoster").and_then(Value::as_array) {
        for id in ids.iter().filter_map(Value::as_str) {
            if let Some(ch) = characters_read::find_by_id(main, mount, id)? {
                roster.push(json!({
                    "id": ch.get("id").cloned().unwrap_or(Value::Null),
                    "name": ch.get("name").cloned().unwrap_or(Value::Null),
                }));
            }
        }
    }

    // Counts (findAll().filter(projectId === …).length; files via a scoped COUNT).
    let chat_count = count_by_project(&chats_read::find_all(main)?, &context.project_id);
    let file_count = files::FilesRepository::new(main).count_by_project_id(&context.project_id)?;
    let memory_count = count_by_project(&memories_read::find_all(main)?, &context.project_id);

    let document_store = resolve_project_document_store(mount, &context.project_id);

    // v4 field order: id, name, description, allowAnyCharacter, characterRoster,
    // fileCount, chatCount, memoryCount, documentStore.
    let mut data = Map::new();
    data.insert(
        "id".into(),
        project.get("id").cloned().unwrap_or(Value::Null),
    );
    data.insert(
        "name".into(),
        project.get("name").cloned().unwrap_or(Value::Null),
    );
    // `description` is set from `project.description`; an overlay that omits a null
    // description leaves the key absent (v4 `undefined` → dropped by JSON.stringify).
    if let Some(desc) = project.get("description") {
        data.insert("description".into(), desc.clone());
    }
    data.insert(
        "allowAnyCharacter".into(),
        project
            .get("allowAnyCharacter")
            .cloned()
            .unwrap_or(Value::Bool(false)),
    );
    data.insert("characterRoster".into(), Value::Array(roster));
    data.insert("fileCount".into(), Value::from(file_count));
    data.insert("chatCount".into(), Value::from(chat_count));
    data.insert("memoryCount".into(), Value::from(memory_count));
    data.insert(
        "documentStore".into(),
        document_store.unwrap_or(Value::Null),
    );

    Ok(ProjectInfoOutput::ok("get_info", Value::Object(data)))
}

/// v4 `executeGetInstructions`.
fn execute_get_instructions(
    main: &rusqlite::Connection,
    mount: &rusqlite::Connection,
    context: &ProjectInfoContext,
) -> Result<ProjectInfoOutput, DbError> {
    let project = match projects::ProjectsRepository::new(main, mount)
        .find_by_id(&context.project_id)
        .map_err(overlay_to_db)?
    {
        Some(p) => p,
        None => {
            return Ok(ProjectInfoOutput::err(
                "get_instructions",
                "Project not found",
            ))
        }
    };

    // `project.instructions || null`; `hasInstructions = !!project.instructions`.
    // A truthy instructions value is a non-empty string.
    let instructions = project
        .get("instructions")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty());
    let data = json!({
        "instructions": instructions,
        "hasInstructions": instructions.is_some(),
    });
    Ok(ProjectInfoOutput::ok("get_instructions", data))
}

/// v4 `resolveProjectDocumentStore` — the linked Scriptorium store summary, or
/// `None`. The whole body is `try/catch → null` in v4, so any read error yields
/// `None` (the rest of the info stays usable).
fn resolve_project_document_store(mount: &rusqlite::Connection, project_id: &str) -> Option<Value> {
    resolve_project_document_store_inner(mount, project_id)
        .ok()
        .flatten()
}

fn resolve_project_document_store_inner(
    mount: &rusqlite::Connection,
    project_id: &str,
) -> Result<Option<Value>, DbError> {
    let mount_point_ids = project_doc_mount_links::ProjectDocMountLinksRepository::new(mount)
        .find_by_project_id(project_id)?;
    if mount_point_ids.is_empty() {
        return Ok(None);
    }

    let points = doc_mount_points::DocMountPointsRepository::new(mount);
    let mut stores: Vec<StoreLike> = Vec::new();
    for id in &mount_point_ids {
        if let Some(store) = points.find_store_naming_by_id(id)? {
            stores.push(store);
        }
    }

    let Some(chosen) = pick_primary_project_store(&stores) else {
        return Ok(None);
    };

    let file_count = doc_mount_files::DocMountFilesRepository::new(mount)
        .find_vault_files_by_mount_point_id(&chosen.id)?
        .len() as i64;
    let blob_count =
        doc_mount_blobs::DocMountBlobsRepository::new(mount).count_by_mount_point(&chosen.id)?;

    // v4 field order: mountPointId, name, storeType, fileCount, blobCount.
    // `storeType` is the hydrated DocMountPoint value (schema-defaulted to
    // 'documents' when the column is unset).
    Ok(Some(json!({
        "mountPointId": chosen.id,
        "name": chosen.name,
        "storeType": chosen.store_type.clone().unwrap_or_else(|| "documents".to_string()),
        "fileCount": file_count,
        "blobCount": blob_count,
    })))
}

/// Count rows whose `projectId` (omitted when null in the read marshaling) equals
/// `project_id` — the `findAll().filter(x.projectId === projectId).length` shape.
fn count_by_project(rows: &[Value], project_id: &str) -> i64 {
    rows.iter()
        .filter(|r| r.get("projectId").and_then(Value::as_str) == Some(project_id))
        .count() as i64
}

/// Map the projects overlay error into a [`DbError`] for the shared read path (an
/// overlay failure surfaces to v4's outer catch → failure result).
fn overlay_to_db(e: crate::db::document_store_overlay::OverlayError) -> DbError {
    DbError::Internal(format!("projects overlay: {e:?}"))
}

/// v4 `formatProjectInfoResults`.
pub fn format_project_info(out: &ProjectInfoOutput) -> String {
    if !out.success {
        return format!(
            "Project Info Error: {}",
            out.error.as_deref().unwrap_or("Unknown error")
        );
    }
    let data = out.data.as_ref();
    match out.action.as_str() {
        "get_info" => {
            let d = data.cloned().unwrap_or(Value::Null);
            let mut parts: Vec<String> = Vec::new();
            let name = d.get("name").and_then(Value::as_str).unwrap_or("");
            parts.push(format!("Project: {name}"));
            if let Some(desc) = d.get("description").and_then(Value::as_str) {
                // v4: `info.description ? ... : null` — only a truthy (non-empty)
                // description contributes a line.
                if !desc.is_empty() {
                    parts.push(format!("Description: {desc}"));
                }
            }
            let file_count = d.get("fileCount").and_then(Value::as_i64).unwrap_or(0);
            let chat_count = d.get("chatCount").and_then(Value::as_i64).unwrap_or(0);
            let memory_count = d.get("memoryCount").and_then(Value::as_i64).unwrap_or(0);
            parts.push(format!(
                "Files: {file_count}, Chats: {chat_count}, Memories: {memory_count}"
            ));
            let roster = d
                .get("characterRoster")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            if !roster.is_empty() {
                let names: Vec<&str> = roster
                    .iter()
                    .map(|c| c.get("name").and_then(Value::as_str).unwrap_or(""))
                    .collect();
                parts.push(format!("Characters: {}", names.join(", ")));
            } else {
                parts.push("No characters in roster".to_string());
            }
            if d.get("allowAnyCharacter").and_then(Value::as_bool) == Some(true) {
                parts.push("(Any character can participate)".to_string());
            }
            match d.get("documentStore") {
                Some(ds) if !ds.is_null() => {
                    let name = ds.get("name").and_then(Value::as_str).unwrap_or("");
                    let fc = ds.get("fileCount").and_then(Value::as_i64).unwrap_or(0);
                    let bc = ds.get("blobCount").and_then(Value::as_i64).unwrap_or(0);
                    let st = ds.get("storeType").and_then(Value::as_str).unwrap_or("");
                    parts.push(format!(
                        "Document Store: {name} ({fc} files, {bc} blobs, storeType={st})"
                    ));
                }
                _ => parts.push("Document Store: (none linked)".to_string()),
            }
            parts.join("\n")
        }
        "get_instructions" => {
            let d = data.cloned().unwrap_or(Value::Null);
            if d.get("hasInstructions").and_then(Value::as_bool) != Some(true) {
                return "No project instructions defined.".to_string();
            }
            let inst = d.get("instructions").and_then(Value::as_str).unwrap_or("");
            format!("Project Instructions:\n{inst}")
        }
        _ => "Unknown action result".to_string(),
    }
}
