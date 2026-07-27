//! Chat transcript export as SillyTavern JSONL (P4.9E3B) — v4's
//! `?action=export` arm on the chat GET handler
//! (`app/api/v1/chats/[id]/handlers/get.ts:46–127`) over
//! `exportSTChatAsJSONL` (`lib/sillytavern/chat.ts:222`).
//!
//! The JSONL bytes are the wire payload (a byte download at the quilltap-web
//! edge: `Content-Type: application/x-ndjson` + the attachment filename), so
//! every line is serialized in v4's exact object-literal key order and
//! `JSON.stringify` number formatting. Pinned byte-for-byte by
//! `chat_export_equivalence`.
//!
//! ## The swipe-group emit check is order-sensitive — deliberately
//!
//! v4 emits a swipe group when it meets the message equal to `groupMessages[0]`
//! — but it also SORTS `groupMessages` in place by `swipeIndex` at that moment.
//! For rows inserted in swipe-index order (v4's own writer always does) the
//! sorted head equals the iteration head and the group emits exactly once; rows
//! inserted out of order could re-match a later message and emit the group
//! twice. The port reproduces the algorithm literally, mutation and all —
//! do not "fix" it.

use serde_json::{Map, Value};

use crate::clock::iso_to_ms;
use crate::db::runtime::Db;
use crate::db::{characters_read, chats_messages_read, chats_read, users, DbError};

use crate::api::types::{ErrorKind, Response};

fn not_found(resource: &str) -> Response {
    Response::error(ErrorKind::NotFound, format!("{resource} not found"))
}

fn read_main_mount<T, F>(db: &Db, f: F) -> Result<T, DbError>
where
    F: FnOnce(&rusqlite::Connection, &rusqlite::Connection) -> Result<T, DbError>,
{
    db.read_main(|main| db.read_mount_index(|mount| f(main, mount)))
}

/// `new Date(iso).getTime()` — millisecond epoch, `NaN`-free for the ISO strings
/// the repos store. An unparseable stamp falls to 0 (never reached over repo
/// rows; v4 would emit `null` after `JSON.stringify(NaN)` — the fixture pins
/// only parseable stamps).
fn ms(iso: &str) -> i64 {
    iso_to_ms(iso).unwrap_or(0)
}

fn s<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.get(key).and_then(Value::as_str)
}

/// v4 `resolveName` — the participant map first, then the role fallback.
fn resolve_name<'a>(
    msg: &'a Value,
    participant_names: &'a std::collections::HashMap<String, String>,
    character_name: &'a str,
    user_name: &'a str,
) -> &'a str {
    if let Some(pid) = s(msg, "participantId") {
        if let Some(name) = participant_names.get(pid) {
            return name;
        }
    }
    if s(msg, "role") == Some("USER") {
        user_name
    } else {
        character_name
    }
}

/// One ST message line in v4's object-literal key order. `extra` is
/// `msg.rawResponse || undefined` — a falsy rawResponse omits the key.
#[allow(clippy::too_many_arguments)]
fn st_message(
    name: &str,
    is_user: bool,
    send_date: i64,
    mes: &str,
    swipes: Option<Vec<Value>>,
    swipe_id: Option<i64>,
    raw_response: Option<&Value>,
) -> Value {
    let mut m = Map::new();
    m.insert("name".into(), Value::String(name.to_string()));
    m.insert("is_user".into(), Value::Bool(is_user));
    m.insert("is_name".into(), Value::Bool(true));
    m.insert("send_date".into(), Value::Number(send_date.into()));
    m.insert("mes".into(), Value::String(mes.to_string()));
    if let Some(sw) = swipes {
        m.insert("swipes".into(), Value::Array(sw));
    }
    if let Some(id) = swipe_id {
        m.insert("swipe_id".into(), Value::Number(id.into()));
    }
    if let Some(raw) = raw_response {
        // JS truthiness: null / absent → the key is dropped by JSON.stringify.
        if !raw.is_null() {
            m.insert("extra".into(), raw.clone());
        }
    }
    Value::Object(m)
}

/// v4 `exportSTChatAsJSONL` over the handler's formatted messages. `messages`
/// are the type-'message' events in transcript order.
fn export_st_chat_as_jsonl(
    chat: &Value,
    messages: &[Value],
    character_name: &str,
    user_name: &str,
    participant_names: &std::collections::HashMap<String, String>,
) -> String {
    // Group by swipeGroupId (built over the SYSTEM-filtered list, v4's first pass).
    let visible: Vec<&Value> = messages
        .iter()
        .filter(|m| s(m, "role") != Some("SYSTEM"))
        .collect();
    let mut groups: std::collections::HashMap<String, Vec<&Value>> =
        std::collections::HashMap::new();
    for msg in &visible {
        if let Some(sg) = s(msg, "swipeGroupId") {
            groups.entry(sg.to_string()).or_default().push(msg);
        }
    }

    let mut st_messages: Vec<Value> = Vec::new();
    for msg in &visible {
        if let Some(sg) = s(msg, "swipeGroupId") {
            let group = groups.get_mut(sg).expect("group built above");
            // v4: emit only when this message IS the group head — checked
            // against the group's CURRENT order, then sort in place (see the
            // module header).
            if s(group[0], "id") != s(msg, "id") {
                continue;
            }
            // Stable sort by swipeIndex (V8's stable sort; a null index
            // compares Equal, i.e. the NaN-comparator no-op).
            group.sort_by(|a, b| {
                let ai = a.get("swipeIndex").and_then(Value::as_f64);
                let bi = b.get("swipeIndex").and_then(Value::as_f64);
                match (ai, bi) {
                    (Some(x), Some(y)) => x.partial_cmp(&y).unwrap_or(std::cmp::Ordering::Equal),
                    _ => std::cmp::Ordering::Equal,
                }
            });
            let swipes: Vec<Value> = group
                .iter()
                .map(|m| Value::String(s(m, "content").unwrap_or_default().to_string()))
                .collect();
            // findIndex on STRICT equality — null === null matches too.
            let want = msg.get("swipeIndex").cloned().unwrap_or(Value::Null);
            let current = group
                .iter()
                .position(|m| m.get("swipeIndex").cloned().unwrap_or(Value::Null) == want);
            let pick = current.unwrap_or(0);
            st_messages.push(st_message(
                resolve_name(msg, participant_names, character_name, user_name),
                s(msg, "role") == Some("USER"),
                ms(s(msg, "createdAt").unwrap_or_default()),
                s(group[pick], "content").unwrap_or_default(),
                Some(swipes),
                Some(current.unwrap_or(0) as i64),
                msg.get("rawResponse"),
            ));
        } else {
            st_messages.push(st_message(
                resolve_name(msg, participant_names, character_name, user_name),
                s(msg, "role") == Some("USER"),
                ms(s(msg, "createdAt").unwrap_or_default()),
                s(msg, "content").unwrap_or_default(),
                None,
                None,
                msg.get("rawResponse"),
            ));
        }
    }

    // Header line: user_name, character_name, create_date, chat_metadata.
    let mut header = Map::new();
    header.insert("user_name".into(), Value::String(user_name.to_string()));
    header.insert(
        "character_name".into(),
        Value::String(character_name.to_string()),
    );
    header.insert(
        "create_date".into(),
        Value::Number(ms(s(chat, "createdAt").unwrap_or_default()).into()),
    );
    let metadata = match chat.get("sillyTavernMetadata") {
        Some(v) if !v.is_null() => v.clone(),
        _ => Value::Object(Map::new()),
    };
    header.insert("chat_metadata".into(), metadata);

    let mut lines: Vec<String> = Vec::with_capacity(1 + st_messages.len());
    lines.push(serde_json::to_string(&Value::Object(header)).unwrap_or_default());
    for m in &st_messages {
        lines.push(serde_json::to_string(m).unwrap_or_default());
    }
    lines.join("\n")
}

/// v4's `?action=export` handler. `user_id` selects the operator row (the
/// export's `userName` = `user.name || 'User'`).
pub fn chat_export(db: &Db, user_id: &str, chat_id: &str) -> Response {
    let cid = chat_id.to_string();
    let uid = user_id.to_string();
    let out: Result<Result<Response, Response>, DbError> =
        read_main_mount(db, move |main, mount| {
            let Some(chat) = chats_read::find_by_id(main, &cid)? else {
                return Ok(Err(not_found("Chat")));
            };

            let all_events = chats_messages_read::get_messages(main, &cid)?;
            let messages: Vec<Value> = all_events
                .into_iter()
                .filter(|e| e.get("type").and_then(Value::as_str) == Some("message"))
                .collect();

            let participants: Vec<&Value> = chat
                .get("participants")
                .and_then(Value::as_array)
                .map(|a| a.iter().collect())
                .unwrap_or_default();
            let character_participants: Vec<&Value> = participants
                .iter()
                .copied()
                .filter(|p| {
                    p.get("type").and_then(Value::as_str) == Some("CHARACTER")
                        && s(p, "characterId").is_some_and(|c| !c.is_empty())
                })
                .collect();
            let Some(primary) = character_participants.first() else {
                return Ok(Err(not_found("No character in chat")));
            };
            let primary_character_id = s(primary, "characterId").unwrap_or_default().to_string();

            // Load every character participant; broken vaults are dropped by
            // findByIds and fall back to the primary name in the export.
            let ids: Vec<String> = character_participants
                .iter()
                .filter_map(|p| s(p, "characterId").map(str::to_string))
                .collect();
            let characters = characters_read::find_by_ids(main, mount, &ids)?;
            let mut characters_by_id: std::collections::HashMap<String, &Value> =
                std::collections::HashMap::new();
            for c in &characters {
                if let Some(id) = s(c, "id") {
                    characters_by_id.insert(id.to_string(), c);
                }
            }
            let mut participant_names: std::collections::HashMap<String, String> =
                std::collections::HashMap::new();
            for p in &character_participants {
                let name = s(p, "characterId")
                    .and_then(|cid| characters_by_id.get(cid))
                    .and_then(|c| s(c, "name"));
                if let (Some(pid), Some(name)) = (s(p, "id"), name) {
                    participant_names.insert(pid.to_string(), name.to_string());
                }
            }

            let Some(primary_character) = characters_by_id.get(primary_character_id.as_str())
            else {
                return Ok(Err(not_found("Character")));
            };
            let character_name = s(primary_character, "name").unwrap_or_default().to_string();

            let user_name = users::find_profile_by_id(main, &uid)?
                .and_then(|u| u.name)
                .filter(|n| !n.is_empty())
                .unwrap_or_else(|| "User".to_string());

            let jsonl = export_st_chat_as_jsonl(
                &chat,
                &messages,
                &character_name,
                &user_name,
                &participant_names,
            );
            let filename = format!(
                "{character_name}_chat_{}.jsonl",
                ms(s(&chat, "createdAt").unwrap_or_default())
            );
            Ok(Ok(Response::ChatExportPayload { filename, jsonl }))
        });
    match out {
        Ok(Ok(r)) => r,
        Ok(Err(r)) => r,
        // v4's try/catch → serverError.
        Err(_) => Response::error(ErrorKind::Internal, "Failed to export chat"),
    }
}
