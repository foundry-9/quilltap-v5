//! Shared character resolution by name or id (v4
//! `lib/services/character-resolver.ts`).
//!
//! Carries NO reachability/capability gate — callers that need one apply it on
//! top. Scoped to the user's characters (overlaid read), so a raw id from
//! elsewhere can't reach a character that isn't theirs.

use rusqlite::Connection;
use serde_json::Value;

use super::characters_read;
use super::DbError;
use crate::collation::locale_compare;

/// v4 `findCharactersByName`: every character whose name matches `name`
/// case-insensitively, oldest first (by `createdAt`). Empty for a blank query.
///
/// Archived characters never match by name (v4 `d553f72a`): they can't answer
/// Carina queries or receive mail, and skipping them here lets a live
/// character sharing the name (archive → recreate is a legitimate path to
/// that) win the resolution.
pub fn find_characters_by_name(
    main: &Connection,
    mount: &Connection,
    user_id: &str,
    name: &str,
) -> Result<Vec<Value>, DbError> {
    let wanted = name.trim().to_lowercase();
    if wanted.is_empty() {
        return Ok(Vec::new());
    }
    let mut matches: Vec<Value> = characters_read::find_by_user_id(main, mount, user_id)?
        .into_iter()
        .filter(|c| {
            !crate::api::characters::is_archived(c) && name_of(c).trim().to_lowercase() == wanted
        })
        .collect();
    sort_by_created_at(&mut matches);
    Ok(matches)
}

/// v4 `resolveCharacterByNameOrId`: an exact id match wins; else the
/// case-insensitive name match (oldest wins). `None` when nothing matches.
///
/// Name matches skip archived characters (a live namesake wins); an exact id
/// match STILL returns an archived character (v4 `d553f72a`) so the caller can
/// refuse with the named "archived; rehydrate" message instead of a generic
/// not-found. That asymmetry is deliberate on both sides.
pub fn resolve_character_by_name_or_id(
    main: &Connection,
    mount: &Connection,
    user_id: &str,
    token: &str,
) -> Result<Option<Value>, DbError> {
    let trimmed = token.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let candidates = characters_read::find_by_user_id(main, mount, user_id)?;

    if let Some(by_id) = candidates
        .iter()
        .find(|c| c.get("id").and_then(Value::as_str) == Some(trimmed))
    {
        return Ok(Some(by_id.clone()));
    }

    let wanted = trimmed.to_lowercase();
    let mut name_matches: Vec<Value> = candidates
        .into_iter()
        .filter(|c| {
            !crate::api::characters::is_archived(c) && name_of(c).trim().to_lowercase() == wanted
        })
        .collect();
    sort_by_created_at(&mut name_matches);
    Ok(name_matches.into_iter().next())
}

fn name_of(c: &Value) -> &str {
    c.get("name").and_then(Value::as_str).unwrap_or("")
}

/// `(a.createdAt ?? '').localeCompare(b.createdAt ?? '')` — oldest first.
fn sort_by_created_at(chars: &mut [Value]) {
    chars.sort_by(|a, b| {
        let ca = a.get("createdAt").and_then(Value::as_str).unwrap_or("");
        let cb = b.get("createdAt").and_then(Value::as_str).unwrap_or("");
        locale_compare(ca, cb)
    });
}
