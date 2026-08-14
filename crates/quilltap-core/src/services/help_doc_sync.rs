//! The help-docs disk sync (v4 `lib/help/help-doc-sync.ts`) — read the help
//! Markdown tree and upsert into `help_docs`, clearing a changed doc's stored
//! embedding so it re-embeds, and pruning rows whose file is gone.
//!
//! **Design decision (documented per the P4.1b work order):** the DIRECTORY
//! WALK is host-side — the core sync takes the already-read file list
//! ([`HelpSourceFile`], in walk order) so the engine stays fs-free. The host
//! walker (`quilltap-host::files_store::load_help_source_files`) reproduces v4
//! `findMarkdownFiles`' inline-recursive `readdirSync` order (files and
//! directories interleaved in raw readdir order — both Node and Rust issue the
//! same syscall over the same directory, so the order matches for a shared
//! fixture tree). The order only affects `changed_ids` sequencing; the
//! differential compares path-keyed forms.
//!
//! v4's LOCAL `parseFrontmatter` here is deliberately DISTINCT from the shared
//! `lib/markdown/frontmatter` parser (a loose regex, not the structural
//! parser) — ported as its own private helper, never unified with
//! [`crate::markdown::parse_frontmatter`].
//!
//! v4's private `generateDocumentId` was computed-but-unused here (dead code
//! carried from `build-help-index.ts`), and this port skipped it on that
//! ground. **That judgment expired with v4 `d6e74145`**, which DELETED it from
//! `help-doc-sync.ts` and promoted it to the shared module
//! `lib/help/help-doc-slug.ts` as the live `helpDocSlug` — the path-derived
//! identifier everything outside the database uses, since the DB primary key is
//! a UUID that changes whenever a doc is re-created. It is ported at
//! [`crate::help_doc_slug::help_doc_slug`]; the sync itself is not a consumer.
//!
//! The `ensureHelpDocsSynced` module-promise concurrency guard does not port
//! (the single-writer runtime already serializes callers).
//!
//! ## Not wired at startup (a standing deferral, named)
//!
//! [`ensure_help_docs_synced`] has **no production caller** — v4 reaches it from
//! `HelpSearch.loadFromDatabase()`, and that class is unported (see
//! [`crate::help_doc_slug`]). v4's OTHER sync trigger, the
//! `EMBEDDING_REINDEX_ALL` job handler, is unported too: the job type is known
//! (`job_runner.rs`) and `embedding_refit_job.rs` enqueues it, but no handler
//! exists. So this module is correct FOR WHEN that wiring lands, and today only
//! the differential drives it. Wiring it is the host's business (it owns the
//! walk); the standing seam note lives at `db/help_search.rs`.

use rusqlite::{params, Connection};

use crate::db::embedding_status::EmbeddingStatusRepository;
use crate::db::help_doc_chunks::HelpDocChunksRepository;
use crate::db::help_docs::{HdUpsert, HelpDocsRepository};
use crate::db::runtime::Db;
use crate::db::DbError;
use crate::jsstr::{is_js_ws, js_trim};
use crate::services::help_doc_chunking::build_help_doc_chunks;
use crate::services::mount_index::embedding_scheduler::{
    default_or_first_profile_id, first_user_id,
};
use crate::services::queue_service::enqueue_embedding_generate;

/// One on-disk help file (the host walker's output): `rel_path` is v4's
/// `relative(process.cwd(), filePath)` (e.g. `help/aurora.md`), `raw_content`
/// the UTF-8 file text.
#[derive(Clone, Debug)]
pub struct HelpSourceFile {
    pub rel_path: String,
    pub raw_content: String,
}

/// v4 `HelpDocSyncResult`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HelpDocSyncResult {
    pub total_on_disk: usize,
    pub created: usize,
    pub updated: usize,
    pub unchanged: usize,
    /// Docs deleted (a row in the database whose file is gone from disk) — v4
    /// `551f090b`'s prune counter.
    pub deleted: usize,
    pub failed: usize,
    /// Section chunk rows written across every created/updated doc (v4
    /// `24633026`'s `chunksWritten`).
    pub chunks_written: usize,
    /// Ids of docs created or updated (need re-embedding), in walk order.
    pub changed_ids: Vec<String>,
}

/// Split `s` into JS-multiline "lines" — boundaries at `\r\n` (as one), `\n`,
/// `\r`, U+2028, U+2029 (the JS `/m` LineTerminator set).
fn js_lines(s: &str) -> Vec<&str> {
    let mut lines = Vec::new();
    let mut start = 0;
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < s.len() {
        let c = s[i..].chars().next().unwrap();
        match c {
            '\r' => {
                lines.push(&s[start..i]);
                // \r\n counts as one boundary.
                if i + 1 < s.len() && bytes[i + 1] == b'\n' {
                    i += 2;
                } else {
                    i += 1;
                }
                start = i;
            }
            '\n' | '\u{2028}' | '\u{2029}' => {
                lines.push(&s[start..i]);
                i += c.len_utf8();
                start = i;
            }
            _ => i += c.len_utf8(),
        }
    }
    lines.push(&s[start..]);
    lines
}

/// v4's local `parseFrontmatter` (`help-doc-sync.ts:68`) — the loose
/// `/^---\r?\n([\s\S]*?)\r?\n---\r?\n?/` opener/closer scan + the first
/// `/^url:\s*(.+)$/m` line. Returns `(url, body)`; no frontmatter →
/// `("", content)`.
fn parse_frontmatter(content: &str) -> (String, String) {
    use std::sync::LazyLock;
    // `[\s\S]` = any char in both JS and Rust regex; the lazy group + the
    // optional trailing `\r?\n?` reproduce v4's quirks (a close fence NOT
    // followed by a newline still matches, leaving the rest as the body).
    static FM: LazyLock<regex::Regex> = LazyLock::new(|| {
        regex::Regex::new(r"^---\r?\n([\s\S]*?)\r?\n---\r?\n?").expect("frontmatter regex")
    });
    let Some(m) = FM.captures(content) else {
        return (String::new(), content.to_string());
    };
    let frontmatter = m.get(1).map(|g| g.as_str()).unwrap_or("");
    let body = &content[m.get(0).unwrap().end()..];

    // `/^url:\s*(.+)$/m` — the first line starting `url:` whose remainder is
    // non-empty (the `\s*(.+)` backtrack makes an all-whitespace remainder
    // still match, trimming to ""), then `.trim()`.
    let mut url = String::new();
    for line in js_lines(frontmatter) {
        if let Some(rest) = line.strip_prefix("url:") {
            if !rest.is_empty() {
                url = js_trim(rest).to_string();
                break;
            }
        }
    }
    (url, body.to_string())
}

/// v4 `extractTitle` (`help-doc-sync.ts:84`) — the first `/^#\s+(.+)$/m` H1 in
/// the BODY (trimmed), else the filename title-cased (`split('-')`,
/// first-unit uppercase per word).
fn extract_title(body: &str, rel_path: &str) -> String {
    for line in js_lines(body) {
        if let Some(rest) = line.strip_prefix('#') {
            // `#\s+` needs ≥1 JS-whitespace char, then `(.+)` ≥1 char (the
            // backtrack semantics collapse to: remainder starts with JS-ws and
            // has ≥2 chars total; the capture trims to js_trim(remainder)).
            let mut chars = rest.chars();
            if let Some(first) = chars.next() {
                if is_js_ws(first) && chars.next().is_some() {
                    return js_trim(rest).to_string();
                }
            }
        }
    }

    // Fallback: filename without the FIRST '.md' occurrence, '-'-split,
    // per-word first-char uppercase (JS `charAt(0).toUpperCase() + slice(1)`;
    // ASCII-faithful — help filenames are ASCII; a non-BMP first char is a
    // documented seam, v4 leaves such a word unchanged via lone surrogates).
    let filename = rel_path.rsplit('/').next().unwrap_or("");
    let filename = if filename.is_empty() {
        "Unknown"
    } else {
        filename
    };
    let without_md = match filename.find(".md") {
        Some(i) => format!("{}{}", &filename[..i], &filename[i + 3..]),
        None => filename.to_string(),
    };
    without_md
        .split('-')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(c) => format!("{}{}", c.to_uppercase(), chars.as_str()),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Lowercase-hex SHA-256 of the (trimmed) content (v4 `hashContent`).
fn hash_content(content: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// v4 `clearAllEmbeddingsForDoc` — `_update(id, { embedding: null })`, which
/// NULLs the BLOB and mints `updatedAt` (the base `_update` clock).
fn clear_embedding(main: &Connection, id: &str) -> Result<(), DbError> {
    main.execute(
        "UPDATE help_docs SET embedding = NULL, updatedAt = ?1 WHERE id = ?2",
        params![crate::clock::now_iso(), id],
    )?;
    Ok(())
}

/// v4 `syncHelpDocs` (`help-doc-sync.ts:133`) over an already-walked file list
/// (see the module docs for the host-walk decision). Conn-level: the upserts
/// run on the caller's (writer-held) main connection. Per-file failures are
/// swallowed into `failed` (v4's catch).
///
/// **Enqueues nothing, deliberately** — v4's rationale, carried forward: the two
/// callers want different things. `EMBEDDING_REINDEX_ALL` re-embeds every doc
/// regardless of what changed and batch-inserts its jobs, so enqueueing here
/// would RACE it; [`ensure_help_docs_synced`] instead tops up only the docs that
/// still lack an embedding. Per-entity dedup in `enqueue_embedding_generate`
/// covers the overlap.
pub fn sync_help_docs(main: &Connection, files: &[HelpSourceFile]) -> HelpDocSyncResult {
    let mut result = HelpDocSyncResult {
        total_on_disk: files.len(),
        ..Default::default()
    };
    if files.is_empty() {
        return result;
    }

    let repo = HelpDocsRepository::new(main);

    // One read of the table, indexed by path (v4 `551f090b`). The prune below
    // needs every row anyway, and it doubles as the per-file lookup — the
    // alternative is a findByPath per file, which is ~115 queries on every sync.
    let existing_docs = match repo.find_all() {
        Ok(rows) => rows,
        Err(_) => {
            // v4's findAll is INSIDE syncHelpDocs but OUTSIDE its per-file
            // try/catch, so a failure there propagates rather than counting a
            // `failed`. Nothing is walked, nothing is pruned.
            return result;
        }
    };
    let existing_by_path: std::collections::HashMap<&str, &crate::db::help_docs::HelpDocRow> =
        existing_docs
            .iter()
            .map(|doc| (doc.path.as_str(), doc))
            .collect();

    // The paths the walk actually produced CONTENT for. v4 adds a path here only
    // after the empty-content `continue`, so a whitespace-only file contributes
    // NOTHING — see the prune's note below for why that matters.
    let mut paths_on_disk: std::collections::HashSet<&str> = std::collections::HashSet::new();

    for file in files {
        let outcome = (|| -> Result<(), DbError> {
            let raw = js_trim(&file.raw_content);
            if raw.is_empty() {
                // v4 `continue` — counted in totalOnDisk only.
                return Ok(());
            }
            paths_on_disk.insert(file.rel_path.as_str());

            let content_hash = hash_content(raw);
            let (url, body) = parse_frontmatter(raw);
            let title = extract_title(&body, &file.rel_path);

            let existing = existing_by_path.get(file.rel_path.as_str());
            if let Some(doc) = existing {
                if doc.content_hash == content_hash {
                    result.unchanged += 1;
                    return Ok(());
                }
            }

            let doc_id = repo.upsert_by_path(&HdUpsert {
                title,
                path: file.rel_path.clone(),
                url,
                content: body.clone(),
                content_hash,
            })?;

            // Re-slice the doc into section chunks (v4 `24633026`). v4's *why*,
            // carried forward: boundaries move whenever the prose above them
            // changes, so the old rows are discarded wholesale rather than
            // diffed; their embeddings are filled by the HELP_DOC embedding job
            // that the caller enqueues for this doc.
            let chunks = build_help_doc_chunks(&body);
            HelpDocChunksRepository::new(main).replace_for_doc(&doc_id, &chunks)?;
            result.chunks_written += chunks.len();

            if existing.is_some() {
                clear_embedding(main, &doc_id)?;
                result.updated += 1;
            } else {
                result.created += 1;
            }
            result.changed_ids.push(doc_id);
            Ok(())
        })();
        if outcome.is_err() {
            result.failed += 1;
        }
    }

    // Prune rows whose file is gone from disk (v4 `551f090b` +, since bug 18
    // (`13ddc5ee`), the widened blank-content guard, `help-doc-sync.ts:218-266`).
    //
    // TWO guards protect the table from a wipe:
    //   - The `files.is_empty()` early return above (a missing help/ — the host
    //     walker yields an empty list, v4's `existsSync` guard).
    //   - This `paths_on_disk.is_empty() && !existing_docs.is_empty()` refusal:
    //     `paths_on_disk` holds exactly the files that survived the empty-content
    //     `continue`, so a help/ whose only Markdown is whitespace-only walks
    //     NON-empty (past the first guard) yet produces no usable content — and
    //     the prune below would then delete every row (v4 measured `totalOnDisk
    //     1, deleted 3, rows left 0`). An all-blank help set against a populated
    //     table is suspicious (an interrupted checkout, a half-written file), not
    //     an instruction to wipe the Guide. Skip and leave the rows; the next
    //     healthy sync reconciles them.
    if paths_on_disk.is_empty() && !existing_docs.is_empty() {
        // Log output is non-contractual; `eprintln!` is this module's convention.
        eprintln!(
            "[HelpDocSync] No help docs on disk have usable content but the table is \
             populated ({} rows) — skipping the destructive prune",
            existing_docs.len()
        );
    } else {
        for doc in &existing_docs {
            if paths_on_disk.contains(doc.path.as_str()) {
                continue;
            }
            // v4 wraps the pair in its own try/catch that counts `failed`, so a
            // prune failure is NOT the per-file counter above.
            let pruned = (|| -> Result<(), DbError> {
                // v4 `24633026` — the chunks go first, so the rows never
                // outlive their parent even where the FK cascade is not
                // enforced (a fresh-provisioned instance's table carries no
                // foreign key at all; see `db::help_doc_chunks_repair`).
                HelpDocChunksRepository::new(main).delete_by_doc_id(&doc.id)?;
                repo.delete(&doc.id)?;
                EmbeddingStatusRepository::new(main).delete_by_entity("HELP_DOC", &doc.id)?;
                Ok(())
            })();
            match pruned {
                Ok(()) => result.deleted += 1,
                Err(_) => result.failed += 1,
            }
        }
    }

    result
}

/// v4 `helpDocsDivergeFromDisk` (`help-doc-sync.ts:297`) — whether the help
/// documents on disk and the rows in the database have parted ways in EITHER
/// direction: a file with no row, or a row whose file is gone.
///
/// **Both directions come out of the same directory listing, and the deleted
/// direction is load-bearing** (v4's *why*, carried forward): both need the same
/// fix — `sync_help_docs` creates the missing rows and prunes the stale ones —
/// and ignoring the deleted direction would leave the prune UNREACHABLE, since a
/// deletion alone would never trigger a sync.
///
/// `paths_on_disk` is the raw walk listing, NOT the sync's `pathsOnDisk` (which
/// drops empty-content files). That is v4's shape and it has a consequence worth
/// knowing: an empty `.md` file is forever "unsynced" (it never gets a row), so
/// its presence makes this return `true` on every call — the sync then runs and
/// no-ops over it. Reproduced deliberately; see [`sync_help_docs`]'s prune note.
pub fn help_docs_diverge_from_disk(existing_paths: &[&str], paths_on_disk: &[&str]) -> bool {
    let synced: std::collections::HashSet<&str> = existing_paths.iter().copied().collect();
    let on_disk: std::collections::HashSet<&str> = paths_on_disk.iter().copied().collect();

    let unsynced = paths_on_disk.iter().any(|p| !synced.contains(p));
    let deleted = existing_paths.iter().any(|p| !on_disk.contains(p));
    unsynced || deleted
}

/// v4 `ensureHelpDocsSynced` (`help-doc-sync.ts:269`) — sync when `help_docs` is
/// empty, **and when the set of Markdown files on disk no longer matches the set
/// of rows**. Returns `Some(result)` when a sync ran.
///
/// The divergence gate is v4 `6c59b1ca`'s fix for **bug 1**: the old gate was
/// `if (existing.length > 0) return`, and this is the only sync trigger outside a
/// full embedding reindex — so a doc added to `help/` after the first sync never
/// got a row. Eleven shipped docs were invisible in the Guide.
///
/// Detecting divergence costs a directory scan, not a read of every file, and
/// `sync_help_docs` skips unchanged docs by content hash. Edits to an
/// already-synced doc are still picked up only by the next `sync_help_docs`
/// call — a file's content is never examined here. (The module-promise
/// concurrency guard does not port — the single-writer runtime serializes
/// callers.)
///
/// **v5 seam:** v4 re-walks the disk itself here; the core is fs-free, so the
/// host's walk (`quilltap-host::files_store::load_help_source_files`) supplies
/// `files`, whose `rel_path`s ARE v4's `listHelpDocPathsOnDisk()` output — the
/// walker yields every `.md` it finds, empty-content ones included. No seam
/// extension was needed for the deletion direction. The cost difference (v4's
/// trigger reads no file CONTENTS; the host has already read them by the time it
/// calls) is a host-side concern for the startup wiring, which is a standing
/// deferral — see the module header.
pub async fn ensure_help_docs_synced(
    db: &Db,
    files: &[HelpSourceFile],
) -> Result<Option<HelpDocSyncResult>, DbError> {
    let existing = db.read_main(|c| HelpDocsRepository::new(c).find_all())?;

    let existing_paths: Vec<&str> = existing.iter().map(|d| d.path.as_str()).collect();
    let paths_on_disk: Vec<&str> = files.iter().map(|f| f.rel_path.as_str()).collect();
    if !existing.is_empty() && !help_docs_diverge_from_disk(&existing_paths, &paths_on_disk) {
        // Docs are current, but their section chunks may not exist at all — an
        // instance that upgraded into `help_doc_chunks` has every content hash
        // matching, so nothing above would ever slice them (v4 `24633026`).
        backfill_help_doc_chunks(db, &existing).await;
        return Ok(None);
    }

    let files_owned = files.to_vec();
    let result = db
        .write(move |ws| Ok(sync_help_docs(ws.main().connection(), &files_owned)))
        .await?;

    enqueue_missing_help_doc_embeddings(db).await;
    Ok(Some(result))
}

/// v4 `backfillHelpDocChunks` (`help-doc-sync.ts:327`, new at `24633026`) —
/// slice any already-synced document that has no section chunks, and enqueue an
/// embedding job for it.
///
/// **The path that matters is the upgrade** (v4's *why*, carried forward): an
/// existing instance has a full, unchanged `help_docs` table, so the
/// content-hash check skips every file and the chunks would stay empty forever
/// — section search would silently never engage. Docs whose chunks already
/// exist are left alone, so this costs one count query per boot once it has run.
///
/// The embedding job is enqueued **even though the document's own embedding is
/// present**, because the job is what fills the chunk vectors.
///
/// Whole thing best-effort: a failure warns and never blocks help from loading,
/// since whole-document search still works. That is why this returns `()`.
async fn backfill_help_doc_chunks(db: &Db, existing: &[crate::db::help_docs::HelpDocRow]) {
    let outcome: Result<(), DbError> = async {
        // One count, not a scan (v4's *why*): chunk rows carry embedding BLOBs,
        // and reading them all on every boot to answer "has this run yet?" would
        // be absurd. A non-empty table means the backfill has already happened;
        // docs added afterwards are sliced by `sync_help_docs` on their content
        // hash, and a half-finished backfill is healed by the next full reindex.
        if db.read_main(|c| HelpDocChunksRepository::new(c).count())? > 0 {
            return Ok(());
        }

        // v4 names the whole list `missing` and takes it wholesale — the count
        // above is the only gate, so this is every existing doc.
        let missing = existing;
        if missing.is_empty() {
            return Ok(());
        }

        let docs: Vec<(String, String)> = missing
            .iter()
            .map(|d| (d.id.clone(), d.content.clone()))
            .collect();
        db.write(move |ws| {
            let repo = HelpDocChunksRepository::new(ws.main().connection());
            for (doc_id, content) in &docs {
                let chunks = build_help_doc_chunks(content);
                // v4 skips a doc that slices to nothing rather than calling
                // replaceForDoc with an empty list (which would still delete).
                if chunks.is_empty() {
                    continue;
                }
                repo.replace_for_doc(doc_id, &chunks)?;
            }
            Ok(())
        })
        .await?;

        let doc_ids: Vec<String> = missing.iter().map(|d| d.id.clone()).collect();
        enqueue_help_doc_embeddings(db, &doc_ids).await;
        Ok(())
    }
    .await;

    if let Err(e) = outcome {
        // v4 `logger.warn`; log output is non-contractual (P4.18) and
        // `eprintln!` is this module's convention.
        eprintln!("[HelpDocSync] Help doc section backfill failed: {e}");
    }
}

/// v4 `enqueueMissingHelpDocEmbeddings` (`help-doc-sync.ts:327`) — enqueue
/// embedding jobs for help docs that have no embedding: newly synced docs, docs
/// whose content changed (the sync clears their stale embedding), and any left
/// unembedded by an earlier failure. Without this a new doc lands in the Guide
/// but stays invisible to `help_search` until a full reindex.
///
/// Per-entity dedup in `enqueue_embedding_generate` keeps this from duplicating
/// jobs an `EMBEDDING_REINDEX_ALL` has already queued.
///
/// **Best-effort, deliberately** (v4's *why*, carried forward): every failure is
/// swallowed, because the docs are already in the database and listable in the
/// Guide, which is the caller's actual dependency. That is why this returns `()`
/// and not a `Result`.
async fn enqueue_missing_help_doc_embeddings(db: &Db) {
    // v4 wraps the entire body in one try/catch — a failure at ANY step (not
    // just the enqueue) lands in the same catch, so the whole body is one
    // fallible unit here too.
    let outcome: Result<(), DbError> = async {
        let need_embedding =
            db.read_main(|c| HelpDocsRepository::new(c).find_all_needing_embedding())?;
        let doc_ids: Vec<String> = need_embedding.iter().map(|d| d.id.clone()).collect();
        enqueue_help_doc_embeddings(db, &doc_ids).await;
        Ok(())
    }
    .await;

    if let Err(e) = outcome {
        // v4 logs and moves on (`logger.error` in the catch). The core has no
        // logging crate; `eprintln!` is the module convention here (as in
        // `mount_index::embedding_scheduler`).
        eprintln!("[HelpDocSync] Failed to look up help docs needing embedding: {e}");
    }
}

/// v4 `enqueueHelpDocEmbeddings` (`help-doc-sync.ts:437`, extracted at
/// `24633026` so the chunk backfill can share it) — enqueue a HELP_DOC
/// embedding job for each of `doc_ids`, resolving the default embedding profile
/// and the single user.
///
/// Silent when either is unavailable: an instance with no embedding profile
/// configured simply has no semantic help search yet, which is not an error
/// worth shouting about on every boot.
///
/// Per-entity dedup in `enqueue_embedding_generate` keeps this from duplicating
/// jobs an `EMBEDDING_REINDEX_ALL` has already queued.
async fn enqueue_help_doc_embeddings(db: &Db, doc_ids: &[String]) {
    let outcome: Result<(), DbError> = async {
        if doc_ids.is_empty() {
            return Ok(());
        }

        // v4: `profiles.findAll()` then `find(p => p.isDefault) || profiles[0]`
        // — unscoped, no ORDER BY, and the FALLBACK SURVIVES here: v4's
        // `d553f72a` one-default sweep did not touch help-doc sync, so this is
        // the one site that still embeds under the first row when no default
        // is marked (`default_or_first_profile_id`, not `default_profile_id`).
        let Some(profile_id) = db.read_main(default_or_first_profile_id)? else {
            // v4 debug-logs "need embedding but no profile is configured" and
            // returns — the sync itself still counts as done.
            return Ok(());
        };
        // v4: `users.findAll()[0]?.id`.
        let Some(user_id) = db.read_main(first_user_id)? else {
            return Ok(());
        };

        for doc_id in doc_ids {
            enqueue_embedding_generate(
                db,
                &user_id,
                serde_json::json!({
                    "entityType": "HELP_DOC",
                    "entityId": doc_id,
                    "profileId": profile_id,
                }),
            )
            .await?;
        }
        Ok(())
    }
    .await;

    if let Err(e) = outcome {
        // v4 logs and moves on (`logger.error` in the catch). The core has no
        // logging crate; `eprintln!` is the module convention here (as in
        // `mount_index::embedding_scheduler`).
        eprintln!("[HelpDocSync] Failed to enqueue help doc embeddings: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frontmatter_basic() {
        let (url, body) = parse_frontmatter("---\nurl: /help/x\n---\nBody here");
        assert_eq!(url, "/help/x");
        assert_eq!(body, "Body here");
    }

    #[test]
    fn frontmatter_crlf() {
        let (url, body) = parse_frontmatter("---\r\nurl: /a\r\n---\r\nB");
        assert_eq!(url, "/a");
        assert_eq!(body, "B");
    }

    #[test]
    fn frontmatter_absent_or_unclosed() {
        let (url, body) = parse_frontmatter("# Title\ntext");
        assert_eq!(url, "");
        assert_eq!(body, "# Title\ntext");
        // Unclosed fence → no match, whole content is the body.
        let (url, body) = parse_frontmatter("---\nurl: /a\nno close");
        assert_eq!(url, "");
        assert_eq!(body, "---\nurl: /a\nno close");
    }

    #[test]
    fn frontmatter_close_without_trailing_newline() {
        // v4's optional `\r?\n?` after the close fence: "---\nurl: /a\n---"
        // (EOF right after) matches with an empty body.
        let (url, body) = parse_frontmatter("---\nurl: /a\n---");
        assert_eq!(url, "/a");
        assert_eq!(body, "");
        // And the quirk: a close fence with trailing garbage on the SAME line
        // still closes, the garbage becoming the body ("---MORE").
        let (url, body) = parse_frontmatter("---\nurl: /a\n---MORE");
        assert_eq!(url, "/a");
        assert_eq!(body, "MORE");
    }

    #[test]
    fn url_line_edge_cases() {
        // `url:` alone (nothing after the colon) never matches; a later line can.
        let (url, _) = parse_frontmatter("---\nurl:\nurl: real\n---\nB");
        assert_eq!(url, "real");
        // Whitespace-only remainder matches and trims to "".
        let (url, _) = parse_frontmatter("---\nurl:   \nurl: later\n---\nB");
        assert_eq!(url, "");
    }

    #[test]
    fn title_extraction() {
        assert_eq!(extract_title("# My Title  \nrest", "help/x.md"), "My Title");
        // '#x' (no whitespace) is not an H1.
        assert_eq!(extract_title("#x\n", "help/some-doc.md"), "Some Doc");
        // Fallback casing: existing caps preserved (charAt(0).toUpperCase()).
        assert_eq!(
            extract_title("", "help/weird-CASE-Name.md"),
            "Weird CASE Name"
        );
        // `.replace('.md','')` removes the FIRST occurrence.
        assert_eq!(extract_title("", "help/a.mdx.md"), "Ax.md");
    }

    #[test]
    fn hash_is_sha256_hex() {
        assert_eq!(
            hash_content("abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
