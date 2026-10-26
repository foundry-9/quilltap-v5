//! Tier-1 differential (P4.D324, the Inventory keystone — v4 `4fbdad186`):
//! the allow-list SVG sanitiser.
//!
//! Diffs [`quilltap_core::inventory::svg`] against v4's REAL
//! `lib/inventory/svg.ts` (`sanitizeInventorySvg`, `inventorySvgDataUri`)
//! over `cases/inventory-svg.ts`'s corpus — v4's eight `svg.test.ts` shapes
//! plus the fuzz list — bytes exact, a throw compared by class name and
//! message.
//!
//! Generate the oracle output (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   $N/node --import tsx $V5W/harness/oracle/cases/inventory-svg.ts \
//!     > /tmp/oracle-inv-svg.ndjson
//! Run:
//!   QT_ORACLE_INV_SVG=/tmp/oracle-inv-svg.ndjson \
//!     cargo test -p quilltap-harness --test inventory_svg_equivalence -- --nocapture

use quilltap_core::inventory::svg::{inventory_svg_data_uri, sanitize_inventory_svg};
use quilltap_core::inventory::types::INVENTORY_SVG_MAX_BYTES;
use serde_json::{json, Value};

const BASELINE: &str = "p4.d324-inventory-svg";

#[test]
fn inventory_svg_matches_oracle() {
    let Ok(path) = std::env::var("QT_ORACLE_INV_SVG") else {
        eprintln!("SKIP: set QT_ORACLE_INV_SVG to the oracle NDJSON (see header).");
        return;
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path}: {e}"));
    let mut failures = Vec::new();
    let (mut rows, mut saw_meta) = (0usize, false);
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Value = serde_json::from_str(line).unwrap();
        let (id, got, want) = match row["kind"].as_str().unwrap() {
            "meta" => {
                saw_meta = true;
                assert_eq!(row["baseline"], BASELINE, "stale oracle");
                assert_eq!(row["maxBytes"], json!(INVENTORY_SVG_MAX_BYTES));
                continue;
            }
            "sanitize" => {
                let got = match sanitize_inventory_svg(row["input"].as_str()) {
                    Ok(v) => json!({"ok": true, "value": v}),
                    Err(e) => json!({"ok": false, "name": "InventorySvgError", "message": e.0}),
                };
                (row["id"].clone(), got, row["out"].clone())
            }
            "data_uri" => (
                row["id"].clone(),
                json!(inventory_svg_data_uri(row["svg"].as_str().unwrap())),
                row["out"].clone(),
            ),
            other => panic!("unknown row kind {other}"),
        };
        rows += 1;
        // Byte comparison (a `preserve_order` map's `==` is order-blind).
        if serde_json::to_string(&got).unwrap() != serde_json::to_string(&want).unwrap() {
            failures.push(format!("{id}\n  v5: {got}\n  v4: {want}"));
        }
    }
    assert!(saw_meta, "no meta row");
    eprintln!("inventory svg: {rows} rows");
    assert!(rows > 55, "corpus too small");
    assert!(
        failures.is_empty(),
        "{} mismatches:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
