//! Hard-Link Groups — the port of v4 `lib/mount-index/link-groups.ts`
//! (`40319484`).
//!
//! A `linkGroupId` marks a set of `doc_mount_file_links` rows that were
//! deliberately hard-linked together (`docs link`). Within a group the bytes are
//! one file: a write through any member repoints every member at the new content
//! row, which the repository does inside the write transaction
//! (`db::doc_mount_file_links::fan_out_group_file_id`).
//!
//! What the repository can't do from inside that transaction is rebuild the
//! siblings' chunks — chunks are keyed by `linkId`, so every member maintains its
//! own chunk + embedding set. This module is that second half: after a write
//! lands, re-index the rest of the group so search results and character context
//! don't serve the previous revision from a sibling path.
//!
//! Filesystem groups need this too, for the opposite reason: the OS shares the
//! bytes through the inode (so no content fan-out is required), but the index
//! rows are per-path and would otherwise keep a stale sha, size, and chunk set
//! for every path except the one written.
//!
//! **v4's "parent-process only" caveat does not transfer** (a documented
//! non-divergence). v4 must skip this inside its forked job child, where
//! repository writes are buffered and shipped over IPC so the content this reads
//! back isn't committed yet. v5's job runner is in-process by locked decision —
//! no fork, no buffered writes — so the precondition cannot occur and the v5
//! analogue simply runs. This is the same reasoning `reindex_after_database_write`
//! already records for v4's `QUILLTAP_JOB_CHILD` guard.

use rusqlite::Connection;

use crate::db::doc_mount_file_links::DocMountFileLinksRepository;
use crate::db::doc_mount_points::DocMountPointsRepository;

use super::converters::{DocumentTextExtractor, RefusingTextExtractor};
use super::reindex_file::reindex_single_file;

/// Re-index every other member of the hard-link group containing
/// `(mount_point_id, relative_path)` — v4 `reindexLinkGroupSiblings`.
///
/// No-op — and cheap, one indexed lookup — when the link isn't grouped, which is
/// the overwhelmingly common case. Never fails: a sibling that fails to re-index
/// is logged and skipped by `reindex_single_file`'s own catch-all, because the
/// write that triggered this has already succeeded and must not be reported as
/// failed. A failure to even resolve the group is warned and swallowed here for
/// the same reason.
///
/// Returns the number of siblings re-indexed.
pub fn reindex_link_group_siblings(
    conn: &Connection,
    mount_point_id: &str,
    relative_path: &str,
    extractor: &dyn DocumentTextExtractor,
) -> usize {
    match reindex_inner(conn, mount_point_id, relative_path, extractor) {
        Ok(n) => n,
        Err(e) => {
            eprintln!(
                "Failed to re-index hard-link group: mount={mount_point_id} \
                 path={relative_path}: {e}"
            );
            0
        }
    }
}

/// The database-backed convenience wrapper, mirroring
/// [`super::reindex_file::reindex_after_database_write`]: a database-backed
/// store never consults the pdf/docx extractor seam (its bytes come out of
/// `doc_mount_documents` / `doc_mount_blobs`). A filesystem sibling inside the
/// same group still resolves its own absolute path below and would reach the
/// refusing extractor only for a pdf/docx on disk — which warns and skips,
/// exactly as the standing extractor deferral does everywhere else.
pub fn reindex_link_group_siblings_after_database_write(
    conn: &Connection,
    mount_point_id: &str,
    relative_path: &str,
) -> usize {
    reindex_link_group_siblings(conn, mount_point_id, relative_path, &RefusingTextExtractor)
}

fn reindex_inner(
    conn: &Connection,
    mount_point_id: &str,
    relative_path: &str,
    extractor: &dyn DocumentTextExtractor,
) -> Result<usize, crate::db::DbError> {
    let links = DocMountFileLinksRepository::new(conn);
    let Some(link) = links.find_by_mount_point_and_path(mount_point_id, relative_path)? else {
        return Ok(0);
    };
    let Some(group_id) = link.link_group_id.clone() else {
        return Ok(0);
    };

    let members = links.find_by_link_group_id(&group_id)?;
    let siblings: Vec<_> = members.into_iter().filter(|m| m.id != link.id).collect();
    if siblings.is_empty() {
        return Ok(0);
    }

    let points = DocMountPointsRepository::new(conn);
    let mut reindexed = 0usize;
    for sibling in &siblings {
        // Database-backed siblings read their bytes out of doc_mount_documents,
        // so they take an empty absolute path; filesystem siblings need the real
        // one resolved against their own mount's basePath (v4's `mount &&
        // mount.mountType !== 'database' && mount.basePath` — an empty basePath
        // is falsy there, hence the emptiness check).
        let mount = points.find_by_id_for_docedit(&sibling.mount_point_id)?;
        let absolute_path = match mount {
            Some(m) if m.mount_type != "database" && !m.base_path.is_empty() => {
                format!("{}/{}", m.base_path, sibling.relative_path)
            }
            _ => String::new(),
        };
        reindex_single_file(
            conn,
            &sibling.mount_point_id,
            &sibling.relative_path,
            &absolute_path,
            extractor,
        );
        // v4 increments only on success, inside its per-sibling try. v5's
        // `reindex_single_file` already swallows the failure one level down (its
        // own catch-all), so this counts attempts. The count is log-only on both
        // sides — nothing branches on it.
        reindexed += 1;
    }

    Ok(reindexed)
}
