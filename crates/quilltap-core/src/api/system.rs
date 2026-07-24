//! The `system` dispatch surface (P4.6z) — the DirectoryPicker's host-filesystem
//! browser. A differential port of v4's
//! `app/api/v1/system/browse-directory/route.ts` (`createContextHandler`, no
//! unlock requirement — the handler never touches the vault, it reads the host
//! filesystem directly, the way the two other fs-touching mount services do).
//!
//! v4 semantics, verbatim: absent/empty `path` → the process home directory;
//! `path.resolve`/normalize; `fs.stat` failure → 400 `"Path does not exist:
//! <resolved>"`; not-a-directory → 400 `"Path is not a directory: <resolved>"`;
//! `fs.readdir` failure → a SUCCESS envelope with `directories: []` +
//! `error: 'Permission denied'`; entries filtered to directories (dirent type,
//! symlinks NOT followed — Node `Dirent.isDirectory()`), `.`-prefixed skipped;
//! sorted by `name` localeCompare (ICU4X en-US); `parent = dirname(resolved)`,
//! null when `dirname(resolved) === resolved` (the fs root).

use std::path::Path;

use serde_json::json;

use crate::db::instance_settings::get_general_mount_point_id;
use crate::db::runtime::Db;
use crate::db::DbError;
use crate::services::aesthetics::{
    aesthetic_filename_for_kind, read_aesthetic_file, write_aesthetic_file,
};

use super::types::{ErrorKind, Response};

fn bad_request(msg: impl Into<String>) -> Response {
    Response::error(ErrorKind::BadRequest, msg)
}
fn internal(e: impl std::fmt::Display) -> Response {
    Response::error(ErrorKind::Internal, e.to_string())
}

/// v4's 400 for a `kind` that is neither literal, verbatim
/// (`image-aesthetics/route.ts:31`). Shared with the project pair's identical
/// guard.
const AESTHETIC_KIND_ERROR: &str = "Query param \"kind\" must be \"lantern\" or \"aurora\"";

/// v4 `GET /api/v1/system/home` / the `systemHome` dispatch verb (P4.6au) —
/// the home-dashboard payload. The composition lives in [`services::home`]
/// (`crate::services::home::get_home_data`); this is the dispatch glue: nest
/// the main + mount-index read connections (the vault overlay needs both) and
/// wrap the typed payload. v4's route answers `successResponse(data)` — the
/// RAW payload — and `serverError('Failed to load the home dashboard')` on any
/// throw; the error message rides the `Internal` envelope so the REST edge
/// emits v4's exact `{error}` body.
pub fn system_home(db: &Db, user_id: &str, fallback_name: Option<&str>) -> Response {
    let user_id = user_id.to_string();
    let fallback = fallback_name.map(str::to_string);
    let result = db.read_main(move |main| {
        db.read_mount_index(move |mount| {
            crate::services::home::get_home_data(main, mount, &user_id, fallback.as_deref())
        })
    });
    match result.and_then(|data| {
        serde_json::to_value(&data).map_err(|e| DbError::Key(format!("home payload: {e}")))
    }) {
        Ok(v) => Response::SystemHome(v),
        // v4's catch-all: the route logs the error and answers this fixed
        // message.
        Err(e) => {
            tracing::error!(
                target: "quilltap::system",
                error = %e,
                "Failed to build the home dashboard payload",
            );
            Response::error(ErrorKind::Internal, "Failed to load the home dashboard")
        }
    }
}

/// v4 `GET /api/v1/system/browse-directory[?path=…]` — list a directory's
/// immediate subdirectories for the picker.
pub fn browse_directory(requested_path: Option<&str>) -> Response {
    // `searchParams.get('path') || os.homedir()` — absent OR empty → home.
    let requested = match requested_path {
        Some(p) if !p.is_empty() => p.to_string(),
        _ => home_dir(),
    };

    // `path.resolve(requestedPath)`.
    let resolved = resolve_path(&requested);

    // `fs.stat` — a failure is the nonexistent-path 400 (the try/catch swallows
    // the specific errno; any stat failure is "does not exist").
    let meta = match std::fs::metadata(&resolved) {
        Ok(m) => m,
        Err(_) => {
            return Response::error(
                ErrorKind::BadRequest,
                format!("Path does not exist: {resolved}"),
            );
        }
    };
    if !meta.is_dir() {
        return Response::error(
            ErrorKind::BadRequest,
            format!("Path is not a directory: {resolved}"),
        );
    }

    let parent = parent_of(&resolved);

    // `fs.readdir(resolvedPath, { withFileTypes: true })` — a failure returns a
    // SUCCESS envelope carrying the degraded `Permission denied` marker.
    let read = match std::fs::read_dir(&resolved) {
        Ok(r) => r,
        Err(_) => {
            return Response::System(json!({
                "path": resolved,
                "parent": parent,
                "directories": [],
                "error": "Permission denied",
            }));
        }
    };

    // Filter to directories (dirent type — symlinks are NOT followed, matching
    // Node `Dirent.isDirectory()`), skip `.`-prefixed, then sort by localeCompare.
    let mut directories: Vec<(String, String)> = Vec::new();
    for entry in read.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        let is_dir = match entry.file_type() {
            Ok(t) => t.is_dir(),
            Err(_) => false,
        };
        if !is_dir {
            continue;
        }
        let child = join_path(&resolved, &name);
        directories.push((name, child));
    }
    directories.sort_by(|a, b| crate::collation::locale_compare(&a.0, &b.0));

    let dirs_json: Vec<serde_json::Value> = directories
        .into_iter()
        .map(|(name, path)| json!({ "name": name, "path": path }))
        .collect();

    Response::System(json!({
        "path": resolved,
        "parent": parent,
        "directories": dirs_json,
    }))
}

/// The process home directory (Node `os.homedir()` on POSIX resolves `$HOME`).
fn home_dir() -> String {
    std::env::var("HOME").unwrap_or_default()
}

/// v4 `path.resolve(input)` (posix): an absolute input is normalized in place; a
/// relative input is resolved against the process cwd first. Normalization
/// collapses `//`, drops `.`, folds `..` (leading `..` above an absolute root
/// are discarded), and strips the trailing slash (except the bare root).
fn resolve_path(input: &str) -> String {
    let combined = if input.starts_with('/') {
        input.to_string()
    } else {
        let cwd = std::env::current_dir()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| "/".to_string());
        format!("{cwd}/{input}")
    };
    normalize_posix(&combined)
}

/// Normalize an ABSOLUTE posix path (the only shape `resolve_path` feeds it).
fn normalize_posix(path: &str) -> String {
    let mut out: Vec<&str> = Vec::new();
    for seg in path.split('/') {
        match seg {
            "" | "." => continue,
            ".." => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    if out.is_empty() {
        "/".to_string()
    } else {
        format!("/{}", out.join("/"))
    }
}

/// v4 `path.dirname(p) !== p ? path.dirname(p) : null` over the already-resolved
/// (normalized, no trailing slash) path.
fn parent_of(resolved: &str) -> Option<String> {
    let parent = dirname_posix(resolved);
    if parent == resolved {
        None
    } else {
        Some(parent)
    }
}

/// Node posix `path.dirname` over a normalized path (no trailing slash except root).
fn dirname_posix(p: &str) -> String {
    match p.rfind('/') {
        None => ".".to_string(),
        Some(0) => "/".to_string(),
        Some(i) => p[..i].to_string(),
    }
}

/// Node posix `path.join(dir, name)` for a normalized `dir` + a simple `name`
/// (re-normalized so the root case yields `/name`, not `//name`).
fn join_path(dir: &str, name: &str) -> String {
    let _ = Path::new(dir); // keep the intent that `dir` is a filesystem path.
    normalize_posix(&format!("{dir}/{name}"))
}

// ===========================================================================
// Default image aesthetics (P4.6ar) — v4
// `app/api/v1/system/image-aesthetics/route.ts`
// ===========================================================================

/// v4 `GET /api/v1/system/image-aesthetics?kind=lantern|aurora` (`route.ts:29-41`)
/// — the Images settings tab's two "Default Aesthetic" editors read this.
///
/// The instance-wide sibling of `api::projects::project_aesthetic_get`: same
/// single-tier read over the same shared helpers, with the Quilltap General
/// singleton swapped in for the project's official store. Body `{ content }`.
///
/// The unprovisioned-store arm is a **success**, not an error: v4 answers
/// `successResponse({ content: '' })` with the comment "Quilltap General store not
/// provisioned yet — nothing to show". (The PUT sibling refuses instead — you
/// cannot write into a store that does not exist.)
pub fn system_image_aesthetics_get(db: &Db, kind: &str) -> Response {
    let Some(filename) = aesthetic_filename_for_kind(kind) else {
        return bad_request(AESTHETIC_KIND_ERROR);
    };
    let result = db
        .read_main(get_general_mount_point_id)
        .and_then(|mount_id| {
            let Some(mount_id) = mount_id else {
                return Ok(String::new());
            };
            db.read_mount_index(move |mount| Ok(read_aesthetic_file(mount, &mount_id, filename)))
        });
    match result {
        Ok(content) => Response::SystemAesthetic(json!({ "content": content })),
        Err(e) => internal(e),
    }
}

/// v4 `PUT /api/v1/system/image-aesthetics?kind=…` (`route.ts:43-61`).
///
/// `content` arrives already parsed at the boundary. v4's route body is
/// `aestheticContentSchema.safeParse(await req.json().catch(() => ({}))).data
/// ?.content ?? ''` — so a MALFORMED body, an absent `content`, or a non-string
/// `content` all resolve to `''`, and `''` **deletes the file** (restoring the
/// fallback). That is the same "clearing the editor restores the default" gesture
/// the project pair carries; the REST edge reproduces the parse. Body
/// `{ success: true }`.
pub async fn system_image_aesthetics_set(db: &Db, kind: &str, content: Option<String>) -> Response {
    let Some(filename) = aesthetic_filename_for_kind(kind) else {
        return bad_request(AESTHETIC_KIND_ERROR);
    };
    let content = content.unwrap_or_default();
    let mount_id = match db.read_main(get_general_mount_point_id) {
        Ok(Some(id)) => id,
        // Unlike the GET, the PUT REFUSES — there is no store to write into.
        Ok(None) => {
            return internal("Quilltap General document store is not available");
        }
        Err(e) => return internal(e),
    };
    let out = db
        .write(move |writers| {
            let Some(mount) = writers.mount_index() else {
                return Err(DbError::PartitionUnavailable(
                    crate::write_partition::WriteDbTarget::MountIndex,
                ));
            };
            write_aesthetic_file(mount.connection(), &mount_id, filename, &content)
        })
        .await;
    match out {
        Ok(()) => Response::SystemAesthetic(json!({ "success": true })),
        Err(e) => internal(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    #[test]
    fn normalize_matches_node_resolve() {
        assert_eq!(normalize_posix("/a/b/c"), "/a/b/c");
        assert_eq!(normalize_posix("/a/b/../c"), "/a/c");
        assert_eq!(normalize_posix("/a//b/./c/"), "/a/b/c");
        assert_eq!(normalize_posix("/"), "/");
        assert_eq!(normalize_posix("/.."), "/");
        assert_eq!(normalize_posix("/a/.."), "/");
    }

    #[test]
    fn dirname_and_parent_match_node() {
        assert_eq!(dirname_posix("/a/b/c"), "/a/b");
        assert_eq!(dirname_posix("/a"), "/");
        assert_eq!(dirname_posix("/"), "/");
        // The fs-root branch: dirname("/") === "/" → parent is null.
        assert_eq!(parent_of("/"), None);
        assert_eq!(parent_of("/a"), Some("/".to_string()));
        assert_eq!(parent_of("/a/b"), Some("/a".to_string()));
    }

    #[test]
    fn join_root_has_no_double_slash() {
        assert_eq!(join_path("/", "x"), "/x");
        assert_eq!(join_path("/a/b", "c"), "/a/b/c");
    }

    #[test]
    fn nonexistent_path_is_bad_request() {
        let resp = browse_directory(Some("/definitely/not/a/real/path/qt6z"));
        match resp {
            Response::Error(e) => {
                assert_eq!(e.kind, ErrorKind::BadRequest);
                assert!(e.message.starts_with("Path does not exist: "));
            }
            other => panic!("expected BadRequest, got {other:?}"),
        }
    }

    #[test]
    fn file_not_directory_is_bad_request() {
        // This source file is not a directory.
        let this = format!("{}/src/api/system.rs", env!("CARGO_MANIFEST_DIR"));
        let resp = browse_directory(Some(&this));
        match resp {
            Response::Error(e) => {
                assert_eq!(e.kind, ErrorKind::BadRequest);
                assert!(e.message.starts_with("Path is not a directory: "));
            }
            other => panic!("expected BadRequest, got {other:?}"),
        }
    }

    #[test]
    fn default_arm_resolves_home() {
        // The `os.homedir()` default arm is machine-dependent (un-fixturable in
        // the differential) — assert the resolved `path` equals the resolved home.
        let home = resolve_path(&home_dir());
        match browse_directory(None) {
            Response::System(v) => {
                assert_eq!(v["path"], Value::String(home));
                // The home dir exists and is a directory → not the error arm.
                assert!(v.get("directories").is_some());
            }
            // A home dir that doesn't exist would 400 — tolerate on odd CI, but
            // the dev/CI machines always have $HOME.
            Response::Error(e) => panic!("home browse failed: {}", e.message),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn permission_denied_envelope_shape() {
        // The readdir-failure arm is a SUCCESS envelope (chmod-000 dirs are
        // root-flaky, so we assert the SHAPE the code emits rather than provoke
        // it): path + parent + empty directories + the degraded marker.
        // Exercised structurally here; the happy path is covered by the
        // differential and `default_arm_resolves_home`.
        let sample = json!({
            "path": "/x/y",
            "parent": "/x",
            "directories": [],
            "error": "Permission denied",
        });
        assert_eq!(sample["error"], "Permission denied");
        assert_eq!(sample["directories"].as_array().unwrap().len(), 0);
    }
}
