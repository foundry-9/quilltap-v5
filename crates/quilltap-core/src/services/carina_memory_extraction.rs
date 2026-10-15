//! The Carina memory-extraction job handler — port of v4
//! `lib/background-jobs/handlers/carina-memory-extraction.ts` (`ac6348c5f`,
//! memory refactor Phase D; bug 222's project from `c2795733f`; pin
//! `83334b9a5`), on the ONE extraction pipeline.
//!
//! A Carina answer is posted as a `systemSender: 'carina'` message, which the
//! per-turn transcript builder deliberately skips — and the answerer is
//! frequently not even a participant in the chat. So the ordinary
//! `MEMORY_EXTRACTION` path can never see it. This handler is the dedicated
//! route: it builds the unit's context with the answerer added
//! (`extra_character_ids`), finds the posted message, and runs the SELF pass
//! over the one-slice transcript (`run_carina_extraction` — the question
//! opens it, the answer is the answerer's sole contribution) with the
//! `carina` provenance. The context carries the chat's project id and
//! description and the rolling summary to every unit — Carina's included
//! (bug 222; pre-refactor v5 passed none).

use serde_json::Value;

use crate::db::runtime::Db;
use crate::model::completion::CompletionProvider;
use crate::model::embedding::EmbeddingProvider;
use crate::services::cheap_llm_exec::CheapLlmTaskExecutor;
use crate::services::cost_estimation::MessageCostEstimator;
use crate::services::extraction::run::ExtractionDeps;
use crate::services::extraction::turn::run_carina_extraction;
use crate::services::memory_extraction_job::{
    extraction_context_for_job, finish_extraction_job, JobContextExtra,
};

/// The `CARINA_MEMORY_EXTRACTION` job payload (v4
/// `CarinaMemoryExtractionPayload`).
#[derive(Debug, Clone)]
pub struct CarinaMemoryExtractionPayload {
    pub chat_id: String,
    pub carina_message_id: String,
    pub answerer_id: String,
    pub connection_profile_id: String,
}

/// v4 `handleCarinaMemoryExtraction(job)`. A missing connection profile
/// throws (`Connection profile not found: <id>`, the shared context
/// builder's); a missing chat, answerer or message, and an empty answer, are
/// logged skips (`Ok(())`).
#[allow(clippy::too_many_arguments)]
pub async fn handle_carina_memory_extraction<C, E, K>(
    db: &Db,
    completion: &C,
    embedding: &E,
    executor: &CheapLlmTaskExecutor,
    cost: &K,
    job_id: &str,
    user_id: &str,
    payload: &CarinaMemoryExtractionPayload,
    host_zone_fallback: &str,
) -> Result<(), String>
where
    C: CompletionProvider,
    E: EmbeddingProvider + Sync,
    K: MessageCostEstimator,
{
    let Some(mut ctx) = extraction_context_for_job(
        db,
        job_id,
        user_id,
        &payload.chat_id,
        &payload.connection_profile_id,
        JobContextExtra {
            extra_character_ids: vec![payload.answerer_id.clone()],
            ..Default::default()
        },
        host_zone_fallback,
    )?
    else {
        return Ok(());
    };

    if ctx.cast_get(&payload.answerer_id).is_none() {
        tracing::warn!(
            jobId = %job_id,
            chatId = %payload.chat_id,
            answererId = %payload.answerer_id,
            "[CarinaMemoryExtraction] Answerer character not found; skipping"
        );
        return Ok(());
    }

    let carina_message: Option<Value> = ctx
        .messages()
        .map_err(|e| crate::db::fallback::error_text(&e))?
        .iter()
        .find(|m| m.get("id").and_then(Value::as_str) == Some(payload.carina_message_id.as_str()))
        .cloned();
    let Some(carina_message) = carina_message else {
        tracing::warn!(
            jobId = %job_id,
            chatId = %payload.chat_id,
            carinaMessageId = %payload.carina_message_id,
            "[CarinaMemoryExtraction] Carina message not found; skipping"
        );
        return Ok(());
    };

    let deps = ExtractionDeps {
        completion,
        embedding,
        executor,
    };
    let Some(result) =
        run_carina_extraction(&deps, &mut ctx, &carina_message, &payload.answerer_id).await
    else {
        tracing::info!(
            jobId = %job_id,
            chatId = %payload.chat_id,
            carinaMessageId = %payload.carina_message_id,
            "[CarinaMemoryExtraction] Carina message has no answer text; skipping"
        );
        return Ok(());
    };

    let project = ctx.project_id.as_deref().unwrap_or("null");
    tracing::info!(
        jobId = %job_id,
        chatId = %payload.chat_id,
        carinaMessageId = %payload.carina_message_id,
        answererId = %payload.answerer_id,
        projectId = %project,
        created = result.result.created_ids.len(),
        reinforced = result.result.reinforced_ids.len(),
        "[CarinaMemoryExtraction] Reference answer processed"
    );
    // The debug lines go on the carina message itself.
    finish_extraction_job(
        cost,
        job_id,
        user_id,
        &ctx,
        &result.result,
        "carina-memory-extraction",
        result.source_message_id.as_deref(),
    )
    .await
}
