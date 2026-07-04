//! The per-turn knowledge injector (Phase-3 Unit-3 wave 3) — v4
//! `retrieveKnowledgeForTurn` (`lib/chat/context/knowledge-injector.ts`).
//!
//! Per-turn retrieval of relevant files from the Knowledge/ scopes available to the
//! responding character: their own vault (character tier), the groups they belong
//! to, the active chat-project's linked mounts, and the instance-wide Quilltap
//! General mount. Each tier runs an independent scoped search with a distinct
//! literal-boost fraction so a verbatim hit in the character's own vault outranks
//! the same hit in a looser tier. Hits are merged, deduped by chunkId (best score
//! wins), and greedy-packed by score into a single budget-bounded string. Each
//! entry is either **inline** (full file body ≤ inline threshold, frontmatter tags
//! surfaced, `doc_read_file` footer) or a **pointer** (teaser + `doc_read_file`
//! template) — used when the body is too large, the file is a derived blob, or the
//! inline form would overflow the remaining budget.
//!
//! Read-only: it embeds the query once through the injected [`EmbeddingProvider`]
//! seam, then does mount-index SELECTs (chunk search + file/document lookups). No
//! writes. Errors are swallowed into an empty result / dropped candidates, exactly
//! as v4 does — never throw mid-turn.
//!
//! ## Model / registry seams
//!
//! * The embedding call is the [`EmbeddingProvider`] seam (the same one the memory
//!   gate/processor use); a failure returns the empty result.
//! * `estimateTokens`' provider→chars-per-token resolution is a registry seam in v4;
//!   it is injected here as [`KnowledgeRetrievalParams::chars_per_token`] (as in
//!   [`crate::token_estimation`]).
//!
//! ## Tracked deferrals
//!
//! * The `projectId` / all-enabled-mounts fallbacks in `searchDocumentChunks` are
//!   ported ([`document_search`]) but the injector always passes explicit
//!   `mount_point_ids`, so those branches are exercised only by the (out-of-scope)
//!   tool path.

mod document_search;

pub use document_search::{DocumentSearchOptions, DocumentSearchResult};

/// The `qtap://` URI codec's canonical home is [`crate::doc_edit::qtap_uri`]; this
/// re-export keeps the historical `knowledge_injector::qtap_uri::…` path (the
/// `self_inventory` tool + the RAG renderer below) resolving after the W4.1d
/// batch-3a unification hoisted the producers out of this service.
pub use crate::doc_edit::qtap_uri;

use crate::db::runtime::Db;
use crate::db::tiered_mount_pool::{dedupe_tier_triple, TierTriple};
use crate::db::DbError;
use crate::jsstr::{is_js_ws, js_trim, utf16_len};
use crate::literal_boost::{
    LITERAL_BOOST_CHARACTER, LITERAL_BOOST_GLOBAL, LITERAL_BOOST_GROUP, LITERAL_BOOST_PROJECT,
};
use crate::markdown::parse_frontmatter;
use crate::model::embedding::{EmbeddingPriority, EmbeddingProvider};
use crate::token_estimation::estimate_tokens;
use qtap_uri::{format_doc_store_uri, format_self_uri};
use serde_json::Value;

use self::document_search::{find_link_with_file_by_mount_point_and_path, search_document_chunks};

const DEFAULT_CANDIDATE_LIMIT: usize = 5;
const DEFAULT_INLINE_TOKEN_THRESHOLD: i64 = 500;
const DEFAULT_MIN_SCORE: f64 = 0.3;
const POINTER_TEASER_MAX_CHARS: usize = 120;

/// The four Knowledge/ tiers (v4 `KnowledgeTier`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KnowledgeTier {
    Character,
    Group,
    Project,
    Global,
}

impl KnowledgeTier {
    /// The rendered label (v4 `tierLabel` — note `global` renders as `general`).
    fn label(self) -> &'static str {
        match self {
            KnowledgeTier::Character => "character",
            KnowledgeTier::Group => "group",
            KnowledgeTier::Project => "project",
            KnowledgeTier::Global => "general",
        }
    }

    /// The raw tier key stored in the debug array (v4 stores `c.tier` verbatim —
    /// note this is `global`, unlike the rendered `general` from [`Self::label`]).
    pub fn debug_key(self) -> &'static str {
        match self {
            KnowledgeTier::Character => "character",
            KnowledgeTier::Group => "group",
            KnowledgeTier::Project => "project",
            KnowledgeTier::Global => "global",
        }
    }
}

/// Input parameters (v4 `KnowledgeRetrievalParams`).
#[derive(Debug, Clone, Default)]
pub struct KnowledgeRetrievalParams {
    pub character_id: String,
    pub user_id: String,
    pub embedding_profile_id: Option<String>,
    pub query: String,
    /// Mount point of the responding character's own vault (character tier).
    pub character_mount_point_id: Option<String>,
    /// Mount points of the groups the responding character is a member of.
    pub group_mount_point_ids: Vec<String>,
    /// Mount points linked to the active chat's project.
    pub project_mount_point_ids: Vec<String>,
    /// The Quilltap General singleton mount (global tier). None pre-provision.
    pub global_mount_point_id: Option<String>,
    pub budget_tokens: i64,
    /// The `estimateTokens` chars-per-token rate for `provider` (registry seam).
    pub chars_per_token: f64,
    /// Override the default 0.3 minimum cosine score.
    pub min_score: Option<f64>,
    /// Override the default 5 candidates per tier from the chunk search.
    pub candidate_limit: Option<usize>,
    /// Override the default 500-token inline threshold.
    pub inline_token_threshold: Option<i64>,
}

/// One debug entry (v4 `KnowledgeDebugEntry`).
#[derive(Debug, Clone, PartialEq)]
pub struct KnowledgeDebugEntry {
    pub file_path: String,
    pub tier: KnowledgeTier,
    pub score: f64,
    pub inline: bool,
    pub token_count: i64,
}

/// The result (v4 `KnowledgeRetrievalResult`).
#[derive(Debug, Clone, PartialEq)]
pub struct KnowledgeRetrievalResult {
    /// Formatted block for `CommonplaceParts.knowledge` — empty if nothing fit.
    pub content: String,
    /// Estimated token count of `content`.
    pub token_count: i64,
    pub debug: Vec<KnowledgeDebugEntry>,
}

impl KnowledgeRetrievalResult {
    fn empty() -> Self {
        KnowledgeRetrievalResult {
            content: String::new(),
            token_count: 0,
            debug: Vec::new(),
        }
    }
}

/// An in-flight candidate (v4 `Candidate`).
struct Candidate {
    tier: KnowledgeTier,
    mount_point_id: String,
    mount_point_name: String,
    file_path: String,
    score: f64,
    pointer_teaser: String,
    /// Body for inline candidates; None → pointer-only.
    body: Option<String>,
    /// Tags/topics from frontmatter when present.
    fm_tags: Option<String>,
}

/// v4 `retrieveKnowledgeForTurn`. `db` must have the mount-index partition open.
pub async fn retrieve_knowledge_for_turn<P: EmbeddingProvider>(
    db: &Db,
    provider: &P,
    params: &KnowledgeRetrievalParams,
) -> Result<KnowledgeRetrievalResult, DbError> {
    if params.budget_tokens <= 0 {
        return Ok(KnowledgeRetrievalResult::empty());
    }
    if js_trim(&params.query).is_empty() {
        return Ok(KnowledgeRetrievalResult::empty());
    }

    let min_score = params.min_score.unwrap_or(DEFAULT_MIN_SCORE);
    let candidate_limit = params.candidate_limit.unwrap_or(DEFAULT_CANDIDATE_LIMIT);
    let inline_threshold = params
        .inline_token_threshold
        .unwrap_or(DEFAULT_INLINE_TOKEN_THRESHOLD);

    // Build the tier plan via the shared dedup (character > group > project > global).
    let deduped = dedupe_tier_triple(TierTriple {
        character_mount_point_id: params.character_mount_point_id.clone(),
        group_mount_point_ids: params.group_mount_point_ids.clone(),
        project_mount_point_ids: params.project_mount_point_ids.clone(),
        global_mount_point_id: params.global_mount_point_id.clone(),
    });

    struct TierPlan {
        tier: KnowledgeTier,
        mount_point_ids: Vec<String>,
        boost: f64,
    }
    let mut tiers: Vec<TierPlan> = Vec::new();
    if let Some(id) = &deduped.character_mount_point_id {
        tiers.push(TierPlan {
            tier: KnowledgeTier::Character,
            mount_point_ids: vec![id.clone()],
            boost: LITERAL_BOOST_CHARACTER,
        });
    }
    if !deduped.group_mount_point_ids.is_empty() {
        tiers.push(TierPlan {
            tier: KnowledgeTier::Group,
            mount_point_ids: deduped.group_mount_point_ids.clone(),
            boost: LITERAL_BOOST_GROUP,
        });
    }
    if !deduped.project_mount_point_ids.is_empty() {
        tiers.push(TierPlan {
            tier: KnowledgeTier::Project,
            mount_point_ids: deduped.project_mount_point_ids.clone(),
            boost: LITERAL_BOOST_PROJECT,
        });
    }
    if let Some(id) = &deduped.global_mount_point_id {
        tiers.push(TierPlan {
            tier: KnowledgeTier::Global,
            mount_point_ids: vec![id.clone()],
            boost: LITERAL_BOOST_GLOBAL,
        });
    }

    if tiers.is_empty() {
        return Ok(KnowledgeRetrievalResult::empty());
    }

    // 1. Embed the query once (shared across tiers). Failure → empty result.
    let embedding = match provider
        .generate_embedding_for_user(
            &params.query,
            &params.user_id,
            params.embedding_profile_id.as_deref(),
            EmbeddingPriority::Interactive,
        )
        .await
    {
        Ok(r) => r.embedding,
        Err(_) => return Ok(KnowledgeRetrievalResult::empty()),
    };

    // 2. Run each tier's scoped search. v4 runs them in parallel; we run them
    // sequentially — the result is identical because the per-tier search is a pure
    // read and the merge below is order-independent modulo the stable dedup we
    // reproduce (see step 4). A tier whose search errors is skipped (empty hits).
    let mut tier_hits: Vec<(KnowledgeTier, Vec<DocumentSearchResult>)> = Vec::new();
    for plan in &tiers {
        let embedding = embedding.clone();
        let mount_point_ids = plan.mount_point_ids.clone();
        let query = params.query.clone();
        let boost = plan.boost;
        let opts = DocumentSearchOptions {
            mount_point_ids: Some(mount_point_ids),
            path_prefix: Some("Knowledge/".to_string()),
            limit: Some(candidate_limit),
            min_score: Some(min_score),
            query: Some(query),
            apply_literal_phrase_boost: true,
            literal_boost_fraction: Some(boost),
            include_blocked: false,
        };
        let hits = db
            .read_mount_index(move |conn| search_document_chunks(conn, &embedding, &opts))
            .unwrap_or_default();
        tier_hits.push((plan.tier, hits));
    }

    // 3. Mount-name lookup table for pointer templates (read once). v4 reads
    // findById per unseen mount; we batch the lookup to one map for the seen set.
    let mut seen_mount_ids: Vec<String> = Vec::new();
    for (_, hits) in &tier_hits {
        for h in hits {
            if !seen_mount_ids.contains(&h.mount_point_id) {
                seen_mount_ids.push(h.mount_point_id.clone());
            }
        }
    }
    let mount_names: Vec<(String, String)> = {
        let ids = seen_mount_ids.clone();
        db.read_mount_index(move |conn| {
            let mut out = Vec::new();
            for id in &ids {
                let name: Option<String> = conn
                    .query_row(
                        "SELECT name FROM doc_mount_points WHERE id = ?1",
                        [id],
                        |r| r.get(0),
                    )
                    .ok();
                out.push((
                    id.clone(),
                    name.unwrap_or_else(|| "Knowledge Source".to_string()),
                ));
            }
            Ok(out)
        })
        .unwrap_or_default()
    };
    let mount_name_of = |id: &str| -> String {
        mount_names
            .iter()
            .find(|(mid, _)| mid == id)
            .map(|(_, n)| n.clone())
            .unwrap_or_else(|| "Knowledge Source".to_string())
    };

    // 4. Build candidates from every tier, dedup by chunkId across tiers (best score
    // wins). v4 iterates tiers in plan order and updates the map when the new score
    // is STRICTLY greater — insertion order is the first tier a chunk appears in.
    let mut candidates_by_chunk: Vec<(String, Candidate)> = Vec::new();
    for (tier, hits) in &tier_hits {
        for hit in hits {
            // Look up the file (fileType) by (mountPointId, relativePath).
            let file = {
                let mp = hit.mount_point_id.clone();
                let rp = hit.relative_path.clone();
                db.read_mount_index(move |conn| {
                    find_link_with_file_by_mount_point_and_path(conn, &mp, &rp)
                })
                .ok()
                .flatten()
            };
            let Some(file) = file else { continue };

            let teaser = build_pointer_teaser(hit.heading_context.as_deref(), &hit.content);

            let mut body: Option<String> = None;
            let mut fm_tags: Option<String> = None;

            if file.file_type == "pdf" || file.file_type == "docx" || file.file_type == "blob" {
                // Derived blobs are always pointer-only.
                body = None;
            } else {
                let doc_content = {
                    let mp = hit.mount_point_id.clone();
                    let rp = hit.relative_path.clone();
                    db.read_mount_index(move |conn| {
                        crate::db::doc_mount_documents::DocMountDocumentsRepository::new(conn)
                            .find_by_mount_point_and_path(&mp, &rp)
                    })
                    .ok()
                    .flatten()
                };
                if let Some(content) = doc_content {
                    if file.file_type == "markdown" {
                        let fm = parse_frontmatter(&content);
                        if let Some(data) = &fm.data {
                            fm_tags = render_frontmatter_tags(data);
                        }
                    }
                    let body_tokens = estimate_tokens(&content, params.chars_per_token);
                    body = if body_tokens <= inline_threshold {
                        Some(content)
                    } else {
                        None
                    };
                }
            }

            let candidate = Candidate {
                tier: *tier,
                mount_point_id: hit.mount_point_id.clone(),
                mount_point_name: {
                    let n = mount_name_of(&hit.mount_point_id);
                    // v4: mountNames.get(id) ?? hit.mountPointName ?? 'Knowledge Source'.
                    if n == "Knowledge Source" && !hit.mount_point_name.is_empty() {
                        hit.mount_point_name.clone()
                    } else {
                        n
                    }
                },
                file_path: hit.relative_path.clone(),
                score: hit.score,
                pointer_teaser: teaser,
                body,
                fm_tags,
            };

            match candidates_by_chunk
                .iter_mut()
                .find(|(cid, _)| cid == &hit.chunk_id)
            {
                Some((_, existing)) => {
                    if candidate.score > existing.score {
                        *existing = candidate;
                    }
                }
                None => candidates_by_chunk.push((hit.chunk_id.clone(), candidate)),
            }
        }
    }

    if candidates_by_chunk.is_empty() {
        return Ok(KnowledgeRetrievalResult::empty());
    }

    // 5. Greedy pack into the budget. Sort by score desc (stable — preserves the
    // map's insertion order among equal scores, matching JS's stable Array.sort over
    // `Array.from(map.values())`).
    let mut candidates: Vec<Candidate> = candidates_by_chunk.into_iter().map(|(_, c)| c).collect();
    candidates.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut sections: Vec<String> = Vec::new();
    let mut debug: Vec<KnowledgeDebugEntry> = Vec::new();
    let mut seen_file_paths: Vec<String> = Vec::new();
    let mut running_tokens = 0i64;

    for c in &candidates {
        // Two chunks of the same file shouldn't both surface.
        let file_key = format!("{}::{}", c.mount_point_id, c.file_path);
        if seen_file_paths.contains(&file_key) {
            continue;
        }
        seen_file_paths.push(file_key);

        let mut rendered;
        let mut rendered_tokens;
        let mut inlined = false;

        if c.body.is_some() {
            rendered = render_inline_entry(c);
            rendered_tokens = estimate_tokens(&rendered, params.chars_per_token);
            if running_tokens + rendered_tokens <= params.budget_tokens {
                inlined = true;
            } else {
                rendered = render_pointer_entry(c);
                rendered_tokens = estimate_tokens(&rendered, params.chars_per_token);
            }
        } else {
            rendered = render_pointer_entry(c);
            rendered_tokens = estimate_tokens(&rendered, params.chars_per_token);
        }

        if running_tokens + rendered_tokens > params.budget_tokens {
            break;
        }

        sections.push(rendered);
        running_tokens += rendered_tokens;
        debug.push(KnowledgeDebugEntry {
            file_path: c.file_path.clone(),
            tier: c.tier,
            score: c.score,
            inline: inlined,
            token_count: rendered_tokens,
        });
    }

    if sections.is_empty() {
        return Ok(KnowledgeRetrievalResult::empty());
    }

    Ok(KnowledgeRetrievalResult {
        content: sections.join("\n\n"),
        token_count: running_tokens,
        debug,
    })
}

/// Canonical qtap:// URI for a candidate (v4 `knowledgeUri`). The character tier IS
/// the acting character's own vault → the stable `qtap://self/…` form; every other
/// tier addresses its store by name.
fn knowledge_uri(c: &Candidate) -> String {
    if c.tier == KnowledgeTier::Character {
        format_self_uri(&c.file_path, None, None)
    } else {
        format_doc_store_uri(
            &c.mount_point_name,
            &c.mount_point_id,
            &c.file_path,
            false,
            None,
            None,
        )
    }
}

fn render_inline_entry(c: &Candidate) -> String {
    let uri = knowledge_uri(c);
    let mut lines: Vec<String> = Vec::new();
    lines.push(format!("### Knowledge ({}) — {}", c.tier.label(), uri));
    if let Some(tags) = &c.fm_tags {
        lines.push(format!("Tags: {tags}"));
    }
    lines.push(String::new());
    // (c.body ?? '').trimEnd()
    let body = c.body.as_deref().unwrap_or("");
    lines.push(crate::jsstr::js_trim_end(body).to_string());
    lines.push(String::new());
    lines.push(format!(
        "If you need to re-read with offset/limit: doc_read_file(uri=\"{uri}\")"
    ));
    lines.join("\n")
}

fn render_pointer_entry(c: &Candidate) -> String {
    let uri = knowledge_uri(c);
    let mut lines: Vec<String> = Vec::new();
    lines.push(format!("### Knowledge ({}) — {}", c.tier.label(), uri));
    if !c.pointer_teaser.is_empty() {
        lines.push(format!("Why: {}", c.pointer_teaser));
    }
    if let Some(tags) = &c.fm_tags {
        lines.push(format!("Tags: {tags}"));
    }
    lines.push(format!("Read with: doc_read_file(uri=\"{uri}\")"));
    lines.join("\n")
}

/// v4 `buildPointerTeaser`. Prefers a trimmed heading; else collapses whitespace
/// and trims the chunk content to the 120-char (UTF-16) word-boundary cap.
fn build_pointer_teaser(heading_context: Option<&str>, chunk_content: &str) -> String {
    if let Some(h) = heading_context {
        let heading = js_trim(h);
        if !heading.is_empty() {
            return heading.to_string();
        }
    }
    // collapsed = chunkContent.replace(/\s+/g, ' ').trim()
    let collapsed = collapse_ws(chunk_content);
    let collapsed = js_trim(&collapsed).to_string();
    if utf16_len(&collapsed) <= POINTER_TEASER_MAX_CHARS {
        return collapsed;
    }
    // cut = collapsed.slice(0, 120)  (UTF-16)
    let cut = crate::jsstr::utf16_truncate(&collapsed, POINTER_TEASER_MAX_CHARS);
    // lastSpace = cut.lastIndexOf(' ')  (UTF-16 index)
    let last_space = utf16_last_index_of_space(&cut);
    // trimmed = lastSpace > 60 ? cut.slice(0, lastSpace) : cut
    let trimmed = match last_space {
        Some(idx) if idx > 60 => crate::jsstr::utf16_truncate(&cut, idx),
        _ => cut,
    };
    // trimmed.replace(/[.,;:!?-]+$/, '') + '…'
    format!("{}…", strip_trailing_punct(&trimmed))
}

/// Reproduce JS `str.replace(/\s+/g, ' ')`: collapse every maximal run of JS
/// whitespace to a single ASCII space.
fn collapse_ws(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_ws = false;
    for c in s.chars() {
        if is_js_ws(c) {
            if !in_ws {
                out.push(' ');
                in_ws = true;
            }
        } else {
            out.push(c);
            in_ws = false;
        }
    }
    out
}

/// UTF-16 index of the last space (`' '`, U+0020) in `s`, matching JS
/// `lastIndexOf(' ')`.
fn utf16_last_index_of_space(s: &str) -> Option<usize> {
    let mut last: Option<usize> = None;
    let mut idx = 0usize;
    for c in s.chars() {
        if c == ' ' {
            last = Some(idx);
        }
        idx += c.len_utf16();
    }
    last
}

/// JS `str.replace(/[.,;:!?-]+$/, '')`: strip a trailing run of `.,;:!?-`.
fn strip_trailing_punct(s: &str) -> &str {
    s.trim_end_matches(['.', ',', ';', ':', '!', '?', '-'])
}

/// v4 `renderFrontmatterTags`: surface `tags` then `topics` (string or string-array),
/// joined with `' · '`. Returns None when neither yields content.
fn render_frontmatter_tags(data: &Value) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();
    for key in ["tags", "topics"] {
        let v = data.get(key);
        match v {
            Some(Value::Array(items)) => {
                let joined: Vec<String> = items
                    .iter()
                    .filter_map(|t| t.as_str())
                    .map(|t| js_trim(t).to_string())
                    .filter(|t| !t.is_empty())
                    .collect();
                if !joined.is_empty() {
                    parts.push(joined.join(", "));
                }
            }
            Some(Value::String(s)) => {
                let t = js_trim(s);
                if !t.is_empty() {
                    parts.push(t.to_string());
                }
            }
            _ => {}
        }
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" · "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn teaser_prefers_heading() {
        assert_eq!(
            build_pointer_teaser(Some("  My Heading  "), "ignored body"),
            "My Heading"
        );
    }

    #[test]
    fn teaser_collapses_and_caps() {
        let short = build_pointer_teaser(None, "  a\tb\n c  ");
        assert_eq!(short, "a b c");
        let long = "word ".repeat(40); // 200 chars, spaces at each boundary
        let t = build_pointer_teaser(None, &long);
        assert!(t.ends_with('…'));
        assert!(utf16_len(&t) <= POINTER_TEASER_MAX_CHARS + 1);
    }

    #[test]
    fn frontmatter_tags_render() {
        let data = serde_json::json!({ "tags": ["a", " b ", ""], "topics": "x" });
        assert_eq!(render_frontmatter_tags(&data), Some("a, b · x".to_string()));
        assert_eq!(render_frontmatter_tags(&serde_json::json!({})), None);
    }
}
