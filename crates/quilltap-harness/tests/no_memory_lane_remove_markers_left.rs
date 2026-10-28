//! **P4.D289 — the LANE-REMOVE census of the `83334b9a5` memory-refactor +
//! scenarios round** (work order P4.D289 LR-2, §R.10(o)).
//!
//! v4 DELETED a family of memory modules in the span — `memory-weighting.ts`
//! (`e0ac7493a`), `day-references.ts` (`e0ac7493a`), `reinforced-importance.ts`
//! (`e0ac7493a`), `memory-gate.ts` (`8af764514`), the write half of
//! `episodic.ts` (`e0ac7493a`), and more in the stateful lanes. Their v5 twins
//! are called from files owned by OTHER lanes of the round (recall, admission,
//! embeddings, extraction, maintenance), so the lane that ports the successor
//! cannot delete the twin. Each is kept as a thin wrapper, marked with ONE
//! comment line
//!
//! ```text
//! // LANE-REMOVE(<owner order>[|<owner order>]): v4 deleted <twin> in <sha>
//! ```
//!
//! where `<sha>` is one of this round's twenty v4 commits, and
//! [`no_memory_lane_remove_markers_left`] fails while any such marker remains
//! in `quilltap-core`'s sources. It is RED BY DESIGN on every lane branch; the
//! consumer lanes empty the wrappers they were the last caller of, and the
//! unifier deletes the rest in ONE commit (§S.5) — a wrapper that still has a
//! caller then is a finding against the caller's lane, never a reason to drop
//! the marker.
//!
//! Scoping (LR-2): the wardrobe round's census
//! (`no_wardrobe_lane_remove_markers_left`) counts only markers naming
//! `7a8d6dacc` / `9ecebf986` / `4a38222e6`; this one counts only markers
//! naming one of the twenty shas below. A marker naming NEITHER set fails
//! BOTH censuses' format tests, so it cannot hide.
//!
//! Run standalone:
//!   cargo test -p quilltap-harness --test no_memory_lane_remove_markers_left

use std::path::{Path, PathBuf};

const MARKER: &str = "LANE-REMOVE(";

/// The `83334b9a5` round's twenty v4 commits (the work orders' §R block).
const ROUND_SHAS: [&str; 20] = [
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

/// The wardrobe round's shas (its census's scope).
const WARDROBE_SHAS: [&str; 3] = ["7a8d6dacc", "9ecebf986", "4a38222e6"];

/// A marker belonging to NEITHER round this census (and its twin) scopes must
/// still name SOME v4 commit — a 9..=40-digit lowercase-hex word — so a later
/// round's marker (the `322a5488e` round's, `7263105c6`) cannot hide behind
/// the format test without naming the commit it absorbs. One rule, no
/// per-round constant to keep in step (P4.174 / D340-shas).
fn names_some_v4_commit(text: &str) -> bool {
    text.split(|c: char| !c.is_ascii_alphanumeric()).any(|w| {
        (9..=40).contains(&w.len())
            && w.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            && w.bytes().any(|b| b.is_ascii_digit())
    })
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
fn all_markers() -> Vec<(String, usize, String)> {
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

fn names_round_sha(text: &str) -> bool {
    ROUND_SHAS.iter().any(|sha| text.contains(sha))
}

/// RED BY DESIGN until the unifier's §S.5 commit.
#[test]
fn no_memory_lane_remove_markers_left() {
    let found: Vec<_> = all_markers()
        .into_iter()
        .filter(|(_, _, text)| names_round_sha(text))
        .collect();
    if found.is_empty() {
        return;
    }
    let mut report = String::new();
    for (file, line, text) in &found {
        report.push_str(&format!("  {file}:{line}  {text}\n"));
    }
    panic!(
        "{} LANE-REMOVE wrapper(s) of the `83334b9a5` round remain in quilltap-core (the \
         consumer lanes delete the ones they were the last caller of; the unifier deletes \
         the rest in ONE commit — §S.5):\n{report}",
        found.len()
    );
}

/// The marker's FORMAT is itself pinned, so a wrapper cannot hide from the
/// census by misspelling its mark: `// LANE-REMOVE(<P4.D2xx>[|<P4.D2xx>…]):
/// v4 deleted <twin> in <sha>`, the sha one of this round's twenty. A marker
/// that names NO round sha must name a wardrobe sha (the other census's
/// scope); naming neither fails here.
#[test]
fn every_memory_lane_remove_marker_names_its_owners_and_a_round_commit() {
    for (file, line, text) in all_markers() {
        if !names_round_sha(&text) {
            assert!(
                WARDROBE_SHAS.iter().any(|sha| text.contains(sha)) || names_some_v4_commit(&text),
                "{file}:{line}: a LANE-REMOVE marker names neither this round's commits nor \
                 the wardrobe round's — it would hide from both censuses: {text}"
            );
            continue;
        }
        let rest = text
            .strip_prefix("// LANE-REMOVE(")
            .unwrap_or_else(|| panic!("{file}:{line}: the marker must open the comment: {text}"));
        let (owners, tail) = rest.split_once("): v4 deleted ").unwrap_or_else(|| {
            panic!("{file}:{line}: `(<owners>): v4 deleted <twin> in <sha>`: {text}")
        });
        for owner in owners.split('|') {
            let digits = owner.strip_prefix("P4.D");
            assert!(
                digits.is_some_and(|d| d.len() == 3 && d.bytes().all(|b| b.is_ascii_digit())),
                "{file}:{line}: owner `{owner}` is not a `P4.Dxxx` order: {text}"
            );
        }
        let (twin, sha) = tail
            .rsplit_once(" in ")
            .unwrap_or_else(|| panic!("{file}:{line}: the marker must end `in <sha>`: {text}"));
        assert!(
            !twin.trim().is_empty(),
            "{file}:{line}: the marker names no twin: {text}"
        );
        assert!(
            ROUND_SHAS.contains(&sha),
            "{file}:{line}: `{sha}` is not one of the round's twenty v4 commits: {text}"
        );
    }
}

/// The scoring chokepoint (v4 `__tests__/unit/lib/memory/scoring/
/// scoring-chokepoint.test.ts`, S12 — work order P4.D289 Tier 2 item 13).
///
/// v4's why, carried: every worth-ordering in the memory system goes through
/// `salience` or `protection` (memory refactor §3.4, §5 invariant 5). The
/// floored `effectiveWeight` that once leaked into ranking at four call
/// sites is gone; this keeps it gone — no source outside the scoring module
/// names it, nor the deleted module that computed it. Scans every crate's
/// `src/` (the Rust spellings) and `apps/web/src` (v4's TypeScript
/// spellings); `crates/quilltap-core/src/memory_scoring/` is the allowed
/// home, and the harness crate — v5's test tree, as v4 skips `__tests__` —
/// is out of scope (its corpus still names the re-aimed
/// `memory-weighting.ts` oracle case). RED BY DESIGN with the marker census — `memory_weighting.rs` (the
/// shim) and its callers carry the names until P4.D291 / P4.D292 / P4.D297
/// re-point and the unifier deletes the shim (§S.4 / §S.5).
#[test]
fn no_source_outside_the_scoring_module_names_the_floored_weight() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let allowed = Path::new("crates/quilltap-core/src/memory_scoring");
    let forbidden: Vec<regex::Regex> = [
        r"\beffective_weight\b",
        r"\bcalculate_effective_weight\b",
        r"\brank_memories_by_weight\b",
        r"\bmemory_weighting\b",
        r"\beffectiveWeight\b",
        r"\bcalculateEffectiveWeight\b",
        r"\brankMemoriesByWeight\b",
        r"memory-weighting",
    ]
    .iter()
    .map(|p| regex::Regex::new(p).unwrap())
    .collect();

    let mut files = Vec::new();
    for entry in std::fs::read_dir(root.join("crates")).expect("read crates/") {
        let krate = entry.expect("crate dir").path();
        if krate.file_name().is_some_and(|n| n == "quilltap-harness") {
            continue;
        }
        let src = krate.join("src");
        if src.is_dir() {
            walk_ext(&src, &["rs"], &mut files);
        }
    }
    walk_ext(
        &root.join("apps/web/src"),
        &["ts", "tsx", "js", "mjs"],
        &mut files,
    );
    assert!(
        files.len() > 100,
        "the scan found only {} files",
        files.len()
    );

    let mut offenders = Vec::new();
    for file in files {
        let rel = file.strip_prefix(&root).unwrap_or(&file).to_path_buf();
        if rel.starts_with(allowed) {
            continue;
        }
        let text = std::fs::read_to_string(&file).expect("read source");
        for pattern in &forbidden {
            if pattern.is_match(&text) {
                offenders.push(format!("  {}: {}", rel.display(), pattern.as_str()));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "{} scoring-chokepoint offender(s) — the floored weight or its deleted module is still \
         named outside crates/quilltap-core/src/memory_scoring/:\n{}",
        offenders.len(),
        offenders.join("\n")
    );
}

/// Every file under `dir` with one of `exts`, skipping `node_modules` and
/// dot-directories.
fn walk_ext(dir: &Path, exts: &[&str], out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name == "node_modules" || name.starts_with('.') {
            continue;
        }
        if path.is_dir() {
            walk_ext(&path, exts, out);
        } else if path
            .extension()
            .is_some_and(|e| exts.iter().any(|x| e == *x))
        {
            out.push(path);
        }
    }
}
