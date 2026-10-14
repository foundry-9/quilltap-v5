//! `commonplace_mirror` — the Commonplace mirror written from COMMITTED
//! digests. P4.D288 (C1-7, L-3), porting v4
//! `lib/memory/commonplace-mirror.ts:105-200` (`mirrorCommonplaceForSubjects`,
//! bug 208) at the `83334b9a5` pin.
//!
//! "The mirror describes committed digests, so it is written after the batch
//! that created them commits." [`mirror_commonplace_for_subjects`] rewrites
//! the holder's Commonplace files for the given subjects from the hot digests
//! the database holds NOW, through
//! [`write_commonplace_digests_to_vault`] — which removes the file of a bucket
//! left with no hot digest (bug 208). It is the ONE home: the retire path's
//! [`announce_retirement`](super::memory_retire::announce_retirement) calls it
//! for every removed digest's bucket, P4.D290's `update_memory` and P4.D297's
//! maintenance run call it too.
//!
//! v4's parent-only arm (`Commonplace mirror is parent-only; the commit hook
//! writes it`, the job child) is unreachable — v5 is in-process. The
//! commit-hook halves (`mirrorCommonplaceAfterCommit`,
//! `subjectsTouchedByWrites`, `digestBucketsDeletedByWrites`,
//! `mirrorCommonplaceForDeletedDigests`) are the maintenance job's — P4.D297's
//! to port over this fn. Every line carries v4's child-logger binding
//! `module: 'memory:commonplace-mirror'` first.

use serde_json::Value;

use crate::commonplace_file::CommonplaceDigestEntry;
use crate::db::memories_read::{self, DigestSubject};
use crate::db::runtime::Db;
use crate::db::{characters_read, DbError};
use crate::services::commonplace_digest_vault_bridge::{
    write_commonplace_digests_to_vault, CommonplaceMirrorFile, WriteCommonplaceDigestsInput,
};

/// v4's child-logger binding.
const MODULE: &str = "memory:commonplace-mirror";

/// v4 `COMMONPLACE_MIRROR_DIGESTS_PER_BUCKET` — the hot digests read per
/// bucket (the render orders and trims).
pub const COMMONPLACE_MIRROR_DIGESTS_PER_BUCKET: i64 = 200;

/// v4 `mirrorCommonplaceForSubjects(holderCharacterId, subjectCharacterIds,
/// updatedAt = now)`: rewrite the holder's Commonplace files for the given
/// subjects (deduped, first-seen order) from the hot digests the database
/// holds now. Returns the number of files written. Never fails — "the digests
/// are the record; the mirror is their echo."
///
/// Lines: DEBUG `Commonplace mirror skipped: holder not found {module,
/// characterId}`; per vanished subject DEBUG `Commonplace mirror skips a
/// bucket whose subject no longer exists {module, characterId, subjectId}`;
/// after the write DEBUG `Commonplace mirror written from committed digests
/// {module, characterId, buckets, written, unchanged, skipped,
/// skippedReason?}`; any failure WARN `Commonplace mirror failed (the digests
/// are committed regardless) {module, characterId, error}` and `0`.
pub async fn mirror_commonplace_for_subjects(
    db: &Db,
    holder_id: &str,
    subjects: &[String],
    updated_at: Option<&str>,
) -> usize {
    let mut seen = std::collections::HashSet::new();
    let subjects: Vec<&String> = subjects
        .iter()
        .filter(|s| seen.insert(s.as_str()))
        .collect();
    if subjects.is_empty() {
        return 0;
    }
    let updated_at = updated_at.map_or_else(crate::clock::now_iso, str::to_string);
    match mirror(db, holder_id, &subjects, updated_at).await {
        Ok(written) => written,
        Err(error) => {
            tracing::warn!(
                module = MODULE,
                characterId = %holder_id,
                error = %error,
                "Commonplace mirror failed (the digests are committed regardless)"
            );
            0
        }
    }
}

/// The body of v4's `try`.
async fn mirror(
    db: &Db,
    holder_id: &str,
    subjects: &[&String],
    updated_at: String,
) -> Result<usize, String> {
    let text = |e: DbError| crate::db::fallback::error_text(&e);
    // v4 `findByIdRaw` — the base `_findById` FALLBACK (a failed read is
    // `null`).
    let hid = holder_id.to_string();
    let holder = db
        .read_main(move |conn| Ok(characters_read::find_by_id_raw_or_none(conn, &hid)))
        .map_err(text)?;
    let Some(holder) = holder else {
        tracing::debug!(
            module = MODULE,
            characterId = %holder_id,
            "Commonplace mirror skipped: holder not found"
        );
        return Ok(0);
    };
    let holder_name = name_of(&holder);

    let mut files: Vec<CommonplaceMirrorFile> = Vec::new();
    for subject_id in subjects {
        let is_self = subject_id.as_str() == holder_id;
        let mut subject_name = holder_name.clone();
        if !is_self {
            // v4 tries the overlay read, then the raw one, each in its own
            // `try`/`catch` → `null`.
            let sid = subject_id.to_string();
            let overlaid = db
                .read_main(|main| {
                    db.read_mount_index(|mount| characters_read::find_by_id(main, mount, &sid))
                })
                .ok()
                .flatten();
            let subject = match overlaid {
                Some(s) => Some(s),
                None => {
                    let sid = subject_id.to_string();
                    db.read_main(move |conn| characters_read::find_by_id_raw(conn, &sid))
                        .ok()
                        .flatten()
                }
            };
            let Some(subject) = subject else {
                tracing::debug!(
                    module = MODULE,
                    characterId = %holder_id,
                    subjectId = %subject_id,
                    "Commonplace mirror skips a bucket whose subject no longer exists"
                );
                continue;
            };
            subject_name = name_of(&subject);
        }

        let (hid, bucket) = (
            holder_id.to_string(),
            if is_self {
                DigestSubject::SelfSubject
            } else {
                DigestSubject::About(subject_id.to_string())
            },
        );
        let digests = db
            .read_main(move |conn| {
                memories_read::find_hot_digests(
                    conn,
                    &hid,
                    bucket,
                    COMMONPLACE_MIRROR_DIGESTS_PER_BUCKET,
                )
            })
            .map_err(text)?;
        files.push(CommonplaceMirrorFile {
            subject_character_id: subject_id.to_string(),
            subject_name,
            is_self,
            digests: digests.iter().map(digest_entry).collect(),
        });
    }
    if files.is_empty() {
        return Ok(0);
    }

    let buckets = files.len();
    let result = write_commonplace_digests_to_vault(
        db,
        &WriteCommonplaceDigestsInput {
            holder_character_id: holder_id.to_string(),
            files,
            updated_at,
        },
    )
    .await;
    match result.skipped_reason {
        Some(reason) => tracing::debug!(
            module = MODULE,
            characterId = %holder_id,
            buckets = buckets,
            written = result.written,
            unchanged = result.unchanged,
            skipped = result.skipped,
            skippedReason = reason,
            "Commonplace mirror written from committed digests"
        ),
        None => tracing::debug!(
            module = MODULE,
            characterId = %holder_id,
            buckets = buckets,
            written = result.written,
            unchanged = result.unchanged,
            skipped = result.skipped,
            "Commonplace mirror written from committed digests"
        ),
    }
    Ok(result.written)
}

/// A character row's `name` (v4 reads `holder.name` / `subject.name`).
fn name_of(character: &Value) -> String {
    character
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// v4's `{ content, kind: d.kind ?? 'semantic', occurredAt: d.occurredAt ??
/// null, reinforcedImportance: d.reinforcedImportance ?? d.importance }`.
fn digest_entry(d: &Value) -> CommonplaceDigestEntry {
    let str_of = |k: &str| d.get(k).and_then(Value::as_str).map(str::to_string);
    CommonplaceDigestEntry {
        content: str_of("content").unwrap_or_default(),
        kind: str_of("kind").unwrap_or_else(|| "semantic".to_string()),
        occurred_at: str_of("occurredAt"),
        reinforced_importance: d
            .get("reinforcedImportance")
            .and_then(Value::as_f64)
            .or_else(|| d.get("importance").and_then(Value::as_f64))
            .unwrap_or(f64::NAN),
    }
}
