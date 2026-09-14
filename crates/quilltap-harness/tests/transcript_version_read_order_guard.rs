//! **P4.D183 — the counter is read BEFORE the projection, never after.**
//!
//! v4 spends a paragraph on this and gives it a test of its own ("reads the
//! counter before projecting, never after"), because getting it backwards is
//! silently wrong rather than loudly wrong:
//!
//! > The pairing the caller stores must never claim a version newer than the
//! > rows beside it: a version read after the projection could have moved on,
//! > and the tab would then be answered "unchanged" for a write it has not
//! > seen. Reading first can only cost an extra round trip later, which is the
//! > harmless direction.
//!
//! **No differential can see the order.** Both readings produce byte-identical
//! bodies on any sequential corpus — the counter only moves between the two
//! reads if a WRITE lands in that window, and a differential drives one
//! operation at a time. Reproducing the race would need a writer scheduled into
//! the gap, which the harness has no seam for; so the order is pinned
//! STRUCTURALLY, the way the order allows, by reading the source.
//!
//! The same census covers the short-circuit's other half: on the `unchanged`
//! path the projection must not be called AT ALL. That half IS behavioural and
//! is pinned by `transcript_route_equivalence`'s
//! `unchanged_without_projecting` case (the body carries two keys and no
//! `messages`); this file pins the source shape that makes it structural
//! rather than incidental.
//!
//! Run standalone (no oracle):
//!   cargo test -p quilltap-harness --test transcript_version_read_order_guard

use std::path::PathBuf;

fn source() -> String {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(std::path::Path::parent)
        .expect("repo root")
        .join("crates/quilltap-core/src/api/chat_transcript.rs");
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("read {}: {e}", p.display()))
}

/// Strip doc comments and line comments so the prose ABOUT the order (which
/// names both calls, in either direction) cannot satisfy the census.
fn code_only(src: &str) -> String {
    src.lines()
        .map(|l| {
            let t = l.trim_start();
            if t.starts_with("//") {
                ""
            } else {
                l
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn the_version_is_read_before_the_projection() {
    let src = code_only(&source());
    let version_at = src
        .find("get_transcript_version(")
        .expect("`chat_transcript` must read the counter through `get_transcript_version`");
    let project_at = src
        .find("project_chat_transcript(")
        .expect("`chat_transcript` must project through `project_chat_transcript`");
    assert!(
        version_at < project_at,
        "the transcript handler reads the counter AFTER projecting (version at \
         byte {version_at}, projection at {project_at}). v4 reads it first on \
         purpose: a version newer than the rows handed out with it has the tab \
         answered \"unchanged\" for a message it never received. No differential \
         can catch this — both orders produce identical bodies unless a write \
         lands between them."
    );

    // …and exactly once each, so the census cannot be satisfied by an extra
    // call added later in the function while the real read moved down.
    assert_eq!(
        src.matches("get_transcript_version(").count(),
        1,
        "the counter should be read exactly once in this handler"
    );
    assert_eq!(
        src.matches("project_chat_transcript(").count(),
        1,
        "the transcript should be projected exactly once in this handler"
    );
}

/// The short-circuit's structural half: the `unchanged` return must come
/// BEFORE the projection call, so an agreeing counter cannot pay for a
/// serialization it then throws away.
#[test]
fn the_unchanged_return_precedes_the_projection() {
    let src = code_only(&source());
    let unchanged_at = src
        .find("\"unchanged\": true")
        .expect("the handler must be able to answer `unchanged: true`");
    let project_at = src
        .find("project_chat_transcript(")
        .expect("projection call");
    assert!(
        unchanged_at < project_at,
        "the `unchanged` answer is built AFTER the projection — the whole point \
         of the conditional is that an agreeing counter serializes nothing"
    );
}
