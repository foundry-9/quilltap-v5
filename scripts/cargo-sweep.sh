#!/usr/bin/env bash
# cargo-sweep's stamp/file cycle for this workspace's target/, made to work
# on APFS. (Not the oracle "sweep driver" — that is harness/tools/recipe_sweep.py.)
#
# Usage:
#   scripts/cargo-sweep.sh stamp       # before a working session
#   scripts/cargo-sweep.sh file [-d]   # after it; -d = dry run (keeps the stamp)
#
# Why not bare `cargo sweep --stamp` / `cargo sweep --file`:
#
# `--file` keeps a build unit only if some file in its
# target/<profile>/.fingerprint/<unit>-<hash>/ dir was ACCESSED after the
# stamp (cargo-sweep 0.8's `last_used_time` reads atime and nothing else).
# APFS updates a file's atime only on the first read after a write: once
# cargo has read a fingerprint, every later read leaves its atime frozen.
# So the plain cycle deletes every unit the build REUSED — the crates.io
# deps, the pinned quilltap-sqlite3mc-sys amalgamation — and keeps only what
# it recompiled, the opposite of the point. Measured 2026-10-03 on a scratch
# two-crate project: plain stamp → no-op build → `--file -d` would clean the
# whole target; with the reset below it cleans nothing, and with the reset
# and no build it cleans everything.
#
# `stamp` therefore pushes every fingerprint file's atime into the past
# right after writing the stamp, so the first read of each unit cargo
# actually consults during the session moves its atime forward again. Do
# not stamp while a build is running against this target: a read that lands
# before the reset is erased by it, and that unit is swept and rebuilt later
# (wasteful, never incorrect). Anything else that reads fingerprint files
# in the window only makes the sweep keep more — the safe direction.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
target="${CARGO_TARGET_DIR:-$root/target}"

case "${1:-}" in
  stamp)
    cargo sweep --stamp "$root"
    if [ -d "$target" ]; then
      # Any time older than every possible stamp works; 2000 rather than
      # the epoch so no time zone renders it negative.
      find "$target" -path '*/.fingerprint/*' -type f \
        -exec touch -a -t 200001010000 {} +
    fi
    echo "Stamped $root/sweep.timestamp; fingerprint atimes reset under $target."
    ;;
  file)
    shift
    cargo sweep --file "$@" "$root"
    ;;
  *)
    echo "usage: $0 stamp | file [-d]" >&2
    exit 2
    ;;
esac
