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
use serde_json::{json, Value};

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
        // [P4.D272 R-B] tier-MIGRATED like the oracle's copy (a no-op on the
        // pair P4.D265 widened in place), then the tier cells planted.
        quilltap_core::test_support::ensure_memory_tiers_columns(w.connection());
        plant_memory_tiers_values(w.connection());
        // [P4.D317] the outfit-history table on every copy — the oracle's
        // `plantChatWardrobeHistoryTable` (v4's REAL DDL), table only — but
        // the `…_no_history_table` case, which measures its absence; and the
        // `…_outfit_history` cases' rows (the oracle's `HISTORY_ROWS`).
        if !tag.ends_with("_no_history_table") {
            quilltap_core::test_support::ensure_chat_wardrobe_history_on(w.connection());
        }
        // [P4.D327] the two Inventory columns on EVERY copy (the oracle's
        // `plantInventoryColumns` — the history opt-out does not apply).
        quilltap_core::test_support::ensure_inventory_columns_on(w.connection());
        if tag.ends_with("_outfit_history") {
            plant_outfit_history(w.connection());
        }
        // [P4.D294] the oracle's `plantThrowingCursors`, for the
        // `…_cursor_throw` cases.
        // [P4.D294 Tier 2 #19] the oracle's `SCENARIO_TEXT_CELL_SQL`.
        if tag.ends_with("_scenario_text") {
            w.connection()
                .execute_batch(SCENARIO_TEXT_CELL_SQL)
                .expect("plant the scenario text cell");
        }
        // [P4.D327] the oracle's `INVENTORY_CELLS_SQL`, for the `…_inventory` cases.
        if tag.ends_with("_inventory") {
            for sql in INVENTORY_CELLS_SQL {
                assert_eq!(
                    w.connection()
                        .execute(sql, [])
                        .expect("plant an Inventory cell"),
                    1,
                    "{sql}"
                );
            }
        }
        if tag.ends_with("_cursor_throw") {
            w.connection()
                .execute_batch(&THROWING_CURSORS_SQL.join(";\n"))
                .expect("plant the throwing cursor table");
        }
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
    // [P4.D294] the `…_cursor_throw` cases read their own throwing copy.
    let throwing = fresh_db("reads_cursor_throw");
    // [P4.D294 Tier 2 #19] …and the `…_scenario_text` case its own.
    let scenario_text = fresh_db("reads_scenario_text");
    // [P4.D317] the `…_outfit_history` cases read a copy with history rows;
    // `…_no_history_table` one without the table.
    let history = fresh_db("reads_outfit_history");
    let no_history_table = fresh_db("reads_no_history_table");
    // [P4.D327] the `…_inventory` cases read a copy with the two cells set.
    let inventory = fresh_db("reads_inventory");
    let mut inventory_cases = 0usize;
    let mut history_cases = 0usize;
    let mut failed: Vec<String> = Vec::new();
    let mut ran = 0usize;
    let mut wardrobe_cases = 0usize;

    for name in order.iter().filter(|n| n.as_str() != "_meta") {
        let exp = &oracle[name];
        let db = if name.ends_with("_wardrobe") {
            wardrobe_cases += 1;
            &planted
        } else if name.ends_with("_cursor_throw") {
            &throwing
        } else if name.ends_with("_scenario_text") {
            &scenario_text
        } else if name.ends_with("_outfit_history") {
            history_cases += 1;
            &history
        } else if name.ends_with("_no_history_table") {
            history_cases += 1;
            &no_history_table
        } else if name.ends_with("_inventory") {
            inventory_cases += 1;
            &inventory
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
                // [P4.D294] captured: the `[ExtractionCursors]` lines (the
                // reads run on this thread — `with_conn`).
                let (got, lines) = quilltap_core::test_support::captured_with(|| {
                    db.read_main(|main| {
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
                    .expect("read")
                });
                assert_cursor_logs(name, exp, &v5_cursor_lines(&lines), &mut failed);
                assert_history_logs(name, exp, &lines, &mut failed);
                assert_find_by_id_logs(name, exp, &lines, &mut failed);
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
                // [P4.167 H-3] captured: an unreadable selected chat's two
                // lines (the reads run on this thread — `with_conn`).
                let (got, lines) = quilltap_core::test_support::captured_with(|| {
                    db.read_main(|main| {
                        db.read_mount_index(|mount| {
                            Ok(qtap_export::preview_export(main, mount, &user, &opts))
                        })
                    })
                    .expect("read")
                });
                assert_find_by_id_logs(name, exp, &lines, &mut failed);
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
    // P4.D294: 61 + 2 = 63 (the two `…_cursor_throw` streams) + 1 = 64 (the
    // `…_scenario_text` stream, Tier 2 #19).
    // P4.D317: 64 + 4 = 68 (three `…_outfit_history` cases + the
    // `…_no_history_table` stream).
    // P4.D327: 68 + 2 = 70 (the two `…_inventory` streams).
    // P4.167 H-3: 70 + 1 = 71 (`preview_chats_selected_one_scenario_text`).
    assert_eq!(ran, 71, "expected 71 cases to run, ran {ran}");
    assert_eq!(
        inventory_cases, 2,
        "the two P4.D327 Inventory cases read their own copy"
    );
    assert_eq!(
        wardrobe_cases, 4,
        "the four P4.D264 wardrobe cases read the planted copy"
    );
    assert_eq!(
        history_cases, 4,
        "the four P4.D317 outfit-history cases read their own copies"
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

    // [P4.D294] Non-vacuity of the cursor carriage: the byte diff above only
    // asks something if v4's own lines CARRY the keys — the one-chat export
    // (a per-chat read) and the all-chats export (one preloaded read) each put
    // CHAT_1's three cursor keys after every schema key, before `_tagNames`,
    // and the throwing copy's exports carry none.
    let chat_1_line = |case: &str| -> String {
        oracle[case]["lines"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(Value::as_str)
            .find(|l| {
                l.contains(r#""kind":"chat""#)
                    && l.contains(r#""id":"c1000000-0000-4000-8000-000000000001""#)
            })
            .unwrap_or_else(|| panic!("{case}: no CHAT_1 line"))
            .to_string()
    };
    let tail = r#""otherExtractionWatermarkMessageId":"d1000000-0000-4000-8000-000000000001","episodeExtractionWatermarkMessageId":"d1000000-0000-4000-8000-000000000002","turnExtractionWatermarkMessageId":"d1000000-0000-4000-8000-000000000001","_tagNames""#;
    for case in ["stream_chats_selected_one", "stream_chats_all"] {
        assert!(
            chat_1_line(case).contains(tail),
            "{case}: v4's CHAT_1 line lacks the cursor tail"
        );
    }
    for case in [
        "stream_chats_all_cursor_throw",
        "stream_chats_selected_cursor_throw",
    ] {
        assert!(
            !chat_1_line(case).contains("ExtractionWatermarkMessageId"),
            "{case}: the throwing copy must export no cursor key"
        );
    }
    let scenario = r#""projectId":"a3000000-0000-4000-8000-000000000001","scenarioSource":{"mountPointId":"e2000000-0000-4000-8000-0000000000f3","path":"Scenarios/Harbour.md","bodySha256":"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"},"scenarioNotes":"Bring the lantern.","totalPromptTokens""#;
    assert!(
        chat_1_line("stream_chats_selected_one").contains(scenario),
        "v4's CHAT_1 line carries the scenario pair in its schema slot"
    );
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

// === P4.D272 (append-only) ===

/// [P4.D272] The oracle's `MEMORY_TIERS_PLANT_SQL`, statement for statement:
/// `MEM_1` a CONSOLIDATED hot digest of `MEM_3`, `MEM_3` its cold superseded
/// member (with its cold clock), `CHAT_1`'s three extraction-cursor rows and
/// its chosen scenario (P4.D294), and raw rows for
/// the two new instance settings (Tier 2 item 13). Every export line otherwise
/// carries the tier columns at their defaults — the same bytes whether the
/// key order (`schema-key-order.json`) is right or not.
const MEMORY_TIERS_PLANT_SQL: [&str; 9] = [
    "UPDATE \"memories\" SET \"source\" = 'CONSOLIDATED', \"tier\" = 'hot', \"consolidatedFrom\" = '[\"ad000000-0000-4000-8000-000000000003\"]', \"consolidatedAt\" = '2026-01-01T00:00:00.000Z' WHERE \"id\" = 'ad000000-0000-4000-8000-000000000001'",
    "UPDATE \"memories\" SET \"tier\" = 'cold', \"supersededById\" = 'ad000000-0000-4000-8000-000000000001', \"consolidatedAt\" = '2026-01-01T00:00:00.000Z' WHERE \"id\" = 'ad000000-0000-4000-8000-000000000003'",
    // [P4.D294] `CHAT_1`'s three cursor ROWS (`ac6348c5f` moved them off the
    // chat row), its chosen scenario (`545e299bb`), `MEM_3`'s cold clock.
    "INSERT INTO \"memory_extraction_cursors\" (\"id\", \"chatId\", \"pass\", \"messageId\", \"createdAt\", \"updatedAt\") VALUES ('c1000000-0000-4000-8000-000000000001:other-fold', 'c1000000-0000-4000-8000-000000000001', 'other-fold', 'd1000000-0000-4000-8000-000000000001', '2026-01-01T00:00:00.000Z', '2026-01-01T00:00:00.000Z')",
    "INSERT INTO \"memory_extraction_cursors\" (\"id\", \"chatId\", \"pass\", \"messageId\", \"createdAt\", \"updatedAt\") VALUES ('c1000000-0000-4000-8000-000000000001:episode-fold', 'c1000000-0000-4000-8000-000000000001', 'episode-fold', 'd1000000-0000-4000-8000-000000000002', '2026-01-01T00:00:00.000Z', '2026-01-01T00:00:00.000Z')",
    "INSERT INTO \"memory_extraction_cursors\" (\"id\", \"chatId\", \"pass\", \"messageId\", \"createdAt\", \"updatedAt\") VALUES ('c1000000-0000-4000-8000-000000000001:turn', 'c1000000-0000-4000-8000-000000000001', 'turn', 'd1000000-0000-4000-8000-000000000001', '2026-01-01T00:00:00.000Z', '2026-01-01T00:00:00.000Z')",
    "UPDATE \"chats\" SET \"scenarioSource\" = '{\"mountPointId\":\"e2000000-0000-4000-8000-0000000000f3\",\"path\":\"Scenarios/Harbour.md\",\"bodySha256\":\"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\"}', \"scenarioNotes\" = 'Bring the lantern.' WHERE \"id\" = 'c1000000-0000-4000-8000-000000000001'",
    "UPDATE \"memories\" SET \"supersededAt\" = '2026-01-02T00:00:00.000Z' WHERE \"id\" = 'ad000000-0000-4000-8000-000000000003'",
    "INSERT INTO \"instance_settings\" (\"key\", \"value\") VALUES ('memoryConsolidation', '{\"enabled\":true,\"watermark\":25}')",
    "INSERT INTO \"instance_settings\" (\"key\", \"value\") VALUES ('memoryExtractionMode', '{\"otherPass\":\"fold\"}')",
];

/// Run [`MEMORY_TIERS_PLANT_SQL`], each statement touching exactly one row.
fn plant_memory_tiers_values(conn: &rusqlite::Connection) {
    for sql in MEMORY_TIERS_PLANT_SQL {
        let touched = conn.execute(sql, []).expect("plant the memory-tier cells");
        assert_eq!(
            touched, 1,
            "the memory-tier plant must touch one row: {sql}"
        );
    }
}

// === P4.D294 (append-only) ===

/// [P4.D294] The oracle's `THROWING_CURSORS_SQL`, statement for statement
/// (P4.D293's recipe — memory note `throwing-plant-must-fail-both-reads`): the
/// cursor table rebuilt with `messageId` a VIRTUAL `json("pass")` column added
/// AFTER the rows, so every read naming it throws `malformed JSON` — for the
/// chats that HAVE rows (`CHAT_1`'s three).
const THROWING_CURSORS_SQL: [&str; 5] = [
    r#"ALTER TABLE "memory_extraction_cursors" RENAME TO "qt_cursors_old""#,
    r#"CREATE TABLE "memory_extraction_cursors" ("id" TEXT PRIMARY KEY, "chatId" TEXT NOT NULL, "pass" TEXT NOT NULL, "createdAt" TEXT NOT NULL, "updatedAt" TEXT NOT NULL)"#,
    r#"INSERT INTO "memory_extraction_cursors" SELECT "id", "chatId", "pass", "createdAt", "updatedAt" FROM "qt_cursors_old""#,
    r#"DROP TABLE "qt_cursors_old""#,
    r#"ALTER TABLE "memory_extraction_cursors" ADD COLUMN "messageId" TEXT AS (json("pass")) VIRTUAL"#,
];

/// v5's `[ExtractionCursors]` lines from a captured stream, as `(level,
/// message, chatId)` — the bundle helpers' WARNs / DEBUG and the cursor
/// repository's own lines (P4.D288's — matched by TAG only, never by bytes,
/// §R.5's ownership rule).
fn v5_cursor_lines(lines: &[String]) -> Vec<String> {
    lines
        .iter()
        .filter(|l| l.contains("[ExtractionCursors]"))
        .cloned()
        .collect()
}

/// [P4.D317] The oracle's `HISTORY_ROWS`, cell for cell, through the keystone's
/// `upsert_raw_rows` (v4's `upsertRows` semantics — the hash recomputed).
fn plant_outfit_history(conn: &rusqlite::Connection) {
    const CHAT_1: &str = "c1000000-0000-4000-8000-000000000001";
    const LORIAN: &str = "a1000000-0000-4000-8000-000000000001";
    let empty = json!({"top": [], "bottom": [], "footwear": [], "accessories": [], "hair": []});
    let rows = vec![
        json!({"id": "7e0000e1-0000-4000-8000-0000000000e1", "chatId": CHAT_1,
               "characterId": "a1000000-0000-4000-8000-000000000002", "entryIndex": 0,
               "slots": empty, "slotsHash": "", "source": "chat-start", "actorCharacterId": null,
               "actorToolName": null, "isCurrent": true, "createdAt": "2026-03-01T00:00:00.000Z"}),
        json!({"id": "7e0000e2-0000-4000-8000-0000000000e2", "chatId": CHAT_1,
               "characterId": LORIAN, "entryIndex": 1,
               "slots": {"top": ["ac000000-0000-4000-8000-000000000001"], "bottom": [],
                         "footwear": [], "accessories": [], "hair": []},
               "slotsHash": "", "source": "pascal", "actorCharacterId": LORIAN,
               "actorToolName": "dress_for_dinner", "isCurrent": true,
               "createdAt": "2026-03-03T00:00:00.000Z"}),
        json!({"id": "7e0000e3-0000-4000-8000-0000000000e3", "chatId": CHAT_1,
               "characterId": LORIAN, "entryIndex": 0, "slots": empty, "slotsHash": "",
               "source": "seeded", "actorCharacterId": null, "actorToolName": null,
               "isCurrent": false, "createdAt": "2026-03-02T00:00:00.000Z"}),
    ];
    let written =
        quilltap_core::db::chat_wardrobe_history::ChatWardrobeHistoryRepository::new(conn)
            .upsert_raw_rows(&rows);
    assert_eq!(written, rows.len(), "plant every history row");
}

/// [P4.D317] `ndjson-writer.ts`'s two outfit-history lines (v4 `7953da3a0`,
/// `:434` / `:436-440`), on EVERY stream case: `Exported outfit history`
/// per chat of a `chats` export (0 rows included), `Failed to load outfit
/// history for export` per chat when the read throws (the
/// `…_no_history_table` case), nothing on any other type. v4's line renders
/// `level message chatId=… count=…` (`module` dropped); v5's twin carries the
/// file's `chat_id` spelling (the inform line's), mapped back here.
/// [P4.167 H-3 — the human's ruling of 2026-10-27, CONVERGE] An unreadable
/// chat row (P4.D294 Tier 2 #19: a non-object TEXT `scenarioSource`) read
/// through v4's non-strict `findById` logs ERROR `Data validation failed
/// {collection, error}` then ERROR `Error finding entity by ID {collection,
/// id, error}` and answers `null`, so the export and the preview SKIP the
/// chat. This compares v5's two lines (`chats_read::find_by_id_dropping_
/// invalid`) with v4's on every stream and preview case — a silence leg
/// everywhere but the two `…_scenario_text` cases. It REPLACES the both-ways
/// pin that recorded v5's export failing on that row.
fn assert_find_by_id_logs(name: &str, oracle: &Value, v5: &[String], failed: &mut Vec<String>) {
    let Some(v4) = oracle["findByIdLogs"].as_array() else {
        failed.push(format!(
            "{name}: the oracle carries no `findByIdLogs` — regenerate it"
        ));
        return;
    };
    let v4: Vec<String> = v4.iter().map(|l| l.as_str().unwrap().to_string()).collect();
    let got: Vec<String> = v5
        .iter()
        .filter(|l| {
            l.contains(" Data validation failed") || l.contains(" Error finding entity by ID")
        })
        .map(|l| {
            // `LEVEL target message k=v …` → `level message k=v …`.
            let mut parts = l.splitn(3, ' ');
            let level = parts.next().unwrap_or("").to_lowercase();
            let _target = parts.next();
            format!("{level} {}", parts.next().unwrap_or(""))
        })
        .collect();
    if got != v4 {
        failed.push(format!(
            "{name}: unreadable-chat lines differ\n  rust:   {got:?}\n  oracle: {v4:?}"
        ));
    }
}

fn assert_history_logs(name: &str, oracle: &Value, v5: &[String], failed: &mut Vec<String>) {
    let Some(v4) = oracle["historyLogs"].as_array() else {
        failed.push(format!(
            "{name}: the oracle carries no `historyLogs` — regenerate it"
        ));
        return;
    };
    let v4: Vec<String> = v4.iter().map(|l| l.as_str().unwrap().to_string()).collect();
    let got: Vec<String> = v5
        .iter()
        .filter(|l| {
            l.contains(" Exported outfit history") || l.contains(" Failed to load outfit history")
        })
        .map(|l| {
            // `LEVEL target message k=v …` → `level message k=v …`.
            let mut parts = l.splitn(3, ' ');
            let level = parts.next().unwrap_or("").to_lowercase();
            let _target = parts.next();
            let rest = parts.next().unwrap_or("").replace(" chat_id=", " chatId=");
            format!("{level} {rest}")
        })
        .collect();
    if got != v4 {
        failed.push(format!(
            "{name}: outfit-history lines differ\n  rust:   {got:?}\n  oracle: {v4:?}"
        ));
    }
}

/// [P4.D294] The bundle-cursor read arms, pinned BOTH WAYS against the
/// recorded strict-scope divergence (`resolve_export_ids`' doc — the human's
/// 2026-10-02 ruling: v5 runs the export inside the strict-repository scope,
/// v4 non-strict). v4's two bundle reads sit on FALLBACK base reads, so its
/// export answers "no cursors" SILENTLY; v5's strict reads fail and its
/// bundle WARNs fire. Both export the same bytes (the diff above). So:
///
/// - v4 logs NO `[ExtractionCursors]` line on any case (a v4 move that starts
///   logging trips this);
/// - v5 logs NONE on every case but the two `…_cursor_throw` ones;
/// - `stream_chats_all_cursor_throw` (a multi-chat export): the table WARN
///   once, then the per-chat WARN for `CHAT_1` only (the one chat with rows —
///   a chat with no rows reads none and cannot throw);
/// - `stream_chats_selected_cursor_throw` (one chat): the per-chat WARN for
///   `CHAT_1` and NO table WARN (one chat never preloads, v4 `:349`).
fn assert_cursor_logs(name: &str, oracle: &Value, v5: &[String], failed: &mut Vec<String>) {
    const CHAT_1: &str = "c1000000-0000-4000-8000-000000000001";
    const TABLE_WARN: &str =
        "[ExtractionCursors] Could not read the cursor table for a bundle; reading per chat";
    const CHAT_WARN: &str =
        "[ExtractionCursors] Could not read a chat's cursors for a bundle; leaving them out";
    let v4_lines = oracle["cursorLogs"].as_array().cloned().unwrap_or_default();
    if oracle.get("cursorLogs").is_none() || !v4_lines.is_empty() {
        failed.push(format!(
            "{name}: v4's `cursorLogs` must be present and EMPTY (its bundle reads fall back \
             silently outside a strict scope); got {:?}",
            oracle.get("cursorLogs")
        ));
    }
    let warns = |needle: &str| {
        v5.iter()
            .filter(|l| l.starts_with("WARN") && l.contains(needle))
            .collect::<Vec<_>>()
    };
    let (table, chat) = (warns(TABLE_WARN), warns(CHAT_WARN));
    let want = match name {
        "stream_chats_all_cursor_throw" => (1, 1),
        "stream_chats_selected_cursor_throw" => (0, 1),
        _ => (0, 0),
    };
    if (table.len(), chat.len()) != want {
        failed.push(format!(
            "{name}: v5 bundle WARNs (table, per-chat) = ({}, {}), want {want:?}: {v5:?}",
            table.len(),
            chat.len()
        ));
    }
    for line in chat {
        if !line.contains(&format!("chatId={CHAT_1}"))
            || !line.contains("error=malformed JSON")
            || !line.contains("module=memory:extraction:cursor-bundle")
        {
            failed.push(format!(
                "{name}: the per-chat WARN must carry CHAT_1, the bare error and v4's module \
                 binding: {line}"
            ));
        }
    }
    if want == (0, 0) && !v5.is_empty() {
        failed.push(format!("{name}: silence leg broken — v5 logged {v5:?}"));
    }
}

/// [P4.D294 Tier 2 #19] The oracle's `SCENARIO_TEXT_CELL_SQL`: `CHAT_1`'s stored
/// `scenarioSource` a non-object TEXT cell (valid JSON — a string). v4's chats
/// read refuses the row (`ScenarioSourceSchema` wants an object), so its export
/// SKIPS the chat silently; v5's STRICT export (the 2026-10-02 ruling) FAILS
/// whole with the ZodError — MEASURED DIFFERENT and pinned both ways below
/// (P4.D294 item 19; flagged for the human: keep or converge).
/// [P4.D327] The oracle's `INVENTORY_CELLS_SQL`, statement for statement.
const INVENTORY_CELLS_SQL: [&str; 3] = [
    r#"UPDATE "chats" SET "equippedInventory" = '{"a1000000-0000-4000-8000-000000000001":["1e000000-0000-4000-8000-0000000000c1"]}' WHERE "id" = 'c1000000-0000-4000-8000-000000000001'"#,
    r#"UPDATE "characters" SET "canManageInventory" = 0 WHERE "id" = 'a1000000-0000-4000-8000-000000000001'"#,
    r#"UPDATE "characters" SET "canManageInventory" = 1 WHERE "id" = 'a1000000-0000-4000-8000-000000000002'"#,
];

const SCENARIO_TEXT_CELL_SQL: &str = r#"UPDATE "chats" SET "scenarioSource" = '"Scenarios/Harbour.md"' WHERE "id" = 'c1000000-0000-4000-8000-000000000001'"#;
