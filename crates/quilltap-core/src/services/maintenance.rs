//! Stale-chat asset collapse (v4
//! `lib/background-jobs/maintenance/collapse-stale-chat-assets.ts`).
//!
//! When a chat has gone quiet for `STALE_CHAT_RETENTION_DAYS`, the generated
//! story-backgrounds and wardrobe avatars it accumulated are mostly dead weight —
//! only the *currently-referenced* ones (`chat.storyBackgroundImageId` and each
//! `chat.characterAvatars[].imageId`) still matter. This sweep deletes every other
//! GENERATED/IMAGE `files` row linked to a stale chat, releasing the orphaned
//! bytes. It is gated on CHAT staleness (never per-asset age): an active chat is
//! never touched no matter how many backgrounds it piled up.
//!
//! ## Why it runs on the parent (v4) / here directly (v5)
//! In v4, deletion bottoms out in a write transaction on the raw mount-index DB,
//! impossible in the forked readonly child — so v4 invokes this inline on the
//! parent (the sole writer). In v5 the single-writer [`Db`] makes that a non-issue:
//! reads go through the read pool, the `files.delete` goes through [`Db::write`].
//! There is no `MAINTENANCE` job type; the host driver calls this on its daily
//! cadence (see [`super::job_scheduler`]).
//!
//! ## The `deleteFileCompletely` storage-bytes seam (documented deferral)
//! v4 deletes through `deleteFileCompletely`, which first deletes the file's bytes
//! from `fileStorageManager` (on-disk / mount-blob storage) and then the `files`
//! metadata row. On v5's DB-backed differential path there are no on-disk bytes;
//! the port ports the metadata-delete half (`files.delete`) and treats the
//! storage-bytes delete as a **host FsSeam** (a no-op here — the bytes live in
//! host-owned storage the Phase-4 host manages). The `bytesReleasedEstimate` (v4's
//! own best-effort upper bound from the recorded sizes) is unaffected.

use serde_json::Value;

use crate::clock::iso_to_ms;
use crate::db::doc_mount_file_links::{is_photos_relative_path, DocMountFileLinksRepository};
use crate::db::doc_mount_files::DocMountFilesRepository;
use crate::db::doc_mount_points::DocMountPointsRepository;
use crate::db::files::{FileSweepRow, FilesRepository};
use crate::db::runtime::Db;
use crate::db::{chats_messages_read, chats_read, DbError};

use super::queue_service::{resolve_stale_chat_days, retention_cutoff_iso};

/// The summary of one collapse pass (v4 `StaleChatCollapseSummary`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StaleChatCollapseSummary {
    /// Total chats examined.
    pub chats_scanned: usize,
    /// Chats found stale (eligible for collapse).
    pub stale_chats: usize,
    /// Stale chats from which at least one asset was deleted.
    pub chats_collapsed: usize,
    /// Superseded generated `files` rows deleted.
    pub files_deleted: usize,
    /// Best-effort upper bound on bytes reclaimed (sum of deleted files' recorded
    /// sizes). v4's own estimate — the actual reclaim is lower when bytes are
    /// deduped by sha256 and survive the GC; this is not a guarantee.
    pub bytes_released_estimate: i64,
}

/// The keep-set for one chat: the ids it currently references + their resolved
/// content hashes, so a current asset is protected whether the field holds a
/// `files.id` or a `doc_mount_file_links.id`.
struct KeepSet {
    keep_ids: std::collections::HashSet<String>,
    keep_shas: std::collections::HashSet<String>,
}

/// v4 `resolveCharacterAvatar(id)` reduced to its `.sha256` — the only field the
/// sweep consumes. Tries the vault-link path (post-Phase-3 shape:
/// `doc_mount_file_links.findByIdWithContent`), then the legacy `files.findById`.
/// `Ok(None)` = the id resolves to nothing (safe — no live asset to protect);
/// `Err` propagates (v4's per-chat try/catch aborts THIS chat's collapse: an
/// incomplete keep-set could delete a current asset). An empty sha string is
/// normalized to `None` (v4 `nullIfEmpty`).
fn resolve_avatar_sha256(db: &Db, id: &str) -> Result<Option<String>, DbError> {
    let id_owned = id.to_string();
    // Path 1: vault link (mount-index).
    let link_sha = db.read_mount_index(|conn| {
        let repo = DocMountFileLinksRepository::new(conn);
        Ok(repo.find_link_row_by_id(&id_owned)?.map(|l| l.sha256))
    })?;
    if let Some(sha) = link_sha {
        return Ok(if sha.is_empty() { None } else { Some(sha) });
    }
    // Path 2: legacy files-table id (main).
    let id_owned = id.to_string();
    let file_sha = db.read_main(|conn| {
        let repo = FilesRepository::new(conn);
        Ok(repo.find_sweep_row_by_id(&id_owned)?.map(|f| f.sha256))
    })?;
    Ok(file_sha.filter(|s| !s.is_empty()))
}

/// Build the keep-set for a chat (v4 `buildKeepSet`). Errors from avatar
/// resolution propagate (the caller skips the whole chat — "when unsure, skip");
/// a clean `None` is safe.
fn build_keep_set(db: &Db, chat: &Value) -> Result<KeepSet, DbError> {
    let mut keep_ids = std::collections::HashSet::new();
    let mut keep_shas = std::collections::HashSet::new();

    let mut candidate_ids: Vec<String> = Vec::new();
    if let Some(id) = chat.get("storyBackgroundImageId").and_then(Value::as_str) {
        if !id.is_empty() {
            candidate_ids.push(id.to_string());
        }
    }
    // `characterAvatars` is an object map keyed by characterId → { imageId, ... }.
    if let Some(map) = chat.get("characterAvatars").and_then(Value::as_object) {
        for entry in map.values() {
            if let Some(image_id) = entry.get("imageId").and_then(Value::as_str) {
                if !image_id.is_empty() {
                    candidate_ids.push(image_id.to_string());
                }
            }
        }
    }

    for id in candidate_ids {
        keep_ids.insert(id.clone());
        if let Some(sha) = resolve_avatar_sha256(db, &id)? {
            keep_shas.insert(sha);
        }
    }

    Ok(KeepSet {
        keep_ids,
        keep_shas,
    })
}

/// True when an image's bytes surface as a `photos/` album link OR a
/// `'character'`-vault link — v4 `getPhotoLinkSummaryBySha256(sha).linkers.some(l
/// => l.isPhotoAlbum || l.mountStoreType === 'character')`, reduced to the
/// predicate the sweep needs (the linkedBy/caption/tags fields are NOT consulted).
fn sha256_has_album_or_vault_link(db: &Db, sha256: &str) -> Result<bool, DbError> {
    if sha256.is_empty() {
        return Ok(false);
    }
    let sha = sha256.to_string();
    // sha256 → the content-row id → its links (mount-index).
    let links = db.read_mount_index(|conn| {
        let files = DocMountFilesRepository::new(conn);
        let Some(file_id) = files.find_by_sha256(&sha)? else {
            return Ok(Vec::new());
        };
        let link_repo = DocMountFileLinksRepository::new(conn);
        link_repo.find_by_file_id(&file_id)
    })?;
    if links.is_empty() {
        return Ok(false);
    }
    // Cache mount-point storeType lookups within this sha's link set.
    let mut store_type_cache: std::collections::HashMap<String, Option<String>> =
        std::collections::HashMap::new();
    for link in &links {
        if is_photos_relative_path(Some(&link.relative_path)) {
            return Ok(true);
        }
        let mp = link.mount_point_id.clone();
        let store_type = if let Some(v) = store_type_cache.get(&mp) {
            v.clone()
        } else {
            let looked = db.read_mount_index(|conn| {
                DocMountPointsRepository::new(conn).find_store_type_by_id(&mp)
            })?;
            store_type_cache.insert(mp, looked.clone());
            looked
        };
        if store_type.as_deref() == Some("character") {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Decide whether a candidate generated image is safe to delete (v4 `skipReason`).
/// Returns a reason string when it must be SKIPPED, or `None` when safe to reap.
fn skip_reason(
    db: &Db,
    file: &FileSweepRow,
    keep: &KeepSet,
) -> Result<Option<&'static str>, DbError> {
    if keep.keep_ids.contains(&file.id) {
        return Ok(Some("current"));
    }
    if !file.sha256.is_empty() && keep.keep_shas.contains(&file.sha256) {
        return Ok(Some("current-sha"));
    }
    if !file.sha256.is_empty() && sha256_has_album_or_vault_link(db, &file.sha256)? {
        return Ok(Some("album-or-vault-link"));
    }
    // Promoted to a character default / avatar override (possibly in another,
    // still-active chat): character-level content, out of scope.
    let id = file.id.clone();
    let as_default = db.read_main(|c| chats_read_count_default(c, &id))?;
    let id = file.id.clone();
    let as_override = db.read_main(|c| chats_read_count_override(c, &id))?;
    if as_default > 0 || as_override > 0 {
        return Ok(Some("character-reference"));
    }
    Ok(None)
}

// Thin wrappers so the read closures name concrete fns (keeps the closures simple).
fn chats_read_count_default(conn: &rusqlite::Connection, image_id: &str) -> Result<i64, DbError> {
    crate::db::characters_read::count_by_default_image_id(conn, image_id)
}
fn chats_read_count_override(conn: &rusqlite::Connection, image_id: &str) -> Result<i64, DbError> {
    crate::db::characters_read::count_by_avatar_override_image_id(conn, image_id)
}

/// True when the chat's last *played* activity is older than the cutoff (v4
/// `isStale`). "Played" = a participant/user `type:'message'` with
/// `systemSender IS NULL` (excludes Staff announcements). Falls back to
/// `chat.updatedAt` when the chat has no played messages; a null/unparseable
/// timestamp is never stale ("unknown activity — never touch").
///
/// Exported (crate-internal) as THE shared staleness gate for every stale-gated
/// maintenance sweep (asset collapse, cache collapse, chunk cold-tiering) **AND
/// for the startup render/embed reconcile**, so they can never disagree on what
/// "stale" means (v4 exports `isStale`). The reconcile MUST use this gate:
/// cold-tiering deliberately leaves stale chats with NULL `renderedMarkdown` and
/// NULL chunk embeddings, and a reconcile that can't tell "cold-tiered" from
/// "broken" re-embeds the entire cold tier on every boot just for the next sweep
/// to clear it again (v4 `a0243abd`).
///
/// Reads only `id` and `updatedAt` off `chat`, which is v4's narrowing to
/// `Pick<ChatMetadata, 'id' | 'updatedAt'>` — so a caller scanning with raw SQL
/// can synthesize the two fields instead of hydrating a full chat row.
pub(crate) fn is_stale(db: &Db, chat: &Value, cutoff_ms: i64) -> Result<bool, DbError> {
    db.read_main(|c| is_stale_conn(c, chat, cutoff_ms))
}

/// [`is_stale`] against one borrowed connection — what the boot reconcile needs,
/// since it already runs on the writer's main connection and cannot reach for
/// the read pool's `&Db` from inside a write closure.
pub(crate) fn is_stale_conn(
    conn: &rusqlite::Connection,
    chat: &Value,
    cutoff_ms: i64,
) -> Result<bool, DbError> {
    let chat_id = chat.get("id").and_then(Value::as_str).unwrap_or_default();
    let cid = chat_id.to_string();
    let last_played = chats_messages_read::get_last_played_message_at(conn, &cid)?;
    let last_activity = last_played.or_else(|| {
        chat.get("updatedAt")
            .and_then(Value::as_str)
            .map(str::to_string)
    });
    let Some(last_activity) = last_activity.filter(|s| !s.is_empty()) else {
        return Ok(false); // unknown activity — never touch
    };
    match iso_to_ms(&last_activity) {
        Some(ms) => Ok(ms < cutoff_ms),
        None => Ok(false), // NaN date → never stale
    }
}

/// Collapse a single stale chat (v4 `collapseOneChat`). Returns `(deleted,
/// bytes)`. Idempotent — a no-op once already collapsed.
async fn collapse_one_chat(db: &Db, chat: &Value) -> Result<(usize, i64), DbError> {
    let keep = build_keep_set(db, chat)?;

    let chat_id = chat.get("id").and_then(Value::as_str).unwrap_or_default();
    let cid = chat_id.to_string();
    // Generated story-backgrounds AND wardrobe avatars both land as `files` rows
    // with the chatId in linkedTo and source/category = GENERATED/IMAGE.
    let linked = db.read_main(|c| FilesRepository::new(c).find_sweep_rows_by_linked_to(&cid))?;
    let candidates: Vec<FileSweepRow> = linked
        .into_iter()
        .filter(|f| f.source == "GENERATED" && f.category == "IMAGE")
        .collect();

    let mut deleted = 0usize;
    let mut bytes = 0i64;
    for file in &candidates {
        if skip_reason(db, file, &keep)?.is_some() {
            continue;
        }
        // deleteFileCompletely: the storage-bytes delete is the host FsSeam; the
        // metadata delete goes through the writer.
        let id = file.id.clone();
        let removed = db
            .write(move |ws| FilesRepository::new(ws.main().connection()).delete(&id))
            .await?;
        if removed {
            deleted += 1;
            bytes += file.size as i64;
        }
    }
    // v4 `collapse-stale-chat-assets.ts:252`, gated on `deleted > 0` exactly as
    // v4 gates it — a stale chat with nothing left to collapse says nothing.
    // The sentence is v4's byte-for-byte; the field NAMES follow this crate's
    // snake_case tracing idiom (every other v5 site does), not v4's JSON keys.
    if deleted > 0 {
        tracing::info!(
            target: "quilltap::maintenance",
            chat_id = %chat_id,
            deleted,
            bytes_released_estimate = bytes,
            "Collapsed stale chat assets",
        );
    }
    Ok((deleted, bytes))
}

/// Collapse every stale chat's superseded generated assets (v4
/// `collapseStaleChatAssets`). Each chat is processed independently so one
/// failure cannot abort the rest. `now_ms` is injected (the differential pins it).
pub async fn collapse_stale_chat_assets(
    db: &Db,
    now_ms: i64,
) -> Result<StaleChatCollapseSummary, DbError> {
    // v4 now resolves the window through `resolveStaleChatDays()` (the
    // user-configurable `dataRetention.staleChatDays`, default 30) so the image
    // collapse, cache collapse, and cold-tier always agree on "stale".
    let cutoff = retention_cutoff_iso(resolve_stale_chat_days(db), now_ms);
    let cutoff_ms = iso_to_ms(&cutoff).unwrap_or(now_ms);

    let all_chats = db.read_main(chats_read::find_all)?;
    let mut summary = StaleChatCollapseSummary {
        chats_scanned: all_chats.len(),
        ..Default::default()
    };

    for chat in &all_chats {
        if !is_stale(db, chat, cutoff_ms)? {
            continue;
        }
        summary.stale_chats += 1;
        // v4 wraps each chat's collapse in try/catch — a failure is swallowed
        // (warn) and the rest continue.
        match collapse_one_chat(db, chat).await {
            Ok((deleted, bytes)) => {
                if deleted > 0 {
                    summary.chats_collapsed += 1;
                    summary.files_deleted += deleted;
                    summary.bytes_released_estimate += bytes;
                }
            }
            // v4 `collapse-stale-chat-assets.ts:288` — the warn IS the port
            // surface here: without it a chat that fails to collapse leaves
            // nothing behind to diagnose with (dogfood finding #110).
            Err(e) => {
                // The id is bound OUT of the macro: inside `tracing::warn!`,
                // `Value::as_str` resolves to the trait, not the enum
                // (E0782 — a standing trap in this tree).
                let chat_id = chat.get("id").and_then(Value::as_str).unwrap_or_default();
                tracing::warn!(
                    target: "quilltap::maintenance",
                    chat_id = %chat_id,
                    error = %e,
                    "Failed to collapse stale chat — continuing",
                );
            }
        }
    }

    // v4 `collapse-stale-chat-assets.ts:302` — logged unconditionally, the whole
    // summary, so an operator can see a pass that deleted nothing as readily as
    // one that deleted a hundred files.
    tracing::info!(
        target: "quilltap::maintenance",
        chats_scanned = summary.chats_scanned,
        stale_chats = summary.stale_chats,
        chats_collapsed = summary.chats_collapsed,
        files_deleted = summary.files_deleted,
        bytes_released_estimate = summary.bytes_released_estimate,
        "Stale-chat asset collapse complete",
    );
    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::collapse_stale_chat_assets;
    use crate::db::doc_mount_file_links::is_photos_relative_path;
    use crate::db::runtime::Db;
    use std::sync::{Arc, Mutex};

    // ---- the tracing capture layer (the `cost_events.rs` idiom: `set_default`
    // is THREAD-scoped, so parallel tests cannot steal each other's subscriber).

    struct FieldVisitor(String);
    impl tracing::field::Visit for FieldVisitor {
        fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
            self.0.push_str(&format!(" {}={:?}", field.name(), value));
        }
    }
    struct CaptureLayer(Arc<Mutex<Vec<String>>>);
    impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for CaptureLayer {
        fn on_event(
            &self,
            event: &tracing::Event<'_>,
            _c: tracing_subscriber::layer::Context<'_, S>,
        ) {
            let meta = event.metadata();
            let mut v = FieldVisitor(format!("{} {}", meta.level(), meta.target()));
            event.record(&mut v);
            self.0.lock().unwrap().push(v.0);
        }
    }
    fn captured<T>(f: impl FnOnce() -> T) -> (T, Vec<String>) {
        use tracing_subscriber::layer::SubscriberExt;
        let logs = Arc::new(Mutex::new(Vec::<String>::new()));
        let sub = tracing_subscriber::registry().with(CaptureLayer(logs.clone()));
        let out = {
            let _g = tracing::subscriber::set_default(sub);
            f()
        };
        let lines = logs.lock().unwrap().clone();
        (out, lines)
    }

    const TEST_PEPPER: &str = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";

    /// A freshly PROVISIONED, empty instance — the REAL schema replayed from the
    /// D23-dumped `fresh_schema.json`, never a hand-rolled subset (the standing
    /// reduced-DDL trap: `chats` alone carries ~90 columns and the reader selects
    /// them by name). `with_files == false` DROPS `files` afterwards, which is
    /// how the per-chat FAILURE arm is reached without inventing an error.
    fn collapse_db(with_files: bool) -> (tempfile::TempDir, Db) {
        let dir = tempfile::tempdir().expect("tempdir");
        crate::services::provisioning::provision_fresh_instance(dir.path(), TEST_PEPPER)
            .expect("provision a fresh instance");
        let db = Db::open_main(dir.path().join("quilltap.db"), TEST_PEPPER).expect("open main");
        if !with_files {
            db.write_blocking(|w| {
                w.main().connection().execute_batch("DROP TABLE files;")?;
                Ok(())
            })
            .unwrap();
        }
        (dir, db)
    }

    fn seed_stale_chat(db: &Db, chat_id: &str) {
        let sql = format!(
            "INSERT INTO chats (id, userId, title, createdAt, updatedAt)
             VALUES ('{chat_id}', 'u1', 'Quiet', '2020-01-01T00:00:00.000Z', '2020-01-01T00:00:00.000Z');"
        );
        db.write_blocking(move |w| {
            w.main().connection().execute_batch(&sql)?;
            Ok(())
        })
        .unwrap();
    }

    fn seed_generated_image(db: &Db, file_id: &str, chat_id: &str, size: i64) {
        // An EMPTY sha256 keeps `skip_reason` off its two sha branches (which
        // would reach the mount partition this DB does not have).
        let sql = format!(
            "INSERT INTO files (id, userId, sha256, originalFilename, mimeType, size,
                 linkedTo, source, category, createdAt, updatedAt)
             VALUES ('{file_id}', 'u1', '', 'bg.webp', 'image/webp', {size},
                 '[\"{chat_id}\"]', 'GENERATED', 'IMAGE',
                 '2020-01-01T00:00:00.000Z', '2020-01-01T00:00:00.000Z');"
        );
        db.write_blocking(move |w| {
            w.main().connection().execute_batch(&sql)?;
            Ok(())
        })
        .unwrap();
    }

    fn rt() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
    }

    /// Dogfood finding #110. v4 `collapse-stale-chat-assets.ts:252` and `:302`.
    /// The sweep DELETES the operator's generated images; before this it said
    /// nothing at all, in `combined.log` or anywhere else.
    #[test]
    fn a_collapsed_chat_is_named_in_the_log_with_v4s_sentences() {
        let (_dir, db) = collapse_db(true);
        seed_stale_chat(&db, "chat-quiet");
        seed_generated_image(&db, "file-dead", "chat-quiet", 4096);

        let (summary, lines) = captured(|| {
            rt().block_on(collapse_stale_chat_assets(&db, 1_900_000_000_000))
                .expect("collapse")
        });

        assert_eq!(summary.files_deleted, 1, "the sweep must actually delete");

        let per_chat: Vec<&String> = lines
            .iter()
            .filter(|l| l.contains("Collapsed stale chat assets"))
            .collect();
        assert_eq!(per_chat.len(), 1, "one per-chat line, got {lines:?}");
        assert!(
            per_chat[0].starts_with("INFO quilltap::maintenance"),
            "{:?}",
            per_chat[0]
        );
        assert!(
            per_chat[0].contains("chat_id=chat-quiet"),
            "{:?}",
            per_chat[0]
        );
        assert!(per_chat[0].contains("deleted=1"), "{:?}", per_chat[0]);
        assert!(
            per_chat[0].contains("bytes_released_estimate=4096"),
            "{:?}",
            per_chat[0]
        );

        let done: Vec<&String> = lines
            .iter()
            .filter(|l| l.contains("Stale-chat asset collapse complete"))
            .collect();
        assert_eq!(done.len(), 1, "one completion line, got {lines:?}");
        assert!(done[0].contains("stale_chats=1"), "{:?}", done[0]);
        assert!(done[0].contains("chats_collapsed=1"), "{:?}", done[0]);
        assert!(done[0].contains("files_deleted=1"), "{:?}", done[0]);
    }

    /// The silence half: a stale chat with nothing to collapse says nothing
    /// per-chat (v4 gates on `deleted > 0`) but the pass still reports.
    #[test]
    fn a_chat_with_nothing_to_collapse_is_not_named() {
        let (_dir, db) = collapse_db(true);
        seed_stale_chat(&db, "chat-empty");

        let (summary, lines) = captured(|| {
            rt().block_on(collapse_stale_chat_assets(&db, 1_900_000_000_000))
                .expect("collapse")
        });

        assert_eq!(summary.files_deleted, 0);
        assert!(
            !lines
                .iter()
                .any(|l| l.contains("Collapsed stale chat assets")),
            "a chat that lost nothing must not be named: {lines:?}"
        );
        assert!(
            lines
                .iter()
                .any(|l| l.contains("Stale-chat asset collapse complete")
                    && l.contains("chats_collapsed=0")),
            "the pass still reports: {lines:?}"
        );
    }

    /// v4 `collapse-stale-chat-assets.ts:288` — one chat's failure is swallowed
    /// so the rest continue, and the warn is the only thing left to diagnose
    /// with. Reached by removing `files` rather than by inventing an error.
    #[test]
    fn a_chat_that_fails_to_collapse_is_warned_and_the_pass_continues() {
        let (_dir, db) = collapse_db(false);
        seed_stale_chat(&db, "chat-broken");

        let (summary, lines) = captured(|| {
            rt().block_on(collapse_stale_chat_assets(&db, 1_900_000_000_000))
                .expect("the pass itself must not fail")
        });

        assert_eq!(summary.stale_chats, 1);
        assert_eq!(summary.chats_collapsed, 0);

        let warn: Vec<&String> = lines
            .iter()
            .filter(|l| l.contains("Failed to collapse stale chat — continuing"))
            .collect();
        assert_eq!(warn.len(), 1, "one warn, got {lines:?}");
        assert!(
            warn[0].starts_with("WARN quilltap::maintenance"),
            "{:?}",
            warn[0]
        );
        assert!(warn[0].contains("chat_id=chat-broken"), "{:?}", warn[0]);
        assert!(warn[0].contains("error="), "{:?}", warn[0]);
    }

    #[test]
    fn photos_relative_path_predicate() {
        // In a photos/ folder → true (case-insensitive).
        assert!(is_photos_relative_path(Some("photos/a.webp")));
        assert!(is_photos_relative_path(Some("Photos/a.webp")));
        assert!(is_photos_relative_path(Some("PHOTOS/a.webp")));
        // A nested photos/ subfolder → true (startsWith "photos/").
        assert!(is_photos_relative_path(Some("photos/sub/a.webp")));
        // Not in photos/ → false.
        assert!(!is_photos_relative_path(Some("images/a.webp")));
        assert!(!is_photos_relative_path(Some("a.webp"))); // dirname "." → false
        assert!(!is_photos_relative_path(Some("my-photos/a.webp"))); // dirname "my-photos"
        assert!(!is_photos_relative_path(Some("photosx/a.webp")));
        // null/empty → false.
        assert!(!is_photos_relative_path(None));
        assert!(!is_photos_relative_path(Some("")));
    }
}
