//! The lexical-text writers RULE (P4.D292, Tier 2 #17; v4 `4ae0c4ca6`).
//!
//! Every memory writer embeds the canonical ANCHORED text
//! (`memory_embedding_text::embedding_text_for`) and hands the embedding seam
//! the anchor-free `lexical_embedding_text_for` as `lexical_text` — "what a
//! lexical (BUILTIN, TF-IDF) profile embeds instead of `text`". A writer that
//! forgets it embeds the anchored text on BUILTIN: dates, place names and their
//! bigrams then dominate a short memory's bag of words. No differential sees
//! that on a neural fixture, and a BUILTIN one only through the writer it
//! happens to drive — so the obligation is held here, mechanically, for every
//! writer at once.
//!
//! **A rule, not a count.** Each production-zone CALL of `embedding_text_for(`
//! (word-bounded, so `lexical_embedding_text_for(` is not one; the defining
//! module's `fn` excluded) must be followed — within the same 2,500 bytes of
//! code, comments and literals stripped, which spans a writer's build →
//! guard → embed sequence — by BOTH `lexical_embedding_text_for(` and the
//! seam field (`lexical_text` or `with_lexical_text`). It is green on every
//! branch whatever the writer set; it PRINTS the writers it found, so the
//! unifier can re-measure the union (v4 has eight at the pin: EMBEDDING_GENERATE,
//! admission ×4, the digest embed, the backfill, the anchor probe's anchored
//! side — the admission / digest / probe writers land with P4.D290 / P4.D297).
//!
//! Run standalone:
//!   cargo test -p quilltap-harness --test lexical_text_writers_guard

mod source_census;

use source_census::{code_only, core_src_root, production_zone, rust_sources};

const WINDOW: usize = 2_500;
const CALL: &str = "embedding_text_for(";

/// Every word-bounded `embedding_text_for(` call offset in `code`.
fn calls(code: &str) -> Vec<usize> {
    let mut out = Vec::new();
    let mut from = 0usize;
    while let Some(at) = code[from..].find(CALL) {
        let abs = from + at;
        let before = code[..abs].chars().next_back();
        let bounded = before.is_none_or(|c| !c.is_ascii_alphanumeric() && c != '_');
        let is_definition = code[..abs].trim_end().ends_with("fn");
        if bounded && !is_definition {
            out.push(abs);
        }
        from = abs + CALL.len();
    }
    out
}

fn window(code: &str, at: usize) -> &str {
    let mut end = (at + WINDOW).min(code.len());
    while !code.is_char_boundary(end) {
        end -= 1;
    }
    &code[at..end]
}

#[test]
fn every_memory_writer_hands_the_seam_its_lexical_text() {
    let root = core_src_root();
    let mut files = Vec::new();
    rust_sources(&root, &mut files);
    files.sort();
    let mut writers = Vec::new();
    let mut offenders = Vec::new();
    for path in &files {
        let rel = path
            .strip_prefix(&root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        let src = std::fs::read_to_string(path).unwrap();
        let code = code_only(&production_zone(&src));
        for at in calls(&code) {
            let line = code[..at].matches('\n').count() + 1;
            let w = window(&code, at);
            let lexical = w.contains("lexical_embedding_text_for(");
            let field = w.contains("lexical_text") || w.contains("with_lexical_text");
            writers.push(format!("{rel} (code line ~{line})"));
            if !(lexical && field) {
                offenders.push(format!(
                    "{rel} (code line ~{line}): embeds `embedding_text_for` without handing the seam `lexical_text: lexical_embedding_text_for(..)`"
                ));
            }
        }
    }
    eprintln!(
        "lexical-text writers found ({}):\n  {}",
        writers.len(),
        writers.join("\n  ")
    );
    assert!(
        !writers.is_empty(),
        "no memory writer found at all — the census lost its subject (EMBEDDING_GENERATE embeds the anchored text)"
    );
    assert!(
        offenders.is_empty(),
        "{} memory writer(s) embed the anchored text with no lexical text for BUILTIN:\n{}",
        offenders.len(),
        offenders.join("\n")
    );
}

/// The rule's own fixtures: a writer that forgets the lexical text is caught,
/// one that passes it is not, and the definition / the lexical twin are not
/// calls.
#[test]
fn the_rule_catches_a_forgetting_writer() {
    let forgetting = "let t = embedding_text_for(&m, tz); p.embed(&t, opts).await;";
    let faithful = "let t = embedding_text_for(&m, tz); EmbeddingCallOptions { lexical_text: Some(lexical_embedding_text_for(&m)), ..x }";
    let definition = "pub fn embedding_text_for(row: &S, tz: &str) -> String { lexical_embedding_text_for(row) }";
    for (src, n) in [(forgetting, 1), (faithful, 1), (definition, 0)] {
        assert_eq!(calls(src).len(), n, "{src}");
    }
    let w = window(forgetting, calls(forgetting)[0]);
    assert!(!(w.contains("lexical_embedding_text_for(") && w.contains("lexical_text")));
    let w = window(faithful, calls(faithful)[0]);
    assert!(w.contains("lexical_embedding_text_for(") && w.contains("lexical_text"));
}
