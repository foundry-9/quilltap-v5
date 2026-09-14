//! P4.D182 — `files.generationKey` travels AS-IS: never remapped, never
//! derived, and in v4's `FileEntrySchema` position on the wire.
//!
//! v4's vendored `qtap-export.schema.json` states the contract in prose, and
//! this lane re-vendored the sentence: *"Travels as-is. It folds in the
//! exporting instance's image-profile id, so on a receiving instance the key
//! is simply inert until that same profile id derives it again: nothing on the
//! target will ever compute a matching key by accident, and a round trip back
//! to the origin still hits."*
//!
//! The differentials carry a planted key whose VALUE is a UUID the same
//! archive remaps elsewhere (`PROJECT_1`'s id), so a remap would be visible —
//! but only up to a point, and that point is why this file exists:
//! `system_import_state` normalizes every minted id to `<minted-N>`, which
//! labels a correctly-carried key and a wrongly-remapped one IDENTICALLY (the
//! P4.D126 normalizer blindness, recorded there in as many words). A census is
//! what closes it.
//!
//! No oracle: v4's behaviour here is an ABSENCE — no code in `lib/` rewrites
//! the column on import or restore — so the comparand is v5's own source.

use std::path::PathBuf;

fn core_src(rel: &str) -> String {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../quilltap-core/src")
        .join(rel);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("read {}: {e}", p.display()))
}

/// Every reader that turns an archive's `files` record into a `FileCreate`
/// must take the key VERBATIM. The assignment is matched whole, so wrapping it
/// in anything at all — a remap lookup, an `unwrap_or`, a derive — reddens.
#[test]
fn every_archive_reader_carries_the_key_verbatim() {
    for (file, reader) in [
        (
            "services/backup/restore/orchestrator.rs",
            "the backup restore reader",
        ),
        ("services/quilltap_import/files.rs", "the `.qtap` importer"),
    ] {
        let src = core_src(file);
        let verbatim = src
            .matches("generation_key: os(file, \"generationKey\"),")
            .count();
        assert_eq!(
            verbatim, 1,
            "{reader} ({file}) must assign `generation_key` exactly once, and \
             verbatim from the record: the avatar cache key travels as-is, so \
             nothing here may remap, default or derive it"
        );
        // …and there is no OTHER mention of the key in that file, which is what
        // catches a remap added a few lines away rather than in the literal.
        assert_eq!(
            src.matches("generationKey").count(),
            1,
            "{reader} ({file}) names `generationKey` more than once — the only \
             legitimate mention is the verbatim read"
        );
        assert_eq!(
            src.matches("generation_key").count(),
            1,
            "{reader} ({file}) names `generation_key` more than once"
        );
    }
}

/// The export/backup field spec places the key in v4's `FileEntrySchema` slot
/// — between `generationRevisedPrompt` and `description`. That position is the
/// exported record's KEY ORDER, which `.qtap` consumers compare byte-for-byte,
/// so it is not cosmetic.
#[test]
fn the_export_field_spec_holds_v4s_schema_position() {
    let src = core_src("services/backup/collect.rs");
    let start = src
        .find("pub(crate) const FILES: &[(&str, F)] = &[")
        .expect("the FILES field spec");
    let end = start + src[start..].find("];").expect("the spec's end");
    let names: Vec<&str> = src[start..end]
        .lines()
        .filter_map(|l| {
            let t = l.trim();
            t.strip_prefix("(\"")
                .and_then(|r| r.find('"').map(|e| &r[..e]))
        })
        .collect();

    let at = names
        .iter()
        .position(|n| *n == "generationKey")
        .expect("the FILES spec carries the cache key");
    assert_eq!(names[at - 1], "generationRevisedPrompt");
    assert_eq!(names[at + 1], "description");
    assert_eq!(
        names.len(),
        24,
        "the `files` export record's field count moved: {names:?}"
    );
}

/// **Exactly one** site derives a key and exactly one writes it — v4's shape,
/// and now v5's.
///
/// P4.D182 wrote this as "nothing writes a key yet", naming P4.D184 as the lane
/// that would edit it. This is that edit, and it keeps the claim rather than
/// dropping it: the avatar job binds `cache_keys.key` on a fresh generation, the
/// derivation lives in `services/avatar_cache.rs` (the ONE home — a second
/// spelling is a second format, and the two drift), and nothing else in the
/// crate constructs a key at all. A lane that started deriving one elsewhere
/// would make the cache-hit measurements meaningless in exactly the way
/// P4.D182's version of this test was written to prevent.
#[test]
fn exactly_one_site_writes_the_key_and_one_module_derives_it() {
    let src = core_src("services/character_avatar_job.rs");
    assert!(
        src.contains("generation_key: Some(input.generation_key.clone()),"),
        "the avatar job must bind the cache key it derived (v4 `7fbf8a55b`: \
         `generationKey: cacheKeys.key`)"
    );
    assert!(
        !src.contains("generation_key: None,"),
        "the avatar job must not ALSO have a None-binding write site"
    );

    // The derivation is the cache module's, and only the cache module's. The
    // collapse heal and the job both call in; neither spells out a preimage.
    for (file, why) in [
        (
            "services/character_avatar_job.rs",
            "the job derives through `avatar_cache::derive_avatar_cache_keys`",
        ),
        (
            "db/avatar_rolls_collapse_heal.rs",
            "the heal groups through `avatar_cache::derive_legacy_avatar_cache_key`",
        ),
    ] {
        let src = core_src(file);
        assert!(
            !src.contains("\"v\": 1") && !src.contains("\"v\": 0"),
            "{file}: the key preimage is spelled out here — {why}, and a second \
             derivation is a second format"
        );
    }
}
