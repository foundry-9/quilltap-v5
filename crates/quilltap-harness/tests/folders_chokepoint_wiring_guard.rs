//! The bug-114 wiring census (P4.D145, v4 `a5df98b3f`): the boot ensure, and
//! every folder writer that must go through the `ensure_by_path` chokepoint.
//!
//! **Both halves are differential-blind, for the same reason.**
//!
//! `folders_collapse_heal_equivalence` proves the collapse pass itself against
//! v4's REAL migration, but it calls the function directly — deleting the boot
//! call would leave that differential, and every other test in the workspace,
//! green while no instance ever gained the index (the wiring blind spot the
//! P4.D89 turn-end notice hit).
//!
//! The call sites are worse: `create` and `ensure_by_path` differ ONLY when the
//! read-then-write is raced, or when the read and the index disagree. Neither is
//! reachable from a sequential op list, so reverting a call site to `create` is
//! MEASURED green across `qtap_import_equivalence`, `files_routes_equivalence`
//! and both image tier-3 families (measured, this lane — the
//! `a-differential-cannot-see-a-dropped-batch` class). The chokepoint's own
//! behaviour is pinned by `db::folders`'s nine unit tests and the
//! `folders_remap_tier2_equivalence` constraint arm; that it is USED is pinned
//! here.
//!
//! The second half is the measured NEGATIVE: `services/provisioning/mod.rs` must
//! NOT call the ensure. v4's fresh generateDDL surface cannot express a COALESCE
//! index, so `provisioning_equivalence` compares v5's provisioned
//! `sqlite_master` against a v4 dump that does not carry it; creating it at
//! provisioning time reddens that family on `schema mismatch in partition main`
//! (measured at the `a5df98b3f` pin, then reverted). Fresh instances get the
//! index from the boot chain instead — `Host::assemble` runs `seed_built_ins` on
//! every open, including the first one after Setup.
//!
//! Run standalone (no oracle):
//!   cargo test -p quilltap-harness --test folders_index_boot_wiring_guard

use std::path::PathBuf;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(std::path::Path::parent)
        .expect("repo root")
        .to_path_buf()
}

fn read(rel: &str) -> String {
    let p = repo_root().join(rel);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("read {}: {e}", p.display()))
}

#[test]
fn the_boot_chain_runs_the_folders_collapse_ensure() {
    let src = read("crates/quilltap-host/src/host.rs");
    let idx = src
        .find("fn seed_built_ins")
        .expect("host.rs must still have the boot repair chain in seed_built_ins");
    let chain = &src[idx..];
    assert!(
        chain.contains(
            "quilltap_core::db::folders_unique_path_repair::ensure_folders_unique_path_index("
        ),
        "the bug-114 collapse-then-index ensure must be called from the boot \
         repair chain in `seed_built_ins` — without it no instance ever gains \
         `idx_folders_userId_projectId_path`, and every other test stays green"
    );
    assert!(
        chain.contains("\"Collapsed duplicate folder rows\""),
        "v4's own success log line must ride with it"
    );
}

#[test]
fn provisioning_does_not_create_the_index() {
    let src = read("crates/quilltap-core/src/services/provisioning/mod.rs");
    assert!(
        !src.contains("folders_unique_path_repair"),
        "provisioning must NOT create the bug-114 index: v4's fresh generateDDL \
         surface cannot express a COALESCE index, so `provisioning_equivalence` \
         would red on `schema mismatch in partition main`. Fresh instances take \
         it from the boot chain, which runs on every open."
    );
}

/// Every v5 folder writer v4 `a5df98b3f` converted, with the function the call
/// must sit in. A site reverted to `create` fails here by name.
const CHOKEPOINT_SITES: &[(&str, &str, &str)] = &[
    (
        "crates/quilltap-core/src/services/character_avatar_job.rs",
        "fn ensure_legacy_folder",
        "v4 `character-avatar.ts`: the `/character-avatars/` legacy row — one of          the two machine-written paths that grew the duplicates",
    ),
    (
        "crates/quilltap-core/src/services/story_background_job.rs",
        "fn ensure_legacy_folder",
        "v4 `story-background.ts`: the `/story-backgrounds/` legacy row — the          other one",
    ),
    (
        "crates/quilltap-core/src/api/files.rs",
        "fn ensure_parent_folders_exist",
        "v4 `folders/route.ts` `ensureParentFoldersExist`: the recursive parent          chain (v4 keeps the surrounding guard and the recursion)",
    ),
    (
        "crates/quilltap-core/src/api/files.rs",
        "pub async fn files_folder_create",
        "v4 `folders/route.ts` `handleCreateFolder`: the create branch. The          idempotent `find_by_path` arm above it STAYS — that is what answers 200          `alreadyExists: true`",
    ),
    (
        "crates/quilltap-core/src/services/quilltap_import/files.rs",
        "fn import_folders",
        "v4 `import-files.ts` `importFolders`: the reuse branch above it is the          reuse-REPORTING branch, not the uniqueness guarantee",
    ),
];

/// v4 sites with **no v5 counterpart**. Each row is recorded rather than
/// converted; if one of these surfaces ever lands in v5 it inherits the
/// obligation.
const NO_V5_COUNTERPART: &[(&str, &str)] = &[
    (
        "lib/file-storage/watcher.ts handleDirAdd",
        "v5 ships no file-storage watcher at all (recorded at          services/character_archive/service.rs and services/mount_index/         file_ops.rs). v4 also relabelled its log line `Created folder record          for new directory on disk` -> `Ensured folder record ...`; nothing to          relabel here.",
    ),
    (
        "lib/background-jobs/child/child-repositories-proxy.ts METHOD_OVERRIDES",
        "v4 buffers `folders.ensureByPath` WHOLE in the forked child so the          parent replays it where read-your-writes and the index hold, and its          in-child callers must discard the return. v5's job runner is          in-process with the real connection, so there is nothing to buffer.          The routing half IS pinned — write_partition_equivalence carries the          `folders.ensureByPath` -> main classify row.",
    ),
    (
        "lib/startup/prettify.ts PRETTY_LABELS",
        "v4's migration-runner progress screen has no v5 analogue — the          standing deliberate non-port recorded at          db/chat_activity_recompute_heal.rs.",
    ),
];

#[test]
fn every_converted_writer_goes_through_the_chokepoint() {
    for (file, func, why) in CHOKEPOINT_SITES {
        let src = read(file);
        let at = src
            .find(func)
            .unwrap_or_else(|| panic!("{file}: `{func}` is gone — {why}"));
        // The window is generous on purpose: the point is that THIS function
        // reaches the chokepoint, not where in it the call sits.
        let window = &src[at..(at + 3000).min(src.len())];
        assert!(
            window.contains("ensure_by_path("),
            "{file} :: {func} must write folders through              `FoldersRepository::ensure_by_path` (v4 `a5df98b3f`, bug 114) —              {why}.

Reverting it to `create` is invisible to every              differential: the two differ only under a race or when the read              and the index disagree, neither of which a sequential op list can              reach."
        );
    }
}

#[test]
fn the_two_private_find_folder_by_path_copies_are_gone() {
    // v4 `a5df98b3f` deleted the hand-rolled guard at both image handlers; v5
    // additionally carried a private `find_folder_by_path` copy in each, which
    // existed only to serve that guard.
    for file in [
        "crates/quilltap-core/src/services/character_avatar_job.rs",
        "crates/quilltap-core/src/services/story_background_job.rs",
    ] {
        assert!(
            !read(file).contains("fn find_folder_by_path"),
            "{file} still carries the private find_folder_by_path copy the              chokepoint replaced"
        );
    }
}

#[test]
fn the_no_counterpart_rows_are_recorded() {
    // The census IS the record: this asserts the table has not been quietly
    // emptied, so a future lane porting one of those surfaces still finds the
    // obligation written down.
    assert_eq!(
        NO_V5_COUNTERPART.len(),
        3,
        "the v4 sites with no v5 counterpart are part of the record"
    );
    for (site, why) in NO_V5_COUNTERPART {
        assert!(
            !site.is_empty() && why.len() > 40,
            "{site}: give the reason"
        );
    }
}

#[test]
fn the_index_name_matches_v4() {
    // The name IS the cross-app once-only marker (v4's `shouldRun()` is
    // `!indexExists()` by this exact string), so a typo would silently make
    // both apps re-run their pass forever.
    assert_eq!(
        quilltap_core::db::folders_unique_path_repair::FOLDERS_UNIQUE_PATH_INDEX,
        "idx_folders_userId_projectId_path"
    );
}
