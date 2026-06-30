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
//! Scope: the write/storage path only. The repo's many read/join/GC/conversion
//! helpers (`findByMountPointId`, `deleteWithGC`, `linkBlobContent`,
//! `linkFilesystemFile`, `sweepOrphanedFiles`, …) are out of scope here.

use rusqlite::{params, Connection};
use sha2::{Digest, Sha256};

use super::DbError;
use crate::clock::now_iso;

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
