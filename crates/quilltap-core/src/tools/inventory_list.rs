//! `inventory_list` — the objects the character carries and can reach, with
//! their states in this chat (v4 `lib/tools/handlers/inventory-list-handler.ts`
//! + `inventory-list-tool.ts`, `e1872c94f`; P4.D328).

use std::collections::HashMap;

use serde::Serialize;
use serde_json::{Map, Value};

use crate::inventory::context::load_inventory_context;
use crate::inventory::refs::InventoryState;
use crate::inventory::resolve::kit_state;
use crate::inventory::types::{InventoryFields, ResolvedInventoryItem};
use crate::jsstr::{is_js_ws, js_trim, utf16_len};

use super::inventory_shared::{
    display_name_of, error_message_of, input_object, kind_of, opt_bool, opt_string, state_for,
    tier_of, trimmed_title, InvalidInput, InventoryObjectKind, InventoryToolLog,
    InventoryToolLogLevel, InventoryToolTier, Parsed,
};
use super::inventory_tool_table::{InventoryHandled, InventoryHandlerCx};

/// One listed object (v4 `InventoryListItemResult` — the key order is the
/// wire).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct InventoryListItemResult {
    pub id: String,
    pub name: String,
    pub title: Option<String>,
    /// `None` for an archetype (pick-up-able, never carried).
    pub state: Option<InventoryState>,
    pub kind: InventoryObjectKind,
    pub tier: InventoryToolTier,
    pub folder: Option<String>,
    pub is_own: bool,
    pub description: Option<String>,
    /// A kit's state: how many of its members are equipped.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kit_state: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub archived: Option<bool>,
}

/// v4 `InventoryListToolOutput`.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct InventoryListToolOutput {
    pub success: bool,
    pub items: Vec<InventoryListItemResult>,
    pub total_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

fn failure(error: String) -> InventoryListToolOutput {
    InventoryListToolOutput {
        success: false,
        items: Vec::new(),
        total_count: 0,
        error: Some(error),
    }
}

/// v4 `inventoryListToolInputSchema`'s parsed object, in SHAPE order (the
/// `filters` field of the success line).
struct ListInput {
    equipped: Option<bool>,
    kind: Option<String>,
    folder: Option<String>,
    tier: Option<String>,
    include_archived: Option<bool>,
}

impl ListInput {
    fn filters(&self) -> Value {
        let mut m = Map::new();
        if let Some(v) = self.equipped {
            m.insert("equipped".into(), Value::Bool(v));
        }
        if let Some(v) = &self.kind {
            m.insert("kind".into(), Value::String(v.clone()));
        }
        if let Some(v) = &self.folder {
            m.insert("folder".into(), Value::String(v.clone()));
        }
        if let Some(v) = &self.tier {
            m.insert("tier".into(), Value::String(v.clone()));
        }
        if let Some(v) = self.include_archived {
            m.insert("include_archived".into(), Value::Bool(v));
        }
        Value::Object(m)
    }
}

fn parse(input: &Value) -> Option<ListInput> {
    let o = input_object(input)?;
    let one_of = |key: &str, allowed: &[&str]| -> Parsed<String> {
        match opt_string(o, key, None, None)? {
            Some(s) if !allowed.contains(&s.as_str()) => Err(InvalidInput),
            other => Ok(other),
        }
    };
    Some(ListInput {
        equipped: opt_bool(o, "equipped").ok()?,
        kind: one_of("kind", &["item", "kit", "archetype"]).ok()?,
        folder: opt_string(o, "folder", None, None).ok()?,
        tier: one_of("tier", &["own", "group", "project", "general"]).ok()?,
        include_archived: opt_bool(o, "include_archived").ok()?,
    })
}

const EXCERPT: usize = 140;

/// v4 `excerpt`: whitespace runs collapsed to one space, trimmed; longer than
/// 140 UTF-16 units → the first 139 + `…`.
fn excerpt(text: Option<&str>) -> Option<String> {
    let mut flat = String::new();
    let mut in_ws = false;
    for c in text.unwrap_or("").chars() {
        if is_js_ws(c) {
            if !in_ws {
                flat.push(' ');
            }
            in_ws = true;
        } else {
            flat.push(c);
            in_ws = false;
        }
    }
    let flat = js_trim(&flat).to_string();
    if flat.is_empty() {
        return None;
    }
    if utf16_len(&flat) > EXCERPT {
        return Some(format!(
            "{}…",
            crate::jsstr::utf16_truncate(&flat, EXCERPT - 1)
        ));
    }
    Some(flat)
}

/// v4 `inFolder`: the wanted folder trimmed, its leading / trailing slashes
/// stripped, lower-cased; empty → everything; else the object's folder (lower
/// case) equals it or lies beneath it.
fn in_folder(item: &ResolvedInventoryItem, folder: &str) -> bool {
    let want = js_trim(folder)
        .trim_start_matches('/')
        .trim_end_matches('/')
        .to_lowercase();
    if want.is_empty() {
        return true;
    }
    let have = item.folder().to_lowercase();
    have == want || have.starts_with(&format!("{want}/"))
}

pub fn execute(
    cx: &InventoryHandlerCx<'_>,
    args: &Value,
) -> InventoryHandled<InventoryListToolOutput> {
    let Some(parsed) = parse(args) else {
        return InventoryHandled::logged(
            failure("Invalid input for inventory_list.".into()),
            InventoryToolLog::outcome("invalid input"),
            InventoryToolLogLevel::Info,
        );
    };
    let ctx = match load_inventory_context(cx.main, cx.mount, cx.character_id, Some(cx.chat_id)) {
        Ok(ctx) => ctx,
        Err(e) => {
            let error = error_message_of(&e.into());
            return InventoryHandled::logged(
                failure(error.clone()),
                InventoryToolLog {
                    error: Some(error),
                    ..InventoryToolLog::outcome("failed")
                },
                InventoryToolLogLevel::Info,
            );
        }
    };
    let by_id: HashMap<&str, &ResolvedInventoryItem> =
        ctx.pool.items.iter().map(|i| (i.id(), i)).collect();
    let equipped_items: Vec<ResolvedInventoryItem> = ctx
        .pool
        .items
        .iter()
        .filter(|i| ctx.equipped_ids.contains(i.id()))
        .cloned()
        .collect();

    let mut rows: Vec<InventoryListItemResult> = Vec::new();
    for item in &ctx.pool.items {
        let own = ctx.pool.owns(item.id());
        if item.is_archived() && !(parsed.include_archived == Some(true) && own) {
            continue;
        }
        let kind = kind_of(item);
        if parsed.kind.as_deref().is_some_and(|k| k != kind.as_str()) {
            continue;
        }
        let tier = tier_of(item, own);
        if parsed.tier.as_deref().is_some_and(|t| t != tier.as_str()) {
            continue;
        }
        if let Some(folder) = parsed.folder.as_deref().filter(|f| !f.is_empty()) {
            if !in_folder(item, folder) {
                continue;
            }
        }
        let kit = item.is_kit().then(|| {
            kit_state(&item.item_ids(), &equipped_items, |id| {
                by_id.get(id).map(|i| (*i).clone())
            })
        });
        let equipped = match kit {
            Some(k) => k == crate::inventory::types::KitState::All,
            None => ctx.equipped_ids.contains(item.id()),
        };
        if let Some(want) = parsed.equipped {
            if item.is_archetype() || want != equipped {
                continue;
            }
        }
        rows.push(InventoryListItemResult {
            id: item.id().to_string(),
            name: item.name().to_string(),
            title: trimmed_title(item),
            state: state_for(item, own, equipped),
            kind,
            tier,
            folder: Some(item.folder().to_string()).filter(|f| !f.is_empty()),
            is_own: own,
            description: excerpt(item.description()),
            kit_state: kit.map(|k| k.as_str()),
            archived: item.is_archived().then_some(true),
        });
    }
    let count = rows.len();
    InventoryHandled::logged(
        InventoryListToolOutput {
            success: true,
            items: rows,
            total_count: count,
            error: None,
        },
        InventoryToolLog {
            count: Some(count),
            filters: Some(parsed.filters()),
            ..InventoryToolLog::outcome("listed")
        },
        InventoryToolLogLevel::Debug,
    )
}

/// v4 `formatInventoryListResults`.
pub fn format(output: &InventoryListToolOutput) -> String {
    if !output.success {
        return format!(
            "Inventory Error: {}",
            output
                .error
                .as_deref()
                .filter(|e| !e.is_empty())
                .unwrap_or("Unknown error")
        );
    }
    if output.items.is_empty() {
        return "Inventory: nothing matches.".to_string();
    }
    let n = output.total_count;
    let mut lines = vec![format!(
        "Inventory ({n} object{}):",
        if n == 1 { "" } else { "s" }
    )];
    for row in &output.items {
        let label = display_name_of(&row.name, row.title.as_deref());
        let state = if row.kind == InventoryObjectKind::Archetype {
            "archetype — pick up an instance"
        } else {
            row.state.map_or("null", InventoryState::as_str)
        };
        let kit = row
            .kit_state
            .map(|k| format!(" [kit: {k}]"))
            .unwrap_or_default();
        let folder = row
            .folder
            .as_deref()
            .map(|f| format!(" [folder: {f}]"))
            .unwrap_or_default();
        let shared = if row.is_own {
            ""
        } else {
            " [shared — read-only]"
        };
        let archived = if row.archived == Some(true) {
            " [archived]"
        } else {
            ""
        };
        let description = row
            .description
            .as_deref()
            .map(|d| format!(" - {d}"))
            .unwrap_or_default();
        lines.push(format!(
            "  {label} [{}] ({state}, {}){kit}{folder}{shared}{archived}{description}",
            row.id,
            row.tier.as_str()
        ));
    }
    lines.join("\n")
}
