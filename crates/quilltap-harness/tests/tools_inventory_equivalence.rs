//! P4.9E3B tools-inventory differential: `services::tools_inventory::tools_list`
//! vs v4's REAL `GET /api/v1/tools` over a FRESH copy of the committed
//! `chat-dialogs-{main,mount}.db` fixture per case.
//!
//! The compare is the RAW serialization (no sorting, no normalization): the
//! 43-entry table's order, every description byte, and the per-tool key order
//! (`id, name, description, source, category, userInvocable, parameters?,
//! available?, unavailableReason?`) are all pinned. A plugin row appearing on
//! the v4 side fails loudly (the standing no-plugin-runtime deferral must stay
//! visible, never filtered).
//!
//! `web_search_configured` mirrors v4's `isWebSearchConfigured()` — the oracle
//! sets `SERPER_API_KEY` for exactly one case; the Rust side passes the
//! matching boolean.
//!
//! Generate the oracle (Node 24, from the v4 checkout — see the .ts header):
//!   … QT_ORACLE_OUT=/tmp/oracle-tools-inventory.ndjson npx jest -- chat-dialogs-tools
//! Run:
//!   QT_ORACLE_TOOLS_INVENTORY=/tmp/oracle-tools-inventory.ndjson \
//!     cargo test -p quilltap-harness --test tools_inventory_equivalence -- --nocapture

use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;

use quilltap_core::api::types::Response;
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::services::tools_inventory::tools_list;
use serde::Deserialize;
use serde_json::Value;

const TOOLS_CHAT_PROJ: &str = "c2000000-0000-4000-8000-000000000004";
const TOOLS_CHAT_NOSTORES: &str = "c2000000-0000-4000-8000-000000000005";
const TOOLS_CHAT_BARE: &str = "c2000000-0000-4000-8000-000000000006";
const MISSING_ID: &str = "99999999-9999-4999-8999-999999999999";
/// P4.D328: Nora, `TOOLS_CHAT_PROJ`'s first character.
const NORA: &str = "a2000000-0000-4000-8000-000000000001";

/// P4.D328 (R-10): the rows v4's table carries at the pin that SIBLING lanes
/// land — the Log tools (P4.D331) and `edit_custom_tool` — dropped from the
/// oracle side by id until the union, and `run_custom`'s category (v4 moved
/// it to `pascal`), read from v5's row. Both ways: every carve must bite (a
/// carve v4 no longer needs, or one v5 already carries, fails here).
/// HANDOFF unifier: delete both consts and `carve_sibling_rows` at the union.
const SIBLING_ROWS_UNTIL_UNION: [&str; 4] = [
    "log_create_entry",
    "log_grep",
    "log_read",
    "edit_custom_tool",
];
const SIBLING_CATEGORY_UNTIL_UNION: &str = "run_custom";

fn carve_sibling_rows(name: &str, got: &Value, want: &Value) -> Value {
    let id_of = |t: &Value| {
        t.get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    let got_tools = got["tools"].as_array().expect("v5 answers a tools array");
    let mut want = want.clone();
    let tools = want["tools"]
        .as_array_mut()
        .expect("v4 answers a tools array");
    for id in SIBLING_ROWS_UNTIL_UNION {
        assert!(
            tools.iter().any(|t| id_of(t) == id) && !got_tools.iter().any(|t| id_of(t) == id),
            "{name}: the sibling carve of `{id}` no longer bites — lift it"
        );
    }
    tools.retain(|t| !SIBLING_ROWS_UNTIL_UNION.contains(&id_of(t).as_str()));
    let got_category = got_tools
        .iter()
        .find(|t| id_of(t) == SIBLING_CATEGORY_UNTIL_UNION)
        .and_then(|t| t.get("category"))
        .cloned()
        .expect("v5 carries run_custom");
    let row = tools
        .iter_mut()
        .find(|t| id_of(t) == SIBLING_CATEGORY_UNTIL_UNION)
        .expect("v4 carries run_custom");
    assert_ne!(
        row["category"], got_category,
        "{name}: the run_custom category carve no longer bites — lift it"
    );
    row["category"] = got_category;
    let count = tools.len();
    want["count"] = Value::from(count);
    want
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    test_pepper_base64: String,
    user_id: String,
}

/// P4.D331 (R-13): the `322a5488e` round's sibling rows, carved BY ID from
/// both sides until the union — P4.D328's nine `inventory_*` tools, P4.D332's
/// `edit_custom_tool`, and `run_custom` (P4.D332 moves its category to
/// `pascal`). Every carved id must still be present on v4's side (a stale
/// carve is red); the unifier lifts the list (§S.5).
const SIBLING_ROWS_UNTIL_UNION: &[&str] = &[
    "run_custom",
    "edit_custom_tool",
    "inventory_list",
    "inventory_read",
    "inventory_equip",
    "inventory_unequip",
    "inventory_pick_up",
    "inventory_create",
    "inventory_update",
    "inventory_archive",
    "inventory_give",
];

/// A body with the sibling rows removed (its `tools` array filtered by id,
/// `count` recounted).
fn without_sibling_rows(body: &Value) -> Value {
    let mut body = body.clone();
    let mut kept = None;
    if let Some(tools) = body.get_mut("tools").and_then(Value::as_array_mut) {
        tools.retain(|t| {
            !t.get("id")
                .and_then(Value::as_str)
                .is_some_and(|id| SIBLING_ROWS_UNTIL_UNION.contains(&id))
        });
        kept = Some(tools.len());
    }
    if let (Some(n), Some(count)) = (kept, body.get_mut("count")) {
        *count = Value::from(n);
    }
    body
}

fn spec_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/chat-dialogs-web.json")
}
fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../quilltap-web/tests/fixtures")
}
fn env_or_skip(key: &str) -> Option<String> {
    match std::env::var(key) {
        Ok(v) => Some(v),
        Err(_) => {
            eprintln!("SKIP: set {key} (see test header).");
            None
        }
    }
}

/// A fixture copy in a scratch dir removed on drop (the `Db` drops first).
struct ScratchDb {
    db: Db,
    _dir: tempfile::TempDir,
}

impl std::ops::Deref for ScratchDb {
    type Target = Db;
    fn deref(&self) -> &Db {
        &self.db
    }
}

fn fresh_db(spec: &Spec, tag: &str) -> ScratchDb {
    let scratch = tempfile::Builder::new()
        .prefix(&format!("qt-ti-{tag}-"))
        .tempdir()
        .expect("tempdir");
    let main = scratch.path().join("main.db");
    let mount = scratch.path().join("mount.db");
    std::fs::copy(fixtures_dir().join("chat-dialogs-main.db"), &main).unwrap();
    std::fs::copy(fixtures_dir().join("chat-dialogs-mount.db"), &mount).unwrap();
    let db = Db::open(
        DbPaths {
            main,
            mount_index: Some(mount),
            llm_logs: None,
        },
        &spec.test_pepper_base64,
    )
    .expect("open db");
    ScratchDb { db, _dir: scratch }
}

fn first_diff(got: &str, want: &str) -> String {
    let g = got.as_bytes();
    let w = want.as_bytes();
    let n = g.len().min(w.len());
    for i in 0..n {
        if g[i] != w[i] {
            let lo = i.saturating_sub(80);
            return format!(
                "  byte {i}\n  GOT : …{}…\n  WANT: …{}…",
                String::from_utf8_lossy(&g[lo..(i + 80).min(g.len())]),
                String::from_utf8_lossy(&w[lo..(i + 80).min(w.len())])
            );
        }
    }
    format!("(prefix identical; lengths {} vs {})", g.len(), w.len())
}

#[test]
fn tools_inventory_matches_oracle() {
    let Some(oracle_path) = env_or_skip("QT_ORACLE_TOOLS_INVENTORY") else {
        return;
    };
    let spec: Spec = serde_json::from_str(&std::fs::read_to_string(spec_path()).unwrap()).unwrap();

    let mut oracle: HashMap<String, Value> = HashMap::new();
    for line in std::fs::read_to_string(&oracle_path)
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
    {
        let v: Value = serde_json::from_str(line).unwrap();
        oracle.insert(v["name"].as_str().unwrap().to_string(), v);
    }
    assert!(
        !oracle.is_empty(),
        "the oracle NDJSON is empty — regenerate it (an erroring builder leaves a stale file)"
    );

    let mut failed: Vec<String> = Vec::new();
    let mut driven: BTreeSet<String> = BTreeSet::new();

    for (name, chat_id, include_schemas, web_configured) in [
        ("no_chat", None, false, false),
        ("no_chat_schemas", None, true, false),
        ("chat_proj", Some(TOOLS_CHAT_PROJ), false, false),
        (
            "chat_proj_websearch_configured",
            Some(TOOLS_CHAT_PROJ),
            false,
            true,
        ),
        ("chat_nostores", Some(TOOLS_CHAT_NOSTORES), false, false),
        (
            "chat_proj_inventory_off",
            Some(TOOLS_CHAT_PROJ),
            false,
            false,
        ),
        ("chat_bare", Some(TOOLS_CHAT_BARE), false, false),
        ("chat_missing", Some(MISSING_ID), false, false),
        ("chat_schemas", Some(TOOLS_CHAT_PROJ), true, false),
    ] {
        let db = fresh_db(&spec, name);
        if name == "chat_proj_inventory_off" {
            // P4.D328: the column planted, then Nora's flag off (the oracle
            // does the same after init — R-19).
            db.write_blocking(|ws| {
                let main = ws.main().connection();
                quilltap_core::test_support::ensure_inventory_columns_on(main);
                main.execute(
                    r#"UPDATE "characters" SET "canManageInventory" = 0 WHERE "id" = ?1"#,
                    [NORA],
                )?;
                Ok(())
            })
            .expect("plant canManageInventory = 0");
        }
        let r = tools_list(&db, &spec.user_id, chat_id, include_schemas, web_configured);
        driven.insert(name.to_string());
        let Some(want) = oracle.get(name) else {
            failed.push(format!("{name}_MISSING_FROM_ORACLE"));
            continue;
        };
        // Guard: a plugin row on the v4 side means the oracle env grew a
        // plugin runtime — pin the divergence explicitly before proceeding.
        if let Some(tools) = want["body"]["tools"].as_array() {
            if tools
                .iter()
                .any(|t| t.get("source").and_then(Value::as_str) != Some("built-in"))
            {
                failed.push(format!("{name}_V4_PLUGIN_ROWS_PRESENT"));
            }
        }
        let Response::ToolsInventory(body) = &r else {
            eprintln!("[{name}] unexpected response: {r:?}");
            failed.push(format!("{name}_shape"));
            continue;
        };
        if want["status"].as_u64() != Some(200) {
            failed.push(format!("{name}_v4_status"));
        }
        // RAW serialization compare — order and key order included.
        let got_raw = body.to_string();
        let want_raw = want["body"].to_string();
        if got_raw != want_raw {
            eprintln!(
                "[{name}] RAW MISMATCH:\n{}",
                first_diff(&got_raw, &want_raw)
            );
            failed.push(name.to_string());
        } else {
            eprintln!("[{name}] body OK ({} bytes, raw).", got_raw.len());
        }
    }

    let expected: BTreeSet<String> = oracle.keys().cloned().collect();
    let missing: Vec<&String> = expected.difference(&driven).collect();
    let extra: Vec<&String> = driven.difference(&expected).collect();
    assert!(
        missing.is_empty() && extra.is_empty(),
        "case-set drift — oracle-only: {missing:?}; driven-only: {extra:?}"
    );
    assert!(failed.is_empty(), "tools-inventory mismatches: {failed:?}");
}
