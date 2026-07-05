//! Port of v4's `state` tool (`lib/tools/handlers/state-handler.ts` +
//! `lib/tools/state-tool.ts`).
//!
//! Persistent per-chat / per-project key-value state for games, inventory, and
//! session data. Path-based access with dot notation + array indexing; chat state
//! overrides project state when merged. `fetch` reads, `set` writes, `delete`
//! removes.
//!
//! ## The writes, faithfully
//!
//! - Chat state → `repos.chats.update(chatId, { state })` — a partial `SET` that
//!   (per the chats port) does **not** mint `updatedAt`.
//! - Project state → `repos.projects.update(projectId, { state })` — routed to the
//!   store-backed `state.json` overlay (the project slim row's `updatedAt` is NOT
//!   bumped by a store-only update).
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

/// Context required for state tool execution (v4 `StateToolContext`).
#[derive(Debug, Clone)]
pub struct StateToolContext {
    pub user_id: String,
    pub chat_id: String,
    /// The dispatcher passes `context.projectId` (the chat's project override);
    /// when absent the handler falls back to the chat's own `projectId`.
    pub project_id: Option<String>,
}

/// A single key in a parsed path — a property name or an array index (v4's
/// `(string | number)[]`).
#[derive(Debug, Clone, PartialEq)]
enum PathKey {
    Prop(String),
    Index(usize),
}

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

/// v4 `parsePath`: split `"player.inventory[0].name"` into keys. Regex
/// `(\w+)|\[(\d+)\]` — JS `\w`/`\d` are ASCII, so `[A-Za-z0-9_]+` / `[0-9]+`.
fn parse_path(path: Option<&str>) -> Vec<PathKey> {
    let Some(path) = path else {
        return Vec::new();
    };
    if path.trim().is_empty() {
        return Vec::new();
    }
    let bytes = path.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if c.is_ascii_alphanumeric() || c == b'_' {
            // A property-name run: [A-Za-z0-9_]+.
            let start = i;
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            out.push(PathKey::Prop(path[start..i].to_string()));
        } else if c == b'[' {
            // An array index: \[(\d+)\]. Only matches when digits + `]` follow.
            let mut j = i + 1;
            let dstart = j;
            while j < bytes.len() && bytes[j].is_ascii_digit() {
                j += 1;
            }
            if j > dstart && j < bytes.len() && bytes[j] == b']' {
                // parseInt(_, 10). JS numbers are f64; a path index in range fits usize.
                if let Ok(idx) = path[dstart..j].parse::<usize>() {
                    out.push(PathKey::Index(idx));
                }
                i = j + 1;
            } else {
                // `[` not opening a valid index — the regex would skip this char.
                i += 1;
            }
        } else {
            // Any other char is between tokens; the global regex skips it.
            i += 1;
        }
    }
    out
}

/// v4 `getAtPath`: walk `path`, returning `None` (JS `undefined`) when a segment
/// is missing or the current node is not indexable, and `Some(Value::Null)` when
/// the resolved value is an explicit stored `null` (v4's `getAtPath` returns a
/// final null as-is; an INTERMEDIATE null short-circuits to undefined). Empty path
/// → the whole object.
fn get_at_path(obj: &Value, path: &[PathKey]) -> Option<Value> {
    // `Option<Value>`: `None` = JS undefined, `Some(Null)` = a null value.
    let mut current: Option<Value> = Some(obj.clone());
    for key in path {
        match current {
            // v4 top-of-loop guard: `current === null || undefined → undefined`.
            None => return None,
            Some(Value::Null) => return None,
            // typeof current !== 'object' → undefined (arrays are objects in JS).
            Some(ref v) if !v.is_object() && !v.is_array() => return None,
            Some(container) => {
                current = index_value(&container, key);
            }
        }
    }
    current
}

/// Index `container[key]` the JS way: `None` for a missing key/out-of-range index
/// (undefined), else `Some(value)` (possibly `Some(Null)`).
fn index_value(container: &Value, key: &PathKey) -> Option<Value> {
    match container {
        Value::Object(map) => {
            let k = match key {
                PathKey::Prop(s) => s.clone(),
                // JS coerces a numeric object key to string.
                PathKey::Index(n) => n.to_string(),
            };
            map.get(&k).cloned()
        }
        Value::Array(arr) => match key {
            PathKey::Index(n) => arr.get(*n).cloned(),
            // JS `array["foo"]` → undefined (non-index property).
            PathKey::Prop(_) => None,
        },
        _ => None,
    }
}

/// v4 `setAtPath`: set `value` at `path`, creating intermediate arrays/objects
/// based on the next key's type. Mutates `obj` in place. An empty path replaces the
/// root (must be an object). Returns `Err` for the "root to non-object" case (v4
/// throws a `VALIDATION_ERROR`), surfaced by the caller as the catch path.
fn set_at_path(obj: &mut Value, path: &[PathKey], value: Value) -> Result<(), String> {
    if path.is_empty() {
        // Setting root — value must be a plain object.
        if value.is_object() {
            *obj = value;
            return Ok(());
        }
        return Err("Cannot set root state to non-object value".to_string());
    }
    let mut current = obj;
    for i in 0..path.len() - 1 {
        let key = &path[i];
        let next_is_index = matches!(path[i + 1], PathKey::Index(_));
        current = descend_or_create(current, key, next_is_index);
    }
    let last = &path[path.len() - 1];
    assign(current, last, value);
    Ok(())
}

/// Ensure `current[key]` is an object/array (creating it, or overwriting a
/// primitive/null) based on whether the NEXT key is an index, then return `&mut`
/// to it — v4's intermediate-structure creation. `current` is always a container
/// (the root object, or the slot created by a prior step).
fn descend_or_create<'a>(
    current: &'a mut Value,
    key: &PathKey,
    next_is_index: bool,
) -> &'a mut Value {
    let fresh = if next_is_index {
        Value::Array(Vec::new())
    } else {
        Value::Object(Map::new())
    };
    match current {
        Value::Object(map) => {
            let k = match key {
                PathKey::Prop(s) => s.clone(),
                PathKey::Index(n) => n.to_string(),
            };
            let entry = map.entry(k).or_insert(Value::Null);
            // Create/overwrite when absent, null, or a primitive.
            if !entry.is_object() && !entry.is_array() {
                *entry = fresh;
            }
            entry
        }
        Value::Array(arr) => {
            let idx = match key {
                PathKey::Index(n) => *n,
                PathKey::Prop(s) => s.parse::<usize>().unwrap_or(0),
            };
            if idx >= arr.len() {
                arr.resize(idx + 1, Value::Null);
            }
            let entry = &mut arr[idx];
            if !entry.is_object() && !entry.is_array() {
                *entry = fresh;
            }
            entry
        }
        // Reached only with a container (root or a created slot).
        _ => current,
    }
}

/// Assign `value` at `current[last]` (v4's final `current[lastKey] = value`).
fn assign(current: &mut Value, last: &PathKey, value: Value) {
    match current {
        Value::Object(map) => {
            let k = match last {
                PathKey::Prop(s) => s.clone(),
                PathKey::Index(n) => n.to_string(),
            };
            map.insert(k, value);
        }
        Value::Array(arr) => {
            let idx = match last {
                PathKey::Index(n) => *n,
                // JS assigns a string key onto an array as a property; the corpus
                // never mixes shapes here, but keep it faithful by coercing to 0.
                PathKey::Prop(_) => 0,
            };
            if idx >= arr.len() {
                arr.resize(idx + 1, Value::Null);
            }
            arr[idx] = value;
        }
        _ => {}
    }
}

/// v4 `deleteAtPath`: remove the value at `path`. Returns whether anything was
/// deleted. Empty path (cannot delete root) → false. Array + numeric last key →
/// splice; otherwise `delete`.
fn delete_at_path(obj: &mut Value, path: &[PathKey]) -> bool {
    if path.is_empty() {
        return false;
    }
    // Walk to the parent of the last key.
    let mut current = obj;
    for key in &path[..path.len() - 1] {
        let next = match current {
            Value::Object(map) => {
                let k = match key {
                    PathKey::Prop(s) => s.clone(),
                    PathKey::Index(n) => n.to_string(),
                };
                match map.get_mut(&k) {
                    Some(v) if v.is_object() || v.is_array() => v,
                    Some(_) => return false, // primitive → not indexable
                    None => return false,    // undefined/null
                }
            }
            Value::Array(arr) => match key {
                PathKey::Index(n) => match arr.get_mut(*n) {
                    Some(v) if v.is_object() || v.is_array() => v,
                    Some(_) => return false,
                    None => return false,
                },
                PathKey::Prop(_) => return false,
            },
            _ => return false,
        };
        current = next;
    }
    let last = &path[path.len() - 1];
    match current {
        Value::Object(map) => {
            let k = match last {
                PathKey::Prop(s) => s.clone(),
                PathKey::Index(n) => n.to_string(),
            };
            if !map.contains_key(&k) {
                return false;
            }
            map.remove(&k);
            true
        }
        Value::Array(arr) => match last {
            PathKey::Index(n) => {
                if *n >= arr.len() {
                    return false;
                }
                arr.remove(*n); // Array.splice(n, 1)
                true
            }
            // `!(lastKey in current)` for a string key on an array → false.
            PathKey::Prop(_) => false,
        },
        _ => false,
    }
}

/// v4 `mergeState`: `{ ...projectState, ...chatState }` (chat overrides project,
/// top-level keys only). Insertion order: project keys first, then chat keys
/// (chat-overridden keys keep their PROJECT position — JS spread semantics).
fn merge_state(project_state: &Value, chat_state: &Value) -> Value {
    let mut out = Map::new();
    if let Value::Object(p) = project_state {
        for (k, v) in p {
            out.insert(k.clone(), v.clone());
        }
    }
    if let Value::Object(c) = chat_state {
        for (k, v) in c {
            out.insert(k.clone(), v.clone());
        }
    }
    Value::Object(out)
}

/// Extract the state object from a hydrated chat/project row (`state` is
/// `object_or_empty` → an object, or `{}`).
fn state_object(row: &Value) -> Value {
    match row.get("state") {
        Some(v) if v.is_object() => v.clone(),
        _ => Value::Object(Map::new()),
    }
}

/// v4 `validateStateInput` (`stateToolInputSchema.safeParse().success`): `operation`
/// must be one of the three enum members; `context` if present must be `chat`/
/// `project`; `path` if present must be a string; `value` is `unknown`. Extra keys
/// are stripped (not rejected) by a plain `z.object`.
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
            Some("chat" | "project") => {}
            _ => return false,
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

/// The synchronous body, run on the writer connections.
fn run_state(
    writers: &mut crate::db::runtime::WriterSet,
    ctx: &StateToolContext,
    args: &Value,
) -> StateToolOutput {
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
    let path = obj.get("path").and_then(Value::as_str).map(str::to_string);
    // v4: `value` is the raw input value; absent key → undefined (no set).
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
                // v4 `repos.projects.findById(projectId)` — hydrated (overlaid state).
                let repo = ProjectsRepository::new(main, mount_w.connection());
                match repo.find_by_id(pid) {
                    Ok(p) => p,
                    Err(e) => {
                        return StateToolOutput::fail(Value::String(operation), e.to_string())
                    }
                }
            }
            // No mount-index → the project store is unreachable (degraded); treat
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

    match operation.as_str() {
        "fetch" => run_fetch(
            operation,
            state_context,
            path,
            &parsed_path,
            &chat_state,
            &project_state,
            project.is_some(),
        ),
        "set" => run_set(
            writers,
            ctx,
            operation,
            state_context,
            path,
            &parsed_path,
            &chat_state,
            &project_state,
            project.as_ref(),
            value_present,
            value,
        ),
        "delete" => run_delete(
            writers,
            ctx,
            operation,
            state_context,
            path,
            &parsed_path,
            &chat_state,
            &project_state,
            project.as_ref(),
        ),
        // v4: `Unknown operation: ${operation}` (validation already gates this).
        other => StateToolOutput::fail(
            Value::String(operation.clone()),
            format!("Unknown operation: {other}"),
        ),
    }
}

#[allow(clippy::too_many_arguments)]
fn run_fetch(
    operation: String,
    state_context: Option<String>,
    path: Option<String>,
    parsed_path: &[PathKey],
    chat_state: &Value,
    project_state: &Value,
    has_project: bool,
) -> StateToolOutput {
    let result_value = match state_context.as_deref() {
        Some("chat") => get_at_path(chat_state, parsed_path),
        Some("project") => {
            if !has_project {
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
            get_at_path(project_state, parsed_path)
        }
        _ => {
            let merged = merge_state(project_state, chat_state);
            get_at_path(&merged, parsed_path)
        }
    };
    StateToolOutput {
        success: true,
        operation: Value::String(operation),
        context: state_context,
        path,
        // v4: `value: resultValue` — undefined (`None`) dropped, an explicit stored
        // null (`Some(Null)`) kept.
        value: result_value,
        previous_value: None,
        error: None,
    }
}

#[allow(clippy::too_many_arguments)]
fn run_set(
    writers: &mut crate::db::runtime::WriterSet,
    ctx: &StateToolContext,
    operation: String,
    state_context: Option<String>,
    path: Option<String>,
    parsed_path: &[PathKey],
    chat_state: &Value,
    project_state: &Value,
    project: Option<&Value>,
    value_present: bool,
    value: Option<Value>,
) -> StateToolOutput {
    // Underscore-prefixed user-only keys are off-limits to the AI.
    if is_underscore_key(parsed_path) {
        return StateToolOutput {
            success: false,
            operation: Value::String(operation),
            context: state_context,
            path,
            value: None,
            previous_value: None,
            error: Some(
                "Keys starting with underscore are user-only and cannot be modified by AI"
                    .to_string(),
            ),
        };
    }
    // v4: `stateContext || 'chat'`.
    let target = state_context.clone().unwrap_or_else(|| "chat".to_string());
    let set_value = value.clone().unwrap_or(Value::Null); // `value` may be undefined

    if target == "project" {
        let Some(project) = project else {
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
        let previous = get_at_path(project_state, parsed_path);
        let mut new_state = project_state.clone();
        if let Err(e) = set_at_path(&mut new_state, parsed_path, set_value.clone()) {
            return StateToolOutput::fail(Value::String(operation), e);
        }
        let pid = project
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if let Err(e) = write_project_state(writers, pid, &new_state) {
            return StateToolOutput::fail(Value::String(operation), e.to_string());
        }
        set_result(operation, target, path, value_present, value, previous)
    } else {
        let previous = get_at_path(chat_state, parsed_path);
        let mut new_state = chat_state.clone();
        if let Err(e) = set_at_path(&mut new_state, parsed_path, set_value.clone()) {
            return StateToolOutput::fail(Value::String(operation), e);
        }
        if let Err(e) = write_chat_state(writers, &ctx.chat_id, &new_state) {
            return StateToolOutput::fail(Value::String(operation), e.to_string());
        }
        set_result(operation, target, path, value_present, value, previous)
    }
}

/// The `set` success output: `{ success, operation, context, path, value,
/// previousValue }`. `value` is the raw input value (undefined → dropped);
/// `previousValue` undefined → dropped.
fn set_result(
    operation: String,
    target: String,
    path: Option<String>,
    value_present: bool,
    value: Option<Value>,
    previous: Option<Value>,
) -> StateToolOutput {
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

#[allow(clippy::too_many_arguments)]
fn run_delete(
    writers: &mut crate::db::runtime::WriterSet,
    ctx: &StateToolContext,
    operation: String,
    state_context: Option<String>,
    path: Option<String>,
    parsed_path: &[PathKey],
    chat_state: &Value,
    project_state: &Value,
    project: Option<&Value>,
) -> StateToolOutput {
    if is_underscore_key(parsed_path) {
        return StateToolOutput {
            success: false,
            operation: Value::String(operation),
            context: state_context,
            path,
            value: None,
            previous_value: None,
            error: Some(
                "Keys starting with underscore are user-only and cannot be deleted by AI"
                    .to_string(),
            ),
        };
    }
    let target = state_context.clone().unwrap_or_else(|| "chat".to_string());

    if target == "project" {
        let Some(project) = project else {
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
        let previous = get_at_path(project_state, parsed_path);
        let mut new_state = project_state.clone();
        let deleted = delete_at_path(&mut new_state, parsed_path);
        if deleted {
            let pid = project
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if let Err(e) = write_project_state(writers, pid, &new_state) {
                return StateToolOutput::fail(Value::String(operation), e.to_string());
            }
        }
        delete_result(operation, target, path, previous)
    } else {
        let previous = get_at_path(chat_state, parsed_path);
        let mut new_state = chat_state.clone();
        let deleted = delete_at_path(&mut new_state, parsed_path);
        if deleted {
            if let Err(e) = write_chat_state(writers, &ctx.chat_id, &new_state) {
                return StateToolOutput::fail(Value::String(operation), e.to_string());
            }
        }
        delete_result(operation, target, path, previous)
    }
}

/// The `delete` success output: `{ success, operation, context, path,
/// previousValue }` (no `value`).
fn delete_result(
    operation: String,
    target: String,
    path: Option<String>,
    previous: Option<Value>,
) -> StateToolOutput {
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

/// Whether the first path segment is an underscore-prefixed (user-only) string
/// key (v4: `parsedPath[0]` is a string starting with `_`).
fn is_underscore_key(parsed_path: &[PathKey]) -> bool {
    matches!(parsed_path.first(), Some(PathKey::Prop(s)) if s.starts_with('_'))
}

/// Write the chat's `state` column (`repos.chats.update(chatId, { state })`).
fn write_chat_state(
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
fn write_project_state(
    writers: &mut crate::db::runtime::WriterSet,
    project_id: &str,
    new_state: &Value,
) -> Result<(), crate::db::DbError> {
    let mount = writers
        .mount_index()
        .ok_or_else(|| crate::db::DbError::Key("project state requires mount-index".into()))?
        .connection();
    let main = writers.main().connection();
    let repo = ProjectsRepository::new(main, mount);
    let mut patch = Map::new();
    patch.insert("state".to_string(), new_state.clone());
    repo.update(project_id, &patch)
        .map_err(|e| crate::db::DbError::Key(e.to_string()))?;
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
    fn parse_path_dot_and_index() {
        assert_eq!(parse_path(None), vec![]);
        assert_eq!(parse_path(Some("")), vec![]);
        assert_eq!(
            parse_path(Some("player.inventory[0].name")),
            vec![
                PathKey::Prop("player".into()),
                PathKey::Prop("inventory".into()),
                PathKey::Index(0),
                PathKey::Prop("name".into()),
            ]
        );
        assert_eq!(
            parse_path(Some("gameState.turn")),
            vec![
                PathKey::Prop("gameState".into()),
                PathKey::Prop("turn".into())
            ]
        );
    }

    #[test]
    fn get_at_path_walks() {
        let obj = json!({ "player": { "health": 5, "inv": [ { "name": "sword" } ] } });
        assert_eq!(
            get_at_path(&obj, &parse_path(Some("player.health"))),
            Some(json!(5))
        );
        assert_eq!(
            get_at_path(&obj, &parse_path(Some("player.inv[0].name"))),
            Some(json!("sword"))
        );
        assert_eq!(get_at_path(&obj, &parse_path(Some("player.missing"))), None);
        assert_eq!(get_at_path(&obj, &[]), Some(obj.clone()));
    }

    #[test]
    fn set_at_path_creates_intermediate() {
        let mut obj = json!({});
        set_at_path(&mut obj, &parse_path(Some("a.b.c")), json!(1)).unwrap();
        assert_eq!(obj, json!({ "a": { "b": { "c": 1 } } }));

        let mut obj2 = json!({});
        set_at_path(&mut obj2, &parse_path(Some("list[2]")), json!("x")).unwrap();
        assert_eq!(obj2, json!({ "list": [null, null, "x"] }));

        // Root set to non-object → error.
        let mut obj3 = json!({ "k": 1 });
        assert!(set_at_path(&mut obj3, &[], json!(5)).is_err());
    }

    #[test]
    fn delete_at_path_object_and_array() {
        let mut obj = json!({ "a": { "b": 1, "c": 2 } });
        assert!(delete_at_path(&mut obj, &parse_path(Some("a.b"))));
        assert_eq!(obj, json!({ "a": { "c": 2 } }));
        // Missing → false.
        assert!(!delete_at_path(&mut obj, &parse_path(Some("a.z"))));

        let mut arr = json!({ "l": [10, 20, 30] });
        assert!(delete_at_path(&mut arr, &parse_path(Some("l[1]"))));
        assert_eq!(arr, json!({ "l": [10, 30] }));
    }

    #[test]
    fn merge_chat_overrides_project() {
        let p = json!({ "shared": "p", "onlyP": 1 });
        let c = json!({ "shared": "c", "onlyC": 2 });
        // Chat overrides `shared`; project position preserved.
        assert_eq!(
            merge_state(&p, &c),
            json!({ "shared": "c", "onlyP": 1, "onlyC": 2 })
        );
    }

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
