//! P4.9G5 restore differential — v5's restore path against v4's REAL
//! `lib/backup/restore/**` (the `system-restore` oracle), over the **same
//! committed archives** both sides read
//! (`crates/quilltap-web/tests/fixtures/restore-archives/`).
//!
//! Feeding one archive to both sides is the point: if each side restored an
//! archive it had written itself, the claim would quietly depend on v5's zip
//! WRITER. As it stands the claim is "given identical bytes on disk, the two
//! restore paths agree" — strictly stronger, and independent of the backup half.
//!
//! ## Part 1 — preview (unit 3)
//!
//! `previewRestore` is filesystem-only, so the diff is its 41-key
//! `RestoreSummary` (or its thrown message) plus one invariant v4 gets from a
//! `finally` and v5 gets from ownership: **the extract directory is gone
//! afterwards, on the success path and on every failure path**. Each case runs
//! against a private scratch root, and the root must be empty when it returns.
//!
//! Error strings are compared VERBATIM where v4 authored them
//! (`restore_malformed_archive`) — that wording reaches the client, because the
//! preview route leaks `error.message` (`system/restore/route.ts:176`). The one
//! exception is documented at its case: an OS not-found message is engine text,
//! so `preview_missing_required` asserts that both sides fail and that both name
//! `data/tags.json`, which is the behavior (required collection missing ⇒
//! throw) rather than the phrasing.
//!
//! Generate the oracle (see `harness/oracle/cases/system-restore.test.ts`), then:
//!   QT_ORACLE_SYSTEM_RESTORE=/tmp/oracle-system-restore.ndjson \
//!     cargo test -p quilltap-harness --test system_restore_equivalence -- --nocapture

use std::path::{Path, PathBuf};

use quilltap_core::services::backup::restore::preview_restore;
use serde_json::Value;

fn archives_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../quilltap-web/tests/fixtures/restore-archives")
}

/// A private scratch root per case, removed on drop — so a leaked extract
/// directory is visible as a non-empty root and never survives the run.
struct Scratch {
    root: PathBuf,
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn fresh_scratch(tag: &str) -> Scratch {
    let root = std::env::temp_dir().join(format!("qt-sysrestore-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    Scratch { root }
}

fn is_empty(dir: &Path) -> bool {
    std::fs::read_dir(dir)
        .map(|mut e| e.next().is_none())
        .unwrap_or(true)
}

fn read_cases(var: &str) -> Option<Vec<Value>> {
    let path = std::env::var(var).ok()?;
    let raw = std::fs::read_to_string(&path).expect("read oracle ndjson");
    let cases: Vec<Value> = raw
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("oracle line is JSON"))
        .collect();
    assert!(!cases.is_empty(), "oracle produced no cases");
    Some(cases)
}

/// The archive each preview case reads. Kept beside the oracle's case list so a
/// rename on either side is a loud mismatch rather than a silent skip.
fn archive_for(name: &str) -> &'static str {
    match name {
        "preview_full" => "restore-archive.zip",
        "preview_legacy" => "restore-archive-legacy.zip",
        "preview_minimal" => "restore-archive-minimal.zip",
        "preview_missing_required" => "restore-archive-missing-required.zip",
        "preview_malformed" => "restore-archive-malformed.zip",
        other => panic!("unknown oracle case {other}"),
    }
}

#[test]
fn system_restore_equivalence() {
    let Some(cases) = read_cases("QT_ORACLE_SYSTEM_RESTORE") else {
        eprintln!("SKIP: QT_ORACLE_SYSTEM_RESTORE unset");
        return;
    };

    let mut failures: Vec<String> = Vec::new();
    let mut seen = 0usize;

    for case in &cases {
        let name = case["name"].as_str().unwrap();
        if !name.starts_with("preview_") {
            continue;
        }
        seen += 1;
        let scratch = fresh_scratch(name);
        let zip = archives_dir().join(archive_for(name));
        assert!(zip.exists(), "missing committed archive fixture: {zip:?}");

        let got = preview_restore(&zip, &scratch.root);

        match (case.get("preview"), case.get("error")) {
            (Some(want), None) => match got {
                Ok(summary) => {
                    let got_json = serde_json::to_value(&summary).expect("summary serializes");
                    if &got_json != want {
                        failures.push(format!(
                            "[{name}] preview summary differs\n  rust:   {}\n  oracle: {}",
                            serde_json::to_string(&got_json).unwrap(),
                            serde_json::to_string(want).unwrap()
                        ));
                    } else {
                        println!("OK {name}: 41-key summary matches");
                    }
                }
                Err(e) => failures.push(format!("[{name}] rust errored, oracle succeeded: {e}")),
            },
            (None, Some(want_err)) => {
                let want_err = want_err.as_str().unwrap();
                match got {
                    Ok(_) => {
                        failures.push(format!("[{name}] rust succeeded, oracle threw: {want_err}"))
                    }
                    Err(e) => {
                        // v4-authored wording is compared verbatim; an OS
                        // not-found message is engine text, so that one case
                        // asserts the FILE both sides name instead.
                        let ok = if name == "preview_missing_required" {
                            e.contains("data/tags.json") && want_err.contains("data/tags.json")
                        } else {
                            e == want_err
                        };
                        if ok {
                            println!("OK {name}: error matches ({e})");
                        } else {
                            failures.push(format!(
                                "[{name}] error differs\n  rust:   {e}\n  oracle: {want_err}"
                            ));
                        }
                    }
                }
            }
            _ => panic!("[{name}] oracle line has neither preview nor error"),
        }

        if !is_empty(&scratch.root) {
            let leaked: Vec<String> = std::fs::read_dir(&scratch.root)
                .unwrap()
                .flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect();
            failures.push(format!(
                "[{name}] the extract directory outlived the call (v4 cleans up in a \
                 `finally`, v5 in `Drop`): {leaked:?}"
            ));
        }
    }

    assert_eq!(seen, 5, "expected all five preview cases in the oracle");
    assert!(
        failures.is_empty(),
        "{} restore difference(s):\n{}",
        failures.len(),
        failures.join("\n")
    );
}
