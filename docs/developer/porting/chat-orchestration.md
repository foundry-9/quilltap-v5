# Chat orchestration — the Phase-3 Unit-3 decomposition

> Working plan for porting v4's chat orchestration (the send-message engine, the
> context subsystem, and the stateful turn chain) into `quilltap-core`. Read
> alongside [`phase-3.md`](./phase-3.md) (the phase plan this instantiates),
> [`api-boundary.md`](./api-boundary.md) (the Event-channel + single-writer rules
> it must obey), and [`provider-manifest.md`](./provider-manifest.md) (the
> provider layer it will eventually plug into). Scoped 2026-07-02 from a direct
> survey of the v4 source.

## Where chat orchestration actually lives in v4

The Next.js route `app/api/v1/chats/[id]/messages/route.ts` is a 53-line
transport shim. **The engine is `lib/services/chat-message/`** — 29 files,
~11,700 lines — plus the context subsystem (`lib/chat/context-manager.ts`,
~2,158 lines, and `lib/chat/context/*`), the tool engine
(`lib/chat/tool-executor.ts`, ~1,282 lines), and the stateful turn chain
(`lib/services/chat-message/turn-orchestrator.service.ts`). The
`app/api/v1/chats/[id]/handlers|helpers|actions` files are unrelated CRUD /
side-action code, **not** the send path — do not scope them into this unit.

One full user-message → assistant-response cycle, condensed:

1. `handleSendMessage` (`orchestrator.service.ts:166`) opens the SSE stream and
   calls `processMessage`, then `executeTurnChain` for multi-character chats.
2. `processMessage` (`orchestrator.service.ts:247–1516`) sequences: participant
   resolution → agent-mode/danger/courier resolution → attachment processing →
   **user-message write** → tool build → **context build**
   (`buildContext`, `context-manager.ts:477`) → **primary stream**
   (`runPrimaryStream` → `provider.streamMessage`) → native/text tool loops →
   empty-response failover → **finalize** (`finalizeMessageResponse`: clean the
   text, write the assistant message, compute the next speaker, emit `done`,
   fire the background triggers — memory extraction, context summary, danger
   classification, cost tracking).
3. `executeTurnChain` (`turn-orchestrator.service.ts:284`) loops chained LLM
   turns inside the same stream: `shouldChainNext` re-reads the chat, applies
   the all-LLM pause, pops the turn queue, selects the next speaker, and
   re-enters `processMessage` per turn (guards: depth 20, 300 s wall clock).

Everything streams over SSE as single-key JSON events (`status`, `turnStart`,
`content`, `reasoning`, `carinaAnswer`, `confirmationResult`, `turnComplete`,
`chainComplete`, `pendingExternalTurn`, `done`, `error`, keep-alive comments).
Per [`api-boundary.md`](./api-boundary.md), in v5 this vocabulary becomes typed
`Event` variants on the one push channel; the byte framing is transport.

## Boundary rules this unit must obey (from `api-boundary.md`)

- The whole engine lives **below** `dispatch`; transports only marshal.
  `SendChatMessage` is a `Request`; every streamed artifact is an `Event`
  (`ChatToken`, …), **never** a `Response` payload.
- Visibility filtering (which events a subscriber may see) happens in the core
  before emit, not per-transport.
- A streaming turn's request path never blocks on the writer: token deltas are
  `Event`s, not writes; only committed turn artifacts go through the `Db`
  writer channel (Unit-0 runtime, already live).
- Nothing here may assume an always-on host: the turn chain must stay
  resumable / per-turn-committed so the later enclave `step()` (Phase-3 Unit 4)
  can drive the same machinery.

## What is already ported (do not re-port)

- **Turn manager pure core** (Phase 1): the entire `lib/chat/turn-manager/`
  module except three trivial predicates — state machine, queue ops, weighted
  selection (RNG injected), all-LLM pause, participant filters, predicted turn
  order, and the `computeSpokenThisCycle*` payload builders (the latter already
  wired into `db::chats_messages`).
- **Context pure leaves** (Phase 1): compression sizing (`context_compression`),
  summary cadence (`context_summary`), context-budget arithmetic
  (`context_budget`), token estimation, per-character attribution
  (`message_attribution`), mentioned-characters scan, scenario text, canon
  blocks, `combineScenarioText`, the memory weighting/recall stack.
- **Data layer** (Phase 2): every repo the flow writes through (`chats`,
  `chat_messages` incl. the metadata side-effect, `files` links, `memories`,
  `llm_logs`, `background_jobs`, …).
- **Model boundary** (Phase 3 Units 0.5+): `EmbeddingProvider` and the
  non-streaming `CompletionProvider` + canned responders; `cheap_llm` selection
  + `cheap_llm_exec`; `queue_service` enqueue.
- **Memory family** (Phase 3 Units 1–2): gate, processor, cascade deletes,
  housekeeping, watermark — the things the finalizer's background triggers
  enqueue.

## The decomposition

Leaf-first, pure-to-stateful, one differential per unit — same discipline as
every prior phase. Waves 1–2 are the current work; waves 3+ are scoped but not
scheduled.

### Wave 1 — pure leaves (tier-1 exact, parallelizable)

| # | Unit | v4 source | v5 target | Notes |
|---|---|---|---|---|
| 1.1 | Template processor | `lib/templates/processor.ts` (182) | `templates` | `processTemplate` / `buildTemplateContext` / `processCharacterTemplates` — `{{char}}`/`{{user}}` substitution; prerequisite of the system-prompt builder. Plus the turn-manager pure gap: `isUsersTurn`, `isParticipantsTurn`, `getSelectionExplanation`. |
| 1.2 | Chat timestamps + formatting hint | `lib/chat/timestamp-utils.ts` (409), `lib/chat/template-prompt-hint.ts` (62) | `chat_timestamp`, hint alongside | `resolveTimezone`, `calculateCurrentTimestamp` (clock injected via `clock`), `shouldInjectTimestamp`, `formatTimestampForSystemPrompt`, `initializeFictionalTime`; `generateFormattingPromptHint`. |
| 1.3 | Memory-injector formatters | `lib/chat/context/memory-injector.ts` (640) | `memory_injector` | The seven pure context formatters (`formatMemoryMetadataTag`, scene state, memories, inter-character, frozen archive, dynamic head, summary) + the head-budget constants. |
| 1.4 | Message selector + core-whisper trigger | `lib/chat/context/message-selector.ts` (92), `lib/chat/context/core-whisper-trigger.ts` (180) | `message_selector`, `core_whisper` | The greedy token-budget tail fit; the core-whisper cadence gate. |
| 1.5 | Carina markup parser | `lib/chat/carina-parser.ts` (113) | `carina_parser` | Pure `@Name:` inline-markup parser (the runner service is wave 3). |
| 1.6 | Message formatter + finish reason | `lib/llm/message-formatter.ts` (436), `lib/llm/extract-finish-reason.ts` (36) | `message_formatter`, `finish_reason` | The finalizer's anti-hijack cleanups: `stripCharacterNamePrefix`, `truncateAtForeignSpeaker`, `normalizeContentBlockFormat`; finish-reason extraction. |

Each ships: Rust module + tsx oracle case (drives v4's real functions over a
committed corpus → NDJSON) + a `quilltap-harness` tier-1 exact test.

*Deliberately excluded from wave 1:* the tool-family pure leaves
(`pseudo-tool.service.ts`, `tool-call-threading.ts`, `turn-transcript.ts`,
`rng-pattern-detector.service.ts`). They are pure, but they belong to the tool
subsystem (wave 4) and nothing in waves 1–2 consumes them.

### Wave 2 — first composed / stateful units

| # | Unit | v4 source | v5 target | Verification |
|---|---|---|---|---|
| 2.1 | System-prompt builder | `lib/chat/context/system-prompt-builder.ts` (343) | `system_prompt` | Tier-1; composes 1.1 + 1.2 (identity stack, public identity card, other-participants info, identity reinforcement, `buildSystemPrompt`). |
| 2.2 | Stateful turn orchestration (decision core) | `turn-orchestrator.service.ts:52–248` (`shouldChainNext`, `persistTurnParticipantId`), `actions/turn.ts:31–184` (`handleTurnAction` mutation core) | `services::turn_orchestrator` | tsx real-DB tier-2 differential (no model call): both sides run the same decision/mutation sequence on a seeded fixture; diff `chats` + the returned `ChainDecision`s. The chain *driver* (`executeTurnChain`) is wave 3 — it is model-calling. |
| 2.3 | Streaming model boundary | `streaming.service.ts:312–420` contract (`provider.streamMessage`, the internal `StreamChunk` protocol) | `model::stream` | Self-tests (like `model::completion`): `StreamingCompletionProvider` trait + `CannedStreamingProvider` replaying a recorded chunk sequence keyed by exact call input. The oracle-side injection lands with the primary-stream differential (wave 3), mirroring how `model::completion` waited for the memory processor. |

### Wave 3 — the model-calling services (each its own tier-3 → tier-2 unit)

Ordered by dependency; several are mutually independent once wave 2 lands:

- **Compression service half** — `applyContextCompression`
  (`context/compression.ts:191`) over the ported sizing leaves + `cheap_llm_exec`.
- **Context-summary service half** — `generateContextSummary` /
  `invalidateContextSummaryIfMessageCovered` / `checkAndGenerateSummaryIfNeeded`
  (`context-summary.ts`) — the fold write + title enqueue.
- **Knowledge injector** — `retrieveKnowledgeForTurn` (embedding + repo read).
- **First-message context** — `loadParticipantMemories` / `loadProjectContext` /
  `buildFirstMessageContext`.
- **Participant / user-identity resolvers** —
  `participant-resolver.service.ts`, `user-identity-resolver.service.ts`
  (DB-reading, no model).
- **Primary stream + recovery + failover** — `primary-stream.service.ts`,
  `recovery.service.ts`, `provider-failover.service.ts` over `model::stream`.
- **Finalizer** — `message-finalizer.service.ts` minus tool/carina/confirmation
  branches: clean → write → next speaker → background triggers (the triggers
  reuse `queue_service`).
- **Carina markup runner** — `lib/services/carina/markup-runner.ts` over 1.5.
- **`buildContext` capstone** — `context-manager.ts:477` composing everything
  above (its whisper-posting side effects — core packet, commonplace, mail —
  are separately scoped sub-units; they touch the post-office subsystem).
- **`processMessage` spine + `executeTurnChain`** — the orchestrator itself,
  emitting typed `Event`s; first end-to-end tier-3 differential over a full
  canned turn, diffing `chat_messages` + `chats` + the SSE-event trace against
  v4's recorded stream.

### Wave 4+ — adjacent subsystems (scoped out of Unit 3)

**The batch plan** (the numbering the Status bullets below use; each batch =
one or more self-contained work orders, executed by parallel agents on
disjoint files and unified afterward). **The endgame was re-planned
2026-07-06** from fresh surveys of every remaining v4 subsystem: every
pending batch now has an agent-ready work order in
[`work-orders/`](./work-orders/), and the sequencing below supersedes the
earlier sketch.

| Batch | Scope | Status / work order |
|-------|-------|--------|
| **W4.0** | Wardrobe drift batch: archetype tiers, public READ trio, transfers | ✅ done |
| **W4.1** | The tool subsystem, sub-units a–g: RNG paths (a), pure leaves + all 57 definitions (b), execution/persistence (c), the handler catalog in five batches (d1–d5), the native (e) and text (f) tool loops, `buildTools` + spine wiring (g) | ✅ done — deferrals: the `generate_image` handler (→ W4.9a); the photo trio (→ W4.9b) **now DONE**; provider parse/capabilities + plugin registry (→ W4.7) |
| **W4.2** | Danger orchestration (`dangerous-content/`): resolver, chat-override, manual-flip, gatekeeper (+ the classification job handler), provider-routing (the real `DangerousContentRouter` implementor) | ✅ done — the spine unification is **W4.2u** |
| **W4.2u** | Wire the real danger router/resolver at the orchestrator composition point + the two deferred corpus cases (OFF short-circuit, live uncensored reroute) | ✅ done (2026-07-06) — [`w4.2u-danger-spine-unification.md`](./work-orders/w4.2u-danger-spine-unification.md) |
| **W4.3** ✅ **DONE** (2026-07-06) | Answer-confirmation service (fills the finalizer's `AnswerConfirmationRunner` seam: gate cascade, Commonplace-whisper reference gathering + 24 K truncation, the consistency check + re-affirmation cheap-LLM calls, the uncensored escalation of the check). `services::answer_confirmation` (prompts in a generated `prompt_text` submodule); the finalizer gate leaves hoisted; the real `RealAnswerConfirmation` runner closes the seam. Verified by `answer_confirmation_tier3_equivalence` (14-case jest real-DB oracle driving v4's REAL `finalizeMessageResponse` with the feature ON, canned completions) + 15 self-tests; `message_finalizer_tier3` + `orchestrator_tier3` regenerated green | [`w4.3-answer-confirmation.md`](./work-orders/w4.3-answer-confirmation.md) |
| **W4.4a** | agent-mode resolver ✅ / regenerate-swipe ✅ / compression-cache ✅ / **W4.4a4** ✅ **DONE** (2026-07-07): courier transport + the compression spine plumbing (the finalizer `AsyncCompressionTrigger` + `build_context` cached window) | [`w4.4a4-courier-and-compression-plumbing.md`](./work-orders/w4.4a4-courier-and-compression-plumbing.md) |
| **W4.4b** ✅ **DONE (2026-07-07)** | The file/attachment subsystem: `files::{text_detection, image_processing, attachment_support}` + `services::{file_fallback, chat_files}`. Closes `process_files` (v4 `loadAndProcessFiles`) + the Lantern K seam (now async). Vision description reuses the completion seam (`CompletionParams.attachments` + `CompletionResponse.finish_reason` + `canned_completion_key_with_attachments`, old oracles unchanged); bytes (`FileBytesStore`) + resize (`ImageTranscoder`) are host seams (no image codec in core). Verified: `text_detection_equivalence` (tier-1) + `file_attachment_tier3_equivalence` (jest real-DB) + `orchestrator_tier3`/`message_context_leaves` regenerated green. **Deferred (inherited handoffs, out of checklist):** `model_supports_native_tools` from `pricing_fetcher`; `ConnApiKeys` wiring into danger/cheap/image | [`w4.4b-file-attachment.md`](./work-orders/w4.4b-file-attachment.md) |
| **W4.5** ✅ **DONE** (2026-07-07) | Carina query engine (`services::carina_query` = v4 `runCarinaQuery`) + `services::carina_memory_extraction` + `queue_service::enqueue_carina_memory_extraction`. The `RunCarinaQuery` seam went **async** (RPITIT — frozen = behavior/args not sync-ness; see `[[w4.5-carina-async-seam]]`); `carina_runner_tier3` + `mail_carina_tools` re-verified green (behavior identical, not regenerated). Verified by `carina_query_tier3_equivalence` (13 cases vs v4's REAL `runCarinaQuery`) + `carina_memory_extraction_tier3_equivalence` (SELF-only vs v4's REAL handler). Brahma one-shot console = injected `RunBrahmaConsole` seam (**W4.5b** follow-up; gate + sentinel post ARE ported). **Handed to the spine owner (W4.4b/unification):** the `ask_carina` `BuiltInToolRunner` dispatch row + constructing the real `RunCarinaQuery` at the orchestrator/finalizer composition point + the live `@Name:`/`ask_carina` spine-corpus cases (`orchestrator_tier3`/`message_finalizer_tier3` NOT regenerated this round) | [`w4.5-carina-query.md`](./work-orders/w4.5-carina-query.md) |
| **W4.6a** ✅ **DONE** (2026-07-06) | BuildContext feeder closures: recap + distill (`services::memory_recap`), frozen archive (`services::frozen_archive`), core-whisper config+packet (`services::core_whisper`), Suparṇā mail read (`services::suparna_mail`), off-scene scan (`services::off_scene`), scene-state cache persist + the `updateSceneState` task (`services::scene_state_tracking`), mount pool + recall settings + live clothing (closed with existing code). The trait is shrunk to the W4.6b posts only; verified by `build_context_tier3_equivalence` (mocks dropped one-for-one) + a new `context_feeders_leaves_equivalence` tier-1; `knowledge_injector`/`first_message_context`/`orchestrator_tier3` re-verified. **Deferrals:** the recap/distill stay mocked in the orchestrator oracle (the spine passes `cheap_llm_selection: None` — a spine-owner follow-up); the scene-state JOB WRAPPER is W4.8 | [`w4.6a-context-feeders.md`](./work-orders/w4.6a-context-feeders.md) |
| **W4.6b** ✅ **DONE (2026-07-07)** | The post-office / personified writers: Host, Prospero, Librarian, Concierge, Suparṇā, Aurora (+ the wardrobe announcement drain), Commonplace (+ relevant-conversations refresh), Lantern notification; the vault summary mirror + the `chats.delete` sweep (**the last Phase-2 deferral — closed**); cost events. All six persona modules byte-exact (tier-1 differentials); post-rows proven by `post_office_writers_tier3_equivalence`; the mirror + sweep by `vault_summary_mirror_tier2_equivalence`. Non-spine seams live: Concierge announcers (`danger_gatekeeper_tier3` + manual-flip regenerated), context-summary Librarian re-post + cost events (`context_summary_service_tier3` regenerated). **Handoffs:** the `BuildContextSeams::post_*` + `OrchestratorSeams::post_prospero_context` + the end-of-turn wardrobe-drain wiring into the spine; the context-summary vault-mirror/refresh seams (need vault fixtures + embedding); the Lantern-sink rewire in the image subsystem; the Librarian save-announcement `change:{diff}` coupling in the doc-edit handlers (**all now DONE — see Round-3 unification, Groups 4/6/7**) | [`w4.6b-post-office-writers.md`](./work-orders/w4.6b-post-office-writers.md) |
| **W4.7** | The provider layer, six units a–f — decomposed 2026-07-06 in [`provider-manifest.md`](./provider-manifest.md) ("The porting decomposition"): manifest+registry (a), **the five stream decoders (b ✅ 2026-07-06** — `model::decoders`, `stream_decoders_equivalence`), request builders/transforms/tool-wire (c — closes `ToolCallDetector` + the text-markers strategy + `formatTools`), transport/errors/`api_keys` (d — closes every `ApiKeyResolver` seam), pricing/capability/`logLLMCall`/embeddings (e), image dialects + moderation + web search (f) | [`w4.7a-manifest-registry.md`](./work-orders/w4.7a-manifest-registry.md), [`w4.7b-stream-decoders.md`](./work-orders/w4.7b-stream-decoders.md); c–f specced in the decomposition |
| **W4.8** ✅ **DONE** (2026-07-06) | The background job runner: claim loop + dispatch registry + completion/backoff + recovery, the enqueue-helper remainder, scheduler decision leaves (host-driven cadence), the stale-chat maintenance sweep + `getLastPlayedMessageAt`; closed `ensureProcessorRunning` + the `markCompleted` result-merge deferral. The fork/IPC/buffered-proxy architecture deliberately did NOT port — in-process handlers over the single-writer `Db`. `services::job_runner` + `job_scheduler` + `maintenance`; verified by `photos_relative_path_equivalence` (tier-1) + `maintenance_sweep_tier2_equivalence` (tsx real-DB) + 11 runner self-tests; `memory_watermark_tier3`/`context_summary_service_tier3` regenerated green | [`w4.8-job-runner.md`](./work-orders/w4.8-job-runner.md) |
| **W4.9** | **a: ✅ DONE (2026-07-06)** — the `generate_image` subsystem (the `model::image` seam, the three scene cheap-LLM tasks, appearance resolution + the five-step Concierge sanitize gate, `saveGeneratedImage`, the avatar trigger [closes the W4.1d2 deferral], dispatched via the erased `ImageGenerationRunner`; `image_generation_tier3_equivalence`); **b: ✅ DONE (2026-07-06)** — the photo trio (`keep_image`/`list_images`/`attach_image` + save-to-album + kept-image markdown, `photo_tools_tier3_equivalence`); **c: ✅ DONE (2026-07-07)** — the avatar + story-background JOB HANDLERS (`services::{character_avatar_job, story_background_job}`): the two scene tasks (`deriveSceneContext` / `craftStoryBackgroundPrompt`), the aesthetics module (`resolveAesthetic` tiered + `resolveDepictionGuidelines` Ariel Clause), `buildCharacterAvatarPrompt` (the `6b6e39ad` bare-top branch + the `describeOutfit` omit variant), the two storage bridges (character vault `images/history/` + Lantern `generated/`), `enqueueStoryBackgroundGeneration` + `resolveImageProfileForChat` + the `queueStoryBackgroundIfEnabled` gate (TITLE_UPDATE wiring point documented), and both `JobHandler`s (removed from the loud fallback); `avatar_job_tier3_equivalence` + `story_background_job_tier3_equivalence`. Aesthetics differ per handler (avatar aurora-only; story lantern+aurora+Ariel); storage branch keys differ (avatar `chat.projectId`, story `payload.projectId`); `logLLMCall` deferred (generate_image precedent); the project-store `uploadFile` branch is the injected host FsSeam; `generate_image`'s aesthetic slots stay `None` (STOP-noted — the handlers are the primary consumers) | [`w4.9a-image-generation.md`](./work-orders/w4.9a-image-generation.md), [`w4.9b-photo-trio.md`](./work-orders/w4.9b-photo-trio.md), [`w4.9c-avatar-story-background-handlers.md`](./work-orders/w4.9c-avatar-story-background-handlers.md) |
| **Unit 4** | The enclave engine — `step()`/`RunState`, budget/milestones, cron, lifecycle, schedule tick ([`enclave-engine.md`](./enclave-engine.md)) | [`u4-enclave-engine.md`](./work-orders/u4-enclave-engine.md) — after W4.8 + W4.6b |

**Execution rounds (parallelism guidance).** Two contended resources: the
spine source files (`orchestrator.rs` / `message_finalizer.rs` /
`message_context.rs` / `build_context.rs`) and the **orchestrator oracle
corpus** (`harness/oracle/{cases,fixtures}/orchestrator-tier3*`). Per round,
exactly ONE order owns each (the W4.2∥W4.4a coordination precedent); any
other concurrent order that wants a spine-corpus case hands it to the
round's unification pass instead of editing the corpus itself. Start each
round with a drift check against v4 HEAD.

- **Round 1:** **W4.d1 ✅ DONE** (the unified-diff drift re-port, v4
  `8617ce7a` — [`w4.d1-unified-diff-drift.md`](./work-orders/w4.d1-unified-diff-drift.md);
  landed 2026-07-06: `doc_edit::line_diff` + rewritten `doc_edit::unified_diff`,
  `doc_edit_leaves_equivalence` regenerated + extended, doc-edit-leaves oracle
  green again) ∥ W4.2u
  (owns spine + orchestrator corpus; small) ∥ W4.8 (disjoint) ∥ W4.9b
  (disjoint — but shares the `doc_edit` module tree with W4.d1's
  re-verification; land W4.d1 before W4.9b's unification) ∥ **W4.7a ✅ DONE**
  (the provider manifest + registry core, landed 2026-07-06:
  `quilltap-core::provider_manifest` — schema + nine generated built-in manifests
  + `Registry` accessors + `rewrite_localhost_url`; `provider_registry_equivalence`
  green [253 rows]; the four registry-seam replacements closed in their leaf
  consumers [`message_formatter` now registry-backed name-field — a real behavior
  change; `model_context`/`cheap_model`/`tool_build` oracles regenerated];
  **spine-side seam removals deferred to the orchestrator-spine owner**) ∥ W4.7b
  (disjoint, mutually independent — implements against W4.7a's decoder enums).
- **Round 2:** W4.3 (owns finalizer + orchestrator corpus) ∥ W4.6a
  (build_context + its own feeder differential) ∥ W4.9a (tools + new
  services; its finalizer/orchestrator regenerations go to unification).
- **Round 3:** W4.4a4 (owns spine + orchestrator corpus) ∥ W4.6b (writers;
  its many regenerations listed in the order go to unification where they
  collide) ∥ W4.7c. **Unification Phase B (spine seam wiring): ALL groups DONE —
  Round 3 complete.** Groups 1–5 done
  — G1 W4.7c tool reshape/detector/strategy (orchestrator_tier3 regenerated with
  the real provider registry on the v4 side), G2 W4.6b whisper writers live
  (`RealBuildContextSeams` + the Prospero cadence block; build_context_tier3 +
  orchestrator_tier3 regenerated with writers un-mocked), G3 end-of-turn wardrobe
  drain, G4 Lantern sink → byte-exact writer (image_generation_tier3), G5
  commonplace dedup. **Groups 6, 7 & 8 now DONE + green — Round 3 complete.** G8
  `cheap_llm_selection` spine threading (the spine resolves the real selection →
  buildContext recap/distill feeders + finalizer async-compression fire;
  orchestrator_tier3 regenerated dropping the recap/distill mocks — the distill fires
  61 live cheap-LLM calls, no stream-key cascade since the corpus keeps
  compression off + memories empty). G7 context-summary vault-mirror +
  relevant-conversations-refresh LIVE (two-DB fixture: provisioned vault + pre-seeded
  prior summary with a canned unit embedding; mirror BEFORE refresh; the mirror diffed
  by the `doc_mount_file_links` path set, the refresh by the `chat_messages` whisper;
  `vault_summary_mirror_tier2` kept green). **G6 (Librarian doc-save `change:{diff}`
  coupling) DONE:** the five mutating doc-edit write handlers (`doc_write_file` /
  `doc_str_replace` / `doc_insert_text` / `doc_update_frontmatter` /
  `doc_update_heading`) build the `change:{created,body}/{edited,diff}` payload
  inside the synchronous `execute_doc_edit_tool` `Db::write` closure and stash it in
  a new `#[serde(skip)]` `pending_librarian_announcement` field on
  `DocEditToolResult` (the wardrobe-drain `pending*` precedent); the executor spine
  posts it via the already-ported `post_librarian_write_announcement` after the
  closure returns (best-effort). Ported `resolveActorOrigin`. `doc_text` regenerated
  with the write announcement LIVE (un-mocked `postLibrarianWriteAnnouncement` +
  `contentHiddenFromCharacters`, the existing fixture chat+participant targeted) and
  a third dumped table — the MAIN-db `chat_messages` — diffing the 10 Librarian rows
  byte-exact (`post_librarian_write_announcement_conn` posts them on the direct-drive
  Rust side). `doc_fm`/`doc_ui`/`doc_blob`/`doc_enum`/`tool_dispatch` re-verified
  green (the additive field is `None` for every non-write handler). The
  file-management / blob / open announcements remain separate seams (out of G6
  scope).
- **Round 4** (all orders written 2026-07-07 from fresh v4 surveys at
  `6b6e39ad`): W4.4b (owns spine + orchestrator corpus) ∥ W4.5 (carina —
  its finalizer/orchestrator cases go to unification) ∥
  [W4.7d](./work-orders/w4.7d-transport-errors-api-keys.md) ∥
  [W4.7e](./work-orders/w4.7e-pricing-capability-logging-embeddings.md) ∥
  [W4.7f](./work-orders/w4.7f-image-dialects-moderation-search.md) (d and
  e are mutually independent; f's two api-key lookups depend on d's
  `db::api_keys` — its round note keeps them canned and hands the closure
  to unification when run in parallel) ∥
  [W4.9c](./work-orders/w4.9c-avatar-story-background-handlers.md) (the
  avatar + story-background job handlers — disjoint from the spine;
  benefits from e's logLLMCall and f's dialects but requires neither, both
  are seams) ∥
  [W4.6c](./work-orders/w4.6c-librarian-file-announcements.md) (**DONE**
  2026-07-07 — the remaining Librarian doc-edit announcements: move / copy /
  delete / folder-created / folder-deleted / open / blob-write, threaded via
  the generalized `PendingLibrarianAnnouncement` enum; `doc_fm` / `doc_blob` /
  `doc_ui` regenerated green with the writers live. It touched only the
  executor's post-dispatch block in `tools/executor.rs`, so W4.5's `ask_carina`
  dispatch row is a clean, non-conflicting merge). **W4.7d / W4.7e (sub-units
  1–4) / W4.7f / W4.9c / W4.6c are all DONE and unified on main (2026-07-07,
  the Round-4 unification commit `62e818f`)** — one real cross-branch conflict
  fixed there (W4.9c adapted to W4.7f's `GeneratedImageData` widening) and all
  eleven differentials re-verified against fresh oracles.
- **Round 4 remainder (launch all four IN PARALLEL — layout decided
  2026-07-07, v4 still at `6b6e39ad`):**
  [W4.4b](./work-orders/w4.4b-file-attachment.md) (owns the spine + the orchestrator
  oracle corpus; carries the two inherited handoffs —
  `model_supports_native_tools` sourcing + the ApiKeyResolver spine wiring —
  and now wires `IMAGE_DESCRIPTION` logging through the real
  `services::llm_logging`, W4.7e having landed) ∥
  [W4.5](./work-orders/w4.5-carina-query.md) (carina — finalizer/orchestrator corpus
  cases go to unification; the old W4.5∥W4.6c `tools/executor.rs` contention
  is RETIRED, W4.6c landed) ∥
  [W4.7e2](./work-orders/w4.7e2-builtin-tfidf-vectorizer.md) (the BUILTIN
  TF-IDF/BM25 vectorizer — fully disjoint) ∥
  [W4.7e3](./work-orders/w4.7e3-logllmcall-call-sites.md) (the logLLMCall
  call-site closures + oracle regens — touches five non-spine service files
  + the two W4.9c handlers; if its answer-confirmation regen collides with
  W4.4b's finalizer work, that ONE regen goes to unification). Pairwise:
  e2 conflicts with nothing; e3 and W4.5 are file-disjoint; W4.4b vs e3 is
  covered by the spine-ownership rule.
  **All four are DONE and unified on main (2026-07-08, `68a039d`) —
  conflict-free at the source level; the ownership rules held.**
- **The wiring round (launch all three IN PARALLEL — layout decided
  2026-07-08, v4 still at `6b6e39ad`):**
  [W4.10a](./work-orders/w4.10a-spine-wiring.md) (the spine wiring pass —
  owns the four spine files + `tools/executor.rs` + `tools/ask_carina.rs` +
  the orchestrator corpus/oracle; closes the three deferred
  composition-point seams: `model_supports_native_tools` sourced in-spine
  from the real `check_model_supports_tools`, the real DB-backed
  ApiKeyResolver at the danger router, and the real `RunCarinaQuery` +
  the `ask_carina` dispatch row + the live `@Name:`/`ask_carina` corpus
  cases; its `executor.rs` changes must stay ADDITIVE) ∥
  [W4.5b](./work-orders/w4.5b-brahma-console.md) (the Brahma one-shot
  console — a new `services::brahma_console` composing already-ported
  units; implements the frozen `RunBrahmaConsole` trait; constructs
  `BuiltInToolRunner` via the existing API, touches no W4.10a-owned file;
  the spine/carina swap-in of the real console is a unification one-liner)
  ∥ [W4.10b](./work-orders/w4.10b-logging-regens.md) (the staged W4.7e3
  `llm_logs` oracle regenerations, steps 1–7 — the seven non-spine
  differentials + fixes only where a genuine port divergence surfaces;
  `orchestrator_tier3` explicitly NOT regenerated). Pairwise: W4.5b and
  W4.10b are file-disjoint; W4.10a vs each is covered by the ownership
  lists in the orders. **Deliberately post-round:** the spine
  `with_logging` wiring + an orchestrator `llm_logs` dump (couples
  W4.10a's corpus to W4.10b's primary-stream regen — a unification /
  follow-up step once both land). **All three are DONE and unified on main
  (2026-07-08) — zero source conflicts again; the gate (898 tests) + an
  eighteen-differential sweep against fresh oracles ran green; the W4.5b
  spine swap-in landed at unification (inert — a live Brahma corpus case is
  a follow-up alongside the live ask_carina-through-spine case).**
- **The cleanup round (launch all three IN PARALLEL — layout decided
  2026-07-08, v4 still at `6b6e39ad`):** the four standing follow-ups
  become three lanes.
  [W4.11a](./work-orders/w4.11a-spine-logging-and-owned-providers.md) (the
  spine lane — owns the four spine files + the orchestrator oracle/corpus;
  Arc blanket impls on the provider traits so a composition point can share
  one provider between the borrowed spine deps and the owned erased seams;
  the live `ask_carina`-through-spine + live-Brahma corpus cases; the
  `with_logging` composition + the orchestrator `llm_logs` dump with the
  CHAT_MESSAGE rows filtered both sides as a documented mock artifact —
  W4.11b proves that row shape directly) ∥
  [W4.11b](./work-orders/w4.11b-primary-stream-log-regen.md) (the W4.7e3
  step-6 regen — relocate the primary-stream oracle's model mock below the
  `streamMessage` wrapper, un-mock `logLLMCall`, dump `llm_logs` with the
  requestHashes asserted; ALSO fixes the real gap the survey found: v4's
  provider-failover retries log CHAT_MESSAGE rows and the ported
  `provider_failover.rs` doesn't) ∥
  [W4.11c](./work-orders/w4.11c-moderation-logging-seam.md) (small — widen
  the moderation seam so the wire's per-category `flagged` reaches the
  gatekeeper, write v4's `modelName:'moderation'` row byte-exact, drop the
  `strip_moderation` filter on both sides). Pairwise file-disjoint per the
  ownership lists; `tests/common/mod.rs` additive-only for all three.
  This round is also the enclave's enabler: U4.4's per-run token accounting
  sums real `llm_logs` rows, which the `with_logging` spine composition
  makes live.
  **W4.11a status (2026-07-08): the Arc/logging half is DONE (Arc blanket
  impls + tests; the `ask_carina` seam wired into the spine; `with_logging` +
  the orchestrator `llm_logs` dump green — cheap-LLM rows byte-exact,
  CHAT_MESSAGE + DANGER_CLASSIFICATION seam-filtered; the harness's erased
  ask_carina + a live `RealBrahmaConsole` over Arc-shared providers built +
  inert-verified). The two LIVE corpus cases are DEFERRED with real blockers:
  the ask_carina tool-call case needs v4's tool-path `carinaAnswer` emit
  matched, which requires threading the per-turn sink through
  `ToolExecutionContext` (`services/tool_execution.rs` — outside this lane's
  ownership, a W4.1c-deferral feature); the live-Brahma case needs a global
  default connection profile + api key that ripples through the 23 existing
  cases' profile/cheap-LLM resolution. A follow-up (own `tool_execution.rs` +
  the fixture default-profile work) closes both — the Arc/seam infrastructure
  they need is now in place.**
  **All three lanes are DONE and unified on main (2026-07-08) — zero source
  conflicts for the third consecutive round; the gate (903 tests) + a
  thirteen-differential sweep against fresh oracles at `6b6e39ad` ran green.
  Every pre-enclave follow-up is closed or precisely narrowed (the two live
  corpus cases + the spine failover-log threading remain, blockers named in
  the W4.11a status above and CLAUDE.md).**
- **Round 5:** Unit 4 (enclave), then the Phase-4 kickoff. The enclave order
  is refreshed and ready (`work-orders/u4-enclave-engine.md`).

The markdown-renderer / `qtap-linkify` item below is Phase-4-adjacent and not
in a numbered batch.

- **Server-side markdown rendering** — `markdown-renderer.service.ts` (the
  pre-rendered-HTML path for Librarian/Lantern/Aurora announcement bodies) +
  its pure helpers (`roleplay-rendering`, and the new
  `lib/chat/qtap-linkify.ts` from v4 `52eb0eb8`, 2026-07-02 — bare `qtap://`
  URI → markdown-link upgrade, shared by client and server renderers so they
  stay in lockstep). Porting note: `qtap-linkify`'s `BARE_QTAP_URI_RE` uses
  **lookbehind** (`(?<!\]\()(?<!<)`), which the Rust `regex` crate does not
  support — reproduce it the way the Phase-1 name matchers did (hand-rolled
  boundary check or `fancy-regex`-free rewrite), with a differential corpus.
  W4.6b writers that would embed pre-rendered HTML seam the renderer call
  until this lands.

## Sequencing rationale

- Wave 1 is decision-free: every unit is a pure function with an unambiguous
  v4 oracle, so six agents can port them in parallel on disjoint files (the
  `lib.rs` / harness wiring is pre-staged serially, as in the Phase-2 batches).
- Wave 2 needs wave 1 only for 2.1; 2.2 and 2.3 are independent of it and of
  each other. 2.2 is the first *service* here and reuses the tsx real-DB
  differential shape proven by the memory-deletion units.
- Wave 3 exists so that no single differential has to pin more than one new
  seam at a time — the same reason the memory family landed gate → processor →
  watermark rather than all at once.
- SSE events become `Event` variants only when the emitting service lands
  (per phase-3.md: enums grow one variant per service; no speculative
  enumeration).

## Status

- **W4.4a4: the Courier transport + the compression-cache spine plumbing — DONE**
  (2026-07-07). `services::courier_transport` + `courier::render_markdown` port v4's
  `courier-transport.service.ts`: the two byte-exact Markdown renderers,
  `buildCourierDeltaEvents` (checkpoint scan, `<=` boundary, whisper filter both
  directions, Staff labels, attachment loading), `dispatchCourierTransport` (the
  placeholder + bundle + pause + `pendingExternalTurn`/`done` frames), and the
  paste/cancel resolvers (public service fns; the HTTP route is Phase-4). The
  orchestrator courier gate is closed (dispatch after `build_message_context` + the
  `preparing` status, tool build skipped). Compression: the finalizer's real async
  `AsyncCompressionTrigger` over `compression_cache::trigger_async_compression`, and
  the `build_context` cached-compression window (`cached_compression_result` /
  `cached_compression_message_count` — warm cache used verbatim + the dynamic
  window). New `courier_transport_tier3_equivalence` (green); `orchestrator_tier3`
  (a `courier_send` case), `build_context_tier3` (a warm-cache case),
  `message_finalizer_tier3`, `compression_cache_tier3` regenerated green. Tracked
  deferral: the spine's `cheap_llm_selection` threading (shared with W4.6a) keeps
  the cached-compression read inert in `process_message`; the paste/cancel route
  handlers (Phase-4 HTTP transport).

- **W4.1g: `buildTools` + the tool-slate spine wiring — DONE; W4.1 is CLOSED**
  (2026-07-04). `services::tool_build` ports v4's `buildTools` + the built-in half
  of `buildToolsForProvider`: the flag→tool-set construction over the b.3 catalog,
  `is_tool_disabled` (built-in tools carry empty source metadata so group patterns
  never bite them), the `allowToolUse === false` / `disabledTools === undefined`
  short-circuits (the latter returns EMPTY), `apply_image_constraints_to_tool`
  (pure), and the canonical universal shape (v4's `getProvider===null` fallback).
  `checkModelSupportsTools` + `provider.supportsWebSearch` are injected
  registry-seam inputs (the `getModelContextLimit` precedent); the plugin registry,
  the provider `formatTools` reshape, and image constraint enrichment are W4.7
  deferrals. Added `plugin_config::find_by_user_id` (read for faithfulness, unused
  for the built-in slate). The orchestrator flag region is ported
  (`canDress`/`canCreate`/`helpTools`/`documentEditing`, the `self_inventory`
  transparent strip, the overlay-free `askCarina` probe, the autonomous
  destructive-tool filter, `resolvedToolMode`/`useTextBlockTools`/`actualTools` +
  the mode-switched `toolInstructions` + simple-json `initialStopSequences`), and
  the spine seams are closed: the real slate flows to the primary stream, the
  native loop (real `BuiltInToolRunner` + the injected W4.7 `ToolCallDetector`),
  and the text passes' `continuationTools`; the finalizer + tool-only terminal get
  the real tool messages/images. Verified by `tool_build_equivalence` (27
  flag-matrix cases, byte-exact slate + both flags) and the rebuilt
  `orchestrator_tier3` (18 cases running the REAL `buildTools`; a per-call
  tools-at-wire assertion proves the slate reaches the provider — banking the
  `self_inventory` strip [transparent vs not], the `ask_carina` transparency probe,
  `disabled_tools` filtering, and `textblock_mode` empty slate).
  `native_tool_loop`/`text_tool_loop`/`message_finalizer`/`primary_stream`
  re-verified green. **Deferrals:** W4.7 (plugin registry / provider reshape /
  image constraints); a native tool CALL end-to-end through the spine is proven in
  composition (tools-at-wire + `native_tool_loop_tier3`), the detector/`raw_response`
  plumbing wired but corpus-dormant.

- **Wave 1: ported and green** (2026-07-02) — six tier-1 units, each with a
  fresh-oracle exact differential: `templates_equivalence` (53 rows, incl. the
  two-pass `{{trim}}` quirk) + `turn_predicates_equivalence` (35),
  `chat_timestamp_equivalence` (75 — `jiff` added for the IANA offset lookup,
  proven byte-exact against `Intl` incl. both US DST boundaries and the CUSTOM
  sequential-replace bug ported faithfully) + `template_prompt_hint_equivalence`
  (16), `memory_injector_equivalence` (69 — one tracked note: debug floats at
  the tier-1 1e-12 tolerance, a 1-ULP `Math.pow` libm divergence that never
  survives `toFixed(2)`), `message_selector_equivalence` (29) +
  `core_whisper_equivalence` (39), `carina_parser_equivalence` (67),
  `message_formatter_equivalence` (112) + `finish_reason_equivalence` (38).
- **Wave 2: ported and green** (2026-07-02) — `system_prompt_equivalence`
  (54 rows, tier-1, composing templates + chat_timestamp; the
  character-field-`{{timestamp}}`-empties quirk banked),
  `turn_orchestrator_tier2_equivalence` (13 ops over a two-DB seeded fixture,
  zero normalization, RNG/clock injected; added `ChatUpdate.turn_queue` +
  `last_turn_participant_id` setters), and `model::stream` (the streaming
  tier-3 seam: `StreamChunk` / `StreamingCompletionProvider` /
  `CannedStreamingProvider` over a tokio `mpsc::Receiver`, mid-stream failures
  first-class; oracle-side injection lands with the wave-3 primary-stream
  differential, mirroring `model::completion`'s path).
- **Wave 3 batch 1: ported and green** (2026-07-02) — the seven
  mutually-independent sub-units, six parallel agents on disjoint files (the
  shared `ChatUpdate` setters + `services/mod.rs` module set pre-staged
  serially): the **compression service half** (`services::compression`,
  `compression_tier3_equivalence` — result-object tier-3, no DB writes,
  completions pinned by recorded canned keys); the **context-summary service
  half** (`services::context_summary`,
  `context_summary_service_tier3_equivalence` — 11 ops diffing `chats` /
  `chat_messages` / `background_jobs`; `queue_service` gained
  `enqueue_title_update`; the Librarian re-post / vault mirror /
  relevant-conversations refresh / cost events are a default-no-op
  `ContextSummarySeams` trait matching the oracle mocks — tracked deferrals;
  the prior-generation Librarian-whisper sweep IS ported); the **knowledge
  injector + first-message context** (`knowledge_injector_equivalence` +
  `first_message_context_equivalence` — read-only, zero normalization;
  `search_document_chunks` + `search_memories_semantic` ported as the read
  legs; `recallContext` re-rank deferred); the **participant + user-identity
  resolvers** (`participant_resolver_tier2_equivalence` +
  `user_identity_resolver_equivalence` — tsx real-DB; the inherited roleplay
  template persisted via the new `ChatUpdate.roleplay_template_id` setter;
  API-key acquisition host-side); the **primary stream + recovery + failover**
  (`primary_stream_tier3_equivalence` — the first typed `Event` vocabulary
  `services::chat_events` + `EventSink`; `save_assistant_message` as the
  persistence primitive the finalizer will reuse; the `streamMessage` seam
  mocked rule-match+record → `CannedStreamingProvider` replay; event trace +
  two table dumps + results diffed over 12 calls); and the **carina markup
  runner** (`carina_runner_tier3_equivalence` — `runCarinaQuery` established
  as an injected seam per the STOP rule: it drags in the wave-4 tool loop /
  character resolver / commonplace writer / Brahma console; the
  `postCarinaResponse` message writer ported byte-exact).
- **Wave 3 batch 2: ported and green** (2026-07-02) — the **message
  finalizer** (`services::message_finalizer`,
  `message_finalizer_tier3_equivalence` — the core clean → write → next
  speaker → done → background-triggers path; the tool/confirmation branches
  are wave-4 seams with their gates reproduced and banked;
  `save_assistant_message` / `chat_events` / `queue_service` / `db::files`
  extended additively, the primary-stream differential re-verified) and the
  **`buildContext` capstone** (`services::build_context`,
  `build_context_tier3_equivalence` — the full context assembler over the
  ported subsystem, seven ops diffed byte-for-byte; unported feeders + all
  whisper-posting side effects behind a `BuildContextSeams` trait mirrored by
  the oracle mocks; per-unit deferrals listed in each module doc and the
  CLAUDE.md status).
- **Wave 3 capstone: ported and green** (2026-07-02) — the **`processMessage`
  spine + `executeTurnChain`** (`services::orchestrator`,
  `orchestrator_tier3_equivalence`): the composition of every landed wave-1..3
  service into the full send cycle, verified by the first end-to-end tier-3
  differential (six cases; ordered event trace + three table dumps; the
  finalizer's summary-check deferral closed here; unported subsystems as
  `OrchestratorSeams` with gates banked). Two open items it discovered:
  v4's **`buildMessageContext` wrapper** (`context-builder.service.ts`) — now
  **PORTED (2026-07-03; see below)** — and a **flagged chain-depth divergence**
  — now **INVESTIGATED and RESOLVED (2026-07-03)**.
- **`buildMessageContext` wrapper: ported and green** (2026-07-03) —
  `services::message_context` (v4 `context-builder.service.ts`), the wrapper
  between `processMessage` and `buildContext`. Ported leaf-first: the three
  pure leaves (`build_conversation_messages` — type/role filter +
  `assistantAfter` reverse pass + TOOL-result render/`>3`-turn elision;
  `normalize_whisper_roles` — Staff re-role, opaque-body swap, attachment
  exemption; `collect_lantern_image_file_ids_for_character` — the own-turn-stop
  walk + history cutoff + dedup) carry a dedicated tier-1 differential
  (`message_context_leaves_equivalence`, 12 cases driving v4's REAL exported
  leaves). The composition (`build_message_context`) runs the A–D whisper
  pre-filters (commonplace strip + `relevant-conversations` exception,
  TOOL-whisper target filtering, opaque-anywhere over the LLM participants'
  `systemTransparency`, whisper re-role), `buildConversationMessages`, the
  `buildContext` call, then `formatMessagesForProvider` (multi-character),
  the Lantern image merge, the trailing-prefix injection, and the
  multi-character scene block (Anthropic system-instruction route vs the
  non-Anthropic `[Name]` prefill). The K file-loading half
  (`loadChatFilesForLLM` + fallback) is the injected `MessageContextSeams`
  (default no-op) — the corpus keeps attachments empty; the pure
  id-collection leaf IS exercised. Wired into the orchestrator spine where the
  direct `build_context` call sat (the wrapper's `formattedMessages` feed the
  stream). Verified by REBUILDING the orchestrator oracle to drive v4's REAL
  `buildMessageContext` (dropping the passthrough mock; mocking ONLY the K
  file-loader, mirroring the Rust seam) and extending the corpus with five
  cases banking: the non-Anthropic scene-block prefill route
  (`nonanthropic_scene`, an OPENROUTER profile), commonplace stripping + the
  `relevant-conversations` survival exception (`commonplace_strip`), the opaque
  body swap (`opaque_swap` — the persona-free `opaqueContent` reaches the LLM)
  vs the transparent no-swap (`transparent_no_swap`), and the operator-only
  TOOL-whisper filter (`tool_whisper_filter`). All eight pre-existing cases now
  run the REAL wrapper (every corpus chat is multi-character —
  `isMultiCharacterChat` is true for ≥1 LLM participant — so the scene block +
  `formatMessagesForProvider` apply throughout). **Tracked deferral:** the K
  file subsystem (`loadAndProcessFiles` / `loadChatFilesForLLM` /
  `processFileAttachmentFallback`) is wave-4; the Lantern prefix injection (L,
  with a non-empty prefix) lands when that seam is closed.
- **Chain-depth divergence: resolved — an oracle-harness artifact, not a v5
  bug** (2026-07-03). The flag: a non-continue single-LLM-char + user chat
  where v4 chained the sole LLM character to `max_depth` (20 turns) while the
  Rust spine stopped at `user_turn`. Root cause: the differential's oracle
  **froze `Date.now()`**, so every minted message got an identical `createdAt`;
  v4's `getMessages` sorts `ORDER BY createdAt ASC`, and under the all-equal tie
  the non-continue USER row (which carries the user participant id) could sort
  *after* the assistant replies, flipping
  `calculateTurnStateFromHistory`'s `lastSpeakerId` to the user → the turn
  manager's cycle-wrap (step 3, which ignores `spokenThisCycle`) re-picks the
  sole LLM character every turn → `max_depth`. The Rust side stamps `createdAt`
  from a **real monotonic clock** (distinct, increasing), so the latest
  ASSISTANT always sorts last → `lastSpeakerId` is the LLM char → correctly
  stops at `user_turn`. Proven decisively: making the v4 oracle clock **tick
  (+1ms/read)** so `createdAt` is distinct makes v4 *also* stop at `user_turn`.
  The ported `should_chain_next` / `select_next_speaker` /
  `calculate_turn_state_from_history` are byte-faithful; **v5 is correct**. Fix
  + pinning: the orchestrator oracle clock now **advances 1ms per read** (real
  timestamps are distinct; the +Nms drift is invisible — no case injects a
  prompt timestamp and every minted DB timestamp is normalized), the
  differential now diffs `spokenThisCycleParticipantIds` / `turnQueue` /
  `lastTurnParticipantId` **exactly** (previously placeholdered), and two
  dedicated chain-depth cases were added: `noncontinue_single_user_chain`
  (single LLM + user, non-continue → `user_turn`, `lastTurnParticipantId` =
  the user participant) and `noncontinue_two_llm_maxdepth` (two LLM, no user,
  non-continue → genuine ping-pong to `max_depth`, since the `__user__`
  sentinel disables the all-LLM pause). The job-payload `turnOpenerMessageId` /
  `extractionAnchorMessageId` are now remapped through the shared message idmap
  (a non-continue send mints a fresh turn-opener). See
  `[[chain-depth-frozen-clock-artifact]]`.
- Wave 4: **W4.0 (the wardrobe drift batch) is done** — the public READ trio
  (`db::wardrobe_read`), the General/project shared-archetype tier
  (`db::archetype_wardrobe` + `db::instance_settings` + the read-overlay
  archetype-seeding generalization), the public WRITE generalization to
  General/project tiers, and `services::wardrobe_transfers` (move/copy across
  tiers), each with a differential (`wardrobe_public_read_equivalence`,
  re-verified `vault_wardrobe_read`/`vault_wardrobe_public`,
  `wardrobe_transfers_tier2_equivalence`).
- Wave 4: **W4.1a (the RNG subsystem) is done** — the first sub-unit of the tool
  subsystem. The pure detector (`rng_patterns`), the executor with an injected
  byte-stream RNG (`tools::rng`), and the orchestrator `user_message_rng` seam
  closure, each with a differential (`rng_patterns_equivalence`,
  `rng_executor_equivalence`, and three new cases in
  `orchestrator_tier3_equivalence`). Explicitly out of scope (later W4.1
  sub-units): the finalizer's response-RNG, the tool loops, `buildTools`, and the
  `rng` tool's appearance in any tool slate. The rest of wave 4 (the remaining
  tool subsystem, danger, answer-confirmation, courier/agent-mode/
  compression-cache/regenerate-swipe, carina query, buildContext seam-closers,
  provider manifest) is scoped above, not started.
- Wave 4: **W4.1b (the tool-subsystem pure leaves) is done** — the pure
  foundations the loops (W4.1e/f), executor (W4.1c), and handler catalog (W4.1d)
  will consume, all tier-1 exact. Three sub-units:
  - **b.2** tool-call threading (`services::tool_call_threading`):
    `build_assistant_tool_call_message` / `build_tool_result_messages`, the
    callId pairing rule (`tool_call_threading_equivalence`, 22 cases).
  - **b.1** the pseudo-tool machinery (`tools::{simple_json_parser,
    text_block_parser, simple_json_prompt, text_block_prompt, native_tool_prompt,
    pseudo_tool_support}` + `services::pseudo_tool`): the three-tier simple-json
    parser, the text-block parser/converter, both prompt builders, native-tool
    prompt, mode resolution, and the service wrappers. The two backreference
    regexes are hand-rolled (the `regex` crate has no backreferences); the
    `jsonrepair` tier is a **bounded hand-rolled subset** (single/smart quotes,
    unquoted keys, trailing commas) resolving conservatively (tier-fail → `[]`)
    outside its documented scope, corpus-pinned on both sides. Differentials:
    `pseudo_tool_parsers_equivalence` (138 cases), `pseudo_tool_prompts_equivalence`
    (40 cases). The `log*ToolUsage` wrappers are logging-only → not ported;
    `checkModelSupportsTools` (async pricing lookup) is the host-side boundary,
    its boolean the injected input.
  - **b.3** the tool-definition catalog (`tools::definitions`): all **57**
    definitions from the **56** `*-tool.ts` files (search-scriptorium-tool exports
    both `searchScriptorium` and `searchScriptoriumBrahma`; `ALL_TOOLS` matches
    the directory exactly, no omission/extra). Stored as **byte-exact static
    JSON** transcribed from v4's `JSON.stringify` output — NOT by re-implementing
    the Zod→JSON-Schema emitter — generated by the checked-in
    `harness/oracle/tools/gen-tool-catalog.mjs`. The differential
    (`tool_definitions_equivalence`) proves the serde preserve-order round-trip
    reproduces JS `JSON.stringify`, catalog completeness, and a
    `canonicalize_universal_tools` spot-check over the full real catalog. The
    `.default()`-in-`required` quirk (e.g. rng `rolls`, askCarina `whisper`) is
    preserved verbatim.
  - Out of scope (later W4.1 sub-units): the tool loops,
    `processToolCalls`/`saveToolMessages`, `buildTools` slate construction, the
    handlers themselves, and provider wire parsing.
  - **Documented seams:** the `jsonrepair` bounded subset (out-of-subset →
    conservative `[]`), and the out-of-subset text-block/YAML-style parse
    behavior — both corpus-pinned, never silently a different parse.
- Wave 4: **W4.1c (tool execution + persistence primitives) is done**
  (`services::tool_execution`, v4 `tool-execution.service.ts`) — the execution
  harness + the TOOL-row writer between the loops (W4.1e/f) and the handlers
  (W4.1d). Three sub-units:
  - **c.1** `save_tool_messages` + `compute_tool_message_targets` +
    `files::add_tag`: the TOOL-row persistence primitive (through
    `chats_messages::add_message`, so a TOOL row bumps minted
    `lastMessageAt`/`updatedAt` but is excluded from `messageCount` and never
    touches `spokenThisCycle`), the whisper gate (ALWAYS_PRIVATE + VAULT_READ-vs-
    `allowCrossCharacterVaultReads`, **whispered to the user participant** — not
    the calling character, derived from `computeToolMessageTargets`), the generic
    content JSON in v4 field order, and the generated-image link+tag loop. Tier-2
    differential (`tool_execution_tier2_equivalence`) over the whisper matrix,
    content omission, the batch + `firstToolMessageId`, and the image link+tag.
  - **c.2** `process_tool_calls` + the injected `ToolRunner` boundary
    (v4 `executeToolCallWithContext`, W4.1d) + `ToolExecutionContext`: the
    dispatch harness (detection frame → per-tool status → dispatch → image
    extraction → tool-result frame; failures in-band). `chat_events` gained the
    additive `toolsDetected` + `toolResult` frames. Tier-3 differential
    (`tool_execution_process_tier3_equivalence`) driving v4's real
    `processToolCalls` with only the executor mocked.
  - **c.3** spine wiring: the finalizer's `toolMessages.length > 0` gate (inside
    `save_assistant_message`, before the assistant image-link loop) and the
    orchestrator tool-only terminal branch. Caught + fixed the finalizer done
    frame's `toolsExecuted` (was hardcoded `false`). Both branches corpus-dormant
    until the loops (W4.1e/f); the finalizer branch is directly driven against v4
    by the new `tool-save` case; the orchestrator branch's constituents are each
    differential-proven.
  - The canonical `ToolMessage` now lives once in `services::tool_execution`;
    `tool_call_threading` reuses it (matching v4's single `chat-message/types.ts`).
  - Out of scope (later W4.1 sub-units): the executor dispatch + handlers (W4.1d),
    the tool loops (W4.1e/f), `buildTools`, provider wire parsing (W4.7).

- **W4.1d batch 1: ported and green** (2026-07-04) — the first tool-handler
  batch + the real dispatching runner. Nine tools ported (`tools::` family), each
  with a differential driving v4's REAL handler byte-exact: `read_conversation`,
  `upsert_annotation`/`delete_annotation` (`scriptorium_tools_equivalence`);
  `terminal_read`/`terminal_list` (`terminal_tools_equivalence`; scrollback is an
  injected seam); `whisper` (`whisper_tool_equivalence`; STOP-rule checked — one
  `chat_messages` row, no post-office); `help_settings`/`help_navigate`/
  `submit_final_response` (`help_tools_equivalence`; ported the full
  `chat_settings::find_by_user_id` read marshaling); `self_inventory`
  (`self_inventory_equivalence`; the ten-section report over ~a dozen repo readers
  + `build_system_prompt`, host-env bits as an injected `SelfInventoryEnv` seam).
  New leaves: `scriptorium`, `terminal_clean`, `folder_utils`,
  `qtap_uri::format_scoped_uri`. `LoadedMemoriesContext` typed (its consumer
  `self_inventory` landed).
  - **The dispatching runner** (`tools::executor::BuiltInToolRunner`): v4
    `executeToolCallWithContext`'s built-in dispatch rows (the `{ formattedText,
    … }` result shape + failure mapping + the dispatcher-side guards + annotation
    character-name resolution), with an **injected inner `ToolRunner` fallback**
    for unported names — the loud default reproduces v4's `Unknown tool: <name>`
    for names v4 doesn't know and a "recognized but not yet available" failure for
    a not-yet-ported built-in; batches 2–5 extend the ported set without touching
    callers. End-to-end differential (`tool_dispatch_equivalence`) drives v4's
    REAL `executeToolCallWithContext` over a mixed batch (read / two writes / pure
    tool / handler failure / invalid input); the unknown-tool loud fallback is
    unit-tested. `tool_execution_*` + `message_finalizer` + `orchestrator`
    differentials re-verified green.
  - **Deferrals:** plugin-vs-built-in routing precedence (the plugin registry is
    unported — the dispatch is exact for a no-plugin instance, which is why the
    unknown-tool oracle case is unit-tested, not driven — v4's unknown path routes
    through the registry); `self_inventory`'s `quilltap.releaseNotes`/`.changelog`
    file reads (env seam supports them, corpus requests only `.version`); the
    `terminal_read` scrollback source; `read_conversation`'s deprecated
    `findByCharacterIdRaw`.
  - Remaining handler-catalog batches: 3b (doc-edit, ~26), 5 (host-seamed), and
    `search_web` (W4.1d5, external service). **Batch 4 (embedding/search) DONE.**
    Then `buildTools`/spine wiring (W4.1g).

- **W4.1d batch 2: the seven wardrobe tool handlers — ported and green**
  (2026-07-04). `tools::{wardrobe_list, wardrobe_read, wardrobe_create,
  wardrobe_update, wardrobe_archive, wardrobe_wear, wardrobe_take_off}` compose the
  already-ported vault-public CRUD, the public read trio + shared-archetype tier
  (W4.0), and the equipped-outfit ops over new pure leaves (`crate::wardrobe`:
  `unionTypes`/`describeOutfit`/`expandComposites`/the equip primitives/
  `describeWardrobeEffect`) + the DB helpers (`tools::wardrobe_shared`:
  across-tier resolution, the persisted equip primitives,
  `resolveEquippedOutfitForCharacter`, coverage summary,
  `resolveProjectMountPointIdsForChat`) + `find_by_ids_for_character`. The seven
  `BuiltInToolRunner` dispatch rows each run inside one `Db::write` closure holding
  the main + mount-index connections. `pendingWardrobeAnnouncements` became
  `Arc<Mutex<HashSet>>` (interior mutability through the immutable `ToolRunner::run`
  boundary — no trait-signature change); the announcement drain + avatar generation
  (image seam) stay deferrals. Differential `wardrobe_tools_equivalence` (25 ops,
  success/invalid/edge per handler + read-back, minted values positionally
  normalized) drives v4's REAL handlers; `tool_dispatch` gained a `wardrobe_list`
  call; `tool_execution_*` re-verified green.

- **W4.1d batch 3b: the doc-edit tool handlers — ported, green, and dispatched
  (except the photo trio)** (2026-07-04). The foundation
  (`db::database_store` primitives composing the ported storage leaves +
  `doc_mount_folders`/`doc_mount_file_links` finders + `link_blob_content`) +
  `tools::doc_edit` (the `shared` access-control family / resolution-context
  builders / `applyQtapUri` / `isTextFile` / DB read-write dispatch, and all 23
  non-photo `doc_*` handlers across five groups — text/markdown, file-management,
  document-UI, blob, enumeration — behind a v4-faithful `executeDocEditTool`
  dispatcher). Wired into `BuiltInToolRunner` (`run_doc_edit`, both connections).
  The Librarian-announcement + reindex layers are documented no-op seams; the fs/
  obsidian/general mount branches the FsSeam. Verified by five jest-real-DB
  differentials (`doc_text` 26 / `doc_fm` 20 / `doc_ui` 9 / `doc_blob` 11 /
  `doc_enum` 14 ops) + the extended `tool_dispatch_equivalence`. ~~**The photo
  group (`keep_image`/`list_images`/`attach_image`) is a tracked scoped
  deferral**~~ — **now DONE in W4.9b (2026-07-06)**: ported over the new
  `crate::photos` module (`keep-image-markdown` + `save-image-to-album` behind an
  injected `FileBytesStore` bytes seam; the chunker stays unported with `chunkCount`
  pinned), dispatched, and verified by `photo_tools_tier3_equivalence` +1
  `tool_dispatch` `list_images` row.

- **W4.1e: the native tool loop + the finalizer response-RNG — ported and green**
  (2026-07-04). `services::native_tool_loop` ports v4's `runNativeToolLoop`
  (iterate → detect → execute → thread → re-stream, the agent-mode
  submit/ghost-wrap/truncation/force-final branches) over two injected seams — a
  `ToolCallDetector` (v4 `detectToolCallsInResponse`; the provider wire parse is
  **W4.7**) and the frozen `ToolRunner` (W4.1d) — plus the partial
  `services::agent_mode` (the pure helpers the loop consumes; the resolver cascade
  `resolveAgentModeSetting` + `buildAgentModeInstructions` is **W4.4**). Wired into
  the orchestrator spine at v4's composition point (corpus-dormant: the tool slate
  is empty until `buildTools` [**W4.1g**]). e.2 closed the finalizer's `RngDetector`
  seam — the ported detector + executor run inline on the assistant response
  (`auto-detect-response` TOOL content with a UTF-16 `anchorOffset`), only the
  CSPRNG byte source injected. Differentials: `native_tool_loop_tier3_equivalence`
  (7 case families, three-boundary mock split) + the extended
  `message_finalizer_tier3_equivalence` (RNG fire + no-fire); `orchestrator_tier3`
  re-verified. **Deferrals:** the text loop (W4.1f), `buildTools`/real-slate wiring
  (W4.1g), the agent-mode resolver (W4.4), detection wire-parse (W4.7).

- **W4.4a part 1: the agent-mode resolver — ported and green** (2026-07-05).
  `services::agent_mode` gained `resolveAgentModeSetting` (the Global → Character →
  Project → Chat cascade), `DEFAULT_AGENT_MODE_SETTINGS`, and
  `buildAgentModeInstructions`, closing the orchestrator's agent-mode seam. The
  spine reads `project.defaultAgentModeEnabled` (a store-managed field → the
  overlaid `projects.findById`), resolves the cascade, fires the `agentTurnCount:
  0` reset on a non-continue turn, feeds `enabled` to `buildTools`
  (`submit_final_response`), injects the byte-exact agent-mode system-prompt block
  into `formattedMessages`, and passes the resolved `ResolvedAgentMode` to the
  native loop. Verified through `orchestrator_tier3_equivalence` (a new
  `agent_mode_on` case: chat-level opt-in, custom `maxTurns: 15`, banking the
  instruction injection + the wire slate + the seeded `5 → 0` reset) + resolver
  unit tests. The W4.1e deferral of the resolver is closed.

- **W4.4a part 2: regenerate-swipe — ported and green** (2026-07-05).
  `services::regenerate_swipe` ports v4's `regenerateMessageAsSwipe` (the sibling
  entry point to `process_message`): responder/identity resolution +
  `build_message_context` (continue-mode, context strictly before the target) +
  the `CompletionProvider` seam (a single non-streaming generation) + swipe-group
  bookkeeping + the ported `delete_memories_by_source_message_with_vectors`
  cascade. The orchestrator's `build_context_input` / `BuildContextArgs` were made
  reusable (scalar clock/limit fields). `regenerate_swipe_tier3_equivalence`
  (four cases: first regen, existing group, KEEP_MEMORIES, not-assistant throw)
  drives v4's REAL repo over a two-DB fixture, diffing five tables (the canned
  completion key proves the rebuilt prompt). Deferral: swipe raw/reasoning/thought
  fields null (cheap-LLM `CompletionResponse` subset; W4.7).

- **W4.4a part 3: the compression cache service — ported and green** (2026-07-05).
  `services::compression_cache` ports v4's `triggerAsyncCompression` /
  `getCachedCompression` / `invalidateCompressionCache` (+ `hashString` /
  `isCacheValid` / the persist/load/clear DB layer over the `chats.compressionCache`
  column; added the `ChatUpdate.compression_cache` setter). `withPersistLock` +
  the in-flight-promise state are not ported (the single-writer serializes; the
  trigger computes synchronously). `compression_cache_tier3_equivalence` (five ops:
  trigger→persist, guard, get-DB-hit, get-miss, invalidate) drives v4's REAL
  functions. **Remaining plumbing (deferral):** the finalizer's real
  `AsyncCompressionTrigger` impl (thread the trigger inputs through
  `CompressionContext`) + the `buildContext` cached-compression window
  (`cachedCompressionResult`/`cachedCompressionMessageCount` inputs, computed by
  the spine) — additive; the differentials keep the recording/empty-cache seams.

- **W4.1d batch 3a (part 1): the tiered mount pool + the `qtap://` URI codec —
  ported and green** (the first half of the doc-edit foundation the ~26 `doc_*`
  handlers of batch 3b sit on). `db::tiered_mount_pool` is the canonical home of
  `dedupeTierTriple` (hoisted out of the knowledge injector, which now consumes it
  from there — differential re-verified) plus the ported `resolve_tiered_mount_pool`
  / `classify_mount_tier` / `flatten_tier_pool`: the five-tier
  character/participant/group/project/global resolution with the ownership gate,
  the character-mount fast path, the per-RESPONDING-character group tier, graceful
  global-null, per-tier error swallowing, and the character>group>project>global
  dedup (the resolver takes both a main + mount-index `&Connection`). Verified by
  `tiered_mount_pool_equivalence` (9-case read-differential vs v4's REAL
  `resolveTieredMountPool` over a two-DB fixture; zero normalization). The full
  `qtap://` codec `doc_edit::qtap_uri` (parse/format/producers) is ported +
  **unified** with the previously-hoisted producers (re-exported from the canonical
  home; `self_inventory` + RAG stay green): V8-faithful
  `encodeURIComponent`/`decodeURIComponent`, the last-`:` fragment split, BAD_LEVEL
  bounds, encoded-slash segments, insertion-ordered query. Verified by
  `qtap_uri_equivalence` (54-row tier-1). Added scoped mount-point reads
  (`doc_mount_points::{find_by_id_for_docedit, find_enabled_for_docedit,
  count_by_name}`, `groups::find_official_mount_point_id_raw`).

- **W4.1d batch 3a (part 2): the doc-edit pure leaves — ported and green.**
  `doc_edit::{diacritics, mime_registry, unified_diff, markdown_parser}` (v4
  `lib/doc-edit/{diacritics, mime-registry, unified-diff, markdown-parser}.ts`),
  verified by one grouped tier-1 differential (`doc_edit_leaves_equivalence`, 81
  rows): NFD diacritics matching (`unicode-normalization`; UTF-16 index/length
  remap), the MIME registry (detect/predicates/parse/serialize/validate — the V8
  `JSON.parse` message a documented normalized seam), the hand-rolled unified diff,
  and the markdown heading ops + `serializeFrontmatter`/`updateFrontmatterInContent`
  (reusing the ported eemeli scalar emitter for byte-exact `YAML.stringify` over the
  frontmatter value space). `document-policy.ts` needed no new port.

- **W4.1d batch 3a (part 3): the path resolver + URI producers — ported and green,
  completing batch 3a.** `doc_edit::{path_resolver, uri_producers}` (v4
  `lib/doc-edit/{path-resolver, uri-producers}.ts`), verified by a 23-case
  read-differential (`doc_edit_path_resolver_equivalence`) vs v4's REAL
  `resolveDocEditPath` + `docStoreUriFor`/`uriForResolvedPath`/
  `buildDocStoreUriResolver`. The `document_store` scope resolves over the tiered
  mount pool (SELF token, name-vs-id matching, ambiguity/not-found/disabled errors,
  traversal/absolute/missing-path guards); the `project` scope aliases the official
  mount — byte-exact `PathResolutionError` codes + messages. The legacy on-disk
  branches (FS-backed mounts, the project `<filesDir>` fallback, the `general` scope)
  are a **host-filesystem seam** (`ResolveError::FsSeam`) deferred to Phase-4; the
  corpus is all database-backed so it is never hit. Added
  `projects::find_official_mount_point_id_raw`. **The doc-edit foundation (batch 3a)
  is complete; the ~26 `doc_*` handlers (batch 3b) follow.**

- **W4.1f: the text-tool loop — ported and green** (2026-07-04).
  `services::text_tool_loop` ports v4's `runTextToolPass` — the strategy-driven
  detect-text-markers → execute → re-stream-continuation pass after the native
  loop, reading text markers OUT OF the streamed prose (vs the native loop's raw
  response object). The engine is **strategy-agnostic** behind a `TextToolStrategy`
  trait; ships `SimpleJsonStrategy` + `TextBlockStrategy` (b.1 leaves) and takes a
  provider-text-markers strategy as an **injected seam** (the provider plugin
  detector/parser/stripper is **W4.7**; default `None`). Reproduces the entry gate,
  the duplicate-cap nudge (byte-exact synthetic user message, enforced by the
  continuation canned-key match), the iteration cap, the un-stripped-assistant-turn
  ledger (markers kept — stripping broke simple-json continuations), the
  DISPLAY-ONLY flat reasoning on the continuation, the
  usage/cacheUsage/rawResponse/thoughtSignature **overwrite-on-done**, the
  preserve-partial + rethrow on continuation failure, and
  `assembleStrippedWithOffsets` (strip once/segment, drop whitespace-only with
  offset carry, UTF-16 `\n\n`-join anchor math). Wired into the orchestrator spine
  after the native loop: provider pass gated on the injected `provider_text_strategy`
  seam, then simple-json vs the text-block fall-through per an injected
  `resolved_tool_mode` (defaults `TextBlock` — v4's else-branch); corpus-dormant,
  `orchestrator_tier3` re-verified. `text_tool_loop_tier3_equivalence` (9 case
  families, **DB-free** — the pass writes nothing): real simple-json/text-block +
  a synthetic `<<T:name:args>>` strategy identical in TS + Rust for the mechanics
  (multi-iteration, nudge, parse-empty no-op, mid-stream failure, cap,
  empty-segment surrogate-pair assembly, stopSequences forwarding). **Deferrals:**
  the provider-text-markers strategy (W4.7), the real tool-mode/tool-slate plumbing
  (W4.1g).

- **W4.1d batch 4: the four search/introspection tool handlers — ported and green**
  (2026-07-04). `tools::{search, project_info, help_search, request_full_context}`,
  each byte-exact vs v4's REAL handler + wired into `BuiltInToolRunner` (contiguous
  `// W4.1d4: search tools` block). `search` (v4 `search-scriptorium-handler`) is the
  unified multi-source search — memories (`search_memories_semantic`), conversations
  (new `db::conversation_search`, sibling of `document_search`, NOT merged),
  documents (`document_search`), knowledge (the same narrowed per-tier to
  `Knowledge/`) — reproducing the per-source error-swallowing, tier-ordered dedup
  (character>group>project>global, knowledge wins), `qtap://` URI tagging, the
  operator/Brahma surface, 500-char truncation, and exact result strings; serves
  both tool definitions. `project_info` (over the new pure leaf
  `db::project_store_naming::pick_primary_project_store`). `help_search` (new
  `db::help_search`: semantic + keyword fallback on embedding failure via
  `extract_search_terms`; the `ensureHelpDocsSynced` disk sync is a host seam).
  `request_full_context` (a self-contained single-column `UPDATE` = v4's
  no-`updatedAt`-bump `chats.update`, so **no `db/chats.rs` change**). The dispatcher
  gained an injectable `ErasedEmbeddingProvider` (default never-succeeds) so
  `search`/`help_search` reach the embedding seam without a second generic on the
  shared struct; real provider wires W4.1g. `search_tools_equivalence` — 24 cases,
  two jest real-DB oracles (only `generateEmbeddingForUser` canned, `Date.now()`
  frozen), per-case fresh two-DB fixture copy (search bumps `lastAccessedAt`;
  request_full_context writes), serialized-JSON-string + `format*` compare
  (float-safe) + the full `chats` row for request_full_context.
  `knowledge_injector`/`first_message_context`/`tool_execution_process_tier3`
  re-verified green. **The tool-handler catalog now has only batch 3b (doc-edit) +
  batch 5 (host-seamed) + `search_web` (W4.1d5, external service) outstanding.**

- **W4.2: the dangerous-content ("Concierge") orchestration subsystem — ported
  and green** (2026-07-05). `services::dangerous_content` (v4
  `lib/services/dangerous-content/` + the `CHAT_DANGER_CLASSIFICATION` job
  runner), replacing the injected `DangerousContentRouter` stub with the real
  resolution. Leaf-to-root: `chat_override` (the two-field danger status),
  `resolver` (`resolveDangerousContentSettings`), `gatekeeper` (classification —
  the `ModerationProvider` seam collapsing v4's plugin registry + key auto-detect
  + `moderate`, the port still running `mapModerationResult`; the cheap-LLM
  classify over the `CompletionProvider` seam with the byte-exact
  `CLASSIFICATION_SYSTEM_PROMPT`; `parseClassificationResponse`; the LRU cache),
  `provider_routing` (the REAL implementor of the FROZEN
  `provider_failover::DangerousContentRouter` — the trait shape holds; the
  `ApiKeyResolver` seam keeps key material host-side per the `cheap_llm_exec`
  precedent), `manual_flip` (`applyConciergeFlip` via a raw chat `UPDATE`, since
  the frozen `ChatUpdate` is W4.4a-owned), and `gatekeeper_job`
  (`handleChatDangerClassification` — the bails, the classify input, the system
  event + token aggregate on the LLM path, the danger-field persistence). Added
  additive `connection_profiles`/`image_profiles` net reads. Three differentials
  green: `danger_resolver_equivalence` (pure matrix + tier-2 manual-flip),
  `danger_routing_equivalence` (the reroute matrix, canned api-key seam both
  sides), `danger_gatekeeper_tier3_equivalence` (drives v4's REAL job runner,
  both model boundaries canned). Seams: the moderation plugin registry, the
  cheap-LLM / routing API keys, `logLLMCall`, the job runner, and the Concierge
  personified-announcement writers (**W4.6**). **Unification handoff (W4.4a-owned
  spine files):** construct the real router + gatekeeper at the orchestrator
  composition point + add the OFF-short-circuit and live-uncensored-reroute
  orchestrator-corpus cases.
