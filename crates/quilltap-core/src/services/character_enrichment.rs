//! Characters-list / -detail enrichment (P4.6f) — the route-logic projections
//! v4's `characters/handlers/get.ts` (list) and `[id]/handlers/get.ts` (detail)
//! build over the already-ported repos. Kept deliberately separate from
//! [`crate::services::chat_enrichment`] (the two families do not share their DTO
//! shapes).
//!
//! The list DTO is v4's **hand-assembled whitelist** (`get.ts:58-92`); the detail
//! is the full overlaid row spread + `defaultImage` + `_count`. Both reproduce
//! v4's JS `||` (falsy → null) vs `??` (nullish → default) coercions exactly, and
//! the N+1 partner-name / chat-count fan-out faithfully (correctness first;
//! batching is a post-parity optimization). Field ORDER is not load-bearing — the
//! differential normalizes with a key sort — but present-vs-absent IS: a v4 object
//! literal `k: character.k` omits `k` when the value is `undefined`, so a
//! pass-through field absent on the (differential-proven) overlaid row is omitted
//! here too.

use rusqlite::Connection;
use serde_json::{json, Map, Value};

use crate::db::{characters_read, chats_read, DbError};
use crate::photos::resolve_character_avatar::resolve_character_avatar;

/// v4 `EnrichedDefaultImage` — `{ id, filepath, url: null }` (the reduced wrapper
/// over `resolveCharacterAvatar`; `filepath` carries the resolved URL, `url` is
/// always null).
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct EnrichedDefaultImage {
    pub id: String,
    pub filepath: String,
    /// Always serialized as `null` (NOT skipped — v4 emits the key).
    pub url: Option<String>,
}

/// v4 `enrichWithDefaultImage(imageId, repos)` (`middleware/enrichment.ts:139`):
/// `null`/empty id → `None`; else resolve the avatar and wrap it. The SAME
/// resolver as list and detail.
pub fn enrich_with_default_image(
    main: &Connection,
    mount: &Connection,
    image_id: Option<&str>,
) -> Result<Option<EnrichedDefaultImage>, DbError> {
    let id = match image_id {
        Some(s) if !s.is_empty() => s,
        _ => return Ok(None),
    };
    match resolve_character_avatar(main, mount, Some(id))? {
        Some(r) => Ok(Some(EnrichedDefaultImage {
            id: r.id,
            filepath: r.url,
            url: None,
        })),
        None => Ok(None),
    }
}

/// JS `x || null` — falsy (null/absent/`""`/`false`/`0`) → `null`, else the value.
fn js_or_null(v: Option<&Value>) -> Value {
    match v {
        None | Some(Value::Null) => Value::Null,
        Some(Value::String(s)) if s.is_empty() => Value::Null,
        Some(Value::Bool(false)) => Value::Null,
        Some(Value::Number(n)) if n.as_f64() == Some(0.0) => Value::Null,
        Some(x) => x.clone(),
    }
}

/// JS `x ?? default` — null/absent → `default`, else the value.
fn js_nullish(v: Option<&Value>, default: Value) -> Value {
    match v {
        None | Some(Value::Null) => default,
        Some(x) => x.clone(),
    }
}

/// A pass-through object-literal field (`k: character.k`): present (even `null`)
/// → inserted; absent (v4 `undefined`) → omitted.
fn passthrough(out: &mut Map<String, Value>, character: &Value, key: &str) {
    if let Some(v) = character.get(key) {
        out.insert(key.to_string(), v.clone());
    }
}

/// v4 `characters/handlers/get.ts:58-92` — the per-character whitelist DTO,
/// including the partner-name lookup (N+1 `findById`) and chat count
/// (`chats.findByCharacterId`).
pub fn build_list_dto(
    main: &Connection,
    mount: &Connection,
    character: &Value,
) -> Result<Value, DbError> {
    let mut out = Map::new();

    // Pass-through fields (present-vs-absent mirrored).
    passthrough(&mut out, character, "id");
    passthrough(&mut out, character, "name");
    passthrough(&mut out, character, "title");
    passthrough(&mut out, character, "description");
    passthrough(&mut out, character, "defaultImageId");
    passthrough(&mut out, character, "isFavorite");
    passthrough(&mut out, character, "createdAt");
    passthrough(&mut out, character, "updatedAt");

    // defaultImage (enrichWithDefaultImage of defaultImageId).
    let default_image = enrich_with_default_image(
        main,
        mount,
        character.get("defaultImageId").and_then(Value::as_str),
    )?;
    out.insert(
        "defaultImage".to_string(),
        match default_image {
            Some(i) => serde_json::to_value(i).unwrap_or(Value::Null),
            None => Value::Null,
        },
    );

    // Coerced fields.
    out.insert(
        "controlledBy".to_string(),
        js_nullish(character.get("controlledBy"), json!("llm")),
    );
    out.insert(
        "canBeCarina".to_string(),
        js_nullish(character.get("canBeCarina"), json!(false)),
    );
    out.insert(
        "defaultConnectionProfileId".to_string(),
        js_or_null(character.get("defaultConnectionProfileId")),
    );

    // Partner id (`|| null`) + name (null default; partner.name when found).
    let partner_id = js_or_null(character.get("defaultPartnerId"));
    let mut partner_name = Value::Null;
    if let Some(pid) = partner_id.as_str() {
        if let Some(partner) = characters_read::find_by_id(main, mount, pid)? {
            if let Some(name) = partner.get("name") {
                partner_name = name.clone();
            }
        }
    }
    out.insert("defaultPartnerId".to_string(), partner_id);
    out.insert("defaultPartnerName".to_string(), partner_name);

    out.insert(
        "defaultTimestampConfig".to_string(),
        js_or_null(character.get("defaultTimestampConfig")),
    );
    out.insert(
        "defaultScenarioId".to_string(),
        js_or_null(character.get("defaultScenarioId")),
    );
    out.insert(
        "defaultSystemPromptId".to_string(),
        js_or_null(character.get("defaultSystemPromptId")),
    );
    out.insert(
        "defaultImageProfileId".to_string(),
        js_or_null(character.get("defaultImageProfileId")),
    );
    out.insert(
        "npc".to_string(),
        js_nullish(character.get("npc"), json!(false)),
    );

    // tags (`|| []`) — any array (even empty) is kept; null/absent → [].
    let tags = match character.get("tags") {
        Some(Value::Array(a)) => Value::Array(a.clone()),
        _ => Value::Array(vec![]),
    };
    out.insert("tags".to_string(), tags);

    // systemPrompts → {id, name, isDefault}; scenarios → {id, title, content}.
    out.insert(
        "systemPrompts".to_string(),
        project_array(character.get("systemPrompts"), &["id", "name", "isDefault"]),
    );
    out.insert(
        "scenarios".to_string(),
        project_array(character.get("scenarios"), &["id", "title", "content"]),
    );

    // _count.chats (N+1 chats.findByCharacterId).
    let cid = character.get("id").and_then(Value::as_str).unwrap_or("");
    let chat_count = chats_read::find_by_character_id(main, cid)?.len() as i64;
    out.insert("_count".to_string(), json!({ "chats": chat_count }));

    Ok(Value::Object(out))
}

/// Map an array of objects to a whitelist of keys (v4 `.map(p => ({...}))`):
/// each element keeps only `keys` (present-vs-absent mirrored per key).
fn project_array(v: Option<&Value>, keys: &[&str]) -> Value {
    let Some(Value::Array(items)) = v else {
        return Value::Array(vec![]);
    };
    let projected = items
        .iter()
        .map(|item| {
            let mut o = Map::new();
            for k in keys {
                if let Some(val) = item.get(*k) {
                    o.insert((*k).to_string(), val.clone());
                }
            }
            Value::Object(o)
        })
        .collect();
    Value::Array(projected)
}

/// v4 `[id]/handlers/get.ts` default branch — the full overlaid row spread PLUS
/// `defaultImage` + `_count.chats`.
pub fn build_detail(
    main: &Connection,
    mount: &Connection,
    character: Value,
) -> Result<Value, DbError> {
    let mut out = match character {
        Value::Object(m) => m,
        _ => return Ok(character),
    };
    let default_image = enrich_with_default_image(
        main,
        mount,
        out.get("defaultImageId").and_then(Value::as_str),
    )?;
    out.insert(
        "defaultImage".to_string(),
        match default_image {
            Some(i) => serde_json::to_value(i).unwrap_or(Value::Null),
            None => Value::Null,
        },
    );
    let cid = out.get("id").and_then(Value::as_str).unwrap_or("");
    let chat_count = chats_read::find_by_character_id(main, cid)?.len() as i64;
    out.insert("_count".to_string(), json!({ "chats": chat_count }));
    Ok(Value::Object(out))
}
