//! The canonical scoped-tier dedup — v4 `lib/mount-index/tiered-mount-pool.ts`
//! `dedupeTierTriple`.
//!
//! A mount must never enter more than one tier; precedence among the scoped tiers
//! is **character > group > project > global**. This is the ONE place the scoped
//! dedup rule is implemented in v4 (every resolver and the knowledge injector funnel
//! through it), so the priority can't drift. Only this pure leaf is ported here —
//! the DB-reading resolvers (`resolveTieredMountPool`, etc.) belong to the tier
//! subsystem and nothing in this wave consumes them.
//!
//! **NOTE:** lives under the knowledge injector because it is its only consumer
//! today; may be promoted to a shared module later.

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

/// Canonical dedup for the scoped-tier set (v4 `dedupeTierTriple`).
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
}
