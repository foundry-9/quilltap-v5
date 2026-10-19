//! P4.D316 Tier 1 item 11 — tier-1 differential: the outfit-history tools'
//! pure halves against v4's REAL code at the `e8e17b2d9` pin —
//! `filterStaleItems` (`lib/wardrobe/history/rollback.ts`) →
//! [`quilltap_core::wardrobe::rollback::filter_stale_items`];
//! `windowHistory` + `formatWardrobeHistoryResults`
//! (`wardrobe-history-handler.ts`) → `tools::wardrobe_history::{window_history,
//! format}`; `formatWardrobeRollbackResults` (`wardrobe-rollback-handler.ts`)
//! → `tools::wardrobe_rollback::format`.
//!
//! Both sides build the pool from the SAME layers in
//! `harness/oracle/fixtures/wardrobe-history-tools-format.json` (v4's REAL
//! `buildWearablePool`, v5's `build_wearable_pool`). Compared: the kept slots
//! and the skipped items as JSON BYTES (the skipped items in v4's camelCase
//! route shape — `{ itemId, title, reason }`), the window's shown indexes +
//! gap flag, and every formatted text exact (the `…` / `←` / `—` code points,
//! `Wearing now: null`, the rollback's `Now wearing:` last line, the truthy
//! title rule, the hair-omitted-when-empty rule over the id LIST).
//!
//! Generate (Node 24, from the pinned v4 worktree):
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   cd ~/source/quilltap-server
//!   V5W=${V5W:-$HOME/source/quilltap-v5}
//!   $N/npx tsx $V5W/harness/oracle/cases/wardrobe-history-tools-format.ts \
//!     > /tmp/oracle-wh-tools-format.ndjson
//! Run:
//!   QT_ORACLE_WARDROBE_HISTORY_TOOLS_FORMAT=/tmp/oracle-wh-tools-format.ndjson \
//!     cargo test -p quilltap-harness --test wardrobe_history_tools_format_equivalence -- --nocapture

use std::path::Path;

use quilltap_core::db::chat_wardrobe_history::ChatWardrobeHistoryRow;
use quilltap_core::tools::{wardrobe_history, wardrobe_rollback};
use quilltap_core::wardrobe::rollback::filter_stale_items;
use quilltap_core::wardrobe::Slots;
use quilltap_core::wearable_pool::{build_wearable_pool, WardrobeTierMounts};
use serde_json::{json, Value};

fn layer(v: &Value, k: &str) -> Vec<Value> {
    v[k].as_array().cloned().unwrap_or_default()
}

fn row(index: i64, current: bool) -> ChatWardrobeHistoryRow {
    ChatWardrobeHistoryRow {
        id: String::new(),
        chat_id: String::new(),
        character_id: String::new(),
        entry_index: index,
        slots: Slots::fresh(),
        slots_hash: String::new(),
        source: "character".into(),
        actor_character_id: None,
        actor_tool_name: None,
        is_current: current,
        created_at: String::new(),
    }
}

#[test]
fn history_tools_format_matches_oracle() {
    let Some(oracle_path) = std::env::var_os("QT_ORACLE_WARDROBE_HISTORY_TOOLS_FORMAT") else {
        eprintln!("SKIP: set QT_ORACLE_WARDROBE_HISTORY_TOOLS_FORMAT (see header).");
        return;
    };
    let spec: Value = serde_json::from_str(
        &std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../harness/oracle/fixtures/wardrobe-history-tools-format.json"),
        )
        .unwrap(),
    )
    .unwrap();
    let layers = &spec["pools"]["main"];
    let pool = build_wearable_pool(
        spec["characterId"].as_str().unwrap(),
        WardrobeTierMounts::default(),
        layer(layers, "own"),
        layer(layers, "group"),
        layer(layers, "project"),
        layer(layers, "general"),
    );
    let lines: Vec<Value> = std::fs::read_to_string(&oracle_path)
        .expect("read oracle")
        .lines()
        .filter(|l| l.starts_with('{'))
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let of = |kind: &str| -> Vec<&Value> { lines.iter().filter(|l| l["kind"] == kind).collect() };
    let mut failures = Vec::new();

    let cases = spec["filterCases"].as_array().unwrap();
    let theirs = of("filter");
    assert_eq!(theirs.len(), cases.len(), "filter case count");
    for (c, v4) in cases.iter().zip(theirs) {
        let (slots, skipped) = filter_stale_items(
            &pool,
            &Slots::from_value(Some(&c["target"])),
            &Slots::from_value(Some(&c["live"])),
        );
        let got = json!({
            "slots": slots.to_value(),
            "skipped": skipped.iter().map(|s| s.to_route_value()).collect::<Vec<_>>(),
        });
        let want = json!({ "slots": v4["slots"], "skipped": v4["skipped"] });
        if serde_json::to_string(&got).unwrap() != serde_json::to_string(&want).unwrap() {
            failures.push(format!("[filter {}]\n  v5 {got}\n  v4 {want}", c["name"]));
        }
    }

    let cases = spec["windowCases"].as_array().unwrap();
    let theirs = of("window");
    assert_eq!(theirs.len(), cases.len(), "window case count");
    for (c, v4) in cases.iter().zip(theirs) {
        let rows: Vec<ChatWardrobeHistoryRow> = c["rows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| row(r[0].as_i64().unwrap(), r[1].as_bool().unwrap()))
            .collect();
        let (shown, gap) =
            wardrobe_history::window_history(&rows, c["limit"].as_u64().unwrap() as usize);
        let got = json!({
            "shown": shown.iter().map(|r| r.entry_index).collect::<Vec<_>>(),
            "gapBeforeWindow": gap,
        });
        let want = json!({ "shown": v4["shown"], "gapBeforeWindow": v4["gapBeforeWindow"] });
        if got != want {
            failures.push(format!("[window {}]\n  v5 {got}\n  v4 {want}", c["name"]));
        }
    }

    let cases = spec["rollbackFormatCases"].as_array().unwrap();
    let theirs = of("rollbackFormat");
    assert_eq!(theirs.len(), cases.len(), "rollback format case count");
    for (c, v4) in cases.iter().zip(theirs) {
        let output: wardrobe_rollback::WardrobeRollbackToolOutput =
            serde_json::from_value(c["output"].clone()).expect("rollback output");
        let got = wardrobe_rollback::format(&output);
        if got != v4["text"].as_str().unwrap() {
            failures.push(format!(
                "[rollbackFormat {}]\n  v5 {got:?}\n  v4 {:?}",
                c["name"], v4["text"]
            ));
        }
    }

    let cases = spec["historyFormatCases"].as_array().unwrap();
    let theirs = of("historyFormat");
    assert_eq!(theirs.len(), cases.len(), "history format case count");
    for (c, v4) in cases.iter().zip(theirs) {
        let output: wardrobe_history::WardrobeHistoryToolOutput =
            serde_json::from_value(c["output"].clone()).expect("history output");
        let got = wardrobe_history::format(&output);
        if got != v4["text"].as_str().unwrap() {
            failures.push(format!(
                "[historyFormat {}]\n  v5 {got:?}\n  v4 {:?}",
                c["name"], v4["text"]
            ));
        }
    }

    assert!(failures.is_empty(), "{}", failures.join("\n"));
    eprintln!(
        "OK: wardrobe history tools' pure halves matched oracle ({} lines).",
        lines.len()
    );
}
