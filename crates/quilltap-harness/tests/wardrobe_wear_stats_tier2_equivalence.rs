//! P4.D255 — tier-2 differential: the wardrobe wear ledger's DATA layer
//! (`quilltap_core::db::wardrobe_wear_stats`) against v4's REAL
//! `WardrobeWearRepository` (`3ee3b1342`, minus the chokepoint).
//!
//! The oracle case (`harness/oracle/cases/wardrobe-wear-stats-tier2.ts`)
//! builds the base DB per run at the pin — the table through v4's REAL
//! `addWardrobeWearStatsTableMigration.run()` — writes it to
//! `QT_FIXTURE_WEAR_T2_DIR/base.db`, and runs the committed spec's ops
//! (`harness/oracle/fixtures/wardrobe-wear-stats-tier2.json`). This test runs
//! the same ops on a COPY of that base through v5's ledger and compares, per
//! op: the result (rows minus the minted `id` / `createdAt` / `updatedAt`; a
//! summaries Map as its ordered entries; a thrown message as `{threw}`), and
//! every log line — level, message and fields, v4's call context rendered as
//! v5's capture rig renders it. v4's logger maps onto v5 targets as:
//! the repository's `logger.child({ module: 'wardrobe-wear' })` lines →
//! `quilltap::wardrobe_wear` with `module=wardrobe-wear` leading; the
//! fallback `safeQuery` lines → `quilltap::db`. v4's backend `Raw query failed`
//! line is v5-unported by standing convention and is dropped from v4's side.
//!
//! **RULED DIVERGENCE `FIND_ALL_DROPS_NULLABLE`** (the human, 2026-10-08 — FIX
//! v5, file v4): v4's inherited `findAll()` drops every row carrying a NULL in
//! a `.nullable()` column (NULL hydrates to `undefined`, which
//! `z.string().nullable()` refuses), after `Data validation failed` + `Safe
//! validation failed` per row — so v4's full backup loses every unattributed
//! tally. v5's `find_all` returns every row. Pinned BOTH ways below: v4's
//! result is exactly v5's minus the NULL-carrying rows, with its two lines per
//! dropped row; a v4 fix makes the first assertion red.
//!
//! Generate (Node 24, from the pinned v4 worktree):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   rm -rf /tmp/qt-wear-stats-tier2
//!   QT_FIXTURE_OUT_DIR=/tmp/qt-wear-stats-tier2 \
//!     $N/npx tsx $V5W/harness/oracle/cases/wardrobe-wear-stats-tier2.ts \
//!     > /tmp/oracle-wear-stats-tier2.ndjson
//! Run:
//!   QT_ORACLE_WEAR_T2=/tmp/oracle-wear-stats-tier2.ndjson \
//!   QT_FIXTURE_WEAR_T2_DIR=/tmp/qt-wear-stats-tier2 \
//!     cargo test -p quilltap-harness --test wardrobe_wear_stats_tier2_equivalence

use std::path::{Path, PathBuf};

use quilltap_core::db::fallback::error_text;
use quilltap_core::db::wardrobe_wear_stats::{
    WardrobeWearIncrement, WardrobeWearStatsRepository, WardrobeWearStatsRow,
};
use quilltap_core::db::{DbError, Writer};
use quilltap_core::test_support::captured_with;
use serde_json::{json, Map, Value};

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/wardrobe-wear-stats-tier2.json")
}

fn strip(row: &WardrobeWearStatsRow) -> Value {
    let mut v = serde_json::to_value(row).unwrap();
    let o = v.as_object_mut().unwrap();
    for k in ["id", "createdAt", "updatedAt"] {
        o.remove(k);
    }
    v
}

fn rows(r: Result<Vec<WardrobeWearStatsRow>, DbError>) -> Value {
    match r {
        Ok(rows) => Value::Array(rows.iter().map(strip).collect()),
        Err(e) => json!({ "threw": error_text(&e) }),
    }
}

fn unit(r: Result<(), DbError>) -> Value {
    match r {
        Ok(()) => Value::Null,
        Err(e) => json!({ "threw": error_text(&e) }),
    }
}

fn strs(op: &Value, k: &str) -> Vec<String> {
    op[k]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s.as_str().unwrap().to_string())
        .collect()
}

/// `Array.from(new Set(ids.filter(Boolean)))`.
fn distinct(ids: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for id in ids {
        if !id.is_empty() && !out.contains(id) {
            out.push(id.clone());
        }
    }
    out
}

/// One v4 log entry rendered as v5's capture rig renders the ported line.
fn expected_line(entry: &Value) -> Option<String> {
    let level = entry["level"].as_str().unwrap();
    let message = entry["message"].as_str().unwrap();
    if message == "Raw query failed" {
        return None; // v4's backend line — v5-unported by standing convention
    }
    let fields: Vec<String> = entry["fields"]
        .as_array()
        .unwrap()
        .iter()
        .map(|kv| {
            let k = kv[0].as_str().unwrap();
            let v = match &kv[1] {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            format!("{k}={v}")
        })
        .collect();
    let (target, lead) = if fields.first().is_some_and(|f| f.starts_with("collection=")) {
        ("quilltap::db", String::new())
    } else {
        (
            "quilltap::wardrobe_wear",
            "module=wardrobe-wear ".to_string(),
        )
    };
    Some(format!(
        "{} {target} {message} {lead}{}",
        level.to_uppercase(),
        fields.join(" ")
    ))
}

#[test]
fn wardrobe_wear_stats_tier2_matches_oracle() {
    let (Some(oracle_path), Some(base_dir)) = (
        std::env::var_os("QT_ORACLE_WEAR_T2"),
        std::env::var_os("QT_FIXTURE_WEAR_T2_DIR"),
    ) else {
        eprintln!("SKIP: set QT_ORACLE_WEAR_T2 and QT_FIXTURE_WEAR_T2_DIR (see header).");
        return;
    };
    let spec: Value = serde_json::from_str(&std::fs::read_to_string(spec_path()).unwrap()).unwrap();
    let oracle: Value = serde_json::from_str(
        std::fs::read_to_string(&oracle_path)
            .expect("read oracle")
            .trim(),
    )
    .expect("parse oracle");
    let theirs = oracle["results"].as_array().expect("oracle results");
    let ops = spec["ops"].as_array().unwrap();
    assert_eq!(theirs.len(), ops.len(), "op count — regenerate the oracle");

    let scratch = tempfile::tempdir().unwrap();
    let work = scratch.path().join("work.db");
    std::fs::copy(Path::new(&base_dir).join("base.db"), &work).expect("copy base.db");
    let pepper = spec["testPepperBase64"].as_str().unwrap();
    let w = Writer::open_writable(&work, pepper).expect("open the base copy");
    let conn = w.connection();
    let repo = WardrobeWearStatsRepository::new(conn);

    let mut find_all_seen = false;
    for (i, (op, v4)) in ops.iter().zip(theirs).enumerate() {
        let label = op["label"].as_str().unwrap();
        let kind = op["kind"].as_str().unwrap();
        let (mine, lines) = captured_with(|| match kind {
            "increment" => unit(
                repo.increment_wears(
                    &op["entries"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|e| WardrobeWearIncrement {
                            item_id: e["itemId"].as_str().unwrap().into(),
                            wearer_character_id: e["wearerCharacterId"].as_str().map(Into::into),
                            chat_id: e["chatId"].as_str().map(Into::into),
                            at: e["at"].as_str().unwrap().into(),
                        })
                        .collect::<Vec<_>>(),
                ),
            ),
            "execSql" => unit(
                conn.execute_batch(op["sql"].as_str().unwrap())
                    .map_err(DbError::from),
            ),
            "dropTable" => unit(
                conn.execute_batch("DROP TABLE \"wardrobe_wear_stats\"")
                    .map_err(DbError::from),
            ),
            "findSummaries" => {
                let ids = strs(op, "itemIds");
                let map = repo.find_summaries(&ids);
                Value::Array(
                    distinct(&ids)
                        .into_iter()
                        .map(|id| json!([id, map[&id]]))
                        .collect(),
                )
            }
            "findHistory" => {
                serde_json::to_value(repo.find_history(op["itemId"].as_str().unwrap())).unwrap()
            }
            "findRowsForItems" => rows(repo.find_rows_for_items(&strs(op, "itemIds"))),
            "findRowsForWearer" => {
                rows(repo.find_rows_for_wearer(op["characterId"].as_str().unwrap()))
            }
            // v4's `foldWearerIntoUnattributed` answers `void`.
            "fold" => match repo.fold_wearer_into_unattributed(op["characterId"].as_str().unwrap())
            {
                Ok(_) => Value::Null,
                Err(e) => json!({ "threw": error_text(&e) }),
            },
            "upsertRows" => {
                let rows: Vec<WardrobeWearStatsRow> =
                    serde_json::from_value(op["rows"].clone()).unwrap();
                unit(repo.upsert_rows(&rows))
            }
            "deleteByItemIds" => unit(repo.delete_by_item_ids(&strs(op, "itemIds"))),
            "findAll" => Value::Array(repo.find_all().iter().map(strip).collect()),
            "dump" => {
                let mut stmt = conn
                    .prepare("SELECT * FROM \"wardrobe_wear_stats\" ORDER BY rowid")
                    .unwrap();
                let names: Vec<String> =
                    stmt.column_names().into_iter().map(String::from).collect();
                let out: Vec<Value> = stmt
                    .query_map([], |r| {
                        let mut o = Map::new();
                        for (j, n) in names.iter().enumerate() {
                            if ["id", "createdAt", "updatedAt"].contains(&n.as_str()) {
                                continue;
                            }
                            let v = match r.get_ref(j)? {
                                rusqlite::types::ValueRef::Null => Value::Null,
                                rusqlite::types::ValueRef::Integer(x) => json!(x),
                                rusqlite::types::ValueRef::Real(x) => json!(x),
                                rusqlite::types::ValueRef::Text(t) => {
                                    json!(String::from_utf8_lossy(t))
                                }
                                rusqlite::types::ValueRef::Blob(_) => Value::Null,
                            };
                            o.insert(n.clone(), v);
                        }
                        Ok(Value::Object(o))
                    })
                    .unwrap()
                    .map(Result::unwrap)
                    .collect();
                Value::Array(out)
            }
            other => panic!("unknown op kind {other}"),
        });

        if kind == "findAll" {
            // FIND_ALL_DROPS_NULLABLE — both ways.
            find_all_seen = true;
            let all = mine.as_array().unwrap();
            let kept: Vec<Value> = all
                .iter()
                .filter(|r| !r["wearerCharacterId"].is_null() && !r["lastWornChatId"].is_null())
                .cloned()
                .collect();
            let dropped = all.len() - kept.len();
            assert!(
                dropped > 0,
                "[{label}] the corpus must hold a NULL-carrying row"
            );
            assert_eq!(
                v4["result"],
                Value::Array(kept),
                "[{label}] v4 no longer drops NULL-carrying rows — FIND_ALL_DROPS_NULLABLE \
                 converged: retire the carve into a plain comparand"
            );
            let v4_logs = v4["logs"].as_array().unwrap();
            assert_eq!(
                v4_logs.len(),
                dropped * 2,
                "[{label}] v4's two lines per dropped row"
            );
            assert!(
                lines.is_empty(),
                "[{label}] v5 drops nothing and logs nothing: {lines:#?}"
            );
            continue;
        }

        assert_eq!(mine, v4["result"], "[#{i} {label}] result");
        let want: Vec<String> = v4["logs"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(expected_line)
            .collect();
        assert_eq!(lines, want, "[#{i} {label}] log lines");
    }
    assert!(find_all_seen, "the findAll arm did not run");
    eprintln!("OK: wardrobe wear ledger matched v4 on {} ops.", ops.len());
}
