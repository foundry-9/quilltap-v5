//! Path resolver for document-editing tools — v4 `lib/doc-edit/path-resolver.ts`.
//!
//! Resolves a `{ scope, mount_point, relative_path }` address to a
//! [`ResolvedPath`]. This port covers the **database-backed** paths — the
//! `document_store` scope (over the tiered mount pool) and the `project` scope's
//! official-mount alias — with byte-exact [`PathResolutionError`] codes +
//! messages (they surface in tool output).
//!
//! ## Host-filesystem seam
//!
//! The legacy on-disk branches — a `filesystem`/`obsidian` mount's real path
//! (`fs.realpath` / `safeRealpath` / `verifyPathIsWithinBase`), the `project`
//! scope's legacy `<filesDir>/<projectId>/` fallback when no official mount is
//! provisioned, and the entire `general` scope — are host-filesystem and are a
//! **tracked deferral to the Phase-4 host** ([`FsSeam`]). Every store in the
//! differential corpus is `mountType: 'database'` (and every project has an
//! official database mount), so v4 returns early with `absolutePath: ''` and
//! never touches the disk; the seam is therefore never exercised by the diff.

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
    /// A host-filesystem branch (FS-backed mount, `general` scope, or the
    /// project legacy fallback). Never produced for the differential corpus.
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

/// Resolve a doc-edit path (v4 `resolveDocEditPath`). `relative_path` is
/// `Option` so a truncated tool call (arguments cut off → `path` undefined) hits
/// the same guard v4 does.
pub fn resolve_doc_edit_path(
    main: &Connection,
    mount: &Connection,
    scope: DocEditScope,
    relative_path: Option<&str>,
    context: &PathResolutionContext,
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
            resolve_document_store_path(main, mount, relative_path, context)
        }
        DocEditScope::Project => resolve_project_path(main, mount, relative_path, context),
        // The `general` scope is entirely host-filesystem.
        DocEditScope::General => Err(ResolveError::FsSeam),
    }
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

    // Filesystem-backed store → host FS seam.
    Err(ResolveError::FsSeam)
}

fn resolve_project_path(
    main: &Connection,
    mount: &Connection,
    relative_path: &str,
    context: &PathResolutionContext,
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
                // Filesystem official mount → host FS seam.
                return Err(ResolveError::FsSeam);
            }
        }
        // official mount missing / disabled → legacy FS fallback (seam).
    }

    // No official mount → legacy `<filesDir>/<projectId>/` fallback (seam).
    Err(ResolveError::FsSeam)
}

fn read_err(e: DbError) -> ResolveError {
    ResolveError::path(PathErrorCode::AccessDenied, e.to_string())
}
