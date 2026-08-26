//! `qtap://` Document URI codec — v4 `lib/doc-edit/qtap-uri.ts`.
//!
//! A `qtap://` URI is the first-class, human- and model-readable serialization of
//! the document-addressing triple `{ scope, mount_point, path }` that the path
//! resolver understands. The codec is **pure string work**: it never touches the
//! database and never resolves anything. It only turns a triple into a URI and
//! back.
//!
//! Grammar:
//! ```text
//! qtap-uri    = "qtap://" authority "/" path [ "#" fragment ] [ "?" query ]
//! authority   = reserved / store-ref
//! reserved    = "self" / "project" / "general"          ; case-insensitive
//! store-ref   = encoded-name / uuid
//! path        = path-segment *( "/" path-segment )       ; each segment percent-encoded
//! fragment    = encoded-heading [ ":" level ]            ; optional markdown heading anchor
//! query       = key "=" value *( "&" key "=" value )     ; reserved for future use
//! ```
//!
//! Reserved authorities always win in the authority slot; a real store literally
//! named `self`/`project`/`general` is reachable only by its UUID. This mirrors
//! the `SELF_VAULT_TOKEN` rule in the resolver.
//!
//! ## Producers
//!
//! The producer half (`format_self_uri` / `format_scoped_uri` /
//! `format_doc_store_uri`) is the canonical home for these emitters — the
//! knowledge injector (RAG rendering) and the `self_inventory` tool re-export them
//! from here, and the async DB-touching [`super::uri_producers`] build on them.

use super::DocEditScope;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// The `qtap://` scheme prefix (7 code units).
pub const QTAP_URI_SCHEME: &str = "qtap://";

/// The reserved authority for the acting character's own vault (must equal
/// `SELF_VAULT_TOKEN` in the path resolver).
const SELF_VAULT_TOKEN: &str = "self";

/// The reserved authorities that name a non-`document_store` scope (or the
/// self-vault). Matched case-insensitively in the authority slot.
const RESERVED_AUTHORITIES: &[&str] = &["self", "project", "general"];

/// The fully-decoded address a `qtap://` URI denotes (v4 `QtapUriParts`).
/// Serializes to v4's exact JSON shape (`{ scope, mountPoint?, path, heading?,
/// level?, query? }` — absent optionals omitted, matching JS `undefined`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QtapUriParts {
    pub scope: DocEditScope,
    /// Present only when `scope == DocumentStore`. `self` (SELF token) or a store
    /// name/UUID, verbatim and decoded.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub mount_point: Option<String>,
    /// Relative path within the store/scope. `""` means the store root.
    pub path: String,
    /// Optional markdown heading anchor (decoded).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub heading: Option<String>,
    /// Optional heading level (1–6) if the fragment carried `:N`.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub level: Option<u8>,
    /// Reserved for future use; parsed but currently unused. An
    /// **insertion-ordered** map (`serde_json::Map` under the crate's
    /// `preserve_order` feature) so it round-trips through `formatQtapUri` in the
    /// same key order v4's `Object.entries` yields; values are always strings.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub query: Option<Map<String, Value>>,
}

/// The error codes a malformed `qtap://` URI surfaces (v4 `QtapUriErrorCode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QtapUriErrorCode {
    NotAQtapUri,
    Malformed,
    EmptyAuthority,
    BadLevel,
}

impl QtapUriErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            QtapUriErrorCode::NotAQtapUri => "NOT_A_QTAP_URI",
            QtapUriErrorCode::Malformed => "MALFORMED",
            QtapUriErrorCode::EmptyAuthority => "EMPTY_AUTHORITY",
            QtapUriErrorCode::BadLevel => "BAD_LEVEL",
        }
    }
}

/// A parse error (v4 `QtapUriError`). Carries the byte-exact message + code that
/// v4 throws (both reach tool output).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QtapUriError {
    pub message: String,
    pub code: QtapUriErrorCode,
}

impl std::fmt::Display for QtapUriError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}
impl std::error::Error for QtapUriError {}

/// True iff the string starts with the `qtap://` scheme (case-insensitive, v4
/// `isQtapUri`).
pub fn is_qtap_uri(s: &str) -> bool {
    s.get(..QTAP_URI_SCHEME.len())
        .is_some_and(|p| p.eq_ignore_ascii_case(QTAP_URI_SCHEME))
}

/// Reproduce JavaScript's `encodeURIComponent`: percent-encode every byte EXCEPT
/// the unreserved set `A–Z a–z 0–9 - _ . ! ~ * ' ( )`. Multi-byte UTF-8 chars are
/// encoded byte-by-byte.
fn encode_uri_component(s: &str) -> String {
    fn is_unreserved(b: u8) -> bool {
        b.is_ascii_alphanumeric()
            || matches!(
                b,
                b'-' | b'_' | b'.' | b'!' | b'~' | b'*' | b'\'' | b'(' | b')'
            )
    }
    let mut out = String::with_capacity(s.len());
    for &b in s.as_bytes() {
        if is_unreserved(b) {
            out.push(b as char);
        } else {
            out.push('%');
            out.push_str(&format!("{b:02X}"));
        }
    }
    out
}

/// Reproduce JavaScript's `decodeURIComponent` throw/accept behaviour. Decodes
/// `%XX` runs as UTF-8 byte sequences (validating continuation bytes + the
/// resulting code point exactly as V8's `Decode`); literal characters pass
/// through. Returns `Err(())` on any malformed escape or invalid UTF-8 run —
/// the caller turns that into a [`QtapUriErrorCode::Malformed`].
fn decode_uri_component(component: &str) -> Result<String, ()> {
    let bytes = component.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0usize;
    // Read one `%XX` byte starting at index `at` ('%' at `at`); returns (byte,
    // next_index). Needs bytes[at+1] and bytes[at+2] to exist and be hex digits.
    let read_pct = |at: usize| -> Result<(u8, usize), ()> {
        if at + 3 > bytes.len() {
            return Err(());
        }
        let h1 = (bytes[at + 1] as char).to_digit(16).ok_or(())?;
        let h2 = (bytes[at + 2] as char).to_digit(16).ok_or(())?;
        Ok(((h1 * 16 + h2) as u8, at + 3))
    };
    while i < bytes.len() {
        let c = bytes[i];
        if c != b'%' {
            out.push(c);
            i += 1;
            continue;
        }
        // First escaped byte.
        let (b0, mut next) = read_pct(i)?;
        if b0 < 0x80 {
            out.push(b0);
            i = next;
            continue;
        }
        // Multi-byte lead: count leading 1-bits to get the sequence length (2..=4).
        let n = match b0 {
            0xC0..=0xDF => 2,
            0xE0..=0xEF => 3,
            0xF0..=0xF7 => 4,
            _ => return Err(()), // 0x80..=0xBF lone continuation, or 0xF8+ invalid lead
        };
        let mut seq = vec![b0];
        for _ in 1..n {
            if next >= bytes.len() || bytes[next] != b'%' {
                return Err(());
            }
            let (bx, after) = read_pct(next)?;
            if !(0x80..=0xBF).contains(&bx) {
                return Err(());
            }
            seq.push(bx);
            next = after;
        }
        // Validate the assembled sequence is a legal UTF-8 code point.
        match std::str::from_utf8(&seq) {
            Ok(s) if s.chars().count() == 1 => out.extend_from_slice(&seq),
            _ => return Err(()),
        }
        i = next;
    }
    String::from_utf8(out).map_err(|_| ())
}

/// v4 `safeDecode`: decode one component, mapping a malformed escape to a
/// [`QtapUriError`] with the component embedded (byte-exact message).
fn safe_decode(component: &str) -> Result<String, QtapUriError> {
    decode_uri_component(component).map_err(|_| QtapUriError {
        message: format!("Malformed percent-encoding in qtap:// URI segment: \"{component}\""),
        code: QtapUriErrorCode::Malformed,
    })
}

/// Decode the optional `#fragment` into `{ heading, level }` (v4 `parseFragment`).
fn parse_fragment(fragment: &str) -> Result<(Option<String>, Option<u8>), QtapUriError> {
    if fragment.is_empty() {
        return Ok((None, None));
    }
    match fragment.rfind(':') {
        None => Ok((Some(safe_decode(fragment)?), None)),
        Some(colon_idx) => {
            let heading_part = &fragment[..colon_idx];
            let level_part = &fragment[colon_idx + 1..];
            if level_part.is_empty() || !level_part.bytes().all(|b| b.is_ascii_digit()) {
                return Err(QtapUriError {
                    message: format!(
                        "Invalid heading level \"{level_part}\" in qtap:// fragment; expected an integer 1–6."
                    ),
                    code: QtapUriErrorCode::BadLevel,
                });
            }
            // parseInt(levelPart, 10): all-digit → an integer. Values ≤ 2^53 render
            // exactly (the corpus keeps levels small; astronomically-long digit
            // strings are a documented seam — parseInt's float rendering diverges).
            let level: u64 = level_part.parse().unwrap_or(u64::MAX);
            if !(1..=6).contains(&level) {
                return Err(QtapUriError {
                    message: format!(
                        "Heading level {level} out of range in qtap:// fragment; expected 1–6."
                    ),
                    code: QtapUriErrorCode::BadLevel,
                });
            }
            Ok((Some(safe_decode(heading_part)?), Some(level as u8)))
        }
    }
}

/// Decode the optional `?query` into a flat key→value map (v4 `parseQuery`).
/// Insertion order = appearance order (last write wins for a repeated key), so
/// the map round-trips through `formatQtapUri` byte-identically.
fn parse_query(query: &str) -> Result<Option<Map<String, Value>>, QtapUriError> {
    if query.is_empty() {
        return Ok(None);
    }
    let mut out: Map<String, Value> = Map::new();
    for pair in query.split('&') {
        if pair.is_empty() {
            continue;
        }
        match pair.find('=') {
            None => {
                out.insert(safe_decode(pair)?, Value::String(String::new()));
            }
            Some(eq) => {
                let k = safe_decode(&pair[..eq])?;
                let v = safe_decode(&pair[eq + 1..])?;
                out.insert(k, Value::String(v));
            }
        }
    }
    Ok(if out.is_empty() { None } else { Some(out) })
}

/// Parse a `qtap://` URI into its parts (v4 `parseQtapUri`). Pure string work.
pub fn parse_qtap_uri(uri: &str) -> Result<QtapUriParts, QtapUriError> {
    if !is_qtap_uri(uri) {
        return Err(QtapUriError {
            message: format!("Not a qtap:// URI: \"{uri}\""),
            code: QtapUriErrorCode::NotAQtapUri,
        });
    }

    // Strip scheme (fixed 7-char prefix) → split query → split fragment → split
    // authority/path.
    let mut rest = &uri[QTAP_URI_SCHEME.len()..];

    // Query (`?…`), trailing per the grammar.
    let mut query: Option<Map<String, Value>> = None;
    if let Some(q_idx) = rest.find('?') {
        query = parse_query(&rest[q_idx + 1..])?;
        rest = &rest[..q_idx];
    }

    // Fragment (`#…`).
    let mut heading: Option<String> = None;
    let mut level: Option<u8> = None;
    if let Some(hash_idx) = rest.find('#') {
        let (h, l) = parse_fragment(&rest[hash_idx + 1..])?;
        heading = h;
        level = l;
        rest = &rest[..hash_idx];
    }

    // Authority from path on the FIRST `/`.
    let (raw_authority, raw_path) = match rest.find('/') {
        None => (rest, ""),
        Some(slash_idx) => (&rest[..slash_idx], &rest[slash_idx + 1..]),
    };

    let authority = safe_decode(raw_authority)?;
    if authority.is_empty() {
        return Err(QtapUriError {
            message: "qtap:// URI has an empty authority.".to_string(),
            code: QtapUriErrorCode::EmptyAuthority,
        });
    }

    // Decode each path segment independently; never re-split or normalize.
    let path = if raw_path.is_empty() {
        String::new()
    } else {
        let mut segs: Vec<String> = Vec::new();
        for seg in raw_path.split('/') {
            segs.push(safe_decode(seg)?);
        }
        segs.join("/")
    };

    let lower = authority.to_lowercase();
    let (scope, mount_point) = if lower == SELF_VAULT_TOKEN {
        (
            DocEditScope::DocumentStore,
            Some(SELF_VAULT_TOKEN.to_string()),
        )
    } else if lower == "project" {
        (DocEditScope::Project, None)
    } else if lower == "general" {
        (DocEditScope::General, None)
    } else {
        (DocEditScope::DocumentStore, Some(authority))
    };

    Ok(QtapUriParts {
        scope,
        mount_point,
        path,
        heading,
        level,
        query,
    })
}

/// Encode the authority for a parsed parts object (v4 `encodeAuthority`).
fn encode_authority(parts: &QtapUriParts) -> String {
    match parts.scope {
        DocEditScope::Project => "project".to_string(),
        DocEditScope::General => "general".to_string(),
        DocEditScope::DocumentStore => {
            let mp = parts.mount_point.as_deref().unwrap_or("");
            if mp.to_lowercase() == SELF_VAULT_TOKEN {
                "self".to_string()
            } else {
                encode_uri_component(mp)
            }
        }
    }
}

/// Encode a relative path: split on `/`, percent-encode each segment, rejoin.
fn encode_path(path: &str) -> String {
    if path.is_empty() {
        return String::new();
    }
    path.split('/')
        .map(encode_uri_component)
        .collect::<Vec<_>>()
        .join("/")
}

/// Inverse of [`parse_qtap_uri`] (v4 `formatQtapUri`). Always emits the canonical
/// encoded form (authority + each path segment percent-encoded; `:` as `%3A`).
pub fn format_qtap_uri(parts: &QtapUriParts) -> String {
    let authority = encode_authority(parts);
    let mut out = format!("{QTAP_URI_SCHEME}{authority}/{}", encode_path(&parts.path));
    if let Some(heading) = &parts.heading {
        if !heading.is_empty() {
            out.push('#');
            out.push_str(&encode_uri_component(heading));
            if let Some(level) = parts.level {
                out.push(':');
                out.push_str(&level.to_string());
            }
        }
    }
    if let Some(query) = &parts.query {
        if !query.is_empty() {
            let q = query
                .iter()
                .map(|(k, v)| {
                    format!(
                        "{}={}",
                        encode_uri_component(k),
                        encode_uri_component(v.as_str().unwrap_or(""))
                    )
                })
                .collect::<Vec<_>>()
                .join("&");
            out.push('?');
            out.push_str(&q);
        }
    }
    out
}

/// Build the `{ scope, mount_point, path }` triple the path resolver expects from
/// a parsed URI (v4 `qtapUriToResolverInput`).
pub fn qtap_uri_to_resolver_input(parts: &QtapUriParts) -> (DocEditScope, Option<String>, String) {
    (parts.scope, parts.mount_point.clone(), parts.path.clone())
}

// ---------------------------------------------------------------------------
// Producer-side helpers — every emitter makes the name-vs-ID decision the same
// way. A producer holds a resolved document and wants a human-facing URI back.
// ---------------------------------------------------------------------------

/// The two non-`document_store` reserved authorities [`format_scoped_uri`]
/// accepts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScopedAuthority {
    Project,
    General,
}

impl ScopedAuthority {
    fn scope(self) -> DocEditScope {
        match self {
            ScopedAuthority::Project => DocEditScope::Project,
            ScopedAuthority::General => DocEditScope::General,
        }
    }
}

/// Pick the addressable reference for a document store: its **name** when that
/// reads unambiguously, else its **UUID** (v4 `docStoreAuthority`, extracted from
/// `formatDocStoreUri` at `b220999d`). The UUID wins when the caller says the
/// name is ambiguous (two enabled stores share it) OR when the name collides
/// with a reserved authority (`self`/`project`/`general`), since a store so
/// named is only reachable by UUID.
///
/// Every producer of a store reference goes through this — `qtap://` URIs via
/// [`format_doc_store_uri`], and the global search bar's document results via
/// [`super::uri_producers::DocStoreRefResolver`] — so a name that's safe in one
/// is safe in both. Both forms are accepted by the path resolver.
///
/// The returned name is the **untrimmed original**: only the collision test
/// trims and lower-cases.
pub fn doc_store_authority<'a>(
    mount_point_name: &'a str,
    mount_point_id: &'a str,
    name_is_ambiguous: bool,
) -> &'a str {
    let name_collides =
        RESERVED_AUTHORITIES.contains(&mount_point_name.trim().to_lowercase().as_str());
    // v4's `args.nameIsAmbiguous === true` is a STRICT compare against a
    // `boolean | undefined`; a Rust `bool` models exactly that domain.
    let use_id = name_is_ambiguous || name_collides;
    if use_id {
        mount_point_id
    } else {
        mount_point_name
    }
}

/// Build a human-facing `qtap://` URI for a `document_store` document (v4
/// `formatDocStoreUri`). The authority is chosen by [`doc_store_authority`]: the
/// store name, or its UUID when the name is ambiguous or reserved.
pub fn format_doc_store_uri(
    mount_point_name: &str,
    mount_point_id: &str,
    path: &str,
    name_is_ambiguous: bool,
    heading: Option<&str>,
    level: Option<u8>,
) -> String {
    let mp = doc_store_authority(mount_point_name, mount_point_id, name_is_ambiguous);
    format_qtap_uri(&QtapUriParts {
        scope: DocEditScope::DocumentStore,
        mount_point: Some(mp.to_string()),
        path: path.to_string(),
        heading: heading.map(String::from),
        level,
        query: None,
    })
}

/// Build a `qtap://` URI for project/general scope (v4 `formatScopedUri`).
pub fn format_scoped_uri(
    scope: ScopedAuthority,
    path: &str,
    heading: Option<&str>,
    level: Option<u8>,
) -> String {
    format_qtap_uri(&QtapUriParts {
        scope: scope.scope(),
        mount_point: None,
        path: path.to_string(),
        heading: heading.map(String::from),
        level,
        query: None,
    })
}

/// Build the canonical self-vault URI `qtap://self/<path>` (v4 `formatSelfUri`).
pub fn format_self_uri(path: &str, heading: Option<&str>, level: Option<u8>) -> String {
    format_qtap_uri(&QtapUriParts {
        scope: DocEditScope::DocumentStore,
        mount_point: Some(SELF_VAULT_TOKEN.to_string()),
        path: path.to_string(),
        heading: heading.map(String::from),
        level,
        query: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn self_uri_encodes_segments() {
        assert_eq!(
            format_self_uri("Knowledge/My File.md", None, None),
            "qtap://self/Knowledge/My%20File.md"
        );
        assert_eq!(format_self_uri("", None, None), "qtap://self/");
    }

    /// v4 `docStoreAuthority`'s three arms + the untrimmed-name return
    /// (`b220999d`). The extraction must be neutral for `format_doc_store_uri`
    /// (pinned by `qtap_uri_equivalence`), and correct on its own for the
    /// search bar's `mountPointRef`.
    #[test]
    fn doc_store_authority_picks_name_or_uuid() {
        // Unambiguous, unreserved → the name, VERBATIM (untrimmed).
        assert_eq!(
            doc_store_authority("Quilltap General", "mp-1", false),
            "Quilltap General"
        );
        assert_eq!(
            doc_store_authority("  Padded  ", "mp-1", false),
            "  Padded  "
        );
        // Reserved authority (trimmed + lower-cased compare) → the UUID.
        assert_eq!(doc_store_authority("Project", "mp-2", false), "mp-2");
        assert_eq!(doc_store_authority(" SELF ", "mp-2", false), "mp-2");
        assert_eq!(doc_store_authority("general", "mp-2", false), "mp-2");
        // Caller-declared ambiguity → the UUID.
        assert_eq!(doc_store_authority("Dupe", "mp-3", true), "mp-3");
        // A name that merely CONTAINS a reserved word is not reserved.
        assert_eq!(
            doc_store_authority("Project Notes", "mp-4", false),
            "Project Notes"
        );
    }

    #[test]
    fn doc_store_prefers_name_falls_back_to_id() {
        assert_eq!(
            format_doc_store_uri(
                "Quilltap General",
                "mp-1",
                "Knowledge/a.md",
                false,
                None,
                None
            ),
            "qtap://Quilltap%20General/Knowledge/a.md"
        );
        assert_eq!(
            format_doc_store_uri("Project", "mp-2", "x.md", false, None, None),
            "qtap://mp-2/x.md"
        );
        assert_eq!(
            format_doc_store_uri("Dupe", "mp-3", "x.md", true, None, None),
            "qtap://mp-3/x.md"
        );
    }

    #[test]
    fn scoped_uri_emits_reserved_authority() {
        assert_eq!(
            format_scoped_uri(ScopedAuthority::Project, "Notes/a b.md", None, None),
            "qtap://project/Notes/a%20b.md"
        );
        assert_eq!(
            format_scoped_uri(ScopedAuthority::General, "", None, None),
            "qtap://general/"
        );
    }

    #[test]
    fn round_trip_with_heading_and_level() {
        let parts = parse_qtap_uri("qtap://self/Knowledge/a.md#Intro%3A%20Notes:2").unwrap();
        assert_eq!(parts.scope, DocEditScope::DocumentStore);
        assert_eq!(parts.mount_point.as_deref(), Some("self"));
        assert_eq!(parts.path, "Knowledge/a.md");
        assert_eq!(parts.heading.as_deref(), Some("Intro: Notes"));
        assert_eq!(parts.level, Some(2));
        assert_eq!(
            format_qtap_uri(&parts),
            "qtap://self/Knowledge/a.md#Intro%3A%20Notes:2"
        );
    }

    #[test]
    fn parse_errors() {
        assert_eq!(
            parse_qtap_uri("not-a-uri").unwrap_err().code,
            QtapUriErrorCode::NotAQtapUri
        );
        assert_eq!(
            parse_qtap_uri("qtap:///x").unwrap_err().code,
            QtapUriErrorCode::EmptyAuthority
        );
        assert_eq!(
            parse_qtap_uri("qtap://self/a#h:9").unwrap_err().code,
            QtapUriErrorCode::BadLevel
        );
        assert_eq!(
            parse_qtap_uri("qtap://self/a%2").unwrap_err().code,
            QtapUriErrorCode::Malformed
        );
    }
}
