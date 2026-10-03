//! P4.D248 tier-1 differential — the boot-time structural table check (v4
//! `e5c6bd0c0`, bug 176): `quilltap_core::db::table_shape` against v4's REAL
//! `findTableShapeProblem`, REAL schemas, REAL repository container and REAL
//! `AbstractDedicatedDbRepository.verifyStructure`.
//!
//! Row kinds (`harness/oracle/cases/table-shape.ts`, spec
//! `harness/oracle/fixtures/table-shape-spec.json`):
//!   - `census` — v4's 11 structure-verifiable repositories in container order
//!     with their schema fields, diffed against [`STRUCTURAL_TABLES`]
//!     field-for-field (order included). On a mismatch the test PRINTS the
//!     regenerated literal — the table's generator;
//!   - `shape` — five shapes over every real schema (the SQL each ran is in the
//!     row), replayed through the ported `find_table_shape_problem`;
//!   - `links` — v4's own test repository through both dedicated targets: the
//!     REAL guards' messages through [`unavailable`], and the ensure-error /
//!     fresh / pre-made forms replayed by running v4's own `generateDDL` output
//!     (in the row) on rusqlite — so the SQLite message itself is compared, not
//!     only the template;
//!   - `substrate` — the DDL v4's ensures create on an empty mount index + LLM
//!     logs; its sound shape is checked here.
//!
//! Red-first (P4.D248's lane record): this binary did not compile on `main`
//! (no `db::table_shape`), and the case FAILS TO IMPORT at the baseline pin
//! `f6426e196` (`lib/database/table-shape.ts` does not exist there).
//!
//! Generate the oracle (Node 24, from the v4 checkout — or the pinned worktree):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   $N/node --import tsx $V5W/harness/oracle/cases/table-shape.ts \
//!     > /tmp/oracle-table-shape.ndjson
//! Run:
//!   QT_ORACLE_TABLE_SHAPE=/tmp/oracle-table-shape.ndjson \
//!     cargo test -p quilltap-harness --test table_shape_equivalence -- --nocapture
//!
//! Skips (does not fail) when the env var is unset — the standing gated
//! discipline.

use quilltap_core::db::fallback::error_text;
use quilltap_core::db::table_shape::{
    ensure_failed, find_table_shape_problem, unavailable, Partition, StructuralTable,
    STRUCTURAL_TABLES,
};
use quilltap_core::db::DbError;
use rusqlite::Connection;
use serde_json::Value;

fn rows() -> Option<Vec<Value>> {
    let Ok(path) = std::env::var("QT_ORACLE_TABLE_SHAPE") else {
        eprintln!("SKIP: QT_ORACLE_TABLE_SHAPE unset");
        return None;
    };
    let text = std::fs::read_to_string(&path).expect("read oracle");
    let rows: Vec<Value> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("row"))
        .collect();
    // 11 census + 1 count + 55 shapes + 10 links + 1 substrate + the plants: a
    // truncated regen (the empty-file trap) must not pass vacuously.
    assert!(rows.len() >= 78, "oracle row count {}", rows.len());
    Some(rows)
}

fn of_kind<'a>(rows: &'a [Value], kind: &str) -> Vec<&'a Value> {
    rows.iter().filter(|r| r["kind"] == kind).collect()
}

fn strings(v: &Value) -> Vec<String> {
    v.as_array()
        .expect("array")
        .iter()
        .map(|s| s.as_str().expect("string").to_string())
        .collect()
}

fn partition_of(db_target: &str) -> Partition {
    match db_target {
        "main" => Partition::Main,
        "mountIndex" => Partition::MountIndex,
        "llmLogs" => Partition::LlmLogs,
        other => panic!("unknown dbTarget {other}"),
    }
}

fn table(collection: &str) -> &'static StructuralTable {
    STRUCTURAL_TABLES
        .iter()
        .find(|t| t.collection == collection)
        .unwrap_or_else(|| panic!("{collection} is not in STRUCTURAL_TABLES"))
}

/// The generator: the census rows rendered as the Rust literal.
fn render_census(census: &[&Value]) -> String {
    let mut out = String::from("pub const STRUCTURAL_TABLES: &[StructuralTable] = &[\n");
    for r in census {
        let partition = match r["dbTarget"].as_str().unwrap() {
            "main" => "Main",
            "mountIndex" => "MountIndex",
            "llmLogs" => "LlmLogs",
            other => panic!("unknown dbTarget {other}"),
        };
        out.push_str(&format!(
            "    StructuralTable {{\n        repository: {:?},\n        collection: {:?},\n        partition: Partition::{partition},\n        fields: &[\n",
            r["repository"].as_str().unwrap(),
            r["collection"].as_str().unwrap()
        ));
        for f in strings(&r["fields"]) {
            out.push_str(&format!("            {f:?},\n"));
        }
        out.push_str("        ],\n    },\n");
    }
    out.push_str("];\n");
    out
}

#[test]
fn the_census_is_v4s_container_in_order() {
    let Some(rows) = rows() else { return };
    let census = of_kind(&rows, "census");
    let count = of_kind(&rows, "census-count");
    assert_eq!(count.len(), 1);
    assert_eq!(count[0]["checked"], census.len());

    let matches = census.len() == STRUCTURAL_TABLES.len()
        && census
            .iter()
            .zip(STRUCTURAL_TABLES)
            .enumerate()
            .all(|(i, (r, t))| {
                r["index"] == i
                    && r["repository"] == t.repository
                    && r["collection"] == t.collection
                    && partition_of(r["dbTarget"].as_str().unwrap()) == t.partition
                    && strings(&r["fields"]) == t.fields
            });
    assert!(
        matches,
        "STRUCTURAL_TABLES disagrees with v4's census — regenerate it:\n{}",
        render_census(&census)
    );
    eprintln!("census: {} tables, field-for-field", census.len());
}

#[test]
fn every_shape_over_every_real_schema_matches() {
    let Some(rows) = rows() else { return };
    let shapes = of_kind(&rows, "shape");
    assert_eq!(
        shapes.len(),
        STRUCTURAL_TABLES.len() * 5,
        "five shapes per table"
    );
    let mut failures = Vec::new();
    let mut nulls = 0;
    for r in &shapes {
        let collection = r["collection"].as_str().unwrap();
        let conn = Connection::open_in_memory().unwrap();
        for sql in strings(&r["sql"]) {
            conn.execute_batch(&sql)
                .unwrap_or_else(|e| panic!("{collection}/{}: {sql}: {e}", r["case"]));
        }
        let got = find_table_shape_problem(&conn, collection, table(collection).fields)
            .expect("shape check");
        let want = r["out"].as_str().map(str::to_string);
        if want.is_none() {
            nulls += 1;
        }
        if got != want {
            failures.push(format!(
                "{collection}/{}: v5 {got:?} != v4 {want:?}",
                r["case"]
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    // Exactly the sound-extra case per table answers null — the other four
    // shapes are damage.
    assert_eq!(nulls, STRUCTURAL_TABLES.len());
    eprintln!("shape: {} rows", shapes.len());
}

/// v4's ensure, replayed: the row's own `generateDDL` output (v4's real
/// emitter) run statement by statement — the first failure is the ensure's.
fn replay_ensure(conn: &Connection, ddl: &[String]) -> Result<(), DbError> {
    for sql in ddl {
        conn.execute_batch(sql)?;
    }
    Ok(())
}

#[test]
fn the_links_repository_shapes_match_through_both_targets() {
    let Some(rows) = rows() else { return };
    let links = of_kind(&rows, "links");
    assert_eq!(links.len(), 10, "five cases x two targets");
    let mut failures = Vec::new();
    for r in &links {
        let label = partition_of(r["target"].as_str().unwrap()).label();
        let case = r["case"].as_str().unwrap();
        let want = r["out"].as_str().map(str::to_string);
        let got = match case {
            "uninitialized" | "degraded" => Some(unavailable(
                label,
                r["guardMessage"].as_str().expect("guard message"),
            )),
            _ => {
                let conn = Connection::open_in_memory().unwrap();
                for sql in strings(&r["sql"]) {
                    conn.execute_batch(&sql).unwrap();
                }
                let fields = strings(&r["fields"]);
                let fields: Vec<&str> = fields.iter().map(String::as_str).collect();
                match replay_ensure(&conn, &strings(&r["ddl"])) {
                    Err(e) => Some(ensure_failed("links", label, &error_text(&e))),
                    Ok(()) => find_table_shape_problem(&conn, "links", &fields).expect("shape"),
                }
            }
        };
        if got != want {
            failures.push(format!("{}/{case}: v5 {got:?} != v4 {want:?}", r["target"]));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    eprintln!("links: {} rows", links.len());
}

/// The substrate v4's own ensures create is sound to the ported check, table by
/// table — the false-positive guard every plant row stands on.
#[test]
fn v4s_own_substrate_is_sound_to_the_ported_check() {
    let Some(rows) = rows() else { return };
    let substrate = of_kind(&rows, "substrate");
    assert_eq!(substrate.len(), 1);
    let mount = Connection::open_in_memory().unwrap();
    let llm = Connection::open_in_memory().unwrap();
    for sql in strings(&substrate[0]["mount"]) {
        mount.execute_batch(&sql).unwrap();
    }
    for sql in strings(&substrate[0]["llm"]) {
        llm.execute_batch(&sql).unwrap();
    }
    for t in STRUCTURAL_TABLES {
        let conn = match t.partition {
            Partition::MountIndex => &mount,
            Partition::LlmLogs => &llm,
            Partition::Main => continue,
        };
        assert_eq!(
            find_table_shape_problem(conn, t.collection, t.fields).unwrap(),
            None,
            "{}",
            t.collection
        );
    }
}
