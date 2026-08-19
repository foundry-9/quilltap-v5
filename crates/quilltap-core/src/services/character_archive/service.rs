//! Character archive: pack a character into a `.qtap` bundle, commit a
//! tombstone, and **prune the vault in place** (§4.2a of the archive spec) —
//! the port of v4 `lib/characters/archive-service.ts` (889 ln, v4 `ed8934f1`).
//!
//! Operator-only — no tool, no job, no sweep.
//!
//! Archiving does NOT delete the character's vault. It keeps the ten
//! managed-field documents, the avatar blob(s) and their link rows, and the
//! wardrobe — so a tombstone is still a readable character page and every id
//! old chats reference keeps resolving — and moves everything heavy (photos,
//! `Mail/`, conversation summaries, the character's own memories, and the
//! chunks of everything deleted) into the bundle before deleting it.
//!
//! **Ordering is chosen for crash-safety**, because the main DB and the mount
//! index cannot share a transaction:
//!
//! 1. Write the bundle to an ARCHIVE `files` row, encrypted under the instance
//!    passphrase (§4.2c — never the pepper; see [`super::crypto`]).
//! 2. Verify it by decrypting and reading back the ciphertext that will be
//!    persisted — a hard gate before anything is deleted.
//! 3. Commit the tombstone in one main-DB write: `archivedAt` + `archiveFileId`,
//!    null the default-* FKs, flip chat seats to absent. Deliberately NOT
//!    touched: `characterDocumentMountPointId` (the vault survives),
//!    `defaultImageId` and `avatarOverrides` (they point at blobs the prune
//!    keeps — nulling them is what took the face off old messages).
//! 4. Prune (idempotent, re-runnable): own memories, the vector store, then
//!    every vault link outside the keep-set, through the document-store delete
//!    paths so link-group orphan GC runs.
//!
//! A failure before step 3 removes the bundle it wrote and leaves the character
//! fully live. A failure during step 4 reports `pruneComplete: false`; calling
//! [`archive_character`] again re-runs the prune only and never rewrites the
//! bundle. Because the prune is a delete-set rather than a teardown, re-running
//! it is naturally idempotent — anything already gone stays gone, and the
//! keep-set is never a candidate.
//!
//! Untouched, by design: chats, messages, annotations, group membership,
//! project rosters, and *other* characters' memories about this one.
//!
//! ## The two v5 shape differences (neither is a behavior change)
//!
//! - **The passphrase reaches the service as data.** v4 calls the module-global
//!   `resolveArchivePassphrase()`; v5's runtime passphrase lives on the engine,
//!   so the caller hands in a [`crypto::PassphraseSource`] and this module
//!   resolves it *at v4's exact point* — after the already-archived early
//!   return, so re-running a prune on a locked-passphrase instance still works.
//! - **The host seams ride in a struct.** Storage backend, pixel codec, text
//!   extractor and app version are v4 module singletons; here they are
//!   [`ArchiveSeams`] fields, so the differential drives the same code the
//!   engine does.

use std::collections::HashSet;
use std::sync::Arc;

use serde_json::{json, Map, Value};

use super::crypto::{
    decrypt_archive, encrypt_archive, is_encrypted_archive, resolve_archive_passphrase,
    ArchiveCryptoError, PassphraseSource,
};
use crate::db::runtime::Db;
use crate::db::{
    characters_read, chats_read,
    doc_mount_blobs::DocMountBlobsRepository,
    doc_mount_chunks::DocMountChunksRepository,
    doc_mount_documents::DocMountDocumentsRepository,
    doc_mount_file_links::DocMountFileLinksRepository,
    doc_mount_folders::DocMountFoldersRepository,
    doc_mount_points::DocMountPointsRepository,
    embedding_status::EmbeddingStatusRepository,
    files::{CreateOptions as FileCreateOptions, FileCreate, FileEntry, FilesRepository},
    memories::MemoriesRepository,
    memories_read, vault_character_update,
    vector_indices::VectorIndicesRepository,
    DbError,
};
use crate::services::file_storage::{PixelCodec, StorageBackend};
use crate::services::mount_index::converters::DocumentTextExtractor;
use crate::services::qtap_export::{stream_export_records, ExportOptions};
use crate::services::quilltap_import::ndjson::{assemble_export_from_stream, read_ndjson_lines};
use crate::services::quilltap_import::{
    execute_import, ConflictStrategy, ImportOptions, PreserveIdsMode, QuilltapExport,
};

// ---------------------------------------------------------------------------
// Results + errors
// ---------------------------------------------------------------------------

/// v4 `ArchiveCharacterResult`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveCharacterResult {
    pub archived: bool,
    pub archive_file_id: Option<String>,
    /// False when any prune step failed. The character is still archived and
    /// the bundle is still good; re-run [`archive_character`] to finish.
    pub prune_complete: bool,
}

impl ArchiveCharacterResult {
    /// v4's `NextResponse.json(result)` body — key order included.
    pub fn to_value(&self) -> Value {
        json!({
            "archived": self.archived,
            "archiveFileId": self.archive_file_id,
            "pruneComplete": self.prune_complete,
        })
    }
}

/// What the bundle import actually put back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestoredCounts {
    pub memories: u32,
    pub documents: u32,
    pub blobs: u32,
}

/// v4 `RehydrateCharacterResult`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RehydrateCharacterResult {
    pub rehydrated: bool,
    pub archived: bool,
    /// The ARCHIVE bundle file deliberately left in the library after a
    /// successful restore (§6 step 6 — cheap insurance). `None` when the
    /// tombstone had no bundle. The UI offers deletion; nothing here deletes it.
    pub archive_bundle_file_id: Option<String>,
    /// Absent when there was no bundle — v4 spreads `restored` conditionally,
    /// so the KEY IS ABSENT rather than null.
    pub restored: Option<RestoredCounts>,
    pub warnings: Vec<String>,
}

impl RehydrateCharacterResult {
    /// v4's route body. `restored` is spread only when present (`...(restored ?
    /// { restored } : {})`), so an absent restore omits the key entirely.
    pub fn to_value(&self) -> Value {
        let mut m = Map::new();
        m.insert("rehydrated".into(), Value::Bool(self.rehydrated));
        m.insert("archived".into(), Value::Bool(self.archived));
        m.insert(
            "archiveBundleFileId".into(),
            match &self.archive_bundle_file_id {
                Some(s) => Value::String(s.clone()),
                None => Value::Null,
            },
        );
        if let Some(r) = &self.restored {
            m.insert(
                "restored".into(),
                json!({"memories": r.memories, "documents": r.documents, "blobs": r.blobs}),
            );
        }
        m.insert(
            "warnings".into(),
            Value::Array(
                self.warnings
                    .iter()
                    .map(|w| Value::String(w.clone()))
                    .collect(),
            ),
        );
        Value::Object(m)
    }
}

/// Every way archive/rehydrate refuses. The variants map 1:1 onto v4's thrown
/// error CLASSES, because the route arms dispatch on `instanceof` and the
/// status each produces is contractual (`handlers/post.ts:372`/`:396`).
#[derive(Debug)]
pub enum ArchiveError {
    /// v4 `` throw new Error(`Character ${id} not found`) ``.
    CharacterNotFound(String),
    /// v4 `` throw new Error(`Character ${id} is not archived`) ``.
    NotArchived(String),
    /// v4 `ArchiveVerificationError` — the bundle we just wrote (or are about
    /// to restore) does not match the live character.
    Verification(String),
    /// v4 `CharacterRehydrationError`.
    Rehydration {
        character_id: String,
        detail: String,
    },
    /// v4's three crypto classes plus `ArchiveKeyUnavailableError`, carried as
    /// one variant because [`ArchiveCryptoError`] already distinguishes them
    /// and the route arm re-splits on that.
    Crypto(ArchiveCryptoError),
    /// A DB failure at a step v4 does not catch.
    Db(DbError),
    /// A storage failure at a step v4 does not catch (`uploadRaw`,
    /// `downloadFile`) — v4's raw `Error.message`.
    Storage(String),
}

impl std::fmt::Display for ArchiveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ArchiveError::CharacterNotFound(id) => write!(f, "Character {id} not found"),
            ArchiveError::NotArchived(id) => write!(f, "Character {id} is not archived"),
            ArchiveError::Verification(d) => write!(f, "Archive bundle verification failed: {d}"),
            ArchiveError::Rehydration {
                character_id,
                detail,
            } => write!(f, "Character {character_id} cannot be rehydrated: {detail}"),
            ArchiveError::Crypto(e) => write!(f, "{e}"),
            ArchiveError::Db(e) => write!(f, "{e}"),
            ArchiveError::Storage(m) => write!(f, "{m}"),
        }
    }
}

impl std::error::Error for ArchiveError {}

impl From<DbError> for ArchiveError {
    fn from(e: DbError) -> Self {
        ArchiveError::Db(e)
    }
}

/// The host capabilities v4 reaches for as module singletons.
pub struct ArchiveSeams<'a> {
    /// `fileStorageManager` — where the encrypted bundle bytes land, and where
    /// rehydrate reads them back from. `None` on a host with no disk layer:
    /// archiving then fails loudly at the upload (there is nowhere to put the
    /// only copy of the pruned material), exactly as v4's not-configured
    /// backend throws.
    pub backend: Option<Arc<dyn StorageBackend>>,
    /// v4 `resolveArchivePassphrase()`'s two inputs.
    pub passphrase: PassphraseSource<'a>,
    /// v4 reads `packageJson.version` inside the export writer's manifest.
    pub app_version: String,
    /// The import path's image transcoder (`None` → the not-configured codec,
    /// which passes bytes through — v4's sharp-failed arm).
    ///
    /// `Arc`, not `&dyn`: the import and the re-chunk both run inside a
    /// `Db::write` closure, which the runtime requires to be `'static`.
    pub codec: Option<Arc<dyn PixelCodec>>,
    /// `reindexLinks`' text extractor for the post-restore re-chunk (`Arc` for
    /// the same reason as `codec`).
    pub extractor: Arc<dyn DocumentTextExtractor>,
}

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// The writer only emits `doc_mount_document` records for text file types;
/// everything else in a store travels as a blob. Verification has to apply the
/// same filter or it compares against a total the bundle was never going to
/// carry. (v4 `TEXT_DOCUMENT_FILE_TYPES`.)
const TEXT_DOCUMENT_FILE_TYPES: [&str; 4] = ["markdown", "txt", "json", "jsonl"];

/// The §4.2a keep-set: vault paths that survive the prune. The ten managed-field
/// documents, plus everything under `Wardrobe/` (checked by prefix, not listed
/// here). Avatar links are kept by id, not path — see [`prune_vault`].
///
/// v4 spells these as ten `CHARACTER_*_PATH` imports from
/// `vault-overlay/schema.ts`. v5 has the same nine in
/// [`crate::db::vault_read_overlay::SINGLE_FILE_OVERLAY_PATHS`] plus
/// `wardrobe.json`, but this list stays SEPARATE on purpose: v4's keep-set is
/// its own enumeration, so binding v5's to the overlay list would make a future
/// overlay addition silently change what the prune keeps — a behavior change no
/// differential is positioned to see (the round-1 shared-helper lesson).
const KEPT_MANAGED_PATHS: [&str; 10] = [
    "properties.json",
    "metadata.json",
    "identity.md",
    "description.md",
    "manifesto.md",
    "personality.md",
    "example-dialogues.md",
    "physical-description.md",
    "physical-prompts.json",
    "wardrobe.json",
];

/// v4 `CHARACTER_WARDROBE_FOLDER` (`mount-index/character-vault.ts`).
const CHARACTER_WARDROBE_FOLDER: &str = "Wardrobe";

/// [`KEPT_MANAGED_PATHS`], lowercased — exactly as v4 lowercases the schema
/// constants into its `Set`.
fn kept_managed_paths() -> HashSet<String> {
    KEPT_MANAGED_PATHS
        .iter()
        .map(|p| p.to_lowercase())
        .collect()
}

// ---------------------------------------------------------------------------
// archiveCharacter
// ---------------------------------------------------------------------------

/// v4 `archiveCharacter(userId, characterId)`.
pub async fn archive_character(
    db: &Db,
    user_id: &str,
    character_id: &str,
    seams: &ArchiveSeams<'_>,
) -> Result<ArchiveCharacterResult, ArchiveError> {
    let cid = character_id.to_string();
    let character = db
        .read_main(move |c| characters_read::find_by_id_raw(c, &cid))?
        .ok_or_else(|| ArchiveError::CharacterNotFound(character_id.to_string()))?;

    // Already archived: a previous run committed the tombstone and then failed
    // (or was killed) partway through the prune. Re-run the prune; never rewrite
    // the bundle, and never touch the tombstone fields again.
    if str_field(&character, "archivedAt").is_some() {
        tracing::info!(
            target: "quilltap::archive",
            character_id,
            "Character already archived; re-running the prune only",
        );
        let prune_complete = prune_archived_character_state(db, character_id).await?;
        return Ok(ArchiveCharacterResult {
            archived: true,
            archive_file_id: str_field(&character, "archiveFileId"),
            prune_complete,
        });
    }

    let cid = character_id.to_string();
    let memory_count = db
        .read_main(move |c| memories_read::find_by_character_id(c, &cid))?
        .len();
    let mount_point_id = str_field(&character, "characterDocumentMountPointId");

    // Resolve the encryption passphrase up front (§4.2c) — a named failure here
    // beats discovering it after the export has already streamed.
    let passphrase = resolve_archive_passphrase(seams.passphrase).map_err(ArchiveError::Crypto)?;

    let bundle = create_archive_bundle(db, user_id, character_id, &passphrase, seams)?;
    verify_archive_bundle(
        db,
        &bundle,
        character_id,
        memory_count,
        mount_point_id.as_deref(),
    )?;

    // Everything from here to the commit is undone on failure: an orphan ARCHIVE
    // file per failed attempt would pile up in the library unnoticed.
    let mut written: Vec<String> = Vec::new();
    let outcome = async {
        let archive_file_id = create_archive_file_record(db, user_id, &bundle, seams).await?;
        written.push(archive_file_id.clone());

        flip_character_participants(db, character_id, ParticipantFlip::ToAbsent).await;

        // The commit. Note what is NOT here: characterDocumentMountPointId (the
        // vault survives the archive), defaultImageId and avatarOverrides (they
        // point at blobs the prune keeps — old messages keep their faces).
        let patch: Map<String, Value> = json!({
            "archivedAt": crate::clock::now_iso(),
            "archiveFileId": archive_file_id,
            "defaultPartnerId": Value::Null,
            "defaultConnectionProfileId": Value::Null,
            "defaultImageProfileId": Value::Null,
            "defaultRoleplayTemplateId": Value::Null,
        })
        .as_object()
        .cloned()
        .expect("literal object");
        update_character_patch(db, character_id, patch).await?;

        let prune_complete = prune_archived_character_state(db, character_id).await?;
        Ok::<_, ArchiveError>(ArchiveCharacterResult {
            archived: true,
            archive_file_id: Some(archive_file_id),
            prune_complete,
        })
    }
    .await;

    match outcome {
        Ok(result) => Ok(result),
        Err(e) => {
            discard_written_files(db, &written).await;
            Err(e)
        }
    }
}

// ---------------------------------------------------------------------------
// rehydrateCharacter
// ---------------------------------------------------------------------------

/// Bring an archived character back (WP B4 / spec §6) — v4
/// `rehydrateCharacter`.
///
/// §4.2a made this "restore the pruned material back into a mount that still
/// exists", not "import a bundle into empty space": the mount point, the ten
/// managed documents, the avatar and the wardrobe never left, and everything
/// the prune deleted comes back at its original ids.
///
/// Any failure before step 3's clear throws and leaves the character archived
/// with the bundle intact. The bundle file stays in the library afterwards
/// (cheap insurance); the UI offers its deletion.
///
/// A tombstone with no bundle (nothing was ever packed, so nothing was pruned)
/// is simply un-flagged.
pub async fn rehydrate_character(
    db: &Db,
    user_id: &str,
    character_id: &str,
    seams: &ArchiveSeams<'_>,
) -> Result<RehydrateCharacterResult, ArchiveError> {
    let cid = character_id.to_string();
    let character = db
        .read_main(move |c| characters_read::find_by_id_raw(c, &cid))?
        .ok_or_else(|| ArchiveError::CharacterNotFound(character_id.to_string()))?;

    if str_field(&character, "archivedAt").is_none() {
        return Err(ArchiveError::NotArchived(character_id.to_string()));
    }

    let archive_file_id = str_field(&character, "archiveFileId");
    let archived_avatar_file_id = str_field(&character, "archivedAvatarFileId");
    let vault_id = str_field(&character, "characterDocumentMountPointId");

    let mut restored: Option<RestoredCounts> = None;
    let mut warnings: Vec<String> = Vec::new();

    if let Some(bundle_id) = archive_file_id.as_deref() {
        let outcome = restore_archive_bundle(
            db,
            user_id,
            character_id,
            vault_id.as_deref(),
            bundle_id,
            seams,
        )
        .await?;
        restored = Some(outcome.0);
        warnings = outcome.1;
    }

    // The commit. The §4.4 guard sanctions exactly one patch on an archived
    // row — { archivedAt: null } alone — so the pointer cleanup rides second.
    update_character_patch(
        db,
        character_id,
        json!({"archivedAt": Value::Null})
            .as_object()
            .cloned()
            .expect("literal object"),
    )
    .await?;

    let mut follow_up = Map::new();
    if archive_file_id.is_some() {
        follow_up.insert("archiveFileId".into(), Value::Null);
    }
    if archived_avatar_file_id.is_some() {
        follow_up.insert("archivedAvatarFileId".into(), Value::Null);
    }
    if !follow_up.is_empty() {
        update_character_patch(db, character_id, follow_up).await?;
    }

    // A pre-revision tombstone carried a standalone avatar thumbnail; the live
    // character resolves its face through the vault again, so the thumbnail
    // row goes (§6 step 4). Cosmetic — never fails the rehydrate.
    if let Some(thumb) = archived_avatar_file_id {
        let fid = thumb.clone();
        if let Err(e) = db
            .write(move |ws| FilesRepository::new(ws.main().connection()).delete(&fid))
            .await
        {
            tracing::warn!(
                target: "quilltap::archive",
                character_id,
                file_id = %thumb,
                error = %e,
                "Failed to delete pre-revision archived-avatar thumbnail",
            );
        }
    }

    flip_character_participants(db, character_id, ParticipantFlip::ToPresent).await;

    // Re-chunk + re-embed the restored vault content (§6 step 5). Non-fatal:
    // the boot reconcile sweep is the backstop, and the character is already
    // live either way.
    if let (Some(_), Some(mount_point_id)) = (archive_file_id.as_deref(), vault_id.as_deref()) {
        rechunk_and_embed_vault(db, character_id, mount_point_id, seams).await;
    }

    tracing::info!(
        target: "quilltap::archive",
        character_id,
        archive_bundle_file_id = ?archive_file_id,
        warning_count = warnings.len(),
        "Character rehydrated from archive",
    );

    Ok(RehydrateCharacterResult {
        rehydrated: true,
        archived: false,
        archive_bundle_file_id: archive_file_id,
        restored,
        warnings,
    })
}

/// §6 steps 1–3: load, decrypt, verify, and import the bundle — v4
/// `restoreArchiveBundle`. Errors leave the character archived and the bundle
/// untouched.
async fn restore_archive_bundle(
    db: &Db,
    user_id: &str,
    character_id: &str,
    vault_mount_point_id: Option<&str>,
    archive_file_id: &str,
    seams: &ArchiveSeams<'_>,
) -> Result<(RestoredCounts, Vec<String>), ArchiveError> {
    let fid = archive_file_id.to_string();
    let file_row = db
        .read_main(move |c| FilesRepository::new(c).find_full_by_id(&fid))?
        .ok_or_else(|| ArchiveError::Rehydration {
            character_id: character_id.to_string(),
            detail: format!(
                "its archive bundle (file {archive_file_id}) is missing from the library. \
                 If the bundle is gone for good, the pruned material is unrecoverable; \
                 delete the character to let them go."
            ),
        })?;

    let backend = seams.backend.as_deref().ok_or_else(|| {
        ArchiveError::Storage(
            "File storage backend not available. Initialize the manager first.".to_string(),
        )
    })?;
    let entry = FileEntry {
        id: file_row.id.clone(),
        sha256: file_row.sha256.clone(),
        original_filename: file_row.original_filename.clone(),
        mime_type: file_row.mime_type.clone(),
        size: file_row.size,
        width: file_row.width,
        height: file_row.height,
        category: file_row.category.clone(),
        generation_prompt: None,
        generation_model: None,
        generation_revised_prompt: None,
        description: file_row.description.clone(),
        storage_key: file_row.storage_key.clone(),
    };
    let raw = crate::services::file_storage::download_file(db, backend, &entry)
        .map_err(ArchiveError::Storage)?;

    // §4.2c: bundles written since the encryption revision carry the QTAPARC1
    // magic; a pre-encryption bundle is plaintext NDJSON and passes through.
    // v4's `decryptArchive(raw)` resolves the instance passphrase itself and
    // diagnoses a wrong one as ArchivePassphraseMismatchError ("predates your
    // passphrase change") via the header's key hash, before touching the
    // ciphertext — v5 resolves it here, one call earlier, from the same source.
    // `raw` stays alive past the decrypt: the bug-69 self-heal below needs the
    // digest of the bytes AS STORED to tell a clobbered row from a corrupt one.
    let plaintext: std::borrow::Cow<'_, [u8]> = if is_encrypted_archive(&raw) {
        let passphrase =
            resolve_archive_passphrase(seams.passphrase).map_err(ArchiveError::Crypto)?;
        std::borrow::Cow::Owned(decrypt_archive(&raw, &passphrase).map_err(ArchiveError::Crypto)?)
    } else {
        std::borrow::Cow::Borrowed(&raw)
    };

    // The file row's sha256 is the plaintext digest (§4.2d) — the one check
    // that survives re-encryption and actually verifies content.
    let digest = sha256_hex(&plaintext);
    let mut clobber_warnings: Vec<String> = Vec::new();
    if !file_row.sha256.is_empty() && digest != file_row.sha256 {
        // Bug 69 self-heal (v4 `aa464abf`). Until the file watcher learned to
        // leave content digests alone, it re-derived this row's sha256 from the
        // encrypted bytes seconds after the archive was written, and every
        // rehydrate afterwards failed here. If the recorded digest is exactly
        // the digest of the file as stored, the bundle is intact and the ROW is
        // what was damaged: repair it and carry on. Any other mismatch is a
        // genuinely corrupt bundle.
        //
        // ⚠ No v5 code path can CAUSE this: v5 has no file-storage watcher and
        // no boot reconciliation (`api::files::files_sync` refuses loudly), and
        // the only writers of `files.sha256` are the same-name upload overwrite
        // and image generation. The arm exists because v5 opens the same
        // instances a pre-4.9 v4 damaged.
        let stored_digest = sha256_hex(&raw);
        if file_row.sha256 != stored_digest {
            return Err(ArchiveError::Verification(format!(
                "the bundle's decrypted content does not match its recorded digest \
                 (expected {}, got {digest}) — the bundle is corrupt",
                file_row.sha256
            )));
        }
        tracing::warn!(
            target: "quilltap::archive",
            character_id,
            archive_file_id,
            recorded_digest = %digest_prefix(&file_row.sha256),
            plaintext_digest = %digest_prefix(&digest),
            "Repairing an archive row whose plaintext digest was overwritten with the ciphertext digest (bug 69)",
        );
        clobber_warnings.push(
            "The bundle's recorded digest had been overwritten with the digest of its \
             encrypted bytes; the contents verified against the file as stored, and the \
             record has been repaired."
                .to_string(),
        );
        // v4 repairs `sha256` ALONE — an archive row's `size` is the real
        // on-disk (encrypted) byte count and was never wrong.
        let repair_id = archive_file_id.to_string();
        let repaired = digest.clone();
        let repair = db
            .write(move |ws| {
                FilesRepository::new(ws.main().connection()).update(
                    &repair_id,
                    &crate::db::files::FileUpdate {
                        sha256: Some(repaired),
                        updated_at: crate::clock::now_iso(),
                        ..Default::default()
                    },
                )
            })
            .await;
        if let Err(error) = repair {
            // A repair failure is not fatal: the bundle verified, so the
            // rehydrate continues and the row simply heals on a later run.
            tracing::warn!(
                target: "quilltap::archive",
                character_id,
                archive_file_id,
                error = %error,
                "Failed to repair the archive row digest; the rehydrate continues",
            );
        }
    }

    let export_data = parse_bundle(&plaintext)?;

    let manifest = &export_data.manifest;
    let format_ok = manifest.get("format").and_then(Value::as_str) == Some("quilltap-export");
    let preserve_ok =
        manifest.get("settings").and_then(|s| s.get("preserveIds")) == Some(&Value::Bool(true));
    if !format_ok || !preserve_ok {
        return Err(ArchiveError::Verification(
            "the bundle is not a preserve-ids character archive — its manifest is missing \
             or was written without preserveIds"
                .to_string(),
        ));
    }

    let characters = bundle_array(&export_data, "characters");
    if characters.len() != 1
        || characters
            .first()
            .and_then(|c| c.get("id"))
            .and_then(Value::as_str)
            != Some(character_id)
    {
        return Err(ArchiveError::Verification(format!(
            "the bundle does not carry character {character_id} (it holds {} character record(s))",
            characters.len()
        )));
    }

    let options = ImportOptions {
        conflict_strategy: ConflictStrategy::Skip,
        include_memories: true,
        include_related_entities: false,
        selected_ids: None,
        preserve_ids: true,
        preserve_ids_mode: PreserveIdsMode::SkipIfPresent {
            target_character_id: character_id.to_string(),
            target_vault_mount_point_id: vault_mount_point_id.map(str::to_string),
        },
    };

    let uid = user_id.to_string();
    let codec = seams.codec.clone();
    let result = db
        .write(move |ws| {
            let main = ws.main().connection();
            let Some(mi) = ws.mount_index() else {
                return Err(DbError::PartitionUnavailable(
                    crate::write_partition::WriteDbTarget::MountIndex,
                ));
            };
            execute_import(
                main,
                mi.connection(),
                &uid,
                &export_data,
                &options,
                codec.as_deref(),
            )
            .map_err(|e| match e {
                crate::services::quilltap_import::ImportError::Db(d) => d,
                other => DbError::Internal(other.to_string()),
            })
        })
        .await?;

    if !result.success {
        // Failing before the tombstone clear is the whole contract (§6): the
        // character stays archived, the bundle stays put, and because the import
        // is additive at preserved ids a re-run skips whatever did land.
        let detail = if result.warnings.is_empty() {
            "unknown import error".to_string()
        } else {
            result.warnings.join("; ")
        };
        // Say "rehydrate" in the log. The importer's own warning names only the
        // import module, so a failed rehydrate otherwise leaves no log line that
        // grepping for the operation would ever find.
        tracing::error!(
            target: "quilltap::archive",
            character_id,
            archive_file_id,
            detail = %detail,
            "Rehydrate failed: the archive bundle import was refused",
        );
        return Err(ArchiveError::Rehydration {
            character_id: character_id.to_string(),
            detail: format!("the bundle import failed: {detail}"),
        });
    }

    Ok((
        RestoredCounts {
            memories: result.imported.memories,
            documents: result.imported.document_store_documents.unwrap_or(0),
            blobs: result.imported.document_store_blobs.unwrap_or(0),
        },
        // v4 `[...clobberWarnings, ...result.warnings]` — the repair warning
        // leads, because it is about the bundle rather than about the import.
        clobber_warnings
            .into_iter()
            .chain(result.warnings)
            .collect(),
    ))
}

/// v4's `digest.slice(0, 12) + '...'` log form, without assuming the string is
/// 64 hex characters (a clobbered row is whatever was written over it).
fn digest_prefix(digest: &str) -> String {
    let head: String = digest.chars().take(12).collect();
    format!("{head}...")
}

/// §6 step 5: restored text documents come back with no chunks —
/// `reindexLinks` without `force` chunks exactly the links whose chunkCount is
/// 0, leaving the keep-set (which never lost its chunks) alone — then the
/// embedding scheduler enqueues jobs for every un-embedded chunk in the mount.
///
/// Whole-function fail-soft, exactly as v4's try/warn.
async fn rechunk_and_embed_vault(
    db: &Db,
    character_id: &str,
    mount_point_id: &str,
    seams: &ArchiveSeams<'_>,
) {
    let mp_id = mount_point_id.to_string();
    let extractor = Arc::clone(&seams.extractor);
    let step = db
        .write(move |ws| {
            let main_conn = ws.main().connection();
            let Some(mi) = ws.mount_index() else {
                return Err(DbError::PartitionUnavailable(
                    crate::write_partition::WriteDbTarget::MountIndex,
                ));
            };
            let mount = mi.connection();
            if let Some(mp) = crate::api::mount_files::load_mount_service_info(mount, &mp_id)? {
                crate::services::mount_index::reindex::reindex_links(
                    mount,
                    &mp,
                    &crate::services::mount_index::reindex::ReindexOptions {
                        path: None,
                        force: false,
                    },
                    extractor.as_ref(),
                )?;
                DocMountPointsRepository::new(mount).refresh_stats(&mp_id)?;
            }
            crate::services::mount_index::embedding_scheduler::enqueue_embedding_jobs_for_mount_point(
                main_conn, mount, &mp_id,
            )?;
            Ok(())
        })
        .await;
    if let Err(e) = step {
        tracing::warn!(
            target: "quilltap::archive",
            character_id,
            mount_point_id,
            error = %e,
            "Failed to re-chunk or re-embed the rehydrated vault; the boot reconcile will catch up",
        );
    }
}

// ---------------------------------------------------------------------------
// The bundle
// ---------------------------------------------------------------------------

/// v4 `ArchiveBundle`.
pub struct ArchiveBundle {
    /// The NDJSON export in the clear — hashed, verified, never persisted.
    pub plaintext: Vec<u8>,
    /// What actually lands on disk (§4.2c).
    pub encrypted: Vec<u8>,
    pub export_data: QuilltapExport,
}

/// Run the live characters export into a buffer, encrypt it (§4.2c), then
/// decrypt the ciphertext and read *that* back — so verification covers the
/// artifact that will actually be persisted, encryption round-trip included,
/// rather than what we meant to write. (v4 `createArchiveBundle`.)
fn create_archive_bundle(
    db: &Db,
    user_id: &str,
    character_id: &str,
    passphrase: &str,
    seams: &ArchiveSeams<'_>,
) -> Result<ArchiveBundle, ArchiveError> {
    let options = ExportOptions {
        entity_type: "characters".to_string(),
        scope: "selected".to_string(),
        selected_ids: vec![character_id.to_string()],
        include_memories: true,
    };
    let uid = user_id.to_string();
    let created_at = crate::clock::now_iso();
    let app_version = seams.app_version.clone();
    let storage = seams.backend.as_deref();
    let records = db.read_main(|main| {
        db.read_mount_index(|mount| {
            stream_export_records(
                main,
                mount,
                storage,
                &uid,
                &options,
                // §4.2b: the bundle is the ONE export written with
                // preserveIds — rehydration restores at original ids.
                true,
                &created_at,
                &app_version,
            )
            .map_err(|e| DbError::Internal(e.to_string()))
        })
    })?;

    let plaintext = crate::services::qtap_export::records_to_ndjson(&records).into_bytes();
    let encrypted = encrypt_archive(&plaintext, passphrase, None).map_err(ArchiveError::Crypto)?;
    let round_tripped = decrypt_archive(&encrypted, passphrase).map_err(ArchiveError::Crypto)?;
    let export_data = parse_bundle(&round_tripped)?;

    Ok(ArchiveBundle {
        plaintext,
        encrypted,
        export_data,
    })
}

/// v4's `assembleExportFromStream(readNdjsonLines(bufferToByteStream(buf)))`
/// chain, collapsed: v5's reader takes the parsed records directly, so the
/// single-chunk `ReadableStream` shim (`bufferToByteStream`) has no analog.
///
/// The reader's own truncation checks run here, so a blob missing chunks
/// refuses before verification even looks at counts — v4's note, preserved.
fn parse_bundle(bytes: &[u8]) -> Result<QuilltapExport, ArchiveError> {
    let records = read_ndjson_lines(bytes).map_err(ArchiveError::Verification)?;
    assemble_export_from_stream(&records).map_err(ArchiveError::Verification)
}

/// The hard gate before anything is deleted (parent §11.1) — v4
/// `verifyArchiveBundle`.
///
/// Checks three things, in the order they'd betray a problem:
///   - the bundle carries the character and every one of its memories;
///   - it carries the vault — mount point, text documents and blobs — matching
///     what the live store actually holds;
///   - the footer counts agree with the records that were parsed, so a bundle
///     that under-reports itself can't slip through.
fn verify_archive_bundle(
    db: &Db,
    bundle: &ArchiveBundle,
    character_id: &str,
    expected_memory_count: usize,
    mount_point_id: Option<&str>,
) -> Result<(), ArchiveError> {
    let counts = bundle.export_data.manifest.get("counts");
    let count_of = |key: &str| -> i64 {
        counts
            .and_then(|c| c.get(key))
            .and_then(Value::as_i64)
            .unwrap_or(0)
    };

    let characters = bundle_array(&bundle.export_data, "characters");
    let memories = bundle_array(&bundle.export_data, "memories");
    let mount_points = bundle_array(&bundle.export_data, "mountPoints");
    let documents = bundle_array(&bundle.export_data, "documents");
    let blobs = bundle_array(&bundle.export_data, "blobs");

    if characters.len() != 1
        || characters
            .first()
            .and_then(|c| c.get("id"))
            .and_then(Value::as_str)
            != Some(character_id)
    {
        return Err(ArchiveError::Verification(format!(
            "expected exactly the character {character_id}, got {} character record(s)",
            characters.len()
        )));
    }

    if memories.len() != expected_memory_count {
        return Err(ArchiveError::Verification(format!(
            "memory count mismatch: bundle has {}, character has {expected_memory_count}",
            memories.len()
        )));
    }

    // Footer honesty: the counts a reader will trust must match what is there.
    assert_count("characters", count_of("characters"), characters.len())?;
    assert_count("memories", count_of("memories"), memories.len())?;
    assert_count(
        "documentStores",
        count_of("documentStores"),
        mount_points.len(),
    )?;
    assert_count(
        "documentStoreDocuments",
        count_of("documentStoreDocuments"),
        documents.len(),
    )?;
    assert_count(
        "documentStoreBlobs",
        count_of("documentStoreBlobs"),
        blobs.len(),
    )?;

    let Some(mount_point_id) = mount_point_id else {
        if !mount_points.is_empty() {
            return Err(ArchiveError::Verification(format!(
                "character has no vault but the bundle carries {} store(s)",
                mount_points.len()
            )));
        }
        return Ok(());
    };

    let live = count_live_vault(db, mount_point_id)?;
    if mount_points.len() != 1 {
        return Err(ArchiveError::Verification(format!(
            "expected the character's vault in the bundle, got {} store(s)",
            mount_points.len()
        )));
    }
    if documents.len() != live.0 {
        return Err(ArchiveError::Verification(format!(
            "vault document count mismatch: bundle has {}, vault has {}",
            documents.len(),
            live.0
        )));
    }
    if blobs.len() != live.1 {
        return Err(ArchiveError::Verification(format!(
            "vault blob count mismatch: bundle has {}, vault has {}",
            blobs.len(),
            live.1
        )));
    }
    Ok(())
}

fn assert_count(label: &str, reported: i64, actual: usize) -> Result<(), ArchiveError> {
    if reported != actual as i64 {
        return Err(ArchiveError::Verification(format!(
            "footer count for {label} says {reported}, bundle carries {actual}"
        )));
    }
    Ok(())
}

/// What the live vault holds, filtered the way the writer filters it — v4
/// `countLiveVault`. Returns `(documents, blobs)`.
fn count_live_vault(db: &Db, mount_point_id: &str) -> Result<(usize, usize), DbError> {
    let mp = mount_point_id.to_string();
    db.read_mount_index(move |mount| {
        let documents = DocMountDocumentsRepository::new(mount)
            .find_file_types_by_mount_point_id(&mp)?
            .into_iter()
            .filter(|ft| TEXT_DOCUMENT_FILE_TYPES.contains(&ft.as_str()))
            .count();
        let blobs = DocMountBlobsRepository::new(mount)
            .list_by_mount_point(&mp, None)?
            .len();
        Ok((documents, blobs))
    })
}

/// Create the ARCHIVE `files` row and upload the encrypted bytes — v4
/// `createArchiveFileRecord`.
///
/// The recorded sha256 is the digest of the **plaintext** bundle (§4.2d step
/// 1), not the ciphertext: this row is the only copy of an archived
/// character's pruned material, and the plaintext digest is what rehydration
/// verifies after decrypting — a ciphertext digest would change on every
/// re-encryption (passphrase change) and verify nothing about the content.
/// `size` is the real on-disk (encrypted) byte count.
async fn create_archive_file_record(
    db: &Db,
    user_id: &str,
    bundle: &ArchiveBundle,
    seams: &ArchiveSeams<'_>,
) -> Result<String, ArchiveError> {
    let sha256 = sha256_hex(&bundle.plaintext);
    let mime_type = "application/octet-stream";

    // Mint the id up front so the row carries its storageKey from the first
    // write. The bytes land on disk *after* the row exists, so the file
    // watcher's add event finds the record by storageKey and stays silent —
    // creating the row keyless and patching it post-upload raced the watcher
    // into adopting the fresh bundle as an orphaned DOCUMENT.
    //
    // ⚠ v4 LORE, kept deliberately: v5 ports no file watcher (`files_sync`
    // refuses loudly), so the race this defends against cannot happen here —
    // but the ORDER it forces (row before bytes) is still correct and still
    // load-bearing for v4-written instances. v4's watcher had a second hole in
    // this same area, bug 69: it re-derived `sha256` from the encrypted bytes
    // and clobbered the plaintext digest recorded just below. That one IS
    // reachable in v5 — through damaged rows, not through v5 code — and
    // `restore_archive_bundle` self-heals it.
    let file_id = uuid::Uuid::new_v4().to_string();
    let storage_key = format!("{file_id}/character-archive.qtap");
    let now = crate::clock::now_iso();
    let original_filename = format!("{}-character-archive.qtap", now.replace([':', '.'], "-"));

    let create = FileCreate {
        user_id: user_id.to_string(),
        sha256,
        original_filename,
        mime_type: mime_type.to_string(),
        size: bundle.encrypted.len() as f64,
        width: None,
        height: None,
        is_plain_text: None,
        linked_to: Vec::new(),
        source: "SYSTEM".to_string(),
        category: super::reencrypt::ARCHIVE_CATEGORY.to_string(),
        generation_prompt: None,
        generation_model: None,
        generation_revised_prompt: None,
        description: None,
        tags: Vec::new(),
        project_id: None,
        folder_path: Some("/archives".to_string()),
        storage_key: Some(storage_key.clone()),
        file_status: "ok".to_string(),
    };
    let opts = FileCreateOptions {
        id: file_id.clone(),
        created_at: now.clone(),
        updated_at: now,
    };
    db.write(move |ws| FilesRepository::new(ws.main().connection()).create(&create, &opts))
        .await?;

    let backend = seams.backend.as_deref().ok_or_else(|| {
        ArchiveError::Storage(
            "File storage backend not available. Initialize the manager first.".to_string(),
        )
    })?;
    crate::services::file_storage::upload_raw(backend, &storage_key, &bundle.encrypted, mime_type)
        .map_err(ArchiveError::Storage)?;

    Ok(file_id)
}

/// Roll back the file rows a failed pre-commit attempt created — v4
/// `discardWrittenFiles`.
async fn discard_written_files(db: &Db, file_ids: &[String]) {
    for file_id in file_ids {
        let fid = file_id.clone();
        if let Err(e) = db
            .write(move |ws| FilesRepository::new(ws.main().connection()).delete(&fid))
            .await
        {
            tracing::warn!(
                target: "quilltap::archive",
                file_id = %file_id,
                error = %e,
                "Failed to discard an archive file after a failed archive attempt",
            );
        }
    }
}

// ---------------------------------------------------------------------------
// The participant flips
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
enum ParticipantFlip {
    ToAbsent,
    ToPresent,
}

/// v4 `flipCharacterParticipantsToAbsent` / `…ToPresent` (§4.5).
///
/// The archive flip's inverse touches only `absent` seats — a `removed`
/// participant left the chat before the archive and stays gone. Both are
/// best-effort: a stubborn seat must not fail the operation.
async fn flip_character_participants(db: &Db, character_id: &str, flip: ParticipantFlip) {
    let cid = character_id.to_string();
    let chats = match db.read_main(move |c| chats_read::find_by_character_id(c, &cid)) {
        Ok(rows) => rows,
        Err(e) => {
            warn_flip(character_id, flip, &e.to_string());
            return;
        }
    };

    let mut targets: Vec<(String, String)> = Vec::new();
    for chat in &chats {
        let Some(chat_id) = chat.get("id").and_then(Value::as_str) else {
            continue;
        };
        let participants = chat
            .get("participants")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        for p in participants {
            if p.get("characterId").and_then(Value::as_str) != Some(character_id) {
                continue;
            }
            if p.get("type").and_then(Value::as_str) != Some("CHARACTER") {
                continue;
            }
            if matches!(flip, ParticipantFlip::ToPresent)
                && p.get("status").and_then(Value::as_str) != Some("absent")
            {
                continue;
            }
            let Some(pid) = p.get("id").and_then(Value::as_str) else {
                continue;
            };
            targets.push((chat_id.to_string(), pid.to_string()));
        }
    }
    if targets.is_empty() {
        return;
    }

    let new_status = match flip {
        ParticipantFlip::ToAbsent => "absent",
        ParticipantFlip::ToPresent => "active",
    };
    // v4 `Promise.allSettled` — every seat gets its attempt and a failure is
    // swallowed. v5 applies them in one write session, in the same order.
    let write = db
        .write(move |ws| {
            let conn = ws.main().connection();
            for (chat_id, participant_id) in &targets {
                let _ = crate::db::chats::ChatsRepository::new(conn).set_participant_status(
                    chat_id,
                    participant_id,
                    new_status,
                );
            }
            Ok(())
        })
        .await;
    if let Err(e) = write {
        warn_flip(character_id, flip, &e.to_string());
    }
}

fn warn_flip(character_id: &str, flip: ParticipantFlip, error: &str) {
    match flip {
        ParticipantFlip::ToAbsent => tracing::warn!(
            target: "quilltap::archive",
            character_id,
            error,
            "Failed to flip character participants to absent during archive",
        ),
        ParticipantFlip::ToPresent => tracing::warn!(
            target: "quilltap::archive",
            character_id,
            error,
            "Failed to flip character participants back to active during rehydrate",
        ),
    }
}

// ---------------------------------------------------------------------------
// The prune
// ---------------------------------------------------------------------------

/// Idempotent prune of everything the bundle now carries (§4.2a) — v4
/// `pruneArchivedCharacterState`.
///
/// Every step reports whether it succeeded and the result is their conjunction —
/// a caller must be able to tell "archived, prune incomplete" from "archived,
/// done", or the failure semantics are a fiction. Steps do not abort each other:
/// a vault link that refuses to delete should not also strand the memories.
async fn prune_archived_character_state(db: &Db, character_id: &str) -> Result<bool, ArchiveError> {
    let steps = [
        delete_own_memories(db, character_id).await,
        delete_vector_store(db, character_id).await,
        prune_vault(db, character_id).await,
    ];
    let complete = steps.iter().all(|s| *s);
    if !complete {
        tracing::warn!(
            target: "quilltap::archive",
            character_id,
            "Archive prune incomplete; re-run archiveCharacter to finish",
        );
    }
    Ok(complete)
}

async fn delete_own_memories(db: &Db, character_id: &str) -> bool {
    let cid = character_id.to_string();
    let outcome = db
        .write(move |ws| {
            let conn = ws.main().connection();
            let memories = memories_read::find_by_character_id(conn, &cid)?;
            if memories.is_empty() {
                return Ok(());
            }
            let ids: Vec<String> = memories
                .iter()
                .filter_map(|m| m.get("id").and_then(Value::as_str).map(str::to_string))
                .collect();
            // Through the gate, never repos.memories.delete*: it scrubs the
            // deleted ids out of surviving neighbours' relatedMemoryIds first.
            MemoriesRepository::new(conn).delete_many_with_unlink(&ids)?;
            let statuses = EmbeddingStatusRepository::new(conn);
            for id in &ids {
                statuses.delete_by_entity("MEMORY", id)?;
            }
            Ok(())
        })
        .await;
    match outcome {
        Ok(()) => true,
        Err(e) => {
            tracing::warn!(
                target: "quilltap::archive",
                character_id,
                error = %e,
                "Failed to delete memories for archived character",
            );
            false
        }
    }
}

async fn delete_vector_store(db: &Db, character_id: &str) -> bool {
    let cid = character_id.to_string();
    let outcome = db
        .write(move |ws| {
            VectorIndicesRepository::new(ws.main().connection()).delete_by_character_id(&cid)
        })
        .await;
    match outcome {
        Ok(_) => true,
        Err(e) => {
            tracing::warn!(
                target: "quilltap::archive",
                character_id,
                error = %e,
                "Failed to delete vector store for archived character",
            );
            false
        }
    }
}

/// Prune the vault in place — v4 `pruneVault`: delete every link outside the
/// keep-set, then any folder left with nothing kept beneath it.
///
/// The keep-set (§4.2a): the ten managed-field documents, everything under
/// `Wardrobe/`, and the avatar links — `defaultImageId` plus every
/// `avatarOverrides[].imageId`. Those are ids old chat messages resolve
/// through; keeping the rows is what keeps their faces.
///
/// Deletion goes through `deleteWithGC` — the document-store delete path — so
/// chunks cascade, the underlying file row is ref-counted before it is
/// collected, and link-group orphan GC runs. Never `deleteStoreCascade`: that
/// tears down the whole store, which is exactly what this revision stopped
/// doing. The mount point and `characterDocumentMountPointId` are never touched.
///
/// Honesty check: after the loop the surviving links are re-listed; any doomed
/// link still present (deleteWithGC swallows backend errors into a no-op)
/// fails the step so the caller reports "prune incomplete".
async fn prune_vault(db: &Db, character_id: &str) -> bool {
    let cid = character_id.to_string();
    let outcome = db
        .write(move |ws| {
            let main = ws.main().connection();
            // Raw read (global repo — the user-scoped one doesn't expose it):
            // the prune needs only pointer + avatar ids, and must work even if
            // the overlay is somehow broken mid-archive. Ownership was checked
            // at entry.
            let character = characters_read::find_by_id_raw(main, &cid)?;
            let mount_point_id = character
                .as_ref()
                .and_then(|c| str_field(c, "characterDocumentMountPointId"));
            let Some(mount_point_id) = mount_point_id else {
                // No vault: a pre-§4.2a tombstone whose vault was deleted
                // outright, or a character that never had one. Nothing to prune.
                return Ok(0usize);
            };

            let mut kept_link_ids: HashSet<String> = HashSet::new();
            if let Some(c) = character.as_ref() {
                if let Some(id) = str_field(c, "defaultImageId") {
                    kept_link_ids.insert(id);
                }
                for o in c
                    .get("avatarOverrides")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default()
                {
                    if let Some(id) = o.get("imageId").and_then(Value::as_str) {
                        kept_link_ids.insert(id.to_string());
                    }
                }
            }

            let Some(mi) = ws.mount_index() else {
                return Err(DbError::PartitionUnavailable(
                    crate::write_partition::WriteDbTarget::MountIndex,
                ));
            };
            let mount = mi.connection();

            let wardrobe_prefix = format!("{}/", CHARACTER_WARDROBE_FOLDER.to_lowercase());
            let managed = kept_managed_paths();
            let is_kept = |id: &str, relative_path: &str| -> bool {
                if kept_link_ids.contains(id) {
                    return true;
                }
                let path = relative_path.to_lowercase();
                managed.contains(&path) || path.starts_with(&wardrobe_prefix)
            };

            let links_repo = DocMountFileLinksRepository::new(mount);
            let links = links_repo.find_by_mount_point_id(&mount_point_id)?;
            let doomed: Vec<(String, String)> = links
                .iter()
                .filter(|l| !is_kept(&l.id, &l.relative_path))
                .map(|l| (l.id.clone(), l.relative_path.clone()))
                .collect();

            let chunks_repo = DocMountChunksRepository::new(mount);
            let statuses = EmbeddingStatusRepository::new(main);
            for (link_id, _) in &doomed {
                // Chunk ids first: the delete cascades them away, and their
                // embedding_status rows would otherwise linger as permanent
                // orphans.
                let chunk_ids = chunks_repo.find_ids_by_link_id(link_id)?;
                links_repo.delete_with_gc(link_id)?;
                for chunk_id in chunk_ids {
                    statuses.delete_by_entity("MOUNT_CHUNK", &chunk_id)?;
                }
            }

            let survivors = links_repo.find_by_mount_point_id(&mount_point_id)?;
            let undead = survivors
                .iter()
                .filter(|l| !is_kept(&l.id, &l.relative_path))
                .count();
            if undead > 0 {
                tracing::warn!(
                    target: "quilltap::archive",
                    character_id = %cid,
                    mount_point_id = %mount_point_id,
                    remaining = undead,
                    "Vault prune left doomed links behind",
                );
                return Err(DbError::Internal(
                    "prune left doomed links behind".to_string(),
                ));
            }

            prune_empty_folders(mount, &mount_point_id, &survivors, &wardrobe_prefix)?;

            tracing::debug!(
                target: "quilltap::archive",
                character_id = %cid,
                mount_point_id = %mount_point_id,
                deleted = doomed.len(),
                kept = survivors.len(),
                "Pruned vault for archived character",
            );
            Ok(doomed.len())
        })
        .await;
    match outcome {
        Ok(_) => true,
        Err(e) => {
            // v4's `pruneVault` returns false for BOTH the undead-links honesty
            // check and any thrown error; the two arms differ only in the log
            // line, which the closure already emitted for the former.
            tracing::warn!(
                target: "quilltap::archive",
                character_id,
                error = %e,
                "Failed to prune vault for archived character",
            );
            false
        }
    }
}

/// Delete folder rows with no kept content beneath them (`Mail/`,
/// `Conversation Summaries/`, photo folders, …) — v4 `pruneEmptyFolders`. The
/// `Wardrobe/` folder is in the keep-set explicitly — it survives even when
/// empty — and so does any folder that still shelters a surviving link.
fn prune_empty_folders(
    mount: &rusqlite::Connection,
    mount_point_id: &str,
    survivors: &[crate::db::doc_mount_file_links::LinkRow],
    wardrobe_prefix: &str,
) -> Result<(), DbError> {
    let kept_paths: Vec<String> = survivors
        .iter()
        .map(|l| l.relative_path.to_lowercase())
        .collect();
    let wardrobe_folder = &wardrobe_prefix[..wardrobe_prefix.len() - 1];

    let folders_repo = DocMountFoldersRepository::new(mount);
    let folders = folders_repo.find_by_mount_point_id(mount_point_id)?;
    for folder in folders {
        let folder_path = folder.path.to_lowercase();
        if folder_path.is_empty()
            || folder_path == wardrobe_folder
            || folder_path.starts_with(wardrobe_prefix)
        {
            continue;
        }
        let shelters = kept_paths
            .iter()
            .any(|p| p.starts_with(&format!("{folder_path}/")));
        if !shelters {
            folders_repo.delete(&folder.id)?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Small helpers
// ---------------------------------------------------------------------------

/// A present, non-null, non-EMPTY string field. v4 reads these through JS
/// truthiness (`character.archivedAt`, `character.archiveFileId ?? null`), so
/// an empty string is "absent" on both sides.
fn str_field(v: &Value, key: &str) -> Option<String> {
    v.get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// The parsed bundle's per-collection array (`data.characters` etc.), `[]` when
/// the key is absent — v4's `?? []`.
fn bundle_array<'a>(export: &'a QuilltapExport, key: &str) -> &'a [Value] {
    export
        .data
        .get(key)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

/// v4 `repos.characters.update(id, patch)` — through the vault write overlay so
/// the §4.4 archive guard runs at v4's own placement.
async fn update_character_patch(
    db: &Db,
    character_id: &str,
    patch: Map<String, Value>,
) -> Result<(), ArchiveError> {
    let cid = character_id.to_string();
    db.write(move |ws| {
        let main = ws.main().connection();
        let Some(mi) = ws.mount_index() else {
            return Err(DbError::PartitionUnavailable(
                crate::write_partition::WriteDbTarget::MountIndex,
            ));
        };
        vault_character_update::update_character(main, mi.connection(), &cid, &patch).map_err(
            |e| match e {
                crate::db::document_store_overlay::OverlayError::Db(d) => d,
                other => DbError::Internal(other.to_string()),
            },
        )?;
        Ok(())
    })
    .await?;
    Ok(())
}
