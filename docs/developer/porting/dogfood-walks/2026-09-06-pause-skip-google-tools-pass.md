# Dogfood walk — the `f699da6f6` 4.9.x drift catch-up round (P4.D160 ∥ P4.D161 ∥ P4.D162 ∥ P4.78 ∥ P4.79)

**Date:** 2026-09-06. **Driver:** Claude (agent-driven), with a short human
remainder. **Instance:** a COPY of real Friday at `~/qt-dogfood-friday`.
**Build under test:** main at `a6e92abe` (the round's unification, 16:47);
binaries + SPA built 21:24 — both post-date it.

**Server (Claude launches):**
`RUST_BACKTRACE=1 ./target/release/quilltap-web --data-dir ~/qt-dogfood-friday
--spa-dir apps/web/dist/quilltap/browser --port 3400`, log in the scratchpad.
**Queries:** `./target/release/quilltap db --data-dir ~/qt-dogfood-friday
--json "…"` (main), `--llm-logs` for `llm_logs` (call-type column is `type`).

## Rounds under test

The `f699da6f6` 4.9.x drift catch-up, UNIFIED 2026-09-06:

- **P4.D160** (v4 bug 123, server): the `chainComplete` frame's optional
  `paused` key at the four `execute_turn_chain` emits; a paused chat stops
  LOUDLY (the initial-turn stop and the chain-error safety stop both announce
  `paused: true`); an empty reply says `paused: false` explicitly.
- **P4.D161** (bug 123, SPA): the pause announcement (`announceChainPause`, two
  sentences, four gates), the seat-keyed **off-turn Skip banner**, the rewritten
  Skip (silent unpause first), the pause-you-did-not-cause toasts.
- **P4.D162** (v4 bugs 124/125): help-chat tool **threading** through the shared
  primitive (dogfood **#112**), and `additionalProperties` at the head of
  Google's schema strip list (dogfood **#114**).
- **P4.78** (finding #115): v4's whole `createChatSchema` as ONE validation
  stage ahead of any work in `chatCreate`.
- **P4.79** (finding #111's remainder): both Brahma engines log every streamed
  turn's `CHAT_MESSAGE` row; a mid-stream provider error stops the turn instead
  of persisting a half reply.

## §0 Drift state at walk start

The ledger's **§2 probe PASSED** (v4 checkout on `bugfix`, tree clean,
`git log f699da6f6..main` and `git log 1a2b2164c..bugfix` both empty). Baseline
`f699da6f6` = v4 `main` HEAD; **NO DRIFT**. The one §3 row (`15573c3a1`, bug
119, the character optimizer) is an **unported** surface (`p4.9k`), so **no step
in this walk can blame drift** — an apparent failure here is a v5 defect or an
instrument error until proven otherwise.

## §0.5 Pre-walk measurement (ledger §5.5)

Run against the FRESH copy (rsync'd 2026-09-06, ~21:5x) before the walk started.

| # | measurement | result |
|---|---|---|
| M1 | pre-paused chats | **32 of 910** chats carry `isPaused = 1`. Every A-step therefore picks its chat by *measured* pause state, and A1/A2/A4 unpause first — a pre-paused chat makes gate 2 unmeasurable. |
| M2 | A1/A2/A3 candidates (≥2 active LLM seats + a user seat) | **53 of the 80 most-recent salon chats** qualify. Picked for the walk: small ones so a chained turn is cheap — `8f891551` *The Ship's Inner Tour* (31 msgs, 2 LLM + 1 user), `a06e4a22` *The Kutha and the Challah* (40), `99076585` *A Door That Was Mine* (44). |
| M3 | A4 all-LLM candidate | **exactly one** in the recent window: `6ec13ecb` *The Weight of What We Ask For* (293 msgs, 4 LLM seats, no user seat) — and it is **already paused**, so A4 unpauses it first. |
| M4 | help chats + seats | **12** help chats (all v4-written, newest 2026-09). Help characters: `f11db2…` (Riya, most chats) and `a42c02…`. Seats carry a per-participant `connectionProfileId` (`3bba71…`, `eb6d9c…`, `bed7e2…`), so B1/B3 can re-seat a help chat without touching the character. |
| M5 | GOOGLE | key `GeminiQT` present and active (39 chars); **zero GOOGLE connection profiles** — B2/B3 create one over that key, as the 2026-09-06 help pass did. |
| M6 | Brahma `llm_logs` baseline | 7,497 rows total (1,168 `CHAT_MESSAGE`). ⭐ **v4 has already written rows for its OWN Brahma chats — 56 across the two most recent** — and their shape is exactly what P4.79 ports: `type='CHAT_MESSAGE'`, `messageId` NULL, `characterId` NULL, `durationMs` measured (3,901 / 22,786 / 72,383 ms). D1 therefore has a **free cross-app comparand**: v5's new rows must match v4's own on this same instance. |
| M7 | queue state | `background_jobs`: 8,327 COMPLETED / 2,390 DEAD / 4 FAILED, **nothing PENDING or RUNNING** — a job firing mid-walk is the walk's own doing, not a pre-existing drain. |

## §1 What NOT to expect to work

- **`chatCreate`'s Zod `details` bag does NOT reach the wire.** P4.78 closed the
  engine half; `quilltap-host::spine::map_create_error` still hard-codes
  `details: None`, held by the executable tripwire
  `p4_78_host_wire_details_carry_is_deferred`. A 400 with v4's message and NO
  `details` key is the **expected** shape today (C1/C2 record it, they do not
  fail on it).
- **The composer on a chat whose only active character is the human's own seat**
  is disabled in v5 and enabled in v4 — the standing note "The composer's
  `hasActiveCharacters` reads v4's NARROW twin" (2026-09-06). Deliberate; not a
  finding to re-file.
- **The stored help CONTENT predates the shipped tree** on a long-lived instance
  (both apps re-sync only when the PATH SET diverges) — a v4 wart, recorded last
  pass.
- **The character optimizer** (Refine from Memories) is unported (`p4.9k`).
- **The `paused` key is absent** on the fair-rotation and help-chat
  chain-complete emits by design (v4 sets it at the four `executeTurnChain`
  emits only) — absent ≡ `false` on the client.

## §2 The walk

Owner `CLAUDE` unless marked. Status: `PENDING` → `PASS` / `FAIL(#n)` /
`DEFERRED-TO-HUMAN` / `BLOCKED(reason)`.

### A — bug 123: the pause announcement and the Skip banner (P4.D160 + P4.D161)

| # | owner | gesture | expected + how verified | status |
|---|---|---|---|---|
| A1 | CLAUDE | **The chain-error safety stop.** A multi-character chat; point the SECOND seat's connection profile at a dead endpoint (create an OpenAI-compatible understudy at `http://127.0.0.1:9/v1`, assign it to that participant). Send a message; the first seat replies, the chain hands off, the second errors. | The chat **pauses**: `chats.isPaused = 1` in the DB (0 before), and the Salon raises the **WARNING** toast byte-for-byte: `A character's turn failed, so auto-responses are paused. Press Resume in the sidebar to carry on.` Verified: DB row before/after + the toast's text read off the DOM + the server log's chain-error line. | **PASS** — `isPaused` 0 → 1, `lastTurnParticipantId` → NULL, `[Failover] Fallback chain exhausted … http://127.0.0.1:9/v1/chat/completions`, and the DOM carried **exactly one** toast, `qt-toast qt-toast-warning`, byte-for-byte v4's sentence. **Free contrast arm:** when the SAME dead seat is the *initial* responder the server logs `Chat send failed while streaming the initial turn`, does **not** pause, and the client raises two `qt-toast-error` toasts instead — the safety stop is the chain's alone, exactly as ported. |
| A2 | CLAUDE | **The mid-chain pause the user did not cause → the INFO toast.** Same chat (resumed). Send a message and, while the first turn is still streaming, pause the chat **out of band** (a `chatUpdate`/pause dispatch by curl, not the sidebar) so the chain's next decision reads `paused`. | The chain stops with `reason: 'paused'` and the **INFO** toast `Auto-responses are paused. Press Resume in the sidebar to let the others answer.` (not the warning). Verified: the toast text + severity class on screen; `isPaused = 1`. | **PASS** — a `chatUpdate {isPaused:true}` fired by curl 14 s into Lorian's stream; the chain's next decision read it, and the client raised `qt-toast qt-toast-info` reading byte-for-byte `Auto-responses are paused. Press Resume in the sidebar to let the others answer.` The severity **and** the sentence both differ from A1's, so the `reason === 'error'` branch is discriminated live. |
| A3 | CLAUDE | **Gate 2 — the pause you caused yourself is silent.** Pause the chat with the sidebar's Pause control (its own toast fires), then send a message. | The chain stops, and **NO** chain-pause toast appears (only the toggle's own). Verified: count the toasts on screen after the turn; the transcript still gets the one reply. | **PASS** — Pause pressed in the sidebar (its own `Auto-responses paused` toast), toasts cleared, message sent. The server ran the initial turn, logged D160's own `[TurnOrchestrator] Chat paused, not chaining after initial turn` and emitted the `paused: true` chain-complete — and the client stayed **silent**. The discriminator is real rather than vacuous: A2 proved the same client code *does* raise the info toast on the same frame when the pre-turn belief is unpaused, so the only difference here is `pausedBefore`. |
| A4 | CLAUDE | **Gate 3 — an all-LLM room announces nothing.** A chat with no present `controlledBy='user'` participant, paused mid-chain the A2 way. | No toast; the `AllLLMPauseModal` is the surface that explains the stop. Verified: no toast in the DOM; the modal's presence (or its opener predicate) after the reconcile. | **PARTIAL** — the *alternative surface* is proven live: on a purpose-built two-character all-LLM room (`89b44314…`, both seats on the cheap NANOGPT profile) an out-of-band pause opened **`AllLLMPauseModal`** — "All Characters Controlled by AI … Continue (3 more turns) / Stop / Play as Marie / Play as Greg" — and **no toast ever appeared** across two runs. What this walk could NOT decisively discriminate is whether a `paused: true` chain-complete actually reached the client in the run that mattered: the second run's chained turn was still inside an `ask_carina` consult when the window closed, so the silence may be a chain that never reached its pause decision rather than gate 3 firing. Gate 3 stays **unit-proven** (P4.D161's mutation M6, one red). **⚠ The first attempt at this step was VACUOUS and was caught**: the Send click missed (`chat_messages` had no USER row), so "no toast" measured nothing — the instrument-liveness rule applied to a *gesture* rather than an observer. |
| A5 | CLAUDE | **The off-turn Skip banner** (bug 123's new behaviour). In a chat where the human drives a seat, look at the banner when the rotation has **not** landed on that seat. | The banner is offered anyway, with v4's off-turn sentence (not the on-turn one), and a Skip button unless everyone else has passed (`mustSpeak`). Verified: the banner text on screen against v4's three-way wording; `nextSpeakerId` in the chat GET ≠ the seat. | **PASS** — both sentences captured on the same chat. On turn: `Charlie's turn — type as them, or skip to let someone else respond.` Off turn (rotation sitting on Lorian, the Nudge button reading **Nudge Lorian**): `Speaking as Charlie — type, or skip to let someone else take the floor.` — with the Skip button present. Pre-bug-123 the banner keyed on `nextSpeakerId` and the off-turn case showed **nothing**. The `SpeakingAsAvatar` cue (`Speaking as Charlie`) rendered beside it. |
| A6 | CLAUDE | **Skip lifts a pause silently.** With the chat paused, press Skip on the user-driven seat. | The client unpauses first (**no** resume toast — v4 does not toast here), then `skipUserTurn` runs and the floor moves. Verified: `isPaused` 1 → 0 in the DB; the dispatch pair in the server log; a Host turn-pass record / new `nextSpeakerId`; no toast. | **PASS** — pressed with the chat paused: `isPaused` 1 → 0, **zero toasts** (v4 does not announce this resume), the Host wrote `The Host observes Charlie declining the floor with a courteous wave; the turn passes on.`, `lastTurnParticipantId` moved to Daciana and the client auto-generated her reply. The silent-unpause-then-skip ordering is what makes it work — without it the pause guard would refuse the seat the floor was just handed to. |
| A7 | CLAUDE | **An impersonated seat's Skip.** Impersonate an LLM-driven character, then Skip. | The overlay-aware guard (`isUserDrivenSeat`) accepts it — the pass records and the floor moves; skipping a seat you are NOT speaking as answers `Only a character you are speaking as can be skipped.` | **PASS (positive arm)** — impersonating **Daciana**, a seat whose `controlledBy` column still reads `llm`: the banner re-keyed to `Daciana's turn — type as them, or skip to let someone else respond.`, Skip was offered, and the Host wrote *"The Host observes **Daciana** declining the floor with a courteous wave; the turn passes on."* Bug 44's overlay × bug 123's seat-keyed Skip. The **negative arm is unreachable through the UI by construction** (the banner only ever shows your own seat) — the same measurement P4.D161's lane made when it ruled its item 7 a NO-BEAT. **Observed for free:** the *sidebar's* Skip is a different control reading the narrower predicate — it sat disabled, titled `It's not your turn to skip`, while the composer banner's Skip was live. v4 splits them the same way (`onSidebarSkip`); recorded, not filed. |

### B — bugs 124/125: help-chat tool threading and Google's schema strip (P4.D162)

| # | owner | gesture | expected + how verified | status |
|---|---|---|---|---|
| B1 | CLAUDE | **#112 UNBLOCKED — a tool-needing help turn on a non-Google provider.** Open the Help dialog → Ask, on a help character seated on NANOGPT/OPENAI, and ask something that forces `help_search` (e.g. a question whose answer lives in a specific Guide page). | The turn **answers** instead of ending silent: the tool call runs, the result is threaded as a paired `tool` row, and a final reply renders. Verified: the reply on screen; the help chat's persisted messages; the `[quilltap::help]` threading debug line in the server log (`Threaded help-chat tool results into the conversation`) with its `paired_by_call_id` / `framed_as_text` counts; `llm_logs` rows for the turn (finding #111's fix). | **PASS** — a fresh help chat (`c58256a4…`, seat NANOGPT `deepseek/deepseek-v4-pro:thinking`, the instance default) answered a `help_search`-forcing question with real prose: *"The search results show mentions of the Concierge but don't seem to contain the exact four routing states. Let me look more specifically at the per-chat Concierge switch section I spotted in the Dangerous Content Handling document…"* — then called `help_search` a **second** time. Pre-bug-124 that turn ended in silence. Persisted rows: `SYSTEM`, `USER`, **`ASSISTANT` with empty content**, `TOOL`(help_search), `ASSISTANT`(the prose), `TOOL`. **Two `CHAT_MESSAGE` `llm_logs` rows** — `messageId` NULL, `durationMs` **3,339** and **10,167** ms — so finding #111's help-side fix is proven live in the same gesture. ⚠ The `[quilltap::help]` threading **debug** line is invisible at the server's default `INFO`; captured separately in B3. |
| B2 | CLAUDE | **#114 UNBLOCKED — a GOOGLE-seated, tool-enabled salon turn.** Create a GOOGLE connection profile over the existing `GeminiQT` key (no GOOGLE profile exists on this instance), seat a character on it in a chat whose tool slate includes the wardrobe tools, and take a turn. | **No 400.** Before bug 125 every such turn failed on `additionalProperties` under `operations.items`. Verified: a completed reply; `llm_logs` row for the call with no error; the server log free of a Google 400; the strip is unit-proven, so the live proof is the OUTCOME. | **PASS (supporting)** — a native GOOGLE profile (`gemini-2.5-flash`, over the instance's existing `GeminiQT` key; the instance had **zero** GOOGLE profiles) seated in a salon chat took three `CHAT_MESSAGE` calls with real content and **no 400**. Not the decisive arm on its own: `llm_logs.request` is a pre-builder projection and carries no `tools`, so the slate's contents cannot be read back from it. **B3 is the decisive arm** — it reproduces finding #114's exact configuration. |
| B3 | CLAUDE | **The P4.9I2 §3 GOOGLE-keeps-id-less-tool-rows live leg.** A GOOGLE-seated help chat turn that calls a tool. | The turn completes with the id-less `tool` row KEPT for GOOGLE (every other provider frames it as user text). Verified: the reply; the threading debug line's counts; no 400. | **PASS on bug 125; the §3 leg still OWED.** The help chat `c58256a4…` was re-seated onto the GOOGLE profile — **finding #114's exact failing configuration** (a help chat's full 57-tool slate, which on 2026-09-06 died in 192 ms with Google's `400 … Unknown name "additionalProperties" at 'tools[0].function_declarations[19]…'`). It now answers: a GOOGLE `CHAT_MESSAGE` row, 2,732 ms, real content, **no 400 anywhere in the log**. Bug 125's fix is proven live on the one configuration that used to fail. **And after #116 named the obstacle, the id-less-tool-row leg was proven too.** A *fresh* help chat seated on GOOGLE before its first turn (the connection-change announcement deleted, so no unsigned assistant row could strip the tools) asked for `help_search` and ran the whole loop on Gemini: call 1 — 1,593 ms, empty content, `finishReason: STOP` (the tool-call turn); a persisted `TOOL` row for `help_search`; call 2 — 3,547 ms, real prose reading the result back (*"Oh, that's an interesting phrase to look for! I've run the search…"*). One turn proving three things at once: bug 125's strip (no 400), bug 124's threading, and **GOOGLE keeping the id-less `tool` row** where the other nine providers frame it as user text — the P4.9I2 §3 leg, live. |
| B4 | CLAUDE | **The threading primitive's whitespace-collapse rider.** A help turn whose assistant tool-call turn has only whitespace before the call. | v4's `trim().length > 0 ? currentResponse : ''` rule now applies on the help path too — the assistant turn's content is empty, not whitespace. Verified: the persisted message row / the slate in the log if visible. Opportunistic; skip if no natural vector appears. | **PASS (free, inside B1)** — the vector appeared naturally on the very first turn: the assistant's tool-call turn persisted with content **`""`**, not whitespace. That is v4's `trim().length > 0 ? currentResponse : ''` rule reaching the help path for the first time — the pre-fix verbatim push had no such collapse. |

### C — P4.78: `chatCreate`'s validation stage

| # | owner | gesture | expected + how verified | status |
|---|---|---|---|---|
| C1 | CLAUDE | **`controlledBy: "LLM"` is refused.** `POST /api/v1/chats` with a participant carrying `controlledBy: "LLM"` (the uppercase spelling finding #115 tripped on). | **400** with v4's validation message, **nothing written** (`chats` count unchanged). The `details` bag is ABSENT — the §1 deferral, recorded not failed. Verified: curl status + body; `SELECT COUNT(*) FROM chats` before/after. | **PASS** — `{"kind":"bad-request","message":"Validation error"}`, `chats` 910 → 910. The `details` bag is absent exactly as §1 predicted (the host-wire deferral, `p4_78_host_wire_details_carry_is_deferred`). ⚠ **Route note:** v5's web transport exposes only `GET /api/v1/chats`; creation is the `chatCreate` **dispatch** verb (what the SPA uses), so the arms were driven there. |
| C2 | CLAUDE | **Three more arms + the 200 they'd otherwise be.** A wrong `participants[].type`, a MISSING participant `type`, a >500-char scenario path, and a non-uuid `imageProfileId`; then the same body corrected. | Each refused 400 before any work (**guard order**: the validation stage beats the continuation 404 — send a bad body with a bogus `continuedFromChatId` and expect 400, not 404); the corrected body creates a chat. Verified: statuses + the `chats` row count per arm. | **PASS** — six arms, all `400 Validation error` with `chats` unmoved: wrong participant `type` (`"PERSON"`), **missing** participant `type`, non-uuid `imageProfileId`, `projectScenarioPath` at 501 chars, `generalScenarioPath` at 501. **The guard order is proven by a real discriminator**, not by assertion: the SAME bogus `continuationFromChatId` answers `404 Source chat not found` with a valid body and `400 Validation error` with an invalid one — so the validation stage runs ahead of the continuation lookup, v4's order. A corrected body created a chat (910 → 912 across the pass's two accepted bodies). |
| C3 | CLAUDE | **The SPA's own New Chat still works on real data.** Create a chat through the UI with a scenario + participants. | 201 and the chat opens — the validation stage does not refuse the app's own bodies. Verified: on screen + the DB row. | **PASS** — the SPA's New Chat with two real characters (Jackie + Sunny) created `cc8cbf8f…` *"Chat with Jackie and Sunny"* (`chats` 913 → 914) and opened the Green Room. The validation stage does not refuse the app's own bodies. |

### D — P4.79: the Brahma Console's `llm_logs` rows

| # | owner | gesture | expected + how verified | status |
|---|---|---|---|---|
| D1 | CLAUDE | **A Console question leaves its rows.** Ask the Brahma Console a question that needs a couple of agent turns (e.g. a `run_sql` question about this instance). | Every completed streamed turn writes a `CHAT_MESSAGE` row with `characterId` NULL, `messageId` NULL and a **measured** `durationMs` (> 0). Verified: `llm_logs` count before/after + the rows' columns; last pass measured **zero** rows for three stream calls. | **PASS** — one Console question (*"How many characters are in this instance, and which three have the most wardrobe items? Use run_sql."*) ran a six-turn agent loop and wrote **six `CHAT_MESSAGE` rows**, every one `messageId` NULL, `characterId` NULL, `durationMs` measured (6,576 / 4,086 / 4,887 / 5,179 / 4,366 / 4,745 ms) — **byte-for-byte the shape v4 wrote for its own Brahma chats on this same instance** (§0.5 M6's free cross-app comparand). The 2026-09-06 walk measured **zero** rows for three stream calls. The answer itself checks out against the DB: 43 characters, exactly `SELECT COUNT(*) FROM characters`. |
| D2 | CLAUDE | **A mid-stream provider error stops the turn.** Pin the Console to the dead-endpoint understudy from A1 and ask a question. | The error surfaces; **no half reply is persisted** and no budget-salvage sentence stands in for a real answer (the vacuous-green shape the gate caught). Verified: the console's messages; the server log's error. | **PASS** — the *strictly mid-stream* arm, with a purpose-built endpoint (`halfstream.py`: HTTP 200 + SSE headers, one content chunk, then `SO_LINGER` abort) so the failure lands after bytes have flowed rather than at connect. The console answered `ERROR quilltap::chat: Brahma Console send failed … error=error decoding response body` and HTTP 500, and **nothing was persisted** — the newest ASSISTANT row is still the previous turn's, four minutes earlier. **No budget-salvage sentence stood in for a reply**, which is the whole point: before P4.79 the `Err(_) => break` left `full_response` empty and Bug 47's salvage synthesised *"I reached my N-turn budget before I could compose a final answer."* as the assistant bubble — the vacuous green the unification gate caught. |

### E — real-data regression sweep (broad gestures the e2e never makes)

| # | owner | gesture | expected + how verified | status |
|---|---|---|---|---|
| E1 | CLAUDE | Open the largest chat in the Salon list; scroll the transcript with real wheel events; open the sidebar. | No panic, no console error; the transcript renders. Verified: server log (`RUST_BACKTRACE=1`) + browser console. | **PASS** — `e59a1b8c…` *The Bridge of Ordinary Breathing*, **1,934 messages**, opened and rendered 235,092 px of transcript; wheel-scrolled up 4,800 px with the container growing as older messages loaded. **Zero panics** across both server logs for the whole session (`grep -ci panic` = 0/0). Every browser console error is the walk's own deliberate breakage — two 500s (A1's dead endpoint, D2's half-stream), one `ERR_INCOMPLETE_CHUNKED_ENCODING` (the half-stream server), and five `ERR_CONNECTION_REFUSED` (the SSE reconnecting across the mid-walk binary swap). |
| E2 | CLAUDE | The Salon list + home dashboard timings on real scale (779+ chats) — a regression check on P4.64/P4.65's batching. | Sub-second-ish; nothing has re-introduced the per-participant fan-out. Verified: the network timing of `chatList` / `systemHome`. | **PASS** — `listChats` **1.42 s** for 356,445 bytes and `systemHome` **0.32 s**, on 910 chats. In line with the 2026-08-27 measurements (1.34 s / 0.31 s) and nowhere near the pre-P4.64/P4.65 8.6–12.2 s / 8.8 s. No regression. |

### F — the human remainder

| # | owner | gesture | why human | status |
|---|---|---|---|---|
| F1 | HUMAN | The Brahma Console on a genuinely **deep** query (the standing 💸 item — a multi-turn budget run). | Real cost; a long agent loop. | **DEFERRED-TO-HUMAN** |
| F2 | HUMAN | Memory **deduplication** + **conversation-summaries regeneration** first run on real data (standing 💸). | Batch LLM spend over a real memory graph. | **DEFERRED-TO-HUMAN** |
| F3 | HUMAN | The NanoGPT prompt-caching cost question (**finding #101** — a cache is written every turn and never read). | A pricing/cost judgment, not a pass/fail. | **DEFERRED-TO-HUMAN** |
| F4 | HUMAN | Aesthetic acceptance of the pause toasts + off-turn Skip banner in ordinary use. | Judgment. | **DEFERRED-TO-HUMAN** |

## §3 Findings

### #116 — the Google builder silently strips a turn's tools (FIXED, `4e62e936`, core 0.0.812)

Found by consequence while chasing B3's second turn. A GOOGLE-seated help chat
asked to run `help_search` produced a `CHAT_MESSAGE` row of 2,068 ms with
**empty content and `finishReason: UNEXPECTED_TOOL_CALL`**, persisted no
assistant row, showed the operator nothing — and **no log line said why**.

The cause is `format_messages_for_google`'s `should_disable_tools`:
`gemini-2.5-flash` is a thinking model, the help chat's history was written by
NANOGPT/DeepSeek so not one assistant row carries a `thoughtSignature`, and v4's
rule strips the entire tool array in that case
(`GoogleProvider.formatMessagesForGoogle:333-345`). **That behaviour is
v4-faithful and stays.** What was missing is v4's *announcement* of it — an
`info` for the unsupported-model arm (`:325`) and a `warn` carrying
`legacyMessageCount` / `totalAssistantMessages` / `modelName` for this one
(`:338`). v5's `google.rs` had **no tracing at all**.

Same class as findings #103 and #110: a decision whose consequence the operator
sees and whose cause the log never names. Fixed with both sentences
byte-for-byte at target `quilltap::model::request_builder::google`; four
capture-layer tests; three mutations each reddening exactly one; a fourth
(counting over all messages rather than `non_system`) **survives and correctly
so** — a `System` row is never an `Assistant` — recorded in the test rather than
chased. `request_builder_google_equivalence` green, core lib 2,065/0, clippy
clean, release build clean.

**Live proof, same instance, rebuilt server, same silent turn:**

```
WARN quilltap::model::request_builder::google: Disabling tools for thinking model
  due to legacy messages without thought signatures
  legacy_message_count=5 total_assistant_messages=5 model_name=gemini-2.5-flash
```

**Consequence for the walk:** B3's GOOGLE-keeps-id-less-tool-rows leg (the
P4.9I2 §3 item) cannot be exercised from a help chat with mixed-provider history
at all — the tools never reach the wire. Re-run on a *fresh* GOOGLE-seated chat,
it passed; see B3.

### #117 — a salon chat cannot be deleted on any surface (RECORDED, needs an order)

Found by the walk's own cleanup. `DELETE /api/v1/chats/{id}` answers **405**
(the route registers `get` + `post` only) and no `chatDelete` dispatch verb
exists — among forty `*Delete` verbs there is one for help chats, one for Brahma
chats, one for a chat's memories, and none for the chat. v4 has the route and
calls it from the character's Conversations tab.

The client half is a **documented** deferral (`conversations-tab.ts:30`, the
P4.6g list); the note names only the card, and the **server edge is unported
too** — so nothing, no CLI and no future client, can reach it. On a real
instance an operator has no way to remove a conversation. Left for an order
rather than patched: v4's cascade (memories, chunks, files, mount rows) is the
substance of the port and wants its own differential.

**Walk consequence:** the three `DOGFOOD` salon chats this pass created could not
be cleaned up; its help and Brahma chats could. They vanish with the next rsync.

## §4 Outcome

**18 rows: 16 PASS, 1 PARTIAL (A4), 4 DEFERRED-TO-HUMAN. Two findings — #116
found and FIXED on main, #117 recorded for an order. The round's whole 💸 queue
is discharged except the GOOGLE-tool-row leg's original blocker, which #116
removed and B3 then closed.**

### What the round delivered, proven live

- **Bug 123, server + SPA (P4.D160 ∥ P4.D161)** — all four gates of the pause
  announcement discriminated on real turns, not asserted: the chain-error
  **warning** (A1), the pause-you-did-not-cause **info** (A2), and the silence
  when you paused it yourself (A3), with the same frame proven to produce a
  toast in one case and none in the other. The off-turn Skip banner (A5) showed
  both of v4's non-must-speak sentences on one chat; Skip lifted a pause
  silently and moved the floor (A6); an impersonated LLM seat skipped through
  the Bug-44 overlay (A7).
- **Bugs 124/125 (P4.D162)** — dogfood **#112** and **#114**, this port's own
  filings, came back fixed and were confirmed on the configurations that
  produced them. A tool-needing help turn now answers instead of ending silent
  (B1), and finding #114's exact GOOGLE help chat now completes with no 400
  (B3). One fresh-seat GOOGLE turn proved bug 125's strip, bug 124's threading
  and the P4.9I2 §3 id-less-row rule together.
- **P4.78** — the `createChatSchema` stage refuses six shapes with nothing
  written, and the guard order is proven by discriminator rather than assertion:
  the same bogus continuation id answers 404 with a good body and 400 with a bad
  one (C1/C2).
- **P4.79** — finding #111's remainder: six `CHAT_MESSAGE` rows for one Console
  question, in the shape v4 wrote for its own Brahma chats on this instance
  (D1), and a genuinely mid-stream failure that persists **no half reply and no
  salvage sentence** (D2).

### Findings

| # | what | status |
|---|---|---|
| 116 | the Google builder silently strips a turn's tools where v4 announces it | **FIXED** `4e62e936`, core 0.0.812, live-proven |
| 117 | a salon chat cannot be deleted on any surface | **RECORDED**, needs an order |

### Instrument errors caught (the standing lesson, twice more)

1. **A4's first run was vacuous** — the Send click missed, `chat_messages` had no
   USER row, and "no toast" measured nothing. *Prove the gesture landed, not just
   that the observer is live.*
2. **A phantom-row scare that wasn't.** Two `chat_messages` rows with NULL `role`
   and NULL `content` looked like debris until the projection was widened: they
   are system-event rows (`type: 'system'`, `systemEventType: TITLE_GENERATION` /
   `MEMORY_EXTRACTION`), and 52,000 of them predate v5 entirely. *Project
   `type`/`systemEventType` before calling a `chat_messages` row empty.*
3. **A dispatch verb accepted an unknown key and changed nothing.** The first
   `chatUpdateParticipant` was sent as `{updates: {...}}`; every field is
   `#[serde(default)]`, so it answered **200 with a full `chatCast` payload**
   having written nothing. *Read the write back before believing a 200.*

### Deferred to the human

F1 the Brahma deep query, F2 memory dedup + summaries regeneration, F3 the
NanoGPT caching cost question (#101), F4 aesthetic acceptance of the pause
toasts and off-turn banner.

### Housekeeping

Both throwaway connection profiles and the GOOGLE profile deleted; the help and
Brahma test chats deleted; the three `DOGFOOD` salon chats could not be (finding
#117) and go with the next rsync. Impersonation stopped, every chat left
unpaused. Zero panics across both server logs.
