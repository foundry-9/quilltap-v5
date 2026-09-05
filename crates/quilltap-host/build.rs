//! Embed the shipped help tree (`<repo>/help/**/*.md`) into the host binary at
//! compile time (P4.9I2A, Tier 1 item 1b).
//!
//! v4 reads its help documentation from `join(process.cwd(), 'help')` at
//! runtime, because a Next.js server always runs from its checkout. A native
//! binary runs from anywhere — the Tauri bundle, a Docker image, `cargo run`
//! from a sibling crate — and until this build script v5 had NO `help/` tree at
//! all: the runtime walker found nothing, every `EMBEDDING_REINDEX_ALL` synced
//! an EMPTY tree, and `help_search` on a fresh instance read zero rows. The
//! same idea as `services::quilltap_import::seed_assets` (the first-startup
//! assets ride `include_str!`), at the scale of 120 files.
//!
//! The generated file is one table literal:
//!
//! ```text
//! pub static EMBEDDED_HELP: &[(&str, &str)] = &[
//!     ("help/aurora.md", include_str!("/abs/path/help/aurora.md")),
//!     …
//! ];
//! ```
//!
//! whose `rel_path` is v4's `relative(process.cwd(), filePath)` (`help/<name>.md`)
//! and whose ORDER is exactly the runtime walker's — `find_markdown_files`
//! (`src/files_store.rs`) reproduced here verbatim, because a build script cannot
//! import its own crate: each directory's entries sorted by name bytes (Node's
//! `readdirSync` order — libuv sorts `scandir` with `strcmp`), subdirectory
//! contents inlined at the directory's position, `.md` suffix only. The insertion order into
//! `help_docs` (and so `findAll`'s rowid order, the Guide list order and the
//! context resolver's "first match wins") follows it, exactly as v4's
//! `readdirSync` order does on the same filesystem. The harness guard
//! `help_tree_embed_guard` holds the embedded table equal to the on-disk walk
//! (path set, order, bytes), so a stale build cannot pass a gate.
//!
//! The file lands in `OUT_DIR`, never in the source tree (the
//! `quilltap-sqlite3mc-sys` build-script precedent).

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

/// v4 `findMarkdownFiles` — mirrored from `src/files_store.rs::find_markdown_files`
/// (read its doc comment for the ORDER rule). Recurse `dir` in `readdirSync`
/// order: each directory's entries SORTED by `strcmp` on the name (libuv's
/// `uv__fs_scandir_sort` — Node never surfaces the raw syscall order), files and
/// directories interleaved, subdirectory contents inlined at the directory's
/// position. A directory read error yields no entries for that directory.
fn find_markdown_files(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let Ok(entries) = fs::read_dir(dir) else {
        return files;
    };
    let mut collected: Vec<fs::DirEntry> = Vec::new();
    for entry in entries {
        let Ok(entry) = entry else {
            return files;
        };
        collected.push(entry);
    }
    collected.sort_unstable_by(|a, b| {
        a.file_name()
            .as_encoded_bytes()
            .cmp(b.file_name().as_encoded_bytes())
    });
    for entry in collected {
        let path = entry.path();
        let Ok(meta) = fs::metadata(&path) else {
            break;
        };
        if meta.is_dir() {
            files.extend(find_markdown_files(&path));
        } else if path
            .file_name()
            .and_then(|n| n.to_str())
            .map(|n| n.ends_with(".md"))
            .unwrap_or(false)
        {
            files.push(path);
        }
    }
    files
}

fn main() {
    let manifest_dir =
        PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    // The workspace root's `help/` — the vendored v4 tree, byte-identical to the
    // pinned v4 checkout (the lane record carries the `diff -r`).
    let repo_root = manifest_dir
        .join("../..")
        .canonicalize()
        .expect("workspace root");
    let help_dir = repo_root.join("help");
    println!("cargo:rerun-if-changed={}", help_dir.display());

    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
    let out_path = out_dir.join("help_embedded.rs");
    let mut out = fs::File::create(&out_path).expect("create help_embedded.rs");

    writeln!(
        out,
        "/// The shipped help tree, embedded at compile time (see `build.rs`). One\n\
         /// `(rel_path, content)` per file, in the runtime walker's order.\n\
         pub static EMBEDDED_HELP: &[(&str, &str)] = &["
    )
    .unwrap();

    let files = if help_dir.is_dir() {
        find_markdown_files(&help_dir)
    } else {
        // A checkout without `help/` builds an EMPTY table rather than failing
        // the build — the embed guard test is what refuses that state.
        Vec::new()
    };
    for path in &files {
        println!("cargo:rerun-if-changed={}", path.display());
        let rel = path
            .strip_prefix(&repo_root)
            .expect("under the repo root")
            .to_string_lossy()
            .replace('\\', "/");
        // `include_str!` needs an absolute path (the generated file lives in
        // OUT_DIR, not beside the tree). Rust string escapes for the two paths.
        writeln!(
            out,
            "    ({:?}, include_str!({:?})),",
            rel,
            path.to_string_lossy()
        )
        .unwrap();
    }
    writeln!(out, "];").unwrap();
}
