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
    BootError, CoreConfig, CoreEngine, EngineAssembler, EngineShutdown, InstanceDirectory,
};
use quilltap_core::db::chat_settings;
use quilltap_core::db::runtime::Db;
use quilltap_core::enclave::step::AutonomousRoomScheduleTickHandler;
use quilltap_core::services::aurora_notifications::WardrobeOutfitAnnouncementHandler;
use quilltap_core::services::embedding_refit_job::EmbeddingRefitHandler;
use quilltap_core::services::job_runner::{
    HandlerRegistry, JobFuture, JobHandler, JobRunner, STUCK_JOB_TIMEOUT_MINUTES,
};
use quilltap_core::services::job_scheduler::{POLL_INTERVAL_MS, STUCK_JOB_CHECK_INTERVAL_MS};
use quilltap_core::services::queue_service;

use crate::instances::InstanceRegistry;

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
    /// Additional job handlers (tests; the P4.1 lanes register the
    /// seam-needing ones here until they move into the built-in set).
    pub extra_handlers: Vec<(String, Arc<dyn JobHandler>)>,
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
            extra_handlers: Vec::new(),
        }
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

        let assembler = HostAssembler {
            tz: config.tz,
            autonomous_tick_ms: config.autonomous_tick_ms,
            stuck_check_ms: config.stuck_check_ms,
            extra: config.extra_handlers,
            rt,
        };

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

        Ok(Host { core })
    }

    /// The boundary handle (`Clone`) transports dispatch through.
    pub fn core(&self) -> &CoreEngine {
        &self.core
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
    tz: String,
    autonomous_tick_ms: u64,
    stuck_check_ms: u64,
    extra: Vec<(String, Arc<dyn JobHandler>)>,
    rt: tokio::runtime::Handle,
}

struct HostShutdown {
    stop: watch::Sender<bool>,
    /// Keeps this assembly's wake target alive; dropping it prunes the weak
    /// from the fan-out.
    _wake_target: Arc<WakeFn>,
}

impl EngineShutdown for HostShutdown {
    fn shutdown(&self) {
        let _ = self.stop.send(true);
    }
}

impl EngineAssembler for HostAssembler {
    fn assemble(&self, db: &Db) -> Result<Box<dyn EngineShutdown>, String> {
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
            stop_rx,
            self.autonomous_tick_ms,
        ));

        Ok(Box::new(HostShutdown {
            stop: stop_tx,
            _wake_target: wake_target,
        }))
    }
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
