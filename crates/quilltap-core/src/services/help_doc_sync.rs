//! The help-docs disk sync and startup reconcile (v4 `lib/help/help-doc-sync.ts`)
//! — read the help Markdown tree into `help_docs`, re-slice changed docs into
//! `help_doc_chunks`, prune rows whose file is gone, and (at every boot) queue
//! the embedding work that leaves the index complete.
//!
//! **Design decision (documented per the P4.1b work order):** the DIRECTORY
//! WALK is host-side — the core sync takes the already-read file list
//! ([`HelpSourceFile`], in walk order) so the engine stays fs-free. The host
//! walker (`quilltap-host::files_store::load_help_source_files`) reproduces v4
//! `findMarkdownFiles`' inline-recursive `readdirSync` order (files and
//! directories interleaved in raw readdir order — both Node and Rust issue the
//! same syscall over the same directory, so the order matches for a shared
//! fixture tree). The order only affects `changed_ids` sequencing; the
//! differential compares path-keyed forms. Two v4 log lines therefore live on
//! the host side of the seam and are not emitted here: `Error reading
//! directory` (the walk) and `Help directory not found` (production reads the
//! EMBEDDED tree, which cannot be missing; an empty list takes v4's `No
//! Markdown files found` INFO).
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
//! ## Section chunks (P4.D77, v4 `24633026`)
//!
//! Each created/updated doc is re-sliced wholesale into `help_doc_chunks`
//! ([`crate::services::help_doc_chunking`]) and a pruned doc's chunks are
//! deleted explicitly. A doc that has NO section rows at all (an instance that
//! upgraded into the table, or one whose reindex was rolled back by v4 bug 167)
//! is sliced by the reconcile below.
//!
//! ## The startup reconcile (P4.D222, v4 `492771aff`, bugs 167 + 168)
//!
//! [`reconcile_help_docs`] replaced v4's old gate ("sync only when the table is
//! empty or the set of FILE NAMES diverges"), the `count() > 0` chunk backfill
//! and the enqueue-missing pass. It runs the full sync (every file re-read and
//! hashed — an edited page is picked up at the next boot, where before it
//! stayed stale until a full reindex), slices every section-less doc, and
//! queues a HELP_DOC embedding job for every doc missing its own vector or any
//! section's. The job reuses stored section vectors, so only what is missing
//! costs a provider call.
//!
//! **Wired at boot** (`quilltap-host`'s assembly, v4's Phase 3.66 — awaited,
//! and BEFORE the embedding-dimension reconcile, Phase 3.7). v4 memoizes the
//! run ONCE per process ([`HelpDocReconcileGate`]): the old "the promise guard
//! does not port — the single-writer runtime serializes callers" claim no
//! longer holds, because the memo is now what makes a second caller NOT re-run
//! a successful reconcile, and what makes a caller after a FAILED one retry.
//! The gate is an owned value the host constructs, never a `static` here.
//! v4's other caller, `HelpSearch.loadFromDatabase`'s lazy ensure, has no v5
//! counterpart (help reads run over the table the boot already reconciled —
//! the P4.9I2A eager-boot divergence), so the host's boot is the gate's one
//! production caller.

use rusqlite::{params, Connection};

use crate::db::embedding_status::EmbeddingStatusRepository;
use crate::db::help_doc_chunks::{HelpDocChunksRepository, SectionCounts};
use crate::db::help_docs::{CreateOptions, HdCreate, HdUpdate, HelpDocsRepository};
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

/// v4 `syncHelpDocs` (`help-doc-sync.ts:124-297`) over an already-walked file
/// list (see the module docs for the host-walk decision). Conn-level: the
/// writes run on the caller's (writer-held) main connection. Per-file failures
/// are swallowed into `failed` (v4's catch); the ONE read outside that catch —
/// `findAll` — is v4's FALLBACK `safeQuery` ([`find_all_or_empty`]): a failing
/// read logs ONE ERROR and the sync carries on over an EMPTY table (every file
/// is "new", nothing is pruned — the prune guard sees zero rows). It never
/// fails the sync. (P4.D222 first landed it as a propagating `Err`, "as v4's
/// throw does" — v4 never throws there; fixed at the `b0b6656b5` unification.)
///
/// **Enqueues nothing, deliberately** — v4's rationale, carried forward: the two
/// callers want different things. `EMBEDDING_REINDEX_ALL` re-embeds every doc
/// regardless of what changed, while [`reconcile_help_docs`] queues only the
/// docs left incomplete.
/// v4 `repos.helpDocs.findAll()` — `AbstractBaseRepository._findAll`
/// (`base.repository.ts:263-278`), a FALLBACK `safeQuery`: a DB failure logs
/// ONE ERROR `Error finding all entities` with the base class's enriched
/// context (`collection: 'help_docs'`) plus the error, and answers `[]`. Both
/// help-sync reads of the table go through it (the sync's index and the
/// reconcile's re-read), so neither ever fails on the read.
fn find_all_or_empty(repo: &HelpDocsRepository<'_>) -> Vec<crate::db::help_docs::HelpDocRow> {
    match repo.find_all() {
        Ok(rows) => rows,
        Err(e) => {
            log_find_all_fallback(&e);
            Vec::new()
        }
    }
}

/// The reconcile's projection of the same fallback read (see
/// [`find_all_or_empty`]).
fn find_all_for_reconcile_or_empty(
    repo: &HelpDocsRepository<'_>,
) -> Vec<crate::db::help_docs::HelpDocReconcileRow> {
    match repo.find_all_for_reconcile() {
        Ok(rows) => rows,
        Err(e) => {
            log_find_all_fallback(&e);
            Vec::new()
        }
    }
}

fn log_find_all_fallback(e: &DbError) {
    tracing::error!(
        target: "quilltap::db",
        collection = "help_docs",
        error = %e,
        "Error finding all entities",
    );
}

pub fn sync_help_docs(
    main: &Connection,
    files: &[HelpSourceFile],
) -> Result<HelpDocSyncResult, DbError> {
    let mut result = HelpDocSyncResult {
        total_on_disk: files.len(),
        ..Default::default()
    };
    if files.is_empty() {
        tracing::info!(
            target: "quilltap::help",
            context = "syncHelpDocs",
            "[HelpDocSync] No Markdown files found in help directory",
        );
        return Ok(result);
    }

    let repo = HelpDocsRepository::new(main);

    // One read of the table, indexed by path (v4 `551f090b`). The prune below
    // needs every row anyway, and it doubles as the per-file lookup — the
    // alternative is a findByPath per file, which is ~115 queries on every sync.
    // Outside the per-file catch in v4, but a FALLBACK read: a failure logs and
    // answers `[]` (see `find_all_or_empty`).
    let existing_docs = find_all_or_empty(&repo);
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

            let existing = existing_by_path.get(file.rel_path.as_str()).copied();
            if let Some(doc) = existing {
                if doc.content_hash == content_hash {
                    result.unchanged += 1;
                    return Ok(());
                }
            }

            // v4's *why* (bug 167), carried forward: the chunk rows below are
            // keyed to this id, so it must be the id the row really has — never
            // one handed back by a write. Inside v4's job child (the reindex)
            // writes are buffered and return a synthetic result: `upsertByPath`
            // came back with a random UUID, the parent's replay updated the real
            // row, and every chunk insert failed its foreign key, rolling back
            // the whole reindex batch. An existing row's id is already in hand;
            // a new row's id is minted here and passed to `create`.
            let now = crate::clock::now_iso();
            let doc_id = match existing {
                Some(doc) => {
                    // Preserves the embedding column — cleared separately below.
                    repo.update(
                        &doc.id,
                        &HdUpdate {
                            title: Some(title),
                            path: Some(file.rel_path.clone()),
                            url: Some(url),
                            content: Some(body.clone()),
                            content_hash: Some(content_hash),
                            updated_at: now,
                        },
                    )?;
                    doc.id.clone()
                }
                None => {
                    let id = uuid::Uuid::new_v4().to_string();
                    repo.create(
                        &HdCreate {
                            title,
                            path: file.rel_path.clone(),
                            url,
                            content: body.clone(),
                            content_hash,
                            embedding: None,
                        },
                        &CreateOptions {
                            id: id.clone(),
                            created_at: now.clone(),
                            updated_at: now,
                        },
                    )?;
                    id
                }
            };

            // Re-slice the doc into section chunks (v4 `24633026`). v4's *why*,
            // carried forward: boundaries move whenever the prose above them
            // changes, so the old rows are discarded wholesale rather than
            // diffed; their embeddings are filled by the HELP_DOC embedding job.
            let chunks = build_help_doc_chunks(&body);
            HelpDocChunksRepository::new(main).replace_for_doc(&doc_id, &chunks)?;
            result.chunks_written += chunks.len();

            // Content changed — clear the old embedding so it gets re-generated.
            if existing.is_some() {
                clear_embedding(main, &doc_id)?;
                // v4's *why* (bug 168): a FAILED status belongs to the old text.
                // Left in place it would keep the new text out of a partial
                // reindex, which skips failed entities — a page that once
                // overflowed the provider stayed unembedded after it was fixed.
                EmbeddingStatusRepository::new(main).delete_by_entity("HELP_DOC", &doc_id)?;
                result.updated += 1;
            } else {
                result.created += 1;
            }

            tracing::debug!(
                target: "quilltap::help",
                context = "syncHelpDocs",
                path = file.rel_path.as_str(),
                docId = doc_id.as_str(),
                action = if existing.is_some() { "updated" } else { "created" },
                chunks = chunks.len(),
                "[HelpDocSync] Synced help doc",
            );

            result.changed_ids.push(doc_id);
            Ok(())
        })();
        if let Err(e) = outcome {
            result.failed += 1;
            // v4 logs the ABSOLUTE `filePath` here; the fs-free core only has
            // the relative one (the host-walk seam).
            tracing::error!(
                target: "quilltap::help",
                context = "syncHelpDocs",
                filePath = file.rel_path.as_str(),
                error = %e,
                "[HelpDocSync] Failed to sync file",
            );
        }
    }

    // Prune rows whose file is gone from disk (v4 `551f090b` +, since bug 18
    // (`13ddc5ee`), the widened blank-content guard).
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
        tracing::warn!(
            target: "quilltap::help",
            context = "syncHelpDocs",
            totalOnDisk = result.total_on_disk,
            existingRows = existing_docs.len(),
            "[HelpDocSync] No help docs on disk have usable content but the table is populated — skipping the destructive prune",
        );
    } else {
        for doc in &existing_docs {
            if paths_on_disk.contains(doc.path.as_str()) {
                continue;
            }
            // v4 wraps the trio in its own try/catch that counts `failed`, so a
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
                Err(e) => {
                    result.failed += 1;
                    tracing::error!(
                        target: "quilltap::help",
                        context = "syncHelpDocs",
                        docId = doc.id.as_str(),
                        path = doc.path.as_str(),
                        error = %e,
                        "[HelpDocSync] Failed to prune deleted help doc",
                    );
                }
            }
        }
    }

    tracing::info!(
        target: "quilltap::help",
        context = "syncHelpDocs",
        totalOnDisk = result.total_on_disk,
        created = result.created,
        updated = result.updated,
        unchanged = result.unchanged,
        deleted = result.deleted,
        failed = result.failed,
        chunksWritten = result.chunks_written,
        changedIds = result.changed_ids.len(),
        "[HelpDocSync] Sync completed",
    );

    Ok(result)
}

/// v4 `HelpDocReconcileResult` — the sync's tallies plus what the reconcile
/// found incomplete and queued for embedding.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HelpDocReconcileResult {
    pub sync: HelpDocSyncResult,
    /// Docs with no section rows, sliced by this pass.
    pub sections_backfilled: usize,
    /// Docs missing their own vector or any section vector.
    pub incomplete: usize,
}

/// v4 `reconcileHelpDocs` (`help-doc-sync.ts:335-377`, new at `492771aff`) —
/// bring the help index in line with the help files on disk, and queue the
/// embedding work that leaves it complete.
///
/// v4's *why*, carried forward: the steps are cheap when nothing changed —
/// every file is read and hashed, and the index is checked with one row read of
/// `help_docs` and one GROUP BY over `help_doc_chunks` — so a full content
/// comparison is affordable on every boot. Before this, only a change in the
/// *set* of file names triggered a sync, and an edited page stayed stale until
/// a full reindex.
///
/// 1. [`sync_help_docs`]: new files are created, edited files are rewritten and
///    re-sliced with their vectors and failure status cleared, and rows whose
///    file is gone are pruned.
/// 2. Any doc with no section rows is sliced now (an instance whose reindex was
///    rolled back by bug 167 has an empty section table).
/// 3. A HELP_DOC embedding job is queued for every doc that lacks its own
///    vector or has any section without one.
///
/// Callers go through [`HelpDocReconcileGate::ensure`], which runs this once
/// per boot and never propagates its `Err`.
pub async fn reconcile_help_docs(
    db: &Db,
    files: &[HelpSourceFile],
) -> Result<HelpDocReconcileResult, DbError> {
    let files_owned = files.to_vec();
    let sync = db
        .write(move |ws| sync_help_docs(ws.main().connection(), &files_owned))
        .await?;

    // Re-read AFTER the sync (v4 reads `findAll()` again here — the same
    // FALLBACK read: a failure logs and the reconcile carries on over `[]`,
    // finding nothing incomplete).
    let docs =
        db.read_main(|c| Ok(find_all_for_reconcile_or_empty(&HelpDocsRepository::new(c))))?;
    let section_counts = db.read_main(|c| Ok(HelpDocChunksRepository::new(c).count_by_doc()))?;

    let mut sections_backfilled = 0usize;
    let mut incomplete_ids: Vec<String> = Vec::new();

    for doc in &docs {
        let mut counts = section_counts.get(&doc.id).copied();

        if counts.is_none() {
            let chunks = build_help_doc_chunks(&doc.content);
            // A doc that slices to nothing gets no rows, and is judged on its
            // own vector alone below.
            if !chunks.is_empty() {
                // One writer round-trip per doc, as v4 awaits `replaceForDoc`
                // per doc: a failure part-way keeps the docs already sliced and
                // fails the reconcile (its gate logs; the next caller resumes).
                let (id, drafts) = (doc.id.clone(), chunks.clone());
                db.write(move |ws| {
                    HelpDocChunksRepository::new(ws.main().connection())
                        .replace_for_doc(&id, &drafts)
                })
                .await?;
                sections_backfilled += 1;
                counts = Some(SectionCounts {
                    total: chunks.len() as i64,
                    embedded: 0,
                });
            }
        }

        let section_vector_missing = counts.is_some_and(|c| c.embedded < c.total);
        if doc.doc_vector_missing || section_vector_missing {
            incomplete_ids.push(doc.id.clone());
        }
    }

    tracing::info!(
        target: "quilltap::help",
        context = "reconcileHelpDocs",
        created = sync.created,
        updated = sync.updated,
        deleted = sync.deleted,
        unchanged = sync.unchanged,
        sectionsBackfilled = sections_backfilled,
        incomplete = incomplete_ids.len(),
        "[HelpDocSync] Help docs reconciled",
    );

    enqueue_help_doc_embeddings(db, &incomplete_ids).await;

    Ok(HelpDocReconcileResult {
        sync,
        sections_backfilled,
        incomplete: incomplete_ids.len(),
    })
}

/// v4's module-level `reconcilePromise` memo + `ensureHelpDocsSynced`
/// (`help-doc-sync.ts:379-406`) as an OWNED value: the host constructs one per
/// engine assembly and every caller goes through it.
///
/// v4's semantics, all three reproduced:
///   - a SUCCESSFUL reconcile runs once — later callers get its result without
///     re-running it;
///   - callers that arrive WHILE a run is in flight wait for that run and share
///     its outcome, a failure included (each logs its own WARN, as each v4
///     `await` lands in its own `catch`);
///   - a FAILED run is forgotten, so the next caller to arrive after it starts
///     a fresh one.
///
/// It never propagates an error: failure logs v4's WARN `Help doc reconcile
/// failed; serving help from the existing index` and answers `None` — help
/// still loads from whatever the table holds.
///
/// Mechanism: the run happens under an async mutex, so a concurrent caller
/// queues behind it. `failures` counts completed FAILED runs; a caller
/// snapshots it before queueing, and if it moved by the time the caller holds
/// the lock, a run that was in flight while it waited has failed — it takes
/// that failure instead of starting its own.
#[derive(Default)]
pub struct HelpDocReconcileGate {
    state: tokio::sync::Mutex<GateState>,
    failures: std::sync::atomic::AtomicU64,
}

#[derive(Default)]
struct GateState {
    done: Option<HelpDocReconcileResult>,
    last_error: String,
}

impl HelpDocReconcileGate {
    pub fn new() -> Self {
        Self::default()
    }

    /// v4 `ensureHelpDocsSynced` — wait for this boot's help reconcile,
    /// starting it if nothing has. `Some` with the reconcile's result, or `None`
    /// when it failed (logged).
    pub async fn ensure(
        &self,
        db: &Db,
        files: &[HelpSourceFile],
    ) -> Option<HelpDocReconcileResult> {
        use std::sync::atomic::Ordering;

        let ticket = self.failures.load(Ordering::SeqCst);
        let mut state = self.state.lock().await;
        if let Some(done) = &state.done {
            return Some(done.clone());
        }
        let error = if self.failures.load(Ordering::SeqCst) != ticket {
            // The run in flight while this caller waited failed: share it.
            state.last_error.clone()
        } else {
            match reconcile_help_docs(db, files).await {
                Ok(result) => {
                    state.done = Some(result.clone());
                    return Some(result);
                }
                Err(e) => {
                    state.last_error = e.to_string();
                    self.failures.fetch_add(1, Ordering::SeqCst);
                    state.last_error.clone()
                }
            }
        };
        drop(state);
        tracing::warn!(
            target: "quilltap::help",
            context = "ensureHelpDocsSynced",
            error = error.as_str(),
            "[HelpDocSync] Help doc reconcile failed; serving help from the existing index",
        );
        None
    }
}

/// v4 `enqueueHelpDocEmbeddings` (`help-doc-sync.ts:414-466`) — enqueue a
/// HELP_DOC embedding job for each of `doc_ids`, resolving the default
/// embedding profile and the single user.
///
/// Silent when either is unavailable (v4's *why*): an instance with no
/// embedding profile configured simply has no semantic help search yet, which
/// is not an error worth shouting about on every boot — hence DEBUG, not WARN.
///
/// Per-entity dedup in `enqueue_embedding_generate` keeps this from duplicating
/// jobs an `EMBEDDING_REINDEX_ALL` has already queued; `enqueued` counts only
/// the NEW jobs (v4's `isNew`).
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
            tracing::debug!(
                target: "quilltap::help",
                context = "enqueueHelpDocEmbeddings",
                needEmbedding = doc_ids.len(),
                "[HelpDocSync] Help docs need embedding but no embedding profile is configured",
            );
            return Ok(());
        };
        // v4: `users.findAll()[0]?.id` — silent when absent.
        let Some(user_id) = db.read_main(first_user_id)? else {
            return Ok(());
        };

        let mut enqueued = 0usize;
        for doc_id in doc_ids {
            let (_, is_new) = enqueue_embedding_generate(
                db,
                &user_id,
                serde_json::json!({
                    "entityType": "HELP_DOC",
                    "entityId": doc_id,
                    "profileId": profile_id,
                }),
            )
            .await?;
            if is_new {
                enqueued += 1;
            }
        }

        tracing::info!(
            target: "quilltap::help",
            context = "enqueueHelpDocEmbeddings",
            enqueued,
            needEmbedding = doc_ids.len(),
            "[HelpDocSync] Enqueued help doc embeddings",
        );
        Ok(())
    }
    .await;

    if let Err(e) = outcome {
        // v4's *why*: embedding top-up is best-effort — the docs are already in
        // the database and listable in the Guide, which is the caller's actual
        // dependency.
        tracing::error!(
            target: "quilltap::help",
            context = "enqueueHelpDocEmbeddings",
            error = %e,
            "[HelpDocSync] Failed to enqueue help doc embeddings",
        );
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

    // ---- P4.D222: the reconcile gate (v4's `ensureHelpDocsSynced` memo) ----

    const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
    const FAIL_TRIGGER: &str =
        "CREATE TRIGGER fail_section_insert BEFORE INSERT ON help_doc_chunks \
         BEGIN SELECT RAISE(ABORT, 'planted reconcile failure'); END";

    fn fresh_db(dir: &std::path::Path) -> Db {
        let data = dir.join("data");
        std::fs::create_dir_all(&data).unwrap();
        crate::services::provisioning::provision_fresh_instance(&data, PEPPER).unwrap();
        Db::open(
            crate::db::runtime::DbPaths {
                main: data.join("quilltap.db"),
                mount_index: None,
                llm_logs: None,
            },
            PEPPER,
        )
        .unwrap()
    }

    fn one_file() -> Vec<HelpSourceFile> {
        vec![HelpSourceFile {
            rel_path: "help/a.md".into(),
            raw_content: "# A\n\n## One\n\nbody".into(),
        }]
    }

    fn count_lines(lines: &[String], needle: &str) -> usize {
        lines.iter().filter(|l| l.contains(needle)).count()
    }

    /// Callers that arrive while a run is in flight SHARE its failure — one
    /// attempt, one WARN per caller (each v4 `await` lands in its own catch) —
    /// and a caller after it starts a FRESH run. The oracle's
    /// `fail-once-then-retry` arm proves the sequential half against v4; the
    /// concurrent half has no v4 arm, so it is pinned here.
    #[test]
    fn concurrent_callers_share_a_failed_run_and_the_next_caller_retries() {
        let dir = tempfile::tempdir().unwrap();
        let db = fresh_db(dir.path());
        let files = one_file();
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        // The sync's own slice insert is swallowed by its per-file catch
        // (`failed = 1`, the doc row stays); what FAILS the reconcile is the
        // section BACKFILL's `replace_for_doc`, which hits the same trigger
        // outside any catch.
        db.write_blocking(|ws| Ok(ws.main().connection().execute_batch(FAIL_TRIGGER)?))
            .unwrap();
        let gate = HelpDocReconcileGate::new();
        let ((a, b), lines) = crate::test_support::captured_with(|| {
            rt.block_on(async { tokio::join!(gate.ensure(&db, &files), gate.ensure(&db, &files)) })
        });
        assert!(a.is_none() && b.is_none());
        assert_eq!(
            gate.failures.load(std::sync::atomic::Ordering::SeqCst),
            1,
            "ONE attempt, shared by both callers"
        );
        let warns: Vec<&String> = lines
            .iter()
            .filter(|l| {
                l.contains(
                    "[HelpDocSync] Help doc reconcile failed; serving help from the existing index",
                )
            })
            .collect();
        assert_eq!(warns.len(), 2, "one WARN per caller: {lines:?}");
        for w in &warns {
            assert!(w.starts_with("WARN quilltap::help"), "{w}");
            assert!(w.contains("context=ensureHelpDocsSynced"), "{w}");
            assert!(w.contains("planted reconcile failure"), "{w}");
        }
        assert_eq!(count_lines(&lines, "[HelpDocSync] Help docs reconciled"), 0);

        db.write_blocking(|ws| {
            Ok(ws
                .main()
                .connection()
                .execute_batch("DROP TRIGGER fail_section_insert")?)
        })
        .unwrap();
        let (third, lines) =
            crate::test_support::captured_with(|| rt.block_on(gate.ensure(&db, &files)));
        let third = third.expect("the failure was forgotten: a fresh run succeeds");
        // The failed run's sync kept the doc (its slice failed inside the
        // per-file catch, counted `failed`), so the retry finds it unchanged
        // and section-less, and backfills it — exactly the step that failed.
        assert_eq!(third.sync.unchanged, 1);
        assert_eq!(third.sections_backfilled, 1);
        assert_eq!(
            count_lines(&lines, "Help doc reconcile failed"),
            0,
            "silence leg"
        );
        assert_eq!(count_lines(&lines, "[HelpDocSync] Help docs reconciled"), 1);
    }

    /// A SUCCESSFUL run is memoized: a later caller gets its result without a
    /// second reconcile (no second INFO), even after the tree changed.
    #[test]
    fn a_successful_run_is_not_repeated() {
        let dir = tempfile::tempdir().unwrap();
        let db = fresh_db(dir.path());
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let gate = HelpDocReconcileGate::new();
        let first = rt
            .block_on(gate.ensure(&db, &one_file()))
            .expect("first run");
        let (second, lines) =
            crate::test_support::captured_with(|| rt.block_on(gate.ensure(&db, &[])));
        assert_eq!(second.expect("memoized"), first);
        assert!(lines.is_empty(), "nothing re-ran: {lines:?}");
    }

    /// The reconcile's INFO: v4's six fields, in v4's order, at INFO on
    /// `quilltap::help`.
    #[test]
    fn the_reconcile_info_carries_v4s_six_fields_in_order() {
        let dir = tempfile::tempdir().unwrap();
        let db = fresh_db(dir.path());
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let (result, lines) = crate::test_support::captured_with(|| {
            rt.block_on(reconcile_help_docs(&db, &one_file()))
        });
        let result = result.unwrap();
        assert_eq!(result.sync.created, 1);
        // The provisioned instance's default profile queues the new doc.
        assert_eq!(result.incomplete, 1);
        let info: Vec<&String> = lines
            .iter()
            .filter(|l| l.contains("[HelpDocSync] Help docs reconciled"))
            .collect();
        assert_eq!(info.len(), 1, "{lines:?}");
        assert_eq!(
            info[0].as_str(),
            "INFO quilltap::help [HelpDocSync] Help docs reconciled context=reconcileHelpDocs \
             created=1 updated=0 deleted=0 unchanged=0 sectionsBackfilled=0 incomplete=1"
        );
    }

    // ---- P4.D222 Tier 2 item 9: the sync's lines on `tracing`, capture-pinned.
    // Conn-level on an in-memory database so the capture (thread-scoped) sees
    // every line — through the writer they would land on its thread.

    fn mem_conn() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch(crate::db::help_doc_chunks_repair::HELP_DOCS_TABLE_DDL)
            .unwrap();
        c.execute_batch(crate::db::help_doc_chunks_repair::HELP_DOC_CHUNKS_TABLE_DDL)
            .unwrap();
        c.execute_batch(
            "CREATE TABLE embedding_status (id TEXT PRIMARY KEY, entityType TEXT, \
             entityId TEXT, profileId TEXT, status TEXT)",
        )
        .unwrap();
        c
    }

    fn file(path: &str, body: &str) -> HelpSourceFile {
        HelpSourceFile {
            rel_path: path.into(),
            raw_content: body.into(),
        }
    }

    #[test]
    fn a_sync_logs_v4s_lines_created_updated_and_completed() {
        let c = mem_conn();
        let (first, lines) = crate::test_support::captured_with(|| {
            sync_help_docs(&c, &[file("help/a.md", "# A\n\nbody")]).unwrap()
        });
        assert_eq!(first.created, 1);
        let id = &first.changed_ids[0];
        let synced: Vec<&String> = lines
            .iter()
            .filter(|l| l.contains("[HelpDocSync] Synced help doc"))
            .collect();
        assert_eq!(synced.len(), 1, "{lines:?}");
        assert_eq!(
            synced[0].as_str(),
            format!(
                "DEBUG quilltap::help [HelpDocSync] Synced help doc context=syncHelpDocs \
                 path=help/a.md docId={id} action=created chunks=1"
            )
        );
        let done: Vec<&String> = lines
            .iter()
            .filter(|l| l.contains("[HelpDocSync] Sync completed"))
            .collect();
        assert_eq!(
            done,
            vec![
                "INFO quilltap::help [HelpDocSync] Sync completed context=syncHelpDocs \
                  totalOnDisk=1 created=1 updated=0 unchanged=0 deleted=0 failed=0 \
                  chunksWritten=1 changedIds=1"
            ]
        );

        // An edit: the update keeps the id, and a planted FAILED status goes.
        c.execute(
            "INSERT INTO embedding_status VALUES ('s1','HELP_DOC',?1,'p','FAILED')",
            [id],
        )
        .unwrap();
        let (second, lines) = crate::test_support::captured_with(|| {
            sync_help_docs(&c, &[file("help/a.md", "# A\n\nedited body")]).unwrap()
        });
        assert_eq!(second.updated, 1);
        assert_eq!(&second.changed_ids[0], id, "updated by the id it read");
        assert!(
            lines.iter().any(|l| l.contains("action=updated")),
            "{lines:?}"
        );
        let status_rows: i64 = c
            .query_row("SELECT count(*) FROM embedding_status", [], |r| r.get(0))
            .unwrap();
        assert_eq!(status_rows, 0, "the old text's FAILED status is cleared");

        // Unchanged: silence on the per-doc DEBUG (the INFO still fires).
        let (_, lines) = crate::test_support::captured_with(|| {
            sync_help_docs(&c, &[file("help/a.md", "# A\n\nedited body")]).unwrap()
        });
        assert!(
            !lines.iter().any(|l| l.contains("Synced help doc")),
            "{lines:?}"
        );
        assert!(lines.iter().any(|l| l.contains("unchanged=1")), "{lines:?}");
    }

    #[test]
    fn the_guard_and_failure_lines_fire_at_v4s_levels() {
        let c = mem_conn();
        // No files: v4's INFO, nothing else.
        let (_, lines) = crate::test_support::captured_with(|| sync_help_docs(&c, &[]).unwrap());
        assert_eq!(
            lines,
            vec![
                "INFO quilltap::help [HelpDocSync] No Markdown files found in help directory \
                  context=syncHelpDocs"
            ]
        );

        sync_help_docs(&c, &[file("help/a.md", "# A\n\nbody")]).unwrap();
        // Only blank content against a populated table: the prune refusal.
        let (r, lines) = crate::test_support::captured_with(|| {
            sync_help_docs(&c, &[file("help/b.md", "   ")]).unwrap()
        });
        assert_eq!(r.deleted, 0);
        assert!(
            lines.contains(
                &"WARN quilltap::help [HelpDocSync] No help docs on disk have usable content but \
                  the table is populated — skipping the destructive prune context=syncHelpDocs \
                  totalOnDisk=1 existingRows=1"
                    .to_string()
            ),
            "{lines:?}"
        );

        // A per-file failure: ERROR `Failed to sync file`, counted `failed`.
        c.execute_batch(
            "CREATE TRIGGER no_insert BEFORE INSERT ON help_docs \
             BEGIN SELECT RAISE(ABORT, 'planted'); END",
        )
        .unwrap();
        let (r, lines) = crate::test_support::captured_with(|| {
            sync_help_docs(
                &c,
                &[file("help/a.md", "# A\n\nbody"), file("help/c.md", "# C")],
            )
            .unwrap()
        });
        assert_eq!(r.failed, 1);
        let errs: Vec<&String> = lines.iter().filter(|l| l.starts_with("ERROR")).collect();
        assert_eq!(errs.len(), 1, "{lines:?}");
        assert!(errs[0].starts_with(
            "ERROR quilltap::help [HelpDocSync] Failed to sync file context=syncHelpDocs \
             filePath=help/c.md error="
        ));

        // A prune failure: ERROR `Failed to prune deleted help doc`.
        c.execute_batch(
            "CREATE TRIGGER no_delete BEFORE DELETE ON help_docs \
             BEGIN SELECT RAISE(ABORT, 'planted'); END",
        )
        .unwrap();
        let (r, lines) = crate::test_support::captured_with(|| {
            sync_help_docs(&c, &[file("help/other.md", "# O")]).unwrap()
        });
        assert_eq!(r.deleted, 0);
        assert!(
            lines.iter().any(|l| l.starts_with(
                "ERROR quilltap::help [HelpDocSync] Failed to prune deleted help doc \
                 context=syncHelpDocs docId="
            ) && l.contains(" path=help/a.md error=")),
            "{lines:?}"
        );
    }

    /// v4's `findAll` sits OUTSIDE the per-file catch: a failing read
    /// propagates (the reconcile's gate then logs and lets the next caller
    /// retry) instead of answering an empty result.
    #[test]
    fn a_failing_find_all_is_v4s_fallback_read_and_the_sync_carries_on() {
        let c = mem_conn();
        c.execute_batch("DROP TABLE help_docs").unwrap();
        let (result, lines) =
            crate::test_support::captured_with(|| sync_help_docs(&c, &[file("help/a.md", "# A")]));
        // v4's `findAll` never throws: the sync sees an empty table and goes on
        // (the per-file create then fails on the missing table, into `failed`).
        let result = result.expect("the fallback read never fails the sync");
        assert_eq!(result.failed, 1);
        let fallback: Vec<&String> = lines
            .iter()
            .filter(|l| l.contains("Error finding all entities"))
            .collect();
        assert_eq!(fallback.len(), 1, "{lines:?}");
        assert!(
            fallback[0].starts_with(
                "ERROR quilltap::db Error finding all entities collection=help_docs error="
            ),
            "{}",
            fallback[0]
        );
        assert!(
            fallback[0].contains("no such table: help_docs"),
            "{}",
            fallback[0]
        );
    }

    /// The enqueue's two lines: INFO `Enqueued help doc embeddings` with the
    /// NEW-job count, and DEBUG when no profile is configured.
    #[test]
    fn the_enqueue_logs_v4s_lines() {
        let dir = tempfile::tempdir().unwrap();
        let db = fresh_db(dir.path());
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let (_, lines) = crate::test_support::captured_with(|| {
            rt.block_on(reconcile_help_docs(&db, &one_file())).unwrap()
        });
        assert!(
            lines.contains(
                &"INFO quilltap::help [HelpDocSync] Enqueued help doc embeddings \
                  context=enqueueHelpDocEmbeddings enqueued=1 needEmbedding=1"
                    .to_string()
            ),
            "{lines:?}"
        );
        // A second pass finds the job already pending: needEmbedding 1,
        // enqueued 0 (v4's `isNew`).
        let (_, lines) = crate::test_support::captured_with(|| {
            rt.block_on(reconcile_help_docs(&db, &one_file())).unwrap()
        });
        assert!(
            lines
                .iter()
                .any(|l| l.contains("enqueued=0 needEmbedding=1")),
            "{lines:?}"
        );

        db.write_blocking(|ws| {
            Ok(ws
                .main()
                .connection()
                .execute_batch("DELETE FROM embedding_profiles")?)
        })
        .unwrap();
        let (_, lines) = crate::test_support::captured_with(|| {
            rt.block_on(reconcile_help_docs(&db, &one_file())).unwrap()
        });
        assert!(
            lines.contains(
                &"DEBUG quilltap::help [HelpDocSync] Help docs need embedding but no embedding \
                  profile is configured context=enqueueHelpDocEmbeddings needEmbedding=1"
                    .to_string()
            ),
            "{lines:?}"
        );
        assert!(!lines
            .iter()
            .any(|l| l.contains("Enqueued help doc embeddings")));
    }
}
