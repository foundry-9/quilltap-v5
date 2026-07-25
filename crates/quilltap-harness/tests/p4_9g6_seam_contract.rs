//! The unification wire for the "finish the restore side" round
//! (P4.9G5-resumed ∥ P4.9G6) — the one proof neither lane could run alone.
//!
//! ## Why this file exists instead of a flipped call site
//!
//! The round's Shared contract §2 made `remap_backup_data` a cross-lane seam:
//! **P4.9G6** delivers it, **P4.9G5** consumes it as restore's `new-account`
//! pre-step, and the unifier flips an `ACTIVATE-AT-UNIFY` marker. P4.9G5 landed
//! unit 3 (parse / preview / upload) but **not** units 4–5 — the orchestrator is
//! blocked on a human ruling about two v4 restore bugs (see the order's status
//! header and `dogfood-findings.md`). So there is no call site to flip yet.
//!
//! That leaves the seam unexercised at exactly the moment the two lanes stop
//! being able to check each other, which is the failure mode a unification wire
//! exists to prevent. This file substitutes the two obligations that ARE
//! dischargeable now:
//!
//! 1. **The §2 signature is pinned**, so the unit-4 call site cannot be written
//!    against a shape that has since drifted. Function-item coercions do the
//!    pinning at COMPILE time — if any signature moves, this file stops
//!    building, which is louder than a failing assertion.
//! 2. **The composition is proven end to end**: G5's `parse_backup_zip` reads a
//!    committed archive off disk, and G6's `remap_backup_data` consumes the
//!    `BackupData` it produces. Each lane proved its own half against v4; only
//!    here do the halves meet, and meeting is the thing unit 4 will do on its
//!    first line.
//!
//! This is NOT a differential — both halves already have one
//! (`system_restore_equivalence`, `backup_uuid_remap_equivalence`). It is a
//! contract-and-composition test, in the spirit of `p4_9g1_wire_contract`, and it
//! needs no oracle and no env var so it can never silently skip.
//!
//! **When unit 4 lands, keep this file.** The signature pins stay useful, and
//! the composition case becomes the cheap smoke test under the real
//! orchestrator.

use std::collections::HashSet;
use std::path::PathBuf;

use quilltap_core::services::backup::restore::parse_backup_zip;
use quilltap_core::services::backup::uuid_remap::remap_backup_data;
use quilltap_core::services::backup::uuid_remapper::UuidRemapper;
use quilltap_core::services::backup::BackupData;
use serde_json::Value;

// ── §2, pinned at compile time ───────────────────────────────────────────────
//
// Written out exactly as the order's Shared contract §2 states them. A coercion
// to a `fn` pointer accepts only an identical signature, so a changed parameter
// type, a changed order, or a changed return type is a build failure here.

const _REMAP_BACKUP_DATA: fn(&BackupData, &str, &mut UuidRemapper) -> BackupData =
    remap_backup_data;

#[allow(clippy::type_complexity)]
const _UUID_REMAPPER_API: (
    fn() -> UuidRemapper,
    fn(Box<dyn FnMut() -> String + Send>) -> UuidRemapper,
    fn(&mut UuidRemapper, &Value) -> String,
    fn(&mut UuidRemapper, &str) -> String,
    fn(&mut UuidRemapper, &Value) -> Value,
    fn(&mut UuidRemapper, &Value, &[&str]) -> Value,
    fn(&mut UuidRemapper, &Value, &[&str]) -> Value,
    fn(&UuidRemapper) -> Vec<(String, String)>,
    fn(&mut UuidRemapper),
    fn(&UuidRemapper) -> usize,
) = (
    UuidRemapper::new,
    UuidRemapper::with_id_source,
    UuidRemapper::remap,
    UuidRemapper::remap_str,
    UuidRemapper::remap_array,
    UuidRemapper::remap_fields,
    UuidRemapper::remap_array_fields,
    UuidRemapper::mapping,
    UuidRemapper::clear,
    UuidRemapper::size,
);

// ── The composition ─────────────────────────────────────────────────────────

fn archives_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../quilltap-web/tests/fixtures/restore-archives")
}

/// A private scratch root, removed on drop — the same discipline
/// `system_restore_equivalence` uses, so a leaked extract directory shows up as
/// a non-empty root rather than as disk that quietly accumulates.
struct Scratch {
    root: PathBuf,
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn fresh_scratch(tag: &str) -> Scratch {
    let root = std::env::temp_dir().join(format!("qt-g6seam-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    Scratch { root }
}

/// Every `id` present across the collections this archive actually populates.
fn collected_ids(data: &BackupData) -> HashSet<String> {
    let mut ids = HashSet::new();
    let mut take = |rows: &Vec<Value>| {
        for row in rows {
            if let Some(id) = row.get("id").and_then(Value::as_str) {
                ids.insert(id.to_string());
            }
        }
    };
    take(&data.characters);
    take(&data.chats);
    take(&data.tags);
    take(&data.connection_profiles);
    take(&data.image_profiles);
    take(&data.embedding_profiles);
    take(&data.memories);
    take(&data.files);
    take(&data.projects);
    take(&data.groups);
    take(&data.folders);
    take(&data.doc_mount_points);
    take(&data.doc_mount_files);
    take(&data.doc_mount_file_links);
    ids
}

/// The seam: G5 parses the archive, G6 remaps what it parsed.
///
/// This asserts the *composition*, not the remap's correctness — the remap is
/// proven byte-for-byte against v4 over 19 cases in
/// `backup_uuid_remap_equivalence`, and duplicating that claim here would just
/// be a second, weaker copy of it. What can only be checked here is that the
/// type P4.9G5's parser hands back is the type P4.9G6's remap accepts, and that
/// running one on the other's output does something coherent.
#[test]
fn parse_then_remap_composes() {
    let scratch = fresh_scratch("compose");
    let parsed = parse_backup_zip(&archives_dir().join("restore-archive.zip"), &scratch.root)
        .expect("the committed full archive parses");

    let before = collected_ids(&parsed.data);
    assert!(
        before.len() > 10,
        "the committed archive should carry a real graph, not a handful of rows; got {}",
        before.len()
    );

    // A deterministic id source, exactly as the differential runs it — so a
    // failure here is legible instead of a wall of v4 UUIDs.
    let mut n = 0u64;
    let mut remapper = UuidRemapper::with_id_source(Box::new(move || {
        n += 1;
        format!("00000000-0000-4000-8000-{n:012}")
    }));

    let remapped = remap_backup_data(&parsed.data, "seam-target-user", &mut remapper);

    // Shape is preserved: the remap rewrites ids, it never adds or drops rows.
    assert_eq!(remapped.characters.len(), parsed.data.characters.len());
    assert_eq!(remapped.chats.len(), parsed.data.chats.len());
    assert_eq!(remapped.files.len(), parsed.data.files.len());
    assert_eq!(
        remapped.doc_mount_points.len(),
        parsed.data.doc_mount_points.len()
    );

    // Every id the parser produced is gone, and every id the remap produced is
    // new. This is the property unit 4 depends on: restoring into an existing
    // instance must not collide with a single row already there.
    let after = collected_ids(&remapped);
    assert!(
        after.is_disjoint(&before),
        "new-account remap left {} original id(s) in place",
        after.intersection(&before).count()
    );
    assert_eq!(
        after.len(),
        before.len(),
        "the remap should be a bijection over the ids it rewrites"
    );

    // The memo is what keeps cross-references connected; it must have been
    // consulted, and it must cover at least the ids we counted.
    assert!(
        remapper.size() >= before.len(),
        "memo holds {} entries for {} ids",
        remapper.size(),
        before.len()
    );

    // Ownership moved to the target user wherever v4 reassigns it.
    for row in &remapped.tags {
        assert_eq!(
            row.get("userId").and_then(Value::as_str),
            Some("seam-target-user"),
            "tags carry the target userId"
        );
    }
}

/// The manifest is deliberately outside the §2 transform (v5's `BackupData`
/// carries no manifest field), so the caller — unit 4 — is the one that must
/// carry it forward. Pin that here so the split cannot be forgotten when the
/// orchestrator is written.
#[test]
fn manifest_is_the_callers_responsibility() {
    let scratch = fresh_scratch("manifest");
    let parsed = parse_backup_zip(&archives_dir().join("restore-archive.zip"), &scratch.root)
        .expect("the committed full archive parses");

    assert!(
        parsed.manifest.get("backupFormat").is_some(),
        "the parser exposes the manifest beside the collections"
    );
    assert_eq!(
        parsed.backup_format(),
        parsed.manifest.get("backupFormat").and_then(Value::as_i64),
        "and `backup_format()` reads it off that same manifest"
    );
}
