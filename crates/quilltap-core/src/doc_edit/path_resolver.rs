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
    flatten_tier_pool, resolve_tiered_mount_pool, FlattenOptions, TierContext, TierResolveOptions,
    TieredMountPool,
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
    /// The doc-tool opacity covenant: hide every CHARACTER VAULT (the acting
    /// character's own and every peer's) while leaving the group, project and
    /// global tiers reachable. Set by the doc-edit context builders for a
    /// character with `systemTransparency !== true`.
    ///
    /// `character_id` must still be supplied alongside it — group membership is
    /// derived from `character_id` and from nothing else, so hiding vaults by
    /// withholding the character instead of setting this flag also erases every
    /// group store she belongs to (v4 bug 152, `1065a1f53`). The reserved `self`
    /// token is refused while this is set, since her own vault is among what's
    /// hidden.
    pub hide_character_vaults: bool,
    /// Mount point name or ID (required for `document_store` scope).
    pub mount_point: Option<String>,
    /// Operator "look everywhere" override — reaches ANY enabled mount.
    pub operator_override: bool,
    /// A pre-built pool that IS the accessible set — used by a tool loop that
    /// must see "what this chat could see" before the chat exists (the Scenario
    /// Builder; v4 `PathResolutionContext.mountPool`, `d1c06cd9d`). When set,
    /// resolution never consults `character_id` / `project_id` for the pool, and
    /// the participant tier is admitted. Mutually exclusive with
    /// `operator_override`.
    pub mount_pool: Option<TieredMountPool>,
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
/// else the tiered pool flattened — the participant AND character tiers included
/// only while vaults are visible; the opacity covenant subtracts both (v4 bug 152).
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
        // v4 logs this arm too; the port had never carried the line (restored
        // beside the pre-built-pool arm's, P4.D216).
        tracing::debug!(
            count = ids.len(),
            "Path resolver: operator override — all enabled stores accessible"
        );
        return Ok(ids);
    }

    // A pre-built pool (Scenario Builder) is the accessible set, verbatim. The
    // cast vaults ride in the participant tier; there is no character tier.
    // v4 `d1c06cd9d` — AFTER the operator arm, BEFORE the covenant: the pool is
    // not subject to the opacity covenant (the caller built it).
    if let Some(pool) = &context.mount_pool {
        return Ok(prebuilt_pool_accessible_ids(pool));
    }

    // The opacity covenant subtracts the two vault tiers and nothing else. The
    // character still goes INTO the pool so her group stores resolve — that is
    // the whole point of expressing this as a subtraction (v4 bug 152).
    let vaults_visible = !context.hide_character_vaults;
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
            include_participants: vaults_visible,
        },
    );
    Ok(flatten_tier_pool(
        &pool,
        FlattenOptions {
            include_participants: vaults_visible,
            include_character_tier: vaults_visible,
            ..Default::default()
        },
    ))
}

/// The pre-built-pool arm of v4 `collectAccessibleMountPointIds` (`d1c06cd9d`):
/// `flattenTierPool(mountPool, { includeParticipants: true })` — every tier, the
/// participant tier folded in, `includeCharacterTier` left at its default (the
/// feature spec's §5.2 said to set it false; that would drop the participant
/// tier too, since both live inside `addCharacterTier` — the shipped code does
/// not, and the port follows the code). Shared with the enumeration side
/// (`tools::doc_edit::shared::get_accessible_mount_points`), which v4 routes
/// through the same function — so the DEBUG fires on both.
pub(crate) fn prebuilt_pool_accessible_ids(pool: &TieredMountPool) -> Vec<String> {
    let ids = flatten_tier_pool(
        pool,
        FlattenOptions {
            include_participants: true,
            ..Default::default()
        },
    );
    tracing::debug!(
        count = ids.len(),
        "Path resolver: pre-built mount pool — accessible set supplied by caller"
    );
    ids
}

/// v4 `describeCharacters`: the context's character ids as one comma-joined
/// string for the refusal warns, or `none` when empty. v4 builds a `Set` seeded
/// with `characterId` and then every `characterIds` entry, so INSERTION order is
/// preserved and duplicates drop.
fn describe_characters(context: &PathResolutionContext) -> String {
    let mut ids: Vec<&str> = Vec::new();
    // v4 `if (context.characterId)` is JS truthiness: an EMPTY string is skipped
    // (the `8f910137`-round audit shape — `is_some()` is not `x ? …` over a string).
    if let Some(cid) = context.character_id.as_deref().filter(|s| !s.is_empty()) {
        ids.push(cid);
    }
    for id in &context.character_ids {
        if !ids.contains(&id.as_str()) {
            ids.push(id);
        }
    }
    if ids.is_empty() {
        "none".to_string()
    } else {
        ids.join(",")
    }
}

/// v4 `findEnabledMountPointByRef`: find an enabled store matching `ref` (name,
/// case-insensitively, then id) ANYWHERE, ignoring scope — used only to tell "no
/// such store" apart from "exists, out of scope" in the refusal.
///
/// **Character vaults are deliberately excluded.** A vault is exactly what the
/// opacity covenant and the cross-character boundary hide, so admitting one
/// exists would leak through the refusal what the access rule withholds — the
/// same reason `assert_character_may_read` mirrors the "missing file" shape. A
/// vault therefore keeps the indistinguishable NOT_FOUND. Fails soft: on any
/// lookup error the caller falls back to NOT_FOUND (v4's `catch`, and its
/// `findEnabled` is itself a `safeQuery` with an empty-array fallback).
fn find_enabled_mount_point_by_ref(mount: &Connection, r#ref: &str) -> Option<(String, String)> {
    let rows = DocMountPointsRepository::new(mount)
        .find_enabled_for_search()
        .ok()?;
    let needle = r#ref.to_lowercase();
    let matched = rows
        .iter()
        .find(|mp| mp.name.to_lowercase() == needle)
        .or_else(|| rows.iter().find(|mp| mp.id == r#ref))?;
    if matched.store_type.as_deref() == Some("character") {
        return None;
    }
    Some((matched.id.clone(), matched.name.clone()))
}

fn resolve_document_store_path(
    main: &Connection,
    mount: &Connection,
    relative_path: &str,
    context: &PathResolutionContext,
    files_dir: Option<&Path>,
) -> Result<ResolvedPath, ResolveError> {
    let Some(mount_point_ref) = &context.mount_point else {
        tracing::warn!("document_store scope requires mountPoint in context");
        return Err(ResolveError::path(
            PathErrorCode::MissingContext,
            "Mount point is required for document_store scope",
        ));
    };

    let has_character_context = context.character_id.is_some() || !context.character_ids.is_empty();
    // The operator override AND a pre-built pool each carry their own accessible
    // set, so neither needs a project or a character (v4 `d1c06cd9d`).
    if !context.operator_override
        && context.mount_pool.is_none()
        && context.project_id.is_none()
        && !has_character_context
    {
        tracing::warn!("document_store scope requires projectId or characterId in context");
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
    // `hide_character_vaults` covers her OWN vault too, so the token is refused
    // explicitly here. It used to be refused as a side effect of the opacity gate
    // withholding `character_id` — the same withholding that hid her group stores
    // (v4 bug 152). Stating it keeps the refusal once the character stays. NOTE
    // (§R.4): bug 152's commit message says the token "is now refused explicitly";
    // the shipped hunk adds a CONDITION to this existing gate rather than a new
    // refusal arm, so a hidden-vault `self` falls through to the loops below and
    // ends in the ordinary not-found answer.
    if let Some(cid) = &context.character_id {
        if !context.hide_character_vaults && needle == SELF_VAULT_TOKEN {
            let own = resolve_self_vault_mount_point_id(main, Some(cid));
            if let Some(own_id) = &own {
                if accessible_ids.iter().any(|id| id == own_id) {
                    matched = repo.find_by_id_for_docedit(own_id).map_err(read_err)?;
                }
            }
            if matched.is_none() {
                tracing::warn!(
                    character_id = %cid,
                    "Self-token resolution failed: no accessible vault for character"
                );
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
        // "No such store" and "exists, but out of scope here" are different
        // answers, and collapsing them into one NOT_FOUND is what turned bug 152
        // into eight minutes of guesswork: a model reads NOT_FOUND as a typo and
        // rationally tries another spelling. Say which wall it is, so an
        // unreachable store ends the loop instead of feeding it.
        let project_display = context.project_id.as_deref().unwrap_or("none");
        let characters = describe_characters(context);
        let vaults_hidden = context.hide_character_vaults;
        if let Some((existing_id, existing_name)) =
            find_enabled_mount_point_by_ref(mount, mount_point_ref)
        {
            tracing::warn!(
                mount_point = %existing_name,
                mount_point_id = %existing_id,
                project_id = %project_display,
                characters = %characters,
                vaults_hidden,
                "Mount point exists but is out of scope: {existing_name} ({existing_id}) (project: {project_display}, characters: {characters}, vaultsHidden: {vaults_hidden})"
            );
            return Err(ResolveError::path(
                PathErrorCode::AccessDenied,
                format!(
                    "The document store \"{existing_name}\" exists but is not reachable from this conversation. \
                     It is not linked to this project, and it is not one of your own group's stores. \
                     Retrying with a different spelling will not help — use doc_list_files with no path to see the stores you can reach."
                ),
            ));
        }
        tracing::warn!(
            mount_point = %mount_point_ref,
            project_id = %project_display,
            characters = %characters,
            vaults_hidden,
            "Mount point not found or not accessible: {mount_point_ref} (project: {project_display}, characters: {characters}, vaultsHidden: {vaults_hidden})"
        );
        return Err(ResolveError::path(
            PathErrorCode::NotFound,
            "Mount point not found or not accessible in this context",
        ));
    };

    if !mp.enabled {
        tracing::warn!(
            mount_point_id = %mp.id,
            "Attempt to access disabled mount point: {}",
            mp.id
        );
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

    // ---- v4 bug 152's refusal split: `describe_characters` + the two warns ----

    #[test]
    fn describe_characters_preserves_insertion_order_and_dedups() {
        // v4 builds a `Set` seeded with `characterId`, then every `characterIds`
        // entry: insertion order, duplicates dropped, `none` when empty.
        let ctx = |cid: Option<&str>, ids: &[&str]| PathResolutionContext {
            character_id: cid.map(String::from),
            character_ids: ids.iter().map(|s| s.to_string()).collect(),
            ..Default::default()
        };
        assert_eq!(describe_characters(&ctx(None, &[])), "none");
        assert_eq!(describe_characters(&ctx(Some("a"), &[])), "a");
        assert_eq!(describe_characters(&ctx(Some("a"), &["b", "c"])), "a,b,c");
        // the acting character repeated among the peers collapses
        assert_eq!(describe_characters(&ctx(Some("a"), &["b", "a"])), "a,b");
        // duplicates WITHIN the peers collapse too
        assert_eq!(describe_characters(&ctx(Some("a"), &["b", "b"])), "a,b");
        // no acting character: the peers alone, in order
        assert_eq!(describe_characters(&ctx(None, &["c", "b"])), "c,b");
        // an EMPTY acting id is falsy in v4 (`if (context.characterId)`) — skipped
        assert_eq!(describe_characters(&ctx(Some(""), &["b"])), "b");
        assert_eq!(describe_characters(&ctx(Some(""), &[])), "none");
    }

    /// The fixture both warn pins share: one enabled `documents` store (the
    /// out-of-scope subject) and one enabled `character` vault (which must keep
    /// the indistinguishable NOT_FOUND), with NOTHING accessible to the context.
    fn warn_fixture() -> (rusqlite::Connection, rusqlite::Connection) {
        let main = rusqlite::Connection::open_in_memory().unwrap();
        let mount = rusqlite::Connection::open_in_memory().unwrap();
        mount
            .execute_batch(
                r#"CREATE TABLE "doc_mount_points" (
                     "id" TEXT PRIMARY KEY, "name" TEXT NOT NULL, "basePath" TEXT NOT NULL,
                     "mountType" TEXT NOT NULL, "storeType" TEXT, "enabled" INTEGER NOT NULL
                   );
                   CREATE TABLE "project_doc_mount_links" (
                     "id" TEXT PRIMARY KEY, "projectId" TEXT NOT NULL,
                     "mountPointId" TEXT NOT NULL, "createdAt" TEXT, "updatedAt" TEXT
                   );
                   INSERT INTO "doc_mount_points" VALUES
                     ('s-1','Someone Elses Papers','','database','documents',1),
                     ('v-1','Leilani Character Vault','','database','character',1),
                     ('r-1','Project Papers','','database','documents',1);
                   INSERT INTO "project_doc_mount_links" VALUES ('l-1','p-1','r-1','','');"#,
            )
            .unwrap();
        (main, mount)
    }

    #[test]
    fn out_of_scope_store_warns_and_denies_with_v4s_sentence() {
        let (main, mount) = warn_fixture();
        // No override: the pool reaches only the project-linked `r-1`, so the
        // stranger store is enabled-but-out-of-scope — the split's subject.
        let ctx = PathResolutionContext {
            project_id: Some("p-1".to_string()),
            character_id: Some("c-1".to_string()),
            mount_point: Some("Someone Elses Papers".to_string()),
            ..Default::default()
        };
        let (out, lines) = crate::test_support::captured_with(|| {
            resolve_doc_edit_path(
                &main,
                &mount,
                DocEditScope::DocumentStore,
                Some("notes.md"),
                &ctx,
                None,
            )
        });
        match out {
            Err(ResolveError::Path { code, message }) => {
                assert_eq!(code, PathErrorCode::AccessDenied);
                assert_eq!(
                    message,
                    "The document store \"Someone Elses Papers\" exists but is not reachable \
                     from this conversation. It is not linked to this project, and it is not one \
                     of your own group's stores. Retrying with a different spelling will not \
                     help — use doc_list_files with no path to see the stores you can reach."
                );
            }
            other => panic!("expected ACCESS_DENIED, got {other:?}"),
        }
        let warn = lines
            .iter()
            .find(|l| l.contains("Mount point exists but is out of scope"))
            .unwrap_or_else(|| panic!("no out-of-scope warn in {lines:?}"));
        assert!(warn.starts_with("WARN "), "level must be WARN: {warn}");
        assert!(
            warn.contains(
                "Mount point exists but is out of scope: Someone Elses Papers (s-1) \
                 (project: p-1, characters: c-1, vaultsHidden: false)"
            ),
            "v4's sentence must render byte-for-byte: {warn}"
        );
        assert!(
            warn.contains("vaults_hidden=false"),
            "fields carried: {warn}"
        );
    }

    #[test]
    fn a_character_vault_keeps_the_indistinguishable_not_found() {
        // The covenant must not leak through the refusal: naming the vault would
        // disclose exactly what the access rule withholds.
        let (main, mount) = warn_fixture();
        let ctx = PathResolutionContext {
            project_id: Some("p-1".to_string()),
            character_id: Some("c-1".to_string()),
            hide_character_vaults: true,
            mount_point: Some("Leilani Character Vault".to_string()),
            ..Default::default()
        };
        let (out, lines) = crate::test_support::captured_with(|| {
            resolve_doc_edit_path(
                &main,
                &mount,
                DocEditScope::DocumentStore,
                Some("notes.md"),
                &ctx,
                None,
            )
        });
        match out {
            Err(ResolveError::Path { code, message }) => {
                assert_eq!(code, PathErrorCode::NotFound);
                assert_eq!(
                    message,
                    "Mount point not found or not accessible in this context"
                );
            }
            other => panic!("expected NOT_FOUND, got {other:?}"),
        }
        let warn = lines
            .iter()
            .find(|l| l.contains("Mount point not found or not accessible:"))
            .unwrap_or_else(|| panic!("no not-found warn in {lines:?}"));
        assert!(warn.starts_with("WARN "), "v4 logs it at warn: {warn}");
        assert!(
            warn.contains(
                "Mount point not found or not accessible: Leilani Character Vault \
                 (project: p-1, characters: c-1, vaultsHidden: true)"
            ),
            "v4's sentence with the appended vaultsHidden: {warn}"
        );
        assert!(
            !lines
                .iter()
                .any(|l| l.contains("Mount point exists but is out of scope")),
            "a vault must NOT be disclosed as existing: {lines:?}"
        );
    }

    #[test]
    fn a_resolved_store_logs_neither_warn() {
        // The silence leg: both lines belong to the refusal, not the happy path.
        let (main, mount) = warn_fixture();
        let ctx = PathResolutionContext {
            mount_point: Some("Someone Elses Papers".to_string()),
            operator_override: true,
            ..Default::default()
        };
        let (out, lines) = crate::test_support::captured_with(|| {
            resolve_doc_edit_path(
                &main,
                &mount,
                DocEditScope::DocumentStore,
                Some("notes.md"),
                &ctx,
                None,
            )
        });
        assert!(out.is_ok(), "the override must resolve it: {out:?}");
        assert!(
            !lines
                .iter()
                .any(|l| l.contains("Mount point exists but is out of scope")
                    || l.contains("Mount point not found or not accessible:")),
            "no refusal warn on a resolved store: {lines:?}"
        );
    }

    // ---- the pre-existing absent v4 log lines (P4.D200 Tier 2 item 12) ----
    // These sit BESIDE bug 152's hunks rather than inside them: v4 has had them all
    // along and the port dropped them, the #103/#110 class. Each is pinned with its
    // rendered sentence, its level, and a leg proving it does NOT fire otherwise.

    #[test]
    fn missing_mount_point_warns() {
        let (main, mount) = warn_fixture();
        let ctx = PathResolutionContext {
            project_id: Some("p-1".to_string()),
            ..Default::default()
        };
        let (_out, lines) = crate::test_support::captured_with(|| {
            resolve_doc_edit_path(
                &main,
                &mount,
                DocEditScope::DocumentStore,
                Some("notes.md"),
                &ctx,
                None,
            )
        });
        assert!(
            lines.iter().any(|l| l.starts_with("WARN ")
                && l.contains("document_store scope requires mountPoint in context")),
            "{lines:?}"
        );
    }

    #[test]
    fn missing_project_and_character_warns() {
        let (main, mount) = warn_fixture();
        let ctx = PathResolutionContext {
            mount_point: Some("Project Papers".to_string()),
            ..Default::default()
        };
        let (_out, lines) = crate::test_support::captured_with(|| {
            resolve_doc_edit_path(
                &main,
                &mount,
                DocEditScope::DocumentStore,
                Some("notes.md"),
                &ctx,
                None,
            )
        });
        assert!(
            lines.iter().any(|l| l.starts_with("WARN ")
                && l.contains("document_store scope requires projectId or characterId in context")),
            "{lines:?}"
        );
        // …and the OTHER context warn does not fire: a mountPoint WAS supplied.
        assert!(
            !lines
                .iter()
                .any(|l| l.contains("requires mountPoint in context")),
            "{lines:?}"
        );
    }

    #[test]
    fn self_token_failure_warns_with_the_character_id() {
        // `c-1` has no vault row at all, so the self-token arm cannot resolve.
        let (main, mount) = warn_fixture();
        let ctx = PathResolutionContext {
            project_id: Some("p-1".to_string()),
            character_id: Some("c-1".to_string()),
            mount_point: Some("self".to_string()),
            ..Default::default()
        };
        let (out, lines) = crate::test_support::captured_with(|| {
            resolve_doc_edit_path(
                &main,
                &mount,
                DocEditScope::DocumentStore,
                Some("notes.md"),
                &ctx,
                None,
            )
        });
        assert!(matches!(
            out,
            Err(ResolveError::Path {
                code: PathErrorCode::NotFound,
                ..
            })
        ));
        let warn = lines
            .iter()
            .find(|l| l.contains("Self-token resolution failed: no accessible vault for character"))
            .unwrap_or_else(|| panic!("{lines:?}"));
        assert!(warn.starts_with("WARN "), "{warn}");
        assert!(warn.contains("character_id=c-1"), "{warn}");
    }

    #[test]
    fn a_disabled_mount_warns_with_its_id() {
        let main = rusqlite::Connection::open_in_memory().unwrap();
        let mount = rusqlite::Connection::open_in_memory().unwrap();
        mount
            .execute_batch(
                r#"CREATE TABLE "doc_mount_points" (
                     "id" TEXT PRIMARY KEY, "name" TEXT NOT NULL, "basePath" TEXT NOT NULL,
                     "mountType" TEXT NOT NULL, "storeType" TEXT, "enabled" INTEGER NOT NULL
                   );
                   CREATE TABLE "project_doc_mount_links" (
                     "id" TEXT PRIMARY KEY, "projectId" TEXT NOT NULL,
                     "mountPointId" TEXT NOT NULL, "createdAt" TEXT, "updatedAt" TEXT
                   );
                   INSERT INTO "doc_mount_points" VALUES
                     ('d-1','Disabled Store','','database','documents',0);
                   INSERT INTO "project_doc_mount_links" VALUES ('l-1','p-1','d-1','','');"#,
            )
            .unwrap();
        // NOT the operator override: its accessible set is `findEnabled`, which
        // cannot contain a disabled store, so that path can never reach this arm
        // (v4's is the same shape). The tiered pool does NOT filter on `enabled`,
        // so a project-linked disabled store is how the refusal is reachable.
        let ctx = PathResolutionContext {
            project_id: Some("p-1".to_string()),
            mount_point: Some("Disabled Store".to_string()),
            ..Default::default()
        };
        let (out, lines) = crate::test_support::captured_with(|| {
            resolve_doc_edit_path(
                &main,
                &mount,
                DocEditScope::DocumentStore,
                Some("notes.md"),
                &ctx,
                None,
            )
        });
        match out {
            Err(ResolveError::Path { code, message }) => {
                assert_eq!(code, PathErrorCode::AccessDenied);
                assert_eq!(message, "Mount point is disabled");
            }
            other => panic!("expected the disabled refusal, got {other:?}"),
        }
        assert!(
            lines.iter().any(|l| l.starts_with("WARN ")
                && l.contains("Attempt to access disabled mount point: d-1")),
            "{lines:?}"
        );
    }

    #[test]
    fn an_enabled_store_resolves_on_the_pool_path_with_no_warn_at_all() {
        // (The self-token warn's silence leg lives in the harness —
        // `doc_edit_path_resolver_equivalence`'s `self-token` rows — because a
        // RESOLVING self token needs a real vault behind the character-tier read,
        // which applies the document-store overlay an in-memory fixture cannot
        // satisfy.)
        // The silence leg for the disabled-mount warn (and, on the NON-operator
        // path, for both refusal warns): `Project Papers` is enabled and linked
        // to `p-1`, so the pool reaches it and nothing at WARN may fire.
        let (main, mount) = warn_fixture();
        let ctx = PathResolutionContext {
            project_id: Some("p-1".to_string()),
            mount_point: Some("Project Papers".to_string()),
            ..Default::default()
        };
        let (out, lines) = crate::test_support::captured_with(|| {
            resolve_doc_edit_path(
                &main,
                &mount,
                DocEditScope::DocumentStore,
                Some("notes.md"),
                &ctx,
                None,
            )
        });
        assert_eq!(out.unwrap().mount_point_id.as_deref(), Some("r-1"));
        assert!(
            !lines.iter().any(|l| l.starts_with("WARN ")),
            "a resolving store must log nothing at WARN: {lines:?}"
        );
    }
}
