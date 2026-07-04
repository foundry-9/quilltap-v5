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
use super::qtap_uri::{format_doc_store_uri, format_scoped_uri, format_self_uri, ScopedAuthority};
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
        DocStoreUriResolver {
            self_vault_id,
            ambiguous,
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
