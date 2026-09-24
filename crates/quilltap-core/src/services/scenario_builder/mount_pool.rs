//! Scenario Builder mount pool — "what this chat could see", before the chat
//! exists (v4 `lib/scenario-builder/mount-pool.ts`, `d1c06cd9d`,
//! `resolveScenarioBuilderMountPool`).
//!
//! v4's own header, carried: the Scenario Builder runs a character-less tool
//! loop that must read exactly the stores a chat with this cast (and project)
//! would reach — every cast member's vault, the union of every cast member's
//! group stores, the project's stores, and Quilltap General.
//! [`resolve_tiered_mount_pool`](crate::db::tiered_mount_pool::resolve_tiered_mount_pool)
//! cannot express that (its group tier is keyed on a single responding
//! character), so the pool is assembled here by hand from the same per-tier
//! helpers, in precedence order, and deduped with the same rule
//! ([`dedupe_tier_triple`]). The cast vaults sit in the PARTICIPANT tier; there
//! is no acting character, so `character_mount_point_id` is always `None`.
//! Consumers flatten with `include_participants: true`. Any tier whose lookup
//! fails drops out with a `warn`; this never fails.
//!
//! ## The reads, and one that is v5's by necessity
//!
//! - The cast is read RAW (`characters_read::find_by_id_raw` — no vault
//!   overlay): v4 wants only the vault pointer, the owner and the archive flag,
//!   and "a broken vault overlay must cost a tier, not the run". (The ROUTE's
//!   cast scoping reads the OVERLAID, user-scoped `findById` — two different
//!   reads over the same ids, both kept; §R.4(f).)
//! - The project tier is v4's `resolveProjectMountPointIds`, which v5 never
//!   grew as a function — `resolve_tiered_mount_pool` inlines the same
//!   `ProjectDocMountLinksRepository::find_by_project_id` read. That read is
//!   inlined here too ([`project_mount_point_ids`]), with v4's fail-soft WARN
//!   (`db/**` is outside this lane's ownership, so the helper is not hoisted).
//! - Quilltap General is `get_general_mount_point_id`. **Its WARN arm is
//!   unreachable on BOTH sides**: v4's `readSetting` catches its own failure
//!   (logging `[InstanceSettings] Failed to read setting` and answering `null`),
//!   and v5's `read_setting` answers `None` on any failure. The family's
//!   poisoned-settings arm measures exactly that (both pools lose the global
//!   tier; neither pool logger warns). v4's `[InstanceSettings]` WARN has NO v5
//!   emitter — a pre-existing absence in `db/instance_settings.rs`, recorded
//!   for the unifier in the P4.D217 lane record, not taken here.
//!
//! ## The four log lines (logger `ScenarioBuilderMountPool`) — two unreachable
//!
//! DEBUG `Archived cast member contributes nothing to the pool` `{ characterId }`
//! and DEBUG `Resolved Scenario Builder mount pool` `{ castCount,
//! liveCastCount, participants, groups, projects, hasGlobal }` are ported and
//! pinned. v4's two WARNs are UNREACHABLE through v4's real code, measured:
//! `Cast vault lookup failed; tier dropped for this character` sits behind
//! `findByIdRaw`, a fallback-mode `safeQuery` that never throws (see the read
//! loop), and `Quilltap General lookup failed; global tier dropped` sits
//! behind `readSetting`, which catches its own failure (see above). v5 keeps
//! the General arm's shape (its reader is `Result`-typed) and reproduces the
//! cast read's `safeQuery` fallback in place of its WARN. Both unreachable
//! lines are recorded in the P4.D217 lane record; v4's `mount-pool.test.ts`
//! reaches them only by mocking the repositories to throw.

use rusqlite::Connection;
use serde_json::Value;

use crate::db::characters_read;
use crate::db::instance_settings::get_general_mount_point_id;
use crate::db::project_doc_mount_links::ProjectDocMountLinksRepository;
use crate::db::tiered_mount_pool::{
    dedupe_tier_triple, resolve_group_mount_point_ids_for_character, TierTriple, TieredMountPool,
};

/// Push `id` onto `list` unless already present — JS `[...new Set(xs)]`
/// (insertion-ordered).
fn push_unique(list: &mut Vec<String>, id: &str) {
    if !list.iter().any(|x| x == id) {
        list.push(id.to_string());
    }
}

/// v4 `resolveProjectMountPointIds(projectId)` — `[]` for a missing id or a
/// failed lookup (WARN `Project mount lookup failed`, v4's `TieredMountPool`
/// logger).
fn project_mount_point_ids(mount: &Connection, project_id: Option<&str>) -> Vec<String> {
    let Some(project_id) = project_id.filter(|p| !p.is_empty()) else {
        return Vec::new();
    };
    match ProjectDocMountLinksRepository::new(mount).find_by_project_id(project_id) {
        Ok(ids) => ids,
        Err(e) => {
            tracing::warn!(projectId = %project_id, error = %e, "Project mount lookup failed");
            Vec::new()
        }
    }
}

/// v4 `resolveScenarioBuilderMountPool({ userId, projectId, characterIds })`.
pub fn resolve_scenario_builder_mount_pool(
    main: &Connection,
    mount: &Connection,
    user_id: &str,
    project_id: Option<&str>,
    character_ids: &[String],
) -> TieredMountPool {
    // `[...new Set(characterIds.filter(Boolean))]`
    let mut cast_ids: Vec<String> = Vec::new();
    for id in character_ids.iter().filter(|id| !id.is_empty()) {
        push_unique(&mut cast_ids, id);
    }

    // 1. Cast vaults. Raw reads: only the vault pointer, the owner and the
    //    archive flag are wanted, and a broken vault overlay must cost a tier,
    //    not the run. An archived character is a tombstone and contributes
    //    nothing — never `ensureCharacterVault` one here.
    let mut live_cast_ids: Vec<String> = Vec::new();
    let mut vault_ids: Vec<String> = Vec::new();
    for character_id in &cast_ids {
        let character: Value = match characters_read::find_by_id_raw(main, character_id) {
            Ok(Some(c)) => c,
            Ok(None) => continue,
            // v4's `findByIdRaw` is `_findById`, i.e. `safeQuery(…, 'Error
            // finding entity by ID', { id }, null)` in FALLBACK mode
            // (`base.repository.ts:236-246`): a failed read — a row that no
            // longer decodes included — logs that line at the repository and
            // answers `null`, so v4's pool skips the member SILENTLY and its own
            // `Cast vault lookup failed` WARN never fires. v5's raw read
            // surfaces the error instead, so the fallback is reproduced here
            // (the `api::chat_media` precedent), not the unreachable WARN.
            // Measured: the family's `unreadable-member-warns` arm (a BLOB
            // `name`) — v4's pool logger is silent.
            Err(e) => {
                tracing::error!(
                    collection = "characters",
                    id = %character_id,
                    error = %e,
                    "Error finding entity by ID"
                );
                continue;
            }
        };
        let str_of = |k: &str| character.get(k).and_then(Value::as_str);
        // `character.userId && character.userId !== userId` — an empty owner
        // is falsy and admitted.
        if let Some(owner) = str_of("userId").filter(|o| !o.is_empty()) {
            if owner != user_id {
                continue;
            }
        }
        if str_of("archivedAt").is_some_and(|a| !a.is_empty()) {
            tracing::debug!(
                characterId = %character_id,
                "Archived cast member contributes nothing to the pool"
            );
            continue;
        }
        live_cast_ids.push(character_id.clone());
        if let Some(vault) = str_of("characterDocumentMountPointId").filter(|v| !v.is_empty()) {
            vault_ids.push(vault.to_string());
        }
    }

    // 2. Group stores — the union over the whole cast (the helper fails soft).
    let mut group_ids: Vec<String> = Vec::new();
    for character_id in &live_cast_ids {
        group_ids.extend(resolve_group_mount_point_ids_for_character(
            main,
            mount,
            character_id,
        ));
    }

    // 3. Project stores (fails soft to []).
    let project_ids = project_mount_point_ids(mount, project_id);

    // 4. Quilltap General (`None` during the pre-provisioning window).
    let global_mount_point_id = match get_general_mount_point_id(main) {
        Ok(g) => g,
        Err(e) => {
            tracing::warn!(error = %e, "Quilltap General lookup failed; global tier dropped");
            None
        }
    };

    // 5. Scoped-tier dedup, then participants excluded from every scoped tier
    //    so each mount classifies into exactly one bucket.
    let deduped = dedupe_tier_triple(TierTriple {
        character_mount_point_id: None,
        group_mount_point_ids: group_ids,
        project_mount_point_ids: project_ids,
        global_mount_point_id,
    });
    let excluded = |id: &String| {
        deduped.group_mount_point_ids.contains(id)
            || deduped.project_mount_point_ids.contains(id)
            || deduped.global_mount_point_id.as_ref() == Some(id)
    };
    let mut participant_mount_point_ids: Vec<String> = Vec::new();
    for id in &vault_ids {
        push_unique(&mut participant_mount_point_ids, id);
    }
    participant_mount_point_ids.retain(|id| !excluded(id));

    let pool = TieredMountPool {
        character_mount_point_id: deduped.character_mount_point_id,
        participant_mount_point_ids,
        group_mount_point_ids: deduped.group_mount_point_ids,
        project_mount_point_ids: deduped.project_mount_point_ids,
        global_mount_point_id: deduped.global_mount_point_id,
    };
    tracing::debug!(
        castCount = cast_ids.len(),
        liveCastCount = live_cast_ids.len(),
        participants = pool.participant_mount_point_ids.len(),
        groups = pool.group_mount_point_ids.len(),
        projects = pool.project_mount_point_ids.len(),
        hasGlobal = pool.global_mount_point_id.is_some(),
        "Resolved Scenario Builder mount pool"
    );
    pool
}
