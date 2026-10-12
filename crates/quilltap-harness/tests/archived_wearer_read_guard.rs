//! P4.D120 — the OPPOSITE-direction archived pin (v4 `d25dacc1`'s
//! `resolve-equipped` suite): **a garment archived mid-chat stays worn.**
//!
//! Archiving hides a garment from the AUDITION, not from the wearer. Since v4
//! `9ecebf986` (P4.D279) that split lives in ONE place — the wearable pool
//! (`crate::wearable_pool`):
//!
//!   * `WearablePool::wearable()` — the candidate set the outfit LLM is shown
//!     — drops archived items from EACH tier BEFORE shadowing (so an archived
//!     personal copy never hides a live shared one), pinned end-to-end by
//!     `outfit_llm_choose_tier3_equivalence`'s never-auditions rows and by
//!     `wearable_pool_equivalence`;
//!   * `WearablePool::by_id()` / `get()` / `get_many()` — the already-equipped
//!     ids, whose TITLES would otherwise vanish from an outfit the moment
//!     someone archived a worn piece — keep archived items, and
//!     `find_by_title` finds an archived OWN item (so `wardrobe_archive` can
//!     name one) but never an archived SHARED one.
//!
//! The contract is pinned where it lives — in the source — the way this repo
//! pins other load-bearing arms (`db_error_key_guard`,
//! `outfit_instructions_wiring_guard`). Flipping either side fails this test:
//! the audition must filter per tier, the by-id layer must not filter at all.
//!
//! Run standalone:
//!   cargo test -p quilltap-harness --test archived_wearer_read_guard

use std::path::PathBuf;

/// `src[start..start+len]`, clamped to the file (the shims sit at the END of
/// `wardrobe_read.rs`, so a fixed-width window would run off it).
fn window(src: &str, start: usize, len: usize) -> &str {
    &src[start..src.len().min(start + len)]
}

fn source(rel: &str) -> String {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../quilltap-core/src")
        .join(rel);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("read {}: {e}", p.display()))
}

#[test]
fn the_audition_excludes_archived_per_tier_and_the_by_id_layer_keeps_them() {
    let src = source("wearable_pool.rs");
    // The audition: `wearable()` filters EACH tier before shadowing.
    let wearable_start = src
        .find("pub fn wearable(&self) -> &[Value] {")
        .expect("the pool's wearable() exists");
    let wearable_body = &window(&src, wearable_start, 900);
    assert!(
        wearable_body.contains(".filter(|item| !is_archived_truthy(item))"),
        "`wearable()` must drop archived items from each tier BEFORE shadowing"
    );
    assert!(
        wearable_body.contains("shadow(&refs)"),
        "`wearable()` must shadow the FILTERED tiers (archived dropped per tier, not after the merge)"
    );
    // The wearer: `by_id` is built from the UNFILTERED tiers.
    let build_start = src
        .find("pub fn build_wearable_pool(")
        .expect("build_wearable_pool exists");
    let build_body = &window(&src, build_start, 900);
    assert!(
        !build_body.contains("is_archived_truthy"),
        "`build_wearable_pool` must not filter archived items out of `by_id` — a garment \
         archived mid-chat stays worn, and its title must still resolve"
    );
    // `find_by_title`: own layer archived-INCLUDED, shared through `wearable()`.
    let find_start = src
        .find("pub fn find_by_title(&self, title: &str) -> Option<&Value> {")
        .expect("find_by_title exists");
    let find_body = &window(&src, find_start, 900);
    assert!(
        find_body.contains("let own = &self.layers_weakest_first[3];"),
        "`find_by_title` must search the OWN layer first, archived included"
    );
    assert!(
        find_body.contains("self.wearable()"),
        "`find_by_title` must fall back to the WEARABLE shared set (never an archived shared item)"
    );
}

/// The three pre-`9ecebf986` finders survive as LANE-REMOVE shims over the
/// pool; each must sit on the right side of the split until it is deleted.
#[test]
fn the_shims_sit_on_the_right_side_of_the_split() {
    let src = source("db/wardrobe_read.rs");
    let pool_fn = src
        .find("pub fn find_wearable_pool_for_character(")
        .expect("the pool shim exists");
    assert!(
        window(&src, pool_fn, 900).contains(".wearable()"),
        "the audition shim must answer the pool's `wearable()`"
    );
    let ids_fn = src
        .find("pub fn find_by_ids_for_character(")
        .expect("the ids shim exists");
    assert!(
        window(&src, ids_fn, 1600).contains(".by_id_ordered()"),
        "the wearer shim must read the ARCHIVED-INCLUDING `by_id` layer"
    );
    let id_fn = src
        .find("pub fn find_by_id_for_character(")
        .expect("the id shim exists");
    assert!(
        window(&src, id_fn, 900).contains(".get(id)"),
        "the single-item shim must read the ARCHIVED-INCLUDING `by_id` layer"
    );
}
