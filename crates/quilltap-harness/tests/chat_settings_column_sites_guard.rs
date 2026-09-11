//! The `chat_settings` per-column six-sites census (P4.D179, Tier 2 item 10).
//!
//! Adopting one v4 `chat_settings` column means touching the same handful of
//! places in `db/chat_settings.rs` every time: the struct field, the create
//! patch field, the INSERT bind + its column-list entry, the UPDATE assignment,
//! the tolerant SELECT's column-name array, and the read's `obj.insert`. The
//! P4.D73 order called it "the six-site shape"; that claim has been prose in
//! three work orders and has never been executable.
//!
//! Nothing else catches a missed site cheaply. A dropped INSERT entry is
//! invisible to the settings differential (the create branch goes through
//! `tolerant_insert`, which drops unknown columns SILENTLY, and the read then
//! surfaces the Zod default — the row looks right); a dropped UPDATE assignment
//! shows up only as a PUT that answers 200 and changes nothing; a dropped array
//! entry shifts every positional index after it. So this is a source census in
//! the `db_error_key_guard` idiom: for each adopted column name, assert it
//! appears in each of the named regions of the file.
//!
//! The census is deliberately per-REGION rather than a bare occurrence count —
//! a count floor passes when one site is duplicated and another is missing.
//!
//! Run standalone:
//!   cargo test -p quilltap-harness --test chat_settings_column_sites_guard

use std::path::PathBuf;

/// The columns whose adoption this guard pins. Not every `chat_settings` column
/// (the JSON-object bags have their own shape) — the plain boolean columns
/// adopted one at a time from a v4 migration, which are the ones that keep
/// arriving and keep taking the same six edits.
const ADOPTED_BOOLEAN_COLUMNS: &[&str] = &[
    // P4.d (pre-4.8)
    "autoDetectRng",
    "customTools",
    "compositionModeDefault",
    "composerSpellcheck",
    // P4.D73 (v4 4.8.2)
    "composerEmoji",
    "composerUnicode",
    // P4.D179 (v4 4.10 `686954937`)
    "impersonationVoiceRewrite",
    // pre-existing
    "textReplacementsEnabled",
    "autoScrollOnResponseComplete",
];

fn source() -> String {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../quilltap-core/src/db/chat_settings.rs");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
}

/// Slice the file between two anchors, both of which must be unique.
fn region<'a>(src: &'a str, name: &str, start: &str, end: &str) -> &'a str {
    assert_eq!(
        src.matches(start).count(),
        1,
        "the `{name}` region's START anchor is no longer unique — re-anchor the census, \
         do not delete it"
    );
    let from = src.find(start).unwrap();
    let rest = &src[from..];
    let to = rest.find(end).unwrap_or_else(|| {
        panic!("the `{name}` region's END anchor was not found after its start")
    });
    &rest[..to]
}

#[test]
fn every_adopted_column_reaches_all_six_sites() {
    let src = source();

    // 1. The INSERT's column list (inside `tolerant_insert`'s slice array).
    let insert = region(
        &src,
        "INSERT column list",
        "crate::db::tolerant_insert(\n            self.conn,",
        "\n    }\n",
    );
    // 2. The UPDATE assignment builder.
    let update = region(
        &src,
        "UPDATE assignments",
        "if let Some(auto_detect_rng) = patch.auto_detect_rng",
        "if let Some(timezone) = &patch.timezone",
    );
    // 3. The tolerant SELECT's column-name array + 4. the read's `obj.insert`s —
    //    one region, since the array immediately precedes the marshaling.
    let read = region(
        &src,
        "tolerant SELECT + read",
        "let cols = crate::db::tolerant_select_list(",
        "\npub fn find_auto_housekeeping_settings_by_user_id",
    );

    let mut missing: Vec<String> = Vec::new();
    for col in ADOPTED_BOOLEAN_COLUMNS {
        let quoted = format!("\"{col}\"");
        // The UPDATE builder spells the column inside a `format!` SQL fragment
        // (`"composerUnicode = ?{}"`), so the quoted-name needle would never
        // match there; its needle is the assignment itself.
        let assignment = format!("{col} = ?");
        for (region_name, body, needle, want) in [
            ("the INSERT column list", insert, quoted.as_str(), 1usize),
            (
                "the UPDATE assignment builder",
                update,
                assignment.as_str(),
                1,
            ),
            // The array entry AND the `obj.insert` key both spell the quoted
            // name, so the read region must carry it twice — which is what
            // makes a dropped array entry (an index shift) visible here.
            ("the tolerant SELECT + read", read, quoted.as_str(), 2),
        ] {
            let n = body.matches(needle).count();
            if n < want {
                missing.push(format!("{col}: {region_name} has {n}, expected >= {want}"));
            }
        }
    }
    assert!(
        missing.is_empty(),
        "chat_settings column sites missing — adopting a column takes the SAME six edits \
         every time:\n  {}",
        missing.join("\n  ")
    );

    // The struct + patch fields are the two sites the quoted-name census cannot
    // see (they are snake_case Rust identifiers), so they get their own pin for
    // the column this guard was written for.
    assert!(
        src.contains("pub impersonation_voice_rewrite: bool,"),
        "the ChatSettingsCreate field is missing"
    );
    assert!(
        src.contains("pub impersonation_voice_rewrite: Option<bool>,"),
        "the ChatSettingsUpdate patch field is missing"
    );

    eprintln!(
        "OK: {} adopted columns reach every census region.",
        ADOPTED_BOOLEAN_COLUMNS.len()
    );
}
