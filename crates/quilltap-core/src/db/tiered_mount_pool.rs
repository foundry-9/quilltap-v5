//! Tiered Mount Pool — v4 `lib/mount-index/tiered-mount-pool.ts`.
//!
//! Single source of truth for the "tri-tier" content pattern. Several features
//! read content from up to five tiers of document stores, ranked by how "close"
//! the store is to the responding character:
//!
//!   1. **character** — the responding character's own vault
//!   2. **participant** — the vaults of the OTHER characters present in a chat
//!      (multi-character document access only; not a search/knowledge tier)
//!   3. **group** — the official + linked stores of every group the RESPONDING
//!      character is a member of (per-character, NOT per-chat — a character never
//!      gains a co-participant's group stores)
//!   4. **project** — every store linked to the active chat's project
//!   5. **global** — the singleton "Quilltap General" mount
//!
//! Knowledge injection, the scriptorium search tool, wardrobe resolution, and the
//! document-edit path resolver all funnel through this module so:
//!   - the dedup order is defined exactly once ([`dedupe_tier_triple`]);
//!   - the resolution (DB lookups, ownership gate, participant vaults, graceful
//!     global-null) lives in one place ([`resolve_tiered_mount_pool`]);
//!   - features test mount membership uniformly ([`classify_mount_tier`],
//!     [`flatten_tier_pool`]).
//!
//! ## Connections
//!
//! v4's `resolveTieredMountPool` uses `getRepositories()`, which spans the main +
//! mount-index databases. In the Rust read path each repo read takes a
//! `&Connection`, so the resolver takes BOTH: the `characters` slim/overlay read,
//! the `groups` officialMountPointId pointer, and `instance_settings` live in the
//! MAIN db; `group_character_members` / `group_doc_mount_links` /
//! `project_doc_mount_links` live in the MOUNT-INDEX db.
//!
//! ## Fails soft
//!
//! Every tier degrades gracefully: a failed lookup (or an unprovisioned mount)
//! yields an empty/null tier rather than throwing. v4 wraps each lookup in
//! try/catch + logs; the port swallows the error to `None`/`vec![]` (no logger in
//! the core — the corpus never exercises the log side effect, only the tier
//! result). The one exception faithfully preserved: the ownership-gate /
//! participant / non-fast-path character read goes through the OVERLAID
//! `characters_read::find_by_id` (v4 `findById`), and any error there (e.g. a
//! broken vault) is swallowed to a dropped tier — matching v4's try/catch.

use rusqlite::Connection;
use serde::Serialize;

use super::group_character_members::GroupCharacterMembersRepository;
use super::group_doc_mount_links::GroupDocMountLinksRepository;
use super::project_doc_mount_links::ProjectDocMountLinksRepository;
use super::{characters_read, groups, instance_settings};

/// Which tier a given mount point belongs to within a resolved pool (v4
/// `MountTier`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum MountTier {
    Character,
    Participant,
    Group,
    Project,
    Global,
}

/// The inputs needed to resolve a pool (v4 `TierContext`). Provide ids; the
/// resolver does the DB lookups.
#[derive(Debug, Clone, Default)]
pub struct TierContext {
    /// Calling user. Required only when `require_ownership` is set — the ownership
    /// gate fails closed (excludes the character vault) when this is absent.
    pub user_id: Option<String>,
    /// Responding character id — resolved to its vault mount.
    pub character_id: Option<String>,
    /// Pre-resolved responding-character vault mount. Fast path for callers that
    /// already hold the character object. IGNORED when `require_ownership` is set
    /// (ownership demands a fresh lookup).
    pub character_mount_point_id: Option<String>,
    /// Other character ids whose vaults should be admitted as the `participant`
    /// tier. Only consulted when `include_participants` is set.
    pub character_ids: Option<Vec<String>>,
    /// Active chat's project id — resolved to its linked store mounts.
    pub project_id: Option<String>,
}

/// Resolution options (v4 `TierResolveOptions`).
#[derive(Debug, Clone, Copy, Default)]
pub struct TierResolveOptions {
    /// Gate the character vault on ownership: only admit it when the character's
    /// `userId` matches the context `user_id`. Off by default.
    pub require_ownership: bool,
    /// Resolve `character_ids` into the `participant` tier.
    pub include_participants: bool,
}

/// A resolved, deduped pool (v4 `TieredMountPool`). Each mount appears in exactly
/// one bucket.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TieredMountPool {
    pub character_mount_point_id: Option<String>,
    pub participant_mount_point_ids: Vec<String>,
    pub group_mount_point_ids: Vec<String>,
    pub project_mount_point_ids: Vec<String>,
    pub global_mount_point_id: Option<String>,
}

/// The scoped-tier inputs (v4 `TierTriple`). `character_mount_point_id` /
/// `global_mount_point_id` are `Option`; the group/project lists default empty.
#[derive(Debug, Clone, Default)]
pub struct TierTriple {
    pub character_mount_point_id: Option<String>,
    pub group_mount_point_ids: Vec<String>,
    pub project_mount_point_ids: Vec<String>,
    pub global_mount_point_id: Option<String>,
}

/// The deduped output (v4's `Omit<TieredMountPool, 'participantMountPointIds'>`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DedupedTiers {
    pub character_mount_point_id: Option<String>,
    pub group_mount_point_ids: Vec<String>,
    pub project_mount_point_ids: Vec<String>,
    pub global_mount_point_id: Option<String>,
}

/// Canonical dedup for the scoped-tier set (v4 `dedupeTierTriple`). A mount must
/// never enter more than one tier; precedence among the scoped tiers is
/// **character > group > project > global**. (The participant tier is deduped
/// separately in [`resolve_tiered_mount_pool`] — it sits between character and
/// group in the overall ranking but is never subject to this triple's dedup.)
/// This is the ONE place the scoped dedup rule is implemented so the priority
/// can't drift.
pub fn dedupe_tier_triple(triple: TierTriple) -> DedupedTiers {
    let character_mount_point_id = triple.character_mount_point_id;

    // Group: closest after character. Drop any mount equal to the character vault;
    // dedup within the group set. (Empty strings dropped — v4's `if (!id) continue`.)
    let mut group_seen: Vec<String> = Vec::new();
    let mut group_mount_point_ids: Vec<String> = Vec::new();
    for id in triple.group_mount_point_ids {
        if id.is_empty() {
            continue;
        }
        if Some(&id) == character_mount_point_id.as_ref() {
            continue;
        }
        if group_seen.contains(&id) {
            continue;
        }
        group_seen.push(id.clone());
        group_mount_point_ids.push(id);
    }

    // Global: nulled when it collides with a closer tier (character or group). A
    // project collision is resolved by dropping the project mount (below).
    let mut global_mount_point_id = triple.global_mount_point_id;
    if let Some(g) = &global_mount_point_id {
        if Some(g) == character_mount_point_id.as_ref() || group_seen.contains(g) {
            global_mount_point_id = None;
        }
    }

    // Project: excludes character, every group mount, and global; dedup within.
    let mut seen: Vec<String> = Vec::new();
    let mut project_mount_point_ids: Vec<String> = Vec::new();
    for id in triple.project_mount_point_ids {
        if id.is_empty() {
            continue;
        }
        if Some(&id) == character_mount_point_id.as_ref() {
            continue;
        }
        if group_seen.contains(&id) {
            continue;
        }
        if Some(&id) == global_mount_point_id.as_ref() {
            continue;
        }
        if seen.contains(&id) {
            continue;
        }
        seen.push(id.clone());
        project_mount_point_ids.push(id);
    }

    DedupedTiers {
        character_mount_point_id,
        group_mount_point_ids,
        project_mount_point_ids,
        global_mount_point_id,
    }
}

/// Push `id` onto `list` only if not already present (insertion-ordered set).
fn push_unique(list: &mut Vec<String>, id: String) {
    if !list.contains(&id) {
        list.push(id);
    }
}

/// Read `characterDocumentMountPointId` off an overlaid character JSON value.
fn character_mount_of(v: &serde_json::Value) -> Option<String> {
    v.get("characterDocumentMountPointId")
        .and_then(|x| x.as_str())
        .map(String::from)
}

/// Resolve the group tier — the union of the official store and every linked
/// store across all groups the given character is a member of (v4
/// `resolveGroupMountPointIdsForCharacter`). Keyed on the RESPONDING character
/// (never the chat). Returns `[]` for a missing character id or on any lookup
/// failure (fails soft). Insertion order: per membership, the group's official
/// mount first, then its linked stores.
pub fn resolve_group_mount_point_ids_for_character(
    main: &Connection,
    mount: &Connection,
    character_id: &str,
) -> Vec<String> {
    if character_id.is_empty() {
        return Vec::new();
    }
    let memberships = match GroupCharacterMembersRepository::new(mount)
        .find_group_ids_by_character_id(character_id)
    {
        Ok(m) => m,
        Err(_) => return Vec::new(),
    };
    if memberships.is_empty() {
        return Vec::new();
    }
    let mut ids: Vec<String> = Vec::new();
    for group_id in memberships {
        // Per-membership try/catch (v4) — a failed lookup for one group is logged
        // and skipped, never aborting the whole resolution.
        let mut per_membership = || -> Result<(), super::DbError> {
            // findByIdRaw avoids a store read on this hot path — we only need the
            // group's officialMountPointId pointer, not its hydrated content.
            if let Some(Some(off)) = groups::find_official_mount_point_id_raw(main, &group_id)? {
                if !off.is_empty() {
                    push_unique(&mut ids, off);
                }
            }
            let links = GroupDocMountLinksRepository::new(mount).find_by_group_id(&group_id)?;
            for link in links {
                push_unique(&mut ids, link);
            }
            Ok(())
        };
        let _ = per_membership();
    }
    ids
}

/// Resolve the tri-tier mount pool for a context (v4 `resolveTieredMountPool`).
/// Does the DB lookups, applies the optional ownership gate + participant
/// inclusion, then dedups via [`dedupe_tier_triple`]. Degrades gracefully.
pub fn resolve_tiered_mount_pool(
    main: &Connection,
    mount: &Connection,
    ctx: &TierContext,
    opts: &TierResolveOptions,
) -> TieredMountPool {
    // 1. Responding character's vault (optionally ownership-gated).
    let mut character_mount_point_id: Option<String> = None;
    if opts.require_ownership {
        if let Some(cid) = &ctx.character_id {
            if let Ok(Some(character)) = characters_read::find_by_id(main, mount, cid) {
                let user_matches =
                    character.get("userId").and_then(|v| v.as_str()) == ctx.user_id.as_deref();
                if user_matches {
                    character_mount_point_id = character_mount_of(&character);
                }
            }
        }
    } else if let Some(mp) = &ctx.character_mount_point_id {
        character_mount_point_id = Some(mp.clone());
    } else if let Some(cid) = &ctx.character_id {
        if let Ok(Some(character)) = characters_read::find_by_id(main, mount, cid) {
            character_mount_point_id = character_mount_of(&character);
        }
    }

    // 2. Group stores — per RESPONDING character (keyed on ctx.character_id), never
    //    the chat. Fails soft.
    let group_mount_point_ids: Vec<String> = match &ctx.character_id {
        Some(cid) => resolve_group_mount_point_ids_for_character(main, mount, cid),
        None => Vec::new(),
    };

    // 3. Project-linked stores.
    let mut project_mount_point_ids: Vec<String> = Vec::new();
    if let Some(pid) = &ctx.project_id {
        if let Ok(links) = ProjectDocMountLinksRepository::new(mount).find_by_project_id(pid) {
            project_mount_point_ids = links;
        }
    }

    // 4. Quilltap General singleton (null during the pre-provisioning window).
    let global_mount_point_id = instance_settings::get_general_mount_point_id(main)
        .ok()
        .flatten();

    // 5. Canonical dedup of the scoped tiers (character > group > project > global).
    let deduped = dedupe_tier_triple(TierTriple {
        character_mount_point_id,
        group_mount_point_ids,
        project_mount_point_ids,
        global_mount_point_id,
    });

    // 6. Participant vaults — admitted into their own tier, excluded from every
    //    other resolved tier (including group) so each mount classifies into
    //    exactly one bucket.
    let mut participant_mount_point_ids: Vec<String> = Vec::new();
    if opts.include_participants {
        if let Some(character_ids) = &ctx.character_ids {
            if !character_ids.is_empty() {
                let mut excluded: Vec<String> = Vec::new();
                if let Some(c) = &deduped.character_mount_point_id {
                    excluded.push(c.clone());
                }
                excluded.extend(deduped.group_mount_point_ids.iter().cloned());
                excluded.extend(deduped.project_mount_point_ids.iter().cloned());
                if let Some(g) = &deduped.global_mount_point_id {
                    excluded.push(g.clone());
                }
                for character_id in character_ids {
                    if character_id.is_empty() {
                        continue;
                    }
                    if let Ok(Some(character)) =
                        characters_read::find_by_id(main, mount, character_id)
                    {
                        if let Some(mp) = character_mount_of(&character) {
                            if !excluded.contains(&mp) && !participant_mount_point_ids.contains(&mp)
                            {
                                participant_mount_point_ids.push(mp);
                            }
                        }
                    }
                }
            }
        }
    }

    TieredMountPool {
        character_mount_point_id: deduped.character_mount_point_id,
        participant_mount_point_ids,
        group_mount_point_ids: deduped.group_mount_point_ids,
        project_mount_point_ids: deduped.project_mount_point_ids,
        global_mount_point_id: deduped.global_mount_point_id,
    }
}

/// Which subset a [`flatten_tier_pool`] call selects (v4 `flattenTierPool`'s
/// `scope` option).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FlattenScope {
    #[default]
    All,
    Character,
    Group,
    Project,
}

/// Flatten a pool into a deduped id list (v4 `flattenTierPool`). `scope` narrows
/// the selection; `include_participants` folds the participant tier into the
/// character selection (the document path resolver wants it; search does not).
pub fn flatten_tier_pool(
    pool: &TieredMountPool,
    scope: FlattenScope,
    include_participants: bool,
) -> Vec<String> {
    let mut ids: Vec<String> = Vec::new();
    let add_character_tier = |ids: &mut Vec<String>| {
        if let Some(c) = &pool.character_mount_point_id {
            push_unique(ids, c.clone());
        }
        if include_participants {
            for id in &pool.participant_mount_point_ids {
                push_unique(ids, id.clone());
            }
        }
    };
    match scope {
        FlattenScope::Character => add_character_tier(&mut ids),
        FlattenScope::Group => {
            for id in &pool.group_mount_point_ids {
                push_unique(&mut ids, id.clone());
            }
        }
        FlattenScope::Project => {
            for id in &pool.project_mount_point_ids {
                push_unique(&mut ids, id.clone());
            }
        }
        FlattenScope::All => {
            add_character_tier(&mut ids);
            for id in &pool.group_mount_point_ids {
                push_unique(&mut ids, id.clone());
            }
            for id in &pool.project_mount_point_ids {
                push_unique(&mut ids, id.clone());
            }
            if let Some(g) = &pool.global_mount_point_id {
                push_unique(&mut ids, g.clone());
            }
        }
    }
    ids
}

/// Classify which tier a mount belongs to within a resolved pool, or `None` when
/// the mount is outside the pool (v4 `classifyMountTier`). Precedence: character >
/// participant > group > project > global.
pub fn classify_mount_tier(mount_point_id: &str, pool: &TieredMountPool) -> Option<MountTier> {
    if pool.character_mount_point_id.as_deref() == Some(mount_point_id) {
        return Some(MountTier::Character);
    }
    if pool
        .participant_mount_point_ids
        .iter()
        .any(|id| id == mount_point_id)
    {
        return Some(MountTier::Participant);
    }
    if pool
        .group_mount_point_ids
        .iter()
        .any(|id| id == mount_point_id)
    {
        return Some(MountTier::Group);
    }
    if pool
        .project_mount_point_ids
        .iter()
        .any(|id| id == mount_point_id)
    {
        return Some(MountTier::Project);
    }
    if pool.global_mount_point_id.as_deref() == Some(mount_point_id) {
        return Some(MountTier::Global);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn precedence_character_over_group_over_project_over_global() {
        let out = dedupe_tier_triple(TierTriple {
            character_mount_point_id: Some("c".into()),
            group_mount_point_ids: s(&["c", "g1", "g1", "g2"]),
            project_mount_point_ids: s(&["c", "g1", "p1", "p1", "glob"]),
            global_mount_point_id: Some("glob".into()),
        });
        assert_eq!(out.character_mount_point_id, Some("c".into()));
        assert_eq!(out.group_mount_point_ids, s(&["g1", "g2"]));
        // glob survives (not in character/group), so project drops it.
        assert_eq!(out.project_mount_point_ids, s(&["p1"]));
        assert_eq!(out.global_mount_point_id, Some("glob".into()));
    }

    #[test]
    fn global_nulled_when_it_collides_with_group() {
        let out = dedupe_tier_triple(TierTriple {
            character_mount_point_id: None,
            group_mount_point_ids: s(&["g1"]),
            project_mount_point_ids: vec![],
            global_mount_point_id: Some("g1".into()),
        });
        assert_eq!(out.global_mount_point_id, None);
    }

    #[test]
    fn flatten_and_classify() {
        let pool = TieredMountPool {
            character_mount_point_id: Some("c".into()),
            participant_mount_point_ids: s(&["p1", "p2"]),
            group_mount_point_ids: s(&["g1"]),
            project_mount_point_ids: s(&["pr1"]),
            global_mount_point_id: Some("glob".into()),
        };
        assert_eq!(
            flatten_tier_pool(&pool, FlattenScope::All, false),
            s(&["c", "g1", "pr1", "glob"])
        );
        assert_eq!(
            flatten_tier_pool(&pool, FlattenScope::All, true),
            s(&["c", "p1", "p2", "g1", "pr1", "glob"])
        );
        assert_eq!(
            flatten_tier_pool(&pool, FlattenScope::Character, true),
            s(&["c", "p1", "p2"])
        );
        assert_eq!(classify_mount_tier("c", &pool), Some(MountTier::Character));
        assert_eq!(
            classify_mount_tier("p2", &pool),
            Some(MountTier::Participant)
        );
        assert_eq!(classify_mount_tier("g1", &pool), Some(MountTier::Group));
        assert_eq!(classify_mount_tier("pr1", &pool), Some(MountTier::Project));
        assert_eq!(classify_mount_tier("glob", &pool), Some(MountTier::Global));
        assert_eq!(classify_mount_tier("nope", &pool), None);
    }
}
