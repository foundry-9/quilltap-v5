//! Stage 1 — the turn's recall query, embedded once (v4
//! `lib/memory/recall/query.ts` `buildRecallQuery`, `584d28164`).
//!
//! Everything the later stages need to know about the question being asked:
//! the text, its ONE embedding, the distilled signals and what they imply.
//! The retrospective probes are embedded here too, so nothing downstream ever
//! embeds again — every pool of the turn (the head, the inter-character
//! relevance half, the recap's conversation list, the per-turn list, the
//! retrospective mini-recap) reuses this vector.
//!
//! The distiller itself (v4 `extractMemorySearchKeywords`, moved verbatim into
//! `query.ts` by Phase D) stays at [`crate::services::memory_recap::distill`]
//! (R-L12); this stage calls it.

use serde_json::{Map, Value};

use crate::cheap_llm::CheapLlmSelection;
use crate::model::completion::{CannedCompletionProvider, CompletionProvider};
use crate::model::embedding::{EmbeddingCallOptions, EmbeddingProvider, EmbeddingResult};
use crate::recall_tags::TimeWindow;
use crate::services::cheap_llm_exec::{CheapLlmTaskExecutor, CheapLlmTaskOptions};
use crate::services::memory_recap::distill::{
    distill_memory_search, DistillMessage, DistilledSearch, ExtractionClock,
};

use super::context::build_retrospective_probes;
use super::types::{num, trace_stage, RecallStage, RecallTrace};

const RECALL_LOG_TARGET: &str = "quilltap::recall";

/// The insertion-ordered text → embedding map behind [`EmbeddingMemo`] (v4's
/// `Map<string, EmbeddingResult>`): a re-set keeps the key's position, and
/// [`EmbeddingMemoMap::trim_to`] drops the OLDEST first.
#[derive(Debug, Default)]
pub struct EmbeddingMemoMap {
    order: std::collections::VecDeque<String>,
    map: std::collections::HashMap<String, EmbeddingResult>,
}

impl EmbeddingMemoMap {
    /// v4 `memo.get(key)` (a clone — the lock is never held across an await).
    pub fn get(&self, key: &str) -> Option<EmbeddingResult> {
        self.map.get(key).cloned()
    }

    /// v4 `memo.set(key, value)`.
    pub fn set(&mut self, key: String, value: EmbeddingResult) {
        if !self.map.contains_key(&key) {
            self.order.push_back(key.clone());
        }
        self.map.insert(key, value);
    }

    /// v4 `memo.size`.
    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// Drop the oldest entries `while len > max` (v4 `boundedMemo()`).
    pub fn trim_to(&mut self, max: usize) {
        while self.map.len() > max {
            let Some(oldest) = self.order.pop_front() else {
                break;
            };
            self.map.remove(&oldest);
        }
    }
}

/// v4 `embeddingMemo` — reuse embeddings by text across calls. The provider's
/// vectors for one text drift slightly from call to call; the replay harness
/// passes one memo so the runs it compares embed the same vectors. Live recall
/// leaves it off. Never lock it across an `.await`.
pub type EmbeddingMemo = std::sync::Arc<std::sync::Mutex<EmbeddingMemoMap>>;

/// A text and the vector embedded for it (`None` when embedding failed) —
/// v4 `EmbeddedText`.
#[derive(Clone, Debug)]
pub struct EmbeddedText {
    pub text: String,
    pub embedding: Option<EmbeddingResult>,
}

/// v4 `RecallQuery`.
#[derive(Clone, Debug, Default)]
pub struct RecallQuery {
    /// The text searched and embedded. Empty → there is nothing to search.
    pub text: String,
    /// The main query's embedding, or `None` (no profile, a failed call) — the
    /// text fallback then runs.
    pub embedding: Option<EmbeddingResult>,
    /// Retrospective probes (≤ 2), each embedded once here.
    pub probes: Vec<EmbeddedText>,
    /// The turn's distilled signals, when a distillation ran (or was given).
    pub signals: Option<DistilledSearch>,
    /// Entity anchors (the signals' entities, trimmed, non-empty).
    pub anchors: Vec<String>,
    /// The resolved event-time window, ungated from the retrospective flag.
    pub window: Option<TimeWindow>,
    pub retrospective: bool,
    /// Why `embedding` is `None`, when it is.
    pub embedding_error: Option<String>,
}

/// The distillation leg of [`BuildRecallQueryInput`] (v4 `input.distill`):
/// distil the signals here when none were handed in.
pub struct RecallDistill<'a, C: CompletionProvider> {
    pub completion: &'a C,
    pub executor: &'a CheapLlmTaskExecutor,
    pub messages: Vec<DistillMessage>,
    pub character_name: &'a str,
    pub selection: &'a CheapLlmSelection,
    pub character_id: &'a str,
    /// The distiller's clock. Its `time_zone` is the memory calendar's zone
    /// (bug 212) — the caller resolves it through
    /// [`crate::services::memory_time_zone::refresh_memory_time_zone`], as v4's
    /// `clock?.timeZone ?? await refreshMemoryTimeZone()` does.
    pub clock: Option<ExtractionClock>,
    /// v4 `latency` (the proactive pass's background default; the turn's
    /// fallback distil passes interactive — bug 115).
    pub latency: CheapLlmTaskOptions,
}

/// The type a caller names for "no distillation" (the one-shot search, the
/// replay's pinned signals): [`build_recall_query`] never calls it.
pub type NoDistill<'a> = RecallDistill<'a, CannedCompletionProvider>;

/// v4 `BuildRecallQueryInput`.
pub struct BuildRecallQueryInput<'a, C: CompletionProvider = CannedCompletionProvider> {
    pub user_id: &'a str,
    pub embedding_profile_id: Option<&'a str>,
    /// The fallback text: the recent-window prose, the tool's query, the draft.
    pub text: &'a str,
    /// Signals already distilled (the proactive pass, a pinned replay).
    pub signals: Option<DistilledSearch>,
    /// Distil the signals here when none were handed in. `None` → no
    /// distillation; the query is `text` as given.
    pub distill: Option<RecallDistill<'a, C>>,
    pub embedding_memo: Option<EmbeddingMemo>,
    /// Leave the query unembedded (the text fallback is all that will run).
    pub skip_embedding: bool,
    /// Probes to embed beside the query, instead of the ones the signals
    /// imply.
    pub probes: Option<Vec<String>>,
}

impl<'a> BuildRecallQueryInput<'a, CannedCompletionProvider> {
    /// An input with no distillation, no memo, no probes — the one-shot form.
    pub fn plain(user_id: &'a str, embedding_profile_id: Option<&'a str>, text: &'a str) -> Self {
        BuildRecallQueryInput {
            user_id,
            embedding_profile_id,
            text,
            signals: None,
            distill: None,
            embedding_memo: None,
            skip_embedding: false,
            probes: None,
        }
    }
}

/// v4 `recallQueryText` — the text the query searches: the paraphrase, else
/// the keywords, else the given text.
pub fn recall_query_text(signals: Option<&DistilledSearch>, fallback: &str) -> String {
    if let Some(s) = signals {
        // `if (signals?.paraphrase)` — an empty paraphrase is falsy.
        if let Some(p) = s.paraphrase.as_deref().filter(|p| !p.is_empty()) {
            return p.to_string();
        }
        if !s.keywords.is_empty() {
            return s.keywords.join(" ");
        }
    }
    fallback.to_string()
}

/// v4's `embed(value)` closure: the memo first (key
/// `` `${embeddingProfileId ?? 'default'}|${value}` ``), else the provider,
/// then `memo.set`. v4 passes no priority — `interactive` is its default — and
/// no `lexicalText` (C1-10).
async fn embed<E: EmbeddingProvider>(
    embedding: &E,
    user_id: &str,
    embedding_profile_id: Option<&str>,
    memo: Option<&EmbeddingMemo>,
    value: &str,
) -> Result<EmbeddingResult, String> {
    let key = format!("{}|{value}", embedding_profile_id.unwrap_or("default"));
    if let Some(memo) = memo {
        if let Some(hit) = memo.lock().unwrap_or_else(|p| p.into_inner()).get(&key) {
            return Ok(hit);
        }
    }
    let result = embedding
        .generate_embedding_for_user(
            value,
            user_id,
            embedding_profile_id,
            EmbeddingCallOptions::interactive(),
        )
        .await
        // v4 `error instanceof Error ? error.message : String(error)`.
        .map_err(|e| e.message)?;
    if let Some(memo) = memo {
        memo.lock()
            .unwrap_or_else(|p| p.into_inner())
            .set(key, result.clone());
    }
    Ok(result)
}

/// v4 `buildRecallQuery` — distil (when asked and not already done), choose
/// the text, and embed it, once. Never fails: a failed distillation falls
/// back to the given text, a failed embedding leaves `embedding` `None` and the
/// text fallback runs.
///
/// v4's `catch` around the distil call (DEBUG `[Recall] Query distillation
/// failed; using the recent-window text`) is UNREACHABLE through v4's real
/// code — `extractMemorySearchKeywords` resolves `{ success: false }` rather
/// than throwing (its executor catches every provider failure, and
/// `refreshMemoryTimeZone` never fails) — and v5's distiller returns an
/// `Option`, so the line has no v5 twin (§R.5's negative-pin rule: no
/// reachable home exists on either side).
pub async fn build_recall_query<E, C>(
    embedding: &E,
    input: BuildRecallQueryInput<'_, C>,
    trace: &mut RecallTrace,
) -> RecallQuery
where
    E: EmbeddingProvider,
    C: CompletionProvider,
{
    let mut signals = input.signals;
    let mut distilled = if signals.is_some() { "given" } else { "none" };
    if signals.is_none() && !crate::jsstr::js_trim(input.text).is_empty() {
        if let Some(d) = &input.distill {
            match distill_memory_search(
                d.executor,
                d.completion,
                &d.messages,
                d.character_name,
                d.selection,
                d.character_id,
                d.clock.as_ref(),
                d.latency,
            )
            .await
            {
                Some(result) => {
                    signals = Some(result);
                    distilled = "ran";
                }
                None => distilled = "failed",
            }
        }
    }

    let text = crate::jsstr::js_trim(&recall_query_text(signals.as_ref(), input.text)).to_string();
    let retrospective = signals.as_ref().is_some_and(|s| s.retrospective);
    let probe_texts: Vec<String> = input
        .probes
        .clone()
        .or_else(|| build_retrospective_probes(signals.as_ref(), retrospective))
        .unwrap_or_default()
        .iter()
        .map(|p| crate::jsstr::js_trim(p).to_string())
        .filter(|p| !p.is_empty())
        .take(2)
        .collect();

    let memo = input.embedding_memo.as_ref();
    let mut query_embedding: Option<EmbeddingResult> = None;
    let mut embedding_error: Option<String> = None;
    let mut probes: Vec<EmbeddedText> = Vec::new();
    if !text.is_empty() && !input.skip_embedding {
        match embed(
            embedding,
            input.user_id,
            input.embedding_profile_id,
            memo,
            &text,
        )
        .await
        {
            Ok(r) => query_embedding = Some(r),
            Err(error) => {
                tracing::warn!(
                    target: RECALL_LOG_TARGET,
                    userId = %input.user_id,
                    query = %crate::jsstr::utf16_truncate(&text, 100),
                    error = %error,
                    "[Recall] Query embedding failed; recall falls back to text search"
                );
                embedding_error = Some(error);
            }
        }
        if query_embedding.is_some() {
            for probe in &probe_texts {
                match embed(
                    embedding,
                    input.user_id,
                    input.embedding_profile_id,
                    memo,
                    probe,
                )
                .await
                {
                    Ok(r) => probes.push(EmbeddedText {
                        text: probe.clone(),
                        embedding: Some(r),
                    }),
                    Err(error) => {
                        probes.push(EmbeddedText {
                            text: probe.clone(),
                            embedding: None,
                        });
                        tracing::debug!(
                            target: RECALL_LOG_TARGET,
                            probe = %crate::jsstr::utf16_truncate(probe, 80),
                            error = %error,
                            "[Recall] Probe embedding failed; skipping probe"
                        );
                    }
                }
            }
        }
    }
    if query_embedding.is_none() {
        // The text fallback still reads the probes' words.
        for probe in &probe_texts {
            if !probes.iter().any(|p| &p.text == probe) {
                probes.push(EmbeddedText {
                    text: probe.clone(),
                    embedding: None,
                });
            }
        }
    }

    let anchors: Vec<String> = signals
        .as_ref()
        .map(|s| {
            s.entities
                .iter()
                .map(|e| crate::jsstr::js_trim(e).to_string())
                .filter(|e| !e.is_empty())
                .collect()
        })
        .unwrap_or_default();
    let window = signals.as_ref().and_then(|s| s.time_range.clone());

    let mut detail = Map::new();
    detail.insert("distilled".into(), Value::from(distilled));
    detail.insert(
        "textLength".into(),
        num(crate::jsstr::utf16_len(&text) as f64),
    );
    detail.insert("embedded".into(), Value::Bool(query_embedding.is_some()));
    let embedding_calls = if query_embedding.is_some() {
        1 + probes.iter().filter(|p| p.embedding.is_some()).count()
    } else {
        0
    };
    detail.insert("embeddingCalls".into(), num(embedding_calls as f64));
    detail.insert("probes".into(), num(probes.len() as f64));
    detail.insert("anchors".into(), num(anchors.len() as f64));
    detail.insert("window".into(), window_json(window.as_ref()));
    detail.insert("retrospective".into(), Value::Bool(retrospective));
    trace_stage(trace, RecallStage::Query, detail, None);

    RecallQuery {
        text,
        embedding: query_embedding,
        probes,
        signals,
        anchors,
        window,
        retrospective,
        embedding_error,
    }
}

/// A `{from, to}` window as v4's JSON (`null` when absent).
pub fn window_json(window: Option<&TimeWindow>) -> Value {
    match window {
        Some(w) => {
            let mut m = Map::new();
            m.insert("from".into(), Value::String(w.from.clone()));
            m.insert("to".into(), Value::String(w.to.clone()));
            Value::Object(m)
        }
        None => Value::Null,
    }
}
