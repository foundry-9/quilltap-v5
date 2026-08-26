//! Server-side `qtap://` URI producers — v4 `lib/doc-edit/uri-producers.ts`.
//!
//! Build human-/model-facing URIs from resolved documents. They touch the DB (to
//! detect the self-vault and count name collisions) but stay free of the heavier
//! doc-edit modules. Every URI is an additive convenience, so any failure
//! degrades to `""` rather than throwing (v4's defensive `try/catch`).
//!
//! v4's functions are async; the Rust reads are synchronous over the MAIN
//! connection (`characters.findByIdRaw`) + the MOUNT connection
//! (`doc_mount_points.{countByName, findEnabled}`).

use rusqlite::Connection;

use super::path_resolver::{resolve_self_vault_mount_point_id, ResolvedPath};
use super::qtap_uri::{
    doc_store_authority, format_doc_store_uri, format_scoped_uri, format_self_uri, ScopedAuthority,
};
use super::DocEditScope;
use crate::db::doc_mount_points::DocMountPointsRepository;

/// Build a `qtap://` URI for a single document-store document from its mount
/// id/name (v4 `docStoreUriFor`). Prefers `qtap://self/…` for the acting
/// character's own vault, else the store name (UUID when the name is ambiguous).
#[allow(clippy::too_many_arguments)]
pub fn doc_store_uri_for(
    main: &Connection,
    mount: &Connection,
    mount_point_id: &str,
    mount_point_name: &str,
    relative_path: &str,
    character_id: Option<&str>,
    heading: Option<&str>,
    level: Option<u8>,
) -> String {
    if let Some(cid) = character_id {
        if !cid.is_empty() && !mount_point_id.is_empty() {
            if let Some(self_id) = resolve_self_vault_mount_point_id(main, Some(cid)) {
                if mount_point_id == self_id {
                    return format_self_uri(relative_path, heading, level);
                }
            }
        }
    }

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

/// Build the most stable readable URI for a resolved document (v4
/// `uriForResolvedPath`).
pub fn uri_for_resolved_path(
    main: &Connection,
    mount: &Connection,
    resolved: &ResolvedPath,
    character_id: Option<&str>,
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
            main,
            mount,
            resolved.mount_point_id.as_deref().unwrap_or(""),
            resolved.mount_point_name.as_deref().unwrap_or(""),
            &resolved.relative_path,
            character_id,
            heading,
            level,
        ),
    }
}

/// A precomputed, synchronous batch URI resolver (v4 `buildDocStoreUriResolver`) —
/// resolves the self-vault id + the set of ambiguous (duplicated) store names ONCE
/// so many URIs can be built without a per-row DB lookup.
pub struct DocStoreUriResolver {
    self_vault_id: Option<String>,
    /// Ambiguous store names, each `name.trim().to_lowercase()`.
    ambiguous: Vec<String>,
}

impl DocStoreUriResolver {
    /// Precompute (v4's async body). A degraded mount index leaves the ambiguity
    /// set empty (the name form is still readable).
    pub fn build(main: &Connection, mount: &Connection, character_id: Option<&str>) -> Self {
        let self_vault_id = resolve_self_vault_mount_point_id(main, character_id);
        DocStoreUriResolver {
            self_vault_id,
            ambiguous: collect_ambiguous_store_names(mount),
        }
    }

    /// Build a URI for a document-store row (v4's `uriForMount`).
    pub fn uri_for_mount(&self, name: &str, id: &str, relative_path: &str) -> String {
        if let Some(self_id) = &self.self_vault_id {
            if id == self_id.as_str() {
                return format_self_uri(relative_path, None, None);
            }
        }
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
///
/// ⚠ v4's `b220999d` also fixed a real bug here in passing: before the
/// extraction, `buildDocStoreUriResolver` computed the ambiguity set INSIDE the
/// same `try` as the self-vault resolution, so a throw from
/// `selfVaultMountPointId` left the set empty and every ambiguous name silently
/// resolved to its (ambiguous) name form. **v5 never had it** —
/// `DocStoreUriResolver::build` has always computed the two independently, and
/// `resolve_self_vault_mount_point_id` returns `Option` rather than throwing.
/// Nothing to port from that half; only the new `refForMount` sibling below.
fn collect_ambiguous_store_names(mount: &Connection) -> Vec<String> {
    let mut ambiguous: Vec<String> = Vec::new();
    if let Ok(enabled) = DocMountPointsRepository::new(mount).find_enabled_for_docedit() {
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
/// There is **no self-vault shorthand** here: `self` only means anything inside
/// a character's own prompt, and these references are handed to operator
/// surfaces.
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
