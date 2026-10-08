//! P4.9G4 `.qtap` EXPORT differential: direct-drives the Rust
//! `services::qtap_export::*` over a FRESH copy of the committed
//! `system-data-*` fixture family per case and diffs the emitted NDJSON
//! **line by line** against v4's REAL `createNdjsonStream` (the `system-export`
//! oracle), plus `previewExport` and the `?action=export-entities` listing.
//!
//! ── THE DIFF IS BYTE-EXACT, WITH TWO WALL-CLOCK EXCEPTIONS ───────────────────
//! `manifest.createdAt` is wall-clock, so the Rust side is handed the ORACLE's
//! own `createdAt` for that case (parsed off the oracle envelope). The second is
//! `character.data.physicalDescription.{createdAt,updatedAt}`: a character with
//! no stored physical description gets a DEFAULT one MINTED at read time by the
//! vault overlay on both sides (its `id` is derived and so does match) — those
//! two timestamps are normalized in place, which `preserve_order` keeps at their
//! original key positions.
//!
//! Every other byte — `appVersion` (the oracle emits v4's `package.json` version
//! in `_meta`, which the Rust side is handed verbatim), every key order, and the
//! whole line sequence — must match exactly.
//!
//! Generate the oracle (see the .test.ts header), then run:
//!   QT_ORACLE_SYSTEM_EXPORT=/tmp/oracle-system-export.ndjson \
//!     cargo test -p quilltap-harness --test system_export_equivalence -- --nocapture

use std::collections::HashMap;
use std::path::PathBuf;

use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::services::qtap_export::{self, ExportError, ExportOptions};
use serde_json::Value;

const TEST_PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../quilltap-web/tests/fixtures")
}

/// A fixture copy in a scratch dir removed on drop (the `Db` drops first).
struct ScratchDb {
    db: Db,
    _dir: tempfile::TempDir,
}

impl std::ops::Deref for ScratchDb {
    type Target = Db;
    fn deref(&self) -> &Db {
        &self.db
    }
}

fn fresh_db(tag: &str) -> ScratchDb {
    let scratch = tempfile::Builder::new()
        .prefix(&format!("qt-sysexport-{tag}-"))
        .tempdir()
        .expect("tempdir");
    let main = scratch.path().join("main.db");
    let mount = scratch.path().join("mount.db");
    std::fs::copy(fixtures_dir().join("system-data-main.db"), &main).unwrap();
    std::fs::copy(fixtures_dir().join("system-data-mount.db"), &mount).unwrap();
    // P4.111 widened the committed `system-data-{main,mount}.db` to carry
    // the P4.D171/P4.D182 columns natively (`pragma_table_info` proof:
    // P4.117 lane record) — the `ensure_*_column[s]` heals that used to run
    // here are dead and removed; the value PLANTS stay (the non-default
    // cells this family exists to compare).
    {
        let w = quilltap_core::db::Writer::open_writable(&main, TEST_PEPPER).unwrap();
        plant_p4d171_values(w.connection());
        plant_p4d182_values(w.connection());
        // [P4.D264] the oracle's `plantWardrobe`, for the `…_wardrobe` cases.
        if tag.ends_with("_wardrobe") {
            let m = quilltap_core::db::Writer::open_writable(&mount, TEST_PEPPER).unwrap();
            plant_wardrobe(w.connection(), m.connection());
        }
    }
    let db = Db::open(
        DbPaths {
            main,
            mount_index: Some(mount),
            llm_logs: None,
        },
        TEST_PEPPER,
    )
    .expect("open db");
    ScratchDb { db, _dir: scratch }
}

fn options_from(v: &Value) -> ExportOptions {
    ExportOptions {
        entity_type: v["type"].as_str().unwrap_or_default().to_string(),
        scope: v["scope"].as_str().unwrap_or("all").to_string(),
        selected_ids: v["selectedIds"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|s| s.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default(),
        include_memories: v["includeMemories"].as_bool().unwrap_or(false),
    }
}

/// The `createdAt` v4 stamped on this case's envelope — injected into the Rust
/// writer so the wall-clock field can't be the only difference.
fn oracle_created_at(lines: &[Value]) -> String {
    lines
        .first()
        .and_then(|l| l.pointer("/manifest/createdAt"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

#[test]
fn system_export_matches_oracle() {
    let oracle_path = match std::env::var("QT_ORACLE_SYSTEM_EXPORT") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_SYSTEM_EXPORT to the oracle NDJSON (see test header).");
            return;
        }
    };
    let mut oracle: HashMap<String, Value> = HashMap::new();
    let mut order: Vec<String> = Vec::new();
    for line in std::fs::read_to_string(&oracle_path)
        .unwrap_or_else(|e| panic!("read oracle: {e}"))
        .lines()
        .filter(|l| !l.trim().is_empty())
    {
        let v: Value = serde_json::from_str(line).expect("parse oracle line");
        let name = v["name"].as_str().unwrap().to_string();
        order.push(name.clone());
        oracle.insert(name, v);
    }

    let meta = oracle.get("_meta").expect("oracle carries _meta");
    let app_version = meta["appVersion"].as_str().unwrap().to_string();
    let user = meta["userId"].as_str().unwrap().to_string();

    let plain = fresh_db("reads");
    // [P4.D264] the `…_wardrobe` cases read their own planted copy.
    let planted = fresh_db("reads_wardrobe");
    let mut failed: Vec<String> = Vec::new();
    let mut ran = 0usize;
    let mut wardrobe_cases = 0usize;

    for name in order.iter().filter(|n| n.as_str() != "_meta") {
        let exp = &oracle[name];
        let db = if name.ends_with("_wardrobe") {
            wardrobe_cases += 1;
            &planted
        } else {
            &plain
        };
        match exp["kind"].as_str().unwrap_or("") {
            "ndjson" => {
                let opts = options_from(&exp["options"]);
                let exp_lines: Vec<Value> = exp["lines"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|l| serde_json::from_str::<Value>(l.as_str().unwrap()).unwrap())
                    .collect();
                let created_at = oracle_created_at(&exp_lines);
                let got = db
                    .read_main(|main| {
                        db.read_mount_index(|mount| {
                            Ok(qtap_export::stream_export_records(
                                main,
                                mount,
                                None,
                                &user,
                                &opts,
                                false,
                                &created_at,
                                &app_version,
                            ))
                        })
                    })
                    .expect("read");
                match got {
                    Ok(records) => {
                        if let Some(msg) =
                            diff_lines(name, &records, exp["lines"].as_array().unwrap())
                        {
                            failed.push(msg);
                            continue;
                        }
                    }
                    Err(e) => {
                        failed.push(format!(
                            "{name}: rust threw {e} but the oracle emitted lines"
                        ));
                        continue;
                    }
                }
            }
            "preview" => {
                let opts = options_from(&exp["options"]);
                let got = db
                    .read_main(|main| {
                        db.read_mount_index(|mount| {
                            Ok(qtap_export::preview_export(main, mount, &user, &opts))
                        })
                    })
                    .expect("read");
                let Ok(body) = got else {
                    failed.push(format!(
                        "{name}: rust threw on a preview the oracle answered"
                    ));
                    continue;
                };
                if body != exp["body"] {
                    failed.push(format!(
                        "{name}: preview mismatch\n  rust:   {body}\n  oracle: {}",
                        exp["body"]
                    ));
                    continue;
                }
            }
            "entities" => {
                let entity_type = exp["type"].as_str().unwrap();
                let got = db
                    .read_main(|main| {
                        db.read_mount_index(|mount| {
                            Ok(qtap_export::export_entities(
                                main,
                                mount,
                                &user,
                                entity_type,
                            ))
                        })
                    })
                    .expect("read");
                let (status, body) = match got {
                    Ok(v) => (200u16, v),
                    Err(ExportError::UnknownType(t)) => (
                        400,
                        serde_json::json!({
                            "error": qtap_export::unknown_entity_type_message(&t)
                        }),
                    ),
                    Err(e) => {
                        failed.push(format!("{name}: rust errored: {e}"));
                        continue;
                    }
                };
                let exp_status = exp["status"].as_u64().unwrap() as u16;
                if status != exp_status {
                    failed.push(format!("{name}: status {status} != oracle {exp_status}"));
                    continue;
                }
                if body != exp["body"] {
                    failed.push(format!(
                        "{name}: body mismatch\n  rust:   {body}\n  oracle: {}",
                        exp["body"]
                    ));
                    continue;
                }
            }
            "throw" => {
                let expected = exp["error"].as_str().unwrap_or("");
                // Both throw cases use the same unknown type; drive whichever
                // surface the case name names.
                let opts = ExportOptions {
                    entity_type: "nonsense".into(),
                    scope: "all".into(),
                    selected_ids: vec![],
                    include_memories: false,
                };
                let got: Result<(), ExportError> = db
                    .read_main(|main| {
                        db.read_mount_index(|mount| {
                            Ok(if name.starts_with("preview_") {
                                qtap_export::preview_export(main, mount, &user, &opts).map(|_| ())
                            } else {
                                qtap_export::stream_export_records(
                                    main,
                                    mount,
                                    None,
                                    &user,
                                    &opts,
                                    false,
                                    "T",
                                    &app_version,
                                )
                                .map(|_| ())
                            })
                        })
                    })
                    .expect("read");
                match got {
                    Err(e) if e.to_string() == expected => {}
                    other => {
                        failed.push(format!(
                            "{name}: expected throw {expected:?}, got {other:?}"
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
    // P4.D264: 57 + 4 = 61 (the three `…_wardrobe` streams + the preview).
    assert_eq!(ran, 61, "expected 61 cases to run, ran {ran}");
    assert_eq!(
        wardrobe_cases, 4,
        "the four P4.D264 wardrobe cases read the planted copy"
    );

    // [P4.D46, `7189a968`] The embedding strip is a MEASUREMENT, not a vacuous
    // pass: the fixture must carry at least one embedding-BEARING memory
    // (MEM_3), and no emitted memory line may carry the field. The per-line
    // diff above already proves byte-parity with v4's stripped output; this
    // guards the fixture itself so a rebuild that loses the vector can't turn
    // the strip arms green by absence.
    let bearing: i64 = plain
        .read_main(|main| {
            Ok(main
                .query_row(
                    "SELECT COUNT(*) FROM memories WHERE embedding IS NOT NULL",
                    [],
                    |r| r.get(0),
                )
                .unwrap_or(0))
        })
        .expect("count embedding-bearing memories");
    assert!(
        bearing > 0,
        "fixture carries no embedding-bearing memory — the strip arm is vacuous"
    );
    let strip_opts = ExportOptions {
        entity_type: "characters".into(),
        scope: "all".into(),
        selected_ids: vec![],
        include_memories: true,
    };
    let records = plain
        .read_main(|main| {
            plain.read_mount_index(|mount| {
                Ok(qtap_export::stream_export_records(
                    main,
                    mount,
                    None,
                    &user,
                    &strip_opts,
                    false,
                    "T",
                    &app_version,
                ))
            })
        })
        .expect("read")
        .expect("characters export streams");
    let mut memory_lines = 0usize;
    for r in &records {
        if r["kind"] == "memory" {
            memory_lines += 1;
            assert!(
                r["data"].get("embedding").is_none(),
                "a memory line carries `embedding` — the strip did not run: {r}"
            );
        }
    }
    assert!(memory_lines > 0, "no memory lines streamed at all");
}

/// Replace the vault-minted default `physicalDescription` timestamps (see the
/// header) with a sentinel. `preserve_order` keeps each key at its original
/// position, so the surrounding byte order is still under test.
fn normalize_minted(record: &mut Value) {
    let Some(pd) = record
        .pointer_mut("/data/physicalDescription")
        .and_then(Value::as_object_mut)
    else {
        return;
    };
    for k in ["createdAt", "updatedAt"] {
        if pd.contains_key(k) {
            pd.insert(k.to_string(), Value::String("<MINTED>".into()));
        }
    }
}

/// Byte-exact per-line diff (key order included). Reports the FIRST divergent
/// line with both renderings so a key-order slip is immediately visible.
fn diff_lines(name: &str, got: &[Value], exp: &[Value]) -> Option<String> {
    let got_lines: Vec<String> = got
        .iter()
        .map(|r| {
            let mut r = r.clone();
            normalize_minted(&mut r);
            r.to_string()
        })
        .collect();
    let exp_lines: Vec<String> = exp
        .iter()
        .map(|l| {
            let mut v: Value = serde_json::from_str(l.as_str().unwrap()).unwrap();
            normalize_minted(&mut v);
            v.to_string()
        })
        .collect();
    for (i, (g, e)) in got_lines.iter().zip(exp_lines.iter()).enumerate() {
        if g != e {
            return Some(format!(
                "{name}: line {i} differs\n  rust:   {g}\n  oracle: {e}"
            ));
        }
    }
    if got_lines.len() != exp_lines.len() {
        return Some(format!(
            "{name}: line count {} != oracle {}\n  rust tail:   {:?}\n  oracle tail: {:?}",
            got_lines.len(),
            exp_lines.len(),
            got_lines.get(exp_lines.len().min(got_lines.len())),
            exp_lines.get(got_lines.len().min(exp_lines.len())),
        ));
    }
    None
}

/// P4.88: the twin of the oracle's `plantP4d171Values` — NON-DEFAULT values in
/// the two `78b381a96` columns, so this family measures the carry rather than
/// two Zod defaults agreeing. The oracle plants the identical cells on v4's
/// copy, so both engines provably start from the same bytes.
fn plant_p4d171_values(conn: &rusqlite::Connection) {
    conn.execute(
        "UPDATE \"chats\" SET \"cycleOrderParticipantIds\" = ?1 WHERE \"id\" = ?2",
        rusqlite::params![
            r#"["e1000000-0000-4000-8000-000000000001","e1000000-0000-4000-8000-0000000000e2"]"#,
            "c1000000-0000-4000-8000-000000000001"
        ],
    )
    .expect("plant the drawn rotation");
    conn.execute(
        "UPDATE \"chat_messages\" SET \"routeTrail\" = ?1 WHERE \"id\" = ?2",
        rusqlite::params![
            r#"[{"profileId":"c0000001-0000-4000-8000-000000000001","profileName":"Primary","provider":"OPENAI","modelName":"gpt-4o","via":"primary","outcome":"failed","trigger":"rate-limit","detail":"429 slow down"},{"profileId":"c0000001-0000-4000-8000-0000000000f2","profileName":"Understudy","provider":"ANTHROPIC","modelName":"claude-x","via":"understudy","outcome":"answered"}]"#,
            "d1000000-0000-4000-8000-000000000002"
        ],
    )
    .expect("plant the route trail");
}

/// P4.D182: the twin of the oracle's `plantP4d182Values`. A NON-NULL
/// `files.generationKey` on one row (the others stay NULL, which proves the
/// omit-when-null rule at the same time) and a BUMPED `chats.transcriptVersion`
/// that must reach no export record at all.
///
/// The planted key's VALUE is a real UUID this archive remaps elsewhere
/// (`PROJECT_1`'s id), chosen on purpose: the cache key travels AS-IS, so a
/// remap-shaped value surviving unchanged is the only way to tell "carried"
/// from "carried and rewritten".
fn plant_p4d182_values(conn: &rusqlite::Connection) {
    let touched = conn
        .execute(
            "UPDATE \"files\" SET \"generationKey\" = ?1 WHERE \"id\" = ?2",
            rusqlite::params![P4D182_GENERATION_KEY, P4D182_FILE_ID],
        )
        .expect("plant the avatar cache key");
    assert_eq!(touched, 1, "the planted file row must exist in the fixture");
    let touched = conn
        .execute(
            "UPDATE \"chats\" SET \"transcriptVersion\" = 7 WHERE \"id\" = ?1",
            rusqlite::params!["c1000000-0000-4000-8000-000000000001"],
        )
        .expect("plant the transcript counter");
    assert_eq!(touched, 1, "the planted chat row must exist in the fixture");
}

/// The planted cache key — deliberately `PROJECT_1`'s own id (see
/// [`plant_p4d182_values`]).
const P4D182_GENERATION_KEY: &str = "a3000000-0000-4000-8000-000000000001";
const P4D182_FILE_ID: &str = "f0000001-0000-4000-8000-000000000001";

// === P4.D264 (append-only) ===

/// [P4.D264] The twin of the oracle's `plantWardrobe`, statement for
/// statement (no write clock reaches the export bytes): the ledger table
/// through C1 §6's helper (the migration's DDL, as the oracle runs v4's
/// `WARDROBE_WEAR_STATS_DDL`), the oracle's three rows verbatim, `portrait.png`
/// linked to the coat, and the coat's frontmatter archived + pointing at it.
fn plant_wardrobe(main: &rusqlite::Connection, mount: &rusqlite::Connection) {
    quilltap_core::test_support::ensure_wear_ledger_on(main);
    main.execute_batch(WEAR_PLANT_SQL)
        .expect("plant the wear rows");
    let linked = main
        .execute(
            "UPDATE \"files\" SET \"linkedTo\" = '[\"ac000000-0000-4000-8000-000000000001\"]' \
             WHERE \"id\" = ?1",
            ["f0000001-0000-4000-8000-000000000001"],
        )
        .expect("link portrait.png to the coat");
    assert_eq!(linked, 1, "plantWardrobe: portrait.png not found");
    let edited = mount
        .execute(
            "UPDATE \"doc_mount_documents\" SET \"content\" = replace(\"content\", ?1, ?2) \
             WHERE \"content\" LIKE '%title: Travelling Coat%'",
            [COAT_EDIT_FROM, COAT_EDIT_TO],
        )
        .expect("edit the coat's frontmatter");
    assert_eq!(edited, 1, "plantWardrobe: the coat document not found");
}

const COAT_EDIT_FROM: &str = "imagePrompt: brown coat\n";
const COAT_EDIT_TO: &str =
    "imagePrompt: brown coat\narchived: true\narchivedAt: 2026-03-04T00:00:00.000Z\n\
imageFileId: f0000001-0000-4000-8000-000000000001\n";

/// The oracle's `WEAR_PLANT_SQL`, byte for byte.
const WEAR_PLANT_SQL: &str = "INSERT INTO \"wardrobe_wear_stats\" (\"id\", \"itemId\", \"wearerCharacterId\", \"wearCount\", \
\"firstWornAt\", \"lastWornAt\", \"lastWornChatId\", \"createdAt\", \"updatedAt\") VALUES \
('3e0000e1-0000-4000-8000-0000000000e1', 'ac000000-0000-4000-8000-000000000001', \
'a1000000-0000-4000-8000-000000000001', 3, '2026-03-02T00:00:00.000Z', \
'2026-03-05T00:00:00.000Z', 'c1000000-0000-4000-8000-000000000001', \
'2026-03-02T00:00:00.000Z', '2026-03-05T00:00:00.000Z'), \
('3e0000e2-0000-4000-8000-0000000000e2', 'ac000000-0000-4000-8000-000000000001', NULL, 2, \
'2026-03-01T00:00:00.000Z', '2026-03-03T00:00:00.000Z', NULL, \
'2026-03-01T00:00:00.000Z', '2026-03-03T00:00:00.000Z'), \
('3e0000e3-0000-4000-8000-0000000000e3', 'ae0000e3-0000-4000-8000-0000000000e3', \
'a1000000-0000-4000-8000-000000000002', 9, '2026-03-01T00:00:00.000Z', \
'2026-03-09T00:00:00.000Z', NULL, '2026-03-01T00:00:00.000Z', '2026-03-09T00:00:00.000Z')";
// === end P4.D264 ===
