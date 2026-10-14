//! v4 `lib/backup/restore/delete-service.ts` (P4.9G3) — the per-user wipe
//! ([`delete_user_data`]), the full account reset ([`delete_all_user_data`]) and
//! its count-only sibling ([`preview_delete_all_user_data`]).
//!
//! [`delete_user_data`] is the seam restore's `replace` mode consumes (P4.9G5);
//! its signature is pinned by the round's Shared contract §2.
//!
//! ## The four deliberate, named divergences
//!
//! 1. **Id collection is slim, not overlaid.** v4 gathers its delete lists with
//!    the overlaid finders (`characters.findByUserId`, `projects.findAll`,
//!    `groups.findAll`) — which **drop** an entity whose vault / official store is
//!    unavailable, so a broken vault silently survives "delete all my data" (and
//!    is under-counted in the summary). v5 collects ids with slim
//!    `SELECT id … [WHERE userId = ?]` queries, so the wipe is complete. Identical
//!    on healthy data (what the differential exercises); strictly better on broken
//!    data.
//! 2. **The file byte sweep is metadata-only.** v4 calls
//!    `fileStorageManager.deleteFile(file)` before each `files` row delete. In v5
//!    that byte reclaim is a host seam and this bulk-wipe path threads no
//!    `StorageBackend`, so the stored bytes are not reclaimed here (v4's own
//!    delete-all does no thumbnail cleanup either — that is the per-file
//!    `api::files::file_delete` path + the daily orphan sweep). Mount-blob-backed
//!    bytes are *not* orphaned:
//!    [`clear_format3_entities`] truncates `doc_mount_blobs` /
//!    `doc_mount_file_links` wholesale. Legacy `<base>/files/**` disk bytes DO
//!    survive the wipe — the loud, named deferral of this lane.
//!    Note v4's try/catch wraps BOTH the byte delete and the row delete, so a
//!    throwing byte delete makes v4 *skip* that row; v5 (no byte step) always
//!    deletes the row, which is v4's success path.
//! 3. **No wardrobe step (P4.D284, v4 `9ecebf986`).** v4's delete-service no
//!    longer reads or deletes wardrobe items at all — the `wardrobe.findAll()`
//!    read, the per-item `wardrobe.delete` and the `wardrobeItems` count key are
//!    gone (its repository stopped knowing the legacy `wardrobe_items` table).
//!    Wardrobe items live only in document stores, which
//!    [`clear_format3_entities`] wipes with every other store table (the
//!    `wardrobe_wear_stats` ledger with the main tables). v5 follows: the old
//!    SQL-mirror loop this note used to describe is removed, and a leftover
//!    legacy `wardrobe_items` row is left where it lies, as v4 leaves it.
//! 4. **`conversation_annotations` is wiped.** v4 used to delete the table on
//!    no path, so a replace-mode restore collided with the survivors; v5
//!    truncated it first and v4 has since converged (bug 10 — the table is in
//!    `delete-service.ts`' `mainTables` at `f5e953a3f`). See
//!    [`V5_EXTRA_MAIN_TABLES`] for the reasoning, the vintage evidence and the
//!    ruling it was made under (dogfood finding #57).

use rusqlite::Connection;
use serde::Serialize;

use crate::db::characters::CharactersRepository;
use crate::db::chat_settings::ChatSettingsRepository;
use crate::db::chats::ChatsRepository;
use crate::db::connection_profiles::ConnectionProfilesRepository;
use crate::db::embedding_profiles::EmbeddingProfilesRepository;
use crate::db::files::FilesRepository;
use crate::db::folders::FoldersRepository;
use crate::db::groups::GroupsRepository;
use crate::db::image_profiles::ImageProfilesRepository;
use crate::db::llm_logs::LLMLogsRepository;
use crate::db::projects::ProjectsRepository;
use crate::db::prompt_templates::PromptTemplatesRepository;
use crate::db::roleplay_templates::RoleplayTemplatesRepository;
use crate::db::runtime::{Db, WriterSet};
use crate::db::tags::TagsRepository;
use crate::db::{api_keys, chat_settings, DbError};
use crate::services::memory_retire::{retire_memories, RetireOptions, RetireReason, RetireReport};

// ── The summary shape (v4 `DeleteSummary`, :191) ─────────────────────────────

/// v4 `DeleteSummary.profiles`.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub struct SummaryProfiles {
    pub connection: i64,
    pub image: i64,
    pub embedding: i64,
}

/// v4 `DeleteSummary.templates`.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub struct SummaryTemplates {
    pub prompt: i64,
    pub roleplay: i64,
}

/// v4 `DeleteSummary` (`delete-service.ts:191`) — field order is v4's literal
/// order (it is the wire body under `preserve_order`).
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DeleteSummary {
    pub characters: i64,
    pub chats: i64,
    pub tags: i64,
    pub files: i64,
    pub memories: i64,
    pub api_keys: i64,
    pub backups: i64,
    /// Archived-character bundles (`files` rows of category `ARCHIVE`) present
    /// at the time of the operation. In a PREVIEW this is the count on hand;
    /// after a deletion it is the count kept (when
    /// `keep_archived_character_bundles`) or wiped (when not) — read alongside
    /// `archive_bundles_kept` (v4 `d553f72a`).
    pub archive_bundles: i64,
    /// True when the operation spared the archive bundles (the `files` count
    /// then EXCLUDES them).
    pub archive_bundles_kept: bool,
    pub projects: i64,
    pub profiles: SummaryProfiles,
    pub templates: SummaryTemplates,
}

/// v4 `DeleteUserDataOptions` (`delete-service.ts`, new in `d553f72a`).
#[derive(Debug, Clone, Copy, Default)]
pub struct DeleteUserDataOptions {
    /// When true (**the effective default** — v4's `!== false`), `files` rows
    /// of category `ARCHIVE` and their on-disk bytes are spared from the wipe.
    ///
    /// The character ROWS are not spared (they are ordinary rows), so what
    /// survives is a LOOSE bundle: importable, not rehydratable. The bundle is
    /// encrypted under the user passphrase (or the internal constant), never
    /// under anything stored in a database, so it stays openable after the
    /// wipe. The destructive choice must be the explicit one — pass `false` to
    /// wipe archives too.
    pub keep_archived_character_bundles: Option<bool>,
}

impl DeleteUserDataOptions {
    /// v4's `options?.keepArchivedCharacterBundles !== false` — absent means
    /// KEEP, and only an explicit `false` wipes.
    fn keep_archives(&self) -> bool {
        self.keep_archived_character_bundles != Some(false)
    }
}

/// Is this `files` row an archived-character bundle? (v4 `isArchiveBundle`.)
fn is_archive_bundle(file: &crate::db::files::FileFull) -> bool {
    file.category == crate::services::character_archive::reencrypt::ARCHIVE_CATEGORY
}

// ── Slim id collection (see divergence 1 in the module docs) ─────────────────
//
// Every v4 finder here runs under `AbstractBaseRepository.safeQuery(op, msg,
// ctx, [])` — the **fallback** overload, which swallows the error and yields the
// empty list. That is not academic: on a real instance a lazily-created
// collection (`prompt_templates` in the committed fixture) simply has no table,
// and v4 counts it as zero. The `list_or_empty` helper reproduces exactly that;
// the DELETE calls stay strict (v4's deletes use the no-fallback overload and
// rethrow).

/// v4's `safeQuery(…, fallback: [])`: any read error → the empty list.
fn list_or_empty<T>(r: Result<Vec<T>, rusqlite::Error>) -> Vec<T> {
    r.unwrap_or_default()
}

fn query_ids(conn: &Connection, sql: &str, p: &[&dyn rusqlite::ToSql]) -> Vec<String> {
    list_or_empty((|| {
        let mut stmt = conn.prepare(sql)?;
        let rows = stmt.query_map(p, |r| r.get::<_, String>(0))?;
        rows.collect::<Result<Vec<String>, _>>()
    })())
}

/// `SELECT id FROM <table> WHERE userId = ?1`, in rowid order.
fn ids_by_user(conn: &Connection, table: &str, user_id: &str) -> Vec<String> {
    query_ids(
        conn,
        &format!("SELECT id FROM \"{table}\" WHERE userId = ?1"),
        &[&user_id],
    )
}

/// `SELECT id FROM <table>` (the instance-scoped entities — projects / groups
/// carry no `userId` since the store cutover; v4's wrappers ignore the user too).
fn ids_all(conn: &Connection, table: &str) -> Vec<String> {
    query_ids(conn, &format!("SELECT id FROM \"{table}\""), &[])
}

/// `SELECT id FROM memories WHERE characterId = ?1` — v4
/// `memories.findByCharacterId(characterId)` reduced to the ids the wipe (and
/// the count) need.
fn memory_ids_for_character(conn: &Connection, character_id: &str) -> Vec<String> {
    query_ids(
        conn,
        "SELECT id FROM memories WHERE characterId = ?1",
        &[&character_id],
    )
}

// ── clearFormat3Entities (v4 :33) ────────────────────────────────────────────

/// v4's main-DB truncate list (:34). `text_replacement_rules` is included
/// deliberately: it is global (no `userId`), so the per-row userId-scoped wipe
/// never reaches it, and a replace-mode restore must be able to re-insert the
/// backup's rules without colliding on `(fromText, caseSensitive)`.
const FORMAT3_MAIN_TABLES: &[&str] = &[
    "chat_documents",
    // === P4.D205 (v4 `e7d77bb60`, `delete-service.ts:33-42`) ===
    // Inform rows. No `userId` column (single-user), so the per-row scoped
    // deletion never reaches them; without this a replace-mode restore would
    // collide on the preserved primary keys, exactly as `chat_documents` does.
    "chat_informs",
    // === end P4.D205 ===
    "conversation_chunks",
    "tfidf_vocabularies",
    "embedding_status",
    "vector_entries",
    "vector_indices",
    "text_replacement_rules",
    // === P4.D264 (v4 `3ee3b1342`, `delete-service.ts:53-56`) ===
    // "The wardrobe wear ledger. Global like the rest of this list (no userId),
    // and its unique (itemId, wearer) index would otherwise make a
    // replace-mode restore merge into stale tallies rather than replace them."
    "wardrobe_wear_stats",
    // === end P4.D264 ===
];

/// ## ⚠ DELIBERATE DIVERGENCE (dogfood #57, 2026-08-03) — v5 wipes what v4 leaks
///
/// `conversation_annotations` appeared in **no** v4 delete path (before v4's
/// bug-10 convergence, below). It was not in `clearFormat3Entities`' main list,
/// `deleteUserData` never collected it, and `chats.repository.delete()` sweeps
/// only the message rows — so on v4 the table survived "delete all my data"
/// and survived a `replace`-mode restore's wipe.
///
/// That is not merely untidy. The table is chat-scoped and the backup collects
/// it per chat, so a replace-mode restore re-inserts every archived annotation
/// on top of the survivors. On a **migration-vintage** instance the migrated DDL
/// carries `UNIQUE("chatId","messageIndex","characterName")` (v4
/// `migrations/scripts/sqlite-initial-schema.ts:188`, repeated in
/// `create-conversation-tables.ts:68`) which `generateDDL` does NOT emit — so the
/// re-insert collides and the restore reports
/// `UNIQUE constraint failed: conversation_annotations.chatId, messageIndex,
/// characterName` once per archived annotation. That is exactly what the
/// 2026-08-03 Part F walk saw, eight times.
///
/// Whether v4 *sometimes* gets away with it is a vintage accident too: the
/// oldest DDL (`sqlite-initial-schema.ts:189`) also declares
/// `FOREIGN KEY ("chatId") REFERENCES "chats"("id") ON DELETE CASCADE`, so on
/// that vintage the per-chat deletes cascade the annotations away; the later
/// `create-conversation-tables.ts` DDL and `generateDDL` both omit the FK, and
/// there the rows simply persist.
///
/// Under the **standing ruling of 2026-08-03** (backup/restore/import/export:
/// v5 FIXES v4's bugs rather than reproducing them) v5 truncates the table here.
/// v5 made this fix first; **v4 has since CONVERGED** (`3bb664f0`, bug 10 —
/// `conversation_annotations` added to `clearFormat3Entities`'s `mainTables`),
/// so `system_delete_data_equivalence` now compares this key as a plain equality.
///
/// Kept as its own list rather than folded into [`FORMAT3_MAIN_TABLES`] so the
/// v4 list stays a verbatim transcription and the addition is visible as an
/// addition. Statement order among these truncates carries no meaning — they are
/// independent `DELETE`s with no foreign keys between them.
const V5_EXTRA_MAIN_TABLES: &[&str] = &["conversation_annotations"];

/// v4's mount-index truncate list (:62).
const FORMAT3_MOUNT_TABLES: &[&str] = &[
    "doc_mount_chunks",
    "doc_mount_blobs",
    "doc_mount_documents",
    "doc_mount_file_links",
    "doc_mount_files",
    "doc_mount_folders",
    "project_doc_mount_links",
    "group_doc_mount_links",
    "group_character_members",
    "doc_mount_points",
];

/// v4 `clearFormat3Entities` (:33) — raw bulk `DELETE`s (per-row repository
/// cascades are too slow at chunk/vector scale). Each statement is independently
/// guarded so a very old database missing a table still truncates the rest.
///
/// `instance_settings` is deliberately LEFT ALONE (v4's :54 comment): startup
/// migrations write keys that are not part of a backup, and restore upserts the
/// backup's rows by key.
pub fn clear_format3_entities(main: &Connection, mount: Option<&Connection>) {
    for table in FORMAT3_MAIN_TABLES.iter().chain(V5_EXTRA_MAIN_TABLES) {
        let _ = main.execute(&format!("DELETE FROM \"{table}\""), []);
    }
    // v4: `isMountIndexDegraded()` / a null handle → skip the sibling sweep.
    let Some(mount) = mount else { return };
    for table in FORMAT3_MOUNT_TABLES {
        let _ = mount.execute(&format!("DELETE FROM \"{table}\""), []);
    }
}

// ── The wipe (v4 `deleteUserData`, :90) ──────────────────────────────────────

/// v4 `lib/backup/restore/delete-service.ts:90` `deleteUserData(userId)` —
/// the full per-user wipe, ALSO used by restore's `replace` mode.
pub async fn delete_user_data(
    db: &Db,
    user_id: &str,
    options: DeleteUserDataOptions,
) -> Result<RetireReport, DbError> {
    let user_id = user_id.to_string();
    db.write(move |ws| delete_user_data_on_writer(ws, &user_id, options))
        .await
}

/// The wipe body, on the writer thread. Reads run over the writer's own
/// connections so the collect → delete sequence sees one consistent view.
pub fn delete_user_data_on_writer(
    ws: &mut WriterSet,
    user_id: &str,
    options: DeleteUserDataOptions,
) -> Result<RetireReport, DbError> {
    let keep_archives = options.keep_archives();
    let main = ws.main().connection();
    let mount = ws.mount_index().map(|w| w.connection());
    let llm = ws.llm_logs().map(|w| w.connection());

    // 1. Collect every list (v4's one `Promise.all`, :97). All of these are
    //    `safeQuery(…, [])` reads in v4 — an error yields the empty list.
    let characters = ids_by_user(main, "characters", user_id);
    let chats = ids_by_user(main, "chats", user_id);
    let tags = ids_by_user(main, "tags", user_id);
    let files = FilesRepository::new(main)
        .find_by_user_id(user_id)
        .unwrap_or_default();
    let connection_profiles = ids_by_user(main, "connection_profiles", user_id);
    let image_profiles = ids_by_user(main, "image_profiles", user_id);
    let embedding_profiles = ids_by_user(main, "embedding_profiles", user_id);
    let prompt_templates = ids_by_user(main, "prompt_templates", user_id);
    let roleplay_templates = ids_by_user(main, "roleplay_templates", user_id);
    let projects = ids_all(main, "projects");
    let groups = ids_all(main, "groups");
    // v4: `repos.llmLogs.findAll(10000)` → `findByUserId(userId, 10000, 0)`, i.e.
    // `createdAt DESC LIMIT 10000`. The hard-coded limit is carried verbatim: an
    // instance with more than 10 000 logs keeps the oldest ones (v4's behavior).
    let llm_logs = match llm {
        Some(conn) => LLMLogsRepository::new(conn)
            .find_recent(user_id, 10_000.0)
            .unwrap_or_default(),
        None => Vec::new(),
    };
    let chat_settings_row = chat_settings::find_by_user_id(main, user_id).unwrap_or(None);
    let folders = FoldersRepository::new(main)
        .find_by_user_id(user_id)
        .unwrap_or_default();

    // 2. Memories first, per character, through the deletion chokepoint
    //    (v4 `914b59e13`, :149). v4's why-comment, carried: with the whole
    //    corpus in one doomed set the neighbour scrub is a no-op (every
    //    candidate is itself doomed), so this degrades to per-character bulk
    //    deletes instead of the old per-row loop. The direct per-row
    //    `memories.delete` was the last bypass of the chokepoint on this path;
    //    [`delete_all_routes_memory_deletion_through_the_chokepoint`] pins that
    //    it is not reached from here, the shape of v4's own regression test.
    //
    //    P4.D288 (C1-7, §R.10(q)): the ONE retire path (`restore-replace`, v4
    //    `delete-service.ts:169`). Its report leaves the closure as this fn's
    //    return value; the after-commit `announce_retirement` (R-AF) at the two
    //    callers is P4.D293's (§R.10(l)) — the CONSUMER CONTRACT: hand
    //    `report.announcement` to `memory_retire::announce_retirement` once the
    //    wipe's write has committed.
    let retired = {
        let mut memory_ids: Vec<String> = Vec::new();
        for character_id in &characters {
            memory_ids.extend(memory_ids_for_character(main, character_id));
        }
        retire_memories(
            main,
            &memory_ids,
            RetireOptions::new(RetireReason::RestoreReplace),
        )?
    };

    // 3. The per-row entity deletes, in v4's `Promise.all` order (:126).
    //
    //    Each v4 `characters.delete` logs INFO `Character deleted` and then
    //    awaits `reconcileVaultNames('character-deleted', id)` (bug 186). Under
    //    the `Promise.all`, MEASURED at the pin (P4.D274 R-F, M3 — five runs,
    //    identical): every delete's line first, then N reconciles that EACH
    //    plan from the same post-delete read (all N reads land before the
    //    first write) and EACH log the whole rename list. The renames are
    //    state-inert here (`clear_format3_entities` below deletes every
    //    `doc_mount_points` row) but the lines are not, so v5 replays that
    //    cadence literally: the delete loop, then
    //    `reconcile_vault_names_after_concurrent_deletes`.
    {
        let repo = CharactersRepository::new(main);
        let mut deleted: Vec<String> = Vec::new();
        for id in &characters {
            if repo.delete(id)? {
                tracing::info!(target: "quilltap::db", characterId = %id, "Character deleted");
                deleted.push(id.clone());
            }
        }
        crate::db::store_name_reconcile::reconcile_vault_names_after_concurrent_deletes(
            main, mount, &deleted,
        );
    }
    {
        // v4 passes `{ syncVaults: false }`; v5's chat delete never runs the
        // participant-vault summary sweep (a deferred external subsystem), so the
        // flag is already the behavior.
        let repo = ChatsRepository::new(main);
        for id in &chats {
            repo.delete(id)?;
        }
    }
    {
        let repo = TagsRepository::new(main);
        for id in &tags {
            repo.delete(id)?;
        }
    }
    {
        let repo = ConnectionProfilesRepository::new(main);
        for id in &connection_profiles {
            repo.delete(id)?;
        }
    }
    {
        let repo = ImageProfilesRepository::new(main);
        for id in &image_profiles {
            repo.delete(id)?;
        }
    }
    {
        let repo = EmbeddingProfilesRepository::new(main);
        for id in &embedding_profiles {
            repo.delete(id)?;
        }
    }
    {
        let repo = PromptTemplatesRepository::new(main);
        for id in &prompt_templates {
            repo.delete(id)?;
        }
    }
    {
        let repo = RoleplayTemplatesRepository::new(main);
        for id in &roleplay_templates {
            repo.delete(id)?;
        }
    }
    // Projects + group SLIM rows only. Membership and additional-store links are
    // cleared in bulk by `clear_format3_entities`
    // (`group_character_members` / `group_doc_mount_links`); the official store
    // mount points go with `doc_mount_points` there too (v4 :135-138).
    if let Some(mount_conn) = mount {
        let repo = ProjectsRepository::new(main, mount_conn);
        for id in &projects {
            repo.delete(id)?;
        }
        let repo = GroupsRepository::new(main, mount_conn);
        for id in &groups {
            repo.delete(id)?;
        }
    }
    if let Some(conn) = llm {
        let repo = LLMLogsRepository::new(conn);
        for log in &llm_logs {
            repo.delete(&log.id)?;
        }
    }
    if let Some(row) = &chat_settings_row {
        if let Some(id) = row.get("id").and_then(|v| v.as_str()) {
            ChatSettingsRepository::new(main).delete(id)?;
        }
    }
    {
        let repo = FoldersRepository::new(main);
        for folder in &folders {
            repo.delete(&folder.id)?;
        }
    }

    // 4. Files last, sequentially. v4 deletes the storage bytes first inside a
    //    warn-only try/catch; v5 removes the metadata row only (divergence 2).
    //    Archived-character bundles are spared when
    //    `keep_archived_character_bundles` (the default) — row and bytes both
    //    survive as loose, importable bundles (spec §4.7, v4 `d553f72a`).
    {
        let repo = FilesRepository::new(main);
        let mut archive_bundles_kept = 0i64;
        for file in &files {
            if keep_archives && is_archive_bundle(file) {
                archive_bundles_kept += 1;
                continue;
            }
            repo.delete(&file.id)?;
        }
        if archive_bundles_kept > 0 {
            tracing::info!(
                target: "quilltap::delete",
                user_id,
                archive_bundles_kept,
                "Spared archived character bundles from wipe",
            );
        }
    }

    // 5. The format-3 bulk truncate (v4 :164).
    clear_format3_entities(main, mount);

    Ok(retired)
}

// ── The counts (v4 `deleteAllUserData` :221 / `previewDeleteAllUserData` :303) ─

/// The shared count pass both `deleteAllUserData` (before deleting) and
/// `previewDeleteAllUserData` run.
fn collect_summary(main: &Connection, user_id: &str) -> Result<DeleteSummary, DbError> {
    let characters = ids_by_user(main, "characters", user_id);
    let chats = ids_by_user(main, "chats", user_id);
    let tags = ids_by_user(main, "tags", user_id);
    let files = FilesRepository::new(main)
        .find_by_user_id(user_id)
        .unwrap_or_default();
    let connection_profiles = ids_by_user(main, "connection_profiles", user_id);
    let image_profiles = ids_by_user(main, "image_profiles", user_id);
    let embedding_profiles = ids_by_user(main, "embedding_profiles", user_id);
    // v4 `getAllApiKeys()` — the fallback by-user read: a failed read is its
    // line and `[]` (count 0); a bad row is dropped with its WARN (P4.139).
    let api_keys = crate::services::api_key_service::api_keys_by_user_id_or_empty(main, user_id);
    let prompt_templates = ids_by_user(main, "prompt_templates", user_id);
    let roleplay_templates = ids_by_user(main, "roleplay_templates", user_id);
    let projects = ids_all(main, "projects");

    let mut memories = 0i64;
    for character_id in &characters {
        memories += memory_ids_for_character(main, character_id).len() as i64;
    }

    // v4 :243 — `folderPath === '/backups' || originalFilename?.endsWith('.zip')`.
    // The `||` is deliberate (a .zip anywhere counts as a backup).
    let backups = files
        .iter()
        .filter(|f| {
            f.folder_path.as_deref() == Some("/backups") || f.original_filename.ends_with(".zip")
        })
        .count() as i64;

    Ok(DeleteSummary {
        characters: characters.len() as i64,
        chats: chats.len() as i64,
        tags: tags.len() as i64,
        files: files.len() as i64,
        memories,
        api_keys: api_keys.len() as i64,
        backups,
        archive_bundles: files.iter().filter(|f| is_archive_bundle(f)).count() as i64,
        // The PREVIEW reports what's on hand; whether they survive is the
        // caller's checkbox, so `kept` here just states the default.
        // `delete_all_user_data` overwrites both this and `files` from the
        // options it was actually given (v4 `d553f72a`).
        archive_bundles_kept: true,
        projects: projects.len() as i64,
        profiles: SummaryProfiles {
            connection: connection_profiles.len() as i64,
            image: image_profiles.len() as i64,
            embedding: embedding_profiles.len() as i64,
        },
        templates: SummaryTemplates {
            prompt: prompt_templates.len() as i64,
            roleplay: roleplay_templates.len() as i64,
        },
    })
}

/// v4 `previewDeleteAllUserData(userId)` (:299) — counts only, no writes. Must
/// produce a summary IDENTICAL to what [`delete_all_user_data`] would return on
/// the same state.
pub fn preview_delete_all_user_data(db: &Db, user_id: &str) -> Result<DeleteSummary, DbError> {
    let user_id = user_id.to_string();
    db.read_main(move |main| collect_summary(main, &user_id))
}

/// v4 `deleteAllUserData(userId)` (:215) — count everything, run the wipe, then
/// delete the API keys (a warn-only loop), and return the summary. The `/backups`
/// folder itself rides the file sweep (v4's :265 only logs).
pub async fn delete_all_user_data(
    db: &Db,
    user_id: &str,
    options: DeleteUserDataOptions,
) -> Result<DeleteSummary, DbError> {
    let keep_archives = options.keep_archives();
    let uid = user_id.to_string();
    let mut summary = db.read_main(move |main| collect_summary(main, &uid))?;
    // v4 counts the bundles BEFORE deleting so the summary can report how many
    // were kept (or wiped), and subtracts them from `files` only when they
    // survive — the count then means "rows actually removed".
    summary.archive_bundles_kept = keep_archives;
    if keep_archives {
        summary.files -= summary.archive_bundles;
    }

    delete_user_data(db, user_id, options).await?;

    let uid = user_id.to_string();
    db.write(move |ws| {
        let main = ws.main().connection();
        let repo = api_keys::ApiKeysRepository::new(main);
        // v4 wraps each delete in a warn-only try/catch — one failure must not
        // abort the rest.
        // P4.139: the fallback by-user read — a bad row is dropped with its
        // WARN and every OTHER key deleted (the whole list had been `[]`, so
        // one bad row had left every key behind); a failed read is its line.
        for key in crate::services::api_key_service::api_keys_by_user_id_or_empty(main, &uid) {
            let _ = repo.delete(&key.id);
        }
        Ok(())
    })
    .await?;

    Ok(summary)
}
