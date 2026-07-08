//! The Carina markup runner (Phase-3 Unit-3 wave 3) — v4
//! `lib/services/carina/markup-runner.ts` (`runCarinaMarkupQuery`), the shared
//! `@Name:` / `@Name?` handling that both the user-message path (the
//! orchestrator) and the assistant-markup path (the message finalizer) drive.
//!
//! Each path scans a piece of text for Carina markup ([`crate::carina_parser`],
//! already ported), fires the isolated reference query, surfaces the answer live,
//! and routes a failure through Prospero. The only differences are caller-side:
//! the user-message path emits a "Consulting…" status first and splices a public
//! answer into the in-flight turn; the assistant-markup path does neither, and
//! the two word their log lines differently. Those variations are passed in as
//! callbacks and labels so the one implementation here stays the single source of
//! truth for markup handling.
//!
//! ## Scope: `runCarinaQuery` is an injected seam (documented reduction)
//!
//! The task scoped this unit as "the markup runner + the isolated query it
//! drives". A direct read of v4's `runCarinaQuery`
//! (`lib/services/carina/carina.service.ts`, ~370 lines) established that it
//! drags in **genuinely unported subsystems too large for this unit**, so per the
//! task's explicit STOP rule the scope was reduced to `runCarinaMarkupQuery` with
//! `runCarinaQuery` as an injected seam ([`RunCarinaQuery`]). The evidence, all
//! confirmed against the current tree:
//!
//!   - the **tool subsystem** — `buildTools`, `streamMessage`
//!     (`streaming.service`), `processToolCalls` / `detectToolCallsInResponse` /
//!     `createToolContext` (`tool-execution.service`) — is **wave 4**, explicitly
//!     scoped out in `docs/developer/porting/chat-orchestration.md`, and has no
//!     port (only [`crate::model::stream`], the seam, exists);
//!   - `findCharactersByName` (the character resolver) — not ported;
//!   - `searchMemoriesSemantic` — noted as deliberately omitted in `recall_tags`;
//!   - `buildCommonplaceLLMContext` (the Commonplace-Book / post-office writer) —
//!     not ported;
//!   - the Brahma Console (`brahma-console/one-shot.service`) — not ported (only
//!     the `'brahma'` chat-type predicate exists);
//!   - `enqueueCarinaMemoryExtraction` — not ported (`queue_service` carries only
//!     the housekeeping-enqueue slice).
//!
//! `buildIdentityStack` / `buildPublicIdentityCard` ARE ported
//! ([`crate::system_prompt`]), but the rest of the query engine is not. The whole
//! query is therefore the [`RunCarinaQuery`] seam; the differential drives v4's
//! REAL `runCarinaMarkupQuery` with `runCarinaQuery` jest-mocked to a recorder
//! that posts a real Carina message (through the REAL `postCarinaResponse`, so
//! `chat_messages` fills) and returns a fixed [`CarinaResult`], and the Rust side
//! injects a [`RunCarinaQuery`] impl that does the same over the ported
//! [`writer::post_carina_response`].
//!
//! ## The Prospero seam
//!
//! `postProsperoCarinaError` is the post-office subsystem (wave 4+). It is behind
//! the injected [`PostProsperoCarinaError`] seam (a recorder in the harness,
//! jest-mocked to a recorder on the oracle side). Tracked deferral.
//!
//! ## The never-throws catch-all
//!
//! v4 wraps the `runCarinaQuery` call + the Prospero post in a `try/catch` that
//! logs and swallows: a failed Carina side-effect must not sink the turn that
//! triggered it. In Rust the two seams return `Result`, and the runner logs +
//! swallows an `Err` from either, so `run_carina_markup_query` is infallible
//! (`-> ()`), matching v4's "never throws". The callback contract differs by
//! language: v4's `onConsulting` runs *before* the try (a throw there would
//! propagate), `onPublicAnswer` runs *inside* it (a throw is caught). Rust
//! closures/trait methods are infallible here (they return `()`), so that
//! JS-only throw asymmetry has no Rust analogue — the "callback throws" case is
//! not banked (documented deviation, nothing to diff).

pub mod writer;

use crate::carina_parser::parse_carina_query;
use crate::db::runtime::Db;

pub use writer::{
    build_carina_message, post_carina_response, PostCarinaResponseParams, PostedCarinaMessage,
};

/// The failure that prevented a Carina query from being answered — v4
/// `CarinaErrorKind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CarinaErrorKind {
    NotFound,
    NoProfile,
    LlmFailed,
}

impl CarinaErrorKind {
    /// The v4 string tag (`'not-found'` / `'no-profile'` / `'llm-failed'`), used
    /// verbatim in the Prospero error seam and the harness trace.
    pub fn as_str(self) -> &'static str {
        match self {
            CarinaErrorKind::NotFound => "not-found",
            CarinaErrorKind::NoProfile => "no-profile",
            CarinaErrorKind::LlmFailed => "llm-failed",
        }
    }
}

/// v4 `CarinaError`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CarinaError {
    pub kind: CarinaErrorKind,
    /// Short summary for the `llm-failed` case (network/rate-limit/etc.).
    pub detail: Option<String>,
    /// The resolved answerer's name, when known (`None` for `not-found`).
    pub character_name: Option<String>,
}

/// v4 `CarinaResult` — the success payload or the failure. The success arm
/// carries the posted message so a caller can splice it into the current turn.
#[derive(Debug, Clone)]
pub enum CarinaResult {
    Ok {
        /// The answerer's reply text.
        answer: String,
        /// The posted Carina message's id.
        message_id: String,
        /// The posted Carina message itself, so a caller can splice it into the
        /// current turn's in-memory context.
        message: PostedCarinaMessage,
        /// The resolved answerer character's id and name.
        answerer_id: String,
        answerer_name: String,
    },
    Err {
        error: CarinaError,
    },
}

/// The options handed to [`RunCarinaQuery::run`] — v4 `RunCarinaQueryOptions`
/// (minus `onPosted`, which the seam owns internally in this port; see
/// [`RunCarinaQuery`]).
#[derive(Debug, Clone)]
pub struct RunCarinaQueryOptions {
    pub user_id: String,
    pub chat_id: String,
    /// Answerer name as written in the markup.
    pub character_name: String,
    /// The question to answer.
    pub question: String,
    /// `?` separator → answer is whispered to the asker only.
    pub whisper: bool,
    /// Participant id of the asker — the whisper target. `None` when no
    /// participant context is available (answer goes public).
    pub asker_participant_id: Option<String>,
    /// The human operator initiated this query (the user-message path). The
    /// operator can always reach any character.
    pub operator_initiated: bool,
}

/// Log labels for the detection/failure lines — the two caller paths word them
/// differently (v4 `CarinaMarkupLogLabels`). Carried on the trace but not
/// otherwise consulted (host-side logging is out of the port's scope).
#[derive(Debug, Clone)]
pub struct CarinaMarkupLogLabels {
    /// Fills "Carina query detected in <detected>" (e.g. `"user message"`).
    pub detected: String,
    /// Fills "Carina <failed> query failed" (e.g. `"user-message"`).
    pub failed: String,
}

/// The isolated reference-query seam — v4's `runCarinaQuery`. In this port the
/// whole query engine (name resolution, profile/key resolution, context build,
/// the model call + tool loop, answer posting, memory-extraction enqueue) is
/// unported (see the module docs), so it is injected. The seam OWNS the
/// answer-posting (`postCarinaResponse` → [`writer::post_carina_response`]) and
/// the live `onPosted` emit, exactly as v4's `runCarinaQuery` does — the runner
/// only splices a *public* answer via `on_public_answer` and routes a failure
/// through Prospero.
pub trait RunCarinaQuery {
    /// Run the isolated query and return the result. In v4 the query never
    /// `Err`-throws (failures are the `CarinaResult::Err` arm), but a genuine
    /// panic-like failure is surfaced through `Result::Err` so the runner's
    /// catch-all logs + swallows it, matching v4's outer `try/catch`.
    ///
    /// Async (RPITIT, `-> impl Future + Send`): the real implementation
    /// ([`super::carina_query::run_carina_query`]) composes the async tool
    /// subsystem + streaming + memory search, and every caller (the runner, the
    /// finalizer, the `ask_carina` dispatch) is already in an async context. The
    /// argument SHAPE is frozen (the work order's "frozen" constraint); the
    /// sync-ness was an artifact of the canned test impl — the codebase has taken
    /// the same seams async before (`BuildContextSeams` / `ContextSummarySeams` /
    /// `LanternNotificationSink`). Generic-consumed (no boxing; the future stays
    /// `Send`).
    fn run(
        &mut self,
        opts: RunCarinaQueryOptions,
    ) -> impl std::future::Future<Output = Result<CarinaResult, CarinaRunError>> + Send;
}

/// The Prospero error-announcement seam — v4's `postProsperoCarinaError`
/// (post-office subsystem, wave 4+). Recorded by the harness. The runner never
/// inspects its result; a failure is caught by the outer catch-all.
pub trait PostProsperoCarinaError {
    fn post(&mut self, args: ProsperoCarinaErrorArgs) -> Result<(), CarinaRunError>;
}

/// The exact argument set the runner passes to the Prospero seam — v4's
/// `ProsperoCarinaErrorAnnouncement` (the subset the runner fills).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProsperoCarinaErrorArgs {
    pub chat_id: String,
    pub kind: CarinaErrorKind,
    pub character_name: Option<String>,
    pub detail: Option<String>,
    pub whisper: bool,
    pub asker_participant_id: Option<String>,
}

/// An opaque failure from a seam — the analogue of a thrown JS error caught by
/// v4's outer `try/catch`. The runner logs + swallows it; it never propagates.
#[derive(Debug, Clone)]
pub struct CarinaRunError(pub String);

/// Callbacks the caller supplies — v4's `onConsulting` / `onPublicAnswer`. The
/// `onPosted` callback is NOT here: the seam owns it (as `runCarinaQuery` does in
/// v4). Each method is a no-op by default — the v4 `?.` no-op.
pub trait CarinaMarkupCallbacks {
    /// Called once, before the query runs, with the answerer's name — the
    /// user-message path emits a "Consulting <name>…" status here. v4 calls this
    /// BEFORE the guarded region.
    fn on_consulting(&mut self, _character_name: &str) {}
    /// Called with a successful PUBLIC (non-whisper) answer message — the
    /// user-message path splices it into the in-flight turn. v4 calls this INSIDE
    /// the guarded region.
    fn on_public_answer(&mut self, _message: &PostedCarinaMessage) {}
}

/// A no-op callbacks impl (the assistant-markup path supplies neither
/// `onConsulting` nor `onPublicAnswer` in practice, though it does pass
/// `onPosted` — owned by the seam here).
pub struct NoCallbacks;
impl CarinaMarkupCallbacks for NoCallbacks {}

/// The runner options — v4 `CarinaMarkupOptions` (minus `onPosted`, seam-owned).
pub struct CarinaMarkupOptions {
    pub user_id: String,
    pub chat_id: String,
    /// Text to scan for `@Name:` / `@Name?` markup.
    pub text: String,
    /// Asker participant id: the user's persona (user-message path) or the
    /// responding character's participant (assistant-markup path). `None` when
    /// unavailable.
    pub asker_participant_id: Option<String>,
    /// The human operator typed this markup (user-message path). Gates the operator
    /// privilege AND whether the detection log records the operator's userId.
    pub operator_initiated: bool,
    /// Log labels for the detection/failure lines.
    pub log_labels: CarinaMarkupLogLabels,
}

/// One line of the observable trace, for the differential. Ordered: exactly the
/// callback fires and seam calls the runner made, in order. (Logs are not diffed
/// — they are not DB state, matching the other tier-3 differentials.)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CarinaTrace {
    /// The parse yielded no markup — the runner is a no-op. No further trace.
    NoMarkup,
    /// `onConsulting(<name>)` fired.
    Consulting { character_name: String },
    /// The `runCarinaQuery` seam was invoked with these (differential-relevant)
    /// arguments.
    QueryInvoked {
        character_name: String,
        question: String,
        whisper: bool,
        asker_participant_id: Option<String>,
        operator_initiated: bool,
    },
    /// A successful PUBLIC answer was spliced via `onPublicAnswer` (message id).
    PublicAnswer { message_id: String },
    /// A successful WHISPER answer — `onPublicAnswer` was deliberately NOT fired.
    WhisperAnswerNotSpliced { message_id: String },
    /// The Prospero error seam was invoked with these args.
    ProsperoErrorPosted(ProsperoCarinaErrorArgs),
    /// A seam threw; the catch-all logged + swallowed it (v4's outer catch).
    SeamThrewAndSwallowed { detail: String },
}

/// Detect Carina markup in `opts.text` and, if present, run the isolated
/// reference query via the injected [`RunCarinaQuery`] seam: fire the optional
/// "Consulting…" callback, let the seam post the answer (surfaced live inside the
/// seam), splice a PUBLIC answer via [`CarinaMarkupCallbacks::on_public_answer`],
/// and route any failure through the [`PostProsperoCarinaError`] seam. Missing
/// markup is a no-op. Never throws.
///
/// Returns the observable trace (ordered callback + seam calls) for the
/// differential. The runner itself performs no reads or writes — a real seam impl
/// reaches the writer through the [`Db`] it captures.
pub async fn run_carina_markup_query<Q, P>(
    opts: &CarinaMarkupOptions,
    callbacks: &mut dyn CarinaMarkupCallbacks,
    run_query: &mut Q,
    prospero: &mut P,
) -> Vec<CarinaTrace>
where
    Q: RunCarinaQuery,
    P: PostProsperoCarinaError,
{
    let mut trace = Vec::new();

    // v4: `const carinaQuery = parseCarinaQuery(opts.text); if (!carinaQuery) return;`
    let carina_query = match parse_carina_query(Some(&opts.text)) {
        Some(q) => q,
        None => {
            trace.push(CarinaTrace::NoMarkup);
            return trace;
        }
    };

    // v4 logs `Carina query detected in <detected>` here (operator-gated userId,
    // answerer, whisper). Logging is host-side and not diffed; the fields feed no
    // downstream branch, so the port omits the emit.

    // v4: `opts.onConsulting?.(carinaQuery.characterName)` — BEFORE the try.
    callbacks.on_consulting(&carina_query.character_name);
    trace.push(CarinaTrace::Consulting {
        character_name: carina_query.character_name.clone(),
    });

    // --- the guarded region (v4's `try { … } catch { logger.warn(...) }`) ---
    let query_opts = RunCarinaQueryOptions {
        user_id: opts.user_id.clone(),
        chat_id: opts.chat_id.clone(),
        character_name: carina_query.character_name.clone(),
        question: carina_query.question.clone(),
        whisper: carina_query.whisper,
        asker_participant_id: opts.asker_participant_id.clone(),
        operator_initiated: opts.operator_initiated,
    };
    trace.push(CarinaTrace::QueryInvoked {
        character_name: query_opts.character_name.clone(),
        question: query_opts.question.clone(),
        whisper: query_opts.whisper,
        asker_participant_id: query_opts.asker_participant_id.clone(),
        operator_initiated: query_opts.operator_initiated,
    });

    match run_query.run(query_opts).await {
        Ok(CarinaResult::Ok {
            message,
            message_id,
            ..
        }) => {
            // v4: splice a PUBLIC answer into the live turn; whispers stay scoped
            // to the asker. `if (!carinaQuery.whisper) opts.onPublicAnswer?.(msg)`.
            if !carina_query.whisper {
                callbacks.on_public_answer(&message);
                trace.push(CarinaTrace::PublicAnswer { message_id });
            } else {
                trace.push(CarinaTrace::WhisperAnswerNotSpliced { message_id });
            }
        }
        Ok(CarinaResult::Err { error }) => {
            // v4: `await postProsperoCarinaError({ chatId, kind, characterName,
            //       detail, whisper, askerParticipantId })`.
            let args = ProsperoCarinaErrorArgs {
                chat_id: opts.chat_id.clone(),
                kind: error.kind,
                character_name: error.character_name.clone(),
                detail: error.detail.clone(),
                whisper: carina_query.whisper,
                asker_participant_id: opts.asker_participant_id.clone(),
            };
            match prospero.post(args.clone()) {
                Ok(()) => trace.push(CarinaTrace::ProsperoErrorPosted(args)),
                // A throw from the Prospero post is caught by the outer catch-all.
                Err(e) => trace.push(CarinaTrace::SeamThrewAndSwallowed { detail: e.0 }),
            }
        }
        // v4's outer `catch (carinaError) { logger.warn(`Carina <failed> query
        // failed`, …) }` — the query seam threw; log + swallow, never re-throw.
        Err(e) => {
            trace.push(CarinaTrace::SeamThrewAndSwallowed { detail: e.0 });
        }
    }

    trace
}

/// Convenience: build a [`RunCarinaQuery`]-shaped seam from an `FnMut` returning a
/// future. Lets a caller (or the harness) supply the query as a closure without a
/// named type.
pub struct ClosureRunQuery<F>(pub F);

impl<F, Fut> RunCarinaQuery for ClosureRunQuery<F>
where
    F: FnMut(RunCarinaQueryOptions) -> Fut,
    Fut: std::future::Future<Output = Result<CarinaResult, CarinaRunError>> + Send,
{
    fn run(
        &mut self,
        opts: RunCarinaQueryOptions,
    ) -> impl std::future::Future<Output = Result<CarinaResult, CarinaRunError>> + Send {
        (self.0)(opts)
    }
}

/// Convenience: a [`PostProsperoCarinaError`] closure seam.
pub struct ClosureProspero<F>(pub F)
where
    F: FnMut(ProsperoCarinaErrorArgs) -> Result<(), CarinaRunError>;

impl<F> PostProsperoCarinaError for ClosureProspero<F>
where
    F: FnMut(ProsperoCarinaErrorArgs) -> Result<(), CarinaRunError>,
{
    fn post(&mut self, args: ProsperoCarinaErrorArgs) -> Result<(), CarinaRunError> {
        (self.0)(args)
    }
}

/// Marker to keep [`Db`] referenced from this module's public surface (a real
/// seam impl reaches the writer through it; the runner itself is db-free).
#[allow(dead_code)]
fn _db_is_a_dependency(_db: &Db) {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    fn posted(id: &str, whisper: bool, asker: Option<&str>) -> PostedCarinaMessage {
        let targets = if whisper {
            asker.map(|a| vec![a.to_string()])
        } else {
            None
        };
        PostedCarinaMessage {
            id: id.to_string(),
            message: serde_json::json!({ "type": "message", "id": id }),
            target_participant_ids: targets,
        }
    }

    fn opts(text: &str, operator: bool) -> CarinaMarkupOptions {
        CarinaMarkupOptions {
            user_id: "u1".into(),
            chat_id: "c1".into(),
            text: text.into(),
            asker_participant_id: Some("asker-p".into()),
            operator_initiated: operator,
            log_labels: CarinaMarkupLogLabels {
                detected: "user message".into(),
                failed: "user-message".into(),
            },
        }
    }

    #[derive(Default)]
    struct RecordingCallbacks {
        consulting: Vec<String>,
        public: Vec<String>,
    }
    impl CarinaMarkupCallbacks for RecordingCallbacks {
        fn on_consulting(&mut self, name: &str) {
            self.consulting.push(name.to_string());
        }
        fn on_public_answer(&mut self, m: &PostedCarinaMessage) {
            self.public.push(m.id.clone());
        }
    }

    #[tokio::test]
    async fn no_markup_is_a_noop() {
        let mut cb = RecordingCallbacks::default();
        let mut q = ClosureRunQuery(|_o| async { panic!("query must not run on no-markup") });
        let mut p = ClosureProspero(|_a| panic!("prospero must not run on no-markup"));
        let trace =
            run_carina_markup_query(&opts("just some text", true), &mut cb, &mut q, &mut p).await;
        assert_eq!(trace, vec![CarinaTrace::NoMarkup]);
        assert!(cb.consulting.is_empty());
        assert!(cb.public.is_empty());
    }

    #[tokio::test]
    async fn public_answer_fires_on_public_answer() {
        let mut cb = RecordingCallbacks::default();
        let mut q = ClosureRunQuery(|o: RunCarinaQueryOptions| async move {
            assert!(!o.whisper);
            Ok(CarinaResult::Ok {
                answer: "hi".into(),
                message_id: "m1".into(),
                message: posted("m1", false, Some("asker-p")),
                answerer_id: "char-a".into(),
                answerer_name: "Alice".into(),
            })
        });
        let mut p = ClosureProspero(|_a| panic!("no error path"));
        let trace =
            run_carina_markup_query(&opts("@Alice: hello", true), &mut cb, &mut q, &mut p).await;
        assert_eq!(cb.consulting, vec!["Alice".to_string()]);
        assert_eq!(cb.public, vec!["m1".to_string()]);
        assert!(matches!(trace[2], CarinaTrace::PublicAnswer { .. }));
    }

    #[tokio::test]
    async fn whisper_answer_does_not_fire_on_public_answer() {
        let mut cb = RecordingCallbacks::default();
        let mut q = ClosureRunQuery(|o: RunCarinaQueryOptions| async move {
            assert!(o.whisper);
            Ok(CarinaResult::Ok {
                answer: "psst".into(),
                message_id: "m2".into(),
                message: posted("m2", true, Some("asker-p")),
                answerer_id: "char-b".into(),
                answerer_name: "Bob".into(),
            })
        });
        let mut p = ClosureProspero(|_a| panic!("no error path"));
        let trace =
            run_carina_markup_query(&opts("@Bob? secret", true), &mut cb, &mut q, &mut p).await;
        assert_eq!(cb.consulting, vec!["Bob".to_string()]);
        assert!(
            cb.public.is_empty(),
            "whisper must NOT splice a public answer"
        );
        assert!(matches!(
            trace[2],
            CarinaTrace::WhisperAnswerNotSpliced { .. }
        ));
    }

    #[tokio::test]
    async fn error_routes_to_prospero_with_exact_args() {
        let recorded: Rc<RefCell<Vec<ProsperoCarinaErrorArgs>>> = Rc::new(RefCell::new(Vec::new()));
        let rec2 = recorded.clone();
        let mut cb = RecordingCallbacks::default();
        let mut q = ClosureRunQuery(|_o: RunCarinaQueryOptions| async {
            Ok(CarinaResult::Err {
                error: CarinaError {
                    kind: CarinaErrorKind::NotFound,
                    detail: None,
                    character_name: None,
                },
            })
        });
        let mut p = ClosureProspero(move |a| {
            rec2.borrow_mut().push(a);
            Ok(())
        });
        let trace =
            run_carina_markup_query(&opts("@Ghost? anyone", true), &mut cb, &mut q, &mut p).await;
        let args = &recorded.borrow()[0];
        assert_eq!(args.kind, CarinaErrorKind::NotFound);
        assert!(args.whisper);
        assert_eq!(args.asker_participant_id.as_deref(), Some("asker-p"));
        assert!(matches!(
            trace.last(),
            Some(CarinaTrace::ProsperoErrorPosted(_))
        ));
        assert!(cb.public.is_empty());
    }

    #[tokio::test]
    async fn query_seam_throw_is_swallowed() {
        let mut cb = RecordingCallbacks::default();
        let mut q = ClosureRunQuery(|_o: RunCarinaQueryOptions| async {
            Err(CarinaRunError("boom".into()))
        });
        let mut p = ClosureProspero(|_a| panic!("error arm not reached on a throw"));
        let trace =
            run_carina_markup_query(&opts("@Alice: hi", false), &mut cb, &mut q, &mut p).await;
        assert!(matches!(
            trace.last(),
            Some(CarinaTrace::SeamThrewAndSwallowed { .. })
        ));
    }

    #[test]
    fn error_kind_strings_match_v4() {
        assert_eq!(CarinaErrorKind::NotFound.as_str(), "not-found");
        assert_eq!(CarinaErrorKind::NoProfile.as_str(), "no-profile");
        assert_eq!(CarinaErrorKind::LlmFailed.as_str(), "llm-failed");
    }
}
