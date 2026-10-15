//! R-N — the process-global maintenance runner the host installs
//! (`ConsolidationRunner` → `MaintenanceRunner`; the P4.D271 R-E shape).
//!
//! [`super::run::run_maintenance`] is generic over the completion and
//! embedding providers, which only the host can build. The host installs one
//! runner at boot ([`install_maintenance_runner`]); the `maintain` dry run
//! (in-process, in the API) and the `MEMORY_MAINTENANCE` job handler reach the
//! model through it. Without one, the dry run answers the loud typed refusal
//! AFTER v4's 400 / 404 arms (R-N, L-5).

use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, RwLock};

use super::run::MaintenanceRun;
use super::types::{MaintenanceContext, MaintenanceRequest};
use crate::db::runtime::Db;
use crate::db::DbError;

/// The boxed future a [`MaintenanceRunner`] answers (boxed: the run's future
/// is large — the debug-build dispatch stack overflow, memory note
/// `unify-review-finding-shapes`).
pub type MaintenanceRunFuture<'a> =
    Pin<Box<dyn Future<Output = Result<MaintenanceRun, DbError>> + Send + 'a>>;

/// Runs [`super::run::run_maintenance`] over the providers it owns.
pub trait MaintenanceRunner: Send + Sync {
    fn run<'a>(
        &'a self,
        db: &'a Db,
        request: MaintenanceRequest,
        ctx: MaintenanceContext,
    ) -> MaintenanceRunFuture<'a>;
}

fn runner_slot() -> &'static RwLock<Option<Arc<dyn MaintenanceRunner>>> {
    static SLOT: RwLock<Option<Arc<dyn MaintenanceRunner>>> = RwLock::new(None);
    &SLOT
}

/// Install the process's maintenance runner (the host, once at boot; a test
/// may swap in a canned one).
pub fn install_maintenance_runner(runner: Arc<dyn MaintenanceRunner>) {
    *runner_slot().write().unwrap_or_else(|p| p.into_inner()) = Some(runner);
}

/// The installed runner, if any.
pub fn maintenance_runner() -> Option<Arc<dyn MaintenanceRunner>> {
    runner_slot()
        .read()
        .unwrap_or_else(|p| p.into_inner())
        .clone()
}

/// Test seam: clear the installed runner.
pub fn clear_maintenance_runner() {
    *runner_slot().write().unwrap_or_else(|p| p.into_inner()) = None;
}
