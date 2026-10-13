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
//! The WEARER's read is `services::resolve_equipped` (v4 `resolve-equipped.ts`,
//! the ONE port since the `4a38222e6` unification): it resolves the equipped
//! ids through `component_graph_lookup` → `component_graph`, a BFS over
//! `pool.get` — the archived-including layer — never `wearable()`. (The three
//! pre-`9ecebf986` `db/wardrobe_read.rs` finders this file once also pinned
//! were LANE-REMOVE shims; the unification deleted them, `24e110237`.)
//!
//! The contract is pinned where it lives — in the source — the way this repo
//! pins other load-bearing arms (`db_error_key_guard`,
//! `outfit_instructions_wiring_guard`). Flipping either side fails this test:
//! the audition must filter per tier, the by-id layer must not filter at all,
//! and the wearer must read the by-id layer.
//!
//! Run standalone:
//!   cargo test -p quilltap-harness --test archived_wearer_read_guard

use std::path::PathBuf;

/// `src[start..start+len]`, clamped to the file and pulled back to a char
/// boundary — `start` comes from `find` (always a boundary), but a fixed width
/// can land inside a multi-byte character (the sources carry `—`, `→`), which
/// would panic the slice.
fn window(src: &str, start: usize, len: usize) -> &str {
    let mut end = src.len().min(start + len);
    while !src.is_char_boundary(end) {
        end -= 1;
    }
    &src[start..end]
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

/// The wearer reads the archived-INCLUDING layer: `resolve_equipped` looks the
/// equipped ids up through `component_graph_lookup`, whose BFS answers
/// `pool.get` — so a garment archived mid-chat keeps its title in the outfit.
#[test]
fn the_wearer_resolves_equipped_ids_through_the_by_id_layer() {
    let resolver = source("services/resolve_equipped.rs");
    let resolve_start = resolver
        .find("pub fn resolve_equipped_outfit_for_character(")
        .expect("the one resolve-equipped port exists");
    let resolve_body = window(&resolver, resolve_start, 1200);
    assert!(
        resolve_body.contains("component_graph_lookup(pool, &equipped_item_ids)"),
        "the wearer must resolve the equipped ids through the pool's component graph"
    );
    assert!(
        !resolve_body.contains(".wearable()"),
        "the wearer must never read the AUDITION set — an archived worn garment would vanish"
    );

    let pool = source("wearable_pool.rs");
    let graph_start = pool
        .find("pub fn component_graph(pool: &WearablePool, root_ids: &[String]) -> Vec<Value> {")
        .expect("component_graph exists");
    let graph_body = window(&pool, graph_start, 900);
    assert!(
        graph_body.contains("pool.get(&id)"),
        "`component_graph` must walk the ARCHIVED-INCLUDING `by_id` layer"
    );
    assert!(
        !graph_body.contains("wearable()") && !graph_body.contains("is_archived_truthy"),
        "`component_graph` must not filter archived items"
    );
}

#[test]
fn window_clamps_to_a_char_boundary() {
    // `—` is three bytes; a width that ends inside it backs off to before it.
    let src = "ab—cd";
    assert_eq!(window(src, 0, 3), "ab");
    assert_eq!(window(src, 0, 5), "ab—");
    assert_eq!(window(src, 2, 100), "—cd");
}
