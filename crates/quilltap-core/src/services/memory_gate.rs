//! **BEHAVIOUR-FROZEN LANE-REMOVE shim (work order P4.D290, LR-13 /
//! §R.10(o)).** v4 deleted this service in `8af764514` — admission
//! (`crate::services::admission`) is its replacement. It stays, unchanged in
//! behaviour, only while sibling files of the round still call it
//! (`api/memories.rs` → P4.D298; `memory_processor.rs` + `fold_episode_pass.rs`
//! → P4.D296; `memory_dedup.rs` → P4.D297; the `memory_watermark_tier3`
//! family → P4.D297): their unit tests pin the old `[MemoryGate]` contract, so
//! a forwarding shim onto `admit_row` would redden them. Each marked item goes
//! with its last caller's lane, or at §S.5. Its two items with no caller
//! outside this file (`RELATED_THRESHOLD`, `GATE_TOP_K`) lost `pub`.
//!
//! The **memory gate** service — v4's `createMemoryWithGate` / `runMemoryGate`
//! (`lib/memory/memory-service.ts` + `lib/memory/memory-gate.ts`). The pre-write
//! similarity check that turned the memory system from append-only into
//! append-or-reinforce.
//!
//! Given a candidate memory it (1) generates an embedding for the candidate text
//! (the model call — the tier-3 seam, with v4's one-retry-on-failure), (2) queries
//! the character's vector store for the top-`GATE_TOP_K` nearest existing memories,
//! and (3) makes a five-outcome decision by similarity band:
//!
//! | Outcome | Band | Effect |
//! |---|---|---|
//! | `SkipNearDuplicate`   | `>= NEAR_DUPLICATE_THRESHOLD` | counted on the existing row; no new row |
//! | `Reinforce`           | `>= MERGE_THRESHOLD`          | boost the existing memory |
//! | `InsertRelated`       | `>= RELATED_THRESHOLD`        | insert + link the related memories |
//! | `Insert`              | below `RELATED_THRESHOLD`     | fresh memory |
//! | `SkipEmbeddingFailed` | — (embedding unavailable)     | skip, no row |
//!
//! (Historical note: v4's header comment once claimed "REINFORCE >= 0.80 /
//! INSERT_RELATED 0.70–0.80" — stale against its own exported constants
//! (`0.90` / `0.85` / `0.70`), the same comments-lie trap as
//! SQLCipher-vs-ChaCha20. v4 `8bf3cb5f` fixed its header to cite the
//! constants; the differential proves the bands either way.)
//!
//! ## Shape under the Phase-3 runtime
//!
//! The service is `async` and generic over an [`EmbeddingProvider`]: it awaits the
//! model call, reads the vector store + matched memories off the read pool
//! ([`Db::read_main`]), and funnels every mutation through the writer thread
//! ([`Db::write`]) — so the "channel is the only mutator" invariant holds and this
//! is the first service to exercise the whole Unit-0 write path end to end. The
//! vector store is the in-memory [`CharacterVectorStore`] shim (load off the read
//! pool, flush on the writer). This validates Units 0 + 0.5 + tier-2 together.
//!
//! ## Deferred (tracked, out of scope for this unit)
//!
//!   - the `skipGate` / `skipEmbedding` → `createMemoryDirect` path — the
//!     force-insert-without-gate flow (no similarity check). This port always runs
//!     the gate; the direct path lands with the extraction driver.
//!   - the 500 ms inter-retry delay (`EMBEDDING_RETRY_DELAY_MS`) — a host-timing
//!     concern with no DB-state effect; reproducing it would pull a timer into the
//!     scheduler-free core, so the retry is issued without the sleep.

use std::collections::HashMap;

use serde_json::Value;

use crate::about_character::resolve_about_character_id;
use crate::db::characters_read;
use crate::db::memories::{CreateOptions, MemCreate, MemUpdate, TierExtra};
use crate::db::vector_store::CharacterVectorStore;
use crate::db::{memories_read, DbError};
use crate::episodic::{build_memory_embedding_text, EpisodicAnchorView};
use crate::memory_gate::{
    append_capped_footnotes, count_reinforcement_footnotes, extract_novel_details,
    occasions_are_distinct, MAX_REINFORCEMENT_FOOTNOTES,
};
use crate::model::embedding::{EmbeddingCallOptions, EmbeddingProvider};
use crate::services::activity_kinds::ActivityKind;
use crate::services::activity_registry::track_activity;

use crate::db::runtime::Db;

// LANE-REMOVE(P4.D290|P4.D296|P4.D297|P4.D298): v4 deleted memory-gate.ts's NEAR_DUPLICATE_THRESHOLD in 8af764514
/// At or above this cosine similarity the candidate is an exact-or-near-exact
/// restatement of an existing memory. No new row is written and the existing
/// text is left alone, but the observation still counts:
/// [`absorb_near_duplicate`] bumps the row's reinforcement count (v4
/// `3f7320138` F1).
pub const NEAR_DUPLICATE_THRESHOLD: f64 = 0.90;
// LANE-REMOVE(P4.D290|P4.D296|P4.D297|P4.D298): v4 deleted memory-gate.ts's MERGE_THRESHOLD in 8af764514
/// At or above this the best match is reinforced instead of duplicated.
pub const MERGE_THRESHOLD: f64 = 0.85;
/// Related-but-distinct band floor — matches here get bidirectionally linked.
const RELATED_THRESHOLD: f64 = 0.70;
/// Top-K nearest existing memories fetched from the vector store during the gate.
const GATE_TOP_K: usize = 5;

// LANE-REMOVE(P4.D296|P4.D297|P4.D298): v4 deleted memory-service.ts's MemoryGateOutcome.action in 8af764514
/// The action the gate took (v4 `MemoryGateOutcome.action`, minus `SKIP_GATE`
/// which is the deferred direct path).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GateAction {
    Insert,
    InsertRelated,
    Reinforce,
    SkipNearDuplicate,
    SkipEmbeddingFailed,
}

// LANE-REMOVE(P4.D296|P4.D297|P4.D298): v4 deleted memory-service.ts's MemoryGateOutcome in 8af764514
/// The gate's result (v4 `MemoryGateOutcome`): the affected memory + the action,
/// plus the per-outcome extras callers (e.g. the extraction path) consume.
#[derive(Debug, Clone, PartialEq)]
pub struct MemoryGateOutcome {
    /// The memory affected by this outcome. For `SkipNearDuplicate` it is the
    /// existing memory that absorbed the observation; `None` only for
    /// `SkipEmbeddingFailed`.
    pub memory_id: Option<String>,
    pub action: GateAction,
    /// Novel details appended on `Reinforce` (empty otherwise).
    pub novel_details: Vec<String>,
    /// The ids linked on `InsertRelated` (empty otherwise).
    pub related_memory_ids: Vec<String>,
    /// Cosine similarity to the existing memory (`SkipNearDuplicate` / `Reinforce`).
    pub similarity: Option<f64>,
    /// Human-readable reason (`SkipEmbeddingFailed`).
    pub reason: Option<String>,
    /// The post-reinforcement `reinforcementCount` (`Reinforce` only) — v4's
    /// `outcome.memory.reinforcementCount`, which the extraction driver's debug
    /// log reads. On a failed reinforcement update this is the existing row's
    /// count (v4 returns the unchanged memory), defaulted `?? 1`.
    pub reinforcement_count: Option<f64>,
}

// LANE-REMOVE(P4.D296|P4.D297|P4.D298): v4 deleted memory-service.ts's CreateMemoryOptions in 8af764514
/// Options for memory creation (v4 `CreateMemoryOptions`), minus the deferred
/// `skipGate` / `skipEmbedding` flags (this port always runs the gate).
#[derive(Debug, Clone, Default)]
pub struct CreateMemoryOptions {
    pub character_id: String,
    pub content: String,
    pub summary: String,
    pub keywords: Vec<String>,
    pub tags: Vec<String>,
    pub importance: Option<f64>,
    pub about_character_id: Option<String>,
    pub chat_id: Option<String>,
    pub project_id: Option<String>,
    /// `"AUTO"` / `"MANUAL"` (v4 default `"MANUAL"`).
    pub source: Option<String>,
    pub source_message_id: Option<String>,
    /// Override createdAt/updatedAt with the source-message timestamp (batch
    /// extraction). When present, `create` pins both timestamps to it.
    pub source_message_timestamp: Option<String>,
    pub witnessed_context: Option<String>,
    /// Episodic spine (v4 8bf3cb5f): ISO wall-clock EVENT time. Callers stamp
    /// it from the source turn's message timestamp; retold events resolve
    /// their `when` phrase server-side before reaching here (rounds 2/3 —
    /// round 1 callers pass `None` and the row lands NULL, the inert path).
    pub occurred_at: Option<String>,
    /// Free-text in-story time, for chats on a fictional timeline.
    pub narrative_time: Option<String>,
    /// Proper nouns of the episode (places, people, named things).
    pub entities: Vec<String>,
    /// Declared memory kind (`'semantic'` | `'episodic'`). `None` → `'semantic'`.
    pub kind: Option<String>,
}

/// Cap on regex-derived fallback entities so noise can't flood the column
/// (v4 `FALLBACK_ANCHOR_MAX_ENTITIES`).
const FALLBACK_ANCHOR_MAX_ENTITIES: usize = 6;

/// Episodic safety net for AUTO-extracted memories (v4
/// `applyEpisodicFallbackAnchors`, `memory-service.ts`): when the extractor
/// omitted `entities` and/or `occurredAt`, derive them deterministically from
/// the memory text via the same date/proper-noun regexes reinforcement uses
/// ([`extract_novel_details`]), resolving any captured date phrase against the
/// source-message timestamp (or the write clock). Fills gaps only — a
/// caller-supplied anchor always wins — and MANUAL memories pass through
/// untouched (they carry the user's deliberate choices).
fn apply_episodic_fallback_anchors(data: CreateMemoryOptions) -> CreateMemoryOptions {
    // v4 `data.source && data.source !== 'AUTO'` — JS-truthy, so an absent
    // (or empty) source falls through to the fallback like AUTO does.
    if let Some(source) = data.source.as_deref() {
        if !source.is_empty() && source != "AUTO" {
            return data;
        }
    }
    let need_entities = data.entities.is_empty();
    // `!data.occurredAt` — absent OR empty string.
    let need_occurred_at = data.occurred_at.as_deref().is_none_or(str::is_empty);
    if !need_entities && !need_occurred_at {
        return data;
    }

    let details = extract_novel_details(&data.content, "");
    let mut next = data;

    if need_occurred_at {
        let anchor_iso = next
            .source_message_timestamp
            .clone()
            .unwrap_or_else(crate::clock::now_iso);
        for detail in &details {
            if let Some(resolved) = crate::episodic::resolve_when_phrase(Some(detail), &anchor_iso)
            {
                next.occurred_at = Some(resolved);
                break;
            }
        }
    }

    if need_entities {
        // Proper-noun-shaped details only (skip the date/number/measure
        // captures): `/^[A-Z]/` and no `/\d/` — both ASCII classes in JS.
        let entities: Vec<String> = details
            .iter()
            .filter(|d| d.starts_with(|c: char| c.is_ascii_uppercase()))
            .filter(|d| !d.contains(|c: char| c.is_ascii_digit()))
            .take(FALLBACK_ANCHOR_MAX_ENTITIES)
            .cloned()
            .collect();
        if !entities.is_empty() {
            next.entities = entities;
        }
    }

    next
}

// LANE-REMOVE(P4.D296|P4.D297|P4.D298): v4 deleted memory-service.ts's MemoryServiceOptions in 8af764514
/// Options for the operation (v4 `MemoryServiceOptions`), minus the deferred skip
/// flags.
#[derive(Debug, Clone, Default)]
pub struct MemoryServiceOptions {
    pub user_id: String,
    pub embedding_profile_id: Option<String>,
}

/// The internal gate decision (v4 `GateDecision`), carrying the matched memories as
/// their net JSON (the shape [`memories_read::find_by_ids`] returns).
enum GateDecision {
    Insert,
    Reinforce { existing: Value },
    InsertRelated { related: Vec<RelatedMatch> },
    SkipNearDuplicate { existing: Value, similarity: f64 },
    SkipEmbeddingFailed { reason: String },
}

struct RelatedMatch {
    memory: Value,
    similarity: f64,
}

struct GateResult {
    decision: GateDecision,
    embedding: Option<Vec<f32>>,
}

// LANE-REMOVE(P4.D296|P4.D297|P4.D298): v4 deleted memory-service.ts's createMemoryWithGate in 8af764514
/// Create a memory through the gate (v4 `createMemoryWithGate`). Runs the
/// name-presence check, the gate, and the per-outcome writes, returning the full
/// outcome.
pub async fn create_memory_with_gate<P: EmbeddingProvider>(
    db: &Db,
    provider: &P,
    data: &CreateMemoryOptions,
    opts: &MemoryServiceOptions,
) -> Result<MemoryGateOutcome, DbError> {
    let data = apply_name_presence_check(db, data).await;

    // Episodic safety net: run the deterministic date/proper-noun regexes on
    // FIRST WRITE (not just reinforce) so entities/occurredAt get populated even
    // when the extractor model omits them. Only fills gaps — never overrides a
    // caller-supplied anchor.
    let data = apply_episodic_fallback_anchors(data);

    // Episodic spine: the candidate's anchors ride along so (a) the gate
    // embedding carries the anchor line and (b) the >7-day date guard can split
    // distinct occasions.
    let anchors = EpisodicAnchorView {
        occurred_at: data.occurred_at.clone(),
        narrative_time: data.narrative_time.clone(),
        entities: data.entities.clone(),
    };

    let gate = run_memory_gate(
        db,
        provider,
        &data.character_id,
        &data.content,
        &data.summary,
        &opts.user_id,
        opts.embedding_profile_id.as_deref(),
        &anchors,
    )
    .await?;

    let GateResult {
        decision,
        embedding,
    } = gate;

    match decision {
        GateDecision::SkipNearDuplicate {
            existing,
            similarity,
        } => {
            // v4 `3f7320138` F1: the observation still counts — the facts
            // restated most faithfully must not be the ones that never accrue
            // reinforcement. Text, links and vector stay untouched.
            let absorbed = absorb_near_duplicate(db, &existing).await?;
            Ok(MemoryGateOutcome {
                memory_id: id_of(&absorbed),
                action: GateAction::SkipNearDuplicate,
                novel_details: Vec::new(),
                related_memory_ids: Vec::new(),
                similarity: Some(similarity),
                reason: None,
                reinforcement_count: None,
            })
        }

        GateDecision::SkipEmbeddingFailed { reason } => Ok(MemoryGateOutcome {
            memory_id: None,
            action: GateAction::SkipEmbeddingFailed,
            novel_details: Vec::new(),
            related_memory_ids: Vec::new(),
            similarity: None,
            reason: Some(reason),
            reinforcement_count: None,
        }),

        GateDecision::Reinforce { existing } => {
            // The candidate's anchors ride along so a retelling that supplies
            // better when/where anchors than the original capture can upgrade
            // the row.
            let (memory_id, novel_details, reinforcement_count) = reinforce_memory(
                db,
                provider,
                &existing,
                &data.content,
                &opts.user_id,
                opts.embedding_profile_id.as_deref(),
                &anchors,
            )
            .await?;
            Ok(MemoryGateOutcome {
                memory_id: Some(memory_id),
                action: GateAction::Reinforce,
                novel_details,
                related_memory_ids: Vec::new(),
                similarity: None,
                reason: None,
                reinforcement_count: Some(reinforcement_count),
            })
        }

        GateDecision::InsertRelated { related } => {
            let new_id = create_memory_direct_with_embedding(db, &data, embedding).await?;
            let linked = link_related_memories(db, &new_id, &data.character_id, &related).await?;
            // P4.D288 (§R.10(e)): the after-insert housekeeping enqueue is gone —
            // v4 deleted `after-insert.ts` with `MEMORY_HOUSEKEEPING` (`104e986c7`);
            // its successor is the admission path's `maybeEnqueueMaintenance`
            // (§R.10(h)). The gate keeps its admission behaviour, enqueues nothing.
            Ok(MemoryGateOutcome {
                memory_id: Some(new_id),
                action: GateAction::InsertRelated,
                novel_details: Vec::new(),
                related_memory_ids: linked,
                similarity: None,
                reason: None,
                reinforcement_count: None,
            })
        }

        GateDecision::Insert => {
            let new_id = create_memory_direct_with_embedding(db, &data, embedding).await?;
            // P4.D288 (§R.10(e)): the after-insert housekeeping enqueue is gone —
            // v4 deleted `after-insert.ts` with `MEMORY_HOUSEKEEPING` (`104e986c7`);
            // its successor is the admission path's `maybeEnqueueMaintenance`
            // (§R.10(h)). The gate keeps its admission behaviour, enqueues nothing.
            Ok(MemoryGateOutcome {
                memory_id: Some(new_id),
                action: GateAction::Insert,
                novel_details: Vec::new(),
                related_memory_ids: Vec::new(),
                similarity: None,
                reason: None,
                reinforcement_count: None,
            })
        }
    }
}

/// Run the gate (v4 `runMemoryGate`): embed the candidate (one retry), search the
/// vector store, and decide the band.
///
/// Memories formed from a tool call rather than the extraction job have no job
/// row to count them, so the gate registers itself. Re-entrant by kind, so the
/// extraction job's own row is not double-counted; the embedding it mints inside
/// still counts separately under "Emb". v4 `memory-gate.ts:138` (`664cfca84`),
/// which wraps this same function.
#[allow(clippy::too_many_arguments)] // mirrors v4 runMemoryGate's 8-arg shape
async fn run_memory_gate<P: EmbeddingProvider>(
    db: &Db,
    provider: &P,
    character_id: &str,
    content: &str,
    summary: &str,
    user_id: &str,
    embedding_profile_id: Option<&str>,
    anchors: &EpisodicAnchorView,
) -> Result<GateResult, DbError> {
    track_activity(
        ActivityKind::Memory,
        run_memory_gate_inner(
            db,
            provider,
            character_id,
            content,
            summary,
            user_id,
            embedding_profile_id,
            anchors,
        ),
    )
    .await
}

/// v4 `runMemoryGateInner`.
#[allow(clippy::too_many_arguments)]
async fn run_memory_gate_inner<P: EmbeddingProvider>(
    db: &Db,
    provider: &P,
    character_id: &str,
    content: &str,
    summary: &str,
    user_id: &str,
    embedding_profile_id: Option<&str>,
    anchors: &EpisodicAnchorView,
) -> Result<GateResult, DbError> {
    // v4 8bf3cb5f: the canonical builder — the anchor line is part of the
    // embedded text, so the gate compares against the same-shaped vectors the
    // create path stores (see build_memory_embedding_text).
    let embedding_text = build_memory_embedding_text(summary, content, Some(anchors));

    let embedding =
        match generate_with_retry(provider, &embedding_text, user_id, embedding_profile_id).await {
            Ok(e) => e,
            Err(second_msg) => {
                // v4 `memory-gate.ts:194`. Log-only (P4.88): the returned
                // decision and the untouched `memories` table are identical
                // whether or not this fires. `error` is the RAW provider message
                // — v4's `secondMsg`, not the composed reason below.
                tracing::warn!(
                    characterId = character_id,
                    userId = user_id,
                    error = second_msg,
                    "[MemoryGate] Skipping memory write — embedding generation failed after retry"
                );
                return Ok(GateResult {
                    decision: GateDecision::SkipEmbeddingFailed {
                        reason: format!("Embedding failed after retry: {second_msg}"),
                    },
                    embedding: None,
                });
            }
        };

    // Load the store + search off the read pool.
    let results = {
        let embedding = &embedding;
        db.read_main(move |conn| {
            let store = CharacterVectorStore::load(conn, character_id);
            Ok(store.search(embedding, GATE_TOP_K))
        })?
    };

    if results.is_empty() {
        return Ok(GateResult {
            decision: GateDecision::Insert,
            embedding: Some(embedding),
        });
    }

    // Fetch full memory data for the matched ids (only the top-K).
    let matched_ids: Vec<String> = results.iter().map(|r| r.id.clone()).collect();
    let matched = db.read_main(move |conn| memories_read::find_by_ids(conn, &matched_ids))?;
    let memory_map: HashMap<String, Value> = matched
        .into_iter()
        .filter_map(|m| id_of(&m).map(|id| (id, m)))
        .collect();

    let best = &results[0];
    let best_mem = memory_map.get(&best.id).cloned();

    // Episodic date guard: a near-identical embedding with an occurredAt more
    // than DATE_GUARD_DAYS from the best match is a DIFFERENT OCCASION of the
    // same activity — link, never absorb. v4 checks `bestResult.score >=
    // MERGE_THRESHOLD` so the downgrade covers both the SKIP and the REINFORCE
    // band, and it returns INSERT_RELATED carrying ONLY the best match.
    if let Some(best_memory) = best_mem.as_ref() {
        let distinct_occasion = best.score >= MERGE_THRESHOLD
            && occasions_are_distinct(
                anchors.occurred_at.as_deref(),
                best_memory.get("occurredAt").and_then(Value::as_str),
            );
        if distinct_occasion {
            // The guard judges the BEST match's own `occurredAt` (a cold row's
            // too), then links its live digest when it is a superseded member.
            let link_target = redirect_cold_link(db, best_memory.clone())?;
            return Ok(GateResult {
                decision: GateDecision::InsertRelated {
                    related: vec![RelatedMatch {
                        memory: link_target,
                        similarity: best.score,
                    }],
                },
                embedding: Some(embedding),
            });
        }
    }

    if best.score >= NEAR_DUPLICATE_THRESHOLD {
        if let Some(existing) = best_mem {
            let (target, _redirected) = resolve_cold_for_absorb(db, existing).await?;
            return Ok(GateResult {
                decision: GateDecision::SkipNearDuplicate {
                    existing: target,
                    similarity: best.score,
                },
                embedding: Some(embedding),
            });
        }
    }

    if best.score >= MERGE_THRESHOLD {
        if let Some(existing) = best_mem {
            let (target, redirected) = resolve_cold_for_absorb(db, existing).await?;
            if redirected {
                // A digest is reinforced by count only: no [+] footnote (the
                // next consolidation folds any novel detail in).
                // SKIP_NEAR_DUPLICATE is the count-only absorption path.
                return Ok(GateResult {
                    decision: GateDecision::SkipNearDuplicate {
                        existing: target,
                        similarity: best.score,
                    },
                    embedding: Some(embedding),
                });
            }
            return Ok(GateResult {
                decision: GateDecision::Reinforce { existing: target },
                embedding: Some(embedding),
            });
        }
    }

    // Related band: [RELATED_THRESHOLD, MERGE_THRESHOLD), in score-desc order.
    // A cold, superseded member is linked via its digest; duplicates (several
    // members of one cluster) collapse onto the digest at its best similarity
    // — v4's `Map` keyed by target id, whose overwrite keeps the FIRST
    // insertion position. A cold row seen only here is never promoted:
    // linking a new row to a cold row is not a promotion.
    let mut related: Vec<RelatedMatch> = Vec::new();
    for r in results
        .iter()
        .filter(|r| r.score >= RELATED_THRESHOLD && r.score < MERGE_THRESHOLD)
    {
        let Some(memory) = memory_map.get(&r.id).cloned() else {
            continue;
        };
        let target = redirect_cold_link(db, memory)?;
        let target_id = id_of(&target);
        match related.iter_mut().find(|m| id_of(&m.memory) == target_id) {
            Some(prior) => {
                if r.score > prior.similarity {
                    *prior = RelatedMatch {
                        memory: target,
                        similarity: r.score,
                    };
                }
            }
            None => related.push(RelatedMatch {
                memory: target,
                similarity: r.score,
            }),
        }
    }

    if !related.is_empty() {
        return Ok(GateResult {
            decision: GateDecision::InsertRelated { related },
            embedding: Some(embedding),
        });
    }

    Ok(GateResult {
        decision: GateDecision::Insert,
        embedding: Some(embedding),
    })
}

/// Longest supersession chain the gate will follow to a live digest (v4
/// `MAX_DIGEST_HOPS`).
const MAX_DIGEST_HOPS: usize = 3;

/// "Cold" is v4's services-side `m.tier === 'cold'` (§R.4(b)'s third
/// semantic): a NULL or absent tier is hot.
fn is_cold(memory: &Value) -> bool {
    memory.get("tier").and_then(Value::as_str) == Some("cold")
}

/// `memory.supersededById`, JS-truthy (an empty string is no digest).
fn superseded_by(memory: &Value) -> Option<String> {
    str_opt_field(memory, "supersededById").filter(|s| !s.is_empty())
}

/// Gate-side handling of a COLD best match (v4 `resolveColdForAbsorb`,
/// `d58548051`, `memory-gate.ts:186-216`). The gate searches both tiers so a
/// cold row still stops re-insertion; what it does with one depends on why the
/// row is cold:
///
/// - superseded (`supersededById`): the observation belongs to the digest
///   that replaced it, so absorption is redirected there (≤ 3 hops) and the
///   result is flagged `redirected`;
/// - demoted by cap pressure (no `supersededById`): the world asked for it
///   again, so it is promoted back to hot.
///
/// Hot rows pass through untouched. The code, not v4's doc comment, is the
/// spec: a VANISHED digest stops the walk on the cold row whose
/// `supersededById` is still set, so that row is NOT promoted (v4's own
/// `gate-cold-tier.test.ts:109-116`); a found digest that is itself cold and
/// unsuperseded IS promoted, with `redirected` still true; a longer chain
/// stops at the third hop on a cold, superseded row and absorbs there.
async fn resolve_cold_for_absorb(db: &Db, memory: Value) -> Result<(Value, bool), DbError> {
    if !is_cold(&memory) {
        return Ok((memory, false));
    }
    let mut current = memory;
    let mut redirected = false;
    for _ in 0..MAX_DIGEST_HOPS {
        if !is_cold(&current) {
            break;
        }
        let Some(digest_id) = superseded_by(&current) else {
            break;
        };
        let ids = vec![digest_id.clone()];
        let digest = db
            .read_main(move |conn| memories_read::find_by_ids(conn, &ids))?
            .into_iter()
            .next();
        let Some(digest) = digest else {
            tracing::debug!(
                memoryId = id_of(&current).unwrap_or_default(),
                supersededById = digest_id,
                "[MemoryGate] Superseding digest missing; falling back to cold row"
            );
            break;
        };
        current = digest;
        redirected = true;
    }

    if is_cold(&current) && superseded_by(&current).is_none() {
        let promoted = promote_cold_memory(db, current).await?;
        return Ok((promoted, redirected));
    }
    Ok((current, redirected))
}

/// Move a cold, non-superseded memory back to the hot tier (v4
/// `promoteColdMemory`, `:224-241`) and return the LOCAL view — `tier: 'hot'`,
/// `supersededById: null`, `updatedAt` not refreshed (v4 spreads the snapshot).
///
/// R-B (NO-PORT by architecture): v4 then re-stamps its cached vector store
/// (`setTier`, with its `Re-stamped vector tiers` debug and the WARN
/// `[MemoryGate] Failed to re-stamp vector tier after promotion`). v5 keeps no
/// cached store — every path calls `CharacterVectorStore::load`, which stamps
/// `cold` from the column — so the column write IS the re-stamp.
async fn promote_cold_memory(db: &Db, memory: Value) -> Result<Value, DbError> {
    let character_id = str_field(&memory, "characterId");
    let memory_id = id_of(&memory).unwrap_or_default();
    let (cid, ids) = (character_id.clone(), vec![memory_id.clone()]);
    db.write(move |writers| {
        writers.main().memories().update_tier_bulk(
            &cid,
            &ids,
            "hot",
            TierExtra {
                superseded_by_id: Some(None),
                consolidated_at: None,
            },
        )
    })
    .await?;
    tracing::debug!(
        memoryId = memory_id,
        characterId = character_id,
        "[MemoryGate] Promoted cold memory to hot on re-observation"
    );
    let mut promoted = memory;
    if let Some(obj) = promoted.as_object_mut() {
        obj.insert("tier".into(), Value::String("hot".into()));
        obj.insert("supersededById".into(), Value::Null);
    }
    Ok(promoted)
}

/// The INSERT_RELATED target for a match (v4 `redirectColdLink`, `:248-258`):
/// a cold, superseded member is replaced by its live digest (≤ 3 hops);
/// anything else (hot, cold-unsuperseded, digest gone) links as-is. Never
/// writes — linking a new row to a cold row is not a promotion — and a
/// missing digest stops the walk silently.
fn redirect_cold_link(db: &Db, memory: Value) -> Result<Value, DbError> {
    let mut current = memory;
    for _ in 0..MAX_DIGEST_HOPS {
        if !is_cold(&current) {
            break;
        }
        let Some(digest_id) = superseded_by(&current) else {
            break;
        };
        let ids = vec![digest_id];
        let digest = db
            .read_main(move |conn| memories_read::find_by_ids(conn, &ids))?
            .into_iter()
            .next();
        match digest {
            Some(d) => current = d,
            None => break,
        }
    }
    Ok(current)
}

/// Generate an embedding with v4's one-retry-on-failure (`SKIP_EMBEDDING_FAILED`
/// reason on the second failure). The 500 ms inter-retry delay is omitted (see the
/// module deferrals).
///
/// The error is the **raw second-attempt provider message** — v4's `secondMsg`
/// (`secondError instanceof EmbeddingError ? secondError.message : String(…)`) —
/// not the `Embedding failed after retry: …` sentence. v4 logs the raw message
/// and composes the sentence separately, so both callers get what they need
/// without either re-deriving the other's string.
async fn generate_with_retry<P: EmbeddingProvider>(
    provider: &P,
    text: &str,
    user_id: &str,
    embedding_profile_id: Option<&str>,
) -> Result<Vec<f32>, String> {
    match provider
        .generate_embedding_for_user(
            text,
            user_id,
            embedding_profile_id,
            // lexical_text: None until P4.D290 (admission — v4 `admit.ts:145` / `neighbours.ts:144` / `apply.ts:85`) lands it (C1-10).
            EmbeddingCallOptions::background(),
        )
        .await
    {
        Ok(r) => Ok(r.embedding),
        Err(_first) => match provider
            .generate_embedding_for_user(
                text,
                user_id,
                embedding_profile_id,
                // lexical_text: None until P4.D290 (admission — v4 `admit.ts:145` / `neighbours.ts:144` / `apply.ts:85`) lands it (C1-10).
                EmbeddingCallOptions::background(),
            )
            .await
        {
            Ok(r) => Ok(r.embedding),
            Err(second) => Err(second.message),
        },
    }
}

/// Create a memory and store its pre-computed embedding (v4
/// `createMemoryDirectWithEmbedding`). Mints id + timestamps (or pins them to the
/// source-message timestamp), writes the row, then — when an embedding is present —
/// sets the BLOB and adds the vector to the store, all on the writer thread.
/// Returns the new memory id.
async fn create_memory_direct_with_embedding(
    db: &Db,
    data: &CreateMemoryOptions,
    embedding: Option<Vec<f32>>,
) -> Result<String, DbError> {
    let importance = data.importance.unwrap_or(0.5);
    let id = uuid::Uuid::new_v4().to_string();
    let (created_at, updated_at) = match &data.source_message_timestamp {
        Some(ts) => (ts.clone(), ts.clone()),
        None => {
            let now = crate::clock::now_iso();
            (now.clone(), now)
        }
    };

    let create = MemCreate {
        character_id: data.character_id.clone(),
        about_character_id: data.about_character_id.clone(),
        chat_id: data.chat_id.clone(),
        project_id: data.project_id.clone(),
        content: data.content.clone(),
        summary: data.summary.clone(),
        keywords: data.keywords.clone(),
        tags: data.tags.clone(),
        importance,
        // v4 creates the row WITHOUT an embedding, then updates it below.
        embedding: None,
        source: data.source.clone().unwrap_or_else(|| "MANUAL".to_string()),
        witnessed_context: data.witnessed_context.clone(),
        // Episodic spine: v4 `occurredAt: data.occurredAt ?? null` (and
        // siblings) — the caller's anchors land verbatim, defaults otherwise.
        occurred_at: data.occurred_at.clone(),
        narrative_time: data.narrative_time.clone(),
        entities: data.entities.clone(),
        kind: data.kind.clone().unwrap_or_else(|| "semantic".to_string()),
        source_message_id: data.source_message_id.clone(),
        last_accessed_at: None,
        reinforcement_count: 1.0,
        last_reinforced_at: None,
        related_memory_ids: Vec::new(),
        reinforced_importance: importance,
        // P4.D265: v4's `memories.create` prefill (`tier ?? 'hot'`,
        // `consolidatedFrom ?? []`) — the gate never names either.
        tier: "hot".to_string(),
        superseded_by_id: None,
        superseded_at: None,
        consolidated_from: Vec::new(),
        consolidated_at: None,
    };
    let create_opts = CreateOptions {
        id: id.clone(),
        created_at,
        updated_at,
    };

    let character_id = data.character_id.clone();
    let id_for_write = id.clone();
    db.write(move |writers| {
        let main = writers.main();
        main.memories().create(&create, &create_opts)?;

        if let Some(emb) = embedding {
            // Store the pre-computed embedding on the row (v4's
            // `updateForCharacter(id, { embedding })`).
            let patch = MemUpdate {
                embedding: Some(Some(emb.clone())),
                ..Default::default()
            };
            main.memories()
                .update_for_character(&character_id, &id_for_write, &patch)?;
            // P4.D288 (C1-3, mechanical): the row write above IS the index
            // write — the vector mirror is gone.
        }
        Ok(())
    })
    .await?;

    Ok(id)
}

/// What [`count_reinforcement`] wrote: v4's `Pick<Memory, 'reinforcementCount' |
/// 'lastReinforcedAt' | 'reinforcedImportance'>`.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Counted {
    pub reinforcement_count: f64,
    pub reinforced_importance: f64,
    pub last_reinforced_at: String,
}

impl Counted {
    /// `{ ...memory, ...counted }` — the counted fields over a row's JSON.
    fn merge_into(&self, memory: &Value) -> Value {
        let mut merged = memory.clone();
        if let Some(obj) = merged.as_object_mut() {
            obj.insert(
                "reinforcementCount".into(),
                crate::db::js_number_to_json(self.reinforcement_count),
            );
            obj.insert(
                "lastReinforcedAt".into(),
                Value::String(self.last_reinforced_at.clone()),
            );
            obj.insert(
                "reinforcedImportance".into(),
                crate::db::js_number_to_json(self.reinforced_importance),
            );
        }
        merged
    }
}

/// Count one more observation of `memory` and return the row's reinforcement
/// fields as they now stand (v4 `countReinforcement`, `70f9b495c`).
///
/// The count is incremented atomically against the row at write time
/// ([`MemoriesRepository::increment_reinforcement`](crate::db::memories::MemoriesRepository::increment_reinforcement)),
/// never written as an absolute value from the gate's snapshot — two jobs
/// reinforcing the same row would otherwise both write N+1 (bug 182). `None`
/// when the row is missing; a DB failure propagates (v4's rethrow). NO-PORT
/// (§R.4(o)): v4's job-child buffered arm (`…Reinforcement buffered (job
/// child)…`) — v5 has no child.
pub(crate) async fn count_reinforcement(
    db: &Db,
    memory: &Value,
) -> Result<Option<Counted>, DbError> {
    let at = crate::clock::now_iso();
    let character_id = str_field(memory, "characterId");
    let memory_id = id_of(memory).unwrap_or_default();
    let at_w = at.clone();
    let counted = db
        .write(move |writers| {
            writers
                .main()
                .memories()
                // P4.D288 (C1-7, mechanical): `by` joined the verb (bug 226);
                // one observation here, as before.
                .increment_reinforcement(&character_id, &memory_id, &at_w, 1)
        })
        .await?;
    Ok(counted.map(|r| Counted {
        reinforcement_count: r.reinforcement_count,
        reinforced_importance: r.reinforced_importance,
        last_reinforced_at: at,
    }))
}

/// Count a near-duplicate re-observation (`SkipNearDuplicate`) as reinforcement
/// (v4 `absorbNearDuplicate`, `3f7320138` F1).
///
/// The candidate restates the existing memory almost verbatim, so nothing about
/// the row's text or vector changes: no footnotes, no anchor upgrades, no
/// re-embed. Only the reinforcement signal moves — count, `lastReinforcedAt`,
/// `reinforcedImportance` — which is what protection and ranking read. Returns
/// the existing row with the counted fields merged in (its `updatedAt` stays
/// the SNAPSHOT's, as v4's spread does).
pub(crate) async fn absorb_near_duplicate(db: &Db, existing: &Value) -> Result<Value, DbError> {
    let counted = count_reinforcement(db, existing).await?;
    let memory_id = id_of(existing).unwrap_or_default();
    let Some(counted) = counted else {
        tracing::warn!(
            memoryId = memory_id,
            characterId = str_field(existing, "characterId"),
            "[MemoryGate] Failed to reinforce near-duplicate memory"
        );
        return Ok(existing.clone());
    };
    tracing::debug!(
        memoryId = memory_id,
        reinforcementCount = counted.reinforcement_count,
        reinforcedImportance = counted.reinforced_importance,
        "[MemoryGate] Near-duplicate absorbed as reinforcement"
    );
    Ok(counted.merge_into(existing))
}

/// Persist a patch to a memory and return the row as it now stands (v4
/// `patchMemory`). `None` only when the row is genuinely missing (or not the
/// character's) — `update_for_character`'s not-found answer; a DB failure
/// propagates (v4's `updateForCharacter` is a RETHROW-mode `safeQuery`). The
/// re-read runs on the caller thread after the write commits. NO-PORT
/// (§R.4(o)): v4's job-child buffered arm (`[MemoryGate] Memory patch
/// buffered (job child); using local view`).
pub(crate) async fn patch_memory(
    db: &Db,
    memory: &Value,
    patch: MemUpdate,
) -> Result<Option<Value>, DbError> {
    let character_id = str_field(memory, "characterId");
    let memory_id = id_of(memory).unwrap_or_default();
    let id_w = memory_id.clone();
    let updated = db
        .write(move |writers| {
            writers
                .main()
                .memories()
                .update_for_character(&character_id, &id_w, &patch)
        })
        .await?;
    if !updated {
        return Ok(None);
    }
    db.read_main(|conn| memories_read::find_by_id(conn, &memory_id))
}

/// The episodic anchors a memory row carries (v4 passes the `Memory` itself
/// as the `EpisodicAnchorView`).
fn anchors_of(memory: &Value) -> EpisodicAnchorView {
    EpisodicAnchorView {
        occurred_at: str_opt_field(memory, "occurredAt"),
        narrative_time: str_opt_field(memory, "narrativeTime"),
        entities: str_array_field(memory, "entities"),
    }
}

/// Regenerate a memory's embedding from its current summary / content /
/// anchors and write it to both the row and the character's vector store (v4
/// `reembedMemory`, `3f7320138` F3). ONE provider call — no retry (the gate's
/// own candidate embedding keeps its one retry; this does not). Best-effort:
/// ANY failure (the provider, the row write, the store) warns `[MemoryGate]
/// Failed to re-embed memory` and leaves the old vector in place — the whole
/// block is v4's one `try`. Both lines are emitted on the caller thread, after
/// the write returns.
///
/// `error` renders v4's `String(error)`: a provider failure is an
/// `EmbeddingError`, whose `this.name` makes it `EmbeddingError: <message>`;
/// a v5 DB / store failure renders `Error: <display>` (no fixture reaches it).
pub(crate) async fn reembed_memory<P: EmbeddingProvider>(
    db: &Db,
    provider: &P,
    memory: &Value,
    user_id: &str,
    embedding_profile_id: Option<&str>,
) -> bool {
    let memory_id = id_of(memory).unwrap_or_default();
    let text = build_memory_embedding_text(
        &str_field(memory, "summary"),
        &str_field(memory, "content"),
        Some(&anchors_of(memory)),
    );
    let embedding = match provider
        .generate_embedding_for_user(
            &text,
            user_id,
            embedding_profile_id,
            // lexical_text: None until P4.D290 (admission — v4 `admit.ts:145` / `neighbours.ts:144` / `apply.ts:85`) lands it (C1-10).
            EmbeddingCallOptions::background(),
        )
        .await
    {
        Ok(result) => result.embedding,
        Err(e) => {
            tracing::warn!(
                memoryId = memory_id,
                error = format!("EmbeddingError: {}", e.message),
                "[MemoryGate] Failed to re-embed memory"
            );
            return false;
        }
    };
    // Two writes, as v4's two awaits: the row's embedding commits FIRST, so a
    // store failure after it leaves the row re-embedded (v4's catch fires after
    // `updateForCharacter` has already landed).
    let character_id = str_field(memory, "characterId");
    let (cid_row, id_row, emb_row) = (character_id.clone(), memory_id.clone(), embedding.clone());
    let row_written = db
        .write(move |writers| {
            let patch = MemUpdate {
                embedding: Some(Some(emb_row)),
                ..Default::default()
            };
            // v4 ignores `updateForCharacter`'s answer here and goes on to the
            // store; only a throw reaches the catch.
            writers
                .main()
                .memories()
                .update_for_character(&cid_row, &id_row, &patch)?;
            Ok(())
        })
        .await;
    // P4.D288 (C1-3, mechanical): the row's embedding is the only copy — the
    // vector-mirror write that followed it is gone with the mirror.
    let _ = (character_id, embedding);
    let written = row_written;
    match written {
        Ok(()) => {
            tracing::debug!(memoryId = memory_id, "[MemoryGate] Re-embedded memory");
            true
        }
        Err(e) => {
            tracing::warn!(
                memoryId = memory_id,
                error = format!("Error: {e}"),
                "[MemoryGate] Failed to re-embed memory"
            );
            false
        }
    }
}

/// The `error` v5's dedup re-embed reports when its embedding seam is not
/// assembled (R-D — a recorded v5-only text, unreachable on a full host).
pub(crate) const DEDUP_REEMBED_SEAM_MISSING: &str =
    "Error: memory embedding not assembled (dedup re-embed seam)";

// LANE-REMOVE(P4.D297): v4 deleted memory-merge.ts's applyMemoryMerge in 8af764514
/// Apply a merge plan to the survivor and re-embed it when its content (or its
/// anchor date) changed (v4 `applyMemoryMerge`, `memory-merge.ts:139-176`,
/// `3f7320138` F9). Returns the survivor as it now stands, or `None` when the
/// row could not be updated (a DB failure PROPAGATES — v4's rethrow).
///
/// `user_id` is needed for the re-embed; without it a changed survivor keeps
/// its old vector (logged) until the next reindex — unreachable through dedup,
/// which always passes one. `provider` is dedup's optional embedding seam
/// (R-D): `None` makes the re-embed fail NON-FATALLY with v4's WARN (v4 dedup
/// never refuses for embedding reasons, so neither does v5), and the closing
/// DEBUG still says `reembedded: true` — v4's `needsReembed && !!userId`, true
/// even when the re-embed failed.
pub(crate) async fn apply_memory_merge<P: EmbeddingProvider>(
    db: &Db,
    provider: Option<&P>,
    survivor: &Value,
    plan: &crate::memory_merge::MemoryMergePlan,
    user_id: Option<&str>,
    embedding_profile_id: Option<&str>,
) -> Result<Option<Value>, DbError> {
    let survivor_id = id_of(survivor).unwrap_or_default();
    if plan.patch.is_empty() {
        tracing::debug!(
            survivorId = survivor_id,
            "[MemoryMerge] Nothing to fold into survivor"
        );
        return Ok(Some(survivor.clone()));
    }

    let Some(updated) = patch_memory(db, survivor, plan.to_mem_update()).await? else {
        tracing::warn!(
            survivorId = survivor_id,
            characterId = str_field(survivor, "characterId"),
            "[MemoryMerge] Failed to update survivor"
        );
        return Ok(None);
    };

    let needs_reembed = plan.content_changed || plan.patch.contains_key("occurredAt");
    if needs_reembed {
        match user_id {
            Some(uid) => match provider {
                Some(p) => {
                    reembed_memory(db, p, &updated, uid, embedding_profile_id).await;
                }
                None => {
                    tracing::warn!(
                        memoryId = survivor_id,
                        error = DEDUP_REEMBED_SEAM_MISSING,
                        "[MemoryGate] Failed to re-embed memory"
                    );
                }
            },
            None => {
                tracing::warn!(
                    survivorId = survivor_id,
                    "[MemoryMerge] Survivor changed but no userId to re-embed with"
                );
            }
        }
    }

    let fields: Vec<&String> = plan.patch.keys().collect();
    tracing::debug!(
        survivorId = survivor_id,
        fieldsJson = %serde_json::to_string(&fields).unwrap_or_default(),
        mergedDetails = plan.merged_details.len(),
        reembedded = needs_reembed && user_id.is_some(),
        "[MemoryMerge] Folded losers into survivor"
    );
    Ok(Some(updated))
}

/// Reinforce an existing memory with a new observation (v4 `reinforceMemory`,
/// `memory-gate.ts:589-685` — `3f7320138` F3 rewritten by `70f9b495c`):
///
/// 1. extract the candidate's novel details and append them as `[+]`
///    footnotes, up to [`MAX_REINFORCEMENT_FOOTNOTES`];
/// 2. upgrade the episodic anchors when the retelling supplies better ones
///    (fill a null `occurredAt` / `narrativeTime`; union new entities, bounded);
/// 3. COUNT the observation through the atomic increment FIRST — counted
///    atomically against the row at write time, never from the gate's
///    snapshot (bug 182) — then write the content / anchors as a SECOND patch;
/// 4. re-embed when the content or the anchor line changed — unless the row
///    was already at the footnote cap: past the cap the vector is frozen until
///    consolidation rewrites it (anchors still land on the row).
///
/// Returns `(memory_id, novel_details, count_for_log)`: the APPENDED (capped)
/// details, and the count the extraction driver's debug line reports (the
/// counted value, or the snapshot's `reinforcementCount ?? 1` when the count
/// could not land and v4 returns the unchanged memory).
#[allow(clippy::too_many_arguments)] // mirrors v4 reinforceMemory's shape
async fn reinforce_memory<P: EmbeddingProvider>(
    db: &Db,
    provider: &P,
    existing: &Value,
    candidate_content: &str,
    user_id: &str,
    embedding_profile_id: Option<&str>,
    candidate_anchors: &EpisodicAnchorView,
) -> Result<(String, Vec<String>, f64), DbError> {
    let existing_id = id_of(existing).unwrap_or_default();
    let existing_char = str_field(existing, "characterId");
    let existing_content = str_field(existing, "content");
    // v4: `existingMemory.reinforcementCount ?? 1` (the failed-count arm's log
    // value).
    let existing_count = num_field(existing, "reinforcementCount").unwrap_or(1.0);

    let all_novel_details = extract_novel_details(candidate_content, &existing_content);
    let at_cap = count_reinforcement_footnotes(&existing_content) >= MAX_REINFORCEMENT_FOOTNOTES;

    let (new_content, novel_details) =
        append_capped_footnotes(&existing_content, &all_novel_details);
    if novel_details.len() < all_novel_details.len() {
        tracing::debug!(
            memoryId = existing_id,
            offered = all_novel_details.len(),
            appended = novel_details.len(),
            cap = MAX_REINFORCEMENT_FOOTNOTES,
            "[MemoryGate] Reinforcement footnote cap reached; dropping extra details"
        );
    }
    let content_changed = new_content != existing_content;

    // The count, lastReinforcedAt and reinforcedImportance are NOT in this
    // patch: they move only through count_reinforcement's atomic increment.
    let mut update = MemUpdate::default();
    let mut has_update = false;
    if content_changed {
        update.content = Some(new_content);
        has_update = true;
    }

    // Anchor upgrades: a retelling that supplies when/where the original capture
    // lacked should improve the row — otherwise only footnote prose accumulates.
    // The date guard upstream already diverted candidates whose occurredAt
    // CONFLICTS with the existing one by > DATE_GUARD_DAYS, so filling a null is
    // safe here. Entities union (bounded) so repeated retellings converge.
    let existing_anchors = anchors_of(existing);
    let mut anchors_changed = false;
    // Both sides JS-truthy: a present-but-empty anchor is "missing".
    if let Some(occurred) = candidate_anchors
        .occurred_at
        .as_deref()
        .filter(|s| !s.is_empty())
    {
        if existing_anchors
            .occurred_at
            .as_deref()
            .is_none_or(str::is_empty)
        {
            update.occurred_at = Some(Some(occurred.to_string()));
            has_update = true;
            anchors_changed = true;
        }
    }
    if let Some(narrative) = candidate_anchors
        .narrative_time
        .as_deref()
        .filter(|s| !s.is_empty())
    {
        if existing_anchors
            .narrative_time
            .as_deref()
            .is_none_or(str::is_empty)
        {
            update.narrative_time = Some(Some(narrative.to_string()));
            has_update = true;
            anchors_changed = true;
        }
    }
    let candidate_entities: Vec<&str> = candidate_anchors
        .entities
        .iter()
        .map(String::as_str)
        .filter(|e| !crate::jsstr::js_trim(e).is_empty())
        .collect();
    if !candidate_entities.is_empty() {
        let existing_entities = &existing_anchors.entities;
        let existing_lower: Vec<String> =
            existing_entities.iter().map(|e| e.to_lowercase()).collect();
        let fresh: Vec<&str> = candidate_entities
            .into_iter()
            .filter(|e| !existing_lower.contains(&e.to_lowercase()))
            .collect();
        if !fresh.is_empty() {
            let mut merged = existing_entities.clone();
            merged.extend(fresh.into_iter().map(str::to_string));
            merged.truncate(12);
            // A row already holding 12 entities never "changes" — the bounded
            // union is the same list, so no write and no re-embed.
            if merged.len() > existing_entities.len() {
                update.entities = Some(merged);
                has_update = true;
                anchors_changed = true;
            }
        }
    }

    let Some(counted) = count_reinforcement(db, existing).await? else {
        tracing::warn!(
            memoryId = existing_id,
            characterId = existing_char,
            "[MemoryGate] Failed to update memory during reinforcement"
        );
        return Ok((existing_id, novel_details, existing_count));
    };

    let mut updated_memory = counted.merge_into(existing);
    if has_update {
        match patch_memory(db, &updated_memory, update).await? {
            None => {
                tracing::warn!(
                    memoryId = existing_id,
                    characterId = existing_char,
                    "[MemoryGate] Failed to update memory during reinforcement"
                );
                // The count landed; v4 returns the counted local view.
                return Ok((existing_id, novel_details, counted.reinforcement_count));
            }
            Some(patched) => updated_memory = counted.merge_into(&patched),
        }
    }

    // Re-embed if content or anchors changed (the anchor line is embedded
    // text). Past the footnote cap the row's vector is frozen until
    // consolidation rewrites it — anchors still land on the row, they just
    // don't re-embed.
    if !at_cap && (content_changed || anchors_changed) {
        reembed_memory(db, provider, &updated_memory, user_id, embedding_profile_id).await;
    }

    Ok((existing_id, novel_details, counted.reinforcement_count))
}

/// Bidirectionally link a new memory with related existing memories (v4
/// `linkRelatedMemories`). Returns the linked ids in order.
async fn link_related_memories(
    db: &Db,
    new_memory_id: &str,
    new_memory_character_id: &str,
    related: &[RelatedMatch],
) -> Result<Vec<String>, DbError> {
    let mut linked: Vec<String> = Vec::new();

    for rm in related {
        let rel_char = str_field(&rm.memory, "characterId");
        let rel_id = id_of(&rm.memory).unwrap_or_default();
        let existing_related = str_array_field(&rm.memory, "relatedMemoryIds");

        if !existing_related.iter().any(|x| x == new_memory_id) {
            let mut updated = existing_related.clone();
            updated.push(new_memory_id.to_string());
            let rel_char = rel_char.clone();
            let rel_id = rel_id.clone();
            db.write(move |writers| {
                let patch = MemUpdate {
                    related_memory_ids: Some(updated),
                    ..Default::default()
                };
                writers
                    .main()
                    .memories()
                    .update_for_character(&rel_char, &rel_id, &patch)
            })
            .await?;
        }
        linked.push(rel_id);
    }

    if !linked.is_empty() {
        let linked_for_write = linked.clone();
        let new_char = new_memory_character_id.to_string();
        let new_id = new_memory_id.to_string();
        db.write(move |writers| {
            let patch = MemUpdate {
                related_memory_ids: Some(linked_for_write),
                ..Default::default()
            };
            writers
                .main()
                .memories()
                .update_for_character(&new_char, &new_id, &patch)
        })
        .await?;
    }

    Ok(linked)
}

/// The AUTO mis-attribution safety net (v4 `applyNamePresenceCheck`), ported for
/// the no-lookup branches only (see the module deferrals): a null / self /
/// non-AUTO proposal passes through unchanged. A cross-character AUTO proposal
/// reads both characters through the vault overlay and resolves via the ported
/// `resolve_about_character_id`, collapsing a mis-attributed about-target to a
/// self-reference on the holder (v4's LLM-mis-attribution safety net).
async fn apply_name_presence_check(db: &Db, data: &CreateMemoryOptions) -> CreateMemoryOptions {
    // Steps 1-3 (no DB read): null proposal / self-reference / non-AUTO source.
    let proposed = match &data.about_character_id {
        None => return data.clone(),
        Some(proposed) if proposed == &data.character_id => return data.clone(),
        Some(proposed) => proposed.clone(),
    };
    if let Some(src) = &data.source {
        if src != "AUTO" {
            return data.clone();
        }
    }

    // The cross-character AUTO resolution: read both characters through the
    // vault-overlaid `characters_read::find_by_id` (v4 `repos.characters
    // .findById`), then run the ported pure `resolve_about_character_id`. v4
    // wraps the lookup in a try/catch — the safety net never blocks a write —
    // so any read failure (including a missing mount-index partition) passes
    // the proposal through unchanged.
    let holder_id = data.character_id.clone();
    let lookup = db.read_main(|main| {
        db.read_mount_index(|mount| {
            let about = characters_read::find_by_id(main, mount, &proposed)?;
            let holder = characters_read::find_by_id(main, mount, &holder_id)?;
            Ok((about, holder))
        })
    });
    let (about, holder) = match lookup {
        Ok(pair) => pair,
        Err(_) => return data.clone(),
    };

    fn name_and_aliases(character: &Value) -> (String, Vec<String>) {
        let name = character
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let aliases = character
            .get("aliases")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        (name, aliases)
    }

    let holder_parts = holder.as_ref().map(name_and_aliases);
    let about_parts = about.as_ref().map(|c| {
        let (name, aliases) = name_and_aliases(c);
        let controlled_by = c
            .get("controlledBy")
            .and_then(Value::as_str)
            .unwrap_or("llm")
            .to_string();
        (name, aliases, controlled_by)
    });

    let text = format!("{}\n{}", data.summary, data.content);
    let resolution = resolve_about_character_id(
        &data.character_id,
        holder_parts
            .as_ref()
            .map(|(name, aliases)| (name.as_str(), aliases.as_slice())),
        Some(&proposed),
        about_parts.as_ref().map(|(name, aliases, controlled_by)| {
            (name.as_str(), aliases.as_slice(), controlled_by.as_str())
        }),
        &text,
    );
    if resolution.flipped {
        let mut flipped = data.clone();
        flipped.about_character_id = Some(data.character_id.clone());
        flipped
    } else {
        data.clone()
    }
}

// ---------------------------------------------------------------------------
// Small accessors over the net Memory JSON (`memories_read::find_by_ids` shape).
// ---------------------------------------------------------------------------

fn id_of(memory: &Value) -> Option<String> {
    memory.get("id").and_then(Value::as_str).map(str::to_string)
}

fn str_field(memory: &Value, key: &str) -> String {
    memory
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// Nullable string field (`None` when absent or JSON null).
fn str_opt_field(memory: &Value, key: &str) -> Option<String> {
    memory.get(key).and_then(Value::as_str).map(str::to_string)
}

fn num_field(memory: &Value, key: &str) -> Option<f64> {
    memory.get(key).and_then(Value::as_f64)
}

fn str_array_field(memory: &Value, key: &str) -> Vec<String> {
    memory
        .get(key)
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Writer;
    use crate::model::embedding::CannedEmbeddingProvider;
    use tempfile::{tempdir, TempDir};

    /// A throwaway base64 pepper keys the fresh encrypted DB (never a real one).
    const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
    const CHAR: &str = "char-1";

    /// The minimal DDL the gate touches — the column sets the memories /
    /// vector_indices / vector_entries repos name (SQLite is dynamically typed, so
    /// this is enough for a *functional* self-test; byte-exactness is the
    /// differential's job).
    const DDL: &str = "
        CREATE TABLE memories (
            id TEXT PRIMARY KEY, characterId TEXT, aboutCharacterId TEXT, chatId TEXT,
            projectId TEXT, content TEXT, summary TEXT, keywords TEXT, tags TEXT,
            importance REAL, embedding BLOB, source TEXT, witnessedContext TEXT,
            occurredAt TEXT, narrativeTime TEXT, entities TEXT DEFAULT '[]', kind TEXT DEFAULT 'semantic',
            sourceMessageId TEXT, lastAccessedAt TEXT, reinforcementCount REAL,
            lastReinforcedAt TEXT, relatedMemoryIds TEXT, reinforcedImportance REAL,
            tier TEXT DEFAULT 'hot', supersededById TEXT, supersededAt TEXT, consolidatedFrom TEXT DEFAULT '[]', consolidatedAt TEXT, createdAt TEXT, updatedAt TEXT);
    ";

    /// Build a fresh encrypted DB with the three tables, optionally seeding one
    /// existing memory + its vector, then open a `Db` over it.
    fn make_db(seed: Option<(&str, &str, Vec<f32>)>) -> (TempDir, Db) {
        crate::test_support::global_capture::install();
        let dir = tempdir().unwrap();
        let path = dir.path().join("main.db");
        {
            let w = Writer::open_writable(&path, PEPPER).unwrap();
            w.connection().execute_batch(DDL).unwrap();
            if let Some((content, summary, vec)) = seed {
                w.memories()
                    .create(
                        &MemCreate {
                            character_id: CHAR.to_string(),
                            about_character_id: None,
                            chat_id: None,
                            project_id: None,
                            content: content.to_string(),
                            summary: summary.to_string(),
                            keywords: vec![],
                            tags: vec![],
                            importance: 0.5,
                            embedding: Some(vec.clone()),
                            source: "AUTO".to_string(),
                            witnessed_context: None,
                            occurred_at: None,
                            narrative_time: None,
                            entities: Vec::new(),
                            kind: "semantic".to_string(),
                            source_message_id: None,
                            last_accessed_at: None,
                            reinforcement_count: 1.0,
                            last_reinforced_at: None,
                            related_memory_ids: vec![],
                            reinforced_importance: 0.5,
                            tier: "hot".to_string(),
                            superseded_by_id: None,
                            superseded_at: None,
                            consolidated_from: Vec::new(),
                            consolidated_at: None,
                        },
                        &CreateOptions {
                            id: "mem-seed".to_string(),
                            created_at: "2020-01-01T00:00:00.000Z".to_string(),
                            updated_at: "2020-01-01T00:00:00.000Z".to_string(),
                        },
                    )
                    .unwrap();
                // P4.D288: the row IS the index — no mirror to seed.
            }
        }
        let db = Db::open_main(&path, PEPPER).unwrap();
        (dir, db)
    }

    fn opts() -> MemoryServiceOptions {
        MemoryServiceOptions {
            user_id: "user-1".to_string(),
            embedding_profile_id: None,
        }
    }

    fn candidate(content: &str, summary: &str) -> CreateMemoryOptions {
        CreateMemoryOptions {
            character_id: CHAR.to_string(),
            content: content.to_string(),
            summary: summary.to_string(),
            source: Some("AUTO".to_string()),
            ..Default::default()
        }
    }

    fn count(db: &Db, table: &str) -> i64 {
        db.read_main(|c| {
            Ok(c.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))?)
        })
        .unwrap()
    }

    /// An empty store → INSERT: a new memory row, a new vector entry, and a fresh
    /// metadata row all land — the service driving a write through the Unit-0
    /// channel end to end.
    #[tokio::test]
    async fn empty_store_inserts_and_persists_vector() {
        let (_dir, db) = make_db(None);
        let provider = CannedEmbeddingProvider::new().with_vector("s\n\nnew fact", vec![1.0, 0.0]);
        let outcome = create_memory_with_gate(&db, &provider, &candidate("new fact", "s"), &opts())
            .await
            .unwrap();
        assert_eq!(outcome.action, GateAction::Insert);
        assert!(outcome.memory_id.is_some());
        assert_eq!(count(&db, "memories"), 1);
        // P4.D288: the vector lands on the row — the only copy.
        assert_eq!(count(&db, "memories WHERE embedding IS NOT NULL"), 1);
    }

    /// A best match in the merge band → REINFORCE: no new row, the existing memory's
    /// reinforcementCount bumps to 2, no content change.
    #[tokio::test]
    async fn merge_band_reinforces_existing() {
        let (_dir, db) = make_db(Some((
            "Mara trusts the crew completely.",
            "sum",
            vec![1.0, 0.0],
        )));
        // dot([0.87, 0.49299], [1,0]) = 0.87 -> [MERGE, NEAR_DUPLICATE) -> REINFORCE.
        let provider = CannedEmbeddingProvider::new()
            .with_vector("sum\n\nMara fully trusts the crew.", vec![0.87, 0.492_99]);
        let outcome = create_memory_with_gate(
            &db,
            &provider,
            &candidate("Mara fully trusts the crew.", "sum"),
            &opts(),
        )
        .await
        .unwrap();
        assert_eq!(outcome.action, GateAction::Reinforce);
        assert_eq!(outcome.memory_id.as_deref(), Some("mem-seed"));
        assert_eq!(count(&db, "memories"), 1, "no new row");
        let rc: f64 = db
            .read_main(|c| {
                Ok(c.query_row(
                    "SELECT reinforcementCount FROM memories WHERE id = 'mem-seed'",
                    [],
                    |r| r.get(0),
                )?)
            })
            .unwrap();
        assert_eq!(rc, 2.0);
    }

    fn seed_row(db: &Db) -> Value {
        db.read_main(|c| memories_read::find_by_id(c, "mem-seed"))
            .unwrap()
            .expect("the seed row")
    }

    fn cell<T: rusqlite::types::FromSql>(db: &Db, col: &str) -> T {
        db.read_main(|c| {
            Ok(c.query_row(
                &format!("SELECT {col} FROM memories WHERE id = 'mem-seed'"),
                [],
                |r| r.get(0),
            )?)
        })
        .unwrap()
    }

    async fn set_cells(db: &Db, sets: &str) {
        let sql = format!("UPDATE memories SET {sets} WHERE id = 'mem-seed'");
        db.write(move |w| {
            w.main().connection().execute_batch(&sql)?;
            Ok(())
        })
        .await
        .unwrap();
    }

    fn vector_bytes(db: &Db) -> Vec<u8> {
        db.read_main(|c| {
            Ok(c.query_row(
                "SELECT embedding FROM memories WHERE id = 'mem-seed'",
                [],
                |r| r.get(0),
            )?)
        })
        .unwrap()
    }

    /// A canned provider that RECORDS every text it is asked to embed — the
    /// "exactly one provider call" / "no provider call" proofs.
    #[derive(Clone)]
    struct Recording {
        inner: CannedEmbeddingProvider,
        calls: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
        /// P4.D288 (C1-10): the `lexical_text` each call carried — a double
        /// keys on `text` and RECORDS the lexical text.
        lexical: std::sync::Arc<std::sync::Mutex<Vec<Option<String>>>>,
    }

    impl Recording {
        fn new(inner: CannedEmbeddingProvider) -> Self {
            Self {
                inner,
                calls: Default::default(),
                lexical: Default::default(),
            }
        }
        fn calls(&self) -> Vec<String> {
            self.calls.lock().unwrap().clone()
        }
    }

    impl EmbeddingProvider for Recording {
        fn generate_embedding_for_user(
            &self,
            text: &str,
            user_id: &str,
            profile_id: Option<&str>,
            opts: EmbeddingCallOptions,
        ) -> impl std::future::Future<
            Output = Result<
                crate::model::embedding::EmbeddingResult,
                crate::model::embedding::EmbeddingError,
            >,
        > + Send {
            self.calls.lock().unwrap().push(text.to_string());
            self.lexical.lock().unwrap().push(opts.lexical_text.clone());
            self.inner
                .generate_embedding_for_user(text, user_id, profile_id, opts)
        }
    }

    /// v4 `3f7320138` F1 (`memory-recall-housekeeping-fixes.test.ts:116-136`):
    /// a best match at/above the near-duplicate threshold → SKIP_NEAR_DUPLICATE
    /// writes no new row and leaves the text, the embedding and the vector
    /// alone — but the observation COUNTS: `reinforcementCount` 2,
    /// `reinforcedImportance` recomputed, `lastReinforcedAt` stamped.
    #[tokio::test]
    async fn near_duplicate_absorbs_as_reinforcement() {
        let (_dir, db) = make_db(Some(("Mara trusts the crew.", "sum", vec![1.0, 0.0])));
        let before_vector = vector_bytes(&db);
        // dot([0.97, 0.24310], [1,0]) = 0.97 >= 0.90 -> SKIP_NEAR_DUPLICATE.
        let provider = CannedEmbeddingProvider::new()
            .with_vector("sum\n\nMara trusts the crew.", vec![0.97, 0.243_1]);
        let (outcome, lines) =
            crate::test_support::global_capture::capture_async(create_memory_with_gate(
                &db,
                &provider,
                &candidate("Mara trusts the crew.", "sum"),
                &opts(),
            ))
            .await;
        let outcome = outcome.unwrap();
        assert_eq!(outcome.action, GateAction::SkipNearDuplicate);
        assert_eq!(outcome.memory_id.as_deref(), Some("mem-seed"));
        assert_eq!(
            outcome.reinforcement_count, None,
            "R-H: the shape does not grow"
        );
        assert_eq!(count(&db, "memories"), 1);
        assert_eq!(count(&db, "memories WHERE embedding IS NOT NULL"), 1);
        assert_eq!(cell::<f64>(&db, "reinforcementCount"), 2.0);
        assert_eq!(
            cell::<f64>(&db, "reinforcedImportance"),
            crate::memory_gate::calculate_reinforced_importance(0.5, 2.0)
        );
        assert!(cell::<Option<String>>(&db, "lastReinforcedAt").is_some());
        assert_eq!(cell::<String>(&db, "content"), "Mara trusts the crew.");
        // P4.D288: the row carries the vector (the only copy) — untouched.
        assert_eq!(vector_bytes(&db), before_vector, "the vector is untouched");
        let line = only_line(
            &lines,
            "[MemoryGate] Near-duplicate absorbed as reinforcement",
        );
        assert!(line.starts_with("DEBUG "), "{line:?}");
        assert!(line.contains(" memoryId=mem-seed"), "{line:?}");
        assert!(line.contains(" reinforcementCount=2"), "{line:?}");
        assert!(
            line.contains(&format!(
                " reinforcedImportance={}",
                crate::memory_gate::calculate_reinforced_importance(0.5, 2.0)
            )),
            "{line:?}"
        );
        assert!(
            !lines
                .iter()
                .any(|l| l.contains("Failed to reinforce near-duplicate")),
            "silence leg: {lines:?}"
        );
    }

    /// v4 `:147-159`: the row vanished between the search and the count → WARN
    /// `[MemoryGate] Failed to reinforce near-duplicate memory` and the
    /// snapshot comes back unchanged (no absorbed DEBUG).
    #[tokio::test]
    async fn a_vanished_near_duplicate_warns_and_returns_the_snapshot() {
        let (_dir, db) = make_db(Some(("Mara trusts the crew.", "sum", vec![1.0, 0.0])));
        let mut snapshot = seed_row(&db);
        snapshot["id"] = Value::String("mem-gone".into());
        let (absorbed, lines) = crate::test_support::global_capture::capture_async(
            absorb_near_duplicate(&db, &snapshot),
        )
        .await;
        let absorbed = absorbed.unwrap();
        assert_eq!(absorbed, snapshot);
        let line = only_line(
            &lines,
            "[MemoryGate] Failed to reinforce near-duplicate memory",
        );
        assert!(line.starts_with("WARN "), "{line:?}");
        assert!(line.contains(" memoryId=mem-gone"), "{line:?}");
        assert!(line.contains(&format!(" characterId={CHAR}")), "{line:?}");
        assert!(
            !lines.iter().any(|l| l.contains("Near-duplicate absorbed")),
            "silence leg: {lines:?}"
        );
    }

    /// Bug 182 (v4 `70f9b495c`; the in-process analog of
    /// `child-proxy-memory-housekeeping.test.ts:171-208`): two reinforcements
    /// from the SAME snapshot both count — the increment reads the row at write
    /// time, so the second lands 3, not a second absolute 2.
    #[tokio::test]
    async fn two_reinforcements_from_one_snapshot_both_count() {
        let (_dir, db) = make_db(Some(("Mara trusts the crew.", "sum", vec![1.0, 0.0])));
        let snapshot = seed_row(&db);
        let provider = CannedEmbeddingProvider::new();
        let anchors = EpisodicAnchorView::default();
        for _ in 0..2 {
            reinforce_memory(
                &db,
                &provider,
                &snapshot,
                "Mara trusts the crew.",
                "user-1",
                None,
                &anchors,
            )
            .await
            .unwrap();
        }
        assert_eq!(cell::<f64>(&db, "reinforcementCount"), 3.0);
        assert_eq!(
            cell::<f64>(&db, "reinforcedImportance"),
            crate::memory_gate::calculate_reinforced_importance(0.5, 3.0)
        );
    }

    /// v4 `:175-201`: a row AT the footnote cap still counts and still takes
    /// its entity union, but appends nothing and is NOT re-embedded (no
    /// provider call at all) — the vector is frozen until consolidation.
    #[tokio::test]
    async fn at_cap_counts_and_unions_entities_without_re_embedding() {
        let (_dir, db) = make_db(Some(("x", "sum", vec![1.0, 0.0])));
        let capped = format!(
            "Mara sails.{}",
            (0..8)
                .map(|i| format!("\n[+] Detail{i}"))
                .collect::<String>()
        );
        set_cells(
            &db,
            &format!("content = '{capped}', entities = '[\"Mara\"]'"),
        )
        .await;
        let before_vector = vector_bytes(&db);
        let snapshot = seed_row(&db);
        let provider = Recording::new(CannedEmbeddingProvider::new());
        let anchors = EpisodicAnchorView {
            entities: vec!["Lisbon".into()],
            ..Default::default()
        };
        let (res, lines) = crate::test_support::global_capture::capture_async(reinforce_memory(
            &db,
            &provider,
            &snapshot,
            "Mara sailed to Lisbon with Joao.",
            "user-1",
            None,
            &anchors,
        ))
        .await;
        let (_, novel, counted) = res.unwrap();
        assert!(novel.is_empty(), "nothing appended at the cap: {novel:?}");
        assert_eq!(counted, 2.0);
        assert_eq!(cell::<String>(&db, "content"), capped);
        assert_eq!(cell::<String>(&db, "entities"), "[\"Mara\",\"Lisbon\"]");
        assert!(provider.calls().is_empty(), "no provider call at the cap");
        assert_eq!(vector_bytes(&db), before_vector);
        let line = only_line(
            &lines,
            "[MemoryGate] Reinforcement footnote cap reached; dropping extra details",
        );
        assert!(line.starts_with("DEBUG "), "{line:?}");
        assert!(line.contains(" memoryId=mem-seed"), "{line:?}");
        assert!(line.contains(" offered=2"), "{line:?}");
        assert!(line.contains(" appended=0"), "{line:?}");
        assert!(line.contains(" cap=8"), "{line:?}");
        assert!(
            !lines.iter().any(|l| l.contains("Re-embedded memory")),
            "{lines:?}"
        );
    }

    /// v4 `:202-222`: below the cap the details append (up to the cap) and the
    /// row re-embeds through EXACTLY ONE provider call; the DEBUG
    /// `[MemoryGate] Re-embedded memory` fires and the vector moves.
    #[tokio::test]
    async fn below_cap_appends_up_to_the_cap_and_re_embeds_once() {
        let (_dir, db) = make_db(Some(("x", "sum", vec![1.0, 0.0])));
        let seven = format!(
            "Mara sails.{}",
            (0..7)
                .map(|i| format!("\n[+] Detail{i}"))
                .collect::<String>()
        );
        set_cells(&db, &format!("content = '{seven}'")).await;
        let before_vector = vector_bytes(&db);
        let snapshot = seed_row(&db);
        let expected_content = format!("{seven}\n[+] Lisbon");
        let text = build_memory_embedding_text(
            "sum",
            &expected_content,
            Some(&EpisodicAnchorView::default()),
        );
        let provider = Recording::new(
            CannedEmbeddingProvider::new().with_vector(text.clone(), vec![0.0, 1.0]),
        );
        let anchors = EpisodicAnchorView::default();
        let (res, lines) = crate::test_support::global_capture::capture_async(reinforce_memory(
            &db,
            &provider,
            &snapshot,
            "Mara sailed to Lisbon with Joao.",
            "user-1",
            None,
            &anchors,
        ))
        .await;
        let (_, novel, counted) = res.unwrap();
        assert_eq!(novel, vec!["Lisbon".to_string()], "capped at 8");
        assert_eq!(counted, 2.0);
        assert_eq!(cell::<String>(&db, "content"), expected_content);
        assert_eq!(provider.calls(), vec![text], "exactly one provider call");
        assert_ne!(
            vector_bytes(&db),
            before_vector,
            "the re-embed rewrote the vector"
        );
        let line = only_line(&lines, "[MemoryGate] Re-embedded memory");
        assert!(line.starts_with("DEBUG "), "{line:?}");
        assert!(line.contains(" memoryId=mem-seed"), "{line:?}");
        assert!(
            !lines.iter().any(|l| l.contains("Failed to re-embed")),
            "silence leg: {lines:?}"
        );
        only_line(&lines, "Reinforcement footnote cap reached");
    }

    /// v4 `:612-626`: a row already holding 12 entities never "changes" — the
    /// bounded union is the same list, so no entity write and no re-embed.
    #[tokio::test]
    async fn a_twelve_entity_row_takes_no_entity_write() {
        let (_dir, db) = make_db(Some(("Mara trusts the crew.", "sum", vec![1.0, 0.0])));
        let twelve: Vec<String> = (0..12).map(|i| format!("E{i}")).collect();
        set_cells(
            &db,
            &format!("entities = '{}'", serde_json::to_string(&twelve).unwrap()),
        )
        .await;
        let snapshot = seed_row(&db);
        let provider = Recording::new(CannedEmbeddingProvider::new());
        let anchors = EpisodicAnchorView {
            entities: vec!["Fresh".into()],
            ..Default::default()
        };
        reinforce_memory(
            &db,
            &provider,
            &snapshot,
            "Mara trusts the crew.",
            "user-1",
            None,
            &anchors,
        )
        .await
        .unwrap();
        assert_eq!(
            cell::<String>(&db, "entities"),
            serde_json::to_string(&twelve).unwrap()
        );
        assert!(
            provider.calls().is_empty(),
            "no re-embed: {:?}",
            provider.calls()
        );
        assert_eq!(cell::<f64>(&db, "reinforcementCount"), 2.0);
    }

    // -----------------------------------------------------------------------
    // The gate's cold tier (v4 `d58548051`) — `gate-cold-tier.test.ts:79-145`
    // row for row, over a real DB: the candidate embeds to [1, 0], and each
    // seeded vector is the unit vector at the wanted cosine.
    // -----------------------------------------------------------------------

    struct TierSeed {
        id: &'static str,
        tier: &'static str,
        superseded_by: Option<&'static str>,
        source: &'static str,
        /// The cosine to the candidate; `None` → no vector (a digest).
        score: Option<f32>,
    }

    fn tier_seed(
        id: &'static str,
        tier: &'static str,
        superseded_by: Option<&'static str>,
        score: Option<f32>,
    ) -> TierSeed {
        TierSeed {
            id,
            tier,
            superseded_by,
            source: if id.starts_with("digest") {
                "CONSOLIDATED"
            } else {
                "AUTO"
            },
            score,
        }
    }

    fn make_tier_db(seeds: &[TierSeed]) -> (TempDir, Db) {
        crate::test_support::global_capture::install();
        let dir = tempdir().unwrap();
        let path = dir.path().join("main.db");
        {
            let w = Writer::open_writable(&path, PEPPER).unwrap();
            w.connection().execute_batch(DDL).unwrap();
            for seed in seeds {
                w.memories()
                    .create(
                        &MemCreate {
                            character_id: CHAR.to_string(),
                            about_character_id: None,
                            chat_id: None,
                            project_id: None,
                            content: format!("content {}", seed.id),
                            summary: "summary".to_string(),
                            keywords: vec![],
                            tags: vec![],
                            importance: 0.6,
                            embedding: seed
                                .score
                                .map(|score| vec![score, (1.0 - score * score).sqrt()]),
                            source: seed.source.to_string(),
                            witnessed_context: None,
                            occurred_at: None,
                            narrative_time: None,
                            entities: Vec::new(),
                            kind: "semantic".to_string(),
                            source_message_id: None,
                            last_accessed_at: None,
                            reinforcement_count: 1.0,
                            last_reinforced_at: None,
                            related_memory_ids: vec![],
                            reinforced_importance: 0.6,
                            tier: seed.tier.to_string(),
                            superseded_by_id: seed.superseded_by.map(str::to_string),
                            superseded_at: None,
                            consolidated_from: Vec::new(),
                            consolidated_at: None,
                        },
                        &CreateOptions {
                            id: seed.id.to_string(),
                            created_at: "2026-01-01T00:00:00.000Z".to_string(),
                            updated_at: "2026-01-01T00:00:00.000Z".to_string(),
                        },
                    )
                    .unwrap();
            }
        }
        let db = Db::open_main(&path, PEPPER).unwrap();
        (dir, db)
    }

    /// Run the gate for a candidate that embeds to [1, 0].
    async fn run_cold_gate(db: &Db) -> GateDecision {
        let provider =
            CannedEmbeddingProvider::new().with_vector("summary\n\ncontent", vec![1.0, 0.0]);
        run_memory_gate(
            db,
            &provider,
            CHAR,
            "content",
            "summary",
            "user-1",
            None,
            &EpisodicAnchorView::default(),
        )
        .await
        .unwrap()
        .decision
    }

    fn tier_of(db: &Db, id: &str) -> (String, Option<String>) {
        let id = id.to_string();
        db.read_main(move |c| {
            Ok(c.query_row(
                "SELECT tier, supersededById FROM memories WHERE id = ?",
                [&id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?)
        })
        .unwrap()
    }

    fn decided(decision: &GateDecision) -> (&'static str, Vec<String>) {
        match decision {
            GateDecision::Insert => ("INSERT", vec![]),
            GateDecision::Reinforce { existing } => ("REINFORCE", vec![id_of(existing).unwrap()]),
            GateDecision::SkipNearDuplicate { existing, .. } => {
                ("SKIP_NEAR_DUPLICATE", vec![id_of(existing).unwrap()])
            }
            GateDecision::InsertRelated { related } => (
                "INSERT_RELATED",
                related.iter().map(|r| id_of(&r.memory).unwrap()).collect(),
            ),
            GateDecision::SkipEmbeddingFailed { .. } => ("SKIP_EMBEDDING_FAILED", vec![]),
        }
    }

    /// v4 `:79-86`: a near-duplicate of a superseded cold row → its digest; no
    /// tier move.
    #[tokio::test]
    async fn cold_superseded_near_duplicate_redirects_to_the_digest() {
        let (_d, db) = make_tier_db(&[
            tier_seed("cold", "cold", Some("digest"), Some(0.95)),
            tier_seed("digest", "hot", None, None),
        ]);
        let d = run_cold_gate(&db).await;
        assert_eq!(
            decided(&d),
            ("SKIP_NEAR_DUPLICATE", vec!["digest".to_string()])
        );
        assert_eq!(tier_of(&db, "cold"), ("cold".into(), Some("digest".into())));
    }

    /// v4 `:88-94`: a REINFORCE-band match redirected to the digest is a
    /// count-only absorption (SKIP_NEAR_DUPLICATE — no footnote path).
    #[tokio::test]
    async fn cold_superseded_reinforce_band_is_count_only_on_the_digest() {
        let (_d, db) = make_tier_db(&[
            tier_seed("cold", "cold", Some("digest"), Some(0.87)),
            tier_seed("digest", "hot", None, None),
        ]);
        let d = run_cold_gate(&db).await;
        assert_eq!(
            decided(&d),
            ("SKIP_NEAR_DUPLICATE", vec!["digest".to_string()])
        );
    }

    /// v4 `:96-107`: two members of one digest in the related band → ONE link,
    /// onto the digest, at the higher similarity.
    #[tokio::test]
    async fn cold_members_collapse_onto_their_digest_in_the_related_band() {
        let (_d, db) = make_tier_db(&[
            tier_seed("cold1", "cold", Some("digest"), Some(0.78)),
            tier_seed("cold2", "cold", Some("digest"), Some(0.74)),
            tier_seed("digest", "hot", None, None),
        ]);
        let d = run_cold_gate(&db).await;
        assert_eq!(decided(&d), ("INSERT_RELATED", vec!["digest".to_string()]));
        let GateDecision::InsertRelated { related } = d else {
            unreachable!()
        };
        assert!(
            (related[0].similarity - 0.78).abs() < 1e-5,
            "{}",
            related[0].similarity
        );
    }

    /// v4 `:109-116`: a vanished digest → absorb into the cold row itself, and
    /// it is NOT promoted (its `supersededById` is still set — the code, not
    /// v4's doc comment).
    #[tokio::test]
    async fn a_vanished_digest_falls_back_to_the_cold_row_unpromoted() {
        let (_d, db) = make_tier_db(&[tier_seed("cold", "cold", Some("gone"), Some(0.95))]);
        let (d, lines) =
            crate::test_support::global_capture::capture_async(run_cold_gate(&db)).await;
        assert_eq!(
            decided(&d),
            ("SKIP_NEAR_DUPLICATE", vec!["cold".to_string()])
        );
        assert_eq!(tier_of(&db, "cold"), ("cold".into(), Some("gone".into())));
        let line = only_line(
            &lines,
            "[MemoryGate] Superseding digest missing; falling back to cold row",
        );
        assert!(line.starts_with("DEBUG "), "{line:?}");
        assert!(line.contains(" memoryId=cold"), "{line:?}");
        assert!(line.contains(" supersededById=gone"), "{line:?}");
        assert!(
            !lines.iter().any(|l| l.contains("Promoted cold memory")),
            "{lines:?}"
        );
    }

    /// v4 `:118-128`: a non-superseded cold row re-observed at the merge
    /// threshold is promoted (column `hot`, `supersededById` NULL) and then
    /// reinforced normally. R-B: a FRESH store load answers it hot — the column
    /// write is the re-stamp.
    #[tokio::test]
    async fn a_demoted_cold_row_is_promoted_and_reinforced() {
        let (_d, db) = make_tier_db(&[tier_seed("cold", "cold", None, Some(0.87))]);
        let before = db
            .read_main(|c| Ok(CharacterVectorStore::load(c, CHAR).is_hot("cold")))
            .unwrap();
        assert!(!before, "the store loads it cold first");
        let (d, lines) =
            crate::test_support::global_capture::capture_async(run_cold_gate(&db)).await;
        assert_eq!(decided(&d), ("REINFORCE", vec!["cold".to_string()]));
        let GateDecision::Reinforce { existing } = &d else {
            unreachable!()
        };
        assert_eq!(existing["tier"], "hot");
        assert_eq!(existing["supersededById"], Value::Null);
        assert_eq!(tier_of(&db, "cold"), ("hot".into(), None));
        let after = db
            .read_main(|c| Ok(CharacterVectorStore::load(c, CHAR).is_hot("cold")))
            .unwrap();
        assert!(after, "R-B: a fresh load answers the promoted row hot");
        let line = only_line(
            &lines,
            "[MemoryGate] Promoted cold memory to hot on re-observation",
        );
        assert!(line.starts_with("DEBUG "), "{line:?}");
        assert!(line.contains(" memoryId=cold"), "{line:?}");
        assert!(line.contains(&format!(" characterId={CHAR}")), "{line:?}");
        assert!(
            !lines
                .iter()
                .any(|l| l.contains("Superseding digest missing")),
            "{lines:?}"
        );
    }

    /// v4 `:130-136`: a cold row seen only in the related band is linked as-is
    /// and never promoted.
    #[tokio::test]
    async fn a_related_band_cold_row_is_linked_not_promoted() {
        let (_d, db) = make_tier_db(&[tier_seed("cold", "cold", None, Some(0.75))]);
        let d = run_cold_gate(&db).await;
        assert_eq!(decided(&d), ("INSERT_RELATED", vec!["cold".to_string()]));
        assert_eq!(tier_of(&db, "cold"), ("cold".into(), None));
    }

    /// v4 `:138-144`: hot matches exactly as before.
    #[tokio::test]
    async fn hot_matches_are_untouched() {
        let (_d, db) = make_tier_db(&[tier_seed("hot", "hot", None, Some(0.87))]);
        let d = run_cold_gate(&db).await;
        assert_eq!(decided(&d), ("REINFORCE", vec!["hot".to_string()]));
        assert_eq!(tier_of(&db, "hot"), ("hot".into(), None));
    }

    /// A 4-hop chain (A→B→C→D→E) stops at the third hop, on D — a cold,
    /// superseded row — and absorbs there (redirected → SKIP in the 0.85 band).
    #[tokio::test]
    async fn a_four_hop_chain_absorbs_on_the_third_hop() {
        let (_d, db) = make_tier_db(&[
            tier_seed("a", "cold", Some("b"), Some(0.87)),
            tier_seed("b", "cold", Some("c"), None),
            tier_seed("c", "cold", Some("d"), None),
            tier_seed("d", "cold", Some("e"), None),
            tier_seed("e", "hot", None, None),
        ]);
        let d = run_cold_gate(&db).await;
        assert_eq!(decided(&d), ("SKIP_NEAR_DUPLICATE", vec!["d".to_string()]));
        assert_eq!(tier_of(&db, "d"), ("cold".into(), Some("e".into())));
    }

    /// A chain ending on a cold, UNsuperseded digest promotes it with
    /// `redirected` still true → SKIP (count-only) in the 0.85 band.
    #[tokio::test]
    async fn a_chain_ending_cold_unsuperseded_promotes_and_skips() {
        let (_d, db) = make_tier_db(&[
            tier_seed("cold", "cold", Some("digest"), Some(0.87)),
            tier_seed("digest", "cold", None, None),
        ]);
        let d = run_cold_gate(&db).await;
        assert_eq!(
            decided(&d),
            ("SKIP_NEAR_DUPLICATE", vec!["digest".to_string()])
        );
        assert_eq!(tier_of(&db, "digest"), ("hot".into(), None));
    }

    /// The embedding provider failing on both attempts → SKIP_EMBEDDING_FAILED: no
    /// write, a reason surfaced.
    #[tokio::test]
    async fn embedding_failure_skips() {
        let (_dir, db) = make_db(None);
        let provider = CannedEmbeddingProvider::new().with_failure("s\n\nc");
        let outcome = create_memory_with_gate(&db, &provider, &candidate("c", "s"), &opts())
            .await
            .unwrap();
        assert_eq!(outcome.action, GateAction::SkipEmbeddingFailed);
        assert!(outcome.memory_id.is_none());
        assert!(outcome.reason.is_some());
        assert_eq!(count(&db, "memories"), 0);
    }

    // -----------------------------------------------------------------------
    // The three pre-`4a9be9878` `[MemoryGate]` WARNs (v4 `memory-gate.ts:194`,
    // `:382`, `:416`) — P4.88. Log-only: the gate's decision, the returned
    // triple and every `memories` row are identical whether or not these fire,
    // so the capturing layer over the REAL function is the only proof.
    // -----------------------------------------------------------------------

    fn only_line(lines: &[String], needle: &str) -> String {
        let hits: Vec<&String> = lines.iter().filter(|l| l.contains(needle)).collect();
        assert_eq!(hits.len(), 1, "expected exactly one {needle:?}: {lines:?}");
        hits[0].clone()
    }

    /// v4 `:194`. `error` is the RAW second-attempt provider message — v4's
    /// `secondMsg` — not the `Embedding failed after retry: …` sentence the same
    /// branch composes for the decision's `reason`.
    #[tokio::test]
    async fn embedding_failure_warns_with_v4s_fields() {
        let (_dir, db) = make_db(None);
        let provider = CannedEmbeddingProvider::new()
            .with_failure("s\n\nc")
            .with_failure_message("provider exploded");
        let (outcome, lines) = crate::test_support::global_capture::capture_async(
            create_memory_with_gate(&db, &provider, &candidate("c", "s"), &opts()),
        )
        .await;
        let outcome = outcome.unwrap();
        assert_eq!(outcome.action, GateAction::SkipEmbeddingFailed);
        let line = only_line(
            &lines,
            "[MemoryGate] Skipping memory write — embedding generation failed after retry",
        );
        assert!(line.starts_with("WARN "), "v4 logs at warn: {line:?}");
        assert!(line.contains(&format!(" characterId={CHAR}")), "{line:?}");
        assert!(line.contains(" userId=user-1"), "{line:?}");
        assert!(line.contains(" error=provider exploded"), "{line:?}");
        assert!(
            !line.contains("Embedding failed after retry"),
            "the composed reason is the DECISION's, not the log's: {line:?}"
        );
        assert_eq!(
            outcome.reason.as_deref(),
            Some("Embedding failed after retry: provider exploded"),
            "…and the decision still carries v4's composed sentence"
        );
    }

    /// v4 `:382`. Defensive on both sides — `existing` always came from a real
    /// row, so production never reaches it. Driven directly with a mismatched
    /// `characterId`, which is exactly what makes the character-scoped UPDATE
    /// match nothing.
    #[tokio::test]
    async fn a_failed_reinforcement_update_warns() {
        let (_dir, db) = make_db(Some(("seed content", "seed summary", vec![1.0, 0.0])));
        let provider = CannedEmbeddingProvider::new();
        let existing = serde_json::json!({
            "id": "mem-seed",
            "characterId": "someone-else",
            "content": "seed content",
            "summary": "seed summary",
            "importance": 0.5,
            "reinforcementCount": 1.0,
        });
        let anchors = crate::episodic::EpisodicAnchorView::default();
        let (res, lines) = crate::test_support::global_capture::capture_async(reinforce_memory(
            &db,
            &provider,
            &existing,
            "a different retelling entirely",
            "user-1",
            None,
            &anchors,
        ))
        .await;
        let (id, _novel, count_for_log) = res.unwrap();
        assert_eq!(id, "mem-seed");
        assert_eq!(
            count_for_log, 1.0,
            "v4 returns the EXISTING count when the update failed"
        );
        let line = only_line(
            &lines,
            "[MemoryGate] Failed to update memory during reinforcement",
        );
        assert!(line.starts_with("WARN "), "v4 logs at warn: {line:?}");
        assert!(line.contains(" memoryId=mem-seed"), "{line:?}");
        assert!(line.contains(" characterId=someone-else"), "{line:?}");
    }

    /// v4 `:416`. A content change forces the re-embed; the provider refuses, and
    /// v4's catch warns without failing the reinforcement.
    #[tokio::test]
    async fn a_failed_re_embed_warns_without_failing_the_reinforcement() {
        let (_dir, db) = make_db(Some(("seed content", "seed summary", vec![1.0, 0.0])));
        // A bare canned provider refuses every unregistered text, so whatever
        // `build_memory_embedding_text` composes here comes back an error.
        let provider = CannedEmbeddingProvider::new();
        let existing = serde_json::json!({
            "id": "mem-seed",
            "characterId": CHAR,
            "content": "seed content",
            "summary": "seed summary",
            "importance": 0.5,
            "reinforcementCount": 1.0,
        });
        // An anchor upgrade the existing row lacks is what makes
        // `anchorsChanged` true and forces the re-embed (novel-detail extraction
        // needs a date or proper noun, which this prose has not got).
        let anchors = crate::episodic::EpisodicAnchorView {
            occurred_at: Some("2021-05-05T00:00:00.000Z".to_string()),
            ..Default::default()
        };
        let (res, lines) = crate::test_support::global_capture::capture_async(reinforce_memory(
            &db,
            &provider,
            &existing,
            "a different retelling entirely",
            "user-1",
            None,
            &anchors,
        ))
        .await;
        let (id, _novel, count_for_log) = res.unwrap();
        assert_eq!(id, "mem-seed");
        assert_eq!(count_for_log, 2.0, "the reinforcement itself still landed");
        let line = only_line(&lines, "[MemoryGate] Failed to re-embed memory");
        assert!(
            !lines.iter().any(|l| l.contains("Re-embedded memory")),
            "silence leg: {lines:?}"
        );
        assert!(line.starts_with("WARN "), "v4 logs at warn: {line:?}");
        assert!(line.contains(" memoryId=mem-seed"), "{line:?}");
        assert!(
            line.contains(" error=EmbeddingError: no canned embedding registered"),
            "v4's `String(error)` renders `<name>: <message>`, and `EmbeddingError` \
             sets `this.name`: {line:?}"
        );
    }
}
