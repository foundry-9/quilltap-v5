//! Shared test-only tracing-capture rig (P4.77 — consolidated from 19
//! independently copy-pasted instances across core/host/web/harness test
//! modules).
//!
//! A log-only fix (a v4 `logger.debug`/`warn`/`error` line with no
//! accompanying write) is invisible to every other proof this repo has: the
//! wire bytes, the parsed response, and every DB row are identical whether
//! or not the line fires — see the `differential-blind-to-a-log-only-fix`
//! memory note. The sanctioned proof is a capturing `tracing::Layer` over
//! the REAL function, asserting both that the line fires on its own branch
//! and that sibling branches stay silent. Nineteen call sites had each
//! written the same struct by hand before this module existed.
//!
//! Gated `#[cfg(any(test, feature = "test-support"))]` rather than plain
//! `#[cfg(test)]`: `cfg(test)` is true only while THIS crate compiles its own
//! tests, so it is invisible to `quilltap-host`, `quilltap-web`, and
//! `quilltap-harness` when they compile theirs. Those three enable the
//! `test-support` feature under `[dev-dependencies]` instead, which is how a
//! `#[cfg(test)]`-flavoured module reaches across a crate boundary at all.
//!
//! Two idioms live here, not one — they are genuinely different contracts,
//! not two copies of the same thing that happened to drift:
//!
//! - [`capture`] — the dominant shape (18 of the 19 sites): a fresh
//!   `Arc<Mutex<Vec<String>>>` per test, installed with
//!   `tracing::subscriber::set_default`, which is THREAD-scoped. Parallel
//!   tests cannot steal each other's subscriber; see the
//!   `a-process-global-test-seam-must-be-thread-scoped` memory note.
//! - [`global_capture`] — `job_runner.rs`'s one holdout: a process-global
//!   `set_global_default`, installed once, with a per-THREAD buffer behind a
//!   `thread_local!`. `tracing` caches each callsite's `Interest` globally on
//!   first use, so a thread-scoped subscriber can lose a callsite forever if
//!   a sibling test reaches it first with no subscriber armed on ITS thread —
//!   `job_runner`'s smoke test flaked 17 runs in 25 under
//!   `--test-threads=8` before P4.40 fixed it exactly this way. Do not
//!   "simplify" this back to [`capture`]'s idiom; the difference is load-bearing.

use std::sync::{Arc, Mutex};

/// Captures one tracing event's level, target, and fields as one line:
/// `"<LEVEL> <target> <field>=<value> … "<message debug>""`.
///
/// `record_str`/`record_u64`/`record_i64`/`record_f64`/`record_bool` all
/// format the value via `Display` (no quotes); `record_debug` formats via
/// `Debug`, with the `message` field's rendering carrying no leading `=`
/// (`tracing`'s own convention: the format-args message is the line's
/// narration, not a `key=value` pair). This is the exact union of what the
/// 18 sites this replaces implemented — a site that only ever logged `&str`
/// and debug-formatted fields still gets byte-identical output, because
/// `tracing::field::Visit`'s own default methods for the untyped variants
/// already delegate to `record_debug`, and `Debug`/`Display` render
/// identically for str/u64/i64/bool (only whole-number `f64` diverges,
/// `"1.0"` vs `"1"`, and no migrated call site emits an f64 field that used
/// to reach the default path).
pub struct FieldVisitor(pub String);

impl tracing::field::Visit for FieldVisitor {
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        self.0.push_str(&format!(" {}={}", field.name(), value));
    }
    fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
        self.0.push_str(&format!(" {}={}", field.name(), value));
    }
    fn record_i64(&mut self, field: &tracing::field::Field, value: i64) {
        self.0.push_str(&format!(" {}={}", field.name(), value));
    }
    fn record_f64(&mut self, field: &tracing::field::Field, value: f64) {
        self.0.push_str(&format!(" {}={}", field.name(), value));
    }
    fn record_bool(&mut self, field: &tracing::field::Field, value: bool) {
        self.0.push_str(&format!(" {}={}", field.name(), value));
    }
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.0.push_str(&format!(" {value:?}"));
        } else {
            self.0.push_str(&format!(" {}={value:?}", field.name()));
        }
    }
}

/// A `tracing_subscriber::Layer` that renders every event through
/// [`FieldVisitor`] and pushes the line into a shared `Vec`.
///
/// Install with `tracing::subscriber::set_default` — thread-scoped, so
/// parallel tests cannot steal each other's subscriber (see the module doc).
pub struct CaptureLayer(pub Arc<Mutex<Vec<String>>>);

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for CaptureLayer {
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        let meta = event.metadata();
        let mut visitor = FieldVisitor(format!("{} {}", meta.level(), meta.target()));
        event.record(&mut visitor);
        self.0.lock().unwrap().push(visitor.0);
    }
}

/// Run `f` with a capturing subscriber installed on this thread and hand
/// back every line it logged.
pub fn captured(f: impl FnOnce()) -> Vec<String> {
    use tracing_subscriber::layer::SubscriberExt;
    let logs = Arc::new(Mutex::new(Vec::<String>::new()));
    let subscriber = tracing_subscriber::registry().with(CaptureLayer(logs.clone()));
    {
        let _guard = tracing::subscriber::set_default(subscriber);
        f();
    }
    let out = logs.lock().unwrap().clone();
    out
}

/// As [`captured`], but returns `f`'s own return value alongside the lines —
/// the `cost_events.rs` / `maintenance.rs` / `message_context.rs` /
/// `scheduled_maintenance.rs` idiom, for a caller that needs both the
/// durable result AND what got logged along the way.
pub fn captured_with<T>(f: impl FnOnce() -> T) -> (T, Vec<String>) {
    use tracing_subscriber::layer::SubscriberExt;
    let logs = Arc::new(Mutex::new(Vec::<String>::new()));
    let subscriber = tracing_subscriber::registry().with(CaptureLayer(logs.clone()));
    let out = {
        let _guard = tracing::subscriber::set_default(subscriber);
        f()
    };
    let lines = logs.lock().unwrap().clone();
    (out, lines)
}

/// `job_runner.rs`'s holdout idiom: a process-global subscriber, armed once,
/// with a per-thread buffer — see the module doc for why this is a
/// genuinely different contract from [`captured`], not a copy that drifted.
pub mod global_capture {
    use std::cell::RefCell;

    thread_local! {
        /// `Some` only while this thread is inside [`capture_events`].
        static CAPTURED: RefCell<Option<Vec<String>>> = const { RefCell::new(None) };
    }

    /// The process-global capturing layer. Reads only from [`CAPTURED`], so an
    /// un-armed thread's events are silently dropped rather than colouring
    /// whichever test happens to be capturing at the time.
    struct GlobalCaptureLayer;

    struct MessageVisitor(String);
    impl tracing::field::Visit for MessageVisitor {
        fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
            if field.name() == "message" {
                self.0 = format!("{value:?}");
            }
        }
    }

    impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for GlobalCaptureLayer {
        fn on_event(
            &self,
            event: &tracing::Event<'_>,
            _ctx: tracing_subscriber::layer::Context<'_, S>,
        ) {
            CAPTURED.with(|cell| {
                let mut cell = cell.borrow_mut();
                let Some(buf) = cell.as_mut() else {
                    return; // this thread is not capturing
                };
                let mut visitor = MessageVisitor(String::new());
                event.record(&mut visitor);
                let meta = event.metadata();
                buf.push(format!("{} {} {}", meta.level(), meta.target(), visitor.0));
            });
        }
    }

    /// Install the global capturing subscriber exactly once per test binary.
    fn install_capture_subscriber() {
        static INIT: std::sync::Once = std::sync::Once::new();
        INIT.call_once(|| {
            use tracing_subscriber::layer::SubscriberExt;
            // Ignore an error: another test binary layout could conceivably have
            // set one already, and the capture below is a no-op if so — which
            // the assertions would catch loudly rather than silently.
            let _ = tracing::subscriber::set_global_default(
                tracing_subscriber::registry().with(GlobalCaptureLayer),
            );
        });
    }

    /// Arm this thread's capture buffer, run `f`, and return everything the
    /// callee narrated on this thread while it ran.
    pub async fn capture_events<F: std::future::Future<Output = ()>>(f: F) -> String {
        install_capture_subscriber();
        CAPTURED.with(|c| *c.borrow_mut() = Some(Vec::new()));
        f.await;
        let lines = CAPTURED
            .with(|c| c.borrow_mut().take())
            .expect("capture buffer armed");
        lines.join("\n")
    }
}
