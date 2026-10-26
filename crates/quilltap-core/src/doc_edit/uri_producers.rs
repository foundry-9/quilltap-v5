//! Server-side `qtap://` URI producers — v4 `lib/doc-edit/uri-producers.ts`.
//!
//! Build human-/model-facing URIs from resolved documents. They touch the DB (to
//! count name collisions) but stay free of the heavier doc-edit modules. Every
//! URI is an additive convenience, so any failure degrades to `""` rather than
//! throwing (v4's defensive `try/catch`).
//!
//! ## No `self` here
//!
//! A produced address must not depend on who reads it. `qtap://self/…` means
//! the vault of whoever follows it, and these URIs land in tool results and
//! announcements that every character in a chat reads — so one character's
//! `qtap://self/metadata.json`, followed by another, opened the follower's own
//! file with no error (bug 256). Every producer here names the store (its UUID
//! when the name is ambiguous). A character may still TYPE `self` for its own
//! vault; the resolver honours it. Owner-only surfaces (the knowledge
//! injector, the Prospero vault whisper) may still teach the shorthand.
//!
//! v4's functions are async; the Rust reads are synchronous over the MOUNT
//! connection (`doc_mount_points.{countByName, findEnabled}`).

use rusqlite::Connection;

use super::path_resolver::ResolvedPath;
use super::qtap_uri::{
    doc_store_authority, format_doc_store_uri, format_scoped_uri, ScopedAuthority,
};
use super::DocEditScope;
use crate::db::doc_mount_points::DocMountPointsRepository;

/// Build a `qtap://` URI for a single document-store document from its mount
/// id/name (v4 `docStoreUriFor` — the form blob handlers and search hold
/// instead of a `ResolvedPath`): the store name, or its UUID when the name is
/// ambiguous — a character's own vault included. The URI is an additive
/// convenience, so any failure degrades to `""` rather than throwing.
pub fn doc_store_uri_for(
    mount: &Connection,
    mount_point_id: &str,
    mount_point_name: &str,
    relative_path: &str,
    heading: Option<&str>,
    level: Option<u8>,
) -> String {
    // Empty name → ambiguous (must use the UUID); else count collisions.
    let mut name_is_ambiguous = mount_point_name.is_empty();
    if !mount_point_name.is_empty() {
        name_is_ambiguous = DocMountPointsRepository::new(mount)
            .count_by_name(mount_point_name)
            .map(|n| n > 1)
            .unwrap_or(false);
    }
    format_doc_store_uri(
        mount_point_name,
        mount_point_id,
        relative_path,
        name_is_ambiguous,
        heading,
        level,
    )
}

/// Build a human-/model-facing `qtap://` URI for a single resolved document
/// (v4 `uriForResolvedPath`): project/general scope → the scoped authority; any
/// document store, a character's own vault included → its name (or UUID when
/// ambiguous). For multi-row tools that emit many URIs at once, prefer
/// [`DocStoreUriResolver`].
pub fn uri_for_resolved_path(
    mount: &Connection,
    resolved: &ResolvedPath,
    heading: Option<&str>,
    level: Option<u8>,
) -> String {
    match resolved.scope {
        DocEditScope::Project => format_scoped_uri(
            ScopedAuthority::Project,
            &resolved.relative_path,
            heading,
            level,
        ),
        DocEditScope::General => format_scoped_uri(
            ScopedAuthority::General,
            &resolved.relative_path,
            heading,
            level,
        ),
        DocEditScope::DocumentStore => doc_store_uri_for(
            mount,
            resolved.mount_point_id.as_deref().unwrap_or(""),
            resolved.mount_point_name.as_deref().unwrap_or(""),
            &resolved.relative_path,
            heading,
            level,
        ),
    }
}

/// A precomputed, synchronous batch URI resolver (v4 `buildDocStoreUriResolver`) —
/// resolves the set of ambiguous (duplicated) store names ONCE so many URIs can
/// be built without a per-row DB lookup.
pub struct DocStoreUriResolver {
    /// Ambiguous store names, each `name.trim().to_lowercase()`.
    ambiguous: Vec<String>,
}

impl DocStoreUriResolver {
    /// Precompute (v4's async body). A degraded mount index leaves the ambiguity
    /// set empty (the name form is still readable).
    pub fn build(mount: &Connection) -> Self {
        DocStoreUriResolver {
            ambiguous: collect_ambiguous_store_names(mount),
        }
    }

    /// Build a URI for a document-store row (v4's `uriForMount`).
    pub fn uri_for_mount(&self, name: &str, id: &str, relative_path: &str) -> String {
        let name_is_ambiguous = name.is_empty()
            || self
                .ambiguous
                .iter()
                .any(|k| *k == name.trim().to_lowercase());
        format_doc_store_uri(name, id, relative_path, name_is_ambiguous, None, None)
    }

    /// Build a URI for a project/general scope row (v4's `uriForScope`).
    pub fn uri_for_scope(&self, scope: ScopedAuthority, relative_path: &str) -> String {
        format_scoped_uri(scope, relative_path, None, None)
    }
}

/// The lower-cased names shared by more than one enabled store (v4
/// `collectAmbiguousStoreNames`, extracted at `b220999d`). A name in this set
/// can't address a store on its own, so producers fall back to the UUID.
/// Degrades to an empty set on a broken mount index — the readable name form is
/// still the better guess than nothing.
fn collect_ambiguous_store_names(mount: &Connection) -> Vec<String> {
    let mut ambiguous: Vec<String> = Vec::new();
    // P4.163: v4 `findEnabled()` (`uri-producers.ts:158`) is a fallback — the
    // filter line and `[]` (v5 had swallowed the failure with no line).
    {
        let enabled = crate::db::fallback::find_by_filter_or_empty("doc_mount_points", || {
            DocMountPointsRepository::new(mount).find_enabled_for_docedit()
        });
        let mut counts: Vec<(String, usize)> = Vec::new();
        for mp in &enabled {
            let key = mp.name.trim().to_lowercase();
            if let Some(entry) = counts.iter_mut().find(|(k, _)| *k == key) {
                entry.1 += 1;
            } else {
                counts.push((key, 1));
            }
        }
        for (key, count) in counts {
            if count > 1 {
                ambiguous.push(key);
            }
        }
    }
    ambiguous
}

/// The bare-reference sibling of [`DocStoreUriResolver`] (v4
/// `buildDocStoreRefResolver`, NEW at `b220999d`): hands back a synchronous
/// [`DocStoreRefResolver::ref_for_mount`] that yields the store **name**, or its
/// **UUID** when the name is ambiguous or reserved. Used where the consumer
/// wants an addressable store reference rather than a full `qtap://` URI — the
/// global search bar's document results, whose click targets and deep links
/// carry `mountPoint=<ref>`.
///
/// Like every producer here, it never yields `self` (see the module note).
pub struct DocStoreRefResolver {
    /// Ambiguous store names, each `name.trim().to_lowercase()`.
    ambiguous: Vec<String>,
}

impl DocStoreRefResolver {
    /// Precompute the ambiguity set once (v4's async body).
    pub fn build(mount: &Connection) -> Self {
        DocStoreRefResolver {
            ambiguous: collect_ambiguous_store_names(mount),
        }
    }

    /// v4 `refForMount(mountPointName, mountPointId)`. An EMPTY name is treated
    /// as ambiguous (the same guard `uri_for_mount` applies), so a nameless
    /// store still addresses by UUID. v4 wraps the call in a `try/catch`
    /// returning the id — unreachable here, since
    /// [`doc_store_authority`] cannot fail.
    pub fn ref_for_mount<'a>(&self, mount_point_name: &'a str, mount_point_id: &'a str) -> &'a str {
        let name_is_ambiguous = mount_point_name.is_empty()
            || self
                .ambiguous
                .iter()
                .any(|k| *k == mount_point_name.trim().to_lowercase());
        doc_store_authority(mount_point_name, mount_point_id, name_is_ambiguous)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resolver(ambiguous: &[&str]) -> DocStoreRefResolver {
        DocStoreRefResolver {
            ambiguous: ambiguous.iter().map(|s| s.to_string()).collect(),
        }
    }

    /// v4 `refForMount`'s three arms plus the empty-name guard. Names are
    /// returned UNTRIMMED; the ambiguity compare trims and lower-cases.
    #[test]
    fn ref_for_mount_arms() {
        let r = resolver(&["dupe"]);
        // 1. Unambiguous, unreserved → the name.
        assert_eq!(r.ref_for_mount("Airship Logs", "mp-1"), "Airship Logs");
        // 2. Ambiguous (two enabled stores share it) → the UUID.
        assert_eq!(r.ref_for_mount("Dupe", "mp-2"), "mp-2");
        assert_eq!(r.ref_for_mount("  dUpE  ", "mp-2b"), "mp-2b");
        // 3. Reserved authority → the UUID, even when unambiguous.
        assert_eq!(r.ref_for_mount("self", "mp-3"), "mp-3");
        assert_eq!(r.ref_for_mount("Project", "mp-4"), "mp-4");
        assert_eq!(r.ref_for_mount("General", "mp-5"), "mp-5");
        // The empty-name guard (v4 `!mountPointName`) → the UUID.
        assert_eq!(r.ref_for_mount("", "mp-6"), "mp-6");
    }

    /// No self-vault shorthand: the ref resolver never emits `self` for a
    /// store, however it was built (v4's design note — `self` only means
    /// anything inside a character's own prompt).
    #[test]
    fn ref_resolver_has_no_self_vault_shorthand() {
        let r = resolver(&[]);
        // Even the id that WOULD be a character's own vault addresses by name.
        assert_eq!(
            r.ref_for_mount("Ottoline's Vault", "vault-1"),
            "Ottoline's Vault"
        );
    }
}
