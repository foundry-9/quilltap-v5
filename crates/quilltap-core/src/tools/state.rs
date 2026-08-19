//! Port of v4's `state` tool (`lib/tools/handlers/state-handler.ts` +
//! `lib/tools/state-tool.ts`).
//!
//! Persistent key-value state for games, inventory, and session data across the
//! FOUR-tier cascade (chat → project → group → general, `f48f34dc`). Path-based
//! access with dot notation + array indexing; a merged fetch resolves the shared
//! cascade (narrower tiers win). `fetch` reads, `set` writes, `delete` removes.
//! The group tier is scoped to the responding character's own memberships
//! (Knowledge's rule) and pinned down via `resolve_group_for_context`.
//!
//! ## The writes, faithfully
//!
//! - Chat state → `repos.chats.update(chatId, { state })` — a partial `SET` that
//!   (per the chats port) does **not** mint `updatedAt`.
//! - Project state → `repos.projects.update(projectId, { state })` — routed to the
//!   store-backed `state.json` overlay (the project slim row's `updatedAt` is NOT
//!   bumped by a store-only update).
//! - Group state → `repos.groups.update(groupId, { state })` — the same overlay
//!   shape on the group's official store.
//! - General state → `writeGeneralState` (the mount-root `state.json`).
//!
//! Both run on the writer connections inside one [`Db::write`] closure (reads +
//! writes on the same borrowed connections, serialized on the writer thread).
//!
//! ## Output shape (byte-exact JSON)
//!
//! v4's handler returns per-branch object literals; a fixed serde field order
//! `success, operation, context, path, value, previousValue, error` with
//! `skip_serializing_if` reproduces every branch's `JSON.stringify` exactly (v4
//! never emits `error` alongside `value`/`previousValue`, and success always
//! renders `value` before `previousValue`). `None` = v4 `undefined` (dropped);
//! `Some(Value::Null)` = an explicit JSON `null` (kept) — the undefined-vs-null
//! distinction the differential relies on.
//!
//! **Open-JSON key-order seam:** `state` values are arbitrary JSON. `serde_json`
//! under `preserve_order` emits insertion order, matching v4's `JSON.stringify`,
//! but the corpus keeps stored/echoed values to single-key / already-sorted /
//! non-`X.0`-float shapes (the standing `parameters`/`equippedOutfit` constraint).

use serde::ser::SerializeStruct;
use serde::{Serialize, Serializer};
use serde_json::{Map, Value};

use crate::db::chats_read;
use crate::db::projects::ProjectsRepository;
use crate::db::runtime::Db;
use crate::state::paths::{delete_at_path, get_at_path, parse_path, set_at_path, PathKey};

/// Context required for state tool execution (v4 `StateToolContext`).
#[derive(Debug, Clone)]
pub struct StateToolContext {
    pub user_id: String,
    pub chat_id: String,
    /// The dispatcher passes `context.projectId` (the chat's project override);
    /// when absent the handler falls back to the chat's own `projectId`.
    pub project_id: Option<String>,
    /// Responding character ID (optional, `f48f34dc`). Scopes the group tier to
    /// this character's own memberships (Knowledge's rule). Absent -> no group
    /// tier.
    pub character_id: Option<String>,
}

// The pure path helpers (`PathKey`, `parse_path`, `get_at_path`, `set_at_path`,
// `delete_at_path`) moved to `crate::state::paths` at the v4 `f48f34dc`
// extraction (v4's handler re-exports them for back-compat; this module simply
// imports them).

/// v4's `StateToolOutput`. Serialized in a fixed field order that reproduces
/// every per-branch object literal (see the module docs). `operation` is a raw
/// [`Value`] so a validation-failure can echo a non-string input `operation`
/// exactly (v4 casts `input.operation` through unchanged).
#[derive(Debug, Clone)]
pub struct StateToolOutput {
    pub success: bool,
    pub operation: Value,
    pub context: Option<String>,
    pub path: Option<String>,
    /// `None` = v4 `undefined` (dropped); `Some(Null)` = an explicit JSON `null`.
    pub value: Option<Value>,
    pub previous_value: Option<Value>,
    pub error: Option<String>,
}

impl Serialize for StateToolOutput {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        // Count the present optional fields for the struct length.
        let mut len = 2; // success, operation
        if self.context.is_some() {
            len += 1;
        }
        if self.path.is_some() {
            len += 1;
        }
        if self.value.is_some() {
            len += 1;
        }
        if self.previous_value.is_some() {
            len += 1;
        }
        if self.error.is_some() {
            len += 1;
        }
        let mut st = s.serialize_struct("StateToolOutput", len)?;
        st.serialize_field("success", &self.success)?;
        st.serialize_field("operation", &self.operation)?;
        if let Some(c) = &self.context {
            st.serialize_field("context", c)?;
        }
        if let Some(p) = &self.path {
            st.serialize_field("path", p)?;
        }
        if let Some(v) = &self.value {
            st.serialize_field("value", v)?;
        }
        if let Some(pv) = &self.previous_value {
            st.serialize_field("previousValue", pv)?;
        }
        if let Some(e) = &self.error {
            st.serialize_field("error", e)?;
        }
        st.end()
    }
}

impl StateToolOutput {
    fn fail(operation: Value, error: impl Into<String>) -> Self {
        StateToolOutput {
            success: false,
            operation,
            context: None,
            path: None,
            value: None,
            previous_value: None,
            error: Some(error.into()),
        }
    }
}

/// Extract the state object from a hydrated chat/project/group row (`state` is
/// `object_or_empty` -> an object, or `{}`).
fn state_object(row: &Value) -> Value {
    match row.get("state") {
        Some(v) if v.is_object() => v.clone(),
        _ => Value::Object(Map::new()),
    }
}

/// v4 `validateStateInput` (`stateToolInputSchema.safeParse().success`): `operation`
/// must be one of the three enum members; `context` if present must be one of the
/// FOUR tiers (`f48f34dc`); `group` / `path` if present must be strings; `value`
/// is `unknown`. Extra keys are stripped (not rejected) by a plain `z.object`.
fn validate_input(args: &Value) -> bool {
    let Some(obj) = args.as_object() else {
        return false;
    };
    match obj.get("operation").and_then(Value::as_str) {
        Some("fetch" | "set" | "delete") => {}
        _ => return false,
    }
    if let Some(c) = obj.get("context") {
        match c.as_str() {
            Some("chat" | "project" | "group" | "general") => {}
            _ => return false,
        }
    }
    if let Some(g) = obj.get("group") {
        if !g.is_string() {
            return false;
        }
    }
    if let Some(p) = obj.get("path") {
        if !p.is_string() {
            return false;
        }
    }
    true
}

/// The failure `operation` v4 derives when validation fails / catch fires:
/// `input.operation` if `input` is an object with that key, else `'fetch'`.
fn fallback_operation(args: &Value) -> Value {
    args.as_object()
        .and_then(|o| o.get("operation").cloned())
        .unwrap_or_else(|| Value::String("fetch".to_string()))
}

/// Execute the `state` tool (v4 `executeStateTool`). Runs reads + the write inside
/// a single [`Db::write`] closure. A DB error surfaces as v4's catch result
/// (`{ success:false, operation:<fallback>, error: err.message }`).
pub async fn execute_state_tool(db: &Db, ctx: &StateToolContext, args: &Value) -> StateToolOutput {
    let ctx = ctx.clone();
    let args_owned = args.clone();
    let args_for_closure = args.clone();
    let result = db
        .write(move |writers| Ok(run_state(writers, &ctx, &args_for_closure)))
        .await;
    match result {
        Ok(out) => out,
        // v4's outer catch: `error.message`.
        Err(e) => StateToolOutput::fail(fallback_operation(&args_owned), e.to_string()),
    }
}

/// The synchronous body, run on the writer connections (v4's rewritten
/// four-tier `executeStateTool`, `f48f34dc`).
fn run_state(
    writers: &mut crate::db::runtime::WriterSet,
    ctx: &StateToolContext,
    args: &Value,
) -> StateToolOutput {
    use crate::services::mount_index::general_state::{read_general_state, write_general_state};
    use crate::state::cascade::{
        resolve_group_candidates, resolve_group_for_context, resolve_state_cascade, GroupScope,
    };

    if !validate_input(args) {
        return StateToolOutput::fail(
            fallback_operation(args),
            "Invalid input: operation is required and must be \"fetch\", \"set\", or \"delete\"",
        );
    }
    let obj = args.as_object().expect("validated object");
    let operation = obj
        .get("operation")
        .and_then(Value::as_str)
        .expect("validated operation")
        .to_string();
    let state_context = obj
        .get("context")
        .and_then(Value::as_str)
        .map(str::to_string);
    let group_ref = obj.get("group").and_then(Value::as_str).map(str::to_string);
    let path = obj.get("path").and_then(Value::as_str).map(str::to_string);
    // v4: `value` is the raw input value; absent key -> undefined (no set).
    let value_present = obj.contains_key("value");
    let value = obj.get("value").cloned();
    let parsed_path = parse_path(path.as_deref());

    let main = writers.main().connection();

    // Fetch the chat + ownership check.
    let chat = match chats_read::find_by_id(main, &ctx.chat_id) {
        Ok(Some(c)) => c,
        Ok(None) => {
            return StateToolOutput::fail(
                Value::String(operation),
                "Chat not found or permission denied",
            )
        }
        Err(e) => return StateToolOutput::fail(Value::String(operation), e.to_string()),
    };
    if chat.get("userId").and_then(Value::as_str) != Some(ctx.user_id.as_str()) {
        return StateToolOutput::fail(
            Value::String(operation),
            "Chat not found or permission denied",
        );
    }

    // Resolve the project (context override, then the chat's own projectId).
    let project_id = ctx
        .project_id
        .clone()
        .filter(|s| !s.is_empty())
        .or_else(|| {
            chat.get("projectId")
                .and_then(Value::as_str)
                .map(str::to_string)
        });

    let project = match &project_id {
        Some(pid) => match writers.mount_index() {
            Some(mount_w) => {
                // v4 `repos.projects.findById(projectId)` -- hydrated (overlaid state).
                let repo = ProjectsRepository::new(main, mount_w.connection());
                match repo.find_by_id(pid) {
                    Ok(p) => p,
                    Err(e) => {
                        return StateToolOutput::fail(Value::String(operation), e.to_string())
                    }
                }
            }
            // No mount-index -> the project store is unreachable (degraded); treat
            // as "no project" (the corpus always provisions the store).
            None => None,
        },
        None => None,
    };

    let chat_state = state_object(&chat);
    let project_state = project
        .as_ref()
        .map(state_object)
        .unwrap_or(Value::Object(Map::new()));

    // The group tier follows the responding character's own memberships
    // (Knowledge's rule). Absent characterId -> no group tier at all.
    let group_scope = match &ctx.character_id {
        Some(character_id) if !character_id.is_empty() => GroupScope::Character {
            character_id: character_id.clone(),
        },
        _ => GroupScope::None,
    };

    // Resolve one group for an explicit group-context op, returning a typed
    // tool error (never propagating) when it can't be pinned down — v4
    // `resolveGroupOrError`. Returns the hydrated group row.
    let resolve_group_or_error = |writers: &mut crate::db::runtime::WriterSet| {
        let mount = writers.mount_index().map(|w| w.connection());
        let main = writers.main().connection();
        let candidates = resolve_group_candidates(main, mount, &chat, &group_scope);
        match resolve_group_for_context(group_ref.as_deref(), &candidates) {
            Ok(g) => Ok(g.clone()),
            Err(e) => Err(e.message),
        }
    };

    // Underscore-prefixed user-only keys are off-limits to the AI (SET and
    // DELETE only -- fetch of `_` keys is allowed).
    let underscore_denied = |verb: &str| -> Option<StateToolOutput> {
        if is_underscore_key(&parsed_path) {
            return Some(StateToolOutput {
                success: false,
                operation: Value::String(operation.clone()),
                context: state_context.clone(),
                path: path.clone(),
                value: None,
                previous_value: None,
                error: Some(format!(
                    "Keys starting with underscore are user-only and cannot be {verb} by AI"
                )),
            });
        }
        None
    };

    match operation.as_str() {
        "fetch" => {
            let result_value = match state_context.as_deref() {
                Some("chat") => get_at_path(&chat_state, &parsed_path),
                Some("project") => {
                    if project.is_none() {
                        return StateToolOutput {
                            success: false,
                            operation: Value::String(operation),
                            context: state_context,
                            path,
                            value: None,
                            previous_value: None,
                            error: Some("Chat is not part of a project".to_string()),
                        };
                    }
                    get_at_path(&project_state, &parsed_path)
                }
                Some("group") => match resolve_group_or_error(writers) {
                    Ok(group) => get_at_path(&state_object(&group), &parsed_path),
                    Err(error) => {
                        return StateToolOutput {
                            success: false,
                            operation: Value::String(operation),
                            context: state_context,
                            path,
                            value: None,
                            previous_value: None,
                            error: Some(error),
                        }
                    }
                },
                Some("general") => {
                    let mount = writers.mount_index().map(|w| w.connection());
                    let main = writers.main().connection();
                    let general_state = read_general_state(main, mount);
                    get_at_path(&general_state, &parsed_path)
                }
                _ => {
                    // Merged cascade (chat over project over group over general).
                    // NB v4's cascade reads the CHAT's own projectId (no
                    // context.projectId override on this path).
                    let mount = writers.mount_index().map(|w| w.connection());
                    let main = writers.main().connection();
                    let cascade = resolve_state_cascade(main, mount, &chat, &group_scope);
                    get_at_path(&cascade.merged, &parsed_path)
                }
            };
            StateToolOutput {
                success: true,
                operation: Value::String(operation),
                context: state_context,
                path,
                // v4: `value: resultValue` -- undefined (`None`) dropped, an explicit
                // stored null (`Some(Null)`) kept.
                value: result_value,
                previous_value: None,
                error: None,
            }
        }

        "set" => {
            if let Some(denied) = underscore_denied("modified") {
                return denied;
            }
            // v4: `stateContext || 'chat'`.
            let target = state_context.clone().unwrap_or_else(|| "chat".to_string());
            let set_value = value.clone().unwrap_or(Value::Null); // `value` may be undefined

            let (previous, write_result): (Option<Value>, Result<(), String>) = match target
                .as_str()
            {
                "project" => {
                    let Some(project) = project.as_ref() else {
                        return StateToolOutput {
                            success: false,
                            operation: Value::String(operation),
                            context: Some(target),
                            path,
                            value: None,
                            previous_value: None,
                            error: Some("Chat is not part of a project".to_string()),
                        };
                    };
                    let previous = get_at_path(&project_state, &parsed_path);
                    let mut new_state = project_state.clone();
                    if let Err(e) = set_at_path(&mut new_state, &parsed_path, set_value.clone()) {
                        return StateToolOutput::fail(Value::String(operation), e);
                    }
                    let pid = project
                        .get("id")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string();
                    (
                        previous,
                        write_project_state(writers, &pid, &new_state).map_err(|e| e.to_string()),
                    )
                }
                "group" => {
                    let group = match resolve_group_or_error(writers) {
                        Ok(g) => g,
                        Err(error) => {
                            return StateToolOutput {
                                success: false,
                                operation: Value::String(operation),
                                context: Some(target),
                                path,
                                value: None,
                                previous_value: None,
                                error: Some(error),
                            }
                        }
                    };
                    let group_state = state_object(&group);
                    let previous = get_at_path(&group_state, &parsed_path);
                    let mut new_state = group_state.clone();
                    if let Err(e) = set_at_path(&mut new_state, &parsed_path, set_value.clone()) {
                        return StateToolOutput::fail(Value::String(operation), e);
                    }
                    let gid = group
                        .get("id")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string();
                    (
                        previous,
                        write_group_state(writers, &gid, &new_state).map_err(|e| e.to_string()),
                    )
                }
                "general" => {
                    let mount = writers.mount_index().map(|w| w.connection());
                    let main = writers.main().connection();
                    let general_state = read_general_state(main, mount);
                    let previous = get_at_path(&general_state, &parsed_path);
                    let mut new_state = general_state.clone();
                    if let Err(e) = set_at_path(&mut new_state, &parsed_path, set_value.clone()) {
                        return StateToolOutput::fail(Value::String(operation), e);
                    }
                    let write = match writers.mount_index() {
                        Some(mount_w) => {
                            let mount_c = mount_w.connection();
                            let main_c = writers.main().connection();
                            write_general_state(main_c, mount_c, &new_state)
                                .map_err(|e| e.to_string())
                        }
                        // Degraded open: the unprovisioned arm (v4 throws).
                        None => {
                            Err("Quilltap General mount has not been provisioned yet".to_string())
                        }
                    };
                    (previous, write)
                }
                _ => {
                    let previous = get_at_path(&chat_state, &parsed_path);
                    let mut new_state = chat_state.clone();
                    if let Err(e) = set_at_path(&mut new_state, &parsed_path, set_value.clone()) {
                        return StateToolOutput::fail(Value::String(operation), e);
                    }
                    (
                        previous,
                        write_chat_state(writers, &ctx.chat_id, &new_state)
                            .map_err(|e| e.to_string()),
                    )
                }
            };
            if let Err(e) = write_result {
                return StateToolOutput::fail(Value::String(operation), e);
            }
            StateToolOutput {
                success: true,
                operation: Value::String(operation),
                context: Some(target),
                path,
                value: if value_present { value } else { None },
                previous_value: previous,
                error: None,
            }
        }

        "delete" => {
            if let Some(denied) = underscore_denied("deleted") {
                return denied;
            }
            let target = state_context.clone().unwrap_or_else(|| "chat".to_string());

            let (previous, write_result): (Option<Value>, Result<(), String>) =
                match target.as_str() {
                    "project" => {
                        let Some(project) = project.as_ref() else {
                            return StateToolOutput {
                                success: false,
                                operation: Value::String(operation),
                                context: Some(target),
                                path,
                                value: None,
                                previous_value: None,
                                error: Some("Chat is not part of a project".to_string()),
                            };
                        };
                        let previous = get_at_path(&project_state, &parsed_path);
                        let mut new_state = project_state.clone();
                        let deleted = delete_at_path(&mut new_state, &parsed_path);
                        let write = if deleted {
                            let pid = project
                                .get("id")
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_string();
                            write_project_state(writers, &pid, &new_state)
                                .map_err(|e| e.to_string())
                        } else {
                            Ok(())
                        };
                        (previous, write)
                    }
                    "group" => {
                        let group = match resolve_group_or_error(writers) {
                            Ok(g) => g,
                            Err(error) => {
                                return StateToolOutput {
                                    success: false,
                                    operation: Value::String(operation),
                                    context: Some(target),
                                    path,
                                    value: None,
                                    previous_value: None,
                                    error: Some(error),
                                }
                            }
                        };
                        let group_state = state_object(&group);
                        let previous = get_at_path(&group_state, &parsed_path);
                        let mut new_state = group_state.clone();
                        let deleted = delete_at_path(&mut new_state, &parsed_path);
                        let write = if deleted {
                            let gid = group
                                .get("id")
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_string();
                            write_group_state(writers, &gid, &new_state).map_err(|e| e.to_string())
                        } else {
                            Ok(())
                        };
                        (previous, write)
                    }
                    "general" => {
                        let mount = writers.mount_index().map(|w| w.connection());
                        let main = writers.main().connection();
                        let general_state = read_general_state(main, mount);
                        let previous = get_at_path(&general_state, &parsed_path);
                        let mut new_state = general_state.clone();
                        let deleted = delete_at_path(&mut new_state, &parsed_path);
                        let write = if deleted {
                            match writers.mount_index() {
                                Some(mount_w) => {
                                    let mount_c = mount_w.connection();
                                    let main_c = writers.main().connection();
                                    write_general_state(main_c, mount_c, &new_state)
                                        .map_err(|e| e.to_string())
                                }
                                None => Err("Quilltap General mount has not been provisioned yet"
                                    .to_string()),
                            }
                        } else {
                            Ok(())
                        };
                        (previous, write)
                    }
                    _ => {
                        let previous = get_at_path(&chat_state, &parsed_path);
                        let mut new_state = chat_state.clone();
                        let deleted = delete_at_path(&mut new_state, &parsed_path);
                        let write = if deleted {
                            write_chat_state(writers, &ctx.chat_id, &new_state)
                                .map_err(|e| e.to_string())
                        } else {
                            Ok(())
                        };
                        (previous, write)
                    }
                };
            if let Err(e) = write_result {
                return StateToolOutput::fail(Value::String(operation), e);
            }
            StateToolOutput {
                success: true,
                operation: Value::String(operation),
                context: Some(target),
                path,
                value: None,
                previous_value: previous,
                error: None,
            }
        }

        // v4: `Unknown operation: ${operation}` (validation already gates this).
        other => StateToolOutput::fail(
            Value::String(operation.clone()),
            format!("Unknown operation: {other}"),
        ),
    }
}

/// Whether the first path segment is an underscore-prefixed (user-only) string
/// key (v4: `parsedPath[0]` is a string starting with `_`).
fn is_underscore_key(parsed_path: &[PathKey]) -> bool {
    matches!(parsed_path.first(), Some(PathKey::Prop(s)) if s.starts_with('_'))
}

/// Write the chat's `state` column (`repos.chats.update(chatId, { state })`).
pub(crate) fn write_chat_state(
    writers: &mut crate::db::runtime::WriterSet,
    chat_id: &str,
    new_state: &Value,
) -> Result<(), crate::db::DbError> {
    let update = crate::db::chats::ChatUpdate {
        state: Some(new_state.clone()),
        ..Default::default()
    };
    writers.main().chats().update(chat_id, &update)?;
    Ok(())
}

/// Write the project's `state.json` overlay (`repos.projects.update(id, { state })`).
pub(crate) fn write_project_state(
    writers: &mut crate::db::runtime::WriterSet,
    project_id: &str,
    new_state: &Value,
) -> Result<(), crate::db::DbError> {
    let mount = writers
        .mount_index()
        .ok_or_else(|| crate::db::DbError::Internal("project state requires mount-index".into()))?
        .connection();
    let main = writers.main().connection();
    let repo = ProjectsRepository::new(main, mount);
    let mut patch = Map::new();
    patch.insert("state".to_string(), new_state.clone());
    repo.update(project_id, &patch)
        .map_err(|e| crate::db::DbError::Internal(e.to_string()))?;
    Ok(())
}

/// Write the group's `state.json` overlay (`repos.groups.update(id, { state })`).
pub(crate) fn write_group_state(
    writers: &mut crate::db::runtime::WriterSet,
    group_id: &str,
    new_state: &Value,
) -> Result<(), crate::db::DbError> {
    let mount = writers
        .mount_index()
        .ok_or_else(|| crate::db::DbError::Internal("group state requires mount-index".into()))?
        .connection();
    let main = writers.main().connection();
    let repo = crate::db::groups::GroupsRepository::new(main, mount);
    let mut patch = Map::new();
    patch.insert("state".to_string(), new_state.clone());
    repo.update(group_id, &patch)
        .map_err(|e| crate::db::DbError::Internal(e.to_string()))?;
    Ok(())
}

/// v4 `formatStateResults` — the human-readable line for the LLM context.
pub fn format_state_results(out: &StateToolOutput) -> String {
    if !out.success {
        return format!(
            "State Error: {}",
            out.error
                .as_deref()
                .filter(|s| !s.is_empty())
                .unwrap_or("Unknown error")
        );
    }
    let path_display = match &out.path {
        Some(p) if !p.is_empty() => p.clone(),
        _ => "(root)".to_string(),
    };
    let context_display = match &out.context {
        Some(c) => format!(" [{c}]"),
        None => String::new(),
    };
    match out.operation.as_str() {
        Some("fetch") => match &out.value {
            None => format!("State{context_display} at \"{path_display}\": (not set)"),
            Some(v) => format!(
                "State{context_display} at \"{path_display}\": {}",
                json_pretty(v)
            ),
        },
        Some("set") => match &out.previous_value {
            None => format!(
                "State{context_display} set \"{path_display}\" to: {}",
                json_compact(out.value.as_ref().unwrap_or(&Value::Null))
            ),
            Some(prev) => format!(
                "State{context_display} updated \"{path_display}\": {} → {}",
                json_compact(prev),
                json_compact(out.value.as_ref().unwrap_or(&Value::Null))
            ),
        },
        Some("delete") => match &out.previous_value {
            None => format!("State{context_display} delete \"{path_display}\": (was not set)"),
            Some(prev) => format!(
                "State{context_display} deleted \"{path_display}\" (was: {})",
                json_compact(prev)
            ),
        },
        _ => "State operation completed".to_string(),
    }
}

/// `JSON.stringify(value, null, 2)` — 2-space pretty (serde matches JS for the
/// constrained corpus shapes).
fn json_pretty(v: &Value) -> String {
    serde_json::to_string_pretty(v).unwrap_or_else(|_| "null".into())
}

/// `JSON.stringify(value)` — compact.
fn json_compact(v: &Value) -> String {
    serde_json::to_string(v).unwrap_or_else(|_| "null".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn output_serializes_in_fixed_order() {
        let set_ok = StateToolOutput {
            success: true,
            operation: json!("set"),
            context: Some("chat".into()),
            path: Some("hp".into()),
            value: Some(json!(5)),
            previous_value: Some(json!(3)),
            error: None,
        };
        assert_eq!(
            serde_json::to_string(&set_ok).unwrap(),
            r#"{"success":true,"operation":"set","context":"chat","path":"hp","value":5,"previousValue":3}"#
        );
        let fetch_missing = StateToolOutput {
            success: true,
            operation: json!("fetch"),
            context: None,
            path: Some("hp".into()),
            value: None,
            previous_value: None,
            error: None,
        };
        assert_eq!(
            serde_json::to_string(&fetch_missing).unwrap(),
            r#"{"success":true,"operation":"fetch","path":"hp"}"#
        );
    }

    #[test]
    fn format_matches_v4() {
        let set = StateToolOutput {
            success: true,
            operation: json!("set"),
            context: Some("chat".into()),
            path: Some("player.hp".into()),
            value: Some(json!(5)),
            previous_value: Some(json!(3)),
            error: None,
        };
        assert_eq!(
            format_state_results(&set),
            "State [chat] updated \"player.hp\": 3 → 5"
        );
        let fetch = StateToolOutput {
            success: true,
            operation: json!("fetch"),
            context: None,
            path: None,
            value: Some(json!({ "hp": 5 })),
            previous_value: None,
            error: None,
        };
        assert_eq!(
            format_state_results(&fetch),
            "State at \"(root)\": {\n  \"hp\": 5\n}"
        );
    }
}
