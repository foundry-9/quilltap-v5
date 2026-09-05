//! Memory subject resolution — the port of v4 `lib/memory/memory-subject.ts`
//! (`d883a5ee1`, bug 122).
//!
//! A character's memory store is keyed on `characterId` alone. It holds what
//! they remember about themselves and what they remember about everyone else
//! side by side, and `aboutCharacterId` is the only thing that separates the
//! two. The self-facing context blocks — `## Memory Anchors`, `## Relevant
//! Memories`, `Most relevant memories for this turn:` — arrive under a
//! second-person heading, so every line about someone else has to name its
//! subject or it reads as the character's own life.
//!
//! This module is the one place that turns a pool of memories into the
//! [`MemorySubjectContext`] those formatters need. It lives here rather than in
//! [`crate::memory_injector`] so that module stays pure formatting with no
//! repository reach — exactly v4's reason for the file split.

use std::collections::{HashMap, HashSet};

use crate::db::runtime::Db;
use crate::memory_injector::MemorySubjectContext;

/// Resolve the display names of every other character a pool of memories is
/// about, and pair them with the owning character (v4
/// `buildMemorySubjectContext`).
///
/// `about_character_ids` is the pool's `aboutCharacterId` column — v4 takes
/// `ReadonlyArray<Pick<Memory, 'aboutCharacterId'>>` and reads nothing else, so
/// the caller projects. Only ids that are neither absent nor the character's
/// own are looked up, so **a store of purely first-person memories costs no
/// query at all**. The lookup goes through
/// [`crate::db::characters_read::find_names_by_ids`], which skips the vault
/// overlay and yields an empty map on failure — a missing name degrades one
/// line's prefix to `About another character: ` rather than taking the turn
/// down with it.
pub fn build_memory_subject_context(
    db: &Db,
    self_character_id: &str,
    about_character_ids: impl IntoIterator<Item = Option<String>>,
) -> MemorySubjectContext {
    build_memory_subject_context_with(self_character_id, about_character_ids, &|ids| {
        // v4's `safeQuery(..., new Map())` fallback lives inside
        // `find_names_by_ids`; a read-pool failure before it gets there lands on
        // the same empty map — and logs the same sentence, so an operator reading
        // `combined.log` sees WHY a block came back unprefixed (the §3 review at
        // the `d883a5ee1` unification: a silent pool failure was the one leg v4
        // never has, since its `safeQuery` wraps the whole body).
        match db.read_main(|conn| Ok(crate::db::characters_read::find_names_by_ids(conn, ids))) {
            Ok(names) => names,
            Err(e) => {
                tracing::error!(
                    count = ids.len(),
                    error = %e,
                    "Error resolving character names"
                );
                std::collections::HashMap::new()
            }
        }
    })
}

/// The seam under [`build_memory_subject_context`]: everything but where the
/// names come from. The lookup is `FnOnce` in spirit — it is called at most
/// once per build, and NOT AT ALL when the subject set is empty, which is what
/// the zero-query test pins.
pub(crate) fn build_memory_subject_context_with(
    self_character_id: &str,
    about_character_ids: impl IntoIterator<Item = Option<String>>,
    lookup: &dyn Fn(&[String]) -> HashMap<String, String>,
) -> MemorySubjectContext {
    // v4 builds a `Set`, which is insertion-ordered; keep that so the `IN (…)`
    // parameter order is deterministic run to run.
    let mut subject_ids: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut memory_count = 0usize;
    for about_id in about_character_ids {
        memory_count += 1;
        // v4's `if (aboutId && aboutId !== selfCharacterId)` — absent and
        // empty-string are both falsy.
        if let Some(id) = about_id {
            if !id.is_empty() && id != self_character_id && seen.insert(id.clone()) {
                subject_ids.push(id);
            }
        }
    }

    if subject_ids.is_empty() {
        return MemorySubjectContext {
            self_character_id: self_character_id.to_string(),
            character_names: HashMap::new(),
        };
    }

    let character_names = lookup(&subject_ids);

    tracing::debug!(
        target: "quilltap::memory",
        character_id = self_character_id,
        memory_count,
        subject_count = subject_ids.len(),
        resolved_count = character_names.len(),
        "[MemorySubject] Resolved memory subjects"
    );

    MemorySubjectContext {
        self_character_id: self_character_id.to_string(),
        character_names,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::captured;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    const SELF_ID: &str = "c-kumar";

    fn ids(v: &[Option<&str>]) -> Vec<Option<String>> {
        v.iter().map(|o| o.map(str::to_string)).collect()
    }

    /// v4 returns `{selfCharacterId, characterNames: new Map()}` with NO query
    /// when the set is empty — "it queries nothing for a purely first-person
    /// store". Deleting that early return leaves this at 1.
    #[test]
    fn a_purely_first_person_store_costs_no_query() {
        let calls = AtomicUsize::new(0);
        let lookup = |_: &[String]| {
            calls.fetch_add(1, Ordering::SeqCst);
            HashMap::new()
        };
        let out = build_memory_subject_context_with(
            SELF_ID,
            // absent, empty-string, and the character's own id — all three are
            // filtered before the set is consulted.
            ids(&[None, Some(""), Some(SELF_ID), None]),
            &lookup,
        );
        assert_eq!(
            calls.load(Ordering::SeqCst),
            0,
            "the DB must not be touched"
        );
        assert_eq!(out.self_character_id, SELF_ID);
        assert!(out.character_names.is_empty());
    }

    /// One lookup, over the deduped non-self ids, in first-seen order.
    #[test]
    fn one_lookup_over_the_deduped_non_self_ids() {
        let seen: Arc<Mutex<Vec<Vec<String>>>> = Arc::new(Mutex::new(Vec::new()));
        let sink = seen.clone();
        let lookup = move |batch: &[String]| {
            sink.lock().unwrap().push(batch.to_vec());
            HashMap::from([("c-marion".to_string(), "Marion".to_string())])
        };
        let out = build_memory_subject_context_with(
            SELF_ID,
            ids(&[
                Some("c-marion"),
                Some(SELF_ID),
                Some("c-marion"),
                None,
                Some("c-charlie"),
            ]),
            &lookup,
        );
        let batches = seen.lock().unwrap().clone();
        assert_eq!(batches.len(), 1, "exactly one lookup per build");
        assert_eq!(batches[0], vec!["c-marion", "c-charlie"]);
        assert_eq!(
            out.character_names.get("c-marion").map(String::as_str),
            Some("Marion")
        );
        // An id the map could not resolve is simply absent — the formatter
        // falls back to `About another character: `.
        assert!(!out.character_names.contains_key("c-charlie"));
    }

    /// The debug line, field for field (v4 `logger.debug('[MemorySubject]
    /// Resolved memory subjects', {characterId, memoryCount, subjectCount,
    /// resolvedCount})`). A differential cannot see a log line, so this is the
    /// proof.
    #[test]
    fn the_resolved_debug_line_carries_v4s_four_fields() {
        let lines = captured(|| {
            build_memory_subject_context_with(
                SELF_ID,
                ids(&[Some("c-marion"), Some(SELF_ID), None, Some("c-charlie")]),
                &|_| HashMap::from([("c-marion".to_string(), "Marion".to_string())]),
            );
        });
        let line = lines
            .iter()
            .find(|l| l.contains("[MemorySubject] Resolved memory subjects"))
            .unwrap_or_else(|| panic!("no MemorySubject line in {lines:?}"));
        assert!(line.starts_with("DEBUG quilltap::memory"), "{line}");
        assert!(line.contains("character_id=c-kumar"), "{line}");
        assert!(line.contains("memory_count=4"), "{line}");
        assert!(line.contains("subject_count=2"), "{line}");
        assert!(line.contains("resolved_count=1"), "{line}");
    }

    /// …and it is NOT emitted on the zero-query path (v4 returns before it).
    #[test]
    fn the_first_person_path_logs_nothing() {
        let lines = captured(|| {
            build_memory_subject_context_with(SELF_ID, ids(&[None, Some(SELF_ID)]), &|_| {
                HashMap::new()
            });
        });
        assert!(
            !lines.iter().any(|l| l.contains("[MemorySubject]")),
            "{lines:?}"
        );
    }
}
