# Survey C — Pascal custom tools, the Workbench, the Salon turn spine, interrupts, Brahma/Carina

Read-only survey, v5 `main` @ `4e43b714e` (clean), v4 read at the pin `322a5488e` via `git show`.
Orders covered: P4.D304–D312, D314, D319, D320, D329, D332–D336, plus the `25b8fab6a` round's recorded
follow-ups (phase-4.md ~7769-7776) and the `322a5488e` round's review notes assigned to this slice.

**Counts:**
- **OPEN: 17.**
  - 13 firm: §A (the R-4 lane), F-1, F-2, F-3, F-4, F-6, I-1, I-2, I-4, I-5, N-1 (NEW), P4.D306 item 18, and
    P4.D334's `_meta.baseline`.
  - 4 optional: P4.D309's cast, P4.D309 item 21, P4.D310 item 14, P4.D314 item 11.
- **CLOSED: 1** from the follow-up lists (F-5, the enclave consult). Roughly 20 Tier-3 rows were also closed at
  the unions; they are marked in §D.
- **NEEDS-RULING: 9.**
  - 3 rulings: I-3, P4.D305 T3-23, P4.D310 item 13.
  - 6 v4 filing candidates.
- **NO-ACTION:** the rest of the Tier-3 rows (recorded classes and NO-PORTs, tabled in §D).

---

## §A — ⚖ RULED (A): converge on v4 — Stop ends Carina's Brahma one-shot stream (overturns P4.D335 R-4)   [OPEN — full lane, M]

### What v4 does @322a5488e (exact path)

1. **The ALS store.** `lib/chat/turn-control.ts:47-56`: a `processSingleton('chat.turnControl')` holding
   `byChat: Map<chatId, Set<TurnControlEntry>>` plus `als: new AsyncLocalStorage<TurnControlEntry>()`. Each
   entry carries `controller: new AbortController()`. `runWithTurnControl(chatId, fn)` (`:62-93`) registers the
   entry, then `state.als.run(entry, fn)`, so everything the Salon send awaits inherits the entry. That
   includes the finalizer, the Carina markup runner, the `ask_carina` tool, `runBrahmaQuery` and the one-shot loop.
2. **Where it is read.** `currentTurnStopSignal()` (`turn-control.ts:101-103`) returns
   `state.als.getStore()?.controller.signal`. Its ONE stream reader is the funnel:
   `lib/services/chat-message/streaming.service.ts:458`, `abortSignal: currentTurnStopSignal()`, passed into
   `withStallWatchdog(provider.streamMessage(…), {…, logContext: {context:'streaming.service', userId, chatId,
   characterId, messageId}})`.
3. **How Brahma reaches the funnel.** Carina's Brahma branch runs as follows. `carina.service.ts:353-421`
   `answerAsBrahma` calls `runBrahmaQuery({repos,userId,chatId,question})`. That is
   `brahma-console/one-shot.service.ts:69-189`, which passes **no `signal`** to the loop (`:174-187`). The loop
   is `agent-loop/one-shot-loop.ts:150-401` `runOneShotToolLoop`. At `:219` it calls
   `for await (const chunk of streamMessage({…}))`, and `streamMessage` is the funnel above (import at `:32`).
   The stop therefore arrives ONLY through the ambient signal. The loop's own `signal` stays `undefined`.
   - Entrances inside a turn (both inherit the ALS):
     - the finalizer's markup path, `message-finalizer.service.ts:470-490` `runCarinaMarkupQuery`, which is
       guarded `cleanedResponse && !turnStopped` at `:478`, so it only STARTS on an unstopped turn;
     - the `ask_carina` tool inside the tool loops.
   - The user-message Carina path is also inside the ALS in v4. v5 has never wired it: the standing
     `user_message_carina` deferral, `services/orchestrator.rs:48, 864, 2043, 2093`.
4. **What happens to the stream.** `lib/llm/stream-watchdog.ts:89-178`:
   - `stopRequested` resolves at once on an already-aborted signal (`:107-112`).
   - Each `next()` is raced `Promise.race([withTimeout(next), stopRequested])` (`:125`), with STOP winning a
     buffered chunk.
   - On STOPPED it logs ONE INFO and `return`s quietly (`:145-154`):
     `logger.info('[LLMStream] Abandoned a provider stream at the operator\'s stop', {...logContext, provider,
     modelName, chunksReceived, elapsedMs})`. The bag is `context:'streaming.service'`, `userId`, `chatId`;
     `characterId` and `messageId` are undefined and drop out.
   - `iterator.return()` is `void`ed, not awaited (`:168-171`).
5. **What is persisted, logged and emitted** for a stop landing mid-Brahma:
   - **llm_logs:** NO row for the stopped leg. `streamMessage` calls `logLLMCall` only when `chunk.done` passes
     through it (`streaming.service.ts:487-528`), and the watchdog returned before any `done`.
   - **The one-shot loop** (no `signal`) treats the partial text as the turn's response.
     - With no tool calls, `fullResponse = partial` and `break` (`:357-359`). The `Brahma one-shot: finished`
       DEBUG follows, and it returns `{ok:true, answer: partial.trim()}`.
     - If nothing had streamed (stop before the first chunk), `finalAnswer === ''`. It returns
       `{ok:false, detail:'empty response'}` with DEBUG `Brahma one-shot produced an empty answer` (`:389-392`).
     - **Quirk to preserve:** a stop landing BETWEEN tool turns makes the next `streamMessage` yield zero chunks,
       because the signal is already aborted (one more INFO, `chunksReceived: 0`). `currentResponse` is then
       `''`, but `lastToolResultText` is set, so the **bug-47 salvage** fires. It logs WARN
       `Brahma one-shot exhausted its turn budget without a final response` and returns `{ok:true, answer:
       "I reached my N-turn budget …\n\nHere is what I gathered before I stopped:\n\n<last tool result>"}`
       (`:374-386`).
   - **Carina:**
     - `ok:true` posts the Brahma answer as a `systemSender:'carina'` row with `answererId:
       BRAHMA_CARINA_ANSWERER_ID` (`carina.service.ts:369-377`), emits the `carinaAnswer` frame via `onPosted`
       (`:383-394`), and logs INFO `[Carina] Brahma Console answered {context, chatId, whisper, answerLength}`
       (`:398-403`).
     - `ok:false` is routed to Prospero as `llm-failed` with `detail:'empty response'` (`:364-368`).
   - **Turn control:** `stopActiveTurns` waits on `entry.settled`. The stopped Brahma stream unwinds at once, so
     the stop answers `{stopped:1, settled:true, paused:…}` with no 15 s wait.

### What v5 does now (main @ HEAD)

- **The registry.** `crates/quilltap-core/src/services/turn_control.rs` holds `TurnControlHandle` (`:98`),
  `StopSignal` (`:102-122`, `wait()` resolving at once when already stopped) and `register_active_turn`
  (`:211-237`). `stop_active_turns` is at `:297` (INFO `:323`, the `did not settle in time` WARN `:342`), and
  `api/chat_interrupt.rs:34` `chat_stop_turn` calls it.
- **The watchdog.** `model/stream_watchdog.rs:210-237` defines `watch_stream` and `watch_stream_with_stop`.
  `recv` (`:256-330`) has the stop-first race and the INFO at `quilltap::llm_stream`. The module doc
  (`:52-58`) states explicitly that the one-shot loop and Brahma "keep [`watch_stream`]".
- **The Brahma path, in call order:**
  - `services/carina_query.rs`:
    - `RunBrahmaConsole::run(&self, user_id, chat_id, question)` (`:115-122`) — **no stop parameter**;
    - `CarinaQueryDeps.turn_control: Option<&TurnControlHandle>` already present (`:185-189`, used by Carina's
      own stream at `:1294-1307`);
    - `answer_as_brahma` (`:822-838`) calls `deps.brahma.run(&opts.user_id, &opts.chat_id, &opts.question)`
      and drops the handle.
  - `services/brahma_console/mod.rs`:
    - `run_brahma_query(deps, user_id, chat_id, question)` (`:172`) builds `RunOneShotToolLoopOptions {…,
      signal: None, …}` (`:328-345`);
    - `RealBrahmaConsole` (`:383-426`) carries no stop.
  - `services/agent_loop/one_shot_loop.rs`:
    - `RunOneShotToolLoopOptions` (`:148-182`) has no stop field;
    - `run_stream` (`:299-312`) wraps with plain `watch_stream(` at `:346-356`;
    - the llm-log gate `if error.is_none() && (!aborted || saw_done)` (`:441`) WOULD log a stopped leg (no
      `done`, not `aborted`) once a stop is wired. **This must change** (see the design);
    - the call site is `:585-599`.
- **Composition.**
  - `crates/quilltap-host/src/spine.rs:1529-1535` builds the Carina-markup `brahma`, which feeds
    `RealCarinaQuery { … turn_control: Some(&turn_handle) }` at `:1557-1569`.
  - `:1542-1548` builds the `ask_carina` tool's `brahma` inside `TypedAskCarina { … turn_control:
    Some(turn_handle.clone()) }` (`tools/ask_carina.rs:307-345`).
  - `:2090-2109` is the enclave `run_step`, which is unregistered (`turn_control: None`) and stays unchanged.
- **Net effect:** Stop during a Brahma answer is not heard. The one-shot stream runs to its natural end, and
  `chat_stop_turn` may wait the full 15 s `DEFAULT_SETTLE_TIMEOUT` and answer `settled:false` with the WARN.

### R-4 pins / docs that must flip

- `crates/quilltap-harness/tests/stream_watchdog_wrap_census.rs`:
  - the `CENSUS` row `services/agent_loop/one_shot_loop.rs` (`:118-127`) goes `(1, 1, 0)` → `(1, 0, 1)`, and
    its why-string should cite v4 `streaming.service.ts:458` inherited via ALS;
  - add the file to `STOP_SITES` (`:175-182`), making EIGHT;
  - correct the module doc `:38-45` ("the five others … Brahma") to say the one-shot loop takes the stop. The
    STANDALONE console's `brahma_console/orchestrator.rs` `stream_turn`, the greeting, the help chat and the
    swipe stay signal-less.
- `crates/quilltap-core/src/model/stream_watchdog.rs:52-58` module doc: the same correction.
- `crates/quilltap-core/src/services/turn_control.rs:28-35` ("No AsyncLocalStorage twin … threads it
  EXPLICITLY"): add the Carina → Brahma → one-shot thread to the listed hops.
- The P4.D335 order's R-4 text and Tier 3 item 31 are history and are NOT edited. Record the overturn in the
  new order and the status-log.

### Concrete design (one lane, Rust core + host + harness + one e2e beat)

1. `RunBrahmaConsole::run` gains a LAST parameter, `stop: Option<&StopSignal>`. Update the three impls:
   - `UnavailableBrahmaConsole` (`carina_query.rs:131`) ignores it;
   - `RealBrahmaConsole` (`brahma_console/mod.rs:405`) forwards it;
   - the harness `CannedBrahma` (`carina_query_tier3_equivalence.rs:327`) ignores it.

   `answer_as_brahma` passes `deps.turn_control.map(|h| h.stop_signal())`. That makes `CarinaQueryDeps.
   turn_control` the ONE source, and both in-turn entrances (finalizer markup and the `ask_carina` tool) inherit
   it with no spine edit. Alternative: a `RealBrahmaConsole::with_stop(StopSignal)` field set at spine
   `:1529` / `:1542`. It is rejected because it duplicates the handle CarinaQueryDeps already carries.
2. `run_brahma_query(deps, user_id, chat_id, question, stop)` puts `stop` into
   `RunOneShotToolLoopOptions.stop: Option<&'a StopSignal>`, added LAST after `log_label`. The Scenario Builder
   (`scenario_builder/mod.rs:461`) and the harness literal (`brahma_console_tier3_equivalence.rs:902`) pass
   `None`. **Do NOT set the loop's `signal`**: v4 passes none, and the stop must only quiet the stream so the
   loop's partial-answer, empty-answer and bug-47-salvage arms run as in v4.
3. `run_stream` takes `stop` and calls `watch_stream_with_stop(…, stop)`. Change the log gate (`:441`) to v4's
   rule — log iff a terminal chunk passed: `error.is_none() && saw_done`. Today's `!aborted` disjunct logs a
   `done`-less close. v4 never logs one, so the lane must first measure whether any existing
   `brahma_console_tier3` / `scenario_builder_tier3` row closes without `done`. Minimum safe form:
   `error.is_none() && (saw_done || (!aborted && !rx.stopped_by_operator()))`.
4. Census and docs: flip the rows listed above.
5. **NEW incidental, same file family (fold in or ride along):** v5's `answer_as_brahma` emits NONE of v4's
   Brahma lines — INFO `[Carina] Brahma Console answered`, WARN `[Carina] Live Brahma answer emit failed;
   answer stands`, ERROR `[Carina] Brahma query failed`. A stop-mid-Brahma row would naturally capture the
   INFO; see item N-1.

### Differential proof

- **Tier-3 (mocked LLM): `orchestrator_tier3_equivalence`** (`harness/oracle/cases/orchestrator-tier3.test.ts`
  + `harness/oracle/fixtures/orchestrator-tier3.json`). It already has everything needed: a provider-level
  mock (`jest.doMock('@/lib/llm', …)` at `:310`, so v4's REAL `streamMessage` + watchdog run), the REAL turn
  registry, and the `latch: 'stop'` ChunkSpec that calls the REAL `handleStopTurn` and parks the provider
  (`:91-93, 399-412`). The Rust side has the drain-then-marker `QueuedStreamingProvider` and the REAL
  `RealBrahmaConsole` over the shared Arc provider (`orchestrator_tier3_equivalence.rs:1898-1930`).
  - **New case `interrupt_stop_mid_brahma_markup`.** The reply is `"Let me check.\n@Brahma: How many
    chats?"`, streamed whole and `done` (shaped after the `carina_markup` case, fixture `:3161-3170` /
    `:4687-4697`). The asker must be Brahma-reachable (`systemTransparency: true`, or a user-controlled seat;
    v4 `carina.service.ts:325-344`). The Brahma one-shot canned sequence is `[{content:"There are"},
    {latch:"stop"}].`
  - Comparands: the `chat_messages` dump (the Brahma carina row with content `There are`), the `llm_logs` dump
    (the reply's row, NO Brahma row), the frame trace (`carinaAnswer`, then the stopped chain), stops
    `{stopped:1, settled:true, …}`, and the captured `[LLMStream]` INFO with `context:streaming.service`.
  - Two siblings:
    - `…_before_first_chunk`: sequence `[{latch:"stop"}]`, giving Prospero's `llm-failed` `empty response`;
    - `…_between_tool_turns`: a `run_sql` native call, then the stop, giving the bug-47 salvage text and the
      WARN.

    Red-first: on unported main the v5 side never stops, so the Brahma row differs and the stop settle times out.
- **Unit (in-crate):** `brahma_console/tests.rs`, in three rows over an already-stopped or mid-stream
  `StopSignal`:
  - no `llm_logs` row for a stopped leg;
  - `empty response` when nothing streamed;
  - the INFO's bag captured, with no `characterId` / `messageId`.
- **Playwright:** add beat (C) to `apps/web/e2e/salon-interrupt-flow.spec.ts` (own server 4350, sequenced mock
  45348, `plan.replies` / `plan.delayMs` at `:86-157`). Script `plan.replies = [FAST "Let me check.\n@Brahma:
  How many chats?", SLOW Brahma answer]` with `delayMs` 350. Click Stop while the Brahma text streams, then
  assert:
  - the composer settles to idle well under the 15 s settle window;
  - the `Stopped.` toast shows;
  - the transcript shows Brahma's partial bubble (or the Prospero card when the stop is pre-first-chunk).

  The seed needs a transparent asker.
- **Files a lane would own:**
  - core: `services/carina_query.rs`, `services/brahma_console/{mod.rs,tests.rs}`,
    `services/agent_loop/one_shot_loop.rs`, `services/scenario_builder/mod.rs` (one `None`),
    `model/stream_watchdog.rs` (doc), `services/turn_control.rs` (doc);
  - harness: `crates/quilltap-harness/tests/{stream_watchdog_wrap_census.rs,
    carina_query_tier3_equivalence.rs,brahma_console_tier3_equivalence.rs,orchestrator_tier3_equivalence.rs}`,
    `harness/oracle/cases/orchestrator-tier3.test.ts`, `harness/oracle/fixtures/orchestrator-tier3.json` (+ its
    builder if the transparent asker needs a seat);
  - SPA: `apps/web/e2e/salon-interrupt-flow.spec.ts`.
- **Size:** M.

---

## §B — The `25b8fab6a` round's recorded follow-ups

### F-1 — `CHEAP_LLM_KEY_ORDER`: `cheapLLMSettings` read in stored order, not Zod schema order   [OPEN]
- Source: P4.D304 status header OPEN; status-log 188316; pin comment at `settings_routes_equivalence.rs:583-589`.
- v5 site: `crates/quilltap-core/src/db/chat_settings.rs:174-182`. `default_cheap_llm_keys` only appends
  `allowCheapFallback` when absent; the read site at `:1462-1466` (`strip_retired_cheap_llm_keys(
  default_cheap_llm_keys(parse_json(cell)))`) keeps the stored bag's order. A row stored `…,
  allowCheapFallback, embeddingProvider` reads back in that order.
- v4 site @322a5488e: `lib/schemas/settings.types.ts:49-72` `CheapLLMSettingsSchema = z.object({ strategy
  .default('PROVIDER_CHEAPEST'), userDefinedProfileId: UUIDSchema.nullable().optional(),
  defaultCheapProfileId: …nullable().optional(), fallbackToLocal .default(true), embeddingProvider
  .default('OPENAI'), allowCheapFallback .default(false) })`. Zod's output is in shape order, defaults are
  filled, and unknown keys are stripped.
- Fix shape: replace the two helpers with ONE `zod_parse_cheap_llm_settings(bag) -> Value` that rebuilds the
  object in shape order:
  - `strategy` (default `PROVIDER_CHEAPEST`), the two optional ids only when present (null kept),
    `fallbackToLocal` (true), `embeddingProvider` (`OPENAI`), `allowCheapFallback` (false);
  - unknown keys dropped, which subsumes the `imagePromptProfileId` strip.

  Measure the enum-invalid arm (v4's `.parse` throws) before deciding how a bad enum reads.
- Differential: `settings_routes_equivalence` — extend the `byte_family` top-key compare at `:587-600` to the
  nested `cheapLLMSettings` key sequence for every family, and add a corpus row seeded
  `allowCheapFallback`-before-`embeddingProvider`. Plus the in-crate tests at `chat_settings.rs:1975-2025`.
- Files: `crates/quilltap-core/src/db/chat_settings.rs`, `crates/quilltap-harness/tests/settings_routes_equivalence.rs`, the settings-routes oracle case + corpus.
- Size: S.

### F-2 — `ZOD_STRING_LENGTH_PAST_INVALID_TYPE` on the four zod twins   [OPEN]
- Source: P4.D311 status header OPEN; status-log 188317.
- v5 sites (each stops at `invalid_type` for a non-array):
  1. `services/chat_create.rs:623-628` `check_opt_subprompt_ids` (`selectedSubpromptIds`, max
     `SELECTED_SUBPROMPT_IDS_MAX_ITEMS`);
  2. `memory_candidate.rs:560-573` `string_array` (`cap: Option<usize>`);
  3. `api/memories.rs:683-700` + `array_of` `:560-583` (`entities`, max `MEMORY_ENTITY_CAP` = 12);
  4. `services/scenario_builder/request_schema.rs:242-276` `uuid_array_field` (max 32).
- v4 @322a5488e: zod 4.6.7 runs the array's `.min` / `.max` checks on any input that has a `.length` even after
  `invalid_type`, so a STRING input past the cap also yields a `too_big` with `origin:"string"` ("Too big:
  expected string to have <=N characters"). This reaches the wire through the scenario-builder route's
  `details`. The model already in v5 is the fifth (Pascal) twin:
  `pascal/custom_tool_types.rs:4207-4220` `authored_length_checks_past_invalid_type`.
- Fix shape: lift a shared `zod_length_checks_past_invalid_type(v, min, max, path) -> Vec<ZodIssue>` emitting
  Zod's DEFAULT string-origin sentences (the Pascal twin uses authored messages) and call it in the four
  `invalid_type` arms. Measure each schema's exact checks (`.min(1)`? `.max(n)`) at the pin before writing rows.
- Differential: `scenario_build_request_schema_equivalence`, `memory_candidate_equivalence`,
  `memories_routes_equivalence`, `chat_create_capstone_equivalence`, each grown by `string-over-cap` /
  `empty-string` rows. Also update `zod_version_guard.rs`'s doc list.
- Files: the four source files above + `crates/quilltap-core/src/api/zod_issues.rs` (the helper) + the four families' cases.
- Size: S–M.

### F-3 — the strict `users` read in `user_identity_resolver`   [OPEN]
- Source: P4.D305 status header OPEN; status-log 188318.
- v5 site: `services/user_identity_resolver.rs:198-202` calls `db.read_main(|c| users::find_name_by_id(c,
  &uid))?`. `db/users.rs:277-292` propagates any error except no-rows, so the whole identity resolution fails.
- v4 @322a5488e: `lib/services/chat-message/user-identity-resolver.service.ts:79` calls `repos.users.
  findById(userId)`, which is `users.repository.ts:48-54` `safeQuery(() => this._findById(id), 'Error finding
  user by ID')` over `base.repository.ts:247-258`'s `safeQuery(…, 'Error finding entity by ID', {id}, null)`.
  A read error or a Zod-invalid row logs and answers `null`, and the chain falls to step 4 `"User"`. (v4 also
  creates the `users` table lazily; v5 provisions it.)
- Fix shape: route step 3 through the repository-fallback home (`db/fallback.rs` idiom). Map an `Err` to
  `None` after logging v4's ERROR line `Error finding entity by ID {id}`; measure whether the outer
  `Error finding user by ID` also fires (the inner default swallows it, so likely not). Unreachable while
  provisioning creates `users`, so this is a hardening/fidelity item.
- Differential: unit-only (no clean plant: a missing `users` table is the only error and v4 creates it), plus
  a source census row in the repository-fallback census if one lists `find_name_by_id`.
- Files: `crates/quilltap-core/src/services/user_identity_resolver.rs`, `crates/quilltap-core/src/db/users.rs`.
- Size: S.

### F-4 — `run_custom`'s two missing v4 WARNs   [OPEN]
- Source: P4.D305 status header OPEN; status-log 188319.
- v5 site: `crates/quilltap-core/src/tools/run_custom.rs`:
  - `:830-838`, the invalid-input arm, returns `failure("Invalid input: `tool` must be …")` with NO log;
  - `:908-923`, the unknown-tool arm, returns the "No custom tool named …" failure with NO log.
- v4 @322a5488e: `lib/tools/handlers/run-custom-handler.ts`:
  - `:104-106` `logger.warn('run_custom validation failed', { context: CONTEXT, userId, input })`;
  - `:155-160` `logger.warn('run_custom named an unknown tool', { context: CONTEXT, chatId, tool, available })`;
  - `CONTEXT = 'run-custom-handler'` (`:28`).
- Fix shape: two `tracing::warn!(target: "quilltap::pascal", context = HANDLER_CONTEXT, …)` lines with v4's
  field order. `input` must render as JSON and `available` as the roster's key list in roster order. The
  existing `report_failure` WARN at `:738-746` is the idiom.
- Differential: `pascal_run_custom_handler_equivalence`, which already captures lines via `global_capture`
  (`:395-397`) against `harness/oracle/cases/pascal-run-custom-handler.test.ts`'s transports splice. Widen the
  message filter and add rows: invalid input, unknown tool with roster, and unknown tool with an empty roster.
- Files: `tools/run_custom.rs`, the handler family + oracle case.
- Size: S.

### F-5 — the enclave turn's missing consult runner   [CLOSED — the record is stale]
- Source: P4.D305 status header OPEN ("recorded in `enclave/step.rs`"); status-log 188320.
- v5 evidence:
  - The PRODUCTION autonomous turn is the host's `run_step` (`crates/quilltap-host/src/spine.rs:2033`, the
    `AUTONOMOUS_ROOM_TURN` runner body). Its `OrchestratorDeps` carries `consult: self.consult.clone()` at
    `spine.rs:2158`, and `web_search: self.web_search.clone()` at `:2153`.
  - Both were wired by the SAME unification commit `fbe0b6b9c` that recorded the gap.
  - The `consult: None` + comment at `crates/quilltap-core/src/enclave/step.rs:1918-1921` sits inside
    `#[cfg(test)] mod tests` (`:1447-2147`; `run_one_step` `:1864`), the core's E2E test runner. Its
    `web_search: None` comment at `:1914-1916` is likewise test-only.
- Fix shape (optional nit): reword the `step.rs:1914-1921` comments to say "the test runner" so no later reader
  re-opens it; correct the status-log/P4.D305 header text at the next round record.
- Size: S (doc only).

### F-6 — the stale L8 note   [OPEN]
- Source: P4.D310 status header OPEN (NIT).
- v5 site: `harness/oracle/fixtures/turn-orchestrator-tier2.json:1004`. The note reads "L8: … the early return
  precedes the user-driven-seat arm (user_turn WITHOUT participantId/characterName) -- a v4 oddity". P4.D310
  measured the opposite: v4's selector answers `user_turn` for a user-driven seat WITH participant and name
  (R-5 retired).
- Fix shape: rewrite the note to the measured behaviour and regenerate `turn-orchestrator-tier2.ts` from the
  pin if the note is echoed into the NDJSON (the case type carries `note?: string`, `:55`).
- Differential: `turn_orchestrator_tier2_equivalence` stays green, with no behaviour change.
- Files: `harness/oracle/fixtures/turn-orchestrator-tier2.json`.
- Size: S.

---

## §C — Interrupts / turn-spine / evaluator items assigned to this slice

### I-1 — `?action=raise-hand` refuses a non-object body before loading the chat (400 vs v4's 404)   [OPEN]
- Source: P4.D335 status header OPEN; `322a5488e` round review "Recorded OPEN (notes)".
- v5 sites:
  - The edge, `crates/quilltap-web/src/wardrobe_routes.rs:873-900`: the `raise-hand` arm parses the body and,
    when the body is not an object (e.g. `[]`), returns 400 `{error:'Validation error', details:[invalid_type
    object]}` BEFORE any dispatch.
  - The core, `crates/quilltap-core/src/api/chat_interrupt.rs:107-127` `chat_raise_hand`, loads the chat first
    (`:108-110` → 404) but is never reached for a non-object body.
- v4 @322a5488e: `app/api/v1/chats/[id]/handlers/post.ts:78-82` `const chat = await repos.chats.findById(chatId);
  if (!chat) return notFound('Chat')` runs BEFORE `dispatchAction`. `'raise-hand': () => handleRaiseHand(req,
  chatId)` (`:92`) → `actions/interrupt.ts:54-63` `raiseHandSchema.parse(await req.json().catch(() => ({})))`.
- Fix shape: move the root-shape refusal behind the chat lookup. Either:
  - (a) carry the non-object parsed body into core (e.g. a skip-serialized `body_root: Option<Value>` on
    `Request::ChatRaiseHand`, `api/types.rs:316`), with `chat_raise_hand` doing load → 404, then root
    `invalid_type` → 400;
  - or (b) on a non-object body, have the edge dispatch a chat-existence probe (`Request::ChatGet`,
    `api/types.rs:166`) first.

  (a) keeps the order in one place.
- Differential: `crates/quilltap-web/tests/chat_interrupt_rest_routes.rs` gains a `missing chat + body []` → 404
  row (plus `tri_state_edges_share_the_decoder` / `dispatch_wrong_type_census` if the wire grows a field).
- Files: `crates/quilltap-web/src/wardrobe_routes.rs`, `crates/quilltap-core/src/api/{chat_interrupt.rs,types.rs,engine.rs:1105}`.
- Size: S.

### I-2 — integer-like gate / `when` / record keys iterate in authored order, not V8 `Object.keys` order   [OPEN]
- Source: P4.D329 + P4.D333 status headers; `322a5488e` round wire 18.
- v5 sites:
  - **Root cause, one chokepoint:** `crates/quilltap-core/src/pascal/custom_tool_types.rs:2749-2772`
    `parse_record` iterates `for (key, v) in obj` in serde's insertion order. Every keyed record of the
    definition is built here:
    - gate `metadata` `:2513`, gate `progress` `:2540`;
    - gate/when `inventory` / `inventoryCount` / `inventoryAggregate` `:2939` via `parse_inventory_records`
      `:2924`;
    - when `params` `:3318`, when `metadata` `:3333`, when `progress` `:3349`;
    - the tool's `parameters` `:4081`.

    The resulting `Vec<(String, _)>` (`:852-885`, `:957-1007`, `:1716`) is what every reader walks.
  - **Downstream readers that inherit the authored order:**
    - `pascal/tool_gate.rs:203` (`metadata`/`progress` tests), `:227` `inventory`, `:243` `inventoryCount`,
      `:248` `inventoryAggregate`. Short-circuit order shows up in the decline DEBUG and `heldGroup`;
    - `pascal/custom_tools.rs:1094-1115` `collect_metadata_tested`, an accumulator v4 builds as an OBJECT. Its
      `pascalMeta.metadataTested` key order is V8 order even ACROSS groups, so it needs its own ordering at
      output even after the parse fix;
    - the serialized `data` (`ser_opt_param_comparators` / `ser_opt_record` `:1762-1810`), which the
      definition corpus byte-compares;
    - the issue order from `parse_record`;
    - the tool's `parameters` order in the model-visible schema.
  - **The only site honouring V8 order today:** `pascal/tool_gate_keys.rs:175-198` `object_keys` + `array_index`
    (the census).
  - **Four duplicate canonical-array-index helpers exist:** `embedding_blob.rs:356-360`,
    `pascal/tool_gate_keys.rs:193-198`, `pascal/js_value.rs:181-187` (inline in `json_stringify_pretty`) and
    `inventory/types.rs:1088-1094`. A whole-value re-keyer `inventory::types::v8_key_order` already exists at
    `:1098-1135`.
- v4 @322a5488e: `JSON.parse` already yields integer-like keys first, ascending (ES `OrdinaryOwnPropertyKeys`).
  Zod's `z.record` walks own keys in that order, and every evaluator does `Object.entries(group.metadata ??
  {})`, so ALL of v4 sees V8 order.
- Fix shape:
  1. Move `v8_key_order` + ONE `is_array_index` into a shared home (`jsstr` or `pascal/js_value.rs`) and
     delete the three other copies (DRY).
  2. Apply `v8_key_order` once at the definition parse entry: `safe_parse`'s input. That covers roster load
     (`roster.rs:383`), the Workbench / bench bodies, `.qtap` import and `edit_custom_tool`.
  3. Give `collect_metadata_tested`'s output V8 order.
  4. `tool_gate_keys::object_keys` becomes redundant (keep it as a no-op or delete it).
- Differential: `pascal_custom_tool_definition` corpus (`harness/oracle/cases/pascal-custom-tool-definition.ts`;
  the SPA copy byte-equal) gains rows with `{"lamp":…,"42":…,"7":…}` in gate `metadata`, a `when.params`, and an
  inventory record (serialized data + issue order). Also `pascal_tool_gate_equivalence` (decline order),
  `pascal_metadata_match_equivalence` / the execution rows (`metadataTested` across two groups), and
  `pascal_tool_gate_keys` (unchanged, green). **The SPA twin** (`apps/web/src/app/pascal/*.ts`) gets this free
  from `JSON.parse`; verify by the corpus spec only.
- Files: `pascal/custom_tool_types.rs`, `pascal/custom_tools.rs`, `pascal/tool_gate_keys.rs`,
  `inventory/types.rs`, `embedding_blob.rs`, `pascal/js_value.rs` (or `jsstr.rs`), and the definition corpus
  generator + NDJSON (Rust + SPA copies).
- Size: M.

### I-3 — `chat_messages` lazy-create (effects family fixture)   [NEEDS-RULING — recommend NO-ACTION]
- Source: P4.D329 status header OPEN; `322a5488e` round "Pascal carve lift".
- v5: `crates/quilltap-core/src/services/provisioning/fresh_schema.json` carries `chat_messages`, so every
  v5-provisioned instance has it. The family fixtures now include it (the routes pair gained `chat_messages`
  + `background_jobs` at the union).
- v4 @322a5488e: the chats repository creates `chat_messages` lazily through `getCollection`/generateDDL
  (`lib/database/repositories/base.repository.ts:247-258` path). It is not a migration table. Every real v4
  instance that has booted carries it, because `lib/startup/reconcile-chat-message-fts.ts:34,78`
  `ensureChatMessageFtsSchema` creates triggers ON it at every boot.
- Recommendation: NO-ACTION, recorded as a fixture-premise class. A boot ensure would only matter for an
  instance v4 created and never booted far enough to reconcile FTS. If the human wants belt-and-braces: add
  `chat_messages` to the PHASE 3.1 structural-create census (`db/table_shape.rs:456`
  `create_missing_structural_tables`), which already creates absent dedicated tables from the DDL dump.
- Size: S if ruled in.

### I-4 — R-6: `PendingInventoryAnnouncements` drains in key order, v4 in insertion order   [OPEN]
- Source: P4.D328 R-6 (recorded-only); `322a5488e` round review "Recorded OPEN (notes)".
- v5 sites, all in `crates/quilltap-core/src/services/inventory_change_effects.rs`:
  - `:65` `pub type PendingInventoryAnnouncements = Arc<Mutex<HashMap<String, PendingInventoryChange>>>`;
  - `record_pending_inventory_change(&mut HashMap…)` `:225`, `strike_from_pending_inventory_change` `:278`;
  - `flush_pending_inventory_announcements` `:435-480`, which drains, then `entries.sort_by(key)` (`:452`);
  - the module doc "## Ordering" `:36-42`;
  - C1's `InventoryChangeContext.pending: &mut HashMap` lives in `inventory/equip.rs`.
- v4 @322a5488e: `lib/inventory/equip-effects.ts:180-197` `pending.set(key, entry)` (never deleted mid-turn;
  strike edits in place). `:223-245` `flushPendingInventoryAnnouncements` does
  `Array.from(pending.values())`, so posts happen in FIRST-record order, and the DEBUG's `provenances` array
  follows it.
- Fix shape: make the map insertion-ordered. Either add `indexmap` as a direct dependency (already in
  `Cargo.lock` via `serde_json/preserve_order`, so no new crate) and use `IndexMap` with `get_mut`/`insert`
  (never `swap_remove`), or keep a `Vec<(String, PendingInventoryChange)>` with linear lookup (turn-sized).
  Delete the sort and rewrite the doc. Sibling class to look at in the same lane: the wardrobe turn set /
  history map drain "v5's sorted order — R-3, recorded" (`services/outfit_change_effects.rs`), since v4's `Set`
  / `Map` are insertion-ordered too.
- Differential: `pascal_inventory_change_equivalence` (or `inventory_tools_equivalence`) gets a row where
  character B records before A in one turn. Compare the two Pascal rows' order and the drain DEBUG's
  `provenances` order.
- Files: `services/inventory_change_effects.rs`, `inventory/equip.rs` (the `pending` field type), callers that
  name `HashMap` (`pascal/inventory_effects.rs:564`, `tools/inventory_tool_table.rs:73`,
  `pascal/side_effects.rs:184,1125`, `tools/run_custom.rs:694`, `services/outfit_change_effects.rs:47`), and
  `crates/quilltap-core/Cargo.toml` if `indexmap` is used.
- Size: S–M.

### I-5 — P4.D336 NOTE: the SPA's multi-character check reads raw participants / `p.character`   [OPEN]
- Source: P4.D336 status header OPEN (NOTE).
- v5 site: `apps/web/src/app/screens/salon/salon-conversation.ts:2728-2737` `isMultiCharacterRoom` (used by
  `stop()` at `:4304`). It counts `controlledBy === 'user'` over ALL raw participants (no `type`/character
  filter) and tests `p.character` for LLM seats. Its neighbour `isSingleCharacterChat` (`:2718-2722`) lacks
  v4's characterId filter.
- v4 @322a5488e: `app/salon/[id]/hooks/useParticipants.ts:34-57` `participantsAsBase` filters to `type ===
  'CHARACTER' && (characterId || character?.id)` and defaults `controlledBy ?? 'llm'` and `status`. `:65-68`
  `isMultiChar = isMultiCharacterChat(participantsAsBase)` (`lib/chat/turn-manager/utils.ts:197-208`, with
  `getActiveLLMParticipants` `:176-185` testing `p.characterId`). `:74-76` `isSingleCharacterChat` uses the
  same base.
- Fix shape: port `participantsAsBase` as one computed and derive both predicates from it. Observable: the stop's
  optimistic pause and toast wording, and `canRaiseHand`.
- Differential: unit-only (SPA spec — `salon-conversation` stop cases with a USER-type seat and a seat lacking
  `characterId`).
- Files: `apps/web/src/app/screens/salon/salon-conversation.ts` + its spec.
- Size: S.

### N-1 — NEW (incidental): Carina's service log lines mostly unported   [OPEN — NEW finding]
- Source: found during the R-4 survey. Not recorded in any order or the status-log (grep).
- v5 site: `crates/quilltap-core/src/services/carina_query.rs` emits only 2 tracing lines (`:429` debug, `:1150`
  warn).
- v4 @322a5488e: `lib/services/carina/carina.service.ts` has 16 logger calls. The INFO/WARN/ERROR ones are:
  - `:148`, `:189`, `:243`, `:296`, `:337` — WARNs;
  - `:390` WARN `Live Brahma answer emit failed`;
  - `:398` INFO `[Carina] Brahma Console answered`;
  - `:414` ERROR `Brahma query failed`;
  - `:491`, `:498`, `:506` — INFO;
  - `:795`, `:816` — WARNs;
  - `:824` INFO `[Carina] Query answered`;
  - `:842` ERROR.
- Fix shape: port the lines at their v5 homes, with field order measured.
- Differential: `carina_query_tier3_equivalence` (add a transports splice to `carina-query-tier3.test.ts` and
  a `global_capture` compare), plus the R-4 orchestrator row for the Brahma INFO.
- Files: `services/carina_query.rs`, the carina tier3 case + family.
- Size: M (can ride the R-4 lane for the three Brahma lines; the rest is a separate smalls item).

---

## §D — Every other OPEN / recorded / Tier-3 item, order by order

### P4.D304
- **OPEN:** `CHEAP_LLM_KEY_ORDER`, which is **F-1**.
- **Tier 3:**
  - 18–21, 23 and 24: **NO-ACTION.** These are the migration-runner deferral, NO-PORT log lines, test files,
    P4.D313's docs, consumed contracts, and items that do not exist in v4.
  - 22 (`pascalMeta.invokedBy 'trigger'`): **CLOSED.** It landed in P4.D305 / P4.D313.

### P4.D305
- **OPEN:** F-3, F-4 and F-5 (F-5 is CLOSED, the record is stale).
- **Tier 3 22** — the user-message Carina path is the standing `user_message_carina` deferral. It is still
  unwired (`orchestrator.rs:48, 864, 2043, 2093`). **NO-ACTION** here; note that it bounds the R-4 lane (only
  the markup path and the `ask_carina` path reach Brahma).
- **Tier 3 23** — the executor `on_posted` gap. **NEEDS-RULING.**
  - v5 is still divergent: `tools/executor.rs:757-762` persists Pascal's post but leaves the live
    `pascalResult` splice to the post-turn refresh, and the same is true of `emitCarinaAnswer` at `:1724-1727`.
  - v4 emits it live through `context.emitPascalResult`.
  - The fix would be a per-turn sink on `ToolExecutionContext`. It is recorded as "must not fix here", so it is
    the human's call whether it becomes an order. M.
- **Tier 3 24–30:**
  - 24, 26 and 29 are **NO-ACTION**.
  - 25, 27 and 30 are **CLOSED** (green on the union, P4.D307 landed, P4.D313 vendored).
  - 28 (a LIVE consult trigger) is a dogfood 💸 row and **NO-ACTION** here.

### P4.D306
- **OPEN Tier 2 item 18: the handler family's pair rebuilt** (a wear/take-off `regalia` tool). **OPEN, S–M.**
  - Status: still unlanded. `harness/oracle/fixtures/build-pascal-run-custom-fixture.ts` has no `regalia`, and
    there is no `effects-wardrobe-wear-and-take-off` row in `harness/oracle/cases/pascal-run-custom-handler.test.ts`.
  - Measured obstacle (status-log 185884): FIVE families read the pair.
  - Fix shape: an `extend-pascal-run-custom-wardrobe.ts` extender, following the
    `extend-pascal-run-custom-{announcer,inventory,triggers}.ts` precedent, so the base pair stays untouched for
    the siblings. Add the two jest rows.
  - Differential: `pascal_run_custom_handler_equivalence`.
- **v4-bug candidate `{{params.constructor}}`** (`custom-tool.types.ts:1357`): **NEEDS-RULING** (filing, the
  human's).
- **Tier 3:**
  - 20, 21 and 24: **NO-ACTION.**
  - 22: **CLOSED.** `pending_wardrobe_announcements` is `Option`, set at unification.
  - 23: **CLOSED** at the union.

### P4.D307
- **OPEN:** none.
- **Tier 3:**
  - 21: **CLOSED.** The popup wire landed at unification.
  - 22–27: **NO-ACTION** (spec-only twin, NO-PORT files, v4's own expression, P4.D309's line).

### P4.D308
- **OPEN:** none.
- **Tier 3:**
  - 27–34 and 38–41: **NO-ACTION** (NO-PORTs and recorded classes `CUSTOM_RUN_PARAMETERS_OMITTED`,
    `SLASH_LISTBOX_ID_PREFIX`, `RUNNER_ERROR_ARMS`). `SLASH_ROW_GLYPH_TEXT` was retired by item 23.
  - 35 and 36: **CLOSED** at unification.
  - 37 (the `core-contract.ts` accessor cast, "droppable after the union", wire 8 skipped): **NO-ACTION /
    optional nicety**, S.

### P4.D309
- **OPEN (NIT): the `as CoreRequest` cast.** **OPEN-optional, S.** It is still present at
  `apps/web/src/app/wardrobe/wardrobe-folders.api.ts:133` (`} as CoreRequest);`). The SPA has 8 such casts
  outside specs. Fix: type the literal by its variant (`satisfies Extract<CoreRequest,{type:…}>`). Unit-only.
- **Tier 2 item 21: the outfit-asks e2e beat.** **OPEN-optional, S.**
  - Status: still not landed (`apps/web/e2e/wardrobe-copy-to-folder-flow.spec.ts:41` records it).
  - Fix: one beat composing `Flight Jacket` + `Goggles` through the item editor and asserting the legend `This
    outfit bundles 2 components`.
- **Tier 3 25–30:** **NO-ACTION** (v4-faithful classes, P4.D310's half).

### P4.D310
- **OPEN:** the L8 note, which is **F-6**.
- **Tier 2 item 13: oracle-side capture of the `[TurnOrchestrator]` lines.** **OPEN-optional / NEEDS-RULING,
  M.** It is deferred because the renderer would need a camelCase → snake_case key map; that mapping is a
  ruling on whether a key-case normalizer is acceptable.
- **Tier 2 item 14: a `sqlite3_trace_v2` statement-order pin for the render job.** **OPEN-optional, S–M.**
  There is still no trace hook on `Db`'s pool / writer.
- **Tier 3 16–21:** **NO-ACTION.** Item 21's filing candidate was retired by measurement.

### P4.D311
- **OPEN:** `ZOD_STRING_LENGTH_PAST_INVALID_TYPE`, which is **F-2**.
- **The OAC falsy-usage frame:** **NO-ACTION** (NO-PORT).
- **Tier 3 16–20:** **NO-ACTION.**

### P4.D312
- **OPEN:** none.
- **Tier 3 18–24:** **NO-ACTION.**

### P4.D314
- **OPEN:** none.
- **Tier 3 item 11:** Rust `is_moderation_refusal` (`services/dangerous_content/refusal.rs:381`) is test-only.
  **OPEN-optional (YAGNI), S.**
  - Callers today: `model/image_dialects.rs:2508-2535` (inside tests) and
    `quilltap-harness/tests/refusal_classify_equivalence.rs:47,209`.
  - Fix: delete it, and have both tests call `classify_refusal(..).refused`.
  - Differential: `refusal_classify_equivalence` stays green.
- **Tier 3 12–14:** **NO-ACTION.**

### P4.D319
- **OPEN: R-10's route-level status on a degraded mount** (a byte-compare owed). **NO-ACTION here**; it is a
  dogfood row.
- **OPEN: the Courier `[Staff: pascal]` filing candidate.** **NEEDS-RULING (filing).**
  - v5 still renders the raw key (`services/courier_transport.rs:35-49`, pinned `:1023`).
  - The candidate is already listed in `dogfood-findings.md:707, 743-744`.
- **Tier 3:**
  - 24, 25 and 27: **NO-ACTION.** 27 is the same filing candidate.
  - 26: **CLOSED** (P4.D320 landed).

### P4.D320
- **OPEN:** Tier 3 items 27–34. All are **NO-ACTION**, being recorded classes or NO-PORTs:
  - 27: no server code;
  - 28: P4.D323 help;
  - 29: the TanStack cache-sharing divergence class;
  - 30: `message-list.ts` unthreaded;
  - 31: the expressions refusal omits `character`, as in v4;
  - 32: the toggle without a seated character, as in v4;
  - 33 and 34: React-only NO-PORTs.

### P4.D329
- **OPEN:** integer-like key order, which is **I-2**; `chat_messages` lazy-create, which is **I-3**.
- **Tier 3:**
  - 25, 26, 31 and 32: **CLOSED** at the union (stand-in deleted, list rule landed, any-of rows landed,
    siblings landed).
  - 27: **NO-ACTION.** These are NEGATIVE guard pins.
  - 28: the bench's always-null `item()`. **NEEDS-RULING** (v4 filing candidate).
  - 29 and 30: **NO-ACTION** (NO-PORT, ratified).

### P4.D332
- **OPEN:** none.
- **Tier 3:**
  - 19 and 22: **NO-ACTION.**
  - 20, 21, 23 and 24: **CLOSED** at the union. The specimen and embed guard are green; the `uri` mask was
    lifted (`pascal_edit_custom_tool_equivalence.rs:115`).
  - 25: **NO-ACTION** (dogfood).

### P4.D333
- **OPEN NOTE: `collectMetadataTested` records losing groups' keys.** It was ported faithfully. **NEEDS-RULING
  (v4 filing candidate).**
- **OPEN: integer-like key order**, which is **I-2**.
- **Tier 3:**
  - 27–31: **CLOSED** at the union.
  - 32 and 34: **NO-ACTION.**
  - 33: the same filing candidate.

### P4.D334
- **OPEN HANDOFF (9): the help-sentence byte-compare.** **NO-ACTION, with a v4-shared filing candidate.**
  - The SPA option reads "Enter a log line" (`screens/custom-tools/side-effects-section.ts:208`); the aria
    label is "Log line" (`:258`).
  - Both are byte-equal to v4 (`components/custom-tools/SideEffectsSection.tsx:220`).
  - The vendored `help/pascals-workbench.md:34, 90` names the kind *Log line*, byte-equal to the pin.
  - So v4's own help and UI disagree. That is a filing candidate, and v5 is faithful.
- **OPEN: `workspace-core-fixtures.json` `_meta.baseline` literal.** **OPEN, S.**
  - `apps/web/src/app/workspace/core/__fixtures__/workspace-core-fixtures.json:4` reads `"baseline":
    "e646f58b"`, hard-coded in the generator at `harness/oracle/cases/workspace-core.ts:631` and echoed in
    `workspace-core.spec.ts:13`.
  - Fix: write the real pin (or `git rev-parse --short HEAD` of the v4 checkout at run time) and regen from
    `322a5488e`. The fixture body should be byte-unchanged apart from `_meta`; measure that.
- **Tier 3:**
  - 33 and 34: **CLOSED** at §S.9.
  - 35: R-8's three v4 defects. **NEEDS-RULING** (filing).
  - 32 and 36–40: **NO-ACTION.**

### P4.D335
- **OPEN:** the R-4 ruling item, which is **§A** (ruled (A)); the raise-hand order, which is **I-1**.
- **Tier 3 30–37:** **NO-ACTION.**
  - These are the enclave turn unregistered (deliberate), the ALS not ported (explicit threading, which the
    §A lane extends), T6/T15 dogfood rows, the socket-abort note, the `details`-less 400 standing class, and
    NO-PORT files.
  - 33's two v5 gaps were retired by P4.D336 (landed).

### P4.D336
- **OPEN NOTE:** `isMultiCharacterRoom`, which is **I-5**.
- **Tier 3:**
  - 17–23 and 25–27: **NO-ACTION.**
  - 24 (the `IconName` doc count): **CLOSED.** `apps/web/src/app/ui/icon.ts:6` now reads "The 88-name".
  - 21: Madman's Box theme overrides were P4.D339's, landed with the round.

---

## §E — Suggested lane grouping for `/setupphase`

| Lane | Items | Ownership | Size |
|---|---|---|---|
| **Brahma stop (ruled A)** | §A + N-1's three Brahma lines | core `carina_query.rs` / `brahma_console/*` / `one_shot_loop.rs` / watchdog doc; harness census + orchestrator_tier3 + carina/brahma tier3 literals; one e2e beat | M |
| **Pascal key order + run_custom WARNs** | I-2, F-4, (P4.D306 item 18 extender) | `pascal/*`, `tools/run_custom.rs`, definition corpus (Rust + SPA copy), handler family | M |
| **Chat-settings / zod / identity smalls** | F-1, F-2, F-3 | `db/chat_settings.rs`, the four zod twins + `api/zod_issues.rs`, `user_identity_resolver.rs`, `db/users.rs` | S–M |
| **Interrupt + inventory-order smalls** | I-1, I-4 (+ the wardrobe-set sibling look) | `quilltap-web/src/wardrobe_routes.rs`, `api/chat_interrupt.rs`, `services/inventory_change_effects.rs` + callers | S–M |
| **SPA smalls** | I-5, P4.D309 cast + item 21 beat, P4.D334 `_meta.baseline`, F-6 note | `salon-conversation.ts`, `wardrobe-folders.api.ts`, e2e, `workspace-core.ts` + fixture, tier2 JSON | S |
| **Carina log census** (optional, separate) | N-1 remainder | `services/carina_query.rs` + carina tier3 | M |

Rulings/filings for the human:
- P4.D305 T3-23, the live `pascalResult` / `carinaAnswer` splice for the model entrance;
- I-3, `chat_messages` (recommend no-action);
- P4.D310 item 13, the key-case normalizer;
- v4 filing candidates: `{{params.constructor}}`, `[Staff: pascal]`, the bench's always-null `item()`,
  `collectMetadataTested`'s losing groups, P4.D334 R-8's three defects, and the Workbench help-vs-UI "Log
  line" mismatch.
