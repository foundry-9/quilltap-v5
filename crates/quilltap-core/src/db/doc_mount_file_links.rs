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
}

const LINK_WITH_CONTENT_SELECT: &str = "SELECT \
       l.id, l.fileId, l.mountPointId, l.relativePath, l.fileName, \
       l.originalFileName, l.originalMimeType, l.extractedText, l.createdAt, \
       f.sha256, f.fileSizeBytes \
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
    })
}

/// A link update patch — v4 `docMountFileLinks.update(id, Partial<...>)`, narrowed
/// to the columns the move ops set. Each `Some` field sets that column;
/// `folder_id` is `Option<Option<String>>` so the caller can distinguish "leave
/// untouched" (outer `None`) from "set to SQL NULL" (`Some(None)`), matching v4
/// passing `folderId: null` for a root-level move. `updated_at` is always set.
#[derive(Default)]
pub struct LinkUpdate {
    pub relative_path: Option<String>,
    pub file_name: Option<String>,
    pub folder_id: Option<Option<String>>,
    pub updated_at: String,
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
}

/// Repository over a borrowed connection (held by the [`super::Writer`]).
pub struct DocMountFileLinksRepository<'c> {
    conn: &'c Connection,
}

impl<'c> DocMountFileLinksRepository<'c> {
    pub fn new(conn: &'c Connection) -> Self {
        Self { conn }
    }

    /// v4 `writeDatabaseDocument` (`database-store.ts:102`): normalize the path,
    /// detect the (text-only) file type, compute the content sha + lengths, and
    /// land the bytes via [`Self::link_document_content`]. The mtime-conflict
    /// guard (`expectedMtime`) and the post-write `reindexSingleFile` chunk pass
    /// (skipped in v4 when `QUILLTAP_JOB_CHILD=1`) are out of scope — the
    /// differential pins `QUILLTAP_JOB_CHILD=1` so neither runs.
    pub fn write_database_document(
        &self,
        mount_point_id: &str,
        relative_path: &str,
        content: &str,
    ) -> Result<LinkDocumentResult, DbError> {
        let rel = normalise_relative_path(relative_path)?;
        let file_type = detect_database_file_type(&rel).ok_or_else(|| {
            DbError::Key(format!(
                "database-backed stores only accept text documents; got path: {rel}"
            ))
        })?;
        let content_sha256 = sha256_of_string(content);
        let file_name = basename(&rel).to_string();

        self.link_document_content(&LinkDocumentInput {
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
        })
    }

    /// v4 `linkDocumentContent` (`doc-mount-file-links.repository.ts:738`). The
    /// content/link split in a single transaction (see the module header). Mints
    /// `now` + any new ids internally; returns the resolved file / document / link
    /// ids.
    pub fn link_document_content(
        &self,
        input: &LinkDocumentInput,
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
                let id = new_id();
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
                let id = new_id();
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
        let folder_id =
            ensure_link_folder_id(&tx, &input.mount_point_id, &input.relative_path, &now)?;

        // 4. upsert doc_mount_file_links by (mountPointId, relativePath).
        let existing_link: Option<String> = tx
            .query_row(
                "SELECT id FROM doc_mount_file_links WHERE mountPointId = ?1 AND relativePath = ?2",
                params![input.mount_point_id, input.relative_path],
                |row| row.get::<_, String>(0),
            )
            .map(Some)
            .or_else(no_rows_to_none)?;

        let link_id = if let Some(link_id) = existing_link {
            tx.execute(
                "UPDATE doc_mount_file_links SET \
                   fileId = ?1, fileName = ?2, folderId = ?3, \
                   plainTextLength = ?4, \
                   conversionStatus = 'converted', conversionError = NULL, \
                   allowEmbed = ?5, allowCharacterRead = ?6, allowCharacterWrite = ?7, \
                   lastModified = ?8, updatedAt = ?9 \
                 WHERE id = ?10",
                params![
                    file_id,
                    input.file_name,
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
            link_id
        } else {
            let link_id = new_id();
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
                    input.relative_path,
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

        Ok(LinkDocumentResult {
            link_id,
            file_id,
            document_id,
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
                let id = new_id();
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
        //    the DB result — both run inside the one transaction.)
        let folder_id =
            ensure_link_folder_id(&tx, &input.mount_point_id, &input.relative_path, &now)?;

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
                let id = new_id();
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

        let existing_link: Option<String> = tx
            .query_row(
                "SELECT id FROM doc_mount_file_links WHERE mountPointId = ?1 AND relativePath = ?2",
                params![input.mount_point_id, input.relative_path],
                |row| row.get::<_, String>(0),
            )
            .map(Some)
            .or_else(no_rows_to_none)?;

        let link_id = if let Some(link_id) = existing_link {
            tx.execute(
                "UPDATE doc_mount_file_links SET \
                   fileId = ?1, fileName = ?2, folderId = ?3, \
                   originalFileName = ?4, originalMimeType = ?5, \
                   description = ?6, descriptionUpdatedAt = ?7, \
                   extractedText = ?8, extractedTextSha256 = ?9, extractionStatus = ?10, \
                   lastModified = ?11, updatedAt = ?12 \
                 WHERE id = ?13",
                params![
                    file_id,
                    input.file_name,
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
            link_id
        } else {
            let link_id = new_id();
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
                    input.relative_path,
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

        Ok(LinkBlobResult {
            link_id,
            file_id,
            blob_id,
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

    /// v4 `DocMountFileLinksRepository.deleteWithGC`: delete the link row, then —
    /// if it was the last link referencing its file — delete the file row too.
    /// Chunks cascade off the link (FK `ON DELETE CASCADE`); documents/blobs
    /// cascade off the file row. The writable open enforces `foreign_keys = ON`,
    /// so the cascades fire. No-op when the link id is unknown.
    pub fn delete_with_gc(&self, link_id: &str) -> Result<(), DbError> {
        let file_id: Option<String> = self
            .conn
            .query_row(
                "SELECT fileId FROM doc_mount_file_links WHERE id = ?1",
                params![link_id],
                |row| row.get::<_, String>(0),
            )
            .map(Some)
            .or_else(no_rows_to_none)?;
        let Some(file_id) = file_id else {
            return Ok(());
        };

        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "DELETE FROM doc_mount_file_links WHERE id = ?1",
            params![link_id],
        )?;
        let remaining: i64 = tx.query_row(
            "SELECT COUNT(*) FROM doc_mount_file_links WHERE fileId = ?1",
            params![file_id],
            |row| row.get(0),
        )?;
        if remaining == 0 {
            tx.execute(
                "DELETE FROM doc_mount_files WHERE id = ?1",
                params![file_id],
            )?;
        }
        tx.commit()?;
        Ok(())
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
            current_path = if current_path.is_empty() {
                segment.to_string()
            } else {
                format!("{current_path}/{segment}")
            };

            let found: Option<String> = self
                .conn
                .query_row(
                    "SELECT id FROM doc_mount_folders WHERE mountPointId = ?1 AND path = ?2",
                    params![mount_point_id, current_path],
                    |row| row.get::<_, String>(0),
                )
                .map(Some)
                .or_else(no_rows_to_none)?;

            current_parent = match found {
                Some(id) => Some(id),
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
                            current_path,
                            now,
                            now
                        ],
                    )?;
                    Some(id)
                }
            };
        }

        Ok(current_parent)
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

    /// Delete every `doc_mount_chunks` row for a link (v4
    /// `docMountChunks.deleteByLinkId`). Used by the chunk-rollup pass to clear
    /// prior chunks before re-chunking; a no-op when none exist. The Rust port does
    /// not carry the chunker, so a keep writes no new chunk rows — this only clears
    /// any pre-existing ones for a rewrite-in-place.
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
               f.sha256, f.fileSizeBytes, f.fileType, f.source \
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
fn ensure_link_folder_id(
    tx: &Connection,
    mount_point_id: &str,
    relative_path: &str,
    now: &str,
) -> Result<Option<String>, DbError> {
    let dir = dirname(relative_path);
    if dir.is_empty() || dir == "." || dir == "/" {
        return Ok(None);
    }

    // Collapse backslashes + redundant/leading/trailing slashes (v4's regex chain).
    let normalized = collapse_slashes(&dir.replace('\\', "/"));
    if normalized.is_empty() {
        return Ok(None);
    }
    let segments: Vec<&str> = normalized.split('/').filter(|s| !s.is_empty()).collect();
    if segments.is_empty() {
        return Ok(None);
    }

    let mut current_parent: Option<String> = None;
    let mut current_path = String::new();

    for segment in segments {
        current_path = if current_path.is_empty() {
            segment.to_string()
        } else {
            format!("{current_path}/{segment}")
        };

        let found: Option<String> = tx
            .query_row(
                "SELECT id FROM doc_mount_folders WHERE mountPointId = ?1 AND path = ?2",
                params![mount_point_id, current_path],
                |row| row.get::<_, String>(0),
            )
            .map(Some)
            .or_else(no_rows_to_none)?;

        current_parent = match found {
            Some(id) => Some(id),
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
                        current_path,
                        now,
                        now
                    ],
                )?;
                Some(id)
            }
        };
    }

    Ok(current_parent)
}

/// Normalise a database-store relative path — v4 `normaliseRelativePath`
/// (`database-store.ts:51`): backslashes → `/`, strip leading/trailing slashes,
/// reject any `..` traversal segment. (The corpus uses already-clean POSIX paths;
/// full Node `path.normalize` `./`/`../` resolution is not reproduced — the store
/// paths never contain them.)
pub fn normalise_relative_path(relative_path: &str) -> Result<String, DbError> {
    let normalised = collapse_slashes(&relative_path.replace('\\', "/"));
    if normalised.split('/').any(|s| s == "..") {
        return Err(DbError::Key(format!(
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
fn no_rows_to_none(e: rusqlite::Error) -> Result<Option<String>, rusqlite::Error> {
    match e {
        rusqlite::Error::QueryReturnedNoRows => Ok(None),
        other => Err(other),
    }
}
