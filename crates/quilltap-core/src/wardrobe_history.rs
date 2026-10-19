//! Rendering an outfit-history entry — v4 `lib/wardrobe/history/
//! describe-entry.ts` at its FINAL shape (`ebb480edd` created it, `f46cf40a2`
//! flattened the summary, `4ae20977f` added the operator / Pascal labels; the
//! `e8e17b2d9` pin; P4.D315, C1 §6).
//!
//! An entry's slots resolved against a character's wearable pool, stale ids
//! marked rather than dropped, and a one-line summary. Shared by the
//! `wardrobe_history` tool (P4.D316) and `GET ?action=wardrobe-history`
//! (`api::chat_outfits::chat_wardrobe_history_get`) — ONE module.
//!
//! Entries store dissolved leaves; a legacy whole-composite id resolves to the
//! composite's own title. Pure: no I/O, no logging — the caller loads the pool
//! once.
//!
//! **JS whitespace.** The summary's `/^\s*[-*]\s+/` strip and `.trim()` use
//! JS `\s` ([`crate::jsstr::is_js_ws`] — it includes U+FEFF and excludes
//! U+0085; Rust's `char::is_whitespace` is the reverse).
//!
//! **`changed_by_label`'s third parameter** is v4's plain `selfLabel: string`
//! (`self_label: &str`): the GET passes the character's raw name or `the
//! character`, the tool passes `you`.

use serde::Serialize;
use serde_json::Value;

use crate::db::chat_wardrobe_history::{ChatWardrobeHistoryRow, WardrobeHistorySource};
use crate::jsstr::is_js_ws;
use crate::wardrobe::{describe_outfit, OutfitSlotValues, Slots, WARDROBE_SLOT_TYPES};
use crate::wearable_pool::WearablePool;

/// v4 `HistorySlotItem` — one id in an entry's slot, as the pool sees it now.
/// Key order `itemId, title, archived, missing`; `title` serialises `null`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistorySlotItem {
    pub item_id: String,
    pub title: Option<String>,
    pub archived: bool,
    pub missing: bool,
}

/// v4 `HistorySlots` — `Record<WardrobeItemType, HistorySlotItem[]>`: ALL FIVE
/// keys always, in `WARDROBE_SLOT_TYPES` order.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct HistorySlots {
    pub top: Vec<HistorySlotItem>,
    pub bottom: Vec<HistorySlotItem>,
    pub footwear: Vec<HistorySlotItem>,
    pub accessories: Vec<HistorySlotItem>,
    pub hair: Vec<HistorySlotItem>,
}

impl HistorySlots {
    /// A slot's items by canonical name (an unknown name reads as `top`,
    /// unreachable — callers walk [`WARDROBE_SLOT_TYPES`]).
    pub fn slot(&self, name: &str) -> &[HistorySlotItem] {
        match name {
            "bottom" => &self.bottom,
            "footwear" => &self.footwear,
            "accessories" => &self.accessories,
            "hair" => &self.hair,
            _ => &self.top,
        }
    }

    fn slot_mut(&mut self, name: &str) -> &mut Vec<HistorySlotItem> {
        match name {
            "bottom" => &mut self.bottom,
            "footwear" => &mut self.footwear,
            "accessories" => &mut self.accessories,
            "hair" => &mut self.hair,
            _ => &mut self.top,
        }
    }
}

/// JS `Boolean(v)` over an item field.
fn js_truthy(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|f| f != 0.0 && !f.is_nan()),
        Some(Value::String(s)) => !s.is_empty(),
        Some(_) => true,
    }
}

/// v4 `resolveHistorySlots(pool, slots)` (`:31-40`) — each stored id resolved
/// against the pool, duplicates and order KEPT. Archived items still resolve
/// (`archived: Boolean(item.archivedAt)` — `''` is NOT archived); an id the
/// pool does not hold is `{ title: null, archived: false, missing: true }`.
pub fn resolve_history_slots(pool: &WearablePool, slots: &Slots) -> HistorySlots {
    let mut out = HistorySlots::default();
    for slot in WARDROBE_SLOT_TYPES {
        let items = slots
            .slot(slot)
            .iter()
            .map(|item_id| match pool.get(item_id) {
                Some(item) => HistorySlotItem {
                    item_id: item_id.clone(),
                    title: item
                        .get("title")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    archived: js_truthy(item.get("archivedAt")),
                    missing: false,
                },
                None => HistorySlotItem {
                    item_id: item_id.clone(),
                    title: None,
                    archived: false,
                    missing: true,
                },
            })
            .collect();
        *out.slot_mut(slot) = items;
    }
    out
}

/// v4 `staleItemsOf(resolved)` (`:43-54`) — the ids in an entry that cannot
/// be put back on (archived or gone), canonical slot order, deduplicated by
/// `itemId` (first seen).
pub fn stale_items_of(resolved: &HistorySlots) -> Vec<HistorySlotItem> {
    let mut seen = std::collections::HashSet::new();
    let mut stale = Vec::new();
    for slot in WARDROBE_SLOT_TYPES {
        for entry in resolved.slot(slot) {
            if !(entry.archived || entry.missing) || !seen.insert(entry.item_id.clone()) {
                continue;
            }
            stale.push(entry.clone());
        }
    }
    stale
}

/// JS `line.replace(/^\s*[-*]\s+/, '')` (JS `\s`).
fn strip_list_marker(line: &str) -> &str {
    let rest = line.trim_start_matches(is_js_ws);
    let mut chars = rest.chars();
    match chars.next() {
        Some('-') | Some('*') => {
            let after = chars.as_str();
            let stripped = after.trim_start_matches(is_js_ws);
            if stripped.len() < after.len() {
                stripped
            } else {
                line
            }
        }
        _ => line,
    }
}

/// v4 `summarizeHistorySlots(resolved)` (`:61-73`) — ONE plain-text line:
/// `describeOutfit` over the titles (`<itemId> (no longer exists)` for a
/// missing id, `<title> (archived)` for an archived one, else `title ??
/// itemId`), its Markdown list flattened — each line's leading `- ` / `* `
/// marker stripped, every `**` removed, JS-trimmed, empties dropped, joined
/// `"; "`.
pub fn summarize_history_slots(resolved: &HistorySlots) -> String {
    let render = |slot: &str| -> Vec<String> {
        resolved
            .slot(slot)
            .iter()
            .map(|e| {
                if e.missing {
                    format!("{} (no longer exists)", e.item_id)
                } else if e.archived {
                    // A template literal over `null` renders `null`.
                    format!("{} (archived)", e.title.as_deref().unwrap_or("null"))
                } else {
                    e.title.clone().unwrap_or_else(|| e.item_id.clone())
                }
            })
            .collect()
    };
    let values = OutfitSlotValues {
        top: render("top"),
        bottom: render("bottom"),
        footwear: render("footwear"),
        accessories: render("accessories"),
        hair: render("hair"),
    };
    describe_outfit(&values)
        .split('\n')
        .map(|line| {
            let without_bold = strip_list_marker(line).replace("**", "");
            crate::jsstr::js_trim(&without_bold).to_string()
        })
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("; ")
}

/// v4 `changedByLabel(entry, actorName, selfLabel)` (`:84-118`) — who made an
/// entry, for a reader. `self_label` names the character the timeline belongs
/// to when they made it (`you` for the tool; the raw name or `the character`
/// for the GET). The thirteen labels, verbatim:
///
/// - `operator`: no actor → `the operator`; else `the operator, as <as>` with
///   `as = actor_name ?? (actor == the character ? self_label : none)`, or
///   `the operator` when there is no `as`;
/// - `pascal`: `Pascal's <tool>` when `actorToolName` is non-empty, else
///   `Pascal`; `… (rolled by <actor_name>)` when the actor is someone else and
///   named;
/// - `character`: no actor, or the character itself → `self_label`; else
///   `actor_name ?? "another character"`;
/// - `chat-start` → `chat opened`; `participant-added` → `joined the chat`;
///   `merge` → `merged`; `seeded` → `before history was kept`.
pub fn changed_by_label(
    row: &ChatWardrobeHistoryRow,
    actor_name: Option<&str>,
    self_label: &str,
) -> String {
    // JS truthiness: an empty actor id is no actor.
    let actor = row.actor_character_id.as_deref().filter(|a| !a.is_empty());
    match WardrobeHistorySource::from_stored(&row.source) {
        WardrobeHistorySource::Operator => {
            let Some(actor) = actor else {
                return "the operator".to_string();
            };
            let as_name = actor_name.or((actor == row.character_id).then_some(self_label));
            match as_name {
                // `as ? … : …` — an empty name is falsy.
                Some(name) if !name.is_empty() => format!("the operator, as {name}"),
                _ => "the operator".to_string(),
            }
        }
        WardrobeHistorySource::Pascal => {
            let pascal = match row.actor_tool_name.as_deref() {
                Some(tool) if !tool.is_empty() => format!("Pascal's {tool}"),
                _ => "Pascal".to_string(),
            };
            match (actor, actor_name) {
                (Some(actor), Some(name)) if actor != row.character_id && !name.is_empty() => {
                    format!("{pascal} (rolled by {name})")
                }
                _ => pascal,
            }
        }
        WardrobeHistorySource::Character => match actor {
            None => self_label.to_string(),
            Some(actor) if actor == row.character_id => self_label.to_string(),
            Some(_) => actor_name.unwrap_or("another character").to_string(),
        },
        WardrobeHistorySource::ChatStart => "chat opened".to_string(),
        WardrobeHistorySource::ParticipantAdded => "joined the chat".to_string(),
        WardrobeHistorySource::Merge => "merged".to_string(),
        WardrobeHistorySource::Seeded => "before history was kept".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(source: &str, actor: Option<&str>, tool: Option<&str>) -> ChatWardrobeHistoryRow {
        ChatWardrobeHistoryRow {
            id: "r".into(),
            chat_id: "chat".into(),
            character_id: "self".into(),
            entry_index: 0,
            slots: Slots::fresh(),
            slots_hash: String::new(),
            source: source.into(),
            actor_character_id: actor.map(str::to_string),
            actor_tool_name: tool.map(str::to_string),
            is_current: false,
            created_at: String::new(),
        }
    }

    /// v4's `labels who changed it` (`describe-entry.test.ts:43`) plus the
    /// thirteen labels as a literal table (Tier 2 item 23 — the tool passes
    /// `you`).
    #[test]
    fn the_thirteen_labels() {
        /// `(source, actor, tool, actor name, self label, label)`.
        type Case<'a> = (
            &'a str,
            Option<&'a str>,
            Option<&'a str>,
            Option<&'a str>,
            &'a str,
            &'a str,
        );
        let cases: &[Case<'_>] = &[
            ("operator", None, None, None, "you", "the operator"),
            (
                "operator",
                Some("other"),
                None,
                Some("Bertie"),
                "you",
                "the operator, as Bertie",
            ),
            (
                "operator",
                Some("self"),
                None,
                None,
                "Madeline",
                "the operator, as Madeline",
            ),
            ("operator", Some("other"), None, None, "you", "the operator"),
            ("pascal", None, None, None, "you", "Pascal"),
            (
                "pascal",
                None,
                Some("dress_for_dinner"),
                None,
                "you",
                "Pascal's dress_for_dinner",
            ),
            ("pascal", None, Some(""), None, "you", "Pascal"),
            (
                "pascal",
                Some("other"),
                None,
                Some("Jeeves"),
                "you",
                "Pascal (rolled by Jeeves)",
            ),
            (
                "pascal",
                Some("other"),
                Some("dress_for_dinner"),
                Some("Jeeves"),
                "you",
                "Pascal's dress_for_dinner (rolled by Jeeves)",
            ),
            (
                "pascal",
                Some("self"),
                Some("t"),
                Some("Me"),
                "you",
                "Pascal's t",
            ),
            ("character", None, None, None, "you", "you"),
            (
                "character",
                Some("self"),
                None,
                Some("Me"),
                "the character",
                "the character",
            ),
            (
                "character",
                Some("other"),
                None,
                Some("Jeeves"),
                "you",
                "Jeeves",
            ),
            (
                "character",
                Some("other"),
                None,
                None,
                "you",
                "another character",
            ),
            ("chat-start", None, None, None, "you", "chat opened"),
            (
                "participant-added",
                None,
                None,
                None,
                "you",
                "joined the chat",
            ),
            ("merge", None, None, None, "you", "merged"),
            ("seeded", None, None, None, "you", "before history was kept"),
            ("bogus", None, None, None, "you", "before history was kept"),
        ];
        for (source, actor, tool, name, self_label, want) in cases {
            assert_eq!(
                changed_by_label(&row(source, *actor, *tool), *name, self_label),
                *want,
                "{source} {actor:?} {tool:?} {name:?}"
            );
        }
    }

    #[test]
    fn the_list_marker_strip_is_js_whitespace() {
        assert_eq!(strip_list_marker("- **top:** x"), "**top:** x");
        assert_eq!(strip_list_marker("  * a"), "a");
        assert_eq!(strip_list_marker("-a"), "-a");
        assert_eq!(strip_list_marker("\u{FEFF}-\u{FEFF}a"), "a");
        assert_eq!(strip_list_marker("\u{85}- a"), "\u{85}- a");
    }
}
