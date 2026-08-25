//! Markdown transcript export — v4 `lib/export/markdown-transcript.ts`
//! (`b3ee00f1`, P4.d28).
//!
//! Renders a chat as a single, deterministic Markdown document — the readable
//! record of what was said, not a data interchange format (that's
//! [`super::chat_export`]'s SillyTavern JSONL and the `.qtap` export) and not
//! the pre-rendered HTML cache. Given the same chat state it always produces
//! byte-identical output: nothing here reads the wall clock.
//!
//! ⚠ **Not** [`super::conversation_markdown`] — that is P4.6BM's Scriptorium
//! CONVERSATION_RENDER renderer (v4 `lib/scriptorium/markdown-renderer.ts`), a
//! different file with a different job.
//!
//! What's included:
//! - The opening scenario (`chat.scenarioText`), when present.
//! - Every participant- and user-authored message (the active swipe of each
//!   swipe group — the same variant the Salon displays).
//! - Pascal roll announcements, Carina answers (including Brahma Console
//!   answers), user-authored announcements (Insert Announcement, whether voiced
//!   by a Staff member, a character, or a custom name), and the Host's
//!   continuation / merge notices that link a chat to the conversation it
//!   continues or absorbs.
//!
//! Everything else — SYSTEM/TOOL roles, Staff housekeeping chatter (memory
//! whispers, image announcements, time marks, …) — is left out. Prompts sent to
//! LLMs never appear.
//!
//! Each message renders under a `## Speaker — timestamp` heading. Timestamps are
//! the chat's own clock: fictional time when the chat runs one, otherwise real
//! time, both rendered in the chat's resolved timezone and configured format.
//! DATE_ONLY / TIME_ONLY formats are promoted to FRIENDLY — a transcript needs
//! both halves of the timestamp to stay readable.
//!
//! ## Shapes
//!
//! v4's renderer takes typed `ChatMetadata` / `ChatEvent[]`; v5's repositories
//! marshal both as `serde_json::Value` (the [`super::chat_export`] precedent),
//! so this reads the same JSON the repos hand the route. `timestampConfig`
//! likewise arrives as raw column JSON — [`config_from_value`] is the local twin
//! of the host's `timestamp_config_from_value` (`quilltap-host/src/spine.rs`),
//! defaulting a missing `format`/`mode` the way Zod's `.default()` would rather
//! than reproducing v4's `FORMAT_OPTIONS[undefined]` throw; every config a v4
//! write leaves behind carries all five keyed defaults.

use std::collections::HashMap;

use serde_json::Value;

use crate::api::types::{ErrorKind, Response};
use crate::chat_timestamp::{
    calculate_timestamp_at, default_timestamp_config, resolve_timezone, InvalidTimezone,
    TimestampConfig, TimestampFormat, TimestampMode,
};
use crate::clock::iso_to_ms;
use crate::db::runtime::Db;
use crate::db::{characters_read, chat_settings, chats_messages_read, chats_read, users, DbError};
use crate::jsstr::{is_js_ws, js_trim};
use crate::services::carina_query::BRAHMA_CARINA_ANSWERER_ID;
use crate::staff_display_names::staff_display_name;
use crate::templates::{process_template, TemplateContext};

/// Host notices a reader needs: where the conversation came from or moved to,
/// and any mid-transcript revision of the scene. The header prints whatever
/// scene is in force at export time, so without the revision notices a reader
/// would see the story relocate with nothing to mark the move. (v4
/// `HOST_LINK_KINDS`; `scenario-change` joined at v4 [`44a8137e`].)
const HOST_LINK_KINDS: [&str; 5] = [
    "continuation-from",
    "continuation-to",
    "merge-from",
    "merge-to",
    "scenario-change",
];

/// [`HOST_LINK_KINDS`], for the cross-module pin in
/// `host_notifications`: the kind the Host writer stamps on a scenario
/// revision has to be one this set keeps, or every revision notice would
/// vanish from an export in silence.
pub fn host_link_kinds() -> &'static [&'static str] {
    &HOST_LINK_KINDS
}

/// The host offset the zone-less formatting path reads (v4
/// `date.getTimezoneOffset()`, positive = west of UTC).
///
/// v4 asks each `Date` for its own offset, so a transcript spanning a DST change
/// in the HOST zone (only reachable with no timezone configured anywhere — not
/// in the chat, not in the Salon defaults, not in `QUILLTAP_TIMEZONE`) renders
/// each message at the offset then in force. [`LocalOffset::Zone`] reproduces
/// that per message; [`LocalOffset::Fixed`] is what the differential injects
/// (`TZ=UTC` ⇒ 0), mirroring the rest of the `chat_timestamp` family.
#[derive(Debug, Clone, Copy)]
pub enum LocalOffset<'a> {
    Fixed(i64),
    Zone(&'a str),
}

impl LocalOffset<'_> {
    fn at(&self, utc_ms: i64) -> i64 {
        match self {
            LocalOffset::Fixed(m) => *m,
            LocalOffset::Zone(name) => {
                let Ok(tz) = jiff::tz::TimeZone::get(name) else {
                    return 0;
                };
                let Ok(ts) = jiff::Timestamp::from_millisecond(utc_ms) else {
                    return 0;
                };
                // jiff is east-positive; JS getTimezoneOffset() is west-positive.
                -(tz.to_offset(ts).seconds() as i64) / 60
            }
        }
    }
}

/// v4 `MarkdownTranscriptInput`.
pub struct MarkdownTranscriptInput<'a> {
    /// The chat record as `chats_read::find_by_id` marshals it.
    pub chat: &'a Value,
    /// The full event list from `chats_messages_read::get_messages`, already
    /// `createdAt`-ascending.
    pub events: &'a [Value],
    /// Character display names keyed by character id (participants, announcers,
    /// Carina answerers).
    pub character_names_by_id: &'a HashMap<String, String>,
    /// The human operator's display name.
    pub user_name: &'a str,
    /// Salon-level `defaultTimestampConfig`, used when the chat has none.
    pub default_timestamp_config: Option<&'a Value>,
    /// Salon-level timezone (`chatSettings.timezone`), second link of the
    /// `resolveTimezone` chain.
    pub chat_settings_timezone: Option<&'a str>,
    /// The host offset source for the zone-less path.
    pub local_offset: LocalOffset<'a>,
}

fn s<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.get(key).and_then(Value::as_str)
}

/// A stored `timestampConfig` object → the typed config. See the module header
/// for why a missing enum falls to its Zod default rather than throwing.
fn config_from_value(v: &Value) -> Option<TimestampConfig> {
    let obj = v.as_object()?;
    let mode = match obj.get("mode").and_then(Value::as_str).unwrap_or("NONE") {
        "START_ONLY" => TimestampMode::StartOnly,
        "EVERY_MESSAGE" => TimestampMode::EveryMessage,
        "EVERY_N_MINUTES" => TimestampMode::EveryNMinutes,
        _ => TimestampMode::None,
    };
    let format = match obj
        .get("format")
        .and_then(Value::as_str)
        .unwrap_or("FRIENDLY")
    {
        "ISO8601" => TimestampFormat::Iso8601,
        "DATE_ONLY" => TimestampFormat::DateOnly,
        "TIME_ONLY" => TimestampFormat::TimeOnly,
        "CUSTOM" => TimestampFormat::Custom,
        _ => TimestampFormat::Friendly,
    };
    let str_key = |k: &str| obj.get(k).and_then(Value::as_str).map(String::from);
    Some(TimestampConfig {
        mode,
        format,
        custom_format: str_key("customFormat"),
        use_fictional_time: obj
            .get("useFictionalTime")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        fictional_base_timestamp: str_key("fictionalBaseTimestamp"),
        fictional_base_real_time: str_key("fictionalBaseRealTime"),
        auto_prepend: obj
            .get("autoPrepend")
            .and_then(Value::as_bool)
            .unwrap_or(true),
        timezone: str_key("timezone"),
        interval_minutes: obj
            .get("intervalMinutes")
            .and_then(Value::as_f64)
            .map(|v| v as i64)
            .unwrap_or(15),
    })
}

/// True when the message belongs in the readable transcript — v4
/// `isTranscriptMessage`.
fn is_transcript_message(msg: &Value) -> bool {
    match s(msg, "role") {
        Some("SYSTEM") | Some("TOOL") => return false,
        _ => {}
    }
    // JS truthiness: an empty `systemSender` string is falsy, so it is "no
    // sender" and the message rides the participant path.
    match s(msg, "systemSender").filter(|v| !v.is_empty()) {
        Some("pascal") | Some("carina") => true,
        Some("host") => HOST_LINK_KINDS.contains(&s(msg, "systemKind").unwrap_or("")),
        // Other Staff messages only when the user authored them via Insert
        // Announcement.
        Some(_) => s(msg, "systemKind") == Some("announcement"),
        None => true,
    }
}

/// Headings must stay on one line; names occasionally carry stray whitespace —
/// v4 `headingSafe` (`replace(/\s+/g, ' ').trim()`, JS `\s` semantics).
fn heading_safe(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut in_ws = false;
    for c in name.chars() {
        if is_js_ws(c) {
            in_ws = true;
            continue;
        }
        if in_ws && !out.is_empty() {
            out.push(' ');
        }
        in_ws = false;
        out.push(c);
    }
    out
}

/// v4 `resolveSpeakerName` — the precedence, in order: `customAnnouncer`,
/// Carina's answerer (Brahma via the sentinel), any other `systemSender` (with
/// the raw-key fallback), the participant's character, the USER role, then the
/// primary character.
fn resolve_speaker_name(
    msg: &Value,
    participants_by_id: &HashMap<&str, &Value>,
    character_names_by_id: &HashMap<String, String>,
    user_name: &str,
    primary_character_name: Option<&str>,
) -> String {
    // Mirrors the Salon's getMessageAvatar precedence: customAnnouncer wins over
    // systemSender by construction (they are mutually exclusive).
    if let Some(announcer) = msg.get("customAnnouncer").filter(|v| v.is_object()) {
        if s(announcer, "kind") == Some("character") {
            if let Some(cid) = s(announcer, "characterId").filter(|v| !v.is_empty()) {
                return character_names_by_id
                    .get(cid)
                    .cloned()
                    .unwrap_or_else(|| "Off-scene character".to_string());
            }
        }
        // `displayName || 'Announcement'` — JS truthiness, so an empty string
        // takes the fallback too.
        return match s(announcer, "displayName").filter(|v| !v.is_empty()) {
            Some(name) => name.to_string(),
            None => "Announcement".to_string(),
        };
    }

    let sender = s(msg, "systemSender").filter(|v| !v.is_empty());

    if sender == Some("carina") {
        let answerer = msg
            .get("carinaMeta")
            .and_then(|m| s(m, "answererId"))
            .filter(|v| !v.is_empty());
        if answerer == Some(BRAHMA_CARINA_ANSWERER_ID) {
            return "Brahma".to_string();
        }
        if let Some(id) = answerer {
            if let Some(name) = character_names_by_id.get(id) {
                return name.clone();
            }
        }
        return "Carina".to_string();
    }

    if let Some(sender) = sender {
        // v4's inline `STAFF_DISPLAY_NAMES[sender] ?? sender`, now the shared
        // `staffDisplayName` — the raw-tag fallback is the same one this
        // exporter already carried, so the bytes are unmoved.
        return staff_display_name(Some(sender));
    }

    if let Some(pid) = s(msg, "participantId").filter(|v| !v.is_empty()) {
        // A broken vault leaves the participant present but nameless; v4 falls
        // through to the role/primary fallbacks.
        if let Some(participant) = participants_by_id.get(pid) {
            if let Some(cid) = s(participant, "characterId") {
                if let Some(name) = character_names_by_id.get(cid) {
                    return name.clone();
                }
            }
        }
    }

    if s(msg, "role") == Some("USER") {
        return user_name.to_string();
    }
    primary_character_name.unwrap_or("Assistant").to_string()
}

/// Collapse swipe groups to the variant the Salon displays — v4
/// `collapseSwipes`: the highest `swipeIndex` of each group (strict `>`, so the
/// first-seen wins a tie), emitted at the group's chronological position.
///
/// Deliberately NOT the order-sensitive algorithm the SillyTavern export
/// reproduces ([`super::chat_export`]) — this is a different function in v4.
fn collapse_swipes(messages: Vec<&Value>) -> Vec<&Value> {
    let mut best_by_group: HashMap<&str, &Value> = HashMap::new();
    for msg in &messages {
        let Some(group) = s(msg, "swipeGroupId").filter(|v| !v.is_empty()) else {
            continue;
        };
        // `msg.swipeIndex ?? 0` on both sides — a JSON number, so read as f64.
        let index = msg.get("swipeIndex").and_then(Value::as_f64).unwrap_or(0.0);
        match best_by_group.get(group) {
            Some(best) => {
                let best_index = best
                    .get("swipeIndex")
                    .and_then(Value::as_f64)
                    .unwrap_or(0.0);
                if index > best_index {
                    best_by_group.insert(group, msg);
                }
            }
            None => {
                best_by_group.insert(group, msg);
            }
        }
    }

    let mut emitted: std::collections::HashSet<&str> = std::collections::HashSet::new();
    let mut result: Vec<&Value> = Vec::new();
    for msg in messages {
        let Some(group) = s(msg, "swipeGroupId").filter(|v| !v.is_empty()) else {
            result.push(msg);
            continue;
        };
        if !emitted.insert(group) {
            continue;
        }
        result.push(best_by_group.get(group).copied().unwrap_or(msg));
    }
    result
}

/// Build the whole transcript document — v4 `buildMarkdownTranscript`. Pure and
/// deterministic; the only `Err` is an unresolvable IANA zone, which is v4's
/// `Intl.DateTimeFormat` throw (the route answers it as a 500).
pub fn build_markdown_transcript(
    input: &MarkdownTranscriptInput<'_>,
) -> Result<String, InvalidTimezone> {
    let chat = input.chat;

    // `chat.timestampConfig ?? defaultTimestampConfig ?? FALLBACK_CONFIG`.
    // chats_read omits a NULL column, so an absent key is exactly v4's
    // `undefined`.
    let config = chat
        .get("timestampConfig")
        .filter(|v| !v.is_null())
        .and_then(config_from_value)
        .or_else(|| {
            input
                .default_timestamp_config
                .filter(|v| !v.is_null())
                .and_then(config_from_value)
        })
        // v4's `FALLBACK_CONFIG`, now the shared `DEFAULT_TIMESTAMP_CONFIG` —
        // the same literal the exporter carried as a fifth copy.
        .unwrap_or_else(default_timestamp_config);

    // DATE_ONLY / TIME_ONLY drop half the information a transcript needs. The
    // promotion is a copy: `resolveTimezone` still reads the ORIGINAL config's
    // timezone (same value either way, but the order is v4's).
    let effective_config = match config.format {
        TimestampFormat::DateOnly | TimestampFormat::TimeOnly => TimestampConfig {
            format: TimestampFormat::Friendly,
            ..config.clone()
        },
        _ => config.clone(),
    };
    let timezone = resolve_timezone(config.timezone.as_deref(), input.chat_settings_timezone);
    // `new Date(chat.createdAt)` — the anchor for a fictional clock whose config
    // predates `fictionalBaseRealTime` stamping.
    let fallback_anchor = s(chat, "createdAt").and_then(iso_to_ms).unwrap_or(0);

    let participants: Vec<&Value> = chat
        .get("participants")
        .and_then(Value::as_array)
        .map(|a| a.iter().collect())
        .unwrap_or_default();
    let mut participants_by_id: HashMap<&str, &Value> = HashMap::new();
    for p in &participants {
        if let Some(id) = s(p, "id") {
            participants_by_id.insert(id, p);
        }
    }
    // The first CHARACTER participant in stored order — v4's `find`.
    let primary_character_name = participants
        .iter()
        .find(|p| s(p, "type") == Some("CHARACTER"))
        .and_then(|p| s(p, "characterId"))
        .and_then(|cid| input.character_names_by_id.get(cid))
        .map(String::as_str);

    let messages = collapse_swipes(
        input
            .events
            .iter()
            .filter(|e| s(e, "type") == Some("message"))
            .filter(|e| is_transcript_message(e))
            .collect(),
    );

    let mut lines: Vec<String> = Vec::new();
    let title = s(chat, "title")
        .filter(|t| !t.is_empty())
        .unwrap_or("Untitled Chat");
    lines.push(format!("# {}", heading_safe(title)));
    lines.push(String::new());

    // Scenario text is stored with its template variables; render the names in.
    let scenario = match s(chat, "scenarioText").filter(|t| !t.is_empty()) {
        Some(text) => {
            let mut ctx = TemplateContext::default();
            if let Some(name) = primary_character_name {
                ctx.set("char", name);
            }
            ctx.set("user", input.user_name);
            js_trim(&process_template(text, &ctx)).to_string()
        }
        None => String::new(),
    };
    if !scenario.is_empty() {
        lines.push("## Scenario".to_string());
        lines.push(String::new());
        lines.push(scenario);
        lines.push(String::new());
    }

    for msg in messages {
        let speaker = heading_safe(&resolve_speaker_name(
            msg,
            &participants_by_id,
            input.character_names_by_id,
            input.user_name,
            primary_character_name,
        ));
        let whisper = match msg.get("targetParticipantIds").and_then(Value::as_array) {
            Some(ids) if !ids.is_empty() => " (whisper)",
            _ => "",
        };
        let created_at = s(msg, "createdAt").and_then(iso_to_ms).unwrap_or(0);
        let stamp = calculate_timestamp_at(
            created_at,
            &effective_config,
            timezone.as_deref(),
            Some(fallback_anchor),
            input.local_offset.at(created_at),
        )?;

        lines.push(format!("## {speaker}{whisper} — {}", stamp.formatted));
        lines.push(String::new());
        let body = js_trim(s(msg, "content").unwrap_or_default());
        if !body.is_empty() {
            lines.push(body.to_string());
            lines.push(String::new());
        }
    }

    // Single trailing newline — v4's `replace(/\n*$/, '\n')`.
    let joined = lines.join("\n");
    let trimmed = joined.trim_end_matches('\n');
    Ok(format!("{trimmed}\n"))
}

/// Filesystem-safe download filename for a chat's transcript — v4
/// `transcriptFilename`. Non-ASCII survives, so the RFC 5987 arm of
/// [`crate::content_disposition::build_content_disposition`] is reachable here
/// (unlike the SillyTavern export, whose name is ASCII furniture).
pub fn transcript_filename(chat: &Value) -> String {
    let title = s(chat, "title").filter(|t| !t.is_empty()).unwrap_or("chat");
    let sanitized: String = title
        .chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '|' | '?' | '*' | '/' | '\\' => '_',
            c if (c as u32) <= 0x1f => '_',
            c => c,
        })
        .collect();
    let base = js_trim(&sanitized);
    let base = if base.is_empty() { "chat" } else { base };
    format!("{base}_transcript.md")
}

// ===========================================================================
// The route tier — v4 `app/api/v1/chats/[id]/actions/export-markdown.ts`
// ===========================================================================

/// The host's local IANA zone, for the zone-less formatting path (the
/// `api::autonomous_rooms::system_tz` precedent — v4 reads the same host TZ
/// through `Intl`). A fixed offset with no IANA name falls back to `UTC`.
fn system_tz() -> String {
    jiff::tz::TimeZone::system()
        .iana_name()
        .unwrap_or("UTC")
        .to_string()
}

/// v4 `handleExportMarkdown`. `user_id` selects the operator row (the
/// transcript's `userName` = `user.name || 'User'`).
///
/// The `Content-Type` / `Content-Disposition` / `Cache-Control` headers belong
/// at the quilltap-web edge (the `characters_routes.rs:373` byte-leg
/// precedent); this returns the bytes and the filename v4's header names.
pub fn chat_export_markdown(db: &Db, user_id: &str, chat_id: &str) -> Response {
    let cid = chat_id.to_string();
    let uid = user_id.to_string();
    let tz = system_tz();
    let out: Result<Result<Response, Response>, DbError> = db.read_main(|main| {
        db.read_mount_index(|mount| {
            let Some(chat) = chats_read::find_by_id(main, &cid)? else {
                return Ok(Err(Response::error(ErrorKind::NotFound, "Chat not found")));
            };

            let events = chats_messages_read::get_messages(main, &cid)?;

            // Every character the transcript may need a name for:
            // participants, custom announcers voiced by a (possibly
            // off-scene) character, and Carina answerers. The Brahma
            // sentinel is not a real character. v4 builds a Set, so ids are
            // deduped in first-seen order.
            let mut character_ids: Vec<String> = Vec::new();
            let push_id = |id: &str, ids: &mut Vec<String>| {
                if !ids.iter().any(|existing| existing == id) {
                    ids.push(id.to_string());
                }
            };
            if let Some(participants) = chat.get("participants").and_then(Value::as_array) {
                for p in participants {
                    if let Some(id) = s(p, "characterId").filter(|v| !v.is_empty()) {
                        push_id(id, &mut character_ids);
                    }
                }
            }
            for event in &events {
                if s(event, "type") != Some("message") {
                    continue;
                }
                if let Some(id) = event
                    .get("customAnnouncer")
                    .and_then(|a| s(a, "characterId"))
                    .filter(|v| !v.is_empty())
                {
                    push_id(id, &mut character_ids);
                }
                if let Some(id) = event
                    .get("carinaMeta")
                    .and_then(|m| s(m, "answererId"))
                    .filter(|v| !v.is_empty() && *v != BRAHMA_CARINA_ANSWERER_ID)
                {
                    push_id(id, &mut character_ids);
                }
            }

            // Broken-vault characters are dropped by findByIds; their
            // messages fall back to the generic names in the builder.
            let characters = characters_read::find_by_ids(main, mount, &character_ids)?;
            let mut character_names_by_id: HashMap<String, String> = HashMap::new();
            for c in &characters {
                if let (Some(id), Some(name)) = (s(c, "id"), s(c, "name")) {
                    character_names_by_id.insert(id.to_string(), name.to_string());
                }
            }

            let settings = chat_settings::find_by_user_id(main, &uid)?;
            let default_timestamp_config = settings
                .as_ref()
                .and_then(|s| s.get("defaultTimestampConfig"));
            let chat_settings_timezone = settings
                .as_ref()
                .and_then(|s| s.get("timezone"))
                .and_then(Value::as_str);

            let user_name = users::find_profile_by_id(main, &uid)?
                .and_then(|u| u.name)
                .filter(|n| !n.is_empty())
                .unwrap_or_else(|| "User".to_string());

            let markdown = match build_markdown_transcript(&MarkdownTranscriptInput {
                chat: &chat,
                events: &events,
                character_names_by_id: &character_names_by_id,
                user_name: &user_name,
                default_timestamp_config,
                chat_settings_timezone,
                local_offset: LocalOffset::Zone(&tz),
            }) {
                Ok(md) => md,
                // v4's `Intl` throw lands in the handler's catch.
                Err(_) => {
                    return Ok(Err(Response::error(
                        ErrorKind::Internal,
                        "Failed to export chat as Markdown",
                    )))
                }
            };

            Ok(Ok(Response::ChatMarkdownTranscriptPayload {
                filename: transcript_filename(&chat),
                markdown,
            }))
        })
    });
    match out {
        Ok(Ok(r)) => r,
        Ok(Err(r)) => r,
        // v4's try/catch → serverError.
        Err(_) => Response::error(ErrorKind::Internal, "Failed to export chat as Markdown"),
    }
}
