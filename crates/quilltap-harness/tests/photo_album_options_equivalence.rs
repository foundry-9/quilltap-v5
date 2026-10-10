//! P4.D275 — tier-2 differential: the wardrobe picture viewer's every-store
//! save-target list (`quilltap_core::photos::photo_album_options::
//! list_all_photo_album_options`) against v4's REAL `listAllPhotoAlbumOptions`
//! (`lib/photos/photo-album-options.ts`, `01a83539d`).
//!
//! Both sides start from the item-images tier-2 builder's pair, built into this
//! family's OWN paths (never a shared `/tmp` path); per case each side runs the
//! case's `main` / `mount` SQL plants (`harness/oracle/fixtures/
//! photo-album-options.json`) on a FRESH copy, then ONE call. Compared:
//!   - the `{albums}` BYTES — key order included (`isDefault` LAST, appended to
//!     the first option when no General is configured);
//!   - the ordered log lines of the call — level, message and fields, the
//!     targets dropped (v4's child-logger context is v5's target: the
//!     `createServiceLogger` `module=service` key is not a field on either
//!     side) and v4's backend `Raw query failed` dropped by the standing
//!     convention.
//!
//! **Three measured divergences, each a NAMED both-ways carve** (a v4 change —
//! or a v5 one — trips it):
//!   - `ENABLED_STORE_ZOD_DROP` (`malformed_store_row`, M2): v4's `findEnabled`
//!     runs every row through `DocMountPointSchema` and DROPS one whose
//!     `mountType` is outside the enum (after `Data validation failed` + `Safe
//!     validation failed`); v5's `find_enabled_for_search` reads `id, name,
//!     storeType` and keeps it, so v5 offers it as a `document-store`. Recorded,
//!     not fixed — `db/doc_mount_points.rs` is P4.D274's (a follow-ups item).
//!   - `RAW_CHARACTER_ZOD_DROP` (`archived_with_empty_vault_pointer`): v4's
//!     `characters.findAllRaw()` Zod-drops a row whose
//!     `characterDocumentMountPointId` is `''` (not a uuid) — on BOTH of its
//!     reads (the archived-ids helper and the list) — so it scans 2 characters
//!     where v5's `find_all_raw` (no per-row validation, pre-existing) scans 3.
//!     The albums are identical (an `''` pointer is falsy either way).
//!   - `ENSURE_COLLECTION_RECREATES` (`dropped_mount_points_table`): v4's
//!     `findByFilter` re-creates an ABSENT `doc_mount_points` table (and its
//!     NOCASE index — INFO `Mount-index: created the unique document-store name
//!     index`) and answers `[]` with no error; v5 answers `[]` through R-C's
//!     `Error finding entities by filter` arm. Unreachable on a booted instance
//!     (the boot owns table creation — the wear ledger's recorded twin).
//!
//! R-C's DEGRADED arm (a mount index that cannot be acquired) has no v4 driver
//! here; it is pinned v5-only below over `DbPaths::main_only`, quoting v4's
//! `dedicated-db.repository.ts` DEBUG.
//!
//! Generate (Node 24, from the pinned v4 worktree — see the .ts header):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   TMPO=/tmp/qt-pao-oracle
//!   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
//!   cp "$V5W/harness/oracle/cases/photo-album-options.test.ts" "$TMPO/cases/"
//!   cp "$V5W/harness/oracle/fixtures/wardrobe-item-images-tier2.json" "$TMPO/fixtures/"
//!   cp "$V5W/harness/oracle/fixtures/photo-album-options.json" "$TMPO/fixtures/"
//!   cd ~/source/quilltap-server
//!   rm -f /tmp/qt-pao-main.db /tmp/qt-pao-mount.db
//!   QT_FIXTURE_WII_MAIN=/tmp/qt-pao-main.db QT_FIXTURE_WII_MOUNT=/tmp/qt-pao-mount.db \
//!     $N/node --import tsx $V5W/harness/oracle/fixtures/build-wardrobe-item-images-fixture.ts
//!   QT_FIXTURE_PAO_MAIN=/tmp/qt-pao-main.db QT_FIXTURE_PAO_MOUNT=/tmp/qt-pao-mount.db \
//!   QT_ORACLE_OUT=/tmp/oracle-photo-album-options.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=180000 \
//!       --roots "$PWD" --roots "$TMPO/cases" -- "cases/photo-album-options\.test\.ts$"
//! Run:
//!   QT_ORACLE_PAO=/tmp/oracle-photo-album-options.ndjson \
//!   QT_FIXTURE_PAO_MAIN=/tmp/qt-pao-main.db QT_FIXTURE_PAO_MOUNT=/tmp/qt-pao-mount.db \
//!     cargo test -p quilltap-harness --test photo_album_options_equivalence -- --nocapture

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::db::Writer;
use quilltap_core::photos::photo_album_options::list_all_photo_album_options;
use quilltap_core::test_support::global_capture;
use serde_json::Value;

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../harness/oracle/fixtures")
}

fn read_json(name: &str) -> Value {
    serde_json::from_str(&std::fs::read_to_string(fixtures_dir().join(name)).unwrap()).unwrap()
}

/// v4's line as `LEVEL message k=v…` — the child-logger keys (`module`,
/// `service`) and the backend's `Raw query failed` dropped.
fn v4_line(entry: &Value) -> Option<String> {
    let message = entry["message"].as_str().unwrap();
    if message == "Raw query failed" {
        return None;
    }
    let mut line = format!(
        "{} {message}",
        entry["level"].as_str().unwrap().to_uppercase()
    );
    for kv in entry["fields"].as_array().unwrap() {
        let k = kv[0].as_str().unwrap();
        if k == "module" || k == "service" {
            continue;
        }
        let v = match &kv[1] {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        line.push_str(&format!(" {k}={v}"));
    }
    Some(line)
}

/// v5's captured line minus its target (`LEVEL target message k=v…` →
/// `LEVEL message k=v…`).
fn v5_line(captured: &str) -> String {
    let mut parts = captured.splitn(3, ' ');
    let level = parts.next().unwrap();
    let _target = parts.next();
    format!("{level} {}", parts.next().unwrap_or_default())
}

#[test]
fn photo_album_options_match_oracle() {
    global_capture::install();
    let (Ok(oracle_path), Ok(main_fixture), Ok(mount_fixture)) = (
        std::env::var("QT_ORACLE_PAO"),
        std::env::var("QT_FIXTURE_PAO_MAIN"),
        std::env::var("QT_FIXTURE_PAO_MOUNT"),
    ) else {
        eprintln!(
            "SKIP: set QT_ORACLE_PAO, QT_FIXTURE_PAO_MAIN and QT_FIXTURE_PAO_MOUNT (see header)."
        );
        return;
    };
    let pepper = read_json("wardrobe-item-images-tier2.json")["testPepperBase64"]
        .as_str()
        .unwrap()
        .to_string();
    let spec = read_json("photo-album-options.json");
    let cases = spec["cases"].as_array().unwrap();

    let oracle: HashMap<String, Value> = std::fs::read_to_string(&oracle_path)
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let v: Value = serde_json::from_str(l).unwrap();
            (v["name"].as_str().unwrap().to_string(), v)
        })
        .collect();
    assert_eq!(
        oracle.len(),
        cases.len(),
        "stale oracle: {} cases for {} in the spec — regenerate it",
        oracle.len(),
        cases.len()
    );

    let mut failed: Vec<String> = Vec::new();
    let mut carves_seen = 0;
    for case in cases {
        let name = case["name"].as_str().unwrap();
        let want = &oracle[name];

        let dir = tempfile::tempdir().unwrap();
        let (main, mount) = (dir.path().join("main.db"), dir.path().join("mount.db"));
        std::fs::copy(&main_fixture, &main).unwrap();
        std::fs::copy(&mount_fixture, &mount).unwrap();
        for (path, key) in [(&main, "main"), (&mount, "mount")] {
            let w = Writer::open_writable(path, &pepper).unwrap();
            for sql in case[key].as_array().unwrap() {
                w.connection().execute_batch(sql.as_str().unwrap()).unwrap();
            }
        }
        let db = Db::open(
            DbPaths {
                main: main.clone(),
                mount_index: Some(mount.clone()),
                llm_logs: None,
            },
            &pepper,
        )
        .unwrap();

        let (albums, lines) = global_capture::capture(|| list_all_photo_album_options(&db));
        let got_albums = serde_json::to_string(&albums).unwrap();
        let want_albums = serde_json::to_string(&want["albums"]).unwrap();
        let got_lines: Vec<String> = lines.iter().map(|l| v5_line(l)).collect();
        let want_lines: Vec<String> = want["logs"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(v4_line)
            .collect();

        match name {
            "malformed_store_row" => {
                // ENABLED_STORE_ZOD_DROP — both ways.
                carves_seen += 1;
                let odd = r#"{"mountPointId":"aa000000-0000-4000-8000-00000000000a","name":"Odd Shelf","kind":"document-store"}"#;
                assert!(
                    !want_albums.contains("Odd Shelf"),
                    "[{name}] v4 now offers the malformed store — ENABLED_STORE_ZOD_DROP \
                     converged: retire the carve"
                );
                assert_eq!(
                    got_albums.replace(&format!("{odd},"), ""),
                    want_albums,
                    "[{name}] v5 = v4 plus the kept row"
                );
                assert!(
                    want_lines.iter().any(|l| l
                        .starts_with("ERROR Data validation failed collection=doc_mount_points")),
                    "[{name}] v4's validation drop line: {want_lines:#?}"
                );
                let v4_rest: Vec<&String> = want_lines
                    .iter()
                    .filter(|l| !l.contains("validation failed"))
                    .collect();
                let v4_rest: Vec<String> = v4_rest
                    .into_iter()
                    .map(|l| {
                        l.replace("enabledStores=6", "enabledStores=7")
                            .replace("offered=5", "offered=6")
                    })
                    .collect();
                assert_eq!(
                    got_lines, v4_rest,
                    "[{name}] lines (v5 counts the kept row)"
                );
                continue;
            }
            "archived_with_empty_vault_pointer" => {
                // RAW_CHARACTER_ZOD_DROP — both ways.
                carves_seen += 1;
                assert_eq!(got_albums, want_albums, "[{name}] albums");
                let drops = want_lines
                    .iter()
                    .filter(|l| l.starts_with("ERROR Data validation failed collection=characters"))
                    .count();
                assert_eq!(
                    drops, 2,
                    "[{name}] v4 drops the row on BOTH raw reads — RAW_CHARACTER_ZOD_DROP \
                     moved: {want_lines:#?}"
                );
                let v4_rest: Vec<String> = want_lines
                    .iter()
                    .filter(|l| !l.contains("validation failed"))
                    .map(|l| l.replace("charactersScanned=2", "charactersScanned=3"))
                    .collect();
                assert_eq!(got_lines, v4_rest, "[{name}] lines (v5 scans the kept row)");
                continue;
            }
            "dropped_mount_points_table" => {
                // ENSURE_COLLECTION_RECREATES — both ways.
                carves_seen += 1;
                assert_eq!(got_albums, want_albums, "[{name}] albums");
                assert!(
                    want_lines
                        .iter()
                        .any(|l| l
                            == "INFO Mount-index: created the unique document-store name index"),
                    "[{name}] v4 no longer re-creates the table — ENSURE_COLLECTION_RECREATES \
                     moved: {want_lines:#?}"
                );
                let v4_rest: Vec<&String> = want_lines
                    .iter()
                    .filter(|l| !l.starts_with("INFO Mount-index: created"))
                    .collect();
                assert_eq!(
                    got_lines.len(),
                    v4_rest.len() + 1,
                    "[{name}] v5 adds R-C's one ERROR: {got_lines:#?}"
                );
                assert_eq!(
                    got_lines[0],
                    "ERROR Error finding entities by filter collection=doc_mount_points error=no such table: doc_mount_points"
                );
                assert_eq!(
                    got_lines[1..].iter().collect::<Vec<_>>(),
                    v4_rest,
                    "[{name}] the rest"
                );
                continue;
            }
            _ => {}
        }

        if got_albums != want_albums {
            eprintln!("[{name}] ALBUMS\n  v5: {got_albums}\n  v4: {want_albums}");
            failed.push(format!("{name}:albums"));
        }
        if got_lines != want_lines {
            eprintln!("[{name}] LINES\n  v5: {got_lines:#?}\n  v4: {want_lines:#?}");
            failed.push(format!("{name}:lines"));
        }
    }
    assert_eq!(carves_seen, 3, "every named carve ran");
    assert!(
        failed.is_empty(),
        "photo-album-options diverged: {failed:?}"
    );
    eprintln!(
        "OK: photo-album-options matched v4 on {} cases.",
        cases.len()
    );
}

/// R-C's DEGRADED arm (v5-only — no v4 driver): a `Db` with NO mount index
/// answers the enabled-stores read with v4's dedicated-db DEBUG (`dedicated-db.
/// repository.ts`: `Dedicated database unavailable; answering with the
/// fallback {collection, dbTarget, error}`) and offers nothing.
#[test]
fn a_degraded_mount_index_offers_nothing() {
    global_capture::install();
    let Ok(main_fixture) = std::env::var("QT_FIXTURE_PAO_MAIN") else {
        eprintln!("SKIP: set QT_FIXTURE_PAO_MAIN (see header).");
        return;
    };
    let pepper = read_json("wardrobe-item-images-tier2.json")["testPepperBase64"]
        .as_str()
        .unwrap()
        .to_string();
    let dir = tempfile::tempdir().unwrap();
    let main = dir.path().join("main.db");
    std::fs::copy(&main_fixture, &main).unwrap();
    let db = Db::open(DbPaths::main_only(main), &pepper).unwrap();
    let (albums, lines) = global_capture::capture(|| list_all_photo_album_options(&db));
    assert!(albums.is_empty());
    let lines: Vec<String> = lines.iter().map(|l| v5_line(l)).collect();
    assert_eq!(
        lines.first().map(String::as_str),
        Some("DEBUG Dedicated database unavailable; answering with the fallback collection=doc_mount_points dbTarget=mountIndex error=Mount index database is in degraded mode")
    );
    assert_eq!(
        lines.last().map(String::as_str),
        Some("DEBUG [photo-album-options] Listed every store as a save target enabledStores=0 archivedVaultsSkipped=1 offered=0")
    );
}
