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

/// Nothing anywhere DERIVES a key. v4 has exactly one deriver
/// (`lib/wardrobe/avatar-cache.ts`, P4.D184's to port) and exactly one writer
/// (the avatar job's `files.create`); until that lane lands, every v5 write
/// site binds `None`. This is the absence that makes the round's two halves
/// separable — and it is worth an assertion, because a lane that quietly
/// started writing a key here would make P4.D184's cache-hit measurements
/// meaningless.
#[test]
fn nothing_in_this_lane_writes_a_non_null_key() {
    let src = core_src("services/character_avatar_job.rs");
    assert!(
        src.contains("generation_key: None,"),
        "the avatar job must still bind None — P4.D184 is what changes this \
         line, and when it does, this assertion is what it edits"
    );
}
