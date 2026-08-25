---
description: Plan the next porting round and write agent-ready work orders (as many parallel lanes as the scope allows)
---

# Set up the next porting round

Plan the next unit of the port and write the work orders that a separate agent
(or several in parallel) can carry out unattended. The output of this command
is **committed work-order documents plus a report telling the human exactly
where each one is** — not implementation.

## 1. Read the drift ledger first

Drift state lives in **`docs/developer/porting/drift-ledger.md`** — do NOT
re-derive it here. Run the ledger's §2 freshness probe (four read-only
commands); if the probe fails, run `/driftcheck` to bring the ledger current
before planning anything.

Then plan FROM the ledger:

- **UNPROCESSED `PORT`/`PORT-NEW`/`CONVERGENCE` rows make the drift
  catch-up the round** (or its first lanes) — the standing rule is that
  drift debt is cleared before new scope. Use each row's intersection
  column (which v5 surfaces/rounds it hits) to shape the lanes instead of
  re-deriving it; the per-order fresh v4 survey in §4 still happens, but
  from the shipped hunks the ledger points at, never from commit prose
  (ledger §5.3).
- **Mark every row you turn into an order as `ORDERED(<order-id>)`** in the
  ledger's §3, in the same commit as the orders.
- Carry the ledger's **regen rule** (§1) into every order's preamble — if a
  pin is required, say so and point at the ledger's §5.1 recipe.

## 2. Determine the scope

Read, in this order:

- The current phase plan under `docs/developer/porting/` (e.g. `phase-4.md`)
  — the locked decisions and the decomposition.
- The **existing work orders** in `docs/developer/porting/work-orders/` —
  an order with an OPEN remainder in its status header beats new scope.
- `docs/developer/porting/status-log.md` (tail) and
  `docs/developer/porting/dogfood-findings.md` — banked deferrals and
  findings that were promoted into "the next order".
- The memory notes (MEMORY.md index) for standing gotchas and carry-forwards.

These five sources are the round-lifecycle handoff: `/unify` keeps the
phase plan, order status headers, status-log, and memory current, and
`/dogfood` keeps the findings file's standing notes and order headers
current — so between them, everything needed to answer "what's next"
should already be here. If it isn't (a stale header, a phase plan that
doesn't reflect main), fix that first and flag the gap.

## 3. Decide the parallel split

Default to as many parallel lanes as the scope honestly supports. A split is
valid only when:

- **Ownership is disjoint.** Each lane owns specific files/crates; no two
  lanes edit the same source file. `CHANGELOG` and `status-log.md` are
  append-only for everyone (they union-merge at unification).
- **Every meeting point is pinned as a Shared contract.** Where lanes must
  agree (e.g. request/response variants between a server lane and an SPA
  lane), write the contract out **verbatim and identically in both orders**
  and mark it binding. Name-level, not vibes-level.
- **Version-bump ownership is assigned** (which lane bumps which crate) so
  the unifier only accumulates.

If the scope is one deep vertical that can't split, say so and write ONE
order — don't manufacture fake parallelism.

## 4. Write each order

Location: `docs/developer/porting/work-orders/<id>-<slug>.md`. Follow the
shape of the existing orders (e.g. `p4.6f-characters-server.md`):

- **Status header** (starts empty/OPEN) — the unifier updates it later.
- **Preamble**: the v4 baseline commit + the drift-ledger regen rule in
  force (pin required or not, per `drift-ledger.md` §1) + the instruction to
  run the ledger's §2 freshness probe at lane start and STOP on a probe
  failure; sibling lanes; worktree note.
- **Mandate**: one paragraph of what the lane delivers, and what it must NOT
  touch.
- **Survey-verified starting points**: do a FRESH v4 survey now (file paths,
  line numbers, route tables, quirks) so the lane doesn't re-derive them —
  and date it.
- **Tiered deliverables**: tier 1 must-land, tier 2 should-land, tier 3
  explicit refusal-arm deferrals (`not_available`-style loud typed refusals,
  never silent stubs).
- **The differential requirement, restated**: every ported unit ships with an
  equivalence test driving v4's REAL code (jest/tsx oracle → NDJSON → Rust
  diff, per the tier). Point at the closest existing recipe (memory notes,
  status-log) rather than inventing one.
- **Fixtures**: which committed fixtures the lane consumes or must deliver,
  and who owns regenerating dependent oracles when a fixture changes.
- **Shared contract + Ownership** sections (binding, identical across
  sibling orders).
- **Verification gate the lane must run before declaring done** (fmt, clippy
  both feature sets, workspace tests, its own differentials, SPA
  tests/build if applicable) and the per-commit rule: follow
  `.claude/commands/commit.md`.

## 5. Commit and report

Commit the orders per `.claude/commands/commit.md` (docs-only: CHANGELOG
entry, no version bumps) and push. Then report to the human:

- One line per order: **the exact file path**, the lane's scope, and its
  sibling/dependency relationships.
- The recommended execution arrangement (which lanes run in parallel, which
  model/agent tier, worktree per lane).
- Anything you deliberately left out of the round and why.
