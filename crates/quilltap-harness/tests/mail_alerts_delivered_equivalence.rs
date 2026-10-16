//! P4.D291 tier-2 differential (real DB): the deferred mail `alerted` flip —
//! v4 bug 227 (`69d38dd60`), `markMailAlertsDelivered(owed, { chatId, via })`
//! against `quilltap_core::post_office::mailbox::mark_mail_alerts_delivered`.
//!
//! Each oracle record is one scenario on a FRESH copy of the Post Office
//! fixture (`build-mail-carina-tools-fixture.ts` — the reader vault's
//! letters): the same owed list replayed here, then compared —
//!   - every DEBUG / WARN / ERROR line v4 logged during the call (message +
//!     the context keys in v4's order, values verbatim), against the lines v5
//!     logged on ANY thread — `mark_alerted`'s own WARN and the repository
//!     fallback's ERROR run inside the writer closure, on the writer thread,
//!     which a thread-scoped capture cannot see; this binary's one test owns a
//!     process-global capturing subscriber instead;
//!   - `collect_unalerted_mail`'s paths afterwards;
//!   - each touched path's stored content afterwards, byte-for-byte.
//!
//! The seven shapes mirror `mailbox.test.ts`'s `markMailAlertsDelivered`
//! cases on a real store: nothing owed (`null`, empty), all marked, a vanished
//! letter (warned, still counted marked), a per-letter write failure (a
//! planted `BEFORE INSERT` trigger — v4 stores content by address, so a
//! changed letter is a NEW document row: each letter's WARN with the bare
//! SQLite message, `marked` 0), an unreadable store (a planted column rename:
//! the repository fallback answers the read as absent — its ERROR, then the
//! NOT_FOUND WARN, still counted marked), and an already-alerted letter.
//!
//! Regenerate + run (the oracle case header has the jest staging recipe):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   STAGE=/tmp/qt-mail-alerts-stage
//!   rm -rf $STAGE && mkdir -p $STAGE/harness/oracle/cases $STAGE/harness/oracle/fixtures
//!   cp $V5W/harness/oracle/cases/mail-alerts-delivered.test.ts $STAGE/harness/oracle/cases/
//!   cp $V5W/harness/oracle/fixtures/mail-carina-tools.json $STAGE/harness/oracle/fixtures/
//!   cd ~/source/quilltap-server
//!   rm -f /tmp/qt-mailalerts-*
//!   QT_FIXTURE_TMP_MAIN=/tmp/qt-mailalerts-main.db QT_FIXTURE_TMP_MOUNT=/tmp/qt-mailalerts-mount.db \
//!     $N/node --import tsx $V5W/harness/oracle/fixtures/build-mail-carina-tools-fixture.ts
//!   QT_FIXTURE_TMP_MAIN=/tmp/qt-mailalerts-main.db QT_FIXTURE_TMP_MOUNT=/tmp/qt-mailalerts-mount.db \
//!   QT_ORACLE_OUT=/tmp/oracle-mail-alerts-delivered.ndjson PATH=$N:$PATH \
//!     $N/npx jest --silent --watchman=false --roots "$PWD" --roots "$STAGE/harness/oracle/cases" \
//!     -- "mail-alerts-delivered\.test\.ts$"
//!   cd $V5W
//!   QT_ORACLE_MAIL_ALERTS=/tmp/oracle-mail-alerts-delivered.ndjson \
//!   QT_FIXTURE_TMP_MAIN=/tmp/qt-mailalerts-main.db QT_FIXTURE_TMP_MOUNT=/tmp/qt-mailalerts-mount.db \
//!     cargo test -p quilltap-harness --test mail_alerts_delivered_equivalence -- --nocapture

use std::path::{Path, PathBuf};

use quilltap_core::db::database_store::read_database_document;
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::post_office::mailbox::{
    collect_unalerted_mail, mark_mail_alerts_delivered, MailAlertsOwed, MailAlertsVia,
};
use serde_json::Value;

fn env_or_skip(key: &str) -> Option<String> {
    match std::env::var(key) {
        Ok(v) => Some(v),
        Err(_) => {
            eprintln!("SKIP: set {key} (see test header).");
            None
        }
    }
}

fn pepper() -> String {
    let spec: Value = serde_json::from_str(
        &std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../harness/oracle/fixtures/mail-carina-tools.json"),
        )
        .expect("read the fixture spec"),
    )
    .expect("the fixture spec parses");
    spec["testPepperBase64"].as_str().unwrap().to_string()
}

fn clear(p: &Path) {
    for suffix in ["", "-journal", "-wal", "-shm"] {
        let _ = std::fs::remove_file(format!("{}{suffix}", p.display()));
    }
}

fn fresh_copy(main_fx: &str, mount_fx: &str, tag: &str) -> (PathBuf, PathBuf) {
    let dir = std::env::temp_dir();
    let pid = std::process::id();
    let main = dir.join(format!("qt-mailalerts-main-rust-{pid}-{tag}.db"));
    let mount = dir.join(format!("qt-mailalerts-mount-rust-{pid}-{tag}.db"));
    for p in [&main, &mount] {
        clear(p);
    }
    std::fs::copy(main_fx, &main).unwrap_or_else(|e| panic!("copy main: {e}"));
    std::fs::copy(mount_fx, &mount).unwrap_or_else(|e| panic!("copy mount: {e}"));
    (main, mount)
}

fn via_of(s: &str) -> MailAlertsVia {
    match s {
        "finalizer" => MailAlertsVia::Finalizer,
        "preserved-partial" => MailAlertsVia::PreservedPartial,
        "tool-only" => MailAlertsVia::ToolOnly,
        "turn-skip" => MailAlertsVia::TurnSkip,
        other => panic!("unknown via {other}"),
    }
}

/// The planted failure — the same SQL the oracle ran on v4's raw handle —
/// through the runtime's mount-index writer (the sibling
/// `mail_carina_tools` family's recipe).
fn plant(rt: &tokio::runtime::Runtime, db: &Db, kind: &str) {
    let sql = match kind {
        "documents" => "ALTER TABLE doc_mount_documents RENAME COLUMN content TO content_x",
        "trigger" => {
            "CREATE TRIGGER qt_plant_no_insert BEFORE INSERT ON doc_mount_documents \
             BEGIN SELECT RAISE(ABORT, 'disk on fire'); END"
        }
        other => panic!("unknown plant {other}"),
    };
    rt.block_on(db.write(move |writers| {
        writers
            .mount_index()
            .expect("mount present")
            .connection()
            .execute_batch(sql)
            .map_err(|e| quilltap_core::db::DbError::Internal(e.to_string()))
    }))
    .expect("plant");
}

/// One v4 log record against the v5 lines: the same level, the message, and
/// every `key=value` in v4's order.
fn find_line<'l>(lines: &'l [String], want: &Value) -> Option<&'l String> {
    let level = want["level"].as_str().unwrap().to_uppercase();
    let message = want["message"].as_str().unwrap();
    let fields: Vec<String> = want["fields"]
        .as_array()
        .unwrap()
        .iter()
        .map(|kv| format!("{}={}", kv[0].as_str().unwrap(), kv[1].as_str().unwrap()))
        .collect();
    lines.iter().find(|l| {
        if !l.starts_with(&format!("{level} ")) || !l.contains(&format!(" {message}")) {
            return false;
        }
        let mut at = 0usize;
        fields.iter().all(|f| match l[at..].find(&format!(" {f}")) {
            Some(i) => {
                at += i + f.len();
                true
            }
            None => false,
        })
    })
}

#[test]
fn mail_alerts_delivered_matches_oracle() {
    let (Some(oracle), Some(main_fx), Some(mount_fx)) = (
        env_or_skip("QT_ORACLE_MAIL_ALERTS"),
        env_or_skip("QT_FIXTURE_TMP_MAIN"),
        env_or_skip("QT_FIXTURE_TMP_MOUNT"),
    ) else {
        return;
    };
    let pepper = pepper();
    // Every thread's events (the writer thread's included) — this binary's one
    // test owns the process-global subscriber.
    let logs = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    {
        use tracing_subscriber::layer::SubscriberExt;
        tracing::subscriber::set_global_default(
            tracing_subscriber::registry()
                .with(quilltap_core::test_support::CaptureLayer(logs.clone())),
        )
        .expect("this binary's one test owns the global subscriber");
    }
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let mut seen = 0usize;
    let mut warned_failures = 0usize;
    for line in std::fs::read_to_string(&oracle)
        .unwrap_or_else(|e| panic!("read oracle {oracle}: {e}"))
        .lines()
        .filter(|l| !l.trim().is_empty())
    {
        let rec: Value = serde_json::from_str(line).expect("oracle line parses");
        let label = rec["label"].as_str().unwrap().to_string();
        assert!(
            rec["threw"].is_null(),
            "{label}: v4 threw — the case is broken"
        );
        let (main, mount) = fresh_copy(&main_fx, &mount_fx, &label);
        let db = Db::open(
            DbPaths {
                main: main.clone(),
                mount_index: Some(mount.clone()),
                llm_logs: None,
            },
            &pepper,
        )
        .unwrap_or_else(|e| panic!("{label}: open: {e}"));
        if let Some(kind) = rec["plant"].as_str() {
            plant(&rt, &db, kind);
        }
        let vault = rec["vaultId"].as_str().unwrap().to_string();
        let paths: Option<Vec<String>> = rec["owed"]
            .as_array()
            .map(|a| a.iter().map(|p| p.as_str().unwrap().to_string()).collect());
        let owed = paths.clone().map(|paths| MailAlertsOwed {
            vault_id: vault.clone(),
            paths,
        });
        let via = via_of(rec["via"].as_str().unwrap());

        logs.lock().unwrap().clear();
        rt.block_on(mark_mail_alerts_delivered(
            &db,
            owed.as_ref(),
            "chat-mail",
            via,
        ));
        let lines: Vec<String> = std::mem::take(&mut *logs.lock().unwrap());
        // Only the lines this unit owns or reaches (the mailbox, the document
        // store's fallback) — the runtime's own boot lines are not v4's.
        let relevant: Vec<String> = lines
            .into_iter()
            .filter(|l| {
                // v4 `01f508d4e`: the DEBUG's message is `Mail alerts marked
                // delivered`; the per-letter WARN keeps `markMailAlertsDelivered:`.
                l.contains("Mail alerts marked delivered")
                    || l.contains("markMailAlertsDelivered")
                    || l.contains("markAlerted")
                    || l.contains("Error finding document")
            })
            .collect();
        let want = rec["logs"].as_array().unwrap();
        assert_eq!(
            relevant.len(),
            want.len(),
            "{label}: line count\nv5: {relevant:#?}\nv4: {want:#?}"
        );
        for w in want {
            assert!(
                find_line(&relevant, w).is_some(),
                "{label}: v4's line {w} not among v5's:\n{relevant:#?}"
            );
            if w["message"] == "markMailAlertsDelivered: could not mark letter alerted" {
                warned_failures += 1;
            }
        }

        if !rec["unalerted"].is_null() {
            let v = vault.clone();
            let unalerted: Vec<Value> = db
                .read_mount_index(move |c| collect_unalerted_mail(c, &v))
                .unwrap_or_else(|e| panic!("{label}: collect: {e}"))
                .into_iter()
                .map(|l| Value::String(l.path))
                .collect();
            assert_eq!(
                Value::Array(unalerted),
                rec["unalerted"],
                "{label}: the unalerted letters afterwards"
            );
            let contents: Vec<Value> = paths
                .unwrap_or_default()
                .into_iter()
                .map(|p| {
                    let v = vault.clone();
                    db.read_mount_index(move |c| {
                        Ok(read_database_document(c, &v, &p)
                            .ok()
                            .map(|d| Value::String(d.content))
                            .unwrap_or(Value::Null))
                    })
                    .unwrap()
                })
                .collect();
            assert_eq!(
                Value::Array(contents),
                rec["contents"],
                "{label}: the touched letters' stored content"
            );
        }
        drop(db);
        clear(&main);
        clear(&mount);
        seen += 1;
    }
    assert_eq!(seen, 7, "the oracle lost scenarios");
    assert_eq!(
        warned_failures, 2,
        "the per-letter failure WARN arm lost its rows"
    );
    eprintln!("OK: mail-alerts-delivered matched the oracle ({seen} scenarios).");
}
