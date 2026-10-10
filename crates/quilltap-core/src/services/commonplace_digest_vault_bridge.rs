//! Commonplace Digest Vault Bridge — v4
//! `lib/file-storage/commonplace-digest-vault-bridge.ts` (`d58548051`,
//! byte-identical at the `01a83539d` pin).
//!
//! Mirrors a character's hot memory digests into their own vault after a
//! consolidation run (memory-consolidation-and-tiers.md §C6): one file per
//! subject bucket, `Commonplace/<Subject Name>.md` and `Commonplace/Self.md`,
//! rewritten whole each time. The folder is separate from the hand-written
//! `Others/` folder, so no user prose is ever touched. The extractor reads the
//! files back as canon (`canon.rs`).
//!
//! v4's `QUILLTAP_JOB_CHILD` host-RPC arm is NO-PORT by architecture (v5 has no
//! child; the in-job run writes here directly, after its main-partition commit).
//!
//! Archived characters are tombstones: their vault stays live and writable, so
//! this bridge checks the raw row's `archivedAt` and skips them — it never
//! resolves or creates a vault for an archived character.
//!
//! **The holder's vault (P4.D271 R-I, as measured).** v4's
//! `getCharacterVaultStore` is the OVERLAY character read (a dangling vault
//! throws `… has no usable vault …` → `Failed to resolve character vault store`
//! WARN, `context: 'file-storage.character-vault-bridge'`) then the
//! `mountType = 'database'` + `storeType = 'character'` predicate (a mismatch
//! is a silent `null`). R-I planned to compose the conversation-summary
//! bridge's raw-pointer resolver for the WARN half; measured at the pin, that
//! resolver's WARN carries `character_id` and no `context`, and its error text
//! is v5's own — so the bridge resolves through the overlay itself
//! ([`holder_vault_mount_id`]) and emits v4's keys and v4's error text. The
//! summary bridge's file is untouched.
//!
//! Every write is its own mount-index `Db::write` (the folder once, then one per
//! file), so the per-file WARN and every debug line run on the caller's thread.
//! v4's `context:` field rides each line as a key.

use serde_json::{json, Value};

use crate::commonplace_file::{
    commonplace_path_for, render_commonplace_file, CommonplaceDigestEntry, CommonplaceFileInput,
    CommonplaceSubject, COMMONPLACE_FOLDER,
};
use crate::db::database_store::{
    read_database_document, write_database_document, DbStoreErrorCode, StoreError,
};
use crate::db::doc_mount_file_links::DocMountFileLinksRepository;
use crate::db::doc_mount_points::DocMountPointsRepository;
use crate::db::runtime::Db;
use crate::db::{characters_read, DbError};

const CONTEXT: &str = "file-storage.commonplace-digest-vault-bridge";

/// One subject bucket's mirror (v4 `CommonplaceMirrorFile`).
#[derive(Debug, Clone, PartialEq)]
pub struct CommonplaceMirrorFile {
    /// The subject's character id; the holder's own id when `is_self`.
    pub subject_character_id: String,
    pub subject_name: String,
    pub is_self: bool,
    /// Every hot digest in the bucket after the run (the render orders and
    /// trims).
    pub digests: Vec<CommonplaceDigestEntry>,
}

/// v4 `WriteCommonplaceDigestsInput`.
#[derive(Debug, Clone, PartialEq)]
pub struct WriteCommonplaceDigestsInput {
    pub holder_character_id: String,
    pub files: Vec<CommonplaceMirrorFile>,
    /// ISO timestamp stamped into each file's frontmatter.
    pub updated_at: String,
}

/// v4 `WriteCommonplaceDigestsResult`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct WriteCommonplaceDigestsResult {
    pub written: usize,
    pub unchanged: usize,
    pub skipped: usize,
    /// Why the whole mirror was skipped, when it was: `archived` |
    /// `missing-character` | `no-vault`.
    pub skipped_reason: Option<&'static str>,
}

impl WriteCommonplaceDigestsResult {
    /// v4's object (`skippedReason` omitted when absent).
    pub fn to_json(&self) -> Value {
        let mut v = json!({
            "written": self.written,
            "unchanged": self.unchanged,
            "skipped": self.skipped,
        });
        if let Some(r) = self.skipped_reason {
            v["skippedReason"] = json!(r);
        }
        v
    }
}

/// v4 `stripUpdatedAt`: compare files ignoring the `updatedAt` stamp, so a
/// bucket whose digests did not change is not rewritten (and its vault document
/// not re-embedded) merely because the clock moved. `/^updatedAt: .*$/m`
/// without `g` — the FIRST such line only; `.` stops at a JS line terminator.
fn strip_updated_at(content: &str) -> String {
    let mut line_start = 0usize;
    loop {
        let rest = &content[line_start..];
        if rest.starts_with("updatedAt: ") {
            let end = rest
                .char_indices()
                .find(|(_, c)| matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}'))
                .map(|(i, _)| line_start + i)
                .unwrap_or(content.len());
            return format!("{}{}", &content[..line_start], &content[end..]);
        }
        // Advance past the next JS line terminator (a `^` under the `m` flag).
        match rest
            .char_indices()
            .find(|(_, c)| matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}'))
        {
            Some((i, c)) => line_start += i + c.len_utf8(),
            None => return content.to_string(),
        }
    }
}

/// v4 `readDatabaseDocumentIfExists` (`database-store.ts:88-102`): `None` only
/// for NOT_FOUND (with v4's debug line); every other failure propagates. Runs on
/// the caller's thread.
pub(crate) fn read_database_document_if_exists(
    db: &Db,
    mount_point_id: &str,
    relative_path: &str,
) -> Result<Option<String>, String> {
    let mp = mount_point_id.to_string();
    let rel = relative_path.to_string();
    let read = db
        .read_mount_index(move |conn| Ok(read_database_document(conn, &mp, &rel)))
        .map_err(|e| e.to_string())?;
    match read {
        Ok(doc) => Ok(Some(doc.content)),
        Err(StoreError::Store(e)) if e.code == DbStoreErrorCode::NotFound => {
            // v4's `database-store.ts` child logger (`module: 'service'`).
            tracing::debug!(
                module = "service",
                mountPointId = %mount_point_id,
                relativePath = %relative_path,
                "Document absent from database-backed store"
            );
            Ok(None)
        }
        Err(e) => Err(e.to_string()),
    }
}

fn store_to_db_error(e: StoreError) -> DbError {
    match e {
        StoreError::Db(e) => e,
        StoreError::Store(e) => DbError::Internal(e.message),
    }
}

/// v4 `getCharacterVaultStore(holder)` → `mountPointId`
/// (`character-vault-bridge.ts:58-82`): the OVERLAY character read (a dangling
/// vault throws → v4's WARN + `null`), then the mount point, which must be
/// `mountType = 'database'` + `storeType = 'character'` (the character
/// gallery's predicate over `find_store_naming_by_id`); a mismatch is v4's
/// silent `null`.
fn holder_vault_mount_id(db: &Db, character_id: &str) -> Option<String> {
    let id = character_id.to_string();
    let resolved = db.read_main(|main| {
        db.read_mount_index(|mount| {
            let Some(character) = characters_read::find_by_id(main, mount, &id)? else {
                return Ok(None);
            };
            let Some(mp_id) = character
                .get("characterDocumentMountPointId")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
            else {
                return Ok(None);
            };
            let Some(mp) = DocMountPointsRepository::new(mount).find_store_naming_by_id(mp_id)?
            else {
                return Ok(None);
            };
            if mp.mount_type != "database" || mp.store_type.as_deref() != Some("character") {
                return Ok(None);
            }
            Ok(Some(mp.id))
        })
    });
    match resolved {
        Ok(found) => found,
        Err(e) => {
            // The overlay's absent-keystone refusal carries v4's
            // `CharacterVaultUnavailableError` structurally; render ITS message
            // (the api layer's `StoreUnavailable` sentence is v5's own).
            let error = match &e {
                DbError::StoreUnavailable {
                    entity_label: "character",
                    id,
                    mount_point_id: Some(mount_id),
                    ..
                } => crate::db::vault_read_overlay::VaultUnavailable {
                    character_id: id.clone(),
                    mount_id: mount_id.clone(),
                }
                .message(),
                other => crate::db::fallback::error_text(other),
            };
            tracing::warn!(
                context = "file-storage.character-vault-bridge",
                characterId = %character_id,
                error = %error,
                "Failed to resolve character vault store"
            );
            None
        }
    }
}

/// The body of v4's outer `try`: `Ok` carries the early-return result, `Err`
/// the outer catch's message.
async fn mirror(
    db: &Db,
    input: &WriteCommonplaceDigestsInput,
    result: &mut WriteCommonplaceDigestsResult,
) -> Result<Option<WriteCommonplaceDigestsResult>, String> {
    // Tombstone guard, on the raw row: the overlay read is for live characters.
    let holder = input.holder_character_id.clone();
    let raw = db
        .read_main(move |conn| characters_read::find_by_id_raw(conn, &holder))
        .map_err(|e| crate::db::fallback::error_text(&e))?;
    let Some(raw) = raw else {
        tracing::debug!(
            context = CONTEXT,
            characterId = %input.holder_character_id,
            "Skipping Commonplace mirror: holder not found"
        );
        return Ok(Some(WriteCommonplaceDigestsResult {
            skipped: input.files.len(),
            skipped_reason: Some("missing-character"),
            ..result.clone()
        }));
    };
    if crate::api::characters::is_archived(&raw) {
        tracing::debug!(
            context = CONTEXT,
            characterId = %input.holder_character_id,
            "Skipping Commonplace mirror for archived character"
        );
        return Ok(Some(WriteCommonplaceDigestsResult {
            skipped: input.files.len(),
            skipped_reason: Some("archived"),
            ..result.clone()
        }));
    }

    let Some(mount_point_id) = holder_vault_mount_id(db, &input.holder_character_id) else {
        tracing::debug!(
            context = CONTEXT,
            characterId = %input.holder_character_id,
            "Skipping Commonplace mirror: holder has no character vault"
        );
        return Ok(Some(WriteCommonplaceDigestsResult {
            skipped: input.files.len(),
            skipped_reason: Some("no-vault"),
            ..result.clone()
        }));
    };

    let to_write: Vec<&CommonplaceMirrorFile> = input
        .files
        .iter()
        .filter(|f| !f.digests.is_empty())
        .collect();
    if !to_write.is_empty() {
        let mp = mount_point_id.clone();
        db.write(move |writers| {
            let Some(mi) = writers.mount_index() else {
                return Err(DbError::PartitionUnavailable(
                    crate::write_partition::WriteDbTarget::MountIndex,
                ));
            };
            DocMountFileLinksRepository::new(mi.connection())
                .ensure_folder_path(&mp, COMMONPLACE_FOLDER)?;
            Ok(())
        })
        .await
        .map_err(|e| crate::db::fallback::error_text(&e))?;
    }
    result.skipped += input.files.len() - to_write.len();

    for file in to_write {
        let subject = if file.is_self {
            CommonplaceSubject::SelfFile
        } else {
            CommonplaceSubject::Other {
                id: file.subject_character_id.clone(),
                name: file.subject_name.clone(),
            }
        };
        let path = commonplace_path_for(&subject);
        let attempt: Result<bool, String> = async {
            let rendered = render_commonplace_file(&CommonplaceFileInput {
                subject_character_id: file.subject_character_id.clone(),
                subject_name: file.subject_name.clone(),
                is_self: file.is_self,
                digests: file.digests.clone(),
                updated_at: input.updated_at.clone(),
            });
            let existing = read_database_document_if_exists(db, &mount_point_id, &path)?;
            if let Some(existing) = existing {
                if strip_updated_at(&existing) == strip_updated_at(&rendered.content) {
                    return Ok(false);
                }
            }
            let (mp, rel, content) = (
                mount_point_id.clone(),
                path.clone(),
                rendered.content.clone(),
            );
            db.write(move |writers| {
                let Some(mi) = writers.mount_index() else {
                    return Err(DbError::PartitionUnavailable(
                        crate::write_partition::WriteDbTarget::MountIndex,
                    ));
                };
                write_database_document(mi.connection(), &mp, &rel, &content)
                    .map_err(store_to_db_error)
            })
            .await
            .map_err(|e| match &e {
                DbError::Internal(m) => m.clone(),
                other => crate::db::fallback::error_text(other),
            })?;
            tracing::debug!(
                context = CONTEXT,
                characterId = %input.holder_character_id,
                path = %path,
                entriesWritten = rendered.entries_written,
                entriesDropped = rendered.entries_dropped,
                "Wrote Commonplace digest mirror"
            );
            Ok(true)
        }
        .await;
        match attempt {
            Ok(true) => result.written += 1,
            Ok(false) => result.unchanged += 1,
            Err(error) => {
                result.skipped += 1;
                tracing::warn!(
                    context = CONTEXT,
                    characterId = %input.holder_character_id,
                    path = %path,
                    error = %error,
                    "Failed to write a Commonplace digest mirror"
                );
            }
        }
    }
    Ok(None)
}

/// v4 `writeCommonplaceDigestsToVault`: write (or rewrite) the holder's
/// Commonplace mirror files. Best-effort per file; never fails to the caller —
/// the digests themselves are the record, the mirror is a convenience the next
/// run rewrites anyway.
pub async fn write_commonplace_digests_to_vault(
    db: &Db,
    input: &WriteCommonplaceDigestsInput,
) -> WriteCommonplaceDigestsResult {
    let mut result = WriteCommonplaceDigestsResult::default();
    match mirror(db, input, &mut result).await {
        // An early return (missing / archived / no vault) never logs the
        // completion line — v4's is not a `finally`.
        Ok(Some(early)) => return early,
        Ok(None) => {}
        Err(error) => {
            tracing::warn!(
                context = CONTEXT,
                characterId = %input.holder_character_id,
                error = %error,
                "Commonplace mirror failed"
            );
        }
    }
    tracing::debug!(
        context = CONTEXT,
        characterId = %input.holder_character_id,
        written = result.written,
        unchanged = result.unchanged,
        skipped = result.skipped,
        "Commonplace mirror complete"
    );
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_updated_at_takes_the_first_line_only() {
        let a = "---\ntype: x\nupdatedAt: \"1\"\n---\nupdatedAt: body\n";
        assert_eq!(
            strip_updated_at(a),
            "---\ntype: x\n\n---\nupdatedAt: body\n"
        );
        assert_eq!(strip_updated_at("updatedAt: x"), "");
        assert_eq!(strip_updated_at("no stamp"), "no stamp");
        assert_eq!(strip_updated_at("a\r\nupdatedAt: q\r\nb"), "a\r\n\r\nb");
    }
}
