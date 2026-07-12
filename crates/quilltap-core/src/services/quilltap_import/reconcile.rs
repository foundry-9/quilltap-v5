//! v4 `reconcileRelationships` (`reconcile.ts`) — the character loop only. Every
//! branch is a no-op for the seed: no tags, no `defaultPartnerId`, and the
//! connection/image/template/mount maps are all empty in the subset, so every
//! remap resolves to null and `hasUpdates` stays false. Ported faithfully (it is
//! cheap and the differential covers it); the chats/projects/profiles/templates
//! loops don't iterate (their id-maps are empty).

use rusqlite::Connection;
use serde_json::Value;

use super::{IdMap, ImportError};
use crate::db::characters::{CharacterUpdate, CharactersRepository};
use crate::db::characters_read;

/// Reconcile the imported characters' relationship FKs through the id-maps. For
/// the subset only the character map is non-empty (used by `defaultPartnerId`);
/// the tags/connection/image/template/mount maps are empty, so those remaps
/// resolve to null and never trigger an update.
pub(super) fn reconcile_characters(
    main: &Connection,
    mount: &Connection,
    character_id_map: &IdMap,
    warnings: &mut Vec<String>,
) -> Result<(), ImportError> {
    for (_source_id, new_id) in character_id_map.0.iter() {
        let character = match characters_read::find_by_id(main, mount, new_id) {
            Ok(Some(c)) => c,
            Ok(None) => continue,
            Err(e) => {
                warnings.push(format!("Failed to reconcile character relationships: {e}"));
                continue;
            }
        };

        let mut patch = CharacterUpdate::default();
        let mut has_updates = false;

        // Remap tags through the (empty) tags map → identity. Only a non-empty tag
        // set would rewrite; the seed characters carry none.
        if let Some(tags) = character.get("tags").and_then(Value::as_array) {
            if !tags.is_empty() {
                // Empty tags map → each id maps to itself (`get(id) || id`).
                let remapped: Vec<String> = tags
                    .iter()
                    .filter_map(Value::as_str)
                    .map(|s| s.to_string())
                    .collect();
                if !remapped.is_empty() {
                    patch.tags = Some(remapped);
                    has_updates = true;
                }
            }
        }

        // Remap defaultPartnerId through the character map. Absent for the seed.
        if let Some(partner) = character.get("defaultPartnerId").and_then(Value::as_str) {
            if let Some(new_partner) = character_id_map.get(partner) {
                patch.default_partner_id = Some(new_partner.to_string());
                has_updates = true;
            }
        }

        // The connection/image/template/mount remaps use maps that are always
        // empty in the subset, so `remapId` returns null and never sets an update
        // (v4's `if (newX)` guards). The freshly-provisioned vault mount id on the
        // row is therefore preserved — nulling it would orphan the vault.

        if has_updates {
            patch.updated_at = crate::clock::now_iso();
            if let Err(e) = CharactersRepository::new(main).update(new_id, &patch) {
                warnings.push(format!("Failed to reconcile character relationships: {e}"));
            }
        }
    }

    Ok(())
}
