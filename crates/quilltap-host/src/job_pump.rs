//! The host job-pump control seam (P4.9G1) — the live implementation of
//! `quilltap_core::api::system_data::JobPumpControl`.
//!
//! v4 controls a forked child process (`processor-host.ts`). v5 runs the job
//! runner in-process (`host::pump_loop`), so this seam wraps the shared
//! `running` gate + the pump `wake` handle:
//!
//! - `start()` — v4 `startProcessor`/`ensureProcessorRunning`: set running, wake.
//! - `stop()`  — v4 `stopProcessor`: clear running (the pump loop stops claiming
//!   new jobs; an in-flight job finishes on its own — v5 never hard-kills a
//!   running handler mid-job, a deliberate divergence from v4's SIGTERM).
//! - `wake()`  — v4 `wakeProcessor`: nudge the dispatcher so a new concurrency
//!   cap applies without waiting for the 2 s poll.
//! - `status()` — v4 `getProcessorStatus`: `running` reflects the gate. The
//!   in-process runner does not expose a live in-flight counter, so
//!   `processing`/`inFlight` are reported as idle (0/false) — a documented
//!   simplification vs v4's child-dispatcher snapshot (the tasks-queue "running
//!   now" indicator is coarser in v5; the differential pins the status via a
//!   test double so this does not affect equivalence).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use tokio::sync::Notify;

use quilltap_core::api::system_data::{JobPumpControl, ProcessorStatus};

/// The live job-pump control shared with `host::pump_loop`.
pub struct HostJobPump {
    running: Arc<AtomicBool>,
    wake: Arc<Notify>,
}

impl HostJobPump {
    pub fn new(running: Arc<AtomicBool>, wake: Arc<Notify>) -> Self {
        Self { running, wake }
    }
}

impl JobPumpControl for HostJobPump {
    fn status(&self) -> ProcessorStatus {
        ProcessorStatus {
            running: self.running.load(Ordering::Relaxed),
            processing: false,
            in_flight: 0,
            child_crashed: false,
        }
    }

    fn start(&self) {
        self.running.store(true, Ordering::Relaxed);
        self.wake.notify_one();
    }

    fn stop(&self) {
        self.running.store(false, Ordering::Relaxed);
    }

    fn wake(&self) {
        self.wake.notify_one();
    }
}
