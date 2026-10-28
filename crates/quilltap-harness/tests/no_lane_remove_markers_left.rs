//! **The round-agnostic LANE-REMOVE census** (the `322a5488e` follow-ups
//! unification, closing P4.174's D340-shas finding).
//!
//! A `LANE-REMOVE(<owner>): v4 deleted <twin> in <sha>` marker flags a thin
//! wrapper a lane had to keep because another lane's file still called it;
//! every round's unifier deletes the last of them (§S.5). The two scoped
//! censuses (`no_memory_lane_remove_markers_left`,
//! `no_wardrobe_lane_remove_markers_left`) count only markers naming their own
//! round's shas, and their exact `LANE-REMOVE(` match misses a spaced
//! `LANE-REMOVE (`. This census counts EVERY marker, in any comment form and
//! any spacing, in every shipped crate's `src/` — so a marker naming a later
//! round's sha, a v5 sha, or no sha at all cannot outlive its unification.
//! RED BY DESIGN on a lane branch that still carries one.
//!
//! Run standalone:
//!   cargo test -p quilltap-harness --test no_lane_remove_markers_left

use std::path::{Path, PathBuf};

/// `LANE-REMOVE`, optional whitespace, `(`.
fn is_marker_line(line: &str) -> bool {
    let mut rest = line;
    while let Some(i) = rest.find("LANE-REMOVE") {
        let after = rest[i + "LANE-REMOVE".len()..].trim_start();
        if after.starts_with('(') {
            return true;
        }
        rest = &rest[i + 1..];
    }
    false
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            walk(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

#[test]
fn no_lane_remove_markers_left_anywhere() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut files = Vec::new();
    for e in std::fs::read_dir(&crates).expect("crates dir").flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        // The harness's own censuses quote the marker; it ships nothing.
        if name == "quilltap-harness" {
            continue;
        }
        walk(&e.path().join("src"), &mut files);
    }
    files.sort();
    assert!(
        files.len() > 500,
        "the census must see the workspace's sources (saw {})",
        files.len()
    );
    let mut found = Vec::new();
    for file in files {
        let text = std::fs::read_to_string(&file).expect("read source");
        for (i, line) in text.lines().enumerate() {
            if is_marker_line(line) {
                found.push(format!(
                    "{}:{}: {}",
                    file.strip_prefix(&crates).unwrap_or(&file).display(),
                    i + 1,
                    line.trim()
                ));
            }
        }
    }
    assert!(
        found.is_empty(),
        "{} LANE-REMOVE marker(s) outlived their unification — delete each \
         wrapper (its last caller's lane owns the finding):\n{}",
        found.len(),
        found.join("\n")
    );
}

/// The matcher sees every spelling a marker can take, and nothing else.
#[test]
fn the_matcher_sees_spaced_and_doc_comment_markers() {
    for hit in [
        "// LANE-REMOVE(P4.D999): v4 deleted foo in 6c7ea4410",
        "// LANE-REMOVE (P4.D999): v4 deleted foo",
        "//! LANE-REMOVE\t(P4.D999)",
        "/// LANE-REMOVE(",
        "    /* LANE-REMOVE  ( */",
    ] {
        assert!(is_marker_line(hit), "{hit}");
    }
    for miss in [
        "// LANE-REMOVE marker census",
        "// LANE-REMOVED(",
        "LANE_REMOVE(",
    ] {
        assert!(!is_marker_line(miss), "{miss}");
    }
}
