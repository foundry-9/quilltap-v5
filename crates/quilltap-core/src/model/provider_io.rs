//! The ONE provider→wire-family dispatch table (P4.13 unit 5).
//!
//! Before this module, the provider id string was independently matched in
//! three places — the request-builder dispatch, the response-parse dispatch,
//! and the stream-decoder flavor split — so adding or re-flavoring a provider
//! meant keeping three string matches in agreement by hand. Every wire-family
//! choice now keys off [`ProviderKind`]: one string-parse point, one table.
//!
//! What stays per-provider stays inside the family modules as narrow arms
//! (OpenRouter's zod re-emission both directions, google's URL/wire framing,
//! the Anthropic model-family boundary) — those are wire facts, not dispatch.

use super::response_parse::ChatFlavor;

/// The typed provider id — every provider the built-in manifest registry
/// declares. `of` is the single place a provider STRING becomes a wire-family
/// choice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProviderKind {
    Anthropic,
    DeepSeek,
    ZAi,
    OpenRouter,
    Ollama,
    OpenAi,
    Grok,
    Google,
    OpenAiCompatible,
    NanoGpt,
}

impl ProviderKind {
    /// Parse the canonical UPPERCASE provider id. `None` for an unknown
    /// provider — the builder errors there; the response parse falls back to
    /// the chat-completions base (v4's `default` plugin shape).
    pub fn of(provider: &str) -> Option<ProviderKind> {
        Some(match provider {
            "ANTHROPIC" => ProviderKind::Anthropic,
            "DEEPSEEK" => ProviderKind::DeepSeek,
            "Z_AI" => ProviderKind::ZAi,
            "OPENROUTER" => ProviderKind::OpenRouter,
            "OLLAMA" => ProviderKind::Ollama,
            "OPENAI" => ProviderKind::OpenAi,
            "GROK" => ProviderKind::Grok,
            "GOOGLE" => ProviderKind::Google,
            "OPENAI_COMPATIBLE" => ProviderKind::OpenAiCompatible,
            "NANOGPT" => ProviderKind::NanoGpt,
            _ => return None,
        })
    }

    /// The non-streaming parse flavor for the chat-completions family; `None`
    /// for providers whose parse is a dedicated family (anthropic / responses
    /// API / google / ollama).
    pub fn chat_parse_flavor(self) -> Option<ChatFlavor> {
        match self {
            ProviderKind::DeepSeek => Some(ChatFlavor::DeepSeek),
            ProviderKind::ZAi => Some(ChatFlavor::ZAi),
            ProviderKind::OpenRouter => Some(ChatFlavor::OpenRouter),
            ProviderKind::OpenAiCompatible => Some(ChatFlavor::OpenAiCompatible),
            ProviderKind::NanoGpt => Some(ChatFlavor::NanoGpt),
            _ => None,
        }
    }
}
