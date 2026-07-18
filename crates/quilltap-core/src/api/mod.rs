//! The Core API boundary (Phase 4, `api-boundary.md` Part 1) — **the one
//! contract every transport shares**.
//!
//! ```text
//! transports (axum / CLI / Tauri / uniffi)   — thin, marshalling only
//!                    │
//!             trait QuilltapCore             — dispatch + subscribe
//!                    │
//!               CoreEngine                    — the engine-backed impl
//!                    │
//!   the Phase-3 engine (services, repos, the single writer, the job runner)
//! ```
//!
//! The rule, stated once: **no business logic above this line.** A transport
//! may marshal, validate shape, and translate errors; the moment it wants to
//! *decide* something, that decision belongs behind [`QuilltapCore::dispatch`].
//! Streaming is always the [`Event`](types::Event) channel, never a return
//! value.
//!
//! Submodules: [`types`] (the pure contract — D8), [`provision`] (the pepper
//! resolution the readiness gate rides on), [`engine`] (the implementation).

// === P4.6ad: autonomous rooms ===
pub mod autonomous_rooms;
// === end P4.6ad ===
pub mod characters;
pub mod chat_create;
// === P4.6ab: courier + chat images ===
pub mod chat_media;
// === end P4.6ab ===
pub mod chat_send;
pub mod custom_tools;
pub mod documents;
pub mod engine;
pub mod groups;
pub mod image_profiles;
// === P4.6ar: the llm-logs read surface (lane A) ===
pub mod llm_logs;
pub mod memories;
pub mod mount_files;
pub mod mount_points;
pub mod projects;
pub mod provider_actions;
pub mod provision;
pub mod roleplay_templates;
pub mod salon;
pub mod scenarios;
pub mod settings;
// === P4.6z: system ===
pub mod system;
// === end P4.6z ===
// === P4.6ae: files family (lane A, append-only) ===
pub mod files;
// === end P4.6ae ===
// === P4.6ak: text-replacements (lane A, append-only) ===
pub mod text_replacements;
// === end P4.6ak ===
pub mod types;

pub use chat_create::{ChatCreateDriver, ChatCreateDriverRequest, ChatCreateFuture};
pub use chat_send::{ChatSendDriver, ChatSendFuture, ChatSendRequest};
pub use engine::{
    BootError, CoreConfig, CoreEngine, EngineAssembler, EngineAssembly, EngineShutdown,
    SINGLE_USER_ID,
};
pub use provider_actions::{ProviderActionsDriver, RealProviderActions};
pub use types::{
    AckDto, ChatCreateResultDto, ChatErrorPayload, ChatSendResultDto, ChatSummaryDto, ChatWrapDto,
    CoreError, ErrorKind, Event, EventPayload, HealthDto, InstanceDto, InstancesDto,
    PendingToolResult, PepperState, Request, Response, SetupResultDto, UnlockStateDto,
};

/// The boundary trait. Transports are generic over `C: QuilltapCore` (the
/// RPITIT future keeps the trait allocation-free; it is not dyn-compatible,
/// and does not need to be — each transport monomorphizes over the one
/// engine).
pub trait QuilltapCore: Send + Sync {
    /// Request/response. Never streams — streaming lives on [`Self::subscribe`].
    fn dispatch(
        &self,
        req: types::Request,
    ) -> impl std::future::Future<Output = types::Response> + Send;

    /// The server-push channel (D3: one global stream per client, every event
    /// scope-tagged). A lagging subscriber drops oldest events — transports
    /// treat a lag as a resync signal.
    fn subscribe(&self) -> tokio::sync::broadcast::Receiver<types::Event>;
}

/// The instance-registry seam (the launcher's `instances.json`): pure trait
/// here, fs implementation in `quilltap-host` (the Phase-3 injected-seam
/// pattern). Listing is pre-unlock surface — choosing an instance precedes
/// unlocking it.
pub trait InstanceDirectory: Send + Sync {
    fn list(&self) -> Result<types::InstancesDto, String>;
}
