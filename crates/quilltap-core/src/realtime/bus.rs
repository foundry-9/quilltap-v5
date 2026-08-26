//! The realtime invalidation bus (v4 `lib/realtime/bus.ts`, `f3892158d`).
//!
//! The one fan-out point for "this slice of server state just changed."
//! Publishers call [`publish_realtime`]; every connected client gets a ~40-byte
//! hint and decides for itself whether it cares.
//!
//! ## Design notes, ported
//!
//! * **Coalescing is mandatory, not a nicety.** An `EMBEDDING_REINDEX_ALL`
//!   sweep completes thousands of jobs and a memory-extraction batch commits
//!   dozens of writes; without a trailing-edge debounce per `topic`/`topic:id`
//!   the stream would carry a storm the UI cannot use. Clients tolerate both
//!   duplicates (invalidation is idempotent) and gaps (a reconnect invalidates
//!   everything), so collapsing is always safe.
//! * **A global, not a threaded handle.** v4 uses a `globalThis` singleton so
//!   services publish without signature churn; v5 uses a `static`. The
//!   `globalThis` part of v4's reason (surviving Next dev HMR) has no v5
//!   analogue.
//! * **Publishing before arming is a silent no-op.** v4's equivalent is its
//!   `IS_JOB_CHILD` guard — the forked job child owns no sockets, so every
//!   publish there is dropped, and shared code like `queue-service` needs no
//!   guard of its own. v5 has no child; the shape that remains is "the bus has
//!   not been armed yet (or the engine is gone)", which covers unit tests,
//!   the CLI's direct-core mode, and anything constructed before boot.
//!
//! ## What v5 does NOT have to port
//!
//! v4's fan-out walks a socket set, swallowing per-socket failures and dropping
//! the socket. **v5's fan-out IS the broadcast channel** — one `send` on the
//! engine's `broadcast::Sender<Event>`, whose per-subscriber delivery, lag, and
//! teardown the channel already owns. So `attachRealtimeSocket`,
//! `realtimeListenerCount`, and the per-socket drop legs have no twin, and the
//! "fanned out {delivered}" debug line becomes an emit-time line: the count of
//! receivers is the channel's, not ours. These are debug logs, not contractual
//! output, so this comment is the record rather than a capturing-layer pin.
//!
//! ## The spawn seam (the STOP rule: no timers in this core)
//!
//! A trailing-edge debounce needs a timer, and `quilltap-core` deliberately has
//! no tokio scheduler in its default build (`Cargo.toml`:
//! `default-features = false, features = ["sync", "time"]`; `tokio::spawn`
//! lives behind `rt`). That is the same rule `services::job_runner` states in
//! its header — the core decides *when*, the host driver *schedules* it. So
//! [`arm_realtime_bus`] takes both the engine's event sender AND a spawner from
//! the composition root, which is where a runtime actually exists. v4's
//! `timer.unref?.()` — "a pending hint must never hold the process open" —
//! carries over as a property of the host's spawner: a detached task the
//! runtime drops at shutdown.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex, OnceLock};

use tokio::sync::broadcast;

use crate::api::types::Event;
use crate::realtime::types::{RealtimeHint, RealtimeTopic};

/// Trailing-edge debounce window (v4 `COALESCE_WINDOW_MS`). Long enough to
/// swallow a job batch's worth of completions, short enough that a chip
/// lighting up still feels immediate.
pub const COALESCE_WINDOW_MS: u64 = 250;

/// How the bus gets a task onto a runtime. The composition root supplies it;
/// see the module header on why the core cannot spawn for itself.
pub type BusSpawner = Arc<dyn Fn(Pin<Box<dyn Future<Output = ()> + Send>>) + Send + Sync>;

/// The wall clock the flushed hint is stamped with (v4's `Date.now()` inside
/// the timer, NOT at publish time — a coalesced burst reports the flush).
type NowMs = fn() -> i64;

struct Bus {
    events: broadcast::Sender<Event>,
    spawn: BusSpawner,
    now_ms: NowMs,
    /// Keyed `topic` or `topic:id`; the value counts absorbed publishes.
    pending: Mutex<HashMap<String, u32>>,
}

static BUS: OnceLock<Mutex<Option<Arc<Bus>>>> = OnceLock::new();

fn slot() -> &'static Mutex<Option<Arc<Bus>>> {
    BUS.get_or_init(|| Mutex::new(None))
}

fn current() -> Option<Arc<Bus>> {
    slot().lock().unwrap_or_else(|p| p.into_inner()).clone()
}

/// Arm the bus with the engine's event sender and a runtime spawner.
///
/// Called once by the composition root at boot. Arming again replaces the
/// previous wiring (a second instance being served, or a test).
pub fn arm_realtime_bus(events: broadcast::Sender<Event>, spawn: BusSpawner) {
    arm_realtime_bus_with_clock(events, spawn, crate::clock::now_unix_ms);
}

/// [`arm_realtime_bus`] with an injected clock — the tests' seam for pinning
/// the stamped `at`.
pub fn arm_realtime_bus_with_clock(
    events: broadcast::Sender<Event>,
    spawn: BusSpawner,
    now_ms: NowMs,
) {
    let bus = Arc::new(Bus {
        events,
        spawn,
        now_ms,
        pending: Mutex::new(HashMap::new()),
    });
    *slot().lock().unwrap_or_else(|p| p.into_inner()) = Some(bus);
}

/// Drop the wiring. Every subsequent publish is a no-op until the bus is armed
/// again. (Tests; and a host tearing an instance down.)
pub fn disarm_realtime_bus() {
    *slot().lock().unwrap_or_else(|p| p.into_inner()) = None;
}

fn pending_key(topic: RealtimeTopic, id: Option<&str>) -> String {
    match id {
        Some(id) => format!("{}:{}", topic.as_str(), id),
        None => topic.as_str().to_string(),
    }
}

/// Announce that `topic` — optionally, just row `id` within it — has changed
/// (v4 `publishRealtime`).
///
/// Cheap and fire-and-forget: safe to call from any chokepoint, including hot
/// ones. Repeated calls inside the debounce window collapse into a single
/// delivered hint on the trailing edge. A no-op before the bus is armed.
///
/// ```ignore
/// publish_realtime(RealtimeTopic::Jobs, None);
/// publish_realtime(RealtimeTopic::Chats, Some(chat_id));
/// ```
pub fn publish_realtime(topic: RealtimeTopic, id: Option<&str>) {
    let Some(bus) = current() else {
        return;
    };
    let key = pending_key(topic, id);

    {
        let mut pending = bus.pending.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(coalesced) = pending.get_mut(&key) {
            *coalesced += 1;
            tracing::debug!(
                target: "quilltap::realtime",
                topic = topic.as_str(),
                id = id.unwrap_or(""),
                coalesced = *coalesced,
                "Realtime publish coalesced",
            );
            return;
        }
        pending.insert(key.clone(), 0);
    }

    tracing::debug!(
        target: "quilltap::realtime",
        topic = topic.as_str(),
        id = id.unwrap_or(""),
        "Realtime publish queued",
    );

    let flush_bus = Arc::clone(&bus);
    let id_owned = id.map(str::to_string);
    (bus.spawn)(Box::pin(async move {
        tokio::time::sleep(std::time::Duration::from_millis(COALESCE_WINDOW_MS)).await;
        let coalesced = flush_bus
            .pending
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(&key);
        // The hint is stamped at FLUSH time, not publish time (v4's `Date.now()`
        // inside the timer callback).
        let hint = RealtimeHint::new(topic, id_owned.as_deref(), (flush_bus.now_ms)());
        // v4 fans out per socket and logs the delivered count; the broadcast
        // channel owns delivery here, so an error means only that nobody is
        // listening — never a reason to keep the hint.
        let delivered = flush_bus.events.send(Event::realtime(hint)).unwrap_or(0);
        tracing::debug!(
            target: "quilltap::realtime",
            topic = topic.as_str(),
            id = id_owned.as_deref().unwrap_or(""),
            delivered,
            coalesced = coalesced.unwrap_or(0),
            "Realtime hint emitted",
        );
    }));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::types::EventPayload;

    /// The bus is a process-global, so its tests serialize on this.
    static TEST_LOCK: Mutex<()> = Mutex::new(());

    struct BusTestGuard(std::sync::MutexGuard<'static, ()>);

    impl Drop for BusTestGuard {
        fn drop(&mut self) {
            disarm_realtime_bus();
            let _ = &self.0;
        }
    }

    /// Arm the bus onto a fresh broadcast channel with a fixed clock, returning
    /// the receiver + the serialization guard.
    fn armed(now: i64) -> (broadcast::Receiver<Event>, BusTestGuard) {
        let guard = BusTestGuard(TEST_LOCK.lock().unwrap_or_else(|p| p.into_inner()));
        let (tx, rx) = broadcast::channel(64);
        // Tests run under a tokio runtime, so `tokio::spawn` is available here
        // even though the core's default build cannot call it.
        let spawn: BusSpawner = Arc::new(|fut| {
            tokio::spawn(fut);
        });
        // A fixed clock: `now` is baked in via a fn pointer table of one.
        fn zero() -> i64 {
            0
        }
        if now == 0 {
            arm_realtime_bus_with_clock(tx, spawn, zero);
        } else {
            arm_realtime_bus_with_clock(tx, spawn, crate::clock::now_unix_ms);
        }
        (rx, guard)
    }

    fn hint_of(ev: &Event) -> &RealtimeHint {
        match &ev.payload {
            EventPayload::Realtime(h) => h,
            other => panic!("expected a realtime hint, got {other:?}"),
        }
    }

    /// Nothing is emitted synchronously — the debounce is TRAILING edge.
    #[tokio::test(start_paused = true)]
    async fn a_publish_emits_nothing_before_the_window_closes() {
        let (mut rx, _g) = armed(0);
        publish_realtime(RealtimeTopic::Jobs, None);

        tokio::time::sleep(std::time::Duration::from_millis(COALESCE_WINDOW_MS - 1)).await;
        assert!(rx.try_recv().is_err(), "emitted before the window closed");

        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
        let ev = rx.try_recv().expect("one hint after the window");
        assert_eq!(hint_of(&ev).topic, "jobs");
        assert!(rx.try_recv().is_err(), "exactly one");
    }

    /// A storm inside the window collapses to ONE hint.
    #[tokio::test(start_paused = true)]
    async fn a_burst_of_publishes_coalesces_into_one_hint() {
        let (mut rx, _g) = armed(0);
        for _ in 0..12 {
            publish_realtime(RealtimeTopic::Jobs, None);
        }
        tokio::time::sleep(std::time::Duration::from_millis(COALESCE_WINDOW_MS + 5)).await;

        assert_eq!(hint_of(&rx.try_recv().expect("one hint")).topic, "jobs");
        assert!(rx.try_recv().is_err(), "twelve publishes, one hint");
    }

    /// Coalescing is PER KEY: different topics, and different ids within one
    /// topic, do not collapse into each other.
    #[tokio::test(start_paused = true)]
    async fn coalescing_is_keyed_by_topic_and_id() {
        let (mut rx, _g) = armed(0);
        publish_realtime(RealtimeTopic::Jobs, None);
        publish_realtime(RealtimeTopic::Chats, Some("c-1"));
        publish_realtime(RealtimeTopic::Chats, Some("c-2"));
        // A collection-wide hint for a topic that also has scoped hints is its
        // own key, not a duplicate of either.
        publish_realtime(RealtimeTopic::Chats, None);
        // …and repeats of each collapse.
        publish_realtime(RealtimeTopic::Chats, Some("c-1"));
        publish_realtime(RealtimeTopic::Jobs, None);

        tokio::time::sleep(std::time::Duration::from_millis(COALESCE_WINDOW_MS + 5)).await;

        let mut seen: Vec<(String, Option<String>)> = Vec::new();
        while let Ok(ev) = rx.try_recv() {
            let h = hint_of(&ev);
            seen.push((h.topic.clone(), h.id.clone()));
        }
        seen.sort();
        assert_eq!(
            seen,
            vec![
                ("chats".to_string(), None),
                ("chats".to_string(), Some("c-1".to_string())),
                ("chats".to_string(), Some("c-2".to_string())),
                ("jobs".to_string(), None),
            ]
        );
    }

    /// After a window flushes, the key is free again — a later publish is a new
    /// hint, not a swallowed duplicate.
    #[tokio::test(start_paused = true)]
    async fn a_key_rearms_after_its_window_flushes() {
        let (mut rx, _g) = armed(0);
        publish_realtime(RealtimeTopic::Jobs, None);
        tokio::time::sleep(std::time::Duration::from_millis(COALESCE_WINDOW_MS + 5)).await;
        rx.try_recv().expect("first");

        publish_realtime(RealtimeTopic::Jobs, None);
        tokio::time::sleep(std::time::Duration::from_millis(COALESCE_WINDOW_MS + 5)).await;
        rx.try_recv().expect("second");
    }

    /// The stamp is taken when the hint FLUSHES, not when it was queued (v4
    /// calls `Date.now()` inside the timer).
    #[tokio::test(start_paused = true)]
    async fn at_is_stamped_at_flush_time() {
        let (mut rx, _g) = armed(1);
        let before = crate::clock::now_unix_ms();
        publish_realtime(RealtimeTopic::Jobs, None);
        tokio::time::sleep(std::time::Duration::from_millis(COALESCE_WINDOW_MS + 5)).await;
        let ev = rx.try_recv().expect("hint");
        assert!(
            hint_of(&ev).at >= before,
            "the hint should be stamped no earlier than the publish"
        );
    }

    /// Publishing with no bus armed is silent and harmless — v4's job-child
    /// no-op, in the shape v5 has for it.
    #[tokio::test(start_paused = true)]
    async fn publishing_before_arming_is_a_silent_noop() {
        let _g = BusTestGuard(TEST_LOCK.lock().unwrap_or_else(|p| p.into_inner()));
        disarm_realtime_bus();
        publish_realtime(RealtimeTopic::Jobs, None);
        publish_realtime(RealtimeTopic::Chats, Some("c-1"));
        tokio::time::sleep(std::time::Duration::from_millis(COALESCE_WINDOW_MS + 5)).await;
        // Arming AFTER the fact must not deliver the dropped publishes.
        let (tx, mut rx) = broadcast::channel(8);
        let spawn: BusSpawner = Arc::new(|fut| {
            tokio::spawn(fut);
        });
        arm_realtime_bus(tx, spawn);
        tokio::time::sleep(std::time::Duration::from_millis(COALESCE_WINDOW_MS + 5)).await;
        assert!(rx.try_recv().is_err(), "a pre-arming publish is dropped");
    }

    /// A publish with NO subscriber must not panic or wedge the key — v4
    /// logs "dropped — no listeners" and moves on.
    #[tokio::test(start_paused = true)]
    async fn a_publish_with_no_listeners_is_harmless() {
        let (rx, _g) = armed(0);
        drop(rx);
        publish_realtime(RealtimeTopic::Jobs, None);
        tokio::time::sleep(std::time::Duration::from_millis(COALESCE_WINDOW_MS + 5)).await;

        // …and the key rearmed, so a later publish with a listener still lands.
        let mut rx2 = {
            let bus = current().expect("armed");
            bus.events.subscribe()
        };
        publish_realtime(RealtimeTopic::Jobs, None);
        tokio::time::sleep(std::time::Duration::from_millis(COALESCE_WINDOW_MS + 5)).await;
        assert!(rx2.try_recv().is_ok());
    }
}
