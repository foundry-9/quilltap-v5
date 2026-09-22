//! The swipe's inform re-apply handle — source census (P4.D205, v4
//! `e7d77bb60`; written at the `f45a517a9` unification, §3 finding 6).
//!
//! `services/regenerate_swipe.rs` carries an OUT-OF-MANDATE fence: P4.D205 put
//! the swipe's inform re-apply ids there, and P4.D207 owns the same file and
//! rewrites the generation below it as a watched stream. The fence's own comment
//! promises that *"`chat_informs_swipe_handle` in the harness is the source
//! census that fails if it does not [survive]"* — and until now no such test
//! existed, so the promise was the only thing guarding a cross-lane handoff that
//! a rewrite can drop in silence.
//!
//! Why a SOURCE census and not a behavioural test: what is being guarded is a
//! three-part wiring, and the only part a behavioural test can reach cheaply is
//! the last one. A rewrite that computed the ids and then forgot to pass them
//! would leave every re-roll of an informed line without its passage — visible
//! only as a model that has quietly stopped knowing something, on a path that
//! needs a live generation to exercise. The three assertions below are the
//! wiring, held by name.
//!
//! ## Mutation table
//!
//! Each row was applied to `regenerate_swipe.rs`, run, and reverted from a file
//! backup. All three **still compile** — a mutation that only breaks the build
//! proves nothing about a census.
//!
//! | mutation | reddens |
//! |---|---|
//! | delete the first `// === P4.D205 OUT-OF-MANDATE …` header (a comment — builds) | all three (the fence count drops to 1, and the second block can no longer be located) |
//! | replace the computed block with `vec![<target id>]`, keeping the binding | `the_ids_are_computed_from_the_swipe_group_inside_the_fence` only |
//! | `regeneration_of_message_ids: Some(…)` → `: None`, keeping the binding used | `the_ids_reach_the_context_build` only |
//!
//! Run standalone:
//!   cargo test -p quilltap-harness --test chat_informs_swipe_handle

use std::path::PathBuf;

/// The fence header P4.D205 left and P4.D207 must preserve. It appears TWICE in
/// the file — once around the computation, once around the field — and both
/// occurrences are load-bearing (see the two assertions below).
const FENCE: &str = "// === P4.D205 OUT-OF-MANDATE — P4.D207 preserves ===";
const FENCE_END: &str = "// === end P4.D205 OUT-OF-MANDATE ===";

fn source() -> String {
    let path: PathBuf = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("the harness crate sits two levels under the repo root")
        .join("crates/quilltap-core/src/services/regenerate_swipe.rs");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// The text between the `n`-th fence header and the fence end that follows it.
fn fenced_block(text: &str, n: usize) -> String {
    let start = text
        .match_indices(FENCE)
        .nth(n)
        .unwrap_or_else(|| panic!("regenerate_swipe.rs has fewer than {} fence headers", n + 1))
        .0;
    let rest = &text[start..];
    let end = rest
        .find(FENCE_END)
        .expect("every P4.D205 fence header is closed by its end marker");
    rest[..end].to_string()
}

#[test]
fn the_out_of_mandate_fence_is_present() {
    let text = source();
    assert_eq!(
        text.matches(FENCE).count(),
        2,
        "`regenerate_swipe.rs` must keep BOTH P4.D205 fences — one around the \
         re-apply ids' computation, one around the field that carries them into \
         the context build. A rewrite of this file that drops either has dropped \
         the swipe's inform re-apply, which no other test in the tree can see."
    );
    assert_eq!(
        text.matches(FENCE_END).count(),
        2,
        "each fence header must still be closed"
    );
}

#[test]
fn the_ids_are_computed_from_the_swipe_group_inside_the_fence() {
    let text = source();
    let block = fenced_block(&text, 0);
    assert!(
        block.contains("let regeneration_of_message_ids: Vec<String> = {"),
        "the re-apply ids must still be COMPUTED inside the first fence; found:\n{block}"
    );
    // The set is "the target plus every message in its swipe group,
    // deduplicated, first-seen order" (v4 `regenerate-swipe.service.ts:146-155`).
    // Losing the group leg would silently narrow it to the target alone, which
    // still compiles and still passes a re-roll — and re-applies nothing that
    // the SIBLING swipes consumed.
    assert!(
        block.contains("existing_swipe_group_id"),
        "the computation must still read the target's swipe group; found:\n{block}"
    );
    assert!(
        block.contains("swipeGroupId"),
        "the computation must still match siblings on `swipeGroupId`; found:\n{block}"
    );
    assert!(
        block.contains("seen.insert("),
        "the ids must still be deduplicated first-seen (v4's `[...new Set([...])]`); \
         found:\n{block}"
    );
}

#[test]
fn the_ids_reach_the_context_build() {
    let text = source();
    let block = fenced_block(&text, 1);
    assert!(
        block.contains("regeneration_of_message_ids: Some(regeneration_of_message_ids),"),
        "the computed ids must still be PASSED into the context build — a \
         computation nothing reads is the exact shape a stream rewrite leaves \
         behind, and it compiles; found:\n{block}"
    );
}
