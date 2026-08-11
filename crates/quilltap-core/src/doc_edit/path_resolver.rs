//! Path resolver for document-editing tools — v4 `lib/doc-edit/path-resolver.ts`.
//!
//! Resolves a `{ scope, mount_point, relative_path }` address to a
//! [`ResolvedPath`]. This port covers the **database-backed** paths — the
//! `document_store` scope (over the tiered mount pool) and the `project` scope's
//! official-mount alias — with byte-exact [`PathResolutionError`] codes +
//! messages (they surface in tool output).
//!
//! ## Host-filesystem branches (the `files_dir` thread — P4.6bg)
//!
//! The legacy on-disk branches — a `filesystem`/`obsidian` mount's real path
//! (`fs.realpath` / `safeRealpath` / `verifyPathIsWithinBase`), the `project`
//! scope's legacy `<filesDir>/<projectId>/` fallback when no official mount is
//! provisioned, and the entire `general` scope — reach the host filesystem. They
//! are gated by the `files_dir: Option<&Path>` thread the Phase-4 host supplies:
//! `Some(<base>/files)` makes the host disk available (v4's `getFilesDir()`), and
//! the branches run for real (byte-exact codes/messages); `None` preserves the
//! historic [`FsSeam`] refusal (the pre-P4.6bg behaviour, kept for the differential
//! corpora whose stores are all `mountType: 'database'`). Every fs branch is now
//! exercised by fs-backed differential coverage — see
//! `doc_edit_path_resolver_equivalence`.

use std::path::{Path, PathBuf};

use rusqlite::Connection;
use serde::Serialize;

use super::DocEditScope;
use crate::db::doc_mount_points::DocMountPointsRepository;
use crate::db::tiered_mount_pool::{
    flatten_tier_pool, resolve_tiered_mount_pool, FlattenScope, TierContext, TierResolveOptions,
};

/// Reserved `mount_point` token meaning "the acting character's own vault"
/// (v4 `SELF_VAULT_TOKEN`).
pub const SELF_VAULT_TOKEN: &str = "self";

/// The resolution context (v4 `PathResolutionContext`).
#[derive(Debug, Clone, Default)]
pub struct PathResolutionContext {
    pub project_id: Option<String>,
    pub character_id: Option<String>,
    pub character_ids: Vec<String>,
    /// Mount point name or ID (required for `document_store` scope).
    pub mount_point: Option<String>,
    /// Operator "look everywhere" override — reaches ANY enabled mount.
    pub operator_override: bool,
}

/// A resolved path (v4 `ResolvedPath`). For database-backed stores
/// `absolute_path`/`base_path` are empty and callers dispatch on `mount_type`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedPath {
    pub absolute_path: String,
    pub scope: DocEditScope,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mount_point_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mount_point_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mount_type: Option<String>,
    pub base_path: String,
    pub relative_path: String,
}

/// The path-resolution error codes (v4 `PathResolutionError.code`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathErrorCode {
    InvalidPath,
    AccessDenied,
    NotFound,
    MissingContext,
    TraversalAttempt,
}

impl PathErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            PathErrorCode::InvalidPath => "INVALID_PATH",
            PathErrorCode::AccessDenied => "ACCESS_DENIED",
            PathErrorCode::NotFound => "NOT_FOUND",
            PathErrorCode::MissingContext => "MISSING_CONTEXT",
            PathErrorCode::TraversalAttempt => "TRAVERSAL_ATTEMPT",
        }
    }
}

/// A resolution failure (v4 `PathResolutionError`) OR the host-filesystem seam
/// (a branch this port defers to the Phase-4 host — see the module docs).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolveError {
    Path {
        message: String,
        code: PathErrorCode,
    },
    /// The host filesystem is unavailable (`files_dir: None`): an FS-backed mount,
    /// the `general` scope, or the project legacy fallback was addressed on a host
    /// that supplies no files dir. When a files dir IS supplied these branches run
    /// for real; this refusal is only the `None` case.
    FsSeam,
}

impl ResolveError {
    fn path(code: PathErrorCode, message: impl Into<String>) -> Self {
        ResolveError::Path {
            code,
            message: message.into(),
        }
    }
}

/// A DB error surfaced from a repo read (v4 swallows some; we bubble read errors).
type DbError = crate::db::DbError;

/// Resolve the acting character's own vault mount-point id (v4
/// `resolveSelfVaultMountPointId`, `characters.findByIdRaw`). `None` on no
/// character / no vault / lookup failure.
pub fn resolve_self_vault_mount_point_id(
    main: &Connection,
    character_id: Option<&str>,
) -> Option<String> {
    let cid = character_id?;
    if cid.is_empty() {
        return None;
    }
    let acting = crate::db::characters_read::find_by_id_raw(main, cid)
        .ok()
        .flatten()?;
    // Archived characters keep a live vault (§4.2a prunes in place rather than
    // deleting), so the old "tombstone has no pointer → tools degrade" safety
    // no longer happens on its own. Refuse explicitly (v4 `d553f72a`,
    // `path-resolver.ts:65`): an archived character is read-only and must not
    // reach its own vault through doc_edit or the list/grep/blob handlers.
    // Returning `None` degrades with the same no-vault sentence those tools
    // have always produced.
    if crate::api::characters::is_archived(&acting) {
        return None;
    }
    acting
        .get("characterDocumentMountPointId")
        .and_then(|v| v.as_str())
        .map(String::from)
}

/// Translate a caller-supplied `mount_point` ref into the literal to match (v4
/// `resolveMountPointRef`): the reserved self-token (case-insensitive) → the
/// acting character's vault id; everything else passes through.
pub fn resolve_mount_point_ref(
    main: &Connection,
    mount_point_ref: &str,
    character_id: Option<&str>,
) -> String {
    if let Some(cid) = character_id {
        if !cid.is_empty() && mount_point_ref.to_lowercase() == SELF_VAULT_TOKEN {
            if let Some(own) = resolve_self_vault_mount_point_id(main, Some(cid)) {
                return own;
            }
        }
    }
    mount_point_ref.to_string()
}

/// Does the path contain a `..` traversal segment (v4 `hasTraversalSegments`,
/// posix `path.sep`)?
fn has_traversal_segments(p: &str) -> bool {
    p.split('/').any(|seg| seg == "..")
}

/// Is the path absolute (v4 `path.isAbsolute`, posix)?
fn is_absolute_path(p: &str) -> bool {
    p.starts_with('/')
}

// ============================================================================
// Host-filesystem helpers (v4 path-resolver.ts `safeRealpath` /
// `verifyPathIsWithinBase` + the POSIX `path.*` primitives they lean on).
// ============================================================================

/// POSIX `path.normalize` (lexical): collapse `//`, drop `.` segments, resolve
/// `..` against prior segments, preserve a leading `/` and a single trailing `/`.
/// Our inputs are already-absolute, `..`-free paths (the resolver rejected
/// traversal), so this is near-identity — but ported faithfully for the
/// containment string compare.
fn posix_normalize(p: &str) -> String {
    let is_absolute = p.starts_with('/');
    let has_trailing = p.len() > 1 && p.ends_with('/');
    let mut out: Vec<&str> = Vec::new();
    for seg in p.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                if let Some(&last) = out.last() {
                    if last != ".." {
                        out.pop();
                        continue;
                    }
                }
                if !is_absolute {
                    out.push("..");
                }
            }
            other => out.push(other),
        }
    }
    let joined = out.join("/");
    let mut result = if is_absolute {
        format!("/{joined}")
    } else if joined.is_empty() {
        ".".to_string()
    } else {
        joined
    };
    if has_trailing && !result.ends_with('/') {
        result.push('/');
    }
    result
}

/// POSIX `path.join(base, rel)` then normalize (v4 `path.join`). `base` is an
/// absolute dir, `rel` a clean relative path.
fn posix_join(base: &str, rel: &str) -> String {
    if rel.is_empty() {
        return posix_normalize(base);
    }
    let combined = if base.ends_with('/') {
        format!("{base}{rel}")
    } else {
        format!("{base}/{rel}")
    };
    posix_normalize(&combined)
}

/// v4 `safeRealpath` (`path-resolver.ts:175`): realpath a path, walking up to the
/// deepest existing ancestor when the leaf doesn't exist yet (a new file we're
/// about to write), realpath'ing THAT, then re-attaching the missing tail.
///
/// This keeps boundary checks correct on data directories that live behind a
/// symlink — e.g. `~/iCloud` on macOS, which resolves to
/// `~/Library/Mobile Documents/com~apple~CloudDocs`. Without the walk-up, the
/// file's realpath would expand the symlink while a missing sibling's
/// `path.resolve` would not, and the two sides of a containment check would
/// disagree even though both refer to the same tree. The multi-level walk-up
/// re-attaches the tail in v4's exact order (`join(realParent, ...tail.reverse(),
/// basename(current))`) so the two ports agree byte-for-byte.
fn safe_realpath(p: &Path) -> PathBuf {
    if let Ok(real) = std::fs::canonicalize(p) {
        return real;
    }
    // Walk up to the deepest existing ancestor and realpath that, then re-attach
    // the unresolved tail (v4's `path.dirname`/`path.basename`/`path.join`).
    let mut tail: Vec<std::ffi::OsString> = Vec::new();
    let mut current: PathBuf = p.to_path_buf();
    // v4 loops `while (current !== path.dirname(current))` — i.e. until the root,
    // whose dirname is itself (Rust: `Path::parent()` is `None`).
    while let Some(parent) = current.parent().map(Path::to_path_buf) {
        if let Ok(real_parent) = std::fs::canonicalize(&parent) {
            let mut out = real_parent;
            for seg in tail.iter().rev() {
                out.push(seg);
            }
            if let Some(base) = current.file_name() {
                out.push(base);
            }
            return out;
        }
        if let Some(base) = current.file_name() {
            tail.push(base.to_os_string());
        }
        current = parent;
    }
    p.to_path_buf()
}

/// v4 `verifyPathIsWithinBase` (`path-resolver.ts:203`): both args already
/// realpath'd; normalize, suffix the base with a separator, prefix-check.
fn verify_path_is_within_base(resolved_path: &str, base_dir: &str) -> bool {
    let normalized_resolved = posix_normalize(resolved_path);
    let normalized_base = posix_normalize(base_dir);
    let base_with_sep = if normalized_base.ends_with('/') {
        normalized_base.clone()
    } else {
        format!("{normalized_base}/")
    };
    normalized_resolved.starts_with(&base_with_sep) || normalized_resolved == normalized_base
}

/// The `general`/`project`-legacy base directory under the host files dir
/// (`files_dir` = v4 `getFilesDir()` = `<base>/files`). `None` when the host
/// supplies no files dir (the [`ResolveError::FsSeam`] refusal).
fn files_dir_or_seam(files_dir: Option<&Path>) -> Result<&Path, ResolveError> {
    files_dir.ok_or(ResolveError::FsSeam)
}

/// Resolve a doc-edit path (v4 `resolveDocEditPath`). `relative_path` is
/// `Option` so a truncated tool call (arguments cut off → `path` undefined) hits
/// the same guard v4 does.
pub fn resolve_doc_edit_path(
    main: &Connection,
    mount: &Connection,
    scope: DocEditScope,
    relative_path: Option<&str>,
    context: &PathResolutionContext,
    files_dir: Option<&Path>,
) -> Result<ResolvedPath, ResolveError> {
    let Some(relative_path) = relative_path else {
        return Err(ResolveError::path(
            PathErrorCode::InvalidPath,
            "A file path is required, but none was provided (the tool call may have been cut off before its arguments finished generating).",
        ));
    };

    if has_traversal_segments(relative_path) {
        return Err(ResolveError::path(
            PathErrorCode::TraversalAttempt,
            "Path contains traversal segments (..)",
        ));
    }
    if is_absolute_path(relative_path) {
        return Err(ResolveError::path(
            PathErrorCode::InvalidPath,
            "Path must be relative, not absolute",
        ));
    }

    match scope {
        DocEditScope::DocumentStore => {
            resolve_document_store_path(main, mount, relative_path, context, files_dir)
        }
        DocEditScope::Project => {
            resolve_project_path(main, mount, relative_path, context, files_dir)
        }
        // The `general` scope is entirely host-filesystem.
        DocEditScope::General => resolve_general_path(relative_path, files_dir),
    }
}

/// v4 `resolveGeneralPath` (`path-resolver.ts:571`): base =
/// `<filesDir>/_general`; realpath the base + the joined path, containment-check,
/// and return the on-disk `ResolvedPath`. `files_dir: None` → [`ResolveError::FsSeam`].
fn resolve_general_path(
    relative_path: &str,
    files_dir: Option<&Path>,
) -> Result<ResolvedPath, ResolveError> {
    let files_dir = files_dir_or_seam(files_dir)?;
    let base_dir = files_dir.join("_general");
    // ⚠ v5 robustness fix over a v4 LATENT QUIRK: v4's `resolveGeneralPath` never
    // creates `<files>/_general`, and nothing else does either (`ensureDataDir…`
    // makes `<files>` but not `_general`). When the base dir is absent, `safeRealpath`
    // walks up TWO missing levels and — via its reversed-tail join (the port
    // reproduces this exactly) — yields a mis-ordered path that fails the containment
    // check, so v4's general-scope new-blank throws "Path escapes general storage
    // boundary" on a FRESH instance. The mandate is that the general scope works
    // end-to-end, so v5 ensures the base dir exists (idempotent, best-effort) BEFORE
    // resolving. Inert in the differentials — every general fixture pre-creates
    // `_general`, so `create_dir_all` is a no-op there and the resolution stays
    // byte-identical to v4.
    let _ = std::fs::create_dir_all(&base_dir);
    let base_dir_str = base_dir.to_string_lossy().to_string();
    let joined = posix_join(&base_dir_str, relative_path);
    let real_base = safe_realpath(&base_dir);
    let real_path = safe_realpath(Path::new(&joined));
    let real_base_str = real_base.to_string_lossy().to_string();
    let real_path_str = real_path.to_string_lossy().to_string();

    if !verify_path_is_within_base(&real_path_str, &real_base_str) {
        return Err(ResolveError::path(
            PathErrorCode::TraversalAttempt,
            "Path escapes general storage boundary",
        ));
    }

    Ok(ResolvedPath {
        absolute_path: real_path_str,
        scope: DocEditScope::General,
        mount_point_id: None,
        mount_point_name: None,
        mount_type: None,
        base_path: real_base_str,
        relative_path: relative_path.to_string(),
    })
}

/// The accessible mount-point id set for a context (v4
/// `collectAccessibleMountPointIds`): operator override → every enabled store;
/// else the tiered pool flattened (participants included).
fn collect_accessible_mount_point_ids(
    main: &Connection,
    mount: &Connection,
    context: &PathResolutionContext,
) -> Result<Vec<String>, DbError> {
    if context.operator_override {
        let rows = DocMountPointsRepository::new(mount).find_enabled_for_docedit()?;
        let mut ids: Vec<String> = Vec::new();
        for r in rows {
            if !ids.contains(&r.id) {
                ids.push(r.id);
            }
        }
        return Ok(ids);
    }

    let pool = resolve_tiered_mount_pool(
        main,
        mount,
        &TierContext {
            character_id: context.character_id.clone(),
            character_ids: if context.character_ids.is_empty() {
                None
            } else {
                Some(context.character_ids.clone())
            },
            project_id: context.project_id.clone(),
            ..Default::default()
        },
        &TierResolveOptions {
            require_ownership: false,
            include_participants: true,
        },
    );
    Ok(flatten_tier_pool(&pool, FlattenScope::All, true))
}

fn resolve_document_store_path(
    main: &Connection,
    mount: &Connection,
    relative_path: &str,
    context: &PathResolutionContext,
    files_dir: Option<&Path>,
) -> Result<ResolvedPath, ResolveError> {
    let Some(mount_point_ref) = &context.mount_point else {
        return Err(ResolveError::path(
            PathErrorCode::MissingContext,
            "Mount point is required for document_store scope",
        ));
    };

    let has_character_context = context.character_id.is_some() || !context.character_ids.is_empty();
    if !context.operator_override && context.project_id.is_none() && !has_character_context {
        return Err(ResolveError::path(
            PathErrorCode::MissingContext,
            "Project ID or character ID is required for document_store scope",
        ));
    }

    let repo = DocMountPointsRepository::new(mount);
    let accessible_ids = collect_accessible_mount_point_ids(main, mount, context)
        .map_err(|e| ResolveError::path(PathErrorCode::AccessDenied, e.to_string()))?;

    if accessible_ids.is_empty() {
        return Err(ResolveError::path(
            PathErrorCode::AccessDenied,
            "No document stores accessible in this context",
        ));
    }

    let needle = mount_point_ref.to_lowercase();
    let mut matched: Option<crate::db::doc_mount_points::DmpRow> = None;

    // Reserved self-token: address the acting character's OWN vault via the DB link.
    if let Some(cid) = &context.character_id {
        if needle == SELF_VAULT_TOKEN {
            let own = resolve_self_vault_mount_point_id(main, Some(cid));
            if let Some(own_id) = &own {
                if accessible_ids.iter().any(|id| id == own_id) {
                    matched = repo.find_by_id_for_docedit(own_id).map_err(read_err)?;
                }
            }
            if matched.is_none() {
                return Err(ResolveError::path(
                    PathErrorCode::NotFound,
                    format!("No personal vault is available to address as \"{SELF_VAULT_TOKEN}\""),
                ));
            }
        }
    }

    // Name match (case-insensitive), then id match.
    if matched.is_none() {
        for id in &accessible_ids {
            if let Some(mp) = repo.find_by_id_for_docedit(id).map_err(read_err)? {
                if mp.name.to_lowercase() == needle {
                    matched = Some(mp);
                    break;
                }
            }
        }
    }
    if matched.is_none() {
        for id in &accessible_ids {
            if id == mount_point_ref {
                if let Some(mp) = repo.find_by_id_for_docedit(id).map_err(read_err)? {
                    matched = Some(mp);
                    break;
                }
            }
        }
    }

    let Some(mp) = matched else {
        return Err(ResolveError::path(
            PathErrorCode::NotFound,
            "Mount point not found or not accessible in this context",
        ));
    };

    if !mp.enabled {
        return Err(ResolveError::path(
            PathErrorCode::AccessDenied,
            "Mount point is disabled",
        ));
    }

    if mp.mount_type == "database" {
        return Ok(ResolvedPath {
            absolute_path: String::new(),
            scope: DocEditScope::DocumentStore,
            mount_point_id: Some(mp.id),
            mount_point_name: Some(mp.name),
            mount_type: Some("database".to_string()),
            base_path: String::new(),
            relative_path: relative_path.to_string(),
        });
    }

    // Filesystem-backed store: realpath the mount's base + joined path and
    // containment-check (v4 `path-resolver.ts:440-466`). The host disk must be
    // available (`files_dir: Some`); otherwise the FsSeam refusal stands.
    files_dir_or_seam(files_dir)?;
    let base_dir = mp.base_path.clone();
    let joined = posix_join(&base_dir, relative_path);
    let real_base = safe_realpath(Path::new(&base_dir));
    let real_path = safe_realpath(Path::new(&joined));
    let real_base_str = real_base.to_string_lossy().to_string();
    let real_path_str = real_path.to_string_lossy().to_string();
    if !verify_path_is_within_base(&real_path_str, &real_base_str) {
        return Err(ResolveError::path(
            PathErrorCode::TraversalAttempt,
            "Path escapes mount point boundary",
        ));
    }
    // v4 returns the RAW `baseDir` (mount.basePath) here, NOT `realBase`.
    Ok(ResolvedPath {
        absolute_path: real_path_str,
        scope: DocEditScope::DocumentStore,
        mount_point_id: Some(mp.id),
        mount_point_name: Some(mp.name),
        mount_type: Some(mp.mount_type),
        base_path: base_dir,
        relative_path: relative_path.to_string(),
    })
}

fn resolve_project_path(
    main: &Connection,
    mount: &Connection,
    relative_path: &str,
    context: &PathResolutionContext,
    files_dir: Option<&Path>,
) -> Result<ResolvedPath, ResolveError> {
    let Some(project_id) = &context.project_id else {
        return Err(ResolveError::path(
            PathErrorCode::MissingContext,
            "Project ID is required for project scope",
        ));
    };

    // v4 reads `projects.findById(projectId).officialMountPointId` — the slim
    // pointer lives in the MAIN db. When set + the mount is a database store, the
    // `project` scope is just an alias for that mount.
    let official = crate::db::projects::find_official_mount_point_id_raw(main, project_id)
        .map_err(read_err)?
        .flatten();

    if let Some(official_id) = official {
        let repo = DocMountPointsRepository::new(mount);
        if let Some(mp) = repo
            .find_by_id_for_docedit(&official_id)
            .map_err(read_err)?
        {
            if mp.enabled {
                if mp.mount_type == "database" {
                    return Ok(ResolvedPath {
                        absolute_path: String::new(),
                        scope: DocEditScope::Project,
                        mount_point_id: Some(mp.id),
                        mount_point_name: Some(mp.name),
                        mount_type: Some("database".to_string()),
                        base_path: String::new(),
                        relative_path: relative_path.to_string(),
                    });
                }
                // Filesystem official mount: realpath its base + joined path
                // (v4 `path-resolver.ts:511-534`). Requires host disk.
                files_dir_or_seam(files_dir)?;
                let base_dir = mp.base_path.clone();
                let joined = posix_join(&base_dir, relative_path);
                let real_base = safe_realpath(Path::new(&base_dir));
                let real_path = safe_realpath(Path::new(&joined));
                let real_base_str = real_base.to_string_lossy().to_string();
                let real_path_str = real_path.to_string_lossy().to_string();
                if !verify_path_is_within_base(&real_path_str, &real_base_str) {
                    return Err(ResolveError::path(
                        PathErrorCode::TraversalAttempt,
                        "Path escapes project boundary",
                    ));
                }
                // v4 returns `realBase` here (unlike the document_store fs branch).
                return Ok(ResolvedPath {
                    absolute_path: real_path_str,
                    scope: DocEditScope::Project,
                    mount_point_id: Some(mp.id),
                    mount_point_name: Some(mp.name),
                    mount_type: Some(mp.mount_type),
                    base_path: real_base_str,
                    relative_path: relative_path.to_string(),
                });
            }
        }
        // official mount missing / disabled → legacy FS fallback below.
    }

    // No official mount → legacy `<filesDir>/<projectId>/` fallback
    // (v4 `path-resolver.ts:541-565`). Requires host disk.
    let files_dir = files_dir_or_seam(files_dir)?;
    let base_dir = files_dir.join(project_id);
    let base_dir_str = base_dir.to_string_lossy().to_string();
    let joined = posix_join(&base_dir_str, relative_path);
    let real_base = safe_realpath(&base_dir);
    let real_path = safe_realpath(Path::new(&joined));
    let real_base_str = real_base.to_string_lossy().to_string();
    let real_path_str = real_path.to_string_lossy().to_string();
    if !verify_path_is_within_base(&real_path_str, &real_base_str) {
        return Err(ResolveError::path(
            PathErrorCode::TraversalAttempt,
            "Path escapes project boundary",
        ));
    }
    Ok(ResolvedPath {
        absolute_path: real_path_str,
        scope: DocEditScope::Project,
        mount_point_id: None,
        mount_point_name: None,
        mount_type: None,
        base_path: real_base_str,
        relative_path: relative_path.to_string(),
    })
}

fn read_err(e: DbError) -> ResolveError {
    ResolveError::path(PathErrorCode::AccessDenied, e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn posix_normalize_matches_node() {
        assert_eq!(posix_normalize("/a/b/c"), "/a/b/c");
        assert_eq!(posix_normalize("/a//b/./c"), "/a/b/c");
        assert_eq!(posix_normalize("/a/b/../c"), "/a/c");
        assert_eq!(posix_normalize("/a/b/"), "/a/b/");
        assert_eq!(posix_normalize("/"), "/");
    }

    #[test]
    fn posix_join_matches_node() {
        assert_eq!(posix_join("/base", "notes.md"), "/base/notes.md");
        assert_eq!(posix_join("/base/", "notes.md"), "/base/notes.md");
        assert_eq!(posix_join("/base", "sub/./x.md"), "/base/sub/x.md");
        assert_eq!(posix_join("/base", ""), "/base");
    }

    #[test]
    fn verify_within_base() {
        assert!(verify_path_is_within_base("/base/sub/x.md", "/base"));
        assert!(verify_path_is_within_base("/base", "/base"));
        assert!(!verify_path_is_within_base("/base-other/x.md", "/base"));
        assert!(!verify_path_is_within_base("/other/x.md", "/base"));
    }

    #[test]
    fn safe_realpath_walks_up_to_existing_ancestor() {
        // A missing leaf under an existing dir: realpath the parent + re-attach.
        let dir = std::env::temp_dir().join(format!("qt-dpr-srp-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let real_dir = std::fs::canonicalize(&dir).unwrap();
        let missing = dir.join("nope.md");
        assert_eq!(safe_realpath(&missing), real_dir.join("nope.md"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
