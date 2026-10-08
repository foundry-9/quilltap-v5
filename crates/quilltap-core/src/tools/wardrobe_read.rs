//! `wardrobe_read` handler (v4 `lib/tools/handlers/wardrobe-read-handler.ts`
//! `executeWardrobeReadTool` + `formatWardrobeReadResults` + the shared
//! `buildWardrobeReadOutput` / `buildWardrobeReadFailure`, reused by
//! `wardrobe_update`).
//!
//! Resolves ONE wardrobe item across the (non-group) tiers and returns its full
//! detail — the Portrait Cue, default/replace flags, component list, archived
//! status, ownership, and equipped slots.

use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::db::chats_outfits::ChatOutfitsRepository;
use crate::db::doc_mount_documents::DocMountDocumentsRepository;
use crate::db::wardrobe_read::find_by_ids_for_character;
use crate::db::DbError;
use crate::wardrobe::normalize_no_item_sentinel;

use super::wardrobe_shared::{
    find_equipped_slots, is_own_wardrobe_item, resolve_wardrobe_item_across_tiers,
};
use crate::wardrobe_tiers::{resolve_shared_wardrobe_tiers_for_chat, SharedWardrobeTiers};

/// v4 `WardrobeReadToolOutput` — also the `wardrobe_update` output (a read-shaped
/// echo). `error` is omitted on success.
#[derive(Debug, Serialize)]
pub struct WardrobeReadToolOutput {
    pub success: bool,
    pub item_id: String,
    pub title: String,
    pub description: Option<String>,
    pub image_prompt: Option<String>,
    /// The item's current picture (v4 `b3f937076` — `item.imageFileId ??
    /// null`; always serialized).
    pub image_file_id: Option<String>,
    pub types: Vec<String>,
    pub appropriateness: Option<String>,
    pub is_default: bool,
    pub replace: bool,
    pub is_composite: bool,
    pub component_item_ids: Vec<String>,
    pub component_titles: Vec<String>,
    pub archived: bool,
    pub is_own: bool,
    pub is_equipped: bool,
    pub equipped_slots: Vec<String>,
    /// The item's wear history (v4 `3ee3b1342`) — absent on failure.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wear: Option<WardrobeReadWearResult>,
    /// `wardrobe_update`'s picture outcome (v4's `{ ...output,
    /// image_generation }`); the read itself never sets it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_generation: Option<crate::services::tool_image_generation::WardrobeToolImageResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// One wearer in [`WardrobeReadWearResult`] (v4 `WardrobeReadWearerResult`,
/// `wardrobe-read-tool.ts:77-89`), named for the CALLING character.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WardrobeReadWearerResult {
    /// `null` for the unattributed row (a wearer since folded away).
    pub character_id: Option<String>,
    pub name: String,
    /// The calling character themselves.
    pub is_you: bool,
    /// The ledger can no longer name them (a deleted character, or the
    /// unattributed row).
    pub departed: bool,
    pub wear_count: i64,
    pub first_worn_at: String,
    pub last_worn_at: String,
}

/// The item's wear history as `wardrobe_read` reports it (v4
/// `WardrobeReadWearResult`, `:94-99`; `3ee3b1342`) — totals across wearers,
/// the wearers most recent first.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WardrobeReadWearResult {
    pub wear_count: i64,
    pub first_worn_at: Option<String>,
    pub last_worn_at: Option<String>,
    pub wearers: Vec<WardrobeReadWearerResult>,
}

/// v4 `wearerPhrase` — how a wearer is named to the character reading the
/// tool output.
fn wearer_phrase(wearer: &WardrobeReadWearerResult) -> &str {
    if wearer.is_you {
        "you"
    } else if wearer.departed {
        "someone no longer in the household"
    } else {
        &wearer.name
    }
}

/// v4 `timesPhrase` — "once", "twice", "4 times".
fn times_phrase(count: i64) -> String {
    match count {
        1 => "once".to_string(),
        2 => "twice".to_string(),
        n => format!("{n} times"),
    }
}

/// en-GB short month names as Node 24's ICU renders them (September is
/// `Sept`, measured).
const EN_GB_MONTHS_SHORT: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sept", "Oct", "Nov", "Dec",
];

/// v4 `wearDate(iso)` — an absolute date as "14 Mar 2026":
/// `toLocaleDateString('en-GB', { day: 'numeric', month: 'short', year:
/// 'numeric', timeZone: 'UTC' })` (UTC, so a reader's locale cannot reshape
/// it); an unparseable stamp is returned as given.
fn wear_date(iso: &str) -> String {
    let Some(ms) = crate::episodic::js_date_parse_ms(iso) else {
        return iso.to_string();
    };
    let Ok(ts) = jiff::Timestamp::from_millisecond(ms) else {
        return iso.to_string();
    };
    let date = ts.to_zoned(jiff::tz::TimeZone::UTC).date();
    // ICU's `year: 'numeric'` drops the era: year 0 renders 1, -5 renders 6.
    let year = i32::from(date.year());
    let year = if year <= 0 { 1 - year } else { year };
    format!(
        "{} {} {}",
        date.day(),
        EN_GB_MONTHS_SHORT[usize::from(date.month() as u8) - 1],
        year
    )
}

/// v4 `relativeWearDate(iso, nowMs)` — [`format_relative_days`] or, for an
/// unparseable stamp, the stamp itself.
///
/// [`format_relative_days`]: crate::format_time::format_relative_days
fn relative_wear_date(iso: &str, now_ms: f64) -> String {
    match crate::episodic::js_date_parse_ms(iso) {
        Some(ms) => crate::format_time::format_relative_days(ms as f64, now_ms),
        None => iso.to_string(),
    }
}

/// v4 `joinPhrases` — `a`, `a and b`, `a, b and c`.
fn join_phrases(parts: &[String]) -> String {
    match parts {
        [] => String::new(),
        [one] => one.clone(),
        [init @ .., last] => format!("{} and {last}", init.join(", ")),
    }
}

/// v4 `formatWardrobeWearParagraph(wear, nowMs)` (`wardrobe-read-handler.ts:
/// 170-188`) — the `Wear` paragraph of `wardrobe_read`: "Worn 4 times, first
/// 14 Mar 2026, last 3 days ago by you. Also worn by Marguerite (once)." — or
/// "Never worn." `now_ms` is the caller's clock (v4's injectable `nowMs`).
pub fn format_wardrobe_wear_paragraph(
    wear: Option<&WardrobeReadWearResult>,
    now_ms: f64,
) -> String {
    let Some(wear) = wear else {
        return "Never worn.".to_string();
    };
    let last_worn_at = match wear.last_worn_at.as_deref() {
        Some(s) if !s.is_empty() => s,
        _ => return "Never worn.".to_string(),
    };
    let Some((latest, others)) = wear.wearers.split_first() else {
        return "Never worn.".to_string();
    };
    if wear.wear_count == 0 {
        return "Never worn.".to_string();
    }
    let last = format!(
        "{} by {}",
        relative_wear_date(last_worn_at, now_ms),
        wearer_phrase(latest)
    );
    let head = if wear.wear_count == 1 {
        format!("Worn once, {last}.")
    } else {
        // v4 `wear.first_worn_at ?? wear.last_worn_at` — `??` keeps an empty
        // string.
        let first = wear.first_worn_at.as_deref().unwrap_or(last_worn_at);
        format!(
            "Worn {} times, first {}, last {last}.",
            wear.wear_count,
            wear_date(first)
        )
    };
    if others.is_empty() {
        return head;
    }
    let also: Vec<String> = others
        .iter()
        .map(|w| format!("{} ({})", wearer_phrase(w), times_phrase(w.wear_count)))
        .collect();
    format!("{head} Also worn by {}.", join_phrases(&also))
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

/// v4 `buildWardrobeReadFailure` — the all-empty read output carrying an error.
pub fn build_read_failure(error: impl Into<String>) -> WardrobeReadToolOutput {
    WardrobeReadToolOutput {
        success: false,
        item_id: String::new(),
        title: String::new(),
        description: None,
        image_prompt: None,
        image_file_id: None,
        types: Vec::new(),
        appropriateness: None,
        is_default: false,
        replace: false,
        is_composite: false,
        component_item_ids: Vec::new(),
        component_titles: Vec::new(),
        archived: false,
        is_own: false,
        is_equipped: false,
        equipped_slots: Vec::new(),
        wear: None,
        image_generation: None,
        error: Some(error.into()),
    }
}

/// v4 `buildWardrobeReadOutput` — the full read-shaped output for a resolved item.
/// Shared by `wardrobe_read` and `wardrobe_update`.
pub fn build_read_output(
    main: &Connection,
    docs: &DocMountDocumentsRepository,
    character_id: &str,
    chat_id: &str,
    item: &Value,
    tiers: &SharedWardrobeTiers,
) -> Result<WardrobeReadToolOutput, DbError> {
    let component_item_ids = component_ids_of(item);
    let is_composite = !component_item_ids.is_empty();

    let component_titles: Vec<String> = if is_composite {
        let components =
            find_by_ids_for_character(main, docs, character_id, &component_item_ids, tiers)?;
        let title_by_id: std::collections::HashMap<String, String> = components
            .iter()
            .filter_map(|c| {
                let id = c.get("id").and_then(Value::as_str)?;
                let title = c.get("title").and_then(Value::as_str)?;
                Some((id.to_string(), title.to_string()))
            })
            .collect();
        component_item_ids
            .iter()
            .filter_map(|cid| title_by_id.get(cid).cloned())
            .collect()
    } else {
        Vec::new()
    };

    let equipped_slots_val =
        ChatOutfitsRepository::new(main).get_equipped_outfit_for_character(chat_id, character_id);
    let id = str_field(item, "id").unwrap_or_default();
    let equipped = find_equipped_slots(&id, equipped_slots_val.as_ref());

    let wear = build_wardrobe_read_wear(main, character_id, &id);

    Ok(WardrobeReadToolOutput {
        success: true,
        item_id: id,
        title: str_field(item, "title").unwrap_or_default(),
        description: str_field(item, "description"),
        image_prompt: str_field(item, "imagePrompt"),
        image_file_id: str_field(item, "imageFileId"),
        types: types_of(item),
        appropriateness: str_field(item, "appropriateness"),
        is_default: item
            .get("isDefault")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        replace: item
            .get("replace")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        is_composite,
        component_item_ids,
        component_titles,
        archived: matches!(item.get("archivedAt"), Some(v) if !v.is_null()),
        is_own: is_own_wardrobe_item(item, character_id),
        is_equipped: !equipped.is_empty(),
        equipped_slots: equipped,
        wear: Some(wear),
        image_generation: None,
        error: None,
    })
}

/// How a wearer resolved (v4 `WearerKind`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WearerKind {
    Character,
    Departed,
    Unattributed,
}

/// v4 `resolveWearers(wearers, repos)` without avatars (`lib/wardrobe/
/// wear-history.ts:85-131`) — each wearer's display name, index-aligned. The
/// null wearer is `unattributed`; a character that no longer exists (or has
/// no name) is `a departed character`. Names resolve RAW (`findByIdRaw` — a
/// broken vault costs a label, never the read); v4's read is a fallback
/// `safeQuery` that never throws, so its `Could not read wearer` WARN is
/// unreachable and not ported.
//
// HANDOFF(P4.D256): `services::wardrobe_wear_history::resolve_wearers` is
// P4.D256's (§R.10(b)); it was not on this lane's base. The unifier repoints
// this call at P4.D256's fn (with `avatars: false`) and deletes this copy.
fn resolve_wearers(
    main: &Connection,
    wearers: &[crate::db::wardrobe_wear_stats::WardrobeWearer],
) -> Vec<(String, WearerKind)> {
    wearers
        .iter()
        .map(|w| match w.character_id.as_deref() {
            None | Some("") => ("unattributed".to_string(), WearerKind::Unattributed),
            Some(id) => match crate::db::characters_read::find_by_id_raw_or_none(main, id)
                .and_then(|c| c.get("name").and_then(Value::as_str).map(str::to_string))
                .filter(|n| !n.is_empty())
            {
                Some(name) => (name, WearerKind::Character),
                None => ("a departed character".to_string(), WearerKind::Departed),
            },
        })
        .collect()
}

/// v4 `buildWardrobeReadWear(repos, characterId, itemId)` (`wardrobe-read-
/// handler.ts:97-127`) — the item's wear history, each wearer named for the
/// calling character: themselves flagged `is_you`, anyone the ledger can no
/// longer name flagged `departed`. v4's DEBUG `Wardrobe read resolved wear
/// history` (`context`, `characterId`, `itemId`, `wearCount`, `wearerCount`).
fn build_wardrobe_read_wear(
    main: &Connection,
    character_id: &str,
    item_id: &str,
) -> WardrobeReadWearResult {
    let history = crate::db::wardrobe_wear_stats::WardrobeWearStatsRepository::new(main)
        .find_history(item_id);
    let resolved = resolve_wearers(main, &history.wearers);
    tracing::debug!(
        context = "wardrobe-read-handler",
        characterId = character_id,
        itemId = item_id,
        wearCount = history.wear_count,
        wearerCount = history.wearers.len(),
        "Wardrobe read resolved wear history"
    );
    WardrobeReadWearResult {
        wear_count: history.wear_count,
        first_worn_at: history.first_worn_at,
        last_worn_at: history.last_worn_at,
        wearers: history
            .wearers
            .iter()
            .zip(resolved)
            .map(|(w, (name, kind))| WardrobeReadWearerResult {
                character_id: w.character_id.clone(),
                name,
                is_you: w.character_id.as_deref() == Some(character_id),
                departed: kind != WearerKind::Character,
                wear_count: w.wear_count,
                first_worn_at: w.first_worn_at.clone(),
                last_worn_at: w.last_worn_at.clone(),
            })
            .collect(),
    }
}

/// Validated `wardrobe_read` / `wardrobe_update`-style locate input (`item_id` /
/// `item_title`, at least one required). `None` ⇒ validation failure.
pub(crate) struct LocateInput {
    pub item_id: Option<String>,
    pub item_title: Option<String>,
}

/// v4 `wardrobeReadToolInputSchema` (also the locate half of update/archive): an
/// object with optional string `item_id`/`item_title`, refined so at least one is
/// present.
pub(crate) fn validate_locate(args: &Value) -> Option<LocateInput> {
    let obj = args.as_object()?;
    let item_id = match obj.get("item_id") {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) => Some(s.clone()),
        Some(_) => return None,
    };
    let item_title = match obj.get("item_title") {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) => Some(s.clone()),
        Some(_) => return None,
    };
    if item_id.is_none() && item_title.is_none() {
        return None; // the `.refine()` — at least one required.
    }
    Some(LocateInput {
        item_id,
        item_title,
    })
}

/// v4 the not-found message: `Wardrobe item not found[ with ID "<id>"][ with title
/// "<title>"]` — using the RAW input values (not the sentinel-normalized ones).
pub(crate) fn not_found_message(item_id: &Option<String>, item_title: &Option<String>) -> String {
    let mut msg = String::from("Wardrobe item not found");
    if let Some(id) = item_id {
        msg.push_str(&format!(" with ID \"{id}\""));
    }
    if let Some(title) = item_title {
        msg.push_str(&format!(" with title \"{title}\""));
    }
    msg
}

/// Execute `wardrobe_read` (v4 `executeWardrobeReadTool`).
pub fn execute(
    main: &Connection,
    mount: &Connection,
    user_id: &str,
    chat_id: &str,
    character_id: &str,
    args: &Value,
) -> WardrobeReadToolOutput {
    let _ = user_id;
    let Some(input) = validate_locate(args) else {
        return build_read_failure("Invalid input: item_id or item_title is required.");
    };
    match run(main, mount, chat_id, character_id, &input) {
        Ok(out) => out,
        Err(e) => build_read_failure(e.to_string()),
    }
}

fn run(
    main: &Connection,
    mount: &Connection,
    chat_id: &str,
    character_id: &str,
    input: &LocateInput,
) -> Result<WardrobeReadToolOutput, DbError> {
    let docs = DocMountDocumentsRepository::new(mount);
    let tiers = resolve_shared_wardrobe_tiers_for_chat(
        main,
        mount,
        chat_id,
        character_id,
        // A character tool call: the project roster applies (v4 `9753d0eb2`).
        Default::default(),
    );
    let item = resolve_wardrobe_item_across_tiers(
        main,
        &docs,
        character_id,
        normalize_no_item_sentinel(input.item_id.as_deref()).as_deref(),
        normalize_no_item_sentinel(input.item_title.as_deref()).as_deref(),
        &tiers,
    )?;
    let Some(item) = item else {
        return Ok(build_read_failure(not_found_message(
            &input.item_id,
            &input.item_title,
        )));
    };
    build_read_output(main, &docs, character_id, chat_id, &item, &tiers)
}

/// v4 `formatWardrobeReadResults(output, nowMs = Date.now())` on the
/// production clock — the wear paragraph's relative dates read now.
pub fn format(output: &WardrobeReadToolOutput) -> String {
    format_at(output, crate::clock::now_unix_ms() as f64)
}

/// [`format`] at a pinned clock (v4's second parameter) — the differentials
/// pass one fixed `nowMs` on both sides so the relative dates compare.
pub fn format_at(output: &WardrobeReadToolOutput, now_ms: f64) -> String {
    if !output.success {
        return format!(
            "Wardrobe Error: {}",
            output.error.as_deref().unwrap_or("Unknown error")
        );
    }
    let mut lines: Vec<String> = vec![format!("{} ({})", output.title, output.item_id)];
    lines.push(format!("  types: {}", output.types.join(", ")));
    if let Some(a) = &output.appropriateness {
        lines.push(format!("  appropriateness: {a}"));
    }
    if let Some(d) = &output.description {
        lines.push(format!("  description: {d}"));
    }
    lines.push(format!(
        "  portrait cue: {}",
        output
            .image_prompt
            .as_deref()
            .unwrap_or("(none — falls back to title)")
    ));
    // v4 `b3f937076`: the item's picture, right after the Portrait Cue.
    lines.push(format!(
        "  picture: {}",
        match output.image_file_id.as_deref() {
            Some(id) if !id.is_empty() =>
                crate::services::tool_image_generation::format_wardrobe_image_handle(id),
            _ => "(none)".to_string(),
        }
    ));
    if output.is_composite {
        let titles = output.component_titles.join(", ");
        let inner = if titles.is_empty() {
            "unresolved components".to_string()
        } else {
            titles
        };
        lines.push(format!(
            "  composite: {} (replace={})",
            inner, output.replace
        ));
    }
    lines.push(format!(
        "  default: {} | own: {}",
        if output.is_default { "yes" } else { "no" },
        if output.is_own {
            "yes"
        } else {
            "no (shared — read-only)"
        }
    ));
    if output.archived {
        lines.push("  archived: yes (hidden from listings, cannot be worn)".to_string());
    }
    lines.push(format!(
        "  equipped: {}",
        if output.is_equipped {
            output.equipped_slots.join(", ")
        } else {
            "no".to_string()
        }
    ));
    // v4 `3ee3b1342`: the wear paragraph LAST, only when the output carries it.
    if let Some(wear) = &output.wear {
        lines.push(format!(
            "  wear: {}",
            format_wardrobe_wear_paragraph(Some(wear), now_ms)
        ));
    }
    lines.join("\n")
}
