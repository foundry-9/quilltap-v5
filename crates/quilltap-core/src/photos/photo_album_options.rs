//! The shape of a "save this picture into…" choice, and the instance-wide list
//! of them (v4 `lib/photos/photo-album-options.ts`, `01a83539d`).
//!
//! Two surfaces offer the same save dialog: the Salon (whose candidates are the
//! chat's own — `api::chat_media::chat_photo_albums`, v4 `handleGetPhotoAlbums`,
//! which `01a83539d` left unchanged and which keeps its own literals) and the
//! wardrobe picture viewer, which is not inside any conversation and so offers
//! every document store the operator could file a picture in. Both answer with
//! [`PhotoAlbumOption`]'s shape, so the dialog renders either list the same way.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::collation::locale_compare;
use crate::db::doc_mount_points::{DocMountPointsRepository, EnabledStoreRow};
use crate::db::fallback::{
    find_all_or_empty, find_by_filter_or_empty, log_mount_index_unavailable,
};
use crate::db::projects::ProjectsRepository;
use crate::db::runtime::Db;
use crate::db::{characters_read, instance_settings, DbError};
use crate::services::mount_index::document_text_search::archived_character_vault_mount_point_ids;

/// v4 `PhotoAlbumKind` — wire `character` / `project` / `document-store` /
/// `general`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PhotoAlbumKind {
    Character,
    Project,
    DocumentStore,
    General,
}

/// v4 `KIND_ORDER` (`:37-42`) — General sorts LAST (and is the default).
const fn kind_order(kind: PhotoAlbumKind) -> u8 {
    match kind {
        PhotoAlbumKind::Character => 0,
        PhotoAlbumKind::Project => 1,
        PhotoAlbumKind::DocumentStore => 2,
        PhotoAlbumKind::General => 3,
    }
}

/// v4 `PhotoAlbumOption` (`:22-35`). Field order is v4's interface order,
/// which puts `isDefault` LAST both in the General literal v4 builds and when
/// v4 appends it to the first option. Every `Option` is omitted when absent
/// (v4's optional keys).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhotoAlbumOption {
    pub mount_point_id: String,
    /// Display label. Character albums use the character name; others use the
    /// mount-point name.
    pub name: String,
    pub kind: PhotoAlbumKind,
    /// Present for `kind: 'character'`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub character_id: Option<String>,
    /// The chat participant whose vault this is — the chat list's only; never
    /// set by [`list_all_photo_album_options`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub participant_id: Option<String>,
    /// Present for `kind: 'character'` — true when the character is
    /// user-controlled.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_user_character: Option<bool>,
    /// Exactly one option in the response is marked default.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_default: Option<bool>,
}

impl PhotoAlbumOption {
    fn store(mp: &EnabledStoreRow, kind: PhotoAlbumKind) -> Self {
        PhotoAlbumOption {
            mount_point_id: mp.id.clone(),
            name: mp.name.clone(),
            kind,
            character_id: None,
            participant_id: None,
            is_user_character: None,
            is_default: None,
        }
    }
}

/// JS truthiness of an optional string cell: present, a string, non-empty.
fn truthy_str<'a>(row: &'a Value, key: &str) -> Option<&'a str> {
    row.get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
}

/// v4 `listAllPhotoAlbumOptions(repos)` (`:53-107`) — every enabled document
/// store a picture may be filed in, outside any chat.
///
/// v4's why, carried: archived characters' vaults are left out — a
/// tombstone's vault is still live and writable, and a save into it would be
/// an edit the archive guards exist to prevent. Character vaults are labelled
/// with their character's name (the LAST live character pointing at a shared
/// vault wins); a project's official store is `project` (a group's official
/// store is a plain `document-store`); Quilltap General is `general` and is
/// the default; everything else — a retired `<Name> Version … Store` no
/// character points at, a filesystem mount — is a `document-store`. Sorted by
/// kind, then name (`localeCompare`, a STABLE sort — ties keep the enabled
/// stores' rowid order); with no General, the first option is the default.
///
/// Infallible, as v4's is — each of the five reads answers v4's fallback:
/// - the enabled stores (`findEnabled` → `findByFilter`): a DEGRADED / absent
///   mount index answers `[]` with v4's dedicated-db DEBUG `Dedicated database
///   unavailable; answering with the fallback`; any other failure answers `[]`
///   with `findByFilter`'s own `Error finding entities by filter` (it answers
///   first, so `findEnabled`'s outer `Error finding enabled mount points` never
///   fires — P4.D275 R-C);
/// - the archived vault ids and the raw characters: `_findAll`'s `[]` +
///   `Error finding all entities` (`characters`);
/// - General's id: `readSetting`'s `null`;
/// - the projects: the OVERLAID `projects.findAll()` (a project whose store is
///   unavailable is dropped, as `applyOverlay` drops it — R-D); a whole-read
///   failure is `_findAll`'s `[]` + `Error finding all entities` (`projects`).
///
/// Then v4's DEBUG `[photo-album-options] Listed every store as a save target`
/// `{enabledStores, archivedVaultsSkipped, offered}` — `archivedVaultsSkipped`
/// is the COLLECTED archived-vault count, as v4's is. Every read and the line
/// run on the caller's thread.
pub fn list_all_photo_album_options(db: &Db) -> Vec<PhotoAlbumOption> {
    let mount_points: Vec<EnabledStoreRow> =
        match db.read_mount_index(|m| DocMountPointsRepository::new(m).find_enabled_for_search()) {
            Ok(rows) => rows,
            Err(error @ DbError::PartitionUnavailable(_)) => {
                log_mount_index_unavailable("doc_mount_points", &error);
                Vec::new()
            }
            Err(error) => find_by_filter_or_empty("doc_mount_points", || Err(error)),
        };
    let archived_vault_ids: Vec<String> = find_all_or_empty("characters", || {
        db.read_main(archived_character_vault_mount_point_ids)
    });
    let general_id: Option<String> = db
        .read_main(instance_settings::get_general_mount_point_id)
        .ok()
        .flatten();
    let characters: Vec<Value> =
        find_all_or_empty("characters", || db.read_main(characters_read::find_all_raw));
    let projects: Vec<Value> = match db.read_main(|main| {
        db.read_mount_index(|mount| {
            ProjectsRepository::new(main, mount)
                .find_all()
                .map_err(|e| e.into_db())
        })
    }) {
        Ok(rows) => rows,
        // The degraded mount index: no enabled store is listed above, so no
        // project id can classify anything — v4's per-project overlay drops
        // are not reproduced here (recorded, P4.D275 R-C).
        Err(DbError::PartitionUnavailable(_)) => Vec::new(),
        Err(error) => find_all_or_empty("projects", || Err(error)),
    };

    let mut character_by_vault: Vec<(&str, &Value)> = Vec::new();
    for c in &characters {
        let archived = truthy_str(c, "archivedAt").is_some();
        if let (Some(vault), false) = (truthy_str(c, "characterDocumentMountPointId"), archived) {
            // v4's `Map.set` — a later character pointing at the same vault wins.
            match character_by_vault.iter_mut().find(|(v, _)| *v == vault) {
                Some(slot) => slot.1 = c,
                None => character_by_vault.push((vault, c)),
            }
        }
    }
    let project_store_ids: Vec<&str> = projects
        .iter()
        .filter_map(|p| truthy_str(p, "officialMountPointId"))
        .collect();

    let mut options: Vec<PhotoAlbumOption> = Vec::new();
    for mp in &mount_points {
        if archived_vault_ids.contains(&mp.id) {
            continue;
        }
        let character = character_by_vault
            .iter()
            .find(|(vault, _)| *vault == mp.id)
            .map(|(_, c)| *c);
        if let Some(character) = character {
            options.push(PhotoAlbumOption {
                mount_point_id: mp.id.clone(),
                name: character
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                kind: PhotoAlbumKind::Character,
                character_id: character
                    .get("id")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                participant_id: None,
                is_user_character: Some(
                    character.get("controlledBy").and_then(Value::as_str) == Some("user"),
                ),
                is_default: None,
            });
        } else if general_id.as_deref() == Some(mp.id.as_str()) {
            let mut general = PhotoAlbumOption::store(mp, PhotoAlbumKind::General);
            general.is_default = Some(true);
            options.push(general);
        } else if project_store_ids.contains(&mp.id.as_str()) {
            options.push(PhotoAlbumOption::store(mp, PhotoAlbumKind::Project));
        } else {
            // Includes a retired `<Name> Version … Store` no character points at.
            options.push(PhotoAlbumOption::store(mp, PhotoAlbumKind::DocumentStore));
        }
    }

    // `sort_by` is stable — V8's `Array.prototype.sort` is, so ties keep the
    // rowid order. Never `sort_unstable_by` here.
    options.sort_by(|a, b| {
        kind_order(a.kind)
            .cmp(&kind_order(b.kind))
            .then_with(|| locale_compare(&a.name, &b.name))
    });
    if !options.iter().any(|o| o.is_default.is_some()) {
        if let Some(first) = options.first_mut() {
            first.is_default = Some(true);
        }
    }

    tracing::debug!(
        target: "quilltap::photos",
        enabledStores = mount_points.len(),
        archivedVaultsSkipped = archived_vault_ids.len(),
        offered = options.len(),
        "[photo-album-options] Listed every store as a save target"
    );
    options
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_option_serializes_in_v4s_key_order() {
        let mut o = PhotoAlbumOption {
            mount_point_id: "m".into(),
            name: "Notes".into(),
            kind: PhotoAlbumKind::DocumentStore,
            character_id: None,
            participant_id: None,
            is_user_character: None,
            is_default: None,
        };
        assert_eq!(
            serde_json::to_string(&o).unwrap(),
            r#"{"mountPointId":"m","name":"Notes","kind":"document-store"}"#
        );
        o.is_default = Some(true);
        assert_eq!(
            serde_json::to_string(&o).unwrap(),
            r#"{"mountPointId":"m","name":"Notes","kind":"document-store","isDefault":true}"#
        );
        o.kind = PhotoAlbumKind::Character;
        o.character_id = Some("c".into());
        o.is_user_character = Some(false);
        o.is_default = None;
        assert_eq!(
            serde_json::to_string(&o).unwrap(),
            r#"{"mountPointId":"m","name":"Notes","kind":"character","characterId":"c","isUserCharacter":false}"#
        );
    }

    #[test]
    fn kind_order_puts_general_last() {
        let mut kinds = [
            PhotoAlbumKind::General,
            PhotoAlbumKind::DocumentStore,
            PhotoAlbumKind::Project,
            PhotoAlbumKind::Character,
        ];
        kinds.sort_by_key(|k| kind_order(*k));
        assert_eq!(
            kinds,
            [
                PhotoAlbumKind::Character,
                PhotoAlbumKind::Project,
                PhotoAlbumKind::DocumentStore,
                PhotoAlbumKind::General,
            ]
        );
    }
}
