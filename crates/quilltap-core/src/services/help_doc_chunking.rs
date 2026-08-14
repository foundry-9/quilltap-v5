//! Help-document chunking — port of v4 `lib/help/help-doc-chunking.ts` (v4
//! `24633026`).
//!
//! Slices a help document into section-sized pieces for embedding. v4's *why*,
//! carried forward: help docs are long and topically broad — `help/chat-settings.md`
//! alone covers a dozen unrelated subsystems — so a single whole-document
//! embedding is a smear that matches any specific question only weakly. Chunking
//! gives each section its own vector, which is what lets "how do I describe an
//! image for a model that can't see?" land on the paragraph that answers it.
//!
//! **The chunker itself is the Scriptorium's** ([`super::mount_index::chunker`],
//! v4 `lib/mount-index/chunker.ts`) — same Markdown-aware paragraph
//! accumulation, same heading tracking, same overlap. v4 reuses it; so does
//! this. Only the size targets differ.

use super::mount_index::chunker::{chunk_document, ChunkOptions};
use crate::db::help_doc_chunks::HelpDocChunkDraft;

/// v4 `HELP_CHUNK_OPTIONS` — deliberately smaller than the Scriptorium's
/// 800–1200 defaults.
///
/// v4's *why*: help sections are short — a settings subsection runs a few
/// hundred words — and the whole point here is precision, so a chunk that
/// swallows four unrelated settings sections defeats the exercise. The overlap
/// keeps a paragraph that straddles a boundary reachable from both sides.
pub const HELP_CHUNK_OPTIONS: ChunkOptions = ChunkOptions {
    target_min_tokens: Some(400),
    target_max_tokens: Some(700),
    overlap_tokens: Some(100),
};

/// v4 `buildHelpDocChunks` — split a help document into chunk drafts, in
/// document order; empty for empty input.
///
/// The title is NOT prepended here — it is added at embedding time
/// ([`help_chunk_embedding_text`]), so the stored chunk text stays a faithful
/// excerpt of the document, which is what gets shown back to a reader.
///
/// @param content the document body, frontmatter already stripped
pub fn build_help_doc_chunks(content: &str) -> Vec<HelpDocChunkDraft> {
    chunk_document(content, HELP_CHUNK_OPTIONS)
        .into_iter()
        .map(|chunk| HelpDocChunkDraft {
            chunk_index: chunk.chunk_index,
            heading: chunk.heading_context,
            content: chunk.content,
        })
        .collect()
}

/// v4 `helpChunkEmbeddingText` — the text actually handed to the embedding
/// provider for a chunk.
///
/// Title and heading are prefixed so a chunk carries the context a reader would
/// have from the page around it — "Uncensored fallback profile" means little on
/// its own, but a great deal under "Chat Settings › Image Description Settings".
///
/// ⚠ v4's guard is `heading ? … : docTitle` — a **JS truthiness** test, so an
/// EMPTY-STRING heading takes the title-only branch just as `null`/`undefined`
/// do. `Some(String::new())` must therefore behave like `None` here; a plain
/// `match heading { Some(h) => … }` would silently emit `"Title › \n\nbody"`.
/// The chunker can produce an empty heading (`## ` followed by whitespace never
/// matches, but a heading whose text trims away does not arise) — the guard is
/// carried because v4's is, not because a caller is known to hit it.
pub fn help_chunk_embedding_text(doc_title: &str, heading: Option<&str>, content: &str) -> String {
    let path = match heading {
        Some(h) if !h.is_empty() => format!("{doc_title} \u{203a} {h}"),
        _ => doc_title.to_string(),
    };
    format!("{path}\n\n{content}")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// v4's test helper `longDocument(sections)`, verbatim in shape.
    fn long_document(sections: usize) -> String {
        let filler = "Words about the subject at hand, repeated for length. ".repeat(60);
        (0..sections)
            .map(|i| format!("## Section {i}\n\n{filler}"))
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    #[test]
    fn empty_input_yields_nothing() {
        assert!(build_help_doc_chunks("").is_empty());
        assert!(build_help_doc_chunks("   \n\n  ").is_empty());
    }

    #[test]
    fn short_document_stays_one_chunk() {
        let chunks = build_help_doc_chunks("# Title\n\nA brief paragraph.");
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].chunk_index, 0);
        assert!(chunks[0].content.contains("A brief paragraph."));
    }

    #[test]
    fn long_document_splits_with_sequential_indices_and_headings() {
        let chunks = build_help_doc_chunks(&long_document(6));
        assert!(chunks.len() > 1);
        for (i, chunk) in chunks.iter().enumerate() {
            assert_eq!(chunk.chunk_index, i as i64);
            assert!(
                chunk
                    .heading
                    .as_deref()
                    .is_some_and(|h| h.starts_with("Section")),
                "chunk {i} heading {:?}",
                chunk.heading
            );
        }
    }

    #[test]
    fn chunks_stay_small_enough_to_be_worth_the_exercise() {
        // v4's assertion: 700-token target at ~4 chars/token plus the overlap
        // prefix — the point is that one chunk never swallows the document.
        for chunk in build_help_doc_chunks(&long_document(8)) {
            assert!(chunk.content.len() < 4200, "{}", chunk.content.len());
        }
    }

    #[test]
    fn embedding_text_prefixes_title_and_heading() {
        assert_eq!(
            help_chunk_embedding_text(
                "Chat Settings",
                Some("Image Description Settings"),
                "Body text."
            ),
            "Chat Settings \u{203a} Image Description Settings\n\nBody text."
        );
        assert_eq!(
            help_chunk_embedding_text("Chat Settings", None, "Body text."),
            "Chat Settings\n\nBody text."
        );
        // The JS-truthiness edge: an empty heading is falsy, so title-only.
        assert_eq!(
            help_chunk_embedding_text("Chat Settings", Some(""), "Body text."),
            "Chat Settings\n\nBody text."
        );
    }
}
