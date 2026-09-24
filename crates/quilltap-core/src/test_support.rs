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
//!   It renders either the message alone or, since P4.112, every field the
//!   way [`captured`] does — the three auto-title harness families' rig
//!   folded in here, so there is ONE process-global capture rig, not two.

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

/// P4.D171: heal a fixture's main-partition connection with the two
/// `78b381a96`-round schema moves (`chat_messages.routeTrail`,
/// `chats.cycleOrderParticipantIds`) — the repaired-at-boot idiom, for the
/// many committed and hand-rolled `chats`/`chat_messages` test fixtures that
/// predate them. Idempotent; a no-op on a table-less partition or an
/// already-healed one. Call on a writable connection before any read that
/// names either column (every `chats_read`/`chats_messages_read` call).
pub fn ensure_p4d171_columns(conn: &rusqlite::Connection) {
    crate::db::chat_messages_route_trail_repair::ensure_chat_messages_route_trail_column(conn)
        .expect("ensure the route-trail column on a test fixture");
    crate::db::chats_cycle_order_repair::ensure_chats_cycle_order_column(conn)
        .expect("ensure the cycle-order column on a test fixture");
}

/// P4.D182: heal a fixture's main-partition connection with the two
/// `31436bae4`-round schema moves (`files.generationKey` + its index,
/// `chats.transcriptVersion`) — the same repaired-at-boot idiom as
/// [`ensure_p4d171_columns`], for the committed and hand-rolled `files` /
/// `chats` fixtures that predate them.
///
/// `files.generationKey` is the one that BITES: this port's `files` INSERT is
/// a fixed column list that now always binds it, where v4's insert names only
/// the keys its data object carries — so v4 writes happily to a pre-4.10
/// table and v5 answers `no such column`. That asymmetry is the whole reason
/// this helper exists; it is not a difference in what either engine stores.
///
/// Idempotent; a no-op on a table-less partition or an already-healed one.
pub fn ensure_p4d182_columns(conn: &rusqlite::Connection) {
    crate::db::files_generation_key_repair::ensure_files_generation_key_column_and_index(conn)
        .expect("ensure the generation-key column + index on a test fixture");
    crate::db::chats_transcript_version_repair::ensure_chats_transcript_version_column(conn)
        .expect("ensure the transcript-version column on a test fixture");
}

/// `job_runner.rs`'s holdout idiom: a process-global subscriber, armed once,
/// with a per-thread buffer — see the module doc for why this is a
/// genuinely different contract from [`captured`], not a copy that drifted.
///
/// Two renderings share the one subscriber, chosen per armed thread:
///
/// - [`capture_events`] — the MESSAGE only, `"<LEVEL> <target> <message
///   debug>"` (`job_runner`'s original contract);
/// - `global_capture::capture` / `capture_async` — every field, through [`FieldVisitor`],
///   byte-identical to [`captured`]'s lines. P4.112 folded the harness's
///   `auto_title_capture` rig in here: it was this module's design with
///   `CaptureLayer`'s rendering, built separately because these entry points
///   rendered the message alone and its pins assert FIELDS. A binary that
///   runs a differential test (no capture) in parallel with capture tests over
///   the SAME callsites needs this idiom, and every test in it calls
///   [`install`] first so no callsite is ever registered against the no-op
///   default (the `Interest` race in the module doc).
pub mod global_capture {
    use std::cell::RefCell;

    /// How an armed thread renders each event.
    #[derive(Clone, Copy)]
    enum Rendering {
        /// `"<LEVEL> <target> <message debug>"` — [`capture_events`].
        Message,
        /// [`super::FieldVisitor`]'s line — [`capture`] / [`capture_async`].
        Fields,
    }

    thread_local! {
        /// `Some` only while this thread is inside a capture.
        static CAPTURED: RefCell<Option<(Rendering, Vec<String>)>> = const { RefCell::new(None) };
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
                let Some((rendering, buf)) = cell.as_mut() else {
                    return; // this thread is not capturing
                };
                let meta = event.metadata();
                match rendering {
                    Rendering::Message => {
                        let mut visitor = MessageVisitor(String::new());
                        event.record(&mut visitor);
                        buf.push(format!("{} {} {}", meta.level(), meta.target(), visitor.0));
                    }
                    Rendering::Fields => {
                        let mut visitor =
                            super::FieldVisitor(format!("{} {}", meta.level(), meta.target()));
                        event.record(&mut visitor);
                        buf.push(visitor.0);
                    }
                }
            });
        }
    }

    /// Install the global capturing subscriber exactly once per test binary.
    /// Idempotent. A binary whose capture tests share callsites with an
    /// un-armed test calls this at the top of EVERY test, before any callsite
    /// is reached.
    pub fn install() {
        static INIT: std::sync::Once = std::sync::Once::new();
        INIT.call_once(|| {
            use tracing_subscriber::layer::SubscriberExt;
            // LOUD, never ignored: if another subscriber already owns this
            // binary, the capture is a no-op and every "no line logged"
            // (silence) assertion would pass having captured nothing — the
            // "a second global default silences the first" trap. The folded
            // `auto_title_capture` rig (P4.112 unit 1) had this `expect`; the
            // d1c06cd9d unification review restored it here.
            tracing::subscriber::set_global_default(
                tracing_subscriber::registry().with(GlobalCaptureLayer),
            )
            .expect("the global capture rig must own this binary's global subscriber");
        });
    }

    fn arm(rendering: Rendering) {
        install();
        CAPTURED.with(|c| *c.borrow_mut() = Some((rendering, Vec::new())));
    }

    fn disarm() -> Vec<String> {
        CAPTURED
            .with(|c| c.borrow_mut().take())
            .expect("capture buffer armed")
            .1
    }

    /// Arm this thread's capture buffer, run `f`, and return everything the
    /// callee narrated on this thread while it ran (the message only).
    pub async fn capture_events<F: std::future::Future<Output = ()>>(f: F) -> String {
        arm(Rendering::Message);
        f.await;
        disarm().join("\n")
    }

    /// Run `f` with this thread's buffer armed; return its value and every line
    /// it logged on this thread, rendered with every field (the
    /// [`super::captured`] line shape).
    pub fn capture<T>(f: impl FnOnce() -> T) -> (T, Vec<String>) {
        arm(Rendering::Fields);
        let out = f();
        (out, disarm())
    }

    /// [`capture`] for an async body on a current-thread runtime (every event
    /// the future emits is on this thread).
    pub async fn capture_async<T>(f: impl std::future::Future<Output = T>) -> (T, Vec<String>) {
        arm(Rendering::Fields);
        let out = f.await;
        (out, disarm())
    }
}

// === P4.115 ===
/// Open an existing encrypted database **read-only**, for a test that only
/// READS what the code under test wrote (P4.115 item 4 — the Scenario
/// Builder dispatch-wire test's `llm_logs` read had used
/// [`crate::db::Writer::open_writable`], whose open sequence WRITES:
/// `foreign_keys`, `journal_mode = TRUNCATE`).
///
/// The CLAUDE.md read path, mirroring the engine's own private read opener
/// (`db::runtime`'s `open_readonly`): `SQLITE_OPEN_READ_ONLY`, then
/// `PRAGMA key = "x'<hex>'"` as the first and ONLY pragma (the raw-hex form,
/// KDF skipped), then `qt_text()` registered as on every engine connection —
/// no `journal_mode`, no `foreign_keys`. Any write through the returned
/// connection fails with SQLite's read-only error.
pub fn open_readonly(path: &std::path::Path, pepper_b64: &str) -> rusqlite::Connection {
    use rusqlite::OpenFlags;
    let key_hex = crate::dbkey::pepper_b64_to_key_hex(pepper_b64).expect("a valid test pepper");
    let conn = rusqlite::Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY
            | OpenFlags::SQLITE_OPEN_NO_MUTEX
            | OpenFlags::SQLITE_OPEN_URI,
    )
    .unwrap_or_else(|e| panic!("read-only open of {}: {e}", path.display()));
    conn.pragma_update(None, "key", format!("x'{key_hex}'"))
        .expect("the cipher key, first and only pragma");
    crate::db::text_compression::register_qt_text(&conn).expect("register qt_text");
    conn
}
// === end P4.115 ===
