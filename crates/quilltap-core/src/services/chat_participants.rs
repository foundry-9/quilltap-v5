//! The chat **cast** service layer (P4.9E1A) — a differential port of v4's
//! participant helpers in `app/api/v1/chats/[id]/helpers.ts`:
//! `resolveParticipantCharacterName` (:38), `getEnrichedCharacter` (:52),
//! `getEnrichedConnectionProfile` (:72), `enrichParticipant` (:90),
//! `handleParticipantUpdate` (:124), `handleAddParticipant` (:329),
//! `handleRemoveParticipant` (:420) and `recordStatusChangeEvent` (:614).
//!
//! This is the layer BETWEEN the already-ported repo ops
//! ([`crate::db::chats_participants`], proven by
//! `chats_participants_tier2_equivalence`) and the two v4 entrances that call
//! them — the `?action=*-participant` verbs ([`crate::api::chat_cast`]) and the
//! chat-PUT bag ([`crate::api::salon::chat_update`]). v4 shares one
//! implementation across both entrances, so v5 does too: everything here is
//! called from both.
//!
//! ## Three semantics a faithful port has to reproduce
//!
//! **1. `nullish` is three-valued.** `updateParticipantSchema` marks
//! `imageProfileId` / `selectedSystemPromptId` / `joinScenario` / `talkativeness`
//! `.nullish()`, and `helpers.ts` branches on `!== undefined`, so *absent* and
//! *explicit null* take different paths — an explicit `null` clears the override,
//! an absent key leaves it alone. [`ParticipantUpdateData`] models those as
//! `Option<Option<T>>`; [`ParticipantUpdateData::to_patch`] emits the key only
//! when the outer `Option` is `Some`, with an explicit JSON `null` inside.
//!
//! **2. `controlledBy` drives impersonation as a side effect** (`helpers.ts:180`).
//! Flipping to `'user'` appends the id to `impersonatingParticipantIds` and — only
//! when `activeTypingParticipantId` is currently falsy — sets that too. Flipping
//! to `'llm'` removes the id and — only when it *was* the active typist —
//! promotes `newImpersonating[0] || null`. v4 `bd419ae9` (bug 23) **DELETED** the
//! early `findById`-and-return that once followed this block, so a `controlledBy`
//! patch now falls through to the status-sync block, the `isActive` back-compat
//! block, and the identity-stack recompile tail — which is where v4's
//! `compileAllIdentityStacks(finalChat)` finally runs for it (step 9). v5 had
//! reproduced that early return deliberately as a v4 quirk; the quirk is now
//! fixed on both sides, so this port follows.
//!
//! **3. A connection-profile change posts a Prospero announcement**
//! (`helpers.ts:159`) — only when `connectionProfileId` is present **and** differs
//! from the old value **and** the participant has a `characterId`. Both labels are
//! resolved (the old may be `null` → the writer's `unassigned`).
//!
//! ## ⚠ A pre-existing gap this lane's differential is the first to reach
//!
//! v4's `ChatParticipantBaseSchema` marks `joinScenario`, `talkativeness` and
//! `roleplayTemplateId` `.nullable().optional()`, so a stored **explicit `null`**
//! survives its parse and `JSON.stringify`. v5's [`crate::db::chats::ChatParticipant`]
//! models all three as plain `Option<T>`, which collapses `null` and absent to the
//! same `None` and DROPS the key on re-serialization. `handleAddParticipant`
//! writes `joinScenario: data.joinScenario || null`, so **every** v4-added
//! participant carries `"joinScenario":null` where v5 omits the key.
//!
//! The fix is three field types in `db/chats.rs` — **P4.9E3A's file this round**
//! — so this lane does NOT make it (order §2: escalate, never resolve
//! unilaterally). `chat_cast_routes_equivalence` asserts the gap in both
//! directions with a tripwire that FAILS the moment it closes. It is a
//! stored-bytes fidelity gap, not a behavior change: every consumer treats an
//! absent key and a `null` identically.

use rusqlite::Connection;
use serde::Serialize;
use serde_json::{json, Map, Value};

use crate::clock::now_iso;
use crate::db::chats::{ChatUpdate, ChatsRepository};
use crate::db::chats_messages::{ChatEventInput, SystemEventInput};
use crate::db::runtime::Db;
use crate::db::{
    api_keys, characters_read, chats_read, connection_profiles, image_profiles, DbError,
};
use crate::photos::resolve_character_avatar::resolve_character_avatar;
use crate::services::chat_enrichment::EnrichedImage;
use crate::services::host_notifications::{
    post_host_remove_announcement, post_host_silent_mode_announcement,
    post_host_status_change_announcement, HostRemoveAnnouncement, HostSilentModeAnnouncement,
    HostStatusChangeAnnouncement,
};
use crate::services::outfit_selections::OutfitLlmChooseRunner;
use crate::services::prospero_notifications::{
    post_prospero_connection_profile_change_announcement,
    ProsperoConnectionProfileChangeAnnouncement,
};
use crate::services::system_prompt_compiler::compile_identity_stack_for_participant;

// ===========================================================================
// Errors — v4's `{ error, status }` result arm
// ===========================================================================

/// v4's `{ error: string; status: number }` failure shape, returned by the three
/// `handle*Participant` helpers and mapped to the response envelope by the
/// dispatch layer.
#[derive(Debug, Clone)]
pub struct ParticipantError {
    pub status: u16,
    pub message: String,
}

impl ParticipantError {
    fn new(status: u16, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for ParticipantError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl From<DbError> for ParticipantError {
    fn from(e: DbError) -> Self {
        // v4's repos throw → the route's serverError.
        ParticipantError::new(500, e.to_string())
    }
}

// ===========================================================================
// The validated field bags (v4 `updateParticipantSchema` / `addParticipantSchema`)
// ===========================================================================

/// `z.uuid()` — Zod 4 accepts any RFC 9562 text form; reproduced as the shape
/// test (the same idiom as [`crate::api::chat_post_office`]).
pub fn is_uuid(s: &str) -> bool {
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

/// v4 `ParticipantStatusEnum` as `updateParticipantSchema` spells it.
const STATUS_VALUES: [&str; 4] = ["active", "silent", "absent", "removed"];
/// v4 `ControlledByEnum`.
const CONTROLLED_BY_VALUES: [&str; 2] = ["llm", "user"];

/// v4 `updateParticipantSchema` (`schemas.ts:35`) after a successful parse. Each
/// `Option<Option<T>>` is a `.nullish()` field: `None` = key absent (leave alone),
/// `Some(None)` = explicit `null` (clear), `Some(Some(v))` = set.
#[derive(Debug, Clone, Default)]
pub struct ParticipantUpdateData {
    pub participant_id: String,
    /// `.optional()` (NOT nullish) — absent or a uuid.
    pub connection_profile_id: Option<String>,
    pub image_profile_id: Option<Option<String>>,
    pub selected_system_prompt_id: Option<Option<String>>,
    pub display_order: Option<i64>,
    pub is_active: Option<bool>,
    pub status: Option<String>,
    pub controlled_by: Option<String>,
    pub has_history_access: Option<bool>,
    pub join_scenario: Option<Option<String>>,
    pub talkativeness: Option<Option<f64>>,
}

/// A Zod rejection: v4 answers 400 `{error: 'Validation error', details: […]}`.
/// v5's envelope carries no `details` array (the standing, named P4.6bb
/// deferral), so only the message is reproduced.
pub const VALIDATION_ERROR: &str = "Validation error";

impl ParticipantUpdateData {
    /// The `updateParticipantSchema` floors v4's Zod enforces before any handler
    /// code runs. `Err` is the whole-parse rejection (v4's 400 `Validation error`).
    pub fn validate(&self) -> Result<(), ParticipantError> {
        let bad = || ParticipantError::new(400, VALIDATION_ERROR);
        if !is_uuid(&self.participant_id) {
            return Err(bad());
        }
        if let Some(id) = &self.connection_profile_id {
            if !is_uuid(id) {
                return Err(bad());
            }
        }
        for opt in [&self.image_profile_id, &self.selected_system_prompt_id] {
            if let Some(Some(id)) = opt {
                if !is_uuid(id) {
                    return Err(bad());
                }
            }
        }
        if let Some(s) = &self.status {
            if !STATUS_VALUES.contains(&s.as_str()) {
                return Err(bad());
            }
        }
        if let Some(c) = &self.controlled_by {
            if !CONTROLLED_BY_VALUES.contains(&c.as_str()) {
                return Err(bad());
            }
        }
        // `z.number().min(0.1).max(1.0).nullish()` — the bound applies only to a
        // present, non-null value.
        if let Some(Some(t)) = &self.talkativeness {
            if !(0.1..=1.0).contains(t) {
                return Err(bad());
            }
        }
        Ok(())
    }

    /// The `participantData` object v4 spreads into `repos.chats.updateParticipant`
    /// (`{ participantId, ...participantData } = data`): every PRESENT key, with an
    /// explicit `null` for a nullish key the caller cleared.
    pub fn to_patch(&self) -> Map<String, Value> {
        let mut m = Map::new();
        if let Some(v) = &self.connection_profile_id {
            m.insert("connectionProfileId".into(), json!(v));
        }
        if let Some(v) = &self.image_profile_id {
            m.insert("imageProfileId".into(), json!(v));
        }
        if let Some(v) = &self.selected_system_prompt_id {
            m.insert("selectedSystemPromptId".into(), json!(v));
        }
        if let Some(v) = self.display_order {
            m.insert("displayOrder".into(), json!(v));
        }
        if let Some(v) = self.is_active {
            m.insert("isActive".into(), json!(v));
        }
        if let Some(v) = &self.status {
            m.insert("status".into(), json!(v));
        }
        if let Some(v) = &self.controlled_by {
            m.insert("controlledBy".into(), json!(v));
        }
        if let Some(v) = self.has_history_access {
            m.insert("hasHistoryAccess".into(), json!(v));
        }
        if let Some(v) = &self.join_scenario {
            m.insert("joinScenario".into(), json!(v));
        }
        if let Some(v) = &self.talkativeness {
            m.insert("talkativeness".into(), json!(v));
        }
        m
    }
}

/// v4 `addParticipantSchema` (`schemas.ts:53`) after a successful parse. The
/// `type: z.literal('CHARACTER')` narrowing is carried by the verb name (see the
/// order's tier-2 item 6), so there is no `type` field here.
#[derive(Debug, Clone, Default)]
pub struct ParticipantAddData {
    pub character_id: String,
    pub connection_profile_id: Option<String>,
    pub image_profile_id: Option<Option<String>>,
    pub display_order: Option<i64>,
    pub has_history_access: Option<bool>,
    pub join_scenario: Option<Option<String>>,
    pub controlled_by: Option<String>,
    /// The `OutfitSelectionSchema` bag, shaped by
    /// [`crate::services::outfit_selections`].
    pub outfit_selection: Option<Value>,
}

impl ParticipantAddData {
    pub fn validate(&self) -> Result<(), ParticipantError> {
        let bad = || ParticipantError::new(400, VALIDATION_ERROR);
        if !is_uuid(&self.character_id) {
            return Err(bad());
        }
        if let Some(id) = &self.connection_profile_id {
            if !is_uuid(id) {
                return Err(bad());
            }
        }
        if let Some(Some(id)) = &self.image_profile_id {
            if !is_uuid(id) {
                return Err(bad());
            }
        }
        if let Some(c) = &self.controlled_by {
            if !CONTROLLED_BY_VALUES.contains(&c.as_str()) {
                return Err(bad());
            }
        }
        Ok(())
    }
}

// ===========================================================================
// Enrichment (v4 `getEnrichedCharacter` / `getEnrichedConnectionProfile` /
// `enrichParticipant`) — field order is v4's object-literal order, verbatim.
// ===========================================================================

/// v4 `EnrichedApiKey` (`lib/api/middleware/enrichment.ts:63`) —
/// `{ id, label, provider, isActive }`. NOTE this is a DIFFERENT shape from
/// [`crate::services::chat_enrichment::ApiKeySummary`] (`{id, provider, label}`),
/// which the single-chat GET path uses; the two are deliberately not unified.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnrichedApiKey {
    pub id: String,
    pub label: String,
    pub provider: String,
    pub is_active: bool,
}

/// v4 `getEnrichedCharacter` (`helpers.ts:52`).
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnrichedParticipantCharacter {
    pub id: String,
    pub name: String,
    pub title: Option<String>,
    /// `charData.talkativeness ?? 0.5`.
    pub talkativeness: f64,
    /// v4 copies `charData.defaultImageId` straight through, so an unset one is
    /// `undefined` and `JSON.stringify` DROPS the key (`title`, by contrast, is
    /// materialized as an explicit `null` by the vault overlay and stays).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_image_id: Option<String>,
    pub default_image: Option<EnrichedImage>,
    /// Presence-sensitive for the same reason as `default_image_id`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_connection_profile_id: Option<String>,
    /// `charData.archivedAt ?? null` — ALWAYS present, so the participant
    /// chips can badge archived seats (character-archive spec §5.2, v4
    /// `d553f72a`). Unlike the two fields above this is explicitly
    /// null-coalesced by v4, so the key survives `JSON.stringify`.
    pub archived_at: Option<String>,
}

/// v4 `getEnrichedConnectionProfile` (`helpers.ts:72`).
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnrichedParticipantConnectionProfile {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub model_name: String,
    /// v4 `bd419ae9` (bug 36): whether this profile permits tool use — surfaced so
    /// the tool-settings dialog can warn that per-chat tool toggles are moot when
    /// it's false. Between `modelName` and `apiKey` to match v4's key order.
    pub allow_tool_use: bool,
    pub api_key: Option<EnrichedApiKey>,
}

/// v4 `enrichParticipant` (`helpers.ts:90`). `joinScenario` is
/// presence-sensitive: v4 copies `participant.joinScenario` straight through, so
/// an `undefined` (a participant stored without the key) makes `JSON.stringify`
/// DROP it while a stored `null` renders as `null`. The double-`Option` + skip
/// reproduces all three shapes.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnrichedParticipant {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub controlled_by: String,
    pub character_id: Option<String>,
    pub display_order: i64,
    pub is_active: bool,
    pub status: String,
    pub removed_at: Option<String>,
    pub has_history_access: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub join_scenario: Option<Option<String>>,
    pub character: Option<EnrichedParticipantCharacter>,
    pub connection_profile: Option<EnrichedParticipantConnectionProfile>,
    pub created_at: String,
    pub updated_at: String,
}

fn s(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(Value::as_str).map(str::to_string)
}

/// v4 `enrichWithDefaultImage(imageId, repos)` — `resolveCharacterAvatar` →
/// `{id, filepath: resolved.url, url: null}`, or `null`.
fn enrich_with_default_image(
    main: &Connection,
    mount: &Connection,
    image_id: Option<&str>,
) -> Result<Option<EnrichedImage>, DbError> {
    let Some(resolved) = resolve_character_avatar(main, mount, image_id)? else {
        return Ok(None);
    };
    Ok(Some(EnrichedImage {
        id: resolved.id,
        filepath: resolved.url,
        url: None,
    }))
}

/// v4 `enrichWithApiKey(apiKeyId, repos)` — `{id, label, provider, isActive}`.
fn enrich_with_api_key(
    main: &Connection,
    api_key_id: Option<&str>,
) -> Result<Option<EnrichedApiKey>, DbError> {
    let Some(id) = api_key_id else {
        return Ok(None);
    };
    let Some(key) = api_keys::find_by_id(main, id)? else {
        return Ok(None);
    };
    Ok(Some(EnrichedApiKey {
        id: key.id,
        label: key.label,
        provider: key.provider,
        is_active: key.is_active,
    }))
}

/// v4 `getEnrichedCharacter` — the overlaid character + its default image.
pub fn get_enriched_character(
    main: &Connection,
    mount: &Connection,
    character_id: &str,
) -> Result<Option<EnrichedParticipantCharacter>, DbError> {
    let Some(c) = characters_read::find_by_id(main, mount, character_id)? else {
        return Ok(None);
    };
    let default_image_id = s(&c, "defaultImageId");
    let default_image = enrich_with_default_image(main, mount, default_image_id.as_deref())?;
    Ok(Some(EnrichedParticipantCharacter {
        id: s(&c, "id").unwrap_or_default(),
        name: s(&c, "name").unwrap_or_default(),
        title: s(&c, "title"),
        talkativeness: c
            .get("talkativeness")
            .and_then(Value::as_f64)
            .unwrap_or(0.5),
        default_image_id,
        default_image,
        default_connection_profile_id: s(&c, "defaultConnectionProfileId"),
        archived_at: s(&c, "archivedAt"),
    }))
}

/// v4 `getEnrichedConnectionProfile`.
pub fn get_enriched_connection_profile(
    main: &Connection,
    profile_id: &str,
) -> Result<Option<EnrichedParticipantConnectionProfile>, DbError> {
    let Some(p) = connection_profiles::find_by_id(main, profile_id)? else {
        return Ok(None);
    };
    let api_key = enrich_with_api_key(main, s(&p, "apiKeyId").as_deref())?;
    Ok(Some(EnrichedParticipantConnectionProfile {
        id: s(&p, "id").unwrap_or_default(),
        name: s(&p, "name").unwrap_or_default(),
        provider: s(&p, "provider").unwrap_or_default(),
        model_name: s(&p, "modelName").unwrap_or_default(),
        // v4 `profile.allowToolUse ?? true` — `find_by_id` renders the column as a
        // bool (NOT NULL, default 1), so an absent value can only mean the true default.
        allow_tool_use: p
            .get("allowToolUse")
            .and_then(Value::as_bool)
            .unwrap_or(true),
        api_key,
    }))
}

/// v4 `enrichParticipant`. v4 throws for a non-`CHARACTER` participant; today
/// `CHARACTER` is the only type, so the guard is carried as a plain error.
pub fn enrich_participant(
    main: &Connection,
    mount: &Connection,
    participant: &Value,
) -> Result<EnrichedParticipant, DbError> {
    let kind = s(participant, "type").unwrap_or_default();
    if kind != "CHARACTER" {
        return Err(DbError::Key(
            "Only CHARACTER participants are supported".to_string(),
        ));
    }
    let character_id = s(participant, "characterId");
    let character = match character_id.as_deref() {
        Some(cid) => get_enriched_character(main, mount, cid)?,
        None => None,
    };
    let connection_profile = match s(participant, "connectionProfileId") {
        Some(pid) => get_enriched_connection_profile(main, &pid)?,
        None => None,
    };
    Ok(EnrichedParticipant {
        id: s(participant, "id").unwrap_or_default(),
        kind,
        controlled_by: s(participant, "controlledBy")
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| "llm".to_string()),
        character_id,
        display_order: participant
            .get("displayOrder")
            .and_then(Value::as_i64)
            .unwrap_or(0),
        is_active: participant
            .get("isActive")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        status: s(participant, "status")
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| "active".to_string()),
        removed_at: s(participant, "removedAt"),
        has_history_access: participant
            .get("hasHistoryAccess")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        join_scenario: participant
            .get("joinScenario")
            .map(|v| v.as_str().map(str::to_string)),
        character,
        connection_profile,
        created_at: s(participant, "createdAt").unwrap_or_default(),
        updated_at: s(participant, "updatedAt").unwrap_or_default(),
    })
}

/// v4 `resolveParticipantCharacterName` (`helpers.ts:38`) — the participant's
/// character name, `'Unknown'` when there is no character or the read misses.
pub fn resolve_participant_character_name(
    main: &Connection,
    mount: &Connection,
    participant: Option<&Value>,
) -> Result<String, DbError> {
    if let Some(cid) = participant.and_then(|p| s(p, "characterId")) {
        if let Some(c) = characters_read::find_by_id(main, mount, &cid)? {
            if let Some(name) = s(&c, "name") {
                return Ok(name);
            }
        }
    }
    Ok("Unknown".to_string())
}

// ===========================================================================
// Small read/write helpers over the single-writer handle
// ===========================================================================

fn find_chat(db: &Db, chat_id: &str) -> Result<Option<Value>, DbError> {
    let cid = chat_id.to_string();
    db.read_main(move |c| chats_read::find_by_id(c, &cid))
}

fn participant_of<'a>(chat: &'a Value, participant_id: &str) -> Option<&'a Value> {
    chat.get("participants")
        .and_then(Value::as_array)?
        .iter()
        .find(|p| p.get("id").and_then(Value::as_str) == Some(participant_id))
}

/// v4 `isParticipantPresent(status)` — `active` or `silent`.
pub fn is_present(status: &str) -> bool {
    matches!(status, "active" | "silent")
}

/// `repos.chats.updateParticipant(chatId, participantId, data)` on the writer
/// thread. Returns whether the participant was found.
async fn write_update_participant(
    db: &Db,
    chat_id: &str,
    participant_id: &str,
    data: Map<String, Value>,
) -> Result<bool, DbError> {
    let (cid, pid) = (chat_id.to_string(), participant_id.to_string());
    db.write(move |w| {
        w.main()
            .chat_participants()
            .update_participant(&cid, &pid, &Value::Object(data))
    })
    .await
}

/// `repos.chats.update(chatId, patch)` on the writer thread.
async fn write_chat(db: &Db, chat_id: &str, patch: ChatUpdate) -> Result<bool, DbError> {
    let cid = chat_id.to_string();
    db.write(move |w| ChatsRepository::new(w.main().connection()).update(&cid, &patch))
        .await
}

/// Run a read that needs BOTH the main and mount-index connections (the
/// character-vault overlay).
fn read_main_mount<T, F>(db: &Db, f: F) -> Result<T, DbError>
where
    F: FnOnce(&Connection, &Connection) -> Result<T, DbError>,
{
    db.read_main(|main| db.read_mount_index(|mount| f(main, mount)))
}

/// `compileIdentityStackForParticipant(chat, participantId)` on the writer
/// thread. Only `rebuild-system-prompt` inspects the result (v4 turns a throw
/// into its 500); every other call site wraps it in a try/catch that just warns
/// — use [`compile_stack_best_effort`] there.
pub async fn compile_stack(db: &Db, chat: Value, participant_id: &str) -> Result<(), DbError> {
    let pid = participant_id.to_string();
    db.write(move |w| {
        let mount =
            w.mount_index()
                .map(|m| m.connection())
                .ok_or(DbError::PartitionUnavailable(
                    crate::write_partition::WriteDbTarget::MountIndex,
                ))?;
        let main = w.main().connection();
        compile_identity_stack_for_participant(main, mount, &chat, &pid)
    })
    .await
}

/// [`compile_stack`] with v4's swallow-and-warn wrapper.
pub async fn compile_stack_best_effort(db: &Db, chat: Value, participant_id: &str) {
    if let Err(e) = compile_stack(db, chat, participant_id).await {
        tracing::warn!(
            participant_id, error = %e,
            "[Chats v1] Failed to compile identity stack for participant"
        );
    }
}

/// `compileAllIdentityStacks(chat)` on the writer thread — recompile EVERY
/// participant's identity stack and persist the map. v4 runs this after a
/// `controlledBy` flip (a new user-controlled participant changes
/// `{{user}}/{{persona}}` for everyone), from both the participant-update tail
/// (`helpers.ts`) and the impersonate/stop-impersonate actions
/// (`actions/participants.ts`).
pub async fn compile_all_stacks(db: &Db, chat: Value) -> Result<(), DbError> {
    db.write(move |w| {
        let mount =
            w.mount_index()
                .map(|m| m.connection())
                .ok_or(DbError::PartitionUnavailable(
                    crate::write_partition::WriteDbTarget::MountIndex,
                ))?;
        let main = w.main().connection();
        crate::services::system_prompt_compiler::compile_all_identity_stacks(main, mount, &chat)
    })
    .await
}

/// [`compile_all_stacks`] with v4's swallow-and-warn wrapper (every call site
/// wraps `compileAllIdentityStacks` in a try/catch that only warns).
pub async fn compile_all_stacks_best_effort(db: &Db, chat: Value) {
    if let Err(e) = compile_all_stacks(db, chat).await {
        tracing::warn!(
            error = %e,
            "[Chats v1] Failed to recompile identity stacks"
        );
    }
}

// ===========================================================================
// `recordStatusChangeEvent` (v4 `helpers.ts:614`)
// ===========================================================================

/// v4's `statusLabels` map, verbatim.
fn status_label(status: &str) -> &str {
    match status {
        "active" => "active (speaking normally)",
        "silent" => "silent (observing, not speaking)",
        "absent" => "absent (away from the scene)",
        "removed" => "removed (left the conversation)",
        other => other,
    }
}

/// v4 `recordStatusChangeEvent` — a `STATUS_CHANGE` system event so the other
/// characters see the change in their next prompt. The write error is swallowed
/// (v4's try/catch logs and moves on).
pub async fn record_status_change_event(
    db: &Db,
    chat_id: &str,
    character_name: &str,
    old_status: &str,
    new_status: &str,
) {
    let description = format!(
        "{character_name} changed from {} to {}",
        status_label(old_status),
        status_label(new_status)
    );
    let event = ChatEventInput::System(SystemEventInput {
        id: uuid::Uuid::new_v4().to_string(),
        system_event_type: "STATUS_CHANGE".to_string(),
        description,
        prompt_tokens: None,
        completion_tokens: None,
        total_tokens: None,
        provider: None,
        model_name: None,
        estimated_cost_usd: None,
        created_at: now_iso(),
    });
    let cid = chat_id.to_string();
    let _ = db
        .write(move |w| w.main().chat_messages().add_message(&cid, &event))
        .await;
}

// ===========================================================================
// `handleParticipantUpdate` (v4 `helpers.ts:124`)
// ===========================================================================

/// v4 `handleParticipantUpdate` — the shared update spine for BOTH entrances
/// (`?action=update-participant` and the chat-PUT `updateParticipant` bag).
/// Returns the chat v4 returns (`finalChat || result`; see the module header for
/// the `controlledBy` side effects, and bug 23's removed early return).
pub async fn handle_participant_update(
    db: &Db,
    chat_id: &str,
    data: &ParticipantUpdateData,
) -> Result<Value, ParticipantError> {
    let participant_id = data.participant_id.as_str();

    // 1-2. Referenced-profile existence gates. v4 tests TRUTHINESS
    // (`if (participantData.connectionProfileId)`), so an explicit
    // `imageProfileId: null` skips the image-profile gate entirely.
    if let Some(id) = &data.connection_profile_id {
        let id = id.clone();
        if db
            .read_main(move |c| connection_profiles::find_by_id(c, &id))?
            .is_none()
        {
            return Err(ParticipantError::new(404, "Connection profile not found"));
        }
    }
    if let Some(Some(id)) = &data.image_profile_id {
        let id = id.clone();
        if db
            .read_main(move |c| image_profiles::find_by_id(c, &id))?
            .is_none()
        {
            return Err(ParticipantError::new(404, "Image profile not found"));
        }
    }

    // 3. The chat.
    let Some(chat) = find_chat(db, chat_id)? else {
        return Err(ParticipantError::new(404, "Chat not found"));
    };
    let old_participant = participant_of(&chat, participant_id).cloned();
    let old_connection_profile_id = old_participant
        .as_ref()
        .and_then(|p| s(p, "connectionProfileId"));
    let old_selected_prompt_id = old_participant
        .as_ref()
        .and_then(|p| p.get("selectedSystemPromptId").cloned());

    // 4. The patch itself.
    if !write_update_participant(db, chat_id, participant_id, data.to_patch()).await? {
        return Err(ParticipantError::new(404, "Participant not found"));
    }
    let result_chat = find_chat(db, chat_id)?.ok_or_else(|| {
        ParticipantError::new(500, "chat vanished between updateParticipant and re-read")
    })?;

    // 5. The Prospero connection-profile announcement.
    if let Some(new_profile_id) = &data.connection_profile_id {
        let changed = Some(new_profile_id.as_str()) != old_connection_profile_id.as_deref();
        let character_id = old_participant.as_ref().and_then(|p| s(p, "characterId"));
        if changed {
            if let Some(character_id) = character_id {
                let cid = character_id.clone();
                let character = read_main_mount(db, |main, mount| {
                    characters_read::find_by_id(main, mount, &cid)
                })?;
                if let Some(character) = character {
                    let old_label = match &old_connection_profile_id {
                        Some(old_id) => {
                            let old_id = old_id.clone();
                            db.read_main(move |c| connection_profiles::find_by_id(c, &old_id))?
                                .and_then(|p| s(&p, "name"))
                        }
                        None => None,
                    };
                    let new_id = new_profile_id.clone();
                    let new_label = db
                        .read_main(move |c| connection_profiles::find_by_id(c, &new_id))?
                        .and_then(|p| s(&p, "name"));
                    post_prospero_connection_profile_change_announcement(
                        db,
                        ProsperoConnectionProfileChangeAnnouncement {
                            chat_id: chat_id.to_string(),
                            character_name: s(&character, "name").unwrap_or_default(),
                            old_profile_label: old_label,
                            new_profile_label: new_label,
                        },
                    )
                    .await;
                }
            }
        }
    }

    // 6. The `controlledBy` impersonation sync. v4 `bd419ae9` (bug 23) removed the
    //    early return that once followed this block; it now falls through to the
    //    status/`isActive` sync and the whole-chat recompile tail (step 9).
    if let Some(controlled_by) = &data.controlled_by {
        let current: Vec<String> = chat
            .get("impersonatingParticipantIds")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        let is_currently = current.iter().any(|id| id == participant_id);
        let active_typing = s(&result_chat, "activeTypingParticipantId");

        if controlled_by == "user" && !is_currently {
            let mut next = current.clone();
            next.push(participant_id.to_string());
            let patch = ChatUpdate {
                impersonating_participant_ids: Some(next),
                active_typing_participant_id: match active_typing {
                    Some(_) => None,
                    None => Some(Some(participant_id.to_string())),
                },
                ..Default::default()
            };
            write_chat(db, chat_id, patch).await?;
        } else if controlled_by == "llm" && is_currently {
            let next: Vec<String> = current
                .iter()
                .filter(|id| id.as_str() != participant_id)
                .cloned()
                .collect();
            let promote = active_typing.as_deref() == Some(participant_id);
            let patch = ChatUpdate {
                // v4 sets the key only when the leaver WAS the active typist,
                // and then to `newImpersonating[0] || null`.
                active_typing_participant_id: promote.then(|| next.first().cloned()),
                impersonating_participant_ids: Some(next),
                ..Default::default()
            };
            write_chat(db, chat_id, patch).await?;
        }
        // v4 `bd419ae9` (bug 23) DELETED the early `findById`-and-return here: a
        // `controlledBy` patch now FALLS THROUGH to the status/`isActive`
        // back-compat sync and the identity-stack recompile tail below, so the
        // `compileAllIdentityStacks` call (step 9) is reached for it too.
    }

    // 7. Explicit `status`: sync `isActive`/`removedAt` and announce.
    //    v4 tests TRUTHINESS, so this never runs for an (impossible) empty string.
    if let Some(new_status) = data.status.as_deref().filter(|s| !s.is_empty()) {
        // v4 reads the PRE-update participant here (`chat`, not the result).
        let participant = participant_of(&chat, participant_id).cloned();
        let old_status = participant
            .as_ref()
            .map(|p| {
                s(p, "status").unwrap_or_else(|| {
                    if p.get("isActive").and_then(Value::as_bool) == Some(true) {
                        "active".to_string()
                    } else if p.get("removedAt").and_then(Value::as_str).is_some() {
                        "removed".to_string()
                    } else {
                        "absent".to_string()
                    }
                })
            })
            .unwrap_or_else(|| "absent".to_string());

        let mut sync = Map::new();
        sync.insert("isActive".into(), json!(is_present(new_status)));
        sync.insert(
            "removedAt".into(),
            if new_status == "removed" {
                json!(now_iso())
            } else {
                Value::Null
            },
        );
        write_update_participant(db, chat_id, participant_id, sync).await?;

        if old_status != new_status {
            if let Some(character_id) = participant.as_ref().and_then(|p| s(p, "characterId")) {
                let cid = character_id.clone();
                let character = read_main_mount(db, |main, mount| {
                    characters_read::find_by_id(main, mount, &cid)
                })?;
                if let Some(character) = character {
                    let character_name = s(&character, "name").unwrap_or_default();
                    record_status_change_event(
                        db,
                        chat_id,
                        &character_name,
                        &old_status,
                        new_status,
                    )
                    .await;
                    if new_status == "removed" {
                        post_host_remove_announcement(
                            db,
                            HostRemoveAnnouncement {
                                chat_id: chat_id.to_string(),
                                character_name: character_name.clone(),
                                participant_id: participant_id.to_string(),
                            },
                        )
                        .await;
                    } else if matches!(old_status.as_str(), "active" | "silent" | "absent")
                        && matches!(new_status, "active" | "silent" | "absent")
                    {
                        post_host_status_change_announcement(
                            db,
                            HostStatusChangeAnnouncement {
                                chat_id: chat_id.to_string(),
                                character_name: character_name.clone(),
                                participant_id: participant_id.to_string(),
                                old_status: old_status.clone(),
                                new_status: new_status.to_string(),
                            },
                        )
                        .await;
                        if new_status == "silent" && old_status != "silent" {
                            post_host_silent_mode_announcement(
                                db,
                                HostSilentModeAnnouncement {
                                    chat_id: chat_id.to_string(),
                                    character_name: character_name.clone(),
                                    target_participant_id: participant_id.to_string(),
                                    transition: "enter".to_string(),
                                },
                            )
                            .await;
                        } else if old_status == "silent" && new_status != "silent" {
                            post_host_silent_mode_announcement(
                                db,
                                HostSilentModeAnnouncement {
                                    chat_id: chat_id.to_string(),
                                    character_name,
                                    target_participant_id: participant_id.to_string(),
                                    transition: "exit".to_string(),
                                },
                            )
                            .await;
                        }
                    }
                }
            }
        }
    }

    // 8. Back-compat: `isActive` without `status` derives one.
    if let Some(is_active) = data.is_active {
        if data.status.as_deref().filter(|s| !s.is_empty()).is_none() {
            let participant = participant_of(&chat, participant_id).cloned();
            let old_is_active = participant
                .as_ref()
                .and_then(|p| p.get("isActive").and_then(Value::as_bool));
            let new_status = if is_active { "active" } else { "absent" };
            let mut sync = Map::new();
            sync.insert("status".into(), json!(new_status));
            write_update_participant(db, chat_id, participant_id, sync).await?;

            if old_is_active != Some(is_active) {
                if let Some(character_id) = participant.as_ref().and_then(|p| s(p, "characterId")) {
                    let old_status = if old_is_active == Some(true) {
                        "active"
                    } else {
                        "absent"
                    };
                    let cid = character_id.clone();
                    let character = read_main_mount(db, |main, mount| {
                        characters_read::find_by_id(main, mount, &cid)
                    })?;
                    if let Some(character) = character {
                        let character_name = s(&character, "name").unwrap_or_default();
                        record_status_change_event(
                            db,
                            chat_id,
                            &character_name,
                            old_status,
                            new_status,
                        )
                        .await;
                        post_host_status_change_announcement(
                            db,
                            HostStatusChangeAnnouncement {
                                chat_id: chat_id.to_string(),
                                character_name,
                                participant_id: participant_id.to_string(),
                                old_status: old_status.to_string(),
                                new_status: new_status.to_string(),
                            },
                        )
                        .await;
                    }
                }
            }
        }
    }

    // 9. Re-read + the identity-stack invalidation hooks. v4 wraps BOTH recompiles
    //    in ONE try/catch that only warns; each best-effort helper swallows here.
    //    For a `controlledBy`-only patch (no `selectedSystemPromptId`) only the
    //    whole-chat recompile runs; when both fire, `compileAllIdentityStacks`
    //    rewrites the entire map last, so the end state matches v4's ordering.
    let final_chat = find_chat(db, chat_id)?;
    if let Some(final_chat) = &final_chat {
        if let Some(new_prompt_id) = &data.selected_system_prompt_id {
            let new_value = match new_prompt_id {
                Some(v) => Value::String(v.clone()),
                None => Value::Null,
            };
            // v4 compares against the RAW old value: an absent key is `undefined`,
            // which `!==` any string AND `!== null`.
            let old_value = old_selected_prompt_id.clone().unwrap_or(Value::Null);
            let old_absent = old_selected_prompt_id.is_none();
            if old_absent || old_value != new_value {
                compile_stack_best_effort(db, final_chat.clone(), participant_id).await;
            }
        }
        // v4 `bd419ae9` (bug 23): a `controlledBy` change alters
        // `{{user}}/{{persona}}` for everyone, so recompile ALL stacks. Now
        // REACHABLE — the step-6 early return is gone.
        if data.controlled_by.is_some() {
            compile_all_stacks_best_effort(db, final_chat.clone()).await;
        }
    }

    Ok(final_chat.unwrap_or(result_chat))
}

// ===========================================================================
// `handleAddParticipant` (v4 `helpers.ts:329`)
// ===========================================================================

/// v4 `resolveFallbackConnectionProfile` (`helpers.ts:315`) — the user's marked
/// default, else their first profile, else `None`.
fn resolve_fallback_connection_profile(
    main: &Connection,
    user_id: &str,
) -> Result<Option<Value>, DbError> {
    if let Some(d) = connection_profiles::find_default(main, user_id)? {
        return Ok(Some(d));
    }
    Ok(connection_profiles::find_by_user_id(main, user_id)?
        .into_iter()
        .next())
}

/// v4 `handleAddParticipant` — the shared add spine for BOTH entrances. Returns
/// the chat v4 returns (the `addParticipant` result, or the tag-merge update's
/// result when the character contributed new tags).
pub async fn handle_add_participant(
    db: &Db,
    chat_id: &str,
    data: &ParticipantAddData,
    current_participant_count: i64,
    user_id: &str,
) -> Result<Value, ParticipantError> {
    // v4's `if (!data.characterId)` arm. The typed bag makes the id required at
    // the boundary, so only an empty string can reach it.
    if data.character_id.is_empty() {
        return Err(ParticipantError::new(
            400,
            "characterId is required for CHARACTER participants",
        ));
    }
    let cid = data.character_id.clone();
    let Some(character) = read_main_mount(db, |main, mount| {
        characters_read::find_by_id(main, mount, &cid)
    })?
    else {
        return Err(ParticipantError::new(404, "Character not found"));
    };

    let controlled_by = data
        .controlled_by
        .clone()
        .filter(|v| !v.is_empty())
        .or_else(|| s(&character, "controlledBy").filter(|v| !v.is_empty()))
        .unwrap_or_else(|| "llm".to_string());
    let is_user_controlled = controlled_by == "user";

    let mut resolved_profile: Option<String> = data.connection_profile_id.clone();

    if !is_user_controlled {
        match resolved_profile.clone() {
            None => {
                let uid = user_id.to_string();
                let fallback =
                    db.read_main(move |c| resolve_fallback_connection_profile(c, &uid))?;
                let Some(fallback) = fallback else {
                    return Err(ParticipantError::new(
                        400,
                        "connectionProfileId is required for LLM-controlled CHARACTER participants",
                    ));
                };
                resolved_profile = s(&fallback, "id");
            }
            Some(requested) => {
                let req = requested.clone();
                if db
                    .read_main(move |c| connection_profiles::find_by_id(c, &req))?
                    .is_none()
                {
                    let uid = user_id.to_string();
                    let fallback =
                        db.read_main(move |c| resolve_fallback_connection_profile(c, &uid))?;
                    let Some(fallback) = fallback else {
                        return Err(ParticipantError::new(
                            404,
                            "Connection profile not found and no fallback profile available",
                        ));
                    };
                    resolved_profile = s(&fallback, "id");
                }
            }
        }
    } else if let Some(requested) = resolved_profile.clone() {
        if db
            .read_main(move |c| connection_profiles::find_by_id(c, &requested))?
            .is_none()
        {
            resolved_profile = None;
        }
    }

    // v4's participant literal, key for key. `|| null` is FALSY coalescing: an
    // absent key, an explicit null and an empty string all become `null`.
    let participant = json!({
        "type": "CHARACTER",
        "characterId": data.character_id,
        "controlledBy": controlled_by,
        "connectionProfileId": resolved_profile,
        "imageProfileId": data.image_profile_id.clone().flatten().filter(|v| !v.is_empty()),
        "displayOrder": data.display_order.unwrap_or(current_participant_count),
        "isActive": true,
        "status": "active",
        "hasHistoryAccess": data.has_history_access.unwrap_or(false),
        "joinScenario": data.join_scenario.clone().flatten().filter(|v| !v.is_empty()),
    });

    let cid = chat_id.to_string();
    let added = db
        .write(move |w| {
            w.main()
                .chat_participants()
                .add_participant(&cid, &participant)
        })
        .await?;
    if !added {
        return Err(ParticipantError::new(500, "Failed to add participant"));
    }
    let mut result = find_chat(db, chat_id)?
        .ok_or_else(|| ParticipantError::new(500, "Failed to add participant"))?;

    // The character's tags are merged onto the chat.
    let character_tags: Vec<String> = character
        .get("tags")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    if !character_tags.is_empty() {
        let existing: Vec<String> = result
            .get("tags")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        let new_tags: Vec<String> = character_tags
            .into_iter()
            .filter(|t| !existing.contains(t))
            .collect();
        if !new_tags.is_empty() {
            let mut merged = existing;
            merged.extend(new_tags);
            let patch = ChatUpdate {
                tags: Some(merged),
                ..Default::default()
            };
            if write_chat(db, chat_id, patch).await? {
                if let Some(updated) = find_chat(db, chat_id)? {
                    result = updated;
                }
            }
        }
    }

    Ok(result)
}

// ===========================================================================
// `handleRemoveParticipant` (v4 `helpers.ts:420`)
// ===========================================================================

/// v4 `handleRemoveParticipant` — the soft-delete, with the repo's
/// last-participant throw mapped to v4's 400.
pub async fn handle_remove_participant(
    db: &Db,
    chat_id: &str,
    participant_id: &str,
) -> Result<Value, ParticipantError> {
    let (cid, pid) = (chat_id.to_string(), participant_id.to_string());
    let removed = db
        .write(
            move |w| match w.main().chat_participants().remove_participant(&cid, &pid) {
                Ok(found) => Ok(Some(found)),
                Err(crate::db::chats_participants::ParticipantOpError::LastParticipant) => Ok(None),
                Err(crate::db::chats_participants::ParticipantOpError::Db(e)) => Err(e),
            },
        )
        .await?;
    match removed {
        None => Err(ParticipantError::new(
            400,
            "Cannot remove the last participant from a chat",
        )),
        Some(false) => Err(ParticipantError::new(404, "Participant not found")),
        Some(true) => find_chat(db, chat_id)?
            .ok_or_else(|| ParticipantError::new(404, "Participant not found")),
    }
}

// ===========================================================================
// `applyOutfitForAddedParticipant` (v4 `actions/participants.ts:169`)
// ===========================================================================

/// v4's equipped-outfit shape (the same object `services::outfit_selections`
/// writes) — the canonical serialization, hair key included.
fn slots_value(slots: &crate::wardrobe::Slots) -> Value {
    slots.to_value()
}

fn slots_from_value(v: &Value) -> crate::wardrobe::Slots {
    crate::wardrobe::Slots::from_value(Some(v))
}

/// v4 `applyOutfitForAddedParticipant` — dress a freshly-added (or reactivated)
/// participant. When the caller sent no `outfitSelection`, v4 defaults to
/// `{characterId, mode: 'default'}` so the character arrives in their wardrobe
/// defaults rather than undressed by accident. Failures are logged and swallowed:
/// dressing must never block someone from joining the chat.
///
/// v4 routes through `applyOutfitSelections`, whose five modes this reproduces
/// over the SAME ported leaves ([`resolve_default_outfit`] +
/// `set_equipped_outfit`). `previous_chat` degenerates to `default` here because
/// this call site passes no `sourceChatId` (v4 `participants.ts:187` builds the
/// context without one), which is v4's own fallback.
///
/// ## `llm_choose` (P4.9E3B — the refusal is GONE)
///
/// The cheap-LLM pick rides the `runner` host seam
/// ([`crate::services::outfit_selections::OutfitLlmChooseRunner`] — the
/// `RegenerateTitleDriver` arrangement). v4's shape on ANY failure — no
/// provider, task failure, thrown read — is fall back to the DEFAULT outfit
/// and never block the join; an unwired runner takes the same path (with a
/// named warning), exactly like v4's no-resolvable-profile arm. The context
/// mirrors v4 `participants.ts:186–189`: `scenarioText` from the chat,
/// `cheapLLMConfig` from the user's chat settings, NO `sourceChatId`.
pub async fn apply_outfit_for_added_participant(
    db: &Db,
    user_id: &str,
    chat_id: &str,
    character_id: &str,
    outfit_selection: Option<&Value>,
    runner: Option<&std::sync::Arc<dyn OutfitLlmChooseRunner>>,
) {
    let mode = outfit_selection
        .and_then(|s| s.get("mode"))
        .and_then(Value::as_str)
        .unwrap_or("default")
        .to_string();
    let manual_slots = outfit_selection
        .and_then(|s| s.get("slots"))
        .filter(|v| v.is_object())
        .map(slots_from_value);

    // Project wardrobe tier for this chat, so a joining character can be dressed
    // from the project's shared stores as well as their own vault (v4
    // `participants.ts` threads `resolveProjectMountPointIds(chat.projectId)`).
    let project_mount_point_ids = {
        let cid = chat_id.to_string();
        read_main_mount(db, |main, mount| {
            Ok(
                crate::tools::wardrobe_shared::resolve_project_mount_point_ids_for_chat(
                    main, mount, &cid,
                ),
            )
        })
        .unwrap_or_default()
    };

    let slots = match mode.as_str() {
        // No `sourceChatId` at this call site, so v4's `previous_chat` falls
        // straight through to the default wardrobe.
        "default" | "previous_chat" => {
            let cid = character_id.to_string();
            let mounts = project_mount_point_ids.clone();
            match read_main_mount(db, |main, mount| {
                crate::services::outfit_selections::resolve_default_outfit(
                    main, mount, &cid, &mounts,
                )
            }) {
                Ok(s) => s,
                Err(e) => {
                    tracing::warn!(
                        chat_id, character_id, mode = %mode, error = %e,
                        "[Chats v1] Failed to apply outfit for added participant"
                    );
                    return;
                }
            }
        }
        "manual" => manual_slots.unwrap_or_default(),
        "none" => crate::wardrobe::Slots::default(),
        "llm_choose" => {
            let chosen = match runner {
                Some(runner) => {
                    // v4's context: the chat's scenarioText + the user's
                    // cheapLLMSettings (both best-effort reads).
                    let cid = chat_id.to_string();
                    let uid = user_id.to_string();
                    let context = db.read_main(move |c| {
                        let chat = crate::db::chats_read::find_by_id(c, &cid)?;
                        let scenario = chat
                            .as_ref()
                            .and_then(|ch| ch.get("scenarioText"))
                            .and_then(Value::as_str)
                            .map(str::to_string);
                        let settings = crate::db::chat_settings::find_by_user_id(c, &uid)?;
                        let cheap = settings
                            .as_ref()
                            .and_then(|s| s.get("cheapLLMSettings"))
                            .filter(|v| !v.is_null())
                            .cloned();
                        Ok((scenario, cheap))
                    });
                    let (scenario_text, cheap_settings) = context.unwrap_or((None, None));
                    runner
                        .choose(crate::services::outfit_selections::OutfitLlmChooseRequest {
                            chat_id: chat_id.to_string(),
                            character_id: character_id.to_string(),
                            scenario_text,
                            cheap_settings,
                            project_mount_point_ids: project_mount_point_ids.clone(),
                        })
                        .await
                }
                None => {
                    tracing::warn!(
                        chat_id,
                        character_id,
                        "[Chats v1] llm_choose outfit runner is not assembled — \
                         falling back to the default outfit (v4's own failure shape)"
                    );
                    None
                }
            };
            match chosen {
                Some(slots) => slots,
                // v4: any failure → resolveDefaultOutfit.
                None => {
                    let cid = character_id.to_string();
                    let mounts = project_mount_point_ids.clone();
                    match read_main_mount(db, |main, mount| {
                        crate::services::outfit_selections::resolve_default_outfit(
                            main, mount, &cid, &mounts,
                        )
                    }) {
                        Ok(s) => s,
                        Err(e) => {
                            tracing::warn!(
                                chat_id, character_id, error = %e,
                                "[Chats v1] Failed to apply outfit for added participant"
                            );
                            return;
                        }
                    }
                }
            }
        }
        // v4 logs and skips an unknown mode (no write).
        _ => return,
    };

    let (cid, chid, value) = (
        chat_id.to_string(),
        character_id.to_string(),
        slots_value(&slots),
    );
    let _ = db
        .write(move |w| {
            crate::db::chats_outfits::ChatOutfitsRepository::new(w.main().connection())
                .set_equipped_outfit(&cid, &chid, &value)
        })
        .await;
}

// ===========================================================================
// The chat-PUT bag coercion (v4 `chatUpdateRequestSchema.updateParticipant` /
// `.addParticipant` — the SAME Zod schemas the `?action=` verbs parse)
// ===========================================================================

fn bad() -> ParticipantError {
    ParticipantError::new(400, VALIDATION_ERROR)
}

/// `z.string().optional()` — absent → `None`; a string → `Some`; anything else
/// fails the whole parse.
fn opt_str(obj: &Map<String, Value>, key: &str) -> Result<Option<String>, ParticipantError> {
    match obj.get(key) {
        None => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.clone())),
        Some(_) => Err(bad()),
    }
}

/// `.nullish()` — absent → `None`; `null` → `Some(None)`; a string →
/// `Some(Some(_))`. The three-valued shape §2 point 1 depends on.
fn nullish_str(
    obj: &Map<String, Value>,
    key: &str,
) -> Result<Option<Option<String>>, ParticipantError> {
    match obj.get(key) {
        None => Ok(None),
        Some(Value::Null) => Ok(Some(None)),
        Some(Value::String(s)) => Ok(Some(Some(s.clone()))),
        Some(_) => Err(bad()),
    }
}

fn opt_bool(obj: &Map<String, Value>, key: &str) -> Result<Option<bool>, ParticipantError> {
    match obj.get(key) {
        None => Ok(None),
        Some(Value::Bool(b)) => Ok(Some(*b)),
        Some(_) => Err(bad()),
    }
}

fn opt_int(obj: &Map<String, Value>, key: &str) -> Result<Option<i64>, ParticipantError> {
    match obj.get(key) {
        None => Ok(None),
        Some(Value::Number(n)) => n.as_i64().map(Some).ok_or_else(bad),
        Some(_) => Err(bad()),
    }
}

fn nullish_f64(
    obj: &Map<String, Value>,
    key: &str,
) -> Result<Option<Option<f64>>, ParticipantError> {
    match obj.get(key) {
        None => Ok(None),
        Some(Value::Null) => Ok(Some(None)),
        Some(Value::Number(n)) => n.as_f64().map(|f| Some(Some(f))).ok_or_else(bad),
        Some(_) => Err(bad()),
    }
}

impl ParticipantUpdateData {
    /// v4 `updateParticipantSchema.parse(raw)`. Unknown keys are stripped (Zod's
    /// default object mode); a wrong-typed key fails the whole parse.
    pub fn from_value(raw: &Value) -> Result<Self, ParticipantError> {
        let obj = raw.as_object().ok_or_else(bad)?;
        let out = Self {
            participant_id: opt_str(obj, "participantId")?.ok_or_else(bad)?,
            connection_profile_id: opt_str(obj, "connectionProfileId")?,
            image_profile_id: nullish_str(obj, "imageProfileId")?,
            selected_system_prompt_id: nullish_str(obj, "selectedSystemPromptId")?,
            display_order: opt_int(obj, "displayOrder")?,
            is_active: opt_bool(obj, "isActive")?,
            status: opt_str(obj, "status")?,
            controlled_by: opt_str(obj, "controlledBy")?,
            has_history_access: opt_bool(obj, "hasHistoryAccess")?,
            join_scenario: nullish_str(obj, "joinScenario")?,
            talkativeness: nullish_f64(obj, "talkativeness")?,
        };
        out.validate()?;
        Ok(out)
    }
}

impl ParticipantAddData {
    /// v4 `addParticipantSchema.parse(raw)`. `type: z.literal('CHARACTER')` is
    /// enforced here (the bag entrance carries a raw object, unlike the verb,
    /// whose name IS the narrowing).
    pub fn from_value(raw: &Value) -> Result<Self, ParticipantError> {
        let obj = raw.as_object().ok_or_else(bad)?;
        if obj.get("type").and_then(Value::as_str) != Some("CHARACTER") {
            return Err(bad());
        }
        let out = Self {
            character_id: opt_str(obj, "characterId")?.ok_or_else(bad)?,
            connection_profile_id: opt_str(obj, "connectionProfileId")?,
            image_profile_id: nullish_str(obj, "imageProfileId")?,
            display_order: opt_int(obj, "displayOrder")?,
            has_history_access: opt_bool(obj, "hasHistoryAccess")?,
            join_scenario: nullish_str(obj, "joinScenario")?,
            controlled_by: opt_str(obj, "controlledBy")?,
            outfit_selection: obj.get("outfitSelection").cloned(),
        };
        out.validate()?;
        Ok(out)
    }
}
