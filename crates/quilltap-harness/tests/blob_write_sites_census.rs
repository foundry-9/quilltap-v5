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
//! The lexer (`production_zone` / `next_token` / `test_item_end` / `code_only`)
//! lives in `tests/source_census/mod.rs`, shared with
//! `compressed_column_write_sites_census.rs` (P4.110 lifted it there — this file
//! had carried a verbatim copy since P4.104).
//!
//! Run standalone:
//!   cargo test -p quilltap-harness --test blob_write_sites_census

mod source_census;

use source_census::{code_only, core_src_root, production_zone, rust_sources};

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
