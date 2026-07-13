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
use quilltap_core::services::creation_progress::CreationProgressBus;
use quilltap_core::services::danger_scan;
use quilltap_core::services::embedding_refit_job::EmbeddingRefitHandler;
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
        let assembler = HostAssembler {
            base_dir: config.base_dir.clone(),
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
            terminal: terminal_slot,
            base_dir,
        })
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
    fn assemble(
        &self,
        db: &Db,
        events: &tokio::sync::broadcast::Sender<Event>,
        pepper: &str,
        data_dir: &std::path::Path,
        bus: &Arc<CreationProgressBus>,
    ) -> Result<EngineAssembly, String> {
        // The single-instance lock (v4 acquires at backend init — here, when
        // the databases open). A live conflict is a typed boot error the
        // engine surfaces as `BootError::Assemble` (the P4.2 startup-status
        // route carries it to the UI).
        let lock_path = lock::instance_lock_path(&self.base_dir);
        lock::acquire_instance_lock(&lock_path).map_err(|e| e.to_string())?;

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
        for (job_type, handler) in &self.extra {
            registry.register(job_type.clone(), Box::new(SharedHandler(handler.clone())));
        }
        let (chat_send, chat_create, swipe_generate, provider_actions, memory_embedding) =
            match spine_bundle {
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
                    )
                }
                None => (None, None, None, None, None),
            };

        let runner = JobRunner::new(db.clone(), registry);
        let (stop_tx, stop_rx) = watch::channel(false);
        let wake = Arc::new(Notify::new());

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

        self.rt
            .spawn(pump_loop(runner.clone(), wake, stop_rx.clone()));
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
        ));
        self.rt.spawn(danger_scan_loop(
            db.clone(),
            stop_rx,
            self.danger_scan_interval_ms,
        ));

        Ok(EngineAssembly {
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
            // === P4.6ab: courier + chat images ===
            // The courier-resolve driver + the save-image bytes seam land with the
            // sibling P4.6ac SPA integration (the unification wire); an unwired host
            // answers the loud refusal / the NotConfiguredBytes EMPTY_BYTES fallback.
            courier_resolve: None,
            save_image_bytes: None,
            // === end P4.6ab ===
        })
    }
}

/// Seed the built-in roleplay templates + provision-or-adopt the three built-in
/// mount stores (P4.4u3, families 1 & 2) through the writer thread. Spawned on a
/// fresh OS thread and joined so `write_blocking` is legal from either the sync
/// boot path or an async `Unlock` dispatch. The mount families are skipped on a
/// main-only instance (no mount-index partition).
fn seed_built_ins(db: &Db) -> Result<(), String> {
    use quilltap_core::db::DbError;
    use quilltap_core::services::{builtin_mounts, builtin_templates};

    let db = db.clone();
    std::thread::spawn(move || -> Result<(), DbError> {
        db.write_blocking(|ws| {
            let main = ws.main().connection();
            builtin_templates::seed_built_in_templates(main)?;
            if let Some(mi) = ws.mount_index() {
                let mount_index = mi.connection();
                builtin_mounts::ensure_builtin_mounts(main, mount_index)?;
                builtin_mounts::ensure_general_scenarios_folder(main, mount_index)?;
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
async fn pump_loop(runner: JobRunner, wake: Arc<Notify>, mut stop: watch::Receiver<bool>) {
    let _ = runner.reset_orphaned_jobs().await;
    loop {
        if *stop.borrow() {
            break;
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
        let _ = runner.tick_stuck_reset(STUCK_JOB_TIMEOUT_MINUTES).await;
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
) {
    if !sleep_or_stop(&mut stop, grace_ms).await {
        return;
    }
    let now = now_unix_ms();
    let last = db
        .read_main(quilltap_core::db::instance_settings::get_last_maintenance_sweep_at)
        .unwrap_or(None); // read failure → run anyway (v4 warns + runs)
    if should_run_startup_tick(now, last) {
        let _ = run_scheduled_maintenance(&db, now, &transcripts).await;
    }
    loop {
        if !sleep_or_stop(&mut stop, interval_ms).await {
            break;
        }
        let _ = run_scheduled_maintenance(&db, now_unix_ms(), &transcripts).await;
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
