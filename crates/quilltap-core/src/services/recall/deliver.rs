//! Stage 8 — deliver: the only writer of "what was shown" (v4
//! `lib/memory/recall/deliver.ts`, `584d28164`).
//!
//! v4's why, carried: it "runs after the request body is assembled — after
//! Phase-2 memory compression has had its say — with the text that actually
//! goes to the model. A rendered line counts as delivered only when its
//! memory's words are still in that text (a line compression cut is not
//! delivered). For what was delivered, and only that, it:
//!
//! - stamps `lastAccessedAt` (the recent-access term of `protection`);
//! - appends the head's ids to the chat's anti-repetition ring, and the
//!   retrospective mini-recap's signature to its spam guard;
//! - writes the turn's debug record (the stage trace, the delivered pools and
//!   a machine-readable ring line the replay reads back).
//!
//! A search never stamps anything: the head over-fetches about three times
//! what it shows, and stamping every candidate once left 83% of a corpus
//! 'recently accessed'."
//!
//! [`stamp_accessed`] is the ONE writer of `memories.lastAccessedAt` in v5
//! (R-AG; pinned by `last_accessed_at_writer_census`).

use std::sync::LazyLock;

use regex::Regex;
use serde_json::{Map, Value};

use crate::db::runtime::Db;

use super::assemble::RecallBundle;
use super::gather::significant_query_words;
use super::types::{RecallPool, RecallTraceEntry, RenderedLine};

const RECALL_LOG_TARGET: &str = "quilltap::recall";

/// Every recall debug line starts with this.
pub const RECALL_DEBUG_PREFIX: &str = "[Recall]";
/// The machine-readable line carrying the head ids delivered this turn.
pub const RECALL_RING_PREFIX: &str = "[Recall:ring]";

/// How much of a line's significant words one delivered line must hold to
/// count as carrying it.
pub const SURVIVAL_WORD_SHARE: f64 = 0.6;

/// v4 `stampAccessed(characterId, memoryIds)` — stamp `lastAccessedAt` on rows
/// a consumer actually used (whispered into a prompt, returned to a model as
/// tool output, shown to an answerer). The ONE writer of the column.
/// Character-scoped, and never fails the caller.
///
/// - The ids: non-empty strings, de-duplicated in first-seen order; none →
///   return silently.
/// - DEBUG `[Recall] Stamping delivered memories accessed {characterId,
///   count}` — on the CALLER thread, before the write (memory note
///   `capture-rig-writer-thread`).
/// - A failed write logs the REPOSITORY's lines (`updateAccessTimeBulk` is a
///   FALLBACK `safeQuery` over the base `updateMany` RETHROW) on this thread.
///   v4's own WARN `[Recall] Failed to stamp lastAccessedAt on delivered
///   memories` is UNREACHABLE through v4's real code — the fallback never
///   rejects — and is pinned NEGATIVE (§R.5; P4.D268's pin, carried).
pub async fn stamp_accessed(db: &Db, character_id: &str, memory_ids: &[String]) {
    let mut seen = std::collections::HashSet::new();
    let ids: Vec<String> = memory_ids
        .iter()
        .filter(|id| !id.is_empty() && seen.insert(id.as_str()))
        .cloned()
        .collect();
    if ids.is_empty() {
        return;
    }
    tracing::debug!(
        target: RECALL_LOG_TARGET,
        characterId = %character_id,
        count = ids.len(),
        "[Recall] Stamping delivered memories accessed"
    );
    let count = ids.len();
    let cid = character_id.to_string();
    let result = db
        .write(move |writers| {
            writers
                .main()
                .memories()
                .update_access_time_bulk(&cid, &ids)
        })
        .await;
    let _ = crate::db::fallback::memory_access_times_or_zero(character_id, count, || result);
}

/// v4 `normalise` — lower-cased, whitespace runs collapsed, trimmed.
fn normalise(text: &str) -> String {
    let lower = text.to_lowercase();
    let mut out = String::with_capacity(lower.len());
    let mut in_ws = false;
    for c in lower.chars() {
        if crate::jsstr::is_js_ws(c) {
            if !in_ws {
                out.push(' ');
            }
            in_ws = true;
        } else {
            out.push(c);
            in_ws = false;
        }
    }
    crate::jsstr::js_trim(&out).to_string()
}

/// v4's `/[^\p{L}\p{N}\s'-]/gu` (JS `\s` spelled out — JS's set, not
/// Unicode's `White_Space`).
static NON_WORD: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"[^\p{L}\p{N}'\-\t\n\x0B\x0C\r \x{A0}\x{1680}\x{2000}-\x{200A}\x{2028}\x{2029}\x{202F}\x{205F}\x{3000}\x{FEFF}]",
    )
    .expect("the survival strip compiles")
});

fn significant_words(text: &str) -> Vec<String> {
    significant_query_words(&NON_WORD.replace_all(text, " "))
}

/// v4 `survivingLines` — the rendered lines whose memory reached the model. A
/// line survives when the WHOLE rendered line is in the delivered text
/// verbatim (whitespace and case aside) — always the case without
/// compression. The whole line, not just the memory's words: a short body
/// ("likes tea") can turn up in another line and say nothing about this one.
/// Only when the section was rewritten by compression (`compressed`) does a
/// line also survive when one delivered line holds at least
/// [`SURVIVAL_WORD_SHARE`] of its significant words (three or more of them).
/// A line compression dropped is not delivered.
///
/// The `compressed` arm is dead in production until Phase-2 memory
/// compression lands (R-U — ported and tier-1 pinned now).
pub fn surviving_lines<'l>(
    lines: &'l [RenderedLine],
    delivered_text: &str,
    compressed: bool,
) -> Vec<&'l RenderedLine> {
    if delivered_text.is_empty() {
        return Vec::new();
    }
    let whole = normalise(delivered_text);
    let delivered_lines: Vec<std::collections::HashSet<String>> = delivered_text
        .split('\n')
        .map(|l| significant_words(l).into_iter().collect())
        .collect();
    lines
        .iter()
        .filter(|line| {
            let rendered = normalise(&line.line);
            if !rendered.is_empty() && whole.contains(&rendered) {
                return true;
            }
            if !compressed {
                return false;
            }
            let mut seen = std::collections::HashSet::new();
            let words: Vec<String> = significant_words(&line.body)
                .into_iter()
                .filter(|w| seen.insert(w.clone()))
                .collect();
            if words.len() < 3 {
                return false;
            }
            delivered_lines.iter().any(|set| {
                words.iter().filter(|w| set.contains(*w)).count() as f64 / words.len() as f64
                    >= SURVIVAL_WORD_SHARE
            })
        })
        .collect()
}

/// What a turn's delivery did (v4 `RecallDelivery`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RecallDelivery {
    /// Ids delivered, by pool, in the bundle's pool order (v4's
    /// `Object.entries(bundle.lines)` insertion order).
    pub delivered: Vec<(RecallPool, Vec<String>)>,
    /// Ids rendered but not delivered (cut by compression).
    pub lost: Vec<String>,
    /// The lines for the assistant message's `debugMemoryLogs`.
    pub debug_log: Vec<String>,
}

impl RecallDelivery {
    /// The delivered ids of one pool (empty when the pool delivered nothing).
    pub fn pool(&self, pool: RecallPool) -> &[String] {
        self.delivered
            .iter()
            .find(|(p, _)| *p == pool)
            .map(|(_, ids)| ids.as_slice())
            .unwrap_or(&[])
    }
}

/// v4 `DeliverOptions`.
#[derive(Clone, Copy, Debug)]
pub struct DeliverOptions<'a> {
    pub character_id: &'a str,
    pub chat_id: &'a str,
    /// The chat's raw `commonplaceRecallHistory` cell.
    pub commonplace_recall_history: &'a Value,
    /// The recall text the request body carries (the trailing section).
    pub delivered_text: &'a str,
    /// Record the head in the anti-repetition ring (off for the replay).
    pub persist_ring: bool,
}

/// v4 `traceLines` — a stage trace as debug lines:
/// `[Recall] <stage>[ (<label>)]: <JSON.stringify(detail)>`.
pub fn trace_lines(trace: &[RecallTraceEntry]) -> Vec<String> {
    trace
        .iter()
        .map(|entry| {
            let label = match &entry.label {
                Some(l) if !l.is_empty() => format!(" ({l})"),
                _ => String::new(),
            };
            format!(
                "{RECALL_DEBUG_PREFIX} {}{label}: {}",
                entry.stage.as_str(),
                serde_json::to_string(&entry.detail).unwrap_or_default()
            )
        })
        .collect()
}

/// v4 `deliverRecall` — deliver a turn's recall bundle: decide what reached
/// the model, stamp it, record the head in the ring, and compose the debug
/// record.
pub async fn deliver_recall(
    db: &Db,
    bundle: &RecallBundle,
    opts: DeliverOptions<'_>,
) -> RecallDelivery {
    let mut delivered: Vec<(RecallPool, Vec<String>)> = Vec::new();
    let mut lost: Vec<String> = Vec::new();
    // A section still in the text verbatim was not compressed: its lines count
    // only verbatim. A rewritten one is read by word overlap.
    let untouched = |section: Option<&String>| {
        section.is_some_and(|s| opts.delivered_text.contains(crate::jsstr::js_trim(s)))
    };
    let relevant_compressed = !untouched(bundle.sections.relevant.as_ref());
    let inter_compressed = !untouched(bundle.sections.inter_character.as_ref());
    for (pool, lines) in &bundle.lines {
        if lines.is_empty() {
            continue;
        }
        let compressed = match pool {
            RecallPool::Archive | RecallPool::Head => relevant_compressed,
            RecallPool::InterCharacter => inter_compressed,
            RecallPool::Recap | RecallPool::Relevant => false,
        };
        let kept = surviving_lines(lines, opts.delivered_text, compressed);
        let mut kept_ids: Vec<String> = Vec::new();
        for line in kept {
            if !kept_ids.contains(&line.memory_id) {
                kept_ids.push(line.memory_id.clone());
            }
        }
        for line in lines {
            if !kept_ids.contains(&line.memory_id) {
                lost.push(line.memory_id.clone());
            }
        }
        delivered.push((*pool, kept_ids));
    }
    // The recap is a narrative, not lines: the rows it summarised did not reach
    // the model in their own words, so they are not stamped (only the ones a
    // line-rendered pool delivered are).
    let all: Vec<String> = delivered.iter().flat_map(|(_, ids)| ids.clone()).collect();
    stamp_accessed(db, opts.character_id, &all).await;

    let head: Vec<String> = delivered
        .iter()
        .find(|(p, _)| *p == RecallPool::Head)
        .map(|(_, ids)| ids.clone())
        .unwrap_or_default();
    // The whisper builder trims each section; compare what it actually carries.
    let retrospective = bundle
        .sections
        .retrospective
        .as_deref()
        .map(crate::jsstr::js_trim)
        .filter(|r| !r.is_empty());
    let retro_delivered = retrospective.is_some_and(|r| opts.delivered_text.contains(r));
    let signature = if retro_delivered {
        bundle.retro_signature.clone()
    } else {
        None
    };
    // `head.length > 0 || signature` — an empty signature string is falsy.
    let signature = signature.filter(|s| !s.is_empty());
    if opts.persist_ring && (!head.is_empty() || signature.is_some()) {
        let mut next =
            crate::recall_history::append_recall_turn(opts.commonplace_recall_history, &head);
        if let Some(sig) = &signature {
            next = crate::recall_history::append_retro_signature(&next, sig);
        }
        let chat_id = opts.chat_id.to_string();
        let value = next.to_value();
        let result = db
            .write(move |writers| {
                let repo = crate::db::chats::ChatsRepository::new(writers.main().connection());
                repo.update(
                    &chat_id,
                    &crate::db::chats::ChatUpdate {
                        commonplace_recall_history: Some(value),
                        ..Default::default()
                    },
                )
                .map(|_| ())
            })
            .await;
        if let Err(error) = result {
            tracing::warn!(
                target: RECALL_LOG_TARGET,
                chatId = %opts.chat_id,
                error = %crate::db::fallback::error_text(&error),
                "[Recall] Failed to persist the anti-repetition ring"
            );
        }
    }

    let counts: Vec<String> = delivered
        .iter()
        .map(|(pool, ids)| format!("{} {}", pool.as_str(), ids.len()))
        .collect();
    let counts = counts.join(", ");
    let mut debug_log = trace_lines(&bundle.trace);
    debug_log.push(format!(
        "{RECALL_DEBUG_PREFIX} deliver: {}{}",
        if counts.is_empty() {
            "nothing"
        } else {
            &counts
        },
        if lost.is_empty() {
            String::new()
        } else {
            format!("; {} cut by compression", lost.len())
        }
    ));
    debug_log.push(
        crate::jsstr::js_trim_end(&format!("{RECALL_RING_PREFIX} {}", head.join(" "))).to_string(),
    );
    let mut delivered_counts = Map::new();
    for (pool, ids) in &delivered {
        delivered_counts.insert(pool.as_str().to_string(), Value::from(ids.len()));
    }
    let delivered_json = Value::Object(delivered_counts);
    tracing::debug!(
        target: RECALL_LOG_TARGET,
        chatId = %opts.chat_id,
        characterId = %opts.character_id,
        deliveredJson = %delivered_json,
        lost = lost.len(),
        "[Recall] Delivered"
    );
    RecallDelivery {
        delivered,
        lost,
        debug_log,
    }
}

/// v4 `ringFromDebugLog` — the head ids a debug record says were delivered,
/// or `None` when it has no ring line.
pub fn ring_from_debug_log(lines: Option<&[String]>) -> Option<Vec<String>> {
    let line = lines?.iter().find(|l| l.starts_with(RECALL_RING_PREFIX))?;
    Some(
        crate::jsstr::js_trim(&line[RECALL_RING_PREFIX.len()..])
            .split(crate::jsstr::is_js_ws)
            .filter(|id| !id.is_empty())
            .map(str::to_string)
            .collect(),
    )
}

/// v4 `withRecallLines` — a message's debug log with a new set of non-recall
/// lines (an extraction job's), keeping the recall record the turn wrote,
/// first.
pub fn with_recall_lines(existing: Option<&[String]>, lines: &[String]) -> Vec<String> {
    let mut out: Vec<String> = existing
        .unwrap_or(&[])
        .iter()
        .filter(|l| l.starts_with(RECALL_DEBUG_PREFIX) || l.starts_with(RECALL_RING_PREFIX))
        .cloned()
        .collect();
    out.extend(lines.iter().cloned());
    out
}
