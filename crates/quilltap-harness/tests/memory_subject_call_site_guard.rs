//! P4.D153 — the three memory-subject call sites, pinned where they live
//! (v4 `d883a5ee1`, bug 122).
//!
//! `build_memory_subject_context` is what makes the self-facing blocks name
//! their subject. Its correctness is proven by unit tests and by the tier-1
//! `memory_injector_equivalence` corpus; what neither of those can see is
//! **who calls it, and how many times**. Three things can regress silently:
//!
//!   * a call site drops back to a bare `MemorySubjectContext::default()` —
//!     the formatters still compile, the corpus still passes, and every line
//!     about someone else silently goes back to reading as autobiography;
//!   * the per-turn build resolves the two pools SEPARATELY, doubling the
//!     lookup on every multi-character turn (v4 builds ONE context over the
//!     union of `frozenArchive` and `dynamicHeadResults.map(r => r.memory)`
//!     precisely so the turn pays for one query);
//!   * the union loses one of its two halves, which no differential over a
//!     fixture whose archive and head draw from the same store can detect.
//!
//! v4's three sites: `context-manager.ts:1495` (the per-turn build),
//! `carina.service.ts:229` (the answerer's recall) and
//! `character-voiced.ts:131` (the announcement recall). A source census in the
//! `db_error_key_guard` / `lora_log_anchor_guard` idiom.
//!
//! Run standalone:
//!   cargo test -p quilltap-harness --test memory_subject_call_site_guard

use std::path::PathBuf;

fn source(rel: &str) -> String {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../quilltap-core/src")
        .join(rel);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("read {}: {e}", p.display()))
}

const CALL: &str = "build_memory_subject_context(";

/// Exactly one build per v4 call site, and no fourth site that slipped in
/// unreviewed. (`services/memory_subject.rs` holds the definition and its
/// tests, so it is not counted here.)
#[test]
fn the_three_v4_call_sites_and_no_others() {
    for (file, want) in [
        ("services/build_context.rs", 1usize),
        ("services/carina_query.rs", 1),
        ("services/announcer/character_voiced.rs", 1),
    ] {
        let n = source(file).matches(CALL).count();
        assert_eq!(
            n, want,
            "{file}: expected {want} `{CALL}` call(s), found {n}"
        );
    }
}

/// The per-turn build is ONE lookup over the UNION of both pools — v4's
/// `[...frozenArchive, ...dynamicHeadResults.map(r => r.memory)]`.
#[test]
fn the_per_turn_build_spans_both_pools_in_one_lookup() {
    let src = source("services/build_context.rs");
    let at = src
        .find(CALL)
        .expect("the per-turn build has moved or gone");
    // The argument list, up to the `format_frozen_memory_archive` that follows it.
    let end = src[at..]
        .find("format_frozen_memory_archive(")
        .expect("the archive format call no longer follows the subject build");
    let call = &src[at..at + end];
    assert!(
        call.contains("frozen_archive"),
        "the union must carry the frozen archive:\n{call}"
    );
    assert!(
        call.contains("dynamic_head_results"),
        "the union must carry the dynamic head:\n{call}"
    );
    assert!(
        call.contains(".chain("),
        "both pools must reach ONE lookup (v4 spreads them into a single array):\n{call}"
    );
}

/// Every self-facing formatter call in production passes the resolved context,
/// never a default. `MemorySubjectContext::default()` has an empty
/// `self_character_id`, so a call site that reached for it would prefix the
/// character's OWN memories — the opposite of the fix.
#[test]
fn no_production_call_site_formats_under_a_default_context() {
    for file in [
        "services/build_context.rs",
        "services/carina_query.rs",
        "services/announcer/character_voiced.rs",
    ] {
        let src = source(file);
        assert!(
            !src.contains("MemorySubjectContext::default()"),
            "{file} formats a self-facing block under a default subject context"
        );
    }
}
