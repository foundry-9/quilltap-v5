//! `wardrobe_history` handler (v4 `lib/tools/handlers/wardrobe-history-handler.ts`
//! `executeWardrobeHistoryTool` + `formatWardrobeHistoryResults`, `ebb480edd`;
//! `4ae20977f`'s operator clause).
//!
//! List the numbered outfits the calling character has worn in this chat —
//! the last `limit` (default 20), with the current one prepended when it falls
//! before the window. Each entry is rendered through the keystone's ONE
//! describe-entry module ([`crate::wardrobe_history`]). A history call changes
//! nothing. Inside a turn it reads the record as it stands: the turn's own
//! changes settle at turn end (v4's description says so — never flush early).

use std::collections::HashMap;

use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::db::chat_wardrobe_history::{ChatWardrobeHistoryRepository, ChatWardrobeHistoryRow};
use crate::wardrobe_history::{changed_by_label, resolve_history_slots, summarize_history_slots};

use super::wardrobe_shared::zod_safe_integer;
use super::wardrobe_tool_table::load_tool_pool;

/// v4's handler `CONTEXT`.
pub const CONTEXT: &str = "wardrobe-history-handler";

/// v4 `WARDROBE_HISTORY_DEFAULT_LIMIT`.
pub const WARDROBE_HISTORY_DEFAULT_LIMIT: usize = 20;

/// v4 `WardrobeHistoryToolEntry`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WardrobeHistoryToolEntry {
    pub index: i64,
    /// The row's raw `createdAt` ISO string, not localized.
    pub at: String,
    pub source: String,
    pub changed_by: String,
    pub is_current: bool,
    pub summary: String,
}

/// v4 `WardrobeHistoryToolOutput` — `current_index` serialises `null`, never
/// skipped; `gap_before_window` only when the gap fired; `error` only on a
/// failure.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WardrobeHistoryToolOutput {
    pub success: bool,
    pub current_index: Option<i64>,
    pub total: usize,
    pub entries: Vec<WardrobeHistoryToolEntry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gap_before_window: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

fn failure(error: impl Into<String>) -> WardrobeHistoryToolOutput {
    WardrobeHistoryToolOutput {
        success: false,
        current_index: None,
        total: 0,
        entries: Vec::new(),
        gap_before_window: None,
        error: Some(error.into()),
    }
}

/// v4 `validateWardrobeHistoryInput` — `safeParse(input ?? {})` over `{ limit:
/// z.number().int().positive().max(100).optional() }`: `0` refused, `100`
/// admitted, `101` refused; `null` input is `{}`; `{ limit: null }` refused.
/// `Some(None)` = valid with no limit.
pub fn validate(input: &Value) -> Option<Option<usize>> {
    let obj = match input {
        Value::Null => return Some(None),
        Value::Object(obj) => obj,
        _ => return None,
    };
    match obj.get("limit") {
        None => Some(None),
        Some(v) => zod_safe_integer(v)
            .filter(|n| (1..=100).contains(n))
            .map(|n| Some(n as usize)),
    }
}

/// v4 `windowHistory(rows, limit)` — the last `limit` rows; when the current
/// row falls BEFORE that window it is prepended and the gap is flagged. Pure.
pub fn window_history(
    rows: &[ChatWardrobeHistoryRow],
    limit: usize,
) -> (Vec<ChatWardrobeHistoryRow>, bool) {
    let start = rows.len().saturating_sub(limit);
    let window = &rows[start..];
    match rows.iter().position(|r| r.is_current) {
        Some(current) if current < start => {
            let mut shown = Vec::with_capacity(window.len() + 1);
            shown.push(rows[current].clone());
            shown.extend_from_slice(window);
            (shown, true)
        }
        _ => (window.to_vec(), false),
    }
}

/// v4 `executeWardrobeHistoryTool`, inside the write (both connections; it
/// writes nothing).
pub fn execute(
    main: &Connection,
    mount: &Connection,
    chat_id: &str,
    character_id: &str,
    input: &Value,
) -> WardrobeHistoryToolOutput {
    let Some(limit) = validate(input) else {
        return failure(
            "Invalid input: \"limit\" must be a whole number from 1 to 100, or omitted.",
        );
    };
    if character_id.is_empty() {
        return failure("wardrobe_history needs a character.");
    }

    let rows = match ChatWardrobeHistoryRepository::new(main).find_entries(chat_id, character_id) {
        Ok(rows) => rows,
        Err(error) => {
            let message = crate::db::fallback::error_text(&error);
            tracing::warn!(
                context = CONTEXT,
                chatId = chat_id,
                characterId = character_id,
                error = %message,
                "Wardrobe history read failed"
            );
            return failure(format!("Could not read the outfit record: {message}"));
        }
    };
    if rows.is_empty() {
        return WardrobeHistoryToolOutput {
            success: true,
            current_index: None,
            total: 0,
            entries: Vec::new(),
            gap_before_window: None,
            error: None,
        };
    }

    let pool = load_tool_pool(main, mount, chat_id, character_id);
    let (shown, gap_before_window) =
        window_history(&rows, limit.unwrap_or(WARDROBE_HISTORY_DEFAULT_LIMIT));

    // The character's own calls read "you"; an operator speaking as them is
    // still named (`4ae20977f`). Names for SHOWN rows only, each read once,
    // from the RAW row (`findByIdRaw(actor)?.name ?? null` — NULLISH: an empty
    // name stays empty), a failed read naming nobody.
    let mut actor_names: HashMap<String, Option<String>> = HashMap::new();
    for row in &shown {
        let Some(actor) = row.actor_character_id.as_deref().filter(|a| !a.is_empty()) else {
            continue;
        };
        if (actor == character_id && row.source != "operator") || actor_names.contains_key(actor) {
            continue;
        }
        let name = crate::db::characters_read::find_by_id_raw_or_none(main, actor)
            .as_ref()
            .and_then(|c| c.get("name"))
            .and_then(Value::as_str)
            .map(str::to_string);
        actor_names.insert(actor.to_string(), name);
    }

    let entries: Vec<WardrobeHistoryToolEntry> = shown
        .iter()
        .map(|row| {
            let actor_name = row
                .actor_character_id
                .as_deref()
                .filter(|a| !a.is_empty())
                .and_then(|a| actor_names.get(a).cloned().flatten());
            WardrobeHistoryToolEntry {
                index: row.entry_index,
                at: row.created_at.clone(),
                source: row.source.clone(),
                changed_by: changed_by_label(row, actor_name.as_deref(), "you"),
                is_current: row.is_current,
                summary: summarize_history_slots(&resolve_history_slots(&pool, &row.slots)),
            }
        })
        .collect();

    tracing::debug!(
        context = CONTEXT,
        chatId = chat_id,
        characterId = character_id,
        total = rows.len(),
        shown = entries.len(),
        "Listed wardrobe history"
    );

    WardrobeHistoryToolOutput {
        success: true,
        current_index: rows.iter().find(|r| r.is_current).map(|r| r.entry_index),
        total: rows.len(),
        entries,
        gap_before_window: gap_before_window.then_some(true),
        error: None,
    }
}

/// v4 `formatWardrobeHistoryResults`. A null `current_index` with rows present
/// (a timeline with no current row) renders the literal `null`, as JS's
/// template interpolation does.
pub fn format(output: &WardrobeHistoryToolOutput) -> String {
    if !output.success {
        let error = output.error.as_deref().filter(|e| !e.is_empty());
        return format!("Wardrobe Error: {}", error.unwrap_or("Unknown error"));
    }
    if output.total == 0 {
        return "No outfits recorded yet in this chat. The first change of clothes starts the record."
            .to_string();
    }
    let current = output
        .current_index
        .map_or_else(|| "null".to_string(), |i| i.to_string());
    let mut lines = vec![format!(
        "Outfits in this chat: {} (numbered 0 to {}). Wearing now: {current}.",
        output.total,
        output.total as i64 - 1
    )];
    for (i, entry) in output.entries.iter().enumerate() {
        if i == 1 && output.gap_before_window == Some(true) {
            lines.push("  \u{2026}".to_string());
        }
        let marker = if entry.is_current {
            " \u{2190} wearing now"
        } else {
            ""
        };
        lines.push(format!(
            "  {}. {} \u{2014} {}: {}{marker}",
            entry.index, entry.at, entry.changed_by, entry.summary
        ));
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    //! v4 `wardrobe-history-rollback-handlers.test.ts`'s history block (the pure
    //! halves) + the validation table (T13).
    use super::*;
    use crate::wardrobe::Slots;
    use serde_json::json;

    fn row(index: i64, current: bool) -> ChatWardrobeHistoryRow {
        ChatWardrobeHistoryRow {
            id: format!("r{index}"),
            chat_id: "c".into(),
            character_id: "ch".into(),
            entry_index: index,
            slots: Slots::fresh(),
            slots_hash: String::new(),
            source: "character".into(),
            actor_character_id: None,
            actor_tool_name: None,
            is_current: current,
            created_at: format!("2026-01-0{}T00:00:00.000Z", index + 1),
        }
    }

    #[test]
    fn validation_follows_the_zod_schema() {
        assert_eq!(validate(&json!(null)), Some(None));
        assert_eq!(validate(&json!({ "limit": 1 })), Some(Some(1)));
        assert_eq!(validate(&json!({ "limit": 100 })), Some(Some(100)));
        for bad in [
            json!({ "limit": 0 }),
            json!({ "limit": 101 }),
            json!({ "limit": null }),
            json!({ "limit": "5" }),
            json!({ "limit": 2.5 }),
            json!([1]),
        ] {
            assert_eq!(validate(&bad), None, "{bad}");
        }
    }

    #[test]
    fn the_window_keeps_the_current_row_and_flags_the_gap() {
        let rows: Vec<_> = (0..5).map(|i| row(i, i == 1)).collect();
        let (shown, gap) = window_history(&rows, 2);
        assert_eq!(
            shown.iter().map(|r| r.entry_index).collect::<Vec<_>>(),
            vec![1, 3, 4]
        );
        assert!(gap);
        let (shown, gap) = window_history(&rows, 4);
        assert_eq!(
            shown.iter().map(|r| r.entry_index).collect::<Vec<_>>(),
            vec![1, 2, 3, 4]
        );
        assert!(!gap, "inside the window: nothing added");
        let (shown, gap) = window_history(&rows, 20);
        assert_eq!(shown.len(), 5);
        assert!(!gap);
        let none: Vec<_> = (0..3).map(|i| row(i, false)).collect();
        assert_eq!(window_history(&none, 1).0.len(), 1);
    }

    #[test]
    fn formats_the_list_with_v4s_code_points() {
        let entry = |index, current| WardrobeHistoryToolEntry {
            index,
            at: format!("t{index}"),
            source: "character".into(),
            changed_by: "you".into(),
            is_current: current,
            summary: "a Hat".into(),
        };
        let out = WardrobeHistoryToolOutput {
            success: true,
            current_index: Some(1),
            total: 5,
            entries: vec![entry(1, true), entry(3, false), entry(4, false)],
            gap_before_window: Some(true),
            error: None,
        };
        assert_eq!(
            format(&out),
            "Outfits in this chat: 5 (numbered 0 to 4). Wearing now: 1.\n  1. t1 \u{2014} you: a Hat \u{2190} wearing now\n  \u{2026}\n  3. t3 \u{2014} you: a Hat\n  4. t4 \u{2014} you: a Hat"
        );
        let null_current = WardrobeHistoryToolOutput {
            current_index: None,
            entries: vec![entry(0, false)],
            total: 1,
            gap_before_window: None,
            ..out.clone()
        };
        assert!(format(&null_current)
            .starts_with("Outfits in this chat: 1 (numbered 0 to 0). Wearing now: null."));
        let empty = WardrobeHistoryToolOutput {
            total: 0,
            entries: Vec::new(),
            current_index: None,
            gap_before_window: None,
            ..out
        };
        assert_eq!(
            format(&empty),
            "No outfits recorded yet in this chat. The first change of clothes starts the record."
        );
        assert_eq!(format(&failure("x")), "Wardrobe Error: x");
        assert_eq!(
            serde_json::to_string(&failure("x")).unwrap(),
            r#"{"success":false,"current_index":null,"total":0,"entries":[],"error":"x"}"#
        );
    }
}
