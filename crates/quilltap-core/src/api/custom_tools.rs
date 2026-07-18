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
use crate::pascal::custom_tool_types::{display_title, Visibility};
use crate::pascal::custom_tools::{execute_custom_tool, CustomToolRunError, CustomToolRunResult};
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
    /// Boxed — the discovered definition dwarfs the other variants.
    Ready(Box<(DiscoveredCustomTool, Map<String, Value>)>),
}

/// v4 `handleRun` — run one tool at the operator's behest.
pub async fn chat_custom_tool_run(
    db: &Db,
    user_id: &str,
    chat_id: &str,
    tool: &str,
    parameters: Option<Map<String, Value>>,
    private: Option<bool>,
    as_character_id: Option<String>,
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
                    Ok(RunPrep::Ready(Box::new((entry.clone(), metadata))))
                }
            }
        })
    });

    let (entry, metadata) = match prep {
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
            let (entry, metadata) = *boxed;
            (entry, metadata)
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
    let mut rng = crate::tools::rng::OsRandomBytes;
    let result: CustomToolRunResult = match execute_custom_tool(
        &entry.definition,
        parameters.as_ref(),
        private,
        metadata_arg,
        &mut rng,
    ) {
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
