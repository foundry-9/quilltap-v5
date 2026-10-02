#!/usr/bin/env bash
# Regenerate the text-provider HTTP-error corpus (P4.118) by driving v4's TEN
# REAL text plugins against the posed exchanges in
# fixtures/text-http-errors/cases.json — non-2xx responses (P4.118), a thrown
# `fetch` (P4.128), a hang under a 50 ms `requestTimeoutMs` and posed 2xx
# bodies (P4.141) — recording what each plugin threw (or answered) plus v4's
# classifyRefusal / classifyFallbackTrigger verdicts and the plugin's own
# ERROR / WARN lines.
#
# Runs FROM the v4 checkout ROOT (so the classifier's `@/` imports resolve);
# the plugins are imported by absolute path, so their SDKs resolve from each
# plugin's own node_modules. Point V4 at a PINNED detached worktree (drift
# ledger §5.1 — the three node_modules symlink classes), never the human's
# checkout when it is past the baseline.
#
# Usage:
#   V4=/tmp/qt-v4-pin-<order>-<sha> bash harness/oracle/providers/regenerate-text-errors.sh
# Requires Node 24 (the ledger's standard; no DB here). Run with TZ=UTC:
#   PATH=$HOME/.nvm/versions/node/v24.13.1/bin:$PATH TZ=UTC \
#     V4=/tmp/qt-v4-pin-<order>-<sha> bash harness/oracle/providers/regenerate-text-errors.sh
# About 30 s: the SDK fetch-throws rows spend ~1.3 s each in the SDKs' own
# retry backoff; the hang rows ~100 ms each.
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
