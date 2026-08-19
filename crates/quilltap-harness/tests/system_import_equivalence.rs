//! P4.9G4 `.qtap` IMPORT-READ differential: replays the oracle's OWN `.qtap`
//! bytes (emitted as `qtapBase64`, so both sides provably read identical input)
//! through the Rust reader (`services::quilltap_import::ndjson::
//! load_qtap_from_upload`) and the Rust `preview_import`, diffing:
//!
//!   - the assembled `{manifest, data}` — **exact**, key order included
//!   - the `previewImport` body — **exact**, key order included
//!   - the reader's throwing arms — v4's error strings, verbatim
//!
//! Nothing here writes: the preview is v4's count-only pass. The import EXECUTE
//! half lives in `system_import_state`.
//!
//! ## The sparse-array blob divergence — DISCHARGED (P4.d22, 2026-07-26)
//!
//! This family used to carry one ruled divergence. v4's
//! `assembleExportFromStream` asked `accum.received.every(...)` over a pre-sized
//! SPARSE array, and `Array.prototype.every` skips holes — so a blob finalized on
//! its FIRST chunk however many were outstanding, was truncated by `join('')`,
//! and every later chunk hit "received without preceding doc_mount_blob". v4
//! could not re-import a document-store blob larger than its own 3 MB
//! `BLOB_CHUNK_BYTES`. v5's reader waited for every slot instead (reader-only —
//! the writer stays byte-identical), which was **ruled** a deliberate divergence
//! on 2026-07-24 and asserted in both directions so an upstream fix could not
//! pass unnoticed.
//!
//! **v4 fixed it in `c1507f47`** (`lib/import/quilltap-import-stream.ts:284` now
//! counts arrivals against `chunkCount`). The tripwire fired, the divergence list
//! is gone, and both arms are plain equalities:
//!
//! - `throw_ndjson_truncated_blob` — a genuinely short stream. Both engines now
//!   reach v4's own end-of-stream check, and the message is compared **byte for
//!   byte** (`mask_parser_text` is a no-op on it): `NDJSON export truncated:
//!   1 blob(s) missing chunks — [{"sha256":"s","received":1,"expected":2}]`.
//!   That string matters beyond the diff — the preview route leaks
//!   `error.message` to the client.
//! - `read_ndjson_multi_chunk_blob` — a COMPLETE two-chunk blob, added by this
//!   lane. The corpus had no case for the arm the fix actually unblocked: the
//!   short-stream case only ever proved where the error landed, never that the
//!   round trip works. Both sides now assemble `dataBase64: "AAAABBBB"`.
//!
//! ## [P4.48] The planted read-failure arms — one equality, one divergence
//!
//! The preview's existence checks used to swallow repository read errors to
//! `exists: false`. Two arms plant a failure IN THE DATABASE, on a per-case
//! fixture COPY (the committed bytes stay read-only), and hold each leg to the
//! disposition MEASUREMENT earned — the order's premise that v4 propagates at
//! all of them is refuted, by v4's own recorded output:
//!
//! - `preview_planted_unavailable_project_store` — **EQUALITY**. `PROJECT_1`'s
//!   `officialMountPointId` is nulled, so the slim read succeeds and
//!   `applyOverlayOne` throws. That call is NOT wrapped in a `safeQuery` and
//!   `previewImport` carries no `try`, so v4's whole preview dies too. The
//!   refusal message is compared BYTE FOR BYTE (`Project … has no usable
//!   document store (officialMountPointId=null): officialMountPointId is
//!   null`), which also keeps P4.D51's per-entity wording convergence honest.
//! - `preview_planted_unreadable_tags_table` — **DIVERGENCE**, pinned in BOTH
//!   directions. `DROP TABLE tags`, then a tags payload. v4 reports a confident
//!   `exists: false`; v5 refuses, under the standing 2026-08-03 ruling that this
//!   family fixes v4's bugs rather than reproducing them.
//!
//!   **[P4.D91] The mechanism, re-measured — the P4.48 reading was wrong.** It
//!   is not `safeQuery`'s fallback swallowing a read error: v4's
//!   `getCollection()` calls `ensureCollection` on first access, whose
//!   `CREATE TABLE IF NOT EXISTS` puts the dropped table straight back, so the
//!   read succeeds against a table v4 has just built. That is why v4's bug-79
//!   strict scope (`275cd7bc`), which was supposed to stop exactly this class
//!   of silence, left the arm unchanged — there is no error here to propagate.
//!   The oracle now emits `plantProbe.tags` (does the table exist after the
//!   preview?) so the claim is a comparand rather than prose, and the arm
//!   asserts the asymmetry on BOTH engines: v4 ends with the table back, v5
//!   ends without it, having provisioned nothing on a read path.
//!
//!   Both tripwires stay live: if v4 stops answering `exists:false` the arm says
//!   re-measure, and if v5 stops refusing it says either P4.48 regressed or v4
//!   converged.
//!
//! Mutation-proven: reverting the preview's projects site reddens the first,
//! reverting its tags site reddens the second.
//!
//! Generate the oracle (see the .test.ts header), then run:
//!   QT_ORACLE_SYSTEM_IMPORT=/tmp/oracle-system-import.ndjson \
//!     cargo test -p quilltap-harness --test system_import_equivalence -- --nocapture

use std::path::PathBuf;

use base64::Engine;
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::services::quilltap_import::ndjson::load_qtap_from_upload;
use quilltap_core::services::quilltap_import::preview::preview_import;
use quilltap_core::services::quilltap_import::QuilltapExport;
use serde_json::{Map, Value};

const TEST_PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../quilltap-web/tests/fixtures")
}

fn fresh_db(tag: &str) -> Db {
    let scratch = std::env::temp_dir().join(format!("qt-sysimport-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    let main = scratch.join("main.db");
    let mount = scratch.join("mount.db");
    std::fs::copy(fixtures_dir().join("system-data-main.db"), &main).unwrap();
    std::fs::copy(fixtures_dir().join("system-data-mount.db"), &mount).unwrap();
    Db::open(
        DbPaths {
            main,
            mount_index: Some(mount),
            llm_logs: None,
        },
        TEST_PEPPER,
    )
    .expect("open db")
}

/// [P4.48] The in-database failures the planted-preview arms stage. Both run on
/// a per-case fixture COPY — the committed bytes are read-only — and each must
/// produce a read ERROR rather than an empty result, which is what the oracle's
/// own recorded outcome proves for the v4 side.
#[derive(Clone, Copy)]
enum Plant {
    /// The table the preview is about to consult is simply gone. v4 swallows
    /// this (`safeQuery(…, null)`); v5 refuses.
    DropTagsTable,
    /// The slim row survives but its document store does not, so the read
    /// succeeds and the OVERLAY throws. v4 throws here too.
    OrphanProjectStore,
}

/// `PROJECT_1` from `build-system-data-fixture.ts` — deterministic in the
/// committed fixture, so both engines address the same row.
const FIXTURE_PROJECT_ID: &str = "a3000000-0000-4000-8000-000000000001";

fn apply_plant(db: &Db, plant: Plant) -> Result<(), String> {
    db.write_blocking(move |ws| {
        let main = ws.main().connection();
        match plant {
            Plant::DropTagsTable => {
                main.execute_batch("DROP TABLE tags")?;
            }
            Plant::OrphanProjectStore => {
                let touched = main.execute(
                    "UPDATE projects SET officialMountPointId = NULL WHERE id = ?1",
                    [FIXTURE_PROJECT_ID],
                )?;
                // A plant that touches nothing is a silently green arm — the
                // `planted-fs-conditions-in-differentials` trap in its
                // in-database form.
                assert_eq!(
                    touched, 1,
                    "the fixture no longer carries project {FIXTURE_PROJECT_ID}; \
                     the plant would be a no-op and the arm vacuous"
                );
            }
        }
        Ok(())
    })
    .map_err(|e| e.to_string())
}

fn decode(v: &Value) -> Vec<u8> {
    base64::engine::general_purpose::STANDARD
        .decode(v.as_str().expect("qtapBase64 is a string"))
        .expect("qtapBase64 decodes")
}

/// The assembled export as one `{manifest, data}` object, so the diff covers
/// both halves and their key order in a single comparison.
fn as_value(export: &QuilltapExport) -> Value {
    let mut m = Map::new();
    m.insert("manifest".into(), export.manifest.clone());
    m.insert("data".into(), export.data.clone());
    Value::Object(m)
}

/// Mask the JSON-ENGINE's own parse message inside v4's wrapper. v4 embeds V8's
/// text (`Unexpected token 'o', "not json" is not valid JSON`) where serde emits
/// its own (`expected ident at line 1 column 2`) — the standing
/// "`is not valid JSON:` wording seam", out of contract for every ported
/// surface. v4's WRAPPER (`Invalid NDJSON on line N: … (near: X)` /
/// `Invalid JSON: …`) is fully under test; only the engine's sentence is masked.
fn mask_parser_text(msg: &str) -> String {
    if let Some(rest) = msg.strip_prefix("Invalid JSON: ") {
        let _ = rest;
        return "Invalid JSON: <PARSER>".to_string();
    }
    if let Some(head) = msg.find("Invalid NDJSON on ") {
        // Keep everything up to the first `: ` after the line number, and the
        // trailing ` (near: …)` excerpt — mask only what lies between.
        let after = &msg[head..];
        if let (Some(colon), Some(near)) = (after.find(": "), after.rfind(" (near: ")) {
            if colon + 2 <= near {
                return format!("{}: <PARSER>{}", &after[..colon], &after[near..]);
            }
        }
    }
    msg.to_string()
}

#[test]
fn system_import_read_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_SYSTEM_IMPORT") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_SYSTEM_IMPORT to the oracle NDJSON (see test header).");
            return;
        }
    };
    let lines: Vec<Value> = std::fs::read_to_string(&oracle_path)
        .unwrap_or_else(|e| panic!("read oracle: {e}"))
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str::<Value>(l).expect("parse oracle line"))
        .collect();

    let meta = lines
        .iter()
        .find(|l| l["name"] == "_meta")
        .expect("oracle carries _meta");
    let user = meta["userId"].as_str().unwrap().to_string();

    let db = fresh_db("reads");
    let mut failed: Vec<String> = Vec::new();
    let mut ran = 0usize;

    for exp in lines.iter().filter(|l| l["name"] != "_meta") {
        let name = exp["name"].as_str().unwrap();
        match exp["kind"].as_str().unwrap_or("") {
            // Export bytes → reader → preview, both diffed exactly.
            "roundtrip" | "read" | "preview" => {
                let export = if exp["kind"] == "preview" {
                    // The hand-built payload arm: no `.qtap` bytes, just the object.
                    let p = &exp["payload"];
                    QuilltapExport {
                        manifest: p["manifest"].clone(),
                        data: p["data"].clone(),
                    }
                } else {
                    match load_qtap_from_upload(&decode(&exp["qtapBase64"])) {
                        Ok(e) => e,
                        Err(msg) => {
                            failed.push(format!("{name}: rust reader threw: {msg}"));
                            continue;
                        }
                    }
                };

                if let Some(expected) = exp.get("assembled").filter(|v| !v.is_null()) {
                    let got = as_value(&export);
                    let mut want = Map::new();
                    want.insert("manifest".into(), expected["manifest"].clone());
                    want.insert("data".into(), expected["data"].clone());
                    let want = Value::Object(want);
                    if got != want {
                        failed.push(format!(
                            "{name}: assembled export differs\n  rust:   {got}\n  oracle: {want}"
                        ));
                        continue;
                    }
                }

                if let Some(expected) = exp.get("preview").filter(|v| !v.is_null()) {
                    let got = db
                        .read_main(|main| {
                            db.read_mount_index(|mount| preview_import(main, mount, &user, &export))
                        })
                        .expect("preview read");
                    if got != *expected {
                        failed.push(format!(
                            "{name}: preview differs\n  rust:   {got}\n  oracle: {expected}"
                        ));
                        continue;
                    }
                }
            }
            "throw" => {
                let got = load_qtap_from_upload(&decode(&exp["qtapBase64"]));
                match exp["error"].as_str() {
                    // A `null` error means v4 did NOT throw where the case says it
                    // should. There is no longer any such case: the one that used
                    // to land here — the short multi-chunk blob — converged when v4
                    // fixed the sparse-array `every` in `c1507f47` (see the header).
                    // Kept as a live tripwire in the other direction: if v4 ever
                    // stops throwing on a malformed input again, this says so
                    // instead of quietly tolerating it.
                    None => {
                        failed.push(format!(
                            "{name}: the oracle did NOT throw on a `throw` case — v4 has \
                             regressed to accepting malformed input, or the case needs \
                             re-classifying. There is no recorded divergence here any more."
                        ));
                        continue;
                    }
                    Some(expected) => match got {
                        Err(msg) if mask_parser_text(&msg) == mask_parser_text(expected) => {}
                        other => {
                            failed.push(format!(
                                "{name}: expected throw {expected:?}, got {other:?}"
                            ));
                            continue;
                        }
                    },
                }
            }
            // [P4.48] A read failure planted IN THE DATABASE, on a per-case
            // fixture COPY. The oracle records whether v4 threw; this side
            // classifies from that record rather than from an assumption, and
            // holds each arm to the disposition the measurement earned.
            "preview_planted" => {
                let plant = match name {
                    "preview_planted_unreadable_tags_table" => Plant::DropTagsTable,
                    "preview_planted_unavailable_project_store" => Plant::OrphanProjectStore,
                    other => {
                        failed.push(format!("{other}: no Rust plant defined for this case"));
                        continue;
                    }
                };
                let planted = fresh_db(&format!("planted-{ran}"));
                if let Err(e) = apply_plant(&planted, plant) {
                    failed.push(format!("{name}: could not apply the plant: {e}"));
                    continue;
                }
                let p = &exp["payload"];
                let export = QuilltapExport {
                    manifest: p["manifest"].clone(),
                    data: p["data"].clone(),
                };
                let got = planted.read_main(|main| {
                    planted.read_mount_index(|mount| preview_import(main, mount, &user, &export))
                });

                match (exp["error"].as_str(), &got) {
                    // ── the EQUALITY leg ────────────────────────────────────
                    // v4 threw. `applyOverlayOne` is not wrapped in a
                    // `safeQuery` and `previewImport` has no `try`, so an
                    // unavailable store sinks the preview on both sides. The
                    // message is compared BYTE FOR BYTE — v5's single
                    // `OverlayError::Unavailable` Display was converged onto
                    // v4's per-entity wording by P4.D51, and this is the arm
                    // that keeps it honest.
                    (Some(expected), Err(e)) => {
                        let actual = e.to_string();
                        if actual != expected {
                            failed.push(format!(
                                "{name}: refusal message differs\n  rust:   {actual}\n  \
                                 oracle: {expected}"
                            ));
                            continue;
                        }
                    }
                    (Some(expected), Ok(v)) => {
                        failed.push(format!(
                            "{name}: v4 refused with {expected:?} but v5 answered a \
                             preview — the propagation fix has regressed at this \
                             site.\n  rust: {v}"
                        ));
                        continue;
                    }
                    // ── the DIVERGENCE leg, pinned in BOTH directions ───────
                    // v4 did NOT throw: its `_findById` is a
                    // `safeQuery(…, null)`, so an unreadable table reads as
                    // "the id is free" and the preview reports a confident
                    // `exists: false` the instance cannot support. v5 refuses
                    // instead, under the standing 2026-08-03 ruling that this
                    // family FIXES v4's bugs rather than reproducing them.
                    (None, Err(_)) => {
                        // v5 refuses (correct) and v4 answers anyway — the
                        // recorded divergence. Assert v4's answer is still
                        // exactly what we recorded, so a partial upstream fix
                        // cannot pass unnoticed.
                        let v4_exists = exp["preview"]["entities"]["tags"][0]["exists"].as_bool();
                        if v4_exists != Some(false) {
                            failed.push(format!(
                                "{name}: v4 no longer reports `exists:false` for a \
                                 dropped table — re-measure before trusting this \
                                 divergence.\n  oracle: {}",
                                exp["preview"]
                            ));
                            continue;
                        }
                        // [P4.D91] …and assert WHY, on both engines. v4 answers
                        // because `ensureCollection` rebuilt the table under it;
                        // v5 provisions nothing on a read, so its copy is still
                        // missing the table it refused over. If either half
                        // moves, the divergence needs re-measuring rather than
                        // re-asserting.
                        if exp["plantProbe"]["tags"].as_bool() != Some(true) {
                            failed.push(format!(
                                "{name}: v4 no longer re-creates the dropped table on \
                                 first repository access — the recorded mechanism for \
                                 this divergence has changed; re-measure.\n  \
                                 oracle plantProbe: {}",
                                exp["plantProbe"]
                            ));
                            continue;
                        }
                        let v5_has_tags = planted
                            .read_main(|c| {
                                Ok(c.query_row(
                                    "SELECT COUNT(*) FROM sqlite_master \
                                     WHERE type = 'table' AND name = 'tags'",
                                    [],
                                    |r| r.get::<_, i64>(0),
                                )?)
                            })
                            .expect("probe v5 sqlite_master");
                        if v5_has_tags != 0 {
                            failed.push(format!(
                                "{name}: v5 put the dropped table BACK — a read path \
                                 must provision nothing (that is what makes the \
                                 refusal meaningful)."
                            ));
                            continue;
                        }
                    }
                    (None, Ok(v)) => {
                        // BOTH sides swallowed. Either v5 regressed, or v4
                        // converged and this divergence is ready to retire —
                        // the tripwire that makes the pin two-directional.
                        failed.push(format!(
                            "{name}: v5 did NOT refuse a planted read failure. Either \
                             the P4.48 propagation regressed, or v4 has converged and \
                             this arm should become a plain equality — measure before \
                             deciding.\n  rust: {v}"
                        ));
                        continue;
                    }
                }
            }
            other => {
                failed.push(format!("{name}: unknown oracle kind {other}"));
                continue;
            }
        }
        ran += 1;
        eprintln!("OK {name}");
    }

    assert!(
        failed.is_empty(),
        "{} case(s) failed:\n{}",
        failed.len(),
        failed.join("\n\n")
    );
    // 19 from P4.9G4 + `read_ndjson_multi_chunk_blob` (P4.d22) + the two
    // planted read-failure arms (P4.48).
    assert_eq!(ran, 29, "expected 29 cases to run, ran {ran}");
}
