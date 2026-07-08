# The enclave engine — Phase-3 Unit 4 decomposition

> The autonomous-room ("enclave") engine: the last Phase-3 unit. Read with
> [`api-boundary.md`](./api-boundary.md) Part 3 (the `step()` + `RunState`
> decision this implements) and [`chat-orchestration.md`](./chat-orchestration.md)
> (the spine it drives). Scoped 2026-07-06 from a direct survey of the v4
> source. Work order: [`work-orders/u4-enclave-engine.md`](./work-orders/u4-enclave-engine.md).
>
> **STATUS: DONE (2026-07-08, Round 5).** All four sub-units ported and
> differential-green against v4 `6bf88959`. Decision #3 was REVISED at U4.4
> (direct writes — see below). Three faithful-port findings the port pins
> broken-but-exact / as-dead-code: (a) v4's `getTotalTokenUsageSince`
> carries the `$ne: null` translator bug — on SQLite the daily-spend sum is
> ALWAYS 0, so the daily-token-budget gates never bind through actual spend
> (the `tokens_user_daily` pause branches are dead code; empirically probed
> and banked in the capstone corpus); (b) the turn handler's `turn_error:`
> transition is unreachable — v4's `handleSendMessage` stream shell
> swallows every error, so a mid-turn failure counts the turn and
> re-enqueues (banked as `stream_error_swallow`); (c) `suppressAutomaticImages`
> has NO consumer anywhere in v4 — declared, set, never read; nothing to
> plumb.

## What v4 has

~2,135 lines of engine (+357 API routes, +668 migrations already covered by
the ported marshaling, +1,656 tests):

- `background-jobs/handlers/autonomous-room-turn.ts` (880) — one turn:
  guards → budget check → speaker selection → `handleSendMessage`
  (`continueMode`, `neverPauseForUser`, `suppressAutomaticImages`,
  `singleTurn`, `autonomousContextCap`) → token/turn accounting → budget
  re-check → milestones → summary fold → re-enqueue.
- `chat-message/autonomous-room.service.ts` (568) — the lifecycle:
  start-manual / pause / resume / stop / update-settings / startup + failure
  reconciliation.
- `autonomous-room-schedule-tick.ts` (235) — the cron tick: due-slot scan,
  freshness window (default 12 h), wedge healing.
- `autonomous-run-start.ts` (154) + `autonomous-room-announce.ts` (146) —
  the shared run-start (row flip → first-turn enqueue → banner) and the
  Host-authored announcements.
- `autonomous-run-context.ts` (39) — the ambient run-id that tags `llm_logs`
  rows (`autonomous_run_id`) so per-run spend is summable.
- `scheduled-autonomous-rooms.ts` (~83) — the 60 s host timer.
- `tools/destructive-tools.ts` (30) — already ported in W4.1g.

State lives entirely on the `chats` row (all already in the ported
marshaling — verify, they postdate some sub-units): `runState`
(`idle|running|paused|stopped|budgetExhausted|error`), `currentRunId`,
`runStateMessage`, `runStartedAt/runEndedAt/runPausedAt/runPausedAccumMs`,
`runTurnsConsumed/runTokensConsumed`, `runMilestonesAnnounced` (bitmask:
1=halfway, 2=near-end, 4=grace), `budgetMaxTurns/MaxTokens/MaxWallClockMs`,
`budgetExcludeCacheHits`, `scheduleCron/scheduleFreshnessWindowMs/
scheduleNextRunAt/scheduleLastRunAt`, `runDestructiveToolsAllowed`,
`runVisibility`.

## How it maps to the v5 design (the decisions)

1. **`step()` is the turn handler.** v4's `handleAutonomousRoomTurn` is
   already shaped as one persisted transition: read state → decide →
   execute one turn → persist counters → decide continuation. v5 ports it
   as `enclave::step(db, chat_id, run_id, now, deps) -> StepOutcome` where
   `StepOutcome` says "re-enqueue / ended(reason) / yielded". **No loop
   inside the core.** The W4.8 job runner's AUTONOMOUS_ROOM_TURN dispatch
   row calls `step` once per claimed job and re-enqueues per the outcome —
   exactly v4's shape, which is why resume-on-open works: an iOS host that
   dies mid-run just has a claimable job / a reconcilable `running` row.
2. **Cadence is the host's.** The 60 s scheduler timer and the cron
   evaluation cadence live in the host driver (W4.8's `tick_scheduler`
   family). The core exposes `enclave::schedule_tick(db, user_id, now)`
   (due-scan + freshness + wedge healing) as a plain function.
3. **The turn writes directly through `Db` — the `write_apply` routing is
   SUPERSEDED (revised at U4.4, 2026-07-08).** The original decision here
   routed the turn's writes through `write_apply` (v4's one main-primary
   job type). That could not survive contact with the ported spine: the
   turn calls `process_message`, which writes incrementally through the
   single-writer `Db` everywhere (the W4.8 encoded decision deliberately
   dropped the job-level all-or-nothing buffer for direct-writing
   handlers — see `[[job-runner-fork-ipc-non-port]]`), and batching the
   whole turn would re-introduce the buffered-proxy model the port
   rejected. Decisive: the v4 ORACLE side runs unforked (no
   `QUILLTAP_JOB_CHILD`), so v4's differential-observable behavior is also
   direct-write — the U4.4 tier-3 capstone pins exactly the in-process
   semantics, and v4's buffered-read workarounds (the local-snapshot
   counter pinning) are still reproduced faithfully where they change
   observable arithmetic. `maxAttempts: 1` + the failure reconciliation
   keep v4's no-automatic-retry semantics; a mid-turn failure is handled
   by `reconcile_failed_turn` (v4's own recovery path), not by write
   atomicity. `write_apply` keeps its own equivalence proof
   (`write_apply_equivalence`, re-verified at U4.4) and remains available
   for a future batch-mode consumer; the work-order deliverable "the
   main-primary `ApplyHost` trace assertion" is superseded accordingly.
4. **The run-id context** becomes an explicit parameter (no ambient
   AsyncLocalStorage). This landed with W4.7e/e3: `LogContext` carries the
   explicit autonomous-run-id field, `CheapLlmTaskExecutor::with_logging`
   takes it via `CheapLlmLogConfig.ctx`, and every request-path caller
   passes `LogContext::none()`. The enclave is the first caller to supply a
   REAL context: the turn handler constructs its executor
   `with_logging(... ctx: <the run's LogContext>)`. **The known gap is
   CLOSED (U4.4):** `log_chat_message_call` is parameterized — a
   `log_context` input threaded through the spine to `run_primary_stream`
   (via `ProcessMessageInput.log_context`), defaulting to none so every
   pre-existing caller/corpus stayed byte-identical (`primary_stream_tier3`
   + `orchestrator_tier3` regenerated + green). Token accounting sums real
   `llm_logs` rows by run-id; the 9c summary fold runs OUTSIDE the run
   scope (v4: `getAutonomousRunId()` null there) so its rows stay untagged
   — banked two-sided in the capstone differential. (The provider-failover
   retry legs still pin none — a pre-existing standing follow-up, noted at
   the site.)
5. **Announcements** ride W4.6b's Host writer (persona + opaque bodies,
   byte-exact); the run-start banner, halfway/near-end/grace nudges, and
   end/paused/error posts are enclave-owned strings in a generated
   submodule.

## Sub-units (leaf-first)

- **U4.1 — the pure budget/milestone leaves** (tier-1):
  `check_budget` (turns/tokens/wall-clock/daily gates; wall-clock =
  `now − parse(runStartedAt) − runPausedAccumMs`; the daily cap PAUSES,
  others END; the grace-turn rule — exhausted without near-end ever fired →
  one grace turn), `compute_budget_progress` (fraction + binding
  `turns|tokens|time|daily`), the milestone threshold/bitmask logic
  (halfway ≥0.5, near-end ≥0.9 sets halfway too, each fires once), and the
  `runStateMessage` string vocabulary (`budget:turns`, `budget:tokens`,
  `budget:time`, `budget:tokens_user_daily`, `manual:paused`,
  `manual:stopped`, `restart:interrupted`, `turn_error:{message}` [the turn
  handler's own catch — distinct from `turn_failed:{reason}`, which is
  `reconcile_failed_turn`'s, UTF-16 `.slice(0,500)`-capped],
  `no_eligible_speaker:{reason}`, `start:enqueue_failed`,
  `schedule:enqueue_failed`).
  (`compute_autonomous_context_cap` is already ported — Phase 1.)
- **U4.2 — cron slot computation** (tier-1): find v4's cron library
  (read `package.json` + the tick source), then decide crate-vs-hand-roll
  by differential: a corpus of (cron expr × from-instant) → next-slot
  rows driven through v4's real evaluation, including DST boundaries in
  the instance timezone (jiff is already in-tree), month-end wraps, and
  the freshness-window skip/advance rule. If no Rust crate matches v4's
  library semantics exactly, hand-roll the subset the corpus pins (the
  bounded-jsonrepair precedent).
- **U4.3 — the lifecycle service** (tier-2, no model): `begin_autonomous_run`
  (the exact row patch — counters zeroed, `runState='running'`, minted
  `currentRunId`; enqueue-first-turn; ROLLBACK to `error` on enqueue
  failure; banner best-effort), `start_manually` (cron-slot consumption
  when within the window), `pause` (idempotent, `runPausedAt` stamp),
  `resume` (paused-same-run → accumulate `runPausedAccumMs`, preserve
  counters/runId; else fall back to fresh start), `stop` (bump
  `currentRunId` so queued turns die on the stale-run guard),
  `update_settings`, `reconcile_at_startup` (`running` → `paused`,
  `runPausedAt = lastMessageAt ?? runStartedAt ?? nowIso` — a nullish-coalesce
  chain, NOT a max; the U4.3 port verified this against the v4 code and
  banked all three branches), and `reconcile_failed_turn`.
- **U4.4 — `step()` + the schedule tick** (tier-3, the capstone): the
  guard chain (missing/non-autonomous → exit; stale-run; the concurrency
  tie-break — earlier `(createdAt, id)` wins; lifecycle gate), the
  idle→running defensive fallback, pre-turn budget check + grace, speaker
  selection over the ported turn manager (`userParticipantId = None`),
  the `process_message` call with the five autonomous options (all already
  plumbed — `never_pause_for_user`, `single_turn`,
  `autonomous_context_cap`; verify `suppress_automatic_images` reaches the
  finalizer gates), post-turn token accounting (`llm_logs` sum by run-id,
  cache-hit exclusion per `budgetExcludeCacheHits`, the one-turn-behind
  floor at the local snapshot — carry the why-comment), re-check + grace +
  milestones (+opaque swap), the run-scope-EXITED summary fold, and
  re-enqueue. Plus `schedule_tick` (due scan, stale-slot skip + advance,
  fresh-slot start, wedge healing: `running` + no in-flight turn +
  >60 s grace → re-enqueue with the LIVE run-id).

## Verification

U4.1/U4.2 are tier-1 against v4's real exports (the budget functions are
exported; the survey notes `@port-oracle-export` markers). U4.3 is a
jest/tsx real-DB tier-2 over the lifecycle matrix (every transition, the
rollback, the resume-accumulation arithmetic with an injected clock),
diffing the `chats` row + `background_jobs`. U4.4 is the tier-3 capstone:
drive v4's REAL `handleAutonomousRoomTurn` under jest (the
`[[jest-real-db-oracle]]` + orchestrator-oracle machinery — model
boundaries canned, clock ticking +1 ms/read, `crypto.randomUUID` pinned,
llm_logs seeded so token accounting is deterministic) over a multi-turn
scripted sequence per case: normal turn + re-enqueue, budget-end by each
of the four gates, the grace turn (both grant and completion), milestones
(incl. the vault-past-90% no-nudge rule), stale-run exit, concurrency
tie-break, wedge heal, daily-cap pause vs end, and error → reconcile.
Diff `chats` + `chat_messages` + `background_jobs` + `llm_logs` per the
established forms. **The enclave differential is also the proof for
`write_apply`'s main-primary path in live composition** — assert the
partition ordering through the recorded `ApplyHost` trace on at least one
case.

## Explicitly deferred

The API routes + the system listing (Phase-4 transports); `runVisibility`
enforcement (a read-side/UI concern — port the column pass-through only);
the memory-extraction fire-and-forget (already ported machinery; the
enqueue is banked by the spine differentials).
