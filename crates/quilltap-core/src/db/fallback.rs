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

use super::document_store_overlay::{OverlayError, StoreKind};
use super::table_shape::Partition;
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
///
/// A sibling's [`DbError::PartitionUnavailable`] renders v4's DEGRADED guard
/// sentence (`mount-index-guard.ts:20`, `llm-logs-guard.ts:21`) — the message
/// `acquireDb()` throws inside v4's `safeQuery`, so every fallback / rethrow
/// line over an unavailable sibling carries it (the `94fbb1ae3` boot-hardness
/// unification, P4.159's HANDOFF; contract C1's reasoning — after P4.159 the
/// only reachable unavailable sibling state is degraded). The main partition
/// has no guard and keeps v5's Display.
pub fn error_text(error: &DbError) -> String {
    match error {
        DbError::Sqlite(e) => e.to_string(),
        DbError::PartitionUnavailable(target) => {
            let partition = match target {
                crate::write_partition::WriteDbTarget::Main => Partition::Main,
                crate::write_partition::WriteDbTarget::MountIndex => Partition::MountIndex,
                crate::write_partition::WriteDbTarget::LlmLogs => Partition::LlmLogs,
            };
            partition_degraded_sentence(partition)
                .map(str::to_string)
                .unwrap_or_else(|| error.to_string())
        }
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
        log_find_by_id_failure(collection, id, &error, false);
        None
    })
}

/// The ONE emitter of `Error finding entity by ID` (both the plain twin and
/// [`find_by_id_strict_aware`] log through it).
fn log_find_by_id_failure(collection: &str, id: &str, error: &DbError, strict: bool) {
    tracing::error!(
        target: "quilltap::db",
        collection = collection,
        id = %id,
        error = %error_text(error),
        strictFailures = strict.then_some(true),
        "Error finding entity by ID"
    );
}

/// P4.163 (R-D) — [`find_by_id_or_none`] for a repository whose OWN methods are
/// reached from inside the strict scope: v4's `_findById` is a fallback
/// `safeQuery` (`base.repository.ts:247-258`), and EVERY fallback `safeQuery`
/// honours `withStrictRepositoryFailures` (`safe-query.ts:57-71`) — outside the
/// scope the line and `Ok(None)`; inside it the line gains
/// `strictFailures=true` and the `Err` propagates to the caller's own
/// `safeQuery` (`_update`'s rethrow line, then the method's wrap). A SIBLING,
/// not a changed twin — the plain twin answers a bare `Option` to callers no
/// strict scope reaches.
pub fn find_by_id_strict_aware<T>(
    collection: &'static str,
    id: &str,
    read: impl FnOnce() -> Result<Option<T>, DbError>,
) -> Result<Option<T>, DbError> {
    read().or_else(|error| {
        let strict = strict_repository_failures_active();
        log_find_by_id_failure(collection, id, &error, strict);
        if strict {
            Err(error)
        } else {
            Ok(None)
        }
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
        log_find_by_filter_failure(collection, &error, false);
        Vec::new()
    })
}

/// The ONE emitter of `Error finding entities by filter` (both the plain twin
/// and [`find_by_filter_strict_aware`] log through it).
fn log_find_by_filter_failure(collection: &str, error: &DbError, strict: bool) {
    tracing::error!(
        target: "quilltap::db",
        collection = collection,
        error = %error_text(error),
        strictFailures = strict.then_some(true),
        "Error finding entities by filter"
    );
}

/// P4.156 — [`find_by_filter_or_empty`] for a repository whose OWN methods are
/// reached from inside the strict scope (the chat-informs repository: the
/// `.qtap` export's `findByChatId`, the importer, the restore). v4's
/// `findByFilter` is a fallback `safeQuery`, and EVERY fallback `safeQuery`
/// honours `withStrictRepositoryFailures` (`safe-query.ts:57-71`): outside the
/// scope the line and `Ok([])`; inside it the line gains `strictFailures=true`
/// and the `Err` propagates to the method's own (outer) `safeQuery`. A SIBLING,
/// not a changed twin — the plain twin answers a bare `Vec` to dozens of
/// callers no strict scope reaches.
pub fn find_by_filter_strict_aware<T>(
    collection: &'static str,
    read: impl FnOnce() -> Result<Vec<T>, DbError>,
) -> Result<Vec<T>, DbError> {
    read().or_else(|error| {
        let strict = strict_repository_failures_active();
        log_find_by_filter_failure(collection, &error, strict);
        if strict {
            Err(error)
        } else {
            Ok(Vec::new())
        }
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
        log_joined_file_links_failure(where_clause, &error, false);
        Vec::new()
    })
}

/// The ONE emitter of `Error querying joined file links` (both the plain twin
/// and [`joined_file_links_strict_aware`] log through it).
fn log_joined_file_links_failure(where_clause: &str, error: &DbError, strict: bool) {
    tracing::error!(
        target: "quilltap::db",
        collection = "doc_mount_file_links",
        whereClause = %where_clause,
        error = %error_text(error),
        strictFailures = strict.then_some(true),
        "Error querying joined file links"
    );
}

/// P4.149 (item 4) — [`joined_file_links_or_empty`] for a caller the importer
/// reaches: v4's `queryJoined` is a fallback `withRawDb`, and EVERY fallback
/// `safeQuery` honours `withStrictRepositoryFailures` (`safe-query.ts:57-71`),
/// so inside the scope the line gains `strictFailures=true` and the `Err`
/// propagates (the import fails, as v4's does); outside it answers `Ok([])`.
/// A SIBLING, not a changed twin — the plain twin has 7 callers and its
/// 32-caller path-read wrapper answers a bare `Option` (survey §4).
///
/// v4's one arm that does NOT honour the scope is carried too: `withRawDb`'s
/// `acquireDb()` failure answers the fallback QUIETLY, strict or not, with a
/// DEBUG (`dedicated-db.repository.ts:242-251`) — v5's
/// [`DbError::PartitionUnavailable`] (the mount-index partition not open).
///
/// **That arm is UNREACHABLE in production** (P4.156 Tier 2 item 7, measured
/// over every caller): the one caller,
/// `DocMountFileLinksRepository::find_by_mount_point_and_path_or_none_strict_aware`
/// (reached only from `file_storage::store_mount_blob`), runs its closure over
/// a `&Connection` it already HOLDS — the mount writer acquired upstream — and
/// a rusqlite read over a held connection cannot answer `PartitionUnavailable`,
/// which only `Db::read_mount_index` (and its llm-logs twin) mint, before any
/// connection exists. v5's partition-acquire failure therefore surfaces
/// upstream of this home; v4's acquire-inside-the-read shape has no v5
/// counterpart at this site. The arm stays (it is v4's shape, and a caller that
/// hands in a `read_mount_index` result would reach it) and is unit-pinned by
/// its posed error; where a mount checkout CAN fail inside the read — the chat
/// PUT's project gate (`api/salon.rs`) — the same DEBUG is logged through
/// [`log_mount_index_unavailable`].
pub fn joined_file_links_strict_aware<T>(
    where_clause: &str,
    read: impl FnOnce() -> Result<Vec<T>, DbError>,
) -> Result<Vec<T>, DbError> {
    match read() {
        Ok(rows) => Ok(rows),
        Err(error @ DbError::PartitionUnavailable(_)) => {
            log_mount_index_unavailable("doc_mount_file_links", &error);
            Ok(Vec::new())
        }
        Err(error) => {
            let strict = strict_repository_failures_active();
            log_joined_file_links_failure(where_clause, &error, strict);
            if strict {
                Err(error)
            } else {
                Ok(Vec::new())
            }
        }
    }
}

/// v4's guard sentence for a DEGRADED dedicated partition — the message
/// `acquireDb()` throws (`mount-index-guard.ts:20`, `llm-logs-guard.ts:21`).
/// `None` for the main partition: v4 has no main guard.
fn partition_degraded_sentence(partition: Partition) -> Option<&'static str> {
    match partition {
        Partition::Main => None,
        Partition::MountIndex => Some("Mount index database is in degraded mode"),
        Partition::LlmLogs => Some("LLM logs database is in degraded mode"),
    }
}

/// The ONE emitter of v4's quiet `withRawDb` arm (`dedicated-db.repository.ts:
/// 235-263`, the DEBUG at `:246`): the dedicated database could not be
/// acquired, so the read answers its fallback with DEBUG `Dedicated database
/// unavailable; answering with the fallback {collection, dbTarget, error}` —
/// never an ERROR, strict scope or not. `collection` is the repository the read
/// belongs to; `dbTarget` is v4's spelling (`mountIndex` / `llmLogs`,
/// [`Partition::db_target`]).
///
/// P4.163 (contract C1 item 1, Ruling R-C): for [`DbError::PartitionUnavailable`]
/// the `error` value is v4's DEGRADED guard sentence for `partition` (`Mount
/// index database is in degraded mode` / `LLM logs database is in degraded
/// mode`) — after P4.159 the only reachable unavailable state of a sibling is
/// degraded (its path is created at provisioning; the `None`-path case is a
/// recorded pre-existing divergence, not a state this home renders). Any other
/// `DbError` renders [`error_text`].
pub fn log_partition_unavailable(partition: Partition, collection: &str, error: &DbError) {
    // `error_text` renders the guard sentence for a sibling's
    // `PartitionUnavailable` (keyed by the error's own target, which every
    // caller passes as `partition`).
    let error = error_text(error);
    tracing::debug!(
        target: "quilltap::db",
        collection = collection,
        dbTarget = partition.db_target(),
        error = %error,
        "Dedicated database unavailable; answering with the fallback"
    );
}

/// [`log_partition_unavailable`] for the mount index — kept as a thin wrapper
/// so its callers stay unchanged (`pub` since P4.156: the chat PUT's project
/// gate stands in for v4's overlay reads when its mount-index checkout fails,
/// `api/salon.rs`).
pub fn log_mount_index_unavailable(collection: &str, error: &DbError) {
    log_partition_unavailable(Partition::MountIndex, collection, error);
}

/// P4.163 (contract C1 item 2) — v4 `verifyStructure`'s problem string for a
/// repository whose dedicated database is DEGRADED (`dedicated-db.repository.
/// ts:176-182`): `acquireDb()` threw the guard sentence, so the pass records
/// `"{label} database unavailable: {Sentence}"` — `Mount index database
/// unavailable: …` is `mount index database unavailable: Mount index database
/// is in degraded mode`, the LLM-logs twin `LLM logs database unavailable: LLM
/// logs database is in degraded mode` — through [`table_shape::unavailable`]
/// with `label` = [`Partition::label`]. v4's string carries no repository name
/// (the structural pass's per-problem ERROR does, so `repository` is accepted
/// for the caller's symmetry and not rendered); this logs NOTHING. The main
/// partition has no guard, so it renders its `label` with `error_text`'s
/// counterpart — `partition not available: main` — a shape v4 never reaches.
///
/// [`table_shape::unavailable`]: super::table_shape::unavailable
pub fn log_partition_structural_unavailable(partition: Partition, _repository: &str) -> String {
    let sentence = partition_degraded_sentence(partition)
        .map(str::to_string)
        .unwrap_or_else(|| {
            DbError::PartitionUnavailable(crate::write_partition::WriteDbTarget::Main).to_string()
        });
    super::table_shape::unavailable(partition.label(), &sentence)
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

// === P4.149 — v4's base-repository RETHROW lines and the per-repository
// fallback wraps above them. `_create` / `_update` / `_delete`
// (`base.repository.ts:350-447`) are 3-argument `safeQuery`s: a failure logs
// ERROR and RETHROWS — "log and propagate", never a fallback — with
// `strictFailures: true` appended inside the strict scope (`safe-query.ts:
// 57-71`). The FALLBACK sits one layer up, in a repository method that wraps
// the call in its own 4-argument `safeQuery`. Measured through a
// `Logger.prototype` spy on the REAL `ChatInformsRepository` at `07b8f0209`
// (contract C2): `{collection, error}` on create, `{collection, id, error}` on
// update and delete, `strictFailures` LAST. v4's backend line beneath each
// (`SQLite insertOne error {table, error}`, …) is unported by standing
// convention; so are the success INFOs (`Entity created`, `Entity deleted`) and
// the not-found WARNs (Ruling R-A). ===

/// v4 `_create`'s rethrow line, `createErrorMessage()`'s default sentence
/// (`base.repository.ts:320-322`): ERROR `Error creating entity {collection,
/// error, strictFailures?}`. Logs only — the caller keeps propagating `error`.
pub fn log_create_failure(collection: &str, error: &DbError) {
    tracing::error!(
        target: "quilltap::db",
        collection = collection,
        error = %error_text(error),
        strictFailures = strict_repository_failures_active().then_some(true),
        "Error creating entity"
    );
}

/// v4's base-repository refusal of a create whose entity fails its schema
/// (`base.repository.ts:130-141, 350-379`): `validate`'s ERROR `Data
/// validation failed {collection, error}` — a DIRECT logger call, so it never
/// carries `strictFailures` — then `_create`'s rethrowing `safeQuery` line
/// [`log_create_failure`]. `zod` is the `ZodError.message`. Logs only. Shared
/// by the `.qtap` import and the backup restore (P4.161); the restore runs
/// outside the strict scope, so its second line carries no `strictFailures`.
/// Folded here from P4.161's lane-local copy at the `94fbb1ae3` boot-hardness
/// unification (§S.1 — C1 had described `log_create_failure` as this pair).
pub fn log_refused_create(collection: &str, zod: &str) {
    tracing::error!(
        target: "quilltap::db",
        collection = collection,
        error = %zod,
        "Data validation failed"
    );
    log_create_failure(collection, &DbError::Internal(zod.to_string()));
}

/// v4's STORE-BACKED override of `createErrorMessage()`
/// (`store-backed.repository.ts:234-236`): `_create`'s rethrowing `safeQuery`
/// logs ERROR `Error creating {project|group} entity {collection, error,
/// strictFailures?}` — NOT the base [`log_create_failure`] sentence (P4.155
/// measured it at `94fbb1ae3`; neither `projects.repository.ts` nor
/// `groups.repository.ts` overrides it, the shared store-backed base does).
/// `error` is the already-rendered text (the refused import passes the
/// ZodError message). Logs only — the caller keeps propagating. Folded here
/// from P4.155's lane-local copy at the `94fbb1ae3` smalls unification (§S.1).
pub fn log_store_entity_create_failure(kind: StoreKind, error: &str) {
    let strict = strict_repository_failures_active().then_some(true);
    match kind {
        StoreKind::Project => tracing::error!(
            target: "quilltap::db",
            collection = "projects",
            error = %error,
            strictFailures = strict,
            "Error creating project entity"
        ),
        StoreKind::Group => tracing::error!(
            target: "quilltap::db",
            collection = "groups",
            error = %error,
            strictFailures = strict,
            "Error creating group entity"
        ),
    }
}

/// v4's store-backed `create`'s OWN `safeQuery` wrap (`store-backed.
/// repository.ts:130-175`), one level ABOVE [`log_store_entity_create_failure`]:
/// ERROR `Error creating {project|group} {collection, name, error,
/// strictFailures?}`, `name` the create payload's — omitted when absent, as
/// winston drops `undefined`; a non-string name renders as its JSON text.
/// Logs only. Folded from P4.155's lane-local copy (§S.1).
pub fn log_store_create_failure(kind: StoreKind, name: Option<&str>, error: &str) {
    let strict = strict_repository_failures_active().then_some(true);
    let name = name.map(tracing::field::display);
    match kind {
        StoreKind::Project => tracing::error!(
            target: "quilltap::db",
            collection = "projects",
            name = name,
            error = %error,
            strictFailures = strict,
            "Error creating project"
        ),
        StoreKind::Group => tracing::error!(
            target: "quilltap::db",
            collection = "groups",
            name = name,
            error = %error,
            strictFailures = strict,
            "Error creating group"
        ),
    }
}

/// v4 `chats.repository.ts:280`'s own `safeQuery` wrap ABOVE the base `_create`
/// — ERROR `Failed to create chat {collection: chats, error, strictFailures?}`,
/// logged right after [`log_create_failure`]'s line by every chat-creating
/// caller v4 routes through `repos.chats.create` (the Concierge validation
/// refusal, the `.qtap` import, the restore). Logs only — the caller keeps
/// propagating (the import and the restore skip that chat). ONE home since the
/// `07b8f0209` follow-ups unification (three hand copies before it).
pub fn log_chat_create_wrap_failure(error: &DbError) {
    tracing::error!(
        target: "quilltap::db",
        collection = "chats",
        error = %error_text(error),
        strictFailures = strict_repository_failures_active().then_some(true),
        "Failed to create chat"
    );
}

// === P4.163 (contract C1 item 3) — the per-repository CREATE wraps the
// import and the restore log when they refuse a row. Each v4 repository's
// `create` wraps the base `_create` in its OWN 3-argument (rethrow)
// `safeQuery`, so a refusal logs `_create`'s line ([`log_create_failure`], or
// the characters override below) and then the wrap, both ERROR, `{collection,
// …context, error, strictFailures?}` — `strictFailures=true` appended only
// inside [`with_strict_repository_failures`] (the import runs strict, the
// restore does not). Measured at `94fbb1ae3` through a `Logger.prototype` spy
// on v4's REAL repositories (`create_wrap_lines_equivalence`); a context field
// v4 reads as `undefined` is OMITTED (winston drops it), hence the `Option`s.
// Logs only — the caller keeps propagating. ===

/// Every per-kind wrap below renders through here: ERROR at `quilltap::db`,
/// `collection` first, then the kind's context IN v4's ORDER, the bare `error`,
/// `strictFailures` last. A tracing message must be a literal, so each kind
/// keeps its own `tracing::error!` and this only computes the shared values.
fn create_wrap_values(error: &DbError) -> (String, Option<bool>) {
    (
        error_text(error),
        strict_repository_failures_active().then_some(true),
    )
}

/// v4 `characters.repository.ts:352-354` OVERRIDES `createErrorMessage()`, so a
/// refused character's `_create` logs `Error creating character entity`, not
/// [`log_create_failure`]'s sentence — preceded, as every schema refusal is, by
/// `validate`'s `Data validation failed {collection, error}` (no
/// `strictFailures`: `validate` logs directly, outside any `safeQuery`). This
/// home renders that PAIR for a refused character row; the wrap above it is
/// [`log_character_create_wrap_failure`].
pub fn log_character_create_failure(error: &DbError) {
    let (error, strict) = create_wrap_values(error);
    tracing::error!(
        target: "quilltap::db",
        collection = "characters",
        error = %error,
        "Data validation failed"
    );
    tracing::error!(
        target: "quilltap::db",
        collection = "characters",
        error = %error,
        strictFailures = strict,
        "Error creating character entity"
    );
}

/// `characters.create`'s wrap (`characters.repository.ts:262-314`): ERROR
/// `Error creating character {collection, userId, name, error, strictFailures?}`.
pub fn log_character_create_wrap_failure(user_id: &str, name: Option<&str>, error: &DbError) {
    let (error, strict) = create_wrap_values(error);
    tracing::error!(
        target: "quilltap::db",
        collection = "characters",
        userId = %user_id,
        name = name.map(tracing::field::display),
        error = %error,
        strictFailures = strict,
        "Error creating character"
    );
}

/// `connectionProfiles.create`'s wrap (`connection-profiles.repository.ts:67-86`):
/// ERROR `Error creating connection profile {collection, userId, name, provider,
/// error, strictFailures?}`.
pub fn log_connection_profile_create_wrap_failure(
    user_id: &str,
    name: Option<&str>,
    provider: Option<&str>,
    error: &DbError,
) {
    let (error, strict) = create_wrap_values(error);
    tracing::error!(
        target: "quilltap::db",
        collection = "connection_profiles",
        userId = %user_id,
        name = name.map(tracing::field::display),
        provider = provider.map(tracing::field::display),
        error = %error,
        strictFailures = strict,
        "Error creating connection profile"
    );
}

/// `imageProfiles.create`'s wrap (`image-profiles.repository.ts:55-75`): ERROR
/// `Error creating image profile {collection, userId, name, provider, error,
/// strictFailures?}`.
pub fn log_image_profile_create_wrap_failure(
    user_id: &str,
    name: Option<&str>,
    provider: Option<&str>,
    error: &DbError,
) {
    let (error, strict) = create_wrap_values(error);
    tracing::error!(
        target: "quilltap::db",
        collection = "image_profiles",
        userId = %user_id,
        name = name.map(tracing::field::display),
        provider = provider.map(tracing::field::display),
        error = %error,
        strictFailures = strict,
        "Error creating image profile"
    );
}

/// `embeddingProfiles.create`'s wrap (`embedding-profiles.repository.ts:73-93`):
/// ERROR `Error creating embedding profile {collection, userId, name, provider,
/// error, strictFailures?}`.
pub fn log_embedding_profile_create_wrap_failure(
    user_id: &str,
    name: Option<&str>,
    provider: Option<&str>,
    error: &DbError,
) {
    let (error, strict) = create_wrap_values(error);
    tracing::error!(
        target: "quilltap::db",
        collection = "embedding_profiles",
        userId = %user_id,
        name = name.map(tracing::field::display),
        provider = provider.map(tracing::field::display),
        error = %error,
        strictFailures = strict,
        "Error creating embedding profile"
    );
}

/// `files.create`'s wrap (`files.repository.ts:134-151`): ERROR `Error creating
/// file {collection, userId, filename, error, strictFailures?}` — `filename` is
/// the payload's `originalFilename`.
pub fn log_file_create_wrap_failure(user_id: &str, filename: Option<&str>, error: &DbError) {
    let (error, strict) = create_wrap_values(error);
    tracing::error!(
        target: "quilltap::db",
        collection = "files",
        userId = %user_id,
        filename = filename.map(tracing::field::display),
        error = %error,
        strictFailures = strict,
        "Error creating file"
    );
}

/// `folders.create`'s wrap (`folders.repository.ts:34-54`): ERROR `Error
/// creating folder {collection, userId, path, error, strictFailures?}`.
pub fn log_folder_create_wrap_failure(user_id: &str, path: Option<&str>, error: &DbError) {
    let (error, strict) = create_wrap_values(error);
    tracing::error!(
        target: "quilltap::db",
        collection = "folders",
        userId = %user_id,
        path = path.map(tracing::field::display),
        error = %error,
        strictFailures = strict,
        "Error creating folder"
    );
}

/// `tags.create`'s wrap (`tags.repository.ts:60-89`): ERROR `Error creating tag
/// {collection, userId, name, error, strictFailures?}`. (A payload with no
/// `name` throws a `TypeError` inside the wrap BEFORE `_create` — v4 lowercases
/// `data.nameLower || data.name` first — so that refusal logs this line ALONE.)
pub fn log_tag_create_wrap_failure(user_id: &str, name: Option<&str>, error: &DbError) {
    let (error, strict) = create_wrap_values(error);
    tracing::error!(
        target: "quilltap::db",
        collection = "tags",
        userId = %user_id,
        name = name.map(tracing::field::display),
        error = %error,
        strictFailures = strict,
        "Error creating tag"
    );
}

/// `roleplayTemplates.create`'s wrap (`roleplay-templates.repository.ts:285-305`):
/// ERROR `Error creating roleplay template {collection, userId, name, error,
/// strictFailures?}`.
pub fn log_roleplay_template_create_wrap_failure(
    user_id: &str,
    name: Option<&str>,
    error: &DbError,
) {
    let (error, strict) = create_wrap_values(error);
    tracing::error!(
        target: "quilltap::db",
        collection = "roleplay_templates",
        userId = %user_id,
        name = name.map(tracing::field::display),
        error = %error,
        strictFailures = strict,
        "Error creating roleplay template"
    );
}

/// `promptTemplates.create`'s wrap (`prompt-templates.repository.ts:244-264`):
/// ERROR `Error creating prompt template {collection, userId, name, error,
/// strictFailures?}`.
pub fn log_prompt_template_create_wrap_failure(user_id: &str, name: Option<&str>, error: &DbError) {
    let (error, strict) = create_wrap_values(error);
    tracing::error!(
        target: "quilltap::db",
        collection = "prompt_templates",
        userId = %user_id,
        name = name.map(tracing::field::display),
        error = %error,
        strictFailures = strict,
        "Error creating prompt template"
    );
}

/// `chats.addMessage` (`chats-messages.ops.ts:387-446`): a STANDALONE 3-argument
/// `safeQuery` — no repository wrapper, so NO `collection` — whose
/// `ChatEventSchema.parse` throws with no validation line of its own: ERROR
/// `Failed to add message to chat {chatId, error, strictFailures?}`, the one
/// line a refused message logs.
pub fn log_chat_message_add_create_wrap_failure(chat_id: &str, error: &DbError) {
    let (error, strict) = create_wrap_values(error);
    tracing::error!(
        target: "quilltap::db",
        chatId = %chat_id,
        error = %error,
        strictFailures = strict,
        "Failed to add message to chat"
    );
}
// === end P4.163 (the create wraps) ===

/// v4 `_update`'s rethrow line: ERROR `Error updating entity {collection, id,
/// error, strictFailures?}`. Logs only — the caller keeps propagating `error`.
pub fn log_update_failure(collection: &str, id: &str, error: &DbError) {
    tracing::error!(
        target: "quilltap::db",
        collection = collection,
        id = %id,
        error = %error_text(error),
        strictFailures = strict_repository_failures_active().then_some(true),
        "Error updating entity"
    );
}

/// v4 `_delete`'s rethrow line: ERROR `Error deleting entity {collection, id,
/// error, strictFailures?}`. Logs only — the caller keeps propagating `error`.
pub fn log_delete_failure(collection: &str, id: &str, error: &DbError) {
    tracing::error!(
        target: "quilltap::db",
        collection = collection,
        id = %id,
        error = %error_text(error),
        strictFailures = strict_repository_failures_active().then_some(true),
        "Error deleting entity"
    );
}

/// v4 `chatInforms.deletePendingByBatch` as its callers see it
/// (`chat-informs.repository.ts:271-291`): a 4-argument FALLBACK `safeQuery`
/// over a loop of pass-through `_delete`s, so `delete()`'s `Err` (already
/// logged by [`log_delete_failure`] — the FIRST failed row ends the loop) logs
/// ERROR `Error deleting pending informs by batch {collection, batchId, error}`
/// and answers `Ok(0)`. Inside [`with_strict_repository_failures`] the line
/// gains `strictFailures=true` and the `Err` propagates.
pub fn pending_informs_by_batch_deleted_or_zero(
    batch_id: &str,
    delete: impl FnOnce() -> Result<usize, DbError>,
) -> Result<usize, DbError> {
    delete().or_else(|error| {
        let strict = strict_repository_failures_active();
        tracing::error!(
            target: "quilltap::db",
            collection = "chat_informs",
            batchId = %batch_id,
            error = %error_text(&error),
            strictFailures = strict.then_some(true),
            "Error deleting pending informs by batch"
        );
        if strict {
            Err(error)
        } else {
            Ok(0)
        }
    })
}

/// v4 `chatInforms.deletePendingForParticipant` as its callers see it
/// (`chat-informs.repository.ts:294-313`): the same shape as
/// [`pending_informs_by_batch_deleted_or_zero`] — ERROR `Error deleting
/// pending informs for participant {collection, chatId, participantId, error}`
/// → `Ok(0)`, strict-aware. With it, v4's route-level `Could not drop pending
/// informs for removed seat` WARN (`participants.ts:625-638`) is unreachable on
/// a database failure: the DEBUG `Pending informs dropped with removed seat`
/// fires with `droppedInforms: 0` instead.
pub fn pending_informs_for_participant_deleted_or_zero(
    chat_id: &str,
    participant_id: &str,
    delete: impl FnOnce() -> Result<usize, DbError>,
) -> Result<usize, DbError> {
    delete().or_else(|error| {
        let strict = strict_repository_failures_active();
        tracing::error!(
            target: "quilltap::db",
            collection = "chat_informs",
            chatId = %chat_id,
            participantId = %participant_id,
            error = %error_text(&error),
            strictFailures = strict.then_some(true),
            "Error deleting pending informs for participant"
        );
        if strict {
            Err(error)
        } else {
            Ok(0)
        }
    })
}
// === end P4.149 (rethrow lines + the inform wraps) ===

// === P4.156 (R-G) — the memories repository's own RETHROW wraps
// (`memories.repository.ts:418-522`), each a 3-argument `safeQuery` ABOVE the
// base `_create` / `_update` / `_delete` (whose lines log first, through
// [`log_create_failure`] / [`log_update_failure`] / [`log_delete_failure`]):
// ERROR, `{collection: memories, …context, error, strictFailures?}`, then the
// error propagates. Five hand copies in `db/memories.rs` folded here, bytes
// unchanged (P4.149 measured them through `fold_episode_tier3`). Log only. ===

/// `memories.create`'s wrap: `Error creating memory {collection, characterId}`.
pub fn log_memory_create_failure(character_id: &str, error: &DbError) {
    tracing::error!(
        target: "quilltap::db",
        collection = "memories",
        characterId = %character_id,
        error = %error_text(error),
        strictFailures = strict_repository_failures_active().then_some(true),
        "Error creating memory"
    );
}

/// `memories.update`'s wrap: `Error updating memory {collection, memoryId}`.
pub fn log_memory_update_failure(memory_id: &str, error: &DbError) {
    tracing::error!(
        target: "quilltap::db",
        collection = "memories",
        memoryId = %memory_id,
        error = %error_text(error),
        strictFailures = strict_repository_failures_active().then_some(true),
        "Error updating memory"
    );
}

/// `memories.delete`'s wrap: `Error deleting memory {collection, memoryId}`.
pub fn log_memory_delete_failure(memory_id: &str, error: &DbError) {
    tracing::error!(
        target: "quilltap::db",
        collection = "memories",
        memoryId = %memory_id,
        error = %error_text(error),
        strictFailures = strict_repository_failures_active().then_some(true),
        "Error deleting memory"
    );
}

/// `memories.updateForCharacter`'s outer wrap: `Error updating memory for
/// character {collection, characterId, memoryId}` (beneath the two update lines).
pub fn log_memory_update_for_character_failure(
    character_id: &str,
    memory_id: &str,
    error: &DbError,
) {
    tracing::error!(
        target: "quilltap::db",
        collection = "memories",
        characterId = %character_id,
        memoryId = %memory_id,
        error = %error_text(error),
        strictFailures = strict_repository_failures_active().then_some(true),
        "Error updating memory for character"
    );
}

/// `memories.deleteForCharacter`'s outer wrap: `Error deleting memory for
/// character {collection, characterId, memoryId}`.
pub fn log_memory_delete_for_character_failure(
    character_id: &str,
    memory_id: &str,
    error: &DbError,
) {
    tracing::error!(
        target: "quilltap::db",
        collection = "memories",
        characterId = %character_id,
        memoryId = %memory_id,
        error = %error_text(error),
        strictFailures = strict_repository_failures_active().then_some(true),
        "Error deleting memory for character"
    );
}
// === end P4.156 (R-G) ===

// === P4.156 — the chat-informs repository's remaining 4-argument FALLBACK
// wraps (`chat-informs.repository.ts:87-183, 238-268`), measured one by one at
// `94fbb1ae3` through a `Logger.prototype` spy on the REAL repository
// (`chat_informs_tier2_equivalence`). Each read method is `safeQuery(…, [])`
// around the base `findByFilter` — itself a fallback — so OUTSIDE the strict
// scope the INNER line answers first ([`find_by_filter_strict_aware`]) and
// these five outer lines are unreachable; INSIDE it the inner rethrows and the
// outer line follows, both `strictFailures=true`, and the `Err` propagates.
// `markConsumed`'s wrap is reachable always: its body's `_update` RETHROWS.
// Every context in v4's key order after the injected `collection`. ===

/// The five outer read wraps share one shape: log (strict-aware) and propagate
/// inside the scope, `Ok([])` outside it. `log` emits the method's own line.
fn inform_read_wrap<T>(
    read: impl FnOnce() -> Result<Vec<T>, DbError>,
    log: impl FnOnce(&DbError, Option<bool>),
) -> Result<Vec<T>, DbError> {
    read().or_else(|error| {
        let strict = strict_repository_failures_active();
        log(&error, strict.then_some(true));
        if strict {
            Err(error)
        } else {
            Ok(Vec::new())
        }
    })
}

/// v4 `chatInforms.findPendingForParticipant` (`:87-100`): ERROR `Error finding
/// pending informs for participant {collection, chatId, participantId, error}`.
pub fn pending_informs_for_participant_or_empty<T>(
    chat_id: &str,
    participant_id: &str,
    read: impl FnOnce() -> Result<Vec<T>, DbError>,
) -> Result<Vec<T>, DbError> {
    inform_read_wrap(read, |error, strict| {
        tracing::error!(
            target: "quilltap::db",
            collection = "chat_informs",
            chatId = %chat_id,
            participantId = %participant_id,
            error = %error_text(error),
            strictFailures = strict,
            "Error finding pending informs for participant"
        );
    })
}

/// v4 `chatInforms.findConsumedByMessages` (`:107-129`): ERROR `Error finding
/// informs consumed by messages {collection, chatId, participantId,
/// messageCount, error}` — `messageCount` is `messageIds.length`.
pub fn informs_consumed_by_messages_or_empty<T>(
    chat_id: &str,
    participant_id: &str,
    message_count: usize,
    read: impl FnOnce() -> Result<Vec<T>, DbError>,
) -> Result<Vec<T>, DbError> {
    inform_read_wrap(read, |error, strict| {
        tracing::error!(
            target: "quilltap::db",
            collection = "chat_informs",
            chatId = %chat_id,
            participantId = %participant_id,
            messageCount = message_count,
            error = %error_text(error),
            strictFailures = strict,
            "Error finding informs consumed by messages"
        );
    })
}

/// v4 `chatInforms.findPendingBatches` (`:135-163`): ERROR `Error finding
/// pending inform batches {collection, chatId, error}`.
pub fn pending_inform_batches_or_empty<T>(
    chat_id: &str,
    read: impl FnOnce() -> Result<Vec<T>, DbError>,
) -> Result<Vec<T>, DbError> {
    inform_read_wrap(read, |error, strict| {
        tracing::error!(
            target: "quilltap::db",
            collection = "chat_informs",
            chatId = %chat_id,
            error = %error_text(error),
            strictFailures = strict,
            "Error finding pending inform batches"
        );
    })
}

/// v4 `chatInforms.findByChatId` (`:166-173`): ERROR `Error finding informs by
/// chat ID {collection, chatId, error}`.
pub fn informs_by_chat_id_or_empty<T>(
    chat_id: &str,
    read: impl FnOnce() -> Result<Vec<T>, DbError>,
) -> Result<Vec<T>, DbError> {
    inform_read_wrap(read, |error, strict| {
        tracing::error!(
            target: "quilltap::db",
            collection = "chat_informs",
            chatId = %chat_id,
            error = %error_text(error),
            strictFailures = strict,
            "Error finding informs by chat ID"
        );
    })
}

/// v4 `chatInforms.findByBatchId` (`:176-183`): ERROR `Error finding informs by
/// batch ID {collection, batchId, error}`.
pub fn informs_by_batch_id_or_empty<T>(
    batch_id: &str,
    read: impl FnOnce() -> Result<Vec<T>, DbError>,
) -> Result<Vec<T>, DbError> {
    inform_read_wrap(read, |error, strict| {
        tracing::error!(
            target: "quilltap::db",
            collection = "chat_informs",
            batchId = %batch_id,
            error = %error_text(error),
            strictFailures = strict,
            "Error finding informs by batch ID"
        );
    })
}

/// v4 `chatInforms.markConsumed` (`:238-268`): a 4-argument FALLBACK around a
/// loop of rethrowing `_update`s, so the FIRST failed row ends the loop — its
/// base [`log_update_failure`] line first — and this one follows: ERROR `Error
/// marking informs consumed {collection, ids, messageId, error}` → `Ok(0)`;
/// strict-aware. `ids` is an ARRAY, so it rides the file layer's `…Json`
/// convention (`idsJson`, the compact JSON v4's line carries).
pub fn informs_marked_consumed_or_zero(
    ids: &[String],
    message_id: &str,
    write: impl FnOnce() -> Result<usize, DbError>,
) -> Result<usize, DbError> {
    write().or_else(|error| {
        let strict = strict_repository_failures_active();
        let ids_json = serde_json::to_string(ids).unwrap_or_else(|_| "[]".to_string());
        tracing::error!(
            target: "quilltap::db",
            collection = "chat_informs",
            idsJson = ids_json.as_str(),
            messageId = %message_id,
            error = %error_text(&error),
            strictFailures = strict.then_some(true),
            "Error marking informs consumed"
        );
        if strict {
            Err(error)
        } else {
            Ok(0)
        }
    })
}

/// v4 `chats.deleteMessagesByIds` as its callers see it (`chats-messages.ops.ts:
/// 633-686`): the STANDALONE `safeQuery(…, 'Failed to delete messages from
/// chat', { chatId, count: messageIds.length }, 0)` — no repository wrapper, so
/// NO injected `collection` — logs ERROR `{chatId, count, error}` and answers
/// `0` (measured at `94fbb1ae3`, `chat_informs_tier2_equivalence`). Which is why
/// the Inform cancel's own WARN `Could not delete inform record message`
/// (`inform.ts:228-237`) is unreachable on a database failure (dogfood #145).
/// The CALLER passes the whole write's result (a writer-thread failure lands
/// here once, as v4's `getCollection()` inside the same `safeQuery`).
pub fn messages_deleted_or_zero(
    chat_id: &str,
    count: usize,
    delete: impl FnOnce() -> Result<i64, DbError>,
) -> i64 {
    delete().unwrap_or_else(|error| {
        tracing::error!(
            target: "quilltap::db",
            chatId = %chat_id,
            count = count,
            error = %error_text(&error),
            "Failed to delete messages from chat"
        );
        0
    })
}
// === end P4.156 (the chat-informs wraps) ===

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

    /// P4.149 (item 4) — the strict-aware joined-links sibling: `Ok([])` + the
    /// line outside the scope, the line + `strictFailures=true` + the `Err`
    /// inside it, and v4's QUIET unavailable-partition arm (a DEBUG, `Ok([])`,
    /// strict or not); the plain twin's bytes unchanged.
    #[test]
    fn the_strict_aware_joined_links_sibling_honours_the_scope() {
        let unavailable =
            || DbError::PartitionUnavailable(crate::write_partition::WriteDbTarget::MountIndex);
        let (got, lines) = crate::test_support::captured_with(|| {
            (
                joined_file_links_strict_aware::<i32>("WHERE l.id = ?", || Err(posed())).unwrap(),
                with_strict_repository_failures(|| {
                    joined_file_links_strict_aware::<i32>("WHERE l.id = ?", || Err(posed()))
                })
                .is_err(),
                with_strict_repository_failures(|| {
                    joined_file_links_strict_aware::<i32>("WHERE l.id = ?", || Err(unavailable()))
                })
                .unwrap(),
                joined_file_links_strict_aware("WHERE l.id = ?", || Ok(vec![7])).unwrap(),
                joined_file_links_or_empty::<i32>("WHERE l.id = ?", || Err(posed())),
            )
        });
        assert_eq!(got, (vec![], true, vec![], vec![7], vec![]));
        assert_eq!(
            lines,
            vec![
                "ERROR quilltap::db Error querying joined file links collection=doc_mount_file_links whereClause=WHERE l.id = ? error=posed".to_string(),
                "ERROR quilltap::db Error querying joined file links collection=doc_mount_file_links whereClause=WHERE l.id = ? error=posed strictFailures=true".to_string(),
                // P4.163 (R-C): the partition-unavailable arm renders v4's
                // DEGRADED guard sentence (`mount-index-guard.ts:20`), never
                // v5's `partition not available: …` Display.
                "DEBUG quilltap::db Dedicated database unavailable; answering with the fallback collection=doc_mount_file_links dbTarget=mountIndex error=Mount index database is in degraded mode".to_string(),
                "ERROR quilltap::db Error querying joined file links collection=doc_mount_file_links whereClause=WHERE l.id = ? error=posed".to_string(),
            ]
        );
    }

    /// P4.149 — the three base RETHROW lines in contract C2's measured bytes
    /// (`{collection, [id], error}`, `strictFailures=true` LAST inside the
    /// strict scope), rendered bare from a real SQLite failure.
    #[test]
    fn the_three_rethrow_lines_carry_c2s_measured_fields() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        let sqlite = || {
            DbError::from(
                conn.execute_batch("SELECT 1 FROM no_such_table")
                    .unwrap_err(),
            )
        };
        let ((), lines) = crate::test_support::captured_with(|| {
            log_create_failure("chat_informs", &sqlite());
            log_update_failure("chat_informs", "i-1", &sqlite());
            log_delete_failure("chat_informs", "i-1", &sqlite());
            with_strict_repository_failures(|| {
                log_create_failure("chats", &posed());
                log_update_failure("memories", "m-1", &posed());
                log_delete_failure("chat_informs", "i-2", &posed());
            });
        });
        assert_eq!(
            lines,
            vec![
                "ERROR quilltap::db Error creating entity collection=chat_informs error=no such table: no_such_table".to_string(),
                "ERROR quilltap::db Error updating entity collection=chat_informs id=i-1 error=no such table: no_such_table".to_string(),
                "ERROR quilltap::db Error deleting entity collection=chat_informs id=i-1 error=no such table: no_such_table".to_string(),
                "ERROR quilltap::db Error creating entity collection=chats error=posed strictFailures=true".to_string(),
                "ERROR quilltap::db Error updating entity collection=memories id=m-1 error=posed strictFailures=true".to_string(),
                "ERROR quilltap::db Error deleting entity collection=chat_informs id=i-2 error=posed strictFailures=true".to_string(),
            ]
        );
    }

    /// P4.149 — the two chat-informs wraps: v4's line + `Ok(0)` outside the
    /// strict scope, the line + the `Err` inside it, silence on success.
    #[test]
    fn the_two_inform_wraps_answer_zero_or_propagate_under_strict() {
        let (got, lines) = crate::test_support::captured_with(|| {
            (
                pending_informs_by_batch_deleted_or_zero("b-1", || Err(posed())).unwrap(),
                pending_informs_for_participant_deleted_or_zero("c-1", "p-1", || Err(posed()))
                    .unwrap(),
                pending_informs_by_batch_deleted_or_zero("b-1", || Ok(2)).unwrap(),
                pending_informs_for_participant_deleted_or_zero("c-1", "p-1", || Ok(3)).unwrap(),
            )
        });
        assert_eq!(got, (0, 0, 2, 3));
        assert_eq!(
            lines,
            vec![
                "ERROR quilltap::db Error deleting pending informs by batch collection=chat_informs batchId=b-1 error=posed".to_string(),
                "ERROR quilltap::db Error deleting pending informs for participant collection=chat_informs chatId=c-1 participantId=p-1 error=posed".to_string(),
            ]
        );
        let (got, lines) = crate::test_support::captured_with(|| {
            with_strict_repository_failures(|| {
                (
                    pending_informs_by_batch_deleted_or_zero("b-1", || Err(posed())),
                    pending_informs_for_participant_deleted_or_zero("c-1", "p-1", || Err(posed())),
                )
            })
        });
        assert!(matches!(got.0, Err(DbError::Internal(ref m)) if m == "posed"));
        assert!(matches!(got.1, Err(DbError::Internal(ref m)) if m == "posed"));
        assert_eq!(
            lines,
            vec![
                "ERROR quilltap::db Error deleting pending informs by batch collection=chat_informs batchId=b-1 error=posed strictFailures=true".to_string(),
                "ERROR quilltap::db Error deleting pending informs for participant collection=chat_informs chatId=c-1 participantId=p-1 error=posed strictFailures=true".to_string(),
            ]
        );
    }

    /// P4.156 — the strict-aware filter sibling: `Ok([])` + the plain bytes
    /// outside the scope, `strictFailures=true` + the `Err` inside it, silence
    /// on success; the plain twin's bytes unchanged through the shared emitter.
    #[test]
    fn the_strict_aware_filter_sibling_honours_the_scope() {
        let (got, lines) = crate::test_support::captured_with(|| {
            (
                find_by_filter_strict_aware::<i32>("chat_informs", || Err(posed())).unwrap(),
                with_strict_repository_failures(|| {
                    find_by_filter_strict_aware::<i32>("chat_informs", || Err(posed()))
                })
                .is_err(),
                find_by_filter_strict_aware("chat_informs", || Ok(vec![1])).unwrap(),
                find_by_filter_or_empty::<i32>("chat_informs", || Err(posed())),
            )
        });
        assert_eq!(got, (vec![], true, vec![1], vec![]));
        assert_eq!(
            lines,
            vec![
                "ERROR quilltap::db Error finding entities by filter collection=chat_informs error=posed".to_string(),
                "ERROR quilltap::db Error finding entities by filter collection=chat_informs error=posed strictFailures=true".to_string(),
                "ERROR quilltap::db Error finding entities by filter collection=chat_informs error=posed".to_string(),
            ]
        );
    }

    /// P4.156 — the five chat-informs outer read wraps (their bytes in v4's key
    /// order, `strictFailures=true` LAST and the `Err` inside the scope,
    /// `Ok([])` outside it) and the consume wrap (`idsJson`, `Ok(0)`).
    #[test]
    fn the_chat_informs_wraps_log_v4s_lines() {
        let (got, lines) = crate::test_support::captured_with(|| {
            with_strict_repository_failures(|| {
                [
                    pending_informs_for_participant_or_empty::<i32>("c-1", "p-1", || Err(posed()))
                        .is_err(),
                    informs_consumed_by_messages_or_empty::<i32>("c-1", "p-1", 2, || Err(posed()))
                        .is_err(),
                    pending_inform_batches_or_empty::<i32>("c-1", || Err(posed())).is_err(),
                    informs_by_chat_id_or_empty::<i32>("c-1", || Err(posed())).is_err(),
                    informs_by_batch_id_or_empty::<i32>("b-1", || Err(posed())).is_err(),
                ]
            })
        });
        assert_eq!(got, [true; 5]);
        assert_eq!(
            lines,
            vec![
                "ERROR quilltap::db Error finding pending informs for participant collection=chat_informs chatId=c-1 participantId=p-1 error=posed strictFailures=true".to_string(),
                "ERROR quilltap::db Error finding informs consumed by messages collection=chat_informs chatId=c-1 participantId=p-1 messageCount=2 error=posed strictFailures=true".to_string(),
                "ERROR quilltap::db Error finding pending inform batches collection=chat_informs chatId=c-1 error=posed strictFailures=true".to_string(),
                "ERROR quilltap::db Error finding informs by chat ID collection=chat_informs chatId=c-1 error=posed strictFailures=true".to_string(),
                "ERROR quilltap::db Error finding informs by batch ID collection=chat_informs batchId=b-1 error=posed strictFailures=true".to_string(),
            ]
        );
        let ids = vec!["i-1".to_string(), "i-2".to_string()];
        let (got, lines) = crate::test_support::captured_with(|| {
            (
                informs_marked_consumed_or_zero(&ids, "m-1", || Err(posed())).unwrap(),
                with_strict_repository_failures(|| {
                    informs_marked_consumed_or_zero(&ids, "m-1", || Err(posed()))
                })
                .is_err(),
                pending_inform_batches_or_empty::<i32>("c-1", || Err(posed())).unwrap(),
            )
        });
        assert_eq!(got, (0, true, vec![]));
        assert_eq!(
            lines,
            vec![
                r#"ERROR quilltap::db Error marking informs consumed collection=chat_informs idsJson=["i-1","i-2"] messageId=m-1 error=posed"#.to_string(),
                r#"ERROR quilltap::db Error marking informs consumed collection=chat_informs idsJson=["i-1","i-2"] messageId=m-1 error=posed strictFailures=true"#.to_string(),
                "ERROR quilltap::db Error finding pending inform batches collection=chat_informs chatId=c-1 error=posed".to_string(),
            ]
        );
    }

    /// P4.156 (R-G) — the five memories wraps: v4's bytes (`collection`
    /// first, the context in v4's order, the bare `error`), `strictFailures`
    /// LAST inside the scope only.
    #[test]
    fn the_five_memory_wraps_log_v4s_lines() {
        let ((), lines) = crate::test_support::captured_with(|| {
            log_memory_create_failure("ch-1", &posed());
            log_memory_update_failure("m-1", &posed());
            log_memory_delete_failure("m-1", &posed());
            log_memory_update_for_character_failure("ch-1", "m-1", &posed());
            with_strict_repository_failures(|| {
                log_memory_delete_for_character_failure("ch-1", "m-1", &posed());
            });
        });
        assert_eq!(
            lines,
            vec![
                "ERROR quilltap::db Error creating memory collection=memories characterId=ch-1 error=posed".to_string(),
                "ERROR quilltap::db Error updating memory collection=memories memoryId=m-1 error=posed".to_string(),
                "ERROR quilltap::db Error deleting memory collection=memories memoryId=m-1 error=posed".to_string(),
                "ERROR quilltap::db Error updating memory for character collection=memories characterId=ch-1 memoryId=m-1 error=posed".to_string(),
                "ERROR quilltap::db Error deleting memory for character collection=memories characterId=ch-1 memoryId=m-1 error=posed strictFailures=true".to_string(),
            ]
        );
    }

    #[test]
    fn the_chat_informs_wraps_are_silent_on_success() {
        let ids = vec!["i-1".to_string()];
        let (got, lines) = crate::test_support::captured_with(|| {
            (
                pending_informs_for_participant_or_empty("c", "p", || Ok(vec![1])).unwrap(),
                informs_consumed_by_messages_or_empty("c", "p", 1, || Ok(vec![2])).unwrap(),
                pending_inform_batches_or_empty("c", || Ok(vec![3])).unwrap(),
                informs_by_chat_id_or_empty("c", || Ok(vec![4])).unwrap(),
                informs_by_batch_id_or_empty("b", || Ok(vec![5])).unwrap(),
                informs_marked_consumed_or_zero(&ids, "m", || Ok(1)).unwrap(),
            )
        });
        assert_eq!(got, (vec![1], vec![2], vec![3], vec![4], vec![5], 1));
        assert!(lines.is_empty(), "{lines:?}");
    }

    /// P4.163 (C1 item 1) — `log_partition_unavailable` for BOTH dedicated
    /// targets and BOTH `error` renderings: the degraded guard sentence for a
    /// `PartitionUnavailable`, the bare `error_text` for anything else; a DEBUG
    /// with no `strictFailures` even inside the strict scope (v4's `withRawDb`
    /// arm never honours it); and the mount-index wrapper's bytes are the home's.
    #[test]
    fn the_partition_unavailable_home_renders_v4s_debug_for_both_targets() {
        use crate::write_partition::WriteDbTarget;
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        let sqlite = DbError::from(conn.execute_batch("SELECT 1 FROM gone").unwrap_err());
        let ((), lines) = crate::test_support::captured_with(|| {
            log_partition_unavailable(
                Partition::MountIndex,
                "doc_mount_points",
                &DbError::PartitionUnavailable(WriteDbTarget::MountIndex),
            );
            log_partition_unavailable(
                Partition::LlmLogs,
                "llm_logs",
                &DbError::PartitionUnavailable(WriteDbTarget::LlmLogs),
            );
            log_partition_unavailable(Partition::LlmLogs, "llm_logs", &sqlite);
            with_strict_repository_failures(|| {
                log_mount_index_unavailable(
                    "doc_mount_documents",
                    &DbError::PartitionUnavailable(WriteDbTarget::MountIndex),
                )
            });
            log_mount_index_unavailable("doc_mount_documents", &posed());
        });
        assert_eq!(
            lines,
            vec![
                "DEBUG quilltap::db Dedicated database unavailable; answering with the fallback collection=doc_mount_points dbTarget=mountIndex error=Mount index database is in degraded mode",
                "DEBUG quilltap::db Dedicated database unavailable; answering with the fallback collection=llm_logs dbTarget=llmLogs error=LLM logs database is in degraded mode",
                "DEBUG quilltap::db Dedicated database unavailable; answering with the fallback collection=llm_logs dbTarget=llmLogs error=no such table: gone",
                "DEBUG quilltap::db Dedicated database unavailable; answering with the fallback collection=doc_mount_documents dbTarget=mountIndex error=Mount index database is in degraded mode",
                "DEBUG quilltap::db Dedicated database unavailable; answering with the fallback collection=doc_mount_documents dbTarget=mountIndex error=posed",
            ]
        );
    }

    /// P4.163 (C1 item 2) — v4's `verifyStructure` unavailable string for a
    /// degraded sibling, exactly (`dedicated-db.repository.ts:179-182` over the
    /// guards' sentences), through `table_shape::unavailable`; it logs nothing.
    #[test]
    fn the_structural_unavailable_home_returns_v4s_string_and_logs_nothing() {
        let (got, lines) = crate::test_support::captured_with(|| {
            (
                log_partition_structural_unavailable(Partition::MountIndex, "docMountPoints"),
                log_partition_structural_unavailable(Partition::LlmLogs, "llmLogs"),
            )
        });
        assert_eq!(
            got.0,
            "mount index database unavailable: Mount index database is in degraded mode"
        );
        assert_eq!(
            got.1,
            "LLM logs database unavailable: LLM logs database is in degraded mode"
        );
        assert!(lines.is_empty(), "{lines:?}");
    }

    /// P4.163 (C1 item 3) — the create wraps in-crate: the characters PAIR
    /// (validate's line carries NO `strictFailures`), an absent context field
    /// OMITTED, `strictFailures=true` only inside the scope, and `addMessage`'s
    /// collection-less line. The whole-corpus proof against v4's REAL
    /// repositories is `create_wrap_lines_equivalence`.
    #[test]
    fn the_create_wrap_homes_render_v4s_lines() {
        let ((), lines) = crate::test_support::captured_with(|| {
            with_strict_repository_failures(|| {
                log_character_create_failure(&posed());
                log_character_create_wrap_failure("u-1", Some("Abigail"), &posed());
            });
            log_connection_profile_create_wrap_failure("u-1", None, Some("OPENAI"), &posed());
            log_file_create_wrap_failure("u-1", Some("notes.md"), &posed());
            log_chat_message_add_create_wrap_failure("c-1", &posed());
        });
        assert_eq!(
            lines,
            vec![
                "ERROR quilltap::db Data validation failed collection=characters error=posed",
                "ERROR quilltap::db Error creating character entity collection=characters error=posed strictFailures=true",
                "ERROR quilltap::db Error creating character collection=characters userId=u-1 name=Abigail error=posed strictFailures=true",
                "ERROR quilltap::db Error creating connection profile collection=connection_profiles userId=u-1 provider=OPENAI error=posed",
                "ERROR quilltap::db Error creating file collection=files userId=u-1 filename=notes.md error=posed",
                "ERROR quilltap::db Failed to add message to chat chatId=c-1 error=posed",
            ]
        );
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
