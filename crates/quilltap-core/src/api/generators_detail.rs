//! P4.9K1 — the per-character generator verbs (§B.2 of the `p4.9k` contract):
//! `characterRename`, `characterRefreshArchive`, and (later units)
//! `characterGenerateExternalPrompt` / `characterOptimize`. v4 mounts all four
//! on `POST /api/v1/characters/[id]?action=` (`app/api/v1/characters/[id]/
//! handlers/post.ts`); v5's SPA drives them over `/api/dispatch`, and the REST
//! edge (`quilltap-web::characters_routes::characters_action_post`) delegates
//! into these same handlers.
//!
//! ## The guard order, MEASURED on v4's route (the lane record's item 0)
//!
//! `handlePost` runs `repos.characters.findById(id)` and answers 404
//! `Character not found` BEFORE it validates the action or reads a body, so
//! every arm here resolves the character first. Then, per action:
//!
//! * `rename` — `renameSchema.parse(body)` (an uncaught `ZodError` → the
//!   middleware's 400 `{error: 'Validation error', details}`), then the
//!   route-level 400 `At least one replacement must be specified`, then the
//!   service inside a try whose catch answers 500 `Failed to process rename
//!   request`. NOTE the service receives the already-loaded character.
//! * `refresh-archive` — no body; the arm's own catch answers 500 `Failed to
//!   refresh conversation archive`.
//! * `generate-external-prompt` — `generateExternalPromptSchema.parse(body)`
//!   (the same uncaught-Zod 400), the `[Characters v1] External prompt
//!   generation starting` line, then the service through the host driver; a
//!   `{success: false}` result is a 500 whose message is `result.error ||
//!   'Generation failed'`, and a `{success: true}` one answers
//!   `{ prompt, tokensUsed }`. The DRIVER refusal (no host bundle) comes
//!   after the 404 and the Zod arms, so a read-only embedder still answers
//!   v4's shapes for everything short of the model call.
//!
//! * `optimize-stream` — `optimizeStreamSchema.parse(body)` (the same
//!   uncaught-Zod 400), the `[Characters v1] Character optimizer starting
//!   (streaming)` line, then the runner through the host driver: v4 answers a
//!   `text/event-stream` of every `onProgress` event and NEVER a status past
//!   that point (a failed run is an `error` FRAME, not a 500). v5 publishes the
//!   same frames on the Event channel under the client's `progressId` and
//!   resolves the dispatch with `{ terminal: <the last frame> }` (§B.1); the
//!   REST edge re-frames them as v4's SSE bytes.
//!
//! Neither arm carries an archived-character refusal: v4 hides the tabs
//! client-side (`CharacterDetailView.tsx:415`) and the server runs the request
//! — an executed rename on a tombstone then trips the repository's archive
//! write guard, which lands in the catch as the 500 above. Pinned by the
//! differential's `execute_archived` arm.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use serde_json::{json, Value};
use tokio::sync::broadcast;

use crate::api::types::{Event, GeneratorKind};
use crate::db::runtime::Db;
use crate::db::{characters_read, DbError};
use crate::generators::external_prompt::{ExternalPromptRequest, ExternalPromptResult};
use crate::generators::optimizer::{
    OnProgress, OptimizerOptions, OptimizerOutputMode, MAX_MEMORIES_FOR_ANALYSIS,
};
use crate::generators::refresh_archive::refresh_archive;
use crate::generators::rename::{run_character_rename, RenameRequest, ReplacementPair};
use crate::services::generator_progress::GeneratorProgressEmitter;

use super::settings::{zod_parsed_type, zod_uuid_ok, ZOD_UUID_PATTERN};
use super::types::{db_error_response, ErrorKind, Response};

// ===========================================================================
// The driver seam (the `HelpChatSendDriver` / `ImageDescribeDriver` precedent)
// ===========================================================================

/// The boxed future a [`GeneratorsDetailDriver`] method returns.
pub type GeneratorsDetailFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// The projected `characterGenerateExternalPrompt` a driver runs: the resolved
/// single-user id, the route's character id, and the Zod-validated body.
#[derive(Clone, Debug)]
pub struct ExternalPromptDriverRequest {
    pub user_id: String,
    pub character_id: String,
    pub request: ExternalPromptRequest,
}

/// The per-character generator driver: only the composing host holds the
/// completion (and, for the optimizer, embedding) providers the two
/// model-calling verbs run over. The engine keeps every DB-side arm (404, Zod,
/// the refusal sentences) in front of it, so a driver sees only requests v4
/// would have handed its service.
pub trait GeneratorsDetailDriver: Send + Sync {
    /// v4 `generateExternalPrompt(characterId, request, userId, repos)`. `Err`
    /// is a DB failure escaping the reads (v4's throw); every modelled refusal
    /// is an `Ok` result with `success: false`.
    fn external_prompt<'a>(
        &'a self,
        req: ExternalPromptDriverRequest,
    ) -> GeneratorsDetailFuture<'a, Result<ExternalPromptResult, DbError>>;

    /// v4 `runCharacterOptimizer(characterId, connectionProfileId, userId,
    /// repos, onProgress, options)` — never fails; every outcome is a frame
    /// through `on_progress` (the `error` frame included).
    fn optimize<'a>(
        &'a self,
        req: OptimizeDriverRequest,
        on_progress: OnProgress<'a>,
    ) -> GeneratorsDetailFuture<'a, ()>;
}

/// The projected `characterOptimize` a driver runs: the resolved single-user
/// id, the route's character id, the Zod-validated profile id + options, and
/// the wall clock the run is stamped with.
#[derive(Clone, Debug)]
pub struct OptimizeDriverRequest {
    pub user_id: String,
    pub character_id: String,
    pub connection_profile_id: String,
    pub options: OptimizerOptions,
    pub now_ms: i64,
}

// ===========================================================================
// Shared helpers
// ===========================================================================

fn not_found(resource: &str) -> Response {
    Response::error(ErrorKind::NotFound, format!("{resource} not found"))
}

/// v4's ownership gate: `repos.characters.findById(id)` (the OVERLAID read) +
/// `exists`. A broken vault's `CharacterVaultUnavailableError` answers the
/// contextful 503 (P4.23), as every other `[id]` read in this tree does.
fn require_character(db: &Db, character_id: &str) -> Result<Value, Response> {
    let cid = character_id.to_string();
    match db.read_main(|main| {
        db.read_mount_index(|mount| characters_read::find_by_id(main, mount, &cid))
    }) {
        Ok(Some(c)) => Ok(c),
        Ok(None) => Err(not_found("Character")),
        Err(e) => Err(db_error_response(e)),
    }
}

// ===========================================================================
// Zod 4 issue rendering (the `chat_create::CreateZodIssue` idiom, as values)
// ===========================================================================

/// `invalid_type` — Zod 4's key order `expected, code, path, message`.
fn invalid_type(expected: &str, path: &[Value], got: Option<&Value>) -> Value {
    json!({
        "expected": expected,
        "code": "invalid_type",
        "path": path,
        "message": format!("Invalid input: expected {expected}, received {}", zod_parsed_type(got)),
    })
}

/// `z.string().min(n, message)` — `origin, code, minimum, inclusive, path, message`.
fn too_small_string(minimum: usize, path: &[Value], message: &str) -> Value {
    json!({
        "origin": "string",
        "code": "too_small",
        "minimum": minimum,
        "inclusive": true,
        "path": path,
        "message": message,
    })
}

/// `z.string().uuid()` / `z.uuid()` — `origin, code, format, pattern, path,
/// message`, the pattern echoed verbatim (Zod 4's `$ZodUUID` check, the same
/// issue for both spellings — measured on the `zod_bad_uuids_*` arms).
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

/// `z.string().uuid()` over a raw value at `path`: a non-string is
/// `invalid_type`, a string that misses the pattern is `invalid_format`.
fn check_uuid(v: Option<&Value>, path: &[Value], issues: &mut Vec<Value>) -> Option<String> {
    match v {
        Some(Value::String(s)) => {
            if zod_uuid_ok(s) {
                Some(s.clone())
            } else {
                issues.push(invalid_uuid(path));
                None
            }
        }
        other => {
            issues.push(invalid_type("string", path, other));
            None
        }
    }
}

/// `z.number().int().min(lo).max(hi)`: the type check, then the integer check
/// (which ABORTS the bounds — a fractional value answers ONE issue, measured on
/// `zod_max_tokens_above_max_and_fractional`), then the two inclusive bounds.
fn check_int_range(
    v: Option<&Value>,
    lo: i64,
    hi: i64,
    path: &[Value],
    issues: &mut Vec<Value>,
) -> Option<i64> {
    let Some(n) = v.and_then(Value::as_f64) else {
        issues.push(invalid_type("number", path, v));
        return None;
    };
    if !n.is_finite() || n.fract() != 0.0 {
        issues.push(json!({
            "expected": "int",
            "format": "safeint",
            "code": "invalid_type",
            "path": path,
            "message": "Invalid input: expected int, received number",
        }));
        return None;
    }
    let mut ok = true;
    if n < lo as f64 {
        issues.push(json!({
            "origin": "number",
            "code": "too_small",
            "minimum": lo,
            "inclusive": true,
            "path": path,
            "message": format!("Too small: expected number to be >={lo}"),
        }));
        ok = false;
    }
    if n > hi as f64 {
        issues.push(json!({
            "origin": "number",
            "code": "too_big",
            "maximum": hi,
            "inclusive": true,
            "path": path,
            "message": format!("Too big: expected number to be <={hi}"),
        }));
        ok = false;
    }
    if ok {
        Some(n as i64)
    } else {
        None
    }
}

fn push_path(path: &[Value], key: Value) -> Vec<Value> {
    let mut p = path.to_vec();
    p.push(key);
    p
}

/// v4 `renameReplacementPairSchema` over one value at `path`:
/// `oldValue: z.string().min(1, 'Find value is required')`, `newValue:
/// z.string().min(1, 'Replace value is required')`, `caseSensitive:
/// z.boolean().default(false)`. Issues are appended in schema key order; a
/// non-object answers ONE `invalid_type` and does not descend.
fn parse_pair(v: &Value, path: &[Value], issues: &mut Vec<Value>) -> Option<ReplacementPair> {
    let Some(obj) = v.as_object() else {
        issues.push(invalid_type("object", path, Some(v)));
        return None;
    };
    let mut ok = true;
    let mut old_value = String::new();
    let mut new_value = String::new();
    for (key, message, slot) in [
        ("oldValue", "Find value is required", &mut old_value),
        ("newValue", "Replace value is required", &mut new_value),
    ] {
        let p = push_path(path, Value::String(key.into()));
        match obj.get(key) {
            Some(Value::String(s)) => {
                if !crate::jsstr::zod_len_min_ok(s, 1) {
                    issues.push(too_small_string(1, &p, message));
                    ok = false;
                } else {
                    *slot = s.clone();
                }
            }
            other => {
                issues.push(invalid_type("string", &p, other));
                ok = false;
            }
        }
    }
    let case_sensitive = match obj.get("caseSensitive") {
        None => false,
        Some(Value::Bool(b)) => *b,
        other => {
            issues.push(invalid_type(
                "boolean",
                &push_path(path, Value::String("caseSensitive".into())),
                other,
            ));
            ok = false;
            false
        }
    };
    if ok {
        Some(ReplacementPair {
            old_value,
            new_value,
            case_sensitive,
        })
    } else {
        None
    }
}

/// v4 `renameSchema.parse(body)` over the three raw body fields the verb
/// carries. `Err` is the middleware's `details` array (every issue, in schema
/// order — Zod collects them all).
pub fn parse_rename_body(
    primary_rename: Option<&Value>,
    additional_replacements: Option<&Value>,
    dry_run: Option<&Value>,
) -> Result<RenameRequest, Vec<Value>> {
    let mut issues: Vec<Value> = Vec::new();

    // `primaryRename: pairSchema.optional()` — absent is fine; `null` is not
    // (`.optional()` admits only `undefined`).
    let primary = match primary_rename {
        None => None,
        Some(v) => parse_pair(v, &[Value::String("primaryRename".into())], &mut issues),
    };

    // `additionalReplacements: z.array(pairSchema).default([])`.
    let mut additional: Vec<ReplacementPair> = Vec::new();
    match additional_replacements {
        None => {}
        Some(Value::Array(items)) => {
            for (i, item) in items.iter().enumerate() {
                let path = vec![
                    Value::String("additionalReplacements".into()),
                    Value::from(i),
                ];
                if let Some(p) = parse_pair(item, &path, &mut issues) {
                    additional.push(p);
                }
            }
        }
        other => issues.push(invalid_type(
            "array",
            &[Value::String("additionalReplacements".into())],
            other,
        )),
    }

    // `dryRun: z.boolean().default(true)`.
    let dry_run = match dry_run {
        None => true,
        Some(Value::Bool(b)) => *b,
        other => {
            issues.push(invalid_type(
                "boolean",
                &[Value::String("dryRun".into())],
                other,
            ));
            true
        }
    };

    if !issues.is_empty() {
        return Err(issues);
    }
    Ok(RenameRequest {
        primary_rename: primary,
        additional_replacements: additional,
        dry_run,
    })
}

// ===========================================================================
// characterRename (v4 `post.ts:425-445`)
// ===========================================================================

/// v4's `rename` action, whole: 404 → Zod 400 → the neither-pair 400 → the
/// service (on the writer pair, one transaction) → the catch-all 500.
pub async fn character_rename(
    db: &Db,
    user_id: &str,
    character_id: &str,
    primary_rename: Option<&Value>,
    additional_replacements: Option<&Value>,
    dry_run: Option<&Value>,
) -> Response {
    let character = match require_character(db, character_id) {
        Ok(c) => c,
        Err(r) => return r,
    };

    let request = match parse_rename_body(primary_rename, additional_replacements, dry_run) {
        Ok(r) => r,
        Err(issues) => return Response::validation_error(Value::Array(issues)),
    };

    if request.primary_rename.is_none() && request.additional_replacements.is_empty() {
        return Response::error(
            ErrorKind::BadRequest,
            "At least one replacement must be specified",
        );
    }

    let uid = user_id.to_string();
    let cid = character_id.to_string();
    // A dry run scans and writes NOTHING (v4's preview), so it runs on the
    // read pool and never takes the single writer — the §3 review's catch at
    // the `2f4254b42` unification: a preview over a real instance's chats and
    // messages used to block every other write for the whole scan. Only the
    // execute leg holds the writer pair (one transaction, the recorded shape).
    let outcome = if request.dry_run {
        db.read_main(|main| {
            db.read_mount_index(|mount| {
                run_character_rename(main, mount, &character, &request, &uid)
            })
        })
    } else {
        db.write(move |writers| {
            let mount = writers
                .mount_index()
                .ok_or_else(|| DbError::Internal("no mount-index database".into()))?
                .connection();
            let main = writers.main().connection();
            run_character_rename(main, mount, &character, &request, &uid)
        })
        .await
    };

    match outcome {
        Ok(result) => Response::Character(serde_json::to_value(result).unwrap_or(Value::Null)),
        Err(e) => {
            tracing::error!(
                character_id = %cid,
                error = %e,
                "[Characters v1] Error processing rename"
            );
            Response::error(ErrorKind::Internal, "Failed to process rename request")
        }
    }
}

// ===========================================================================
// characterRefreshArchive (v4 `post.ts:336-370`)
// ===========================================================================

/// v4's `refresh-archive` action: 404, then the render enqueue over every chat
/// the character sits in; the arm's own catch answers the fixed 500.
pub async fn character_refresh_archive(db: &Db, user_id: &str, character_id: &str) -> Response {
    if let Err(r) = require_character(db, character_id) {
        return r;
    }
    match refresh_archive(db, user_id, character_id).await {
        Ok(body) => Response::Character(body),
        Err(e) => {
            tracing::error!(
                character_id = %character_id,
                error = %e,
                "[Characters v1] Error refreshing conversation archive"
            );
            Response::error(
                ErrorKind::Internal,
                "Failed to refresh conversation archive",
            )
        }
    }
}

// ===========================================================================
// characterGenerateExternalPrompt (v4 `post.ts:316-334`)
// ===========================================================================

/// v4 `generateExternalPromptSchema.parse(body)`: `connectionProfileId:
/// z.string().uuid()`, `systemPromptId: z.string().uuid()`, `scenarioId:
/// z.string().uuid().optional()`, `maxTokens: z.number().int().min(1000)
/// .max(20000)`. `Err` is the middleware's `details` array.
pub fn parse_external_prompt_body(
    connection_profile_id: Option<&Value>,
    system_prompt_id: Option<&Value>,
    scenario_id: Option<&Value>,
    max_tokens: Option<&Value>,
) -> Result<ExternalPromptRequest, Vec<Value>> {
    let mut issues: Vec<Value> = Vec::new();
    let cp = check_uuid(
        connection_profile_id,
        &[Value::String("connectionProfileId".into())],
        &mut issues,
    );
    let sp = check_uuid(
        system_prompt_id,
        &[Value::String("systemPromptId".into())],
        &mut issues,
    );
    // `.optional()` admits only `undefined`; a present `null` is `invalid_type`.
    let sc = match scenario_id {
        None => None,
        Some(v) => check_uuid(Some(v), &[Value::String("scenarioId".into())], &mut issues),
    };
    let mt = check_int_range(
        max_tokens,
        1000,
        20000,
        &[Value::String("maxTokens".into())],
        &mut issues,
    );
    if !issues.is_empty() {
        return Err(issues);
    }
    Ok(ExternalPromptRequest {
        connection_profile_id: cp.unwrap_or_default(),
        system_prompt_id: sp.unwrap_or_default(),
        scenario_id: sc,
        max_tokens: mt.unwrap_or_default(),
    })
}

/// v4's `generate-external-prompt` action, whole: 404 → Zod 400 → the
/// `[Characters v1] External prompt generation starting` line → the driver
/// (or the named not-assembled refusal) → `{prompt, tokensUsed}` / the 500.
#[allow(clippy::too_many_arguments)]
pub async fn character_generate_external_prompt(
    db: &Db,
    driver: Option<&Arc<dyn GeneratorsDetailDriver>>,
    user_id: &str,
    character_id: &str,
    connection_profile_id: Option<&Value>,
    system_prompt_id: Option<&Value>,
    scenario_id: Option<&Value>,
    max_tokens: Option<&Value>,
) -> Response {
    if let Err(r) = require_character(db, character_id) {
        return r;
    }
    let request = match parse_external_prompt_body(
        connection_profile_id,
        system_prompt_id,
        scenario_id,
        max_tokens,
    ) {
        Ok(r) => r,
        Err(issues) => return Response::validation_error(Value::Array(issues)),
    };

    tracing::info!(
        user_id = %user_id,
        character_id = %character_id,
        connection_profile_id = %request.connection_profile_id,
        max_tokens = request.max_tokens,
        "[Characters v1] External prompt generation starting"
    );

    let Some(driver) = driver else {
        return Response::error(
            ErrorKind::Unavailable,
            "external prompt generation not available: no GeneratorsDetailDriver is assembled",
        );
    };
    match driver
        .external_prompt(ExternalPromptDriverRequest {
            user_id: user_id.to_string(),
            character_id: character_id.to_string(),
            request,
        })
        .await
    {
        Ok(result) if result.success => Response::Character(json!({
            "prompt": result.prompt,
            "tokensUsed": result.tokens_used,
        })),
        // v4 `serverError(result.error || 'Generation failed')` — JS `||`, so
        // an empty error sentence takes the default too.
        Ok(result) => Response::error(
            ErrorKind::Internal,
            result
                .error
                .filter(|e| !e.is_empty())
                .unwrap_or_else(|| "Generation failed".to_string()),
        ),
        // A DB failure escaping the service's reads is v4's uncaught throw →
        // the middleware's 500 (or the contextful 503 for a broken vault).
        Err(e) => db_error_response(e),
    }
}

// ===========================================================================
// characterOptimize (v4 `post.ts:92-143` `handleOptimizeStream`)
// ===========================================================================

/// v4 `optimizeStreamSchema.parse(body)` over the seven raw body fields the
/// verb carries. `Err` is the middleware's `details` array (every issue, in
/// schema key order — Zod collects them all). `Ok` is `(connectionProfileId,
/// the materialized options)`.
///
/// Measured on the pin (`zod` 4.5.4): `.optional().default(x)` admits only
/// `undefined` — an explicit `null` is `invalid_type` for the number / boolean
/// fields and `invalid_value` for the enum; the two `.nullable()` dates take
/// `null` as-is; `z.string().max(500)` counts code points (the P4.D158 rule,
/// `jsstr::zod_len_max_ok`).
#[allow(clippy::too_many_arguments)]
pub fn parse_optimize_body(
    connection_profile_id: Option<&Value>,
    max_memories: Option<&Value>,
    search_query: Option<&Value>,
    use_semantic_search: Option<&Value>,
    since_date: Option<&Value>,
    before_date: Option<&Value>,
    output_mode: Option<&Value>,
) -> Result<(String, OptimizerOptions), Vec<Value>> {
    let mut issues: Vec<Value> = Vec::new();
    let path = |key: &str| vec![Value::String(key.to_string())];

    // `connectionProfileId: z.string().uuid()`
    let profile_id = check_uuid(
        connection_profile_id,
        &path("connectionProfileId"),
        &mut issues,
    );

    // `maxMemories: z.number().int().min(5).max(200).optional().default(30)`
    let max_memories = match max_memories {
        None => Some(MAX_MEMORIES_FOR_ANALYSIS),
        Some(v) => check_int_range(Some(v), 5, 200, &path("maxMemories"), &mut issues),
    };

    // `searchQuery: z.string().max(500).optional().default('')`
    let search_query = match search_query {
        None => Some(String::new()),
        Some(Value::String(s)) => {
            if crate::jsstr::zod_len_max_ok(s, 500) {
                Some(s.clone())
            } else {
                issues.push(json!({
                    "origin": "string",
                    "code": "too_big",
                    "maximum": 500,
                    "inclusive": true,
                    "path": path("searchQuery"),
                    "message": "Too big: expected string to have <=500 characters",
                }));
                None
            }
        }
        other => {
            issues.push(invalid_type("string", &path("searchQuery"), other));
            None
        }
    };

    // `useSemanticSearch: z.boolean().optional().default(true)`
    let use_semantic_search = match use_semantic_search {
        None => Some(true),
        Some(Value::Bool(b)) => Some(*b),
        other => {
            issues.push(invalid_type("boolean", &path("useSemanticSearch"), other));
            None
        }
    };

    // `sinceDate` / `beforeDate: z.string().nullable().optional().default(null)`
    let mut nullable_string = |key: &str, v: Option<&Value>| -> Option<Option<String>> {
        match v {
            None | Some(Value::Null) => Some(None),
            Some(Value::String(s)) => Some(Some(s.clone())),
            other => {
                issues.push(invalid_type("string", &path(key), other));
                None
            }
        }
    };
    let since_date = nullable_string("sinceDate", since_date);
    let before_date = nullable_string("beforeDate", before_date);

    // `outputMode: z.enum(['apply', 'suggestions-file']).optional().default('apply')`
    let output_mode = match output_mode {
        None => Some(OptimizerOutputMode::Apply),
        Some(Value::String(s)) if s == "apply" => Some(OptimizerOutputMode::Apply),
        Some(Value::String(s)) if s == "suggestions-file" => {
            Some(OptimizerOutputMode::SuggestionsFile)
        }
        Some(_) => {
            issues.push(json!({
                "code": "invalid_value",
                "values": ["apply", "suggestions-file"],
                "path": path("outputMode"),
                "message": "Invalid option: expected one of \"apply\"|\"suggestions-file\"",
            }));
            None
        }
    };

    if !issues.is_empty() {
        return Err(issues);
    }
    Ok((
        profile_id.expect("no issues"),
        OptimizerOptions {
            max_memories: max_memories.expect("no issues"),
            search_query: search_query.expect("no issues"),
            use_semantic_search: use_semantic_search.expect("no issues"),
            since_date: since_date.expect("no issues"),
            before_date: before_date.expect("no issues"),
            output_mode: output_mode.expect("no issues"),
        },
    ))
}

/// v4's `optimize-stream` action, whole: 404 → Zod 400 → the route's info
/// line → the driver refusal → the runner, every `onProgress` published
/// through the K0 emitter under `progress_id` and the LAST event answered as
/// the `{ terminal }` dispatch payload (§B.1). The REST SSE edge
/// (`quilltap-web::generator_sse`) re-frames the same events as v4's
/// `data: <JSON>\n\n` stream.
#[allow(clippy::too_many_arguments)]
pub async fn character_optimize(
    db: &Db,
    driver: Option<&Arc<dyn GeneratorsDetailDriver>>,
    events: &broadcast::Sender<Event>,
    user_id: &str,
    character_id: &str,
    progress_id: Option<&str>,
    connection_profile_id: Option<&Value>,
    max_memories: Option<&Value>,
    search_query: Option<&Value>,
    use_semantic_search: Option<&Value>,
    since_date: Option<&Value>,
    before_date: Option<&Value>,
    output_mode: Option<&Value>,
    now_ms: i64,
) -> Response {
    if let Err(r) = require_character(db, character_id) {
        return r;
    }

    let (profile_id, options) = match parse_optimize_body(
        connection_profile_id,
        max_memories,
        search_query,
        use_semantic_search,
        since_date,
        before_date,
        output_mode,
    ) {
        Ok(parsed) => parsed,
        Err(issues) => return Response::validation_error(Value::Array(issues)),
    };

    tracing::info!(
        user_id = %user_id,
        character_id = %character_id,
        connection_profile_id = %profile_id,
        max_memories = options.max_memories,
        search_query = %if options.search_query.is_empty() { "(none)" } else { options.search_query.as_str() },
        use_semantic_search = options.use_semantic_search,
        since_date = %options.since_date.as_deref().unwrap_or("null"),
        before_date = %options.before_date.as_deref().unwrap_or("null"),
        output_mode = %options.output_mode.as_str(),
        "[Characters v1] Character optimizer starting (streaming)"
    );

    let Some(driver) = driver else {
        return Response::error(
            ErrorKind::Unavailable,
            "character optimization not available: no GeneratorsDetailDriver is assembled",
        );
    };

    let emitter =
        GeneratorProgressEmitter::from_id(progress_id, GeneratorKind::Optimizer, events.clone());
    let mut terminal: Option<Value> = None;
    {
        let mut sink = |event: Value| {
            emitter.emit(event.clone());
            terminal = Some(event);
        };
        driver
            .optimize(
                OptimizeDriverRequest {
                    user_id: user_id.to_string(),
                    character_id: character_id.to_string(),
                    connection_profile_id: profile_id,
                    options,
                    now_ms,
                },
                &mut sink,
            )
            .await;
    }
    Response::Character(json!({ "terminal": terminal }))
}
