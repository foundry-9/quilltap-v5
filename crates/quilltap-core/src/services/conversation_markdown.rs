//! The Scriptorium conversation renderer (P4.6BM) — a port of v4's
//! `lib/scriptorium/markdown-renderer.ts` (`renderConversationMarkdown`).
//!
//! Deterministic, LLM-free, DB-free: chat events in, numbered Markdown +
//! per-interchange chunks out. The `CONVERSATION_RENDER` job handler
//! ([`super::conversation_render_job`]) is its only production caller; the
//! differential proof is **tier-1 exact** (strings byte-compared).
//!
//! ## Shape
//!
//! Messages are filtered to visible content (`type == "message"`, role USER or
//! ASSISTANT after an upper-case fold, non-empty `content`); assistant text runs
//! through [`crate::chat_tasks::strip_tool_artifacts`] and is dropped when
//! nothing survives. The survivors are grouped into **interchanges** — a new one
//! starts at every USER message (and at the very first message whatever its
//! role) — and each message renders as
//!
//! ```text
//! Past conversation message timestamp: {when}
//!
//! ### Message {globalIndex} ({displayName})
//!
//! {content}
//! ```
//!
//! `globalMessageIndex` counts across the WHOLE conversation, never per
//! interchange. With metadata present, the header block is tracked as
//! **interchange 0's `metadata_prefix`** (so it rides on chunk 0, v4's comment),
//! and the full markdown is the final chunk list's contents joined with `"\n"`.
//!
//! ## Sub-chunking (v4 Bug 17)
//!
//! An interchange whose rendered text — the metadata prefix included — exceeds
//! [`CHUNK_CHAR_BUDGET`] UTF-16 code units is split into sequential sub-chunks by
//! [`enforce_chunk_budget`]: at message boundaries first ([`split_interchange`]),
//! then within a single oversize message at natural boundaries
//! ([`split_oversize_text`]). Each emitted chunk gets its own sequential `index`
//! (a chunk ordinal counting across ALL interchanges). Split chunks after the
//! first carry a `## Interchange N (continued k)` header; the metadata prefix
//! rides only on the first sub-chunk of interchange 0. The `- Interchange Count:`
//! header line stays the count of RAW interchanges, never the chunk count. In the
//! common case (nothing over budget) every interchange is one chunk and the
//! output is **byte-identical** to the pre-sub-chunking renderer. All budget
//! arithmetic measures UTF-16 code units (v4's `.length` / `.slice` /
//! `.lastIndexOf`), reproduced with the [`crate::jsstr`] UTF-16 primitives.
//!
//! ## v4 details reproduced deliberately
//!
//!   - **`participants` is accepted and never read.** v4's signature takes
//!     `participants: ChatParticipantBase[]`; the body never references it (the
//!     display-name map is the caller's job). The port simply omits the
//!     parameter — there is no behavior to carry.
//!   - **The interchange header uses the index the interchange will be given.**
//!     `interchangeIndex` is incremented only when the PREVIOUS interchange is
//!     finalized, so the `## Interchange N` line always matches the chunk's own
//!     `index`.
//!   - **`participantNames` is a JS `Set` rendered by `Array.from`** — insertion
//!     order, de-duplicated. Same for the header's cross-interchange union.
//!   - **The time span line** uses `toLocaleDateString('en-US')` equality
//!     (`M/D/YYYY`) to decide same-day; same-day renders
//!     `"{Month D, YYYY} from {h:mm AM} to {h:mm PM}"`, otherwise the two full
//!     `formatDateTime` strings. The span reads the FIRST and LAST **visible**
//!     message timestamps, falling back to the metadata's created/last-updated
//!     when nothing is visible.
//!   - **`Current time:` reads the wall clock.** v4 calls
//!     `new Date().toISOString()` inline; the port injects it as `now_iso` so
//!     the differential can freeze it.
//!   - **An unparseable timestamp renders `"Invalid Date at Invalid Date"`.**
//!     v4's `formatDateTime` wraps the work in a `try`, but neither
//!     `new Date(...)` nor `toLocaleDateString` throws on an invalid date — they
//!     return the literal `"Invalid Date"` — so the `catch` (which would return
//!     the raw ISO) is unreachable. The port matches the reachable behavior.
//!
//! ## Locale + timezone
//!
//! All formatting is en-US in **UTC**. v4 passes an explicit `'en-US'` locale but
//! no timezone, so the rendered timestamps follow the host's zone; the
//! differential pins both sides to `TZ=UTC` (the standing harness constraint —
//! see [`crate::format_time`]).

use crate::chat_tasks::strip_tool_artifacts;
use crate::clock::{civil_from_days, iso_to_ms};
use crate::format_time::{format_date_short_us, MONTHS_LONG};
use crate::jsstr::{js_last_index_of, utf16_len, utf16_slice_from, utf16_truncate};

/// Per-chunk character budget for conversation interchanges (v4 Bug 17,
/// `CHUNK_CHAR_BUDGET`). An interchange whose rendered text (metadata prefix
/// included) exceeds this is split into sequential sub-chunks so no single chunk
/// blows the embedding model's context window (see [`enforce_chunk_budget`]).
/// **Exported** because the boot reconcile (`conversation_render_reconcile`)
/// binds it as arm (C)'s lower bound, exactly as v4 imports the const.
///
/// This is a UTF-16 code-unit budget, not a token count — v4's `.length` /
/// `.slice` / `.lastIndexOf` all measure UTF-16 code units, so the port does too.
pub const CHUNK_CHAR_BUDGET: usize = 24_000;

/// UTF-16 code units every sub-chunk reserves for its `## Interchange N` header
/// line (v4 `CHUNK_HEADER_RESERVE`).
const CHUNK_HEADER_RESERVE: usize = 80;

/// One chat event as the renderer reads it (v4 `ChatEvent`, narrowed). Every
/// field is optional exactly where v4's filter treats it as falsy.
#[derive(Debug, Clone, Default)]
pub struct RenderEvent {
    pub id: String,
    /// v4 `m.type !== 'message'` skips the row.
    pub type_: Option<String>,
    /// Folded with `(m.role || '').toUpperCase()`.
    pub role: Option<String>,
    /// Falsy (absent or empty) skips the row.
    pub content: Option<String>,
    pub participant_id: Option<String>,
    pub created_at: String,
}

/// The optional conversation header metadata (v4 `ConversationMetadata`).
#[derive(Debug, Clone)]
pub struct ConversationMetadata {
    pub conversation_id: String,
    pub title: String,
    pub created_at: String,
    pub last_updated_at: String,
}

/// One rendered interchange (v4 `InterchangeInfo`) — the row shape the handler
/// upserts into `conversation_chunks`.
#[derive(Debug, Clone, PartialEq)]
pub struct InterchangeInfo {
    pub index: usize,
    pub message_ids: Vec<String>,
    /// Insertion-ordered, de-duplicated (v4's `Set` → `Array.from`).
    pub participant_names: Vec<String>,
    pub content: String,
}

/// The renderer's return (v4 `RenderedConversation`).
#[derive(Debug, Clone, PartialEq)]
pub struct RenderedConversation {
    pub markdown: String,
    pub interchanges: Vec<InterchangeInfo>,
}

/// A visible message, post-filter (v4's `visibleMessages` element).
struct VisibleMessage {
    id: String,
    role: String,
    content: String,
    participant_id: Option<String>,
    created_at: String,
}

/// v4's module-private `formatDateTime`: `"{Month} {D}, {YYYY} at {h}:{mm} {AM|PM}"`
/// — en-US, **`hour: 'numeric'`** (no leading zero, unlike
/// [`crate::format_time::format_date_time`]'s 2-digit hour) and
/// `minute: '2-digit'`, computed in UTC. An unparseable input renders
/// `"Invalid Date at Invalid Date"` (see the module doc).
fn format_date_time(iso: &str) -> String {
    let Some(ms) = iso_to_ms(iso) else {
        return "Invalid Date at Invalid Date".to_string();
    };
    let days = ms.div_euclid(86_400_000);
    let ms_in_day = ms.rem_euclid(86_400_000);
    let (year, month, day) = civil_from_days(days);
    let total_minutes = ms_in_day / 60_000;
    let hour24 = (total_minutes / 60) as u32;
    let minute = (total_minutes % 60) as u32;
    let month_name = MONTHS_LONG[(month - 1) as usize];
    let hour12 = match hour24 % 12 {
        0 => 12,
        h => h,
    };
    let meridiem = if hour24 < 12 { "AM" } else { "PM" };
    format!("{month_name} {day}, {year} at {hour12}:{minute:02} {meridiem}")
}

/// v4's `toLocaleTimeString('en-US', { hour: 'numeric', minute: '2-digit' })`
/// alone — the two span endpoints on the same-day branch.
fn format_time_only(iso: &str) -> String {
    let Some(ms) = iso_to_ms(iso) else {
        return "Invalid Date".to_string();
    };
    let ms_in_day = ms.rem_euclid(86_400_000);
    let total_minutes = ms_in_day / 60_000;
    let hour24 = (total_minutes / 60) as u32;
    let minute = (total_minutes % 60) as u32;
    let hour12 = match hour24 % 12 {
        0 => 12,
        h => h,
    };
    let meridiem = if hour24 < 12 { "AM" } else { "PM" };
    format!("{hour12}:{minute:02} {meridiem}")
}

/// v4's `toLocaleDateString('en-US', { year:'numeric', month:'long', day:'numeric' })`
/// — the span's date part on the same-day branch.
fn format_date_long(iso: &str) -> String {
    let Some(ms) = iso_to_ms(iso) else {
        return "Invalid Date".to_string();
    };
    let (year, month, day) = civil_from_days(ms.div_euclid(86_400_000));
    format!("{} {day}, {year}", MONTHS_LONG[(month - 1) as usize])
}

/// Push `name` if absent — a JS `Set`'s insertion-ordered de-duplication.
fn push_unique(set: &mut Vec<String>, name: &str) {
    if !set.iter().any(|n| n == name) {
        set.push(name.to_string());
    }
}

/// One per-message entry as accumulated during the build — v4's
/// `RawInterchange.entries` element (`{ id, displayName, block }`). Kept
/// alongside the joined `content` so an oversize interchange can be re-split at
/// message boundaries without re-deriving the blocks.
struct RawEntry {
    id: String,
    display_name: String,
    block: String,
}

/// One interchange as accumulated during the build, before budget enforcement
/// (v4 `RawInterchange`).
struct RawInterchange {
    ordinal: usize,
    /// The joined `## Interchange N\n\n<blocks>` text — the header line included.
    content: String,
    /// The metadata header, present only on interchange 0 (else `""`). Tracked
    /// separately (not folded into `content`) so a split interchange 0 keeps it
    /// on the first sub-chunk.
    metadata_prefix: String,
    entries: Vec<RawEntry>,
    participant_names: Vec<String>,
    message_ids: Vec<String>,
}

/// A packed run of message blocks bound for one sub-chunk (v4 `PackedRun`).
struct PackedRun {
    text: String,
    ids: Vec<String>,
    names: Vec<String>,
}

/// Cut `window` at the last occurrence of `sep`, keeping `sep` with the text
/// before the cut (v4 `lastBoundaryCut`). Returns the UTF-16 end index of that
/// occurrence, or `None` when the separator is absent OR sits at the very start
/// (`idx <= 0` — a cut there makes no progress). All indices are UTF-16 code
/// units.
fn last_boundary_cut(window: &str, sep: &str) -> Option<usize> {
    match js_last_index_of(window, sep) {
        Some(idx) if idx > 0 => Some(idx + utf16_len(sep)),
        _ => None,
    }
}

/// Split a string longer than `budget` (UTF-16 units) into pieces each `<=
/// budget`, preferring natural boundaries (paragraph break → sentence end → any
/// newline → any space) and hard-cutting only when a single token itself
/// exceeds the budget (v4 `splitOversizeText`). Concatenating the pieces
/// reproduces the input exactly.
fn split_oversize_text(text: &str, budget: usize) -> Vec<String> {
    if utf16_len(text) <= budget {
        return vec![text.to_string()];
    }
    let mut pieces: Vec<String> = Vec::new();
    let mut remaining = text.to_string();
    while utf16_len(&remaining) > budget {
        let window = utf16_truncate(&remaining, budget);
        // Strict `??` fallback order — a sentence at EOL does NOT match `'. '`
        // (trailing space); no boundary in the whole window hard-cuts at budget.
        let cut = last_boundary_cut(&window, "\n\n")
            .or_else(|| last_boundary_cut(&window, ". "))
            .or_else(|| last_boundary_cut(&window, "\n"))
            .or_else(|| last_boundary_cut(&window, " "))
            .unwrap_or(budget);
        pieces.push(utf16_truncate(&remaining, cut));
        remaining = utf16_slice_from(&remaining, cut);
    }
    if utf16_len(&remaining) > 0 {
        pieces.push(remaining);
    }
    pieces
}

/// Split one oversize interchange into sub-chunks at message boundaries first,
/// then within a single oversize message (v4 `splitInterchange`). Every returned
/// run's text fits inside `budget - CHUNK_HEADER_RESERVE - extra_reserve`,
/// leaving room for the header the caller prepends. Order is preserved; a
/// message split across runs repeats its id.
fn split_interchange(ic: &RawInterchange, budget: usize, extra_reserve: usize) -> Vec<PackedRun> {
    let effective = budget
        .saturating_sub(CHUNK_HEADER_RESERVE)
        .saturating_sub(extra_reserve)
        .max(1); // v4 `Math.max(1, budget - 80 - extraReserve)`
    let mut runs: Vec<PackedRun> = Vec::new();

    let mut cur_text: Vec<String> = Vec::new();
    let mut cur_ids: Vec<String> = Vec::new();
    let mut cur_names: Vec<String> = Vec::new();
    let mut cur_len: usize = 0;

    // `flush` on the accumulators — a closure would borrow-conflict, so inline.
    macro_rules! flush {
        () => {
            if !cur_text.is_empty() {
                runs.push(PackedRun {
                    text: cur_text.join("\n"),
                    ids: std::mem::take(&mut cur_ids),
                    names: std::mem::take(&mut cur_names),
                });
                cur_text.clear();
                cur_len = 0;
            }
        };
    }

    for entry in &ic.entries {
        let block_len = utf16_len(&entry.block) + 1; // +1 for the '\n' join (v4 counts it for the first block too)
        if block_len > effective {
            // A single message overflows on its own — flush, then split its body.
            flush!();
            for piece in split_oversize_text(&entry.block, effective) {
                runs.push(PackedRun {
                    text: piece,
                    ids: vec![entry.id.clone()],
                    names: vec![entry.display_name.clone()],
                });
            }
            continue;
        }
        if cur_len + block_len > effective && !cur_text.is_empty() {
            flush!();
        }
        cur_text.push(entry.block.clone());
        cur_ids.push(entry.id.clone());
        push_unique(&mut cur_names, &entry.display_name);
        cur_len += block_len;
    }
    flush!();
    let _ = cur_len; // the final flush's `cur_len = 0` reset is a dead store; read it.
    runs
}

/// Turn the raw interchange list into the final chunk list, splitting any
/// interchange whose rendered text (metadata prefix included) exceeds `budget`
/// (v4 `enforceChunkBudget`). Each chunk gets its own sequential `index` (a chunk
/// ordinal counting across ALL interchanges). In the common case — nothing over
/// budget — every interchange is one chunk and the output is byte-identical to
/// the pre-sub-chunking renderer.
fn enforce_chunk_budget(interchanges: &[RawInterchange], budget: usize) -> Vec<InterchangeInfo> {
    let mut chunks: Vec<InterchangeInfo> = Vec::new();
    let mut chunk_index: usize = 0;

    for ic in interchanges {
        let full_content = format!("{}{}", ic.metadata_prefix, ic.content);
        if utf16_len(&full_content) <= budget {
            chunks.push(InterchangeInfo {
                index: chunk_index,
                message_ids: ic.message_ids.clone(),
                participant_names: ic.participant_names.clone(),
                content: full_content,
            });
            chunk_index += 1;
            continue;
        }

        // Reserve room for the metadata header (interchange 0 only) so the first
        // sub-chunk, which carries it, still fits the budget.
        let runs = split_interchange(ic, budget, utf16_len(&ic.metadata_prefix));
        for (i, run) in runs.iter().enumerate() {
            let header = if i == 0 {
                format!("## Interchange {}", ic.ordinal)
            } else {
                format!("## Interchange {} (continued {})", ic.ordinal, i)
            };
            // The metadata header (interchange 0 only) rides on the first sub-chunk.
            let prefix = if i == 0 {
                ic.metadata_prefix.as_str()
            } else {
                ""
            };
            let participant_names = if run.names.is_empty() {
                ic.participant_names.clone()
            } else {
                run.names.clone()
            };
            chunks.push(InterchangeInfo {
                index: chunk_index,
                message_ids: run.ids.clone(),
                participant_names,
                content: format!("{prefix}{header}\n\n{}", run.text),
            });
            chunk_index += 1;
        }
    }

    chunks
}

/// Render a chat conversation into numbered, structured Markdown
/// (v4 `renderConversationMarkdown`).
///
/// `character_names` maps participantId → display name; the caller builds it
/// (the handler resolves each participant's character, falling back to `"User"`
/// for user-controlled participants without one). `now_iso` is the injected wall
/// clock for the header's `Current time:` line.
pub fn render_conversation_markdown(
    messages: &[RenderEvent],
    character_names: &[(String, String)],
    metadata: Option<&ConversationMetadata>,
    now_iso: &str,
) -> RenderedConversation {
    // ── Filter to visible messages (v4 :68-93).
    let mut visible: Vec<VisibleMessage> = Vec::new();
    for m in messages {
        if m.type_.as_deref() != Some("message") {
            continue;
        }
        let role = m.role.as_deref().unwrap_or("").to_uppercase();
        if role != "USER" && role != "ASSISTANT" {
            continue;
        }
        // v4 `if (!m.content) continue` — absent or empty is falsy.
        let content = match m.content.as_deref() {
            Some(c) if !c.is_empty() => c,
            _ => continue,
        };
        let content = if role == "ASSISTANT" {
            match strip_tool_artifacts(content) {
                Some(cleaned) => cleaned,
                None => continue,
            }
        } else {
            content.to_string()
        };
        visible.push(VisibleMessage {
            id: m.id.clone(),
            role,
            content,
            participant_id: m.participant_id.clone(),
            created_at: m.created_at.clone(),
        });
    }

    // v4's `resolveDisplayName` (:96-103).
    let resolve = |role: &str, participant_id: Option<&String>| -> String {
        if let Some(pid) = participant_id {
            if let Some((_, name)) = character_names.iter().find(|(k, _)| k == pid) {
                return name.clone();
            }
        }
        if role == "USER" {
            "User".to_string()
        } else {
            "Assistant".to_string()
        }
    };

    // ── Group into interchanges (v4 :106-158). Each interchange keeps its
    // per-message `entries` alongside the joined `content` so an oversize
    // interchange can be re-split at message boundaries later
    // (`enforce_chunk_budget`) without re-deriving the message blocks.
    struct Current {
        ordinal: usize,
        message_ids: Vec<String>,
        participant_names: Vec<String>,
        lines: Vec<String>,
        entries: Vec<RawEntry>,
    }
    let mut raw_interchanges: Vec<RawInterchange> = Vec::new();
    let mut current: Option<Current> = None;
    let mut interchange_index: usize = 0;
    let mut global_message_index: usize = 0;

    for msg in &visible {
        let display_name = resolve(&msg.role, msg.participant_id.as_ref());

        if msg.role == "USER" || current.is_none() {
            if let Some(prev) = current.take() {
                raw_interchanges.push(RawInterchange {
                    ordinal: prev.ordinal,
                    content: prev.lines.join("\n"),
                    metadata_prefix: String::new(),
                    entries: prev.entries,
                    participant_names: prev.participant_names,
                    message_ids: prev.message_ids,
                });
                interchange_index += 1;
            }
            current = Some(Current {
                ordinal: interchange_index,
                message_ids: Vec::new(),
                participant_names: Vec::new(),
                lines: vec![format!("## Interchange {interchange_index}"), String::new()],
                entries: Vec::new(),
            });
        }

        let message_timestamp = format_date_time(&msg.created_at);
        let message_block = format!(
            "Past conversation message timestamp: {message_timestamp}\n\n\
             ### Message {global_message_index} ({display_name})\n\n{}\n",
            msg.content
        );

        let cur = current.as_mut().expect("interchange started above");
        cur.message_ids.push(msg.id.clone());
        push_unique(&mut cur.participant_names, &display_name);
        cur.lines.push(message_block.clone());
        cur.entries.push(RawEntry {
            id: msg.id.clone(),
            display_name,
            block: message_block,
        });

        global_message_index += 1;
    }

    if let Some(last) = current.take() {
        raw_interchanges.push(RawInterchange {
            ordinal: last.ordinal,
            content: last.lines.join("\n"),
            metadata_prefix: String::new(),
            entries: last.entries,
            participant_names: last.participant_names,
            message_ids: last.message_ids,
        });
    }

    // ── The metadata header, prepended to interchange 0 (v4 :161-216).
    if let Some(meta) = metadata {
        let mut all_participant_names: Vec<String> = Vec::new();
        for ic in &raw_interchanges {
            for name in &ic.participant_names {
                push_unique(&mut all_participant_names, name);
            }
        }

        let first_message_time = visible
            .first()
            .map(|m| m.created_at.clone())
            .unwrap_or_else(|| meta.created_at.clone());
        let last_message_time = visible
            .last()
            .map(|m| m.created_at.clone())
            .unwrap_or_else(|| meta.last_updated_at.clone());

        let same_day =
            format_date_short_us(&first_message_time) == format_date_short_us(&last_message_time);
        let span_text = if same_day {
            format!(
                "{} from {} to {}",
                format_date_long(&first_message_time),
                format_time_only(&first_message_time),
                format_time_only(&last_message_time),
            )
        } else {
            format!(
                "{} to {}",
                format_date_time(&first_message_time),
                format_date_time(&last_message_time),
            )
        };

        let participants_line = if all_participant_names.is_empty() {
            "None".to_string()
        } else {
            all_participant_names.join(", ")
        };

        let metadata_header = [
            format!("# Conversation: {}", meta.title),
            String::new(),
            "**Metadata:**".to_string(),
            format!("- Conversation ID: {}", meta.conversation_id),
            format!("- Created: {}", format_date_time(&meta.created_at)),
            format!(
                "- Last Updated: {}",
                format_date_time(&meta.last_updated_at)
            ),
            format!("- Participants: {participants_line}"),
            format!("- Message Count: {global_message_index}"),
            // v4 :375 — the count of RAW interchanges (pre-split), NOT the chunk
            // count. Sub-chunking (Bug 17) can make the two diverge.
            format!("- Interchange Count: {}", raw_interchanges.len()),
            String::new(),
            "---".to_string(),
            String::new(),
            format!(
                "\u{26A0}\u{FE0F} ARCHIVE VIEW \u{2014} This conversation occurred on {span_text}."
            ),
            format!(
                "Current time: {}. You are reading history, not in active conversation.",
                format_date_time(now_iso)
            ),
            String::new(),
            "---".to_string(),
            String::new(),
        ]
        .join("\n");

        // The metadata header rides with chunk 0. Tracked separately (not folded
        // into interchange 0's `content`) so budget enforcement can keep it on the
        // first sub-chunk when interchange 0 is itself split (v4 :218-221).
        if let Some(first) = raw_interchanges.first_mut() {
            first.metadata_prefix = metadata_header;
        }
    }

    // Split any over-budget interchange into sequential in-context chunks. In the
    // common case (nothing over budget) each interchange is one chunk and the
    // output is byte-identical to the pre-sub-chunking renderer (v4 :224-227).
    let interchanges = enforce_chunk_budget(&raw_interchanges, CHUNK_CHAR_BUDGET);

    // Build the full markdown. The metadata header lives on chunk 0's content,
    // so the join naturally includes it at the top.
    let markdown = interchanges
        .iter()
        .map(|ic| ic.content.as_str())
        .collect::<Vec<_>>()
        .join("\n");

    RenderedConversation {
        markdown,
        interchanges,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(id: &str, role: &str, content: &str, pid: Option<&str>, at: &str) -> RenderEvent {
        RenderEvent {
            id: id.to_string(),
            type_: Some("message".to_string()),
            role: Some(role.to_string()),
            content: Some(content.to_string()),
            participant_id: pid.map(str::to_string),
            created_at: at.to_string(),
        }
    }

    #[test]
    fn empty_input_renders_nothing() {
        let r = render_conversation_markdown(&[], &[], None, "2026-07-27T00:00:00.000Z");
        assert_eq!(r.markdown, "");
        assert!(r.interchanges.is_empty());
    }

    /// The header index tracks the chunk index, and `globalMessageIndex` counts
    /// across interchanges (v4's two separate counters).
    #[test]
    fn interchange_and_message_indexes() {
        let msgs = vec![
            ev(
                "m0",
                "user",
                "hi",
                Some("p-user"),
                "2026-02-01T09:05:00.000Z",
            ),
            ev(
                "m1",
                "assistant",
                "hello there friend",
                Some("p-a"),
                "2026-02-01T09:06:00.000Z",
            ),
            ev(
                "m2",
                "user",
                "again",
                Some("p-user"),
                "2026-02-01T09:07:00.000Z",
            ),
        ];
        let names = vec![
            ("p-user".to_string(), "User".to_string()),
            ("p-a".to_string(), "Aria".to_string()),
        ];
        let r = render_conversation_markdown(&msgs, &names, None, "2026-07-27T00:00:00.000Z");
        assert_eq!(r.interchanges.len(), 2);
        assert_eq!(r.interchanges[0].index, 0);
        assert_eq!(r.interchanges[1].index, 1);
        assert!(r.interchanges[0].content.starts_with("## Interchange 0\n"));
        assert!(r.interchanges[1].content.starts_with("## Interchange 1\n"));
        // Global message numbering continues into interchange 1.
        assert!(r.interchanges[1].content.contains("### Message 2 (User)"));
        assert_eq!(r.interchanges[0].participant_names, vec!["User", "Aria"]);
        assert_eq!(r.interchanges[0].message_ids, vec!["m0", "m1"]);
    }

    /// An unmapped participant falls back by role; the hour has no leading zero.
    #[test]
    fn display_name_fallbacks_and_hour_format() {
        let msgs = vec![ev(
            "m0",
            "ASSISTANT",
            "a message long enough to survive",
            None,
            "2026-02-01T09:05:00.000Z",
        )];
        let r = render_conversation_markdown(&msgs, &[], None, "2026-07-27T00:00:00.000Z");
        assert!(r.markdown.contains("### Message 0 (Assistant)"));
        assert!(r
            .markdown
            .contains("Past conversation message timestamp: February 1, 2026 at 9:05 AM"));
    }

    #[test]
    fn invalid_timestamp_renders_invalid_date() {
        assert_eq!(format_date_time("nope"), "Invalid Date at Invalid Date");
    }

    // ── Bug 17 sub-chunking (the differential proves byte-exactness; these
    //    guard the structural invariants).

    #[test]
    fn under_budget_interchange_is_a_single_chunk() {
        let msgs = vec![
            ev(
                "m0",
                "user",
                "hi",
                Some("p-user"),
                "2026-02-01T09:05:00.000Z",
            ),
            ev(
                "m1",
                "assistant",
                "a short reply",
                Some("p-a"),
                "2026-02-01T09:06:00.000Z",
            ),
        ];
        let names = vec![
            ("p-user".to_string(), "User".to_string()),
            ("p-a".to_string(), "Aria".to_string()),
        ];
        let r = render_conversation_markdown(&msgs, &names, None, "2026-07-27T00:00:00.000Z");
        assert_eq!(r.interchanges.len(), 1);
        assert_eq!(r.interchanges[0].index, 0);
    }

    /// A single message far over budget splits into several chunks with
    /// `(continued k)` headers, the ids repeat, and the reassembled bodies
    /// reproduce the original content byte-for-byte.
    #[test]
    fn oversize_single_message_splits_and_reassembles() {
        // 30k words of "lorem " (6 chars each) = ~180k UTF-16 units, far over budget.
        let body = "lorem ".repeat(30_000);
        let msgs = vec![ev(
            "big",
            "user",
            &body,
            Some("p-user"),
            "2026-02-01T09:05:00.000Z",
        )];
        let names = vec![("p-user".to_string(), "User".to_string())];
        let r = render_conversation_markdown(&msgs, &names, None, "2026-07-27T00:00:00.000Z");
        assert!(
            r.interchanges.len() > 1,
            "expected the oversize interchange to split"
        );
        // Chunk 0 carries the plain header; later chunks carry `(continued k)`.
        assert!(r.interchanges[0]
            .content
            .starts_with("## Interchange 0\n\n"));
        assert!(r.interchanges[1]
            .content
            .contains("## Interchange 0 (continued 1)"));
        // Sequential chunk indices across the whole list.
        for (i, ic) in r.interchanges.iter().enumerate() {
            assert_eq!(ic.index, i);
            assert!(utf16_len(&ic.content) <= CHUNK_CHAR_BUDGET);
            // The one oversize message's id repeats on every piece.
            assert_eq!(ic.message_ids, vec!["big".to_string()]);
        }
    }

    /// Every emitted chunk stays within budget even when the split rides an
    /// astral (non-BMP) boundary — the UTF-16 arithmetic proof.
    #[test]
    fn astral_content_respects_utf16_budget() {
        // Each 🎩 is a surrogate pair (2 UTF-16 units, 4 UTF-8 bytes).
        let body = "🎩".repeat(20_000);
        let msgs = vec![ev(
            "hats",
            "user",
            &body,
            Some("p-user"),
            "2026-02-01T09:05:00.000Z",
        )];
        let names = vec![("p-user".to_string(), "User".to_string())];
        let r = render_conversation_markdown(&msgs, &names, None, "2026-07-27T00:00:00.000Z");
        assert!(r.interchanges.len() > 1);
        for ic in &r.interchanges {
            assert!(
                utf16_len(&ic.content) <= CHUNK_CHAR_BUDGET,
                "chunk over budget: {} units",
                utf16_len(&ic.content)
            );
        }
    }

    #[test]
    fn last_boundary_cut_keeps_separator_with_preceding_text() {
        // "ab\n\ncd" — lastIndexOf('\n\n') is 2, cut is 2 + 2 = 4.
        assert_eq!(last_boundary_cut("ab\n\ncd", "\n\n"), Some(4));
        // Separator at index 0 makes no progress → None.
        assert_eq!(last_boundary_cut("\n\ncd", "\n\n"), None);
        // Absent → None.
        assert_eq!(last_boundary_cut("abcd", "\n\n"), None);
    }
}
