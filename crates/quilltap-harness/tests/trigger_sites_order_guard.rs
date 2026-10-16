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
//! - `services/orchestrator.rs`: the USER-ROW persist's
//!   `user_message_persisted: true` (the one followed by the `[Orchestrator]
//!   User message persisted` DEBUG — NOT the fair-rotation pause's) precedes
//!   `run_user_message_triggers(`, which precedes the paused-chat hold's
//!   `if hold_for_paused_chat {` (v4's paused return is BELOW its site, so a
//!   held turn still fires);
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

/// The orchestrator's user-row persist literal: the LAST
/// `user_message_persisted: true` before the persist's own DEBUG text.
fn persist_literal(text: &str) -> usize {
    let debug = only(
        text,
        "\"[Orchestrator] User message persisted\",",
        "orchestrator.rs",
    );
    text[..debug]
        .rfind("user_message_persisted: true")
        .expect("orchestrator.rs: the user-row persist flips `user_message_persisted`")
}

#[test]
fn the_user_message_site_follows_the_persist_and_precedes_the_hold() {
    let text = source("services/orchestrator.rs");
    let persisted = persist_literal(&text);
    let site = only(&text, "run_user_message_triggers(", "orchestrator.rs");
    // The hold that follows the site (the fair-rotation pause has its own,
    // earlier `if hold_for_paused_chat` above the persist).
    let hold = text[site..]
        .find("if hold_for_paused_chat {")
        .map(|i| site + i)
        .expect("orchestrator.rs: the paused-chat hold below the trigger site");
    assert!(
        persisted < site,
        "the trigger site must follow the user-row persist"
    );
    assert!(
        site < hold,
        "the trigger site must precede the paused-chat hold"
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
