//! The `get_active_character_participants` caller census (P4.D172, item 7).
//!
//! `getActiveCharacterParticipants` is a MISNOMER — despite the name it returns
//! `controlledBy === 'llm'` seats only. v4 bug 131 was four selection sites
//! building their talkativeness map from it, so a seat the human drives was
//! invisible: its character's talkativeness fell through to the 0.5 default and
//! an archived character on it was never dropped.
//!
//! v4 `d14da3a56` moved all six sites onto `loadRoomCharacters` /
//! `getPresentCharacterSeats`, and P4.D172 did the same in v5. **But v4 KEPT the
//! alias**, because one caller survives at the tip:
//!
//! ```text
//! lib/background-jobs/handlers/autonomous-room-announce.ts:129
//!   const startParticipants = getActiveCharacterParticipants(chat.participants ?? []);
//! ```
//!
//! So v5's `enclave/announce.rs` keeps calling its port of it, and the alias
//! stays exported. That is a deliberate pair of decisions, and both directions
//! can rot quietly:
//!
//!   * a future lane "tidies up" the alias away, silently widening the
//!     autonomous-room announcement to seats the human drives; or
//!   * a NEW selection site reaches for it by name — reintroducing bug 131 in a
//!     file no differential covers.
//!
//! This census makes both loud. The allow-list IS the statement: every file that
//! may name the alias, with the reason it may.
//!
//! Sibling pins: `participant_filters::tests::the_alias_is_narrower_than_the_room`
//! (the alias and the room reader measurably disagree) and
//! `turn_pause_filters_equivalence`, which compares BOTH accessors against v4 by
//! name — if that family reddens, a lane widened the alias instead of its callers.
//!
//! Run standalone:
//!   cargo test -p quilltap-harness --test deprecated_alias_callers_guard

use std::path::{Path, PathBuf};

/// `(repo-relative path, expected occurrences, why this file may name it)`.
const CENSUS: &[(&str, usize, &str)] = &[
    (
        "crates/quilltap-core/src/participant_filters.rs",
        3,
        "the alias's own definition, the doc reference from \
         `get_present_character_seats` warning it is NOT the general case, and \
         `the_alias_is_narrower_than_the_room`'s assertion",
    ),
    (
        "crates/quilltap-core/src/enclave/announce.rs",
        2,
        "the import and the ONE production call — v5's port of v4's surviving \
         caller, `autonomous-room-announce.ts:129`. P4.D172 left this alone by \
         mandate: v4 still calls the alias here, so v5 must too",
    ),
    (
        "crates/quilltap-core/src/cycle_order.rs",
        1,
        "a doc comment naming it as the WRONG input for a room map (v4's own \
         `cycle-order.ts` docblock says the same)",
    ),
    (
        "crates/quilltap-harness/tests/room_characters_equivalence.rs",
        1,
        "a comment on the bug-131 assertion naming what the map must not narrow \
         back to",
    ),
    (
        "crates/quilltap-harness/tests/turn_pause_filters_equivalence.rs",
        2,
        "the import and the by-name comparison against v4's own accessor — the \
         family that reddens if a lane widens the alias itself",
    ),
];

const NEEDLE: &str = "get_active_character_participants";

/// This file names the needle throughout its own prose and census; it is the
/// guard, not a caller.
const SELF: &str = "crates/quilltap-harness/tests/deprecated_alias_callers_guard.rs";

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("harness crate sits two levels under the repo root")
        .to_path_buf()
}

fn rust_sources(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = std::fs::read_dir(dir).unwrap_or_else(|e| panic!("read {}: {e}", dir.display()));
    for entry in entries {
        let path = entry.expect("dir entry").path();
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        if path.is_dir() {
            if name == "target" || name == "vendor" {
                continue;
            }
            rust_sources(&path, out);
        } else if name.ends_with(".rs") {
            out.push(path);
        }
    }
}

#[test]
fn the_deprecated_alias_keeps_exactly_its_v4_callers() {
    let root = repo_root();
    let mut files = Vec::new();
    rust_sources(&root.join("crates"), &mut files);
    files.sort();
    assert!(
        files.len() > 100,
        "the walk found only {} rust files — it is not reaching the tree",
        files.len()
    );

    let mut failures: Vec<String> = Vec::new();
    let mut seen: Vec<(String, usize)> = Vec::new();

    for path in &files {
        let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
        let count = text.matches(NEEDLE).count();
        if count == 0 {
            continue;
        }
        let rel = path
            .strip_prefix(&root)
            .expect("under the repo root")
            .to_string_lossy()
            .replace('\\', "/");
        if rel == SELF {
            continue;
        }
        seen.push((rel.clone(), count));

        match CENSUS.iter().find(|(p, ..)| *p == rel) {
            None => failures.push(format!(
                "{rel}: {count} use(s) of `{NEEDLE}` outside the census. That \
                 helper returns LLM-controlled seats ONLY — a selection site \
                 that reaches for it is reintroducing v4 bug 131. The \
                 whole-room reader is `room_characters::load_room_characters` \
                 / `participant_filters::get_present_character_seats`."
            )),
            Some((_, expected, _)) if count != *expected => failures.push(format!(
                "{rel}: {count} use(s) of `{NEEDLE}`, census says {expected}. A \
                 new one even in a listed file wants justifying here — or \
                 routing through the whole-room reader."
            )),
            Some(_) => {}
        }
    }

    for (rel, expected, why) in CENSUS {
        if !seen.iter().any(|(p, _)| p == rel) {
            failures.push(format!(
                "{rel}: census expects {expected} use(s) of `{NEEDLE}`, found \
                 none. If v4 finally retired the alias, retire this row WITH \
                 the v4 evidence ({why})."
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "the deprecated LLM-only alias's caller set has moved (v4 bug 131):\n  {}",
        failures.join("\n  ")
    );
}
