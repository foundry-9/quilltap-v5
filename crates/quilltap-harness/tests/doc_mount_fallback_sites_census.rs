//! P4.131 — the document-store repository-read census, as a guard.
//!
//! v4's three document-store repositories read through FALLBACKS: a failed read
//! logs the repository's ERROR and answers "nothing" (`[]` / `null` / `false`),
//! it never throws — `docMountFileLinks.findByMountPointId` /
//! `findByMountPointAndPath` over the fallback `queryJoined`,
//! `docMountDocuments.findByMountPointAndPath` over a fallback `withRawDb(null)`,
//! `docMountFolders.findByMountPointId` / `findByMountPointAndPath` over
//! `findByFilter` / `findOneByFilter`, and `docMountFileLinks.deleteWithGC` over a
//! fallback `withRawDb`. The ONE exception is `withStrictRepositoryFailures`
//! (v4 bug 79, `strict-failures.ts`), entered only by the importer
//! (`quilltap-import/execute.ts:430`, `preview.ts:31`).
//!
//! v5's repository fns PROPAGATE (and stay so — the importer's strict reads need
//! them), with fallback TWINS beside each (`…_or_none` / `…_or_empty` /
//! `…_or_false`, all over `db::fallback`). The store layer (`database_store.rs`)
//! and the mail wrappers take the twins. **This census is the measured list of
//! every OTHER direct call site**, so the next order converts from a list, not a
//! guess. It is a tripwire: one row per call, `(file, the nearest preceding fn,
//! method, class)`, in source order, and it fails when any row moves — a new
//! caller must CHOOSE (twin or propagating) and record the choice here; a row
//! going away is also a failure; a SWAP of two sites' classes inside one file
//! reddens because every row carries its own class.
//!
//! **Classes.**
//!
//! - `converted` — the call is a twin (`…_or_none` / `…_or_empty` /
//!   `…_or_false`): v4's fallback, v4's line.
//! - `internal` — the repository's own composition inside `db/doc_mount_
//!   {documents,file_links,folders}.rs` (a twin's closure over its propagating
//!   sibling; the folders' exact-then-scan).
//! - `other-repo` — a `DocMountBlobsRepository` read: a different repository,
//!   not part of this order.
//! - `strict-in-v4 (import)` — a site under `services/quilltap_import/`, where v4
//!   runs strict.
//! - `swallowed-by-other-means` — the call is NOT followed by `?`: the site
//!   matches the `Result`, `.ok()`s it, `let Ok(..) else`s it, or hands it up to a
//!   caller that does. Not a v5 propagation at THIS site.
//! - `no-v4-counterpart` — a v5-only read with no v4 line to match (one row, named
//!   in [`OVERRIDES`]).
//! - `fallback-in-v4` — the call is followed by `?` (v5 propagates) and the v4
//!   counterpart is a fallback read outside the strict scope: THE CONVERSION
//!   LIST. `services/mount_index/sync/apply_store.rs` is in it by measurement
//!   (v4's `apply-store.ts:64-207` reads are not strict) but is ESCALATED — a
//!   data-safety question of bug 79's kind, for the human's ruling, unchanged
//!   this round. `api/chat_media.rs` (`:2317`) and `db/group_doc_mount_links.rs`
//!   are another lane's (P4.130) and recorded here, not touched.
//!
//! **Scanner.** The shared census lexer (`source_census`): test items stripped,
//! comments and string literals blanked, so a why-comment naming a method is not
//! a call and a test module's calls are not counted. A call is `.<method>(` or
//! `::<method>(` where `<method>` is one of [`METHODS`] (or its twin suffixes);
//! the definitions (`fn <method>(`) are declarations. The receiver kind is read
//! off the statement text before the call (`Blobs`/`blobs` ⇒ `other-repo`).
//!
//! Run: `cargo test -p quilltap-harness --test doc_mount_fallback_sites_census`.
//! Set `QT_CENSUS_PRINT=<file>` to write the measured rows there (for pasting
//! into EXPECTED).

mod source_census;

use std::path::{Path, PathBuf};

use source_census::{code_only, production_zone, workspace_rust_sources};

/// The four propagating repository methods the census follows (and, by suffix,
/// their twins).
const METHODS: [&str; 4] = [
    "find_by_mount_point_and_path",
    "find_by_mount_point_id",
    "find_content_and_mtime_by_mount_point_and_path",
    "delete_with_gc",
];

const TWIN_SUFFIXES: [&str; 3] = ["_or_none", "_or_empty", "_or_false"];

#[derive(Clone, Debug, PartialEq, Eq)]
struct Row {
    file: String,
    func: String,
    method: String,
    class: String,
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("the harness crate sits two levels under the repo root")
        .to_path_buf()
}

fn is_ident(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Byte index of the `)` closing the `(` at `open`.
fn closing_paren(code: &str, open: usize) -> usize {
    let bytes = code.as_bytes();
    let mut depth = 0i32;
    for (i, &b) in bytes.iter().enumerate().skip(open) {
        match b {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return i;
                }
            }
            _ => {}
        }
    }
    panic!("unbalanced parens");
}

/// The nearest `fn <ident>` declaration above `at`.
fn enclosing_fn(code: &str, at: usize) -> String {
    let head = &code[..at];
    let mut best = String::from("<module>");
    let mut from = 0usize;
    while let Some(i) = head[from..].find("fn ") {
        let abs = from + i;
        let before_ok = abs == 0 || !is_ident(head.as_bytes()[abs - 1]);
        if before_ok {
            let rest = &head[abs + 3..];
            let name: String = rest
                .trim_start()
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect();
            if !name.is_empty() {
                best = name;
            }
        }
        from = abs + 3;
    }
    best
}

/// The statement text before `at` — back to the previous `;`, `{` or `}`.
fn statement_head(code: &str, at: usize) -> &str {
    let head = &code[..at];
    let start = head.rfind([';', '{', '}']).map(|i| i + 1).unwrap_or(0);
    &head[start..]
}

/// The receiver of the call whose `.`/`::` sits at `dot`: the identifier just
/// before it, or — when that is a `)` — the path token before the matching
/// `(` (`DocMountBlobsRepository::new(c).find…` ⇒ `DocMountBlobsRepository::new`).
fn receiver(code: &str, dot: usize) -> String {
    let bytes = code.as_bytes();
    let mut end = dot;
    while end > 0 && bytes[end - 1].is_ascii_whitespace() {
        end -= 1;
    }
    if end > 0 && bytes[end - 1] == b')' {
        let mut depth = 0i32;
        let mut k = end;
        while k > 0 {
            k -= 1;
            match bytes[k] {
                b')' => depth += 1,
                b'(' => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                _ => {}
            }
        }
        end = k;
        while end > 0 && bytes[end - 1].is_ascii_whitespace() {
            end -= 1;
        }
    }
    let mut start = end;
    while start > 0 && (is_ident(bytes[start - 1]) || bytes[start - 1] == b':') {
        start -= 1;
    }
    code[start..end].to_string()
}

/// Sites whose class the scanner cannot read off the text, with the reason.
const OVERRIDES: &[(&str, &str, &str, &str)] = &[
    // v5's `write_database_document` re-reads the stored row to return the
    // `mtime` it actually stored (a spurious-CONFLICT fix for open→write round
    // trips). v4 has NO such read — it returns `new Date(now).getTime()` — so
    // there is no v4 line to match, and the read stays propagating: it runs only
    // after a successful link + reindex.
    (
        "quilltap-core/src/db/database_store.rs",
        "write_database_document",
        "find_content_and_mtime_by_mount_point_and_path",
        "no-v4-counterpart",
    ),
];

fn classify(
    rel: &str,
    func: &str,
    method: &str,
    recv: &str,
    after: &str,
    head: &str,
) -> &'static str {
    if let Some((_, _, _, class)) = OVERRIDES
        .iter()
        .find(|(f, n, m, _)| *f == rel && *n == func && *m == method)
    {
        return class;
    }
    if TWIN_SUFFIXES.iter().any(|s| method.ends_with(s)) {
        return "converted";
    }
    if matches!(
        rel,
        "quilltap-core/src/db/doc_mount_documents.rs"
            | "quilltap-core/src/db/doc_mount_file_links.rs"
            | "quilltap-core/src/db/doc_mount_folders.rs"
    ) {
        return "internal";
    }
    if recv.to_lowercase().contains("blobs") {
        return "other-repo";
    }
    if rel.starts_with("quilltap-core/src/services/quilltap_import/") {
        return "strict-in-v4 (import)";
    }
    if after.starts_with('?') {
        return "fallback-in-v4";
    }
    // No `?` here: the site swallows the `Result` itself (`.ok()`, a defaulting
    // adapter, a `match` / `let Ok … else`) — or hands it up, which is still a
    // v5 propagation and stays on the conversion list.
    let h = head.trim_start();
    let swallows = [".ok()", ".unwrap_or", ".is_ok", ".is_err", ".map_or"]
        .iter()
        .any(|a| after.starts_with(a))
        || h.starts_with("match ")
        || h.contains(" match ")
        || h.starts_with("let Ok(")
        || h.starts_with("if let Ok(")
        || h.contains("let Ok(")
        || h.contains("Ok(Some(");
    if swallows {
        "swallowed-by-other-means"
    } else {
        "fallback-in-v4"
    }
}

fn scan_file(root: &Path, path: &Path) -> Vec<Row> {
    let src = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
    let code = code_only(&production_zone(&src));
    let rel = path
        .strip_prefix(root.join("crates"))
        .unwrap()
        .to_string_lossy()
        .replace('\\', "/");
    let mut rows = Vec::new();
    let bytes = code.as_bytes();
    let mut i = 0usize;
    while i < code.len() {
        // A method-name token preceded by `.` or `::` and followed by `(`.
        if bytes[i] == b'.' || (bytes[i] == b':' && i > 0 && bytes[i - 1] == b':') {
            let start = i + 1;
            let mut end = start;
            while end < code.len() && is_ident(bytes[end]) {
                end += 1;
            }
            let name = &code[start..end];
            let base = TWIN_SUFFIXES
                .iter()
                .find_map(|s| name.strip_suffix(s))
                .unwrap_or(name);
            if METHODS.contains(&base) && bytes.get(end) == Some(&b'(') {
                let close = closing_paren(&code, end);
                let after = code[close + 1..].trim_start();
                let head = statement_head(&code, i);
                let func = enclosing_fn(&code, i);
                let class = classify(&rel, &func, name, &receiver(&code, i), after, head);
                rows.push(Row {
                    file: rel.clone(),
                    func,
                    method: name.to_string(),
                    class: class.to_string(),
                });
                i = end;
                continue;
            }
        }
        i += 1;
    }
    rows
}

fn measured() -> Vec<Row> {
    let root = repo_root();
    let mut files = Vec::new();
    for krate in [
        "quilltap-core",
        "quilltap-web",
        "quilltap-host",
        "quilltap-cli",
    ] {
        let dir = root.join("crates").join(krate).join("src");
        if dir.is_dir() {
            workspace_rust_sources(&dir, &mut files);
        }
    }
    files.sort();
    files.iter().flat_map(|f| scan_file(&root, f)).collect()
}

fn render(rows: &[Row]) -> String {
    rows.iter()
        .map(|r| {
            format!(
                "    (\"{}\", \"{}\", \"{}\", \"{}\"),",
                r.file, r.func, r.method, r.class
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Every direct call site, in SOURCE ORDER per file (files sorted by path):
/// `(file under crates/, nearest preceding fn, method, class)`.
#[rustfmt::skip]
const EXPECTED: &[(&str, &str, &str, &str)] = &[
    ("quilltap-core/src/api/characters.rs", "character_depiction_guidelines", "find_by_mount_point_and_path", "swallowed-by-other-means"),
    ("quilltap-core/src/api/characters.rs", "character_stats", "find_by_mount_point_id", "fallback-in-v4"),
    ("quilltap-core/src/api/chat_media.rs", "chat_attach_mount_file", "find_by_mount_point_and_path", "fallback-in-v4"),
    ("quilltap-core/src/api/chat_media.rs", "chat_attach_mount_file", "find_by_mount_point_and_path", "other-repo"),
    ("quilltap-core/src/api/mount_files.rs", "mount_file_update", "find_by_mount_point_and_path", "other-repo"),
    ("quilltap-core/src/api/mount_files.rs", "mount_blob_update", "find_by_mount_point_and_path", "other-repo"),
    ("quilltap-core/src/api/projects.rs", "project_file_list", "find_by_mount_point_id", "fallback-in-v4"),
    ("quilltap-core/src/api/scenarios.rs", "create_op", "find_by_mount_point_and_path", "fallback-in-v4"),
    ("quilltap-core/src/api/scenarios.rs", "update_op", "find_by_mount_point_and_path", "fallback-in-v4"),
    ("quilltap-core/src/api/scenarios.rs", "rename_op", "find_by_mount_point_and_path", "fallback-in-v4"),
    ("quilltap-core/src/api/scenarios.rs", "rename_op", "find_by_mount_point_and_path", "fallback-in-v4"),
    ("quilltap-core/src/db/character_vault.rs", "ensure_character_metadata_file", "find_by_mount_point_and_path", "fallback-in-v4"),
    ("quilltap-core/src/db/database_store.rs", "read_database_document", "find_content_and_mtime_by_mount_point_and_path_or_none", "converted"),
    ("quilltap-core/src/db/database_store.rs", "write_database_document", "find_by_mount_point_and_path_or_none", "converted"),
    ("quilltap-core/src/db/database_store.rs", "write_database_document", "find_content_and_mtime_by_mount_point_and_path", "no-v4-counterpart"),
    ("quilltap-core/src/db/database_store.rs", "move_database_document", "find_by_mount_point_and_path_or_none", "converted"),
    ("quilltap-core/src/db/database_store.rs", "move_database_document", "find_by_mount_point_and_path_or_none", "converted"),
    ("quilltap-core/src/db/database_store.rs", "move_database_document", "find_by_mount_point_and_path_or_none", "converted"),
    ("quilltap-core/src/db/database_store.rs", "list_database_files", "find_by_mount_point_id_or_empty", "converted"),
    ("quilltap-core/src/db/database_store.rs", "list_database_files", "find_by_mount_point_id_or_empty", "converted"),
    ("quilltap-core/src/db/database_store.rs", "delete_database_folder", "find_by_mount_point_and_path_or_none", "converted"),
    ("quilltap-core/src/db/database_store.rs", "move_database_folder", "find_by_mount_point_and_path_or_none", "converted"),
    ("quilltap-core/src/db/database_store.rs", "move_database_folder", "find_by_mount_point_and_path_or_none", "converted"),
    ("quilltap-core/src/db/database_store.rs", "move_database_folder", "find_by_mount_point_and_path_or_none", "converted"),
    ("quilltap-core/src/db/database_store.rs", "move_database_folder", "find_by_mount_point_id_or_empty", "converted"),
    ("quilltap-core/src/db/database_store.rs", "move_database_folder", "find_by_mount_point_id_or_empty", "converted"),
    ("quilltap-core/src/db/database_store.rs", "move_database_folder", "find_by_mount_point_and_path_or_none", "converted"),
    ("quilltap-core/src/db/database_store.rs", "database_document_exists", "find_by_mount_point_and_path_or_none", "converted"),
    ("quilltap-core/src/db/database_store.rs", "database_folder_exists", "find_by_mount_point_and_path_or_none", "converted"),
    ("quilltap-core/src/db/database_store.rs", "folder_has_contents", "find_by_mount_point_id_or_empty", "converted"),
    ("quilltap-core/src/db/database_store.rs", "folder_has_contents", "find_by_mount_point_id_or_empty", "converted"),
    ("quilltap-core/src/db/doc_mount_blobs.rs", "create_with_ids", "find_by_mount_point_and_path", "fallback-in-v4"),
    ("quilltap-core/src/db/doc_mount_documents.rs", "find_by_mount_point_and_path_or_none", "find_by_mount_point_and_path", "internal"),
    ("quilltap-core/src/db/doc_mount_documents.rs", "find_content_and_mtime_by_mount_point_and_path_or_none", "find_content_and_mtime_by_mount_point_and_path", "internal"),
    ("quilltap-core/src/db/doc_mount_file_links.rs", "delete_database_document", "find_by_mount_point_and_path_or_none", "converted"),
    ("quilltap-core/src/db/doc_mount_file_links.rs", "delete_database_document", "delete_with_gc_or_false", "converted"),
    ("quilltap-core/src/db/doc_mount_file_links.rs", "find_by_mount_point_id_or_empty", "find_by_mount_point_id", "internal"),
    ("quilltap-core/src/db/doc_mount_file_links.rs", "find_by_mount_point_and_path_or_none", "find_by_mount_point_and_path", "internal"),
    ("quilltap-core/src/db/doc_mount_file_links.rs", "delete_with_gc_or_false", "delete_with_gc_or_false", "converted"),
    ("quilltap-core/src/db/doc_mount_file_links.rs", "delete_with_gc_or_false", "delete_with_gc", "internal"),
    ("quilltap-core/src/db/doc_mount_folders.rs", "find_by_mount_point_and_path", "find_by_mount_point_id", "internal"),
    ("quilltap-core/src/db/doc_mount_folders.rs", "find_by_mount_point_and_path_or_none", "find_by_mount_point_id_or_empty", "converted"),
    ("quilltap-core/src/db/doc_mount_folders.rs", "find_by_mount_point_id_or_empty", "find_by_mount_point_id", "internal"),
    ("quilltap-core/src/db/document_store_overlay.rs", "read_properties", "find_by_mount_point_and_path", "fallback-in-v4"),
    ("quilltap-core/src/db/vault_character_update.rs", "read_current_properties", "find_by_mount_point_and_path", "fallback-in-v4"),
    ("quilltap-core/src/documents/mod.rs", "classify_resolved_target", "find_by_mount_point_and_path", "fallback-in-v4"),
    ("quilltap-core/src/photos/avatar_rolls_service.rs", "delete_avatar_roll", "delete_with_gc", "fallback-in-v4"),
    ("quilltap-core/src/photos/character_gallery_service.rs", "list_character_gallery", "find_by_mount_point_id", "fallback-in-v4"),
    ("quilltap-core/src/photos/character_gallery_service.rs", "remove_from_character_gallery", "delete_with_gc", "fallback-in-v4"),
    ("quilltap-core/src/photos/chat_gallery.rs", "add_blob_reference", "find_by_mount_point_and_path", "swallowed-by-other-means"),
    ("quilltap-core/src/photos/save_image_to_album.rs", "find_existing_photos_link_by_sha", "find_by_mount_point_id", "fallback-in-v4"),
    ("quilltap-core/src/photos/save_image_to_album.rs", "resolve_unique_relative_path", "find_by_mount_point_and_path", "other-repo"),
    ("quilltap-core/src/photos/save_image_to_album.rs", "resolve_unique_relative_path", "find_by_mount_point_and_path", "other-repo"),
    ("quilltap-core/src/photos/user_gallery_service.rs", "list_user_gallery", "find_by_mount_point_id", "fallback-in-v4"),
    ("quilltap-core/src/photos/user_gallery_service.rs", "remove_from_user_gallery", "delete_with_gc", "fallback-in-v4"),
    ("quilltap-core/src/services/aesthetics.rs", "read_store_file_internal", "find_by_mount_point_and_path", "swallowed-by-other-means"),
    ("quilltap-core/src/services/character_archive/service.rs", "prune_vault", "find_by_mount_point_id", "fallback-in-v4"),
    ("quilltap-core/src/services/character_archive/service.rs", "prune_vault", "delete_with_gc", "fallback-in-v4"),
    ("quilltap-core/src/services/character_archive/service.rs", "prune_vault", "find_by_mount_point_id", "fallback-in-v4"),
    ("quilltap-core/src/services/character_archive/service.rs", "prune_empty_folders", "find_by_mount_point_id", "fallback-in-v4"),
    ("quilltap-core/src/services/file_storage.rs", "delete_mount_blob_conn", "delete_with_gc", "fallback-in-v4"),
    ("quilltap-core/src/services/file_storage.rs", "resolve_unique_relative_path", "find_by_mount_point_and_path", "other-repo"),
    ("quilltap-core/src/services/file_storage.rs", "resolve_unique_relative_path", "find_by_mount_point_and_path", "other-repo"),
    ("quilltap-core/src/services/file_storage.rs", "store_mount_blob", "find_by_mount_point_and_path", "fallback-in-v4"),
    ("quilltap-core/src/services/image_job_storage.rs", "write_main_avatar_to_vault", "find_by_mount_point_and_path", "fallback-in-v4"),
    ("quilltap-core/src/services/image_job_storage.rs", "write_main_avatar_to_vault", "delete_with_gc", "fallback-in-v4"),
    ("quilltap-core/src/services/image_job_storage.rs", "resolve_unique_relative_path", "find_by_mount_point_and_path", "other-repo"),
    ("quilltap-core/src/services/image_job_storage.rs", "resolve_unique_relative_path", "find_by_mount_point_and_path", "other-repo"),
    ("quilltap-core/src/services/knowledge_injector.rs", "retrieve_knowledge_for_turn", "find_by_mount_point_and_path", "fallback-in-v4"),
    ("quilltap-core/src/services/librarian_notifications.rs", "document_hidden_from_characters", "find_by_mount_point_and_path", "fallback-in-v4"),
    ("quilltap-core/src/services/memory_processor.rs", "read_vault_text_file_conn", "find_by_mount_point_and_path", "swallowed-by-other-means"),
    ("quilltap-core/src/services/mount_index/embedding_scheduler.rs", "enqueue_embedding_jobs_for_mount_point", "find_by_mount_point_id_or_empty", "converted"),
    ("quilltap-core/src/services/mount_index/file_ops.rs", "source_exists_or_throw", "find_by_mount_point_and_path", "fallback-in-v4"),
    ("quilltap-core/src/services/mount_index/file_ops.rs", "dest_exists", "find_by_mount_point_and_path", "fallback-in-v4"),
    ("quilltap-core/src/services/mount_index/file_ops.rs", "compute_dest_sha256", "find_by_mount_point_and_path", "fallback-in-v4"),
    ("quilltap-core/src/services/mount_index/file_ops.rs", "delete_at_source", "delete_with_gc", "fallback-in-v4"),
    ("quilltap-core/src/services/mount_index/file_ops.rs", "delete_at_source", "delete_with_gc", "fallback-in-v4"),
    ("quilltap-core/src/services/mount_index/file_ops.rs", "delete_at_dest", "find_by_mount_point_and_path", "fallback-in-v4"),
    ("quilltap-core/src/services/mount_index/file_ops.rs", "delete_at_dest", "delete_with_gc", "fallback-in-v4"),
    ("quilltap-core/src/services/mount_index/file_ops.rs", "move_file", "delete_with_gc", "fallback-in-v4"),
    ("quilltap-core/src/services/mount_index/file_ops.rs", "link_file", "find_by_mount_point_and_path", "swallowed-by-other-means"),
    ("quilltap-core/src/services/mount_index/folder_ops.rs", "move_folder", "find_by_mount_point_id", "fallback-in-v4"),
    ("quilltap-core/src/services/mount_index/general_state.rs", "ensure_general_state_file", "find_by_mount_point_and_path", "fallback-in-v4"),
    ("quilltap-core/src/services/mount_index/link_groups.rs", "reindex_inner", "find_by_mount_point_and_path", "fallback-in-v4"),
    ("quilltap-core/src/services/mount_index/list.rs", "mount_files_list", "find_by_mount_point_id", "fallback-in-v4"),
    ("quilltap-core/src/services/mount_index/read_file.rs", "read_mount_file_bytes_conn", "find_by_mount_point_and_path", "fallback-in-v4"),
    ("quilltap-core/src/services/mount_index/read_file.rs", "read_mount_file_bytes_conn", "find_by_mount_point_and_path", "fallback-in-v4"),
    ("quilltap-core/src/services/mount_index/read_file.rs", "read_mount_file_bytes_conn", "find_by_mount_point_and_path", "other-repo"),
    ("quilltap-core/src/services/mount_index/read_file.rs", "read_mount_file", "find_by_mount_point_and_path", "fallback-in-v4"),
    ("quilltap-core/src/services/mount_index/reindex.rs", "reindex_links", "find_by_mount_point_id", "fallback-in-v4"),
    ("quilltap-core/src/services/mount_index/reindex.rs", "enqueue_embedding_jobs_scoped", "find_by_mount_point_id", "fallback-in-v4"),
    ("quilltap-core/src/services/mount_index/reindex_file.rs", "reindex_inner", "find_content_and_mtime_by_mount_point_and_path", "fallback-in-v4"),
    ("quilltap-core/src/services/mount_index/reindex_file.rs", "reindex_inner", "find_by_mount_point_and_path", "other-repo"),
    ("quilltap-core/src/services/mount_index/reindex_file.rs", "reindex_inner", "find_by_mount_point_and_path", "fallback-in-v4"),
    ("quilltap-core/src/services/mount_index/scanner.rs", "process_mount_file", "find_by_mount_point_and_path", "fallback-in-v4"),
    ("quilltap-core/src/services/mount_index/scanner.rs", "remove_mount_file", "find_by_mount_point_and_path", "fallback-in-v4"),
    ("quilltap-core/src/services/mount_index/scanner.rs", "remove_mount_file", "delete_with_gc", "fallback-in-v4"),
    ("quilltap-core/src/services/mount_index/scanner.rs", "rescan_database_mount_point", "find_by_mount_point_id", "fallback-in-v4"),
    ("quilltap-core/src/services/mount_index/store_file.rs", "resolve_unique_relative_path", "find_by_mount_point_and_path", "other-repo"),
    ("quilltap-core/src/services/mount_index/store_file.rs", "resolve_unique_relative_path", "find_by_mount_point_and_path", "other-repo"),
    ("quilltap-core/src/services/mount_index/store_file.rs", "store_mount_file", "find_by_mount_point_and_path", "fallback-in-v4"),
    ("quilltap-core/src/services/mount_index/store_file.rs", "store_mount_file", "find_content_and_mtime_by_mount_point_and_path", "fallback-in-v4"),
    ("quilltap-core/src/services/mount_index/store_file.rs", "store_mount_file", "find_by_mount_point_and_path", "fallback-in-v4"),
    ("quilltap-core/src/services/mount_index/store_file.rs", "store_mount_file", "find_by_mount_point_and_path", "fallback-in-v4"),
    ("quilltap-core/src/services/mount_index/sync/apply_store.rs", "assert_unchanged", "find_by_mount_point_and_path", "fallback-in-v4"),
    ("quilltap-core/src/services/mount_index/sync/apply_store.rs", "write_store_file", "find_by_mount_point_and_path", "fallback-in-v4"),
    ("quilltap-core/src/services/mount_index/sync/apply_store.rs", "read_store_bytes", "find_by_mount_point_and_path", "fallback-in-v4"),
    ("quilltap-core/src/services/mount_index/sync/apply_store.rs", "apply_store_action", "find_by_mount_point_and_path", "fallback-in-v4"),
    ("quilltap-core/src/services/mount_index/sync/apply_store.rs", "apply_store_action", "delete_with_gc", "fallback-in-v4"),
    ("quilltap-core/src/services/qtap_export/records.rs", "stream_one_store", "find_by_mount_point_id", "fallback-in-v4"),
    ("quilltap-core/src/tools/doc_edit/blob.rs", "handle_read_blob", "find_by_mount_point_and_path", "swallowed-by-other-means"),
    ("quilltap-core/src/tools/doc_edit/shared.rs", "document_hidden_from_characters", "find_by_mount_point_and_path", "swallowed-by-other-means"),
    ("quilltap-core/src/tools/doc_edit/shared.rs", "assert_character_may_read", "find_by_mount_point_and_path", "swallowed-by-other-means"),
    ("quilltap-core/src/tools/doc_edit/shared.rs", "assert_character_may_write", "find_by_mount_point_and_path", "swallowed-by-other-means"),
    ("quilltap-core/src/tools/doc_edit/shared.rs", "get_character_blocked_read_paths", "find_by_mount_point_id", "swallowed-by-other-means"),
    ("quilltap-core/src/tools/doc_edit/shared.rs", "assert_folder_has_no_write_protected_descendants", "find_by_mount_point_id", "swallowed-by-other-means"),
    ("quilltap-core/src/tools/generate_image.rs", "resolve_unique_relative_path", "find_by_mount_point_and_path", "other-repo"),
    ("quilltap-core/src/tools/generate_image.rs", "resolve_unique_relative_path", "find_by_mount_point_and_path", "other-repo"),
    ("quilltap-core/src/tools/photo.rs", "handle_list_images", "find_by_mount_point_id", "swallowed-by-other-means"),
    ("quilltap-core/src/tools/photo.rs", "semantic_branch", "find_by_mount_point_and_path", "swallowed-by-other-means"),
    ("quilltap-core/src/tools/photo.rs", "find_existing_photos_link_by_sha", "find_by_mount_point_id", "fallback-in-v4"),
    ("quilltap-web/src/files_routes.rs", "mount_file_get", "find_by_mount_point_and_path", "fallback-in-v4"),
    ("quilltap-web/src/files_routes.rs", "mount_file_get", "find_by_mount_point_and_path", "fallback-in-v4"),
    ("quilltap-web/src/files_routes.rs", "mount_file_get", "find_by_mount_point_and_path", "other-repo"),
    ("quilltap-web/src/files_routes.rs", "mount_blob_get", "find_by_mount_point_and_path", "other-repo"),
    ("quilltap-web/src/files_routes.rs", "mount_blob_get", "find_by_mount_point_and_path", "fallback-in-v4"),
    ("quilltap-web/src/files_routes.rs", "mount_blob_get", "find_by_mount_point_and_path", "fallback-in-v4"),
    ("quilltap-web/src/qtap_target_route.rs", "qtap_target_get", "find_by_mount_point_and_path", "fallback-in-v4"),
    ("quilltap-web/src/qtap_target_route.rs", "qtap_target_get", "find_by_mount_point_and_path", "fallback-in-v4"),
    ("quilltap-web/src/qtap_target_route.rs", "qtap_target_get", "find_by_mount_point_and_path", "other-repo"),
];

#[test]
fn every_document_store_read_site_is_classified() {
    let got = measured();
    if let Ok(out) = std::env::var("QT_CENSUS_PRINT") {
        std::fs::write(&out, render(&got)).expect("write the measured rows");
    }
    let want: Vec<Row> = EXPECTED
        .iter()
        .map(|(file, func, method, class)| Row {
            file: file.to_string(),
            func: func.to_string(),
            method: method.to_string(),
            class: class.to_string(),
        })
        .collect();
    assert_eq!(
        got,
        want,
        "a document-store read site moved — classify it (twin or propagating), then \
         update the EXPECTED table.\nmeasured rows:\n{}",
        render(&got)
    );
}

#[test]
fn the_class_counts_are_pinned() {
    let got = measured();
    let count = |class: &str| got.iter().filter(|r| r.class == class).count();
    // The arithmetic lives in the comment above COUNTS.
    assert_eq!(
        (
            count("converted"),
            count("internal"),
            count("other-repo"),
            count("strict-in-v4 (import)"),
            count("swallowed-by-other-means"),
            count("no-v4-counterpart"),
            count("fallback-in-v4"),
        ),
        COUNTS,
        "class counts moved (converted, internal, other-repo, strict-in-v4, swallowed, no-v4-counterpart, fallback-in-v4)"
    );
}

/// (converted, internal, other-repo, strict-in-v4 (import), swallowed-by-other-means,
/// no-v4-counterpart, fallback-in-v4), with the arithmetic:
///
/// - 130 direct call sites in all = 23 + 7 + 18 + 0 + 13 + 1 + 68. (The survey
///   counted ~101: it left the blobs repository and the twins out.)
/// - **converted 23** = `database_store.rs` 18 (read 1, write pre-read 1, move
///   document 3, list 2, delete folder 1, move folder 6, exists 2, has-contents 2)
///   + `doc_mount_file_links.rs` 3 (`delete_database_document`'s lookup and GC,
///     the twin's own wrapper) + `doc_mount_folders.rs` 1 (the folders' fallback
///     scan over its `_or_empty` sibling) + `embedding_scheduler.rs` 1.
/// - **internal 7** = each twin's closure over its propagating sibling (documents
///   2, links 3, folders 2).
/// - **other-repo 18** = `DocMountBlobsRepository` reads: `resolve_unique_
///   relative_path` ×2 in each of `file_storage`, `image_job_storage`,
///   `save_image_to_album`, `store_file`, `generate_image` (10), `api/chat_media`
///   1, `api/mount_files` 2, `read_file` 1, `reindex_file` 1, `web/files_routes`
///   2, `web/qtap_target_route` 1.
/// - **strict-in-v4 (import) 0** — the importer's strict reads are
///   `get_messages_strict` (P4.109), not these repositories; the class stays so a
///   future importer site must say so.
/// - **swallowed-by-other-means 13** = `.ok().flatten()` / a `match` arm /
///   `.unwrap_or_else` / `let Ok … else` at the site: `api/characters` 1,
///   `chat_gallery` 1, `aesthetics` 1, `memory_processor` 1, `file_ops::link_file`
///   1, `doc_edit/blob` 1, `doc_edit/shared` 5, `tools/photo` 2.
/// - **no-v4-counterpart 1** = `write_database_document`'s stored-mtime re-read
///   (see [`OVERRIDES`]).
/// - **fallback-in-v4 68** — THE CONVERSION LIST for the next order: v5
///   propagates, v4's repository falls back. Named sub-groups: the sync applier's
///   5 (`apply_store.rs`) are ESCALATED for the human's ruling (v4's
///   `apply-store.ts:64-207` reads are not strict); `api/chat_media.rs:2317`
///   (P4.130's file) and the character overlay's reads
///   (`document_store_overlay.rs`, `db/character_vault.rs`,
///   `vault_character_update.rs`) — the last being why `send_mail` over a broken
///   links table throws at the recipient resolve where v4 fails soft (the
///   recorded divergence in `mail_carina_tools_equivalence`).
const COUNTS: (usize, usize, usize, usize, usize, usize, usize) = (23, 7, 18, 0, 13, 1, 68);

/// The scanner itself, on synthetic text: a twin is `converted`, a `?` is a
/// propagation, a match is swallowed, a blob receiver is another repo, and a
/// comment or string mention is not a call.
#[test]
fn the_scanner_reads_a_synthetic_source() {
    let code = code_only(&production_zone(
        r#"
fn a(c: &Connection) -> Result<(), DbError> {
    // links.find_by_mount_point_id(x)? in a comment is nothing
    let s = "links.find_by_mount_point_id(x)?";
    let l = links.find_by_mount_point_id(x)?;
    let m = match links.find_by_mount_point_and_path(x, y) { Ok(v) => v, Err(_) => None };
    let t = links.find_by_mount_point_id_or_empty(x);
    let b = blobs.find_by_mount_point_and_path(x, y)?;
    Ok(())
}
#[cfg(test)]
mod tests { fn t() { links.find_by_mount_point_id(x)?; } }
"#,
    ));
    let tmp = std::env::temp_dir().join(format!("qt-census-synth-{}.rs", std::process::id()));
    std::fs::write(&tmp, &code).unwrap();
    // scan_file strips relative to crates/, so call the pieces directly.
    let bytes = code.as_bytes();
    let mut found = Vec::new();
    for (i, _) in code.match_indices('.') {
        let start = i + 1;
        let mut end = start;
        while end < code.len() && is_ident(bytes[end]) {
            end += 1;
        }
        let name = &code[start..end];
        let base = TWIN_SUFFIXES
            .iter()
            .find_map(|s| name.strip_suffix(s))
            .unwrap_or(name);
        if METHODS.contains(&base) && bytes.get(end) == Some(&b'(') {
            let close = closing_paren(&code, end);
            let after = code[close + 1..].trim_start();
            let head = statement_head(&code, i);
            found.push((
                enclosing_fn(&code, i),
                name.to_string(),
                classify(
                    "quilltap-core/src/x.rs",
                    "a",
                    name,
                    &receiver(&code, i),
                    after,
                    head,
                ),
            ));
        }
    }
    let _ = std::fs::remove_file(&tmp);
    let names: Vec<(&str, &str)> = found.iter().map(|(_, m, c)| (m.as_str(), *c)).collect();
    assert_eq!(
        names,
        vec![
            ("find_by_mount_point_id", "fallback-in-v4"),
            ("find_by_mount_point_and_path", "swallowed-by-other-means"),
            ("find_by_mount_point_id_or_empty", "converted"),
            ("find_by_mount_point_and_path", "other-repo"),
        ]
    );
    assert!(found.iter().all(|(f, _, _)| f == "a"));
}
