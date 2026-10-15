//! Fresh message ids for a chat imported under a fresh chat id — P4.D294,
//! porting v4 `lib/import/quilltap-import/message-id-remap.ts` (`8050958be`,
//! bug 228; widened by `c2795733f` / `ac6348c5f` / `64743ee13` to all three
//! extraction-cursor keys) at the `83334b9a5` pin.
//!
//! v4's module doc, verbatim:
//!
//! > `chat_messages.id` is a global primary key, not a per-chat one. A chat the
//! > importer creates under a new id (the `duplicate` conflict strategy, or any
//! > import without `preserveIds`) therefore cannot reuse its bundle's message
//! > ids whenever those messages may already exist — the original chat beside
//! > its duplicate, or a second import of the same bundle (bug 228). So every
//! > message is reminted, and every reference *to* a message inside the chat
//! > row or the message rows follows it through one old → new map:
//! >
//! > - `message.swipeGroupId` — `swipe-<messageId>` (what regenerate-swipe
//! >   writes) is rewritten; any other key (SillyTavern's `swipe-<index>`) is an
//! >   opaque, per-chat grouping key and is kept.
//! > - `chat.summaryAnchorMessageIds` — if any anchor names no message in the
//! >   bundle, the running summary is cleared with its anchors and fold cursor,
//! >   exactly as `?action=rebuild-summary` does, so the ordinary cadence
//! >   refolds rather than trusting a summary with a partial anchor set.
//! > - `chat.otherExtractionWatermarkMessageId` /
//! >   `chat.episodeExtractionWatermarkMessageId` /
//! >   `chat.turnExtractionWatermarkMessageId` (the bundle's carriage of the
//! >   extraction cursors) — nulled when unresolvable, which simply lets the
//! >   next pass catch up (a reopened turn then reads from its opener).
//! > - `chat.courierCheckpoints[*].lastResolvedMessageId` — a checkpoint whose
//! >   message is gone (or that cannot be read) is dropped, so the next Courier
//! >   turn sends full context.
//! >
//! > Sidecars outside the chat row (informs, annotations, memories, file links)
//! > resolve through `remapImportedMessageId`, keyed by their source chat.
//!
//! JSON-level (the bundle chat and its messages are the raw `Value`s the import
//! decodes later), pure, and minted through an injected `mint` so the tier-1
//! family (`message_id_remap_equivalence`) replays v4's real function with a
//! counter. Production mints `uuid::Uuid::new_v4()`.
//!
//! Recorded edges v4's TypeScript types rule out and v5 does not chase: a
//! truthy NON-STRING message `id` (v4 would key its `Map` by the number) is
//! not mapped; a truthy non-string `swipeGroupId` (v4's `startsWith` would
//! throw) is kept; a non-object message is reminted as an object holding only
//! its new `id`.

use std::collections::HashMap;

use serde_json::{Map, Value};

use crate::db::memory_extraction_cursors::ExtractionCursorPass;

/// v4 `IdMappingState.messages` (`types.ts:240`): "Source chat id → (source
/// message id → the id the chat import minted for it). Present only for chats
/// created under a fresh id … an entry is recorded only once its message has
/// actually been inserted."
pub type MessageIdMaps = HashMap<String, HashMap<String, String>>;

const SWIPE_PREFIX: &str = "swipe-";

/// v4 `RemintedChatMessages`.
#[derive(Debug, Clone, PartialEq)]
pub struct RemintedChatMessages {
    /// "The bundle's messages, each under a fresh id. Order is preserved."
    pub messages: Vec<Value>,
    /// "Chat-row fields to spread over the chat before `create`. Only fields
    /// present on the source chat appear." (A cleared summary also writes
    /// `contextSummary` and `lastSummaryTurn`, present or not — v4's patch.)
    /// In v4's write order: `contextSummary` / `summaryAnchorMessageIds` /
    /// `lastSummaryTurn` (or the anchors alone), the three cursor keys
    /// (other → episode → turn), `courierCheckpoints`.
    pub chat_patch: Map<String, Value>,
    /// "Source message id → new message id, for remapping references. A
    /// source id that occurs twice in one bundle chat maps to its first
    /// occurrence; every occurrence still gets its own new id in `messages`."
    /// Insertion-ordered (v4's `Map`).
    pub id_map: Vec<(String, String)>,
    /// "Message references in the chat row that named no message in the
    /// bundle."
    pub dropped_refs: usize,
    /// "Whether the running summary was cleared because an anchor did not
    /// resolve."
    pub summary_cleared: bool,
}

/// V8's own-key order for a plain object: every canonical array-index key
/// first, ascending, then the rest in insertion order — what iterating a
/// parsed JS object (and spreading it) visits.
fn v8_ordered(map: Map<String, Value>) -> Map<String, Value> {
    let index = |k: &str| -> Option<u32> {
        let n: u32 = k.parse().ok()?;
        (n != u32::MAX && k == n.to_string()).then_some(n)
    };
    if map.keys().all(|k| index(k).is_none()) {
        return map;
    }
    let mut indexed: Vec<(u32, String)> = map
        .keys()
        .filter_map(|k| index(k).map(|n| (n, k.clone())))
        .collect();
    indexed.sort_by_key(|(n, _)| *n);
    let mut out = Map::new();
    for (_, k) in &indexed {
        out.insert(k.clone(), map[k].clone());
    }
    for (k, v) in map {
        if index(&k).is_none() {
            out.insert(k, v);
        }
    }
    out
}

/// v4 `readJsonObject` — "A bundle may carry a JSON column as its serialized
/// string; read either." An object (never an array), or `None`.
fn read_json_object(value: &Value) -> Option<Map<String, Value>> {
    let parsed = match value {
        Value::String(s) => serde_json::from_str::<Value>(s).ok()?,
        other => other.clone(),
    };
    match parsed {
        Value::Object(map) => Some(v8_ordered(map)),
        _ => None,
    }
}

fn js_truthy(v: Option<&Value>) -> bool {
    crate::api::system_qtap::js_truthy(v)
}

/// v4 `remapSwipeGroupId`: only `swipe-<id>` whose `<id>` is a reminted
/// message is rewritten.
fn remap_swipe_group_id(group_id: &str, lookup: &HashMap<&str, &str>) -> String {
    let Some(rest) = group_id.strip_prefix(SWIPE_PREFIX) else {
        return group_id.to_string();
    };
    match lookup.get(rest) {
        Some(mapped) if !mapped.is_empty() => format!("{SWIPE_PREFIX}{mapped}"),
        _ => group_id.to_string(),
    }
}

/// v4 `remintChatMessageIds(chat, messages, mintId)`: "Remint a chat's
/// message ids and rewrite the chat's own message references. Pure: mints ids
/// with `mintId` … and never touches the database."
pub fn remint_chat_message_ids(
    chat: &Map<String, Value>,
    messages: &[Value],
    mut mint: impl FnMut() -> String,
) -> RemintedChatMessages {
    let mut id_map: Vec<(String, String)> = Vec::new();
    let new_ids: Vec<String> = messages
        .iter()
        .map(|message| {
            let new_id = mint();
            if let Some(Value::String(id)) = message.get("id") {
                if !id.is_empty() && !id_map.iter().any(|(k, _)| k == id) {
                    id_map.push((id.clone(), new_id.clone()));
                }
            }
            new_id
        })
        .collect();
    let lookup: HashMap<&str, &str> = id_map
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();

    let reminted: Vec<Value> = messages
        .iter()
        .zip(&new_ids)
        .map(|(message, new_id)| {
            let mut out = message.as_object().cloned().unwrap_or_default();
            out.insert("id".to_string(), Value::String(new_id.clone()));
            if let Some(Value::String(group)) = message.get("swipeGroupId") {
                if !group.is_empty() {
                    out.insert(
                        "swipeGroupId".to_string(),
                        Value::String(remap_swipe_group_id(group, &lookup)),
                    );
                }
            }
            Value::Object(out)
        })
        .collect();

    let mut chat_patch = Map::new();
    let mut dropped_refs = 0usize;
    let mut summary_cleared = false;

    if let Some(Value::Array(anchors)) = chat.get("summaryAnchorMessageIds") {
        let mapped: Vec<Option<&str>> = anchors
            .iter()
            .map(|a| a.as_str().and_then(|id| lookup.get(id).copied()))
            .collect();
        let unresolved = mapped
            .iter()
            .filter(|m| m.is_none_or(str::is_empty))
            .count();
        if unresolved > 0 {
            dropped_refs += unresolved;
            summary_cleared = true;
            chat_patch.insert("contextSummary".into(), Value::Null);
            chat_patch.insert("summaryAnchorMessageIds".into(), Value::Array(Vec::new()));
            chat_patch.insert("lastSummaryTurn".into(), Value::from(0));
        } else {
            chat_patch.insert(
                "summaryAnchorMessageIds".into(),
                Value::Array(
                    mapped
                        .into_iter()
                        .map(|m| Value::String(m.unwrap_or_default().to_string()))
                        .collect(),
                ),
            );
        }
    }

    for pass in ExtractionCursorPass::ALL {
        let field = pass.bundle_field();
        let watermark = chat.get(field);
        if !js_truthy(watermark) {
            continue;
        }
        let mapped = watermark
            .and_then(Value::as_str)
            .and_then(|w| lookup.get(w).copied())
            .filter(|m| !m.is_empty());
        if mapped.is_none() {
            dropped_refs += 1;
        }
        chat_patch.insert(
            field.to_string(),
            mapped.map_or(Value::Null, |m| Value::String(m.to_string())),
        );
    }

    if let Some(raw_checkpoints) = chat.get("courierCheckpoints").filter(|v| !v.is_null()) {
        let checkpoints = read_json_object(raw_checkpoints);
        let mut next = Map::new();
        for (character_id, raw) in checkpoints.iter().flatten() {
            let checkpoint = read_json_object(raw);
            let mapped = checkpoint
                .as_ref()
                .and_then(|c| c.get("lastResolvedMessageId"))
                .and_then(Value::as_str)
                .and_then(|id| lookup.get(id).copied())
                .filter(|m| !m.is_empty());
            match (checkpoint, mapped) {
                (Some(mut checkpoint), Some(mapped)) => {
                    checkpoint.insert(
                        "lastResolvedMessageId".into(),
                        Value::String(mapped.to_string()),
                    );
                    next.insert(character_id.clone(), Value::Object(checkpoint));
                }
                _ => dropped_refs += 1,
            }
        }
        if checkpoints.is_none() {
            dropped_refs += 1;
        }
        chat_patch.insert(
            "courierCheckpoints".into(),
            if next.is_empty() {
                Value::Null
            } else {
                Value::Object(next)
            },
        );
    }

    RemintedChatMessages {
        messages: reminted,
        chat_patch,
        id_map,
        dropped_refs,
        summary_cleared,
    }
}

/// The production minter (v4's default `randomUUID`).
pub fn mint_message_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// v4 `remapImportedMessageId(idMaps, sourceChatId, messageId)`: "Resolve a
/// sidecar's message reference against the chat it belongs to.
///
/// - The source chat was not reminted (kept its ids, was skipped, or is not in
///   this import): the id is returned unchanged.
/// - The source chat was reminted and the message landed: its new id.
/// - The source chat was reminted and the message did not land (absent from
///   the bundle, or its insert failed): `null` — there is nothing to point at."
///
/// `source_chat_id` is the sidecar's SOURCE chat id; `None` / `""` (v4's falsy
/// check) is "no per-chat map".
pub fn remap_imported_message_id(
    maps: &MessageIdMaps,
    source_chat_id: Option<&str>,
    message_id: &str,
) -> Option<String> {
    let chat_map = source_chat_id
        .filter(|c| !c.is_empty())
        .and_then(|c| maps.get(c));
    match chat_map {
        None => Some(message_id.to_string()),
        Some(chat_map) => chat_map.get(message_id).cloned(),
    }
}

#[cfg(test)]
mod tests {
    //! v4 `duplicate-chat-message-ids.test.ts`'s six `remintChatMessageIds`
    //! cases, mirrored by name (the whole corpus is the tier-1 family's).
    use super::*;
    use serde_json::json;

    const USER_MSG: &str = "d1000000-0000-4000-8000-000000000001";
    const REPLY_A: &str = "d1000000-0000-4000-8000-000000000002";
    const REPLY_B: &str = "d1000000-0000-4000-8000-000000000003";

    fn obj(v: Value) -> Map<String, Value> {
        v.as_object().unwrap().clone()
    }

    fn counter(prefix: &'static str) -> impl FnMut() -> String {
        let mut n = 0;
        move || {
            n += 1;
            format!("{prefix}-{n}")
        }
    }

    #[test]
    fn keeps_an_opaque_swipe_group_key_that_names_no_message() {
        let r = remint_chat_message_ids(
            &Map::new(),
            &[json!({"id": REPLY_A, "swipeGroupId": "swipe-3"})],
            counter("new"),
        );
        assert_eq!(r.messages[0]["id"], "new-1");
        assert_eq!(r.messages[0]["swipeGroupId"], "swipe-3");
    }

    #[test]
    fn gives_every_occurrence_of_a_repeated_id_its_own_new_id_mapping_references_to_the_first() {
        let r = remint_chat_message_ids(
            &obj(json!({"summaryAnchorMessageIds": [USER_MSG]})),
            &[json!({"id": USER_MSG}), json!({"id": USER_MSG})],
            counter("new"),
        );
        let ids: Vec<&str> = r
            .messages
            .iter()
            .map(|m| m["id"].as_str().unwrap())
            .collect();
        assert_eq!(ids, ["new-1", "new-2"]);
        assert_eq!(r.chat_patch["summaryAnchorMessageIds"], json!(["new-1"]));
    }

    #[test]
    fn keeps_a_bare_swipe_group_key_even_when_it_equals_a_message_id() {
        let r = remint_chat_message_ids(
            &Map::new(),
            &[json!({"id": REPLY_A, "swipeGroupId": REPLY_A})],
            || "new".to_string(),
        );
        assert_eq!(r.messages[0]["swipeGroupId"], REPLY_A);
    }

    #[test]
    fn reads_courier_checkpoints_carried_as_a_json_string() {
        let carried =
            json!({"c": {"lastResolvedMessageId": REPLY_B, "resolvedAt": "x"}}).to_string();
        let r = remint_chat_message_ids(
            &obj(json!({ "courierCheckpoints": carried })),
            &[json!({"id": REPLY_B})],
            || "new-b".to_string(),
        );
        assert_eq!(
            r.chat_patch["courierCheckpoints"],
            json!({"c": {"lastResolvedMessageId": "new-b", "resolvedAt": "x"}})
        );
    }

    #[test]
    fn drops_courier_checkpoints_it_cannot_read() {
        let r = remint_chat_message_ids(
            &obj(json!({"courierCheckpoints": "not json"})),
            &[],
            mint_message_id,
        );
        assert_eq!(r.chat_patch["courierCheckpoints"], Value::Null);
    }

    #[test]
    fn nulls_unresolvable_watermarks_and_checkpoints_and_leaves_absent_fields_absent() {
        let r = remint_chat_message_ids(
            &obj(json!({
                "otherExtractionWatermarkMessageId": "gone",
                "episodeExtractionWatermarkMessageId": "gone",
                "turnExtractionWatermarkMessageId": "gone",
                "courierCheckpoints": {"c": {"lastResolvedMessageId": "gone", "resolvedAt": "x"}},
            })),
            &[json!({"id": USER_MSG})],
            mint_message_id,
        );
        assert_eq!(
            Value::Object(r.chat_patch.clone()).to_string(),
            json!({
                "otherExtractionWatermarkMessageId": null,
                "episodeExtractionWatermarkMessageId": null,
                "turnExtractionWatermarkMessageId": null,
                "courierCheckpoints": null,
            })
            .to_string()
        );
        assert_eq!(r.dropped_refs, 4);
        assert!(!r.chat_patch.contains_key("summaryAnchorMessageIds"));
    }

    #[test]
    fn remap_imported_message_id_follows_v4s_three_arms() {
        let mut maps = MessageIdMaps::new();
        maps.insert("c1".into(), HashMap::from([("m1".into(), "n1".into())]));
        assert_eq!(
            remap_imported_message_id(&maps, Some("c2"), "m1"),
            Some("m1".into())
        );
        assert_eq!(
            remap_imported_message_id(&maps, None, "m1"),
            Some("m1".into())
        );
        assert_eq!(
            remap_imported_message_id(&maps, Some("c1"), "m1"),
            Some("n1".into())
        );
        assert_eq!(remap_imported_message_id(&maps, Some("c1"), "m9"), None);
    }
}
