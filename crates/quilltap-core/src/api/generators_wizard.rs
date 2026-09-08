//! P4.9K2 — the creation-pair generator verbs (§B.3 of the `p4.9k` contract):
//! `characterWizard` / `characterWizardStream` (v4 `POST /api/v1/characters
//! ?action=ai-wizard` / `ai-wizard-stream`, `app/api/v1/characters/handlers/
//! post.ts:518-575`) and `aiImportStream` (v4 `POST /api/v1/system/tools
//! ?action=ai-import-stream`, `app/api/v1/system/tools/route.ts:1190-1260`).
//! v5's SPA drives them over `/api/dispatch`; the REST edges delegate into
//! these same handlers.
//!
//! ## The guard order, MEASURED on v4's routes
//!
//! * `ai-wizard` / `ai-wizard-stream` — `wizardRequestSchema.parse(body)` (an
//!   uncaught `ZodError` → the middleware's 400 `{error: 'Validation error',
//!   details}`), the `[Characters v1] AI Wizard starting` line, then the
//!   runner. The non-streaming twin's runner THROWS (`Primary profile not
//!   found`, `Image not found`, …) into the middleware's generic catch: `500
//!   Internal server error`. The streaming twin never throws — every failure
//!   is a `done {error}` FRAME.
//! * `ai-import-stream` — the hand-rolled body read (`profileId`,
//!   `sourceFileIds || []`, `sourceText || ''`, `includeMemories ?? true`,
//!   `includeChats ?? false`, `existingResult || undefined`, `regenerateSteps
//!   || undefined`), the two 400s (`Missing required field: profileId`, `Must
//!   provide at least one source file or source text`), the `[System Tools
//!   v1] AI Import stream starting` line, then the runner (never throws — a
//!   `done {error}` frame); the route's own catch answers `serverError(message)`
//!   for a throw OUTSIDE the runner (a non-JSON body).
//!
//! The DRIVER refusal (no host bundle) comes after every parse arm, so a
//! read-only embedder still answers v4's shapes for everything short of the
//! model call.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use serde_json::{json, Value};
use tokio::sync::broadcast;

use crate::api::types::{Event, GeneratorKind};
use crate::generators::ai_import::AiImportRequest;
use crate::generators::wizard::{OnProgress, WizardRequest, WizardResult};
use crate::services::generator_progress::GeneratorProgressEmitter;

use super::settings::{zod_parsed_type, zod_uuid_ok, ZOD_UUID_PATTERN};
use super::types::{ErrorKind, Response};

// ===========================================================================
// The driver seam
// ===========================================================================

/// The boxed future a [`GeneratorsWizardDriver`] method returns.
pub type GeneratorsWizardFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// The projected wizard request a driver runs: the resolved single-user id and
/// the Zod-validated body.
#[derive(Clone, Debug)]
pub struct WizardDriverRequest {
    pub user_id: String,
    pub request: WizardRequest,
}

/// The projected AI-import request a driver runs.
#[derive(Clone, Debug)]
pub struct AiImportDriverRequest {
    pub user_id: String,
    pub request: AiImportRequest,
    pub now_ms: i64,
}

/// The creation-pair generator driver: only the composing host holds the
/// completion provider and the storage backend the three model-calling verbs
/// run over. The engine keeps every parse arm in front of it.
pub trait GeneratorsWizardDriver: Send + Sync {
    /// v4 `runCharacterWizard` — `Err` is v4's throw (the route's generic 500).
    fn wizard<'a>(
        &'a self,
        req: WizardDriverRequest,
    ) -> GeneratorsWizardFuture<'a, Result<WizardResult, String>>;
    /// v4 `runCharacterWizardStreaming` — never fails; every outcome is a frame.
    fn wizard_stream<'a>(
        &'a self,
        req: WizardDriverRequest,
        on_progress: OnProgress<'a>,
    ) -> GeneratorsWizardFuture<'a, ()>;
    /// v4 `runAIImportStreaming` — never fails; every outcome is a frame.
    fn ai_import_stream<'a>(
        &'a self,
        req: AiImportDriverRequest,
        on_progress: OnProgress<'a>,
    ) -> GeneratorsWizardFuture<'a, ()>;
}

// ===========================================================================
// Zod 4 issue rendering (the `generators_detail` idiom, as values)
// ===========================================================================

fn invalid_type(expected: &str, path: &[Value], got: Option<&Value>) -> Value {
    json!({
        "expected": expected,
        "code": "invalid_type",
        "path": path,
        "message": format!("Invalid input: expected {expected}, received {}", zod_parsed_type(got)),
    })
}

fn invalid_uuid(path: &[Value]) -> Value {
    json!({
        "origin": "string",
        "code": "invalid_format",
        "format": "uuid",
        "pattern": ZOD_UUID_PATTERN,
        "path": path,
        "message": "Invalid UUID",
    })
}

fn invalid_value(values: &[&str], path: &[Value]) -> Value {
    json!({
        "code": "invalid_value",
        "values": values,
        "path": path,
        "message": format!(
            "Invalid option: expected one of {}",
            values.iter().map(|v| format!("\"{v}\"")).collect::<Vec<_>>().join("|")
        ),
    })
}

fn push_path(path: &[Value], key: impl Into<Value>) -> Vec<Value> {
    let mut p = path.to_vec();
    p.push(key.into());
    p
}

/// `z.uuid()` at `path`.
fn check_uuid(v: Option<&Value>, path: &[Value], issues: &mut Vec<Value>) -> Option<String> {
    match v {
        Some(Value::String(s)) if zod_uuid_ok(s) => Some(s.clone()),
        Some(Value::String(_)) => {
            issues.push(invalid_uuid(path));
            None
        }
        other => {
            issues.push(invalid_type("string", path, other));
            None
        }
    }
}

/// `z.uuid().optional()` — absent is fine, `null` is not.
fn check_optional_uuid(
    v: Option<&Value>,
    path: &[Value],
    issues: &mut Vec<Value>,
) -> Option<Option<String>> {
    match v {
        None => Some(None),
        Some(x) => check_uuid(Some(x), path, issues).map(Some),
    }
}

/// `z.string()` at `path`.
fn check_string(v: Option<&Value>, path: &[Value], issues: &mut Vec<Value>) -> Option<String> {
    match v {
        Some(Value::String(s)) => Some(s.clone()),
        other => {
            issues.push(invalid_type("string", path, other));
            None
        }
    }
}

/// `z.string().optional()` inside a bag — absent is fine, `null` is not.
fn check_optional_string(v: Option<&Value>, path: &[Value], issues: &mut Vec<Value>) -> bool {
    match v {
        None => true,
        Some(x) => check_string(Some(x), path, issues).is_some(),
    }
}

/// v4 `SOURCE_TYPES` / `WIZARD_FIELDS` — the two enums, in schema order.
pub const SOURCE_TYPES: &[&str] = &["existing", "upload", "gallery", "document", "skip"];
pub const WIZARD_FIELDS: &[&str] = &[
    "name",
    "title",
    "identity",
    "description",
    "manifesto",
    "personality",
    "scenarios",
    "exampleDialogues",
    "firstMessage",
    "systemPrompt",
    "properties",
    "physicalDescription",
    "wardrobeItems",
];

fn check_enum(
    v: Option<&Value>,
    values: &[&str],
    path: &[Value],
    issues: &mut Vec<Value>,
) -> Option<String> {
    match v {
        Some(Value::String(s)) if values.contains(&s.as_str()) => Some(s.clone()),
        _ => {
            issues.push(invalid_value(values, path));
            None
        }
    }
}

/// `existingData`'s nested schema (`z.object({...}).optional()`): every
/// member optional, `scenarios[]` of `{id, title, content}` strings,
/// `pronouns` a nullable triple, `aliases` an array of strings. Issues are
/// appended in schema key order; the whole bag is carried as a `Value` on
/// success (the runner's [`crate::generators::wizard::build_context_prompt`]
/// reads it with JS semantics).
fn check_existing_data(v: Option<&Value>, issues: &mut Vec<Value>) -> Option<Option<Value>> {
    let base = vec![Value::String("existingData".into())];
    let Some(v) = v else {
        return Some(None);
    };
    let Some(obj) = v.as_object() else {
        issues.push(invalid_type("object", &base, Some(v)));
        return None;
    };
    let before = issues.len();
    for key in [
        "title",
        "identity",
        "description",
        "manifesto",
        "personality",
    ] {
        check_optional_string(obj.get(key), &push_path(&base, key), issues);
    }
    if let Some(scenarios) = obj.get("scenarios") {
        let spath = push_path(&base, "scenarios");
        match scenarios.as_array() {
            Some(items) => {
                for (i, item) in items.iter().enumerate() {
                    let ipath = push_path(&spath, i);
                    match item.as_object() {
                        Some(o) => {
                            for key in ["id", "title", "content"] {
                                check_string(o.get(key), &push_path(&ipath, key), issues);
                            }
                        }
                        None => issues.push(invalid_type("object", &ipath, Some(item))),
                    }
                }
            }
            None => issues.push(invalid_type("array", &spath, Some(scenarios))),
        }
    }
    for key in ["exampleDialogues", "systemPrompt", "firstMessage"] {
        check_optional_string(obj.get(key), &push_path(&base, key), issues);
    }
    if let Some(pronouns) = obj.get("pronouns") {
        let ppath = push_path(&base, "pronouns");
        match pronouns {
            Value::Null => {}
            Value::Object(o) => {
                for key in ["subject", "object", "possessive"] {
                    check_string(o.get(key), &push_path(&ppath, key), issues);
                }
            }
            other => issues.push(invalid_type("object", &ppath, Some(other))),
        }
    }
    if let Some(aliases) = obj.get("aliases") {
        let apath = push_path(&base, "aliases");
        match aliases.as_array() {
            Some(items) => {
                for (i, item) in items.iter().enumerate() {
                    check_string(Some(item), &push_path(&apath, i), issues);
                }
            }
            None => issues.push(invalid_type("array", &apath, Some(aliases))),
        }
    }
    if issues.len() > before {
        None
    } else {
        Some(Some(v.clone()))
    }
}

/// v4 `wizardRequestSchema.parse(body)` over the raw body. `Err` is the
/// middleware's `details` array (every issue, in schema key order). A body
/// that is not an object answers the root-level `invalid_type`.
pub fn parse_wizard_body(body: &Value) -> Result<WizardRequest, WizardBodyError> {
    let Some(obj) = body.as_object() else {
        return Err(WizardBodyError::Invalid(vec![invalid_type(
            "object",
            &[],
            Some(body),
        )]));
    };
    let mut issues: Vec<Value> = Vec::new();
    let path = |key: &str| vec![Value::String(key.to_string())];

    let primary_profile_id = check_uuid(
        obj.get("primaryProfileId"),
        &path("primaryProfileId"),
        &mut issues,
    );
    let vision_profile_id = check_optional_uuid(
        obj.get("visionProfileId"),
        &path("visionProfileId"),
        &mut issues,
    );
    let source_type = check_enum(
        obj.get("sourceType"),
        SOURCE_TYPES,
        &path("sourceType"),
        &mut issues,
    );
    let image_id = check_optional_uuid(obj.get("imageId"), &path("imageId"), &mut issues);
    let document_id = check_optional_uuid(obj.get("documentId"), &path("documentId"), &mut issues);
    let character_name = check_string(
        obj.get("characterName"),
        &path("characterName"),
        &mut issues,
    );
    let existing_data = check_existing_data(obj.get("existingData"), &mut issues);
    let background = check_string(obj.get("background"), &path("background"), &mut issues);
    let fields_to_generate: Option<Vec<String>> = match obj.get("fieldsToGenerate") {
        Some(Value::Array(items)) => {
            let mut out = Vec::new();
            let mut ok = true;
            for (i, item) in items.iter().enumerate() {
                match check_enum(
                    Some(item),
                    WIZARD_FIELDS,
                    &push_path(&path("fieldsToGenerate"), i),
                    &mut issues,
                ) {
                    Some(f) => out.push(f),
                    None => ok = false,
                }
            }
            ok.then_some(out)
        }
        other => {
            issues.push(invalid_type("array", &path("fieldsToGenerate"), other));
            None
        }
    };
    let character_id =
        check_optional_uuid(obj.get("characterId"), &path("characterId"), &mut issues);

    if !issues.is_empty() {
        return Err(WizardBodyError::Invalid(issues));
    }
    // Every checker that answers `None` pushes an issue, so the guard above
    // means all ten are `Some` here. A `None` surviving it would be a v5 BUG,
    // not a client error — and v4 has no counterpart at all (Zod's `.parse`
    // either throws a `ZodError` or hands back the value). It used to be ten
    // `.expect("no issues")` panics; it is now the same generic 500 v4's
    // middleware answers for an unhandled throw (P4.86 tier-2 item 8).
    let request = (|| {
        Some(WizardRequest {
            primary_profile_id: primary_profile_id?,
            vision_profile_id: vision_profile_id?,
            source_type: source_type?,
            image_id: image_id?,
            document_id: document_id?,
            character_name: character_name?,
            existing_data: existing_data?,
            background: background?,
            fields_to_generate: fields_to_generate?,
            character_id: character_id?,
        })
    })();
    request.ok_or(WizardBodyError::Internal(
        "a wizard body field was missing with no validation issue recorded",
    ))
}

/// v4 `handlers/post.ts:524` / `:542` — the same four-key bag under two
/// sentences. Rendered as v4's `logger.info(msg, bag)` JSON so
/// `fieldsToGenerate` is the ARRAY v4 logs, not a Rust `Debug` rendering
/// (P4.86 tier-2 item 9; pinned by `wizard_starting_line_matches_v4`).
fn log_wizard_starting(user_id: &str, request: &WizardRequest, streaming: bool) {
    let context = json!({
        "userId": user_id,
        "characterName": request.character_name,
        "fieldsToGenerate": request.fields_to_generate,
        "sourceType": request.source_type,
    });
    if streaming {
        tracing::info!(
            target: GENERATORS_WIZARD_LOG_TARGET,
            context = %context,
            "[Characters v1] AI Wizard starting (streaming)"
        );
    } else {
        tracing::info!(
            target: GENERATORS_WIZARD_LOG_TARGET,
            context = %context,
            "[Characters v1] AI Wizard starting"
        );
    }
}

/// The tracing target the two route-level `[Characters v1]` /
/// `[System Tools v1]` generator lines are emitted under (P4.86).
pub const GENERATORS_WIZARD_LOG_TARGET: &str = "quilltap::generators_wizard";

/// v4's middleware sentence for an unhandled throw (`Internal server error`).
const INTERNAL_SERVER_ERROR: &str = "Internal server error";

/// Why `parse_wizard_body` refused (P4.86 tier-2 item 8).
pub enum WizardBodyError {
    /// v4's `ZodError` issues — a 400 carrying the `details` array.
    Invalid(Vec<Value>),
    /// A v5 invariant violation with no v4 counterpart: the generic 500, and
    /// an `error` line naming it. Never reachable from a client body.
    Internal(&'static str),
}

impl WizardBodyError {
    fn into_response(self) -> Response {
        match self {
            WizardBodyError::Invalid(issues) => Response::validation_error(Value::Array(issues)),
            WizardBodyError::Internal(why) => {
                tracing::error!(
                    target: GENERATORS_WIZARD_LOG_TARGET,
                    reason = %why,
                    "[Characters v1] AI Wizard body could not be built"
                );
                Response::error(ErrorKind::Internal, INTERNAL_SERVER_ERROR)
            }
        }
    }
}

const WIZARD_UNAVAILABLE: &str =
    "AI wizard generation not available: no GeneratorsWizardDriver is assembled";

// ===========================================================================
// characterWizard (v4 `handleAiWizard`)
// ===========================================================================

/// v4's `ai-wizard` action: Zod 400 → the info line → the driver refusal →
/// the runner; `WizardResult` raw on success, v4's generic 500 on a throw.
pub async fn character_wizard(
    driver: Option<&Arc<dyn GeneratorsWizardDriver>>,
    user_id: &str,
    body: &Value,
) -> Response {
    let request = match parse_wizard_body(body) {
        Ok(r) => r,
        Err(e) => return e.into_response(),
    };
    log_wizard_starting(user_id, &request, false);
    let Some(driver) = driver else {
        return Response::error(ErrorKind::Unavailable, WIZARD_UNAVAILABLE);
    };
    match driver
        .wizard(WizardDriverRequest {
            user_id: user_id.to_string(),
            request,
        })
        .await
    {
        // `serde_json::to_value` on a `WizardResult` can only fail on a shape
        // this type cannot hold (a non-string map key, a non-finite float).
        // It used to fall back to `null` — a 200 carrying nothing; it now
        // takes v4's generic 500 (P4.86 tier-2 item 8).
        Ok(result) => match serde_json::to_value(result) {
            Ok(v) => Response::Character(v),
            Err(e) => {
                tracing::error!(
                    target: GENERATORS_WIZARD_LOG_TARGET,
                    error = %e,
                    "[Characters v1] AI Wizard result could not be serialized"
                );
                Response::error(ErrorKind::Internal, INTERNAL_SERVER_ERROR)
            }
        },
        Err(error) => {
            // v4's middleware catch: the error is logged, the client gets the
            // generic sentence.
            tracing::error!(error = %error, "[Characters v1] AI Wizard failed");
            Response::error(ErrorKind::Internal, INTERNAL_SERVER_ERROR)
        }
    }
}

// ===========================================================================
// characterWizardStream (v4 `handleAiWizardStream`)
// ===========================================================================

/// v4's `ai-wizard-stream` action: Zod 400 → the info line → the driver
/// refusal → the runner, every `onProgress` published under `progress_id`
/// and the LAST event answered as `{ terminal }` (§B.1).
pub async fn character_wizard_stream(
    driver: Option<&Arc<dyn GeneratorsWizardDriver>>,
    events: &broadcast::Sender<Event>,
    user_id: &str,
    progress_id: Option<&str>,
    body: &Value,
) -> Response {
    let request = match parse_wizard_body(body) {
        Ok(r) => r,
        Err(e) => return e.into_response(),
    };
    log_wizard_starting(user_id, &request, true);
    let Some(driver) = driver else {
        return Response::error(ErrorKind::Unavailable, WIZARD_UNAVAILABLE);
    };
    let emitter =
        GeneratorProgressEmitter::from_id(progress_id, GeneratorKind::Wizard, events.clone());
    let mut terminal: Option<Value> = None;
    {
        let mut sink = |event: Value| {
            emitter.emit(event.clone());
            terminal = Some(event);
        };
        driver
            .wizard_stream(
                WizardDriverRequest {
                    user_id: user_id.to_string(),
                    request,
                },
                &mut sink,
            )
            .await;
    }
    Response::Character(json!({ "terminal": terminal }))
}

// ===========================================================================
// aiImportStream (v4 `handleAIImportStream`)
// ===========================================================================

/// v4's hand-rolled body read (`route.ts:1196-1204`) + its two 400s. `body`
/// is the parsed JSON object (v4 `await req.json()`); the defaults are v4's
/// `||` / `??` spellings exactly.
pub fn parse_ai_import_body(body: &Value) -> Result<AiImportRequest, Response> {
    let get = |k: &str| body.get(k);
    // `profileId: body.profileId` then `if (!request.profileId)` — truthy.
    let profile_id = get("profileId");
    if !crate::api::system_qtap::js_truthy(profile_id) {
        return Err(Response::error(
            ErrorKind::BadRequest,
            "Missing required field: profileId",
        ));
    }
    // `sourceFileIds: body.sourceFileIds || []` — the RAW value, never
    // narrowed (P4.86). A truthy non-array keeps its own `.length`: a STRING
    // passes the emptiness gate below on its UTF-16 length and is then
    // iterated by CODE POINT as file ids; a number or an object has no
    // `.length` at all, so `undefined === 0` is FALSE, the gate passes, and
    // the runner's `for…of` throws `sourceFileIds is not iterable`.
    let source_file_ids: Value = match get("sourceFileIds") {
        Some(v) if crate::api::system_qtap::js_truthy(Some(v)) => v.clone(),
        _ => Value::Array(Vec::new()),
    };
    // `sourceText: body.sourceText || ''`
    let source_text = match get("sourceText") {
        Some(v) if crate::api::system_qtap::js_truthy(Some(v)) => {
            crate::pascal::js_value::to_js_string(v)
        }
        _ => String::new(),
    };
    // `includeMemories: body.includeMemories ?? true` / `includeChats ?? false`
    // — NULLISH: `false`/`0`/`''` are kept and read for truthiness downstream.
    let include_memories = match get("includeMemories") {
        None | Some(Value::Null) => Value::Bool(true),
        Some(v) => v.clone(),
    };
    let include_chats = match get("includeChats") {
        None | Some(Value::Null) => Value::Bool(false),
        Some(v) => v.clone(),
    };
    // `existingResult: body.existingResult || undefined`
    let existing_result = get("existingResult")
        .filter(|v| crate::api::system_qtap::js_truthy(Some(v)))
        .cloned();
    // `regenerateSteps: body.regenerateSteps || undefined` — the RAW value.
    // v4 never narrows it to an array: `shouldRunStep` calls `.includes(step)`
    // on whatever it is, so a STRING is a substring test and anything else
    // throws (P4.86; `generators::ai_import::should_run_step`).
    let regenerate_steps: Option<Value> = get("regenerateSteps")
        .filter(|v| crate::api::system_qtap::js_truthy(Some(v)))
        .cloned();
    // `request.sourceFileIds.length === 0` — a strict comparison against a
    // possibly-`undefined` length, so ONLY a real zero closes this gate.
    let empty_file_ids = crate::generators::ai_import::js_length_of(&source_file_ids) == Some(0);
    if empty_file_ids && crate::jsstr::js_trim(&source_text).is_empty() {
        return Err(Response::error(
            ErrorKind::BadRequest,
            "Must provide at least one source file or source text",
        ));
    }
    Ok(AiImportRequest {
        profile_id: crate::pascal::js_value::to_js_string(profile_id.unwrap()),
        source_file_ids,
        source_text,
        include_memories,
        include_chats,
        existing_result,
        regenerate_steps,
    })
}

/// v4's `ai-import-stream` action: the two 400s → the info line → the driver
/// refusal → the runner under `progress_id`, `{ terminal }` on resolve.
pub async fn ai_import_stream(
    driver: Option<&Arc<dyn GeneratorsWizardDriver>>,
    events: &broadcast::Sender<Event>,
    user_id: &str,
    progress_id: Option<&str>,
    body: &Value,
    now_ms: i64,
) -> Response {
    let request = match parse_ai_import_body(body) {
        Ok(r) => r,
        Err(r) => return r,
    };
    // v4's `logger.info(msg, bag)` (`route.ts:1214`), in v4's key order. The
    // bag shape (rather than named tracing fields) is load-bearing: v4's
    // `sourceFileCount` is `request.sourceFileIds.length`, which is
    // `undefined` for a value with no `.length` — and the logger's JSON DROPS
    // an undefined member (P4.86).
    let mut starting = serde_json::Map::new();
    starting.insert("userId".into(), json!(user_id));
    starting.insert("profileId".into(), json!(request.profile_id));
    if let Some(n) = crate::generators::ai_import::js_length_of(&request.source_file_ids) {
        starting.insert("sourceFileCount".into(), json!(n));
    }
    starting.insert(
        "hasSourceText".into(),
        json!(!crate::jsstr::js_trim(&request.source_text).is_empty()),
    );
    starting.insert("includeMemories".into(), request.include_memories.clone());
    starting.insert("includeChats".into(), request.include_chats.clone());
    // Bound outside the macro: `tracing`'s expansion shadows `Value`.
    let starting = Value::Object(starting);
    tracing::info!(
        target: GENERATORS_WIZARD_LOG_TARGET,
        context = %starting,
        "[System Tools v1] AI Import stream starting"
    );
    let Some(driver) = driver else {
        return Response::error(
            ErrorKind::Unavailable,
            "AI import not available: no GeneratorsWizardDriver is assembled",
        );
    };
    let emitter =
        GeneratorProgressEmitter::from_id(progress_id, GeneratorKind::AiImport, events.clone());
    let mut terminal: Option<Value> = None;
    {
        let mut sink = |event: Value| {
            emitter.emit(event.clone());
            terminal = Some(event);
        };
        driver
            .ai_import_stream(
                AiImportDriverRequest {
                    user_id: user_id.to_string(),
                    request,
                    now_ms,
                },
                &mut sink,
            )
            .await;
    }
    Response::Character(json!({ "terminal": terminal }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::captured_with;

    fn ai_import_body(extra: Value) -> Value {
        let mut body = json!({ "profileId": "c0000002-0000-4000-8000-000000000001" });
        for (k, v) in extra.as_object().unwrap() {
            body[k] = v.clone();
        }
        body
    }

    /// P4.86 tier-2 item 9 — v4 `handlers/post.ts:524` / `:542`. Both
    /// sentences, v4's four keys in v4's order, `fieldsToGenerate` as the
    /// ARRAY v4 logs.
    #[test]
    fn wizard_starting_line_matches_v4() {
        let request = WizardRequest {
            primary_profile_id: "c0000002-0000-4000-8000-000000000001".into(),
            vision_profile_id: None,
            source_type: "text".into(),
            image_id: None,
            document_id: None,
            character_name: "Mira Lanternwright".into(),
            existing_data: None,
            background: "A harbor at dusk.".into(),
            fields_to_generate: vec!["identity".into(), "personality".into()],
            character_id: None,
        };
        for (streaming, sentence) in [
            (false, "[Characters v1] AI Wizard starting"),
            (true, "[Characters v1] AI Wizard starting (streaming)"),
        ] {
            let ((), lines) = captured_with(|| log_wizard_starting("u-1", &request, streaming));
            let line = lines
                .iter()
                .find(|l| l.contains(sentence))
                .unwrap_or_else(|| panic!("no {sentence:?} line in {lines:?}"));
            assert!(line.starts_with("INFO"), "{line}");
            assert!(
                line.contains(&format!("target={GENERATORS_WIZARD_LOG_TARGET}"))
                    || line.contains(GENERATORS_WIZARD_LOG_TARGET),
                "{line}"
            );
            assert!(
                line.contains(
                    r#"context={"userId":"u-1","characterName":"Mira Lanternwright","fieldsToGenerate":["identity","personality"],"sourceType":"text"}"#
                ),
                "{line}"
            );
            // The two sentences are distinct — the non-streaming one must not
            // carry the `(streaming)` suffix.
            assert_eq!(
                line.contains("(streaming)"),
                streaming,
                "the two sentences must not collapse: {line}"
            );
        }
    }

    /// P4.86 tier 2: `sourceFileCount: request.sourceFileIds.length` is
    /// `undefined` for a value with no `.length`, and v4's logger DROPS an
    /// undefined member.
    #[test]
    fn ai_import_starting_line_drops_an_undefined_source_file_count() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let events = tokio::sync::broadcast::channel(16).0;
        let cases: [(Value, Option<&str>); 4] = [
            (json!({"sourceFileIds": ["f-1", "f-2"]}), Some("2")),
            (json!({"sourceFileIds": "abc"}), Some("3")),
            (json!({"sourceFileIds": 5, "sourceText": "Mira."}), None),
            (
                json!({"sourceFileIds": {"a": 1}, "sourceText": "Mira."}),
                None,
            ),
        ];
        for (extra, want) in cases {
            let body = ai_import_body(extra);
            let (_r, lines) = captured_with(|| {
                rt.block_on(ai_import_stream(None, &events, "u-1", None, &body, 0))
            });
            let line = lines
                .iter()
                .find(|l| l.contains("[System Tools v1] AI Import stream starting"))
                .unwrap_or_else(|| panic!("no starting line in {lines:?}"));
            match want {
                Some(n) => assert!(
                    line.contains(&format!(r#""sourceFileCount":{n}"#)),
                    "{line}"
                ),
                None => assert!(
                    !line.contains("sourceFileCount"),
                    "an undefined `.length` must DROP the key: {line}"
                ),
            }
            // v4's key order, `sourceFileCount` between `profileId` and
            // `hasSourceText` wherever it renders at all.
            assert!(
                line.contains(r#"context={"userId":"u-1","profileId":"#),
                "{line}"
            );
        }
    }

    /// P4.86 tier-2 item 6: the route's `sourceFileIds.length === 0` gate is a
    /// STRICT comparison against a possibly-`undefined` length.
    #[test]
    fn the_empty_source_gate_only_closes_on_a_real_zero() {
        // No files, no text → the 400.
        let refused = parse_ai_import_body(&ai_import_body(json!({"sourceFileIds": []})));
        assert!(refused.is_err());
        // A string's `.length` is 3 → the gate passes with no source text.
        let ok = parse_ai_import_body(&ai_import_body(json!({"sourceFileIds": "abc"})));
        assert_eq!(ok.unwrap().source_file_ids, json!("abc"));
        // A number has NO `.length` → `undefined === 0` is false → passes.
        let ok = parse_ai_import_body(&ai_import_body(json!({"sourceFileIds": 5})));
        assert_eq!(ok.unwrap().source_file_ids, json!(5));
        // Falsy → `[]` → the gate closes.
        let refused = parse_ai_import_body(&ai_import_body(json!({"sourceFileIds": 0})));
        assert!(refused.is_err());
    }

    /// P4.86 tier-2 item 6: `regenerateSteps` is carried RAW.
    #[test]
    fn regenerate_steps_is_carried_raw() {
        let r = parse_ai_import_body(&ai_import_body(
            json!({"sourceText": "Mira.", "regenerateSteps": "identitydescription"}),
        ))
        .unwrap();
        assert_eq!(r.regenerate_steps, Some(json!("identitydescription")));
        let r = parse_ai_import_body(&ai_import_body(
            json!({"sourceText": "Mira.", "regenerateSteps": []}),
        ))
        .unwrap();
        // `[] || undefined` — an empty array is TRUTHY, so it is kept.
        assert_eq!(r.regenerate_steps, Some(json!([])));
        let r = parse_ai_import_body(&ai_import_body(
            json!({"sourceText": "Mira.", "regenerateSteps": null}),
        ))
        .unwrap();
        assert_eq!(r.regenerate_steps, None);
    }

    /// P4.86 tier-2 item 8: the invariant violation answers v4's generic 500,
    /// never a panic, and names itself in an `error` line.
    #[test]
    fn a_body_that_cannot_be_built_answers_the_generic_500() {
        let ((), lines) = captured_with(|| {
            let r = WizardBodyError::Internal("a wizard body field was missing").into_response();
            match r {
                Response::Error(e) => {
                    assert!(matches!(e.kind, ErrorKind::Internal));
                    assert_eq!(e.message, INTERNAL_SERVER_ERROR);
                }
                other => panic!("unexpected response {other:?}"),
            }
        });
        assert!(
            lines.iter().any(|l| l.starts_with("ERROR")
                && l.contains("[Characters v1] AI Wizard body could not be built")),
            "{lines:?}"
        );
    }
}
