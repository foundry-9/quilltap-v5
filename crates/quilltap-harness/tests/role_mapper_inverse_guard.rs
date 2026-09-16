//! The hand-rolled-role-mapper regrowth guard (P4.93).
//!
//! Every tier-3 family has to turn an oracle-recorded `role` string back into
//! a [`CompletionRole`], and until P4.93 there was no inverse to call: the
//! enum carried `as_str()` and nothing else, so twenty-three families each
//! wrote the `match` by hand. Nine of them had no `"tool"` arm and swept it
//! into a catch-all `_ => CompletionRole::User`.
//!
//! That is not cosmetic. The canned tier-3 key RENDERS the role
//! (`canned_completion_key` / `canned_stream_key` via `CannedKeyMessage::
//! key_role`), so a `tool` row filed as `user` computes a key the oracle's own
//! registration can never match — and the family then fails as an
//! "unregistered input" whose messages diff byte-for-byte against the oracle,
//! which is about as misleading as a red gets. P4.90 spent a lane finding
//! exactly that on `orchestrator_tier3`'s failover-then-tool-call arm.
//!
//! So the fix is only durable if the hand-rolls cannot come back: this test
//! walks `crates/quilltap-harness/tests/**.rs` and fails on any file that
//! matches the hand-rolled shape. Every mapper goes through
//! `CompletionRole::from_v4_wire`, which is exhaustive by construction and
//! unit-pinned in `model/completion.rs`; the call site keeps its OWN default
//! (`unwrap_or(User)` where v4's path is single-shot, `unwrap_or_else(||
//! panic!(…))` where an unknown role must abort), which is the point — a
//! default you had to write is a decision, a catch-all is an accident.
//!
//! Run standalone:
//!   cargo test -p quilltap-harness --test role_mapper_inverse_guard

use std::path::{Path, PathBuf};

/// The hand-rolled shape. Any arm mapping a wire spelling straight onto a
/// variant is one; `"assistant"` is the one spelling every such mapper has
/// (a mapper without it could not answer an assistant row at all), so it is
/// the cheapest total needle.
const NEEDLE: &str = "\"assistant\" => CompletionRole::Assistant";

/// Files allowed to carry the needle, each with the reason.
///
/// `orchestrator_tier3_equivalence.rs` is P4.92's file this round (§R.10 (b)):
/// its mapper already carries the `"tool"` arm P4.90 added, so it is correct
/// but still hand-rolled. **The unifier repoints it onto the inverse at the
/// wire and DELETES this exemption** — one line, recorded in the round's §R.10.
const EXEMPT: &[(&str, &str)] = &[(
    "crates/quilltap-harness/tests/orchestrator_tier3_equivalence.rs",
    "P4.92 owns this file this round; the unifier repoints it at the wire and \
     removes this row (round §R.10 (b))",
)];

/// This file names the needle in its own prose and census.
const SELF: &str = "crates/quilltap-harness/tests/role_mapper_inverse_guard.rs";

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
            rust_sources(&path, out);
        } else if name.ends_with(".rs") {
            out.push(path);
        }
    }
}

#[test]
fn every_harness_role_mapper_goes_through_the_inverse() {
    let root = repo_root();
    let tests_dir = root.join("crates/quilltap-harness/tests");
    let mut files = Vec::new();
    rust_sources(&tests_dir, &mut files);
    files.sort();
    assert!(
        files.len() > 100,
        "the walk found only {} test files — it is not reaching the tree",
        files.len()
    );

    // Anti-vacuity: the inverse must actually be in use, or a tree that simply
    // stopped mapping roles would pass this guard while proving nothing.
    let users = files
        .iter()
        .filter(|p| {
            std::fs::read_to_string(p)
                .map(|t| t.contains("CompletionRole::from_v4_wire"))
                .unwrap_or(false)
        })
        .count();
    assert!(
        users >= 25,
        "only {users} families call `CompletionRole::from_v4_wire` — the sweep \
         has been undone, not the guard satisfied"
    );

    let mut failures: Vec<String> = Vec::new();
    let mut seen_exempt: Vec<String> = Vec::new();

    for path in &files {
        let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
        if !text.contains(NEEDLE) {
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
        match EXEMPT.iter().find(|(p, _)| *p == rel) {
            Some(_) => seen_exempt.push(rel),
            None => failures.push(format!(
                "{rel}: a hand-rolled role match. Call \
                 `CompletionRole::from_v4_wire(<role>)` and write the default \
                 you want — a catch-all silently files a `tool` row as `user`, \
                 and the canned key renders the role (P4.90, P4.93)."
            )),
        }
    }

    for (rel, why) in EXEMPT {
        if !seen_exempt.iter().any(|p| p == rel) {
            failures.push(format!(
                "{rel}: exempted but no longer hand-rolls a role match — DELETE \
                 the exemption ({why})."
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "hand-rolled role mappers are regrowing:\n  {}",
        failures.join("\n  ")
    );
}
