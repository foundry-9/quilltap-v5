//! Passphrase-change re-encryption of archive bundles (§4.2c) — the port of v4
//! `lib/characters/archive-reencrypt.ts` (v4 `d553f72a`).
//!
//! `changePassphrase` re-wraps `.dbkey` and nothing else; archive bundles in
//! `files/` would silently still want the old passphrase. This second phase
//! enumerates every ARCHIVE file, decrypts it with the old passphrase, and
//! re-encrypts it with the new one — including bundles written before archive
//! encryption existed, which are plaintext NDJSON and simply gain a header.
//!
//! The operation is not atomic across files (each is rewritten in place, one
//! at a time), so a partial failure leaves a mixed-passphrase library. The
//! result therefore names exactly which archives still hold the old
//! passphrase. A bundle that already refuses the old passphrase (it predates
//! an *earlier* passphrase change that failed partway) is reported the same
//! way rather than silently skipped.
//!
//! **Failures never abort the sweep** — every remaining file still gets its
//! attempt. That is the whole design: a passphrase change that stopped at the
//! first bad bundle would leave the rest of the library unreachable with no
//! record of which ones moved.

use serde::Serialize;

use super::crypto::{decrypt_archive, encrypt_archive, is_encrypted_archive, ArchiveCryptoError};
use crate::db::files::{FileUpdate, FilesRepository};
use crate::db::runtime::Db;
use crate::db::DbError;
use crate::services::file_storage::StorageBackend;

/// The `files.category` value an archive bundle carries.
pub const ARCHIVE_CATEGORY: &str = "ARCHIVE";

/// v4's per-file failure record. `reason` is surfaced verbatim in the settings
/// UI, so its wording is contractual.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ArchiveReencryptFailure {
    #[serde(rename = "fileId")]
    pub file_id: String,
    pub filename: String,
    /// Human-readable diagnosis — surfaced verbatim in the settings UI.
    pub reason: String,
}

/// v4 `ArchiveReencryptResult`.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ArchiveReencryptResult {
    /// How many ARCHIVE files exist. **`-1` means the sweep itself failed** —
    /// the unlock route's marker for "this did not run", distinct from "ran
    /// and found nothing".
    pub total: i64,
    /// How many were successfully rewritten under the new passphrase.
    pub reencrypted: i64,
    /// The archives left holding the old passphrase, by name.
    pub failures: Vec<ArchiveReencryptFailure>,
}

/// The reason a passphrase mismatch produces — v4's exact sentence, and the
/// one arm whose wording is not simply `error.message`.
const MISMATCH_REASON: &str =
    "does not open with the old passphrase — it predates an earlier passphrase change";

/// Rewrite every ARCHIVE bundle from `old_passphrase` to `new_passphrase` —
/// v4 `reencryptArchiveBundles`.
///
/// Failures never abort the sweep; the result names the ones left behind. The
/// caller resolves the empty-passphrase legs to the internal sentinel BEFORE
/// calling (v4's route does the same), so this function sees real key material
/// on both sides.
pub fn reencrypt_archive_bundles(
    db: &Db,
    backend: &dyn StorageBackend,
    user_id: &str,
    old_passphrase: &str,
    new_passphrase: &str,
) -> Result<ArchiveReencryptResult, DbError> {
    let uid = user_id.to_string();
    let archives = db.read_main(move |conn| {
        FilesRepository::new(conn).find_by_category(&uid, ARCHIVE_CATEGORY)
    })?;

    let mut result = ArchiveReencryptResult {
        total: archives.len() as i64,
        reencrypted: 0,
        failures: Vec::new(),
    };
    if archives.is_empty() {
        return Ok(result);
    }

    tracing::info!(
        target: "quilltap::archive",
        total = archives.len(),
        "Re-encrypting archive bundles for passphrase change",
    );

    for (index, file) in archives.iter().enumerate() {
        match reencrypt_one(db, backend, file, old_passphrase, new_passphrase) {
            Ok(()) => {
                result.reencrypted += 1;
                tracing::info!(
                    target: "quilltap::archive",
                    file_id = %file.id,
                    filename = %file.original_filename,
                    progress = %format!("{}/{}", index + 1, archives.len()),
                    "Archive bundle re-encrypted",
                );
            }
            Err(reason) => {
                tracing::warn!(
                    target: "quilltap::archive",
                    file_id = %file.id,
                    filename = %file.original_filename,
                    reason = %reason,
                    "Archive bundle NOT re-encrypted; it still expects the old passphrase",
                );
                result.failures.push(ArchiveReencryptFailure {
                    file_id: file.id.clone(),
                    filename: file.original_filename.clone(),
                    reason,
                });
            }
        }
    }

    tracing::info!(
        target: "quilltap::archive",
        total = result.total,
        reencrypted = result.reencrypted,
        failed = result.failures.len(),
        "Archive re-encryption pass complete",
    );
    Ok(result)
}

/// One bundle's download → decrypt-or-passthrough → encrypt → upload-in-place
/// → size update. `Err` carries the `reason` string verbatim (v4's
/// `error.message`, except the named passphrase-mismatch sentence).
fn reencrypt_one(
    db: &Db,
    backend: &dyn StorageBackend,
    file: &crate::db::files::FileFull,
    old_passphrase: &str,
    new_passphrase: &str,
) -> Result<(), String> {
    let Some(storage_key) = file.storage_key.as_deref().filter(|k| !k.is_empty()) else {
        return Err("file row has no storageKey — bytes were never uploaded".to_string());
    };

    // Download through the MANAGER, not the raw backend: v4's sweep goes
    // through `fileStorageManager.downloadFile` (`archive-reencrypt.ts:76`),
    // whose missing-content arm yields the clean un-wrapped sentence and whose
    // generic arm wraps with `Failed to download file '<name>': `. A raw
    // `backend.download` here leaked the Bug-55 SOH sentinel verbatim into the
    // change-passphrase `failures[].reason` — caught by the round-1
    // unification review before the sweep gained its caller.
    let entry = crate::db::files::FileEntry {
        id: file.id.clone(),
        sha256: file.sha256.clone(),
        original_filename: file.original_filename.clone(),
        mime_type: file.mime_type.clone(),
        size: file.size,
        width: file.width,
        height: file.height,
        category: file.category.clone(),
        generation_prompt: None,
        generation_model: None,
        generation_revised_prompt: None,
        description: file.description.clone(),
        storage_key: file.storage_key.clone(),
    };
    let bytes = crate::services::file_storage::download_file(db, backend, &entry)?;
    // Pre-encryption bundles are plaintext NDJSON; they "decrypt" to
    // themselves and simply gain a header on the way back out.
    let plaintext = if is_encrypted_archive(&bytes) {
        decrypt_archive(&bytes, old_passphrase).map_err(|e| match e {
            ArchiveCryptoError::PassphraseMismatch { .. } => MISMATCH_REASON.to_string(),
            other => other.to_string(),
        })?
    } else {
        bytes
    };
    let reencrypted =
        encrypt_archive(&plaintext, new_passphrase, None).map_err(|e| e.to_string())?;

    backend.upload(
        storage_key,
        &reencrypted,
        if file.mime_type.is_empty() {
            "application/octet-stream"
        } else {
            &file.mime_type
        },
    )?;

    let size = reencrypted.len() as f64;
    let file_id = file.id.clone();
    db.write_blocking(move |ws| {
        FilesRepository::new(ws.main().connection()).update(
            &file_id,
            &FileUpdate {
                size: Some(size),
                updated_at: crate::clock::now_iso(),
                ..Default::default()
            },
        )
    })
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_result_serializes_in_v4s_shape() {
        let r = ArchiveReencryptResult {
            total: -1,
            reencrypted: 0,
            failures: vec![ArchiveReencryptFailure {
                file_id: String::new(),
                filename: "(all archives)".to_string(),
                reason: "boom".to_string(),
            }],
        };
        assert_eq!(
            serde_json::to_string(&r).expect("serialize"),
            r#"{"total":-1,"reencrypted":0,"failures":[{"fileId":"","filename":"(all archives)","reason":"boom"}]}"#
        );
    }
}
