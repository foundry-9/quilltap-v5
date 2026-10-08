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
use quilltap_core::db::table_shape::EnsureFailures;
use quilltap_core::enclave::step::AutonomousRoomScheduleTickHandler;
use quilltap_core::realtime::bus::BusSpawner;
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
    /// IANA timezone NAME for the CALENDAR paths — enclave cron evaluation and
    /// every already-threaded `server_tz` (v4 uses the process zone). `new()`
    /// derives it from [`Self::display_zone`]: the host's zone has ONE read.
    /// Set both through [`Self::set_display_zone`].
    pub tz: String,
    /// The host's display zone as a VALUE (P4.127; threaded everywhere by
    /// P4.140's Option V) — what v4's zone-less `toLocale*` renders resolve.
    /// Read ONCE, here, and injected into `CoreConfig`, the render-job handler,
    /// the Almanack and the chat spine (every turn, swipe, greeting, autonomous
    /// step and tool runner); a zone with no IANA name (a POSIX `TZ` rule) keeps
    /// its real offsets on every display path while `tz` falls back to `"UTC"`
    /// for the calendar ones.
    pub display_zone: quilltap_core::host_zone::TimeZone,
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
        // The host's ONE zone read (P4.127); `tz` below is derived from it.
        let display_zone = quilltap_core::host_zone::system_display_zone();
        Self {
            base_dir: base_dir.into(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            env_pepper: std::env::var("ENCRYPTION_MASTER_PEPPER")
                .ok()
                .filter(|p| !p.is_empty()),
            tz: quilltap_core::host_zone::zone_name(&display_zone).to_string(),
            display_zone,
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

    /// Set the host's zone (P4.140): BOTH the display VALUE and the calendar
    /// `tz` NAME derived from it ([`quilltap_core::host_zone::zone_name`]), so
    /// the two never disagree — a test that pins `tz` alone leaves the value on
    /// the machine's zone, which every threaded display entry now reads.
    pub fn set_display_zone(&mut self, zone: quilltap_core::host_zone::TimeZone) {
        self.tz = quilltap_core::host_zone::zone_name(&zone).to_string();
        self.display_zone = zone;
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
    /// P4.D248: the CURRENT assembly's structural problems (v4's
    /// `startupState.structuralProblems`, `e5c6bd0c0`) — replaced on every
    /// assemble, read by `/health`'s `structure` service.
    structural_problems: Arc<Mutex<Vec<String>>>,
    base_dir: PathBuf,
}

impl Host {
    /// Resolve, provision, boot, and start driving. A locked vault boots
    /// successfully into the locked state (the unlock family is live; the
    /// drivers start on unlock).
    pub fn start(config: HostConfig) -> Result<Host, HostError> {
        let rt = tokio::runtime::Handle::try_current().map_err(|_| HostError::NoRuntime)?;
        let rt_handle = rt.clone();

        let instances: Arc<dyn InstanceDirectory> = Arc::new(match &config.instances_path {
            Some(p) => InstanceRegistry::at(p.clone()),
            None => InstanceRegistry::at_default_location(),
        });

        let terminal_slot: Arc<Mutex<Option<Arc<TerminalManager>>>> = Arc::new(Mutex::new(None));
        let structural_problems: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
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
            structural_problems: structural_problems.clone(),
            tz: config.tz,
            display_zone: config.display_zone.clone(),
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
                display_zone: config.display_zone,
            },
            Box::new(assembler),
            instances,
        )
        .map_err(HostError::Boot)?;

        // Arm the realtime invalidation bus (P4.D124). The composition root is
        // the right place and the only possible one: the bus needs BOTH the
        // engine's event sender and a way to schedule its 250 ms coalescing
        // window, and `quilltap-core` deliberately ships no tokio scheduler
        // (the same STOP rule `services::job_runner` states — the core decides
        // *when*, the host *schedules*). Publishing before this point is a
        // silent no-op by design, which is what the CLI's direct-core mode and
        // every unit test rely on.
        //
        // The task is detached: a pending hint must never hold the process open
        // (v4's `timer.unref?.()`), and the runtime drops it at shutdown.
        {
            let rt = rt_handle.clone();
            let spawner: BusSpawner = Arc::new(move |fut| {
                rt.spawn(fut);
            });
            quilltap_core::realtime::bus::arm_realtime_bus(core.event_sender().clone(), spawner);
        }

        // === P4.120: the fire-and-forget spawner. v4's `void call().catch(warn)`
        // (the chat upload's auto-describe, the photo save's embedding enqueue)
        // needs a runtime to carry the future; the core ships no scheduler, so
        // the composition root arms one — the bus's arrangement, detached for the
        // same reason (a background describe must never hold the process open).
        // Unarmed engines (the harness, the CLI's direct mode) skip those fires
        // with a DEBUG line. ===
        {
            let rt = rt_handle.clone();
            quilltap_core::background::arm_background_spawner(Arc::new(move |fut| {
                rt.spawn(fut);
            }));
        }
        // === end P4.120 ===

        Ok(Host {
            core,
            backup_services,
            terminal: terminal_slot,
            structural_problems,
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

    /// The structural problems the CURRENT assembly's PHASE 3.1 pass found
    /// (P4.D248, v4 `startupState.getStructuralProblems()`) — a copy, as v4's
    /// getter copies. Empty while locked (no assembly yet) and on a sound boot.
    pub fn structural_problems(&self) -> Vec<String> {
        self.structural_problems.lock().unwrap().clone()
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
    /// P4.D248: shared with the `Host` (see its field).
    structural_problems: Arc<Mutex<Vec<String>>>,
    tz: String,
    display_zone: quilltap_core::host_zone::TimeZone,
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

/// One assembly's ordered teardown, shared by two callers: the engine's own
/// `EngineShutdown` (Lock / passphrase change / drop) and the heartbeat
/// loop's lock-loss arm.
///
/// **v4 `25f534c0b` (bug 126), the audited half.** v4's heartbeat used to
/// reach its database close through a dynamic `require('./client')`, which did
/// not survive bundling into the standalone server: it threw and the process
/// exited with the WAL unmerged. The fix registers the SAME ordered teardown
/// SIGTERM and SIGINT use, inward, and runs it before exiting. v5's `None`
/// arm used to `std::process::exit(1)` straight out of the heartbeat loop,
/// reaching neither this teardown nor anything else — so the PTY children and
/// the terminal manager were simply orphaned. It now runs this first.
///
/// Two measured differences from v4, both recorded in the P4.D166 lane record
/// rather than ported: v5 has no SIGTERM/SIGINT handler at all (there is no
/// other ordered shutdown to share), and v5's databases open
/// `journal_mode = TRUNCATE`, never WAL (`db/mod.rs`), with no `close` or
/// checkpoint to run — v4's specific damage has no v5 counterpart.
struct AssemblyTeardown {
    stop: watch::Sender<bool>,
    /// Clears the host's terminal-manager slot for this assembly.
    terminal_slot: Arc<Mutex<Option<Arc<TerminalManager>>>>,
    /// Clears the host's structural-problems record for this assembly (the
    /// getter's "empty while locked" contract; the next assemble replaces it).
    structural_problems: Arc<Mutex<Vec<String>>>,
    /// The instance lock this assembly holds; released on shutdown (AFTER the
    /// stop flag flips, so the heartbeat loop never mistakes our own release
    /// for a lock loss).
    lock_path: PathBuf,
    /// Keeps this assembly's wake target alive; dropping it prunes the weak
    /// from the fan-out.
    _wake_target: Arc<WakeFn>,
}

impl AssemblyTeardown {
    fn run(&self) {
        let _ = self.stop.send(true);
        // Drop this assembly's terminal manager (live PTYs keep their reader
        // threads until the shells exit; new spawns need a fresh unlock).
        self.terminal_slot.lock().unwrap().take();
        self.structural_problems.lock().unwrap().clear();
        // Idempotent: a second shutdown finds no file (or not ours) and no-ops.
        // On the lock-loss path this is the ownership test doing its job —
        // a record another process now owns is left strictly alone.
        lock::release_instance_lock(&self.lock_path);
    }
}

struct HostShutdown(Arc<AssemblyTeardown>);

impl EngineShutdown for HostShutdown {
    fn shutdown(&self) {
        self.0.run();
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
        let ensure_failures = seed_built_ins(db)?;

        // The gated sample-content seed (P4.4u4): on a first boot (zero-characters
        // gate) import Lorian + Riya + 42 memories + Lorian's avatar, matching a
        // fresh v4 instance. Behind the config flag (default off this lane).
        // Swallows every failure (v4: seeding never blocks startup).
        if self.seed_sample_content {
            seed_sample_content(db)?;
        }

        // === P4.D248: v4's PHASE 3.1, the structural table check ===
        // After the migrations' twins and the seeds (3.1 < 3.66 < 3.7, v4's
        // `instrumentation.ts:568-582`), read-only; the result REPLACES the
        // host's record on every assemble — boot AND unlock — as v4's
        // `setStructuralProblems` replaces (a lock/unlock after a repair clears a
        // stale record). `/health` reports it as the `structure` service.
        *self.structural_problems.lock().unwrap() =
            verify_structural_tables_at_boot(db, &ensure_failures);
        // === end P4.D248 ===

        // === P4.9I2A / P4.D222: v4's PHASE 3.66, the help reconcile ===
        // Since v4 `492771aff` the help reconcile is a STARTUP phase in v4 too
        // (awaited in `instrumentation.ts`, not only the lazy
        // `HelpSearch.loadFromDatabase` ensure it used to be), so the P4.9I2A
        // eager-boot divergence has converged for the boot half. It runs after
        // the partitions are open and the built-in seeds have run (so
        // `help_doc_chunks` exists), BEFORE the job pump starts below (it
        // enqueues HELP_DOC embedding jobs) — and, as v4 orders it, BEFORE the
        // embedding-dimension reconcile (3.7) next, which used to run inside
        // `seed_built_ins` ahead of it. Best-effort (help never blocks boot).
        reconcile_help_docs_at_boot(db);
        // === end P4.9I2A / P4.D222 ===

        // === P4.d27 / P4.D222: v4's PHASE 3.7, after 3.66 ===
        reconcile_embedding_dimensions_at_boot(db);
        // === end P4.d27 / P4.D222 ===

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
        // and died.
        //
        // P4.9I2A: the tree is the compile-time EMBEDDED table, not a
        // `current_dir()` walk. v4's `join(process.cwd(), 'help')` assumes the
        // server runs from its checkout; a native binary does not, and this
        // repo never had a `help/` beside the binary at all — so every
        // reindex-all since P4.6BM synced an EMPTY tree. The same table feeds
        // the boot-time help reconcile (`HelpDocReconcileGate::ensure`) in `assemble`, so reindex-all
        // and the boot ensure cannot disagree about the shipped documentation.
        let help_files = crate::files_store::embedded_help_source_files();
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
            Box::new(ConversationRenderHandler {
                now_iso: None,
                display_zone: self.display_zone.clone(),
            }),
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
        // === P4.9I2A: the help-chat send driver, read off the bundle BEFORE the
        // tuple below consumes it (a fenced two-line pickup — the tuple stays
        // untouched for the sibling lane). ===
        let help_chat_send = spine_bundle
            .as_ref()
            .and_then(|bundle| bundle.help_chat_send.clone());
        // === end P4.9I2A ===
        // === P4.9K1: the per-character generator driver, the same pickup shape. ===
        let generators_detail = spine_bundle
            .as_ref()
            .and_then(|bundle| bundle.generators_detail.clone());
        // === end P4.9K1 ===
        // === P4.9K2: the creation-pair generator driver, the same pickup shape. ===
        let generators_wizard = spine_bundle
            .as_ref()
            .and_then(|bundle| bundle.generators_wizard.clone());
        // === end P4.9K2 ===
        // === P4.D217: the Scenario Builder driver, the same pickup shape. ===
        let scenario_builder = spine_bundle
            .as_ref()
            .and_then(|bundle| bundle.scenario_builder.clone());
        // === end P4.D217 ===
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
            // ⚠ OUT-OF-OWNERSHIP (P4.D180, authorised by the human 2026-09-11):
            // `host.rs` is P4.D179's file this round, for its one boot-ensure
            // call ~400 lines below. These four `in_scene_voice` sites are the
            // whole of P4.D180's edit here, and they are unavoidable: host.rs is
            // the ONLY place the spine bundle is threaded into `EngineAssembly`,
            // whose struct literal is exhaustive, so even a DEFERRED wire could
            // not compile without touching this file. Flagged for the unifier.
            in_scene_voice,
            operator_tool_runner,
            regenerate_title,
            outfit_llm_choose,
            image_describe,
            // P4.42: the web-search provider (the tools-inventory bool derives
            // from `is_some()`; the spine's own copy runs `search_web`).
            web_search,
            // P4.59: the registered SEARCH-provider manifests the providers
            // listing serves — the same registration `web_search` came from.
            search_providers,
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
                    bundle.in_scene_voice, // ⚠ P4.D180 out-of-ownership — see above
                    bundle.operator_tool_runner,
                    bundle.regenerate_title,
                    bundle.outfit_llm_choose,
                    bundle.image_describe,
                    bundle.web_search,
                    bundle.search_providers,
                )
            }
            None => (
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None, // ⚠ P4.D180 in_scene_voice — see the destructure's note
                None,
                None,
                None,
                None,
                None,
                Vec::new(),
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

        // The one ordered teardown for this assembly, registered inward into
        // the heartbeat loop exactly as v4's `client.ts` registers its
        // `handleShutdown` (bug 126) and handed to the engine as its
        // `EngineShutdown` below.
        let teardown = Arc::new(AssemblyTeardown {
            stop: stop_tx,
            terminal_slot: self.terminal_slot.clone(),
            structural_problems: self.structural_problems.clone(),
            lock_path: lock_path.clone(),
            _wake_target: wake_target,
        });

        // The lock heartbeat (v4: 60 s). Losing the lock runs the ordered
        // teardown (drivers stopped, PTYs dropped, our lock released iff it is
        // still ours), then the configured handler — default: exit 1, the
        // faithful v4 shutdown (see `HostConfig::on_lock_lost`).
        self.rt.spawn(heartbeat_loop(
            lock_path.clone(),
            teardown.clone(),
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
                    self.display_zone.clone(),
                    self.started,
                    Arc::new(SystemClock),
                ),
            )),
            shutdown: Box::new(HostShutdown(teardown)),
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
            // P4.D100: the honest Fetch Models discovery seam. Independent of
            // the spine bundle — it needs only the host's HTTP client and the
            // `ca22ec45` image-download seam, so it is built here directly.
            // P4.D138 unit 7: the HuggingFace LoRA lookup. Like the discovery
            // seam above it needs only the host's HTTP client, so it is built
            // here directly — with v4's ten-second bound as the transport's
            // per-request timeout.
            lora_metadata: Some(
                quilltap_core::image_gen::huggingface_lookup::ErasedLoraMetadata::new(
                    crate::wire::ReqwestLoraMetadata::new(),
                ),
            ),
            image_discovery: Some(quilltap_core::model::image::ErasedImageDiscovery::new(
                quilltap_core::model::image_dialects::RealImageProvider::with_bytes_fetch(
                    crate::wire::ReqwestWireTransport::new(),
                    crate::wire::ReqwestImageBytes::new(),
                ),
            )),
            // === end P4.6ai ===
            // === P4.73: the images-collection import fetch. v4's bare
            // `fetch(url)` — no timeout, no headers — so the transport adds
            // none of its own. ===
            image_import_fetch: Some(quilltap_core::api::images::ErasedImageImportFetch::new(
                crate::image_import_fetch::ReqwestImageImportFetch::new(),
            )),
            // === end P4.73 ===
            // === P4.76: the images-collection GENERATE action, wired LIVE.
            // Independent of the spine bundle (the `image_discovery` /
            // `lora_metadata` precedent above): the route needs only the plain
            // completion + moderation wires and the image dialect, all of which
            // `ProviderIo` builds. ⚠ 💸 one image-provider call per request,
            // plus a cheap-LLM classification whenever the Concierge is armed. ===
            images_generate: Some(crate::images_generate::images_generate_seams(&self.version)),
            // === end P4.76 ===
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
            // === P4.D180: the IN-SCENE voice rehearsal ("In Their Own Words"),
            // wired LIVE from the spine's completion + embedding providers
            // (`HostInSceneVoiceRunner`, the announcement-preview arrangement —
            // a per-call logging cheap executor so the request's own user/chat
            // land on the `llm_logs` row, as v4 does). Spine-less assemblies
            // keep `None` → the arm answers the loud not-assembled refusal
            // AFTER v4's whole ladder, so the dialog renders the reason.
            // ⚠ LIVE means real money: one cheap-LLM call per rehearsal.
            // ⚠ OUT-OF-OWNERSHIP for P4.D180 — see the note at the destructure.
            in_scene_voice,
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
            // === P4.59: the registered SEARCH-provider manifests. v4's
            // `GET /api/v1/providers` lists `searchProviderRegistry.getAllProviders()`,
            // and that registry is exactly what decided `web_search`'s
            // `serper_registered` above — so the listing and the runner answer
            // from one fact. Spine-less assemblies list none, matching their
            // `web_search: None`. ===
            search_providers,
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
            // === P4.9I2A: the help-chat send driver, LIVE from the spine (⚠ real
            // spend: one streamed call per help character per send). Spine-less
            // assemblies keep `None` → the `HelpChatSend` arm answers its NAMED
            // refusal. ===
            help_chat_send,
            // === end P4.9I2A ===
            // === P4.9K1: the per-character generator driver, LIVE from the spine
            // bundle (⚠ 💸 real spend: the optimizer + the external prompt).
            // Spine-less assemblies keep `None` → the two model-calling verbs
            // answer their NAMED refusal after v4's 404 + Zod arms. ===
            generators_detail,
            // === end P4.9K1 ===
            // === P4.9K2: the creation-pair generator driver, LIVE from the spine
            // bundle (⚠ 💸 real spend: the wizard + the AI import). Spine-less
            // assemblies keep `None` → the three verbs answer their NAMED
            // refusal after v4's parse arms. ===
            generators_wizard,
            // === end P4.9K2 ===
            // === P4.D217: the Scenario Builder run driver (LIVE from the spine;
            // spine-less assemblies keep `None` → the named refusal). ===
            scenario_builder,
            // === end P4.D217 ===
        })
    }
}

/// P4.9I2A / P4.D222 — v4's PHASE 3.66: the help reconcile over the EMBEDDED
/// help tree (`files_store::embedded_help_source_files`, the same table the
/// `EMBEDDING_REINDEX_ALL` handler reads), through a [`HelpDocReconcileGate`]
/// constructed for this assembly (v4's once-per-process memo), on a fresh OS
/// thread with its own current-thread runtime so the async `Db::write` inside
/// it is legal whether `assemble` was reached from the sync boot path or an
/// async `Unlock` dispatch (the `seed_built_ins` thread-bridge idiom).
///
/// The gate logs its own lines (`Help docs reconciled`, or the WARN `Help doc
/// reconcile failed; serving help from the existing index`) and never fails;
/// the only thing left to report here is v4's instrumentation catch — a WARN
/// `Help doc reconciliation failed`, reached in v4 only when the module
/// import throws, and here when the thread cannot run it at all.
///
/// [`HelpDocReconcileGate`]: quilltap_core::services::help_doc_sync::HelpDocReconcileGate
fn reconcile_help_docs_at_boot(db: &Db) {
    use quilltap_core::services::help_doc_sync::HelpDocReconcileGate;

    let db = db.clone();
    let outcome = std::thread::spawn(move || -> Result<(), String> {
        let files = crate::files_store::embedded_help_source_files();
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| format!("help reconcile runtime: {e}"))?;
        let gate = HelpDocReconcileGate::new();
        rt.block_on(gate.ensure(&db, &files));
        Ok(())
    })
    .join();
    let error = match outcome {
        Ok(Ok(())) => return,
        Ok(Err(e)) => e,
        Err(_) => "the help reconcile thread panicked".to_string(),
    };
    tracing::warn!(
        target: "quilltap::boot",
        context = "instrumentation.register",
        error = error.as_str(),
        "Help doc reconciliation failed",
    );
}

/// v4's PHASE 3.7 (`reconcileEmbeddingDimensions`), lifted OUT of
/// [`seed_built_ins`]' writer closure by P4.D222 so the help reconcile (Phase
/// 3.66, v4 `492771aff`) can run BEFORE it — v4 awaits 3.66 precisely so its
/// writes land before 3.7 can enqueue a reindex whose sync would race it. Same
/// fresh-thread `write_blocking` idiom as `seed_built_ins`; never fails the boot
/// — P4.134: the pass itself is total, so the only failures left (the thread
/// panicking, the writer gone) log v4's `.catch` WARN `Embedding dimension
/// reconciliation failed` (`instrumentation.ts:935-950`) and the boot goes on,
/// where they used to fail `assemble`.
fn reconcile_embedding_dimensions_at_boot(db: &Db) {
    use quilltap_core::db::DbError;

    let db = db.clone();
    let outcome = std::thread::spawn(move || -> Result<(), DbError> {
        db.write_blocking(|ws| {
            let main = ws.main().connection();
            // === P4.d27 (v4 `7391404e`) ===
            // v4's PHASE 3.7 — since P4.D222 its own writer closure, run after
            // the help reconcile (3.66). One embedding standard per instance:
            // delete non-conforming vector-index entries, snap the index meta,
            // and enqueue ONE deduped `mismatched-dim` reindex for whatever
            // still needs re-embedding — stale chats included (v4 `f7f3d7bf0`:
            // conversation embeddings are kept warm, never cold-tiered).
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
                );
            // Same lesson as the gate above: report the pass whenever it had a
            // profile to enforce, so a healthy "corpus conforms" is visible too.
            if let Some(target) = dim_reconcile.target_dimensions {
                tracing::info!(
                    target: "quilltap::boot",
                    target_dimensions = target,
                    vector_entries_deleted = dim_reconcile.vector_entries_deleted,
                    vector_index_meta_fixed = dim_reconcile.vector_index_meta_fixed,
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
            Ok(())
        })
    })
    .join();
    let error = match outcome {
        Ok(Ok(())) => return,
        Ok(Err(e)) => quilltap_core::db::fallback::error_text(&e),
        Err(_) => "the embedding dimension reconcile thread panicked".to_string(),
    };
    tracing::warn!(
        target: "quilltap::boot",
        context = "instrumentation.register",
        error = error.as_str(),
        "Embedding dimension reconciliation failed",
    );
}

/// Seed the built-in roleplay templates + provision-or-adopt the three built-in
/// mount stores (P4.4u3, families 1 & 2) through the writer thread, and run the
/// main partition's boot repairs. Spawned on a fresh OS thread and joined so
/// `write_blocking` is legal from either the sync boot path or an async
/// `Unlock` dispatch. The mount families are skipped on a main-only instance
/// (no mount-index partition).
///
/// Answers this boot's structural ensure failures by collection (P4.D248): the
/// built-in mounts' lazy-home sub-steps and the absent-table creation, which
/// the PHASE 3.1 pass reports in v4's ensure form without re-running them.
fn seed_built_ins(db: &Db) -> Result<EnsureFailures, String> {
    use quilltap_core::db::DbError;
    use quilltap_core::services::mount_index::general_state;
    use quilltap_core::services::{builtin_mounts, builtin_templates};

    let db = db.clone();
    std::thread::spawn(move || -> Result<EnsureFailures, DbError> {
        db.write_blocking(|ws| {
            let main = ws.main().connection();
            let mut ensure_failures = EnsureFailures::default();
            // === P4.134 (dogfood #134(b)) ===
            // v4 seeds the templates inside `seedInitialData` (phase 1.25)
            // through a FALLBACK `safeQuery`: a failure logs `Error seeding
            // built-in roleplay templates` and seeding carries on (the
            // `seed-initial-data.ts` ERROR around it is unreachable on a
            // database failure). Never a boot failure in v4, so not here.
            quilltap_core::db::fallback::seed_built_in_templates_or_log(|| {
                builtin_templates::seed_built_in_templates(main)
            });
            // === end P4.134 ===
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
            // === P4.D251 (v4 `07b8f0209`, migration `impersonation-voice-mode-v1`)
            // — REPLACES the P4.D179 `add-impersonation-voice-rewrite-field-v1`
            // ensure that stood here ===
            // `chat_settings.impersonationVoiceMode` (TEXT off/ask/always)
            // replaced the 4.10-dev boolean `impersonationVoiceRewrite` in
            // place: add the mode when absent, translate rows still at the
            // default from the retired column (1 → 'ask'), DROP the retired
            // column. Re-homed from v4's migration runner for the same reason
            // as its P4.D73 neighbours above, and load-bearing in the same way
            // (the read tolerates absence with v4's Zod default `'off'`; the
            // write is a plain `UPDATE … SET impersonationVoiceMode = ?`). The
            // OLD ensure had to go, not merely be followed: kept, it would
            // re-ADD the column v4's migration dropped on the SHARED Friday
            // instance and ping-pong with v4 (§R.13); this ensure's
            // both-columns arm heals exactly that file. v4's `dependsOn` is
            // satisfied by construction — an instance with NEITHER column lands
            // on the same shape as v4's two migrations in sequence (proven by
            // `chat_settings_voice_mode_ensure_equivalence`'s mode (B)).
            quilltap_core::db::chat_settings_impersonation_voice_mode_repair::
                ensure_chat_settings_impersonation_voice_mode(main)?;
            // === end P4.D251 ===
            // === P4.D79 (v4 `23af7146`, migration
            // `add-profile-multi-character-prefill-field-v1`) ===
            // The `connection_profiles.multiCharacterPrefill` column, re-homed
            // from v4's migration runner for the same reason. Load-bearing on
            // an existing instance: without the column the profile PUT's
            // `UPDATE … SET multiCharacterPrefill = ?` would 500, and the turn
            // anchor would have no stored choice to read. The ensure carries
            // v4's ONE-TIME backfill (Anthropic off, everything else on) and
            // never re-runs it — see the module header on why an explicit
            // choice must survive the next boot.
            quilltap_core::db::connection_profiles_prefill_repair::
                ensure_connection_profiles_prefill_column(main)?;
            // === end P4.D79 ===
            // === P4.D135 (v4 `65f5021c8`, migration
            // `add-profile-fallback-fields-v1`) ===
            // The `connection_profiles` fallback-chain pair
            // (`fallbackProfileId` + `allowTierFallback`), re-homed from v4's
            // migration runner for the same reason. Load-bearing on an existing
            // instance: without the columns the profile PUT's
            // `UPDATE … SET fallbackProfileId = ?` would 500, and no chain
            // could ever name an understudy. v4's migration `dependsOn`s the
            // prefill one, so it sits here, after it.
            quilltap_core::db::connection_profiles_fallback_repair::
                ensure_connection_profiles_fallback_columns(main)?;
            // === end P4.D135 ===
            // === P4.D171 (v4 `5841a8c62`, migration
            // `add-route-trail-message-column-v1`) ===
            // The `chat_messages.routeTrail` column, re-homed from v4's
            // migration runner for the same reason. Load-bearing on an
            // existing instance: without it, the message INSERT this port
            // performs at every turn (which now always binds the column)
            // would 500.
            quilltap_core::db::chat_messages_route_trail_repair::
                ensure_chat_messages_route_trail_column(main)?;
            // === end P4.D171 ===
            // === P4.D171 (v4 `2aca73ad6`, migration
            // `add-cycle-order-column-v1`) ===
            // The `chats.cycleOrderParticipantIds` column, re-homed from v4's
            // migration runner for the same reason. Load-bearing on an
            // existing instance: without it, `ChatUpdate`'s `set_col!` arm
            // for the drawn rotation would 500 on every turn once the turn
            // manager writes it.
            quilltap_core::db::chats_cycle_order_repair::ensure_chats_cycle_order_column(main)?;
            // === end P4.D171 ===
            // === P4.D182 (v4 `7fbf8a55b`, migration
            // `add-file-generation-key-column-v1`) ===
            // The `files.generationKey` column AND its `idx_files_generationKey`
            // index, re-homed from v4's migration runner for the same reason as
            // the ensures above. Two arms, because `generateDDL` emits the
            // column (v4's `FileEntrySchema` declares it, so the D23 re-dump
            // carries it) but cannot express a plain index — so a FRESH v5
            // instance arrives here with the column and no index, and an
            // existing one with neither. Load-bearing on an existing instance:
            // the `files` INSERT this port performs now always binds the
            // column, so without it the first file write would 500; the index
            // is what keeps P4.D184's pre-generation avatar-cache lookup cheap.
            quilltap_core::db::files_generation_key_repair::
                ensure_files_generation_key_column_and_index(main)?;
            // === end P4.D182 ===
            // === P4.D182 (v4 `5029075bb`, migration
            // `add-transcript-version-column-v1`) ===
            // The `chats.transcriptVersion` counter column. Unlike every
            // sibling here, this pass is the column's ONLY source on EVERY
            // instance, fresh included: v4 keeps the field out of
            // `ChatMetadataSchema` on purpose (Zod strips what it does not
            // declare, so a whole-row rewrite cannot rewind the counter), and
            // `generateDDL` walks the schema — measured at the `31436bae4`
            // re-dump, the column appears nowhere in `fresh_schema.json`.
            // Load-bearing: without it P4.D183's atomic
            // `SET transcriptVersion = transcriptVersion + 1` on the message
            // funnel would 500 on every message written. Nothing else in v5
            // may read, project or export the column — see the module header.
            quilltap_core::db::chats_transcript_version_repair::
                ensure_chats_transcript_version_column(main)?;
            // === end P4.D182 ===
            // === P4.D225 (v4 `49059fb14`, migration
            // `add-chat-refusal-ledger-v1`) ===
            // The Concierge refusal ledger's two `chats` columns. Like
            // `transcriptVersion` just above, this pass is their ONLY source on
            // EVERY instance, fresh included: v4 keeps both out of
            // `ChatMetadataSchema` on purpose (a whole-row rewrite must not be
            // able to rewind the tally), and `generateDDL` walks the schema —
            // measured live at the pin, the `chats` CREATE carries neither, so
            // `fresh_schema.json` does not move. Load-bearing: the ledger's
            // `SET … "moderationRefusalCount" + ?` and its reads would fail on
            // every chat without them.
            quilltap_core::db::chats_moderation_refusal_ledger_repair::
                ensure_chats_moderation_refusal_ledger_columns(main)?;
            // === end P4.D225 ===
            // === P4.D97 (v4 `97d2fcb5`, migration
            // `retire-prefill-on-thinking-profiles-v1`) ===
            // The data pass that turns the multi-character [Name] prefill off
            // on existing DeepSeek/Ollama rows running a thinking turn (v4
            // bugs 68/85) — those rows got their 1 from the old provider
            // default, not a user choice, and a stored boolean outranks every
            // later default. Unlike the column ensures above, this pass is
            // DATA-only, so its once-only guard is v4's own migrations_state
            // ledger (cross-app in both directions — see the module header);
            // it must run AFTER the P4.D79 ensure, because on a pre-4.9
            // instance the column does not exist until that call.
            if let quilltap_core::db::thinking_prefill_retire_heal::RetireOutcome::Ran {
                examined,
                cleared,
            } = quilltap_core::db::thinking_prefill_retire_heal::retire_prefill_on_thinking_profiles(
                main,
                &quilltap_core::clock::now_iso(),
            )? {
                tracing::info!(
                    target: "quilltap::boot",
                    examined,
                    cleared,
                    "Retired the [Name] prefill on thinking connection profiles"
                );
            }
            // === end P4.D97 ===
            // === P4.D140 (v4 `735d9408c`, migration
            // `recompute-chat-last-message-at-v1`) ===
            // Bug 112's data pass: `lastMessageAt` used to be bumped by any
            // message row, so every Staff announcement (a story background
            // finishing, a folded summary, a Pascal roll, a Host notice) dated a
            // months-dead chat as freshly active — 608 of 871 chats mis-dated on
            // the real instance. This rewrites the column for existing chats
            // under the shipped predicate, so history reads the way new activity
            // will. DATA-only like the P4.D97 pass above, so its once-only guard
            // is v4's own migrations_state ledger — and, exactly as v4's runner
            // does, a boot that finds NO drift writes NO ledger row and simply
            // re-checks next time (a stamp on a clean boot would make a later v4
            // boot skip a migration it never ran).
            if let quilltap_core::db::chat_activity_recompute_heal::RecomputeOutcome::Ran {
                updated,
                cleared,
            } = quilltap_core::db::chat_activity_recompute_heal::recompute_chat_last_message_at(
                main,
                &quilltap_core::clock::now_iso(),
            )? {
                tracing::info!(
                    target: "quilltap::boot",
                    updated,
                    cleared,
                    "Recomputed chat last-activity from character-authored messages"
                );
            }
            // === end P4.D140 ===
            // === P4.D145 (v4 `a5df98b3f`, migration
            // `collapse-duplicate-folders-v1`, bug 114) ===
            // One `folders` row per (userId, COALESCE(projectId,''), path),
            // enforced by a unique index. Collapses the duplicates a
            // pre-4.9 instance carries (607 rows describing 24 folders on
            // the real one) — oldest wins, children repointed at the
            // survivor before the deletes — then creates the index that
            // makes a repeat impossible.
            //
            // Unlike the two ledger-guarded passes above, this one's
            // once-only marker is the INDEX, because that is v4's own
            // guard (`shouldRun()` is `!indexExists()`): a
            // `migrations_state` row here would claim a completion v4
            // never claims for this migration. See the module header.
            //
            // v4's `generateDDL` cannot express a COALESCE index, so
            // `fresh_schema.json` correctly does not carry it (no D23
            // re-dump); since P4.153 (dogfood #149) the provisioner replays
            // v4's MIGRATION-created index family (`migration_indexes.json`),
            // which does — so a fresh instance answers `AlreadyIndexed` here,
            // and this pass still covers every instance provisioned before
            // that (`assemble` runs this chain on EVERY open). Since P4.160
            // the REST of that family is backfilled on such an instance by
            // the LAST step of this chain (`migration_index_family_repair`,
            // below) — after this collapse, so the folders are already
            // deduped when it reaches this name and finds it present.
            if let quilltap_core::db::folders_unique_path_repair::CollapseOutcome::Ran {
                scanned,
                surviving,
                deleted,
                repointed,
            } = quilltap_core::db::folders_unique_path_repair::ensure_folders_unique_path_index(
                main,
                &quilltap_core::clock::now_iso(),
            )? {
                if deleted > 0 || repointed > 0 {
                    tracing::info!(
                        target: "quilltap::boot",
                        scanned,
                        surviving,
                        deleted,
                        repointed,
                        "Collapsed duplicate folder rows"
                    );
                }
            }
            // === end P4.D145 ===
            // === P4.D77 (v4 `24633026`, migration
            // `create-help-doc-chunks-table-v1`) ===
            // The `help_doc_chunks` table itself, re-homed from v4's migration
            // runner for the same reason as the repairs above. An upgraded
            // instance matches every help-doc content hash, so the sync would
            // never slice it and section search would silently never engage —
            // the startup reconcile (`reconcile_help_docs`, P4.D222) slices any
            // section-less doc, but it needs the table to read. Fresh instances already carry the
            // `generateDDL` shape (the D23 re-dump); this gives an existing one
            // the MIGRATION shape, exactly as v4's own migration would.
            quilltap_core::db::help_doc_chunks_repair::ensure_help_doc_chunks_table(main)?;
            // === end P4.D77 ===
            // === p4.9i2 unification: the `help_docs` table itself ===
            // v4 grows this collection lazily (`ensureCollection` on the first
            // help read); v5's boot sync READS it, so a pre-help_docs instance
            // (the e2e `salon-*` fixture) failed the sync, emptied the Guide
            // and killed every help send — the activated beats' first live run.
            // P4.134: v4 grows it lazily, so a failure there is v4's lazy
            // `ensureCollection` pair of lines on the first help read, never a
            // boot failure — logged here and the boot goes on (the help
            // reconcile below then logs its own guarded line).
            quilltap_core::db::fallback::ensure_collection_or_log("help_docs", || {
                quilltap_core::db::help_doc_chunks_repair::ensure_help_docs_table(main)
            });
            // === end p4.9i2 unification ===
            // === P4.6BM (replaces the P4.6BL stand-in) ===
            // v4's startup reconcile (`instrumentation.ts` PHASE 3.6): scan for
            // chats the Scriptorium pipeline left half-finished — arm (A) real
            // messages but no conversation chunks, arm (B) recoverable
            // un-embedded interchange chunks, arm (C) sub-chunkable oversize
            // chunks — and re-enqueue a CONVERSATION_RENDER for each. The
            // handler re-chunks (preserving existing embeddings) and
            // re-enqueues the missing embeds, so every arm heals. Stale chats
            // are healed too since v4 `f7f3d7bf0` (the stale gate and its
            // `skippedStale` are gone): on an instance the old sweep
            // cold-tiered, THIS is the one-time first-boot re-embed of the
            // backlog — a real provider cost, measured by the recipe in the
            // P4.D235 lane record before any copy boots. This REPLACES the P4.6BL v5-only direct-embed repair,
            // which existed only because that handler was unported; the
            // coverage argument is in the reconcile's module doc. No-op on a
            // healthy instance; returns zeros (never fails the boot) when a
            // lazily-created table is absent.
            let reconcile =
                quilltap_core::services::conversation_render_reconcile::reconcile_conversation_rendering(
                    main,
                );
            // The gate is `incomplete_chats > 0`, NOT `enqueued > 0` (dogfood
            // finding, `dogfood-findings.md`: a reuse-only pass must still say
            // so). v4 logs its "found incomplete conversations" line before the
            // loop (restored inside the reconcile by P4.D235) and its completion
            // line unconditionally after an early return on zero rows, so this
            // is v4's shape. Log output sits outside the differential contract
            // (P4.18).
            if reconcile.incomplete_chats > 0 {
                tracing::info!(
                    target: "quilltap::boot",
                    incomplete_chats = reconcile.incomplete_chats,
                    enqueued = reconcile.enqueued,
                    reused = reconcile.reused,
                    failed = reconcile.failed,
                    "Conversation render reconciliation complete",
                );
            }
            // === end P4.6BM ===
            // === P4.D205 (v4 `e7d77bb60`, migration
            // `add-chat-informs-table-v1`) ===
            // The `chat_informs` table, re-homed from v4's migration runner to
            // a boot ensure for the same reason as its neighbours above.
            // Load-bearing on an existing instance: the composer's pending-chip
            // read runs on every Salon open, so without the table the first
            // read would hit `no such table: chat_informs`.
            //
            // It emits the **generateDDL** shape — no foreign key, the
            // `createdAt` index — which is what `fresh_schema.json` carries
            // from the D23 re-dump, so a fresh instance and an ensured one
            // agree byte for byte. v4's migration writes a different shape (an
            // FK plus three purpose-built indexes); the two are
            // column-name-addressed and interchangeable, and whichever creates
            // the table first wins. The consequence v5 must carry is in
            // `db/chats.rs::delete`: with no FK, the chat-delete cascade
            // deletes these rows explicitly.
            quilltap_core::db::chat_informs::ensure_chat_informs_table(main)?;
            // === end P4.D205 ===
            // === P4.D249 (v4 52d6e7ecd, migration add-chat-informs-permanent-v1) ===
            // The standing-inform flag, re-homed from v4's migration runner
            // for the same reason as the table ensure above — and AFTER it,
            // which is v4's `dependsOn: ['add-chat-informs-table-v1']`. Every
            // inform read names `permanent` explicitly, so on an instance whose
            // table predates `52d6e7ecd` the composer's pending-chip poll would
            // fail on its first read without this. It ALTERs in v4's migration
            // DDL (`INTEGER NOT NULL DEFAULT 0`, appended), no backfill — the
            // default makes every existing row the one-shot it always was. A
            // table the ensure above just created already has the column (the
            // generateDDL shape), and this is a no-op.
            //
            // Fatal on failure, like the ensure beside it — v4's migration is
            // not `resumable`, so a failed `add-chat-informs-permanent-v1`
            // lands in the runner's `failed` list and `instrumentation.ts`
            // exits with "Migrations failed - cannot start server" (measured at
            // the pin; not the P4.D248 defer-and-boot class, which is the
            // `resumable` flag's alone).
            quilltap_core::db::chat_informs_permanent_repair::ensure_chat_informs_permanent_column(
                main,
            )?;
            // === end P4.D249 ===

            // === P4.D204 (v4 `f45a517a9`, `lib/startup/reconcile-chat-
            // message-fts.ts` PHASE 3.65 + the migration
            // `create-chat-message-fts-v1`) ===
            // The message search index: the map table, the contentless FTS5
            // virtual table and the three sync triggers, ensured and brought
            // into step with the transcript on every boot.
            //
            // NOT ledger-guarded, unlike the two heals below it — this is v4's
            // PHASE 3.65, which runs unconditionally on every start. It is
            // cheap by construction (five `IF NOT EXISTS` statements plus two
            // counts on indexed columns) and it has to be unconditional,
            // because the failure it heals is SILENT: any table rebuild of
            // `chat_messages` drops the triggers with the old table and raises
            // nothing, after which search quietly goes stale.
            //
            // It is also the ONLY place a v5 instance can get these objects at
            // all. v4 creates them in the migration, not in `ensureCollection`,
            // so the D23 schema dump cannot carry them: the dumper keeps only
            // `table` and `index` rows from v4's `generateDDL`, filtering
            // triggers and virtual tables out by construction (measured —
            // `fresh_schema.json` has 0 TRIGGER / 0 VIRTUAL / 0 fts, and
            // `provisioning_equivalence` stays green BECAUSE of that).
            // Without this call a freshly provisioned v5 instance would search
            // through the exact-scan fallback forever.
            //
            // Total by design: a broken index must never keep the instance
            // from starting, so the pass swallows its own failures with a warn
            // and search degrades to the fallback.
            let _ = quilltap_core::db::chat_message_fts_reconcile::reconcile_chat_message_fts(
                main,
            );
            // === end P4.D204 ===
            // === P4.D208 (v4 `da9c4f34f`, migration
            // `clear-scenario-seeded-chat-summaries-v1`, bug 158) ===
            // Bug 158's data pass. Chat creation used to write the chosen
            // scenario into `contextSummary` as well as `scenarioText`, so
            // every reader of the summary column believed a brand-new chat had
            // already been summarized — and the greeting's "Recent
            // Conversations" block handed the next character a stage direction
            // to open from. 186 of 712 chats were in that state on v4's own
            // live instance. This nulls the summary wherever it is
            // byte-identical to the row's own non-empty scenario, and bumps
            // `updatedAt` on exactly those rows.
            //
            // DATA-only like the P4.D97 and P4.D140 passes above, so its
            // once-only guard is v4's own `migrations_state` ledger — and, as
            // v4's runner does, a boot that finds NO seeded row writes NO
            // ledger row and simply re-checks next time (a stamp on a clean
            // boot would make a later v4 boot skip a migration it never ran).
            //
            // Ordered AFTER the P4.D145 collapse for no reason but the fence
            // convention; it reads and writes only `chats` and shares nothing
            // with its neighbours.
            if let quilltap_core::db::scenario_seeded_summary_heal::SeededSummaryHealOutcome::Ran {
                cleared,
            } = quilltap_core::db::scenario_seeded_summary_heal::clear_scenario_seeded_chat_summaries(
                main,
                &quilltap_core::clock::now_iso(),
            )? {
                tracing::info!(
                    target: "quilltap::boot",
                    cleared,
                    "Cleared the scenario standing in as a summary"
                );
            }
            // === end P4.D208 ===
            if let Some(mi) = ws.mount_index() {
                let mount_index = mi.connection();
                // === P4.D152 (v4 `0b0617fee`, migration
                // `realign-file-entry-sha256-v1`, bug 117) ===
                // `files.sha256` named the bytes a chat upload ARRIVED as, while
                // the bridge stored a transcoded WebP — so every join to
                // `doc_mount_files.sha256` was between two different languages and
                // returned an empty result nobody logged (118 of 239 uploaded
                // images on the real instance). The forward fix hashes after the
                // transcode; this realigns the rows written before it, reading
                // each blob's own hash out of the mount partition. Needs BOTH
                // connections, so it lives in this mount-aware block rather than
                // with the main-only passes above.
                //
                // DATA-only like the P4.D97/P4.D140 passes, so its once-only guard
                // is v4's own migrations_state ledger in the P4.D140 shape: an
                // existing row (from either app) is honoured, and a pass that
                // realigns NOTHING writes no row. ⚠ That second half DIVERGES
                // from v4 on an instance whose mount-blob rows exist and all
                // AGREE: v4's `shouldRun()` for THIS migration tests presence,
                // not drift, so its runner stamps a zero-`itemsAffected` row
                // there. Deliberate, pinned both directions by the heal's own
                // family — see the module header for why (a stamp on a pass
                // that changed nothing tells a later v4 boot to skip a
                // migration that never ran). A clean pass is also SILENT here
                // (no `Ran`, no line); the module header says so.
                if let quilltap_core::db::files_sha256_realign_heal::RealignOutcome::Ran {
                    scanned,
                    realigned,
                    orphaned,
                    malformed_key,
                } = quilltap_core::db::files_sha256_realign_heal::realign_file_entry_sha256(
                    main,
                    Some(mount_index),
                    &quilltap_core::clock::now_iso(),
                )? {
                    tracing::info!(
                        target: "quilltap::boot",
                        scanned,
                        realigned,
                        orphaned,
                        malformed_key,
                        "Realigned FileEntry sha256 values with the bytes actually stored"
                    );
                }
                // === end P4.D152 ===
                // === P4.D184 (v4 `7fbf8a55b`, migration
                // `collapse-duplicate-avatar-rolls-v1`) ===
                // The avatar configuration cache's data pass: group every pre-cache
                // avatar roll by its v0 key, keep the newest of each configuration,
                // repoint every reference to the rest, and delete them. DESTRUCTIVE
                // and visible — the duplicates are different seeds of one prompt, and
                // a chat that displayed an older roll now displays the survivor.
                //
                // It must run AFTER P4.D182's `files` ensure directly above: the pass
                // reads and writes `generationKey`, which on a pre-4.10 instance does
                // not exist until that call. Its once-only guard is v4's own
                // `migrations_state` ledger, honoured in both directions — the real
                // Friday instance has already been collapsed BY v4, so a v5 boot
                // there must find the row and do nothing at all.
                //
                // A failed pass DEFERS and the boot CONTINUES — v4 `e5c6bd0c0`
                // (bug 175, this port's own filing from P4.135). The 2026-10-01
                // ruling that made it FAIL THE BOOT (P4.135, matching v4's
                // `process.exit(1)` at `f6426e196`) is OVERTAKEN by v4's own fix.
                // v4 at `e5c6bd0c0`, in order: the migration's catch logs ERROR
                // `Failed to collapse duplicate avatar rolls` `{context:
                // 'migration.collapse-duplicate-avatar-rolls', error}` and
                // returns `success: false` (`collapse-duplicate-avatar-rolls-v1.ts:
                // 646-663`); the runner logs ERROR `Migration failed` `{context:
                // 'migrations.runMigrations', migrationId, error, message}` — now
                // BEFORE its defer check — and, the migration being `resumable`
                // (`:328-331`), `deferResumable` logs WARN `Resumable migration
                // deferred to the next boot; continuing startup` `{context,
                // migrationId}` and the loop continues (`migrations/index.ts:
                // 73-81`, `:189-217`). No ledger row is written, and only
                // survivors (+ protected portraits) are keyed before the delete
                // loop, so the next boot's gate finds the unfinished victims and
                // the pass finishes (measured on both boots by
                // `host_boot_hardness`'s resume arm). v5 has no runner, so it
                // carries v4's two runner lines here, as the `ShouldRun` arm
                // below already carries one.
                //
                // v4's `message` META field on `Migration failed` is carried as
                // `resultMessage`: a second field named `message` collides with
                // tracing's own (the format string), and the file layer then
                // writes it as the record's envelope `message` — the record would
                // read `Failed to collapse …` where v4's reads `Migration failed`
                // (measured at P4.D248; v4's own record nests it under
                // `context.message`, which no v5 field can reach).
                //
                // A failed LEDGER WRITE after a committed pass (`Stamp`) defers
                // the same way, through v4's throw arm — `Migration threw an
                // exception` `{context, migrationId, error}` then the WARN
                // (`index.ts:218-244`), after the pass's own success line (v4
                // logs that inside `run()`). v4 reaches that arm only when its
                // FILE-ledger fallback also fails (`migrations/state.ts:181-205`);
                // v5 has no file ledger, so it defers on the first failure —
                // recorded. The ledger PROBE (`Probe`) stays fatal and is dead
                // through the boot: v4's `loadMigrationState` never throws, and
                // P4.D97's identical probe above kills the boot first. v4's
                // aggregate lines (`Migration runner completed` / `Migrations
                // completed successfully` with `deferred`) and its
                // dependant-deferral arm have no v5 twin — no runner, and nothing
                // in v5 (or v4) depends on this migration — recorded NO-PORT.
                use quilltap_core::db::avatar_rolls_collapse_heal::{self, CollapseError};
                match avatar_rolls_collapse_heal::collapse_duplicate_avatar_rolls(
                    main,
                    Some(mount_index),
                    &quilltap_core::clock::now_iso(),
                ) {
                    // v4's ONE success line (`Collapsed duplicate avatar rolls`,
                    // with `durationMs`) is core's, emitted inside the pass
                    // before the ledger write; the early (no-victims) exit logs
                    // nothing, as v4's (P4.150 — the host's v5-only snake_case
                    // summary is gone).
                    Ok(_) => {}
                    // A failed `shouldRun` read is v4's runner SKIP arm, not a
                    // failed pass: the runner logs this line and boots on
                    // (`migrations/index.ts:162-179` at `e5c6bd0c0`). It is the one v4 line on
                    // this path, so v5 carries it even though v5 has no runner.
                    Err(CollapseError::ShouldRun(error)) => {
                        tracing::error!(
                            target: "quilltap::boot",
                            context = "migrations.runMigrations",
                            migrationId = avatar_rolls_collapse_heal::MIGRATION_ID,
                            error = %quilltap_core::db::fallback::error_text(&error),
                            "Error checking if migration should run"
                        );
                    }
                    Err(CollapseError::Pass(error)) => {
                        let error = quilltap_core::db::fallback::error_text(&error);
                        tracing::error!(
                            target: "quilltap::boot",
                            context = "migration.collapse-duplicate-avatar-rolls",
                            error = %error,
                            "Failed to collapse duplicate avatar rolls"
                        );
                        tracing::error!(
                            target: "quilltap::boot",
                            context = "migrations.runMigrations",
                            migrationId = avatar_rolls_collapse_heal::MIGRATION_ID,
                            error = %error,
                            resultMessage = "Failed to collapse duplicate avatar rolls",
                            "Migration failed"
                        );
                        log_collapse_deferred();
                    }
                    // Core's success line already logged, before the stamp —
                    // v4's order (the migration's `run()`, then the runner).
                    Err(CollapseError::Stamp { error, .. }) => {
                        tracing::error!(
                            target: "quilltap::boot",
                            context = "migrations.runMigrations",
                            migrationId = avatar_rolls_collapse_heal::MIGRATION_ID,
                            error = %quilltap_core::db::fallback::error_text(&error),
                            "Migration threw an exception"
                        );
                        log_collapse_deferred();
                    }
                    Err(CollapseError::Probe(error)) => return Err(error),
                }
                // === end P4.D184 ===
                // === P4.D175 (v4 `78b381a96`, migration
                // `clear-generated-image-placeholder-descriptions-v1`, bug 132) ===
                // Two image jobs stamped a LABEL into `description` — "Story
                // background for: <title>", "<Name> — wardrobe portrait" — and
                // `describe_image` served it ahead of the generation prompt.
                // The forward fix governs new images only: for everything
                // already on disk the caption still sits in the column, and
                // `auto_describe_precheck` answers `already-described` on the
                // strength of it, so the vision tier stays unreachable until
                // this pass clears it. Needs BOTH connections (the Scriptorium
                // links live in the mount partition), hence this block.
                //
                // Once-only through v4's own migrations_state ledger in the
                // P4.D140 shape. Unlike the P4.D152 pass above there is NO
                // divergence: v4's `shouldRun()` here COUNTS placeholders on
                // both sides, so a pass that clears nothing means v4 never ran
                // and stamped nothing either.
                if let quilltap_core::db::generated_image_placeholder_heal::PlaceholderHealOutcome::Ran {
                    files_cleared,
                    links_cleared,
                    links_skipped,
                } = quilltap_core::db::generated_image_placeholder_heal::clear_generated_image_placeholder_descriptions(
                    main,
                    Some(mount_index),
                    &quilltap_core::clock::now_iso(),
                )? {
                    tracing::info!(
                        target: "quilltap::boot",
                        files_cleared,
                        links_cleared,
                        links_skipped,
                        "Cleared placeholder descriptions from generated images"
                    );
                }
                // === end P4.D175 ===
                // === P4.134 (dogfood #134(b)) ===
                // The mount-index DDL, the link-content backlog sweep and the
                // three store provisions stay FATAL (v4 migrations — a failed
                // migration exits v4's process); the three case repairs, the
                // link-group column and the orphaned store-children reap log
                // v4's line and continue (v4 runs them lazily per access, or
                // as a fallback read at phase 3.3b), so a damaged mount-index
                // column degrades reads instead of killing the engine.
                // P4.D248: the failures they log are collected for the
                // PHASE 3.1 pass, which reports them without re-running them.
                ensure_failures.extend(builtin_mounts::ensure_builtin_mounts_with(
                    main,
                    mount_index,
                    builtin_mounts::LazyRepairFailures::LogAndContinue,
                )?);
                // v4's phase 3.4c. The folder failure is logged INSIDE (v4's
                // `[GeneralScenarios]` catch); this arm is v4's
                // `instrumentation.ts:766-776` WARN for anything else.
                if let Err(e) = builtin_mounts::ensure_general_scenarios_folder(main, mount_index) {
                    tracing::warn!(
                        target: "quilltap::boot",
                        context = "instrumentation.register",
                        error = %quilltap_core::db::fallback::error_text(&e),
                        "Error ensuring general scenarios folder, continuing startup",
                    );
                }
                // === end P4.134 ===
                // Companion (v4 instrumentation.ts Phase 3 tail, `f48f34dc`):
                // ensure the general mount's root state.json (the bottom tier
                // of the state cascade). Idempotent; never heals existing
                // content; warn-and-continue on error (v4's try/catch,
                // `instrumentation.ts:782-797` — P4.134: with v4's `context`
                // field on both lines and the bare driver message, not
                // `DbError`'s `sqlite error: ` prefix).
                match general_state::ensure_general_state_file(main, mount_index) {
                    Ok(true) => tracing::info!(
                        target: "quilltap::boot",
                        context = "instrumentation.register",
                        "Seeded general state.json in the Quilltap General mount",
                    ),
                    Ok(false) => {}
                    Err(e) => tracing::warn!(
                        target: "quilltap::boot",
                        context = "instrumentation.register",
                        error = %quilltap_core::db::fallback::error_text(&e),
                        "Error ensuring general state.json, continuing startup",
                    ),
                }
                // === P4.82 ===
                // v4's one-time head-and-shoulders backfill scan
                // (`lib/startup/enqueue-headshoulders-backfill.ts`), chained
                // LAST in `instrumentation.ts`'s vault-backfill `.then` behind
                // `backfillCharacterVaults` and the vault file migrations, so
                // every character already has a vault to write the result back
                // into. v5 has no twin of that chain as a boot stage, so it
                // sits here — after every repair, inside `seed_built_ins`'s
                // write closure. ⚠ RECORDED MECHANISM DIVERGENCE (the §3
                // unification review): `seed_built_ins` JOINS its thread, so
                // this scan runs ON the boot path and `assemble` waits for it,
                // where v4 chains it in a non-blocking `.then`. v4 enqueues
                // rather than generating inline precisely so per-character
                // LLM work cannot block the loading screen — that part holds
                // (no model call happens here); the scan itself is one
                // indexed read on every instance v4 has ever booted (its flag
                // is already set) and a single overlay `find_all` + N enqueues
                // on a v5-only instance's first boot. Detaching it is a small
                // follow-up; the host boot test's second/third-boot asserts
                // are deterministic BECAUSE of the join.
                //
                // It needs BOTH connections (the scan reads characters through
                // the vault overlay), so it lives in this mount-aware block.
                // ⚠ Its once-only guard is `instance_settings
                // ['headshoulders_backfill_enqueued_v1']` — v4's OWN row,
                // already written on every instance v4 has booted since the
                // feature shipped. A v5 boot there scans nothing and writes
                // nothing, by design; see the module header.
                // The scan narrates itself (v4's `scanning` / `enqueue
                // complete` lines); `instrumentation.ts` logs nothing after
                // the `await`, so neither does this site.
                let _ =
                    quilltap_core::services::headshoulders_backfill_enqueue::enqueue_headshoulders_backfill(
                        main,
                        mount_index,
                    );
                // === end P4.82 ===
            }
            // === P4.D248 (v4 `e5c6bd0c0`, bug 176) ===
            // v4's PHASE 3.1 runs each structural repository's `ensureTable`
            // before its shape check, and that ensure CREATES an absent table
            // (v4's tables are created lazily, so an instance — and most
            // committed fixtures — can lack the group/project link tables with
            // nothing wrong). v5's own boot ensures cover `help_doc_chunks`,
            // `doc_mount_points` and `doc_mount_folders`; this creates any
            // other dedicated structural table that is ABSENT, from v4's own
            // DDL dump (RULED 2026-10-03 by the human — the order's R3 said
            // "report it", which would have answered a false `degraded`). It
            // never touches an existing table, so the pass still runs no
            // ensure of its own.
            quilltap_core::db::table_shape::create_missing_structural_tables(
                ws.mount_index().map(|w| w.connection()),
                ws.llm_logs().map(|w| w.connection()),
                &mut ensure_failures,
            );
            // === end P4.D248 ===
            // === P4.D255 (v4 f5e953a3f, migrations add-wardrobe-wear-stats-table-v1 / seed-wardrobe-wear-stats-v1 / add-wardrobe-image-settings-field-v1) ===
            // The wardrobe wear ledger, then its one-time seed, then the
            // wardrobe-picture settings column — v4's registration order
            // (`migrations/scripts/index.ts:849-855`), BEFORE the index-family
            // backfill below, so the two hand indexes already exist when it
            // runs (its "present → skipped" arm). The ledger ensure STAMPS
            // `migrations_state` for each step that ran and skips a step either
            // app recorded (R-B: v4's seed has no once-only gate of its own —
            // an unstamped seed would make v4's next boot credit every current
            // outfit twice). The table step is fatal on failure like every
            // table ensure in this chain; a failed SEED rolls back, logs, is
            // not stamped and lets the boot continue (R-D). The column ensure
            // stamps nothing (a re-ADD is a no-op).
            let wear = quilltap_core::db::wardrobe_wear_stats_repair::ensure_wardrobe_wear_stats(main)?;
            if wear.table_created {
                tracing::info!(
                    target: "quilltap::boot",
                    migrationId = quilltap_core::db::wardrobe_wear_stats_repair::TABLE_MIGRATION_ID,
                    "Created the wardrobe wear ledger"
                );
            }
            if let quilltap_core::db::wardrobe_wear_stats_repair::SeedOutcome::Seeded {
                wears,
                chats_with_outfits,
            } = wear.seed
            {
                tracing::info!(
                    target: "quilltap::boot",
                    migrationId = quilltap_core::db::wardrobe_wear_stats_repair::SEED_MIGRATION_ID,
                    wears,
                    chatsWithOutfits = chats_with_outfits,
                    "Seeded the wardrobe wear ledger from current outfits"
                );
            }
            quilltap_core::db::chat_settings_wardrobe_image_settings_repair::ensure_chat_settings_wardrobe_image_settings(main)?;
            // === end P4.D255 ===
            // === P4.160 (P4.153's OPEN item) ===
            // v4's MIGRATION-created index family on an instance provisioned
            // before P4.153 (the 56 absent names + the PLAIN `mp_path`),
            // from `migration_indexes.json` alone. LAST, so a table the step
            // above just created gets its indexes this boot and every ensure
            // before it (the folder collapse, the built-in mounts' UNIQUE
            // `mp_path` request) has run. NON-FATAL (R-C): the pass logs and
            // reports every failure itself; only an unparseable embedded
            // artifact answers `Err` here. Not an `EnsureFailures` entry — no
            // v4 repository ensure makes these indexes, so recording one would
            // report a `degraded` v4 never reports.
            if let Err(e) =
                quilltap_core::db::migration_index_family_repair::ensure_migration_index_family(
                    main,
                    ws.mount_index().map(|w| w.connection()),
                    ws.llm_logs().map(|w| w.connection()),
                )
            {
                // Distinct from the per-partition `Migration index backfill
                // failed {partition, error}` — this arm never reached a
                // partition (the embedded artifact did not parse).
                tracing::error!(
                    target: "quilltap::boot",
                    error = %e,
                    "Migration index backfill could not start"
                );
            }
            // === end P4.160 ===
            Ok(ensure_failures)
        })
    })
    .join()
    .map_err(|_| "built-in seed thread panicked".to_string())?
    .map_err(|e| format!("built-in seed failed: {e}"))
}

/// v4's PHASE 3.1 (`lib/startup/verify-structural-tables.ts`, `e5c6bd0c0`, bug
/// 176): check every structural table once, read-only, and answer the
/// problems for the host's slot. The shape check runs on the read pools (an
/// absent partition answers `PartitionUnavailable` → skipped, R4; a DEGRADED
/// one — P4.159, dogfood #150 — is never read: it is counted with v4's
/// `<label> database unavailable: …` problem per table); the ensure
/// failures this boot already logged are REUSED, never re-run. The pass logs
/// v4's lines itself (`table_shape::verify_structural_tables`). v4 wraps the
/// phase in its own catch (`instrumentation.ts:568-582`): a failure there logs
/// WARN `Structural table check could not run, continuing startup` and the
/// boot goes on with nothing recorded. The pass returns no error of its own,
/// so here that arm is a PANIC inside it.
fn verify_structural_tables_at_boot(db: &Db, ensure_failures: &EnsureFailures) -> Vec<String> {
    use quilltap_core::db::runtime::PartitionState;
    use quilltap_core::db::table_shape::{
        find_table_shape_problem, verify_structural_tables, Partition, TableRead,
    };
    use quilltap_core::db::DbError;

    let pass = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        verify_structural_tables(ensure_failures, |table| {
            if db.partition_state(table.partition) == PartitionState::Degraded {
                return TableRead::PartitionDegraded;
            }
            let (name, fields) = (table.collection, table.fields);
            let read = match table.partition {
                Partition::Main => db.read_main(|c| find_table_shape_problem(c, name, fields)),
                Partition::MountIndex => {
                    db.read_mount_index(|c| find_table_shape_problem(c, name, fields))
                }
                Partition::LlmLogs => {
                    db.read_llm_logs(|c| find_table_shape_problem(c, name, fields))
                }
            };
            match read {
                Err(DbError::PartitionUnavailable(_)) => TableRead::PartitionAbsent,
                other => TableRead::Read(other),
            }
        })
    }));
    match pass {
        Ok(problems) => problems.into_iter().map(|p| p.problem).collect(),
        Err(_) => {
            tracing::warn!(
                target: "quilltap::boot",
                context = "instrumentation.register",
                error = "the structural table check panicked",
                "Structural table check could not run, continuing startup"
            );
            Vec::new()
        }
    }
}

/// v4 `deferResumable`'s WARN (`migrations/index.ts:73-81` at `e5c6bd0c0`),
/// the last line on both of the collapse's deferring arms.
fn log_collapse_deferred() {
    tracing::warn!(
        target: "quilltap::boot",
        context = "migrations.runMigrations",
        migrationId = quilltap_core::db::avatar_rolls_collapse_heal::MIGRATION_ID,
        "Resumable migration deferred to the next boot; continuing startup"
    );
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
/// ownership + rewrite `lastHeartbeat`. On LOSS (file vanished, or a record
/// that is no longer the one we wrote) the assembly's ordered teardown runs —
/// v4's inward-registered `handleShutdown` (bug 126) — and then the configured
/// handler; default `std::process::exit(1)`, the faithful v4 shutdown. Our own
/// shutdown's release flips the stop flag BEFORE unlinking, so the post-tick
/// stop check keeps a release from reading as a loss.
///
/// A mere OS hostname change is NOT a loss: ownership is
/// `lock::is_still_our_lock` (PID + `startedAt`), never the recorded label.
async fn heartbeat_loop(
    lock_path: PathBuf,
    teardown: Arc<AssemblyTeardown>,
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
            teardown.run();
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

/// v4 `scheduled-danger-scan.ts`: the summary-classification pre-check (v4
/// `3b463d6b1` — no user opted in, no loop) gates STARTING the loop at all (a
/// check failure also skips — v4 warns and returns); when
/// enabled, scan immediately and then every 10 min. Sweep errors are swallowed
/// (v4 catches + logs).
async fn danger_scan_loop(db: Db, mut stop: watch::Receiver<bool>, interval_ms: u64) {
    if !danger_scan::any_user_wants_summary_classification(&db).await {
        return;
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
