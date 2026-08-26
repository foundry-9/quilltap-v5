//! The in-flight activity registry (v4
//! `lib/background-jobs/activity-registry.ts`, `664cfca84`).
//!
//! The toolbar chips used to count only rows in `background_jobs`, which meant
//! every piece of work that happens inline in a request — the Lantern's
//! `generate_image` tool, the wardrobe avatar preview, the Concierge's
//! per-message classification, an embedding minted to answer a search — ran to
//! completion without a chip ever moving. This registry is the other half of
//! the readout: any code path that does work a user would expect to see in a
//! chip wraps itself in [`track_activity`], and the count is live for the whole
//! span, from the first token of prompt crafting to the moment the result
//! lands.
//!
//! ## v4's design notes, and where v5 lands
//!
//!   - **Global, not threaded through.** v4 chose a `globalThis` counter map
//!     deliberately (single-user, single-process). v5's twin is a `static`
//!     atomic array: readable from the jobs verb and writable from any service
//!     without threading a handle through ten call sites' signatures.
//!   - **Counters, not booleans.** Overlapping work of different kinds (a
//!     summarizer ticking up in the middle of an image generation) is the
//!     intended reading, and two concurrent images read as `Img 2`.
//!   - **`started` is monotonic.** The UI compares successive polls against it,
//!     so work that begins and ends between two polls still registers as a blip
//!     instead of vanishing. It increments when a span *ENDS*, and only if the
//!     span outlived [`BLIP_THRESHOLD_MS`] — a cache hit never makes a chip
//!     flicker.
//!
//! ## The child-IPC mirror does NOT port — and there is nothing to replace it
//!
//! v4's `applyChildActivityDelta` / `resetChildActivity` / `mirrorToParent` /
//! `isJobChild` exist because v4 job handlers run in a **forked child process**,
//! so the child's spans have to be mirrored to the parent over IPC and zeroed
//! when the child dies (a crash mid-generation would otherwise strand a chip
//! above zero). **v5's job runner is in-process** — see
//! `services/job_runner.rs`'s header, which records why the whole fork/IPC
//! apparatus dissolved under Rust's ownership rules. There is one process, so
//! `local` is the whole truth, `getActivityCounts()` has nothing to add to it,
//! and there is no crash-mirror to zero. The *accounting* semantics
//! (re-entrancy by kind, idempotent end, the blip threshold, the floor at zero)
//! are all ported; only the IPC plumbing is dropped.
//!
//! ## Attribution: v4's `AsyncLocalStorage`, without a runtime dependency
//!
//! v4 collapses same-kind nesting through an `AsyncLocalStorage<Set<Kind>>`.
//! The obvious Rust twin is `tokio::task_local!`, but that lives behind tokio's
//! `rt` feature and **the default `quilltap-core` build has no tokio scheduler
//! at all** (`Cargo.toml`: `default-features = false, features = ["sync",
//! "time"]`; a runtime only arrives with `native-transport`). So the
//! propagation is hand-rolled: [`Attributed`] sets a thread-local bitmask for
//! the duration of each `poll` of the wrapped future and restores it after,
//! which is exactly the mechanism `TaskLocalFuture` uses and works under any
//! executor — including none.
//!
//! ⚠ **Attribution does not cross `tokio::spawn`**, for the same reason v4's
//! `AsyncLocalStorage` does (it does, actually — Node propagates into
//! `setImmediate` chains; a spawned Rust task starts with an empty mask). A
//! spawned same-kind span would therefore COUNT where v4 collapses it. Surveyed
//! at the ten wrapped call sites for this round: none spawns same-kind work
//! (the `join_all` resolves inside the wardrobe/appearance paths are polled
//! within the caller's own poll, so they DO inherit the mask). Any future
//! wrapped path that spawns must record the divergence rather than silently
//! double-count.

use std::cell::Cell;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicI64, Ordering};
use std::task::{Context, Poll};

use tokio::time::{Duration, Instant};

use crate::services::activity_kinds::{ActivityCounts, ActivityKind, ACTIVITY_KINDS};

/// A span shorter than this is not worth telling the user about — a cached
/// classification, a memoized embedding. Only longer spans register as a blip.
/// (v4 `BLIP_THRESHOLD_MS`.)
pub const BLIP_THRESHOLD_MS: u64 = 250;

/// In-flight counts for work running in this process (v4's `local`; v4's
/// separate `child` mirror has no v5 analogue — see the module header).
static LOCAL: [AtomicI64; 5] = [const { AtomicI64::new(0) }; 5];

/// Monotonic count of spans that outlived the blip threshold (v4's `started`).
static STARTED: [AtomicI64; 5] = [const { AtomicI64::new(0) }; 5];

thread_local! {
    /// Kinds already accounted for by an enclosing span (or by the job row of
    /// the handler we are running inside), as a bitmask over
    /// [`ActivityKind::index`]. v4's `AsyncLocalStorage<ReadonlySet<Kind>>`.
    static ATTRIBUTED: Cell<u8> = const { Cell::new(0) };
}

const fn bit(kind: ActivityKind) -> u8 {
    1u8 << kind.index()
}

/// The kinds the current poll is already attributed to.
fn current_attribution() -> u8 {
    ATTRIBUTED.with(|c| c.get())
}

/// A future that runs `inner` with `mask` installed as the attribution set for
/// the duration of every poll — the runtime-free stand-in for v4's
/// `attributed.run(next, fn)`.
///
/// Boxed rather than pin-projected: these spans are coarse (an LLM call, an
/// image generation), so one allocation each is free, and it keeps the whole
/// module `unsafe`-less.
struct Attributed<F> {
    inner: Pin<Box<F>>,
    mask: u8,
}

impl<F: Future> Future for Attributed<F> {
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<F::Output> {
        let me = self.get_mut();
        let mask = me.mask;
        let previous = ATTRIBUTED.with(|c| c.replace(mask));
        let out = me.inner.as_mut().poll(cx);
        ATTRIBUTED.with(|c| c.set(previous));
        out
    }
}

/// A live span. Ending is **idempotent** (v4's `ended` latch): calling
/// [`ActivitySpan::end`] twice will not double-decrement.
///
/// **v5 addition, deliberate:** `Drop` ends the span too. v4 relies on its
/// `finally` always running, which in JavaScript it does — there is no
/// cancellation. A Rust future can be dropped mid-await, which would strand the
/// count exactly the way v4's child crash used to; the guard closes that. Every
/// ported call site still ends explicitly, so the drop leg only fires on
/// cancellation.
#[derive(Debug)]
pub struct ActivitySpan {
    kind: ActivityKind,
    started_at: Instant,
    ended: Cell<bool>,
}

impl ActivitySpan {
    /// Release the span: decrement (floored at zero) and, if it outlived
    /// [`BLIP_THRESHOLD_MS`], bump the monotonic started total. Idempotent.
    pub fn end(&self) {
        if self.ended.replace(true) {
            return;
        }
        let slot = &LOCAL[self.kind.index()];
        // v4 `Math.max(0, s.local[kind] - 1)`.
        let _ = slot.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |v| Some((v - 1).max(0)));
        if self.started_at.elapsed() >= Duration::from_millis(BLIP_THRESHOLD_MS) {
            STARTED[self.kind.index()].fetch_add(1, Ordering::SeqCst);
        }
    }
}

impl Drop for ActivitySpan {
    fn drop(&mut self) {
        self.end();
    }
}

/// Mark a span of work as started (v4 `beginActivity`). Returns the guard whose
/// [`end`](ActivitySpan::end) releases it.
///
/// Does **not** participate in same-kind collapsing — prefer [`track_activity`]
/// unless the start and end genuinely live in different scopes.
pub fn begin_activity(kind: ActivityKind) -> ActivitySpan {
    LOCAL[kind.index()].fetch_add(1, Ordering::SeqCst);
    ActivitySpan {
        kind,
        started_at: Instant::now(),
        ended: Cell::new(false),
    }
}

/// Run `fut` with `kind` counted as in flight for its whole duration, including
/// failures (v4 `trackActivity`). This is the normal way to register non-job
/// work.
///
/// Re-entrant by kind: if an enclosing span (or the job handler this is running
/// inside) already accounts for `kind`, this call is transparent. That makes it
/// safe to wrap a shared chokepoint — the Concierge classifier, the embedding
/// service — without inflating the chip when a job of the same kind calls it.
pub async fn track_activity<F: Future>(kind: ActivityKind, fut: F) -> F::Output {
    if current_attribution() & bit(kind) != 0 {
        return fut.await;
    }
    // The guard's `Drop` is the `finally`: a failure — or a cancelled future —
    // still ends the span.
    let _span = begin_activity(kind);
    with_attribution(kind, fut).await
}

/// Run a job handler attributed to its own activity kind *without* adding a
/// count (v4 `runAttributedToJob`) — the job's PENDING/PROCESSING row is
/// already the count, and has been since before the handler started. Inline
/// work of the same kind inside the handler then collapses into it; inline work
/// of any other kind still counts. A `None` kind passes through untouched.
pub async fn run_attributed_to_job<F: Future>(kind: Option<ActivityKind>, fut: F) -> F::Output {
    match kind {
        None => fut.await,
        Some(kind) => with_attribution(kind, fut).await,
    }
}

/// v4's `withAttribution`: add `kind` to the enclosing attribution set for the
/// duration of `fut`.
fn with_attribution<F: Future>(kind: ActivityKind, fut: F) -> Attributed<F> {
    Attributed {
        inner: Box::pin(fut),
        mask: current_attribution() | bit(kind),
    }
}

/// Current in-flight counts (v4 `getActivityCounts`; v4 additionally adds the
/// child mirror, which v5 has no analogue for — module header).
pub fn activity_counts() -> ActivityCounts {
    let mut out = ActivityCounts::default();
    for kind in ACTIVITY_KINDS {
        out.set(kind, LOCAL[kind.index()].load(Ordering::SeqCst));
    }
    out
}

/// Monotonic per-kind totals of spans that outlived the blip threshold since
/// this process booted (v4 `getActivityStartTotals`).
pub fn activity_start_totals() -> ActivityCounts {
    let mut out = ActivityCounts::default();
    for kind in ACTIVITY_KINDS {
        out.set(kind, STARTED[kind.index()].load(Ordering::SeqCst));
    }
    out
}

/// Test hook: drop all counters (v4 `__resetActivityRegistryForTests`).
///
/// The registry is process-global, so tests that touch it must also serialize —
/// see `registry_lock()` in this module's tests, and `ActivityTestGuard` for
/// consumers in other crates.
pub fn reset_activity_registry_for_tests() {
    for i in 0..5 {
        LOCAL[i].store(0, Ordering::SeqCst);
        STARTED[i].store(0, Ordering::SeqCst);
    }
    ATTRIBUTED.with(|c| c.set(0));
}

/// A serializing, self-resetting handle on the process-global registry for
/// tests in ANY crate: takes the lock, zeroes the counters, and releases on
/// drop. Without it, two `#[test]`s in the same binary race the same statics.
pub struct ActivityTestGuard(std::sync::MutexGuard<'static, ()>);

static TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

impl Default for ActivityTestGuard {
    fn default() -> Self {
        Self::new()
    }
}

impl ActivityTestGuard {
    /// Acquire the registry test lock and zero the counters.
    pub fn new() -> Self {
        let guard = TEST_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        reset_activity_registry_for_tests();
        Self(guard)
    }
}

impl Drop for ActivityTestGuard {
    fn drop(&mut self) {
        reset_activity_registry_for_tests();
        let _ = &self.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// v5 counterpart of v4's `__tests__/unit/background-jobs/
    /// activity-registry.test.ts`. Every semantic v4 pins there is mirrored;
    /// the four `child mirror` cases have no v5 analogue (module header).

    // ── counting ─────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn counts_a_span_for_its_whole_duration_and_releases_it_afterwards() {
        let _g = ActivityTestGuard::new();
        let mut observed = -1;
        track_activity(ActivityKind::Image, async {
            observed = activity_counts().image;
        })
        .await;
        assert_eq!(observed, 1);
        assert_eq!(activity_counts().image, 0);
    }

    #[tokio::test]
    async fn releases_the_count_when_the_work_fails() {
        let _g = ActivityTestGuard::new();
        let out: Result<(), &str> =
            track_activity(ActivityKind::Image, async { Err("provider exploded") }).await;
        assert_eq!(out, Err("provider exploded"));
        assert_eq!(activity_counts().image, 0);
    }

    #[tokio::test]
    async fn counts_concurrent_work_of_the_same_kind_separately() {
        let _g = ActivityTestGuard::new();
        let one = begin_activity(ActivityKind::Image);
        let two = begin_activity(ActivityKind::Image);
        assert_eq!(activity_counts().image, 2);
        one.end();
        assert_eq!(activity_counts().image, 1);
        two.end();
        assert_eq!(activity_counts().image, 0);
    }

    #[tokio::test]
    async fn ignores_a_duplicated_end() {
        let _g = ActivityTestGuard::new();
        let span = begin_activity(ActivityKind::Memory);
        span.end();
        span.end();
        assert_eq!(activity_counts().memory, 0);
    }

    /// ⚠ The case above CANNOT see a missing idempotence latch: the floor at
    /// zero absorbs the second decrement (1 → 0 → max(0, -1) = 0), so it stays
    /// green either way — and so does v4's, for the same reason. These two do
    /// see it. A concurrent span of the same kind gives the second decrement
    /// somewhere to land…
    #[tokio::test]
    async fn a_duplicated_end_does_not_steal_a_concurrent_span_s_count() {
        let _g = ActivityTestGuard::new();
        let one = begin_activity(ActivityKind::Memory);
        let two = begin_activity(ActivityKind::Memory);
        assert_eq!(activity_counts().memory, 2);
        one.end();
        one.end();
        assert_eq!(
            activity_counts().memory,
            1,
            "the second end must be a no-op, not a decrement of the other span"
        );
        two.end();
    }

    /// …and a long span's blip total counts the span once, not once per `end`.
    #[tokio::test(start_paused = true)]
    async fn a_duplicated_end_records_one_blip_not_two() {
        let _g = ActivityTestGuard::new();
        let span = begin_activity(ActivityKind::Memory);
        tokio::time::sleep(Duration::from_millis(BLIP_THRESHOLD_MS)).await;
        span.end();
        span.end();
        assert_eq!(activity_start_totals().memory, 1);
    }

    /// v5-only (v4 has no cancellation): a dropped span still releases.
    #[tokio::test]
    async fn a_dropped_span_still_releases_the_count() {
        let _g = ActivityTestGuard::new();
        {
            let _span = begin_activity(ActivityKind::Danger);
            assert_eq!(activity_counts().danger, 1);
        }
        assert_eq!(activity_counts().danger, 0);
    }

    /// v4's `never lets a stray decrement drive a count negative`, at the only
    /// v5 site that can decrement (v4's is the child-mirror leg).
    #[tokio::test]
    async fn never_lets_a_span_drive_a_count_negative() {
        let _g = ActivityTestGuard::new();
        let span = begin_activity(ActivityKind::Summary);
        span.end();
        // A second, independent span object over the same kind, already at zero.
        let rogue = ActivitySpan {
            kind: ActivityKind::Summary,
            started_at: Instant::now(),
            ended: Cell::new(false),
        };
        rogue.end();
        assert_eq!(activity_counts().summary, 0);
    }

    // ── same-kind collapsing ─────────────────────────────────────────────────

    #[tokio::test]
    async fn does_not_double_count_a_nested_span_of_the_same_kind() {
        let _g = ActivityTestGuard::new();
        let mut inner = -1;
        track_activity(ActivityKind::Image, async {
            track_activity(ActivityKind::Image, async {
                inner = activity_counts().image;
            })
            .await;
        })
        .await;
        assert_eq!(inner, 1);
    }

    #[tokio::test]
    async fn still_counts_a_nested_span_of_a_different_kind() {
        let _g = ActivityTestGuard::new();
        let mut seen = (-1, -1);
        track_activity(ActivityKind::Image, async {
            track_activity(ActivityKind::Danger, async {
                let c = activity_counts();
                seen = (c.image, c.danger);
            })
            .await;
        })
        .await;
        assert_eq!(seen, (1, 1));
    }

    #[tokio::test]
    async fn attributes_a_job_handler_to_its_kind_without_adding_a_count() {
        let _g = ActivityTestGuard::new();
        let mut inside_job = -1;
        let mut inside_nested = -1;
        run_attributed_to_job(Some(ActivityKind::Image), async {
            inside_job = activity_counts().image;
            // Inline image work inside an image job collapses into the job row.
            track_activity(ActivityKind::Image, async {
                inside_nested = activity_counts().image;
            })
            .await;
        })
        .await;
        assert_eq!(inside_job, 0);
        assert_eq!(inside_nested, 0);
    }

    #[tokio::test]
    async fn counts_other_kind_work_inside_an_attributed_job() {
        let _g = ActivityTestGuard::new();
        let mut danger = -1;
        run_attributed_to_job(Some(ActivityKind::Image), async {
            track_activity(ActivityKind::Danger, async {
                danger = activity_counts().danger;
            })
            .await;
        })
        .await;
        assert_eq!(danger, 1);
    }

    /// v4's `runAttributedToJob(null, fn)` passes through untouched — the
    /// uncounted job types (`LLM_LOG_CLEANUP`, the two autonomous ones).
    #[tokio::test]
    async fn a_null_kind_attributes_nothing() {
        let _g = ActivityTestGuard::new();
        let mut image = -1;
        run_attributed_to_job(None, async {
            track_activity(ActivityKind::Image, async {
                image = activity_counts().image;
            })
            .await;
        })
        .await;
        // Nothing was attributed, so the inline span counts normally.
        assert_eq!(image, 1);
    }

    /// The attribution set unwinds: work after a span ends is no longer
    /// collapsed into it.
    #[tokio::test]
    async fn attribution_unwinds_after_the_span_ends() {
        let _g = ActivityTestGuard::new();
        track_activity(ActivityKind::Image, async {}).await;
        let mut after = -1;
        track_activity(ActivityKind::Image, async {
            after = activity_counts().image;
        })
        .await;
        assert_eq!(after, 1);
    }

    // ── blip detection ───────────────────────────────────────────────────────

    #[tokio::test(start_paused = true)]
    async fn does_not_record_a_blip_for_a_span_shorter_than_the_threshold() {
        let _g = ActivityTestGuard::new();
        track_activity(ActivityKind::Danger, async {}).await;
        assert_eq!(activity_start_totals().danger, 0);
    }

    #[tokio::test(start_paused = true)]
    async fn records_a_blip_once_a_span_outlives_the_threshold() {
        let _g = ActivityTestGuard::new();
        track_activity(ActivityKind::Summary, async {
            tokio::time::sleep(Duration::from_millis(BLIP_THRESHOLD_MS)).await;
        })
        .await;
        assert_eq!(activity_start_totals().summary, 1);
        // The blip does not move the live count.
        assert_eq!(activity_counts().summary, 0);
    }

    /// The threshold is `>=`, so one millisecond short does NOT blip.
    #[tokio::test(start_paused = true)]
    async fn one_millisecond_short_of_the_threshold_does_not_blip() {
        let _g = ActivityTestGuard::new();
        track_activity(ActivityKind::Summary, async {
            tokio::time::sleep(Duration::from_millis(BLIP_THRESHOLD_MS - 1)).await;
        })
        .await;
        assert_eq!(activity_start_totals().summary, 0);
    }

    /// A collapsed same-kind span records no blip of its own — the enclosing
    /// span is the one that reports.
    #[tokio::test(start_paused = true)]
    async fn a_collapsed_span_records_no_blip_of_its_own() {
        let _g = ActivityTestGuard::new();
        run_attributed_to_job(Some(ActivityKind::Memory), async {
            track_activity(ActivityKind::Memory, async {
                tokio::time::sleep(Duration::from_millis(BLIP_THRESHOLD_MS * 4)).await;
            })
            .await;
        })
        .await;
        assert_eq!(activity_start_totals().memory, 0);
    }
}
