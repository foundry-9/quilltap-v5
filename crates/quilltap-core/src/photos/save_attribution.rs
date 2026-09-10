//! Who a Salon image-save is attributed to (v4 `lib/photos/save-attribution.ts`,
//! `86d59660c`) — plus the request body both save routes share.
//!
//! There are two doors onto the same album picker — the message toolbar's
//! bookmark (`messageSaveImage`) and the chat gallery's Save
//! (`chatSaveGalleryImage`) — and they must write the same name into the
//! kept-image sidecar's frontmatter, because that name is what the Commonplace
//! Book and every later reader see. This is the one place that decides it.
//!
//! The rule: if the chosen album *is* a participant's character vault, the save
//! is that character's, matching the LLM `keep_image` flow. Otherwise it is the
//! operator's, under the name of whichever persona they are speaking as — the
//! actively-impersonated one when there is one, else the first user-controlled
//! participant, else the account's own name.

use rusqlite::Connection;
use serde_json::Value;

use crate::db::characters_read;
use crate::db::doc_mount_points::DocMountPointsRepository;
use crate::photos::keep_image_markdown::KeptImageAttributionRole;
use crate::photos::save_image_to_album::SaveImageAttribution;

/// The parsed body both save routes accept (v4 `SaveImageRequestSchema`, now in
/// `lib/photos/save-image-to-album.ts`).
#[derive(Debug)]
pub struct SaveImageRequest {
    pub file_id: String,
    pub mount_point_id: String,
    pub caption: Option<String>,
    pub tags: Vec<String>,
}

/// v4's Zod 4 sentences for the shared schema, in issue order, joined `'; '` by
/// the caller.
///
/// `fileId` is deliberately a plain string rather than a uuid: it may be a
/// `files.id` *or* a `doc_mount_file_links.id`, and the service resolves either.
/// `86d59660c` DELETED the message route's private uuid schema for this one, so
/// its `fileId must be a UUID` / `mountPointId must be a UUID` sentences are
/// gone — a non-uuid id now passes the parse and fails at the attachment guard.
///
/// The two arms differ, and both are pinned: an ABSENT key is Zod's type issue
/// (`Invalid input: expected string, received undefined`), while a PRESENT
/// empty string is the `.min(1, …)` message (`fileId is required`).
pub fn parse_save_image_request(body: &Value) -> Result<SaveImageRequest, String> {
    let mut issues: Vec<String> = Vec::new();

    let file_id = string_field(body, "fileId", "fileId is required", &mut issues);
    let mount_point_id = string_field(
        body,
        "mountPointId",
        "mountPointId is required",
        &mut issues,
    );

    // `caption: z.string().optional()` — absent/undefined passes, any other
    // non-string is an issue. (A JSON `null` is NOT `undefined` to Zod.)
    let caption = match body.get("caption") {
        None => None,
        Some(Value::String(s)) => Some(s.clone()),
        Some(other) => {
            issues.push(format!(
                "Invalid input: expected string, received {}",
                zod_received(other)
            ));
            None
        }
    };

    // `tags: z.array(z.string()).optional()`.
    let tags = match body.get("tags") {
        None => Vec::new(),
        Some(Value::Array(a)) => {
            let mut out = Vec::with_capacity(a.len());
            for item in a {
                match item {
                    Value::String(s) => out.push(s.clone()),
                    other => issues.push(format!(
                        "Invalid input: expected string, received {}",
                        zod_received(other)
                    )),
                }
            }
            out
        }
        Some(other) => {
            issues.push(format!(
                "Invalid input: expected array, received {}",
                zod_received(other)
            ));
            Vec::new()
        }
    };

    if !issues.is_empty() {
        return Err(issues.join("; "));
    }
    Ok(SaveImageRequest {
        file_id: file_id.unwrap_or_default(),
        mount_point_id: mount_point_id.unwrap_or_default(),
        caption,
        tags,
    })
}

fn string_field(
    body: &Value,
    key: &str,
    min_message: &str,
    issues: &mut Vec<String>,
) -> Option<String> {
    match body.get(key) {
        Some(Value::String(s)) => {
            if s.is_empty() {
                issues.push(min_message.to_string());
                None
            } else {
                Some(s.clone())
            }
        }
        Some(other) => {
            issues.push(format!(
                "Invalid input: expected string, received {}",
                zod_received(other)
            ));
            None
        }
        None => {
            issues.push("Invalid input: expected string, received undefined".to_string());
            None
        }
    }
}

/// Zod 4's `received` word for a JSON value.
fn zod_received(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

/// v4 `resolveSaveAttribution(chat, mountPointId, user, repos)`.
///
/// Never throws and never returns null: an unresolvable persona falls back to
/// the account name, and an unresolvable name falls back to "Quilltap", because
/// a save with an odd byline is a better outcome than a save that fails.
pub fn resolve_save_attribution(
    main: &Connection,
    mount: &Connection,
    chat: &Value,
    mount_point_id: &str,
    user_id: &str,
) -> SaveImageAttribution {
    let chat_id = chat.get("id").and_then(Value::as_str).unwrap_or_default();
    let participants = chat
        .get("participants")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    // Is the chosen album a participant's own vault?
    for participant in &participants {
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
        let Some((vault_mp_id, vault_mp_name, character_name)) =
            character_vault(main, mount, character_id)
        else {
            continue;
        };
        if vault_mp_id != mount_point_id {
            continue;
        }
        tracing::debug!(
            chat_id = %chat_id,
            mount_point_id = %mount_point_id,
            character_id = %character_id,
            "Save attributed to a character vault"
        );
        return SaveImageAttribution {
            // v4 `character?.name ?? vault.mountPointName`.
            name: if character_name.is_empty() {
                vault_mp_name
            } else {
                character_name
            },
            id: Some(character_id.to_string()),
            role: KeptImageAttributionRole::Character,
        };
    }

    // Otherwise the operator, under whichever persona they are speaking as.
    let mut user_persona_name: Option<String> = None;
    let mut user_persona_id: Option<String> = None;
    let active_typing = chat
        .get("activeTypingParticipantId")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty());
    let is_user = |p: &Value| p.get("controlledBy").and_then(Value::as_str) == Some("user");
    let active = active_typing.and_then(|id| {
        participants
            .iter()
            .find(|p| p.get("id").and_then(Value::as_str) == Some(id) && is_user(p))
            .cloned()
    });
    let fallback = participants.iter().find(|p| is_user(p)).cloned();
    if let Some(up) = active.or(fallback) {
        if let Some(character_id) = up.get("characterId").and_then(Value::as_str) {
            if let Ok(Some(character)) = characters_read::find_by_id(main, mount, character_id) {
                if let Some(name) = character
                    .get("name")
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                {
                    user_persona_name = Some(name.to_string());
                    user_persona_id = character
                        .get("id")
                        .and_then(Value::as_str)
                        .map(str::to_string);
                }
            }
        }
    }

    let user_name = read_user_name(main, user_id);
    tracing::debug!(
        chat_id = %chat_id,
        mount_point_id = %mount_point_id,
        persona_id = ?user_persona_id,
        "Save attributed to the operator"
    );
    SaveImageAttribution {
        name: user_persona_name
            .or(user_name)
            .unwrap_or_else(|| "Quilltap".to_string()),
        // v4 `userPersonaId ?? user.id ?? null` — the single-user auth id is
        // always present, so the `null` arm is unreachable here.
        id: user_persona_id.or_else(|| Some(user_id.to_string())),
        role: KeptImageAttributionRole::User,
    }
}

/// v4 `getCharacterVaultStore(characterId)` → `(mountPointId, mountPointName,
/// characterName)`. `None` when the character has no database-backed vault.
fn character_vault(
    main: &Connection,
    mount: &Connection,
    character_id: &str,
) -> Option<(String, String, String)> {
    let character = characters_read::find_by_id(main, mount, character_id).ok()??;
    let character_name = character.get("name").and_then(Value::as_str)?.to_string();
    let mp_id = character
        .get("characterDocumentMountPointId")
        .and_then(Value::as_str)?;
    let mp = DocMountPointsRepository::new(mount)
        .find_store_naming_by_id(mp_id)
        .ok()??;
    if mp.mount_type != "database" || mp.store_type.as_deref() != Some("character") {
        return None;
    }
    Some((mp.id, mp.name, character_name))
}

/// v4 auth `user.name` (single-user): the `users` row's display name.
fn read_user_name(main: &Connection, user_id: &str) -> Option<String> {
    main.query_row(
        "SELECT name FROM users WHERE id = ?1",
        rusqlite::params![user_id],
        |r| r.get::<_, Option<String>>(0),
    )
    .ok()
    .flatten()
    .filter(|s| !s.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn absent_keys_take_zods_type_issue_not_the_min_message() {
        let err = parse_save_image_request(&json!({})).unwrap_err();
        assert_eq!(
            err,
            "Invalid input: expected string, received undefined; \
             Invalid input: expected string, received undefined"
        );
    }

    #[test]
    fn empty_strings_take_the_min_messages() {
        let err =
            parse_save_image_request(&json!({ "fileId": "", "mountPointId": "" })).unwrap_err();
        assert_eq!(err, "fileId is required; mountPointId is required");
    }

    #[test]
    fn wrong_types_name_what_they_received() {
        let err = parse_save_image_request(
            &json!({ "fileId": 42, "mountPointId": true, "tags": "nope" }),
        )
        .unwrap_err();
        assert_eq!(
            err,
            "Invalid input: expected string, received number; \
             Invalid input: expected string, received boolean; \
             Invalid input: expected array, received string"
        );
    }

    #[test]
    fn a_non_uuid_file_id_parses() {
        let parsed = parse_save_image_request(
            &json!({ "fileId": "not-a-uuid", "mountPointId": "also-not-a-uuid" }),
        )
        .expect("the shared schema does not gate on uuid");
        assert_eq!(parsed.file_id, "not-a-uuid");
        assert!(parsed.caption.is_none());
        assert!(parsed.tags.is_empty());
    }
}
