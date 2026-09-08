//! The five prompt-template verbs (P4.83): v4's
//! `app/api/v1/prompt-templates/route.ts` (GET list / POST create) and
//! `[id]/route.ts` (GET / PUT / DELETE), as dispatch handlers over
//! [`crate::db::prompt_templates`]. The REST edges
//! (`quilltap-web/src/prompt_templates_routes.rs`) and the dispatch arms both
//! land here, so the guard ladders live in ONE place.
//!
//! ## The lazy seeding — v4's site, not a startup hook
//!
//! Every read v4's routes make goes through `findAllForUser` / `findById`, and
//! BOTH call `seedSamplePrompts()` first. So the 21 built-in "Sample Prompts"
//! appear the first time anybody opens the Import-from-Template modal — and the
//! first time anybody GETs a single template, or PUTs one, or DELETEs one.
//! [`crate::services::builtin_prompt_templates`] is the seeder; every handler
//! below that reaches a read calls [`seed_sample_prompts_if_needed`] first.
//! `prompt_templates_routes_equivalence` measures the whole thing, including
//! the two arms where seeding must NOT happen: v4's POST and PUT parse the body
//! BEFORE any read, so a Zod-refused body leaves the table exactly as it found
//! it (`create_zod_*_400` / `put_zod_*_400`, all `table = 1`).
//!
//! ## The guard ladders — MEASURED on v4's real handlers
//!
//! - **POST** — `createTemplateSchema.parse(body)` first (a ZodError is the
//!   context middleware's `validationError` 400), then the write.
//! - **PUT** — `updateTemplateSchema.parse(body)` FIRST, then `findById` → 404,
//!   then the built-in refusal → 403. So a bad body against a missing id is 400,
//!   not 404 (`put_zod_beats_missing_404`).
//! - **GET [id]** / **DELETE** — `findById` → 404 first (no body to parse), then
//!   (DELETE only) the built-in refusal → 403.
//! - The built-in refusals are the ROUTE's, above the repository's own silent
//!   guard: `forbidden('Cannot update built-in templates')` /
//!   `forbidden('Cannot delete built-in templates')`, both 403, and the repo's
//!   `null` / `false` arm is unreachable through these routes.
//!
//! ## The wire shapes — three DIFFERENT null contracts, all measured
//!
//! v4's SQLite backend hydrates every SQL NULL to `undefined`
//! (`backend.ts:404-451`, "null → undefined for Zod .optional() compatibility"),
//! and `PromptTemplateSchema`'s four nullable columns are `.nullable()
//! .optional()`. So:
//!
//!   - a **READ** body (the list, and `GET [id]`) **OMITS** a null column
//!     entirely — `userId` is absent on a built-in, `description` / `category` /
//!     `modelHint` are absent on a bare user template. That is why
//!     [`crate::db::prompt_templates::PromptTemplateRecord`] carries
//!     `skip_serializing_if`. (The work order's §B declared `description: string
//!     | null`; the wire says the key is not there at all. Measured 2026-09-07,
//!     `list_first_seeds_21` / `get_user_template_200`.)
//!   - a **CREATE** 201 body carries explicit `null`s, because v4 validates the
//!     INPUT object (`description: validatedData.description || null`) and never
//!     re-reads the row.
//!   - an **UPDATE** 200 body is `{...existing, ...updates}` — the read record
//!     (omitting) with the PROVIDED patch keys laid over it (each `|| null`), so
//!     a cleared field is present-as-null and an untouched null field stays
//!     absent. Zod re-emits the merge in schema key order either way.
//!
//! ## What is NOT ported (recorded, not silent)
//!
//! v4's repository layer logs generically around every op (`Entity created`,
//! `Prompt template created successfully`, `Prompt template not found for
//! update`, the `Safe validation failed` warn behind each dropped row). v5's
//! `db::` layer has no logging anywhere, by design, so those lines have no
//! counterpart here and the differential compares only the two families this
//! port owns: the seed line and the route's `[Prompt Templates v1] …` lines.

use serde_json::{json, Map, Value};

use crate::clock;
use crate::db::prompt_templates::{self, CreateOptions, PromptTemplateRecord, PtCreate, PtUpdate};
use crate::db::runtime::Db;
use crate::db::DbError;
use crate::services::builtin_prompt_templates;
use crate::services::chat_create::{create_issue_details, CreateZodIssue};

use super::types::{ErrorKind, Response};

/// v4's routes log through the global `logger` with a `[Prompt Templates v1]`
/// message prefix; the seeder logs bare from the repository.
pub const LOG_TARGET: &str = "quilltap::prompt_templates";

/// `PromptTemplateSchema`'s key order — what Zod re-emits whatever the input
/// object's order was (`lib/schemas/template.types.ts:270-282`).
const SCHEMA_KEYS: [&str; 11] = [
    "id",
    "userId",
    "name",
    "content",
    "description",
    "isBuiltIn",
    "category",
    "modelHint",
    "tags",
    "createdAt",
    "updatedAt",
];

fn not_found(resource: &str) -> Response {
    Response::error(ErrorKind::NotFound, format!("{resource} not found"))
}
fn forbidden(msg: &str) -> Response {
    Response::error(ErrorKind::Forbidden, msg.to_string())
}
fn internal(msg: &str) -> Response {
    Response::error(ErrorKind::Internal, msg.to_string())
}

// ── The Zod ladder ──────────────────────────────────────────────────────────

fn zod_received(v: Option<&Value>) -> &'static str {
    match v {
        None => "undefined",
        Some(Value::Null) => "null",
        Some(Value::Bool(_)) => "boolean",
        Some(Value::Number(_)) => "number",
        Some(Value::String(_)) => "string",
        Some(Value::Array(_)) => "array",
        Some(Value::Object(_)) => "object",
    }
}

fn invalid_type(expected: &'static str, path: Vec<Value>, got: Option<&Value>) -> CreateZodIssue {
    CreateZodIssue::InvalidType {
        expected,
        code: "invalid_type",
        path,
        message: format!(
            "Invalid input: expected {expected}, received {}",
            zod_received(got)
        ),
    }
}

/// Zod's own `.min(1)` sentence — what `updateTemplateSchema` reports, since it
/// passes no custom message.
const ZOD_MIN_1: &str = "Too small: expected string to have >=1 characters";

/// One `z.string()` field of either schema.
///
/// `min` is `Some(<message>)` when the field HAS a `.min(1)` and carries that
/// sentence, and `None` when it has none at all. That distinction is the whole
/// of `create_empty_optionals_become_null_201`: `description` / `category` /
/// `modelHint` are bare `z.string()`s, so `''` is VALID and the route's
/// `|| null` then stores NULL — a hand-added `.min(1)` would 400 a body v4
/// accepts. Zod 4.5 counts CODE POINTS.
fn check_string(
    v: Option<&Value>,
    key: &'static str,
    min: Option<&'static str>,
    max: Option<usize>,
    optional: bool,
    issues: &mut Vec<CreateZodIssue>,
) {
    let path = vec![Value::String(key.to_string())];
    let Some(v) = v else {
        if !optional {
            issues.push(invalid_type("string", path, None));
        }
        return;
    };
    let Some(s) = v.as_str() else {
        issues.push(invalid_type("string", path, Some(v)));
        return;
    };
    if let Some(message) = min {
        if !crate::jsstr::zod_len_min_ok(s, 1) {
            issues.push(CreateZodIssue::TooSmall {
                origin: "string",
                code: "too_small",
                minimum: json!(1),
                inclusive: true,
                path,
                message: message.to_string(),
            });
            return;
        }
    }
    if let Some(max) = max {
        if !crate::jsstr::zod_len_max_ok(s, max) {
            issues.push(CreateZodIssue::TooBig {
                origin: "string",
                code: "too_big",
                maximum: json!(max),
                inclusive: true,
                path,
                message: format!("Too big: expected string to have <={max} characters"),
            });
        }
    }
}

/// `schema.parse(<non-object>)` — ONE issue, `invalid_type expected object` at
/// path `[]`. The REST edge renders this for a body that is not an object at
/// all; the handlers reach it too, because both dispatch variants carry the raw
/// body.
pub fn body_not_object_details(body: &Value) -> Value {
    create_issue_details(&[invalid_type("object", vec![], Some(body))])
}

/// v4 `createTemplateSchema`: `{name: min(1,'Name is required').max(100),
/// content: min(1,'Content is required'), description?: max(500), category?,
/// modelHint?}`. Issues report in schema key order.
fn parse_create_body(body: &Value) -> Result<CreateInput, Response> {
    let Some(obj) = body.as_object() else {
        return Err(Response::validation_error(body_not_object_details(body)));
    };
    let mut issues = Vec::new();
    check_string(
        obj.get("name"),
        "name",
        Some("Name is required"),
        Some(100),
        false,
        &mut issues,
    );
    check_string(
        obj.get("content"),
        "content",
        Some("Content is required"),
        None,
        false,
        &mut issues,
    );
    check_string(
        obj.get("description"),
        "description",
        None,
        Some(500),
        true,
        &mut issues,
    );
    check_string(
        obj.get("category"),
        "category",
        None,
        None,
        true,
        &mut issues,
    );
    check_string(
        obj.get("modelHint"),
        "modelHint",
        None,
        None,
        true,
        &mut issues,
    );
    if !issues.is_empty() {
        return Err(Response::validation_error(create_issue_details(&issues)));
    }
    let s = |k: &str| obj.get(k).and_then(Value::as_str).map(str::to_string);
    Ok(CreateInput {
        name: s("name").unwrap_or_default(),
        content: s("content").unwrap_or_default(),
        // v4 `validatedData.description || null` — an EMPTY string is falsy, so
        // `''` stores NULL, not `''`.
        description: s("description").filter(|v| !v.is_empty()),
        category: s("category").filter(|v| !v.is_empty()),
        model_hint: s("modelHint").filter(|v| !v.is_empty()),
    })
}

struct CreateInput {
    name: String,
    content: String,
    description: Option<String>,
    category: Option<String>,
    model_hint: Option<String>,
}

/// v4 `updateTemplateSchema`: every field optional, Zod's own min/max sentences.
/// Returns the patch plus `Object.keys(updates)` — the PRESENT keys in the
/// route's own assignment order, which is what the info line reports.
fn parse_update_body(body: &Value) -> Result<(UpdateInput, Vec<&'static str>), Response> {
    let Some(obj) = body.as_object() else {
        return Err(Response::validation_error(body_not_object_details(body)));
    };
    let mut issues = Vec::new();
    check_string(
        obj.get("name"),
        "name",
        Some(ZOD_MIN_1),
        Some(100),
        true,
        &mut issues,
    );
    check_string(
        obj.get("content"),
        "content",
        Some(ZOD_MIN_1),
        None,
        true,
        &mut issues,
    );
    check_string(
        obj.get("description"),
        "description",
        None,
        Some(500),
        true,
        &mut issues,
    );
    check_string(
        obj.get("category"),
        "category",
        None,
        None,
        true,
        &mut issues,
    );
    check_string(
        obj.get("modelHint"),
        "modelHint",
        None,
        None,
        true,
        &mut issues,
    );
    if !issues.is_empty() {
        return Err(Response::validation_error(create_issue_details(&issues)));
    }
    let s = |k: &str| obj.get(k).and_then(Value::as_str).map(str::to_string);
    let mut keys: Vec<&'static str> = Vec::new();
    let name = s("name");
    if name.is_some() {
        keys.push("name");
    }
    let content = s("content");
    if content.is_some() {
        keys.push("content");
    }
    // The three nullable columns are TRI-STATE on the wire: absent leaves the
    // column alone, present sets it to `value || null`.
    let description = s("description").map(|v| (!v.is_empty()).then_some(v));
    if description.is_some() {
        keys.push("description");
    }
    let category = s("category").map(|v| (!v.is_empty()).then_some(v));
    if category.is_some() {
        keys.push("category");
    }
    let model_hint = s("modelHint").map(|v| (!v.is_empty()).then_some(v));
    if model_hint.is_some() {
        keys.push("modelHint");
    }
    keys.push("updatedAt");
    Ok((
        UpdateInput {
            name,
            content,
            description,
            category,
            model_hint,
        },
        keys,
    ))
}

struct UpdateInput {
    name: Option<String>,
    content: Option<String>,
    description: Option<Option<String>>,
    category: Option<Option<String>>,
    model_hint: Option<Option<String>>,
}

// ── The seeding site ────────────────────────────────────────────────────────

/// v4 `seedSamplePrompts()`, at v4's site: before every read. The catalogue
/// probe runs on the READ pool, so a list request on an already-seeded instance
/// never takes the writer; the inserts re-run the same per-name lookup under the
/// write lock, so the decision is never acted on stale.
///
/// v4 wraps the whole seed in `safeQuery(..., 'Error seeding sample prompts',
/// {}, undefined)` — a failure is logged and SWALLOWED, and the read proceeds.
/// Reproduced.
async fn seed_sample_prompts_if_needed(db: &Db) {
    let needed = match db.read_main(builtin_prompt_templates::needs_seeding) {
        Ok(n) => n,
        Err(e) => {
            tracing::error!(target: LOG_TARGET, error = %e, "Error seeding sample prompts");
            return;
        }
    };
    if !needed {
        return;
    }
    let written = db
        .write(|w| {
            builtin_prompt_templates::seed_sample_prompts(w.main().connection(), &mut || {
                uuid::Uuid::new_v4().to_string()
            })
        })
        .await;
    if let Err(e) = written {
        tracing::error!(target: LOG_TARGET, error = %e, "Error seeding sample prompts");
    }
}

// ── Wire shaping ────────────────────────────────────────────────────────────

/// The READ wire body for one row (nulls OMITTED).
fn read_json(rec: &PromptTemplateRecord) -> Value {
    serde_json::to_value(rec).unwrap_or(Value::Null)
}

/// Re-emit `obj` in `PromptTemplateSchema`'s key order, keeping only the keys it
/// actually carries — v4's Zod parse over `{...existing, ...updates}`.
fn schema_order(obj: Map<String, Value>) -> Value {
    let mut out = Map::new();
    for key in SCHEMA_KEYS {
        if let Some(v) = obj.get(key) {
            out.insert(key.to_string(), v.clone());
        }
    }
    Value::Object(out)
}

fn ok_template(v: Value) -> Response {
    Response::PromptTemplate(json!({ "template": v }))
}

// ── GET /api/v1/prompt-templates ────────────────────────────────────────────

pub async fn prompt_template_list(db: &Db, user_id: &str) -> Response {
    seed_sample_prompts_if_needed(db).await;
    // v4's `safeQuery` fallback for this read is `[]` — a failure answers 200
    // with an empty catalogue rather than an error. Reproduced.
    let templates = match db.read_main(|c| prompt_templates::find_all_for_user(c, user_id)) {
        Ok(v) => v,
        Err(e) => {
            tracing::error!(
                target: LOG_TARGET,
                user_id = %user_id,
                error = %e,
                "Error finding all prompt templates for user"
            );
            Vec::new()
        }
    };
    let count = templates.len();
    let rows: Vec<Value> = templates.iter().map(read_json).collect();
    Response::PromptTemplates(json!({ "templates": rows, "count": count }))
}

// ── POST /api/v1/prompt-templates ───────────────────────────────────────────

pub async fn prompt_template_create(db: &Db, user_id: &str, body: &Value) -> Response {
    let input = match parse_create_body(body) {
        Ok(i) => i,
        Err(r) => return r,
    };
    let id = uuid::Uuid::new_v4().to_string();
    let now = clock::now_iso();

    let create = PtCreate {
        user_id: Some(user_id.to_string()),
        name: input.name.clone(),
        content: input.content.clone(),
        description: input.description.clone(),
        is_built_in: false,
        category: input.category.clone(),
        model_hint: input.model_hint.clone(),
        tags: Vec::new(),
    };
    let opts = CreateOptions {
        id: id.clone(),
        created_at: now.clone(),
        updated_at: now.clone(),
    };
    if let Err(e) = db
        .write(move |w| {
            // v4's `_create` goes through `getCollection()` → `ensureCollection`
            // (`base.repository.ts:100-114`, `:345`): a POST on an instance that
            // has never had the table answers 201, not `no such table`. The §3
            // unification review's catch — the read paths had the ensure, the
            // create path did not.
            prompt_templates::ensure_prompt_templates_table(w.main().connection())?;
            prompt_templates::PromptTemplatesRepository::new(w.main().connection())
                .create(&create, &opts)
        })
        .await
    {
        tracing::error!(
            target: LOG_TARGET,
            user_id = %user_id,
            error = %e,
            "[Prompt Templates v1] Error creating template"
        );
        return internal("Internal server error");
    }

    tracing::info!(
        target: LOG_TARGET,
        template_id = %id,
        user_id = %user_id,
        name = %input.name,
        "[Prompt Templates v1] Template created"
    );

    // v4 answers `_create`'s VALIDATED INPUT, not a re-read: the nullable
    // columns are explicit `null`s here, never omitted.
    ok_template(json!({
        "id": id,
        "userId": user_id,
        "name": input.name,
        "content": input.content,
        "description": input.description,
        "isBuiltIn": false,
        "category": input.category,
        "modelHint": input.model_hint,
        "tags": [],
        "createdAt": now,
        "updatedAt": now,
    }))
}

// ── GET /api/v1/prompt-templates/[id] ───────────────────────────────────────

pub async fn prompt_template_get(db: &Db, user_id: &str, id: &str) -> Response {
    seed_sample_prompts_if_needed(db).await;
    match find_one(db, id) {
        Ok(Some(rec)) => ok_template(read_json(&rec)),
        Ok(None) => {
            tracing::warn!(
                target: LOG_TARGET,
                template_id = %id,
                user_id = %user_id,
                "[Prompt Templates v1] Template not found"
            );
            not_found("Template")
        }
        Err(_) => {
            // v4's `findById` safeQuery fallback is `null`, which the route
            // reports as the same 404.
            tracing::warn!(
                target: LOG_TARGET,
                template_id = %id,
                user_id = %user_id,
                "[Prompt Templates v1] Template not found"
            );
            not_found("Template")
        }
    }
}

fn find_one(db: &Db, id: &str) -> Result<Option<PromptTemplateRecord>, DbError> {
    db.read_main(|c| prompt_templates::find_by_id(c, id))
}

// ── PUT /api/v1/prompt-templates/[id] ───────────────────────────────────────

pub async fn prompt_template_update(db: &Db, user_id: &str, id: &str, body: &Value) -> Response {
    // v4 parses BEFORE the lookup, so this 400 beats the 404.
    let (patch, updated_fields) = match parse_update_body(body) {
        Ok(p) => p,
        Err(r) => return r,
    };
    seed_sample_prompts_if_needed(db).await;

    let existing = match find_one(db, id) {
        Ok(Some(r)) => r,
        Ok(None) | Err(_) => {
            tracing::warn!(
                target: LOG_TARGET,
                template_id = %id,
                user_id = %user_id,
                "[Prompt Templates v1] Template not found for update"
            );
            return not_found("Template");
        }
    };
    if existing.is_built_in {
        tracing::warn!(
            target: LOG_TARGET,
            template_id = %id,
            user_id = %user_id,
            "[Prompt Templates v1] Attempted to update built-in template"
        );
        return forbidden("Cannot update built-in templates");
    }

    let now = clock::now_iso();
    let db_patch = PtUpdate {
        name: patch.name.clone(),
        content: patch.content.clone(),
        description: patch.description.clone(),
        category: patch.category.clone(),
        model_hint: patch.model_hint.clone(),
        tags: None,
        updated_at: now.clone(),
    };
    let target = id.to_string();
    let applied = db
        .write(move |w| {
            prompt_templates::PromptTemplatesRepository::new(w.main().connection())
                .update(&target, &db_patch)
        })
        .await;
    match applied {
        Ok(true) => {}
        Ok(false) | Err(_) => {
            tracing::error!(
                target: LOG_TARGET,
                template_id = %id,
                user_id = %user_id,
                "[Prompt Templates v1] Failed to update template"
            );
            return internal("Failed to update template");
        }
    }

    tracing::info!(
        target: LOG_TARGET,
        template_id = %id,
        user_id = %user_id,
        updated_fields = ?updated_fields,
        "[Prompt Templates v1] Template updated"
    );

    // v4 answers `{...existing, ...updates}` re-emitted in schema order: the
    // read record (which OMITS null columns) with the provided patch keys laid
    // over it, each already `|| null`.
    let mut merged = match read_json(&existing) {
        Value::Object(m) => m,
        _ => Map::new(),
    };
    if let Some(v) = &patch.name {
        merged.insert("name".into(), json!(v));
    }
    if let Some(v) = &patch.content {
        merged.insert("content".into(), json!(v));
    }
    if let Some(v) = &patch.description {
        merged.insert("description".into(), json!(v));
    }
    if let Some(v) = &patch.category {
        merged.insert("category".into(), json!(v));
    }
    if let Some(v) = &patch.model_hint {
        merged.insert("modelHint".into(), json!(v));
    }
    merged.insert("updatedAt".into(), json!(now));
    ok_template(schema_order(merged))
}

// ── DELETE /api/v1/prompt-templates/[id] ────────────────────────────────────

pub async fn prompt_template_delete(db: &Db, user_id: &str, id: &str) -> Response {
    seed_sample_prompts_if_needed(db).await;

    let existing = match find_one(db, id) {
        Ok(Some(r)) => r,
        Ok(None) | Err(_) => {
            tracing::warn!(
                target: LOG_TARGET,
                template_id = %id,
                user_id = %user_id,
                "[Prompt Templates v1] Template not found for deletion"
            );
            return not_found("Template");
        }
    };
    if existing.is_built_in {
        tracing::warn!(
            target: LOG_TARGET,
            template_id = %id,
            user_id = %user_id,
            "[Prompt Templates v1] Attempted to delete built-in template"
        );
        return forbidden("Cannot delete built-in templates");
    }

    let target = id.to_string();
    let deleted = db
        .write(move |w| {
            prompt_templates::PromptTemplatesRepository::new(w.main().connection()).delete(&target)
        })
        .await;
    match deleted {
        Ok(true) => {}
        Ok(false) | Err(_) => {
            tracing::error!(
                target: LOG_TARGET,
                template_id = %id,
                user_id = %user_id,
                "[Prompt Templates v1] Failed to delete template"
            );
            return internal("Failed to delete template");
        }
    }

    tracing::info!(
        target: LOG_TARGET,
        template_id = %id,
        user_id = %user_id,
        "[Prompt Templates v1] Template deleted"
    );
    Response::PromptTemplateDeleted(json!({ "success": true }))
}

/// The flat dispatch variants' five tri-states, folded back into the object body
/// the Zod ladders read (absent → key absent; `Some(None)` → `null`;
/// `Some(Some(v))` → `v`). The `subprompts::flat_body` shape.
pub fn flat_body(
    name: Option<Option<Value>>,
    content: Option<Option<Value>>,
    description: Option<Option<Value>>,
    category: Option<Option<Value>>,
    model_hint: Option<Option<Value>>,
) -> Value {
    let mut obj = Map::new();
    for (key, v) in [
        ("name", name),
        ("content", content),
        ("description", description),
        ("category", category),
        ("modelHint", model_hint),
    ] {
        if let Some(v) = v {
            obj.insert(key.to_string(), v.unwrap_or(Value::Null));
        }
    }
    Value::Object(obj)
}
