//! Document Mount Text Search — v4 `lib/mount-index/document-text-search.ts`
//! (NEW at `b220999d`), ported whole.
//!
//! Keyword (substring) search across every enabled document store — file names,
//! relative paths, and extracted chunk text — for the global search bar's
//! **Documents** chip. The deliberate sibling of the semantic document search,
//! which does the same job over embeddings; this one is the plain `LIKE` scan
//! the search bar's other branches all are.
//!
//! Scope notes (v4's, carried):
//! - Character vaults are ordinary mount points and ARE searched — except the
//!   vaults of archived characters, which are tombstones (see
//!   [`archived_character_vault_mount_point_ids`]).
//! - Documents flagged `character_read: false` ARE included. This is a human
//!   operator's surface, mirroring `includeBlocked: true` on the operator's
//!   semantic-search endpoint; that flag gates *characters*, not the user.
//! - Only file types Document Mode can open are searched (see
//!   [`EDITABLE_TEXT_FILE_TYPES`]), so every result is clickable.

/// The `fileType` values that hold editable plain text — the set the global
/// search bar's Documents chip searches and the set Document Mode can open
/// (v4 `EDITABLE_TEXT_FILE_TYPES`, `lib/schemas/mount-index.types.ts`).
/// `pdf`/`docx` carry extracted text but are not editable documents, and `blob`
/// has no text representation at all.
///
/// **Home note (P4.D122).** v4 declares this beside the `DocMountFile` Zod
/// schema, which both repositories import. v5 has no schema-types module in
/// `db/`, and the two same-valued constants v5 already carries
/// (`services::character_archive::service::TEXT_DOCUMENT_FILE_TYPES`,
/// `services::mount_index::reindex::TEXT_NATIVE`) are deliberately left alone —
/// so the work order placed the third, newly-named constant here, with the
/// search module that names it. The two `db::doc_mount_*` scans import it back.
pub const EDITABLE_TEXT_FILE_TYPES: [&str; 4] = ["markdown", "txt", "json", "jsonl"];
