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
//! answers `pool.tiers`. The `4a38222e6` round's unification deleted the
//! `LANE-REMOVE` resolver wrappers and the helpers only they used
//! (`SharedWardrobeTierOptions` — now the pool's `operator` option — and the
//! `none` / `project_only` / `scoped_mounts` constructors); what is left is the
//! carrier itself.
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
