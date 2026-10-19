//! P4.D288 unit 13 — the ZERO-re-run proof (R-E's stamp rule, §R.13(a)):
//! after v5's whole span chain (`db::p4d288_span::run_span_ensures`, the twelve
//! ensures in `seed_built_ins` order), v4's REAL `MigrationRunner` at the
//! `83334b9a5` pin, narrowed to the twelve ids, re-runs NOTHING and changes
//! nothing — and every row v5 stamps is byte-equal to the row v4's runner
//! writes for the same migration on the same instance.
//!
//! Four base modes, each pinned both ways — v4's runner records no `+RAN` and
//! changes no table, index or ledger row after v5's chain:
//! - `planted` — a PLANTED pre-round instance the oracle builds (never a
//!   real-instance file: the other watermark column, the vector mirror, the
//!   dead settings columns + key, retired job rows, legacy UTC-midnight dates,
//!   stale reinforced importance); every v5 stamp byte-equal to the row v4's
//!   runner writes running the twelve on the same base;
//! - `fresh` — a fresh v5-provisioned instance (its first boot's chain);
//! - `fresh-used` — the same in use: chats whose summary anchors v4's episode
//!   migration would backfill (one already holding its `episode-fold` cursor),
//!   plus the candidate filter's edge shapes (`[ ]`, unparseable, `[]`, a last
//!   element `""`);
//! - `v4-migrated` — the planted instance already migrated by v4's runner
//!   (every ensure a no-op, no duplicate or changed stamp).
//!
//! **P4.D304 (the `25b8fab6a` round):** the narrowed runner also carries the
//! two 4.11 `chat_settings` migrations after the span
//! (`CHAT_SETTINGS_4_11_MIGRATIONS`), the planted base takes both columns
//! back, and `run_chain` runs `run_chat_settings_4_11_ensures` after the span
//! chain (the host's order). v4 runs both on the planted base (14 in all) and
//! NEITHER after v5's chain on any base; v5 stamps neither — the `-unstamped`
//! twins re-run exactly what they re-ran before, which proves no stamp is owed.
//!
//! **`STAMP_RULE_RED_FIRST`** (a named both-ways row, §R.13(a)'s red-first set
//! BY ID, kept as a permanent pin): each base but `v4-migrated` has an
//! `-unstamped` twin — the instance after v5's chain with the span's ledger
//! rows DELETED. v4 re-runs on it EXACTLY the ids v5 stamped, in v4's order
//! (measured: `add-memory-tiers-v1`, the episode watermark, the two data
//! rewrites, the cursor migration — on all three), so every stamp is necessary
//! and no unstamped ensure needs one. On `fresh` / `fresh-used` the row v4
//! writes on the twin is also byte-compared with v5's stamp — there v5 stamped
//! the settled chain WITHOUT running it
//! (`memory_extraction_cursors_repair::stamp_settled_chain`), so the twin's
//! row IS v4's row for the same migration on the same instance.
//!
//! **The ping-pong id** (the `83334b9a5` unification's review): none of the
//! bases carries an `add-memory-tiers-v1` row (P4.D265's ensure never stamped
//! it), and v4's one pass on the planted base never runs it — its chat column
//! is still there. It becomes due only AFTER the cursor migration drops that
//! column, so measured with tiers in the narrowed runner v4 re-ran it on
//! `planted` AND on `v4-migrated` after v5's chain (re-adding the dead column
//! for good). v5 stamps it on every shape
//! (`memory_extraction_cursors_repair::stamp_tiers_after_cursor_move`); the
//! row v4 writes for it is read from the `planted-unstamped` twin — v4's own
//! re-run on the post-chain instance of that base — for `planted` and
//! `v4-migrated` alike (`v4-migrated` has no twin, and with the stamp in place
//! v4 runs nothing there to measure).
//! Before that arm the `fresh` mode re-ran the episode watermark + the cursor
//! migration (P4.D288 unit 7's `FRESH_INSTANCE_UNLEDGERED`; on `fresh-used`
//! that inserts `episode-fold` cursors at the last summary anchor).
//!
//! Regenerate + run (three steps around this file, Node 24, from the round's
//! pin, `TZ=UTC QUILLTAP_TIMEZONE=UTC`):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   rm -rf /tmp/qt-zero-rerun /tmp/oracle-zero-rerun-*.ndjson
//!   cd ~/source/quilltap-server
//!   TZ=UTC QUILLTAP_TIMEZONE=UTC QT_ZERO_RERUN_PHASE=build QT_FIXTURE_ZERO_RERUN_DIR=/tmp/qt-zero-rerun \
//!     $N/npx tsx $V5W/harness/oracle/cases/zero-rerun.ts > /tmp/oracle-zero-rerun-build.ndjson
//!   cd $V5W && QT_ZERO_RERUN_WRITE=1 QT_FIXTURE_ZERO_RERUN_DIR=/tmp/qt-zero-rerun \
//!     cargo test -p quilltap-harness --test zero_migrations_rerun_equivalence -- write_after_v5
//!   cd ~/source/quilltap-server
//!   TZ=UTC QUILLTAP_TIMEZONE=UTC QT_ZERO_RERUN_PHASE=v4 QT_FIXTURE_ZERO_RERUN_DIR=/tmp/qt-zero-rerun \
//!     $N/npx tsx $V5W/harness/oracle/cases/zero-rerun.ts > /tmp/oracle-zero-rerun-v4.ndjson
//!   cd $V5W && QT_ORACLE_ZERO_RERUN_DIR=/tmp QT_FIXTURE_ZERO_RERUN_DIR=/tmp/qt-zero-rerun \
//!     cargo test -p quilltap-harness --test zero_migrations_rerun_equivalence -- no_migration_reruns --nocapture

use std::path::{Path, PathBuf};

use quilltap_core::db::Writer;
use serde_json::Value;

const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
/// The ping-pong id: v4's `shouldRun` for it holds through the chat watermark
/// column the span's cursor migration drops, on every base whose tiers row is
/// absent (a v5 instance P4.D265's unstamped ensure tiered).
const TIERS: &str = "add-memory-tiers-v1";
/// `P4D288_SPAN_MIGRATIONS`: `add-memory-tiers-v1` (P4.D265's, pre-span — the
/// ping-pong id, see the header) then v4's twelve span ids in registration
/// order.
const SPAN: [&str; 13] = [
    TIERS,
    "add-memory-superseded-at-v1",
    "add-chat-scenario-source-v1",
    "add-episode-extraction-watermark-v1",
    "recompute-reinforced-importance-v1",
    "rezone-legacy-memory-occurred-at-v1",
    "add-memories-superseded-by-index-v1",
    "move-memory-maintenance-settings-v1",
    "retire-memory-vector-entries-v1",
    "add-memory-extraction-cursors-v1",
    "drop-inter-character-memory-jobs-v1",
    "add-tfidf-text-shape-version-v1",
    "drop-housekeeping-consolidation-jobs-v1",
];

/// The base modes; each has an `-unstamped` twin except `v4-migrated` (whose
/// rows are v4's own).
const BASES: [&str; 4] = ["planted", "fresh", "fresh-used", "v4-migrated"];

fn dir() -> Option<PathBuf> {
    std::env::var("QT_FIXTURE_ZERO_RERUN_DIR")
        .ok()
        .map(PathBuf::from)
}

/// P4.D304: the two 4.11 `chat_settings` migrations the narrowed runner runs
/// after the span (`CHAT_SETTINGS_4_11_MIGRATIONS`) — UNSTAMPED on v5's side,
/// so v4's `shouldRun` (false once v5's chain added both columns) is the whole
/// zero-rerun proof for them.
const CHAT_SETTINGS_4_11: [&str; 2] = [
    "add-custom-tool-triggers-field-v1",
    "add-composer-slash-commands-field-v1",
];

/// P4.D315: the chat wardrobe history's three migrations the narrowed runner
/// runs after the 4.11 pair (`CHAT_WARDROBE_HISTORY_MIGRATIONS`) — v5 STAMPS the
/// table and the seed as v4's runner would (R-C); the column step is "not
/// needed" on a table created at its final shape, on both sides.
const CHAT_WARDROBE_HISTORY: [&str; 3] = [
    quilltap_core::db::chat_wardrobe_history_repair::TABLE_MIGRATION_ID,
    quilltap_core::db::chat_wardrobe_history_repair::TOOL_NAME_MIGRATION_ID,
    quilltap_core::db::chat_wardrobe_history_repair::SEED_MIGRATION_ID,
];

/// Run v5's span chain, then the 4.11 `chat_settings` chain, then the chat
/// wardrobe history's (the host's boot order), on `path` (a caller-thread
/// writer).
fn run_chain(path: &Path) {
    let w = Writer::open_writable(path, PEPPER).expect("open");
    quilltap_core::db::p4d288_span::run_span_ensures(w.connection(), "UTC")
        .expect("the span chain");
    quilltap_core::db::chat_settings_4_11_repair::run_chat_settings_4_11_ensures(w.connection())
        .expect("the 4.11 chat_settings chain");
    quilltap_core::db::chat_wardrobe_history_repair::run_chat_wardrobe_history_ensures(
        w.connection(),
    )
    .expect("the chat wardrobe history chain");
}

/// A fresh v5-provisioned main partition at `out` (its first boot not yet run).
fn provision_to(out: &Path) {
    let scratch = tempfile::tempdir().unwrap();
    quilltap_core::services::provisioning::provision_fresh_instance(scratch.path(), PEPPER)
        .expect("provision");
    std::fs::copy(scratch.path().join("quilltap.db"), out).expect("copy fresh");
}

/// The write step: v5's chain over each base → `<mode>.after-v5.db`, and each
/// stamped base's `-unstamped` twin (the span's ledger rows DELETED after the
/// chain).
#[test]
fn write_after_v5() {
    if std::env::var("QT_ZERO_RERUN_WRITE").is_err() {
        eprintln!("SKIP: the write step runs only under QT_ZERO_RERUN_WRITE=1 (see header).");
        return;
    }
    let dir = dir().expect("QT_FIXTURE_ZERO_RERUN_DIR");
    for mode in BASES {
        let out = dir.join(format!("{mode}.after-v5.db"));
        let _ = std::fs::remove_file(&out);
        match mode {
            "planted" | "v4-migrated" => {
                std::fs::copy(dir.join(format!("{mode}.db")), &out).expect("copy base");
            }
            "fresh" => provision_to(&out),
            "fresh-used" => {
                // A v5-provisioned instance in use: chats whose summary anchors
                // v4's episode migration would backfill (one already holding its
                // `episode-fold` cursor), plus the filter's edge shapes.
                provision_to(&out);
                let w = Writer::open_writable(&out, PEPPER).unwrap();
                w.connection()
                    .execute_batch(
                        "INSERT INTO chats (id, userId, title, summaryAnchorMessageIds, createdAt, updatedAt) VALUES \
                           ('u-c1', 'u1', 't', '[\"a1\",\"a2\"]', 't', 't'), \
                           ('u-c2', 'u1', 't', '[ ]', 't', 't'), \
                           ('u-c3', 'u1', 't', 'not json', 't', 't'), \
                           ('u-c4', 'u1', 't', '[\"b1\"]', 't', 't'), \
                           ('u-c5', 'u1', 't', '[]', 't', 't'), \
                           ('u-c6', 'u1', 't', '[\"x\", \"\"]', 't', 't'); \
                         INSERT INTO memory_extraction_cursors (id, chatId, pass, messageId, createdAt, updatedAt) \
                           VALUES ('u-c4:episode-fold', 'u-c4', 'episode-fold', 'b0', 't', 't');",
                    )
                    .expect("plant fresh-used");
            }
            _ => unreachable!(),
        }
        run_chain(&out);
        if mode == "v4-migrated" {
            continue;
        }
        let twin = dir.join(format!("{mode}-unstamped.after-v5.db"));
        let _ = std::fs::remove_file(&twin);
        std::fs::copy(&out, &twin).expect("copy twin");
        let w = Writer::open_writable(&twin, PEPPER).expect("open twin");
        for id in SPAN {
            w.connection()
                .execute("DELETE FROM migrations_state WHERE id = ?1", [id])
                .expect("unstamp");
        }
        // P4.D315: the history twin — the HISTORY rows deleted instead.
        let twin = dir.join(format!("{mode}-history-unstamped.after-v5.db"));
        let _ = std::fs::remove_file(&twin);
        std::fs::copy(&out, &twin).expect("copy history twin");
        let w = Writer::open_writable(&twin, PEPPER).expect("open history twin");
        for id in CHAT_WARDROBE_HISTORY {
            w.connection()
                .execute("DELETE FROM migrations_state WHERE id = ?1", [id])
                .expect("unstamp history");
        }
    }
}

fn read_lines(path: &Path) -> Vec<Value> {
    std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

fn ledger_by_id(ledger: &Value) -> Vec<(String, Value)> {
    ledger
        .as_array()
        .unwrap()
        .iter()
        .map(|r| (r["id"].as_str().unwrap().to_string(), r.clone()))
        .collect()
}

/// The span rows of a ledger dump, keyed by id.
fn span_rows(ledger: &Value) -> Vec<(String, Value)> {
    ledger_by_id(ledger)
        .into_iter()
        .filter(|(id, _)| SPAN.contains(&id.as_str()))
        .collect()
}

#[test]
fn no_migration_reruns_after_the_v5_chain() {
    let Ok(oracle_dir) = std::env::var("QT_ORACLE_ZERO_RERUN_DIR") else {
        eprintln!("SKIP: set QT_ORACLE_ZERO_RERUN_DIR (+ QT_FIXTURE_ZERO_RERUN_DIR) — see header.");
        return;
    };
    let oracle_dir = PathBuf::from(oracle_dir);
    let build = read_lines(&oracle_dir.join("oracle-zero-rerun-build.ndjson"));
    let v4 = read_lines(&oracle_dir.join("oracle-zero-rerun-v4.ndjson"));
    let v4_ledger = ledger_by_id(&build[0]["ledger"]);
    assert_eq!(
        build[0]["ran"].as_array().unwrap().len(),
        16,
        "v4 ran the twelve + the two 4.11 chat_settings migrations + the history table and \
         seed on the planted base (the column is `not needed` on a table born at its final \
         shape — P4.D315)"
    );
    let build_ran: Vec<&str> = build[0]["ran"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert!(
        build_ran.contains(&CHAT_WARDROBE_HISTORY[0])
            && !build_ran.contains(&CHAT_WARDROBE_HISTORY[1])
            && build_ran.contains(&CHAT_WARDROBE_HISTORY[2]),
        "v4's build ran the history table + seed, not the column: {build_ran:?}"
    );
    // P4.D304: v4 stamped the two 4.11 ids on its own migrated base; v5 never
    // stamps them on any base (R-B).
    for id in CHAT_SETTINGS_4_11 {
        assert!(
            v4_ledger.iter().any(|(i, _)| i == id),
            "v4's build stamped {id}"
        );
    }
    let row_of = |mode: &str| -> &Value {
        v4.iter()
            .find(|r| r["mode"] == mode)
            .unwrap_or_else(|| panic!("no v4 row for mode {mode}"))
    };
    let ran_of = |row: &Value| -> Vec<String> {
        row["ran"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect()
    };
    let mut failures = Vec::new();
    // v4's row for the ping-pong id: its re-run on the post-chain planted
    // instance (the twin), the one place v4 writes it.
    let planted_twin_tiers: Option<(String, Value)> =
        span_rows(&row_of("planted-unstamped")["after"]["ledger"])
            .into_iter()
            .find(|(id, _)| id == TIERS);
    for mode in BASES {
        let row = row_of(mode);
        // (1) After v5's chain v4's runner runs NOTHING and changes nothing.
        let ran = ran_of(row);
        if !ran.is_empty() {
            failures.push(format!("{mode}: v4 re-ran {ran:?} after v5's chain"));
        }
        for key in ["master", "tables", "ledger"] {
            if row["before"][key] != row["after"][key] {
                failures.push(format!(
                    "{mode}: v4's runner changed {key}\n  before {}\n  after  {}",
                    row["before"][key], row["after"][key]
                ));
            }
        }
        let v5_rows = span_rows(&row["before"]["ledger"]);
        // P4.D315 (R-C): v5 stamps EXACTLY the history table + seed on a base
        // its chain created them on, each byte-equal (bar the two masked
        // columns) to v4's own row — the planted base against v4's build, the
        // two v5-native bases against v4's re-run on their history twins (the
        // seed; the table stamp is byte-checked on `planted`) — and adds NO row
        // on v4's migrated base (its rows ARE v4's build rows).
        let v5_history: Vec<(String, Value)> = ledger_by_id(&row["before"]["ledger"])
            .into_iter()
            .filter(|(id, _)| CHAT_WARDROBE_HISTORY.contains(&id.as_str()))
            .collect();
        let history_ids: Vec<&str> = v5_history.iter().map(|(id, _)| id.as_str()).collect();
        let want_ids = [CHAT_WARDROBE_HISTORY[0], CHAT_WARDROBE_HISTORY[2]];
        if history_ids != want_ids {
            failures.push(format!(
                "{mode}: the history ledger rows are {history_ids:?}, want {want_ids:?}"
            ));
        }
        if matches!(mode, "planted" | "v4-migrated") {
            for (id, v5_row) in &v5_history {
                match v4_ledger.iter().find(|(i, _)| i == id) {
                    Some((_, v4_row))
                        if serde_json::to_string(v4_row).unwrap()
                            == serde_json::to_string(v5_row).unwrap() => {}
                    other => failures.push(format!(
                        "{mode}: the history stamp for {id} is not v4's build row\n  v5 {v5_row}\n  v4 {other:?}"
                    )),
                }
            }
        }
        if mode != "v4-migrated" {
            // `HISTORY_STAMP_RED_FIRST`: on the history twin v4 re-runs EXACTLY
            // the seed (its stamp is the only thing that stops it); the table
            // and column steps are structural (`shouldRun` false on a present
            // table) — the table stamp is byte-parity with v4's runner, not a
            // once-gate.
            let twin = row_of(&format!("{mode}-history-unstamped"));
            let twin_ran = ran_of(twin);
            if twin_ran != [CHAT_WARDROBE_HISTORY[2]] {
                failures.push(format!(
                    "HISTORY_STAMP_RED_FIRST {mode}: v4 re-ran {twin_ran:?}, want only the seed"
                ));
            }
            if mode != "planted" {
                let v4_seed = ledger_by_id(&twin["after"]["ledger"])
                    .into_iter()
                    .find(|(i, _)| i == CHAT_WARDROBE_HISTORY[2]);
                let v5_seed = v5_history
                    .iter()
                    .find(|(i, _)| i == CHAT_WARDROBE_HISTORY[2]);
                match (v4_seed, v5_seed) {
                    (Some((_, a)), Some((_, b)))
                        if serde_json::to_string(&a).unwrap()
                            == serde_json::to_string(b).unwrap() => {}
                    other => failures.push(format!(
                        "{mode}: v5's seed stamp is not v4's twin row: {other:?}"
                    )),
                }
            }
        }
        // P4.D304 (R-B): v5 stamps neither 4.11 id; only v4's own migrated
        // base carries them (its build ran them).
        if mode != "v4-migrated" {
            for id in CHAT_SETTINGS_4_11 {
                if ledger_by_id(&row["before"]["ledger"])
                    .iter()
                    .any(|(i, _)| i == id)
                {
                    failures.push(format!("{mode}: a ledger row for the unstamped {id}"));
                }
            }
        }
        match mode {
            "planted" => {
                for (id, v5_row) in &v5_rows {
                    // The ping-pong id's v4 row is the twin's (see the header).
                    let v4_row = v4_ledger.iter().find(|(i, _)| i == id).or_else(|| {
                        if id == TIERS {
                            planted_twin_tiers.as_ref()
                        } else {
                            None
                        }
                    });
                    match v4_row {
                        None => {
                            failures.push(format!("planted: v5 stamped {id}, which v4 never ran"))
                        }
                        Some((_, v4_row)) if v4_row != v5_row => {
                            failures.push(format!("planted: {id}\n  v5 {v5_row}\n  v4 {v4_row}"))
                        }
                        _ => {}
                    }
                }
            }
            "v4-migrated" => {
                // v5 adds EXACTLY the ping-pong row (the header), nothing else.
                let (tiers, rest): (Vec<_>, Vec<_>) =
                    v5_rows.iter().cloned().partition(|(id, _)| id == TIERS);
                if rest != span_rows(&build[0]["ledger"]) {
                    failures.push(format!(
                        "v4-migrated: v5 moved the ledger (a duplicate or changed stamp)\n  v5 {:?}\n  v4 {:?}",
                        v5_rows, v4_ledger
                    ));
                }
                match (tiers.as_slice(), &planted_twin_tiers) {
                    ([(_, v5_row)], Some((_, v4_row)))
                        if serde_json::to_string(v4_row).unwrap()
                            == serde_json::to_string(v5_row).unwrap() => {}
                    other => failures.push(format!(
                        "v4-migrated: the ping-pong stamp is not v4's row for {TIERS}\n  {other:?}"
                    )),
                }
                continue;
            }
            _ => {}
        }
        // (2) `STAMP_RULE_RED_FIRST` (both ways): on the `-unstamped` twin v4
        // re-runs EXACTLY the ids v5 stamped (in v4's order) — every stamp is
        // necessary, no unstamped ensure needs one — and the row v4 writes for
        // each is byte-equal to v5's stamp.
        let twin = row_of(&format!("{mode}-unstamped"));
        let mut want: Vec<String> = v5_rows.iter().map(|(id, _)| id.clone()).collect();
        want.sort_by_key(|id| SPAN.iter().position(|s| s == id));
        let twin_ran = ran_of(twin);
        if twin_ran != want {
            failures.push(format!(
                "STAMP_RULE_RED_FIRST {mode}: v4 re-ran {twin_ran:?}, v5 stamped {want:?}"
            ));
        }
        // The bytes: on `planted` v5 DID the work, so v4's re-run on the twin
        // finds none and writes zero counts — that mode's bytes are pinned above
        // against v4's run on the pre-chain base. On the two v5-native modes v5
        // stamped WITHOUT running (the settled chain) or ran over nothing, so
        // v4's row on the twin IS the row for the same migration on the same
        // instance.
        if mode == "planted" {
            continue;
        }
        let v4_rows = span_rows(&twin["after"]["ledger"]);
        for (id, v5_row) in &v5_rows {
            match v4_rows.iter().find(|(i, _)| i == id) {
                Some((_, v4_row))
                    if serde_json::to_string(v4_row).unwrap()
                        == serde_json::to_string(v5_row).unwrap() => {}
                other => failures.push(format!(
                    "{mode}: v5's stamp for {id} is not v4's row\n  v5 {v5_row}\n  v4 {other:?}"
                )),
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
    eprintln!(
        "OK: zero migrations re-run after v5's chain ({} modes); every stamp necessary and v4's bytes.",
        BASES.len()
    );
}
