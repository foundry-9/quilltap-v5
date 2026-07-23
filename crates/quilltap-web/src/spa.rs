//! Where the Angular dist comes from (P4.10).
//!
//! The dist used to arrive *only* via `--spa-dir`, with no fallback and no
//! diagnostic: a `quilltap-web` copied out of `target/release` — or, until
//! this unit, the container itself — served the embedded placeholder pages
//! forever and never said so. This gives the dist the same shape of
//! resolution chain `--data-dir` already has in `quilltap-host::paths`:
//!
//! 1. `--spa-dir <path>` (explicit wins, verbatim),
//! 2. `QUILLTAP_SPA_DIR` (explicit too, verbatim),
//! 3. a binary-relative conventional location that actually holds a dist,
//! 4. `None` — the placeholder pages, unchanged.
//!
//! Links 1 and 2 are taken **verbatim, unvalidated**: an explicit path is an
//! instruction, and `--spa-dir`'s existing semantics are frozen (the
//! Playwright suite passes it and is their proof). Link 3 is a *guess*, so it
//! must prove itself — a candidate counts only when it holds an
//! `index.html` — which also keeps the startup banner honest.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// The binary-relative locations searched, in order, when nothing explicit
/// names a dist:
///
/// - `<exe dir>/spa` — the self-contained drop: the binary and its `spa/`
///   sitting in one directory, which is how a copied-out release binary is
///   expected to travel.
/// - `<exe dir>/../share/quilltap/spa` — the FHS install layout, which is
///   what the Docker image lays down (`/usr/local/bin/quilltap-web` beside
///   `/usr/local/share/quilltap/spa`).
const BINARY_RELATIVE_CANDIDATES: &[&str] = &["spa", "../share/quilltap/spa"];

/// A directory counts as a dist only if it holds the `index.html` the SPA
/// fallback serves. Guessing wrong is worse than not guessing: it would make
/// the startup banner announce an SPA that is not there.
fn holds_a_dist(dir: &Path) -> bool {
    dir.join("index.html").is_file()
}

/// The pure chain: flag → env → binary-relative candidate → `None`.
///
/// Split from the process for testability — the caller supplies the env value
/// and the executable's directory, so a test never has to mutate the ambient
/// environment (which is process-global and races other tests).
pub fn resolve_spa_dir(
    flag: Option<PathBuf>,
    env: Option<OsString>,
    exe_dir: Option<&Path>,
) -> Option<PathBuf> {
    if let Some(dir) = flag {
        return Some(dir);
    }
    if let Some(env) = env {
        if !env.is_empty() {
            return Some(PathBuf::from(env));
        }
    }
    let exe_dir = exe_dir?;
    BINARY_RELATIVE_CANDIDATES
        .iter()
        .map(|rel| exe_dir.join(rel))
        .find(|candidate| holds_a_dist(candidate))
}

/// The chain over the real process: reads `QUILLTAP_SPA_DIR` and the running
/// executable's directory. A binary whose own path is unknowable (no
/// `current_exe`) simply has no link-3 candidate.
pub fn resolve_spa_dir_for_process(flag: Option<PathBuf>) -> Option<PathBuf> {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf));
    resolve_spa_dir(
        flag,
        std::env::var_os("QUILLTAP_SPA_DIR"),
        exe_dir.as_deref(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A directory holding an `index.html`, i.e. something the chain accepts.
    fn dist_at(root: &Path, rel: &str) -> PathBuf {
        let dir = root.join(rel);
        std::fs::create_dir_all(&dir).expect("create dist dir");
        std::fs::write(dir.join("index.html"), "<!doctype html>").expect("write index");
        dir
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("qt-spa-resolve-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create scratch");
        dir
    }

    #[test]
    fn flag_wins_over_everything_and_is_taken_verbatim() {
        let root = scratch("flag-wins");
        let exe_dir = root.join("bin");
        dist_at(&exe_dir, "spa");
        // The flag names a path that does NOT exist: explicit is an
        // instruction, not a suggestion, so it is returned unvalidated.
        let flag = root.join("nowhere-at-all");

        let got = resolve_spa_dir(
            Some(flag.clone()),
            Some(OsString::from(root.join("env-dist"))),
            Some(&exe_dir),
        );
        assert_eq!(got, Some(flag));
    }

    #[test]
    fn env_wins_over_the_binary_relative_default() {
        let root = scratch("env-wins");
        let exe_dir = root.join("bin");
        dist_at(&exe_dir, "spa");
        let env_dir = root.join("env-dist");

        let got = resolve_spa_dir(None, Some(OsString::from(env_dir.clone())), Some(&exe_dir));
        assert_eq!(got, Some(env_dir));
    }

    #[test]
    fn an_empty_env_var_does_not_count_as_explicit() {
        let root = scratch("empty-env");
        let exe_dir = root.join("bin");
        let beside = dist_at(&exe_dir, "spa");

        let got = resolve_spa_dir(None, Some(OsString::new()), Some(&exe_dir));
        assert_eq!(got, Some(beside), "an empty value falls through the chain");
    }

    #[test]
    fn finds_the_dist_sitting_beside_the_binary() {
        let root = scratch("beside");
        let exe_dir = root.join("bin");
        let beside = dist_at(&exe_dir, "spa");

        let got = resolve_spa_dir(None, None, Some(&exe_dir));
        assert_eq!(got, Some(beside));
    }

    #[test]
    fn finds_the_fhs_install_layout_the_container_uses() {
        // /usr/local/bin/quilltap-web + /usr/local/share/quilltap/spa
        let root = scratch("fhs");
        let exe_dir = root.join("bin");
        std::fs::create_dir_all(&exe_dir).expect("create bin");
        let share = dist_at(&root, "share/quilltap/spa");

        let got = resolve_spa_dir(None, None, Some(&exe_dir)).expect("resolved");
        assert_eq!(
            std::fs::canonicalize(&got).expect("canonicalize got"),
            std::fs::canonicalize(&share).expect("canonicalize share"),
        );
    }

    #[test]
    fn the_beside_candidate_is_searched_before_the_share_one() {
        let root = scratch("precedence");
        let exe_dir = root.join("bin");
        let beside = dist_at(&exe_dir, "spa");
        dist_at(&root, "share/quilltap/spa");

        let got = resolve_spa_dir(None, None, Some(&exe_dir));
        assert_eq!(got, Some(beside));
    }

    #[test]
    fn a_candidate_without_an_index_html_is_not_a_dist() {
        let root = scratch("no-index");
        let exe_dir = root.join("bin");
        // An empty `spa/` next to the binary must not satisfy the chain, or
        // the banner would announce an SPA that cannot be served.
        std::fs::create_dir_all(exe_dir.join("spa")).expect("create empty spa");

        assert_eq!(resolve_spa_dir(None, None, Some(&exe_dir)), None);
    }

    #[test]
    fn nothing_anywhere_yields_none_the_placeholder_tail() {
        let root = scratch("none");
        let exe_dir = root.join("bin");
        std::fs::create_dir_all(&exe_dir).expect("create bin");

        assert_eq!(resolve_spa_dir(None, None, Some(&exe_dir)), None);
        assert_eq!(resolve_spa_dir(None, None, None), None);
    }
}
