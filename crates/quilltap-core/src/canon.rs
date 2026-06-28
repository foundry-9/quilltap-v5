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

/// Emitted when no canon is on file for a character.
pub const NO_CANON_FALLBACK: &str = "(no canonical identity recorded for this character yet)";

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

/// OTHER-pass canon source. `body` carries the vault contents, the subject's
/// identity text, the subject's description text, or `None`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CanonSource {
    pub character_id: String,
    pub character_name: String,
    pub body: Option<String>,
    pub source: CanonSourceKind,
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
            let trimmed = v.trim();
            if !trimmed.is_empty() {
                lines.push(format!("[{label}] {trimmed}"));
            }
        }
    }
    let body = if lines.is_empty() {
        NO_CANON_FALLBACK.to_string()
    } else {
        lines.join("\n")
    };
    format!("ALREADY ESTABLISHED about {}\n{body}", canon.character_name)
}

/// Render an OTHER canon into the ALREADY ESTABLISHED block. A vault body is
/// rendered raw (the observer's own authored notes); identity and description
/// fallbacks are labelled; absence (or an empty body for the named source)
/// emits the fallback line.
pub fn render_other_canon_block(canon: &CanonSource) -> String {
    let trimmed = canon
        .body
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty());
    let body = match (&canon.source, trimmed) {
        (CanonSourceKind::Vault, Some(t)) => t.to_string(),
        (CanonSourceKind::Identity, Some(t)) => format!("[IDENTITY] {t}"),
        (CanonSourceKind::Description, Some(t)) => format!("[DESCRIPTION] {t}"),
        _ => NO_CANON_FALLBACK.to_string(),
    };
    format!("ALREADY ESTABLISHED about {}\n{body}", canon.character_name)
}

/// SELF-pass canon loader: the character's own vantage-point fields, no vault
/// lookup. In v4 the `?? null` normalises absent fields; the Rust `Option`
/// inputs already carry that distinction, so this is a straight construction.
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
    }
}
