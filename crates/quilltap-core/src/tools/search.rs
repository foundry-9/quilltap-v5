//! Port of v4's `search` tool — the Scriptorium unified search
//! (`lib/tools/handlers/search-scriptorium-handler.ts` +
//! `lib/tools/search-scriptorium-tool.ts`).
//!
//! Searches across four sources — `memories` (the ported
//! [`search_memories_semantic`](crate::services::memory_service::search_memories_semantic)),
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
//! **Not purely read-only:** the memory branch's `search_memories_semantic` bumps
//! `lastAccessedAt` on matched memories (v4's `updateAccessTimeBulk`, non-fatal).
//! The returned metadata is computed before the bump; the differential isolates
//! each case on a fresh fixture copy so the bump never contaminates a later case.

use serde::Serialize;
use serde_json::{json, Map, Value};

use crate::db::conversation_search::{search_conversation_chunks, ConversationSearchOptions};
use crate::db::runtime::Db;
use crate::db::tiered_mount_pool::{
    flatten_tier_pool, resolve_tiered_mount_pool, FlattenScope, TierContext, TierResolveOptions,
    TieredMountPool,
};
use crate::db::{characters_read, doc_mount_points, js_number_to_json, DbError};
use crate::doc_edit::uri_producers::DocStoreUriResolver;
use crate::jsnum::to_fixed;
use crate::literal_boost::{
    LITERAL_BOOST_CHARACTER, LITERAL_BOOST_GLOBAL, LITERAL_BOOST_GROUP, LITERAL_BOOST_PROJECT,
};
use crate::model::embedding::{EmbeddingPriority, EmbeddingProvider};
use crate::services::knowledge_injector::{
    search_document_chunks, DocumentSearchOptions, DocumentSearchResult,
};
use crate::services::memory_service::{search_memories_semantic, SemanticSearchOptions};
use crate::tools::llm_number::llm_number;

use super::help_search::truncate_utf16;

/// v4 `SearchScriptoriumToolContext`.
pub struct SearchContext {
    pub user_id: String,
    /// Absent on the operator surface (Brahma Console).
    pub character_id: Option<String>,
    pub embedding_profile_id: Option<String>,
    pub project_id: Option<String>,
    pub operator_surface: bool,
}

/// v4 `SearchScriptoriumToolOutput` — `{ success, results?, error?, totalFound,
/// query }`. `results` is pre-built as v4-exact `Value`s (per-source metadata order
/// + JS-number score rendering).
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
}

impl SearchOutput {
    fn failure(error: impl Into<String>, query: impl Into<String>) -> Self {
        SearchOutput {
            success: false,
            results: None,
            error: Some(error.into()),
            total_found: 0,
            query: query.into(),
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
    true
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
        return SearchOutput::failure(
            "Invalid input: query is required and must be a non-empty string",
            "",
        );
    }

    let query = args.get("query").and_then(Value::as_str).unwrap_or("");
    let sources: Vec<String> = match args.get("sources").and_then(Value::as_array) {
        Some(arr) => arr
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect(),
        None => vec![
            "memories".into(),
            "conversations".into(),
            "documents".into(),
            "knowledge".into(),
        ],
    };
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
        .unwrap_or(0.0);

    let operator_wide = context.operator_surface;
    let search_memories =
        sources.iter().any(|s| s == "memories") && !operator_wide && context.character_id.is_some();
    let search_conversations = sources.iter().any(|s| s == "conversations");
    let search_documents = sources.iter().any(|s| s == "documents");
    let search_knowledge = sources.iter().any(|s| s == "knowledge");

    // Build the URI resolver (always) + the tri-tier pool / operator ids (only when
    // a document/knowledge source is requested). Any DB error here → outer failure.
    let pool_ctx = match build_pool_context(db, context, search_documents || search_knowledge) {
        Ok(pc) => pc,
        Err(e) => return SearchOutput::failure(e.to_string(), query),
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
            if let Ok(true) = owned {
                let opts = SemanticSearchOptions {
                    user_id: context.user_id.clone(),
                    embedding_profile_id: context.embedding_profile_id.clone(),
                    limit: Some(limit),
                    min_score: None,
                    min_importance: Some(min_importance),
                    source: None,
                    about_character_id: None,
                    apply_literal_phrase_boost: true,
                    now_ms,
                    ..Default::default()
                };
                if let Ok(mems) = search_memories_semantic(db, provider, cid, query, &opts).await {
                    for mr in mems {
                        results.push((
                            mr.score,
                            memory_result(&mr.memory, mr.score, mr.effective_weight),
                        ));
                    }
                }
            }
        }
    }

    // --- Conversations -----------------------------------------------------
    if search_conversations {
        if let Ok(emb) = embed(provider, query, context).await {
            let opts = ConversationSearchOptions {
                character_id: context.character_id.clone(),
                user_id: Some(context.user_id.clone()),
                limit: Some(limit),
                min_score: Some(0.3),
                query: Some(query.to_string()),
                apply_literal_phrase_boost: true,
            };
            if let Ok(convs) = db.read_main(|c| search_conversation_chunks(c, &emb, &opts)) {
                for cr in convs {
                    results.push((cr.score, conversation_result(&cr)));
                }
            }
        }
    }

    // --- Documents (deferred until after knowledge dedup) ------------------
    let mut document_rows = Vec::new();
    if search_documents {
        let documents_pool = if operator_wide {
            operator_store_ids.clone()
        } else {
            flatten_tier_pool(&pool, flatten_scope(scope), false)
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
        let tiers = build_knowledge_tiers(scope, operator_wide, &pool, &operator_store_ids);
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

    SearchOutput {
        success: true,
        total_found: results.len() as i64,
        results: Some(results),
        error: None,
        query: query.to_string(),
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
            let resolver = DocStoreUriResolver::build(main, mount, context.character_id.as_deref());
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
                if context.operator_surface {
                    operator_store_ids = doc_mount_points::DocMountPointsRepository::new(mount)
                        .find_enabled_for_docedit()?
                        .into_iter()
                        .map(|r| r.id)
                        .collect();
                } else {
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
                            include_participants: false,
                        },
                    );
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
            EmbeddingPriority::Interactive,
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

/// v4 `buildKnowledgeTiers`.
fn build_knowledge_tiers(
    scope: &str,
    operator_wide: bool,
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
    let mut tiers = Vec::new();
    let want_character = scope == "all" || scope == "character";
    let want_group = scope == "all" || scope == "group";
    let want_project = scope == "all" || scope == "project";
    let want_global = scope == "all";
    if want_character {
        if let Some(id) = &pool.character_mount_point_id {
            tiers.push(KnowledgeTier {
                tier: "character",
                mount_point_ids: vec![id.clone()],
                boost: LITERAL_BOOST_CHARACTER,
            });
        }
    }
    if want_group && !pool.group_mount_point_ids.is_empty() {
        tiers.push(KnowledgeTier {
            tier: "group",
            mount_point_ids: pool.group_mount_point_ids.clone(),
            boost: LITERAL_BOOST_GROUP,
        });
    }
    if want_project && !pool.project_mount_point_ids.is_empty() {
        tiers.push(KnowledgeTier {
            tier: "project",
            mount_point_ids: pool.project_mount_point_ids.clone(),
            boost: LITERAL_BOOST_PROJECT,
        });
    }
    if want_global {
        if let Some(id) = &pool.global_mount_point_id {
            tiers.push(KnowledgeTier {
                tier: "global",
                mount_point_ids: vec![id.clone()],
                boost: LITERAL_BOOST_GLOBAL,
            });
        }
    }
    tiers
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

/// memory metadata: `{ memoryId, summary?, importance, effectiveWeight, createdAt,
/// source }`.
fn memory_result(memory: &Value, score: f64, effective_weight: f64) -> Value {
    let content = memory.get("content").and_then(Value::as_str).unwrap_or("");
    let mut md = Map::new();
    if let Some(v) = memory.get("id") {
        md.insert("memoryId".into(), v.clone());
    }
    if let Some(v) = memory.get("summary") {
        md.insert("summary".into(), v.clone());
    }
    if let Some(v) = memory.get("importance") {
        md.insert("importance".into(), v.clone());
    }
    md.insert(
        "effectiveWeight".into(),
        js_number_to_json(effective_weight),
    );
    if let Some(v) = memory.get("createdAt") {
        md.insert("createdAt".into(), v.clone());
    }
    if let Some(v) = memory.get("source") {
        md.insert("source".into(), v.clone());
    }
    result_value(content, "memory", score, md)
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

/// v4 `formatSearchScriptoriumResults`. Operates on the built result `Value`s.
pub fn format_search_scriptorium_results(results: &[Value]) -> String {
    if results.is_empty() {
        return "No relevant results found.".to_string();
    }
    let formatted: Vec<String> = results
        .iter()
        .enumerate()
        .map(|(index, result)| format_one(index, result))
        .collect();
    format!(
        "Found {} results:\n\n{}",
        results.len(),
        formatted.join("\n\n")
    )
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
            let importance_label = if importance >= 0.7 {
                "High"
            } else if importance >= 0.4 {
                "Medium"
            } else {
                "Low"
            };
            let summary = md
                .get("summary")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .unwrap_or("No summary");
            format!(
                "[Result {n} - Memory] (Importance: {importance_label}, Relevance: {pct}%)\nSummary: {summary}\nDetails: {content}"
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
