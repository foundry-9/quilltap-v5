//! Realtime hints for the document stores — the port of v4
//! `lib/mount-index/realtime.ts` (v4 `aec626eff`, P4.D381 R-5) plus the
//! PUBLISH half of `lib/mount-index/db-store-events.ts:68-83`.
//!
//! v4's doc, carried: a store's contents changed, so open browsers of THAT
//! store should refetch. The hint carries no data — `publishRealtime(
//! 'mountPoints', id)` — and the client maps it to the store's listing,
//! detail and file queries. Every parent-side chokepoint that changes a
//! store's files or folders calls [`publish_mount_change`]: the five published
//! file-op wrappers, the database-store emit sites, the folder ops, the
//! folders route, and unlink.
//!
//! v4's job-child sentence, rewritten for v5: v4 publishes nothing from its
//! forked job child (its buffered writes surface through `job-topics.ts`).
//! v5 has no child — an in-process job's store write publishes HERE, on the
//! writer thread, and its batch may ALSO surface through
//! `realtime::job_topics` where that table names the write. Both land on the
//! same `(mountPoints, <id>)` key inside one coalescing window, so the client
//! sees one hint either way.
//!
//! ## The emit helpers — the publish half only
//!
//! [`emit_document_written`] / [`emit_document_deleted`] /
//! [`emit_document_moved`] are the publish half of v4's
//! `emitDocumentWritten` / `Deleted` / `Moved`. The OTHER half — the
//! subscription the mount watcher uses to schedule a debounced re-embed — is
//! the standing "chokidar-equivalent watcher (host cadence)" deferral
//! (`phase-4.md`), so the helpers take the path arguments v4's events carry
//! (R-P7) although only `mount_point_id` is published today: the watcher can
//! subscribe later without moving a call site. v4 logs nothing at an emit, so
//! these add no line.

use crate::realtime::bus::publish_realtime;
use crate::realtime::types::RealtimeTopic;

/// v4 `publishMountChange(mountPointId)` — `publishRealtime('mountPoints', id)`.
pub fn publish_mount_change(mount_point_id: &str) {
    publish_realtime(RealtimeTopic::MountPoints, Some(mount_point_id));
}

/// The publish half of v4 `emitDocumentWritten({mountPointId, relativePath})`.
/// `_relative_path` is the event payload the deferred watcher subscription
/// will read (R-P7).
pub fn emit_document_written(mount_point_id: &str, _relative_path: &str) {
    publish_mount_change(mount_point_id);
}

/// The publish half of v4 `emitDocumentDeleted({mountPointId, relativePath})`.
pub fn emit_document_deleted(mount_point_id: &str, _relative_path: &str) {
    publish_mount_change(mount_point_id);
}

/// The publish half of v4 `emitDocumentMoved({mountPointId, fromRelativePath,
/// toRelativePath})`.
pub fn emit_document_moved(mount_point_id: &str, _from: &str, _to: &str) {
    publish_mount_change(mount_point_id);
}
