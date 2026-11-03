//! The trigger-phrase sites' SOURCE ORDER (P4.D305, v4 `362426c8f`).
//!
//! v4 pins its two sites with three source-text assertions
//! (`__tests__/unit/lib/services/chat-message/trigger-phrase-sites.test.ts`):
//! `ledger.userMessagePersisted = true` precedes `await
//! runUserMessageTriggers(` in the orchestrator; `saveAssistantMessage(`
//! precedes `await runReplyTriggers(` in the finalizer; and
//! `regenerate-swipe.service.ts` never mentions the runner. The cheapest twin
//! of a source-text assertion is a source-text assertion, over v5's sources:
//!
//! - `services/operator_post.rs` (P4.D348 — v4 `a8214b253` moved the site into
//!   `recordOperatorPost`): the USER-ROW persist's `user_message_persisted:
//!   true` (the one followed by the `[OperatorPost] User message persisted`
//!   DEBUG) precedes `run_user_message_triggers(`, which precedes the Carina
//!   markup pass `run_carina_markup_query(`; and `services/orchestrator.rs`
//!   records the post (`record_operator_post(`) BEFORE the held-post return
//!   (`finish_held_post(` — v4's held return is BELOW its recorder, so a held
//!   turn still fires);
//! - `services/message_finalizer.rs`: `save_assistant_message(` precedes
//!   `run_reply_triggers(`, which precedes both `run_assistant_carina(` and
//!   `ChatEvent::done(`;
//! - `services/regenerate_swipe.rs` names none of `run_user_message_triggers`,
//!   `run_reply_triggers`, `run_triggered_custom_tools`.
//!
//! Run standalone:
//!   cargo test -p quilltap-harness --test trigger_sites_order_guard

use std::path::PathBuf;

fn source(rel: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../quilltap-core/src")
        .join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
}

/// The byte offset of the ONE occurrence of `needle` (a second one would make
/// the order ambiguous, so it is refused).
fn only(text: &str, needle: &str, file: &str) -> usize {
    let hits: Vec<usize> = text.match_indices(needle).map(|(i, _)| i).collect();
    assert_eq!(
        hits.len(),
        1,
        "{file}: expected exactly one `{needle}`, found {}",
        hits.len()
    );
    hits[0]
}

/// The recorder's user-row persist literal: the LAST
/// `user_message_persisted: true` before the persist's own DEBUG text.
fn persist_literal(text: &str) -> usize {
    let debug = only(
        text,
        "\"[OperatorPost] User message persisted\",",
        "operator_post.rs",
    );
    text[..debug]
        .rfind("user_message_persisted: true")
        .expect("operator_post.rs: the user-row persist flips `user_message_persisted`")
}

#[test]
fn the_user_message_site_follows_the_persist_and_precedes_carina() {
    let text = source("services/operator_post.rs");
    let persisted = persist_literal(&text);
    let site = only(&text, "run_user_message_triggers(", "operator_post.rs");
    let carina = only(&text, "run_carina_markup_query(", "operator_post.rs");
    assert!(
        persisted < site,
        "the trigger site must follow the user-row persist"
    );
    assert!(
        site < carina,
        "the trigger site must precede the Carina markup pass"
    );
}

#[test]
fn the_orchestrator_records_the_post_before_the_held_return() {
    let text = source("services/orchestrator.rs");
    // The main path's recorder call is the LAST of the two (the fair-rotation
    // pause records earlier, in `maybe_pause_for_user_seat_turn`).
    let recorded = text
        .rfind("record_operator_post(")
        .expect("orchestrator.rs: the main path records the post");
    let held = only(&text, "finish_held_post(", "orchestrator.rs");
    assert!(
        recorded < held,
        "the post must be recorded before the held-post return"
    );
    assert!(
        !text.contains("run_user_message_triggers("),
        "the trigger site lives in the recorder, not the orchestrator"
    );
}

#[test]
fn the_reply_site_follows_the_save_and_precedes_carina_and_done() {
    let text = source("services/message_finalizer.rs");
    let saved = only(&text, "save_assistant_message(", "message_finalizer.rs");
    let site = only(&text, "run_reply_triggers(", "message_finalizer.rs");
    let carina = only(&text, "run_assistant_carina(", "message_finalizer.rs");
    let done = only(&text, "ChatEvent::done(", "message_finalizer.rs");
    assert!(saved < site, "the reply site must follow the saved reply");
    assert!(
        site < carina,
        "the reply site must precede the Carina markup path"
    );
    assert!(site < done, "the reply site must precede the done event");
}

#[test]
fn a_swipe_never_names_the_runner() {
    let text = source("services/regenerate_swipe.rs");
    for name in [
        "run_user_message_triggers",
        "run_reply_triggers",
        "run_triggered_custom_tools",
    ] {
        assert!(
            !text.contains(name),
            "regenerate_swipe.rs must not reach the trigger runner (`{name}`)"
        );
    }
}
