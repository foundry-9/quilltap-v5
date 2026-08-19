//! Port of v4's `help_search` tool
//! (`lib/tools/handlers/help-search-handler.ts` + `lib/tools/help-search-tool.ts`).
//!
//! Semantic search over the help documentation with an automatic fallback to the
//! keyword scorer when the embedding call fails. The search primitives live in
//! [`crate::db::help_search`] (`semantic_search` / `keyword_search`); this handler
//! validates input, runs the embed-then-search path, catches an embedding failure
//! to fall back, and formats.
//!
//! The disk-sync half of v4's `HelpSearch.loadFromDatabase()` (`ensureHelpDocsSynced`)
//! is a host-side startup index-build that no-ops once `help_docs` is populated; the
//! tool path only reads stored embeddings, so it is a documented host seam (see
//! [`crate::db::help_search`]).

use serde::Serialize;
use serde_json::{json, Value};

use crate::db::help_search::{keyword_search, semantic_search, HelpSearchResult};
use crate::db::runtime::Db;
use crate::model::embedding::{EmbeddingPriority, EmbeddingProvider};
use crate::tools::llm_number::llm_number;

/// v4 `HelpSearchToolOutput` — `{ success, results?, error?, totalFound, query }`.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct HelpSearchOutput {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub results: Option<Vec<HelpResultItem>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(rename = "totalFound")]
    pub total_found: i64,
    pub query: String,
}

/// A help result as the tool emits it (v4 `HelpSearchResult`): `{ id, title, path,
/// url, score, content }`, plus `matchedSection` since v4 `24633026`.
#[derive(Debug, Clone, PartialEq)]
pub struct HelpResultItem {
    pub id: String,
    pub title: String,
    pub path: String,
    pub url: String,
    pub score: f64,
    pub content: String,
    /// v4: the section of the document that actually matched, when the match
    /// came from a section vector rather than the whole-document one. Absent
    /// for keyword results and for docs with no embedded sections — and the KEY
    /// itself is absent, not null (v4 spreads a conditional object), which is
    /// why the hand-rolled `Serialize` below only inserts it when present.
    ///
    /// Note it carries `heading` + `content` only: the tool's shape drops the
    /// `chunkIndex` that `db::help_search::HelpMatchedSection` keeps.
    pub matched_section: Option<HelpResultSection>,
}

/// The tool's `matchedSection` object — v4 `{ heading, content }`.
#[derive(Debug, Clone, PartialEq)]
pub struct HelpResultSection {
    pub heading: Option<String>,
    pub content: String,
}

impl Serialize for HelpResultItem {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        // Build the exact v4 object (order + JS-number score rendering).
        self.to_value().serialize(s)
    }
}

impl HelpResultItem {
    /// The v4 result object: `{ id, title, path, url, score, content }` with `score`
    /// rendered like `JSON.stringify` (an integer-valued float collapses), plus
    /// `matchedSection` LAST when present — v4 spreads it after `content`.
    pub fn to_value(&self) -> Value {
        let mut out = json!({
            "id": self.id,
            "title": self.title,
            "path": self.path,
            "url": self.url,
            "score": crate::db::js_number_to_json(self.score),
            "content": self.content,
        });
        if let Some(section) = &self.matched_section {
            out.as_object_mut().expect("object").insert(
                "matchedSection".to_string(),
                json!({
                    "heading": section.heading,
                    "content": section.content,
                }),
            );
        }
        out
    }
}

impl From<HelpSearchResult> for HelpResultItem {
    fn from(r: HelpSearchResult) -> Self {
        HelpResultItem {
            id: r.id,
            title: r.title,
            path: r.path,
            url: r.url,
            score: r.score,
            content: r.content,
            matched_section: r.matched_section.map(|s| HelpResultSection {
                heading: s.heading,
                content: s.content,
            }),
        }
    }
}

/// v4 `validateHelpSearchInput` — query `min(1).max(500)` (UTF-16 length) + a
/// `refine` that the trimmed query is non-empty; `limit` an optional int `1..=10`.
/// Extra keys allowed.
fn validate_input(args: &Value) -> bool {
    let Some(query) = args.get("query").and_then(Value::as_str) else {
        return false;
    };
    let qlen = query.encode_utf16().count();
    if !(1..=500).contains(&qlen) {
        return false;
    }
    if query.trim().is_empty() {
        return false;
    }
    // limit is an llmNumber(...) field: a numeric-looking string converts before
    // validation, then the bounds apply exactly as they do to a bare number.
    if let Some(limit) = args.get("limit") {
        match llm_number(limit).as_f64() {
            Some(n) if n.fract() == 0.0 && (1.0..=10.0).contains(&n) => {}
            _ => return false,
        }
    }
    true
}

/// Execute the `help_search` tool (v4 `executeHelpSearchTool`).
pub async fn execute_help_search<P: EmbeddingProvider>(
    db: &Db,
    provider: &P,
    user_id: &str,
    args: &Value,
) -> HelpSearchOutput {
    if !validate_input(args) {
        return HelpSearchOutput {
            success: false,
            results: None,
            error: Some(
                "Invalid input: query is required and must be a non-empty string (max 500 chars)"
                    .to_string(),
            ),
            total_found: 0,
            query: String::new(),
        };
    }

    let query = args.get("query").and_then(Value::as_str).unwrap_or("");
    // The read goes through the same seam (the Zod parse replaces the value).
    let limit = args
        .get("limit")
        .and_then(|raw| llm_number(raw).as_f64())
        .map(|n| n as usize)
        .unwrap_or(3);

    // Semantic first (embed → search); any error there falls back to keyword search.
    let semantic = match provider
        .generate_embedding_for_user(query, user_id, None, EmbeddingPriority::Interactive)
        .await
    {
        Ok(emb) => db.read_main(|c| semantic_search(c, &emb.embedding, limit, Some(query))),
        Err(_) => Err(crate::db::DbError::Internal("embedding failed".to_string())),
    };

    let results: Vec<HelpSearchResult> = match semantic {
        Ok(r) => r,
        Err(_) => match db.read_main(|c| keyword_search(c, query, limit)) {
            Ok(r) => r,
            // v4's outer catch: a keyword-search DB error → failure result.
            Err(e) => {
                return HelpSearchOutput {
                    success: false,
                    results: None,
                    error: Some(e.to_string()),
                    total_found: 0,
                    query: query.to_string(),
                }
            }
        },
    };

    let items: Vec<HelpResultItem> = results.into_iter().map(HelpResultItem::from).collect();
    HelpSearchOutput {
        success: true,
        total_found: items.len() as i64,
        results: Some(items),
        error: None,
        query: query.to_string(),
    }
}

/// Character budgets for the context block handed to the model (v4
/// `24633026`).
///
/// v4's *why*, carried forward: `help_search` is the only way a character can
/// read help text — there is no separate "read this doc" tool — so a result
/// must carry enough of the document to answer with, while a handful of results
/// must still fit in a turn. A matched section gets the larger share; the
/// document excerpt beneath it is orientation, not the answer.
const MATCHED_SECTION_CHARS: usize = 1500;
const DOC_CONTEXT_CHARS: usize = 600;
const DOC_EXCERPT_CHARS: usize = 1000;

/// v4 `formatHelpSearchResults`.
pub fn format_help_search(results: &[HelpResultItem]) -> String {
    if results.is_empty() {
        return "No relevant help documentation found.".to_string();
    }
    let formatted: Vec<String> = results
        .iter()
        .enumerate()
        .map(|(index, result)| {
            let label = if result.score >= 0.7 {
                "High"
            } else if result.score >= 0.4 {
                "Medium"
            } else {
                "Low"
            };
            let header = format!(
                "[Help Document {}] (Relevance: {})\nTitle: {}\nPath: {}\nURL: {}",
                index + 1,
                label,
                result.title,
                result.path,
                result.url,
            );

            // When a specific section matched, LEAD with it (v4's *why*): the
            // document excerpt below is the first N characters of the file,
            // which for a long settings page is a table of contents and a
            // preamble — never the answer. Putting the matching section first
            // is the whole point of embedding sections.
            if let Some(section) = &result.matched_section {
                // `heading ?` is a JS truthiness test, so an EMPTY heading takes
                // the unlabelled branch.
                let section_label = match section.heading.as_deref() {
                    Some(h) if !h.is_empty() => format!("Matching section — \"{h}\":"),
                    _ => "Matching section:".to_string(),
                };
                let body = truncate_utf16(&section.content, MATCHED_SECTION_CHARS);
                let surrounding = truncate_utf16(&result.content, DOC_CONTEXT_CHARS);
                return format!(
                    "{header}\n{section_label}\n{body}\n\nDocument begins:\n{surrounding}"
                );
            }

            format!(
                "{header}\nContent:\n{}",
                truncate_utf16(&result.content, DOC_EXCERPT_CHARS)
            )
        })
        .collect();
    format!(
        "Found {} relevant help documents:\n\n{}",
        results.len(),
        formatted.join("\n\n---\n\n")
    )
}

/// v4's `content.length > n ? content.substring(0, n) + '...' : content` — UTF-16
/// code-unit length + slice.
pub(crate) fn truncate_utf16(content: &str, n: usize) -> String {
    let units: Vec<u16> = content.encode_utf16().collect();
    if units.len() > n {
        let head = String::from_utf16_lossy(&units[..n]);
        format!("{head}...")
    } else {
        content.to_string()
    }
}
