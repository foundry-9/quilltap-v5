//! v4 `lib/services/character-rename.service.ts` — Aurora's "Rename/Replace"
//! tab (`p4.9k`, P4.9K1): a bulk search-and-replace over a character's own
//! data — the managed character fields, the physical description and its
//! prompt variants, the character's memories, and the titles and message
//! bodies of every chat the character appears in. Two modes off one code path:
//! `dryRun: true` scans and reports; `dryRun: false` scans, then commits.
//!
//! ## The matcher is a LITERAL SCAN, not a regex — a ruling, not a shortcut
//!
//! v4 builds `new RegExp(escapeRegex(pair.oldValue), caseSensitive ? 'g' : 'gi')`
//! and calls `text.match` / `text.replace`. Three facts were MEASURED on Node
//! 24.13.1 at the `f699da6f6` pin before this port was written (the P4.9K1 lane
//! record, "Measured for unit 3"):
//!
//! 1. **`escapeRegex` is COMPLETE for literal matching** — every JS
//!    metacharacter outside a character class is in its set, so the pattern
//!    only ever matches the literal `oldValue`, left-to-right, NON-overlapping
//!    (`"aaaa"` + `aa` → 2 matches).
//! 2. **Case-insensitive matching is ECMAScript `Canonicalize`** (the `i` flag
//!    WITHOUT `u`): each UTF-16 code unit maps through `toUpperCase`, but ONLY
//!    when that yields a single code unit, and NEVER when a non-ASCII unit would
//!    fold onto an ASCII one — `"Strasse Straſse"` + `strasse`/`gi` → **1**
//!    match, and `"STRAẞSE"` + `straßse`/`gi` → **0**. A Rust `(?i)` regex
//!    folds both and would diverge on every such row.
//! 3. **`String.prototype.replace` EXPANDS the replacement** through
//!    `GetSubstitution`: `$$` → `$`, `$&` → the match, `` $` `` → the text
//!    before it, `$'` → the text after it — while `$1` / `${x}` / `$<n>` stay
//!    LITERAL because the pattern has no capture groups. Rust's
//!    `Regex::replace_all` would DELETE `$1` and cannot spell the other three.
//!
//! So the port scans UTF-16 code units with `Canonicalize` and applies
//! `GetSubstitution` by hand. Both divergences a regex port would have had to
//! record simply vanish. [`replace_literal`] is the whole matcher.
//!
//! ## Writes
//!
//! The commit step runs every write on the ONE writer connection pair the
//! caller holds (the store-delete precedent — one transaction), where v4 issues
//! them as separate repository calls. The end state is identical; the
//! transaction shape is a recorded divergence (the P4.D77 class). Character
//! fields route through [`crate::db::vault_character_update::update_character`]
//! exactly as v4's `repos.characters.update` routes them (only `name` lands on
//! the row; the rest project into the vault); memories update in place; chat
//! titles and message bodies update in place; every chat whose MESSAGES changed
//! is re-rendered + re-embedded through the render queue (best-effort, warn on
//! failure — v4's own posture).

use rusqlite::Connection;
use serde::Serialize;
use serde_json::{json, Map, Value};

use crate::clock::now_iso;
use crate::db::chats::{ChatUpdate, ChatsRepository};
use crate::db::chats_messages::ChatMessagesRepository;
use crate::db::memories::{MemUpdate, MemoriesRepository};
use crate::db::vault_character_update::update_character;
use crate::db::{chats_messages_read, chats_read, memories_read, DbError};
use crate::pascal::js_value::to_js_string;
use crate::services::queue_service::enqueue_conversation_render_blocking;

// ============================================================================
// Types (v4 `character-rename.service.ts:35-73`)
// ============================================================================

/// v4 `ReplacementPair`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplacementPair {
    pub old_value: String,
    pub new_value: String,
    pub case_sensitive: bool,
}

/// v4 `RenameRequest`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenameRequest {
    /// Primary character-name replacement (oldValue is the current name).
    pub primary_rename: Option<ReplacementPair>,
    /// Additional replacements for nicknames, aliases, or arbitrary terms.
    pub additional_replacements: Vec<ReplacementPair>,
    /// When true, scan and report without committing any writes.
    pub dry_run: bool,
}

/// v4 `ReplacementResult` — one preview row, in v4's key order.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplacementResult {
    pub field: String,
    pub location: String,
    pub old_text: String,
    pub new_text: String,
    /// `context?: string` — every row `rewriteField` pushes carries one.
    pub context: String,
}

/// v4 `RenameSummary`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameSummary {
    pub character_fields: i64,
    pub physical_descriptions: i64,
    pub memories: i64,
    pub chat_titles: i64,
    pub chat_messages: i64,
    pub total: i64,
}

/// v4 `RenamePreviewResponse` — the shape the Rename/Replace tab renders,
/// preserved verbatim (v4's own history note).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RenamePreviewResponse {
    pub character_id: String,
    pub character_name: String,
    pub dry_run: bool,
    pub replacements: Vec<ReplacementResult>,
    pub summary: RenameSummary,
}

// ============================================================================
// The matcher (v4 `escapeRegex` + `performReplacement`, as a literal scan)
// ============================================================================

/// ECMAScript `Canonicalize(ch)` for a non-`u` pattern with `ignoreCase`
/// (ES2024 §22.2.2.7.3): map the code unit through `toUpperCase`; keep the
/// original when the result is not exactly ONE code unit, and keep it when a
/// non-ASCII unit would fold onto an ASCII one. Surrogate halves are units
/// with no case mapping, so they canonicalize to themselves.
fn canonicalize_unit(u: u16) -> u16 {
    if (0xD800..=0xDFFF).contains(&u) {
        return u;
    }
    let Some(ch) = char::from_u32(u32::from(u)) else {
        return u;
    };
    let mut upper = ch.to_uppercase();
    let Some(first) = upper.next() else {
        return u;
    };
    if upper.next().is_some() {
        // A multi-character upper-case mapping (`ß` → `SS`): "If cu does not
        // consist of a single code unit, return ch."
        return u;
    }
    let mut buf = [0u16; 2];
    let enc = first.encode_utf16(&mut buf);
    if enc.len() != 1 {
        return u;
    }
    let cu = enc[0];
    // "If ch's code unit value ≥ 128 and cu's code unit value < 128, return ch."
    if u >= 128 && cu < 128 {
        return u;
    }
    cu
}

/// The non-overlapping, left-to-right match starts of `needle` in `text` — v4's
/// `text.match(regex).length` over an escaped literal with the `g` flag, in
/// UTF-16 code units. An empty needle never reaches here (the route's Zod
/// `min(1)`), but is answered with no matches rather than an infinite scan.
fn find_literal_matches(text: &[u16], needle: &[u16], case_sensitive: bool) -> Vec<usize> {
    let mut out = Vec::new();
    if needle.is_empty() || needle.len() > text.len() {
        return out;
    }
    let canon = |u: u16| {
        if case_sensitive {
            u
        } else {
            canonicalize_unit(u)
        }
    };
    let needle_c: Vec<u16> = needle.iter().map(|&u| canon(u)).collect();
    let text_c: Vec<u16> = text.iter().map(|&u| canon(u)).collect();
    let mut i = 0usize;
    let last = text_c.len() - needle_c.len();
    while i <= last {
        if text_c[i..i + needle_c.len()] == needle_c[..] {
            out.push(i);
            i += needle_c.len();
        } else {
            i += 1;
        }
    }
    out
}

/// ES `GetSubstitution` for a pattern with NO capture groups and NO named
/// groups: `$$`, `$&`, `` $` `` and `$'` expand; every other `$` sequence is
/// literal (a `$n` with `n > m` is "implementation-defined", and Node 24 keeps
/// it literal — measured; `$<` with no named captures is literal by spec).
fn expand_replacement(
    replacement: &[u16],
    text: &[u16],
    start: usize,
    end: usize,
    out: &mut Vec<u16>,
) {
    let mut i = 0usize;
    while i < replacement.len() {
        let u = replacement[i];
        if u == u16::from(b'$') && i + 1 < replacement.len() {
            match replacement[i + 1] {
                0x24 => {
                    out.push(0x24);
                    i += 2;
                    continue;
                }
                0x26 => {
                    out.extend_from_slice(&text[start..end]);
                    i += 2;
                    continue;
                }
                0x60 => {
                    out.extend_from_slice(&text[..start]);
                    i += 2;
                    continue;
                }
                0x27 => {
                    out.extend_from_slice(&text[end..]);
                    i += 2;
                    continue;
                }
                _ => {}
            }
        }
        out.push(u);
        i += 1;
    }
}

/// v4 `performReplacement` for a non-empty `text`: the rewritten text, whether
/// anything changed, and how many occurrences were swapped. Public because the
/// K1 lane record's Node measurements are pinned on it directly.
pub fn replace_literal(text: &str, pair: &ReplacementPair) -> (String, bool, usize) {
    let units: Vec<u16> = text.encode_utf16().collect();
    let needle: Vec<u16> = pair.old_value.encode_utf16().collect();
    let starts = find_literal_matches(&units, &needle, pair.case_sensitive);
    if starts.is_empty() {
        return (text.to_string(), false, 0);
    }
    let replacement: Vec<u16> = pair.new_value.encode_utf16().collect();
    let mut out: Vec<u16> = Vec::with_capacity(units.len());
    let mut cursor = 0usize;
    for &s in &starts {
        out.extend_from_slice(&units[cursor..s]);
        expand_replacement(&replacement, &units, s, s + needle.len(), &mut out);
        cursor = s + needle.len();
    }
    out.extend_from_slice(&units[cursor..]);
    // A Rust `String` holds no lone surrogates and a match can only start on a
    // unit the needle starts on, so the pairs survive intact; `lossy` is the
    // signature, not a behaviour.
    (String::from_utf16_lossy(&out), true, starts.len())
}

/// v4 `performReplacement` — `!text` (a null / absent / EMPTY value) answers
/// `{result: null, changed: false, matches: 0}` before any scan.
fn perform_replacement(
    text: Option<&str>,
    pair: &ReplacementPair,
) -> (Option<String>, bool, usize) {
    match text {
        None | Some("") => (None, false, 0),
        Some(t) => {
            let (r, changed, n) = replace_literal(t, pair);
            (Some(r), changed, n)
        }
    }
}

/// v4 `getContext` — a short snippet of surrounding text for the preview table.
/// `indexOf` runs over the LOWERCASED strings (JS `toLowerCase`, which this
/// crate's `str::to_lowercase` reproduces byte-for-byte), and the resulting
/// UTF-16 index slices the ORIGINAL text — exactly v4's mismatch when a
/// lowercase mapping changes the unit count.
fn get_context(text: &str, search_term: &str) -> String {
    const MAX_LENGTH: usize = 100;
    let units: Vec<u16> = text.encode_utf16().collect();
    let lower_text = text.to_lowercase();
    let lower_term = search_term.to_lowercase();
    let Some(index) = crate::jsstr::js_index_of(&lower_text, &lower_term, 0) else {
        return String::from_utf16_lossy(&units[..units.len().min(MAX_LENGTH)]);
    };
    let term_len = search_term.encode_utf16().count();
    let start = index.saturating_sub(30);
    let end = (index + term_len + 30).min(units.len());
    // `text.slice(start, end)`: an index past the end reads as the end, and an
    // inverted window reads as empty (both JS `slice` rules).
    let start = start.min(units.len());
    let end = end.max(start);
    let mut context = String::from_utf16_lossy(&units[start..end]);
    if start > 0 {
        context = format!("...{context}");
    }
    if end < units.len() {
        context.push_str("...");
    }
    context
}

/// v4 `rewriteField` — apply every pair to one string field in sequence,
/// recording a preview row per matched pair. A non-string value (`null`,
/// absent, a number) is returned untouched with zero matches.
fn rewrite_field(
    value: Option<&Value>,
    replacements: &[ReplacementPair],
    field: &str,
    location: &str,
    out: &mut Vec<ReplacementResult>,
) -> (Option<String>, usize) {
    let Some(Value::String(s)) = value else {
        return (None, 0);
    };
    let mut current = s.clone();
    let mut total = 0usize;
    for pair in replacements {
        let (result, changed, matches) = perform_replacement(Some(&current), pair);
        if changed {
            if let Some(result) = result {
                out.push(ReplacementResult {
                    field: field.to_string(),
                    location: location.to_string(),
                    old_text: pair.old_value.clone(),
                    new_text: pair.new_value.clone(),
                    context: get_context(&current, &pair.old_value),
                });
                current = result;
                total += matches;
            }
        }
    }
    (Some(current), total)
}

// ============================================================================
// The runner (v4 `runCharacterRename`)
// ============================================================================

fn s<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.get(key).and_then(Value::as_str)
}

/// JS `${x}` for a field that may be absent — an absent key renders
/// `undefined`, an explicit `null` renders `null` (the location labels are
/// template literals over fields v4 does not null-guard).
fn interp(v: Option<&Value>) -> String {
    match v {
        None => "undefined".to_string(),
        Some(x) => to_js_string(x),
    }
}

/// One rewritten sub-object (a scenario / system prompt / the physical
/// description): `{...item}` with the touched fields overwritten and, when
/// anything was touched, a fresh `updatedAt`.
#[allow(clippy::too_many_arguments)]
fn rewrite_object(
    item: &Value,
    fields: &[&str],
    prefix: &str,
    location: &str,
    replacements: &[ReplacementPair],
    out: &mut Vec<ReplacementResult>,
    counter: &mut i64,
    now: &str,
) -> (Value, bool) {
    let mut next = item.as_object().cloned().unwrap_or_default();
    let mut touched = false;
    for field in fields {
        let (value, matches) = rewrite_field(
            item.get(*field),
            replacements,
            &format!("{prefix}.{field}"),
            location,
            out,
        );
        if matches > 0 {
            if let Some(v) = value {
                next.insert((*field).to_string(), Value::String(v));
            }
            *counter += matches as i64;
            touched = true;
        }
    }
    if touched {
        next.insert("updatedAt".to_string(), Value::String(now.to_string()));
    }
    (Value::Object(next), touched)
}

/// Run a character rename / bulk replace. Always scans; only writes when
/// `request.dry_run` is false. `character` is the overlay-resolved character
/// (vault fields rehydrated), exactly what v4's route hands the service.
///
/// Runs on the caller's writer pair (`main` + `mount`) so the execute leg's
/// writes and the scan's reads share ONE transaction (module doc).
pub fn run_character_rename(
    main: &Connection,
    mount: &Connection,
    character: &Value,
    request: &RenameRequest,
    user_id: &str,
) -> Result<RenamePreviewResponse, DbError> {
    let start_ms = crate::clock::now_unix_ms();
    let character_id = s(character, "id").unwrap_or("").to_string();
    let character_name = s(character, "name").unwrap_or("").to_string();

    // Order matters: the primary rename runs before aliases/nicknames.
    let mut all_replacements: Vec<ReplacementPair> = Vec::new();
    if let Some(p) = &request.primary_rename {
        all_replacements.push(p.clone());
    }
    all_replacements.extend(request.additional_replacements.iter().cloned());
    let replacements_in = &all_replacements;

    let dry_run = request.dry_run;
    let mut replacements: Vec<ReplacementResult> = Vec::new();
    let mut summary = RenameSummary::default();
    let mut character_updates: Map<String, Value> = Map::new();
    let location = format!("Character: {}", interp(character.get("name")));
    let now = now_iso();

    // ── 1. Scalar character fields ──────────────────────────────────────────
    // `name` lands on the DB row; the rest are vault-managed (the repo routes
    // them there on update).
    for field in [
        "name",
        "title",
        "identity",
        "description",
        "manifesto",
        "personality",
        "firstMessage",
        "exampleDialogues",
    ] {
        let (value, matches) = rewrite_field(
            character.get(field),
            replacements_in,
            field,
            &location,
            &mut replacements,
        );
        if matches > 0 {
            if let Some(v) = value {
                character_updates.insert(field.to_string(), Value::String(v));
            }
            summary.character_fields += matches as i64;
        }
    }

    // ── 2. Aliases (array of name-like strings) ─────────────────────────────
    if let Some(aliases) = character.get("aliases").and_then(Value::as_array) {
        if !aliases.is_empty() {
            let mut changed = false;
            let updated: Vec<Value> = aliases
                .iter()
                .map(|alias| {
                    let (value, matches) = rewrite_field(
                        Some(alias),
                        replacements_in,
                        "alias",
                        &location,
                        &mut replacements,
                    );
                    if matches > 0 {
                        changed = true;
                        summary.character_fields += matches as i64;
                    }
                    match value {
                        Some(v) => Value::String(v),
                        None => alias.clone(),
                    }
                })
                .collect();
            if changed {
                character_updates.insert("aliases".to_string(), Value::Array(updated));
            }
        }
    }

    // ── 3. Scenarios (title / content / description) ────────────────────────
    // v4 `character.scenarios ?? []` — nullish, and a non-array is left to
    // `.length`; no overlay read produces one.
    if let Some(scenarios) = character.get("scenarios").and_then(Value::as_array) {
        if !scenarios.is_empty() {
            let mut changed = false;
            let updated: Vec<Value> = scenarios
                .iter()
                .map(|scenario| {
                    let sc_location = format!("Scenario: {}", interp(scenario.get("title")));
                    let (next, touched) = rewrite_object(
                        scenario,
                        &["title", "content", "description"],
                        "scenario",
                        &sc_location,
                        replacements_in,
                        &mut replacements,
                        &mut summary.character_fields,
                        &now,
                    );
                    changed |= touched;
                    next
                })
                .collect();
            if changed {
                character_updates.insert("scenarios".to_string(), Value::Array(updated));
            }
        }
    }

    // ── 4. System prompts (name / content) ──────────────────────────────────
    if let Some(prompts) = character.get("systemPrompts").and_then(Value::as_array) {
        if !prompts.is_empty() {
            let mut changed = false;
            let updated: Vec<Value> = prompts
                .iter()
                .map(|prompt| {
                    let sp_location = format!("System Prompt: {}", interp(prompt.get("name")));
                    let (next, touched) = rewrite_object(
                        prompt,
                        &["name", "content"],
                        "systemPrompt",
                        &sp_location,
                        replacements_in,
                        &mut replacements,
                        &mut summary.character_fields,
                        &now,
                    );
                    changed |= touched;
                    next
                })
                .collect();
            if changed {
                character_updates.insert("systemPrompts".to_string(), Value::Array(updated));
            }
        }
    }

    // ── 5. Physical description (single object + prompt variants) ───────────
    // v4 `if (physical)` — JS truthiness over the object (null / absent skip).
    if let Some(physical) = character
        .get("physicalDescription")
        .filter(|p| p.is_object())
    {
        let pd_location = format!("Description: {}", interp(physical.get("name")));
        let (next, touched) = rewrite_object(
            physical,
            &[
                "name",
                "usageContext",
                "headAndShouldersPrompt",
                "shortPrompt",
                "mediumPrompt",
                "longPrompt",
                "completePrompt",
                "fullDescription",
            ],
            "physicalDescription",
            &pd_location,
            replacements_in,
            &mut replacements,
            &mut summary.physical_descriptions,
            &now,
        );
        if touched {
            character_updates.insert("physicalDescription".to_string(), next);
        }
    }

    // ── 6. Memories (content / summary / keywords) ──────────────────────────
    let memories = memories_read::find_by_character_id(main, &character_id)?;
    struct MemoryUpdate {
        id: String,
        content: Option<String>,
        summary: Option<String>,
        keywords: Option<Vec<String>>,
    }
    let mut memory_updates: Vec<MemoryUpdate> = Vec::new();
    for memory in &memories {
        let id = s(memory, "id").unwrap_or("").to_string();
        // v4 `memory.id.slice(0, 8)` — UTF-16 units; ids are ASCII uuids.
        let mem_location = format!("Memory: {}...", crate::jsstr::utf16_truncate(&id, 8));
        let mut update = MemoryUpdate {
            id: id.clone(),
            content: None,
            summary: None,
            keywords: None,
        };

        let (content, content_matches) = rewrite_field(
            memory.get("content"),
            replacements_in,
            "memory.content",
            &mem_location,
            &mut replacements,
        );
        if content_matches > 0 {
            update.content = content;
            summary.memories += content_matches as i64;
        }

        let (summary_v, summary_matches) = rewrite_field(
            memory.get("summary"),
            replacements_in,
            "memory.summary",
            &mem_location,
            &mut replacements,
        );
        if summary_matches > 0 {
            update.summary = summary_v;
            summary.memories += summary_matches as i64;
        }

        // Keywords: replaced silently — no preview row, no count (v4 loops
        // `performReplacement` directly, not `rewriteField`).
        if let Some(keywords) = memory.get("keywords").and_then(Value::as_array) {
            if !keywords.is_empty() {
                let mut keywords_changed = false;
                let updated: Vec<String> = keywords
                    .iter()
                    .map(|keyword| {
                        let mut current = to_js_string(keyword);
                        for pair in replacements_in {
                            let (result, changed, _) = perform_replacement(Some(&current), pair);
                            if changed {
                                if let Some(r) = result {
                                    current = r;
                                    keywords_changed = true;
                                }
                            }
                        }
                        current
                    })
                    .collect();
                if keywords_changed {
                    update.keywords = Some(updated);
                }
            }
        }

        if update.content.is_some() || update.summary.is_some() || update.keywords.is_some() {
            memory_updates.push(update);
        }
    }

    // ── 7. Chats (title + message bodies) ───────────────────────────────────
    let chats = chats_read::find_by_character_id(main, &character_id)?;
    struct ChatUpdatePlan {
        chat_id: String,
        title_update: Option<String>,
        message_updates: Vec<(String, String)>,
    }
    let mut chat_updates: Vec<ChatUpdatePlan> = Vec::new();
    for chat in &chats {
        let chat_id = s(chat, "id").unwrap_or("").to_string();
        let chat_location = format!("Chat: {}", interp(chat.get("title")));
        let (title, title_matches) = rewrite_field(
            chat.get("title"),
            replacements_in,
            "chat.title",
            &chat_location,
            &mut replacements,
        );
        let title_update = if title_matches > 0 { title } else { None };
        if title_matches > 0 {
            summary.chat_titles += title_matches as i64;
        }

        let mut message_updates: Vec<(String, String)> = Vec::new();
        let messages = chats_messages_read::get_messages(main, &chat_id)?;
        for message in &messages {
            // Only genuine user/character prose. Skip non-message events and
            // Staff / personified-feature messages (systemSender != null) —
            // those carry structured payloads and opaque rewrites that a
            // blind replace would corrupt. v4's test is JS truthiness on
            // `systemSender`, so an empty-string sender does NOT skip.
            let is_message = s(message, "type") == Some("message");
            let staff = message
                .get("systemSender")
                .is_some_and(|v| crate::api::system_qtap::js_truthy(Some(v)));
            if !is_message || staff {
                continue;
            }
            let (content, matches) = rewrite_field(
                message.get("content"),
                replacements_in,
                "chat.message",
                &chat_location,
                &mut replacements,
            );
            if matches > 0 {
                if let Some(c) = content {
                    message_updates.push((s(message, "id").unwrap_or("").to_string(), c));
                }
                summary.chat_messages += matches as i64;
            }
        }

        // v4 `if (titleUpdate || messageUpdates.length > 0)` — the title test is
        // JS truthiness, so a title rewritten to the empty string does not count.
        let title_truthy = title_update.as_deref().is_some_and(|t| !t.is_empty());
        if title_truthy || !message_updates.is_empty() {
            chat_updates.push(ChatUpdatePlan {
                chat_id,
                title_update,
                message_updates,
            });
        }
    }

    summary.total = summary.character_fields
        + summary.physical_descriptions
        + summary.memories
        + summary.chat_titles
        + summary.chat_messages;

    // ── 8. Commit (execute mode only) ───────────────────────────────────────
    if !dry_run && summary.total > 0 {
        let field_names: Vec<&String> = character_updates.keys().collect();
        tracing::info!(
            character_id = %character_id,
            character_name = %character_name,
            total_changes = summary.total,
            character_fields_changed = ?field_names,
            memories_changed = memory_updates.len(),
            chats_changed = chat_updates.len(),
            "[CharacterRename] Executing"
        );

        if !character_updates.is_empty() {
            update_character(main, mount, &character_id, &character_updates)
                .map_err(|e| e.into_db())?;
        }

        let memories_repo = MemoriesRepository::new(main);
        for mu in &memory_updates {
            let patch = MemUpdate {
                content: mu.content.clone(),
                summary: mu.summary.clone(),
                keywords: mu.keywords.clone(),
                ..Default::default()
            };
            memories_repo.update(&mu.id, &patch)?;
        }

        let chats_repo = ChatsRepository::new(main);
        let messages_repo = ChatMessagesRepository::new(main);
        for cu in &chat_updates {
            if let Some(title) = cu.title_update.as_deref().filter(|t| !t.is_empty()) {
                chats_repo.update(
                    &cu.chat_id,
                    &ChatUpdate {
                        title: Some(title.to_string()),
                        ..Default::default()
                    },
                )?;
            }
            for (message_id, content) in &cu.message_updates {
                messages_repo.update_message(
                    &cu.chat_id,
                    message_id,
                    &json!({ "content": content }),
                )?;
            }

            // Re-render + re-embed any chat whose message bodies changed so the
            // searchable conversation archive reflects the new text
            // (best-effort; a pending render job for the chat is reused rather
            // than duplicated).
            if !cu.message_updates.is_empty() {
                if let Err(err) =
                    enqueue_conversation_render_blocking(main, user_id, &cu.chat_id, Some(true))
                {
                    tracing::warn!(
                        character_id = %character_id,
                        chat_id = %cu.chat_id,
                        error = %err,
                        "[CharacterRename] Failed to enqueue archive re-render"
                    );
                }
            }
        }

        tracing::info!(
            character_id = %character_id,
            duration_ms = crate::clock::now_unix_ms() - start_ms,
            summary = ?serde_json::to_value(&summary).unwrap_or(serde_json::Value::Null),
            "[CharacterRename] Completed"
        );
    }

    Ok(RenamePreviewResponse {
        character_id,
        character_name,
        dry_run,
        replacements,
        summary,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pair(old: &str, new: &str, cs: bool) -> ReplacementPair {
        ReplacementPair {
            old_value: old.into(),
            new_value: new.into(),
            case_sensitive: cs,
        }
    }

    /// The K1 lane record's Node 24.13.1 measurements at the `f699da6f6` pin
    /// (v4's `new RegExp(escapeRegex(old), flags)` + `String.prototype.replace`).
    /// v4 exports only the runner, so these pin the matcher directly.
    #[test]
    fn get_substitution_expands_only_the_four_forms() {
        let t = "Hello Bob.";
        assert_eq!(
            replace_literal(t, &pair("Bob", "$&$&", false)).0,
            "Hello BobBob."
        );
        assert_eq!(
            replace_literal(t, &pair("Bob", "a$$b", false)).0,
            "Hello a$b."
        );
        assert_eq!(
            replace_literal("Hello Bob there.", &pair("Bob", "[$`]", false)).0,
            "Hello [Hello ] there."
        );
        assert_eq!(
            replace_literal("Hello Bob there.", &pair("Bob", "[$']", false)).0,
            "Hello [ there.] there."
        );
        // No capture groups: `$1` and `${x}` are LITERAL (a regex port deletes them).
        assert_eq!(replace_literal(t, &pair("Bob", "$1", false)).0, "Hello $1.");
        assert_eq!(
            replace_literal(t, &pair("Bob", "${x}", false)).0,
            "Hello ${x}."
        );
        assert_eq!(
            replace_literal(t, &pair("Bob", "$<n>", false)).0,
            "Hello $<n>."
        );
        // A trailing lone `$` is literal too.
        assert_eq!(replace_literal(t, &pair("Bob", "x$", false)).0, "Hello x$.");
    }

    #[test]
    fn canonicalize_never_folds_non_ascii_onto_ascii() {
        // `"Strasse Straſse"` + `strasse`/`gi` → 1 (the long s stays itself).
        assert_eq!(
            replace_literal("Strasse Straſse", &pair("strasse", "X", false)).2,
            1
        );
        // `"STRAẞSE"` + `straßse`/`gi` → 0 (ß upper-cases to two units → kept).
        assert_eq!(
            replace_literal("STRAẞSE", &pair("straßse", "X", false)).2,
            0
        );
        // Dotless i upper-cases to ASCII `I` and is therefore NOT folded.
        assert_eq!(replace_literal("ısland", &pair("island", "X", false)).2, 0);
        // Plain ASCII folding works both ways.
        assert_eq!(
            replace_literal("voss and Voss", &pair("Voss", "M", false)).2,
            2
        );
        assert_eq!(
            replace_literal("voss and Voss", &pair("Voss", "M", true)).2,
            1
        );
        // Greek folds within the non-ASCII plane.
        assert_eq!(
            replace_literal("σοφία ΣΟΦΊΑ", &pair("Σοφία", "X", false)).2,
            2
        );
    }

    #[test]
    fn matches_are_non_overlapping_and_escape_is_complete() {
        assert_eq!(
            replace_literal("aaaa", &pair("aa", "b", true)),
            ("bb".into(), true, 2)
        );
        assert_eq!(replace_literal("x-y", &pair("x-y", "z", true)).2, 1);
        assert_eq!(
            replace_literal("a.c abc", &pair("a.c", "z", true)),
            ("z abc".into(), true, 1)
        );
        assert_eq!(replace_literal("(a)", &pair("(a)", "z", true)).2, 1);
    }

    #[test]
    fn context_slices_the_original_by_the_lowercased_index() {
        assert_eq!(get_context("Hello Bob.", "bob"), "Hello Bob.");
        let long = "x".repeat(50) + "Bob" + &"y".repeat(50);
        let ctx = get_context(&long, "Bob");
        assert!(ctx.starts_with("...") && ctx.ends_with("..."));
        assert_eq!(ctx.len(), 3 + 30 + 3 + 30 + 3);
        // No hit → the first 100 units.
        assert_eq!(get_context(&"z".repeat(150), "Bob").len(), 100);
    }
}
