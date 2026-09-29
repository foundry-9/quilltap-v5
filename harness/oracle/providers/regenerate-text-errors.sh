#!/usr/bin/env bash
# Regenerate the text-provider HTTP-error corpus (P4.118) by driving v4's TEN
# REAL text plugins against the posed non-2xx responses in
# fixtures/text-http-errors/cases.json, recording what each plugin threw plus
# v4's classifyRefusal / classifyFallbackTrigger verdicts over it.
#
# Runs FROM the v4 checkout ROOT (so the classifier's `@/` imports resolve);
# the plugins are imported by absolute path, so their SDKs resolve from each
# plugin's own node_modules. Point V4 at a PINNED detached worktree (drift
# ledger §5.1 — the three node_modules symlink classes), never the human's
# checkout when it is past the baseline.
#
# Usage:
#   V4=/tmp/qt-v4-pin-<order>-<sha> bash harness/oracle/providers/regenerate-text-errors.sh
# Requires Node 24 (the ledger's standard; no DB here).
set -euo pipefail

V4="${V4:-$HOME/source/quilltap-server}"
V5="${V5:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)}"
REC="$V5/harness/oracle/providers/record-text-errors.mjs"
DIR="$V5/harness/oracle/fixtures/text-http-errors"
OUT="$DIR/text-http-errors.recorded.ndjson"

rm -f "$OUT"
( cd "$V4" && npx tsx "$REC" --v4 "$V4" --cases "$DIR/cases.json" --out "$OUT" )
test -s "$OUT"
echo "done — $(wc -l < "$OUT") row(s) in $OUT (v4 at $(git -C "$V4" rev-parse --short HEAD))" >&2
