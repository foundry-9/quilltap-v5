//! Tier-1 differential (P4.D324, the Inventory keystone — v4 `4fbdad186`'s
//! LIFT): the generic item-folder rules and the Inventory's delegates.
//!
//! Diffs [`quilltap_core::inventory::item_folders`] — under BOTH rule sets,
//! the wardrobe's ([`quilltap_core::wardrobe_folders::WARDROBE_FOLDER_RULES`])
//! and the Inventory's — and [`quilltap_core::inventory::folders`] against
//! v4's REAL `lib/mount-index/item-folders.ts` and `lib/inventory/folders.ts`,
//! string-exact, over `wardrobe-folders.ts`'s probe matrix plus the
//! `.json` / `.md` document-path, `folderUnderItemRoot` and error-message
//! rows. Every row compares the JSON BYTES of the outcome (`{ok, value}` /
//! `{ok, name, message}`), so a wardrobe sentence the lift moved reddens here
//! (and in the unchanged `wardrobe_folders_equivalence`).
//!
//! Generate the oracle output (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   $N/node --import tsx $V5W/harness/oracle/cases/item-folders.ts \
//!     > /tmp/oracle-item-folders.ndjson
//! Run:
//!   QT_ORACLE_ITEM_FOLDERS=/tmp/oracle-item-folders.ndjson \
//!     cargo test -p quilltap-harness --test inventory_folders_equivalence -- --nocapture

use quilltap_core::inventory::folders::{
    folder_of_inventory_path, inventory_folder_store_path, is_inventory_item_document_path,
    is_safe_inventory_folder_path, is_valid_inventory_folder_path, normalize_inventory_folder_name,
    normalize_inventory_folder_path, normalize_inventory_folder_path_or_root,
    resolve_inventory_folder, InventoryFolderError, INVENTORY_FOLDER, INVENTORY_FOLDER_MAX_DEPTH,
    INVENTORY_FOLDER_RULES, INVENTORY_FOLDER_SCHEMA_MESSAGE, INVENTORY_RESERVED_FOLDER_SEGMENTS,
};
use quilltap_core::inventory::item_folders::{
    self, folder_crumb, folder_name, folder_of_item_path, folder_segments, folder_under_item_root,
    is_item_document_path, is_safe_item_folder_path, is_same_folder, is_under_folder,
    is_valid_item_folder_path, item_folder_store_path, join_folder, normalize_item_folder_name,
    normalize_item_folder_path, normalize_item_folder_path_or_root, parent_folder, rebase_folder,
    resolve_item_folder, ItemFolderRules,
};
use quilltap_core::wardrobe_folders::WARDROBE_FOLDER_RULES;
use quilltap_core::wardrobe_instructions::is_wardrobe_instructions_file_name;
use serde_json::{json, Value};

const BASELINE: &str = "p4.d324-item-folders";

fn rules_of(key: &str) -> ItemFolderRules {
    match key {
        "wardrobe" => WARDROBE_FOLDER_RULES,
        "inventory" => INVENTORY_FOLDER_RULES,
        other => panic!("unknown rules {other}"),
    }
}

fn outcome(r: Result<String, String>, error_name: &str) -> Value {
    match r {
        Ok(v) => json!({"ok": true, "value": v}),
        Err(m) => json!({"ok": false, "name": error_name, "message": m}),
    }
}

fn inv_outcome(r: Result<String, InventoryFolderError>) -> Value {
    outcome(r.map_err(|e| e.to_string()), "InventoryFolderPathError")
}

fn s<'a>(row: &'a Value, key: &str) -> &'a str {
    row[key]
        .as_str()
        .unwrap_or_else(|| panic!("{key} missing: {row}"))
}

fn opt<'a>(row: &'a Value, key: &str) -> Option<&'a str> {
    row[key].as_str()
}

fn strings(row: &Value, key: &str) -> Vec<String> {
    row[key]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect()
}

#[test]
fn item_folders_match_oracle() {
    let Ok(path) = std::env::var("QT_ORACLE_ITEM_FOLDERS") else {
        eprintln!("SKIP: set QT_ORACLE_ITEM_FOLDERS to the oracle NDJSON (see header).");
        return;
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path}: {e}"));
    let mut failures: Vec<String> = Vec::new();
    let mut rows = 0usize;
    let mut saw_meta = false;
    let mut kinds = std::collections::BTreeMap::<String, usize>::new();

    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Value =
            serde_json::from_str(line).unwrap_or_else(|e| panic!("bad row {e}: {line}"));
        let kind = s(&row, "kind").to_string();
        *kinds.entry(kind.clone()).or_default() += 1;
        rows += 1;
        let label = format!(
            "{kind}/{}/{}",
            row["rules"].as_str().unwrap_or("-"),
            row["id"].as_str().unwrap_or("-")
        );
        let (got, want): (Value, Value) = match kind.as_str() {
            "meta" => {
                saw_meta = true;
                assert_eq!(s(&row, "baseline"), BASELINE, "stale oracle");
                let rules = |r: &ItemFolderRules| json!({"root": r.root, "reserved": r.reserved, "maxDepth": r.max_depth, "noun": r.noun});
                (
                    json!({
                        "rules": {"wardrobe": rules(&WARDROBE_FOLDER_RULES), "inventory": rules(&INVENTORY_FOLDER_RULES)},
                        "inventoryFolder": INVENTORY_FOLDER,
                        "inventoryMaxDepth": INVENTORY_FOLDER_MAX_DEPTH,
                        "inventoryReserved": INVENTORY_RESERVED_FOLDER_SEGMENTS,
                    }),
                    json!({
                        "rules": row["rules"],
                        "inventoryFolder": row["inventoryFolder"],
                        "inventoryMaxDepth": row["inventoryMaxDepth"],
                        "inventoryReserved": row["inventoryReserved"],
                    }),
                )
            }
            "messages" => {
                let noun = s(&row, "noun");
                let ids = vec!["i-1".to_string(), "i-2".to_string()];
                (
                    json!({
                        "notFound": item_folders::not_found_message(noun, "Formal/Evening"),
                        "notFoundRoot": item_folders::not_found_message(noun, ""),
                        "conflict": item_folders::conflict_message(noun, "Formal"),
                        "notInLocation": item_folders::not_in_location_message(noun, &ids),
                        "notInLocationNone": item_folders::not_in_location_message(noun, &[]),
                    }),
                    json!({
                        "notFound": row["notFound"], "notFoundRoot": row["notFoundRoot"],
                        "conflict": row["conflict"], "notInLocation": row["notInLocation"],
                        "notInLocationNone": row["notInLocationNone"],
                    }),
                )
            }
            "inventory_messages" => (
                json!({
                    "notFound": InventoryFolderError::NotFound("Satchel/Inner".into()).to_string(),
                    "conflict": InventoryFolderError::conflict_default("Satchel").to_string(),
                    "conflictCustom": InventoryFolderError::Conflict("custom sentence".into()).to_string(),
                    "notInLocation": InventoryFolderError::NotInLocation(vec!["a".into(), "b".into()]).to_string(),
                    "schema": INVENTORY_FOLDER_SCHEMA_MESSAGE,
                }),
                json!({
                    "notFound": row["notFound"], "conflict": row["conflict"],
                    "conflictCustom": row["conflictCustom"], "notInLocation": row["notInLocation"],
                    "schema": row["schema"],
                }),
            ),
            "normalize" => (
                outcome(
                    normalize_item_folder_path(&rules_of(s(&row, "rules")), opt(&row, "input")),
                    "ItemFolderPathError",
                ),
                row["out"].clone(),
            ),
            "normalize_or_root" => (
                json!(normalize_item_folder_path_or_root(
                    &rules_of(s(&row, "rules")),
                    opt(&row, "input")
                )),
                row["out"].clone(),
            ),
            "normalize_name" => (
                outcome(
                    normalize_item_folder_name(&rules_of(s(&row, "rules")), opt(&row, "input")),
                    "ItemFolderPathError",
                ),
                row["out"].clone(),
            ),
            "safe" => (
                json!(is_safe_item_folder_path(
                    &rules_of(s(&row, "rules")),
                    s(&row, "path")
                )),
                row["out"].clone(),
            ),
            "valid" => (
                json!(is_valid_item_folder_path(
                    &rules_of(s(&row, "rules")),
                    s(&row, "path")
                )),
                row["out"].clone(),
            ),
            "resolve" => (
                outcome(
                    resolve_item_folder(
                        &rules_of(s(&row, "rules")),
                        &strings(&row, "existing"),
                        opt(&row, "requested"),
                    ),
                    "ItemFolderPathError",
                ),
                row["out"].clone(),
            ),
            "folder_of" => (
                json!(folder_of_item_path(
                    &rules_of(s(&row, "rules")),
                    s(&row, "path")
                )),
                row["out"].clone(),
            ),
            "store_path" => (
                json!(item_folder_store_path(
                    &rules_of(s(&row, "rules")),
                    s(&row, "folder")
                )),
                row["out"].clone(),
            ),
            "under_root" => (
                json!(folder_under_item_root(
                    &rules_of(s(&row, "rules")),
                    s(&row, "path")
                )),
                row["out"].clone(),
            ),
            "doc_path" => {
                let skip: Option<&dyn Fn(&str) -> bool> = if row["skip"].as_bool().unwrap() {
                    Some(&is_wardrobe_instructions_file_name)
                } else {
                    None
                };
                (
                    json!(is_item_document_path(
                        &rules_of(s(&row, "rules")),
                        s(&row, "path"),
                        s(&row, "ext"),
                        skip
                    )),
                    row["out"].clone(),
                )
            }
            "views" => {
                let f = s(&row, "folder");
                (
                    json!({"segments": folder_segments(f), "parent": parent_folder(f), "name": folder_name(f), "crumb": folder_crumb(f)}),
                    json!({"segments": row["segments"], "parent": row["parent"], "name": row["name"], "crumb": row["crumb"]}),
                )
            }
            "join" => {
                let parts = strings(&row, "parts");
                let refs: Vec<&str> = parts.iter().map(String::as_str).collect();
                (json!(join_folder(&refs)), row["out"].clone())
            }
            "same" => (
                json!(is_same_folder(opt(&row, "a"), opt(&row, "b"))),
                row["out"].clone(),
            ),
            "under" => (
                json!(is_under_folder(s(&row, "folder"), s(&row, "ancestor"))),
                row["out"].clone(),
            ),
            "rebase" => (
                json!(rebase_folder(
                    s(&row, "folder"),
                    s(&row, "from"),
                    s(&row, "to")
                )),
                row["out"].clone(),
            ),
            "inv_normalize" => (
                inv_outcome(normalize_inventory_folder_path(opt(&row, "input"))),
                row["out"].clone(),
            ),
            "inv_normalize_or_root" => (
                json!(normalize_inventory_folder_path_or_root(opt(&row, "input"))),
                row["out"].clone(),
            ),
            "inv_normalize_name" => (
                inv_outcome(normalize_inventory_folder_name(opt(&row, "input"))),
                row["out"].clone(),
            ),
            "inv_safe" => (
                json!(is_safe_inventory_folder_path(s(&row, "path"))),
                row["out"].clone(),
            ),
            "inv_valid" => (
                json!(is_valid_inventory_folder_path(s(&row, "path"))),
                row["out"].clone(),
            ),
            "inv_resolve" => (
                inv_outcome(resolve_inventory_folder(
                    &strings(&row, "existing"),
                    opt(&row, "requested"),
                )),
                row["out"].clone(),
            ),
            "inv_paths" => {
                let p = s(&row, "path");
                (
                    json!({"folderOf": folder_of_inventory_path(p), "isDoc": is_inventory_item_document_path(p)}),
                    json!({"folderOf": row["folderOf"], "isDoc": row["isDoc"]}),
                )
            }
            "inv_store_path" => (
                json!(inventory_folder_store_path(s(&row, "folder"))),
                row["out"].clone(),
            ),
            other => panic!("unknown row kind {other}"),
        };
        let (g, w) = (
            serde_json::to_string(&got).unwrap(),
            serde_json::to_string(&want).unwrap(),
        );
        if g != w {
            failures.push(format!("{label}\n  v5: {g}\n  v4: {w}"));
        }
    }
    assert!(saw_meta, "no meta row — not this corpus");
    eprintln!("item folders: {rows} rows {kinds:?}");
    assert!(
        rows > 600,
        "corpus too small ({rows}) — a stale or truncated oracle"
    );
    assert!(
        failures.is_empty(),
        "{} mismatches:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
