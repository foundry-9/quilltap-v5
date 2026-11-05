//! P4.D365 — every committed encrypted fixture opens on the linked build.
//!
//! The workspace's git-tracked `*.db` files (113 at the `624d0d25c` round —
//! all under `crates/quilltap-web/tests/fixtures/`, three in
//! `migration-vintage/`) were written by v4's 12.11.1 driver (SQLite3MC 2.3.5)
//! and by v5's own 2.3.5 writer. When the sys crate moved to the 2.4.0
//! amalgamation (P4.D365, the human's ruling R-A), this guard became the
//! 2.3.5-written -> 2.4.0-read proof: each one opens READ-ONLY through the
//! engine's read path with one of the workspace's committed SYNTHETIC test
//! peppers ([`PEPPERS`]), answers `PRAGMA cipher` = `chacha20`, `PRAGMA quick_check` =
//! `ok`, and has a non-empty `sqlite_master`.
//!
//! The list comes from `git ls-files`, so a new fixture is covered
//! automatically; a fixture no committed pepper opens FAILS by name (add its
//! builder's pepper to [`PEPPERS`] — never a real instance's). No oracle: a
//! guard over committed bytes. It is GREEN on the 2.3.5 build too (measured
//! before the bump) — a guard, not a red-first test.
//!
//! Run: `cargo test -p quilltap-harness --test committed_fixture_cipher_guard -- --nocapture`

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use quilltap_core::test_support::open_readonly;

/// The committed synthetic test peppers the fixture builders use (each
/// appears in a builder recipe or a test's constant — never a real pepper).
const PEPPERS: &[(&str, &str)] = &[
    // `crates/quilltap-web/tests/common/mod.rs` `TEST_PEPPER` — the harness
    // corpus pepper most builders take.
    (
        "harness-corpus",
        "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=",
    ),
    ("fixture-b", "ZpjI5jcj5CYsyBA6zPH90G4frQEbv2WsAhERvEKrjJk="),
    ("fixture-c", "ZpjI5jcj5CYsyBA6zPH9WgY4Q0nqLT0lVbF3mFjO5vE="),
    // `host_boot_p4d292_builtin_help_embed.rs` and kin.
    ("deadbeef-a", "3q2+796tvu/erb7v3q2+796tvu/erb7v3q2+796tvu8="),
    ("deadbeef-b", "3q2+796tvu3erb7t3q2+796tvu3erb7t3q2+796tvu0="),
    ("zeros", "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA="),
    ("counting", "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8="),
    ("chat-cast", "Y2hhdC1jYXN0LXRlc3QtcGVwcGVyLTMyLWJ5dGVzISE="),
    (
        "maintenance",
        "Qm1haW50ZW5hbmNlLXRpZXIyLXRlc3QtcGVwcGVyMDE=",
    ),
    // The `testPepperBase64` keys of the committed fixture specs
    // (`harness/oracle/fixtures/*.json`, read by each builder).
    (
        "spec-oracle",
        "b3JhY2xlLXRlc3QtcGVwcGVyLTMyLWJ5dGVzLWxvbmch",
    ),
    (
        "spec-mail-carina",
        "bWFpbGNhcmluYXRvb2xzdGVzdHBlcHBlcjAxMjM0NTY3ODlhYg==",
    ),
    (
        "spec-state-sql",
        "c3RhdGVzcWx0b29sc3Rlc3RwZXBwZXIwMTIzNDU2Nzg5YWJjZGU=",
    ),
    (
        "spec-quilltap-test",
        "cXVpbGx0YXAtdGVzdC1wZXBwZXItMzItYnl0ZXMhIQ==",
    ),
    (
        "spec-fixtures-only-a",
        "dGVzdC1wZXBwZXItZm9yLWZpeHR1cmVzLW9ubHktMzJieXRl",
    ),
    (
        "spec-fixtures-only-b",
        "dGVzdC1wZXBwZXItZm9yLWZpeHR1cmVzLW9ubHkxMjM0NTY3OA==",
    ),
];

/// `EMPTY_FIXTURE` — committed `*.db` files that are deliberately ZERO bytes
/// (an absent sibling planted as an empty file: SQLite opens a 0-byte file as
/// an empty database with any key, so there is no cipher to prove). Named both
/// ways: each must still be empty, and an empty file not listed fails.
const EMPTY_FIXTURE: &[&str] = &["crates/quilltap-web/tests/fixtures/chat-compressed-mount.db"];

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

fn tracked_dbs(root: &Path) -> Vec<String> {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["ls-files", "-z", "*.db"])
        .output()
        .expect("git ls-files (this guard needs git)");
    assert!(
        out.status.success(),
        "git ls-files failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout)
        .unwrap()
        .split('\0')
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

/// The first committed pepper whose read-only open reads `sqlite_master`.
fn opening_pepper(path: &Path) -> Option<(&'static str, &'static str, i64)> {
    PEPPERS.iter().find_map(|(name, pepper)| {
        let conn = open_readonly(path, pepper);
        conn.query_row("SELECT count(*) FROM sqlite_master", [], |r| {
            r.get::<_, i64>(0)
        })
        .ok()
        .map(|n| (*name, *pepper, n))
    })
}

#[test]
fn every_committed_db_fixture_opens_encrypted() {
    let root = workspace_root();
    let files = tracked_dbs(&root);
    assert!(
        !files.is_empty(),
        "git ls-files found no *.db — wrong root {}",
        root.display()
    );
    let mut failures = Vec::new();
    let mut by_pepper: BTreeMap<&str, usize> = BTreeMap::new();
    let mut empty = 0usize;
    // Every open reads a scratch COPY: a WAL-mode fixture (the three
    // `migration-vintage/` files) grows `-shm` / `-wal` siblings on any open,
    // read-only included, and those must never appear in the source tree.
    let scratch = tempfile::tempdir().unwrap();
    for (n, rel) in files.iter().enumerate() {
        let src = root.join(rel);
        let path = scratch.path().join(format!("{n}.db"));
        std::fs::copy(&src, &path).unwrap_or_else(|e| panic!("{rel}: copy: {e}"));
        let head = std::fs::read(&path).unwrap_or_else(|e| panic!("{rel}: {e}"));
        let listed_empty = EMPTY_FIXTURE.contains(&rel.as_str());
        if head.is_empty() != listed_empty {
            failures.push(format!(
                "{rel}: EMPTY_FIXTURE — {} bytes, listed empty = {listed_empty}",
                head.len()
            ));
            continue;
        }
        if listed_empty {
            empty += 1;
            continue;
        }
        if head.starts_with(b"SQLite format 3\0") {
            failures.push(format!(
                "{rel}: PLAINTEXT — every committed fixture must be encrypted"
            ));
            continue;
        }
        let Some((name, pepper, n)) = opening_pepper(&path) else {
            failures.push(format!("{rel}: no committed test pepper opens it"));
            continue;
        };
        let conn = open_readonly(&path, pepper);
        let cipher: String = conn.query_row("PRAGMA cipher", [], |r| r.get(0)).unwrap();
        let qc: Vec<String> = {
            let mut s = conn.prepare("PRAGMA quick_check").unwrap();
            let rows = s.query_map([], |r| r.get::<_, String>(0)).unwrap();
            rows.map(|r| r.unwrap_or_else(|e| format!("<error {e}>")))
                .collect()
        };
        if cipher != "chacha20" {
            failures.push(format!("{rel}: PRAGMA cipher = {cipher}, want chacha20"));
        }
        if qc != ["ok"] {
            failures.push(format!("{rel}: quick_check = {qc:?}"));
        }
        if n == 0 {
            failures.push(format!("{rel}: empty sqlite_master"));
        }
        *by_pepper.entry(name).or_default() += 1;
    }
    assert!(
        failures.is_empty(),
        "{} fixture failure(s):\n{}",
        failures.len(),
        failures.join("\n")
    );
    eprintln!(
        "OK: {} committed *.db fixtures — {} opened (chacha20, quick_check ok), {empty} EMPTY_FIXTURE; by pepper {by_pepper:?}",
        files.len(),
        files.len() - empty
    );
    assert_eq!(
        empty,
        EMPTY_FIXTURE.len(),
        "an EMPTY_FIXTURE entry is no longer tracked"
    );
}
