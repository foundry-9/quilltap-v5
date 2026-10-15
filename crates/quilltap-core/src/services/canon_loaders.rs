//! The canon loaders the memory extractors and the consolidation call share —
//! v4 `lib/memory/cheap-llm-tasks/canon.ts`'s vault-reading half (unchanged
//! across the `83334b9a5` span; `ac6348c5f` only moved its callers).
//!
//! MOVED VERBATIM out of `services/memory_processor.rs` by work order P4.D296
//! (L-9): v4 deleted `memory-processor.ts` in `ac6348c5f`, and these loaders
//! are the part of it that survives — the extraction context memoises them
//! per unit of work, the consolidation call reads the OTHER canon with
//! `include_commonplace: false`, and the wardrobe-instructions cascade reads
//! through [`read_vault_text_file_conn`]. Their `[Canon]` lines move with them
//! (`canon_commonplace_tier2` pins them).

use crate::canon::{load_canon_for_self, CanonSource, CanonSourceKind, SelfCanon};
use crate::commonplace_file::{
    commonplace_file_to_canon_text, commonplace_path_for, CommonplaceSubject,
};
use crate::db::doc_mount_documents::DocMountDocumentsRepository;
use crate::db::doc_mount_file_links::normalise_relative_path;
use crate::db::runtime::Db;
use crate::vault_overlay::sanitize_file_name;

const OTHERS_FOLDER: &str = "Others";

/// v4 `readVaultTextFile` (`vault-overlay/vault-readers.ts:53-69`): read a
/// vault document's content through `readDatabaseDocumentIfExists`
/// (`database-store.ts:88-102`), `None` on NOT_FOUND — with v4's `Document
/// absent from database-backed store` debug line — and on any other failure
/// too (v4 warns `Failed to read vault file` and returns null, never throws).
/// Both the extraction canons and the consolidation canon read through this
/// ONE reader, as v4's do (the unifier folded P4.D271's lane-local copy here).
pub fn read_vault_text_file(
    db: &Db,
    mount_point_id: &str,
    path: &str,
    character_id: Option<&str>,
) -> Option<String> {
    match crate::services::commonplace_digest_vault_bridge::read_database_document_if_exists(
        db,
        mount_point_id,
        path,
    ) {
        Ok(content) => content,
        Err(error) => {
            // v4's `{ mountPointId, path, characterId, error }` — `characterId`
            // is the caller's optional third argument (both canon sites pass
            // the holder's / observer's id; an absent one is dropped from the
            // object).
            match character_id {
                Some(character_id) => tracing::warn!(
                    mountPointId = %mount_point_id,
                    path = %path,
                    characterId = %character_id,
                    error = %error,
                    "Failed to read vault file"
                ),
                None => tracing::warn!(
                    mountPointId = %mount_point_id,
                    path = %path,
                    error = %error,
                    "Failed to read vault file"
                ),
            }
            None
        }
    }
}

/// [`read_vault_text_file`] over a mount-index connection the caller already
/// holds (the wardrobe-instructions cascade probes up to four tiers inside one
/// read). Same contract: `None` on NOT_FOUND and on any other failure.
pub fn read_vault_text_file_conn(
    mount: &rusqlite::Connection,
    mount_point_id: &str,
    path: &str,
) -> Option<String> {
    let rel = normalise_relative_path(path).ok()?;
    DocMountDocumentsRepository::new(mount)
        .find_by_mount_point_and_path(mount_point_id, &rel)
        .ok()
        .flatten()
}

/// The subject an OTHER canon is loaded for (v4 `loadCanonForObserverAbout
/// Subject`'s `subject: { id, name, identity, description }`).
#[derive(Clone, Debug)]
pub struct CanonSubject {
    pub id: String,
    pub name: String,
    pub identity: Option<String>,
    pub description: Option<String>,
}

/// v4 `loadCommonplaceCanon(holder, subject)` (`canon.ts:201-228`, `d58548051`):
/// read a Commonplace digest mirror from a holder's vault and return it as
/// canon text (frontmatter and heading stripped), or `None` when there is no
/// vault or no file. Never fails — a missing digest is not an error.
///
/// v4's catch line `[Canon] Commonplace digest unavailable; canon continues
/// without it` is NOT ported: it cannot fire. `readVaultTextFile` catches every
/// error itself (`vault-readers.ts:53-69` — WARN `Failed to read vault file` →
/// `null`), so the loader's own `try` never sees a throw; the
/// `canon_commonplace_tier2` family pins it negative.
pub fn load_commonplace_canon(
    db: &Db,
    holder_character_id: &str,
    holder_mount_point_id: Option<&str>,
    subject: &CommonplaceSubject,
) -> Option<String> {
    let mount_point_id = holder_mount_point_id.filter(|m| !m.is_empty())?;
    let path = commonplace_path_for(subject);
    let raw = read_vault_text_file(db, mount_point_id, &path, Some(holder_character_id));
    let text = commonplace_file_to_canon_text(raw.as_deref());
    if let Some(text) = &text {
        tracing::debug!(
            characterId = %holder_character_id,
            path = %path,
            chars = crate::jsstr::utf16_len(text),
            "[Canon] Commonplace digest loaded into canon"
        );
    }
    text
}

/// v4 `loadCanonForSelfWithCommonplace` (`canon.ts:233-247`): SELF-pass canon
/// with the character's own `Commonplace/Self.md` appended after the card
/// fields. Same shape as [`crate::canon::load_canon_for_self`]; the extra
/// `mount_point_id` is the character's own vault. v4 spreads the digest only
/// when truthy, so an empty digest reads as absent.
#[allow(clippy::too_many_arguments)]
pub fn load_canon_for_self_with_commonplace(
    db: &Db,
    id: &str,
    name: &str,
    manifesto: Option<&str>,
    personality: Option<&str>,
    description: Option<&str>,
    identity: Option<&str>,
    mount_point_id: Option<&str>,
) -> SelfCanon {
    let mut canon = load_canon_for_self(id, name, manifesto, personality, description, identity);
    canon.commonplace =
        load_commonplace_canon(db, id, mount_point_id, &CommonplaceSubject::SelfFile)
            .filter(|c| !c.is_empty());
    canon
}

/// v4 `loadCanonForObserverAboutSubject(observer, subject, options)`
/// (`canon.ts:255-271`): the hand canon first — the observer's vault
/// `Others/<sanitized-subject-name>.md`, then the subject's identity, then
/// description, never personality or manifesto (no observer sees another
/// character's interior or axiomatic core) — then, unless
/// `include_commonplace` is false, the observer's `Commonplace/<subject>.md`
/// digest, the order the block renders in. The per-turn and fold-grain OTHER
/// passes pass `true` (v4's default); only the consolidation call passes
/// `false` (it is handed the digests directly).
pub fn load_canon_for_observer_about_subject(
    db: &Db,
    observer_mount_point_id: Option<&str>,
    observer_character_id: &str,
    subject: &CanonSubject,
    include_commonplace: bool,
) -> CanonSource {
    let hand = load_hand_canon(db, observer_mount_point_id, observer_character_id, subject);
    if !include_commonplace {
        return hand;
    }
    let commonplace = load_commonplace_canon(
        db,
        observer_character_id,
        observer_mount_point_id,
        &CommonplaceSubject::Other {
            id: subject.id.clone(),
            name: subject.name.clone(),
        },
    );
    match commonplace.filter(|c| !c.is_empty()) {
        Some(c) => CanonSource {
            commonplace: Some(c),
            ..hand
        },
        None => hand,
    }
}

/// v4 `loadHandCanon` (`canon.ts:274-322`): the hand-written half of OTHER
/// canon — vault `Others/` file, else identity, else description.
fn load_hand_canon(
    db: &Db,
    observer_mount_point_id: Option<&str>,
    observer_character_id: &str,
    subject: &CanonSubject,
) -> CanonSource {
    if let Some(mount_id) = observer_mount_point_id.filter(|s| !s.is_empty()) {
        let path = format!("{OTHERS_FOLDER}/{}.md", sanitize_file_name(&subject.name));
        if let Some(from_vault) =
            read_vault_text_file(db, mount_id, &path, Some(observer_character_id))
        {
            let trimmed = crate::jsstr::js_trim(&from_vault);
            if !trimmed.is_empty() {
                return CanonSource {
                    character_id: subject.id.clone(),
                    character_name: subject.name.clone(),
                    body: Some(trimmed.to_string()),
                    source: CanonSourceKind::Vault,
                    commonplace: None,
                };
            }
        }
    }

    if let Some(identity) = subject
        .identity
        .as_deref()
        .map(crate::jsstr::js_trim)
        .filter(|s| !s.is_empty())
    {
        return CanonSource {
            character_id: subject.id.clone(),
            character_name: subject.name.clone(),
            body: Some(identity.to_string()),
            source: CanonSourceKind::Identity,
            commonplace: None,
        };
    }

    if let Some(description) = subject
        .description
        .as_deref()
        .map(crate::jsstr::js_trim)
        .filter(|s| !s.is_empty())
    {
        return CanonSource {
            character_id: subject.id.clone(),
            character_name: subject.name.clone(),
            body: Some(description.to_string()),
            source: CanonSourceKind::Description,
            commonplace: None,
        };
    }

    CanonSource {
        character_id: subject.id.clone(),
        character_name: subject.name.clone(),
        body: None,
        source: CanonSourceKind::None,
        commonplace: None,
    }
}
