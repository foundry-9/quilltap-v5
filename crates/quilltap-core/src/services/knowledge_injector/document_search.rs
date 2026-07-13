//! Document Mount Chunk Search — v4 `lib/mount-index/document-search.ts`
//! (`searchDocumentChunks`).
//!
//! Semantic search across the document-mount chunk index using cosine similarity on
//! pre-computed embeddings, over the **mount-index** sibling DB. Read-only: it loads
//! embedded chunks for the target mount points, applies the `pathPrefix` + character-
//! read policy constraints BEFORE scoring, computes the cosine (with the optional
//! literal-phrase boost), filters by `minScore`, sorts by score desc, slices to
//! `limit`, then joins display metadata (mount name, file name / relative path /
//! file id) from the surviving rows.
//!
//! The scoring leaves are already ported and reused verbatim: cosine + dimension
//! guard ([`crate::embedding_vector`]), the literal-phrase leaves
//! ([`crate::literal_boost`]), and the Float32 BLOB decode
//! ([`crate::embedding_blob`]).
//!
//! **NOTE:** these mount-index read helpers live under the knowledge injector because
//! it is their only consumer today; they read the same tables the doc-mount repos
//! own, but as plain SELECTs (no writes), so they don't touch the writer-side repos.
//! They may be promoted to the doc-mount read layer later.

use rusqlite::{Connection, ToSql};

use crate::db::DbError;
use crate::embedding_blob::blob_to_float32;
use crate::embedding_vector::{assert_embedding_dimensions_match, cosine_similarity};
use crate::literal_boost::{apply_literal_boost, contains_literal_phrase, get_literal_phrase};

/// One search hit — the row shape `retrieveKnowledgeForTurn` consumes
/// (v4 `DocumentSearchResult`).
#[derive(Debug, Clone)]
pub struct DocumentSearchResult {
    pub chunk_id: String,
    pub mount_point_id: String,
    pub mount_point_name: String,
    pub file_id: String,
    pub file_name: String,
    pub relative_path: String,
    pub chunk_index: i64,
    pub heading_context: Option<String>,
    pub content: String,
    pub score: f64,
}

/// Search options (v4 `DocumentSearchOptions`), restricted to the fields the
/// knowledge injector passes. `mount_point_ids` is required by the injector (it
/// never uses the `projectId` / all-enabled fallbacks — those live in the tool
/// path, out of scope), but the fallbacks are ported for faithfulness.
#[derive(Debug, Clone, Default)]
pub struct DocumentSearchOptions {
    /// Scope to a specific project's linked mount points (v4 `projectId`) —
    /// consulted only when `mount_point_ids` is `None`; an empty link set
    /// short-circuits to no results (P4.6y, the semantic-search route).
    pub project_id: Option<String>,
    /// Scope to specific mount point IDs. When `None`, falls back to all enabled
    /// mounts.
    pub mount_point_ids: Option<Vec<String>>,
    /// Maximum results to return (v4 default 10).
    pub limit: Option<usize>,
    /// Minimum cosine similarity score (v4 default 0.3).
    pub min_score: Option<f64>,
    /// Restrict to chunks whose file's relativePath starts with this prefix
    /// (case-insensitive), applied to the chunk universe BEFORE scoring.
    pub path_prefix: Option<String>,
    /// Original query text — required when `apply_literal_phrase_boost` is set.
    pub query: Option<String>,
    /// When true and the trimmed query is long enough, verbatim (case-insensitive)
    /// content hits get their cosine lifted toward 1.0 before filtering/slicing.
    pub apply_literal_phrase_boost: bool,
    /// Fraction of the distance to 1.0 a literal hit lifts by (v4 default 0.5).
    pub literal_boost_fraction: Option<f64>,
    /// Include documents flagged `character_read:false` (operator escape hatch;
    /// default false — character-safe).
    pub include_blocked: bool,
}

/// A single embedded chunk loaded for scoring (v4 `CachedMountChunk`).
struct LoadedChunk {
    id: String,
    mount_point_id: String,
    link_id: String,
    chunk_index: i64,
    heading_context: Option<String>,
    content: String,
    embedding: Vec<f32>,
}

/// Search document mount chunks by semantic similarity (v4 `searchDocumentChunks`).
///
/// `conn` is a **mount-index** connection (read pool). The whole path is read-only.
pub fn search_document_chunks(
    conn: &Connection,
    query_embedding: &[f32],
    options: &DocumentSearchOptions,
) -> Result<Vec<DocumentSearchResult>, DbError> {
    // v4 uses JS `||` defaults — a FALSY zero falls back too (`limit: 0` → 10,
    // `minScore: 0` → 0.3; the semantic-search route's `threshold: 0` really
    // searches at 0.3). Ported broken-but-exact.
    let limit = match options.limit {
        Some(l) if l != 0 => l,
        _ => 10,
    };
    let min_score = match options.min_score {
        Some(m) if m != 0.0 => m,
        _ => 0.3,
    };

    // Determine which mount point IDs to search (v4 order: explicit ids win;
    // else a projectId resolves its linked mounts — empty set returns [];
    // else every enabled mount).
    let mount_point_ids: Vec<String> = match &options.mount_point_ids {
        Some(ids) => ids.clone(),
        None => match &options.project_id {
            Some(pid) => {
                let ids =
                    crate::db::project_doc_mount_links::ProjectDocMountLinksRepository::new(conn)
                        .find_by_project_id(pid)?;
                if ids.is_empty() {
                    return Ok(Vec::new());
                }
                ids
            }
            None => find_enabled_mount_point_ids(conn)?,
        },
    };
    if mount_point_ids.is_empty() {
        return Ok(Vec::new());
    }

    // Load all embedded chunks for the target mount points (v4: the in-memory
    // cache, but the underlying read is per-mount, embedding non-null non-empty).
    let all_chunks = load_embedded_chunks(conn, &mount_point_ids)?;
    if all_chunks.is_empty() {
        return Ok(Vec::new());
    }

    // Pre-flight dimension guard against the first stored chunk.
    let sample_stored = &all_chunks[0].embedding;
    assert_embedding_dimensions_match(
        query_embedding.len(),
        sample_stored.len(),
        Some("document chunk search"),
    )
    .map_err(|e| DbError::Sqlite(rusqlite::Error::InvalidParameterName(e.message())))?;

    // pathPrefix + character-read policy are hard constraints applied to the chunk
    // universe BEFORE scoring. Both are gathered in a single pass over the target
    // mounts' link rows.
    let include_blocked = options.include_blocked;
    let lower_prefix = options.path_prefix.as_ref().map(|p| p.to_lowercase());

    // allowed_link_ids: Some(set) when a prefix is active (only those pass);
    // blocked_link_ids: Some(set) when NOT include_blocked (those are dropped).
    let mut allowed_link_ids: Option<std::collections::HashSet<String>> = None;
    let mut blocked_link_ids: Option<std::collections::HashSet<String>> = None;
    if lower_prefix.is_some() || !include_blocked {
        let mut allowed = if lower_prefix.is_some() {
            Some(std::collections::HashSet::new())
        } else {
            None
        };
        let mut blocked = if include_blocked {
            None
        } else {
            Some(std::collections::HashSet::new())
        };
        for mp_id in &mount_point_ids {
            for link in find_links_for_policy(conn, mp_id)? {
                if let (Some(set), Some(prefix)) = (&mut allowed, &lower_prefix) {
                    if link
                        .relative_path
                        .to_lowercase()
                        .starts_with(prefix.as_str())
                    {
                        set.insert(link.id.clone());
                    }
                }
                if let Some(set) = &mut blocked {
                    if !link.allow_character_read {
                        set.insert(link.id.clone());
                    }
                }
            }
        }
        allowed_link_ids = allowed;
        blocked_link_ids = blocked;
    }

    // Apply the link-scope constraints.
    let mut chunks_in_scope: Vec<&LoadedChunk> = all_chunks.iter().collect();
    if let Some(allowed) = &allowed_link_ids {
        if allowed.is_empty() {
            return Ok(Vec::new());
        }
        chunks_in_scope.retain(|c| allowed.contains(&c.link_id));
    }
    if let Some(blocked) = &blocked_link_ids {
        if !blocked.is_empty() {
            chunks_in_scope.retain(|c| !blocked.contains(&c.link_id));
        }
    }
    if chunks_in_scope.is_empty() {
        return Ok(Vec::new());
    }

    // Compute cosine (with optional literal boost).
    let literal_phrase = if options.apply_literal_phrase_boost {
        get_literal_phrase(options.query.as_deref())
    } else {
        None
    };
    let literal_boost_fraction = options.literal_boost_fraction.unwrap_or(0.5);

    struct Scored<'a> {
        chunk: &'a LoadedChunk,
        score: f64,
    }
    let mut scored_all: Vec<Scored> = Vec::with_capacity(chunks_in_scope.len());
    for chunk in &chunks_in_scope {
        // Cosine on unit-length vectors; a dimension mismatch surfaces the same
        // guard as v4 (already pre-flighted against the sample above).
        let raw_score = cosine_similarity(query_embedding, &chunk.embedding)
            .map_err(|e| DbError::Sqlite(rusqlite::Error::InvalidParameterName(e.message())))?;
        let literal_hit = literal_phrase
            .as_ref()
            .map(|p| contains_literal_phrase(Some(&chunk.content), p))
            .unwrap_or(false);
        let score = if literal_hit {
            apply_literal_boost(raw_score, literal_boost_fraction)
        } else {
            raw_score
        };
        scored_all.push(Scored { chunk, score });
    }

    // filter >= minScore, sort by score desc (STABLE — v4's Array.sort is stable),
    // slice to limit.
    let mut scored: Vec<Scored> = scored_all
        .into_iter()
        .filter(|s| s.score >= min_score)
        .collect();
    // Stable sort: preserve original (load) order among equal scores, matching JS's
    // stable Array.prototype.sort on `b.score - a.score`.
    scored.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    scored.truncate(limit);

    if scored.is_empty() {
        return Ok(Vec::new());
    }

    // Mount-name map (from ALL mount points, v4 reads findAll here).
    let mount_point_map = find_all_mount_point_names(conn)?;

    // Display metadata by linkId, from the surviving rows only.
    let mut results = Vec::with_capacity(scored.len());
    for s in &scored {
        let file_info = find_link_display_by_id(conn, &s.chunk.link_id)?;
        let (file_id, file_name, relative_path) = match file_info {
            Some((fid, fname, rpath)) => (fid, fname, rpath),
            None => (
                s.chunk.link_id.clone(),
                "Unknown".to_string(),
                String::new(),
            ),
        };
        results.push(DocumentSearchResult {
            chunk_id: s.chunk.id.clone(),
            mount_point_id: s.chunk.mount_point_id.clone(),
            mount_point_name: mount_point_map
                .iter()
                .find(|(id, _)| id == &s.chunk.mount_point_id)
                .map(|(_, n)| n.clone())
                .unwrap_or_else(|| "Unknown".to_string()),
            file_id,
            file_name,
            relative_path,
            chunk_index: s.chunk.chunk_index,
            heading_context: s.chunk.heading_context.clone(),
            content: s.chunk.content.clone(),
            score: s.score,
        });
    }

    Ok(results)
}

/// `docMountPoints.findEnabled()` → the ids of every enabled mount point.
fn find_enabled_mount_point_ids(conn: &Connection) -> Result<Vec<String>, DbError> {
    let mut stmt = conn.prepare("SELECT id FROM doc_mount_points WHERE enabled = 1")?;
    let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

/// `docMountPoints.findAll()` → `(id, name)` pairs for the mount-name lookup.
fn find_all_mount_point_names(conn: &Connection) -> Result<Vec<(String, String)>, DbError> {
    let mut stmt = conn.prepare("SELECT id, name FROM doc_mount_points")?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

/// The link-policy subset the pre-scoring pass reads (v4 iterates
/// `docMountFileLinks.findByMountPointId(mpId)` for `id`/`relativePath`/
/// `allowCharacterRead`). Per mount point.
struct PolicyLink {
    id: String,
    relative_path: String,
    allow_character_read: bool,
}

fn find_links_for_policy(
    conn: &Connection,
    mount_point_id: &str,
) -> Result<Vec<PolicyLink>, DbError> {
    let mut stmt = conn.prepare(
        "SELECT id, relativePath, allowCharacterRead \
         FROM doc_mount_file_links WHERE mountPointId = ?1",
    )?;
    let rows = stmt.query_map([mount_point_id], |r| {
        // allowCharacterRead is an INTEGER 0/1 (or NULL — treat NULL as allowed,
        // matching v4's `link.allowCharacterRead === false` strict-false check:
        // only an explicit false blocks).
        let allow: Option<i64> = r.get(2)?;
        Ok(PolicyLink {
            id: r.get(0)?,
            relative_path: r.get(1)?,
            allow_character_read: allow != Some(0),
        })
    })?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

/// Load every embedded (non-null, non-empty) chunk for the target mount points, in
/// mount-point order (v4's per-mount loop + non-empty filter). Only the columns the
/// scorer + renderer consume.
fn load_embedded_chunks(
    conn: &Connection,
    mount_point_ids: &[String],
) -> Result<Vec<LoadedChunk>, DbError> {
    let mut out = Vec::new();
    // Per mount point so the result order matches v4's `for (const mpId ...)` loop.
    let mut stmt = conn.prepare(
        "SELECT id, mountPointId, linkId, chunkIndex, headingContext, content, embedding \
         FROM doc_mount_chunks WHERE mountPointId = ?1 AND embedding IS NOT NULL",
    )?;
    for mp_id in mount_point_ids {
        let rows = stmt.query_map([mp_id], |r| {
            let blob: Vec<u8> = r.get(6)?;
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, f64>(3)? as i64,
                r.get::<_, Option<String>>(4)?,
                r.get::<_, String>(5)?,
                blob,
            ))
        })?;
        for r in rows {
            let (id, mount_point_id, link_id, chunk_index, heading_context, content, blob) = r?;
            let embedding = blob_to_float32(&blob);
            // v4: filter chunks with embedding.length > 0.
            if embedding.is_empty() {
                continue;
            }
            out.push(LoadedChunk {
                id,
                mount_point_id,
                link_id,
                chunk_index,
                heading_context,
                content,
                embedding,
            });
        }
    }
    Ok(out)
}

/// `docMountFileLinks.findByIdWithContent(linkId)` display subset →
/// `(fileId, fileName, relativePath)` (v4 joins `doc_mount_file_links` with
/// `doc_mount_files`; the injector reads only these three).
fn find_link_display_by_id(
    conn: &Connection,
    link_id: &str,
) -> Result<Option<(String, String, String)>, DbError> {
    let res = conn.query_row(
        "SELECT l.fileId, l.fileName, l.relativePath \
         FROM doc_mount_file_links l \
         JOIN doc_mount_files f ON f.id = l.fileId \
         WHERE l.id = ?1 LIMIT 1",
        [link_id],
        |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        },
    );
    match res {
        Ok(t) => Ok(Some(t)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// The joined file row the injector reads via `docMountFiles.findByMountPointAndPath`
/// (v4 returns a `DocMountFileLinkWithContent` carrying `fileType`/`fileName`/
/// `relativePath`/`fileId`). Case-insensitive path compare. Returns the first match.
pub struct LinkFileRow {
    /// The joined columns; only `file_type` is consumed by the injector today, but
    /// the full row is returned for parity with v4's `DocMountFileLinkWithContent`.
    #[allow(dead_code)]
    pub file_id: String,
    #[allow(dead_code)]
    pub file_name: String,
    #[allow(dead_code)]
    pub relative_path: String,
    pub file_type: String,
}

pub fn find_link_with_file_by_mount_point_and_path(
    conn: &Connection,
    mount_point_id: &str,
    relative_path: &str,
) -> Result<Option<LinkFileRow>, DbError> {
    let params: [&dyn ToSql; 2] = [&mount_point_id, &relative_path];
    let res = conn.query_row(
        "SELECT l.fileId, l.fileName, l.relativePath, f.fileType \
         FROM doc_mount_file_links l \
         JOIN doc_mount_files f ON f.id = l.fileId \
         WHERE l.mountPointId = ?1 AND LOWER(l.relativePath) = LOWER(?2) LIMIT 1",
        params,
        |r| {
            Ok(LinkFileRow {
                file_id: r.get(0)?,
                file_name: r.get(1)?,
                relative_path: r.get(2)?,
                file_type: r.get(3)?,
            })
        },
    );
    match res {
        Ok(t) => Ok(Some(t)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}
