//! P4.D256 helper-level differential: `services::wardrobe_wear_history`
//! (`attach_wear`, `resolve_wearers`, `build_wear_history_payload`) vs v4's
//! REAL `lib/wardrobe/wear-history.ts` exports (`3ee3b1342`), each case over a
//! FRESH copy of the committed `wardrobe-routes-{main,mount}.db` pair.
//!
//! The corpus is `harness/oracle/fixtures/wardrobe-wear-history.json#cases`,
//! shared with the oracle. A `ledger: true` case gets `wardrobe_wear_stats`
//! (v4: its migration's own `WARDROBE_WEAR_STATS_DDL`; v5:
//! `test_support::ensure_wear_ledger_on`); `plants` run on the main copy
//! first, on both sides.
//!
//! Compared EXACTLY: each result's JSON (key ORDER included — `wear` lands
//! after `origin`, the payload's three keys and the history's five), and the
//! lines each side logged while the helper ran: every WARN / ERROR plus the two
//! wear-history DEBUG lines (`Attached wear summaries to wardrobe read`,
//! `Built wear history`), rendered `LEVEL message k=v …` with `error` dropped
//! on both sides (each driver's own text). v4's backend `Raw query failed` is
//! v5-unported by standing convention and dropped from v4's side.
//!
//! R-F's measurement rides `resolve_labels_*` / `payload_full`: a corrupt
//! `characters` row (a BLOB `name`) makes v4's `findByIdRaw` answer `null`
//! after `Data validation failed` + `Error finding entity by ID` — the
//! departed arm, NEVER the `Could not read wearer; labelling as departed` WARN
//! (unreachable through v4's real code; v5 never logs it either — pinned here
//! by the line comparison).
//!
//! Generate the oracle (see `harness/oracle/cases/wardrobe-wear-history.test.ts`),
//! then:
//!   QT_ORACLE_WARDROBE_WEAR_HISTORY=/tmp/oracle-wardrobe-wear-history.ndjson \
//!     cargo test -p quilltap-harness --test wardrobe_wear_history_equivalence -- --nocapture

use std::collections::HashMap;
use std::path::PathBuf;

use quilltap_core::db::wardrobe_wear_stats::WardrobeWearer;
use quilltap_core::db::Writer;
use quilltap_core::services::wardrobe_wear_history::{
    attach_wear, build_wear_history_payload, resolve_wearers, WearerKind,
};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
struct Plant {
    sql: String,
    params: Vec<Value>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CaseEntry {
    name: String,
    kind: String,
    #[serde(default)]
    ledger: bool,
    #[serde(default)]
    plants: Vec<Plant>,
    #[serde(default)]
    items: Vec<Value>,
    #[serde(default)]
    wearers: Vec<Value>,
    #[serde(default)]
    avatars: bool,
    #[serde(default)]
    item_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    test_pepper_base64: String,
    cases: Vec<CaseEntry>,
}

const DEBUG_MESSAGES: &[&str] = &[
    "Attached wear summaries to wardrobe read",
    "Built wear history",
];
const UNPORTED_V4_LINES: &[&str] = &[
    "Raw query failed",
    "SQLite find error",
    "SQLite findOne error",
];

/// ⚠ RECORDED v4-only line, pinned BOTH ways (the
/// `scenario_builder_mount_pool_equivalence` `V4_ONLY_VALIDATION` precedent):
/// v4's `characters.findByIdRaw` VALIDATES the corrupt row and logs `Data
/// validation failed {collection: characters}` before the fallback's `Error
/// finding entity by ID`; v5's raw character read
/// (`db::characters_read::find_by_id_raw`, outside this lane's files) fails at
/// the cell decode and logs only the second line. The VALUES agree (both take
/// the departed arm). If v5 converges, the `v5 lacks it` half trips — retire
/// the row then. Keyed by case name.
const V4_ONLY_CHARACTER_VALIDATION: &[&str] = &[
    "resolve_labels_no_avatars",
    "resolve_labels_with_avatars",
    "payload_full",
];
const CHARACTER_VALIDATION_LINE: &str = "ERROR Data validation failed collection=characters";

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../quilltap-web/tests/fixtures")
}
fn spec_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/wardrobe-wear-history.json")
}

fn to_sql(v: &Value) -> rusqlite::types::Value {
    match v {
        Value::Null => rusqlite::types::Value::Null,
        Value::String(s) => rusqlite::types::Value::Text(s.clone()),
        Value::Number(n) => rusqlite::types::Value::Integer(n.as_i64().expect("integer param")),
        other => panic!("unsupported plant param {other}"),
    }
}

/// v4's `{level, message, fields}` → `LEVEL message k=v …`.
fn v4_line(entry: &Value) -> Option<String> {
    let message = entry["message"].as_str().unwrap();
    if UNPORTED_V4_LINES.contains(&message) {
        return None;
    }
    let mut line = format!(
        "{} {message}",
        entry["level"].as_str().unwrap().to_uppercase()
    );
    for f in entry["fields"].as_array().unwrap() {
        line.push_str(&format!(
            " {}={}",
            f[0].as_str().unwrap(),
            f[1].as_str().unwrap()
        ));
    }
    Some(line)
}

/// v5's captured `LEVEL target message k=v …` → `LEVEL message k=v …`, with
/// `error=…` dropped and only the compared lines kept.
fn v5_line(line: &str) -> Option<String> {
    let mut parts = line.splitn(3, ' ');
    let level = parts.next()?;
    let _target = parts.next()?;
    let rest = parts.next().unwrap_or_default();
    // Split before every ` key=` boundary (messages carry no `=`).
    let re = regex::Regex::new(r" (?:[A-Za-z_]+)=").unwrap();
    let mut cuts: Vec<usize> = re.find_iter(rest).map(|m| m.start()).collect();
    cuts.push(rest.len());
    let message = &rest[..cuts[0]];
    if level == "DEBUG" && !DEBUG_MESSAGES.contains(&message) {
        return None;
    }
    if level != "DEBUG" && level != "WARN" && level != "ERROR" {
        return None;
    }
    let mut out = format!("{level} {message}");
    for w in cuts.windows(2) {
        let field = &rest[w[0]..w[1]];
        if field.starts_with(" error=") {
            continue;
        }
        out.push_str(field);
    }
    Some(out)
}

fn kind_str(k: WearerKind) -> &'static str {
    match k {
        WearerKind::Character => "character",
        WearerKind::Departed => "departed",
        WearerKind::Unattributed => "unattributed",
    }
}

#[test]
fn wardrobe_wear_history_equivalence() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_WARDROBE_WEAR_HISTORY") else {
        eprintln!("SKIP: set QT_ORACLE_WARDROBE_WEAR_HISTORY (see test header).");
        return;
    };
    let spec: Spec =
        serde_json::from_str(&std::fs::read_to_string(spec_path()).unwrap()).expect("spec");
    let mut oracle: HashMap<String, Value> = HashMap::new();
    for line in std::fs::read_to_string(&oracle_path).unwrap().lines() {
        if line.trim().is_empty() {
            continue;
        }
        let v: Value = serde_json::from_str(line).unwrap();
        oracle.insert(v["name"].as_str().unwrap().to_string(), v);
    }
    assert_eq!(
        oracle.len(),
        spec.cases.len(),
        "oracle row count != corpus case count — regenerate the NDJSON"
    );

    let mut failures: Vec<String> = Vec::new();
    for case in &spec.cases {
        let scratch = tempfile::Builder::new()
            .prefix(&format!("qt-wwh-{}-", case.name))
            .tempdir()
            .unwrap();
        let main_path = scratch.path().join("main.db");
        let mount_path = scratch.path().join("mount.db");
        std::fs::copy(fixtures_dir().join("wardrobe-routes-main.db"), &main_path).unwrap();
        std::fs::copy(fixtures_dir().join("wardrobe-routes-mount.db"), &mount_path).unwrap();
        let main_w = Writer::open_writable(&main_path, &spec.test_pepper_base64).unwrap();
        let mount_w = Writer::open_writable(&mount_path, &spec.test_pepper_base64).unwrap();
        let main = main_w.connection();
        let mount = mount_w.connection();
        if case.ledger {
            quilltap_core::test_support::ensure_wear_ledger_on(main);
        }
        for p in &case.plants {
            let params: Vec<rusqlite::types::Value> = p.params.iter().map(to_sql).collect();
            main.execute(&p.sql, rusqlite::params_from_iter(params))
                .unwrap_or_else(|e| panic!("{}: plant `{}`: {e}", case.name, p.sql));
        }

        let (out, lines) =
            quilltap_core::test_support::captured_with(|| match case.kind.as_str() {
                "attachWear" => Value::Array(attach_wear(main, case.items.clone())),
                "resolveWearers" => {
                    let wearers: Vec<WardrobeWearer> = case
                        .wearers
                        .iter()
                        .map(|w| WardrobeWearer {
                            character_id: w["characterId"].as_str().map(str::to_string),
                            wear_count: 0,
                            first_worn_at: String::new(),
                            last_worn_at: String::new(),
                            last_worn_chat_id: None,
                        })
                        .collect();
                    Value::Array(
                        resolve_wearers(main, mount, &wearers, case.avatars)
                            .into_iter()
                            .map(|r| {
                                json!({
                                    "characterId": r.character_id,
                                    "name": r.name,
                                    "avatarUrl": r.avatar_url,
                                    "kind": kind_str(r.kind),
                                })
                            })
                            .collect(),
                    )
                }
                "buildWearHistoryPayload" => {
                    build_wear_history_payload(main, mount, case.item_id.as_deref().unwrap())
                }
                other => panic!("unknown kind {other}"),
            });

        let want = &oracle[&case.name];
        // EXACT, key order included (serde_json `preserve_order`).
        let got_s = serde_json::to_string(&out).unwrap();
        let want_s = serde_json::to_string(&want["out"]).unwrap();
        if got_s != want_s {
            failures.push(format!("{}: out\n  v4: {want_s}\n  v5: {got_s}", case.name));
        }
        // v4's three wear-history catches are UNREACHABLE (every read under
        // them is fallback-mode) — pinned NEGATIVE on every case, both sides.
        for dead in [
            "Could not read wearer; labelling as departed",
            "Could not resolve wearer avatar",
            "Could not read last-worn chat",
        ] {
            if lines.iter().any(|l| l.contains(dead)) || want["logs"].to_string().contains(dead) {
                failures.push(format!("{}: the unreachable `{dead}` fired", case.name));
            }
        }
        let mut want_lines: Vec<String> = want["logs"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(v4_line)
            .collect();
        let got_lines: Vec<String> = lines.iter().filter_map(|l| v5_line(l)).collect();
        if V4_ONLY_CHARACTER_VALIDATION.contains(&case.name.as_str()) {
            let before = want_lines.len();
            want_lines.retain(|l| l != CHARACTER_VALIDATION_LINE);
            if want_lines.len() + 1 != before {
                failures.push(format!(
                    "{}: V4_ONLY_CHARACTER_VALIDATION — v4 no longer logs exactly one `{CHARACTER_VALIDATION_LINE}`: retire the row",
                    case.name
                ));
            }
            if got_lines.iter().any(|l| l == CHARACTER_VALIDATION_LINE) {
                failures.push(format!(
                    "{}: V4_ONLY_CHARACTER_VALIDATION — v5 CONVERGED (logs the line): retire the row",
                    case.name
                ));
            }
        }
        if got_lines != want_lines {
            failures.push(format!(
                "{}: lines\n  v4: {want_lines:#?}\n  v5: {got_lines:#?}\n  (raw v5: {lines:#?})",
                case.name
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "wardrobe wear-history helpers differ:\n{}",
        failures.join("\n")
    );
    eprintln!(
        "OK: wardrobe_wear_history_equivalence — {} cases matched v4 (values + lines).",
        spec.cases.len()
    );
}
