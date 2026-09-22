//! Tier-1 differential (P4.D211 Tier 2): `z.email()`'s verdict, byte-exact
//! against v4's REAL validator over a 1,014-input corpus.
//!
//! v5 hand-rolls this validator once, at
//! `quilltap_core::api::user_profile::is_zod_email`, for v4's `email:
//! z.email().optional()` on `PUT /api/v1/user/profile`
//! (`app/api/v1/user/profile/route.ts:32`) and for `UserSchema`'s
//! `z.email().nullable().optional()` (`lib/schemas/auth.types.ts:23`, the users
//! repo's own gate, mirrored at `db/users.rs:52`). The verdict is payload, not a
//! debugging aid: a refused address answers 400 with v4's flat `Validation
//! error` envelope and the column is never written, so an over-permissive port
//! **persists an address v4 refuses**.
//!
//! # Why this family exists
//!
//! `is_zod_email` was written as a loose hand rule — any non-`@`, non-whitespace
//! local part, a dotted domain with a 2+-letter last label — under the comment
//! "Zod v4's `z.email()` regex, transcribed". It was not a transcription, and
//! nothing compared it to anything: the only coverage was five hand asserts
//! (`user_profile.rs`'s shape test), all five of which the loose rule and the
//! real regex agree on. The `6b0615807` drift check measured the gap in passing
//! — 204 of 596 sampled inputs disagreed, at 4.5.4 AND 4.6.5 equally — and
//! banked it, because the zod bump did not cause it.
//!
//! This is the differential that gap needed. The corpus is recorded from v4's
//! own `z.email()` rather than transcribed from `v4/core/regexes.js`, so it
//! re-records with the dependency instead of rotting beside it — which matters
//! precisely because `4.6.5` rewrote this regex (the lookahead form
//! `^(?!\.)(?!.*\.\.)…` became the grouped `^(?:X+\.)*X*…` one). Recorded at
//! BOTH versions against the same v4 source, the 1,014 rows are byte-identical,
//! so the rewrite is semantically neutral and v5 needs one rule, not two.
//!
//! # What the corpus probes
//!
//! A deterministic cross product of 40 local parts and 25 domains plus 19
//! hand-written shapes, chosen against the grammar's three positions rather
//! than against v5's rule — the dot-separated run `(?:[A-Za-z0-9_'+\-]+\.)*`,
//! the optional tail `[A-Za-z0-9_'+\-]*`, and the FINAL character class
//! `[A-Za-z0-9_+-]`, which (unlike the others) admits neither `'` nor `.`. That
//! last asymmetry is the single most-missed rule in the grammar and the loose
//! rule had no notion of it at all.
//!
//! Generate the oracle output (Node 24, from the PINNED v4 worktree):
//!   cd /tmp/qt-v4-pin-<order>-<sha>
//!   TZ=UTC npx tsx <V5>/harness/oracle/cases/zod-email.ts \
//!     > /tmp/oracle-zod-email.ndjson
//! Run:
//!   QT_ORACLE_ZOD_EMAIL=/tmp/oracle-zod-email.ndjson \
//!     cargo test -p quilltap-harness --test zod_email_equivalence -- --nocapture

use quilltap_core::api::user_profile::is_zod_email;
use serde::Deserialize;

#[derive(Deserialize)]
struct Row {
    input: String,
    valid: bool,
    /// v4's issue sentence, recorded so a message change is visible rather than
    /// silent. `None` on an accepted input.
    message: Option<String>,
}

/// v4's only sentence for this format, measured over the whole corpus (869 of
/// 1,014 rows). Asserted so a locale change reaches this family instead of
/// passing unnoticed — `parse_email`'s refusal rides `api/zod_issues.rs`'s
/// `invalid_format` renderer, which owns these bytes.
const EXPECTED_MESSAGE: &str = "Invalid email address";

#[test]
fn is_zod_email_matches_v4s_real_validator() {
    let path = match std::env::var("QT_ORACLE_ZOD_EMAIL") {
        Ok(p) if !p.is_empty() => p,
        _ => {
            println!("SKIP: set QT_ORACLE_ZOD_EMAIL to the oracle NDJSON (see header).");
            return;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path}: {e}"));
    let rows: Vec<Row> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap_or_else(|e| panic!("parse row {l}: {e}")))
        .collect();

    // A truncated oracle must not pass silently (the
    // `harness-corpus-shape-constants-rot` rule: a floor, plus a census the
    // reader can see).
    assert!(
        rows.len() >= 1_000,
        "the corpus carries {} rows — a truncated oracle, or a shrunk generator",
        rows.len()
    );

    let mut accepted = 0usize;
    let mut mismatches: Vec<String> = Vec::new();
    for row in &rows {
        if row.valid {
            accepted += 1;
            assert!(
                row.message.is_none(),
                "{:?}: v4 accepted it AND carried a message",
                row.input
            );
        } else {
            assert_eq!(
                row.message.as_deref(),
                Some(EXPECTED_MESSAGE),
                "{:?}: v4's sentence for a refused address moved",
                row.input
            );
        }
        let ours = is_zod_email(&row.input);
        if ours != row.valid {
            mismatches.push(format!(
                "  {:?}: v4 {} / v5 {}",
                row.input,
                if row.valid { "ACCEPT" } else { "refuse" },
                if ours { "ACCEPT" } else { "refuse" },
            ));
        }
    }

    // Both verdicts must be exercised, or the family proves only one of them.
    assert!(
        accepted > 0 && accepted < rows.len(),
        "the corpus is one-sided: {accepted} of {} accepted",
        rows.len()
    );

    assert!(
        mismatches.is_empty(),
        "`is_zod_email` disagrees with v4's real `z.email()` on {} of {} inputs:\n{}",
        mismatches.len(),
        rows.len(),
        mismatches.join("\n")
    );
}
