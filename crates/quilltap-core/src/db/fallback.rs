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

use super::document_store_overlay::OverlayError;
use super::DbError;

/// The `error` field's bytes: v4 logs `extractErrorMessage(error)` — the
/// thrown error's own message, which for a SQLite failure is the driver's
/// bare sentence (`no such table: chats`). `DbError::Sqlite`'s `Display`
/// prefixes it with `sqlite error: ` (a v5 rendering convention, useful in a
/// propagated error, wrong on a line that stands in for v4's); every other
/// variant already IS a bare message. Every shape in this home renders the
/// field through here (the `97b25fc53` smalls unification — the P4.131 lines
/// had all carried the prefix, and one caller had worked around it locally).
/// `pub` since P4.134: the host's boot guards stand in for v4's
/// `instrumentation.ts` catches, whose `error` field is the same bare message.
pub fn error_text(error: &DbError) -> String {
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
/// plain string v4 passes (`WHERE l.mountPointId = ?`, …) — a `&str` since
/// P4.142 (v4's batched `findByIdsWithContent` builds `WHERE l.id IN (?,…)`
/// per call; a tracing field VALUE need not be static, only its name). Every
/// public joined read (`findByMountPointId`, `findByMountPointAndPath`,
/// `findByFileId`, `findByIdsWithContent`) wraps this in its own `safeQuery`,
/// which therefore NEVER fires — this is the only line.
pub fn joined_file_links_or_empty<T>(
    where_clause: &str,
    read: impl FnOnce() -> Result<Vec<T>, DbError>,
) -> Vec<T> {
    read().unwrap_or_else(|error| {
        tracing::error!(
            target: "quilltap::db",
            collection = "doc_mount_file_links",
            whereClause = %where_clause,
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

// === P4.136 — v4's two API-key reads (`connection-profiles.repository.ts:
// 249-288`). Both are 4-arg fallback `safeQuery`s on the repository
// constructed with `'connection_profiles'`, so the injected `collection` is
// the REPOSITORY's — never `api_keys`, the table they read — and it is
// hard-coded here rather than taken as a parameter, so no caller can pass the
// table name. A corrupt cell (a BLOB `key_value`) throws inside the wrapper
// (v4's `ApiKeySchema.parse`; v5's `marshal_row` type check) and lands on the
// line;
// a missing `api_keys` TABLE is NOT a v4 arm (`getApiKeysCollection` heals it
// with `ensureCollection` first). Per access — v4 has no once-only gate. ===

/// v4 `findApiKeyById` as its callers see it: `read()`'s `Err` logs `Error
/// finding API key by ID {collection, keyId, error}` and answers `None`
/// (`connection-profiles.repository.ts:249-265`) — the UNSCOPED read the
/// participant resolver, Carina, the gate+lookup composite and the
/// connection-profile routes make.
pub fn find_api_key_by_id_or_none(
    key_id: &str,
    read: impl FnOnce() -> Result<Option<super::api_keys::ApiKey>, DbError>,
) -> Option<super::api_keys::ApiKey> {
    read().unwrap_or_else(|error| {
        tracing::error!(
            target: "quilltap::db",
            collection = "connection_profiles",
            keyId = %key_id,
            error = %error_text(&error),
            "Error finding API key by ID"
        );
        None
    })
}

/// v4 `findApiKeyByIdAndUserId` as its callers see it: `read()`'s `Err` logs
/// `Error finding API key by ID and user ID {collection, keyId, userId,
/// error}` and answers `None` (`connection-profiles.repository.ts:270-288`) —
/// the SCOPED read the cheap-LLM resolver, image description and the
/// Concierge's understudy resolvers make.
pub fn find_api_key_by_id_and_user_id_or_none(
    key_id: &str,
    user_id: &str,
    read: impl FnOnce() -> Result<Option<super::api_keys::ApiKey>, DbError>,
) -> Option<super::api_keys::ApiKey> {
    read().unwrap_or_else(|error| {
        tracing::error!(
            target: "quilltap::db",
            collection = "connection_profiles",
            keyId = %key_id,
            userId = %user_id,
            error = %error_text(&error),
            "Error finding API key by ID and user ID"
        );
        None
    })
}
// === end P4.136 ===

// === P4.134 (dogfood #134(b)) — v4's lazy-init and boot-reachable repository
// lines, for the boot steps v5 runs eagerly where v4 degrades per read. Each
// shape is the line v4 ACTUALLY reaches on a database failure of that step
// (measured at `ca363178d`): in all three boot sites below v4's own
// `instrumentation.ts` / `seed-initial-data.ts` catch is UNREACHABLE for a
// SQLite failure, because the repository call inside it is a fallback
// `safeQuery` that logs and swallows first. ===

/// v4 `AbstractDedicatedDbRepository.ensureTable` (`dedicated-db.repository.ts:
/// 134-152`): the table DDL + the repository's `onTableEnsured` repairs inside
/// one try; a throw logs ERROR `Failed to ensure ${collection} table in
/// ${DB_LABELS[dbTarget]} database` `{error}` (`mountIndex → 'mount index'`)
/// and rethrows into the caller's `safeQuery`. v4 runs it LAZILY — on every
/// access until it succeeds, since `tableEnsured` stays false — where v5 runs
/// the same repairs once per boot (the recorded cadence divergence), so here
/// the `Err` is logged once and answered `false`: the boot continues, exactly
/// as v4's boot never touches the repair at all.
pub fn ensure_table_or_log(
    collection: &'static str,
    db_label: &'static str,
    ensure: impl FnOnce() -> Result<(), DbError>,
) -> bool {
    match ensure() {
        Ok(()) => true,
        Err(error) => {
            tracing::error!(
                target: "quilltap::db",
                error = %error_text(&error),
                "Failed to ensure {} table in {} database",
                collection,
                db_label
            );
            false
        }
    }
}

/// v4 `BaseRepository.getCollection()`'s lazy `ensureCollection`, as a failure
/// reaches the log (`base.repository.ts:113-124` + `backends/sqlite/backend.ts:
/// 763-791`): the backend logs ERROR `Failed to ensure collection` `{table,
/// error}` and rethrows into the repository's rethrow-mode `safeQuery`, which
/// logs ERROR `Failed to ensure collection exists` `{collection, error}` — two
/// lines, in that order. v5 runs the ensure at boot (`help_docs`, p4.9i2), so
/// the `Err` is logged as v4's pair and answered `false`.
pub fn ensure_collection_or_log(
    collection: &'static str,
    ensure: impl FnOnce() -> Result<(), DbError>,
) -> bool {
    match ensure() {
        Ok(()) => true,
        Err(error) => {
            let error = error_text(&error);
            tracing::error!(
                target: "quilltap::db",
                table = collection,
                error = %error,
                "Failed to ensure collection"
            );
            tracing::error!(
                target: "quilltap::db",
                collection = collection,
                error = %error,
                "Failed to ensure collection exists"
            );
            false
        }
    }
}

/// v4 `roleplayTemplates._doSeedBuiltInTemplates` as its caller sees it
/// (`roleplay-templates.repository.ts:141-191`): a FALLBACK `safeQuery(…,
/// 'Error seeding built-in roleplay templates', {}, undefined)`, so `seed()`'s
/// `Err` logs that ERROR `{collection, error}` and answers — `seed-initial-
/// data.ts:56-63`'s own ERROR `Failed to seed built-in roleplay templates` is
/// unreachable behind it on a database failure.
pub fn seed_built_in_templates_or_log(seed: impl FnOnce() -> Result<(), DbError>) -> bool {
    match seed() {
        Ok(()) => true,
        Err(error) => {
            tracing::error!(
                target: "quilltap::db",
                collection = "roleplay_templates",
                error = %error_text(&error),
                "Error seeding built-in roleplay templates"
            );
            false
        }
    }
}

/// v4 `docMountFileLinks.sweepOrphanedStoreChildren` as its caller sees it
/// (`doc-mount-file-links.repository.ts:1419-1435`): a fallback `withRawDb({0,
/// 0, 0})`, so `sweep()`'s `Err` logs ERROR `Error sweeping orphaned store
/// children` `{collection, error}` and answers the default — `instrumentation.
/// ts:682-690`'s WARN `Error reaping orphaned doc-store children, continuing
/// startup` is unreachable behind it on a database failure.
pub fn sweep_orphaned_store_children_or_default<T: Default>(
    sweep: impl FnOnce() -> Result<T, DbError>,
) -> T {
    sweep().unwrap_or_else(|error| {
        tracing::error!(
            target: "quilltap::db",
            collection = "doc_mount_file_links",
            error = %error_text(&error),
            "Error sweeping orphaned store children"
        );
        T::default()
    })
}
// === end P4.134 ===

// === P4.142 — v4's strict repository scope (`strict-failures.ts`). v4's
// `safeQuery` answers its fallback UNLESS the caller runs inside
// `withStrictRepositoryFailures` (an `AsyncLocalStorage` bit only the importer
// sets, `execute.ts:430` / `preview.ts:31` — Bug 79): there it logs the same
// line with `strictFailures: true` appended and rethrows. v5's P4.131 rule
// keeps strict callers on the PROPAGATING repository fns, so the scope matters
// only where one fallback layer serves both kinds of caller: the vault
// overlay's two batch reads, under every character / project / group list.
// The backup collect and the `.qtap` export (the 2026-08-03 "fix, don't
// match" family) enter it too — a deliberate divergence, since v4 runs them
// non-strict and exports a broken store EMPTY (RULED, the human, 2026-10-02).
// The bit is THREAD-local: a `Db::write` closure runs on the writer thread, so
// a caller enters the scope INSIDE the closure that makes the reads — and the
// scope is SYNCHRONOUS: a closure that returns a `Future` leaves the scope
// before the future is polled (nothing refuses it), so wrap the sync body,
// never an async one. Entered at the `f6426e196` recorded-divergences
// unification by the backup collect, the export's three entry points, the
// import's execute + preview, and the cascade delete's image checks. ===

thread_local! {
    static STRICT_REPOSITORY_FAILURES: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// v4 `withStrictRepositoryFailures` (`strict-failures.ts:40-42`): run `f` with
/// the scoped fallbacks suspended — a home that honours the scope logs v4's line
/// with `strictFailures=true` and propagates instead of answering its fallback.
/// Nests (the previous bit is restored on exit, panic or not).
pub fn with_strict_repository_failures<R>(f: impl FnOnce() -> R) -> R {
    struct Restore(bool);
    impl Drop for Restore {
        fn drop(&mut self) {
            STRICT_REPOSITORY_FAILURES.with(|s| s.set(self.0));
        }
    }
    let _restore = Restore(STRICT_REPOSITORY_FAILURES.with(|s| s.replace(true)));
    f()
}

/// v4 `strictRepositoryFailuresActive` (`strict-failures.ts:53-55`).
pub fn strict_repository_failures_active() -> bool {
    STRICT_REPOSITORY_FAILURES.with(|s| s.get())
}

/// v4 `docMountDocuments.findManyByMountPointsAndPath` as its callers see it: a
/// fallback `withRawDb([])`, so `read()`'s `Err` logs `Error finding documents by
/// mount point IDs and path {collection, mountPointIdCount, relativePath, error}`
/// and answers `Ok([])` (`doc-mount-documents.repository.ts:142-168`) — the
/// overlay's nine keystone/single-file reads. Inside
/// [`with_strict_repository_failures`] the line gains `strictFailures=true` and
/// the `Err` propagates (`safe-query.ts:57-71`); that is the ONLY `Err` this
/// answers.
pub fn documents_by_mount_point_ids_and_path_or_empty<T>(
    mount_point_id_count: usize,
    relative_path: &str,
    read: impl FnOnce() -> Result<Vec<T>, DbError>,
) -> Result<Vec<T>, DbError> {
    read().or_else(|error| {
        let strict = strict_repository_failures_active();
        tracing::error!(
            target: "quilltap::db",
            collection = "doc_mount_documents",
            mountPointIdCount = mount_point_id_count,
            relativePath = %relative_path,
            error = %error_text(&error),
            strictFailures = strict.then_some(true),
            "Error finding documents by mount point IDs and path"
        );
        if strict {
            Err(error)
        } else {
            Ok(Vec::new())
        }
    })
}

/// v4 `docMountDocuments.findManyByMountPointsInFolder` as its callers see it: a
/// fallback `withRawDb([])`, so `read()`'s `Err` logs `Error finding documents by
/// mount point IDs and folder {collection, mountPointIdCount, folder, extension,
/// recursive, error}` and answers `Ok([])` (`doc-mount-documents.repository.ts:
/// 179-220`; `recursive` is v4's `options.recursive === true`, a boolean —
/// `false` on the overlay's `Prompts` / `Scenarios` listings). The strict scope
/// as [`documents_by_mount_point_ids_and_path_or_empty`].
pub fn documents_by_mount_point_ids_in_folder_or_empty<T>(
    mount_point_id_count: usize,
    folder: &str,
    extension: &str,
    recursive: bool,
    read: impl FnOnce() -> Result<Vec<T>, DbError>,
) -> Result<Vec<T>, DbError> {
    read().or_else(|error| {
        let strict = strict_repository_failures_active();
        tracing::error!(
            target: "quilltap::db",
            collection = "doc_mount_documents",
            mountPointIdCount = mount_point_id_count,
            folder = %folder,
            extension = %extension,
            recursive = recursive,
            error = %error_text(&error),
            strictFailures = strict.then_some(true),
            "Error finding documents by mount point IDs and folder"
        );
        if strict {
            Err(error)
        } else {
            Ok(Vec::new())
        }
    })
}

/// v4 `docMountChunks.countEmbeddedByMountPointIds` as its callers see it: a
/// fallback `withRawDb(new Map())`, so `read()`'s `Err` logs `Error counting
/// embedded chunks by mount point IDs {collection, mountPointIdCount, error}`
/// and answers the empty map (`doc-mount-chunks.repository.ts:130-153`).
pub fn count_embedded_chunks_or_empty<T: Default>(
    mount_point_id_count: usize,
    read: impl FnOnce() -> Result<T, DbError>,
) -> T {
    read().unwrap_or_else(|error| {
        tracing::error!(
            target: "quilltap::db",
            collection = "doc_mount_chunks",
            mountPointIdCount = mount_point_id_count,
            error = %error_text(&error),
            "Error counting embedded chunks by mount point IDs"
        );
        T::default()
    })
}

/// v4 `docMountChunks.searchContent` as its callers see it: a fallback
/// `withRawDb([])`, so `read()`'s `Err` logs `Error searching chunk content
/// {collection, mountPointIdCount, queryLength, error}` and answers `[]`
/// (`doc-mount-chunks.repository.ts:172-215`). `query_length` is v4's
/// `query.length` — UTF-16 units (`jsstr::utf16_len`).
pub fn search_chunk_content_or_empty<T>(
    mount_point_id_count: usize,
    query_length: usize,
    read: impl FnOnce() -> Result<Vec<T>, DbError>,
) -> Vec<T> {
    read().unwrap_or_else(|error| {
        tracing::error!(
            target: "quilltap::db",
            collection = "doc_mount_chunks",
            mountPointIdCount = mount_point_id_count,
            queryLength = query_length,
            error = %error_text(&error),
            "Error searching chunk content"
        );
        Vec::new()
    })
}

/// v4 `docMountFileLinks.searchByNameOrPath` as its callers see it: a fallback
/// `withRawDb([])`, so `read()`'s `Err` logs `Error searching file links by name
/// or path {collection, mountPointIdCount, queryLength, error}` and answers `[]`
/// (`doc-mount-file-links.repository.ts:592-628`).
pub fn search_file_links_by_name_or_path_or_empty<T>(
    mount_point_id_count: usize,
    query_length: usize,
    read: impl FnOnce() -> Result<Vec<T>, DbError>,
) -> Vec<T> {
    read().unwrap_or_else(|error| {
        tracing::error!(
            target: "quilltap::db",
            collection = "doc_mount_file_links",
            mountPointIdCount = mount_point_id_count,
            queryLength = query_length,
            error = %error_text(&error),
            "Error searching file links by name or path"
        );
        Vec::new()
    })
}

/// v4 `docMountFiles.findByMountPointId` as its callers see it: a fallback
/// `withRawDb([])` on the repository constructed with `'doc_mount_files'`, so
/// `read()`'s `Err` logs `Error finding files by mount point ID {collection,
/// mountPointId, error}` and answers `[]` (`doc-mount-files.repository.ts:
/// 86-95`). NOT the links repository's `queryJoined` line: v4's files-list and
/// project-files routes read through this repository (`queryLinks` is a plain
/// helper inside it, no fallback of its own), so a v5 site that reads the same
/// rows through the links repository still logs THIS line (P4.142, measured
/// while converting G1 — the census had mapped both sites to the links line).
pub fn files_by_mount_point_id_or_empty<T>(
    mount_point_id: &str,
    read: impl FnOnce() -> Result<Vec<T>, DbError>,
) -> Vec<T> {
    read().unwrap_or_else(|error| {
        tracing::error!(
            target: "quilltap::db",
            collection = "doc_mount_files",
            mountPointId = %mount_point_id,
            error = %error_text(&error),
            "Error finding files by mount point ID"
        );
        Vec::new()
    })
}

/// v4 `docMountFiles.findByMountPointAndPath` as its callers see it: a
/// fallback `withRawDb(null)` on the repository constructed with
/// `'doc_mount_files'` (`queryLinks` inside it is a plain helper), so `read()`'s
/// `Err` logs `Error finding file by mount point and path {collection,
/// mountPointId, relativePath, error}` and answers `None`
/// (`doc-mount-files.repository.ts:100-117`) — the chat attach route's read
/// (`chats/[id]/files/route.ts:363-366`), NOT the links repository's line.
pub fn file_by_mount_point_and_path_or_none<T>(
    mount_point_id: &str,
    relative_path: &str,
    read: impl FnOnce() -> Result<Option<T>, DbError>,
) -> Option<T> {
    read().unwrap_or_else(|error| {
        tracing::error!(
            target: "quilltap::db",
            collection = "doc_mount_files",
            mountPointId = %mount_point_id,
            relativePath = %relative_path,
            error = %error_text(&error),
            "Error finding file by mount point and path"
        );
        None
    })
}

/// v4 `docMountChunks.clearEmbeddingsByLinkId` as its callers see it: a
/// fallback `withRawDb(0)` (a WRITE), so `write()`'s `Err` logs `Error clearing
/// embeddings by link ID {collection, linkId, error}` and answers `0`
/// (`doc-mount-chunks.repository.ts:256-276`) — which is why the embedding
/// scheduler's own `Failed to clear embeddings for embed:false link` WARN
/// (`embedding-scheduler.ts:47-53`) is unreachable.
pub fn clear_embeddings_by_link_id_or_zero(
    link_id: &str,
    write: impl FnOnce() -> Result<usize, DbError>,
) -> usize {
    write().unwrap_or_else(|error| {
        tracing::error!(
            target: "quilltap::db",
            collection = "doc_mount_chunks",
            linkId = %link_id,
            error = %error_text(&error),
            "Error clearing embeddings by link ID"
        );
        0
    })
}

/// v4 `getApiKeysByUserId` as its callers see it: `read()`'s `Err` logs
/// `Error finding API keys by user ID {collection, userId, error}` and answers
/// `[]` (`connection-profiles.repository.ts:218-244`, 4-arg fallback
/// `safeQuery` on the repository constructed with `'connection_profiles'`).
pub fn find_api_keys_by_user_id_or_empty(
    user_id: &str,
    read: impl FnOnce() -> Result<Vec<super::api_keys::ApiKey>, DbError>,
) -> Vec<super::api_keys::ApiKey> {
    read().unwrap_or_else(|error| {
        tracing::error!(
            target: "quilltap::db",
            collection = "connection_profiles",
            userId = %user_id,
            error = %error_text(&error),
            "Error finding API keys by user ID"
        );
        Vec::new()
    })
}
// === end P4.142 ===

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

    /// The two API-key shapes (P4.136): v4's line with the REPOSITORY's
    /// collection, the context keys in v4's order, and `None`.
    #[test]
    fn each_api_key_shape_logs_v4s_line_and_answers_none() {
        let (got, lines) = crate::test_support::captured_with(|| {
            find_api_key_by_id_or_none("k-1", || Err(posed()))
        });
        assert!(got.is_none());
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert_eq!(
            lines[0],
            "ERROR quilltap::db Error finding API key by ID collection=connection_profiles keyId=k-1 error=posed"
        );
        let (got, lines) = crate::test_support::captured_with(|| {
            find_api_key_by_id_and_user_id_or_none("k-1", "u-1", || Err(posed()))
        });
        assert!(got.is_none());
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert_eq!(
            lines[0],
            "ERROR quilltap::db Error finding API key by ID and user ID collection=connection_profiles keyId=k-1 userId=u-1 error=posed"
        );
    }

    /// A corrupt cell — a BLOB `key_value`, which BOTH sides fail to read (see
    /// [`test_plants`]; a missing TABLE is no v4 arm, `ensureCollection` heals
    /// it) — renders the driver's bare sentence, and a healthy row (or a miss)
    /// is silent.
    #[test]
    fn a_corrupt_api_key_cell_renders_the_bare_message_and_a_healthy_row_is_silent() {
        let conn = test_plants::conn_with_api_keys();
        test_plants::plant_api_key(&conn, "k-bad", "u-1", true);
        test_plants::plant_api_key(&conn, "k-ok", "u-1", false);
        let (got, lines) = crate::test_support::captured_with(|| {
            (
                find_api_key_by_id_or_none("k-bad", || {
                    super::super::api_keys::find_by_id(&conn, "k-bad")
                }),
                find_api_key_by_id_and_user_id_or_none("k-bad", "u-1", || {
                    super::super::api_keys::find_by_id_and_user_id(&conn, "k-bad", "u-1")
                }),
            )
        });
        assert!(got.0.is_none() && got.1.is_none());
        assert_eq!(
            lines,
            vec![
                "ERROR quilltap::db Error finding API key by ID collection=connection_profiles keyId=k-bad error=Invalid column type Blob at index: 4, name: key_value".to_string(),
                "ERROR quilltap::db Error finding API key by ID and user ID collection=connection_profiles keyId=k-bad userId=u-1 error=Invalid column type Blob at index: 4, name: key_value".to_string(),
            ]
        );
        let (got, lines) = crate::test_support::captured_with(|| {
            (
                find_api_key_by_id_or_none("k-ok", || {
                    super::super::api_keys::find_by_id(&conn, "k-ok")
                }),
                find_api_key_by_id_and_user_id_or_none("k-ok", "u-1", || {
                    super::super::api_keys::find_by_id_and_user_id(&conn, "k-ok", "u-1")
                }),
                find_api_key_by_id_or_none("k-gone", || {
                    super::super::api_keys::find_by_id(&conn, "k-gone")
                }),
            )
        });
        assert_eq!(
            got.0.map(|k| k.key_value).as_deref(),
            Some("synthetic-k-ok")
        );
        assert_eq!(
            got.1.map(|k| k.key_value).as_deref(),
            Some("synthetic-k-ok")
        );
        assert!(got.2.is_none());
        assert!(lines.is_empty(), "{lines:?}");
    }

    /// P4.142's five document-store shapes: v4's exact line (collection
    /// injected first, the context keys in v4's order, the bare `error`) and the
    /// fallback. `recursive=false` renders bare (v4's boolean).
    #[test]
    fn each_p4142_document_store_shape_logs_v4s_exact_line() {
        let (got, lines) = crate::test_support::captured_with(|| {
            documents_by_mount_point_ids_and_path_or_empty::<i32>(3, "properties.json", || {
                Err(posed())
            })
        });
        assert_eq!(got.unwrap(), Vec::<i32>::new());
        assert_eq!(
            lines,
            vec!["ERROR quilltap::db Error finding documents by mount point IDs and path collection=doc_mount_documents mountPointIdCount=3 relativePath=properties.json error=posed".to_string()]
        );

        let (got, lines) = crate::test_support::captured_with(|| {
            documents_by_mount_point_ids_in_folder_or_empty::<i32>(
                3,
                "Prompts",
                ".md",
                false,
                || Err(posed()),
            )
        });
        assert_eq!(got.unwrap(), Vec::<i32>::new());
        assert_eq!(
            lines,
            vec!["ERROR quilltap::db Error finding documents by mount point IDs and folder collection=doc_mount_documents mountPointIdCount=3 folder=Prompts extension=.md recursive=false error=posed".to_string()]
        );

        let (got, lines) = crate::test_support::captured_with(|| {
            count_embedded_chunks_or_empty::<std::collections::HashMap<String, i64>>(2, || {
                Err(posed())
            })
        });
        assert!(got.is_empty());
        assert_eq!(
            lines,
            vec!["ERROR quilltap::db Error counting embedded chunks by mount point IDs collection=doc_mount_chunks mountPointIdCount=2 error=posed".to_string()]
        );

        let (got, lines) = crate::test_support::captured_with(|| {
            search_chunk_content_or_empty::<i32>(2, 5, || Err(posed()))
        });
        assert!(got.is_empty());
        assert_eq!(
            lines,
            vec!["ERROR quilltap::db Error searching chunk content collection=doc_mount_chunks mountPointIdCount=2 queryLength=5 error=posed".to_string()]
        );

        let (got, lines) = crate::test_support::captured_with(|| {
            search_file_links_by_name_or_path_or_empty::<i32>(2, 5, || Err(posed()))
        });
        assert!(got.is_empty());
        assert_eq!(
            lines,
            vec!["ERROR quilltap::db Error searching file links by name or path collection=doc_mount_file_links mountPointIdCount=2 queryLength=5 error=posed".to_string()]
        );
    }

    #[test]
    fn each_p4142_document_store_shape_is_silent_on_success() {
        let (got, lines) = crate::test_support::captured_with(|| {
            (
                documents_by_mount_point_ids_and_path_or_empty(1, "a.json", || Ok(vec![1]))
                    .unwrap(),
                documents_by_mount_point_ids_in_folder_or_empty(1, "Prompts", ".md", true, || {
                    Ok(vec![2])
                })
                .unwrap(),
                count_embedded_chunks_or_empty(1, || Ok(3_i64)),
                search_chunk_content_or_empty(1, 1, || Ok(vec![4])),
                search_file_links_by_name_or_path_or_empty(1, 1, || Ok(vec![5])),
            )
        });
        assert_eq!(got, (vec![1], vec![2], 3, vec![4], vec![5]));
        assert!(lines.is_empty(), "{lines:?}");
    }

    /// v4's strict scope (`safe-query.ts:57-71`): the same line with
    /// `strictFailures=true` LAST, and the `Err` propagates; the bit is
    /// restored on exit, so the very next read falls back again.
    #[test]
    fn the_strict_scope_appends_strict_failures_and_propagates() {
        let (got, lines) = crate::test_support::captured_with(|| {
            let strict = with_strict_repository_failures(|| {
                assert!(strict_repository_failures_active());
                (
                    documents_by_mount_point_ids_and_path_or_empty::<i32>(1, "p.json", || {
                        Err(posed())
                    }),
                    documents_by_mount_point_ids_in_folder_or_empty::<i32>(
                        1,
                        "Scenarios",
                        ".md",
                        false,
                        || Err(posed()),
                    ),
                )
            });
            let after =
                documents_by_mount_point_ids_and_path_or_empty::<i32>(1, "p.json", || Err(posed()));
            (strict, after, strict_repository_failures_active())
        });
        let ((path, folder), after, active) = got;
        assert!(matches!(path, Err(DbError::Internal(ref m)) if m == "posed"));
        assert!(matches!(folder, Err(DbError::Internal(ref m)) if m == "posed"));
        assert_eq!(after.unwrap(), Vec::<i32>::new());
        assert!(!active);
        assert_eq!(
            lines,
            vec![
                "ERROR quilltap::db Error finding documents by mount point IDs and path collection=doc_mount_documents mountPointIdCount=1 relativePath=p.json error=posed strictFailures=true".to_string(),
                "ERROR quilltap::db Error finding documents by mount point IDs and folder collection=doc_mount_documents mountPointIdCount=1 folder=Scenarios extension=.md recursive=false error=posed strictFailures=true".to_string(),
                "ERROR quilltap::db Error finding documents by mount point IDs and path collection=doc_mount_documents mountPointIdCount=1 relativePath=p.json error=posed".to_string(),
            ]
        );
    }

    /// The scope nests and survives a panic inside it (the bit is restored).
    #[test]
    fn the_strict_scope_nests_and_restores_after_a_panic() {
        with_strict_repository_failures(|| {
            with_strict_repository_failures(|| assert!(strict_repository_failures_active()));
            assert!(
                strict_repository_failures_active(),
                "the outer scope stays strict"
            );
        });
        assert!(!strict_repository_failures_active());
        let caught = std::panic::catch_unwind(|| with_strict_repository_failures(|| panic!("x")));
        assert!(caught.is_err());
        assert!(!strict_repository_failures_active());
    }

    /// The files repository's PATH line (P4.142 G2 — the chat attach route).
    #[test]
    fn the_file_by_path_shape_logs_v4s_line() {
        let (got, lines) = crate::test_support::captured_with(|| {
            file_by_mount_point_and_path_or_none::<i32>("mp-1", "a.md", || Err(posed()))
        });
        assert!(got.is_none());
        assert_eq!(
            lines,
            vec!["ERROR quilltap::db Error finding file by mount point and path collection=doc_mount_files mountPointId=mp-1 relativePath=a.md error=posed".to_string()]
        );
        let (got, lines) = crate::test_support::captured_with(|| {
            file_by_mount_point_and_path_or_none("mp-1", "a.md", || Ok(Some(1)))
        });
        assert_eq!(got, Some(1));
        assert!(lines.is_empty(), "{lines:?}");
    }

    /// The chunk-clear write's line (P4.142 Tier 2) — answers 0.
    #[test]
    fn the_clear_embeddings_shape_logs_v4s_line_and_answers_zero() {
        let (got, lines) = crate::test_support::captured_with(|| {
            clear_embeddings_by_link_id_or_zero("l-1", || Err(posed()))
        });
        assert_eq!(got, 0);
        assert_eq!(
            lines,
            vec!["ERROR quilltap::db Error clearing embeddings by link ID collection=doc_mount_chunks linkId=l-1 error=posed".to_string()]
        );
        let (got, lines) = crate::test_support::captured_with(|| {
            clear_embeddings_by_link_id_or_zero("l-1", || Ok(3))
        });
        assert_eq!(got, 3);
        assert!(lines.is_empty(), "{lines:?}");
    }

    /// The files repository's list line (P4.142, G1) — collection
    /// `doc_mount_files`, never the links repository's.
    #[test]
    fn the_files_by_mount_point_id_shape_logs_v4s_line() {
        let (got, lines) = crate::test_support::captured_with(|| {
            files_by_mount_point_id_or_empty::<i32>("mp-1", || Err(posed()))
        });
        assert!(got.is_empty());
        assert_eq!(
            lines,
            vec!["ERROR quilltap::db Error finding files by mount point ID collection=doc_mount_files mountPointId=mp-1 error=posed".to_string()]
        );
        let (got, lines) = crate::test_support::captured_with(|| {
            files_by_mount_point_id_or_empty("mp-1", || Ok(vec![1]))
        });
        assert_eq!(got, vec![1]);
        assert!(lines.is_empty(), "{lines:?}");
    }

    /// P4.139's delivered home (the Shared contract): v4's bytes with the
    /// REPOSITORY's collection, and a healthy key is silent.
    #[test]
    fn the_api_keys_by_user_id_shape_logs_v4s_line_and_answers_empty() {
        let (got, lines) = crate::test_support::captured_with(|| {
            find_api_keys_by_user_id_or_empty("u-1", || Err(posed()))
        });
        assert!(got.is_empty());
        assert_eq!(
            lines,
            vec!["ERROR quilltap::db Error finding API keys by user ID collection=connection_profiles userId=u-1 error=posed".to_string()]
        );
        let conn = test_plants::conn_with_api_keys();
        test_plants::plant_api_key(&conn, "k-ok", "u-1", false);
        let healthy = super::super::api_keys::find_by_id(&conn, "k-ok")
            .unwrap()
            .unwrap();
        let (got, lines) = crate::test_support::captured_with(|| {
            find_api_keys_by_user_id_or_empty("u-1", || Ok(vec![healthy]))
        });
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].key_value, "synthetic-k-ok");
        assert!(lines.is_empty(), "{lines:?}");
    }
}

/// Planted `api_keys` rows for the P4.136 unit pins across core: the table
/// (v4's DDL) and a row whose `key_value` is `synthetic-<id>`, or — `corrupt`
/// — a BLOB. That is the plant BOTH sides fail on, measured (the
/// `title_update_tier3` lifted case): v4's backend decodes a stray Buffer as
/// Float32 and `ApiKeySchema.parse`'s `z.string()` refuses it; v5's
/// `marshal_row` answers `InvalidColumnType`. A text `isActive` is NOT one —
/// v4 coerces a non-number boolean cell with `Boolean(value)`.
#[cfg(test)]
pub(crate) mod test_plants {
    use rusqlite::Connection;

    pub(crate) const API_KEYS_DDL: &str = "CREATE TABLE IF NOT EXISTS api_keys (\
        id TEXT PRIMARY KEY, userId TEXT NOT NULL, label TEXT NOT NULL, \
        provider TEXT NOT NULL, key_value TEXT NOT NULL, isActive INTEGER DEFAULT 1, \
        lastUsed TEXT, createdAt TEXT NOT NULL, updatedAt TEXT NOT NULL);";

    pub(crate) fn conn_with_api_keys() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(API_KEYS_DDL).unwrap();
        conn
    }

    /// A pooled [`crate::db::runtime::Db`] over a main DB holding only the
    /// `api_keys` table and the planted `(id, userId, corrupt)` rows — for
    /// sites that read through `db.read_main`.
    pub(crate) fn db_with_api_keys(
        rows: &[(&str, &str, bool)],
    ) -> (tempfile::TempDir, crate::db::runtime::Db) {
        const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("main.db");
        {
            let w = crate::db::Writer::open_writable(&path, PEPPER).unwrap();
            w.connection().execute_batch(API_KEYS_DDL).unwrap();
            for (id, user_id, corrupt) in rows {
                plant_api_key(w.connection(), id, user_id, *corrupt);
            }
        }
        let db = crate::db::runtime::Db::open_main(&path, PEPPER).unwrap();
        (dir, db)
    }

    pub(crate) fn plant_api_key(conn: &Connection, id: &str, user_id: &str, corrupt: bool) {
        let key_value = if corrupt {
            "x'00000000'"
        } else {
            "'synthetic-' || ?1"
        };
        conn.execute(
            &format!(
                "INSERT INTO api_keys (id, userId, label, provider, key_value, isActive, \
                 createdAt, updatedAt) VALUES (?1, ?2, 'k', 'OPENAI', {key_value}, 1, \
                 '2026-10-01T00:00:00.000Z', '2026-10-01T00:00:00.000Z')"
            ),
            rusqlite::params![id, user_id],
        )
        .unwrap();
    }
}

/// v4 `ProjectsRepository.canCharacterParticipate` as its ONE caller
/// (`projectRosterAdmits`, `lib/projects/roster-access.ts`) sees it: a fallback
/// `safeQuery(…, 'Error checking character participation', { projectId,
/// characterId }, false)` around `findById` (`projects.repository.ts:186-206`).
/// Fail-closed in TWO layers, each with its own v4 line:
///
/// - the inner `_findById` is itself a fallback read, so a SLIM-ROW failure logs
///   `Error finding entity by ID {collection: projects, id}` and answers `null`
///   → `false` with NO outer line;
/// - a throw from the store OVERLAY (`applyOverlayOne` — the store missing or
///   unreadable) passes the inner read and lands in the OUTER catch → ERROR
///   `Error checking character participation {collection: projects, projectId,
///   characterId, error}` → `false`.
///
/// v5's `can_character_participate` propagates both as `OverlayError`; this home
/// maps each to its v4 line. The split is attributable because
/// `OverlayError::Db` can only come from the slim MAIN read: the overlay's own
/// mount read is the fallback `documents_by_mount_point_ids_and_path_or_empty`
/// (P4.142), so a mount failure surfaces as `Unavailable` (`properties.json
/// missing`) — the OUTER line, exactly as v4. (Inside
/// [`with_strict_repository_failures`] v4 would THROW from the inner read where
/// v5 logs the inner line and answers `false`; no roster-gate site runs under
/// the strict scope — backup, export, import and the cascade delete alone do.)
/// The `error` field's bytes: the bare SQLite message for the inner arm, the
/// overlay error's own message (`Project … has no usable document store (…):
/// …`, v4's `ProjectStoreUnavailableError.message`) for the outer.
pub fn can_character_participate_or_false(
    project_id: &str,
    character_id: &str,
    check: impl FnOnce() -> Result<bool, OverlayError>,
) -> bool {
    match check() {
        Ok(allowed) => allowed,
        Err(OverlayError::Db(error)) => {
            // v4's inner `_findById` line, through the ONE emitter of it.
            let _: Option<()> = find_by_id_or_none("projects", project_id, || Err(error));
            false
        }
        Err(unavailable) => {
            tracing::error!(
                target: "quilltap::db",
                collection = "projects",
                projectId = %project_id,
                characterId = %character_id,
                error = %unavailable,
                "Error checking character participation"
            );
            false
        }
    }
}
