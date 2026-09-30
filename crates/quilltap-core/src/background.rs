//! The fire-and-forget spawner (P4.120) — v4's `void promise.catch(…)`.
//!
//! v4 kicks background work off with a bare `void asyncCall().catch(log)` (the
//! upload's auto-describe, the photo save's embedding enqueue). The Node event
//! loop is always there to carry the promise; this crate is not so lucky —
//! `quilltap-core`'s default build has no tokio scheduler (`Cargo.toml`:
//! `default-features = false, features = ["sync", "time"]`), the same rule
//! [`crate::realtime::bus`] and `services::job_runner` state: the core decides
//! *what*, the host *schedules*.
//!
//! So this is [`crate::realtime::bus`]'s arrangement, minus the bus: a
//! process-global slot the composition root arms once with a runtime spawner
//! (`quilltap-host`, `rt_handle.spawn`), and a [`spawn_background`] any service
//! can call without threading a handle through its signature. **Unarmed** —
//! the differential harness, the CLI's direct-core mode, every unit test — the
//! call answers `false` and the caller logs a DEBUG line and carries on. It
//! never blocks and never panics.
//!
//! The global (rather than an engine field) is deliberate: the three photo
//! paths that need it — the chat upload, `save_image_to_album`, and the
//! `describe_image` tool — are reached from the engine, the Salon turn's tool
//! runner and the enclave, and only a process-wide slot reaches all of them
//! without a required field on the shared turn-deps structs.

use std::future::Future;
use std::pin::Pin;
use std::sync::{Mutex, OnceLock};

use crate::realtime::bus::BusSpawner;

/// How a background task gets onto a runtime (the bus's own spawner type — one
/// shape for "the host's `rt_handle.spawn`").
pub type BackgroundSpawner = BusSpawner;

static SPAWNER: OnceLock<Mutex<Option<BackgroundSpawner>>> = OnceLock::new();

std::thread_local! {
    /// A THREAD-SCOPED spawner that shadows the process-global one — the same
    /// test seam [`crate::realtime::bus::arm_realtime_bus_for_current_thread`]
    /// gives the bus, for the same reason: a globally armed spawner in one test
    /// would catch the spawns of every other test in the binary.
    static THREAD_SPAWNER: std::cell::RefCell<Option<BackgroundSpawner>> =
        const { std::cell::RefCell::new(None) };
}

fn slot() -> &'static Mutex<Option<BackgroundSpawner>> {
    SPAWNER.get_or_init(|| Mutex::new(None))
}

fn current() -> Option<BackgroundSpawner> {
    if let Some(s) = THREAD_SPAWNER.with(|s| s.borrow().clone()) {
        return Some(s);
    }
    slot().lock().unwrap_or_else(|p| p.into_inner()).clone()
}

/// Arm the process-global spawner. Called once by the composition root at boot;
/// arming again replaces the previous one.
pub fn arm_background_spawner(spawn: BackgroundSpawner) {
    *slot().lock().unwrap_or_else(|p| p.into_inner()) = Some(spawn);
}

/// Drop the process-global spawner (a host tearing an instance down; tests).
pub fn disarm_background_spawner() {
    *slot().lock().unwrap_or_else(|p| p.into_inner()) = None;
}

/// **The test seam** — arm a spawner visible only to the CURRENT THREAD.
/// Production never calls this.
pub fn arm_background_spawner_for_current_thread(spawn: BackgroundSpawner) {
    THREAD_SPAWNER.with(|s| *s.borrow_mut() = Some(spawn));
}

/// Drop the current thread's shadowing spawner.
pub fn disarm_background_spawner_for_current_thread() {
    THREAD_SPAWNER.with(|s| *s.borrow_mut() = None);
}

/// Hand `fut` to the armed runtime and return at once. `true` = spawned;
/// `false` = nothing is armed and `fut` was dropped unrun (the caller decides
/// what to say about that — every caller logs a DEBUG line).
pub fn spawn_background(fut: Pin<Box<dyn Future<Output = ()> + Send>>) -> bool {
    match current() {
        Some(spawn) => {
            spawn(fut);
            true
        }
        None => false,
    }
}

/// A spawner that runs each task on a fresh OS thread with its own
/// current-thread runtime — for tests that want the spawned work to really run
/// without depending on the caller's runtime.
#[cfg(test)]
pub fn thread_spawner() -> BackgroundSpawner {
    std::sync::Arc::new(|fut| {
        std::thread::spawn(move || {
            tokio::runtime::Builder::new_current_thread()
                .build()
                .expect("test runtime")
                .block_on(fut);
        });
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    #[test]
    fn unarmed_spawn_answers_false_and_never_runs() {
        disarm_background_spawner_for_current_thread();
        let ran = Arc::new(AtomicBool::new(false));
        let r = ran.clone();
        let spawned = spawn_background(Box::pin(async move {
            r.store(true, Ordering::SeqCst);
        }));
        assert!(!spawned);
        assert!(!ran.load(Ordering::SeqCst));
    }

    #[test]
    fn a_thread_scoped_spawner_receives_the_task() {
        let got = Arc::new(AtomicBool::new(false));
        let g = got.clone();
        arm_background_spawner_for_current_thread(Arc::new(move |_fut| {
            g.store(true, Ordering::SeqCst);
        }));
        assert!(spawn_background(Box::pin(async {})));
        assert!(got.load(Ordering::SeqCst));
        disarm_background_spawner_for_current_thread();
        assert!(!spawn_background(Box::pin(async {})));
    }
}
