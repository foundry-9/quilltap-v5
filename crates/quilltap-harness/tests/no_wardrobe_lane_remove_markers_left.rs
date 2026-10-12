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
//! Run standalone:
//!   cargo test -p quilltap-harness --test no_wardrobe_lane_remove_markers_left

use std::path::{Path, PathBuf};

const MARKER: &str = "LANE-REMOVE(";

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
    let found = markers();
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
/// census by misspelling its mark: every marker names at least one lane and
/// the deleting v4 commit.
#[test]
fn every_lane_remove_marker_names_a_lane_and_the_v4_commit() {
    for (file, line, text) in markers() {
        assert!(
            text.contains("LANE-REMOVE(P4.D28"),
            "{file}:{line}: a marker must name its owning lane(s) `P4.D28x`: {text}"
        );
        assert!(
            text.contains("9ecebf986"),
            "{file}:{line}: a marker must name the deleting v4 commit `9ecebf986`: {text}"
        );
    }
}
