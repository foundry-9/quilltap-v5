//! P4.D120 — the OPPOSITE-direction archived pin (v4 `d25dacc1`'s
//! `resolve-equipped` suite): **a garment archived mid-chat stays worn.**
//!
//! Archiving hides a garment from the AUDITION, not from the wearer. v5 splits
//! that across two reads in `db::wardrobe_read`:
//!
//!   * `find_wearable_pool_for_character` reads with `include_archived: false`
//!     — the candidate pool the outfit LLM is shown (pinned end-to-end by
//!     `outfit_llm_choose_tier3_equivalence`'s four never-auditions rows);
//!   * `find_by_ids_for_character` reads with `include_archived: true` — the
//!     already-equipped ids, whose TITLES would otherwise vanish from an outfit
//!     the moment someone archived a worn piece.
//!
//! No differential reaches the second one today: `handleGetOutfit` returns ids
//! only, and the title-resolving summary surface has no oracle case. So the
//! contract is pinned where it lives — in the source — the way this repo pins
//! other unreachable-but-load-bearing arms (`db_error_key_guard`,
//! `outfit_instructions_wiring_guard`). Flipping either flag fails this test.
//!
//! Run standalone:
//!   cargo test -p quilltap-harness --test archived_wearer_read_guard

use std::path::PathBuf;

fn source() -> String {
    let p =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../quilltap-core/src/db/wardrobe_read.rs");
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("read {}: {e}", p.display()))
}

#[test]
fn the_pool_excludes_archived_and_the_wearer_read_includes_them() {
    let src = source();

    // The audition: BOTH tier reads exclude.
    assert!(
        src.contains("let shared = find_archetypes(main, docs, false, tiers)?;"),
        "the candidate pool's shared-tier read must exclude archived garments"
    );
    assert!(
        src.contains("let own = find_by_character_id(main, docs, character_id, false)?;"),
        "the candidate pool's own-vault read must exclude archived garments"
    );

    // The wearer: the equipped-ids read INCLUDES them.
    assert!(
        src.contains("let items = find_by_character_id(main, docs, character_id, true)?;"),
        "`find_by_ids_for_character` must read WITH archived — a garment archived \
         mid-chat stays worn, and its title must still resolve"
    );

    // …and there is exactly ONE of each spelling, so a new read cannot quietly
    // join the wrong side.
    assert_eq!(
        src.matches("find_by_character_id(main, docs, character_id, false)")
            .count(),
        1,
        "a second archived-EXCLUDING per-character read appeared; classify it here"
    );
    // TWO archived-INCLUDING reads, both deliberate and both about a garment
    // someone already has: `find_by_ids_for_character` (the equipped set) and
    // `find_by_id_for_character` (a single item by id — the detail GET and the
    // update pre-check, which must find an archived item, not 404 on it).
    assert_eq!(
        src.matches("find_by_character_id(main, docs, character_id, true)")
            .count(),
        2,
        "the archived-INCLUDING per-character reads moved; classify the change here"
    );
}
