//! P4.D215 — the log-capture rig for the three auto-title families
//! (`title_update_tier3`, `chat_regenerate_title_tier3`,
//! `context_summary_service_tier3`).
//!
//! ## Why not `test_support::CaptureLayer` + `set_default`
//!
//! Each of these binaries runs its differential test (no subscriber) in
//! parallel with its capture tests, over the SAME callsites. `tracing` caches a
//! callsite's `Interest` globally, so a thread-scoped subscriber can lose a
//! callsite a sibling reached first with nothing armed — measured on this lane:
//! the regenerate family's capture test saw ZERO lines under the default test
//! threads and passed alone. (`test_support::global_capture` is the sanctioned
//! fix, but it records only the message; these pins assert FIELDS.)
//!
//! So this rig is `global_capture`'s shape with `CaptureLayer`'s rendering: ONE
//! process-global subscriber, installed before any test body touches a
//! callsite (every test in a binary using it calls [`install`] first), routing
//! each event into the CALLING thread's buffer — `"<LEVEL> <target> <field>=
//! <value> … "<message>""`, byte-identical to `CaptureLayer` because it uses the
//! same [`FieldVisitor`]. An un-armed thread's events are dropped.

use std::cell::RefCell;

use quilltap_core::test_support::FieldVisitor;

thread_local! {
    static CAPTURED: RefCell<Option<Vec<String>>> = const { RefCell::new(None) };
}

struct ThreadCaptureLayer;

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for ThreadCaptureLayer {
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        CAPTURED.with(|cell| {
            let mut cell = cell.borrow_mut();
            let Some(buf) = cell.as_mut() else {
                return;
            };
            let meta = event.metadata();
            let mut visitor = FieldVisitor(format!("{} {}", meta.level(), meta.target()));
            event.record(&mut visitor);
            buf.push(visitor.0);
        });
    }
}

/// Install the process-global capturing subscriber (once per binary). Call at
/// the top of EVERY test in a binary that captures, so no callsite is ever
/// registered against the no-op default.
pub fn install() {
    static INIT: std::sync::Once = std::sync::Once::new();
    INIT.call_once(|| {
        use tracing_subscriber::layer::SubscriberExt;
        tracing::subscriber::set_global_default(
            tracing_subscriber::registry().with(ThreadCaptureLayer),
        )
        .expect("the auto-title capture rig must own this binary's global subscriber");
    });
}

/// Run `f` with this thread's buffer armed; return its value and every line
/// it logged on this thread.
#[allow(dead_code)]
pub fn capture<T>(f: impl FnOnce() -> T) -> (T, Vec<String>) {
    install();
    CAPTURED.with(|c| *c.borrow_mut() = Some(Vec::new()));
    let out = f();
    let lines = CAPTURED
        .with(|c| c.borrow_mut().take())
        .expect("capture buffer armed");
    (out, lines)
}

/// [`capture`] for an async body on a current-thread runtime (every event the
/// future emits is on this thread).
#[allow(dead_code)]
pub async fn capture_async<T>(f: impl std::future::Future<Output = T>) -> (T, Vec<String>) {
    install();
    CAPTURED.with(|c| *c.borrow_mut() = Some(Vec::new()));
    let out = f.await;
    let lines = CAPTURED
        .with(|c| c.borrow_mut().take())
        .expect("capture buffer armed");
    (out, lines)
}
