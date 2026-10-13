//! `wardrobe_list` handler (v4 `lib/tools/handlers/wardrobe-list-handler.ts`
//! `executeWardrobeListTool` + `formatWardrobeListResults`).
//!
//! Lists everything the character can wear — the tool call's ONE wearable pool
//! (v4 `9ecebf986`: their own vault over their groups' stores, the chat's
//! project stores behind the roster, and Quilltap General), archived items
//! dropped PER TIER before shadowing (`pool.wearable()`, so an archived own
//! copy no longer hides the shared namesake) — filters by type /
//! appropriateness, and marks each item's equipped slots, wear, composite
//! metadata and (v4 `4a38222e6`) the operator's `folder`. Component titles
//! resolve against the WHOLE pool, so an archived or filtered-out part still
//! names itself.

use rusqlite::Connection;
use serde::Serialize;
use serde_json::Value;

use crate::db::chats_outfits::ChatOutfitsRepository;

use super::wardrobe_shared::find_equipped_slots;
use super::wardrobe_tool_table::load_tool_pool;

/// One item in the list result (v4 `WardrobeListItemResult`). The composite fields
/// are omitted for leaf items (v4's conditional spread).
#[derive(Debug, Serialize)]
pub struct WardrobeListItemResult {
    pub item_id: String,
    pub title: String,
    pub description: Option<String>,
    pub image_prompt: Option<String>,
    /// The item's current picture (v4 `b3f937076` — `item.imageFileId ??
    /// null`; always serialized).
    pub image_file_id: Option<String>,
    pub types: Vec<String>,
    pub appropriateness: Option<String>,
    pub is_own: bool,
    /// The operator's folder for the item under its wardrobe (e.g.
    /// `Formal/Evening`), `null` at the root (v4 `4a38222e6`: `item.folder ||
    /// null`). Filing only — it says nothing about when to wear it.
    pub folder: Option<String>,
    pub is_equipped: bool,
    pub equipped_slot: Option<String>,
    /// Times the whole HOUSEHOLD has worn it — other characters' wears
    /// included, so context, not the caller's own (v4 `wardrobe-list-tool.ts:
    /// 81-92`, `ed8b15b50` bug 184; `0` = never worn by anyone).
    pub wear_count: i64,
    /// When anyone in the household last wore it (`null` = never).
    pub last_worn_at: Option<String>,
    /// Times the CALLING character has worn it (`0` = never by them).
    pub worn_by_you: i64,
    /// When the calling character last wore it (`null` = never; always
    /// serialized).
    pub last_worn_by_you_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_composite: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub component_item_ids: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub component_titles: Option<Vec<String>>,
}

/// v4 `WardrobeListToolOutput` (presets omitted — the handler never populates it).
#[derive(Debug, Serialize)]
pub struct WardrobeListToolOutput {
    pub success: bool,
    pub items: Vec<WardrobeListItemResult>,
    pub total_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Validated `wardrobe_list` input (v4 `WardrobeListToolInput`).
struct ListInput {
    type_filter: Option<Vec<String>>,
    appropriateness_filter: Option<String>,
    include_equipped: Option<bool>,
}

const VALIDATION_ERROR: &str = "Invalid input: type_filter must be a string array, appropriateness_filter must be a string, include_equipped must be a boolean";

/// v4 `validateWardrobeListInput` (`wardrobeListToolInputSchema.safeParse`): an
/// object whose optional fields, if present, have the right types. Unknown keys are
/// stripped (Zod non-strict) — since v4 `9ecebf986` that includes
/// `include_presets`, which the schema no longer declares, so a non-boolean
/// `include_presets` succeeds. `None` ⇒ validation failure.
fn validate(args: &Value) -> Option<ListInput> {
    let obj = args.as_object()?;
    let type_filter = match obj.get("type_filter") {
        None | Some(Value::Null) => None,
        Some(Value::Array(a)) => {
            let mut v = Vec::with_capacity(a.len());
            for e in a {
                v.push(e.as_str()?.to_string());
            }
            Some(v)
        }
        Some(_) => return None,
    };
    let appropriateness_filter = match obj.get("appropriateness_filter") {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) => Some(s.clone()),
        Some(_) => return None,
    };
    let include_equipped = match obj.get("include_equipped") {
        None | Some(Value::Null) => None,
        Some(Value::Bool(b)) => Some(*b),
        Some(_) => return None,
    };
    Some(ListInput {
        type_filter,
        appropriateness_filter,
        include_equipped,
    })
}

fn str_field(item: &Value, key: &str) -> Option<String> {
    item.get(key).and_then(Value::as_str).map(str::to_string)
}

fn types_of(item: &Value) -> Vec<String> {
    item.get("types")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|e| e.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn component_ids_of(item: &Value) -> Vec<String> {
    item.get("componentItemIds")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|e| e.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// Execute `wardrobe_list` (v4 `executeWardrobeListTool`).
pub fn execute(
    main: &Connection,
    mount: &Connection,
    user_id: &str,
    chat_id: &str,
    character_id: &str,
    args: &Value,
) -> WardrobeListToolOutput {
    let Some(input) = validate(args) else {
        tracing::warn!(
            target: "quilltap::tools",
            context = LOG_CONTEXT,
            userId = user_id,
            characterId = character_id,
            input = %args,
            "Wardrobe list tool validation failed",
        );
        return WardrobeListToolOutput {
            success: false,
            items: Vec::new(),
            total_count: 0,
            error: Some(VALIDATION_ERROR.to_string()),
        };
    };
    // v4's outer catch (ERROR `Wardrobe list tool execution failed`) guards a
    // body that, at `9ecebf986`, cannot throw: the pool loader fails soft per
    // tier, and the equipped and ledger reads are fallback reads. UNREACHABLE
    // through v4's real code — pinned NEGATIVE (an unreadable instance answers
    // an empty list), not ported.
    run(main, mount, user_id, chat_id, character_id, &input)
}

/// v4's `context` field on every line the handler logs.
const LOG_CONTEXT: &str = "wardrobe-list-handler";

fn run(
    main: &Connection,
    mount: &Connection,
    user_id: &str,
    chat_id: &str,
    character_id: &str,
    input: &ListInput,
) -> WardrobeListToolOutput {
    // Everything the character can wear: their own vault over their groups'
    // stores, the chat's project stores and Quilltap General.
    let pool = load_tool_pool(main, mount, chat_id, character_id);
    let all_items = pool.wearable();

    let equipped =
        ChatOutfitsRepository::new(main).get_equipped_outfit_for_character(chat_id, character_id);

    // Type filter.
    let type_lower: Option<Vec<String>> = input
        .type_filter
        .as_ref()
        .filter(|t| !t.is_empty())
        .map(|t| t.iter().map(|s| s.to_lowercase()).collect());
    // Appropriateness filter (trimmed non-empty).
    let appr_lower: Option<String> = input
        .appropriateness_filter
        .as_ref()
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.to_lowercase());

    let mut filtered: Vec<&Value> = all_items.iter().collect();
    if let Some(tl) = &type_lower {
        filtered.retain(|item| {
            types_of(item)
                .iter()
                .any(|t| tl.contains(&t.to_lowercase()))
        });
    }
    if let Some(al) = &appr_lower {
        filtered.retain(|item| {
            item.get("appropriateness")
                .and_then(Value::as_str)
                .is_some_and(|a| a.to_lowercase().contains(al.as_str()))
        });
    }

    // One ledger read for every listed item's wear, the caller's own share
    // kept apart from the household's (v4 `ed8b15b50`, bug 184 — over the type
    // / appropriateness-filtered items, before the `include_equipped` filter).
    // A failed read is v4's fallback: never worn.
    let filtered_ids: Vec<String> = filtered
        .iter()
        .filter_map(|item| str_field(item, "id"))
        .collect();
    let wear_summaries = crate::db::wardrobe_wear_stats::WardrobeWearStatsRepository::new(main)
        .find_summaries_for_wearer(&filtered_ids, character_id);

    let result_items: Vec<WardrobeListItemResult> = filtered
        .into_iter()
        .map(|item| {
            let id = str_field(item, "id").unwrap_or_default();
            let (household, yours) = match wear_summaries.get(&id) {
                Some(wear) => (wear.household.clone(), wear.yours.clone()),
                None => (
                    crate::db::wardrobe_wear_stats::never_worn_summary(),
                    crate::db::wardrobe_wear_stats::never_worn_summary(),
                ),
            };
            let equipped_slots = find_equipped_slots(&id, equipped.as_ref());
            let component_item_ids = component_ids_of(item);
            let is_composite = !component_item_ids.is_empty();
            // Component titles resolve against the whole pool, so a filtered-out
            // (or archived) part still names itself.
            let component_titles: Option<Vec<String>> = if is_composite {
                Some(
                    pool.get_many(&component_item_ids)
                        .into_iter()
                        .map(|c| str_field(c, "title").unwrap_or_default())
                        .collect(),
                )
            } else {
                None
            };
            WardrobeListItemResult {
                item_id: id.clone(),
                title: str_field(item, "title").unwrap_or_default(),
                description: str_field(item, "description"),
                image_prompt: str_field(item, "imagePrompt"),
                image_file_id: str_field(item, "imageFileId"),
                types: types_of(item),
                appropriateness: str_field(item, "appropriateness"),
                // The field compare (v4 keeps it here; `wardrobe_read` asks the
                // pool's `owns`).
                is_own: item.get("characterId").and_then(Value::as_str) == Some(character_id),
                folder: str_field(item, "folder").filter(|f| !f.is_empty()),
                is_equipped: !equipped_slots.is_empty(),
                equipped_slot: equipped_slots.first().cloned(),
                wear_count: household.wear_count,
                last_worn_at: household.last_worn_at,
                worn_by_you: yours.wear_count,
                last_worn_by_you_at: yours.last_worn_at,
                is_composite: if is_composite { Some(true) } else { None },
                component_item_ids: if is_composite {
                    Some(component_item_ids)
                } else {
                    None
                },
                component_titles,
            }
        })
        .collect();

    // include_equipped === false drops equipped items.
    let final_items: Vec<WardrobeListItemResult> = if input.include_equipped == Some(false) {
        result_items
            .into_iter()
            .filter(|i| !i.is_equipped)
            .collect()
    } else {
        result_items
    };

    // v4's INFO, every count over the final items (after `include_equipped`).
    // JS truthiness: `!!type_filter` is true for `[]`; `!!appropriateness_filter`
    // is false for `''`.
    tracing::info!(
        target: "quilltap::tools",
        context = LOG_CONTEXT,
        userId = user_id,
        characterId = character_id,
        chatId = chat_id,
        totalItems = all_items.len(),
        filteredCount = final_items.len(),
        hasTypeFilter = input.type_filter.is_some(),
        hasAppropriatenessFilter = input
            .appropriateness_filter
            .as_deref()
            .is_some_and(|s| !s.is_empty()),
        includeEquipped = input.include_equipped != Some(false),
        compositeCount = final_items
            .iter()
            .filter(|i| i.is_composite == Some(true))
            .count(),
        neverWornCount = final_items.iter().filter(|i| i.wear_count == 0).count(),
        neverWornByCallerCount = final_items.iter().filter(|i| i.worn_by_you == 0).count(),
        withPictureCount = final_items
            .iter()
            .filter(|i| i.image_file_id.as_deref().is_some_and(|f| !f.is_empty()))
            .count(),
        "Wardrobe list completed",
    );

    let total_count = final_items.len();
    WardrobeListToolOutput {
        success: true,
        items: final_items,
        total_count,
        error: None,
    }
}

/// v4 `listTimes(count)` (`:209-212`) — "once" or "12×" for the compact
/// list note (2 is `2×`, never "twice").
fn list_times(count: i64) -> String {
    if count == 1 {
        "once".to_string()
    } else {
        format!("{count}×")
    }
}

/// v4 `formatWardrobeListWearNote(item, nowMs)` (`wardrobe-list-handler.ts:
/// 225-237`, `ed8b15b50`) — the compact wear note a listed item carries, from
/// the caller's side with the household's total as context (bug 184):
///
///  - ` · never worn` — nobody has
///  - ` · never worn by you (worn 115× by others)`
///  - ` · worn by you 12×, last yesterday` — only the caller has worn it
///  - ` · worn by you 12×, last yesterday (115× in the household)`
///
/// v4's `Pick` names `last_worn_at` but never reads it, so it is not taken
/// here. A count with no readable date (a hand-edited row) drops the date
/// part; the date reads through [`format_worn_relative`] (bug 183 — never
/// "last last week"). `now_ms` is the caller's clock (v4's injectable
/// `nowMs`).
///
/// [`format_worn_relative`]: crate::format_time::format_worn_relative
pub fn format_wardrobe_list_wear_note(
    wear_count: i64,
    worn_by_you: i64,
    last_worn_by_you_at: Option<&str>,
    now_ms: f64,
) -> String {
    if wear_count == 0 {
        return " · never worn".to_string();
    }
    if worn_by_you == 0 {
        return format!(
            " · never worn by you (worn {} by others)",
            list_times(wear_count)
        );
    }
    let your_last_ms = last_worn_by_you_at
        .filter(|s| !s.is_empty())
        .and_then(crate::episodic::js_date_parse_ms);
    let when = match your_last_ms {
        Some(ms) => format!(
            ", last {}",
            crate::format_time::format_worn_relative(ms as f64, now_ms)
        ),
        None => String::new(),
    };
    let yours = format!("worn by you {}{when}", list_times(worn_by_you));
    if wear_count <= worn_by_you {
        return format!(" · {yours}");
    }
    format!(" · {yours} ({} in the household)", list_times(wear_count))
}

/// v4 `formatWardrobeListResults(output, nowMs = Date.now())` on the
/// production clock — the relative "last worn" dates read now.
pub fn format(output: &WardrobeListToolOutput) -> String {
    format_at(output, crate::clock::now_unix_ms() as f64)
}

/// [`format`] at a pinned clock (v4's second parameter) — the differentials
/// pass one fixed `nowMs` on both sides so the relative dates compare.
pub fn format_at(output: &WardrobeListToolOutput, now_ms: f64) -> String {
    if !output.success {
        return format!(
            "Wardrobe Error: {}",
            output.error.as_deref().unwrap_or("Unknown error")
        );
    }
    if output.items.is_empty() {
        return "Wardrobe: No items found matching the specified filters.".to_string();
    }
    let mut lines: Vec<String> = vec![format!(
        "Wardrobe ({} item{}):",
        output.total_count,
        if output.total_count != 1 { "s" } else { "" }
    )];
    for item in &output.items {
        let type_tags = item
            .types
            .iter()
            .map(|t| format!("[{t}]"))
            .collect::<Vec<_>>()
            .join(" ");
        // v4 `4a38222e6`: the folder tag sits between the title and the
        // equipped tag.
        let folder_tag = match item.folder.as_deref() {
            Some(f) if !f.is_empty() => format!(" [folder: {f}]"),
            _ => String::new(),
        };
        let equipped_tag = if item.is_equipped {
            format!(
                " (EQUIPPED in {})",
                item.equipped_slot.as_deref().unwrap_or("null")
            )
        } else {
            String::new()
        };
        let shared_tag = if item.is_own {
            ""
        } else {
            " [shared — read-only]"
        };
        let appropriateness_tag = match &item.appropriateness {
            Some(a) => format!(" | {a}"),
            None => String::new(),
        };
        let description = match &item.description {
            Some(d) => format!(" - {d}"),
            None => String::new(),
        };
        let cue_tag = match &item.image_prompt {
            Some(c) => format!(" (cue: {c})"),
            None => String::new(),
        };
        let composite_tag = if item.is_composite == Some(true) {
            let titles = item.component_titles.clone().unwrap_or_default().join(", ");
            let inner = if titles.is_empty() {
                "unresolved components".to_string()
            } else {
                titles
            };
            format!(" [composite: {inner}]")
        } else {
            String::new()
        };
        // v4 `3ee3b1342` / `b3f937076`: the wear note, then the picture handle.
        let wear_tag = format_wardrobe_list_wear_note(
            item.wear_count,
            item.worn_by_you,
            item.last_worn_by_you_at.as_deref(),
            now_ms,
        );
        let picture_tag = match item.image_file_id.as_deref() {
            Some(id) if !id.is_empty() => format!(
                " · picture: {}",
                crate::services::tool_image_generation::format_wardrobe_image_handle(id)
            ),
            _ => String::new(),
        };
        lines.push(format!(
            "  {} {}{}{}{}{}{}{}{}{}{}",
            type_tags,
            item.title,
            folder_tag,
            equipped_tag,
            shared_tag,
            appropriateness_tag,
            composite_tag,
            cue_tag,
            description,
            wear_tag,
            picture_tag
        ));
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::captured_with;
    use rusqlite::Connection;
    use serde_json::json;

    /// R-B: v4's WARN on a validation failure (`{context, userId,
    /// characterId, input}` — no `chatId`, v4's own key set), on the caller's
    /// thread; a refused input never reaches the run. P4.D280: v4's catch
    /// (`Wardrobe list tool execution failed`) is UNREACHABLE at `9ecebf986` —
    /// the pool fails soft per tier — so an unreadable instance answers an
    /// empty list and logs NO ERROR (pinned negative), only the completed INFO.
    #[test]
    fn the_validation_line_is_v4s_and_the_catch_is_unreachable() {
        let (main, mount) = (
            Connection::open_in_memory().unwrap(),
            Connection::open_in_memory().unwrap(),
        );
        let (out, lines) = captured_with(|| {
            execute(
                &main,
                &mount,
                "u1",
                "c1",
                "ch1",
                &json!({ "type_filter": "top" }),
            )
        });
        assert_eq!(out.error.as_deref(), Some(VALIDATION_ERROR));
        assert_eq!(
            lines,
            vec![r#"WARN quilltap::tools Wardrobe list tool validation failed context=wardrobe-list-handler userId=u1 characterId=ch1 input={"type_filter":"top"}"#.to_string()]
        );
        // `include_presets` is no longer declared: Zod strips it, any type.
        let (out, _) = captured_with(|| {
            execute(
                &main,
                &mount,
                "u1",
                "c1",
                "ch1",
                &json!({ "include_presets": 5 }),
            )
        });
        assert!(out.success, "{out:?}");
        // An empty partition: every tier fails soft, the list is empty.
        let (out, lines) = captured_with(|| execute(&main, &mount, "u1", "c1", "ch1", &json!({})));
        let lines: Vec<String> = lines
            .into_iter()
            .filter(|l| l.contains(" quilltap::tools "))
            .collect();
        assert!(out.success);
        assert!(out.items.is_empty());
        assert_eq!(lines.len(), 1, "{lines:#?}");
        assert!(
            lines[0].starts_with("INFO quilltap::tools Wardrobe list completed "),
            "{lines:#?}"
        );
        assert!(!lines.iter().any(|l| l.contains("execution failed")));
    }

    /// v4's `listTimes` (`once` / `N×`, never "twice") and the note's six steps.
    #[test]
    fn the_wear_note_leads_with_the_caller() {
        const NOW: f64 = 1_781_524_800_000.0; // 2026-06-15T12:00:00Z
        let three_days = "2026-06-12T12:00:00.000Z";
        assert_eq!(
            format_wardrobe_list_wear_note(0, 0, None, NOW),
            " · never worn"
        );
        assert_eq!(
            format_wardrobe_list_wear_note(115, 0, None, NOW),
            " · never worn by you (worn 115× by others)"
        );
        assert_eq!(
            format_wardrobe_list_wear_note(1, 1, Some(three_days), NOW),
            " · worn by you once, last 3 days ago"
        );
        assert_eq!(
            format_wardrobe_list_wear_note(4, 3, Some(three_days), NOW),
            " · worn by you 3×, last 3 days ago (4× in the household)"
        );
        assert_eq!(
            format_wardrobe_list_wear_note(4, 2, Some(""), NOW),
            " · worn by you 2× (4× in the household)"
        );
        assert_eq!(
            format_wardrobe_list_wear_note(2, 2, Some("2026-06-06T12:00:00.000Z"), NOW),
            " · worn by you 2×, last a week ago"
        );
    }
}
