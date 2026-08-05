//! The Almanack — Phase 5, "Assembling the dramatis personae"
//! (v4 `lib/tools/almanack/phase5-personae.ts`).
//!
//! Who has actually been on stage: the ten busiest characters, every project,
//! every group and its cast.
//!
//! The per-character chat count is one pass over `chats` with `json_each` over
//! the participants array. There is no participants join table, so the
//! alternative is a full scan per character — which is what v4's old report did
//! and why it crawled.
//!
//! Two of the joins here cross databases (member names live in the main DB while
//! memberships live in the mount index; vault sizes likewise), so they are done
//! in application code after one query per side.

use std::collections::HashMap;

use rusqlite::types::Value as SqlValue;
use rusqlite::ToSql;

use crate::collation::locale_compare;
use crate::db::runtime::Db;
use crate::db::DbError;

use super::db::{main_rows, mount_rows, num_col, opt_text, text};
use super::types::{GroupRow, PersonaeInfo, ProjectRow, TopCharacterRow};

/// How many characters the top table lists.
const TOP_CHARACTER_COUNT: usize = 10;

/// Chat kinds excluded from the "chats participated in" count.
///
/// Help chats and the Brahma Console are machinery, not drama — counting them
/// would put whichever character happens to answer help questions at the top of
/// a table that is meant to show who the household actually plays with.
const EXCLUDED_CHAT_TYPES: [&str; 2] = ["help", "brahma"];

/// Whether a participant entry with `status: 'removed'` still counts toward a
/// character's chat total.
///
/// `true` — a character who was written out of a scene was still in it. The
/// rendered section footnotes the choice.
const COUNT_REMOVED_PARTICIPANTS: bool = true;

fn sql_values(ids: &[String]) -> Vec<SqlValue> {
    ids.iter().map(|v| SqlValue::Text(v.clone())).collect()
}

fn as_params(values: &[SqlValue]) -> Vec<&dyn ToSql> {
    values.iter().map(|v| v as &dyn ToSql).collect()
}

fn placeholders(n: usize) -> String {
    vec!["?"; n].join(", ")
}

/// v4 `collectChatCountsByCharacter` — chats participated in, per character id,
/// in one pass over the whole table.
fn collect_chat_counts_by_character(db: &Db, user_id: &str) -> HashMap<String, f64> {
    let status_clause = if COUNT_REMOVED_PARTICIPANTS {
        ""
    } else {
        r#"AND COALESCE(json_extract(p.value, '$.status'), 'active') != 'removed'"#
    };
    let sql = format!(
        r#"SELECT json_extract(p.value, '$.characterId') AS characterId,
            COUNT(DISTINCT c."id")                 AS chats
     FROM "chats" c, json_each(c."participants") p
     WHERE c."userId" = ?
       AND COALESCE(c."chatType", 'salon') NOT IN ({})
       AND json_valid(c."participants")
       {status_clause}
     GROUP BY characterId"#,
        placeholders(EXCLUDED_CHAT_TYPES.len())
    );

    let mut binds = vec![SqlValue::Text(user_id.to_string())];
    binds.extend(EXCLUDED_CHAT_TYPES.iter().map(|t| SqlValue::Text((*t).to_string())));

    let rows: Vec<(Option<String>, f64)> = main_rows(
        db,
        &sql,
        &as_params(&binds),
        "personae.chatCountsByCharacter",
        |r| Ok((opt_text(r, 0)?, num_col(r, 1)?)),
    );

    let mut counts = HashMap::new();
    for (id, chats) in rows {
        // v4's `if (row.characterId)` — a null OR empty id is dropped.
        if let Some(id) = id.filter(|s| !s.is_empty()) {
            counts.insert(id, chats);
        }
    }
    counts
}

/// v4 `collectTopCharacters` — the ten busiest.
///
/// Ranked by chat count, ties broken by memories. Storage is the character
/// vault's cached `totalSizeBytes`, refreshed by scans rather than measured here
/// — cheap, and close enough for a diagnostic. Because mount content is
/// content-addressed and hard-linkable across vaults, bytes shared between two
/// characters are counted for both; the rendered section says so.
pub fn collect_top_characters(
    db: &Db,
    user_id: &str,
    memories_by_character: &HashMap<String, f64>,
) -> Vec<TopCharacterRow> {
    let characters: Vec<(String, String, Option<String>)> = main_rows(
        db,
        r#"SELECT "id", "name", "characterDocumentMountPointId" AS vaultId
       FROM "characters" WHERE "userId" = ?"#,
        &[&user_id],
        "personae.characters",
        |r| Ok((text(r, 0)?, text(r, 1)?, opt_text(r, 2)?)),
    );
    let chat_counts = collect_chat_counts_by_character(db, user_id);

    // Vault sizes live in the other database, so they are fetched in one go and
    // joined here rather than per character.
    let vault_ids: Vec<String> = characters.iter().filter_map(|c| c.2.clone()).collect();
    let mut vault_sizes: HashMap<String, f64> = HashMap::new();
    if !vault_ids.is_empty() {
        let binds = sql_values(&vault_ids);
        let sql = format!(
            r#"SELECT "id", "totalSizeBytes" FROM "doc_mount_points"
       WHERE "id" IN ({})"#,
            placeholders(vault_ids.len())
        );
        for (id, size) in mount_rows(
            db,
            &sql,
            &as_params(&binds),
            "personae.vaultSizes",
            |r| Ok((text(r, 0)?, num_col(r, 1)?)),
        ) {
            vault_sizes.insert(id, size);
        }
    }

    let mut rows: Vec<TopCharacterRow> = characters
        .iter()
        .map(|(id, name, vault_id)| TopCharacterRow {
            name: name.clone(),
            chats: chat_counts.get(id).copied().unwrap_or(0.0),
            memories: memories_by_character.get(id).copied().unwrap_or(0.0),
            storage_bytes: vault_id
                .as_ref()
                .and_then(|v| vault_sizes.get(v).copied())
                .unwrap_or(0.0),
        })
        .collect();

    // v4's comparator chain — chats DESC, memories DESC, bytes DESC, name ASC.
    // `sort` in V8 is stable, and so is `sort_by` here, so the tail order of
    // fully-tied rows agrees.
    rows.sort_by(|a, b| {
        cmp_desc(b.chats, a.chats)
            .then_with(|| cmp_desc(b.memories, a.memories))
            .then_with(|| cmp_desc(b.storage_bytes, a.storage_bytes))
            .then_with(|| locale_compare(&a.name, &b.name))
    });
    rows.truncate(TOP_CHARACTER_COUNT);
    rows
}

/// v4's `b - a` numeric comparator, expressed as an `Ordering` (the values are
/// finite counts, so the `partial_cmp` fallback is unreachable).
fn cmp_desc(a: f64, b: f64) -> std::cmp::Ordering {
    a.partial_cmp(&b).unwrap_or(std::cmp::Ordering::Equal)
}

/// v4 `collectProjects` — every project, with what is attached to it.
pub fn collect_projects(db: &Db) -> Vec<ProjectRow> {
    let projects: Vec<(String, String, Option<String>)> = main_rows(
        db,
        r#"SELECT "id", "name", "officialMountPointId" FROM "projects" ORDER BY "name""#,
        &[],
        "personae.projects",
        |r| Ok((text(r, 0)?, text(r, 1)?, opt_text(r, 2)?)),
    );
    if projects.is_empty() {
        return Vec::new();
    }

    let chats_by_project: HashMap<String, f64> = main_rows(
        db,
        r#"SELECT "projectId", COUNT(*) AS n FROM "chats"
       WHERE "projectId" IS NOT NULL GROUP BY "projectId""#,
        &[],
        "personae.projectChats",
        |r| Ok((text(r, 0)?, num_col(r, 1)?)),
    )
    .into_iter()
    .collect();

    let files_by_project: HashMap<String, f64> = main_rows(
        db,
        r#"SELECT "projectId", COUNT(*) AS n FROM "files"
       WHERE "projectId" IS NOT NULL GROUP BY "projectId""#,
        &[],
        "personae.projectFiles",
        |r| Ok((text(r, 0)?, num_col(r, 1)?)),
    )
    .into_iter()
    .collect();

    let project_ids: Vec<String> = projects.iter().map(|p| p.0.clone()).collect();
    let id_binds = sql_values(&project_ids);
    let stores_by_project: HashMap<String, f64> = mount_rows(
        db,
        &format!(
            r#"SELECT "projectId", COUNT(*) AS n FROM "project_doc_mount_links"
     WHERE "projectId" IN ({})
     GROUP BY "projectId""#,
            placeholders(project_ids.len())
        ),
        &as_params(&id_binds),
        "personae.projectStoreLinks",
        |r| Ok((text(r, 0)?, num_col(r, 1)?)),
    )
    .into_iter()
    .collect();

    let mount_ids: Vec<String> = projects.iter().filter_map(|p| p.2.clone()).collect();
    let mut document_counts: HashMap<String, f64> = HashMap::new();
    let mut stateful_mounts: Vec<String> = Vec::new();
    if !mount_ids.is_empty() {
        let binds = sql_values(&mount_ids);
        let ph = placeholders(mount_ids.len());
        for (mount_id, n) in mount_rows(
            db,
            &format!(
                r#"SELECT "mountPointId", COUNT(*) AS n FROM "doc_mount_file_links"
       WHERE "mountPointId" IN ({ph}) GROUP BY "mountPointId""#
            ),
            &as_params(&binds),
            "personae.projectDocuments",
            |r| Ok((text(r, 0)?, num_col(r, 1)?)),
        ) {
            document_counts.insert(mount_id, n);
        }
        for mount_id in mount_rows(
            db,
            &format!(
                r#"SELECT DISTINCT l."mountPointId" AS mountPointId
       FROM "doc_mount_file_links" l
       JOIN "doc_mount_documents" d ON d."fileId" = l."fileId"
       WHERE l."mountPointId" IN ({ph})
         AND lower(l."relativePath") = 'state.json'
         AND trim(d."content") NOT IN ('', '{{}}')"#
            ),
            &as_params(&binds),
            "personae.projectState",
            |r| text(r, 0),
        ) {
            stateful_mounts.push(mount_id);
        }
    }

    projects
        .iter()
        .map(|(id, name, mount_id)| ProjectRow {
            name: name.clone(),
            linked_stores: stores_by_project.get(id).copied().unwrap_or(0.0),
            chats: chats_by_project.get(id).copied().unwrap_or(0.0),
            files: files_by_project.get(id).copied().unwrap_or(0.0),
            documents: mount_id
                .as_ref()
                .and_then(|m| document_counts.get(m).copied())
                .unwrap_or(0.0),
            has_state: mount_id
                .as_ref()
                .is_some_and(|m| stateful_mounts.contains(m)),
        })
        .collect()
}

/// v4 `collectGroups` — every group, with its cast by name.
///
/// A group with no `officialMountPointId` never got its store provisioned — a
/// real gap, so it gets a column rather than being inferred from a blank.
pub fn collect_groups(db: &Db) -> Vec<GroupRow> {
    let groups: Vec<(String, String, Option<String>)> = main_rows(
        db,
        r#"SELECT "id", "name", "officialMountPointId" FROM "groups" ORDER BY "name""#,
        &[],
        "personae.groups",
        |r| Ok((text(r, 0)?, text(r, 1)?, opt_text(r, 2)?)),
    );
    if groups.is_empty() {
        return Vec::new();
    }

    let group_ids: Vec<String> = groups.iter().map(|g| g.0.clone()).collect();
    let binds = sql_values(&group_ids);
    let ph = placeholders(group_ids.len());

    // Membership and store links live in the mount index; the character NAMES
    // live in the main DB, so the two halves are joined in application code.
    let member_rows: Vec<(String, String)> = mount_rows(
        db,
        &format!(
            r#"SELECT "groupId", "characterId" FROM "group_character_members"
     WHERE "groupId" IN ({ph})"#
        ),
        &as_params(&binds),
        "personae.groupMembers",
        |r| Ok((text(r, 0)?, text(r, 1)?)),
    );
    let stores_by_group: HashMap<String, f64> = mount_rows(
        db,
        &format!(
            r#"SELECT "groupId", COUNT(*) AS n FROM "group_doc_mount_links"
     WHERE "groupId" IN ({ph}) GROUP BY "groupId""#
        ),
        &as_params(&binds),
        "personae.groupStoreLinks",
        |r| Ok((text(r, 0)?, num_col(r, 1)?)),
    )
    .into_iter()
    .collect();

    let mut character_ids: Vec<String> = Vec::new();
    for (_, cid) in &member_rows {
        if !character_ids.contains(cid) {
            character_ids.push(cid.clone());
        }
    }
    let mut names_by_id: HashMap<String, String> = HashMap::new();
    if !character_ids.is_empty() {
        let cbinds = sql_values(&character_ids);
        for (id, name) in main_rows(
            db,
            &format!(
                r#"SELECT "id", "name" FROM "characters"
       WHERE "id" IN ({})"#,
                placeholders(character_ids.len())
            ),
            &as_params(&cbinds),
            "personae.groupMemberNames",
            |r| Ok((text(r, 0)?, text(r, 1)?)),
        ) {
            names_by_id.insert(id, name);
        }
    }

    let mut members_by_group: HashMap<String, Vec<String>> = HashMap::new();
    for (group_id, character_id) in &member_rows {
        // A membership row pointing at a character that no longer exists is a
        // dangling cross-database reference — shown as such rather than dropped.
        let name = names_by_id
            .get(character_id)
            .cloned()
            .unwrap_or_else(|| "(missing character)".to_string());
        members_by_group.entry(group_id.clone()).or_default().push(name);
    }

    groups
        .iter()
        .map(|(id, name, mount_id)| {
            let mut members = members_by_group.get(id).cloned().unwrap_or_default();
            members.sort_by(|a, b| locale_compare(a, b));
            GroupRow {
                name: name.clone(),
                members,
                linked_stores: stores_by_group.get(id).copied().unwrap_or(0.0),
                has_official_store: mount_id.as_ref().is_some_and(|m| !m.is_empty()),
            }
        })
        .collect()
}

/// v4 `collectPersonae` — the whole cast list.
pub fn collect_personae(
    db: &Db,
    user_id: &str,
    memories_by_character: &HashMap<String, f64>,
) -> Result<PersonaeInfo, DbError> {
    Ok(PersonaeInfo {
        top_characters: collect_top_characters(db, user_id, memories_by_character),
        counts_removed_participants: COUNT_REMOVED_PARTICIPANTS,
        projects: collect_projects(db),
        groups: collect_groups(db),
    })
}
