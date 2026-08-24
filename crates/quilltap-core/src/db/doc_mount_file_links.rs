//! The document-store **storage primitive** — `writeDatabaseDocument` +
//! `DocMountFileLinksRepository.linkDocumentContent` + `ensureLinkFolderId`,
//! ported from v4's
//! `lib/mount-index/database-store.ts` and
//! `lib/database/repositories/doc-mount-file-links.repository.ts`.
//!
//! This is the byte-landing path every store-backed entity (project/group
//! store, character vault) ultimately calls: a `(mountPointId, relativePath,
//! content)` write is content-addressed by SHA-256 and split across three tables
//! — `doc_mount_files` (content identity, keyed by sha), `doc_mount_documents`
//! (the text bytes, keyed by `fileId`), and `doc_mount_file_links` (the location
//! `(mountPointId, relativePath) → fileId`) — with `doc_mount_folders` rows
//! auto-created for any parent path. See `docs/developer/porting/
//! document-store-overlay.md` (this is build step 1 of that slice).
//!
//! ## The sibling DB
//!
//! Like every mount-index repo, in v4 these tables live in
//! `quilltap-mount-index.db`. In the Rust port that is simply the file the
//! [`super::Writer`] was opened against (see `doc_mount_points` for the full
//! note); the code is identical in shape to a main-DB repo.
//!
//! ## The `linkDocumentContent` transaction (v4 lines 738-864)
//!
//! One `db.transaction(...)`:
//!   1. **find-or-create `doc_mount_files` by `contentSha256`** — identical
//!      content written to two paths reuses ONE file + ONE document row (dedup);
//!   2. **upsert `doc_mount_documents` by `fileId`** (this is where the bytes
//!      land) — created only when the file row is new;
//!   3. derive `folderId` from `relativePath` via [`ensure_link_folder_id`]
//!      (find-or-create every parent folder segment, in-transaction);
//!   4. **upsert `doc_mount_file_links` by `(mountPointId, relativePath)`** —
//!      rewriting a path updates the link IN PLACE (new `fileId`, refreshed
//!      `lastModified`/`updatedAt`), never duplicating it.
//!
//! The Rust INSERTs list **exactly v4's column subset**, so SQLite fills the
//! same column DEFAULTs (`description=''`, `extractionStatus='none'`, the nullable
//! columns NULL) from the shared fixture DDL — the unset columns match without
//! enumerating them.
//!
//! ## Per-document policy
//!
//! For `markdown` files the three `allow*` flags derive from the frontmatter via
//! [`policy_from_content`] (other text types → permissive default). v4 parses the
//! frontmatter with the `yaml` library; this port reproduces the *scalar* subset
//! (the only shape the three policy keys take) and reads `embed` /
//! `character_read` / `character_write` through [`coerce_policy_bool`]. Full
//! arbitrary-YAML frontmatter is deferred to the character-vault slice (which
//! needs the general YAML round-trip anyway); the tier-2 corpus stays within the
//! scalar subset, and the differential verifies it against v4's real parser.
//!
//! Determinism: `linkDocumentContent` mints all ids (`randomUUID`) and a single
//! `now` internally — nothing is injectable — so the tier-2 differential uses the
//! minted-values remap form (first-seen id tokens in natural-key order across all
//! four tables, timestamps placeholdered).
//!
//! Scope: the write/storage path only. The repo's remaining read/join/GC/
//! conversion helpers (`linkFilesystemFile`, `sweepOrphanedFiles`, …) are out of
//! scope here. ([`link_blob_content`](DocMountFileLinksRepository::link_blob_content)
//! — the BINARY analogue of `link_document_content` — is ported here for the
//! blob doc-edit handlers, W4.1d batch 3b.)

use rusqlite::types::ToSql;
use rusqlite::{params, Connection};
use sha2::{Digest, Sha256};

use super::DbError;
use crate::clock::now_iso;

/// One row of the joined `doc_mount_file_links l JOIN doc_mount_files f` view — v4
/// `DocMountFileLinkWithContent` (`queryJoined`, `doc-mount-file-links.repository
/// .ts:1017`), restricted to the columns the database-store primitives + the
/// access-control / photo consumers read. The link columns come from `l`; the
/// content fingerprint / size / type / source from the joined `f`.
///
/// `allow_character_read` / `allow_character_write` are stored as SQLite 0/1;
/// v4's `coerceAllow` maps absent/NULL → permissive `true`, else `!= 0` → bool.
/// Explicit row ids a `preserveIds` import asks the two content writers to
/// claim (v4 `01e481f6`, spec F4). v4 hangs these off `LinkBlobInput` /
/// `LinkDocumentInput`; v5 passes them as their own argument so the dozen
/// ordinary call sites — every one of which wants all three absent — stay
/// untouched. v4's note on the semantics rides verbatim:
///
/// > Honored only when the row in question is actually being **created**; an
/// > existing row found by sha256 or (mountPointId, relativePath) keeps its own
/// > id — the content-addressed dedup and path-upsert invariants win.
#[derive(Default, Clone, Debug)]
pub struct CarriedRowIds {
    /// The `doc_mount_files` content row.
    pub file_id: Option<String>,
    /// The `doc_mount_documents` row (text writes only).
    pub document_id: Option<String>,
    /// The `doc_mount_blobs` row (binary writes only).
    pub blob_id: Option<String>,
    /// The `doc_mount_file_links` row.
    pub link_id: Option<String>,
}

/// A `doc_mount_files` CONTENT row, narrowed to what the `preserveIds`
/// preflight asks of it (see [`DocMountFileLinksRepository::find_content_row_by_id`]).
#[derive(Clone, Debug)]
pub struct ContentRow {
    pub id: String,
    pub sha256: String,
}

#[derive(Clone, Debug)]
pub struct LinkRow {
    pub id: String,
    pub file_id: String,
    pub mount_point_id: String,
    pub relative_path: String,
    pub file_name: String,
    pub folder_id: Option<String>,
    /// `f.sha256`.
    pub sha256: String,
    /// `f.fileSizeBytes` (REAL-affinity int; kept as i64 for the listing shape).
    pub file_size_bytes: i64,
    /// `f.fileType`.
    pub file_type: String,
    /// `f.source` (`'filesystem' | 'database'`).
    pub source: String,
    pub last_modified: String,
    pub created_at: String,
    /// SQLite 0/1 → bool via v4 `coerceAllow` (absent/NULL → permissive true).
    pub allow_character_read: bool,
    /// SQLite 0/1 → bool via v4 `coerceAllow`.
    pub allow_character_write: bool,
    pub extracted_text: Option<String>,
    pub original_mime_type: Option<String>,
    pub conversion_status: String,
    pub chunk_count: i64,
    /// `l.description` (the link's caption fallback; `''` default). Read for the
    /// character-gallery listing's `caption` field.
    pub description: Option<String>,
    /// `l.extractionStatus` (`'none'|'pending'|'converted'|'failed'|'skipped'`,
    /// nullable in old rows) — the reindex `shouldProcess` gate reads it (P4.6y).
    pub extraction_status: Option<String>,
    /// `l.allowEmbed` via v4 `coerceAllow` — the embedding scheduler's
    /// per-document `embed:false` policy gate (P4.6y).
    pub allow_embed: bool,
    /// `l.linkGroupId` — set when this link belongs to a deliberate hard-link
    /// group (v4 `40319484`). **Not** the same thing as sharing a `fileId`:
    /// content rows are sha256-addressed, so unrelated byte-identical files
    /// share one by coincidence. Only a non-null group means "these are one
    /// file". Read by the sibling re-index pass.
    pub link_group_id: Option<String>,
    /// `l.originalFileName` — v4's `queryJoined` selects it and the bug-38
    /// attach path reads `originalFileName || fileName` for the display title.
    pub original_file_name: Option<String>,
}

/// A link row joined with its file-row content fields — v4
/// `DocMountFileLinkWithContent`, restricted to the fields the photo tools read
/// (`attach_image` + the `saveImageToAlbum` mount-blob fallback). `sha256` /
/// `file_size_bytes` come from the joined `doc_mount_files` row.
#[derive(Clone, Debug)]
pub struct LinkWithContent {
    pub id: String,
    pub file_id: String,
    pub mount_point_id: String,
    pub relative_path: String,
    pub file_name: String,
    pub original_file_name: Option<String>,
    pub original_mime_type: Option<String>,
    pub extracted_text: Option<String>,
    pub created_at: String,
    pub sha256: String,
    pub file_size_bytes: i64,
    /// `f.fileType` (P4.6y additive — the hard-link metadata copy reads it).
    pub file_type: String,
    /// `f.source` (P4.6y additive).
    pub source: String,
    /// `l.description` (P4.6y additive; `''` default).
    pub description: Option<String>,
    /// `l.conversionStatus` (P4.6y additive).
    pub conversion_status: String,
    /// `l.plainTextLength` (P4.6y additive; REAL, nullable).
    pub plain_text_length: Option<f64>,
    /// `l.linkGroupId` (v4 `40319484` additive — the same
    /// `DocMountFileLinkWithContent` field [`LinkRow::link_group_id`] carries).
    pub link_group_id: Option<String>,
}

const LINK_WITH_CONTENT_SELECT: &str = "SELECT \
       l.id, l.fileId, l.mountPointId, l.relativePath, l.fileName, \
       l.originalFileName, l.originalMimeType, l.extractedText, l.createdAt, \
       f.sha256, f.fileSizeBytes, \
       f.fileType, f.source, l.description, l.conversionStatus, l.plainTextLength, \
       l.linkGroupId \
     FROM doc_mount_file_links l \
     JOIN doc_mount_files f ON f.id = l.fileId \
     WHERE l.id = ?1";

fn map_link_with_content(row: &rusqlite::Row<'_>) -> rusqlite::Result<LinkWithContent> {
    Ok(LinkWithContent {
        id: row.get(0)?,
        file_id: row.get(1)?,
        mount_point_id: row.get(2)?,
        relative_path: row.get(3)?,
        file_name: row.get(4)?,
        original_file_name: row.get(5)?,
        original_mime_type: row.get(6)?,
        extracted_text: row.get(7)?,
        created_at: row.get(8)?,
        sha256: row.get(9)?,
        // `fileSizeBytes` has REAL affinity; tolerate Real/Integer storage.
        file_size_bytes: {
            match row.get_ref(10)? {
                rusqlite::types::ValueRef::Integer(i) => i,
                rusqlite::types::ValueRef::Real(f) => f as i64,
                _ => 0,
            }
        },
        file_type: row.get(11)?,
        source: row.get(12)?,
        description: row.get(13)?,
        conversion_status: row.get(14)?,
        link_group_id: row.get(16)?,
        plain_text_length: match row.get_ref(15)? {
            rusqlite::types::ValueRef::Integer(i) => Some(i as f64),
            rusqlite::types::ValueRef::Real(f) => Some(f),
            _ => None,
        },
    })
}

/// A link update patch — v4 `docMountFileLinks.update(id, Partial<...>)`. Each
/// `Some` field sets that column; nullable columns are `Option<Option<...>>` so
/// the caller can distinguish "leave untouched" (outer `None`) from "set to SQL
/// NULL" (`Some(None)`), matching v4 passing explicit `null`s. `updated_at` is
/// always set. Extended for the P4.6y scanner/reindex/store-file ports (the
/// extraction-state + rollup + cross-mount-move columns).
#[derive(Default)]
pub struct LinkUpdate {
    pub relative_path: Option<String>,
    pub file_name: Option<String>,
    pub folder_id: Option<Option<String>>,
    /// v4 `moveFile` db→db re-points the link at the destination mount.
    pub mount_point_id: Option<String>,
    pub file_id: Option<String>,
    pub conversion_status: Option<String>,
    pub conversion_error: Option<Option<String>>,
    /// REAL number column (`Some(None)` = SQL NULL).
    pub plain_text_length: Option<Option<f64>>,
    /// REAL number column.
    pub chunk_count: Option<f64>,
    pub extracted_text: Option<Option<String>>,
    pub extracted_text_sha256: Option<Option<String>>,
    pub extraction_status: Option<String>,
    pub extraction_error: Option<Option<String>>,
    pub last_modified: Option<String>,
    pub updated_at: String,
}

/// v4 `LinkFilesystemFileInput` — the scanner/reindex upsert input
/// (`linkFilesystemFile`). `folderId` is always derived from the relativePath
/// (the v4 caller never passes one). `None` optionals take v4's defaults:
/// `source` 'filesystem', `conversionStatus` 'pending', `chunkCount` 0,
/// policy flags permissive.
#[derive(Default)]
pub struct LinkFilesystemFileInput {
    pub mount_point_id: String,
    pub relative_path: String,
    pub file_name: String,
    pub file_type: String,
    pub sha256: String,
    /// REAL number column.
    pub file_size_bytes: f64,
    pub last_modified: String,
    pub source: Option<String>,
    pub conversion_status: Option<String>,
    pub conversion_error: Option<String>,
    /// REAL number column (`None` → SQL NULL).
    pub plain_text_length: Option<f64>,
    /// REAL number column.
    pub chunk_count: Option<f64>,
    pub allow_embed: Option<bool>,
    pub allow_character_read: Option<bool>,
    pub allow_character_write: Option<bool>,
}

/// The three per-document policy flags, positive sense (`true` == permissive ==
/// the frontmatter default). Mirrors v4 `DocumentPolicy`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DocumentPolicy {
    pub embed: bool,
    pub character_read: bool,
    pub character_write: bool,
}

/// v4 `DEFAULT_DOCUMENT_POLICY` — all permissive.
pub const DEFAULT_DOCUMENT_POLICY: DocumentPolicy = DocumentPolicy {
    embed: true,
    character_read: true,
    character_write: true,
};

/// Hex-encoded SHA-256 of a UTF-8 string — v4 `sha256OfString`
/// (`lib/utils/sha256.ts`). Used for content-addressed dedup.
pub fn sha256_of_string(content: &str) -> String {
    hex::encode(Sha256::digest(content.as_bytes()))
}

/// Detect the database-store file type from a relative path's extension — v4
/// `detectDatabaseFileType` (`database-store.ts:33`). `None` for unsupported
/// extensions (the caller raises an "only accept text documents" error). The
/// extension match is case-insensitive (`path.extname(...).toLowerCase()`).
pub fn detect_database_file_type(relative_path: &str) -> Option<&'static str> {
    let ext = relative_path
        .rsplit_once('.')
        // `path.extname` only treats a dot as an extension when it is not the
        // first char of the basename; for our store paths a trailing segment
        // like `notes.md` always has a real extension.
        .map(|(_, e)| e.to_ascii_lowercase());
    match ext.as_deref() {
        Some("md") | Some("markdown") => Some("markdown"),
        Some("txt") => Some("txt"),
        Some("json") => Some("json"),
        Some("jsonl") | Some("ndjson") => Some("jsonl"),
        _ => None,
    }
}

/// JS `path.posix.dirname(p)` — the directory portion of a POSIX path. Reproduces
/// Node's algorithm: empty → `"."`; strip trailing slashes; the substring before
/// the last remaining `/` (or `"/"` for a root path, `"."` when there is no
/// separator). Used only by [`is_photos_relative_path`].
fn posix_dirname(p: &str) -> String {
    if p.is_empty() {
        return ".".to_string();
    }
    let bytes = p.as_bytes();
    let has_root = bytes[0] == b'/';
    // Find the last slash that is not a run of trailing slashes.
    let mut end: Option<usize> = None; // index of the last non-trailing-slash slash
    let mut matched_slash = true;
    let mut i = p.len();
    while i > 1 {
        i -= 1;
        if bytes[i] == b'/' {
            if !matched_slash {
                end = Some(i);
                break;
            }
        } else {
            matched_slash = false;
        }
    }
    match end {
        None => {
            if has_root {
                "/".to_string()
            } else {
                ".".to_string()
            }
        }
        Some(0) => "/".to_string(),
        Some(e) if has_root && e == 1 => "/".to_string(),
        Some(e) => p[..e].to_string(),
    }
}

/// True when a `doc_mount_file_links.relativePath` lives in a `photos/` folder —
/// v4 `isPhotosRelativePath` (`lib/photos/photos-paths.ts`). Case-insensitive
/// (matching the rest of the mount-index lookups): the path's `dirname`,
/// lowercased, equals `"photos"` or starts with `"photos/"`. `None`/empty → false.
/// Used by the stale-chat sweep to protect album-saved generated images.
pub fn is_photos_relative_path(relative_path: Option<&str>) -> bool {
    let Some(rp) = relative_path.filter(|s| !s.is_empty()) else {
        return false;
    };
    let folder = posix_dirname(rp).to_lowercase();
    folder == "photos" || folder.starts_with("photos/")
}

/// Coerce a frontmatter scalar token to a policy boolean — v4 `coercePolicyBool`
/// (`doc-edit/document-policy.ts:58`). `false`/`no`/`0`/`off`/`n` → false;
/// `true`/`yes`/`1`/`on`/`y` → true; absent/empty/unrecognized → `fallback`
/// (permissive). Case-insensitive, whitespace-trimmed.
pub fn coerce_policy_bool(value: Option<&str>, fallback: bool) -> bool {
    match value {
        None => fallback,
        Some(raw) => {
            let v = raw.trim().to_ascii_lowercase();
            if v.is_empty() {
                return fallback;
            }
            const FALSE_TOKENS: [&str; 5] = ["false", "no", "0", "off", "n"];
            const TRUE_TOKENS: [&str; 5] = ["true", "yes", "1", "on", "y"];
            if FALSE_TOKENS.contains(&v.as_str()) {
                false
            } else if TRUE_TOKENS.contains(&v.as_str()) {
                true
            } else {
                fallback // unrecognized → default
            }
        }
    }
}

/// Read the three policy flags from raw scalar frontmatter values — v4
/// `policyFromFrontmatterData` (`document-policy.ts:83`). `character_read` is the
/// **master gate**: when it is false, `embed` and `character_write` are forced
/// false regardless of their own values (the cascade is materialized here, once).
fn policy_from_frontmatter_scalars(
    character_read: Option<&str>,
    embed: Option<&str>,
    character_write: Option<&str>,
) -> DocumentPolicy {
    let character_read = coerce_policy_bool(character_read, true);
    DocumentPolicy {
        embed: character_read && coerce_policy_bool(embed, true),
        character_read,
        character_write: character_read && coerce_policy_bool(character_write, true),
    }
}

/// Parse raw file text → policy — v4 `policyFromContent` (`document-policy.ts:102`)
/// over `parseFrontmatter` (`markdown-parser.ts:33`). No frontmatter / no closing
/// delimiter / non-markdown → the permissive default. A frontmatter block is only
/// recognized when the content starts with `---\n` and has a `---` on its own
/// line closing it; the three policy keys are read as `key: scalar` lines (the
/// scalar subset — see the module header).
pub fn policy_from_content(content: &str) -> DocumentPolicy {
    let Some(frontmatter) = extract_frontmatter_block(content) else {
        return DEFAULT_DOCUMENT_POLICY;
    };
    let read = scalar_frontmatter_value(frontmatter, "character_read");
    let embed = scalar_frontmatter_value(frontmatter, "embed");
    let write = scalar_frontmatter_value(frontmatter, "character_write");
    policy_from_frontmatter_scalars(read.as_deref(), embed.as_deref(), write.as_deref())
}

/// The YAML text between the opening `---\n` and the closing `---` line, or
/// `None` when there is no well-formed frontmatter block (mirrors
/// `parseFrontmatter` returning `data: null`). Matches v4's exact requirements:
/// the opener must be the very first four bytes (`---\n`), and the closer is the
/// first subsequent line equal to exactly `---`.
fn extract_frontmatter_block(content: &str) -> Option<&str> {
    if !content.starts_with("---\n") {
        return None;
    }
    let lines: Vec<&str> = content.split('\n').collect();
    let closing = lines.iter().skip(1).position(|l| *l == "---")? + 1;
    // YAML lines are lines[1..closing]; recover their slice from the source.
    let yaml_lines = &lines[1..closing];
    // Rebuild only to find the bounds; callers read per-key, so return the join.
    // (Small frontmatter blocks; allocation is fine and keeps the API a &str.)
    // SAFETY of indices: closing >= 1 and < lines.len() by construction.
    let _ = yaml_lines;
    // Return the substring of `content` spanning the YAML body.
    let start = "---\n".len();
    // Offset of the closing delimiter line within `content`.
    let mut offset = start;
    for line in &lines[1..closing] {
        offset += line.len() + 1; // +1 for the '\n'
    }
    // The body is content[start..offset] minus the trailing newline before `---`.
    let body = &content[start..offset];
    Some(body.strip_suffix('\n').unwrap_or(body))
}

/// Pull a single top-level `key: value` scalar from a frontmatter body. Returns
/// the trimmed raw token (so [`coerce_policy_bool`] can interpret it), or `None`
/// when the key is absent. Only the flat scalar form is handled (the policy keys
/// never nest); richer YAML is the deferred seam.
fn scalar_frontmatter_value(frontmatter: &str, key: &str) -> Option<String> {
    for line in frontmatter.split('\n') {
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix(key) {
            // Require the key to be followed by ':' (exact key, not a prefix).
            if let Some(value) = rest.strip_prefix(':') {
                return Some(value.trim().to_string());
            }
        }
    }
    None
}

/// Input to [`DocMountFileLinksRepository::link_document_content`], mirroring v4's
/// `LinkDocumentInput`. The `allow*` overrides are `None` on the normal
/// `writeDatabaseDocument` path (policy is derived from `content`).
pub struct LinkDocumentInput {
    pub mount_point_id: String,
    pub relative_path: String,
    pub file_name: String,
    /// `'markdown' | 'txt' | 'json' | 'jsonl'`.
    pub file_type: String,
    pub content: String,
    pub content_sha256: String,
    /// JS `content.length` — UTF-16 code units (NOT UTF-8 byte length).
    pub plain_text_length: i64,
    /// UTF-8 byte length (`Buffer.byteLength(content, 'utf-8')`).
    pub file_size_bytes: i64,
    pub allow_embed: Option<bool>,
    pub allow_character_read: Option<bool>,
    pub allow_character_write: Option<bool>,
}

/// What [`DocMountFileLinksRepository::link_document_content`] minted/resolved.
pub struct LinkDocumentResult {
    pub link_id: String,
    pub file_id: String,
    pub document_id: String,
    /// Hard-link group members this write repointed; each needs re-chunking
    /// (chunks are keyed by `linkId`). Empty for the ordinary ungrouped write.
    pub group_siblings: Vec<GroupSibling>,
}

/// Input to [`DocMountFileLinksRepository::link_blob_content`], mirroring v4's
/// `LinkBlobInput` (`doc-mount-file-links.repository.ts:128`) — the BINARY
/// analogue of [`LinkDocumentInput`]. `sha256` is advisory: `link_blob_content`
/// recomputes it from `data` and uses the computed value for dedup + both
/// inserts (see v4 lines 577-598).
pub struct LinkBlobInput {
    pub mount_point_id: String,
    pub relative_path: String,
    pub file_name: String,
    /// File-row `fileType`. `None` → `'blob'` (no chunkable text). pdf/docx
    /// declare their type so the conversion pipeline picks them up.
    pub file_type: Option<String>,
    pub original_file_name: String,
    pub original_mime_type: String,
    pub stored_mime_type: String,
    /// Advisory only — recomputed from `data`.
    pub sha256: String,
    /// Already-transcoded bytes destined for `doc_mount_blobs`.
    pub data: Vec<u8>,
    /// `None` → `''` (link `description` default).
    pub description: Option<String>,
    /// `None` → derived from `file_type` (`'blob'` → `'skipped'`, else `'pending'`).
    pub conversion_status: Option<String>,
    pub extracted_text: Option<String>,
    pub extracted_text_sha256: Option<String>,
    /// `None` → `'none'`.
    pub extraction_status: Option<String>,
}

/// What [`DocMountFileLinksRepository::link_blob_content`] minted/resolved (the
/// `blobId` + `fileId` + `linkId`, mirroring v4's `{ link, file, blobId }`).
pub struct LinkBlobResult {
    pub link_id: String,
    pub file_id: String,
    pub blob_id: String,
    /// Hard-link group members this write repointed (see
    /// [`LinkDocumentResult::group_siblings`]).
    pub group_siblings: Vec<GroupSibling>,
}

/// The three columns both write paths read off an existing link before
/// upserting it — v4 selects `id, fileId, linkGroupId` (previously just `id`).
struct ExistingLink {
    id: String,
    file_id: String,
    link_group_id: Option<String>,
}

fn map_existing_link(row: &rusqlite::Row<'_>) -> rusqlite::Result<ExistingLink> {
    Ok(ExistingLink {
        id: row.get(0)?,
        file_id: row.get(1)?,
        link_group_id: row.get(2)?,
    })
}

/// Repository over a borrowed connection (held by the [`super::Writer`]).
pub struct DocMountFileLinksRepository<'c> {
    conn: &'c Connection,
}

impl<'c> DocMountFileLinksRepository<'c> {
    pub fn new(conn: &'c Connection) -> Self {
        Self { conn }
    }

    /// The plain base-repository `create(data, {id})` this table never needed
    /// until restore (P4.9G5 phase 22d). Every other writer here goes through a
    /// purpose-built `link_*` helper that MINTS the row from a file on disk; a
    /// restore instead re-lays a row that already exists, column for column, with
    /// its original id.
    ///
    /// `row` is the archive's projection, where a NULL column is omitted rather
    /// than written as `null`; the defaults below are the schema's
    /// (`description ''`, `conversionStatus 'pending'`, `extractionStatus
    /// 'none'`, `chunkCount 0`, the three policy flags `true`).
    pub fn create_from_row(
        &self,
        row: &serde_json::Value,
        id: &str,
        now: &str,
    ) -> Result<(), DbError> {
        let s = |k: &str| {
            row.get(k)
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
        };
        let os = |k: &str| row.get(k).and_then(serde_json::Value::as_str);
        let on = |k: &str| row.get(k).and_then(serde_json::Value::as_f64);
        let ob = |k: &str, d: bool| {
            i64::from(row.get(k).and_then(serde_json::Value::as_bool).unwrap_or(d))
        };
        self.conn.execute(
            "INSERT INTO doc_mount_file_links (\
               id, fileId, linkGroupId, mountPointId, relativePath, fileName, folderId, \
               originalFileName, originalMimeType, description, descriptionUpdatedAt, \
               conversionStatus, conversionError, plainTextLength, \
               extractedText, extractedTextSha256, extractionStatus, extractionError, \
               chunkCount, allowEmbed, allowCharacterRead, allowCharacterWrite, \
               lastModified, createdAt, updatedAt\
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, \
                       ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25)",
            rusqlite::params![
                id,
                s("fileId"),
                // v4 `40319484` additive. An archive taken before the column
                // existed simply omits it, which is the ordinary un-grouped NULL.
                os("linkGroupId"),
                s("mountPointId"),
                s("relativePath"),
                s("fileName"),
                os("folderId"),
                os("originalFileName"),
                os("originalMimeType"),
                row.get("description")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default(),
                os("descriptionUpdatedAt"),
                row.get("conversionStatus")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("pending"),
                os("conversionError"),
                on("plainTextLength"),
                os("extractedText"),
                os("extractedTextSha256"),
                row.get("extractionStatus")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("none"),
                os("extractionError"),
                on("chunkCount").unwrap_or(0.0),
                ob("allowEmbed", true),
                ob("allowCharacterRead", true),
                ob("allowCharacterWrite", true),
                s("lastModified"),
                now,
                now,
            ],
        )?;
        Ok(())
    }

    /// v4 `writeDatabaseDocument` (`database-store.ts:102`): normalize the path,
    /// detect the (text-only) file type, compute the content sha + lengths,
    /// land the bytes via [`Self::link_document_content`], then run v4's
    /// post-write chunk pass (P4.6BK) so the document is immediately
    /// searchable — best-effort, an overwrite re-chunks. The mtime-conflict
    /// guard (`expectedMtime`) remains out of scope (a standing deferral of its
    /// own). v4's `QUILLTAP_JOB_CHILD` skip does not port — v5's job runner is
    /// in-process, so the buffered-write condition it dodges cannot occur (see
    /// `reindex_after_database_write`).
    pub fn write_database_document(
        &self,
        mount_point_id: &str,
        relative_path: &str,
        content: &str,
    ) -> Result<LinkDocumentResult, DbError> {
        let rel = normalise_relative_path(relative_path)?;
        let file_type = detect_database_file_type(&rel).ok_or_else(|| {
            DbError::Internal(format!(
                "database-backed stores only accept text documents; got path: {rel}"
            ))
        })?;
        let content_sha256 = sha256_of_string(content);
        let file_name = basename(&rel).to_string();

        let result = self.link_document_content(&LinkDocumentInput {
            mount_point_id: mount_point_id.to_string(),
            relative_path: rel.clone(),
            file_name,
            file_type: file_type.to_string(),
            content: content.to_string(),
            content_sha256,
            // content.length (UTF-16 code units), Buffer.byteLength (UTF-8).
            plain_text_length: content.encode_utf16().count() as i64,
            file_size_bytes: content.len() as i64,
            allow_embed: None,
            allow_character_read: None,
            allow_character_write: None,
        })?;

        // Chunk the just-written content (v4 `database-store.ts:133-155`) — the
        // link lands with chunkCount 0 and stays semantically unsearchable
        // until chunked. Best-effort: a failure warns, never fails the write.
        crate::services::mount_index::reindex_file::reindex_after_database_write(
            self.conn,
            mount_point_id,
            &rel,
        );

        // `link_document_content` has already repointed every member of this
        // file's hard-link group at the new content row, but chunks are
        // per-link: without this pass a sibling path would keep serving the
        // previous revision's chunks to search and to character context (v4
        // `database-store.ts:158`, its own try/log after the chunk pass).
        crate::services::mount_index::link_groups::reindex_link_group_siblings_after_database_write(
            self.conn,
            mount_point_id,
            &rel,
        );

        Ok(result)
    }

    /// v4 `linkDocumentContent` (`doc-mount-file-links.repository.ts:738`). The
    /// content/link split in a single transaction (see the module header). Mints
    /// `now` + any new ids internally; returns the resolved file / document / link
    /// ids.
    pub fn link_document_content(
        &self,
        input: &LinkDocumentInput,
    ) -> Result<LinkDocumentResult, DbError> {
        self.link_document_content_with_ids(input, &CarriedRowIds::default())
    }

    /// [`Self::link_document_content`] with explicit row ids for the rows this
    /// call CREATES — the `preserveIds` import path's entrance (see
    /// [`CarriedRowIds`]). Every other caller wants the plain form.
    pub fn link_document_content_with_ids(
        &self,
        input: &LinkDocumentInput,
        carried: &CarriedRowIds,
    ) -> Result<LinkDocumentResult, DbError> {
        let now = now_iso();

        // Per-document policy: derive from markdown frontmatter, else permissive.
        let parsed_policy = if input.file_type == "markdown" {
            policy_from_content(&input.content)
        } else {
            DEFAULT_DOCUMENT_POLICY
        };
        let allow_embed = i64::from(input.allow_embed.unwrap_or(parsed_policy.embed));
        let allow_character_read = i64::from(
            input
                .allow_character_read
                .unwrap_or(parsed_policy.character_read),
        );
        let allow_character_write = i64::from(
            input
                .allow_character_write
                .unwrap_or(parsed_policy.character_write),
        );

        let tx = self.conn.unchecked_transaction()?;

        // 1. find-or-create doc_mount_files by sha (dedup).
        let file_id: String = match tx
            .query_row(
                "SELECT id FROM doc_mount_files WHERE sha256 = ?1",
                params![input.content_sha256],
                |row| row.get::<_, String>(0),
            )
            .map(Some)
            .or_else(no_rows_to_none)?
        {
            Some(id) => id,
            None => {
                // [`01e481f6`] A `preserveIds` import may claim this row's id — honored
                // ONLY here, where the row is actually being created.
                let id = carried.file_id.clone().unwrap_or_else(new_id);
                tx.execute(
                    "INSERT INTO doc_mount_files \
                       (id, sha256, fileSizeBytes, fileType, source, createdAt, updatedAt) \
                     VALUES (?1, ?2, ?3, ?4, 'database', ?5, ?6)",
                    params![
                        id,
                        input.content_sha256,
                        input.file_size_bytes,
                        input.file_type,
                        now,
                        now
                    ],
                )?;
                id
            }
        };

        // 2. upsert doc_mount_documents by fileId (the bytes land here, once).
        let document_id: String = match tx
            .query_row(
                "SELECT id FROM doc_mount_documents WHERE fileId = ?1",
                params![file_id],
                |row| row.get::<_, String>(0),
            )
            .map(Some)
            .or_else(no_rows_to_none)?
        {
            Some(id) => id,
            None => {
                // [`01e481f6`] A `preserveIds` import may claim this row's id — honored
                // ONLY here, where the row is actually being created.
                let id = carried.document_id.clone().unwrap_or_else(new_id);
                tx.execute(
                    "INSERT INTO doc_mount_documents \
                       (id, fileId, content, contentSha256, plainTextLength, createdAt, updatedAt) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![
                        id,
                        file_id,
                        input.content,
                        input.content_sha256,
                        input.plain_text_length,
                        now,
                        now
                    ],
                )?;
                id
            }
        };

        // 3. derive folderId from relativePath (find-or-create folder segments).
        //    canonicalDir carries the stored folder casing so the link's path
        //    never disagrees with the folder rows except in the leaf name.
        let (folder_id, canonical_dir) =
            ensure_link_folder_id(&tx, &input.mount_point_id, &input.relative_path, &now)?;
        let canonical_rel = if canonical_dir.is_empty() {
            input.file_name.clone()
        } else {
            format!("{canonical_dir}/{}", input.file_name)
        };

        // 4. Case-insensitive, case-preserving upsert: a write to `NOTES.md`
        //    updates the row stored as `notes.md` and keeps its casing.
        let existing_link: Option<ExistingLink> = tx
            .query_row(
                "SELECT id, fileId, linkGroupId FROM doc_mount_file_links \
                 WHERE mountPointId = ?1 AND relativePath = ?2 COLLATE NOCASE",
                params![input.mount_point_id, canonical_rel],
                map_existing_link,
            )
            .map(Some)
            .or_else(no_rows_to_none)?;

        let mut group_siblings: Vec<GroupSibling> = Vec::new();
        let link_id = if let Some(existing) = existing_link {
            let link_id = existing.id;
            tx.execute(
                "UPDATE doc_mount_file_links SET \
                   fileId = ?1, folderId = ?2, \
                   plainTextLength = ?3, \
                   conversionStatus = 'converted', conversionError = NULL, \
                   allowEmbed = ?4, allowCharacterRead = ?5, allowCharacterWrite = ?6, \
                   lastModified = ?7, updatedAt = ?8 \
                 WHERE id = ?9",
                params![
                    file_id,
                    folder_id,
                    input.plain_text_length,
                    allow_embed,
                    allow_character_read,
                    allow_character_write,
                    now,
                    now,
                    link_id,
                ],
            )?;
            // Deliberate hard links move together, then the abandoned content row
            // (if this write orphaned it) is collected. Order matters: the
            // siblings must already be repointed before the GC counts the
            // references still pointing at the old row.
            group_siblings = fan_out_group_file_id(
                &tx,
                existing.link_group_id.as_deref(),
                &link_id,
                &file_id,
                &now,
                Some(&FanOutTextState {
                    plain_text_length: input.plain_text_length,
                    allow_embed,
                    allow_character_read,
                    allow_character_write,
                }),
            )?;
            if existing.file_id != file_id {
                gc_orphaned_file_row(&tx, &existing.file_id)?;
            }
            link_id
        } else {
            // [`01e481f6`] A `preserveIds` import may claim this row's id — honored
            // ONLY here, where the row is actually being created.
            let link_id = carried.link_id.clone().unwrap_or_else(new_id);
            tx.execute(
                "INSERT INTO doc_mount_file_links ( \
                   id, fileId, mountPointId, relativePath, fileName, folderId, \
                   conversionStatus, plainTextLength, \
                   allowEmbed, allowCharacterRead, allowCharacterWrite, \
                   chunkCount, lastModified, createdAt, updatedAt \
                 ) VALUES ( \
                   ?1, ?2, ?3, ?4, ?5, ?6, \
                   'converted', ?7, \
                   ?8, ?9, ?10, \
                   0, ?11, ?12, ?13 \
                 )",
                params![
                    link_id,
                    file_id,
                    input.mount_point_id,
                    canonical_rel,
                    input.file_name,
                    folder_id,
                    input.plain_text_length,
                    allow_embed,
                    allow_character_read,
                    allow_character_write,
                    now,
                    now,
                    now,
                ],
            )?;
            link_id
        };

        tx.commit()?;

        if !group_siblings.is_empty() {
            tracing::debug!(
                target: "quilltap::mount_index",
                link_id = %link_id,
                siblings = group_siblings.len(),
                file_id = %file_id,
                "linkDocumentContent: fanned write out to hard-link group",
            );
        }

        Ok(LinkDocumentResult {
            link_id,
            file_id,
            document_id,
            group_siblings,
        })
    }

    /// v4 `linkBlobContent` (`doc-mount-file-links.repository.ts:562`) — the
    /// BINARY analogue of [`Self::link_document_content`]. Writes a binary asset
    /// into a database-backed mount as a hard-linkable resource in a single
    /// transaction:
    ///   1. **find-or-create `doc_mount_files` by the sha RECOMPUTED from `data`**
    ///      (the store owns its own hashes — the caller's `sha256` is advisory,
    ///      warned-on-mismatch in v4; here we simply use the computed value);
    ///   2. **upsert `doc_mount_blobs` by `fileId`** — the bytes land here, only
    ///      when the file row is new (a reused content row keeps its identical
    ///      bytes);
    ///   3. derive `folderId` from `relativePath` via [`ensure_link_folder_id`];
    ///   4. **upsert `doc_mount_file_links` by `(mountPointId, relativePath)`** —
    ///      rewriting a path updates the link in place.
    ///
    /// The Rust INSERTs list **exactly v4's column subset**, so SQLite fills the
    /// same column DEFAULTs on the unset columns (the `link_document_content`
    /// precedent). Mints `now` + any new ids internally.
    pub fn link_blob_content(&self, input: &LinkBlobInput) -> Result<LinkBlobResult, DbError> {
        self.link_blob_content_with_ids(input, &CarriedRowIds::default())
    }

    /// [`Self::link_blob_content`] with explicit row ids for the rows this call
    /// CREATES — the `preserveIds` import path's entrance (see
    /// [`CarriedRowIds`]). Every other caller wants the plain form.
    pub fn link_blob_content_with_ids(
        &self,
        input: &LinkBlobInput,
        carried: &CarriedRowIds,
    ) -> Result<LinkBlobResult, DbError> {
        // v4 lazily creates the blob table on first repo access (P4.6y parity
        // for stores minted at runtime).
        crate::db::doc_mount_blobs::DocMountBlobsRepository::ensure_table(self.conn)?;
        let now = now_iso();
        let size_bytes = input.data.len() as i64;
        // The content-addressed store is authoritative about its own hashes:
        // recompute sha256 from the actual bytes rather than trusting the caller
        // (v4 lines 577-598 — warns on disagreement, uses the computed value).
        let computed = hex::encode(Sha256::digest(&input.data));

        let file_type = input.file_type.as_deref().unwrap_or("blob");
        // Default per-link conversion lifecycle: a `blob` fileType has no
        // chunkable text (skipped); pdf/docx start `pending` (v4 line 611).
        let conversion_status = input.conversion_status.clone().unwrap_or_else(|| {
            if file_type == "blob" {
                "skipped".to_string()
            } else {
                "pending".to_string()
            }
        });

        let tx = self.conn.unchecked_transaction()?;

        // 1. find-or-create doc_mount_files by the computed sha (dedup).
        let file_id: String = match tx
            .query_row(
                "SELECT id FROM doc_mount_files WHERE sha256 = ?1",
                params![computed],
                |row| row.get::<_, String>(0),
            )
            .map(Some)
            .or_else(no_rows_to_none)?
        {
            Some(id) => id,
            None => {
                // [`01e481f6`] A `preserveIds` import may claim this row's id — honored
                // ONLY here, where the row is actually being created.
                let id = carried.file_id.clone().unwrap_or_else(new_id);
                tx.execute(
                    "INSERT INTO doc_mount_files \
                       (id, sha256, fileSizeBytes, fileType, source, createdAt, updatedAt) \
                     VALUES (?1, ?2, ?3, ?4, 'database', ?5, ?6)",
                    params![id, computed, size_bytes, file_type, now, now],
                )?;
                id
            }
        };

        // 3. derive folderId from relativePath (find-or-create folder segments).
        //    (v4 derives folderId before the blob upsert; order is immaterial to
        //    the DB result — both run inside the one transaction.) canonicalDir
        //    carries the stored folder casing so the link's path never disagrees
        //    with the folder rows except in the leaf name.
        let (folder_id, canonical_dir) =
            ensure_link_folder_id(&tx, &input.mount_point_id, &input.relative_path, &now)?;
        let canonical_rel = if canonical_dir.is_empty() {
            input.file_name.clone()
        } else {
            format!("{canonical_dir}/{}", input.file_name)
        };

        // 2. upsert doc_mount_blobs by fileId (the bytes land here, once). A
        //    reused content row keeps its identical bytes (insert only if missing).
        let blob_id: String = match tx
            .query_row(
                "SELECT id FROM doc_mount_blobs WHERE fileId = ?1",
                params![file_id],
                |row| row.get::<_, String>(0),
            )
            .map(Some)
            .or_else(no_rows_to_none)?
        {
            Some(id) => id,
            None => {
                // [`01e481f6`] A `preserveIds` import may claim this row's id — honored
                // ONLY here, where the row is actually being created.
                let id = carried.blob_id.clone().unwrap_or_else(new_id);
                tx.execute(
                    "INSERT INTO doc_mount_blobs \
                       (id, fileId, sha256, sizeBytes, storedMimeType, data, createdAt, updatedAt) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    params![
                        id,
                        file_id,
                        computed,
                        size_bytes,
                        input.stored_mime_type,
                        input.data,
                        now,
                        now
                    ],
                )?;
                id
            }
        };

        // 4. upsert doc_mount_file_links by (mountPointId, relativePath).
        let description = input.description.clone().unwrap_or_default();
        let description_updated_at: Option<String> = if description.is_empty() {
            None
        } else {
            Some(now.clone())
        };
        let extraction_status = input.extraction_status.as_deref().unwrap_or("none");

        // Case-insensitive, case-preserving upsert (see link_document_content):
        // a re-write in a different casing updates the existing row in place and
        // keeps its stored relativePath/fileName casing.
        let existing_link: Option<ExistingLink> = tx
            .query_row(
                "SELECT id, fileId, linkGroupId FROM doc_mount_file_links \
                 WHERE mountPointId = ?1 AND relativePath = ?2 COLLATE NOCASE",
                params![input.mount_point_id, canonical_rel],
                map_existing_link,
            )
            .map(Some)
            .or_else(no_rows_to_none)?;

        let mut group_siblings: Vec<GroupSibling> = Vec::new();
        let link_id = if let Some(existing) = existing_link {
            let link_id = existing.id;
            tx.execute(
                "UPDATE doc_mount_file_links SET \
                   fileId = ?1, folderId = ?2, \
                   originalFileName = ?3, originalMimeType = ?4, \
                   description = ?5, descriptionUpdatedAt = ?6, \
                   extractedText = ?7, extractedTextSha256 = ?8, extractionStatus = ?9, \
                   lastModified = ?10, updatedAt = ?11 \
                 WHERE id = ?12",
                params![
                    file_id,
                    folder_id,
                    input.original_file_name,
                    input.original_mime_type,
                    description,
                    description_updated_at,
                    input.extracted_text,
                    input.extracted_text_sha256,
                    extraction_status,
                    now,
                    now,
                    link_id,
                ],
            )?;
            // Bytes are shared, so the whole group moves; each member keeps its
            // own description and extracted caption (no text state).
            group_siblings = fan_out_group_file_id(
                &tx,
                existing.link_group_id.as_deref(),
                &link_id,
                &file_id,
                &now,
                None,
            )?;
            if existing.file_id != file_id {
                gc_orphaned_file_row(&tx, &existing.file_id)?;
            }
            link_id
        } else {
            // [`01e481f6`] A `preserveIds` import may claim this row's id — honored
            // ONLY here, where the row is actually being created.
            let link_id = carried.link_id.clone().unwrap_or_else(new_id);
            tx.execute(
                "INSERT INTO doc_mount_file_links ( \
                   id, fileId, mountPointId, relativePath, fileName, folderId, \
                   originalFileName, originalMimeType, \
                   description, descriptionUpdatedAt, \
                   conversionStatus, conversionError, plainTextLength, \
                   extractedText, extractedTextSha256, extractionStatus, extractionError, \
                   chunkCount, lastModified, createdAt, updatedAt \
                 ) VALUES ( \
                   ?1, ?2, ?3, ?4, ?5, ?6, \
                   ?7, ?8, \
                   ?9, ?10, \
                   ?11, NULL, NULL, \
                   ?12, ?13, ?14, NULL, \
                   0, ?15, ?16, ?17 \
                 )",
                params![
                    link_id,
                    file_id,
                    input.mount_point_id,
                    canonical_rel,
                    input.file_name,
                    folder_id,
                    input.original_file_name,
                    input.original_mime_type,
                    description,
                    description_updated_at,
                    conversion_status,
                    input.extracted_text,
                    input.extracted_text_sha256,
                    extraction_status,
                    now,
                    now,
                    now,
                ],
            )?;
            link_id
        };

        tx.commit()?;

        if !group_siblings.is_empty() {
            tracing::debug!(
                target: "quilltap::mount_index",
                link_id = %link_id,
                siblings = group_siblings.len(),
                file_id = %file_id,
                "linkBlobContent: fanned write out to hard-link group",
            );
        }

        Ok(LinkBlobResult {
            link_id,
            file_id,
            blob_id,
            group_siblings,
        })
    }

    /// v4 `deleteDatabaseDocument` (`database-store.ts`): unlink a document by
    /// `(mountPointId, relativePath)` with GC. Returns `false` when no link exists
    /// at that path (v4's `NOT_FOUND`-tolerant early return), else `true`.
    pub fn delete_database_document(
        &self,
        mount_point_id: &str,
        relative_path: &str,
    ) -> Result<bool, DbError> {
        let rel = normalise_relative_path(relative_path)?;
        let link_id: Option<String> = self
            .conn
            .query_row(
                "SELECT id FROM doc_mount_file_links \
                 WHERE mountPointId = ?1 AND LOWER(relativePath) = LOWER(?2)",
                params![mount_point_id, rel],
                |row| row.get::<_, String>(0),
            )
            .map(Some)
            .or_else(no_rows_to_none)?;
        let Some(link_id) = link_id else {
            return Ok(false);
        };
        self.delete_with_gc(&link_id)?;
        Ok(true)
    }

    // ========================================================================
    // Deliberate hard-link groups (v4 `40319484`)
    // ========================================================================

    /// Enrol two links in the same hard-link group, so a write through either one
    /// repoints both (see [`fan_out_group_file_id`]) — v4 `bindLinkGroup`.
    /// Reuses the source's existing group when it already has one, so linking a
    /// third location to an already-linked file EXTENDS the group rather than
    /// splitting it (and leaves the source row's `updatedAt` untouched).
    ///
    /// Only `docs link` calls this. `docs copy` deliberately does not: a copy
    /// that happens to share a content row through sha dedup must still fork on
    /// the next write, which is exactly what a null group gives you.
    ///
    /// Returns the group id both links now carry, or `None` if either link is
    /// gone.
    pub fn bind_link_group(
        &self,
        source_link_id: &str,
        dest_link_id: &str,
    ) -> Result<Option<String>, DbError> {
        let now = now_iso();
        let tx = self.conn.unchecked_transaction()?;

        let source: Option<(String, Option<String>)> = tx
            .query_row(
                "SELECT id, linkGroupId FROM doc_mount_file_links WHERE id = ?1",
                params![source_link_id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
            )
            .map(Some)
            .or_else(no_rows_to_none)?;
        let dest_exists: Option<String> = tx
            .query_row(
                "SELECT id FROM doc_mount_file_links WHERE id = ?1",
                params![dest_link_id],
                |row| row.get::<_, String>(0),
            )
            .map(Some)
            .or_else(no_rows_to_none)?;

        let (Some((_, source_group)), Some(_)) = (source, dest_exists) else {
            return Ok(None);
        };

        let group_id = source_group.clone().unwrap_or_else(new_id);
        if source_group.is_none() {
            tx.execute(
                "UPDATE doc_mount_file_links SET linkGroupId = ?1, updatedAt = ?2 WHERE id = ?3",
                params![group_id, now, source_link_id],
            )?;
        }
        tx.execute(
            "UPDATE doc_mount_file_links SET linkGroupId = ?1, updatedAt = ?2 WHERE id = ?3",
            params![group_id, now, dest_link_id],
        )?;
        tx.commit()?;

        tracing::debug!(
            target: "quilltap::mount_index",
            source_link_id = %source_link_id,
            dest_link_id = %dest_link_id,
            group_id = %group_id,
            "Bound links into hard-link group",
        );
        Ok(Some(group_id))
    }

    /// Every link in a hard-link group, joined with content fields — v4
    /// `findByLinkGroupId`. Used to re-chunk the siblings a write just
    /// repointed.
    pub fn find_by_link_group_id(&self, link_group_id: &str) -> Result<Vec<LinkRow>, DbError> {
        let mut stmt = self
            .conn
            .prepare(&Self::join_query("WHERE l.linkGroupId = ?1"))?;
        let rows = stmt
            .query_map(params![link_group_id], Self::map_link_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// v4 `DocMountFileLinksRepository.deleteWithGC`: delete the link row, then —
    /// if it was the last link referencing its file — delete the file row and its
    /// payload too. Chunks cascade off the link (FK `ON DELETE CASCADE`, where the
    /// tables came from the migration); the document/blob payload is deleted
    /// EXPLICITLY through [`gc_orphaned_file_row`], because schema-generated
    /// tables carry no foreign keys at all and a cascade would silently keep every
    /// payload forever (v4 `40319484` — v5 shared that leak until now). No-op when
    /// the link id is unknown. Returns `fileGC` — `true` when this was the last
    /// link and the file row was reclaimed.
    pub fn delete_with_gc(&self, link_id: &str) -> Result<bool, DbError> {
        let link: Option<(String, Option<String>)> = self
            .conn
            .query_row(
                "SELECT fileId, linkGroupId FROM doc_mount_file_links WHERE id = ?1",
                params![link_id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
            )
            .map(Some)
            .or_else(no_rows_to_none)?;
        let Some((file_id, link_group_id)) = link else {
            return Ok(false);
        };

        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "DELETE FROM doc_mount_file_links WHERE id = ?1",
            params![link_id],
        )?;

        // A group of one is not a hard link any more — unlinking the last sibling
        // must leave an ordinary independent file behind, or the survivor would
        // keep a dangling group id that a future link could accidentally join.
        if let Some(group_id) = link_group_id {
            let survivors: i64 = tx.query_row(
                "SELECT COUNT(*) FROM doc_mount_file_links WHERE linkGroupId = ?1",
                params![group_id],
                |row| row.get(0),
            )?;
            if survivors <= 1 {
                tx.execute(
                    "UPDATE doc_mount_file_links SET linkGroupId = NULL, updatedAt = ?1 \
                     WHERE linkGroupId = ?2",
                    params![now_iso(), group_id],
                )?;
            }
        }

        let file_gc = gc_orphaned_file_row(&tx, &file_id)?;
        tx.commit()?;
        Ok(file_gc)
    }

    /// Idempotently create every missing `doc_mount_folders` segment along
    /// `folderPath`, returning the leaf folder id — v4 `ensureFolderPath`
    /// (`folder-paths.ts:103`). Unlike [`ensure_link_folder_id`] (which walks a
    /// file's *dirname*) this walks the path directly, so a single-segment path
    /// like `"Prompts"` creates one root-level folder. `None` for the empty/root
    /// path. Mints `now` + ids internally. Used by [`super::character_vault`]'s
    /// scaffold for the seven explicit top-level folders.
    pub fn ensure_folder_path(
        &self,
        mount_point_id: &str,
        folder_path: &str,
    ) -> Result<Option<String>, DbError> {
        let normalized = collapse_slashes(&folder_path.replace('\\', "/"));
        if normalized.is_empty() {
            return Ok(None);
        }
        let segments: Vec<&str> = normalized.split('/').filter(|s| !s.is_empty()).collect();
        if segments.is_empty() {
            return Ok(None);
        }

        let now = now_iso();
        let mut current_parent: Option<String> = None;
        let mut current_path = String::new();

        for segment in segments {
            // Canonical prefix (stored casing) + the requested leaf. The folder
            // namespace is case-insensitive and case-preserving: a segment that
            // matches an existing folder except for casing reuses it and the walk
            // continues under the folder's STORED casing (v4 folder-paths.ts).
            let requested_path = if current_path.is_empty() {
                segment.to_string()
            } else {
                format!("{current_path}/{segment}")
            };

            let found: Option<(String, String)> = self
                .conn
                .query_row(
                    "SELECT id, path FROM doc_mount_folders \
                     WHERE mountPointId = ?1 AND path = ?2 COLLATE NOCASE \
                     ORDER BY (path = ?3) DESC LIMIT 1",
                    params![mount_point_id, requested_path, requested_path],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
                )
                .map(Some)
                .or_else(no_rows_to_none)?;

            match found {
                Some((id, stored_path)) => {
                    current_parent = Some(id);
                    current_path = stored_path;
                }
                None => {
                    let id = new_id();
                    self.conn.execute(
                        "INSERT INTO doc_mount_folders \
                           (id, mountPointId, parentId, name, path, createdAt, updatedAt) \
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                        params![
                            id,
                            mount_point_id,
                            current_parent,
                            segment,
                            requested_path,
                            now,
                            now
                        ],
                    )?;
                    current_parent = Some(id);
                    current_path = requested_path;
                }
            }
        }

        Ok(current_parent)
    }

    /// v4 `linkFilesystemFile` (`doc-mount-file-links.repository.ts:873`) — the
    /// scanner/reindex upsert for filesystem-indexed files: find-or-create the
    /// content row by `(sha256, source)` (keeping its UUID stable across
    /// rewrites), derive `folderId` from the relativePath, then insert or
    /// re-point the `(mountPointId, relativePath)` link at the new content.
    /// Returns the link id. Timestamps + minted ids use a fresh `now` (the
    /// tier-2 differentials normalize them).
    pub fn link_filesystem_file(&self, input: &LinkFilesystemFileInput) -> Result<String, DbError> {
        let now = now_iso();
        let source = input.source.as_deref().unwrap_or("filesystem");

        let existing_file: Option<String> = self
            .conn
            .query_row(
                "SELECT id FROM doc_mount_files WHERE sha256 = ?1 AND source = ?2",
                params![input.sha256, source],
                |row| row.get::<_, String>(0),
            )
            .map(Some)
            .or_else(no_rows_to_none)?;
        let file_id = match existing_file {
            Some(id) => id,
            None => {
                let id = new_id();
                self.conn.execute(
                    "INSERT INTO doc_mount_files (id, sha256, fileSizeBytes, fileType, source, createdAt, updatedAt) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![
                        id,
                        input.sha256,
                        input.file_size_bytes,
                        input.file_type,
                        source,
                        now,
                        now
                    ],
                )?;
                id
            }
        };

        // v4: folderId is DERIVED from relativePath here (the scanner passes
        // none) — `ensureLinkFolderId` walks the dirname. The canonical dir is
        // ignored — the filesystem is the source of truth for these rows, so the
        // scanned relativePath is used verbatim below.
        let (folder_id, _canonical_dir) =
            ensure_link_folder_id(self.conn, &input.mount_point_id, &input.relative_path, &now)?;

        // NOCASE match so a case-only rename on disk updates the existing row
        // instead of minting a case-variant duplicate. Unlike the database-store
        // writers, the UPDATE below ADOPTS the scanned casing (relativePath +
        // fileName) — the filesystem is the source of truth for these rows.
        let existing_link: Option<String> = self
            .conn
            .query_row(
                "SELECT id FROM doc_mount_file_links \
                 WHERE mountPointId = ?1 AND relativePath = ?2 COLLATE NOCASE",
                params![input.mount_point_id, input.relative_path],
                |row| row.get::<_, String>(0),
            )
            .map(Some)
            .or_else(no_rows_to_none)?;

        let conversion_status = input.conversion_status.as_deref().unwrap_or("pending");
        let plain_text_length = input.plain_text_length;
        let chunk_count = input.chunk_count.unwrap_or(0.0);
        // Per-document policy (markdown frontmatter). Default permissive.
        let allow_embed = i64::from(input.allow_embed != Some(false));
        let allow_character_read = i64::from(input.allow_character_read != Some(false));
        let allow_character_write = i64::from(input.allow_character_write != Some(false));

        let link_id = match existing_link {
            Some(link_id) => {
                self.conn.execute(
                    "UPDATE doc_mount_file_links SET \
                       fileId = ?1, relativePath = ?2, fileName = ?3, folderId = ?4, \
                       conversionStatus = ?5, conversionError = ?6, \
                       plainTextLength = ?7, chunkCount = ?8, \
                       allowEmbed = ?9, allowCharacterRead = ?10, allowCharacterWrite = ?11, \
                       lastModified = ?12, updatedAt = ?13 \
                     WHERE id = ?14",
                    params![
                        file_id,
                        input.relative_path,
                        input.file_name,
                        folder_id,
                        conversion_status,
                        input.conversion_error,
                        plain_text_length,
                        chunk_count,
                        allow_embed,
                        allow_character_read,
                        allow_character_write,
                        input.last_modified,
                        now,
                        link_id
                    ],
                )?;
                link_id
            }
            None => {
                let link_id = new_id();
                self.conn.execute(
                    "INSERT INTO doc_mount_file_links (\
                       id, fileId, mountPointId, relativePath, fileName, folderId, \
                       conversionStatus, conversionError, plainTextLength, \
                       allowEmbed, allowCharacterRead, allowCharacterWrite, \
                       chunkCount, lastModified, createdAt, updatedAt\
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
                    params![
                        link_id,
                        file_id,
                        input.mount_point_id,
                        input.relative_path,
                        input.file_name,
                        folder_id,
                        conversion_status,
                        input.conversion_error,
                        plain_text_length,
                        allow_embed,
                        allow_character_read,
                        allow_character_write,
                        chunk_count,
                        input.last_modified,
                        now,
                        now
                    ],
                )?;
                link_id
            }
        };
        Ok(link_id)
    }

    /// v4 `updatePolicyFlags` — rewrite the three per-document policy columns.
    pub fn update_policy_flags(
        &self,
        link_id: &str,
        policy: &DocumentPolicy,
    ) -> Result<(), DbError> {
        self.conn.execute(
            "UPDATE doc_mount_file_links \
               SET allowEmbed = ?1, allowCharacterRead = ?2, allowCharacterWrite = ?3, updatedAt = ?4 \
             WHERE id = ?5",
            params![
                i64::from(policy.embed),
                i64::from(policy.character_read),
                i64::from(policy.character_write),
                now_iso(),
                link_id
            ],
        )?;
        Ok(())
    }

    /// The lowercased `relativePath` of every link at `mountPointId` — v4's
    /// `vaultHasRequiredFiles` reads `findByMountPointId(...).map(l =>
    /// l.relativePath.toLowerCase())` into a `Set`. Returned as a `Vec` so the
    /// caller builds its own membership test (the character-vault adopt path checks
    /// the six `REQUIRED_VAULT_FILES` against it).
    pub fn relative_paths_lower(&self, mount_point_id: &str) -> Result<Vec<String>, DbError> {
        let mut stmt = self
            .conn
            .prepare("SELECT relativePath FROM doc_mount_file_links WHERE mountPointId = ?1")?;
        let paths = stmt
            .query_map(params![mount_point_id], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<String>, _>>()?
            .into_iter()
            .map(|p| p.to_lowercase())
            .collect();
        Ok(paths)
    }

    /// True iff a link already exists at `(mountPointId, relativePath)` — v4's
    /// scaffold skip-if-present check (it consults `docMountDocuments`, but a
    /// document exists at a path iff its link does). Case-insensitive on the path,
    /// matching the link upsert / delete lookups.
    pub fn link_exists_at_path(
        &self,
        mount_point_id: &str,
        relative_path: &str,
    ) -> Result<bool, DbError> {
        let rel = normalise_relative_path(relative_path)?;
        let found: Option<i64> = self
            .conn
            .query_row(
                "SELECT 1 FROM doc_mount_file_links \
                 WHERE mountPointId = ?1 AND LOWER(relativePath) = LOWER(?2)",
                params![mount_point_id, rel],
                |row| row.get::<_, i64>(0),
            )
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other),
            })?;
        Ok(found.is_some())
    }

    /// v4 `findByMountPointAndPath` (the joined view, `doc-mount-file-links
    /// .repository.ts:352`): the single link at a `(mountPointId, relativePath)`,
    /// **case-insensitive** on the path (`LOWER(l.relativePath) = LOWER(?)`), or
    /// `None`. Drives the database-store move/read (the link half).
    pub fn find_by_mount_point_and_path(
        &self,
        mount_point_id: &str,
        relative_path: &str,
    ) -> Result<Option<LinkRow>, DbError> {
        self.conn
            .query_row(
                &Self::join_query(
                    "WHERE l.mountPointId = ?1 AND LOWER(l.relativePath) = LOWER(?2)",
                ),
                params![mount_point_id, relative_path],
                Self::map_link_row,
            )
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other.into()),
            })
    }

    /// v4 `findByMountPointId` (the joined view, `doc-mount-file-links.repository
    /// .ts:338`): every link for a mount point with the content fields joined in,
    /// in the DB's natural order (callers filter/rewrite). Drives the listing +
    /// move-folder descendant rewrite + `folderHasContents`'s link scan.
    pub fn find_by_mount_point_id(&self, mount_point_id: &str) -> Result<Vec<LinkRow>, DbError> {
        let mut stmt = self
            .conn
            .prepare(&Self::join_query("WHERE l.mountPointId = ?1"))?;
        let rows = stmt
            .query_map(params![mount_point_id], Self::map_link_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// v4 `findByFileId` (`doc-mount-file-links.repository.ts:375`): every link
    /// referencing one file (the inverse of the FK), joined with content fields.
    /// Drives the stale-chat sweep's sha256→album/vault-link reverse index.
    pub fn find_by_file_id(&self, file_id: &str) -> Result<Vec<LinkRow>, DbError> {
        let mut stmt = self
            .conn
            .prepare(&Self::join_query("WHERE l.fileId = ?1"))?;
        let rows = stmt
            .query_map(params![file_id], Self::map_link_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// v4 `findByIdWithContent` (`doc-mount-file-links.repository.ts:387`): one link
    /// by its primary key, joined with content fields (`f.sha256` etc.). `None` when
    /// absent. Drives the character-avatar sha256 resolution (vault-link path).
    /// (Renamed from `find_by_id_with_content` in the W4.8+W4.9b integration so it
    /// coexists with the photo-tools `LinkWithContent` port of the same v4 method;
    /// this one returns the full `LinkRow` shape the stale-chat sweep consumes.)
    pub fn find_link_row_by_id(&self, link_id: &str) -> Result<Option<LinkRow>, DbError> {
        self.conn
            .query_row(
                &Self::join_query("WHERE l.id = ?1"),
                params![link_id],
                Self::map_link_row,
            )
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other.into()),
            })
    }

    /// v4 `docMountFiles.findById` — the CONTENT row (`doc_mount_files`) by
    /// primary key, not a link. Only its `sha256` is ever wanted: the
    /// `preserveIds` preflight (`01e481f6`, Bug 54) settles a carried
    /// content-row id by comparing the live row's hash against the bundle's,
    /// because the content tables are found-or-created by sha256 and a matching
    /// row means dedup rather than a collision.
    pub fn find_content_row_by_id(&self, file_id: &str) -> Result<Option<ContentRow>, DbError> {
        self.conn
            .query_row(
                "SELECT id, sha256 FROM doc_mount_files WHERE id = ?1",
                params![file_id],
                |row| {
                    Ok(ContentRow {
                        id: row.get(0)?,
                        sha256: row.get(1)?,
                    })
                },
            )
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other.into()),
            })
    }

    /// v4 `findByIdWithContent(id)` (`doc-mount-file-links.repository.ts`): a single
    /// link by its id, with the file-row content fields (`sha256` / `fileSizeBytes`)
    /// joined in — the shape the photo tools (`attach_image` / the
    /// `saveImageToAlbum` mount-blob fallback) consume. `None` when absent.
    pub fn find_by_id_with_content(&self, id: &str) -> Result<Option<LinkWithContent>, DbError> {
        self.conn
            .query_row(LINK_WITH_CONTENT_SELECT, params![id], map_link_with_content)
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other.into()),
            })
    }

    /// v4 `findByFileId(fileId)` in the with-content shape — every link whose
    /// `fileId` matches, joined with the file-row content fields. The W4.4b
    /// `loadMountFileAsAttachment` mount-fallback path reads `[0]` (v4 treats the
    /// arg as a file id when the link-id lookup misses). Insertion (rowid) order.
    pub fn find_with_content_by_file_id(
        &self,
        file_id: &str,
    ) -> Result<Vec<LinkWithContent>, DbError> {
        let sql = LINK_WITH_CONTENT_SELECT.replace("WHERE l.id = ?1", "WHERE l.fileId = ?1");
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt
            .query_map(params![file_id], map_link_with_content)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Set the chunk rollups on a link (v4 `chunkAndInsertExtractedText`'s
    /// `links.update({ chunkCount, plainTextLength })` beat). `plainTextLength` is
    /// deterministic (UTF-16 length of the stored `extractedText`); `chunkCount` is
    /// v4's `reindexSingleFile`-family value, pinned in the differential (see the
    /// groups/projects `<cc>` precedent), so the Rust port — which does not carry
    /// the chunker — writes `0` here. Bumps `updatedAt` like v4's `update`.
    pub fn set_chunk_rollups(
        &self,
        link_id: &str,
        chunk_count: i64,
        plain_text_length: i64,
        updated_at: &str,
    ) -> Result<bool, DbError> {
        let affected = self.conn.execute(
            "UPDATE doc_mount_file_links \
               SET chunkCount = ?1, plainTextLength = ?2, updatedAt = ?3 WHERE id = ?4",
            params![chunk_count, plain_text_length, updated_at, link_id],
        )?;
        Ok(affected > 0)
    }

    /// The auto-describe blank-link update (v4
    /// `auto-describe-attachment.ts`'s `repos.docMountFileLinks.update(link.id,
    /// { description, descriptionUpdatedAt, extractedText, extractedTextSha256,
    /// extractionStatus: 'converted' })`). `description_updated_at` is the
    /// module's single pre-loop `now`; `updatedAt` is stamped at call time (v4's
    /// base `_update` uses its own `getCurrentTimestamp()`). Returns `Ok(false)`
    /// when no row matched.
    pub fn apply_auto_description(
        &self,
        link_id: &str,
        description: &str,
        extracted_text_sha256: &str,
        description_updated_at: &str,
        updated_at: &str,
    ) -> Result<bool, DbError> {
        let affected = self.conn.execute(
            "UPDATE doc_mount_file_links \
               SET description = ?1, descriptionUpdatedAt = ?2, \
                   extractedText = ?1, extractedTextSha256 = ?3, \
                   extractionStatus = 'converted', updatedAt = ?4 \
             WHERE id = ?5",
            params![
                description,
                description_updated_at,
                extracted_text_sha256,
                updated_at,
                link_id
            ],
        )?;
        Ok(affected > 0)
    }

    /// Delete every `doc_mount_chunks` row for a link (v4
    /// `docMountChunks.deleteByLinkId`). Used by the chunk-rollup pass to clear
    /// prior chunks before re-chunking; a no-op when none exist.
    pub fn delete_chunks_by_link_id(&self, link_id: &str) -> Result<usize, DbError> {
        let affected = self.conn.execute(
            "DELETE FROM doc_mount_chunks WHERE linkId = ?1",
            params![link_id],
        )?;
        Ok(affected)
    }

    /// Apply a link update patch to `link_id`. Follows the dynamic-SET pattern of
    /// `doc_mount_folders`'s `update`; `folder_id` supports set-to-NULL via the
    /// `Option<Option<_>>` shape. Returns `Ok(false)` when no row matched.
    pub fn update(&self, link_id: &str, patch: &LinkUpdate) -> Result<bool, DbError> {
        let mut assignments: Vec<String> = Vec::new();
        let mut values: Vec<Box<dyn ToSql>> = Vec::new();

        if let Some(relative_path) = &patch.relative_path {
            assignments.push(format!("relativePath = ?{}", values.len() + 1));
            values.push(Box::new(relative_path.clone()));
        }
        if let Some(file_name) = &patch.file_name {
            assignments.push(format!("fileName = ?{}", values.len() + 1));
            values.push(Box::new(file_name.clone()));
        }
        if let Some(folder_id) = &patch.folder_id {
            // Some(None) => SQL NULL; Some(Some(id)) => the id.
            assignments.push(format!("folderId = ?{}", values.len() + 1));
            values.push(Box::new(folder_id.clone()));
        }
        if let Some(mount_point_id) = &patch.mount_point_id {
            assignments.push(format!("mountPointId = ?{}", values.len() + 1));
            values.push(Box::new(mount_point_id.clone()));
        }
        if let Some(file_id) = &patch.file_id {
            assignments.push(format!("fileId = ?{}", values.len() + 1));
            values.push(Box::new(file_id.clone()));
        }
        if let Some(conversion_status) = &patch.conversion_status {
            assignments.push(format!("conversionStatus = ?{}", values.len() + 1));
            values.push(Box::new(conversion_status.clone()));
        }
        if let Some(conversion_error) = &patch.conversion_error {
            assignments.push(format!("conversionError = ?{}", values.len() + 1));
            values.push(Box::new(conversion_error.clone()));
        }
        if let Some(plain_text_length) = &patch.plain_text_length {
            assignments.push(format!("plainTextLength = ?{}", values.len() + 1));
            values.push(Box::new(*plain_text_length));
        }
        if let Some(chunk_count) = patch.chunk_count {
            assignments.push(format!("chunkCount = ?{}", values.len() + 1));
            values.push(Box::new(chunk_count));
        }
        if let Some(extracted_text) = &patch.extracted_text {
            assignments.push(format!("extractedText = ?{}", values.len() + 1));
            values.push(Box::new(extracted_text.clone()));
        }
        if let Some(extracted_text_sha256) = &patch.extracted_text_sha256 {
            assignments.push(format!("extractedTextSha256 = ?{}", values.len() + 1));
            values.push(Box::new(extracted_text_sha256.clone()));
        }
        if let Some(extraction_status) = &patch.extraction_status {
            assignments.push(format!("extractionStatus = ?{}", values.len() + 1));
            values.push(Box::new(extraction_status.clone()));
        }
        if let Some(extraction_error) = &patch.extraction_error {
            assignments.push(format!("extractionError = ?{}", values.len() + 1));
            values.push(Box::new(extraction_error.clone()));
        }
        if let Some(last_modified) = &patch.last_modified {
            assignments.push(format!("lastModified = ?{}", values.len() + 1));
            values.push(Box::new(last_modified.clone()));
        }
        assignments.push(format!("updatedAt = ?{}", values.len() + 1));
        values.push(Box::new(patch.updated_at.clone()));

        let id_idx = values.len() + 1;
        values.push(Box::new(link_id.to_string()));

        let sql = format!(
            "UPDATE doc_mount_file_links SET {} WHERE id = ?{}",
            assignments.join(", "),
            id_idx
        );
        let params_refs: Vec<&dyn ToSql> = values.iter().map(|b| b.as_ref()).collect();
        let affected = self.conn.execute(&sql, params_refs.as_slice())?;
        Ok(affected > 0)
    }

    /// The `l JOIN f` SELECT used by both finders, with the caller's WHERE clause
    /// appended. Column list + sources mirror v4 `queryJoined` (restricted to the
    /// [`LinkRow`] fields).
    fn join_query(where_clause: &str) -> String {
        format!(
            "SELECT \
               l.id, l.fileId, l.mountPointId, l.relativePath, l.fileName, l.folderId, \
               l.lastModified, l.createdAt, \
               l.allowCharacterRead, l.allowCharacterWrite, \
               l.extractedText, l.originalMimeType, l.conversionStatus, l.chunkCount, \
               f.sha256, f.fileSizeBytes, f.fileType, f.source, l.description, \
               l.extractionStatus, l.allowEmbed, l.linkGroupId, l.originalFileName \
             FROM doc_mount_file_links l \
             JOIN doc_mount_files f ON f.id = l.fileId \
             {where_clause}"
        )
    }

    fn map_link_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<LinkRow> {
        Ok(LinkRow {
            id: row.get(0)?,
            file_id: row.get(1)?,
            mount_point_id: row.get(2)?,
            relative_path: row.get(3)?,
            file_name: row.get(4)?,
            folder_id: row.get(5)?,
            last_modified: row.get(6)?,
            created_at: row.get(7)?,
            // v4 coerceAllow: absent/NULL → permissive true, else `!= 0`.
            allow_character_read: coerce_allow(row.get::<_, Option<i64>>(8)?),
            allow_character_write: coerce_allow(row.get::<_, Option<i64>>(9)?),
            extracted_text: row.get(10)?,
            original_mime_type: row.get(11)?,
            conversion_status: row.get(12)?,
            // `chunkCount` has REAL affinity (`z.number()` DDL, `REAL DEFAULT 0`),
            // so a cell may be stored as Real (`0.0`) or Integer; read the raw
            // value ref and collapse either form to i64.
            chunk_count: real_affinity_i64(row.get_ref(13)?),
            sha256: row.get(14)?,
            // `fileSizeBytes` also has REAL affinity; tolerate Real/Integer.
            file_size_bytes: real_affinity_i64(row.get_ref(15)?),
            file_type: row.get(16)?,
            source: row.get(17)?,
            description: row.get(18)?,
            extraction_status: row.get(19)?,
            allow_embed: coerce_allow(row.get::<_, Option<i64>>(20)?),
            link_group_id: row.get(21)?,
            original_file_name: row.get(22)?,
        })
    }
}

/// Read a REAL-affinity `chunkCount` cell as i64, tolerating both Integer and
/// Real (`0.0`) storage forms (the column is `REAL DEFAULT 0`). NULL/other → 0.
fn real_affinity_i64(v: rusqlite::types::ValueRef<'_>) -> i64 {
    match v {
        rusqlite::types::ValueRef::Integer(i) => i,
        rusqlite::types::ValueRef::Real(f) => f as i64,
        _ => 0,
    }
}

/// Coerce a SQLite `allow*` policy column (stored 0/1, occasionally absent) into a
/// boolean — v4 `coerceAllow` (`doc-mount-file-links.repository.ts:115`). Absent /
/// NULL → permissive (`true`); else `!= 0`.
fn coerce_allow(value: Option<i64>) -> bool {
    match value {
        None => true,
        Some(v) => v != 0,
    }
}

/// Walk every segment of `relativePath`'s directory and find-or-create a
/// `doc_mount_folders` row for each, returning the leaf folder's id — v4
/// `ensureLinkFolderId` (`doc-mount-file-links.repository.ts:60`). Runs inside the
/// caller's transaction so folder rows roll back with a failed link write.
/// `None` when the file is at the mount root (`dir` empty / `.` / `/`).
/// Resolve (creating as needed) the folder chain for a link's `relativePath`,
/// returning the leaf `folder_id` (`None` for a root-level file) and the
/// stored-casing directory path (`canonical_dir`, `""` for root).
///
/// Folder matching is case-insensitive and case-preserving: a segment that
/// matches an existing folder except for casing reuses that folder, and the walk
/// continues under the folder's STORED casing. `canonical_dir` lets callers keep
/// the link's `relativePath` consistent with the folder rows.
fn ensure_link_folder_id(
    tx: &Connection,
    mount_point_id: &str,
    relative_path: &str,
    now: &str,
) -> Result<(Option<String>, String), DbError> {
    let dir = dirname(relative_path);
    if dir.is_empty() || dir == "." || dir == "/" {
        return Ok((None, String::new()));
    }

    // Collapse backslashes + redundant/leading/trailing slashes (v4's regex chain).
    let normalized = collapse_slashes(&dir.replace('\\', "/"));
    if normalized.is_empty() {
        return Ok((None, String::new()));
    }
    let segments: Vec<&str> = normalized.split('/').filter(|s| !s.is_empty()).collect();
    if segments.is_empty() {
        return Ok((None, String::new()));
    }

    let mut current_parent: Option<String> = None;
    let mut current_path = String::new();

    for segment in segments {
        // Canonical prefix (stored casing) + the requested leaf segment.
        let requested_path = if current_path.is_empty() {
            segment.to_string()
        } else {
            format!("{current_path}/{segment}")
        };

        // Exact match wins; the NOCASE fallback rides the case-insensitive
        // unique index on (mountPointId, parentId, name)-equivalent paths.
        let found: Option<(String, String)> = tx
            .query_row(
                "SELECT id, path FROM doc_mount_folders \
                 WHERE mountPointId = ?1 AND path = ?2 COLLATE NOCASE \
                 ORDER BY (path = ?3) DESC LIMIT 1",
                params![mount_point_id, requested_path, requested_path],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .map(Some)
            .or_else(no_rows_to_none)?;

        match found {
            Some((id, stored_path)) => {
                current_parent = Some(id);
                current_path = stored_path;
            }
            None => {
                let id = new_id();
                tx.execute(
                    "INSERT INTO doc_mount_folders \
                       (id, mountPointId, parentId, name, path, createdAt, updatedAt) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![
                        id,
                        mount_point_id,
                        current_parent,
                        segment,
                        requested_path,
                        now,
                        now
                    ],
                )?;
                current_parent = Some(id);
                current_path = requested_path;
            }
        }
    }

    Ok((current_parent, current_path))
}

/// Normalise a database-store relative path — v4 `normaliseRelativePath`
/// (`database-store.ts:51`): backslashes → `/`, strip leading/trailing slashes,
/// reject any `..` traversal segment. (The corpus uses already-clean POSIX paths;
/// full Node `path.normalize` `./`/`../` resolution is not reproduced — the store
/// paths never contain them.)
pub fn normalise_relative_path(relative_path: &str) -> Result<String, DbError> {
    let normalised = collapse_slashes(&relative_path.replace('\\', "/"));
    if normalised.split('/').any(|s| s == "..") {
        return Err(DbError::Internal(format!(
            "invalid relative path (traversal): {relative_path}"
        )));
    }
    Ok(normalised)
}

/// Collapse runs of `/` and strip leading/trailing `/` (the `/\/+/`,`/^\/+|\/+$/`
/// chain). Does not resolve `.`/`..`.
fn collapse_slashes(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_slash = false;
    for ch in s.chars() {
        if ch == '/' {
            if !prev_slash {
                out.push('/');
            }
            prev_slash = true;
        } else {
            out.push(ch);
            prev_slash = false;
        }
    }
    out.trim_matches('/').to_string()
}

/// POSIX `path.dirname` for a clean relative path: everything before the last
/// `/`, or `.` when there is none.
fn dirname(path: &str) -> String {
    match path.rsplit_once('/') {
        Some((dir, _)) if !dir.is_empty() => dir.to_string(),
        Some((_, _)) => "/".to_string(), // leading slash case
        None => ".".to_string(),
    }
}

/// POSIX `path.basename` for a clean relative path: everything after the last `/`.
fn basename(path: &str) -> &str {
    match path.rsplit_once('/') {
        Some((_, name)) => name,
        None => path,
    }
}

/// Mint a v4-style id (`crypto.randomUUID()` → RFC-4122 v4).
fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// Map `QueryReturnedNoRows` to `Ok(None)`, propagate other errors.
fn no_rows_to_none<T>(e: rusqlite::Error) -> Result<Option<T>, rusqlite::Error> {
    match e {
        rusqlite::Error::QueryReturnedNoRows => Ok(None),
        other => Err(other),
    }
}

// ============================================================================
// P4.1d append-only additions (the maintenance sweep's orphan reaper)
// ============================================================================

/// v4 `DocMountFileLinksRepository.sweepOrphanedFiles()` (P4.1d): the
/// reconciliation sweep — delete every `doc_mount_files` row that has no
/// surviving `doc_mount_file_links` row (belt-and-suspenders after the
/// stale-chat asset collapse; a writer that bypassed `deleteWithGC` leaves these
/// behind). Returns the count deleted.
///
/// Runs against the **mount-index** partition (`getRawMountIndexDatabase()` in
/// v4 — the caller supplies that connection). v4 wraps the body in `safeQuery`
/// with a `0` default (an absent mount-index handle → 0, an error → 0); the
/// port surfaces the error to the caller, which maps a missing partition /
/// failure back to v4's observable `0` + swallowed shape (see
/// `services::scheduled_maintenance`).
pub fn sweep_orphaned_files(conn: &Connection) -> Result<usize, DbError> {
    let changes = conn.execute(
        "DELETE FROM doc_mount_files \
         WHERE id NOT IN (SELECT DISTINCT fileId FROM doc_mount_file_links)",
        [],
    )?;
    Ok(changes)
}

// ============================================================================
// Deliberate hard-link groups (v4 `40319484`)
// ============================================================================

/// Identity of a link, enough to re-index it — v4 `GroupSibling`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GroupSibling {
    pub id: String,
    pub mount_point_id: String,
    pub relative_path: String,
}

/// The text-shaped columns [`fan_out_group_file_id`] carries along; omitted for
/// blobs (v4's `textState` parameter).
pub(crate) struct FanOutTextState {
    pub plain_text_length: i64,
    pub allow_embed: i64,
    pub allow_character_read: i64,
    pub allow_character_write: i64,
}

/// Fan a content repoint out to the rest of a deliberate hard-link group — v4
/// `fanOutGroupFileId` (`doc-mount-file-links.repository.ts:62`).
///
/// This is what makes `docs link` behave like a POSIX hard link: a write
/// through any member moves EVERY member onto the new content row, so no member
/// can silently drift to stale bytes. Callers pass the group id read off the
/// link they just wrote; a null group is a no-op (an ordinary, independent link
/// — including one that merely shares a content-addressed `fileId` with an
/// unrelated file of identical bytes).
///
/// Per-link metadata is deliberately NOT propagated. Two consumers of the same
/// bytes may keep their own `description` and their own extracted text /
/// caption — that independence is a documented property of the link model, and
/// only the bytes are shared. Chunks are keyed by `linkId` and are rebuilt by
/// the caller (see `services::mount_index::link_groups`), not here.
///
/// Runs inside the caller's transaction (`conn` is the transaction handle).
///
/// Returns the siblings that were repointed (excluding `exclude_link_id`).
pub(crate) fn fan_out_group_file_id(
    conn: &Connection,
    group_id: Option<&str>,
    exclude_link_id: &str,
    new_file_id: &str,
    now: &str,
    text_state: Option<&FanOutTextState>,
) -> Result<Vec<GroupSibling>, DbError> {
    let Some(group_id) = group_id else {
        return Ok(Vec::new());
    };

    let siblings: Vec<GroupSibling> = {
        let mut stmt = conn.prepare(
            "SELECT id, mountPointId, relativePath FROM doc_mount_file_links \
             WHERE linkGroupId = ?1 AND id <> ?2",
        )?;
        let rows = stmt
            .query_map(params![group_id, exclude_link_id], |row| {
                Ok(GroupSibling {
                    id: row.get(0)?,
                    mount_point_id: row.get(1)?,
                    relative_path: row.get(2)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        rows
    };
    if siblings.is_empty() {
        return Ok(Vec::new());
    }

    match text_state {
        Some(t) => {
            conn.execute(
                "UPDATE doc_mount_file_links SET \
                   fileId = ?1, plainTextLength = ?2, \
                   conversionStatus = 'converted', conversionError = NULL, \
                   allowEmbed = ?3, allowCharacterRead = ?4, allowCharacterWrite = ?5, \
                   lastModified = ?6, updatedAt = ?7 \
                 WHERE linkGroupId = ?8 AND id <> ?9",
                params![
                    new_file_id,
                    t.plain_text_length,
                    t.allow_embed,
                    t.allow_character_read,
                    t.allow_character_write,
                    now,
                    now,
                    group_id,
                    exclude_link_id,
                ],
            )?;
        }
        None => {
            conn.execute(
                "UPDATE doc_mount_file_links SET fileId = ?1, lastModified = ?2, updatedAt = ?3 \
                 WHERE linkGroupId = ?4 AND id <> ?5",
                params![new_file_id, now, now, group_id, exclude_link_id],
            )?;
        }
    }

    Ok(siblings)
}

/// Drop a content row that no link references any more — v4 `gcOrphanedFileRow`
/// (`doc-mount-file-links.repository.ts:114`).
///
/// Every write to a database-backed mount is content-addressed: it
/// finds-or-creates a `doc_mount_files` row for the NEW sha and repoints the
/// link at it. Without this the row the link just left behind lingers forever,
/// holding its `doc_mount_documents` / `doc_mount_blobs` payload — a slow leak
/// that had accumulated dozens of orphans in the wild. Content rows still
/// referenced by some other link (a real hard link, or an unrelated file that
/// happens to have identical bytes) are left alone.
///
/// The payload rows are deleted explicitly rather than left to the FK cascade.
/// `ON DELETE CASCADE` is only present on databases whose tables came from the
/// add-doc-mount-file-links migration; tables created from the Zod schema by
/// `generateDDL` carry no foreign keys at all, so on those instances a cascade
/// would silently keep every payload forever. Deleting children first is a
/// no-op where the cascade does exist.
///
/// The payload tables are created lazily by their repositories on first access
/// (`doc_mount_blobs` has no Zod schema, so `generateDDL` never mints it; a
/// document-only, restored, or hand-built index may likewise never have held a
/// blob). Deleting from a table that was never created throws `no such table` —
/// a hard failure on the SECOND write to any path. So each payload delete is
/// guarded behind a table-existence check (v4 `7bcd8515`, bug 13); a missing
/// table has nothing to collect anyway. `doc_mount_files` stays unguarded — the
/// content row we are collecting had to exist for the link to point at it.
///
/// Runs inside the caller's transaction (`conn` is the transaction handle).
///
/// Returns `true` when the row was collected.
pub(crate) fn gc_orphaned_file_row(conn: &Connection, file_id: &str) -> Result<bool, DbError> {
    let still: i64 = conn.query_row(
        "SELECT COUNT(*) FROM doc_mount_file_links WHERE fileId = ?1",
        params![file_id],
        |row| row.get(0),
    )?;
    if still > 0 {
        return Ok(false);
    }
    if table_exists_sync(conn, "doc_mount_documents")? {
        conn.execute(
            "DELETE FROM doc_mount_documents WHERE fileId = ?1",
            params![file_id],
        )?;
    }
    if table_exists_sync(conn, "doc_mount_blobs")? {
        conn.execute(
            "DELETE FROM doc_mount_blobs WHERE fileId = ?1",
            params![file_id],
        )?;
    }
    conn.execute(
        "DELETE FROM doc_mount_files WHERE id = ?1",
        params![file_id],
    )?;
    Ok(true)
}

/// True when `table` exists in the connection's schema — v4 `tableExistsSync`
/// (`doc-mount-file-links.repository.ts`, bug 13). Used to guard the lazily
/// created payload deletes in [`gc_orphaned_file_row`].
fn table_exists_sync(conn: &Connection, table: &str) -> Result<bool, DbError> {
    let exists: Option<String> = conn
        .query_row(
            "SELECT name FROM sqlite_master WHERE type = 'table' AND name = ?1",
            params![table],
            |row| row.get(0),
        )
        .map(Some)
        .or_else(no_rows_to_none)?;
    Ok(exists.is_some())
}

/// Collect the backlog of content rows abandoned by content-addressed rewrites
/// before [`gc_orphaned_file_row`] existed — v4 migration
/// `add-doc-mount-link-groups-v1` step 2.
///
/// v5 has no migration runner (a locked deferral), so this runs from the
/// `services::builtin_mounts` boot hook instead, on the P4.d7 boot-repair
/// precedent: idempotent every boot, a cheap indexed no-op once the backlog is
/// gone (the write path now reaps eagerly).
///
/// It deliberately reuses [`gc_orphaned_file_row`] rather than being a third
/// independent reaper: the count re-check is trivially true for a row already
/// known to be orphaned, and the payload deletes then cannot drift apart from
/// the write path's. Distinct from [`sweep_orphaned_files`], which is v4's
/// unchanged maintenance sweep (cascade-reliant, and ported byte-faithfully).
///
/// Returns the number of content rows collected.
pub fn sweep_orphaned_link_content(conn: &Connection) -> Result<usize, DbError> {
    // v4's migration `shouldRun` gates on `doc_mount_file_links` existing; the
    // sweep additionally names the three content tables. A legacy-vintage mount
    // index can be missing any of them, and where v4 contains a migration
    // failure (`success: false`, logged) a throw from this boot hook would abort
    // startup — so the gate covers everything the sweep touches.
    for table in [
        "doc_mount_file_links",
        "doc_mount_files",
        "doc_mount_documents",
        "doc_mount_blobs",
    ] {
        let exists: Option<String> = conn
            .query_row(
                "SELECT name FROM sqlite_master WHERE type = 'table' AND name = ?1",
                params![table],
                |row| row.get(0),
            )
            .map(Some)
            .or_else(no_rows_to_none)?;
        if exists.is_none() {
            return Ok(0);
        }
    }

    let orphans: Vec<String> = {
        let mut stmt = conn.prepare(
            "SELECT f.id FROM doc_mount_files f \
             WHERE NOT EXISTS (SELECT 1 FROM doc_mount_file_links l WHERE l.fileId = f.id)",
        )?;
        let rows = stmt
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        rows
    };

    let tx = conn.unchecked_transaction()?;
    let mut collected = 0usize;
    for file_id in &orphans {
        if gc_orphaned_file_row(&tx, file_id)? {
            collected += 1;
        }
    }
    tx.commit()?;

    if collected > 0 {
        tracing::info!(
            target: "quilltap::mount_repair",
            collected,
            "Collected orphaned document-store content rows left by content-addressed rewrites",
        );
    }
    Ok(collected)
}

/// What one run of [`sweep_orphaned_store_children`] collected.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ReapedStoreChildren {
    pub links: usize,
    pub folders: usize,
    pub chunks: usize,
    /// `doc_mount_files` rows (plus their document / blob payload) collected
    /// because the reaped links were their last reference.
    pub content: usize,
}

impl ReapedStoreChildren {
    /// True when the pass found nothing — the steady state after the first boot.
    pub fn is_empty(&self) -> bool {
        self.links == 0 && self.folders == 0 && self.chunks == 0 && self.content == 0
    }
}

/// The ORPHAN REAPER (P4.31) — collect the `doc_mount_file_links`,
/// `doc_mount_folders` and `doc_mount_chunks` whose `mountPointId` matches no
/// surviving `doc_mount_points` row, plus the file content those links were the
/// last reference to.
///
/// This is the repair half of dogfood finding #58. `api::mount_points::
/// cascade_delete` stops NEW orphans; these are the ones already on disk — 43
/// links and 118 folders across 21 vanished character vaults on the measured
/// instance (2026-08-03), which no query in either app has ever looked for.
/// v4 offers no equivalent, so this is a deliberate v5 divergence, pinned in
/// both directions by `store_delete_equivalence`'s `reap_orphans` arm.
///
/// Distinct from its two neighbours, which key on the OPPOSITE end and cannot
/// see these rows: [`sweep_orphaned_link_content`] and [`sweep_orphaned_files`]
/// both collect content with no LINK, and every orphan here still has one. It
/// deliberately reuses [`gc_orphaned_file_row`] for the payload so the delete
/// set cannot drift from the write path's.
///
/// Idempotent every boot and a cheap indexed no-op once the backlog is gone.
/// Fail-soft on table existence (the P4.d7 / P4.D41 boot-repair shape), in TWO
/// tiers rather than all-or-nothing — the unification review's catch: the
/// content tables are lazily created (`doc_mount_blobs` only when a blob is
/// first stored), so gating the whole pass on all seven would silently no-op
/// the #58 repair forever on a text-only instance, which is exactly the shape
/// that carries the damage. Instead:
///
///  - missing any of the four tables the reaper itself sweeps
///    (`doc_mount_points` / `doc_mount_file_links` / `doc_mount_folders` /
///    `doc_mount_chunks`) → the whole pass is a no-op, WARNED;
///  - missing any of the three content tables (`doc_mount_files` /
///    `doc_mount_documents` / `doc_mount_blobs`) → links/folders/chunks are
///    still reaped, the content-GC leg is skipped, WARNED. A skipped content
///    leg can strand `doc_mount_files` rows — but those are exactly what
///    [`sweep_orphaned_link_content`] collects once the tables exist, so
///    nothing is lost for good.
///
/// Deliberately NOT in scope (recorded, not silently covered): a folder whose
/// PARENT FOLDER is missing while its store lives, and a chunk whose `linkId` is
/// dead under a live store. Both key on something other than `mountPointId`;
/// P4.28 named the first as unmeasured and neither is what #58 is.
pub fn sweep_orphaned_store_children(conn: &Connection) -> Result<ReapedStoreChildren, DbError> {
    let table_exists = |table: &str| table_exists_sync(conn, table);
    for table in [
        "doc_mount_points",
        "doc_mount_file_links",
        "doc_mount_folders",
        "doc_mount_chunks",
    ] {
        if !table_exists(table)? {
            tracing::warn!(
                target: "quilltap::mount_repair",
                missing = table,
                "Orphan reaper skipped whole: a table it sweeps is missing",
            );
            return Ok(ReapedStoreChildren::default());
        }
    }
    let mut gc_content = true;
    for table in ["doc_mount_files", "doc_mount_documents", "doc_mount_blobs"] {
        if !table_exists(table)? {
            tracing::warn!(
                target: "quilltap::mount_repair",
                missing = table,
                "Orphan reaper: content tables incomplete; reaping links/folders/chunks only",
            );
            gc_content = false;
            break;
        }
    }

    // The file ids the doomed links reference — snapshotted BEFORE the delete,
    // for the same reason `cascade_delete` snapshots its own.
    let doomed_files: Vec<String> = if gc_content {
        let mut stmt = conn.prepare(
            "SELECT DISTINCT l.fileId FROM doc_mount_file_links l \
             WHERE NOT EXISTS (SELECT 1 FROM doc_mount_points p WHERE p.id = l.mountPointId)",
        )?;
        let rows = stmt
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        rows
    } else {
        Vec::new()
    };

    // `NOT EXISTS`, not `NOT IN` — the same NULL-safe predicate the snapshot
    // above and the differential's census use, so the delete and its own
    // verification can never disagree on a NULL (unreachable on today's
    // NOT NULL schema, but the predicates should not be able to drift apart).
    let tx = conn.unchecked_transaction()?;
    let chunks = tx.execute(
        "DELETE FROM doc_mount_chunks AS c WHERE NOT EXISTS \
         (SELECT 1 FROM doc_mount_points p WHERE p.id = c.mountPointId)",
        [],
    )?;
    let links = tx.execute(
        "DELETE FROM doc_mount_file_links AS l WHERE NOT EXISTS \
         (SELECT 1 FROM doc_mount_points p WHERE p.id = l.mountPointId)",
        [],
    )?;
    let mut content = 0usize;
    for file_id in &doomed_files {
        if gc_orphaned_file_row(&tx, file_id)? {
            content += 1;
        }
    }
    let folders = tx.execute(
        "DELETE FROM doc_mount_folders AS f WHERE NOT EXISTS \
         (SELECT 1 FROM doc_mount_points p WHERE p.id = f.mountPointId)",
        [],
    )?;
    tx.commit()?;
    let reaped = ReapedStoreChildren {
        links,
        folders,
        chunks,
        content,
    };

    if !reaped.is_empty() {
        tracing::info!(
            target: "quilltap::mount_repair",
            links = reaped.links,
            folders = reaped.folders,
            chunks = reaped.chunks,
            content = reaped.content,
            "Reaped document-store rows whose mount point no longer exists",
        );
    }
    Ok(reaped)
}

#[cfg(test)]
mod orphan_backlog_tests {
    use super::*;

    /// The four tables the backlog sweep touches, in their `generateDDL` shape
    /// (no foreign keys — which is exactly why the payload deletes are explicit).
    fn mount_index_db() -> Connection {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE \"doc_mount_files\" (\"id\" TEXT PRIMARY KEY NOT NULL, \
               \"sha256\" TEXT NOT NULL, \"fileSizeBytes\" REAL);\
             CREATE TABLE \"doc_mount_documents\" (\"id\" TEXT PRIMARY KEY NOT NULL, \
               \"fileId\" TEXT NOT NULL, \"content\" TEXT);\
             CREATE TABLE \"doc_mount_blobs\" (\"id\" TEXT PRIMARY KEY NOT NULL, \
               \"fileId\" TEXT NOT NULL, \"data\" BLOB);\
             CREATE TABLE \"doc_mount_file_links\" (\"id\" TEXT PRIMARY KEY NOT NULL, \
               \"fileId\" TEXT NOT NULL, \"linkGroupId\" TEXT);",
        )
        .unwrap();
        db
    }

    fn seed_content(db: &Connection, file_id: &str) {
        db.execute(
            "INSERT INTO doc_mount_files (id, sha256, fileSizeBytes) VALUES (?1, ?1, 10)",
            params![file_id],
        )
        .unwrap();
        db.execute(
            "INSERT INTO doc_mount_documents (id, fileId, content) VALUES (?1 || '-d', ?1, 'x')",
            params![file_id],
        )
        .unwrap();
        db.execute(
            "INSERT INTO doc_mount_blobs (id, fileId, data) VALUES (?1 || '-b', ?1, x'00')",
            params![file_id],
        )
        .unwrap();
    }

    fn count(db: &Connection, sql: &str) -> i64 {
        db.query_row(sql, [], |row| row.get(0)).unwrap()
    }

    #[test]
    fn collects_orphans_with_their_payload_and_leaves_referenced_rows_alone() {
        let db = mount_index_db();
        seed_content(&db, "orphan");
        seed_content(&db, "kept");
        db.execute(
            "INSERT INTO doc_mount_file_links (id, fileId) VALUES ('l1', 'kept')",
            [],
        )
        .unwrap();

        assert_eq!(sweep_orphaned_link_content(&db).unwrap(), 1);

        // The payload goes with the file row — `ON DELETE CASCADE` does not exist
        // on schema-generated tables, so a cascade would have kept both forever.
        assert_eq!(count(&db, "SELECT COUNT(*) FROM doc_mount_files"), 1);
        assert_eq!(count(&db, "SELECT COUNT(*) FROM doc_mount_documents"), 1);
        assert_eq!(count(&db, "SELECT COUNT(*) FROM doc_mount_blobs"), 1);
        assert_eq!(
            count(
                &db,
                "SELECT COUNT(*) FROM doc_mount_files WHERE id = 'kept'"
            ),
            1
        );

        // Idempotent: the next boot finds nothing.
        assert_eq!(sweep_orphaned_link_content(&db).unwrap(), 0);
    }

    #[test]
    fn is_a_no_op_on_a_legacy_vintage_index_missing_the_content_tables() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE \"doc_mount_file_links\" (\"id\" TEXT PRIMARY KEY NOT NULL, \
               \"fileId\" TEXT NOT NULL)",
        )
        .unwrap();
        assert_eq!(sweep_orphaned_link_content(&db).unwrap(), 0);
    }

    #[test]
    fn gc_leaves_a_content_row_that_another_link_still_references() {
        let db = mount_index_db();
        seed_content(&db, "shared");
        db.execute(
            "INSERT INTO doc_mount_file_links (id, fileId) VALUES ('l1', 'shared')",
            [],
        )
        .unwrap();

        assert!(!gc_orphaned_file_row(&db, "shared").unwrap());
        assert_eq!(count(&db, "SELECT COUNT(*) FROM doc_mount_files"), 1);
        assert_eq!(count(&db, "SELECT COUNT(*) FROM doc_mount_documents"), 1);
    }

    /// Bug 13 (v4 `7bcd8515`): a document-only / restored / hand-built index
    /// never held a blob, so `doc_mount_blobs` was never lazily created. Before
    /// the table guards, `gc_orphaned_file_row`'s `DELETE FROM doc_mount_blobs`
    /// threw `no such table: doc_mount_blobs` — a hard failure on the second
    /// write to any path. The guard must skip the missing table (not create it)
    /// and still collect the file + document rows.
    #[test]
    fn gc_survives_a_mount_index_without_the_blobs_table() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE \"doc_mount_files\" (\"id\" TEXT PRIMARY KEY NOT NULL, \
               \"sha256\" TEXT NOT NULL, \"fileSizeBytes\" REAL);\
             CREATE TABLE \"doc_mount_documents\" (\"id\" TEXT PRIMARY KEY NOT NULL, \
               \"fileId\" TEXT NOT NULL, \"content\" TEXT);\
             CREATE TABLE \"doc_mount_file_links\" (\"id\" TEXT PRIMARY KEY NOT NULL, \
               \"fileId\" TEXT NOT NULL, \"linkGroupId\" TEXT);",
        )
        .unwrap();
        // Content the way a native-text-only index holds it: a files row + a
        // documents row, no blob (the table does not exist at all).
        db.execute(
            "INSERT INTO doc_mount_files (id, sha256, fileSizeBytes) VALUES ('solo', 'solo', 10)",
            [],
        )
        .unwrap();
        db.execute(
            "INSERT INTO doc_mount_documents (id, fileId, content) VALUES ('solo-d', 'solo', 'x')",
            [],
        )
        .unwrap();
        // No link references it → gc must collect it.
        assert!(
            gc_orphaned_file_row(&db, "solo").unwrap(),
            "gc should collect the orphan and NOT throw on the absent blobs table"
        );
        assert_eq!(count(&db, "SELECT COUNT(*) FROM doc_mount_files"), 0);
        assert_eq!(count(&db, "SELECT COUNT(*) FROM doc_mount_documents"), 0);
        // …and the guard skipped the table rather than creating it.
        assert_eq!(
            count(
                &db,
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='doc_mount_blobs'"
            ),
            0
        );
    }
}

#[cfg(test)]
mod store_children_gate_tests {
    //! The reaper's two-tier fail-soft gate (the unification review's catch):
    //! the all-or-nothing seven-table gate would have silently no-opped the
    //! #58 repair FOREVER on a text-only instance, because `doc_mount_blobs`
    //! is lazily created and the boot hook ensures only points + folders.

    use super::*;

    fn reaper_db(with_content_tables: bool, with_links: bool) -> Connection {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE doc_mount_points (id TEXT PRIMARY KEY, name TEXT NOT NULL);\
             CREATE TABLE doc_mount_folders (id TEXT PRIMARY KEY, mountPointId TEXT NOT NULL);\
             CREATE TABLE doc_mount_chunks (id TEXT PRIMARY KEY, mountPointId TEXT NOT NULL);",
        )
        .unwrap();
        if with_links {
            db.execute_batch(
                "CREATE TABLE doc_mount_file_links (id TEXT PRIMARY KEY, \
                   fileId TEXT NOT NULL, mountPointId TEXT NOT NULL);",
            )
            .unwrap();
        }
        if with_content_tables {
            db.execute_batch(
                "CREATE TABLE doc_mount_files (id TEXT PRIMARY KEY, sha256 TEXT NOT NULL);\
                 CREATE TABLE doc_mount_documents (id TEXT PRIMARY KEY, fileId TEXT NOT NULL);\
                 CREATE TABLE doc_mount_blobs (id TEXT PRIMARY KEY, fileId TEXT NOT NULL);",
            )
            .unwrap();
        }
        db
    }

    /// A text-only instance — `doc_mount_blobs` never lazily created — MUST
    /// still have its #58 damage reaped. The old seven-table gate returned
    /// `default()` here; this is red with that gate restored.
    #[test]
    fn reaps_links_folders_chunks_even_when_the_content_tables_are_missing() {
        let db = reaper_db(false, true);
        db.execute_batch(
            "INSERT INTO doc_mount_points VALUES ('live','Keeper');\
             INSERT INTO doc_mount_file_links VALUES ('l1','f1','ghost'),('l2','f2','live');\
             INSERT INTO doc_mount_folders VALUES ('fo1','ghost'),('fo2','live');\
             INSERT INTO doc_mount_chunks VALUES ('c1','ghost');",
        )
        .unwrap();

        let reaped = sweep_orphaned_store_children(&db).unwrap();
        assert_eq!(
            (reaped.links, reaped.folders, reaped.chunks, reaped.content),
            (1, 1, 1, 0),
            "the content leg is skipped (its tables are absent), never the reap itself"
        );
        let survivors: i64 = db
            .query_row("SELECT COUNT(*) FROM doc_mount_file_links", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(survivors, 1, "the live store's link must survive");
    }

    /// Missing one of the four tables the reaper itself sweeps is the genuine
    /// legacy-vintage arm: the whole pass declines, and startup proceeds.
    #[test]
    fn is_a_whole_no_op_when_a_swept_table_is_missing() {
        let db = reaper_db(true, false);
        db.execute_batch("INSERT INTO doc_mount_folders VALUES ('fo1','ghost');")
            .unwrap();
        assert_eq!(
            sweep_orphaned_store_children(&db).unwrap(),
            ReapedStoreChildren::default()
        );
        let folders: i64 = db
            .query_row("SELECT COUNT(*) FROM doc_mount_folders", [], |r| r.get(0))
            .unwrap();
        assert_eq!(folders, 1, "a declined pass must not delete anything");
    }
}
