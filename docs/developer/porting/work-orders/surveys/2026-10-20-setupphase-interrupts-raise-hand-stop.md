# Survey — raise a hand / stop from the composer, whole (`2b5e8ee24` + the CSS rider `b8362beb5`; server + SPA)

Read-only planning survey, **2026-10-20**. v4 = `~/source/quilltap-server`,
read ONLY through `git show 322a5488e:<path>` (**THE PIN** = v4 `main`
`322a5488e7bd93f0234a25b42f87d5418dbf4c2c`, `4.11.0-dev.53`, the tree CLEAN
at the pin) and `git show <sha> -- <paths>`. **The oracle baseline v5 has
ported is `e8e17b2d9`**; v5 = `main` at **`5a94a9d1e`**
(`/Volumes/External 1/source_overflow/quilltap-v5`). Every v4 line number
below is AT THE PIN; every v5 line number is at `5a94a9d1e`. Nothing is taken
from commit prose (ledger §5.3) — every claim is from a shipped hunk. The
ledger rows `2b5e8ee24` (`drift-ledger.md:416`) and `b8362beb5` (`:420`) were
read first and are built on; where a hunk disagrees with a row this survey
says so (§8).

**Measured: nothing past these two commits touches the slice before the pin.**
`2b5e8ee24` → `cc57e6cbc` (docs) → `c56fe278c` (`edit_custom_tool`) →
`a53024d3c` (any-of gates) → `b8362beb5` (this slice's CSS rider) →
`d88c1ce39` (Inventory hint) → `322a5488e` (bug 257): none of the four
non-slice commits touches `lib/chat/turn-control.ts`, `turn-manager/`,
`stream-watchdog.ts`, `services/chat-message/`, `host-notifications/writer.ts`,
`actions/interrupt.ts`, `ChatComposer.tsx`, `useSSEStreaming.ts` or
`_chat.css` — so "at the pin" = "at `b8362beb5`" for every file here (the
`git show 322a5488e:` line numbers below prove it: every hunk sits where the
two diffs put it).

## Headline findings (written last)

1. **PORT-NEW on the ported chat spine, NO D23 move, NO new dependency** — but
   the server half is NOT a bolt-on: v4 threads one ambient handle
   (`AsyncLocalStorage`) through TEN production files, and v5 has NO ambient
   handle and NO single `streamMessage` funnel — **v4's one `abortSignal:
   currentTurnStopSignal()` call site (`streaming.service.ts:458`) is SEVEN
   `watch_stream(...)` call sites in v5** (§2.3) plus five that must NOT get it.
2. **Where the in-flight turn lives in v5:** the whole send runs on a dedicated
   OS thread + a current-thread tokio runtime (`spine.rs:2526-2551`), so a
   tokio `AbortHandle`/`JoinHandle::abort` cannot reach it; the stop must be a
   COOPERATIVE signal raced inside `WatchedStream::recv`
   (`stream_watchdog.rs:209-266`) and polled at v4's eleven guard sites. The
   default core build has `tokio = { features = ["sync","time"] }` — no
   scheduler, no `macros`, no `tokio-util` — so the race is `futures_util`
   `select`/`poll_fn` over `tokio::sync::Notify`/`watch`, not `tokio::select!`
   and not `CancellationToken` (§2.6).
3. **The registry needs a home the ENGINE can reach:** v4's `processSingleton`
   is a process-wide map; the two action arms run in the engine while the
   stream runs in the host's spine thread. Two precedented homes (§2.6): a
   `static` in core (the `activity_registry.rs` precedent — v4's
   `globalThis`/`processSingleton` twin) or two new methods on the
   `ChatSendDriver` trait implemented by the host's `ChatSpine`. A ruling (§8).
4. **The yield note is model-visible and lands LAST** — after the turn-skip
   note — in v5's ONE `trailing_context_sections` list
   (`build_context.rs:3062-3072`); it is NEVER persisted; the operator name is
   `userCharacter?.name || 'The User'` (`context-manager.ts:2207`).
5. **A yield does NOT advance the rotation** (unlike a skip): `handleTurnYield`
   (`orchestrator.service.ts:2383-2432`) posts the Host note and emits a
   `done` with `skipped: true, skippedParticipantId, yielded: true` but never
   touches `spokenThisCycleParticipantIds` / `cycleOrderParticipantIds` — the
   §1.11 event ORDER differs between a bare yield (hostAnnouncement → done)
   and a yield-with-words (finalizer's done → hostAnnouncement).
6. **Eleven guard sites** on the server (`isCurrentTurnStopped()`), each with
   its v5 twin named in §2.4; the finalizer's four are ONE boolean read once
   at `message-finalizer.service.ts:96`, not four reads.
7. **The SPA's Stop is a local overlay clear today** (`salon-conversation.ts:
   4222-4232`, "tracked deferral: a real stop dispatch") and v5's participant
   card STILL carries the Stop v4 removed (`participant-card.ts:288-298`):
   the SPA lane removes one button, adds two, and turns `stop()` into a
   dispatch + the 3 s local fallback.
8. **The oracle's queued streaming providers deliver whole canned sequences
   into a pre-filled channel** (`orchestrator_tier3_equivalence.rs:1178+`), so
   a MID-STREAM stop cannot be planted with them as they are — the tier-3 stop
   corpus needs a gate-able provider (§4.3); the e2e mock already has the
   `delayMs` knob (`mock-llm.ts:21-27`).
9. **Two SSE additions the SPA reducer must learn:** a NEW frame
   `{handAnswered: true, participantId, characterName, yielded}` and two new
   `done` flags (`yielded`, `stopped`) + two new `chainComplete` reasons
   (`yielded` paused:false, `stopped` paused:true) — the binding Shared
   contract (§5.3).
10. **The storybook package is NO-PORT** (v5 has no `packages/theme-storybook`
    twin — `apps/web/` has no storybook; §1.17), `help/interrupting.md` is the
    vendor survey's, and `docs/developer/API.md` is docs-only.

---

## 0. The span for this slice, in one paragraph

v4 `2b5e8ee24` (2026-10-20, `dev.47` → `dev.48`, 56 files +1,823 / −206 incl.
~390 lines of tests) gives the operator two ways of cutting in on a running
Salon stream. A NEW in-process registry (`lib/chat/turn-control.ts`, 189
lines) registers every open Salon stream per chat for its lifetime under an
`AsyncLocalStorage` handle carrying an `AbortController` + a `handRaised`
flag; two NEW chat actions reach it — `?action=raise-hand` flips the flag on
every open stream, `?action=stop-turn` pauses a rotation room FIRST, aborts
every open stream, then waits (15 s bound) for them to settle. The soft path:
the next LLM-voiced turn built after the raise consumes the hand and gets a
trailing Turn note inviting `[YIELD]`; the raw reply is read for the sentinel
(a NEW `yield-signal.ts`, sharing a NEW `matchesSentinelLine` with the
`[NOTHING TO ADD]` matcher); a yield posts a Host `turn-yield` message, emits
a NEW `handAnswered` SSE frame, and ends the chain with `chainComplete`
`reason: 'yielded', paused: false`; a decline emits the same frame with
`yielded: false` and the turn proceeds. The hard path: the watchdog gains an
`abortSignal` raced against the next chunk so the provider stream ENDS
quietly (no error), the streamed prose is kept as the reply, reasoning is
dropped, the two tool loops halt before their next call, empty-response
recovery is skipped, the finalizer skips answer confirmation / dice /
reply-triggers / Carina, no sentinel is read from a cut-off line, and the
chain closes with `reason: 'stopped', paused: true` (an initial stopped turn
with nothing streamed emits a `done` with `stopped: true`). The SPA composer
gains Send + an amber raised-hand + a red stop-sign octagon (the sidebar
card's Stop is REMOVED), `useSSEStreaming` gains `handRaised` / `stopping` /
`toggleRaisedHand` and a server-first `stopStreaming` with a 3 s local
fallback, two new icons (+ Madman's Box overrides), and
`.qt-chat-interrupt-button`. `b8362beb5` (`dev.51`) then stacks the three
buttons vertically at every width (2.5 rem each). Help: `interrupting.md` NEW
+ three pages edited; `docs/developer/API.md` documents the two actions.

---

## 1. v4 AT THE PIN — file by file (hunk-by-hunk inventory)

### 1.1 `app/api/v1/chats/[id]/actions/interrupt.ts` (NEW, 62 lines — whole file)

- `:1-10` header: both handlers "act on the in-process registry in
  `lib/chat/turn-control.ts`; the stream itself does the saving and closing,
  so these handlers only flip state and (for a stop) wait for the stream to
  finish putting its partial reply away."
- `:32-51` `handleStopTurn(chatId, chat, { repos })`:
  - `:37` `const pause = chat.chatType !== 'autonomous' && isMultiCharacterChat(chat.participants)`
    (`isMultiCharacterChat` from `@/lib/chat/turn-manager` — v5 twin
    `crate::participant_filters::is_multi_character_chat`).
  - `:38-40` `if (pause && !chat.isPaused) await repos.chats.update(chatId, { isPaused: true })`
    — the pause is written BEFORE the abort (so the chain's next decision, which
    re-reads the flag, sees it). Note the gate is `!chat.isPaused` on the row
    the route loaded (`handlers/post.ts:78`), so a chat already paused is not
    re-written (no `updatedAt` churn).
  - `:42` `const result = await stopActiveTurns(chatId)` (default 15 s settle).
  - `:43-48` INFO `[Interrupt] Stop handled` `{ chatId, paused: pause, stopped: result.stopped, settled: result.settled }`
    — NOTE `paused` logs the PREDICATE `pause`, not whether a write happened.
  - `:50` `successResponse({ stopped: result.stopped, settled: result.settled, paused: pause })`
    — `successResponse` (`lib/api/responses.ts:63-68`) is the BARE object as
    the body, status 200 (`NextResponse.json(data)`). Key order
    `stopped, settled, paused`.
- `:58-62` `handleRaiseHand(req, chatId)`:
  - `:59` `const { raised } = raiseHandSchema.parse(await req.json().catch(() => ({})))`
    — ANY unparsable / absent body is `{}` → `raised: true` (the `prefault`);
    a parsable body with a non-boolean `raised` throws a ZodError out of the
    handler (the route's generic error path — §8 Q4).
  - `:60` `const streams = setHandRaised(chatId, raised)`.
  - `:61` INFO `[Interrupt] Hand state changed` `{ chatId, raised, streams }`.
  - `:62` `successResponse({ raised: raised && streams > 0, active: streams > 0 })`.
    Key order `raised, active`. Lowering with nothing running answers
    `{ raised: false, active: false }`; raising with nothing running
    `{ raised: false, active: false }` too (the SPA toasts on `active === false`).

### 1.2 `app/api/v1/chats/[id]/actions/index.ts:36`

`export { handleStopTurn, handleRaiseHand } from './interrupt';` — placed
after the `./turn` export, before `./avatars`.

### 1.3 `app/api/v1/chats/[id]/handlers/post.ts:91-92`

The thunk map (`dispatchAction(req, { … })`, no fallback) gains, in literal
order between `turn` and `add-participant`:

```
    'stop-turn': () => handleStopTurn(chatId, chat, ctx),
    'raise-hand': () => handleRaiseHand(req, chatId),
```

`chat` is the row `:78` loaded (`notFound('Chat')` first). ⚠ At the pin the
map ALSO carries the UNPORTED Inventory keys `inventory-equip` /
`inventory-unequip` / `inventory-reset-defaults` between `wardrobe-rollback`
and `toggle-avatar-generation` (`:112-114`, the Inventory survey's) — the
`CHAT_POST_ACTIONS` order constant in v5 (§2.5) is a meeting point of this
lane and the Inventory lane (§5.3).

### 1.4 `app/api/v1/chats/[id]/schemas.ts:170-173`

```ts
/** Soft interrupt: raise (default) or lower the operator's hand. */
export const raiseHandSchema = z.object({
  raised: z.boolean().prefault(true),
});
```

Zod 4 `prefault(true)`: an ABSENT key (or `undefined`) becomes `true` BEFORE
validation; `null` is NOT absent → a ZodError (`Invalid input: expected
boolean, received null`). No `.strict()` — extra keys are stripped.

### 1.5 `lib/chat/turn-control.ts` (NEW, 189 lines — whole module)

- `:1-31` header. Key sentences: "Each open stream registers a handle here for
  its lifetime (`runWithTurnControl`), and the handle is also the ambient
  `AsyncLocalStorage` store, so code deep in the turn (`streamMessage`, the
  tool loops, the finalizer) asks about *its own* stream without a chatId
  lookup — a stop aimed at a stream that is still winding down never touches
  a fresh send made a moment later." / "The HTTP actions … run in a different
  route bundle from the SSE stream, so the registry is a `processSingleton`
  (bug 235). It is in-process only: autonomous-room turns run in the job
  child, where nothing ever stops or interrupts them."
- `:38-45` `TurnControlEntry { chatId, controller: AbortController, handRaised: boolean, settled: Promise<void>, settle }`.
- `:47-55` `state = processSingleton('chat.turnControl', () => ({ byChat: new Map<string, Set<TurnControlEntry>>(), als: new AsyncLocalStorage() }))`.
- `:62-93` `runWithTurnControl(chatId, fn)`: mint the entry (`handRaised:
  false`), add to `byChat[chatId]` (creating the set), DEBUG
  `[TurnControl] Stream registered` `{ chatId, openStreams }`; `await
  state.als.run(entry, fn)`; `finally`: delete the entry, delete the chat key
  when the set empties, `entry.settle()`, DEBUG `[TurnControl] Stream
  unregistered` `{ chatId, stopped: entry.controller.signal.aborted, openStreams }`.
  A throwing `fn` still unregisters (test `:46`).
- `:100-102` `currentTurnStopSignal(): AbortSignal | undefined` — the ambient
  store's signal, `undefined` outside a registered stream ("Carina's own
  surfaces, the Console, autonomous rooms in the job child").
- `:105-107` `isCurrentTurnStopped(): boolean` — `store?.controller.signal.aborted === true`.
- `:114-120` `takeRaisedHand(): boolean` — false when no store, hand down, or
  ABORTED; else clears `handRaised`, DEBUG `[TurnControl] Raised hand consumed
  by the next speaker` `{ chatId }`, returns true. **At most once per raise.**
- `:123-125` `hasActiveTurn(chatId)` — `byChat.get(chatId)?.size > 0` (exported, used by no production caller at the pin — only the test).
- `:132-143` `setHandRaised(chatId, raised): number` — for every entry of the
  chat NOT aborted: `entry.handRaised = raised`, count++; DEBUG
  `[TurnControl] Hand state set` `{ chatId, raised, streams: count }`; returns
  count (0 when the chat has no set).
- `:145-151` `StopActiveTurnsResult { stopped: number; settled: boolean }`.
- `:158-189` `stopActiveTurns(chatId, { settleTimeoutMs? } = {})`:
  - no entries → DEBUG `[TurnControl] Stop requested with no open stream`
    `{ chatId }`, return `{ stopped: 0, settled: true }`;
  - for each entry (snapshot `Array.from`): `handRaised = false`, `controller.abort()`;
  - INFO `[TurnControl] Stopping open Salon stream(s)` `{ chatId, streams }`;
  - `timeoutMs = settleTimeoutMs ?? 15_000`; `Promise.race([Promise.all(settled…).then(() => true), timedOut(false)])`, timer cleared;
  - not settled → WARN `[TurnControl] Stopped stream(s) did not settle in time` `{ chatId, timeoutMs }`;
  - return `{ stopped: entries.length, settled }`.
  The snapshot means a stream registered AFTER the stop is untouched (test `:100`).

### 1.6 `lib/chat/turn-manager/yield-signal.ts` (NEW, 114 lines — whole module)

- `:21-22` imports `normalizeContentBlockFormat, stripCharacterNamePrefix` from
  `@/lib/llm/response-normalizer` and `matchesSentinelLine` from `./skip-signal`.
- `:25` `YIELD_SENTINEL = '[YIELD]'`; `:28` `TURN_YIELD_SYSTEM_KIND = 'turn-yield'`;
  `:30` `YIELD_PHRASE = 'yield'`.
- `:33` `LEADING_BRACKETED_YIELD = /^\s*[*_~"'`]*\[\s*yield\s*\][*_~"'`]*[.!?,;:]*\s*/i`
  — a BRACKETED yield at the head of a line, with optional wrapper chars and
  trailing punctuation, then whitespace. ⚠ V8 semantics the Rust twin must
  reproduce: `\s` is Unicode whitespace (incl. U+FEFF, U+00A0, U+2028/9), the
  `i` flag is simple case folding (`yield` ASCII — but `K`/`ſ`
  folding does NOT apply without `u`), and `.match()` returns the FIRST match
  at `^`.
- `:35-37` `DetectYieldResult = { yielded: true; cleaned: string } | { yielded: false }`.
- `:46-92` `detectYieldSentinel(response, characterName?, aliases?)`:
  1. `!response` → `{ yielded: false }` (empty string only — `'   \n  '` passes this gate and falls out at step 4).
  2. `normalizedRaw = normalizeContentBlockFormat(response)`; strip the name
     prefix ONLY when `characterName || aliases?.length` (same guard as
     `detectSkipSentinel` — "or the generic bracketed-name fallback eats
     `[YIELD]` itself").
  3. split on `'\n'`; `firstIdx` / `lastIdx` = first / last line whose
     `trim().length !== 0`.
  4. none → `{ yielded: false }`.
  5. `without(idx)` = all lines but `idx`, joined `'\n'`, `.trim()`.
  6. `matchesSentinelLine(lines[firstIdx], 'yield')` → `{ yielded: true, cleaned: without(firstIdx) }`.
  7. else if `lastIdx !== firstIdx && matchesSentinelLine(lines[lastIdx], 'yield')` → `{ yielded: true, cleaned: without(lastIdx) }`.
  8. else `leading = lines[firstIdx].match(LEADING_BRACKETED_YIELD)` →
     `{ yielded: true, cleaned: [before…, lines[firstIdx].slice(leading[0].length), after…].join('\n').trim() }`.
  9. else `{ yielded: false }`.
  Note steps 6/7 accept a BARE `yield` word on its own line (`matchesSentinelLine`
  makes brackets optional) — `'YIELD'` alone yields (test `:18`); a bare
  `yield` INSIDE prose does not (step 8 needs brackets; tests `:43-46`).
- `:101-114` `buildTurnYieldInstruction(operatorName)` — VERBATIM (the bytes
  v5 must match; backtick template, so the newlines are literal):

```
[Turn note from the Salon — not spoken by any character]
${operatorName} has raised a hand, wishing to interject before you take your turn.
You may yield the floor to them, or carry on as though you had not noticed.

To yield, reply with exactly this line:

[YIELD]

You may follow it with a brief in-character line (a gesture inviting them to
speak, say), but keep it short — the floor will be theirs. To carry on
instead, write your reply as normal and ignore this note entirely; do not
mention the raised hand unless you choose to acknowledge it in character.
```

  (The em-dashes are U+2014; `${YIELD_SENTINEL}` is interpolated on its own line.)

### 1.7 `lib/chat/turn-manager/skip-signal.ts:142-171` (the shared matcher)

`isSentinelLine(line)` (`:142-144`) now delegates to a NEW exported
`matchesSentinelLine(line, phrase)` (`:151-171`): the SAME body as before (the
repeated wrapper-shedding loop, one `[` / `]` strip from each end, trailing
`[.!?,;:]+` strip) with the final compare `s.toLowerCase() === phrase`. The
`[NOTHING TO ADD]` behaviour is byte-unchanged (phrase `'nothing to add'`).
`lib/chat/turn-manager/index.ts:65-73` re-exports `YIELD_SENTINEL`,
`TURN_YIELD_SYSTEM_KIND`, `detectYieldSentinel`, `buildTurnYieldInstruction`,
`DetectYieldResult` (the orchestrator imports `detectYieldSentinel` from the
barrel, `orchestrator.service.ts:24`).

### 1.8 `lib/llm/stream-watchdog.ts` (the `abortSignal` race)

- `:75-80` `StallWatchdogOptions.abortSignal?: AbortSignal` — "When it fires
  the stream simply ends — no error, so no fallback or recovery reads it as a
  provider failure — and the caller keeps whatever it has accumulated."
- `:99-100` `let stopped = false; const abortSignal = options.abortSignal`.
- `:105-111` `STOPPED = Symbol('stopped')`; `stopRequested` = a promise that
  resolves `STOPPED` immediately if `abortSignal.aborted`, else on the
  `'abort'` event (`{ once: true }`); `null` without a signal.
- `:117-129` per iteration: `next = withTimeout(iterator.next(), budgetMs, …stalled error…)`;
  `result = stopRequested ? await Promise.race([next, stopRequested]) : await next`;
  on `STOPPED`, `next.catch(() => {})` (the abandoned `next()` may reject later).
  The STALL budget still runs underneath: whichever settles first wins.
- `:145-155` `if (result === STOPPED) { stopped = true; logger.info('[LLMStream] Abandoned a provider stream at the operator\'s stop', { ...options.logContext, provider, modelName, chunksReceived, elapsedMs: Date.now() - startedAt }); return }`
  — INFO (the stall line is WARN); key order `logContext` spread FIRST, then
  `provider, modelName, chunksReceived, elapsedMs`.
- `:163-175` `finally`: `unwound = Promise.resolve(iterator.return?.()).catch(() => {})`;
  `if (stalled || stopped) void unwound; else await unwound` — the source may
  be parked mid-await on a stop exactly as on a stall, so its `return()` is
  NOT awaited.
- Observable contract (tests `:139-186`): mid-stream stop → the consumer loop
  ENDS normally with what arrived; a stop while waiting on the first chunk →
  ends at once (< 1 s against a 5 s budget); already-aborted before start →
  yields nothing; no stop → untouched.

### 1.9 `lib/chat/context-manager.ts` (the yield NOTE's text + placement)

- `:159` `import { buildTurnYieldInstruction } from '@/lib/chat/turn-manager/yield-signal'`.
- `:550-555` `BuildContextOptions.offerYield?: boolean` — "Trails every other
  section, the skip note included. Never persisted."
- `:2206-2214`:
  ```ts
  const turnYieldInstruction = options.offerYield
    ? buildTurnYieldInstruction(userCharacter?.name || 'The User')
    : ''
  if (turnYieldInstruction) {
    logger.debug('[TurnYield] Offering the yield note to the next speaker', {
      chatId: chat.id,
      participantId: respondingParticipant?.id,
    })
  }
  ```
  `userCharacter` is the user-controlled character the context already
  resolved (the persona); the fallback literal is `'The User'` (capital T).
- `:2259-2270` the trailing-sections array gains `turnYieldInstruction` as its
  LAST element, after `turnSkipInstruction` — i.e. `[…, inventoryLLMContext,
  informBlock, turnSkipInstruction, turnYieldInstruction].filter(Boolean)`.
  Both notes can ride the SAME turn (a skip-eligible turn with a raised hand
  gets both, skip first).
- `lib/services/chat-message/context-builder.service.ts:112-116` `offerYield?:
  boolean` on `BuildMessageContextOptions`; `:1129-1130` plumbed to `buildContext`.

### 1.10 `lib/services/chat-message/orchestrator.service.ts` (334 lines changed)

- `:24` import `detectYieldSentinel` (barrel); `:26` `runWithTurnControl,
  takeRaisedHand, isCurrentTurnStopped`; `:28` `postHostTurnYieldAnnouncement`;
  `:69` `encodeHandAnsweredEvent`.
- `:218-287` `handleSendMessage`'s `ReadableStream.start` body is wrapped
  WHOLE in `await runWithTurnControl(chatId, async () => { try { processMessage;
  executeTurnChain; scene tracking; conversation render; safeClose } catch {
  handleStreamError } })` — the first turn, the chain, AND the post-chain
  triggers run under one handle. Indentation-only elsewhere.
- `:659-674` (after the paused-hold decision, BEFORE the Prospero re-injection):
  ```ts
  const offerYield = characterParticipant.controlledBy !== 'user'
    && !holdForPausedChat
    && takeRaisedHand()
  if (offerYield) {
    logger.info('[TurnYield] Asking the next speaker whether to yield to the raised hand', {
      chatId, characterId: character.id, participantId: characterParticipant.id,
    })
  }
  ```
  Short-circuit order matters: a user-controlled seat or a held turn does NOT
  consume the hand (it stays up for the next LLM turn).
- `:1295-1296` `offerYield` passed into `buildMessageContext`.
- `:1759-1776` inside the terminal `try`, FIRST thing:
  ```ts
  const turnStopped = isCurrentTurnStopped()
  if (turnStopped) {
    streamingState.reasoningContent = undefined
    streamingState.reasoningSegments = []
    logger.info('[TurnStop] Turn stopped by the operator; keeping what streamed', {
      chatId, characterId: character.id, participantId: characterParticipant.id,
      partialLength: streamingState.fullResponse.length,
      toolCount: toolMessages.length,
    })
  }
  ```
  (`partialLength` is a JS `.length` — UTF-16 units.)
- `:1778-1830` yield detection, ONLY `if (offerYield && !turnStopped)`:
  `detectYieldSentinel(streamingState.fullResponse, character.name, character.aliases ?? undefined)`;
  ALWAYS `safeEnqueue(encodeHandAnsweredEvent({ participantId, characterName: character.name, yielded }))`
  (before any save); on yield: `yieldedFloor = true`,
  `streamingState.fullResponse = yieldDetection.cleaned`, INFO
  `[TurnYield] Character yielded the floor to the raised hand`
  `{ chatId, characterId, participantId, withReply: cleaned.length > 0, toolCount }`;
  if `cleaned.trim().length === 0 && toolMessages.length === 0` →
  `return handleTurnYield({...})` (the BARE yield); else fall through. On a
  decline: INFO `[TurnYield] Character carried on past the raised hand`
  `{ chatId, characterId, participantId }`.
- `:1833-1844` `finishYieldedTurn(result)`: if `yieldedFloor`, `await
  announceTurnYield({ chatId, character, characterParticipant, operatorName:
  resolvedIdentity.name, controller, encoder })` then `{ ...result, yielded: true }`.
  `operatorName` here is `resolvedIdentity.name` (the resolved speaking
  identity), whereas the NOTE used `userCharacter?.name || 'The User'` — two
  different sources (§6 T4).
- `:1851` the skip-sentinel block is gated `if (turnSkip && !turnStopped && !yieldedFloor)`.
- `:1894-1928` the prose branch: `const finalized = await finalizeMessageResponse({...}); return finishYieldedTurn(finalized)`.
- `:1962-1968` the tool-only branch returns `finishYieldedTurn({ isMultiCharacter, hasContent: true, messageId: firstToolMessageId || null, userParticipantId, isPaused: chat.isPaused })`.
- `:1969-1990` NEW branch `else if (turnStopped)` (before the empty-response
  branch): `encodeDoneEvent({ messageId: null, participantId, usage, cacheUsage,
  attachmentResults, toolsExecuted: false, stopped: true, provider, modelName })`
  and return `{ isMultiCharacter, hasContent: false, messageId: null,
  userParticipantId, isPaused: chat.isPaused }` — NO `emptyResponse`, no
  recovery, no warn line.
- `:2354-2370` `announceTurnYield(params)`: `postHostTurnYieldAnnouncement({
  chatId, characterName: character.name, participantId:
  characterParticipant.id, operatorName })`; if posted →
  `safeEnqueue(encodeHostAnnouncementEvent(hostMessage))`.
- `:2383-2432` `handleTurnYield(params)` (the bare yield): `await
  markMailAlertsDelivered(mailAlertsOwed, { chatId, via: 'turn-yield' })`
  (NEW `via` value); `await announceTurnYield(params)`; `encodeDoneEvent({
  messageId: null, participantId, usage, cacheUsage, attachmentResults,
  toolsExecuted: false, skipped: true, skippedParticipantId: participantId,
  yielded: true, provider, modelName })`; DEBUG `[TurnYield] Bare yield closed
  without a reply` `{ chatId, characterId, participantId }`; return `{
  isMultiCharacter, hasContent: false, messageId: null, userParticipantId,
  isPaused: params.isPaused, yielded: true }` — `isPaused` is the chat row's
  flag as loaded (no re-read), and **NO rotation advance** (contrast
  `handleTurnSkip`, which re-reads the chat and writes
  `spokenThisCycleParticipantIds` / `cycleOrderParticipantIds`).

### 1.11 Observable event ORDER (what the tier-3 frame diff pins)

| Path | Frames, in order |
|---|---|
| decline | … stream frames …, `{handAnswered:true, participantId, characterName, yielded:false}`, then the ordinary finalizer `done` (or tool-only `done`) |
| yield WITH words/tools | `{handAnswered…, yielded:true}`, the finalizer's frames + `done` (content = `cleaned`), THEN `{hostAnnouncement: <turn-yield message>}` |
| BARE yield | `{handAnswered…, yielded:true}`, `{hostAnnouncement: …}`, `{done:true, messageId:null, participantId, usage, cacheUsage, attachmentResults, toolsExecuted:false, skipped:true, skippedParticipantId, yielded:true, provider, modelName}` |
| stop, prose streamed | `[LLMStream]` INFO, finalizer `done` as usual (no confirmationResult pass, no dice TOOL rows, no pascalResult, no carinaAnswer) |
| stop, nothing streamed, no tools | `{done:true, messageId:null, participantId, usage, cacheUsage, attachmentResults, toolsExecuted:false, stopped:true, provider, modelName}` |
| chain close on yield | `{chainComplete:true, reason:'yielded', nextSpeakerId:null, chainDepth, paused:false}` |
| chain close on stop | `{chainComplete:true, reason:'stopped', nextSpeakerId:null, chainDepth, paused:true}` |

On a chained turn the driver's `turnComplete` frame precedes the chain close
(`turn-orchestrator.service.ts:434-450`).

### 1.12 `lib/services/chat-message/turn-orchestrator.service.ts`

- `:12` import `isCurrentTurnStopped`.
- `:342-345` FIRST guard in `executeTurnChain`, BEFORE the multi/empty guard:
  `if (initialResult.isMultiCharacter && isCurrentTurnStopped()) { await
  endChainOnStop(…, 0, …); return }` — a stopped SINGLE-character initial turn
  returns silently (no `chainComplete`), as any single-character turn does.
- `:347` `if (!isMultiCharacter || (!hasContent && !skipped && !yielded)) return`.
- `:353-356` `if (initialResult.yielded) { await endChainOnYield(…, 0, …); return }`
  — BEFORE the `singleTurn` guard (`:363`) and the paused guard (`:378`).
- `:442-450` in the loop, right after the `turnComplete` emit: `if
  (isCurrentTurnStopped()) { await endChainOnStop(…, chainDepth, …); break }`
  then `if (chainResult.yielded) { await endChainOnYield(…); break }` — both
  BEFORE the empty-response check.
- `:497-512` `endChainOnYield`: INFO `[TurnOrchestrator] Chain stopped: the
  floor was yielded to the operator` `{ chatId, chainDepth }`; `await
  persistTurnParticipant(repos, chatId, null)`; `chainComplete { reason:
  'yielded', nextSpeakerId: null, chainDepth, paused: false }`.
- `:519-534` `endChainOnStop`: INFO `[TurnOrchestrator] Chain stopped: the
  operator stopped the turn` `{ chatId, chainDepth }`; persist `null`;
  `chainComplete { reason: 'stopped', nextSpeakerId: null, chainDepth, paused:
  true }` — `paused: true` is ASSERTED, not read (the stop action paused the
  room first; a multi-character AUTONOMOUS room would be announced paused
  without being paused — unreachable in v4 because autonomous turns are never
  registered, §6 T6).

### 1.13 The guards (`isCurrentTurnStopped()` sites) — what each prevents

| # | v4 site | What it prevents |
|---|---|---|
| G1 | `message-finalizer.service.ts:96` `const turnStopped = isCurrentTurnStopped()` (ONE read) | — |
| G1a | `:222` `globalEnabled: …enabled === true && !turnStopped` | answer confirmation (the rewrite pass) on a cut-off reply |
| G1b | `:388` `if (autoDetectRngInResponse && cleanedResponse && !turnStopped)` | dice/coin TOOL rows from half-written notation |
| G1c | `:458` `if (!turnStopped) await runReplyTriggers({…})` | Pascal trigger phrases firing on a cut-off reply |
| G1d | `:478` `if (cleanedResponse && !turnStopped) await runCarinaMarkupQuery` | the `@Name:` Carina pass |
| G2 | `native-tool-loop.service.ts:144-147` loop head: INFO `Native tool loop halted: the operator stopped the turn` `{ chatId, toolIterations }`, `break` | no further tool call — "not even one the model finished asking for" (a detected batch on the latest raw response is left unexecuted) |
| G3 | `text-tool-loop.service.ts:177-180` loop head: INFO `Text tool loop halted: the operator stopped the turn` `{ chatId, iterations }`, `break` | a tool block that streamed before the stop is left unexecuted; no continuation |
| G4 | `provider-failover.service.ts:185-188` in `attemptEmptyResponseRecovery`, AFTER `flags()` is built, BEFORE the non-empty early return: INFO `Skipping empty-response recovery: the operator stopped the turn` `{ chatId }`, `return flags()` | no same-provider retry, no uncensored understudy, no fallback chain — and the route-trail `recordAttempt` that follows the non-empty check never runs |
| G5 | `orchestrator.service.ts:1766` (`turnStopped`) | reasoning dropped; `[TurnStop]` line |
| G6 | `:1779` `offerYield && !turnStopped` | no yield read from a cut-off line |
| G7 | `:1851` `turnSkip && !turnStopped && !yieldedFloor` | no `[NOTHING TO` sentinel read from a cut-off line |
| G8 | `:1969` `else if (turnStopped)` | the stopped-empty `done`, no recovery |
| G9 | `turn-orchestrator.service.ts:342` | no chain after a stopped initial turn |
| G10 | `:442` | no chain after a stopped chained turn |
| G11 | `stream-watchdog.ts:124` (the race) | the provider stream itself |

Plus the watchdog is reached ONCE: `streaming.service.ts:452-459` passes
`abortSignal: currentTurnStopSignal()` into `withStallWatchdog` — "every
chat-message call site (primary, retries, failover, tool-loop continuations,
Carina inside the turn) inherits it from here." Sites OUTSIDE the registered
stream (`currentTurnStopSignal()` → `undefined`): the Console, help chat, the
greeting, swipe regeneration, Carina's own surfaces, autonomous rooms.

### 1.14 `lib/services/chat-message/streaming.service.ts`

- `:452-459` the `abortSignal` option (above).
- `:707-724` `encodeDoneEvent` data gains `yielded?: boolean` ("Rides with
  `skipped` on a bare yield") and `stopped?: boolean` ("Like `skipped`, the
  client resets its buffer without a bubble or a toast"). Serialized position:
  wherever the caller's object literal puts them — `handleTurnYield` puts
  `yielded` AFTER `skippedParticipantId` and BEFORE `provider`; the stopped
  branch puts `stopped` AFTER `toolsExecuted` and BEFORE `provider`.
- `:817-820` `encodeChainCompleteEvent` reason union gains `'yielded' | 'stopped'`.
- `:844-854` `encodeHandAnsweredEvent(encoder, { participantId, characterName,
  yielded })` → `data: ${JSON.stringify({ handAnswered: true, ...data })}\n\n`
  — key order `handAnswered, participantId, characterName, yielded`.
- `lib/services/chat-message/types.ts:308-314` `ProcessMessageResult.yielded?: boolean`.

### 1.15 `lib/services/host-notifications/writer.ts:366-408` (the Host `turn-yield` message)

- `:377` `HOST_KIND_TURN_YIELD = 'turn-yield'` (a module-local const; the
  exported twin is `TURN_YIELD_SYSTEM_KIND` in `yield-signal.ts`).
- `:379-381` `buildTurnYieldContent(characterName, operatorName)` VERBATIM:
  `` `The Host, spying a hand raised at the back of the room, catches ${characterName}'s eye — and ${characterName} yields the floor to ${operatorName} with a gracious little bow. Pray, go on.` ``
- `:383-385` `buildTurnYieldOpaqueContent(characterName, operatorName)`:
  `` `${operatorName} raised a hand to interject, and ${characterName} yielded the floor to them. ${operatorName} speaks next.` ``
- `:387-394` `HostTurnYieldAnnouncement { chatId, characterName, participantId, operatorName }`.
- `:401-408` `postHostTurnYieldAnnouncement(params)` → `postHostMessage(chatId,
  content, opaqueContent, 'turn-yield', { participantId })` — the existing
  writer (`:211-262`): `repos.chats.findById` (null → `null`), a
  `randomUUID()` id, `role: 'ASSISTANT'`, `participantId: null`,
  `systemSender: 'host'`, `systemKind: 'turn-yield'`, `hostEvent: {
  participantId }`, `attachments: []`, `repos.chats.addMessage` (PERSISTED in
  the transcript — not ephemeral), INFO `[HostNotification] Announcement
  posted` `{ context: 'host-notifications', chatId, messageId, kindLabel,
  hostEvent }`; errors swallowed → `null`. The persona `content` never
  contains `[YIELD]`.

### 1.16 The SPA (v4)

**`app/salon/[id]/SalonView.tsx`** — `:1827-1831` the composer props:
`onStopStreaming={sseStreaming.stopStreaming}`,
`onToggleRaisedHand={sseStreaming.toggleRaisedHand}`,
`handRaised={sseStreaming.handRaised}`,
`canRaiseHand={!participantsWithImpersonation.isSingleCharacterChat && chat?.chatType !== 'autonomous'}`,
`stopping={sseStreaming.stopping}`; the `hideStopButton` prop is GONE; the
sidebar's `onStopStreaming` prop is REMOVED (`:2055` old).
`isSingleCharacterChat` (`hooks/useParticipants.ts:74-76`) =
`participantsAsBase.filter(p => p.type === 'CHARACTER' && p.isActive).length === 1`
(ACTIVE character seats, user-controlled included).

**`app/salon/[id]/components/ChatComposer.tsx`** — props `:103-115`:
`onStopStreaming: () => void` ("Hard stop: cut the reply off, keep what
streamed, cede the floor."), `onToggleRaisedHand: () => void`, `handRaised?`,
`canRaiseHand?` ("a room with a rotation (more than one character). A lone
character's reply hands the floor back anyway."), `stopping?` ("A hard stop
is being put away on the server."); defaults `:199-201` all `false`;
`:219` `const isGenerating = streaming || waitingForResponse`.
The render `:606-660`, inside `<div className="qt-chat-composer-actions">`:

1. Send `<button type="submit" className="qt-chat-composer-send"
   aria-label="Send message" disabled={composerLocked || isGenerating ||
   (!hasContent && attachedFiles.length === 0 && pendingToolResults.length
   === 0) || !hasActiveCharacters} title={…}>` — titles unchanged:
   `"Add a character to start chatting"` / `"Generating..."` / the voice-
   rehearsal title / `"Send message"`; icon `send`.
2. Raise hand `<button type="button" onClick={stopPropagation;
   onToggleRaisedHand()} disabled={!isGenerating || !canRaiseHand || stopping}
   className={`qt-chat-composer-send qt-chat-interrupt-button${handRaised ?
   ' qt-chat-interrupt-button-raised' : ''}`} aria-pressed={handRaised}
   aria-label={handRaised ? 'Lower your hand' : 'Raise your hand to
   interrupt'} title={…}>` with `<Icon name="hand-raised" className="w-5 h-5" />`.
   Title ladder: `!isGenerating` → `'Raise your hand (while someone is
   speaking)'`; `!canRaiseHand` → `'Only one character is present — the floor
   returns to you after this reply'`; `handRaised` → `'Hand raised — the next
   speaker will be asked to yield. Click to lower it.'`; else `'Raise your
   hand — the next speaker may yield the floor to you'`.
3. Stop `<button type="button" onClick={stopPropagation; onStopStreaming()}
   disabled={!isGenerating || stopping} className="qt-chat-composer-send
   qt-chat-stop-button" aria-label="Stop" title={…}>` with `<Icon
   name="stop-sign" className="w-5 h-5" />`. Title ladder: `stopping` →
   `'Stopping…'` (U+2026); `isGenerating` → `'Stop — cut the reply off here
   and take the floor'`; else `'Stop (while someone is speaking)'`.

All three are ALWAYS rendered (greyed when disabled) — no more swap of Send
for Stop. Keyboard: no new key bindings; the buttons are plain buttons
(`type="button"`, so Enter in the editor still submits only Send). There is
NO raised-hand indicator on the participant card or sidebar — the ONLY
indicator is the composer button's `-raised` class (ring + pulse) and
`aria-pressed`.

**`app/salon/[id]/hooks/useSSEStreaming.ts`** (+124):
- `:134-142` `SSEEvent` gains `stopped?`, `yielded?`, `handAnswered?`.
- `:338-343` state `handRaised`, `stopping`, `stoppingRef`.
- `:734-742` in the frame reader: `if (data.handAnswered) { setHandRaised(false);
  if (!data.yielded) showInfoToast(`${data.characterName || 'The speaker'}
  carried on regardless. Your hand is lowered.`) }`.
- `:1084`, `:1148`, `:1344`, `:1371` — the four `onDone` / `onIntermediateDone`
  guards become `if (data.skipped || data.stopped) { finishSkippedTurn();
  return }` (`finishSkippedTurn` `:452-455` = `resetStreamingContent();
  setStreaming(false)` — no bubble, no toast).
- `:1255-1260` and `:1433-1435` (both `finally` blocks): `setHandRaised(false);
  stoppingRef.current = false; setStopping(false)` — "any hand still up has
  nobody left to ask, and a stop has finished putting the reply away."
- `:1449-1463` `abandonLocalStream` (the OLD stop, now the fallback): abort the
  fetch's `AbortController`, `setStreaming(false)`, `setWaitingForResponse(false)`,
  `setRespondingParticipantId(null)`, `setStreamingToolBatches([])`,
  `dismissToolExecutionStatus()`, `resetStreamingContent()`, clear `stopping`.
- `:1471-1507` `stopStreaming` (async): re-entrancy latch on `stoppingRef`;
  `setStopping(true)`; `setHandRaised(false)`; `if (isMultiChar) void
  setPauseState(true)` (the CLIENT's pause flips FIRST — so the later
  `chainComplete {paused:true}` is NOT news and `announceChainPause` stays
  quiet, §6 T7); `fetch(`/api/v1/chats/${chatId}?action=stop-turn`, { method:
  'POST' })` (NO body, NO content-type); `serverStopped = (data.stopped ?? 0)
  > 0`; on a thrown fetch `console.error('[Salon] Stop request failed;
  abandoning the stream locally', msg)`; `if (!serverStopped) {
  abandonLocalStream(); await fetchChat() } else { setTimeout(3000: if still
  stopping && a controller exists → abandonLocalStream(); void fetchChat()) }`;
  then ALWAYS `showInfoToast(isMultiChar ? 'Stopped. The floor is yours, and
  the room is paused.' : 'Stopped. The floor is yours.')`. The OLD toast
  `'Response stopped - chat paused'` is GONE.
- `:1514-1532` `toggleRaisedHand` (async): `raise = !handRaised`; optimistic
  `setHandRaised(raise)`; `fetch(…?action=raise-hand, { method: 'POST',
  headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({
  raised: raise }) })`; `!res.ok` → `throw new Error(res.statusText ||
  'Request failed')`; `setHandRaised(data.raised === true)`; `if (raise &&
  data.active === false) showInfoToast('Nobody is speaking just now. The
  floor is already yours.')`; catch → `setHandRaised(false);
  showErrorToast(`Could not raise your hand: ${getErrorMessage(err)}`)`.
- `:1546-1550` returns `handRaised, toggleRaisedHand, stopping`.
- `announceChainPause` (`:487`) is UNCHANGED: a `chainComplete` with
  `reason: 'stopped', paused: true` passes through its gates (`!paused` no;
  `isPausedRef.current` — ALREADY true from `setPauseState(true)` above — →
  return). `reason: 'yielded', paused: false` → returns at the first gate. No
  reason-specific wording exists for either.

**`components/chat/ChatSidebar.tsx:162, :850`** — the `onStopStreaming` prop
and its pass-through to `ParticipantCard` are DELETED.
**`components/chat/ParticipantCard.tsx:92, :139, :555-570`** — the
`onStopStreaming` prop and the `turnStatus === 'generating' && onStopStreaming`
Stop button branch are DELETED; the actions block starts at `isUserParticipant
&& onSkip`. The "Speaking..." disabled button for the generating participant
is unchanged (test `:948-961`).

**`app/styles/qt-components/_chat.css`** (post-`b8362beb5` shape):
- `:938-950` (old) the `.qt-chat-stop-button` pair is REMOVED from BEFORE
  `.qt-chat-composer-send` …
- `:1189-1204` (pin):
  ```css
  .qt-chat-composer-actions {
    @apply flex flex-col items-center;
    gap: 0.375rem;
    flex-shrink: 0;
  }
  /* Three round buttons (Send / raise hand / Stop) side by side would steal
     a lot of the editor's width, so they stack in a column beside it, a size
     smaller than a lone Send button. */
  .qt-chat-composer-actions .qt-chat-composer-send {
    height: 2.5rem;
    width: 2.5rem;
  }
  ```
  (`2b5e8ee24` had the column only under `@media (max-width: 639px)`; the
  rider made it unconditional — port the rider's shape.)
- `:1226-1270` (pin) AFTER `.qt-chat-composer-send:hover`:
  `.qt-chat-stop-button` (danger bg/fg/border) + `:hover:not(:disabled)`
  `color-mix(in srgb, var(--qt-status-danger-bg) 70%, black)` — "These follow
  `.qt-chat-composer-send` on purpose: the buttons carry both classes, and at
  equal specificity the later rule's colours must win." Then
  `.qt-chat-interrupt-button` (warning bg/fg/border) + hover (warning 70%
  black); `.qt-chat-interrupt-button-raised { box-shadow: 0 0 0 3px
  color-mix(in srgb, var(--qt-status-warning-border) 45%, transparent);
  animation: qt-hand-raised-pulse 1.6s ease-in-out infinite; }`;
  `@keyframes qt-hand-raised-pulse { 0%, 100% { 3px / 45% } 50% { 6px / 15% } }`;
  `@media (prefers-reduced-motion: reduce) { .qt-chat-interrupt-button-raised { animation: none; } }`.
- `:3015-3028` (old) `.qt-participant-stop-button` + hover DELETED.
- `app/styles/qt-components/_icons.css:98-99` two mask lines:
  `[data-icon="stop-sign"] { --_qt-icon-mask: url("/images/icons/stop-sign.svg"); --_qt-icon-bg: none; }`
  `[data-icon="hand-raised"] { --_qt-icon-mask: url("/images/icons/hand-raised.svg"); --_qt-icon-bg: none; }`
  (after `stop`, before `zoom-in`).

**Icons** — `components/ui/icons/icon-registry.ts:110-113` two entries after
`'stop'`: `'stop-sign': { defaultFile: '/images/icons/stop-sign.svg',
defaultMode: 'mask' }`, `'hand-raised': { … '/images/icons/hand-raised.svg' …
}`. `public/images/icons/stop-sign.svg` (3 lines: a filled octagon, `fill=
"currentColor" stroke="currentColor" stroke-width="1.5" stroke-linejoin=
"round"`, points `8.36 3.22 15.64 3.22 20.78 8.36 20.78 15.64 15.64 20.78
8.36 20.78 3.22 15.64 3.22 8.36`); `public/images/icons/hand-raised.svg` (6
lines, four stroked paths — a Lucide-style open hand). Madman's Box:
`themes/bundled/madmans-box/icons/stop-sign.svg` (octagon outline + inner
filled octagon), `hand-raised.svg` (a blocky polygon hand + three finger
lines); `themes/bundled/madmans-box/theme.json:234-235` `"stop-sign":
"icons/stop-sign.svg", "hand-raised": "icons/hand-raised.svg"`;
`themes/tools/madmans-box-icon-preview.html:124` adds both to the preview
list; `docs/developer/ICON_INVENTORY.md` +2.

### 1.17 NO-PORT / note-only

- `packages/theme-storybook/**` (CHANGELOG, package.json ×2, package-lock,
  `src/css/qt-components.css` +89 / −11, `src/stories/components/Chat.tsx`
  +34, `Icons.tsx`): v5 has no storybook package — `apps/web/` has no
  storybook config and `rg storybook apps/web/src` hits only a comment in
  `announcement-group.ts`. NO-PORT (the CSS it mirrors is ported from
  `_chat.css`).
- `help/interrupting.md` (NEW, 55 lines; the rider edits its first
  paragraph "seated to the right … beside" → "stacked to the right … beneath"),
  `help/chat-turn-manager.md` (the section renamed "Raising a Hand and
  Stopping", a Related-Pages link), `help/chats.md:216-217`,
  `help/chat-participants.md:191` (the sidebar Stop bullet removed): the
  VENDOR survey's (`help/` is embedded whole — v5's `help/` has 131 files and
  NO `interrupting.md` today; `help/` count 131 → 132).
- `README.md`, `package.json`, `packages/quilltap/package.json`,
  `package-lock.json`, `docs/CHANGELOG.md`, `docs/developer/API.md` (+36 —
  the action set list gains `stop-turn`, `raise-hand`; two new sections
  documenting the bodies / responses, consistent with §1.1 — it also SAYS the
  stop "closes with `chainComplete` reason `stopped`" and the response "comes
  back once the stream has settled, or after 15 s").

### 1.18 Tests v4 shipped (the seed corpus) — `it` names verbatim

- `__tests__/unit/lib/chat/turn-control.test.ts` (NEW, 125 lines, **10 cases**):
  `has no handle outside a registered stream`; `registers a stream for its
  lifetime only`; `unregisters a stream that throws`; `hands a raised hand to
  the first turn that asks, and only once`; `lowers a hand before anyone is
  asked`; `reports no stream to raise a hand on when nothing is running`;
  `stops the running stream, waits for it to settle, and clears its hand`
  (expects `{ stopped: 1, settled: true }` and `{ aborted: true, stopped:
  true, hand: false }`); `never touches a fresh stream opened after the stop`;
  `gives up waiting on a stream that will not settle` (`settleTimeoutMs: 20`
  → `settled: false`); `is a no-op stop when nothing is running`.
- `__tests__/unit/lib/chat/turn-manager/yield-signal.test.ts` (NEW, 68 lines,
  **10 cases**): 8 under `detectYieldSentinel` (`reads a bare sentinel as a
  yield with nothing else said`; `forgives case, brackets, wrapping markdown
  and trailing punctuation` — forms `['[yield]', 'YIELD', '**[YIELD]**',
  '`[Yield]`', '_yield._', '[YIELD]!']`; `keeps the prose that follows a
  sentinel line as the reply`; `honours a sentinel tacked on as the last
  line`; `honours a bracketed sentinel opening a line of prose`; `strips a
  leading own-name prefix before looking` (`'Alice: [YIELD]'`, `'Alice'`);
  `does not mistake the word "yield" in prose for the sentinel`; `reads an
  ordinary reply, or nothing, as no yield`), 1 under
  `buildTurnYieldInstruction` (`names the operator and offers the exact
  sentinel`), 1 under `TURN_YIELD_SYSTEM_KIND` (`is the Host message kind the
  writer stamps on a yield`).
- `__tests__/unit/lib/llm/stream-watchdog.test.ts` (+49; the file has **12
  cases**, 4 NEW under `the operator's stop (abortSignal)`): `ends quietly,
  keeping what arrived, when the stop lands mid-stream`; `abandons a provider
  still waiting on its first chunk`; `yields nothing when already stopped
  before it starts`; `leaves a healthy stream alone when the stop never fires`.
- `__tests__/unit/lib/services/chat-message/turn-orchestrator.test.ts` (+148;
  **38 cases**, 4 NEW under `executeTurnChain`): `ends the chain, unpaused,
  when a chained turn yields to the raised hand` (asserts
  `persistTurnParticipant(repos, 'chat-1', null)`, NO `isPaused: true` update,
  `{reason:'yielded', nextSpeakerId:null, chainDepth:1, paused:false}`);
  `ends the chain as a pause when the operator stops a chained turn` (the
  stopped turn HAS content — only the flag stops it; `{reason:'stopped',
  chainDepth:1, paused:true}`); `does not chain at all when the INITIAL turn
  was stopped` (`decideNextTurn` never called; `chainDepth:0`); `ends a
  yielded INITIAL turn without chaining` (`initialContinueMode: true`;
  `chainDepth:0, paused:false`). The file now `jest.mock`s
  `@/lib/chat/turn-control` to `{ isCurrentTurnStopped: jest.fn() }`.
- `__tests__/unit/components/chat/ParticipantCard.test.tsx` (47 lines changed):
  the old Stop-button describe is REPLACED by `Stop button (moved to the
  composer)` with ONE case `shows the Speaking button, not a Stop button,
  while this participant is generating`.
- No v4 test for `interrupt.ts`, `useSSEStreaming`'s new callbacks,
  `ChatComposer`'s buttons, the finalizer guards, the tool-loop guards, the
  failover guard, `context-manager`'s note placement, or the writer's texts —
  those are v5-only corpora (§4).

---

## 2. The v5 intersection map (file:line at `5a94a9d1e`)

### 2.1 The chat spine — where v4's registered stream lives in v5

- **The send is a dispatch, not an HTTP stream.** `Request::ChatSend`
  (`api/types.rs:391-416`) → `engine.rs:2014-2043` → `CoreEngine::chat_send`
  (`:6962-6980`) → the host's `ChatSendDriver` (`api/chat_send.rs:46-48`;
  `spine.rs:2526-2551`): the turn runs on **its own OS thread with a
  `new_current_thread` tokio runtime** (`std::thread::spawn` +
  `rt.block_on(state.run_send(req))`), the driver future awaits a `oneshot`,
  and the `Response::ChatSend(dto)` returns ONLY after `run_send` has run
  `process_message_with_ledger` (`:1631`), `execute_turn_chain` (`:1714`) and
  the post-chain `trigger_conversation_render` (`:1771-1777`) — the exact
  span v4 wraps in `runWithTurnControl` (`orchestrator.service.ts:222-287`).
  Frames ride the `Event` broadcast (`BroadcastSink`, `:1462-1465`;
  `Event::chat`, `types.rs:5819`) and the web re-frames them on
  `GET /api/events` (`quilltap-web/src/events.rs:63-110`) — one global SSE,
  not per-send.
- **Who else enters the spine:** the autonomous-room job handler goes through
  `enclave_step` → `process_message` (`spine.rs:2020-2050`, `enclave/step.rs:9`),
  NOT `run_send` — so, as in v4 (the job child), an autonomous turn is never
  registered. v4 `autonomous-room-turn.ts:624` calls `handleSendMessage` (and
  so `runWithTurnControl`) in the CHILD, where the registry is a different
  process; v5 must NOT register the enclave's turn (§6 T6). Swipe regeneration
  (`regenerate_swipe.rs`), help chat, Brahma, the greeting: separate
  services, never registered.
- **`OrchestratorDeps`** (`orchestrator.rs:529-636`): the explicit bundle
  v4's ambient store stands in for — every seam the turn needs is a field
  (`sink`, `confirmation`, `carina_query`, `consult`, …). It is cloned by
  reference into `execute_turn_chain` (`:4432`) and re-entered per chained
  turn (`:4569`). v5 has NO `AsyncLocalStorage` twin: `activity_registry.rs:
  44-57` records that `tokio::task_local!` is unavailable in the default core
  build and hand-rolls a poll-time thread-local (`Attributed`).

### 2.2 The watchdog — v4's ONE funnel is SEVEN v5 call sites (+5 that must stay signal-less)

`model/stream_watchdog.rs:150-266`: `WatchedStream { rx: Option<Receiver<
StreamChunkResult>>, budgets, ctx, chunks_received, started_at, stalled }`;
`watch_stream(rx, budgets, ctx)` (`:182-194`); `recv()` (`:209-266`) =
`tokio::time::timeout(budget, rx.recv())` — the stall path WARNs
`[LLMStream] Abandoned a stalled provider stream` with `context, user_id,
chat_id, character_id, message_id, provider, model_name, budget_ms,
chunks_received, elapsed_ms` (`:235-249`), drops the receiver ("abandoned, not
cancelled" — the producer's next `blocking_send` fails, `streaming_provider.rs:
592-620`, a plain OS thread pump) and answers ONE `Err(StreamError::stalled)`.
The stop race belongs HERE (v4 `:124`): a second awaited future beside
`rx.recv()` inside the timeout, answering `None` (end-of-stream) with INFO
`[LLMStream] Abandoned a provider stream at the operator's stop` and the
SAME receiver drop. `StallWatchdogContext` (`:100-147`) is where the handle
can ride (or a new `watch_stream_with_stop` sibling — the `_with_codec`
sibling pattern from the memory notes).

Salon-side `watch_stream(` sites that v4's funnel covers (ALL must take the
signal):

| v5 site | v4 call site it mirrors |
|---|---|
| `services/primary_stream.rs:1299-1310` | the primary stream |
| `services/native_tool_loop.rs:496` and `:638` | the native loop's continuation + its force-final re-stream |
| `services/text_tool_loop.rs:721` | the text loop's continuation |
| `services/provider_failover.rs:1158` | the failover legs |
| `services/recovery.rs:397` | the same-provider / uncensored retries |
| `services/carina_query.rs:1273` | Carina INSIDE the turn (`ask_carina` tool + the finalizer's `@Name:` pass — v4: "Carina inside the turn") |

Sites that must NOT get it (v4 `currentTurnStopSignal()` is `undefined`
there): `services/initial_greeting.rs:233`, `services/help_chat/
orchestrator.rs:1420`, `services/agent_loop/one_shot_loop.rs:351`,
`services/brahma_console/orchestrator.rs:978`, `services/regenerate_swipe.rs:
876`. `crates/quilltap-harness/tests/stream_watchdog_wrap_census.rs` is the
census that pins which sites wrap — it must grow a "which sites carry the
stop" column.

⚠ `carina_query.rs:1273` is reached from the `ask_carina` TOOL (`ErasedAskCarina`,
`OrchestratorDeps::ask_carina`, an OWNED `'static` seam built per send at
`spine.rs:1528-1545`) AND from the finalizer's markup pass (`carina_query`,
`:1547-1558`) — both are constructed in `run_send`, so a per-send handle can be
threaded into their constructors; the finalizer's pass is ALSO gated off
entirely by G1d on a stop, so only the tool path streams under the signal.

### 2.3 The chain + the per-turn result

- `orchestrator.rs:4416-4660` `execute_turn_chain` — v4 `:326-534` minus the
  two new guards. The initial guard is `:4466-4468` (`!is_multi_character ||
  (!has_content && !skipped)`), `single_turn` `:4470-4477`, paused
  `:4493-4510`, the loop `:4525`, the `turn_complete` emit `:4585-4594`, the
  empty-stop `:4598-4620` (`reason: "error"`, `paused: Some(false)`), the
  chain-error pause `:4622-4656`. The two new guards go at `:4466` (stop,
  before the multi/empty guard; depth 0) / after `:4468` (yielded; depth 0)
  and at `:4595` (stop, then yielded; `chain_depth`), each a
  `persist_turn_participant_id(db, chat_id, None)` (`turn_orchestrator.rs:635`)
  + `ChatEvent::chain_complete(ChainCompletePayload { reason, next_speaker_id:
  None, chain_depth, paused: Some(false|true), held_user_turn: None })`
  (`chat_events.rs:477-514`; `reason` is a `String`, so no enum widening).
- `message_finalizer.rs:572-589` `ProcessMessageResult { is_multi_character,
  has_content, message_id: String, user_participant_id, is_paused,
  scene_tracking_character_ids, skipped: bool, skipped_participant_id }` —
  gains `yielded: bool` (v4 `yielded?: boolean`). Constructed at
  `orchestrator.rs:837`, `:1025`, `:3328`, `:3926-3932`, `:4007`, `:4176-4184`
  and in the finalizer; `ChatSendResultDto` (`types.rs:5168-5179`) projects
  it (v4's `ProcessMessageResult.yielded` is a stream-internal field; the DTO
  may stay as is — §8 Q5).
- `orchestrator.rs:1707-1735` the `turn_skip` triple (v4 `:~1240`); `:1176-
  1190` `hold_for_paused_chat`; `:2062-2072` the held return — the
  `offer_yield` computation sits between the hold decision and the Prospero
  cadence gate (`:1739`), exactly v4 `:659-674`: `controlled_by != "user" &&
  !hold_for_paused_chat && take_raised_hand()`.
- `:3640-3700` the sentinel block (`resolve_sentinel_action` →
  `ClearResponse` / `HandleSkip` / `ReplaceWithCleaned`) — gets the
  `!turn_stopped && !yielded_floor` gate; the `[TurnStop]` block + the yield
  detection + the `handAnswered` emit go immediately BEFORE it (v4 `:1759-1830`
  precede `:1851`); `:3776` the finalizer call (wrap in `finish_yielded_turn`);
  `:3876-3932` the tool-only branch (same); the stopped-empty branch goes
  between `:3932` and the empty-response `else` at `:3933`; `:4078-4188`
  `handle_turn_skip` is the template for `handle_turn_yield` (minus the
  rotation write, plus `yielded: true`, `MailAlertsVia::TurnYield` — a NEW
  enum arm in `post_office::mailbox`, v4 `via: 'turn-yield'`).
- `chat_events.rs:162-215` `DonePayload` — gains `yielded: Option<bool>`
  (declared right after `skipped_participant_id`, `skip_serializing_if`) and
  `stopped: Option<bool>` (right after `tools_executed`, before `skipped`);
  `ChatEvent` (`:264+`) gains a `HandAnswered { hand_answered: TrueBool,
  participant_id, character_name, yielded }` variant (the `TurnStart` shape,
  `:290-299`); `ChatEvent::done` (`:552`), `host_announcement` (`:583`) are
  the constructors the new paths use.
- `host_notifications.rs:549-589` `post_host_message(db, chat_id, content,
  opaque_content, kind_label, host_event)` — the writer; `:813-856` the
  turn-pass twin to copy (`HostTurnPassAnnouncement`,
  `post_host_turn_pass_announcement` → `json!({ "participantId": … })`); the
  `TURN_PASS_SYSTEM_KIND` const lives in `skip_signal.rs` — the yield twin
  (`TURN_YIELD_SYSTEM_KIND = "turn-yield"`) belongs in a NEW
  `yield_signal.rs` beside it.
- `skip_signal.rs:236-271` `is_sentinel_line` — the body becomes
  `matches_sentinel_line(line, phrase)` with `phrase = "nothing to add"`;
  `:160-180` `detect_skip_sentinel` is the template for `detect_yield_sentinel`
  (same `normalize_content_block_format` + guarded
  `strip_character_name_prefix` from `message_formatter`, `:37`). The
  `LEADING_BRACKETED_YIELD` regex needs the `regex` twin with JS `\s` + `i`
  semantics (the `jsregex`/`jsstr` helpers v5 keeps — `crate::jsstr::js_trim`
  is already what the matcher uses).
- `build_context.rs:620` `BuildContextInput.turn_skip: Option<TurnSkip>`;
  `:640-650` `TurnSkip`; `:651` `build_turn_skip_instruction`; `:2956-2961`
  the skip note; **`:3062-3072` the ONE `trailing_context_sections` list**
  (`core_whisper, llm_recall_text, suparna_mail, progressions, inform_block,
  turn_skip_instruction`) — `turn_yield_instruction` goes LAST. The operator
  name: v5's `user_character` resolution inside `build_context` (find the
  field `build_context.rs` resolves for the persona name — the note uses
  `userCharacter?.name || 'The User'`). `services/message_context.rs:2492`
  plumbs `turn_skip: None` for the non-Salon builder — `offer_yield: false`
  there.
- `message_finalizer.rs`: G1a at `:838-855` (`global_enabled` →
  `&& !turn_stopped`); G1b at `:1242-1246` (`auto_detect_rng &&
  !cleaned_response.is_empty()` → `&& !turn_stopped`); G1c at `:1331-1345`
  (`run_reply_triggers`, wrap in `if !turn_stopped`); G1d at `:1360-1364`
  (`if !cleaned_response.is_empty()` → `&& !turn_stopped`). The finalizer
  takes `FinalizeOptions` (`:592+`) — a `turn_stopped: bool` field is the
  explicit twin of v4's one ambient read at `:96`.
- `native_tool_loop.rs:262` `while current_raw_response.is_some() &&
  tool_iterations < effective_max_turns {` — G2 as the loop's first statement
  (INFO `Native tool loop halted: the operator stopped the turn` with
  `chat_id, tool_iterations`); `text_tool_loop.rs:404` `while iterations <
  MAX_TEXT_TOOL_ITERATIONS {` — G3 (`chat_id, iterations`).
- `provider_failover.rs:272-335` `attempt_empty_response_recovery` — G4 goes
  after `let mut flags = …` (`:328`) and BEFORE the "Not empty" early return
  (`:331`), so the route-trail `record` that follows (`:336+`) never runs.
- `activity_registry.rs:1-70` — the process-wide `static` precedent for v4's
  `processSingleton` (and the recorded `tokio::task_local!` unavailability).

### 2.4 The two actions — the dispatch table and the web edge

- `api/types.rs:73` `Request` is `#[serde(tag = "type", rename_all =
  "camelCase")]`; `ChatTurnAction { chat_id, action, participant_id }`
  (`:253-262`) → `engine.rs:1081-1096` → `salon::turn_action(&db, …)`;
  `Response::TurnAction(Value)` (`:4496-4497`). The two new variants
  (`ChatStopTurn { chat_id }`, `ChatRaiseHand { chat_id, raised: Option<bool> }`
  — names in §5.3) follow this shape; `salon.rs:2275` `chat_set_active_speaker`
  is the smallest handler template (loads the chat, writes, answers).
- `quilltap-web/src/wardrobe_routes.rs:650-704` `CHAT_POST_ACTIONS` — v4's
  thunk-map ORDER, the `availableActions` of the `Unknown action` refusal
  (`query::dispatch_required_action`, `:717-726`); `stop-turn` and
  `raise-hand` go after `"turn"` (`:657`). The REST arm `chat_action_post`
  (`:709-830`) serves only `equip | wardrobe-rollback | regenerate-avatar |
  inform | cancel-inform`; everything else answers the loud pointer `:822-827`
  — the SPA dispatches by `Request` variant, so the two actions need NO REST
  arm unless the lane chooses one (v4's `useSSEStreaming` calls
  `?action=stop-turn` / `?action=raise-hand` over REST, but v5's SPA dispatches
  `type: 'chatSend'` via `core.dispatchExpect` — `salon-conversation.ts:3752-3780`).
  `wardrobe_routes.rs:942-949` has order tests on the constant (`at(…) ==
  at(…) + 1`) — the model for pinning the new keys' positions.
- The chat row the stop handler reads (`chat.chatType`, `participants`,
  `isPaused`) and the `isPaused: true` write: `db::chats::ChatUpdate {
  is_paused: Some(true), .. }` as `orchestrator.rs:4632-4641` writes it;
  `participant_filters::is_multi_character_chat` is the predicate twin.

### 2.5 The SPA

| v4 | v5 | verdict |
|---|---|---|
| `ChatComposer.tsx` three buttons | `apps/web/src/app/chat/chat-composer.ts:527-547` — `@if (busy())` swaps Send for ONE Stop (`qt-chat-composer-send qt-chat-stop-button`, title/aria `Stop generating`, icon `stop`, `(click)="stop.emit()"`); inputs `busy` (`:596`), `disabled`, `hasActiveCharacters`; outputs `send`, `stop`, `continue`… (`:681-690`); `composerLocked` `:851`, `canSend` `:853-855` | v5 HAS the PRE-drift shape (the old swap). The lane renders all three inside `.qt-chat-composer-actions`, adds inputs `handRaised`, `canRaiseHand`, `stopping` and output `toggleRaisedHand`, moves the Send `[disabled]` to v4's `isGenerating` form, and ports the two title ladders. |
| `useSSEStreaming.stopStreaming` | `screens/salon/salon-conversation.ts:4222-4232` `stop()` — local only: `dismissToolExecutionStatus()`, the OLD toast `'Response stopped - chat paused'` when `stream()?.content`, `stream.set(null)`, `clearProvisionalMessages()`. Comment: "The server turn rides the shared SSE and can't be aborted from here yet … (tracked deferral: a real stop dispatch)". It does NOT set the pause (v4's old one did for multi-char). | v5 LACKS the server stop. Becomes: latch → `stopping` → `handRaised=false` → `setPauseState(true)` when multi-char (`:3495`) → dispatch `chatStopTurn` → on `stopped > 0` arm the 3 s fallback, else clear locally + refetch → the toast. |
| `useSSEStreaming.toggleRaisedHand` / `handRaised` / `stopping` | nothing | v5 LACKS. New signals on the vertical; `handAnswered` handling in the frame fold. |
| `useSSEStreaming` frame reader (`handAnswered`, `data.stopped` on done) | `core/chat-stream.reducer.ts:286-288` (`hostAnnouncement` dedupe push), `:337-356` (`chainComplete` → `chainReason` / `chainPaused` / `chainHeldUserTurn`), `:368-395` `reduceDone` (`skipped` → reset without a bubble) | v5 HAS the pre-drift fold. Add: `frame.stopped` joins `frame.skipped` in `reduceDone` (`:389`), `finalDone` gains `stopped` / `yielded`, a `handAnswered` frame sets a `handAnswered: { participantId, characterName, yielded }` transition the vertical turns into the decline toast (`reportStreamTransitions`, `:3745`). |
| `announceChainPause` | `salon-conversation.ts:3838-3848` + the method body (`if (!paused) return; …`) | HAS — no change needed for `yielded` (paused false → silent) or `stopped` (paused true, but `pausedBefore` must already be true — the lane's `stop()` must flip the client's pause belief BEFORE the frame lands, §6 T7). |
| `SalonView` `canRaiseHand` | the vertical's chat signal `c.chatType === 'autonomous'` (`:403`), `isAllLLMRoom` (`:1697-1703`); NO `isSingleCharacterChat` twin exists (`rg` empty) — the count of ACTIVE `CHARACTER` seats must be computed (`p.type === 'CHARACTER' && p.isActive`, v4 `useParticipants.ts:74-76`) | LACKS (new computed). |
| `ParticipantCard` Stop removed | `chat/sidebar/participant-card.ts:288-298` the Stop button (`qt-participant-stop-button`, `Stop generating`, `stopStreaming.emit()`), `:452` output; `participants-section.ts:96, :148`; `chat-sidebar.ts:261, :444`; `salon-conversation.ts:409` `(stopStreaming)="stop()"` | v5 HAS what v4 DELETED — remove the branch, the four outputs and the binding; `participant-card.spec.ts` loses its Stop cases and gains v4's `shows the Speaking button, not a Stop button…`. |
| `icon-registry.ts` | `ui/icon.ts:17` the `IconName` union (`'play' | 'pause' | 'stop' | 'zoom-in'`), `styles/qt-components/_icons.css:99` `[data-icon="stop"]`, `public/images/icons/stop.svg`, `public/themes/madmans-box/theme.json:233` `"stop"` + `public/themes/madmans-box/icons/` | HAS the registry twins; LACKS the two names, two masks, four SVGs, two theme entries. |
| `_chat.css` | `styles/qt-components/_chat.css:956-967` the OLD `.qt-chat-stop-button` pair BEFORE `.qt-chat-composer-send` (the specificity bug v4 fixed); `:3144-3155` `.qt-participant-stop-button`; `.qt-chat-composer-actions` has NO rule (retired by p4.9l — `:1304-1309`'s comment) | HAS pre-drift; port: move the stop pair AFTER `.qt-chat-composer-send:hover`, add the interrupt rules + keyframes + reduced-motion, add the actions container rules (the rider's shape), delete the participant pair. ⚠ v5's composer is "v4's real 2-column gutter grid" (`:1304-1309`) — the actions column must be placed in THAT grid, not v4's flex row (§6 T9). |
| `core-contract.ts` | `:139-144` `ChatTurnActionRequest`; `:2646` the `Request` union; `:5089-5175` `ChatStreamFrame` (`skipped`, `chainComplete`, `paused`, `heldUserTurn`, `hostAnnouncement`) | HAS the mirror; add the two request interfaces, their responses, `handAnswered?` / `yielded?` / `stopped?` on the frame. |
| `showInfoToast` / `showErrorToast` | `ui/toast.service.ts:53` `ToastService.showInfo` / `showError` / `showWarning` | HAS. |

### 2.6 How a per-chat in-flight turn CAN be addressed in v5 (facts, not a design)

1. **The thread.** `spine.rs:2526-2551`: each send is `std::thread::spawn` +
   a `new_current_thread` runtime; the driver's `ChatSendFuture` only awaits a
   `oneshot`. `JoinHandle::abort` / `AbortHandle` would abort the AWAITING
   future, not the thread's work; `std::thread` has no cancel. So the stop is
   cooperative: a shared flag the thread polls at v4's eleven guard sites +
   a wakeable the watchdog races.
2. **Primitives available in the default core build.** `Cargo.toml:112`
   `tokio = { default-features = false, features = ["sync", "time"] }` and
   `:121` `futures-util = { default-features = false, features = ["alloc"] }`;
   NO `macros` (so no `tokio::select!` in core — the web/host crates have it,
   `quilltap-web/Cargo.toml:23`, `quilltap-host/Cargo.toml:27`), NO
   `tokio-util` anywhere in the workspace (no `CancellationToken` without a
   NEW dependency — a ruling, §8 Q1). `tokio::sync::{Notify, watch,
   broadcast}` and `std::sync::atomic::AtomicBool` ARE available; a
   `watch::Receiver::changed()` or `Notify::notified()` raced with
   `rx.recv()` via `futures_util::future::select` / a hand-rolled `poll_fn`
   inside the existing `tokio::time::timeout` is the shape
   `WatchedStream::recv` (`:209-266`) can take. `Notify` is runtime-agnostic
   (works across the send thread's runtime and the engine's).
3. **The settle wait.** v4's `settled: Promise<void>` + 15 s race →
   `tokio::sync::Notify`/`oneshot` resolved in the thread's `finally` (the
   unregister), awaited under `tokio::time::timeout(15 s)` in the action arm
   (the engine's dispatch runs on the host runtime, which has `time`).
4. **Where the registry can live — two precedented homes:**
   (a) a `static` in core keyed by `chat_id` (the `activity_registry.rs`
   twin of v4's `processSingleton`; `Mutex<HashMap<String, Vec<Arc<Entry>>>>`),
   reachable from BOTH the engine's action arms and the spine's `run_send`
   with no new seam — the memory-note trap: parallel tests in one binary
   must use UNIQUE chat ids and never reset it globally; (b) a per-`ChatSpine`
   field (`spine.rs:1082-1134` — the spine is `clone_state`d per send, so the
   registry would be an `Arc` shared by the clones) exposed to the engine
   through TWO NEW `ChatSendDriver` methods (`stop_turn(chat_id) ->
   StopActiveTurnsResult`, `set_hand_raised(chat_id, raised) -> usize`) the
   engine arms call like `chat_send` does (`engine.rs:6962-6980`,
   `EngineState::Ready(r).chat_send`). (b) keeps "chat dispatch not assembled"
   semantics for a read-only embedder; (a) answers `stopped: 0` there (v4's
   "no stream in this process"). Ruling owed (§8 Q2).
5. **How the handle reaches the guard sites.** v4's ambient store is
   `AsyncLocalStorage`; v5 has no task-local in core (§2.1). The handle is
   threaded EXPLICITLY: one `Arc<TurnControlEntry>`-shaped field on
   `OrchestratorDeps` (`:529-636`, the field every guard site already has in
   hand) → `FinalizeOptions` (`message_finalizer.rs:592+`), the two loops'
   option structs, `AttemptEmptyResponseRecoveryOptions`
   (`provider_failover.rs:291-306`), and the watchdog context at the seven
   sites. The enclave's `StepDeps` and the canned differentials leave it
   `None` (= v4's `undefined` outside a stream) — the `OrchestratorDeps::
   web_search: None` precedent (`:596-603`).
6. **The host driver lanes (P4.1).** Provider IO (`quilltap-host/src/
   providers.rs`; the byte pump `streaming_provider.rs:592-620`) already
   unwinds on a dropped receiver — the stop needs NO socket abort (v4 does not
   abort the socket either; it `void`s `iterator.return()`). Cadence
   (`host.rs` job pump / stuck reset) is untouched. The Tauri shell
   (`quilltap-tauri`) dispatches the same `Request` enum — no per-variant
   code there (`rg Request:: crates/quilltap-tauri/src` is empty).
7. **The SPA side.** `core.dispatchExpect({type:'chatStopTurn'…})` returns
   when the server's settle wait returns (≤ 15 s); the chain-close frames
   arrive on `core.events$` (`salon-conversation.ts:3740-3746`) and settle
   the vertical exactly as a normal turn's do — v4's "the stream's own done /
   chainComplete events then settle this side" (`useSSEStreaming.ts:1468-1470`).

---

## 3. Schema / migrations / boot ensures

**NONE.** No table, column, index, `migrations_state` id, `fresh_schema.json`
or `migration_indexes.json` change. The Host `turn-yield` message is a
`chat_messages` row with a NEW `systemKind` VALUE (`'turn-yield'`) and a NEW
`hostEvent` shape only in that it carries a one-key `{participantId}` like
`turn-pass` / `nudge` — no schema. ⚠ Every v5 reader that SWITCHES on
`systemKind` (the transcript projection, the Almanack, export / `.qtap` /
backup carriers, the memory extraction's Host filters, `participant_filters`)
must be censused for an exhaustive match that would refuse or mislabel
`turn-yield` where v4 treats an unknown Host kind generically — v5's
`skip_signal.rs` / `turn_state.rs` / `services/extraction/turn.rs` /
`transcript_projection.rs` all mention `turn-pass` (`rg` hits in §2 above);
the lane runs the census (§4.6).

---

## 4. The differential plan

### 4.1 Tier 1 (exact) — the pure twins

| Unit | Oracle | Existing family it extends |
|---|---|---|
| `matches_sentinel_line` + the unchanged `[NOTHING TO ADD]` behaviour | `harness/oracle/cases/skip-signal.ts` (imports v4's real `skip-signal.ts`) grows a `matchesSentinelLine` op over the wrapper/bracket/punctuation corpus with phrases `nothing to add` / `yield` | `crates/quilltap-harness/tests/skip_signal_equivalence.rs` (recipe: `cd ~/source/quilltap-server && npx tsx …/skip-signal.ts > /tmp/oracle-skip-signal.ndjson`, `QT_ORACLE_SKIP_SIGNAL`) |
| `detect_yield_sentinel` (all nine forms + the name-prefix guard + `'   \n  '` + the leading-bracketed regex incl. `\s` Unicode and wrapper chars) | a NEW `yield-signal.ts` case (or ops in `skip-signal.ts`) importing v4's real `@/lib/chat/turn-manager` | a NEW `yield_signal_equivalence.rs` modelled on `skip_signal_equivalence.rs` |
| `build_turn_yield_instruction(operator_name)` bytes | same case: a `buildTurnYieldInstruction` op over names incl. `'The User'`, a Unicode name, an empty string | same |
| the two Host texts `build_turn_yield_content` / `build_turn_yield_opaque_content` | `harness/oracle/cases/post-office-host.ts` or `host-gateway.test.ts` — whichever already imports `host-notifications/writer.ts`'s builders (the turn-pass builders are pinned somewhere: find `buildTurnPassContent` in `harness/oracle/cases/`) | that family |
| the SSE frame bytes: `handAnswered`, the two `done` shapes, the two `chainComplete` shapes | the `chat_events.rs` self-tests (`:880-1010` — the skip `done` literal at `:959` is the model) + the orchestrator tier-3 frame diff | `chat_events` unit tests + §4.3 |

### 4.2 Tier 2 (structural DB diff)

| Unit | Oracle | Family |
|---|---|---|
| `post_host_turn_yield_announcement` row (all columns incl. `systemKind`, `hostEvent`, `opaqueContent`) | the host-notifications oracle that drives v4's real writer (the turn-pass case) grows a `postHostTurnYieldAnnouncement` op | the family that pins turn-pass rows (`turn_state` / `host-gateway` — locate by `postHostTurnPassAnnouncement` in `harness/oracle/cases/`) |
| `?action=stop-turn` with no stream: `{stopped:0, settled:true, paused}` + the `isPaused` write on a multi-character non-autonomous unpaused chat / NO write on autonomous / single / already-paused; `?action=raise-hand` with no stream: `{raised:false, active:false}`; the `raised: null` 400; the unparsable-body → `raised: true` | `harness/oracle/cases/salon-skip.test.ts` (drives v4's REAL `handleTurnAction` over the committed salon GROUP fixture — the same route file `handlers/post.ts`) grows the two actions | `crates/quilltap-harness/tests/salon_skip_equivalence.rs` (`QT_ORACLE_OUT=/tmp/oracle-salon-skip.ndjson npx jest -- salon-skip`; `QT_ORACLE_SALON_SKIP`) |
| the `CHAT_POST_ACTIONS` order (`stop-turn`, `raise-hand` after `turn`) + the `Unknown action` `availableActions` bytes | the web route family that pins the constant | `wardrobe_routes.rs:936-950` order tests + the chats-routes equivalence family |

### 4.3 Tier 3 (mocked LLM) — the spine paths

The home is `crates/quilltap-harness/tests/orchestrator_tier3_equivalence.rs`
+ `harness/oracle/cases/orchestrator-tier3.test.ts` (both sides copy the same
two-DB seed fixture, the oracle drives v4's REAL `handleSendMessage` with a
canned `streamMessage` mock that rule-matches by stream label — `:278-395`;
v5 drives `process_message` + `execute_turn_chain` on `QueuedStreamingProvider`
`:1078-1200`). Rows to add, each comparing the frame sequence byte-for-byte
(§1.11), the `ProcessMessageResult`, and the DB delta:

- **Yield, bare**: raise the hand before the (chained) turn; canned reply
  `[YIELD]` → `handAnswered{yielded:true}`, `hostAnnouncement` (the turn-yield
  row), the `done{skipped,yielded}`, `chainComplete{yielded, paused:false}`;
  DB: ONE Host row, NO assistant row, NO rotation change
  (`spokenThisCycleParticipantIds` unchanged — the difference from a skip),
  `lastTurnParticipantId = null`, mail alerts marked `turn-yield`.
- **Yield with words**: `"After you, my dear."\n\n[YIELD]` → the finalizer's
  frames with `content` = cleaned, THEN `hostAnnouncement`; DB: assistant row
  with the cleaned text + the Host row AFTER it (`createdAt` order).
- **Yield + a tool call + no words** (the `toolMessages.length > 0` arm of
  the bare-yield gate) → tool-only `done` then `hostAnnouncement`.
- **Decline**: an ordinary reply → `handAnswered{yielded:false}`, normal
  finalizer; the hand is consumed (a second chained turn gets NO note —
  assert the second turn's recorded prompt lacks `[Turn note from the Salon`).
- **Hand consumed only by an LLM-voiced, non-held turn**: raise before a
  user-controlled seat's turn → no note, hand still up for the next LLM turn;
  raise into a PAUSED room (the held branch) → no note.
- **Note placement**: the recorded `messages` of the asked turn end with the
  skip note THEN the yield note (a skip-eligible seat), both absent from the
  persisted rows.
- **Stop, prose streamed** (needs a GATE-ABLE provider — the queued providers
  pre-fill the channel from a `Vec` (`:1178-1200`): ADD a provider whose
  sequence pauses on a `Notify` after N chunks so the test can flip the stop
  between chunks; the oracle side drives v4's `streamMessage` mock the same
  way with a latch promise — the `withStallWatchdog` test's `silentAfter`
  shape, `stream-watchdog.test.ts:25-37`): `[LLMStream]` INFO, finalizer
  `done` with the partial text, NO `confirmationResult` pass / dice TOOL rows
  / `pascalResult` / `carinaAnswer`, `reasoningContent` ABSENT on the done
  frame and NULL in the row, `chainComplete{stopped, paused:true}`; DB: the
  assistant row = the partial prose, `isPaused = 1` (written by the ACTION,
  so the corpus row must call the action, not just flip a flag).
- **Stop before the first chunk, no tools** → the `done{stopped:true}` frame,
  NO assistant row, NO `Empty response` WARN, NO recovery (`llm_logs` has ONE
  CHAT_MESSAGE row, not a retry's second).
- **Stop between tool batches** (native): tools already run are saved, the
  detected-but-unrun batch is dropped, `Native tool loop halted…` INFO,
  `done{toolsExecuted:true}`. Text loop: the same with `Text tool loop halted…`.
  Extend `native_tool_loop_tier3_equivalence.rs` / `text_tool_loop_tier3_
  equivalence.rs` (their oracles RECORD each `streamMessage` call — a stop
  planted between calls is observable as a shorter recorded call list).
- **Stop during a failover leg**: `primary_stream_tier3_equivalence.rs`
  (`QueuedStreamingProvider` `:610`) — the empty-reply + stop row asserts
  `Skipping empty-response recovery…` and NO second leg.
- **A fresh send after a stop is untouched** (the registry's per-entry
  semantics) — a v5 unit test on the registry module (v4's
  `turn-control.test.ts` ten cases ported 1:1 as Rust unit tests; no oracle
  — the module is new and the ten assertions ARE the contract).
- **The watchdog's four abort cases** — Rust unit tests in
  `stream_watchdog.rs` mirroring `stream-watchdog.test.ts:139-186`, with the
  INFO line capture-pinned (`CaptureLayer`, `:533-590`'s pattern) — ⚠ the
  memory note: a line emitted on the pump thread needs the dispatcher
  propagation the pump already does (`streaming_provider.rs:602-606`); the
  stop line is emitted on the CONSUMER side, so `captured_with` sees it.

### 4.4 The log-line census

Every new v4 line is capture-pinned per the `25b8fab6a`-round rule ("nothing
captured v5's lines" was a false claim once): `[TurnControl] Stream
registered` / `unregistered` / `Raised hand consumed by the next speaker` /
`Hand state set` / `Stop requested with no open stream` / `Stopping open
Salon stream(s)` / `Stopped stream(s) did not settle in time`; `[Interrupt]
Stop handled` / `Hand state changed`; `[TurnYield] Asking the next speaker…`
/ `Character yielded the floor…` / `Character carried on…` / `Bare yield
closed without a reply` / `Offering the yield note to the next speaker`;
`[TurnStop] Turn stopped by the operator; keeping what streamed`;
`[LLMStream] Abandoned a provider stream at the operator's stop`; `Native tool
loop halted…`; `Text tool loop halted…`; `Skipping empty-response recovery…`;
`[TurnOrchestrator] Chain stopped: the floor was yielded to the operator` /
`Chain stopped: the operator stopped the turn`; `[HostNotification]
Announcement posted` with `kindLabel: 'turn-yield'`. Levels: DEBUG for the
seven `[TurnControl]` state lines except `Stopping…` (INFO) and `did not
settle` (WARN); INFO for the rest; the context-manager's `[TurnYield]
Offering…` is DEBUG.

### 4.5 Tier R (CLI)

None — v4's `npx quilltap` has no interrupt verb.

### 4.6 SPA + Playwright

- Component specs (`chat-composer.spec.ts`, `participant-card.spec.ts`,
  `chat-stream.reducer.spec.ts`, a `salon-conversation` spec for `stop()` /
  `toggleRaisedHand()` with a dispatch stub): the three buttons' disabled
  ladders (`!isGenerating`, `!canRaiseHand`, `stopping`), the title/aria
  strings byte-for-byte, `aria-pressed`, the `-raised` class, the reducer's
  `stopped` reset + `handAnswered` transition, the decline toast
  `${name || 'The speaker'} carried on regardless. Your hand is lowered.`, the
  `active === false` toast, the error toast, the re-entrancy latch, the pause
  flip BEFORE the dispatch, the 3 s fallback.
- Playwright: the e2e mock (`apps/web/e2e/support/mock-llm.ts:21-27`) has
  `delayMs` — a Stop beat on the shared salon fixture (a 2-character room)
  with a slow stream: click Stop mid-stream → the partial bubble persists after
  the refetch, the room shows paused, the toast `Stopped. The floor is yours,
  and the room is paused.`; a raise-hand beat needs the mock to answer
  `[YIELD]` for the SECOND speaker (the mock's reply is fixed per server —
  `startMockLlm(reply)` — so an own-server beat: the `own-instance-e2e-fixture-
  choice` note says use the salon pair; `salon-chain-pause-toast-flow.spec.ts`
  is the model for a chain beat; `salon-paused-hold-flow.spec.ts` for the
  pause). Both gated LIVE on the server lane's flag until unification.
- The `systemKind` census (§3): grep v5 for every `"turn-pass"` /
  `TURN_PASS_SYSTEM_KIND` match arm and decide `turn-yield`'s treatment at
  each from v4's own handling of unknown kinds (v4 adds NO handling of
  `turn-yield` anywhere else in the diff — so every v4 reader treats it as
  "a Host message with a `systemKind`": the lane proves each v5 reader does too).

### 4.7 Runtime breaks at the pin (oracle builders / cases)

- `harness/oracle/cases/orchestrator-tier3.test.ts`, `message-finalizer-tier3
  .test.ts`, `native-tool-loop-tier3.test.ts`, `text-tool-loop-tier3.test.ts`,
  `primary-stream-tier3.test.ts`: these import v4's real services, which now
  import `@/lib/chat/turn-control` (→ `node:async_hooks` + `processSingleton`).
  Outside `runWithTurnControl` every helper answers `undefined` / `false`, so
  the EXISTING rows run unchanged at the pin (no stream registered ⇒ no stop,
  no hand) — NOT a break, but the regen MUST be from the pin's `node_modules`
  (`npm ci --offline`, the ledger's regen rule) and the jest cases that
  `jest.mock('@/lib/chat/turn-manager', …)` with an explicit export list
  (e.g. `turn-orchestrator.test.ts:19-36`'s pattern) will THROW on the
  barrel's new `detectYieldSentinel` import if any oracle case mocks the
  barrel that way — `rg "jest.mock\('@/lib/chat/turn-manager'" harness/oracle`
  at order-drafting time.
- The ledger's §1 IMPORT-DEAD note: `cfa0cba29` deletes
  `RosterContext.characterIds` / `RunCustomToolContext.characterIds` — a
  sibling (Pascal) lane's REAIM; the orchestrator-tier3 oracle builds a roster
  context (`:278+` "resolveCustomToolRoster") — if the REAIM lane has not
  landed first, this lane's regen of `orchestrator-tier3` at the pin hits it.
  ORDER: the REAIM lane precedes every tier-3 regen (ledger `:290+`).

---

## 5. Proposed lane cut — TWO lanes, disjoint files

### 5.1 LANE S (server, whole): `crates/quilltap-core` + `crates/quilltap-host` + `crates/quilltap-web` + harness

Edits: NEW `crates/quilltap-core/src/services/turn_control.rs` (the registry +
handle; v4 `turn-control.ts`), NEW `crates/quilltap-core/src/yield_signal.rs`
(v4 `yield-signal.ts`), `skip_signal.rs` (`matches_sentinel_line`),
`model/stream_watchdog.rs` (the race), the seven `watch_stream` sites
(§2.2), `services/orchestrator.rs` (the hand consume, the stop block, the
yield detection, `handle_turn_yield`, `announce_turn_yield`, the stopped-empty
branch, the two chain guards), `services/message_finalizer.rs` (G1a-d +
`ProcessMessageResult.yielded`), `services/native_tool_loop.rs` (G2),
`services/text_tool_loop.rs` (G3), `services/provider_failover.rs` (G4),
`services/build_context.rs` (the note, LAST), `services/host_notifications.rs`
(the writer), `post_office/mailbox.rs` (`MailAlertsVia::TurnYield`),
`services/chat_events.rs` (`HandAnswered`, `DonePayload.yielded/stopped`),
`api/types.rs` (two `Request` variants + two `Response` variants),
`api/engine.rs` (two arms), `api/salon.rs` or a NEW `api/chat_interrupt.rs`
(the two handlers), `api/chat_send.rs` (if home (b): the two trait methods),
`quilltap-host/src/spine.rs` (register the send; thread the handle into
`OrchestratorDeps` / the Carina constructors), `quilltap-web/src/
wardrobe_routes.rs` (`CHAT_POST_ACTIONS` + optionally a REST arm),
`stream_watchdog_wrap_census.rs`, the harness families of §4. Bumps: core,
host, web (harness frozen at 0.0.1110 per the standing rule).

Must NOT touch: `apps/web/**`, `help/**`, `docs/v4/**`, the Pascal run service
(`pascal/*` — bug 257's lane), the Inventory files, `CHAT_POST_ACTIONS`'
Inventory keys (the Inventory lane adds `inventory-equip` etc.; this lane adds
ONLY its two after `turn` — a textual merge point, §5.3).

### 5.2 LANE C (client, whole): `apps/web/**`

Edits: `src/app/chat/chat-composer.ts` (+spec), `src/app/screens/salon/
salon-conversation.ts` (`stop()`, `toggleRaisedHand()`, `handRaised` /
`stopping` / `canRaiseHand` signals, the `handAnswered` transition, the
composer bindings, the sidebar binding removal), `src/app/core/
chat-stream.reducer.ts` (+spec), `src/app/core/core-contract.ts` (the fenced
region — the two requests, responses, three frame keys), `src/app/chat/sidebar/
{participant-card,participants-section,chat-sidebar}.ts` (+specs; the Stop
removal), `src/app/ui/icon.ts` (two names), `src/styles/qt-components/
_chat.css` + `_icons.css`, `public/images/icons/{stop-sign,hand-raised}.svg`,
`public/themes/madmans-box/icons/{stop-sign,hand-raised}.svg` +
`theme.json`, the e2e beats (gated). Bumps: the SPA `package.json`
(0.5.881 → …).

Must NOT touch: anything under `crates/`, `help/`.

### 5.3 The Shared contract — name-level, binding on both lanes

```
// api/types.rs — Request (tag "type", camelCase)
ChatStopTurn  { chat_id: String }                                  // "chatStopTurn"
ChatRaiseHand { chat_id: String, #[serde(default)] raised: Option<bool> } // "chatRaiseHand"
//   raised: None | Some(true) → true (v4 prefault); Some(false) → false; a JSON `null`
//   is v4's ZodError 400 (the tri-state envelope precedent — `request_envelope`).
// Response
StopTurn  (StopTurnResultDto  { stopped: u64, settled: bool, paused: bool })   // key order stopped, settled, paused
RaiseHand (RaiseHandResultDto { raised: bool, active: bool })                   // key order raised, active

// REST (v4): POST /api/v1/chats/{id}?action=stop-turn   (no body)   → 200 the StopTurn body
//            POST /api/v1/chats/{id}?action=raise-hand  {raised?}  → 200 the RaiseHand body
// CHAT_POST_ACTIONS: … "turn", "stop-turn", "raise-hand", "add-participant", …

// Event channel — ChatEvent additions (serde names)
{ "handAnswered": true, "participantId": "<seat>", "characterName": "<name>", "yielded": bool }
done  += "yielded": true   (bare yield: after skippedParticipantId, before provider)
done  += "stopped": true   (stopped-empty: after toolsExecuted, before provider)
chainComplete.reason += "yielded" (paused:false) | "stopped" (paused:true)

// Host message
systemSender "host", systemKind "turn-yield", hostEvent { "participantId": "<seat>" }, participantId null

// core-contract.ts (LANE C mirrors; LANE S is the source of truth)
interface ChatStopTurnRequest  { type: 'chatStopTurn'; chatId: string }
interface ChatRaiseHandRequest { type: 'chatRaiseHand'; chatId: string; raised?: boolean }
interface StopTurnResult  { stopped: number; settled: boolean; paused: boolean }
interface RaiseHandResult { raised: boolean; active: boolean }
ChatStreamFrame += handAnswered?: boolean; yielded?: boolean; stopped?: boolean
// (characterName / participantId already exist on the frame type)

// CSS class names (LANE C owns the rules; the names are v4's)
.qt-chat-composer-actions  .qt-chat-stop-button  .qt-chat-interrupt-button
.qt-chat-interrupt-button-raised  @keyframes qt-hand-raised-pulse
// icon names: 'stop-sign', 'hand-raised'
```

LANE C's live beats and its `stop()` dispatch are GATED on LANE S's two
variants existing (a `chatStopTurn` against an unlanded server answers the
engine's unknown-variant refusal — the fallback branch `abandonLocalStream` +
refetch then runs, which is v4's "old server" path, so the SPA degrades to
today's behaviour until unification).

---

## 6. Traps

- **T1 — "the whole stream" is wider in v5's clock.** v4 registers the
  stream for `processMessage` + `executeTurnChain` + scene tracking +
  conversation render (`:222-287`). v5's `run_send` has NO scene-tracking
  call (unported, `spine.rs:1766`) — the registered span is the same minus
  that; `trigger_conversation_render` (`:1771`) runs INSIDE it (v4 too).
- **T2 — `stopActiveTurns` snapshots BEFORE aborting**, so a stream that
  registers while the stop is in flight is NOT stopped (test `:100`), and a
  stop with two open streams for one chat stops both (`stopped: 2`). v5's
  single-writer does not forbid two concurrent sends for one chat (two tabs),
  so the registry is a `Vec`/`Set` per chat, not an `Option`.
- **T3 — the hand survives a user-controlled turn and a held turn**
  (`:664-666` short-circuits before `takeRaisedHand()`), but NOT a stop
  (`takeRaisedHand` returns false on an aborted entry, and `stopActiveTurns`
  lowers it) and NOT the stream's end (the SPA lowers it in `finally`; the
  server entry is gone). A hand raised with NO stream open is a no-op
  (`streams: 0`) — it does NOT persist to the next send.
- **T4 — two operator-name sources.** The NOTE uses `userCharacter?.name ||
  'The User'` (`context-manager.ts:2207`); the Host TEXTS use
  `resolvedIdentity.name` (`orchestrator.service.ts:1839`, `:1812`). In a chat
  where the persona differs from the resolved speaking identity (Speaking-As
  another user-controlled seat), the two differ. Port both as they are.
- **T5 — `partialLength` is UTF-16 units**; `withReply: cleaned.length > 0`
  but the bare-yield gate is `cleaned.trim().length === 0` — a reply of
  whitespace logs `withReply: true` and still takes the bare path. Tiny, but
  a byte-compared log line.
- **T6 — `endChainOnStop` asserts `paused: true`.** Reachable only for a
  multi-character registered stream; the action pauses `chatType !==
  'autonomous' && isMultiCharacterChat` — a multi-character AUTONOMOUS room
  sent to from the Salon composer (an operator interjection into an
  autonomous room goes through `handleSendMessage` → registered) would be
  announced `paused: true` WITHOUT a pause write. v5 must reproduce (the
  `chainComplete` bytes), and the dogfood walk should look.
- **T7 — the double-toast hazard on the client.** v4's `stopStreaming` flips
  `setPauseState(true)` BEFORE the server's `chainComplete{stopped,
  paused:true}` arrives, so `announceChainPause`'s `isPausedRef.current` gate
  swallows the generic pause toast and only `'Stopped. The floor is yours, and
  the room is paused.'` shows. v5's `announceChainPause` reads `pausedBefore`
  from its reconcile (`salon-conversation.ts:3841-3846`) — the lane's `stop()`
  must update that belief before the dispatch resolves, or two toasts show.
  v4's `stopStreaming` also toasts UNCONDITIONALLY (even when the server
  answered `stopped: 0` and nothing was running).
- **T8 — `setPauseState(true)` on the client is a SECOND pause write**
  (`PUT /chats/{id}` with `isPaused: true`) racing the action's own write —
  both idempotent, but a tier-2 row for the action must not count the
  client's write; the e2e beat will see two `chats` invalidations.
- **T9 — the composer grid.** v5's composer is "v4's real 2-column gutter
  grid" (`_chat.css:1304-1309`) where v4's React composer is a flex row with
  the actions `div` as the last flex child. v4's rider gives the column
  `flex-shrink: 0` and 2.5 rem buttons; in v5's grid the column must occupy
  the Send cell. Measure in the browser; keep the class names.
- **T10 — `prefault` vs `default`.** Zod 4 `.prefault(true)` applies BEFORE
  parsing; `raised: "true"` (a string) is a ZodError, `raised: undefined` is
  `true`. `null` is NOT absent. The web edge's existing tri-state decoder
  (`request_envelope`) is the precedent for the 400.
- **T11 — the skip + yield interaction order.** v4 reads the yield FIRST
  (`:1779`) and the skip second, gated `!yieldedFloor` (`:1851`): a reply
  that is BOTH `[YIELD]` and `[NOTHING TO ADD]` on separate lines is a yield
  with the other sentinel kept as "words" (`cleaned` non-empty → the finalizer
  saves `[NOTHING TO ADD]` as the reply text). Faithful, odd, and a corpus row.
- **T12 — the `turnComplete` frame precedes the stop close on a chained
  turn** (`:434-440` then `:442`), so the SPA sees `turnComplete{skipped:false}`
  → `chainComplete{stopped}`; on the INITIAL turn there is no `turnComplete`.
- **T13 — reasoning is cleared on `streamingState` AFTER the primary stream
  forwarded live `reasoning` frames**, so the client's live thinking block
  was shown and the `done` frame then carries NO `reasoningContent` /
  `reasoningSegments` (`Omittable` absent, `chat_events.rs:236-240`) and the
  row's columns are NULL — assert both.
- **T14 — the watchdog's stall budget keeps running under the race.** A
  stop that lands AFTER the stall timer fired is a stall (an `Err`), not a
  stop; a stop that lands first ends the stream. The v5 `recv` must keep the
  `timeout` OUTSIDE the race (v4 races `withTimeout(next)` against
  `stopRequested`).
- **T15 — `stopped: 0` with `settled: true`** is the no-stream answer, and
  the SPA treats `stopped > 0` as "server handled it" — a stop that raced the
  stream's natural end (unregistered a tick before) falls to the local
  fallback + refetch. Not a defect; a dogfood row.
- **T16 — the Inventory survey's keys in the same map.** At the pin
  `handlers/post.ts` carries `inventory-equip` / `-unequip` /
  `-reset-defaults` (`:112-114`); this lane's `CHAT_POST_ACTIONS` hunk must
  insert ONLY `stop-turn`, `raise-hand` after `turn` and leave the
  Inventory lane its three (a merge-time order tripwire: pin both pairs with
  `at()` tests, `wardrobe_routes.rs:942-949`).
- **T17 — V8 regex semantics in `LEADING_BRACKETED_YIELD`:** `\s` matches
  the JS Unicode whitespace set (not Rust `\s` default, which is close but
  includes/excludes differently — U+FEFF is JS-`\s`, U+180E is not); the `i`
  flag without `u` folds only via the simple case mapping, and `[*_~"'`]*`
  is a plain class. Use the existing `jsregex`-style helpers / the `regex`
  crate with an explicit class.
- **T18 — a commit-message misdescription:** "the sidebar participant card's
  Stop button is removed" is right, but the message's "a fix for
  `.qt-chat-stop-button` losing to `.qt-chat-composer-send`" is the RULE
  ORDER move (equal specificity, later wins) — not a specificity change; v5's
  `_chat.css:956-967` has the SAME losing order today (the pre-drift bug is
  live in v5: the composer's Stop renders in the Send colours).

---

## 7. Tiering suggestion

**Tier 1 — must land (both lanes):** the registry + the two actions + the
dispatch variants + `CHAT_POST_ACTIONS`; the watchdog race at the seven
Salon-side sites; the eleven guards with their lines; the yield note (LAST),
`detect_yield_sentinel`, the Host `turn-yield` writer, `handle_turn_yield` /
`announce_turn_yield`, the `handAnswered` frame, the `done` flags, the two
chain closes; the SPA's three buttons, `stop()` as a dispatch + fallback,
`toggleRaisedHand`, the reducer additions, the toasts, the icons, the CSS
(post-rider shape), the participant-card Stop removal, the contract mirror;
the tier-1/2/3 corpora of §4 and the log census.

**Tier 2 — should land:** the REST arms for the two actions on
`chat_action_post` (v4 serves them over REST; v5's SPA does not need them,
but the CLI-less HTTP deployment (D-first-class no-auth Docker) might — a
pointer today, an arm tomorrow); the `systemKind` reader census outcomes
(§3) where a v5 reader needs a `turn-yield` arm; the e2e raise-hand beat
(needs an own-server mock answering `[YIELD]`).

**Tier 3 — explicit typed refusals:** none needed — nothing in the slice is
host-impossible. The storybook twin is NO-PORT (no twin), `help/` is the
vendor lane's, `API.md` is docs.

---

## 8. Open questions / rulings owed / ledger disagreements

- **Q1 (ruling): a NEW dependency or not.** `tokio-util`'s `CancellationToken`
  is the idiomatic Rust twin of `AbortController`, but the workspace has no
  `tokio-util` and the core build is deliberately scheduler-free (`Cargo.toml:
  100-131`). The no-dependency shape (`AtomicBool` + `tokio::sync::Notify`,
  raced with `futures_util::future::select`) is available today. The human
  decides; the survey leans NO new dependency (the `activity_registry.rs`
  precedent hand-rolled its task-local for the same reason).
- **Q2 (ruling): the registry's home** — (a) a core `static` keyed by chat id
  (v4's `processSingleton` twin; the engine arms and the spine both reach it;
  tests must use unique chat ids) vs (b) an `Arc` registry on the host's
  `ChatSpine` behind two new `ChatSendDriver` methods (the engine stays
  stateless; a read-only embedder answers "chat dispatch not assembled" to a
  stop, where v4 answers `stopped: 0`). §2.6 item 4.
- **Q3: should the enclave's autonomous turn be registered?** v4: NO (job
  child; "nothing ever stops or interrupts them"). v5's job runner is
  in-process, so registering it would make v5 STRICTLY more capable (a stop
  from the Salon composer would reach an autonomous turn) — a divergence from
  v4 and from the action's `paused` predicate (T6). The survey says NO
  (faithful); record it as a deliberate non-capability.
- **Q4: the `raised: null` / non-boolean 400 body.** v4's `raiseHandSchema.
  parse` throws a ZodError inside `handleRaiseHand`; the chats `[id]` route's
  error shell maps it (the lane measures the exact status + body at the pin
  with a `curl`-shaped jest row in `salon-skip.test.ts`; v5's `zod_issues.rs`
  + `request_envelope` are the twins).
- **Q5: `ChatSendResultDto.yielded`.** v4's `ProcessMessageResult.yielded` is
  stream-internal (the SPA reads `done.yielded` / `chainComplete.yielded`);
  the DTO is v5's dispatch reply. Leave the DTO alone unless a consumer needs
  it (YAGNI); the lane records the choice.
- **Q6: the REST arms.** See Tier 2. v4's SPA calls REST; v5's SPA dispatches.
  Without the arms, `POST /api/v1/chats/{id}?action=stop-turn` answers the
  loud pointer, not v4's body — a recorded edge divergence or two small arms
  (~30 lines; the `inform` arm is the precedent).
- **Ledger agreement:** the `2b5e8ee24` row (`:416`) is accurate in every
  checked particular (the registry shape, `stopActiveTurns`' bounded wait,
  the pause-first order, the response bodies, the `[Interrupt]` lines, the
  note placed LAST, `postHostTurnYieldAnnouncement`'s `systemKind`, the
  watchdog's quiet end, the finalizer's four skips, `encodeHandAnsweredEvent`'s
  payload, the `done` flags). Two refinements the row does not say: (i) the
  row calls the SPA's `ParticipantCard.tsx` loss "its Stop button" — correct,
  and v5's card STILL HAS it (a removal, not a no-op); (ii) the row lists
  `turn_state.rs`'s Host turn-pass as an intersection — the yield does NOT
  touch `turn_state` (no rotation write); the intersection is
  `host_notifications.rs` + `skip_signal.rs` + `post_office::mailbox`
  (`MailAlertsVia`). The `b8362beb5` row (`:420`) is accurate (port the
  post-fix shape).
- **Not in the ledger:** v5's pre-drift CSS bug (T18) and v5's "tracked
  deferral: a real stop dispatch" comment (`salon-conversation.ts:4222-4225`)
  are the two places this round retires a v5-recorded gap, not just absorbs
  v4 drift — worth a line in the round record.
