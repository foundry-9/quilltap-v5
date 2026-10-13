//! The shared wardrobe tiers — the carrier (v4 `WardrobeTierMounts`,
//! `lib/wardrobe/pool.ts:40-45` at `4a38222e6`; born as `SharedWardrobeTiers`
//! in `shared-tiers.ts`, `8600c83f`).
//!
//! The mounts that make up everything a character can wear *without owning it*:
//! their groups' stores and the chat's project stores. (Quilltap General is the
//! instance singleton — always in scope, never passed around.)
//!
//! One type, so the tiers travel together. A call site that threads only
//! `project_mount_point_ids` is the bug this type exists to prevent: items
//! moved into a group's `Wardrobe/` folder were invisible to every read path,
//! so nobody could wear the household livery hanging by the door.
//!
//! Precedence, mirroring [`dedupe_tier_triple`](crate::db::tiered_mount_pool::dedupe_tier_triple):
//! **character > group > project > general**.
//!
//! **`9ecebf986` deleted `shared-tiers.ts`.** Its two resolvers became the
//! wearable pool's (`crate::wearable_pool`): `resolveProjectTierForChat` moved
//! there whole (roster-gated, tag `[WearablePool]`), and "both tiers for a
//! character in a chat" IS `loadWearablePool(repos, characterId, undefined,
//! { chatId, operator })` — the pool resolves the group tier itself and
//! answers `pool.tiers`. The two resolver fns below are kept ONLY as
//! `LANE-REMOVE` wrappers over the pool so the files of the `4a38222e6`
//! round's other lanes compile until each re-points its callers.
//!
//! v4 states both fields optional so a caller with no chat/project context can
//! pass a partial object; v5 states them as plain (possibly empty) lists on one
//! struct, which is the same guarantee with the "forgot a tier" hole welded
//! shut.

/// The shared mounts in scope for one character's wardrobe (v4
/// `WardrobeTierMounts` / the old `SharedWardrobeTiers` — v5 has only the
/// resolved shape). `crate::wearable_pool::WardrobeTierMounts` is this type.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SharedWardrobeTiers {
    /// Group stores (official + linked) of every group the character belongs to,
    /// weakest first. Resolved per character, never per chat: a character never
    /// gains a co-participant's group stores.
    pub group_mount_point_ids: Vec<String>,
    /// Document stores linked to the chat's project.
    pub project_mount_point_ids: Vec<String>,
}

impl SharedWardrobeTiers {
    /// The no-context tiers: Quilltap General alone.
    pub fn none() -> Self {
        SharedWardrobeTiers::default()
    }

    /// The project tier alone, for the handful of call sites that genuinely have
    /// no character to key the group tier on (an instance-wide archetype read).
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

/// v4 `SharedWardrobeTierOptions` (`9753d0eb2`) — now the pool's
/// `LoadWearablePoolOptions.operator`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SharedWardrobeTierOptions {
    /// The human operator is choosing on the character's behalf (the Salon's
    /// outfit dialog), so the project roster does not apply. Character tool calls
    /// never set this.
    pub operator: bool,
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
