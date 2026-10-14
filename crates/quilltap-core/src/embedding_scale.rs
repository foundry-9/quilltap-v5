//! Port of v4's `lib/embedding/cosine-scale.ts` (`4ae0c4ca6`) — which cosine
//! scale an embedding provider's vectors live on.
//!
//! v4's why, carried: neural embedders put unrelated text near ~0.25 and a
//! restated fact at 0.85+; a lexical (bag-of-words) embedder such as the
//! Built-in TF-IDF profile puts unrelated text near 0 and a restatement at
//! ~0.4–0.6. Every memory cut-off that depends on the scale — the recall
//! floor, the R1 boost gate, admission's bands, the consolidation cluster
//! threshold — asks this ONE question, so a new lexical provider is wired in
//! one place. Provider ROUTING (which embedder to call) is a different
//! question and stays a literal `== "BUILTIN"` at its sites, as v4's do.
//!
//! Pure.

/// Providers whose vectors are lexical (term-weight) rather than neural.
pub const LEXICAL_EMBEDDING_PROVIDERS: [&str; 1] = ["BUILTIN"];

/// True when `provider`'s vectors are on the lexical (TF-IDF) cosine scale.
///
/// v4 `!!provider && set.has(provider)`: an ABSENT or EMPTY provider is not
/// lexical, and the match is case-sensitive (`"builtin"` is neural).
pub fn is_lexical_embedding_provider(provider: Option<&str>) -> bool {
    match provider {
        Some(p) if !p.is_empty() => LEXICAL_EMBEDDING_PROVIDERS.contains(&p),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::is_lexical_embedding_provider;

    #[test]
    fn only_the_exact_builtin_string_is_lexical() {
        assert!(is_lexical_embedding_provider(Some("BUILTIN")));
        assert!(!is_lexical_embedding_provider(Some("builtin")));
        assert!(!is_lexical_embedding_provider(Some("")));
        assert!(!is_lexical_embedding_provider(None));
        assert!(!is_lexical_embedding_provider(Some("OPENAI")));
    }
}
