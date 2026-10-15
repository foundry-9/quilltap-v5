//! **P4.D291 — the ONE `lastAccessedAt` writer census** (R-AG, v4
//! `584d28164`).
//!
//! v4 Phase E made delivery the only thing that marks a memory accessed:
//! `stampAccessed` in `lib/memory/recall/deliver.ts`, called with exactly the
//! memories a consumer DELIVERED (the turn's surviving recall lines, Carina's
//! answer, a voice rehearsal's head, the greeting, the search tool's returned
//! rows). A search stamps nothing; a GET stamps nothing (bug 219). The old
//! writer — `markMemoriesAccessed` and its `[Memory] Marking memories
//! accessed` line — is gone.
//!
//! This census pins that in v5's sources:
//!
//! 1. every call of the repository's two access writers
//!    (`update_access_time` / `update_access_time_bulk`) under `crates/*/src`
//!    sits inside `services/recall/deliver.rs` (`stamp_accessed`), apart from
//!    the repository's own definitions;
//! 2. no source under `crates/*/src` names the retired writer or its line.
//!
//! RED on the P4.D291 lane branch by exactly ONE site — the memories item
//! GET's stamp in `api/memories.rs` (bug 219, P4.D298's file, R-AG) — and
//! GREEN on the union once P4.D298 removes it.
//!
//! Run standalone:
//!   cargo test -p quilltap-harness --test last_accessed_at_writer_census

use std::path::{Path, PathBuf};

/// The one module allowed to call the access writers.
const THE_WRITER: &str = "quilltap-core/src/services/recall/deliver.rs";
/// The repository that defines them.
const THE_REPOSITORY: &str = "quilltap-core/src/db/memories.rs";

const WRITER_CALLS: [&str; 2] = [".update_access_time(", ".update_access_time_bulk("];

/// The retired writer's name and line, split so this file does not name them
/// itself (it lives outside the scanned tree, but a copy-paste into `src`
/// must not carry a literal).
fn retired_names() -> [String; 2] {
    [
        ["mark_memories", "_accessed"].concat(),
        ["[Memory] Marking ", "memories accessed"].concat(),
    ]
}

fn crates_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            walk(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// Every `.rs` file under each crate's `src/`, as `(crate-relative path, text)`.
fn sources() -> Vec<(String, String)> {
    let root = crates_root();
    let mut files = Vec::new();
    for entry in std::fs::read_dir(&root).expect("read crates/") {
        let src = entry.expect("dir entry").path().join("src");
        if src.is_dir() {
            walk(&src, &mut files);
        }
    }
    files.sort();
    files
        .into_iter()
        .map(|f| {
            let rel = f
                .strip_prefix(&root)
                .unwrap_or(&f)
                .to_string_lossy()
                .replace('\\', "/");
            (rel, std::fs::read_to_string(&f).expect("read source"))
        })
        .collect()
}

/// The scan must SEE the writer it permits — an empty or mis-rooted walk would
/// pass both censuses vacuously.
#[test]
fn the_scan_reaches_the_one_writer() {
    let all = sources();
    assert!(all.len() > 100, "the walk found {} files", all.len());
    let writer = all
        .iter()
        .find(|(rel, _)| rel == THE_WRITER)
        .unwrap_or_else(|| panic!("{THE_WRITER} not found by the walk"));
    assert!(
        writer.1.contains(".update_access_time_bulk("),
        "{THE_WRITER} no longer calls the bulk writer — the census is stale"
    );
}

#[test]
fn only_delivery_writes_last_accessed_at() {
    let mut offenders = Vec::new();
    for (rel, text) in sources() {
        if rel == THE_WRITER || rel == THE_REPOSITORY {
            continue;
        }
        for (i, line) in text.lines().enumerate() {
            let code = line.trim_start();
            if code.starts_with("//") {
                continue;
            }
            if WRITER_CALLS.iter().any(|c| code.contains(c)) {
                offenders.push(format!("{rel}:{}: {}", i + 1, code));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "{} lastAccessedAt writer call(s) outside stamp_accessed (v4 584d28164 — delivery is \
         the ONE writer; the memories GET stamp is P4.D298's, R-AG):\n  {}",
        offenders.len(),
        offenders.join("\n  ")
    );
}

#[test]
fn no_source_names_the_retired_writer() {
    let names = retired_names();
    let mut offenders = Vec::new();
    for (rel, text) in sources() {
        for (i, line) in text.lines().enumerate() {
            for name in &names {
                if line.contains(name.as_str()) {
                    offenders.push(format!("{rel}:{}: {name}", i + 1));
                }
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "the retired access writer is still named:\n  {}",
        offenders.join("\n  ")
    );
}
