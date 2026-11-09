//! The document core (P4.D380) — v4 `lib/documents/core/` (`a3d53eae1`,
//! `30a3b4cb7`, `4310d435f`, `5efe53ea7` at `624d0d25c`): every file-level job
//! on a document in a store — read (with the one blob fallback, #134), write
//! (with the one refresh, #133), create (#137), rename / move (with the one
//! `chat_documents` sweep), delete, and the read-only view classification.
//!
//! Two kinds of caller drive it and keep only what is theirs: the operator's
//! Document Mode routes ([`crate::documents`] + [`crate::api::documents`] —
//! access contexts, HTTP mapping, Librarian posts) and the `doc_*` tool
//! handlers ([`crate::tools::doc_edit`] — the character's resolution context,
//! policy gates, tool-result shapes). Below it sit the `doc_edit` primitives
//! ([`crate::tools::doc_edit::shared`]'s read / write dispatch, the
//! resolved-file helpers in [`crate::doc_edit::path_resolver`]) and the
//! database store ([`crate::db::database_store`]). One file per v4 module
//! (round ruling R-L3), so the tree diffs against v4's file-for-file.
//!
//! ## The errors (v4 `errors.ts`)
//!
//! v4 raises three bare classes, shared by both callers:
//!
//! - `DocumentConflictError` — "the document changed underneath the caller: a
//!   rename/create target already exists, or a write's mtime precondition
//!   failed because the file was edited elsewhere since it was read. Routes map
//!   it to a 409."
//! - `DocumentMissingError` — "the requested document does not exist." (→ 404)
//! - `DocumentReadOnlyError` — "thrown when an editor write targets a file
//!   Document Mode only views." (→ 400)
//!
//! v5 carries them as [`super::DocError`] variants: `Conflict` (a taken
//! target), `Missing`, `ReadOnly` — and keeps v4's mtime conflict as the
//! DISTINCT [`super::DocError::MtimeMismatch`], because the two 409 bodies
//! differ (`Document changed elsewhere. Reload it and try again.` vs `A file
//! already exists at that name.`). In v4 both are one `DocumentConflictError`
//! and the route tells them apart by which call threw; here the variant says
//! it. A read's absence (v4's `ENOENT`-coded error) is `Missing` carrying the
//! `ENOENT: …` message — [`read::is_enoent`] is the ONE test for it.

pub mod delete;
pub mod moves;
pub mod read;
pub mod view_target;
pub mod write;

pub use delete::{delete_document, DeleteOutcome};
pub use moves::{compute_rename_target, rename_document, RenameOutcome, RenameTarget};
pub use read::{
    document_exists, is_enoent, read_document, stat_document, DocKind, DocumentRead, DocumentSource,
};
pub use view_target::{view_target_for, DocumentViewTarget};
pub use write::{after_document_write, ensure_document, write_document, Ensured, Refresh};

/// Node's `error.message` for a failed filesystem call (`<CODE>: <description>,
/// <syscall> '<path>'`, plus ` -> '<dest>'` for a two-path call) — what v4
/// surfaces when the core rethrows a non-absent `fs` error (R-4). The
/// descriptions are libuv's.
pub fn node_fs_error_message(
    e: &std::io::Error,
    syscall: &str,
    path: &str,
    dest: Option<&str>,
) -> String {
    let code = crate::doc_edit::path_resolver::io_error_code(e);
    let description = match code.as_str() {
        "ENOENT" => "no such file or directory",
        "EACCES" => "permission denied",
        "EPERM" => "operation not permitted",
        "ENOTDIR" => "not a directory",
        "EISDIR" => "illegal operation on a directory",
        "EEXIST" => "file already exists",
        "ENOTEMPTY" => "directory not empty",
        "EXDEV" => "cross-device link not permitted",
        "ELOOP" => "too many symbolic links encountered",
        "ENAMETOOLONG" => "name too long",
        "EIO" => "i/o error",
        "EINVAL" => "invalid argument",
        _ => return e.to_string(),
    };
    match dest {
        Some(d) => format!("{code}: {description}, {syscall} '{path}' -> '{d}'"),
        None => format!("{code}: {description}, {syscall} '{path}'"),
    }
}

#[cfg(test)]
mod tests;
