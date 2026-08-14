//! The shared wardrobe tiers (v4 `lib/wardrobe/shared-tiers.ts`, `8600c83f`).
//!
//! The mounts that make up everything a character can wear *without owning it*:
//! their groups' stores and the chat's project stores. (Quilltap General is the
//! instance singleton — always in scope, never passed around.)
//!
//! One type and one resolver, so the tiers travel together. A call site that
//! threads only `project_mount_point_ids` is the bug this module exists to
//! prevent: items moved into a group's `Wardrobe/` folder were invisible to
//! every read path, so nobody could wear the household livery hanging by the
//! door.
//!
//! Precedence, mirroring [`dedupe_tier_triple`](crate::db::tiered_mount_pool::dedupe_tier_triple):
//! **character > group > project > general**.
//!
//! v4 states both fields optional so a caller with no chat/project context can
//! pass a partial object; v5 states them as plain (possibly empty) lists on one
//! struct, which is the same guarantee with the "forgot a tier" hole welded
//! shut — there is no way to construct the carrier that names only one tier
//! without saying so.

use rusqlite::Connection;

use crate::db::tiered_mount_pool::resolve_group_mount_point_ids_for_character;
use crate::tools::wardrobe_shared::{
    resolve_project_mount_point_ids, resolve_project_mount_point_ids_for_chat,
};

/// The shared mounts in scope for one character's wardrobe (v4
/// `SharedWardrobeTiers` / `ResolvedSharedWardrobeTiers` — v5 has only the
/// resolved shape).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SharedWardrobeTiers {
    /// Group stores (official + linked) of every group the character belongs to.
    /// Resolved per character, never per chat: a character never gains a
    /// co-participant's group stores.
    pub group_mount_point_ids: Vec<String>,
    /// Document stores linked to the chat's project.
    pub project_mount_point_ids: Vec<String>,
}

impl SharedWardrobeTiers {
    /// v4 `noSharedWardrobeTiers()` — the no-context tiers: Quilltap General
    /// alone.
    pub fn none() -> Self {
        SharedWardrobeTiers::default()
    }

    /// The project tier alone, for the handful of call sites that genuinely have
    /// no character to key the group tier on (an instance-wide archetype read).
    /// **Not** a shortcut for "I didn't thread the group tier" — v4 keys the
    /// group tier on a character, so a site with a character in hand must go
    /// through [`shared_wardrobe_tiers_for_character`].
    pub fn project_only(project_mount_point_ids: &[String]) -> Self {
        SharedWardrobeTiers {
            group_mount_point_ids: Vec::new(),
            project_mount_point_ids: project_mount_point_ids.to_vec(),
        }
    }

    /// The mounts to fold over Quilltap General, **weakest tier first**, so a
    /// later mount's item shadows an earlier one: project, then group (v4's
    /// `[...projectMountPointIds, ...groupMountPointIds]`).
    pub fn scoped_mounts(&self) -> Vec<String> {
        let mut scoped = self.project_mount_point_ids.clone();
        scoped.extend(self.group_mount_point_ids.iter().cloned());
        scoped
    }
}

/// v4 `resolveSharedWardrobeTiersForChat(chatId, characterId)` — both shared
/// tiers for a character in a chat. Each tier fails soft to `[]` on its own (the
/// underlying resolvers swallow and log), so a missing chat or a character with
/// no group memberships simply narrows the pool.
pub fn resolve_shared_wardrobe_tiers_for_chat(
    main: &Connection,
    mount: &Connection,
    chat_id: &str,
    character_id: &str,
) -> SharedWardrobeTiers {
    SharedWardrobeTiers {
        group_mount_point_ids: resolve_group_mount_point_ids_for_character(
            main,
            mount,
            character_id,
        ),
        project_mount_point_ids: resolve_project_mount_point_ids_for_chat(main, mount, chat_id),
    }
}

/// v4 `resolveSharedWardrobeTiersForProject(projectId, characterId)` — both
/// shared tiers when the caller already holds the project id (chat creation, the
/// outfit composer) rather than a chat id.
pub fn resolve_shared_wardrobe_tiers_for_project(
    main: &Connection,
    mount: &Connection,
    project_id: Option<&str>,
    character_id: &str,
) -> SharedWardrobeTiers {
    SharedWardrobeTiers {
        group_mount_point_ids: resolve_group_mount_point_ids_for_character(
            main,
            mount,
            character_id,
        ),
        project_mount_point_ids: resolve_project_mount_point_ids(mount, project_id),
    }
}

/// v4 `sharedWardrobeTiersForCharacter(characterId, projectMountPointIds)` —
/// pair an already-resolved project tier with one character's group tier.
///
/// For the loops that resolve a whole cast against a single chat: the project
/// stores are the same for everyone and are fetched once by the caller, while
/// the group stores differ per character and must be fetched inside the loop.
pub fn shared_wardrobe_tiers_for_character(
    main: &Connection,
    mount: &Connection,
    character_id: &str,
    project_mount_point_ids: &[String],
) -> SharedWardrobeTiers {
    SharedWardrobeTiers {
        group_mount_point_ids: resolve_group_mount_point_ids_for_character(
            main,
            mount,
            character_id,
        ),
        project_mount_point_ids: project_mount_point_ids.to_vec(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scoped_mounts_puts_the_project_tier_first_so_group_shadows_it() {
        let tiers = SharedWardrobeTiers {
            group_mount_point_ids: vec!["g1".into(), "g2".into()],
            project_mount_point_ids: vec!["p1".into()],
        };
        assert_eq!(tiers.scoped_mounts(), vec!["p1", "g1", "g2"]);
    }

    #[test]
    fn no_tiers_is_general_alone() {
        assert!(SharedWardrobeTiers::none().scoped_mounts().is_empty());
    }
}
