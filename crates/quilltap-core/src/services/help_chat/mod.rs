//! The help/HelpChat server family (P4.9I2A) — v4 `lib/help-chat/**`,
//! `lib/services/help-chat/**` and the `help-docs` route's Guide text search.
//!
//! - [`context_resolver`] — v4 `lib/help-chat/context-resolver.ts`: the
//!   six-strategy URL → help-document resolver (exact → query → pattern →
//!   prefix → wildcard → fallback) and its `resolveAll` sibling.
//! - [`system_prompt`] — v4 `lib/help-chat/system-prompt-builder.ts`: the
//!   help-mode character system prompt, byte-exact.
//! - [`guide_search`] — v4 `app/api/v1/help-docs/route.ts`'s `buildSnippet` +
//!   `handleSearch` mapper: the Guide's plain substring search over document
//!   TEXT, plus the `listDocuments` / `getDocument(idOrSlug)` projections of v4
//!   `lib/help-search.ts`.
//! - `orchestrator` (unit 7) — v4 `lib/services/help-chat/orchestrator.service.ts`:
//!   the help-chat send loop (all selected help characters answer in turn).
//!
//! ## One recorded divergence: no process cache
//!
//! v4's `HelpSearch` keeps an in-process `documents` array filled once by
//! `loadFromDatabase()` (which FIRST runs `ensureHelpDocsSynced()`), with an
//! `invalidate()` hook nothing production calls. v5 reads the `help_docs` table
//! per call — the boot ensure (P4.9I2A unit 2) has already synced it — so
//! `invalidate()` has no twin and the "loaded" state has no analog. Rows are
//! identical; only the read timing differs.

pub mod context_resolver;
pub mod guide_search;
pub mod system_prompt;

use serde_json::Value;

use crate::db::help_docs::HelpDocRow;
use crate::help_doc_slug::help_doc_slug;

/// v4 `HelpDocument` (`lib/help-search.types.ts`) — the in-process document
/// shape `HelpSearch.loadFromDatabase()` maps each `help_docs` row to: the DB
/// row plus its path-derived `slug`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HelpDocument {
    pub id: String,
    pub slug: String,
    pub title: String,
    pub path: String,
    pub url: String,
    pub content: String,
}

impl HelpDocument {
    /// v4 `loadFromDatabase`'s per-row map (`help-search.ts:47-54`).
    pub fn from_row(row: &HelpDocRow) -> Self {
        HelpDocument {
            id: row.id.clone(),
            slug: help_doc_slug(&row.path),
            title: row.title.clone(),
            path: row.path.clone(),
            url: row.url.clone(),
            content: row.content.clone(),
        }
    }

    /// v4 `listDocuments()`'s row (`{id, slug, title, path, url}` — no content).
    pub fn listing(&self) -> Value {
        serde_json::json!({
            "id": self.id,
            "slug": self.slug,
            "title": self.title,
            "path": self.path,
            "url": self.url,
        })
    }
}

/// v4 `getDocument(idOrSlug)` — `doc.id === x || doc.slug === x`, first match in
/// document order, `None` on a miss.
pub fn get_document<'a>(docs: &'a [HelpDocument], id_or_slug: &str) -> Option<&'a HelpDocument> {
    docs.iter()
        .find(|d| d.id == id_or_slug || d.slug == id_or_slug)
}
