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
8. **Disk discipline.** A lane's per-worktree `target/` has grown to ~70 GB
   (each full `cargo test --workspace` gate adds ~10 GB of
   `target/debug/incremental`), and full disks have blocked the harness
   mid-gate as mysterious slowness or `os error 28`. So:
   - **Check `df -h ~` before starting** and before each full workspace
     gate. If free space is under ~20 GB, reclaim before building.
   - **Run the workspace gates with `CARGO_INCREMENTAL=0`** — the
     incremental cache is the multiplier and buys little in a
     compile-once-per-commit lane.
   - **Mid-lane reclaim, safe anytime:** delete YOUR OWN worktree's
     `target/debug/incremental` (pure regenerable cache; only costs
     recompile). **Never** touch a sibling lane's `target/` — it may be
     actively building.
   - Keep only what the lane needs: don't accumulate extra
     `--release` builds, scratch clones, or copied fixtures beyond what the
     order's gate requires (e2e can reuse main's release bins when the
     order allows it).

## Before declaring done

Run the order's verification gate yourself: `cargo fmt --all --check`,
`cargo clippy --workspace --all-targets -- -D warnings` (AND with
`--features quilltap-core/native-transport`), `cargo test --workspace` with
your differential env vars set (confirm your tests RAN, not skipped), plus
the SPA gate (`ng test`, `ng build`) if the order touches `apps/web`.

## After the last commit: clean up your binaries

Once the gate is green and every commit is on the lane branch, the
worktree's build artifacts are pure regenerable weight — the unifier
rebuilds on its own branch and never reuses them. Before (or alongside)
your final report:

- `rm -rf <your-worktree>/target` (or `cargo clean --manifest-path
  <your-worktree>/Cargo.toml`). Yours only — never a sibling lane's.
- Delete any scratch build output you created outside the worktree
  (oracle regen scratch dirs, copied instance fixtures, `/tmp` release
  builds). Committed fixtures and NDJSON oracles stay, obviously.
- Leave the worktree and branch themselves intact — `/unify` consumes
  and removes them.

## Final report

Report back: the branch name; the commit list; what landed vs what remains
OPEN under the order (be exact — the unifier and the human rely on this);
every fixture you changed and which other oracles that invalidates; the
exact regen recipe for each oracle you authored (env vars, invocation,
working directory); any gotchas worth a memory note; and confirmation that
the lane's `target/` was cleaned.
