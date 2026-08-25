//! `POST /api/v1/chats/[id]?action=scenario` — change the scene mid-conversation.
//!
//! A differential port of v4 `app/api/v1/chats/[id]/actions/scenario.ts` (NEW at
//! v4 [`44a8137e`]).
//!
//! The payload mirrors the New Chat dialog's scenario fields exactly and runs
//! through the same [`resolve_scenario_selection`] chain, so a preset picked
//! from the sidebar lands as the same text it would have at chat creation.
//!
//! Three things have to happen together, or the change is only half-made:
//!   1. `chat.scenarioText` is rewritten — it feeds `{{scenario}}` in the
//!      identity stack.
//!   2. Every participant's precompiled identity stack is rebuilt, because that
//!      template variable is baked into the compiled copy.
//!   3. The Host announces the revision, so the change is visible to the company
//!      rather than silently rewriting the world between turns.
//!
//! ## Guard order — the COMPOSITE, measured
//!
//! `handleSetScenario` parses first and 404s second, but the route it hangs
//! from (`handlers/post.ts:114-118`) 404s on a missing chat BEFORE it dispatches
//! to any action handler. v5 has no separate route layer, so what this verb must
//! reproduce is the composite a client actually sees:
//!
//!   chat 404 → parse (400) → owning-character lookup → resolve →
//!   **no-op guard** → write → recompile (try/warn) → announce (NOT wrapped) →
//!   log.
//!
//! An invalid body against a missing chat is therefore a **404**, not a 400 —
//! pinned by the corpus's `verb_bad_body_chat_missing` case, which exists
//! because the handler read in isolation says the opposite.
//!
//! ## Why the fields are raw `Value`s
//!
//! v4 validates with Zod inside the handler, so a wrong-TYPED field is v4's flat
//! `Validation error` 400. Typing them `Option<String>` in the dispatch variant
//! would instead fail at the serde boundary, where the transport answers its own
//! `Invalid request: …` — a different sentence for the same input. The variant
//! therefore carries raw `Value`s and the Zod floors are reproduced here (the
//! P4.60 wrong-type-collapse convention).
//!
//! `.nullish()` means an explicit `null` and an absent key are the SAME thing on
//! every one of the six fields, which is why no tri-state survives to the
//! resolver — v4's chain reads both as falsy.
//!
//! Pinned by `chat_scenario_routes_equivalence` (tier 2).

use serde_json::{json, Map, Value};

use crate::api::types::{ErrorKind, Response};
use crate::db::chats::ChatUpdate;
use crate::db::runtime::Db;
use crate::db::{characters_read, chats_read, DbError};
use crate::services::chat_participants::compile_all_stacks_best_effort;
use crate::services::host_notifications::{
    post_host_scenario_revision_announcement, HostScenarioRevisionAnnouncement,
};
use crate::services::scenario_selection::{
    resolve_scenario_selection, ResolveScenarioSelectionOptions, ScenarioSelectionFields,
};

/// v4's `logTag` for this route.
const LOG_TAG: &str = "[Chats v1]";

/// v4's Zod arm (`lib/api/middleware/context.ts` → `validationError`): a schema
/// rejection is a 400 whose body is `{error: 'Validation error', details: […]}`.
/// v5's error envelope carries no `details` array — the standing, named P4.6bb
/// deferral — so only the `error` string is reproduced, and the differential
/// asserts that gap in BOTH directions.
fn validation_error() -> Response {
    Response::error(ErrorKind::BadRequest, "Validation error")
}

/// `z.uuid()` — v4 (Zod 4) accepts any RFC 9562 UUID text form: 8-4-4-4-12 hex
/// with the variant/version nibbles unconstrained beyond the shape it checks.
fn is_uuid(s: &str) -> bool {
    let b = s.as_bytes();
    if b.len() != 36 {
        return false;
    }
    for (i, c) in b.iter().enumerate() {
        match i {
            8 | 13 | 18 | 23 => {
                if *c != b'-' {
                    return false;
                }
            }
            _ => {
                if !c.is_ascii_hexdigit() {
                    return false;
                }
            }
        }
    }
    true
}

/// The six `?action=scenario` body fields, raw. See the module header for why
/// they are `Value`s rather than typed strings.
#[derive(Debug, Default, Clone)]
pub struct SetScenarioBody {
    pub scenario: Option<Value>,
    pub scenario_id: Option<Value>,
    pub project_scenario_path: Option<Value>,
    pub group_scenario_path: Option<Value>,
    pub group_scenario_group_id: Option<Value>,
    pub general_scenario_path: Option<Value>,
}

/// The validated shape: every field is `Option<String>` because `.nullish()`
/// collapses "absent" and "null" into one thing.
#[derive(Debug, Default)]
struct ValidatedScenarioBody {
    scenario: Option<String>,
    scenario_id: Option<String>,
    project_scenario_path: Option<String>,
    group_scenario_path: Option<String>,
    group_scenario_group_id: Option<String>,
    general_scenario_path: Option<String>,
}

/// `z.string().nullish()` (with an optional `.max()` in UTF-16 code units, which
/// is what Zod counts — `zod-string-check-order-and-length`). `null` and absent
/// both yield `Ok(None)`; anything not a string is a rejection.
fn zod_string(v: &Option<Value>, max: Option<usize>) -> Result<Option<String>, ()> {
    match v {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => {
            if let Some(max) = max {
                if s.encode_utf16().count() > max {
                    return Err(());
                }
            }
            Ok(Some(s.clone()))
        }
        Some(_) => Err(()),
    }
}

/// `z.uuid().nullish()`.
fn zod_uuid(v: &Option<Value>) -> Result<Option<String>, ()> {
    match zod_string(v, None)? {
        None => Ok(None),
        Some(s) if is_uuid(&s) => Ok(Some(s)),
        Some(_) => Err(()),
    }
}

/// v4 `setScenarioSchema.parse` (`app/api/v1/chats/[id]/schemas.ts`): six fields,
/// ALL `.nullish()`. An entirely empty payload validates and CLEARS the scenario.
fn validate(body: &SetScenarioBody) -> Result<ValidatedScenarioBody, Response> {
    let parsed = (|| {
        Ok(ValidatedScenarioBody {
            scenario: zod_string(&body.scenario, None)?,
            scenario_id: zod_uuid(&body.scenario_id)?,
            project_scenario_path: zod_string(&body.project_scenario_path, Some(500))?,
            group_scenario_path: zod_string(&body.group_scenario_path, Some(500))?,
            group_scenario_group_id: zod_uuid(&body.group_scenario_group_id)?,
            general_scenario_path: zod_string(&body.general_scenario_path, Some(500))?,
        })
    })();
    parsed.map_err(|()| validation_error())
}

/// Find which of the room's characters owns a given character-scenario id — v4
/// `findCharacterOwningScenario`.
///
/// At creation there is one "primary character" to look the id up on; an
/// established chat may hold several, and the picker only offers character
/// scenarios when exactly one LLM character is present. Searching the whole
/// active cast keeps the lookup honest either way. A character whose vault is
/// unavailable is SKIPPED rather than allowed to fail the whole change.
///
/// ⚠ The filter is `type === 'CHARACTER'` + a present `characterId` +
/// `status !== 'removed'`. It does NOT look at `controlledBy`, so a
/// user-controlled seat is walked like any other.
fn find_character_owning_scenario(
    main: &rusqlite::Connection,
    mount: &rusqlite::Connection,
    chat: &Value,
    chat_id: &str,
    scenario_id: &str,
) -> Option<Value> {
    let participants = chat.get("participants")?.as_array()?;
    for participant in participants {
        if participant.get("type").and_then(Value::as_str) != Some("CHARACTER") {
            continue;
        }
        let Some(character_id) = participant
            .get("characterId")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
        else {
            continue;
        };
        if participant.get("status").and_then(Value::as_str) == Some("removed") {
            continue;
        }
        match characters_read::find_by_id(main, mount, character_id) {
            Ok(Some(character)) => {
                let owns = character
                    .get("scenarios")
                    .and_then(Value::as_array)
                    .is_some_and(|arr| {
                        arr.iter()
                            .any(|s| s.get("id").and_then(Value::as_str) == Some(scenario_id))
                    });
                if owns {
                    return Some(character);
                }
            }
            Ok(None) => {}
            Err(error) => {
                tracing::debug!(
                    chat_id,
                    character_id,
                    error = %error,
                    "{LOG_TAG} Skipping character while resolving scenarioId"
                );
            }
        }
    }
    None
}

/// v4 `handleSetScenario` — change (or clear) the chat's scenario.
pub async fn chat_set_scenario(db: &Db, chat_id: &str, body: SetScenarioBody) -> Response {
    // The route's ownership gate runs BEFORE any action handler — see the
    // module header's composite-order note.
    let cid = chat_id.to_string();
    match db.read_main(move |c| chats_read::find_by_id(c, &cid)) {
        Ok(Some(_)) => {}
        Ok(None) => return Response::error(ErrorKind::NotFound, "Chat not found"),
        Err(e) => return Response::error(ErrorKind::Internal, e.to_string()),
    }

    let validated = match validate(&body) {
        Ok(v) => v,
        Err(r) => return r,
    };

    let cid = chat_id.to_string();
    let sid = validated.scenario_id.clone();
    let resolved: Result<Option<(Value, Option<String>)>, DbError> = db.read_main(|main| {
        db.read_mount_index(|mount| {
            let Some(chat) = chats_read::find_by_id(main, &cid)? else {
                return Ok(None);
            };
            let character = sid
                .as_deref()
                .and_then(|s| find_character_owning_scenario(main, mount, &chat, &cid, s));
            let project_id = chat
                .get("projectId")
                .and_then(Value::as_str)
                .map(str::to_string);
            let resolved = resolve_scenario_selection(
                main,
                mount,
                &ScenarioSelectionFields {
                    scenario: validated.scenario.as_deref(),
                    scenario_id: validated.scenario_id.as_deref(),
                    project_scenario_path: validated.project_scenario_path.as_deref(),
                    group_scenario_path: validated.group_scenario_path.as_deref(),
                    group_scenario_group_id: validated.group_scenario_group_id.as_deref(),
                    general_scenario_path: validated.general_scenario_path.as_deref(),
                },
                &ResolveScenarioSelectionOptions {
                    project_id: project_id.as_deref(),
                    character: character.as_ref(),
                    log_tag: Some(LOG_TAG),
                },
            )?;
            Ok(Some((chat, resolved)))
        })
    });

    let (chat, resolved) = match resolved {
        Ok(Some(v)) => v,
        Ok(None) => return Response::error(ErrorKind::NotFound, "Chat not found"),
        Err(e) => return Response::error(ErrorKind::Internal, e.to_string()),
    };

    let next_scenario_text = resolved;
    let previous_scenario_text = chat
        .get("scenarioText")
        .and_then(Value::as_str)
        .map(str::to_string);

    // A no-op change earns no announcement and no recompile — re-picking the
    // scene you already have shouldn't post a bubble saying it changed.
    if next_scenario_text == previous_scenario_text {
        tracing::debug!(chat_id, "{LOG_TAG} Scenario unchanged; nothing to do");
        return ok_body(next_scenario_text.as_deref(), false, "Scenario unchanged");
    }

    let cid = chat_id.to_string();
    let patch_text = next_scenario_text.clone();
    // v4 hands `compileAllIdentityStacks` the row `update` RETURNED, so the
    // recompile sees the NEW scene; v5's `update` answers a bool, so the row is
    // re-read inside the same write turn.
    let updated_chat = db
        .write(move |w| {
            let wrote = w.main().chats().update(
                &cid,
                &ChatUpdate {
                    scenario_text: Some(patch_text),
                    ..Default::default()
                },
            )?;
            if !wrote {
                return Ok(None);
            }
            chats_read::find_by_id(w.main().connection(), &cid)
        })
        .await;
    let updated_chat = match updated_chat {
        Ok(Some(c)) => c,
        // v4: `if (!updatedChat) return serverError('Failed to update chat')`.
        Ok(None) => return Response::error(ErrorKind::Internal, "Failed to update chat"),
        Err(e) => {
            tracing::warn!(chat_id, error = %e, "{LOG_TAG} Failed to update chat");
            return Response::error(ErrorKind::Internal, "Failed to update chat");
        }
    };

    // `chat.scenarioText` feeds the {{scenario}} template variable, which is
    // baked into every participant's precompiled identity stack — a changed
    // scene means every stack is stale. Non-fatal: the compiler has a
    // read-through fallback, so a failure here costs speed, not correctness.
    compile_all_stacks_best_effort(db, updated_chat).await;

    // NOT wrapped in v4 either — a throw here propagates.
    post_host_scenario_revision_announcement(
        db,
        HostScenarioRevisionAnnouncement {
            chat_id: chat_id.to_string(),
            scenario_text: next_scenario_text.clone(),
        },
    )
    .await;

    tracing::info!(
        chat_id,
        had_scenario = previous_scenario_text.is_some(),
        has_scenario = next_scenario_text.is_some(),
        // v4 logs `scenarioText?.length ?? 0` — a JS string length, i.e. UTF-16
        // code units.
        scenario_length = next_scenario_text
            .as_deref()
            .map(|s| s.encode_utf16().count())
            .unwrap_or(0),
        source = source_label(&validated),
        "{LOG_TAG} Scenario changed"
    );

    ok_body(
        next_scenario_text.as_deref(),
        true,
        if next_scenario_text.is_some() {
            "Scenario updated"
        } else {
            "Scenario cleared"
        },
    )
}

/// v4's first-match cascade for the `source` log field.
fn source_label(v: &ValidatedScenarioBody) -> &'static str {
    if v.scenario_id.is_some() {
        "character"
    } else if v.project_scenario_path.is_some() {
        "project"
    } else if v.group_scenario_path.is_some() {
        "group"
    } else if v.general_scenario_path.is_some() {
        "general"
    } else {
        "custom"
    }
}

/// v4's success envelope, in v4's key order.
fn ok_body(scenario_text: Option<&str>, changed: bool, message: &str) -> Response {
    let mut out = Map::new();
    out.insert(
        "scenarioText".into(),
        scenario_text.map_or(Value::Null, |s| json!(s)),
    );
    out.insert("changed".into(), json!(changed));
    out.insert("message".into(), json!(message));
    Response::ChatAdmin(Value::Object(out))
}
