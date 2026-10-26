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
//!   resolves identically for everyone is listed once; one that differs is
//!   listed once per variant, tagged with the character whose perspective
//!   produced it. `asCharacterId` carries that choice back to POST.
//!   **The roll spec and the outcome table are NEVER in the payload** — the
//!   house does not show the odds.
//!
//!   Even the single-variant row must name somebody, because POST consults that
//!   character's fact sheet and groups. It names the operator's own played
//!   character whenever one is a candidate — see [`prefer_operator`], and the
//!   `characterLabel` it earns when none is.
//! - **POST `?action=run`** (`chat_custom_tool_run`) — run one tool at the
//!   operator's behest. Posts Pascal's outcome and nothing else: the
//!   announcement is byte-identical to a character's roll, so the transcript
//!   never records that the operator reached for the tool (`invokedBy: 'user'`
//!   in `pascalMeta` does). A private run is whispered to the OPERATOR's
//!   `userId` — a UUID that is NOT a participant id, so every character's context
//!   filter excludes it while the human still sees it.

use serde_json::{json, Map, Value};

use super::types::{ErrorKind, Response};
use crate::chat_predicates::{is_participant_present, ParticipantStatus};
use crate::db::runtime::Db;
use crate::db::{characters_read, chats_read, DbError};
use crate::jsstr::js_trim;
use crate::pascal::bench_wardrobe::resolve_bench_wardrobe;
// === LANE-REMOVE (P4.D329): C2 import ===
use crate::pascal::custom_tool_types::{
    display_title, Visibility, MAX_LLM_OUTPUT_CEILING, MAX_LLM_OUTPUT_LENGTH,
};
use crate::pascal::custom_tools::{
    execute_custom_tool, CustomToolRunError, CustomToolRunResult, LlmInvokeOptions,
    LlmInvokeResult, LlmInvoker, LlmSubject,
};
use crate::pascal::llm_consult::{ConsultRunner, CustomToolConsultContext, SeamConsultInvoker};
use crate::pascal::p4d329_c2_standins::InventoryContext;
use crate::pascal::roster::{
    resolve_custom_tool_roster, CustomToolRoster, DiscoveredCustomTool, RosterContext,
};
use crate::pascal::run_service::{
    run_custom_tool, InvokedBy, RunCustomToolOutcome, RunCustomToolParams,
};
use crate::pascal::tool_gate::{evaluate_tool_gate, has_tool_gate};
use crate::pascal::tool_vocabulary::collect_tool_vocabulary;
use crate::progressions::flatten_progressions;
use crate::services::prospero_notifications::{
    post_prospero_custom_tool_error, ProsperoCustomToolErrorArgs,
};

/// v4 `route.ts`'s `HANDLER` — the `context` field of the run arm's lines.
const HANDLER: &str = "api.v1.chats.custom-tools";

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
// P4.D169: mirrors v4 `resolveForPerspective` plus the run clock the progress
// sheet is derived against.
#[allow(clippy::too_many_arguments)]
fn resolve_for_perspective(
    main: &rusqlite::Connection,
    mount: &rusqlite::Connection,
    user_id: &str,
    chat_id: &str,
    project_id: Option<&str>,
    perspective: &Perspective,
    all_character_ids: &[String],
    // The listing's one clock reading, so every perspective in a chat is
    // resolved against the same instant rather than drifting across the loop.
    now_ms: i64,
) -> CustomToolRoster {
    resolve_custom_tool_roster(
        &RosterContext {
            user_id: user_id.to_string(),
            chat_id: chat_id.to_string(),
            character_id: Some(perspective.character_id.clone()),
            character_mount_point_id: perspective.character_mount_point_id.clone(),
            character_ids: Some(all_character_ids.to_vec()),
            project_id: project_id.map(str::to_string),
            // Availability gates are answered against this character's own sheet
            // — the same sheet the roster already resolves their vault tier
            // through, and already in hand from `load_perspectives`.
            metadata: Some(perspective.metadata.clone()),
            // P4.D329: the bag loads lazily, only when a gate names it.
            inventory: None,
        },
        main,
        mount,
        now_ms,
    )
}

/// Two variants are the same deal when they came from the same file in the same
/// store (v4 `variantKey`).
fn variant_key(entry: &DiscoveredCustomTool) -> String {
    format!("{}::{}", entry.mount_point_id, entry.definition_path)
}

/// v4's `isParticipantPresent(p.status ?? 'active')` over a raw participant
/// object.
///
/// The `?? 'active'` is v4's, and this port keeps it: a missing or `null`
/// `status` reads as present. (The persisted column always materializes
/// `status` through Zod, so the coalesce is belt-and-braces on both sides — but
/// a status v4 does not recognize is present in NEITHER, since its
/// `isParticipantPresent` is a pair of `===` tests.)
fn participant_is_present(p: &Value) -> bool {
    let status = match p.get("status") {
        None | Some(Value::Null) => "active",
        Some(Value::String(s)) => s.as_str(),
        // A non-string status fails both of v4's `===` comparisons.
        Some(_) => return false,
    };
    // Answered by the shared predicate, never by re-deriving the present set.
    // A status v4's enum does not carry fails both of its `===` tests.
    [
        ParticipantStatus::Active,
        ParticipantStatus::Silent,
        ParticipantStatus::Absent,
        ParticipantStatus::Removed,
    ]
    .into_iter()
    .find(|s| s.as_str() == status)
    .is_some_and(is_participant_present)
}

/// The characters the operator is playing, in the order to prefer them: the one
/// they are currently typing as first, then their remaining user-controlled
/// participants in stored order (v4 `operatorCharacterIds`).
///
/// A removed participant is not a candidate — the operator is not playing them
/// any more, whatever the roster still resolves through them.
fn operator_character_ids(chat: &Value) -> Vec<String> {
    let participants = chat
        .get("participants")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let active_typing = chat
        .get("activeTypingParticipantId")
        .and_then(Value::as_str);

    let own: Vec<&Value> = participants
        .iter()
        .filter(|p| {
            p.get("type").and_then(Value::as_str) == Some("CHARACTER")
                && p.get("controlledBy").and_then(Value::as_str) == Some("user")
                && participant_is_present(p)
        })
        .collect();

    // `own.find(p => p.id === activeTypingParticipantId)` — a null/absent
    // `activeTypingParticipantId` matches nobody (v4 compares against a string
    // `p.id`), so the list stays in stored order.
    let active = active_typing.and_then(|id| {
        own.iter()
            .position(|p| p.get("id").and_then(Value::as_str) == Some(id))
    });

    let ordered: Vec<&Value> = match active {
        Some(i) => std::iter::once(own[i])
            .chain(
                own.iter()
                    .enumerate()
                    .filter(|(j, _)| *j != i)
                    .map(|(_, p)| *p),
            )
            .collect(),
        None => own,
    };

    ordered
        .into_iter()
        .map(|p| {
            p.get("characterId")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        })
        .collect()
}

/// Choose whose perspective an otherwise-arbitrary choice is made from (v4
/// `preferOperator`).
///
/// When every character resolves a tool to the same file the perspective decides
/// nothing about *which* deal is dealt — but it still decides whose fact sheet
/// the `when.metadata` tests read and whose groups a `$state` reference resolves
/// through. "Arbitrary" must therefore not mean "whoever happens to be first"
/// while one of the candidates is the person actually pressing the button.
///
/// Falls back to stored order, and says so, when none of the operator's own
/// characters is a candidate: an all-LLM room, or a tool whose availability gate
/// their character did not pass.
///
/// Both call sites hold a non-empty `candidates` (v4's `candidates[0]` fallback
/// would be `undefined` otherwise, and both of its call sites guard first).
fn prefer_operator<'a, T>(
    candidates: &'a [T],
    character_id_of: impl Fn(&T) -> &str,
    operator_ids: &[String],
) -> (&'a T, bool) {
    for character_id in operator_ids {
        if let Some(hit) = candidates
            .iter()
            .find(|candidate| character_id_of(candidate) == character_id)
        {
            return (hit, true);
        }
    }
    (&candidates[0], false)
}

/// The listing DTO (v4 `buildListing`) — deliberately odds-free (never the roll
/// spec or the outcome table).
///
/// `character_label` is v4's `characterLabel`: whose version this is, when that
/// is not obvious. Set on a per-character variant, and on a shared tool that had
/// to fall back off the operator's own character. Absent means "runs as you", or
/// a room with nobody else in it.
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
    // What this tool quotes beyond its parameters — vocabulary, never odds
    // (v4 `faab6881`). Positioned after `parameters` and before
    // `defaultVisibility`, which is v4's own declaration order.
    m.insert(
        "references".into(),
        serde_json::to_value(collect_tool_vocabulary(&entry.definition)).unwrap_or(Value::Null),
    );
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
    // That character's vault mount, where run presets
    // (`Tools/{name}.{preset}.settings.json`) are kept (v4 `c988fbd2`). JSON
    // `null` — never absent, never invented — for a character whose vault could
    // not be resolved; the dialog hides its preset controls off this field.
    m.insert(
        "vaultMountPointId".into(),
        json!(perspective.character_mount_point_id),
    );
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
pub fn chat_custom_tools_list(
    db: &Db,
    user_id: &str,
    chat_id: &str,
    // ONE clock reading for the listing, so every perspective's gates are
    // answered against the same instant rather than drifting across the loop.
    now_ms: i64,
) -> Response {
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
            let operator_ids = operator_character_ids(&chat);

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
                    now_ms,
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
                    // Everyone resolves the same file: one row. The perspective
                    // picks no variant here, but it is still whose sheet a run
                    // consults, so it goes to the operator's own played
                    // character rather than to whoever the participants array
                    // happens to lead with.
                    let (chosen, is_operator) = prefer_operator(
                        sightings,
                        |(p, _): &(Perspective, DiscoveredCustomTool)| p.character_id.as_str(),
                        &operator_ids,
                    );
                    let (perspective, entry) = chosen;
                    // Unlabelled when it runs as the operator themselves, or
                    // when the room holds one character and there is nothing to
                    // disambiguate. Otherwise the row names the character it
                    // will run as: a run that borrows somebody else's secrets
                    // should say whose before the operator commits to it.
                    let label = if is_operator || perspectives.len() == 1 {
                        None
                    } else {
                        Some(perspective.character_name.clone())
                    };
                    tools.push(build_listing(entry, perspective, label.as_deref()));
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
            // v4 `faab6881`'s `logger.debug('Custom-tool roster resolved for the
            // operator', {…})`, same point and same counts.
            tracing::debug!(
                target: "quilltap::pascal",
                chat_id = %chat_id_owned,
                perspectives = perspectives.len(),
                tools = tools.len(),
                errors = errors.len(),
                dropped_for_cap = dropped_for_cap.len(),
                "Custom-tool roster resolved for the operator",
            );
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
    /// tool entry, the AS-character's fact sheet, the chat row (handed to the
    /// run service so its cascade read is saved — v4 `runCustomTool({ chat })`),
    /// and the resolved perspective's character id.
    Ready(Box<(DiscoveredCustomTool, Map<String, Value>, Value, String)>),
}

/// v4 `runSchema` (`route.ts:371-378`), ported whole:
///
/// ```text
/// tool:          z.string().min(1, 'A tool name is required')
/// parameters:    z.record(z.string(), z.unknown()).nullish()
/// private:       z.boolean().optional()
/// asCharacterId: z.string().nullish()
/// ```
///
/// `handleRun` calls `runSchema.parse` — **uncaught** — so every failure
/// surfaces through v4's middleware as the FLAT 400 `{error: 'Validation
/// error'}` (`context.ts:166`; the `details` issue array is the standing
/// project-wide deferral, same as the groups/wardrobe routes). The per-issue
/// sentences (`'A tool name is required'` included) only ever appear INSIDE
/// `details`, so this parser reports pass/fail and nothing finer.
///
/// This is what P4.60 replaced at the web edge: reading each key with
/// `and_then(Value::as_str)`/`as_bool`/`as_object` collapsed absent, explicit
/// `null`, and a present-but-WRONG-TYPED value into one `None`, so a body v4
/// refuses outright ran as if the caller had said nothing.
#[derive(Debug, Clone, PartialEq)]
pub struct CustomToolRunBody {
    pub tool: String,
    /// `nullish()` — absent and `null` are indistinguishable downstream
    /// (`body.parameters ?? undefined`).
    pub parameters: Option<Map<String, Value>>,
    pub private: Option<bool>,
    /// `nullish()`, and every downstream read is `body.asCharacterId ? … : …`
    /// — so an EMPTY string is the falsy branch, exactly like absent. Normalized
    /// here so both entrances (this edge and the dispatch verb) agree.
    pub as_character_id: Option<String>,
}

/// Parse a raw `POST ?action=run` body against v4's `runSchema`.
///
/// `Err` is v4's uncaught-ZodError 400, already shaped.
pub fn parse_run_body(body: &Value) -> Result<CustomToolRunBody, Response> {
    let invalid = || {
        Response::error(
            ErrorKind::BadRequest,
            crate::services::chat_participants::VALIDATION_ERROR,
        )
    };

    // `z.object` — a non-object input is itself an invalid_type issue.
    let Some(obj) = body.as_object() else {
        return Err(invalid());
    };

    // tool: z.string().min(1) — required. Lengths are UTF-16 units, but `min(1)`
    // only separates empty from non-empty, and no encoding disagrees there.
    let tool = match obj.get("tool") {
        Some(Value::String(s)) if !s.is_empty() => s.clone(),
        _ => return Err(invalid()),
    };

    // parameters: z.record(...).nullish() — an ARRAY is `received array`, not a
    // record (probed on v4's zod 4.4.3), so `as_object` is the right gate.
    let parameters = match obj.get("parameters") {
        None | Some(Value::Null) => None,
        Some(Value::Object(m)) => Some(m.clone()),
        Some(_) => return Err(invalid()),
    };

    // private: z.boolean().optional() — optional but NOT nullable, so an
    // explicit `null` is a ZodError where an absent key is not.
    let private = match obj.get("private") {
        None => None,
        Some(Value::Bool(b)) => Some(*b),
        Some(_) => return Err(invalid()),
    };

    // asCharacterId: z.string().nullish() — then the handler's truthiness gate.
    let as_character_id = match obj.get("asCharacterId") {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) => Some(s.clone()),
        Some(_) => return Err(invalid()),
    };

    Ok(CustomToolRunBody {
        tool,
        parameters,
        private,
        as_character_id,
    })
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
    // Epoch milliseconds at run start — ONE reading for the roster's gates, the
    // progress sheet, `{{now}}` and the `updatedAt` an effect stamps (v4
    // `chats/[id]/custom-tools/route.ts:456`). Injected so a differential can
    // freeze it; production passes `now_unix_ms()`.
    now_ms: i64,
) -> Response {
    let user_id_owned = user_id.to_string();
    let chat_id_owned = chat_id.to_string();
    let tool_owned = tool.to_string();
    // v4 reads `body.asCharacterId` through a truthiness gate at all four sites
    // (perspective choice, `metadata`, the group scope, the effect `characterId`),
    // so an EMPTY string means "nobody named" — not "a character whose id is ''".
    // `parse_run_body` already normalizes the edge's copy; this keeps the
    // dispatch entrance (the SPA's path) from disagreeing with it.
    let as_character_id = as_character_id.filter(|s| !s.is_empty());
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
            // A run that names nobody still has to resolve the roster through
            // someone's vault tier, so it uses the same preference GET does. It
            // consults no fact sheet either way (see the metadata note below) —
            // this only decides which definition of a shadowed name gets dealt.
            let perspective = match &as_char {
                Some(cid) => perspectives
                    .iter()
                    .find(|p| &p.character_id == cid)
                    .cloned(),
                None => Some(
                    prefer_operator(
                        &perspectives,
                        |p: &Perspective| p.character_id.as_str(),
                        &operator_character_ids(&chat),
                    )
                    .0
                    .clone(),
                ),
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
                now_ms,
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
                    Ok(RunPrep::Ready(Box::new((
                        entry.clone(),
                        metadata,
                        chat,
                        perspective.character_id.clone(),
                    ))))
                }
            }
        })
    });

    let (entry, metadata, chat, perspective_character_id) = match prep {
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
            tracing::debug!(
                target: "quilltap::pascal",
                context = HANDLER,
                chatId = chat_id,
                tool = tool,
                availableJson = %serde_json::to_string(&available).unwrap_or_default(),
                "Manual custom-tool run named an unknown tool",
            );
            return bad_request(if available.is_empty() {
                format!("Unknown custom tool \"{tool}\". This chat has no custom tools available.")
            } else {
                format!(
                    "Unknown custom tool \"{tool}\". Available: {}",
                    available.join(", ")
                )
            });
        }
        Ok(RunPrep::Ready(boxed)) => {
            let (entry, metadata, chat, perspective_character_id) = *boxed;
            (entry, metadata, chat, perspective_character_id)
        }
        Err(e) => return internal(e),
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

    // A private run is whispered to the OPERATOR's userId — a UUID that is not
    // a participant id, so every character's context filter excludes it while
    // the "show all whispers" toggle still reveals it to the human.
    //
    // Metadata comes from the character the run is made AS; a run that names
    // nobody is one nobody made: an empty sheet (and progress sheet), its group
    // tier skipped, and its metadata effects written to nobody — an
    // unattributed operator roll must not borrow, or edit, an arbitrary
    // participant's secrets. The deal itself is the shared run service's
    // (`pascal::run_service`, v4 `lib/pascal/run-service.ts`).
    let mut rng = crate::tools::rng::OsRandomBytes;
    let outcome = run_custom_tool(
        db,
        RunCustomToolParams {
            user_id,
            chat_id,
            chat: Some(&chat),
            entry: &entry,
            parameters: parameters.as_ref(),
            private,
            character_id: as_character_id
                .as_ref()
                .map(|_| perspective_character_id.as_str()),
            metadata: &metadata,
            // v4's operator route passes no name → the service reads the RAW row.
            character_name: None,
            whisper_target_ids: Some(vec![user_id.to_string()]),
            invoked_by: InvokedBy::User,
            caller_participant_id: None,
            trigger_phrase: None,
            now_ms,
            // v4's route (`route.ts:451`) passes no set: an operator-run
            // gesture enqueues its announcement at once.
            pending_wardrobe_announcements: None,
            pending_wardrobe_history: None,
            pending_inventory_announcements: None,
        },
        &mut rng,
        // The chat run attributes the consult to THIS chat, so a dangerous room
        // reroutes to the uncensored profile.
        invoker.as_ref().map(|i| i as &dyn LlmInvoker),
    )
    .await;

    let (result, posted) = match outcome {
        Ok(RunCustomToolOutcome::Ok { result, posted, .. }) => (result, posted),
        Ok(RunCustomToolOutcome::Refused { reason, whisper }) => {
            // Pascal never announces a run that did not happen — the failure is
            // Prospero's to report. `whisper` is the service's (`private ??
            // defaultVisibility === 'whisper'`), so a default-whisper tool's
            // refusal bubble is whispered to the operator (v4 `362426c8f`).
            tracing::debug!(
                target: "quilltap::pascal",
                context = HANDLER,
                chatId = chat_id,
                tool = tool,
                reason = reason.as_str(),
                "Manual custom-tool run refused",
            );
            post_prospero_custom_tool_error(
                db,
                ProsperoCustomToolErrorArgs {
                    chat_id: chat_id.to_string(),
                    tool_name: tool.to_string(),
                    reason: reason.clone(),
                    whisper,
                    caller_participant_id: if whisper {
                        Some(user_id.to_string())
                    } else {
                        None
                    },
                },
            )
            .await;
            return bad_request(reason);
        }
        // v4 has no `try` around the service: a rethrow is the middleware's 500.
        Err(e) => return internal(e),
    };

    // The DEFINITION's visibility, not `private === true` (v4 `362426c8f`): a
    // default-whisper tool run with no `private` is whispered, and says so.
    let whispered = result.visibility == Visibility::Whisper;

    tracing::info!(
        target: "quilltap::pascal",
        context = HANDLER,
        chatId = chat_id,
        tool = result.tool.as_str(),
        state = result.state.as_str(),
        whispered,
        "[Pascal] Manual custom-tool run completed",
    );

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

/// Everything both bench actions need before they can deal (v4 `0246c6c8`'s
/// `parseBenchRequest`).
struct BenchRequest {
    params: Option<Map<String, Value>>,
    state: Option<Value>,
    definition: crate::pascal::custom_tool_types::QtapCustomTool,
    metadata: Map<String, Value>,
    /// The seated character's name, for `{{character}}` — kept only when
    /// non-empty (v4's truthy spread); `None` for a hand-typed sheet. Untrimmed:
    /// `execute_custom_tool` trims (v4 `a05c6f755`). The audit ignores it.
    character_name: Option<String>,
}

/// The steps both bench actions open with: read the params and the mock state,
/// validate the definition, resolve the fact sheet. Each can bail with the 4xx
/// the author reads, so the caller gets either everything it needs or the
/// response to return instead.
///
/// v4's `benchRefusal` — the other half of the same collapse — has no analog
/// worth extracting here: it exists to re-`throw` anything that is not a
/// `CustomToolRunError`, and v5's `Err(CustomToolRunError(reason))` arm already
/// names that one case, with no other error to pass along.
fn parse_bench_request(
    db: &Db,
    definition: &Value,
    params: Option<&Value>,
    metadata: Option<&Value>,
    state: Option<&Value>,
) -> Result<BenchRequest, Response> {
    let params = parse_params(params)?;
    // §B: mock merged state for `$state` refs (v4 `body.value.state ?? {}`).
    let state = parse_state(state)?;
    let definition = parse_definition(definition)?;
    let (metadata, character_name) = resolve_metadata(db, metadata)?;
    Ok(BenchRequest {
        params,
        state,
        definition,
        metadata,
        character_name: character_name.filter(|n| !n.is_empty()),
    })
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

// === P4.D329: the bench's mock bag ===
/// v4 `inventory: MockInventorySchema.nullish()` — absent / `null` → no bag
/// (`None`: inventory tests decline); `[]` is a bag with nothing in it; a bad
/// row is v4's `badRequest('Invalid request body', issues)` (the `details`
/// envelope stays the standing P4.6bb deferral).
fn parse_bench_inventory(
    raw: Option<&Value>,
) -> Result<Option<crate::pascal::bench_inventory::MockInventoryContext>, Response> {
    match crate::pascal::bench_inventory::parse_mock_inventory(raw) {
        Ok(rows) => {
            Ok(rows.map(|rows| crate::pascal::bench_inventory::mock_inventory_context(&rows)))
        }
        Err(_) => Err(bad_request("Invalid request body")),
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
/// Three details are load-bearing and mirror v4 exactly:
///
/// 1. **The `{ characterId }` test is [`bench_character_id`]**, not the
///    schema's: exactly one key, named `characterId`, holding a NON-EMPTY
///    string. So `{ characterId: "" }` (which fails the schema's `min(1)`
///    branch and lands in the record branch) is a HAND-TYPED sheet: 200 with
///    the sheet as given (v4 `5cb0e0190` — it was a 404 before); preview and
///    audit share it.
/// 2. **A broken vault is surfaced honestly, never papered over with `{}`** —
///    the bench would otherwise report metadata rows declining for the wrong
///    reason. The hydration therefore goes through the overlay directly rather
///    than `characters_read::find_by_id`, which flattens the vault-unavailable
///    case into a generic DB error and loses the 422.
/// 3. **The seated character's name answers `{{character}}`**; a hand-typed
///    sheet names nobody, so the placeholder renders as written, as for a run
///    nobody made (v4 `a05c6f755`). The name is the OVERLAID row's, untrimmed.
fn resolve_metadata(
    db: &Db,
    metadata: Option<&Value>,
) -> Result<(Map<String, Value>, Option<String>), Response> {
    let Some(metadata) = metadata else {
        return Ok((Map::new(), None));
    };
    if metadata.is_null() {
        return Ok((Map::new(), None));
    }
    let Some(obj) = metadata.as_object() else {
        return Err(bad_request("Invalid request body"));
    };

    let Some(character_id) = bench_character_id(Some(metadata)) else {
        return Ok((obj.clone(), None));
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
            // P4.D172 moved the formatter onto the error itself so the list
            // overlay's `Dropping character from list` log carries the SAME
            // `detail` v4's does.
            Err(unprocessable(u.message()))
        }
        Ok(Ok(None)) => Err(not_found("Character")),
        Ok(Ok(Some(character))) => Ok((
            character
                .get("metadata")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default(),
            character
                .get("name")
                .and_then(Value::as_str)
                .map(str::to_string),
        )),
    }
}

/// v4 `benchCharacterId` (`5cb0e0190`) — the character a bench request seats
/// in the chair, when its fact sheet is the `{ characterId }` form: an object
/// of EXACTLY one key, `characterId` a non-empty string.
fn bench_character_id(metadata: Option<&Value>) -> Option<&str> {
    let obj = metadata?.as_object()?;
    if obj.len() != 1 {
        return None;
    }
    obj.get("characterId")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
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
    // `chipLabel` then `effects` (v4 `c4d4b0de`), after `llm` — the declaration
    // order of `CustomToolRunResult`, which IS the preview body's key order
    // because `handlePreview` spreads the whole result. The bench shows the
    // resolved effects as a DRY run and applies nothing; the audit ignores both.
    if let Some(chip_label) = &r.chip_label {
        m.insert("chipLabel".into(), json!(chip_label));
    }
    if let Some(effects) = &r.effects {
        m.insert(
            "effects".into(),
            serde_json::to_value(effects).unwrap_or(Value::Null),
        );
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

/// v4 `handlePreview` — one deal through the shared execution core. Posts
/// nothing, writes nothing. With a character in the chair, also says which
/// garment each wardrobe gesture would have found (a read — v4 `5cb0e0190`'s
/// `wardrobeResolution`, the body's LAST key).
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
    // P4.D329: the mock inventory rows (v4 `body.inventory`).
    inventory: Option<&Value>,
    llm: Option<&Value>,
    user_id: &str,
    consult: Option<&dyn ConsultRunner>,
    // The bench's one clock reading (v4 `benchNowMs`), shared by the preview's
    // progress sheet, its gate verdict and `{{now}}`.
    now_ms: i64,
) -> Response {
    // v4 reads the chair from the RAW body (`benchCharacterId(body.metadata)`),
    // not the resolved sheet — taken before the sheet shadows it.
    let chair = bench_character_id(metadata).map(str::to_string);
    // === P4.D329: the mock bag — part of v4's BODY schema, so a bad row is
    // the body 400, ahead of the definition's ===
    let inventory = match parse_bench_inventory(inventory) {
        Ok(bag) => bag,
        Err(r) => return r,
    };
    let BenchRequest {
        params,
        state,
        definition,
        metadata,
        character_name,
    } = match parse_bench_request(db, definition, params, metadata, state) {
        Ok(p) => p,
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

    // The bench's sheet: derived from the body's own `metadata` (which may carry
    // a `progressions` block) at the bench's one clock reading.
    let progress = flatten_progressions(
        Some(&Value::Object(metadata.clone())),
        now_ms,
        &mut |_, _| {},
    );

    let mut rng = crate::tools::rng::OsRandomBytes;
    match execute_custom_tool(
        &definition,
        params.as_ref(),
        private,
        Some(&metadata),
        state.as_ref(),
        &mut rng,
        invoker,
        Some(&progress),
        now_ms,
        character_name.as_deref(),
        // P4.D329: `body.inventory ? mockInventoryContext(…) : null`.
        inventory.as_ref().map(|b| b as &dyn InventoryContext),
    )
    .await
    {
        Ok(result) => {
            let mut body = run_result_to_value(&result);
            // With a character in the chair, say which garment each wardrobe
            // gesture would have found — the applier's own lookup, and a read
            // only. A hand-typed sheet names nobody, so it gets no resolution
            // (v4 `5cb0e0190`). Runs AFTER the deal, so a `{{character}}` in a
            // ref is already the seated name.
            let wardrobe_resolution = chair.as_deref().and_then(|id| {
                resolve_bench_wardrobe(
                    db,
                    id,
                    result.effects.as_deref().unwrap_or_default(),
                    &definition.name,
                )
            });
            // The bench always deals — a gate decides whether a character is
            // OFFERED a tool, not whether the author may test one they are
            // holding. The verdict rides along so the bench can say "and this
            // character would never have been dealt it", which a roll alone
            // could never reveal. Present ONLY when the definition gates
            // (v4 `6864bf0e`'s `...(hasToolGate(...) ? {gate} : {})`).
            if has_tool_gate(&definition) {
                if let Some(obj) = body.as_object_mut() {
                    obj.insert(
                        "gate".into(),
                        serde_json::to_value(evaluate_tool_gate(
                            &definition,
                            Some(&metadata),
                            Some(&progress),
                            inventory.as_ref().map(|b| b as &dyn InventoryContext),
                        ))
                        .unwrap_or(Value::Null),
                    );
                }
            }
            // v4's response spread puts `progress` AFTER `gate`, and only when
            // the sheet is non-empty — a bench body with no `progressions` block
            // answers exactly as it did before this feature.
            if !progress.is_empty() {
                if let Some(obj) = body.as_object_mut() {
                    obj.insert("progress".into(), Value::Object(progress.clone()));
                }
            }
            // ...and `wardrobeResolution` LAST, only when defined.
            if let Some(resolution) = wardrobe_resolution {
                if let Some(obj) = body.as_object_mut() {
                    obj.insert("wardrobeResolution".into(), Value::Array(resolution));
                }
            }
            Response::CustomToolPreview(body)
        }
        Err(CustomToolRunError(reason)) => unprocessable(reason),
    }
}

/// v4 `handleAudit` — ten thousand hands, reported by where they landed.
#[allow(clippy::too_many_arguments)]
pub fn custom_tool_audit(
    db: &Db,
    definition: &Value,
    params: Option<&Value>,
    metadata: Option<&Value>,
    state: Option<&Value>,
    // P4.D329: the mock inventory rows, held fixed across every draw.
    inventory: Option<&Value>,
    llm: Option<&Value>,
    // The audit's one clock reading: the progress sheet is derived ONCE and held
    // fixed across every draw, so hit rates for a table branching on
    // `cannon.complete` are conditional on that single instant — the caveat the
    // bench already states for a pretend consult.
    now_ms: i64,
) -> Response {
    // §B's mock state is held fixed across every draw. The audit never
    // renders, so it has no use for the seated name (v4 `handleAudit`).
    // P4.D329: the mock bag — the body schema's, ahead of the definition.
    let inventory = match parse_bench_inventory(inventory) {
        Ok(bag) => bag,
        Err(r) => return r,
    };
    let BenchRequest {
        params,
        state,
        definition,
        metadata,
        character_name: _,
    } = match parse_bench_request(db, definition, params, metadata, state) {
        Ok(p) => p,
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

    // Derived ONCE and held fixed across every draw (v4 `handleAudit` passes
    // `flattenProgressions(metadata, Date.now())` as the trailing argument).
    let progress = flatten_progressions(
        Some(&Value::Object(metadata.clone())),
        now_ms,
        &mut |_, _| {},
    );

    let mut rng = crate::tools::rng::OsRandomBytes;
    match crate::pascal::custom_tools::simulate_outcomes(
        &definition,
        params.as_ref(),
        AUDIT_RUNS,
        Some(&metadata),
        fixed_llm.as_ref(),
        state.as_ref(),
        &mut rng,
        Some(&progress),
        // P4.D329: the mock bag, held fixed across every draw.
        inventory.as_ref().map(|b| b as &dyn InventoryContext),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::tiered_mount_pool::MountTier;

    fn entry() -> DiscoveredCustomTool {
        DiscoveredCustomTool {
            definition: crate::pascal::custom_tool_types::safe_parse(&json!({
                "name": "coin_toss",
                "description": "d",
                "outcomes": [{"when": true, "message": "-", "state": "info"}]
            }))
            .unwrap(),
            tier: MountTier::Global,
            mount_point_id: "mp-1".into(),
            mount_name: "Global".into(),
            definition_path: "Tools/coin_toss.tool.json".into(),
        }
    }

    fn perspective(vault: Option<&str>) -> Perspective {
        Perspective {
            character_id: "char-1".into(),
            character_name: "Charlie".into(),
            character_mount_point_id: vault.map(str::to_string),
            metadata: Map::new(),
        }
    }

    /// v4 `c988fbd2`: the listing carries the running character's vault mount,
    /// and a character with none must say so with `null` — not invent one, and
    /// not omit the key. The dialog hides its preset controls off this field,
    /// so an absent key would read as "no vault" only by accident of JS.
    ///
    /// The committed `pascal-run-custom-*` fixture cannot reach the None arm
    /// (all three of its live characters have vaults, and CHAR_D's broken vault
    /// drops that perspective entirely), so the route differential pins the
    /// Some arm and this pins the other — the same split v4 uses, whose own
    /// coverage of null is likewise unit-level.
    #[test]
    fn vault_mount_point_id_is_json_null_when_the_character_has_no_vault() {
        let with_vault = build_listing(&entry(), &perspective(Some("vault-charlie")), None);
        assert_eq!(
            with_vault.get("vaultMountPointId"),
            Some(&json!("vault-charlie"))
        );

        let without = build_listing(&entry(), &perspective(None), None);
        assert_eq!(without.get("vaultMountPointId"), Some(&Value::Null));
    }

    /// Key ORDER is part of the route differential's diff: v4 inserts the field
    /// after `asCharacterId` and before `definitionPath`.
    #[test]
    fn vault_mount_point_id_sits_between_as_character_id_and_definition_path() {
        let listing = build_listing(&entry(), &perspective(Some("v")), None);
        let keys: Vec<&str> = listing
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        let at = |k: &str| keys.iter().position(|s| *s == k).unwrap();
        assert_eq!(at("vaultMountPointId"), at("asCharacterId") + 1);
        assert_eq!(at("definitionPath"), at("vaultMountPointId") + 1);
    }
}

#[cfg(test)]
mod bench_character_id_tests {
    //! v4 `benchCharacterId` (`5cb0e0190`): an object of EXACTLY one key,
    //! `characterId` a NON-EMPTY string — the route family pins the 200s
    //! (`preview-empty-character-id`, `audit-empty-character-id`,
    //! `preview-character-id-plus-key`); these pin the predicate's corners.
    use super::bench_character_id;
    use serde_json::json;

    #[test]
    fn seats_only_a_lone_non_empty_character_id() {
        assert_eq!(
            bench_character_id(Some(&json!({ "characterId": "x" }))),
            Some("x")
        );
        assert_eq!(
            bench_character_id(Some(&json!({ "characterId": "" }))),
            None
        );
        assert_eq!(
            bench_character_id(Some(&json!({ "characterId": "x", "other": 1 }))),
            None
        );
        assert_eq!(bench_character_id(Some(&json!({ "characterId": 5 }))), None);
        assert_eq!(bench_character_id(Some(&json!({ "mood": "grim" }))), None);
        assert_eq!(bench_character_id(Some(&json!(null))), None);
        assert_eq!(bench_character_id(Some(&json!(["x"]))), None);
        assert_eq!(bench_character_id(None), None);
    }
}
