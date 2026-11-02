//! P4.D341 — the roleplay-template unmarked-text boot ensure
//! (`db::roleplay_templates_unmarked_repair`) against v4's REAL
//! `add-roleplay-template-unmarked-fields-v1` (v4 `778b97b2b`, the
//! TWENTY-FIRST D23 move) through v4's REAL `MigrationRunner` narrowed to it,
//! in five shapes (`harness/oracle/cases/roleplay-templates-unmarked-ensure.ts`
//! — its header names them): a (both columns absent), b (no table),
//! c (migrated), d (generateDDL-current), e (only `markdownAsterisks` absent).
//!
//! Comparands per shape, byte-exact: `PRAGMA table_info`, `sqlite_master.sql`,
//! every row's `unmarkedStyle` / `markdownAsterisks` (`quote()` + `typeof`),
//! and on (a) / (d) the FIRST-BOOT REWRITE under the ensure: v4's REAL
//! `seedBuiltInTemplates()` vs v5's `builtin_templates::seed_built_in_templates`
//! — every built-in row by name (`Novel` inserted; `Standard`'s bare `*` →
//! `"*"`; `Quilltap RP`'s pair kept as JSON text) with its `narrationDelimiters`
//! / `unmarkedStyle` / `markdownAsterisks` `quote()`d + storage class. The
//! ledger is `UNSTAMPED_COLUMN_ENSURES`, both ways.
//!
//! Generate (Node 24, from the pinned v4 worktree):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   rm -rf /tmp/qt-rt-unmarked-ensure /tmp/oracle-rt-unmarked-ensure.ndjson
//!   QT_FIXTURE_OUT_DIR=/tmp/qt-rt-unmarked-ensure $N/npx tsx $V5W/harness/oracle/cases/roleplay-templates-unmarked-ensure.ts > /tmp/oracle-rt-unmarked-ensure.ndjson
//! Run:
//!   cd $V5W
//!   QT_ORACLE_RT_UNMARKED_ENSURE=/tmp/oracle-rt-unmarked-ensure.ndjson QT_FIXTURE_RT_UNMARKED_ENSURE_DIR=/tmp/qt-rt-unmarked-ensure cargo test -p quilltap-harness --test roleplay_templates_unmarked_ensure_equivalence -- --nocapture

mod keystone_ensure_support;

use std::path::Path;

use keystone_ensure_support as kit;
use quilltap_core::db::roleplay_templates_unmarked_repair::{
    ensure_roleplay_templates_unmarked_fields, MIGRATION_ID,
};
use quilltap_core::db::Writer;
use serde_json::{json, Map, Value};

const SEEDED_SQL: &str =
    "SELECT name, quote(\"narrationDelimiters\"), typeof(\"narrationDelimiters\"), \
     quote(\"unmarkedStyle\"), typeof(\"unmarkedStyle\"), quote(\"markdownAsterisks\"), \
     typeof(\"markdownAsterisks\") FROM roleplay_templates WHERE \"isBuiltIn\" = 1 ORDER BY name";

fn seeded(c: &rusqlite::Connection) -> Value {
    let keys = [
        "name",
        "narrationDelimiters",
        "narrationDelimitersType",
        "unmarkedStyle",
        "unmarkedStyleType",
        "markdownAsterisks",
        "markdownAsterisksType",
    ];
    let mut stmt = c.prepare(SEEDED_SQL).unwrap();
    let rows: Vec<Value> = stmt
        .query_map([], |r| {
            let mut m = Map::new();
            for (i, k) in keys.iter().enumerate() {
                m.insert((*k).into(), json!(r.get::<_, String>(i)?));
            }
            Ok(Value::Object(m))
        })
        .unwrap()
        .map(Result::unwrap)
        .collect();
    Value::Array(rows)
}

#[test]
fn the_unmarked_ensure_matches_v4s_migration_in_five_shapes() {
    let (Some(oracle), Some(dir)) = (
        std::env::var_os("QT_ORACLE_RT_UNMARKED_ENSURE"),
        std::env::var_os("QT_FIXTURE_RT_UNMARKED_ENSURE_DIR"),
    ) else {
        eprintln!(
            "SKIP: set QT_ORACLE_RT_UNMARKED_ENSURE and QT_FIXTURE_RT_UNMARKED_ENSURE_DIR (see header)."
        );
        return;
    };
    let mut modes = Vec::new();
    for v4 in kit::read_ndjson(&oracle) {
        let mode = v4["mode"].as_str().unwrap().to_string();
        let report = kit::report(&v4);
        let runs = matches!(mode.as_str(), "a" | "e");
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
        let work = scratch.path().join("work.db");
        std::fs::copy(
            Path::new(&dir).join(format!("rt-unmarked-{mode}.db")),
            &work,
        )
        .unwrap();
        let w = Writer::open_writable(&work, kit::PEPPER).unwrap();
        let c = w.connection();
        let added = ensure_roleplay_templates_unmarked_fields(c).expect("ensure");
        let want_added = match mode.as_str() {
            "a" => 2,
            "e" => 1,
            _ => 0,
        };
        assert_eq!(added, want_added, "[{mode}] the ensure's answer");
        kit::assert_bytes(
            &kit::table_info(c, "roleplay_templates"),
            &v4["tableInfo"],
            &format!("[{mode}] table_info"),
        );
        kit::assert_bytes(
            &json!(kit::table_sql(c, "roleplay_templates")),
            &v4["sql"],
            &format!("[{mode}] sql"),
        );
        kit::assert_bytes(
            &kit::cells(
                c,
                "roleplay_templates",
                &["unmarkedStyle", "markdownAsterisks"],
            ),
            &v4["rows"],
            &format!("[{mode}] cells"),
        );
        assert!(kit::ledger(c).is_empty(), "[{mode}] v5 stamped");
        let got_seeded = if matches!(mode.as_str(), "a" | "d") {
            quilltap_core::services::builtin_templates::seed_built_in_templates(c)
                .expect("v5's seeder under the ensure");
            seeded(c)
        } else {
            Value::Null
        };
        kit::assert_bytes(
            &got_seeded,
            &v4["seeded"],
            &format!("[{mode}] the first-boot rewrite"),
        );
        if mode == "a" {
            assert!(
                v4["sql"].as_str().unwrap().ends_with(
                    ", \"unmarkedStyle\" TEXT, \"markdownAsterisks\" INTEGER DEFAULT 1)"
                ),
                "marker"
            );
            assert!(
                serde_json::to_string(&v4["seeded"])
                    .unwrap()
                    .contains(r#"'\"*\"'"#),
                "marker: Standard's narration rewritten to \"*\""
            );
        }
        modes.push(mode);
    }
    assert_eq!(modes, ["a", "b", "c", "d", "e"], "every shape ran");
    eprintln!("OK: the unmarked ensure (and the seeder under it) matched v4 in shapes {modes:?}.");
}
