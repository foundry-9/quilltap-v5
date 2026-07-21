//! Memory recap (v4 `lib/memory/memory-recap.ts` + the two supporting cheap-LLM
//! tasks `summarizeMemoryRecap` [`memory-tasks.ts`] and the vault
//! conversation-summary search [`conversation-summary-search.ts`]).
//!
//! Generates a narrative summary of a character's recent memories to inject at
//! the start of a conversation. Gathers memories by importance tier
//! (high/medium/low), sends them to the cheap LLM for narrative summarization,
//! builds the two vault-summary conversation lists (recent + relevant), and
//! returns the assembled recap content.
//!
//! Closes the `BuildContextSeams::resolve_memory_recap` seam (W4.6a). Everything
//! model-facing rides the ported [`CheapLlmTaskExecutor`] + `EmbeddingProvider`;
//! the vault-summary search composes the ported `search_document_chunks` +
//! `read_database_document` + `parse_frontmatter`.

pub mod distill;
mod prompt_text;

use serde_json::Value;

use crate::cheap_llm::{CheapLlmSelection, UncensoredFallbackOptions};
use crate::db::runtime::Db;
use crate::markdown::parse_frontmatter;
use crate::memory_weighting::{format_relative_age, MemoryInputs};
use crate::model::completion::{CompletionMessage, CompletionProvider};
use crate::model::embedding::{EmbeddingPriority, EmbeddingProvider};
use crate::services::cheap_llm_exec::CheapLlmTaskExecutor;
use crate::services::knowledge_injector::document_search::{
    search_document_chunks, DocumentSearchOptions,
};

use prompt_text::MEMORY_RECAP_PROMPT;

/// v4 `SUMMARIES_FOLDER` (`conversation-summary-vault-bridge.ts`).
const SUMMARIES_FOLDER: &str = "Conversation Summaries";

/// v4 recent-conversations ramp anchors (`memory-recap.ts`). The recap's two
/// lists ramp over the same 4K→32K token window (the 5→20 anchors of the
/// standalone greeting block are not on the recap path).
const RECENT_CONVERSATIONS_RAMP_MIN_TOKENS: i64 = 4000;
const RECENT_CONVERSATIONS_RAMP_MAX_TOKENS: i64 = 32000;

/// v4 recap-conversations ramp anchors (both lists ramp 3→10 over 4K→32K).
const RECAP_CONVERSATIONS_MIN: i64 = 3;
const RECAP_CONVERSATIONS_MAX: i64 = 10;

/// v4 `findRecentByImportanceTier` default limits.
const TIER_HIGH_LIMIT: i64 = 20;
const TIER_MEDIUM_LIMIT: i64 = 10;
const TIER_LOW_LIMIT: i64 = 5;

/// v4 `MemoryRecapResult` (the subset build_context consumes — the content).
#[derive(Clone, Debug, Default)]
pub struct MemoryRecapResult {
    pub content: String,
}

/// v4 `rampLimit`: linear ramp from `min` (at `min_tokens` of context or less) to
/// `max` (at `max_tokens` or more), rounded (JS `Math.round`, half-away-from-zero
/// for the non-negative inputs here). `None` maxContext yields `max`.
pub(crate) fn ramp_limit(
    max_context: Option<i64>,
    min: i64,
    max: i64,
    min_tokens: i64,
    max_tokens: i64,
) -> i64 {
    let Some(mc) = max_context else {
        return max;
    };
    if mc <= min_tokens {
        return min;
    }
    if mc >= max_tokens {
        return max;
    }
    let ratio = (mc - min_tokens) as f64 / (max_tokens - min_tokens) as f64;
    // JS `Math.round` (half up for non-negative).
    (min as f64 + ratio * ((max - min) as f64) + 0.5).floor() as i64
}

/// A past conversation surfaced by the vault-summary search (v4
/// `VaultConversationMatch`). Public since P4.d13 so the harness can drive the
/// search directly.
#[derive(Clone, Debug, PartialEq)]
pub struct VaultConversationMatch {
    pub conversation_id: String,
    pub conversation_title: String,
    /// Vault-relative path of the summary file the match came from.
    pub relative_path: String,
    /// Best cosine score among this conversation's chunks (post window boost).
    pub score: f64,
    /// ISO timestamp of the conversation's first message (frontmatter), when
    /// recorded AND finite under `Date.parse`.
    pub first_message_at: Option<String>,
    /// ISO timestamp of the conversation's last message (frontmatter).
    pub last_message_at: Option<String>,
}

/// v4 `renderRelevantConversationsBlock` — the `### Relevant Past Conversations`
/// block (entries only, no call note). `''` for an empty list. Episodic recall:
/// prints the conversation's date so "last week" becomes confirmable — the
/// frontmatter finally gets read.
pub fn render_relevant_conversations_block(matches: &[VaultConversationMatch]) -> String {
    if matches.is_empty() {
        return String::new();
    }
    let entries = matches
        .iter()
        .map(|m| {
            let date = match &m.first_message_at {
                // v4 `m.firstMessageAt ? \` (${m.firstMessageAt.slice(0, 10)})\` : ''`
                // — truthy gate, so an empty string renders no parenthetical.
                Some(iso) if !iso.is_empty() => {
                    format!(" ({})", crate::jsstr::utf16_truncate(iso, 10))
                }
                _ => String::new(),
            };
            format!(
                "#### {}{} (`{}`)",
                m.conversation_title, date, m.conversation_id
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!("### Relevant Past Conversations\n\n{entries}")
}

/// v4 `READ_CONVERSATION_CALL_NOTE`.
pub(crate) const READ_CONVERSATION_CALL_NOTE: &str = "_Pass any of the conversation IDs above (in backticks) to the `read_conversation` tool to revisit the full transcript._";

/// v4 `truncateGist` — cap an inlined gist so the recap block stays bounded.
/// `text.trim()`; if ≤ `max_chars` return it, else `slice(0, max_chars-1)` +
/// `trimEnd()` + `'…'`. UTF-16-faithful slice (JS `String.length` / `.slice`).
fn truncate_gist(text: &str, max_chars: usize) -> String {
    let trimmed = crate::jsstr::js_trim(text);
    if crate::jsstr::utf16_len(trimmed) <= max_chars {
        return trimmed.to_string();
    }
    let sliced = crate::jsstr::utf16_truncate(trimmed, max_chars - 1);
    format!("{}…", crate::jsstr::js_trim_end(&sliced))
}

/// v4 `searchVaultConversationSummaries`: semantically search a character's vault
/// conversation summaries. Degrades gracefully to `[]` on any failure (no vault,
/// dead embedding provider, unreadable files). `time_range` (episodic recall):
/// prefer conversations whose `[firstMessageAt, lastMessageAt]` span overlaps the
/// window — two-stage with the same fallback the memory path uses (filter first;
/// when fewer than `limit` overlap, fall back to the full pool with window hits
/// boosted ×1.3 instead — never fewer results than an unwindowed search).
/// Public since P4.d13 so the harness can drive it directly.
#[allow(clippy::too_many_arguments)]
pub async fn search_vault_conversation_summaries<E: EmbeddingProvider>(
    db: &Db,
    embedding: &E,
    character_id: &str,
    character_mount_point_id: Option<&str>,
    query: &str,
    user_id: &str,
    embedding_profile_id: Option<&str>,
    limit: usize,
    exclude_conversation_id: Option<&str>,
    time_range: Option<&crate::recall_tags::TimeWindow>,
) -> Vec<VaultConversationMatch> {
    if limit == 0 {
        return Vec::new();
    }
    let trimmed_query = crate::jsstr::js_trim(query);
    if trimmed_query.is_empty() {
        return Vec::new();
    }

    // Resolve the character's vault mount point (v4 `getCharacterVaultStore`):
    // must be a database-backed `'character'` store.
    let Some(mount_id) = character_mount_point_id else {
        return Vec::new();
    };
    let mount_id = mount_id.to_string();
    let store_ok = {
        let mid = mount_id.clone();
        db.read_mount_index(move |conn| {
            let repo = crate::db::doc_mount_points::DocMountPointsRepository::new(conn);
            match repo.find_store_naming_by_id(&mid)? {
                // v4 `getCharacterVaultStore`: `mountType === 'database'` +
                // `storeType === 'character'`.
                Some(store) => Ok(store.mount_type == "database"
                    && store.store_type.as_deref() == Some("character")),
                None => Ok(false),
            }
        })
        .unwrap_or(false)
    };
    if !store_ok {
        return Vec::new();
    }

    // Embed the query. A dead embedding provider simply yields no relevant list.
    let query_embedding = match embedding
        .generate_embedding_for_user(
            trimmed_query,
            user_id,
            embedding_profile_id,
            EmbeddingPriority::Interactive,
        )
        .await
    {
        Ok(r) => r.embedding,
        Err(_) => return Vec::new(),
    };

    // Search the vault, scoped to `Conversation Summaries/`. v4 pulls a larger
    // chunk pool (`max(limit*4, 20)`) so multi-chunk summaries still fill the list.
    let pool = std::cmp::max(limit * 4, 20);
    let path_prefix = format!("{SUMMARIES_FOLDER}/");
    let opts = DocumentSearchOptions {
        mount_point_ids: Some(vec![mount_id.clone()]),
        limit: Some(pool),
        path_prefix: Some(path_prefix),
        query: Some(trimmed_query.to_string()),
        ..Default::default()
    };
    let chunks = {
        let emb = query_embedding.clone();
        match db.read_mount_index(move |conn| search_document_chunks(conn, &emb, &opts)) {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        }
    };

    // Collapse to the best-scoring chunk per summary file (one file per
    // conversation), ordered by score desc. Use a stable insertion-ordered map so
    // ties keep first-seen order (JS `Map` iteration order).
    let mut best_by_path: Vec<(String, f64)> = Vec::new();
    for chunk in &chunks {
        if let Some(entry) = best_by_path
            .iter_mut()
            .find(|(p, _)| p == &chunk.relative_path)
        {
            if chunk.score > entry.1 {
                entry.1 = chunk.score;
            }
        } else {
            best_by_path.push((chunk.relative_path.clone(), chunk.score));
        }
    }
    // Stable sort by score desc (JS `sort((a,b) => b.score - a.score)` is stable in
    // V8; `sort_by` is stable in Rust).
    best_by_path.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    // Read frontmatter per surviving file to recover the conversationId (the
    // filename is derived from the title, not the id) and the message-span
    // timestamps. With a timeRange in play, read past `limit` so the window
    // staging below has a pool to choose from.
    let read_cap = if time_range.is_some() {
        std::cmp::max(limit * 3, 15)
    } else {
        limit
    };
    let mut matches: Vec<VaultConversationMatch> = Vec::new();
    for (relative_path, score) in best_by_path {
        if matches.len() >= read_cap {
            break;
        }
        // Read via a closure that maps the store error into `Option` so the
        // pooled-read wrapper (which wants `Result<_, DbError>`) is satisfied;
        // an unreadable/garbled summary file is skipped (v4's per-file try/catch).
        let content = {
            let mid = mount_id.clone();
            let rp = relative_path.clone();
            db.read_mount_index(move |conn| {
                Ok(
                    crate::db::database_store::read_database_document(conn, &mid, &rp)
                        .ok()
                        .map(|doc| doc.content),
                )
            })
        };
        let content = match content {
            Ok(Some(c)) => c,
            _ => continue,
        };
        let parsed = parse_frontmatter(&content);
        let Some(data) = parsed.data.as_ref() else {
            continue;
        };
        let conversation_id = match data.get("conversationId").and_then(Value::as_str) {
            Some(s) => s.to_string(),
            None => continue,
        };
        if let Some(excl) = exclude_conversation_id {
            if conversation_id == excl {
                continue;
            }
        }
        let title_value = data
            .get("conversationTitle")
            .and_then(Value::as_str)
            .map(|s| crate::jsstr::js_trim(s).to_string())
            .unwrap_or_default();
        let conversation_title = if !title_value.is_empty() {
            title_value
        } else {
            "Untitled conversation".to_string()
        };
        // v4 `tsField(key)`: a string that parses finite under `Date.parse`,
        // else null.
        let ts_field = |key: &str| -> Option<String> {
            data.get(key)
                .and_then(Value::as_str)
                .filter(|v| crate::episodic::event_time_ms(Some(v)).is_some())
                .map(str::to_string)
        };
        matches.push(VaultConversationMatch {
            conversation_id,
            conversation_title,
            relative_path: relative_path.clone(),
            score,
            first_message_at: ts_field("firstMessageAt"),
            last_message_at: ts_field("lastMessageAt"),
        });
    }
    let _ = character_id; // (logging-only in v4)

    // Window staging: filter-first with boost fallback (mirrors the memory
    // path's two-stage rule — never fewer results than an unwindowed search).
    if let Some(window) = time_range {
        if let (Some(from), Some(to)) = (
            crate::episodic::event_time_ms(Some(&window.from)),
            crate::episodic::event_time_ms(Some(&window.to)),
        ) {
            if from <= to {
                let overlaps_window = |m: &VaultConversationMatch| -> bool {
                    // v4: `start = m.firstMessageAt ? Date.parse(...) : NaN`
                    // (truthy gate — tsField already guarantees finite when
                    // present); `end = m.lastMessageAt ? Date.parse(...) : start`.
                    let start = match m
                        .first_message_at
                        .as_deref()
                        .filter(|s| !s.is_empty())
                        .and_then(|s| crate::episodic::event_time_ms(Some(s)))
                    {
                        Some(t) => t,
                        None => return false,
                    };
                    let span_end = m
                        .last_message_at
                        .as_deref()
                        .filter(|s| !s.is_empty())
                        .and_then(|s| crate::episodic::event_time_ms(Some(s)))
                        .unwrap_or(start);
                    start <= to && span_end >= from
                };
                let window_hits: Vec<VaultConversationMatch> = matches
                    .iter()
                    .filter(|m| overlaps_window(m))
                    .cloned()
                    .collect();
                if window_hits.len() >= limit {
                    let mut hits = window_hits;
                    hits.truncate(limit);
                    return hits;
                }
                const CONVERSATION_WINDOW_BOOST: f64 = 1.3;
                let mut boosted: Vec<VaultConversationMatch> = matches
                    .into_iter()
                    .map(|m| {
                        if overlaps_window(&m) {
                            VaultConversationMatch {
                                score: m.score * CONVERSATION_WINDOW_BOOST,
                                ..m
                            }
                        } else {
                            m
                        }
                    })
                    .collect();
                // Stable sort by score desc (V8 sort is stable).
                boosted.sort_by(|a, b| {
                    b.score
                        .partial_cmp(&a.score)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
                boosted.truncate(limit);
                return boosted;
            }
        }
    }

    matches.truncate(limit);
    matches
}

/// v4 `buildConversationRecallLists`: the recap's two vault-summary lists
/// (relevant + recent), deduped and rendered with the closing call note.
#[allow(clippy::too_many_arguments)]
async fn build_conversation_recall_lists<E: EmbeddingProvider>(
    db: &Db,
    embedding: &E,
    character_id: &str,
    character_mount_point_id: Option<&str>,
    current_chat_id: Option<&str>,
    user_id: &str,
    embedding_profile_id: Option<&str>,
    relevance_query: &str,
    max_context: Option<i64>,
) -> String {
    let recent_limit = ramp_limit(
        max_context,
        RECAP_CONVERSATIONS_MIN,
        RECAP_CONVERSATIONS_MAX,
        RECENT_CONVERSATIONS_RAMP_MIN_TOKENS,
        RECENT_CONVERSATIONS_RAMP_MAX_TOKENS,
    );
    let relevant_limit = ramp_limit(
        max_context,
        RECAP_CONVERSATIONS_MIN,
        RECAP_CONVERSATIONS_MAX,
        RECENT_CONVERSATIONS_RAMP_MIN_TOKENS,
        RECENT_CONVERSATIONS_RAMP_MAX_TOKENS,
    );
    if recent_limit <= 0 && relevant_limit <= 0 {
        return String::new();
    }

    // Relevant list — semantic retrieval over the embedded vault summaries.
    let mut relevant: Vec<VaultConversationMatch> = Vec::new();
    let trimmed_query = crate::jsstr::js_trim(relevance_query);
    if relevant_limit > 0 && !trimmed_query.is_empty() {
        relevant = search_vault_conversation_summaries(
            db,
            embedding,
            character_id,
            character_mount_point_id,
            trimmed_query,
            user_id,
            embedding_profile_id,
            relevant_limit as usize,
            current_chat_id,
            // Round 2 has no production caller passing a time window (the
            // round-3 mini-recap is the consumer).
            None,
        )
        .await;
    }

    // Recent list — recency-ordered chats that carry a summary.
    let mut recent: Vec<Value> = Vec::new();
    if recent_limit > 0 {
        let cid = character_id.to_string();
        let excl = current_chat_id.map(str::to_string);
        recent = db
            .read_main(move |conn| {
                crate::db::chats_read::find_recent_summarized_by_character(
                    conn,
                    &cid,
                    recent_limit,
                    excl.as_deref(),
                )
            })
            .unwrap_or_default();
    }

    // Dedup: a conversation in both lists stays in relevant, dropped from recent.
    let relevant_ids: std::collections::HashSet<&str> = relevant
        .iter()
        .map(|r| r.conversation_id.as_str())
        .collect();
    let recent_filtered: Vec<&Value> = recent
        .iter()
        .filter(|c| {
            let id = c.get("id").and_then(Value::as_str).unwrap_or("");
            !relevant_ids.contains(id)
        })
        .collect();

    if relevant.is_empty() && recent_filtered.is_empty() {
        return String::new();
    }

    let mut sections: Vec<String> = Vec::new();
    let relevant_block = render_relevant_conversations_block(&relevant);
    if !relevant_block.is_empty() {
        sections.push(relevant_block);
    }
    if !recent_filtered.is_empty() {
        let entries = recent_filtered
            .iter()
            .map(|c| {
                let title = c.get("title").and_then(Value::as_str).unwrap_or("");
                let id = c.get("id").and_then(Value::as_str).unwrap_or("");
                let gist_raw = c
                    .get("contextSummary")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let gist = crate::jsstr::js_trim(gist_raw);
                if crate::jsstr::utf16_len(gist) > 0 {
                    format!("#### {title} (`{id}`)\n{}", truncate_gist(gist, 280))
                } else {
                    format!("#### {title} (`{id}`)")
                }
            })
            .collect::<Vec<_>>()
            .join("\n\n");
        sections.push(format!("### Recent Conversations\n\n{entries}"));
    }

    format!(
        "{}\n\n{}",
        sections.join("\n\n"),
        READ_CONVERSATION_CALL_NOTE
    )
}

/// v4 `summarizeMemoryRecap`: format the tiered memories, call the cheap LLM,
/// parse `NO_MEMORIES`→`''`. `now_ms` drives the relative-age labels.
#[allow(clippy::too_many_arguments)]
async fn summarize_memory_recap<C: CompletionProvider>(
    executor: &CheapLlmTaskExecutor,
    completion: &C,
    character_name: &str,
    tiered: &TieredMemories,
    selection: &CheapLlmSelection,
    uncensored_fallback: Option<&UncensoredFallbackOptions<'_>>,
    character_id: &str,
    now_ms: f64,
) -> String {
    let total = tiered.high.len() + tiered.medium.len() + tiered.low.len();
    if total == 0 {
        return String::new();
    }

    let format_tier = |label: &str, rows: &[Value]| -> String {
        if rows.is_empty() {
            return String::new();
        }
        let lines = rows
            .iter()
            .map(|m| {
                let summary = m.get("summary").and_then(Value::as_str).unwrap_or("");
                let age = format_relative_age(&memory_inputs(m), now_ms);
                format!("- [{age}] {summary}")
            })
            .collect::<Vec<_>>()
            .join("\n");
        format!("### {label} Importance\n{lines}")
    };

    let memories_text = [
        format_tier("High", &tiered.high),
        format_tier("Medium", &tiered.medium),
        format_tier("Low", &tiered.low),
    ]
    .into_iter()
    .filter(|s| !s.is_empty())
    .collect::<Vec<_>>()
    .join("\n\n");

    let user_content = format!("Character: {character_name}\n\n## Memories\n{memories_text}");
    let messages = vec![
        CompletionMessage::system(MEMORY_RECAP_PROMPT),
        CompletionMessage::user(user_content),
    ];

    let result = executor
        .execute(
            completion,
            selection,
            messages,
            |content: &str| {
                let trimmed = crate::jsstr::js_trim(content);
                if trimmed == "NO_MEMORIES" {
                    String::new()
                } else {
                    trimmed.to_string()
                }
            },
            uncensored_fallback,
            None,
            Some(character_id),
            Some("memory-recap-summarization"),
        )
        .await;

    // v4: on failure or empty → narrative stays ''.
    if !result.success {
        return String::new();
    }
    result.result.unwrap_or_default()
}

/// The three tiers returned by `findRecentByImportanceTier`.
struct TieredMemories {
    high: Vec<Value>,
    medium: Vec<Value>,
    low: Vec<Value>,
}

/// The recap inputs build_context already carries.
pub struct MemoryRecapParams<'a> {
    pub character_id: &'a str,
    pub character_name: &'a str,
    pub character_mount_point_id: Option<&'a str>,
    pub selection: &'a CheapLlmSelection,
    pub user_id: &'a str,
    pub chat_id: Option<&'a str>,
    pub uncensored_fallback: Option<&'a UncensoredFallbackOptions<'a>>,
    pub max_context: Option<i64>,
    pub relevance_query: &'a str,
    pub embedding_profile_id: Option<&'a str>,
    pub now_ms: f64,
}

/// v4 `generateMemoryRecap`. Fetches memories across importance tiers, summarizes
/// via the cheap LLM, builds the two conversation lists from vault summaries, and
/// assembles the recap content. Any error → `{ content: '' }` (best-effort).
pub async fn generate_memory_recap<E: EmbeddingProvider, C: CompletionProvider>(
    db: &Db,
    embedding: &E,
    completion: &C,
    executor: &CheapLlmTaskExecutor,
    params: &MemoryRecapParams<'_>,
) -> MemoryRecapResult {
    // Fetch memories across all three importance tiers.
    let tiered = {
        let cid = params.character_id.to_string();
        match db.read_main(move |conn| {
            crate::db::memories_read::find_recent_by_importance_tier(
                conn,
                &cid,
                TIER_HIGH_LIMIT,
                TIER_MEDIUM_LIMIT,
                TIER_LOW_LIMIT,
            )
        }) {
            Ok((high, medium, low)) => TieredMemories { high, medium, low },
            Err(_) => return MemoryRecapResult::default(),
        }
    };

    // Build the two conversation lists (recent + relevant) from vault summaries.
    let conversations_block = build_conversation_recall_lists(
        db,
        embedding,
        params.character_id,
        params.character_mount_point_id,
        params.chat_id,
        params.user_id,
        params.embedding_profile_id,
        params.relevance_query,
        params.max_context,
    )
    .await;

    let total = tiered.high.len() + tiered.medium.len() + tiered.low.len();
    let mut narrative = String::new();
    if total > 0 {
        narrative = summarize_memory_recap(
            executor,
            completion,
            params.character_name,
            &tiered,
            params.selection,
            params.uncensored_fallback,
            params.character_id,
            params.now_ms,
        )
        .await;
    }

    if narrative.is_empty() && conversations_block.is_empty() {
        return MemoryRecapResult::default();
    }

    let intro = format!(
        "## What You Remember\nAs {}, here is what you recall from your experiences and past conversations:",
        params.character_name
    );
    let mut sections = vec![intro];
    if !narrative.is_empty() {
        sections.push(narrative);
    }
    if !conversations_block.is_empty() {
        sections.push(conversations_block);
    }
    MemoryRecapResult {
        content: sections.join("\n\n"),
    }
}

/// Build [`MemoryInputs`] from a memory `Value` (the reader shared with the
/// frozen-archive / weighting paths).
fn memory_inputs(v: &Value) -> MemoryInputs {
    let num = |k: &str| v.get(k).and_then(Value::as_f64);
    let ms = |k: &str| {
        v.get(k)
            .and_then(Value::as_str)
            .and_then(crate::clock::iso_to_ms)
            .map(|m| m as f64)
    };
    MemoryInputs {
        importance: num("importance").unwrap_or(0.0),
        reinforced_importance: num("reinforcedImportance"),
        created_at_ms: ms("createdAt").unwrap_or(f64::NAN),
        last_reinforced_at_ms: ms("lastReinforcedAt"),
        last_accessed_at_ms: ms("lastAccessedAt"),
        reinforcement_count: num("reinforcementCount").map(|c| c as u64),
        graph_degree: v
            .get("relatedMemoryIds")
            .and_then(Value::as_array)
            .map(|a| a.len())
            .unwrap_or(0),
        // Episodic spine (v4 8bf3cb5f): the declared kind + the event clock.
        kind_episodic: v.get("kind").and_then(Value::as_str) == Some("episodic"),
        occurred_at_ms: crate::episodic::event_time_ms(v.get("occurredAt").and_then(Value::as_str)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ramp_limit_endpoints_and_midpoint() {
        // None → max.
        assert_eq!(ramp_limit(None, 3, 10, 4000, 32000), 10);
        // <= min_tokens → min.
        assert_eq!(ramp_limit(Some(4000), 3, 10, 4000, 32000), 3);
        assert_eq!(ramp_limit(Some(1000), 3, 10, 4000, 32000), 3);
        // >= max_tokens → max.
        assert_eq!(ramp_limit(Some(32000), 3, 10, 4000, 32000), 10);
        assert_eq!(ramp_limit(Some(40000), 3, 10, 4000, 32000), 10);
        // Midpoint (18000): ratio = 14000/28000 = 0.5 → 3 + 0.5*7 = 6.5 → round 7.
        assert_eq!(ramp_limit(Some(18000), 3, 10, 4000, 32000), 7);
    }

    #[test]
    fn truncate_gist_caps_and_ellipsizes() {
        assert_eq!(truncate_gist("  short  ", 280), "short");
        let long = "a".repeat(300);
        let out = truncate_gist(&long, 280);
        // 279 'a' + '…'.
        assert_eq!(out, format!("{}…", "a".repeat(279)));
    }

    fn mk_match(cid: &str, title: &str, first: Option<&str>) -> VaultConversationMatch {
        VaultConversationMatch {
            conversation_id: cid.to_string(),
            conversation_title: title.to_string(),
            relative_path: "Conversation Summaries/x.md".to_string(),
            score: 0.9,
            first_message_at: first.map(str::to_string),
            last_message_at: None,
        }
    }

    #[test]
    fn render_relevant_block_empty_and_nonempty() {
        assert_eq!(render_relevant_conversations_block(&[]), "");
        let m = vec![mk_match("cid-1", "A Talk", None)];
        assert_eq!(
            render_relevant_conversations_block(&m),
            "### Relevant Past Conversations\n\n#### A Talk (`cid-1`)"
        );
    }

    #[test]
    fn render_relevant_block_prints_frontmatter_date() {
        let m = vec![mk_match(
            "cid-3",
            "The Harbor Visit",
            Some("2026-07-14T10:00:00.000Z"),
        )];
        assert_eq!(
            render_relevant_conversations_block(&m),
            "### Relevant Past Conversations\n\n#### The Harbor Visit (2026-07-14) (`cid-3`)"
        );
    }
}
