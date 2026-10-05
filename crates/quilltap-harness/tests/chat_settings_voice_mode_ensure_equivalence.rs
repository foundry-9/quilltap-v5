//! Tier-2 differential: v5's `chat_settings.impersonationVoiceMode` boot
//! ensure against v4's REAL `impersonation-voice-mode-v1` migration (P4.D251,
//! v4 `07b8f0209`) in THREE starting shapes.
//!
//! Both sides start from the SAME base file per mode, which the oracle case
//! builds at the target pin from v4's OWN generateDDL with the one
//! `impersonationVoiceMode` line removed (the pre-`686954937` statement byte
//! for byte) and the retired column added back through the add-field
//! migration's exact ALTER where the mode calls for it:
//!
//!   (a) old present, new absent, rows 1 / 0 / NULL / 1 (the §R.13 Friday shape);
//!   (b) NEITHER column — v4 runs add-field THEN mode, v5 runs its ONE ensure;
//!   (c) BOTH present with the old one appended LAST (the ping-pong shape),
//!       rows (old 1, 'always') / (1, 'off') / (0, NULL) / (1, 'ask').
//!
//! The oracle then runs v4's own migration modules through
//! `harness/oracle/lib/v4-migrations.ts` on a copy; this test runs
//! `ensure_chat_settings_impersonation_voice_mode` on its own copy. Three
//! comparands per mode, each byte-exact: `PRAGMA table_info` (name / type /
//! notnull / default / pk, column ORDER included — the ALTER appends, the DROP
//! removes), the table's `sqlite_master.sql` (the text SQLite stores for an
//! ALTERed-then-DROPped table), and every row's `(id, impersonationVoiceMode)`.
//! v4's migration report (`itemsAffected`, `message`) is recorded in the NDJSON
//! but NOT compared — v5 has no report — the counts are asserted from the rows
//! and from v5's own outcome instead, and the report is checked only for
//! having RUN (a `not needed` on the mode migration would make the diff
//! vacuous).
//!
//! Generate (Node 24, from the TARGET-pinned v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT_DIR=/tmp/qt-voice-mode-ensure \
//!     $N/npx tsx $V5W/harness/oracle/cases/chat-settings-voice-mode-ensure.ts \
//!     > /tmp/oracle-voice-mode-ensure.ndjson
//! Run:
//!   QT_ORACLE_VOICE_MODE_ENSURE=/tmp/oracle-voice-mode-ensure.ndjson \
//!   QT_FIXTURE_VOICE_MODE_ENSURE_DIR=/tmp/qt-voice-mode-ensure \
//!     cargo test -p quilltap-harness --test chat_settings_voice_mode_ensure_equivalence -- --nocapture

use std::path::Path;

use quilltap_core::db::chat_settings_impersonation_voice_mode_repair::{
    ensure_chat_settings_impersonation_voice_mode, VoiceModeEnsureOutcome,
};
use quilltap_core::db::Writer;
use serde_json::{json, Value};

fn spec_pepper() -> String {
    let spec: Value = serde_json::from_str(
        &std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../harness/oracle/fixtures/chat-settings-tier2.json"),
        )
        .unwrap(),
    )
    .unwrap();
    spec["testPepperBase64"].as_str().unwrap().to_string()
}

fn column_names(conn: &rusqlite::Connection) -> Vec<String> {
    let mut stmt = conn
        .prepare("PRAGMA table_info(\"chat_settings\")")
        .unwrap();
    stmt.query_map([], |r| r.get::<_, String>(1))
        .unwrap()
        .map(|r| r.unwrap())
        .collect()
}

fn snapshot(conn: &rusqlite::Connection) -> (Value, Value, Value) {
    let mut stmt = conn
        .prepare("PRAGMA table_info(\"chat_settings\")")
        .unwrap();
    let info: Vec<Value> = stmt
        .query_map([], |r| {
            Ok(json!({
                "cid": r.get::<_, i64>(0)?,
                "name": r.get::<_, String>(1)?,
                "type": r.get::<_, String>(2)?,
                "notnull": r.get::<_, i64>(3)?,
                "dflt_value": r.get::<_, Option<String>>(4)?,
                "pk": r.get::<_, i64>(5)?,
            }))
        })
        .unwrap()
        .map(|r| r.unwrap())
        .collect();
    let sql: String = conn
        .query_row(
            "SELECT sql FROM sqlite_master WHERE name = 'chat_settings'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let mut stmt = conn
        .prepare("SELECT id, impersonationVoiceMode FROM chat_settings ORDER BY id")
        .unwrap();
    let rows: Vec<Value> = stmt
        .query_map([], |r| {
            Ok(json!({
                "id": r.get::<_, String>(0)?,
                "impersonationVoiceMode": r.get::<_, Option<String>>(1)?,
            }))
        })
        .unwrap()
        .map(|r| r.unwrap())
        .collect();
    (Value::Array(info), Value::String(sql), Value::Array(rows))
}

/// v4's report per mode: the add-field migration RUNS only in (b); the mode
/// migration RUNS in every mode (anything else makes the diff vacuous). The
/// outcome is v5's, pinned here from the rows; in (a) and (c) v4's own
/// `translated N` count is cross-checked against it below. In (b) the two
/// DIFFER by construction and agree only in the final state: v4 runs the
/// add-field migration first (adding the old column), so its mode migration
/// translates every row and drops that column, while v5's one ensure only
/// appends the mode.
fn expected(mode: &str) -> (&'static str, VoiceModeEnsureOutcome) {
    match mode {
        "a" => (
            "not needed",
            VoiceModeEnsureOutcome {
                column_added: true,
                rows_backfilled: 4,
                column_dropped: true,
            },
        ),
        "b" => (
            "+RAN add-impersonation-voice-rewrite-field-v1",
            VoiceModeEnsureOutcome {
                column_added: true,
                rows_backfilled: 0,
                column_dropped: false,
            },
        ),
        "c" => (
            "not needed",
            VoiceModeEnsureOutcome {
                column_added: false,
                rows_backfilled: 2,
                column_dropped: true,
            },
        ),
        other => panic!("unknown mode {other}"),
    }
}

#[test]
fn the_boot_ensure_matches_v4s_migration_in_three_starting_shapes() {
    let (Ok(oracle_path), Ok(fixture_dir)) = (
        std::env::var("QT_ORACLE_VOICE_MODE_ENSURE"),
        std::env::var("QT_FIXTURE_VOICE_MODE_ENSURE_DIR"),
    ) else {
        eprintln!(
            "SKIP: set QT_ORACLE_VOICE_MODE_ENSURE and QT_FIXTURE_VOICE_MODE_ENSURE_DIR (see header)."
        );
        return;
    };
    let text = std::fs::read_to_string(&oracle_path).unwrap();
    let lines: Vec<Value> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("NDJSON line"))
        .collect();
    assert_eq!(lines.len(), 3, "three modes, one line each");

    let mut seen = Vec::new();
    for oracle in &lines {
        let mode = oracle["mode"].as_str().unwrap();
        seen.push(mode.to_string());
        let report = oracle["report"].as_array().unwrap();
        let (add_field_verdict, want_outcome) = expected(mode);
        assert_eq!(report.len(), 2, "[{mode}] both migrations reported");
        assert!(
            report[0].as_str().unwrap().contains(add_field_verdict),
            "[{mode}] add-field: {}",
            report[0]
        );
        assert!(
            report[1]
                .as_str()
                .unwrap()
                .starts_with("+RAN impersonation-voice-mode-v1"),
            "[{mode}] the mode migration must have RUN, or the diff is vacuous: {}",
            report[1]
        );
        if mode != "b" {
            let msg = report[1].as_str().unwrap();
            let translated: usize = msg
                .split("translated ")
                .nth(1)
                .and_then(|t| t.split_whitespace().next())
                .and_then(|n| n.parse().ok())
                .unwrap_or_else(|| panic!("[{mode}] no `translated N` in v4's report: {msg}"));
            assert_eq!(
                translated, want_outcome.rows_backfilled,
                "[{mode}] v4's translated count vs v5's backfill"
            );
        }

        let fixture = Path::new(&fixture_dir).join(format!("voice-mode-{mode}.db"));
        let work = std::env::temp_dir().join(format!(
            "qt-voice-mode-ensure-{mode}-{}.db",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&work);
        std::fs::copy(&fixture, &work).unwrap();
        let writer = Writer::open_writable(&work, &spec_pepper()).unwrap();
        let conn = writer.connection();
        let before = column_names(conn);
        match mode {
            "a" => assert!(
                before.iter().any(|c| c == "impersonationVoiceRewrite")
                    && !before.iter().any(|c| c == "impersonationVoiceMode"),
                "[a] old present, new absent"
            ),
            "b" => assert!(
                !before.iter().any(|c| c.starts_with("impersonationVoice")),
                "[b] neither column"
            ),
            "c" => assert_eq!(
                before.last().map(String::as_str),
                Some("impersonationVoiceRewrite"),
                "[c] both present, the old one LAST"
            ),
            _ => unreachable!(),
        }

        let outcome = ensure_chat_settings_impersonation_voice_mode(conn).unwrap();
        assert_eq!(outcome, want_outcome, "[{mode}] v5's outcome");
        let (info, sql, rows) = snapshot(conn);
        drop(writer);
        let _ = std::fs::remove_file(&work);

        assert_eq!(info, oracle["tableInfo"], "[{mode}] PRAGMA table_info");
        assert_eq!(sql, oracle["sql"], "[{mode}] sqlite_master.sql");
        assert_eq!(
            rows, oracle["rows"],
            "[{mode}] (id, impersonationVoiceMode) per row"
        );
        eprintln!(
            "OK [{mode}]: the boot ensure matches v4's migration ({} columns, {} rows).",
            info.as_array().unwrap().len(),
            rows.as_array().unwrap().len()
        );
    }
    seen.sort();
    assert_eq!(seen, ["a", "b", "c"], "every mode present in the oracle");
}
