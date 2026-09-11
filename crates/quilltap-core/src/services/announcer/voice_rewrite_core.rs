//! Shared core for the two "say it in the character's own voice" rehearsals —
//! v4 `lib/services/announcer/voice-rewrite-core.ts` (`686954937`).
//!
//! Both rehearsals hand a character a draft the operator typed and ask for it
//! back in the character's own voice. They differ only in framing:
//!
//!   - [`super::character_voiced`] is OFF-SCENE: the character stands outside
//!     the conversation and speaks in to the people in it (Insert Announcement).
//!   - [`super::in_scene_voiced`] is IN-SCENE: the character is in the room, has
//!     been following along, and it is now their turn to say the drafted line
//!     (the impersonated-seat rewrite).
//!
//! What the two share — the Commonplace recall against the draft, the
//! `executeCheapLLMTask` call, and the never-throws result shape — lives here so
//! the two framings cannot drift apart in everything except their wording.
//!
//! Nothing in this module persists anything. (The recall does bump
//! `lastAccessedAt` on the memories it returns, which is
//! `searchMemoriesSemantic`'s own write, not this module's.)
//!
//! **The extraction is behaviour-neutral, proven not read.**
//! `announcer_tier3_equivalence`'s NDJSON regenerated from a worktree pinned at
//! `cc65d6bfc` (pre-extraction) and one pinned at `f4ad2c8d1` (post-extraction)
//! is byte-identical, and v5's own tree is green against both — see that
//! family's header.
//!
//! ## Where v5's `recall_for_seed` differs from v4's signature
//!
//! v4 reads its repositories and its embedding provider out of module-level
//! singletons and its clock off `Date.now()`; v5 injects all three (`db`,
//! `embedding`, `now_ms`) because the core is a library. v4 also passes
//! `profile.provider` to `formatDynamicMemoryHead`, which v5's port of that
//! function does not take — measured unused at the P4.9E2A port and pinned ever
//! since by this family's assembled-prompt comparand, so the `profile` argument
//! is gone here rather than carried as decoration.

use serde_json::Value;

use crate::cheap_llm::CheapLlmSelection;
use crate::db::runtime::Db;
use crate::jsstr::js_trim;
use crate::memory_injector::{format_dynamic_memory_head, InjectorResult, RecallAdjustment};
use crate::model::completion::{CompletionMessage, CompletionProvider};
use crate::model::embedding::EmbeddingProvider;
use crate::services::cheap_llm_exec::{CheapLlmTaskExecutor, CheapLlmTaskOptions};
use crate::services::commonplace_notifications::{build_commonplace_llm_context, CommonplaceParts};
use crate::services::memory_service::{
    search_memories_semantic, SemanticSearchOptions, SemanticSearchResult,
};

/// v4 `RECALL_LIMIT` — upper bound on how many memories the recall considers.
const RECALL_LIMIT: usize = 20;
/// v4 `RECALL_MIN_IMPORTANCE` — floor on memory importance for a rehearsal.
const RECALL_MIN_IMPORTANCE: f64 = 0.3;
/// v4 `RECALL_MAX_ENTRIES` — how many recalled memories survive into the block.
const RECALL_MAX_ENTRIES: usize = 12;

/// v4 `VoiceRewriteResult` — the shape both rehearsals return. Never throws;
/// failure is a field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoiceRewriteResult {
    pub success: bool,
    pub proposed_markdown: String,
    pub error: Option<String>,
}

/// v4 `formatNameList` — `"Alice"`, `"Alice and Bob"`, `"Alice, Bob, and Carol"`.
///
/// (The zero-length case is unreachable: every call site guards on a non-empty
/// audience, and v4's own `names[names.length - 1]` would be `undefined` there.)
pub fn format_name_list(names: &[String]) -> String {
    match names {
        [] => String::new(),
        [one] => one.clone(),
        [a, b] => format!("{a} and {b}"),
        [head @ .., last] => format!("{}, and {last}", head.join(", ")),
    }
}

/// Project a semantic-search result into the injector shape the memory formatter
/// reads (mirrors `build_context`'s private `injector_result_from_search`).
fn injector_result_from_search(r: &SemanticSearchResult) -> InjectorResult {
    InjectorResult {
        memory: crate::services::carina_query::injector_memory_from_json(&r.memory),
        score: r.score,
        effective_weight: Some(r.effective_weight),
        raw_weight: Some(r.raw_weight),
        recall_adjustment: r.recall_adjustment.as_ref().map(|a| RecallAdjustment {
            multiplier: Some(a.multiplier),
            fired: a.fired.clone(),
            blended_before: Some(a.blended_before),
            blended_after: Some(a.blended_after),
        }),
    }
}

/// v4 `recallForSeed` — Commonplace Book recall against the operator's draft,
/// rendered as the block that leads the user-role message.
///
/// A recall failure is logged and tolerated — it is context, not payload, and a
/// dead embedding provider must not cost the operator their rehearsal. Returns
/// an empty string when there is nothing to say.
///
/// `log_context` is the caller's bracketed tag (`[CharacterVoicedAnnouncement]`
/// / `[InSceneVoicedLine]`), which is the only thing the warn line's bytes vary
/// by — v4 interpolates it the same way.
pub async fn recall_for_seed<E>(
    db: &Db,
    embedding: &E,
    character_id: &str,
    seed_markdown: &str,
    chat_id: &str,
    now_ms: f64,
    log_context: &str,
) -> String
where
    E: EmbeddingProvider,
{
    let memory_results = search_memories_semantic(
        db,
        embedding,
        character_id,
        seed_markdown,
        &SemanticSearchOptions {
            limit: Some(RECALL_LIMIT),
            min_importance: Some(RECALL_MIN_IMPORTANCE),
            now_ms,
            ..Default::default()
        },
        None,
    )
    .await;

    let results = match memory_results {
        // v4 `if (memoryResults.length === 0) return ''`.
        Ok(results) if results.is_empty() => return String::new(),
        Ok(results) => results,
        Err(err) => {
            // v4's catch: warn and proceed without. `chatId`/`characterId`/
            // `error` are v4's bag keys, in v4's order.
            tracing::warn!(
                chatId = %chat_id,
                characterId = %character_id,
                error = %err,
                "{log_context} Memory recall failed; proceeding without"
            );
            return String::new();
        }
    };

    let injector: Vec<InjectorResult> = results.iter().map(injector_result_from_search).collect();
    // The recall spans the character's whole store, so it carries their memories
    // about other people too; attribute them or the rewrite reads someone else's
    // life as its own (v4 `d883a5ee1`, bug 122).
    let subject = crate::services::memory_subject::build_memory_subject_context(
        db,
        character_id,
        injector.iter().map(|r| r.memory.about_character_id.clone()),
    );
    let formatted =
        format_dynamic_memory_head(&injector, None, Some(RECALL_MAX_ENTRIES), now_ms, &subject);
    // v4 `if (!formatted.content) return ''`.
    if formatted.content.is_empty() {
        return String::new();
    }
    build_commonplace_llm_context(&CommonplaceParts {
        relevant: Some(formatted.content),
        ..Default::default()
    })
}

/// v4 `ExecuteVoiceRewriteParams`.
pub struct ExecuteVoiceRewriteParams<'a> {
    pub selection: &'a CheapLlmSelection,
    pub messages: Vec<CompletionMessage>,
    pub task_type: &'a str,
    pub character_id: &'a str,
    pub max_tokens: f64,
}

/// v4 `executeVoiceRewrite` — run the rewrite and normalise the outcome.
///
/// An empty or failed completion is reported as `success: false` with the
/// provider's own message, which is what both dialogs surface to the operator.
/// v4 passes `undefined` for the uncensored fallback at both call sites; the
/// `chatId` v4 threads for its log row is carried by v5's executor construction
/// (the host's per-request logging executor), not by this argument list.
pub async fn execute_voice_rewrite<C>(
    completion: &C,
    executor: &CheapLlmTaskExecutor,
    params: ExecuteVoiceRewriteParams<'_>,
) -> VoiceRewriteResult
where
    C: CompletionProvider,
{
    let llm_result = executor
        .execute(
            completion,
            params.selection,
            params.messages,
            |content: &str| js_trim(content).to_string(),
            None,
            Some(params.max_tokens),
            Some(params.character_id),
            Some(params.task_type),
            CheapLlmTaskOptions::default(),
        )
        .await;

    // v4: `if (!llmResult.success || !llmResult.result)` — an empty-string
    // result is falsy in JS, so a whitespace-only completion (the parser trims)
    // takes the failure arm too.
    let result = llm_result.result.filter(|r| !r.is_empty());
    match (llm_result.success, result) {
        (true, Some(proposed)) => VoiceRewriteResult {
            success: true,
            proposed_markdown: proposed,
            error: None,
        },
        _ => VoiceRewriteResult {
            success: false,
            proposed_markdown: String::new(),
            error: Some(
                llm_result
                    .error
                    .filter(|e| !e.is_empty())
                    .unwrap_or_else(|| "The LLM returned no content.".to_string()),
            ),
        },
    }
}

/// `profile.provider`-shaped read used by both rehearsals' selection builders.
pub(super) fn s(v: &Value, k: &str) -> Option<String> {
    v.get(k).and_then(Value::as_str).map(str::to_string)
}

/// v4 `buildSelection(profile)` / `selectionFromProfile(profile)` — straight off
/// the operator's chosen profile. The profile's provider params (e.g. DeepSeek
/// thinking mode) are forwarded so per-model settings take effect for this
/// utility call too.
pub(super) fn build_selection(profile: &Value) -> CheapLlmSelection {
    let provider = s(profile, "provider").unwrap_or_default();
    CheapLlmSelection {
        is_local: provider == "OLLAMA",
        provider,
        model_name: s(profile, "modelName").unwrap_or_default(),
        // JS `profile.baseUrl || undefined` — the empty string is falsy.
        base_url: s(profile, "baseUrl").filter(|u| !u.is_empty()),
        connection_profile_id: s(profile, "id"),
        // v4 `d9c5a1c7` converted this inline construction to the shared
        // `profileParams()` helper, so the Ollama `num_ctx` injection applies
        // to this utility call too.
        profile_parameters: crate::cheap_llm::profile_params_value(profile),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::completion::{
        CompletionError, CompletionParams, CompletionResponse, CompletionRole,
    };
    use std::sync::Mutex;

    /// A provider that answers everything and records the `max_tokens` it was
    /// handed. The canned provider keys on `provider|model|temperature|messages`
    /// — which deliberately EXCLUDES `max_tokens` — so this is the only seam
    /// that can see the ceiling at all.
    #[derive(Default)]
    struct MaxTokensSpy(Mutex<Vec<Option<i64>>>);

    impl CompletionProvider for MaxTokensSpy {
        async fn send_message(
            &self,
            _provider: &str,
            _base_url: Option<&str>,
            params: &CompletionParams,
        ) -> Result<CompletionResponse, CompletionError> {
            self.0.lock().unwrap().push(params.max_tokens);
            Ok(CompletionResponse {
                content: "restated".to_string(),
                usage: None,
                finish_reason: None,
                attachment_results: None,
                cache_usage: None,
            })
        }
    }

    fn selection() -> CheapLlmSelection {
        CheapLlmSelection {
            is_local: false,
            provider: "ANTHROPIC".to_string(),
            model_name: "cheap".to_string(),
            base_url: None,
            connection_profile_id: Some("p".to_string()),
            profile_parameters: None,
        }
    }

    /// **The CALLER's ceiling reaches the wire.** `announcer_tier3_equivalence`
    /// cannot see it: its canned key omits `max_tokens`, and the ceiling was
    /// MEASURED invisible there (2048 → 99, family still green, 2026-09-10).
    /// Since `execute_voice_rewrite` is now shared by BOTH rehearsals — one flat
    /// 2048 and one scaled to the draft — a dropped or swapped argument here
    /// would silently truncate every rewrite on the instance, so it is pinned at
    /// the one place both framings pass through.
    ///
    /// ⚠ The values below are deliberately NOT 2048 and NOT either in-scene
    /// bound. A first draft asserted `2048 → Some(2048)` and a mutation that
    /// hard-coded `Some(2048.0)` inside this function survived it: the test was
    /// asking whether the number it had just supplied came back. Each call site's
    /// own CHOICE of value is pinned separately (`character_voiced`'s
    /// `the_announcement_ceiling_is_flat_2048`, `in_scene_voiced`'s
    /// `max_tokens_for_seed` tier-1 rows); this pins only the forwarding, which
    /// is why an arbitrary number is the right comparand here.
    ///
    /// ⚠ They are also deliberately ABOVE 2048, because the executor applies
    /// v4's `effectiveMaxTokens` floor — `Math.max(maxTokens ?? 2048, 2048)`
    /// (`core-execution.ts:316`, mirrored at `cheap_llm_exec.rs:442`). A ceiling
    /// under 2048 never reaches the wire on EITHER side, which is why
    /// `MIN_MAX_TOKENS = 1024` in `in_scene_voiced` is inert at the provider
    /// even though it is v4-faithful at the service boundary the differential
    /// compares. Picking 1337 here made this test fail against correct code.
    #[tokio::test]
    async fn the_callers_ceiling_reaches_the_provider() {
        for ceiling in [3001.0_f64, 4095.0] {
            let spy = MaxTokensSpy::default();
            let executor = CheapLlmTaskExecutor::new();
            let sel = selection();
            let out = execute_voice_rewrite(
                &spy,
                &executor,
                ExecuteVoiceRewriteParams {
                    selection: &sel,
                    messages: vec![CompletionMessage {
                        role: CompletionRole::User,
                        content: "draft".to_string(),
                    }],
                    task_type: "announcement-rewrite",
                    character_id: "char",
                    max_tokens: ceiling,
                },
            )
            .await;
            assert!(out.success, "canned provider answers: {out:?}");
            assert_eq!(
                spy.0.lock().unwrap().as_slice(),
                &[Some(ceiling as i64)],
                "the ceiling the caller chose must be the ceiling the wire sees"
            );
        }
    }

    /// v4 `formatNameList` — the Oxford comma is v4's, not ours.
    #[test]
    fn name_list_is_oxford_comma_joined() {
        let n = |xs: &[&str]| -> Vec<String> { xs.iter().map(|s| s.to_string()).collect() };
        assert_eq!(format_name_list(&n(&["Alice"])), "Alice");
        assert_eq!(format_name_list(&n(&["Alice", "Bob"])), "Alice and Bob");
        assert_eq!(
            format_name_list(&n(&["Alice", "Bob", "Carol"])),
            "Alice, Bob, and Carol"
        );
        assert_eq!(
            format_name_list(&n(&["Alice", "Bob", "Carol", "Dan"])),
            "Alice, Bob, Carol, and Dan"
        );
    }
}
