//! The server state: either a running [`Host`] or a typed startup failure
//! (the lock-conflict / boot-error surface `/health` reports — the P4.1d
//! startup-conflict handoff).

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use quilltap_host::Host;
use serde_json::Value;

/// Why the host failed to boot (the `/health` vocabulary; v4's
/// `startupState` phases collapse to what exists in v5 — no
/// migrations/plugins/seeding phases).
pub enum StartupStatus {
    Running(Box<Host>),
    /// The single-instance lock is held elsewhere (v4 `lock-conflict`, 409).
    LockConflict {
        message: String,
        /// The classified lock info (reason + status), v4's `lockConflict`
        /// object collapsed to what the host lock classifier reports.
        lock_conflict: Value,
    },
    /// Any other boot failure (v4 `unhealthy`, 503).
    Failed {
        message: String,
    },
}

/// The state every route shares.
pub struct WebState {
    pub startup: StartupStatus,
    pub started: Instant,
    pub version: String,
    /// The SPA dist directory (D5 static serving); `None` serves the embedded
    /// placeholder page only.
    pub spa_dir: Option<PathBuf>,
    /// The instance root (`<base>/files` is the disk storage backend root).
    pub base_dir: PathBuf,
}

pub type SharedState = Arc<WebState>;

impl WebState {
    pub fn host(&self) -> Option<&Host> {
        match &self.startup {
            StartupStatus::Running(h) => Some(h),
            _ => None,
        }
    }

    /// Seconds since the server started (v4 `process.uptime()`).
    pub fn uptime_secs(&self) -> f64 {
        self.started.elapsed().as_secs_f64()
    }
}
