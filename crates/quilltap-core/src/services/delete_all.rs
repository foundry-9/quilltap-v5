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
//!    that byte reclaim is a host seam and the dispatch layer threads no
//!    `StorageBackend` (the standing files-family precedent — see
//!    `api::files::file_delete`'s "storage + thumbnail deletes are best-effort
//!    DB-invisible side effects"). Mount-blob-backed bytes are *not* orphaned:
//!    [`clear_format3_entities`] truncates `doc_mount_blobs` /
//!    `doc_mount_file_links` wholesale. Legacy `<base>/files/**` disk bytes DO
//!    survive the wipe — the loud, named deferral of this lane.
//!    Note v4's try/catch wraps BOTH the byte delete and the row delete, so a
//!    throwing byte delete makes v4 *skip* that row; v5 (no byte step) always
//!    deletes the row, which is v4's success path.
//! 3. **Wardrobe rows.** v4's `wardrobe.findAll()` reads the `wardrobe_items`
//!    SQL table, but wardrobe items are vault-only (v4's `create` refuses a SQL
//!    fallback), so the table is empty on every real instance and the loop is a
//!    no-op. v5 ports the loop faithfully over
//!    [`crate::db::vault_wardrobe_public::delete_vault_wardrobe_item`]; it is NOT
//!    exercised by the corpus (flagged, like `cascade_delete`'s legacy-file
//!    branch).
//! 4. **`conversation_annotations` is wiped.** v4 never deletes the table on any
//!    path, so a replace-mode restore collides with the survivors. v5 truncates
//!    it — see [`V5_EXTRA_MAIN_TABLES`] for the full reasoning, the vintage
//!    evidence and the ruling it is made under (dogfood finding #57).

use rusqlite::Connection;
use serde::Serialize;

use crate::db::characters::CharactersRepository;
use crate::db::chat_settings::ChatSettingsRepository;
use crate::db::chats::ChatsRepository;
use crate::db::connection_profiles::ConnectionProfilesRepository;
use crate::db::doc_mount_documents::DocMountDocumentsRepository;
use crate::db::doc_mount_file_links::DocMountFileLinksRepository;
use crate::db::embedding_profiles::EmbeddingProfilesRepository;
use crate::db::files::FilesRepository;
use crate::db::folders::FoldersRepository;
use crate::db::groups::GroupsRepository;
use crate::db::image_profiles::ImageProfilesRepository;
use crate::db::llm_logs::LLMLogsRepository;
use crate::db::memories::MemoriesRepository;
use crate::db::projects::ProjectsRepository;
use crate::db::prompt_templates::PromptTemplatesRepository;
use crate::db::roleplay_templates::RoleplayTemplatesRepository;
use crate::db::runtime::{Db, WriterSet};
use crate::db::tags::TagsRepository;
use crate::db::vault_wardrobe_public::delete_vault_wardrobe_item;
use crate::db::{api_keys, chat_settings, DbError};

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
    pub projects: i64,
    pub profiles: SummaryProfiles,
    pub templates: SummaryTemplates,
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

/// `SELECT id, characterId FROM wardrobe_items` — the (always empty in practice)
/// SQL mirror v4's `wardrobe.findAll()` reads.
fn wardrobe_rows(conn: &Connection) -> Vec<(String, Option<String>)> {
    list_or_empty((|| {
        let mut stmt = conn.prepare("SELECT id, characterId FROM wardrobe_items")?;
        let rows = stmt.query_map([], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?))
        })?;
        rows.collect::<Result<Vec<_>, _>>()
    })())
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
    "conversation_chunks",
    "tfidf_vocabularies",
    "embedding_status",
    "vector_entries",
    "vector_indices",
    "text_replacement_rules",
];

/// ## ⚠ DELIBERATE DIVERGENCE (dogfood #57, 2026-08-03) — v5 wipes what v4 leaks
///
/// `conversation_annotations` appears in **no** v4 delete path. It is not in
/// `clearFormat3Entities`' main list, `deleteUserData` never collects it, and
/// `chats.repository.delete()` sweeps only the message rows — so on v4 the table
/// survives "delete all my data" and survives a `replace`-mode restore's wipe.
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
pub async fn delete_user_data(db: &Db, user_id: &str) -> Result<(), DbError> {
    let user_id = user_id.to_string();
    db.write(move |ws| delete_user_data_on_writer(ws, &user_id))
        .await
}

/// The wipe body, on the writer thread. Reads run over the writer's own
/// connections so the collect → delete sequence sees one consistent view.
fn delete_user_data_on_writer(ws: &mut WriterSet, user_id: &str) -> Result<(), DbError> {
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
    let wardrobe_items = wardrobe_rows(main);

    // 2. Memories first, per character (v4 :118).
    {
        let memories = MemoriesRepository::new(main);
        for character_id in &characters {
            for memory_id in memory_ids_for_character(main, character_id) {
                memories.delete(&memory_id)?;
            }
        }
    }

    // 3. The per-row entity deletes, in v4's `Promise.all` order (:126).
    {
        let repo = CharactersRepository::new(main);
        for id in &characters {
            repo.delete(id)?;
        }
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
    // Wardrobe: vault-only in practice (see divergence 3). NOT corpus-exercised.
    if let Some(mount_conn) = mount {
        let links = DocMountFileLinksRepository::new(mount_conn);
        let docs = DocMountDocumentsRepository::new(mount_conn);
        for (id, character_id) in &wardrobe_items {
            // v4 `wardrobe.delete(w.id, w.characterId ?? null)`; a `NoMount`
            // (v4 throws) is warn-and-continue here — the wipe must not abort.
            let _ = delete_vault_wardrobe_item(main, &links, &docs, id, character_id.as_deref());
        }
    }

    // 4. Files last, sequentially. v4 deletes the storage bytes first inside a
    //    warn-only try/catch; v5 removes the metadata row only (divergence 2).
    {
        let repo = FilesRepository::new(main);
        for file in &files {
            repo.delete(&file.id)?;
        }
    }

    // 5. The format-3 bulk truncate (v4 :164).
    clear_format3_entities(main, mount);

    Ok(())
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
    let api_keys = api_keys::get_api_keys_by_user_id(main, user_id).unwrap_or_default();
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
pub async fn delete_all_user_data(db: &Db, user_id: &str) -> Result<DeleteSummary, DbError> {
    let uid = user_id.to_string();
    let summary = db.read_main(move |main| collect_summary(main, &uid))?;

    delete_user_data(db, user_id).await?;

    let uid = user_id.to_string();
    db.write(move |ws| {
        let main = ws.main().connection();
        let repo = api_keys::ApiKeysRepository::new(main);
        // v4 wraps each delete in a warn-only try/catch — one failure must not
        // abort the rest.
        for key in api_keys::get_api_keys_by_user_id(main, &uid).unwrap_or_default() {
            let _ = repo.delete(&key.id);
        }
        Ok(())
    })
    .await?;

    Ok(summary)
}
