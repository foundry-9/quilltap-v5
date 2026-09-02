//! The bare-`CheapLlmTaskExecutor` regrowth guard (P4.68 — the P4.D135 carried
//! follow-up).
//!
//! `CheapLlmTaskExecutor::with_logging` populates BOTH the `llm_logs` writer and
//! the `CheapFallbackHandle` (the `Db` + user id a failed task's fallback-chain
//! walk needs, v4 `65f5021c8`). `::new()` / `::default()` populate neither: an
//! executor built that way logs nothing and, more importantly, **cannot fail
//! over** — a cheap-LLM task that dies on its primary profile just dies, where
//! v4 reaches its ambient `getRepositories()` and walks the chain.
//!
//! P4.D135's record named that as a live gap, listing `tools::generate_image`'s
//! prompt expansion and one `enclave::step` leg as bare PRODUCTION sites.
//! **P4.68 re-measured it against the tree and the gap is closed**: both of
//! those sites now sit inside `#[cfg(test)]` modules, and every one of the
//! host's sixteen construction sites in `quilltap-host/src/spine.rs` calls
//! `with_logging`. There is no production executor without a chain.
//!
//! That is a fact about today's tree, not a property of the type — nothing stops
//! the next lane from writing `CheapLlmTaskExecutor::new()` into a production
//! path, and the resulting executor would be silently chain-less (no compile
//! error, no test failure, no log line — the symptom is a cheap-LLM task that
//! stops failing over, which nothing in the suite asserts). So the measurement
//! is kept executable here, in the `db_error_key_guard` idiom.
//!
//! **The census below is the whole allow-list, and it is EMPTY.** A bare
//! construction anywhere in a production `src/` tree fails this test. If a new
//! one is genuinely right — a call path with no `Db` to give it — add it here
//! with its one-line justification; otherwise it wants `with_logging`.
//!
//! Test code is exempt by design and by construction: `#[cfg(test)]` modules
//! (detected per file) and the whole of `crates/quilltap-harness/tests/**` build
//! executors bare on purpose, because a differential injects its own canned
//! provider and has no chain to walk.
//!
//! Run standalone:
//!   cargo test -p quilltap-harness --test bare_cheap_llm_executor_guard

use std::path::{Path, PathBuf};

/// `(repo-relative path, expected PRODUCTION bare-construction count, why)`.
///
/// Empty on purpose — see the module docs. This is the census, not a sample.
const CENSUS: &[(&str, usize, &str)] = &[];

/// Both bare constructors. `new()` delegates to `default()`, and the type is
/// `#[derive(Default)]`, so either spelling yields the same chain-less executor.
const NEEDLES: &[&str] = &[
    "CheapLlmTaskExecutor::new(",
    "CheapLlmTaskExecutor::default(",
];

/// The production trees. `crates/quilltap-harness/tests/**` is deliberately
/// absent: an integration test file is test code end to end, with no
/// `#[cfg(test)]` marker to find.
const PRODUCTION_TREES: &[&str] = &[
    "crates/quilltap-core/src",
    "crates/quilltap-host/src",
    "crates/quilltap-web/src",
    "crates/quilltap-cli/src",
    "crates/quilltap-tauri/src",
];

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("harness crate sits two levels under the repo root")
        .to_path_buf()
}

fn rust_sources(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
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

/// Count bare constructions in a file's PRODUCTION zone — everything above its
/// first `#[cfg(test)]` line. Comment lines are skipped so prose naming the
/// needle (this module's own docs, and `cheap_llm_exec`'s note on the two
/// constructors) is not a site.
fn production_bare_sites(text: &str) -> usize {
    let mut count = 0;
    let lines: Vec<&str> = text.lines().collect();
    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        // The production zone ends at the `#[cfg(test)]` that opens the test
        // MODULE — not at a `#[cfg(test)]` on a single item (three production
        // files carry one on a helper fn mid-file; breaking there would hide the
        // rest of the file from the census — the §3 unification review).
        if trimmed.starts_with("#[cfg(test)]") {
            let opens_module = lines[i + 1..]
                .iter()
                .map(|l| l.trim_start())
                .find(|l| !l.is_empty() && !l.starts_with("#["))
                .is_some_and(|l| {
                    l.starts_with("mod ")
                        || l.starts_with("pub mod ")
                        || l.starts_with("pub(crate) mod ")
                });
            if opens_module {
                break;
            }
            continue;
        }
        if trimmed.starts_with("//") {
            continue;
        }
        for needle in NEEDLES {
            count += line.matches(needle).count();
        }
    }
    count
}

#[test]
fn no_production_path_builds_a_chainless_cheap_llm_executor() {
    let root = repo_root();
    let mut files = Vec::new();
    for tree in PRODUCTION_TREES {
        rust_sources(&root.join(tree), &mut files);
    }
    files.sort();
    assert!(
        files.len() > 100,
        "the walk found only {} rust files — it is not reaching the production \
         trees, so a green result here would mean nothing",
        files.len()
    );
    // The type must actually be reachable from the walk, or the needle is
    // stale (a rename would make every count zero and this test vacuous).
    let mentions: usize = files
        .iter()
        .filter(|p| {
            std::fs::read_to_string(p)
                .map(|t| t.contains("CheapLlmTaskExecutor"))
                .unwrap_or(false)
        })
        .count();
    assert!(
        mentions > 5,
        "only {mentions} production file(s) mention `CheapLlmTaskExecutor` — the \
         type was probably renamed; move this guard with it"
    );

    let mut failures: Vec<String> = Vec::new();
    let mut seen: Vec<(String, usize)> = Vec::new();

    for path in &files {
        let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
        let count = production_bare_sites(&text);
        if count == 0 {
            continue;
        }
        let rel = path
            .strip_prefix(&root)
            .expect("under the repo root")
            .to_string_lossy()
            .replace('\\', "/");
        seen.push((rel.clone(), count));

        match CENSUS.iter().find(|(p, ..)| *p == rel) {
            None => failures.push(format!(
                "{rel}: {count} bare `CheapLlmTaskExecutor::new()/::default()` \
                 site(s) in production code. A bare executor logs no `llm_logs` \
                 row AND carries no fallback chain, so a cheap-LLM task that \
                 fails on its primary profile never fails over (v4 `65f5021c8` \
                 walks the chain). Use `with_logging` — it supplies both — or \
                 justify this site in the census (P4.68)."
            )),
            Some((_, expected, _)) if count != *expected => failures.push(format!(
                "{rel}: {count} bare site(s), census says {expected}. A new one \
                 in an already-justified file is still suspect."
            )),
            Some(_) => {}
        }
    }

    for (rel, expected, why) in CENSUS {
        if !seen.iter().any(|(p, _)| p == rel) {
            failures.push(format!(
                "{rel}: census expects {expected} bare site(s), found none. If \
                 the site moved or was fixed, retire the census row ({why})."
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "a production path is building a chain-less cheap-LLM executor:\n  {}",
        failures.join("\n  ")
    );
}
