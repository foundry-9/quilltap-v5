//! The participant-status parser census (P4.74 — the round's §C contract).
//!
//! §C: **participant-status parsing has ONE home**,
//! `quilltap_core::chat_predicates::participant_status_from_str`, whose rule is
//! v4's: `None`/`"active"` → Active, `"silent"` → Silent, `"removed"` →
//! Removed, **anything else → Absent**. That last arm is the load-bearing one —
//! v4 never parses a status at all, it tests `isParticipantPresent(status)` =
//! `status === 'active' || status === 'silent'`, so an unrecognised string is
//! simply not present.
//!
//! Private copies of that mapping kept regrowing (P4.68 consolidated a batch and
//! measured, but could not own, the rest; P4.74 retired two more). Two rules
//! drifted in the copies, in opposite directions:
//!
//! - `_ => Active` — the OLD rule, which makes an unknown status *present*
//!   where v4 makes it absent;
//! - `other => panic!(…)` — STRICTER than v4, which has no unknown case: a
//!   corpus row with a new spelling aborts the binary instead of measuring
//!   v4's answer.
//!
//! So this test walks `crates/**/*.rs` for `=> ParticipantStatus::` — a match
//! arm PRODUCING a status, which is what a parser is — and holds every file
//! against the census below. **The allow-list IS the census:** a new file, or a
//! new arm in a listed file, fails here. The remaining entries are not
//! endorsements; each carries what is still wrong with it and who owns it.
//!
//! Sibling pins: `chat_predicates`'s own unit tests (the canonical rule's
//! bytes) and `message_attribution_equivalence`'s
//! `assert_corpus_status_strings_are_known` (that the corpus cannot smuggle a
//! fifth spelling past the retirement).
//!
//! Run standalone:
//!   cargo test -p quilltap-harness --test participant_status_home_guard

use std::path::{Path, PathBuf};

/// `(repo-relative path, expected arms, why this file still matches)`.
const CENSUS: &[(&str, usize, &str)] = &[
    (
        "crates/quilltap-core/src/chat_predicates.rs",
        4,
        "THE HOME — `participant_status_from_str`, v4's rule verbatim",
    ),
    (
        "crates/quilltap-core/src/skip_signal.rs",
        4,
        "DELIBERATE: `participant_is_present` must NOT default a missing \
         status (an absent field returns false outright), which the canonical \
         reader's `unwrap_or(\"active\")` would undo. Its own doc comment \
         carries the reason; measured and kept by P4.68",
    ),
    (
        "crates/quilltap-core/src/db/chats_messages.rs",
        4,
        "OPEN — `participants_from_chat` still carries the OLD `_ => Active` \
         rule, and unlike the copy P4.74 retired from `answer_confirmation` \
         this one's status IS read: it builds `turn_state::ParticipantView`, \
         whose `is_active_character` calls `is_participant_present`. So an \
         unknown status would count as present here where v4 counts it \
         absent. LATENT today — v4's Zod schema constrains the column to the \
         four spellings, so no real row can reach the arm — and OUTSIDE \
         P4.74's ownership (its Ownership row grants `answer_confirmation.rs` \
         only). Recorded for its owning lane; do not fix it here",
    ),
    (
        "crates/quilltap-harness/tests/select_speaker_equivalence.rs",
        4,
        "OPEN — harness-side copy with the `panic!` rule, same shape as the \
         one P4.74 retired from `message_attribution_equivalence`; not in \
         P4.74's ownership (§C names two copies)",
    ),
    (
        "crates/quilltap-harness/tests/small_utils_equivalence.rs",
        4,
        "OPEN — harness-side copy with the `panic!` rule (as above)",
    ),
    (
        "crates/quilltap-harness/tests/system_prompt_equivalence.rs",
        4,
        "OPEN — harness-side copy with the `panic!` rule (as above)",
    ),
    (
        "crates/quilltap-harness/tests/turn_pause_filters_equivalence.rs",
        4,
        "OPEN — harness-side copy with the `panic!` rule (as above)",
    ),
    (
        "crates/quilltap-harness/tests/turn_state_equivalence.rs",
        4,
        "OPEN — harness-side copy with the `panic!` rule (as above)",
    ),
];

const NEEDLE: &str = "=> ParticipantStatus::";

/// This file names the needle in its prose, so it would flag itself; it is the
/// guard, not a parser.
const SELF: &str = "crates/quilltap-harness/tests/participant_status_home_guard.rs";

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("harness crate sits two levels under the repo root")
        .to_path_buf()
}

fn rust_sources(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = std::fs::read_dir(dir).unwrap_or_else(|e| panic!("read {}: {e}", dir.display()));
    for entry in entries {
        let path = entry.expect("dir entry").path();
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        if path.is_dir() {
            if name == "target" || name == "vendor" {
                continue;
            }
            rust_sources(&path, out);
        } else if name.ends_with(".rs") {
            out.push(path);
        }
    }
}

#[test]
fn participant_status_parsing_has_one_home() {
    let root = repo_root();
    let mut files = Vec::new();
    rust_sources(&root.join("crates"), &mut files);
    files.sort();
    assert!(
        files.len() > 100,
        "the walk found only {} rust files — it is not reaching the tree",
        files.len()
    );

    let mut failures: Vec<String> = Vec::new();
    let mut seen: Vec<(String, usize)> = Vec::new();

    for path in &files {
        let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
        let count = text.matches(NEEDLE).count();
        if count == 0 {
            continue;
        }
        let rel = path
            .strip_prefix(&root)
            .expect("under the repo root")
            .to_string_lossy()
            .replace('\\', "/");
        if rel == SELF {
            continue;
        }
        seen.push((rel.clone(), count));

        match CENSUS.iter().find(|(p, ..)| *p == rel) {
            None => failures.push(format!(
                "{rel}: {count} `{NEEDLE}` arm(s) outside the census. §C: \
                 participant-status parsing has ONE home — call \
                 `chat_predicates::participant_status_from_str` instead. If \
                 this site genuinely needs a different rule (only \
                 `skip_signal` does today), add it here with its reason."
            )),
            Some((_, expected, _)) if count != *expected => failures.push(format!(
                "{rel}: {count} `{NEEDLE}` arm(s), census says {expected}. A \
                 new arm in a listed file is still a private parser growing — \
                 justify it in the census or route it through \
                 `participant_status_from_str`."
            )),
            Some(_) => {}
        }
    }

    for (rel, expected, why) in CENSUS {
        if !seen.iter().any(|(p, _)| p == rel) {
            failures.push(format!(
                "{rel}: census expects {expected} `{NEEDLE}` arm(s), found none. \
                 If the copy was retired, drop its census row ({why})."
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "private participant-status parsers are regrowing (§C):\n  {}",
        failures.join("\n  ")
    );
}
