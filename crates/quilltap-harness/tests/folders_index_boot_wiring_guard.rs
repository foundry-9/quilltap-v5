//! The bug-114 boot-ensure wiring census (P4.D145, v4 `a5df98b3f`).
//!
//! `folders_collapse_heal_equivalence` proves the pass itself against v4's REAL
//! migration, but it calls the function directly. Deleting the boot call would
//! leave that differential — and every other test in the workspace — green while
//! no instance ever gained the index: the same log-only/wiring blind spot the
//! P4.D89 turn-end notice hit. So the call site is held here, mechanically.
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
