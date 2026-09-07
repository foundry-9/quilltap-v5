//! The host's [`GeneratorsDetailDriver`] (P4.9K1 unit 5): the two model-calling
//! per-character generator verbs — `characterGenerateExternalPrompt` and
//! `characterOptimize` — run over the SAME completion + embedding providers the
//! spine's send driver holds (one `WireConfig` per assembly; the provider Arcs
//! are shared, so a profile's per-model settings, gateway rewrites and key
//! resolution are the production ones).
//!
//! No thread bridge (unlike the help-chat / Brahma send drivers): both runners
//! are plain `async fn`s whose every await is a `Send` provider future or a
//! synchronous `Db` read, so the boxed future the trait wants is built
//! directly. The engine keeps v4's 404 + Zod arms in front of the driver; what
//! arrives here is a request v4 would have handed its service.
//!
//! ⚠ 💸 LIVE once assembled: one model call per external prompt; one analysis
//! call plus one per sub-step (the general fields, every scenario, every system
//! prompt, the physical description, the wardrobe, the aliases, new prompts)
//! per optimizer run, plus one embedding call when a semantic search query is
//! given.

use std::sync::Arc;

use quilltap_core::api::generators_detail::{
    ExternalPromptDriverRequest, GeneratorsDetailDriver, GeneratorsDetailFuture,
    OptimizeDriverRequest,
};
use quilltap_core::db::runtime::Db;
use quilltap_core::db::DbError;
use quilltap_core::generators::external_prompt::{generate_external_prompt, ExternalPromptResult};
use quilltap_core::generators::optimizer::{run_character_optimizer, OnProgress};
use quilltap_core::model::completion::CompletionProvider;
use quilltap_core::model::embedding::EmbeddingProvider;

/// The production driver: the assembly's `Db` and its shared providers.
pub struct HostGeneratorsDetailDriver<CMP, EMB> {
    pub db: Db,
    pub completion: Arc<CMP>,
    pub embedding: Arc<EMB>,
}

impl<CMP, EMB> GeneratorsDetailDriver for HostGeneratorsDetailDriver<CMP, EMB>
where
    CMP: CompletionProvider + Send + Sync + 'static,
    EMB: EmbeddingProvider + Send + Sync + 'static,
{
    fn external_prompt<'a>(
        &'a self,
        req: ExternalPromptDriverRequest,
    ) -> GeneratorsDetailFuture<'a, Result<ExternalPromptResult, DbError>> {
        Box::pin(async move {
            generate_external_prompt(
                &self.db,
                &*self.completion,
                &req.character_id,
                &req.request,
                &req.user_id,
            )
            .await
        })
    }

    fn optimize<'a>(
        &'a self,
        req: OptimizeDriverRequest,
        on_progress: OnProgress<'a>,
    ) -> GeneratorsDetailFuture<'a, ()> {
        Box::pin(async move {
            run_character_optimizer(
                &self.db,
                &*self.completion,
                &*self.embedding,
                &req.character_id,
                &req.connection_profile_id,
                &req.user_id,
                on_progress,
                &req.options,
                req.now_ms,
            )
            .await
        })
    }
}
