//! The host's memory-consolidation runner (P4.D271 R-E).
//!
//! Core's [`run_consolidation`] is generic over the completion and embedding
//! providers, which only the host can build (`WireConfig`'s builders are
//! private to `spine`). [`HostConsolidationRunner`] builds them per run from
//! `WireConfig`'s pub fields — the `images_generate.rs` precedent — plus a
//! logging cheap-LLM executor, and [`memory_consolidation_job_handler`] both
//! INSTALLS it as core's process-global [`ConsolidationRunner`] (the API's
//! in-process dry run reaches the model through it) and returns the
//! `MEMORY_CONSOLIDATION` job handler over it.

use std::sync::Arc;

use quilltap_core::api::SINGLE_USER_ID;
use quilltap_core::db::runtime::Db;
use quilltap_core::services::cheap_llm_exec::{CheapLlmLogConfig, CheapLlmTaskExecutor};
use quilltap_core::services::consolidation::{
    install_consolidation_runner, run_consolidation, ConsolidationRunFuture, ConsolidationRunner,
    RunConsolidationOptions,
};
use quilltap_core::services::embedding_provider::ApiEmbeddingProvider;
use quilltap_core::services::job_runner::JobHandler;
use quilltap_core::services::llm_logging::LogContext;
use quilltap_core::services::memory_consolidation_job::MemoryConsolidationJobHandler;

use crate::spine::{DbProviderKeys, WireCompletionProvider, WireConfig};
use crate::wire::ReqwestWireTransport;

/// Runs [`run_consolidation`] over freshly-built wires (a `reqwest` client is
/// cheap; the providers close over the run's `Db`).
pub struct HostConsolidationRunner {
    pub wire: WireConfig,
}

impl ConsolidationRunner for HostConsolidationRunner {
    fn run<'a>(
        &'a self,
        db: &'a Db,
        character_id: &'a str,
        opts: RunConsolidationOptions,
    ) -> ConsolidationRunFuture<'a> {
        Box::pin(async move {
            let completion = WireCompletionProvider::new(
                DbProviderKeys(db.clone()),
                self.wire.policy,
                self.wire.user_agent.clone(),
                self.wire.base_url_env.clone(),
            )
            .with_localhost_gateway(self.wire.localhost_gateway.clone());
            let embedding = ApiEmbeddingProvider::new(db.clone(), ReqwestWireTransport::new())
                .with_localhost_gateway(self.wire.localhost_gateway.clone());
            let executor = CheapLlmTaskExecutor::with_logging(CheapLlmLogConfig {
                db: db.clone(),
                user_id: opts
                    .user_id
                    .clone()
                    .unwrap_or_else(|| SINGLE_USER_ID.to_string()),
                chat_id: None,
                message_id: None,
                ctx: LogContext::none(),
            });
            run_consolidation(db, &completion, &embedding, &executor, character_id, opts).await
        })
    }
}

/// Install the host's [`HostConsolidationRunner`] as core's process-global
/// runner AND return the `MEMORY_CONSOLIDATION` job handler over it — the ONE
/// `spine.rs` registration entry calls this.
pub fn memory_consolidation_job_handler(wire: WireConfig) -> Box<dyn JobHandler> {
    let runner: Arc<dyn ConsolidationRunner> = Arc::new(HostConsolidationRunner { wire });
    install_consolidation_runner(Arc::clone(&runner));
    Box::new(MemoryConsolidationJobHandler { runner })
}

#[cfg(test)]
mod tests {
    use super::*;
    use quilltap_core::db::background_jobs::BackgroundJob;
    use quilltap_core::services::consolidation::{
        clear_consolidation_runner, consolidation_runner,
    };
    use quilltap_core::services::job_runner::JobOutcome;

    const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";

    /// The ONE spine entry's constructor installs core's process-global runner
    /// (R-E — the API's in-process dry run reaches the model through it) and
    /// answers a `MEMORY_CONSOLIDATION` job through the core handler: a payload
    /// with no `characterId` completes without running (v4's WARN arm).
    #[tokio::test(flavor = "current_thread")]
    async fn the_registration_installs_the_runner_and_handles_a_job() {
        clear_consolidation_runner();
        let handler = memory_consolidation_job_handler(WireConfig {
            policy: quilltap_core::model::transport::TransportPolicy::default(),
            user_agent: "quilltap-test".to_string(),
            base_url_env: None,
            localhost_gateway: None,
        });
        assert!(consolidation_runner().is_some(), "the runner is installed");

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("main.db");
        {
            let w = quilltap_core::db::Writer::open_writable(&path, PEPPER).unwrap();
            w.connection()
                .execute_batch("CREATE TABLE stand_in (id TEXT);")
                .unwrap();
        }
        let db = Db::open_main(&path, PEPPER).unwrap();
        let job = BackgroundJob {
            id: "job-1".to_string(),
            user_id: SINGLE_USER_ID.to_string(),
            job_type: "MEMORY_CONSOLIDATION".to_string(),
            status: "PROCESSING".to_string(),
            payload: r#"{"trigger":"manual"}"#.to_string(),
            priority: 0.0,
            attempts: 1.0,
            max_attempts: 1.0,
            last_error: None,
            scheduled_at: String::new(),
            started_at: None,
            completed_at: None,
            created_at: String::new(),
            updated_at: String::new(),
        };
        assert!(matches!(
            handler.handle(&db, &job).await,
            JobOutcome::Completed(None)
        ));
        clear_consolidation_runner();
    }
}
