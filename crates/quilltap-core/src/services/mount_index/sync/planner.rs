//! The sync planner. PURE: two walks, a base, and the options in; a list of
//! actions out. No SQLite, no filesystem, no clock.
//!
//! Port of v4 `lib/mount-index/sync/planner.ts` (`23da0b322`).
//!
//! Everything the sync decides is decided here, which is what makes the
//! decision table testable row by row rather than by running a sync and
//! inspecting a directory afterwards.
//!
//! The rules, in one place:
//!
//!   - **SHA-256 first, timestamps second.** Equal bytes and unequal clocks is
//!     a `touch`, never a copy. This is with the grain of the mount index,
//!     whose scanner already decides "unchanged" purely by sha.
//!   - **A difference is resolved by the newer side**, unless the base shows
//!     both sides moved since the last agreement — that is a `conflict`,
//!     reported and skipped. `--prefer store|disk` overrides both.
//!   - **A deletion propagates only when the base proves it.** With no base — a
//!     first run — an entry missing on one side is created there. The sync never
//!     deletes something it has no record of.
//!   - **`createdAt` takes the older of the two.** A file's creation date must
//!     not move later merely because a copy of it was made.

use std::collections::HashSet;

use regex::Regex;
use std::sync::OnceLock;

use super::sidecar::{description_sha256, descriptions_equal};
use super::types::{
    has_sidecar, EntryKind, ManifestEntry, OrderedMap, SyncAction, SyncActionKind, SyncDirection,
    SyncEntry, SyncEntryMap, SyncOptions, SyncOutcome, SyncPreference, SyncSide,
    MTIME_TOLERANCE_MS,
};
use crate::collation::locale_compare;
use crate::episodic::js_date_parse_ms;

/// A character vault's keystones. `writeCharacterVaultManagedFields` writes
/// these unconditionally, and the overlay treats a vault without them as broken,
/// so a sync must never propagate their deletion from disk — an operator who
/// tidied a directory would otherwise hollow the character. Mirrors
/// `REQUIRED_VAULT_FILES` in `character-vault.ts`, plus the dressing
/// instructions the wardrobe reads.
pub const CHARACTER_VAULT_KEYSTONES: &[&str] = &[
    "properties.json",
    "identity.md",
    "description.md",
    "manifesto.md",
    "personality.md",
    "example-dialogues.md",
    "wardrobe/instructions.md",
];

/// v4 `PlanInput`.
pub struct PlanInput<'a> {
    pub store: &'a SyncEntryMap,
    pub disk: &'a SyncEntryMap,
    pub base: &'a OrderedMap<ManifestEntry>,
    pub options: &'a SyncOptions,
    /// True for a `storeType = 'character'` store: keystone deletions are
    /// refused.
    pub is_character_vault: bool,
    /// Whether this platform can be made to report the creation date the sync
    /// asks for (macOS, via the `utimes` two-step). Where it cannot, a disk
    /// `createdAt` that disagrees must NOT drive a `touch`: the touch could not
    /// change the answer, the next walk would read the same disagreement, and
    /// the sync would plan the same futile action every run forever. The
    /// manifest carries the value instead, so the comparison stays right even
    /// where the filesystem's own answer cannot be.
    pub can_set_disk_birthtime: bool,
}

pub struct PlanOutput {
    pub actions: Vec<SyncAction>,
    pub warnings: Vec<String>,
}

// ============================================================================
// Comparison helpers
// ============================================================================

/// v4 `timeOf`: `new Date(iso).getTime()`, with `NaN` reported as absent. An
/// empty string is falsy in v4's `if (!iso)` guard and never reaches `Date`.
fn time_of(iso: Option<&str>) -> Option<i64> {
    let iso = iso?;
    if iso.is_empty() {
        return None;
    }
    js_date_parse_ms(iso)
}

/// v4 `sameInstant`: within [`MTIME_TOLERANCE_MS`] counts as the same instant.
/// Two unparseable sides are "the same" (`null === null`), a parseable and an
/// unparseable one are not.
fn same_instant(a: Option<&str>, b: Option<&str>) -> bool {
    match (time_of(a), time_of(b)) {
        (Some(x), Some(y)) => (x - y).abs() <= MTIME_TOLERANCE_MS,
        (x, y) => x == y,
    }
}

/// v4 `olderCreatedAt`: the older of two creation dates; a side that cannot say
/// never wins.
fn older_created_at(a: Option<&str>, b: Option<&str>) -> Option<String> {
    let x = time_of(a);
    let y = time_of(b);
    match (x, y) {
        (None, _) => b.map(str::to_string),
        (_, None) => a.map(str::to_string),
        (Some(x), Some(y)) => {
            if x <= y {
                a.map(str::to_string)
            } else {
                b.map(str::to_string)
            }
        }
    }
}

/// v4 `humanGap`. `Math.round` is half-up toward +∞, which for a non-negative
/// magnitude is `(v + 0.5).floor()`.
fn human_gap(a_iso: &str, b_iso: &str) -> String {
    let a = time_of(Some(a_iso)).unwrap_or(0);
    let b = time_of(Some(b_iso)).unwrap_or(0);
    let ms = (a - b).abs();
    let mins = js_round(ms as f64 / 60_000.0);
    if mins < 1 {
        return "moments".to_string();
    }
    if mins < 60 {
        return format!("{mins}m");
    }
    let hours = mins.div_euclid(60);
    let rem = mins.rem_euclid(60);
    if hours < 24 {
        return if rem != 0 {
            format!("{hours}h {rem}m")
        } else {
            format!("{hours}h")
        };
    }
    let days = hours.div_euclid(24);
    format!("{days}d {}h", hours.rem_euclid(24))
}

/// `Math.round` — half away from zero toward +∞, not Rust's half-away-from-zero.
fn js_round(v: f64) -> i64 {
    (v + 0.5).floor() as i64
}

// ============================================================================
// Plan
// ============================================================================

pub fn plan_sync(input: &PlanInput<'_>) -> PlanOutput {
    let PlanInput {
        store,
        disk,
        base,
        options,
        is_character_vault,
        can_set_disk_birthtime,
    } = *input;

    let mut warnings: Vec<String> = Vec::new();
    let mut actions: Vec<SyncAction> = Vec::new();

    // v4: `new Set([...store.keys(), ...disk.keys(), ...base.keys()])` — a JS
    // Set iterates in first-insertion order, so store keys come first, then the
    // disk keys the store did not have, then the base-only ones.
    let mut keys: Vec<&str> = Vec::new();
    let mut seen: HashSet<&str> = HashSet::new();
    for key in store.keys().chain(disk.keys()).chain(base.keys()) {
        if seen.insert(key) {
            keys.push(key);
        }
    }

    // Folders first, then files — a `create disk` needs its directory to exist,
    // and the folder ordering below puts parents before children. Deletions are
    // re-sorted at the end, children before parents.
    let mut folder_actions: Vec<SyncAction> = Vec::new();
    let mut file_actions: Vec<SyncAction> = Vec::new();

    // Paths whose store-side content this plan rewrites; their group siblings
    // are stale on disk.
    let mut store_rewrites: HashSet<String> = HashSet::new();

    for key in keys {
        let s = store.get(key);
        let d = disk.get(key);
        let b = base.get(key);

        // The kinds disagree — one side has a directory where the other has a
        // file. Nothing sensible to do but say so.
        if let (Some(s), Some(d)) = (s, d) {
            if s.kind() != d.kind() {
                file_actions.push(conflict_action(
                    &s.relative_path,
                    s.kind(),
                    &format!(
                        "a {} in the store, a {} on disk",
                        s.kind().as_str(),
                        d.kind().as_str()
                    ),
                ));
                continue;
            }
        }

        let kind = match s.or(d) {
            Some(entry) => entry.kind(),
            None => b.map(|b| b.kind).unwrap_or(EntryKind::File),
        };
        let folder_sink = kind == EntryKind::Folder;

        let (s, d) = match (s, d) {
            // Gone from both sides; the manifest simply forgets it.
            (None, None) => continue,

            // ---- present on one side only ---------------------------------
            (Some(s), None) => {
                let sink = if folder_sink {
                    &mut folder_actions
                } else {
                    &mut file_actions
                };
                let store_changed = match b {
                    None => true,
                    Some(b) => b.sha256.as_deref() != s.sha256.as_deref(),
                };
                if b.is_none() {
                    push(
                        sink,
                        materialise("disk", s, older_created_at(s.created_at.as_deref(), None)),
                        options,
                    );
                    // A newly-materialised binary takes its caption with it; the
                    // both-sides path below never sees this entry.
                    if has_sidecar(s.file_type.as_deref())
                        && !s.description.as_deref().unwrap_or("").is_empty()
                    {
                        let mut action = SyncAction::bare(
                            SyncActionKind::Describe,
                            &s.relative_path,
                            EntryKind::File,
                        );
                        action.side = Some(SyncSide::Disk);
                        action.description.clone_from(&s.description);
                        action.last_modified = Some(
                            s.description_updated_at
                                .clone()
                                .unwrap_or_else(|| s.last_modified.clone()),
                        );
                        action.outcome = Some(SyncOutcome::Planned);
                        push(sink, action, options);
                    }
                } else if store_changed && kind == EntryKind::File {
                    // Edited in the store, deleted on disk. Either answer
                    // discards work.
                    sink.push(conflict_action(
                        &s.relative_path,
                        kind,
                        "edited in the store and deleted on disk",
                    ));
                } else if !options.propagate_deletes {
                    sink.push(skip_action(
                        &s.relative_path,
                        kind,
                        "deleted on disk (--no-delete)",
                    ));
                } else if is_character_vault && is_keystone(&s.relative_path) {
                    sink.push(conflict_action(
                        &s.relative_path,
                        kind,
                        "a character vault keystone cannot be deleted by a sync",
                    ));
                } else {
                    push(
                        sink,
                        removal("store", s, "deleted on disk since last sync"),
                        options,
                    );
                }
                continue;
            }

            (None, Some(d)) => {
                let sink = if folder_sink {
                    &mut folder_actions
                } else {
                    &mut file_actions
                };
                let disk_changed = match b {
                    None => true,
                    Some(b) => b.sha256.as_deref() != d.sha256.as_deref(),
                };
                if b.is_none() {
                    push(
                        sink,
                        materialise("store", d, older_created_at(None, d.created_at.as_deref())),
                        options,
                    );
                    // The sidecar beside a file being adopted into the store is
                    // its caption; the link it belongs to does not exist until
                    // the create above lands, so the applier resolves it by path.
                    if !d.description.as_deref().unwrap_or("").is_empty()
                        && !is_text_path(&d.relative_path)
                    {
                        let mut action = SyncAction::bare(
                            SyncActionKind::Describe,
                            &d.relative_path,
                            EntryKind::File,
                        );
                        action.side = Some(SyncSide::Store);
                        action.description.clone_from(&d.description);
                        action.last_modified = Some(
                            d.description_updated_at
                                .clone()
                                .unwrap_or_else(|| d.last_modified.clone()),
                        );
                        action.outcome = Some(SyncOutcome::Planned);
                        push(sink, action, options);
                    }
                } else if disk_changed && kind == EntryKind::File {
                    sink.push(conflict_action(
                        &d.relative_path,
                        kind,
                        "edited on disk and deleted in the store",
                    ));
                } else if !options.propagate_deletes {
                    sink.push(skip_action(
                        &d.relative_path,
                        kind,
                        "deleted in the store (--no-delete)",
                    ));
                } else {
                    push(
                        sink,
                        removal("disk", d, "deleted in the store since last sync"),
                        options,
                    );
                }
                continue;
            }

            (Some(s), Some(d)) => (s, d),
        };

        // ---- present on both sides ----------------------------------------
        if kind == EntryKind::Folder {
            // A folder that exists on both sides has nothing to reconcile: its
            // timestamps are not meaningful (every file written into it moves
            // the directory's mtime) and its contents are separate entries.
            continue;
        }

        let content_equal = s.sha256.as_deref() == d.sha256.as_deref();

        if !content_equal {
            match choose_winner(s, d, b, options) {
                None => {
                    file_actions.push(conflict_action(
                        &s.relative_path,
                        EntryKind::File,
                        "both sides changed; --prefer to resolve",
                    ));
                    continue;
                }
                Some(winner) => {
                    let from = if winner == SyncSide::Store { s } else { d };
                    let to = if winner == SyncSide::Store {
                        "disk"
                    } else {
                        "store"
                    };
                    let gap = human_gap(&s.last_modified, &d.last_modified);
                    let reason = if options.prefer != SyncPreference::Newer {
                        format!("--prefer {}", options.prefer.as_str())
                    } else {
                        format!("{} newer by {gap}", winner.as_str())
                    };
                    let mut action = materialise(
                        to,
                        from,
                        older_created_at(s.created_at.as_deref(), d.created_at.as_deref()),
                    );
                    action.kind = SyncActionKind::Modify;
                    action.reason = Some(reason);
                    // The store's path casing is authoritative on the store
                    // side; the disk's on the disk side. A pure-case rename
                    // follows the content winner.
                    action.relative_path = if to == "store" {
                        d.relative_path.clone()
                    } else {
                        s.relative_path.clone()
                    };
                    action.link_id.clone_from(&s.link_id);
                    action.expected_store_sha256.clone_from(&s.sha256);
                    push(&mut file_actions, action, options);
                    if to == "store" {
                        store_rewrites.insert(key.to_string());
                    }
                }
            }
        } else {
            // Bytes agree. Timestamps may not.
            let winner_time = newer_timestamp(s, d, options);
            let target_created_at =
                older_created_at(s.created_at.as_deref(), d.created_at.as_deref());

            if !same_instant(Some(&s.last_modified), Some(&winner_time))
                || !same_instant(s.created_at.as_deref(), target_created_at.as_deref())
            {
                let mut action =
                    SyncAction::bare(SyncActionKind::Touch, &s.relative_path, EntryKind::File);
                action.side = Some(SyncSide::Store);
                action.last_modified = Some(winner_time.clone());
                action.created_at = Some(target_created_at.clone());
                action.reason = Some(format!("mtime {winner_time}"));
                action.link_id.clone_from(&s.link_id);
                action.outcome = Some(SyncOutcome::Planned);
                push(&mut file_actions, action, options);
            }
            let disk_birth_off = can_set_disk_birthtime
                && d.created_at.is_some()
                && !same_instant(d.created_at.as_deref(), target_created_at.as_deref());
            if !same_instant(Some(&d.last_modified), Some(&winner_time)) || disk_birth_off {
                let mut action =
                    SyncAction::bare(SyncActionKind::Touch, &d.relative_path, EntryKind::File);
                action.side = Some(SyncSide::Disk);
                action.last_modified = Some(winner_time.clone());
                // v4 writes `undefined` — the key is OMITTED, not null — where
                // the platform cannot set a birthtime.
                action.created_at = if can_set_disk_birthtime {
                    Some(target_created_at.clone())
                } else {
                    None
                };
                action.reason = Some(format!("mtime {winner_time}"));
                action.outcome = Some(SyncOutcome::Planned);
                push(&mut file_actions, action, options);
            }
        }

        // ---- descriptions --------------------------------------------------
        plan_description(s, d, b, options, &mut file_actions, &mut warnings);
    }

    // A store-side rewrite fans out to the writer's hard-link group, so the
    // siblings' disk copies are stale the moment this run lands. Refresh them in
    // the SAME run rather than leaving the operator to invoke the verb twice.
    plan_group_fan_out(
        store,
        disk,
        &store_rewrites,
        options,
        &mut file_actions,
        &mut warnings,
    );

    // Deletions go last, children before parents, so an `rmdir` finds an empty
    // directory. Everything else runs parents-first.
    let all: Vec<SyncAction> = folder_actions.into_iter().chain(file_actions).collect();
    let mut delete_actions: Vec<SyncAction> = all
        .iter()
        .filter(|a| matches!(a.kind, SyncActionKind::Delete | SyncActionKind::Rmdir))
        .cloned()
        .collect();
    let mut creations: Vec<SyncAction> = all
        .into_iter()
        .filter(|a| !matches!(a.kind, SyncActionKind::Delete | SyncActionKind::Rmdir))
        .collect();

    // Store-side work runs first. A hard-link fan-out reads the sibling's bytes
    // back out of the store, so the write that produced them must already have
    // landed; nothing else depends on the order across sides.
    //
    // `sort_by` is stable, as V8's `Array.prototype.sort` is: two actions that
    // tie on the whole comparator keep the order they were pushed in.
    creations.sort_by(|a, b| {
        store_first_rank(a)
            .cmp(&store_first_rank(b))
            .then(by_path_depth(a, b, 1))
    });
    delete_actions.sort_by(|a, b| by_path_depth(a, b, -1));

    actions.extend(creations);
    actions.extend(delete_actions);
    PlanOutput { actions, warnings }
}

// ============================================================================
// Pieces
// ============================================================================

/// A disk-side guess at whether a path will land in the store as a text document
/// rather than a blob. The store walk knows the real `fileType`, but on a
/// first-run adoption there is no store entry yet, and only a blob can carry a
/// caption.
fn is_text_path(relative_path: &str) -> bool {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)\.(md|markdown|txt|json|jsonl|ndjson)$").unwrap())
        .is_match(relative_path)
}

fn is_keystone(relative_path: &str) -> bool {
    CHARACTER_VAULT_KEYSTONES.contains(&relative_path.to_lowercase().as_str())
}

/// v4 `storeFirstThen`'s rank: store-side actions before disk-side ones, and
/// side-less ones (conflict / skip) last.
fn store_first_rank(a: &SyncAction) -> u8 {
    match a.side {
        Some(SyncSide::Store) => 0,
        Some(SyncSide::Disk) => 1,
        None => 2,
    }
}

/// v4 `byPathDepth`: folders before files at the same depth, then
/// shallow-to-deep (or the reverse).
fn by_path_depth(a: &SyncAction, b: &SyncAction, sign: i32) -> std::cmp::Ordering {
    let depth = |p: &str| p.split('/').count() as i32;
    let by_depth = (depth(&a.relative_path) - depth(&b.relative_path)) * sign;
    if by_depth != 0 {
        return by_depth.cmp(&0);
    }
    if a.entry_kind != b.entry_kind {
        let v = if a.entry_kind == EntryKind::Folder {
            -sign
        } else {
            sign
        };
        return v.cmp(&0);
    }
    // v4 `localeCompare` — true ICU collation, not a byte compare.
    locale_compare(&a.relative_path, &b.relative_path)
}

fn conflict_action(relative_path: &str, entry_kind: EntryKind, reason: &str) -> SyncAction {
    let mut action = SyncAction::bare(SyncActionKind::Conflict, relative_path, entry_kind);
    action.reason = Some(reason.to_string());
    action.outcome = Some(SyncOutcome::Skipped);
    action
}

fn skip_action(relative_path: &str, entry_kind: EntryKind, reason: &str) -> SyncAction {
    let mut action = SyncAction::bare(SyncActionKind::Skip, relative_path, entry_kind);
    action.reason = Some(reason.to_string());
    action.outcome = Some(SyncOutcome::Skipped);
    action
}

/// v4 `materialise` — `create`/`mkdir` on `side`, taking `from`'s content and
/// clocks.
fn materialise(side: &str, from: &SyncEntry, created_at: Option<String>) -> SyncAction {
    let kind = if from.kind() == EntryKind::Folder {
        SyncActionKind::Mkdir
    } else {
        SyncActionKind::Create
    };
    let mut action = SyncAction::bare(kind, &from.relative_path, from.kind());
    action.side = Some(if side == "store" {
        SyncSide::Store
    } else {
        SyncSide::Disk
    });
    action.last_modified = Some(from.last_modified.clone());
    action.created_at = Some(created_at);
    action.sha256.clone_from(&from.sha256);
    action.size_bytes = from.size_bytes;
    action.link_id.clone_from(&from.link_id);
    action.outcome = Some(SyncOutcome::Planned);
    action
}

fn removal(side: &str, from: &SyncEntry, reason: &str) -> SyncAction {
    let kind = if from.kind() == EntryKind::Folder {
        SyncActionKind::Rmdir
    } else {
        SyncActionKind::Delete
    };
    let mut action = SyncAction::bare(kind, &from.relative_path, from.kind());
    action.side = Some(if side == "store" {
        SyncSide::Store
    } else {
        SyncSide::Disk
    });
    action.reason = Some(reason.to_string());
    action.link_id.clone_from(&from.link_id);
    action.outcome = Some(SyncOutcome::Planned);
    action
}

/// v4 `push` — `--direction` filters the plan; a filtered-out action becomes a
/// `skip` line.
fn push(sink: &mut Vec<SyncAction>, action: SyncAction, options: &SyncOptions) {
    if options.direction == SyncDirection::Both || action.side.is_none() {
        sink.push(action);
        return;
    }
    let allowed = if options.direction == SyncDirection::ToDisk {
        SyncSide::Disk
    } else {
        SyncSide::Store
    };
    if action.side == Some(allowed) {
        sink.push(action);
    } else {
        sink.push(skip_action(
            &action.relative_path,
            action.entry_kind,
            &format!("--direction {}", options.direction.as_str()),
        ));
    }
}

fn newer_timestamp(s: &SyncEntry, d: &SyncEntry, options: &SyncOptions) -> String {
    match options.prefer {
        SyncPreference::Store => s.last_modified.clone(),
        SyncPreference::Disk => d.last_modified.clone(),
        SyncPreference::Newer => {
            let sm = time_of(Some(&s.last_modified)).unwrap_or(0);
            let dm = time_of(Some(&d.last_modified)).unwrap_or(0);
            if sm >= dm {
                s.last_modified.clone()
            } else {
                d.last_modified.clone()
            }
        }
    }
}

/// v4 `chooseWinner` — which side's content wins; `None` is v4's `'conflict'`.
///
/// `--prefer store|disk` is absolute: the operator has said which side is right,
/// and that is the whole point of the flag. Otherwise the base decides whether
/// this is a one-sided edit (resolve by the newer clock) or a genuine divergence
/// (refuse).
fn choose_winner(
    s: &SyncEntry,
    d: &SyncEntry,
    b: Option<&ManifestEntry>,
    options: &SyncOptions,
) -> Option<SyncSide> {
    match options.prefer {
        SyncPreference::Store => return Some(SyncSide::Store),
        SyncPreference::Disk => return Some(SyncSide::Disk),
        SyncPreference::Newer => {}
    }

    // v4 `if (b?.sha256)` — a falsy sha (absent OR the empty string) falls
    // through to the clocks.
    if let Some(base_sha) = b
        .and_then(|b| b.sha256.as_deref())
        .filter(|s| !s.is_empty())
    {
        let store_changed = s.sha256.as_deref() != Some(base_sha);
        let disk_changed = d.sha256.as_deref() != Some(base_sha);
        if store_changed && disk_changed {
            return None;
        }
        if store_changed {
            return Some(SyncSide::Store);
        }
        if disk_changed {
            return Some(SyncSide::Disk);
        }
        // Neither matches the base yet both differ from each other: the base is
        // stale in a way that cannot be reconciled from here.
        return None;
    }

    // No base — a first run over a directory that already has content. The
    // clocks are all there is.
    let sm = time_of(Some(&s.last_modified)).unwrap_or(0);
    let dm = time_of(Some(&d.last_modified)).unwrap_or(0);
    if (sm - dm).abs() <= MTIME_TOLERANCE_MS {
        return None;
    }
    Some(if sm > dm {
        SyncSide::Store
    } else {
        SyncSide::Disk
    })
}

/// The sidecar half. Text documents are out of scope entirely — their
/// `description` has no disk home and is reported once as a `skip`.
fn plan_description(
    s: &SyncEntry,
    d: &SyncEntry,
    b: Option<&ManifestEntry>,
    options: &SyncOptions,
    sink: &mut Vec<SyncAction>,
    warnings: &mut Vec<String>,
) {
    if !has_sidecar(s.file_type.as_deref()) {
        if !s.description.as_deref().unwrap_or("").is_empty() {
            sink.push(skip_action(
                &s.relative_path,
                EntryKind::File,
                "description on a text document is not synced",
            ));
        }
        return;
    }

    let store_text = s.description.clone().unwrap_or_default();
    let disk_text = d.description.clone().unwrap_or_default();
    let disk_has_sidecar = d.description.is_some();

    if descriptions_equal(Some(&store_text), Some(&disk_text))
        && (disk_has_sidecar || store_text.is_empty())
    {
        return;
    }

    let base_sha = b.and_then(|b| b.description_sha256.as_deref());
    let store_changed = match base_sha {
        None => true,
        Some(base_sha) => description_sha256(&store_text) != base_sha,
    };
    let disk_changed = match base_sha {
        None => true,
        Some(base_sha) => description_sha256(&disk_text) != base_sha,
    };

    // A sidecar the operator deleted while the store's caption stood still is a
    // deliberate clearing, not a conflict — but only the base can tell the two
    // apart, so with no base an absent sidecar simply takes the store's text.
    if !disk_has_sidecar {
        if base_sha.is_some() && !store_changed && options.propagate_deletes {
            let mut action =
                SyncAction::bare(SyncActionKind::Describe, &s.relative_path, EntryKind::File);
            action.side = Some(SyncSide::Store);
            action.description = Some(String::new());
            action.reason = Some("sidecar deleted on disk (cleared)".to_string());
            action.link_id.clone_from(&s.link_id);
            action.outcome = Some(SyncOutcome::Planned);
            push(sink, action, options);
        } else if !store_text.is_empty() {
            push(
                sink,
                describe_disk(
                    d,
                    &store_text,
                    s.description_updated_at.as_deref(),
                    &s.last_modified,
                ),
                options,
            );
        }
        return;
    }

    if options.prefer == SyncPreference::Store
        || (options.prefer == SyncPreference::Newer && store_changed && !disk_changed)
    {
        push(
            sink,
            describe_disk(
                d,
                &store_text,
                s.description_updated_at.as_deref(),
                &s.last_modified,
            ),
            options,
        );
        return;
    }
    if options.prefer == SyncPreference::Disk
        || (options.prefer == SyncPreference::Newer && disk_changed && !store_changed)
    {
        push(
            sink,
            describe_store(
                s,
                &disk_text,
                d.description_updated_at.as_deref(),
                &d.last_modified,
            ),
            options,
        );
        return;
    }

    // Both moved since the base (or there is no base and they simply differ):
    // fall back to the sidecar's own clock, and refuse when even that is a tie.
    let store_at = time_of(s.description_updated_at.as_deref())
        .or_else(|| time_of(Some(&s.last_modified)))
        .unwrap_or(0);
    let disk_at = time_of(d.description_updated_at.as_deref())
        .or_else(|| time_of(Some(&d.last_modified)))
        .unwrap_or(0);
    if base_sha.is_some() && store_changed && disk_changed {
        sink.push(conflict_action(
            &s.relative_path,
            EntryKind::File,
            "the caption changed in the store and in the sidecar; --prefer to resolve",
        ));
        warnings.push(format!("Caption conflict on {}", s.relative_path));
        return;
    }
    if (store_at - disk_at).abs() <= MTIME_TOLERANCE_MS {
        sink.push(conflict_action(
            &s.relative_path,
            EntryKind::File,
            "the caption differs and both sides carry the same clock; --prefer to resolve",
        ));
        return;
    }
    if store_at > disk_at {
        push(
            sink,
            describe_disk(
                d,
                &store_text,
                s.description_updated_at.as_deref(),
                &s.last_modified,
            ),
            options,
        );
    } else {
        push(
            sink,
            describe_store(
                s,
                &disk_text,
                d.description_updated_at.as_deref(),
                &d.last_modified,
            ),
            options,
        );
    }
}

fn describe_disk(
    d: &SyncEntry,
    text: &str,
    updated_at: Option<&str>,
    fallback: &str,
) -> SyncAction {
    let mut action = SyncAction::bare(SyncActionKind::Describe, &d.relative_path, EntryKind::File);
    action.side = Some(SyncSide::Disk);
    action.description = Some(text.to_string());
    action.last_modified = Some(updated_at.unwrap_or(fallback).to_string());
    action.outcome = Some(SyncOutcome::Planned);
    action
}

fn describe_store(
    s: &SyncEntry,
    text: &str,
    updated_at: Option<&str>,
    fallback: &str,
) -> SyncAction {
    let mut action = SyncAction::bare(SyncActionKind::Describe, &s.relative_path, EntryKind::File);
    action.side = Some(SyncSide::Store);
    action.description = Some(text.to_string());
    action.last_modified = Some(updated_at.unwrap_or(fallback).to_string());
    action.link_id.clone_from(&s.link_id);
    action.outcome = Some(SyncOutcome::Planned);
    action
}

/// A store write repoints every member of the writer's hard-link group at the
/// new content row, so each sibling's disk copy is stale the instant this run
/// applies. Emit the second `modify disk` now, from the same bytes, so the run
/// converges without a second invocation.
///
/// Two members of one group edited differently on disk in one run cannot both
/// win — the group is one file — so both are refused.
fn plan_group_fan_out(
    store: &SyncEntryMap,
    disk: &SyncEntryMap,
    store_rewrites: &HashSet<String>,
    options: &SyncOptions,
    sink: &mut Vec<SyncAction>,
    warnings: &mut Vec<String>,
) {
    if store_rewrites.is_empty() {
        return;
    }

    // v4 builds a `Map<groupId, keys[]>` by iterating the store map, so both the
    // group order and the member order are the store walk's.
    let mut by_group: OrderedMap<Vec<String>> = OrderedMap::new();
    for (key, entry) in store.iter() {
        let Some(group_id) = entry.link_group_id.as_deref() else {
            continue;
        };
        match by_group.get_mut(group_id) {
            Some(members) => members.push(key.to_string()),
            None => by_group.insert(group_id, vec![key.to_string()]),
        }
    }

    for (group_id, members) in by_group.iter() {
        let written: Vec<&String> = members
            .iter()
            .filter(|m| store_rewrites.contains(*m))
            .collect();
        if written.is_empty() {
            continue;
        }
        if written.len() > 1 {
            for key in &written {
                let entry = store.get(key).expect("store key from the store walk");
                sink.push(conflict_action(
                    &entry.relative_path,
                    EntryKind::File,
                    &format!(
                        "two members of one hard-link group changed differently on disk (group {})",
                        js_slice_8(group_id)
                    ),
                ));
            }
            warnings.push(format!(
                "Hard-link group {} was edited at more than one of its paths",
                js_slice_8(group_id)
            ));
            continue;
        }

        let first = written[0];
        let source = store.get(first).expect("store key from the store walk");
        for key in members {
            if key == first {
                continue;
            }
            let (Some(sibling), Some(on_disk)) = (store.get(key), disk.get(key)) else {
                continue;
            };
            // The sibling's store copy is about to become the written bytes; its
            // disk copy must follow, sourced from the path that actually changed.
            let mut action = SyncAction::bare(
                SyncActionKind::Modify,
                &on_disk.relative_path,
                EntryKind::File,
            );
            action.side = Some(SyncSide::Disk);
            action.reason = Some(format!("hard-linked to {}", source.relative_path));
            action.last_modified = Some(source.last_modified.clone());
            action.created_at = Some(older_created_at(
                sibling.created_at.as_deref(),
                on_disk.created_at.as_deref(),
            ));
            action.link_id.clone_from(&sibling.link_id);
            action.outcome = Some(SyncOutcome::Planned);
            push(sink, action, options);
        }
    }
}

/// `groupId.slice(0, 8)` on a JS string — UTF-16 code units. Group ids are
/// UUIDs, so this is ASCII in practice; the guard keeps a non-ASCII id from
/// panicking on a byte boundary.
fn js_slice_8(s: &str) -> String {
    s.chars().take(8).collect()
}
