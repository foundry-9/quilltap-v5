//! The repo-wide spelling guard (P4.D68 tier 2 — the v5 analog of v4 4.8.1's
//! `scripts/check-quilltap-spelling.mjs` + its `npm run lint` wiring).
//!
//! The standing rule — the project is "Quilltap" (quill + tap), never the
//! quilt-based misspelling — had no mechanical enforcement in v5 at all.
//! `harness/tools/check_spelling.py` is the sweep (see its docstring for the
//! allowlist reasoning); this test runs it under `cargo test --workspace`, so
//! the misspelling fails the same gate everything else does.
//!
//! Run standalone:
//!   cargo test -p quilltap-harness --test spelling_guard

use std::path::PathBuf;
use std::process::Command;

#[test]
fn repo_spells_quilltap_correctly() {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("harness crate sits two levels under the repo root")
        .to_path_buf();
    let script = repo_root.join("harness/tools/check_spelling.py");
    assert!(script.exists(), "missing {}", script.display());

    let out = Command::new("python3")
        .arg(&script)
        .current_dir(&repo_root)
        .output()
        .expect("python3 must be runnable (the recipe sweep already requires it)");
    assert!(
        out.status.success(),
        "the spelling sweep found misspellings:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
}
