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

- **Tool subsystem** — `tool-executor.ts`, `tool-execution.service.ts`, the
  native/text tool loops, pseudo-tool instruction builders, RNG tools.
- **Provider manifest + the five stream decoders** ([`provider-manifest.md`](./provider-manifest.md))
  — replaces the canned provider with real wire decoding.
- **Danger orchestration, agent mode, answer-confirmation service, courier
  transport, compression-cache, regenerate/swipe** — each an independent
  follow-on with the same tier-3 shape.
- **Autonomous room / enclave engine** — Phase-3 Unit 4 (`step()` + `RunState`),
  drives the same turn machinery with `userParticipantId = null`.
- **Prospero / Librarian / post-office whisper writers** — cross-subsystem
  side-effect posts reached from `processMessage` and `buildContext`.
- **Server-side markdown rendering** — `markdown-renderer.service.ts` (the
  pre-rendered-HTML path for Librarian/Lantern/Aurora announcement bodies) +
  its pure helpers (`roleplay-rendering`, and the new
  `lib/chat/qtap-linkify.ts` from v4 `52eb0eb8`, 2026-07-02 — bare `qtap://`
  URI → markdown-link upgrade, shared by client and server renderers so they
  stay in lockstep). Porting note: `qtap-linkify`'s `BARE_QTAP_URI_RE` uses
  **lookbehind** (`(?<!\]\()(?<!<)`), which the Rust `regex` crate does not
  support — reproduce it the way the Phase-1 name matchers did (hand-rolled
  boundary check or `fancy-regex`-free rewrite), with a differential corpus.
- **Answer-confirmation service** (expanded from the line above; v4
  `29f3ae63`): `answer-confirmation.service.ts` — `isUserDrivenTurn` /
  `hasCheckableInputs` / `findLatestCommonplaceWhisper` + the active
  consistency-check/re-affirmation cheap-LLM calls. The finalizer already
  ports its gates, `confirmed`-state persistence, and the
  `confirmationResult` event; this unit fills the seam. Depends on the tool
  subsystem (it vets tool-using replies) and the Commonplace whisper shape.

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
  - Remaining handler-catalog batches: 3 (doc-edit, ~20),
    4 (embedding/search), 5 (host-seamed). Then the text loop (W4.1f) +
    `buildTools`/spine wiring (W4.1g).

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
