//! Conversation-summary vault bridge — v4
//! `lib/file-storage/conversation-summary-vault-bridge.ts`.
//!
//! Mirrors a conversation's rolling context summary into every participant
//! character's vault as a managed markdown file under `Conversation Summaries/`
//! (so past conversations become per-character retrievable — the documents are
//! chunked + embedded). The file carries YAML frontmatter — most importantly the
//! conversation UUID (`conversationId`) — so each regeneration can find-and-replace
//! its own prior file even after a rename, and so deletion of the conversation can
//! sweep the matching file out of every vault.
//!
//! ## Composition
//! Spans both databases: the participant cast + names come from the MAIN-db
//! `characters` table (via [`crate::db::characters_read::find_by_id_raw`] — the raw
//! read survives a broken vault), while the store lives in the mount-index db (the
//! ported `crate::db::database_store` / `crate::db::doc_mount_file_links` write
//! primitives + `crate::markdown::parse_frontmatter` +
//! `crate::doc_edit::markdown_parser::serialize_frontmatter`). Each per-character
//! write runs in ONE mount-index write closure (delete-priors → collision check →
//! write), because the desired-name collision check must see the state after the
//! prior deletes (v4's ordering).
//!
//! ## Best-effort
//! Every path is best-effort per character (one bad vault never aborts the rest,
//! and a failure never propagates — the summary is already persisted on the chat
//! row). The v4 `QUILLTAP_JOB_CHILD` host-RPC short-circuit does NOT port (the v5
//! runtime has no forked write-buffer hazard — every mutation goes through the
//! single writer channel).
//!
//! This closes the LAST Phase-2 deferral — the `chats.delete` participant-vault
//! summary sweep (`removeConversationSummariesFromVaults`, composed here as
//! [`delete_conversation_with_vault_sweep`]).

use serde_json::{Map, Value};

use crate::db::characters_read;
use crate::db::database_store::{
    database_document_exists, list_database_files, read_database_document,
};
use crate::db::doc_mount_file_links::DocMountFileLinksRepository;
use crate::db::runtime::Db;
use crate::db::DbError;
use crate::doc_edit::markdown_parser::serialize_frontmatter;
use crate::markdown::parse_frontmatter;

/// Folder, inside each character vault, that holds conversation summaries.
pub const SUMMARIES_FOLDER: &str = "Conversation Summaries";

/// Frontmatter `type` marker stamped on every summary file we author.
pub const SUMMARY_FRONTMATTER_TYPE: &str = "conversation-summary";

/// v4 `isConversationalMessage`: a "real" conversational message — a `type:'message'`
/// USER/ASSISTANT event that is not a `systemSender` announcement, not a
/// `customAnnouncer` bubble, and not a whisper (`targetParticipantIds` non-empty).
pub fn is_conversational_message(event: &Value) -> bool {
    if event.get("type").and_then(Value::as_str) != Some("message") {
        return false;
    }
    let role = event.get("role").and_then(Value::as_str);
    if role != Some("USER") && role != Some("ASSISTANT") {
        return false;
    }
    // A present `systemSender` (any non-null value) disqualifies.
    if event
        .get("systemSender")
        .map(|v| !v.is_null())
        .unwrap_or(false)
    {
        return false;
    }
    if event
        .get("customAnnouncer")
        .map(|v| !v.is_null())
        .unwrap_or(false)
    {
        return false;
    }
    if let Some(arr) = event.get("targetParticipantIds").and_then(Value::as_array) {
        if !arr.is_empty() {
            return false;
        }
    }
    true
}

/// v4 `SummaryConversationStats`.
#[derive(Clone, Debug, PartialEq)]
pub struct SummaryConversationStats {
    pub message_count: i64,
    pub first_message_at: Option<String>,
    pub last_message_at: Option<String>,
}

/// v4 `computeConversationStats`: real-message count + first/last `createdAt`
/// (`events` in ascending `createdAt` order, as `getMessages` returns).
pub fn compute_conversation_stats(events: &[Value]) -> SummaryConversationStats {
    let mut message_count = 0i64;
    let mut first_message_at: Option<String> = None;
    let mut last_message_at: Option<String> = None;
    for event in events {
        if !is_conversational_message(event) {
            continue;
        }
        message_count += 1;
        let created = event
            .get("createdAt")
            .and_then(Value::as_str)
            .map(str::to_string);
        if first_message_at.is_none() {
            first_message_at = created.clone();
        }
        last_message_at = created;
    }
    SummaryConversationStats {
        message_count,
        first_message_at,
        last_message_at,
    }
}

/// v4 `WriteConversationSummaryInput`.
#[derive(Clone, Debug)]
pub struct WriteConversationSummaryInput {
    pub chat_id: String,
    pub chat_title: String,
    pub summary: String,
    /// `chat.compactionGeneration` after the fold that produced this summary.
    pub summary_generation: i64,
    /// All participant character ids (both llm- and user-controlled).
    pub participant_character_ids: Vec<String>,
    pub message_count: i64,
    pub first_message_at: Option<String>,
    pub last_message_at: Option<String>,
    /// ISO timestamp stamped into frontmatter as `updatedAt`.
    pub updated_at: String,
}

/// v4 `sanitizeLeafName` (`lib/file-storage/bridge-path-helpers.ts`): strip the
/// unsafe leaf character set (`/[\/\\:*?"<>|\x00-\x1f\x7f]/g`), collapse `__+` →
/// `_`, trim leading/trailing `_`/`.`, `'unnamed'` when empty.
fn sanitize_leaf_name(filename: &str) -> String {
    let basename = filename.rsplit(['\\', '/']).next().unwrap_or(filename);
    let mut safe: String = basename
        .chars()
        .map(|c| {
            if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|')
                || (c as u32) <= 0x1f
                || (c as u32) == 0x7f
            {
                '_'
            } else {
                c
            }
        })
        .collect();
    while safe.contains("__") {
        safe = safe.replace("__", "_");
    }
    let trimmed = safe.trim_matches(|c| c == '_' || c == '.');
    if trimmed.is_empty() {
        "unnamed".to_string()
    } else {
        trimmed.to_string()
    }
}

/// v4 `summaryFileName`: `sanitizeLeafName(title)`; when empty/`unnamed`, fall
/// back to `conversation-<chatId>`.
fn summary_file_name(chat_id: &str, chat_title: &str) -> String {
    let stem = sanitize_leaf_name(chat_title);
    if !stem.is_empty() && stem != "unnamed" {
        format!("{stem}.md")
    } else {
        format!("conversation-{chat_id}.md")
    }
}

/// v4 `buildSummaryFile`: frontmatter (fixed key order) + the trimmed summary.
fn build_summary_file(input: &WriteConversationSummaryInput, cast: &[(String, String)]) -> String {
    // Insertion order matters (serde_json Map is preserve_order): reproduce v4's
    // exact key sequence.
    let mut data = Map::new();
    data.insert("type".to_string(), Value::from(SUMMARY_FRONTMATTER_TYPE));
    data.insert(
        "conversationId".to_string(),
        Value::from(input.chat_id.clone()),
    );
    data.insert(
        "conversationTitle".to_string(),
        Value::from(input.chat_title.clone()),
    );
    data.insert(
        "characters".to_string(),
        Value::from(cast.iter().map(|c| c.1.clone()).collect::<Vec<_>>()),
    );
    data.insert(
        "characterIds".to_string(),
        Value::from(cast.iter().map(|c| c.0.clone()).collect::<Vec<_>>()),
    );
    data.insert("messageCount".to_string(), Value::from(input.message_count));
    data.insert(
        "firstMessageAt".to_string(),
        input
            .first_message_at
            .clone()
            .map(Value::from)
            .unwrap_or(Value::Null),
    );
    data.insert(
        "lastMessageAt".to_string(),
        input
            .last_message_at
            .clone()
            .map(Value::from)
            .unwrap_or(Value::Null),
    );
    data.insert(
        "summaryGeneration".to_string(),
        Value::from(input.summary_generation),
    );
    data.insert(
        "updatedAt".to_string(),
        Value::from(input.updated_at.clone()),
    );
    let frontmatter = serialize_frontmatter(&data);
    // v4 `${frontmatter}\n${summary.trim()}\n`.
    format!("{frontmatter}\n{}\n", input.summary.trim())
}

/// v4 `resolveCast`: `{ id, name }` for each character via the raw read (survives a
/// broken vault; a missing/failed read is skipped).
async fn resolve_cast(db: &Db, character_ids: &[String]) -> Vec<(String, String)> {
    let mut cast = Vec::new();
    for id in character_ids {
        let id_owned = id.clone();
        let row = db
            .read_main(move |conn| characters_read::find_by_id_raw(conn, &id_owned))
            .ok()
            .flatten();
        if let Some(row) = row {
            if let Some(name) = row.get("name").and_then(Value::as_str) {
                cast.push((id.clone(), name.to_string()));
            }
        }
    }
    cast
}

/// The character's vault mount-point id (v4 `getCharacterVaultStore` → `mountPointId`).
async fn resolve_vault_mount_id(db: &Db, character_id: &str) -> Option<String> {
    let id_owned = character_id.to_string();
    let row = db
        .read_main(move |conn| characters_read::find_by_id_raw(conn, &id_owned))
        .ok()
        .flatten()?;
    row.get("characterDocumentMountPointId")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// v4 `findExistingSummaryPaths`: the `.md` files under `Conversation Summaries/`
/// whose frontmatter `conversationId` matches `chatId`. Unreadable/garbled files
/// are left alone (v4's per-file try/catch). Runs on a mount-index connection.
fn find_existing_summary_paths(
    conn: &rusqlite::Connection,
    mount_point_id: &str,
    chat_id: &str,
) -> Result<Vec<String>, DbError> {
    let entries = list_database_files(conn, mount_point_id, Some(SUMMARIES_FOLDER))?;
    let mut matches = Vec::new();
    for entry in entries {
        if entry.kind == "folder" {
            continue;
        }
        if !entry.relative_path.to_lowercase().ends_with(".md") {
            continue;
        }
        // Unreadable file → skip (v4's catch).
        let content = match read_database_document(conn, mount_point_id, &entry.relative_path) {
            Ok(doc) => doc.content,
            Err(_) => continue,
        };
        let parsed = parse_frontmatter(&content);
        if let Some(data) = parsed.data {
            if data.get("conversationId").and_then(Value::as_str) == Some(chat_id) {
                matches.push(entry.relative_path);
            }
        }
    }
    Ok(matches)
}

/// v4 `writeConversationSummaryToVaults`: write (or replace) the conversation's
/// summary file in every participant character's vault. Best-effort per character.
pub async fn write_conversation_summary_to_vaults(db: &Db, input: &WriteConversationSummaryInput) {
    let cast = resolve_cast(db, &input.participant_character_ids).await;
    let body = build_summary_file(input, &cast);
    let desired_name = summary_file_name(&input.chat_id, &input.chat_title);

    for character_id in &input.participant_character_ids {
        let Some(mount_id) = resolve_vault_mount_id(db, character_id).await else {
            continue;
        };
        let chat_id = input.chat_id.clone();
        let body = body.clone();
        let desired_name = desired_name.clone();
        // One mount-index write closure: ensure folder → delete priors → collision
        // check (sees post-delete state) → write.
        let _ = db
            .write(move |writers| {
                let Some(mi) = writers.mount_index() else {
                    return Ok(());
                };
                let conn = mi.connection();
                let links = DocMountFileLinksRepository::new(conn);
                links.ensure_folder_path(&mount_id, SUMMARIES_FOLDER)?;

                let prior = find_existing_summary_paths(conn, &mount_id, &chat_id)?;
                for prior_path in &prior {
                    links.delete_database_document(&mount_id, prior_path)?;
                }

                // Cross-conversation collision guard: disambiguate with the short id.
                let mut relative_path = format!("{SUMMARIES_FOLDER}/{desired_name}");
                if database_document_exists(conn, &mount_id, &relative_path)? {
                    let stem = desired_name
                        .strip_suffix(".md")
                        .or_else(|| desired_name.strip_suffix(".MD"))
                        .unwrap_or(&desired_name);
                    let short: String = chat_id.chars().take(8).collect();
                    relative_path = format!("{SUMMARIES_FOLDER}/{stem} ({short}).md");
                }

                links.write_database_document(&mount_id, &relative_path, &body)?;
                Ok(())
            })
            .await;
    }
}

/// v4 `removeConversationSummariesFromVaults`: remove the conversation's summary
/// file from every participant character's vault (matched by frontmatter
/// `conversationId`). Best-effort per character.
pub async fn remove_conversation_summaries_from_vaults(
    db: &Db,
    chat_id: &str,
    participant_character_ids: &[String],
) {
    for character_id in participant_character_ids {
        let Some(mount_id) = resolve_vault_mount_id(db, character_id).await else {
            continue;
        };
        let chat_id = chat_id.to_string();
        let _ = db
            .write(move |writers| {
                let Some(mi) = writers.mount_index() else {
                    return Ok(());
                };
                let conn = mi.connection();
                let links = DocMountFileLinksRepository::new(conn);
                let paths = find_existing_summary_paths(conn, &mount_id, &chat_id)?;
                for path in &paths {
                    links.delete_database_document(&mount_id, path)?;
                }
                Ok(())
            })
            .await;
    }
}

/// v4 `ChatsRepository.delete(id, { syncVaults })`: capture participants BEFORE
/// the row is gone, delete the chat row + its messages, then (when `sync_vaults`)
/// sweep the conversation's summary file out of each participant vault. Returns
/// whether the chat row was deleted. This closes the last Phase-2 deferral (the
/// `chats.delete` participant-vault sweep).
pub async fn delete_conversation_with_vault_sweep(
    db: &Db,
    chat_id: &str,
    sync_vaults: bool,
) -> Result<bool, DbError> {
    // Capture participants first (read fails closed — no participants → no cleanup).
    let mut participant_character_ids: Vec<String> = Vec::new();
    if sync_vaults {
        let id_owned = chat_id.to_string();
        if let Ok(Some(chat)) =
            db.read_main(move |conn| crate::db::chats_read::find_by_id(conn, &id_owned))
        {
            // Dedupe, preserving first-seen order (v4 `Array.from(new Set(...))`).
            let mut seen = std::collections::HashSet::new();
            if let Some(parts) = chat.get("participants").and_then(Value::as_array) {
                for p in parts {
                    if let Some(cid) = p.get("characterId").and_then(Value::as_str) {
                        if seen.insert(cid.to_string()) {
                            participant_character_ids.push(cid.to_string());
                        }
                    }
                }
            }
        }
    }

    // Delete the slim chat row (v4 `_delete`).
    let chat_id_owned = chat_id.to_string();
    let deleted = db
        .write(move |writers| writers.main().chats().delete(&chat_id_owned))
        .await?;
    if !deleted {
        return Ok(false);
    }

    // Delete the chat's messages (best-effort — v4 warns and continues).
    let chat_id_owned = chat_id.to_string();
    let _ = db
        .write(move |writers| {
            writers
                .main()
                .chat_messages()
                .clear_messages(&chat_id_owned)
        })
        .await;

    // Sweep summaries out of each participant vault (best-effort).
    if sync_vaults && !participant_character_ids.is_empty() {
        remove_conversation_summaries_from_vaults(db, chat_id, &participant_character_ids).await;
    }

    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn conversational_message_filter() {
        assert!(is_conversational_message(&json!({
            "type": "message", "role": "USER", "content": "hi", "createdAt": "t"
        })));
        assert!(is_conversational_message(&json!({
            "type": "message", "role": "ASSISTANT", "content": "hi", "createdAt": "t"
        })));
        // A whisper (targeted).
        assert!(!is_conversational_message(&json!({
            "type": "message", "role": "ASSISTANT", "targetParticipantIds": ["p1"]
        })));
        // Empty target array is fine.
        assert!(is_conversational_message(&json!({
            "type": "message", "role": "USER", "targetParticipantIds": []
        })));
        // A Staff announcement.
        assert!(!is_conversational_message(&json!({
            "type": "message", "role": "ASSISTANT", "systemSender": "host"
        })));
        // A customAnnouncer bubble.
        assert!(!is_conversational_message(&json!({
            "type": "message", "role": "USER", "customAnnouncer": {"kind": "x"}
        })));
        // Non-message / non-USER/ASSISTANT.
        assert!(!is_conversational_message(&json!({ "type": "system" })));
        assert!(!is_conversational_message(&json!({
            "type": "message", "role": "TOOL"
        })));
    }

    #[test]
    fn stats_first_and_last() {
        let events = vec![
            json!({ "type": "message", "role": "USER", "createdAt": "1" }),
            json!({ "type": "message", "role": "ASSISTANT", "systemSender": "host", "createdAt": "2" }),
            json!({ "type": "message", "role": "ASSISTANT", "createdAt": "3" }),
            json!({ "type": "system", "createdAt": "4" }),
        ];
        let stats = compute_conversation_stats(&events);
        assert_eq!(stats.message_count, 2);
        assert_eq!(stats.first_message_at.as_deref(), Some("1"));
        assert_eq!(stats.last_message_at.as_deref(), Some("3"));
    }

    #[test]
    fn file_name_fallbacks() {
        assert_eq!(summary_file_name("abc12345", "My Chat"), "My Chat.md");
        assert_eq!(
            summary_file_name("abc12345", "///"),
            "conversation-abc12345.md"
        );
        assert_eq!(
            summary_file_name("abc12345", ""),
            "conversation-abc12345.md"
        );
    }

    #[test]
    fn sanitize_leaf_matches_v4() {
        // v4 takes the basename after the last slash first, so "a/" is dropped.
        assert_eq!(sanitize_leaf_name("a/b:c*d"), "b_c_d");
        assert_eq!(sanitize_leaf_name("__lead__"), "lead");
        assert_eq!(sanitize_leaf_name("///"), "unnamed");
    }
}
