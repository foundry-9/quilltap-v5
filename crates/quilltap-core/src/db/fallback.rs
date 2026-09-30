//! v4's FALLBACK repository reads, as their callers see them — the ONE home
//! for the shape (unified at the `97b25fc53` follow-ups round; before it the
//! first two lines lived in four hand-copies across `chats_read`,
//! `characters_read`, `image_profiles` and `dangerous_content::understudy`;
//! P4.131 grew it by the document-store shapes, and the `97b25fc53` smalls
//! unification folded seven more module-target twins of `Error finding entity
//! by ID` onto it — `fallback_home_guard` in the harness now holds the line).
//!
//! v4 `BaseRepository._findById` is `safeQuery(…, 'Error finding entity by ID',
//! { id }, null)` and `_findAll` is `safeQuery(…, 'Error finding all
//! entities', {}, [])` (`base.repository.ts:247-277`): a FAILED read logs
//! that ERROR with the repository's `collection` injected first
//! (`safeQuery`'s `enrichedContext`, `:101`) and the error's message, and
//! answers the fallback — it never throws. `getCollection()` runs INSIDE the
//! same `safeQuery`, so a caller that hands `read` the whole pool checkout
//! (`db.read_main(…)`) gets v4's line for a pool failure too, once.
//!
//! `target: "quilltap::db"` is v4's `Repository` logger module as the
//! differentials map it (`danger_routing_equivalence`'s `Repository →
//! quilltap::db`); every emitter of these two lines uses it.

use super::DbError;

/// The `error` field's bytes: v4 logs `extractErrorMessage(error)` — the
/// thrown error's own message, which for a SQLite failure is the driver's
/// bare sentence (`no such table: chats`). `DbError::Sqlite`'s `Display`
/// prefixes it with `sqlite error: ` (a v5 rendering convention, useful in a
/// propagated error, wrong on a line that stands in for v4's); every other
/// variant already IS a bare message. Every shape in this home renders the
/// field through here (the `97b25fc53` smalls unification — the P4.131 lines
/// had all carried the prefix, and one caller had worked around it locally).
fn error_text(error: &DbError) -> String {
    match error {
        DbError::Sqlite(e) => e.to_string(),
        other => other.to_string(),
    }
}

/// v4 `_findById` as its callers see it: `read()`'s `Err` logs `Error finding
/// entity by ID {collection, id, error}` and answers `None`.
pub fn find_by_id_or_none<T>(
    collection: &'static str,
    id: &str,
    read: impl FnOnce() -> Result<Option<T>, DbError>,
) -> Option<T> {
    read().unwrap_or_else(|error| {
        tracing::error!(
            target: "quilltap::db",
            collection = collection,
            id = %id,
            error = %error_text(&error),
            "Error finding entity by ID"
        );
        None
    })
}

/// v4 `_findAll` as its callers see it: `read()`'s `Err` logs `Error finding
/// all entities {collection, error}` and answers `[]`.
pub fn find_all_or_empty<T>(
    collection: &'static str,
    read: impl FnOnce() -> Result<Vec<T>, DbError>,
) -> Vec<T> {
    read().unwrap_or_else(|error| {
        tracing::error!(
            target: "quilltap::db",
            collection = collection,
            error = %error_text(&error),
            "Error finding all entities"
        );
        Vec::new()
    })
}

/// v4 `BaseRepository.findByFilter` as its callers see it: `read()`'s `Err` logs
/// `Error finding entities by filter {collection, error}` and answers `[]`
/// (`base.repository.ts:283-297`). The context is `{}` — v4 logs NO `filter`
/// field — so the line carries `collection` and `error` only. Every v4
/// repository method that wraps `findByFilter` in its OWN `safeQuery` (the
/// folders' `findByMountPointId`, the group links' `findByGroupId`) is
/// answered by THIS line first; the outer one is unreachable.
pub fn find_by_filter_or_empty<T>(
    collection: &'static str,
    read: impl FnOnce() -> Result<Vec<T>, DbError>,
) -> Vec<T> {
    read().unwrap_or_else(|error| {
        tracing::error!(
            target: "quilltap::db",
            collection = collection,
            error = %error_text(&error),
            "Error finding entities by filter"
        );
        Vec::new()
    })
}

/// v4 `BaseRepository.findOneByFilter` as its callers see it: `read()`'s `Err`
/// logs `Error finding entity by filter {collection, error}` and answers `None`
/// (`base.repository.ts:300-314`).
pub fn find_one_by_filter_or_none<T>(
    collection: &'static str,
    read: impl FnOnce() -> Result<Option<T>, DbError>,
) -> Option<T> {
    read().unwrap_or_else(|error| {
        tracing::error!(
            target: "quilltap::db",
            collection = collection,
            error = %error_text(&error),
            "Error finding entity by filter"
        );
        None
    })
}

/// v4 `docMountFileLinks.queryJoined` as its callers see it: a fallback
/// `withRawDb`, so `read()`'s `Err` logs `Error querying joined file links
/// {collection, whereClause, error}` and answers `[]`
/// (`doc-mount-file-links.repository.ts:1445-1505`). `where_clause` is the
/// plain string v4 passes (`WHERE l.mountPointId = ?`, …). Both public joined
/// reads (`findByMountPointId`, `findByMountPointAndPath`) wrap this in their
/// own `safeQuery`, which therefore NEVER fires — this is the only line.
pub fn joined_file_links_or_empty<T>(
    where_clause: &'static str,
    read: impl FnOnce() -> Result<Vec<T>, DbError>,
) -> Vec<T> {
    read().unwrap_or_else(|error| {
        tracing::error!(
            target: "quilltap::db",
            collection = "doc_mount_file_links",
            whereClause = where_clause,
            error = %error_text(&error),
            "Error querying joined file links"
        );
        Vec::new()
    })
}

/// v4 `docMountDocuments.findByMountPointAndPath` as its callers see it: a
/// fallback `withRawDb(null)`, so `read()`'s `Err` logs `Error finding document
/// by mount point and path {collection, mountPointId, relativePath, error}` and
/// answers `None` (`doc-mount-documents.repository.ts:110-135`).
pub fn document_by_mount_point_and_path_or_none<T>(
    mount_point_id: &str,
    relative_path: &str,
    read: impl FnOnce() -> Result<Option<T>, DbError>,
) -> Option<T> {
    read().unwrap_or_else(|error| {
        tracing::error!(
            target: "quilltap::db",
            collection = "doc_mount_documents",
            mountPointId = %mount_point_id,
            relativePath = %relative_path,
            error = %error_text(&error),
            "Error finding document by mount point and path"
        );
        None
    })
}

/// v4 `docMountFileLinks.deleteWithGC` as its callers see it: a fallback
/// `withRawDb({ fileId: null, fileGC: false })`, so `read()`'s `Err` logs `Error
/// deleting file link with GC {collection, linkId, error}` and answers `false`
/// (`doc-mount-file-links.repository.ts:753-800`). The CALLER decides what a
/// `false` means: v4's `deleteDatabaseDocument` goes on to emit and answer
/// `true` regardless.
pub fn delete_with_gc_or_false(
    link_id: &str,
    read: impl FnOnce() -> Result<bool, DbError>,
) -> bool {
    read().unwrap_or_else(|error| {
        tracing::error!(
            target: "quilltap::db",
            collection = "doc_mount_file_links",
            linkId = %link_id,
            error = %error_text(&error),
            "Error deleting file link with GC"
        );
        false
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_failed_read_logs_v4s_line_and_answers_the_fallback() {
        let (got, lines) = crate::test_support::captured_with(|| {
            find_by_id_or_none::<i32>("widgets", "w-1", || Err(DbError::Internal("posed".into())))
        });
        assert_eq!(got, None);
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(
            lines[0].starts_with(
                "ERROR quilltap::db Error finding entity by ID collection=widgets id=w-1 error="
            ),
            "{}",
            lines[0]
        );
        let (got, lines) = crate::test_support::captured_with(|| {
            find_all_or_empty::<i32>("widgets", || Err(DbError::Internal("posed".into())))
        });
        assert!(got.is_empty());
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(
            lines[0].starts_with(
                "ERROR quilltap::db Error finding all entities collection=widgets error="
            ),
            "{}",
            lines[0]
        );
    }

    #[test]
    fn a_successful_read_logs_nothing() {
        let (got, lines) = crate::test_support::captured_with(|| {
            find_by_id_or_none("widgets", "w-1", || Ok(Some(7)))
        });
        assert_eq!(got, Some(7));
        assert!(lines.is_empty(), "{lines:?}");
        let (got, lines) =
            crate::test_support::captured_with(|| find_all_or_empty("widgets", || Ok(vec![1, 2])));
        assert_eq!(got, vec![1, 2]);
        assert!(lines.is_empty(), "{lines:?}");
    }

    fn posed() -> DbError {
        DbError::Internal("posed".into())
    }

    /// One capture + one silence leg per v4 line shape added with the
    /// A SQLite failure's `error` field is the driver's bare sentence, as v4's
    /// `extractErrorMessage` renders it — never `DbError::Sqlite`'s
    /// `sqlite error: ` prefix (the `97b25fc53` smalls unification).
    #[test]
    fn a_sqlite_failure_renders_v4s_bare_message() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        let (got, lines) = crate::test_support::captured_with(|| {
            find_by_id_or_none::<i32>("chats", "c1", || {
                conn.execute_batch("SELECT 1 FROM no_such_table")
                    .map(|_| None)
                    .map_err(DbError::from)
            })
        });
        assert!(got.is_none());
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert_eq!(
            lines[0],
            "ERROR quilltap::db Error finding entity by ID collection=chats id=c1 error=no such table: no_such_table",
            "{lines:?}"
        );
    }

    /// document-store fallbacks (P4.131): bytes, level, target, field order.
    #[test]
    fn each_document_store_shape_logs_v4s_line_and_answers_its_fallback() {
        let (got, lines) = crate::test_support::captured_with(|| {
            find_by_filter_or_empty::<i32>("widgets", || Err(posed()))
        });
        assert!(got.is_empty());
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(
            lines[0].starts_with(
                "ERROR quilltap::db Error finding entities by filter collection=widgets error="
            ),
            "{}",
            lines[0]
        );
        assert!(!lines[0].contains("filter="), "v4 logs no filter field");

        let (got, lines) = crate::test_support::captured_with(|| {
            find_one_by_filter_or_none::<i32>("widgets", || Err(posed()))
        });
        assert_eq!(got, None);
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(
            lines[0].starts_with(
                "ERROR quilltap::db Error finding entity by filter collection=widgets error="
            ),
            "{}",
            lines[0]
        );

        let (got, lines) = crate::test_support::captured_with(|| {
            joined_file_links_or_empty::<i32>("WHERE l.mountPointId = ?", || Err(posed()))
        });
        assert!(got.is_empty());
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(
            lines[0].starts_with(
                "ERROR quilltap::db Error querying joined file links collection=doc_mount_file_links whereClause=WHERE l.mountPointId = ? error="
            ),
            "{}",
            lines[0]
        );

        let (got, lines) = crate::test_support::captured_with(|| {
            document_by_mount_point_and_path_or_none::<i32>("mp-1", "Mail/a.md", || Err(posed()))
        });
        assert_eq!(got, None);
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(
            lines[0].starts_with(
                "ERROR quilltap::db Error finding document by mount point and path collection=doc_mount_documents mountPointId=mp-1 relativePath=Mail/a.md error="
            ),
            "{}",
            lines[0]
        );

        let (got, lines) =
            crate::test_support::captured_with(|| delete_with_gc_or_false("l-1", || Err(posed())));
        assert!(!got);
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(
            lines[0].starts_with(
                "ERROR quilltap::db Error deleting file link with GC collection=doc_mount_file_links linkId=l-1 error="
            ),
            "{}",
            lines[0]
        );
    }

    #[test]
    fn each_document_store_shape_is_silent_on_success() {
        let (got, lines) = crate::test_support::captured_with(|| {
            (
                find_by_filter_or_empty("widgets", || Ok(vec![1])),
                find_one_by_filter_or_none("widgets", || Ok(Some(2))),
                joined_file_links_or_empty("WHERE l.id = ?", || Ok(vec![3])),
                document_by_mount_point_and_path_or_none("mp", "a.md", || Ok(Some(4))),
                delete_with_gc_or_false("l", || Ok(true)),
            )
        });
        assert_eq!(got, (vec![1], Some(2), vec![3], Some(4), true));
        assert!(lines.is_empty(), "{lines:?}");
    }
}
