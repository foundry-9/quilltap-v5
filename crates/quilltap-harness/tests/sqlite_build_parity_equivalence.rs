//! P4.D365 — tier-1 build parity: v5's linked SQLite against v4's REAL driver.
//!
//! v4 `8832cfd20` moved its driver to `better-sqlite3-multiple-ciphers` 13
//! (13.0.3 at the `624d0d25c` pin: SQLite3 Multiple Ciphers 2.4.0 on SQLite
//! 3.53.4). By the human's ruling R-A (2026-11-04) v5 follows it: the sys
//! crate vendors the DRIVER's own amalgamation bytes and compiles them with
//! the driver's own `defines.gypi`. This family proves the two builds are the
//! same build, and that each library reads what the other wrote:
//!
//! - `sqlite_version()`, `sqlite_source_id()`, `sqlite3mc_version()` — exact;
//! - `PRAGMA compile_options` — the ORDERED list, exact, with exactly two
//!   named carve classes, both ways: [`ENV_COMPILER`] (the `COMPILER=` row is
//!   toolchain truth — each side has exactly one, its value masked) and
//!   [`HELD`] (each define `build.rs` holds back from `defines.gypi`: present
//!   on v4, ABSENT on v5 — the table fails the moment a hold stops being
//!   needed or a new difference appears);
//! - `PRAGMA cipher` = `chacha20` on a fresh keyed file (sqleet, both sides —
//!   no `cipher=` pragma anywhere);
//! - v4 -> v5: the oracle writes `/tmp/p4d365/v4-written.db` (raw-hex key,
//!   `journal_mode = TRUNCATE`), v5 reads its rows back through the engine's
//!   read path byte for byte;
//! - v5 -> v4: v5's WRITABLE open (`Writer::open_writable`) writes
//!   `/tmp/p4d365/v5-written.db` with the same rows; v4's driver reads them
//!   back, `quick_check`s it, and reports its cipher;
//! - the planted refusals (Tier-2 item 7): a WRONG pepper on each
//!   cross-written file, and a TAMPERED copy of v4's file (16 bytes flipped
//!   inside page 2 — the plant is self-checked on both sides) — v4 and v5
//!   fail the same way with the same engine message.
//!
//! The self-test leg ([`v5_build_constants`]) needs no oracle: it pins v5's
//! own linked constants so the bump is held even where Node is absent.
//!
//! Regenerate + run (Node 24, from the round's pin; TZ irrelevant):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   rm -f /tmp/p4d365/oracle-sqlite-build-parity.ndjson /tmp/p4d365/oracle-sqlite-build-parity-readback.ndjson /tmp/p4d365/v5-written.db
//!   cd ~/source/quilltap-server
//!   $N/npx tsx $V5W/harness/oracle/cases/sqlite-build-parity.ts > /tmp/p4d365/oracle-sqlite-build-parity.ndjson
//!   cd $V5W && QT_SQLITE_BUILD_PARITY_WRITE=1 \
//!     cargo test -p quilltap-harness --test sqlite_build_parity_equivalence -- write_v5_file
//!   cd ~/source/quilltap-server
//!   $N/npx tsx $V5W/harness/oracle/cases/sqlite-build-parity.ts --read /tmp/p4d365/v5-written.db > /tmp/p4d365/oracle-sqlite-build-parity-readback.ndjson
//!   cd $V5W && QT_ORACLE_SQLITE_BUILD_PARITY=/tmp/p4d365/oracle-sqlite-build-parity.ndjson \
//!     QT_ORACLE_SQLITE_BUILD_PARITY_READBACK=/tmp/p4d365/oracle-sqlite-build-parity-readback.ndjson \
//!     cargo test -p quilltap-harness --test sqlite_build_parity_equivalence -- parity --nocapture

use std::path::Path;

use quilltap_core::db::Writer;
use quilltap_core::test_support::open_readonly;
use rusqlite::Connection;
use serde_json::{json, Value};

/// The harness corpus test pepper (synthetic) — the oracle's `TEST_PEPPER`.
const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
/// The wrong-key plant — the oracle's `WRONG_PEPPER` (also synthetic).
const WRONG_PEPPER: &str = "d3Jvbmd3cm9uZ3dyb25nd3Jvbmd3cm9uZ3dyb25nMDI=";

const V5_WRITTEN: &str = "/tmp/p4d365/v5-written.db";

/// `ENV_COMPILER` — the one `compile_options` row that is toolchain truth,
/// not build configuration: `COMPILER=<cc>-<version>`. Each side must carry
/// exactly one; its value is masked before the ordered compare.
const ENV_COMPILER: &str = "COMPILER=";

/// `HELD` — the `compile_options` rows (SQLite's spelling, without the
/// `SQLITE_` prefix) of the defines `build.rs` holds back from v4's
/// `defines.gypi`. Must equal `build.rs`'s own `HELD` table. Each is present
/// on v4 and absent on v5, both ways.
///
/// `SQLITE_DEFAULT_FK_HELD`: `DEFAULT_FOREIGN_KEYS` (see `build.rs`).
const HELD: &[&str] = &["DEFAULT_FOREIGN_KEYS"];

const V5_SQLITE_VERSION: &str = "3.53.4";
const V5_SQLITE3MC_VERSION: &str = "SQLite3 Multiple Ciphers 2.4.0";

fn key_hex(pepper: &str) -> String {
    quilltap_core::dbkey::pepper_b64_to_key_hex(pepper).expect("a valid test pepper")
}

fn scalar(conn: &Connection, sql: &str) -> String {
    conn.query_row(sql, [], |r| r.get::<_, String>(0))
        .unwrap_or_else(|e| panic!("{sql}: {e}"))
}

fn compile_options(conn: &Connection) -> Vec<String> {
    let mut stmt = conn.prepare("PRAGMA compile_options").unwrap();
    let rows = stmt.query_map([], |r| r.get::<_, String>(0)).unwrap();
    rows.map(Result::unwrap).collect()
}

/// `PRAGMA cipher` on a fresh file keyed through the raw-hex form, with a
/// table written so the codec is live.
fn fresh_file_cipher() -> String {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("v5-cipher-probe.db");
    let conn = Connection::open(&path).unwrap();
    conn.pragma_update(None, "key", format!("x'{}'", key_hex(PEPPER)))
        .unwrap();
    conn.execute_batch("CREATE TABLE t (x)").unwrap();
    scalar(&conn, "PRAGMA cipher")
}

/// The four rows both sides write — byte-identical to the oracle's inserts.
fn insert_parity_rows(conn: &Connection) {
    conn.execute_batch("CREATE TABLE parity (id INTEGER PRIMARY KEY, t TEXT, r REAL, b BLOB)")
        .unwrap();
    let mut ins = conn
        .prepare("INSERT INTO parity (id, t, r, b) VALUES (?1, ?2, ?3, ?4)")
        .unwrap();
    ins.execute(rusqlite::params![
        1,
        "Quilltap \u{2014} the quill taps; caf\u{e9} \u{1F58B}",
        Option::<f64>::None,
        Option::<Vec<u8>>::None
    ])
    .unwrap();
    ins.execute(rusqlite::params![
        2,
        Option::<String>::None,
        std::f64::consts::PI,
        Option::<Vec<u8>>::None
    ])
    .unwrap();
    ins.execute(rusqlite::params![
        3,
        Option::<String>::None,
        Option::<f64>::None,
        vec![0x00u8, 0x01, 0x7f, 0x80, 0xfe, 0xff]
    ])
    .unwrap();
    ins.execute(rusqlite::params![
        4,
        "x".repeat(9000),
        -0.5,
        vec![0xabu8; 3000]
    ])
    .unwrap();
}

/// The parity table as the oracle's `readRows` renders it (BLOBs as hex).
fn read_rows(conn: &Connection) -> Value {
    let mut stmt = conn
        .prepare(
            "SELECT id, t, r, b, typeof(t) AS tt, typeof(r) AS tr, typeof(b) AS tb \
             FROM parity ORDER BY id",
        )
        .unwrap();
    let rows = stmt
        .query_map([], |r| {
            Ok(json!({
                "id": r.get::<_, i64>(0)?,
                "t": r.get::<_, Option<String>>(1)?,
                "r": r.get::<_, Option<f64>>(2)?,
                "b": r.get::<_, Option<Vec<u8>>>(3)?.map(hex::encode),
                "tt": r.get::<_, String>(4)?,
                "tr": r.get::<_, String>(5)?,
                "tb": r.get::<_, String>(6)?,
            }))
        })
        .unwrap();
    Value::Array(rows.map(Result::unwrap).collect())
}

/// better-sqlite3's `err.code` for the extended result codes this family can
/// meet (the full transcription lives in the CLI's `db integrity`).
fn code_name(e: &rusqlite::Error) -> String {
    match e {
        rusqlite::Error::SqliteFailure(f, _) => match f.extended_code {
            11 => "SQLITE_CORRUPT".to_string(),
            26 => "SQLITE_NOTADB".to_string(),
            n => format!("UNKNOWN_SQLITE_ERROR_{n}"),
        },
        _ => "error".to_string(),
    }
}

fn message(e: &rusqlite::Error) -> String {
    match e {
        rusqlite::Error::SqliteFailure(_, Some(m)) => m.clone(),
        other => other.to_string(),
    }
}

fn error_json(e: &rusqlite::Error) -> Value {
    json!({ "code": code_name(e), "message": message(e) })
}

/// The oracle's `tryRead`: a read-only open with `pepper`, the first
/// statement's outcome.
fn try_read(path: &Path, pepper: &str) -> Value {
    let conn = open_readonly(path, pepper);
    match conn.query_row("SELECT count(*) FROM sqlite_master", [], |r| {
        r.get::<_, i64>(0)
    }) {
        Ok(n) => json!({ "ok": true, "sqliteMasterCount": n }),
        Err(e) => {
            let mut v = json!({ "ok": false });
            let o = v.as_object_mut().unwrap();
            for (k, val) in error_json(&e).as_object().unwrap() {
                o.insert(k.clone(), val.clone());
            }
            v
        }
    }
}

fn load(path: &str) -> (Value, std::collections::BTreeMap<String, Value>) {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));
    let mut lines = text.lines().filter(|l| !l.trim().is_empty());
    let probe: Value = serde_json::from_str(lines.next().expect("oracle file is empty")).unwrap();
    assert_eq!(
        probe["kind"], "probe",
        "{path}: the first row must be the probe"
    );
    let mut rows = std::collections::BTreeMap::new();
    for line in lines {
        let v: Value = serde_json::from_str(line).unwrap_or_else(|e| panic!("bad row {e}: {line}"));
        let id = v["id"].as_str().expect("row id").to_string();
        assert!(
            rows.insert(id.clone(), v).is_none(),
            "{path}: duplicate row {id}"
        );
    }
    (probe, rows)
}

/// Strip a row down to the fields after `kind` / `id` / `path`.
fn payload(row: &Value) -> Value {
    let mut o = row.as_object().unwrap().clone();
    for k in ["kind", "id", "path"] {
        o.remove(k);
    }
    Value::Object(o)
}

/// The ordered `compile_options` compare with the two named carve classes.
fn compare_compile_options(v4: &[String], v5: &[String]) -> Vec<String> {
    let mut failures = Vec::new();
    let mask = |list: &[String], side: &str, failures: &mut Vec<String>| -> Vec<String> {
        let n = list.iter().filter(|o| o.starts_with(ENV_COMPILER)).count();
        if n != 1 {
            failures.push(format!(
                "ENV_COMPILER: {side} carries {n} `COMPILER=` rows, want 1"
            ));
        }
        list.iter()
            .map(|o| {
                if o.starts_with(ENV_COMPILER) {
                    "COMPILER=<masked>".to_string()
                } else {
                    o.clone()
                }
            })
            .collect()
    };
    let v4m = mask(v4, "v4", &mut failures);
    let v5m = mask(v5, "v5", &mut failures);
    for held in HELD {
        if !v4m.iter().any(|o| o == held) {
            failures.push(format!(
                "HELD {held}: absent on v4 — the hold is stale, remove it"
            ));
        }
        if v5m.iter().any(|o| o == held) {
            failures.push(format!(
                "HELD {held}: present on v5 — the hold no longer holds"
            ));
        }
    }
    let v4_kept: Vec<String> = v4m
        .into_iter()
        .filter(|o| !HELD.contains(&o.as_str()))
        .collect();
    if v4_kept != v5m {
        let only_v4: Vec<&String> = v4_kept.iter().filter(|o| !v5m.contains(o)).collect();
        let only_v5: Vec<&String> = v5m.iter().filter(|o| !v4_kept.contains(o)).collect();
        failures.push(format!(
            "compile_options differ:\n  v4 only: {only_v4:?}\n  v5 only: {only_v5:?}\n  v4: {v4_kept:?}\n  v5: {v5m:?}"
        ));
    }
    failures
}

/// The self-test leg: v5's own linked constants, no oracle needed.
#[test]
fn v5_build_constants() {
    assert_eq!(
        rusqlite::version(),
        V5_SQLITE_VERSION,
        "rusqlite::version()"
    );
    let mem = Connection::open_in_memory().unwrap();
    assert_eq!(scalar(&mem, "select sqlite_version()"), V5_SQLITE_VERSION);
    assert_eq!(
        scalar(&mem, "select sqlite3mc_version()"),
        V5_SQLITE3MC_VERSION
    );
    assert_eq!(
        fresh_file_cipher(),
        "chacha20",
        "the default cipher must stay sqleet"
    );
    eprintln!("OK: v5 links SQLite {V5_SQLITE_VERSION} / {V5_SQLITE3MC_VERSION}, cipher chacha20");
}

/// The v5 -> v4 write step (recipe step 2): v5's WRITABLE open writes the
/// parity rows for the oracle's `--read` run.
#[test]
fn write_v5_file() {
    if std::env::var("QT_SQLITE_BUILD_PARITY_WRITE").as_deref() != Ok("1") {
        eprintln!("SKIP: QT_SQLITE_BUILD_PARITY_WRITE unset (recipe step 2 only)");
        return;
    }
    std::fs::create_dir_all("/tmp/p4d365").unwrap();
    for p in [V5_WRITTEN.to_string(), format!("{V5_WRITTEN}-journal")] {
        let _ = std::fs::remove_file(p);
    }
    let w = Writer::open_writable(Path::new(V5_WRITTEN), PEPPER).expect("v5 writable open");
    insert_parity_rows(w.connection());
    assert_eq!(scalar(w.connection(), "PRAGMA cipher"), "chacha20");
    drop(w);
    eprintln!("OK: wrote {V5_WRITTEN}");
}

/// The oracle leg: every build fact, the v4 -> v5 read, the refusals.
#[test]
fn parity_against_v4_driver() {
    let Ok(path) = std::env::var("QT_ORACLE_SQLITE_BUILD_PARITY") else {
        eprintln!("SKIP: QT_ORACLE_SQLITE_BUILD_PARITY unset (see the test header)");
        return;
    };
    let (probe, rows) = load(&path);
    eprintln!("oracle probe: {probe}");
    let row = |id: &str| {
        rows.get(id)
            .unwrap_or_else(|| panic!("oracle row {id} missing"))
            .clone()
    };
    let mut failures: Vec<String> = Vec::new();
    let mut check = |id: &str, v4: Value, v5: Value| {
        if v4 != v5 {
            failures.push(format!("{id}:\n  v4 {v4}\n  v5 {v5}"));
        } else {
            eprintln!("  ok {id}");
        }
    };

    let mem = Connection::open_in_memory().unwrap();
    check(
        "sqliteVersion",
        row("sqliteVersion")["value"].clone(),
        json!(scalar(&mem, "select sqlite_version()")),
    );
    check(
        "sourceId",
        row("sourceId")["value"].clone(),
        json!(scalar(&mem, "select sqlite_source_id()")),
    );
    check(
        "sqlite3mcVersion",
        row("sqlite3mcVersion")["value"].clone(),
        json!(scalar(&mem, "select sqlite3mc_version()")),
    );
    check(
        "cipher",
        row("cipher")["value"].clone(),
        json!(fresh_file_cipher()),
    );

    // v4 -> v5: the engine's read path reads v4's rows byte for byte.
    let v4w = row("v4Written");
    let v4_path = Path::new(v4w["path"].as_str().unwrap()).to_path_buf();
    let conn = open_readonly(&v4_path, PEPPER);
    check("v4Written", v4w["rows"].clone(), read_rows(&conn));
    check(
        "v4WrittenCipher",
        row("v4WrittenCipher")["value"].clone(),
        json!(scalar(&conn, "PRAGMA cipher")),
    );
    drop(conn);
    check(
        "v4WrittenWrongKey",
        payload(&row("v4WrittenWrongKey")),
        try_read(&v4_path, WRONG_PEPPER),
    );

    // The tampered copy: self-check the plant on THIS side too.
    let t = row("v4Tampered");
    let t_path = Path::new(t["path"].as_str().unwrap()).to_path_buf();
    let at = t["offset"].as_u64().unwrap() as usize;
    let orig = std::fs::read(&v4_path).unwrap();
    let tampered = std::fs::read(&t_path).unwrap();
    assert_eq!(
        orig.len(),
        tampered.len(),
        "the tampered copy must be the same size"
    );
    assert_ne!(
        orig[at..at + 16],
        tampered[at..at + 16],
        "the tamper plant did not plant"
    );
    assert_eq!(
        orig[..at],
        tampered[..at],
        "the plant touched bytes before its offset"
    );
    let tc = open_readonly(&t_path, PEPPER);
    let mut v5t = json!({ "offset": at });
    {
        let o = v5t.as_object_mut().unwrap();
        let qc: Result<Vec<String>, rusqlite::Error> = (|| {
            let mut s = tc.prepare("PRAGMA quick_check")?;
            let r = s.query_map([], |r| r.get::<_, String>(0))?;
            r.collect()
        })();
        match qc {
            Ok(v) => o.insert("quickCheck".into(), json!(v)),
            Err(e) => o.insert("quickCheckError".into(), error_json(&e)),
        };
        match tc.query_row(
            "SELECT count(*) AS n, sum(length(t)) AS l FROM parity",
            [],
            |r| Ok(json!({ "n": r.get::<_, i64>(0)?, "l": r.get::<_, Option<i64>>(1)? })),
        ) {
            Ok(v) => o.insert("scan".into(), v),
            Err(e) => o.insert("scanError".into(), error_json(&e)),
        };
    }
    check("v4Tampered", payload(&t), v5t);

    let v4_opts: Vec<String> = row("compileOptions")["value"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    let v5_opts = compile_options(&mem);
    let co = compare_compile_options(&v4_opts, &v5_opts);
    if co.is_empty() {
        eprintln!(
            "  ok compileOptions ({} rows; HELD {HELD:?})",
            v5_opts.len()
        );
    }
    failures.extend(co);

    assert!(
        failures.is_empty(),
        "{} build-parity failure(s):\n{}",
        failures.len(),
        failures.join("\n")
    );
    eprintln!(
        "OK: sqlite build parity — {} oracle rows, zero diffs",
        rows.len()
    );
}

/// The v5 -> v4 leg: v4's driver read what v5's writable open wrote.
#[test]
fn parity_v5_written_reads_on_v4_driver() {
    let (Ok(main), Ok(readback)) = (
        std::env::var("QT_ORACLE_SQLITE_BUILD_PARITY"),
        std::env::var("QT_ORACLE_SQLITE_BUILD_PARITY_READBACK"),
    ) else {
        eprintln!("SKIP: QT_ORACLE_SQLITE_BUILD_PARITY(_READBACK) unset (see the test header)");
        return;
    };
    let (_, main_rows) = load(&main);
    let (probe, rows) = load(&readback);
    eprintln!("readback probe: {probe}");
    let row = |id: &str| {
        rows.get(id)
            .unwrap_or_else(|| panic!("readback row {id} missing"))
            .clone()
    };
    let mut failures: Vec<String> = Vec::new();
    let mut check = |id: &str, want: Value, got: Value| {
        if want != got {
            failures.push(format!("{id}:\n  want {want}\n  got  {got}"));
        } else {
            eprintln!("  ok {id}");
        }
    };
    let rb = row("readbackRows");
    assert_eq!(rb["path"], V5_WRITTEN, "the readback must read v5's file");
    // Both sides inserted the same four rows, so v4's read of v5's file must
    // equal v4's read of its own.
    check(
        "readbackRows",
        main_rows["v4Written"]["rows"].clone(),
        rb["rows"].clone(),
    );
    check(
        "readbackQuickCheck",
        json!(["ok"]),
        row("readbackQuickCheck")["value"].clone(),
    );
    check(
        "readbackCipher",
        json!("chacha20"),
        row("readbackCipher")["value"].clone(),
    );
    // The same file refuses the wrong pepper the same way on both sides.
    check(
        "readbackWrongKey",
        payload(&row("readbackWrongKey")),
        try_read(Path::new(V5_WRITTEN), WRONG_PEPPER),
    );
    assert!(
        failures.is_empty(),
        "{} readback failure(s):\n{}",
        failures.len(),
        failures.join("\n")
    );
    eprintln!("OK: v5 -> v4 readback — {} rows, zero diffs", rows.len());
}
