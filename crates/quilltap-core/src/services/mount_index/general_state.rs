//! General (instance-wide) state — v4 `lib/mount-index/general-state.ts` (NEW
//! at `f48f34dc`): read/write helpers for the `state.json` document at the root
//! of the singleton "Quilltap General" mount point.
//!
//! General state is the bottom tier of the four-tier state cascade
//! (chat → project → group → general). It is the instance-wide default layer:
//! keys set here are visible to every chat unless a narrower tier overrides
//! them. There is no entity row for it — it is simply a JSON document living at
//! the mount root, provisioned idempotently at startup the same way character
//! vaults get their `metadata.json` and the general mount gets its `Scenarios/`
//! folder.
//!
//! All helpers degrade gracefully when the mount has not yet been provisioned:
//! reads return `{}`, `ensure` no-ops, and only the authenticated `write` path
//! errors.

use rusqlite::Connection;
use serde_json::{Map, Value};

use crate::db::database_store::{
    read_database_document, write_database_document, DbStoreErrorCode, StoreError,
};
use crate::db::doc_mount_documents::DocMountDocumentsRepository;
use crate::db::instance_settings::get_general_mount_point_id;
use crate::db::DbError;

/// Relative path of the general state document inside the mount.
pub const GENERAL_STATE_JSON_PATH: &str = "state.json";

/// A [`write_general_state`] failure. `Unprovisioned` carries v4's plain-`Error`
/// message VERBATIM in its `Display` — the state tool's catch surfaces
/// `error.message` to the LLM, so the bytes matter.
#[derive(Debug)]
pub enum GeneralStateWriteError {
    /// v4: `throw new Error('Quilltap General mount has not been provisioned yet')`.
    Unprovisioned,
    Db(DbError),
    Store(StoreError),
}

impl std::fmt::Display for GeneralStateWriteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GeneralStateWriteError::Unprovisioned => {
                f.write_str("Quilltap General mount has not been provisioned yet")
            }
            GeneralStateWriteError::Db(e) => write!(f, "{e}"),
            GeneralStateWriteError::Store(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for GeneralStateWriteError {}

/// v4 `ensureGeneralStateFile` — idempotent: ensure a `state.json` exists at
/// the root of the "Quilltap General" mount, seeding an empty `{}` if not.
///
/// Returns true when it created the file, false when the file already existed
/// or the mount is not yet provisioned. Existence is checked directly rather
/// than by parsing: a body the user has hand-edited into invalid JSON is still
/// their state and must never be "healed" into an empty one — mirroring
/// `ensureCharacterMetadataFile`.
pub fn ensure_general_state_file(main: &Connection, mount: &Connection) -> Result<bool, DbError> {
    let Some(mount_point_id) = get_general_mount_point_id(main)? else {
        return Ok(false);
    };

    let docs = DocMountDocumentsRepository::new(mount);
    if docs
        .find_by_mount_point_and_path(&mount_point_id, GENERAL_STATE_JSON_PATH)?
        .is_some()
    {
        return Ok(false);
    }

    write_database_document(mount, &mount_point_id, GENERAL_STATE_JSON_PATH, "{}").map_err(
        |e| match e {
            StoreError::Db(db) => db,
            StoreError::Store(s) => DbError::Key(s.message),
        },
    )?;
    Ok(true)
}

/// v4 `readGeneralState` — read general state. Returns `{}` when the mount is
/// not yet provisioned, the document is missing, or its body is unparseable —
/// the last case warns but never errors, matching the group store overlay's
/// corrupt-`state.json` behaviour (state is not the keystone document).
///
/// `mount` is optional so degraded (no mount-index) opens read as unprovisioned.
pub fn read_general_state(main: &Connection, mount: Option<&Connection>) -> Value {
    let empty = || Value::Object(Map::new());
    let Some(mount) = mount else {
        return empty();
    };
    let mount_point_id = match get_general_mount_point_id(main) {
        Ok(Some(id)) => id,
        // Unprovisioned (or an unreadable settings table — v4's catch) → {}.
        _ => return empty(),
    };

    match read_database_document(mount, &mount_point_id, GENERAL_STATE_JSON_PATH) {
        Ok(doc) => {
            // v4: `JSON.parse(content) ?? {}` — a literal `null` body coalesces to {}.
            let parsed = match serde_json::from_str::<Value>(&doc.content) {
                Ok(Value::Null) => return empty(),
                Ok(v) => v,
                Err(_) => {
                    eprintln!(
                        "[GeneralState] state.json unparseable; defaulting to {{}} (mount {mount_point_id})"
                    );
                    return empty();
                }
            };
            if !parsed.is_object() {
                eprintln!(
                    "[GeneralState] state.json is not a JSON object; defaulting to {{}} (mount {mount_point_id})"
                );
                return empty();
            }
            parsed
        }
        Err(StoreError::Store(s)) if s.code == DbStoreErrorCode::NotFound => empty(),
        Err(_) => {
            eprintln!(
                "[GeneralState] state.json unparseable; defaulting to {{}} (mount {mount_point_id})"
            );
            empty()
        }
    }
}

/// v4 `writeGeneralState` — overwrite general state wholesale
/// (`JSON.stringify(state ?? {}, null, 2)`). Errors when the mount has not yet
/// been provisioned, since this only fires from authenticated write paths where
/// the mount should already exist (matching `setGeneralScenarioDefault`).
pub fn write_general_state(
    main: &Connection,
    mount: &Connection,
    state: &Value,
) -> Result<(), GeneralStateWriteError> {
    let Some(mount_point_id) =
        get_general_mount_point_id(main).map_err(GeneralStateWriteError::Db)?
    else {
        return Err(GeneralStateWriteError::Unprovisioned);
    };
    // `state ?? {}` — a JS null/undefined collapses to {} (a Rust caller passes
    // Value::Null for that case).
    let body = if state.is_null() {
        "{}".to_string()
    } else {
        serde_json::to_string_pretty(state).map_err(|e| {
            GeneralStateWriteError::Db(DbError::Key(format!("general state serialize: {e}")))
        })?
    };
    write_database_document(mount, &mount_point_id, GENERAL_STATE_JSON_PATH, &body)
        .map_err(GeneralStateWriteError::Store)?;
    Ok(())
}
