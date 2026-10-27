//! **P4.D279 — the LANE-REMOVE census (the `4a38222e6` round's tripwire).**
//!
//! v4 `9ecebf986` DELETED a family of wardrobe functions (`findByIdForCharacter`,
//! `findByIdsForCharacter`, `findWearablePoolForCharacter`, `findArchetypes*`,
//! `shared-tiers.ts`, `resolve-container.ts`, `dissolve-bundles.ts`,
//! `hydrate-components.ts`, the four outfit-displacement primitives, the
//! project / group wardrobe façades) in favour of ONE location and ONE
//! wearable pool. Their v5 twins are called from files owned by FOUR other
//! lanes of the round (P4.D280 tools, P4.D281 routes, P4.D282 outfit, P4.D283
//! images, P4.D284 carriers), so the keystone could not delete them without
//! editing those lanes' files. Each is kept as a thin, COMPLETE wrapper over
//! the new API, marked with a comment line
//!
//! ```text
//! // LANE-REMOVE(<lane>[|<lane>]): v4 deleted <twin> in 9ecebf986
//! ```
//!
//! and THIS test fails while any marker remains in `quilltap-core`'s sources.
//! It is RED BY DESIGN on the keystone and on every lane branch; the unifier
//! deletes the wrappers in ONE commit after P4.D280–P4.D283 are picked (§S.5)
//! and the test goes green. A wrapper that still has a caller then is a
//! finding against the caller's lane — never a reason to drop the marker.
//!
//! **Scoped by sha (P4.D289 LR-2, the `83334b9a5` round).** The next round's
//! memory lanes write markers of their own, counted by
//! `no_memory_lane_remove_markers_left`. This census counts only markers
//! naming the wardrobe round's commits (`7a8d6dacc` / `9ecebf986` /
//! `4a38222e6`); a marker naming neither set fails both censuses' format
//! tests, so it cannot hide.
//!
//! Run standalone:
//!   cargo test -p quilltap-harness --test no_wardrobe_lane_remove_markers_left

use std::path::{Path, PathBuf};

const MARKER: &str = "LANE-REMOVE(";

/// The wardrobe round's v4 commits — this census's scope.
const WARDROBE_SHAS: [&str; 3] = ["7a8d6dacc", "9ecebf986", "4a38222e6"];

/// The `83334b9a5` memory round's twenty commits — the OTHER census's scope.
const MEMORY_ROUND_SHAS: [&str; 20] = [
    "545e299bb",
    "124c8aca3",
    "69d38dd60",
    "8050958be",
    "97f0d8cb8",
    "4ce64fa1e",
    "c2795733f",
    "e0ac7493a",
    "af48f6cc2",
    "8af764514",
    "ac6348c5f",
    "93c5af6ad",
    "584d28164",
    "4ae0c4ca6",
    "104e986c7",
    "b045dd986",
    "48790fb73",
    "9ec4fa2e9",
    "64743ee13",
    "83334b9a5",
];

// === P4.D340: the `322a5488e` round's markers name the v4 commit their lane
// absorbs (P4.D340's: `7263105c6`, the title lint). No census of that round's
// markers exists yet; the unifier folds this list into one at §S.5. ===
const INVENTORY_ROUND_SHAS: [&str; 1] = ["7263105c6"];
// === end P4.D340 ===

fn names_wardrobe_sha(text: &str) -> bool {
    WARDROBE_SHAS.iter().any(|sha| text.contains(sha))
}

fn core_src() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../quilltap-core/src")
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("read core src") {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            walk(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// Every `LANE-REMOVE(` marker line in `quilltap-core/src`, as
/// `(relative path, line number, marker text)`.
fn markers() -> Vec<(String, usize, String)> {
    let root = core_src();
    let mut files = Vec::new();
    walk(&root, &mut files);
    files.sort();
    let mut found = Vec::new();
    for file in files {
        let text = std::fs::read_to_string(&file).expect("read source");
        for (i, line) in text.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("// ") && trimmed.contains(MARKER) {
                let rel = file
                    .strip_prefix(&root)
                    .unwrap_or(&file)
                    .to_string_lossy()
                    .to_string();
                found.push((rel, i + 1, trimmed.to_string()));
            }
        }
    }
    found
}

/// RED BY DESIGN until the unifier's §S.5 commit.
#[test]
fn no_wardrobe_lane_remove_markers_left() {
    let found: Vec<_> = markers()
        .into_iter()
        .filter(|(_, _, text)| names_wardrobe_sha(text))
        .collect();
    if found.is_empty() {
        return;
    }
    let mut report = String::new();
    for (file, line, text) in &found {
        report.push_str(&format!("  {file}:{line}  {text}\n"));
    }
    panic!(
        "{} LANE-REMOVE wrapper(s) remain in quilltap-core (the `4a38222e6` round's \
         stage-2 lanes delete the ones they were the last caller of; the unifier deletes \
         the rest in ONE commit — §S.5):\n{report}",
        found.len()
    );
}

/// The marker's FORMAT is itself pinned, so a wrapper cannot hide from the
/// census by misspelling its mark: every wardrobe marker names at least one
/// lane and a wardrobe-round commit. A marker naming no wardrobe commit must
/// name one of the memory round's (the other census's scope); naming neither
/// fails here.
#[test]
fn every_lane_remove_marker_names_a_lane_and_the_v4_commit() {
    for (file, line, text) in markers() {
        if !names_wardrobe_sha(&text) {
            assert!(
                MEMORY_ROUND_SHAS.iter().any(|sha| text.contains(sha))
                    || INVENTORY_ROUND_SHAS.iter().any(|sha| text.contains(sha)),
                "{file}:{line}: a LANE-REMOVE marker names neither the wardrobe round's commits \
                 nor the memory round's — it would hide from both censuses: {text}"
            );
            continue;
        }
        assert!(
            text.contains("LANE-REMOVE(P4.D28"),
            "{file}:{line}: a marker must name its owning lane(s) `P4.D28x`: {text}"
        );
    }
}
