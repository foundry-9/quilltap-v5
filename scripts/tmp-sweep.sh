#!/usr/bin/env bash
# Reclaim the temp-dir weight this repo's tests and oracles leave behind.
# (Not cargo-sweep.sh — that one garbage-collects main's target/.)
#
# Usage:
#   scripts/tmp-sweep.sh [-n] [--min-age MINUTES]
#     -n               dry run: count and size what would go, delete nothing
#     --min-age N      only scratch entries untouched for N minutes (default 120)
#
# Why this exists — measured 2026-10-05:
#
# 1. JEST'S CACHE. v4's jest oracles cache transforms and haste maps in
#    $TMPDIR/jest_<uid base36> (jest_dz for uid 503) and never evict. The
#    cache key includes the file's PATH, and our regens run from pinned
#    worktrees and per-family restored mirrors at fresh paths, so almost
#    nothing written there is ever read again. It reached ~81 GB. Deleting
#    it costs one cold transform pass on the next jest run.
#
# 2. LEAKED SCRATCH DIRS. Node's `os.tmpdir()` and Rust's
#    `std::env::temp_dir()` both resolve to $TMPDIR (/var/folders/…/T/), NOT
#    /tmp — so "remove /tmp oracle artifacts" never reached them. Most oracle
#    cases `mkdtempSync(join(tmpdir(), 'qt-…-'))` without removing it, as do
#    several harness tests; 80,000 `qt-*` entries held ~36 GB after eleven
#    days (~8,000 a day). macOS only purges $TMPDIR on reboot.
#
# What it removes:
#   - $TMPDIR/jest_*                                  whole, unless jest is running
#   - $TMPDIR/{qt-*,quilltap-backup*} and /tmp/qt-*   only if older than --min-age
#
# The age gate is what makes this safe beside a running gate: a live test's
# scratch dir is younger than the threshold (its top-level mtime moves as
# the test writes into it). Still prefer a quiet tree — /cleanup,
# /setupphase and /unify run this only when no lane or gate is building.
set -euo pipefail

dry=0
min_age=120
while [[ $# -gt 0 ]]; do
  case $1 in
    -n) dry=1 ;;
    --min-age) min_age=${2:?--min-age needs minutes}; shift ;;
    *) echo "usage: $0 [-n] [--min-age MINUTES]" >&2; exit 2 ;;
  esac
  shift
done
[[ $min_age =~ ^[0-9]+$ ]] || { echo "--min-age must be whole minutes" >&2; exit 2; }

tmp=${TMPDIR:-$(getconf DARWIN_USER_TEMP_DIR)}
tmp=${tmp%/}
[[ -d $tmp ]] || { echo "temp dir $tmp does not exist" >&2; exit 1; }

free_kb() { df -k "$1" | awk 'NR==2 {print $4}'; }
human() { awk -v k="$1" 'BEGIN { printf (k >= 1048576 ? "%.1f GB" : "%.0f MB"), (k >= 1048576 ? k/1048576 : k/1024) }'; }

# Is a jest process alive? Only NODE processes count (by executable name,
# `comm`), so no shell whose command line merely mentions jest can match —
# the pgrep -f self-match trap. Their args are then checked for jest.
jest_running=0
while read -r pid comm; do
  [[ ${comm##*/} = node ]] || continue
  case $(ps -o args= -p "$pid" 2>/dev/null) in *jest*) jest_running=1; break ;; esac
done < <(ps -axo pid=,comm=)

before=$(free_kb "$tmp")
echo "temp dir: $tmp   free: $(human "$before")   min-age: ${min_age}m   $([[ $dry = 1 ]] && echo DRY RUN)"

# 1. jest's cache
shopt -s nullglob
jest_dirs=("$tmp"/jest_*)
shopt -u nullglob
if [[ ${#jest_dirs[@]} -eq 0 ]]; then
  echo "jest cache: none"
elif [[ $jest_running = 1 ]]; then
  echo "jest cache: SKIPPED — a jest process is running (re-run when it exits)"
else
  for d in "${jest_dirs[@]}"; do
    if [[ $dry = 1 ]]; then
      echo "jest cache: would remove $d ($(human "$(du -sk "$d" | awk '{print $1}')"))"
    else
      rm -rf "$d"
      echo "jest cache: removed $d"
    fi
  done
fi

# 2. leaked scratch entries, age-gated. A test may delete its own dir while
# we walk, so a vanished-file error is expected and not a failure.
sweep() {
  local dir=$1; shift
  local n
  n=$(find "$dir" -mindepth 1 -maxdepth 1 \( "$@" \) -mmin "+$min_age" 2>/dev/null | wc -l | tr -d ' ')
  if [[ $dry = 1 ]]; then
    local kb=0
    [[ $n -gt 0 ]] && kb=$(find "$dir" -mindepth 1 -maxdepth 1 \( "$@" \) -mmin "+$min_age" -print0 2>/dev/null \
      | xargs -0 du -sk 2>/dev/null | awk '{s+=$1} END {print s+0}')
    echo "scratch: would remove $n entries in $dir ($(human "$kb"))"
  else
    find "$dir" -mindepth 1 -maxdepth 1 \( "$@" \) -mmin "+$min_age" -exec rm -rf {} + 2>/dev/null || true
    echo "scratch: removed $n entries in $dir"
  fi
}
sweep "$tmp" -name 'qt-*' -o -name 'quilltap-backup*'
real_tmp=$(cd /tmp && pwd -P)   # /tmp is a symlink; find will not descend one
[[ $real_tmp != "$(cd "$tmp" && pwd -P)" ]] && sweep "$real_tmp" -name 'qt-*'

after=$(free_kb "$tmp")
[[ $dry = 1 ]] || echo "free: $(human "$before") -> $(human "$after") (reclaimed $(human $((after - before))))"
