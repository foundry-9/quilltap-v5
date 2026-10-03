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
//! - `strict-by-ruling` — v4 falls back, but a standing ruling keeps v5 STRICT
//!   (P4.142: the `.qtap` export's store read under the 2026-08-03 "fix, don't
//!   match" ruling — named in [`OVERRIDES`]).
//! - `held-pending-ruling` — v5's downstream mirrors v4's, but v4's fallback at
//!   that site destroys or misreports data on a failed read (an overwrite, a
//!   settings reset, a false delete success); v5 keeps propagating until the
//!   human rules (P4.142 — each row in [`OVERRIDES`] names its hazard).
//! - `handed(P4.144)` — not a row: a documentation table ([`HANDED`]) naming
//!   reads another lane of the same round converts; it checks only that each
//!   file exists.
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

/// The propagating repository methods the census follows (and, by suffix,
/// their twins). P4.131's four, plus P4.142's widening — the census had followed
/// four method NAMES, so it could not see the reads `send_mail` and the chat
/// list actually threw at (§R.4 (c)): the vault overlay's two BATCH document
/// reads (+ the `_opts` form) and the `doc_mount_chunks` reads (the two counts
/// are free fns, called path-qualified).
const METHODS: [&str; 12] = [
    "find_by_mount_point_and_path",
    "find_by_mount_point_id",
    "find_content_and_mtime_by_mount_point_and_path",
    "delete_with_gc",
    // P4.142 — the overlay's batch reads (`doc_mount_documents.rs`).
    "find_many_by_mount_points_and_path",
    "find_many_by_mount_points_in_folder",
    "find_many_by_mount_points_in_folder_opts",
    // P4.142 — the chunk reads (`doc_mount_chunks.rs`).
    "find_rows_by_mount_point_id",
    "find_ids_by_link_id",
    "find_row_by_id",
    "count_embedded_by_mount_point_ids",
    "count_nonempty_embeddings_by_mount_point_id",
];

/// Method names OTHER repositories share, counted only when the receiver names
/// this repository (or the call sits in its own file): `find_row_by_id` is also
/// `doc_mount_points`' and `conversation_chunks`' — `(method, receiver needle,
/// the repository's file)`.
const SCOPED_METHODS: &[(&str, &str, &str)] = &[(
    "find_row_by_id",
    "DocMountChunks",
    "quilltap-core/src/db/doc_mount_chunks.rs",
)];

const TWIN_SUFFIXES: [&str; 4] = ["_or_none", "_or_empty", "_or_false", "_or_zero"];

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
    // P4.142 (§R.4 item 2): the BLOBS repository calling its OWN method — the
    // receiver text is `self`, so the `blobs` receiver rule missed it and P4.131
    // filed it `fallback-in-v4` (the "68" that is 67).
    (
        "quilltap-core/src/db/doc_mount_blobs.rs",
        "create_with_ids",
        "find_by_mount_point_and_path",
        "other-repo",
    ),
    // P4.142: the two chunk reads whose WHOLE `read_mount_index` checkout sits
    // inside a `db::fallback` home closure (v4's `getCollection()` runs inside
    // the same `safeQuery`) — v4's fallback, v4's line, though the call text is
    // the propagating sibling's.
    (
        "quilltap-core/src/services/embedding_reindex_job.rs",
        "phase_mount_chunks",
        "find_rows_by_mount_point_id",
        "converted",
    ),
    (
        "quilltap-core/src/services/embedding_generate_job.rs",
        "mount_chunk_branch",
        "find_row_by_id",
        "converted",
    ),
    // P4.142 (G1): two read-only routes whose v4 read is the FILES repository's
    // `findByMountPointId` (`Error finding files by mount point ID`) or the
    // folders' — v5 hands the propagating read to the matching `db::fallback`
    // home (the folders' over the whole checkout).
    (
        "quilltap-core/src/api/projects.rs",
        "project_file_list",
        "find_by_mount_point_id",
        "converted",
    ),
    (
        "quilltap-core/src/services/mount_index/list.rs",
        "mount_files_list",
        "find_by_mount_point_id",
        "converted",
    ),
    // P4.142 G2: the chat attach route reads v4's FILES repository
    // (`docMountFiles.findByMountPointAndPath`) — the propagating links read sits
    // inside the `doc_mount_files` PATH home (`Error finding file by mount point
    // and path`).
    (
        "quilltap-core/src/api/chat_media.rs",
        "chat_attach_mount_file",
        "find_by_mount_point_and_path",
        "converted",
    ),
    // P4.142 G2/G3 — HELD PENDING A RULING (the human, 2026-10-02: "convert safe,
    // hold risky"). Each site's v5 downstream MIRRORS v4's on a failed read, and
    // that is the problem: v4's fallback there destroys or misreports data, and
    // v5's propagation refuses instead. Each row names the hazard; the lane
    // record writes them up beside the sync ruling.
    // a failed read is "absent" → the user's `metadata.json` is overwritten with the empty seed; v5 calls this on every FK `ensure_character_vault` AND from the importer's adopt arm, where v4 calls it from the boot backfill only.
    (
        "quilltap-core/src/db/character_vault.rs",
        "ensure_character_metadata_file",
        "find_by_mount_point_and_path",
        "held-pending-ruling",
    ),
    // a failed read is "absent" → the whole project/group settings bag is reseeded from schema defaults (v4 `dcd9440a` is incomplete outside the strict scope); importer-reachable (`reconcile.rs:493`).
    (
        "quilltap-core/src/db/document_store_overlay.rs",
        "read_properties",
        "find_by_mount_point_and_path",
        "held-pending-ruling",
    ),
    // a failed read is "absent" → all six vault properties reset to `empty_properties_default` — the loss bug 8 / dogfood #47 prevent; v4's refusal arm is unreachable outside the strict scope.
    (
        "quilltap-core/src/db/vault_character_update.rs",
        "read_current_properties",
        "find_by_mount_point_and_path",
        "held-pending-ruling",
    ),
    // a swallowed delete still deletes the `files` row and reports `deleted: true` — the roll's link + blob survive, invisible.
    (
        "quilltap-core/src/photos/avatar_rolls_service.rs",
        "delete_avatar_roll",
        "delete_with_gc",
        "held-pending-ruling",
    ),
    // the portrait pointers are cleared first; a swallowed delete then reports 200 `deleted: true` while the photo stays in the vault.
    (
        "quilltap-core/src/photos/character_gallery_service.rs",
        "remove_from_character_gallery",
        "delete_with_gc",
        "held-pending-ruling",
    ),
    // a failed dedup read saves a DUPLICATE album photo (a unique-suffixed name) where v5 fails the save.
    (
        "quilltap-core/src/photos/save_image_to_album.rs",
        "find_existing_photos_link_by_sha",
        "find_by_mount_point_id",
        "held-pending-ruling",
    ),
    // DIFFERS besides: v4 answers `deleted: result.fileId !== null` (→ 404 on the fallback); the bool twin cannot carry it.
    (
        "quilltap-core/src/photos/user_gallery_service.rs",
        "remove_from_user_gallery",
        "delete_with_gc",
        "held-pending-ruling",
    ),
    // a failed survivors re-read → `undead = 0` → `prune_empty_folders` with no survivors deletes every non-Wardrobe folder row (the kept avatar links' folders included) and reports the prune succeeded.
    (
        "quilltap-core/src/services/character_archive/service.rs",
        "prune_vault",
        "find_by_mount_point_id",
        "held-pending-ruling",
    ),
    // a swallowed delete still deletes the surviving chunks' `embedding_status` rows.
    (
        "quilltap-core/src/services/character_archive/service.rs",
        "prune_vault",
        "delete_with_gc",
        "held-pending-ruling",
    ),
    // prunes nothing and says nothing (paired with the `prune_vault` hazard).
    (
        "quilltap-core/src/services/character_archive/service.rs",
        "prune_empty_folders",
        "find_by_mount_point_id",
        "held-pending-ruling",
    ),
    // callers drop the `files` row after a swallowed link delete — the link + blob orphaned, success reported.
    (
        "quilltap-core/src/services/file_storage.rs",
        "delete_mount_blob_conn",
        "delete_with_gc",
        "held-pending-ruling",
    ),
    // a filesystem move then renames on disk but never deletes the source link (a stale row until rescan).
    (
        "quilltap-core/src/services/mount_index/file_ops.rs",
        "source_exists_or_throw",
        "find_by_mount_point_and_path",
        "held-pending-ruling",
    ),
    // a failed read skips the DEST_EXISTS guard → a non-force copy / move / write SILENTLY OVERWRITES the destination (and its hard-link siblings).
    (
        "quilltap-core/src/services/mount_index/file_ops.rs",
        "dest_exists",
        "find_by_mount_point_and_path",
        "held-pending-ruling",
    ),
    // a move reports success while the source link survives (DB arm: the file is duplicated, not moved).
    (
        "quilltap-core/src/services/mount_index/file_ops.rs",
        "delete_at_source",
        "delete_with_gc",
        "held-pending-ruling",
    ),
    // a force overwrite keeps the old link; with the verify read also failing, `delete_file` reports a false `deleted: true`.
    (
        "quilltap-core/src/services/mount_index/file_ops.rs",
        "delete_at_dest",
        "find_by_mount_point_and_path",
        "held-pending-ruling",
    ),
    // as `delete_at_dest`'s read — the swallowed delete half of the same false success.
    (
        "quilltap-core/src/services/mount_index/file_ops.rs",
        "delete_at_dest",
        "delete_with_gc",
        "held-pending-ruling",
    ),
    // a move reports success while the source link survives.
    (
        "quilltap-core/src/services/mount_index/file_ops.rs",
        "move_file",
        "delete_with_gc",
        "held-pending-ruling",
    ),
    // the disk rename has happened; the links keep the old paths and success is reported — the next scan drops them with their chunks, embeddings and descriptions.
    (
        "quilltap-core/src/services/mount_index/folder_ops.rs",
        "move_folder",
        "find_by_mount_point_id",
        "held-pending-ruling",
    ),
    // a failed read is "absent" → the user's general `state.json` is reset to `{}` at boot (v5's boot warns and skips today).
    (
        "quilltap-core/src/services/mount_index/general_state.rs",
        "ensure_general_state_file",
        "find_by_mount_point_and_path",
        "held-pending-ruling",
    ),
    // a swallowed delete is counted in `files_deleted` (the stale link heals on the next scan).
    (
        "quilltap-core/src/services/mount_index/scanner.rs",
        "remove_mount_file",
        "delete_with_gc",
        "held-pending-ruling",
    ),
    // bypasses the optimistic-concurrency guard → a concurrent edit is SILENTLY OVERWRITTEN; also logs the documents line twice where v4 logs once (the inner `write_database_document` pre-read).
    (
        "quilltap-core/src/services/mount_index/store_file.rs",
        "store_mount_file",
        "find_content_and_mtime_by_mount_point_and_path",
        "held-pending-ruling",
    ),
    // P4.142 Tier 3 (G5): the `.qtap` export's store read stays STRICT under the
    // standing backup/restore/import/export ruling (2026-08-03, "fix, don't
    // match"): v4's `lib/export/ndjson-writer.ts:625,642` falls back and exports
    // a broken store EMPTY, silently. A recorded divergence; the both-ways pin
    // lands when the export family next regenerates.
    (
        "quilltap-core/src/services/qtap_export/records.rs",
        "stream_one_store",
        "find_by_mount_point_id",
        "strict-by-ruling",
    ),
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
    // A blobs-repository receiver is `other-repo` before any override: an
    // `OVERRIDES` key is (file, fn, method), and one fn can call the same method
    // name on two repositories (`chat_attach_mount_file` — P4.142).
    if recv.to_lowercase().contains("blobs") {
        return "other-repo";
    }
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
            | "quilltap-core/src/db/doc_mount_chunks.rs"
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
            let scoped_out = SCOPED_METHODS.iter().any(|(m, needle, home)| {
                *m == base && rel != *home && !receiver(&code, i).contains(needle)
            });
            if METHODS.contains(&base) && bytes.get(end) == Some(&b'(') && !scoped_out {
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
    ("quilltap-core/src/api/characters.rs", "character_stats", "find_by_mount_point_id_or_empty", "converted"),
    ("quilltap-core/src/api/chat_media.rs", "chat_attach_mount_file", "find_by_mount_point_and_path", "converted"),
    ("quilltap-core/src/api/chat_media.rs", "chat_attach_mount_file", "find_by_mount_point_and_path", "other-repo"),
    ("quilltap-core/src/api/mount_files.rs", "mount_file_update", "find_by_mount_point_and_path", "other-repo"),
    ("quilltap-core/src/api/mount_files.rs", "mount_blob_update", "find_by_mount_point_and_path", "other-repo"),
    ("quilltap-core/src/api/mount_points.rs", "mount_point_list", "count_embedded_by_mount_point_ids_or_empty", "converted"),
    ("quilltap-core/src/api/mount_points.rs", "mount_point_get", "count_nonempty_embeddings_by_mount_point_id_or_zero", "converted"),
    ("quilltap-core/src/api/projects.rs", "project_file_list", "find_by_mount_point_id", "converted"),
    ("quilltap-core/src/api/scenarios.rs", "create_op", "find_by_mount_point_and_path_or_none", "converted"),
    ("quilltap-core/src/api/scenarios.rs", "update_op", "find_by_mount_point_and_path_or_none", "converted"),
    ("quilltap-core/src/api/scenarios.rs", "rename_op", "find_by_mount_point_and_path_or_none", "converted"),
    ("quilltap-core/src/api/scenarios.rs", "rename_op", "find_by_mount_point_and_path_or_none", "converted"),
    ("quilltap-core/src/db/character_vault.rs", "ensure_character_metadata_file", "find_by_mount_point_and_path", "held-pending-ruling"),
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
    ("quilltap-core/src/db/doc_mount_blobs.rs", "create_with_ids", "find_by_mount_point_and_path", "other-repo"),
    ("quilltap-core/src/db/doc_mount_chunks.rs", "find_rows_by_mount_point_id_or_empty", "find_rows_by_mount_point_id", "internal"),
    ("quilltap-core/src/db/doc_mount_chunks.rs", "find_row_by_id_or_none", "find_row_by_id", "internal"),
    ("quilltap-core/src/db/doc_mount_chunks.rs", "find_ids_by_link_id_or_empty", "find_ids_by_link_id", "internal"),
    ("quilltap-core/src/db/doc_mount_documents.rs", "find_by_mount_point_and_path_or_none", "find_by_mount_point_and_path", "internal"),
    ("quilltap-core/src/db/doc_mount_documents.rs", "find_content_and_mtime_by_mount_point_and_path_or_none", "find_content_and_mtime_by_mount_point_and_path", "internal"),
    ("quilltap-core/src/db/doc_mount_documents.rs", "find_many_by_mount_points_and_path_or_empty", "find_many_by_mount_points_and_path", "internal"),
    ("quilltap-core/src/db/doc_mount_documents.rs", "find_many_by_mount_points_in_folder", "find_many_by_mount_points_in_folder_opts", "internal"),
    ("quilltap-core/src/db/doc_mount_documents.rs", "find_many_by_mount_points_in_folder_or_empty", "find_many_by_mount_points_in_folder_opts_or_empty", "converted"),
    ("quilltap-core/src/db/doc_mount_documents.rs", "find_many_by_mount_points_in_folder_opts_or_empty", "find_many_by_mount_points_in_folder_opts", "internal"),
    ("quilltap-core/src/db/doc_mount_file_links.rs", "delete_database_document", "find_by_mount_point_and_path_or_none", "converted"),
    ("quilltap-core/src/db/doc_mount_file_links.rs", "delete_database_document", "delete_with_gc_or_false", "converted"),
    ("quilltap-core/src/db/doc_mount_file_links.rs", "find_by_mount_point_id_or_empty", "find_by_mount_point_id", "internal"),
    ("quilltap-core/src/db/doc_mount_file_links.rs", "find_by_mount_point_and_path_or_none", "find_by_mount_point_and_path", "internal"),
    ("quilltap-core/src/db/doc_mount_file_links.rs", "delete_with_gc_or_false", "delete_with_gc_or_false", "converted"),
    ("quilltap-core/src/db/doc_mount_file_links.rs", "delete_with_gc_or_false", "delete_with_gc", "internal"),
    ("quilltap-core/src/db/doc_mount_folders.rs", "find_by_mount_point_and_path", "find_by_mount_point_id", "internal"),
    ("quilltap-core/src/db/doc_mount_folders.rs", "find_by_mount_point_and_path_or_none", "find_by_mount_point_id_or_empty", "converted"),
    ("quilltap-core/src/db/doc_mount_folders.rs", "find_by_mount_point_id_or_empty", "find_by_mount_point_id", "internal"),
    ("quilltap-core/src/db/document_store_overlay.rs", "load_store_files", "find_many_by_mount_points_and_path_or_empty", "converted"),
    ("quilltap-core/src/db/document_store_overlay.rs", "read_properties", "find_by_mount_point_and_path", "held-pending-ruling"),
    ("quilltap-core/src/db/scenarios.rs", "list_scenarios_in_folder", "find_many_by_mount_points_in_folder_or_empty", "converted"),
    ("quilltap-core/src/db/scenarios.rs", "set_scenario_default_in_folder", "find_many_by_mount_points_in_folder_or_empty", "converted"),
    ("quilltap-core/src/db/vault_character_update.rs", "read_current_properties", "find_by_mount_point_and_path", "held-pending-ruling"),
    ("quilltap-core/src/db/vault_read_overlay.rs", "load_vault_file_maps", "find_many_by_mount_points_and_path_or_empty", "converted"),
    ("quilltap-core/src/db/vault_read_overlay.rs", "load_vault_file_maps", "find_many_by_mount_points_in_folder_or_empty", "converted"),
    ("quilltap-core/src/db/vault_read_overlay.rs", "load_vault_file_maps", "find_many_by_mount_points_in_folder_or_empty", "converted"),
    ("quilltap-core/src/db/vault_read_overlay.rs", "read_character_vault_wardrobe", "find_many_by_mount_points_in_folder_or_empty", "converted"),
    ("quilltap-core/src/db/vault_read_overlay.rs", "read_character_vault_wardrobe", "find_many_by_mount_points_and_path_or_empty", "converted"),
    ("quilltap-core/src/db/vault_wardrobe_write.rs", "project_array_into_vault_folder", "find_many_by_mount_points_in_folder_or_empty", "converted"),
    ("quilltap-core/src/documents/mod.rs", "classify_resolved_target", "find_by_mount_point_and_path_or_none", "converted"),
    ("quilltap-core/src/photos/avatar_rolls_service.rs", "delete_avatar_roll", "delete_with_gc", "held-pending-ruling"),
    ("quilltap-core/src/photos/character_gallery_service.rs", "list_character_gallery", "find_by_mount_point_id_or_empty", "converted"),
    ("quilltap-core/src/photos/character_gallery_service.rs", "remove_from_character_gallery", "delete_with_gc", "held-pending-ruling"),
    ("quilltap-core/src/photos/chat_gallery.rs", "add_blob_reference", "find_by_mount_point_and_path", "swallowed-by-other-means"),
    ("quilltap-core/src/photos/save_image_to_album.rs", "find_existing_photos_link_by_sha", "find_by_mount_point_id", "held-pending-ruling"),
    ("quilltap-core/src/photos/save_image_to_album.rs", "resolve_unique_relative_path", "find_by_mount_point_and_path", "other-repo"),
    ("quilltap-core/src/photos/save_image_to_album.rs", "resolve_unique_relative_path", "find_by_mount_point_and_path", "other-repo"),
    ("quilltap-core/src/photos/user_gallery_service.rs", "list_user_gallery", "find_by_mount_point_id_or_empty", "converted"),
    ("quilltap-core/src/photos/user_gallery_service.rs", "remove_from_user_gallery", "delete_with_gc", "held-pending-ruling"),
    ("quilltap-core/src/services/aesthetics.rs", "read_store_file_internal", "find_by_mount_point_and_path", "swallowed-by-other-means"),
    ("quilltap-core/src/services/character_archive/service.rs", "prune_vault", "find_by_mount_point_id", "held-pending-ruling"),
    ("quilltap-core/src/services/character_archive/service.rs", "prune_vault", "find_ids_by_link_id_or_empty", "converted"),
    ("quilltap-core/src/services/character_archive/service.rs", "prune_vault", "delete_with_gc", "held-pending-ruling"),
    ("quilltap-core/src/services/character_archive/service.rs", "prune_vault", "find_by_mount_point_id", "held-pending-ruling"),
    ("quilltap-core/src/services/character_archive/service.rs", "prune_empty_folders", "find_by_mount_point_id", "held-pending-ruling"),
    ("quilltap-core/src/services/core_whisper.rs", "read_vault_core_files", "find_many_by_mount_points_in_folder_opts_or_empty", "converted"),
    ("quilltap-core/src/services/core_whisper.rs", "assemble_group_core_files", "find_many_by_mount_points_in_folder_opts_or_empty", "converted"),
    ("quilltap-core/src/services/embedding_generate_job.rs", "mount_chunk_branch", "find_row_by_id", "converted"),
    ("quilltap-core/src/services/embedding_reindex_job.rs", "phase_mount_chunks", "find_rows_by_mount_point_id", "converted"),
    ("quilltap-core/src/services/file_storage.rs", "delete_mount_blob_conn", "delete_with_gc", "held-pending-ruling"),
    ("quilltap-core/src/services/file_storage.rs", "resolve_unique_relative_path", "find_by_mount_point_and_path", "other-repo"),
    ("quilltap-core/src/services/file_storage.rs", "resolve_unique_relative_path", "find_by_mount_point_and_path", "other-repo"),
    ("quilltap-core/src/services/file_storage.rs", "store_mount_blob", "find_by_mount_point_and_path_or_none", "converted"),
    ("quilltap-core/src/services/image_job_storage.rs", "write_main_avatar_to_vault", "find_by_mount_point_and_path_or_none", "converted"),
    ("quilltap-core/src/services/image_job_storage.rs", "write_main_avatar_to_vault", "delete_with_gc_or_false", "converted"),
    ("quilltap-core/src/services/image_job_storage.rs", "resolve_unique_relative_path", "find_by_mount_point_and_path", "other-repo"),
    ("quilltap-core/src/services/image_job_storage.rs", "resolve_unique_relative_path", "find_by_mount_point_and_path", "other-repo"),
    ("quilltap-core/src/services/knowledge_injector.rs", "retrieve_knowledge_for_turn", "find_by_mount_point_and_path_or_none", "converted"),
    ("quilltap-core/src/services/librarian_notifications.rs", "document_hidden_from_characters", "find_by_mount_point_and_path_or_none", "converted"),
    ("quilltap-core/src/services/memory_processor.rs", "read_vault_text_file_conn", "find_by_mount_point_and_path", "swallowed-by-other-means"),
    ("quilltap-core/src/services/mount_index/embedding_scheduler.rs", "enqueue_embedding_jobs_for_mount_point", "find_by_mount_point_id_or_empty", "converted"),
    ("quilltap-core/src/services/mount_index/embedding_scheduler.rs", "enqueue_embedding_jobs_for_mount_point", "find_rows_by_mount_point_id_or_empty", "converted"),
    ("quilltap-core/src/services/mount_index/file_ops.rs", "source_exists_or_throw", "find_by_mount_point_and_path", "held-pending-ruling"),
    ("quilltap-core/src/services/mount_index/file_ops.rs", "dest_exists", "find_by_mount_point_and_path", "held-pending-ruling"),
    ("quilltap-core/src/services/mount_index/file_ops.rs", "compute_dest_sha256", "find_by_mount_point_and_path_or_none", "converted"),
    ("quilltap-core/src/services/mount_index/file_ops.rs", "delete_at_source", "delete_with_gc", "held-pending-ruling"),
    ("quilltap-core/src/services/mount_index/file_ops.rs", "delete_at_source", "delete_with_gc", "held-pending-ruling"),
    ("quilltap-core/src/services/mount_index/file_ops.rs", "delete_at_dest", "find_by_mount_point_and_path", "held-pending-ruling"),
    ("quilltap-core/src/services/mount_index/file_ops.rs", "delete_at_dest", "delete_with_gc", "held-pending-ruling"),
    ("quilltap-core/src/services/mount_index/file_ops.rs", "move_file", "delete_with_gc", "held-pending-ruling"),
    ("quilltap-core/src/services/mount_index/file_ops.rs", "link_file", "find_by_mount_point_and_path", "swallowed-by-other-means"),
    ("quilltap-core/src/services/mount_index/folder_ops.rs", "move_folder", "find_by_mount_point_id", "held-pending-ruling"),
    ("quilltap-core/src/services/mount_index/general_state.rs", "ensure_general_state_file", "find_by_mount_point_and_path", "held-pending-ruling"),
    ("quilltap-core/src/services/mount_index/link_groups.rs", "reindex_inner", "find_by_mount_point_and_path_or_none", "converted"),
    ("quilltap-core/src/services/mount_index/list.rs", "mount_files_list", "find_by_mount_point_id", "converted"),
    ("quilltap-core/src/services/mount_index/read_file.rs", "read_mount_file_bytes_conn", "find_by_mount_point_and_path_or_none", "converted"),
    ("quilltap-core/src/services/mount_index/read_file.rs", "read_mount_file_bytes_conn", "find_by_mount_point_and_path_or_none", "converted"),
    ("quilltap-core/src/services/mount_index/read_file.rs", "read_mount_file_bytes_conn", "find_by_mount_point_and_path", "other-repo"),
    ("quilltap-core/src/services/mount_index/read_file.rs", "read_mount_file", "find_by_mount_point_and_path_or_none", "converted"),
    ("quilltap-core/src/services/mount_index/reindex.rs", "reindex_links", "find_by_mount_point_id_or_empty", "converted"),
    ("quilltap-core/src/services/mount_index/reindex.rs", "enqueue_embedding_jobs_scoped", "find_by_mount_point_id_or_empty", "converted"),
    ("quilltap-core/src/services/mount_index/reindex.rs", "enqueue_embedding_jobs_scoped", "find_rows_by_mount_point_id_or_empty", "converted"),
    ("quilltap-core/src/services/mount_index/reindex_file.rs", "reindex_inner", "find_content_and_mtime_by_mount_point_and_path_or_none", "converted"),
    ("quilltap-core/src/services/mount_index/reindex_file.rs", "reindex_inner", "find_by_mount_point_and_path", "other-repo"),
    ("quilltap-core/src/services/mount_index/reindex_file.rs", "reindex_inner", "find_by_mount_point_and_path_or_none", "converted"),
    ("quilltap-core/src/services/mount_index/scanner.rs", "process_mount_file", "find_by_mount_point_and_path_or_none", "converted"),
    ("quilltap-core/src/services/mount_index/scanner.rs", "remove_mount_file", "find_by_mount_point_and_path_or_none", "converted"),
    ("quilltap-core/src/services/mount_index/scanner.rs", "remove_mount_file", "delete_with_gc", "held-pending-ruling"),
    ("quilltap-core/src/services/mount_index/scanner.rs", "rescan_database_mount_point", "find_by_mount_point_id_or_empty", "converted"),
    ("quilltap-core/src/services/mount_index/store_file.rs", "resolve_unique_relative_path", "find_by_mount_point_and_path", "other-repo"),
    ("quilltap-core/src/services/mount_index/store_file.rs", "resolve_unique_relative_path", "find_by_mount_point_and_path", "other-repo"),
    ("quilltap-core/src/services/mount_index/store_file.rs", "store_mount_file", "find_by_mount_point_and_path_or_none", "converted"),
    ("quilltap-core/src/services/mount_index/store_file.rs", "store_mount_file", "find_content_and_mtime_by_mount_point_and_path", "held-pending-ruling"),
    ("quilltap-core/src/services/mount_index/store_file.rs", "store_mount_file", "find_by_mount_point_and_path_or_none", "converted"),
    ("quilltap-core/src/services/mount_index/store_file.rs", "store_mount_file", "find_by_mount_point_and_path_or_none", "converted"),
    ("quilltap-core/src/services/mount_index/sync/apply_store.rs", "assert_unchanged", "find_by_mount_point_and_path", "fallback-in-v4"),
    ("quilltap-core/src/services/mount_index/sync/apply_store.rs", "write_store_file", "find_by_mount_point_and_path", "fallback-in-v4"),
    ("quilltap-core/src/services/mount_index/sync/apply_store.rs", "read_store_bytes", "find_by_mount_point_and_path", "fallback-in-v4"),
    ("quilltap-core/src/services/mount_index/sync/apply_store.rs", "apply_store_action", "find_by_mount_point_and_path", "fallback-in-v4"),
    ("quilltap-core/src/services/mount_index/sync/apply_store.rs", "apply_store_action", "delete_with_gc", "fallback-in-v4"),
    ("quilltap-core/src/services/qtap_export/records.rs", "stream_one_store", "find_by_mount_point_id", "strict-by-ruling"),
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
    ("quilltap-core/src/tools/photo.rs", "find_existing_photos_link_by_sha", "find_by_mount_point_id_or_empty", "converted"),
    ("quilltap-web/src/files_routes.rs", "read_database_mount_file", "find_by_mount_point_and_path_or_none", "converted"),
    ("quilltap-web/src/files_routes.rs", "read_database_mount_file", "find_by_mount_point_and_path_or_none", "converted"),
    ("quilltap-web/src/files_routes.rs", "read_database_mount_file", "find_by_mount_point_and_path", "other-repo"),
    ("quilltap-web/src/files_routes.rs", "read_mount_blob", "find_by_mount_point_and_path", "other-repo"),
    ("quilltap-web/src/files_routes.rs", "read_mount_blob", "find_by_mount_point_and_path_or_none", "converted"),
    ("quilltap-web/src/files_routes.rs", "read_mount_blob", "find_by_mount_point_and_path_or_none", "converted"),
    ("quilltap-web/src/qtap_target_route.rs", "read_resolved_target", "find_by_mount_point_and_path_or_none", "converted"),
    ("quilltap-web/src/qtap_target_route.rs", "read_resolved_target", "find_by_mount_point_and_path_or_none", "converted"),
    ("quilltap-web/src/qtap_target_route.rs", "read_resolved_target", "find_by_mount_point_and_path", "other-repo"),
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
            count("strict-by-ruling"),
            count("held-pending-ruling"),
            count("fallback-in-v4"),
        ),
        COUNTS,
        "class counts moved (converted, internal, other-repo, strict-in-v4, swallowed, no-v4-counterpart, strict-by-ruling, held-pending-ruling, fallback-in-v4)"
    );
}

/// (converted, internal, other-repo, strict-in-v4 (import), swallowed-by-other-means,
/// no-v4-counterpart, strict-by-ruling, held-pending-ruling, fallback-in-v4),
/// with the arithmetic:
///
/// - **155 direct call sites in all** = 80 + 13 + 19 + 0 + 13 + 1 + 1 + 23 + 5.
///   P4.131 measured 130 over four method names; P4.142 widened [`METHODS`] by
///   the overlay's batch reads and the chunk reads (+25 rows: 19 converted, 6
///   internal) and corrected two classes (below).
/// - **converted 80** = P4.131's 23 + P4.142's 19 + G1's 14 + G2's first 5 +
///   the G2/G3 SAFE 19 (below). The 19: the overlay batch twins at
///   their 11 callers (`vault_read_overlay` 5, `document_store_overlay` 1,
///   `vault_wardrobe_write` 1, `scenarios` 2, `core_whisper` 2) + the documents
///   repository's own `…_in_folder_or_empty` → `…_opts_or_empty` 1 + the chunk
///   twins at their 7 callers (`embedding_scheduler` 1, `reindex` 1, `character_
///   archive` 1, `mount_points` 2, and the two whole-checkout wraps in
///   [`OVERRIDES`]: `embedding_reindex_job` 1, `embedding_generate_job` 1). **G1
///   14** (P4.142 item 8, the read-only route/listing sites): `api/characters`
///   `character_stats` 1, `api/projects` `project_file_list` 1 and
///   `mount_index/list` `mount_files_list` 1 (both through the home's FILES /
///   folders line, [`OVERRIDES`]), `read_file` 3, `web/files_routes` 4,
///   `web/qtap_target_route` 2, the two gallery lists 2. **G2's first 5**
///   (item 10): `api/scenarios` `create_op` 1 + `update_op` 1 + `rename_op` 2,
///   `documents/mod.rs` `classify_resolved_target` 1. **The G2/G3 safe 19**
///   (after the human's 2026-10-02 "convert safe, hold risky" ruling over two
///   per-site classifications): G2 14 — `tools/photo.rs` 1, `file_storage::
///   store_mount_blob` 1, `image_job_storage` 2, `store_file` 3 (the path
///   reads), `scanner::process_mount_file` 1, `reindex_file` 2, `link_groups` 1,
///   `knowledge_injector` 1, `librarian_notifications` 1, `chat_media` 1 (the
///   FILES path home, [`OVERRIDES`]); G3 5 — `file_ops::compute_dest_sha256`,
///   `scanner::remove_mount_file`'s lookup, `rescan_database_mount_point`,
///   `reindex` 2.
/// - **internal 13** = P4.131's 7 + each P4.142 twin's closure over its
///   propagating sibling (documents 3, chunks 3).
/// - **other-repo 19** = P4.131's 18 + `doc_mount_blobs.rs` `create_with_ids`
///   (the BLOBS repository's self-call, misfiled `fallback-in-v4` — [`OVERRIDES`]).
/// - **strict-in-v4 (import) 0** — the importer's strict reads are
///   `get_messages_strict` (P4.109), not these repositories; the class stays so a
///   future importer site must say so.
/// - **swallowed-by-other-means 13** = `.ok().flatten()` / a `match` arm /
///   `.unwrap_or_else` / `let Ok … else` at the site: `api/characters` 1,
///   `chat_gallery` 1, `aesthetics` 1, `memory_processor` 1, `file_ops::link_file`
///   1, `doc_edit/blob` 1, `doc_edit/shared` 5, `tools/photo` 2.
/// - **no-v4-counterpart 1** = `write_database_document`'s stored-mtime re-read
///   (see [`OVERRIDES`]).
/// - **strict-by-ruling 1** = `qtap_export/records.rs` `stream_one_store` (G5):
///   held strict under the 2026-08-03 backup/export ruling — v4 exports a broken
///   store EMPTY ([`OVERRIDES`]).
/// - **held-pending-ruling 23** — G2 6 + G3 17 whose v4 fallback destroys or
///   misreports data on a failed read (each named in [`OVERRIDES`]).
/// - **fallback-in-v4 5** (P4.131's "68" was 67 — the blobs self-call — G5
///   moved out, the rest converted or held) — **G4**
///   the sync applier's 5 (`apply_store.rs`) — the human's
///   RULING, unchanged (P4.142 Tier 3: option B, strict sync, recommended; v4's
///   `walkStore` reads through the same fallbacks and, with `propagateDeletes`
///   true, would plan the deletion of every unchanged disk file). Each G2/G3 site
///   converts only after its v4 downstream arm is read.
const COUNTS: (
    usize,
    usize,
    usize,
    usize,
    usize,
    usize,
    usize,
    usize,
    usize,
) = (80, 13, 19, 0, 13, 1, 1, 23, 5);

/// P4.142 §S.4 — reads HANDED to P4.144 this round, recorded as documentation:
/// the fold-episode pass's two memory reads (`services/fold_episode_pass.rs` —
/// the per-turn fragment read `memories_read::find_by_character_and_source_
/// message_ids` and the `memories_read::find_by_id` behind it), which P4.144
/// converts in its own lane (a twin in `db/memories_read.rs`). Not
/// document-store reads; listed so the conversion list says where they went.
/// The guard below asserts ONLY that each file exists — never its call text —
/// so P4.144's conversion cannot redden this census on the union.
const HANDED: &[(&str, &str, &str)] = &[
    (
        "quilltap-core/src/services/fold_episode_pass.rs",
        "memories_read::find_by_character_and_source_message_ids",
        "handed(P4.144)",
    ),
    (
        "quilltap-core/src/services/fold_episode_pass.rs",
        "memories_read::find_by_id",
        "handed(P4.144)",
    ),
];

#[test]
fn the_handed_rows_name_files_that_exist() {
    for (file, read, class) in HANDED {
        assert_eq!(*class, "handed(P4.144)");
        assert!(
            repo_root().join("crates").join(file).is_file(),
            "the handed row's file is gone: {file} ({read})"
        );
    }
}

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
