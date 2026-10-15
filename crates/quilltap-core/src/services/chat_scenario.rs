//! `POST /api/v1/chats/[id]?action=scenario` — change the scene mid-conversation
//! — and `GET /api/v1/chats/[id]?action=scenario-source` — where the scene came
//! from, and whether the shelf copy has since been revised or removed.
//!
//! A differential port of v4 `app/api/v1/chats/[id]/actions/scenario.ts`
//! (`545e299bb`, pinned at `83334b9a5`).
//!
//! The payload mirrors the New Chat dialog's scenario fields exactly and runs
//! through the same [`resolve_scenario_selection`] chain, so a preset picked
//! from the sidebar lands as the same text it would have at chat creation.
//!
//! Three things have to happen together, or the change is only half-made:
//!   1. `chat.scenarioText` is rewritten — it feeds `{{scenario}}` in the
//!      identity stack — together with `scenarioSource` / `scenarioNotes`, the
//!      recipe that produced it.
//!   2. Every participant's precompiled identity stack is rebuilt, because that
//!      template variable is baked into the compiled copy.
//!   3. The Host announces the revision, so the change is visible to the company
//!      rather than silently rewriting the world between turns.
//!
//! Since `545e299bb` the second and third happen ONLY when the TEXT changed: a
//! change of provenance alone (the same words, now traced to their file) or of
//! the notes' bookkeeping is WRITTEN and answers `changed: true`, but needs
//! neither a recompile nor a bubble. The INFO `Scenario changed` still fires
//! on every write (with `textChanged`).
//!
//! ## Guard order — the COMPOSITE, measured
//!
//! `handleSetScenario` parses first and 404s second, but the route it hangs
//! from (`handlers/post.ts:114-118`) 404s on a missing chat BEFORE it dispatches
//! to any action handler. v5 has no separate route layer, so what this verb must
//! reproduce is the composite a client actually sees:
//!
//!   chat 404 → parse (400) → owning-character lookup → resolve (a refused
//!   `scenarioRef` 400) → keep the current scene unless `reload` → **no-op
//!   guard** (the WHOLE recipe) → write all three keys → if the text changed:
//!   recompile (try/warn) + announce (NOT wrapped) → log.
//!
//! An invalid body against a missing chat is therefore a **404**, not a 400 —
//! pinned by the corpus's `verb_bad_body_chat_missing` case, which exists
//! because the handler read in isolation says the opposite.
//!
//! ## Why the fields are raw `Value`s
//!
//! v4 validates with Zod inside the handler, so a wrong-TYPED field is v4's flat
//! `Validation error` 400. Typing them in the dispatch variant would instead
//! fail at the serde boundary, where the transport answers its own `Invalid
//! request: …` — a different sentence for the same input. The variant therefore
//! carries raw `Value`s and the Zod floors are reproduced here (the P4.60
//! wrong-type-collapse convention).
//!
//! `.nullish()` means an explicit `null` and an absent key are the SAME thing on
//! the seven nullish fields; `reload: z.boolean().optional()` is NOT nullable,
//! so `reload: null` is a 400.
//!
//! Pinned by `chat_scenario_routes_equivalence` (tier 2).

use serde_json::{json, Map, Value};

use crate::api::types::{ErrorKind, Response};
use crate::db::chats::{ChatUpdate, ScenarioRef, ScenarioSource};
use crate::db::runtime::Db;
use crate::db::vault_read_overlay::VaultUnavailable;
use crate::db::{characters_read, chats_read, DbError};
use crate::scenario_text::{combine_scenario_text, recover_preset_body};
use crate::scenarios::location::{lookup_scenario_ref, ScenarioRefLookup};
use crate::services::chat_participants::compile_all_stacks;
use crate::services::host_notifications::{
    post_host_scenario_revision_announcement, HostScenarioRevisionAnnouncement,
};
use crate::services::scenario_selection::{
    hash_scenario_body, resolve_scenario_selection, same_resolved_scenario,
    ResolveScenarioSelectionOptions, ResolvedScenario, ScenarioSelectionError,
    ScenarioSelectionFields,
};

/// v4's `logTag` for this route.
const LOG_TAG: &str = "[Chats v1]";

/// The `error` field of L28 / L31 as v4 renders it: `error.message`. A
/// character whose vault cannot be read is v4's `CharacterVaultUnavailableError`
/// — its message is ported as [`VaultUnavailable::message`]; v5's
/// `characters_read::find_by_id` carries the same id + mount on
/// `DbError::StoreUnavailable` but a v5 sentence in its `message`, so the
/// lines render v4's from the structured fields. (HANDOFF recorded by P4.D295:
/// that read's own message is the root fix, outside this file.)
fn v4_error_text(error: &DbError) -> String {
    match error {
        DbError::StoreUnavailable {
            entity_label: "character",
            id,
            mount_point_id: Some(mount_id),
            ..
        } => VaultUnavailable {
            character_id: id.clone(),
            mount_id: mount_id.clone(),
        }
        .message(),
        other => crate::db::fallback::error_text(other),
    }
}

/// v4's Zod arm (`lib/api/middleware/context.ts` → `validationError`): a schema
/// rejection is a 400 whose body is `{error: 'Validation error', details: […]}`.
/// v5's error envelope carries no `details` array — the standing, named P4.6bb
/// deferral — so only the `error` string is reproduced, and the differential
/// asserts that gap in BOTH directions.
fn validation_error() -> Response {
    Response::error(ErrorKind::BadRequest, "Validation error")
}

/// The eight `?action=scenario` body fields, raw. See the module header for
/// why they are `Value`s rather than typed strings.
#[derive(Debug, Default, Clone)]
pub struct SetScenarioBody {
    pub scenario: Option<Value>,
    /// v4 `scenarioRef: ScenarioRefSchema.nullish()` (`545e299bb`).
    pub scenario_ref: Option<Value>,
    /// v4 `reload: z.boolean().optional()` (`545e299bb`) — NOT nullable.
    pub reload: Option<Value>,
    pub scenario_id: Option<Value>,
    pub project_scenario_path: Option<Value>,
    pub group_scenario_path: Option<Value>,
    pub group_scenario_group_id: Option<Value>,
    pub general_scenario_path: Option<Value>,
}

/// The validated shape: `.nullish()` collapses "absent" and "null" into one.
#[derive(Debug, Default)]
struct ValidatedScenarioBody {
    scenario: Option<String>,
    scenario_ref: Option<ScenarioRef>,
    reload: Option<bool>,
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
        // Zod 4's RFC regex, not the bare shape (P4.162).
        Some(s) if crate::api::zod_issues::zod_uuid_ok(&s) => Ok(Some(s)),
        Some(_) => Err(()),
    }
}

/// `ScenarioRefSchema.nullish()` — `{mountPointId: z.uuid(), path:
/// z.string().min(1).max(500)}`, unknown keys stripped.
fn zod_scenario_ref(v: &Option<Value>) -> Result<Option<ScenarioRef>, ()> {
    match v {
        None | Some(Value::Null) => Ok(None),
        Some(value) => {
            if !crate::api::zod_issues::zod_scenario_ref_issues(value, &[]).is_empty() {
                return Err(());
            }
            let s = |k: &str| {
                value
                    .get(k)
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string()
            };
            Ok(Some(ScenarioRef {
                mount_point_id: s("mountPointId"),
                path: s("path"),
            }))
        }
    }
}

/// `z.boolean().optional()` — absent is fine; `null` and every non-boolean refuse.
fn zod_optional_bool(v: &Option<Value>) -> Result<Option<bool>, ()> {
    match v {
        None => Ok(None),
        Some(Value::Bool(b)) => Ok(Some(*b)),
        Some(_) => Err(()),
    }
}

/// v4 `setScenarioSchema.parse` (`app/api/v1/chats/[id]/schemas.ts:45-58`).
/// An entirely empty payload validates and CLEARS the scenario.
fn validate(body: &SetScenarioBody) -> Result<ValidatedScenarioBody, Response> {
    let parsed = (|| {
        Ok(ValidatedScenarioBody {
            scenario: zod_string(&body.scenario, None)?,
            scenario_ref: zod_scenario_ref(&body.scenario_ref)?,
            reload: zod_optional_bool(&body.reload)?,
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
/// `findCharacterOwningScenario` (`actions/scenario.ts:50-72`), which since
/// `545e299bb` returns the character's ID (the resolver reads the vault through
/// the location).
///
/// At creation there is one "primary character" to look the id up on; an
/// established chat may hold several, and the picker only offers character
/// scenarios when exactly one LLM character is present. Searching the whole
/// active cast keeps the lookup honest either way. A character whose vault is
/// unavailable is SKIPPED rather than allowed to fail the whole change.
///
/// ⚠ The filter is `type === 'CHARACTER'` + a present `characterId` +
/// `status !== 'removed'`. It does NOT look at `controlledBy`, so a
/// user-controlled seat is walked like any other. The walk reads the HYDRATED
/// character (`character.scenarios`), as v4's does.
fn find_character_owning_scenario(
    main: &rusqlite::Connection,
    mount: &rusqlite::Connection,
    chat: &Value,
    chat_id: &str,
    scenario_id: &str,
) -> Option<String> {
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
                    return character
                        .get("id")
                        .and_then(Value::as_str)
                        .map(str::to_string);
                }
            }
            Ok(None) => {}
            Err(error) => {
                tracing::debug!(
                    chatId = chat_id,
                    characterId = character_id,
                    error = %v4_error_text(&error),
                    "{LOG_TAG} Skipping character while resolving scenarioId"
                );
            }
        }
    }
    None
}

/// The chat's stored recipe — `{scenarioText, scenarioSource, scenarioNotes}`,
/// each `?? null` (v4 `previous`).
fn stored_recipe(chat: &Value) -> ResolvedScenario {
    let text = |k: &str| chat.get(k).and_then(Value::as_str).map(str::to_string);
    ResolvedScenario {
        scenario_text: text("scenarioText"),
        scenario_source: chat
            .get("scenarioSource")
            .and_then(|v| ScenarioSource::parse(v, &[]).ok()),
        scenario_notes: text("scenarioNotes"),
    }
}

/// v4 `keepCurrentScene(next, chat)` (`actions/scenario.ts:83-96`).
///
/// An ordinary save keeps the scene the chat has. When the request resolves to
/// the chat's own source file but that file has been revised since it was
/// picked, the chat's stored preset body (verified against the source's hash)
/// is kept, with the request's notes beneath it; only "Reload from source"
/// (`reload: true`) pulls the revised copy in. Returns `None` when there is
/// nothing to keep — a different file, an unchanged one, or a stored body that
/// no longer matches its hash — and the resolved recipe stands.
///
/// `current` is the chat's stored recipe (`scenarioText`, `scenarioSource`,
/// `scenarioNotes`) — the three fields v4 reads off the chat.
pub fn keep_current_scene(
    next: &ResolvedScenario,
    current: &ResolvedScenario,
) -> Option<ResolvedScenario> {
    let stored = current.scenario_source.as_ref()?;
    let incoming = next.scenario_source.as_ref()?;
    if stored.mount_point_id != incoming.mount_point_id || stored.path != incoming.path {
        return None;
    }
    if stored.body_sha256 == incoming.body_sha256 {
        return None;
    }
    let body = recover_preset_body(
        current.scenario_text.as_deref(),
        current.scenario_notes.as_deref(),
    )?;
    if hash_scenario_body(&body) != stored.body_sha256 {
        return None;
    }
    Some(ResolvedScenario {
        scenario_text: combine_scenario_text(Some(&body), next.scenario_notes.as_deref()),
        scenario_source: Some(stored.clone()),
        scenario_notes: next.scenario_notes.clone(),
    })
}

/// A throw v4's chat route catches as its own 500 (measured — the
/// `verb_malformed_percent_ref` row).
fn route_error() -> Response {
    Response::error(ErrorKind::Internal, "Internal server error")
}

/// The chat row read beside the resolver's answer.
type ChatAndRecipe = (Value, Result<ResolvedScenario, ScenarioSelectionError>);

/// v4 `handleSetScenario` — change (or clear) the chat's scenario.
pub async fn chat_set_scenario(
    db: &Db,
    chat_id: &str,
    body: SetScenarioBody,
    user_id: &str,
) -> Response {
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
    let resolved: Result<Option<ChatAndRecipe>, DbError> = db.read_main(|main| {
        db.read_mount_index(|mount| {
            let Some(chat) = chats_read::find_by_id(main, &cid)? else {
                return Ok(None);
            };
            let character_id = validated
                .scenario_id
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
                    scenario_ref: validated.scenario_ref.as_ref(),
                    scenario_id: validated.scenario_id.as_deref(),
                    project_scenario_path: validated.project_scenario_path.as_deref(),
                    group_scenario_path: validated.group_scenario_path.as_deref(),
                    group_scenario_group_id: validated.group_scenario_group_id.as_deref(),
                    general_scenario_path: validated.general_scenario_path.as_deref(),
                },
                &ResolveScenarioSelectionOptions {
                    user_id,
                    project_id: project_id.as_deref(),
                    character_id: character_id.as_deref(),
                    log_tag: Some(LOG_TAG),
                },
            );
            Ok(Some((chat, resolved)))
        })
    });

    let (chat, resolved) = match resolved {
        Ok(Some(v)) => v,
        Ok(None) => return Response::error(ErrorKind::NotFound, "Chat not found"),
        Err(e) => return Response::error(ErrorKind::Internal, e.to_string()),
    };
    let mut next = match resolved {
        Ok(next) => next,
        Err(refused @ ScenarioSelectionError::Refused(_)) => {
            return Response::error(ErrorKind::BadRequest, refused.to_string())
        }
        Err(ScenarioSelectionError::Location(_)) => return route_error(),
    };

    let previous = stored_recipe(&chat);

    if validated.reload != Some(true) {
        if let Some(kept) = keep_current_scene(&next, &previous) {
            tracing::debug!(
                chatId = chat_id,
                mountPointId = kept
                    .scenario_source
                    .as_ref()
                    .map_or("null", |s| s.mount_point_id.as_str()),
                path = kept
                    .scenario_source
                    .as_ref()
                    .map_or("null", |s| s.path.as_str()),
                "{LOG_TAG} Source file revised; keeping the chat's scene (no reload asked)"
            );
            next = kept;
        }
    }

    // A no-op change earns no announcement and no recompile — re-picking the
    // scene you already have shouldn't post a bubble saying it changed.
    if same_resolved_scenario(&next, &previous) {
        tracing::debug!(
            chatId = chat_id,
            "{LOG_TAG} Scenario unchanged; nothing to do"
        );
        return ok_body(next.scenario_text.as_deref(), false, "Scenario unchanged");
    }

    let cid = chat_id.to_string();
    let patch = next.clone();
    // v4 hands `compileAllIdentityStacks` the row `update` RETURNED, so the
    // recompile sees the NEW scene; v5's `update` answers a bool, so the row is
    // re-read inside the same write turn.
    let updated_chat = db
        .write(move |w| {
            let wrote = w.main().chats().update(
                &cid,
                &ChatUpdate {
                    scenario_text: Some(patch.scenario_text),
                    scenario_source: Some(patch.scenario_source),
                    scenario_notes: Some(patch.scenario_notes),
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
            tracing::warn!(chatId = chat_id, error = %e, "{LOG_TAG} Failed to update chat");
            return Response::error(ErrorKind::Internal, "Failed to update chat");
        }
    };

    // Only the text reaches the prompt. A change of provenance alone (the same
    // words, now traced to their file) needs neither a recompile nor a bubble.
    let text_changed = next.scenario_text != previous.scenario_text;
    if text_changed {
        // `chat.scenarioText` feeds the {{scenario}} template variable, which is
        // baked into every participant's precompiled identity stack — a changed
        // scene means every stack is stale. Non-fatal: the compiler has a
        // read-through fallback, so a failure here costs speed, not correctness.
        if let Err(error) = compile_all_stacks(db, updated_chat).await {
            tracing::warn!(
                chatId = chat_id,
                error = %v4_error_text(&error),
                "{LOG_TAG} Failed to recompile identity stacks after scenario change"
            );
        }

        // NOT wrapped in v4 either — a throw here propagates.
        post_host_scenario_revision_announcement(
            db,
            HostScenarioRevisionAnnouncement {
                chat_id: chat_id.to_string(),
                scenario_text: next.scenario_text.clone(),
            },
        )
        .await;
    }

    tracing::info!(
        chatId = chat_id,
        textChanged = text_changed,
        hadScenario = previous.scenario_text.is_some(),
        hasScenario = next.scenario_text.is_some(),
        // v4 logs `scenarioText?.length ?? 0` — a JS string length, i.e. UTF-16
        // code units.
        scenarioLength = next
            .scenario_text
            .as_deref()
            .map(|s| s.encode_utf16().count())
            .unwrap_or(0),
        source = source_label(&validated),
        sourceMountPointId = next
            .scenario_source
            .as_ref()
            .map_or("null", |s| s.mount_point_id.as_str()),
        "{LOG_TAG} Scenario changed"
    );

    ok_body(
        next.scenario_text.as_deref(),
        true,
        // v4 `next.scenarioText ? …` — JS truthiness (an empty text is falsy).
        if next.scenario_text.as_deref().is_some_and(|t| !t.is_empty()) {
            "Scenario updated"
        } else {
            "Scenario cleared"
        },
    )
}

/// v4 `handleGetScenarioSource(chatId, ctx)` (`actions/scenario.ts:226-266`):
/// where the chat's scene came from. `exists` is false when the source file
/// (or its owner) is gone; `changed` is true when the shelf copy's body no
/// longer hashes to what was picked. A chat with no source — a custom scene, or
/// one from before sources were recorded — answers `source: null`. The lookup's
/// WARN lines fire on EVERY GET of a refused source (L-3 — v4-faithful).
pub async fn chat_get_scenario_source(db: &Db, chat_id: &str, user_id: &str) -> Response {
    let cid = chat_id.to_string();
    let out: Result<Option<Value>, DbError> = db.read_main(|main| {
        db.read_mount_index(|mount| {
            let Some(chat) = chats_read::find_by_id(main, &cid)? else {
                return Ok(None);
            };
            let source = chat
                .get("scenarioSource")
                .and_then(|v| ScenarioSource::parse(v, &[]).ok());
            let notes = chat
                .get("scenarioNotes")
                .and_then(Value::as_str)
                .map_or(Value::Null, |n| json!(n));
            let Some(source) = source else {
                return Ok(Some(json!({
                    "source": null,
                    "owner": null,
                    "scenarioName": null,
                    "scenarioId": null,
                    "exists": false,
                    "changed": false,
                    "notes": notes,
                })));
            };
            let lookup = lookup_scenario_ref(
                main,
                mount,
                &ScenarioRef {
                    mount_point_id: source.mount_point_id.clone(),
                    path: source.path.clone(),
                },
                user_id,
            )
            .map_err(|e| DbError::Internal(e.to_string()))?;
            let (status, location, scenario) = match &lookup {
                ScenarioRefLookup::Ok { location, scenario } => {
                    ("ok", Some(location), Some(scenario))
                }
                ScenarioRefLookup::Missing { location, .. } => ("missing", Some(location), None),
                ScenarioRefLookup::Refused(_) => ("refused", None, None),
            };
            let owner = location.map_or(
                Value::Null,
                |l| json!({ "scope": l.scope().as_str(), "id": l.id(), "name": l.owner_name() }),
            );
            let exists = status == "ok";
            let changed =
                scenario.is_some_and(|s| hash_scenario_body(&s.body) != source.body_sha256);
            tracing::debug!(
                chatId = cid.as_str(),
                mountPointId = %source.mount_point_id,
                path = %source.path,
                status,
                changed,
                "{LOG_TAG} Scenario source checked"
            );
            let mut body = Map::new();
            body.insert(
                "source".into(),
                serde_json::to_value(&source).unwrap_or(Value::Null),
            );
            body.insert("owner".into(), owner);
            body.insert(
                "scenarioName".into(),
                scenario.map_or(Value::Null, |s| json!(s.name)),
            );
            // A character scenario's path-derived id, so the picker can show it
            // under its native heading (a `missing` character source still
            // answers it).
            body.insert(
                "scenarioId".into(),
                location
                    .and_then(|l| l.id_for_path(&source.path))
                    .map_or(Value::Null, Value::String),
            );
            body.insert("exists".into(), json!(exists));
            body.insert("changed".into(), json!(changed));
            body.insert("notes".into(), notes);
            Ok(Some(Value::Object(body)))
        })
    });
    match out {
        Ok(Some(body)) => Response::ChatAdmin(body),
        Ok(None) => Response::error(ErrorKind::NotFound, "Chat not found"),
        Err(_) => route_error(),
    }
}

/// v4's first-match cascade for the `source` log field (`actions/scenario.ts:
/// 199-209`): `'ref'` FIRST (`545e299bb`), then the legacy kinds. JS
/// truthiness, like the resolver's: v4 tests `validatedData.projectScenarioPath
/// ? …`, so an EMPTY string (which `z.string().max(500).nullish()` admits) is
/// falsy and the cascade falls through — a bare `is_some()` would label an
/// empty-path body `"project"` where v4 logs `"custom"`. (`scenario_id` cannot
/// be `Some("")` — `z.uuid()` refuses it — but reads the same way for
/// symmetry; a `scenarioRef` is an object, always truthy once present.)
fn source_label(v: &ValidatedScenarioBody) -> &'static str {
    let truthy = |s: &Option<String>| s.as_deref().is_some_and(|s| !s.is_empty());
    if v.scenario_ref.is_some() {
        "ref"
    } else if truthy(&v.scenario_id) {
        "character"
    } else if truthy(&v.project_scenario_path) {
        "project"
    } else if truthy(&v.group_scenario_path) {
        "group"
    } else if truthy(&v.general_scenario_path) {
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The `source` log cascade follows JS truthiness (the §3 unification
    /// review's catch): an empty-string path is FALSY in v4's ternary chain, so
    /// it must fall through rather than claim the label; a `scenarioRef` wins
    /// first (`545e299bb`).
    #[test]
    fn source_label_treats_empty_strings_as_falsy() {
        let empty_project_path = ValidatedScenarioBody {
            project_scenario_path: Some(String::new()),
            ..Default::default()
        };
        assert_eq!(source_label(&empty_project_path), "custom");

        let empty_beats_nothing = ValidatedScenarioBody {
            project_scenario_path: Some(String::new()),
            general_scenario_path: Some("Scenarios/g.md".into()),
            ..Default::default()
        };
        assert_eq!(source_label(&empty_beats_nothing), "general");

        let cascade = ValidatedScenarioBody {
            project_scenario_path: Some("Scenarios/p.md".into()),
            general_scenario_path: Some("Scenarios/g.md".into()),
            ..Default::default()
        };
        assert_eq!(source_label(&cascade), "project");

        let reffed = ValidatedScenarioBody {
            scenario_ref: Some(ScenarioRef {
                mount_point_id: "m".into(),
                path: "Scenarios/r.md".into(),
            }),
            scenario_id: Some("x".into()),
            ..Default::default()
        };
        assert_eq!(source_label(&reffed), "ref");

        assert_eq!(source_label(&ValidatedScenarioBody::default()), "custom");
    }

    /// `reload` is `z.boolean().optional()` — NOT nullable; `scenarioRef` is
    /// `.nullish()`.
    #[test]
    fn reload_null_refuses_and_a_null_ref_is_absent() {
        let refused = |body: SetScenarioBody| validate(&body).is_err();
        assert!(refused(SetScenarioBody {
            reload: Some(Value::Null),
            ..Default::default()
        }));
        assert!(refused(SetScenarioBody {
            reload: Some(json!("yes")),
            ..Default::default()
        }));
        assert!(!refused(SetScenarioBody {
            reload: Some(json!(true)),
            scenario_ref: Some(Value::Null),
            ..Default::default()
        }));
        assert!(refused(SetScenarioBody {
            scenario_ref: Some(json!({ "mountPointId": "x", "path": "Scenarios/a.md" })),
            ..Default::default()
        }));
    }
}
