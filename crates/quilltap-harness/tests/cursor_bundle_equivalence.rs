//! Differential test — P4.D288 (C1-11): `quilltap_core::services::carriers::
//! cursor_bundle` against v4's REAL `lib/memory/extraction/cursor-bundle.ts`
//! (`ac6348c5f` + bug 245) at the `83334b9a5` pin.
//!
//! - **Tier-1:** `cursors_to_bundle_fields` (other → episode → turn, a blank
//!   cursor left out, never `null`) and `take_bundle_cursors` (all three keys
//!   deleted in place, only non-empty strings kept) — bytes, key order
//!   included.
//! - **Tier-2:** the three stateful wrappers over a `base-<shape>.db` v4's own
//!   code builds, for each cursor-table shape — `migration` (the table v4's
//!   migration writes, with its foreign key), `fresh` (the repository's
//!   generateDDL table) and `absent` (none: a read answers "no cursors" and
//!   creates nothing). Replayed on a caller-thread `Writer` (the capture rig is
//!   thread-local), each op's answer and its lines compared: the restore's
//!   DEBUG, the foreign-key refusal's WARN, then the failure arms after a
//!   renamed `chatId` (the per-chat read's WARN inside the strict scope; the
//!   fallback's silence outside it) and a renamed `messageId` (the restore's
//!   WARN per pass). v4's one-read `findAllByChat` reads `SELECT *` and only
//!   Zod-drops rows, so its WARN arm is POSED in the oracle by a throwing
//!   method (v5 fails for real); that op's `error` text is masked
//!   (`maskError`) — its level, message, `module` and key set still compare.
//!   Every `error` text compares STRICT, v4's quoted-identifier hint included:
//!   since P4.172 v5's connections refuse SQLite's double-quoted-string
//!   fallback as v4's `SQLITE_DQS=0` build does, so the renamed `chatId` fails
//!   the same read with the same sentence on both sides (the four rows the
//!   retired `DQS_QUOTED_IDENTIFIER_FALLBACK` table pinned both ways).
//!
//! Regenerate (Node 24, from the round's pin), one invocation per shape:
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   rm -rf /tmp/qt-cursor-bundle /tmp/oracle-cursor-bundle-*.ndjson
//!   cd ~/source/quilltap-server
//!   for S in migration fresh absent; do
//!     QT_CURSOR_BUNDLE_SHAPE=$S QT_FIXTURE_CURSOR_BUNDLE_DIR=/tmp/qt-cursor-bundle \
//!       $N/npx tsx $V5W/harness/oracle/cases/cursor-bundle.ts > /tmp/oracle-cursor-bundle-$S.ndjson
//!   done
//! Run:
//!   QT_ORACLE_CURSOR_BUNDLE_DIR=/tmp QT_FIXTURE_CURSOR_BUNDLE_DIR=/tmp/qt-cursor-bundle \
//!     cargo test -p quilltap-harness --test cursor_bundle_equivalence

mod common;

use quilltap_core::db::fallback::with_strict_repository_failures;
use quilltap_core::db::memory_extraction_cursors::{ExtractionCursorMap, ExtractionCursorPass};
use quilltap_core::db::Writer;
use quilltap_core::services::carriers::cursor_bundle::{
    cursors_to_bundle_fields, load_bundle_cursors, restore_bundle_cursors, take_bundle_cursors,
    with_bundle_cursor_fields,
};
use serde_json::{json, Map, Value};

const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";

/// The module's four lines — the oracle's capture set (the repository's own
/// `[ExtractionCursors] Cursor set` DEBUG is the cursor families', not this
/// one's).
const BUNDLE_MESSAGES: [&str; 4] = [
    "[ExtractionCursors] Could not read the cursor table for a bundle; reading per chat",
    "[ExtractionCursors] Could not read a chat's cursors for a bundle; leaving them out",
    "[ExtractionCursors] Could not restore a bundled cursor; the pass will re-read",
    "[ExtractionCursors] Restored bundled cursors",
];

/// A v4 cursor map (`{"other-fold": …}`) as v5's struct.
fn map_from_json(v: &Value) -> ExtractionCursorMap {
    let mut map = ExtractionCursorMap::default();
    for (k, id) in v.as_object().unwrap() {
        let pass = ExtractionCursorPass::parse(k).unwrap_or_else(|| panic!("pass {k}"));
        map.set(pass, id.as_str().unwrap().to_string());
    }
    map
}

/// v4's lines as `LEVEL message k=v …` (`error` strict; masked on request).
fn want_lines(op: &Value, mask_error: bool) -> Vec<String> {
    op["lines"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| {
            let mut out = format!(
                "{} {}",
                l["level"].as_str().unwrap().to_uppercase(),
                l["message"].as_str().unwrap()
            );
            for (k, v) in l["fields"].as_object().unwrap() {
                let rendered = match v {
                    Value::String(_) if k == "error" && mask_error => "<masked>".to_string(),
                    Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                out.push_str(&format!(" {k}={rendered}"));
            }
            out
        })
        .collect()
}

/// v5's lines minus their target (`error` masked on request).
fn got_lines(lines: Vec<String>, mask_error: bool) -> Vec<String> {
    lines
        .into_iter()
        .map(|l| {
            let mut parts = l.splitn(3, ' ');
            let level = parts.next().unwrap().to_string();
            let _target = parts.next();
            let mut rest = parts.next().unwrap_or_default().to_string();
            if mask_error {
                if let Some(at) = rest.find(" error=") {
                    rest = format!("{} error=<masked>", &rest[..at]);
                }
            }
            format!("{level} {rest}")
        })
        .filter(|l| BUNDLE_MESSAGES.iter().any(|m| l.contains(m)))
        .collect()
}

fn tier1(lines: &[Value]) -> usize {
    let mut n = 0;
    for case in lines {
        match case["case"].as_str().unwrap() {
            "fields" => {
                let got = Value::Object(cursors_to_bundle_fields(&map_from_json(&case["input"])));
                assert_eq!(
                    got.to_string(),
                    case["result"].to_string(),
                    "fields {}",
                    case["input"]
                );
                n += 1;
            }
            "take" => {
                let mut chat = case["input"].as_object().unwrap().clone();
                let cursors = take_bundle_cursors(&mut chat);
                assert_eq!(
                    Value::Object(chat).to_string(),
                    case["chat"].to_string(),
                    "take (chat) {}",
                    case["input"]
                );
                assert_eq!(
                    cursors.to_json(),
                    case["cursors"],
                    "take (cursors) {}",
                    case["input"]
                );
                n += 1;
            }
            _ => {}
        }
    }
    n
}

fn replay(writer: &Writer, op: &Value) -> (Value, Vec<String>) {
    let input = &op["input"];
    let strict = input["strict"].as_bool().unwrap();
    let conn = writer.connection();
    let (answer, lines) = quilltap_core::test_support::captured_with(|| {
        let go = || -> Value {
            match op["op"].as_str().unwrap() {
                "load" => match load_bundle_cursors(conn) {
                    Some(by_chat) => Value::Array(
                        by_chat
                            .into_iter()
                            .map(|(id, map)| json!([id, map.to_json()]))
                            .collect(),
                    ),
                    None => Value::Null,
                },
                "with" => {
                    let chat: Map<String, Value> = input["chat"].as_object().unwrap().clone();
                    let preloaded = if input["preloaded"].as_bool().unwrap() {
                        Some(load_bundle_cursors(conn).expect("the preload reads"))
                    } else {
                        None
                    };
                    Value::Object(with_bundle_cursor_fields(conn, chat, preloaded.as_ref()))
                }
                "restore" => Value::from(restore_bundle_cursors(
                    conn,
                    input["chatId"].as_str().unwrap(),
                    &map_from_json(&input["cursors"]),
                )),
                other => panic!("unknown op {other}"),
            }
        };
        if strict {
            with_strict_repository_failures(go)
        } else {
            go()
        }
    });
    (answer, lines)
}

#[test]
fn cursor_bundle_matches_v4() {
    let (Ok(oracle_dir), Ok(fixture_dir)) = (
        std::env::var("QT_ORACLE_CURSOR_BUNDLE_DIR"),
        std::env::var("QT_FIXTURE_CURSOR_BUNDLE_DIR"),
    ) else {
        eprintln!(
            "SKIP: set QT_ORACLE_CURSOR_BUNDLE_DIR + QT_FIXTURE_CURSOR_BUNDLE_DIR (see header)."
        );
        return;
    };
    let mut failures = Vec::new();
    let (mut pure, mut stateful, mut compared_lines) = (0usize, 0usize, 0usize);
    for shape in ["migration", "fresh", "absent"] {
        let path =
            std::path::Path::new(&oracle_dir).join(format!("oracle-cursor-bundle-{shape}.ndjson"));
        let lines: Vec<Value> = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        pure += tier1(&lines);
        let scratch = tempfile::tempdir().unwrap();
        let work = scratch.path().join("main.db");
        std::fs::copy(
            std::path::Path::new(&fixture_dir).join(format!("base-{shape}.db")),
            &work,
        )
        .expect("copy base");
        let writer = Writer::open_writable(&work, PEPPER).expect("open the caller-thread writer");
        for (i, op) in lines.iter().filter(|l| l["case"] == "stateful").enumerate() {
            if let Some(sql) = op.get("plant").and_then(Value::as_str) {
                writer.connection().execute_batch(sql).expect("plant");
                continue;
            }
            let kind = op["op"].as_str().unwrap();
            let mask = op["input"]["maskError"].as_bool().unwrap_or(false);
            let (got, got_l) = replay(&writer, op);
            let got_l = got_lines(got_l, mask);
            let want_l = want_lines(op, mask);
            // `with` answers a chat whose key ORDER is v4's spread — bytes;
            // `load`'s per-chat maps follow v4's query plan, an order v5's
            // struct does not carry (the module doc) — `Value` equality,
            // which is order-blind inside an object.
            let same = if kind == "with" {
                // Bytes: `Value` equality is key-order-blind (serde_json's
                // `preserve_order` map) — never clippy's `cmp_owned` rewrite.
                serde_json::to_string(&got).unwrap()
                    == serde_json::to_string(&op["result"]).unwrap()
            } else {
                got == op["result"]
            };
            if !same {
                failures.push(format!(
                    "{shape} op {i} ({kind}): result\n  v5 {got}\n  v4 {}",
                    op["result"]
                ));
            }
            if got_l != want_l {
                failures.push(format!(
                    "{shape} op {i} ({kind}): lines\n  v5 {got_l:#?}\n  v4 {want_l:#?}"
                ));
            }
            compared_lines += want_l.len();
            stateful += 1;
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
    assert_eq!(pure, 7, "the tier-1 case count");
    assert!(
        stateful >= 28 && compared_lines >= 8,
        "{stateful} ops / {compared_lines} lines"
    );
    eprintln!("OK: cursor bundle matched v4 ({pure} pure, {stateful} stateful ops, {compared_lines} lines).");
}
