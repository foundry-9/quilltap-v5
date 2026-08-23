//! The hard-coded `durationMs` regrowth guard (dogfood finding #100).
//!
//! `llm_logs.durationMs` is v4's `Date.now() - startTime` around the provider
//! call the row describes. Three v5 log sites emitted a hard-coded `0.0`
//! instead: the streaming `CHAT_MESSAGE` write (`primary_stream.rs`, v4
//! `streaming.service.ts:382/440`) and both `IMAGE_DESCRIPTION` writes
//! (`file_fallback.rs`, v4 `file-attachment-fallback.ts:205/351/460`).
//!
//! The streaming one mattered most: `CHAT_MESSAGE` is the commonest row there
//! is — 911 of the 6,115 rows on the real instance the 2026-08-22 dogfood pass
//! walked — and it is the row the LLM Inspector's duration column reads. The
//! divergence was measured, not inferred: across every one of those v4-written
//! rows, in all sixteen call types, **not one** carried a zero duration; v5's
//! first two streamed turns on the same instance both did.
//!
//! Each site had carried a comment calling the zero a tracked deferral — a real
//! stream clock could not be diffed, so both sides were pinned to 0. That
//! blocker had since been lifted and the comments never caught up:
//! `common::normalize_duration_ms` collapses any non-NULL duration to `"<ms>"`
//! on BOTH sides and keeps NULL NULL, so a measured elapsed and the oracle's
//! frozen-clock 0 compare equal — while a side that stops writing a duration at
//! all still fails.
//!
//! A hard-coded zero is indistinguishable from a measured one in any dump that
//! normalizes the column, so no differential can hold this line. This guard
//! reads the source instead: no `duration_ms: Some(0.0)` (or `Some(0)`) may
//! exist anywhere under `crates/`. A site that genuinely has no clock to read
//! writes `None` — which the normalizer preserves, and which v4's own NULL rows
//! are compared against.
//!
//! Run standalone:
//!   cargo test -p quilltap-harness --test llm_log_duration_guard

use std::path::{Path, PathBuf};

/// The literals that spell "I am not measuring anything, but say I measured 0".
const NEEDLES: &[&str] = &["duration_ms: Some(0.0)", "duration_ms: Some(0)"];

/// This file names the needles in its prose; it is the guard, not a call site.
const SELF: &str = "crates/quilltap-harness/tests/llm_log_duration_guard.rs";

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
fn no_llm_log_site_hard_codes_a_zero_duration() {
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
    for path in &files {
        let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
        let rel = path
            .strip_prefix(&root)
            .expect("under the repo root")
            .to_string_lossy()
            .replace('\\', "/");
        if rel == SELF {
            continue;
        }
        for needle in NEEDLES {
            let count = text.matches(needle).count();
            if count > 0 {
                failures.push(format!(
                    "{rel}: {count} `{needle}` site(s). An `llm_logs` row must carry \
                     the measured elapsed of the provider call it describes (v4's \
                     `Date.now() - startTime`), or `None` where v4 writes no \
                     duration — never a hard-coded zero (dogfood finding #100)."
                ));
            }
        }
    }

    assert!(
        failures.is_empty(),
        "hard-coded zero durations found:\n  {}",
        failures.join("\n  ")
    );
}
