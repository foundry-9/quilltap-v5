//! The composition root: boot the engine, wire the seam-free handler set,
//! and drive the cadence (D20 — all timers live here, never in the core).
//!
//! ## The assembler
//!
//! The engine calls [`HostAssembler::assemble`] whenever the pepper becomes
//! operational (boot, or an `Unlock` dispatch after a lock). Each assembly
//! builds a fresh [`HandlerRegistry`] + [`JobRunner`] over the fresh [`Db`]
//! and spawns three tasks:
//!
//! - the **pump loop** — v4's dispatcher: `reset_orphaned_jobs` once, then
//!   pump on the enqueue wake / the next-due wake delay / the 2 s poll,
//! - the **stuck-job reset** — every 5 minutes,
//! - the **autonomous schedule tick** — v4 `scheduled-autonomous-rooms.ts`:
//!   immediately and then every 60 s, enqueue one
//!   `AUTONOMOUS_ROOM_SCHEDULE_TICK` per chat-settings user (dedupe is in
//!   the ported enqueue).
//!
//! The returned shutdown handle flips a `watch` flag; the loops exit, their
//! `Db`/runner clones drop, and the writer thread ends — that is what makes
//! the `Lock` dispatch a real teardown.
//!
//! ## The wake hook
//!
//! `queue_service::set_wake_hook` is a process-global `OnceLock` (first
//! registration wins), but assemblies come and go (lock/unlock) and tests run
//! several hosts in one process. So the host registers ONE forwarding hook
//! that fans out to a registry of weak per-assembly targets: an enqueue wakes
//! every live assembly (a spurious wake pumps an empty queue — harmless), and
//! a torn-down assembly's target self-prunes.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock, Weak};
use std::time::Duration;

use tokio::sync::{watch, Notify};

use crate::backup_services::{HostBackupServices, SystemClock};
use quilltap_core::api::{
    BootError, CoreConfig, CoreEngine, EngineAssembler, EngineAssembly, EngineShutdown, Event,
    InstanceDirectory,
};
use quilltap_core::clock::{iso_to_ms, now_unix_ms};
use quilltap_core::db::background_jobs::BackgroundJobsRepository;
use quilltap_core::db::chat_settings;
use quilltap_core::db::runtime::Db;
use quilltap_core::enclave::step::AutonomousRoomScheduleTickHandler;
use quilltap_core::services::aurora_notifications::WardrobeOutfitAnnouncementHandler;
use quilltap_core::services::conversation_render_job::ConversationRenderHandler;
use quilltap_core::services::creation_progress::CreationProgressBus;
use quilltap_core::services::danger_scan;
use quilltap_core::services::embedding_refit_job::EmbeddingRefitHandler;
use quilltap_core::services::embedding_reindex_job::EmbeddingReindexAllHandler;
use quilltap_core::services::job_runner::{
    HandlerRegistry, JobFuture, JobHandler, JobRunner, STUCK_JOB_TIMEOUT_MINUTES,
};
use quilltap_core::services::job_scheduler::{
    should_run_startup_tick, DAILY_INTERVAL_MS, DANGER_SCAN_INTERVAL_MS, POLL_INTERVAL_MS,
    RECENT_RUN_WINDOW_MS, STARTUP_GRACE_MS, STUCK_JOB_CHECK_INTERVAL_MS,
};
use quilltap_core::services::queue_service;
use quilltap_core::services::scheduled_maintenance::{run_scheduled_maintenance, TranscriptStore};

use crate::instances::InstanceRegistry;
use crate::lock;
use crate::spine::SpineFactory;
use crate::terminal::{TerminalManager, TerminalManagerConfig};

// ============================================================================
// Config
// ============================================================================

/// Host construction inputs. `new` fills the conventional defaults (env
/// pepper from `ENCRYPTION_MASTER_PEPPER`, the system timezone, the v4
/// cadences); fields are public for overriding.
pub struct HostConfig {
    /// The instance root (contains `data/`); resolve via
    /// [`crate::paths::resolve_base_dir`].
    pub base_dir: PathBuf,
    /// Reported by the `Health` dispatch.
    pub version: String,
    /// The `ENCRYPTION_MASTER_PEPPER` env pepper, if set.
    pub env_pepper: Option<String>,
    /// IANA timezone for enclave cron evaluation (v4 uses the process zone).
    pub tz: String,
    /// Override the instance-registry file (tests); `None` = the launcher's
    /// per-user location.
    pub instances_path: Option<PathBuf>,
    /// The autonomous schedule-tick cadence (v4: 60 s).
    pub autonomous_tick_ms: u64,
    /// The stuck-job reset cadence (v4: 5 min).
    pub stuck_check_ms: u64,
    /// The LLM-log cleanup sweep cadence (v4: 24 h; runs immediately at start).
    pub cleanup_interval_ms: u64,
    /// The memory-housekeeping sweep cadence (v4: 24 h; startup tick after the
    /// grace, skipped when a scheduled sweep COMPLETED within 20 h).
    pub housekeeping_interval_ms: u64,
    /// The maintenance sweep cadence (v4: 24 h; startup tick after the grace,
    /// skipped when `lastMaintenanceSweepAt` is within 20 h).
    pub maintenance_interval_ms: u64,
    /// The danger-scan enqueuer cadence (v4: 10 min; runs immediately at start;
    /// the loop does not start at all when every user's danger mode is OFF).
    pub danger_scan_interval_ms: u64,
    /// The daily sweeps' startup grace (v4: 5 min).
    pub startup_grace_ms: u64,
    /// The instance-lock heartbeat cadence (v4: 60 s).
    pub heartbeat_ms: u64,
    /// What to do when the instance lock is LOST mid-run (file vanished /
    /// foreign content): the drivers are always stopped first, then this runs.
    /// `None` = the faithful v4 default — exit the process with status 1
    /// (v4 closes the DB and `process.exit(1)`s). Tests inject a recorder.
    pub on_lock_lost: Option<Arc<dyn Fn() + Send + Sync>>,
    /// Additional job handlers (tests; the P4.1 lanes register the
    /// seam-needing ones here until they move into the built-in set).
    pub extra_handlers: Vec<(String, Arc<dyn JobHandler>)>,
    /// The chat-send spine factory (P4.2). `Some` wires the `ChatSend`
    /// dispatch driver + the model-dependent job handlers per assembly;
    /// `None` keeps the P4.0 read-only shape (ChatSend answers
    /// "chat dispatch not assembled").
    pub spine: Option<Arc<dyn SpineFactory>>,
    /// Whether each assembly runs a [`TerminalManager`] (the PTY host driver).
    /// Default true; tests that don't need PTYs may switch it off.
    pub terminal: bool,
    /// Whether a first boot seeds the sample content (v4's `seedFromImports` +
    /// `seedAvatars`: Lorian + Riya + 42 memories + Lorian's avatar), behind the
    /// zero-characters gate. **Default `true`** (v4 parity — its startup seeding
    /// is unconditional); tests that need a bare fresh instance opt out.
    pub seed_sample_content: bool,
}

impl HostConfig {
    pub fn new(base_dir: impl Into<PathBuf>) -> Self {
        Self {
            base_dir: base_dir.into(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            env_pepper: std::env::var("ENCRYPTION_MASTER_PEPPER")
                .ok()
                .filter(|p| !p.is_empty()),
            tz: crate::paths::system_timezone(),
            instances_path: None,
            autonomous_tick_ms: 60_000,
            stuck_check_ms: STUCK_JOB_CHECK_INTERVAL_MS as u64,
            cleanup_interval_ms: DAILY_INTERVAL_MS as u64,
            housekeeping_interval_ms: DAILY_INTERVAL_MS as u64,
            maintenance_interval_ms: DAILY_INTERVAL_MS as u64,
            danger_scan_interval_ms: DANGER_SCAN_INTERVAL_MS as u64,
            startup_grace_ms: STARTUP_GRACE_MS as u64,
            heartbeat_ms: lock::HEARTBEAT_INTERVAL_MS,
            on_lock_lost: None,
            extra_handlers: Vec::new(),
            spine: None,
            terminal: true,
            seed_sample_content: true,
        }
    }

    /// Wire a chat-send spine factory (chainable).
    pub fn with_spine(mut self, spine: Arc<dyn SpineFactory>) -> Self {
        self.spine = Some(spine);
        self
    }

    /// Register an extra job handler (chainable).
    pub fn with_handler(
        mut self,
        job_type: impl Into<String>,
        handler: Arc<dyn JobHandler>,
    ) -> Self {
        self.extra_handlers.push((job_type.into(), handler));
        self
    }
}

/// Host startup failures.
#[derive(Debug)]
pub enum HostError {
    /// [`Host::start`] must run inside a tokio runtime (it spawns the cadence
    /// loops).
    NoRuntime,
    Boot(BootError),
}

impl std::fmt::Display for HostError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HostError::NoRuntime => write!(f, "Host::start requires a running tokio runtime"),
            HostError::Boot(e) => write!(f, "{e}"),
        }
    }
}
impl std::error::Error for HostError {}

// ============================================================================
// The host
// ============================================================================

/// A booted instance: the engine handle plus the drivers running behind it.
/// Transports hold `core()` (it is `Clone`); dropping the `Host` does not
/// stop the drivers — dispatch `Lock` (or end the runtime) to tear down.
pub struct Host {
    core: CoreEngine,
    /// P4.9G5: the backup host services. Held on the HOST (not per-assembly) so
    /// the single-use download store survives a lock/unlock cycle, matching
    /// v4's `globalThis`-anchored map.
    backup_services: Arc<HostBackupServices>,
    /// The CURRENT assembly's terminal manager (filled on assemble, cleared
    /// on shutdown — a lock/unlock cycle swaps it).
    terminal: Arc<Mutex<Option<Arc<TerminalManager>>>>,
    base_dir: PathBuf,
}

impl Host {
    /// Resolve, provision, boot, and start driving. A locked vault boots
    /// successfully into the locked state (the unlock family is live; the
    /// drivers start on unlock).
    pub fn start(config: HostConfig) -> Result<Host, HostError> {
        let rt = tokio::runtime::Handle::try_current().map_err(|_| HostError::NoRuntime)?;

        let instances: Arc<dyn InstanceDirectory> = Arc::new(match &config.instances_path {
            Some(p) => InstanceRegistry::at(p.clone()),
            None => InstanceRegistry::at_default_location(),
        });

        let terminal_slot: Arc<Mutex<Option<Arc<TerminalManager>>>> = Arc::new(Mutex::new(None));
        // P4.9G5: one backup-services instance per HOST (see the field's note).
        let backup_services = Arc::new(HostBackupServices::new(
            config.base_dir.clone(),
            config.version.clone(),
            Arc::new(SystemClock),
        ));
        let assembler = HostAssembler {
            base_dir: config.base_dir.clone(),
            backup_services: backup_services.clone(),
            version: config.version.clone(),
            env_pepper: config.env_pepper.clone(),
            started: std::time::Instant::now(),
            spine: config.spine,
            terminal: config.terminal,
            terminal_slot: terminal_slot.clone(),
            tz: config.tz,
            autonomous_tick_ms: config.autonomous_tick_ms,
            stuck_check_ms: config.stuck_check_ms,
            cleanup_interval_ms: config.cleanup_interval_ms,
            housekeeping_interval_ms: config.housekeeping_interval_ms,
            maintenance_interval_ms: config.maintenance_interval_ms,
            danger_scan_interval_ms: config.danger_scan_interval_ms,
            startup_grace_ms: config.startup_grace_ms,
            heartbeat_ms: config.heartbeat_ms,
            on_lock_lost: config.on_lock_lost,
            extra: config.extra_handlers,
            seed_sample_content: config.seed_sample_content,
            rt,
        };

        let base_dir = config.base_dir.clone();
        let core = CoreEngine::boot(
            CoreConfig {
                base_dir: config.base_dir,
                version: config.version,
                env_pepper: config.env_pepper,
            },
            Box::new(assembler),
            instances,
        )
        .map_err(HostError::Boot)?;

        Ok(Host {
            core,
            backup_services,
            terminal: terminal_slot,
            base_dir,
        })
    }

    /// P4.9G5: the backup host services. `quilltap-web`'s byte-level download
    /// leg (`GET /api/v1/system/backup/{id}`) reads the single-use temp store
    /// through this — it serves bytes, so it has no dispatch verb.
    pub fn backup_services(&self) -> &Arc<HostBackupServices> {
        &self.backup_services
    }

    /// The boundary handle (`Clone`) transports dispatch through.
    pub fn core(&self) -> &CoreEngine {
        &self.core
    }

    /// The CURRENT assembly's terminal manager (None while locked or when
    /// `HostConfig::terminal` is off).
    pub fn terminal_manager(&self) -> Option<Arc<TerminalManager>> {
        self.terminal.lock().unwrap().clone()
    }

    /// The instance root this host serves.
    pub fn base_dir(&self) -> &std::path::Path {
        &self.base_dir
    }
}

// ============================================================================
// The wake-hook fan-out
// ============================================================================

type WakeFn = dyn Fn() + Send + Sync;

fn wake_targets() -> &'static Mutex<Vec<Weak<WakeFn>>> {
    static TARGETS: OnceLock<Mutex<Vec<Weak<WakeFn>>>> = OnceLock::new();
    TARGETS.get_or_init(|| Mutex::new(Vec::new()))
}

/// Register the one process-global forwarding hook (idempotent — the core's
/// `OnceLock` keeps the first) and add this assembly's target to the fan-out.
fn register_wake_target(target: &Arc<WakeFn>) {
    queue_service::set_wake_hook(|| {
        wake_targets()
            .lock()
            .unwrap()
            .retain(|weak| match weak.upgrade() {
                Some(t) => {
                    t();
                    true
                }
                None => false,
            });
    });
    wake_targets().lock().unwrap().push(Arc::downgrade(target));
}

// ============================================================================
// The assembler + drivers
// ============================================================================

/// Delegating wrapper so config-supplied `Arc<dyn JobHandler>`s can be
/// registered into each assembly's owned registry.
struct SharedHandler(Arc<dyn JobHandler>);

impl JobHandler for SharedHandler {
    fn handle<'a>(
        &'a self,
        db: &'a Db,
        job: &'a quilltap_core::db::background_jobs::BackgroundJob,
    ) -> JobFuture<'a> {
        self.0.handle(db, job)
    }
}

struct HostAssembler {
    base_dir: PathBuf,
    /// P4.9G5: shared with the `Host` so the web-edge download leg and the
    /// dispatch verb see the same temp store.
    backup_services: Arc<HostBackupServices>,
    /// P4.37: the almanack host seam's inputs — the app version, the env
    /// pepper (for the passphrase flag's `provision` re-read) and the process
    /// start instant (the report's honest uptime).
    version: String,
    env_pepper: Option<String>,
    started: std::time::Instant,
    spine: Option<Arc<dyn SpineFactory>>,
    terminal: bool,
    terminal_slot: Arc<Mutex<Option<Arc<TerminalManager>>>>,
    tz: String,
    autonomous_tick_ms: u64,
    stuck_check_ms: u64,
    cleanup_interval_ms: u64,
    housekeeping_interval_ms: u64,
    maintenance_interval_ms: u64,
    danger_scan_interval_ms: u64,
    startup_grace_ms: u64,
    heartbeat_ms: u64,
    on_lock_lost: Option<Arc<dyn Fn() + Send + Sync>>,
    extra: Vec<(String, Arc<dyn JobHandler>)>,
    seed_sample_content: bool,
    rt: tokio::runtime::Handle,
}

struct HostShutdown {
    stop: watch::Sender<bool>,
    /// Clears the host's terminal-manager slot for this assembly.
    terminal_slot: Arc<Mutex<Option<Arc<TerminalManager>>>>,
    /// The instance lock this assembly holds; released on shutdown (AFTER the
    /// stop flag flips, so the heartbeat loop never mistakes our own release
    /// for a lock loss).
    lock_path: PathBuf,
    /// Keeps this assembly's wake target alive; dropping it prunes the weak
    /// from the fan-out.
    _wake_target: Arc<WakeFn>,
}

impl EngineShutdown for HostShutdown {
    fn shutdown(&self) {
        let _ = self.stop.send(true);
        // Drop this assembly's terminal manager (live PTYs keep their reader
        // threads until the shells exit; new spawns need a fresh unlock).
        self.terminal_slot.lock().unwrap().take();
        // Idempotent: a second shutdown finds no file (or not ours) and no-ops.
        lock::release_instance_lock(&self.lock_path);
    }
}

impl EngineAssembler for HostAssembler {
    /// The single-instance lock, taken BEFORE the engine opens (or creates) a
    /// single partition — v4's ordering (`backend.ts` `connect()` locks ahead of
    /// `new Database`), which v5 did not have until P4.46: acquisition used to
    /// sit at the head of [`Self::assemble`], i.e. after three writable opens
    /// and their `journal_mode = TRUNCATE` header writes, and after first-run
    /// provisioning's whole DDL replay. A live conflict is a typed boot error
    /// the engine surfaces as `BootError::Assemble` (the P4.2 startup-status
    /// route carries it to the UI) — the same class as before the move.
    ///
    /// Re-entrant per PID (`lock::acquire_instance_lock`), which is what lets
    /// `Setup` claim before provisioning and claim again through `open_ready`.
    fn pre_open(&self, _data_dir: &std::path::Path) -> Result<(), String> {
        let lock_path = lock::instance_lock_path(&self.base_dir);
        // The lock file lives in `<base>/data/`, and on a brand-new install
        // NOTHING has created that directory yet — before the P4.46 reorder it
        // was `save_dbkey`'s `create_dir_all`, which now runs AFTER this claim.
        // v4 pre-creates the instance paths before `connect()` locks; mirror
        // that here or first-run `Setup` dies on `create_new` with NotFound
        // (the 4.8.2-round unification review's executed repro — every test
        // had masked it by pre-creating `data/`).
        if let Some(parent) = lock_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("create the instance data dir for the lock: {e}"))?;
        }
        // If the claim succeeds but the open/assemble behind it then fails
        // (wrong pepper, driver failure), the lock file stays behind with no
        // heartbeat: bounded on purpose — the same process re-acquires
        // re-entrantly, a same-host contender reaps the dead PID, and a
        // cross-host contender waits out at most the stale window.
        lock::acquire_instance_lock(&lock_path).map_err(|e| e.to_string())
    }

    fn assemble(
        &self,
        db: &Db,
        events: &tokio::sync::broadcast::Sender<Event>,
        pepper: &str,
        data_dir: &std::path::Path,
        bus: &Arc<CreationProgressBus>,
    ) -> Result<EngineAssembly, String> {
        // The instance lock is already held — `pre_open` above took it before
        // the engine opened anything (P4.46). `HostShutdown` releases it.
        let lock_path = lock::instance_lock_path(&self.base_dir);

        // Seed the built-in roleplay templates + provision-or-adopt the three
        // built-in mount stores (P4.4u3), on EVERY assemble/unlock — matching v4's
        // every-startup `seedBuiltInTemplates` + the mount-provisioning migrations
        // + `ensureGeneralScenariosFolder`. Idempotent by construction: a
        // pre-existing instance drift-updates its templates and ADOPTS its existing
        // stores, never duplicating. Run on a fresh OS thread so `write_blocking`
        // is legal whether `assemble` was reached from the sync boot path or an
        // async `Unlock` dispatch (`blocking_recv` panics on a tokio worker).
        seed_built_ins(db)?;

        // The gated sample-content seed (P4.4u4): on a first boot (zero-characters
        // gate) import Lorian + Riya + 42 memories + Lorian's avatar, matching a
        // fresh v4 instance. Behind the config flag (default off this lane).
        // Swallows every failure (v4: seeding never blocks startup).
        if self.seed_sample_content {
            seed_sample_content(db)?;
        }

        // The terminal manager (P4.1c) — one per assembly (it holds this
        // assembly's Db); published on the host slot for the transport's
        // terminal routes.
        let terminal_manager = if self.terminal {
            let manager = TerminalManager::new(TerminalManagerConfig::new(
                db.clone(),
                self.base_dir.join("files"),
                self.base_dir.join("logs"),
                self.base_dir.clone(),
            ));
            *self.terminal_slot.lock().unwrap() = Some(manager.clone());
            Some(manager)
        } else {
            None
        };

        // The manager doubles as `ChatGet`'s live-PTY reconcile probe (the
        // P4.2-era stub-probe deferral, closed — v4 reconciles with the real
        // `ptyManager.get`). No terminal subsystem → no probe, which the engine
        // treats as v4's empty PTY map.
        let terminal_probe = terminal_manager.clone().map(|m| {
            m as std::sync::Arc<
                dyn quilltap_core::services::ariel_notifications::TerminalLivenessProbe,
            >
        });

        // The chat-send + chat-create spines (P4.2 / P4.4u2b), when configured:
        // the ChatSend / ChatCreate drivers + the model-dependent job handlers.
        let spine_bundle = self
            .spine
            .as_ref()
            .map(|f| f.build(db, events, terminal_manager, pepper, data_dir, bus));

        // The seam-free built-in handler set (P4.0). Every other known job
        // type stays on the runner's loud fallback until its P4.1 lane wires
        // the model/host seams its handler needs.
        let mut registry = HandlerRegistry::new();
        registry.register(
            "AUTONOMOUS_ROOM_SCHEDULE_TICK",
            Box::new(AutonomousRoomScheduleTickHandler {
                tz: self.tz.clone(),
            }),
        );
        registry.register(
            "WARDROBE_OUTFIT_ANNOUNCEMENT",
            Box::new(WardrobeOutfitAnnouncementHandler),
        );
        registry.register(
            "EMBEDDING_REFIT",
            Box::new(EmbeddingRefitHandler { now_iso: None }),
        );
        // === P4.6BM ===
        // EMBEDDING_REINDEX_ALL, likewise seam-free apart from the help-tree
        // walk this crate owns. v5 has been MINTING these jobs since the
        // EMBEDDING_REFIT handler shipped (a BUILTIN refit with triggerReindex
        // enqueues one) with nothing to run them, so each retried three times
        // and died. The walk is v4's own `join(process.cwd(), 'help')`,
        // resolved once here — the tree ships with the binary and cannot change
        // under a running process. This is also the production caller
        // `help_doc_sync` has been documented as lacking.
        let help_files = std::env::current_dir()
            .map(|cwd| crate::files_store::load_help_source_files(&cwd))
            .unwrap_or_default();
        registry.register(
            "EMBEDDING_REINDEX_ALL",
            Box::new(EmbeddingReindexAllHandler {
                help_files,
                now_iso: None,
            }),
        );
        // CONVERSATION_RENDER needs no model/wire seam — only the DB — so it
        // joins the seam-free set beside EMBEDDING_REFIT rather than riding the
        // spine. Before this registration every job the manual
        // "render conversation" button minted retried three times and died.
        registry.register(
            "CONVERSATION_RENDER",
            Box::new(ConversationRenderHandler { now_iso: None }),
        );
        // === end P4.6BM ===
        // === P4.9H2A: the Matryoshka re-apply job — seam-free (DB only, no
        // provider call). Before this the embedding-profiles PUT matrix's
        // narrow arm + the ?action=reapply route minted EMBEDDING_REAPPLY_PROFILE
        // jobs with nothing to run them (each retried three times and died). ===
        registry.register(
            "EMBEDDING_REAPPLY_PROFILE",
            Box::new(
                quilltap_core::services::embedding_reapply_profile::EmbeddingReapplyProfileHandler {
                    now_iso: None,
                    millis: None,
                },
            ),
        );
        // === end P4.9H2A ===
        // === P4.43: the conversation-summaries re-mirror backfill — seam-free
        // (DB only, no model wire; it re-mirrors existing contextSummaries into
        // the vaults). Before this the Settings "Regenerate conversation
        // summaries" button minted REGENERATE_CONVERSATION_SUMMARIES jobs with
        // nothing to run them (each retried and died). ===
        registry.register(
            "REGENERATE_CONVERSATION_SUMMARIES",
            Box::new(
                quilltap_core::services::conversation_summaries_regen::RegenerateConversationSummariesHandler,
            ),
        );
        // === end P4.43 ===
        for (job_type, handler) in &self.extra {
            registry.register(job_type.clone(), Box::new(SharedHandler(handler.clone())));
        }
        let (
            chat_send,
            chat_create,
            swipe_generate,
            provider_actions,
            memory_embedding,
            courier_resolve,
            save_image_bytes,
            image_generation,
            consult,
            brahma_console_send,
            recall_replay,
            announcement_preview,
            operator_tool_runner,
            regenerate_title,
            outfit_llm_choose,
            image_describe,
            // P4.42: the web-search provider (the tools-inventory bool derives
            // from `is_some()`; the spine's own copy runs `search_web`).
            web_search,
        ) = match spine_bundle {
            Some(bundle) => {
                for (job_type, handler) in bundle.job_handlers {
                    registry.register(job_type, handler);
                }
                (
                    Some(bundle.chat_send),
                    Some(bundle.chat_create),
                    bundle.swipe_generate,
                    bundle.provider_actions,
                    bundle.memory_embedding,
                    bundle.courier_resolve,
                    bundle.save_image_bytes,
                    bundle.image_generation,
                    bundle.consult,
                    bundle.brahma_console_send,
                    bundle.recall_replay,
                    bundle.announcement_preview,
                    bundle.operator_tool_runner,
                    bundle.regenerate_title,
                    bundle.outfit_llm_choose,
                    bundle.image_describe,
                    bundle.web_search,
                )
            }
            None => (
                None, None, None, None, None, None, None, None, None, None, None, None, None, None,
                None, None, None,
            ),
        };

        let runner = JobRunner::new(db.clone(), registry);
        let (stop_tx, stop_rx) = watch::channel(false);
        let wake = Arc::new(Notify::new());
        // P4.9G1: the job-pump-control gate. `running` starts true (the pump
        // claims jobs); the tasks-queue Stop/Start control toggles it via the
        // `HostJobPump` seam. `pump_loop` checks it before claiming.
        let job_pump_running = Arc::new(std::sync::atomic::AtomicBool::new(true));

        // Enqueue wake → runner flag + pump-loop notify.
        let wake_target: Arc<WakeFn> = {
            let runner = runner.clone();
            let wake = wake.clone();
            Arc::new(move || {
                runner.wake();
                wake.notify_one();
            })
        };
        register_wake_target(&wake_target);

        self.rt.spawn(pump_loop(
            runner.clone(),
            wake.clone(),
            stop_rx.clone(),
            job_pump_running.clone(),
        ));
        self.rt.spawn(stuck_reset_loop(
            runner,
            stop_rx.clone(),
            self.stuck_check_ms,
        ));
        self.rt.spawn(autonomous_tick_loop(
            db.clone(),
            stop_rx.clone(),
            self.autonomous_tick_ms,
        ));

        // The lock heartbeat (v4: 60 s). Losing the lock stops the drivers,
        // then runs the configured handler (default: exit 1, the faithful v4
        // shutdown — see `HostConfig::on_lock_lost`).
        self.rt.spawn(heartbeat_loop(
            lock_path.clone(),
            stop_tx.clone(),
            stop_rx.clone(),
            self.heartbeat_ms,
            self.on_lock_lost.clone(),
        ));

        // The four scheduler sweeps (v4 instrumentation.ts order: cleanup →
        // housekeeping → maintenance → danger scan; the autonomous tick above
        // is the fifth).
        self.rt.spawn(cleanup_loop(
            db.clone(),
            stop_rx.clone(),
            self.cleanup_interval_ms,
        ));
        self.rt.spawn(housekeeping_loop(
            db.clone(),
            stop_rx.clone(),
            self.startup_grace_ms,
            self.housekeeping_interval_ms,
        ));
        self.rt.spawn(maintenance_loop(
            db.clone(),
            stop_rx.clone(),
            self.startup_grace_ms,
            self.maintenance_interval_ms,
            FsTranscriptStore {
                transcripts_dir: self.base_dir.join("logs").join("terminals"),
            },
            crate::files_store::LocalStorageBackend::new(self.base_dir.join("files")),
        ));
        self.rt.spawn(danger_scan_loop(
            db.clone(),
            stop_rx,
            self.danger_scan_interval_ms,
        ));

        Ok(EngineAssembly {
            // === P4.37: the Almanack host seam — LIVE (resume item 3) ===
            // Paths + honest runtime facts + the passphrase flag + version +
            // clock + the disk storage backend; the four `SystemAlmanack*`
            // verbs now reach the report pipeline in production.
            almanack_host: Some(Arc::new(
                crate::almanack_services::HostAlmanackServices::new(
                    self.base_dir.clone(),
                    self.version.clone(),
                    self.env_pepper.clone(),
                    self.tz.clone(),
                    self.started,
                    Arc::new(SystemClock),
                ),
            )),
            shutdown: Box::new(HostShutdown {
                stop: stop_tx,
                terminal_slot: self.terminal_slot.clone(),
                lock_path,
                _wake_target: wake_target,
            }),
            chat_send,
            chat_create,
            swipe_generate,
            provider_actions,
            memory_embedding,
            // P4.6y: the document-store refresh scheduler, wired LIVE to the
            // reindex/embed/stats chain (the P4.6w deferral closed). Unwired
            // (`None`) assemblies — read-only embedders, focused tests — keep
            // the loud skip at the write sites.
            mount_refresh: Some(std::sync::Arc::new(
                quilltap_core::services::mount_index::refresh::DbMountRefreshScheduler::new(
                    db.clone(),
                ),
            )),
            terminal_probe,
            // === P4.6ab: courier + chat images (wired LIVE at the P4.6ab/ac/ad
            // unification) === The spine backs the courier resolve (completion +
            // cheap executor for the settle's triggers); the production byte store
            // backs save-image. Spine-less assemblies (read-only embedders) keep
            // the loud refusal / the NotConfiguredBytes EMPTY_BYTES fallback.
            courier_resolve,
            save_image_bytes,
            // === end P4.6ab ===
            // === P4.6ai: the imageProfileGenerate un-refusal seam, wired LIVE from
            // the spine's W4.7f Real*Providers. Spine-less assemblies (read-only
            // embedders) keep `None` → the loud not-assembled refusal. ===
            image_generation,
            // === end P4.6ai ===
            // === P4.6bd: the custom-tool consult seam, wired LIVE from the
            // spine's wire-config runner (60 s timeout decorated). Spine-less
            // assemblies keep `None` → the composer/bench arms answer the loud
            // not-assembled error; the in-turn tool path stays fail-soft. ===
            consult,
            // === end P4.6bd ===
            // === P4.d13: the recall-replay runner, wired LIVE from the spine
            // (the distill costs one cheap-LLM call per replay; spine-less
            // assemblies keep None → the loud not-assembled error). ===
            recall_replay,
            // === end P4.d13 ===
            // === P4.9f1 / P4.6bf: the avatar-preview render seam, wired LIVE.
            // The render step now runs a raw portrait generation + WebP transcode
            // over the W4.7f RealImageProvider (rebuilt per request) + the
            // HostImageCodec — so the wardrobe dialog's out-of-chat Preview
            // button costs real money (one image-provider call per click).
            // Spine-less assemblies would keep `None`; the production Host always
            // has the codec, so the renderer is unconditional here. ===
            avatar_preview: Some(quilltap_core::api::wardrobe::ErasedAvatarPreview::new(
                crate::avatar_preview::HostAvatarPreviewRenderer,
            )),
            // === end P4.9f1 / P4.6bf ===
            // === P4.9I1A: the Brahma Console orchestrator send driver, wired LIVE
            // from the spine (streaming + tool runner + pricing). Spine-less
            // assemblies keep `None` → the arm answers "not assembled". ===
            brahma_console_send,
            // === end P4.9I1A ===
            // === P4.6bf (S1): the dispatch-layer blob-WebP transcoder — the live
            // production codec. The AT-UNIFY wire threads this into lane BG's
            // re-signatured `store_mount_file` handlers; until then it is carried
            // but unread (behavior unchanged: the handlers still refuse). ===
            blob_webp: Some(std::sync::Arc::new(crate::image_codec::HostImageCodec)),
            // === end P4.6bf ===
            // === P4.9G1: the job-pump control seam, wired LIVE (the host owns the
            // in-process pump loop + its running gate + wake handle). ===
            job_pump: Some(std::sync::Arc::new(crate::job_pump::HostJobPump::new(
                job_pump_running,
                wake,
            ))),
            // === end P4.9G1 ===
            // === P4.9G5: the backup host seam, wired LIVE — the disk storage
            // backend, the plugins/themes directories, the app version, the
            // system clock, and the single-use 30-minute download store (held
            // on the Host, so it survives a lock/unlock cycle the way v4's
            // globalThis-anchored map does). ===
            backup_host: Some(self.backup_services.clone()),
            // === end P4.9G5 ===
            // === P4.9E2A: the in-chat announcement-preview seam, WIRED LIVE at
            // the round's unification over the spine's completion + embedding
            // providers (`HostAnnouncementPreviewRunner`, which rebuilds the
            // logging cheap executor per call so the request's own user/chat land
            // on the `llm_logs` row, as v4 does). Spine-less assemblies keep
            // `None` → the arm answers the loud not-assembled refusal AFTER v4's
            // validation / character / profile arms, so the Insert Announcement
            // dialog renders the reason rather than breaking.
            // ⚠ LIVE means real money: one cheap-LLM call per Generate. ===
            announcement_preview,
            // === P4.9E3A: the operator run-tool seam, wired LIVE from the spine's
            // own `BuiltInToolRunner` — a tool run from the Run Tool modal behaves
            // exactly as it does mid-turn (scrollback + consult included).
            // Spine-less assemblies keep `None` → the `ChatRunTool` arm answers the
            // loud not-assembled refusal AFTER v4's deny-list and chat arms. ===
            operator_tool_runner,
            // The manual title regeneration, wired LIVE from the spine's
            // completion provider + a per-call logging cheap executor.
            // ⚠ one cheap-LLM call per Regenerate Title.
            regenerate_title,
            // === end P4.9E3A ===
            // === end P4.9E2A ===
            // === P4.42: the web-search provider, wired LIVE from the spine (built
            // iff SERPER_API_KEY is set; the plugin-registry half stays deferred).
            // The engine derives the tools-inventory `web_search_configured` bool
            // from `web_search.is_some()`, and the spine's own copy of this same
            // provider runs `search_web` — so advertised and executed cannot
            // disagree (the dogfood finding). Spine-less assemblies (read-only
            // embedders) get `None` → the tool is advertised unavailable AND
            // refuses, consistently. ===
            web_search,
            // The out-of-create llm_choose pick, LIVE from the spine's
            // completion provider (⚠ one cheap-LLM call per pick); spine-less
            // assemblies keep None → the default-outfit fallback.
            outfit_llm_choose,
            // === end P4.9E3B ===
            // === P4.9E4A: the attach-mount-file vision describe, wired LIVE from
            // the spine's completion provider + the host image codec
            // (`HostImageDescribeRunner`). Spine-less assemblies keep `None` →
            // the describe ladder resolves to `''` and the attach STILL
            // SUCCEEDS, which is v4's own posture for every describe failure.
            // ⚠ LIVE means real money: one vision-LLM call per attach of an
            // image with neither a cached description nor kept-image markdown. ===
            image_describe,
            // === end P4.9E4A ===
        })
    }
}

/// Seed the built-in roleplay templates + provision-or-adopt the three built-in
/// mount stores (P4.4u3, families 1 & 2) through the writer thread, and run the
/// main partition's boot repairs. Spawned on a fresh OS thread and joined so
/// `write_blocking` is legal from either the sync boot path or an async
/// `Unlock` dispatch. The mount families are skipped on a main-only instance
/// (no mount-index partition).
fn seed_built_ins(db: &Db) -> Result<(), String> {
    use quilltap_core::db::DbError;
    use quilltap_core::services::mount_index::general_state;
    use quilltap_core::services::{builtin_mounts, builtin_templates};

    let db = db.clone();
    std::thread::spawn(move || -> Result<(), DbError> {
        db.write_blocking(|ws| {
            let main = ws.main().connection();
            builtin_templates::seed_built_in_templates(main)?;
            // v4 `e3a9654f`'s migration `anchor-fictional-clock-base-v1`,
            // re-homed as a boot repair because v5's migration runner is
            // deferred — the same shape as the mount-index case repair below.
            // Backfills `timestampConfig.fictionalBaseRealTime` from each
            // chat's own `createdAt`, so a story clock created before the
            // write-path fix resumes where 1:1 tracking would have put it
            // instead of staying frozen. Idempotent; a no-op on an instance
            // with no unanchored fictional clocks.
            quilltap_core::db::fictional_clock_anchor_repair::anchor_fictional_clock_bases(
                main,
                quilltap_core::clock::now_unix_ms(),
            )?;
            // === P4.D63 (v4 `d553f72a`, migration
            // `add-character-archive-fields-v1`) ===
            // The three `characters` archive columns, re-homed from v4's
            // migration runner to a boot repair for the same reason as the
            // clock anchor above. Fresh instances already carry them (the D23
            // re-dump); an existing instance gains them here, so every
            // instance v5 boots can hold a tombstone. Idempotent; a no-op
            // after the first boot.
            quilltap_core::db::character_archive_repair::ensure_character_archive_columns(main)?;
            // === end P4.D63 ===
            // === P4.D73 (v4 4.8.2, migrations `add-composer-emoji-field-v1`,
            // `add-composer-unicode-field-v1`,
            // `add-smart-typography-settings-field-v1`) ===
            // The three `chat_settings` composer/typography columns, re-homed
            // from v4's migration runner for the same reason. Load-bearing
            // rather than cosmetic here: the read tolerates absence with the
            // Zod default and the write drops the column, so without this the
            // toggles would silently never persist on an existing instance.
            quilltap_core::db::chat_settings_composer_repair::ensure_chat_settings_composer_columns(
                main,
            )?;
            // === end P4.D73 ===
            // === P4.D77 (v4 `24633026`, migration
            // `create-help-doc-chunks-table-v1`) ===
            // The `help_doc_chunks` table itself, re-homed from v4's migration
            // runner for the same reason as the repairs above. An upgraded
            // instance matches every help-doc content hash, so the sync would
            // never slice it and section search would silently never engage —
            // the backfill in `ensure_help_docs_synced` handles that, but it
            // needs a table to count. Fresh instances already carry the
            // `generateDDL` shape (the D23 re-dump); this gives an existing one
            // the MIGRATION shape, exactly as v4's own migration would.
            quilltap_core::db::help_doc_chunks_repair::ensure_help_doc_chunks_table(main)?;
            // === end P4.D77 ===
            // === P4.6BM (replaces the P4.6BL stand-in) ===
            // v4's startup reconcile (`instrumentation.ts` PHASE 3.6): scan for
            // chats the Scriptorium pipeline left half-finished — arm (A) real
            // messages but no rendered Markdown, arm (B) recoverable
            // un-embedded interchange chunks — and re-enqueue a
            // CONVERSATION_RENDER for each. The handler re-chunks (preserving
            // existing embeddings) and re-enqueues the missing embeds, so both
            // arms heal. STALE chats are excluded (P4.D25 / v4 a0243abd): the
            // cache collapse cold-tiers quiet chats into exactly the state this
            // scan reads as damage, so healing them here re-embedded the whole
            // cold tier on every boot at real cost. This REPLACES the P4.6BL v5-only direct-embed repair,
            // which existed only because that handler was unported; the
            // coverage argument is in the reconcile's module doc. No-op on a
            // healthy instance; returns zeros (never fails the boot) when a
            // lazily-created table is absent.
            let reconcile =
                quilltap_core::services::conversation_render_reconcile::reconcile_conversation_rendering(
                    main,
                    quilltap_core::clock::now_unix_ms(),
                );
            // The gate is `incomplete_chats > 0`, NOT `enqueued > 0` (dogfood
            // finding, `dogfood-findings.md`): the healthy P4.D25 outcome is
            // `enqueued` ≈ 0 with `skipped_stale` large, and the old gate printed
            // nothing at all for it — so the whole signature of the stale-skip fix
            // was invisible in the field. v4 logs its "found incomplete
            // conversations" line before the loop and its completion line
            // unconditionally, so this is also the nearer shape. Log output sits
            // outside the differential contract (P4.18).
            if reconcile.incomplete_chats > 0 {
                tracing::info!(
                    target: "quilltap::boot",
                    incomplete_chats = reconcile.incomplete_chats,
                    enqueued = reconcile.enqueued,
                    reused = reconcile.reused,
                    failed = reconcile.failed,
                    skipped_stale = reconcile.skipped_stale,
                    "Conversation render reconciliation complete",
                );
            }
            // === end P4.6BM ===
            // === P4.d27 (v4 `7391404e`) ===
            // v4's PHASE 3.7, immediately behind 3.6 and inside the same closure
            // so the order is preserved. One embedding standard per instance:
            // delete non-conforming vector-index entries, snap the index meta,
            // converge stale chats to the cold tier, and enqueue ONE deduped
            // `mismatched-dim` reindex for whatever still needs re-embedding.
            // COUNT-only (nothing hydrated) on a conforming corpus, and the
            // repair is enqueued rather than run inline, so a big backlog cannot
            // block the loading screen. Never fails the boot.
            //
            // The mount-index connection is passed for fidelity with v4's call
            // shape; v4's own guard reads `doc_mount_points` from the MAIN
            // database, where that table does not live, so the mount-chunk count
            // is dead in v4 and reproduced dead here — see the module doc's ⚠.
            let dim_reconcile =
                quilltap_core::services::embedding_dimension_reconcile::reconcile_embedding_dimensions(
                    main,
                    ws.mount_index().map(|mi| mi.connection()),
                    quilltap_core::clock::now_unix_ms(),
                );
            // Same lesson as the gate above: report the pass whenever it had a
            // profile to enforce, so a healthy "corpus conforms" is visible too.
            if let Some(target) = dim_reconcile.target_dimensions {
                tracing::info!(
                    target: "quilltap::boot",
                    target_dimensions = target,
                    vector_entries_deleted = dim_reconcile.vector_entries_deleted,
                    vector_index_meta_fixed = dim_reconcile.vector_index_meta_fixed,
                    stale_chunk_embeddings_cleared = dim_reconcile.stale_chunk_embeddings_cleared,
                    mismatched_memories = dim_reconcile.mismatched.memories,
                    mismatched_conversation_chunks = dim_reconcile.mismatched.conversation_chunks,
                    mismatched_help_docs = dim_reconcile.mismatched.help_docs,
                    reindex_enqueued = dim_reconcile.reindex_enqueued,
                    "Embedding dimension reconciliation complete",
                );
            } else if let Some(reason) = dim_reconcile.skipped_reason {
                tracing::info!(
                    target: "quilltap::boot",
                    reason = reason.as_str(),
                    "Embedding dimension reconciliation skipped",
                );
            }
            // === end P4.d27 ===
            if let Some(mi) = ws.mount_index() {
                let mount_index = mi.connection();
                builtin_mounts::ensure_builtin_mounts(main, mount_index)?;
                builtin_mounts::ensure_general_scenarios_folder(main, mount_index)?;
                // Companion (v4 instrumentation.ts Phase 3 tail, `f48f34dc`):
                // ensure the general mount's root state.json (the bottom tier
                // of the state cascade). Idempotent; never heals existing
                // content; warn-and-continue on error (v4's try/catch).
                match general_state::ensure_general_state_file(main, mount_index) {
                    Ok(true) => tracing::info!(
                        target: "quilltap::boot",
                        "Seeded general state.json in the Quilltap General mount",
                    ),
                    Ok(false) => {}
                    Err(e) => tracing::warn!(
                        target: "quilltap::boot",
                        error = %e,
                        "Error ensuring general state.json, continuing startup",
                    ),
                }
            }
            Ok(())
        })
    })
    .join()
    .map_err(|_| "built-in seed thread panicked".to_string())?
    .map_err(|e| format!("built-in seed failed: {e}"))
}

/// The gated sample-content seed (P4.4u4): v4's `seedFromImports` + `seedAvatars`
/// tail, behind the zero-characters gate, through the writer thread with the host
/// image codec. Spawned on a fresh OS thread and joined (like [`seed_built_ins`])
/// so `write_blocking` is legal from either the sync boot path or an async
/// `Unlock` dispatch. Requires the mount-index partition (vault writes); a
/// main-only instance is a no-op. Never fails the boot — v4's seeding is
/// swallow-and-continue, so the collected warnings are dropped here.
fn seed_sample_content(db: &Db) -> Result<(), String> {
    use crate::image_codec::HostImageCodec;
    use quilltap_core::db::DbError;
    use quilltap_core::services::quilltap_import::seed;

    let db = db.clone();
    std::thread::spawn(move || -> Result<(), DbError> {
        db.write_blocking(|ws| {
            let main = ws.main().connection();
            if let Some(mi) = ws.mount_index() {
                let mount = mi.connection();
                // The report's warnings are v4's swallowed per-item log lines; core
                // has no logger, so they are dropped here (the boot never blocks).
                let _report = seed::seed_sample_content(main, mount, &HostImageCodec);
            }
            Ok(())
        })
    })
    .join()
    .map_err(|_| "sample-content seed thread panicked".to_string())?
    .map_err(|e| format!("sample-content seed failed: {e}"))
}

/// v4's dispatcher loop over the ported [`JobRunner::pump_claim`]:
/// orphan-reset once, then pump on wake / next-due delay / the 2 s poll.
async fn pump_loop(
    runner: JobRunner,
    wake: Arc<Notify>,
    mut stop: watch::Receiver<bool>,
    running: Arc<std::sync::atomic::AtomicBool>,
) {
    // v4 job-dispatcher.ts:113 `resetOrphanedJobs().catch(err =>
    // log.error('Error resetting orphaned jobs at startup', …))`.
    match runner.reset_orphaned_jobs().await {
        Ok(0) => {}
        Ok(n) => {
            tracing::info!(target: "quilltap::jobs", count = n, "Reset orphaned jobs at startup")
        }
        Err(e) => tracing::error!(
            target: "quilltap::jobs",
            error = %e,
            "Error resetting orphaned jobs at startup",
        ),
    }
    loop {
        if *stop.borrow() {
            break;
        }
        // P4.9G1: the tasks-queue Stop control clears `running`; while stopped
        // the loop claims no new jobs and just waits for a wake / poll / stop.
        if !running.load(std::sync::atomic::Ordering::Relaxed) {
            tokio::select! {
                _ = wake.notified() => {}
                res = stop.changed() => {
                    if res.is_err() || *stop.borrow() {
                        break;
                    }
                }
                _ = tokio::time::sleep(Duration::from_millis(POLL_INTERVAL_MS.max(1) as u64)) => {}
            }
            continue;
        }
        let outcome = runner.pump_claim().await;
        if *stop.borrow() {
            break;
        }
        // A wake that arrived during the pump means new work: go again now.
        if runner.take_wake_request() {
            continue;
        }
        let delay_ms = outcome.next_wake_ms.unwrap_or(POLL_INTERVAL_MS).max(1) as u64;
        tokio::select! {
            _ = wake.notified() => {}
            res = stop.changed() => {
                if res.is_err() || *stop.borrow() {
                    break;
                }
            }
            _ = tokio::time::sleep(Duration::from_millis(delay_ms)) => {}
        }
    }
}

/// The 5-minute stuck-PROCESSING reset (v4's stuck-job sweep; the ported
/// `tick_stuck_reset` body).
async fn stuck_reset_loop(runner: JobRunner, mut stop: watch::Receiver<bool>, interval_ms: u64) {
    loop {
        tokio::select! {
            res = stop.changed() => {
                if res.is_err() || *stop.borrow() {
                    break;
                }
            }
            _ = tokio::time::sleep(Duration::from_millis(interval_ms.max(1))) => {}
        }
        if *stop.borrow() {
            break;
        }
        // v4 job-dispatcher.ts:106 `resetStuckJobs().catch(err =>
        // log.error('Error in stuck-job sweep', …))`.
        match runner.tick_stuck_reset(STUCK_JOB_TIMEOUT_MINUTES).await {
            Ok(0) => {}
            Ok(n) => tracing::warn!(
                target: "quilltap::jobs",
                count = n,
                "Reset stuck jobs",
            ),
            Err(e) => tracing::error!(
                target: "quilltap::jobs",
                error = %e,
                "Error in stuck-job sweep",
            ),
        }
    }
}

/// v4 `scheduled-autonomous-rooms.ts`: immediately and then every 60 s,
/// enqueue one schedule-tick job per chat-settings user. Per-user errors are
/// swallowed (v4 warns and continues); a missing `chat_settings` table (a
/// bare fixture) yields no users and the tick is a no-op.
async fn autonomous_tick_loop(db: Db, mut stop: watch::Receiver<bool>, interval_ms: u64) {
    loop {
        if *stop.borrow() {
            break;
        }
        let users = db
            .read_main(chat_settings::find_all_scheduler_settings)
            .unwrap_or_default();
        for user in users {
            let _ = queue_service::enqueue_autonomous_room_schedule_tick(&db, &user.user_id).await;
        }
        tokio::select! {
            res = stop.changed() => {
                if res.is_err() || *stop.borrow() {
                    break;
                }
            }
            _ = tokio::time::sleep(Duration::from_millis(interval_ms.max(1))) => {}
        }
    }
}

/// Sleep `ms` or wake on stop; returns `false` when the loop should exit.
async fn sleep_or_stop(stop: &mut watch::Receiver<bool>, ms: u64) -> bool {
    tokio::select! {
        res = stop.changed() => {
            if res.is_err() || *stop.borrow() {
                return false;
            }
            true
        }
        _ = tokio::time::sleep(Duration::from_millis(ms.max(1))) => !*stop.borrow(),
    }
}

/// The instance-lock heartbeat (v4's 60 s `setInterval` body): verify
/// ownership + rewrite `lastHeartbeat`. On LOSS (file vanished / foreign
/// content) the drivers stop first (the stop flag), then the configured
/// handler runs — default `std::process::exit(1)`, the faithful v4 shutdown.
/// Our own shutdown's release flips the stop flag BEFORE unlinking, so the
/// post-tick stop check keeps a release from reading as a loss.
async fn heartbeat_loop(
    lock_path: PathBuf,
    stop_tx: watch::Sender<bool>,
    mut stop: watch::Receiver<bool>,
    interval_ms: u64,
    on_lock_lost: Option<Arc<dyn Fn() + Send + Sync>>,
) {
    loop {
        if !sleep_or_stop(&mut stop, interval_ms).await {
            break;
        }
        if !lock::heartbeat_tick(&lock_path) {
            if *stop.borrow() {
                break; // our own release, not a takeover
            }
            let _ = stop_tx.send(true);
            match &on_lock_lost {
                Some(handler) => handler(),
                None => std::process::exit(1),
            }
            break;
        }
    }
}

/// v4 `scheduled-cleanup.ts`: run the LLM-log cleanup enqueuer immediately at
/// startup, then every 24 h. Errors are swallowed (v4 catches + logs).
async fn cleanup_loop(db: Db, mut stop: watch::Receiver<bool>, interval_ms: u64) {
    loop {
        if *stop.borrow() {
            break;
        }
        let _ = queue_service::run_scheduled_cleanup(&db).await;
        if !sleep_or_stop(&mut stop, interval_ms).await {
            break;
        }
    }
}

/// v4 `runStartupHousekeepingTick`'s short-circuit: skip the startup tick when
/// a COMPLETED `MEMORY_HOUSEKEEPING` job with `payload.reason === 'scheduled'`
/// finished within the 20 h window (peeking the 50 most recent rows). A check
/// failure runs anyway (v4 warns + runs).
fn should_run_startup_housekeeping(db: &Db, now_ms: i64) -> bool {
    let recent = db.read_main(|conn| {
        BackgroundJobsRepository::new(conn).find_recent_by_type("MEMORY_HOUSEKEEPING", 50)
    });
    let Ok(jobs) = recent else {
        return true;
    };
    let cutoff = now_ms - RECENT_RUN_WINDOW_MS;
    !jobs.iter().any(|job| {
        if job.status != "COMPLETED" {
            return false;
        }
        let scheduled = serde_json::from_str::<serde_json::Value>(&job.payload)
            .ok()
            .and_then(|p| p.get("reason").and_then(|r| r.as_str().map(str::to_string)))
            == Some("scheduled".to_string());
        if !scheduled {
            return false;
        }
        iso_to_ms(&job.updated_at).map(|ts| ts >= cutoff) == Some(true)
    })
}

/// v4 `scheduled-housekeeping.ts`: a 5-minute-grace startup tick (skipped when
/// a scheduled sweep COMPLETED within 20 h), then the 24 h cadence.
async fn housekeeping_loop(
    db: Db,
    mut stop: watch::Receiver<bool>,
    grace_ms: u64,
    interval_ms: u64,
) {
    if !sleep_or_stop(&mut stop, grace_ms).await {
        return;
    }
    if should_run_startup_housekeeping(&db, now_unix_ms()) {
        let _ = queue_service::run_scheduled_housekeeping(&db).await;
    }
    loop {
        if !sleep_or_stop(&mut stop, interval_ms).await {
            break;
        }
        let _ = queue_service::run_scheduled_housekeeping(&db).await;
    }
}

/// The transcript-file half of the terminal cleanup (the core's
/// [`TranscriptStore`] seam): unlink the row's `transcriptPath`, else the
/// default `<logsDir>/terminals/<id>.log`. ENOENT is swallowed and NOT counted
/// (v4's "already gone is success"); other unlink errors are warned-equivalent
/// (not counted, sweep continues).
pub struct FsTranscriptStore {
    /// `<base>/logs/terminals` (v4 `getLogsDir()` + `'terminals'`).
    pub transcripts_dir: PathBuf,
}

impl TranscriptStore for FsTranscriptStore {
    fn unlink_transcript(&self, session_id: &str, transcript_path: Option<&str>) -> bool {
        let path = transcript_path
            .filter(|p| !p.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| self.transcripts_dir.join(format!("{session_id}.log")));
        std::fs::remove_file(&path).is_ok()
    }
}

/// v4 `scheduled-maintenance.ts`: a 5-minute-grace startup tick (skipped when
/// `lastMaintenanceSweepAt` is within the 20 h window; a read failure runs
/// anyway), then the 24 h cadence.
async fn maintenance_loop(
    db: Db,
    mut stop: watch::Receiver<bool>,
    grace_ms: u64,
    interval_ms: u64,
    transcripts: FsTranscriptStore,
    backend: crate::files_store::LocalStorageBackend,
) {
    if !sleep_or_stop(&mut stop, grace_ms).await {
        return;
    }
    let now = now_unix_ms();
    let last = db
        .read_main(quilltap_core::db::instance_settings::get_last_maintenance_sweep_at)
        .unwrap_or(None); // read failure → run anyway (v4 warns + runs)
    if should_run_startup_tick(now, last) {
        let _ = run_scheduled_maintenance(&db, now, &transcripts, &backend).await;
    }
    loop {
        if !sleep_or_stop(&mut stop, interval_ms).await {
            break;
        }
        let _ = run_scheduled_maintenance(&db, now_unix_ms(), &transcripts, &backend).await;
    }
}

/// v4 `scheduled-danger-scan.ts`: the all-users-OFF pre-check gates STARTING
/// the loop at all (a check failure also skips — v4 warns and returns); when
/// enabled, scan immediately and then every 10 min. Sweep errors are swallowed
/// (v4 catches + logs).
async fn danger_scan_loop(db: Db, mut stop: watch::Receiver<bool>, interval_ms: u64) {
    match danger_scan::any_user_danger_enabled(&db).await {
        Ok(true) => {}
        Ok(false) | Err(_) => return,
    }
    loop {
        if *stop.borrow() {
            break;
        }
        let _ = danger_scan::run_scheduled_danger_scan(&db).await;
        if !sleep_or_stop(&mut stop, interval_ms).await {
            break;
        }
    }
}
