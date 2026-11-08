//! Scenario Builder mount pool — "what this chat could see", before the chat
//! exists (v4 `lib/scenario-builder/mount-pool.ts`,
//! `resolveScenarioBuilderMountPool`).
//!
//! v4's header, carried (as of `0c47f4539`): the Scenario Builder runs a
//! character-less tool loop that must read exactly the stores a chat with this
//! cast (and project) would reach — every cast member's vault, the union of
//! every cast member's group stores, the project's stores, and Quilltap
//! General. Launched from a group's page, the run also names that group
//! outright (`groupIds`), whose stores join the group tier whether or not any
//! cast member belongs to it.
//!
//! That is a cast's reach, and a cast's reach is
//! [`resolve_cast_pool`](crate::db::reach::resolve_cast_pool); this module is
//! the Builder's name for it. The cast vaults sit in the participant tier and
//! `character_mount_point_id` is always `None`, so consumers flatten with
//! `include_participants: true`. Any tier whose lookup fails drops out; this
//! never fails.
//!
//! P4.D376: the module's own walk and its three `ScenarioBuilderMountPool`
//! lines are GONE (v4 deleted them), replaced by reach's — `[Reach] Archived
//! cast member contributes nothing to the pool` and `[Reach] Resolved cast
//! pool`, whose `namedGroupCount` is the RAW `groupIds` length — and the
//! project tier is the shared `resolve_project_mount_point_ids` (official
//! first; this module's private `Project mount lookup failed` WARN went with
//! it). v4 answers the whole `CastPool` typed as a `TieredMountPool`; v5 hands
//! back the pool half, since no consumer serialises it (the run's DEBUG reads
//! only its counts).

use rusqlite::Connection;

use crate::db::reach::{resolve_cast_pool, CastPoolOptions};
use crate::db::tiered_mount_pool::TieredMountPool;

/// v4 `resolveScenarioBuilderMountPool({ userId, projectId, characterIds,
/// groupIds })` — `return resolveCastPool(opts)`.
pub fn resolve_scenario_builder_mount_pool(
    main: &Connection,
    mount: &Connection,
    user_id: &str,
    project_id: Option<&str>,
    character_ids: &[String],
    group_ids: &[String],
) -> TieredMountPool {
    resolve_cast_pool(
        main,
        mount,
        &CastPoolOptions {
            user_id,
            project_id,
            character_ids,
            group_ids,
        },
    )
    .pool
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The pool DEBUG is reach's (P4.D376, v4 `0c47f4539`): `namedGroupCount`
    /// counts the RAW `groupIds` — `["g-1", "", "g-1"]` is THREE (P4.D231's pin
    /// said one, counted post-dedup; v4 at `624d0d25c` says three). The field
    /// ORDER is pinned over the rendered line (the tier-2 family compares the
    /// fields as a set).
    #[test]
    fn the_pool_line_is_reachs_and_counts_the_raw_named_groups() {
        let main = Connection::open_in_memory().unwrap();
        let mount = Connection::open_in_memory().unwrap();
        let named = vec!["g-1".to_string(), String::new(), "g-1".to_string()];
        let (_, lines) = crate::test_support::captured_with(|| {
            resolve_scenario_builder_mount_pool(&main, &mount, "u", None, &[], &named)
        });
        let line = lines
            .iter()
            .find(|l| l.contains("[Reach] Resolved cast pool"))
            .unwrap_or_else(|| panic!("{lines:?}"));
        assert!(
            line.starts_with("DEBUG quilltap_core::db::reach "),
            "{line}"
        );
        assert!(
            line.contains(
                " castCount=0 liveCastCount=0 namedGroupCount=3 participants=0 groups=0 projects=0 hasGlobal=false"
            ),
            "{line}"
        );
        assert!(
            !lines
                .iter()
                .any(|l| l.contains("Scenario Builder mount pool")),
            "the old line is gone: {lines:?}"
        );
    }
}
