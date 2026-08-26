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

use rusqlite::Connection;

use crate::db::characters_read;
use crate::db::doc_mount_chunks::DocMountChunksRepository;
use crate::db::doc_mount_file_links::DocMountFileLinksRepository;
use crate::db::doc_mount_points::DocMountPointsRepository;
use crate::db::DbError;
use crate::doc_edit::uri_producers::DocStoreRefResolver;
use crate::episodic::js_date_parse_ms;
use crate::jsstr::{js_index_of, js_trim, utf16_len};

/// Documents returned by default; also the ceiling the route paginates over
/// (v4 `DEFAULT_LIMIT`).
pub const DEFAULT_LIMIT: usize = 100;

/// Rows each SQL scan may match before it short-circuits (v4 `SCAN_CAP`).
/// Generous enough that ranking still has something to choose between, small
/// enough that a large instance never streams its whole corpus through JS.
///
/// ⚠ `total_count` is therefore **capped by the scans, knowingly** — v4's own
/// doc comment says so. Do not "fix" it.
const SCAN_CAP: i64 = 200;

/// Characters of chunk text shown around a content match (v4 `SNIPPET_LENGTH`).
const SNIPPET_LENGTH: usize = 200;

/// Where the query matched, and how strongly — mirrors the search route's
/// `getMatchPriority` ordering: 0 an exact file name, 1 a name/path substring,
/// 2 a hit inside the document's text (v4 `DocumentTextMatchField`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentTextMatchField {
    FileName,
    RelativePath,
    Content,
}

impl DocumentTextMatchField {
    /// The wire spelling the route pushes as `matchedField`.
    pub fn as_str(self) -> &'static str {
        match self {
            DocumentTextMatchField::FileName => "fileName",
            DocumentTextMatchField::RelativePath => "relativePath",
            DocumentTextMatchField::Content => "content",
        }
    }
}

/// One matched document (v4 `DocumentTextSearchResult`).
#[derive(Debug, Clone)]
pub struct DocumentTextSearchResult {
    /// The `doc_mount_file_links` row id — document identity, one result per link.
    pub link_id: String,
    pub mount_point_id: String,
    pub mount_point_name: String,
    /// Addressable store reference: the name, or the UUID when it's
    /// ambiguous/reserved.
    pub mount_point_ref: String,
    /// `'documents'` or `'character'` (v4 defaults a missing value to
    /// `'documents'` in `describe()`).
    pub store_type: String,
    pub relative_path: String,
    pub file_name: String,
    pub matched_field: DocumentTextMatchField,
    /// The matched text the caller renders; for a name-only hit, the path itself.
    pub matched_value: String,
    pub snippet: String,
    pub match_priority: u8,
    pub updated_at: String,
}

/// v4 `DocumentTextSearchOptions`.
#[derive(Debug, Clone, Default)]
pub struct DocumentTextSearchOptions {
    /// Maximum documents to return (default [`DEFAULT_LIMIT`]).
    pub limit: Option<usize>,
    /// Stores to leave out on top of the archived-vault exclusion.
    pub exclude_mount_point_ids: Vec<String>,
}

/// The vault mount-point ids belonging to **archived** characters (v4
/// `getArchivedCharacterVaultMountPointIds`, `lib/mount-index/character-vault
/// .ts:121`).
///
/// An archived character is a tombstone: its row survives, its vault is pruned
/// but still enabled and still enumerable by `findEnabled()`. Operator surfaces
/// that walk every store (the global search bar's Documents chip) subtract this
/// set, so a tombstone's leftovers never become a click target that leads back
/// to an edit path the archive guards exist to prevent.
///
/// Reads the **raw** character rows deliberately: the overlay read path mounts
/// each character's vault, which is precisely what a pruned vault can't serve,
/// and all we need is the `archivedAt` / `characterDocumentMountPointId`
/// columns. (The same raw-read-suffices pattern as `documents/mod.rs:650-680`
/// and `api/files.rs:502-505`.)
///
/// **Lives here, not in `db::character_vault`**, per the P4.D122 work order —
/// that module is another lane's this round, and the read this needs is already
/// public.
pub fn archived_character_vault_mount_point_ids(main: &Connection) -> Result<Vec<String>, DbError> {
    let characters = characters_read::find_all_raw(main)?;
    let mut ids = Vec::new();
    for c in &characters {
        // v4: `c.archivedAt && c.characterDocumentMountPointId` — JS truthiness,
        // so null/absent/"" all fail on either side.
        let archived =
            matches!(c.get("archivedAt").and_then(|v| v.as_str()), Some(s) if !s.is_empty());
        let vault = c
            .get("characterDocumentMountPointId")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty());
        if let (true, Some(v)) = (archived, vault) {
            ids.push(v.to_string());
        }
    }
    tracing::debug!(
        target: "quilltap::document_text_search",
        archived_with_vault = ids.len(),
        characters_scanned = characters.len(),
        "Collected archived character vault mount points",
    );
    Ok(ids)
}

/// JS `String.prototype.slice(start, end)` over UTF-16 code units, with both
/// bounds already clamped by the caller (v4's `buildContentSnippet` only ever
/// passes in-range values).
fn utf16_slice(s: &str, start: usize, end: usize) -> String {
    let units: Vec<u16> = s
        .encode_utf16()
        .skip(start)
        .take(end.saturating_sub(start))
        .collect();
    String::from_utf16_lossy(&units)
}

/// v4 `buildContentSnippet` — trim a chunk to a readable window around the
/// match, prefixed with the chunk's heading context when it has one.
///
/// Two quirks are load-bearing and deliberately preserved:
///
/// 1. **`lead` is one THIRD of the remaining budget, not one half**
///    (`floor((SNIPPET_LENGTH - query.length) / 3)`), so the match sits about a
///    third of the way in rather than centred. `query.length` is UTF-16 units,
///    and a query longer than the window drives `lead` to 0 through the
///    `Math.max(0, …)`.
/// 2. **The indices come from the LOWERCASED string but slice the ORIGINAL** —
///    the same case-fold skew `api::ui_search`'s header already documents. A
///    character whose lowercase form has a different UTF-16 length shifts the
///    window; v4 does this and so does the port.
///
/// The `.trim()` runs BEFORE the `...` ellipses are added, so a window that
/// begins mid-whitespace loses it and still gets its leading ellipsis.
fn build_content_snippet(content: &str, query: &str, heading_context: Option<&str>) -> String {
    let lower_content = content.to_lowercase();
    let lower_query = query.to_lowercase();
    let match_index = js_index_of(&lower_content, &lower_query, 0);
    // `Math.max(0, Math.floor((SNIPPET_LENGTH - query.length) / 3))` — computed
    // in signed arithmetic because the subtraction goes negative for a long query.
    let lead = {
        let raw = SNIPPET_LENGTH as i64 - utf16_len(query) as i64;
        // JS `Math.floor` on a negative rounds DOWN; the max(0, …) then clamps
        // it, so the sign of the division never reaches the result.
        let floored = (raw as f64 / 3.0).floor();
        if floored < 0.0 {
            0usize
        } else {
            floored as usize
        }
    };
    let content_len = utf16_len(content);
    let start = match match_index {
        None => 0,
        Some(i) => i.saturating_sub(lead),
    };
    let end = content_len.min(start + SNIPPET_LENGTH);

    let mut snippet = js_trim(&utf16_slice(content, start, end)).to_string();
    if start > 0 {
        snippet = format!("...{snippet}");
    }
    if end < content_len {
        snippet = format!("{snippet}...");
    }
    // `headingContext ? … : …` — an EMPTY heading is JS-falsy, so it adds no
    // prefix. The separator is an em dash with a space on each side.
    match heading_context.filter(|h| !h.is_empty()) {
        Some(h) => format!("{h} — {snippet}"),
        None => snippet,
    }
}

/// Search every enabled document store for `query` (v4 `searchDocumentText`).
///
/// File-name / path hits shadow content hits for the same document — one result
/// per document, best match wins — and results are ranked by match priority then
/// recency. `total_count` is the number of distinct documents matched (bounded
/// by [`SCAN_CAP`]); `results` is that list sliced to `limit`.
pub fn search_document_text(
    main: &Connection,
    mount: &Connection,
    query: &str,
    opts: &DocumentTextSearchOptions,
) -> (Vec<DocumentTextSearchResult>, usize) {
    let limit = opts.limit.unwrap_or(DEFAULT_LIMIT);
    let trimmed = js_trim(query);
    if trimmed.is_empty() {
        // v4 returns before touching a single repo.
        return (Vec::new(), 0);
    }

    let enabled = DocMountPointsRepository::new(mount)
        .find_enabled_for_search()
        .unwrap_or_default();

    // v4's `excluded` is a `Set`, so duplicates collapse — both on insert and in
    // the `excluded.size` the no-stores debug log carries.
    let mut excluded: Vec<String> = Vec::new();
    for id in &opts.exclude_mount_point_ids {
        if !excluded.contains(id) {
            excluded.push(id.clone());
        }
    }
    let mut archived_vaults_excluded = 0usize;
    match archived_character_vault_mount_point_ids(main) {
        Ok(ids) => {
            for id in ids {
                if !excluded.contains(&id) {
                    excluded.push(id);
                }
                archived_vaults_excluded += 1;
            }
        }
        Err(error) => {
            // Fail CLOSED: if we can't tell which vaults belong to archived
            // characters, drop every character vault rather than risk surfacing
            // a tombstone's contents. Ordinary document stores still search.
            tracing::error!(
                target: "quilltap::document_text_search",
                error = %error,
                "Could not resolve archived character vaults; excluding all character vaults from search",
            );
            for mp in &enabled {
                // v4's compare is STRICT `mp.storeType === 'character'`, so a
                // NULL-`storeType` vault SURVIVES this sweep. Carried.
                if mp.store_type.as_deref() == Some("character") && !excluded.contains(&mp.id) {
                    excluded.push(mp.id.clone());
                }
            }
        }
    }

    let stores: Vec<_> = enabled
        .iter()
        .filter(|mp| !excluded.contains(&mp.id))
        .collect();
    if stores.is_empty() {
        tracing::debug!(
            target: "quilltap::document_text_search",
            enabled = enabled.len(),
            // v4 logs `excluded.size` — the SET's size, so duplicate ids
            // collapse. `excluded` is deduped on insert above for the same
            // reason.
            excluded = excluded.len(),
            "Document text search found no stores in scope",
        );
        return (Vec::new(), 0);
    }

    let mount_point_ids: Vec<String> = stores.iter().map(|mp| mp.id.clone()).collect();
    let name_hits = DocMountFileLinksRepository::new(mount).search_by_name_or_path(
        trimmed,
        &mount_point_ids,
        SCAN_CAP,
    );
    let content_hits =
        DocMountChunksRepository::new(mount).search_content(trimmed, &mount_point_ids, SCAN_CAP);

    let ref_resolver = DocStoreRefResolver::build(mount);
    let lower_query = trimmed.to_lowercase();

    // Insertion-ordered map keyed by linkId (v4's `Map`): `set` on an existing
    // key OVERWRITES the value and KEEPS the original position.
    let mut order: Vec<String> = Vec::new();
    let mut by_link_id: std::collections::HashMap<String, DocumentTextSearchResult> =
        std::collections::HashMap::new();

    let describe = |mount_point_id: &str| -> Option<(String, String, String)> {
        let store = stores.iter().find(|mp| mp.id == mount_point_id)?;
        Some((
            store.name.clone(),
            ref_resolver
                .ref_for_mount(&store.name, &store.id)
                .to_string(),
            // `store.storeType ?? 'documents'`.
            store
                .store_type
                .clone()
                .unwrap_or_else(|| "documents".to_string()),
        ))
    };

    // Name/path hits first — they outrank (and so shadow) a content hit on the
    // same document.
    for hit in &name_hits {
        // A hit whose store isn't in the resolver's map is silently dropped.
        let Some((name, store_ref, store_type)) = describe(&hit.mount_point_id) else {
            continue;
        };
        let name_lower = hit.file_name.to_lowercase();
        let name_matches = name_lower.contains(&lower_query);
        if !by_link_id.contains_key(&hit.id) {
            order.push(hit.id.clone());
        }
        by_link_id.insert(
            hit.id.clone(),
            DocumentTextSearchResult {
                link_id: hit.id.clone(),
                mount_point_id: hit.mount_point_id.clone(),
                mount_point_name: name,
                mount_point_ref: store_ref,
                store_type,
                relative_path: hit.relative_path.clone(),
                file_name: hit.file_name.clone(),
                matched_field: if name_matches {
                    DocumentTextMatchField::FileName
                } else {
                    DocumentTextMatchField::RelativePath
                },
                matched_value: if name_matches {
                    hit.file_name.clone()
                } else {
                    hit.relative_path.clone()
                },
                // For a name/path hit the snippet IS the path.
                snippet: hit.relative_path.clone(),
                // Priority 0 only when the WHOLE lowercased file name equals the
                // WHOLE lowercased query — so `manifesto` against
                // `manifesto.md` is 1, not 0.
                match_priority: if name_lower == lower_query { 0 } else { 1 },
                updated_at: hit.updated_at.clone(),
            },
        );
    }

    for hit in &content_hits {
        if by_link_id.contains_key(&hit.link_id) {
            continue;
        }
        let Some((name, store_ref, store_type)) = describe(&hit.mount_point_id) else {
            continue;
        };
        order.push(hit.link_id.clone());
        by_link_id.insert(
            hit.link_id.clone(),
            DocumentTextSearchResult {
                link_id: hit.link_id.clone(),
                mount_point_id: hit.mount_point_id.clone(),
                mount_point_name: name,
                mount_point_ref: store_ref,
                store_type,
                relative_path: hit.relative_path.clone(),
                file_name: hit.file_name.clone(),
                matched_field: DocumentTextMatchField::Content,
                // A hard-coded 200 in v4, NOT `SNIPPET_LENGTH` — the two happen
                // to agree today; keep them separate so a change to one doesn't
                // silently move the other.
                matched_value: crate::jsstr::utf16_truncate(&hit.content, 200),
                snippet: build_content_snippet(
                    &hit.content,
                    trimmed,
                    hit.heading_context.as_deref(),
                ),
                match_priority: 2,
                updated_at: hit.updated_at.clone(),
            },
        );
    }

    let mut merged: Vec<DocumentTextSearchResult> = order
        .into_iter()
        .filter_map(|id| by_link_id.remove(&id))
        .collect();
    // `matchPriority` asc, then `new Date(updatedAt)` desc. An unparseable date
    // makes v4's comparator NaN, which V8's sort treats as equal.
    merged.sort_by(|a, b| {
        a.match_priority.cmp(&b.match_priority).then_with(|| {
            match (
                js_date_parse_ms(&a.updated_at),
                js_date_parse_ms(&b.updated_at),
            ) {
                (Some(ta), Some(tb)) => tb.cmp(&ta),
                _ => std::cmp::Ordering::Equal,
            }
        })
    });

    let total_count = merged.len();
    tracing::debug!(
        target: "quilltap::document_text_search",
        // The query TEXT is never logged — only its length (v4).
        query_length = utf16_len(trimmed),
        stores = stores.len(),
        archived_vaults_excluded,
        name_hits = name_hits.len(),
        content_hits = content_hits.len(),
        documents = total_count,
        returned = total_count.min(limit),
        "Document text search completed",
    );

    merged.truncate(limit);
    (merged, total_count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;

    /// A scratch mount-index DB with just the four tables the engine reads.
    fn mount_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE doc_mount_points (
                id TEXT PRIMARY KEY NOT NULL, name TEXT NOT NULL,
                storeType TEXT, enabled INTEGER DEFAULT 1,
                mountType TEXT DEFAULT 'database', basePath TEXT DEFAULT '');
             CREATE TABLE doc_mount_files (id TEXT PRIMARY KEY NOT NULL, fileType TEXT);
             CREATE TABLE doc_mount_file_links (
                id TEXT PRIMARY KEY NOT NULL, fileId TEXT NOT NULL,
                mountPointId TEXT NOT NULL, relativePath TEXT NOT NULL,
                fileName TEXT NOT NULL, updatedAt TEXT NOT NULL);
             CREATE TABLE doc_mount_chunks (
                id TEXT PRIMARY KEY NOT NULL, linkId TEXT NOT NULL,
                mountPointId TEXT NOT NULL, chunkIndex REAL NOT NULL,
                content TEXT NOT NULL, headingContext TEXT);
             INSERT INTO doc_mount_files (id, fileType) VALUES ('f-md', 'markdown');",
        )
        .unwrap();
        conn
    }

    fn store(conn: &Connection, id: &str, name: &str, store_type: Option<&str>) {
        conn.execute(
            "INSERT INTO doc_mount_points (id, name, storeType, enabled) VALUES (?1, ?2, ?3, 1)",
            params![id, name, store_type],
        )
        .unwrap();
    }

    fn doc(conn: &Connection, id: &str, mp: &str, path: &str, updated: &str) {
        conn.execute(
            "INSERT INTO doc_mount_file_links \
             (id, fileId, mountPointId, relativePath, fileName, updatedAt) \
             VALUES (?1, 'f-md', ?2, ?3, ?4, ?5)",
            params![id, mp, path, path.rsplit('/').next().unwrap(), updated],
        )
        .unwrap();
    }

    /// A `characters` table carrying exactly the columns `find_all_raw` selects.
    fn main_db_with_characters() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        let cols = "id, userId, name, defaultImageId, defaultConnectionProfileId, \
             defaultPartnerId, defaultRoleplayTemplateId, defaultImageProfileId, sillyTavernData, \
             isFavorite, npc, controlledBy, defaultAgentModeEnabled, defaultHelpToolsEnabled, \
             defaultTimestampConfig, defaultScenarioId, defaultSystemPromptId, \
             characterDocumentMountPointId, canDressThemselves, canCreateOutfits, \
             systemTransparency, coreWhisperEnabled, canBeCarina, partnerLinks, tags, \
             avatarOverrides, createdAt, updatedAt, archivedAt, archiveFileId, \
             archivedAvatarFileId";
        let ddl: Vec<String> = cols
            .split(',')
            .map(|c| format!("\"{}\" TEXT", c.trim()))
            .collect();
        conn.execute_batch(&format!("CREATE TABLE characters ({});", ddl.join(", ")))
            .unwrap();
        conn
    }

    /// A main DB with NO `characters` table at all — every `find_all_raw`
    /// errors, which is the ONLY way to reach v4's fail-closed arm (no input
    /// can; the "break the table" recipe).
    fn main_db_without_characters() -> Connection {
        Connection::open_in_memory().unwrap()
    }

    fn add_character(conn: &Connection, id: &str, vault: Option<&str>, archived: Option<&str>) {
        conn.execute(
            "INSERT INTO characters \
             (id, userId, name, characterDocumentMountPointId, archivedAt, createdAt, updatedAt) \
             VALUES (?1, 'u-1', ?1, ?2, ?3, '2026-01-01T00:00:00.000Z', '2026-01-01T00:00:00.000Z')",
            params![id, vault, archived],
        )
        .unwrap();
    }

    fn names(results: &[DocumentTextSearchResult]) -> Vec<&str> {
        results.iter().map(|r| r.file_name.as_str()).collect()
    }

    fn search(main: &Connection, mount: &Connection, q: &str) -> Vec<DocumentTextSearchResult> {
        search_document_text(main, mount, q, &DocumentTextSearchOptions::default()).0
    }

    /// An empty (or whitespace-only) query returns zeros without touching a
    /// single repo — proven by handing it a main DB with no tables at all.
    #[test]
    fn an_empty_query_touches_nothing() {
        let mount = Connection::open_in_memory().unwrap();
        let main = Connection::open_in_memory().unwrap();
        assert_eq!(
            search_document_text(&main, &mount, "   ", &DocumentTextSearchOptions::default()).1,
            0
        );
    }

    /// The archived-vault exclusion: an archived character's vault is swept out
    /// while an ordinary store and a live character's vault both answer.
    #[test]
    fn an_archived_characters_vault_is_excluded() {
        let mount = mount_db();
        store(&mount, "mp-docs", "Airship Papers", Some("documents"));
        store(&mount, "mp-live", "Live Vault", Some("character"));
        store(&mount, "mp-dead", "Dead Vault", Some("character"));
        doc(
            &mount,
            "l-docs",
            "mp-docs",
            "manifesto.md",
            "2026-01-03T00:00:00.000Z",
        );
        doc(
            &mount,
            "l-live",
            "mp-live",
            "manifesto-live.md",
            "2026-01-02T00:00:00.000Z",
        );
        doc(
            &mount,
            "l-dead",
            "mp-dead",
            "manifesto-dead.md",
            "2026-01-01T00:00:00.000Z",
        );

        let main = main_db_with_characters();
        add_character(&main, "c-live", Some("mp-live"), None);
        add_character(
            &main,
            "c-dead",
            Some("mp-dead"),
            Some("2026-01-05T00:00:00.000Z"),
        );

        assert_eq!(
            names(&search(&main, &mount, "manifesto")),
            ["manifesto.md", "manifesto-live.md"]
        );
    }

    /// v4's filter is `c.archivedAt && c.characterDocumentMountPointId` — JS
    /// truthiness on BOTH sides, so an empty string is as good as null.
    #[test]
    fn the_archived_filter_is_js_truthy_on_both_columns() {
        let main = main_db_with_characters();
        add_character(&main, "c-1", Some("mp-1"), Some(""));
        add_character(&main, "c-2", Some(""), Some("2026-01-05T00:00:00.000Z"));
        add_character(&main, "c-3", None, Some("2026-01-05T00:00:00.000Z"));
        add_character(&main, "c-4", Some("mp-4"), None);
        add_character(&main, "c-5", Some("mp-5"), Some("2026-01-05T00:00:00.000Z"));
        assert_eq!(
            archived_character_vault_mount_point_ids(&main).unwrap(),
            ["mp-5"]
        );
    }

    /// FAIL CLOSED. When the archived set can't be resolved, EVERY character
    /// vault is dropped and ordinary stores still search — v4's deliberate
    /// asymmetry, and the reason a tombstone can never leak.
    #[test]
    fn an_unreadable_archived_set_drops_every_character_vault() {
        let mount = mount_db();
        store(&mount, "mp-docs", "Airship Papers", Some("documents"));
        store(&mount, "mp-vault", "A Vault", Some("character"));
        doc(
            &mount,
            "l-docs",
            "mp-docs",
            "manifesto.md",
            "2026-01-02T00:00:00.000Z",
        );
        doc(
            &mount,
            "l-vault",
            "mp-vault",
            "manifesto-vault.md",
            "2026-01-01T00:00:00.000Z",
        );

        let broken = main_db_without_characters();
        assert_eq!(
            names(&search(&broken, &mount, "manifesto")),
            ["manifesto.md"]
        );

        // The same corpus with a readable (empty) characters table keeps both.
        let ok = main_db_with_characters();
        assert_eq!(
            names(&search(&ok, &mount, "manifesto")),
            ["manifesto.md", "manifesto-vault.md"]
        );
    }

    /// The fail-closed sweep compares `storeType === 'character'` STRICTLY, so a
    /// store whose `storeType` is NULL survives it — v4's edge, carried.
    #[test]
    fn a_null_store_type_survives_the_fail_closed_sweep() {
        let mount = mount_db();
        store(&mount, "mp-null", "Unlabelled", None);
        store(&mount, "mp-vault", "A Vault", Some("character"));
        doc(
            &mount,
            "l-null",
            "mp-null",
            "manifesto-null.md",
            "2026-01-02T00:00:00.000Z",
        );
        doc(
            &mount,
            "l-vault",
            "mp-vault",
            "manifesto-vault.md",
            "2026-01-01T00:00:00.000Z",
        );

        let results = search(&main_db_without_characters(), &mount, "manifesto");
        assert_eq!(names(&results), ["manifesto-null.md"]);
        // And `describe()`'s `?? 'documents'` names it.
        assert_eq!(results[0].store_type, "documents");
    }

    /// Caller-supplied exclusions stack on top of the archived sweep.
    #[test]
    fn caller_exclusions_are_honoured() {
        let mount = mount_db();
        store(&mount, "mp-a", "A", Some("documents"));
        store(&mount, "mp-b", "B", Some("documents"));
        doc(
            &mount,
            "l-a",
            "mp-a",
            "manifesto-a.md",
            "2026-01-02T00:00:00.000Z",
        );
        doc(
            &mount,
            "l-b",
            "mp-b",
            "manifesto-b.md",
            "2026-01-01T00:00:00.000Z",
        );
        let main = main_db_with_characters();
        let (results, total) = search_document_text(
            &main,
            &mount,
            "manifesto",
            &DocumentTextSearchOptions {
                limit: None,
                exclude_mount_point_ids: vec!["mp-a".to_string()],
            },
        );
        assert_eq!(names(&results), ["manifesto-b.md"]);
        assert_eq!(total, 1);
    }

    /// `limit` slices; `totalCount` reports the WHOLE merged set (itself bounded
    /// by the scan cap — v4's own doc comment calls that out and this port keeps
    /// it).
    #[test]
    fn limit_slices_but_total_count_does_not() {
        let mount = mount_db();
        store(&mount, "mp-a", "A", Some("documents"));
        for (i, stamp) in ["2026-01-03", "2026-01-02", "2026-01-01"]
            .iter()
            .enumerate()
        {
            doc(
                &mount,
                &format!("l-{i}"),
                "mp-a",
                &format!("manifesto-{i}.md"),
                &format!("{stamp}T00:00:00.000Z"),
            );
        }
        let main = main_db_with_characters();
        let (results, total) = search_document_text(
            &main,
            &mount,
            "manifesto",
            &DocumentTextSearchOptions {
                limit: Some(2),
                exclude_mount_point_ids: Vec::new(),
            },
        );
        assert_eq!(results.len(), 2);
        assert_eq!(total, 3);
    }

    /// A hit whose store isn't in scope is silently dropped rather than
    /// crashing the search (v4's `if (!store) continue`). Only reachable when a
    /// link points at a mount point row that no longer exists.
    #[test]
    fn a_hit_in_an_unknown_store_is_dropped() {
        let mount = mount_db();
        store(&mount, "mp-a", "A", Some("documents"));
        doc(
            &mount,
            "l-a",
            "mp-a",
            "manifesto-a.md",
            "2026-01-02T00:00:00.000Z",
        );
        doc(
            &mount,
            "l-gone",
            "mp-gone",
            "manifesto-gone.md",
            "2026-01-01T00:00:00.000Z",
        );
        let main = main_db_with_characters();
        assert_eq!(
            names(&search(&main, &mount, "manifesto")),
            ["manifesto-a.md"]
        );
    }

    // ── buildContentSnippet ────────────────────────────────────────────────

    #[test]
    fn snippet_leads_by_one_third_not_one_half() {
        // A 7-unit query: lead = floor((200 - 7) / 3) = 64, NOT 96.
        // The filler is `q` — an `a` would run straight into "airship".
        let content = format!("{}airship{}", "q".repeat(100), "b".repeat(200));
        let s = build_content_snippet(&content, "airship", None);
        assert!(s.starts_with("...") && s.ends_with("..."));
        let body = s.trim_start_matches("...").trim_end_matches("...");
        // EXACTLY 64 units of lead-in before the match — a `/2` lead would put
        // 96 here, and a `/1` 193. Counting the run is what makes this
        // discriminating: any lead ≥ 64 still *starts with* 64 `a`s.
        assert_eq!(body.chars().take_while(|c| *c == 'q').count(), 64);
        assert_eq!(utf16_len(body), 200);
    }

    #[test]
    fn snippet_trims_before_it_ellipsizes() {
        // The window opens EXACTLY on a space (lead 64 from a match at index
        // 74, and index 10 is a space). `.trim()` runs BEFORE the ellipses are
        // added, so the leading `...` butts straight against the text instead
        // of leaving `... ` behind.
        let content = format!(
            "{} {}airship trails on and on{}",
            "z".repeat(10),
            "z".repeat(63),
            "y".repeat(300)
        );
        assert_eq!(content.find("airship"), Some(74));
        let s = build_content_snippet(&content, "airship", None);
        assert!(s.starts_with("...z"), "got {:?}", &s[..12.min(s.len())]);
        assert!(
            !s.starts_with("... "),
            "the trim must run before the ellipsis"
        );
    }

    #[test]
    fn snippet_prefixes_a_heading_with_an_em_dash() {
        assert_eq!(
            build_content_snippet("the airship", "airship", Some("Chapter II")),
            "Chapter II — the airship"
        );
        // An EMPTY heading is JS-falsy — no prefix, no separator.
        assert_eq!(
            build_content_snippet("the airship", "airship", Some("")),
            "the airship"
        );
        assert_eq!(
            build_content_snippet("the airship", "airship", None),
            "the airship"
        );
    }

    #[test]
    fn snippet_with_no_match_starts_at_zero() {
        let content = "q".repeat(400);
        let s = build_content_snippet(&content, "airship", None);
        assert!(!s.starts_with("..."));
        assert!(s.ends_with("..."));
        assert_eq!(utf16_len(&s), 203);
    }

    #[test]
    fn a_query_longer_than_the_window_clamps_the_lead_to_zero() {
        // (200 - 400) / 3 is negative; `Math.max(0, …)` clamps it, so `start`
        // is the match index itself.
        let needle = "n".repeat(400);
        let content = format!("{}{needle}", "m".repeat(50));
        let s = build_content_snippet(&content, &needle, None);
        assert!(s.starts_with("...n"), "got {}", &s[..12.min(s.len())]);
    }
}
