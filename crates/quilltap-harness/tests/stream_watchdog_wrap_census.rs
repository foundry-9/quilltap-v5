//! The stall-watchdog wrap census (P4.D189 — v4 `f90144ac4`, bug 141).
//!
//! v4 has ONE funnel: every Salon-side consumer — the primary stream, the
//! provider failover, recovery, both tool loops, the turn orchestrator, the
//! Courier, the danger orchestrator, Carina, both Brahma services and the
//! help-chat orchestrator — calls `streaming.service.ts`'s `streamMessage`,
//! which wraps the provider ONCE. v5 has no funnel: each consumer drives the
//! [`StreamingCompletionProvider`](quilltap_core::model::stream::StreamingCompletionProvider)
//! seam itself, so v4's two wrapped `provider.streamMessage(` call sites are
//! ELEVEN wrap sites here.
//!
//! An unwrapped twelfth is invisible to every differential in the repo: a
//! canned sequence is pre-pushed into its channel and CLOSES, so `recv()` never
//! pends and the watchdog never has anything to fire on. Nothing but a census
//! can see the wire. Hence the `db_error_key_guard` idiom: per-file exact pairs
//! over the production zone, plus the assertion that the census file list IS
//! the set of files reaching the seam.
//!
//! **The one site that is NOT on the Salon's budgets:** `services/initial_
//! greeting.rs` (P4.D190) wraps on the greeting's own 90 s/60 s constants with
//! `context: "initial-greeting"`, exactly as v4's `initial-greeting.ts` passes
//! `withStallWatchdog` its own options. The count census treats it like any
//! other consumer; the budget census below pins it to ITS constants instead of
//! `StallBudgets::default()`. (P4.D189 listed it at zero wraps; the `ffb6b3119`
//! round's unification moved the row when P4.D190 landed beside it.)
//!
//! Run standalone:
//!   cargo test -p quilltap-harness --test stream_watchdog_wrap_census

use std::path::{Path, PathBuf};

/// `(path under `crates/quilltap-core/src`, production `.stream_message(`
/// calls, production `watch_stream(` calls, why)`.
const CENSUS: &[(&str, usize, usize, &str)] = &[
    (
        "model/stream.rs",
        1,
        0,
        "the blanket `impl StreamingCompletionProvider for Arc<T>` delegating to \
         the inner value — the seam itself, not a consumer",
    ),
    (
        "services/primary_stream.rs",
        1,
        1,
        "`consume_stream`, v4's primary-stream.service.ts:197 AND its \
         tool-unsupported retry at :261 (one v5 function, called twice)",
    ),
    (
        "services/provider_failover.rs",
        1,
        1,
        "`restream_into` — v4 provider-failover.service.ts:482",
    ),
    (
        "services/recovery.rs",
        1,
        1,
        "`drain_recovery_stream` — v4 recovery.service.ts:303",
    ),
    (
        "services/native_tool_loop.rs",
        2,
        2,
        "the tool re-stream (v4 :340) and the force-final re-stream (v4 :421)",
    ),
    (
        "services/text_tool_loop.rs",
        1,
        1,
        "`stream_continuation` — v4 text-tool-loop.service.ts:390",
    ),
    (
        "services/carina_query.rs",
        1,
        1,
        "`run_stream` — v4 carina.service.ts:676",
    ),
    (
        "services/help_chat/orchestrator.rs",
        1,
        1,
        "`stream_turn` — v4 help-chat/orchestrator.service.ts:361",
    ),
    (
        "services/brahma_console/mod.rs",
        1,
        1,
        "the one-shot `run_stream` — v4 brahma-console/one-shot.service.ts:211",
    ),
    (
        "services/brahma_console/orchestrator.rs",
        1,
        1,
        "`stream_turn` — v4 brahma-console/orchestrator.service.ts:343",
    ),
    (
        "services/initial_greeting.rs",
        1,
        1,
        "`generate_greeting_message` — v4 lib/chat/initial-greeting.ts:174, the \
         one consumer OUTSIDE v4's funnel, on the greeting's own 90 s/60 s \
         budgets (P4.D190; the count moved from zero at the round's unification)",
    ),
];

const CALL: &str = ".stream_message(";
const WRAP: &str = "watch_stream(";

fn core_src_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("the harness crate sits two levels under the repo root")
        .join("crates/quilltap-core/src")
}

fn rust_sources(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = std::fs::read_dir(dir).unwrap_or_else(|e| panic!("read {}: {e}", dir.display()));
    for entry in entries {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            rust_sources(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            out.push(path);
        }
    }
}

/// The file with every `#[cfg(test)]` item removed by brace balance.
///
/// A census that counts the WHOLE file measures the wrong thing twice over: a
/// test module's canned providers call the seam (`model/streaming_provider.rs`
/// has twenty-four such calls and not one production one), and a wiring probe
/// that constructs its own watched stream would paper over an unwrapped
/// production site. Scanned by braces and not by line, because both attributes
/// and bodies are indented arbitrarily.
fn production_zone(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let mut rest = src;
    while let Some(at) = rest.find("#[cfg(test)]") {
        out.push_str(&rest[..at]);
        let after = &rest[at..];
        // The item may be a `mod`, a `fn`, a `use` (no braces) or an `impl`.
        // Find its first `{` before the next `;` at depth 0; a `use` line ends
        // at the semicolon and carries no body.
        let brace = after.find('{');
        let semi = after.find(';');
        match (brace, semi) {
            (Some(b), s) if s.is_none_or(|s| b < s) => {
                let mut depth = 0usize;
                let mut end = None;
                for (offset, ch) in after[b..].char_indices() {
                    match ch {
                        '{' => depth += 1,
                        '}' => {
                            depth -= 1;
                            if depth == 0 {
                                end = Some(b + offset + 1);
                                break;
                            }
                        }
                        _ => {}
                    }
                }
                let end = end.expect("a `#[cfg(test)]` item's braces must balance");
                rest = &after[end..];
            }
            (_, Some(s)) => rest = &after[s + 1..],
            _ => {
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

#[test]
fn every_production_stream_message_call_wears_the_watchdog() {
    let root = core_src_root();
    let mut files = Vec::new();
    rust_sources(&root, &mut files);
    files.sort();
    assert!(
        files.len() > 100,
        "the walk found only {} rust files under quilltap-core/src — it is not \
         reaching the tree",
        files.len()
    );

    let mut failures: Vec<String> = Vec::new();
    let mut seen: Vec<String> = Vec::new();

    for path in &files {
        let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
        if !text.contains(CALL) {
            continue;
        }
        let zone = production_zone(&text);
        let calls = zone.matches(CALL).count();
        if calls == 0 {
            continue; // test-module-only, e.g. model/streaming_provider.rs
        }
        let wraps = zone.matches(WRAP).count();
        let rel = path
            .strip_prefix(&root)
            .expect("under quilltap-core/src")
            .to_string_lossy()
            .replace('\\', "/");
        seen.push(rel.clone());

        match CENSUS.iter().find(|(p, ..)| *p == rel) {
            None => failures.push(format!(
                "{rel}: {calls} production `{CALL}` call(s) and no census row. A \
                 twelfth site reaching the streaming seam must wear \
                 `watch_stream(…)` — a provider that answers with headers and \
                 then goes quiet holds that loop open forever (bug 141) — or be \
                 justified here."
            )),
            Some((_, want_calls, want_wraps, why)) => {
                if calls != *want_calls {
                    failures.push(format!(
                        "{rel}: {calls} production `{CALL}` call(s), census says \
                         {want_calls} ({why}). A new call site needs its own \
                         wrap and its own census row."
                    ));
                }
                if wraps != *want_wraps {
                    failures.push(format!(
                        "{rel}: {wraps} production `{WRAP}` call(s), census says \
                         {want_wraps} ({why}). Every production stream this \
                         file opens must be watched."
                    ));
                }
            }
        }
    }

    for (rel, want_calls, _, why) in CENSUS {
        if !seen.iter().any(|p| p == rel) {
            failures.push(format!(
                "{rel}: census expects {want_calls} production `{CALL}` call(s), \
                 found none. If the consumer moved, move the census with it \
                 ({why})."
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "the stall-watchdog wrap census does not hold:\n  {}",
        failures.join("\n  ")
    );
    eprintln!(
        "OK stream-watchdog wrap census: {} production consumer file(s)",
        seen.len()
    );
}

/// The greeting is v4's ONE consumer outside the funnel, and the one site whose
/// budgets are NOT the defaults (`initial-greeting.ts:10-20`: 90 s to the first
/// chunk, 60 s between chunks, `context: 'initial-greeting'`).
const GREETING_SITE: &str = "services/initial_greeting.rs";

/// The wrap is only a wrap if the budgets are v4's. A site that passed its own
/// `StallBudgets { … }` would satisfy the count census above while quietly
/// buying itself a different deadline — so the Salon-side sites are pinned to
/// `StallBudgets::default()` by name, and the greeting to its OWN two constants
/// and its own `context` (a greeting on the Salon's 240 s would hold the Green
/// Room's undismissable dialog for four minutes instead of ninety seconds).
#[test]
fn every_salon_side_wrap_uses_the_default_budgets() {
    let root = core_src_root();
    let mut failures: Vec<String> = Vec::new();
    for (rel, _, want_wraps, _) in CENSUS {
        if *want_wraps == 0 {
            continue;
        }
        let text =
            std::fs::read_to_string(root.join(rel)).unwrap_or_else(|e| panic!("read {rel}: {e}"));
        let zone = production_zone(&text);
        if *rel == GREETING_SITE {
            for needle in [
                "first_chunk_ms: GREETING_FIRST_CHUNK_TIMEOUT_MS",
                "idle_ms: GREETING_IDLE_TIMEOUT_MS",
                "context: \"initial-greeting\"",
            ] {
                if zone.matches(needle).count() != *want_wraps {
                    failures.push(format!(
                        "{rel}: expected {want_wraps} `{needle}` — the greeting wraps on its \
                         OWN 90 s/60 s constants and its own context, never the Salon's"
                    ));
                }
            }
            if zone.contains("StallBudgets::default()") {
                failures.push(format!(
                    "{rel}: `StallBudgets::default()` present — the greeting must not take \
                     the Salon's 240 s/120 s"
                ));
            }
            continue;
        }
        let defaults = zone.matches("StallBudgets::default()").count();
        if defaults != *want_wraps {
            failures.push(format!(
                "{rel}: {defaults} `StallBudgets::default()` against {want_wraps} \
                 wrap(s) — a Salon-side site must take v4's 240 s/120 s, not a \
                 budget of its own"
            ));
        }
        let context = zone
            .matches("StallWatchdogContext::streaming_service(")
            .count();
        if context != *want_wraps {
            failures.push(format!(
                "{rel}: {context} `StallWatchdogContext::streaming_service(` \
                 against {want_wraps} wrap(s) — v4's one funnel stamps \
                 `context: 'streaming.service'` on every Salon-side log line"
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "a Salon-side wrap is not on v4's budgets:\n  {}",
        failures.join("\n  ")
    );
}
