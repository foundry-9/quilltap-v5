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

## 3. The unification wires

Do the cross-lane proof obligations no single lane could do — read each
order's Shared contract section for what these are. Typical: diff the
contract name-for-name across sides; un-skip / restore annotated e2e beats
that were waiting on another lane; extend a walk to cover the seam. The wire
work is a commit of its own.

## 4. The full gate — block on any failure

- `cargo fmt --all --check`; release build; `cargo clippy --workspace
  --all-targets -- -D warnings` both plain AND with
  `--features quilltap-core/native-transport`.
- **Regenerate every affected oracle FRESH from the v4 checkout** (each in
  its own clean invocation; jest cases via a `/tmp` mirror; Node 24), then
  `cargo test --workspace` with all differential env vars set — and re-run
  the new differentials by name so you SEE them pass rather than silently
  skip.
- SPA: `ng test`, `ng build`, then the FULL Playwright suite against the
  fresh build. Fix the port (or the spec's gesture), never the assertion.

## 5. Docs, then fast-forward

- CHANGELOG: one unification entry (scope, wires, gate results, final
  versions). Status-log: the round record. Each order's **status header**:
  move landed items to Done, enumerate exactly what stays OPEN.
- **Keep the phase plan current** (the `phase-*.md` decomposition): mark
  this round's items done and make sure what's next is legible — the next
  `/setupphase` reads the phase plan FIRST, so "what's next" must live
  there and in the order status headers, not only in your chat report.
- Update CLAUDE.md's Status only if this completes a round/phase — keep it
  phase-level.
- `git checkout main && git merge --ff-only unify/<round> && git push`.

## 6. Clean up

- Remove each lane worktree (`git worktree remove`), delete the lane
  branches and the temp branch. Check for agent child worktrees that were
  subsumed. Worktree targets are large (tens of GB) — confirm the space
  came back.
- Remove /tmp oracle artifacts, fixture mirrors, stale `test-results/`, and
  any debug servers.
- Update the round's memory note (what landed, what's OPEN, new gotchas)
  and its MEMORY.md index line.

## 7. Report

Lead with what's on main (commit hash, pushed) and the gate numbers. Then
the caveats that change what the human does next: anything a lane banked,
any surface still refusal-armed, and the recommended next ask. Everything
in that recommendation must ALSO already be durable in the docs above — a
fresh `/setupphase` session must be able to reconstruct it without this
conversation.
