//! P4.D227 (v4 `3b463d6b1`, #76) — the retired Concierge settings, by census
//! (the order's Tier 2 item 11).
//!
//! #76 gave the Concierge ONE settings home, `chat_settings.conciergeSettings`,
//! and retired three others: the `dangerousContentSettings` bag (its mode,
//! scans, threshold, desk ids and display), the top-level
//! `uncensoredImageDescriptionProfileId` (the vision fallback) and
//! `cheapLLMSettings.imagePromptProfileId` (the image-prompt crafter). v5
//! retired both of its structs for them (`DangerousContentSettings` in
//! `db::chat_settings` and the narrow one in `cheap_llm`) and the
//! `provider_failover::DangerSettings` carrier. What must never come back is a
//! consumer that reads the old homes: a migrated instance still CARRIES them
//! (v5 never drops a column — E.2), so a stray read would quietly obey a
//! setting the operator can no longer see or change.
//!
//! The census, over every production file of the three server crates (test
//! items stripped, COMMENTS stripped, string literals KEPT — a wire key is a
//! literal):
//!
//!   1. no retired Rust identifier survives anywhere;
//!   2. the two retired wire keys appear only where a reader MUST name them —
//!      the legacy mapper (restore / the backup remap translate through it) and
//!      the settings PUT's refusal (which names them in its 400);
//!   3. the crafter under the cheap-LLM bag is named only by those two plus the
//!      repository read that strips it.
//!
//! Standing on the shared source-census lexer (`source_census`).

mod source_census;

use std::collections::BTreeMap;
use std::path::PathBuf;

use source_census::{contains_word, core_src_root, next_token, production_zone, rust_sources};

/// The production zone with comments removed and everything else — code AND
/// string literals — kept verbatim.
fn without_comments(zone: &str) -> String {
    let mut out = String::with_capacity(zone.len());
    let mut i = 0usize;
    while i < zone.len() {
        let (end, is_code) = next_token(zone, i);
        let is_literal = zone[i..].starts_with('"')
            || zone[i..].starts_with("r\"")
            || zone[i..].starts_with("r#")
            || zone[i..].starts_with("b\"")
            || zone[i..].starts_with("br\"")
            || zone[i..].starts_with("br#")
            || zone[i..].starts_with('\'');
        if is_code || is_literal {
            out.push_str(&zone[i..end]);
        } else {
            out.push(' ');
        }
        i = end.max(i + 1);
    }
    out
}

/// `(crate-relative path, production zone without comments)` for every file of
/// the three server crates.
fn production_files() -> Vec<(String, String)> {
    let crates = core_src_root()
        .parent()
        .and_then(|p| p.parent())
        .expect("crates/")
        .to_path_buf();
    let mut out = Vec::new();
    for krate in ["quilltap-core", "quilltap-web", "quilltap-host"] {
        let mut files: Vec<PathBuf> = Vec::new();
        rust_sources(&crates.join(krate).join("src"), &mut files);
        files.sort();
        for f in files {
            let src = std::fs::read_to_string(&f).expect("read source");
            let rel = f
                .strip_prefix(&crates)
                .expect("under crates/")
                .display()
                .to_string();
            out.push((rel, without_comments(&production_zone(&src))));
        }
    }
    out
}

/// Word-bounded occurrences of `needle`, per file.
fn census(needle: &str) -> BTreeMap<String, usize> {
    let mut hits = BTreeMap::new();
    for (file, zone) in production_files() {
        let mut n = 0usize;
        let mut from = 0usize;
        while let Some(at) = zone[from..].find(needle) {
            let abs = from + at;
            let end = abs + needle.len();
            if contains_word(
                &zone[abs.saturating_sub(1)..(end + 1).min(zone.len())],
                needle,
            ) {
                n += 1;
            }
            from = end;
        }
        if n > 0 {
            hits.insert(file, n);
        }
    }
    hits
}

const MAPPER: &str = "quilltap-core/src/services/dangerous_content/legacy_concierge_settings.rs";
const SETTINGS_PUT: &str = "quilltap-core/src/api/settings.rs";
const SETTINGS_REPO: &str = "quilltap-core/src/db/chat_settings.rs";

#[test]
fn no_retired_concierge_identifier_survives() {
    for ident in [
        "DangerousContentSettings",
        "DangerSettings",
        "danger_settings",
        "danger_mode_off",
        "resolve_dangerous_content_settings",
        "resolve_danger_settings_for_chat",
    ] {
        let hits = census(ident);
        assert!(
            hits.is_empty(),
            "the retired `{ident}` is back in production code: {hits:?}"
        );
    }
}

#[test]
fn the_retired_wire_keys_are_named_only_by_the_mapper_and_the_refusal() {
    for key in [
        "dangerousContentSettings",
        "uncensoredImageDescriptionProfileId",
    ] {
        let hits = census(key);
        let files: Vec<&str> = hits.keys().map(String::as_str).collect();
        assert_eq!(
            files,
            [SETTINGS_PUT, MAPPER],
            "`{key}` must be named only by the settings PUT's refusal and the legacy \
             mapper; found {hits:?}"
        );
        // The refusal names each key twice (the presence test + the reported
        // name) and nothing else in the settings module reads it.
        assert_eq!(hits[SETTINGS_PUT], 2, "`{key}` in {SETTINGS_PUT}: {hits:?}");
    }
    // …and the mapper actually reads them (the comparand bites).
    assert!(census("dangerousContentSettings")[MAPPER] >= 1);
    assert!(census("uncensoredImageDescriptionProfileId")[MAPPER] >= 1);
}

#[test]
fn the_cheap_llm_crafter_is_named_only_by_the_mapper_the_refusal_and_the_strip() {
    // The crafter's retired home is the cheap-LLM bag: the dotted name the
    // refusal reports, the key the repository strips, the key the mapper reads.
    let dotted = census("cheapLLMSettings.imagePromptProfileId");
    assert_eq!(
        dotted.keys().map(String::as_str).collect::<Vec<_>>(),
        [SETTINGS_PUT],
        "{dotted:?}"
    );
    // Every production line that names `imagePromptProfileId` next to the
    // cheap-LLM bag.
    let mut near: BTreeMap<String, usize> = BTreeMap::new();
    for (file, zone) in production_files() {
        let lower = zone.to_ascii_lowercase();
        let mut from = 0usize;
        while let Some(at) = zone[from..].find("imagePromptProfileId") {
            let abs = from + at;
            let window_start = abs.saturating_sub(200);
            let window_start = (0..=window_start)
                .rev()
                .find(|i| zone.is_char_boundary(*i))
                .unwrap_or(0);
            if lower[window_start..abs].contains("cheap") {
                *near.entry(file.clone()).or_default() += 1;
            }
            from = abs + "imagePromptProfileId".len();
        }
    }
    let mut files: Vec<&str> = near.keys().map(String::as_str).collect();
    files.sort_unstable();
    for f in &files {
        assert!(
            [SETTINGS_PUT, MAPPER, SETTINGS_REPO].contains(f),
            "{f} names the crafter beside the cheap-LLM bag — its home is \
             `conciergeSettings.imagePromptProfileId` since v4 `3b463d6b1`: {near:?}"
        );
    }
    // The three sanctioned sites are all present (the comparand bites).
    for f in [SETTINGS_PUT, MAPPER, SETTINGS_REPO] {
        assert!(
            near.contains_key(f),
            "{f} no longer handles the retired crafter: {near:?}"
        );
    }
}
