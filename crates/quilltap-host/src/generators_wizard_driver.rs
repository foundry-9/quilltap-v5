//! The host's [`GeneratorsWizardDriver`] (P4.9K2 unit 6): the AI Wizard's two
//! runners and Summon From Lore over the SAME completion provider the spine's
//! send driver holds, the instance's disk storage backend (the wizard's image
//! and document sources, the import's source files — mount-blob keys read from
//! the mount DB before the backend is consulted), and the process version the
//! assembled export's manifest stamps (`appVersion` — v4 stamps its
//! `package.json` version; v5 stamps its own, recorded).
//!
//! No thread bridge: every await in the runners is a `Send` provider future or
//! a synchronous `Db` read. The engine keeps every parse arm in front of the
//! driver.
//!
//! ⚠ 💸 LIVE once assembled: one model call per requested wizard field (six
//! for the physical description, one more for a vision source); up to nine
//! per import run.

use std::sync::Arc;

use quilltap_core::api::generators_wizard::{
    AiImportDriverRequest, GeneratorsWizardDriver, GeneratorsWizardFuture, WizardDriverRequest,
};
use quilltap_core::db::runtime::Db;
use quilltap_core::generators::ai_import::run_ai_import_streaming;
use quilltap_core::generators::wizard::{
    run_character_wizard, run_character_wizard_streaming, OnProgress, WizardResult,
};
use quilltap_core::model::completion::CompletionProvider;
use quilltap_core::services::file_storage::StorageBackend;

/// The production driver: the assembly's `Db`, its shared completion
/// provider, the disk backend, and the process version.
pub struct HostGeneratorsWizardDriver<CMP> {
    pub db: Db,
    pub completion: Arc<CMP>,
    pub backend: Arc<dyn StorageBackend>,
    pub app_version: String,
}

impl<CMP> GeneratorsWizardDriver for HostGeneratorsWizardDriver<CMP>
where
    CMP: CompletionProvider + Send + Sync + 'static,
{
    fn wizard<'a>(
        &'a self,
        req: WizardDriverRequest,
    ) -> GeneratorsWizardFuture<'a, Result<WizardResult, String>> {
        Box::pin(async move {
            run_character_wizard(
                &self.db,
                &*self.completion,
                &*self.backend,
                &req.request,
                &req.user_id,
            )
            .await
        })
    }

    fn wizard_stream<'a>(
        &'a self,
        req: WizardDriverRequest,
        on_progress: OnProgress<'a>,
    ) -> GeneratorsWizardFuture<'a, ()> {
        Box::pin(async move {
            run_character_wizard_streaming(
                &self.db,
                &*self.completion,
                &*self.backend,
                &req.request,
                &req.user_id,
                on_progress,
            )
            .await
        })
    }

    fn ai_import_stream<'a>(
        &'a self,
        req: AiImportDriverRequest,
        on_progress: OnProgress<'a>,
    ) -> GeneratorsWizardFuture<'a, ()> {
        Box::pin(async move {
            run_ai_import_streaming(
                &self.db,
                &*self.completion,
                &*self.backend,
                &req.request,
                &req.user_id,
                on_progress,
                &self.app_version,
                req.now_ms,
            )
            .await
        })
    }
}
