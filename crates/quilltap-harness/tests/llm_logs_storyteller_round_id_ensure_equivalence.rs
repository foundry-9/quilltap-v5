//! P4.D341 — the llm-logs round-id boot ensure
//! (`db::llm_logs_storyteller_round_id_repair`, the FIRST llm-logs column
//! ensure) against v4's REAL `add-llm-logs-storyteller-round-id-column-v1`
//! (v4 `b15fe9404`, the NINETEENTH D23 move) through v4's REAL
//! `MigrationRunner` narrowed to it, in five shapes
//! (`harness/oracle/cases/llm-logs-storyteller-round-id-ensure.ts` — its header
//! names them): a (column absent), b (logs FILE absent — v5's host has no
//! partition and SKIPS, R-17), c (file present, table absent), d (migrated),
//! e (generateDDL-current, NO index — `COLUMN_WITHOUT_INDEX_NOT_NEEDED`).
//!
//! Comparands per shape, byte-exact: whether the logs file exists, `PRAGMA
//! table_info`, the table's and the index's `sqlite_master.sql`, the rows'
//! `storytellerRoundId` (`quote()` + `typeof`), the header's journal mode
//! after (v4's through the runner helper, which restores it), and the main
//! file's ledger (`UNSTAMPED_COLUMN_ENSURES` — v4 stamps on a, v5 never).
//!
//! **`V4_LLM_LOGS_MIGRATION_WAL`** (a named both-ways carve, MEASURED): v4's
//! module opens the logs file with its OWN `journal_mode = WAL` connection —
//! in `shouldRun` as well as `run` — and that mode PERSISTS in the file
//! header (`rawJournalMode` = `wal` on every mode with a file: a, c, d, e).
//! v5's partitions open TRUNCATE and must NOT mirror a header flip only v4's
//! migration-time opener causes: v5's header after its ensure reads `delete`
//! (TRUNCATE is not persisted). Pinned both ways below.
//!
//! Generate (Node 24, from the pinned v4 worktree):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   rm -rf /tmp/qt-llm-logs-round-id-ensure /tmp/oracle-llm-logs-round-id-ensure.ndjson
//!   QT_FIXTURE_OUT_DIR=/tmp/qt-llm-logs-round-id-ensure $N/npx tsx $V5W/harness/oracle/cases/llm-logs-storyteller-round-id-ensure.ts > /tmp/oracle-llm-logs-round-id-ensure.ndjson
//! Run:
//!   cd $V5W
//!   QT_ORACLE_LLM_LOGS_ROUND_ID_ENSURE=/tmp/oracle-llm-logs-round-id-ensure.ndjson QT_FIXTURE_LLM_LOGS_ROUND_ID_ENSURE_DIR=/tmp/qt-llm-logs-round-id-ensure cargo test -p quilltap-harness --test llm_logs_storyteller_round_id_ensure_equivalence -- --nocapture

mod keystone_ensure_support;

use std::path::Path;

use keystone_ensure_support as kit;
use quilltap_core::db::llm_logs_storyteller_round_id_repair::{
    ensure_llm_logs_storyteller_round_id, LlmLogsRoundIdOutcome, INDEX_NAME, MIGRATION_ID,
};
use quilltap_core::db::Writer;
use serde_json::{json, Value};

fn header_journal_mode(path: &Path) -> String {
    let conn = rusqlite::Connection::open(path).unwrap();
    let hex: String = {
        use base64::Engine as _;
        base64::engine::general_purpose::STANDARD
            .decode(kit::PEPPER)
            .unwrap()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    };
    conn.execute_batch(&format!("PRAGMA key = \"x'{hex}'\";"))
        .unwrap();
    conn.query_row("PRAGMA journal_mode", [], |r| r.get(0))
        .unwrap()
}

#[test]
fn the_round_id_ensure_matches_v4s_migration_in_five_shapes() {
    let (Some(oracle), Some(dir)) = (
        std::env::var_os("QT_ORACLE_LLM_LOGS_ROUND_ID_ENSURE"),
        std::env::var_os("QT_FIXTURE_LLM_LOGS_ROUND_ID_ENSURE_DIR"),
    ) else {
        eprintln!(
            "SKIP: set QT_ORACLE_LLM_LOGS_ROUND_ID_ENSURE and QT_FIXTURE_LLM_LOGS_ROUND_ID_ENSURE_DIR (see header)."
        );
        return;
    };
    let mut modes = Vec::new();
    for v4 in kit::read_ndjson(&oracle) {
        let mode = v4["mode"].as_str().unwrap().to_string();
        let report = kit::report(&v4);
        let runs = mode == "a";
        assert_eq!(report.len(), 1, "[{mode}] {report:?}");
        if runs {
            assert!(
                report[0].starts_with(&format!("+RAN {MIGRATION_ID} ")),
                "[{mode}] {report:?}"
            );
        } else {
            assert!(report[0].contains(" not needed"), "[{mode}] {report:?}");
        }
        let want_ledger: Vec<String> = if runs {
            vec![MIGRATION_ID.into()]
        } else {
            vec![]
        };
        assert_eq!(
            serde_json::from_value::<Vec<String>>(v4["ledger"].clone()).unwrap(),
            want_ledger,
            "[{mode}] v4's ledger"
        );

        let scratch = tempfile::tempdir().unwrap();
        let logs = scratch.path().join("work-llm-logs.db");
        let src = Path::new(&dir).join(format!("roundid-{mode}-llm-logs.db"));
        let got = if src.exists() {
            std::fs::copy(&src, &logs).unwrap();
            let outcome = {
                let w = Writer::open_writable(&logs, kit::PEPPER).unwrap();
                let outcome = ensure_llm_logs_storyteller_round_id(w.connection()).expect("ensure");
                let c = w.connection();
                let snap = json!({
                    "fileExists": true,
                    "tableInfo": kit::table_info(c, "llm_logs"),
                    "sql": kit::table_sql(c, "llm_logs"),
                    "indexSql": kit::index_sql(c, INDEX_NAME),
                    "rows": kit::cells(c, "llm_logs", &["storytellerRoundId"]),
                });
                assert!(
                    kit::ledger(c).is_empty(),
                    "[{mode}] v5 stamped the logs file"
                );
                (outcome, snap)
            };
            let (outcome, mut snap) = outcome;
            let want = if runs {
                LlmLogsRoundIdOutcome::Added {
                    index_created: true,
                }
            } else {
                LlmLogsRoundIdOutcome::NotNeeded
            };
            assert_eq!(outcome, want, "[{mode}] the ensure's answer");
            snap["journalMode"] = json!(header_journal_mode(&logs));
            snap
        } else {
            // (b) — the host's `None` arm: nothing runs, nothing is created.
            assert_eq!(mode, "b", "only (b) has no logs file");
            json!({"fileExists": false, "tableInfo": [], "sql": null, "indexSql": null, "rows": [], "journalMode": null})
        };
        for key in [
            "fileExists",
            "tableInfo",
            "sql",
            "indexSql",
            "rows",
            "journalMode",
        ] {
            kit::assert_bytes(&got[key], &v4[key], &format!("[{mode}] {key}"));
        }
        // `V4_LLM_LOGS_MIGRATION_WAL`, both ways.
        if v4["fileExists"] == json!(true) {
            assert_eq!(
                v4["rawJournalMode"],
                json!("wal"),
                "[{mode}] v4's opener persists WAL"
            );
            assert_ne!(
                got["journalMode"],
                json!("wal"),
                "[{mode}] v5 must not mirror it"
            );
        } else {
            assert_eq!(v4["rawJournalMode"], Value::Null);
        }
        if mode == "a" {
            assert!(
                v4["sql"]
                    .as_str()
                    .unwrap()
                    .ends_with(", \"storytellerRoundId\" TEXT DEFAULT NULL)"),
                "marker"
            );
        }
        if mode == "e" {
            assert_eq!(
                v4["indexSql"],
                Value::Null,
                "COLUMN_WITHOUT_INDEX_NOT_NEEDED"
            );
        }
        modes.push(mode);
    }
    assert_eq!(modes, ["a", "b", "c", "d", "e"], "every shape ran");
    eprintln!("OK: the llm-logs round-id ensure matched v4 in shapes {modes:?}.");
}
