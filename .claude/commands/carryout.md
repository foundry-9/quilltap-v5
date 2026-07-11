---
description: Carry out one porting work order as an isolated lane on its own branch
argument-hint: <path to a work order under docs/developer/porting/work-orders/>
---

# Carry out a work order

Execute the work order at: **$ARGUMENTS**

Read the ENTIRE order before writing any code. The order is binding — its
Shared contract, Ownership, and deferral sections are not suggestions. If the
order has a status header, only the OPEN items are your scope; do not redo
what's marked landed.

## Ground rules for the lane

1. **Drift-check first.** The order pins a v4 baseline commit. In
   `~/source/quilltap-server`, run `git log <baseline>..HEAD`. If HEAD moved,
   **STOP and report** — do not port against a moved oracle.
2. **Work on your own branch, never on main.** Use a worktree
   (`.claude/worktrees/…`, branch `claude/<order-slug>-…`) or a branch off
   main. Unification back into main is a separate step (`/unify`) — do not
   merge, rebase onto main mid-lane, or push to main. Don't use `git stash`.
3. **Stay inside your Ownership section.** `docs/CHANGELOG.md` and
   `docs/developer/porting/status-log.md` are append-only (they union-merge
   at unification). If you believe you must touch a file another lane owns,
   stop and report instead.
4. **The differential discipline is non-negotiable.** For every unit: fresh
   v4 survey → port → oracle case that drives v4's **real** `lib/` code →
   regenerate the oracle → run the Rust diff → **fix the port, not the
   diff**. Never accept a unit without its equivalence test. Reuse the
   recipes in the memory notes and `status-log.md` (jest `/tmp` mirror
   because jest ignores `.claude/` paths; Node 24 at
   `~/.nvm/versions/node/v24.13.1/bin`; regenerate each oracle in its OWN
   clean invocation — batched runs have left stale minted ids).
5. **Small units, one commit each**, every commit through
   `.claude/commands/commit.md` (changelog entry, version bump for touched
   crates, full workspace gate). Append each unit's record to
   `status-log.md` as you go.
6. **Defer loudly, never silently.** Anything you bank instead of landing
   gets the loud typed refusal pattern (the `not_available` idiom), an entry
   in your status-log records, and a line in your final report. No stubs, no
   TODOs.
7. **Safety:** never point a writable open at live Friday data; the pepper
   never gets committed, logged, or written where it syncs; test-pepper
   synthetic fixtures only.

## Before declaring done

Run the order's verification gate yourself: `cargo fmt --all --check`,
`cargo clippy --workspace --all-targets -- -D warnings` (AND with
`--features quilltap-core/native-transport`), `cargo test --workspace` with
your differential env vars set (confirm your tests RAN, not skipped), plus
the SPA gate (`ng test`, `ng build`) if the order touches `apps/web`.

## Final report

Report back: the branch name; the commit list; what landed vs what remains
OPEN under the order (be exact — the unifier and the human rely on this);
every fixture you changed and which other oracles that invalidates; the
exact regen recipe for each oracle you authored (env vars, invocation,
working directory); and any gotchas worth a memory note.
