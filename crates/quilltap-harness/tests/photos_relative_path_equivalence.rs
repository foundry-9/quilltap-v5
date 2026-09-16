//! Tier-1 differential test: `isPhotosRelativePath` (the stale-chat maintenance
//! sweep's album-protection predicate, v4 `lib/photos/photos-paths.ts`).
//!
//! Exact-equality check against the v4 oracle: for every relative path in the
//! corpus, `quilltap_core::photos::photos_paths::is_photos_relative_path` must
//! return the boolean v4's real `isPhotosRelativePath` returns (the JS
//! `path.posix.dirname(...).toLowerCase()` folder check, reproduced faithfully).
//!
//! The import is the SURVIVING home (P4.91). Until that lane it drove a second
//! copy in `db::doc_mount_file_links` — the copy that was already Node-faithful,
//! so the corpus could be grown with the trailing-slash-run shapes and stay
//! green having measured nothing. Repointing FIRST is what let the growth go red.
//!
//! Generate the oracle output:
//!   cd ~/source/quilltap-server
//!   npx tsx ~/source/quilltap-v5/harness/oracle/cases/photos-relative-path.ts \
//!     > /tmp/oracle-photos-relative-path.ndjson
//! Run:
//!   QT_ORACLE_PHOTOS_PATH=/tmp/oracle-photos-relative-path.ndjson \
//!     cargo test -p quilltap-harness --test photos_relative_path_equivalence

use quilltap_core::photos::photos_paths::is_photos_relative_path;
use serde::Deserialize;

#[derive(Deserialize)]
struct Row {
    id: String,
    path: Option<String>,
    out: bool,
}

#[test]
fn photos_relative_path_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_PHOTOS_PATH") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_PHOTOS_PATH to the oracle NDJSON (see header).");
            return;
        }
    };
    let text =
        std::fs::read_to_string(&oracle_path).unwrap_or_else(|e| panic!("cannot read oracle: {e}"));

    // Collect every mismatch rather than asserting per row: a first-diff
    // comparator names ONE row and cannot bound the red set, which is exactly
    // what P4.91's red-first step needed to read (the trailing-slash-run rows).
    let mut n = 0;
    let mut mismatches: Vec<String> = Vec::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Row = serde_json::from_str(line).expect("parse oracle row");
        let got = is_photos_relative_path(row.path.as_deref());
        if got != row.out {
            mismatches.push(format!(
                "[{}] is_photos_relative_path({:?}) = {got}, oracle = {}",
                row.id, row.path, row.out
            ));
        }
        n += 1;
    }
    assert!(
        mismatches.is_empty(),
        "{} of {n} rows disagree with v4:\n  {}",
        mismatches.len(),
        mismatches.join("\n  ")
    );
    // EXACT, not a floor: a stale oracle regenerated before P4.91 grew the
    // corpus would sail past a `>=` having measured none of the slash shapes.
    assert_eq!(
        n, 30,
        "expected the full corpus (30 rows), saw {n} — stale oracle?"
    );
    eprintln!("OK: is_photos_relative_path matched oracle on {n} inputs.");
}
