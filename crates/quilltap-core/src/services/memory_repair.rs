//! The memory graph repair — v4 `lib/memory/repair.ts` (`b045dd986`, memory
//! refactor §3.11, at the `83334b9a5` pin), behind `POST
//! /api/v1/memories?action=repair` (R-K — P4.D297's).
//!
//! v4's why, carried: "Finds what the chokepoints now prevent but older data
//! may still carry: links in `relatedMemoryIds`, `consolidatedFrom` or
//! `supersededById` that name a row which no longer exists, and rows with no
//! embedding. A dry run reports them; a live run scrubs the links through
//! `scrubDanglingMemoryLinks` (the retirement's own removal deltas) and queues
//! an embedding job for each unembedded row."
//!
//! The scan ([`memories_read::find_dangling_links`]) has NO fallback — "a scan
//! that failed must not read as a clean graph": its `Err` is the route's 500.
//! The scrub's retirement effects run after the writer commit (R-AF).

use serde_json::{json, Map, Value};

use crate::db::memories_read::{self, DanglingMemoryLinks};
use crate::db::runtime::Db;
use crate::db::DbError;
use crate::services::memory_index::{enqueue_missing_embeddings, EnqueueMissingEmbeddingsResult};
use crate::services::memory_retire::{
    announce_retirement, scrub_dangling_memory_links, DanglingScrubReport,
};

const MODULE: &str = "memory:repair";

/// v4 `REPAIR_EMBEDDING_BATCH`: "embedding jobs a single repair queues at
/// most; run it again to queue more."
pub const REPAIR_EMBEDDING_BATCH: i64 = 2000;

/// v4 `MemoryGraphRepairReport` (`repair.ts:25-45`).
#[derive(Clone, Debug, PartialEq)]
pub struct MemoryGraphRepairReport {
    pub dry_run: bool,
    /// The holder scanned, or `None` for every memory in the instance.
    pub character_id: Option<String>,
    /// `found` — `{related {links, rows}, consolidatedFrom {links, digests},
    /// supersededBy, missingEmbeddings}`.
    pub found: Value,
    /// Live runs only: the scrub's counts + `embeddingJobsQueued` +
    /// `[embeddingSkippedReason]`.
    pub repaired: Option<Value>,
}

impl MemoryGraphRepairReport {
    /// v4's object: `dryRun, characterId, found, [repaired]`.
    pub fn to_json(&self) -> Value {
        let mut m = Map::new();
        m.insert("dryRun".into(), json!(self.dry_run));
        m.insert("characterId".into(), json!(self.character_id));
        m.insert("found".into(), self.found.clone());
        if let Some(r) = &self.repaired {
            m.insert("repaired".into(), r.clone());
        }
        Value::Object(m)
    }
}

/// v4's distinct-id count (`new Set(links.map(l => l.id)).size`).
fn distinct<'a>(ids: impl Iterator<Item = &'a String>) -> usize {
    let mut seen: Vec<&String> = Vec::new();
    for id in ids {
        if !seen.contains(&id) {
            seen.push(id);
        }
    }
    seen.len()
}

fn found_of(links: &DanglingMemoryLinks, missing_embeddings: i64) -> Value {
    json!({
        "related": {
            "links": links.related.len(),
            "rows": distinct(links.related.iter().map(|l| &l.id)),
        },
        "consolidatedFrom": {
            "links": links.consolidated_from.len(),
            "digests": distinct(links.consolidated_from.iter().map(|(l, _)| &l.id)),
        },
        "supersededBy": links.superseded_by.len(),
        "missingEmbeddings": missing_embeddings,
    })
}

/// v4 `repairMemoryGraph({userId, characterId?, dryRun})` (`repair.ts:47-98`):
/// scan (one holder, or the whole instance), count the unembedded rows; a
/// live run scrubs the dangling links (one writer closure; its announcements
/// after the commit) and, when rows lack an embedding, queues up to
/// [`REPAIR_EMBEDDING_BATCH`] embedding jobs (`no-embedding-profile` when the
/// user has no default profile). INFO `Memory graph repair {module, dryRun,
/// characterId (id ?? 'all'), found, repaired (?? null)}` — `found` /
/// `repaired` as `foundJson` / `repairedJson` (L-8).
pub async fn repair_memory_graph(
    db: &Db,
    user_id: &str,
    character_id: Option<&str>,
    dry_run: bool,
) -> Result<MemoryGraphRepairReport, DbError> {
    let scope: Option<Vec<String>> = character_id.map(|c| vec![c.to_string()]);
    let links = db.read_main(|c| memories_read::find_dangling_links(c, scope.as_deref()))?;
    let cid = character_id.map(str::to_string);
    let missing_embeddings =
        db.read_main(|c| memories_read::count_without_embedding(c, cid.as_deref()))?;

    let mut report = MemoryGraphRepairReport {
        dry_run,
        character_id: character_id.map(str::to_string),
        found: found_of(&links, missing_embeddings),
        repaired: None,
    };

    if !dry_run {
        let scrub_links = links.clone();
        let scrubbed: DanglingScrubReport = db
            .write(move |w| scrub_dangling_memory_links(w.main().connection(), &scrub_links))
            .await?;
        for announcement in &scrubbed.announcements {
            announce_retirement(db, Some(announcement)).await;
        }
        let mut embedding_jobs_queued = 0usize;
        let mut embedding_skipped_reason: Option<&str> = None;
        if missing_embeddings > 0 {
            match enqueue_missing_embeddings(db, user_id, character_id, REPAIR_EMBEDDING_BATCH)
                .await?
            {
                EnqueueMissingEmbeddingsResult::Ok { enqueued, .. } => {
                    embedding_jobs_queued = enqueued;
                }
                EnqueueMissingEmbeddingsResult::NoProfile { .. } => {
                    embedding_skipped_reason = Some("no-embedding-profile");
                }
            }
        }
        let mut repaired = match scrubbed.to_json() {
            Value::Object(m) => m,
            _ => Map::new(),
        };
        repaired.insert("embeddingJobsQueued".into(), json!(embedding_jobs_queued));
        if let Some(reason) = embedding_skipped_reason {
            repaired.insert("embeddingSkippedReason".into(), json!(reason));
        }
        report.repaired = Some(Value::Object(repaired));
    }

    let found_json = report.found.to_string();
    let repaired_json = report
        .repaired
        .as_ref()
        .map_or_else(|| "null".to_string(), Value::to_string);
    tracing::info!(
        module = MODULE,
        dryRun = dry_run,
        characterId = %character_id.unwrap_or("all"),
        foundJson = %found_json,
        repairedJson = %repaired_json,
        "Memory graph repair"
    );
    Ok(report)
}
