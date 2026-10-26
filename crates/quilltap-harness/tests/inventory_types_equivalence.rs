//! Tier-1 differential (P4.D324, the Inventory keystone — v4 `4fbdad186`):
//! the Inventory's Zod model.
//!
//! Diffs [`quilltap_core::inventory::types`]'s Zod twins — the full object,
//! the sparse stored file, the full file, and the ten request-body parsers —
//! plus `inventory_shape_issues` and the two key lists, against v4's REAL
//! `lib/schemas/inventory.types.ts` over `cases/inventory-types.ts`'s probe
//! matrix. Each row compares BYTES (`serde_json::to_string`): the parsed
//! data (KEY ORDER included — zod answers shape order, defaults filled) and
//! the issue list (each code's own key order, the paths, the messages).
//!
//! Generate the oracle output (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   $N/node --import tsx $V5W/harness/oracle/cases/inventory-types.ts \
//!     > /tmp/oracle-inv-types.ndjson
//! Run:
//!   QT_ORACLE_INV_TYPES=/tmp/oracle-inv-types.ndjson \
//!     cargo test -p quilltap-harness --test inventory_types_equivalence -- --nocapture

use quilltap_core::api::zod_issues::ZodIssue;
use quilltap_core::inventory::types::{
    inventory_shape_issues, parse_chat_inventory_equip_body, parse_chat_inventory_reset_body,
    parse_clear_inventory_override_body, parse_create_inventory_body,
    parse_file_inventory_items_body, parse_instantiate_inventory_body,
    parse_inventory_folder_path_body, parse_inventory_transfer_body,
    parse_rename_inventory_folder_body, parse_update_inventory_body, zod_inventory_full_file,
    zod_inventory_item, zod_inventory_item_file, INVENTORY_FILE_KEYS, INVENTORY_INHERITABLE_FIELDS,
    INVENTORY_KIT_MAX_MEMBERS, INVENTORY_SVG_MAX_BYTES,
};
use serde_json::{json, Map, Value};

const BASELINE: &str = "p4.d324-inventory-types";

type Parser = fn(&Value) -> Result<Map<String, Value>, Vec<ZodIssue>>;

fn parser(schema: &str) -> Parser {
    match schema {
        "item" => zod_inventory_item,
        "file" => zod_inventory_item_file,
        "fullFile" => zod_inventory_full_file,
        "create" => parse_create_inventory_body,
        "update" => parse_update_inventory_body,
        "clearOverride" => parse_clear_inventory_override_body,
        "instantiate" => parse_instantiate_inventory_body,
        "folderPath" => parse_inventory_folder_path_body,
        "renameFolder" => parse_rename_inventory_folder_body,
        "fileItems" => parse_file_inventory_items_body,
        "chatEquip" => parse_chat_inventory_equip_body,
        "chatReset" => parse_chat_inventory_reset_body,
        "transfer" => parse_inventory_transfer_body,
        other => panic!("unknown schema {other}"),
    }
}

#[test]
fn inventory_types_match_oracle() {
    let Ok(path) = std::env::var("QT_ORACLE_INV_TYPES") else {
        eprintln!("SKIP: set QT_ORACLE_INV_TYPES to the oracle NDJSON (see header).");
        return;
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path}: {e}"));
    let mut failures = Vec::new();
    let (mut parses, mut shapes, mut saw_meta) = (0usize, 0usize, false);
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Value = serde_json::from_str(line).unwrap();
        match row["kind"].as_str().unwrap() {
            "meta" => {
                saw_meta = true;
                assert_eq!(row["baseline"], BASELINE, "stale oracle");
                let got = json!({
                    "fileKeys": INVENTORY_FILE_KEYS,
                    "inheritable": INVENTORY_INHERITABLE_FIELDS,
                    "svgMaxBytes": INVENTORY_SVG_MAX_BYTES,
                    "kitMaxMembers": INVENTORY_KIT_MAX_MEMBERS,
                });
                let want = json!({
                    "fileKeys": row["fileKeys"], "inheritable": row["inheritable"],
                    "svgMaxBytes": row["svgMaxBytes"], "kitMaxMembers": row["kitMaxMembers"],
                });
                assert_eq!(got.to_string(), want.to_string(), "meta");
            }
            "parse" => {
                parses += 1;
                let id = row["id"].as_str().unwrap();
                let got = match parser(row["schema"].as_str().unwrap())(&row["input"]) {
                    Ok(data) => json!({"ok": true, "data": data, "issues": []}),
                    Err(issues) => json!({"ok": false, "data": null, "issues": issues}),
                };
                let want = json!({"ok": row["ok"], "data": row["data"], "issues": row["issues"]});
                let (g, w) = (got.to_string(), want.to_string());
                if g != w {
                    failures.push(format!("{id}\n  v5: {g}\n  v4: {w}"));
                }
            }
            "shape" => {
                shapes += 1;
                let id = row["id"].as_str().unwrap();
                let got = json!(inventory_shape_issues(row["item"].as_object().unwrap()));
                if got != row["issues"] {
                    failures.push(format!("shape {id}\n  v5: {got}\n  v4: {}", row["issues"]));
                }
            }
            other => panic!("unknown row kind {other}"),
        }
    }
    assert!(saw_meta, "no meta row");
    eprintln!("inventory types: {parses} parses, {shapes} shapes");
    assert!(parses > 120 && shapes >= 10, "corpus too small");
    assert!(
        failures.is_empty(),
        "{} mismatches:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
