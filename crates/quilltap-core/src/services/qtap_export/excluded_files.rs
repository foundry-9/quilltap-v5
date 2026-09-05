//! v4 `lib/export/excluded-files.ts` (NEW at `01e481f6`) — which library files
//! never travel in a `.qtap` export.
//!
//! v4's own header rides verbatim, because it records why the file exists at
//! all:
//!
//! > One predicate, three call sites — the writer's `files` streamer, the
//! > "everything of this type" id resolver, and the export wizard's entity
//! > picker. The rule used to be spelled out at each of them, which is how
//! > `ARCHIVE` came to be excluded nowhere after it was added.
//! >
//! > Two categories are excluded, for the same reason: both are themselves
//! > archives of the instance, and nesting them inside a fresh export bloats it
//! > enormously (base64-inflated) while adding nothing a restore could use.
//! >
//! > - **BACKUP** — mirrors the backup service's own rule. Nobody wants last
//! >   month's backup riding inside this month's.
//! > - **ARCHIVE** — a character archive bundle is a `.qtap` in its own right.
//! >   It is reachable through `characters.archiveFileId`, and it survives a
//! >   wipe only by the operator's explicit "keep archived characters" choice;
//! >   an export is not the place to smuggle copies of it.
//!
//! v5 has the same FOUR call sites v4 does: `streamFiles` and
//! `resolveExportIds` (both in `mod.rs`/`records.rs`), the wizard's entity
//! picker ([`super::entities`], v4's `handleExportEntities`), and — since v4
//! `0506517d3` (correction (b)) — the wizard's preview count
//! ([`super::preview`]).
//!
//! The preview was the odd one out for two commits. `01e481f6` converted three
//! sites and left `previewExport`'s own inline two-clause filter (BACKUP +
//! `/backups`) in place, so the preview counted the ARCHIVE bundles the writer
//! then refused; `0506517d3` finished the job while collapsing the 4.9 diff's
//! duplicates. `system_export_equivalence`'s `preview_files_all` case is what
//! measures it, in both directions: it was green on three sites and red the
//! moment v4 made it four.
//!
//! `services::backup::collect`'s own BACKUP rule is a DIFFERENT rule that v4
//! did not touch — leave that one alone.

use serde_json::Value;

/// v4 `EXPORT_EXCLUDED_FILE_CATEGORIES`.
pub(super) const EXPORT_EXCLUDED_FILE_CATEGORIES: [&str; 2] = ["BACKUP", "ARCHIVE"];

/// v4 `EXPORT_EXCLUDED_FOLDER_PATHS`.
pub(super) const EXPORT_EXCLUDED_FOLDER_PATHS: [&str; 2] = ["/backups", "/archives"];

/// v4 `isFileExcludedFromExport(file)`. `folderPath` is `?? ''` before the
/// membership test, so a NULL/absent path is simply not in the list.
pub(super) fn is_file_excluded_from_export(file: &Value) -> bool {
    let category = file.get("category").and_then(Value::as_str).unwrap_or("");
    let folder_path = file.get("folderPath").and_then(Value::as_str).unwrap_or("");
    EXPORT_EXCLUDED_FILE_CATEGORIES.contains(&category)
        || EXPORT_EXCLUDED_FOLDER_PATHS.contains(&folder_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn both_clauses_are_live_and_independent() {
        // Category alone.
        assert!(is_file_excluded_from_export(
            &json!({"category": "BACKUP", "folderPath": null})
        ));
        assert!(is_file_excluded_from_export(
            &json!({"category": "ARCHIVE", "folderPath": null})
        ));
        // Folder path alone.
        assert!(is_file_excluded_from_export(
            &json!({"category": "DOCUMENT", "folderPath": "/backups"})
        ));
        assert!(is_file_excluded_from_export(
            &json!({"category": "DOCUMENT", "folderPath": "/archives"})
        ));
        // Neither.
        assert!(!is_file_excluded_from_export(
            &json!({"category": "IMAGE", "folderPath": null})
        ));
        assert!(!is_file_excluded_from_export(
            &json!({"category": "DOCUMENT", "folderPath": "/notes"})
        ));
        // v4 compares the WHOLE path, not a prefix — `/archives/2026` is a
        // different folder and is not excluded.
        assert!(!is_file_excluded_from_export(
            &json!({"category": "DOCUMENT", "folderPath": "/archives/2026"})
        ));
        // An absent key reads as `''` on both sides.
        assert!(!is_file_excluded_from_export(&json!({})));
    }
}
