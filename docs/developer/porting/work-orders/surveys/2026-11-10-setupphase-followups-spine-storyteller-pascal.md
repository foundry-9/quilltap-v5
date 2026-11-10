# Survey A: the Salon spine, Storyteller, Carina, Pascal

Pin: v4 `624d0d25c`, read through `git show` / `git grep` only. v5 main @ `8ad1a0640`. Read-only: nothing was built and no test was run. Every "measured" below is a source read; every runtime claim is marked UNMEASURED.

Tally: **OPEN 22 · CLOSED 4 · NO-ACTION 5 · NEEDS-RULING 2 · DEFER-TO-CATCH-UP 3.** The per-order index is at the end.

Past-pin check (`git log 624d0d25c..main -- <paths>`), so the planner knows what a smalls fix would NOT lose:

| v4 surface | Commits past the pin | Consequence |
|---|---|---|
| `lib/storyteller/**`, `app/api/v1/chats/[id]/actions/storyteller.ts` | none | Storyteller fold is safe to do now |
| `useSSEStreaming.ts` `runStorytellerSelection` (`:1467-1570`) | none inside the function (`348172b9c` adds an opening reader at `:1304+`, `b941280d5` +10 lines elsewhere) | safe |
| `lib/services/carina/carina.service.ts` | none | Carina FINDING safe |
| `lib/services/chat-message/turn-holds.service.ts` | none | `[TurnFairness]` safe |
| `lib/pascal/**`, `lib/tools/custom/**` | none | Pascal items safe |
| `orchestrator.service.ts` | `348172b9c`, `4b97636b2` | `run_send` deps fold (S7) is on rewritten ground |
| `message-finalizer.service.ts`, `turn-orchestrator.service.ts` | `b941280d5` (Turns mode) | clock seam / chain rows are on rewritten ground |
| `brahma-console/orchestrator.service.ts` | `3cc12d346` | only the turn cap; X3's three-line fix survives |

---

## A. The round's FIRST item: the Storyteller live run fold   [OPEN — ONE lane, server + SPA + e2e]

### A.1 What v4 does at the pin

- Route: `app/api/v1/chats/[id]/actions/storyteller.ts:48-70` `handleStorytellerRunAction` — parse, INFO `[Storyteller] Running a selection`, `await handleStorytellerRun(...)` (the PLAN runs here; a `StorytellerRoundError` answers 400/404/409 as JSON before any stream), then `return sseStreamResponse(stream)`. **The 200 response body IS the round's stream.**
- Server stream: `lib/services/chat-message/orchestrator.service.ts:363-401` — `onTurnStart` → `turnStart {participantId, characterName, chainDepth: depth}` (depth starts at 1, `round.service.ts:390`), the turn's frames, `onTurnEnd` → `turnComplete`; concurrent turns get neither and stream through `seatedFrameEncoder`; then ONE `chainComplete` (`:398-401`: `stopped`+`paused:true` / `yielded` / `user_turn`, `chainDepth: turns.length`).
- Client: `app/salon/[id]/hooks/useSSEStreaming.ts:1467-1570` `runStorytellerSelection`:
  - refuses with `One moment — the room is still speaking.` while `streaming || waitingForResponse`;
  - `fetch(...?action=storyteller-run)`; `!res.ok` → throw `errorData.error || 'The storyteller could not run that selection'`;
  - `await readSSEStream(reader, {...})` — **read to the body's close**. `onDone` / `onIntermediateDone` → `pushReply` (an optimistic assistant row `storyteller-${Date.now()}` attributed to the `currentParticipantId` the last `turnStart` set); `onTurnStart` resets the slot and sets the responder; `onTurnComplete` → `await fetchChat()` (a refetch PER sequential turn); `onChainComplete` → clear `concurrentStreams`, `fetchChat()`, `announceChainPause(event)`;
  - `finally`: clear everything, `settleHandAtStreamEnd`, `fetchChat()`, `notifyQueueChange()`.
- Render: `SalonView.tsx:1723` maps `concurrentStreams` to one `StreamingMessage` each; the ordinary slot's `StreamingMessage` returns `null` when neither waiting nor streaming.

### A.2 What v5 does on main

- **Host** `crates/quilltap-host/src/storyteller_spine.rs:887-941` (`StorytellerDriver::run`): a thread + current-thread runtime plans (`rounds::plan_storyteller_run`), answers a refusal through the oneshot, then **`:924 let _ = tx.send(Ok(()));` BEFORE `:925 state.run_planned_round(...)`**. The dispatch reply therefore settles as soon as the plan is good; the round streams afterwards on the chat's Event channel (`BroadcastSink::frames`, `:623`), ending with ONE `chainComplete` (`:660`).
- **The send's bridge does the opposite** (the model to copy): `crates/quilltap-host/src/spine.rs:2685-2712` runs `run_send` to completion on its thread and only then `tx.send(result)` — the `chatSend` dispatch settles AFTER the last frame was emitted.
- **Transport**: frames only ever ride the Event channel (`quilltap-web/src/events.rs` SSE; Tauri's event pump) — CLAUDE.md's invariant, and nothing here should change it. The dispatch reply is EMPTY (R-8; REST twin `quilltap-web/src/wardrobe_routes.rs:1061-1090`). Holding the reply does NOT put frames on the reply; it only makes the reply mean "the round's stream has closed", which is exactly what `chatSend`'s reply already means.
- **SPA** `apps/web/src/app/screens/salon/salon-conversation.ts:4399-4434` `runStorytellerSelection` → `foldTurnStream` (`:4291-4321`): subscribes to `core.events$` filtered by chat BEFORE dispatching, folds through `reduceChatFrame`, and **unsubscribes in the `finally` of `await dispatch()`** (`:4316`). With the early reply the subscription lives for milliseconds: no `turnStart`, no content, no seat frame is ever folded. `endTurnStream` then sets `stream` to null; the rows appear only through the refetch / realtime invalidation. This is true for in-order runs too — their beats pass because they count SETTLED rows.
- **Reducer** `apps/web/src/app/core/chat-stream.reducer.ts`: `reduceSeat` (`:375-440`) builds `concurrentStreams`; `turnStart` with `chainDepth > 0` sets `inChain` (`:549-559`), so a sequential run's `done` takes the intermediate-bubble path (`reduceDone`, `:654-709`); `chainComplete` sets `finished` (`:566-582`). The fold logic exists and is unit-tested (`salon-conversation.spec.ts:5947` "folds a two-turn chain") — it has simply never run against a live server.
- **Render** `apps/web/src/app/chat/message-list.ts:201-223`: `@if (stream(); as s) { <qt-streaming-message …> }` then one `<qt-streaming-message>` per `concurrentSlots()`. `streaming-message.ts:52` / `:74` draw a `.qt-chat-message-row` only when waiting or carrying content.

### A.3 The unifier's held-reply attempt — FOUND

`/private/tmp/claude-503/-Volumes-External-1-source-overflow-quilltap-v5/d1dcdcf4-ad40-44d9-b24f-0e6992769398/scratchpad/storyteller-hold-reply-attempt.patch` (102 lines, 2026-11-10 00:03). ⚠ It lives under `/private/tmp` and `scripts/tmp-sweep.sh` may delete it, so it is reproduced whole in §A.7. Three hunks:

1. `storyteller_spine.rs`: delete `:924`'s early `tx.send(Ok(()))`, send it after `run_planned_round` returns (a failure past the plan stays the stream's error frame; the reply stays EMPTY).
2. `storyteller_simultaneous_tier3_equivalence.rs` `run_v5` (`:387-440`): read frames with `try_recv` first and assert `!waited_for_a_frame` — "the run's reply settled before its round finished". **Red-first evidence survives beside the patch:** `sts3-red.log` (all 3 tests panic at `:439` on main's spine) and `sts3-green.log` (3 passed with the hold).
3. `salon-storyteller-flow.spec.ts:493`: (K)'s count becomes `qt-streaming-message:has(.qt-chat-message-row)`.

**NOT found:** any log of the run in which the held reply reddened (B)(C)(D)(E)(F2). The scratch's three storyteller e2e logs (`e2e-salon-storyteller-flow.log`, `fx-st.log`, `st-probe.log`) are all main-spine runs: 9 passed, (K) alone red (`Expected: 2 / Received: 0`). So **the cause of the (B)–(F2) reds is unrecorded** — the lane must reproduce it first (apply the patch, run the file once, read (B)).

### A.4 What each gated beat asserts, and what the hold can change

File `apps/web/e2e/salon-storyteller-flow.spec.ts` — its own server (port 4353) and bid-aware mock, started in `beforeAll`; beats run in file order G H I J A B C D E F2 K and SHARE the instance's state.

| Beat | Line | Asserts | Touches the run? |
|---|---|---|---|
| (A) | `:360` | Storyteller radio on, toast, a post fills the board with bids | no |
| (B) | `:381` | tick two, **Run in order**; `.qt-chat-message` with `TURN_REPLY` `toHaveCount(2)`; then a ONE-SHOT `allTextContents()` of `.qt-chat-message-row … .qt-chat-message-author`; `.qt-storyteller-summary` = `Last round: <ran order>` | **yes** |
| (C) | `:415` | a fresh post, **Let it lie**, summary `The last round was let lie.` | no (needs (A)'s mode + a non-busy room) |
| (D) | `:428` | Normal with a round open folds the board away; back to Storyteller | no |
| (E) | `:441` | `Invite by talkativeness` round-trips a reload | **no — pure settings** |
| (F2) | `:455` | Inspector lists ≥ 2 `Bid` rows under Storyteller Rounds | **no — reads earlier bids** |
| (K) | `:469` | **All at once**; `qt-streaming-message` `toHaveCount(2)`; replies `before + 2`; summary contains ` (all at once)` and both names; `turnCalls` +2, `bidCalls` +0 | **yes** |

Reading (UNMEASURED — the lane proves or kills each):

- **(E) and (F2) cannot be reddened by the run fold directly.** The only mechanism that reddens them is Playwright's worker restart after (B) fails: `beforeAll` re-copies the fixture pair into a fresh instance, (A) is not re-run, so the chat is back in Normal mode and has no bids. The same cascade explains (C) and (D). **Expect ONE independent red — (B) — and four cascades.** (CLAUDE.md records this class twice: "the flipped beats' worker-restart cascade".)
- **(B)'s likely premise break.** Today nothing renders until the refetch, so `toHaveCount(2)` is first true on two SETTLED `qt-message-row`s, each with its `.qt-chat-message-author` header (`chat/message-row.ts:211`). With the live fold the count can be true EARLIER on live bubbles: the streaming bubble (`streaming-message.ts:75-89`) is a `.qt-chat-message-row` holding a `.qt-chat-message` with the full reply and **no `.qt-chat-message-author`**. The very next line reads the authors once, without retry — it can read one name or none, and `expect(sorted).toEqual(names)` fails; the summary assert then compares against the short list. Fix shape if confirmed: count settled rows (`qt-message-row` / a row that has an author), or `expect.poll` the author list to length 2.
- **A second candidate in the SPA, not the spec:** v4 refetches the chat on every sequential `turnComplete` and pushes its optimistic row attributed to `currentParticipantId`; v5's fold keeps the intermediate bubble in `stream.messages` until the run-end reconcile. If (B) shows a mis-attributed or duplicated bubble mid-run, the fold needs v4's per-turn settle (`settleTranscriptAfterTurn()` on the `turnComplete` transition in `reportStreamTransitions`).
- **(K)'s premise** (measured by reading): while `stream()` is non-null v5 mounts the ordinary slot's `<qt-streaming-message>` host as well as the two concurrent ones — three hosts, the ordinary one empty once `turnStart` (seated) cleared `waitingForResponse` (`reducer :385-390`). v4 has no host element at all. Count `qt-streaming-message:has(.qt-chat-message-row)`.

### A.5 Recommended design

1. **Server holds.** Apply hunk 1: the reply is sent after `run_planned_round` returns, error or not. Contract unchanged in bytes: EMPTY reply; refusals (400 / 404 / 409, ZodError `Invalid request`) still answer BEFORE any frame; a failure past the plan is the stream's error frame and the reply is still `Ok`. Update the module doc (`:18-21`) and the R-8 comment (`:891-893`).
2. **The SPA folds on the Event channel until the dispatch settles** — `foldTurnStream` as it stands, the same shape `runTurn` uses for `chatSend`. No new frame, no new subscription rule.
3. **The frame that closes the fold is `chainComplete`** (exactly ONE per run on every path: `Ran` / `Stopped` / `Yielded`, and v5's `CONCURRENT_TURN_FAILURE_CONTAINED` keeps it on a failed seat). The dispatch settling is the unsubscribe; `chainComplete` is what the reducer reads (`finished`, `chainReason`, `chainPaused`) and what `endTurnStream` → `announceChainPause` consumes. A run that ends WITHOUT `chainComplete` (a `run_planned_round` `Err` → unseated error frame) falls to `state.error` → the one error toast (`:4424-4427`), already written.
4. **Do not move frames onto the reply** and do not add a "run finished" Event: both would break the transport-agnostic boundary for no gain.
5. Known residual (shared with `chatSend`, NOT new): over HTTP the POST reply and the last SSE frames ride different connections, so the reply can in principle overtake `chainComplete`. `runTurn` tolerates it (`state.chainReason || 'no_next_speaker'`, then the refetch). Record it; do not fix it in this lane.

**ONE lane, not two.** The SPA reds exist only against the held server, the contract between the halves is one sentence ("the `chatStorytellerRun` reply settles after the round's last frame; it stays EMPTY"), and the e2e file is the only instrument that sees either half. Splitting buys nothing and costs a union-only failure mode. Order inside the lane: tier-3 red-first (hunk 2) → server hold → run the e2e file once and read (B) → SPA / spec fix → (K)'s locator → the file green 11/11 (10 + the self-skipping (I)) twice.

- **Owns:** `crates/quilltap-host/src/storyteller_spine.rs`; `crates/quilltap-harness/tests/storyteller_simultaneous_tier3_equivalence.rs`; `apps/web/e2e/salon-storyteller-flow.spec.ts`; the `// === P4.D344 ===` region of `apps/web/src/app/screens/salon/salon-conversation.ts` (`:4381-4435`) and its spec cases (`salon-conversation.spec.ts:5947-6030`); `apps/web/src/app/core/chat-stream.reducer.ts` only if (B) proves a fold defect.
- **Differentials:** `storyteller_simultaneous_tier3_equivalence` (v4's REAL `handleStorytellerRun`; no oracle regen — the assert is v5-side); `salon-conversation.spec.ts`; the Playwright file by itself (e2e is serial-only).
- **Size:** M. **Traps:** the host crate change needs a rebuilt `quilltap-web` release bin before Playwright (memory `lane-spa-env-node-modules-and-release-bins`); `E2E_KEEP_SERVER_LOG`; a red beat restarts the worker and reddens every later state-dependent beat — read only the FIRST red.
- **Also named under this item in `phase-4.md:7243-7244` — the lazy-store-id-in-`queryFn` audit (`5ff2d0f7f`):** it is a documents / mount-store SPA item, not surveyed here; it does not belong in this lane.

### A.6 Storyteller differentials still missing (P4.D343 items 32 / 43) — see B1.

### A.7 The attempt, verbatim

```diff
diff --git a/apps/web/e2e/salon-storyteller-flow.spec.ts b/apps/web/e2e/salon-storyteller-flow.spec.ts
index b365321ac..3d5119930 100644
--- a/apps/web/e2e/salon-storyteller-flow.spec.ts
+++ b/apps/web/e2e/salon-storyteller-flow.spec.ts
@@ -490,7 +490,13 @@ test.describe('P4.D344 — Storyteller mode (the SPA half)', () => {
     const bidCallsBefore = plan.bidCalls;
     await board.getByRole('button', { name: 'All at once' }).click();
     // Two concurrent bubbles stream at the same time (v4 P4.D344 item 16).
-    await expect(page.locator('qt-streaming-message')).toHaveCount(2, { timeout: 30_000 });
+    // Counted by the bubble each draws: v5 keeps the ordinary slot's
+    // `qt-streaming-message` host mounted (empty) for the whole run, where v4's
+    // `StreamingMessage` renders nothing at all once it is neither waiting nor
+    // streaming.
+    await expect(page.locator('qt-streaming-message:has(.qt-chat-message-row)')).toHaveCount(2, {
+      timeout: 30_000,
+    });
     // Both settle into assistant rows carrying the speaking-turn reply.
     await expect(replies).toHaveCount(before + 2, { timeout: 60_000 });
     await expect(page.locator('.qt-storyteller-summary')).toContainText(' (all at once)', { timeout: 30_000 });
diff --git a/crates/quilltap-harness/tests/storyteller_simultaneous_tier3_equivalence.rs b/crates/quilltap-harness/tests/storyteller_simultaneous_tier3_equivalence.rs
index 4846e81cc..541b9a856 100644
--- a/crates/quilltap-harness/tests/storyteller_simultaneous_tier3_equivalence.rs
+++ b/crates/quilltap-harness/tests/storyteller_simultaneous_tier3_equivalence.rs
@@ -387,11 +387,30 @@ fn run_v5(fx: &Fixture, round_id: &str, plan: Plan) -> Ran {
             .await
             .unwrap_or_else(|e| panic!("the run refused: {e:?}"));
         // Every chat frame of the run, until ONE `chainComplete`.
+        //
+        // v4's 200 response IS the round's stream, read to its close
+        // (`runStorytellerSelection`), so the reply settles only once the round
+        // has finished: every frame through `chainComplete` is already on the
+        // channel when it does. Read them without waiting first; a frame the
+        // loop has to wait for means the reply came back early (the Salon folds
+        // a run's frames until its dispatch settles — an early reply left every
+        // all-at-once bubble unrendered).
         let mut frames = Vec::new();
+        let mut waited_for_a_frame = false;
         let deadline = tokio::time::Instant::now() + Duration::from_secs(60);
         loop {
-            let left = deadline.saturating_duration_since(tokio::time::Instant::now());
-            match tokio::time::timeout(left, rx.recv()).await {
+            let next = match rx.try_recv() {
+                Ok(ev) => Ok(Ok(ev)),
+                Err(tokio::sync::broadcast::error::TryRecvError::Lagged(n)) => {
+                    Ok(Err(tokio::sync::broadcast::error::RecvError::Lagged(n)))
+                }
+                Err(_) => {
+                    waited_for_a_frame = true;
+                    let left = deadline.saturating_duration_since(tokio::time::Instant::now());
+                    tokio::time::timeout(left, rx.recv()).await
+                }
+            };
+            match next {
                 Ok(Ok(ev)) => {
                     // Exactly `quilltap-web/src/events.rs`'s `data:` bytes, less
                     // the scope tag (v4's stream body carries none).
@@ -417,6 +436,10 @@ fn run_v5(fx: &Fixture, round_id: &str, plan: Plan) -> Ran {
                 Ok(Err(_)) | Err(_) => panic!("no chainComplete; frames so far: {frames:#?}"),
             }
         }
+        assert!(
+            !waited_for_a_frame,
+            "the run's reply settled before its round finished — the frames after it would go unread"
+        );
         // The round's writes land before its `chainComplete`; the
         // housekeeping enqueues settle.
         tokio::time::sleep(Duration::from_millis(300)).await;
diff --git a/crates/quilltap-host/src/storyteller_spine.rs b/crates/quilltap-host/src/storyteller_spine.rs
index da8952ae3..cd053d839 100644
--- a/crates/quilltap-host/src/storyteller_spine.rs
+++ b/crates/quilltap-host/src/storyteller_spine.rs
@@ -890,7 +890,11 @@ where
         Box::pin(async move {
             // The plan answers its refusals BEFORE any frame (v4 plans before
             // `openTurnStream`); the round then runs on its own thread, the
-            // send's bridge, and the reply is EMPTY (R-8).
+            // send's bridge, and the reply is EMPTY (R-8). Like the send's, the
+            // reply waits for the round: v4's 200 response IS the round's
+            // stream, read to its close (`runStorytellerSelection`), and the
+            // Salon folds a run's frames only until its dispatch settles — an
+            // early reply left every "all at once" bubble unrendered.
             let (tx, rx) = tokio::sync::oneshot::channel();
             std::thread::spawn(move || {
                 let Ok(rt) = tokio::runtime::Builder::new_current_thread()
@@ -921,7 +925,6 @@ where
                             return;
                         }
                     };
-                    let _ = tx.send(Ok(()));
                     if let Err(e) = state.run_planned_round(&req, round, turns).await {
                         emit_send_error_frame(
                             &state.events,
@@ -930,6 +933,9 @@ where
                             orchestrator::SubmissionLedger::default(),
                         );
                     }
+                    // A failure past the plan is the stream's error frame (v4's
+                    // stream errors after its 200); the reply stays EMPTY.
+                    let _ = tx.send(Ok(()));
                 });
             });
             rx.await.unwrap_or_else(|_| {
```

---

## B. Storyteller / spine server (`phase-4.md:7467-7479`, every bullet)

### B1: P4.D343 items 32 / 43 — `storyteller_round_tier3`   [OPEN, L]

- Measured: no `storyteller_round_tier3*` under `crates/quilltap-harness/tests/` or `harness/oracle/cases/`. What exists: `storyteller_pure`, `storyteller_rounds_tier2`, `plan_storyteller_run`, `storyteller_mode_ensure`, and P4.D368's `storyteller_simultaneous_tier3_equivalence` (3 rows over v4's REAL `handleStorytellerRun`, the all-at-once path only).
- Still uncompared against v4: the SEND that opens a round (`open_round_on_send`, `storyteller_spine.rs:~445-535` vs v4 `orchestrator.service.ts:255-290` `openRoundOnSend`) — frame order `storytellerRound` → `storytellerBid`* → `roundOpen` → `chainComplete round_open`; the two `STORYTELLER_BID` `llm_logs` rows; **the R-1 table-delta proof** (v4's `runDry` proxy discards every write but the log; v5's `DRY_SCOPE_BY_GATES` rests on a gate LIST, module doc `:23-37`); R-5's aggregates-unchanged assert; the SEQUENTIAL run's depths + the plan section in the canned key; keepOrder / destroy-alone / ask / lie / leave / supersede; `toolChoice` in the canned key; item 43's Courier bid row.
- Done looks like: a new family in the `storyteller_simultaneous_tier3` rig's shape (seat-keyed real `ChatSpine` vs v4; memory `concurrent-turns-and-real-spine-tier3`) with a before/after table delta per bid on BOTH sides.
- Trap: an op-scoped canned recorder is needed for the detached late bid; the board timeout is env-shortenable (`QT_STORYTELLER_BOARD_TIMEOUT_MS`). Own fixture — do NOT grow `orchestrator-tier3.json` (§F).
- Recommendation: its own lane, harness-only, AFTER lane 1 lands (it would otherwise pin the early reply). If the round is tight, this is the first thing to leave out — but R-1 is the one Storyteller claim with no measured delta.

### B2: P4.D343 item 22 (+2 `message_finalizer_tier3` rows), item 23 (+1 `regenerate_swipe_tier3` row)   [DEFER-TO-CATCH-UP]

- Measured: `message_finalizer_tier3_equivalence.rs` mentions storyteller once (`:943 storyteller: None`); `regenerate_swipe_tier3_equivalence.rs` not at all.
- v4 `b941280d5` (Turns mode) adds 34 lines to `message-finalizer.service.ts` on exactly the turn-mode branch these rows would pin, and both corpora are regenerated at the catch-up. Grow them there.

### B3: P4.D343 item 29 — `.qtap` import refusals   [CLOSED]

- P4.D368 landed it whole (order header `p4.d368-…md` Status; `phase-4.md` status line "CLOSED in the `624d0d25c` round").

### B4: P4.D343 item 10 — the `llm_logs_routes` row   [OPEN, S]

- Measured: `llm_logs_routes_equivalence.rs` and `harness/oracle/cases/llm-logs-routes.test.ts` carry no storyteller row. v4: the `storyteller` filter value + `storytellerRoundId` on the row DTO. Done: one row planting a `STORYTELLER_BID` log with a round id, read through the list route with the filter. Regenerate from the pin.

### B5: P4.D343 item 5 — the roster row through `build_tools`   [OPEN, S]

- v4's jest env resolves no roster, so there is no oracle row to add. The v5-side home exists: `crates/quilltap-harness/tests/pascal_build_tools_roster.rs` (no oracle; drives the committed `pascal-run-custom` pair) carries 0 mentions of `custom_tool_effects`. Done: assert `BuiltTools.custom_tool_effects` for the fixture roster and the `run_custom` note under a `read` ceiling there. Both constituents are already v4-differential (`tool_effects_equivalence`).

### B6: S7 — fold `run_send`'s deps assembly onto `storyteller_spine`   [DEFER-TO-CATCH-UP]

- Judged risky at two unifications (self-referential borrows). v4 `348172b9c` (the opening on the Salon spine) and `b941280d5` (Turns mode's chain) both rewrite `run_send`'s region; a fold now is redone then. The live cost of NOT folding is the class in D1 / B8 (a seam wired on one copy and not the other) — lane 2 must touch BOTH copies for every wire it adds, and say so.

### B7: X1's remaining sub-call sites, Brahma-in-Carina, the one-shot loop, the enclave cheap executor   [OPEN, M — one "LogContext threading" lane]

v4 stamps both ids from `AsyncLocalStorage` inside `logLLMCall` (`lib/services/llm-logging.service.ts:224` `autonomousRunId: getAutonomousRunId()`; the storyteller twin beside it), so EVERY call inside the scope is stamped. v5 threads `LogContext` by hand; these sites still pass `LogContext::none()`:

| v5 site | Reached inside | v4 |
|---|---|---|
| `services/dangerous_content/gatekeeper.rs:531` | a storyteller speaking turn; an autonomous turn | stamped |
| `image_gen/image_attempt.rs:330` | an in-turn image attempt (see B8 — unreachable in-chat today) | stamped |
| `services/brahma_console/orchestrator.rs:1087` and the one-shot loop's callers on the Carina path | Carina's in-turn `@Brahma:` inside a round | stamped |
| `quilltap-host/src/spine.rs:2193-2199` (the enclave turn's `executor`) | `runWithAutonomousRunId` (v4 `autonomous-room-turn.ts:623`) | stamped `autonomousRunId` |
| `spine.rs:2200-2206` (`fold_executor`) | OUTSIDE the scope (v4 `:841-851`) | **correctly none — leave** |

- Done: each site takes the turn's `LogContext`; a row test per site in the shape of the two X1 tests (`carina_query.rs:1776-1830`). A source census over `LogContext::none()` in `services/` + `host/` with an allow-list (the generators, the help chat, the fold executor) would keep it closed.
- Trap: since bug 262 every cheap call writes a row, so the enclave stamp changes what an autonomous run's rows sum to — check v4's budget read before asserting counts.
- Files: `gatekeeper.rs`, `image_attempt.rs`, `brahma_console/orchestrator.rs`, `carina_query.rs` (meets lane 3), `spine.rs` (meets lane 2), `storyteller_spine.rs` (meets lane 1 — only if the Carina-in-round context is assembled there).

### B8: in-chat image generation is unwired in production   [OPEN, M — measured PRE-EXISTING]

- Measured: `ToolExecutor::with_image_generation` (`tools/executor.rs:657`) has NO caller in any crate. `git log -S'with_image_generation'` over `executor.rs` + `spine.rs` returns one commit, `4207ce2a6` (W4.9a, the tool's port) — it was never wired. Both runners omit it: core `build_turn_tool_runner` (`services/orchestrator.rs:202-250`, the in-chat turn) and host `ChatSpine::tool_runner()` (`spine.rs:1232-1265` — Run Tool, Carina, `ask_carina`, Brahma). So `generate_image` reaches `ErasedImageGeneration::none()` (`executor.rs:515`, `:2033-2036`) and answers v4's guard sentence `Image generation is not enabled for this chat` for every chat. (Dogfood #99's wire, recorded 2026-08-21, shows exactly that sentence.)
- The runner exists: `HostImageGenerationRunner` → `ErasedImageGeneration::new(...)` at `spine.rs:3710-3765`, `:4217`, today handed only to `EngineAssembly.image_generation` (`host.rs:838`, the Generate Image dialog).
- v4: the in-chat tool executes for real whenever the chat resolves an image profile.
- Done: an `image_generation: Option<ErasedImageGeneration>` on `OrchestratorDeps` → `build_turn_tool_runner` (+ its unit pin, the `// (the P4.49 lesson)` test beside it) and on `ChatSpine` → `tool_runner()`; BOTH deps assemblies (`run_send` and `storyteller_spine::process_storyteller`); a host wiring test like `d6202b2d3`'s embedding one. ⚠ LIVE spend once wired (an image call per tool use) — say so in the order and the changelog. ⚠ `lantern` census counts `image_transcoder` literals (`3c5ea1872`).
- Needs a ruling only on WHEN (§G R-2): it is a real v5 defect at the pin, not drift.

### B9: X2 — the `[TurnFairness]` pair   [OPEN, S]

- v4 `lib/services/chat-message/turn-holds.service.ts:158` DEBUG `[TurnFairness] Evaluated first-responder rotation after user post {chatId, posterParticipantId, userDrivenSeatCount, nextSpeakerId, nextReason, willPause}` and `:203` INFO `[TurnFairness] User post pauses for another user-driven seat {chatId, posterParticipantId, nextSpeakerId}`.
- v5: no `TurnFairness` string anywhere under `crates/*/src`. The logic is at `services/orchestrator.rs:~946-1010` (`user_driven_seat_count` at `:946`, the `< 2` return at `:958`).
- Done: both lines with v4's field order, capture-pinned by `orchestrator_tier3`'s held cases (`:3250-3380` already capture the paused-hold line; `[TurnFairness] ` joins the prefix list at `:3026`). Regenerate the family from the pin. Order: DEBUG before `recordOperatorPost`, INFO after the `chainComplete` emit.

### B10: X3 — an error after `done` is logged as a failure   [OPEN, S]

- v5: three loops rank the error above `saw_done`: `services/agent_loop/one_shot_loop.rs:478-496`, `services/brahma_console/orchestrator.rs:1067-1075`, `services/help_chat/orchestrator.rs:1489-1497`. v4's funnel (`streaming.service.ts`, `eb2d87788`) writes its ONE row at the terminal chunk and its `finally` writes only when none was written. Carina's `run_stream` already has v4's shape (`carina_query.rs:1425-1450`, `outcome.is_none()`).
- Done: `saw_done` first in all three; one unit row each (a canned stream: done chunk, then `Err`) asserting ONE row with no `error`. Only the LOG row differs — the thrown error still reaches the consumer on both sides.

### B11: X4 — the stale comment at `spine.rs:1788`   [OPEN, S — fold into whichever lane owns `spine.rs`]

- The lines moved; the neighbourhood is now `spine.rs:1778-1796` (the storyteller hold return and `storyteller_single_turn`, citing v4 `:182-184`). The note never recorded WHICH sentence is stale; the lane re-reads that block against v4 `turn-orchestrator.service.ts` at the pin and fixes or strikes the note.

### B12: P4.D347 F-2 — `to_sys_char` carries no pronouns   [OPEN, M — and WIDER than recorded; measure first]

- Measured: production `services/orchestrator.rs:5726-5737` builds the prompt character from SEVEN fields (name, title, identity, description, manifesto, personality, aliases) and `..Default::default()`. `system_prompt::Character` (`system_prompt.rs:142-163`) has five more that `build_identity_stack` reads: `pronouns` (`:371`), `physical_description` (`:382`), `example_dialogues` (`:416`), `scenarios`, `system_prompts`, plus `can_draw_diagrams` (`:683`, the diagram note).
- And the turn never takes the cached stack: `orchestrator.rs:5466 precompiled_identity_stack: None` — "the standing P4.D103/P4.D163 deferral" (`services/system_prompt_compiler.rs:237-241`). v4 `lib/chat/context/turn-system-prompt.ts:56-85` reads `chats.compiledIdentityStacks[participantId]` and falls back to a rebuild from the FULL character (`system-prompt-builder.ts:408-411`).
- So, by reading: a production turn's identity stack is rebuilt from a seven-field character — no `## Character Pronouns`, no `## Physical Appearance` (nor the non-humanoid line P4.D349 ported), no `## Example Dialogue Style`, no character system prompt selected by id, no diagram note. **UNMEASURED at runtime** — the first step of the order is to print one production turn's system prompt for a character that has all five and diff it against v4's for the same chat.
- A complete projector already exists: `system_prompt_compiler::character_from_value` (`:59-110`, pronouns / physicalDescription incl. `humanoid` / scenarios / systemPrompts). Done (single source of truth): `to_sys_char` delegates to it (and the twin at `tools/self_inventory.rs:1436` likewise); `can_draw_diagrams` read from the hydrated row (ties to the other area's V14, `CHARACTER_BOOLEAN_COLUMNS` lacks `canDrawDiagrams`).
- Differential: `orchestrator_tier3` cannot see it (its corpus characters carry none of the fields — the comment at `:5722-5725` says so). Needs ONE new corpus character with the five fields, or a `build_context_equivalence` row; either way the canned keys of every row using that character move.
- NEEDS-RULING §G R-1: land the projector fix now (recommended) and separately decide the `precompiledIdentityStack` deferral.
- Past-pin: `1dc11e947` (character repository / `CharacterRowSchema`) changes how the row is READ, not this projection — not a deferral reason.

### B13: P4.D347 item 24 — the `absent-mentions` REST arm   [NO-ACTION]

- v5 serves the dispatch (`Request::ChatAbsentMentions`) and lists the key (`quilltap-web/src/wardrobe_routes.rs:778`); the chat GET over REST serves only `get-background` + `cost` and answers the loud 400 pointer for every other action — a RECORDED divergence pinned both ways in `query_param_semantics_equivalence`. One more REST arm would be the only one of seventeen. Leave with the class.

### B14: X5 (the referent ledger quoting a whisper to a non-recipient), the NULL `triggerMessageId` round   [NO-ACTION here — v4 filing candidates]

- Both v4-shared; drafts only, the human's go-ahead first. Not lane work.

---

## C. Carina — P4.165's FINDING   [OPEN, S–M]

- v5 `crates/quilltap-core/src/services/carina_query.rs:1362-1458` `run_stream` returns `(String, Option<Value>)`. Its `Err(e)` arm (`:1440-1451`) logs the failed leg and **`break`s**: the partial answer is returned as if the stream had ended. The three callers (v4 `:695` initial, `:735` tool loop, `:756` forced-text) carry on — post the partial text, or answer `empty response`.
- v4 `lib/services/carina/carina.service.ts`: `runStream`'s `for await` throws out of the guarded region into `runCarinaQuery`'s catch — `:841` ERROR `[Carina] Query failed {context, chatId, characterName, error}` and `{ok: false, error: {kind: 'llm-failed', detail: <message>}}`; NOTHING is posted, Prospero's card names the error. v4's own tests pin it (`__tests__/carina.service.test.ts:905`, `:919`).
- Done: `run_stream` → `Result<(String, Option<Value>), String>`; the three call sites `?` into one `llm_failed(detail)` + the ERROR line (v4's field order; `errorJson` shape per §R.5); the leg's `llm_logs` row unchanged (already written before the break).
- **Trap — the stop.** P4.165 made the turn's Stop reach this stream (`watch_stream_with_stop`, `:1413-1423`). In v4 a stopped consult ends QUIETLY (the partial Brahma bubble posts). The lane must prove a stop still yields `Ok(partial)` and only a real provider / stall-watchdog error yields `Err` — `a_stop_mid_stream_keeps_the_partial…` is the guard (and its 50 ms sleep is a recorded NOTE: make it a `Notify` in the same change).
- Differential: `carina_query_tier3_equivalence` + `harness/oracle/cases` twin, with a scripted-error corpus arm (the P4.79 shape: a canned stream that yields one chunk, then throws). Regenerate from the pin.
- Riders from the same header (all S, same file): the v5-only DEBUG `[Carina] Standing instructions applied to query prompt` (`:477`) missing from the N-1 census; the two `error = %…` fields (`Brahma query failed`, `Failed to enqueue memory extraction`, `:859`) not on the `errorJson` shape.
- P4.165 Tier 3 item 10 (`user_message_carina` stays deferred) — NO-ACTION, recorded.

---

## D. Pascal (`phase-4.md:7480-7485`, every bullet)

### D1: P4.D345 Tier 2 item 15 + P4.169 Tier 2 item 5 — the widened `pascal-run-custom` pair and its tier-3 rows   [OPEN, L — one harness lane]

- Measured: `pascal_run_custom_handler_equivalence.rs:8` and `pascal_trigger_runner_equivalence.rs:8` both open with "⚠ DEFERRED — P4.D345 Tier 2 item 15, UNCOMPARED against v4". No `pascal-run-custom-intervention` fixture or extender under `harness/oracle/fixtures/` (the pair has `-announcer`, `-inventory`, `-triggers` extenders only).
- Missing rows: `held-deal-changed-roll`, `held-as-rolled`, `held-deal-nothing-changed`, the three veto callers, `held-stop-vetoes`, `trigger-held-veto-continues`; `pascalMeta.intervention` through a real `chat_messages` row is not compared with v4 anywhere.
- P4.169 item 5 rides the SAME builder (`build-pascal-run-custom-fixture.ts`): the `regalia` wear / take-off tool and the `history_dresser` definition (P4.D306 item 18).
- Done: ONE builder change (both additions), ONE pair rebuild, the scripted-resolver rows in both families, the route family re-run over the widened pair. Traps: memories `rebuilt-fixture-pair-vintage` (run `runV4Migrations` in the builder), `shared-fixture-rebuilds-and-jest-setup-mocks` (minted ids — mask), `pascal-run-custom pair can't boot the Salon`; every family on the pair re-runs (`pascal_build_tools_roster`, `pascal_run_custom*`, `pascal_trigger_runner`, `pascal_custom_tools_execution`, `pascal_workbench_route`).
- B5's roster assert belongs in this lane (same pair).

### D2: P8 — the re-ask settle race (409 where v4 answers 200)   [OPEN, S — low value]
### D3: P9 — a spurious WARN on the Stop-vs-resolve race   [OPEN, S — low value]

- Both live in `crates/quilltap-core/src/pascal/intervention/registry.rs` (the settled-id memory at `:39`, `AlreadyResolved` → 409 at `:143`). Neither has a recorded reproduction beyond the review note; each needs a two-thread unit test first. One small lane with D4's note, or left out (§H).

### D4: P10 — the hold registry is process-global, keyed by chat id   [NO-ACTION]

- `registry.rs:188 static REGISTRY: OnceLock<Mutex<RegistryState>>`. v4's registry is a module-level singleton too, and one process serves one instance in every shipped host (web, Tauri, CLI). Chat ids are UUIDs. Record, do not change; revisit only if a host ever serves two instances in one process.

### D5: P17 SUSPECTED — the transparency fallback's log line   [OPEN, S — measurement first]

- An uncompared arm (`intervention/veto_result.rs:19 veto_result_for(transparent)` and its caller's fallback read of the chat's transparency). Needs one row in `pascal_intervention_route_equivalence` with the transparency read failing; closes as CLOSED or a one-line fix. Ride D1.

### D6: `salon-custom-tools-flow:46`'s heading wait   [OPEN, S — e2e only]

- `apps/web/e2e/salon-custom-tools-flow.spec.ts:44-50` `maybeUnlock` waits on `getByRole('heading', { name: 'Chats', exact: true })`. Owner: whichever lane owns that spec; otherwise the SPA / e2e area's.

### D7: P4.D379   [CLOSED]

- Header: "OPEN remainder: none"; H3 and the two regens were discharged at unification (`4c3d39865`, `62760b85e`, `627ccaee7`).

---

## E. P4.D366 / P4.D368 residue

- **P4.D366 item 19's core capture unit test** (`[TurnYield] Character yielded the floor to the raised hand {… inferredFromGesture}` + the carry-on silence leg) — OPEN, S. The tier-3 half landed on the union (`orchestrator-tier3.json` carries `interrupt_yield_gesture` / `_bare`; the family compares `[TurnYield] ` lines, `:3026`). Site: `services/orchestrator.rs:3920-3930`.
- **P4.D366 Tier 2 item 20 `interrupt_gesture_tools_only`** — OPEN, S–M, absent from the corpus. P4.D370 (the tool rows' canned keys) has landed, so its blocker is gone. Grows `orchestrator-tier3.json` (§F).
- **P4.D368** — CLOSED but for (K) = §A.
- **P4.D348's bug-262 / S1 / S2 work** — CLOSED; its OPEN notes are B7–B11.

---

## F. The harness's frozen-`ProcessClock` asymmetry   [DEFER-TO-CATCH-UP; record the rule now]

- **Where.** `crates/quilltap-harness/tests/orchestrator_tier3_equivalence.rs` builds `ProcessClock { now_ms: spec.frozen_now_ms, … }` for the initial turn (`:2391-2396`) and each chained turn (`:2468-2474`). In core, `ProcessClock.now_ms` (`services/orchestrator.rs:436-446`) stamps the operator's posted USER row and buildContext's timestamps, but the reply and the other minted stamps read the WALL clock: `message_finalizer.rs:1371`, `:1429`, `:1460`; `orchestrator.rs:3263`, `:4568`, `:4877`; and every tool executor (`crate::clock::now_iso()`).
- **v4's side.** The oracle replaces `Date` with one that starts at `frozenNowMs` and ADVANCES 1 ms per read (`harness/oracle/cases/orchestrator-tier3.test.ts:1002-1030`), so its rows are monotonic and all near the frozen instant.
- **The asymmetry.** On v5 a posted USER row is stamped at the frozen instant (the past) and its reply at real now. Within ONE call that orders correctly. Across TWO calls on the same chat the second USER row sorts OLDER than the first reply, and two USER rows tie exactly. Production is unaffected (there `now_ms` IS the wall clock).
- **Size today (measured in the file):** the workarounds are (1) `59ca6cadd` — `deus_send_seat_line` and `p4d370_assistant_agent_on` each moved to their own chat, `deus_send_on_paused_chat` joined `held_cases` (`:3267`); (2) `blank_current_time` + its three regexes (`:486-520`), the render-bearing `contentLength` placeholder (`:712-722`), `CLOCK_TAIL_HASH`; (3) a `wardrobe_history` probe declared unusable (`:4124-4131`). The trap is silent: a lane row "passes" only because a sibling's earlier canned miss hid the prior turn, then goes red on the union.
- **Fix options.** (a) core seam, M: the finalizer and the orchestrator's minted stamps take a ticking clock from `ProcessClock` (production: wall; harness: `frozen + n`), retiring the masks — touches `message_finalizer.rs` + `orchestrator.rs`, exactly the files `b941280d5` rewrites on v4's side. (b) harness-only, S: a guard that refuses a corpus where one chat carries more than one posting call (make the rule mechanical).
- **Recommendation:** (b) now if a harness lane has room (it is twenty lines in the family's own non-vacuity block and stops the union-only reds); (a) with or after the Turns-mode catch-up. Do not do (a) in this round.

---

## G. Rulings needed

- **R-1 — B12's scope.** The seven-field `to_sys_char` is recorded as "pronouns" but by reading it drops five prompt sections on every production turn. Options: (A) fix the projector now (`to_sys_char` → `character_from_value`, + `canDrawDiagrams`), keep `precompiled_identity_stack: None`; (B) also lift the P4.D103 / P4.D163 turn-time deferral (read `chats.compiledIdentityStacks`; the tier-3 oracles stub v4's reader to null and would need un-stubbing); (C) leave both to the catch-up. **Recommend (A)** after the lane's first step confirms it on one real prompt; (B) is its own order.
- **R-2 — B8, wire in-chat image generation now?** It is a three-month-old v5 defect, not drift, and wiring it turns on LIVE image spend in chats that have an image profile. Options: wire now (recommended — v4 parity, small), or hold for the dogfood pass. If wired, the dogfood queue gains a real `generate_image` turn.
- (Carried, not this area's to rule: the two at `phase-4.md:7230-7233`.)

---

## H. Proposed lanes (edited files disjoint unless a meeting point is named)

| # | Lane | Items | Owns | Size |
|---|---|---|---|---|
| 1 | **Storyteller live run fold** (FIRST) | §A | `quilltap-host/src/storyteller_spine.rs`; `storyteller_simultaneous_tier3_equivalence.rs`; `apps/web/e2e/salon-storyteller-flow.spec.ts`; `salon-conversation.ts` `:4381-4435` + its spec cases; `chat-stream.reducer.ts` if proven | M |
| 2 | **Spine wires + prompt character** | B8, B12 (R-1 (A)), B9, B11 | `services/orchestrator.rs` (`build_turn_tool_runner`, `OrchestratorDeps`, `to_sys_char`, the fairness lines); `quilltap-host/src/spine.rs` (`tool_runner`, `run_send`'s deps); `tools/self_inventory.rs:1436`; `orchestrator-tier3.json` + its family (regen from the pin) | M–L |
| 3 | **Carina stream error** | §C + riders | `services/carina_query.rs`; `carina_query_tier3_equivalence.rs` + its oracle case | S–M |
| 4 | **LLM-log rows: error-after-done + LogContext threading** | B10, B7 | `agent_loop/one_shot_loop.rs`; `brahma_console/orchestrator.rs`; `help_chat/orchestrator.rs`; `dangerous_content/gatekeeper.rs`; `image_gen/image_attempt.rs` | M |
| 5 | **Pascal harness: the widened pair** | D1, D5, B5, (D2 / D3 if taken) | `harness/oracle/fixtures/build-pascal-run-custom-fixture.ts` + a new intervention extender; the committed `pascal-run-custom` pair; `pascal_run_custom_handler_equivalence.rs`, `pascal_trigger_runner_equivalence.rs`, `pascal_intervention_route_equivalence.rs`, `pascal_build_tools_roster.rs`; `pascal/intervention/registry.rs` only for D2 / D3 | L |
| 6 | **Storyteller tier-3 family** (after lane 1; optional) | B1, B4 | NEW `storyteller_round_tier3_equivalence.rs` + oracle case + own fixture; `llm_logs_routes_equivalence.rs` + `llm-logs-routes.test.ts` | L |

Meeting points:

- **`quilltap-host/src/spine.rs`** — lane 2 (deps + `tool_runner`) and lane 4's enclave executor (`:2193-2199`). Give the enclave line to lane 2, or serialize 4 after 2. Other areas' lanes that add a host seam also land here.
- **`storyteller_spine.rs`** — lane 1 owns it; lane 2's B8 must add the image-generation wire to `process_storyteller`'s deps copy too → lane 2 AFTER lane 1, or lane 1 takes that one line as a named HANDOFF.
- **`services/orchestrator.rs`** — lane 2 only in this area; P4.D366 item 19's unit test (§E) is in the same file → lane 2. Any other area's context-builder lane meets it.
- **`harness/oracle/fixtures/orchestrator-tier3.json`** — ONE owner per round: lane 2 (B9's capture, B12's new character, §E item 20, §F (b)'s guard). Lane 3 must NOT add its error arm here — it uses the Carina family.
- **`carina_query.rs`** — lane 3 owns; lane 4's Brahma-in-Carina `LogContext` thread passes through it → hand that one site to lane 3.
- **`salon-conversation.ts`** — lane 1 owns only the P4.D344 region; every SPA lane in the round touches this file. Fence by region.
- **`pascal-run-custom` pair** — lane 5 owns the rebuild; no other lane may regenerate a family on it mid-round.

Must land in one change: lane 1's three halves (server hold + SPA + e2e); B8's two runners + both deps copies; D1's two builder additions (ONE rebuild).

---

## I. Recommended OUT of this round

- **B2** (finalizer / regenerate storyteller rows) and **§F (a)** (the clock seam) — Turns mode (`b941280d5`) rewrites the finalizer and the chain; both corpora regenerate at the catch-up.
- **B6 / S7** (the deps fold) — `348172b9c` + `b941280d5` rewrite `run_send`'s region; judged risky twice.
- **B13** (the REST arm) — rides the recorded REST-GET divergence.
- **D4 / P10** — v4-shaped; no host serves two instances in a process.
- **D2 / D3** (P8 / P9) unless lane 5 has room — races with no reproduction and no operator-visible effect recorded.
- **Lane 6** if the round is tight — but then record again, by name, that `DRY_SCOPE_BY_GATES` has no measured delta.
- **B14** and the other v4 filing candidates — drafts for the human, not lanes.
- The lazy-id-in-`queryFn` audit — the documents / SPA area's.

---

## J. Per-order index

| Order | Item | Class | § |
|---|---|---|---|
| P4.D343 | (K) / the early run reply | OPEN | A |
| P4.D343 | items 32 / 43 tier-3 | OPEN | B1 |
| P4.D343 | items 22 / 23 rows | DEFER | B2 |
| P4.D343 | item 29 import refusals | CLOSED (P4.D368) | B3 |
| P4.D343 | item 41 simultaneous | CLOSED (P4.D368) | — |
| P4.D343 | item 10 routes row | OPEN | B4 |
| P4.D343 | item 5 roster row | OPEN | B5 |
| P4.D343 | S7 deps fold | DEFER | B6 |
| P4.D343 | X1 gatekeeper / image attempt | OPEN | B7 |
| P4.D343 | NULL `triggerMessageId` | NO-ACTION (filing) | B14 |
| P4.D345 | Tier 2 item 15 | OPEN | D1 |
| P4.D345 | P8, P9 | OPEN (low) | D2, D3 |
| P4.D345 | P10 | NO-ACTION | D4 |
| P4.D345 | P17 | OPEN (measure) | D5 |
| P4.D345 | `salon-custom-tools-flow:46` | OPEN | D6 |
| P4.D347 | F-2 `to_sys_char` | OPEN + NEEDS-RULING R-1 | B12 |
| P4.D347 | item 24 REST arm | NO-ACTION | B13 |
| P4.D347 | X5 | NO-ACTION (filing) | B14 |
| P4.D348 | X2 `[TurnFairness]` | OPEN | B9 |
| P4.D348 | X3 error-after-done | OPEN | B10 |
| P4.D348 | X4 stale comment | OPEN | B11 |
| P4.D348 | Brahma-in-Carina / one-shot / enclave `LogContext` | OPEN | B7 |
| P4.D348 | in-chat image generation unwired | OPEN + NEEDS-RULING R-2 | B8 |
| P4.D366 | item 19 core capture test | OPEN | E |
| P4.D366 | item 20 `interrupt_gesture_tools_only` | OPEN | E |
| P4.D368 | all lane items | CLOSED | E |
| P4.D379 | all | CLOSED | D7 |
| P4.165 | Carina FINDING | OPEN | C |
| P4.165 | three §3 NOTEs | OPEN (riders) | C |
| P4.165 | Tier 3 item 10 | NO-ACTION | C |
| P4.169 | Tier 2 item 5 (`regalia` / `history_dresser`) | OPEN | D1 |
| P4.169 | Tier 3 item 7 (not owned) | NO-ACTION here | — |
| — | frozen-`ProcessClock` asymmetry | DEFER (+ a harness guard now) | F |
