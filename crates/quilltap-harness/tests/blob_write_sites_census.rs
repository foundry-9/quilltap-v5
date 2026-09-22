//! The blob WRITE-SITE census (P4.104 — v4 `186eb09cb`, bug 159's image half).
//!
//! **v4's normalization cannot be skipped by a caller.** `linkBlobContent`
//! imports `sharp` at module scope and runs `normalizeLinkBlobImage` on every
//! write, so the only way out is the explicit `normalizeImages: false` flag —
//! which exactly one caller passes (`import-document-stores.ts:326`, the
//! byte-fidelity `.qtap` restore).
//!
//! **v5 injects the encoder**, so the default is the opposite: a blob write
//! built on `DocMountFileLinksRepository::new` (or `DocMountBlobsRepository::new`
//! for the `create` facade) has NO encoder and stores the original bytes where
//! v4 stores WebP. That is the round-`f45a517a9` finding this lane closes (ten
//! sites did exactly that), and nothing else in the repo can see it: a
//! differential only notices on a DECODABLE image in a family somebody grew.
//!
//! Hence this census, per file over `crates/quilltap-core/src`'s production
//! zone: the number of blob WRITE calls (`link_blob_content(`,
//! `link_blob_content_with_ids(`, and every `CreateBlobInput {` literal — the
//! `create` facade's input), the number of `with_blob_codec(` constructions,
//! and the assertion that the census's file list IS the set of production
//! files making a blob write. A new write site is a red here, not a note.
//!
//! The lexer ([`production_zone`] / [`next_token`] / [`test_item_end`] /
//! [`code_only`]) is copied verbatim from `compressed_column_write_sites_census.rs`
//! — P4.105 owns that file this round, so lifting it into shared test support
//! is left for a round where nobody is editing it.
//!
//! Run standalone:
//!   cargo test -p quilltap-harness --test blob_write_sites_census

use std::path::{Path, PathBuf};

/// `(path under crates/quilltap-core/src, blob write calls, with_blob_codec
/// constructions, why)`.
///
/// **The arithmetic: 1 + 1 + 1 + 1 + 2 + 1 + 1 + 1 + 1 + 1 + 1 = 12 blob write
/// calls across eleven files, and 12 `with_blob_codec` constructions** — the
/// ten sites the P4.104 order named (with `image_job_storage.rs` holding two),
/// the sync applier P4.D209 wired, and the `create` facade that the doc-edit
/// tool writes through. Every write file constructs exactly as many codec'd
/// repositories as it makes writes.
const CENSUS: &[(&str, usize, usize, &str)] = &[
    (
        "tools/doc_edit/blob.rs",
        1,
        1,
        "`doc_write_blob`'s `CreateBlobInput` through \
         `DocMountBlobsRepository::with_blob_codec` — the tool context's \
         `blob_webp` (the tool runner's byte store).",
    ),
    (
        "db/doc_mount_blobs.rs",
        1,
        1,
        "the `create` facade's `link_blob_content_with_ids`, built \
         `with_blob_codec` when the facade holds an encoder (the doc-edit tool) \
         and `::new` when it does not (the `.qtap` import's `false` write, \
         below).",
    ),
    (
        "photos/character_gallery_service.rs",
        1,
        1,
        "`save_to_character_gallery` — its `blob_webp` argument (the engine's, \
         the web route's host codec, or the avatar-roll byte store's).",
    ),
    (
        "photos/user_gallery_service.rs",
        1,
        1,
        "`save_to_user_gallery` — the byte store's `blob_webp`.",
    ),
    (
        "photos/save_image_to_album.rs",
        1,
        1,
        "`save_image_to_album` — the byte store's `blob_webp`.",
    ),
    (
        "services/image_job_storage.rs",
        2,
        2,
        "`write_main_avatar_to_vault` (`PixelCodecWebp` over the codec it \
         transcodes with) and `store_blob_to_mount` (the `blob_webp` argument \
         of the vault-history and Lantern writers).",
    ),
    (
        "services/mount_index/file_ops.rs",
        1,
        1,
        "`write_dest_bytes`' binary arm — copy / move / write-file — through \
         its `webp` argument (the engine's `blob_webp`).",
    ),
    (
        "services/file_storage.rs",
        1,
        1,
        "`store_mount_blob` — `PixelCodecWebp` over the pixel codec its own \
         pre-transcode uses (the user-uploads and project bridges).",
    ),
    (
        "services/mount_index/store_file.rs",
        1,
        1,
        "`store_mount_file` — its `webp` argument, the same encoder as its \
         pre-transcode.",
    ),
    (
        "tools/generate_image.rs",
        1,
        1,
        "`save_generated_image` — the image-generation deps' `blob_webp`.",
    ),
    (
        "services/mount_index/sync/apply_store.rs",
        1,
        1,
        "the `quilltap sync` applier (P4.D209, the first wired site): \
         `with_blob_codec` when the engine holds an encoder.",
    ),
];

/// Production files that make a blob write WITHOUT an encoder — each with its
/// reason. There is exactly one, and it must stay the only one.
const EXEMPT: &[(&str, usize, &str)] = &[(
    "services/quilltap_import/document_stores.rs",
    1,
    "the `.qtap` import's document-store blobs: `normalize_images: false`, the \
     ONE byte-fidelity write (v4 `import-document-stores.ts:326`, the only \
     `false` in v4's tree). It writes through `DocMountBlobsRepository::new`, \
     which holds no encoder — and none is consulted under the flag.",
)];

fn count_calls(code: &str, ident: &str, exclude_prefix: &str) -> usize {
    let mut n = 0usize;
    let mut from = 0usize;
    while let Some(at) = code[from..].find(ident) {
        let abs = from + at;
        let after = abs + ident.len();
        let before = code[..abs].trim_end();
        let prev_ok = abs == 0
            || !code.as_bytes()[abs - 1].is_ascii_alphanumeric()
                && code.as_bytes()[abs - 1] != b'_';
        if prev_ok && !before.ends_with(exclude_prefix) {
            n += 1;
        }
        from = after;
    }
    n
}

/// Blob write calls in a file's code-only production zone.
fn blob_writes(code: &str) -> usize {
    count_calls(code, "link_blob_content(", "fn")
        + count_calls(code, "link_blob_content_with_ids(", "fn")
        + count_calls(code, "CreateBlobInput {", "struct")
}

/// `with_blob_codec(` constructions (the definition excluded).
fn codec_constructions(code: &str) -> usize {
    count_calls(code, "with_blob_codec(", "fn")
}

/// The links repository's own delegation (`link_blob_content` →
/// `link_blob_content_with_ids`) is the chokepoint's home, not a write site.
const HOME: &str = "db/doc_mount_file_links.rs";

fn census_view() -> Vec<(String, usize, usize, String)> {
    let root = core_src_root();
    let mut files = Vec::new();
    rust_sources(&root, &mut files);
    files.sort();
    let mut out = Vec::new();
    for path in files {
        let rel = path
            .strip_prefix(&root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        let src = std::fs::read_to_string(&path).unwrap();
        let code = code_only(&production_zone(&src));
        let writes = blob_writes(&code);
        if writes > 0 && rel != HOME {
            out.push((rel, writes, codec_constructions(&code), code));
        }
    }
    out
}

#[test]
fn every_production_blob_write_carries_an_encoder() {
    let view = census_view();
    let mut failures = Vec::new();
    for (rel, writes, codecs, code) in &view {
        if let Some((_, want_w, want_c, _)) = CENSUS.iter().find(|(p, ..)| p == rel) {
            if writes != want_w || codecs != want_c {
                failures.push(format!(
                    "{rel}: {writes} blob writes / {codecs} with_blob_codec — the census \
                     says {want_w} / {want_c}"
                ));
            }
        } else if let Some((_, want_w, _)) = EXEMPT.iter().find(|(p, ..)| p == rel) {
            if writes != want_w {
                failures.push(format!(
                    "{rel} (EXEMPT): {writes} blob writes — the census says {want_w}"
                ));
            }
            if !code.contains("normalize_images: false") {
                failures.push(format!(
                    "{rel} (EXEMPT) no longer passes `normalize_images: false` — its \
                     exemption is gone"
                ));
            }
        } else {
            failures.push(format!(
                "{rel}: {writes} blob write(s) the census does not know — a new write \
                 site must build `with_blob_codec` and join CENSUS (or, for a \
                 byte-fidelity write, EXEMPT with its reason)"
            ));
        }
    }
    for (p, ..) in CENSUS {
        if !view.iter().any(|(rel, ..)| rel == p) {
            failures.push(format!(
                "{p}: in the census but makes no blob write any more"
            ));
        }
    }
    for (p, ..) in EXEMPT {
        if !view.iter().any(|(rel, ..)| rel == p) {
            failures.push(format!("{p}: EXEMPT but makes no blob write any more"));
        }
    }
    let total_w: usize = CENSUS.iter().map(|(_, w, _, _)| w).sum();
    let total_c: usize = CENSUS.iter().map(|(_, _, c, _)| c).sum();
    if (total_w, total_c) != (12, 12) {
        failures.push(format!(
            "the census sums to {total_w} writes / {total_c} with_blob_codec — the \
             header's arithmetic says 12 / 12"
        ));
    }
    assert!(
        failures.is_empty(),
        "blob write-site census:\n{}",
        failures.join("\n")
    );
}

#[test]
fn the_counters_see_only_code() {
    let src = r#"
        // a comment naming link_blob_content( and with_blob_codec( counts nothing
        fn link_blob_content(&self) {}
        pub struct CreateBlobInput { a: u8 }
        fn with_blob_codec(c: u8) {}
        fn site() {
            let s = "link_blob_content(";
            let r = Repo::with_blob_codec(conn, c);
            r.link_blob_content(&x);
            let i = CreateBlobInput { a: 1 };
        }
        #[cfg(test)]
        mod tests { fn t() { repo.link_blob_content(&y); Repo::with_blob_codec(a, b); } }
    "#;
    let code = code_only(&production_zone(src));
    assert_eq!(blob_writes(&code), 2);
    assert_eq!(codec_constructions(&code), 1);
}

// ===========================================================================
// The lexer — copied verbatim from `compressed_column_write_sites_census.rs`
// (see the module doc).
// ===========================================================================

fn core_src_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("the harness crate sits two levels under the repo root")
        .join("crates/quilltap-core/src")
}

fn rust_sources(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = std::fs::read_dir(dir).unwrap_or_else(|e| panic!("read {}: {e}", dir.display()));
    for entry in entries {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            rust_sources(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            out.push(path);
        }
    }
}

/// The file with every `#[cfg(test)]` item removed — and **everything else
/// kept verbatim**, string literals included.
///
/// Two properties that pull in opposite directions, and getting them mixed up
/// cost this census its most important arm:
///
/// 1. **Finding an item's boundary needs a LEXER.** The sibling census
///    (`stream_watchdog_wrap_census.rs`) balances braces by counting `{`/`}`
///    characters, which works because it only ever scans a curated file list.
///    Run over all of `crates/quilltap-core/src` the same counter panics on
///    **ten files** — `cycle_order.rs`, `select_speaker.rs`,
///    `db/chats_read.rs`, `db/fictional_clock_anchor_repair.rs`,
///    `api/generators_wizard.rs`, `generators/llm_json.rs`,
///    `services/chat_events.rs`, `services/agent_mode.rs`,
///    `services/off_scene.rs`, `services/avatar_cache.rs` — every one a test
///    module holding JSON fixture text whose braces sit inside a STRING
///    literal. So [`next_token`] skips strings, raw strings, char literals and
///    both comment forms when hunting for the closing brace.
///
/// 2. **The OUTPUT must keep string literals.** The thing this census searches
///    for — `INSERT INTO chat_messages (… content …)` — *is* a string literal.
///    An earlier draft emitted a space in place of every literal, and its
///    mutation proof duly survived: a brand-new file with an unconverted
///    `chat_messages` insert was not caught, because the insert had been
///    elided before the search ever ran.
///
/// Stripping test items is not optional the other way either: a test module's
/// seeds INSERT into `chat_messages` freely against trigger-less in-memory
/// DDL, and counting those would drown the production signal.
fn production_zone(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let mut i = 0usize;
    let mut kept_from = 0usize;
    while i < src.len() {
        if src[i..].starts_with("#[cfg(test)]") {
            out.push_str(&src[kept_from..i]);
            match test_item_end(src, i) {
                Some(end) => {
                    i = end;
                    kept_from = end;
                    continue;
                }
                // Unterminated: keep the rest rather than silently dropping it.
                None => {
                    kept_from = i;
                    break;
                }
            }
        }
        // Step by TOKEN, not by char, so a `#[cfg(test)]` inside a string or a
        // comment is not mistaken for a real attribute.
        let (tok_end, _is_code) = next_token(src, i);
        i = tok_end.max(i + 1);
    }
    out.push_str(&src[kept_from.min(src.len())..]);
    out
}

/// One lexical step from `at`: `(end, is_code)`. A string, raw string, char
/// literal or comment is NOT code; everything else is.
fn next_token(src: &str, at: usize) -> (usize, bool) {
    let rest = &src[at..];
    if rest.starts_with("//") {
        let end = rest.find('\n').map(|n| at + n).unwrap_or(src.len());
        return (end, false);
    }
    if rest.starts_with("/*") {
        let mut depth = 0usize;
        let mut idx = 0usize;
        while idx < rest.len() {
            if rest[idx..].starts_with("/*") {
                depth += 1;
                idx += 2;
            } else if rest[idx..].starts_with("*/") {
                depth -= 1;
                idx += 2;
                if depth == 0 {
                    return (at + idx, false);
                }
            } else {
                idx += next_char_len(rest, idx);
            }
        }
        return (src.len(), false);
    }
    // Raw string: r"…", r#"…"#, br"…", br#"…"#
    let raw_prefix = if rest.starts_with("r\"") || rest.starts_with("r#") {
        Some(1usize)
    } else if rest.starts_with("br\"") || rest.starts_with("br#") {
        Some(2usize)
    } else {
        None
    };
    if let Some(prefix) = raw_prefix {
        let hashes = rest[prefix..].chars().take_while(|c| *c == '#').count();
        let open = prefix + hashes;
        if rest[open..].starts_with('"') {
            let mut close = String::from("\"");
            for _ in 0..hashes {
                close.push('#');
            }
            let body = &rest[open + 1..];
            let end = body
                .find(&close)
                .map(|n| at + open + 1 + n + close.len())
                .unwrap_or(src.len());
            return (end, false);
        }
    }
    if rest.starts_with('"') {
        let mut idx = 1usize;
        while idx < rest.len() {
            let b = rest.as_bytes()[idx];
            if b == b'\\' {
                idx += 1 + next_char_len(rest, idx + 1);
                continue;
            }
            if b == b'"' {
                return (at + idx + 1, false);
            }
            idx += next_char_len(rest, idx);
        }
        return (src.len(), false);
    }
    // A char literal, distinguished from a LIFETIME by its closing quote.
    if rest.starts_with('\'') {
        let idx = if rest.as_bytes().get(1) == Some(&b'\\') {
            // `'\n'`, `'\\'`, `'\u{1f600}'` — scan to the closing quote.
            rest[2..]
                .find('\'')
                .map(|n| 2 + n)
                .unwrap_or(rest.len().min(2))
        } else {
            1 + next_char_len(rest, 1)
        };
        if rest.as_bytes().get(idx) == Some(&b'\'') {
            return (at + idx + 1, false);
        }
        // Otherwise it is a lifetime; fall through as code.
    }
    (at + next_char_len(src, at), true)
}

fn next_char_len(s: &str, at: usize) -> usize {
    s[at..].chars().next().map(char::len_utf8).unwrap_or(1)
}

/// The end byte of the `#[cfg(test)]` item starting at `at` — its balanced
/// body, or the statement's `;` for a brace-less item (a `use`).
fn test_item_end(src: &str, at: usize) -> Option<usize> {
    let mut i = at + "#[cfg(test)]".len();
    let mut brace_start = None;
    while i < src.len() {
        let (end, is_code) = next_token(src, i);
        if is_code {
            match src.as_bytes()[i] {
                b'{' => {
                    brace_start = Some(i);
                    break;
                }
                b';' => return Some(end),
                _ => {}
            }
        }
        i = end;
    }
    let mut i = brace_start?;
    let mut depth = 0usize;
    while i < src.len() {
        let (end, is_code) = next_token(src, i);
        if is_code {
            match src.as_bytes()[i] {
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(end);
                    }
                }
                _ => {}
            }
        }
        i = end;
    }
    None
}

/// The production zone with COMMENTS and STRING LITERALS removed — the view
/// [`codec_calls`] counts over.
///
/// Needed because the zone is verbatim (it must be, so arm (b) can read the
/// SQL), which means a prose mention of `text_to_blob()` in a why-comment
/// counts as a call. That inflated `db/avatar_rolls_collapse_heal.rs` from 2 to
/// 3 on first run, the extra being this port's own comment explaining that the
/// heal writes back through `text_to_blob()`.
fn code_only(zone: &str) -> String {
    let mut out = String::with_capacity(zone.len());
    let mut i = 0usize;
    while i < zone.len() {
        let (end, is_code) = next_token(zone, i);
        if is_code {
            out.push_str(&zone[i..end]);
        } else {
            out.push(' ');
        }
        i = end.max(i + 1);
    }
    out
}
