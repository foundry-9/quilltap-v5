//! The inner circle — who a character holds closest, read from their groups
//! (v4 `lib/memory/recall/inner-circle.ts`, `a5efdbb97`, at the `236c04e35`
//! pin; P4.D342).
//!
//! A holder's inner circle is every character who shares at least one group
//! with them, less the holder, archived characters (tombstones) and whoever the
//! caller excludes (the frozen archive excludes everyone present, whom its
//! first step already covers). Each member takes the **tier** of the smallest
//! group they share with the holder: the holder's distinct group sizes,
//! ascending, are tiers 0, 1, 2, … — ranks, not raw sizes, so a family of four
//! is tier 0 whether the next group up has six members or sixty.
//!
//! v4's why, carried: "The order is deterministic — tier, then group name,
//! then member name, then id — because the frozen archive renders the circle
//! in this order and must stay byte-stable. [`inner_circle_signature`] turns it
//! into the archive cache's freshness check, so a membership change, a rename
//! that reorders the circle, or an archived member reaches the next turn
//! without an explicit invalidation."
//!
//! Membership lives in the MOUNT-INDEX DB; group and character names are plain
//! main-partition columns read without the vault overlay, so a broken vault
//! costs a member, never the turn. Design of record (vendored under
//! `docs/v4/`): `docs/developer/features/complete/inner-circle-recall.md`
//! (Part B).
//!
//! ## Failure legs (the repository-fallback class)
//!
//! v4's two membership reads (`findByCharacterId` / `findByGroupId`) each wrap
//! the base `findByFilter` in their own `safeQuery`, and `findByFilter` is
//! itself a fallback: a failed read logs `Error finding entities by filter
//! {collection: group_character_members}` and answers `[]` BEFORE the outer
//! `Error finding group memberships by …` line can fire — that outer line is
//! UNREACHABLE on a database failure (the P4.142 class, `db::fallback::
//! find_by_filter_or_empty`'s own doc). A degraded or absent mount index
//! answers the same empty read. The function therefore never fails.
//!
//! ## Lines
//!
//! - DEBUG `[InnerCircle] Resolved inner circle` `{holderId, groups, tiers,
//!   excluded, members}` — only when the circle was actually built: NOT on the
//!   two early returns (no memberships; nobody left after the exclusions).

use std::collections::HashSet;

use crate::collation::locale_compare;
use crate::db::group_character_members::GroupCharacterMembersRepository;
use crate::db::runtime::Db;

/// One inner-circle member (v4 `InnerCircleMember`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InnerCircleMember {
    pub character_id: String,
    pub name: String,
    /// Rank of the tightest shared group: 0 = the holder's smallest group.
    pub tier: i64,
    /// Size of that group, for the trace.
    pub group_size: usize,
    pub group_name: String,
}

/// v4 `SharedGroup` + its roster.
struct SharedGroup {
    group_id: String,
    group_name: String,
    size: usize,
    roster: Vec<String>,
}

/// v4 `[...new Set(ids)]` — first-seen dedupe.
fn dedup_first_seen(ids: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    ids.into_iter()
        .filter(|id| seen.insert(id.clone()))
        .collect()
}

/// A membership read through v4's `findByFilter` fallback (see the module
/// doc): any failure — a SQLite error, a degraded or absent mount index —
/// logs `Error finding entities by filter {collection:
/// group_character_members}` and answers `[]`.
fn membership_read(
    db: &Db,
    read: impl FnOnce(&GroupCharacterMembersRepository<'_>) -> Result<Vec<String>, crate::db::DbError>,
) -> Vec<String> {
    crate::db::fallback::find_by_filter_or_empty("group_character_members", || {
        db.read_mount_index(|mount| read(&GroupCharacterMembersRepository::new(mount)))
    })
}

/// Resolve a holder's inner circle, in circle order (v4 `resolveInnerCircle`).
/// Empty when the holder belongs to no group (or the mount index is
/// unavailable — its reads degrade to empty).
pub fn resolve_inner_circle(
    db: &Db,
    holder_id: &str,
    exclude_ids: &HashSet<String>,
) -> Vec<InnerCircleMember> {
    let memberships = membership_read(db, |repo| repo.find_group_ids_by_character_id(holder_id));
    if memberships.is_empty() {
        return Vec::new();
    }

    // Every group the holder is in, with its roster and its name.
    let mut groups: Vec<SharedGroup> = Vec::new();
    for group_id in dedup_first_seen(memberships) {
        // v4 `findByIdRaw` — `null` on a missing row (and on a failed read:
        // the store-owner raw read is itself a fallback).
        let gid = group_id.clone();
        let Some(group) = db
            .read_main(move |main| Ok(crate::db::groups::find_by_id_raw_or_none(main, &gid)))
            .ok()
            .flatten()
        else {
            continue; // a membership row left behind by a deleted group
        };
        let roster = dedup_first_seen(membership_read(db, |repo| {
            repo.find_character_ids_by_group_id(&group_id)
        }));
        // v4 `(group.name ?? '').trim()`. A non-string, non-null name would
        // THROW in v4 (`.trim` of a number) — unreachable through v4's own
        // writer (the Zod group schema), so it is not special-cased: it reads
        // as `''` here.
        let group_name = group
            .get("name")
            .and_then(serde_json::Value::as_str)
            .map(crate::jsstr::js_trim)
            .unwrap_or("")
            .to_string();
        groups.push(SharedGroup {
            group_id,
            group_name,
            size: roster.len(),
            roster,
        });
    }
    let mut sizes: Vec<usize> = Vec::new();
    for g in &groups {
        if !sizes.contains(&g.size) {
            sizes.push(g.size);
        }
    }
    sizes.sort_unstable();
    let tier_of = |size: usize| -> i64 {
        sizes
            .iter()
            .position(|s| *s == size)
            .map_or(-1, |p| p as i64)
    };

    // Each candidate's tightest shared group — v4's insertion-ordered `Map`
    // (a later, tighter group REPLACES the value but keeps the key's slot).
    let mut tightest: Vec<(String, usize)> = Vec::new();
    for (gi, group) in groups.iter().enumerate() {
        for character_id in &group.roster {
            if character_id == holder_id || exclude_ids.contains(character_id) {
                continue;
            }
            match tightest.iter_mut().find(|(id, _)| id == character_id) {
                Some(slot) => {
                    if compare_groups(group, &groups[slot.1]).is_lt() {
                        slot.1 = gi;
                    }
                }
                None => tightest.push((character_id.clone(), gi)),
            }
        }
    }
    if tightest.is_empty() {
        return Vec::new();
    }

    // Names, with archived characters (and rows that no longer exist) left out.
    let ids: Vec<String> = tightest.iter().map(|(id, _)| id.clone()).collect();
    let names = db
        .read_main(move |main| {
            Ok(crate::db::characters_read::find_names_by_ids_with(
                main, &ids, true,
            ))
        })
        .unwrap_or_default();
    let mut circle: Vec<InnerCircleMember> = Vec::new();
    for (character_id, gi) in &tightest {
        let Some(name) = names.get(character_id) else {
            continue;
        };
        let group = &groups[*gi];
        circle.push(InnerCircleMember {
            character_id: character_id.clone(),
            name: name.clone(),
            tier: tier_of(group.size),
            group_size: group.size,
            group_name: group.group_name.clone(),
        });
    }
    // Stable, as V8's `Array.prototype.sort` is (the comparator is total).
    circle.sort_by(|a, b| {
        a.tier
            .cmp(&b.tier)
            .then_with(|| locale_compare(&a.group_name, &b.group_name))
            .then_with(|| locale_compare(&a.name, &b.name))
            .then_with(|| locale_compare(&a.character_id, &b.character_id))
    });

    tracing::debug!(
        target: "quilltap::memory",
        holderId = %holder_id,
        groups = groups.len() as u64,
        tiers = sizes.len() as u64,
        excluded = exclude_ids.len() as u64,
        members = circle.len() as u64,
        "[InnerCircle] Resolved inner circle"
    );
    circle
}

/// v4 `compareGroups`: the smaller group first; equal sizes by name, then id.
fn compare_groups(a: &SharedGroup, b: &SharedGroup) -> std::cmp::Ordering {
    a.size
        .cmp(&b.size)
        .then_with(|| locale_compare(&a.group_name, &b.group_name))
        .then_with(|| locale_compare(&a.group_id, &b.group_id))
}

/// The circle as the archive cache compares it: members and tiers in circle
/// order (v4 `innerCircleSignature`). Order is kept (not sorted) because the
/// archive renders in it.
pub fn inner_circle_signature(circle: &[InnerCircleMember]) -> String {
    circle
        .iter()
        .map(|m| format!("{}:{}", m.character_id, m.tier))
        .collect::<Vec<_>>()
        .join(",")
}

/// A tier's per-member digest allowance; the table's last entry covers every
/// looser tier (v4 `innerCircleAllowanceFor`). A circle member's tier is never
/// negative (only the archive's LIFTED keystones carry `-1`, and they bypass
/// the draw); a negative tier would make v4 index `allowance[-1]` →
/// `undefined ?? 0` → 0, which the `None` arm reproduces.
pub fn inner_circle_allowance_for(tier: i64, allowance: &[i64]) -> i64 {
    if allowance.is_empty() {
        return 0;
    }
    let idx = tier.min(allowance.len() as i64 - 1);
    let v = usize::try_from(idx)
        .ok()
        .and_then(|i| allowance.get(i))
        .copied()
        .unwrap_or(0);
    v.max(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allowance_for_reads_the_table_and_its_last_entry() {
        let a = [3, 2, 1];
        assert_eq!(inner_circle_allowance_for(0, &a), 3);
        assert_eq!(inner_circle_allowance_for(1, &a), 2);
        assert_eq!(inner_circle_allowance_for(2, &a), 1);
        assert_eq!(inner_circle_allowance_for(7, &a), 1);
        assert_eq!(inner_circle_allowance_for(0, &[]), 0);
        assert_eq!(inner_circle_allowance_for(-1, &a), 0);
        assert_eq!(inner_circle_allowance_for(0, &[0]), 0);
    }

    #[test]
    fn signature_is_member_and_tier_in_circle_order() {
        let m = |id: &str, tier| InnerCircleMember {
            character_id: id.into(),
            name: id.into(),
            tier,
            group_size: 3,
            group_name: "Family".into(),
        };
        assert_eq!(inner_circle_signature(&[m("b", 0), m("a", 1)]), "b:0,a:1");
        assert_eq!(inner_circle_signature(&[]), "");
    }
}
