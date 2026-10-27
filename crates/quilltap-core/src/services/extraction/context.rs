//! The extraction context — port of v4 `lib/memory/extraction/context.ts`
//! (`ac6348c5f`, memory refactor Phase D; pin `83334b9a5`): everything a unit
//! of work's passes share, built once.
//!
//! v4's why, carried: a unit of work is one closed turn, one Carina answer,
//! one fold, one catch-up job or one dry run. Each calls
//! [`build_extraction_context`] once and hands the context to every pass it
//! runs, so the chat, its cast, the project, the cheap-LLM route, the memory
//! calendar, the settings and the rate limiter are read once — where the
//! per-turn handler, the fold passes, the catch-ups and the dry run used to
//! rebuild each for themselves (and Carina missed the project, the episode
//! pass the uncensored fallback; bugs 213, 222).
//!
//! v5 shape (work order P4.D296 Tier 1 #7):
//!  - the chat-settings row is NULLABLE — v5's old `Chat settings not found
//!    for user` throw is gone (D1);
//!  - the settings are the KEYSTONE's façade
//!    ([`crate::memory_settings::resolve_memory_settings`], L-13), its
//!    `[MemorySettings]` DEBUG once per context;
//!  - canon is memoised per unit (L-18): [`ExtractionContext::canon_for_self`]
//!    / [`ExtractionContext::canon_for`] (keyed `holder\u0000about`), so the
//!    `[Canon]` lines appear once per (holder) / (holder, about) per unit;
//!  - the messages are read lazily, once ([`ExtractionContext::messages`]);
//!  - the idempotency scope (L-16, C1-B) and the memory-calendar host
//!    fallback ride the context into every admission.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use serde_json::Value;

use crate::db::fallback::error_text;
use crate::db::runtime::Db;
use crate::db::{chat_settings, chats_read, connection_profiles, DbError};
use crate::memory_extraction::prompts::shared::{ExtractionClock, OrientingContext};
use crate::memory_format::Pronouns;
use crate::memory_settings::{resolve_memory_settings, MemoryExtractionSettings};
use crate::services::admission::{AdmissionKeyScope, DryRunBook};
use crate::services::canon_loaders::{
    load_canon_for_observer_about_subject, load_canon_for_self_with_commonplace, CanonSubject,
};
use crate::services::extraction::cursors::CursorStore;
use crate::services::extraction::rate_limit::ExtractionRateLimiter;
use crate::services::extraction::route::{resolve_extraction_route, ExtractionRoute};
use crate::services::memory_time_zone::refresh_memory_time_zone;
use crate::services::speaker_names::{resolve_speaker_names, SpeakerNames};
use crate::services::turn_transcript::{load_chat_message_events, pronouns_from_character};

/// One character the unit of work may extract for or about (v4 `CastMember`).
#[derive(Clone, Debug)]
pub struct CastMember {
    pub character_id: String,
    /// The seat in this chat, or `None` for a character outside it (a Carina
    /// answerer).
    pub participant_id: Option<String>,
    pub name: String,
    pub pronouns: Option<Pronouns>,
    /// Played by the human (`controlledBy: 'user'`).
    pub is_user: bool,
    /// Seated and present (`isParticipantPresent`); false for an answerer
    /// outside the chat.
    pub present: bool,
    /// The hydrated character row.
    pub character: Value,
}

/// v4 `ExtractionContext['clock']` — the memory calendar, read once
/// (`MemoryClock & { timelineMode }`); windows set their own instant
/// ([`clock_at`]).
#[derive(Clone, Debug)]
pub struct ContextClock {
    pub time_zone: String,
    pub now_ms: f64,
    pub timeline_mode: String,
}

/// v4 `ExtractionContextError` — `reason: 'no-chat' | 'no-connection-profile'`.
/// Any other read failure while building propagates as [`Self::Db`] (v4's
/// rethrow).
#[derive(Debug)]
pub enum ExtractionContextError {
    /// `Chat not found: <chatId>`.
    NoChat(String),
    /// `No connection profile for extraction on chat <chatId>`.
    NoConnectionProfile(String),
    Db(DbError),
}

impl ExtractionContextError {
    /// v4's `error.message` (a DB failure renders its bare text).
    pub fn message(&self) -> String {
        match self {
            Self::NoChat(m) | Self::NoConnectionProfile(m) => m.clone(),
            Self::Db(e) => error_text(e),
        }
    }
}

impl From<DbError> for ExtractionContextError {
    fn from(e: DbError) -> Self {
        Self::Db(e)
    }
}

/// v4 `BuildExtractionContextOptions`.
#[derive(Default)]
pub struct BuildExtractionContextOptions {
    pub dry_run: bool,
    /// The profile the unit of work runs against (budget, cost, cheap-LLM
    /// base).
    pub connection_profile: Option<Value>,
    pub connection_profile_id: Option<String>,
    /// The chat row, when the caller already holds it.
    pub chat: Option<Value>,
    /// The chat's message rows, when the caller already holds them.
    pub messages: Option<Vec<Value>>,
    /// Overrides `chat.contextSummary` as background (a fold passes the
    /// summary it just wrote). `Some(None)` is v4's explicit `null`; `None`
    /// is `undefined` (the chat's own summary is read).
    pub chat_context_summary: Option<Option<String>>,
    /// Characters outside the chat's seats to hydrate (a Carina answerer).
    pub extra_character_ids: Vec<String>,
    /// Defaults to the chat's live cursors, or an empty in-memory store for a
    /// dry run.
    pub cursors: Option<CursorStore>,
    /// A rebuild of history (regenerate, queue-memories, import, the dry run
    /// that previews a regenerate): the rate limit is off, and admissions do
    /// not count towards it.
    pub backfill: bool,
    /// The job attempt's idempotency scope (L-16); `None` outside a job and
    /// for a dry run.
    pub key_scope: Option<AdmissionKeyScope>,
    /// The memory calendar's host fallback (the host's calendar NAME).
    pub host_zone_fallback: String,
    /// v4's `Date.now()` for the clock; `None` reads the wall clock.
    pub now_ms: Option<f64>,
}

/// v4 `ExtractionContext`.
pub struct ExtractionContext {
    pub db: Db,
    pub chat: Value,
    pub chat_id: String,
    pub user_id: String,
    pub clock: ContextClock,
    pub settings: MemoryExtractionSettings,
    /// characterId → member, in v4's `Map` insertion order: every CHARACTER
    /// seat (the user's included), plus any extra character asked for.
    pub cast: Vec<CastMember>,
    /// The user-controlled character's id, when one is seated (and hydrated).
    pub user_character_id: Option<String>,
    pub speaker_names: SpeakerNames,
    pub project_id: Option<String>,
    pub project_description: Option<String>,
    /// Background for the prompt footer, never a memory source.
    pub orienting: OrientingContext,
    pub route: ExtractionRoute,
    pub rate_limit: ExtractionRateLimiter,
    pub cursors: CursorStore,
    pub dry_run: bool,
    pub backfill: bool,
    /// A whole-chat dry run's scratch book. No ported caller sets it (the dry
    /// run is deferred, R-W); the plumbing is v4's (L-19).
    pub dry_run_book: Option<tokio::sync::Mutex<DryRunBook>>,
    pub in_autonomous_room: bool,
    pub key_scope: Option<AdmissionKeyScope>,
    pub host_zone_fallback: String,
    messages: OnceLock<Vec<Value>>,
    self_canon: Mutex<HashMap<String, String>>,
    other_canon: Mutex<HashMap<String, (String, String)>>,
}

impl ExtractionContext {
    /// v4 `cast.get(characterId)`.
    pub fn cast_get(&self, character_id: &str) -> Option<&CastMember> {
        self.cast.iter().find(|m| m.character_id == character_id)
    }

    /// v4 `ctx.userCharacter`.
    pub fn user_character(&self) -> Option<&CastMember> {
        self.user_character_id
            .as_deref()
            .and_then(|id| self.cast_get(id))
    }

    /// v4 `messages()` — the chat's `type: 'message'` rows, chronological;
    /// read once, on first use. A read failure propagates (and is retried on
    /// the next call, where v4 would cache the rejection — no ported caller
    /// asks twice after a failure).
    pub fn messages(&self) -> Result<&[Value], DbError> {
        if let Some(m) = self.messages.get() {
            return Ok(m);
        }
        let loaded = load_chat_message_events(&self.db, &self.chat_id)?;
        Ok(self.messages.get_or_init(|| loaded))
    }

    /// v4 `canonForSelf(holderId)` — the holder's own ALREADY ESTABLISHED
    /// block (card fields + `Commonplace/Self.md`). Memoised.
    pub fn canon_for_self(&self, holder_id: &str) -> String {
        if let Some(b) = self.self_canon.lock().unwrap().get(holder_id) {
            return b.clone();
        }
        let member = self.cast_get(holder_id);
        let c = member.map(|m| &m.character);
        let field = |k: &str| c.and_then(|c| c.get(k)).and_then(Value::as_str);
        let name = field("name")
            .map(str::to_string)
            .or_else(|| member.map(|m| m.name.clone()))
            .unwrap_or_else(|| holder_id.to_string());
        let canon = load_canon_for_self_with_commonplace(
            &self.db,
            holder_id,
            &name,
            field("manifesto"),
            field("personality"),
            field("description"),
            field("identity"),
            field("characterDocumentMountPointId"),
        );
        let block = crate::canon::render_self_canon_block(&canon);
        self.self_canon
            .lock()
            .unwrap()
            .insert(holder_id.to_string(), block.clone());
        block
    }

    /// v4 `canonFor(holderId, aboutId)` — what the holder already knows about
    /// the subject (`Others/<name>.md`, identity, digest): `(block, source)`.
    /// Memoised per pair.
    pub fn canon_for(&self, holder_id: &str, about_id: &str) -> (String, String) {
        let key = format!("{holder_id}\u{0}{about_id}");
        if let Some(e) = self.other_canon.lock().unwrap().get(&key) {
            return e.clone();
        }
        let holder = self.cast_get(holder_id).map(|m| &m.character);
        let about = self.cast_get(about_id);
        let about_field = |k: &str| {
            about
                .and_then(|a| a.character.get(k))
                .and_then(Value::as_str)
                .map(str::to_string)
        };
        let canon = load_canon_for_observer_about_subject(
            &self.db,
            holder
                .and_then(|h| h.get("characterDocumentMountPointId"))
                .and_then(Value::as_str),
            holder_id,
            &CanonSubject {
                id: about_id.to_string(),
                name: about
                    .map(|a| a.name.clone())
                    .unwrap_or_else(|| about_id.to_string()),
                identity: about_field("identity"),
                description: about_field("description"),
            },
            true,
        );
        let entry = (
            crate::canon::render_other_canon_block(&canon),
            canon.source.as_str().to_string(),
        );
        self.other_canon.lock().unwrap().insert(key, entry.clone());
        entry
    }
}

/// The connection profile a unit of work runs on when none is named: an LLM
/// seat's, else any seat's (v4 `pickExtractionConnectionProfileId`).
pub fn pick_extraction_connection_profile_id(chat: &Value) -> Option<String> {
    let participants = chat.get("participants").and_then(Value::as_array)?;
    let profile_of = |p: &Value| {
        p.get("connectionProfileId")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    };
    participants
        .iter()
        .find(|p| {
            p.get("type").and_then(Value::as_str) == Some("CHARACTER")
                && p.get("controlledBy").and_then(Value::as_str) != Some("user")
                && profile_of(p).is_some()
        })
        .or_else(|| participants.iter().find(|p| profile_of(p).is_some()))
        .and_then(profile_of)
}

/// The prompt clock for a window: the calendar's zone at the window's instant
/// (v4 `clockAt`).
pub fn clock_at(ctx: &ExtractionContext, now_iso: &str) -> ExtractionClock {
    ExtractionClock {
        now_iso: now_iso.to_string(),
        timeline_mode: ctx.clock.timeline_mode.clone(),
        time_zone: ctx.clock.time_zone.clone(),
    }
}

fn str_of<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v.get(k).and_then(Value::as_str)
}

/// v4 `hydrate(characterId, chatId)`: the overlaid character row, or `None`
/// — a read that throws leaves the character out with a DEBUG line.
fn hydrate(db: &Db, character_id: &str, chat_id: &str) -> Option<Value> {
    let id = character_id.to_string();
    let read = db.read_main(|main| {
        db.read_mount_index(|mount| crate::db::characters_read::find_by_id(main, mount, &id))
    });
    match read {
        Ok(c) => c,
        Err(error) => {
            tracing::debug!(
                chatId = %chat_id,
                characterId = %character_id,
                error = %error_text(&error),
                "[Memory] Character unavailable to extraction; leaving it out"
            );
            None
        }
    }
}

/// v4 `loadProjectDescription(projectId, chatId)`: the description of the
/// chat's project, as background; `None` when absent or unreadable.
fn load_project_description(db: &Db, project_id: Option<&str>, chat_id: &str) -> Option<String> {
    let project_id = project_id.filter(|p| !p.is_empty())?;
    let id = project_id.to_string();
    let read = db.read_main(|main| {
        db.read_mount_index(|mount| {
            crate::db::projects::ProjectsRepository::new(main, mount)
                .find_by_id(&id)
                .map_err(|e| DbError::Internal(e.to_string()))
        })
    });
    match read {
        Ok(p) => p.and_then(|p| {
            p.get("description")
                .and_then(Value::as_str)
                .map(str::to_string)
        }),
        Err(error) => {
            tracing::debug!(
                chatId = %chat_id,
                projectId = %project_id,
                error = %error_text(&error),
                "[Memory] Project description unavailable; continuing without it"
            );
            None
        }
    }
}

/// v4 `isParticipantPresent(status)`.
fn is_present(p: &Value) -> bool {
    matches!(str_of(p, "status"), Some("active") | Some("silent"))
}

/// Build the context for one unit of work (v4 `buildExtractionContext`).
/// Fails with [`ExtractionContextError::NoChat`] /
/// [`ExtractionContextError::NoConnectionProfile`] when the chat is gone or no
/// connection profile can be found; anything else missing (a character, the
/// project) degrades; a failed repository read propagates.
pub fn build_extraction_context(
    db: &Db,
    chat_id: &str,
    user_id: &str,
    mut opts: BuildExtractionContextOptions,
) -> Result<ExtractionContext, ExtractionContextError> {
    let chat = match opts.chat.take() {
        Some(c) => c,
        None => {
            // v4 `repos.chats.findById` is a FALLBACK read (a failure logs and
            // answers `null`), so a failed read is the missing-chat error.
            crate::db::fallback::find_by_id_or_none("chats", chat_id, || {
                let id = chat_id.to_string();
                db.read_main(move |c| chats_read::find_by_id(c, &id))
            })
            .ok_or_else(|| ExtractionContextError::NoChat(format!("Chat not found: {chat_id}")))?
        }
    };

    let connection_profile = match opts.connection_profile.take() {
        Some(p) => Some(p),
        None => {
            let profile_id = opts
                .connection_profile_id
                .clone()
                .or_else(|| pick_extraction_connection_profile_id(&chat));
            match profile_id {
                // v4 `repos.connections.findById` falls back to `null`.
                Some(id) => {
                    crate::db::fallback::find_by_id_or_none("connection_profiles", &id, || {
                        let pid = id.clone();
                        db.read_main(move |c| connection_profiles::find_by_id(c, &pid))
                    })
                }
                None => None,
            }
        }
    };
    let connection_profile = connection_profile.ok_or_else(|| {
        ExtractionContextError::NoConnectionProfile(format!(
            "No connection profile for extraction on chat {chat_id}"
        ))
    })?;

    // v4 `Promise.all([chatSettings, connections, resolveMemorySettings,
    // refreshMemoryTimeZone])` — every one a read that DEGRADES (L-17,
    // measured through v4's real build at the pin: a throwing `chat_settings`
    // / `connection_profiles` / `characters` / `chats` read never fails it):
    // `chatSettings.findByUserId` logs `Error finding chat settings by user ID`
    // and answers `null`, `connections.findByUserId` is `findByFilter`'s `[]`.
    let uid = user_id.to_string();
    // Contract C2's home logs a failed read; a failed pool checkout routes
    // through the same emitter (v4's `getCollection()` runs inside the one
    // `safeQuery`).
    let chat_settings_row = db
        .read_main(move |c| Ok(chat_settings::find_by_user_id_or_none(c, &uid)))
        .unwrap_or_else(|error| {
            chat_settings::log_find_by_user_id_failure(user_id, &error);
            None
        });
    let uid = user_id.to_string();
    let available_profiles =
        crate::db::fallback::find_by_filter_or_empty("connection_profiles", || {
            db.read_main(move |c| connection_profiles::find_by_user_id(c, &uid))
        });
    let uid = user_id.to_string();
    let settings = db.read_main(move |c| Ok(resolve_memory_settings(c, Some(&uid))))?;
    let now_ms = opts
        .now_ms
        .unwrap_or_else(|| crate::clock::now_unix_ms() as f64);
    let time_zone = refresh_memory_time_zone(db, &opts.host_zone_fallback, now_ms);

    // The cast: every CHARACTER seat, hydrated once.
    let participants: Vec<Value> = chat
        .get("participants")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut cast: Vec<CastMember> = Vec::new();
    for p in &participants {
        if str_of(p, "type") != Some("CHARACTER") {
            continue;
        }
        let Some(character_id) = str_of(p, "characterId").filter(|c| !c.is_empty()) else {
            continue;
        };
        if cast.iter().any(|m| m.character_id == character_id) {
            continue;
        }
        let Some(character) = hydrate(db, character_id, chat_id) else {
            continue;
        };
        cast.push(CastMember {
            character_id: str_of(&character, "id").unwrap_or(character_id).to_string(),
            participant_id: str_of(p, "id").map(str::to_string),
            name: str_of(&character, "name").unwrap_or_default().to_string(),
            pronouns: pronouns_from_character(&character),
            is_user: str_of(p, "controlledBy") == Some("user"),
            present: is_present(p),
            character,
        });
    }
    for id in &opts.extra_character_ids {
        if cast.iter().any(|m| &m.character_id == id) {
            continue;
        }
        let Some(character) = hydrate(db, id, chat_id) else {
            continue;
        };
        cast.push(CastMember {
            character_id: str_of(&character, "id").unwrap_or(id).to_string(),
            participant_id: None,
            name: str_of(&character, "name").unwrap_or_default().to_string(),
            pronouns: pronouns_from_character(&character),
            is_user: false,
            present: false,
            character,
        });
    }
    let user_character_id = participants
        .iter()
        .find(|p| {
            str_of(p, "type") == Some("CHARACTER")
                && str_of(p, "controlledBy") == Some("user")
                && str_of(p, "characterId").is_some_and(|c| !c.is_empty())
        })
        .and_then(|p| str_of(p, "characterId"))
        .filter(|id| cast.iter().any(|m| m.character_id == *id))
        .map(str::to_string);

    let project_id = str_of(&chat, "projectId").map(str::to_string);
    let project_description = load_project_description(db, project_id.as_deref(), chat_id);
    let speaker_names = db.read_main(|c| Ok(resolve_speaker_names(c, &participants)))?;

    let route = resolve_extraction_route(
        &chat,
        &connection_profile,
        chat_settings_row.as_ref(),
        &available_profiles,
    );
    let dry_run = opts.dry_run;
    let limits = settings.extraction.limits.clone();
    let rate_limited = limits.enabled;
    let limiter_limits = if opts.backfill {
        crate::db::instance_settings::MemoryExtractionLimits {
            enabled: false,
            ..limits
        }
    } else {
        limits
    };
    let chat_context_summary = match opts.chat_context_summary.take() {
        Some(explicit) => explicit,
        None => str_of(&chat, "contextSummary").map(str::to_string),
    };
    let cursors = opts.cursors.take().unwrap_or_else(|| {
        if dry_run {
            CursorStore::memory(chat_id, Default::default())
        } else {
            CursorStore::live(chat_id)
        }
    });
    let messages = OnceLock::new();
    if let Some(m) = opts.messages.take() {
        let _ = messages.set(m);
    }

    let ctx = ExtractionContext {
        db: db.clone(),
        chat_id: chat_id.to_string(),
        user_id: user_id.to_string(),
        clock: ContextClock {
            time_zone: time_zone.clone(),
            now_ms,
            timeline_mode: str_of(&chat, "timelineMode")
                .unwrap_or("realtime")
                .to_string(),
        },
        settings: settings.extraction.clone(),
        cast,
        user_character_id,
        speaker_names,
        project_id: project_id.clone(),
        project_description: project_description.clone(),
        orienting: OrientingContext {
            project_description: project_description.clone(),
            chat_context_summary,
        },
        route,
        rate_limit: ExtractionRateLimiter::new(limiter_limits),
        cursors,
        dry_run,
        backfill: opts.backfill,
        dry_run_book: None,
        in_autonomous_room: str_of(&chat, "chatType") == Some("autonomous"),
        key_scope: opts.key_scope.take(),
        host_zone_fallback: opts.host_zone_fallback.clone(),
        messages,
        self_canon: Mutex::new(HashMap::new()),
        other_canon: Mutex::new(HashMap::new()),
        chat,
    };

    let user_character = ctx.user_character_id.as_deref().unwrap_or("null");
    let project = project_id.as_deref().unwrap_or("null");
    tracing::debug!(
        chatId = %chat_id,
        dryRun = dry_run,
        backfill = ctx.backfill,
        cast = ctx.cast.len(),
        userCharacterId = %user_character,
        projectId = %project,
        hasProjectDescription = project_description.is_some(),
        timeZone = %time_zone,
        otherPass = %ctx.settings.mode.other_pass,
        rateLimited = rate_limited,
        "[Memory] Extraction context built"
    );
    Ok(ctx)
}
