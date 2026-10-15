//! v4 `lib/memory/extraction/passes/subjects.ts` — the OTHER passes' subject
//! pool. Rate-limited characters are NOT removed from the pool (pre-refactor
//! v5 dropped a skipped character from others' subject lists, and logged a
//! `Skipped duplicate user subject` line that is gone).

use crate::memory_extraction::prompts::other::OtherSubjectInput;
use crate::services::extraction::context::{CastMember, ExtractionContext};

/// The window's speakers plus the user-controlled character, deduplicated,
/// speakers first (v4 `subjectPool`).
pub fn subject_pool(ctx: &ExtractionContext, speakers: &[&CastMember]) -> Vec<CastMember> {
    let mut pool: Vec<CastMember> = Vec::new();
    for m in speakers.iter().copied().chain(ctx.user_character()) {
        if pool.iter().any(|p| p.character_id == m.character_id) {
            continue;
        }
        pool.push(m.clone());
    }
    pool
}

/// One subject entry for an observer (v4's `{ member, input, canonSource }`).
#[derive(Clone, Debug)]
pub struct SubjectEntry {
    pub member: CastMember,
    pub input: OtherSubjectInput,
    pub canon_source: String,
}

/// The prompt's subject entries for one observer, each with the observer's
/// canon about it (v4 `subjectInputsFor`).
pub fn subject_inputs_for(
    ctx: &ExtractionContext,
    observer: &CastMember,
    pool: &[CastMember],
) -> Vec<SubjectEntry> {
    pool.iter()
        .filter(|s| s.character_id != observer.character_id)
        .map(|subject| {
            let (block, source) = ctx.canon_for(&observer.character_id, &subject.character_id);
            SubjectEntry {
                member: subject.clone(),
                input: OtherSubjectInput {
                    id: subject.character_id.clone(),
                    name: subject.name.clone(),
                    pronouns: subject.pronouns.clone(),
                    is_user: subject.is_user,
                    canon_block: block,
                },
                canon_source: source,
            }
        })
        .collect()
}
