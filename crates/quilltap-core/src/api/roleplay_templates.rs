//! The roleplay-templates dispatch handlers (P4.6p) — a differential port of v4's
//! `roleplay-templates/route.ts` (+ `[id]/route.ts`), composed over the ported
//! `db::roleplay_templates` repo (create/update/delete) + the new full-JSON reads,
//! plus the pure `services::annotations::generate_template_rendering_patterns`.
//!
//! P4.D351 (v4 `778b97b2b`, the Novel template, at the `236c04e35` pin): POST
//! admits an explicit `null` narration (unmarked prose) and refuses a bad
//! `unmarkedStyle` / a non-boolean `markdownAsterisks`; every create, an empty-
//! patterns GET, and a PUT that changes any of the four authoring fields
//! (`delimiters`, `narrationDelimiters`, `unmarkedStyle`, `markdownAsterisks`)
//! derive the patterns through ONE `generate_template_rendering_patterns`; the
//! two DEBUG lines and the route-level INFO lines are v4's.
//!
//! Response texture (per v4's `successResponse`): the LIST is a **bare JSON
//! array**; create/get/update return the bare template; delete returns
//! `{success, deletedId}`. `user_id` is a parameter (the differential drives with
//! the fixture's own user id; the engine passes `SINGLE_USER_ID`).

use serde_json::{json, Value};

use crate::collation::locale_compare;
use crate::db::roleplay_templates::{
    delimiters_from_value, find_all_for_user, find_full_json_by_id, find_id_by_user_and_name,
    CreateOptions, DialogueDetection, RenderingPattern, RoleplayTemplatesRepository, RtCreate,
    RtUpdate, StringOrPair, TemplateDelimiter, UnmarkedStyle,
};
use crate::db::runtime::Db;
use crate::jsstr::utf16_len;
use crate::services::annotations::generate_template_rendering_patterns;

use super::types::{ErrorKind, Response};

/// v4's middleware `validationError(zodError)` — `{error: 'Validation error',
/// details: [issues]}` at 400.
fn validation_error(issues: &[ZodIssue]) -> Response {
    Response::validation_error(Value::Array(
        issues.iter().map(ZodIssue::to_value).collect(),
    ))
}
use super::zod_issues::{
    key, zod_narration_delimiters_update_issues, zod_unmarked_style_issues, ZodIssue,
};

// ===========================================================================
// Response helpers
// ===========================================================================

fn internal(e: impl std::fmt::Display) -> Response {
    Response::error(ErrorKind::Internal, e.to_string())
}
fn not_found(resource: &str) -> Response {
    Response::error(ErrorKind::NotFound, format!("{resource} not found"))
}
fn bad_request(msg: impl Into<String>) -> Response {
    Response::error(ErrorKind::BadRequest, msg)
}
fn conflict(msg: impl Into<String>) -> Response {
    Response::error(ErrorKind::Conflict, msg)
}
fn forbidden(msg: impl Into<String>) -> Response {
    Response::error(ErrorKind::Forbidden, msg)
}

/// v4's POST 400 for a missing / `''` narration (`route.ts:82-84`).
const NARRATION_REQUIRED: &str = "Narration delimiters are required (null for unmarked prose)";

/// `issues[0]?.message` — the first Zod issue's sentence.
fn issue_message(issue: &ZodIssue) -> &str {
    issue.message()
}

/// JS truthiness for a JSON value (`!value` inversion): `null`/`false`/`0`/`""`
/// are falsy; every non-empty string, non-zero number, array, and object is
/// truthy. (v4's narration-delimiters + regen guards test truthiness directly.)
fn js_truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().map(|f| f != 0.0).unwrap_or(false),
        Value::String(s) => !s.is_empty(),
        Value::Array(_) | Value::Object(_) => true,
    }
}

// ===========================================================================
// Read-time rendering-pattern auto-generation (shared by GET + the update arms)
// ===========================================================================

/// A stored / sent `narrationDelimiters` value as the generator takes it:
/// `null` (unmarked prose) and an unparseable value are `None`.
fn narration_of(v: Option<&Value>) -> Option<StringOrPair> {
    v.filter(|v| !v.is_null())
        .and_then(|v| serde_json::from_value::<StringOrPair>(v.clone()).ok())
}

/// A stored / sent `unmarkedStyle` value as the generator takes it: absent /
/// `null` are `None`.
fn unmarked_of(v: Option<&Value>) -> Option<UnmarkedStyle> {
    v.filter(|v| !v.is_null())
        .and_then(|v| serde_json::from_value::<UnmarkedStyle>(v.clone()).ok())
}

/// v4 `generateTemplateRenderingPatterns(template)` over a template JSON object
/// (the hydrated row): `delimiters ?? []`, the decoded narration, the
/// `unmarkedStyle`, and `markdownAsterisks` passed through as found (absent →
/// `undefined`, so no literal pair).
fn template_patterns_of(t: &Value) -> Vec<RenderingPattern> {
    let delimiters = t
        .get("delimiters")
        .map(delimiters_from_value)
        .unwrap_or_default();
    generate_template_rendering_patterns(
        Some(&delimiters),
        narration_of(t.get("narrationDelimiters")).as_ref(),
        unmarked_of(t.get("unmarkedStyle")).as_ref(),
        t.get("markdownAsterisks").and_then(Value::as_bool),
    )
}

/// v4 GET-\[id\]'s read-time regen: when `renderingPatterns` is absent or
/// empty, derive the patterns on the fly (NON-persisted) — unconditionally
/// (the pin dropped the old `delimiters || narration` condition). Mutates `t`
/// in place.
fn apply_read_time_regen(t: &mut Value) {
    let patterns_empty = t
        .get("renderingPatterns")
        .and_then(Value::as_array)
        .map(|a| a.is_empty())
        .unwrap_or(true);
    if patterns_empty {
        let generated = template_patterns_of(t);
        if let Some(obj) = t.as_object_mut() {
            obj.insert(
                "renderingPatterns".into(),
                serde_json::to_value(generated).unwrap_or(Value::Array(Vec::new())),
            );
        }
    }
}

// ===========================================================================
// Handlers
// ===========================================================================

/// v4 `GET /api/v1/roleplay-templates` — `findAllForUser` → built-in-first, then
/// `name.localeCompare` (ICU4X en-US). A **bare JSON array**.
pub fn roleplay_template_list(db: &Db, user_id: &str) -> Response {
    match db.read_main(|main| find_all_for_user(main, user_id)) {
        Ok(mut templates) => {
            templates.sort_by(|a, b| {
                let ab = a.get("isBuiltIn").and_then(Value::as_bool).unwrap_or(false);
                let bb = b.get("isBuiltIn").and_then(Value::as_bool).unwrap_or(false);
                if ab != bb {
                    // Built-in templates first.
                    return if ab {
                        std::cmp::Ordering::Less
                    } else {
                        std::cmp::Ordering::Greater
                    };
                }
                let an = a.get("name").and_then(Value::as_str).unwrap_or("");
                let bn = b.get("name").and_then(Value::as_str).unwrap_or("");
                locale_compare(an, bn)
            });
            Response::RoleplayTemplate(Value::Array(templates))
        }
        Err(e) => internal(e),
    }
}

/// v4 `GET /api/v1/roleplay-templates/[id]` — 404 on absent; read-time
/// renderingPatterns auto-gen (non-persisted); the bare template.
pub fn roleplay_template_get(db: &Db, template_id: &str) -> Response {
    match db.read_main(|main| find_full_json_by_id(main, template_id)) {
        Ok(Some(mut t)) => {
            apply_read_time_regen(&mut t);
            Response::RoleplayTemplate(t)
        }
        Ok(None) => not_found("Roleplay template"),
        Err(e) => internal(e),
    }
}

/// v4 `POST /api/v1/roleplay-templates` — the hand-rolled validation (exact
/// messages, in order), duplicate-name 409, delimiters validation, create-time
/// auto-regen, then the 201 bare created template.
pub async fn roleplay_template_create(db: &Db, user_id: &str, body: Value) -> Response {
    // 'Name is required': !name || typeof !== 'string' || trim empty.
    let name = match body.get("name").and_then(Value::as_str) {
        Some(s) if !s.trim().is_empty() => s,
        _ => return bad_request("Name is required"),
    };
    let name_trimmed = name.trim().to_string();
    if utf16_len(&name_trimmed) > 100 {
        return bad_request("Name must be 100 characters or less");
    }
    // 'System prompt is required'.
    let system_prompt = match body.get("systemPrompt").and_then(Value::as_str) {
        Some(s) if !s.trim().is_empty() => s,
        _ => return bad_request("System prompt is required"),
    };
    // 'Description must be 500 characters or less' (only when a string is present).
    if let Some(d) = body.get("description").and_then(Value::as_str) {
        if utf16_len(d) > 500 {
            return bad_request("Description must be 500 characters or less");
        }
    }
    // narrationDelimiters (required; an explicit null means narration is
    // unmarked plain prose) — v4's exact branch structure.
    let narration = match body.get("narrationDelimiters") {
        None => return bad_request(NARRATION_REQUIRED),
        Some(Value::String(s)) if s.is_empty() => return bad_request(NARRATION_REQUIRED),
        Some(v) => v.clone(),
    };
    match &narration {
        // Unmarked narration — nothing further to check.
        Value::Null => {}
        Value::String(s) => {
            // Unreachable after the `''` arm above — kept, as v4 keeps it.
            if s.is_empty() {
                return bad_request("Narration delimiters string must not be empty");
            }
        }
        Value::Array(a) => {
            let ok = a.len() == 2
                && a.first()
                    .and_then(Value::as_str)
                    .is_some_and(|x| !x.is_empty())
                && a.get(1)
                    .and_then(Value::as_str)
                    .is_some_and(|x| !x.is_empty());
            if !ok {
                return bad_request(
                    "Narration delimiters array must have exactly 2 non-empty elements [open, close]",
                );
            }
        }
        _ => return bad_request("Narration delimiters must be a string or [string, string] array"),
    }
    // Duplicate name (user-scoped — a user CAN shadow a built-in name).
    match db.read_main(|m| find_id_by_user_and_name(m, user_id, &name_trimmed)) {
        Ok(Some(_)) => return conflict("A roleplay template with this name already exists"),
        Ok(None) => {}
        Err(e) => return internal(e),
    }
    // delimiters: z.array(TemplateDelimiterSchema).optional().
    let delimiters: Vec<TemplateDelimiter> = match body.get("delimiters") {
        None | Some(Value::Null) => Vec::new(),
        Some(v) => match parse_delimiters_strict(v) {
            Ok(d) => d,
            Err(msg) => return bad_request(format!("Invalid delimiters: {msg}")),
        },
    };
    // unmarkedStyle: UnmarkedStyleSchema.nullable().optional().safeParse(..).
    if let Some(first) = zod_unmarked_style_issues(body.get("unmarkedStyle"), &[]).first() {
        return bad_request(format!("Invalid unmarked style: {}", issue_message(first)));
    }
    let unmarked_style: Option<UnmarkedStyle> = unmarked_of(body.get("unmarkedStyle"));
    // markdownAsterisks: undefined or a boolean (`?? true`).
    let markdown_asterisks = match body.get("markdownAsterisks") {
        None => true,
        Some(Value::Bool(b)) => *b,
        Some(_) => return bad_request("markdownAsterisks must be a boolean"),
    };
    let narration_typed: Option<StringOrPair> = match &narration {
        Value::Null => None,
        v => match serde_json::from_value(v.clone()) {
            Ok(nd) => Some(nd),
            Err(e) => return bad_request(format!("Invalid narrationDelimiters: {e}")),
        },
    };
    // renderingPatterns: `body.renderingPatterns || []`; derived whenever empty.
    let mut rendering_patterns: Vec<RenderingPattern> = match body.get("renderingPatterns") {
        Some(v) if js_truthy(v) => serde_json::from_value(v.clone()).unwrap_or_default(),
        _ => Vec::new(),
    };
    if rendering_patterns.is_empty() {
        rendering_patterns = generate_template_rendering_patterns(
            Some(&delimiters),
            narration_typed.as_ref(),
            unmarked_style.as_ref(),
            Some(markdown_asterisks),
        );
    }
    tracing::debug!(
        delimiterCount = delimiters.len(),
        patternCount = rendering_patterns.len(),
        unmarkedNarration = narration_typed.is_none(),
        markdownAsterisks = markdown_asterisks,
        "Roleplay template patterns resolved for create"
    );
    // description: description?.trim() || null.
    let description: Option<String> = body
        .get("description")
        .and_then(Value::as_str)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    // dialogueDetection: body.dialogueDetection || null.
    let dialogue_detection: Option<DialogueDetection> = match body.get("dialogueDetection") {
        Some(v) if js_truthy(v) => match serde_json::from_value(v.clone()) {
            Ok(dd) => Some(dd),
            Err(e) => return bad_request(format!("Invalid dialogueDetection: {e}")),
        },
        _ => None,
    };

    let id = uuid::Uuid::new_v4().to_string();
    let now = crate::clock::now_iso();
    let create = RtCreate {
        user_id: Some(user_id.to_string()),
        name: name_trimmed.clone(),
        description: description.clone(),
        system_prompt: system_prompt.trim().to_string(),
        is_built_in: false,
        tags: Vec::new(),
        delimiters: delimiters.clone(),
        rendering_patterns: rendering_patterns.clone(),
        dialogue_detection: dialogue_detection.clone(),
        narration_delimiters: narration_typed,
        unmarked_style: unmarked_style.clone(),
        markdown_asterisks,
    };
    let opts = CreateOptions {
        id: id.clone(),
        created_at: now.clone(),
        updated_at: now.clone(),
    };
    let write = db
        .write(move |ws| {
            RoleplayTemplatesRepository::new(ws.main().connection()).create(&create, &opts)
        })
        .await;
    if let Err(e) = write {
        return internal(e);
    }
    tracing::info!(
        userId = %user_id,
        templateId = %id,
        templateName = %name_trimmed,
        "Roleplay template created"
    );

    // The 201 body = v4's `validate(entityInput)` (the in-memory created object):
    // every field present, `description`/`dialogueDetection` null-or-value present,
    // `narrationDelimiters` the input verbatim (string, array, or `null`),
    // `unmarkedStyle` the parsed object or `null` (the object literal carries the
    // key), `markdownAsterisks` the boolean.
    let mut obj = serde_json::Map::new();
    obj.insert("id".into(), Value::String(id));
    obj.insert("userId".into(), Value::String(user_id.to_string()));
    obj.insert("name".into(), Value::String(name_trimmed));
    obj.insert(
        "description".into(),
        description.map(Value::String).unwrap_or(Value::Null),
    );
    obj.insert(
        "systemPrompt".into(),
        Value::String(system_prompt.trim().to_string()),
    );
    obj.insert("isBuiltIn".into(), Value::Bool(false));
    obj.insert("tags".into(), Value::Array(Vec::new()));
    obj.insert(
        "delimiters".into(),
        serde_json::to_value(&delimiters).unwrap_or(Value::Array(Vec::new())),
    );
    obj.insert(
        "renderingPatterns".into(),
        serde_json::to_value(&rendering_patterns).unwrap_or(Value::Array(Vec::new())),
    );
    obj.insert(
        "dialogueDetection".into(),
        dialogue_detection
            .and_then(|dd| serde_json::to_value(dd).ok())
            .unwrap_or(Value::Null),
    );
    obj.insert("narrationDelimiters".into(), narration);
    obj.insert(
        "unmarkedStyle".into(),
        unmarked_style
            .and_then(|u| serde_json::to_value(u).ok())
            .unwrap_or(Value::Null),
    );
    obj.insert("markdownAsterisks".into(), Value::Bool(markdown_asterisks));
    obj.insert("createdAt".into(), Value::String(now.clone()));
    obj.insert("updatedAt".into(), Value::String(now));
    Response::RoleplayTemplate(Value::Object(obj))
}

/// v4 `PUT /api/v1/roleplay-templates/[id]` — 404 / built-in 403 / dup 409 /
/// the partial-body trim seam / the two regen-on-update arms; the bare updated
/// template (v4's `validate({...existing, ...updateData})`).
pub async fn roleplay_template_update(
    db: &Db,
    user_id: &str,
    template_id: &str,
    body: Value,
) -> Response {
    let existing = match db.read_main(|m| find_full_json_by_id(m, template_id)) {
        Ok(Some(t)) => t,
        Ok(None) => return not_found("Roleplay template"),
        Err(e) => return internal(e),
    };
    if existing
        .get("isBuiltIn")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return forbidden("Cannot edit built-in roleplay templates");
    }
    let Some(body_obj) = body.as_object() else {
        return bad_request("Expected object");
    };

    // v4 `updateRoleplayTemplateSchema.parse(body)` THROWS on a refused key and
    // the middleware answers `validationError` — the three P4.D351 keys'
    // arms, in schema key order. (The older keys' parse arms are not ported —
    // a pre-existing P4.6p-era class.)
    let mut parse_issues: Vec<ZodIssue> = Vec::new();
    parse_issues.extend(zod_narration_delimiters_update_issues(
        body_obj.get("narrationDelimiters"),
        &[key("narrationDelimiters")],
    ));
    parse_issues.extend(zod_unmarked_style_issues(
        body_obj.get("unmarkedStyle"),
        &[key("unmarkedStyle")],
    ));
    match body_obj.get("markdownAsterisks") {
        None | Some(Value::Bool(_)) => {}
        got => parse_issues.push(ZodIssue::invalid_type(
            "boolean",
            vec![key("markdownAsterisks")],
            got,
        )),
    }
    if !parse_issues.is_empty() {
        return validation_error(&parse_issues);
    }

    // Duplicate-name guard — only when a name is provided AND it differs. Runs
    // BEFORE the required-field re-validate (v4 order), so a dup-name body missing
    // systemPrompt still 409s.
    if let Some(new_name) = body_obj.get("name").and_then(Value::as_str) {
        let existing_name = existing.get("name").and_then(Value::as_str).unwrap_or("");
        if new_name != existing_name {
            match db.read_main(|m| find_id_by_user_and_name(m, user_id, new_name)) {
                Ok(Some(_)) => {
                    return conflict("A roleplay template with this name already exists")
                }
                Ok(None) => {}
                Err(e) => return internal(e),
            }
        }
    }

    // Regenerate rendering patterns whenever any authoring field they derive
    // from changed and no explicit patterns were sent. Every field falls back to
    // the stored value, so a partial update never drops the others' patterns
    // (`narrationDelimiters` / `unmarkedStyle` fall back on `!== undefined`, so
    // an explicit `null` WINS; `markdownAsterisks` on `??`). v4 regenerates
    // BEFORE the repository's whole-row re-validate, so the DEBUG fires even on
    // a body that then 400s.
    let derives_patterns = [
        "delimiters",
        "narrationDelimiters",
        "unmarkedStyle",
        "markdownAsterisks",
    ]
    .iter()
    .any(|k| body_obj.contains_key(*k));
    let regenerated: Option<Vec<RenderingPattern>> =
        if derives_patterns && !body_obj.contains_key("renderingPatterns") {
            let delimiters = match body_obj.get("delimiters") {
                Some(v) => delimiters_from_value(v),
                None => existing
                    .get("delimiters")
                    .map(delimiters_from_value)
                    .unwrap_or_default(),
            };
            let narration = match body_obj.get("narrationDelimiters") {
                Some(v) => narration_of(Some(v)),
                None => narration_of(existing.get("narrationDelimiters")),
            };
            let unmarked = match body_obj.get("unmarkedStyle") {
                Some(v) => unmarked_of(Some(v)),
                None => unmarked_of(existing.get("unmarkedStyle")),
            };
            let markdown = body_obj
                .get("markdownAsterisks")
                .and_then(Value::as_bool)
                .or_else(|| existing.get("markdownAsterisks").and_then(Value::as_bool));
            let generated = generate_template_rendering_patterns(
                Some(&delimiters),
                narration.as_ref(),
                unmarked.as_ref(),
                markdown,
            );
            tracing::debug!(
                templateId = %template_id,
                patternCount = generated.len(),
                "Roleplay template patterns regenerated for update"
            );
            Some(generated)
        } else {
            None
        };

    // v4 `updateData` ALWAYS sets name / description / systemPrompt to
    // `validatedData.field?.trim()` — undefined when the field is absent (or, for
    // description, explicitly null). The `_update` merge (`{...existing,
    // ...updateData}`) then OVERWRITES those keys with undefined, and the full
    // `RoleplayTemplateSchema.parse` re-validate runs: `name`/`systemPrompt` are
    // REQUIRED, so a body omitting either yields the middleware's 400
    // `Validation error` with Zod's issues; `description` is nullable+optional,
    // so undefined → the key is dropped.
    let name_new = body_obj.get("name").and_then(Value::as_str).map(str::trim);
    let sysprompt_new = body_obj
        .get("systemPrompt")
        .and_then(Value::as_str)
        .map(str::trim);
    let mut required_issues: Vec<ZodIssue> = Vec::new();
    for (k, v) in [("name", name_new), ("systemPrompt", sysprompt_new)] {
        match v {
            None => required_issues.push(ZodIssue::invalid_type("string", vec![key(k)], None)),
            Some("") => required_issues.push(ZodIssue::too_small_string(json!(1), vec![key(k)])),
            Some(_) => {}
        }
    }
    if !required_issues.is_empty() {
        return validation_error(&required_issues);
    }
    let name_new = name_new.unwrap_or_default().to_string();
    let sysprompt_new = sysprompt_new.unwrap_or_default().to_string();

    // Build the merged echo.
    let mut merged = existing.clone();
    let mo = merged.as_object_mut().unwrap();
    mo.insert("name".into(), Value::String(name_new.clone()));
    mo.insert("systemPrompt".into(), Value::String(sysprompt_new));
    // description is ALWAYS overwritten: a body string trims in; anything else
    // (absent / null) drops the key.
    match body_obj.get("description") {
        Some(Value::String(s)) => {
            mo.insert("description".into(), Value::String(s.trim().to_string()));
        }
        _ => {
            mo.remove("description");
        }
    }
    // tags / dialogueDetection / narrationDelimiters: spread verbatim when present.
    if let Some(v) = body_obj.get("tags") {
        mo.insert("tags".into(), v.clone());
    }
    if let Some(v) = body_obj.get("dialogueDetection") {
        mo.insert("dialogueDetection".into(), v.clone());
    }
    if let Some(v) = body_obj.get("narrationDelimiters") {
        mo.insert("narrationDelimiters".into(), v.clone());
    }
    // unmarkedStyle: a sent value WINS (`null` stays `null`, an object is the
    // Zod-parsed one — add-on defaults materialised); absent keeps the stored
    // shape (ABSENT when the cell is NULL).
    if let Some(v) = body_obj.get("unmarkedStyle") {
        let parsed = unmarked_of(Some(v))
            .and_then(|u| serde_json::to_value(u).ok())
            .unwrap_or(Value::Null);
        mo.insert("unmarkedStyle".into(), parsed);
    }
    if let Some(v) = body_obj.get("markdownAsterisks") {
        mo.insert("markdownAsterisks".into(), v.clone());
    }
    // delimiters: the full re-validate re-parses them → normalize via the typed
    // union (materializes addOns defaults, backfills legacy kinds).
    if let Some(v) = body_obj.get("delimiters") {
        let normalized = serde_json::to_value(delimiters_from_value(v))
            .unwrap_or_else(|_| Value::Array(Vec::new()));
        mo.insert("delimiters".into(), normalized);
    }
    if let Some(v) = body_obj.get("renderingPatterns") {
        mo.insert("renderingPatterns".into(), v.clone());
    }
    if let Some(generated) = &regenerated {
        mo.insert(
            "renderingPatterns".into(),
            serde_json::to_value(generated).unwrap_or(Value::Array(Vec::new())),
        );
    }

    // Preserve id + createdAt; mint updatedAt.
    let now = crate::clock::now_iso();
    mo.insert("updatedAt".into(), Value::String(now.clone()));

    // Persist the changed columns (the row is not re-read; the echo above is the
    // response). `updatedAt` always set; each provided field written.
    let patch = build_update_patch(mo, body_obj, regenerated.is_some(), now);
    let id = template_id.to_string();
    let write = db
        .write(move |ws| {
            RoleplayTemplatesRepository::new(ws.main().connection()).update(&id, &patch)
        })
        .await;
    match write {
        Ok(_) => {
            tracing::info!(
                templateId = %template_id,
                templateName = %name_new,
                userId = %user_id,
                "Roleplay template updated successfully"
            );
            Response::RoleplayTemplate(merged)
        }
        Err(e) => internal(e),
    }
}

/// v4 `DELETE /api/v1/roleplay-templates/[id]` — 404 / built-in 403 / delete →
/// `{success: true, deletedId}`.
pub async fn roleplay_template_delete(db: &Db, template_id: &str) -> Response {
    let existing = match db.read_main(|m| find_full_json_by_id(m, template_id)) {
        Ok(Some(t)) => t,
        Ok(None) => return not_found("Roleplay template"),
        Err(e) => return internal(e),
    };
    if existing
        .get("isBuiltIn")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return forbidden("Cannot delete built-in roleplay templates");
    }
    let id = template_id.to_string();
    let out = db
        .write(move |ws| RoleplayTemplatesRepository::new(ws.main().connection()).delete(&id))
        .await;
    match out {
        Ok(true) => {
            // v4 logs the session user's id; this handler takes none, and the
            // only templates that reach a delete are the single user's own
            // (built-ins 403 above), so the row's `userId` is that id.
            let template_name = existing.get("name").and_then(Value::as_str).unwrap_or("");
            let owner_id = existing.get("userId").and_then(Value::as_str).unwrap_or("");
            tracing::info!(
                templateId = %template_id,
                templateName = %template_name,
                userId = %owner_id,
                "Roleplay template deleted successfully"
            );
            Response::RoleplayTemplate(json!({ "success": true, "deletedId": template_id }))
        }
        Ok(false) => internal("Failed to delete roleplay template"),
        Err(e) => internal(e),
    }
}

// ===========================================================================
// Helpers
// ===========================================================================

/// Strictly parse a `delimiters` JSON value into the typed union (kind-backfilled),
/// erroring on a non-array or an unparseable element (v4's Zod `safeParse`
/// failure → the 400 `Invalid delimiters: …`).
fn parse_delimiters_strict(v: &Value) -> Result<Vec<TemplateDelimiter>, String> {
    let Value::Array(items) = v else {
        return Err("expected array".to_string());
    };
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        let mut item = item.clone();
        if let Value::Object(map) = &mut item {
            map.entry("kind")
                .or_insert_with(|| Value::String("wrap".to_string()));
        }
        // P4.D351: a `wrap`'s `matchCurlyQuotes` / `closeOptional` are
        // `z.boolean().optional()` — a present non-boolean (`null` included,
        // which serde's `Option<bool>` would admit) is Zod's first issue when
        // the element is otherwise sound, so answer its sentence rather than
        // the `malformed` fallback (that fallback stays for every OTHER parse
        // failure — a pre-existing P4.6p-era class).
        if let Some(msg) = wrap_flag_type_miss(&item) {
            return Err(msg);
        }
        match serde_json::from_value::<TemplateDelimiter>(item) {
            Ok(d) => out.push(d),
            Err(_) => return Err("malformed".to_string()),
        }
    }
    Ok(out)
}

/// The Zod sentence for a `wrap` delimiter whose `matchCurlyQuotes` /
/// `closeOptional` (schema order) is present and not a boolean, WHEN the
/// element parses once both flags are set aside (so that flag is the element's
/// first issue). `None` otherwise.
fn wrap_flag_type_miss(item: &Value) -> Option<String> {
    let obj = item.as_object()?;
    if obj.get("kind").and_then(Value::as_str) != Some("wrap") {
        return None;
    }
    let bad = ["matchCurlyQuotes", "closeOptional"]
        .into_iter()
        .find(|k| obj.get(*k).is_some_and(|v| !v.is_boolean()))?;
    let mut rest = obj.clone();
    rest.remove("matchCurlyQuotes");
    rest.remove("closeOptional");
    serde_json::from_value::<TemplateDelimiter>(Value::Object(rest)).ok()?;
    Some(
        ZodIssue::invalid_type("boolean", vec![key(bad)], obj.get(bad))
            .message()
            .to_string(),
    )
}

/// Build the persistence patch from the merged echo + the request body. Mirrors
/// v4's `$set: validated` shape closely enough for a correct write (the echo, not
/// a re-read, is the diffed response).
fn build_update_patch(
    merged: &serde_json::Map<String, Value>,
    body_obj: &serde_json::Map<String, Value>,
    regenerated: bool,
    updated_at: String,
) -> RtUpdate {
    let mut patch = RtUpdate {
        updated_at,
        ..Default::default()
    };
    if let Some(v) = merged.get("name").and_then(Value::as_str) {
        if body_obj.contains_key("name") {
            patch.name = Some(v.to_string());
        }
    }
    // description: only persist when a string was provided (explicit null leaves
    // the column unchanged, matching v4's undefined-in-$set).
    if let Some(Value::String(s)) = body_obj.get("description") {
        patch.description = Some(s.trim().to_string());
    }
    if let Some(v) = merged.get("systemPrompt").and_then(Value::as_str) {
        if body_obj.contains_key("systemPrompt") {
            patch.system_prompt = Some(v.to_string());
        }
    }
    if let Some(Value::Array(_)) = body_obj.get("tags") {
        patch.tags = serde_json::from_value(body_obj["tags"].clone()).ok();
    }
    if body_obj.contains_key("delimiters") {
        patch.delimiters = Some(delimiters_from_value(
            merged.get("delimiters").unwrap_or(&Value::Null),
        ));
    }
    // renderingPatterns: persisted whenever sent or regenerated.
    if let Some(rp) = merged.get("renderingPatterns") {
        if regenerated || body_obj.contains_key("renderingPatterns") {
            patch.rendering_patterns = serde_json::from_value(rp.clone()).ok();
        }
    }
    if let Some(dd) = body_obj.get("dialogueDetection") {
        if js_truthy(dd) {
            patch.dialogue_detection = serde_json::from_value(dd.clone()).ok();
        }
    }
    // narrationDelimiters / unmarkedStyle: a sent `null` sets null (`Some(None)`),
    // a value sets it; markdownAsterisks: a sent boolean.
    if let Some(nd) = body_obj.get("narrationDelimiters") {
        patch.narration_delimiters = Some(narration_of(Some(nd)));
    }
    if let Some(u) = body_obj.get("unmarkedStyle") {
        patch.unmarked_style = Some(unmarked_of(Some(u)));
    }
    if let Some(b) = body_obj.get("markdownAsterisks").and_then(Value::as_bool) {
        patch.markdown_asterisks = Some(b);
    }
    patch
}
