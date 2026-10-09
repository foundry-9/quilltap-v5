---
description: Build the release binary exactly as /dogfood needs it, then cargo-sweep main's target/ so only current artifacts survive — the release build plus (by default) the dev/test/clippy set the lanes clone — and sweep the temp dir (jest's cache, leaked qt-* scratch)
argument-hint: [--release-only]
---

# Clean up main's `target/`

Leave main's `target/` holding **only artifacts that are current for this
tree**: the release build `/dogfood` launches, plus — unless `$ARGUMENTS`
contains `--release-only` — the dev/test/clippy set `/setupphase`'s warm
build leaves for the lanes. Everything else (stale hashes from older
sources, other feature sets, dead lanes' leftovers) is swept.

This is the one other sanctioned place to run `scripts/cargo-sweep.sh`
besides `/setupphase` (CLAUDE.md, "Main's `target/` is garbage-collected with
cargo-sweep"). Read that script's header before changing anything here: the
plain `cargo sweep --stamp`/`--file` cycle deletes exactly the REUSED
artifacts on APFS; the script's atime reset is what makes "kept = used this
session" true. Never `cargo clean`, never `rm -rf target/`, never a bare
`cargo sweep`.

## 1. Preconditions

- Run from **main's checkout** (`/Users/csebold/source/quilltap-v5`), never a
  lane worktree — lane targets are deleted whole at lane close, not swept.
- **Nothing else may be building against main's `target/`**: no
  `/setupphase` warm build, no gate, no `/dogfood` release build, no running
  `cargo test` from another session. A fingerprint read that lands before the
  stamp's atime reset gets its artifact swept and rebuilt later (wasteful,
  never incorrect). Check with `pgrep -x cargo` / `pgrep -x rustc` (exact
  binary names — never `pgrep -f` with a pattern your own command contains).
  If anything is running, STOP and ask.
- A stopped `quilltap-web` from a dogfood walk is fine; a RUNNING one holds
  `target/release/quilltap-web` open — sweeping replaces the file, the process
  keeps its inode. Mention it in the report; don't kill it.
- Record `df -h ~/source/quilltap-v5/` and `du -sh target` before — the
  filesystem the repo and its `target/` really live on.
  `~/source/quilltap-v5` is a symlink onto `/Volumes/External 1`, so a bare
  `df -h ~` reports the boot volume, which `target/` is not on (the trailing
  slash makes `df` follow the link). `scripts/tmp-sweep.sh` reports
  `$TMPDIR`'s free space, which IS the boot volume — a different disk;
  report the two separately.

## 1a. Sweep the temp dir

Run `scripts/tmp-sweep.sh` (foreground; seconds to a few minutes). Read its
header first. It deletes jest's cache (`$TMPDIR/jest_*` — v4's jest oracles
never evict it, and it reached ~81 GB) and the `qt-*` / `quilltap-backup*`
scratch dirs tests and oracles leak into `$TMPDIR` and `/tmp`, age-gated at
120 minutes. Node's `tmpdir()` and Rust's `temp_dir()` are `$TMPDIR`
(`/var/folders/…/T/`), **not** `/tmp` — that is where the weight is. If it
prints `jest cache: SKIPPED`, a jest run is alive somewhere; say so in the
report rather than killing it. Keep its output for the report.

## 2. Stamp → build → dry run → sweep

Run as ONE chain with `run_in_background: true`, full output to a log in the
scratchpad plus a sentinel file written on exit (CLAUDE.md's ⛔ rule: wait for
the completion notification; no poll loops, no `tail -N`). The default chain:

```bash
scripts/cargo-sweep.sh stamp && \
cargo build --release && \
CARGO_INCREMENTAL=0 cargo test --workspace --no-run && \
CARGO_INCREMENTAL=0 cargo clippy --workspace --all-targets -- -D warnings && \
CARGO_INCREMENTAL=0 cargo clippy --workspace --all-targets \
  --features quilltap-core/native-transport -- -D warnings && \
scripts/cargo-sweep.sh file -d && \
scripts/cargo-sweep.sh file
```

With `--release-only`, drop the three `CARGO_INCREMENTAL=0` lines. Rules:

- **The release step is exactly `cargo build --release`** — never
  `-p quilltap-web`. Selecting one package resolves dependency features
  differently, so dependencies (the pinned amalgamation included) get new
  hashes and rebuild beside the warm copies — and the `/dogfood` build would
  then not match what this command kept.
- **The dev/test/clippy lines are copied verbatim from `/setupphase` step 6**
  (same commands, same `CARGO_INCREMENTAL=0`), so they reuse exactly the
  hashes the lanes clone. If `/setupphase`'s warm chain changes, change this
  one to match. On a warm tree they are near no-ops whose only job is to mark
  the current artifacts as used; stale ones rebuild here and their old hashes
  are swept.
- **`--release-only` trades disk for time:** the next `/setupphase` warm build
  starts cold (the amalgamation in dev included). Say so in the report.
- **The `file -d` dry run is the record** of what was removed (cargo-sweep
  lists each unit) — keep its output in the log.
- **A failure stops the chain before the sweep** (the stamp stays; the next
  run overwrites it). A red build or clippy on main is news — report it with
  the log lines, don't fix it here, and don't sweep around it.

## 3. Report

- The temp sweep's lines: whether jest's cache went (and its size if the
  dry run was taken), how many scratch entries went, the space reclaimed.
- Exit status of each step (from the log, not the notification alone).
- `df -h ~/source/quilltap-v5/` and `du -sh target` after, and the space
  reclaimed on that volume (the temp sweep's boot-volume line separately).
- A summary of the dry run: how many units removed, by profile
  (`debug`/`release`), and any surprise — a `quilltap-sqlite3mc-sys` unit in
  the removed list for a profile this run built means the atime reset or the
  feature set misfired; say so loudly.
- Whether `target/release/quilltap-web` is ready for `/dogfood` (it is, if the
  chain was green). The SPA (`apps/web/dist/`) is not under `target/` and this
  command does not touch or build it — `/dogfood` still needs
  `cd apps/web && npm run build`.
