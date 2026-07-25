//! v4 `collectUserData` (`lib/backup/backup-service.ts:135`) — the full-graph
//! read that a backup archives.
//!
//! Thirty-eight collections across all three partitions. Families whose v5 read
//! already reproduces v4's `findAll` byte-for-byte are reused from `db::`;
//! the rest are marshaled here (see [`super::marshal`] for why).
//!
//! ## Deliberate divergences, all recorded
//!
//! - **No WAL checkpoint.** v4 checkpoints all three partitions first
//!   (`:556-570`) because its logical read runs on a WAL database. v5 opens
//!   `journal_mode = TRUNCATE` (the standing cloud-sync rule in CLAUDE.md), so
//!   there is no WAL to flush and the read is already consistent.
//! - **`characterPluginData.data` stays a STRING.** v4's repository lists no
//!   JSON columns, so `findByCharacterId` hands the backup the raw JSON text
//!   (`character-plugin-data.repository.ts:28` → the base `findByFilter`). v5's
//!   `db::character_plugin_data::find_by_character_id` PARSES it for its own
//!   consumer; the backup therefore reads the column itself rather than reusing
//!   that function. (The divergence in that reader is recorded in the lane
//!   record as a follow-up — it is not this lane's file to change.)

use serde_json::{Map, Value};

use super::marshal::{dump_table, marshal, query_all, select_list, table_exists, F};
use crate::db::runtime::Db;
use crate::db::{
    characters_read, chats_messages_read, chats_read, connection_profiles, image_profiles,
    js_number_to_json, memories_read, provider_models, tags, DbError,
};
use crate::embedding_blob::blob_to_float32;

/// The missing-table fallback for the collect reads that DON'T go through
/// [`query_all`] (which already guards itself with `table_exists`) — the ones
/// that reuse a typed `db::` finder instead.
///
/// v4's base repository reaches every collection through `getCollection()`,
/// which lazily `ensureCollection`s the table, and wraps the read in
/// `safeQuery(..., [])` (`base.repository.ts:252-267`). So on an instance that
/// never provisioned a given collection, v4 returns `[]` and the backup carries
/// an empty array. v5 provisions no schema outside provisioning, so the same
/// read raised `no such table` and failed the WHOLE backup.
///
/// **Found at this round's unification**, by the first live walk of
/// `systemBackupCreate` against the e2e instance: `no such table:
/// provider_models` → a bare 500 on the Create Backup button. The lane's
/// differential could not see it — it runs against the `system-data-*` fixture,
/// which unit 1 had extended with a row in every collected table, so every
/// table existed there. Any real instance that never touched provider models
/// (or tags, or connection profiles…) could not be backed up at all.
///
/// Only the missing-table case falls back; every other SQL error still
/// surfaces rather than being silently swallowed into an empty backup — the
/// same line [`table_exists`] already draws.
fn if_table<T>(
    conn: &rusqlite::Connection,
    table: &str,
    read: impl FnOnce() -> Result<Vec<T>, DbError>,
) -> Result<Vec<T>, DbError> {
    if !table_exists(conn, table) {
        return Ok(Vec::new());
    }
    read()
}

/// [`if_table`] for a finder that yields at most one row (`chat_settings`).
fn if_table_opt<T>(
    conn: &rusqlite::Connection,
    table: &str,
    read: impl FnOnce() -> Result<Option<T>, DbError>,
) -> Result<Option<T>, DbError> {
    if !table_exists(conn, table) {
        return Ok(None);
    }
    read()
}

/// Every collection a backup carries, in v4's `BackupData` order.
pub struct BackupData {
    pub characters: Vec<Value>,
    pub chats: Vec<Value>,
    pub tags: Vec<Value>,
    pub connection_profiles: Vec<Value>,
    pub image_profiles: Vec<Value>,
    pub embedding_profiles: Vec<Value>,
    pub memories: Vec<Value>,
    pub files: Vec<Value>,
    pub prompt_templates: Vec<Value>,
    pub roleplay_templates: Vec<Value>,
    pub provider_models: Vec<Value>,
    pub projects: Vec<Value>,
    pub groups: Vec<Value>,
    pub llm_logs: Vec<Value>,
    pub plugin_configs: Vec<Value>,
    pub chat_settings: Vec<Value>,
    pub folders: Vec<Value>,
    pub wardrobe_items: Vec<Value>,
    pub character_plugin_data: Vec<Value>,
    pub conversation_annotations: Vec<Value>,
    pub chat_documents: Vec<Value>,
    pub instance_settings: Vec<Value>,
    pub embedding_status: Vec<Value>,
    pub conversation_chunks: Vec<Value>,
    pub tfidf_vocabularies: Vec<Value>,
    pub vector_index_metas: Vec<Value>,
    pub vector_entries: Vec<Value>,
    pub doc_mount_points: Vec<Value>,
    pub doc_mount_folders: Vec<Value>,
    pub doc_mount_files: Vec<Value>,
    pub doc_mount_file_links: Vec<Value>,
    pub doc_mount_chunks: Vec<Value>,
    pub doc_mount_documents: Vec<Value>,
    pub doc_mount_blobs: Vec<Value>,
    pub project_doc_mount_links: Vec<Value>,
    pub group_doc_mount_links: Vec<Value>,
    pub group_character_members: Vec<Value>,
    pub text_replacement_rules: Vec<Value>,
}

impl BackupData {
    /// The total message count across every chat (v4 `createManifest`'s
    /// `totalMessages` reduce, `:433`).
    pub fn total_messages(&self) -> usize {
        self.chats
            .iter()
            .map(|c| {
                c.get("messages")
                    .and_then(Value::as_array)
                    .map(Vec::len)
                    .unwrap_or(0)
            })
            .sum()
    }
}

// ── Field specs (Zod schema order; optionals omitted when the column is NULL) ─

const FILES: &[(&str, F)] = &[
    ("id", F::Str),
    ("userId", F::Str),
    ("sha256", F::Str),
    ("originalFilename", F::Str),
    ("mimeType", F::Str),
    ("size", F::Num),
    ("width", F::NumOpt),
    ("height", F::NumOpt),
    ("isPlainText", F::BoolOpt),
    ("linkedTo", F::Json("[]")),
    ("source", F::Str),
    ("category", F::Str),
    ("generationPrompt", F::StrOpt),
    ("generationModel", F::StrOpt),
    ("generationRevisedPrompt", F::StrOpt),
    ("description", F::StrOpt),
    ("tags", F::Json("[]")),
    ("projectId", F::StrOpt),
    ("folderPath", F::StrOpt),
    ("storageKey", F::StrOpt),
    ("fileStatus", F::StrOpt),
    ("createdAt", F::Str),
    ("updatedAt", F::Str),
];

const FOLDERS: &[(&str, F)] = &[
    ("id", F::Str),
    ("userId", F::Str),
    ("path", F::Str),
    ("name", F::Str),
    ("parentFolderId", F::StrOpt),
    ("projectId", F::StrOpt),
    ("createdAt", F::Str),
    ("updatedAt", F::Str),
];

const EMBEDDING_PROFILES: &[(&str, F)] = &[
    ("id", F::Str),
    ("userId", F::Str),
    ("name", F::Str),
    ("provider", F::Str),
    ("apiKeyId", F::StrOpt),
    ("baseUrl", F::StrOpt),
    ("modelName", F::Str),
    ("dimensions", F::NumOpt),
    ("truncateToDimensions", F::NumOpt),
    ("normalizeL2", F::BoolOpt),
    ("isDefault", F::Bool),
    ("tags", F::Json("[]")),
    ("createdAt", F::Str),
    ("updatedAt", F::Str),
];

const PROMPT_TEMPLATES: &[(&str, F)] = &[
    ("id", F::Str),
    ("userId", F::StrOpt),
    ("name", F::Str),
    ("content", F::Str),
    ("description", F::StrOpt),
    ("isBuiltIn", F::Bool),
    ("category", F::StrOpt),
    ("modelHint", F::StrOpt),
    ("tags", F::Json("[]")),
    ("createdAt", F::Str),
    ("updatedAt", F::Str),
];

const ROLEPLAY_TEMPLATES: &[(&str, F)] = &[
    ("id", F::Str),
    ("userId", F::StrOpt),
    ("name", F::Str),
    ("description", F::StrOpt),
    ("systemPrompt", F::Str),
    ("isBuiltIn", F::Bool),
    ("tags", F::Json("[]")),
    ("delimiters", F::Json("[]")),
    ("renderingPatterns", F::Json("[]")),
    ("dialogueDetection", F::JsonOpt),
    ("narrationDelimiters", F::StrOpt),
    ("createdAt", F::Str),
    ("updatedAt", F::Str),
];

const PLUGIN_CONFIGS: &[(&str, F)] = &[
    ("id", F::Str),
    ("userId", F::Str),
    ("pluginName", F::Str),
    ("config", F::Json("{}")),
    ("enabled", F::BoolOpt),
    ("createdAt", F::Str),
    ("updatedAt", F::Str),
];

const CHARACTER_PLUGIN_DATA: &[(&str, F)] = &[
    ("id", F::Str),
    ("characterId", F::Str),
    ("pluginName", F::Str),
    // `z.unknown()` — the repository lists no JSON columns, so the raw text rides.
    ("data", F::Raw),
    ("createdAt", F::Str),
    ("updatedAt", F::Str),
];

const CONVERSATION_ANNOTATIONS: &[(&str, F)] = &[
    ("id", F::Str),
    ("chatId", F::Str),
    ("messageIndex", F::Num),
    ("sourceMessageId", F::StrOpt),
    ("characterName", F::Str),
    ("content", F::Str),
    ("createdAt", F::Str),
    ("updatedAt", F::Str),
];

const CHAT_DOCUMENTS: &[(&str, F)] = &[
    ("id", F::Str),
    ("chatId", F::Str),
    ("filePath", F::Str),
    ("scope", F::Str),
    ("mountPoint", F::StrOpt),
    ("displayTitle", F::StrOpt),
    ("isActive", F::Bool),
    ("createdAt", F::Str),
    ("updatedAt", F::Str),
];

const TFIDF_VOCABULARIES: &[(&str, F)] = &[
    ("id", F::Str),
    ("profileId", F::Str),
    ("userId", F::Str),
    ("vocabulary", F::Str),
    ("idf", F::Str),
    ("avgDocLength", F::Num),
    ("vocabularySize", F::Num),
    ("includeBigrams", F::Bool),
    ("fittedAt", F::Str),
    ("createdAt", F::Str),
    ("updatedAt", F::Str),
];

const EMBEDDING_STATUS: &[(&str, F)] = &[
    ("id", F::Str),
    ("userId", F::Str),
    ("entityType", F::Str),
    ("entityId", F::Str),
    ("profileId", F::Str),
    ("status", F::Str),
    ("embeddedAt", F::StrOpt),
    ("error", F::StrOpt),
    ("createdAt", F::Str),
    ("updatedAt", F::Str),
];

const VECTOR_INDEX_METAS: &[(&str, F)] = &[
    ("id", F::Str),
    ("characterId", F::Str),
    ("version", F::Num),
    ("dimensions", F::Num),
    ("createdAt", F::Str),
    ("updatedAt", F::Str),
];

const WARDROBE_ITEMS: &[(&str, F)] = &[
    ("id", F::Str),
    ("characterId", F::StrOpt),
    ("title", F::Str),
    ("description", F::StrOpt),
    ("imagePrompt", F::StrOpt),
    ("types", F::Json("[]")),
    ("componentItemIds", F::Json("[]")),
    ("appropriateness", F::StrOpt),
    ("isDefault", F::Bool),
    ("replace", F::Bool),
    ("migratedFromClothingRecordId", F::StrOpt),
    ("archivedAt", F::StrOpt),
    ("createdAt", F::Str),
    ("updatedAt", F::Str),
];

const TEXT_REPLACEMENT_RULES: &[(&str, F)] = &[
    ("id", F::Str),
    ("fromText", F::Str),
    ("toText", F::Str),
    ("caseSensitive", F::Bool),
    ("enabled", F::Bool),
    ("sortOrder", F::Num),
    ("createdAt", F::Str),
    ("updatedAt", F::Str),
];

const LLM_LOGS: &[(&str, F)] = &[
    ("id", F::Str),
    ("userId", F::Str),
    ("type", F::Str),
    ("messageId", F::StrOpt),
    ("chatId", F::StrOpt),
    ("characterId", F::StrOpt),
    ("autonomousRunId", F::StrOpt),
    ("provider", F::Str),
    ("modelName", F::Str),
    ("request", F::Json("{}")),
    ("response", F::Json("{}")),
    ("usage", F::JsonOpt),
    ("cacheUsage", F::JsonOpt),
    ("rawProviderUsage", F::JsonOpt),
    ("requestHashes", F::JsonOpt),
    ("durationMs", F::NumOpt),
    ("createdAt", F::Str),
    ("updatedAt", F::Str),
];

/// v4 `repos.llmLogs.findAll(10000)` — the high limit that "gets all user logs".
const LLM_LOGS_LIMIT: i64 = 10_000;

/// The mount-index tables dumped verbatim, in v4's `collectUserData` order.
const MOUNT_DUMPS: &[&str] = &[
    "doc_mount_points",
    "doc_mount_folders",
    "doc_mount_files",
    "doc_mount_file_links",
    "doc_mount_documents",
    "project_doc_mount_links",
    "group_doc_mount_links",
    "group_character_members",
];

/// v4 `encodeEmbedding` (`:53`) — a stored embedding becomes a plain number
/// array so it survives JSON. The blob decode is header-aware (legacy raw
/// Float32 and the self-describing quantized format both land here).
fn encode_embedding(blob: Option<Vec<u8>>) -> Value {
    match blob {
        None => Value::Null,
        Some(b) => Value::Array(
            blob_to_float32(&b)
                .into_iter()
                .map(|f| js_number_to_json(f as f64))
                .collect(),
        ),
    }
}

/// v4 `collectUserData(userId)`.
pub fn collect_user_data(db: &Db, user_id: &str) -> Result<BackupData, DbError> {
    // The mount-index partition is required for the vault overlays; an instance
    // without one cannot have characters at all.
    let (characters, projects, groups) = db.read_main(|main| {
        db.read_mount_index(|mount| {
            let characters = if_table(main, "characters", || {
                characters_read::find_all(main, mount)
            })?;
            let projects = crate::db::projects::ProjectsRepository::new(main, mount)
                .find_all()
                .unwrap_or_default();
            let groups = crate::db::groups::GroupsRepository::new(main, mount)
                .find_all()
                .unwrap_or_default();
            Ok((characters, projects, groups))
        })
    })?;

    let main_side = db.read_main(|main| {
        // Chats carry their message events inline (v4 `ChatWithMessages`), and
        // only `type === 'message'` events survive the filter (`:196`).
        let mut chats = if_table(main, "chats", || chats_read::find_all(main))?;
        for chat in &mut chats {
            let id = chat
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let messages: Vec<Value> = chats_messages_read::get_messages(main, &id)?
                .into_iter()
                .filter(|e| e.get("type").and_then(Value::as_str) == Some("message"))
                .collect();
            if let Some(obj) = chat.as_object_mut() {
                obj.insert("messages".into(), Value::Array(messages));
            }
        }

        // Memories are collected per character, in character order (`:207`).
        let mut memories = Vec::new();
        let mut character_plugin_data = Vec::new();
        for c in &characters {
            let cid = c.get("id").and_then(Value::as_str).unwrap_or_default();
            memories.extend(memories_read::find_by_character_id(main, cid)?);
            character_plugin_data.extend(query_all(
                main,
                "character_plugin_data",
                CHARACTER_PLUGIN_DATA,
                "characterId = ?1",
                &[&cid],
            )?);
        }

        // Per-chat collections, in chat order (`:219-235`).
        let mut conversation_annotations = Vec::new();
        let mut chat_documents = Vec::new();
        let mut conversation_chunks = Vec::new();
        for chat in &chats {
            let cid = chat.get("id").and_then(Value::as_str).unwrap_or_default();
            conversation_annotations.extend(query_all(
                main,
                "conversation_annotations",
                CONVERSATION_ANNOTATIONS,
                "chatId = ?1",
                &[&cid],
            )?);
            chat_documents.extend(query_all(
                main,
                "chat_documents",
                CHAT_DOCUMENTS,
                "chatId = ?1",
                &[&cid],
            )?);
            conversation_chunks.extend(read_conversation_chunks(main, cid)?);
        }

        // Vector indices, per character id that has one (`:251-265`).
        let mut vector_index_metas = Vec::new();
        let mut vector_entries = Vec::new();
        // v4 `getAllCharacterIds()` — `SELECT characterId FROM vector_indices`,
        // insertion order, no dedupe (the table is one row per character).
        let character_ids_with_vectors: Vec<String> = if table_exists(main, "vector_indices") {
            let mut stmt = main.prepare("SELECT characterId FROM vector_indices")?;
            let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
            let mut v = Vec::new();
            for r in rows {
                v.push(r?);
            }
            v
        } else {
            Vec::new()
        };
        for cid in &character_ids_with_vectors {
            let cid_ref: &str = cid;
            vector_index_metas.extend(query_all(
                main,
                "vector_indices",
                VECTOR_INDEX_METAS,
                "characterId = ?1",
                &[&cid_ref],
            )?);
            vector_entries.extend(read_vector_entries(main, cid)?);
        }

        let chat_settings = match if_table_opt(main, "chat_settings", || {
            crate::db::chat_settings::find_by_user_id(main, user_id)
        })? {
            Some(v) => vec![v],
            None => Vec::new(),
        };

        Ok(MainSide {
            chats,
            tags: if_table(main, "tags", || tags::find_all(main))?,
            connection_profiles: if_table(main, "connection_profiles", || {
                connection_profiles::find_all(main)
            })?,
            image_profiles: if_table(main, "image_profiles", || image_profiles::find_all(main))?,
            embedding_profiles: query_all(main, "embedding_profiles", EMBEDDING_PROFILES, "", &[])?,
            memories,
            files: if_table(main, "files", || collect_files(main))?,
            prompt_templates: query_all(
                main,
                "prompt_templates",
                PROMPT_TEMPLATES,
                "userId = ?1",
                &[&user_id],
            )?,
            roleplay_templates: query_all(
                main,
                "roleplay_templates",
                ROLEPLAY_TEMPLATES,
                "userId = ?1",
                &[&user_id],
            )?,
            provider_models: if_table(main, "provider_models", || provider_models::find_all(main))?,
            plugin_configs: query_all(
                main,
                "plugin_configs",
                PLUGIN_CONFIGS,
                "userId = ?1",
                &[&user_id],
            )?,
            chat_settings,
            folders: query_all(main, "folders", FOLDERS, "userId = ?1", &[&user_id])?,
            wardrobe_items: query_all(main, "wardrobe_items", WARDROBE_ITEMS, "", &[])?,
            character_plugin_data,
            conversation_annotations,
            chat_documents,
            instance_settings: read_instance_settings(main),
            embedding_status: query_all(main, "embedding_status", EMBEDDING_STATUS, "", &[])?,
            conversation_chunks,
            tfidf_vocabularies: query_all(main, "tfidf_vocabularies", TFIDF_VOCABULARIES, "", &[])?,
            vector_index_metas,
            vector_entries,
            text_replacement_rules: query_all(
                main,
                "text_replacement_rules",
                TEXT_REPLACEMENT_RULES,
                "",
                &[],
            )?,
        })
    })?;

    // v4 excludes prior backups from the file list (`:188`).
    let files: Vec<Value> = main_side
        .files
        .into_iter()
        .filter(|f| {
            f.get("category").and_then(Value::as_str) != Some("BACKUP")
                && f.get("folderPath").and_then(Value::as_str) != Some("/backups")
        })
        .collect();

    // The llm-logs partition is optional; without it v4's repo simply finds none.
    let llm_logs = db
        .read_llm_logs(|conn| {
            let list = select_list(conn, "llm_logs", LLM_LOGS)?;
            let mut stmt = conn.prepare(&format!(
                "SELECT {list} FROM \"llm_logs\" ORDER BY createdAt DESC LIMIT ?1"
            ))?;
            let rows = stmt.query_map([LLM_LOGS_LIMIT], |r| marshal(r, LLM_LOGS))?;
            let mut out = Vec::new();
            for r in rows {
                out.push(r?);
            }
            Ok(out)
        })
        .unwrap_or_default();

    let mount = db.read_mount_index(|conn| {
        let mut dumps: Vec<Vec<Value>> = MOUNT_DUMPS.iter().map(|t| dump_table(conn, t)).collect();
        let doc_mount_chunks = read_doc_mount_chunks(conn);
        let doc_mount_blobs = read_doc_mount_blob_metadata(conn);
        // Drain in MOUNT_DUMPS order.
        let group_character_members = dumps.pop().unwrap_or_default();
        let group_doc_mount_links = dumps.pop().unwrap_or_default();
        let project_doc_mount_links = dumps.pop().unwrap_or_default();
        let doc_mount_documents = dumps.pop().unwrap_or_default();
        let doc_mount_file_links = dumps.pop().unwrap_or_default();
        let doc_mount_files = dumps.pop().unwrap_or_default();
        let doc_mount_folders = dumps.pop().unwrap_or_default();
        let doc_mount_points = dumps.pop().unwrap_or_default();
        Ok(MountSide {
            doc_mount_points,
            doc_mount_folders,
            doc_mount_files,
            doc_mount_file_links,
            doc_mount_chunks,
            doc_mount_documents,
            doc_mount_blobs,
            project_doc_mount_links,
            group_doc_mount_links,
            group_character_members,
        })
    })?;

    Ok(BackupData {
        characters,
        chats: main_side.chats,
        tags: main_side.tags,
        connection_profiles: main_side.connection_profiles,
        image_profiles: main_side.image_profiles,
        embedding_profiles: main_side.embedding_profiles,
        memories: main_side.memories,
        files,
        prompt_templates: main_side.prompt_templates,
        roleplay_templates: main_side.roleplay_templates,
        provider_models: main_side.provider_models,
        projects,
        groups,
        llm_logs,
        plugin_configs: main_side.plugin_configs,
        chat_settings: main_side.chat_settings,
        folders: main_side.folders,
        wardrobe_items: main_side.wardrobe_items,
        character_plugin_data: main_side.character_plugin_data,
        conversation_annotations: main_side.conversation_annotations,
        chat_documents: main_side.chat_documents,
        instance_settings: main_side.instance_settings,
        embedding_status: main_side.embedding_status,
        conversation_chunks: main_side.conversation_chunks,
        tfidf_vocabularies: main_side.tfidf_vocabularies,
        vector_index_metas: main_side.vector_index_metas,
        vector_entries: main_side.vector_entries,
        doc_mount_points: mount.doc_mount_points,
        doc_mount_folders: mount.doc_mount_folders,
        doc_mount_files: mount.doc_mount_files,
        doc_mount_file_links: mount.doc_mount_file_links,
        doc_mount_chunks: mount.doc_mount_chunks,
        doc_mount_documents: mount.doc_mount_documents,
        doc_mount_blobs: mount.doc_mount_blobs,
        project_doc_mount_links: mount.project_doc_mount_links,
        group_doc_mount_links: mount.group_doc_mount_links,
        group_character_members: mount.group_character_members,
        text_replacement_rules: main_side.text_replacement_rules,
    })
}

struct MainSide {
    chats: Vec<Value>,
    tags: Vec<Value>,
    connection_profiles: Vec<Value>,
    image_profiles: Vec<Value>,
    embedding_profiles: Vec<Value>,
    memories: Vec<Value>,
    files: Vec<Value>,
    prompt_templates: Vec<Value>,
    roleplay_templates: Vec<Value>,
    provider_models: Vec<Value>,
    plugin_configs: Vec<Value>,
    chat_settings: Vec<Value>,
    folders: Vec<Value>,
    wardrobe_items: Vec<Value>,
    character_plugin_data: Vec<Value>,
    conversation_annotations: Vec<Value>,
    chat_documents: Vec<Value>,
    instance_settings: Vec<Value>,
    embedding_status: Vec<Value>,
    conversation_chunks: Vec<Value>,
    tfidf_vocabularies: Vec<Value>,
    vector_index_metas: Vec<Value>,
    vector_entries: Vec<Value>,
    text_replacement_rules: Vec<Value>,
}

struct MountSide {
    doc_mount_points: Vec<Value>,
    doc_mount_folders: Vec<Value>,
    doc_mount_files: Vec<Value>,
    doc_mount_file_links: Vec<Value>,
    doc_mount_chunks: Vec<Value>,
    doc_mount_documents: Vec<Value>,
    doc_mount_blobs: Vec<Value>,
    project_doc_mount_links: Vec<Value>,
    group_doc_mount_links: Vec<Value>,
    group_character_members: Vec<Value>,
}

fn collect_files(conn: &rusqlite::Connection) -> Result<Vec<Value>, DbError> {
    query_all(conn, "files", FILES, "", &[])
}

/// v4 `dumpInstanceSettings` (`:92`) — `SELECT "key","value" FROM
/// "instance_settings"`, `[]` when the table predates the provisioning migration.
fn read_instance_settings(conn: &rusqlite::Connection) -> Vec<Value> {
    let Ok(mut stmt) = conn.prepare("SELECT \"key\", \"value\" FROM \"instance_settings\"") else {
        return Vec::new();
    };
    let Ok(rows) = stmt.query_map([], |r| {
        let mut o = Map::new();
        o.insert("key".into(), Value::String(r.get::<_, String>(0)?));
        o.insert("value".into(), Value::String(r.get::<_, String>(1)?));
        Ok(Value::Object(o))
    }) else {
        return Vec::new();
    };
    rows.flatten().collect()
}

/// The `SerializedConversationChunk` shape v4 builds at `:236`.
fn read_conversation_chunks(
    conn: &rusqlite::Connection,
    chat_id: &str,
) -> Result<Vec<Value>, DbError> {
    if !table_exists(conn, "conversation_chunks") {
        return Ok(Vec::new());
    }
    let mut stmt = conn.prepare(
        "SELECT id, chatId, interchangeIndex, content, participantNames, messageIds, \
         embedding, createdAt, updatedAt FROM conversation_chunks WHERE chatId = ?1",
    )?;
    let rows = stmt.query_map([chat_id], |r| {
        let mut o = Map::new();
        o.insert("id".into(), Value::String(r.get::<_, String>(0)?));
        o.insert("chatId".into(), Value::String(r.get::<_, String>(1)?));
        o.insert(
            "interchangeIndex".into(),
            js_number_to_json(r.get::<_, f64>(2)?),
        );
        o.insert("content".into(), Value::String(r.get::<_, String>(3)?));
        o.insert("participantNames".into(), json_array_or_empty(r.get(4)?));
        o.insert("messageIds".into(), json_array_or_empty(r.get(5)?));
        o.insert("embedding".into(), encode_embedding(r.get(6)?));
        o.insert("createdAt".into(), Value::String(r.get::<_, String>(7)?));
        o.insert("updatedAt".into(), Value::String(r.get::<_, String>(8)?));
        Ok(Value::Object(o))
    })?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

/// The `SerializedVectorEntry` shape v4 builds at `:260`. `Array.from(entry
/// .embedding as Float32Array)` — never null here (the column is NOT NULL).
fn read_vector_entries(
    conn: &rusqlite::Connection,
    character_id: &str,
) -> Result<Vec<Value>, DbError> {
    if !table_exists(conn, "vector_entries") {
        return Ok(Vec::new());
    }
    let mut stmt = conn.prepare(
        "SELECT id, characterId, embedding, createdAt FROM vector_entries WHERE characterId = ?1",
    )?;
    let rows = stmt.query_map([character_id], |r| {
        let mut o = Map::new();
        o.insert("id".into(), Value::String(r.get::<_, String>(0)?));
        o.insert("characterId".into(), Value::String(r.get::<_, String>(1)?));
        o.insert("embedding".into(), encode_embedding(r.get(2)?));
        o.insert("createdAt".into(), Value::String(r.get::<_, String>(3)?));
        Ok(Value::Object(o))
    })?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

/// The `SerializedDocMountChunk` shape v4 builds at `:311` — the raw dump with
/// the BLOB `embedding` column re-encoded as a number array.
fn read_doc_mount_chunks(conn: &rusqlite::Connection) -> Vec<Value> {
    let Ok(mut stmt) = conn.prepare(
        "SELECT id, linkId, mountPointId, chunkIndex, content, tokenCount, \
         headingContext, embedding, createdAt, updatedAt FROM \"doc_mount_chunks\"",
    ) else {
        return Vec::new();
    };
    let Ok(rows) = stmt.query_map([], |r| {
        let mut o = Map::new();
        o.insert("id".into(), Value::String(r.get::<_, String>(0)?));
        o.insert("linkId".into(), Value::String(r.get::<_, String>(1)?));
        o.insert("mountPointId".into(), Value::String(r.get::<_, String>(2)?));
        o.insert("chunkIndex".into(), js_number_to_json(r.get::<_, f64>(3)?));
        o.insert("content".into(), Value::String(r.get::<_, String>(4)?));
        o.insert("tokenCount".into(), js_number_to_json(r.get::<_, f64>(5)?));
        o.insert(
            "headingContext".into(),
            match r.get::<_, Option<String>>(6)? {
                Some(s) => Value::String(s),
                None => Value::Null,
            },
        );
        o.insert("embedding".into(), encode_embedding(r.get(7)?));
        o.insert("createdAt".into(), Value::String(r.get::<_, String>(8)?));
        o.insert("updatedAt".into(), Value::String(r.get::<_, String>(9)?));
        Ok(Value::Object(o))
    }) else {
        return Vec::new();
    };
    rows.flatten().collect()
}

/// v4 `streamDocMountBlobsWithData` (`:112`) metadata half — every column except
/// the `data` BLOB, which is staged to disk instead.
fn read_doc_mount_blob_metadata(conn: &rusqlite::Connection) -> Vec<Value> {
    let Ok(mut stmt) = conn.prepare(
        "SELECT id, fileId, sha256, sizeBytes, storedMimeType, createdAt, updatedAt \
         FROM \"doc_mount_blobs\"",
    ) else {
        return Vec::new();
    };
    let Ok(rows) = stmt.query_map([], |r| {
        let mut o = Map::new();
        o.insert("id".into(), Value::String(r.get::<_, String>(0)?));
        o.insert("fileId".into(), Value::String(r.get::<_, String>(1)?));
        o.insert("sha256".into(), Value::String(r.get::<_, String>(2)?));
        o.insert("sizeBytes".into(), js_number_to_json(r.get::<_, f64>(3)?));
        o.insert(
            "storedMimeType".into(),
            Value::String(r.get::<_, String>(4)?),
        );
        o.insert("createdAt".into(), Value::String(r.get::<_, String>(5)?));
        o.insert("updatedAt".into(), Value::String(r.get::<_, String>(6)?));
        Ok(Value::Object(o))
    }) else {
        return Vec::new();
    };
    rows.flatten().collect()
}

/// Read a doc-mount blob's bytes by id (the staging leg reads them one at a
/// time so a big archive never holds more than one blob in memory).
pub(super) fn read_doc_mount_blob_bytes(
    conn: &rusqlite::Connection,
    blob_id: &str,
) -> Option<Vec<u8>> {
    conn.query_row(
        "SELECT data FROM \"doc_mount_blobs\" WHERE id = ?1",
        [blob_id],
        |r| r.get::<_, Vec<u8>>(0),
    )
    .ok()
}

fn json_array_or_empty(cell: Option<String>) -> Value {
    cell.and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .filter(Value::is_array)
        .unwrap_or_else(|| Value::Array(Vec::new()))
}
