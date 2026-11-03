//! Turn control — the operator's two ways of cutting in on a Salon chain (v4
//! `lib/chat/turn-control.ts`, whole, at `322a5488e`; landed in `2b5e8ee24`).
//!
//! A Salon send runs its whole chain of character turns inside one SSE stream
//! (`run_send` → `process_message` → `execute_turn_chain`). While that stream
//! is open the operator can:
//!
//!   - **Raise a hand** (soft interrupt). The next character to take the floor
//!     is asked, in its own turn prompt, whether to yield to the operator
//!     (`[YIELD]`) or carry on regardless. The hand is consumed by that one
//!     turn; a decline lowers it silently. See [`crate::yield_signal`].
//!   - **Stop** (hard interrupt). The in-flight provider stream is abandoned at
//!     once; whatever prose had already streamed is kept as the character's
//!     reply, tools already run stay run, and no further tool call, recovery
//!     retry or chained turn starts.
//!
//! Each open stream registers a handle here for its lifetime
//! ([`register_active_turn`]), so code deep in the turn asks about *its own*
//! stream without a chat-id lookup — a stop aimed at a stream that is still
//! winding down never touches a fresh send made a moment later.
//!
//! The actions (`chatStopTurn`, `chatRaiseHand`) arrive on a different thread
//! from the send (the engine's action arm vs the spine's send thread), so the
//! registry is a process `static` (v4's `processSingleton`, bug 235). It is
//! in-process only: autonomous-room turns run in the job child in v4, where
//! nothing ever stops or interrupts them.
//!
//! ## Where v5 lands (P4.D335)
//!
//!   - **No `AsyncLocalStorage` twin.** v4 makes the handle the ambient store
//!     of the stream; v5 threads it EXPLICITLY — `OrchestratorDeps.turn_control`
//!     → `FinalizeOptions.turn_stopped` / the tool-loop opts / the recovery
//!     opts / the watchdog's [`StopSignal`] — and (P4.165, overturning
//!     P4.D335's R-4) `CarinaQueryDeps.turn_control` → `RunBrahmaConsole::run`
//!     → `run_brahma_query` → `RunOneShotToolLoopOptions.stop`, so an in-turn
//!     Brahma consult's stream ends at the stop as v4's does (the default core build has no
//!     `tokio::task_local!` — `activity_registry.rs` records why). A stream
//!     consumer added later without the handle is caught by the wrap census
//!     (`stream_watchdog_wrap_census`), not by a type.
//!   - **The enclave's autonomous turn is NOT registered** (v4's job child
//!     never registers): a stop against a chat whose only open turn is the
//!     enclave's answers `{stopped: 0, settled: true}` — a deliberate
//!     non-capability; registering it would make v5 strictly more capable.
//!   - **The primitives are runtime-agnostic** — an `AtomicBool` + a
//!     `tokio::sync::Notify` for the stop, a `tokio::sync::watch` for the
//!     settle (a waiter that subscribes AFTER the settle still reads `true`,
//!     which a bare `Notify::notify_waiters` would lose), so the engine's
//!     runtime and the send thread's current-thread runtime both reach them.
//!     No new dependency (R-J).
//!   - **`wait_for_earlier_turns` takes the handle explicitly** (P4.D348, bug
//!     258): v4 reads `self` from the ambient store; v5's caller (the spine's
//!     send thread) passes its own registration's handle, or `None` to wait on
//!     every open stream.
//!   - **Tests use UNIQUE chat ids and never clear the static** — it is one
//!     process-global, shared by every parallel test in a binary.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use serde::Serialize;
use tokio::sync::{watch, Notify};

const TARGET: &str = "quilltap::turn_control";

/// v4's default bounded wait for stopped streams to settle (`15_000` ms).
pub const DEFAULT_SETTLE_TIMEOUT: Duration = Duration::from_millis(15_000);

/// One open Salon stream (v4 `TurnControlEntry`).
#[derive(Debug)]
struct TurnControlEntry {
    chat_id: String,
    /// v4 `controller.signal.aborted`.
    stopped: AtomicBool,
    /// Wakes a watchdog parked on the next chunk the moment the stop lands.
    stop_notify: Notify,
    /// The operator's hand is up and no character has been asked yet.
    hand_raised: AtomicBool,
    /// `true` once the stream's work has finished, stopped or not.
    settled: watch::Sender<bool>,
}

impl TurnControlEntry {
    fn is_stopped(&self) -> bool {
        self.stopped.load(Ordering::SeqCst)
    }
}

type Registry = Mutex<HashMap<String, Vec<Arc<TurnControlEntry>>>>;

fn registry() -> &'static Registry {
    static REGISTRY: OnceLock<Registry> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

fn lock() -> std::sync::MutexGuard<'static, HashMap<String, Vec<Arc<TurnControlEntry>>>> {
    // A panic while holding the lock leaves the map consistent (every mutation
    // is a single insert / retain), so a poisoned lock is still usable.
    registry().lock().unwrap_or_else(|p| p.into_inner())
}

/// The handle one open stream's work carries (v4's ambient store, threaded).
#[derive(Debug, Clone)]
pub struct TurnControlHandle(Arc<TurnControlEntry>);

/// The stop half of a handle, cheap to clone into the stream watchdog.
#[derive(Debug, Clone)]
pub struct StopSignal(Arc<TurnControlEntry>);

impl StopSignal {
    /// Has the operator stopped this stream?
    pub fn stopped(&self) -> bool {
        self.0.is_stopped()
    }

    /// Resolve once the stream is stopped — at once when it already is (v4's
    /// `stopRequested` settles immediately on an aborted signal).
    pub async fn wait(&self) {
        let notified = self.0.stop_notify.notified();
        tokio::pin!(notified);
        // The documented `Notify` idiom: register interest BEFORE reading the
        // flag, so a stop landing between the read and the await is not lost.
        notified.as_mut().enable();
        if self.0.is_stopped() {
            return;
        }
        notified.await;
    }
}

/// Two handles are equal when they name the SAME open stream (the
/// `StreamingState` derive compares by identity, never by state).
impl PartialEq for TurnControlHandle {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl TurnControlHandle {
    /// The stop signal the stream watchdog races its next chunk against (v4
    /// `currentTurnStopSignal()`).
    pub fn stop_signal(&self) -> StopSignal {
        StopSignal(Arc::clone(&self.0))
    }

    /// Has the operator hard-stopped this stream? (v4 `isCurrentTurnStopped()`.)
    pub fn is_stopped(&self) -> bool {
        self.0.is_stopped()
    }

    /// The chat this stream belongs to.
    pub fn chat_id(&self) -> &str {
        &self.0.chat_id
    }

    /// Consume the operator's raised hand for the turn about to be built (v4
    /// `takeRaisedHand()`). Returns true at most once per raise: the first
    /// character asked is the only one asked. A stopped stream never offers a
    /// yield.
    pub fn take_raised_hand(&self) -> bool {
        let entry = &self.0;
        if entry.is_stopped() {
            return false;
        }
        if !entry.hand_raised.swap(false, Ordering::SeqCst) {
            return false;
        }
        tracing::debug!(
            target: TARGET,
            chat_id = %entry.chat_id,
            "[TurnControl] Raised hand consumed by the next speaker"
        );
        true
    }
}

/// The registration guard for one stream (v4 `runWithTurnControl`'s span).
/// Dropping it — however the work finishes, a panic included — unregisters
/// the handle and settles it.
#[derive(Debug)]
pub struct TurnControlRegistration {
    handle: TurnControlHandle,
}

impl TurnControlRegistration {
    /// A clone of the stream's handle, for threading into the turn.
    pub fn handle(&self) -> TurnControlHandle {
        self.handle.clone()
    }

    /// The chat this registration belongs to.
    pub fn chat_id(&self) -> &str {
        self.handle.chat_id()
    }
}

impl Drop for TurnControlRegistration {
    fn drop(&mut self) {
        let entry = &self.handle.0;
        let open_streams = {
            let mut map = lock();
            let remaining = match map.get_mut(&entry.chat_id) {
                Some(entries) => {
                    entries.retain(|e| !Arc::ptr_eq(e, entry));
                    entries.len()
                }
                None => 0,
            };
            if remaining == 0 {
                map.remove(&entry.chat_id);
            }
            remaining
        };
        entry.settled.send_replace(true);
        tracing::debug!(
            target: TARGET,
            chat_id = %entry.chat_id,
            stopped = entry.is_stopped(),
            open_streams = open_streams,
            "[TurnControl] Stream unregistered"
        );
    }
}

/// Register one Salon stream's work under a turn-control handle (v4
/// `runWithTurnControl`, minus the `AsyncLocalStorage` run — the caller threads
/// [`TurnControlRegistration::handle`] explicitly and drops the guard when the
/// work finishes).
pub fn register_active_turn(chat_id: &str) -> TurnControlRegistration {
    let (settled, _rx) = watch::channel(false);
    let entry = Arc::new(TurnControlEntry {
        chat_id: chat_id.to_string(),
        stopped: AtomicBool::new(false),
        stop_notify: Notify::new(),
        hand_raised: AtomicBool::new(false),
        settled,
    });
    let open_streams = {
        let mut map = lock();
        let entries = map.entry(chat_id.to_string()).or_default();
        entries.push(Arc::clone(&entry));
        entries.len()
    };
    tracing::debug!(
        target: TARGET,
        chat_id = %chat_id,
        open_streams = open_streams,
        "[TurnControl] Stream registered"
    );
    TurnControlRegistration {
        handle: TurnControlHandle(entry),
    }
}

/// Is any Salon stream open for this chat in this process? (v4 `hasActiveTurn`
/// — exported with no production caller; test-facing.)
pub fn has_active_turn(chat_id: &str) -> bool {
    lock().get(chat_id).is_some_and(|e| !e.is_empty())
}

/// Wait until every stream that opened for this chat BEFORE the calling one has
/// finished (v4 `waitForEarlierTurns`, `036b32a78`). A whisper uses this to
/// queue behind a chain already running rather than start a second speaker
/// beside it (bug 258).
///
/// Only earlier streams are awaited (the per-chat `Vec` is insertion-ordered,
/// as v4's `Set` iterates), so two waiting streams can never wait on each
/// other. A stream opened later is none of this one's business. Called with
/// `None` (outside a registered stream) it waits on every open stream for the
/// chat. Returns how many streams it waited on.
///
/// v5 has no `AsyncLocalStorage`: the caller passes its own handle explicitly
/// (`me`), and the cut is by identity (`Arc::ptr_eq`), never by state.
pub async fn wait_for_earlier_turns(chat_id: &str, me: Option<&TurnControlHandle>) -> usize {
    let earlier: Vec<Arc<TurnControlEntry>> = {
        let map = lock();
        let mut earlier = Vec::new();
        for entry in map.get(chat_id).map(Vec::as_slice).unwrap_or_default() {
            if me.is_some_and(|me| Arc::ptr_eq(entry, &me.0)) {
                break;
            }
            earlier.push(Arc::clone(entry));
        }
        earlier
    };
    if earlier.is_empty() {
        return 0;
    }

    tracing::debug!(
        target: TARGET,
        chat_id = %chat_id,
        streams = earlier.len(),
        "[TurnControl] Waiting for earlier stream(s) to finish"
    );
    futures_util::future::join_all(earlier.iter().map(|entry| {
        let mut rx = entry.settled.subscribe();
        async move {
            // `wait_for` reads the current value first, so a stream that settled
            // between the snapshot and here counts at once.
            let _ = rx.wait_for(|settled| *settled).await;
        }
    }))
    .await;
    tracing::debug!(
        target: TARGET,
        chat_id = %chat_id,
        streams = earlier.len(),
        "[TurnControl] Earlier stream(s) finished"
    );
    earlier.len()
}

/// Raise (or lower) the operator's hand on every open stream for the chat (v4
/// `setHandRaised`). Returns how many streams heard it — zero means nothing was
/// running, and the floor is already the operator's.
pub fn set_hand_raised(chat_id: &str, raised: bool) -> usize {
    let entries: Vec<Arc<TurnControlEntry>> = match lock().get(chat_id) {
        Some(entries) => entries.clone(),
        // v4 `return 0` BEFORE its log line.
        None => return 0,
    };
    let mut count = 0usize;
    for entry in &entries {
        if entry.is_stopped() {
            continue;
        }
        entry.hand_raised.store(raised, Ordering::SeqCst);
        count += 1;
    }
    tracing::debug!(
        target: TARGET,
        chat_id = %chat_id,
        raised = raised,
        streams = count,
        "[TurnControl] Hand state set"
    );
    count
}

/// v4 `StopActiveTurnsResult` — `{ stopped, settled }`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct StopActiveTurnsResult {
    /// How many open streams were told to stop.
    pub stopped: u64,
    /// Whether every stopped stream finished its cleanup inside the timeout.
    pub settled: bool,
}

/// Hard-stop every open stream for the chat and wait (bounded) for them to
/// finish saving what they had (v4 `stopActiveTurns`). The caller sets any
/// pause *before* calling, so the chain's next decision already reads it.
///
/// The open set is SNAPSHOTTED under the lock and the lock released before
/// anything is signalled: a stream registered after the snapshot is untouched.
pub async fn stop_active_turns(
    chat_id: &str,
    settle_timeout: Option<Duration>,
) -> StopActiveTurnsResult {
    let entries: Vec<Arc<TurnControlEntry>> = lock().get(chat_id).cloned().unwrap_or_default();
    if entries.is_empty() {
        tracing::debug!(
            target: TARGET,
            chat_id = %chat_id,
            "[TurnControl] Stop requested with no open stream"
        );
        return StopActiveTurnsResult {
            stopped: 0,
            settled: true,
        };
    }

    for entry in &entries {
        entry.hand_raised.store(false, Ordering::SeqCst);
        entry.stopped.store(true, Ordering::SeqCst);
        entry.stop_notify.notify_waiters();
    }
    tracing::info!(
        target: TARGET,
        chat_id = %chat_id,
        streams = entries.len(),
        "[TurnControl] Stopping open Salon stream(s)"
    );

    let timeout = settle_timeout.unwrap_or(DEFAULT_SETTLE_TIMEOUT);
    let all_settled = futures_util::future::join_all(entries.iter().map(|entry| {
        let mut rx = entry.settled.subscribe();
        async move {
            // `wait_for` reads the current value first, so a stream that settled
            // between the snapshot and here counts at once.
            let _ = rx.wait_for(|settled| *settled).await;
        }
    }));
    let settled = tokio::time::timeout(timeout, all_settled).await.is_ok();

    if !settled {
        tracing::warn!(
            target: TARGET,
            chat_id = %chat_id,
            timeout_ms = timeout.as_millis() as u64,
            "[TurnControl] Stopped stream(s) did not settle in time"
        );
    }
    StopActiveTurnsResult {
        stopped: entries.len() as u64,
        settled,
    }
}

#[cfg(test)]
mod tests {
    //! v4 `__tests__/unit/lib/chat/turn-control.test.ts` — the ten `it`s 1:1
    //! (unique chat ids; the static is never cleared). The differential over
    //! v4's REAL module is the harness's `turn_control_equivalence`.
    use super::*;

    fn cid(test: &str) -> String {
        format!("turn-control-{test}-{}", uuid::Uuid::new_v4())
    }

    #[tokio::test]
    async fn registers_a_stream_for_its_lifetime_only() {
        let chat = cid("lifetime");
        assert!(!has_active_turn(&chat));
        let reg = register_active_turn(&chat);
        assert!(has_active_turn(&chat));
        drop(reg);
        assert!(!has_active_turn(&chat));
    }

    #[tokio::test]
    async fn unregisters_a_stream_that_throws() {
        let chat = cid("throws");
        let c2 = chat.clone();
        let r = std::thread::spawn(move || {
            let _reg = register_active_turn(&c2);
            panic!("boom");
        })
        .join();
        assert!(r.is_err());
        assert!(!has_active_turn(&chat));
    }

    #[tokio::test]
    async fn reports_no_stream_to_raise_a_hand_on_when_nothing_is_running() {
        let chat = cid("nothing");
        assert_eq!(set_hand_raised(&chat, true), 0);
    }

    #[tokio::test]
    async fn stops_the_running_stream_waits_for_it_to_settle_and_clears_its_hand() {
        let chat = cid("stops");
        let reg = register_active_turn(&chat);
        let handle = reg.handle();
        set_hand_raised(&chat, true);
        let worker = tokio::spawn(async move {
            handle.stop_signal().wait().await;
            let seen = (handle.is_stopped(), handle.take_raised_hand());
            drop(reg);
            seen
        });
        let result = stop_active_turns(&chat, None).await;
        assert_eq!(
            result,
            StopActiveTurnsResult {
                stopped: 1,
                settled: true
            }
        );
        assert_eq!(worker.await.unwrap(), (true, false));
    }

    #[tokio::test]
    async fn gives_up_waiting_on_a_stream_that_will_not_settle() {
        let chat = cid("timeout");
        let reg = register_active_turn(&chat);
        let result = stop_active_turns(&chat, Some(Duration::from_millis(20))).await;
        assert_eq!(
            result,
            StopActiveTurnsResult {
                stopped: 1,
                settled: false
            }
        );
        drop(reg);
    }

    #[tokio::test]
    async fn never_touches_a_fresh_stream_opened_after_the_stop() {
        let chat = cid("fresh");
        let old = register_active_turn(&chat);
        let old_handle = old.handle();
        let stopping = {
            let chat = chat.clone();
            tokio::spawn(async move { stop_active_turns(&chat, None).await })
        };
        old_handle.stop_signal().wait().await;
        let fresh = register_active_turn(&chat);
        drop(old);
        assert_eq!(stopping.await.unwrap().stopped, 1);
        assert!(!fresh.handle().is_stopped());
        drop(fresh);
    }

    #[tokio::test]
    async fn hands_a_raised_hand_to_the_first_turn_that_asks_and_only_once() {
        let chat = cid("hand");
        let reg = register_active_turn(&chat);
        assert_eq!(set_hand_raised(&chat, true), 1);
        let h = reg.handle();
        assert!(h.take_raised_hand());
        assert!(!h.take_raised_hand());
    }

    #[tokio::test]
    async fn lowers_a_hand_before_anyone_is_asked() {
        let chat = cid("lower");
        let reg = register_active_turn(&chat);
        set_hand_raised(&chat, true);
        assert_eq!(set_hand_raised(&chat, false), 1);
        assert!(!reg.handle().take_raised_hand());
    }

    #[tokio::test]
    async fn a_stopped_stream_never_hears_a_later_raise() {
        let chat = cid("stop-hand");
        let reg = register_active_turn(&chat);
        set_hand_raised(&chat, true);
        let h = reg.handle();
        let stopping = {
            let chat = chat.clone();
            tokio::spawn(async move { stop_active_turns(&chat, None).await })
        };
        h.stop_signal().wait().await;
        assert!(!h.take_raised_hand());
        // A stopped stream does not hear a later raise either.
        assert_eq!(set_hand_raised(&chat, true), 0);
        drop(reg);
        stopping.await.unwrap();
    }

    #[tokio::test]
    async fn has_no_handle_outside_a_registered_stream() {
        // v4: `isCurrentTurnStopped()` / `takeRaisedHand()` outside `run` are
        // false — v5's twin is a `None` handle at every call site.
        let none: Option<&TurnControlHandle> = None;
        assert!(!none.is_some_and(|h| h.is_stopped()));
        assert!(!none.is_some_and(|h| h.take_raised_hand()));
    }

    #[tokio::test]
    async fn is_a_no_op_stop_when_nothing_is_running() {
        let chat = cid("none");
        assert_eq!(
            stop_active_turns(&chat, None).await,
            StopActiveTurnsResult {
                stopped: 0,
                settled: true
            }
        );
    }

    // ── waitForEarlierTurns (bug 258) — v4's four `it`s, then a v5 guard ──

    #[tokio::test]
    async fn wait_returns_at_once_when_no_earlier_stream_is_open() {
        let chat = cid("w1");
        let reg = register_active_turn(&chat);
        assert_eq!(wait_for_earlier_turns(&chat, Some(&reg.handle())).await, 0);
    }

    #[tokio::test]
    async fn wait_waits_for_a_stream_opened_earlier_to_finish() {
        let chat = cid("w2");
        let order = Arc::new(Mutex::new(Vec::<&str>::new()));
        let chain = register_active_turn(&chat);
        let whisper = register_active_turn(&chat);
        let (o, c) = (Arc::clone(&order), chat.clone());
        let waiter = tokio::spawn(async move {
            let waited = wait_for_earlier_turns(&c, Some(&whisper.handle())).await;
            o.lock().unwrap().push("whisper runs");
            waited
        });
        tokio::task::yield_now().await;
        assert!(order.lock().unwrap().is_empty());
        order.lock().unwrap().push("chain done");
        drop(chain);
        assert_eq!(waiter.await.unwrap(), 1);
        assert_eq!(*order.lock().unwrap(), vec!["chain done", "whisper runs"]);
    }

    #[tokio::test]
    async fn wait_never_waits_on_a_stream_opened_after_it() {
        let chat = cid("w3");
        let chain = register_active_turn(&chat);
        let first = register_active_turn(&chat);
        let second = register_active_turn(&chat);
        let (c1, c2) = (chat.clone(), chat.clone());
        let first_task = tokio::spawn(async move {
            let n = wait_for_earlier_turns(&c1, Some(&first.handle())).await;
            drop(first);
            n
        });
        let second_task = tokio::spawn(async move {
            let n = wait_for_earlier_turns(&c2, Some(&second.handle())).await;
            drop(second);
            n
        });
        // Let both reach their snapshot before the chain settles.
        tokio::task::yield_now().await;
        drop(chain);
        let bounded = Duration::from_secs(5);
        assert_eq!(
            tokio::time::timeout(bounded, first_task)
                .await
                .unwrap()
                .unwrap(),
            1
        );
        assert_eq!(
            tokio::time::timeout(bounded, second_task)
                .await
                .unwrap()
                .unwrap(),
            2
        );
    }

    #[tokio::test]
    async fn wait_ignores_streams_in_other_chats() {
        let chat = cid("w4");
        let other = register_active_turn(&cid("w4-other"));
        let reg = register_active_turn(&chat);
        assert_eq!(wait_for_earlier_turns(&chat, Some(&reg.handle())).await, 0);
        drop(other);
    }

    #[tokio::test]
    async fn wait_outside_a_registered_stream_waits_on_every_open_stream() {
        let chat = cid("w5");
        let a = register_active_turn(&chat);
        let b = register_active_turn(&chat);
        let c = chat.clone();
        let waiter = tokio::spawn(async move { wait_for_earlier_turns(&c, None).await });
        tokio::task::yield_now().await;
        drop(a);
        drop(b);
        assert_eq!(waiter.await.unwrap(), 2);
    }

    /// Beyond v4's suite (P4.D348 Tier 2 item 25): with three streams the
    /// middle waiter settles before the last one proceeds — a regression guard
    /// on the `ptr_eq` cut.
    #[tokio::test]
    async fn wait_three_streams_the_middle_settles_before_the_last() {
        let chat = cid("w6");
        let order = Arc::new(Mutex::new(Vec::<&str>::new()));
        let chain = register_active_turn(&chat);
        let middle = register_active_turn(&chat);
        let last = register_active_turn(&chat);
        let (o1, c1) = (Arc::clone(&order), chat.clone());
        let mid_task = tokio::spawn(async move {
            wait_for_earlier_turns(&c1, Some(&middle.handle())).await;
            o1.lock().unwrap().push("middle");
            drop(middle);
        });
        let (o2, c2) = (Arc::clone(&order), chat.clone());
        let last_task = tokio::spawn(async move {
            let n = wait_for_earlier_turns(&c2, Some(&last.handle())).await;
            o2.lock().unwrap().push("last");
            n
        });
        tokio::task::yield_now().await;
        drop(chain);
        mid_task.await.unwrap();
        assert_eq!(last_task.await.unwrap(), 2);
        assert_eq!(*order.lock().unwrap(), vec!["middle", "last"]);
    }

    #[test]
    fn the_result_serializes_in_v4_key_order() {
        assert_eq!(
            serde_json::to_string(&StopActiveTurnsResult {
                stopped: 1,
                settled: true
            })
            .unwrap(),
            r#"{"stopped":1,"settled":true}"#
        );
    }
}
