//! Port of v4's `search` tool — the Scriptorium unified search
//! (`lib/tools/handlers/search-scriptorium-handler.ts` +
//! `lib/tools/search-scriptorium-tool.ts`).
//!
//! Searches across four sources — `memories` (the recall pipeline's one-shot
//! [`search_memories`](crate::services::recall::search::search_memories)),
//! `conversations` ([`crate::db::conversation_search`]), `documents`
//! ([`crate::services::knowledge_injector::document_search`]), and `knowledge`
//! (the same document search narrowed to `Knowledge/`, per tier) — merges them,
//! ranks by relevance, and slices to `limit`. Each source runs in its own
//! error-swallowing block (v4's per-branch try/catch): one source failing never
//! sinks the others.
//!
//! Serves BOTH the standard `search` and the Brahma-Console variant. The Brahma
//! definition just omits the `memories` source from the schema; the handler
//! enforces the same exclusion defensively (`operatorSurface` → memory search off,
//! documents/knowledge reach every enabled store, conversations go operator-wide).
//!
//! **Search itself writes nothing** (v4 `3f7320138` F6); the tool stamps only
//! the memories it returns, through the recall pipeline's one `lastAccessedAt`
//! writer, [`stamp_accessed`](crate::services::recall::deliver::stamp_accessed). The differential
//! isolates each case on a fresh fixture copy so the stamp never contaminates a
//! later case.

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::db::conversation_search::{search_conversation_chunks, ConversationSearchOptions};
use crate::db::runtime::Db;
// === P4.D376 ===
use crate::db::tiered_mount_pool::{
    flatten_tier_pool, resolve_tiered_mount_pool, Enabled, FlattenOptions, FlattenScope, Roster,
    TierContext, TierResolveOptions, TieredMountPool,
};
// === end P4.D376 ===
use crate::db::reach::{bucket_mounts, BucketScope};
use crate::db::{characters_read, doc_mount_points, js_number_to_json, DbError};
use crate::doc_edit::uri_producers::DocStoreUriResolver;
use crate::jsnum::to_fixed;
use crate::literal_boost::LITERAL_BOOST_GLOBAL;
use crate::memory_scoring::{importance_band, ImportanceBand, TOOL_MIN_IMPORTANCE};
use crate::memory_time::calendar::zoned_day_bounds_iso;
use crate::model::embedding::{EmbeddingCallOptions, EmbeddingProvider};
// === P4.D376 === (`roster_gated_project_id` moved inside the pool — ruling L-4)
// === end P4.D376 ===
use crate::services::knowledge_injector::{
    search_document_chunks, DocumentSearchOptions, DocumentSearchResult,
};
use crate::services::recall::search::{
    search_memories as search_character_memories, RecallSearchOptions, SearchMemoriesOptions,
};
use crate::tools::llm_number::llm_number;
// === P4.D375 ===
use crate::services::chat_window::find_chats_in_window;
use crate::services::recall::score::is_narrow_tool_window;
// === end P4.D375 ===

use super::help_search::truncate_utf16;

/// v4 `SearchScriptoriumToolContext`.
pub struct SearchContext {
    pub user_id: String,
    /// Absent on the operator surface (Brahma Console).
    pub character_id: Option<String>,
    pub embedding_profile_id: Option<String>,
    pub project_id: Option<String>,
    pub operator_surface: bool,
    /// A pre-built pool — "what this chat could see" before the chat exists (the
    /// Scenario Builder; v4 `SearchScriptoriumToolContext.mountPool`,
    /// `d1c06cd9d`). When set, the pool is used as-is instead of resolved, the
    /// cast vaults (participant tier) are searched, and only `documents` /
    /// `knowledge` run: memories and conversations are forced off. Never
    /// combined with `operator_surface`.
    pub mount_pool: Option<TieredMountPool>,
    /// The host's calendar NAME — the memory calendar's fallback when the
    /// instance names no timezone (v4's env / system rungs of
    /// `refreshMemoryTimeZone`); read only when `since` / `until` is given.
    pub memory_zone_fallback: String,
    // === P4.D375 ===
    /// The chat the search runs in — left out of the window census (v4
    /// `chatId`, 4.12.1).
    pub chat_id: Option<String>,
    // === end P4.D375 ===
}

/// v4 `SearchScriptoriumToolOutput` — `{ success, results?, error?, totalFound,
/// query, windowCensus? }`. `results` is pre-built as v4-exact `Value`s
/// (per-source metadata order + JS-number score rendering).
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SearchOutput {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub results: Option<Vec<Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(rename = "totalFound")]
    pub total_found: i64,
    pub query: String,
    /// P4.D375: the period's census, on a dated search that read memories or
    /// conversations (v4's spread — absent, not `null`, otherwise).
    #[serde(rename = "windowCensus", skip_serializing_if = "Option::is_none")]
    pub window_census: Option<SearchWindowCensus>,
}

impl SearchOutput {
    fn failure(error: impl Into<String>, query: impl Into<String>) -> Self {
        SearchOutput {
            success: false,
            results: None,
            error: Some(error.into()),
            total_found: 0,
            query: query.into(),
            window_census: None,
        }
    }
}

/// v4 `validateSearchScriptoriumInput` (the full, non-Brahma schema — the handler
/// always validates with it). `query` string `1..=500` (UTF-16); optional `sources`
/// (array of the 4-enum), `scope` (4-enum), `limit` (int `1..=20`), `minImportance`
/// (`0..=1`). Extra keys allowed.
fn validate_input(args: &Value) -> bool {
    let Some(query) = args.get("query").and_then(Value::as_str) else {
        return false;
    };
    let qlen = query.encode_utf16().count();
    if !(1..=500).contains(&qlen) {
        return false;
    }
    if let Some(sources) = args.get("sources") {
        let Some(arr) = sources.as_array() else {
            return false;
        };
        let ok = arr.iter().all(|s| {
            matches!(
                s.as_str(),
                Some("memories" | "conversations" | "documents" | "knowledge")
            )
        });
        if !ok {
            return false;
        }
    }
    if let Some(scope) = args.get("scope") {
        if !matches!(
            scope.as_str(),
            Some("all" | "project" | "character" | "group")
        ) {
            return false;
        }
    }
    // limit / minImportance are llmNumber(...) fields: a numeric-looking string
    // converts before validation, then the bounds apply exactly as to a number.
    if let Some(limit) = args.get("limit") {
        match llm_number(limit).as_f64() {
            Some(n) if n.fract() == 0.0 && (1.0..=20.0).contains(&n) => {}
            _ => return false,
        }
    }
    if let Some(min_importance) = args.get("minImportance") {
        match llm_number(min_importance).as_f64() {
            Some(n) if (0.0..=1.0).contains(&n) => {}
            _ => return false,
        }
    }
    // Episodic recall (P4.d13): `since`/`until` strings matching
    // `/^\d{4}-\d{2}-\d{2}/`; `aboutCharacter` string 1..=100 (UTF-16).
    for key in ["since", "until"] {
        if let Some(v) = args.get(key) {
            match v.as_str() {
                Some(s) if starts_with_date_prefix(s) => {}
                _ => return false,
            }
        }
    }
    if let Some(about) = args.get("aboutCharacter") {
        match about.as_str() {
            Some(s) => {
                let len = s.encode_utf16().count();
                if !(1..=100).contains(&len) {
                    return false;
                }
            }
            None => return false,
        }
    }
    true
}

/// The Zod `.regex(/^\d{4}-\d{2}-\d{2}/)` gate (JS `\d` is ASCII-only).
fn starts_with_date_prefix(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() >= 10
        && b[..4].iter().all(u8::is_ascii_digit)
        && b[4] == b'-'
        && b[5].is_ascii_digit()
        && b[6].is_ascii_digit()
        && b[7] == b'-'
        && b[8].is_ascii_digit()
        && b[9].is_ascii_digit()
}

/// One knowledge tier — a label, its mount points, and its literal-phrase boost.
struct KnowledgeTier {
    tier: &'static str,
    mount_point_ids: Vec<String>,
    boost: f64,
}

/// Execute the `search` tool (v4 `executeSearchScriptoriumTool`). `now_ms` is the
/// `Date.now()` seam the memory branch's decay reads (pinned by the differential).
pub async fn execute_search_scriptorium<P: EmbeddingProvider>(
    db: &Db,
    provider: &P,
    context: &SearchContext,
    args: &Value,
    now_ms: f64,
) -> SearchOutput {
    if !validate_input(args) {
        // v4 `search-scriptorium-handler.ts:72`; `input` is an object, so it rides
        // the `…Json` file-layer convention (P4.114).
        tracing::warn!(
            context = "search-scriptorium-handler",
            userId = context.user_id.as_str(),
            characterId = context.character_id.as_deref(),
            inputJson = %serde_json::to_string(args).unwrap_or_default(),
            "Search scriptorium tool validation failed"
        );
        return SearchOutput::failure(
            "Invalid input: query is required and must be a non-empty string",
            "",
        );
    }

    let query = args.get("query").and_then(Value::as_str).unwrap_or("");
    // === P4.D375 === `parsed.sources` (an explicit array, even empty, is
    // `Some`; the destructured default is gone — v4 4.12.1).
    let parsed_sources: Option<Vec<String>> =
        args.get("sources").and_then(Value::as_array).map(|arr| {
            arr.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        });
    // === end P4.D375 ===
    let scope = args.get("scope").and_then(Value::as_str).unwrap_or("all");
    // The reads go through the same seam: v4's Zod parse REPLACES the value, so
    // a quoted "5" reaches the handler as the number 5 — reading the raw arg
    // would validate leniently and then silently use the default.
    let limit = args
        .get("limit")
        .and_then(|raw| llm_number(raw).as_f64())
        .map(|n| n as usize)
        .unwrap_or(10);
    let min_importance = args
        .get("minImportance")
        .and_then(|raw| llm_number(raw).as_f64())
        .unwrap_or(TOOL_MIN_IMPORTANCE);
    let since = args.get("since").and_then(Value::as_str);
    let until = args.get("until").and_then(Value::as_str);
    let about_character = args.get("aboutCharacter").and_then(Value::as_str);

    // Time filters (episodic recall): date-only bounds widen to full LOCAL
    // days on the memory calendar (bug 212), the calendar memories are dated
    // on — never UTC days; a date the calendar refuses keeps v4's UTC-day
    // fallback. Applied to memories via occurredAt ?? createdAt and to
    // conversation hits via the source chat's timestamps. (Lengths are UTF-16
    // code units in v4; these prefixes are ASCII so bytes are identical.)
    let memory_time_zone = (since.is_some() || until.is_some()).then(|| {
        crate::services::memory_time_zone::refresh_memory_time_zone(
            db,
            &context.memory_zone_fallback,
            now_ms,
        )
    });
    let zone = memory_time_zone.as_deref().unwrap_or("UTC");
    let since_iso = since.map(|s| {
        if crate::jsstr::utf16_len(s) == 10 {
            zoned_day_bounds_iso(s, zone)
                .map(|w| w.from)
                .unwrap_or_else(|| format!("{s}T00:00:00.000Z"))
        } else {
            s.to_string()
        }
    });
    let until_iso = until.map(|s| {
        if crate::jsstr::utf16_len(s) == 10 {
            zoned_day_bounds_iso(s, zone)
                .map(|w| w.to)
                .unwrap_or_else(|| format!("{s}T23:59:59.999Z"))
        } else {
            s.to_string()
        }
    });
    let time_window = if since_iso.is_some() || until_iso.is_some() {
        Some(crate::recall_tags::TimeWindow {
            from: since_iso.unwrap_or_else(|| "0000-01-01T00:00:00.000Z".to_string()),
            to: until_iso.unwrap_or_else(|| "9999-12-31T23:59:59.999Z".to_string()),
        })
    } else {
        None
    };

    // === P4.D375 ===
    // A dated search with no explicit sources leaves documents out: they carry
    // no dates, and would crowd dated hits out of the cross-source cut. The
    // Scenario Builder (pre-built pool) keeps its own default; the operator
    // surface has no memories, so its dated default is conversations alone.
    let dated = since.is_some() || until.is_some();
    let defaulted_sources = parsed_sources.is_none() && dated && context.mount_pool.is_none();
    let sources: Vec<String> = match parsed_sources {
        Some(s) => s,
        None if defaulted_sources => {
            if context.operator_surface {
                vec!["conversations".into()]
            } else {
                vec!["memories".into(), "conversations".into()]
            }
        }
        None => vec![
            "memories".into(),
            "conversations".into(),
            "documents".into(),
            "knowledge".into(),
        ],
    };
    // === end P4.D375 ===

    // Operator surface (Brahma Console): no character, no memories — and a
    // pre-built pool overrides it (v4 `d1c06cd9d`: `!!operatorSurface &&
    // !mountPool`; the executor refuses the two together, so this only matters
    // to a direct caller).
    let operator_wide = context.operator_surface && context.mount_pool.is_none();
    // A pre-built pool is a documents-only surface (the Scenario Builder): its
    // schema never offers memories or conversations, and the handler enforces the
    // same exclusion defensively.
    let prebuilt = context.mount_pool.is_some();
    let search_memories = sources.iter().any(|s| s == "memories")
        && !operator_wide
        && !prebuilt
        && context.character_id.is_some();
    let search_conversations = sources.iter().any(|s| s == "conversations") && !prebuilt;
    let search_documents = sources.iter().any(|s| s == "documents");
    let search_knowledge = sources.iter().any(|s| s == "knowledge");

    // === P4.D375 ===
    // Window-first: the period's conversations, read once, scope the chunk
    // search and feed the census. A narrow window is its own relevance filter.
    // The current chat is NOT excluded here (the census leaves it out).
    let narrow_window = is_narrow_tool_window(time_window.as_ref());
    let window_chats: Option<Vec<Value>> = match &time_window {
        Some(w) if search_memories || search_conversations => {
            let character = if operator_wide {
                None
            } else {
                context.character_id.as_deref()
            };
            Some(
                db.read_main(|c| {
                    Ok(find_chats_in_window(
                        c,
                        character,
                        &context.user_id,
                        w,
                        None,
                    ))
                })
                .unwrap_or_default(),
            )
        }
        _ => None,
    };
    let mut window_memory_count: Option<i64> = None;
    // === end P4.D375 ===

    // Build the URI resolver (always) + the tri-tier pool / operator ids (only when
    // a document/knowledge source is requested). Any DB error here → outer failure.
    let pool_ctx = match build_pool_context(db, context, search_documents || search_knowledge) {
        Ok(pc) => pc,
        Err(e) => {
            // v4's outer `catch` (`search-scriptorium-handler.ts:544-549`) — the
            // one arm of v5's that reaches it (the smalls unification).
            tracing::error!(
                context = "search-scriptorium-handler",
                userId = context.user_id.as_str(),
                characterId = context.character_id.as_deref(),
                error = %e,
                "Search scriptorium tool execution failed"
            );
            return SearchOutput::failure(e.to_string(), query);
        }
    };
    let PoolContext {
        resolver,
        pool,
        owns_character,
        operator_store_ids,
    } = pool_ctx;

    // (score, value) pairs, pushed in v4 order: memory, conversation, knowledge,
    // then the deferred document rows.
    let mut results: Vec<(f64, Value)> = Vec::new();

    // --- Memories (never on the operator surface) --------------------------
    if search_memories {
        if let Some(cid) = &context.character_id {
            // v4 wraps the character check + search in one try/catch (swallow).
            let owned = db
                .read_main(|main| {
                    db.read_mount_index(|mount| characters_read::find_by_id(main, mount, cid))
                })
                .map(|ch| {
                    ch.map(|c| {
                        c.get("userId").and_then(Value::as_str) == Some(context.user_id.as_str())
                    })
                    .unwrap_or(false)
                });
            // === P4.D375 === Census: every memory dated in the window,
            // whatever the query or aboutCharacter matched (all tiers — the tool
            // sees the archive). Inside v4's one try/catch: a read that fails
            // (strict scope only — the fallback answers `[]`) skips the branch.
            let census_ok = match (&owned, &time_window) {
                (Ok(true), Some(w)) => {
                    let (cid2, w2) = (cid.clone(), w.clone());
                    match db.read_main(move |c| {
                        crate::db::memories_read::find_ids_in_event_window(c, &cid2, &w2, false)
                    }) {
                        Ok(ids) => {
                            window_memory_count = Some(ids.len() as i64);
                            tracing::debug!(
                                characterId = %cid,
                                count = ids.len(),
                                narrowWindow = narrow_window,
                                "Memories in search window"
                            );
                            true
                        }
                        Err(_) => false,
                    }
                }
                _ => true,
            };
            // === end P4.D375 ===
            if let (Ok(true), true) = (owned, census_ok) {
                // Optional about-character filter: resolve the name (or alias)
                // to a character id among the user's characters. An
                // unresolvable name returns NO memory hits rather than
                // silently ignoring the filter.
                let mut about_character_id: Option<String> = None;
                let mut about_character_unresolved = false;
                if let Some(needle_raw) = about_character {
                    let needle = crate::jsstr::js_trim(needle_raw).to_lowercase();
                    let all = db
                        .read_main(|main| {
                            db.read_mount_index(|mount| characters_read::find_all(main, mount))
                        })
                        .unwrap_or_default();
                    let matched = all.iter().find(|c| {
                        let name_match = c
                            .get("name")
                            .and_then(Value::as_str)
                            .map(|n| crate::jsstr::js_trim(n).to_lowercase() == needle)
                            .unwrap_or(false);
                        if name_match {
                            return true;
                        }
                        c.get("aliases")
                            .and_then(Value::as_array)
                            .map(|arr| {
                                arr.iter()
                                    .filter_map(Value::as_str)
                                    .any(|a| crate::jsstr::js_trim(a).to_lowercase() == needle)
                            })
                            .unwrap_or(false)
                    });
                    match matched.and_then(|c| c.get("id").and_then(Value::as_str)) {
                        Some(id) => about_character_id = Some(id.to_string()),
                        None => about_character_unresolved = true,
                    }
                }

                if !about_character_unresolved {
                    let opts = SearchMemoriesOptions {
                        search: RecallSearchOptions {
                            limit: Some(limit),
                            min_importance: Some(min_importance),
                            about_character_id,
                            literal_phrase: true,
                            // The drill-down tool sees the archive too: cold
                            // shards are returned labelled, so a character can
                            // find what a digest replaced (v4 `d58548051`).
                            include_cold: true,
                            now_ms,
                            ..Default::default()
                        },
                        user_id: context.user_id.clone(),
                        embedding_profile_id: context.embedding_profile_id.clone(),
                        // Tool path: a plain hard filter on event time (no
                        // recallContext, so no soft fallback — the caller asked
                        // for a window).
                        window: time_window.clone(),
                        ..Default::default()
                    };
                    if let Ok(mems) =
                        search_character_memories(db, provider, cid, query, &opts).await
                    {
                        for mr in mems {
                            results
                                .push((mr.score, memory_result(&mr.memory, mr.score, mr.salience)));
                        }
                    }
                }
            }
        }
    }

    // --- Conversations -----------------------------------------------------
    // === P4.D375 === The window's chats only, scored whole; within a narrow
    // window every chunk is eligible however loosely it matches. With a window
    // and no chats in it, the chunk search is skipped (no embedding call).
    // v4's post-hoc `spanByChat` filter is gone (4.12.1).
    let window_chat_ids: Option<std::collections::HashSet<String>> =
        window_chats.as_ref().map(|chats| {
            chats
                .iter()
                .filter_map(|c| c.get("id").and_then(Value::as_str).map(str::to_string))
                .collect()
        });
    if search_conversations && window_chat_ids.as_ref().is_some_and(|ids| ids.is_empty()) {
        tracing::debug!(
            characterId = context.character_id.as_deref(),
            "No conversations in search window; chunk search skipped"
        );
    } else if search_conversations {
        if let Ok(emb) = embed(provider, query, context).await {
            let opts = ConversationSearchOptions {
                character_id: context.character_id.clone(),
                user_id: Some(context.user_id.clone()),
                limit: Some(limit),
                min_score: Some(if narrow_window { 0.0 } else { 0.3 }),
                query: Some(query.to_string()),
                apply_literal_phrase_boost: true,
                chat_ids: window_chat_ids.clone(),
            };
            if let Ok(convs) = db.read_main(|c| search_conversation_chunks(c, &emb, &opts)) {
                if let Some(ids) = &window_chat_ids {
                    tracing::debug!(
                        windowChats = ids.len(),
                        hits = convs.len(),
                        "Window conversation search"
                    );
                }
                for cr in convs {
                    results.push((cr.score, conversation_result(&cr)));
                }
            }
        }
    }
    // === end P4.D375 ===

    // --- Documents (deferred until after knowledge dedup) ------------------
    let mut document_rows = Vec::new();
    if search_documents {
        let documents_pool = if operator_wide {
            operator_store_ids.clone()
        } else {
            flatten_tier_pool(
                &pool,
                FlattenOptions {
                    scope: flatten_scope(scope),
                    // A pre-built pool's cast vaults ride the participant tier.
                    include_participants: prebuilt,
                    ..Default::default()
                },
            )
        };
        if !documents_pool.is_empty() {
            if let Ok(emb) = embed(provider, query, context).await {
                let opts = DocumentSearchOptions {
                    mount_point_ids: Some(documents_pool),
                    limit: Some(limit),
                    min_score: Some(0.3),
                    query: Some(query.to_string()),
                    apply_literal_phrase_boost: true,
                    ..Default::default()
                };
                if let Ok(rows) = db.read_mount_index(|m| search_document_chunks(m, &emb, &opts)) {
                    document_rows = rows;
                }
            }
        }
    }

    // --- Knowledge tiers ---------------------------------------------------
    let mut knowledge_chunk_ids: std::collections::HashSet<String> =
        std::collections::HashSet::new();
    if search_knowledge {
        let tiers =
            build_knowledge_tiers(scope, operator_wide, prebuilt, &pool, &operator_store_ids);
        let tiers_allowed: Vec<KnowledgeTier> = tiers
            .into_iter()
            .filter(|t| t.tier != "character" || owns_character)
            .collect();
        if !tiers_allowed.is_empty() {
            if let Ok(emb) = embed(provider, query, context).await {
                // Tiers processed in order (character > group > project > global) so
                // the dedup keeps the closest voice — matching v4's tier-ordered
                // Promise.all push order over synchronous DB reads.
                for tier in &tiers_allowed {
                    let opts = DocumentSearchOptions {
                        mount_point_ids: Some(tier.mount_point_ids.clone()),
                        path_prefix: Some("Knowledge/".to_string()),
                        limit: Some(limit),
                        min_score: Some(0.3),
                        query: Some(query.to_string()),
                        apply_literal_phrase_boost: true,
                        literal_boost_fraction: Some(tier.boost),
                        ..Default::default()
                    };
                    // Per-tier error swallowed (continue with other tiers).
                    if let Ok(hits) =
                        db.read_mount_index(|m| search_document_chunks(m, &emb, &opts))
                    {
                        for kr in hits {
                            if knowledge_chunk_ids.contains(&kr.chunk_id) {
                                continue;
                            }
                            knowledge_chunk_ids.insert(kr.chunk_id.clone());
                            let uri = resolver.uri_for_mount(
                                &kr.mount_point_name,
                                &kr.mount_point_id,
                                &kr.relative_path,
                            );
                            results.push((kr.score, knowledge_result(&kr, &uri, tier.tier)));
                        }
                    }
                }
            }
        }
    }

    // --- Deferred document rows (skip any chunk that came through as knowledge) --
    for dr in &document_rows {
        if knowledge_chunk_ids.contains(&dr.chunk_id) {
            continue;
        }
        let uri =
            resolver.uri_for_mount(&dr.mount_point_name, &dr.mount_point_id, &dr.relative_path);
        results.push((dr.score, document_result(dr, &uri)));
    }

    // Sort by relevance desc (stable), slice to limit.
    results.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    results.truncate(limit);
    let results: Vec<Value> = results.into_iter().map(|(_, v)| v).collect();

    // Only the memories that survived the cross-source cut reach the model;
    // those, and only those, count as accessed (v4 `3f7320138` F6 — the search
    // no longer stamps its over-fetched candidates). v4's filter is `typeof id
    // === 'string'`; an empty id is dropped by the stamp's own filter. Awaited
    // inline (P4.D268 R-D).
    if let Some(cid) = &context.character_id {
        let returned: Vec<String> = results
            .iter()
            .filter(|r| r["sourceType"] == "memory")
            .filter_map(|r| r["metadata"]["memoryId"].as_str().map(str::to_string))
            .collect();
        crate::services::recall::deliver::stamp_accessed(db, cid, &returned).await;
    }

    // === P4.D375 ===
    let window_census = window_chats.as_ref().map(|chats| {
        build_window_census(
            chats,
            since,
            until,
            window_memory_count,
            context.chat_id.as_deref(),
            memory_time_zone.as_deref(),
        )
    });
    // `windowCensus: {memoryCount, conversations: listed + more}` — an object,
    // so the `…Json` file-layer convention.
    let window_census_json = window_census.as_ref().map(|c| {
        serde_json::json!({
            "memoryCount": c.memory_count,
            "conversations": c.conversations.len() as i64 + c.more_conversations,
        })
        .to_string()
    });
    // === end P4.D375 ===

    // v4 `search-scriptorium-handler.ts:522` — the per-call INFO, after the sort
    // + slice, counting the LIMITED results; fields in v4's order (P4.114).
    let count = |pred: &dyn Fn(&Value) -> bool| results.iter().filter(|r| pred(r)).count();
    let of_source = |t: &'static str| move |r: &Value| r["sourceType"] == t;
    let of_tier = |t: &'static str| {
        move |r: &Value| r["sourceType"] == "knowledge" && r["metadata"]["knowledgeTier"] == t
    };
    tracing::info!(
        context = "search-scriptorium-handler",
        userId = context.user_id.as_str(),
        query = crate::jsstr::utf16_truncate(query, 100).as_str(),
        scope,
        defaultedSources = defaulted_sources,
        windowCensusJson = window_census_json.as_ref().map(tracing::field::display),
        totalFound = results.len(),
        memorySources = count(&of_source("memory")),
        conversationSources = count(&of_source("conversation")),
        documentSources = count(&of_source("document")),
        knowledgeSources = count(&of_source("knowledge")),
        knowledgeCharacter = count(&of_tier("character")),
        knowledgeGroup = count(&of_tier("group")),
        knowledgeProject = count(&of_tier("project")),
        knowledgeGlobal = count(&of_tier("global")),
        "Search scriptorium completed"
    );

    SearchOutput {
        success: true,
        total_found: results.len() as i64,
        results: Some(results),
        error: None,
        query: query.to_string(),
        window_census,
    }
}

/// The resolver + pool the document/knowledge branches share.
struct PoolContext {
    resolver: DocStoreUriResolver,
    pool: TieredMountPool,
    owns_character: bool,
    operator_store_ids: Vec<String>,
}

/// Build the URI resolver (always) and the tri-tier pool / operator store ids
/// (only when documents/knowledge are searched). One nested read closure.
fn build_pool_context(
    db: &Db,
    context: &SearchContext,
    need_pool: bool,
) -> Result<PoolContext, DbError> {
    db.read_main(|main| {
        db.read_mount_index(|mount| {
            let resolver = DocStoreUriResolver::build(mount);
            let mut pool = TieredMountPool {
                character_mount_point_id: None,
                participant_mount_point_ids: Vec::new(),
                group_mount_point_ids: Vec::new(),
                project_mount_point_ids: Vec::new(),
                global_mount_point_id: None,
            };
            let mut owns_character = false;
            let mut operator_store_ids = Vec::new();
            if need_pool {
                if let Some(prebuilt_pool) = &context.mount_pool {
                    pool = prebuilt_pool.clone();
                    // The caller vetted the cast before building the pool.
                    owns_character = true;
                    tracing::debug!(
                        participants = pool.participant_mount_point_ids.len(),
                        groups = pool.group_mount_point_ids.len(),
                        projects = pool.project_mount_point_ids.len(),
                        hasGlobal = pool.global_mount_point_id.is_some(),
                        "Search using pre-built mount pool"
                    );
                } else if context.operator_surface {
                    // P4.163: v4 `findEnabled()` (`search-scriptorium-handler.ts:
                    // 162`) is a fallback — the filter line and `[]` (v5 had
                    // failed the search).
                    operator_store_ids =
                        crate::db::fallback::find_by_filter_or_empty("doc_mount_points", || {
                            doc_mount_points::DocMountPointsRepository::new(mount)
                                .find_enabled_for_docedit()
                        })
                        .into_iter()
                        .map(|r| r.id)
                        .collect();
                } else {
                    // === P4.D376 ===
                    // The project tier is roster-gated (lib/projects/roster-access.ts;
                    // `roster: 'gated'`, inside the pool since v4 `0c47f4539`).
                    // Only this standard arm — the pre-built-pool and operator arms
                    // above are v4's untouched arms (v4 `9753d0eb2`).
                    pool = resolve_tiered_mount_pool(
                        main,
                        mount,
                        &TierContext {
                            user_id: Some(context.user_id.clone()),
                            character_id: context.character_id.clone(),
                            character_mount_point_id: None,
                            character_ids: None,
                            project_id: context.project_id.clone(),
                        },
                        &TierResolveOptions {
                            require_ownership: true,
                            ..TierResolveOptions::new(Roster::Gated, Enabled::Ignore)
                        },
                    );
                    // === end P4.D376 ===
                    owns_character = pool.character_mount_point_id.is_some();
                }
            }
            Ok(PoolContext {
                resolver,
                pool,
                owns_character,
                operator_store_ids,
            })
        })
    })
}

/// Embed the query with the branch's shared inputs (v4 `generateEmbeddingForUser(
/// query, userId, embeddingProfileId)`).
async fn embed<P: EmbeddingProvider>(
    provider: &P,
    query: &str,
    context: &SearchContext,
) -> Result<Vec<f32>, ()> {
    provider
        .generate_embedding_for_user(
            query,
            &context.user_id,
            context.embedding_profile_id.as_deref(),
            // lexical_text: None — a query embed; v4 passes no `lexicalText` here (C1-10).
            EmbeddingCallOptions::interactive(),
        )
        .await
        .map(|r| r.embedding)
        .map_err(|_| ())
}

/// v4 `scope` → the flatten scope for the documents pool.
fn flatten_scope(scope: &str) -> FlattenScope {
    match scope {
        "character" => FlattenScope::Character,
        "group" => FlattenScope::Group,
        "project" => FlattenScope::Project,
        _ => FlattenScope::All,
    }
}

/// v4 `KnowledgeBucket.tier`'s wire spelling.
fn knowledge_tier_str(tier: crate::db::reach::KnowledgeTier) -> &'static str {
    use crate::db::reach::KnowledgeTier as T;
    match tier {
        T::Character => "character",
        T::Group => "group",
        T::Project => "project",
        T::Global => "global",
    }
}

/// v4 `buildKnowledgeTiers`: one all-stores `global` tier on the operator
/// surface, otherwise [`bucket_mounts`] (the knowledge injector's bucketing).
fn build_knowledge_tiers(
    scope: &str,
    operator_wide: bool,
    prebuilt: bool,
    pool: &TieredMountPool,
    operator_store_ids: &[String],
) -> Vec<KnowledgeTier> {
    if operator_wide {
        return if operator_store_ids.is_empty() {
            Vec::new()
        } else {
            vec![KnowledgeTier {
                tier: "global",
                mount_point_ids: operator_store_ids.to_vec(),
                boost: LITERAL_BOOST_GLOBAL,
            }]
        };
    }
    // The shared bucketing (v4 `bucketMounts`): tiers nearest first, each with
    // its literal-phrase boost; a pre-built pool's cast vaults stand where the
    // character tier would. Its singleton guards are JS truthiness (an empty
    // id is no tier).
    let bucket_scope = match scope {
        "character" => BucketScope::Character,
        "group" => BucketScope::Group,
        "project" => BucketScope::Project,
        _ => BucketScope::All,
    };
    bucket_mounts(pool, bucket_scope, prebuilt)
        .into_iter()
        .map(|b| KnowledgeTier {
            tier: knowledge_tier_str(b.tier),
            mount_point_ids: b.mount_point_ids,
            boost: b.boost,
        })
        .collect()
}

// ---- result Value builders (v4 field order + JS-number rendering) ----------

fn result_value(
    content: &str,
    source_type: &str,
    score: f64,
    metadata: Map<String, Value>,
) -> Value {
    json!({
        "content": content,
        "sourceType": source_type,
        "relevanceScore": js_number_to_json(score),
        "metadata": Value::Object(metadata),
    })
}

/// memory metadata: `{ memoryId, summary?, importance, salience, createdAt,
/// source, occurredAt, narrativeTime, kind, conversationId? }` (episodic spine:
/// the three dated fields are ALWAYS present — v4 emits `?? null` / `?? 'semantic'`
/// — while `conversationId` is `chatId ?? undefined`, dropped when nullish).
fn memory_result(memory: &Value, score: f64, salience: f64) -> Value {
    let content = memory.get("content").and_then(Value::as_str).unwrap_or("");
    let mut md = Map::new();
    if let Some(v) = memory.get("id") {
        md.insert("memoryId".into(), v.clone());
    }
    if let Some(v) = memory.get("summary") {
        md.insert("summary".into(), v.clone());
    }
    // `importance: filterImportance(mr.memory)` — `reinforcedImportance ??
    // importance`, the value the band and the floor read (C1-A); absent from
    // the JSON only when both are.
    if let Some(v) = memory
        .get("reinforcedImportance")
        .filter(|v| !v.is_null())
        .or_else(|| memory.get("importance"))
    {
        md.insert("importance".into(), v.clone());
    }
    md.insert("salience".into(), js_number_to_json(salience));
    if let Some(v) = memory.get("createdAt") {
        md.insert("createdAt".into(), v.clone());
    }
    if let Some(v) = memory.get("source") {
        md.insert("source".into(), v.clone());
    }
    // `archivedLabel: memoryArchivedLabel(mr.memory)` — `undefined` (absent
    // from the JSON) for a hot row (v4 `d58548051`).
    if let Some(label) = memory_archived_label(memory) {
        md.insert("archivedLabel".into(), Value::String(label));
    }
    // `occurredAt: mr.memory.occurredAt ?? null` — nullish coalescing, so a
    // present-null column emits null (never dropped).
    md.insert(
        "occurredAt".into(),
        memory
            .get("occurredAt")
            .cloned()
            .filter(|v| !v.is_null())
            .unwrap_or(Value::Null),
    );
    md.insert(
        "narrativeTime".into(),
        memory
            .get("narrativeTime")
            .cloned()
            .filter(|v| !v.is_null())
            .unwrap_or(Value::Null),
    );
    md.insert(
        "kind".into(),
        memory
            .get("kind")
            .cloned()
            .filter(|v| !v.is_null())
            .unwrap_or_else(|| Value::String("semantic".to_string())),
    );
    // The memory's source conversation — drillable via read_conversation.
    // `conversationId: mr.memory.chatId ?? undefined` (dropped when nullish).
    if let Some(chat_id) = memory.get("chatId").filter(|v| !v.is_null()) {
        md.insert("conversationId".into(), chat_id.clone());
    }
    result_value(content, "memory", score, md)
}

/// v4 `memoryArchivedLabel(memory)` (`search-scriptorium-handler.ts:584-589`):
/// the label for a cold-tier memory in `search` results — `(archived —
/// superseded by <digest id>)` when a digest replaced it (a truthy
/// `supersededById`), `(archived)` otherwise. Hot rows (anything but
/// `tier === 'cold'`) get no label.
pub fn memory_archived_label(memory: &Value) -> Option<String> {
    if !crate::memory_injector::memory_is_cold(memory) {
        return None;
    }
    match memory
        .get("supersededById")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
    {
        Some(id) => Some(format!("(archived \u{2014} superseded by {id})")),
        None => Some("(archived)".to_string()),
    }
}

/// conversation metadata: `{ conversationId, interchangeIndex, conversationTitle,
/// participantNames }`.
fn conversation_result(cr: &crate::db::conversation_search::ConversationSearchResult) -> Value {
    let content = truncate_utf16(&cr.content, 500);
    let mut md = Map::new();
    md.insert("conversationId".into(), json!(cr.chat_id));
    md.insert(
        "interchangeIndex".into(),
        js_number_to_json(cr.interchange_index),
    );
    md.insert("conversationTitle".into(), json!(cr.conversation_title));
    md.insert("participantNames".into(), json!(cr.participant_names));
    result_value(&content, "conversation", cr.score, md)
}

/// knowledge metadata: `{ mountPointName, fileName, filePath, uri, chunkIndex,
/// headingContext?, knowledgeTier }`.
fn knowledge_result(kr: &DocumentSearchResult, uri: &str, tier: &str) -> Value {
    let content = truncate_utf16(&kr.content, 500);
    let mut md = Map::new();
    md.insert("mountPointName".into(), json!(kr.mount_point_name));
    md.insert("fileName".into(), json!(kr.file_name));
    md.insert("filePath".into(), json!(kr.relative_path));
    md.insert("uri".into(), json!(uri));
    md.insert("chunkIndex".into(), Value::from(kr.chunk_index));
    if let Some(h) = &kr.heading_context {
        md.insert("headingContext".into(), json!(h));
    }
    md.insert("knowledgeTier".into(), json!(tier));
    result_value(&content, "knowledge", kr.score, md)
}

/// document metadata: `{ mountPointName, fileName, filePath, uri, chunkIndex,
/// headingContext? }`.
fn document_result(dr: &DocumentSearchResult, uri: &str) -> Value {
    let content = truncate_utf16(&dr.content, 500);
    let mut md = Map::new();
    md.insert("mountPointName".into(), json!(dr.mount_point_name));
    md.insert("fileName".into(), json!(dr.file_name));
    md.insert("filePath".into(), json!(dr.relative_path));
    md.insert("uri".into(), json!(uri));
    md.insert("chunkIndex".into(), Value::from(dr.chunk_index));
    if let Some(h) = &dr.heading_context {
        md.insert("headingContext".into(), json!(h));
    }
    result_value(&content, "document", dr.score, md)
}

// === P4.D375 === the window census + the honest-empty formatter (v4 4.12.1
// `6121f59b9`, `search-scriptorium-handler.ts:600-766`, `search-scriptorium-
// tool.ts:132-168`).

/// How many of the window's conversations the census lists by name (v4
/// `CENSUS_MAX_CONVERSATIONS`).
const CENSUS_MAX_CONVERSATIONS: usize = 10;

/// v4 `SearchWindowCensus` — key order as declared (`since, until,
/// memoryCount, conversations, moreConversations`). `memoryCount` is `null`
/// when no memory count was taken (no owned character, the operator surface).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SearchWindowCensus {
    pub since: Option<String>,
    pub until: Option<String>,
    pub memory_count: Option<i64>,
    pub conversations: Vec<WindowCensusConversation>,
    pub more_conversations: i64,
}

/// One listed conversation of a [`SearchWindowCensus`] (`id, title,
/// firstMessageAt, lastMessageAt, date`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WindowCensusConversation {
    pub id: String,
    pub title: String,
    pub first_message_at: String,
    pub last_message_at: Option<String>,
    pub date: String,
}

/// v4 `buildWindowCensus` — the window census from the period's chats (newest
/// first): the current chat left out, the first ten listed, the rest counted.
/// A chat's `date` is its `createdAt` on the memory calendar (`zonedDateStamp`)
/// or, with no zone, the UTC date (`toISOString().slice(0, 10)` — reachable
/// only through a direct call: the handler always has a zone when it has a
/// window); an unparseable `createdAt` is sliced as given.
pub fn build_window_census(
    window_chats: &[Value],
    since: Option<&str>,
    until: Option<&str>,
    memory_count: Option<i64>,
    exclude_chat_id: Option<&str>,
    time_zone: Option<&str>,
) -> SearchWindowCensus {
    let local_date = |iso: &str| -> String {
        let ms = crate::memory_scoring::date_ms(iso);
        if !ms.is_finite() {
            return crate::jsstr::utf16_truncate(iso, 10);
        }
        match time_zone.filter(|tz| !tz.is_empty()) {
            Some(tz) => crate::memory_time::calendar::zoned_date_stamp(ms, tz),
            None => crate::memory_time::calendar::js_iso_string(ms)
                .map(|s| crate::jsstr::utf16_truncate(&s, 10))
                .unwrap_or_default(),
        }
    };
    let others: Vec<&Value> = window_chats
        .iter()
        .filter(|c| c.get("id").and_then(Value::as_str) != exclude_chat_id)
        .collect();
    let conversations = others
        .iter()
        .take(CENSUS_MAX_CONVERSATIONS)
        .map(|c| {
            let created = c.get("createdAt").and_then(Value::as_str).unwrap_or("");
            WindowCensusConversation {
                id: c
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                title: c
                    .get("title")
                    .and_then(Value::as_str)
                    .filter(|t| !t.is_empty())
                    .unwrap_or("Untitled")
                    .to_string(),
                first_message_at: created.to_string(),
                last_message_at: c
                    .get("lastMessageAt")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                date: local_date(created),
            }
        })
        .collect();
    SearchWindowCensus {
        since: since.map(str::to_string),
        until: until.map(str::to_string),
        memory_count,
        conversations,
        more_conversations: others.len().saturating_sub(CENSUS_MAX_CONVERSATIONS) as i64,
    }
}

/// v4 `censusDate` — a bound as the caller gave it, shown as a calendar date.
fn census_date(bound: Option<&str>, fallback: &str) -> String {
    match bound.filter(|b| !b.is_empty()) {
        Some(b) => crate::jsstr::utf16_truncate(b, 10),
        None => fallback.to_string(),
    }
}

/// v4 `censusPeriod` — "between X and Y" for a census window, in plain words.
fn census_period(census: &SearchWindowCensus) -> String {
    let since = census.since.as_deref().filter(|s| !s.is_empty());
    let until = census.until.as_deref().filter(|s| !s.is_empty());
    match (since, until) {
        (Some(s), Some(u)) => {
            if crate::jsstr::utf16_truncate(s, 10) == crate::jsstr::utf16_truncate(u, 10) {
                format!("on {}", census_date(since, ""))
            } else {
                format!(
                    "between {} and {}",
                    census_date(since, ""),
                    census_date(until, "")
                )
            }
        }
        (Some(_), None) => format!("between {} and today", census_date(since, "")),
        _ => format!("up to {}", census_date(until, "today")),
    }
}

/// v4 `censusConversationLines` — one line each, with the overflow.
fn census_conversation_lines(census: &SearchWindowCensus) -> Vec<String> {
    let mut lines: Vec<String> = census
        .conversations
        .iter()
        .map(|c| format!("- {} ({}) \u{2014} `{}`", c.title, c.date, c.id))
        .collect();
    if census.more_conversations > 0 {
        lines.push(format!("- \u{2026}and {} more", census.more_conversations));
    }
    lines
}

/// v4 `formatEmptySearch` — what an empty search means, given the window
/// census (or none). Every empty search answers one of these: v4 4.12.1
/// deleted `No relevant results found.` (B.6).
fn format_empty_search(query: Option<&str>, census: Option<&SearchWindowCensus>) -> String {
    let quoted = match query.filter(|q| !q.is_empty()) {
        Some(q) => format!("\"{q}\""),
        None => "your search".to_string(),
    };
    let Some(census) = census else {
        return format!(
            "Nothing matched {quoted}. An empty search is not proof that nothing happened: try other words, or pass since/until if you are asking about a particular time."
        );
    };
    let period = census_period(census);
    let memories = census.memory_count.unwrap_or(0);
    let has_conversations = !census.conversations.is_empty() || census.more_conversations > 0;
    if memories == 0 && !has_conversations {
        return match census.memory_count {
            None => format!("There are no conversations {period}."),
            Some(_) => format!(
                "You hold no memories dated {period}, and took part in no conversations in that period."
            ),
        };
    }
    let mut lines = vec![format!(
        "Nothing matched {quoted} \u{2014} but that is not the same as having no record of the period."
    )];
    let memory_clause = census.memory_count.map(|_| {
        format!(
            "you hold {memories} {}",
            if memories == 1 { "memory" } else { "memories" }
        )
    });
    let conversation_clause = if census.memory_count.is_none() {
        "these conversations took place"
    } else {
        "you took part in these conversations"
    };
    // `period.charAt(0).toUpperCase() + period.slice(1)` — the period always
    // opens with ASCII `o` / `b` / `u`.
    let head = {
        let mut chars = period.chars();
        match chars.next() {
            Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
            None => String::new(),
        }
    };
    if has_conversations {
        lines.push(format!(
            "{head} {}{conversation_clause}:",
            memory_clause
                .as_ref()
                .map(|m| format!("{m}, and "))
                .unwrap_or_default()
        ));
        lines.extend(census_conversation_lines(census));
        lines.push("Read one with read_conversation, or search again with broader words. The date range already narrows the search; the query does not need to name the time.".to_string());
    } else {
        lines.push(format!("{head} {}.", memory_clause.unwrap_or_default()));
        lines.push("Search again with broader words. The date range already narrows the search; the query does not need to name the time.".to_string());
    }
    lines.join("\n")
}

/// The executor's `opts` for [`format_search_scriptorium_results`] (v4 `{
/// query, windowCensus? }`).
pub struct FormatOptions<'a> {
    pub query: &'a str,
    pub window_census: Option<&'a SearchWindowCensus>,
}
// === end P4.D375 ===

/// v4 `formatSearchScriptoriumResults(results, opts?)`. Operates on the built
/// result `Value`s. With `opts.window_census`, a result list gains a footer
/// naming the period's conversations, and an empty result says whether
/// nothing matched the words or nothing happened in the period. Without opts,
/// results print unchanged — and an empty list still answers the honest-empty
/// text (`Nothing matched your search. …`; v4 4.12.1 deleted `No relevant
/// results found.`).
pub fn format_search_scriptorium_results(
    results: &[Value],
    opts: Option<&FormatOptions>,
) -> String {
    if results.is_empty() {
        return format_empty_search(opts.map(|o| o.query), opts.and_then(|o| o.window_census));
    }
    let formatted: Vec<String> = results
        .iter()
        .enumerate()
        .map(|(index, result)| format_one(index, result))
        .collect();
    let body = format!(
        "Found {} results:\n\n{}",
        results.len(),
        formatted.join("\n\n")
    );
    // P4.D375: the footer — only when the census LISTS a conversation (the
    // overflow alone does not trigger it).
    let Some(census) = opts
        .and_then(|o| o.window_census)
        .filter(|c| !c.conversations.is_empty())
    else {
        return body;
    };
    let mut footer = vec![if census.memory_count.is_none() {
        "Conversations during this period:".to_string()
    } else {
        "Conversations you took part in during this period:".to_string()
    }];
    footer.extend(census_conversation_lines(census));
    footer.push("Read any of them with read_conversation.".to_string());
    format!("{body}\n\n{}", footer.join("\n"))
}

fn format_one(index: usize, result: &Value) -> String {
    let n = index + 1;
    let source_type = result
        .get("sourceType")
        .and_then(Value::as_str)
        .unwrap_or("");
    let content = result.get("content").and_then(Value::as_str).unwrap_or("");
    let score = result
        .get("relevanceScore")
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    let pct = to_fixed(score * 100.0, 0); // v4 `(relevanceScore * 100).toFixed(0)`
    let md = result.get("metadata").cloned().unwrap_or(Value::Null);
    match source_type {
        "memory" => {
            let importance = md.get("importance").and_then(Value::as_f64).unwrap_or(0.0);
            let importance_label = match importance_band(importance) {
                ImportanceBand::High => "High",
                ImportanceBand::Medium => "Medium",
                ImportanceBand::Low => "Low",
            };
            let summary = md
                .get("summary")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .unwrap_or("No summary");
            // Episodic spine: surface event time + source conversation so the
            // model can confirm "when" and drill into the transcript. Truthy
            // gates (v4 `if (metadata.occurredAt)`) — null/empty render nothing.
            let mut when_parts: Vec<String> = Vec::new();
            if let Some(occurred) = md
                .get("occurredAt")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
            {
                when_parts.push(format!(
                    "When: {}",
                    crate::jsstr::utf16_truncate(occurred, 10)
                ));
            }
            if let Some(nt) = md
                .get("narrativeTime")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
            {
                when_parts.push(format!("Story time: {nt}"));
            }
            let when_line = if when_parts.is_empty() {
                String::new()
            } else {
                format!("\n{}", when_parts.join(" · "))
            };
            let source_line = match md
                .get("conversationId")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
            {
                Some(cid) => {
                    format!("\nSource conversation: {cid} (readable via read_conversation)")
                }
                None => String::new(),
            };
            // v4: `result.metadata.archivedLabel ? ` ${label}` : ''` (truthy).
            let archived = md
                .get("archivedLabel")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .map(|l| format!(" {l}"))
                .unwrap_or_default();
            format!(
                "[Result {n} - Memory{archived}] (Importance: {importance_label}, Relevance: {pct}%)\nSummary: {summary}{when_line}{source_line}\nDetails: {content}"
            )
        }
        "conversation" => {
            let title = md
                .get("conversationTitle")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .unwrap_or("Untitled");
            let conv_id = md
                .get("conversationId")
                .and_then(Value::as_str)
                .unwrap_or("");
            let interchange = md
                .get("interchangeIndex")
                .map(render_js_number)
                .unwrap_or_default();
            let participants = md
                .get("participantNames")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(Value::as_str)
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "Unknown".to_string());
            format!(
                "[Result {n} - Conversation] (Relevance: {pct}%, Chat: {title})\nConversation ID: {conv_id}\nInterchange: {interchange}\nParticipants: {participants}\nContent: {content}"
            )
        }
        "knowledge" => {
            let heading = md
                .get("headingContext")
                .and_then(Value::as_str)
                .map(|h| format!(", Section: {h}"))
                .unwrap_or_default();
            let path = md
                .get("filePath")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .or_else(|| {
                    md.get("fileName")
                        .and_then(Value::as_str)
                        .filter(|s| !s.is_empty())
                })
                .unwrap_or("Unknown");
            let mount = md
                .get("mountPointName")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .unwrap_or("Unknown");
            let tier = md
                .get("knowledgeTier")
                .and_then(Value::as_str)
                .map(|t| format!(" {}", capitalize(t)))
                .unwrap_or_default();
            format!(
                "[Result {n} -{tier} Knowledge] (Relevance: {pct}%, Source: {mount}{heading})\nFile: {path}\nContent: {content}\nRe-read with: doc_read_file(scope=document_store, mount_point=\"{mount}\", path=\"{path}\")"
            )
        }
        _ => {
            let heading = md
                .get("headingContext")
                .and_then(Value::as_str)
                .map(|h| format!(", Section: {h}"))
                .unwrap_or_default();
            let mount = md
                .get("mountPointName")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .unwrap_or("Unknown");
            let path = md
                .get("filePath")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .or_else(|| {
                    md.get("fileName")
                        .and_then(Value::as_str)
                        .filter(|s| !s.is_empty())
                })
                .unwrap_or("Unknown");
            format!(
                "[Result {n} - Document] (Relevance: {pct}%, Source: {mount}{heading})\nFile: {path}\nContent: {content}"
            )
        }
    }
}

/// `${value}` on a JS number — an integer-valued number renders bare.
fn render_js_number(v: &Value) -> String {
    match v {
        Value::Number(n) => n.to_string(),
        other => other.as_str().unwrap_or("").to_string(),
    }
}

/// v4 `s.charAt(0).toUpperCase() + s.slice(1)` — ASCII-safe capitalize of the tier.
fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// v4's `buildKnowledgeTiers` (`search-scriptorium-handler.ts:234-249` at
    /// `624d0d25c`) is the shared `bucketMounts` (`reach.ts:399`) off the
    /// operator surface, whose singleton guards are JS truthiness: an EMPTY
    /// character / global id is no tier. v5's copy pushed a tier for `""`.
    #[test]
    fn knowledge_tiers_are_the_shared_bucketing_and_drop_empty_singletons() {
        use crate::literal_boost::{
            LITERAL_BOOST_CHARACTER, LITERAL_BOOST_GROUP, LITERAL_BOOST_PROJECT,
        };
        let pool = TieredMountPool {
            character_mount_point_id: Some(String::new()),
            participant_mount_point_ids: vec!["p-1".into()],
            group_mount_point_ids: vec!["g-1".into()],
            project_mount_point_ids: vec!["pr-1".into()],
            global_mount_point_id: Some(String::new()),
        };
        let tiers = build_knowledge_tiers("all", false, true, &pool, &[]);
        let shape: Vec<(&str, Vec<String>, f64)> = tiers
            .iter()
            .map(|t| (t.tier, t.mount_point_ids.clone(), t.boost))
            .collect();
        assert_eq!(
            shape,
            vec![
                (
                    "character",
                    vec!["p-1".to_string()],
                    LITERAL_BOOST_CHARACTER
                ),
                ("group", vec!["g-1".to_string()], LITERAL_BOOST_GROUP),
                ("project", vec!["pr-1".to_string()], LITERAL_BOOST_PROJECT),
            ]
        );

        // Every scope, both pool shapes, against `bucket_mounts` itself.
        let full = TieredMountPool {
            character_mount_point_id: Some("c-1".into()),
            participant_mount_point_ids: vec!["p-1".into()],
            group_mount_point_ids: vec!["g-1".into(), "g-2".into()],
            project_mount_point_ids: vec!["pr-1".into()],
            global_mount_point_id: Some("gl".into()),
        };
        for (scope, bucket_scope) in [
            ("all", BucketScope::All),
            ("character", BucketScope::Character),
            ("group", BucketScope::Group),
            ("project", BucketScope::Project),
        ] {
            for prebuilt in [false, true] {
                let ours: Vec<(&str, Vec<String>, f64)> =
                    build_knowledge_tiers(scope, false, prebuilt, &full, &[])
                        .into_iter()
                        .map(|t| (t.tier, t.mount_point_ids, t.boost))
                        .collect();
                let shared: Vec<(&str, Vec<String>, f64)> =
                    bucket_mounts(&full, bucket_scope, prebuilt)
                        .into_iter()
                        .map(|b| (knowledge_tier_str(b.tier), b.mount_point_ids, b.boost))
                        .collect();
                assert_eq!(ours, shared, "scope={scope} prebuilt={prebuilt}");
            }
        }
    }

    /// The smalls unification (P4.114 review): v4's outer `catch`
    /// (`search-scriptorium-handler.ts:544-549`) logs ERROR `Search scriptorium
    /// tool execution failed` with `{ context, userId, characterId }` plus the
    /// error; v5's counterpart arm — the pool context's read failing — used to
    /// return the failure silently. Driven by an instance opened WITHOUT its
    /// mount-index partition, so `build_pool_context` answers `Err`.
    #[test]
    fn an_outer_failure_logs_v4s_execution_failed_error() {
        const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("data");
        std::fs::create_dir_all(&data).unwrap();
        crate::services::provisioning::provision_fresh_instance(&data, PEPPER).unwrap();
        let db = Db::open(
            crate::db::runtime::DbPaths {
                main: data.join("quilltap.db"),
                mount_index: None,
                llm_logs: None,
            },
            PEPPER,
        )
        .unwrap();
        let context = SearchContext {
            user_id: crate::api::SINGLE_USER_ID.to_string(),
            character_id: Some("c0000000-0000-4000-8000-00000000c001".to_string()),
            embedding_profile_id: None,
            project_id: None,
            operator_surface: false,
            mount_pool: None,
            memory_zone_fallback: "UTC".to_string(),
            chat_id: None,
        };
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let (out, lines) = crate::test_support::captured_with(|| {
            rt.block_on(execute_search_scriptorium(
                &db,
                &crate::model::embedding::NoEmbeddingProvider,
                &context,
                &json!({ "query": "the quay" }),
                0.0,
            ))
        });
        assert!(!out.success, "the outer failure answers failure: {out:?}");
        let errors: Vec<&String> = lines
            .iter()
            .filter(|l| l.contains("Search scriptorium tool execution failed"))
            .collect();
        assert_eq!(errors.len(), 1, "exactly one ERROR: {lines:#?}");
        let line = errors[0];
        assert!(
            line.starts_with("ERROR quilltap_core::tools::search"),
            "{line}"
        );
        let at = |k: &str| line.find(k).unwrap_or_else(|| panic!("{k} in {line}"));
        assert!(
            at(" context=search-scriptorium-handler") < at(" userId=")
                && at(" userId=") < at(" characterId=c0000000-0000-4000-8000-00000000c001")
                && at(" characterId=") < at(" error="),
            "v4's field order: {line}"
        );
    }
}

// === P4.D375 ===
#[cfg(test)]
mod p4d375_format_tests {
    //! v4 4.12.1's honest-empty texts and the census footer (B.3-B.5); the
    //! differential is `search_tools_equivalence`'s `p4d375_format_*` rows.
    use super::*;

    fn census(memory_count: Option<i64>, n: usize, more: i64) -> SearchWindowCensus {
        SearchWindowCensus {
            since: Some("2025-06-01".into()),
            until: Some("2025-06-30".into()),
            memory_count,
            conversations: (0..n)
                .map(|i| WindowCensusConversation {
                    id: format!("c{i}"),
                    title: format!("Chat {i}"),
                    first_message_at: "2025-06-02T10:00:00.000Z".into(),
                    last_message_at: None,
                    date: "2025-06-02".into(),
                })
                .collect(),
            more_conversations: more,
        }
    }

    fn fmt(results: &[Value], query: &str, c: Option<&SearchWindowCensus>) -> String {
        format_search_scriptorium_results(
            results,
            Some(&FormatOptions {
                query,
                window_census: c,
            }),
        )
    }

    #[test]
    fn the_six_empty_texts() {
        assert_eq!(
            format_search_scriptorium_results(&[], None),
            "Nothing matched your search. An empty search is not proof that nothing happened: try other words, or pass since/until if you are asking about a particular time."
        );
        assert!(fmt(&[], "q", None).starts_with("Nothing matched \"q\". An empty search"));
        assert_eq!(
            fmt(&[], "q", Some(&census(None, 0, 0))),
            "There are no conversations between 2025-06-01 and 2025-06-30."
        );
        assert_eq!(
            fmt(&[], "q", Some(&census(Some(0), 0, 0))),
            "You hold no memories dated between 2025-06-01 and 2025-06-30, and took part in no conversations in that period."
        );
        let four = fmt(&[], "q", Some(&census(Some(0), 1, 2)));
        assert_eq!(
            four,
            "Nothing matched \"q\" \u{2014} but that is not the same as having no record of the period.\nBetween 2025-06-01 and 2025-06-30 you hold 0 memories, and you took part in these conversations:\n- Chat 0 (2025-06-02) \u{2014} `c0`\n- \u{2026}and 2 more\nRead one with read_conversation, or search again with broader words. The date range already narrows the search; the query does not need to name the time."
        );
        assert!(fmt(&[], "q", Some(&census(None, 1, 0)))
            .contains("\nBetween 2025-06-01 and 2025-06-30 these conversations took place:\n"));
        assert!(fmt(&[], "q", Some(&census(Some(1), 0, 0)))
            .ends_with("Between 2025-06-01 and 2025-06-30 you hold 1 memory.\nSearch again with broader words. The date range already narrows the search; the query does not need to name the time."));
    }

    #[test]
    fn periods_and_the_footer() {
        let mut same = census(None, 0, 0);
        same.since = Some("2025-12-01".into());
        same.until = Some("2025-12-01T23:00:00Z".into());
        assert_eq!(
            fmt(&[], "q", Some(&same)),
            "There are no conversations on 2025-12-01."
        );
        let mut since_only = census(None, 0, 0);
        since_only.until = None;
        assert_eq!(
            fmt(&[], "q", Some(&since_only)),
            "There are no conversations between 2025-06-01 and today."
        );
        let mut until_only = census(None, 0, 0);
        until_only.since = None;
        assert_eq!(
            fmt(&[], "q", Some(&until_only)),
            "There are no conversations up to 2025-06-30."
        );

        let hit = serde_json::json!({"content": "x", "sourceType": "document", "relevanceScore": 0.5, "metadata": {}});
        let body = format_search_scriptorium_results(std::slice::from_ref(&hit), None);
        assert!(fmt(std::slice::from_ref(&hit), "q", Some(&census(Some(2), 1, 0)))
            .ends_with("\n\nConversations you took part in during this period:\n- Chat 0 (2025-06-02) \u{2014} `c0`\nRead any of them with read_conversation."));
        assert!(
            fmt(std::slice::from_ref(&hit), "q", Some(&census(None, 1, 0)))
                .contains("\n\nConversations during this period:\n")
        );
        // The overflow alone triggers no footer.
        assert_eq!(
            fmt(std::slice::from_ref(&hit), "q", Some(&census(None, 0, 3))),
            body
        );
    }

    #[test]
    fn the_census_utc_arm_and_the_exclusion() {
        let chats = vec![
            serde_json::json!({"id": "a", "title": "", "createdAt": "2025-06-02T23:30:00.000Z"}),
            serde_json::json!({"id": "b", "title": "B", "createdAt": "nope", "lastMessageAt": "2025-06-03T00:00:00.000Z"}),
        ];
        let c = build_window_census(&chats, Some("2025-06-01"), None, None, Some("zzz"), None);
        assert_eq!(c.conversations[0].title, "Untitled");
        assert_eq!(c.conversations[0].date, "2025-06-02");
        assert_eq!(c.conversations[1].date, "nope");
        let c = build_window_census(
            &chats,
            None,
            None,
            Some(1),
            Some("a"),
            Some("America/Chicago"),
        );
        assert_eq!(c.conversations.len(), 1);
        assert_eq!(
            serde_json::to_string(&c).unwrap(),
            r#"{"since":null,"until":null,"memoryCount":1,"conversations":[{"id":"b","title":"B","firstMessageAt":"nope","lastMessageAt":"2025-06-03T00:00:00.000Z","date":"nope"}],"moreConversations":0}"#
        );
    }
}
// === end P4.D375 ===
