//! Links between memories, written as deltas — v4
//! `lib/memory/admission/links.ts` (`8af764514`).
//!
//! v4's why, carried: "Dependency-light (repositories and the logger only), so
//! the episode pass and tests can use it without loading the rest of
//! admission."

use crate::db::runtime::Db;
use crate::db::DbError;

/// Link one memory and others both ways, as deltas against the rows at write
/// time (bug 231) — v4 `linkBothWays(characterId, memoryId, linkTo)`.
/// Admission's insert-linked and the episode pass's fragment links use it.
/// Returns the memory's link list as written (`written ?? targets`).
///
/// The targets are `link_to` minus the memory itself; each target gains the
/// id, then the memory gains the targets (an empty target list writes nothing
/// on the memory and answers `[]`). DEBUG `[Admission] Linked memory to
/// neighbours both ways {characterId, memoryId, linked}` — v4's ROOT logger,
/// so no `module` field — on the caller thread after the writes return.
pub async fn link_both_ways(
    db: &Db,
    character_id: &str,
    memory_id: &str,
    link_to: &[String],
) -> Result<Vec<String>, DbError> {
    let targets: Vec<String> = link_to
        .iter()
        .filter(|id| id.as_str() != memory_id)
        .cloned()
        .collect();
    let (cid, mid, ts) = (
        character_id.to_string(),
        memory_id.to_string(),
        targets.clone(),
    );
    let written = db
        .write(move |w| {
            let repo = w.main().memories();
            for id in &ts {
                repo.add_related_memory_ids(&cid, id, std::slice::from_ref(&mid))?;
            }
            if ts.is_empty() {
                Ok(Some(Vec::new()))
            } else {
                repo.add_related_memory_ids(&cid, &mid, &ts)
            }
        })
        .await?;
    tracing::debug!(
        characterId = %character_id,
        memoryId = %memory_id,
        linked = targets.len(),
        "[Admission] Linked memory to neighbours both ways"
    );
    Ok(written.unwrap_or(targets))
}
