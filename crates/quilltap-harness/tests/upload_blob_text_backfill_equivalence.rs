//! Tier-2 differential (P4.D354, v4 `40fab1c54`, bugs 260 / 261 — contract
//! C2): `services::mount_index::upload_blob_text_backfill::
//! backfill_upload_blob_text` vs v4's REAL `backfillUploadBlobText`
//! (`lib/startup/backfill-upload-blob-text.ts`) over the SAME minted
//! pre-bug-260 pair (`build-upload-blob-text-backfill-fixture.ts` copies the
//! committed mounts pair it finds beside itself — read-only — and plants
//! uploads through v4's REAL `storeMountFile` with the old `extractText:
//! false`), run three times on one copy: `first`, `steady` (idempotence — the
//! bytes-missing row is retried, nothing else moves) and `emptied` (after a
//! raw delete of that row, identical on both sides — the empty-worklist DEBUG).
//!
//! Per run: the result tallies, the captured lines (v5's under
//! `quilltap::startup` through `global_capture` — the call runs on this test's
//! thread, so the rig sees it; in production it runs inside the host's
//! `write_blocking` closure), and the two partitions' dumps (the mount-write
//! family's SQL + normalization: `tests/mount_common/mod.rs`).
//!
//! ⚖ **`REFUSING_SEAM_LEAVES_PDF_UNEXTRACTED` (R-1), both ways.** The planted
//! `uploads/Report.PDF` and the mounts fixture's own `docs/report.pdf` are the
//! rows the two sides treat differently while
//! v5's `DocumentTextExtractor` refuses (`is_available() == false`): v4's
//! converter answers `''` and v4 writes `failed` / `Converter produced no
//! text` / `skipped` and COUNTS them (candidates + 2, failed + 2 on `first`);
//! v5 never lists them, writes nothing (`extractionStatus` stays `'none'`)
//! and logs its own INFO `Leaving PDF/DOCX uploads unextracted; …`
//! (`skipped=2`) on every run. [`carve_tables`] asserts BOTH shapes exactly and
//! then lifts the rows out of the comparison; when a real extractor lands
//! (`is_available()` → true) the v5 shape assertion fails and the carve is
//! retired to equality.
//!
//! Regenerate (Node 24; STAGE outside `.claude/`):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; W=<this worktree>
//!   STAGE=/tmp/qt-oracle-stage-ubtb
//!   rm -rf $STAGE && mkdir -p $STAGE/harness/oracle/cases $STAGE/harness/oracle/fixtures
//!   cp $W/harness/oracle/cases/upload-blob-text-backfill.test.ts $STAGE/harness/oracle/cases/
//!   cp $W/harness/oracle/fixtures/mounts.json $STAGE/harness/oracle/fixtures/
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_UBTB_MAIN=/tmp/qt-ubtb-main.db QT_FIXTURE_UBTB_MOUNT=/tmp/qt-ubtb-mount.db \
//!     $N/node --import tsx $W/harness/oracle/fixtures/build-upload-blob-text-backfill-fixture.ts
//!   QT_FIXTURE_UBTB_MAIN=/tmp/qt-ubtb-main.db QT_FIXTURE_UBTB_MOUNT=/tmp/qt-ubtb-mount.db \
//!   QT_ORACLE_OUT=/tmp/oracle-ubtb.ndjson \
//!     $N/npx jest --silent --watchman=false --testTimeout=180000 \
//!       --roots "$PWD" --roots "$STAGE/harness/oracle/cases" -- "upload-blob-text-backfill\.test\.ts$"
//! Run:
//!   QT_ORACLE_UBTB=/tmp/oracle-ubtb.ndjson \
//!   QT_FIXTURE_UBTB_MAIN=/tmp/qt-ubtb-main.db QT_FIXTURE_UBTB_MOUNT=/tmp/qt-ubtb-mount.db \
//!     cargo test -p quilltap-harness --test upload_blob_text_backfill_equivalence -- --nocapture

#[path = "mount_common/mod.rs"]
mod mount_common;

use mount_common::*;
use quilltap_core::db::Writer;
use quilltap_core::services::mount_index::converters::RefusingTextExtractor;
use quilltap_core::services::mount_index::upload_blob_text_backfill::backfill_upload_blob_text;
use quilltap_core::test_support::global_capture;
use serde_json::{json, Map, Value};

/// The R-1 carve rows: the planted upper-case `.PDF` and the mounts fixture's
/// own pre-existing `docs/report.pdf` blob (also `'none'`).
const CARVE_PATHS: [&str; 2] = ["docs/report.pdf", "uploads/Report.PDF"];
/// v5's own line while the seam refuses (fires on every run: the row stays).
const V5_SKIP_LINE: &str = "INFO quilltap::startup Leaving PDF/DOCX uploads unextracted; no document text extractor is available skippedExtensions=[\".pdf\", \".docx\"] skipped=2";

/// Render one v4 captured line the way `global_capture` renders v5's.
fn render_v4(line: &Value) -> String {
    let level = line["level"].as_str().unwrap().to_uppercase();
    let mut out = format!(
        "{level} quilltap::startup {}",
        line["message"].as_str().unwrap()
    );
    if let Some(fields) = line["fields"].as_object() {
        for (k, v) in fields {
            match v {
                Value::String(s) => out.push_str(&format!(" {k}={s}")),
                other => out.push_str(&format!(" {k}={other}")),
            }
        }
    }
    out
}

/// R-1: v4 counts the carve rows on `first` (they are `failed` after that).
fn carve_counts(run: &str, mut v4: Value) -> Value {
    if run == "first" {
        for k in ["candidates", "failed"] {
            if let Some(n) = v4.get(k).and_then(Value::as_i64) {
                v4[k] = json!(n - CARVE_PATHS.len() as i64);
            }
        }
    }
    v4
}

fn links_row<'a>(tables: &'a Value, path: &str) -> &'a Value {
    tables["links"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["relativePath"] == path)
        .unwrap_or_else(|| panic!("no link row {path}"))
}

/// Assert both sides' shapes for each carve row, then lift them (and only
/// them) out of the comparison.
fn carve_tables(run: &str, got: &mut Value, want: &mut Value) {
    for path in CARVE_PATHS {
        let g = links_row(got, path);
        // Still on the worklist: `COALESCE(extractionStatus, 'none') = 'none'`.
        let status = g["extractionStatus"].as_str().unwrap_or("none").to_string();
        assert_eq!(
            (
                status.as_str(),
                g["extractionError"].clone(),
                g["extractedText"].clone()
            ),
            ("none", Value::Null, Value::Null),
            "{run}: v5's {path} must stay unextracted while the seam refuses \
             (REFUSING_SEAM_LEAVES_PDF_UNEXTRACTED — retire the carve if an extractor landed)"
        );
        let w = links_row(want, path);
        assert_eq!(
            (
                w["extractionStatus"].as_str(),
                w["extractionError"].as_str(),
                w["conversionStatus"].as_str(),
                w["conversionError"].as_str()
            ),
            (
                Some("failed"),
                Some("Converter produced no text"),
                Some("skipped"),
                Some("Converter produced no text")
            ),
            "{run}: v4's {path} shape moved"
        );
    }
    for t in [got, want] {
        let rows = t["links"].as_array_mut().unwrap();
        rows.retain(|r| !CARVE_PATHS.contains(&r["relativePath"].as_str().unwrap_or("")));
    }
}

fn dump(main: &rusqlite::Connection, mount: &rusqlite::Connection) -> Value {
    let mut out = Map::new();
    for (name, sql) in MOUNT_TABLES {
        out.insert((*name).to_string(), Value::Array(rows_to_json(mount, sql)));
    }
    out.insert(
        "jobs".to_string(),
        Value::Array(rows_to_json(main, JOBS_SQL)),
    );
    Value::Object(out)
}

#[test]
fn upload_blob_text_backfill_matches_oracle() {
    global_capture::install();
    let (Ok(oracle), Ok(fix_main), Ok(fix_mount)) = (
        std::env::var("QT_ORACLE_UBTB"),
        std::env::var("QT_FIXTURE_UBTB_MAIN"),
        std::env::var("QT_FIXTURE_UBTB_MOUNT"),
    ) else {
        eprintln!("SKIP: set QT_ORACLE_UBTB + QT_FIXTURE_UBTB_MAIN/MOUNT (see header).");
        return;
    };
    let runs: Vec<Value> = std::fs::read_to_string(&oracle)
        .expect("read oracle")
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("parse oracle line"))
        .collect();
    assert_eq!(runs.len(), 3, "first / steady / emptied");

    let pepper = test_pepper();
    let scratch = std::env::temp_dir().join(format!("qt-ubtb-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    let (wm, wx) = (scratch.join("main.db"), scratch.join("mount.db"));
    std::fs::copy(&fix_main, &wm).unwrap();
    std::fs::copy(&fix_mount, &wx).unwrap();
    let main = Writer::open_writable(&wm, &pepper).expect("open main");
    let mount = Writer::open_writable(&wx, &pepper).expect("open mount");

    let mut red = Vec::new();
    for want_run in &runs {
        let run = want_run["run"].as_str().unwrap();
        if run == "emptied" {
            mount
                .connection()
                .execute(
                    "DELETE FROM doc_mount_file_links WHERE relativePath = 'uploads/gone.md'",
                    [],
                )
                .unwrap();
        }
        let (result, lines) = global_capture::capture(|| {
            backfill_upload_blob_text(
                main.connection(),
                mount.connection(),
                &RefusingTextExtractor,
            )
        });

        // The result tallies (v4's minus the carve row).
        let got_result = serde_json::to_value(result).unwrap();
        let want_result = carve_counts(run, want_run["result"].clone());
        if got_result != want_result {
            red.push(format!("{run}: result {got_result} vs {want_result}"));
        }

        // The lines: v5's own skip INFO first (asserted, then dropped), then v4's.
        let got_lines: Vec<String> = lines
            .into_iter()
            .filter(|l| l.contains(" quilltap::startup ") || l.starts_with("WARN quilltap::db"))
            .collect();
        assert_eq!(
            got_lines.first().map(String::as_str),
            Some(V5_SKIP_LINE),
            "{run}: v5's R-1 INFO must lead every run while the PDF row stays"
        );
        let got_lines = got_lines[1..].to_vec();
        let want_lines: Vec<String> = want_run["log"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| {
                let mut l = l.clone();
                if run == "first" && l["fields"].get("candidates").is_some() {
                    let fields = l["fields"].clone();
                    l["fields"] = carve_counts(run, fields);
                }
                render_v4(&l)
            })
            .collect();
        if got_lines != want_lines {
            red.push(format!(
                "{run}: lines\n  rust: {got_lines:#?}\n  v4:   {want_lines:#?}"
            ));
        }

        // The tables (the carve row lifted after both shapes are asserted).
        let mut got_tables = dump(main.connection(), mount.connection());
        let mut want_tables = want_run["tables"].clone();
        carve_tables(run, &mut got_tables, &mut want_tables);
        let (g, w) = (
            norm(&json!({ "tables": got_tables })),
            norm(&json!({ "tables": want_tables })),
        );
        if g != w {
            red.push(format!("{run}: tables\n{}", diff_first_line(&g, &w)));
        }
    }
    drop(main);
    drop(mount);
    let _ = std::fs::remove_dir_all(&scratch);
    assert!(red.is_empty(), "{} red:\n{}", red.len(), red.join("\n"));
    eprintln!(
        "OK: upload-blob-text backfill matched oracle (3 runs; the R-1 carve asserted both ways)."
    );
}
