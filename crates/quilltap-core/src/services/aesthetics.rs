//! Default image aesthetics + the Ariel Clause (v4 `lib/image-gen/aesthetic.ts`)
//! — the subset the avatar / story-background job handlers consume.
//!
//! Three guidance files shape image-prompt generation:
//!   - `lantern-aesthetics.md`   — general / scene / background look
//!   - `aurora-aesthetics.md`    — how people + their outfits are depicted
//!   - `depiction-guidelines.md` — per-character override (the Ariel Clause)
//!
//! The two aesthetics resolve across two tiers, **project-official overrides
//! Quilltap General**, per file and independently. The Ariel Clause is NOT
//! tiered — it reads only the depicted character's own vault root.
//!
//! Every read fails **soft**: a missing/unreadable file resolves to `None`/`[]`.
//! Image generation must never break because a guidance file couldn't be read.
//!
//! Only the read side is ported (the editors' `writeStoreFile` / route helpers
//! are Phase-4 surface). The reads route through the ported document store
//! (`doc_mount_documents::find_by_mount_point_and_path`) + the raw project
//! pointer + the Quilltap General singleton.

use crate::db::runtime::Db;
use crate::jsstr::{js_trim, utf16_len, utf16_truncate};

/// v4 `LANTERN_AESTHETICS_FILENAME`.
pub const LANTERN_AESTHETICS_FILENAME: &str = "lantern-aesthetics.md";
/// v4 `AURORA_AESTHETICS_FILENAME`.
pub const AURORA_AESTHETICS_FILENAME: &str = "aurora-aesthetics.md";
/// v4 `DEPICTION_GUIDELINES_FILENAME`.
pub const DEPICTION_GUIDELINES_FILENAME: &str = "depiction-guidelines.md";

/// v4 `DEFAULT_AESTHETIC_MAX_CHARS`.
const DEFAULT_AESTHETIC_MAX_CHARS: usize = 4000;
/// v4 `DEFAULT_DEPICTION_MAX_CHARS`.
const DEFAULT_DEPICTION_MAX_CHARS: usize = 2000;

/// v4 `AestheticKind`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AestheticKind {
    Lantern,
    Aurora,
}

impl AestheticKind {
    fn filename(self) -> &'static str {
        match self {
            AestheticKind::Lantern => LANTERN_AESTHETICS_FILENAME,
            AestheticKind::Aurora => AURORA_AESTHETICS_FILENAME,
        }
    }
}

/// v4 `readStoreFileInternal`: read one named file from a store, trimmed and
/// capped. Returns `None` when the file is absent, empty/whitespace-only, or
/// unreadable (whitespace-only is treated as absent so a hand-dropped empty file
/// never suppresses the fallback tier). Sync — takes the mount-index connection.
fn read_store_file_internal(
    mount: &rusqlite::Connection,
    mount_point_id: &str,
    filename: &str,
    max_chars: usize,
) -> Option<String> {
    let repo = crate::db::doc_mount_documents::DocMountDocumentsRepository::new(mount);
    let content = match repo.find_by_mount_point_and_path(mount_point_id, filename) {
        Ok(Some(c)) => c,
        // Absent, or a read error (v4's catch → treat as absent).
        _ => return None,
    };
    let trimmed = js_trim(&content);
    if trimmed.is_empty() {
        return None;
    }
    if utf16_len(trimmed) > max_chars {
        Some(utf16_truncate(trimmed, max_chars))
    } else {
        Some(trimmed.to_string())
    }
}

/// v4 `getProjectOfficialMountPointId`: the project's official document store
/// mount id, or `None`. Reads the slim row pointer (no validating overlay — it
/// can throw mid-transition) and fails soft. A null/absent project yields `None`
/// (global-only resolution).
pub async fn get_project_official_mount_point_id(
    db: &Db,
    project_id: Option<&str>,
) -> Option<String> {
    let project_id = project_id.filter(|s| !s.is_empty())?.to_string();
    // `Result<Option<Option<String>>>` — outer: row found; inner: nullable column.
    db.read_main(move |conn| {
        crate::db::projects::find_official_mount_point_id_raw(conn, &project_id)
    })
    .ok()
    .flatten()
    .flatten()
}

/// v4 `resolveAesthetic`: resolve one aesthetic file. Project official store
/// overrides Quilltap General; returns `None` when neither tier has (non-empty)
/// content. Never throws.
pub async fn resolve_aesthetic(
    db: &Db,
    kind: AestheticKind,
    project_official_mount_point_id: Option<&str>,
    max_chars: Option<usize>,
) -> Option<String> {
    let max_chars = max_chars.unwrap_or(DEFAULT_AESTHETIC_MAX_CHARS);
    let filename = kind.filename();

    // Project tier first.
    if let Some(project_mount) = project_official_mount_point_id.filter(|s| !s.is_empty()) {
        let project_mount = project_mount.to_string();
        let filename_owned = filename.to_string();
        let hit = db
            .read_mount_index(move |mount| {
                Ok(read_store_file_internal(
                    mount,
                    &project_mount,
                    &filename_owned,
                    max_chars,
                ))
            })
            .ok()
            .flatten();
        if hit.is_some() {
            return hit;
        }
    }

    // Global tier fallback.
    let general_mount = db
        .read_main(crate::db::instance_settings::get_general_mount_point_id)
        .ok()
        .flatten()?;
    let filename_owned = filename.to_string();
    db.read_mount_index(move |mount| {
        Ok(read_store_file_internal(
            mount,
            &general_mount,
            &filename_owned,
            max_chars,
        ))
    })
    .ok()
    .flatten()
}

// ============================================================================
// The editor route helpers (v4's "Shared route helpers" section) — the
// single-tier read/write pair BOTH aesthetic editor surfaces sit on: the
// per-project one (`api::projects::project_aesthetic_*`, P4.6l) and the
// instance-wide one (`api::system::system_image_aesthetics_*`, P4.6ar). Only
// the mount they resolve differs: the project's OFFICIAL store vs the Quilltap
// General singleton.
// ============================================================================

/// v4 `parseAestheticKind` (`aesthetic.ts:221`), reduced to the string test the
/// two routes share: the literal `lantern` or `aurora`, else `None` (the caller
/// answers the 400). Returns the FILENAME rather than the kind — every caller
/// wants the file.
pub fn aesthetic_filename_for_kind(kind: &str) -> Option<&'static str> {
    match kind {
        "lantern" => Some(LANTERN_AESTHETICS_FILENAME),
        "aurora" => Some(AURORA_AESTHETICS_FILENAME),
        _ => None,
    }
}

/// v4 `readAesthetic(mountId, kind)` over `readStoreFile`: the editor's read —
/// the **RAW** content (NOT trimmed, NOT capped; the editor shows exactly what is
/// on disk, unlike the injection path's `readStoreFileInternal`). v4 coalesces
/// BOTH the absent-doc case (`doc?.content ?? ''`) and the read-error case
/// (`readAestheticForMount` returns null → `content ?? ''`) to `''`, so the caller
/// can return this directly.
pub fn read_aesthetic_file(
    mount: &rusqlite::Connection,
    mount_point_id: &str,
    filename: &str,
) -> String {
    match crate::db::database_store::read_database_document(mount, mount_point_id, filename) {
        Ok(doc) => doc.content,
        Err(_) => String::new(),
    }
}

/// v4 `writeAesthetic(mountId, kind, content)` over `writeStoreFile`: the
/// editor's write — **empty or whitespace-only content DELETES the file**
/// (`!content || !content.trim()`), so clearing an override restores the fallback
/// tier; otherwise create-or-update.
pub fn write_aesthetic_file(
    mount: &rusqlite::Connection,
    mount_point_id: &str,
    filename: &str,
    content: &str,
) -> Result<(), crate::db::DbError> {
    if js_trim(content).is_empty() {
        crate::db::database_store::delete_database_document(mount, mount_point_id, filename)?;
    } else {
        crate::db::database_store::write_database_document(
            mount,
            mount_point_id,
            filename,
            content,
        )
        .map_err(|e| crate::db::DbError::Internal(e.to_string()))?;
    }
    Ok(())
}

/// A depicted character for [`resolve_depiction_guidelines`] (v4
/// `DepictedCharacter`).
#[derive(Clone, Debug)]
pub struct DepictedCharacter {
    pub id: String,
    pub name: String,
    pub character_document_mount_point_id: Option<String>,
}

/// One resolved depiction guideline (v4 `DepictionGuideline`).
#[derive(Clone, Debug, PartialEq)]
pub struct DepictionGuideline {
    pub character_id: String,
    pub character_name: String,
    pub content: String,
}

/// v4 `resolveDepictionGuidelines`: for each depicted character, read
/// `depiction-guidelines.md` from that character's own vault root. Characters
/// without a vault or without the file are skipped. Reads only each character's
/// own vault; unaffected by project/global tiers. Fails soft per file.
pub async fn resolve_depiction_guidelines(
    db: &Db,
    characters: &[DepictedCharacter],
    max_chars_each: Option<usize>,
) -> Vec<DepictionGuideline> {
    let max_chars_each = max_chars_each.unwrap_or(DEFAULT_DEPICTION_MAX_CHARS);
    let mut out: Vec<DepictionGuideline> = Vec::new();
    for character in characters {
        let Some(mount_point_id) = character
            .character_document_mount_point_id
            .as_deref()
            .filter(|s| !s.is_empty())
        else {
            continue;
        };
        let mount_point_id = mount_point_id.to_string();
        let hit = db
            .read_mount_index(move |mount| {
                Ok(read_store_file_internal(
                    mount,
                    &mount_point_id,
                    DEPICTION_GUIDELINES_FILENAME,
                    max_chars_each,
                ))
            })
            .ok()
            .flatten();
        if let Some(content) = hit {
            out.push(DepictionGuideline {
                character_id: character.id.clone(),
                character_name: character.name.clone(),
                content,
            });
        }
    }
    out
}
