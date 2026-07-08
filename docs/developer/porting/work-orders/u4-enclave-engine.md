# Work order: Unit 4 — the enclave engine

> **COMPLETED 2026-07-08 (Round 5, branch `u4-enclave`).** Every deliverable
> landed except two consciously superseded: the "main-primary `ApplyHost`
> trace assertion" (decision #3 was revised at U4.4 to direct writes —
> `write_apply_equivalence` re-verified instead; see `enclave-engine.md`)
> and the orchestrator autonomous spine case (the capstone drives v4's REAL
> `handleSendMessage` with the autonomous flags inside
> `handleAutonomousRoomTurn`, so a duplicate orchestrator-corpus case adds
> nothing — the packet's own escape hatch). `suppress_automatic_images`
> turned out to have NO consumer in v4 (nothing to plumb); the
> `autonomous_context_cap` context-manager clamp turned out to be UNPLUMBED
> in v5 and was closed here.

**Size: one Opus session** (internal parallelization: U4.1 + U4.2 + U4.3 are
disjoint and can be sub-agented; U4.4 serialized after). **Prerequisites
(ALL met as of the W4.11 cleanup round, 2026-07-08):** W4.8 (the job
runner — `step()`'s dispatch row and the re-enqueue live there), W4.6b
(the Host announcement writer), and the W4.7e/e3 + W4.11 logging
composition (`LogContext` with the explicit autonomous-run-id field;
`with_logging` live at the spine composition point; `llm_logs` rows
byte-verified through real call sites — the token-accounting substrate).
The full decomposition, architecture decisions, and verification plan are
in [`../enclave-engine.md`](../enclave-engine.md) — **read it first; it is
the spec.** This order adds only the execution constraints. **Oracle
baseline: v4 `6b6e39ad`** (verify at session start; drift check first).

## Ground rules

- v4 (`~/source/quilltap-server`, record the oracle SHA) is the oracle;
  every sub-unit ships its differential per the plan in the spec doc.
- New module family: `quilltap-core::enclave` (`budget`, `cron`,
  `lifecycle`, `step`, `announce_text` [generated]). The job-runner dispatch
  rows (`AUTONOMOUS_ROOM_TURN`, `AUTONOMOUS_ROOM_SCHEDULE_TICK`) are added
  in `services::job_runner` — coordinate if W4.8 follow-ups are in flight.
- The turn's writes go through `write_apply` (main-primary) — the spec's
  decision #3. The `ApplyHost` wiring for a live `Db` exists from the W4.8
  work; if it does not, build it here and note it.
- Clock, RNG, and UUID minting injected everywhere (the orchestrator
  precedent); the oracle clock ticks +1 ms/read
  (`[[chain-depth-frozen-clock-artifact]]`).
- Verify the autonomous `chats` columns are all in the ported marshaling
  BEFORE starting U4.3 (they should be; any gap is a drift item to close
  first with its own corpus rows).
- `suppress_automatic_images`: check whether the ported finalizer/spine
  actually consumes it; if it was never plumbed (the corpus kept it
  inert), plumb + bank it here — the enclave is its consumer.
- The run-id `LogContext` threading (spec decision #4): the turn's
  executor is constructed `with_logging(... ctx: <run LogContext>)`, and
  the primary stream's hard-coded `LogContext::none()` in
  `log_chat_message_call` must be parameterized (a `log_context` input
  threaded to `run_primary_stream`, DEFAULTING to none — every existing
  caller and corpus stays untouched; regen only what the enclave
  differential itself dumps). U4.4's token accounting then sums real
  `llm_logs` rows by run-id.

## Sequencing inside the session

1. U4.1 (budget/milestone leaves) ∥ U4.2 (cron) ∥ U4.3 (lifecycle) — three
   sub-agents on disjoint files, each with its differential green before
   integration.
2. U4.4 (`step` + `schedule_tick`) serially, composing 1–3 + the spine.
3. The job-runner dispatch rows + one end-to-end runner test (enqueue tick →
   run start → N turns → budget end) as a v5 integration test on top of the
   differentials.

## Deliverables checklist

- [ ] `enclave::{budget,cron,lifecycle,step}` ported; dispatch rows live;
      announcements composed over the W4.6b Host writer.
- [ ] Differentials green: the U4.1 tier-1, the U4.2 cron corpus (DST rows
      included), the U4.3 lifecycle tier-2, the U4.4 tier-3 capstone (all
      cases in the spec's list), + the main-primary `ApplyHost` trace
      assertion.
- [ ] Regenerated/re-verified: `orchestrator_tier3` (the autonomous options
      now driven by a real consumer — add an autonomous-turn spine case),
      `write_apply_equivalence`, the W4.8 runner self-tests.
- [ ] Full workspace `cargo test` / `clippy -D warnings` / `fmt --check`.
- [ ] CHANGELOG, versions, CLAUDE.md, `enclave-engine.md` status,
      `phase-3.md` unit list; memory note (esp. the cron-library finding).
- [ ] Commit per `.claude/commands/commit.md`, branch `u4-enclave`.

**STOP rules:** no timers/loops in the core (`step()` + host cadence only —
this is the load-bearing design decision of the whole port; see
api-boundary.md Part 3); the turn stays `maxAttempts: 1` + reconciliation
(never automatic retry); schema never changes; if v4's cron library
semantics can't be matched by a crate NOR a bounded hand-roll, STOP and
surface the divergence rather than approximating silently.
