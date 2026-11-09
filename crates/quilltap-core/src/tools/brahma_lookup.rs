//! Port of v4's Brahma lookup tool handlers (P4.D374, v4 `c539c82e0`) —
//! `lib/tools/handlers/brahma-lookup-handler.ts` WHOLE.
//!
//! The four read-only shortcuts the Brahma Console reaches for before it falls
//! back to hand-written SQL:
//!
//!   - `lookup_character` — a character's core fields, memory count and
//!     conversations in one call.
//!   - `lookup_memories`  — a relevance search over one character's (or every
//!     character's) Commonplace Book, as inspection: nothing is stamped as
//!     recalled (no access stamp is written), so a lookup never changes what a
//!     character will remember next.
//!   - `lookup_messages`  — the FTS5 message index (`searchMessagesGlobal`, the
//!     same path as the global search bar), with conversation titles, dates and
//!     speaker names attached.
//!   - `read_messages`    — a bounded, speaker-labelled slice of one
//!     conversation.
//!
//! Every one of them answers in the operator's terms (names, titles, dates)
//! rather than raw rows, and every text field is truncated, so a single call
//! can never flood the console's context the way an unbounded `run_sql` dump
//! did.
//!
//! Errors are returned as data (`{ success: false, error }`), never thrown, so
//! the model can read the message and adjust. Each tool has ONE catch (round
//! ruling R-6): validation runs before it; everything else is inside it, and a
//! failure logs `<tool> failed` and answers `<tool> failed: <bare message>`.
//! The one per-read swallow is `loadChats`' per-chat DEBUG-and-skip.
//!
//! `BRAHMA_LOOKUP_LONE_SURROGATE_CLIP` (ruling L-3): `clip`'s
//! `trimmed.slice(0, max)` can split a surrogate pair and keep the lone half
//! in v4; a Rust `String` cannot hold one, so [`utf16_truncate`] decodes
//! U+FFFD there. The `… [truncated, N chars in all]` count is the UTF-16
//! length either way.

use serde::{Serialize, Serializer};
use serde_json::{Map, Value};

use crate::api::zod_issues::{
    zod_lookup_character_issues, zod_lookup_memories_issues, zod_lookup_messages_issues,
    zod_read_messages_issues, ZodIssue,
};
use crate::chat_activity::is_character_authored_message;
use crate::collation::locale_compare;
use crate::db::chats_search::ChatSearchRepository;
use crate::db::runtime::Db;
use crate::db::{characters_read, chats_messages_read, chats_read, memories_read};
use crate::db::{fallback, js_number_to_json, DbError};
use crate::jsnum::math_round;
use crate::jsstr::{js_index_of, js_trim, utf16_len, utf16_slice_from, utf16_truncate};
use crate::memory_scoring::importance::filter_importance;
use crate::memory_scoring::js_sort::js_array_sort_by;
use crate::memory_time::calendar::zoned_day_bounds_iso;
use crate::model::embedding::EmbeddingProvider;
use crate::recall_tags::TimeWindow;
use crate::services::memory_time_zone::refresh_memory_time_zone;
use crate::services::recall::query::{EmbeddingMemo, EmbeddingMemoMap};
use crate::services::recall::search::{
    search_memories, RecallSearchOptions, SearchMemoriesOptions,
};
use crate::services::speaker_names::{resolve_speaker_names, speaker_label, SpeakerNames, SpeakerRow};
use crate::tools::llm_number::llm_number;
use crate::tools::search::memory_archived_label;

/// The four tool names, in v4's `BRAHMA_LOOKUP_HANDLERS` order
/// (`tool-executor.ts:272-282`).
pub const BRAHMA_LOOKUP_TOOL_NAMES: [&str; 4] = [
    "lookup_character",
    "lookup_memories",
    "lookup_messages",
    "read_messages",
];

/// v4 `createServiceLogger('BrahmaLookup')` — the child bindings ride first.
macro_rules! brahma_lookup_line {
    ($level:ident, $($rest:tt)*) => {
        tracing::$level!(
            target: "quilltap::tools",
            service = "BrahmaLookup",
            module = "service",
            $($rest)*
        )
    };
}

/// Conversation types an operator means by "conversations".
const CONVERSATION_CHAT_TYPES: [&str; 2] = ["salon", "autonomous"];

/// Text budgets, in characters (UTF-16 units).
const CORE_FIELD_CHARS: usize = 3000;
const PHYSICAL_CHARS: usize = 1500;
const MEMORY_CHARS: usize = 700;
const MESSAGE_CHARS: usize = 2500;
const SNIPPET_CHARS: usize = 320;
const RECENT_CONVERSATIONS: usize = 8;
/// How many holders a "search everyone" memory query may visit.
const MAX_HOLDERS: usize = 60;
/// Over-fetch for message searches that are post-filtered by speaker or date.
const MESSAGE_OVERFETCH: i64 = 200;

/// v4 `BrahmaLookupResult`: `({ success: true } & Record<string, unknown>) |
/// { success: false; error }`.
#[derive(Debug, Clone, PartialEq)]
pub enum BrahmaLookupResult {
    /// The fields after `success`, in v4's key order.
    Success(Map<String, Value>),
    Failure(String),
}

impl Serialize for BrahmaLookupResult {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut out = Map::new();
        match self {
            BrahmaLookupResult::Success(fields) => {
                out.insert("success".into(), Value::Bool(true));
                for (k, v) in fields {
                    out.insert(k.clone(), v.clone());
                }
            }
            BrahmaLookupResult::Failure(error) => {
                out.insert("success".into(), Value::Bool(false));
                out.insert("error".into(), Value::String(error.clone()));
            }
        }
        Value::Object(out).serialize(s)
    }
}

/// What a lookup needs beyond the database: the operator's id (attribution
/// only — the databases are per-instance), the host zone fallback for the
/// memory calendar, and the clock.
pub struct BrahmaLookupContext<'a> {
    pub user_id: &'a str,
    pub memory_zone_fallback: &'a str,
    pub now_ms: f64,
}

/// The one error type inside a tool's catch: the BARE message v4's
/// `error.message` carries (a `DbError` renders through the `db::fallback`
/// bare-text rule, never `sqlite error: …`).
#[derive(Debug)]
struct LookupError(String);

impl From<DbError> for LookupError {
    fn from(e: DbError) -> Self {
        LookupError(crate::db::fallback::error_text(&e))
    }
}

impl std::fmt::Display for LookupError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

// ============================================================================
// Shared helpers
// ============================================================================

fn str_of<'v>(v: &'v Value, k: &str) -> Option<&'v str> {
    v.get(k).and_then(Value::as_str)
}

/// A JS-truthy string field (`undefined`, `null` and `""` are all falsy).
fn truthy_str<'v>(v: &'v Value, k: &str) -> Option<&'v str> {
    str_of(v, k).filter(|s| !s.is_empty())
}

/// `x ?? null` — the field's value, or `null` when absent or null.
fn or_null(v: &Value, k: &str) -> Value {
    match v.get(k) {
        None | Some(Value::Null) => Value::Null,
        Some(x) => x.clone(),
    }
}

/// Insert `k` only when the source carries it (`JSON.stringify` drops an
/// `undefined` property; a `null` is kept).
fn put_present(out: &mut Map<String, Value>, k: &str, v: Option<&Value>) {
    if let Some(v) = v {
        out.insert(k.into(), v.clone());
    }
}

/// v4 `clip(text, max)`: falsy → `null`; trimmed; empty → `null`; over `max`
/// UTF-16 units → the head plus `… [truncated, N chars in all]`.
pub fn clip(text: Option<&str>, max: usize) -> Option<String> {
    let text = text.filter(|t| !t.is_empty())?;
    let trimmed = js_trim(text);
    if trimmed.is_empty() {
        return None;
    }
    let len = utf16_len(trimmed);
    Some(if len > max {
        format!(
            "{}\u{2026} [truncated, {len} chars in all]",
            utf16_truncate(trimmed, max)
        )
    } else {
        trimmed.to_string()
    })
}

fn clip_value(text: Option<&str>, max: usize) -> Value {
    clip(text, max).map(Value::String).unwrap_or(Value::Null)
}

/// v4 `invalid(tool, issues)`: the FIRST issue's `path.join('.')` and message
/// (a root issue has an empty path, so the text reads `input:  — …` with two
/// spaces); no issue → `malformed arguments`.
fn invalid(tool: &str, issues: &[ZodIssue]) -> BrahmaLookupResult {
    let detail = match issues.first() {
        Some(issue) => {
            let path: Vec<String> = issue
                .path()
                .iter()
                .map(|p| match p {
                    Value::String(s) => s.clone(),
                    other => other.to_string(),
                })
                .collect();
            format!("{} \u{2014} {}", path.join("."), issue.message())
        }
        None => "malformed arguments".to_string(),
    };
    BrahmaLookupResult::Failure(format!("Invalid {tool} input: {detail}."))
}

/// v4 `CharacterMatch`.
#[derive(Debug, Clone, PartialEq)]
pub enum CharacterMatch {
    /// The hydrated character.
    Found(Value),
    None,
    /// `{ id, name }` of up to ten candidates.
    Ambiguous(Vec<(String, String)>),
}

/// `c.name` as JS reads it (`undefined` for a missing name; the raw rows
/// always carry one).
fn name_of(c: &Value) -> &str {
    str_of(c, "name").unwrap_or("")
}

fn aliases_of(c: &Value) -> Vec<&str> {
    c.get("aliases")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default()
}

/// v4 `resolveCharacterRef(ref)`. Exact id / name matches come from the
/// unhydrated rows (`findAllRaw` — aliases are not needed to match a name, and
/// hydrating every vault to resolve one would be wasted work); only when that
/// misses are the hydrated rows (`findAll`) consulted for aliases and partial
/// names. The winner is always returned hydrated via `findById`. A live
/// character is preferred over an archived namesake.
fn resolve_character_ref(db: &Db, reference: &str) -> Result<CharacterMatch, LookupError> {
    let needle = js_trim(reference).to_lowercase();
    if needle.is_empty() {
        return Ok(CharacterMatch::None);
    }
    let hydrate = |id: &str| -> Result<CharacterMatch, LookupError> {
        let id = id.to_string();
        let found = db.read_main(|main| {
            db.read_mount_index(|mount| {
                Ok(fallback::find_by_id_or_none("characters", &id, || {
                    characters_read::find_by_id(main, mount, &id)
                }))
            })
        })?;
        Ok(found.map_or(CharacterMatch::None, CharacterMatch::Found))
    };

    let raw = db.read_main(|c| Ok(characters_read::find_all_raw_or_empty(c)))?;
    if let Some(by_id) = raw
        .iter()
        .find(|c| str_of(c, "id").is_some_and(|id| id.to_lowercase() == needle))
    {
        return hydrate(str_of(by_id, "id").unwrap_or(""));
    }
    let by_name: Vec<&Value> = raw
        .iter()
        .filter(|c| js_trim(name_of(c)).to_lowercase() == needle)
        .collect();
    if by_name.len() == 1 {
        return hydrate(str_of(by_name[0], "id").unwrap_or(""));
    }

    let all: Vec<Value> = if by_name.len() > 1 {
        by_name.into_iter().cloned().collect()
    } else {
        db.read_main(|main| {
            db.read_mount_index(|mount| {
                Ok(fallback::find_all_or_empty("characters", || {
                    characters_read::find_all(main, mount)
                }))
            })
        })?
    };
    let exact: Vec<&Value> = all
        .iter()
        .filter(|c| {
            js_trim(name_of(c)).to_lowercase() == needle
                || aliases_of(c)
                    .iter()
                    .any(|a| js_trim(a).to_lowercase() == needle)
        })
        .collect();
    let pool: Vec<&Value> = if !exact.is_empty() {
        exact
    } else {
        // The substring arm does NOT trim (v4 `name.toLowerCase().includes`).
        all.iter()
            .filter(|c| {
                name_of(c).to_lowercase().contains(&needle)
                    || aliases_of(c)
                        .iter()
                        .any(|a| a.to_lowercase().contains(&needle))
            })
            .collect()
    };
    let live: Vec<&Value> = pool.iter().copied().filter(|c| !is_archived(c)).collect();
    let chosen = if !live.is_empty() { live } else { pool };
    match chosen.len() {
        1 => hydrate(str_of(chosen[0], "id").unwrap_or("")),
        0 => Ok(CharacterMatch::None),
        _ => Ok(CharacterMatch::Ambiguous(
            chosen
                .iter()
                .take(10)
                .map(|c| {
                    (
                        str_of(c, "id").unwrap_or("").to_string(),
                        name_of(c).to_string(),
                    )
                })
                .collect(),
        )),
    }
}

/// JS truthiness of a non-string field value (`archivedAt` is a string or
/// null in practice; a number / `true` would be truthy too).
fn is_truthy_non_string(v: &Value, k: &str) -> bool {
    match v.get(k) {
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|f| f != 0.0 && !f.is_nan()),
        Some(Value::Array(_)) | Some(Value::Object(_)) => true,
        _ => false,
    }
}

fn is_archived(c: &Value) -> bool {
    truthy_str(c, "archivedAt").is_some() || is_truthy_non_string(c, "archivedAt")
}

/// v4 `unresolved(ref, match)` — a resolution failure, phrased for the model
/// (`ref` is the RAW argument, untrimmed).
fn unresolved(reference: &str, m: &CharacterMatch) -> BrahmaLookupResult {
    if let CharacterMatch::Ambiguous(candidates) = m {
        let names: Vec<String> = candidates
            .iter()
            .map(|(id, name)| format!("{name} ({id})"))
            .collect();
        return BrahmaLookupResult::Failure(format!(
            "\"{reference}\" matches more than one character; pass one of these ids instead: {}.",
            names.join(", ")
        ));
    }
    BrahmaLookupResult::Failure(format!(
        "No character matches \"{reference}\". Try another spelling, an alias, or lookup_character."
    ))
}

/// v4 `resolveWindow(since?, until?)`: date-only bounds widen to whole LOCAL
/// days on the memory calendar (bug 212), matching the `search` tool; a full
/// timestamp passes through. The calendar is consulted only when either bound
/// is set (an empty string is falsy — absent).
fn resolve_window(
    db: &Db,
    ctx: &BrahmaLookupContext<'_>,
    since: Option<&str>,
    until: Option<&str>,
) -> Option<TimeWindow> {
    let since = since.filter(|s| !s.is_empty());
    let until = until.filter(|s| !s.is_empty());
    if since.is_none() && until.is_none() {
        return None;
    }
    let tz = refresh_memory_time_zone(db, ctx.memory_zone_fallback, ctx.now_ms);
    let from = match since {
        Some(s) if utf16_len(s) == 10 => zoned_day_bounds_iso(s, &tz)
            .map(|w| w.from)
            .unwrap_or_else(|| format!("{s}T00:00:00.000Z")),
        Some(s) => s.to_string(),
        None => "0000-01-01T00:00:00.000Z".to_string(),
    };
    let to = match until {
        Some(u) if utf16_len(u) == 10 => zoned_day_bounds_iso(u, &tz)
            .map(|w| w.to)
            .unwrap_or_else(|| format!("{u}T23:59:59.999Z")),
        Some(u) => u.to_string(),
        None => "9999-12-31T23:59:59.999Z".to_string(),
    };
    Some(TimeWindow { from, to })
}

/// v4 `loadChats(ids)` — load chats by id once each (first-seen order), for
/// titles and dates. A read failure logs a DEBUG and skips the chat (the one
/// per-read swallow).
fn load_chats(db: &Db, ids: &[String]) -> Vec<(String, Value)> {
    let mut out: Vec<(String, Value)> = Vec::new();
    let mut seen: Vec<&str> = Vec::new();
    for id in ids {
        if seen.contains(&id.as_str()) {
            continue;
        }
        seen.push(id);
        let cid = id.clone();
        // v4's `chats.findById` is itself a fallback read (MEASURED: a row whose
        // read throws answers `null`, the chat is simply absent), so the DEBUG
        // below fires in v5 only when the read POOL fails (an unavailable main
        // partition) — v4 has no such arm to reach.
        match db.read_main(move |c| Ok(chats_read::find_by_id_or_none(c, &cid))) {
            Ok(Some(chat)) => out.push((id.clone(), chat)),
            Ok(None) => {}
            Err(e) => {
                let error = crate::db::fallback::error_text(&e);
                brahma_lookup_line!(
                    debug,
                    chatId = %id,
                    error = %error,
                    "Chat lookup failed while labelling results"
                );
            }
        }
    }
    out
}

/// v4's repository reads, as the lookups see them — MEASURED at `624d0d25c`:
/// every read these tools make is a FALLBACK read (`safeQuery` with a fallback
/// value), so a broken table answers empty / zero / not-found and never
/// reaches the tool's catch. `memories.countByCharacterId` → 0.
fn count_memories(conn: &rusqlite::Connection, character_id: &str) -> Result<i64, DbError> {
    fallback::memories_count_by_character_or_zero(character_id, || {
        memories_read::count_by_character_id(conn, character_id)
    })
}

/// `chats.findByCharacterId` → `findByFilter`'s fallback `[]` (its own
/// rethrowing `safeQuery` is unreachable: the only throwing call inside it is
/// the already-swallowing `findByFilter`).
fn chats_by_character(conn: &rusqlite::Connection, character_id: &str) -> Vec<Value> {
    fallback::find_by_filter_or_empty("chats", || {
        chats_read::find_by_character_id(conn, character_id)
    })
}

fn chat_type_is_conversation(chat: &Value) -> bool {
    // `chat.chatType ?? 'salon'`.
    let t = match chat.get("chatType") {
        None | Some(Value::Null) => "salon",
        Some(Value::String(s)) => s.as_str(),
        Some(_) => "",
    };
    CONVERSATION_CHAT_TYPES.contains(&t)
}

/// v4 `conversationRef(chat, id)`.
fn conversation_ref(chat: Option<&Value>, id: Option<&str>) -> Value {
    let Some(id) = id.filter(|i| !i.is_empty()) else {
        return Value::Null;
    };
    let mut out = Map::new();
    out.insert("id".into(), Value::String(id.to_string()));
    match chat {
        Some(chat) => {
            // `chat.title || 'Untitled'` — JS truthiness.
            let title = truthy_str(chat, "title").unwrap_or("Untitled");
            out.insert("title".into(), Value::String(title.to_string()));
            put_present(&mut out, "createdAt", chat.get("createdAt"));
            put_present(&mut out, "chatType", chat.get("chatType"));
        }
        None => {
            out.insert("title".into(), Value::Null);
            out.insert(
                "note".into(),
                Value::String("conversation no longer exists".into()),
            );
        }
    }
    Value::Object(out)
}

/// JS `/\s/` — the ECMAScript WhiteSpace + LineTerminator set.
fn is_js_space(c: char) -> bool {
    matches!(
        c,
        '\u{9}'..='\u{D}'
            | ' '
            | '\u{A0}'
            | '\u{1680}'
            | '\u{2000}'..='\u{200A}'
            | '\u{2028}'
            | '\u{2029}'
            | '\u{202F}'
            | '\u{205F}'
            | '\u{3000}'
            | '\u{FEFF}'
    )
}

/// v4 `snippetAround(text, query)` — a window of `text` around the first query
/// word it contains. The index is found in the LOWERCASED text and applied to
/// the ORIGINAL (verbatim — a case mapping that changes UTF-16 length shifts
/// it), all in UTF-16 units.
pub fn snippet_around(text: &str, query: &str) -> String {
    let replaced: String = query
        .chars()
        .map(|c| {
            if matches!(c, '"' | '*' | '(' | ')') {
                ' '
            } else {
                c
            }
        })
        .collect();
    let words: Vec<String> = replaced
        .split(is_js_space)
        .map(|w| js_trim(w).to_lowercase())
        .filter(|w| utf16_len(w) > 1 && !["and", "or", "not", "near"].contains(&w.as_str()))
        .collect();
    let lower = text.to_lowercase();
    let mut at: Option<usize> = None;
    for w in &words {
        if let Some(i) = js_index_of(&lower, w, 0) {
            if at.is_none_or(|a| i < a) {
                at = Some(i);
            }
        }
    }
    let Some(at) = at else {
        return clip(Some(text), SNIPPET_CHARS).unwrap_or_default();
    };
    let len = utf16_len(text);
    let start = at.saturating_sub(SNIPPET_CHARS / 3);
    let end = len.min(start + SNIPPET_CHARS);
    let window = utf16_truncate(&utf16_slice_from(text, start), end - start);
    format!(
        "{}{}{}",
        if start > 0 { "\u{2026}" } else { "" },
        js_trim(&window),
        if end < len { "\u{2026}" } else { "" }
    )
}

/// A validated `llmNumber` int argument (the schema already accepted it).
fn int_arg(args: &Value, k: &str) -> Option<i64> {
    args.get(k)
        .map(|raw| llm_number(raw).into_owned())
        .and_then(|v| v.as_f64())
        .map(|f| f as i64)
}

/// A string argument as the parsed data carries it (validated already).
fn str_arg<'a>(args: &'a Value, k: &str) -> Option<&'a str> {
    args.get(k).and_then(Value::as_str)
}

/// v4's one catch: WARN `<tool> failed` + `<tool> failed: <message>`.
fn caught(tool: &str, user_id: &str, e: LookupError) -> BrahmaLookupResult {
    let message = e.0;
    match tool {
        "lookup_character" => {
            brahma_lookup_line!(warn, userId = %user_id, error = %message, "lookup_character failed")
        }
        "lookup_memories" => {
            brahma_lookup_line!(warn, userId = %user_id, error = %message, "lookup_memories failed")
        }
        "lookup_messages" => {
            brahma_lookup_line!(warn, userId = %user_id, error = %message, "lookup_messages failed")
        }
        _ => brahma_lookup_line!(warn, userId = %user_id, error = %message, "read_messages failed"),
    }
    BrahmaLookupResult::Failure(format!("{tool} failed: {message}"))
}

fn num(f: f64) -> Value {
    js_number_to_json(f)
}

// ============================================================================
// lookup_character
// ============================================================================

/// v4 `executeLookupCharacterTool(input, { userId })`.
pub fn execute_lookup_character_tool(
    db: &Db,
    args: &Value,
    ctx: &BrahmaLookupContext<'_>,
) -> BrahmaLookupResult {
    let issues = zod_lookup_character_issues(args);
    if !issues.is_empty() {
        return invalid("lookup_character", &issues);
    }
    match lookup_character_inner(db, args, ctx) {
        Ok(r) => r,
        Err(e) => caught("lookup_character", ctx.user_id, e),
    }
}

fn lookup_character_inner(
    db: &Db,
    args: &Value,
    ctx: &BrahmaLookupContext<'_>,
) -> Result<BrahmaLookupResult, LookupError> {
    let reference = str_arg(args, "character").unwrap_or("");
    let m = resolve_character_ref(db, reference)?;
    let CharacterMatch::Found(c) = m else {
        return Ok(unresolved(reference, &m));
    };
    let cid = str_of(&c, "id").unwrap_or("").to_string();

    let memory_count = {
        let cid = cid.clone();
        db.read_main(move |conn| count_memories(conn, &cid))?
    };
    let mut chats: Vec<Value> = {
        let cid = cid.clone();
        db.read_main(move |conn| Ok(chats_by_character(conn, &cid)))?
    }
    .into_iter()
    .filter(chat_type_is_conversation)
    .collect();
    // `(b.updatedAt ?? '').localeCompare(a.updatedAt ?? '')` — ICU collation.
    js_array_sort_by(&mut chats, |a, b| {
        let ua = str_of(a, "updatedAt").unwrap_or("");
        let ub = str_of(b, "updatedAt").unwrap_or("");
        locale_compare(ub, ua) as i32 as f64
    });
    // `chats.reduce((a, b) => (a.createdAt ?? '') <= (b.createdAt ?? '') ? a : b)`
    // over the SORTED list (ties keep the earlier).
    let first_chat = chats.iter().reduce(|a, b| {
        let ca = str_of(a, "createdAt").unwrap_or("");
        let cb = str_of(b, "createdAt").unwrap_or("");
        if js_str_le(ca, cb) {
            a
        } else {
            b
        }
    });

    brahma_lookup_line!(
        debug,
        userId = %ctx.user_id,
        characterId = %cid,
        memoryCount = memory_count,
        conversationCount = chats.len() as u64,
        "lookup_character resolved"
    );

    let mut out = Map::new();
    out.insert("id".into(), Value::String(cid.clone()));
    put_present(&mut out, "name", c.get("name"));
    out.insert(
        "aliases".into(),
        match c.get("aliases") {
            None | Some(Value::Null) => Value::Array(Vec::new()),
            Some(v) => v.clone(),
        },
    );
    out.insert("pronouns".into(), or_null(&c, "pronouns"));
    out.insert("title".into(), or_null(&c, "title"));
    put_present(&mut out, "controlledBy", c.get("controlledBy"));
    out.insert("archived".into(), Value::Bool(is_archived(&c)));
    put_present(&mut out, "createdAt", c.get("createdAt"));
    for k in ["identity", "description", "personality", "manifesto"] {
        out.insert(k.into(), clip_value(str_of(&c, k), CORE_FIELD_CHARS));
    }
    out.insert(
        "physicalDescription".into(),
        match c.get("physicalDescription") {
            Some(pd) if is_js_truthy(pd) => {
                // `pd.fullDescription ?? pd.completePrompt ?? pd.longPrompt ??
                // pd.mediumPrompt ?? pd.shortPrompt` — nullish, not falsy.
                let text = [
                    "fullDescription",
                    "completePrompt",
                    "longPrompt",
                    "mediumPrompt",
                    "shortPrompt",
                ]
                .iter()
                .find_map(|k| match pd.get(*k) {
                    None | Some(Value::Null) => None,
                    Some(v) => Some(v),
                });
                clip_value(text.and_then(Value::as_str), PHYSICAL_CHARS)
            }
            _ => Value::Null,
        },
    );
    out.insert(
        "vaultMountPointId".into(),
        or_null(&c, "characterDocumentMountPointId"),
    );
    out.insert("memoriesHeld".into(), num(memory_count as f64));
    let mut conversations = Map::new();
    conversations.insert("count".into(), num(chats.len() as f64));
    conversations.insert(
        "first".into(),
        first_chat
            .map(|fc| conversation_ref(Some(fc), str_of(fc, "id")))
            .unwrap_or(Value::Null),
    );
    conversations.insert(
        "mostRecent".into(),
        Value::Array(
            chats
                .iter()
                .take(RECENT_CONVERSATIONS)
                .map(|chat| {
                    let mut r = match conversation_ref(Some(chat), str_of(chat, "id")) {
                        Value::Object(m) => m,
                        _ => Map::new(),
                    };
                    put_present(&mut r, "updatedAt", chat.get("updatedAt"));
                    Value::Object(r)
                })
                .collect(),
        ),
    );
    out.insert("conversations".into(), Value::Object(conversations));
    out.insert(
        "hint".into(),
        Value::String(
            "For facts not in these fields (an age, a date, an event), use lookup_memories with character set to this one, then lookup_messages."
                .into(),
        ),
    );
    Ok(BrahmaLookupResult::Success(out))
}

/// JS `a <= b` on two strings: UTF-16 code-unit order.
fn js_str_le(a: &str, b: &str) -> bool {
    a.encode_utf16().cmp(b.encode_utf16()) != std::cmp::Ordering::Greater
}

/// JS truthiness of a JSON value.
fn is_js_truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|f| f != 0.0 && !f.is_nan()),
        Value::String(s) => !s.is_empty(),
        Value::Array(_) | Value::Object(_) => true,
    }
}

// ============================================================================
// lookup_memories
// ============================================================================

/// v4 `executeLookupMemoriesTool(input, { userId })`. Inspection only: no
/// access stamp is written.
pub async fn execute_lookup_memories_tool<E: EmbeddingProvider>(
    db: &Db,
    embedding: &E,
    args: &Value,
    ctx: &BrahmaLookupContext<'_>,
) -> BrahmaLookupResult {
    let issues = zod_lookup_memories_issues(args);
    if !issues.is_empty() {
        return invalid("lookup_memories", &issues);
    }
    match lookup_memories_inner(db, embedding, args, ctx).await {
        Ok(r) => r,
        Err(e) => caught("lookup_memories", ctx.user_id, e),
    }
}

async fn lookup_memories_inner<E: EmbeddingProvider>(
    db: &Db,
    embedding: &E,
    args: &Value,
    ctx: &BrahmaLookupContext<'_>,
) -> Result<BrahmaLookupResult, LookupError> {
    let query = str_arg(args, "query").unwrap_or("");
    let character = str_arg(args, "character").filter(|s| !s.is_empty());
    let about = str_arg(args, "about").filter(|s| !s.is_empty());
    let since = str_arg(args, "since");
    let until = str_arg(args, "until");
    let limit = int_arg(args, "limit").unwrap_or(10);

    // Holders: one named character, or every live character holding memories.
    let holders: Vec<(String, String)> = if let Some(reference) = character {
        let m = resolve_character_ref(db, reference)?;
        let CharacterMatch::Found(c) = m else {
            return Ok(unresolved(reference, &m));
        };
        vec![(
            str_of(&c, "id").unwrap_or("").to_string(),
            name_of(&c).to_string(),
        )]
    } else {
        let raw = db.read_main(|c| Ok(characters_read::find_all_raw_or_empty(c)))?;
        let mut holders = Vec::new();
        for c in raw.iter().filter(|c| !is_archived(c)) {
            if holders.len() >= MAX_HOLDERS {
                break;
            }
            let id = str_of(c, "id").unwrap_or("").to_string();
            let count = {
                let id = id.clone();
                db.read_main(move |conn| count_memories(conn, &id))?
            };
            if count > 0 {
                holders.push((id, name_of(c).to_string()));
            }
        }
        holders
    };

    let about_character_id = match about {
        Some(reference) => {
            let m = resolve_character_ref(db, reference)?;
            let CharacterMatch::Found(c) = m else {
                return Ok(unresolved(reference, &m));
            };
            Some(str_of(&c, "id").unwrap_or("").to_string())
        }
        None => None,
    };

    let window = resolve_window(db, ctx, since, until);
    // One embedding for the whole sweep, however many holders it visits.
    let embedding_memo: EmbeddingMemo =
        std::sync::Arc::new(std::sync::Mutex::new(EmbeddingMemoMap::default()));
    let options = SearchMemoriesOptions {
        search: RecallSearchOptions {
            limit: Some(limit.max(0) as usize),
            about_character_id: about_character_id.clone(),
            include_cold: true,
            literal_phrase: true,
            now_ms: ctx.now_ms,
            ..Default::default()
        },
        user_id: ctx.user_id.to_string(),
        window: window.clone(),
        embedding_memo: Some(embedding_memo),
        ..Default::default()
    };
    let mut hits: Vec<(crate::services::recall::types::RecallCandidate, String)> = Vec::new();
    for (holder_id, holder_name) in &holders {
        let found = search_memories(db, embedding, holder_id, query, &options).await?;
        for f in found {
            hits.push((f, holder_name.clone()));
        }
    }
    js_array_sort_by(&mut hits, |a, b| b.0.score - a.0.score);
    let top: Vec<_> = hits.iter().take(limit.max(0) as usize).collect();

    // Names for the about-subjects, titles for the source conversations (a
    // SECOND raw read — observable under a throwing plant).
    let name_by_id: Vec<(String, String)> = db
        .read_main(|c| Ok(characters_read::find_all_raw_or_empty(c)))?
        .iter()
        .map(|c| {
            (
                str_of(c, "id").unwrap_or("").to_string(),
                name_of(c).to_string(),
            )
        })
        .collect();
    let chat_ids: Vec<String> = top
        .iter()
        .filter_map(|(h, _)| truthy_str(&h.memory, "chatId").map(str::to_string))
        .collect();
    let chats = load_chats(db, &chat_ids);

    match &about_character_id {
        Some(about_id) => brahma_lookup_line!(
            debug,
            userId = %ctx.user_id,
            holders = holders.len() as u64,
            aboutCharacterId = %about_id,
            windowed = window.is_some(),
            hits = hits.len() as u64,
            returned = top.len() as u64,
            "lookup_memories completed"
        ),
        None => brahma_lookup_line!(
            debug,
            userId = %ctx.user_id,
            holders = holders.len() as u64,
            windowed = window.is_some(),
            hits = hits.len() as u64,
            returned = top.len() as u64,
            "lookup_memories completed"
        ),
    }

    let mut out = Map::new();
    out.insert("query".into(), Value::String(query.to_string()));
    out.insert(
        "searched".into(),
        Value::String(match character {
            Some(_) => holders[0].1.clone(),
            None => format!("{} characters", holders.len()),
        }),
    );
    out.insert("count".into(), num(top.len() as f64));
    let memories: Vec<Value> = top
        .iter()
        .map(|(h, holder_name)| {
            let m = &h.memory;
            let mut r = Map::new();
            put_present(&mut r, "memoryId", m.get("id"));
            r.insert("heldBy".into(), Value::String(holder_name.clone()));
            r.insert(
                "about".into(),
                match truthy_str(m, "aboutCharacterId") {
                    Some(aid) => Value::String(
                        name_by_id
                            .iter()
                            .find(|(id, _)| id == aid)
                            .map(|(_, n)| n.clone())
                            .unwrap_or_else(|| aid.to_string()),
                    ),
                    None => Value::Null,
                },
            );
            r.insert(
                "content".into(),
                clip_value(str_of(m, "content"), MEMORY_CHARS),
            );
            r.insert("happenedAt".into(), or_null(m, "occurredAt"));
            r.insert("narrativeTime".into(), or_null(m, "narrativeTime"));
            put_present(&mut r, "recordedAt", m.get("createdAt"));
            let importance = filter_importance(
                m.get("importance")
                    .and_then(Value::as_f64)
                    .unwrap_or(f64::NAN),
                m.get("reinforcedImportance").and_then(Value::as_f64),
            );
            r.insert(
                "importance".into(),
                num(math_round(importance * 100.0) / 100.0),
            );
            r.insert(
                "relevance".into(),
                num(math_round(h.score * 1000.0) / 1000.0),
            );
            put_present(&mut r, "source", m.get("source"));
            r.insert(
                "archived".into(),
                memory_archived_label(m)
                    .map(Value::String)
                    .unwrap_or(Value::Null),
            );
            let chat_id = truthy_str(m, "chatId");
            let chat =
                chat_id.and_then(|cid| chats.iter().find(|(id, _)| id == cid).map(|(_, c)| c));
            r.insert(
                "conversation".into(),
                conversation_ref(chat, str_of(m, "chatId")),
            );
            Value::Object(r)
        })
        .collect();
    out.insert("memories".into(), Value::Array(memories));
    Ok(BrahmaLookupResult::Success(out))
}

// ============================================================================
// lookup_messages
// ============================================================================

/// v4 `executeLookupMessagesTool(input, { userId })`.
pub fn execute_lookup_messages_tool(
    db: &Db,
    args: &Value,
    ctx: &BrahmaLookupContext<'_>,
) -> BrahmaLookupResult {
    let issues = zod_lookup_messages_issues(args);
    if !issues.is_empty() {
        return invalid("lookup_messages", &issues);
    }
    match lookup_messages_inner(db, args, ctx) {
        Ok(r) => r,
        Err(e) => caught("lookup_messages", ctx.user_id, e),
    }
}

fn participants_of(chat: &Value) -> &[Value] {
    chat.get("participants")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

fn lookup_messages_inner(
    db: &Db,
    args: &Value,
    ctx: &BrahmaLookupContext<'_>,
) -> Result<BrahmaLookupResult, LookupError> {
    let query = str_arg(args, "query").unwrap_or("");
    let character = str_arg(args, "character").filter(|s| !s.is_empty());
    let speaker = str_arg(args, "speaker").filter(|s| !s.is_empty());
    let since = str_arg(args, "since");
    let until = str_arg(args, "until");
    let limit = int_arg(args, "limit").unwrap_or(10);

    let mut chats: Vec<Value> = match character {
        Some(reference) => {
            let m = resolve_character_ref(db, reference)?;
            let CharacterMatch::Found(c) = m else {
                return Ok(unresolved(reference, &m));
            };
            let cid = str_of(&c, "id").unwrap_or("").to_string();
            db.read_main(move |conn| Ok(chats_by_character(conn, &cid)))?
        }
        None => db.read_main(|conn| {
            Ok(fallback::find_all_or_empty("chats", || {
                chats_read::find_all(conn)
            }))
        })?,
    };
    chats.retain(chat_type_is_conversation);

    let mut speaker_character_id: Option<String> = None;
    if let Some(reference) = speaker {
        let m = resolve_character_ref(db, reference)?;
        let CharacterMatch::Found(c) = m else {
            return Ok(unresolved(reference, &m));
        };
        let sid = str_of(&c, "id").unwrap_or("").to_string();
        chats.retain(|chat| {
            participants_of(chat)
                .iter()
                .any(|p| str_of(p, "characterId") == Some(sid.as_str()))
        });
        speaker_character_id = Some(sid);
    }

    if chats.is_empty() {
        let mut out = Map::new();
        out.insert("query".into(), Value::String(query.to_string()));
        out.insert("count".into(), num(0.0));
        out.insert("hits".into(), Value::Array(Vec::new()));
        out.insert(
            "note".into(),
            Value::String("No conversations match those filters.".into()),
        );
        return Ok(BrahmaLookupResult::Success(out));
    }

    let post_filtered = speaker_character_id.is_some()
        || since.is_some_and(|s| !s.is_empty())
        || until.is_some_and(|u| !u.is_empty());
    let chat_ids: Vec<String> = chats
        .iter()
        .map(|c| str_of(c, "id").unwrap_or("").to_string())
        .collect();
    let raw: Vec<Value> = {
        let ids = chat_ids.clone();
        let q = query.to_string();
        let n = if post_filtered {
            MESSAGE_OVERFETCH
        } else {
            limit
        };
        db.read_main(move |conn| {
            ChatSearchRepository::new(conn).search_messages_global(&ids, &q, n)
        })?
    };

    let window = resolve_window(db, ctx, since, until);
    let chat_by_id = |id: &str| chats.iter().find(|c| str_of(c, "id") == Some(id));

    let kept: Vec<&Value> = raw
        .iter()
        .filter(|hit| {
            if let (Some(w), Some(created)) = (&window, str_of(hit, "createdAt")) {
                // The string-compared window, verbatim (round §R.11).
                if created < w.from.as_str() || created > w.to.as_str() {
                    return false;
                }
            }
            if let Some(sid) = &speaker_character_id {
                let seat = str_of(hit, "chatId").and_then(chat_by_id).and_then(|chat| {
                    let pid = str_of(hit, "participantId")?;
                    participants_of(chat)
                        .iter()
                        .find(|p| str_of(p, "id") == Some(pid))
                });
                if seat.and_then(|s| str_of(s, "characterId")) != Some(sid.as_str()) {
                    return false;
                }
            }
            true
        })
        .take(limit.max(0) as usize)
        .collect();

    let mut names_by_chat: Vec<(String, SpeakerNames)> = Vec::new();
    let mut hits: Vec<Value> = Vec::new();
    for hit in kept {
        let hit_chat_id = str_of(hit, "chatId").unwrap_or("");
        let chat = chat_by_id(hit_chat_id);
        if let Some(known) = chat.filter(|_| !names_by_chat.iter().any(|(id, _)| id == hit_chat_id))
        {
            let participants = participants_of(known).to_vec();
            let names = db.read_main(move |conn| Ok(resolve_speaker_names(conn, &participants)))?;
            names_by_chat.push((hit_chat_id.to_string(), names));
        }
        let empty = SpeakerNames::default();
        let names = names_by_chat
            .iter()
            .find(|(id, _)| id == hit_chat_id)
            .map(|(_, n)| n)
            .unwrap_or(&empty);
        let mut r = Map::new();
        r.insert(
            "conversation".into(),
            conversation_ref(chat, str_of(hit, "chatId")),
        );
        put_present(&mut r, "messageId", hit.get("messageId"));
        put_present(&mut r, "writtenAt", hit.get("createdAt"));
        // `speakerLabel({ participantId: hit.participantId ?? undefined, role }, names)`.
        r.insert(
            "speaker".into(),
            Value::String(speaker_label(
                &SpeakerRow {
                    participant_id: str_of(hit, "participantId"),
                    role: str_of(hit, "role").unwrap_or(""),
                    ..SpeakerRow::default()
                },
                names,
            )),
        );
        r.insert(
            "snippet".into(),
            Value::String(snippet_around(str_of(hit, "content").unwrap_or(""), query)),
        );
        hits.push(Value::Object(r));
    }

    brahma_lookup_line!(
        debug,
        userId = %ctx.user_id,
        chatCount = chats.len() as u64,
        rawHits = raw.len() as u64,
        returned = hits.len() as u64,
        postFiltered = post_filtered,
        "lookup_messages completed"
    );

    let empty = hits.is_empty();
    let mut out = Map::new();
    out.insert("query".into(), Value::String(query.to_string()));
    out.insert("count".into(), num(hits.len() as f64));
    out.insert("hits".into(), Value::Array(hits));
    if empty {
        out.insert(
            "note".into(),
            Value::String(
                "No matches. Try fewer or different words, a word stem (e.g. \"birth\" finds \"birthday\"), or drop a filter."
                    .into(),
            ),
        );
    }
    Ok(BrahmaLookupResult::Success(out))
}

// ============================================================================
// read_messages
// ============================================================================

/// v4 `executeReadMessagesTool(input, { userId })`.
pub fn execute_read_messages_tool(
    db: &Db,
    args: &Value,
    ctx: &BrahmaLookupContext<'_>,
) -> BrahmaLookupResult {
    let issues = zod_read_messages_issues(args);
    if !issues.is_empty() {
        return invalid("read_messages", &issues);
    }
    match read_messages_inner(db, args, ctx) {
        Ok(r) => r,
        Err(e) => caught("read_messages", ctx.user_id, e),
    }
}

fn read_messages_inner(
    db: &Db,
    args: &Value,
    ctx: &BrahmaLookupContext<'_>,
) -> Result<BrahmaLookupResult, LookupError> {
    let conversation_id = str_arg(args, "conversation_id").unwrap_or("").to_string();
    let around = str_arg(args, "around_message_id").filter(|s| !s.is_empty());

    let chat = {
        let id = conversation_id.clone();
        db.read_main(move |conn| Ok(chats_read::find_by_id_or_none(conn, &id)))?
    };
    let Some(chat) = chat else {
        return Ok(BrahmaLookupResult::Failure(format!(
            "No conversation with id {conversation_id}."
        )));
    };

    let lines: Vec<Value> = {
        let id = conversation_id.clone();
        db.read_main(move |conn| chats_messages_read::get_messages(conn, &id))?
    }
    .into_iter()
    .filter(is_character_authored_message)
    .collect();
    let len = lines.len() as i64;
    let (start, end) = if let Some(around) = around {
        let Some(at) = lines.iter().position(|m| str_of(m, "id") == Some(around)) else {
            return Ok(BrahmaLookupResult::Failure(format!(
                "Message {around} is not a spoken line of this conversation (it may be a system notice, or belong to another conversation)."
            )));
        };
        let at = at as i64;
        let before = int_arg(args, "before").unwrap_or(3);
        let after = int_arg(args, "after").unwrap_or(3);
        ((at - before).max(0), len.min(at + after + 1))
    } else {
        let offset = int_arg(args, "offset").unwrap_or(0);
        let count = int_arg(args, "count").unwrap_or(10);
        let start = if offset < 0 {
            (len + offset).max(0)
        } else {
            offset.min(len)
        };
        (start, len.min(start + count))
    };

    let names = {
        let participants = participants_of(&chat).to_vec();
        db.read_main(move |conn| Ok(resolve_speaker_names(conn, &participants)))?
    };
    // `Array.from(new Set(names.values()))` — distinct, insertion order.
    let mut cast: Vec<String> = Vec::new();
    for (_, name) in names.entries() {
        if !cast.contains(name) {
            cast.push(name.clone());
        }
    }

    brahma_lookup_line!(
        debug,
        userId = %ctx.user_id,
        chatId = %conversation_id,
        totalLines = len,
        start = start,
        end = end,
        "read_messages completed"
    );

    let mut conversation = match conversation_ref(Some(&chat), str_of(&chat, "id")) {
        Value::Object(m) => m,
        _ => Map::new(),
    };
    put_present(&mut conversation, "updatedAt", chat.get("updatedAt"));
    conversation.insert(
        "cast".into(),
        Value::Array(cast.into_iter().map(Value::String).collect()),
    );
    conversation.insert("spokenLines".into(), num(len as f64));
    let mut range = Map::new();
    range.insert("from".into(), num(start as f64));
    range.insert("to".into(), num((end - 1) as f64));
    let messages: Vec<Value> = if end > start {
        lines[start as usize..end as usize]
            .iter()
            .enumerate()
            .map(|(i, m)| {
                let mut r = Map::new();
                r.insert("index".into(), num((start + i as i64) as f64));
                put_present(&mut r, "messageId", m.get("id"));
                r.insert(
                    "speaker".into(),
                    // v4 `brahma-lookup-handler.ts:538` passes the WHOLE message
                    // (`speakerLabel(m, names)`), so an established fact there is
                    // labelled `Established fact` (P4.D372's R2 widening).
                    Value::String(speaker_label(&SpeakerRow::from_value(m), &names)),
                );
                put_present(&mut r, "writtenAt", m.get("createdAt"));
                r.insert(
                    "content".into(),
                    Value::String(clip(str_of(m, "content"), MESSAGE_CHARS).unwrap_or_default()),
                );
                Value::Object(r)
            })
            .collect()
    } else {
        Vec::new()
    };

    let mut out = Map::new();
    out.insert("conversation".into(), Value::Object(conversation));
    out.insert("range".into(), Value::Object(range));
    out.insert("messages".into(), Value::Array(messages));
    Ok(BrahmaLookupResult::Success(out))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clip_arms() {
        assert_eq!(clip(None, 5), None);
        assert_eq!(clip(Some(""), 5), None);
        assert_eq!(clip(Some("   "), 5), None);
        assert_eq!(clip(Some("  abc  "), 5), Some("abc".into()));
        assert_eq!(
            clip(Some("abcdefgh"), 5),
            Some("abcde\u{2026} [truncated, 8 chars in all]".into())
        );
    }

    #[test]
    fn invalid_reads_the_first_issue() {
        let r = invalid(
            "lookup_character",
            &zod_lookup_character_issues(&serde_json::json!({})),
        );
        assert_eq!(
            r,
            BrahmaLookupResult::Failure(
                "Invalid lookup_character input: character \u{2014} Invalid input: expected string, received undefined.".into()
            )
        );
        let root = invalid(
            "read_messages",
            &zod_read_messages_issues(&serde_json::json!(5)),
        );
        assert_eq!(
            root,
            BrahmaLookupResult::Failure(
                "Invalid read_messages input:  \u{2014} Invalid input: expected object, received number.".into()
            )
        );
        assert_eq!(
            invalid("x", &[]),
            BrahmaLookupResult::Failure("Invalid x input: malformed arguments.".into())
        );
    }

    #[test]
    fn conversation_ref_arms() {
        assert_eq!(conversation_ref(None, None), Value::Null);
        assert_eq!(
            serde_json::to_string(&conversation_ref(None, Some("c1"))).unwrap(),
            r#"{"id":"c1","title":null,"note":"conversation no longer exists"}"#
        );
        let chat = serde_json::json!({ "title": "", "createdAt": "t", "chatType": "salon" });
        assert_eq!(
            serde_json::to_string(&conversation_ref(Some(&chat), Some("c1"))).unwrap(),
            r#"{"id":"c1","title":"Untitled","createdAt":"t","chatType":"salon"}"#
        );
    }

    #[test]
    fn snippet_around_arms() {
        // No query word found → the clipped text.
        assert_eq!(snippet_around("  hello  ", "zz"), "hello");
        // Stop words and one-letter words are ignored.
        assert_eq!(snippet_around("a and b", "and a"), "a and b");
        let long = format!("{}needle{}", "x".repeat(200), "y".repeat(400));
        let s = snippet_around(&long, "\"Needle\"");
        assert!(s.starts_with('\u{2026}') && s.ends_with('\u{2026}'));
        assert!(s.contains("needle"));
        assert_eq!(utf16_len(&s), SNIPPET_CHARS + 2);
    }

    #[test]
    fn result_serializes_success_first() {
        let mut m = Map::new();
        m.insert("a".into(), Value::from(1));
        assert_eq!(
            serde_json::to_string(&BrahmaLookupResult::Success(m)).unwrap(),
            r#"{"success":true,"a":1}"#
        );
        assert_eq!(
            serde_json::to_string(&BrahmaLookupResult::Failure("e".into())).unwrap(),
            r#"{"success":false,"error":"e"}"#
        );
    }
}
