---
description: Unify finished work-order lane branches into main, run the full gate, and clean up
argument-hint: <work order path(s) whose lanes are finished>
---

# Unify finished lanes into main

Unify the finished lane branch(es) for: **$ARGUMENTS**

## 1. Survey before touching anything

- `git worktree list` + `git branch -a` — find each lane's branch and
  worktree; confirm the worktrees are clean (no uncommitted WIP).
- Confirm v4 (`~/source/quilltap-server`) is still at the oracle baseline;
  if it moved, stop and report before unifying.
- Read each lane's commits AND its status-log/CHANGELOG additions. **Verify
  delivered scope against the order's tier list yourself — do not trust
  "complete".** Anything banked or refusal-armed is a load-bearing fact for
  the final report and the order's status header.
- Note for §3 anything you already doubt: a claim without a differential, a
  "faithful port" you haven't seen the v4 side of, a seam wired to a no-op.

## 2. Reconcile onto a temp branch

- Branch `unify/<round>` from main; cherry-pick each lane **in dependency
  order** (server/core lanes before SPA lanes that consume their contract).
- Expected conflicts and their resolutions:
  - **Version files** (`Cargo.toml`s, `apps/web/package.json`): accumulate
    to the final versions; sync locks (`cargo build` / `npm install
    --package-lock-only`).
  - **`docs/CHANGELOG.md` / `status-log.md`**: both-sides union, newest
    first — never drop a lane's entries.
  - Source-level conflicts should NOT happen if Ownership was respected;
    if one appears, understand it before resolving — it usually means a
    contract drifted.

## 3. Review the code yourself — this is not optional

You are the last reviewer before this reaches main. A green gate proves the
tests that exist pass; it does not prove the code is right, and it never
proves the lane built what the order asked for. **Read the whole combined
diff** (`git diff main...unify/<round>` — every hunk, including files no
conflict touched), and form your own judgment. Delegate breadth to parallel
reviewer agents on a large round if you like, but you own the verdict, and
"an agent said it was fine" is not a verdict.

Judge on two axes:

**Does it honour the contract?** Re-read each order (not just its status
header) and check the code against it, unit by unit. Did the lane build the
thing that was ordered, at the tier ordered, or something adjacent that was
easier? Are the deliverables real — is the differential the one the order
specified, over the fixture it specified, at the right baseline? Does a unit
marked done actually work end-to-end, or only compile? Are the deferrals in
the order's status header the deferrals that are actually in the code —
and does every refusal refuse *loudly and by name*? A silent stub, a
`todo!()`, an arm that returns `Ok(())` where it should refuse, or a
"deferred" item nothing in the code marks are all blocking.

**Is it good code, and is it right?** Specifically for this port:

- **Fidelity to v4.** When the diff claims to port something, open v4's
  real code and compare. Read the port for what a differential cannot see:
  paths the corpus never exercises, error arms, ordering, key order,
  explicit-null vs omitted, the v4 *why*-comments that should have come
  across. A one-mode corpus, a fixture that agrees with itself, and a
  hand-written count that checks v4 rather than the port are the failure
  shapes this project keeps re-learning — look for them by name.
- **The standing rules in CLAUDE.md.** No stubs or `TODO` code; no
  unilateral schema or cipher change; single-writer ownership; the
  transport-agnostic boundary held (no business logic above it); host
  cadence in the host; steampunk register in user-facing strings; the
  spelling.
- **Ordinary engineering.** Encapsulation, SRP, DRY, KISS — duplicated
  logic that should be shared, a helper copied rather than reused, dead
  code, panics on a live path, an unwrap on user input, a seam that
  silently swallows, real spend introduced without it being flagged.
- **Blast radius the lanes could not see.** Lanes are isolated; you are the
  first to see the union. Does one lane's change break another's premise?
  Does a shared file now contain two half-solutions?

**A finding blocks the same way a red gate does.** Fix it on the unify
branch (a commit of its own, called out in the round record), or — if the
right repair is a ruling or a round of its own — stop and escalate rather
than landing it quietly. Either way it goes in the round record and the
§7 report, including findings you fixed: what was wrong, how you caught
it, and what test now keeps it caught.

## 4. The unification wires

Do the cross-lane proof obligations no single lane could do — read each
order's Shared contract section for what these are. Typical: diff the
contract name-for-name across sides; un-skip / restore annotated e2e beats
that were waiting on another lane; extend a walk to cover the seam. The wire
work is a commit of its own.

## 5. The full gate — block on any failure

- `cargo fmt --all --check`; release build; `cargo clippy --workspace
  --all-targets -- -D warnings` both plain AND with
  `--features quilltap-core/native-transport`.
- **Regenerate every affected oracle FRESH from the v4 checkout** (each in
  its own clean invocation; jest cases via a `/tmp` mirror; Node 24), then
  `cargo test --workspace` with all differential env vars set — and re-run
  the new differentials by name so you SEE them pass rather than silently
  skip.
- SPA: `npm test`, `npm run build` (from `apps/web`), then the FULL Playwright
  suite against the fresh build. Fix the port (or the spec's gesture), never the
  assertion. ⚠ **Never invoke `ng` directly** — a bare `ng test`/`ng build`
  finishes and then hangs forever, so anything chained behind it never starts.
  The npm scripts wrap it (`tools/ng-run.mjs`) and exit with the real status.

## 6. Docs, then fast-forward

- CHANGELOG: one unification entry (scope, wires, gate results, final
  versions), in the standard header format from `commit.md` §7 — the H4
  date + subject header and the `_Versions: …_` line, no commit hash. Status-log: the round record. Each order's **status header**:
  move landed items to Done, enumerate exactly what stays OPEN.
- **Keep the phase plan current** (the `phase-*.md` decomposition): mark
  this round's items done and make sure what's next is legible — the next
  `/setupphase` reads the phase plan FIRST, so "what's next" must live
  there and in the order status headers, not only in your chat report.
- Update CLAUDE.md's Status only if this completes a round/phase — keep it
  phase-level.
- `git checkout main && git merge --ff-only unify/<round> && git push`.

## 7. Clean up

- Remove each lane worktree (`git worktree remove`), delete the lane
  branches and the temp branch. Check for agent child worktrees that were
  subsumed. Worktree targets are large (tens of GB) — confirm the space
  came back.
- Remove /tmp oracle artifacts, fixture mirrors, stale `test-results/`, and
  any debug servers.
- Update the round's memory note (what landed, what's OPEN, new gotchas,
  **and what the §3 review caught — the finding shapes are the reusable
  part**) and its MEMORY.md index line.

## 8. Report

Lead with what's on main (commit hash, pushed) and the gate numbers. Then
**what the §3 review found** — every finding, fixed or escalated, with the
one that would have shipped named first; a round where you found nothing
says so explicitly, so the human knows the review ran. Then the caveats
that change what the human does next: anything a lane banked, any surface
still refusal-armed, and the recommended next ask. Everything
in that recommendation must ALSO already be durable in the docs above — a
fresh `/setupphase` session must be able to reconstruct it without this
conversation.
