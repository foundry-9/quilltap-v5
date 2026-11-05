# Survey — the Salon turn wire + spine: `kind`-tagged stream frames (`604a082c4`), the raised-hand chip + gesture yield (`9eab8d2da`), the image transport budget (`29c1e2690` + `d439f6dc9`'s cap behaviour), avatar-job reuse (`d4ddeb943`), and the held `b15fe9404` remainder (P4.D343 items 41 / 29)

Read-only planning survey, **2026-11-04**. **Baseline `236c04e35`; round pin `624d0d25c`** (v4 `main`, `4.13.0-dev.8`). **v5 `main` `ff1651177`** (core 0.0.1427, host 0.0.223, web 0.0.256, cli 0.0.38, harness 0.0.1110, SPA 0.5.971). Every v4 line number is AT THE PIN (`git show 624d0d25c:<path>`; the working tree was never read); every v5 line number is at `ff1651177`. Nothing is taken from commit prose: each claim is from a shipped hunk. Where a hunk disagrees with the drift ledger's row, the correction is written out (§0).

**Commits in scope (sha, subject, v4 `dev.N`):**

| sha | subject | ledger row | this survey's lane |
|---|---|---|---|
| `604a082c4` | Refactor 4.12 spec 5: Salon stream frames as a tagged union (#142) | `drift-ledger.md:519` | W (server) + S (SPA); seat → T |
| `9eab8d2da` | salon: raised-hand status chip; gesture-only answers count as a yield | `:493` | W (server half) + S (chip) |
| `29c1e2690` | Refactor: one image transport budget on the way to a provider (#139) | `:518` | I |
| `d439f6dc9` | connection-profiles: cap images sent per request (default 4) — BEHAVIOUR only (the column / routes / carriers / editor are the D23 survey's) | `:502` | I |
| `d4ddeb943` | Fix: avatar regeneration no longer absorbs changes made mid-job | `:524` | T (rider) |
| `b15fe9404` | (PARTIAL, held) P4.D343 item 41 `simultaneous` + item 29 `.qtap` import refusals + the SPA re-enable (beat K) | `:473` | T |

Interplay only (behaviour left to other surveys): `db6190b44` (Deus Ex Machina: `useSSEStreaming` +6 in `sendMessage`, `ChatComposer`, `orchestrator.service.ts`, `context-builder.service.ts`, `courier-transport.service.ts`), `96156566b` (character kind: `native-tool-loop.service.ts`, `orchestrator.service.ts` +3), `c69a08882` / `ba6c8fe0f` (`context-builder.service.ts`).

## 0. Headline list — what a planner must not miss

1. **`604a082c4` re-shapes EVERY Salon frame, and v5 has ONE vocabulary for both dialects.** v5's `ChatEvent` (`services/chat_events.rs:319`) serializes the pre-4.12 bag, and the SAME enum feeds the Salon spine, Help chat, the Brahma console and the Scenario Builder through ONE `BroadcastSink` (`quilltap-host/src/spine.rs:1077-1086`). v4 keeps Help chat / Brahma / Scenario Builder / the one-shot loop on the old bag (`legacyFrameEncoder`), so v5 needs a **dialect on the sink**, not a re-shaped enum. §2.1 proposes rendering at the sink. That keeps all ~80 emit sites untouched, and with them the collisions with `96156566b` (`native_tool_loop.rs`) and `db6190b44` (`orchestrator.rs`). **Ruling R1.**
2. **The frame union is a binding two-lane contract.** It is written out byte-for-byte in §1.1. The key order comes from the PRODUCER's object literal, never the Zod schema: `encodeFrame` validates and then encodes the UNPARSED object (`stream-frames.ts:391-405`). `kind` is first. On a `done`, `outcome` is second, followed by the caller's keys in the caller's order. `seat` is LAST (`tagFrame` spreads, then appends, `:341-343`).
3. **The legacy dialect is byte-identical to today's v5 bytes for every frame a legacy route emits.** `toLegacyBag` (`:450-524`) inverts each kind. The only reorder is `done`'s outcome flags, which move to the END (`:476`), and no legacy route emits a non-`finished` done. So Help / Brahma / Scenario Builder families stay green. Measure this with a `toLegacyBag` row in the new tier-1 family; don't assume it.
4. **The swipe / "Try uncensored" streams switch to frames too.** `regenerate-swipe-stream.ts:34` uses a plain `TextEncoder`, so the stream is in the frames dialect. The status frame becomes `{"kind":"status","status":{"kind":"status","stage":…}}`, with the nested `kind` KEPT: the route passes the whole progress event (`:42`), and nothing strips it. The terminal frame is now `{"kind":"swipeDone","message":…}`. v5's `regenerate_swipe.rs:119-138 / :241-254` and its pinned unit test `the_status_frame_is_v4s_bytes_including_kind` move, along with the four web wire tests (§5.2).
5. **The SPA's `ScopedEvent` cannot survive as an intersection.** `ScopedEvent = {chatId?…} & ChatStreamFrame & CreationProgressFrame` (`core-contract.ts:5548`). `CreationProgressFrame.kind` is `'status' | 'log' | … | 'done' | 'error' | 'phase'` (`:5588`). A `kind`-tagged chat union intersected with that collapses most members to `never`. The SPA lane must make `ScopedEvent` a scope-discriminated union. The Green Room (`green-room.state.ts:46`) and the Almanack (`almanack-card.ts:271`) already filter by `progressId` first, so runtime is safe. Only the type changes.
6. **Seven SPA readers move, not one** (§2.3): the reducer, the Document-Mode `toolResult` subscription (`salon-conversation.ts:1053-1064`), the swipe reader (`regeneration.state.ts:163-192`), the hand state, the composer chip slot, and two e2e frame rewriters. The help / Brahma readers (`help-stream.ts:132`) stay on the legacy bag. Keep a `LegacyStreamBag` type for them.
7. **`9eab8d2da` adds a frame, `handAsked`, and a pure function, `isGestureOnlyReply`.** It also changes model-visible text: one sentence is added to the Turn note (`yield-signal.ts:163`). On the SPA side it REMOVES the "carried on regardless" toast in favour of the chip (`useSSEStreaming` at the pin has no such toast). v5 still toasts it (`salon-conversation.ts:4449-4453`). **Ledger correction:** the row says "`handAnswered` gains `inferred`". The SSE frame does NOT gain it (`turn-outcome.service.ts:205-209` still sends `{participantId, characterName, yielded}`). Only `TurnOutcome.handAnswered` (`:63`) and the INFO key `inferredFromGesture` (`:242`) gain it.
8. **v4's seat-frame dispatch fixed a bug v5 still has.** In v5's reducer the concurrent branch (`chat-stream.reducer.ts:301`) returns BEFORE `handAnswered` (`:374`), so a seat-tagged `handAnswered` is swallowed. That is v4's pre-`9eab8d2da` bug. v4's fix routes hand and board frames through `applyFloorFrame` from BOTH switches (`useSSEStreaming.ts:637-662, :672-677`). Port it with the frames. Note that `604a082c4`'s "concurrent empty bubble" fix is a v5 NON-bug: v5's pure fold reads its own `concurrentStreams` (`reduceConcurrent :476`), which has no React-updater race. Record it as converged by construction.
9. **`29c1e2690` + `d439f6dc9` are wholly UNPORTED behaviour on v5, and they are one unit.** v5 still has the PRE-`d439f6dc9` shape: there is no per-request image cap anywhere (no `max_images_per_request` in the tree). It has the 2 MiB Lantern-only budget (`files/llm_image_budget.rs:124`, `message_context.rs:858-905`), and the bug-106 adapter only on the uncensored reroute (`provider_failover.rs:858-873`; `walk_fallback_chain` `:1373` re-adapts nothing). At the pin the adapter is DELETED. The cap and the bytes are ONE ranked pass (`image-transport-budget.ts:283-322`) on the final array, for the primary and every substitute. **Port `d439f6dc9`'s behaviour AS `29c1e2690`'s pass. Never port the deleted adapter's cap.** Ledger correction for `d439f6dc9`: "adaptMessagesForProfile enforces the cap … (newest kept)" describes a file the pin no longer has. At the pin the cap counts only RECEIVABLE images, ranked upload > re-hydrated > Lantern, each newest first (`:96-101, :267-277`).
10. **Two wire / model-visible moves the `29c1e2690` row misses.**
    - (a) An uploaded IMAGE no longer goes through `processFileAttachmentFallback` in `loadAndProcessFiles` (`context-builder.service.ts:229-235`). It no longer appears in `fallbackResults`, so the `fileProcessing` frame (`orchestrator.service.ts:895-897`) is absent on an image-only send.
    - (b) A non-vision primary's image description moves from the user message's `messageContentPrefix` to the anchor message's prefix, inside the budget pass's capability arm (`image-transport-budget.ts:419-448, :482`). Over-budget images that can't be described become `[image omitted: over the request budget]` (`:208`): new model-visible text.
11. **IMPORT-DEAD at the pin — one more than the ledger lists.** `cases/file-attachment-tier3.test.ts:444` imports the deleted `lib/chat/message-attachment-adapter` (listed). **`cases/storyteller-pure.ts:37` imports `tagSseChunk` from `lib/storyteller/sse-tag`**, which `604a082c4` deleted. It is NOT in the ledger §1 IMPORT-DEAD list; add it as a stage-0 re-aim. `cases/encode-error-event.ts:60` still imports a live `encodeErrorEvent`, but with a plain `TextEncoder` it now emits the FRAMES dialect. Its family (`chat_error_frame_equivalence`) pins the Salon error frame and reddens as designed.
12. **P4.D343 item 41 should land in THIS round, on top of the frames lane.** `604a082c4` replaces the seat shape that v5's dormant `storyteller/sse_tag.rs:21-33` / `ChatEvent::Tagged` / `TaggingSink` (`chat_events.rs:496-536`) ported from the PRE-pin `tagSseChunk`. Production never runs those today, because `simultaneous` is refused (`storyteller_spine.rs:834-841`). Porting item 41 now means writing the `seat` field once. Item 29 is UNMOVED by the pin: no Storyteller import hunk in `236c04e35..624d0d25c`, `lib/import/**` moved only for `c539c82e0`'s Brahma remap. **Ruling R4.**
13. **`d4ddeb943` is one predicate:** `&& job.status === 'PENDING'` (`queue-service.ts:1477`). v5's twin collapses onto any pending OR processing job (`services/queue_service.rs:1307-1320`). The `wardrobe-tools-avatar-trigger.ts:76-77` oracle already works around the v4 job host claiming jobs mid-scenario. MEASURE whether that family now mints a second job on v4's side.

## 1. Per commit — the shipped hunks (what each DOES)

### 1.1 `604a082c4` — the frame union (THE SHARED CONTRACT)

Production: `lib/chat/stream-frames.ts` NEW (524), `streaming.service.ts` (+/−161), `tool-execution.service.ts`, `turn-outcome.service.ts`, `regenerate-swipe-stream.ts`, `orchestrator.service.ts`, `lib/storyteller/sse-tag.ts`, four `legacyFrameEncoder` adoptions, SPA `useSSEStreaming.ts` (748) + `useRegeneration.ts`. `docs/developer/API.md` (+56) documents the kinds (vendor lane).

**The producers.** `encodeFrame(encoder, frame)` is the only `data:` producer on the Salon path (`stream-frames.ts:391-405`). The chokepoint test (`__tests__/unit/lib/chat/stream-frames-chokepoint.test.ts`) scans `lib/services/chat-message`, `lib/storyteller` and `lib/chat/turn-manager` for `data: ${`, `tagSseChunk` and `taggingController`.

**Encoder profiles.**
- Plain `new TextEncoder()` → `'frames'` (`:358`).
- `legacyFrameEncoder()` → `'legacy'` (`:378-382`).
- `seatedFrameEncoder(pid)` → frames + `seat` (`:367-371`).

**Who uses which, at the pin:**
- **Frames dialect:**
  - `turn-stream.ts:48` (every Salon send / continue / nudge / storyteller-run stream);
  - `regenerate-swipe-stream.ts:34` (swipe AND `chats/[id]/messages/[messageId]?action=retry-uncensored` `:436`);
  - `orchestrator.service.ts:310` (the bid — `sinkController`, bytes discarded);
  - `:354` (seated, concurrent storyteller turns);
  - `autonomous-room-turn.ts:624` (drains `handleSendMessage` without reading it, `:336-346`).
- **Legacy dialect:** `help-chat/orchestrator.service.ts:77`, `brahma-console/orchestrator.service.ts:125`, `scenario-builder.service.ts:127`, `agent-loop/one-shot-loop.ts:182`.
- `app/api/v1/scenario-builder/route.ts:178` still hand-writes its `{error}` bag (outside the Salon path).

**Validation is dev/test only.** `validateFrames = NODE_ENV !== 'production'` (`:384`). It logs ERROR `[StreamFrames] Malformed frame emitted` `{kind, issues}` (`:397-400`), then sends the frame anyway. **Ruling R2** (NO-PORT, recommended).

**The wire union.** Key order is the emitted order. `?` means the key is absent when undefined (JSON.stringify drops it). Every member may end with `seat:{participantId, concurrent:true}`.

| kind | bytes after `"kind"` (in order) | v4 producer @ pin | v5 `ChatEvent` today |
|---|---|---|---|
| `content` | `text` | `encodeContentChunk` `streaming.service.ts:701` | `Content` |
| `reasoning` | `text` (cumulative) | `encodeReasoningChunk :711` | `Reasoning` |
| `status` | `status:{stage, message, toolName?, characterName?, characterId?}` (+ a nested `kind` on the swipe stream) | `encodeStatusEvent :840` | `Status` |
| `fileProcessing` | `info:[{filename, type, usedImageDescriptionLLM, error?}]` | `encodeFallbackInfo :684` | `FileProcessing` |
| `toolsDetected` | `names`, `arguments` (the old count key is GONE) | `tool-execution.service.ts:95-99` | `ToolsDetected` |
| `toolResult` | `index, name, success, result?, error?` (FLAT; `error` only on failure) | `:160-167` | `ToolResult` |
| `turnStart` | `participantId, characterName, chainDepth` | `encodeTurnStartEvent :883` | `TurnStart` |
| `done` | `outcome` then the caller's literal (below) | `encodeDoneEvent :785`, `encodeTurnDoneEvent :794` | `Done` / `TurnDone` |
| `turnComplete` | `participantId, messageId, chainDepth, skipped?` | `:893` | `TurnComplete` |
| `chainComplete` | `reason, nextSpeakerId, chainDepth, paused?, heldUserTurn?` | `:903` | `ChainComplete` |
| `error` | `message, errorType, details, userMessagePersisted?, pendingToolResultsPersisted?` (the old `error` key is now `message`) | `encodeErrorEvent :819` | `Error` + `EventPayload::ChatError` |
| `pendingExternalTurn` | `messageId, participantId, characterName` | `:873` | `PendingExternalTurn` |
| `posted` | `what` (`carinaAnswer` \| `hostAnnouncement` \| `pascalResult`), `message` | `:973 / :990 / :1010` | `CarinaAnswer` / `HostAnnouncement` / `PascalResult` |
| `storytellerRound` | `round` | `:913` | `StorytellerRound` |
| `storytellerBid` | `roundId, bid` (FLAT; was nested) | `:920` | `StorytellerBid` |
| `roundOpen` | `roundId` | `:932` | `RoundOpen` |
| `handAsked` | `participantId, characterName` | `:941` (NEW, `9eab8d2da`) | **absent** |
| `handAnswered` | `participantId, characterName, yielded` | `:953` | `HandAnswered` |
| `confirmationResult` | `messageId, confirmed, revised, notes, content?` (FLAT; was nested) | `:1024` | `ConfirmationResult` |
| `swipeDone` | `message` | `regenerate-swipe-stream.ts:51` | swipe `emit_done` |

**`done.outcome` values** come from `DoneOutcomeSchema` (`stream-frames.ts:107`): `finished | skipped | yielded | stopped | empty`. The four non-`finished` closes are at `turn-outcome.service.ts:310 / :384 / :434 / :477`. Every other caller defaults to `finished` (`streaming.service.ts:785`). The emitted `done` shapes at the pin:

- **Finalizer reply** (`message-finalizer.service.ts:526-543`):
  `kind, outcome:"finished", messageId, participantId, usage, cacheUsage, attachmentResults, toolsExecuted, turn, provider, modelName, routeTrail, isSilentMessage?, reasoningContent, reasoningSegments`.
- **Recovery** (`recovery.service.ts:343-349`, `:421-427`):
  `kind, outcome:"finished", messageId, usage:null, cacheUsage:null, attachmentResults:null, toolsExecuted:false`.
- **Courier** (`courier-transport.service.ts:170-180`):
  `kind, outcome:"finished", messageId, participantId, usage:null, cacheUsage:null, attachmentResults:null, toolsExecuted:false, provider, modelName, pendingExternalTurn:true`.
- **Turn closes** (`encodeTurnDoneEvent`, `streaming.service.ts:794-807`):
  `kind, outcome, usage, cacheUsage, attachmentResults, provider, modelName, messageId, participantId, toolsExecuted`, then `skippedParticipantId` (yielded / skipped), or `emptyResponseReason` (empty), or nothing (stopped / tools-only).

v5's `DonePayload` (`chat_events.rs:163-266`) and `TurnDonePayload` (`:271-298`) already carry these orders, with the boolean flags interleaved. **Dropping `stopped / skipped / yielded / emptyResponse` and inserting `outcome` second reproduces v4 exactly.** The P4.D348 R-N ruling's second struct pays off here.

**The seat.**
- `tagFrame` (`:341-343`) → `{...frame, seat:{participantId, concurrent:true}}`, with `seat` always last.
- In the legacy dialect a seat becomes `participantId` (only when absent) + `concurrent:true` at the end (`:452-454`). That is the old `tagSseChunk` bytes.
- `sharedStreamController` (`sse-tag.ts:11-24`) forwards and ignores `close`.

**Legacy inversion** (`toLegacyBag :450-524`). Each frame is rebuilt as its old single-key / flag bag:
- `toolsDetected` → `{toolsDetected: names.length, toolNames, toolArguments}`.
- `done` → `{done:true, ...rest, ...flags}`, with the flags at the END: `yielded` → `skipped:true, yielded:true`; `empty` → `emptyResponse:true`.
- `swipeDone` → `{done:true, message}`.
- `error` → `{error: message, ...rest}`.

**Parsing (client).** `parseFrame` (`:419-439`) returns null for blank / `[DONE]` / `{}` / non-JSON input. An unknown or malformed kind is WARNed ONCE per kind (`[StreamFrames] Ignoring an unrecognised stream frame` `{kind, issues[≤3]}`) and dropped. `doneLeavesNoReply` (`:322-334`) is true for skipped, yielded and stopped.

**Client semantics at the pin** (`useSSEStreaming.ts`):
- **Dispatch (`:873-881`).** `handleLine` sends a frame with `seat` to `handleSeatFrame` (`:666-728`) and everything else to `handleFrame` (`:730-871`). Both switches are exhaustive (`assertNever`).
- **Hand + board frames.** `applyFloorFrame` (`:637-662`) handles `handAsked`, `handAnswered`, `storytellerRound`, `storytellerBid` and `roundOpen` FIRST in both switches.
- **Seat frames.**
  - `turnStart` opens a bubble.
  - `content` appends to a stream-local `seatText` map (`:634`) and to the state.
  - `done` pushes `assistantMessageFromDone(frame, seatText.get(pid), {participantId: pid})` unless `doneLeavesNoReply`.
  - `posted` surfaces the message; `error` toasts.
  - Everything else is ignored.
- **Main stream.**
  - `status` sets the line and warns on `retrying` / `failing-over`.
  - `error` throws `SalonSendError(details ? message + ': ' + details : message, {userMessagePersisted, pendingToolResultsPersisted})`.
  - `done` warns on failed attachments (bug 94), then goes to `onIntermediateDone` in a chain and `onDone` otherwise; `pendingExternalTurn` → `fetchChat`.
  - `turnStart` with `chainDepth > 0` enters the chain (bug 242).
  - `chainComplete` → `onChainComplete({reason, nextSpeakerId, chainDepth, paused, heldUserTurn})`.
  - `fileProcessing` and `swipeDone` are ignored.
- **The send / continue `onDone` paths** use `doneLeavesNoReply(outcome)` and `outcome === 'empty'` (`:1152-1158, :1201-1205, :1384-1399, :1490`). The old `skipped || stopped` test is semantically identical (`yielded` always rode with `skipped`).
- **`useRegeneration.ts:164-197`** switches on `status`, `content`, `reasoning`, `error` (`details` appended) and `swipeDone`, and ignores the rest.

**Ledger row check (`:519`).** The row is correct, with three additions:
- (a) `encodeErrorEvent`'s first argument is now the frame's `message` key, so the Salon transport-shell error frame moves too (`turn-stream.ts:102-105`).
- (b) The swipe's nested `status.kind` survives.
- (c) The `toolsDetected` count key is gone in the frames dialect.

### 1.2 `9eab8d2da` — the raised-hand chip + gesture-only yield

Production: `lib/chat/turn-manager/yield-signal.ts` (+62), `turn-outcome.service.ts` (+8), `orchestrator.service.ts` (+5), `streaming.service.ts` (+12), SPA `RaisedHandChip.tsx` NEW (76), `ChatComposer.tsx` (+15), `SalonView.tsx` (+2), `useSSEStreaming.ts` (+90). `help/interrupting.md` (+17).

- **`isGestureOnlyReply(text)`** (`yield-signal.ts:54-77`), checked in this order:
  1. `trimmed` = JS `trim()`. False when it is empty or `> 280` UTF-16 units (`GESTURE_ONLY_MAX_CHARS :40`).
  2. False on `/\n\s*\n/` (JS `\s`) or `SPOKEN_QUOTE = /["“”«»„]/` (`:43`).
  3. Peel the wrappers `{'*':'*','_':'_','(':')'}` (`:46`) while the first character is a key and the last character is its close. For same-char wrappers, peel a run while `run < s.length/2`.
  4. `inner = slice.trim()`. A close inside `inner` → false.
  5. Return `peeled && /[\p{L}]/u.test(s)`.

  It is called from `detectYieldSentinel` AFTER the leading-bracketed check (`:139`), on `normalized` (the name-prefix-stripped text). It returns `{yielded:true, cleaned: normalized.trim(), inferred:true}`.
- **The Turn note** (`buildTurnYieldInstruction :152-166`). The paragraph after the sentinel now reads `…the floor will be theirs. A reply that is\nonly a gesture, with no words spoken aloud, is also taken as yielding.\n\nTo carry on instead, write your reply as normal and ignore this note entirely;\ndo not mention the raised hand unless you choose to acknowledge it in\ncharacter.` Note the RE-WRAPPED lines and the NEW blank line before "To carry on". This is model-visible.
- **`turn-outcome.service.ts`.**
  - `TurnOutcome.handAnswered?: {yielded; inferred?}` (`:63`).
  - `resolveTurnOutcome` sets `{yielded:true, inferred:true}` only for an inferred yield (`:93-95`).
  - The INFO `[TurnYield] Character yielded the floor to the raised hand` gains `inferredFromGesture` between `withReply` and `toolCount` (`:239-244`).
  - The `handAnswered` FRAME is unchanged (`:205-209`).
- **`orchestrator.service.ts:600-608`.** Right after the INFO `[TurnYield] Asking the next speaker whether to yield to the raised hand`, the orchestrator emits `encodeHandAskedEvent(encoder, {participantId, characterName: character.name})`.
- **SPA.**
  - `RaisedHandStatus = raised | asked{characterName} | yielded{…} | declined{…} | unasked` (`useSSEStreaming.ts:234-239`).
  - `isHandUp` = raised or asked (`:242-244`).
  - `settleHandAtStreamEnd`: raised → unasked, asked → null, otherwise kept (`:251-255`). It is applied at each stream's `finally` (`:1295, :1450, :1569`).
  - `setHandStatus(null)` on send (`:1032`), continue (`:1333`) and stop (`:1610`).
  - `handAsked` → `asked` ("The next speaker" fallback). `handAnswered` → `yielded` or `declined` ("The speaker" fallback). The **decline toast is REMOVED**.
  - `toggleRaisedHand` (`:1650-1672`):
    - sets `raised` or null optimistically;
    - only `data.active === false` lowers a raised hand, with the "Nobody is speaking just now. The floor is already yours." toast (the `604a082c4` form — the reply no longer overwrites a `handAsked` that beat it);
    - on error it lowers a raised hand and toasts `Could not raise your hand: …`.
  - `dismissHandStatus` keeps an up hand and clears an outcome (`:1675-1677`).
  - **The chip** (`RaisedHandChip.tsx`):
    - An always-mounted `aria-live="polite"` wrapper. Inside: `qt-chat-attachment-list mb-2` > `qt-chat-tool-result-chip[data-hand-state]`, the icon `hand-raised` (`qt-chat-attachment-chip-icon qt-chat-attachment-chip-icon-info`), and the label (`text-foreground`).
    - An × button `qt-chat-attachment-chip-remove`, absent while `asked`. Its tooltip / aria-label is `Lower your hand` (raised) or `Dismiss` / `Dismiss the raised-hand note`.
    - The labels (`:28-37`): `Hand raised — the next speaker will be asked to yield`, `${name} is weighing your raised hand…`, `${name} yields the floor — it is yours`, `${name} carried on regardless; your hand is lowered`, `The room fell quiet before anyone was asked — the floor is yours`.
    - Placement: right after `PendingInformChips` in `ChatComposer.tsx:466`.

### 1.3 `29c1e2690` — one image transport budget (with `d439f6dc9`'s cap, as the pin has it)

Production: `lib/chat/image-transport-budget.ts` NEW (508), `message-attachment-adapter.ts` DELETED (243), `context-builder.service.ts` (+/−258), `provider-failover.service.ts`, `streaming.service.ts` (+1), `lib/files/llm-image-budget.ts`, two import retargets (`orchestrator.service.ts:24`, `primary-stream.service.ts:46`). `help/connection-profiles.md`.

- **Constants.** `LLM_TRANSPORT_TOTAL_BASE64 = 4 * 1024 * 1024` (`llm-image-budget.ts:82`) replaces `LANTERN_IMAGE_BASE64_BUDGET` (2 MiB, deleted). `IMAGE_OMITTED_NOTE = '[image omitted: over the request budget]'` (`image-transport-budget.ts:208`). `ORIGIN_RANK` upload 0 / rehydrated 1 / lantern 2, `UNTAGGED_RANK` 3 (`:96-101`).
- **Tagging.** `tagForTransport(a, {origin, createdAt (epoch ms), describable})` → `{...a, transport}` (`:104-109`). `stripTransportTag(s)` (`:116-125`) is applied in `streamMessage` before the plugin AND the `llm_logs` request (`streaming.service.ts:519`).
  - **Uploads:** `loadAndProcessFiles` tags every IMAGE `upload`, `createdAt: Date.now()`, `describable:true`, and SKIPS the fallback for it (`context-builder.service.ts:229-235`). Metadata is now matched by id (`:227`).
  - **Re-hydrated** (`rehydrateUserAttachments :508-520`): `createdAt` = `Date.parse(message.createdAt) || 0` of the carrying row, `describable:true`. Non-images go through the fallback as before.
  - **Lantern walk** (`:1294-1315`): collects only; no fallback, no budget. `createdAt` = the MAX `createdAt` over ASSISTANT rows whose `attachments` include the id. `describable = hasPersistedImageDescription(repos, id)` (`:429-447`): a non-blank `generationRevisedPrompt`, `generationPrompt` or `description` on `repos.files.findById`. A vault mount-link id → false. It never throws; a DEBUG `Persisted-description probe failed; treating the image as not describable` `{fileId, error}`.
  - The `lanternImagePrefix` anchor splice is DELETED (`:1335`).
- **`planImageTransport(messages, policy)`** (`:283-322`). For each loaded attachment (`isLoadedAttachment` = string `id` + `mimeType`):
  - `!canReceive(mime)` → `capability`;
  - a receivable image → ranked;
  - other → `keep`.
  
  Rank: origin, then `createdAt` DESC (untagged = 0), then array order. Walk the ranked list: `kept >= maxImages` → overflow `count`; `cost (data?.length ?? 0) > bytesLeft` → overflow `bytes` and CONTINUE (a smaller image may fit); else keep.
- **`applyImageTransportBudget(messages, profile, deps, policy = imageTransportPolicyFor(profile))`** (`:359-508`).
  - **Nothing to do:** return the SAME array (plus the report).
  - **Per message:** a capability attachment goes through `processFileAttachmentFallback(…stripped…)`. Its prefix is appended to the message content; the outcome is `described` unless the result is `unsupported`. On a throw: the prefix `⚠️ Attachment Processing Failed: ${filename}\n${err}\n\n` + WARN `[ImageTransport] Fallback failed for an attachment the profile cannot receive; dropping it`.
  - **Overflow:** if `describable` (untagged counts as describable), call `generateImageDescription(stripped)`. On a throw: WARN `[ImageTransport] Describing an over-budget image failed; omitting it with a note`. An `image_description` result is appended as a prefix; anything else adds `IMAGE_OMITTED_NOTE\n\n`.
  - **Rebuild:** `{...message, content: prefix + content}`. `attachments` = the kept list, or the key is DELETED when empty.
  - **Logs:** INFO `[ImageTransport] Images over the request budget were described or omitted` `{...logContext, profileId, profileName, provider, modelName, maxImages, maxTotalBase64, base64Kept, kept, overCount, overBytes, described, omitted}` when anything overflowed. Then DEBUG `[ImageTransport] Transport report` `{…, report}`.
- **Call sites.**
  - Primary: the final assembled array (`context-builder.service.ts:1360-1373`, `logContext {chatId, characterParticipantId}`, `describeOptions {chatId}`).
  - Uncensored reroute (`provider-failover.service.ts:508-512`, `{chatId}`).
  - EVERY fallback understudy (`:852-856`, `{chatId, kind}`). This is new: v5 has nothing there.
- **The per-request cap** (`d439f6dc9`'s behaviour at the pin): `resolveMaxImagesPerRequest(profile)` (`connection-profile-utils.ts:154-162`) — an integer in `1..MAX_IMAGES_PER_REQUEST_CEILING` (50), else `DEFAULT_MAX_IMAGES_PER_REQUEST` (4). Its ONLY production caller is `imageTransportPolicyFor` (`image-transport-budget.ts:196-202`). The `d439f6dc9` hunks in the Lantern walk and the adapter are both superseded at the pin.
- **Deleted log lines:**
  - `Unseen assistant images exceeded the profile's per-request image cap; the oldest were described instead`;
  - `Unseen assistant images exceeded the per-turn byte budget; the oldest were not sent` (v5 has it at `message_context.rs:885-892`);
  - every adapter line.

### 1.4 `d4ddeb943` — avatar-job reuse PENDING-only

`queue-service.ts:1470-1479` (`enqueueCharacterAvatarGeneration`): `pendingJobs.find(type === 'CHARACTER_AVATAR_GENERATION' && status === 'PENDING' && payload.characterId === …)`. The INFO texts are unchanged (`:1482`, `:1498`).

### 1.5 `b15fe9404` remainder at the pin

- **Item 41 (`simultaneous`).** `handleStorytellerRun` (`orchestrator.service.ts:332-399`) is unchanged in logic since the baseline except the encoder / controller pair. A concurrent turn runs `processMessage` with `sharedStreamController(controller)` + `seatedFrameEncoder(pid)` (`:350-355`). `onTurnStart` / `onTurnEnd` emit nothing for a concurrent turn (`:363-379`). Its OWN depth-0 `turnStart` (emitted inside `processMessage`) carries `seat`. The final `chainComplete`: `stopped` → `{reason:'stopped', nextSpeakerId:null, chainDepth: turns.length, paused:true}`; `yielded` → `{reason:'yielded', …}`; otherwise `user_turn` (`:395-398`). `round.service.ts` (the plan, groups and `STORYTELLER_CONCURRENCY_PER_PROFILE`) is unmoved since the baseline (`git diff 236c04e35 624d0d25c -- lib/storyteller` touches only `sse-tag.ts`).
- **The SPA control at the pin** (`StorytellerBoard.tsx:408-416`): `onClick={() => run('simultaneous')}`, `disabled={disabled || chosenBids.length < 2 || destroyMixed}`, title `Simple and Lookup bids speak at once; Act and Destroy bids follow one at a time`. Unchanged since the baseline.
- **Item 29.** No hunk in the span touches the Storyteller import columns. The work order's item 29 (`p4.d343-…md:1081-1095`) stands as written, measured against the same `qtap-export.schema.json` (`:733 / :737 / :963` at the pin).

## 2. The v5 surface map (what v5 does today, by `path:line`)

### 2.1 Frames — server (`604a082c4` + `9eab8d2da`)

- **Vocabulary.**
  - `services/chat_events.rs:319-497` (`ChatEvent`, untagged, the legacy bag).
  - `DonePayload :163`, `TurnDonePayload :271`.
  - Storyteller frames `:474-492`.
  - `Tagged` + `TaggingSink` + `DiscardSink` `:496-543`.
  - `RecordingSink` + `events_json` `:939-962` (the tier-3 comparand).
- **Transport.**
  - `api/types.rs:6154` `Event` (flattened `chatId` / `roomId` / `progressId` envelope), `:6172` `EventPayload::Chat(ChatEvent)`.
  - `ChatErrorPayload :6312-6335` (the Salon shell + Brahma + Help share it), `Event::chat :6339`.
  - `realtime/types.rs:267-281` — the exhaustive tripwire; add one line if a variant is added.
- **Sink.** `quilltap-host/src/spine.rs:1077-1086` `BroadcastSink`, built for the Salon send `:1501`, Brahma `:1871`, Help `:2023`, the enclave step `:2110`, and Storyteller `storyteller_spine.rs:599`.
- **Error shells.** Salon `emit_send_error_frame` `spine.rs:2545-2568` (frames); Brahma `:1907` and Help `:2071` (legacy).
- **Edges (passthrough serde, no frame logic).** `quilltap-web/src/events.rs` (`id:` + `data:` + `: keep-alive`), `quilltap-tauri/src/events_pump.rs:57`. The swipe's REST edge `quilltap-web/src/messages_swipe_routes.rs` + `generator_sse.rs:137` re-frame the `SwipeProgress.frame` Value untouched.
- **Swipe frames.** `services/regenerate_swipe.rs:119-138` (`to_v4_frame`), `:241-243` (`emit_done` `{done:true,message}`), `:248-254` (`emit_error`). The pinned unit test is at `:1224-1240`.
- **Seat.** `storyteller/sse_tag.rs:21-33` `tag_frame` (the pre-pin top-level `participantId` + `concurrent`). It is reachable only through `TaggingSink`, which nothing in production builds while `simultaneous` is refused.
- **Hands.**
  - `services/orchestrator.rs:1765-1778` (`offer_yield` + the INFO; **no `handAsked` emit**).
  - `:3801-3815` (`hand_answered` emit), `:3822-3831` (the yield INFO, no `inferred_from_gesture`).
  - `yield_signal.rs:120-185` (`detect_yield_sentinel`, no gesture arm), `:191-205` (the OLD Turn note text).
  - `DetectYieldResult` (`:55-110`) has no `inferred`.
- **Ported by:** P4.2 (the spine + `/api/events`), P4.13/14 (`StreamMessage`), P4.140 (`fileProcessing`), P4.D207 (swipe stream), P4.D228 (retry-uncensored), P4.D301 (error-frame extras), P4.D335 (hands / stop), P4.D343 (storyteller frames + `tag_frame`), P4.D348 (`TurnDone`, R-N).
- **Collision census (who else edits these files this round):**
  - `orchestrator.rs` — `db6190b44` (Deus Ex Machina), `96156566b` (character kind).
  - `native_tool_loop.rs` — `96156566b`.
  - `courier_transport.rs` — `db6190b44`.
  - With the render-at-sink design (R1-A) the frames lane touches NONE of these except the 4-line `handAsked` emit and the yield INFO key in `orchestrator.rs`.

### 2.2 Images (`29c1e2690` + `d439f6dc9`)

- **Constant.** `files/llm_image_budget.rs:124` `LANTERN_IMAGE_BASE64_BUDGET` (+ its unit pin `:696`). The doc header `:21` names the Lantern walk.
- **The walk + budget.** `services/message_context.rs:37` (import), `:723-815` (`rehydrate_user_attachments`; keeps the fallback for images too). `:817-905` `spend_lantern_image_budget` (its recorded-divergence note `:845-852` RETIRES with it). `:1465-1483` (the walk). `:1519, :1564-1571` (the `lantern_prefix` anchor splice — DELETED at the pin). Tests `:1960-2070`.
- **The loaders.** `services/chat_files.rs:491` `load_and_process_files` (images go through the fallback today), `:585` `load_lantern_images` (fallback + prefix below the seam), `:646` `load_user_attachments`.
- **The adapter (DELETED at the pin).** `services/message_attachment_adapter.rs` — `:91` `collect_attachment_mime_types` (KEEP: v4 moved it into `image-transport-budget.ts:170-180`), `:118` `adapt_messages_for_profile`. Callers: `primary_stream.rs:1572`, `provider_failover.rs:794` (mime list), `:863` (adapt — the uncensored reroute), `orchestrator.rs:3707`.
- **Failover.** `provider_failover.rs:1373` `walk_fallback_chain` — no per-understudy adapt (`d439f6dc9`'s hunk was never ported).
- **The strip seam.** A `transport` key on the attachment `Value` must be removed before the request builders and the `llm_logs` request. The v5 funnel is the `StreamingCompletionProvider::stream_message` boundary (`model/streaming_provider.rs`), with the logging in `primary_stream.rs`. Measure the single point.
- **`fileProcessing` emit.** `orchestrator.rs:2946` — fed by `load_and_process_files`'s results, which will no longer contain images.
- **Ported by:** the bug-106 adapter in the P4.D135-era unit (`status-log.md:95573`); the Lantern byte budget P4.D199 (bug 151); the loaders P4.D198.

### 2.3 Frames — SPA consumers

- **The contract.** `core/core-contract.ts:5369-5545` `ChatStreamFrame` (a flat optional bag), `:5548` `ScopedEvent` (intersection — see §0.5), `:9677-9690` `SwipeProgressEvent` / `isSwipeProgressEvent` (its doc `:9671-9675` lists the old frames).
- **The reducer.** `core/chat-stream.reducer.ts:284-450` `reduceChatFrame` (presence-based branches), `:301` the concurrent early return (the hand-swallow bug), `:374-385` `handAnswered`, `:456-513` `reduceConcurrent`, `:532` `reduceDone` (reads `skipped / stopped / yielded / emptyResponse`). Fixtures: `core/__fixtures__/frame-trace.ts` (124). Specs: `chat-stream.reducer.spec.ts`, `chat/streaming-message.spec.ts`, `chat/message-list.spec.ts`, `screens/salon/salon-turn-controls.spec.ts`, `salon-conversation.spec.ts`.
- **The Salon screen** (`screens/salon/salon-conversation.ts`):
  - `:1053-1064` — Document Mode reads `frame.toolResult`;
  - `:1654` — `handRaised = signal(false)`;
  - `:4056-4066` — the fold;
  - `:4110` — stream end lowers the hand (→ `settleHandAtStreamEnd`);
  - `:4447-4455` — the `handAnswered` transition + the decline TOAST (removed at the pin);
  - `:4578` — stop;
  - `:4643-4660` — `toggleRaisedHand`, which still sets `res.data.raised === true` (the pre-`604a082c4` form).
- **The composer** (`chat/chat-composer.ts`): `:229` `<qt-pending-inform-chips>` (the chip goes right after it), `:630` `handRaised` input, `:739` `toggleRaisedHand` output, `:958-967` the button title.
- **The swipe** (`chat/regeneration.state.ts:163-192`) reads `data['status' | 'content' | 'reasoning' | 'error' | 'done']`.
- **Storyteller.** `chat/storyteller-board.ts:245-268` holds the disabled **All at once** (no click binding; the ruling comment carries v4's enable rule).
- **Unchanged (legacy):** `help/help-stream.ts:35,132`, `brahma/*`, `scenario-builder/*` (their own payload family).
- **e2e:**
  - `e2e/salon-chain-pause-toast-flow.spec.ts:82-145` rewrites frames on `frame['chainComplete'] === true`;
  - `e2e/salon-streaming-avatar-flow.spec.ts:54` has a local frame type with `turnStart?`;
  - `e2e/salon-interrupt-flow.spec.ts:427-431` (yield beat; the chip text does not contain "yields the floor to", so the beat's Host-note match holds — measure);
  - `e2e/salon-storyteller-flow.spec.ts:447-460` beat (K), skipped by name.
- **Ported by:** P4.6a/b (the reducer), P4.D336 (hands), P4.D344 (storyteller SPA), P4.D206 (swipe SPA).

### 2.4 `d4ddeb943`

`services/queue_service.rs:1284-1330` — `enqueue_character_avatar_generation`, whose `existing` predicate has no status check (`:1307-1320`). Ported by P4.123 (wardrobe-tools avatar seam) + P4.D238 (the two INFO lines).

## 3. Schema / D23

None in this area. `d439f6dc9`'s `connection_profiles."maxImagesPerRequest"` (the 23rd D23 move) is the D23 / profiles survey's. **This area CONSUMES** `resolve_max_images_per_request(&profile_row)` — v4's one reader (`connection-profile-utils.ts:154-162`). The fn, and wherever v5 parks it, must be agreed with that survey (§7 contract C-I).

## 4. Log lines (v4 text + keys + level → v5 home)

| v4 @ pin | level | keys | v5 home |
|---|---|---|---|
| `[StreamFrames] Malformed frame emitted` (`stream-frames.ts:397`) | ERROR | `kind, issues` | NO-PORT (dev/test only, R2) |
| `[StreamFrames] Ignoring an unrecognised stream frame` (`:433`, CLIENT) | WARN | `kind, issues≤3` | SPA logger, once per kind (lane S) |
| `[TurnYield] Character yielded the floor to the raised hand` (`turn-outcome.service.ts:239`) | INFO | `…ids, withReply, inferredFromGesture, toolCount` | `orchestrator.rs:3822-3831` (+ key) |
| `[ImageTransport] Images over the request budget were described or omitted` (`image-transport-budget.ts:493`) | INFO | listed in §1.3 | new `image_transport_budget.rs` |
| `[ImageTransport] Transport report` (`:505`) | DEBUG | `…, report` | same |
| `[ImageTransport] Fallback failed for an attachment the profile cannot receive; dropping it` (`:441`) | WARN | `…, attachmentId, mimeType, error` | same |
| `[ImageTransport] Describing an over-budget image failed; omitting it with a note` (`:464`) | WARN | `…, attachmentId, error` | same |
| `Persisted-description probe failed; treating the image as not describable` (`context-builder.service.ts:440`) | DEBUG | `fileId, error` | the Lantern walk |
| DELETED: `Unseen assistant images exceeded the per-turn byte budget; the oldest were not sent` | WARN | — | delete `message_context.rs:885-892` |
| DELETED: `Unseen assistant images exceeded the profile's per-request image cap…` | INFO | — | never ported |

## 5. Harness / oracle impact

### 5.1 IMPORT-DEAD at the pin (stage-0 re-aims)

- `harness/oracle/cases/file-attachment-tier3.test.ts:438-520` — family (E) `adapt`. Re-aim to `applyImageTransportBudget` / `collectAttachmentMimeTypes` from `@/lib/chat/image-transport-budget`. The Rust side is `file_attachment_tier3_equivalence.rs:708-830`. The "absent vs empty attachments" narrowing note in `message_attachment_adapter.rs:34-41` carries over: v4 still `delete`s the key (`image-transport-budget.ts:486`).
- **`harness/oracle/cases/storyteller-pure.ts:37 / :119-127`** — `tagSseChunk` is gone. Re-aim the `sse` rows to `encodeFrame(seatedFrameEncoder('p-tag'), frame)`, OR move them into the new `stream_frames_equivalence`. The fixture `harness/oracle/fixtures/storyteller-pure.json` (`spec.sse`) is re-shaped either way. **Not in the ledger's §1 list — add it.**

### 5.2 Families that REDDEN (v4 drift, not v5 bugs)

- **Every Salon tier-3 family that compares `RecordingSink::events_json()` against v4's SSE bytes:**
  - `orchestrator_tier3_equivalence`, `message_finalizer_tier3_equivalence`, `primary_stream_tier3_equivalence`, `courier_transport_tier3_equivalence`;
  - `native_tool_loop_tier3_equivalence`, `text_tool_loop_tier3_equivalence`, `tool_execution_process_tier3_equivalence` — the oracles use a plain `TextEncoder`, so they are in the FRAMES dialect even though the services are shared with Help chat;
  - `answer_confirmation_tier3_equivalence`, `carina_query_tier3_equivalence`, `refusal_ledger_tier3_equivalence`, `ollama_think_retry_tier3_equivalence`, `enclave_step_tier3_equivalence`, `tool_wire_call_site`, `tool_loop_leg_logging` (measure — it may read logs only).
  
  The fix is `RecordingSink` taking a dialect, never a mask.
- **Stay GREEN (legacy, prove it):** `help_chat_orchestrator_tier3_equivalence`, `brahma_orchestrator_tier3_equivalence`, `scenario_builder_tier3_equivalence`, `scenario_builder_midstream_failure`, `scenario_builder_dispatch_wire`.
- **`chat_error_frame_equivalence`** (oracle `encode-error-event.ts:60`, fixture `chat-error-frame.json`): v4 now emits `{"kind":"error","message":…}`. Split it into two dialect columns: the Salon shell frames, and the Brahma / Help shell legacy (call `encodeErrorEvent(legacyFrameEncoder(), …)` for the second).
- **The swipe:**
  - `regenerate_swipe_tier3_equivalence`, `retry_uncensored_tier3_equivalence`;
  - the `regenerate_swipe.rs:1224-1295` unit pins;
  - web: `messages_swipe_sse_route`, `message_swipe_stream_dispatch_wire`, `message_retry_uncensored_dispatch_wire`, `chat_retry_image_uncensored_dispatch_wire` (measure — the image retry may be JSON-only).
- **Web / host wire:** `quilltap-web/tests/chain_complete_held_turn_wire.rs`, `chat_send_smoke.rs`, `chat_send_submission_ledger.rs`.
- **`storyteller_pure_equivalence`** (IMPORT-DEAD above); `plan_storyteller_run_equivalence` (measure — it drives the run's frames).
- **`turn_control_equivalence`:**
  - its yield corpus gains gesture rows (red until ported);
  - the Turn note's bytes move (wherever `build_turn_yield_instruction` is compared — measure also in `orchestrator_tier3`, where the note is in the request);
  - `orchestrator_tier3`'s raised-hand rows gain the `handAsked` frame.
- **Images:**
  - `orchestrator_tier3_equivalence.rs:3555-3620`'s Lantern outcome pin RE-DERIVES: 4 MiB, ranked, the anchor prefix GONE (`lb_*` rows: `lantern_budget_drops_oldest`'s 1.5 MiB portrait is now KEPT under 4 MiB, but the default cap of 4 still applies). It needs the 23rd D23 column in the fixture, or `resolveMaxImagesPerRequest` defaults to 4 — measure.
  - `llm_image_budget_equivalence` + `llm-image-budget.test.ts` (the constant renamed).
  - `file_attachment_tier3_equivalence` (E); `message_context_leaves_equivalence` (the Lantern leaf is still exported — measure).
  - The `fileProcessing` rows in `orchestrator_tier3` for image uploads.
- **Avatar:** `chat_cast_routes_equivalence` (`chat-cast-routes.test.ts:636-639`, the (d) row) and `wardrobe_tools_avatar_trigger_equivalence` (`wardrobe-tools-avatar-trigger.ts:76-77` — the job host's mid-scenario claim now mints a second job on v4's side; measure before re-baselining), `avatar_job_tier3_equivalence`.

### 5.3 NEW families

- **`stream_frames_equivalence` (tier-1, lane W).** Drive v4's REAL `encodeFrame` with three encoders (plain, `legacyFrameEncoder()`, `seatedFrameEncoder('p-seat')`) over a corpus with one row per kind × every optional, the five outcomes, the four `done` producer shapes, a swipe status with a nested `kind`, and an object with an undefined `result`. Compare v5's render byte-for-byte. Also `toLegacyBag` and `doneLeavesNoReply`.
  - Closest recipe: `chat_error_frame_equivalence.rs` + `encode-error-event.ts` + `fixtures/chat-error-frame.json` (shared corpus, `__ABSENT__` / `__UNDEFINED__` sentinels).
- **`gesture_yield_equivalence` (tier-1, lane W), or rows in `turn_control_equivalence`.** `isGestureOnlyReply` + `detectYieldSentinel` over UTF-16-hostile rows: astral characters adjacent to wrappers, `***x***`, `(*x*)`, `_a_ *b*`, 280 vs 281 units, CRLF blank lines, U+00A0 / U+FEFF whitespace, every quote mark, letters only in a non-Latin script. Plus `buildTurnYieldInstruction` bytes.
- **`image_transport_budget_equivalence` (tier-1 `planImageTransport` + tier-3 `applyImageTransportBudget` with a canned describer, lane I).** Rows: count-before-bytes, the bytes `continue`, exact fit `>` vs `>=`, untagged rank, capability with a throwing fallback, a non-describable overflow → the note, the same-array identity (`None`), the key deletion. Closest recipes: `file_attachment_tier3_equivalence.rs` (E) and the P4.D199 Lantern rows.
- **SPA:** `apps/web/oracle/stream-frames.recorder.ts` + `core/stream-frames.oracle.spec.ts`, recording v4's REAL `parseFrame` (incl. the null cases and the once-per-kind WARN) and `doneLeavesNoReply`. Recipe: `apps/web/oracle/carina-parser.recorder.ts` + `chat/carina-parser.oracle.spec.ts`.

## 6. SPA — the lane-S checklist

1. **The contract (`core-contract.ts`):**
   - a `StreamFrame` discriminated union exactly as §1.1, plus `SeatTag`, `DoneOutcome`, `ChainCompleteReason`, `FrameOf<K>`;
   - `ScopedEvent` as a union discriminated by scope;
   - a `LegacyStreamBag` (today's `ChatStreamFrame`, renamed) for `help-stream.ts`;
   - the `SwipeProgressEvent` doc re-written for the frames.
2. **`chat/stream-frames.ts` NEW:** `parseFrame` (no Zod in the SPA: a hand guard per kind, with the once-per-kind WARN), `doneLeavesNoReply`, `assertNever`.
3. **The reducer:**
   - an exhaustive `switch (frame.kind)`, and a seat switch FIRST on `frame.seat`;
   - hand + board frames handled in both switches (fixes §0.8);
   - `handAsked` carried like `handAnswered`;
   - `reduceDone` reads `outcome`;
   - `posted.what` replaces the three keys;
   - `toolsDetected.names` / `arguments`; `toolResult` flat; `confirmationResult` flat; `error.message`.
4. **The Salon:**
   - a `handStatus` signal (`RaisedHandStatus`) with `handRaised = computed(isHandUp)`;
   - `settleHandAtStreamEnd` at `:4110`; null on send, continue and stop;
   - `handAsked` → asked; `handAnswered` → yielded / declined; **drop the decline toast**;
   - `toggleRaisedHand` per §1.2;
   - Document Mode's `toolResult` subscription → `kind === 'toolResult'`.
5. **The chip:** `chat/raised-hand-chip.ts` (the §1.2 classes, labels and aria), placed after `<qt-pending-inform-chips>` in `chat-composer.ts:229`, with a `handStatus` input and a `dismissHandStatus` output.
6. **`regeneration.state.ts`:** switch on `status / content / reasoning / error / swipeDone`.
7. **e2e:** the frame rewriter in `salon-chain-pause-toast-flow.spec.ts` (`kind === 'chainComplete'`), the local type in `salon-streaming-avatar-flow.spec.ts`, and a NEW beat for the chip states (raised → weighing → yields / carried on / room fell quiet).

## 7. The lane split, by FILE OWNERSHIP

### Lane W — the frames keystone, server (+ `9eab8d2da` server half)

- **Creates:**
  - `services/stream_frames.rs` — `Dialect { Frames, Legacy }`, `render(&ChatEvent, Dialect, seat: Option<&str>) -> Value`, `outcome_of`, the legacy passthrough;
  - harness `stream_frames_equivalence.rs` (+ oracle case + fixture), `gesture_yield` rows.
- **Edits:**
  - `services/chat_events.rs` — `ChatEvent::HandAsked`; `RecordingSink::events_json_in(Dialect)`; DELETE `Tagged` / `TaggingSink` in favour of a seat on the sink;
  - `api/types.rs` — the `Event` / `EventPayload` dialect carrier; `ChatErrorPayload` rendered by dialect;
  - `realtime/types.rs` — the tripwire line, if a variant is added;
  - `quilltap-host/src/spine.rs:1077-1086` — `BroadcastSink { dialect, seat }`; the Salon / enclave / Storyteller sinks use `Frames`, Brahma and Help use `Legacy`; `emit_send_error_frame` uses frames;
  - `storyteller/sse_tag.rs` — retire, or reduce to the seat helper;
  - `services/regenerate_swipe.rs` (`to_v4_frame`, `emit_done` → `swipeDone`, `emit_error`);
  - `yield_signal.rs` (`is_gesture_only_reply`, `inferred`, the Turn note);
  - `services/orchestrator.rs` — ONLY `:1778` (the `handAsked` emit) and `:3822-3831` (the INFO key);
  - every §5.2 frames family's comparand → `events_json_in(Frames)`;
  - `chat_error_frame_equivalence` + its oracle;
  - `storyteller_pure` re-aim;
  - the web wire tests.
- **Contract W1 to lane S:** the §1.1 table, verbatim.
- **Contract W2 to lane T:** `BroadcastSink::seated(pid)` (or a `SeatedSink`) that sets `seat` LAST.

### Lane S — the SPA frames + the chip

- **Owns:** `apps/web/src/app/core/core-contract.ts` (the frames block, fenced), `core/chat-stream.reducer.ts` + spec + `__fixtures__/frame-trace.ts`, NEW `chat/stream-frames.ts`, `screens/salon/salon-conversation.ts` (hand state, transitions, Document-Mode subscription), `chat/chat-composer.ts` (the chip slot), NEW `chat/raised-hand-chip.ts`, `chat/regeneration.state.ts`, the three e2e specs, `apps/web/oracle/stream-frames.recorder.ts`.
- **Gate beats ACTIVATE-AT-UNIFY** against lane W.

### Lane I — the image transport budget

- **Creates:** `services/image_transport_budget.rs` (tag / strip / plan / apply / policy / `IMAGE_OMITTED_NOTE`, plus `collect_attachment_mime_types` moved here); harness `image_transport_budget_equivalence.rs` + oracle case.
- **Edits:**
  - `files/llm_image_budget.rs` (the constant + the header);
  - `services/message_context.rs` — rehydrate tags images; DELETE `spend_lantern_image_budget`; the walk collects + tags + probes; DELETE the `lantern_prefix` splice; the final `apply` pass;
  - `services/chat_files.rs` — images skip the fallback in `load_and_process_files`; `load_lantern_images` stops describing (or a new collect-only loader);
  - `services/provider_failover.rs` — the uncensored reroute + `walk_fallback_chain` per understudy;
  - DELETE `services/message_attachment_adapter.rs` and retarget its 4 importers (`primary_stream.rs:1572`, `provider_failover.rs:794`, `orchestrator.rs:3707` — a one-line import each);
  - the strip seam at the provider / log funnel;
  - `file_attachment_tier3` (E) re-aim; the `orchestrator_tier3` Lantern rows + outcome pin; `llm_image_budget_equivalence`.
- **Contract C-I with the D23 / profiles survey:** that lane lands `connection_profiles.maxImagesPerRequest` + `resolve_max_images_per_request(&Value) -> i64` (1..=50 else 4) in the profile utils; lane I only calls it. If the D23 lane is later, lane I stacks on it.

### Lane T — Storyteller `simultaneous` + import refusals + SPA re-enable (+ `d4ddeb943` rider)

- **Stage 2, after W.**
- **Owns:** `quilltap-host/src/storyteller_spine.rs` (remove the item-44 refusal `:834-841`; the concurrent groups, the per-profile semaphore of 4, a seated sink per concurrent turn, no `turnStart` / `turnComplete` from `on_turn_*` for concurrent turns, `concurrent_follower` → one Prospero briefing), `crates/quilltap-core/src/storyteller/{rounds,types}.rs` (only if the run loop lives there), `services/quilltap_import/entities.rs` (item 29), `apps/web/src/app/chat/storyteller-board.ts:245-268` (restore `(click)="onRun('simultaneous')"`, `[disabled]="disabled() || chosenBids().length < 2 || destroyMixed()"`, v4's title), `e2e/salon-storyteller-flow.spec.ts` (un-skip K), and the `storyteller_round_tier3` concurrent rows.
- **Rider:** `services/queue_service.rs:1307-1320` (`&& j.status == "PENDING"`) + a `chat_cast_routes` row (a PROCESSING job → a new job).

### Disjointness against the whole round

1. **`services/orchestrator.rs`** — W (2 tiny hunks), I (1 import line), the Deus Ex Machina lane, the character-kind lane. The hunks are disjoint; name the lines in each order.
2. **`services/message_context.rs`** — I vs the context-builder survey (`db6190b44` / `ba6c8fe0f` / `c69a08882` edit `context-builder.service.ts` → v5 `message_context.rs` / `build_context.rs`). Lane I owns the image functions (`:723-905`, `:1465-1571`); the context lane owns the rest. If the context lane is wave 0, stack I on it.
3. **`screens/salon/salon-conversation.ts` + `chat/chat-composer.ts`** — S vs the Deus Ex Machina SPA lane (`/deus`, Cmd/Ctrl+Shift+Enter, the ribbon). Use fences, and give each its own composer slot.
4. **`core/core-contract.ts`** — every SPA lane (fenced additive blocks). Lane S REPLACES `ChatStreamFrame` / `ScopedEvent`, which is not additive: land it first, or have the others write against the union.
5. **`services/quilltap_import/entities.rs`** — T (item 29) vs the D23 survey (the established-fact / character-kind columns). Use list positions.
6. **`services/native_tool_loop.rs`, `courier_transport.rs`, `message_finalizer.rs`, `tool_execution.rs`** — UNTOUCHED by W under R1-A (that is the point of R1-A).

## 8. Tiering

- **W, Tier 1:** the full frame union + both dialects + seat; `handAsked`; the gesture yield + the Turn note + the INFO key; the swipe frames; the Salon error frame; every §5.2 family re-pointed; `stream_frames_equivalence`. **Tier 3:** R2's NO-PORT recorded.
- **S, Tier 1:** the contract, the reducer, hands + chip, swipe, Document Mode, the e2e rewriter. **Tier 2:** the client `parseFrame` WARN; the chip e2e beat.
- **I, Tier 1:** tag / plan / apply on all three call sites; the constant; the deletions; `fileProcessing` without images; the strip in the log + plugin request. **Tier 2:** `hasPersistedImageDescription`'s DEBUG.
- **T, Tier 1:** item 41 server + the SPA re-enable + beat K. **Tier 2:** item 29; the avatar rider.

## 9. Fixtures

- No new DB fixture. `orchestrator-tier3.json` (+ builder) gains a `handAsked` send, gesture-yield rows, and image rows over 2 MiB and under 4 MiB with five or more images (the cap).
- If the builder reads `connection_profiles`, it needs the 23rd D23 column from the D23 lane's rebuilt fixture pair (memory note `rebuilt-fixture-pair-vintage`).
- Regenerate one family at a time from the pin with the sweep driver (`--v5w ~/source/quilltap-v5`).

## 10. Rulings owed to the human

- **R1 — the v5 shape of the frame union.**
  - (A) **Recommended:** keep `ChatEvent` as the producer vocabulary. Render to frames or legacy at the sink (`stream_frames::render`, a `Dialect` on `BroadcastSink` / `RecordingSink`). `outcome` is derived from the existing flags. It touches ~6 files, no emit site and no colliding lane.
  - (B) Rewrite `ChatEvent` as a `kind`-tagged enum plus `to_legacy_bag` (v4's literal structure). That is ~80 emit sites across 17 files, colliding with `96156566b` / `db6190b44`.
  - The chokepoint guarantee holds either way, as long as ONE renderer is the only producer.
- **R2 — `[StreamFrames] Malformed frame emitted`** is dev/test-only in v4. Recommend NO-PORT (v5 ships production behaviour), recorded. Optionally a `debug_assert!` in tests.
- **R3 — the SPA client WARN** `[StreamFrames] Ignoring an unrecognised stream frame`. Recommend porting it via the SPA logger, once per kind.
- **R4 — P4.D343 item 41 in THIS round** (lane T, after W). The human's 2026-11-04 ruling ("hidden until ported") retires when it lands. Recommend YES: the seat shape changes anyway, and the dormant pre-pin `tag_frame` / `Tagged` is dead code that W deletes. The alternative is to keep it deferred and delete the dormant tag code with no replacement.
- **R5 — the image budget's `describable` probe for vault (mount-link) images** is false by design (v4's comment `:85-92`), so over-budget avatars are OMITTED rather than described. v4-faithful; no ruling needed unless the human wants divergence. Flagged because it is model-visible.

## 11. Dogfood rows to queue (💸 = spends a model call)

- 💸 A Salon send, continue, nudge and chain on the Friday copy. The stream shows content, chained bubbles, the pause toast and the held-turn banner, with no console WARN about unrecognised frames.
- 💸 A swipe and a "Try uncensored". The status strip, live text and the final variant are selected as before.
- 💸 Raise a hand mid-chain. The chip goes raised → "X is weighing…" → "yields the floor — it is yours", or "carried on regardless" with NO toast. The ×s work. A hand raised on the last speaker shows "The room fell quiet…". A gesture-only reply (`*bows*`) counts as a yield, and the INFO has `inferredFromGesture: true`.
- 💸 Six images (uploads + a Lantern portrait) to a cap-4 vision profile. Four are sent and two described. The Inspector shows the description prefixes and the INFO. A text-only failover understudy gets descriptions. A vault avatar over budget shows `[image omitted: over the request budget]`.
- 💸 A Storyteller round "All at once" with two Simple bids. There are two concurrent bubbles, both keep their text after `done`, and the summary is marked (all at once).
- An outfit change while a portrait job is PROCESSING queues a second job.
- Help chat and the Brahma console stream exactly as before (the legacy dialect).
