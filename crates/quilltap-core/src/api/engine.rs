//! `CoreEngine` — the engine-backed [`QuilltapCore`](super::QuilltapCore)
//! implementation: the one place `Request`s become engine calls.
//!
//! The engine has two macro-states mirroring v4's locked mode:
//!
//! - **Locked** — the pepper is not operational (`needs-setup` /
//!   `needs-passphrase`). Only the always-available family answers
//!   (health, unlock state, unlock, instances); everything else gets the
//!   readiness refusal (D2).
//! - **Ready** — the pepper resolved; the instance's databases are open (one
//!   [`Db`] per the single-writer runtime) and the host's
//!   [`EngineAssembler`] has wired its drivers (job pump, cadence loops).
//!
//! The **assembler seam** is how the composition root stays in
//! `quilltap-host` while the state machine lives here: whenever the pepper
//! becomes operational (at boot, or later via `Unlock`), the engine opens the
//! `Db` and hands it to the assembler, which builds the runner/registry,
//! spawns its cadence tasks, and returns a shutdown handle. `Lock` calls that
//! handle, drops the `Db`, and returns to `needs-passphrase` — faithful to v4
//! `lockDbKey` (including the consequence that an env-pepper-booted,
//! vault-less instance cannot re-unlock; v4 behaves identically).

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use tokio::sync::broadcast;

use crate::db::chat_settings;
use crate::db::runtime::{Db, DbPaths};
use crate::dbkey::{self, DbKeyError};
use crate::services::creation_progress::CreationProgressBus;
use crate::services::provisioning;

use super::chat_create::{ChatCreateDriver, ChatCreateDriverRequest};
use super::chat_send::{ChatSendDriver, ChatSendRequest, SwipeGenerateDriver};
use super::provider_actions::ProviderActionsDriver;
use super::provision::{provision, PepperHashMismatch};
use super::types::{
    AckDto, ChangePassphraseResultDto, ErrorKind, Event, HealthDto, PepperState, Request, Response,
    SetupResultDto, UnlockStateDto,
};
use super::{InstanceDirectory, QuilltapCore};

/// v4 `SINGLE_USER_ID` (`lib/auth/single-user.ts`) — the synthetic single
/// user every row belongs to. D2: the session layer does not port.
pub const SINGLE_USER_ID: &str = "ffffffff-ffff-ffff-ffff-ffffffffffff";

/// v4 `handleSetup`'s one-shot save-the-pepper message (shown once, verbatim).
const SETUP_MESSAGE: &str =
    "Encryption key generated and stored. Save this value — it will not be displayed again.";

/// Broadcast capacity for the event channel. A slow subscriber past this
/// lags (drops oldest) rather than blocking the engine — the SSE transport
/// treats a lag as a resync signal.
const EVENT_CHANNEL_CAPACITY: usize = 1024;

// ============================================================================
// Config + the host seams
// ============================================================================

/// Engine construction inputs. `base_dir` is the instance root (v4
/// `getBaseDataDir()`); the databases and `.dbkey` live in `<base>/data`
/// (v4 `lib/paths.ts` layout).
#[derive(Debug, Clone)]
pub struct CoreConfig {
    pub base_dir: PathBuf,
    /// Reported by `Health` (the host supplies its crate version).
    pub version: String,
    /// The `ENCRYPTION_MASTER_PEPPER` env pepper, if set (host reads env).
    pub env_pepper: Option<String>,
}

impl CoreConfig {
    /// `<base>/data` — where the databases and `quilltap.dbkey` live.
    pub fn data_dir(&self) -> PathBuf {
        self.base_dir.join("data")
    }
}

/// One assembly's products (P4.2 — grown from the bare shutdown handle): the
/// teardown handle plus the optional chat-send driver. `chat_send: None` keeps
/// read-only embedders (and the P4.0 tests) valid — the `ChatSend` dispatch
/// arm answers "chat dispatch not assembled".
pub struct EngineAssembly {
    pub shutdown: Box<dyn EngineShutdown>,
    pub chat_send: Option<Arc<dyn ChatSendDriver>>,
    /// The chat-creation driver (P4.4u2b); `None` keeps read-only embedders (and
    /// the P4.0 tests) valid — the `ChatCreate` arm answers "not assembled".
    pub chat_create: Option<Arc<dyn ChatCreateDriver>>,
    /// The swipe-generate model driver (P4.6c, wired at unification); `None`
    /// keeps read-only embedders valid — the generate branch answers
    /// "not assembled".
    pub swipe_generate: Option<Arc<dyn SwipeGenerateDriver>>,
    /// The provider wire-actions driver (P4.6d, wired at unification) —
    /// connection test / test message / api-key test / models fetch.
    pub provider_actions: Option<Arc<dyn ProviderActionsDriver>>,
    /// The memory embedding provider the `MemoryCreate`/`MemorySearch` arms run
    /// LIVE over (P4.6s, wired at unification — the host threads the spine's
    /// `ApiEmbeddingProvider`); `None` keeps read-only embedders valid — the
    /// arms answer "not assembled".
    pub memory_embedding: Option<crate::model::embedding::ErasedEmbeddingProvider>,
    /// The document-store refresh scheduler (P4.6w, wired at unification to lane
    /// A's reindex/embed services); `None` = a loud unwired skip at the
    /// `document_store` write sites.
    pub mount_refresh: Option<Arc<dyn crate::documents::MountRefreshScheduler>>,
    /// The live-PTY probe `ChatGet`'s terminal reconcile runs over (the P4.2-era
    /// stub-probe deferral, closed post-P4.6u — the host threads its terminal
    /// manager); `None` (read-only embedders, no terminal subsystem) matches
    /// v4's empty `ptyManager` map.
    pub terminal_probe:
        Option<Arc<dyn crate::services::ariel_notifications::TerminalLivenessProbe>>,
    // === P4.6ab: courier + chat images ===
    /// The courier-resolve driver (P4.6ab, wired at unification): the host holds the
    /// completion provider + cheap executor the resolve settle's triggers ride.
    /// `None` → the `messageResolveExternalTurn` arm answers a loud refusal.
    pub courier_resolve: Option<Arc<dyn super::chat_media::CourierResolveDriver>>,
    /// The save-image bytes seam (P4.6ab): the host's `fileStorageManager`-backed
    /// [`FileBytesStore`]. `None` → the `messageSaveImage` arm falls back to
    /// [`NotConfiguredBytes`] (faithful "no host byte store" → `EMPTY_BYTES`).
    pub save_image_bytes: Option<Arc<dyn crate::photos::save_image_to_album::FileBytesStore>>,
    // === end P4.6ab ===
    // === P4.6ai: image generation ===
    /// The `imageProfileGenerate` runner (P4.6ai, wired LIVE in the host from the
    /// W4.7f `Real*Provider`s). `None` → the `ImageProfileGenerate` arm answers the
    /// loud not-assembled refusal (spine-less assemblies — read-only embedders — keep
    /// the un-refusal deferred).
    pub image_generation: Option<crate::tools::generate_image::ErasedImageGeneration>,
    /// The `imageProfileListModels` discovery seam (P4.D100, wired LIVE in the
    /// host from the W4.7f `RealImageProvider`). `None` → the arm answers the
    /// loud not-assembled refusal; a keyless list would silently masquerade as
    /// an honest one, which is exactly what `ca22ec45` set out to stop.
    pub image_discovery: Option<crate::model::image::ErasedImageDiscovery>,
    // === end P4.6ai ===
    // === P4.6bd: the custom-tool consult seam ===
    /// The custom-tool consult runner (P4.6bd, wired LIVE in the host — the
    /// spine's wire-config runner with the 60 s timeout decorator). Behind the
    /// `ChatCustomToolRun` / `CustomToolPreview` arms and the `run_custom` tool
    /// path. `None` (read-only embedders, canned assemblies) → the dispatch
    /// arms answer the loud not-assembled error; the in-turn tool path stays
    /// fail-soft (see `tools/executor.rs`).
    pub consult: Option<Arc<dyn crate::pascal::llm_consult::ConsultRunner>>,
    // === end P4.6bd ===
    // === P4.9f1: the avatar-preview render seam (lane F1, append-only) ===
    /// The `wardrobePreviewAvatar` render seam (one raw portrait render +
    /// WebP transcode — v4 `createImageProvider(...).generateImage` →
    /// `convertToWebP`). `None` → the arm runs its guard tiers live and the
    /// RENDER step answers the loud not-assembled refusal. **The host wire is
    /// DEFERRED to unification** (lane P4.6bd owns `spine.rs`/`host.rs` this
    /// round — the P4.9f1 lane record names the recipe).
    pub avatar_preview: Option<super::wardrobe::ErasedAvatarPreview>,
    // === end P4.9f1 ===
    // === P4.9I1A: the Brahma Console orchestrator send driver ===
    /// The Brahma Console send driver (the orchestrator — `chat_send`'s sibling;
    /// only the composing host can construct the streaming/tool/cost bundle).
    /// `None` (read-only embedders) → the `BrahmaConsoleSend` arm answers a plain
    /// internal error.
    pub brahma_console_send: Option<Arc<dyn super::brahma::BrahmaConsoleSendDriver>>,
    // === end P4.9I1A ===
    // === P4.6bf (S1): the dispatch-layer blob-WebP transcode seam ===
    /// The Scriptorium blob-upload WebP transcoder the two `store_mount_file`
    /// dispatch handlers use (`api/mount_files.rs`). `None` → the handlers keep
    /// their inline [`RefusingWebpTranscoder`] (v4's store-original fallback
    /// arm — behavior unchanged); the production host passes the live
    /// [`HostImageCodec`]. **Lane BF adds the field + plumbing only; the
    /// engine.rs call sites that thread this into the handlers are the
    /// AT-UNIFY wire** (lane BG re-signatures the handlers to accept it).
    pub blob_webp: Option<Arc<dyn crate::services::mount_index::blob_transcode::WebpTranscoder>>,
    // === end P4.6bf ===
    // === P4.d13: the recall-replay runner (episodic recall §3) ===
    /// The `chatRecallReplay` runner — the host holds the completion provider +
    /// cheap executor + embedding provider the replay's distill/search ride
    /// (the courier-resolve pattern). `None` (read-only embedders) → the arm
    /// answers a loud not-assembled error.
    pub recall_replay: Option<Arc<dyn super::recall_replay::RecallReplayDriver>>,
    // === end P4.d13 ===
    // === P4.9G1: the host job-pump control seam ===
    /// The background-job pump control (start/stop/status + wake — the host owns
    /// ALL cadence, P4.0). Behind the `SystemTasksQueue*` /
    /// `SystemJobConcurrencySet` / `SystemJobControl` arms. `None` (read-only
    /// embedders, canned assemblies) → those arms answer the loud not-assembled
    /// refusal.
    pub job_pump: Option<Arc<dyn super::system_data::JobPumpControl>>,
    // === end P4.9G1 ===
    // === P4.9G5: the backup host seam ===
    /// Everything the backup family needs from the host process: the disk
    /// storage backend, the scratch/plugins/themes directories, the app
    /// version, an injected clock, and the process-lifetime single-use
    /// temp store the download leg reads (P4.0 — process-lifetime state is
    /// host state). `None` (read-only embedders, canned assemblies) → the
    /// `SystemBackupCreate` arm answers the loud not-assembled refusal.
    pub backup_host: Option<Arc<dyn crate::services::backup::BackupHost>>,
    // === end P4.9G5 ===
    // === P4.37: the Almanack host seam ===
    /// The host facts the report needs (paths, runtime, version, clock, the
    /// disk storage backend). `None` (read-only embedders, canned assemblies)
    /// → the four `SystemAlmanack*` arms answer the loud not-assembled refusal.
    /// Wired LIVE by `quilltap-host` (`HostAlmanackServices`, P4.37 resume
    /// item 3) — the production spine always supplies it.
    pub almanack_host: Option<Arc<dyn super::almanack::AlmanackHost>>,
    // === end P4.37 ===
    // === P4.9E2A: the in-chat announcement-preview seam ===
    /// The in-character announcement rewriter — the host holds the completion
    /// provider + cheap executor + embedding provider the rewrite's Commonplace
    /// recall and cheap-LLM call ride (the `recall_replay` precedent). `None`
    /// (read-only embedders, canned assemblies) → the `ChatAnnouncementPreview`
    /// arm answers the loud not-assembled refusal AFTER running v4's validation
    /// / character / profile arms, so the dialog can render the reason.
    /// **The host wire is DEFERRED to unification** (the P4.9f1 `avatar_preview`
    /// precedent — this lane owns neither `quilltap-host` nor its version bump;
    /// the recipe is in the lane record).
    pub announcement_preview: Option<Arc<dyn super::chat_post_office::AnnouncementPreviewDriver>>,
    // === end P4.9E2A ===
    // === P4.9E3A: the operator run-tool seam ===
    /// The tool runner the `run-tool` action executes through — the host's
    /// `BuiltInToolRunner`, which carries the `SelfInventoryEnv`, the terminal
    /// scrollback source and the consult runner. `None` (read-only embedders,
    /// canned assemblies) → the `ChatRunTool` arm answers the loud not-assembled
    /// refusal AFTER running v4's deny-list and chat arms, so the Run Tool modal
    /// can render the reason. **The host wire is DEFERRED to unification** (the
    /// P4.9f1 `avatar_preview` / P4.9E2A `announcement_preview` precedent — this
    /// lane owns neither `quilltap-host` nor its version bump; the recipe is in
    /// the lane record).
    pub operator_tool_runner: Option<Arc<dyn crate::services::chat_run_tool::OperatorToolRunner>>,
    /// The manual title-regeneration driver — the host holds the completion
    /// provider + the logging cheap executor the call rides (the
    /// `announcement_preview` precedent). `None` → the `ChatRegenerateTitle` arm
    /// answers the loud not-assembled refusal.
    /// ⚠ LIVE means real money: one cheap-LLM call per regeneration.
    pub regenerate_title: Option<Arc<dyn crate::services::chat_admin::RegenerateTitleDriver>>,
    // === end P4.9E3A ===
    // === P4.9E3B / P4.42 ===
    /// The web-search boundary (P4.42). This ONE `Option` is the single source of
    /// truth for web search: the host threads it into the spine's tool runner
    /// (`search_web` executes through it) AND the tools inventory derives its
    /// `web_search_configured` bool from `is_some()` — so what is advertised and
    /// what executes can never disagree (the dogfood "advertised-vs-refusing"
    /// finding). `None` for canned test factories + read-only embedders → the
    /// `search_web` row reads "No search provider configured…" (v4's own
    /// unconfigured arm) and the runner refuses.
    ///
    /// v4's `isWebSearchConfigured()` is `searchProviderRegistry.isSearchConfigured()
    /// || SERPER_API_KEY`. Since P4.59 v5 has both halves: the host registers the
    /// native Serper search manifest (subject to the site-plugins gate) and
    /// builds this whenever EITHER the provider is registered OR `SERPER_API_KEY`
    /// is set — so `is_some()` is v4's `||`, term for term.
    pub web_search: Option<Arc<dyn crate::tools::web_search::WebSearchProvider>>,
    /// The SEARCH-provider manifests this boot registered (P4.59) — v4's
    /// `searchProviderRegistry.getAllProviders()`, which is what
    /// `GET /api/v1/providers` lists. The host computes registration ONCE and
    /// threads the same answer into [`Self::web_search`] and into this list, so
    /// the listing cannot advertise a provider the runner does not have. Empty
    /// for canned test factories + read-only embedders, and empty on a boot whose
    /// `SITE_PLUGINS_*` disable the bundled plugin.
    pub search_providers: Vec<&'static crate::provider_manifest::search::SearchManifest>,
    /// The out-of-create `llm_choose` outfit runner (P4.9E3B) — the host holds
    /// the completion provider + a per-call logging cheap executor (the
    /// `RegenerateTitleDriver` arrangement). `None` → both call sites fall
    /// back to the DEFAULT outfit with a named warning (v4's own any-failure
    /// shape — never a refusal).
    /// ⚠ LIVE means real money: one cheap-LLM call per llm_choose pick.
    pub outfit_llm_choose:
        Option<Arc<dyn crate::services::outfit_selections::OutfitLlmChooseRunner>>,
    // === end P4.9E3B ===
    // === P4.9E4A: the vision-describe seam ===
    /// The vision describe runner the `attach-mount-file` ladder rides — the
    /// host holds the completion provider + the image transcoder (the
    /// `RegenerateTitleDriver` / `announcement_preview` arrangement). `None`
    /// (read-only embedders, canned assemblies) → the describe resolves to `''`
    /// with a warn and the attach STILL SUCCEEDS, which is v4's own posture for
    /// every describe failure on this path — never a refusal.
    /// ⚠ LIVE means real money: one vision-LLM call per attach of an
    /// undescribed image (cached / kept-image / non-image arms never reach it).
    pub image_describe: Option<Arc<dyn super::chat_media::ImageDescribeDriver>>,
    // === end P4.9E4A ===
}

impl EngineAssembly {
    /// A driver-less assembly around a shutdown handle.
    pub fn shutdown_only(shutdown: Box<dyn EngineShutdown>) -> Self {
        EngineAssembly {
            shutdown,
            chat_send: None,
            brahma_console_send: None,
            chat_create: None,
            swipe_generate: None,
            provider_actions: None,
            memory_embedding: None,
            mount_refresh: None,
            terminal_probe: None,
            courier_resolve: None,
            save_image_bytes: None,
            // === P4.6ai ===
            image_generation: None,
            image_discovery: None,
            // === end P4.6ai ===
            consult: None,
            // === P4.9f1 ===
            avatar_preview: None,
            // === end P4.9f1 ===
            // === P4.6bf (S1) ===
            blob_webp: None,
            // === end P4.6bf ===
            // === P4.d13 ===
            recall_replay: None,
            // === end P4.d13 ===
            // === P4.9G1 ===
            job_pump: None,
            backup_host: None,
            almanack_host: None,
            // === end P4.9G1 ===
            // === P4.9E2A ===
            announcement_preview: None,
            // === end P4.9E2A ===
            // === P4.9E3A ===
            operator_tool_runner: None,
            regenerate_title: None,
            // === end P4.9E3A ===
            // === P4.9E3B / P4.42 ===
            web_search: None,
            search_providers: Vec::new(),
            outfit_llm_choose: None,
            // === end P4.9E3B ===
            // === P4.9E4A ===
            image_describe: None,
            // === end P4.9E4A ===
        }
    }
}

/// The composition-root seam: called whenever the pepper becomes operational
/// (boot or unlock) with the freshly opened [`Db`] and the engine's event
/// broadcast (the spine's frames ride it). The host clones the `Db` into its
/// drivers (job runner, cadence loops), spawns them, and returns the assembly
/// that tears them down again on `Lock`. Must be reusable — a lock/unlock
/// cycle assembles more than once.
///
/// `pepper` + `data_dir` are threaded through because the chat-create driver
/// opens its OWN writable [`Writer`](crate::db::Writer)s per create (the outfit
/// sub-unit holds writable connections across an LLM await, which the sync
/// single-writer closure cannot host); `bus` is the shared
/// [`CreationProgressBus`] the driver's emitter publishes to and the transport
/// replays from.
pub trait EngineAssembler: Send + Sync {
    /// Claim the instance BEFORE any partition file is opened (P4.46).
    ///
    /// v4 acquires its single-instance lock inside `connect()`, *ahead* of
    /// `new Database(...)` — so a contended start never touches the files. v5's
    /// lock lives host-side (the engine is transport-agnostic and knows nothing
    /// about lock files), so the engine calls this hook at every entrance that
    /// is about to open or create a partition: boot, unlock, and first-run
    /// setup. Without it, a contended start performed three writable opens —
    /// each issuing `PRAGMA journal_mode = TRUNCATE`, a header write — against
    /// databases another process believes it holds exclusively, and only then
    /// refused at assembly. That is the class v4's bug-58 fix closes.
    ///
    /// Implementations MUST be idempotent for the calling process: `Setup`
    /// calls this once before provisioning and then reaches `open_ready`, which
    /// calls it again. The host's lock is re-entrant per PID, so the second
    /// call refreshes the claim rather than deadlocking on it.
    ///
    /// A refusal is surfaced as [`BootError::Assemble`] — the same class a
    /// lock conflict raised when acquisition lived inside [`Self::assemble`].
    /// The default is a no-op (test and read-only embedders claim nothing).
    fn pre_open(&self, data_dir: &std::path::Path) -> Result<(), String> {
        let _ = data_dir;
        Ok(())
    }

    fn assemble(
        &self,
        db: &Db,
        events: &broadcast::Sender<Event>,
        pepper: &str,
        data_dir: &std::path::Path,
        bus: &Arc<CreationProgressBus>,
    ) -> Result<EngineAssembly, String>;
}

/// Tears down one assembly (stop loops, drop `Db` clones). Idempotent.
pub trait EngineShutdown: Send + Sync {
    fn shutdown(&self);
}

/// A no-driver assembler for tests and read-only embedders.
pub struct NoopAssembler;

struct NoopShutdown;
impl EngineShutdown for NoopShutdown {
    fn shutdown(&self) {}
}

impl EngineAssembler for NoopAssembler {
    fn assemble(
        &self,
        _db: &Db,
        _events: &broadcast::Sender<Event>,
        _pepper: &str,
        _data_dir: &std::path::Path,
        _bus: &Arc<CreationProgressBus>,
    ) -> Result<EngineAssembly, String> {
        Ok(EngineAssembly::shutdown_only(Box::new(NoopShutdown)))
    }
}

// ============================================================================
// The engine
// ============================================================================

/// Boot failures — hard errors before the engine exists (a locked vault is
/// NOT a failure; the engine boots into the locked state and serves the
/// unlock family).
#[derive(Debug)]
pub enum BootError {
    /// The env pepper contradicts the `.dbkey` hash (v4's FATAL exit).
    PepperMismatch(PepperHashMismatch),
    /// The pepper resolved but `<data>/quilltap.db` does not exist. Opening
    /// would CREATE an empty cipher-keyed file — schema creation (migrations)
    /// is unported P4.4 surface, so a missing main DB is refused instead.
    MissingMainDb(PathBuf),
    Db(crate::db::DbError),
    Assemble(String),
}

impl std::fmt::Display for BootError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BootError::PepperMismatch(e) => write!(f, "{e}"),
            BootError::MissingMainDb(p) => {
                write!(
                    f,
                    "no main database at {} (fresh-instance creation is not yet supported)",
                    p.display()
                )
            }
            BootError::Db(e) => write!(f, "database open failed: {e}"),
            BootError::Assemble(e) => write!(f, "engine assembly failed: {e}"),
        }
    }
}
impl std::error::Error for BootError {}

// One instance per engine, held behind the state mutex. `Ready` is the one
// long-lived value (the engine spends its life there); `Locked` is a two-field
// transient, so the size skew is harmless. Boxing `ReadyEngine` would touch
// every construction site and add a pointer chase to every dispatch for no
// runtime win — flagged twice (the P4.6bd and P4.9f1 seam additions), left
// un-boxed deliberately both times.
#[allow(clippy::large_enum_variant)]
enum EngineState {
    Locked {
        pepper_state: PepperState,
        has_user_passphrase: bool,
    },
    Ready(ReadyEngine),
}

struct ReadyEngine {
    db: Db,
    /// The Almanack's host facts (P4.37); `None` → the not-assembled refusal.
    almanack_host: Option<Arc<dyn super::almanack::AlmanackHost>>,
    pepper_state: PepperState,
    has_user_passphrase: bool,
    shutdown: Box<dyn EngineShutdown>,
    /// The assembly's chat-send driver (P4.2); `None` for read-only embedders.
    chat_send: Option<Arc<dyn ChatSendDriver>>,
    /// The assembly's chat-create driver (P4.4u2b); `None` for read-only embedders.
    chat_create: Option<Arc<dyn ChatCreateDriver>>,
    /// The assembly's swipe-generate driver (P4.6); `None` for read-only embedders.
    swipe_generate: Option<Arc<dyn SwipeGenerateDriver>>,
    /// The assembly's provider wire-actions driver (P4.6).
    provider_actions: Option<Arc<dyn ProviderActionsDriver>>,
    /// The memory embedding provider the create/search arms run LIVE (P4.6s;
    /// threaded from `EngineAssembly::memory_embedding` at the P4.6stu
    /// unification — the host supplies the spine's `ApiEmbeddingProvider`).
    memory_embedding: Option<crate::model::embedding::ErasedEmbeddingProvider>,
    /// The document-store refresh scheduler (P4.6w; `None` until unification wires
    /// lane A's reindex/embed services).
    mount_refresh: Option<Arc<dyn crate::documents::MountRefreshScheduler>>,
    /// The live-PTY probe `ChatGet`'s terminal reconcile runs over (threaded
    /// from `EngineAssembly::terminal_probe`).
    terminal_probe: Option<Arc<dyn crate::services::ariel_notifications::TerminalLivenessProbe>>,
    /// The courier-resolve driver + save-image bytes seam (P4.6ab; `None` until the
    /// unification wire).
    courier_resolve: Option<Arc<dyn super::chat_media::CourierResolveDriver>>,
    save_image_bytes: Option<Arc<dyn crate::photos::save_image_to_album::FileBytesStore>>,
    /// The `imageProfileGenerate` runner (P4.6ai; `None` for spine-less assemblies —
    /// the arm answers the loud not-assembled refusal).
    image_generation: Option<crate::tools::generate_image::ErasedImageGeneration>,
    image_discovery: Option<crate::model::image::ErasedImageDiscovery>,
    /// The custom-tool consult runner (P4.6bd; `None` for spine-less assemblies —
    /// the composer/bench arms answer the loud not-assembled error).
    consult: Option<Arc<dyn crate::pascal::llm_consult::ConsultRunner>>,
    /// The avatar-preview render seam (P4.9f1; `None` until the deferred host
    /// wire — the render step then answers the loud refusal).
    avatar_preview: Option<super::wardrobe::ErasedAvatarPreview>,
    /// The Brahma Console send driver (P4.9I1A; `None` for read-only embedders —
    /// the `BrahmaConsoleSend` arm answers a plain internal error).
    brahma_console_send: Option<Arc<dyn super::brahma::BrahmaConsoleSendDriver>>,
    /// The dispatch-layer blob-WebP transcoder (P4.6bf S1). Threaded from the
    /// assembly and passed into lane BG's re-signatured `store_mount_file`
    /// handlers via [`Self::ready_db_and_blob_webp`] (P4.6bg unit 6 wire).
    blob_webp: Option<Arc<dyn crate::services::mount_index::blob_transcode::WebpTranscoder>>,
    /// The recall-replay runner (P4.d13; `None` for spine-less assemblies —
    /// the `ChatRecallReplay` arm answers the loud not-assembled error).
    recall_replay: Option<Arc<dyn super::recall_replay::RecallReplayDriver>>,
    /// The background-job pump control (P4.9G1; `None` for read-only embedders —
    /// the tasks-queue/control/concurrency-set/job-control arms answer the loud
    /// not-assembled refusal).
    job_pump: Option<Arc<dyn super::system_data::JobPumpControl>>,
    /// The backup host services (P4.9G5; `None` for read-only embedders — the
    /// `SystemBackupCreate` arm answers the loud not-assembled refusal).
    backup_host: Option<Arc<dyn crate::services::backup::BackupHost>>,
    // === P4.9E2A ===
    /// The in-character announcement rewriter (P4.9E2A; `None` until the
    /// deferred host wire — the `ChatAnnouncementPreview` arm then answers the
    /// loud not-assembled refusal after v4's own validation arms).
    announcement_preview: Option<Arc<dyn super::chat_post_office::AnnouncementPreviewDriver>>,
    operator_tool_runner: Option<Arc<dyn crate::services::chat_run_tool::OperatorToolRunner>>,
    regenerate_title: Option<Arc<dyn crate::services::chat_admin::RegenerateTitleDriver>>,
    // === end P4.9E2A ===
    // === P4.9E3B / P4.42 ===
    /// The web-search boundary (P4.42). The tools inventory derives
    /// `web_search_configured` from `is_some()` (see [`Self::web_search_configured`]),
    /// so the advertised availability is LITERALLY the presence of the provider the
    /// runner would use.
    web_search: Option<Arc<dyn crate::tools::web_search::WebSearchProvider>>,
    /// The registered SEARCH-provider manifests (P4.59) — the providers listing's
    /// search rows, from the same registration answer `web_search` came from.
    search_providers: Vec<&'static crate::provider_manifest::search::SearchManifest>,
    outfit_llm_choose: Option<Arc<dyn crate::services::outfit_selections::OutfitLlmChooseRunner>>,
    // === end P4.9E3B ===
    // === P4.9E4A ===
    /// The vision describe runner (P4.9E4A; `None` → the attach's describe
    /// ladder resolves to `''` and the attach still succeeds).
    image_describe: Option<Arc<dyn super::chat_media::ImageDescribeDriver>>,
    // === end P4.9E4A ===
}

/// The engine-backed `QuilltapCore`. Cloneable (`Arc` inside) so every
/// transport and driver can hold one handle.
#[derive(Clone)]
pub struct CoreEngine {
    inner: Arc<EngineInner>,
}

struct EngineInner {
    config: CoreConfig,
    assembler: Box<dyn EngineAssembler>,
    instances: Arc<dyn InstanceDirectory>,
    /// std Mutex — never held across an await (all state work is sync).
    state: Mutex<EngineState>,
    events: broadcast::Sender<Event>,
    /// The Green-Room replay buffer (D6) — one per engine, shared by the
    /// chat-create driver's emitter (publish) and the transport (replay).
    creation_progress_bus: Arc<CreationProgressBus>,
    /// The runtime passphrase cache — v4 `lib/startup/passphrase-cache.ts`
    /// (new in `d553f72a`), whose `global.__quilltapRuntimePassphrase` exists
    /// to survive Next.js HMR. **v5's process-boundary analog is the engine**,
    /// so it lives here rather than in a process global.
    ///
    /// Why it exists at all: `.dbkey` only ever SEES the passphrase at the
    /// moments it passes through (setup / unlock / change / store) — it
    /// derives the pepper and discards it. Archive encryption needs the
    /// passphrase LATER, at archive time, because a bundle must be decryptable
    /// from the passphrase alone: bundles outlive the instance, so the key
    /// material has to be knowledge the operator carries.
    ///
    /// Exposure calculus (v4's, unchanged): the pepper — the actual database
    /// key — is already in memory for the life of the unlocked process, so
    /// within this process the cached passphrase adds nothing an attacker with
    /// memory access didn't already have. The marginal risk is passphrase
    /// REUSE across services, which is why [`CoreEngine::lock`] clears it and
    /// nothing ever writes it to disk or logs.
    runtime_passphrase: Mutex<Option<String>>,
}

impl CoreEngine {
    /// Remember the effective passphrase (the internal sentinel counts too) —
    /// v4 `cacheRuntimePassphrase`. Called from the four `.dbkey` chokepoints
    /// that see it: setup, unlock, change-passphrase, store-pepper.
    fn cache_runtime_passphrase(&self, passphrase: &str) {
        // v4's `hasUserPassphrase ? passphrase : INTERNAL_PASSPHRASE` resolution
        // happens at each deposit site; mirror it here so every caller agrees.
        let effective = if passphrase.is_empty() {
            crate::dbkey::INTERNAL_PASSPHRASE.to_string()
        } else {
            passphrase.to_string()
        };
        *self.inner.runtime_passphrase.lock().unwrap() = Some(effective);
    }

    /// The passphrase archive crypto should use when no explicit one is given —
    /// v4 `resolveArchivePassphrase`: the cache, else the internal sentinel on
    /// a no-passphrase instance, else the loud
    /// [`ArchiveCryptoError::KeyUnavailable`](crate::services::character_archive::crypto::ArchiveCryptoError::KeyUnavailable)
    /// refusal.
    /// (Public because it is the engine capability round 2's `characterArchive`
    /// verb and the passphrase-change re-encrypt sweep both reach for; the
    /// state-machine wiring it depends on is proven by
    /// `runtime_passphrase_cache_follows_the_dbkey_lifecycle` below.)
    pub fn resolve_archive_passphrase(
        &self,
    ) -> Result<String, crate::services::character_archive::crypto::ArchiveCryptoError> {
        let (cached, has_user_passphrase) = self.passphrase_source_parts();
        crate::services::character_archive::crypto::resolve_archive_passphrase(
            crate::services::character_archive::crypto::PassphraseSource {
                cached: cached.as_deref(),
                has_user_passphrase,
            },
        )
    }

    /// The two inputs [`crate::services::character_archive::crypto::PassphraseSource`]
    /// carries, read under their own locks. The archive SERVICE (P4.D65) takes
    /// the source rather than the resolved passphrase, because v4 resolves
    /// *inside* `archiveCharacter` — after the already-archived early return,
    /// so re-running a prune never needs a passphrase at all.
    fn passphrase_source_parts(&self) -> (Option<String>, bool) {
        let cached = self.inner.runtime_passphrase.lock().unwrap().clone();
        let has_user_passphrase = match &*self.inner.state.lock().unwrap() {
            EngineState::Ready(r) => r.has_user_passphrase,
            EngineState::Locked {
                has_user_passphrase,
                ..
            } => *has_user_passphrase,
        };
        (cached, has_user_passphrase)
    }

    /// The host capabilities the archive service reaches for, assembled from
    /// the same accessors the `.qtap` export/import arms use.
    fn archive_seams<'a>(
        &self,
        cached: Option<&'a str>,
        has_user_passphrase: bool,
    ) -> crate::services::character_archive::service::ArchiveSeams<'a> {
        crate::services::character_archive::service::ArchiveSeams {
            backend: self.qtap_file_storage(),
            passphrase: crate::services::character_archive::crypto::PassphraseSource {
                cached,
                has_user_passphrase,
            },
            app_version: self.app_version(),
            codec: self.qtap_pixel_codec(),
            extractor: crate::services::mount_index::converters::default_text_extractor(),
        }
    }

    /// Provision the pepper and boot. An operational pepper opens the
    /// databases and assembles the drivers; a locked vault boots into the
    /// locked state (serving the unlock family). Hard failures — pepper/hash
    /// mismatch, missing main DB, open/assembly errors — refuse the boot.
    pub fn boot(
        config: CoreConfig,
        assembler: Box<dyn EngineAssembler>,
        instances: Arc<dyn InstanceDirectory>,
    ) -> Result<CoreEngine, BootError> {
        let provisioned = provision(&config.data_dir(), config.env_pepper.as_deref())
            .map_err(BootError::PepperMismatch)?;

        let (events, _) = broadcast::channel(EVENT_CHANNEL_CAPACITY);
        let inner = EngineInner {
            config,
            assembler,
            instances,
            state: Mutex::new(EngineState::Locked {
                pepper_state: provisioned.state,
                has_user_passphrase: provisioned.has_user_passphrase,
            }),
            events,
            creation_progress_bus: Arc::new(CreationProgressBus::new()),
            runtime_passphrase: Mutex::new(None),
        };

        if provisioned.state.is_operational() {
            let ready = open_ready(
                &inner,
                provisioned
                    .pepper
                    .as_deref()
                    .expect("operational state carries the pepper"),
                provisioned.state,
                provisioned.has_user_passphrase,
            )?;
            *inner.state.lock().unwrap() = EngineState::Ready(ready);
        }

        Ok(CoreEngine {
            inner: Arc::new(inner),
        })
    }

    /// The event publisher — later units (chat send, creation progress) emit
    /// through this; transports subscribe via [`QuilltapCore::subscribe`].
    pub fn event_sender(&self) -> &broadcast::Sender<Event> {
        &self.inner.events
    }

    /// The Green-Room replay buffer (D6). The transport drains
    /// [`CreationProgressBus::active_snapshot`] onto each new `/api/events`
    /// stream so a late-connecting creation dialog replays the buffered
    /// backlog; the chat-create driver's emitter publishes into the same bus.
    pub fn creation_progress_bus(&self) -> Arc<CreationProgressBus> {
        Arc::clone(&self.inner.creation_progress_bus)
    }

    /// The open `Db`, when ready (a clone — the runtime handle is `Clone`).
    /// The host-supplied app version (v4 `packageJson.version`). Read by the
    /// `.qtap` export's manifest at the web edge (P4.9G4).
    pub fn app_version(&self) -> String {
        self.inner.config.version.clone()
    }

    pub fn db(&self) -> Option<Db> {
        match &*self.inner.state.lock().unwrap() {
            EngineState::Ready(r) => Some(r.db.clone()),
            EngineState::Locked { .. } => None,
        }
    }

    // ── P4.9G3 ──
    /// The host-wired job-pump control, when ready and assembled. The
    /// `/api/v1/system/jobs` COLLECTION edge (v4 `system/jobs/route.ts`) is
    /// web-edge-only — the §1 wire surface has no verb for it — so the edge needs
    /// the same `ProcessorStatus` / `ensureProcessorRunning` the dispatch arms
    /// reach through [`ready_job_pump`](Self::ready_job_pump). `None` on a
    /// read-only embedder (no cadence) → the edge answers the loud
    /// not-assembled refusal, exactly like the tasks-queue arms.
    pub fn job_pump_control(&self) -> Option<Arc<dyn super::system_data::JobPumpControl>> {
        match &*self.inner.state.lock().unwrap() {
            EngineState::Ready(r) => r.job_pump.clone(),
            EngineState::Locked { .. } => None,
        }
    }
    // ── end P4.9G3 ──

    // ── P4.D46 (`7189a968`) ──
    /// The backup host's disk backend, when ready and assembled. The `.qtap`
    /// `files` export (a web-edge-only download leg) reads legacy disk-style
    /// storage keys through it; `mount-blob:` keys read the mount partition
    /// directly. `None` (unassembled) degrades disk-key files to v4's own
    /// warn-and-`_bytesMissing` arm.
    pub fn qtap_file_storage(
        &self,
    ) -> Option<Arc<dyn crate::services::file_storage::StorageBackend>> {
        match &*self.inner.state.lock().unwrap() {
            EngineState::Ready(r) => r.backup_host.as_ref().map(|h| h.storage()),
            EngineState::Locked { .. } => None,
        }
    }

    /// The backup host's image codec, when ready and assembled — the `.qtap`
    /// file importer's storage bridges transcode through it, exactly as the
    /// backup restore's file phase does. `None` falls through to the
    /// not-configured codec (bytes pass through untranscoded, v4's
    /// sharp-failed arm).
    pub fn qtap_pixel_codec(&self) -> Option<Arc<dyn crate::services::file_storage::PixelCodec>> {
        match &*self.inner.state.lock().unwrap() {
            EngineState::Ready(r) => r.backup_host.as_ref().map(|h| h.pixel_codec()),
            EngineState::Locked { .. } => None,
        }
    }
    // ── end P4.D46 ──

    async fn dispatch_impl(&self, req: Request) -> Response {
        match req {
            Request::Health => self.health(),
            Request::UnlockState => self.unlock_state(),
            Request::Unlock { passphrase } => self.unlock(&passphrase),
            Request::Setup { passphrase } => self.setup(&passphrase),
            Request::StorePepper { passphrase } => self.store_pepper(&passphrase),
            Request::ChangePassphrase {
                old_passphrase,
                new_passphrase,
            } => {
                self.change_passphrase_with_archive_sweep(&old_passphrase, &new_passphrase)
                    .await
            }
            Request::Lock => self.lock(),
            Request::ListInstances => match self.inner.instances.list() {
                Ok(dto) => Response::Instances(dto),
                Err(e) => Response::error(ErrorKind::Internal, e),
            },
            Request::ListChats {
                exclude_tag_ids,
                limit,
                include_autonomous,
            } => match self.ready_db() {
                Ok(db) => super::salon::list_chats(
                    &db,
                    SINGLE_USER_ID,
                    &exclude_tag_ids,
                    limit,
                    include_autonomous,
                ),
                Err(r) => r,
            },
            Request::ChatGet { chat_id } => match self.ready_db_and_terminal_probe() {
                Ok((db, probe)) => {
                    super::salon::chat_get(&db, SINGLE_USER_ID, &chat_id, probe.as_deref()).await
                }
                Err(r) => r,
            },
            Request::ChatSettings => match self.ready_db() {
                Ok(db) => super::settings::chat_settings_get(&db, SINGLE_USER_ID).await,
                Err(r) => r,
            },
            Request::ChatSettingsUpdate { settings } => match self.ready_db() {
                Ok(db) => {
                    super::settings::chat_settings_update(&db, SINGLE_USER_ID, &settings).await
                }
                Err(r) => r,
            },
            Request::ConnectionProfileList { image_capable } => match self.ready_db() {
                Ok(db) => {
                    super::settings::connection_profile_list(&db, SINGLE_USER_ID, image_capable)
                }
                Err(r) => r,
            },
            Request::ConnectionProfileCreate { profile } => match self.ready_db() {
                Ok(db) => {
                    super::settings::connection_profile_create(&db, SINGLE_USER_ID, &profile).await
                }
                Err(r) => r,
            },
            Request::ConnectionProfileUpdate {
                profile_id,
                profile,
            } => match self.ready_db() {
                Ok(db) => {
                    super::settings::connection_profile_update(
                        &db,
                        SINGLE_USER_ID,
                        &profile_id,
                        &profile,
                    )
                    .await
                }
                Err(r) => r,
            },
            Request::ConnectionProfileDelete { profile_id } => match self.ready_db() {
                Ok(db) => super::settings::connection_profile_delete(&db, &profile_id).await,
                Err(r) => r,
            },
            Request::ConnectionProfileGetTags { profile_id } => match self.ready_db() {
                Ok(db) => super::settings::connection_profile_get_tags(&db, &profile_id),
                Err(r) => r,
            },
            Request::ConnectionProfileAddTag { profile_id, tag_id } => match self.ready_db() {
                Ok(db) => {
                    super::settings::connection_profile_add_tag(&db, &profile_id, &tag_id).await
                }
                Err(r) => r,
            },
            Request::ConnectionProfileRemoveTag { profile_id, tag_id } => match self.ready_db() {
                Ok(db) => {
                    super::settings::connection_profile_remove_tag(&db, &profile_id, &tag_id).await
                }
                Err(r) => r,
            },
            Request::ConnectionProfileReorder { ordered_ids } => match self.ready_db() {
                Ok(db) => {
                    super::settings::connection_profile_reorder(&db, SINGLE_USER_ID, &ordered_ids)
                        .await
                }
                Err(r) => r,
            },
            Request::ConnectionProfileResetSort => match self.ready_db() {
                Ok(db) => super::settings::connection_profile_reset_sort(&db, SINGLE_USER_ID).await,
                Err(r) => r,
            },
            // The wire actions delegate to the assembly's provider-actions
            // driver (the P4.6 unification wire over the live seams —
            // validator / completion / models fetch); a driver-less assembly
            // keeps the P4.6d refusal.
            Request::ConnectionProfileTest { profile } => match self.ready_provider_actions() {
                Ok(d) => d.connection_test(profile).await,
                Err(r) => r,
            },
            Request::ConnectionProfileTestMessage { profile } => {
                match self.ready_provider_actions() {
                    Ok(d) => d.connection_test_message(profile).await,
                    Err(r) => r,
                }
            }
            Request::ApiKeyTest {
                api_key_id,
                base_url,
            } => match self.ready_provider_actions() {
                Ok(d) => {
                    d.api_key_test(SINGLE_USER_ID.to_string(), api_key_id, base_url)
                        .await
                }
                Err(r) => r,
            },
            Request::ModelFetch {
                provider,
                api_key_id,
                base_url,
            } => match self.ready_provider_actions() {
                Ok(d) => {
                    d.model_fetch(SINGLE_USER_ID.to_string(), provider, api_key_id, base_url)
                        .await
                }
                Err(r) => r,
            },
            Request::ApiKeyList => match self.ready_db() {
                Ok(db) => super::settings::api_key_list(&db, SINGLE_USER_ID),
                Err(r) => r,
            },
            Request::ApiKeyCreate {
                label,
                provider,
                api_key,
            } => match self.ready_db() {
                Ok(db) => {
                    super::settings::api_key_create(
                        &db,
                        SINGLE_USER_ID,
                        &provider,
                        &label,
                        &api_key,
                    )
                    .await
                }
                Err(r) => r,
            },
            Request::ApiKeyUpdate {
                api_key_id,
                label,
                is_active,
                api_key,
            } => match self.ready_db() {
                Ok(db) => {
                    super::settings::api_key_update(
                        &db,
                        &api_key_id,
                        label.as_deref(),
                        is_active,
                        api_key.as_deref(),
                    )
                    .await
                }
                Err(r) => r,
            },
            Request::ApiKeyDelete { api_key_id } => match self.ready_db() {
                Ok(db) => super::settings::api_key_delete(&db, &api_key_id).await,
                Err(r) => r,
            },
            Request::ProviderList => match self.ready_db() {
                Ok(_) => super::settings::provider_list(&self.search_providers()),
                Err(r) => r,
            },
            Request::ModelList { provider } => match self.ready_db() {
                Ok(db) => super::settings::model_list(&db, provider.as_deref()),
                Err(r) => r,
            },
            Request::ChatTurnAction {
                chat_id,
                action,
                participant_id,
            } => match self.ready_db() {
                Ok(db) => {
                    super::salon::turn_action(
                        &db,
                        &chat_id,
                        &action,
                        participant_id.as_deref(),
                        random_f64(),
                    )
                    .await
                }
                Err(r) => r,
            },
            Request::ChatRecallReplay {
                chat_id,
                turn_index,
                character_id,
                limit,
            } => match self.ready_db_and_recall_replay() {
                Ok((db, driver)) => {
                    super::recall_replay::recall_replay(
                        &db,
                        driver.as_ref(),
                        SINGLE_USER_ID,
                        &chat_id,
                        turn_index.as_ref(),
                        character_id.as_ref(),
                        limit.as_ref(),
                        crate::clock::now_unix_ms() as f64,
                    )
                    .await
                }
                Err(r) => r,
            },

            // === P4.9G1: the Data & System server surface ===
            Request::SystemTasksQueue => match self.ready_job_pump() {
                Ok((db, pump)) => {
                    let status = pump.status();
                    match super::system_data::read_max_concurrent_jobs(&db) {
                        Ok(max) => {
                            super::system_data::tasks_queue(&db, SINGLE_USER_ID, &status, max)
                        }
                        Err(r) => r,
                    }
                }
                Err(r) => r,
            },
            Request::SystemTasksQueueControl { action } => match self.ready_job_pump() {
                Ok((_db, pump)) => match super::system_data::validate_control_action(&action) {
                    Ok(()) => {
                        if action == "start" {
                            pump.start();
                        } else {
                            pump.stop();
                        }
                        super::system_data::tasks_queue_control_response(&action, &pump.status())
                    }
                    Err(r) => r,
                },
                Err(r) => r,
            },
            Request::SystemJobConcurrencyGet => match self.ready_db() {
                Ok(db) => match super::system_data::read_max_concurrent_jobs(&db) {
                    Ok(max) => super::system_data::job_concurrency_get_response(max),
                    Err(r) => r,
                },
                Err(r) => r,
            },
            Request::SystemJobConcurrencySet {
                max_concurrent_jobs,
            } => match self.ready_job_pump() {
                Ok((db, pump)) => {
                    let resp =
                        super::system_data::job_concurrency_set(&db, max_concurrent_jobs).await;
                    if matches!(resp, Response::System(_)) {
                        pump.wake();
                    }
                    resp
                }
                Err(r) => r,
            },
            Request::SystemJobGet { job_id } => match self.ready_db() {
                Ok(db) => super::system_data::job_get(&db, &job_id),
                Err(r) => r,
            },
            Request::SystemJobControl { job_id, action } => match self.ready_db() {
                Ok(db) => match super::system_data::job_control(&db, &job_id, &action).await {
                    super::system_data::JobControlOutcome::Responded(resp) => resp,
                    super::system_data::JobControlOutcome::Resumed(resp) => {
                        // v4 resume calls ensureProcessorRunning — best-effort nudge.
                        self.nudge_job_pump();
                        resp
                    }
                },
                Err(r) => r,
            },
            Request::SystemJobDelete { job_id } => match self.ready_db() {
                Ok(db) => super::system_data::job_delete(&db, &job_id).await,
                Err(r) => r,
            },
            // ── P4.9G3 arms ──
            Request::SystemDeleteDataPreview => match self.ready_db() {
                Ok(db) => super::system_data::delete_data_preview(&db, SINGLE_USER_ID),
                Err(r) => r,
            },
            Request::SystemDeleteData {
                confirm,
                keep_archived_character_bundles,
            } => match self.ready_db() {
                Ok(db) => {
                    // dogfood #60 — no job may claim while the tables are being
                    // emptied. See `system_data::PumpPause`.
                    let _pump = super::system_data::PumpPause::new(self.job_pump_control());
                    super::system_data::delete_data(
                        &db,
                        SINGLE_USER_ID,
                        &confirm,
                        keep_archived_character_bundles,
                    )
                    .await
                }
                Err(r) => r,
            },
            // ── end P4.9G3 ──
            // ── P4.9G4 arms ──
            Request::SystemExportEntities { entity_type } => match self.ready_db() {
                Ok(db) => super::system_qtap::export_entities(&db, SINGLE_USER_ID, &entity_type),
                Err(r) => r,
            },
            Request::SystemExportPreview {
                entity_type,
                scope,
                selected_ids,
                include_memories,
            } => match self.ready_db() {
                Ok(db) => super::system_qtap::export_preview(
                    &db,
                    SINGLE_USER_ID,
                    crate::services::qtap_export::ExportOptions {
                        entity_type,
                        // v4's route: `searchParams.get('scope') || 'all'`.
                        scope: scope
                            .filter(|s| !s.is_empty())
                            .unwrap_or_else(|| "all".into()),
                        selected_ids,
                        include_memories,
                    },
                ),
                Err(r) => r,
            },
            Request::SystemImportPreview { export_data } => match self.ready_db() {
                Ok(db) => super::system_qtap::import_preview(&db, SINGLE_USER_ID, &export_data),
                Err(r) => r,
            },
            Request::SystemImportExecute {
                export_data,
                options,
            } => match self.ready_db() {
                Ok(db) => {
                    super::system_qtap::import_execute(
                        &db,
                        SINGLE_USER_ID,
                        &export_data,
                        &options,
                        self.qtap_pixel_codec(),
                    )
                    .await
                }
                Err(r) => r,
            },
            // ── end P4.9G4 ──
            // ── P4.9E2A arms ──
            Request::ChatAnnouncementPost {
                chat_id,
                content_markdown,
                sender,
                target_participant_ids,
            } => match self.ready_db() {
                Ok(db) => {
                    super::chat_post_office::chat_announcement_post(
                        &db,
                        &chat_id,
                        &content_markdown,
                        &sender,
                        target_participant_ids.as_deref(),
                    )
                    .await
                }
                Err(r) => r,
            },
            Request::ChatAnnouncementPreview {
                chat_id,
                seed_markdown,
                character_id,
                connection_profile_id,
                system_prompt_id,
                target_participant_ids,
            } => match self.ready_db_and_announcement_preview() {
                Ok((db, driver)) => {
                    super::chat_post_office::chat_announcement_preview(
                        &db,
                        driver.as_ref(),
                        SINGLE_USER_ID,
                        &chat_id,
                        &seed_markdown,
                        &character_id,
                        &connection_profile_id,
                        system_prompt_id.as_deref(),
                        target_participant_ids.as_deref(),
                    )
                    .await
                }
                Err(r) => r,
            },
            Request::ChatSendMail {
                chat_id,
                from_character_id,
                to_character_id,
                body_markdown,
                in_reply_to_path,
            } => match self.ready_db() {
                Ok(db) => {
                    super::chat_post_office::chat_send_mail(
                        &db,
                        &chat_id,
                        &from_character_id,
                        &to_character_id,
                        &body_markdown,
                        in_reply_to_path.as_deref(),
                        &crate::clock::now_iso(),
                    )
                    .await
                }
                Err(r) => r,
            },
            Request::ChatMailboxList {
                chat_id,
                character_id,
            } => match self.ready_db() {
                Ok(db) => {
                    super::chat_post_office::chat_mailbox_list(&db, &chat_id, &character_id).await
                }
                Err(r) => r,
            },
            // ── end P4.9E2A ──
            // ── P4.9E1A: the chat cast + avatar-override verbs ──
            Request::ChatAddParticipant {
                chat_id,
                character_id,
                connection_profile_id,
                image_profile_id,
                display_order,
                has_history_access,
                join_scenario,
                controlled_by,
                outfit_selection,
            } => match self.ready_db() {
                Ok(db) => {
                    let data = crate::services::chat_participants::ParticipantAddData {
                        character_id,
                        connection_profile_id,
                        image_profile_id,
                        display_order,
                        has_history_access,
                        join_scenario,
                        controlled_by,
                        outfit_selection,
                    };
                    let runner = self.outfit_llm_choose();
                    super::chat_cast::chat_add_participant(
                        &db,
                        SINGLE_USER_ID,
                        &chat_id,
                        &data,
                        runner.as_ref(),
                    )
                    .await
                }
                Err(r) => r,
            },
            Request::ChatUpdateParticipant {
                chat_id,
                participant_id,
                connection_profile_id,
                image_profile_id,
                selected_system_prompt_id,
                display_order,
                is_active,
                status,
                controlled_by,
                has_history_access,
                join_scenario,
                talkativeness,
            } => match self.ready_db() {
                Ok(db) => {
                    let data = crate::services::chat_participants::ParticipantUpdateData {
                        participant_id,
                        connection_profile_id,
                        image_profile_id,
                        selected_system_prompt_id,
                        display_order,
                        is_active,
                        status,
                        controlled_by,
                        has_history_access,
                        join_scenario,
                        talkativeness,
                    };
                    super::chat_cast::chat_update_participant(&db, &chat_id, &data).await
                }
                Err(r) => r,
            },
            Request::ChatRemoveParticipant {
                chat_id,
                participant_id,
            } => match self.ready_db() {
                Ok(db) => {
                    super::chat_cast::chat_remove_participant(&db, &chat_id, &participant_id).await
                }
                Err(r) => r,
            },
            Request::ChatRebuildSystemPrompt {
                chat_id,
                participant_id,
            } => match self.ready_db() {
                Ok(db) => {
                    super::chat_cast::chat_rebuild_system_prompt(&db, &chat_id, &participant_id)
                        .await
                }
                Err(r) => r,
            },
            Request::ChatGetAvatars { chat_id } => match self.ready_db() {
                Ok(db) => super::chat_cast::chat_get_avatars(&db, SINGLE_USER_ID, &chat_id),
                Err(r) => r,
            },
            Request::ChatSetAvatar {
                chat_id,
                character_id,
                image_id,
            } => match self.ready_db() {
                Ok(db) => {
                    super::chat_cast::chat_set_avatar(&db, &chat_id, &character_id, &image_id).await
                }
                Err(r) => r,
            },
            Request::ChatRemoveAvatar {
                chat_id,
                character_id,
            } => match self.ready_db() {
                Ok(db) => super::chat_cast::chat_remove_avatar(&db, &chat_id, &character_id).await,
                Err(r) => r,
            },
            Request::ChatToggleAvatarGeneration { chat_id } => match self.ready_db() {
                Ok(db) => {
                    super::chat_cast::chat_toggle_avatar_generation(&db, SINGLE_USER_ID, &chat_id)
                        .await
                }
                Err(r) => r,
            },
            // ── end P4.9E1A ──
            // === P4.9E3A: the chat-admin + tools verbs ===
            Request::ChatRegenerateTitle { chat_id } => match self.ready_regenerate_title() {
                Ok(driver) => driver.run(chat_id).await,
                Err(r) => r,
            },
            Request::ChatAddTag { chat_id, tag_id } => match self.ready_db() {
                Ok(db) => crate::services::chat_admin::chat_add_tag(&db, &chat_id, &tag_id).await,
                Err(r) => r,
            },
            Request::ChatRemoveTag { chat_id, tag_id } => match self.ready_db() {
                Ok(db) => {
                    crate::services::chat_admin::chat_remove_tag(&db, &chat_id, &tag_id).await
                }
                Err(r) => r,
            },
            Request::ChatBulkReattribute {
                chat_id,
                source_participant_id,
                target_participant_id,
                role_filter,
            } => match self.ready_db() {
                Ok(db) => {
                    crate::services::chat_admin::chat_bulk_reattribute(
                        &db,
                        &chat_id,
                        source_participant_id.as_deref(),
                        &target_participant_id,
                        role_filter.as_deref(),
                    )
                    .await
                }
                Err(r) => r,
            },
            Request::ChatMergeConversation {
                chat_id,
                source_chat_id,
                character_ids,
                outfit_selections,
            } => match self.ready_db() {
                Ok(db) => {
                    let runner = self.outfit_llm_choose();
                    crate::services::chat_merge::chat_merge_conversation(
                        &db,
                        SINGLE_USER_ID,
                        &chat_id,
                        &source_chat_id,
                        character_ids.as_deref(),
                        outfit_selections.as_deref(),
                        runner.as_ref(),
                    )
                    .await
                }
                Err(r) => r,
            },
            Request::ChatUpdateToolSettings {
                chat_id,
                disabled_tools,
                disabled_tool_groups,
            } => match self.ready_db() {
                Ok(db) => {
                    crate::services::chat_admin::chat_update_tool_settings(
                        &db,
                        &chat_id,
                        disabled_tools,
                        disabled_tool_groups,
                    )
                    .await
                }
                Err(r) => r,
            },
            // §1 carries no `enabled` field, so only v4's ABSENT arm is
            // reachable from the wire (the service takes the full tri-state and
            // the differential covers all four — see `services/chat_admin.rs`).
            Request::ChatRunTool {
                chat_id,
                tool_name,
                arguments,
                character_id,
                private,
            } => match self.ready_db_and_operator_tool_runner() {
                Ok((db, runner)) => {
                    let operator_name =
                        crate::services::chat_run_tool::operator_display_name(&db, SINGLE_USER_ID);
                    crate::services::chat_run_tool::chat_run_tool(
                        &db,
                        SINGLE_USER_ID,
                        &chat_id,
                        &tool_name,
                        arguments.as_ref(),
                        character_id.as_deref(),
                        private,
                        runner.as_deref(),
                        operator_name,
                        uuid::Uuid::new_v4().to_string(),
                        crate::clock::now_iso(),
                    )
                    .await
                }
                Err(r) => r,
            },
            Request::ChatRng {
                chat_id,
                kind,
                rolls,
                preview,
            } => match self.ready_db() {
                Ok(db) => {
                    let mut rng = crate::tools::rng::OsRandomBytes;
                    crate::services::chat_rng::chat_rng(
                        &db,
                        SINGLE_USER_ID,
                        &chat_id,
                        &kind,
                        rolls,
                        preview,
                        &mut rng,
                        uuid::Uuid::new_v4().to_string(),
                        crate::clock::now_iso(),
                    )
                    .await
                }
                Err(r) => r,
            },
            Request::ChatToggleAgentMode { chat_id, enabled } => match self.ready_db() {
                Ok(db) => {
                    crate::services::chat_admin::chat_toggle_agent_mode(
                        &db,
                        SINGLE_USER_ID,
                        &chat_id,
                        enabled,
                    )
                    .await
                }
                Err(r) => r,
            },
            Request::ChatSetScenario {
                chat_id,
                scenario,
                scenario_id,
                project_scenario_path,
                group_scenario_path,
                group_scenario_group_id,
                general_scenario_path,
            } => match self.ready_db() {
                Ok(db) => {
                    crate::services::chat_scenario::chat_set_scenario(
                        &db,
                        &chat_id,
                        crate::services::chat_scenario::SetScenarioBody {
                            scenario,
                            scenario_id,
                            project_scenario_path,
                            group_scenario_path,
                            group_scenario_group_id,
                            general_scenario_path,
                        },
                    )
                    .await
                }
                Err(r) => r,
            },
            Request::ChatReclassifyDanger { chat_id } => match self.ready_db() {
                Ok(db) => {
                    crate::services::chat_admin::chat_reclassify_danger(
                        &db,
                        SINGLE_USER_ID,
                        &chat_id,
                    )
                    .await
                }
                Err(r) => r,
            },
            Request::ChatRenderConversation { chat_id } => match self.ready_db() {
                Ok(db) => {
                    crate::services::chat_admin::chat_render_conversation(
                        &db,
                        SINGLE_USER_ID,
                        &chat_id,
                    )
                    .await
                }
                Err(r) => r,
            },
            // === end P4.9E3A ===
            // === P4.9E3B: the chat-dialog server remainder ===
            Request::ChatExport { chat_id } => match self.ready_db() {
                Ok(db) => crate::services::chat_export::chat_export(&db, SINGLE_USER_ID, &chat_id),
                Err(r) => r,
            },
            // === P4.d28: the readable Markdown transcript ===
            Request::ChatExportMarkdown { chat_id } => match self.ready_db() {
                Ok(db) => crate::services::markdown_transcript::chat_export_markdown(
                    &db,
                    SINGLE_USER_ID,
                    &chat_id,
                ),
                Err(r) => r,
            },
            Request::ChatOutfitSummary { chat_id } => match self.ready_db() {
                Ok(db) => super::chat_outfits::chat_outfit_summary(&db, &chat_id),
                Err(r) => r,
            },
            Request::ToolsList {
                chat_id,
                include_schemas,
            } => match self.ready_db() {
                Ok(db) => crate::services::tools_inventory::tools_list(
                    &db,
                    SINGLE_USER_ID,
                    chat_id.as_deref(),
                    include_schemas.unwrap_or(false),
                    self.web_search_configured(),
                ),
                Err(r) => r,
            },
            Request::SearchReplacePreview {
                scope,
                search_text,
                replace_text,
                include_messages,
                include_memories,
            } => match self.ready_db() {
                Ok(db) => {
                    crate::services::search_replace::search_replace(
                        &db,
                        SINGLE_USER_ID,
                        crate::services::search_replace::Action::Preview,
                        &scope,
                        &search_text,
                        &replace_text,
                        include_messages,
                        include_memories,
                    )
                    .await
                }
                Err(r) => r,
            },
            Request::SearchReplaceExecute {
                scope,
                search_text,
                replace_text,
                include_messages,
                include_memories,
            } => match self.ready_db() {
                Ok(db) => {
                    crate::services::search_replace::search_replace(
                        &db,
                        SINGLE_USER_ID,
                        crate::services::search_replace::Action::Execute,
                        &scope,
                        &search_text,
                        &replace_text,
                        include_messages,
                        include_memories,
                    )
                    .await
                }
                Err(r) => r,
            },
            Request::MessageReattribute {
                message_id,
                new_participant_id,
            } => match self.ready_db() {
                Ok(db) => {
                    crate::services::message_reattribute::message_reattribute(
                        &db,
                        SINGLE_USER_ID,
                        &message_id,
                        &new_participant_id,
                    )
                    .await
                }
                Err(r) => r,
            },
            // === end P4.9E3B ===
            // ── P4.9G5 arms ──
            Request::SystemBackupCreate { compact } => match self.ready_backup_host() {
                Ok((db, host)) => super::system_backup::backup_create(&db, host.as_ref(), compact),
                Err(r) => r,
            },
            Request::SystemRestorePreview { upload_id } => match self.ready_backup_host() {
                Ok((_db, host)) => super::system_backup::restore_preview(host.as_ref(), &upload_id),
                Err(r) => r,
            },
            Request::SystemRestoreExecute {
                upload_id,
                mode,
                keep_archived_character_bundles,
            } => match self.ready_backup_host() {
                Ok((db, host)) => {
                    // dogfood #60 — a restore truncates and repopulates 43 tables
                    // across three partitions; nothing may write into the middle
                    // of that. See `system_data::PumpPause`.
                    let _pump = super::system_data::PumpPause::new(self.job_pump_control());
                    super::system_backup::restore_execute(
                        &db,
                        host.as_ref(),
                        &upload_id,
                        &mode,
                        keep_archived_character_bundles.as_ref(),
                    )
                    .await
                }
                Err(r) => r,
            },
            // ── end P4.9G5 ──
            // === end P4.9G1 ===
            Request::MessageEdit {
                message_id,
                content,
            } => match self.ready_db() {
                Ok(db) => {
                    super::salon::message_edit(&db, SINGLE_USER_ID, &message_id, &content).await
                }
                Err(r) => r,
            },
            Request::MessageDelete {
                message_id,
                memory_action,
                skip_confirmation,
            } => match self.ready_db() {
                Ok(db) => {
                    super::salon::message_delete(
                        &db,
                        SINGLE_USER_ID,
                        &message_id,
                        memory_action.as_deref(),
                        skip_confirmation,
                    )
                    .await
                }
                Err(r) => r,
            },
            Request::MessageSwipe {
                message_id,
                swipe_index,
            } => match self.ready_db() {
                Ok(db) => match swipe_index {
                    Some(idx) => {
                        super::salon::message_swipe_switch(&db, SINGLE_USER_ID, &message_id, idx)
                    }
                    // The generate branch (the P4.6c unification wire): the
                    // assembly's model driver composes the regeneration.
                    None => match self.ready_swipe() {
                        Ok((db, driver)) => {
                            super::salon::message_swipe_generate(
                                &db,
                                driver.as_ref(),
                                SINGLE_USER_ID,
                                &message_id,
                            )
                            .await
                        }
                        Err(r) => r,
                    },
                },
                Err(r) => r,
            },
            Request::ChatUpdate {
                chat_id,
                chat,
                // ── P4.9E1A: the bag's participant families ──
                update_participant,
                add_participant,
                remove_participant_id,
            } => match self.ready_db() {
                Ok(db) => {
                    super::salon::chat_update(
                        &db,
                        SINGLE_USER_ID,
                        &chat_id,
                        &chat,
                        update_participant.as_ref(),
                        add_participant.as_ref(),
                        remove_participant_id.as_deref(),
                    )
                    .await
                }
                Err(r) => r,
            },
            Request::ChatImpersonate {
                chat_id,
                participant_id,
            } => match self.ready_db() {
                Ok(db) => super::salon::chat_impersonate(&db, &chat_id, &participant_id).await,
                Err(r) => r,
            },
            Request::ChatStopImpersonate {
                chat_id,
                participant_id,
                new_connection_profile_id,
            } => match self.ready_db() {
                Ok(db) => {
                    super::salon::chat_stop_impersonate(
                        &db,
                        &chat_id,
                        &participant_id,
                        new_connection_profile_id.as_deref(),
                    )
                    .await
                }
                Err(r) => r,
            },
            Request::ChatSetActiveSpeaker {
                chat_id,
                participant_id,
            } => match self.ready_db() {
                Ok(db) => {
                    super::salon::chat_set_active_speaker(&db, &chat_id, &participant_id).await
                }
                Err(r) => r,
            },
            Request::ChatSend {
                chat_id,
                content,
                continue_mode,
                responding_participant_id,
                target_participant_ids,
                speaking_as_participant_id,
                file_ids,
                nudge,
                pending_tool_results,
            } => {
                // v4 `sendMessageSchema` superRefine: a normal (non-continue) send
                // with blank content, no files, and no tool results is rejected.
                if !continue_mode
                    && content.trim().is_empty()
                    && file_ids.is_empty()
                    && pending_tool_results.is_empty()
                {
                    return Response::error(
                        ErrorKind::BadRequest,
                        "Message must have content, attached files, or tool results",
                    );
                }
                self.chat_send(ChatSendRequest {
                    chat_id,
                    content,
                    continue_mode,
                    responding_participant_id,
                    target_participant_ids,
                    speaking_as_participant_id,
                    file_ids,
                    nudge,
                    pending_tool_results,
                })
                .await
            }
            Request::ChatCreate { request } => {
                self.chat_create(ChatCreateDriverRequest {
                    raw: serde_json::Value::Object(request),
                })
                .await
            }
            // --- Characters family (P4.6f) ---------------------------------
            Request::CharacterList {
                archived,
                npc,
                controlled_by,
            } => match self.ready_db() {
                Ok(db) => super::characters::character_list(
                    &db,
                    SINGLE_USER_ID,
                    archived.as_deref(),
                    npc.as_deref(),
                    controlled_by.as_deref(),
                ),
                Err(r) => r,
            },
            Request::CharacterGet { character_id } => match self.ready_db() {
                Ok(db) => super::characters::character_get(&db, SINGLE_USER_ID, &character_id),
                Err(r) => r,
            },
            Request::CharacterDefaultPartner { character_id } => match self.ready_db() {
                Ok(db) => {
                    super::characters::character_default_partner(&db, SINGLE_USER_ID, &character_id)
                }
                Err(r) => r,
            },
            Request::CharacterGetTags { character_id } => match self.ready_db() {
                Ok(db) => super::characters::character_get_tags(&db, SINGLE_USER_ID, &character_id),
                Err(r) => r,
            },
            Request::CharacterPromptList { character_id } => match self.ready_db() {
                Ok(db) => {
                    super::characters::character_prompt_list(&db, SINGLE_USER_ID, &character_id)
                }
                Err(r) => r,
            },
            Request::CharacterScenarioList {
                character_id,
                include_archived,
            } => match self.ready_db() {
                Ok(db) => super::characters::character_scenario_list(
                    &db,
                    SINGLE_USER_ID,
                    &character_id,
                    include_archived,
                ),
                Err(r) => r,
            },
            Request::CharacterWardrobeList {
                character_id,
                scope,
                include_archived,
            } => match self.ready_db() {
                Ok(db) => super::characters::character_wardrobe_list(
                    &db,
                    SINGLE_USER_ID,
                    &character_id,
                    scope.as_deref(),
                    include_archived,
                ),
                Err(r) => r,
            },
            Request::CharacterWardrobeInstructionsGet { character_id } => match self.ready_db() {
                Ok(db) => {
                    super::characters::character_wardrobe_instructions_get(&db, &character_id)
                }
                Err(r) => r,
            },
            Request::CharacterWardrobeInstructionsSet {
                character_id,
                instructions,
            } => match self.ready_db() {
                Ok(db) => {
                    super::characters::character_wardrobe_instructions_set(
                        &db,
                        &character_id,
                        &instructions,
                    )
                    .await
                }
                Err(r) => r,
            },
            Request::CharacterPluginDataMap { character_id } => match self.ready_db() {
                Ok(db) => {
                    super::characters::character_plugin_data_map(&db, SINGLE_USER_ID, &character_id)
                }
                Err(r) => r,
            },
            Request::CharacterPluginDataGet {
                character_id,
                plugin_name,
            } => match self.ready_db() {
                Ok(db) => super::characters::character_plugin_data_get(
                    &db,
                    SINGLE_USER_ID,
                    &character_id,
                    &plugin_name,
                ),
                Err(r) => r,
            },
            // --- Deferred to later P4.6f milestones (loud refusal) ----------
            Request::CharacterCreate { character } => match self.ready_db() {
                Ok(db) => super::characters::character_create(&db, SINGLE_USER_ID, character).await,
                Err(r) => r,
            },
            Request::CharacterQuickCreate { name } => match self.ready_db() {
                Ok(db) => {
                    super::characters::character_quick_create(&db, SINGLE_USER_ID, &name).await
                }
                Err(r) => r,
            },
            Request::CharacterUpdate {
                character_id,
                character,
            } => match self.ready_db() {
                Ok(db) => {
                    super::characters::character_update(
                        &db,
                        SINGLE_USER_ID,
                        &character_id,
                        character,
                    )
                    .await
                }
                Err(r) => r,
            },
            Request::CharacterDelete {
                character_id,
                cascade_chats,
                cascade_images,
            } => match self.ready_db() {
                Ok(db) => {
                    super::characters::character_delete(
                        &db,
                        SINGLE_USER_ID,
                        &character_id,
                        cascade_chats,
                        cascade_images,
                    )
                    .await
                }
                Err(r) => r,
            },
            Request::CharacterCascadePreview { character_id } => match self.ready_db() {
                Ok(db) => {
                    super::characters::character_cascade_preview(&db, SINGLE_USER_ID, &character_id)
                }
                Err(r) => r,
            },
            Request::CharacterAvatar {
                character_id,
                image_id,
            } => match self.ready_db() {
                Ok(db) => {
                    super::characters::character_avatar(
                        &db,
                        SINGLE_USER_ID,
                        &character_id,
                        image_id.as_deref(),
                    )
                    .await
                }
                Err(r) => r,
            },
            Request::CharacterFavorite { character_id } => match self.ready_db() {
                Ok(db) => {
                    super::characters::character_favorite(&db, SINGLE_USER_ID, &character_id).await
                }
                Err(r) => r,
            },
            Request::CharacterToggleControlledBy { character_id } => match self.ready_db() {
                Ok(db) => {
                    super::characters::character_toggle_controlled_by(
                        &db,
                        SINGLE_USER_ID,
                        &character_id,
                    )
                    .await
                }
                Err(r) => r,
            },
            Request::CharacterToggleCarina { character_id } => match self.ready_db() {
                Ok(db) => {
                    super::characters::character_toggle_carina(&db, SINGLE_USER_ID, &character_id)
                        .await
                }
                Err(r) => r,
            },
            Request::CharacterSetDefaultPartner {
                character_id,
                partner_id,
            } => match self.ready_db() {
                Ok(db) => {
                    super::characters::character_set_default_partner(
                        &db,
                        SINGLE_USER_ID,
                        &character_id,
                        partner_id.as_deref(),
                    )
                    .await
                }
                Err(r) => r,
            },
            Request::CharacterAddTag {
                character_id,
                tag_id,
            } => match self.ready_db() {
                Ok(db) => {
                    super::characters::character_add_tag(
                        &db,
                        SINGLE_USER_ID,
                        &character_id,
                        &tag_id,
                    )
                    .await
                }
                Err(r) => r,
            },
            Request::CharacterRemoveTag {
                character_id,
                tag_id,
            } => match self.ready_db() {
                Ok(db) => {
                    super::characters::character_remove_tag(
                        &db,
                        SINGLE_USER_ID,
                        &character_id,
                        &tag_id,
                    )
                    .await
                }
                Err(r) => r,
            },
            Request::CharacterStats { character_id } => match self.ready_db() {
                Ok(db) => super::characters::character_stats(&db, SINGLE_USER_ID, &character_id),
                Err(r) => r,
            },
            Request::CharacterChats {
                character_id,
                search,
                limit,
                offset,
            } => match self.ready_db() {
                Ok(db) => super::characters::character_chats(
                    &db,
                    SINGLE_USER_ID,
                    &character_id,
                    search.as_deref(),
                    limit,
                    offset,
                ),
                Err(r) => r,
            },
            Request::CharacterDepictionGuidelines { character_id } => match self.ready_db() {
                Ok(db) => super::characters::character_depiction_guidelines(
                    &db,
                    SINGLE_USER_ID,
                    &character_id,
                ),
                Err(r) => r,
            },
            Request::CharacterDepictionGuidelinesUpdate {
                character_id,
                content,
            } => match self.ready_db() {
                Ok(db) => {
                    super::characters::character_depiction_guidelines_update(
                        &db,
                        SINGLE_USER_ID,
                        &character_id,
                        &content,
                    )
                    .await
                }
                Err(r) => r,
            },
            Request::CharacterPromptCreate {
                character_id,
                name,
                content,
                is_default,
            } => match self.ready_db() {
                Ok(db) => {
                    super::characters::character_prompt_create(
                        &db,
                        SINGLE_USER_ID,
                        &character_id,
                        &name,
                        &content,
                        is_default,
                    )
                    .await
                }
                Err(r) => r,
            },
            Request::CharacterPromptUpdate {
                character_id,
                prompt_id,
                name,
                content,
                is_default,
            } => match self.ready_db() {
                Ok(db) => {
                    super::characters::character_prompt_update(
                        &db,
                        SINGLE_USER_ID,
                        &character_id,
                        &prompt_id,
                        name.as_deref(),
                        content.as_deref(),
                        is_default,
                    )
                    .await
                }
                Err(r) => r,
            },
            Request::CharacterPromptDelete {
                character_id,
                prompt_id,
            } => match self.ready_db() {
                Ok(db) => {
                    super::characters::character_prompt_delete(
                        &db,
                        SINGLE_USER_ID,
                        &character_id,
                        &prompt_id,
                    )
                    .await
                }
                Err(r) => r,
            },
            Request::CharacterPromptSetDefault {
                character_id,
                prompt_id,
            } => match self.ready_db() {
                Ok(db) => {
                    super::characters::character_prompt_set_default(
                        &db,
                        SINGLE_USER_ID,
                        &character_id,
                        &prompt_id,
                    )
                    .await
                }
                Err(r) => r,
            },
            Request::CharacterScenarioCreate {
                character_id,
                title,
                content,
                archived,
            } => match self.ready_db() {
                Ok(db) => {
                    super::characters::character_scenario_create(
                        &db,
                        SINGLE_USER_ID,
                        &character_id,
                        &title,
                        &content,
                        &archived,
                    )
                    .await
                }
                Err(r) => r,
            },
            Request::CharacterScenarioUpdate {
                character_id,
                scenario_id,
                title,
                content,
                archived,
            } => match self.ready_db() {
                Ok(db) => {
                    super::characters::character_scenario_update(
                        &db,
                        SINGLE_USER_ID,
                        &character_id,
                        &scenario_id,
                        title.as_deref(),
                        content.as_deref(),
                        &archived,
                    )
                    .await
                }
                Err(r) => r,
            },
            Request::CharacterScenarioDelete {
                character_id,
                scenario_id,
            } => match self.ready_db() {
                Ok(db) => {
                    super::characters::character_scenario_delete(
                        &db,
                        SINGLE_USER_ID,
                        &character_id,
                        &scenario_id,
                    )
                    .await
                }
                Err(r) => r,
            },
            Request::CharacterPluginDataUpsert {
                character_id,
                plugin_name,
                data,
            } => match self.ready_db() {
                Ok(db) => {
                    super::characters::character_plugin_data_upsert(
                        &db,
                        SINGLE_USER_ID,
                        &character_id,
                        &plugin_name,
                        data,
                    )
                    .await
                }
                Err(r) => r,
            },
            Request::CharacterPluginDataDelete {
                character_id,
                plugin_name,
            } => match self.ready_db() {
                Ok(db) => {
                    super::characters::character_plugin_data_delete(
                        &db,
                        SINGLE_USER_ID,
                        &character_id,
                        &plugin_name,
                    )
                    .await
                }
                Err(r) => r,
            },
            Request::CharacterWardrobeCreate { character_id, item } => match self.ready_db() {
                Ok(db) => {
                    super::characters::character_wardrobe_create(
                        &db,
                        SINGLE_USER_ID,
                        &character_id,
                        item,
                    )
                    .await
                }
                Err(r) => r,
            },
            Request::CharacterWardrobeGet {
                character_id,
                item_id,
            } => match self.ready_db() {
                Ok(db) => super::characters::character_wardrobe_get(
                    &db,
                    SINGLE_USER_ID,
                    &character_id,
                    &item_id,
                ),
                Err(r) => r,
            },
            Request::CharacterWardrobeUpdate {
                character_id,
                item_id,
                item,
            } => match self.ready_db() {
                Ok(db) => {
                    super::characters::character_wardrobe_update(
                        &db,
                        SINGLE_USER_ID,
                        &character_id,
                        &item_id,
                        item,
                    )
                    .await
                }
                Err(r) => r,
            },
            Request::CharacterWardrobeDelete {
                character_id,
                item_id,
            } => match self.ready_db() {
                Ok(db) => {
                    super::characters::character_wardrobe_delete(
                        &db,
                        SINGLE_USER_ID,
                        &character_id,
                        &item_id,
                    )
                    .await
                }
                Err(r) => r,
            },
            // === P4.D65: the two archive verbs, LIVE ===
            Request::CharacterArchive { character_id } => match self.ready_db() {
                Ok(db) => {
                    let (cached, has_user_passphrase) = self.passphrase_source_parts();
                    let seams = self.archive_seams(cached.as_deref(), has_user_passphrase);
                    super::characters::character_archive(&db, SINGLE_USER_ID, &character_id, &seams)
                        .await
                }
                Err(r) => r,
            },
            Request::CharacterRehydrate { character_id } => match self.ready_db() {
                Ok(db) => {
                    let (cached, has_user_passphrase) = self.passphrase_source_parts();
                    let seams = self.archive_seams(cached.as_deref(), has_user_passphrase);
                    super::characters::character_rehydrate(
                        &db,
                        SINGLE_USER_ID,
                        &character_id,
                        &seams,
                    )
                    .await
                }
                Err(r) => r,
            },
            // === end P4.D65 ===
            Request::CharacterExport {
                character_id,
                format,
            } => match self.ready_db() {
                Ok(db) => super::characters::character_export(
                    &db,
                    SINGLE_USER_ID,
                    &character_id,
                    format.as_deref(),
                ),
                Err(r) => r,
            },
            Request::CharacterImport { payload } => match self.ready_db() {
                Ok(db) => super::characters::character_import(&db, SINGLE_USER_ID, payload).await,
                Err(r) => r,
            },
            Request::CharacterPhotoList {
                character_id,
                limit,
                offset,
            } => match self.ready_db() {
                Ok(db) => super::characters::character_photo_list(
                    &db,
                    SINGLE_USER_ID,
                    &character_id,
                    limit,
                    offset,
                ),
                Err(r) => r,
            },
            Request::CharacterPhotoSaveById {
                character_id,
                file_id,
                link_id,
            } => match self.ready_db() {
                Ok(db) => {
                    super::characters::character_photo_save_by_id(
                        &db,
                        SINGLE_USER_ID,
                        &character_id,
                        file_id.as_deref(),
                        link_id.as_deref(),
                    )
                    .await
                }
                Err(r) => r,
            },
            Request::CharacterPhotoRemove {
                character_id,
                link_id,
            } => match self.ready_db() {
                Ok(db) => {
                    super::characters::character_photo_remove(
                        &db,
                        SINGLE_USER_ID,
                        &character_id,
                        &link_id,
                    )
                    .await
                }
                Err(r) => r,
            },
            Request::TagList { search } => match self.ready_db() {
                Ok(db) => super::characters::tag_list(&db, SINGLE_USER_ID, search.as_deref()),
                Err(r) => r,
            },
            Request::TagCreate { name } => match self.ready_db() {
                Ok(db) => super::characters::tag_create(&db, SINGLE_USER_ID, &name).await,
                Err(r) => r,
            },
            Request::TagGet { tag_id } => match self.ready_db() {
                Ok(db) => super::characters::tag_get(&db, SINGLE_USER_ID, &tag_id),
                Err(r) => r,
            },
            Request::TagUpdate { tag_id, tag } => match self.ready_db() {
                Ok(db) => super::characters::tag_update(&db, SINGLE_USER_ID, &tag_id, tag).await,
                Err(r) => r,
            },
            Request::TagDelete { tag_id } => match self.ready_db() {
                Ok(db) => super::characters::tag_delete(&db, SINGLE_USER_ID, &tag_id).await,
                Err(r) => r,
            },

            // --- Groups family (P4.6k) --------------------------------------
            Request::GroupList => match self.ready_db() {
                Ok(db) => super::groups::group_list(&db),
                Err(r) => r,
            },
            Request::GroupCreate {
                name,
                description,
                instructions,
                color,
                icon,
            } => match self.ready_db() {
                Ok(db) => {
                    super::groups::group_create(&db, name, description, instructions, color, icon)
                        .await
                }
                Err(r) => r,
            },
            Request::GroupGet { group_id } => match self.ready_db() {
                Ok(db) => super::groups::group_get(&db, &group_id),
                Err(r) => r,
            },
            Request::GroupUpdate { group_id, group } => match self.ready_db() {
                Ok(db) => super::groups::group_update(&db, &group_id, group).await,
                Err(r) => r,
            },
            Request::GroupDelete { group_id } => match self.ready_db() {
                Ok(db) => super::groups::group_delete(&db, &group_id).await,
                Err(r) => r,
            },
            Request::GroupMembers { group_id } => match self.ready_db() {
                Ok(db) => super::groups::group_members(&db, &group_id),
                Err(r) => r,
            },
            Request::GroupMemberAdd {
                group_id,
                character_id,
            } => match self.ready_db() {
                Ok(db) => super::groups::group_member_add(&db, &group_id, &character_id).await,
                Err(r) => r,
            },
            Request::GroupMemberRemove {
                group_id,
                character_id,
            } => match self.ready_db() {
                Ok(db) => super::groups::group_member_remove(&db, &group_id, &character_id).await,
                Err(r) => r,
            },
            Request::GroupMountPointList { group_id } => match self.ready_db() {
                Ok(db) => super::groups::group_mount_point_list(&db, &group_id),
                Err(r) => r,
            },
            Request::GroupMountPointLink {
                group_id,
                mount_point_id,
            } => match self.ready_db() {
                Ok(db) => {
                    super::groups::group_mount_point_link(&db, &group_id, &mount_point_id).await
                }
                Err(r) => r,
            },
            Request::GroupMountPointUnlink {
                group_id,
                mount_point_id,
            } => match self.ready_db() {
                Ok(db) => {
                    super::groups::group_mount_point_unlink(&db, &group_id, &mount_point_id).await
                }
                Err(r) => r,
            },
            // --- Groups scenarios (P4.6n) -----------------------------------
            Request::GroupScenarioList {
                group_id,
                include_archived,
            } => match self.ready_db() {
                Ok(db) => {
                    super::groups::group_scenario_list(&db, &group_id, include_archived).await
                }
                Err(r) => r,
            },
            Request::GroupScenarioCreate { group_id, scenario } => match self.ready_db() {
                Ok(db) => super::groups::group_scenario_create(&db, &group_id, scenario).await,
                Err(r) => r,
            },
            Request::GroupScenarioGet {
                group_id,
                scenario_path,
            } => match self.ready_db() {
                Ok(db) => super::groups::group_scenario_get(&db, &group_id, &scenario_path).await,
                Err(r) => r,
            },
            Request::GroupScenarioUpdate {
                group_id,
                scenario_path,
                scenario,
                include_archived,
            } => match self.ready_db() {
                Ok(db) => {
                    super::groups::group_scenario_update(
                        &db,
                        &group_id,
                        &scenario_path,
                        scenario,
                        include_archived,
                    )
                    .await
                }
                Err(r) => r,
            },
            Request::GroupScenarioRename {
                group_id,
                scenario_path,
                new_filename,
                include_archived,
            } => match self.ready_db() {
                Ok(db) => {
                    super::groups::group_scenario_rename(
                        &db,
                        &group_id,
                        &scenario_path,
                        &new_filename,
                        include_archived,
                    )
                    .await
                }
                Err(r) => r,
            },
            Request::GroupScenarioDelete {
                group_id,
                scenario_path,
                include_archived,
            } => match self.ready_db() {
                Ok(db) => {
                    super::groups::group_scenario_delete(
                        &db,
                        &group_id,
                        &scenario_path,
                        include_archived,
                    )
                    .await
                }
                Err(r) => r,
            },
            Request::GroupScenariosUnion {
                character_ids,
                include_archived,
            } => match self.ready_db() {
                Ok(db) => {
                    super::groups::group_scenarios_union(&db, character_ids, include_archived).await
                }
                Err(r) => r,
            },
            // --- Group wardrobe CRUD (P4.D112) ------------------------------
            Request::GroupWardrobeList {
                group_id,
                include_archived,
            } => match self.ready_db() {
                Ok(db) => super::groups::group_wardrobe_list(&db, &group_id, include_archived),
                Err(r) => r,
            },
            Request::GroupWardrobeInstructionsGet { group_id } => match self.ready_db() {
                Ok(db) => super::groups::group_wardrobe_instructions_get(&db, &group_id),
                Err(r) => r,
            },
            Request::GroupWardrobeInstructionsSet {
                group_id,
                instructions,
            } => match self.ready_db() {
                Ok(db) => {
                    super::groups::group_wardrobe_instructions_set(&db, &group_id, &instructions)
                        .await
                }
                Err(r) => r,
            },
            Request::GroupWardrobeCreate { group_id, item } => match self.ready_db() {
                Ok(db) => super::groups::group_wardrobe_create(&db, &group_id, item).await,
                Err(r) => r,
            },
            Request::GroupWardrobeGet { group_id, item_id } => match self.ready_db() {
                Ok(db) => super::groups::group_wardrobe_get(&db, &group_id, &item_id),
                Err(r) => r,
            },
            Request::GroupWardrobeUpdate {
                group_id,
                item_id,
                item,
            } => match self.ready_db() {
                Ok(db) => {
                    super::groups::group_wardrobe_update(&db, &group_id, &item_id, item).await
                }
                Err(r) => r,
            },
            Request::GroupWardrobeDelete { group_id, item_id } => match self.ready_db() {
                Ok(db) => super::groups::group_wardrobe_delete(&db, &group_id, &item_id).await,
                Err(r) => r,
            },

            // --- General (instance-wide) scenarios (P4.6n) -------------------
            Request::ScenarioList { include_archived } => match self.ready_db() {
                Ok(db) => super::scenarios::scenario_list(&db, include_archived).await,
                Err(r) => r,
            },
            Request::ScenarioCreate { scenario } => match self.ready_db() {
                Ok(db) => super::scenarios::scenario_create(&db, scenario).await,
                Err(r) => r,
            },
            Request::ScenarioGet { scenario_path } => match self.ready_db() {
                Ok(db) => super::scenarios::scenario_get(&db, scenario_path).await,
                Err(r) => r,
            },
            Request::ScenarioUpdate {
                scenario_path,
                scenario,
                include_archived,
            } => match self.ready_db() {
                Ok(db) => {
                    super::scenarios::scenario_update(
                        &db,
                        scenario_path,
                        scenario,
                        include_archived,
                    )
                    .await
                }
                Err(r) => r,
            },
            Request::ScenarioRename {
                scenario_path,
                new_filename,
                include_archived,
            } => match self.ready_db() {
                Ok(db) => {
                    super::scenarios::scenario_rename(
                        &db,
                        scenario_path,
                        new_filename,
                        include_archived,
                    )
                    .await
                }
                Err(r) => r,
            },
            Request::ScenarioDelete {
                scenario_path,
                include_archived,
            } => match self.ready_db() {
                Ok(db) => {
                    super::scenarios::scenario_delete(&db, scenario_path, include_archived).await
                }
                Err(r) => r,
            },

            // --- Roleplay templates (P4.6p) ---------------------------------
            Request::RoleplayTemplateList => match self.ready_db() {
                Ok(db) => super::roleplay_templates::roleplay_template_list(&db, SINGLE_USER_ID),
                Err(r) => r,
            },
            Request::RoleplayTemplateCreate { template } => match self.ready_db() {
                Ok(db) => {
                    super::roleplay_templates::roleplay_template_create(
                        &db,
                        SINGLE_USER_ID,
                        template,
                    )
                    .await
                }
                Err(r) => r,
            },
            Request::RoleplayTemplateGet { template_id } => match self.ready_db() {
                Ok(db) => super::roleplay_templates::roleplay_template_get(&db, &template_id),
                Err(r) => r,
            },
            Request::RoleplayTemplateUpdate {
                template_id,
                template,
            } => match self.ready_db() {
                Ok(db) => {
                    super::roleplay_templates::roleplay_template_update(
                        &db,
                        SINGLE_USER_ID,
                        &template_id,
                        template,
                    )
                    .await
                }
                Err(r) => r,
            },
            Request::RoleplayTemplateDelete { template_id } => match self.ready_db() {
                Ok(db) => {
                    super::roleplay_templates::roleplay_template_delete(&db, &template_id).await
                }
                Err(r) => r,
            },

            // --- Image profiles (P4.6p) -------------------------------------
            Request::ImageProfileList { sort_by_character } => match self.ready_db() {
                Ok(db) => super::image_profiles::image_profile_list(
                    &db,
                    SINGLE_USER_ID,
                    sort_by_character,
                ),
                Err(r) => r,
            },
            Request::ImageProfileCreate { profile } => match self.ready_db() {
                Ok(db) => {
                    super::image_profiles::image_profile_create(&db, SINGLE_USER_ID, profile).await
                }
                Err(r) => r,
            },
            Request::ImageProfileGet { profile_id } => match self.ready_db() {
                Ok(db) => super::image_profiles::image_profile_get(&db, &profile_id),
                Err(r) => r,
            },
            Request::ImageProfileUpdate {
                profile_id,
                profile,
            } => match self.ready_db() {
                Ok(db) => {
                    super::image_profiles::image_profile_update(
                        &db,
                        SINGLE_USER_ID,
                        &profile_id,
                        profile,
                    )
                    .await
                }
                Err(r) => r,
            },
            Request::ImageProfileDelete { profile_id } => match self.ready_db() {
                Ok(db) => super::image_profiles::image_profile_delete(&db, &profile_id).await,
                Err(r) => r,
            },
            Request::ImageProviderList => match self.ready_db() {
                Ok(_) => super::image_profiles::image_provider_list(),
                Err(r) => r,
            },
            // === P4.6ai: the imageProfileGenerate un-refusal — thread
            // prompt/chat_id/count into the W4.9a runner via the image-generation
            // seam. A spine-less assembly keeps the loud not-assembled refusal. ===
            Request::ImageProfileGenerate {
                image_profile_id,
                prompt,
                chat_id,
                count,
            } => match self.ready_generate_image() {
                Ok((db, runner)) => {
                    super::image_profiles::image_profile_generate(
                        &db,
                        &runner,
                        SINGLE_USER_ID,
                        &image_profile_id,
                        &prompt,
                        chat_id.as_deref(),
                        count,
                    )
                    .await
                }
                Err(r) => r,
            }, // === end P4.6ai ===
            Request::ImageProfileValidateKey { .. } => match self.ready_db() {
                Ok(_) => super::image_profiles::image_profile_validate_key(),
                Err(r) => r,
            },
            // === P4.D100: the honest Fetch Models un-refusal ===
            Request::ImageProfileListModels {
                provider,
                api_key_id,
            } => match self.ready_list_models() {
                Ok((db, discovery)) => {
                    super::image_profiles::image_profile_list_models(
                        &db,
                        &discovery,
                        provider.as_deref(),
                        api_key_id.as_deref(),
                    )
                    .await
                }
                Err(r) => r,
            }, // === end P4.D100 ===

            // --- Embedding profiles management (P4.9H2A) --------------------
            Request::EmbeddingProfileList => match self.ready_db() {
                Ok(db) => super::embedding_profiles::embedding_profile_list(&db, SINGLE_USER_ID),
                Err(r) => r,
            },
            Request::EmbeddingProfileGet { profile_id } => match self.ready_db() {
                Ok(db) => super::embedding_profiles::embedding_profile_get(&db, &profile_id),
                Err(r) => r,
            },
            Request::EmbeddingProfileCreate { body } => match self.ready_db() {
                Ok(db) => {
                    super::embedding_profiles::embedding_profile_create(&db, SINGLE_USER_ID, body)
                        .await
                }
                Err(r) => r,
            },
            Request::EmbeddingProfileUpdate { profile_id, body } => match self.ready_db() {
                Ok(db) => {
                    super::embedding_profiles::embedding_profile_update(
                        &db,
                        SINGLE_USER_ID,
                        &profile_id,
                        body,
                    )
                    .await
                }
                Err(r) => r,
            },
            Request::EmbeddingProfileDelete { profile_id } => match self.ready_db() {
                Ok(db) => {
                    super::embedding_profiles::embedding_profile_delete(&db, &profile_id).await
                }
                Err(r) => r,
            },
            Request::EmbeddingProfileRefit { profile_id } => match self.ready_db() {
                Ok(db) => {
                    super::embedding_profiles::embedding_profile_refit(
                        &db,
                        SINGLE_USER_ID,
                        &profile_id,
                    )
                    .await
                }
                Err(r) => r,
            },
            Request::EmbeddingProfileReindex { profile_id, scope } => match self.ready_db() {
                Ok(db) => {
                    super::embedding_profiles::embedding_profile_reindex(
                        &db,
                        SINGLE_USER_ID,
                        &profile_id,
                        scope.as_ref(),
                    )
                    .await
                }
                Err(r) => r,
            },
            Request::EmbeddingProfileReapply { profile_id } => match self.ready_db() {
                Ok(db) => {
                    super::embedding_profiles::embedding_profile_reapply(
                        &db,
                        SINGLE_USER_ID,
                        &profile_id,
                    )
                    .await
                }
                Err(r) => r,
            },
            Request::EmbeddingProfileListProviders => match self.ready_db() {
                Ok(_) => super::embedding_profiles::embedding_provider_list(),
                Err(r) => r,
            },
            Request::EmbeddingProfileListModels { provider } => match self.ready_db() {
                Ok(db) => {
                    super::embedding_profiles::embedding_profile_list_models(&db, provider).await
                }
                Err(r) => r,
            },
            Request::EmbeddingProfileFetchModels { .. } => match self.ready_db() {
                Ok(_) => super::embedding_profiles::embedding_profile_fetch_models(),
                Err(r) => r,
            },

            // --- Memory maintenance (P4.9H2A tier 2 — landed P4.43) ---------
            Request::MemoryDedupPreview { threshold } => match self.ready_db() {
                Ok(db) => {
                    super::memory_maintenance::memory_dedup(&db, SINGLE_USER_ID, threshold, true)
                        .await
                }
                Err(r) => r,
            },
            Request::MemoryDedupRun { threshold } => match self.ready_db() {
                Ok(db) => {
                    super::memory_maintenance::memory_dedup(&db, SINGLE_USER_ID, threshold, false)
                        .await
                }
                Err(r) => r,
            },
            Request::ConversationSummariesRegenerateStatus => match self.ready_db() {
                Ok(db) => {
                    super::memory_maintenance::conversation_summaries_regenerate(
                        &db,
                        SINGLE_USER_ID,
                        true,
                    )
                    .await
                }
                Err(r) => r,
            },
            Request::ConversationSummariesRegenerate => match self.ready_db() {
                Ok(db) => {
                    super::memory_maintenance::conversation_summaries_regenerate(
                        &db,
                        SINGLE_USER_ID,
                        false,
                    )
                    .await
                }
                Err(r) => r,
            },

            // --- Global mount points (P4.6p) --------------------------------
            Request::MountPointList => match self.ready_db() {
                Ok(db) => super::mount_points::mount_point_list(&db),
                Err(r) => r,
            },
            Request::MountPointGet { mount_point_id } => match self.ready_db() {
                Ok(db) => super::mount_points::mount_point_get(&db, &mount_point_id),
                Err(r) => r,
            },
            Request::MountPointCreate { mount_point } => match self.ready_db() {
                Ok(db) => super::mount_points::mount_point_create(&db, mount_point).await,
                Err(r) => r,
            },
            Request::MountPointUpdate {
                mount_point_id,
                mount_point,
            } => match self.ready_db() {
                Ok(db) => {
                    super::mount_points::mount_point_update(&db, &mount_point_id, mount_point).await
                }
                Err(r) => r,
            },
            Request::MountPointDelete { mount_point_id } => match self.ready_db() {
                Ok(db) => super::mount_points::mount_point_delete(&db, &mount_point_id).await,
                Err(r) => r,
            },

            // --- Projects family (P4.6k) ------------------------------------
            Request::ProjectList => match self.ready_db() {
                Ok(db) => super::projects::project_list(&db),
                Err(r) => r,
            },
            Request::ProjectCreate { project } => match self.ready_db() {
                Ok(db) => super::projects::project_create(&db, project).await,
                Err(r) => r,
            },
            Request::ProjectGet { project_id } => match self.ready_db() {
                Ok(db) => super::projects::project_get(&db, &project_id),
                Err(r) => r,
            },
            Request::ProjectUpdate {
                project_id,
                project,
            } => match self.ready_db() {
                Ok(db) => super::projects::project_update(&db, &project_id, project).await,
                Err(r) => r,
            },
            Request::ProjectDelete { project_id } => match self.ready_db() {
                Ok(db) => super::projects::project_delete(&db, &project_id).await,
                Err(r) => r,
            },
            Request::ProjectCharacterList { project_id } => match self.ready_db() {
                Ok(db) => super::projects::project_character_list(&db, &project_id),
                Err(r) => r,
            },
            Request::ProjectCharacterAdd {
                project_id,
                character_id,
            } => match self.ready_db() {
                Ok(db) => {
                    super::projects::project_character_add(&db, &project_id, &character_id).await
                }
                Err(r) => r,
            },
            Request::ProjectCharacterRemove {
                project_id,
                character_id,
            } => match self.ready_db() {
                Ok(db) => {
                    super::projects::project_character_remove(&db, &project_id, &character_id).await
                }
                Err(r) => r,
            },
            Request::ProjectChatList {
                project_id,
                limit,
                offset,
            } => match self.ready_db() {
                Ok(db) => super::projects::project_chat_list(&db, &project_id, limit, offset),
                Err(r) => r,
            },
            Request::ProjectChatAdd {
                project_id,
                chat_id,
            } => match self.ready_db() {
                Ok(db) => super::projects::project_chat_add(&db, &project_id, &chat_id).await,
                Err(r) => r,
            },
            Request::ProjectChatRemove {
                project_id,
                chat_id,
            } => match self.ready_db() {
                Ok(db) => super::projects::project_chat_remove(&db, &project_id, &chat_id).await,
                Err(r) => r,
            },
            Request::ProjectFileList { project_id } => match self.ready_db() {
                Ok(db) => super::projects::project_file_list(&db, &project_id),
                Err(r) => r,
            },
            Request::ProjectFileAdd {
                project_id,
                file_id,
            } => match self.ready_db() {
                Ok(db) => super::projects::project_file_add(&db, &project_id, &file_id).await,
                Err(r) => r,
            },
            Request::ProjectFileRemove {
                project_id,
                file_id,
            } => match self.ready_db() {
                Ok(db) => super::projects::project_file_remove(&db, &project_id, &file_id).await,
                Err(r) => r,
            },
            Request::ProjectStateGet { project_id } => match self.ready_db() {
                Ok(db) => super::projects::project_state_get(&db, &project_id),
                Err(r) => r,
            },
            Request::ProjectStateSet { project_id, state } => match self.ready_db() {
                Ok(db) => super::projects::project_state_set(&db, &project_id, state).await,
                Err(r) => r,
            },
            Request::ProjectStateReset { project_id } => match self.ready_db() {
                Ok(db) => super::projects::project_state_reset(&db, &project_id).await,
                Err(r) => r,
            },
            // --- The chat / group / general state tiers (P4.d10 §A) ---
            Request::ChatStateGet { chat_id } => match self.ready_db() {
                Ok(db) => super::salon::chat_state_get(&db, &chat_id).await,
                Err(r) => r,
            },
            Request::ChatStateSet { chat_id, state } => match self.ready_db() {
                Ok(db) => super::salon::chat_state_set(&db, &chat_id, state).await,
                Err(r) => r,
            },
            Request::ChatStateReset { chat_id } => match self.ready_db() {
                Ok(db) => super::salon::chat_state_reset(&db, &chat_id).await,
                Err(r) => r,
            },
            Request::GroupStateGet { group_id } => match self.ready_db() {
                Ok(db) => super::groups::group_state_get(&db, &group_id),
                Err(r) => r,
            },
            Request::GroupStateSet { group_id, state } => match self.ready_db() {
                Ok(db) => super::groups::group_state_set(&db, &group_id, state).await,
                Err(r) => r,
            },
            Request::GroupStateReset { group_id } => match self.ready_db() {
                Ok(db) => super::groups::group_state_reset(&db, &group_id).await,
                Err(r) => r,
            },
            Request::GeneralStateGet {} => match self.ready_db() {
                Ok(db) => super::settings::general_state_get(&db).await,
                Err(r) => r,
            },
            Request::GeneralStateSet { state } => match self.ready_db() {
                Ok(db) => super::settings::general_state_set(&db, state).await,
                Err(r) => r,
            },
            Request::GeneralStateReset {} => match self.ready_db() {
                Ok(db) => super::settings::general_state_reset(&db).await,
                Err(r) => r,
            },
            Request::ProjectBackgroundGet { project_id } => match self.ready_db() {
                Ok(db) => super::projects::project_background_get(&db, &project_id),
                Err(r) => r,
            },
            Request::ProjectAestheticGet { project_id, kind } => match self.ready_db() {
                Ok(db) => super::projects::project_aesthetic_get(&db, &project_id, &kind),
                Err(r) => r,
            },
            Request::ProjectAestheticSet {
                project_id,
                kind,
                content,
            } => match self.ready_db() {
                Ok(db) => {
                    super::projects::project_aesthetic_set(&db, &project_id, &kind, content).await
                }
                Err(r) => r,
            },
            Request::ProjectToolSettingsUpdate {
                project_id,
                default_disabled_tools,
                default_disabled_tool_groups,
            } => match self.ready_db() {
                Ok(db) => {
                    super::projects::project_tool_settings_update(
                        &db,
                        &project_id,
                        default_disabled_tools,
                        default_disabled_tool_groups,
                    )
                    .await
                }
                Err(r) => r,
            },
            Request::ProjectMountPointList { project_id } => match self.ready_db() {
                Ok(db) => super::projects::project_mount_point_list(&db, &project_id),
                Err(r) => r,
            },
            Request::ProjectMountPointLink {
                project_id,
                mount_point_id,
            } => match self.ready_db() {
                Ok(db) => {
                    super::projects::project_mount_point_link(&db, &project_id, &mount_point_id)
                        .await
                }
                Err(r) => r,
            },
            Request::ProjectMountPointUnlink {
                project_id,
                mount_point_id,
            } => match self.ready_db() {
                Ok(db) => {
                    super::projects::project_mount_point_unlink(&db, &project_id, &mount_point_id)
                        .await
                }
                Err(r) => r,
            },
            Request::ProjectScenarioList {
                project_id,
                include_archived,
            } => match self.ready_db() {
                Ok(db) => {
                    super::projects::project_scenario_list(&db, &project_id, include_archived).await
                }
                Err(r) => r,
            },
            Request::ProjectScenarioCreate {
                project_id,
                scenario,
            } => match self.ready_db() {
                Ok(db) => {
                    super::projects::project_scenario_create(&db, &project_id, scenario).await
                }
                Err(r) => r,
            },
            Request::ProjectScenarioGet {
                project_id,
                scenario_path,
            } => match self.ready_db() {
                Ok(db) => {
                    super::projects::project_scenario_get(&db, &project_id, &scenario_path).await
                }
                Err(r) => r,
            },
            Request::ProjectScenarioUpdate {
                project_id,
                scenario_path,
                scenario,
                include_archived,
            } => match self.ready_db() {
                Ok(db) => {
                    super::projects::project_scenario_update(
                        &db,
                        &project_id,
                        &scenario_path,
                        scenario,
                        include_archived,
                    )
                    .await
                }
                Err(r) => r,
            },
            Request::ProjectScenarioRename {
                project_id,
                scenario_path,
                new_filename,
                include_archived,
            } => match self.ready_db() {
                Ok(db) => {
                    super::projects::project_scenario_rename(
                        &db,
                        &project_id,
                        &scenario_path,
                        &new_filename,
                        include_archived,
                    )
                    .await
                }
                Err(r) => r,
            },
            Request::ProjectScenarioDelete {
                project_id,
                scenario_path,
                include_archived,
            } => match self.ready_db() {
                Ok(db) => {
                    super::projects::project_scenario_delete(
                        &db,
                        &project_id,
                        &scenario_path,
                        include_archived,
                    )
                    .await
                }
                Err(r) => r,
            },
            Request::ProjectWardrobeList {
                project_id,
                include_archived,
            } => match self.ready_db() {
                Ok(db) => {
                    super::projects::project_wardrobe_list(&db, &project_id, include_archived)
                }
                Err(r) => r,
            },
            Request::ProjectWardrobeInstructionsGet { project_id } => match self.ready_db() {
                Ok(db) => super::projects::project_wardrobe_instructions_get(&db, &project_id),
                Err(r) => r,
            },
            Request::ProjectWardrobeInstructionsSet {
                project_id,
                instructions,
            } => match self.ready_db() {
                Ok(db) => {
                    super::projects::project_wardrobe_instructions_set(
                        &db,
                        &project_id,
                        &instructions,
                    )
                    .await
                }
                Err(r) => r,
            },
            Request::ProjectWardrobeCreate { project_id, item } => match self.ready_db() {
                Ok(db) => super::projects::project_wardrobe_create(&db, &project_id, item).await,
                Err(r) => r,
            },
            Request::ProjectWardrobeGet {
                project_id,
                item_id,
            } => match self.ready_db() {
                Ok(db) => super::projects::project_wardrobe_get(&db, &project_id, &item_id),
                Err(r) => r,
            },
            Request::ProjectWardrobeUpdate {
                project_id,
                item_id,
                item,
            } => match self.ready_db() {
                Ok(db) => {
                    super::projects::project_wardrobe_update(&db, &project_id, &item_id, item).await
                }
                Err(r) => r,
            },
            Request::ProjectWardrobeDelete {
                project_id,
                item_id,
            } => match self.ready_db() {
                Ok(db) => {
                    super::projects::project_wardrobe_delete(&db, &project_id, &item_id).await
                }
                Err(r) => r,
            },
            // --- Memories (P4.6s) ---
            Request::MemoryList {
                character_id,
                search,
                min_importance,
                source,
                sort_by,
                sort_order,
                limit,
                offset,
            } => match self.ready_db() {
                Ok(db) => super::memories::memory_list(
                    &db,
                    &character_id,
                    search.as_deref(),
                    min_importance,
                    source.as_deref(),
                    sort_by.as_deref(),
                    sort_order.as_deref(),
                    limit,
                    offset,
                ),
                Err(r) => r,
            },
            Request::MemoryGet { memory_id } => match self.ready_db() {
                Ok(db) => super::memories::memory_get(&db, &memory_id).await,
                Err(r) => r,
            },
            Request::MemoryCountByChat { chat_id } => match self.ready_db() {
                Ok(db) => super::memories::memory_count_by_chat(&db, &chat_id),
                Err(r) => r,
            },
            Request::MemoryByMessage { message_id } => match self.ready_db() {
                Ok(db) => super::memories::memory_by_message(&db, SINGLE_USER_ID, &message_id),
                Err(r) => r,
            },
            Request::MemoryCharacterCounts => match self.ready_db() {
                Ok(db) => super::memories::memory_character_counts(&db, SINGLE_USER_ID),
                Err(r) => r,
            },
            Request::MemoryCreate { memory } => match self.ready_memory_embedding() {
                Ok((db, provider)) => {
                    super::memories::memory_create(&db, &provider, SINGLE_USER_ID, memory).await
                }
                Err(r) => r,
            },
            Request::MemoryUpdate { memory_id, memory } => match self.ready_db() {
                Ok(db) => super::memories::memory_update(&db, &memory_id, memory).await,
                Err(r) => r,
            },
            Request::MemoryDelete { memory_id } => match self.ready_db() {
                Ok(db) => super::memories::memory_delete(&db, &memory_id).await,
                Err(r) => r,
            },
            Request::MemoryDeleteByChat { chat_id } => match self.ready_db() {
                Ok(db) => super::memories::memory_delete_by_chat(&db, &chat_id).await,
                Err(r) => r,
            },
            Request::MemorySearch { search } => match self.ready_memory_embedding() {
                Ok((db, provider)) => {
                    super::memories::memory_search(
                        &db,
                        &provider,
                        SINGLE_USER_ID,
                        wall_clock_ms(),
                        search,
                    )
                    .await
                }
                Err(r) => r,
            },
            Request::MemoryHousekeepPreview {
                character_id,
                max_memories,
                max_age_months,
                min_importance,
                merge_similar,
            } => match self.ready_db() {
                Ok(db) => {
                    super::memories::memory_housekeep_preview(
                        &db,
                        &character_id,
                        max_memories,
                        max_age_months,
                        min_importance,
                        merge_similar,
                    )
                    .await
                }
                Err(r) => r,
            },
            Request::MemoryHousekeep { options } => match self.ready_db() {
                Ok(db) => super::memories::memory_housekeep(&db, options).await,
                Err(r) => r,
            },
            Request::MemoryHousekeepSweep => match self.ready_db() {
                Ok(db) => super::memories::memory_housekeep_sweep(&db, SINGLE_USER_ID).await,
                Err(r) => r,
            },
            Request::MemoryHousekeepingConfigGet => match self.ready_db() {
                Ok(db) => super::memories::memory_housekeeping_config_get(&db, SINGLE_USER_ID),
                Err(r) => r,
            },
            Request::MemoryHousekeepingConfigSet { config } => match self.ready_db() {
                Ok(db) => {
                    super::memories::memory_housekeeping_config_set(&db, SINGLE_USER_ID, config)
                        .await
                }
                Err(r) => r,
            },
            Request::MemoryRecallConfigGet => match self.ready_db() {
                Ok(db) => super::memories::memory_recall_config_get(&db),
                Err(r) => r,
            },
            Request::MemoryRecallConfigSet { config } => match self.ready_db() {
                Ok(db) => super::memories::memory_recall_config_set(&db, config).await,
                Err(r) => r,
            },
            Request::MemoryExtractionLimitsGet => match self.ready_db() {
                Ok(db) => super::memories::memory_extraction_limits_get(&db),
                Err(r) => r,
            },
            Request::MemoryExtractionLimitsSet { config } => match self.ready_db() {
                Ok(db) => super::memories::memory_extraction_limits_set(&db, config).await,
                Err(r) => r,
            },
            Request::MemoryExtractionConcurrencyGet => match self.ready_db() {
                Ok(db) => super::memories::memory_extraction_concurrency_get(&db),
                Err(r) => r,
            },
            Request::MemoryExtractionConcurrencySet { concurrency } => match self.ready_db() {
                Ok(db) => {
                    super::memories::memory_extraction_concurrency_set(
                        &db,
                        serde_json::json!({ "concurrency": concurrency }),
                    )
                    .await
                }
                Err(r) => r,
            },
            Request::MemoryBackfillProgress => match self.ready_db() {
                Ok(db) => super::memories::memory_backfill_progress(&db, SINGLE_USER_ID),
                Err(r) => r,
            },
            Request::MemoryRegenerateAllStatus => match self.ready_db() {
                Ok(db) => super::memories::memory_regenerate_all_status(&db, SINGLE_USER_ID),
                Err(r) => r,
            },
            Request::MemoryRegenerateAll => match self.ready_db() {
                Ok(db) => super::memories::memory_regenerate_all(&db, SINGLE_USER_ID).await,
                Err(r) => r,
            },
            Request::MemoryEmbeddingStatus { character_id } => match self.ready_db() {
                Ok(db) => {
                    super::memories::memory_embedding_status(&db, SINGLE_USER_ID, &character_id)
                }
                Err(r) => r,
            },
            Request::MemoryBackfillStart {
                character_id,
                batch_size,
            } => match self.ready_db() {
                Ok(db) => {
                    let mut bag = serde_json::Map::new();
                    if let Some(c) = character_id {
                        bag.insert("characterId".into(), serde_json::json!(c));
                    }
                    if let Some(b) = batch_size {
                        bag.insert("batchSize".into(), serde_json::json!(b));
                    }
                    super::memories::memory_backfill_start(
                        &db,
                        SINGLE_USER_ID,
                        serde_json::Value::Object(bag),
                    )
                    .await
                }
                Err(r) => r,
            },
            // === P4.6BL tier 2: the two repair arms, un-refused over the live
            // memory_embedding seam (the P4.6s refusals retire here). ===
            Request::MemoryGenerateEmbeddings {
                character_id,
                batch_size,
            } => match self.ready_memory_embedding() {
                Ok((db, provider)) => {
                    super::memories::memory_generate_embeddings(
                        &db,
                        &provider,
                        SINGLE_USER_ID,
                        &character_id,
                        batch_size,
                    )
                    .await
                }
                Err(r) => r,
            },
            Request::MemoryRebuildIndex {
                character_id,
                confirm,
            } => match self.ready_db() {
                Ok(db) => super::memories::memory_rebuild_index(&db, &character_id, confirm).await,
                Err(r) => r,
            },
            // === end P4.6BL tier 2 ===
            // === P4.6BM ===
            Request::ChatQueueMemories { chat_id } => match self.ready_db() {
                Ok(db) => super::memories::chat_queue_memories(&db, SINGLE_USER_ID, &chat_id).await,
                Err(r) => r,
            },
            // === end P4.6BM ===
            // === P4.6v: mount files (lane A, append-only) ===
            Request::MountFilesList { mount_point_id } => match self.ready_db() {
                Ok(db) => super::mount_files::mount_files_list(&db, &mount_point_id),
                Err(r) => r,
            },
            Request::MountFileRead {
                mount_point_id,
                path,
                encoding,
                offset,
                limit,
            } => match self.ready_db() {
                Ok(db) => super::mount_files::mount_file_read(
                    &db,
                    &mount_point_id,
                    &path,
                    encoding.as_deref(),
                    offset,
                    limit,
                ),
                Err(r) => r,
            },
            Request::MountScan { mount_point_id } => match self.ready_db() {
                Ok(db) => super::mount_files::mount_scan(&db, &mount_point_id).await,
                Err(r) => r,
            },
            Request::MountReindex {
                mount_point_id,
                path,
                force,
            } => match self.ready_db() {
                Ok(db) => {
                    super::mount_files::mount_reindex(&db, &mount_point_id, path, force).await
                }
                Err(r) => r,
            },
            Request::MountEmbed {
                mount_point_id,
                path,
                force,
            } => match self.ready_db() {
                Ok(db) => super::mount_files::mount_embed(&db, &mount_point_id, path, force).await,
                Err(r) => r,
            },
            Request::MountSemanticSearch { search } => match self.ready_memory_embedding() {
                Ok((db, provider)) => {
                    super::mount_files::mount_semantic_search(
                        &db,
                        &provider,
                        SINGLE_USER_ID,
                        search,
                    )
                    .await
                }
                Err(r) => r,
            },
            Request::MountFileMove {
                mount_point_id,
                source_path,
                dest_mount_point_id,
                dest_path,
            } => match self.ready_db() {
                Ok(db) => {
                    super::mount_files::mount_file_move(
                        &db,
                        &mount_point_id,
                        &source_path,
                        &dest_mount_point_id,
                        &dest_path,
                    )
                    .await
                }
                Err(r) => r,
            },
            Request::MountFileCopy {
                mount_point_id,
                source_path,
                dest_mount_point_id,
                dest_path,
                force,
            } => match self.ready_db() {
                Ok(db) => {
                    super::mount_files::mount_file_copy(
                        &db,
                        &mount_point_id,
                        &source_path,
                        &dest_mount_point_id,
                        &dest_path,
                        force,
                    )
                    .await
                }
                Err(r) => r,
            },
            Request::MountFileLink {
                mount_point_id,
                source_path,
                dest_mount_point_id,
                dest_path,
            } => match self.ready_db() {
                Ok(db) => {
                    super::mount_files::mount_file_link(
                        &db,
                        &mount_point_id,
                        &source_path,
                        &dest_mount_point_id,
                        &dest_path,
                    )
                    .await
                }
                Err(r) => r,
            },
            Request::MountFileDelete {
                mount_point_id,
                path,
            } => match self.ready_db() {
                Ok(db) => super::mount_files::mount_file_delete(&db, &mount_point_id, &path).await,
                Err(r) => r,
            },
            Request::MountFolderDelete {
                mount_point_id,
                path,
            } => match self.ready_db() {
                Ok(db) => {
                    super::mount_files::mount_folder_delete(&db, &mount_point_id, &path).await
                }
                Err(r) => r,
            },
            Request::MountFolderMove {
                mount_point_id,
                from_path,
                to_path,
            } => match self.ready_db() {
                Ok(db) => {
                    super::mount_files::mount_folder_move(
                        &db,
                        &mount_point_id,
                        &from_path,
                        &to_path,
                    )
                    .await
                }
                Err(r) => r,
            },
            Request::MountFileUpdate {
                mount_point_id,
                path,
                description,
                rename,
            } => match self.ready_db() {
                Ok(db) => {
                    super::mount_files::mount_file_update(
                        &db,
                        &mount_point_id,
                        &path,
                        description,
                        rename,
                    )
                    .await
                }
                Err(r) => r,
            },
            Request::MountFolderCreate {
                mount_point_id,
                path,
            } => match self.ready_db() {
                Ok(db) => {
                    super::mount_files::mount_folder_create(&db, &mount_point_id, &path).await
                }
                Err(r) => r,
            },
            Request::MountFileWrite {
                mount_point_id,
                path,
                content,
                encoding,
                expected_mtime,
                force,
                original_mime_type,
                original_file_name,
            } => match self.ready_db_and_blob_webp() {
                Ok((db, webp)) => {
                    super::mount_files::mount_file_write(
                        &db,
                        &mount_point_id,
                        &path,
                        &content,
                        encoding.as_deref(),
                        expected_mtime,
                        force,
                        original_mime_type,
                        original_file_name,
                        webp,
                    )
                    .await
                }
                Err(r) => r,
            },
            Request::MountFileWriteRaw {
                mount_point_id,
                path,
                data,
                force,
            } => match self.ready_db() {
                Ok(db) => {
                    super::mount_files::mount_file_write_raw(
                        &db,
                        &mount_point_id,
                        &path,
                        &data,
                        force,
                    )
                    .await
                }
                Err(r) => r,
            },
            Request::MountBlobUpload {
                mount_point_id,
                path,
                description,
                data,
                original_mime_type,
                original_file_name,
            } => match self.ready_db_and_blob_webp() {
                Ok((db, webp)) => {
                    super::mount_files::mount_blob_upload(
                        &db,
                        &mount_point_id,
                        &path,
                        description,
                        &data,
                        original_mime_type,
                        original_file_name,
                        webp,
                    )
                    .await
                }
                Err(r) => r,
            },
            Request::MountBlobsList {
                mount_point_id,
                folder,
            } => match self.ready_db() {
                Ok(db) => {
                    super::mount_files::mount_blobs_list(&db, &mount_point_id, folder.as_deref())
                }
                Err(r) => r,
            },
            Request::MountBlobDelete {
                mount_point_id,
                path,
            } => match self.ready_db() {
                Ok(db) => super::mount_files::mount_blob_delete(&db, &mount_point_id, &path).await,
                Err(r) => r,
            },
            Request::MountBlobUpdate {
                mount_point_id,
                path,
                description,
            } => match self.ready_db() {
                Ok(db) => {
                    super::mount_files::mount_blob_update(&db, &mount_point_id, &path, description)
                        .await
                }
                Err(r) => r,
            },
            Request::MountConvert { mount_point_id } => match self.ready_db() {
                Ok(db) => super::mount_files::mount_convert(&db, &mount_point_id).await,
                Err(r) => r,
            },
            Request::MountDeconvert {
                mount_point_id,
                target_path,
            } => match self.ready_db() {
                Ok(db) => {
                    super::mount_files::mount_deconvert(&db, &mount_point_id, &target_path).await
                }
                Err(r) => r,
            },
            // === P4.6w: documents ===
            Request::ChatActiveDocument { chat_id } => match self.ready_db() {
                Ok(db) => super::documents::chat_active_document(&db, &chat_id),
                Err(r) => r,
            },
            Request::ChatOpenDocuments { chat_id } => match self.ready_db() {
                Ok(db) => super::documents::chat_open_documents(&db, &chat_id),
                Err(r) => r,
            },
            Request::ChatRecentDocuments { chat_id } => match self.ready_db() {
                Ok(db) => super::documents::chat_recent_documents(&db, &chat_id),
                Err(r) => r,
            },
            Request::ChatAccessibleStores { chat_id, all } => match self.ready_db() {
                Ok(db) => super::documents::chat_accessible_stores(&db, &chat_id, all),
                Err(r) => r,
            },
            Request::ChatDocumentOpen { chat_id, body } => match self.ready_db() {
                Ok(db) => {
                    super::documents::chat_document_open(
                        &db,
                        &chat_id,
                        body,
                        Some(self.inner.config.base_dir.join("files")),
                    )
                    .await
                }
                Err(r) => r,
            },
            Request::ChatDocumentClose {
                chat_id,
                chat_document_id,
            } => match self.ready_db() {
                Ok(db) => {
                    super::documents::chat_document_close(&db, &chat_id, chat_document_id).await
                }
                Err(r) => r,
            },
            Request::ChatDocumentRead { chat_id, body } => match self.ready_db() {
                Ok(db) => super::documents::chat_document_read(
                    &db,
                    &chat_id,
                    body,
                    Some(self.inner.config.base_dir.join("files")),
                ),
                Err(r) => r,
            },
            Request::ChatDocumentResolve { chat_id, body } => match self.ready_db() {
                Ok(db) => super::documents::chat_document_resolve(
                    &db,
                    &chat_id,
                    body,
                    Some(self.inner.config.base_dir.join("files")),
                ),
                Err(r) => r,
            },
            Request::ChatDocumentWrite { chat_id, body } => match self.ready_db_and_refresh() {
                Ok((db, refresh)) => {
                    super::documents::chat_document_write(
                        &db,
                        &chat_id,
                        body,
                        refresh,
                        Some(self.inner.config.base_dir.join("files")),
                    )
                    .await
                }
                Err(r) => r,
            },
            Request::ChatDocumentRename { chat_id, body } => match self.ready_db_and_refresh() {
                Ok((db, refresh)) => {
                    super::documents::chat_document_rename(
                        &db,
                        &chat_id,
                        body,
                        refresh,
                        Some(self.inner.config.base_dir.join("files")),
                    )
                    .await
                }
                Err(r) => r,
            },
            Request::ChatDocumentDelete {
                chat_id,
                chat_document_id,
            } => match self.ready_db() {
                Ok(db) => {
                    super::documents::chat_document_delete(
                        &db,
                        &chat_id,
                        chat_document_id,
                        Some(self.inner.config.base_dir.join("files")),
                    )
                    .await
                }
                Err(r) => r,
            },
            Request::DocumentStores => match self.ready_db() {
                Ok(db) => super::documents::document_stores(&db),
                Err(r) => r,
            },
            Request::DocumentsRecent => match self.ready_db() {
                Ok(db) => super::documents::documents_recent(&db),
                Err(r) => r,
            },
            Request::DocumentOpen { body } => match self.ready_db() {
                Ok(db) => {
                    super::documents::document_open(
                        &db,
                        body,
                        Some(self.inner.config.base_dir.join("files")),
                    )
                    .await
                }
                Err(r) => r,
            },
            Request::DocumentRead { body } => match self.ready_db() {
                Ok(db) => super::documents::document_read(
                    &db,
                    body,
                    Some(self.inner.config.base_dir.join("files")),
                ),
                Err(r) => r,
            },
            Request::DocumentWrite { body } => match self.ready_db_and_refresh() {
                Ok((db, refresh)) => {
                    super::documents::document_write(
                        &db,
                        body,
                        refresh,
                        Some(self.inner.config.base_dir.join("files")),
                    )
                    .await
                }
                Err(r) => r,
            },
            Request::DocumentRename { body } => match self.ready_db_and_refresh() {
                Ok((db, refresh)) => {
                    super::documents::document_rename(
                        &db,
                        body,
                        refresh,
                        Some(self.inner.config.base_dir.join("files")),
                    )
                    .await
                }
                Err(r) => r,
            },
            Request::DocumentDelete { body } => match self.ready_db() {
                Ok(db) => {
                    super::documents::document_delete(
                        &db,
                        body,
                        Some(self.inner.config.base_dir.join("files")),
                    )
                    .await
                }
                Err(r) => r,
            },
            // === end P4.6w ===
            // === P4.6z: system ===
            // The DirectoryPicker's host-fs browser. No DB access (v4's
            // `createContextHandler` never touches the vault), so no readiness gate.
            Request::SystemBrowseDirectory { path } => {
                super::system::browse_directory(path.as_deref())
            } // === end P4.6z ===
            // === P4.6au: the home dashboard ===
            // v4's route passes `fallbackName: ctx.user.name ?? null` (the
            // session mirror of the users row); v5 single-user has no session
            // name apart from that row, so the composition root passes None
            // and the users-row name carries the greeting.
            Request::SystemHome => match self.ready_db() {
                Ok(db) => super::system::system_home(&db, SINGLE_USER_ID, None),
                Err(r) => r,
            },
            // === P4.37: The Almanack ===
            Request::SystemAlmanackGenerate { progress_id } => {
                match (self.ready_db(), self.ready_almanack_host()) {
                    (Ok(db), Ok(host)) => {
                        let progress =
                            crate::services::creation_progress::CreationProgressEmitter::from_id(
                                progress_id.as_deref(),
                                self.creation_progress_bus(),
                                self.inner.events.clone(),
                            );
                        let ctx = crate::almanack::AlmanackContext {
                            db: &db,
                            registry: crate::provider_manifest::Registry::built_in(),
                            user_id: SINGLE_USER_ID,
                            paths: host.paths(),
                            facts: host.runtime_facts(),
                            passphrase_protected: host.passphrase_protected(),
                            version: host.app_version(),
                            node_env: host.node_env(),
                            now_ms: host.now_ms(),
                        };
                        super::almanack::almanack_generate(&ctx, &progress).await
                    }
                    (Err(r), _) | (_, Err(r)) => r,
                }
            }
            Request::SystemAlmanackList => match self.ready_db() {
                Ok(db) => super::almanack::almanack_list(&db, SINGLE_USER_ID),
                Err(r) => r,
            },
            Request::SystemAlmanackGet { report_id } => {
                match (self.ready_db(), self.ready_almanack_host()) {
                    (Ok(db), Ok(host)) => super::almanack::almanack_get(
                        &db,
                        host.storage().as_ref(),
                        SINGLE_USER_ID,
                        &report_id,
                    ),
                    (Err(r), _) | (_, Err(r)) => r,
                }
            }
            Request::SystemAlmanackDelete { report_id } => {
                match (self.ready_db(), self.ready_almanack_host()) {
                    (Ok(db), Ok(host)) => {
                        super::almanack::almanack_delete(
                            &db,
                            host.storage().as_ref(),
                            SINGLE_USER_ID,
                            &report_id,
                        )
                        .await
                    }
                    (Err(r), _) | (_, Err(r)) => r,
                }
            }
            // === end P4.37 ===
            // === end P4.6au ===
            // === P4.6ab: courier + chat images ===
            Request::MessageResolveExternalTurn {
                chat_id,
                message_id,
                reply_content,
            } => match self.ready_courier_resolve() {
                Ok(driver) => {
                    driver
                        .resolve_external_turn(chat_id, message_id, reply_content)
                        .await
                }
                Err(r) => r,
            },
            Request::MessageCancelExternalTurn {
                chat_id,
                message_id,
            } => match self.ready_db() {
                Ok(db) => {
                    super::chat_media::message_cancel_external_turn(&db, &chat_id, &message_id)
                        .await
                }
                Err(r) => r,
            },
            Request::MessageSaveImage {
                chat_id,
                message_id,
                body,
            } => match self.ready_save_image() {
                Ok((db, bytes)) => {
                    super::chat_media::message_save_image(
                        &db,
                        SINGLE_USER_ID,
                        &chat_id,
                        &message_id,
                        &body,
                        bytes,
                        Arc::new(crate::photos::save_image_to_album::NoSideEffects)
                            as Arc<
                                dyn crate::photos::save_image_to_album::SaveImageSideEffects
                                    + Send
                                    + Sync,
                            >,
                        &crate::clock::now_iso(),
                    )
                    .await
                }
                Err(r) => r,
            },
            Request::ChatPhotoAlbums { chat_id } => match self.ready_db() {
                Ok(db) => super::chat_media::chat_photo_albums(&db, &chat_id),
                Err(r) => r,
            },
            // P4.9E3B (the tier-2 audit port — see types.rs).
            Request::ChatGroupStores { chat_id } => match self.ready_db() {
                Ok(db) => super::chat_media::chat_group_stores(&db, &chat_id),
                Err(r) => r,
            },
            // === P4.9E4A ===
            Request::ChatAttachMountFile {
                chat_id,
                mount_point_id,
                relative_path,
            } => match self.ready_db_and_image_describe() {
                Ok((db, describe)) => {
                    super::chat_media::chat_attach_mount_file(
                        &db,
                        describe.as_ref(),
                        &chat_id,
                        &mount_point_id,
                        &relative_path,
                    )
                    .await
                }
                Err(r) => r,
            },
            // === end P4.9E4A ===
            // === P4.9P: the global-search endpoint ===
            Request::UiSearch {
                q,
                types,
                limit,
                offset,
            } => match self.ready_db() {
                Ok(db) => super::ui_search::ui_search(
                    &db,
                    SINGLE_USER_ID,
                    &super::ui_search::UiSearchParams {
                        q: q.as_deref(),
                        types: types.as_deref(),
                        limit: limit.as_deref(),
                        offset: offset.as_deref(),
                    },
                ),
                Err(r) => r,
            },
            // === end P4.9P ===
            Request::ChatAddToolResult { chat_id, body } => match self.ready_db() {
                Ok(db) => {
                    super::chat_media::chat_add_tool_result(&db, SINGLE_USER_ID, &chat_id, &body)
                        .await
                }
                Err(r) => r,
            },
            Request::ChatFilesList { chat_id } => match self.ready_db() {
                Ok(db) => super::chat_media::chat_files_list(&db, &chat_id),
                Err(r) => r,
            },
            Request::ChatFileDelete { file_id } => match self.ready_db() {
                Ok(db) => super::chat_media::chat_file_delete(&db, SINGLE_USER_ID, &file_id).await,
                Err(r) => r,
            },
            Request::ChatFileUpload {
                chat_id,
                filename,
                content_type,
                data,
                resolution,
                conflicting_file_id,
            } => match self.ready_db() {
                Ok(db) => {
                    super::chat_media::chat_file_upload(
                        &db,
                        SINGLE_USER_ID,
                        &chat_id,
                        super::chat_media::ChatFileUploadInput {
                            filename,
                            content_type,
                            data,
                            resolution,
                            conflicting_file_id,
                        },
                    )
                    .await
                }
                Err(r) => r,
            }, // === end P4.6ab ===
            // === P4.6ad: autonomous rooms (lane C, append-only) ===
            Request::SystemAutonomousRooms => match self.ready_db() {
                Ok(db) => super::autonomous_rooms::system_autonomous_rooms(&db, SINGLE_USER_ID),
                Err(resp) => resp,
            },
            Request::ChatAutonomousRoomStatus { chat_id } => match self.ready_db() {
                Ok(db) => super::autonomous_rooms::autonomous_room_status(&db, &chat_id),
                Err(resp) => resp,
            },
            Request::ChatAutonomousRoomStart { chat_id } => match self.ready_db() {
                Ok(db) => {
                    super::autonomous_rooms::autonomous_room_start(&db, SINGLE_USER_ID, &chat_id)
                        .await
                }
                Err(resp) => resp,
            },
            Request::ChatAutonomousRoomPause { chat_id } => match self.ready_db() {
                Ok(db) => super::autonomous_rooms::autonomous_room_pause(&db, &chat_id).await,
                Err(resp) => resp,
            },
            Request::ChatAutonomousRoomStop { chat_id } => match self.ready_db() {
                Ok(db) => super::autonomous_rooms::autonomous_room_stop(&db, &chat_id).await,
                Err(resp) => resp,
            },
            Request::ChatAutonomousRoomResume { chat_id } => match self.ready_db() {
                Ok(db) => {
                    super::autonomous_rooms::autonomous_room_resume(&db, SINGLE_USER_ID, &chat_id)
                        .await
                }
                Err(resp) => resp,
            },
            Request::ChatAutonomousRoomUpdateSettings { chat_id, settings } => {
                match self.ready_db() {
                    Ok(db) => {
                        super::autonomous_rooms::autonomous_room_update_settings(
                            &db,
                            SINGLE_USER_ID,
                            &chat_id,
                            &settings,
                        )
                        .await
                    }
                    Err(resp) => resp,
                }
            } // === end P4.6ad ===
            // === P4.d3 === (data-retention setting — the db-size-reduction drift)
            Request::DataRetentionSettings => match self.ready_db() {
                Ok(db) => super::settings::data_retention_settings_get(&db),
                Err(resp) => resp,
            },
            // P4.57: the decoded tri-state passes STRAIGHT through — an absent
            // key is v4's partial body (the merge keeps the stored value), a
            // present one (including an explicit `null`, which is what v4 400s
            // on) rides raw so the handler's Zod-faithful parse decides. This arm
            // used to rebuild a `bag` and the handler to re-read it, so one wire
            // decode was followed by two more derivations of the same three-way
            // split.
            Request::DataRetentionSettingsUpdate { stale_chat_days } => match self.ready_db() {
                Ok(db) => {
                    super::settings::data_retention_settings_update(&db, stale_chat_days).await
                }
                Err(resp) => resp,
            },
            // === end P4.d3 ===
            // === P4.D50 === (the instance-wide Taboo list)
            Request::TabooSettings => match self.ready_db() {
                Ok(db) => super::settings::taboo_settings_get(&db),
                Err(resp) => resp,
            },
            // P4.57: the decoded tri-state passes STRAIGHT through (see the
            // data-retention arm above) — an absent key is v4's partial body (the
            // merge keeps the stored list), a present one rides raw.
            Request::TabooSettingsUpdate { phrases } => match self.ready_db() {
                Ok(db) => super::settings::taboo_settings_update(&db, phrases).await,
                Err(resp) => resp,
            },
            // === end P4.D50 ===
            // === P4.D57 === (the instance-wide Brahma Console turn budget)
            Request::BrahmaConsoleSettings => match self.ready_db() {
                Ok(db) => super::settings::brahma_console_settings_get(&db),
                Err(resp) => resp,
            },
            // P4.57: the decoded tri-state passes STRAIGHT through (see the
            // data-retention arm above).
            Request::BrahmaConsoleSettingsUpdate { max_agent_turns } => match self.ready_db() {
                Ok(db) => {
                    super::settings::brahma_console_settings_update(&db, max_agent_turns).await
                }
                Err(resp) => resp,
            },
            // === end P4.D57 ===
            // === P4.6ae === (the general files family — lane A, append-only)
            Request::FilesList {
                project_id,
                folder_path,
                filter,
                category,
            } => match self.ready_db() {
                Ok(db) => super::files::files_list(
                    &db,
                    SINGLE_USER_ID,
                    project_id.as_deref(),
                    folder_path.as_deref(),
                    filter.as_deref(),
                    category.as_deref(),
                ),
                Err(resp) => resp,
            },
            Request::FileMove {
                file_id,
                folder_path,
                filename,
                project_id,
            } => match self.ready_db() {
                Ok(db) => {
                    // Decode the double-option into the handler's tri-state.
                    let pid = match project_id {
                        None => super::files::MovePid::Keep,
                        Some(None) => super::files::MovePid::Clear,
                        Some(Some(p)) => super::files::MovePid::Set(p),
                    };
                    super::files::file_move(
                        &db,
                        SINGLE_USER_ID,
                        &file_id,
                        folder_path,
                        filename,
                        pid,
                    )
                    .await
                }
                Err(resp) => resp,
            },
            Request::FilePromote {
                file_id,
                target_project_id,
                folder_path,
            } => match self.ready_db() {
                Ok(db) => {
                    super::files::file_promote(
                        &db,
                        SINGLE_USER_ID,
                        &file_id,
                        target_project_id,
                        folder_path,
                    )
                    .await
                }
                Err(resp) => resp,
            },
            Request::FileDelete {
                file_id,
                force,
                dissociate,
            } => match self.ready_db() {
                Ok(db) => {
                    // The backup host's disk backend (LocalStorageBackend over
                    // `<base>/files`) carries the eager per-delete thumbnail
                    // cleanup (P4.44). `None` on a host with no disk layer — no
                    // thumbnails to reap.
                    let storage = self.qtap_file_storage();
                    super::files::file_delete(
                        &db,
                        SINGLE_USER_ID,
                        &file_id,
                        force,
                        dissociate,
                        storage.as_deref(),
                    )
                    .await
                }
                Err(resp) => resp,
            },
            Request::FilesFoldersList { project_id } => match self.ready_db() {
                Ok(db) => {
                    super::files::files_folders_list(&db, SINGLE_USER_ID, project_id.as_deref())
                }
                Err(resp) => resp,
            },
            Request::FilesFolderCreate { path, project_id } => match self.ready_db() {
                Ok(db) => {
                    super::files::files_folder_create(
                        &db,
                        SINGLE_USER_ID,
                        &path,
                        project_id.as_deref(),
                    )
                    .await
                }
                Err(resp) => resp,
            },
            Request::FilesFolderRename {
                path,
                new_name,
                project_id,
            } => match self.ready_db() {
                Ok(db) => {
                    super::files::files_folder_rename(
                        &db,
                        SINGLE_USER_ID,
                        &path,
                        &new_name,
                        project_id.as_deref(),
                    )
                    .await
                }
                Err(resp) => resp,
            },
            Request::FilesFolderDelete { path, project_id } => match self.ready_db() {
                Ok(db) => {
                    super::files::files_folder_delete(
                        &db,
                        SINGLE_USER_ID,
                        &path,
                        project_id.as_deref(),
                    )
                    .await
                }
                Err(resp) => resp,
            },
            Request::FilesSync => match self.ready_db() {
                Ok(_db) => super::files::files_sync(),
                Err(resp) => resp,
            },
            // === end P4.6ae ===
            // === P4.6ah === (files write + maintenance — lane A, append-only)
            Request::FileUpload {
                filename,
                content_type,
                data,
                tags,
                project_id,
                folder_path,
            } => match self.ready_db() {
                Ok(db) => {
                    use base64::Engine;
                    match base64::engine::general_purpose::STANDARD.decode(data.as_bytes()) {
                        Ok(bytes) => {
                            // The disk backend carries the eager per-overwrite
                            // thumbnail cleanup (P4.44); `None` on a diskless host.
                            let storage = self.qtap_file_storage();
                            super::files::file_upload(
                                &db,
                                SINGLE_USER_ID,
                                &filename,
                                &content_type,
                                bytes,
                                tags.unwrap_or_default(),
                                project_id,
                                folder_path,
                                storage.as_deref(),
                            )
                            .await
                        }
                        Err(_) => super::types::Response::error(
                            super::types::ErrorKind::BadRequest,
                            "Invalid base64 file data",
                        ),
                    }
                }
                Err(resp) => resp,
            },
            Request::ChatFileLink { chat_id, file_id } => match self.ready_db() {
                Ok(db) => {
                    super::chat_media::chat_file_link(&db, SINGLE_USER_ID, &chat_id, &file_id).await
                }
                Err(resp) => resp,
            },
            Request::FilesGenerateThumbnails { file_ids, size: _ } => match self.ready_db() {
                Ok(db) => super::files::files_generate_thumbnails(&db, SINGLE_USER_ID, &file_ids),
                Err(resp) => resp,
            },
            Request::FilesCleanupStale { dry_run } => match self.ready_db() {
                Ok(db) => {
                    // The disk-key existence leg needs a host storage backend, not
                    // wired at the dispatch layer (enumerated) → NotConfigured makes
                    // disk keys a per-file error; mount-blob keys are checked in-DB.
                    let backend = crate::services::file_storage::NotConfiguredStorageBackend;
                    super::files::files_cleanup_stale(
                        &db,
                        &backend,
                        SINGLE_USER_ID,
                        dry_run.unwrap_or(true),
                    )
                    .await
                }
                Err(resp) => resp,
            },
            Request::FilesCleanupOrphans { mode, dry_run } => match self.ready_db() {
                Ok(db) => {
                    super::files::files_cleanup_orphans(
                        &db,
                        SINGLE_USER_ID,
                        &mode,
                        dry_run.unwrap_or(true),
                    )
                    .await
                }
                Err(resp) => resp,
            },
            // === end P4.6ah ===
            // === P4.6ak: text-replacements + get-background (lane A) ===
            Request::TextReplacementsList => match self.ready_db() {
                Ok(db) => super::text_replacements::list(&db),
                Err(resp) => resp,
            },
            Request::TextReplacementCreate { body } => match self.ready_db() {
                Ok(db) => super::text_replacements::create(&db, &body).await,
                Err(resp) => resp,
            },
            Request::TextReplacementUpdate { id, body } => match self.ready_db() {
                Ok(db) => super::text_replacements::update(&db, &id, &body).await,
                Err(resp) => resp,
            },
            Request::TextReplacementDelete { id } => match self.ready_db() {
                Ok(db) => super::text_replacements::delete(&db, &id).await,
                Err(resp) => resp,
            },
            Request::TextReplacementsBulkReplace { body } => match self.ready_db() {
                Ok(db) => super::text_replacements::bulk_replace(&db, &body).await,
                Err(resp) => resp,
            },
            Request::ChatGetBackground { chat_id } => match self.ready_db() {
                Ok(db) => super::chat_media::chat_get_background(&db, &chat_id),
                Err(resp) => resp,
            },
            // P4.6ao un-refused this: the edge validates + enqueues, and the
            // already-registered STORY_BACKGROUND_GENERATION job does the work.
            Request::ChatRegenerateBackground { chat_id } => match self.ready_db() {
                Ok(db) => {
                    super::chat_media::chat_regenerate_background(&db, SINGLE_USER_ID, &chat_id)
                        .await
                }
                Err(resp) => resp,
            },
            // === end P4.6ak ===
            // === P4.6ao ===
            Request::ChatGetCost { chat_id, detailed } => match self.ready_db() {
                Ok(db) => {
                    super::chat_media::chat_get_cost(&db, &chat_id, detailed.unwrap_or(false))
                }
                Err(resp) => resp,
            },
            // === end P4.6ao ===
            // === P4.6ay: Pascal's custom-tools route ===
            Request::ChatCustomToolsList { chat_id } => match self.ready_db() {
                Ok(db) => {
                    super::custom_tools::chat_custom_tools_list(&db, SINGLE_USER_ID, &chat_id)
                }
                Err(r) => r,
            },
            Request::ChatCustomToolRun {
                chat_id,
                tool,
                parameters,
                private,
                as_character_id,
            } => match self.ready_db_and_consult() {
                Ok((db, consult)) => {
                    super::custom_tools::chat_custom_tool_run(
                        &db,
                        SINGLE_USER_ID,
                        &chat_id,
                        &tool,
                        parameters,
                        private,
                        as_character_id,
                        // The assembled consult seam (P4.6bd) — a composer-run
                        // custom tool with an `llm` block consults for real.
                        consult.as_deref(),
                    )
                    .await
                }
                Err(r) => r,
            },
            // --- unit 12: Pascal's Workbench collection resource ---
            Request::CustomToolsLibrary => match self.ready_db() {
                Ok(db) => super::custom_tools::custom_tools_library(&db),
                Err(r) => r,
            },
            Request::CustomToolsDestinations => match self.ready_db() {
                Ok(db) => super::custom_tools::custom_tools_destinations(&db),
                Err(r) => r,
            },
            Request::CustomToolPreview {
                definition,
                params,
                private,
                metadata,
                state,
                llm,
            } => match self.ready_db_and_consult() {
                Ok((db, consult)) => {
                    super::custom_tools::custom_tool_preview(
                        &db,
                        &definition,
                        params.as_ref(),
                        private,
                        metadata.as_ref(),
                        state.as_ref(),
                        llm.as_ref(),
                        SINGLE_USER_ID,
                        // The assembled consult seam (P4.6bd) — the `{live:true}`
                        // bench arm consults for real. The scripted and fail
                        // arms never touch it.
                        consult.as_deref(),
                    )
                    .await
                }
                Err(r) => r,
            },
            Request::CustomToolAudit {
                definition,
                params,
                metadata,
                state,
                llm,
            } => match self.ready_db() {
                Ok(db) => super::custom_tools::custom_tool_audit(
                    &db,
                    &definition,
                    params.as_ref(),
                    metadata.as_ref(),
                    state.as_ref(),
                    llm.as_ref(),
                ),
                Err(r) => r,
            },
            // === end P4.6ay ===
            // === P4.6ar: the llm-logs read surface + the system aesthetics pair ===
            Request::LlmLogsList {
                message_id,
                chat_id,
                character_id,
                log_type,
                standalone,
                include_messages,
                limit,
                offset,
            } => match self.ready_db() {
                Ok(db) => super::llm_logs::llm_logs_list(
                    &db,
                    SINGLE_USER_ID,
                    &super::llm_logs::LlmLogsListParams {
                        message_id: message_id.as_deref(),
                        chat_id: chat_id.as_deref(),
                        character_id: character_id.as_deref(),
                        log_type: log_type.as_deref(),
                        standalone,
                        include_messages,
                        limit: limit.as_deref(),
                        offset: offset.as_deref(),
                    },
                ),
                Err(resp) => resp,
            },
            Request::LlmLogGet { id } => match self.ready_db() {
                Ok(db) => super::llm_logs::llm_log_get(&db, &id),
                Err(resp) => resp,
            },
            Request::LlmLogDelete { id } => match self.ready_db() {
                Ok(db) => super::llm_logs::llm_log_delete(&db, &id).await,
                Err(resp) => resp,
            },
            Request::SystemImageAestheticsGet { kind } => match self.ready_db() {
                Ok(db) => super::system::system_image_aesthetics_get(&db, &kind),
                Err(resp) => resp,
            },
            Request::SystemImageAestheticsSet { kind, content } => match self.ready_db() {
                Ok(db) => super::system::system_image_aesthetics_set(&db, &kind, content).await,
                Err(resp) => resp,
            },
            // === end P4.6ar ===
            // === P4.9c: the user-profile + data-dir surface (lane C, append-only) ===
            Request::UserProfileGet => match self.ready_db() {
                Ok(db) => super::user_profile::user_profile_get(&db, SINGLE_USER_ID),
                Err(resp) => resp,
            },
            Request::UserProfileUpdate { name, email, image } => match self.ready_db() {
                Ok(db) => {
                    super::user_profile::user_profile_update(
                        &db,
                        SINGLE_USER_ID,
                        name,
                        email,
                        image,
                        &crate::clock::now_iso(),
                    )
                    .await
                }
                Err(resp) => resp,
            },
            Request::UserProfileSetAvatar { image_id } => match self.ready_db() {
                Ok(db) => {
                    super::user_profile::user_profile_set_avatar(
                        &db,
                        SINGLE_USER_ID,
                        image_id,
                        &crate::clock::now_iso(),
                    )
                    .await
                }
                Err(resp) => resp,
            },
            // The data dir is environment + config, never the vault — v4's route
            // is a `createContextHandler` with no unlock requirement, so this
            // arm deliberately does NOT go through `ready_db`.
            Request::SystemDataDir => super::data_dir::system_data_dir(&self.inner.config.base_dir), // === end P4.9c ===
            // === P4.9a: the user photo gallery (lane A, append-only) ===
            // The embedding provider is OPTIONAL here, deliberately. v4 calls
            // `generateEmbeddingForUser` only on the query branch, so gating the
            // whole verb on the seam would make a plain `/photos` listing fail
            // on any spine-less assembly (read-only embedders get `None`) —
            // stricter than v4, and it would dark-screen the whole feature. A
            // SEARCH without the seam is the loud refusal; a listing is not.
            Request::PhotoGalleryList {
                q,
                tag,
                limit,
                offset,
            } => match self.ready_db_and_memory_embedding() {
                Ok((db, provider)) => {
                    super::photos::photo_gallery_list(
                        &db,
                        provider.as_ref(),
                        SINGLE_USER_ID,
                        q,
                        tag,
                        limit,
                        offset,
                    )
                    .await
                }
                Err(r) => r,
            },
            Request::PhotoGallerySave {
                file_id,
                caption,
                tags,
                chat_id,
            } => match self.ready_save_image() {
                Ok((db, bytes)) => {
                    super::photos::photo_gallery_save(
                        &db,
                        bytes,
                        SINGLE_USER_ID,
                        file_id,
                        caption,
                        tags,
                        chat_id,
                        &crate::clock::now_iso(),
                    )
                    .await
                }
                Err(r) => r,
            },
            Request::PhotoGalleryEntryGet { id } => match self.ready_db() {
                Ok(db) => super::photos::photo_gallery_entry_get(&db, id),
                Err(r) => r,
            },
            Request::PhotoGalleryEntryRemove { id } => match self.ready_db() {
                Ok(db) => super::photos::photo_gallery_entry_remove(&db, id).await,
                Err(r) => r,
            },
            Request::ImageInfoGet { id } => match self.ready_db() {
                Ok(db) => super::photos::image_info_get(&db, SINGLE_USER_ID, &id),
                Err(r) => r,
            },
            // === end P4.9a ===
            // === P4.9f1: the wardrobe server surface (lane F1, append-only) ===
            Request::ChatOutfitGet { chat_id } => match self.ready_db() {
                Ok(db) => super::chat_outfits::chat_outfit_get(&db, &chat_id),
                Err(r) => r,
            },
            Request::ChatEquip { chat_id, body } => match self.ready_db() {
                Ok(db) => {
                    super::chat_outfits::chat_equip(&db, SINGLE_USER_ID, &chat_id, body).await
                }
                Err(r) => r,
            },
            Request::ChatRegenerateAvatar { chat_id, body } => match self.ready_db() {
                Ok(db) => {
                    super::chat_outfits::chat_regenerate_avatar(&db, SINGLE_USER_ID, &chat_id, body)
                        .await
                }
                Err(r) => r,
            },
            Request::WardrobeTransferDestinations => match self.ready_db() {
                Ok(db) => super::wardrobe::wardrobe_transfer_destinations(&db, SINGLE_USER_ID),
                Err(r) => r,
            },
            Request::WardrobeTransferApply { body } => match self.ready_db() {
                Ok(db) => {
                    super::wardrobe::wardrobe_transfer_apply(
                        &db,
                        SINGLE_USER_ID,
                        body,
                        &crate::clock::now_iso(),
                    )
                    .await
                }
                Err(r) => r,
            },
            Request::WardrobeList { include_archived } => match self.ready_db() {
                Ok(db) => super::wardrobe::wardrobe_list(&db, include_archived),
                Err(r) => r,
            },
            Request::WardrobeInstructionsGet => match self.ready_db() {
                Ok(db) => super::wardrobe::wardrobe_instructions_get(&db),
                Err(r) => r,
            },
            Request::WardrobeInstructionsSet { instructions } => match self.ready_db() {
                Ok(db) => super::wardrobe::wardrobe_instructions_set(&db, &instructions).await,
                Err(r) => r,
            },
            Request::WardrobeCreate { item } => match self.ready_db() {
                Ok(db) => super::wardrobe::wardrobe_create(&db, item).await,
                Err(r) => r,
            },
            Request::WardrobeItemGet { item_id } => match self.ready_db() {
                Ok(db) => super::wardrobe::wardrobe_item_get(&db, &item_id),
                Err(r) => r,
            },
            Request::WardrobeUpdate { item_id, item } => match self.ready_db() {
                Ok(db) => super::wardrobe::wardrobe_update(&db, &item_id, item).await,
                Err(r) => r,
            },
            Request::WardrobeDelete { item_id } => match self.ready_db() {
                Ok(db) => super::wardrobe::wardrobe_delete(&db, &item_id).await,
                Err(r) => r,
            },
            Request::WardrobePreviewAvatar { body } => match self.ready_avatar_preview() {
                Ok((db, renderer)) => {
                    super::wardrobe::wardrobe_preview_avatar(
                        &db,
                        &renderer,
                        SINGLE_USER_ID,
                        body,
                        &crate::clock::now_iso(),
                    )
                    .await
                }
                Err(r) => r,
            },
            Request::WardrobeAnalyzeImage { .. } => super::wardrobe::wardrobe_analyze_image(),
            // === end P4.9f1 ===

            // === P4.9I1A: the Brahma Console dispatch family ===
            Request::BrahmaConsoleList => match self.ready_db() {
                Ok(db) => super::brahma::brahma_console_list(&db, SINGLE_USER_ID),
                Err(r) => r,
            },
            Request::BrahmaConsoleCreate {
                console_connection_profile_id,
            } => match self.ready_db() {
                Ok(db) => {
                    super::brahma::brahma_console_create(
                        &db,
                        SINGLE_USER_ID,
                        console_connection_profile_id.as_ref(),
                    )
                    .await
                }
                Err(r) => r,
            },
            Request::BrahmaConsoleGet { chat_id } => match self.ready_db() {
                Ok(db) => super::brahma::brahma_console_get(&db, SINGLE_USER_ID, &chat_id),
                Err(r) => r,
            },
            Request::BrahmaConsoleRename { chat_id, title } => match self.ready_db() {
                Ok(db) => {
                    super::brahma::brahma_console_rename(&db, SINGLE_USER_ID, &chat_id, &title)
                        .await
                }
                Err(r) => r,
            },
            Request::BrahmaConsoleSetModel {
                chat_id,
                connection_profile_id,
            } => match self.ready_db() {
                Ok(db) => {
                    super::brahma::brahma_console_set_model(
                        &db,
                        SINGLE_USER_ID,
                        &chat_id,
                        &connection_profile_id,
                    )
                    .await
                }
                Err(r) => r,
            },
            Request::BrahmaConsoleDelete { chat_id } => match self.ready_db() {
                Ok(db) => super::brahma::brahma_console_delete(&db, SINGLE_USER_ID, &chat_id).await,
                Err(r) => r,
            },
            Request::BrahmaConsoleMessages { chat_id } => match self.ready_db() {
                Ok(db) => super::brahma::brahma_console_messages(&db, SINGLE_USER_ID, &chat_id),
                Err(r) => r,
            },
            Request::BrahmaConsoleSend {
                chat_id,
                content,
                file_ids,
            } => {
                // The owner/brahma-type gate, before the driver runs (v4's route
                // calls `verifyBrahmaChat` before `handleBrahmaConsoleMessage`).
                match self.ready_db() {
                    // The gate AND the body validation, in v4's order — see
                    // `brahma_send_prepare` (P4.60). Validating the body at the
                    // transport edge would answer 400 where v4 answers 404.
                    Ok(db) => match super::brahma::brahma_send_prepare(
                        &db,
                        &chat_id,
                        SINGLE_USER_ID,
                        &content,
                        file_ids.as_ref(),
                    ) {
                        Ok((content, file_ids)) => {
                            self.brahma_console_send(super::brahma::BrahmaConsoleSendRequest {
                                user_id: SINGLE_USER_ID.to_string(),
                                chat_id,
                                content,
                                file_ids,
                            })
                            .await
                        }
                        Err(r) => r,
                    },
                    Err(r) => r,
                }
            } // === end P4.9I1A ===
        }
    }

    /// Extract the open `Db` under the readiness gate (D2). `Err` is the locked
    /// refusal the caller returns directly.
    /// The host-supplied Almanack facts under the readiness gate (P4.37).
    /// `None` → the loud not-assembled refusal.
    fn ready_almanack_host(
        &self,
    ) -> Result<std::sync::Arc<dyn super::almanack::AlmanackHost>, Response> {
        match &*self.inner.state.lock().unwrap() {
            EngineState::Ready(r) => r.almanack_host.clone().ok_or_else(|| {
                Response::error(
                    ErrorKind::Internal,
                    "The Almanack is not assembled (no host facts wired)",
                )
            }),
            EngineState::Locked { pepper_state, .. } => Err(Response::locked(*pepper_state)),
        }
    }

    fn ready_db(&self) -> Result<Db, Response> {
        match &*self.inner.state.lock().unwrap() {
            EngineState::Ready(r) => Ok(r.db.clone()),
            EngineState::Locked { pepper_state, .. } => Err(Response::locked(*pepper_state)),
        }
    }

    /// The `Db` + the (optional) document-store refresh scheduler under the
    /// readiness gate (P4.6w). `None` when unwired — the write sites loud-skip
    /// the refresh. `Err` is the locked refusal.
    #[allow(clippy::type_complexity)]
    fn ready_db_and_refresh(
        &self,
    ) -> Result<(Db, Option<Arc<dyn crate::documents::MountRefreshScheduler>>), Response> {
        match &*self.inner.state.lock().unwrap() {
            EngineState::Ready(r) => Ok((r.db.clone(), r.mount_refresh.clone())),
            EngineState::Locked { pepper_state, .. } => Err(Response::locked(*pepper_state)),
        }
    }

    /// The `Db` + the (optional) blob-WebP transcoder under the readiness gate
    /// (P4.6bf S1). `None` when unwired — the two `store_mount_file` handlers fall
    /// back to the store-original refusal. `Err` is the locked refusal.
    #[allow(clippy::type_complexity)]
    fn ready_db_and_blob_webp(
        &self,
    ) -> Result<
        (
            Db,
            Option<Arc<dyn crate::services::mount_index::blob_transcode::WebpTranscoder>>,
        ),
        Response,
    > {
        match &*self.inner.state.lock().unwrap() {
            EngineState::Ready(r) => Ok((r.db.clone(), r.blob_webp.clone())),
            EngineState::Locked { pepper_state, .. } => Err(Response::locked(*pepper_state)),
        }
    }

    /// The `Db` + the (optional) live-PTY probe under the readiness gate (the
    /// P4.2-era stub-probe deferral, closed). `None` when unwired — the
    /// `ChatGet` reconcile treats every exitedAt-null session as orphaned,
    /// matching v4's empty `ptyManager` map. `Err` is the locked refusal.
    #[allow(clippy::type_complexity)]
    fn ready_db_and_terminal_probe(
        &self,
    ) -> Result<
        (
            Db,
            Option<Arc<dyn crate::services::ariel_notifications::TerminalLivenessProbe>>,
        ),
        Response,
    > {
        match &*self.inner.state.lock().unwrap() {
            EngineState::Ready(r) => Ok((r.db.clone(), r.terminal_probe.clone())),
            EngineState::Locked { pepper_state, .. } => Err(Response::locked(*pepper_state)),
        }
    }

    /// The courier-resolve driver under the readiness gate (P4.6ab). A ready engine
    /// without the driver (unwired host) answers the loud not-assembled refusal (the
    /// swipe-generate precedent — the host wires it at unification).
    fn ready_courier_resolve(
        &self,
    ) -> Result<Arc<dyn super::chat_media::CourierResolveDriver>, Response> {
        match &*self.inner.state.lock().unwrap() {
            EngineState::Ready(r) => match &r.courier_resolve {
                Some(d) => Ok(Arc::clone(d)),
                None => Err(Response::error(
                    ErrorKind::Internal,
                    "courier resolve not assembled (courier-resolve driver deferral)",
                )),
            },
            EngineState::Locked { pepper_state, .. } => Err(Response::locked(*pepper_state)),
        }
    }

    // ── P4.9E2A ──
    /// The Db + optional announcement-preview driver under the readiness gate.
    /// A ready engine without the driver still answers — the handler runs v4's
    /// validation / character / connection-profile arms and only refuses at the
    /// rewrite step (mirrors v4, where those checks precede the generation).
    #[allow(clippy::type_complexity)]
    /// The host's web-search-configured fact (P4.9E3B / P4.42) — DERIVED from the
    /// presence of the actual [`web_search`](ReadyEngine::web_search) provider the
    /// runner uses, so the tools inventory can never advertise `search_web` while
    /// the runner refuses it. `false` when locked or unassembled (the `ToolsList`
    /// arm gates on `ready_db` first).
    fn web_search_configured(&self) -> bool {
        match &*self.inner.state.lock().unwrap() {
            EngineState::Ready(r) => r.web_search.is_some(),
            EngineState::Locked { .. } => false,
        }
    }

    /// The registered SEARCH-provider manifests (P4.59) — the providers listing's
    /// search rows. Empty when locked or unassembled; the `ProviderList` arm
    /// gates on `ready_db` first, so a locked engine never reaches this.
    fn search_providers(&self) -> Vec<&'static crate::provider_manifest::search::SearchManifest> {
        match &*self.inner.state.lock().unwrap() {
            EngineState::Ready(r) => r.search_providers.clone(),
            EngineState::Locked { .. } => Vec::new(),
        }
    }

    /// The out-of-create llm_choose outfit runner (P4.9E3B) — `None` when
    /// locked or unassembled (both call sites then take v4's default-outfit
    /// fallback with a named warning).
    fn outfit_llm_choose(
        &self,
    ) -> Option<Arc<dyn crate::services::outfit_selections::OutfitLlmChooseRunner>> {
        match &*self.inner.state.lock().unwrap() {
            EngineState::Ready(r) => r.outfit_llm_choose.clone(),
            EngineState::Locked { .. } => None,
        }
    }

    /// The DB plus the (possibly unassembled) operator tool runner — the
    /// `run-tool` arm answers its own loud refusal when the runner is `None`, so
    /// this helper hands the `Option` through rather than erroring here.
    /// The manual title-regeneration driver under the readiness gate. A ready
    /// engine without it answers the loud not-assembled refusal.
    fn ready_regenerate_title(
        &self,
    ) -> Result<Arc<dyn crate::services::chat_admin::RegenerateTitleDriver>, Response> {
        match &*self.inner.state.lock().unwrap() {
            EngineState::Ready(r) => match &r.regenerate_title {
                Some(d) => Ok(Arc::clone(d)),
                None => Err(Response::error(
                    ErrorKind::Internal,
                    "regenerate-title is not available in this build: no title driver is assembled",
                )),
            },
            EngineState::Locked { pepper_state, .. } => Err(Response::locked(*pepper_state)),
        }
    }

    fn ready_db_and_operator_tool_runner(
        &self,
    ) -> Result<
        (
            Db,
            Option<Arc<dyn crate::services::chat_run_tool::OperatorToolRunner>>,
        ),
        Response,
    > {
        match &*self.inner.state.lock().unwrap() {
            EngineState::Ready(r) => Ok((r.db.clone(), r.operator_tool_runner.clone())),
            EngineState::Locked { pepper_state, .. } => Err(Response::locked(*pepper_state)),
        }
    }

    fn ready_db_and_announcement_preview(
        &self,
    ) -> Result<
        (
            Db,
            Option<Arc<dyn super::chat_post_office::AnnouncementPreviewDriver>>,
        ),
        Response,
    > {
        match &*self.inner.state.lock().unwrap() {
            EngineState::Ready(r) => Ok((r.db.clone(), r.announcement_preview.clone())),
            EngineState::Locked { pepper_state, .. } => Err(Response::locked(*pepper_state)),
        }
    }
    // ── end P4.9E2A ──

    // ── P4.9E4A ──
    /// The Db + optional vision-describe driver under the readiness gate. A
    /// ready engine without the driver still attaches — the describe ladder
    /// resolves to `''` (v4's own any-failure arm), never a refusal.
    #[allow(clippy::type_complexity)]
    fn ready_db_and_image_describe(
        &self,
    ) -> Result<(Db, Option<Arc<dyn super::chat_media::ImageDescribeDriver>>), Response> {
        match &*self.inner.state.lock().unwrap() {
            EngineState::Ready(r) => Ok((r.db.clone(), r.image_describe.clone())),
            EngineState::Locked { pepper_state, .. } => Err(Response::locked(*pepper_state)),
        }
    }
    // ── end P4.9E4A ──

    /// The Db + optional recall-replay driver under the readiness gate (P4.d13).
    /// A ready engine without the driver still answers — the handler runs the
    /// body-coercion / settings / anchor arms and only refuses at the run step
    /// (mirrors v4, where those checks precede the replay).
    #[allow(clippy::type_complexity)]
    fn ready_db_and_recall_replay(
        &self,
    ) -> Result<
        (
            Db,
            Option<Arc<dyn super::recall_replay::RecallReplayDriver>>,
        ),
        Response,
    > {
        match &*self.inner.state.lock().unwrap() {
            EngineState::Ready(r) => Ok((r.db.clone(), r.recall_replay.clone())),
            EngineState::Locked { pepper_state, .. } => Err(Response::locked(*pepper_state)),
        }
    }

    /// The Db + save-image bytes seam under the readiness gate (P4.6ab). An unwired
    /// host falls back to [`NotConfiguredBytes`] (faithful `EMPTY_BYTES`), so a ready
    /// engine always answers.
    fn ready_save_image(
        &self,
    ) -> Result<
        (
            Db,
            Arc<dyn crate::photos::save_image_to_album::FileBytesStore>,
        ),
        Response,
    > {
        match &*self.inner.state.lock().unwrap() {
            EngineState::Ready(r) => {
                let bytes = r.save_image_bytes.clone().unwrap_or_else(|| {
                    Arc::new(crate::photos::save_image_to_album::NotConfiguredBytes)
                });
                Ok((r.db.clone(), bytes))
            }
            EngineState::Locked { pepper_state, .. } => Err(Response::locked(*pepper_state)),
        }
    }

    /// The Db + image-generation runner under the readiness gate (P4.6ai). A ready
    /// engine without the runner (spine-less host — read-only embedders) answers the
    /// loud not-assembled refusal (the courier-resolve precedent — the host wires it
    /// LIVE at unification), keeping the `imageProfileGenerate` un-refusal deferred.
    /// The DB + the `ca22ec45` model-discovery seam, or the loud not-assembled
    /// refusal (the `ready_generate_image` precedent).
    fn ready_list_models(
        &self,
    ) -> Result<(Db, crate::model::image::ErasedImageDiscovery), Response> {
        match &*self.inner.state.lock().unwrap() {
            EngineState::Ready(r) => match &r.image_discovery {
                Some(d) => Ok((r.db.clone(), d.clone())),
                None => Err(Response::error(
                    ErrorKind::Internal,
                    "image model discovery not assembled (image-generation seam deferral)",
                )),
            },
            EngineState::Locked { pepper_state, .. } => Err(Response::locked(*pepper_state)),
        }
    }

    fn ready_generate_image(
        &self,
    ) -> Result<(Db, crate::tools::generate_image::ErasedImageGeneration), Response> {
        match &*self.inner.state.lock().unwrap() {
            EngineState::Ready(r) => match &r.image_generation {
                Some(runner) => Ok((r.db.clone(), runner.clone())),
                None => Err(Response::error(
                    ErrorKind::Internal,
                    "image generation not assembled (image-generation seam deferral)",
                )),
            },
            EngineState::Locked { pepper_state, .. } => Err(Response::locked(*pepper_state)),
        }
    }

    /// The Db + job-pump control under the readiness gate (P4.9G1). A ready
    /// engine without the pump (read-only embedder — no cadence) answers the loud
    /// not-assembled refusal (the image-generation precedent; the host wires it
    /// LIVE), keeping the tasks-queue control surface deferred there.
    fn ready_job_pump(
        &self,
    ) -> Result<(Db, Arc<dyn super::system_data::JobPumpControl>), Response> {
        match &*self.inner.state.lock().unwrap() {
            EngineState::Ready(r) => match &r.job_pump {
                Some(pump) => Ok((r.db.clone(), pump.clone())),
                None => Err(Response::error(
                    ErrorKind::Internal,
                    "job pump not assembled (job-pump-control seam deferral)",
                )),
            },
            EngineState::Locked { pepper_state, .. } => Err(Response::locked(*pepper_state)),
        }
    }

    /// The db + the backup host services (P4.9G5). `None` → the loud
    /// not-assembled refusal, never a silent stub.
    fn ready_backup_host(
        &self,
    ) -> Result<(Db, Arc<dyn crate::services::backup::BackupHost>), Response> {
        match &*self.inner.state.lock().unwrap() {
            EngineState::Ready(r) => match &r.backup_host {
                Some(h) => Ok((r.db.clone(), h.clone())),
                None => Err(Response::error(
                    ErrorKind::Internal,
                    "Backup is not available on this host (no backup services assembled).",
                )),
            },
            EngineState::Locked { pepper_state, .. } => Err(Response::locked(*pepper_state)),
        }
    }

    /// Best-effort pump nudge (v4 resume → `ensureProcessorRunning`). A resumed
    /// job is persisted regardless; if no pump is assembled the nudge is a no-op.
    fn nudge_job_pump(&self) {
        if let EngineState::Ready(r) = &*self.inner.state.lock().unwrap() {
            if let Some(pump) = &r.job_pump {
                pump.start();
            }
        }
    }

    /// The Db + avatar-preview renderer under the readiness gate (P4.9f1). An
    /// unwired seam is NOT an error here: the handler runs its guard tiers live
    /// and only the RENDER step answers the loud not-assembled refusal (the
    /// `ErasedAvatarPreview::none()` fold), so the whole verb stays
    /// differential-drivable while the host wire is deferred to unification.
    fn ready_avatar_preview(&self) -> Result<(Db, super::wardrobe::ErasedAvatarPreview), Response> {
        match &*self.inner.state.lock().unwrap() {
            EngineState::Ready(r) => Ok((
                r.db.clone(),
                r.avatar_preview
                    .clone()
                    .unwrap_or_else(super::wardrobe::ErasedAvatarPreview::none),
            )),
            EngineState::Locked { pepper_state, .. } => Err(Response::locked(*pepper_state)),
        }
    }

    /// The Db + swipe-generate driver under the readiness gate; a ready engine
    /// without the driver (read-only embedder) answers a plain internal error.
    fn ready_swipe(&self) -> Result<(Db, Arc<dyn SwipeGenerateDriver>), Response> {
        match &*self.inner.state.lock().unwrap() {
            EngineState::Ready(r) => match &r.swipe_generate {
                Some(d) => Ok((r.db.clone(), Arc::clone(d))),
                None => Err(Response::error(
                    ErrorKind::Internal,
                    "swipe generation not assembled",
                )),
            },
            EngineState::Locked { pepper_state, .. } => Err(Response::locked(*pepper_state)),
        }
    }

    /// The provider wire-actions driver under the readiness gate; a ready engine
    /// without the driver answers the P4.6d refusal verbatim.
    fn ready_provider_actions(&self) -> Result<Arc<dyn ProviderActionsDriver>, Response> {
        match &*self.inner.state.lock().unwrap() {
            EngineState::Ready(r) => match &r.provider_actions {
                Some(d) => Ok(Arc::clone(d)),
                None => Err(Response::error(
                    ErrorKind::Internal,
                    "provider wire actions not assembled (provider-actions driver deferral)",
                )),
            },
            EngineState::Locked { pepper_state, .. } => Err(Response::locked(*pepper_state)),
        }
    }

    /// The Db plus the memory embedding provider **if one is assembled** (P4.9a).
    /// Unlike [`Self::ready_memory_embedding`], an unwired provider is NOT an
    /// error: the photo-gallery listing needs the seam only when the caller
    /// supplied a search query, so the arm decides for itself whether the
    /// absence matters.
    #[allow(clippy::type_complexity)]
    fn ready_db_and_memory_embedding(
        &self,
    ) -> Result<(Db, Option<crate::model::embedding::ErasedEmbeddingProvider>), Response> {
        match &*self.inner.state.lock().unwrap() {
            EngineState::Ready(r) => Ok((r.db.clone(), r.memory_embedding.clone())),
            EngineState::Locked { pepper_state, .. } => Err(Response::locked(*pepper_state)),
        }
    }

    /// The Db + memory embedding provider under the readiness gate (P4.6s); a
    /// ready engine without the provider (unwired host / read-only embedder)
    /// answers the loud not-assembled refusal — the memory create/search arms
    /// alone need it. Host wiring lands with the SPA integration.
    fn ready_memory_embedding(
        &self,
    ) -> Result<(Db, crate::model::embedding::ErasedEmbeddingProvider), Response> {
        match &*self.inner.state.lock().unwrap() {
            EngineState::Ready(r) => match &r.memory_embedding {
                Some(p) => Ok((r.db.clone(), p.clone())),
                None => Err(Response::error(
                    ErrorKind::Internal,
                    "memory embedding not assembled (create/search embedding-seam deferral)",
                )),
            },
            EngineState::Locked { pepper_state, .. } => Err(Response::locked(*pepper_state)),
        }
    }

    /// The Db + the OPTIONAL custom-tool consult seam under the readiness gate
    /// (P4.6bd; the `ready_db_and_memory_embedding` shape). The arm's HANDLER
    /// decides what a `None` seam means: the composer/bench arms answer the
    /// loud not-assembled error only when a request actually wants a consult
    /// (an `llm`-bearing definition / `{live:true}`), so seamless assemblies
    /// keep serving every consult-free custom tool.
    #[allow(clippy::type_complexity)]
    fn ready_db_and_consult(
        &self,
    ) -> Result<
        (
            Db,
            Option<Arc<dyn crate::pascal::llm_consult::ConsultRunner>>,
        ),
        Response,
    > {
        match &*self.inner.state.lock().unwrap() {
            EngineState::Ready(r) => Ok((r.db.clone(), r.consult.clone())),
            EngineState::Locked { pepper_state, .. } => Err(Response::locked(*pepper_state)),
        }
    }

    /// The `ChatCreate` arm: readiness-gated (D2), then delegated to the
    /// assembly's driver (mirrors [`Self::chat_send`]). A ready engine without a
    /// driver (read-only embedder) answers a plain internal error.
    async fn chat_create(&self, req: ChatCreateDriverRequest) -> Response {
        let driver = {
            let state = self.inner.state.lock().unwrap();
            match &*state {
                EngineState::Ready(r) => match &r.chat_create {
                    Some(d) => Arc::clone(d),
                    None => {
                        return Response::error(ErrorKind::Internal, "chat create not assembled");
                    }
                },
                EngineState::Locked { pepper_state, .. } => {
                    return Response::locked(*pepper_state);
                }
            }
        };
        match driver.create(req).await {
            Ok(dto) => Response::ChatCreate(dto),
            Err(e) => Response::Error(e),
        }
    }

    /// The `ChatSend` arm: readiness-gated (D2), then delegated to the
    /// assembly's driver. A ready engine without a driver (read-only embedder)
    /// answers a plain internal error.
    async fn chat_send(&self, req: ChatSendRequest) -> Response {
        let driver = {
            let state = self.inner.state.lock().unwrap();
            match &*state {
                EngineState::Ready(r) => match &r.chat_send {
                    Some(d) => Arc::clone(d),
                    None => {
                        return Response::error(ErrorKind::Internal, "chat dispatch not assembled");
                    }
                },
                EngineState::Locked { pepper_state, .. } => {
                    return Response::locked(*pepper_state);
                }
            }
        };
        match driver.send(req).await {
            Ok(dto) => Response::ChatSend(dto),
            Err(e) => Response::Error(e),
        }
    }

    /// The `BrahmaConsoleSend` arm: readiness-gated (D2), then delegated to the
    /// assembly's orchestrator driver (mirrors [`Self::chat_send`]). A ready engine
    /// without a driver (read-only embedder) answers a plain internal error.
    async fn brahma_console_send(&self, req: super::brahma::BrahmaConsoleSendRequest) -> Response {
        let driver = {
            let state = self.inner.state.lock().unwrap();
            match &*state {
                EngineState::Ready(r) => match &r.brahma_console_send {
                    Some(d) => Arc::clone(d),
                    None => {
                        return Response::error(
                            ErrorKind::Internal,
                            "brahma console dispatch not assembled",
                        );
                    }
                },
                EngineState::Locked { pepper_state, .. } => {
                    return Response::locked(*pepper_state);
                }
            }
        };
        match driver.send(req).await {
            Ok(dto) => Response::BrahmaConsoleSend(dto),
            Err(e) => Response::Error(e),
        }
    }

    fn health(&self) -> Response {
        let (ready, pepper_state) = match &*self.inner.state.lock().unwrap() {
            EngineState::Ready(r) => (true, r.pepper_state),
            EngineState::Locked { pepper_state, .. } => (false, *pepper_state),
        };
        Response::Health(HealthDto {
            status: "ok".to_string(),
            version: self.inner.config.version.clone(),
            ready,
            pepper_state,
        })
    }

    /// v4 `GET /api/v1/system/unlock`: state + hasUserPassphrase, plus
    /// autoLockMinutes when unlocked and the user's auto-lock is enabled
    /// (that read is error-swallowed, as v4's is).
    fn unlock_state(&self) -> Response {
        let state = self.inner.state.lock().unwrap();
        let dto = match &*state {
            EngineState::Locked {
                pepper_state,
                has_user_passphrase,
            } => UnlockStateDto {
                state: *pepper_state,
                has_user_passphrase: *has_user_passphrase,
                auto_lock_minutes: None,
            },
            EngineState::Ready(r) => UnlockStateDto {
                state: r.pepper_state,
                has_user_passphrase: r.has_user_passphrase,
                auto_lock_minutes: read_auto_lock_minutes(&r.db),
            },
        };
        Response::UnlockState(dto)
    }

    /// v4 `?action=unlock`. Only meaningful from `needs-passphrase`; the
    /// 3-attempt limit and auto-lock resume are P4.4 (the full unlock
    /// service).
    fn unlock(&self, passphrase: &str) -> Response {
        let mut state = self.inner.state.lock().unwrap();
        match &*state {
            EngineState::Ready(_) => Response::error(ErrorKind::BadRequest, "Already unlocked"),
            EngineState::Locked {
                pepper_state: PepperState::NeedsSetup,
                ..
            } => Response::error(
                ErrorKind::BadRequest,
                "No database key is configured; setup is required first",
            ),
            EngineState::Locked {
                pepper_state,
                has_user_passphrase,
            } => {
                let pepper_state = *pepper_state;
                let has_user_passphrase = *has_user_passphrase;
                debug_assert_eq!(pepper_state, PepperState::NeedsPassphrase);
                match dbkey::load_pepper(&self.inner.config.data_dir(), Some(passphrase)) {
                    Ok(pepper) => match open_ready(
                        &self.inner,
                        &pepper,
                        PepperState::Resolved,
                        has_user_passphrase,
                    ) {
                        Ok(ready) => {
                            let dto = UnlockStateDto {
                                state: ready.pepper_state,
                                has_user_passphrase: ready.has_user_passphrase,
                                auto_lock_minutes: read_auto_lock_minutes(&ready.db),
                            };
                            *state = EngineState::Ready(ready);
                            // Deposit chokepoint 4 of 4 (v4 `unlockDbKey`,
                            // which caches the passphrase VERBATIM — reaching
                            // this branch means it decrypted the pepper, so it
                            // is by definition the real one). Inline: the state
                            // mutex is held and is not reentrant.
                            *self.inner.runtime_passphrase.lock().unwrap() =
                                Some(passphrase.to_string());
                            Response::UnlockState(dto)
                        }
                        Err(e) => Response::error(ErrorKind::Internal, e.to_string()),
                    },
                    Err(DbKeyError::DecryptFailed) => {
                        Response::error(ErrorKind::BadRequest, "Invalid passphrase")
                    }
                    Err(e) => Response::error(ErrorKind::Internal, e.to_string()),
                }
            }
        }
    }

    /// v4 `?action=setup` (`handleSetup` + `setupDbKey` + fresh-instance
    /// provisioning): mint a pepper, write `quilltap.dbkey`, create a fresh
    /// **encrypted-from-byte-zero** instance (schema + baseline seed), assemble,
    /// and return the pepper ONCE. Only valid from `needs-setup`.
    ///
    /// v4's post-setup plaintext→cipher conversion is a **named non-port**: v4
    /// creates the DB plaintext during pre-setup migrations, so setup must encrypt
    /// it in place; v5 has no plaintext window (the pepper is in hand before any
    /// partition is created), so there is nothing to convert.
    fn setup(&self, passphrase: &str) -> Response {
        let mut state = self.inner.state.lock().unwrap();
        match &*state {
            EngineState::Ready(_) => Response::error(
                ErrorKind::BadRequest,
                "Already set up (the vault is unlocked)",
            ),
            EngineState::Locked {
                pepper_state: PepperState::NeedsSetup,
                ..
            } => {
                let data_dir = self.inner.config.data_dir();
                // P4.46, the destructive-retry guard. A `Setup` that failed late
                // (say the open below) leaves the state `needs-setup` with the
                // key file and the partitions ALREADY on disk. Minting again
                // would overwrite `quilltap.dbkey` with a pepper that cannot
                // open those partitions — the original is display-once and gone,
                // so the instance would be bricked by a button press. Refuse
                // instead, naming what is in the way. Deliberate v5 hardening:
                // v4's setup converts an existing plaintext instance, so it has
                // no analog arm.
                let present = provisioning::existing_instance_files(&data_dir);
                if !present.is_empty() {
                    return Response::error(
                        ErrorKind::Conflict,
                        format!(
                            "This instance is already set up — {} already exists in {}. \
                             Refusing to mint a new encryption key over it (the existing key \
                             would be lost and its databases unreadable). Restart Quilltap to \
                             open the instance; if the first-run setup did not finish, move the \
                             data directory aside and start again.",
                            present.join(", "),
                            data_dir.display()
                        ),
                    );
                }
                // Claim the instance BEFORE creating anything (P4.46). Setup's
                // provisioning is three writable opens plus a whole DDL replay
                // and the baseline seed — every byte of it used to land before
                // any lock was taken.
                if let Err(e) = self.inner.assembler.pre_open(&data_dir) {
                    return Response::error(ErrorKind::Internal, e);
                }
                let pepper = match dbkey::generate_pepper() {
                    Ok(p) => p,
                    Err(e) => return Response::error(ErrorKind::Internal, e.to_string()),
                };
                // Write quilltap.dbkey — the instance's one and only key file
                // (one pepper wrapped once; since v4 4.8.1 / bug 60 no path
                // writes the phantom per-database `quilltap-llm-logs.dbkey`).
                if let Err(e) = dbkey::save_dbkey(&data_dir, &pepper, passphrase) {
                    return Response::error(ErrorKind::Internal, e.to_string());
                }
                // Provision the schema + baseline seed, encrypted from creation.
                if let Err(e) = provisioning::provision_fresh_instance(&data_dir, &pepper) {
                    return Response::error(ErrorKind::Internal, e.to_string());
                }
                let has_user_passphrase = !passphrase.is_empty();
                // Deposit chokepoint 1 of 4 (v4 `setupDbKey`).
                self.cache_runtime_passphrase(passphrase);
                match open_ready(
                    &self.inner,
                    &pepper,
                    PepperState::Resolved,
                    has_user_passphrase,
                ) {
                    Ok(ready) => {
                        *state = EngineState::Ready(ready);
                        Response::Setup(SetupResultDto {
                            pepper,
                            requires_restart: false,
                            message: SETUP_MESSAGE.to_string(),
                        })
                    }
                    // The key is written and the instance exists; only the
                    // re-open failed. v4's bug-64 fix is explicit that the
                    // response still carries the pepper — it is displayed
                    // exactly once, so it is never withheld behind an error —
                    // with `requiresRestart` telling the user the connection
                    // needs a restart to come up. The engine deliberately stays
                    // `needs-setup`: a restart re-reads the `.dbkey` and boots
                    // properly, and a second `Setup` meanwhile hits the guard
                    // above rather than minting over the key.
                    Err(e) => {
                        tracing::error!(
                            error = %e,
                            "setup wrote the key and provisioned, but the instance would not open \
                             — returning the pepper with a restart notice"
                        );
                        Response::Setup(SetupResultDto {
                            pepper,
                            requires_restart: true,
                            message: SETUP_MESSAGE.to_string(),
                        })
                    }
                }
            }
            EngineState::Locked { .. } => Response::error(
                ErrorKind::BadRequest,
                "Setup is only available on a fresh instance",
            ),
        }
    }

    /// v4 `?action=store` (`storeEnvPepperInDbKey`): persist the env-provided
    /// pepper into `quilltap.dbkey`, the `needs-vault-storage` → `resolved`
    /// transition. The engine is already Ready (an env pepper is operational); this
    /// just writes the file and advances the reported state.
    fn store_pepper(&self, passphrase: &str) -> Response {
        let mut state = self.inner.state.lock().unwrap();
        match &mut *state {
            EngineState::Ready(r) if r.pepper_state == PepperState::NeedsVaultStorage => {
                let pepper = match self.inner.config.env_pepper.as_deref() {
                    Some(p) if !p.is_empty() => p,
                    _ => {
                        return Response::error(
                            ErrorKind::Internal,
                            "No pepper in environment to store",
                        )
                    }
                };
                if let Err(e) = dbkey::save_dbkey(&self.inner.config.data_dir(), pepper, passphrase)
                {
                    return Response::error(ErrorKind::Internal, e.to_string());
                }
                r.pepper_state = PepperState::Resolved;
                r.has_user_passphrase = !passphrase.is_empty();
                // Deposit chokepoint 2 of 4 (v4 `storeEnvPepperInDbKey`, which
                // also sets the has-user-passphrase flag above).
                *self.inner.runtime_passphrase.lock().unwrap() = Some(if passphrase.is_empty() {
                    crate::dbkey::INTERNAL_PASSPHRASE.to_string()
                } else {
                    passphrase.to_string()
                });
                Response::Ack(AckDto::default())
            }
            EngineState::Ready(_) => Response::error(
                ErrorKind::BadRequest,
                "The pepper is already stored in a .dbkey file",
            ),
            EngineState::Locked { pepper_state, .. } => Response::locked(*pepper_state),
        }
    }

    /// v4 `?action=change-passphrase` (`changePassphrase`): re-wrap the pepper
    /// under a new passphrase (no DB re-encryption). Only valid when `resolved`
    /// (v4 requires exactly that state — `needs-vault-storage` has no .dbkey to
    /// re-wrap). Either passphrase may be empty (the no-passphrase sentinel).
    /// v4 `handleChangePassphrase` (`system/unlock/route.ts:325`) whole: the
    /// `.dbkey` re-wrap, then the archive-library sweep.
    ///
    /// The two phases are separate because they cannot share a lock. Phase one
    /// holds the engine's state mutex (which is not reentrant), and the sweep
    /// needs `ready_db()` + the storage backend, both of which re-acquire it —
    /// so the sweep runs HERE, at the dispatch arm, not inside
    /// `change_passphrase`. That is v4's own structure, where phase two lives in
    /// the route rather than in `changePassphrase`.
    ///
    /// **A failed sweep does not fail the passphrase change**, which has already
    /// happened and cannot be undone; `archives.total == -1` is v4's marker for
    /// "the sweep did not run at all", distinct from "ran and found nothing".
    async fn change_passphrase_with_archive_sweep(
        &self,
        old_passphrase: &str,
        new_passphrase: &str,
    ) -> Response {
        match self.change_passphrase(old_passphrase, new_passphrase) {
            Response::Ack(_) => {}
            refusal => return refusal,
        }
        Response::ChangePassphrase(ChangePassphraseResultDto {
            success: true,
            archives: self
                .reencrypt_archive_library(old_passphrase, new_passphrase)
                .await,
        })
    }

    /// Phase two of the passphrase change. Every failure lands in the result
    /// rather than in the response's status, so the caller always learns which
    /// bundles were left behind.
    async fn reencrypt_archive_library(
        &self,
        old_passphrase: &str,
        new_passphrase: &str,
    ) -> crate::services::character_archive::reencrypt::ArchiveReencryptResult {
        use crate::services::character_archive::reencrypt::{
            reencrypt_archive_bundles, ArchiveReencryptFailure, ArchiveReencryptResult,
        };
        // v4's catch arm: the whole sweep is reported as failed, under the
        // pseudo-file `(all archives)`, rather than silently pretending it ran.
        let swept_nothing = |reason: String| ArchiveReencryptResult {
            total: -1,
            reencrypted: 0,
            failures: vec![ArchiveReencryptFailure {
                file_id: String::new(),
                filename: "(all archives)".to_string(),
                reason,
            }],
        };
        let db = match self.ready_db() {
            Ok(db) => db,
            Err(_) => {
                return swept_nothing("The database is locked. Unlock it to continue.".into())
            }
        };
        let Some(backend) = self.qtap_file_storage() else {
            return swept_nothing(
                "File storage backend not available. Initialize the manager first.".into(),
            );
        };
        // The same empty-string → internal-sentinel rule `changePassphrase`
        // applied one phase ago, so the sweep sees real key material on both
        // sides (v4 `unlock/route.ts:352`).
        let resolve = |p: &str| {
            if p.is_empty() {
                crate::dbkey::INTERNAL_PASSPHRASE.to_string()
            } else {
                p.to_string()
            }
        };
        match reencrypt_archive_bundles(
            &db,
            backend.as_ref(),
            SINGLE_USER_ID,
            &resolve(old_passphrase),
            &resolve(new_passphrase),
        )
        .await
        {
            Ok(r) => r,
            Err(e) => swept_nothing(e.to_string()),
        }
    }

    fn change_passphrase(&self, old_passphrase: &str, new_passphrase: &str) -> Response {
        let mut state = self.inner.state.lock().unwrap();
        match &mut *state {
            EngineState::Ready(r) if r.pepper_state == PepperState::Resolved => {
                match dbkey::change_passphrase(
                    &self.inner.config.data_dir(),
                    old_passphrase,
                    new_passphrase,
                ) {
                    Ok(_) => {
                        r.has_user_passphrase = !new_passphrase.is_empty();
                        // Deposit chokepoint 3 of 4 (v4 `changePassphrase`).
                        // Inline rather than via the helper: the state mutex is
                        // already held here and it is not reentrant.
                        *self.inner.runtime_passphrase.lock().unwrap() =
                            Some(if new_passphrase.is_empty() {
                                crate::dbkey::INTERNAL_PASSPHRASE.to_string()
                            } else {
                                new_passphrase.to_string()
                            });
                        Response::Ack(AckDto::default())
                    }
                    Err(DbKeyError::DecryptFailed) => {
                        Response::error(ErrorKind::Unauthorized, "Current passphrase is incorrect")
                    }
                    Err(DbKeyError::NotFound(_)) => {
                        Response::error(ErrorKind::BadRequest, "No .dbkey file found")
                    }
                    Err(e) => Response::error(ErrorKind::Internal, e.to_string()),
                }
            }
            EngineState::Ready(_) => Response::error(
                ErrorKind::BadRequest,
                "Application must be unlocked before changing the passphrase",
            ),
            EngineState::Locked { .. } => Response::error(
                ErrorKind::BadRequest,
                "Application must be unlocked before changing the passphrase",
            ),
        }
    }

    /// v4 `?action=lock` / auto-lock (`lockDbKey`): tear the drivers down,
    /// drop the `Db`, return to `needs-passphrase`. Idempotent when already
    /// locked (answers the current state).
    fn lock(&self) -> Response {
        // Scope the guard: `unlock_state` below re-acquires the (non-reentrant)
        // state mutex, so it must be released first.
        {
            let mut state = self.inner.state.lock().unwrap();
            if let EngineState::Ready(r) = &*state {
                let has_user_passphrase = r.has_user_passphrase;
                let old = std::mem::replace(
                    &mut *state,
                    EngineState::Locked {
                        pepper_state: PepperState::NeedsPassphrase,
                        has_user_passphrase,
                    },
                );
                if let EngineState::Ready(r) = old {
                    r.shutdown.shutdown();
                    // r.db drops here; once the drivers' clones are gone too,
                    // the writer thread exits.
                }
                // v4 `lockDbKey` clears the runtime passphrase alongside the
                // pepper: locking is exactly the moment the marginal
                // passphrase-reuse exposure stops being justified.
                *self.inner.runtime_passphrase.lock().unwrap() = None;
            }
        }
        self.unlock_state()
    }
}

/// A uniform random `f64` in `[0, 1)` (v4 `Math.random()`), sourced from the OS
/// CSPRNG. Used by the turn-action next-speaker selection (the engine's real
/// clock/RNG; the differential harness injects a pinned value).
fn random_f64() -> f64 {
    let mut bytes = [0u8; 8];
    getrandom::getrandom(&mut bytes).expect("getrandom");
    let n = u64::from_le_bytes(bytes);
    // 53-bit mantissa → uniform [0,1).
    (n >> 11) as f64 / (1u64 << 53) as f64
}

/// Wall-clock epoch milliseconds (v4 `Date.now()`). Feeds the memory-search
/// recency ranking (the engine's real clock; the differential pins it).
fn wall_clock_ms() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as f64)
        .unwrap_or(0.0)
}

impl QuilltapCore for CoreEngine {
    fn dispatch(&self, req: Request) -> impl std::future::Future<Output = Response> + Send {
        self.dispatch_impl(req)
    }

    fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.inner.events.subscribe()
    }
}

// ============================================================================
// Helpers
// ============================================================================

/// Open the instance's databases and run the host assembler. Main is
/// required; the sibling partitions are opened when their files exist.
///
/// P4.46: the host's claim on the instance ([`EngineAssembler::pre_open`]) is
/// taken FIRST — before the main-DB existence probe and before any open — so a
/// contended boot/unlock refuses without a single byte written.
fn open_ready(
    inner: &EngineInner,
    pepper: &str,
    pepper_state: PepperState,
    has_user_passphrase: bool,
) -> Result<ReadyEngine, BootError> {
    let data = inner.config.data_dir();
    inner
        .assembler
        .pre_open(&data)
        .map_err(BootError::Assemble)?;
    let main = data.join("quilltap.db");
    if !main.exists() {
        return Err(BootError::MissingMainDb(main));
    }
    let optional = |name: &str| -> Option<PathBuf> {
        let p = data.join(name);
        p.exists().then_some(p)
    };
    let paths = DbPaths {
        main,
        mount_index: optional("quilltap-mount-index.db"),
        llm_logs: optional("quilltap-llm-logs.db"),
    };
    let db = Db::open(paths, pepper).map_err(BootError::Db)?;
    let assembly = inner
        .assembler
        .assemble(
            &db,
            &inner.events,
            pepper,
            &data,
            &inner.creation_progress_bus,
        )
        .map_err(BootError::Assemble)?;
    Ok(ReadyEngine {
        db,
        pepper_state,
        has_user_passphrase,
        shutdown: assembly.shutdown,
        chat_send: assembly.chat_send,
        chat_create: assembly.chat_create,
        swipe_generate: assembly.swipe_generate,
        provider_actions: assembly.provider_actions,
        memory_embedding: assembly.memory_embedding,
        mount_refresh: assembly.mount_refresh,
        terminal_probe: assembly.terminal_probe,
        courier_resolve: assembly.courier_resolve,
        save_image_bytes: assembly.save_image_bytes,
        image_generation: assembly.image_generation,
        image_discovery: assembly.image_discovery,
        consult: assembly.consult,
        avatar_preview: assembly.avatar_preview,
        brahma_console_send: assembly.brahma_console_send,
        blob_webp: assembly.blob_webp,
        recall_replay: assembly.recall_replay,
        job_pump: assembly.job_pump,
        backup_host: assembly.backup_host,
        almanack_host: assembly.almanack_host,
        // === P4.9E2A ===
        announcement_preview: assembly.announcement_preview,
        operator_tool_runner: assembly.operator_tool_runner,
        regenerate_title: assembly.regenerate_title,
        // === end P4.9E2A ===
        // === P4.9E3B / P4.42 ===
        web_search: assembly.web_search,
        search_providers: assembly.search_providers,
        outfit_llm_choose: assembly.outfit_llm_choose,
        // === end P4.9E3B ===
        // === P4.9E4A ===
        image_describe: assembly.image_describe,
        // === end P4.9E4A ===
    })
}

/// The auto-lock read behind the unlock-state DTO (v4: chatSettings
/// `autoLockSettings.enabled ? idleMinutes : null`, error-swallowed).
fn read_auto_lock_minutes(db: &Db) -> Option<f64> {
    let settings = db
        .read_main(|conn| chat_settings::find_by_user_id(conn, SINGLE_USER_ID))
        .ok()??;
    let auto = settings.get("autoLockSettings")?;
    if auto.get("enabled").and_then(|v| v.as_bool()) == Some(true) {
        auto.get("idleMinutes").and_then(|v| v.as_f64())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Writer;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tempfile::{tempdir, TempDir};

    const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";

    struct EmptyInstances;
    impl InstanceDirectory for EmptyInstances {
        fn list(&self) -> Result<super::super::types::InstancesDto, String> {
            Ok(super::super::types::InstancesDto {
                instances: vec![],
                default_instance: None,
            })
        }
    }

    /// Records assemble/shutdown counts so the lock/unlock cycle is provable.
    struct CountingAssembler {
        assembled: Arc<AtomicUsize>,
        shutdowns: Arc<AtomicUsize>,
    }
    struct CountingShutdown(Arc<AtomicUsize>);
    impl EngineShutdown for CountingShutdown {
        fn shutdown(&self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
    impl EngineAssembler for CountingAssembler {
        fn assemble(
            &self,
            _db: &Db,
            _events: &broadcast::Sender<Event>,
            _pepper: &str,
            _data_dir: &std::path::Path,
            _bus: &Arc<CreationProgressBus>,
        ) -> Result<EngineAssembly, String> {
            self.assembled.fetch_add(1, Ordering::SeqCst);
            Ok(EngineAssembly::shutdown_only(Box::new(CountingShutdown(
                self.shutdowns.clone(),
            ))))
        }
    }

    /// P4.46: an assembler whose host claim REFUSES — the shape a live lock
    /// conflict has at this seam. `assemble` panics: reaching it would mean the
    /// engine opened (or created) partitions past a refused claim, which is
    /// exactly the defect this lane closes.
    struct RefusingPreOpen;
    impl EngineAssembler for RefusingPreOpen {
        fn pre_open(&self, _data_dir: &std::path::Path) -> Result<(), String> {
            Err(
                "Another Quilltap instance (local server, PID 4242) is already using this \
                 database."
                    .to_string(),
            )
        }
        fn assemble(
            &self,
            _db: &Db,
            _events: &broadcast::Sender<Event>,
            _pepper: &str,
            _data_dir: &std::path::Path,
            _bus: &Arc<CreationProgressBus>,
        ) -> Result<EngineAssembly, String> {
            panic!("assemble reached past a refused pre_open");
        }
    }

    /// P4.46: the claim succeeds but assembly fails — the "setup wrote
    /// everything and then the instance would not open" shape (v4 bug 64's
    /// resume failure).
    struct FailingAssembler;
    impl EngineAssembler for FailingAssembler {
        fn assemble(
            &self,
            _db: &Db,
            _events: &broadcast::Sender<Event>,
            _pepper: &str,
            _data_dir: &std::path::Path,
            _bus: &Arc<CreationProgressBus>,
        ) -> Result<EngineAssembly, String> {
            Err("the drivers refused to come up".to_string())
        }
    }

    /// Every file in a data dir, sorted — the "nothing was created" comparand.
    fn dir_listing(data: &std::path::Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(data)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        names
    }

    /// An instance dir with a main DB (no tables needed for the state tests).
    fn make_instance() -> TempDir {
        let base = tempdir().unwrap();
        let data = base.path().join("data");
        std::fs::create_dir_all(&data).unwrap();
        // A writable open creates the encrypted main DB file.
        let _ = Writer::open_writable(&data.join("quilltap.db"), PEPPER).unwrap();
        base
    }

    fn config(base: &TempDir, env_pepper: Option<&str>) -> CoreConfig {
        CoreConfig {
            base_dir: base.path().to_path_buf(),
            version: "test".to_string(),
            env_pepper: env_pepper.map(str::to_string),
        }
    }

    #[tokio::test]
    async fn env_pepper_boot_is_operational_and_gates_work() {
        let base = make_instance();
        let engine = CoreEngine::boot(
            config(&base, Some(PEPPER)),
            Box::new(NoopAssembler),
            Arc::new(EmptyInstances),
        )
        .unwrap();

        match engine.dispatch(Request::Health).await {
            Response::Health(h) => {
                assert!(h.ready);
                assert_eq!(h.pepper_state, PepperState::NeedsVaultStorage);
                assert_eq!(h.status, "ok");
            }
            other => panic!("unexpected: {other:?}"),
        }
        // ListChats reaches the engine (the fixture has no chats table, so it
        // surfaces as an internal read error — NOT the locked refusal).
        match engine
            .dispatch(Request::ListChats {
                exclude_tag_ids: vec![],
                limit: None,
                include_autonomous: false,
            })
            .await
        {
            Response::Error(e) => assert_eq!(e.kind, ErrorKind::Internal),
            other => panic!("unexpected: {other:?}"),
        }
        // ChatSend on a ready engine WITHOUT a driver (NoopAssembler): the
        // typed not-assembled refusal, not the locked one.
        match engine
            .dispatch(Request::ChatSend {
                chat_id: "c1".into(),
                content: "hi".into(),
                continue_mode: false,
                responding_participant_id: None,
                target_participant_ids: None,
                speaking_as_participant_id: None,
                file_ids: vec![],
                nudge: None,
                pending_tool_results: vec![],
            })
            .await
        {
            Response::Error(e) => {
                assert_eq!(e.kind, ErrorKind::Internal);
                assert_eq!(e.message, "chat dispatch not assembled");
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[tokio::test]
    async fn chat_send_gate_rejects_blank_message() {
        // v4 sendMessageSchema superRefine: a normal send with blank content, no
        // files, and no tool results → bad-request with the exact message.
        let base = make_instance();
        let engine = CoreEngine::boot(
            config(&base, Some(PEPPER)),
            Box::new(NoopAssembler),
            Arc::new(EmptyInstances),
        )
        .unwrap();
        match engine
            .dispatch(Request::ChatSend {
                chat_id: "c1".into(),
                content: "   ".into(),
                continue_mode: false,
                responding_participant_id: None,
                target_participant_ids: None,
                speaking_as_participant_id: None,
                file_ids: vec![],
                nudge: None,
                pending_tool_results: vec![],
            })
            .await
        {
            Response::Error(e) => {
                assert_eq!(e.kind, ErrorKind::BadRequest);
                assert_eq!(
                    e.message,
                    "Message must have content, attached files, or tool results"
                );
            }
            other => panic!("unexpected: {other:?}"),
        }
        // A continue-mode send with blank content is NOT rejected by the gate
        // (it reaches the driver → the not-assembled refusal on NoopAssembler).
        match engine
            .dispatch(Request::ChatSend {
                chat_id: "c1".into(),
                content: String::new(),
                continue_mode: true,
                responding_participant_id: None,
                target_participant_ids: None,
                speaking_as_participant_id: None,
                file_ids: vec![],
                nudge: Some(true),
                pending_tool_results: vec![],
            })
            .await
        {
            Response::Error(e) => {
                assert_eq!(e.kind, ErrorKind::Internal);
                assert_eq!(e.message, "chat dispatch not assembled");
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[tokio::test]
    async fn passphrase_vault_boots_locked_and_unlocks() {
        let base = make_instance();
        dbkey::save_dbkey(&base.path().join("data"), PEPPER, "open sesame").unwrap();

        let assembled = Arc::new(AtomicUsize::new(0));
        let shutdowns = Arc::new(AtomicUsize::new(0));
        let engine = CoreEngine::boot(
            config(&base, None),
            Box::new(CountingAssembler {
                assembled: assembled.clone(),
                shutdowns: shutdowns.clone(),
            }),
            Arc::new(EmptyInstances),
        )
        .unwrap();

        // Locked boot: not assembled, ready-gated variants refused.
        assert_eq!(assembled.load(Ordering::SeqCst), 0);
        match engine
            .dispatch(Request::ListChats {
                exclude_tag_ids: vec![],
                limit: None,
                include_autonomous: false,
            })
            .await
        {
            Response::Error(e) => {
                assert_eq!(e.kind, ErrorKind::Locked);
                assert_eq!(e.pepper_state, Some(PepperState::NeedsPassphrase));
            }
            other => panic!("unexpected: {other:?}"),
        }
        match engine.dispatch(Request::UnlockState).await {
            Response::UnlockState(u) => {
                assert_eq!(u.state, PepperState::NeedsPassphrase);
                assert!(u.has_user_passphrase);
            }
            other => panic!("unexpected: {other:?}"),
        }

        // Wrong passphrase → bad request; still locked.
        match engine
            .dispatch(Request::Unlock {
                passphrase: "wrong".into(),
            })
            .await
        {
            Response::Error(e) => {
                assert_eq!(e.kind, ErrorKind::BadRequest);
                assert_eq!(e.message, "Invalid passphrase");
            }
            other => panic!("unexpected: {other:?}"),
        }

        // Right passphrase → assembled + resolved.
        match engine
            .dispatch(Request::Unlock {
                passphrase: "open sesame".into(),
            })
            .await
        {
            Response::UnlockState(u) => {
                assert_eq!(u.state, PepperState::Resolved);
                assert!(u.has_user_passphrase);
            }
            other => panic!("unexpected: {other:?}"),
        }
        assert_eq!(assembled.load(Ordering::SeqCst), 1);

        // Lock tears down; unlock re-assembles.
        match engine.dispatch(Request::Lock).await {
            Response::UnlockState(u) => assert_eq!(u.state, PepperState::NeedsPassphrase),
            other => panic!("unexpected: {other:?}"),
        }
        assert_eq!(shutdowns.load(Ordering::SeqCst), 1);
        assert!(engine.db().is_none());

        match engine
            .dispatch(Request::Unlock {
                passphrase: "open sesame".into(),
            })
            .await
        {
            Response::UnlockState(u) => assert_eq!(u.state, PepperState::Resolved),
            other => panic!("unexpected: {other:?}"),
        }
        assert_eq!(assembled.load(Ordering::SeqCst), 2);
        assert!(engine.db().is_some());
    }

    #[tokio::test]
    async fn needs_setup_boot_refuses_unlock() {
        let base = tempdir().unwrap();
        std::fs::create_dir_all(base.path().join("data")).unwrap();
        let engine = CoreEngine::boot(
            config(&base, None),
            Box::new(NoopAssembler),
            Arc::new(EmptyInstances),
        )
        .unwrap();
        match engine.dispatch(Request::Health).await {
            Response::Health(h) => {
                assert!(!h.ready);
                assert_eq!(h.pepper_state, PepperState::NeedsSetup);
            }
            other => panic!("unexpected: {other:?}"),
        }
        match engine
            .dispatch(Request::Unlock {
                passphrase: "anything".into(),
            })
            .await
        {
            Response::Error(e) => assert_eq!(e.kind, ErrorKind::BadRequest),
            other => panic!("unexpected: {other:?}"),
        }
    }

    /// P4.D63 — the runtime passphrase cache's whole state machine, which is
    /// what `resolve_archive_passphrase` (and therefore every archive bundle)
    /// depends on. Proves all four of v4's deposit chokepoints reachable here
    /// plus the lock-clears, and the internal-sentinel leg.
    #[tokio::test]
    async fn runtime_passphrase_cache_follows_the_dbkey_lifecycle() {
        let base = tempdir().unwrap();
        std::fs::create_dir_all(base.path().join("data")).unwrap();
        let engine = CoreEngine::boot(
            config(&base, None),
            Box::new(CountingAssembler {
                assembled: Arc::new(AtomicUsize::new(0)),
                shutdowns: Arc::new(AtomicUsize::new(0)),
            }),
            Arc::new(EmptyInstances),
        )
        .unwrap();

        // Before setup nothing has passed through, and no USER passphrase
        // protects the instance yet → the internal sentinel, not a refusal.
        assert_eq!(
            engine.resolve_archive_passphrase().unwrap(),
            crate::dbkey::INTERNAL_PASSPHRASE
        );

        // Deposit 1: setup.
        match engine
            .dispatch(Request::Setup {
                passphrase: "open sesame".into(),
            })
            .await
        {
            Response::Setup(_) => {}
            other => panic!("unexpected: {other:?}"),
        }
        assert_eq!(engine.resolve_archive_passphrase().unwrap(), "open sesame");

        // Deposit 3: change-passphrase moves the cache with the .dbkey.
        match engine
            .dispatch(Request::ChangePassphrase {
                old_passphrase: "open sesame".into(),
                new_passphrase: "friend and enter".into(),
            })
            .await
        {
            // [P4.D65] change-passphrase now answers `{success, archives}`.
            Response::ChangePassphrase(dto) => assert!(dto.success),
            other => panic!("unexpected: {other:?}"),
        }
        assert_eq!(
            engine.resolve_archive_passphrase().unwrap(),
            "friend and enter"
        );

        // Lock clears it. A USER passphrase protects the instance, so the
        // sentinel is NOT a valid substitute — this is the loud refusal, and
        // the reason the archive verb can 400 with a named sentence.
        match engine.dispatch(Request::Lock).await {
            Response::UnlockState(_) => {}
            other => panic!("unexpected: {other:?}"),
        }
        let err = engine.resolve_archive_passphrase().unwrap_err();
        assert_eq!(
            err,
            crate::services::character_archive::crypto::ArchiveCryptoError::KeyUnavailable
        );

        // Deposit 4: unlock restores it verbatim.
        match engine
            .dispatch(Request::Unlock {
                passphrase: "friend and enter".into(),
            })
            .await
        {
            Response::UnlockState(_) => {}
            other => panic!("unexpected: {other:?}"),
        }
        assert_eq!(
            engine.resolve_archive_passphrase().unwrap(),
            "friend and enter"
        );

        // The empty-passphrase leg maps to the sentinel at the deposit, so a
        // no-passphrase instance never reaches the refusal.
        match engine
            .dispatch(Request::ChangePassphrase {
                old_passphrase: "friend and enter".into(),
                new_passphrase: String::new(),
            })
            .await
        {
            Response::ChangePassphrase(dto) => assert!(dto.success),
            other => panic!("unexpected: {other:?}"),
        }
        assert_eq!(
            engine.resolve_archive_passphrase().unwrap(),
            crate::dbkey::INTERNAL_PASSPHRASE
        );
    }

    #[tokio::test]
    async fn setup_provisions_a_bootable_instance() {
        // A truly empty data dir → needs-setup.
        let base = tempdir().unwrap();
        std::fs::create_dir_all(base.path().join("data")).unwrap();
        let assembled = Arc::new(AtomicUsize::new(0));
        let shutdowns = Arc::new(AtomicUsize::new(0));
        let engine = CoreEngine::boot(
            config(&base, None),
            Box::new(CountingAssembler {
                assembled: assembled.clone(),
                shutdowns: shutdowns.clone(),
            }),
            Arc::new(EmptyInstances),
        )
        .unwrap();

        // needs-setup boots locked; not assembled yet.
        assert_eq!(assembled.load(Ordering::SeqCst), 0);

        // Setup with a user passphrase mints a pepper, provisions, assembles.
        let pepper = match engine
            .dispatch(Request::Setup {
                passphrase: "open sesame".into(),
            })
            .await
        {
            Response::Setup(s) => {
                assert!(!s.pepper.is_empty());
                assert!(s.message.contains("will not be displayed again"));
                s.pepper
            }
            other => panic!("unexpected: {other:?}"),
        };
        assert_eq!(assembled.load(Ordering::SeqCst), 1);

        // The instance now boots and answers: ready, resolved, listChats -> [].
        match engine.dispatch(Request::Health).await {
            Response::Health(h) => {
                assert!(h.ready);
                assert_eq!(h.pepper_state, PepperState::Resolved);
            }
            other => panic!("unexpected: {other:?}"),
        }
        match engine
            .dispatch(Request::ListChats {
                exclude_tag_ids: vec![],
                limit: None,
                include_autonomous: false,
            })
            .await
        {
            Response::Chats(c) => assert!(c.is_empty()),
            other => panic!("unexpected: {other:?}"),
        }
        // The .dbkey was written and unlocks with the passphrase.
        let recovered = dbkey::load_pepper(&base.path().join("data"), Some("open sesame")).unwrap();
        assert_eq!(recovered, pepper);

        // A second Setup refuses (already set up).
        match engine
            .dispatch(Request::Setup {
                passphrase: "x".into(),
            })
            .await
        {
            Response::Error(e) => assert_eq!(e.kind, ErrorKind::BadRequest),
            other => panic!("unexpected: {other:?}"),
        }
    }

    /// P4.46 deliverable 1+2, the setup entrance: a refused host claim stops
    /// `Setup` before it writes ANYTHING. Before this lane the claim was taken
    /// inside `assemble` — i.e. after `save_dbkey`, after three writable opens,
    /// after the whole DDL replay and the baseline seed. Mutation check: move
    /// the `pre_open` call below `provision_fresh_instance` and the listing
    /// assertion goes red (four files instead of none).
    #[tokio::test]
    async fn setup_creates_nothing_when_the_instance_claim_is_refused() {
        let base = tempdir().unwrap();
        let data = base.path().join("data");
        std::fs::create_dir_all(&data).unwrap();

        let engine = CoreEngine::boot(
            config(&base, None),
            Box::new(RefusingPreOpen),
            Arc::new(EmptyInstances),
        )
        .unwrap();

        match engine
            .dispatch(Request::Setup {
                passphrase: String::new(),
            })
            .await
        {
            Response::Error(e) => {
                assert_eq!(e.kind, ErrorKind::Internal);
                assert!(e.message.contains("already using this database"), "{e:?}");
            }
            other => panic!("unexpected: {other:?}"),
        }
        // Not one byte: no key file, no partitions.
        assert!(
            dir_listing(&data).is_empty(),
            "setup left files behind: {:?}",
            dir_listing(&data)
        );
    }

    /// P4.46 deliverable 1+2, the boot entrance: a refused claim refuses the
    /// boot with the partitions byte-for-byte untouched (`RefusingPreOpen`
    /// panics if `assemble` is ever reached, so this also pins that no open
    /// happened at all).
    #[tokio::test]
    async fn boot_refuses_before_opening_when_the_claim_is_refused() {
        let base = make_instance();
        let data = base.path().join("data");
        let before = std::fs::read(data.join("quilltap.db")).unwrap();

        match CoreEngine::boot(
            config(&base, Some(PEPPER)),
            Box::new(RefusingPreOpen),
            Arc::new(EmptyInstances),
        ) {
            Err(BootError::Assemble(m)) => {
                assert!(m.contains("already using this database"), "{m}")
            }
            Err(other) => panic!("unexpected: {other:?}"),
            Ok(_) => panic!("a refused claim must refuse the boot"),
        }
        assert_eq!(
            std::fs::read(data.join("quilltap.db")).unwrap(),
            before,
            "the main partition changed on a refused boot"
        );
    }

    /// P4.46 deliverable 1+2, the unlock entrance: same shape, from the locked
    /// state (every unlock repeats the open sequence, so it needed the same
    /// reordering).
    #[tokio::test]
    async fn unlock_refuses_before_opening_when_the_claim_is_refused() {
        let base = make_instance();
        let data = base.path().join("data");
        dbkey::save_dbkey(&data, PEPPER, "open sesame").unwrap();
        let before = std::fs::read(data.join("quilltap.db")).unwrap();

        // A locked boot opens nothing, so it reaches Ready-less state cleanly.
        let engine = CoreEngine::boot(
            config(&base, None),
            Box::new(RefusingPreOpen),
            Arc::new(EmptyInstances),
        )
        .unwrap();

        match engine
            .dispatch(Request::Unlock {
                passphrase: "open sesame".into(),
            })
            .await
        {
            Response::Error(e) => {
                assert_eq!(e.kind, ErrorKind::Internal);
                assert!(e.message.contains("already using this database"), "{e:?}");
            }
            other => panic!("unexpected: {other:?}"),
        }
        assert_eq!(
            std::fs::read(data.join("quilltap.db")).unwrap(),
            before,
            "the main partition changed on a refused unlock"
        );
    }

    /// P4.46 deliverables 3+4: setup finishes writing the key and the instance
    /// but the open fails — the pepper is STILL returned (displayed exactly
    /// once, never withheld behind an error) with `requiresRestart`, and the
    /// retry that would otherwise brick the instance refuses by name.
    #[tokio::test]
    async fn late_setup_failure_returns_the_pepper_and_guards_the_retry() {
        let base = tempdir().unwrap();
        let data = base.path().join("data");
        std::fs::create_dir_all(&data).unwrap();

        let engine = CoreEngine::boot(
            config(&base, None),
            Box::new(FailingAssembler),
            Arc::new(EmptyInstances),
        )
        .unwrap();

        let pepper = match engine
            .dispatch(Request::Setup {
                passphrase: "open sesame".into(),
            })
            .await
        {
            Response::Setup(s) => {
                assert!(s.requires_restart, "a failed open must ask for a restart");
                assert!(!s.pepper.is_empty());
                assert!(s.message.contains("will not be displayed again"));
                s.pepper
            }
            other => panic!("unexpected: {other:?}"),
        };
        // The returned pepper is the real one: it unlocks the key file that was
        // written, which is the whole point of returning it anyway.
        assert_eq!(
            dbkey::load_pepper(&data, Some("open sesame")).unwrap(),
            pepper
        );

        // The retry guard: a second Setup must NOT mint a new pepper over the
        // key file (the old one is display-once and gone, and its partitions
        // would become unreadable). It refuses, naming what is in the way.
        let dbkey_before = std::fs::read(data.join("quilltap.dbkey")).unwrap();
        match engine
            .dispatch(Request::Setup {
                passphrase: "second try".into(),
            })
            .await
        {
            Response::Error(e) => {
                assert_eq!(e.kind, ErrorKind::Conflict);
                assert!(e.message.contains("quilltap.dbkey"), "{e:?}");
                assert!(e.message.contains("already set up"), "{e:?}");
            }
            other => panic!("unexpected: {other:?}"),
        }
        assert_eq!(
            std::fs::read(data.join("quilltap.dbkey")).unwrap(),
            dbkey_before,
            "the retry overwrote the key file"
        );
        // And the pepper still unlocks it (the retry changed nothing).
        assert_eq!(
            dbkey::load_pepper(&data, Some("open sesame")).unwrap(),
            pepper
        );
    }

    /// P4.46 deliverable 4, at the provisioning layer: the doc's "MUST NOT
    /// already exist" is enforced, not merely stated — the DDL has no
    /// `IF NOT EXISTS` anywhere, so a replay over a live instance would die
    /// halfway through its own transaction.
    #[test]
    fn provisioning_refuses_an_already_provisioned_data_dir() {
        let base = tempdir().unwrap();
        let data = base.path().join("data");
        std::fs::create_dir_all(&data).unwrap();
        let _ = Writer::open_writable(&data.join("quilltap.db"), PEPPER).unwrap();

        match provisioning::provision_fresh_instance(&data, PEPPER) {
            Err(e) => {
                let msg = e.to_string();
                assert!(msg.contains("already provisioned"), "{msg}");
                assert!(msg.contains("quilltap.db"), "{msg}");
            }
            Ok(()) => panic!("provisioned over an existing instance"),
        }
    }

    #[tokio::test]
    async fn store_pepper_advances_needs_vault_storage_to_resolved() {
        // Env pepper + a main DB but no .dbkey → needs-vault-storage (operational).
        let base = make_instance();
        let engine = CoreEngine::boot(
            config(&base, Some(PEPPER)),
            Box::new(NoopAssembler),
            Arc::new(EmptyInstances),
        )
        .unwrap();
        match engine.dispatch(Request::UnlockState).await {
            Response::UnlockState(u) => assert_eq!(u.state, PepperState::NeedsVaultStorage),
            other => panic!("unexpected: {other:?}"),
        }

        // Store writes quilltap.dbkey and advances to resolved.
        match engine
            .dispatch(Request::StorePepper {
                passphrase: String::new(),
            })
            .await
        {
            Response::Ack(_) => {}
            other => panic!("unexpected: {other:?}"),
        }
        assert!(base.path().join("data").join("quilltap.dbkey").exists());
        match engine.dispatch(Request::UnlockState).await {
            Response::UnlockState(u) => {
                assert_eq!(u.state, PepperState::Resolved);
                assert!(!u.has_user_passphrase);
            }
            other => panic!("unexpected: {other:?}"),
        }
        // The written .dbkey decrypts to the same pepper (no user passphrase).
        assert_eq!(
            dbkey::load_pepper(&base.path().join("data"), None).unwrap(),
            PEPPER
        );
    }

    #[tokio::test]
    async fn change_passphrase_rewraps_and_gates_wrong_old() {
        let base = tempdir().unwrap();
        std::fs::create_dir_all(base.path().join("data")).unwrap();
        let engine = CoreEngine::boot(
            config(&base, None),
            Box::new(NoopAssembler),
            Arc::new(EmptyInstances),
        )
        .unwrap();
        // Set up with an initial passphrase.
        match engine
            .dispatch(Request::Setup {
                passphrase: "first".into(),
            })
            .await
        {
            Response::Setup(_) => {}
            other => panic!("unexpected: {other:?}"),
        }

        // Wrong old passphrase → unauthorized.
        match engine
            .dispatch(Request::ChangePassphrase {
                old_passphrase: "wrong".into(),
                new_passphrase: "second".into(),
            })
            .await
        {
            Response::Error(e) => assert_eq!(e.kind, ErrorKind::Unauthorized),
            other => panic!("unexpected: {other:?}"),
        }

        // Correct old → re-wrapped; the new passphrase now unlocks, the old does not.
        match engine
            .dispatch(Request::ChangePassphrase {
                old_passphrase: "first".into(),
                new_passphrase: "second".into(),
            })
            .await
        {
            // [P4.D65] The body carries the archive sweep's summary now. This
            // engine has no backup host (`NoopAssembler`), so the sweep cannot
            // reach a storage backend and reports itself as wholly not-run —
            // `total: -1`, v4's marker — WITHOUT failing the passphrase change,
            // which is the whole point of reporting the two separately.
            Response::ChangePassphrase(dto) => {
                assert!(dto.success);
                assert_eq!(dto.archives.total, -1);
                assert_eq!(dto.archives.reencrypted, 0);
                assert_eq!(dto.archives.failures.len(), 1);
                assert_eq!(dto.archives.failures[0].filename, "(all archives)");
            }
            other => panic!("unexpected: {other:?}"),
        }
        let data = base.path().join("data");
        assert!(dbkey::load_pepper(&data, Some("first")).is_err());
        assert!(dbkey::load_pepper(&data, Some("second")).is_ok());
        // One pepper, one file (v4 4.8.1, bug 60): only `quilltap.dbkey` is
        // written; the phantom per-database key file is NOT created.
        assert!(data.join("quilltap.dbkey").exists());
        assert!(!data.join("quilltap-llm-logs.dbkey").exists());
    }

    // ── The dogfood-#60 guard WIRING (the unification-review pin) ────────────
    //
    // `system_data::pump_pause_tests` proves the guard's semantics; this proves
    // the two dispatch arms actually TAKE it — delete either `let _pump = …`
    // line and one assertion below goes red. Both arms cycle the pump even when
    // their body refuses early (wrong sentinel / unknown upload), which is what
    // makes the wiring observable without a real restore.

    struct RecordingPump {
        running: std::sync::atomic::AtomicBool,
        starts: AtomicUsize,
        stops: AtomicUsize,
    }
    impl crate::api::system_data::JobPumpControl for RecordingPump {
        fn status(&self) -> crate::api::system_data::ProcessorStatus {
            crate::api::system_data::ProcessorStatus {
                running: self.running.load(Ordering::SeqCst),
                processing: false,
                in_flight: 0,
                child_crashed: false,
            }
        }
        fn start(&self) {
            self.running.store(true, Ordering::SeqCst);
            self.starts.fetch_add(1, Ordering::SeqCst);
        }
        fn stop(&self) {
            self.running.store(false, Ordering::SeqCst);
            self.stops.fetch_add(1, Ordering::SeqCst);
        }
        fn wake(&self) {}
    }

    /// A backup host whose upload store is empty — `SystemRestoreExecute`
    /// refuses right after the guard is taken, which is all the wiring test
    /// needs.
    struct EmptyBackupHost {
        tmp: PathBuf,
    }
    impl crate::services::backup::BackupHost for EmptyBackupHost {
        fn storage(&self) -> Arc<dyn crate::services::file_storage::StorageBackend> {
            Arc::new(crate::services::file_storage::NotConfiguredStorageBackend)
        }
        fn pixel_codec(&self) -> Arc<dyn crate::services::file_storage::PixelCodec> {
            Arc::new(crate::services::file_storage::NotConfiguredPixelCodec)
        }
        fn temp_dir(&self) -> PathBuf {
            self.tmp.clone()
        }
        fn host_dirs(&self) -> crate::services::backup::HostDirs {
            crate::services::backup::HostDirs {
                npm_plugins: None,
                themes: None,
            }
        }
        fn app_version(&self) -> String {
            "test".into()
        }
        fn now_ms(&self) -> i64 {
            0
        }
        fn store_backup(&self, _backup_id: &str, _zip_path: &std::path::Path) {}
        fn take_backup(&self, _backup_id: &str) -> Option<PathBuf> {
            None
        }
        fn store_upload(&self, _upload_id: &str, _zip_path: &std::path::Path) {}
        fn get_upload(&self, _upload_id: &str) -> Option<PathBuf> {
            None
        }
        fn remove_upload(&self, _upload_id: &str) {}
    }

    struct PumpWiringAssembler {
        pump: Arc<RecordingPump>,
        tmp: PathBuf,
    }
    impl EngineAssembler for PumpWiringAssembler {
        fn assemble(
            &self,
            _db: &Db,
            _events: &broadcast::Sender<Event>,
            _pepper: &str,
            _data_dir: &std::path::Path,
            _bus: &Arc<CreationProgressBus>,
        ) -> Result<EngineAssembly, String> {
            let mut a = EngineAssembly::shutdown_only(Box::new(NoopShutdown));
            a.job_pump = Some(self.pump.clone());
            a.backup_host = Some(Arc::new(EmptyBackupHost {
                tmp: self.tmp.clone(),
            }));
            Ok(a)
        }
    }

    #[tokio::test]
    async fn delete_data_and_restore_execute_take_the_pump_pause() {
        let base = make_instance();
        let pump = Arc::new(RecordingPump {
            running: std::sync::atomic::AtomicBool::new(true),
            starts: AtomicUsize::new(0),
            stops: AtomicUsize::new(0),
        });
        let engine = CoreEngine::boot(
            config(&base, Some(PEPPER)),
            Box::new(PumpWiringAssembler {
                pump: pump.clone(),
                tmp: base.path().join("tmp"),
            }),
            Arc::new(EmptyInstances),
        )
        .unwrap();

        // The wrong-sentinel refusal still cycles the pump: the guard is taken
        // before validation (recorded as a harmless claim-window note in the
        // round record), which is exactly what lets the wiring be proven cheap.
        match engine
            .dispatch(Request::SystemDeleteData {
                confirm: "not the sentinel".into(),
                keep_archived_character_bundles: None,
            })
            .await
        {
            Response::Error(_) => {}
            other => panic!("unexpected: {other:?}"),
        }
        assert_eq!(
            (
                pump.stops.load(Ordering::SeqCst),
                pump.starts.load(Ordering::SeqCst)
            ),
            (1, 1),
            "SystemDeleteData must stop the pump and start it again"
        );

        match engine
            .dispatch(Request::SystemRestoreExecute {
                upload_id: "00000000-0000-4000-8000-000000000000".into(),
                keep_archived_character_bundles: None,
                mode: "replace".into(),
            })
            .await
        {
            Response::Error(_) => {}
            other => panic!("unexpected: {other:?}"),
        }
        assert_eq!(
            (
                pump.stops.load(Ordering::SeqCst),
                pump.starts.load(Ordering::SeqCst)
            ),
            (2, 2),
            "SystemRestoreExecute must stop the pump and start it again"
        );
    }

    #[test]
    fn missing_main_db_refuses_boot() {
        let base = tempdir().unwrap();
        std::fs::create_dir_all(base.path().join("data")).unwrap();
        match CoreEngine::boot(
            config(&base, Some(PEPPER)),
            Box::new(NoopAssembler),
            Arc::new(EmptyInstances),
        ) {
            Err(BootError::MissingMainDb(_)) => {}
            Err(e) => panic!("wrong error: {e}"),
            Ok(_) => panic!("boot should have refused"),
        }
    }
}
