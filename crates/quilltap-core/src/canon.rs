//! Port of the pure canon-block renderers in v4's
//! lib/memory/cheap-llm-tasks/canon.ts.
//!
//! The "canon block" is the ALREADY ESTABLISHED section injected into a memory
//! extraction prompt so the extractor can skip facts already on file about the
//! subject. Two resolution paths:
//!   * SELF — a character extracting about themselves: their own vantage-point
//!     fields, rendered manifesto-first (the axiomatic floor), then personality,
//!     description, identity. Empty fields are dropped.
//!   * OTHER — a character extracting about another participant: a vault body
//!     rendered raw, falling back to the subject's labelled identity, then
//!     labelled description. Never personality or manifesto.
//!
//! Only the pure pieces live here. `loadCanonForObserverAboutSubject` is impure
//! (it reads the observer's vault) and stays above the port boundary.
//!
//! Commonplace feedback (v4 `d58548051`, memory-consolidation-and-tiers.md
//! §C6). The consolidation job mirrors each subject's hot digests into the
//! holder's vault at `Commonplace/<subject-name>.md` and `Commonplace/Self.md`.
//! Those files are appended to canon so a digested fact reads as ALREADY
//! ESTABLISHED and stops being re-mined:
//!
//!   - OTHER canon: the hand-written source above (vault `Others/` file, else
//!     identity, else description) **then** `Commonplace/<name>.md`.
//!   - SELF canon: the card fields **then** `Commonplace/Self.md`.
//!
//! The combined block is capped at [`CANON_BLOCK_TOKEN_CAP`]. Only the digest
//! yields to the cap — hand-written prose and card fields are never cut, so a
//! character with no Commonplace file renders byte-for-byte as before. The
//! loaders live in [`crate::services::memory_processor`].
//!
//! Every trim here is JS `trim()` ([`js_trim`]) — Rust's `str::trim` differs on
//! U+FEFF (JS strips it) and U+0085 (Rust strips it).

use crate::commonplace_file::truncate_to_token_budget;
use crate::jsstr::js_trim;
use crate::token_estimation::{estimate_tokens, DEFAULT_CHARS_PER_TOKEN};

/// Emitted when no canon is on file for a character.
pub const NO_CANON_FALLBACK: &str = "(no canonical identity recorded for this character yet)";

/// Token ceiling on one subject's whole ALREADY ESTABLISHED block (the hand
/// canon plus the Commonplace digest). Sized so a full 1,500-token digest file still fits
/// beside a modest hand-written note, while a multi-subject OTHER call (one
/// block per subject) cannot balloon past a few thousand tokens per subject.
/// The digest is what gives way; hand prose is never truncated.
pub const CANON_BLOCK_TOKEN_CAP: i64 = 2500;

/// Label introducing the Commonplace digest inside a canon block.
pub const COMMONPLACE_LABEL: &str = "[FROM THE COMMONPLACE BOOK]";

/// SELF-pass canon: the subject's own vantage-point fields, held separately so
/// the renderer can label each. Empty fields are dropped at render time.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelfCanon {
    pub character_id: String,
    pub character_name: String,
    pub manifesto: Option<String>,
    pub personality: Option<String>,
    pub description: Option<String>,
    pub identity: Option<String>,
    /// The character's own `Commonplace/Self.md` digest text, when one exists.
    /// Rendered after the card fields, trimmed to what
    /// [`CANON_BLOCK_TOKEN_CAP`] leaves.
    pub commonplace: Option<String>,
}

/// Which OTHER-pass source the `body` came from. `None` means nothing is on
/// file — the renderer emits [`NO_CANON_FALLBACK`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CanonSourceKind {
    Vault,
    Identity,
    Description,
    None,
}

impl CanonSourceKind {
    /// The v4 string value (`'vault' | 'identity' | 'description' | 'none'`) —
    /// the extraction driver's `canon=<source>` debug attribution.
    pub fn as_str(&self) -> &'static str {
        match self {
            CanonSourceKind::Vault => "vault",
            CanonSourceKind::Identity => "identity",
            CanonSourceKind::Description => "description",
            CanonSourceKind::None => "none",
        }
    }
}

/// OTHER-pass canon source. `body` carries the vault contents, the subject's
/// identity text, the subject's description text, or `None`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CanonSource {
    pub character_id: String,
    pub character_name: String,
    pub body: Option<String>,
    pub source: CanonSourceKind,
    /// The observer's `Commonplace/<subject>.md` digest text (frontmatter and
    /// heading stripped), when one exists. Rendered after `body`, trimmed to
    /// what [`CANON_BLOCK_TOKEN_CAP`] leaves.
    pub commonplace: Option<String>,
}

/// v4 `estimateTokens(text)` with no provider.
fn tokens(text: &str) -> i64 {
    estimate_tokens(text, DEFAULT_CHARS_PER_TOKEN)
}

/// v4 `appendCommonplace(block, commonplace)`: append the digest to an
/// already-rendered canon block, giving it only the tokens the hand-written
/// part left under [`CANON_BLOCK_TOKEN_CAP`]. Only the digest yields to the cap.
fn append_commonplace(block: String, commonplace: Option<&str>) -> String {
    let digest = commonplace.map(js_trim).unwrap_or("");
    if digest.is_empty() {
        return block;
    }
    let room = CANON_BLOCK_TOKEN_CAP - tokens(&block) - tokens(COMMONPLACE_LABEL) - 2;
    match truncate_to_token_budget(digest, room) {
        Some(fitted) if !fitted.is_empty() => format!("{block}\n{COMMONPLACE_LABEL}\n{fitted}"),
        _ => block,
    }
}

/// v4 `!!commonplace?.trim()`.
fn has_digest(commonplace: Option<&str>) -> bool {
    commonplace.is_some_and(|c| !js_trim(c).is_empty())
}

/// Render a SELF canon into the ALREADY ESTABLISHED block. Fields are labelled
/// and rendered manifesto-first, then personality, description, identity. Any
/// empty (after-trim) field is omitted; if none survive, the fallback line is
/// emitted. The rendered value is the trimmed form.
pub fn render_self_canon_block(canon: &SelfCanon) -> String {
    let fields: [(&str, &Option<String>); 4] = [
        ("MANIFESTO", &canon.manifesto),
        ("PERSONALITY", &canon.personality),
        ("DESCRIPTION", &canon.description),
        ("IDENTITY", &canon.identity),
    ];
    let mut lines: Vec<String> = Vec::new();
    for (label, value) in fields {
        if let Some(v) = value {
            let trimmed = js_trim(v);
            if !trimmed.is_empty() {
                lines.push(format!("[{label}] {trimmed}"));
            }
        }
    }
    // With a digest on file the fallback line would contradict it; drop it.
    let body = if !lines.is_empty() {
        lines.join("\n")
    } else if has_digest(canon.commonplace.as_deref()) {
        String::new()
    } else {
        NO_CANON_FALLBACK.to_string()
    };
    let head = if body.is_empty() {
        format!("ALREADY ESTABLISHED about {}", canon.character_name)
    } else {
        format!("ALREADY ESTABLISHED about {}\n{body}", canon.character_name)
    };
    append_commonplace(head, canon.commonplace.as_deref())
}

/// Render an OTHER canon into the ALREADY ESTABLISHED block. A vault body is
/// rendered raw (the observer's own authored notes); identity and description
/// fallbacks are labelled; absence (or an empty body for the named source)
/// emits the fallback line.
pub fn render_other_canon_block(canon: &CanonSource) -> String {
    let trimmed = canon.body.as_deref().map(js_trim).filter(|t| !t.is_empty());
    let body = match (&canon.source, trimmed) {
        (CanonSourceKind::Vault, Some(t)) => t.to_string(),
        (CanonSourceKind::Identity, Some(t)) => format!("[IDENTITY] {t}"),
        (CanonSourceKind::Description, Some(t)) => format!("[DESCRIPTION] {t}"),
        // Nothing hand-written, but the Commonplace Book has notes — let them
        // stand alone.
        _ if has_digest(canon.commonplace.as_deref()) => {
            return append_commonplace(
                format!("ALREADY ESTABLISHED about {}", canon.character_name),
                canon.commonplace.as_deref(),
            );
        }
        _ => NO_CANON_FALLBACK.to_string(),
    };
    append_commonplace(
        format!("ALREADY ESTABLISHED about {}\n{body}", canon.character_name),
        canon.commonplace.as_deref(),
    )
}

/// SELF-pass canon loader: the character's own vantage-point fields, no vault
/// lookup. In v4 the `?? null` normalises absent fields; the Rust `Option`
/// inputs already carry that distinction, so this is a straight construction.
/// `commonplace` is `None` — the vault-reading variant is
/// [`crate::services::canon_loaders::load_canon_for_self_with_commonplace`].
#[allow(clippy::too_many_arguments)]
pub fn load_canon_for_self(
    id: &str,
    name: &str,
    manifesto: Option<&str>,
    personality: Option<&str>,
    description: Option<&str>,
    identity: Option<&str>,
) -> SelfCanon {
    SelfCanon {
        character_id: id.to_string(),
        character_name: name.to_string(),
        manifesto: manifesto.map(str::to_string),
        personality: personality.map(str::to_string),
        description: description.map(str::to_string),
        identity: identity.map(str::to_string),
        commonplace: None,
    }
}
