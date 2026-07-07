//! Writer for Prospero announcements — v4 `lib/services/prospero-notifications/
//! writer.ts`.
//!
//! Prospero is the master of the agentic and tool-using systems — the personified
//! feature that knows which LLM is presently driving each character. Prospero
//! injects synthetic ASSISTANT-role chat messages (`systemSender: 'prospero'`) so
//! the user and any system-transparent characters in the room learn of a
//! connection-profile reassignment, the project/general context re-injection, a
//! per-character group/vault context whisper, or a Carina query failure.
//!
//! Errors never propagate — an announcement that could not be written is logged
//! (host-side logging is out of scope for the port) and surfaced to the caller as
//! `None` rather than thrown. Each `post*` builds the visible + opaque body,
//! assembles the exact v4 `MessageEvent` object literal, validates it into a
//! [`ChatEventInput`] (so the stored row is byte-for-byte the `db::chats_messages`
//! insert marshaling), and writes it through the single-writer [`Db`].
//!
//! ## Structure
//! - The five **pure content builders** (each with a `Content` + `OpaqueContent`
//!   variant) are byte-exact ports, proven by `post_office_prospero_equivalence`.
//! - The four **`post*` functions** mint `id`/`createdAt`, build the object, and
//!   write it best-effort.
//! - The four **DB loaders** compose the ported document-store repos to gather the
//!   context objects the posts consume.

use std::cmp::Ordering;

use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};

use crate::db::chats_messages::ChatEventInput;
use crate::db::runtime::Db;
use crate::db::DbError;
use crate::doc_edit::qtap_uri::{
    format_doc_store_uri, format_scoped_uri, format_self_uri, ScopedAuthority,
};

// Reuse the single-source Carina error taxonomy + the runner's argument struct so
// this writer's `post_prospero_carina_error` is call-compatible with the
// `PostProsperoCarinaError` seam that `services::carina_runner` already declares.
pub use crate::services::carina_runner::{CarinaErrorKind, ProsperoCarinaErrorArgs};

// ===========================================================================
// Shared types (v4 interfaces).
// ===========================================================================

/// v4 `ProsperoConnectionProfileChangeAnnouncement`.
#[derive(Debug, Clone)]
pub struct ProsperoConnectionProfileChangeAnnouncement {
    pub chat_id: String,
    pub character_name: String,
    pub old_profile_label: Option<String>,
    pub new_profile_label: Option<String>,
}

/// v4 `ProsperoDocumentStoreInfo`. `mount_type` / `store_type` are the union
/// string values verbatim (`'filesystem' | 'obsidian' | 'database'` /
/// `'documents' | 'character'`).
#[derive(Debug, Clone)]
pub struct ProsperoDocumentStoreInfo {
    pub id: String,
    pub name: String,
    pub mount_type: String,
    pub store_type: String,
    pub is_official: bool,
    pub enabled: bool,
}

/// v4 `ProsperoProjectContext`.
#[derive(Debug, Clone)]
pub struct ProsperoProjectContext {
    pub name: String,
    pub description: Option<String>,
    pub instructions: Option<String>,
    /// Document stores linked to the project (official store first, then
    /// alphabetical — the loader sorts).
    pub document_stores: Vec<ProsperoDocumentStoreInfo>,
}

/// v4 `ProsperoGeneralContext`.
#[derive(Debug, Clone)]
pub struct ProsperoGeneralContext {
    pub mount_point_id: String,
    pub name: String,
    pub mount_type: String,
}

/// v4 `ProsperoContextAnnouncement`.
#[derive(Debug, Clone)]
pub struct ProsperoContextAnnouncement {
    pub chat_id: String,
    pub project: Option<ProsperoProjectContext>,
    pub general: Option<ProsperoGeneralContext>,
}

/// v4 `ProsperoGroupContextWhisper`.
#[derive(Debug, Clone)]
pub struct ProsperoGroupContextWhisper {
    pub chat_id: String,
    /// Participant id of the character the whisper is addressed to.
    pub target_participant_id: String,
    /// Character whose group memberships + vault are resolved.
    pub character_id: String,
}

// ===========================================================================
// Connection-profile change (public).
// ===========================================================================

/// v4 `buildConnectionProfileChangeContent`. `newProfileLabel ?? 'unassigned'`
/// is NULLISH coalescing — only `None` becomes `unassigned`; an empty string
/// stays empty.
pub fn build_connection_profile_change_content(
    character_name: &str,
    old_profile_label: Option<&str>,
    new_profile_label: Option<&str>,
) -> String {
    let new_phrase = new_profile_label.unwrap_or("unassigned");
    let old_phrase = old_profile_label.unwrap_or("unassigned");
    format!(
        "{character_name}'s current response model is now {new_phrase}; previous model was {old_phrase}."
    )
}

/// v4 `buildConnectionProfileChangeOpaqueContent` — delegates to the visible
/// form (a profile change carries no persona names, so the two can't drift).
pub fn build_connection_profile_change_opaque_content(
    character_name: &str,
    old_profile_label: Option<&str>,
    new_profile_label: Option<&str>,
) -> String {
    build_connection_profile_change_content(character_name, old_profile_label, new_profile_label)
}

/// v4 `postProsperoConnectionProfileChangeAnnouncement` — `systemSender:
/// 'prospero'`, `systemKind: 'connection-profile-change'`, public.
pub async fn post_prospero_connection_profile_change_announcement(
    db: &Db,
    params: ProsperoConnectionProfileChangeAnnouncement,
) -> Option<Value> {
    let content = build_connection_profile_change_content(
        &params.character_name,
        params.old_profile_label.as_deref(),
        params.new_profile_label.as_deref(),
    );
    let opaque = build_connection_profile_change_opaque_content(
        &params.character_name,
        params.old_profile_label.as_deref(),
        params.new_profile_label.as_deref(),
    );
    post_prospero_message(
        db,
        &params.chat_id,
        content,
        Some(opaque),
        "connection-profile-change",
        None,
    )
    .await
}

// ===========================================================================
// Project + general context (public, combined).
// ===========================================================================

/// v4 `generalShelfUri` — the root `qtap://` URI for the household's shared shelf
/// (an ordinary store addressed by name/id, NOT the `general` scope).
fn general_shelf_uri(general: &ProsperoGeneralContext) -> String {
    format_doc_store_uri(
        &general.name,
        &general.mount_point_id,
        "",
        false,
        None,
        None,
    )
}

/// v4 `escape` of a backtick inside an inline-code span: `` name.replace(/`/g, '\\`') ``.
fn escape_backticks(s: &str) -> String {
    s.replace('`', "\\`")
}

/// v4 `buildDocumentStoresSection`. Empty on no stores.
fn build_document_stores_section(stores: &[ProsperoDocumentStoreInfo]) -> Vec<String> {
    if stores.is_empty() {
        return Vec::new();
    }

    // Names duplicated within the linked set fall back to the UUID form so the
    // URI stays unambiguous. Key = `name.trim().toLowerCase()`.
    let name_count = |name: &str| -> usize {
        let key = crate::jsstr::js_trim(name).to_lowercase();
        stores
            .iter()
            .filter(|s| crate::jsstr::js_trim(&s.name).to_lowercase() == key)
            .count()
    };

    let mut lines: Vec<String> = vec![
        "**Document stores linked to this project:**".to_string(),
        String::new(),
    ];
    for store in stores {
        let mut tags: Vec<String> = Vec::new();
        if store.is_official {
            tags.push("official project store".to_string());
        }
        tags.push(format!("{}-backed", store.mount_type));
        if store.store_type == "character" {
            tags.push("character vault".to_string());
        }
        if !store.enabled {
            tags.push("currently disabled".to_string());
        }
        let tag_suffix = format!(" *({})*", tags.join("; "));

        let name_is_ambiguous = name_count(&store.name) > 1;
        let store_uri =
            format_doc_store_uri(&store.name, &store.id, "", name_is_ambiguous, None, None);
        let ref_hint = if store.is_official {
            format!(
                "reachable at `{}` (or `{}`)",
                format_scoped_uri(ScopedAuthority::Project, "", None, None),
                store_uri
            )
        } else {
            format!("reachable at `{store_uri}`")
        };

        lines.push(format!(
            "- **{}**{} — {}.",
            store.name, tag_suffix, ref_hint
        ));
    }
    lines.push(String::new());
    // Literal `{ uri: … }` braces — build by concatenation, not `format!`.
    lines.push(
        "Address a file in any of these with a `qtap://` URI — e.g. ".to_string()
            + "`doc_read_file({ uri: \"qtap://<store>/<relative path>\" })` (or `doc_copy_file` with "
            + "`source_uri` / `dest_uri`). The legacy `mount_point` + `path` arguments still work.",
    );
    lines
}

/// v4 `appendProjectBodySection` — pushes description / instructions / stores in
/// order (blank-line separated). Returns whether any body content was pushed.
fn append_project_body_section(lines: &mut Vec<String>, project: &ProsperoProjectContext) -> bool {
    let description = trimmed_nonempty(project.description.as_deref());
    let instructions = trimmed_nonempty(project.instructions.as_deref());
    let stores = &project.document_stores;

    if let Some(description) = description {
        lines.push("**Project description:**".to_string());
        lines.push(String::new());
        lines.push(description.to_string());
    }
    if let Some(instructions) = instructions {
        if description.is_some() {
            lines.push(String::new());
        }
        lines.push("**Project instructions:**".to_string());
        lines.push(String::new());
        lines.push(instructions.to_string());
    }
    if !stores.is_empty() {
        if description.is_some() || instructions.is_some() {
            lines.push(String::new());
        }
        lines.extend(build_document_stores_section(stores));
    }

    description.is_some() || instructions.is_some() || !stores.is_empty()
}

/// v4 `projectHasContent`.
fn project_has_content(project: Option<&ProsperoProjectContext>) -> bool {
    match project {
        None => false,
        Some(p) => {
            trimmed_nonempty(p.description.as_deref()).is_some()
                || trimmed_nonempty(p.instructions.as_deref()).is_some()
                || !p.document_stores.is_empty()
        }
    }
}

/// v4 `buildGeneralContextContent`.
pub fn build_general_context_content(general: &ProsperoGeneralContext) -> String {
    let safe_name = escape_backticks(&general.name);
    let uri = general_shelf_uri(general);
    [
        format!(
            "Prospero would have you remember that, beyond any particular project or vault, every character in this instance has standing access to the household's shared shelf — **{}** — at all times.",
            general.name
        ),
        String::new(),
        format!(
            "Reach it with a `qtap://` URI — `{uri}` (pass it as the `uri` arg to any `doc_*` tool); the `mount_point: \"{safe_name}\"` form and the ID `{id}` still work. Its `Scenarios/` folder holds the general chat-starter scenarios offered alongside project- and character-specific ones; other curated content the household keeps lives here as well.",
            id = general.mount_point_id
        ),
    ]
    .join("\n")
}

/// v4 `buildGeneralContextOpaqueContent`.
pub fn build_general_context_opaque_content(general: &ProsperoGeneralContext) -> String {
    let safe_name = escape_backticks(&general.name);
    let uri = general_shelf_uri(general);
    [
        format!(
            "Beyond any particular project or vault, every character in this instance has standing access to the shared shelf — **{}** — at all times.",
            general.name
        ),
        String::new(),
        format!(
            "Reach it with a `qtap://` URI — `{uri}` (pass it as the `uri` arg to any `doc_*` tool); the `mount_point: \"{safe_name}\"` form and the ID `{id}` still work. Its `Scenarios/` folder holds the general chat-starter scenarios offered alongside project- and character-specific ones; other curated content lives here as well.",
            id = general.mount_point_id
        ),
    ]
    .join("\n")
}

/// v4 `buildCombinedContextContent`.
pub fn build_combined_context_content(
    project: Option<&ProsperoProjectContext>,
    general: Option<&ProsperoGeneralContext>,
) -> String {
    let has_project = project_has_content(project);
    if !has_project {
        if let Some(general) = general {
            return build_general_context_content(general);
        }
    }
    let project = match (has_project, project) {
        (true, Some(p)) => p,
        _ => return String::new(),
    };

    let mut lines: Vec<String> = Vec::new();
    if general.is_some() {
        lines.push(format!(
            "Prospero opens his ledger to the project at hand — *{}* — and lays its particulars before you, with a reminder of the household's shared shelf alongside:",
            project.name
        ));
    } else {
        lines.push(format!(
            "Prospero opens his ledger to the project at hand — *{}* — and lays its particulars before you:",
            project.name
        ));
    }
    lines.push(String::new());

    let has_body = append_project_body_section(&mut lines, project);

    if let Some(general) = general {
        if has_body {
            lines.push(String::new());
        }
        let safe_name = escape_backticks(&general.name);
        lines.push(format!(
            "Beyond this project, every character in this instance has standing access to the household's shared shelf — **{name}** — at all times. Reach it with a `qtap://` URI — `{uri}` (pass it as the `uri` arg to any `doc_*` tool); the `mount_point: \"{safe_name}\"` form and the ID `{id}` still work. Its `Scenarios/` folder holds the general chat-starter scenarios offered alongside project- and character-specific ones; other curated content the household keeps lives here as well.",
            name = general.name,
            uri = general_shelf_uri(general),
            id = general.mount_point_id
        ));
    }

    crate::jsstr::js_trim_end(&lines.join("\n")).to_string()
}

/// v4 `buildCombinedContextOpaqueContent`.
pub fn build_combined_context_opaque_content(
    project: Option<&ProsperoProjectContext>,
    general: Option<&ProsperoGeneralContext>,
) -> String {
    let has_project = project_has_content(project);
    if !has_project {
        if let Some(general) = general {
            return build_general_context_opaque_content(general);
        }
    }
    let project = match (has_project, project) {
        (true, Some(p)) => p,
        _ => return String::new(),
    };

    let mut lines: Vec<String> = vec![
        format!("Project context — *{}*:", project.name),
        String::new(),
    ];
    let has_body = append_project_body_section(&mut lines, project);

    if let Some(general) = general {
        if has_body {
            lines.push(String::new());
        }
        let safe_name = escape_backticks(&general.name);
        lines.push(format!(
            "Beyond this project, every character in this instance has standing access to the shared shelf — **{name}** — at all times. Reach it with a `qtap://` URI — `{uri}` (pass it as the `uri` arg to any `doc_*` tool); the `mount_point: \"{safe_name}\"` form and the ID `{id}` still work. Its `Scenarios/` folder holds the general chat-starter scenarios offered alongside project- and character-specific ones; other curated content lives here as well.",
            name = general.name,
            uri = general_shelf_uri(general),
            id = general.mount_point_id
        ));
    }

    crate::jsstr::js_trim_end(&lines.join("\n")).to_string()
}

/// v4 `postProsperoContextAnnouncement`. `systemKind` is
/// `project-and-general-context` / `project-context` / `general-context`
/// depending on which context is present. Public; returns `None` (posts nothing)
/// when neither context has content.
pub async fn post_prospero_context_announcement(
    db: &Db,
    params: ProsperoContextAnnouncement,
) -> Option<Value> {
    let has_project = project_has_content(params.project.as_ref());
    let has_general = params.general.is_some();
    if !has_project && !has_general {
        return None;
    }

    let content = build_combined_context_content(params.project.as_ref(), params.general.as_ref());
    let opaque =
        build_combined_context_opaque_content(params.project.as_ref(), params.general.as_ref());

    let kind = if has_project && has_general {
        "project-and-general-context"
    } else if has_project {
        "project-context"
    } else {
        "general-context"
    };

    post_prospero_message(db, &params.chat_id, content, Some(opaque), kind, None).await
}

// ===========================================================================
// Group + personal-vault context (whispered).
// ===========================================================================

/// v4 `renderWhisperStoreLine` (a group shelf line).
fn render_whisper_store_line(store: &ProsperoDocumentStoreInfo) -> String {
    let mut tags: Vec<String> = vec![format!("{}-backed", store.mount_type)];
    if store.store_type == "character" {
        tags.push("your private vault".to_string());
    }
    if !store.enabled {
        tags.push("currently disabled".to_string());
    }
    let tag_suffix = format!(" *({})*", tags.join("; "));
    let safe_name = escape_backticks(&store.name);
    let store_uri = format_doc_store_uri(&store.name, &store.id, "", false, None, None);
    format!(
        "- **{}**{} — reachable at `{}` (the `mount_point: \"{}\"` form and ID `{}` work too).",
        store.name, tag_suffix, store_uri, safe_name, store.id
    )
}

/// v4 `renderVaultStoreLine` (the character's own-vault line, `qtap://self/…`).
fn render_vault_store_line(store: &ProsperoDocumentStoreInfo) -> String {
    let mut tags: Vec<String> = vec![
        format!("{}-backed", store.mount_type),
        "your private vault".to_string(),
    ];
    if !store.enabled {
        tags.push("currently disabled".to_string());
    }
    let tag_suffix = format!(" *({})*", tags.join("; "));
    let safe_name = escape_backticks(&store.name);
    format!(
        "- **{}**{} — address it at `{}` (the reserved `self` authority always names your own vault; the name `{}` or ID `{}` work too).",
        store.name,
        tag_suffix,
        format_self_uri("", None, None),
        safe_name,
        store.id
    )
}

/// v4 `buildGroupAndVaultBody`.
fn build_group_and_vault_body(
    group_stores: &[ProsperoDocumentStoreInfo],
    vault_store: Option<&ProsperoDocumentStoreInfo>,
) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    if !group_stores.is_empty() {
        lines.push("**Shared shelves of the groups you belong to:**".to_string());
        lines.push(String::new());
        for store in group_stores {
            lines.push(render_whisper_store_line(store));
        }
        lines.push(String::new());
    }
    if let Some(vault) = vault_store {
        lines.push("**Your own vault:**".to_string());
        lines.push(String::new());
        lines.push(render_vault_store_line(vault));
        lines.push(String::new());
    }

    let mut scope_hints: Vec<String> = Vec::new();
    if !group_stores.is_empty() {
        scope_hints.push("`scope: \"group\"` reaches only these group shelves".to_string());
    }
    if vault_store.is_some() {
        scope_hints.push("`scope: \"character\"` reaches only your own vault".to_string());
    }
    let scope_sentence = if scope_hints.is_empty() {
        String::new()
    } else {
        format!(
            " On `doc_list_files` and `search`, {}.",
            scope_hints.join(", and ")
        )
    };

    lines.push(
        "Pass any of those names (or IDs) as the `mount_point` argument on `doc_*` tools, "
            .to_string()
            + "or as `source_mount_point` / `dest_mount_point` on `doc_copy_file`. The `path` "
            + "argument is the file's relative path within the chosen store."
            + &scope_sentence,
    );
    lines
}

/// v4 `buildGroupAndVaultWhisperContent` (persona-voiced).
pub fn build_group_and_vault_whisper_content(
    group_stores: &[ProsperoDocumentStoreInfo],
    vault_store: Option<&ProsperoDocumentStoreInfo>,
) -> String {
    let has_groups = !group_stores.is_empty();
    if !has_groups && vault_store.is_none() {
        return String::new();
    }

    let opener = if has_groups && vault_store.is_some() {
        "Prospero leafs through the rolls of your fellowships and sets a private memorandum at your elbow — the shelves you may reach by right of membership, and your own vault besides:"
    } else if has_groups {
        "Prospero leafs through the rolls of your fellowships and sets a private memorandum at your elbow — the shelves you may reach by right of membership:"
    } else {
        "Prospero sets a private memorandum at your elbow — the vault that is yours alone:"
    };

    let mut lines: Vec<String> = vec![opener.to_string(), String::new()];
    lines.extend(build_group_and_vault_body(group_stores, vault_store));
    crate::jsstr::js_trim_end(&lines.join("\n")).to_string()
}

/// v4 `buildGroupAndVaultWhisperOpaqueContent` (persona-stripped).
pub fn build_group_and_vault_whisper_opaque_content(
    group_stores: &[ProsperoDocumentStoreInfo],
    vault_store: Option<&ProsperoDocumentStoreInfo>,
) -> String {
    let has_groups = !group_stores.is_empty();
    if !has_groups && vault_store.is_none() {
        return String::new();
    }

    let opener = if has_groups && vault_store.is_some() {
        "Document stores you can reach by group membership, plus your own vault:"
    } else if has_groups {
        "Document stores you can reach by group membership:"
    } else {
        "Your own character vault:"
    };

    let mut lines: Vec<String> = vec![opener.to_string(), String::new()];
    lines.extend(build_group_and_vault_body(group_stores, vault_store));
    crate::jsstr::js_trim_end(&lines.join("\n")).to_string()
}

/// v4 `postProsperoGroupContextWhisper`. Loads the character's group shelves +
/// own vault, then whispers them to `target_participant_id` only
/// (`systemKind: 'group-context'`). Posts nothing (returns `None`) when the
/// character belongs to no groups with stores and has no vault.
pub async fn post_prospero_group_context_whisper(
    db: &Db,
    params: ProsperoGroupContextWhisper,
) -> Option<Value> {
    if params.character_id.is_empty() || params.target_participant_id.is_empty() {
        return None;
    }

    let character_id = params.character_id.clone();
    // v4 loads both stores in parallel; the ported loaders are sync reads over the
    // main + mount-index pooled connections held simultaneously.
    let loaded = db
        .read_main(|main| {
            db.read_mount_index(|mount| {
                Ok((
                    load_prospero_group_stores(main, mount, &character_id),
                    load_prospero_character_vault_store(main, mount, &character_id),
                ))
            })
        })
        .ok()?;
    let (group_stores, vault_store) = loaded;

    if group_stores.is_empty() && vault_store.is_none() {
        return None;
    }

    let content = build_group_and_vault_whisper_content(&group_stores, vault_store.as_ref());
    if content.is_empty() {
        return None;
    }
    let opaque = build_group_and_vault_whisper_opaque_content(&group_stores, vault_store.as_ref());

    post_prospero_message(
        db,
        &params.chat_id,
        content,
        Some(opaque),
        "group-context",
        Some(vec![params.target_participant_id.clone()]),
    )
    .await
}

// ===========================================================================
// Carina error (public `:` / whispered `?`).
// ===========================================================================

/// v4 `buildCarinaErrorContent`. A falsy (`None`/empty) `character_name` renders
/// as "the answerer"; an empty `detail` is dropped.
pub fn build_carina_error_content(
    kind: CarinaErrorKind,
    character_name: Option<&str>,
    detail: Option<&str>,
) -> String {
    match kind {
        CarinaErrorKind::NotFound => {
            "Prospero regrets to inform you that no answerer by that name is currently on duty."
                .to_string()
        }
        CarinaErrorKind::NoProfile => {
            let who = truthy(character_name).unwrap_or("the answerer");
            format!("Prospero notes that {who} lacks a connection to any LLM provider.")
        }
        CarinaErrorKind::LlmFailed => {
            let trailer = truthy(detail)
                .map(|d| format!(" — {d}"))
                .unwrap_or_default();
            let who = truthy(character_name).unwrap_or("the answerer");
            format!("Prospero reports that {who} was unable to respond{trailer}.")
        }
    }
}

/// v4 `buildCarinaErrorOpaqueContent`.
pub fn build_carina_error_opaque_content(
    kind: CarinaErrorKind,
    character_name: Option<&str>,
    detail: Option<&str>,
) -> String {
    match kind {
        CarinaErrorKind::NotFound => {
            "System: The requested Carina character was not found or is not enabled as an answerer."
                .to_string()
        }
        CarinaErrorKind::NoProfile => {
            let suffix = truthy(character_name)
                .map(|n| format!(" ({n})"))
                .unwrap_or_default();
            format!(
                "System: No connection profile available for the requested answerer character{suffix}."
            )
        }
        CarinaErrorKind::LlmFailed => {
            let suffix = truthy(detail)
                .map(|d| format!(" — {d}"))
                .unwrap_or_default();
            format!("System: The Carina answerer call failed{suffix}.")
        }
    }
}

/// v4 `postProsperoCarinaError`. `systemKind: 'carina-error'`. Whispered to the
/// asker when the query used `?` (and the asker participant id is known); public
/// otherwise. Closes the `PostProsperoCarinaError` seam declared in
/// `services::carina_runner`.
pub async fn post_prospero_carina_error(db: &Db, params: ProsperoCarinaErrorArgs) -> Option<Value> {
    let content = build_carina_error_content(
        params.kind,
        params.character_name.as_deref(),
        params.detail.as_deref(),
    );
    let opaque = build_carina_error_opaque_content(
        params.kind,
        params.character_name.as_deref(),
        params.detail.as_deref(),
    );
    // v4: `whisper && askerParticipantId ? [askerParticipantId] : null`.
    let targets = if params.whisper {
        params.asker_participant_id.clone().map(|a| vec![a])
    } else {
        None
    };
    post_prospero_message(
        db,
        &params.chat_id,
        content,
        Some(opaque),
        "carina-error",
        targets,
    )
    .await
}

// ===========================================================================
// The shared post idiom.
// ===========================================================================

/// v4 `postProsperoMessage`. Guards on the chat existing, mints `id`/`createdAt`,
/// assembles the exact `MessageEvent` object, validates it into a
/// [`ChatEventInput`], and writes it best-effort (`Err` → `None`, matching v4's
/// catch-and-return-null). Returns the assembled message object on success.
async fn post_prospero_message(
    db: &Db,
    chat_id: &str,
    content: String,
    opaque_content: Option<String>,
    kind_label: &str,
    target_participant_ids: Option<Vec<String>>,
) -> Option<Value> {
    // v4: `const chat = await repos.chats.findById(chatId); if (!chat) return null;`
    let cid = chat_id.to_string();
    match db.read_main(move |c| crate::db::chats_read::find_by_id(c, &cid)) {
        Ok(Some(_)) => {}
        _ => return None,
    }

    let message_id = uuid::Uuid::new_v4().to_string();
    let now = crate::clock::now_iso();

    // v4: `targetParticipantIds && targetParticipantIds.length ? … : null`.
    let targets: Option<Vec<String>> = target_participant_ids.filter(|t| !t.is_empty());

    let message = json!({
        "type": "message",
        "id": message_id,
        "role": "ASSISTANT",
        "content": content,
        "opaqueContent": opaque_content,
        "attachments": [],
        "createdAt": now,
        "participantId": null,
        "systemSender": "prospero",
        "systemKind": kind_label,
        "targetParticipantIds": targets,
    });

    let event: ChatEventInput = serde_json::from_value(message.clone()).ok()?;
    let cid2 = chat_id.to_string();
    db.write(move |writers| writers.main().chat_messages().add_message(&cid2, &event))
        .await
        .ok()?;

    Some(message)
}

// ===========================================================================
// DB loaders. Each composes the ported document-store repos; each fails soft
// (a lookup failure → `[]` / `None`), matching v4's try/catch shape. They are
// sync reads over the caller-provided connections (main + mount-index).
// ===========================================================================

/// One `doc_mount_points` row, in the field subset the Prospero loaders consume.
struct StoreRow {
    id: String,
    name: String,
    mount_type: String,
    store_type: Option<String>,
    enabled: bool,
}

/// v4 `repos.docMountPoints.findById(...)` — the scoped mount-point read the
/// loaders need (`id`/`name`/`mountType`/`storeType`/`enabled`). Done inline
/// (rather than through the `DocMountPointsRepository` accessors, which each cover
/// a different column subset) so the whole port stays inside this module.
fn find_store_row(mount: &Connection, id: &str) -> Result<Option<StoreRow>, DbError> {
    let row = mount
        .query_row(
            "SELECT id, name, mountType, storeType, enabled FROM doc_mount_points WHERE id = ?1",
            params![id],
            |row| {
                Ok(StoreRow {
                    id: row.get::<_, String>(0)?,
                    name: row.get::<_, String>(1)?,
                    mount_type: row.get::<_, String>(2)?,
                    store_type: row.get::<_, Option<String>>(3)?,
                    enabled: row.get::<_, i64>(4)? != 0,
                })
            },
        )
        .optional()?;
    Ok(row)
}

/// v4 `loadProsperoProjectContext`. `None` when the project does not exist;
/// `documentStores` is `[]` when nothing is linked or the lookup fails.
pub fn load_prospero_project_context(
    main: &Connection,
    mount: &Connection,
    project_id: &str,
) -> Option<ProsperoProjectContext> {
    let repo = crate::db::projects::ProjectsRepository::new(main, mount);
    // v4 `repos.projects.findById` — the overlaid read; a missing/broken store
    // throws (→ swallowed to `None` here on this best-effort path).
    let project = repo.find_by_id(project_id).ok()??;

    let name = project.get("name")?.as_str()?.to_string();
    let official = project
        .get("officialMountPointId")
        .and_then(Value::as_str)
        .map(str::to_string);

    let document_stores = load_linked_document_stores(mount, project_id, official.as_deref());

    Some(ProsperoProjectContext {
        name,
        description: str_field(&project, "description"),
        instructions: str_field(&project, "instructions"),
        document_stores,
    })
}

/// v4 `loadLinkedDocumentStores`. Official store first, then `localeCompare` by
/// name. `[]` on any lookup failure. Disabled stores are KEPT (marked "currently
/// disabled" in the section) — unlike the group/vault loaders, which skip them.
fn load_linked_document_stores(
    mount: &Connection,
    project_id: &str,
    official_mount_point_id: Option<&str>,
) -> Vec<ProsperoDocumentStoreInfo> {
    let inner = || -> Result<Vec<ProsperoDocumentStoreInfo>, DbError> {
        let links = crate::db::project_doc_mount_links::ProjectDocMountLinksRepository::new(mount)
            .find_by_project_id(project_id)?;
        if links.is_empty() {
            return Ok(Vec::new());
        }
        let mut stores: Vec<ProsperoDocumentStoreInfo> = Vec::new();
        for link in links {
            let Some(mp) = find_store_row(mount, &link)? else {
                continue;
            };
            let is_official = official_mount_point_id == Some(mp.id.as_str());
            stores.push(ProsperoDocumentStoreInfo {
                id: mp.id,
                name: mp.name,
                mount_type: mp.mount_type,
                store_type: mp.store_type.unwrap_or_else(|| "documents".to_string()),
                is_official,
                enabled: mp.enabled,
            });
        }
        stores.sort_by(|a, b| match (a.is_official, b.is_official) {
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            _ => crate::collation::locale_compare(&a.name, &b.name),
        });
        Ok(stores)
    };
    inner().unwrap_or_default()
}

/// v4 `loadProsperoGeneralContext`. `None` when the general mount id is unset or
/// the row is missing/disabled.
pub fn load_prospero_general_context(
    main: &Connection,
    mount: &Connection,
) -> Option<ProsperoGeneralContext> {
    let mount_point_id = crate::db::instance_settings::get_general_mount_point_id(main).ok()??;
    let mp = find_store_row(mount, &mount_point_id).ok()??;
    if !mp.enabled {
        return None;
    }
    Some(ProsperoGeneralContext {
        mount_point_id: mp.id,
        name: mp.name,
        mount_type: mp.mount_type,
    })
}

/// v4 `loadProsperoGroupStores`. The official + linked stores of every group the
/// character belongs to (via the shared tier resolver). Disabled stores skipped.
/// `localeCompare` by name. `[]` on no membership or any failure.
pub fn load_prospero_group_stores(
    main: &Connection,
    mount: &Connection,
    character_id: &str,
) -> Vec<ProsperoDocumentStoreInfo> {
    let ids = crate::db::tiered_mount_pool::resolve_group_mount_point_ids_for_character(
        main,
        mount,
        character_id,
    );
    if ids.is_empty() {
        return Vec::new();
    }
    let mut stores: Vec<ProsperoDocumentStoreInfo> = Vec::new();
    for id in ids {
        let Ok(Some(mp)) = find_store_row(mount, &id) else {
            continue;
        };
        if !mp.enabled {
            continue;
        }
        stores.push(ProsperoDocumentStoreInfo {
            id: mp.id,
            name: mp.name,
            mount_type: mp.mount_type,
            store_type: mp.store_type.unwrap_or_else(|| "documents".to_string()),
            is_official: false,
            enabled: mp.enabled,
        });
    }
    stores.sort_by(|a, b| crate::collation::locale_compare(&a.name, &b.name));
    stores
}

/// v4 `loadProsperoCharacterVaultStore`. The character's own vault store (via the
/// raw `characterDocumentMountPointId` pointer). `None` when the character has no
/// vault or on any failure. Note the `'character'` storeType default (vs
/// `'documents'` for the group/project loaders).
pub fn load_prospero_character_vault_store(
    main: &Connection,
    mount: &Connection,
    character_id: &str,
) -> Option<ProsperoDocumentStoreInfo> {
    let character = crate::db::characters_read::find_by_id_raw(main, character_id).ok()??;
    let mount_point_id = character
        .get("characterDocumentMountPointId")
        .and_then(Value::as_str)?
        .to_string();
    let mp = find_store_row(mount, &mount_point_id).ok()??;
    if !mp.enabled {
        return None;
    }
    Some(ProsperoDocumentStoreInfo {
        id: mp.id,
        name: mp.name,
        mount_type: mp.mount_type,
        store_type: mp.store_type.unwrap_or_else(|| "character".to_string()),
        is_official: false,
        enabled: mp.enabled,
    })
}

// ===========================================================================
// Small helpers.
// ===========================================================================

/// `value?.trim()` truthiness — returns the JS-trimmed slice when non-empty, else
/// `None` (a `None`, absent, or all-whitespace value is falsy).
fn trimmed_nonempty(value: Option<&str>) -> Option<&str> {
    match value {
        Some(v) => {
            let t = crate::jsstr::js_trim(v);
            if t.is_empty() {
                None
            } else {
                Some(t)
            }
        }
        None => None,
    }
}

/// JS truthiness of a `string | null` — `None`/empty are falsy.
fn truthy(value: Option<&str>) -> Option<&str> {
    value.filter(|s| !s.is_empty())
}

/// Read a nullable string field off a JSON object (absent / `null` → `None`).
fn str_field(obj: &Value, key: &str) -> Option<String> {
    obj.get(key).and_then(Value::as_str).map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store(
        id: &str,
        name: &str,
        mount_type: &str,
        store_type: &str,
        is_official: bool,
        enabled: bool,
    ) -> ProsperoDocumentStoreInfo {
        ProsperoDocumentStoreInfo {
            id: id.to_string(),
            name: name.to_string(),
            mount_type: mount_type.to_string(),
            store_type: store_type.to_string(),
            is_official,
            enabled,
        }
    }

    #[test]
    fn connection_profile_nullish_coalescing() {
        assert_eq!(
            build_connection_profile_change_content("Ada", Some("A"), Some("B")),
            "Ada's current response model is now B; previous model was A."
        );
        assert_eq!(
            build_connection_profile_change_content("Ada", None, None),
            "Ada's current response model is now unassigned; previous model was unassigned."
        );
        // Empty string is NOT coalesced (nullish, not falsy).
        assert_eq!(
            build_connection_profile_change_content("Ada", Some(""), Some("")),
            "Ada's current response model is now ; previous model was ."
        );
    }

    #[test]
    fn carina_error_falsy_name_and_detail() {
        assert_eq!(
            build_carina_error_content(CarinaErrorKind::NoProfile, None, None),
            "Prospero notes that the answerer lacks a connection to any LLM provider."
        );
        assert_eq!(
            build_carina_error_content(CarinaErrorKind::NoProfile, Some(""), None),
            "Prospero notes that the answerer lacks a connection to any LLM provider."
        );
        assert_eq!(
            build_carina_error_content(CarinaErrorKind::LlmFailed, Some("Bea"), Some("timeout")),
            "Prospero reports that Bea was unable to respond — timeout."
        );
        assert_eq!(
            build_carina_error_opaque_content(CarinaErrorKind::LlmFailed, None, Some("")),
            "System: The Carina answerer call failed."
        );
    }

    #[test]
    fn group_and_vault_empty_is_blank() {
        assert_eq!(build_group_and_vault_whisper_content(&[], None), "");
        assert_eq!(build_group_and_vault_whisper_opaque_content(&[], None), "");
    }

    #[test]
    fn combined_empty_project_and_no_general_is_blank() {
        let empty = ProsperoProjectContext {
            name: "P".to_string(),
            description: None,
            instructions: Some("   ".to_string()),
            document_stores: vec![],
        };
        assert_eq!(build_combined_context_content(Some(&empty), None), "");
        assert_eq!(
            build_combined_context_opaque_content(Some(&empty), None),
            ""
        );
    }

    #[test]
    fn document_stores_section_official_first_tags() {
        let stores = vec![
            store("mp-1", "Docs", "database", "documents", true, true),
            store("mp-2", "Vault", "filesystem", "character", false, false),
        ];
        let section = build_document_stores_section(&stores);
        assert_eq!(section[0], "**Document stores linked to this project:**");
        assert!(section[2].contains("official project store"));
        assert!(section[2].contains("database-backed"));
        assert!(section[3].contains("character vault"));
        assert!(section[3].contains("currently disabled"));
    }
}
