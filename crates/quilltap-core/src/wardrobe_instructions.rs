//! Wardrobe dressing instructions (v4 `lib/wardrobe/wardrobe-instructions.ts`,
//! `b86bb1a5`) — the optional `Wardrobe/instructions.md` file a user may keep at
//! any wardrobe tier, addressed to the character in the second person ("you
//! prefer to wear…").
//!
//! Read in exactly one place: the `llm_choose` outfit selection ("Dress
//! Themselves"), where the winning file's content is handed to the cheap-LLM
//! prompt. The cascade runs nearest-tier-first — character vault, then group,
//! then project, then Quilltap General — and stops at the first tier whose file
//! exists with non-blank content. It deliberately influences nothing else.
//!
//! The file is never a garment: [`is_wardrobe_instructions_file_name`] gates the
//! shared wardrobe reader's skip
//! ([`read_character_vault_wardrobe`](crate::db::vault_read_overlay::read_character_vault_wardrobe))
//! and the projection sweep's preserve list
//! ([`project_vault_wardrobe`](crate::db::vault_wardrobe_write::project_vault_wardrobe)).
//!
//! v5 has five separate `WARDROBE_FOLDER` constants and no shared home for them;
//! the instructions constants live HERE once and are `use`d, rather than
//! joining that spread (the consolidation is deliberately out of this lane).

use rusqlite::Connection;

use crate::db::doc_mount_file_links::DocMountFileLinksRepository;
use crate::db::instance_settings::get_general_mount_point_id;
use crate::db::DbError;
use crate::jsstr::js_trim;
use crate::services::memory_processor::read_vault_text_file_conn;

/// The `Wardrobe/` folder every tier's garments (and this file) live in — v4
/// `CHARACTER_WARDROBE_FOLDER`.
pub const WARDROBE_INSTRUCTIONS_FOLDER: &str = "Wardrobe";
/// v4 `WARDROBE_INSTRUCTIONS_FILENAME`.
pub const WARDROBE_INSTRUCTIONS_FILENAME: &str = "instructions.md";
/// v4 `WARDROBE_INSTRUCTIONS_PATH`.
pub const WARDROBE_INSTRUCTIONS_PATH: &str = "Wardrobe/instructions.md";

/// Case-insensitive match for the dressing-instructions file name (v4
/// `isWardrobeInstructionsFileName`: `name.toLowerCase() === 'instructions.md'`).
pub fn is_wardrobe_instructions_file_name(file_name: &str) -> bool {
    file_name.to_lowercase() == WARDROBE_INSTRUCTIONS_FILENAME
}

/// Which tier won the cascade (v4 `WardrobeInstructionsTier`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WardrobeInstructionsTier {
    Character,
    Group,
    Project,
    General,
}

impl WardrobeInstructionsTier {
    /// v4's string union member, for logs and the differential's comparand.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Character => "character",
            Self::Group => "group",
            Self::Project => "project",
            Self::General => "general",
        }
    }
}

/// v4 `WardrobeInstructionsResult`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WardrobeInstructionsResult {
    /// Trimmed, non-blank instructions content.
    pub content: String,
    /// Which tier won the cascade.
    pub tier: WardrobeInstructionsTier,
    /// The mount the winning file was read from.
    pub mount_point_id: String,
}

/// Deduped, lexicographically sorted copy — upstream resolvers return
/// Set-insertion order (v4 `deterministicMounts`). The sort is JS
/// `Array.prototype.sort()` on strings: UTF-16 code-unit order, which is v5's
/// Decision-B `str::cmp` idiom (`vault_read_overlay.rs`).
fn deterministic_mounts(ids: &[String]) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut out: Vec<String> = ids
        .iter()
        .filter(|id| seen.insert((*id).clone()))
        .cloned()
        .collect();
    out.sort();
    out
}

/// Resolve the dressing instructions for a character about to choose their own
/// outfit. First tier with a non-blank `Wardrobe/instructions.md` wins and the
/// search stops there; a file that exists but trims to empty counts as absent,
/// so clearing the editor behaves as "unset". Per-tier read failures degrade to
/// "not found" ([`read_vault_text_file_conn`] already returns `None` on any
/// failure), so this never fails on a broken mount.
///
/// The General mount id is read UNCONDITIONALLY and FIRST, before any probe —
/// v4 awaits it at the top of the function even when the character tier is about
/// to win. v4's `readSetting` warns and returns null rather than throwing, so a
/// read failure here means "no General tier", not "no cascade".
///
/// The character tier is skipped on JS truthiness (`''` skipped like `null`).
pub fn resolve_wardrobe_instructions(
    main: &Connection,
    mount: &Connection,
    character_mount_point_id: Option<&str>,
    group_mount_point_ids: &[String],
    project_mount_point_ids: &[String],
) -> Option<WardrobeInstructionsResult> {
    let general_mount_point_id = get_general_mount_point_id(main).ok().flatten();

    let mut probes: Vec<(WardrobeInstructionsTier, String)> = Vec::new();
    if let Some(id) = character_mount_point_id.filter(|s| !s.is_empty()) {
        probes.push((WardrobeInstructionsTier::Character, id.to_string()));
    }
    for id in deterministic_mounts(group_mount_point_ids) {
        probes.push((WardrobeInstructionsTier::Group, id));
    }
    for id in deterministic_mounts(project_mount_point_ids) {
        probes.push((WardrobeInstructionsTier::Project, id));
    }
    if let Some(id) = general_mount_point_id.filter(|s| !s.is_empty()) {
        probes.push((WardrobeInstructionsTier::General, id));
    }

    for (tier, mount_point_id) in probes {
        let raw = read_vault_text_file_conn(mount, &mount_point_id, WARDROBE_INSTRUCTIONS_PATH);
        let content = raw.as_deref().map(js_trim).unwrap_or("");
        if content.is_empty() {
            continue;
        }
        tracing::debug!(
            tier = tier.as_str(),
            mount_point_id,
            content_length = content.encode_utf16().count(),
            "[WardrobeInstructions] Dressing instructions resolved"
        );
        return Some(WardrobeInstructionsResult {
            content: content.to_string(),
            tier,
            mount_point_id,
        });
    }
    None
}

/// Read one container's own `Wardrobe/instructions.md` (no cascade — the cascade
/// is an outfit-selection runtime concern; the editor shows each tier its own
/// file). Blank or missing both come back as `None`.
pub fn read_wardrobe_instructions_file(mount: &Connection, mount_point_id: &str) -> Option<String> {
    let raw = read_vault_text_file_conn(mount, mount_point_id, WARDROBE_INSTRUCTIONS_PATH);
    let content = raw.as_deref().map(js_trim).unwrap_or("");
    if content.is_empty() {
        None
    } else {
        Some(content.to_string())
    }
}

/// Write (or, for null/blank content, remove) one container's
/// `Wardrobe/instructions.md`. Deleting a file that isn't there is a no-op, so
/// clearing an already-empty editor never errors — v4 swallows ONLY a
/// `NOT_FOUND` `DatabaseStoreError` and rethrows anything else; v5's
/// [`DocMountFileLinksRepository::delete_database_document`] already answers
/// `false` for the missing-file case and errors for everything else, which is
/// the same contract.
///
/// The folder ensure runs on the WRITE path only — clearing never creates a
/// `Wardrobe/` folder. The bytes on disk are the TRIMMED string: no trailing
/// newline and no frontmatter, unlike a garment file.
///
/// ⚠ On v5's substrate the explicit ensure is **provably redundant**: deleting it
/// leaves the differential's `doc_mount_folders` dump byte-identical, because
/// [`DocMountFileLinksRepository::write_database_document`] find-or-creates every
/// folder segment on each write (the same note
/// [`project_array_into_vault_folder`](crate::db::vault_wardrobe_write::project_array_into_vault_folder)
/// carries). It is kept because v4 calls it, and because relying on the write
/// primitive's incidental folder creation is not a contract either side states.
pub fn write_wardrobe_instructions_file(
    links: &DocMountFileLinksRepository,
    mount_point_id: &str,
    instructions: Option<&str>,
) -> Result<(), DbError> {
    let content = instructions.map(js_trim).unwrap_or("");
    if content.is_empty() {
        links.delete_database_document(mount_point_id, WARDROBE_INSTRUCTIONS_PATH)?;
        tracing::debug!(
            mount_point_id,
            "[WardrobeInstructions] Dressing instructions cleared"
        );
        return Ok(());
    }
    links.ensure_folder_path(mount_point_id, WARDROBE_INSTRUCTIONS_FOLDER)?;
    links.write_database_document(mount_point_id, WARDROBE_INSTRUCTIONS_PATH, content)?;
    tracing::debug!(
        mount_point_id,
        content_length = content.encode_utf16().count(),
        "[WardrobeInstructions] Dressing instructions written"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instructions_file_name_matches_any_casing() {
        assert!(is_wardrobe_instructions_file_name("instructions.md"));
        assert!(is_wardrobe_instructions_file_name("Instructions.MD"));
        assert!(is_wardrobe_instructions_file_name("INSTRUCTIONS.md"));
        assert!(!is_wardrobe_instructions_file_name("instructions.markdown"));
        assert!(!is_wardrobe_instructions_file_name(
            "Wardrobe/instructions.md"
        ));
    }

    #[test]
    fn deterministic_mounts_dedupes_then_sorts_by_code_unit() {
        assert_eq!(
            deterministic_mounts(&["m-b".into(), "m-a".into(), "m-b".into()]),
            vec!["m-a".to_string(), "m-b".to_string()]
        );
        // Code-unit order, not ICU collation: uppercase sorts before lowercase.
        assert_eq!(
            deterministic_mounts(&["a".into(), "B".into()]),
            vec!["B".to_string(), "a".to_string()]
        );
    }

    #[test]
    fn instructions_path_is_the_folder_joined_to_the_file_name() {
        assert_eq!(
            WARDROBE_INSTRUCTIONS_PATH,
            format!("{WARDROBE_INSTRUCTIONS_FOLDER}/{WARDROBE_INSTRUCTIONS_FILENAME}")
        );
    }
}
