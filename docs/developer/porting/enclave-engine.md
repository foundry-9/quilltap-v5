# The enclave engine — Phase-3 Unit 4 decomposition

> The autonomous-room ("enclave") engine: the last Phase-3 unit. Read with
> [`api-boundary.md`](./api-boundary.md) Part 3 (the `step()` + `RunState`
> decision this implements) and [`chat-orchestration.md`](./chat-orchestration.md)
> (the spine it drives). Scoped 2026-07-06 from a direct survey of the v4
> source. Work order: [`work-orders/u4-enclave-engine.md`](./work-orders/u4-enclave-engine.md).

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
3. **The turn uses the batch write path.** v4's AUTONOMOUS_ROOM_TURN is the
   ONE main-primary job type (main commits authoritatively, mount-index /
   llm-logs best-effort). The ported `write_apply` preserves this; the v5
   turn handler routes its writes through it rather than incremental
   `Db::write` — the one handler where the batch model is correctness, not
   a Node workaround (a duplicated turn message on retry is user-visible).
   `maxAttempts: 1` + the failure reconciliation keep v4's
   no-automatic-retry semantics.
4. **The run-id context** becomes an explicit parameter (no ambient
   AsyncLocalStorage): the cheap-LLM/stream executors gain an optional
   `autonomous_run_id` they thread to `logLLMCall` (a W4.7e composition
   point; until it lands, the token accounting reads `llm_logs` rows the
   differential seeds/mocks — see the work order).
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
  `manual:stopped`, `restart:interrupted`, `turn_failed:{reason}`,
  `start:enqueue_failed`, `schedule:enqueue_failed`).
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
  `runPausedAt = max(lastMessageAt, runStartedAt)`), and
  `reconcile_failed_turn`.
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
