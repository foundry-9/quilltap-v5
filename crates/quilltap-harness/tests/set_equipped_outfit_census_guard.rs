//! The equipped-slot WRITE census (P4.D262 R-A — v4 `3ee3b1342`, the wear
//! ledger's fence).
//!
//! **Every equipped-slot write passes through ONE chokepoint.** v4's
//! `commitEquippedOutfit` (`wardrobe-wear.repository.ts`) reads the prior
//! slots, writes the next ones, and credits the garments the write newly put
//! on; a handler that called the slot writer directly would change clothes
//! without the ledger ever hearing of it. v4 fences that with a jest test
//! (`equip-chokepoint-fence.test.ts:18-23`) that greps `lib/` + `app/` for
//! `setEquippedOutfit(` and sanctions exactly two files: the definition
//! (`chats.repository.ts`) and the chokepoint.
//!
//! v5's fence holds the same INTENT over `crates/quilltap-core/src`'s
//! production zone (comments, string literals and `#[cfg(test)]` items
//! stripped by the shared `source_census` lexer): the files that spell
//! `set_equipped_outfit(` ARE this census — `db/chats_outfits.rs` (the
//! definition, and no call) and `services/wardrobe_wear_commit.rs` (the
//! chokepoint, one call). A new site is a red here, by name. (v4's
//! `METHOD_OVERRIDES` half of the fence — the job child buffering the
//! chokepoint whole — has no v5 analog: there is no child process.)
//!
//! Run standalone:
//!   cargo test -p quilltap-harness --test set_equipped_outfit_census_guard

mod source_census;

use source_census::{code_only, core_src_root, production_zone, rust_sources};

const IDENT: &str = "set_equipped_outfit(";

/// `(path under crates/quilltap-core/src, definitions, calls, why)`.
const CENSUS: &[(&str, usize, usize, &str)] = &[
    (
        "db/chats_outfits.rs",
        1,
        0,
        "the slot writer's definition (v4 `ChatsRepository.setEquippedOutfit`) — \
         its own module never calls it.",
    ),
    (
        "services/wardrobe_wear_commit.rs",
        0,
        1,
        "the chokepoint `commit_equipped_outfit` (v4 `WardrobeWearRepository.\
         commitEquippedOutfit`): the ONE sanctioned caller.",
    ),
];

/// `(definitions, calls)` of [`IDENT`] in a code-only zone. A match must not
/// continue an identifier on its left (`xset_equipped_outfit(` is not ours); a
/// match preceded by `fn` is the definition.
fn count(code: &str) -> (usize, usize) {
    let (mut defs, mut calls) = (0usize, 0usize);
    let mut from = 0usize;
    while let Some(at) = code[from..].find(IDENT) {
        let abs = from + at;
        let prev_ok = abs == 0
            || !code.as_bytes()[abs - 1].is_ascii_alphanumeric()
                && code.as_bytes()[abs - 1] != b'_';
        if prev_ok {
            if code[..abs].trim_end().ends_with("fn") {
                defs += 1;
            } else {
                calls += 1;
            }
        }
        from = abs + IDENT.len();
    }
    (defs, calls)
}

fn census_view() -> Vec<(String, usize, usize)> {
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
        let (defs, calls) = count(&code_only(&production_zone(&src)));
        if defs + calls > 0 {
            out.push((rel, defs, calls));
        }
    }
    out
}

#[test]
fn only_the_chokepoint_writes_equipped_slots() {
    let view = census_view();
    let got: Vec<(&str, usize, usize)> =
        view.iter().map(|(p, d, c)| (p.as_str(), *d, *c)).collect();
    let want: Vec<(&str, usize, usize)> = CENSUS.iter().map(|(p, d, c, _)| (*p, *d, *c)).collect();
    assert_eq!(
        got, want,
        "the production files spelling `set_equipped_outfit(` moved. Every equipped-slot \
         write goes through `services::wardrobe_wear_commit::commit_equipped_outfit` (v4 \
         `3ee3b1342`'s chokepoint), which credits the wear ledger — route the new site \
         through it rather than adding a row here."
    );
}

/// The counter sees code only: a comment, a string, a `#[cfg(test)]` module
/// and a longer identifier do not count; a call and a definition do.
#[test]
fn the_counter_sees_only_code() {
    let src = r#"
        // repo.set_equipped_outfit(a, b, c) in a comment
        const S: &str = "set_equipped_outfit(";
        pub fn set_equipped_outfit(&self) {}
        fn other_set_equipped_outfit(x: u8) {}
        fn site() { repo.set_equipped_outfit(&a, &b, &c); }
        #[cfg(test)]
        mod tests { fn t() { repo.set_equipped_outfit(&a, &b, &c); } }
    "#;
    let code = code_only(&production_zone(src));
    assert_eq!(count(&code), (1, 1));
}
