//! The `qtap://` URI producer-side helpers the knowledge injector needs — v4
//! `lib/doc-edit/qtap-uri.ts` (`formatSelfUri` / `formatDocStoreUri`, built on the
//! canonical `formatQtapUri` emitter).
//!
//! A `qtap://` URI is the human- and model-readable serialization of a document
//! address `{ scope, mount_point, path }`. This is **pure string work** — it never
//! touches the database and never resolves anything.
//!
//! Only the *producer* half (URI emission) is ported here, since that is all the
//! knowledge injector consumes: the parser (`parseQtapUri`) and the full codec are
//! out of scope for this wave. **NOTE:** these helpers live under the knowledge
//! injector because it is their only consumer today; they may be promoted to a
//! shared `doc_edit` module when the tool subsystem (wave 4) lands.
//!
//! ## Faithful v4 shapes
//!
//! * The scheme is the fixed 7-char `qtap://`.
//! * The self-vault authority is the reserved token `self` (SELF_VAULT_TOKEN);
//!   `formatSelfUri` always emits it.
//! * `formatDocStoreUri` prefers the store **name**, falling back to the **UUID**
//!   when the name is flagged ambiguous OR collides (case-insensitive, trimmed)
//!   with a reserved authority (`self`/`project`/`general`) — those are reachable
//!   only by UUID.
//! * Authority and every path segment are percent-encoded with the exact
//!   `encodeURIComponent` set (see [`encode_uri_component`]) and joined with a
//!   literal `/`.

/// The reserved self-vault authority (`SELF_VAULT_TOKEN` in v4's path resolver).
const SELF_VAULT_TOKEN: &str = "self";

/// The reserved authorities that name a non-`document_store` scope (or the
/// self-vault). Matched case-insensitively (after trim) in the authority slot.
const RESERVED_AUTHORITIES: &[&str] = &["self", "project", "general"];

/// Reproduce JavaScript's `encodeURIComponent`: percent-encode every byte EXCEPT
/// the unreserved set `A–Z a–z 0–9 - _ . ! ~ * ' ( )`. Multi-byte UTF-8 characters
/// are encoded byte-by-byte (their UTF-8 bytes are each non-unreserved).
fn encode_uri_component(s: &str) -> String {
    // encodeURIComponent's literal (never-encoded) set.
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

/// Encode a relative path: split on `/`, percent-encode each segment, rejoin.
/// (v4 `encodePath`.)
fn encode_path(path: &str) -> String {
    if path.is_empty() {
        return String::new();
    }
    path.split('/')
        .map(encode_uri_component)
        .collect::<Vec<_>>()
        .join("/")
}

/// The canonical `qtap://<authority>/<path>` emitter (v4 `formatQtapUri`, restricted
/// to the fields the producer helpers use — no heading/level/query, which the
/// knowledge injector never passes).
fn format_qtap_uri(authority: &str, path: &str) -> String {
    format!("qtap://{authority}/{}", encode_path(path))
}

/// Build the canonical self-vault URI: `qtap://self/<path>` (v4 `formatSelfUri`).
pub fn format_self_uri(path: &str) -> String {
    format_qtap_uri(SELF_VAULT_TOKEN, path)
}

/// Build a human-facing `qtap://` URI for a `document_store` document (v4
/// `formatDocStoreUri`). Prefers the store **name**; falls back to the **UUID**
/// when `name_is_ambiguous` OR when the trimmed, lower-cased name collides with a
/// reserved authority.
pub fn format_doc_store_uri(
    mount_point_name: &str,
    mount_point_id: &str,
    path: &str,
    name_is_ambiguous: bool,
) -> String {
    let name_collides =
        RESERVED_AUTHORITIES.contains(&mount_point_name.trim().to_lowercase().as_str());
    let use_id = name_is_ambiguous || name_collides;
    let mp = if use_id {
        mount_point_id
    } else {
        mount_point_name
    };
    // document_store authority: `self` stays reserved, else percent-encode the name.
    let authority = if mp.to_lowercase() == SELF_VAULT_TOKEN {
        SELF_VAULT_TOKEN.to_string()
    } else {
        encode_uri_component(mp)
    };
    format_qtap_uri(&authority, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn self_uri_encodes_segments() {
        assert_eq!(
            format_self_uri("Knowledge/My File.md"),
            "qtap://self/Knowledge/My%20File.md"
        );
        assert_eq!(format_self_uri(""), "qtap://self/");
    }

    #[test]
    fn doc_store_prefers_name_falls_back_to_id() {
        // Normal store name — used verbatim (percent-encoded).
        assert_eq!(
            format_doc_store_uri("Quilltap General", "mp-1", "Knowledge/a.md", false),
            "qtap://Quilltap%20General/Knowledge/a.md"
        );
        // Reserved-name collision → UUID.
        assert_eq!(
            format_doc_store_uri("Project", "mp-2", "x.md", false),
            "qtap://mp-2/x.md"
        );
        // Ambiguous flag → UUID.
        assert_eq!(
            format_doc_store_uri("Dupe", "mp-3", "x.md", true),
            "qtap://mp-3/x.md"
        );
    }
}
