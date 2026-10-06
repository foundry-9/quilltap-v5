//! `db::fallback` is the ONE home for v4's repository-layer fallback lines (and,
//! since P4.149, the base repository's RETHROW lines — logged, then handed back)
//! (CLAUDE.md §R.6 of every round since the `97b25fc53` follow-ups
//! unification, which found three hand-copies of one v4 line under three
//! tracing targets; the `97b25fc53` smalls unification found seven more of
//! `Error finding entity by ID` under module targets, plus a local twin of
//! `find_all_or_empty`). A twin written by hand lands under the caller's
//! module target, which the differentials — mapping v4's `Repository` logger
//! to `quilltap::db` — cannot see, so a re-aimed test stays green on a line
//! v4 never logs there.
//!
//! This guard scans the production zone of every core source file (test
//! modules stripped by braces, comments dropped, STRING LITERALS KEPT — the
//! message IS a literal) for each home message as a whole literal, and
//! allows it in `db/fallback.rs` alone. A new emitter anywhere else is red:
//! fold it onto the home's fn (or grow the home by one fn for a new v4 shape).
//!
//! Run standalone:
//!   cargo test -p quilltap-harness --test fallback_home_guard

mod source_census;

use source_census::{core_src_root, production_zone, rust_sources, string_literals};

/// Every line the home emits — the message literal, exactly as `tracing!`
/// carries it. Grows only with `db/fallback.rs`.
const HOME_MESSAGES: &[&str] = &[
    "Error finding entity by ID",
    "Error finding all entities",
    "Error finding entities by filter",
    "Error finding entity by filter",
    "Error querying joined file links",
    "Error finding document by mount point and path",
    "Error deleting file link with GC",
    // P4.134 (dogfood #134(b)): v4's lazy-init and boot-reachable lines.
    "Failed to ensure {} table in {} database",
    "Failed to ensure collection",
    "Failed to ensure collection exists",
    "Error seeding built-in roleplay templates",
    "Error sweeping orphaned store children",
    // P4.136: v4's two API-key reads (`connection-profiles.repository.ts:
    // 249-288`), `collection = connection_profiles`.
    "Error finding API key by ID",
    "Error finding API key by ID and user ID",
    // P4.142: the vault overlay's two batch reads (`doc-mount-documents.
    // repository.ts:142-220`), the two chunk reads and the file-links search
    // (`doc-mount-chunks.repository.ts:130-215`, `doc-mount-file-links.
    // repository.ts:592-628`) — the two searches FOLDED from module-target
    // copies — and P4.139's delivered `getApiKeysByUserId` home (the Shared
    // contract).
    "Error finding documents by mount point IDs and path",
    "Error finding documents by mount point IDs and folder",
    "Error counting embedded chunks by mount point IDs",
    "Error searching chunk content",
    "Error searching file links by name or path",
    "Error finding API keys by user ID",
    // P4.142 (G1): v4's FILES repository list line (`doc-mount-files.
    // repository.ts:86-95`) — the files-list and project-files routes.
    "Error finding files by mount point ID",
    // P4.142 Tier 2: the chunk-clear WRITE's fallback (`doc-mount-chunks.
    // repository.ts:256-276`).
    "Error clearing embeddings by link ID",
    // P4.142 G2: v4's FILES repository PATH line (`doc-mount-files.
    // repository.ts:100-117`) — the chat attach route.
    "Error finding file by mount point and path",
    // P4.D245 (v4 `9753d0eb2`): the roster chokepoint's fail-closed OUTER line
    // (`projects.repository.ts:186-206` `canCharacterParticipate`'s own
    // `safeQuery`); the inner arm reuses `Error finding entity by ID`.
    "Error checking character participation",
    // P4.149: v4's base-repository RETHROW lines (`base.repository.ts:350-447`
    // — log and PROPAGATE, never a fallback; contract C2 measured the fields)
    // and the two chat-informs 4-argument fallback wraps above `_delete`
    // (`chat-informs.repository.ts:271-313`). "Repository-layer lines", not
    // only fallback lines: the rethrow homes log and hand the error back.
    "Error creating entity",
    // The `07b8f0209` follow-ups unification: the chats repository's own wrap
    // above the base `_create` (`chats.repository.ts:280`) — three hand copies
    // (the Concierge validation refusal, the `.qtap` import's C2 twin, the
    // restore's DB arm) folded onto ONE home.
    "Failed to create chat",
    "Error updating entity",
    "Error deleting entity",
    "Error deleting pending informs by batch",
    "Error deleting pending informs for participant",
    // P4.149 (item 4): v4's QUIET unavailable-database arm of `withRawDb`
    // (`dedicated-db.repository.ts:242-251`), carried by the strict-aware
    // joined-links sibling.
    "Dedicated database unavailable; answering with the fallback",
    // P4.156: the chat-informs repository's five outer read wraps
    // (`chat-informs.repository.ts:87-183` — reachable only inside the strict
    // scope; outside it the inner `findByFilter` line answers first).
    // `Error marking informs consumed` (`:238-268`) — the finalizer's and the
    // preserved-partial path's caller-side copies folded onto
    // `db::fallback::informs_marked_consumed_or_zero` at the `94fbb1ae3`
    // smalls unification (P4.156's recorded HANDOFF).
    "Error marking informs consumed",
    "Error finding pending informs for participant",
    "Error finding informs consumed by messages",
    "Error finding pending inform batches",
    "Error finding informs by chat ID",
    "Error finding informs by batch ID",
    // P4.156 (dogfood #145): v4's `deleteMessagesByIds` fallback
    // (`chats-messages.ops.ts:633-686`, the standalone `safeQuery` — no
    // collection), the Inform cancel's record-delete leg.
    "Failed to delete messages from chat",
    // P4.156 (R-G): the memories repository's five own RETHROW wraps
    // (`memories.repository.ts:418-522`), folded from hand copies in
    // `db/memories.rs`.
    "Error creating memory",
    "Error updating memory",
    "Error deleting memory",
    "Error updating memory for character",
    "Error deleting memory for character",
    // P4.155 → §S.1 (the `94fbb1ae3` smalls unification): the store-backed
    // `_create` override's line and the store-backed `create` wrap's line
    // (`store-backed.repository.ts:130-175, 234-236`), folded from the
    // import's lane-local copy.
    "Error creating project entity",
    "Error creating group entity",
    "Error creating project",
    "Error creating group",
];

const HOME: &str = "db/fallback.rs";

#[test]
fn every_home_line_is_emitted_only_by_the_home() {
    let root = core_src_root();
    let mut files = Vec::new();
    rust_sources(&root, &mut files);
    files.sort();
    let mut home_seen = 0usize;
    let mut offenders: Vec<String> = Vec::new();
    for f in &files {
        let rel = f
            .strip_prefix(&root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        let src = std::fs::read_to_string(f).unwrap();
        let zone = production_zone(&src);
        for lit in string_literals(&zone) {
            let body = lit
                .trim_start_matches('r')
                .trim_matches('#')
                .trim_matches('"');
            if !HOME_MESSAGES.contains(&body) {
                continue;
            }
            if rel == HOME {
                home_seen += 1;
            } else {
                offenders.push(format!("{rel}: {body:?}"));
            }
        }
    }
    assert_eq!(
        home_seen,
        HOME_MESSAGES.len(),
        "the home must emit each of its {} messages exactly once (HOME_MESSAGES drifted from `db/fallback.rs`)",
        HOME_MESSAGES.len()
    );
    assert!(
        offenders.is_empty(),
        "v4 repository-layer fallback lines emitted OUTSIDE `db::fallback` (fold each onto the home):\n  {}",
        offenders.join("\n  ")
    );
}

/// The scanner sees a literal in production code and not in a test module —
/// the guard is only as good as its zone.
#[test]
fn the_scanner_sees_production_literals_and_not_test_ones() {
    let src = r#"
fn prod() { tracing::error!(target: "quilltap::db", "Error finding entity by ID"); }
#[cfg(test)]
mod tests {
    #[test]
    fn t() { assert!(l.contains("Error finding all entities")); }
}
"#;
    let zone = production_zone(src);
    let lits: Vec<&str> = string_literals(&zone)
        .into_iter()
        .map(|l| l.trim_matches('"'))
        .collect();
    assert!(lits.contains(&"Error finding entity by ID"), "{lits:?}");
    assert!(!lits.contains(&"Error finding all entities"), "{lits:?}");
}
