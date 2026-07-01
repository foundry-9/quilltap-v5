//! The **public** wardrobe write path (seam #7) — v4's `WardrobeRepository`
//! `create`/`update`/`delete` composed over the already-verified vault leaves.
//!
//! v4's public `WardrobeRepository` is deliberately **vault-only**: every mutation
//! resolves the owning character's document-store mount, reads the current
//! `Wardrobe/*.md` items, applies the change in memory, cycle-checks, and
//! re-projects the whole folder — there is **no SQL mirror**, and an unresolvable
//! mount *throws*. The `wardrobe_tier2` port verified the base-repository SQL
//! marshaling for the (legacy) `wardrobe_items` table; this module ports the
//! public composition itself:
//!
//! - `resolveWardrobeMount` (character scope) — read the character's slim row for
//!   `characterDocumentMountPointId`; a missing character or mount is `NoMount`
//!   (v4 throws).
//! - `createAtLocation` / `updateAtLocation` / `deleteAtLocation` — the
//!   read-modify-project round-trip, minting `updatedAt` on update.
//! - `assertNoCycles` / `buildCyclePeers` — the save-time component-graph guard.
//!
//! It composes the verified leaves: [`read_character_vault_wardrobe`] (read),
//! [`project_vault_wardrobe`] (write), [`detect_component_cycles`] (cycle check),
//! and `characters_read::find_by_id_raw` (mount resolution). The differential
//! (`vault_wardrobe_public_equivalence`) drives v4's REAL public repo and compares
//! the read-back item list per op — a read-back tier because
//! `build_wardrobe_item_file` writes the item's minted `updatedAt` into the
//! content-addressed `.md`, so a byte-level tier-2 dump can't normalize an
//! update's fresh timestamp buried in a content SHA (the read parses it back out,
//! where it normalizes cleanly). The projection primitive itself is already
//! byte-verified (`vault_wardrobe_write_equivalence`).
//!
//! ## Scope / deferrals (mirroring `read_character_vault_wardrobe`)
//!
//! Only the **character** tier is ported. The **General** archetype tier
//! (`characterId == null` → `getGeneralMountPointId`) and the **project** tier
//! (`projectWardrobeLocation`) route through the General-Wardrobe subsystem, which
//! is not ported; the corpus provisions no General store, so v4's
//! `readGeneralWardrobe` / `findArchetypes` yield nothing and the archetype-seed
//! into the cycle peers is a verified no-op. A `null` `characterId` on the public
//! path therefore resolves to `NoMount` here (the unprovisioned-General case v4
//! also surfaces as a throw). v4's per-mount write serialization (`runSerialized`)
//! is a Node-concurrency guard, not on-disk state — the single-writer model
//! already serializes applies.

use std::collections::HashMap;

use rusqlite::Connection;
use serde_json::Value;

use crate::clock;
use crate::vault_overlay::{detect_component_cycles, WardrobeItem};

use super::doc_mount_documents::DocMountDocumentsRepository;
use super::doc_mount_file_links::DocMountFileLinksRepository;
use super::vault_read_overlay::read_character_vault_wardrobe;
use super::vault_wardrobe_write::project_vault_wardrobe;
use super::{characters_read, DbError};

/// A failure on the public wardrobe write path. `NoMount` / `Cycle` are v4's
/// thrown `Error`s (the differential compares their messages); `Db` wraps a
/// storage error.
#[derive(Debug)]
pub enum WardrobePublicError {
    /// No vault mount resolved (missing character, unprovisioned store, or a
    /// `null` characterId with no General tier). v4 throws the "no Character Vault
    /// or Quilltap General mount is available" message.
    NoMount,
    /// The mutation would create a component cycle. Carries v4's exact message.
    Cycle(String),
    /// An underlying storage error.
    Db(DbError),
}

impl From<DbError> for WardrobePublicError {
    fn from(e: DbError) -> Self {
        WardrobePublicError::Db(e)
    }
}

/// v4's create/update/delete throw the SAME "no mount" message; expose it so the
/// differential can match the threw-string. (create says "create", update/delete
/// "update"/"delete", but all share this suffix — the differential keys on the
/// stable prefix.)
pub const NO_MOUNT_MESSAGE: &str = "no Character Vault or Quilltap General mount is available. \
Wardrobe items are stored exclusively in the document store.";

/// A partial wardrobe update (v4's `Partial<WardrobeItem>` patch, `{...cur,
/// ...patch}`). Only the fields the public path mutates; `None` = key absent
/// (unchanged), `Some(None)` = set a nullable field to null.
#[derive(Debug, Default, Clone)]
pub struct WardrobePatch {
    pub title: Option<String>,
    pub types: Option<Vec<String>>,
    pub component_item_ids: Option<Vec<String>>,
    pub description: Option<Option<String>>,
    pub image_prompt: Option<Option<String>>,
    pub appropriateness: Option<Option<String>>,
    pub is_default: Option<bool>,
    pub replace: Option<bool>,
    pub archived_at: Option<Option<String>>,
}

impl WardrobePatch {
    /// Overlay the present keys onto `item` (the `...patch` half of the merge).
    fn apply(&self, item: &mut WardrobeItem) {
        if let Some(v) = &self.title {
            item.title = v.clone();
        }
        if let Some(v) = &self.types {
            item.types = v.clone();
        }
        if let Some(v) = &self.component_item_ids {
            item.component_item_ids = v.clone();
        }
        if let Some(v) = &self.description {
            item.description = Some(v.clone());
        }
        if let Some(v) = &self.image_prompt {
            item.image_prompt = Some(v.clone());
        }
        if let Some(v) = &self.appropriateness {
            item.appropriateness = Some(v.clone());
        }
        if let Some(v) = self.is_default {
            item.is_default = v;
        }
        if let Some(v) = self.replace {
            item.replace = v;
        }
        if let Some(v) = &self.archived_at {
            item.archived_at = Some(v.clone());
        }
    }
}

/// Resolve a character's vault mount (v4 `resolveWardrobeMount`, character scope):
/// the slim row's `characterDocumentMountPointId`. `None` when the character is
/// absent or has no linked mount.
fn resolve_character_mount(
    main: &Connection,
    character_id: &str,
) -> Result<Option<String>, DbError> {
    let Some(row) = characters_read::find_by_id_raw(main, character_id)? else {
        return Ok(None);
    };
    Ok(row
        .get("characterDocumentMountPointId")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string))
}

/// Read the character vault's current `Wardrobe/` items (v4 `readMountItems`):
/// [`read_character_vault_wardrobe`], each item's `characterId` set to `loc`'s
/// (`{...item, characterId: loc.characterId}`). Empty/missing folder → `[]`.
fn read_mount_items(
    docs: &DocMountDocumentsRepository,
    mount_point_id: &str,
    character_id: &str,
) -> Result<Vec<WardrobeItem>, DbError> {
    let Some(vault) = read_character_vault_wardrobe(docs, mount_point_id, character_id)? else {
        return Ok(Vec::new());
    };
    let items = vault
        .get("items")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    Ok(items
        .iter()
        .map(|it| item_from_read(it, character_id))
        .collect())
}

/// Convert one read item (a `WardrobeItemFromFile`-shaped JSON value) into a
/// [`WardrobeItem`] for re-projection, overriding `characterId` with the
/// location's (v4's `readMountItems` map).
fn item_from_read(v: &Value, character_id: &str) -> WardrobeItem {
    WardrobeItem {
        id: str_field(v, "id"),
        character_id: Some(Some(character_id.to_string())),
        title: str_field(v, "title"),
        description: opt_opt(v.get("description")),
        image_prompt: opt_opt(v.get("imagePrompt")),
        types: str_array(v, "types"),
        component_item_ids: str_array(v, "componentItemIds"),
        appropriateness: opt_opt(v.get("appropriateness")),
        is_default: v.get("isDefault").and_then(Value::as_bool).unwrap_or(false),
        replace: v.get("replace").and_then(Value::as_bool).unwrap_or(false),
        migrated_from_clothing_record_id: opt_opt(v.get("migratedFromClothingRecordId")),
        archived_at: opt_opt(v.get("archivedAt")),
        created_at: str_field(v, "createdAt"),
        updated_at: str_field(v, "updatedAt"),
    }
}

fn str_field(v: &Value, k: &str) -> String {
    v.get(k)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn str_array(v: &Value, k: &str) -> Vec<String> {
    v.get(k)
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|e| e.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn opt_opt(v: Option<&Value>) -> Option<Option<String>> {
    match v {
        Some(Value::String(s)) => Some(Some(s.clone())),
        _ => None,
    }
}

/// Build the id→componentItemIds map used for cycle detection (v4
/// `buildCyclePeers`, character scope): the location's current items. The shared
/// General-archetype seeding is the deferred tier (empty in the corpus).
fn build_cycle_peers(current: &[WardrobeItem]) -> HashMap<String, Vec<String>> {
    current
        .iter()
        .map(|i| (i.id.clone(), i.component_item_ids.clone()))
        .collect()
}

/// v4's `assertNoCycles`: ensure the item's own components are in the peer map,
/// then reject any cycle with v4's exact message (` → ` between hops, `; ` between
/// distinct cycles).
fn assert_no_cycles(
    item: &WardrobeItem,
    mut peers: HashMap<String, Vec<String>>,
) -> Result<(), WardrobePublicError> {
    if item.component_item_ids.is_empty() {
        return Ok(());
    }
    peers.insert(item.id.clone(), item.component_item_ids.clone());
    let cycles = detect_component_cycles(&item.id, &item.component_item_ids, &peers);
    if !cycles.is_empty() {
        let joined = cycles
            .iter()
            .map(|c| c.join(" \u{2192} "))
            .collect::<Vec<_>>()
            .join("; ");
        return Err(WardrobePublicError::Cycle(format!(
            "Wardrobe item {} would create a component cycle: {}",
            item.id, joined
        )));
    }
    Ok(())
}

/// The owning character id of an item, or `None` for a shared archetype (`null`).
fn item_character_id(item: &WardrobeItem) -> Option<String> {
    match &item.character_id {
        Some(Some(s)) => Some(s.clone()),
        _ => None,
    }
}

/// v4 `WardrobeRepository.create` → `createVaultWardrobeItem` → `createAtLocation`
/// (character scope). `item` already carries its id/createdAt/updatedAt (the
/// public repo materializes them before this point). Returns the stored item;
/// `NoMount` when the character has no vault (v4 throws).
pub fn create_vault_wardrobe_item(
    main: &Connection,
    links: &DocMountFileLinksRepository,
    docs: &DocMountDocumentsRepository,
    item: &WardrobeItem,
) -> Result<WardrobeItem, WardrobePublicError> {
    let Some(character_id) = item_character_id(item) else {
        return Err(WardrobePublicError::NoMount); // null → General (deferred)
    };
    let Some(mount_point_id) = resolve_character_mount(main, &character_id)? else {
        return Err(WardrobePublicError::NoMount);
    };

    let current = read_mount_items(docs, &mount_point_id, &character_id)?;
    // stored = {...item, characterId: loc.characterId} (already `character_id`).
    let stored = item.clone();
    assert_no_cycles(&stored, build_cycle_peers(&current))?;

    let mut next = current;
    next.push(stored.clone());
    project_vault_wardrobe(links, docs, &mount_point_id, &next)?;
    Ok(stored)
}

/// v4 `WardrobeRepository.update` → `updateVaultWardrobeItem` → `updateAtLocation`
/// (character scope). Merges the patch onto the found item, preserving
/// id/createdAt/characterId and minting a fresh `updatedAt`. `Ok(None)` when the
/// id isn't in the folder; `NoMount` when the character has no vault (v4 throws).
pub fn update_vault_wardrobe_item(
    main: &Connection,
    links: &DocMountFileLinksRepository,
    docs: &DocMountDocumentsRepository,
    id: &str,
    patch: &WardrobePatch,
    character_id_hint: &str,
) -> Result<Option<WardrobeItem>, WardrobePublicError> {
    let Some(mount_point_id) = resolve_character_mount(main, character_id_hint)? else {
        return Err(WardrobePublicError::NoMount);
    };

    let current = read_mount_items(docs, &mount_point_id, character_id_hint)?;
    let Some(idx) = current.iter().position(|i| i.id == id) else {
        return Ok(None);
    };

    // {...current[idx], ...patch, id, characterId, createdAt: cur.createdAt, updatedAt: now}
    let mut merged = current[idx].clone();
    patch.apply(&mut merged);
    merged.id = current[idx].id.clone();
    merged.character_id = Some(Some(character_id_hint.to_string()));
    merged.created_at = current[idx].created_at.clone();
    merged.updated_at = clock::now_iso();

    assert_no_cycles(&merged, build_cycle_peers(&current))?;

    let mut next = current;
    next[idx] = merged.clone();
    project_vault_wardrobe(links, docs, &mount_point_id, &next)?;
    Ok(Some(merged))
}

/// v4 `WardrobeRepository.delete` → `deleteVaultWardrobeItem` → `deleteAtLocation`
/// (character scope). `Ok(false)` when the id isn't present (no re-projection);
/// `NoMount` when the character has no vault (v4 throws).
pub fn delete_vault_wardrobe_item(
    main: &Connection,
    links: &DocMountFileLinksRepository,
    docs: &DocMountDocumentsRepository,
    id: &str,
    character_id_hint: &str,
) -> Result<bool, WardrobePublicError> {
    let Some(mount_point_id) = resolve_character_mount(main, character_id_hint)? else {
        return Err(WardrobePublicError::NoMount);
    };

    let current = read_mount_items(docs, &mount_point_id, character_id_hint)?;
    let next: Vec<WardrobeItem> = current.iter().filter(|i| i.id != id).cloned().collect();
    if next.len() == current.len() {
        return Ok(false);
    }
    project_vault_wardrobe(links, docs, &mount_point_id, &next)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn item(id: &str, comps: &[&str]) -> WardrobeItem {
        WardrobeItem {
            id: id.to_string(),
            character_id: Some(Some("char".to_string())),
            title: format!("Item {id}"),
            description: None,
            image_prompt: None,
            types: vec!["top".to_string()],
            component_item_ids: comps.iter().map(|s| s.to_string()).collect(),
            appropriateness: None,
            is_default: false,
            replace: false,
            migrated_from_clothing_record_id: None,
            archived_at: None,
            created_at: "2026-02-01T00:00:00.000Z".to_string(),
            updated_at: "2026-02-01T00:00:00.000Z".to_string(),
        }
    }

    #[test]
    fn patch_apply_overlays_present_keys_only() {
        let mut it = item("a", &[]);
        let patch = WardrobePatch {
            title: Some("New Title".into()),
            is_default: Some(true),
            description: Some(Some("desc".into())),
            ..Default::default()
        };
        patch.apply(&mut it);
        assert_eq!(it.title, "New Title");
        assert!(it.is_default);
        assert_eq!(it.description, Some(Some("desc".to_string())));
        // Untouched fields preserved.
        assert_eq!(it.types, vec!["top".to_string()]);
        assert!(!it.replace);
    }

    #[test]
    fn no_cycle_when_components_empty_or_acyclic() {
        // Empty components → always OK.
        assert!(assert_no_cycles(&item("a", &[]), build_cycle_peers(&[])).is_ok());
        // a → b, b has no components → acyclic.
        let current = vec![item("b", &[])];
        assert!(assert_no_cycles(&item("a", &["b"]), build_cycle_peers(&current)).is_ok());
    }

    #[test]
    fn direct_and_mutual_cycles_are_rejected_with_v4_message() {
        // Self-reference: a → a.
        let err = assert_no_cycles(&item("a", &["a"]), build_cycle_peers(&[])).unwrap_err();
        match err {
            WardrobePublicError::Cycle(msg) => {
                assert!(msg.starts_with("Wardrobe item a would create a component cycle: "));
                assert!(msg.contains("a \u{2192} a"));
            }
            other => panic!("expected Cycle, got {other:?}"),
        }
        // Mutual: b → a already present, adding a → b closes the loop.
        let current = vec![item("b", &["a"])];
        let err = assert_no_cycles(&item("a", &["b"]), build_cycle_peers(&current)).unwrap_err();
        assert!(matches!(err, WardrobePublicError::Cycle(_)));
    }

    #[test]
    fn item_from_read_overrides_character_id() {
        let v = json!({
            "id": "w1", "characterId": "someone-else", "title": "Hat",
            "description": null, "imagePrompt": "a hat", "types": ["accessories"],
            "componentItemIds": [], "appropriateness": null, "isDefault": true,
            "replace": false, "migratedFromClothingRecordId": null, "archivedAt": null,
            "createdAt": "2026-02-01T00:00:00.000Z", "updatedAt": "2026-02-01T00:00:00.000Z"
        });
        let it = item_from_read(&v, "owner");
        assert_eq!(it.character_id, Some(Some("owner".to_string())));
        assert_eq!(it.title, "Hat");
        assert_eq!(it.image_prompt, Some(Some("a hat".to_string())));
        assert_eq!(it.description, None); // null → None
        assert!(it.is_default);
    }
}
