//! Document-editing foundation layer — v4 `lib/doc-edit/` (W4.1d batch 3a).
//!
//! The pure + DB-backed foundation the ~26 `doc_*` tool handlers (batch 3b) sit
//! on: the `qtap://` URI codec, diacritics-aware matching, the MIME registry,
//! unified diff, markdown heading/frontmatter ops, the per-document policy
//! leaves, and the document-store path resolver.
//!
//! The tiered mount pool this layer composes lives in
//! [`crate::db::tiered_mount_pool`] (v4's `lib/mount-index/tiered-mount-pool.ts`,
//! its canonical home).

pub mod diacritics;
pub mod line_diff;
pub mod markdown_parser;
pub mod mime_registry;
pub mod path_resolver;
pub mod qtap_uri;
pub mod typographic_folding;
pub mod unified_diff;
pub mod uri_producers;
//
// v4's per-document policy leaves (`coercePolicyBool` / `policyFromFrontmatterData`
// / `policyFromContent`) are already ported in
// [`crate::db::doc_mount_file_links`]; there is nothing left of
// `document-policy.ts` to port here.

/// The three doc-edit scopes (v4 `DocEditScope`). `document_store` files live in
/// mounted document stores; `project` / `general` are the (host-filesystem)
/// legacy scopes. Shared by the path resolver and the `qtap://` codec.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DocEditScope {
    DocumentStore,
    Project,
    General,
}

impl DocEditScope {
    /// The lowercase wire token (`document_store` / `project` / `general`).
    pub fn as_str(self) -> &'static str {
        match self {
            DocEditScope::DocumentStore => "document_store",
            DocEditScope::Project => "project",
            DocEditScope::General => "general",
        }
    }
}
