//! Stall watchdog for provider streams (v4 `lib/llm/stream-watchdog.ts`, bug
//! 141).
//!
//! A provider that accepts a streaming request, answers with headers, and then
//! sends no body is indistinguishable — from the consumer's side — from one that
//! is thinking hard. The `while let Some(item) = rx.recv().await` simply never
//! advances. Nothing below us catches it: an SDK-backed provider's own `timeout`
//! stops at the response headers (see [`crate::model::transport`]'s module
//! header and `StreamParams::request_timeout_ms`), which is what makes it safe
//! to apply on a streaming path and useless once the headers have landed.
//!
//! So the budget has to live where the chunks are counted. This wraps a
//! provider's chunk stream and gives each `recv()` a deadline: a generous one
//! for the first chunk, a tighter one between chunks, since a stream that has
//! started is already past the slow part.
//!
//! As with the transport's own bound, the abandoned request is NOT cancelled —
//! the provider owns its client and its socket. We stop *waiting*, which is the
//! part that holds a turn (or a whole chat creation) open.
//!
//! ## What v5 does instead of v4's `iterator.return()` — a NO-COUNTERPART
//!
//! v4's `withStallWatchdog` has a `finally` that asks the source generator to
//! unwind: awaited on an early consumer `break` (Stop, a tool loop cutting the
//! turn short), deliberately NOT awaited on a stall (the generator is suspended
//! at an `await` that never settles, so awaiting its `return()` would reproduce
//! the hang the wrapper exists to end). v5's seam is an
//! [`mpsc::Receiver`](tokio::sync::mpsc::Receiver), not a generator, and it has
//! no `return()`: **dropping the receiver is the one answer for both legs** —
//! the producing task's next `send` fails and it unwinds. That is the whole of
//! v5's "abandoned, not cancelled", and it is why the two legs that differ in v4
//! are one leg here.
//!
//! **Deferred, loudly:** real cancellation of the stalled socket. v4 names it
//! its own "not done here" (an `AbortSignal` on `LLMParams` is a plugin-types
//! contract change), and v5's P4.44 abort-arming deferral STANDS unchanged: no
//! `stream_message` call is armed with an abort today, and this module does not
//! arm one.
//!
//! ## The operator's stop (P4.D335, v4 `2b5e8ee24`'s `abortSignal`)
//!
//! [`watch_stream_with_stop`] races each `recv` against the Salon stream's
//! [`StopSignal`] (`services::turn_control`). When it fires the stream simply
//! ends — no error, so no fallback or recovery reads it as a provider failure —
//! and the caller keeps whatever it has accumulated. The stall budget wraps
//! the race (v4's `Promise.race([withTimeout(next), stopRequested])`): a stop
//! that lands first is a quiet end, a stop that lands after the stall fired is
//! a stall. The receiver is dropped exactly as on a stall (v4 `void`s
//! `iterator.return()` on a stop too — the source may be parked mid-await);
//! there is NO socket abort on a stop either, so the provider's bytes keep
//! flowing until the pump's next send fails (`streaming_provider.rs`).
//!
//! v4's funnel carries the stop at ONE site (`streamMessage`'s
//! `abortSignal: currentTurnStopSignal()`); v5 has no funnel, so the EIGHT
//! Salon-side wrap sites call [`watch_stream_with_stop`] and the four others
//! (greeting, help chat, the standalone Brahma console, swipe — v4's
//! `currentTurnStopSignal()` is `undefined` there) keep [`watch_stream`]. The
//! one-shot loop is a stop site since P4.165 (the human's ruling H-1,
//! overturning P4.D335's R-4): a Brahma consult INSIDE a Salon turn inherits
//! the turn's `AsyncLocalStorage` store in v4, so its stream ends at the stop;
//! the Scenario Builder hands it `None`. A canned differential pre-pushes and
//! closes its channel, so only `stream_watchdog_wrap_census` can see which
//! sites carry the stop.
//!
//! ## Where the budget starts — one sentence v4 does not need
//!
//! v4's `provider.streamMessage(...)` is a LAZY generator, so the SDK's
//! connect + headers happen inside the first `next()` and the 240 s first-chunk
//! budget covers them too. v5's `stream_message` is an `async fn` that awaits
//! the transport's `execute_stream` — bounded by the transport's own
//! time-to-headers budget ([`crate::model::transport`], P4.D42) — BEFORE it
//! hands back the receiver this module wraps, so the two bounds are SEQUENTIAL
//! here: the worst case with no first token is the transport budget plus the
//! first-chunk budget, where v4's is the first-chunk budget alone. Nothing is
//! unbounded either way; the window is merely wider, and recorded.
//!
//! ## The time driver is a load-bearing invariant
//!
//! [`tokio::time::timeout`] needs a runtime with a TIME DRIVER at the await.
//! Every production runtime in this tree is built with `enable_all()`; a test
//! venue that polls a stream consumer under a runtime WITHOUT one panics `there
//! is no reactor running`. The fix is that venue's runtime — never a watchdog
//! that is optional or skipped.

use std::fmt;

use tokio::sync::mpsc::Receiver;
use tokio::time::{Duration, Instant};

use super::stream::{StreamChunkResult, StreamError};
use crate::services::turn_control::StopSignal;

/// Budget for the first chunk of a stream — the model's whole time-to-first-
/// token, which on a long context with extended thinking is legitimately
/// minutes. Deliberately generous: a false positive here aborts a turn that was
/// working. (v4 `DEFAULT_FIRST_CHUNK_TIMEOUT_MS`.)
pub const DEFAULT_FIRST_CHUNK_TIMEOUT_MS: u64 = 240_000;

/// Budget between chunks. Once a stream is flowing the gaps are small — a model
/// still in its thinking phase is emitting reasoning deltas, which count as
/// chunks like any other — so this is much tighter than the first-chunk budget.
/// (v4 `DEFAULT_IDLE_CHUNK_TIMEOUT_MS`.)
pub const DEFAULT_IDLE_CHUNK_TIMEOUT_MS: u64 = 120_000;

/// The two per-gap budgets (v4's `StallWatchdogOptions.firstChunkTimeoutMs` /
/// `idleTimeoutMs`, which default to the two constants above).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StallBudgets {
    pub first_chunk_ms: u64,
    pub idle_ms: u64,
}

impl Default for StallBudgets {
    fn default() -> Self {
        Self {
            first_chunk_ms: DEFAULT_FIRST_CHUNK_TIMEOUT_MS,
            idle_ms: DEFAULT_IDLE_CHUNK_TIMEOUT_MS,
        }
    }
}

/// v4's `StallWatchdogOptions.provider` / `.modelName` / `.logContext`,
/// flattened — v5 has no untyped bag, and the four ids are the only keys any v4
/// caller puts in that bag.
#[derive(Clone, Copy, Debug)]
pub struct StallWatchdogContext<'a> {
    pub provider: &'a str,
    pub model_name: &'a str,
    /// v4 `logContext.context`: `"streaming.service"` at every Salon-side site
    /// (v4's one funnel), `"initial-greeting"` at the greeting.
    pub context: &'static str,
    /// The four ids v4's callers put in `logContext`. Each is rendered on the
    /// warn only when `Some` — v4's JSON drops an `undefined` key, and several
    /// callers genuinely pass none (recovery passes all four; the tool loops'
    /// re-stream and the Brahma/Carina services pass a subset).
    pub user_id: Option<&'a str>,
    pub chat_id: Option<&'a str>,
    pub character_id: Option<&'a str>,
    pub message_id: Option<&'a str>,
}

impl<'a> StallWatchdogContext<'a> {
    /// The Salon-side shape: v4's one funnel, `context: "streaming.service"`.
    pub fn streaming_service(provider: &'a str, model_name: &'a str) -> Self {
        Self {
            provider,
            model_name,
            context: "streaming.service",
            user_id: None,
            chat_id: None,
            character_id: None,
            message_id: None,
        }
    }

    pub fn with_ids(
        mut self,
        user_id: Option<&'a str>,
        chat_id: Option<&'a str>,
        character_id: Option<&'a str>,
        message_id: Option<&'a str>,
    ) -> Self {
        self.user_id = user_id;
        self.chat_id = chat_id;
        self.character_id = character_id;
        self.message_id = message_id;
        self
    }
}

/// A provider chunk stream with a per-gap deadline on every `recv`.
///
/// Built by [`watch_stream`]; consumed exactly like the
/// [`Receiver`](tokio::sync::mpsc::Receiver) it wraps, so every consumer loop
/// (`while let Some(item) = rx.recv().await { … }`) is unchanged.
pub struct WatchedStream<'a> {
    /// `None` once the watchdog has fired: the stalled receiver is DROPPED there
    /// (v5's whole "abandoned, not cancelled"), and every later `recv` answers
    /// `None`.
    rx: Option<Receiver<StreamChunkResult>>,
    budgets: StallBudgets,
    ctx: StallWatchdogContext<'a>,
    chunks_received: u64,
    started_at: Instant,
    /// One stall, one `Err`: the stalled error is answered exactly once and the
    /// stream is over. Kept for the `Debug` rendering — the live state the
    /// `recv` path consults is `rx.is_none()`.
    stalled: bool,
    /// The operator's stop for the Salon stream this call belongs to (P4.D335);
    /// `None` off the Salon spine.
    stop: Option<&'a StopSignal>,
    /// The stop fired and the stream ended quietly.
    stopped_by_operator: bool,
}

impl fmt::Debug for WatchedStream<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WatchedStream")
            .field("budgets", &self.budgets)
            .field("ctx", &self.ctx)
            .field("chunks_received", &self.chunks_received)
            .field("stalled", &self.stalled)
            .field("stopped_by_operator", &self.stopped_by_operator)
            .field("open", &self.rx.is_some())
            .finish()
    }
}

/// Wrap a provider chunk stream so a silent provider fails instead of hanging
/// (v4 `withStallWatchdog`).
///
/// Yields the source's items untouched; answers one
/// [`StreamError::stalled`] the moment a chunk is overdue.
pub fn watch_stream(
    rx: Receiver<StreamChunkResult>,
    budgets: StallBudgets,
    ctx: StallWatchdogContext<'_>,
) -> WatchedStream<'_> {
    watch_stream_with_stop(rx, budgets, ctx, None)
}

/// [`watch_stream`] that also ends quietly, mid-stream, when the operator's
/// `stop` fires (v4 `withStallWatchdog`'s `abortSignal`). The seven Salon-side
/// sites call this; `stop` is `None` wherever no turn is registered.
pub fn watch_stream_with_stop<'a>(
    rx: Receiver<StreamChunkResult>,
    budgets: StallBudgets,
    ctx: StallWatchdogContext<'a>,
    stop: Option<&'a StopSignal>,
) -> WatchedStream<'a> {
    WatchedStream {
        rx: Some(rx),
        budgets,
        ctx,
        chunks_received: 0,
        started_at: Instant::now(),
        stalled: false,
        stop,
        stopped_by_operator: false,
    }
}

impl WatchedStream<'_> {
    /// How many items the source has produced so far (v4 `chunksReceived`).
    pub fn chunks_received(&self) -> u64 {
        self.chunks_received
    }

    /// Did the operator's stop end this stream?
    pub fn stopped_by_operator(&self) -> bool {
        self.stopped_by_operator
    }

    /// The same signature as [`Receiver::recv`](tokio::sync::mpsc::Receiver::recv),
    /// so every consumer loop is unchanged.
    ///
    /// The budget for THIS call is `first_chunk_ms` while nothing has arrived,
    /// else `idle_ms` — per gap, never cumulative, so a long generation is never
    /// cut off for being long (v4: `chunksReceived === 0 ? first : idle`).
    pub async fn recv(&mut self) -> Option<StreamChunkResult> {
        let rx = self.rx.as_mut()?;
        let budget_ms = if self.chunks_received == 0 {
            self.budgets.first_chunk_ms
        } else {
            self.budgets.idle_ms
        };

        let stop = self.stop;
        // The stop race lives INSIDE the budget (v4 `Promise.race([withTimeout(
        // next), stopRequested])`): the stall timer keeps running underneath.
        let next = async move {
            match stop {
                Some(stop) => {
                    // The STOP is polled first: measured at the pin (P4.D335,
                    // `stop_vs_buffered_chunk`), v4's race answers `STOPPED` even
                    // when the source already holds its next chunk — an aborted
                    // signal's settled promise beats `withTimeout(next())`.
                    let stopped = std::pin::pin!(stop.wait());
                    let recv = std::pin::pin!(rx.recv());
                    match futures_util::future::select(stopped, recv).await {
                        futures_util::future::Either::Left(((), _)) => Err(StoppedByOperator),
                        futures_util::future::Either::Right((item, _)) => Ok(item),
                    }
                }
                None => Ok(rx.recv().await),
            }
        };
        match tokio::time::timeout(Duration::from_millis(budget_ms), next).await {
            Ok(Err(StoppedByOperator)) => {
                self.stopped_by_operator = true;
                let elapsed_ms = self.started_at.elapsed().as_millis() as u64;
                // v4's bag: `logContext` spread first, then provider, modelName,
                // chunksReceived, elapsedMs — no `budgetMs` on a stop.
                tracing::info!(
                    target: "quilltap::llm_stream",
                    context = %self.ctx.context,
                    user_id = self.ctx.user_id,
                    chat_id = self.ctx.chat_id,
                    character_id = self.ctx.character_id,
                    message_id = self.ctx.message_id,
                    provider = %self.ctx.provider,
                    model_name = %self.ctx.model_name,
                    chunks_received = self.chunks_received,
                    elapsed_ms = elapsed_ms,
                    "[LLMStream] Abandoned a provider stream at the operator's stop"
                );
                // Abandoned, not cancelled — the same receiver drop as a stall.
                self.rx = None;
                None
            }
            // The source closed, or produced an item. An `Err` from the source
            // passes through AS ITSELF — v4: "a real provider error arrives as
            // itself, not the class".
            Ok(Ok(None)) => None,
            Ok(Ok(Some(item))) => {
                // v4 counts `result.value`s: EVERY yielded chunk, reasoning-only
                // and usage-only and `done` included — and NOT a thrown error,
                // which never reaches `chunksReceived++`. v5's seam carries a
                // provider error as a channel ITEM, so the `Err` is what a v4
                // throw is: passed through, never counted (the `ffb6b3119`
                // round's §3 review).
                if item.is_ok() {
                    self.chunks_received += 1;
                }
                Some(item)
            }
            Err(_elapsed) => {
                self.stalled = true;
                let elapsed_ms = self.started_at.elapsed().as_millis() as u64;
                // v4's one line, with `logContext` spread first and the
                // `undefined` ids dropped by JSON.
                tracing::warn!(
                    target: "quilltap::llm_stream",
                    context = %self.ctx.context,
                    user_id = self.ctx.user_id,
                    chat_id = self.ctx.chat_id,
                    character_id = self.ctx.character_id,
                    message_id = self.ctx.message_id,
                    provider = %self.ctx.provider,
                    model_name = %self.ctx.model_name,
                    budget_ms = budget_ms,
                    chunks_received = self.chunks_received,
                    elapsed_ms = elapsed_ms,
                    "[LLMStream] Abandoned a stalled provider stream"
                );
                // Abandoned, not cancelled: dropping the receiver is what unwinds
                // the producing task (its next `send` fails). Nothing aborts the
                // socket — see the module doc's deferral.
                self.rx = None;
                Some(Err(StreamError::stalled(
                    budget_ms,
                    self.chunks_received,
                    Some(self.ctx.provider),
                    Some(self.ctx.model_name),
                )))
            }
        }
    }
}

/// The operator's stop won a `recv`'s race (the `Err` of its outcome; the
/// `Ok` is the source's next item, or its close).
struct StoppedByOperator;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::stream::{StreamChunk, StreamErrorKind};
    use crate::services::turn_control::{self, TurnControlRegistration};
    use crate::test_support::CaptureLayer;
    use std::sync::{Arc, Mutex};
    use tokio::sync::mpsc;
    use tracing_subscriber::layer::SubscriberExt;

    const BUDGETS: StallBudgets = StallBudgets {
        first_chunk_ms: 60,
        idle_ms: 40,
    };

    fn ctx<'a>() -> StallWatchdogContext<'a> {
        StallWatchdogContext {
            provider: "DEEPSEEK",
            model_name: "deepseek-v4-flash",
            context: "streaming.service",
            user_id: Some("u1"),
            chat_id: Some("c1"),
            character_id: None,
            message_id: Some("m1"),
        }
    }

    /// v4's `paced(values, gapMs)`: a stream that yields `values`, pausing
    /// `gap_ms` before each one. A real task is needed to pace, which is why
    /// this spawns — in the TEST only; the module itself never spawns.
    fn paced(values: Vec<&'static str>, gap_ms: u64) -> mpsc::Receiver<StreamChunkResult> {
        let (tx, rx) = mpsc::channel(1);
        tokio::spawn(async move {
            for v in values {
                tokio::time::sleep(Duration::from_millis(gap_ms)).await;
                if tx.send(Ok(StreamChunk::content(v))).await.is_err() {
                    return;
                }
            }
        });
        rx
    }

    /// v4's `silentAfter(values)`: yields what it has and then never speaks
    /// again. The `Sender` is HELD (returned, not dropped) — a dropped sender
    /// would close the channel, which is the one thing a stalled socket does
    /// not do.
    fn silent_after(
        values: Vec<&'static str>,
    ) -> (
        mpsc::Receiver<StreamChunkResult>,
        mpsc::Sender<StreamChunkResult>,
    ) {
        let (tx, rx) = mpsc::channel(values.len().max(1));
        for v in &values {
            tx.try_send(Ok(StreamChunk::content(*v))).unwrap();
        }
        (rx, tx)
    }

    async fn collect(mut s: WatchedStream<'_>) -> (Vec<String>, Option<StreamError>) {
        let mut out = Vec::new();
        while let Some(item) = s.recv().await {
            match item {
                Ok(c) => out.push(c.content),
                Err(e) => return (out, Some(e)),
            }
        }
        (out, None)
    }

    #[tokio::test(start_paused = true)]
    async fn passes_a_healthy_stream_through_untouched() {
        let (seen, err) =
            collect(watch_stream(paced(vec!["a", "b", "c"], 5), BUDGETS, ctx())).await;
        assert_eq!(seen, vec!["a", "b", "c"]);
        assert!(err.is_none(), "{err:?}");
    }

    #[tokio::test(start_paused = true)]
    async fn reports_the_first_chunk_case_distinctly() {
        let (rx, _held) = silent_after(vec![]);
        let (seen, err) = collect(watch_stream(rx, BUDGETS, ctx())).await;
        assert!(seen.is_empty());
        let err = err.expect("the first chunk never arrived");
        assert!(err.is_stalled(), "{err:?}");
        assert_eq!(err.v4_name(), "LLMStreamStalledError");
        assert_eq!(
            err.kind,
            StreamErrorKind::Stalled {
                budget_ms: 60,
                chunks_received: 0,
                provider: Some("DEEPSEEK".into()),
                model_name: Some("deepseek-v4-flash".into()),
            }
        );
        assert_eq!(
            err.message,
            "Provider stream never sent a first chunk within 60ms"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn yields_what_arrived_then_stalls_mid_flight() {
        let (rx, _held) = silent_after(vec!["a", "b"]);
        let (seen, err) = collect(watch_stream(rx, BUDGETS, ctx())).await;
        assert_eq!(seen, vec!["a", "b"]);
        let err = err.expect("the stream went quiet");
        assert!(err.is_stalled(), "{err:?}");
        assert_eq!(
            err.kind,
            StreamErrorKind::Stalled {
                budget_ms: 40,
                chunks_received: 2,
                provider: Some("DEEPSEEK".into()),
                model_name: Some("deepseek-v4-flash".into()),
            }
        );
        assert_eq!(
            err.message,
            "Provider stream went quiet for 40ms after 2 chunk(s)"
        );
    }

    /// Six gaps of 25 ms each: every gap is inside the 40 ms idle budget, and
    /// the total (150 ms) is well past it. The budget is per-gap, not cumulative
    /// — a long generation must not be penalised for being long.
    #[tokio::test(start_paused = true)]
    async fn does_not_abort_a_slow_stream_that_keeps_making_progress() {
        let (seen, err) = collect(watch_stream(
            paced(vec!["a", "b", "c", "d", "e", "f"], 25),
            BUDGETS,
            ctx(),
        ))
        .await;
        assert_eq!(seen, vec!["a", "b", "c", "d", "e", "f"]);
        assert!(err.is_none(), "{err:?}");
    }

    /// v4's "carries the provider and model onto the error" — here with the
    /// wrapper given a DIFFERENT pair, so the assertion cannot pass by accident
    /// off the shared `ctx()`.
    #[tokio::test(start_paused = true)]
    async fn carries_the_provider_and_model_onto_the_error() {
        let (rx, _held) = silent_after(vec![]);
        let c = StallWatchdogContext {
            provider: "OPENROUTER",
            model_name: "z-ai/glm-5.3",
            ..ctx()
        };
        let (_, err) = collect(watch_stream(rx, BUDGETS, c)).await;
        let StreamErrorKind::Stalled {
            provider,
            model_name,
            ..
        } = err.expect("stalled").kind
        else {
            panic!("not a stall")
        };
        assert_eq!(provider.as_deref(), Some("OPENROUTER"));
        assert_eq!(model_name.as_deref(), Some("z-ai/glm-5.3"));
    }

    /// v4: "lets a real provider error through as itself". The source's `Err`
    /// keeps its own message AND its `Provider` kind — a stall-shaped message
    /// from a decoder is still not a stall.
    #[tokio::test(start_paused = true)]
    async fn lets_a_real_provider_error_through_as_itself() {
        let (tx, rx) = mpsc::channel(2);
        tx.try_send(Ok(StreamChunk::content("a"))).unwrap();
        tx.try_send(Err(StreamError::new("429 rate limit exceeded")))
            .unwrap();
        drop(tx);
        let (seen, err) = collect(watch_stream(rx, BUDGETS, ctx())).await;
        assert_eq!(seen, vec!["a"]);
        let err = err.expect("the provider said no");
        assert_eq!(err.message, "429 rate limit exceeded");
        assert!(!err.is_stalled(), "{err:?}");
        assert_eq!(err.kind, StreamErrorKind::Provider);
        assert_eq!(err.v4_name(), "Error");
    }

    /// v5's counterpart to v4's "the consumer breaks early closes the source":
    /// dropping the watched stream drops the receiver, which is what unwinds a
    /// producing task (see the module doc's NO-COUNTERPART).
    #[tokio::test(start_paused = true)]
    async fn dropping_the_watched_stream_closes_the_source() {
        let (tx, rx) = mpsc::channel::<StreamChunkResult>(1);
        let mut s = watch_stream(rx, BUDGETS, ctx());
        assert!(tx.try_send(Ok(StreamChunk::content("a"))).is_ok());
        assert!(matches!(s.recv().await, Some(Ok(_))));
        drop(s);
        assert!(
            tx.send(Ok(StreamChunk::content("b"))).await.is_err(),
            "the source's next send must fail once the consumer is gone"
        );
    }

    /// One stall, one `Err`: after the watchdog fires, every later `recv`
    /// answers `None` (v5's "asked to unwind" leg — v4's generator is finished
    /// after its throw for the same reason).
    #[tokio::test(start_paused = true)]
    async fn after_a_stall_every_further_recv_is_none() {
        let (rx, _held) = silent_after(vec![]);
        let mut s = watch_stream(rx, BUDGETS, ctx());
        assert!(matches!(s.recv().await, Some(Err(ref e)) if e.is_stalled()));
        assert!(s.recv().await.is_none());
        assert!(s.recv().await.is_none());
    }

    /// A provider `Err` is NOT a chunk: v4's `chunksReceived++` sits after a
    /// successful `next()`, and a throw never reaches it. So a source that
    /// errors and then stays open is still on the FIRST-chunk budget, and the
    /// stall it eventually reports says `never sent a first chunk`.
    #[tokio::test(start_paused = true)]
    async fn a_provider_error_is_not_counted_as_a_chunk() {
        let (tx, rx) = mpsc::channel(1);
        tx.try_send(Err(StreamError::new("429 rate limit exceeded")))
            .unwrap();
        let mut s = watch_stream(rx, BUDGETS, ctx());
        assert!(matches!(s.recv().await, Some(Err(ref e)) if !e.is_stalled()));
        assert_eq!(s.chunks_received(), 0);
        let err = match s.recv().await {
            Some(Err(e)) => e,
            other => panic!("expected a stall, got {other:?}"),
        };
        assert_eq!(
            err.message,
            "Provider stream never sent a first chunk within 60ms"
        );
        drop(tx);
    }

    /// A reasoning-only chunk COUNTS — v4 counts `result.value`s, and a model
    /// still in its thinking phase is emitting reasoning deltas, which is
    /// exactly why the idle budget is the tighter one.
    #[tokio::test(start_paused = true)]
    async fn a_reasoning_only_chunk_counts() {
        let (tx, rx) = mpsc::channel(1);
        tx.try_send(Ok(StreamChunk {
            reasoning_content: Some("thinking…".into()),
            ..Default::default()
        }))
        .unwrap();
        let mut s = watch_stream(rx, BUDGETS, ctx());
        assert!(matches!(s.recv().await, Some(Ok(_))));
        let err = match s.recv().await {
            Some(Err(e)) => e,
            other => panic!("expected a stall, got {other:?}"),
        };
        assert_eq!(
            err.message,
            "Provider stream went quiet for 40ms after 1 chunk(s)"
        );
        drop(tx);
    }

    fn capture() -> (
        Arc<Mutex<Vec<String>>>,
        impl tracing::Subscriber + Send + Sync,
    ) {
        let logs = Arc::new(Mutex::new(Vec::<String>::new()));
        let subscriber = tracing_subscriber::registry().with(CaptureLayer(logs.clone()));
        (logs, subscriber)
    }

    /// v4's one `logger.warn('[LLMStream] Abandoned a stalled provider stream',
    /// {...logContext, provider, modelName, budgetMs, chunksReceived,
    /// elapsedMs})`, with the `undefined` ids dropped.
    #[tokio::test(start_paused = true)]
    async fn the_warn_fires_once_with_v4s_bag() {
        let (logs, subscriber) = capture();
        {
            let _g = tracing::subscriber::set_default(subscriber);
            let (rx, _held) = silent_after(vec!["a"]);
            let _ = collect(watch_stream(rx, BUDGETS, ctx())).await;
        }
        let lines = logs.lock().unwrap().clone();
        let hits: Vec<_> = lines
            .iter()
            .filter(|l| l.contains("[LLMStream] Abandoned a stalled provider stream"))
            .collect();
        assert_eq!(hits.len(), 1, "exactly one warn per stall: {lines:?}");
        let hit = hits[0];
        assert!(hit.starts_with("WARN quilltap::llm_stream"), "{hit}");
        for f in [
            "context=streaming.service",
            "user_id=u1",
            "chat_id=c1",
            "message_id=m1",
            "provider=DEEPSEEK",
            "model_name=deepseek-v4-flash",
            "budget_ms=40",
            "chunks_received=1",
            // Under the paused clock the timer advances exactly the budget, so
            // the wall-clock elapsed IS the budget here; in production it is
            // `>= budget_ms`, which is v4's `Date.now() - startedAt`.
            "elapsed_ms=40",
        ] {
            assert!(hit.contains(f), "missing {f} in {hit}");
        }
        // v4's JSON drops an `undefined` key — `character_id` is None here.
        assert!(!hit.contains("character_id="), "{hit}");
    }

    #[tokio::test(start_paused = true)]
    async fn the_warn_is_absent_on_a_healthy_stream_and_on_a_provider_error() {
        let (logs, subscriber) = capture();
        {
            let _g = tracing::subscriber::set_default(subscriber);
            let _ = collect(watch_stream(paced(vec!["a", "b"], 5), BUDGETS, ctx())).await;
            let (tx, rx) = mpsc::channel(1);
            tx.try_send(Err(StreamError::new("429 rate limit exceeded")))
                .unwrap();
            drop(tx);
            let _ = collect(watch_stream(rx, BUDGETS, ctx())).await;
        }
        let lines = logs.lock().unwrap().clone();
        assert!(
            !lines
                .iter()
                .any(|l| l.contains("[LLMStream] Abandoned a stalled provider stream")),
            "{lines:?}"
        );
    }

    // --- P4.D335: the operator's stop (v4 `stream-watchdog.test.ts:139-186`) ---

    const SLOW: StallBudgets = StallBudgets {
        first_chunk_ms: 5000,
        idle_ms: 5000,
    };

    fn stop_signal(test: &str) -> (TurnControlRegistration, StopSignal) {
        let reg = turn_control::register_active_turn(&format!(
            "watchdog-{test}-{}",
            uuid::Uuid::new_v4()
        ));
        let stop = reg.handle().stop_signal();
        (reg, stop)
    }

    async fn stop(reg: &TurnControlRegistration) {
        // A zero settle budget: the registration is still held by the test.
        let _ = turn_control::stop_active_turns(reg.chat_id(), Some(Duration::ZERO)).await;
    }

    #[tokio::test(start_paused = true)]
    async fn ends_quietly_keeping_what_arrived_when_the_stop_lands_mid_stream() {
        let (reg, sig) = stop_signal("mid");
        let (rx, _held) = silent_after(vec!["a", "b"]);
        let mut s = watch_stream_with_stop(rx, SLOW, ctx(), Some(&sig));
        assert!(matches!(s.recv().await, Some(Ok(_))));
        assert!(matches!(s.recv().await, Some(Ok(_))));
        let third = {
            let waiting = s.recv();
            tokio::pin!(waiting);
            tokio::select! {
                biased;
                _ = &mut waiting => panic!("no third chunk"),
                _ = tokio::time::sleep(Duration::from_millis(10)) => {}
            }
            stop(&reg).await;
            waiting.await
        };
        assert!(third.is_none(), "a stop is a quiet end, not an error");
        assert!(s.stopped_by_operator());
        assert_eq!(s.chunks_received(), 2);
        assert!(s.recv().await.is_none());
    }

    #[tokio::test(start_paused = true)]
    async fn abandons_a_provider_still_waiting_on_its_first_chunk() {
        let (reg, sig) = stop_signal("first");
        let (rx, _held) = silent_after(vec![]);
        let started = Instant::now();
        let s = watch_stream_with_stop(rx, SLOW, ctx(), Some(&sig));
        let collecting = collect(s);
        tokio::pin!(collecting);
        tokio::select! {
            biased;
            _ = &mut collecting => panic!("nothing has stopped it yet"),
            _ = tokio::time::sleep(Duration::from_millis(15)) => {}
        }
        stop(&reg).await;
        let (seen, err) = collecting.await;
        assert!(seen.is_empty() && err.is_none());
        assert!(started.elapsed() < Duration::from_millis(1000));
    }

    #[tokio::test(start_paused = true)]
    async fn yields_nothing_when_already_stopped_before_it_starts() {
        let (reg, sig) = stop_signal("before");
        stop(&reg).await;
        let (seen, err) = collect(watch_stream_with_stop(
            paced(vec!["a"], 5),
            SLOW,
            ctx(),
            Some(&sig),
        ))
        .await;
        assert!(seen.is_empty() && err.is_none());
    }

    /// Measured at the pin (`stop_vs_buffered_chunk`): with the next chunk
    /// already buffered, v4's race still answers the stop — the chunk is
    /// abandoned, not delivered.
    #[tokio::test(start_paused = true)]
    async fn stop_vs_buffered_chunk() {
        let (reg, sig) = stop_signal("buffered");
        let (rx, _held) = silent_after(vec!["a", "b", "c"]);
        let mut s = watch_stream_with_stop(rx, SLOW, ctx(), Some(&sig));
        assert!(matches!(s.recv().await, Some(Ok(_))));
        stop(&reg).await;
        assert!(s.recv().await.is_none(), "the buffered 'b' is abandoned");
        assert_eq!(s.chunks_received(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn leaves_a_healthy_stream_alone_when_the_stop_never_fires() {
        let (_reg, sig) = stop_signal("healthy");
        let (seen, err) = collect(watch_stream_with_stop(
            paced(vec!["a", "b"], 5),
            BUDGETS,
            ctx(),
            Some(&sig),
        ))
        .await;
        assert_eq!(seen, vec!["a", "b"]);
        assert!(err.is_none());
    }

    /// T14: the budget wraps the race, so a stop that lands AFTER the stall
    /// timer fired is a stall, not a quiet end.
    #[tokio::test(start_paused = true)]
    async fn a_stop_after_the_stall_is_a_stall() {
        let (reg, sig) = stop_signal("after-stall");
        let (rx, _held) = silent_after(vec![]);
        let mut s = watch_stream_with_stop(rx, BUDGETS, ctx(), Some(&sig));
        assert!(matches!(s.recv().await, Some(Err(ref e)) if e.is_stalled()));
        stop(&reg).await;
        assert!(s.recv().await.is_none());
        assert!(!s.stopped_by_operator());
    }

    #[tokio::test(start_paused = true)]
    async fn the_stop_info_fires_once_with_v4s_bag() {
        let (logs, subscriber) = capture();
        {
            let _g = tracing::subscriber::set_default(subscriber);
            let (reg, sig) = stop_signal("info");
            let (rx, _held) = silent_after(vec!["a"]);
            let mut s = watch_stream_with_stop(rx, SLOW, ctx(), Some(&sig));
            assert!(matches!(s.recv().await, Some(Ok(_))));
            stop(&reg).await;
            assert!(s.recv().await.is_none());
            assert!(s.recv().await.is_none());
            // The silence leg: a stall-less, stop-less stream says nothing.
            let _ = collect(watch_stream_with_stop(
                paced(vec!["a"], 5),
                BUDGETS,
                ctx(),
                None,
            ))
            .await;
        }
        let lines = logs.lock().unwrap().clone();
        let hits: Vec<_> = lines
            .iter()
            .filter(|l| {
                l.contains("[LLMStream] Abandoned a provider stream at the operator's stop")
            })
            .collect();
        assert_eq!(hits.len(), 1, "exactly one info per stop: {lines:?}");
        let hit = hits[0];
        assert!(hit.starts_with("INFO quilltap::llm_stream"), "{hit}");
        for f in [
            "context=streaming.service",
            "user_id=u1",
            "chat_id=c1",
            "message_id=m1",
            "provider=DEEPSEEK",
            "model_name=deepseek-v4-flash",
            "chunks_received=1",
            "elapsed_ms=",
        ] {
            assert!(hit.contains(f), "missing {f} in {hit}");
        }
        assert!(!hit.contains("budget_ms="), "{hit}");
        assert!(!hit.contains("character_id="), "{hit}");
    }

    #[test]
    fn the_defaults_are_v4s_two_constants() {
        assert_eq!(DEFAULT_FIRST_CHUNK_TIMEOUT_MS, 240_000);
        assert_eq!(DEFAULT_IDLE_CHUNK_TIMEOUT_MS, 120_000);
        assert_eq!(
            StallBudgets::default(),
            StallBudgets {
                first_chunk_ms: 240_000,
                idle_ms: 120_000
            }
        );
    }
}
