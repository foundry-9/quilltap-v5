//! The Inventory's prompt section — what a character carries, told to the
//! model as a trailing per-turn section (v4 `lib/inventory/prompt-section.ts`,
//! `4fbdad186` + bug 253 at `322a5488e`; P4.D324, R-N).
//!
//! [`build_inventory_section`] reads the character's equipped set in the
//! chat, keeps objects with a `description` (what the characters know about
//! it; kits never appear, their members do), sorts them by `title ?? name`
//! and renders
//!
//! ```text
//! ## Carried and equipped
//! - Excalibur (sword): A longsword whose blade is never dulled…
//! - Signet ring: Proves the bearer is of House Vane.
//! ```
//!
//! Never in system block 1, and no builder version is bumped: it rides the
//! uncached trailing tail beside the progressions report, after it and before
//! the inform block. **Empty is absent**: with nothing equipped-and-described
//! it answers `None` and the caller adds nothing, so an unrelated turn stays
//! byte-identical. An object's `imageDescription` never reaches the model
//! here — that goes to the image pipelines.
//!
//! Reads only (no writes on the prompt path). Never fails.

use rusqlite::Connection;
use serde_json::Value;

use super::equipped_set::{equipped_ids_of, read_equipped_set};
use super::pool::{load_inventory_pool, LoadInventoryPoolOptions};
use super::resolve::{inventory_display_name, inventory_label};
use super::types::{InventoryFields, InventoryKind, ResolvedInventoryItem};
use crate::jsstr::js_trim;

/// The section's heading.
pub const INVENTORY_SECTION_HEADER: &str = "## Carried and equipped";

/// Render the section from resolved objects, or `None` when none has a
/// description (v4 `renderInventorySection`). Pure. The sort is v4's STABLE
/// `localeCompare(…, { sensitivity: 'base' })` on the label, so ties keep the
/// equipped-set order.
pub fn render_inventory_section(items: &[ResolvedInventoryItem]) -> Option<String> {
    let mut described: Vec<&ResolvedInventoryItem> = items
        .iter()
        .filter(|item| {
            item.kind() != InventoryKind::Kit
                && item.description().is_some_and(|d| !js_trim(d).is_empty())
        })
        .collect();
    if described.is_empty() {
        return None;
    }
    described.sort_by(|a, b| {
        crate::collation::locale_compare_base(&inventory_label(*a), &inventory_label(*b))
    });
    let lines: Vec<String> = described
        .iter()
        .map(|item| {
            format!(
                "- {}: {}",
                inventory_display_name(*item),
                js_trim(item.description().unwrap_or(""))
            )
        })
        .collect();
    Some(format!("{INVENTORY_SECTION_HEADER}\n{}", lines.join("\n")))
}

/// The trailing section for one character's turn, or `None` (v4
/// `buildInventorySection`). `chat` is the chat row (`id`, `projectId`,
/// `equippedInventory`). Nothing stored → no pool read at all (the common
/// case). A failed read WARNs and the turn continues without it.
pub fn build_inventory_section(
    main: &Connection,
    mount: &Connection,
    chat: &Value,
    character_id: &str,
) -> Option<String> {
    let equipped = chat.get("equippedInventory");
    if equipped_ids_of(equipped, character_id).is_empty() {
        return None;
    }
    let chat_id = chat.get("id").and_then(Value::as_str).unwrap_or_default();
    let project_id = chat.get("projectId").and_then(Value::as_str);
    match load_inventory_pool(
        main,
        mount,
        LoadInventoryPoolOptions {
            character_id,
            chat_id: Some(chat_id),
            project_id,
            own_records: None,
        },
    ) {
        Ok(pool) => {
            let section = render_inventory_section(&read_equipped_set(
                Some(chat_id),
                equipped,
                character_id,
                &pool,
            ));
            tracing::debug!(
                chatId = %chat_id,
                characterId = %character_id,
                present = section.is_some(),
                op = "prompt-section",
                context = "inventory",
                "[InventorySection] Built the carried-objects section"
            );
            section
        }
        Err(error) => {
            tracing::warn!(
                chatId = %chat_id,
                characterId = %character_id,
                context = "inventory",
                error = %crate::db::fallback::error_text(&error),
                "[InventorySection] Failed to build the carried-objects section; turn continues"
            );
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn item(v: Value) -> ResolvedInventoryItem {
        ResolvedInventoryItem(v.as_object().unwrap().clone())
    }

    #[test]
    fn empty_is_absent_and_kits_and_undescribed_objects_never_render() {
        assert_eq!(render_inventory_section(&[]), None);
        let kit = item(json!({"id": "k", "name": "kit", "kind": "kit", "description": "A kit."}));
        let blank =
            item(json!({"id": "b", "name": "pebble", "kind": "item", "description": "   "}));
        let bare = item(json!({"id": "n", "name": "nail", "kind": "item"}));
        assert_eq!(render_inventory_section(&[kit, blank, bare]), None);
    }

    #[test]
    fn lines_sort_by_label_base_sensitivity_stable_with_trimmed_descriptions() {
        let items = [
            item(
                json!({"id": "1", "name": "bun", "title": "eclair", "kind": "item", "description": " Third. "}),
            ),
            item(
                json!({"id": "2", "name": "lamp", "title": "Brass lamp", "kind": "item", "description": "Smokes."}),
            ),
            item(
                json!({"id": "3", "name": "cake", "title": "Eclair", "kind": "item", "description": "Second."}),
            ),
        ];
        assert_eq!(
            render_inventory_section(&items).unwrap(),
            "## Carried and equipped\n- Brass lamp (lamp): Smokes.\n- eclair (bun): Third.\n- Eclair (cake): Second."
        );
    }
}
