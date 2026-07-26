//! Custom Tools API (Pascal the Croupier) — v4
//! `app/api/v1/chats/[id]/custom-tools/route.ts`.
//!
//! The operator's side of the felt. Characters reach for custom tools through
//! `run_custom` (the LLM tool); the human reaches for them through the composer
//! popup, and this is the endpoint behind it.
//!
//! - **GET** (`chat_custom_tools_list`) — the roster the popup lists, resolved
//!   FRESH per request. A roster is resolved *for an invoker* because a
//!   character-tier store shadows the farther tiers, and the human has no vault;
//!   so it resolves once per character participant and MERGES: a tool that
//!   resolves identically for everyone is listed once, unlabelled; one that
//!   differs is listed once per variant, tagged with the character whose
//!   perspective produced it. `asCharacterId` carries that choice back to POST.
//!   **The roll spec and the outcome table are NEVER in the payload** — the
//!   house does not show the odds.
//! - **POST `?action=run`** (`chat_custom_tool_run`) — run one tool at the
//!   operator's behest. Posts Pascal's outcome and nothing else: the
//!   announcement is byte-identical to a character's roll, so the transcript
//!   never records that the operator reached for the tool (`invokedBy: 'user'`
//!   in `pascalMeta` does). A private run is whispered to the OPERATOR's
//!   `userId` — a UUID that is NOT a participant id, so every character's context
//!   filter excludes it while the human still sees it.

use serde_json::{json, Map, Value};

use super::types::{ErrorKind, Response};
use crate::db::runtime::Db;
use crate::db::{characters_read, chats_read, DbError};
use crate::jsstr::js_trim;
use crate::pascal::custom_tool_types::{
    display_title, Visibility, MAX_LLM_OUTPUT_CEILING, MAX_LLM_OUTPUT_LENGTH,
};
use crate::pascal::custom_tools::{
    execute_custom_tool, CustomToolRunError, CustomToolRunResult, LlmInvokeOptions,
    LlmInvokeResult, LlmInvoker, LlmSubject,
};
use crate::pascal::llm_consult::{ConsultRunner, CustomToolConsultContext, SeamConsultInvoker};
use crate::pascal::roster::{
    resolve_custom_tool_roster, CustomToolRoster, DiscoveredCustomTool, RosterContext,
};
use crate::services::pascal_writer::{
    build_pascal_result_content, post_pascal_result, PostPascalResultParams,
};
use crate::services::prospero_notifications::{
    post_prospero_custom_tool_error, ProsperoCustomToolErrorArgs,
};

fn not_found(resource: &str) -> Response {
    Response::error(ErrorKind::NotFound, format!("{resource} not found"))
}
fn bad_request(msg: impl Into<String>) -> Response {
    Response::error(ErrorKind::BadRequest, msg)
}
fn internal(e: DbError) -> Response {
    Response::error(ErrorKind::Internal, e.to_string())
}

/// A character participant, paired with the vault that forms its `character`
/// tier (v4 `Perspective`).
#[derive(Clone)]
struct Perspective {
    character_id: String,
    character_name: String,
    character_mount_point_id: Option<String>,
    /// The character's hydrated fact sheet, for `when.metadata` tests on a run.
    metadata: Map<String, Value>,
}

/// The chat's character participants, each with its vault mount (v4
/// `loadPerspectives`). A character whose vault is unreachable is DROPPED rather
/// than fatal — one broken vault must not take the whole popup down.
fn load_perspectives(
    main: &rusqlite::Connection,
    mount: &rusqlite::Connection,
    chat: &Value,
) -> Vec<Perspective> {
    let mut perspectives = Vec::new();
    let participants = chat
        .get("participants")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    for participant in participants {
        if participant.get("type").and_then(Value::as_str) != Some("CHARACTER") {
            continue;
        }
        let Some(character_id) = participant.get("characterId").and_then(Value::as_str) else {
            continue;
        };
        // findById → null (skip) or a broken vault / read error (dropped).
        if let Ok(Some(character)) = characters_read::find_by_id(main, mount, character_id) {
            let metadata = character
                .get("metadata")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            perspectives.push(Perspective {
                character_id: character
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or(character_id)
                    .to_string(),
                character_name: character
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                character_mount_point_id: character
                    .get("characterDocumentMountPointId")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                metadata,
            });
        }
    }
    perspectives
}

/// Resolve the roster as one character sees it (v4 `resolveForPerspective`).
fn resolve_for_perspective(
    main: &rusqlite::Connection,
    mount: &rusqlite::Connection,
    user_id: &str,
    chat_id: &str,
    project_id: Option<&str>,
    perspective: &Perspective,
    all_character_ids: &[String],
) -> CustomToolRoster {
    resolve_custom_tool_roster(
        &RosterContext {
            user_id: user_id.to_string(),
            chat_id: chat_id.to_string(),
            character_id: Some(perspective.character_id.clone()),
            character_mount_point_id: perspective.character_mount_point_id.clone(),
            character_ids: Some(all_character_ids.to_vec()),
            project_id: project_id.map(str::to_string),
            metadata: None,
        },
        main,
        mount,
    )
}

/// Two variants are the same deal when they came from the same file in the same
/// store (v4 `variantKey`).
fn variant_key(entry: &DiscoveredCustomTool) -> String {
    format!("{}::{}", entry.mount_point_id, entry.definition_path)
}

/// The listing DTO (v4 `buildListing`) — deliberately odds-free (never the roll
/// spec or the outcome table).
fn build_listing(
    entry: &DiscoveredCustomTool,
    perspective: &Perspective,
    character_label: Option<&str>,
) -> Value {
    let params = entry
        .definition
        .parameters
        .as_ref()
        .map(|ps| {
            Value::Object(
                ps.iter()
                    .map(|(k, v)| (k.clone(), serde_json::to_value(v).unwrap_or(Value::Null)))
                    .collect(),
            )
        })
        .unwrap_or_else(|| Value::Object(Map::new()));
    let mut m = Map::new();
    m.insert("name".into(), json!(entry.definition.name));
    m.insert(
        "title".into(),
        json!(display_title(
            &entry.definition.name,
            entry.definition.title.as_deref()
        )),
    );
    m.insert("description".into(), json!(entry.definition.description));
    m.insert("parameters".into(), params);
    m.insert(
        "defaultVisibility".into(),
        json!(match entry.definition.default_visibility {
            Some(Visibility::Whisper) => "whisper",
            _ => "public",
        }),
    );
    m.insert(
        "sourceTier".into(),
        serde_json::to_value(entry.tier).unwrap_or(Value::Null),
    );
    if let Some(label) = character_label {
        m.insert("characterLabel".into(), json!(label));
    }
    m.insert("asCharacterId".into(), json!(perspective.character_id));
    m.insert("definitionPath".into(), json!(entry.definition_path));
    m.insert("mountPointId".into(), json!(entry.mount_point_id));
    m.insert("mountName".into(), json!(entry.mount_name));
    Value::Object(m)
}

fn error_entry(e: &crate::pascal::roster::CustomToolLoadError) -> Value {
    json!({
        "definitionPath": e.definition_path,
        "mountPointId": e.mount_point_id,
        "mountName": e.mount_name,
        "tier": serde_json::to_value(e.tier).unwrap_or(Value::Null),
        "reason": e.reason,
    })
}

/// v4 `handleList` — the roster for the popup.
pub fn chat_custom_tools_list(db: &Db, user_id: &str, chat_id: &str) -> Response {
    let user_id = user_id.to_string();
    let chat_id_owned = chat_id.to_string();
    let out = db.read_main(|main| {
        db.read_mount_index(|mount| {
            let Some(chat) = chats_read::find_by_id(main, &chat_id_owned)? else {
                return Ok(None);
            };
            let perspectives = load_perspectives(main, mount, &chat);
            if perspectives.is_empty() {
                return Ok(Some(json!({ "tools": [], "errors": [] })));
            }
            let all_character_ids: Vec<String> = perspectives
                .iter()
                .map(|p| p.character_id.clone())
                .collect();
            let project_id = chat
                .get("projectId")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty());

            // One roster per character. Errors are unioned by (mount, path) so the
            // same broken file seen from four perspectives earns one badge.
            let mut by_name: Vec<(String, Vec<(Perspective, DiscoveredCustomTool)>)> = Vec::new();
            let mut errors_by_key: Vec<(String, Value)> = Vec::new();
            let mut dropped_for_cap: Vec<String> = Vec::new();

            for perspective in &perspectives {
                let roster = resolve_for_perspective(
                    main,
                    mount,
                    &user_id,
                    &chat_id_owned,
                    project_id,
                    perspective,
                    &all_character_ids,
                );
                for (name, entry) in roster.tools {
                    if let Some((_, bucket)) = by_name.iter_mut().find(|(n, _)| *n == name) {
                        bucket.push((perspective.clone(), entry));
                    } else {
                        by_name.push((name, vec![(perspective.clone(), entry)]));
                    }
                }
                for e in &roster.errors {
                    let key = format!("{}::{}", e.mount_point_id, e.definition_path);
                    if !errors_by_key.iter().any(|(k, _)| *k == key) {
                        errors_by_key.push((key, error_entry(e)));
                    }
                }
                for name in roster.dropped_for_cap {
                    if !dropped_for_cap.contains(&name) {
                        dropped_for_cap.push(name);
                    }
                }
            }

            let mut tools: Vec<Value> = Vec::new();
            for (_name, sightings) in &by_name {
                let mut distinct: Vec<String> = Vec::new();
                for (_, entry) in sightings {
                    let k = variant_key(entry);
                    if !distinct.contains(&k) {
                        distinct.push(k);
                    }
                }
                if distinct.len() == 1 {
                    // Everyone resolves the same file: one unlabelled row.
                    let (perspective, entry) = &sightings[0];
                    tools.push(build_listing(entry, perspective, None));
                    continue;
                }
                // The name means different things to different characters.
                let mut seen: Vec<String> = Vec::new();
                for (perspective, entry) in sightings {
                    let key = format!("{}::{}", variant_key(entry), perspective.character_id);
                    if seen.contains(&key) {
                        continue;
                    }
                    seen.push(key);
                    tools.push(build_listing(
                        entry,
                        perspective,
                        Some(&perspective.character_name),
                    ));
                }
            }

            // Sorted by what the popup shows (title, then characterLabel). v4 uses
            // `localeCompare`; the code-unit order matches for the ASCII/BMP titles
            // the corpus carries (the established `[[vault-yaml-icu-decisions]]`
            // code-unit comparator).
            tools.sort_by(|a, b| {
                let at = a.get("title").and_then(Value::as_str).unwrap_or("");
                let bt = b.get("title").and_then(Value::as_str).unwrap_or("");
                at.cmp(bt).then_with(|| {
                    let al = a
                        .get("characterLabel")
                        .and_then(Value::as_str)
                        .unwrap_or("");
                    let bl = b
                        .get("characterLabel")
                        .and_then(Value::as_str)
                        .unwrap_or("");
                    al.cmp(bl)
                })
            });

            let errors: Vec<Value> = errors_by_key.into_iter().map(|(_, v)| v).collect();
            let mut data = Map::new();
            data.insert("tools".into(), Value::Array(tools));
            data.insert("errors".into(), Value::Array(errors));
            if !dropped_for_cap.is_empty() {
                data.insert("droppedForCap".into(), json!(dropped_for_cap));
            }
            Ok(Some(Value::Object(data)))
        })
    });

    match out {
        Ok(Some(data)) => Response::CustomToolsList(data),
        Ok(None) => not_found("Chat"),
        Err(e) => internal(e),
    }
}

/// The outcome of the read phase of a run (v4 `handleRun`'s guards).
enum RunPrep {
    NoChat,
    NoPerspectives,
    UnknownCharacter(String),
    UnknownTool {
        available: Vec<String>,
    },
    /// Boxed — the discovered definition dwarfs the other variants. Carries the
    /// tool entry, the AS-character's fact sheet, and the merged state cascade.
    Ready(Box<(DiscoveredCustomTool, Map<String, Value>, Value)>),
}

/// v4 `handleRun` — run one tool at the operator's behest.
#[allow(clippy::too_many_arguments)]
pub async fn chat_custom_tool_run(
    db: &Db,
    user_id: &str,
    chat_id: &str,
    tool: &str,
    parameters: Option<Map<String, Value>>,
    private: Option<bool>,
    as_character_id: Option<String>,
    // The assembled consult seam (v4 `handleRun` builds
    // `buildCustomToolLlmInvoker({ userId, chatId: id })` — always live there).
    // `None` is a v5-only state (read-only/canned assemblies); a tool whose
    // definition declares an `llm` block then answers the LOUD not-assembled
    // error rather than silently taking the fail-soft consult path.
    consult: Option<&dyn ConsultRunner>,
) -> Response {
    let user_id_owned = user_id.to_string();
    let chat_id_owned = chat_id.to_string();
    let tool_owned = tool.to_string();
    let as_char = as_character_id.clone();

    let prep = db.read_main(|main| {
        db.read_mount_index(|mount| {
            let Some(chat) = chats_read::find_by_id(main, &chat_id_owned)? else {
                return Ok(RunPrep::NoChat);
            };
            let perspectives = load_perspectives(main, mount, &chat);
            if perspectives.is_empty() {
                return Ok(RunPrep::NoPerspectives);
            }
            let perspective = match &as_char {
                Some(cid) => perspectives
                    .iter()
                    .find(|p| &p.character_id == cid)
                    .cloned(),
                None => perspectives.first().cloned(),
            };
            let Some(perspective) = perspective else {
                return Ok(RunPrep::UnknownCharacter(
                    as_char.clone().unwrap_or_default(),
                ));
            };
            let all_ids: Vec<String> = perspectives
                .iter()
                .map(|p| p.character_id.clone())
                .collect();
            let project_id = chat
                .get("projectId")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty());
            let roster = resolve_for_perspective(
                main,
                mount,
                &user_id_owned,
                &chat_id_owned,
                project_id,
                &perspective,
                &all_ids,
            );
            match roster.get(&tool_owned) {
                None => Ok(RunPrep::UnknownTool {
                    available: roster.tools.iter().map(|(k, _)| k.clone()).collect(),
                }),
                Some(entry) => {
                    // Metadata comes from the character the run is made AS. A run
                    // that names nobody rolls against an empty sheet.
                    let metadata = if as_char.is_some() {
                        perspective.metadata.clone()
                    } else {
                        Map::new()
                    };
                    // The merged state cascade the run's `$state` references
                    // resolve against (v4 `f48f34dc`). The group tier follows
                    // the same asymmetry as metadata above: scoped to the named
                    // character's own groups, or skipped entirely for a run
                    // nobody made (no `asCharacterId`), rather than borrowing an
                    // arbitrary participant's groups. Fail-soft — required
                    // fallbacks keep every run dealable.
                    let scope = match &as_char {
                        Some(cid) => crate::state::cascade::GroupScope::Character {
                            character_id: cid.clone(),
                        },
                        None => crate::state::cascade::GroupScope::None,
                    };
                    let tool_state = crate::state::cascade::resolve_state_cascade(
                        main,
                        Some(mount),
                        &chat,
                        &scope,
                    )
                    .merged;
                    Ok(RunPrep::Ready(Box::new((
                        entry.clone(),
                        metadata,
                        tool_state,
                    ))))
                }
            }
        })
    });

    let (entry, metadata, tool_state) = match prep {
        Ok(RunPrep::NoChat) => return not_found("Chat"),
        Ok(RunPrep::NoPerspectives) => {
            return bad_request(
                "This chat has no character whose perspective a custom tool could be run from",
            )
        }
        Ok(RunPrep::UnknownCharacter(id)) => {
            return bad_request(format!(
                "No character participant with id {id} is in this chat"
            ))
        }
        Ok(RunPrep::UnknownTool { available }) => {
            return bad_request(if available.is_empty() {
                format!("Unknown custom tool \"{tool}\". This chat has no custom tools available.")
            } else {
                format!(
                    "Unknown custom tool \"{tool}\". Available: {}",
                    available.join(", ")
                )
            })
        }
        Ok(RunPrep::Ready(boxed)) => {
            let (entry, metadata, tool_state) = *boxed;
            (entry, metadata, tool_state)
        }
        Err(e) => return internal(e),
    };

    // A private run is whispered to the OPERATOR's userId — NOT a participant id.
    let whispered = private == Some(true);
    let target_participant_ids = if whispered {
        Some(vec![user_id.to_string()])
    } else {
        None
    };

    let metadata_arg = if metadata.is_empty() {
        None
    } else {
        Some(&metadata)
    };
    let invoker = match (entry.definition.llm.as_ref(), consult) {
        (Some(_), Some(runner)) => Some(SeamConsultInvoker {
            runner,
            db,
            context: CustomToolConsultContext {
                user_id: user_id.to_string(),
                chat_id: Some(chat_id.to_string()),
            },
        }),
        // The composer arm surfaces the LOUD not-assembled error (mirroring
        // `ready_memory_embedding`): a canned/read-only assembly must answer
        // loudly, not slip into the fail-soft "no LLM invoker" consult path a
        // live turn deliberately keeps (see `tools/executor.rs`, where a wedged
        // tool call is worse than the author's `errorMessage`).
        (Some(_), None) => {
            return Response::error(
                ErrorKind::Internal,
                "custom-tool consult not assembled (the consult seam — this assembly holds no \
                 completion provider)",
            )
        }
        _ => None,
    };

    let mut rng = crate::tools::rng::OsRandomBytes;
    let result: CustomToolRunResult = match execute_custom_tool(
        &entry.definition,
        parameters.as_ref(),
        private,
        metadata_arg,
        Some(&tool_state),
        &mut rng,
        // v4 builds the invoker only when the definition declares an `llm`
        // block; the chat run attributes the consult to THIS chat, so a
        // dangerous room reroutes to the uncensored profile.
        invoker.as_ref().map(|i| i as &dyn LlmInvoker),
    )
    .await
    {
        Ok(r) => r,
        Err(CustomToolRunError(reason)) => {
            // Pascal never announces a run that did not happen — Prospero reports it.
            post_prospero_custom_tool_error(
                db,
                ProsperoCustomToolErrorArgs {
                    chat_id: chat_id.to_string(),
                    tool_name: tool.to_string(),
                    reason: reason.clone(),
                    whisper: whispered,
                    caller_participant_id: if whispered {
                        Some(user_id.to_string())
                    } else {
                        None
                    },
                },
            )
            .await;
            return bad_request(reason);
        }
    };

    let tool_title = display_title(&entry.definition.name, entry.definition.title.as_deref());
    let body = build_pascal_result_content(&tool_title, &result.message);

    // pascalMeta — `invokedBy: 'user'`, NO callerParticipantId (v4 handleRun).
    let mut pascal_meta = Map::new();
    pascal_meta.insert("tool".into(), json!(result.tool));
    pascal_meta.insert("toolTitle".into(), json!(tool_title));
    pascal_meta.insert(
        "definitionTier".into(),
        serde_json::to_value(entry.tier).unwrap_or(Value::Null),
    );
    pascal_meta.insert("definitionMountId".into(), json!(entry.mount_point_id));
    pascal_meta.insert(
        "params".into(),
        Value::Object(
            result
                .params
                .iter()
                .map(|(k, v)| (k.clone(), v.to_value()))
                .collect(),
        ),
    );
    pascal_meta.insert(
        "rollForm".into(),
        json!(match result.roll_form {
            crate::pascal::custom_tools::RollForm::Range => "range",
            crate::pascal::custom_tools::RollForm::Dice => "dice",
        }),
    );
    if let Some(notation) = &result.notation {
        pascal_meta.insert("notation".into(), json!(notation));
    }
    pascal_meta.insert("raw".into(), crate::db::js_number_to_json(result.raw));
    if let Some(dice_rolls) = &result.dice_rolls {
        pascal_meta.insert("diceRolls".into(), json!(dice_rolls));
    }
    pascal_meta.insert("value".into(), crate::db::js_number_to_json(result.value));
    pascal_meta.insert("state".into(), json!(result.state.as_str()));
    pascal_meta.insert("outcomeIndex".into(), json!(result.outcome_index));
    if let Some(metadata_tested) = &result.metadata_tested {
        pascal_meta.insert(
            "metadataTested".into(),
            Value::Object(
                metadata_tested
                    .iter()
                    .map(|(k, v)| (k.clone(), v.to_value()))
                    .collect(),
            ),
        );
    }
    if let Some(llm) = &result.llm {
        pascal_meta.insert("llm".into(), Value::Object(llm.to_wire()));
    }
    pascal_meta.insert("invokedBy".into(), json!("user"));

    let posted = post_pascal_result(
        db,
        PostPascalResultParams {
            chat_id: chat_id.to_string(),
            content: body.content,
            opaque_content: body.opaque_content,
            pascal_meta: Value::Object(pascal_meta),
            target_participant_ids,
        },
    )
    .await;

    let messages = match posted {
        Some(p) => vec![p.message],
        None => vec![],
    };
    Response::CustomToolRun(json!({
        "messages": messages,
        "result": {
            "tool": result.tool,
            "value": crate::db::js_number_to_json(result.value),
            "state": result.state.as_str(),
            "message": result.message,
            "whispered": whispered,
        },
    }))
}

// ===========================================================================
// P4.6ay unit 12 — Pascal's WORKBENCH collection resource
// (v4 `app/api/v1/custom-tools/route.ts`)
//
// The authoring surface, as opposed to the chat-facing popup above:
//
// - `custom_tools_library`      — every definition in every enabled store,
//                                 valid or broken, with attachment badges.
// - `custom_tools_destinations` — the save-target list, grouped by what each
//                                 store is attached to.
// - `custom_tool_preview`       — dry-run a definition through the ONE true
//                                 execution core. Posts nothing, writes nothing.
// - `custom_tool_audit`         — deal ten thousand hands and report where they
//                                 landed.
//
// File content I/O is deliberately NOT here: reads, writes, and deletes go
// through the existing mount-file verbs, so the Workbench adds no second write
// path into stores.
// ===========================================================================

/// Draws per audit (v4 `route.ts:34`). Roll + match only (no template
/// rendering), so this is cheap. **Server-fixed — no `runs` crosses the wire.**
const AUDIT_RUNS: usize = 10_000;

fn unprocessable(msg: impl Into<String>) -> Response {
    Response::error(ErrorKind::Unprocessable, msg)
}

/// v4 `handleLibrary`.
pub fn custom_tools_library(db: &Db) -> Response {
    match db.read_main(|main| {
        db.read_mount_index(|mount| {
            crate::pascal::workbench::build_custom_tool_library(main, mount)
        })
    }) {
        Ok(library) => {
            Response::CustomToolsLibrary(serde_json::to_value(library).unwrap_or(Value::Null))
        }
        Err(e) => internal(e),
    }
}

/// v4 `handleDestinations`.
pub fn custom_tools_destinations(db: &Db) -> Response {
    match db.read_main(|main| {
        db.read_mount_index(|mount| {
            crate::pascal::workbench::list_custom_tool_destinations(main, mount)
        })
    }) {
        Ok(destinations) => Response::CustomToolsDestinations(
            serde_json::to_value(destinations).unwrap_or(Value::Null),
        ),
        Err(e) => internal(e),
    }
}

/// v4 `parseDefinition` — validate a raw definition, or produce the 400 the
/// author reads. The rejection string is `formatDefinitionIssues`' own output,
/// which this port reproduces byte-for-byte (unit 1), so it crosses the wire
/// unmodified.
fn parse_definition(
    raw: &Value,
) -> Result<crate::pascal::custom_tool_types::QtapCustomTool, Response> {
    crate::pascal::custom_tool_types::safe_parse(raw).map_err(|issues| {
        bad_request(crate::pascal::custom_tool_types::format_definition_issues(
            &issues,
        ))
    })
}

/// v4's `PreviewBodySchema`/`AuditBodySchema` `params` field:
/// `z.record(z.string(), z.unknown()).nullish()`.
fn parse_params(raw: Option<&Value>) -> Result<Option<Map<String, Value>>, Response> {
    match raw {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Object(o)) => Ok(Some(o.clone())),
        Some(_) => Err(bad_request("Invalid request body")),
    }
}

/// §B (P4.d10): the `state` field — `z.record(z.string(), z.unknown()).nullish()`,
/// with v4's `?? {}` collapse applied by the CALLER (absent/null → `None` here).
fn parse_state(raw: Option<&Value>) -> Result<Option<Value>, Response> {
    match raw {
        None | Some(Value::Null) => Ok(None),
        Some(v @ Value::Object(_)) => Ok(Some(v.clone())),
        Some(_) => Err(bad_request("Invalid request body")),
    }
}

/// Resolve the fact sheet (v4 `resolveMetadata`).
///
/// A plain object passes through VERBATIM; the `{ characterId }` form hydrates
/// that character and uses their real `metadata.json` — the honest "what would
/// happen if THEY rolled this".
///
/// Two details are load-bearing and mirror v4 exactly:
///
/// 1. **The `{ characterId }` test is `resolveMetadata`'s own**, not the
///    schema's: exactly one key, named `characterId`, holding a string. The
///    schema's first union branch additionally demands `min(1)`, so
///    `{ characterId: "" }` falls through the union to the catch-all record —
///    and then this check still treats it as a character reference, hydrating
///    the empty id into a 404. Checking `min(1)` here would answer `{}` instead.
/// 2. **A broken vault is surfaced honestly, never papered over with `{}`** —
///    the bench would otherwise report metadata rows declining for the wrong
///    reason. The hydration therefore goes through the overlay directly rather
///    than `characters_read::find_by_id`, which flattens the vault-unavailable
///    case into a generic DB error and loses the 422.
fn resolve_metadata(db: &Db, metadata: Option<&Value>) -> Result<Map<String, Value>, Response> {
    let Some(metadata) = metadata else {
        return Ok(Map::new());
    };
    if metadata.is_null() {
        return Ok(Map::new());
    }
    let Some(obj) = metadata.as_object() else {
        return Err(bad_request("Invalid request body"));
    };

    let character_id = match obj.len() == 1 {
        true => obj.get("characterId").and_then(Value::as_str),
        false => None,
    };
    let Some(character_id) = character_id else {
        return Ok(obj.clone());
    };

    let character_id = character_id.to_string();
    let hydrated = db.read_main(|main| {
        db.read_mount_index(|mount| {
            let raw = characters_read::find_by_id_raw(main, &character_id)?;
            let repo = crate::db::doc_mount_documents::DocMountDocumentsRepository::new(mount);
            Ok(crate::db::vault_read_overlay::apply_document_store_overlay_one(&repo, raw))
        })
    });

    match hydrated {
        Err(e) => Err(internal(e)),
        Ok(Err(crate::db::vault_read_overlay::OverlayOneError::Db(e))) => Err(internal(e)),
        Ok(Err(crate::db::vault_read_overlay::OverlayOneError::Unavailable(u))) => {
            // v4 `CharacterVaultUnavailableError`'s message, byte-for-byte: the
            // SPA renders the server's string rather than re-deriving one.
            Err(unprocessable(format!(
                "Character {} has no usable vault (characterDocumentMountPointId={}): properties.json missing",
                u.character_id, u.mount_id
            )))
        }
        Ok(Ok(None)) => Err(not_found("Character")),
        Ok(Ok(Some(character))) => Ok(character
            .get("metadata")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default()),
    }
}

/// Serialize a run result to v4's `CustomToolRunResult` wire shape.
///
/// Field order and presence follow v4's interface declaration
/// (`custom-tools.ts:431-451`) exactly: `notation`, `diceRolls`, and
/// `metadataTested` are TypeScript-optional, so `JSON.stringify` OMITS them when
/// absent rather than emitting `null`.
fn run_result_to_value(r: &CustomToolRunResult) -> Value {
    let mut m = Map::new();
    m.insert("tool".into(), json!(r.tool));
    m.insert(
        "params".into(),
        Value::Object(
            r.params
                .iter()
                .map(|(k, v)| (k.clone(), v.to_value()))
                .collect(),
        ),
    );
    m.insert(
        "rollForm".into(),
        json!(match r.roll_form {
            crate::pascal::custom_tools::RollForm::Range => "range",
            crate::pascal::custom_tools::RollForm::Dice => "dice",
        }),
    );
    if let Some(notation) = &r.notation {
        m.insert("notation".into(), json!(notation));
    }
    m.insert("raw".into(), crate::db::js_number_to_json(r.raw));
    if let Some(dice_rolls) = &r.dice_rolls {
        m.insert("diceRolls".into(), json!(dice_rolls));
    }
    m.insert("value".into(), crate::db::js_number_to_json(r.value));
    m.insert("state".into(), json!(r.state.as_str()));
    m.insert("outcomeIndex".into(), json!(r.outcome_index));
    m.insert("message".into(), json!(r.message));
    m.insert("diceBreakdown".into(), json!(r.dice_breakdown));
    m.insert(
        "visibility".into(),
        json!(match r.visibility {
            Visibility::Whisper => "whisper",
            Visibility::Public => "public",
        }),
    );
    if let Some(metadata_tested) = &r.metadata_tested {
        m.insert(
            "metadataTested".into(),
            Value::Object(
                metadata_tested
                    .iter()
                    .map(|(k, v)| (k.clone(), v.to_value()))
                    .collect(),
            ),
        );
    }
    // `...(llm ? { llm } : {})` — last, mirroring v4's `CustomToolRunResult`
    // declaration. The preview RESPONSE is the whole run result, so the §A
    // record rides here verbatim.
    if let Some(llm) = &r.llm {
        m.insert("llm".into(), Value::Object(llm.to_wire()));
    }
    Value::Object(m)
}

/// v4's `LlmSimulationSchema` + the preview body's `live` arm.
///
/// `{ live: true }` is preview-ONLY: the audit body's union simply lacks it, so
/// "audits never call live" is a shape, not a runtime check.
enum BenchOracle {
    /// The real invoker, over the cheap-LLM machinery.
    Live,
    /// A pretend answer. Bounded by the CEILING, not the default — the
    /// definition's own `maxOutput` governs what the core keeps, and the bench
    /// must be able to script anything a tool could.
    Scripted(String),
    /// A pretend failure.
    Fail,
}

/// Parse the `llm` field of a preview/audit body. `allow_live` is false for an
/// audit. Returns `Ok(None)` for an absent or null field (the seam stays
/// unwired, so the consult fails soft into the author's `errorMessage`).
fn parse_bench_oracle(
    raw: Option<&Value>,
    allow_live: bool,
) -> Result<Option<BenchOracle>, Response> {
    let Some(raw) = raw else { return Ok(None) };
    if raw.is_null() {
        return Ok(None);
    }
    let Some(obj) = raw.as_object() else {
        return Err(bad_request("Invalid request body"));
    };

    // Each arm is a `strictObject`, so an extra key is a rejection.
    if allow_live && obj.len() == 1 && obj.get("live") == Some(&Value::Bool(true)) {
        return Ok(Some(BenchOracle::Live));
    }
    if obj.len() == 1 {
        if let Some(Value::String(output)) = obj.get("output") {
            let len = crate::jsstr::utf16_len(output);
            if len >= 1 && len as i64 <= MAX_LLM_OUTPUT_CEILING {
                return Ok(Some(BenchOracle::Scripted(output.clone())));
            }
            return Err(bad_request("Invalid request body"));
        }
        if obj.get("fail") == Some(&Value::Bool(true)) {
            return Ok(Some(BenchOracle::Fail));
        }
    }
    Err(bad_request("Invalid request body"))
}

/// The scripted invoker a bench oracle becomes. `Live` has no canned form — the
/// caller builds the real invoker for it.
struct ScriptedBenchInvoker {
    scripted: Option<String>,
}

impl LlmInvoker for ScriptedBenchInvoker {
    fn invoke<'a>(
        &'a self,
        _prompt: &'a str,
        _options: LlmInvokeOptions,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = LlmInvokeResult> + Send + 'a>> {
        Box::pin(async move {
            match &self.scripted {
                Some(output) => LlmInvokeResult::Answered {
                    output: output.clone(),
                    // v4's bench identity, carried verbatim so a scripted run is
                    // legible as one in the roll record.
                    provider: Some("bench".into()),
                    model: Some("simulated".into()),
                },
                None => LlmInvokeResult::Failed {
                    reason: "a simulated failure, as the bench requested".into(),
                },
            }
        })
    }
}

/// v4 `handlePreview` — one deal through the shared execution core.
/// `consult` is the assembled seam behind the `{live:true}` bench arm
/// (`CompletionProvider` itself is RPITIT and so not dyn-compatible — the
/// erased object is the [`ConsultRunner`], per `pascal::llm_consult`). A live
/// request against a `None` seam (read-only/canned assemblies — a v5-only
/// state) answers the LOUD not-assembled error.
#[allow(clippy::too_many_arguments)]
pub async fn custom_tool_preview(
    db: &Db,
    definition: &Value,
    params: Option<&Value>,
    private: Option<bool>,
    metadata: Option<&Value>,
    state: Option<&Value>,
    llm: Option<&Value>,
    user_id: &str,
    consult: Option<&dyn ConsultRunner>,
) -> Response {
    let params = match parse_params(params) {
        Ok(p) => p,
        Err(r) => return r,
    };
    // §B: mock merged state for `$state` refs (v4 `body.value.state ?? {}`).
    let state = match parse_state(state) {
        Ok(s) => s,
        Err(r) => return r,
    };
    let definition = match parse_definition(definition) {
        Ok(d) => d,
        Err(r) => return r,
    };
    let metadata = match resolve_metadata(db, metadata) {
        Ok(m) => m,
        Err(r) => return r,
    };

    // Which oracle answers the bench: the real one (live), a scripted one, or a
    // scripted failure. Absent input leaves the seam unwired, so the consult
    // fails soft into the author's errorMessage path.
    let oracle = match parse_bench_oracle(llm, true) {
        Ok(o) => o,
        Err(r) => return r,
    };
    let scripted = match &oracle {
        Some(BenchOracle::Scripted(output)) => Some(ScriptedBenchInvoker {
            scripted: Some(output.clone()),
        }),
        Some(BenchOracle::Fail) => Some(ScriptedBenchInvoker { scripted: None }),
        _ => None,
    };
    let live = match (&oracle, consult) {
        (Some(BenchOracle::Live), Some(runner)) => Some(SeamConsultInvoker {
            runner,
            db,
            context: CustomToolConsultContext {
                user_id: user_id.to_string(),
                // A bench run belongs to no room, and is therefore never
                // rerouted to the uncensored profile.
                chat_id: None,
            },
        }),
        // The bench arm surfaces the LOUD not-assembled error (mirroring
        // `ready_memory_embedding`) — an explicit `{live:true}` request against
        // a seamless assembly must not silently fail soft into the author's
        // `errorMessage`.
        (Some(BenchOracle::Live), None) => {
            return Response::error(
                ErrorKind::Internal,
                "custom-tool consult not assembled (the consult seam — this assembly holds no \
                 completion provider)",
            )
        }
        _ => None,
    };
    let invoker: Option<&dyn LlmInvoker> = match (&scripted, &live) {
        (Some(s), _) => Some(s),
        (_, Some(l)) => Some(l),
        _ => None,
    };

    let mut rng = crate::tools::rng::OsRandomBytes;
    match execute_custom_tool(
        &definition,
        params.as_ref(),
        private,
        Some(&metadata),
        state.as_ref(),
        &mut rng,
        invoker,
    )
    .await
    {
        Ok(result) => Response::CustomToolPreview(run_result_to_value(&result)),
        Err(CustomToolRunError(reason)) => unprocessable(reason),
    }
}

/// v4 `handleAudit` — ten thousand hands, reported by where they landed.
pub fn custom_tool_audit(
    db: &Db,
    definition: &Value,
    params: Option<&Value>,
    metadata: Option<&Value>,
    state: Option<&Value>,
    llm: Option<&Value>,
) -> Response {
    let params = match parse_params(params) {
        Ok(p) => p,
        Err(r) => return r,
    };
    // §B: mock merged state, held fixed across every draw.
    let state = match parse_state(state) {
        Ok(s) => s,
        Err(r) => return r,
    };
    let definition = match parse_definition(definition) {
        Ok(d) => d,
        Err(r) => return r,
    };
    let metadata = match resolve_metadata(db, metadata) {
        Ok(m) => m,
        Err(r) => return r,
    };

    // The fixed consult every draw shares. A definition with an `llm` block and
    // no scripted answer audits its FAILURE path — the honest default, since
    // that is the path a live table must always survive.
    let oracle = match parse_bench_oracle(llm, false) {
        Ok(o) => o,
        Err(r) => return r,
    };
    let fixed_llm = definition.llm.as_ref().map(|spec| {
        match &oracle {
            // Trimmed and capped exactly as a live run would keep it, so the
            // audit tests the answer the table would actually see.
            Some(BenchOracle::Scripted(output)) => {
                let max_output = spec.max_output.unwrap_or(MAX_LLM_OUTPUT_LENGTH as i64);
                let capped =
                    crate::jsstr::utf16_truncate(js_trim(output), max_output.max(0) as usize);
                LlmSubject {
                    ok: true,
                    output: js_trim(&capped).to_string(),
                }
            }
            // Both a scripted failure and an absent oracle audit the failure path.
            _ => LlmSubject {
                ok: false,
                output: spec.error_message.clone(),
            },
        }
    });

    let mut rng = crate::tools::rng::OsRandomBytes;
    match crate::pascal::custom_tools::simulate_outcomes(
        &definition,
        params.as_ref(),
        AUDIT_RUNS,
        Some(&metadata),
        fixed_llm.as_ref(),
        state.as_ref(),
        &mut rng,
    ) {
        Ok(result) => Response::CustomToolAudit(json!({
            "runs": result.runs,
            "outcomes": result.outcomes.iter().map(|o| json!({
                "index": o.index,
                "hits": o.hits,
                "share": crate::db::js_number_to_json(o.share),
            })).collect::<Vec<_>>(),
            "valueMin": crate::db::js_number_to_json(result.value_min),
            "valueMax": crate::db::js_number_to_json(result.value_max),
            "valueMean": crate::db::js_number_to_json(result.value_mean),
        })),
        Err(CustomToolRunError(reason)) => unprocessable(reason),
    }
}
