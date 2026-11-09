//! Storyteller mode — the host half (v4 `orchestrator.service.ts`
//! `openRoundOnSend` / `runStorytellerBid` / `handleStorytellerRun`, P4.D343).
//!
//! - **The round opener** ([`ChatSpine::open_round_on_send`]): a storyteller
//!   post was recorded (S1's `storyteller_hold`); open a round, ask every
//!   invited seat for a bid IN PARALLEL, stream each card as it lands, and hand
//!   the board over after every bid is in or [`STORYTELLER_BOARD_TIMEOUT_MS`]
//!   passes. The clock is the HOST's (ruling R-3 — `BOARD_TIMEOUT_HOST_CADENCE`,
//!   narrowed by P4.D368 to this 45 s timeout: the core holds the constant and
//!   no clock; the all-at-once cap is core's `run_storyteller_round`). Each bid runs on its own
//!   thread + current-thread runtime (the send's own bridge): a bid still out
//!   at the timeout keeps running DETACHED and lands through `record_bid`, which
//!   drops it once the round is no longer `open` — v4's late-bid behaviour.
//! - **The bid runner** ([`ChatSpine::run_storyteller_bid`]): the ordinary turn
//!   path with `storyteller_bid` set — `purpose == Bid`, the bid section
//!   trailing, `tool_choice: "none"`, logged `STORYTELLER_BID` with the round's
//!   id — through a discarding sink.
//! - **The run endpoint** ([`StorytellerDriver::run`]): plan (the 400 / 404 /
//!   409 refusals answer before any frame), then the round's speaking turns on
//!   the chat's stream, `turnStart` / `turnComplete` around each sequential
//!   turn, the round's housekeeping ONCE, then ONE `chainComplete`.
//!
//! **`DRY_SCOPE_BY_GATES` (ruling R-1).** v4 runs a bid inside `runDry`, a
//! repository proxy that discards EVERY write but the LLM log. v5 has no such
//! proxy: the bid's deps carry the writes-free [`StorytellerSeams::Dry`] for
//! the context builder's post-office writers (Aurora's Core whisper, the
//! Commonplace whispers + their persist, the Host's timestamp note, Suparṇā's
//! mail whisper), and the orchestrator gates its own direct writes on
//! `is_bid` (the `forceToolsOnNextMessage` spend, the raised-hand take, the
//! Prospero cadence post; the recall delivery is C1's `ContextPurpose::Bid`;
//! the agent-turn reset and the inform consume are never reached — continue
//! mode, no finalizer). Both ways: in v4 a NEW write site inside a bid build
//! is silently discarded; in v5 it WRITES. v4's `Dry scope …` DEBUG / WARN
//! have no counterpart. The `AsyncLocalStorage` twins (`round-context.ts`,
//! `dry-scope.ts`) are not ported: `LogContext::storyteller_round_id` and the
//! `is_bid` gates are the explicit threading.
//!
//! **All at once (`order: "simultaneous"`, P4.D368 — P4.D343 item 41).** The
//! concurrent group and its per-profile cap run in core's
//! `run_storyteller_round` (v4 `runStorytellerRound` + `runWithCap`); this
//! file supplies the turns. Every turn of the round runs on ONE task — the
//! run's own thread's current-thread runtime, joined cooperatively (v4's one
//! event loop; the turns' work is network-bound). A concurrent turn streams
//! through [`BroadcastSink::seated`] (v4 `seatedFrameEncoder` — `seat:
//! {participantId, concurrent: true}` LAST on every frame) and the hooks emit
//! no `turnStart` / `turnComplete` for it. v4's `sharedStreamController`
//! (a no-op `close()`) converges by construction: a v5 sink has no close. Each
//! turn keeps its OWN `SubmissionLedger` where v4 shares one across the
//! round: the ledger is read only by the shell's error frame, which a
//! storyteller turn's failure never builds from it (it passes a default) —
//! converged, unobservable.
//!
//! `CONCURRENT_TURN_FAILURE_CONTAINED` (ruling R-3 of P4.D368): a speaking turn
//! that FAILS is caught per turn — ERROR `Storyteller speaking turn failed`, an
//! error frame (SEATED for a concurrent turn, through its seat's sink, so the
//! Salon drops only that seat's bubble; unseated for a sequential one), an
//! empty outcome — and the round goes on, closing `ran` with ONE
//! `chainComplete`. Both ways: v4's concurrent `runTurn` throw rejects
//! `Promise.all`, so `runStorytellerRound` rejects BEFORE `markClosed` — the
//! round is left `running`, `openTurnStream`'s catch logs `Streaming error`
//! and enqueues its unseated `Failed to generate response` frame, and no
//! `chainComplete` follows (the board would stay busy until Stop). v5 keeps
//! the other seats' turns and closes the round.
//!
//! `R-5 BID_ROWS_SKIP_AGGREGATES`: a bid's `llm_logs` row has no message and
//! never reaches the finalizer, so it adds nothing to the chat's token
//! aggregates — v4 bypasses `finalizeMessageResponse`'s aggregate write the
//! same way (a faithful omission).

use super::*;

use quilltap_core::services::build_context::BuildContextSeams;

use quilltap_core::api::chat_storyteller::{
    StorytellerDriver, StorytellerDriverError, StorytellerFuture, StorytellerRunRequest,
};
use quilltap_core::services::chat_events::{
    ChainCompletePayload, DiscardSink, StorytellerBidFrame, TrueBool, TurnCompletePayload,
    TurnStartPayload,
};
use quilltap_core::services::message_finalizer::{ProcessMessageResult, StorytellerHold};
use quilltap_core::services::storyteller_rounds::{
    self as rounds, RoundEnding, RoundServiceError, RoundTurnRunner, StorytellerPlannedTurn,
    StorytellerTurnOutcome, STORYTELLER_BOARD_TIMEOUT_MS,
};
use quilltap_core::storyteller::types::{
    StorytellerBid, StorytellerBidCallOutcome, StorytellerBidRequest, StorytellerTurnContext,
};

/// The context builder's post-office writers for a storyteller call: the real
/// ones for a speaking turn, none for a bid (R-1 — what v4's `runDry` discards).
pub(crate) enum StorytellerSeams<'a> {
    Real(RealBuildContextSeams<'a>),
    Dry,
}

impl BuildContextSeams for StorytellerSeams<'_> {
    async fn post_host_timestamp_announcement(&self, chat_id: &str, formatted: &str) {
        if let StorytellerSeams::Real(r) = self {
            r.post_host_timestamp_announcement(chat_id, formatted).await;
        }
    }
    async fn post_core_whisper(
        &self,
        chat_id: &str,
        target_participant_id: &str,
        persona_content: &str,
        opaque_content: &str,
    ) {
        if let StorytellerSeams::Real(r) = self {
            r.post_core_whisper(
                chat_id,
                target_participant_id,
                persona_content,
                opaque_content,
            )
            .await;
        }
    }
    async fn post_commonplace_whisper(
        &self,
        chat_id: &str,
        target_participant_id: Option<&str>,
        content: &str,
    ) -> bool {
        match self {
            StorytellerSeams::Real(r) => {
                r.post_commonplace_whisper(chat_id, target_participant_id, content)
                    .await
            }
            // Nothing posted → the two persist writes it gates stay unwritten.
            StorytellerSeams::Dry => false,
        }
    }
    async fn post_commonplace_retrospective_recall(
        &self,
        chat_id: &str,
        target_participant_id: Option<&str>,
        content: &str,
    ) {
        if let StorytellerSeams::Real(r) = self {
            r.post_commonplace_retrospective_recall(chat_id, target_participant_id, content)
                .await;
        }
    }
    async fn post_suparna_mail(
        &self,
        chat_id: &str,
        letters: &[quilltap_core::post_office::mailbox::DeliveredLetterSummary],
        target_participant_id: Option<&str>,
        zone: &quilltap_core::host_zone::TimeZone,
    ) {
        if let StorytellerSeams::Real(r) = self {
            r.post_suparna_mail(chat_id, letters, target_participant_id, zone)
                .await;
        }
    }
}

fn core_error(kind: ErrorKind, message: impl Into<String>) -> CoreError {
    CoreError {
        kind,
        message: message.into(),
        pepper_state: None,
        code: None,
        associations: None,
        character_id: None,
        entity: None,
        details: None,
        already_saved: None,
        submission: None,
    }
}

impl<EMB, CMP, STR, PF> ChatSpine<EMB, CMP, STR, PF>
where
    EMB: EmbeddingProvider + Send + Sync + 'static,
    CMP: CompletionProvider + Send + Sync + 'static,
    STR: StreamingCompletionProvider + Send + Sync + 'static,
    PF: PricingFetch + Send + Sync + 'static,
{
    /// One storyteller `processMessage` (a bid when `dry`, else a speaking
    /// turn) with a freshly built deps set — `run_send`'s per-call assembly.
    async fn process_storyteller<SNK: EventSink + Sync>(
        &self,
        chat_id: String,
        sink: &SNK,
        options: SendMessageOptions,
        log_context: LogContext,
        turn_handle: Option<turn_control::TurnControlHandle>,
        dry: bool,
    ) -> Result<ProcessMessageResult, quilltap_core::db::DbError> {
        let db = self.db.clone();
        // Per-call clock + RNG (v4 Date.now() / Math.random()).
        let now_ms = now_unix_ms();
        let random01 = os_draw_source();
        let clock = ProcessClock {
            now_ms,
            local_offset_minutes: self.local_offset_minutes(now_ms),
            random01: random01.clone(),
        };

        let resolved = self
            .preresolve_provider_model(
                &chat_id,
                options.responding_participant_id.as_deref(),
                None,
                None,
                true,
                &random01,
            )
            .await;
        let (model_context_limit, provider_supports_web_search) = self.registry_inputs(resolved);
        let timestamp_config = self.resolve_timestamp_config(&chat_id);

        // The per-call cheap-LLM executor with llm_logs logging (v4's
        // cheap-LLM logLLMCall context: chatId, no messageId) — stamped with
        // the round (`round_sub_call_executor`).
        let executor = round_sub_call_executor(&db, &chat_id, &log_context);
        // The vision describer the turn (and its Carina / Brahma tool runners)
        // reach, re-built carrying the round (v4 `llm-logging.service.ts:227`).
        let image_describe: Option<Arc<dyn quilltap_core::api::chat_media::ImageDescribeDriver>> =
            round_image_describe(
                &db,
                &self.completion,
                self.image_describe.is_some(),
                &log_context,
            )
            .map(|r| Arc::new(r) as _);
        let round_tool_runner = || {
            let runner = self.tool_runner();
            match &image_describe {
                Some(d) => runner.with_image_describe(Arc::clone(d)),
                None => runner,
            }
        };

        // The REAL seams (the harness-mirrored construction; module header).
        let bc_seams = if dry {
            StorytellerSeams::Dry
        } else {
            StorytellerSeams::Real(RealBuildContextSeams { db: &db })
        };
        let orchestrator_seams = HostOrchestratorSeams { db: db.clone() };
        let router = DangerContentRouter::new(db.clone(), DbApiKeys(db.clone()));
        let mut confirmation = TimeoutConfirmation::new(RealAnswerConfirmation {
            completion: &*self.completion,
            executor: &executor,
        });
        let mut compression = RealAsyncCompression {
            db: &db,
            completion: &*self.completion,
            executor: &executor,
        };
        let mut cost = PricingCostTracker {
            fetcher: &self.pricing,
            ctx: build_pricing_context(&db, SINGLE_USER_ID),
        };
        let mut prospero = ThreadedProspero { db: db.clone() };
        // `model_supports_native_tools: true` on the carina/Brahma engines
        // matches v4's checkModelSupportsTools default for models absent from
        // the fallback table (the engines resolve their own profiles, which
        // the spine cannot know here).
        let brahma = RealBrahmaConsole::new(
            db.clone(),
            Arc::clone(&self.streaming),
            round_tool_runner(),
            RegistryToolCallDetector::built_in(),
            true,
        );
        let ask_carina = ErasedAskCarina::new(TypedAskCarina {
            db: db.clone(),
            embedding: Arc::clone(&self.embedding),
            streaming: Arc::clone(&self.streaming),
            tool_runner: round_tool_runner(),
            tool_detector: RegistryToolCallDetector::built_in(),
            brahma: RealBrahmaConsole::new(
                db.clone(),
                Arc::clone(&self.streaming),
                round_tool_runner(),
                RegistryToolCallDetector::built_in(),
                true,
            ),
            prospero: ThreadedProspero { db: db.clone() },
            model_supports_native_tools: true,
            now_ms: now_ms as f64,
            turn_control: turn_handle.clone(),
            // The round's id on the consultation's row (v4
            // `llm-logging.service.ts:227`).
            log_context: log_context.clone(),
        });
        let carina_tool_runner = round_tool_runner();
        let carina_detector = RegistryToolCallDetector::built_in();
        let mut carina_query = RealCarinaQuery::new(CarinaQueryDeps {
            db: &db,
            embedding: &*self.embedding,
            streaming: &*self.streaming,
            tool_runner: &carina_tool_runner,
            tool_detector: &carina_detector,
            sink,
            brahma: &brahma,
            model_supports_native_tools: true,
            now_ms: now_ms as f64,
            turn_control: turn_handle.as_ref(),
            log_context: log_context.clone(),
        });
        let mut rng_bytes = OsRandomBytes;

        let mut deps = OrchestratorDeps {
            db: &db,
            embedding: &*self.embedding,
            completion: &*self.completion,
            streaming: &*self.streaming,
            executor: &executor,
            ask_carina: &ask_carina,
            sink,
            pricing: &self.pricing,
            build_context_seams: &bc_seams,
            orchestrator_seams: &orchestrator_seams,
            file_bytes: &*self.file_bytes,
            image_transcoder: &*self.image_transcoder,
            danger_router: &router,
            confirmation: &mut confirmation,
            compression: &mut compression,
            cost: &mut cost,
            carina_query: &mut carina_query,
            prospero: &mut prospero,
            rng_bytes: &mut rng_bytes,
            // P4.42/P4.59: the in-chat turn's `search_web` provider (None only
            // when the Serper provider is neither registered nor env-keyed).
            web_search: self.web_search.clone(),
            // The a14a1811-round wire: the in-chat `describe_image` vision tier
            // + the photo-tool bytes store (the §3 review's catch).
            image_describe: image_describe.clone(),
            photo_bytes: Some(Arc::clone(&self.file_bytes) as _),
            consult: self.consult.clone(),
            // The `624d0d25c` §3 review's catch (as the Salon spine's turn).
            tool_embedding: Some(self.tool_embedding()),
            turn_control: turn_handle.clone(),
        };

        let input = ProcessMessageInput {
            log_context,
            chat_id: chat_id.clone(),
            user_id: SINGLE_USER_ID.to_string(),
            options,
            clock,
            model_context_limit,
            timestamp_config: timestamp_config.clone(),
            timezone: Some(self.tz.clone()),
            server_tz: Some(self.tz.clone()),
            display_zone: self.display_zone.clone(),
            provider_supports_web_search,
        };
        let ledger = std::cell::Cell::new(orchestrator::SubmissionLedger::default());
        orchestrator::process_message_with_ledger(&mut deps, &input, &ledger).await
    }

    /// v4 `runStorytellerBid`: build one seat's BID through the ordinary turn
    /// path, dry, into a discarding sink. Never fails: a thrown build is WARN
    /// `[Storyteller] Bid build failed` and an `Err` outcome.
    pub(crate) async fn run_storyteller_bid(
        &self,
        chat_id: &str,
        participant_id: &str,
        round_id: &str,
    ) -> StorytellerBidCallOutcome {
        let options = SendMessageOptions {
            continue_mode: true,
            responding_participant_id: Some(participant_id.to_string()),
            storyteller_bid: Some(StorytellerBidRequest {
                round_id: round_id.to_string(),
            }),
            ..Default::default()
        };
        let log_context = LogContext {
            storyteller_round_id: Some(round_id.to_string()),
            ..Default::default()
        };
        match self
            .process_storyteller(
                chat_id.to_string(),
                &DiscardSink,
                options,
                log_context,
                None,
                true,
            )
            .await
        {
            Ok(result) => result.bid.unwrap_or(StorytellerBidCallOutcome::Err {
                reason: "The bid could not be built".to_string(),
            }),
            Err(e) => {
                let reason = e.to_string();
                tracing::warn!(
                    target: "quilltap::storyteller",
                    chatId = %chat_id,
                    participantId = %participant_id,
                    roundId = %round_id,
                    error = %reason,
                    "[Storyteller] Bid build failed"
                );
                StorytellerBidCallOutcome::Err { reason }
            }
        }
    }

    /// v4 `bidForSeat` on its own thread: the bid call, then the card landed
    /// on the round (INFO `Storyteller bid landed`). The answered card comes
    /// back on `tx` — or, past the board timeout, to nobody: it still lands.
    fn spawn_bid(
        &self,
        chat_id: String,
        round_id: String,
        card: StorytellerBid,
        tx: tokio::sync::mpsc::UnboundedSender<StorytellerBid>,
    ) {
        let state = self.clone_state();
        std::thread::spawn(move || {
            let Ok(rt) = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            else {
                return;
            };
            rt.block_on(async move {
                let outcome = state
                    .run_storyteller_bid(&chat_id, &card.participant_id, &round_id)
                    .await;
                if let Ok(answered) =
                    rounds::land_bid(&state.db, &round_id, &chat_id, &card, &outcome).await
                {
                    let _ = tx.send(answered);
                }
            });
        });
    }

    /// v4 `openRoundOnSend`: the storyteller post is recorded; an `@Name:`
    /// post opens no round (it was answered already). Otherwise the round's
    /// frames — `storytellerRound`, a `storytellerBid` per card as it lands
    /// (within the board timeout), `roundOpen`, `chainComplete round_open`.
    /// No chain, no after-turns housekeeping (v4 returns before both).
    pub(crate) async fn open_round_on_send(
        &self,
        chat_id: &str,
        hold: StorytellerHold,
        sink: &BroadcastSink,
    ) -> Result<(), CoreError> {
        if hold.carina {
            tracing::debug!(
                target: "quilltap::storyteller",
                chatId = %chat_id,
                "[Storyteller] Carina query — no round opens"
            );
            sink.emit(ChatEvent::chain_complete(ChainCompletePayload {
                reason: "user_turn".to_string(),
                next_speaker_id: None,
                chain_depth: 0,
                paused: None,
                held_user_turn: None,
            }));
            return Ok(());
        }
        let mut random = os_random01;
        let round = match rounds::open_storyteller_round(
            &self.db,
            chat_id,
            hold.trigger_message_id.as_deref(),
            &mut random,
        )
        .await
        {
            Ok(r) => r,
            Err(RoundServiceError::Refused(r)) => {
                return Err(core_error(ErrorKind::BadRequest, r.message))
            }
            Err(RoundServiceError::Db(e)) => {
                return Err(core_error(ErrorKind::Internal, e.to_string()))
            }
        };
        sink.emit(ChatEvent::StorytellerRound {
            storyteller_round: serde_json::to_value(&round).unwrap_or(Value::Null),
        });
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let pending: Vec<StorytellerBid> = round
            .bids
            .iter()
            .filter(|b| {
                b.status == quilltap_core::storyteller::types::StorytellerBidStatus::Pending
            })
            .cloned()
            .collect();
        let expected = pending.len();
        for card in pending {
            self.spawn_bid(chat_id.to_string(), round.id.clone(), card, tx.clone());
        }
        drop(tx);
        let gather = async {
            let mut landed = 0;
            while landed < expected {
                match rx.recv().await {
                    Some(bid) => {
                        landed += 1;
                        sink.emit(ChatEvent::StorytellerBid {
                            storyteller_bid: StorytellerBidFrame {
                                round_id: round.id.clone(),
                                bid: serde_json::to_value(&bid).unwrap_or(Value::Null),
                            },
                        });
                    }
                    // Every bid thread finished (a failed land sends nothing).
                    None => break,
                }
            }
        };
        let timeout_ms = self.storyteller_board_timeout_ms();
        if tokio::time::timeout(std::time::Duration::from_millis(timeout_ms), gather)
            .await
            .is_err()
        {
            rounds::log_board_timeout(chat_id, &round.id, timeout_ms);
        }
        sink.emit(ChatEvent::RoundOpen {
            round_open: TrueBool,
            round_id: round.id.clone(),
        });
        sink.emit(ChatEvent::chain_complete(ChainCompletePayload {
            reason: "round_open".to_string(),
            next_speaker_id: None,
            chain_depth: 0,
            paused: None,
            held_user_turn: None,
        }));
        Ok(())
    }

    /// v4 `triggerAfterTurnsHousekeeping` after a round whose last turn carried
    /// a scene-tracking context: the DEBUG and the Scriptorium render (the
    /// scene-state job stays unported, as on the send path).
    async fn after_turns_housekeeping(&self, chat_id: &str) {
        tracing::debug!(
            target: "quilltap::chat",
            chatId = %chat_id,
            sceneTracking = true,
            "After-turns housekeeping",
        );
        quilltap_core::services::conversation_render_job::trigger_conversation_render(
            &self.db,
            SINGLE_USER_ID,
            chat_id,
        )
        .await;
    }

    /// Whether the user's chat settings carry `cheapLLMSettings` (v4's
    /// `memoryChatSettings.cheapLLMSettings` — the extraction gate).
    fn cheap_llm_settings_present(&self) -> bool {
        HostOrchestratorSeams {
            db: self.db.clone(),
        }
        .chat_settings(SINGLE_USER_ID)
        .is_some_and(|s| s.cheap_llm_settings_present)
    }

    /// The connection profile a seat answers with (v4's
    /// `sceneTrackingContext.connectionProfile.id`).
    fn chat_connection_profile_id(&self, chat_id: &str, participant_id: &str) -> Option<String> {
        let chat = self
            .db
            .read_main(|c| quilltap_core::db::chats_read::find_by_id(c, chat_id))
            .ok()
            .flatten()?;
        chat.get("participants")
            .and_then(Value::as_array)?
            .iter()
            .find(|p| p.get("id").and_then(Value::as_str) == Some(participant_id))
            .and_then(|p| p.get("connectionProfileId"))
            .and_then(Value::as_str)
            .map(str::to_string)
    }

    /// The board timeout (R-3): [`STORYTELLER_BOARD_TIMEOUT_MS`] unless the
    /// host's environment shortens it (`QT_STORYTELLER_BOARD_TIMEOUT_MS`, the
    /// tier-3 rows' 0 ms / no-wait arm).
    fn storyteller_board_timeout_ms(&self) -> u64 {
        std::env::var("QT_STORYTELLER_BOARD_TIMEOUT_MS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(STORYTELLER_BOARD_TIMEOUT_MS)
    }

    /// v4 `handleSendMessage`'s storyteller nudge: `continueMode && …
    /// neverPauseForUser !== true && turnMode === 'storyteller'` (ONE fresh
    /// read) — storyteller mode never chains.
    pub(crate) fn storyteller_single_turn(&self, chat_id: &str, continue_mode: bool) -> bool {
        continue_mode
            && self
                .db
                .read_main(|c| quilltap_core::db::chats_read::find_by_id(c, chat_id))
                .ok()
                .flatten()
                .and_then(|chat| {
                    chat.get("turnMode")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                })
                .as_deref()
                == Some("storyteller")
    }

    /// v4 `handleStorytellerRun` after the plan: the turns on the chat's
    /// stream, the round's housekeeping once, ONE `chainComplete`.
    async fn run_planned_round(
        &self,
        req: &StorytellerRunRequest,
        round: quilltap_core::storyteller::types::StorytellerRound,
        turns: Vec<StorytellerPlannedTurn>,
    ) -> Result<(), CoreError> {
        let registration = turn_control::register_active_turn(&req.chat_id);
        let handle = registration.handle();
        let sink = BroadcastSink::frames(req.chat_id.clone(), self.events.clone()); // === P4.D366 (L8) ===
        let runner = SpineRoundRunner {
            spine: self,
            sink: &sink,
            chat_id: req.chat_id.clone(),
            round_id: round.id.clone(),
            handle: handle.clone(),
            last_scene: std::cell::RefCell::new(None),
        };
        let (ended, _last) = rounds::run_storyteller_round(
            &self.db,
            &req.chat_id,
            &round,
            &turns,
            req.order,
            &runner,
        )
        .await
        .map_err(|e| core_error(ErrorKind::Internal, e.to_string()))?;
        let last_scene = runner.last_scene.borrow().clone();
        if let Some(profile_id) = last_scene {
            let cheap = self.cheap_llm_settings_present();
            if let Err(e) =
                quilltap_core::services::message_finalizer::run_storyteller_round_housekeeping(
                    &self.db,
                    &req.chat_id,
                    SINGLE_USER_ID,
                    &profile_id,
                    cheap,
                )
                .await
            {
                tracing::warn!(target: "quilltap::storyteller", chatId = %req.chat_id, error = %e, "Storyteller round housekeeping write failed");
            }
            self.after_turns_housekeeping(&req.chat_id).await;
        }
        let depth = turns.len() as i64;
        sink.emit(ChatEvent::chain_complete(match ended {
            RoundEnding::Stopped => ChainCompletePayload {
                reason: "stopped".to_string(),
                next_speaker_id: None,
                chain_depth: depth,
                paused: Some(true),
                held_user_turn: None,
            },
            RoundEnding::Yielded => ChainCompletePayload {
                reason: "yielded".to_string(),
                next_speaker_id: None,
                chain_depth: depth,
                paused: None,
                held_user_turn: None,
            },
            RoundEnding::Ran => ChainCompletePayload {
                reason: "user_turn".to_string(),
                next_speaker_id: None,
                chain_depth: depth,
                paused: None,
                held_user_turn: None,
            },
        }));
        drop(registration);
        Ok(())
    }
}

/// The round runner's turns over the spine (v4 `handleStorytellerRun`'s
/// `runTurn` / `onTurnStart` / `onTurnEnd`). A concurrent (all-at-once) turn
/// streams through `sink.seated(participantId)` — v4 `seatedFrameEncoder`, the
/// seat LAST on every frame it emits — and gets no `turnStart` /
/// `turnComplete` from the hooks (v4 `orchestrator.service.ts:364`, `:372`).
/// The `turn_control` handle is shared by every turn, so a Stop reaches them
/// all.
struct SpineRoundRunner<'s, EMB, CMP, STR, PF>
where
    EMB: EmbeddingProvider + Send + Sync + 'static,
    CMP: CompletionProvider + Send + Sync + 'static,
    STR: StreamingCompletionProvider + Send + Sync + 'static,
    PF: PricingFetch + Send + Sync + 'static,
{
    spine: &'s ChatSpine<EMB, CMP, STR, PF>,
    sink: &'s BroadcastSink,
    chat_id: String,
    round_id: String,
    handle: turn_control::TurnControlHandle,
    /// The connection profile of the last turn to FINISH, when it carried a
    /// scene-tracking context (v4 `lastOutcome.sceneTrackingContext
    /// .connectionProfile` — `lastOutcome` is the last turn to finish).
    last_scene: std::cell::RefCell<Option<String>>,
}

impl<EMB, CMP, STR, PF> RoundTurnRunner for SpineRoundRunner<'_, EMB, CMP, STR, PF>
where
    EMB: EmbeddingProvider + Send + Sync + 'static,
    CMP: CompletionProvider + Send + Sync + 'static,
    STR: StreamingCompletionProvider + Send + Sync + 'static,
    PF: PricingFetch + Send + Sync + 'static,
{
    async fn run_turn(
        &self,
        turn: &StorytellerPlannedTurn,
        context: StorytellerTurnContext,
    ) -> StorytellerTurnOutcome {
        let options = SendMessageOptions {
            continue_mode: true,
            responding_participant_id: Some(turn.participant_id.clone()),
            storyteller: Some(context),
            ..Default::default()
        };
        let log_context = LogContext {
            storyteller_round_id: Some(self.round_id.clone()),
            ..Default::default()
        };
        // v4: `turn.concurrent ? seatedFrameEncoder(turn.participantId) :
        // encoder` (and the shared controller, whose `close()` is a no-op — a
        // v5 sink has no close, so that half converges by construction).
        let seated = turn
            .concurrent
            .then(|| self.sink.seated(&turn.participant_id));
        // Each turn's future is BOXED: the all-at-once group holds several
        // `process_message` futures in one `join_all`, and inline they overflow
        // the run thread's stack in a debug build.
        let result = match &seated {
            Some(seated) => {
                Box::pin(self.spine.process_storyteller(
                    self.chat_id.clone(),
                    seated,
                    options,
                    log_context,
                    Some(self.handle.clone()),
                    false,
                ))
                .await
            }
            None => {
                Box::pin(self.spine.process_storyteller(
                    self.chat_id.clone(),
                    self.sink,
                    options,
                    log_context,
                    Some(self.handle.clone()),
                    false,
                ))
                .await
            }
        };
        match result {
            Ok(result) => {
                let outcome = StorytellerTurnOutcome {
                    spoke: result.has_content && !result.skipped,
                    yielded: result.yielded,
                    message_id: (!result.message_id.is_empty()).then(|| result.message_id.clone()),
                    has_scene_tracking: result.scene_tracking_character_ids.is_some(),
                };
                *self.last_scene.borrow_mut() = if outcome.has_scene_tracking {
                    self.spine
                        .chat_connection_profile_id(&self.chat_id, &turn.participant_id)
                } else {
                    None
                };
                outcome
            }
            Err(e) => {
                tracing::error!(
                    target: "quilltap::storyteller",
                    chatId = %self.chat_id,
                    participantId = %turn.participant_id,
                    error = %e,
                    "Storyteller speaking turn failed"
                );
                match &seated {
                    // A CONCURRENT seat's failure goes out on its own seat —
                    // the round is still streaming the other seats' turns
                    // (`CONCURRENT_TURN_FAILURE_CONTAINED`). An unseated error
                    // frame here would read, to the Salon, as the whole run's
                    // failure: it ended the main stream mid-round and left the
                    // failed seat's half-written bubble on screen for good.
                    Some(seated) => emit_seated_error_frame(seated, &self.chat_id, &e.to_string()),
                    None => emit_send_error_frame(
                        &self.spine.events,
                        &self.chat_id,
                        &e.to_string(),
                        orchestrator::SubmissionLedger::default(),
                    ),
                }
                *self.last_scene.borrow_mut() = None;
                StorytellerTurnOutcome::default()
            }
        }
    }

    fn on_turn_start(&self, turn: &StorytellerPlannedTurn, depth: usize) {
        if turn.concurrent {
            return;
        }
        self.sink.emit(ChatEvent::turn_start(TurnStartPayload {
            participant_id: turn.participant_id.clone(),
            character_name: turn.character_name.clone(),
            chain_depth: depth as i64,
        }));
    }

    fn on_turn_end(
        &self,
        turn: &StorytellerPlannedTurn,
        depth: usize,
        outcome: &StorytellerTurnOutcome,
    ) {
        if turn.concurrent {
            return;
        }
        self.sink
            .emit(ChatEvent::turn_complete(TurnCompletePayload {
                participant_id: turn.participant_id.clone(),
                message_id: outcome.message_id.clone().unwrap_or_default(),
                chain_depth: depth as i64,
                skipped: (!outcome.spoke).then_some(true),
            }));
    }

    fn is_stopped(&self) -> bool {
        self.handle.is_stopped()
    }
}

/// The error frame for a CONCURRENT seat's failed turn: v4's shell frame
/// (`Failed to generate response`, `errorType` `Error`, the failure as
/// `details`) through the seat's own sink, so it carries `seat` LAST like
/// every other frame of that turn. The submission flags are not carried: a
/// storyteller turn runs in continue mode and never writes the operator's
/// submission, so they would always read `false` (and the Salon's seated
/// `error` arm reads only the message). Same `[TurnStream]` DEBUG as the
/// unseated shell frame.
fn emit_seated_error_frame(seated: &BroadcastSink, chat_id: &str, details: &str) {
    let error_type = "Error";
    let submission = orchestrator::SubmissionLedger::default();
    tracing::debug!(
        chatId = %chat_id,
        errorType = error_type,
        userMessagePersisted = submission.user_message_persisted,
        pendingToolResultsPersisted = submission.pending_tool_results_persisted,
        "[TurnStream] Emitting error frame",
    );
    seated.emit(ChatEvent::error(
        "Failed to generate response",
        error_type,
        details,
    ));
}

fn refusal(e: RoundServiceError) -> StorytellerDriverError {
    match e {
        RoundServiceError::Refused(r) => StorytellerDriverError::Refused(r),
        RoundServiceError::Db(d) => {
            StorytellerDriverError::Core(core_error(ErrorKind::Internal, d.to_string()))
        }
    }
}

impl<EMB, CMP, STR, PF> StorytellerDriver for ChatSpine<EMB, CMP, STR, PF>
where
    EMB: EmbeddingProvider + Send + Sync + 'static,
    CMP: CompletionProvider + Send + Sync + 'static,
    STR: StreamingCompletionProvider + Send + Sync + 'static,
    PF: PricingFetch + Send + Sync + 'static,
{
    fn run(&self, req: StorytellerRunRequest) -> StorytellerFuture<'_, ()> {
        let state = self.clone_state();
        Box::pin(async move {
            // The plan answers its refusals BEFORE any frame (v4 plans before
            // `openTurnStream`); the round then runs on its own thread, the
            // send's bridge, and the reply is EMPTY (R-8).
            let (tx, rx) = tokio::sync::oneshot::channel();
            std::thread::spawn(move || {
                let Ok(rt) = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                else {
                    let _ = tx.send(Err(StorytellerDriverError::Core(core_error(
                        ErrorKind::Internal,
                        "spine runtime",
                    ))));
                    return;
                };
                rt.block_on(async move {
                    let planned = rounds::plan_storyteller_run(
                        &state.db,
                        &req.chat_id,
                        &req.round_id,
                        &req.participant_ids,
                        req.order,
                        req.keep_order,
                        &os_draw_source(),
                    )
                    .await;
                    let (round, turns) = match planned {
                        Ok(p) => p,
                        Err(e) => {
                            let _ = tx.send(Err(refusal(e)));
                            return;
                        }
                    };
                    let _ = tx.send(Ok(()));
                    if let Err(e) = state.run_planned_round(&req, round, turns).await {
                        emit_send_error_frame(
                            &state.events,
                            &req.chat_id,
                            &e.message,
                            orchestrator::SubmissionLedger::default(),
                        );
                    }
                });
            });
            rx.await.unwrap_or_else(|_| {
                Err(StorytellerDriverError::Core(core_error(
                    ErrorKind::Internal,
                    "storyteller run thread panicked",
                )))
            })
        })
    }

    fn ask(
        &self,
        chat_id: String,
        round_id: String,
        participant_id: String,
    ) -> StorytellerFuture<'_, StorytellerBid> {
        let state = self.clone_state();
        Box::pin(async move {
            let (tx, rx) = tokio::sync::oneshot::channel();
            std::thread::spawn(move || {
                let Ok(rt) = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                else {
                    return;
                };
                let result = rt.block_on(async {
                    let (round, asking) =
                        rounds::prepare_ask_anyway(&state.db, &chat_id, &round_id, &participant_id)
                            .await
                            .map_err(refusal)?;
                    let outcome = state
                        .run_storyteller_bid(&chat_id, &participant_id, &round.id)
                        .await;
                    rounds::land_bid(&state.db, &round.id, &chat_id, &asking, &outcome)
                        .await
                        .map_err(|e| {
                            StorytellerDriverError::Core(core_error(
                                ErrorKind::Internal,
                                e.to_string(),
                            ))
                        })
                });
                let _ = tx.send(result);
            });
            rx.await.unwrap_or_else(|_| {
                Err(StorytellerDriverError::Core(core_error(
                    ErrorKind::Internal,
                    "storyteller ask thread panicked",
                )))
            })
        })
    }
}

/// The per-call cheap-LLM executor for one storyteller call (a bid or a
/// speaking turn), logging into `llm_logs` with v4's cheap-LLM context (chatId,
/// no messageId) AND the round's [`LogContext`].
///
/// v4 runs the whole round inside `runWithStorytellerRoundId`
/// (`lib/storyteller/round.service.ts:107`, `:391`), and `logLLMCall` stamps
/// `getStorytellerRoundId()` on EVERY row it writes
/// (`lib/services/llm-logging.service.ts:227`) — so a cheap-LLM sub-call made
/// inside a turn (answer confirmation, compression, …) carries the round id
/// too, not only the primary stream's row. v5 has no `AsyncLocalStorage`; the
/// round's context is threaded here explicitly.
pub(crate) fn round_sub_call_executor(
    db: &Db,
    chat_id: &str,
    log_context: &LogContext,
) -> CheapLlmTaskExecutor {
    CheapLlmTaskExecutor::with_logging(CheapLlmLogConfig {
        db: db.clone(),
        user_id: SINGLE_USER_ID.to_string(),
        chat_id: Some(chat_id.to_string()),
        message_id: None,
        ctx: log_context.clone(),
    })
}

/// The vision describer for one storyteller call: the shared runner's shape
/// (`HostImageDescribeRunner`) carrying the round's [`LogContext`], so a
/// `describe_image` inside a round logs its `IMAGE_DESCRIPTION` row with the
/// round's id (v4 `llm-logging.service.ts:227`). `None` when the spine has no
/// describer wired — the turn keeps v4's no-vision-profile answer then.
pub(crate) fn round_image_describe<C>(
    db: &Db,
    completion: &Arc<C>,
    wired: bool,
    log_context: &LogContext,
) -> Option<HostImageDescribeRunner<C>> {
    wired.then(|| HostImageDescribeRunner {
        db: db.clone(),
        user_id: SINGLE_USER_ID.to_string(),
        completion: Arc::clone(completion),
        log_context: log_context.clone(),
    })
}

#[cfg(test)]
mod round_log_context_tests {
    use super::*;
    use quilltap_core::cheap_llm::CheapLlmSelection;
    use quilltap_core::model::completion::{CannedCompletionProvider, CompletionMessage};
    use quilltap_core::services::cheap_llm_exec::CheapLlmTaskOptions;

    const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";

    /// A fresh instance (all three partitions), its `llm_logs` carrying the
    /// boot-ensured `storytellerRoundId` column.
    fn fresh_db(dir: &std::path::Path) -> Db {
        quilltap_core::services::provisioning::provision_fresh_instance(dir, PEPPER)
            .expect("provision");
        {
            let w =
                quilltap_core::db::Writer::open_writable(&dir.join("quilltap-llm-logs.db"), PEPPER)
                    .unwrap();
            quilltap_core::test_support::ensure_llm_logs_storyteller_round_id_on(w.connection());
        }
        Db::open(
            quilltap_core::db::runtime::DbPaths {
                main: dir.join("quilltap.db"),
                mount_index: Some(dir.join("quilltap-mount-index.db")),
                llm_logs: Some(dir.join("quilltap-llm-logs.db")),
            },
            PEPPER,
        )
        .unwrap()
    }

    fn round_ids(db: &Db) -> Vec<Option<String>> {
        db.read_llm_logs(|conn| {
            let mut stmt = conn.prepare("SELECT storytellerRoundId FROM llm_logs")?;
            let out = stmt
                .query_map([], |row| row.get(0))?
                .collect::<Result<Vec<Option<String>>, _>>()?;
            Ok(out)
        })
        .unwrap()
    }

    async fn one_cheap_call(exec: &CheapLlmTaskExecutor) {
        let messages = vec![
            CompletionMessage::system("summarize this"),
            CompletionMessage::user("the transcript"),
        ];
        let provider = CannedCompletionProvider::new().with_response(
            "OLLAMA",
            "llama",
            Some(0.3),
            &messages,
            "a tidy summary",
            None,
        );
        let selection = CheapLlmSelection {
            provider: "OLLAMA".to_string(),
            model_name: "llama".to_string(),
            base_url: None,
            connection_profile_id: None,
            is_local: true,
            profile_parameters: None,
        };
        let r = exec
            .execute(
                &provider,
                &selection,
                messages,
                |s| s.to_string(),
                None,
                None,
                None,
                Some("summarize-chat"),
                CheapLlmTaskOptions::default(),
            )
            .await;
        assert!(r.success, "the canned cheap call answers: {:?}", r.error);
    }

    /// The review's should-fix: a cheap-LLM sub-call made during a storyteller
    /// turn is logged WITH the round's id (v4 `llm-logging.service.ts:227`
    /// stamps `getStorytellerRoundId()` on every row inside the round), and a
    /// call outside a round stays untagged.
    #[tokio::test]
    async fn a_round_sub_call_row_carries_the_round_id() {
        let dir = tempfile::tempdir().unwrap();
        let db = fresh_db(dir.path());

        let round = LogContext {
            storyteller_round_id: Some("round-7".to_string()),
            ..Default::default()
        };
        one_cheap_call(&round_sub_call_executor(&db, "chat-1", &round)).await;
        assert_eq!(round_ids(&db), vec![Some("round-7".to_string())]);

        one_cheap_call(&round_sub_call_executor(&db, "chat-1", &LogContext::none())).await;
        let mut ids = round_ids(&db);
        ids.sort();
        assert_eq!(ids, vec![None, Some("round-7".to_string())]);
    }

    /// The round's vision describer carries the round's context into the
    /// `IMAGE_DESCRIPTION` row (`FallbackDeps::log_context` →
    /// `VisionRequestInput::log_context` → `record`); none is built when the
    /// spine wired no describer.
    #[test]
    fn the_round_describer_carries_the_round_id() {
        let dir = tempfile::tempdir().unwrap();
        let db = fresh_db(dir.path());
        let completion = Arc::new(CannedCompletionProvider::new());
        let round = LogContext {
            storyteller_round_id: Some("round-7".to_string()),
            ..Default::default()
        };
        let runner = round_image_describe(&db, &completion, true, &round).expect("wired");
        assert_eq!(runner.log_context, round);
        assert!(round_image_describe(&db, &completion, false, &round).is_none());
    }
}
