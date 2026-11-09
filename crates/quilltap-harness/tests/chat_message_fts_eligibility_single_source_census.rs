//! P4.D363 Tier 2 item 14 — the chat-message FTS eligibility rule has ONE
//! home. v4's `chatMessageFtsEligibilitySql` "lives here so the triggers, the
//! counts and the rebuild cannot drift apart"; this census makes that
//! mechanical for v5: no PRODUCTION string literal outside
//! `db/chat_message_fts.rs` may spell the fragment's quoted-identifier
//! conjuncts — `"role" IN ('USER','ASSISTANT')` or `"systemSender" IS NULL AND`
//! — so every reader (the triggers, the counts, the rebuild, both search
//! arms) must call `chat_message_fts_eligibility_sql`.
//!
//! The twin predicate's own SQL mirror
//! (`chat_activity::CHARACTER_AUTHORED_MESSAGE_FILTER`, unquoted identifiers,
//! `('USER', 'ASSISTANT')` with a space) is v4's separate
//! `CHARACTER_AUTHORED_MESSAGE_FILTER` and is deliberately NOT matched.
//!
//! Measured at `SCHEMA` (`4e3a1584e`), before the port: 0 hits outside the
//! module. Mutation proof: a production literal spelling
//! `\"role\" IN ('USER','ASSISTANT')` in another `db/` file reddens it.

mod source_census;

use source_census::{core_src_root, production_zone, rust_sources, string_literals};

const HOME: &str = "db/chat_message_fts.rs";
const NEEDLES: [&str; 2] = [
    r#""role" IN ('USER','ASSISTANT')"#,
    r#""systemSender" IS NULL AND"#,
];

#[test]
fn the_eligibility_fragment_has_one_home() {
    let root = core_src_root();
    let mut files = Vec::new();
    rust_sources(&root, &mut files);
    files.sort();
    let mut offenders = Vec::new();
    let mut home_hits = 0usize;
    for file in &files {
        let rel = file
            .strip_prefix(&root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        let text = std::fs::read_to_string(file).unwrap();
        let zone = production_zone(&text);
        for lit in string_literals(&zone) {
            // A cooked literal escapes its quotes; a raw one does not.
            let unescaped = lit.replace("\\\"", "\"");
            for needle in NEEDLES {
                if unescaped.contains(needle) {
                    if rel == HOME {
                        home_hits += 1;
                    } else {
                        offenders.push(format!("{rel}: {needle}"));
                    }
                }
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "the FTS eligibility rule is spelled outside {HOME} — call \
         `chat_message_fts_eligibility_sql` instead: {offenders:#?}"
    );
    // The census is not vacuous: the home itself spells both (the fragment
    // and the insert trigger's `WHEN`).
    assert!(
        home_hits >= 2,
        "the census found the home only {home_hits} times"
    );
}
