//! `memory_retire` — the ONE way a memory row is deleted. P4.D288 (C1-7),
//! porting v4 `lib/memory/retire.ts` (`af48f6cc2`, memory refactor Phase B,
//! with bugs 200 / 201 / 225 / 234) at the `83334b9a5` pin.
//!
//! "A memory is named from three link columns on other rows, and its vector
//! is cached in its character's index. A bare row delete leaves all four
//! behind: an orphan vector at rank 1 blinded the gate (bug 200), and a
//! deleted id left in `consolidatedFrom` or `supersededById` overstated a
//! digest and stranded its members (bug 225). Every deletion therefore comes
//! here, and here alone calls `MemoriesRepository.bulkDeleteUnsafe`." In
//! order, [`retire_memories`]:
//!
//!   1. resolves the requested ids to rows and, under `keep_manual`, spares
//!      what a policy path must never remove (MANUAL rows, cold rows, and a
//!      digest while any of its members survives — decision 4, bug 201);
//!   2. retires, with its last member, any digest whose `consolidatedFrom` the
//!      retirement would empty (invariant 8) — except under
//!      [`RetireReason::Retention`], whose purpose is to let a digest outlive
//!      the cold members it replaced;
//!   3. scrubs the doomed ids from every row's `relatedMemoryIds` and from
//!      surviving digests' `consolidatedFrom`, and cuts `supersededById` on
//!      surviving members of a doomed digest — all as SQL removal deltas, so
//!      two retirements compose (bug 234);
//!   4. deletes the rows ([`MemoriesRepository::bulk_delete_unsafe`]) and
//!      evicts their vectors ([`memory_index::remove`]).
//!
//! **R-AF — the effects run AFTER the commit.** v4's step 5 (invalidate the
//! holders' frozen archives, publish `memories`, refresh the Commonplace
//! mirror of any digest removed) runs in v4's parent once the rows are gone;
//! here [`retire_memories`] runs on the WRITER's connection inside the
//! caller's `Db::write` closure, so it only RECORDS what to announce
//! ([`RetireReport::announcement`]) and the caller hands that to
//! [`announce_retirement`] once the closure has committed. v4's job-child arm
//! (`Retired in the job child; the dispatcher announces after commit`) is
//! unreachable — v5 has no child — and so is the mirror refresh's WARN
//! (`Commonplace mirror refresh after retiring a digest failed`): v4 raises it
//! only when the dynamic `import()` fails, and
//! [`mirror_commonplace_for_subjects`] never fails.
//!
//! **L-2.** `vectorsRemoved` is the LOADED answer — the rows with a stored
//! embedding among those deleted, read by [`memory_index::indexed_among`] in
//! the same closure BEFORE the delete (`MEMORY_INDEX_ALWAYS_LOADED`).
//!
//! Idempotent: ids already gone are scrubbed from links and otherwise ignored.
//! Every line carries v4's child-logger binding `module: 'memory:retire'`
//! first.

use std::collections::HashSet;

use rusqlite::Connection;
use serde_json::{json, Value};

use crate::db::memories::{MemoriesRepository, TierExtra};
use crate::db::memories_read::{
    self, find_digests_containing_members, find_ids_superseded_by_any, find_retire_rows,
    parse_json_id_list, DanglingMemoryLinks, DigestRef, RetireRow,
};
use crate::db::runtime::Db;
use crate::db::vector_store::Tier;
use crate::db::DbError;
use crate::policy_protection::{policy_protection_of, PolicyProtection, SparedBy};
use crate::realtime::bus::publish_realtime;
use crate::realtime::types::RealtimeTopic;
use crate::services::commonplace_mirror::mirror_commonplace_for_subjects;
use crate::services::frozen_archive::invalidate_frozen_archive;
use crate::services::memory_index;

/// v4's child-logger binding (`logger.child({ module: 'memory:retire' })`).
const MODULE: &str = "memory:retire";

/// v4 `NEIGHBOUR_WARN` — warn when one retirement rewrites more neighbours
/// than a cascade normally would (compared with `>=`).
const NEIGHBOUR_WARN: i64 = 200;

/// v4 `MAX_DIGEST_CASCADE_PASSES` — "a digest that retires with its last
/// member may itself be a member; bounded."
const MAX_DIGEST_CASCADE_PASSES: usize = 3;

/// v4 `RetireReason` (`retire.ts:49-71`) — why rows are being retired; logged,
/// and it decides the digest cascade.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RetireReason {
    /// `DELETE /memories/[id]` — the true purge of one row.
    ItemDelete,
    /// A chat's extracted memories wiped (`DELETE /memories?chatId=`,
    /// regenerate).
    ChatWipe,
    /// A deleted message, or a whole swipe group, took its memories with it.
    MessageCascade,
    /// A line re-attributed to another speaker drops what was extracted for
    /// the old one.
    MessageReattribution,
    /// A swipe re-roll discards the memories of the line it replaces.
    SwipeRegenerate,
    /// A character (and with it every memory it holds) deleted.
    CharacterDelete,
    /// A character archived: its memories travel in the bundle, not the
    /// database.
    CharacterArchive,
    /// A restore replacing the data it is about to write back.
    RestoreReplace,
    /// Housekeeping retention: superseded cold rows past `coldRetentionDays`.
    Retention,
    /// The near-duplicate fold removing the rows it folded into a survivor.
    NearDuplicateFold,
    /// The graph repair retiring a digest every one of whose members is
    /// already gone.
    GraphRepair,
}

impl RetireReason {
    /// v4's kebab literal.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ItemDelete => "item-delete",
            Self::ChatWipe => "chat-wipe",
            Self::MessageCascade => "message-cascade",
            Self::MessageReattribution => "message-reattribution",
            Self::SwipeRegenerate => "swipe-regenerate",
            Self::CharacterDelete => "character-delete",
            Self::CharacterArchive => "character-archive",
            Self::RestoreReplace => "restore-replace",
            Self::Retention => "retention",
            Self::NearDuplicateFold => "near-duplicate-fold",
            Self::GraphRepair => "graph-repair",
        }
    }
}

/// v4 `RetireOptions` — `{ reason, keep?: 'manual' }`. `keep_manual`: spare
/// what a policy path must never remove — MANUAL rows, cold rows, and a digest
/// while any of its members survives. Off, every requested row is retired
/// (the per-row purge, cascades of a whole owner).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetireOptions {
    pub reason: RetireReason,
    pub keep_manual: bool,
}

impl RetireOptions {
    /// `{ reason }` — nothing kept back.
    pub fn new(reason: RetireReason) -> Self {
        Self {
            reason,
            keep_manual: false,
        }
    }
}

/// What a committed retirement must announce ([`announce_retirement`]): the
/// holders whose frozen archives drop, and the Commonplace buckets (holder →
/// subject ids, first-seen order) of the digests it removed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RetirementAnnouncement {
    pub holders: Vec<String>,
    pub digest_buckets: Vec<(String, Vec<String>)>,
}

/// v4 `RetireReport` (the ten fields) plus the announcement v4 makes inline
/// (`None` when v4 announces nothing — `retired == 0`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RetireReport {
    pub requested: usize,
    /// Rows deleted.
    pub retired: i64,
    pub retired_ids: Vec<String>,
    /// Requested rows left in place under `keep_manual`.
    pub spared: usize,
    pub spared_by: SparedBy,
    /// Digests retired because the retirement emptied their
    /// `consolidatedFrom`.
    pub digests_retired_with_last_member: usize,
    pub vectors_removed: usize,
    pub neighbours_touched: i64,
    pub digest_links_scrubbed: usize,
    pub characters_affected: usize,
    /// R-AF: hand to [`announce_retirement`] after the commit.
    pub announcement: Option<RetirementAnnouncement>,
}

impl RetireReport {
    fn empty(requested: usize) -> Self {
        Self {
            requested,
            ..Default::default()
        }
    }

    /// v4's object, in its key order.
    pub fn to_json(&self) -> Value {
        json!({
            "requested": self.requested,
            "retired": self.retired,
            "retiredIds": self.retired_ids,
            "spared": self.spared,
            "sparedBy": self.spared_by.to_json(),
            "digestsRetiredWithLastMember": self.digests_retired_with_last_member,
            "vectorsRemoved": self.vectors_removed,
            "neighboursTouched": self.neighbours_touched,
            "digestLinksScrubbed": self.digest_links_scrubbed,
            "charactersAffected": self.characters_affected,
        })
    }
}

/// The doomed set in v4's `Map` insertion order.
#[derive(Default)]
struct Doomed {
    rows: Vec<RetireRow>,
    ids: HashSet<String>,
}

impl Doomed {
    fn insert(&mut self, row: RetireRow) {
        if self.ids.insert(row.id.clone()) {
            self.rows.push(row);
        } else if let Some(slot) = self.rows.iter_mut().find(|r| r.id == row.id) {
            // A JS `Map.set` on an existing key keeps the key's position and
            // replaces the value.
            *slot = row;
        }
    }

    fn keys(&self) -> Vec<String> {
        self.rows.iter().map(|r| r.id.clone()).collect()
    }

    fn len(&self) -> usize {
        self.rows.len()
    }
}

/// v4's `Array.from(new Set(xs))` — first-seen order.
fn dedupe<'a>(xs: impl IntoIterator<Item = &'a String>) -> Vec<String> {
    let mut seen = HashSet::new();
    xs.into_iter()
        .filter(|x| seen.insert(x.as_str()))
        .cloned()
        .collect()
}

/// v4 `digestsWithLivingMember(digests, doomed)` (`:160-177`): the one rule
/// for "this digest is still alive" — some member listed in its
/// `consolidatedFrom`, outside the doomed set, exists and still names it
/// (`supersededById`). Returns the ids of the living digests.
fn digests_with_living_member(
    conn: &Connection,
    digests: &[DigestRef],
    doomed: &HashSet<String>,
) -> Result<HashSet<String>, DbError> {
    let mut alive = HashSet::new();
    if digests.is_empty() {
        return Ok(alive);
    }
    let member_ids: Vec<String> = dedupe(digests.iter().flat_map(|d| d.consolidated_from.iter()))
        .into_iter()
        .filter(|id| !doomed.contains(id))
        .collect();
    if member_ids.is_empty() {
        return Ok(alive);
    }
    let digest_ids: HashSet<&str> = digests.iter().map(|d| d.id.as_str()).collect();
    for member in memories_read::find_by_ids(conn, &member_ids)? {
        let id = member.get("id").and_then(Value::as_str).unwrap_or_default();
        if let Some(sup) = member.get("supersededById").and_then(Value::as_str) {
            if !sup.is_empty() && digest_ids.contains(sup) && !doomed.contains(id) {
                alive.insert(sup.to_string());
            }
        }
    }
    Ok(alive)
}

/// v4 `applyKeepManual(rows, report)` (`:185-221`): split the rows into those
/// to retire and those to spare. A digest is spared only while it has a
/// living member.
fn apply_keep_manual(
    conn: &Connection,
    rows: Vec<RetireRow>,
    report: &mut RetireReport,
) -> Result<Vec<RetireRow>, DbError> {
    let total = rows.len();
    let mut doomed: Vec<RetireRow> = Vec::new();
    let mut spared_digests: Vec<RetireRow> = Vec::new();
    for row in rows {
        let tier = if row.tier.as_deref() == Some("cold") {
            "cold"
        } else {
            "hot"
        };
        match policy_protection_of(row.source.as_deref(), Some(tier)) {
            None => doomed.push(row),
            Some(PolicyProtection::Digest) => spared_digests.push(row),
            Some(protection) => report.spared_by.count(protection),
        }
    }
    if !spared_digests.is_empty() {
        let refs: Vec<DigestRef> = spared_digests
            .iter()
            .map(|d| DigestRef {
                id: d.id.clone(),
                character_id: d.character_id.clone(),
                consolidated_from: parse_json_id_list(d.consolidated_from.as_deref()),
            })
            .collect();
        let doomed_ids: HashSet<String> = doomed.iter().map(|r| r.id.clone()).collect();
        let alive = digests_with_living_member(conn, &refs, &doomed_ids)?;
        for digest in spared_digests {
            if alive.contains(&digest.id) {
                report.spared_by.count(PolicyProtection::Digest);
            } else {
                doomed.push(digest);
            }
        }
    }
    report.spared = total - doomed.len();
    Ok(doomed)
}

/// v4 `cascadeEmptiedDigests(doomed)` (`:233-262`): digests the retirement
/// would leave with no living member, repeated (bounded) in case a retired
/// digest was itself a member of another. Also hands back the digests that
/// contain a doomed member as read against the FINAL doomed set (`None` when
/// the last pass grew the set after reading).
fn cascade_emptied_digests(
    conn: &Connection,
    doomed: &mut Doomed,
) -> Result<(usize, Option<Vec<DigestRef>>), DbError> {
    let mut added = 0usize;
    let mut containing: Option<Vec<DigestRef>> = None;
    for _ in 0..MAX_DIGEST_CASCADE_PASSES {
        let holders = dedupe(doomed.rows.iter().map(|r| &r.character_id));
        let digests = find_digests_containing_members(conn, &holders, &doomed.keys())?;
        let candidates: Vec<DigestRef> = digests
            .iter()
            .filter(|d| !doomed.ids.contains(&d.id))
            .cloned()
            .collect();
        containing = Some(digests);
        if candidates.is_empty() {
            break;
        }
        let alive = digests_with_living_member(conn, &candidates, &doomed.ids)?;
        let emptied: Vec<String> = candidates
            .iter()
            .filter(|d| !alive.contains(&d.id))
            .map(|d| d.id.clone())
            .collect();
        if emptied.is_empty() {
            break;
        }
        for row in find_retire_rows(conn, &emptied)? {
            tracing::debug!(
                module = MODULE,
                digestId = %row.id,
                characterId = %row.character_id,
                members = parse_json_id_list(row.consolidated_from.as_deref()).len(),
                "Digest retires with its last member"
            );
            doomed.insert(row);
        }
        added += emptied.len();
        containing = None; // the doomed set grew after this read
    }
    Ok((added, containing))
}

/// v4 `scrubDigestLinks(doomed, holderIds, prefetched)` (`:271-311`): drop the
/// doomed ids from surviving digests' `consolidatedFrom` (a removal delta),
/// and cut `supersededById` on surviving members of a doomed digest through
/// [`memory_index::set_tier`] (still cold, no digest). Scoped to the doomed
/// rows' holders. Returns rows rewritten.
fn scrub_digest_links(
    conn: &Connection,
    doomed: &HashSet<String>,
    doomed_ids: &[String],
    holders: &[String],
    prefetched: Option<Vec<DigestRef>>,
) -> Result<usize, DbError> {
    let holders = dedupe(holders);
    if doomed.is_empty() || holders.is_empty() {
        return Ok(0);
    }
    let repo = MemoriesRepository::new(conn);
    let mut touched = 0usize;
    let digests = match prefetched {
        Some(d) => d,
        None => find_digests_containing_members(conn, &holders, doomed_ids)?,
    };
    for digest in &digests {
        if doomed.contains(&digest.id) {
            continue;
        }
        let removed: Vec<String> = digest
            .consolidated_from
            .iter()
            .filter(|id| doomed.contains(*id))
            .cloned()
            .collect();
        if removed.is_empty() {
            continue;
        }
        repo.remove_consolidated_members(&digest.character_id, &digest.id, &removed)?;
        touched += 1;
    }

    let mut orphaned_by_character: Vec<(String, Vec<String>)> = Vec::new();
    for (id, character_id) in find_ids_superseded_by_any(conn, &holders, doomed_ids)? {
        if doomed.contains(&id) {
            continue;
        }
        match orphaned_by_character
            .iter_mut()
            .find(|(c, _)| *c == character_id)
        {
            Some((_, list)) => list.push(id),
            None => orphaned_by_character.push((character_id, vec![id])),
        }
    }
    for (character_id, ids) in &orphaned_by_character {
        memory_index::set_tier(
            conn,
            character_id,
            ids,
            Tier::Cold,
            TierExtra {
                superseded_by_id: Some(None),
                consolidated_at: None,
            },
        )?;
        touched += ids.len();
    }

    tracing::debug!(
        module = MODULE,
        doomed = doomed.len(),
        holders = holders.len(),
        digestsScanned = digests.len(),
        touched = touched,
        "Scrubbed digest links to retired memories"
    );
    Ok(touched)
}

/// v4 `digestBucketsOf(rows)` (`:314-323`): holder id → the subject buckets
/// of the digests a retirement removed (a no-subject digest has no mirror).
fn digest_buckets_of<'a>(
    rows: impl IntoIterator<Item = &'a RetireRow>,
) -> Vec<(String, Vec<String>)> {
    let mut buckets: Vec<(String, Vec<String>)> = Vec::new();
    for row in rows {
        if row.source.as_deref() != Some("CONSOLIDATED") {
            continue;
        }
        let Some(about) = row.about_character_id.as_deref().filter(|s| !s.is_empty()) else {
            continue;
        };
        match buckets.iter_mut().find(|(h, _)| *h == row.character_id) {
            Some((_, set)) => {
                if !set.iter().any(|s| s == about) {
                    set.push(about.to_string());
                }
            }
            None => buckets.push((row.character_id.clone(), vec![about.to_string()])),
        }
    }
    buckets
}

/// v4 `announceRetirement(characterIds, digestRows)` (`:330-354`) — the
/// parent-side effects of a retirement, run AFTER the writer commit (R-AF):
/// every holder's frozen archives invalidated, `memories` published, then the
/// Commonplace mirror of each removed digest's bucket refreshed with DEBUG
/// `Refreshed Commonplace mirror after retiring a digest {module,
/// characterId, subjects, written}`. `None` announces nothing (v4 calls it
/// only when `retired > 0`).
pub async fn announce_retirement(db: &Db, announcement: Option<&RetirementAnnouncement>) {
    let Some(announcement) = announcement else {
        return;
    };
    for character_id in &announcement.holders {
        invalidate_frozen_archive(character_id);
    }
    publish_realtime(RealtimeTopic::Memories, None);
    for (holder_id, subjects) in &announcement.digest_buckets {
        let written = mirror_commonplace_for_subjects(db, holder_id, subjects, None).await;
        tracing::debug!(
            module = MODULE,
            characterId = %holder_id,
            subjects = subjects.len(),
            written = written,
            "Refreshed Commonplace mirror after retiring a digest"
        );
    }
}

/// v4 `planRetirement(requestedIds, opts, report)` (`:363-381`): which rows a
/// retirement takes — the requested rows less what `keep_manual` spares, plus
/// every digest left with no living member (except under `Retention`). Reads
/// only; fills the report's spared and cascade counts.
fn plan_retirement(
    conn: &Connection,
    requested_ids: &[String],
    opts: RetireOptions,
    report: &mut RetireReport,
) -> Result<(Doomed, Option<Vec<DigestRef>>), DbError> {
    let mut rows = find_retire_rows(conn, requested_ids)?;
    if opts.keep_manual {
        rows = apply_keep_manual(conn, rows, report)?;
    }
    let mut doomed = Doomed::default();
    for row in rows {
        doomed.insert(row);
    }
    let mut prefetched = None;
    if opts.reason != RetireReason::Retention && doomed.len() > 0 {
        let (added, containing) = cascade_emptied_digests(conn, &mut doomed)?;
        report.digests_retired_with_last_member = added;
        prefetched = containing;
    }
    Ok((doomed, prefetched))
}

/// v4 `retireMemories(ids, opts)` (`:383-457`) on the WRITER's connection,
/// inside the caller's `Db::write` closure: scrub every link that names the
/// rows, delete them, evict their vectors. The only deletion of a memory row.
/// Hand [`RetireReport::announcement`] to [`announce_retirement`] after the
/// commit (R-AF).
///
/// Lines: `Nothing to retire {module, reason, requested, spared}` (DEBUG)
/// when nothing is doomed; else the WARN `Retirement rewrote an unusually
/// large neighbour set` when `neighboursTouched >= 200`, then DEBUG `Retired
/// memories {module, reason, keep, requested, retired, spared, sparedBy,
/// digestsRetiredWithLastMember, vectorsRemoved, neighboursTouched,
/// digestLinksScrubbed, charactersAffected, durationMs}` (`keep` `manual` or
/// `null`; `sparedBy` rides as `sparedByJson`, the object's JSON text).
pub fn retire_memories(
    conn: &Connection,
    ids: &[String],
    opts: RetireOptions,
) -> Result<RetireReport, DbError> {
    let requested_ids = dedupe(ids);
    let mut report = RetireReport::empty(requested_ids.len());
    if requested_ids.is_empty() {
        return Ok(report);
    }
    let started_at = crate::clock::now_unix_ms();
    let repo = MemoriesRepository::new(conn);

    let (doomed, prefetched) = plan_retirement(conn, &requested_ids, opts, &mut report)?;

    // The neighbour scrub is one SQL removal delta across every holder (bug
    // 234). With nothing kept back it takes every requested id, found or
    // already gone, so a stale id leaves its neighbours too; a spared row's id
    // must stay linked.
    let doomed_keys = doomed.keys();
    let scrub_ids: Vec<String> = if opts.keep_manual {
        doomed_keys.clone()
    } else {
        dedupe(requested_ids.iter().chain(doomed_keys.iter()))
    };
    if !scrub_ids.is_empty() {
        report.neighbours_touched = repo.remove_related_memory_ids_everywhere(&scrub_ids)?;
    }

    if doomed.len() == 0 {
        tracing::debug!(
            module = MODULE,
            reason = opts.reason.as_str(),
            requested = report.requested,
            spared = report.spared,
            "Nothing to retire"
        );
        return Ok(report);
    }

    let mut ids_by_character: Vec<(String, Vec<String>)> = Vec::new();
    for row in &doomed.rows {
        match ids_by_character
            .iter_mut()
            .find(|(c, _)| *c == row.character_id)
        {
            Some((_, list)) => list.push(row.id.clone()),
            None => ids_by_character.push((row.character_id.clone(), vec![row.id.clone()])),
        }
    }
    report.characters_affected = ids_by_character.len();
    let holders: Vec<String> = ids_by_character.iter().map(|(c, _)| c.clone()).collect();
    report.digest_links_scrubbed =
        scrub_digest_links(conn, &doomed.ids, &doomed_keys, &holders, prefetched)?;

    for (character_id, character_ids) in &ids_by_character {
        // L-2: the loaded index's share, read before the rows go.
        let indexed = memory_index::indexed_among(conn, character_id, character_ids)?;
        report.retired += repo.bulk_delete_unsafe(character_id, character_ids)?;
        report.vectors_removed += memory_index::remove(character_id, character_ids.len(), indexed);
    }
    report.retired_ids = doomed_keys;

    let duration_ms = crate::clock::now_unix_ms() - started_at;
    let keep = if opts.keep_manual { "manual" } else { "null" };
    let spared_by_json = report.spared_by.to_json().to_string();
    if report.neighbours_touched >= NEIGHBOUR_WARN {
        tracing::warn!(
            module = MODULE,
            reason = opts.reason.as_str(),
            keep = keep,
            requested = report.requested,
            retired = report.retired,
            spared = report.spared,
            sparedByJson = %spared_by_json,
            digestsRetiredWithLastMember = report.digests_retired_with_last_member,
            vectorsRemoved = report.vectors_removed,
            neighboursTouched = report.neighbours_touched,
            digestLinksScrubbed = report.digest_links_scrubbed,
            charactersAffected = report.characters_affected,
            durationMs = duration_ms,
            "Retirement rewrote an unusually large neighbour set"
        );
    }
    tracing::debug!(
        module = MODULE,
        reason = opts.reason.as_str(),
        keep = keep,
        requested = report.requested,
        retired = report.retired,
        spared = report.spared,
        sparedByJson = %spared_by_json,
        digestsRetiredWithLastMember = report.digests_retired_with_last_member,
        vectorsRemoved = report.vectors_removed,
        neighboursTouched = report.neighbours_touched,
        digestLinksScrubbed = report.digest_links_scrubbed,
        charactersAffected = report.characters_affected,
        durationMs = duration_ms,
        "Retired memories"
    );

    if report.retired > 0 {
        report.announcement = Some(RetirementAnnouncement {
            holders,
            digest_buckets: digest_buckets_of(&doomed.rows),
        });
    }
    Ok(report)
}

/// [`retire_memories`] in its own `Db::write`, then [`announce_retirement`]
/// once it has committed (R-AF) — the shape every caller outside a larger
/// write closure takes. Empty `ids` writes nothing.
pub async fn retire_and_announce(
    db: &Db,
    ids: Vec<String>,
    opts: RetireOptions,
) -> Result<RetireReport, DbError> {
    if ids.is_empty() {
        return Ok(RetireReport::default());
    }
    let report = db
        .write(move |w| retire_memories(w.main().connection(), &ids, opts))
        .await?;
    announce_retirement(db, report.announcement.as_ref()).await;
    Ok(report)
}

/// v4 `DanglingScrubReport` (`:460-468`) — rows rewritten per link column,
/// plus what the caller announces after the commit (R-AF): the inner
/// retirement's own announcement first, then the scrub's holders (v4's two
/// `announceRetirement` calls, in order).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DanglingScrubReport {
    pub related_rows_touched: usize,
    pub digests_touched: usize,
    /// Digests every one of whose members was already gone, retired rather
    /// than left empty (invariant 8).
    pub digests_retired: i64,
    pub members_cut: usize,
    pub characters_affected: usize,
    pub announcements: Vec<RetirementAnnouncement>,
}

impl DanglingScrubReport {
    /// v4's object, in its key order.
    pub fn to_json(&self) -> Value {
        json!({
            "relatedRowsTouched": self.related_rows_touched,
            "digestsTouched": self.digests_touched,
            "digestsRetired": self.digests_retired,
            "membersCut": self.members_cut,
            "charactersAffected": self.characters_affected,
        })
    }
}

/// v4 `scrubDanglingMemoryLinks(links)` (`:480-541`) on the WRITER's
/// connection — the graph repair's half (the scan is
/// [`memories_read::find_dangling_links`]; the repair job itself is
/// P4.D297's, R-K). Only the rows `links` names are written:
///
/// - each row loses its dangling `relatedMemoryIds` entries (a removal delta);
/// - a digest loses its missing members — unless every member is missing, in
///   which case it is retired through [`retire_memories`]
///   ([`RetireReason::GraphRepair`]);
/// - a member whose digest is gone keeps its tier and loses its
///   `supersededById` (through [`memory_index::set_tier`]).
///
/// DEBUG `Scrubbed dangling memory links {module, relatedRowsTouched,
/// digestsTouched, digestsRetired, membersCut, charactersAffected}`.
pub fn scrub_dangling_memory_links(
    conn: &Connection,
    links: &DanglingMemoryLinks,
) -> Result<DanglingScrubReport, DbError> {
    let repo = MemoriesRepository::new(conn);
    let mut report = DanglingScrubReport::default();
    let mut characters: Vec<String> = Vec::new();

    // relatedByRow — insertion-ordered, targets deduped first-seen.
    let mut related_by_row: Vec<(String, String, Vec<String>)> = Vec::new();
    for l in &links.related {
        match related_by_row.iter_mut().find(|(id, _, _)| *id == l.id) {
            Some((_, _, targets)) => {
                if !targets.contains(&l.target) {
                    targets.push(l.target.clone());
                }
            }
            None => {
                related_by_row.push((l.id.clone(), l.character_id.clone(), vec![l.target.clone()]))
            }
        }
    }
    for (memory_id, character_id, targets) in &related_by_row {
        repo.remove_related_memory_ids(character_id, memory_id, targets)?;
        report.related_rows_touched += 1;
        touch(&mut characters, character_id);
    }

    // membersByDigest — `dangling` counts every link row (duplicates
    // included), `targets` is a set.
    struct DigestEntry {
        id: String,
        character_id: String,
        targets: Vec<String>,
        dangling: i64,
        member_count: i64,
    }
    let mut members_by_digest: Vec<DigestEntry> = Vec::new();
    for (l, member_count) in &links.consolidated_from {
        match members_by_digest.iter_mut().find(|d| d.id == l.id) {
            Some(entry) => {
                if !entry.targets.contains(&l.target) {
                    entry.targets.push(l.target.clone());
                }
                entry.dangling += 1;
            }
            None => members_by_digest.push(DigestEntry {
                id: l.id.clone(),
                character_id: l.character_id.clone(),
                targets: vec![l.target.clone()],
                dangling: 1,
                member_count: *member_count,
            }),
        }
    }
    let emptied: Vec<String> = members_by_digest
        .iter()
        .filter(|d| d.dangling >= d.member_count)
        .map(|d| d.id.clone())
        .collect();
    if !emptied.is_empty() {
        let retired = retire_memories(
            conn,
            &emptied,
            RetireOptions::new(RetireReason::GraphRepair),
        )?;
        report.digests_retired = retired.retired;
        if let Some(a) = retired.announcement {
            report.announcements.push(a);
        }
    }
    for entry in &members_by_digest {
        if entry.dangling >= entry.member_count {
            continue;
        }
        repo.remove_consolidated_members(&entry.character_id, &entry.id, &entry.targets)?;
        report.digests_touched += 1;
        touch(&mut characters, &entry.character_id);
    }

    // membersByHolderTier, keyed `${characterId}:${tier}`.
    let mut members_by_holder_tier: Vec<(String, Tier, Vec<String>)> = Vec::new();
    for (l, tier) in &links.superseded_by {
        match members_by_holder_tier
            .iter_mut()
            .find(|(c, t, _)| *c == l.character_id && t == tier)
        {
            Some((_, _, ids)) => ids.push(l.id.clone()),
            None => {
                members_by_holder_tier.push((l.character_id.clone(), *tier, vec![l.id.clone()]))
            }
        }
    }
    for (character_id, tier, ids) in &members_by_holder_tier {
        memory_index::set_tier(
            conn,
            character_id,
            ids,
            *tier,
            TierExtra {
                superseded_by_id: Some(None),
                consolidated_at: None,
            },
        )?;
        report.members_cut += ids.len();
        touch(&mut characters, character_id);
    }

    report.characters_affected = characters.len();
    tracing::debug!(
        module = MODULE,
        relatedRowsTouched = report.related_rows_touched,
        digestsTouched = report.digests_touched,
        digestsRetired = report.digests_retired,
        membersCut = report.members_cut,
        charactersAffected = report.characters_affected,
        "Scrubbed dangling memory links"
    );
    // The retirement above announced its own holders.
    if !characters.is_empty() {
        report.announcements.push(RetirementAnnouncement {
            holders: characters,
            digest_buckets: Vec::new(),
        });
    }
    Ok(report)
}

/// v4 `retireChatMemories(chatId)` (`:543-556`) on the WRITER's connection:
/// every memory the chat sourced, retired under `keep_manual` — a regenerate
/// never destroys a hand-written row, a cold row, or a digest that still has a
/// member (bug 201, decision 4). INFO `Chat wipe spared manual, cold and digest
/// rows {module, chatId, spared, sparedBy}` when anything was spared, then
/// DEBUG `Wiped a chat's extracted memories {module, chatId, retired,
/// spared}`. Announce the report after the commit (R-AF).
pub fn retire_chat_memories(conn: &Connection, chat_id: &str) -> Result<RetireReport, DbError> {
    let ids = chat_memory_ids(conn, chat_id)?;
    let report = retire_memories(
        conn,
        &ids,
        RetireOptions {
            reason: RetireReason::ChatWipe,
            keep_manual: true,
        },
    )?;
    if report.spared > 0 {
        tracing::info!(
            module = MODULE,
            chatId = %chat_id,
            spared = report.spared,
            sparedByJson = %report.spared_by.to_json(),
            "Chat wipe spared manual, cold and digest rows"
        );
    }
    tracing::debug!(
        module = MODULE,
        chatId = %chat_id,
        retired = report.retired,
        spared = report.spared,
        "Wiped a chat's extracted memories"
    );
    Ok(report)
}

/// v4 `previewChatWipe(chatId)` (`:562-569`): the ids a chat wipe would retire,
/// without retiring anything — the extraction dry run searches past them.
/// Reads only. DEBUG `Previewed a chat wipe {module, chatId, rows,
/// wouldRetire}` (none when the chat sourced no memory).
pub fn preview_chat_wipe(conn: &Connection, chat_id: &str) -> Result<HashSet<String>, DbError> {
    let ids = dedupe(&chat_memory_ids(conn, chat_id)?);
    if ids.is_empty() {
        return Ok(HashSet::new());
    }
    let mut scratch = RetireReport::empty(ids.len());
    let (doomed, _) = plan_retirement(
        conn,
        &ids,
        RetireOptions {
            reason: RetireReason::ChatWipe,
            keep_manual: true,
        },
        &mut scratch,
    )?;
    tracing::debug!(
        module = MODULE,
        chatId = %chat_id,
        rows = ids.len(),
        wouldRetire = doomed.len(),
        "Previewed a chat wipe"
    );
    Ok(doomed.ids)
}

/// v4's `characters.add(c)` on an insertion-ordered `Set`.
fn touch(characters: &mut Vec<String>, c: &str) {
    if !characters.iter().any(|x| x == c) {
        characters.push(c.to_string());
    }
}

/// The chat's memory ids, as v4's `memories.findByChatId(chatId)` lists them.
fn chat_memory_ids(conn: &Connection, chat_id: &str) -> Result<Vec<String>, DbError> {
    Ok(memories_read::find_by_chat_id(conn, chat_id)?
        .iter()
        .filter_map(|m| m.get("id").and_then(Value::as_str).map(str::to_string))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::memories::{CreateOptions, MemCreate};
    use crate::db::Writer;
    use tempfile::{tempdir, TempDir};

    /// A throwaway base64 pepper keys the fresh encrypted DB (never a real one).
    const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
    const SENTINEL: &str = "2020-01-01T00:00:00.000Z";
    const DDL: &str = "
        CREATE TABLE memories (
            id TEXT PRIMARY KEY, characterId TEXT, aboutCharacterId TEXT, chatId TEXT,
            projectId TEXT, content TEXT, summary TEXT, keywords TEXT, tags TEXT,
            importance REAL, embedding BLOB, source TEXT, witnessedContext TEXT,
            occurredAt TEXT, narrativeTime TEXT, entities TEXT DEFAULT '[]', kind TEXT DEFAULT 'semantic',
            sourceMessageId TEXT, lastAccessedAt TEXT, reinforcementCount REAL,
            lastReinforcedAt TEXT, relatedMemoryIds TEXT, reinforcedImportance REAL,
            tier TEXT DEFAULT 'hot', supersededById TEXT, supersededAt TEXT, consolidatedFrom TEXT DEFAULT '[]', consolidatedAt TEXT, createdAt TEXT, updatedAt TEXT);
    ";

    #[derive(Default)]
    struct Seed {
        id: &'static str,
        character_id: &'static str,
        chat_id: Option<&'static str>,
        source: &'static str,
        tier: &'static str,
        about: Option<&'static str>,
        related: &'static [&'static str],
        superseded_by: Option<&'static str>,
        consolidated_from: &'static [&'static str],
        vector: Option<Vec<f32>>,
    }

    fn seed(id: &'static str, character_id: &'static str) -> Seed {
        Seed {
            id,
            character_id,
            source: "AUTO",
            tier: "hot",
            ..Default::default()
        }
    }

    fn writer(seeds: &[Seed]) -> (TempDir, Writer) {
        let dir = tempdir().unwrap();
        let w = Writer::open_writable(&dir.path().join("main.db"), PEPPER).unwrap();
        w.connection().execute_batch(DDL).unwrap();
        for s in seeds {
            w.memories()
                .plant_unvalidated(
                    &MemCreate {
                        character_id: s.character_id.to_string(),
                        about_character_id: s.about.map(str::to_string),
                        chat_id: s.chat_id.map(str::to_string),
                        project_id: None,
                        content: format!("content {}", s.id),
                        summary: format!("summary {}", s.id),
                        keywords: vec![],
                        tags: vec![],
                        importance: 0.5,
                        embedding: s.vector.clone(),
                        source: s.source.to_string(),
                        witnessed_context: None,
                        occurred_at: None,
                        narrative_time: None,
                        entities: Vec::new(),
                        kind: "semantic".to_string(),
                        source_message_id: None,
                        last_accessed_at: None,
                        reinforcement_count: 1.0,
                        last_reinforced_at: None,
                        related_memory_ids: s.related.iter().map(|r| r.to_string()).collect(),
                        reinforced_importance: 0.5,
                        tier: s.tier.to_string(),
                        superseded_by_id: s.superseded_by.map(str::to_string),
                        superseded_at: None,
                        consolidated_from: s
                            .consolidated_from
                            .iter()
                            .map(|r| r.to_string())
                            .collect(),
                        consolidated_at: None,
                    },
                    &CreateOptions {
                        id: s.id.to_string(),
                        created_at: SENTINEL.to_string(),
                        updated_at: SENTINEL.to_string(),
                    },
                )
                .unwrap();
        }
        (dir, w)
    }

    fn cell(w: &Writer, id: &str, column: &str) -> Option<Option<String>> {
        w.connection()
            .query_row(
                &format!("SELECT {column} FROM memories WHERE id = ?1"),
                [id],
                |r| r.get::<_, Option<String>>(0),
            )
            .map(Some)
            .unwrap_or(None)
    }

    fn ids(xs: &[&str]) -> Vec<String> {
        xs.iter().map(|s| s.to_string()).collect()
    }

    /// The per-row purge spans holders: rows deleted, an unknown id still
    /// scrubbed from a neighbour, vectors counted only where a row carried
    /// one (L-2), and the announcement names both holders.
    #[test]
    fn a_retirement_deletes_scrubs_and_counts_the_loaded_vectors() {
        let (_d, w) = writer(&[
            Seed {
                vector: Some(vec![1.0, 0.0]),
                ..seed("m1", "char-a")
            },
            seed("m2", "char-b"),
            Seed {
                related: &["m1", "ghost", "m9"],
                ..seed("m3", "char-b")
            },
        ]);
        let r = retire_memories(
            w.connection(),
            &ids(&["m1", "m2", "ghost", "m1"]),
            RetireOptions::new(RetireReason::ItemDelete),
        )
        .unwrap();
        assert_eq!((r.requested, r.retired, r.vectors_removed), (3, 2, 1));
        assert_eq!(r.retired_ids, ids(&["m1", "m2"]));
        assert_eq!((r.neighbours_touched, r.characters_affected), (1, 2));
        assert_eq!(
            cell(&w, "m3", "relatedMemoryIds"),
            Some(Some("[\"m9\"]".into()))
        );
        assert_eq!(cell(&w, "m1", "id"), None);
        assert_eq!(
            r.announcement,
            Some(RetirementAnnouncement {
                holders: ids(&["char-a", "char-b"]),
                digest_buckets: vec![],
            })
        );
        // Idempotent: a second pass finds nothing and announces nothing.
        let again = retire_memories(
            w.connection(),
            &ids(&["m1", "m2"]),
            RetireOptions::new(RetireReason::ItemDelete),
        )
        .unwrap();
        assert_eq!((again.retired, again.announcement), (0, None));
    }

    /// Bug 201: under `keep_manual` a MANUAL row, a cold row and a digest with
    /// a living member are spared (and their ids stay linked); a digest whose
    /// only member is going goes too.
    #[test]
    fn keep_manual_spares_manual_cold_and_living_digests() {
        let (_d, w) = writer(&[
            Seed {
                source: "MANUAL",
                chat_id: Some("c1"),
                ..seed("manual", "h")
            },
            Seed {
                tier: "cold",
                chat_id: Some("c1"),
                ..seed("cold", "h")
            },
            Seed {
                source: "CONSOLIDATED",
                chat_id: Some("c1"),
                consolidated_from: &["member-live"],
                ..seed("digest-live", "h")
            },
            Seed {
                tier: "cold",
                superseded_by: Some("digest-live"),
                ..seed("member-live", "h")
            },
            Seed {
                source: "CONSOLIDATED",
                chat_id: Some("c1"),
                about: Some("subject"),
                consolidated_from: &["auto"],
                ..seed("digest-dead", "h")
            },
            Seed {
                chat_id: Some("c1"),
                ..seed("auto", "h")
            },
            Seed {
                related: &["manual", "auto"],
                ..seed("neighbour", "h")
            },
        ]);
        let r = retire_chat_memories(w.connection(), "c1").unwrap();
        assert_eq!(r.spared, 3);
        assert_eq!(
            r.spared_by,
            SparedBy {
                manual: 1,
                digest: 1,
                cold: 1
            }
        );
        assert_eq!(r.retired, 2);
        assert_eq!(cell(&w, "manual", "id"), Some(Some("manual".into())));
        assert_eq!(cell(&w, "digest-dead", "id"), None);
        assert_eq!(
            cell(&w, "neighbour", "relatedMemoryIds"),
            Some(Some("[\"manual\"]".into())),
            "a spared row's id stays linked"
        );
        assert_eq!(
            r.announcement.unwrap().digest_buckets,
            vec![("h".to_string(), ids(&["subject"]))]
        );
        // The preview decides the same rows without writing.
        let (_d2, w2) = writer(&[
            Seed {
                source: "MANUAL",
                chat_id: Some("c2"),
                ..seed("m", "h")
            },
            Seed {
                chat_id: Some("c2"),
                ..seed("a", "h")
            },
        ]);
        let preview = preview_chat_wipe(w2.connection(), "c2").unwrap();
        assert_eq!(preview, HashSet::from(["a".to_string()]));
        assert_eq!(cell(&w2, "a", "id"), Some(Some("a".into())));
    }

    /// Invariant 8: a digest whose last member goes, goes with it — except
    /// under `Retention`; a surviving digest loses the doomed member; an
    /// orphaned member of a doomed digest goes cold with its link cut.
    #[test]
    fn the_digest_cascade_and_scrubs_follow_invariant_8() {
        let seeds = || {
            vec![
                Seed {
                    source: "CONSOLIDATED",
                    consolidated_from: &["only"],
                    ..seed("d-single", "h")
                },
                Seed {
                    tier: "cold",
                    superseded_by: Some("d-single"),
                    ..seed("only", "h")
                },
                Seed {
                    source: "CONSOLIDATED",
                    consolidated_from: &["x", "y"],
                    ..seed("d-pair", "h")
                },
                Seed {
                    tier: "cold",
                    superseded_by: Some("d-pair"),
                    ..seed("x", "h")
                },
                Seed {
                    tier: "cold",
                    superseded_by: Some("d-pair"),
                    ..seed("y", "h")
                },
                Seed {
                    source: "CONSOLIDATED",
                    consolidated_from: &["z"],
                    ..seed("d-gone", "h")
                },
                Seed {
                    tier: "hot",
                    superseded_by: Some("d-gone"),
                    ..seed("z", "h")
                },
            ]
        };
        let (_d, w) = writer(&seeds());
        let r = retire_memories(
            w.connection(),
            &ids(&["only", "x", "d-gone"]),
            RetireOptions::new(RetireReason::ItemDelete),
        )
        .unwrap();
        assert_eq!(r.digests_retired_with_last_member, 1);
        assert_eq!(cell(&w, "d-single", "id"), None);
        assert_eq!(
            cell(&w, "d-pair", "consolidatedFrom"),
            Some(Some("[\"y\"]".into()))
        );
        assert_eq!(cell(&w, "z", "supersededById"), Some(None));
        assert_eq!(cell(&w, "z", "tier"), Some(Some("cold".into())));
        assert_eq!(r.retired, 4);

        // Retention lets the digest outlive the member it replaced.
        let (_d2, w2) = writer(&seeds());
        let r2 = retire_memories(
            w2.connection(),
            &ids(&["only"]),
            RetireOptions::new(RetireReason::Retention),
        )
        .unwrap();
        assert_eq!((r2.retired, r2.digests_retired_with_last_member), (1, 0));
        assert_eq!(
            cell(&w2, "d-single", "consolidatedFrom"),
            Some(Some("[]".into()))
        );
    }

    /// The graph repair's scrub: dangling `related` entries dropped, a digest
    /// whose every member is gone retired (`graph-repair`), a partly dangling
    /// digest trimmed, an orphaned member's link cut with its tier kept.
    #[test]
    fn the_dangling_scrub_repairs_each_link_column() {
        let (_d, w) = writer(&[
            Seed {
                related: &["gone-1", "kept"],
                ..seed("r", "h")
            },
            seed("kept", "h"),
            Seed {
                source: "CONSOLIDATED",
                consolidated_from: &["gone-2"],
                ..seed("d-empty", "h")
            },
            Seed {
                source: "CONSOLIDATED",
                consolidated_from: &["gone-3", "kept"],
                ..seed("d-part", "h")
            },
            Seed {
                superseded_by: Some("gone-4"),
                ..seed("orphan", "h")
            },
        ]);
        let links = memories_read::find_dangling_links(w.connection(), None).unwrap();
        assert_eq!(
            (
                links.related.len(),
                links.consolidated_from.len(),
                links.superseded_by.len()
            ),
            (1, 2, 1)
        );
        let r = scrub_dangling_memory_links(w.connection(), &links).unwrap();
        assert_eq!(
            (
                r.related_rows_touched,
                r.digests_touched,
                r.digests_retired,
                r.members_cut
            ),
            (1, 1, 1, 1)
        );
        assert_eq!(
            cell(&w, "r", "relatedMemoryIds"),
            Some(Some("[\"kept\"]".into()))
        );
        assert_eq!(cell(&w, "d-empty", "id"), None);
        assert_eq!(
            cell(&w, "d-part", "consolidatedFrom"),
            Some(Some("[\"kept\"]".into()))
        );
        assert_eq!(cell(&w, "orphan", "supersededById"), Some(None));
        assert_eq!(cell(&w, "orphan", "tier"), Some(Some("hot".into())));
        assert_eq!(
            r.announcements.len(),
            2,
            "the retirement's, then the scrub's"
        );
        let clean = memories_read::find_dangling_links(w.connection(), Some(&ids(&["h"]))).unwrap();
        assert_eq!(clean, DanglingMemoryLinks::default());
        assert_eq!(
            memories_read::find_dangling_links(w.connection(), Some(&[])).unwrap(),
            DanglingMemoryLinks::default()
        );
    }

    /// v4's eleven kebab reasons.
    #[test]
    fn the_reasons_render_v4s_literals() {
        let all = [
            (RetireReason::ItemDelete, "item-delete"),
            (RetireReason::ChatWipe, "chat-wipe"),
            (RetireReason::MessageCascade, "message-cascade"),
            (RetireReason::MessageReattribution, "message-reattribution"),
            (RetireReason::SwipeRegenerate, "swipe-regenerate"),
            (RetireReason::CharacterDelete, "character-delete"),
            (RetireReason::CharacterArchive, "character-archive"),
            (RetireReason::RestoreReplace, "restore-replace"),
            (RetireReason::Retention, "retention"),
            (RetireReason::NearDuplicateFold, "near-duplicate-fold"),
            (RetireReason::GraphRepair, "graph-repair"),
        ];
        for (reason, literal) in all {
            assert_eq!(reason.as_str(), literal);
        }
    }
}
