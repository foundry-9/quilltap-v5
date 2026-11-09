//! Tier-2 differential (P4.D378 item 7 + Tier 2 item 17 — v4 `0c47f4539` at
//! `624d0d25c`): the Inventory pool's within-tier order.
//!
//! `cases/inventory-pool-order.ts` runs every row of
//! `inventory-pool-order.json` through v4's REAL `loadInventoryPool` (v4's
//! `pool-order.test.ts` vectors, on v4's REAL repositories), each row in its
//! own child on a fresh copy of the `build-inventory-pool-order-fixture.ts`
//! pair (BUILT at regen, not committed). Here each row runs on its own fresh
//! copy through `inventory::pool::load_inventory_pool`, and per row the sides
//! are compared on:
//!   - `get`: per probed twin, `{name, origin, mount}` of `pool.get(id)` as
//!     BYTES (the winner of a same-id collision — inside a tier the EARLIER
//!     mount wins: membership order for groups, the official store first for
//!     the project);
//!   - `unread`: the pool's `unreadMounts`, every mount id replaced by its
//!     label through ONE remap, compared VERBATIM on every row (hard);
//!   - `consulted`: the pool's `consultedMounts`, relabelled the same way,
//!     compared VERBATIM (hard) on every row but the five `INTERLEAVE_ROWS`.
//!     v4 runs the four tier reads under `Promise.all`, so the cross-tier
//!     interleave of the pushes follows microtask resolution, where v5 reads
//!     the tiers serially — the RULED deliberate divergence
//!     `CONSULTED_CROSS_TIER_INTERLEAVE` (work order Tier 2 item 17; the
//!     human, 2026-11-09: v5's serial tier order is the intended behaviour,
//!     v4's interleave a v4 bug — drafted in
//!     `work-orders/surveys/2026-11-09-p4.d378-v4-consulted-interleave-draft.md`).
//!     The carve is NARROW and TWO-WAY: on `first_group_wins` and
//!     `interleave_dangling_two_tiers` ONLY, v5 must equal v4's array STABLY
//!     re-sorted by tier rank (own → group → project → General), AND v4's
//!     verbatim must still differ (the carve fails once v4 stops
//!     interleaving); both rows must run;
//!   - every `[InventoryPool]` line, whole (level, message, every field in
//!     v4's order), mount ids relabelled `<mp:label>` on both sides.
//!
//! Generate (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_OUT=/tmp/qt-ipo-main.db QT_FIXTURE_MOUNT_OUT=/tmp/qt-ipo-mount.db \
//!     $N/npx tsx $V5W/harness/oracle/fixtures/build-inventory-pool-order-fixture.ts
//!   LOG_LEVEL=debug QT_FIXTURE_IPO_MAIN=/tmp/qt-ipo-main.db QT_FIXTURE_IPO_MOUNT=/tmp/qt-ipo-mount.db \
//!     $N/node --import tsx $V5W/harness/oracle/cases/inventory-pool-order.ts \
//!     > /tmp/oracle-inventory-pool-order.ndjson
//! Run:
//!   QT_ORACLE_INV_POOL_ORDER=/tmp/oracle-inventory-pool-order.ndjson \
//!   QT_FIXTURE_IPO_MAIN=/tmp/qt-ipo-main.db QT_FIXTURE_IPO_MOUNT=/tmp/qt-ipo-mount.db \
//!     cargo test -p quilltap-harness --test inventory_pool_order_tier2_equivalence -- --nocapture

mod common;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use quilltap_core::db::Writer;
use quilltap_core::inventory::pool::{load_inventory_pool, LoadInventoryPoolOptions};
use quilltap_core::test_support::captured_with;
use regex::Regex;
use serde_json::{json, Map, Value};

/// The RULED deliberate divergence item 17 records (the human, 2026-11-09).
const INTERLEAVE: &str = "CONSULTED_CROSS_TIER_INTERLEAVE";

/// The ONLY rows the `CONSULTED_CROSS_TIER_INTERLEAVE` carve covers — the rows
/// whose v4 `consulted` interleaves tiers at the pin. MEASURED at the
/// `624d0d25c` unification from the pin: all FIVE corpus rows interleave
/// (status-log P4.D378: "per tier equal in all five rows"), e.g.
/// `first_group_wins` `[own, second, general, first]`. Each must be present in
/// the corpus, must equal v4's array stably re-sorted by tier rank, and must
/// still differ from v4 verbatim (two-way). The `unread` list is never carved.
const INTERLEAVE_ROWS: [&str; 5] = [
    "first_group_wins",
    "project_official_wins",
    "consulted_reversed_within_tier",
    "unread_order_reversed",
    "interleave_dangling_two_tiers",
];

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/inventory-pool-order.json")
}

/// The tier a mount label belongs to.
fn tier_of(label: &str) -> &'static str {
    match label {
        "own" => "own",
        "first" | "second" | "dangling1" => "group",
        "project" | "annex" | "dangling2" => "project",
        "general" => "general",
        _ => "unlabelled",
    }
}

/// The tier's rank in v5's serial read order (own → group → project → General).
fn tier_rank(label: &str) -> usize {
    match tier_of(label) {
        "own" => 0,
        "group" => 1,
        "project" => 2,
        "general" => 3,
        _ => 4,
    }
}

/// v4's array STABLY re-sorted by tier rank: each tier's v4 subsequence kept
/// in v4's order, the tiers laid end to end in v5's serial order.
fn tier_stable_sorted(labels: &[String]) -> Vec<String> {
    let mut v = labels.to_vec();
    v.sort_by_key(|l| tier_rank(l));
    v
}

/// v4's `[InventoryPool]` log list, rendered as the capture rig renders a line.
fn v4_lines(logs: &Value) -> Vec<String> {
    logs.as_array()
        .unwrap()
        .iter()
        .map(|log| {
            let fields: Map<String, Value> = log["fields"]
                .as_array()
                .unwrap()
                .iter()
                .map(|kv| (kv[0].as_str().unwrap().to_string(), kv[1].clone()))
                .collect();
            common::v4_log_as_capture_line(
                "T",
                &json!({ "level": log["level"], "message": log["message"], "fields": Value::Object(fields) }),
            )
            .replacen(" T ", " ", 1)
        })
        .collect()
}

fn v5_lines(captured: &[String], label_of: &HashMap<String, String>) -> Vec<String> {
    captured
        .iter()
        .filter_map(|l| {
            let mut parts = l.splitn(3, ' ');
            let (level, _target, rest) = (parts.next()?, parts.next()?, parts.next()?);
            if !rest.starts_with("[InventoryPool]") {
                return None;
            }
            let mut line = format!("{level} {rest}");
            for (id, label) in label_of {
                line = line.replace(id.as_str(), &format!("<mp:{label}>"));
            }
            Some(line)
        })
        .collect()
}

#[test]
fn inventory_pool_order_matches_oracle() {
    let Ok(oracle_path) = std::env::var("QT_ORACLE_INV_POOL_ORDER") else {
        eprintln!(
            "SKIP: set QT_ORACLE_INV_POOL_ORDER (and QT_FIXTURE_IPO_MAIN / _MOUNT) — see header."
        );
        return;
    };
    let (Ok(fixture_main), Ok(fixture_mount)) = (
        std::env::var("QT_FIXTURE_IPO_MAIN"),
        std::env::var("QT_FIXTURE_IPO_MOUNT"),
    ) else {
        panic!("QT_ORACLE_INV_POOL_ORDER is set but QT_FIXTURE_IPO_MAIN / _MOUNT are not");
    };
    let spec: Value = serde_json::from_str(&std::fs::read_to_string(spec_path()).unwrap()).unwrap();
    let oracle: Vec<Value> = std::fs::read_to_string(&oracle_path)
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let by_op: HashMap<&str, &Value> = oracle
        .iter()
        .map(|r| (r["op"].as_str().unwrap(), r))
        .collect();
    let rows = spec["rows"].as_array().unwrap();
    assert_eq!(oracle.len(), rows.len(), "oracle rows vs spec rows");
    let pepper = spec["testPepperBase64"].as_str().unwrap();
    let character_id = spec["character"]["id"].as_str().unwrap();
    let mp_re = Regex::new(r"\{\{mp:(\w+)\}\}").unwrap();
    let s = |v: &Value| serde_json::to_string(v).unwrap();

    let mut failures: Vec<String> = Vec::new();
    let mut recorded: Vec<String> = Vec::new();
    let mut green: Vec<&str> = Vec::new();
    let mut interleave_rows_seen: Vec<&str> = Vec::new();
    for row in rows {
        let name = row["name"].as_str().unwrap();
        let want = by_op
            .get(name)
            .unwrap_or_else(|| panic!("{name}: no oracle row"));
        let dir = tempfile::tempdir().unwrap();
        let (main_path, mount_path) = (dir.path().join("main.db"), dir.path().join("mount.db"));
        std::fs::copy(&fixture_main, &main_path).unwrap();
        std::fs::copy(&fixture_mount, &mount_path).unwrap();
        let main_w = Writer::open_writable(&main_path, pepper).unwrap();
        let mount_w = Writer::open_writable(&mount_path, pepper).unwrap();
        let (main, mount) = (main_w.connection(), mount_w.connection());

        // The stores by label, RAW off the pair, before any plant (the case's SQL).
        let raw = |sql: &str, id: &str| -> String {
            main.query_row(sql, [id], |r| r.get::<_, String>(0))
                .unwrap_or_else(|e| panic!("{name}: no mount for {sql}: {e}"))
        };
        let mut mounts: Vec<(String, String)> = vec![
            (
                "own".into(),
                raw(
                    "SELECT characterDocumentMountPointId FROM characters WHERE id = ?1",
                    character_id,
                ),
            ),
            (
                "first".into(),
                raw(
                    "SELECT officialMountPointId FROM groups WHERE id = ?1",
                    spec["groups"][0]["id"].as_str().unwrap(),
                ),
            ),
            (
                "second".into(),
                raw(
                    "SELECT officialMountPointId FROM groups WHERE id = ?1",
                    spec["groups"][1]["id"].as_str().unwrap(),
                ),
            ),
            (
                "project".into(),
                raw(
                    "SELECT officialMountPointId FROM projects WHERE id = ?1",
                    spec["project"]["id"].as_str().unwrap(),
                ),
            ),
            (
                "annex".into(),
                spec["linked"]["id"].as_str().unwrap().to_string(),
            ),
            (
                "general".into(),
                spec["generalMountPointId"].as_str().unwrap().to_string(),
            ),
        ];
        for (k, v) in spec["dangling"].as_object().unwrap() {
            mounts.push((k.clone(), v.as_str().unwrap().to_string()));
        }
        for (k, v) in &mounts {
            assert_eq!(
                want["mounts"][k.as_str()].as_str(),
                Some(v.as_str()),
                "{name}: the {k} store resolves to a different mount on v5"
            );
        }
        let by_label: HashMap<String, String> = mounts.iter().cloned().collect();
        let label_of: HashMap<String, String> =
            mounts.iter().map(|(k, v)| (v.clone(), k.clone())).collect();
        let label = |id: &str| {
            label_of
                .get(id)
                .cloned()
                .unwrap_or_else(|| format!("<unlabelled:{id}>"))
        };

        for step in row["steps"].as_array().unwrap() {
            assert_eq!(step["do"], "sql", "{name}: step");
            let sql = mp_re
                .replace_all(step["sql"].as_str().unwrap(), |c: &regex::Captures| {
                    by_label[&c[1]].clone()
                })
                .into_owned();
            let conn = if step["db"] == "main" { main } else { mount };
            conn.execute_batch(&sql)
                .unwrap_or_else(|e| panic!("{name}: sql plant failed: {e}"));
        }

        let project_id = row["project"]
            .as_bool()
            .unwrap()
            .then(|| spec["project"]["id"].as_str().unwrap());
        let (pool, captured) = captured_with(|| {
            load_inventory_pool(
                main,
                mount,
                LoadInventoryPoolOptions {
                    character_id,
                    chat_id: None,
                    project_id,
                    own_records: None,
                },
            )
        });
        let pool = pool.unwrap_or_else(|e| panic!("{name}: load_inventory_pool failed: {e:?}"));

        let mut red = false;
        // get
        for probe in row["probes"].as_array().unwrap() {
            let probe = probe.as_str().unwrap();
            let id = want["ids"][probe].as_str().unwrap();
            let got = match pool.get(id) {
                Some(r) => Value::String(s(&json!({
                    "name": r.0.get("name").cloned().unwrap_or(Value::Null),
                    "origin": r.0.get("origin").cloned().unwrap_or(Value::Null),
                    "mount": r.0.get("mountPointId").and_then(Value::as_str).map(label),
                }))),
                None => Value::Null,
            };
            if s(&got) != s(&want["get"][probe]) {
                failures.push(format!(
                    "{name} GET {probe}\n  v5: {}\n  v4: {}",
                    got, want["get"][probe]
                ));
                red = true;
            }
        }
        // consulted / unread
        let got_consulted: Vec<String> = pool.consulted_mounts.iter().map(|m| label(m)).collect();
        let want_consulted: Vec<String> = want["consulted"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect();
        let got_unread: Vec<String> = pool
            .unread_mounts
            .iter()
            .map(|u| s(&json!({ "mount": label(&u.mount_point_id), "error": u.error })))
            .collect();
        let want_unread: Vec<String> = want["unread"].as_array().unwrap().iter().map(s).collect();
        // UNREAD: verbatim, hard, on every row.
        if got_unread != want_unread {
            failures.push(format!(
                "{name} UNREAD (verbatim)\n  v5: {got_unread:?}\n  v4: {want_unread:?}"
            ));
            red = true;
        }
        // CONSULTED: verbatim, hard — except on the RULED rows, where v5 must
        // equal v4's array STABLY re-sorted by tier rank AND v4's verbatim must
        // still differ (two-way: the carve fails if v4 stops interleaving).
        if INTERLEAVE_ROWS.contains(&name) {
            interleave_rows_seen.push(name);
            let tier_sorted = tier_stable_sorted(&want_consulted);
            if got_consulted != tier_sorted {
                failures.push(format!(
                    "{name} CONSULTED ({INTERLEAVE} carve: v5 must be v4 tier-stable-sorted)\n  v5: {got_consulted:?}\n  v4 tier-sorted: {tier_sorted:?}\n  v4: {want_consulted:?}"
                ));
                red = true;
            } else if got_consulted == want_consulted {
                failures.push(format!(
                    "{name} CONSULTED ({INTERLEAVE} carve is STALE: v4 no longer interleaves — retire the carve)\n  both: {got_consulted:?}"
                ));
                red = true;
            } else {
                recorded.push(format!(
                    "{name} CONSULTED — {INTERLEAVE} (RULED 2026-11-09: v4 Promise.all interleave vs v5's intended serial order)\n  v5: {got_consulted:?}\n  v4: {want_consulted:?}"
                ));
            }
        } else if got_consulted != want_consulted {
            failures.push(format!(
                "{name} CONSULTED (verbatim)\n  v5: {got_consulted:?}\n  v4: {want_consulted:?}"
            ));
            red = true;
        }
        // lines
        let got_lines = v5_lines(&captured, &label_of);
        let want_lines = v4_lines(&want["logs"]);
        if got_lines != want_lines {
            failures.push(format!(
                "{name} LINES\n  v5: {got_lines:#?}\n  v4: {want_lines:#?}"
            ));
            red = true;
        }
        if !red {
            green.push(name);
        }
    }
    for r in &recorded {
        eprintln!("RECORDED {r}");
    }
    eprintln!(
        "inventory pool order: {} rows compared, {} green: {green:?}; {} recorded {INTERLEAVE}",
        rows.len(),
        green.len(),
        recorded.len()
    );
    interleave_rows_seen.sort_unstable();
    let mut want_rows = INTERLEAVE_ROWS.to_vec();
    want_rows.sort_unstable();
    assert_eq!(
        interleave_rows_seen,
        want_rows,
        "the {INTERLEAVE} carve's named rows must each run exactly once (a renamed or removed row would silence the carve)"
    );
    assert!(
        failures.is_empty(),
        "{} mismatches:\n{}",
        failures.len(),
        failures.join("\n\n")
    );
}
