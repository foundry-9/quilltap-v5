//! Source guards (P4.D343, v4 `b15fe9404` at the pin `236c04e35`).
//!
//! Unit E (declared tool effects): the curated destructive list, its INFO and
//! v4's deleted `isComputedTool` (`8becfc6ce`) have no v5 twin left anywhere in
//! `crates/` or `harness/` — the effect ceiling replaced them. Every literal
//! below is assembled from pieces so this file does not match itself.
//!
//! Run (no env var — the guards read the checkout):
//!   cargo test -p quilltap-harness --test p4d343_source_guards

use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Every text source under `dir` (Rust, TypeScript, JSON, shell), recursively,
/// skipping build output and dependency trees.
fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if p.is_dir() {
            if matches!(name, "target" | "node_modules" | "dist" | ".git" | "vendor") {
                continue;
            }
            walk(&p, out);
        } else if matches!(
            p.extension().and_then(|x| x.to_str()),
            Some("rs" | "ts" | "mjs" | "js" | "json" | "sh")
        ) {
            out.push(p);
        }
    }
}

fn hits(needle: &str) -> Vec<String> {
    let root = repo_root();
    let mut files = Vec::new();
    walk(&root.join("crates"), &mut files);
    walk(&root.join("harness"), &mut files);
    let mut found = Vec::new();
    for f in files {
        let Ok(text) = std::fs::read_to_string(&f) else {
            continue;
        };
        for (i, line) in text.lines().enumerate() {
            if line.contains(needle) {
                found.push(format!("{}:{}", f.display(), i + 1));
            }
        }
    }
    found
}

#[test]
fn unit_e_retired_names_are_gone() {
    for needle in [
        ["DESTRUCTIVE", "_TOOL_NAMES"].concat(),
        ["destructive tools", " filtered"].concat(),
        ["is_computed", "_tool"].concat(),
        ["fn destructive", "_allowed"].concat(),
        ["filter_destructive", "_tools"].concat(),
        ["apply_autonomous", "_destructive_filter"].concat(),
    ] {
        let h = hits(&needle);
        assert!(h.is_empty(), "`{needle}` survives: {h:?}");
    }
}

/// The `b15fe9404` log lines that do NOT survive to the pin (v4 `a8214b253`
/// rewrote them) — none may be ported.
#[test]
fn dead_b15fe9404_lines_are_not_ported() {
    for needle in [
        [
            "Storyteller chat — recording the operator post",
            "; a round will open",
        ]
        .concat(),
        [
            "Tools above the turn's effect ceiling",
            " filtered from per-turn list",
        ]
        .concat(),
        ["[Storyteller] Round housekeeping", " failed"].concat(),
    ] {
        let h = hits(&needle);
        assert!(h.is_empty(), "`{needle}` ported: {h:?}");
    }
}
