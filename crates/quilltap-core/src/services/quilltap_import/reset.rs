//! v4 `handleResetBuiltins` (`app/api/v1/characters/handlers/post.ts:196`) as a
//! **service function** (P4.4u4 unit 2 tier-2). Cascade-delete the built-in
//! characters (preserving nothing exclusive), re-import the committed seed with a
//! seed-id → preserved-id remap so re-imported entities keep the pre-existing
//! ids where possible, then reseed the built-in avatars.
//!
//! Its dispatch arm (the `Request` variant + `api/characters.rs` arm) is NOT part
//! of this lane — it lands as a unification wire (lane A owns `api/types.rs` this
//! round), the `[[p4.6i-characters-remainder-server]]` photo_link_summary
//! precedent. This module delivers the `pub` service + its differential.
//!
//! **A note on "preserved" ids:** v4's `repos.characters.create` MINTS a fresh id
//! (it strips the source id), so the seed-id → preserved-id remap does NOT make
//! the re-imported character keep its old id — `create` mints a third id.
//! `preservedIds` in the result is the pre-reset id; `postResetIds` is the newly
//! minted one. This is v4's exact behavior (the differential pins it), reproduced
//! faithfully, quirk and all.

use std::collections::HashMap;

use rusqlite::Connection;
use serde_json::{Map, Value};

use super::seed_assets::LORIAN_AND_RIYA_QTAP;
use super::{
    execute_import, parse_export_file, ImportError, ImportOptions, ImportResult, QuilltapExport,
};
use crate::db::characters_read;
use crate::services::cascade_delete::execute_cascade_delete;
use crate::services::file_storage::PixelCodec;

/// v4 `BUILTIN_CHARACTER_NAMES` (`post.ts:137`).
pub const BUILTIN_CHARACTER_NAMES: &[&str] = &["Lorian", "Riya"];

/// v4's reset-builtins response shape (the subset the service produces). The
/// `preserved_ids` / `post_reset_ids` pairs are ordered `[Lorian, Riya]`.
#[derive(Debug, Clone)]
pub struct ResetBuiltinsResult {
    pub deleted_character_ids: Vec<String>,
    /// `(name, pre-reset id | None)` for each built-in, in order.
    pub preserved_ids: Vec<(String, Option<String>)>,
    /// `(name, post-reset id | None)` for each built-in, in order.
    pub post_reset_ids: Vec<(String, Option<String>)>,
    pub remapped_id_count: usize,
    pub import: ImportResult,
}

/// A reset failure — a cascade delete that reported failure (v4 `serverError`) or
/// an import/store error.
#[derive(Debug)]
pub enum ResetError {
    /// A built-in delete failed (v4 returns `serverError(...)`).
    DeleteFailed {
        name: String,
    },
    /// The embedded seed failed to parse (a build/asset bug).
    Seed(ImportError),
    Import(ImportError),
    Db(crate::db::DbError),
}

impl std::fmt::Display for ResetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResetError::DeleteFailed { name } => {
                write!(f, "Failed to delete {name} during reset")
            }
            ResetError::Seed(e) => write!(f, "Built-in character seed data is unavailable: {e}"),
            ResetError::Import(e) => write!(f, "reset re-import failed: {e}"),
            ResetError::Db(e) => write!(f, "reset failed: {e}"),
        }
    }
}
impl std::error::Error for ResetError {}
impl From<crate::db::DbError> for ResetError {
    fn from(e: crate::db::DbError) -> Self {
        ResetError::Db(e)
    }
}

/// v4 `handleResetBuiltins`. Run inside one `Db::write` closure (both connections
/// writable). `user_id` scopes the character reads (single-user: `SINGLE_USER_ID`).
pub fn reset_builtins(
    main: &Connection,
    mount: &Connection,
    codec: &dyn PixelCodec,
    user_id: &str,
) -> Result<ResetBuiltinsResult, ResetError> {
    // 1. Existing built-in characters by (lowercased) name.
    let all = characters_read::find_by_user_id(main, mount, user_id)?;
    let existing_by_name: HashMap<String, Value> = all
        .iter()
        .filter(|c| {
            c.get("name")
                .and_then(Value::as_str)
                .map(|n| {
                    BUILTIN_CHARACTER_NAMES
                        .iter()
                        .any(|b| b.eq_ignore_ascii_case(n))
                })
                .unwrap_or(false)
        })
        .filter_map(|c| {
            c.get("name")
                .and_then(Value::as_str)
                .map(|n| (n.to_lowercase(), c.clone()))
        })
        .collect();

    let id_of = |name: &str| -> Option<String> {
        existing_by_name
            .get(&name.to_lowercase())
            .and_then(|c| c.get("id").and_then(Value::as_str))
            .map(str::to_string)
    };
    let preserved_ids: Vec<(String, Option<String>)> = BUILTIN_CHARACTER_NAMES
        .iter()
        .map(|n| (n.to_string(), id_of(n)))
        .collect();

    // 2. Cascade-delete each existing built-in (no exclusive chats/images).
    let mut deleted_character_ids = Vec::new();
    for &name in BUILTIN_CHARACTER_NAMES {
        if let Some(id) = id_of(name) {
            let (success, _, _, _) = execute_cascade_delete(main, mount, &id, false, false)?;
            if !success {
                return Err(ResetError::DeleteFailed {
                    name: name.to_string(),
                });
            }
            deleted_character_ids.push(id);
        }
    }

    // 3. The seed → preserved id remap. `create` will still mint fresh ids, but we
    //    reproduce v4's remap verbatim (quirk and all).
    let export = parse_export_file(LORIAN_AND_RIYA_QTAP).map_err(ResetError::Seed)?;
    let seed_id_by_name = find_builtin_character_ids(&export.data);
    let mut id_mapping: HashMap<String, String> = HashMap::new();
    for (name, preserved) in &preserved_ids {
        if let (Some(seed_id), Some(preserved_id)) = (seed_id_by_name.get(name), preserved) {
            if seed_id != preserved_id {
                id_mapping.insert(seed_id.clone(), preserved_id.clone());
            }
        }
    }
    let remapped = remap_export(&export, &id_mapping);

    // 4. Re-import with the same seed options.
    let import = execute_import(
        main,
        mount,
        user_id,
        &remapped,
        &ImportOptions::seed_defaults(),
        None,
    )
    .map_err(ResetError::Import)?;

    // 5. Reseed the built-in avatars.
    let names: Vec<&str> = BUILTIN_CHARACTER_NAMES.to_vec();
    let mut avatar_report = super::seed::SeedReport::default();
    super::seed::seed_avatars(main, mount, codec, Some(&names), &mut avatar_report);

    // 6. Post-reset ids.
    let refreshed = characters_read::find_by_user_id(main, mount, user_id)?;
    let post_of = |name: &str| -> Option<String> {
        refreshed
            .iter()
            .find(|c| {
                c.get("name")
                    .and_then(Value::as_str)
                    .map(|n| n.eq_ignore_ascii_case(name))
                    .unwrap_or(false)
            })
            .and_then(|c| c.get("id").and_then(Value::as_str))
            .map(str::to_string)
    };
    let post_reset_ids: Vec<(String, Option<String>)> = BUILTIN_CHARACTER_NAMES
        .iter()
        .map(|n| (n.to_string(), post_of(n)))
        .collect();

    Ok(ResetBuiltinsResult {
        deleted_character_ids,
        preserved_ids,
        post_reset_ids,
        remapped_id_count: id_mapping.len(),
        import,
    })
}

/// v4 `findBuiltinCharacterIds` (`post.ts:139`): `{name: id}` for every character
/// in the export's `data.characters`.
fn find_builtin_character_ids(data: &Value) -> HashMap<String, String> {
    let mut out = HashMap::new();
    if let Some(chars) = data.get("characters").and_then(Value::as_array) {
        for c in chars {
            if let (Some(name), Some(id)) = (
                c.get("name").and_then(Value::as_str),
                c.get("id").and_then(Value::as_str),
            ) {
                out.insert(name.to_string(), id.to_string());
            }
        }
    }
    out
}

/// v4 `replaceMappedIdsRecursively` (`post.ts:172`): rewrite every string value
/// through `id_mapping` (recursing arrays + objects). Applied to the whole export
/// (manifest + data), matching v4 (`replaceMappedIdsRecursively(seedImport.data)`).
fn remap_export(export: &QuilltapExport, id_mapping: &HashMap<String, String>) -> QuilltapExport {
    let mut whole = Map::new();
    whole.insert("manifest".to_string(), export.manifest.clone());
    whole.insert("data".to_string(), export.data.clone());
    let remapped = replace_mapped_ids_recursively(&Value::Object(whole), id_mapping);
    let obj = remapped.as_object().cloned().unwrap_or_default();
    QuilltapExport {
        manifest: obj.get("manifest").cloned().unwrap_or(Value::Null),
        data: obj.get("data").cloned().unwrap_or(Value::Null),
    }
}

/// The recursive string-remap (v4 `replaceMappedIdsRecursively`).
fn replace_mapped_ids_recursively(value: &Value, id_mapping: &HashMap<String, String>) -> Value {
    match value {
        Value::String(s) => Value::String(id_mapping.get(s).cloned().unwrap_or_else(|| s.clone())),
        Value::Array(arr) => Value::Array(
            arr.iter()
                .map(|v| replace_mapped_ids_recursively(v, id_mapping))
                .collect(),
        ),
        Value::Object(obj) => {
            let mut next = Map::new();
            for (k, v) in obj {
                next.insert(k.clone(), replace_mapped_ids_recursively(v, id_mapping));
            }
            Value::Object(next)
        }
        other => other.clone(),
    }
}
