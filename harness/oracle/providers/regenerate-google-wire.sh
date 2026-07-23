#!/usr/bin/env bash
# Regenerate the GOOGLE wire-body oracle NDJSON (W4.7d; the non-streaming half is
# P4.11 unit 6). Drives v4's REAL google plugin in both modes — `streamMessage`
# (`ai.models.generateContentStream`) and `sendMessage`
# (`ai.models.generateContent`) — and captures the exact bytes the genai SDK
# serialized plus the model-specific url, which is where the
# `:streamGenerateContent?alt=sse` vs `:generateContent` split lives.
#
# Usage: V4=~/source/quilltap-server V5=<repo-root> bash regenerate-google-wire.sh
# Requires Node 24, run under tsx.
set -euo pipefail

V4="${V4:-$HOME/source/quilltap-server}"
V5="${V5:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)}"
REC="$V5/harness/oracle/providers/record-request-envelopes.mjs"
OUT_DIR="$V5/harness/oracle/fixtures/request-envelopes"
TMP="$(mktemp -d)"
mkdir -p "$OUT_DIR"

for mode in stream send; do
  echo "recording google wire bodies [$mode]…" >&2
  ( cd "$V4/plugins/dist/qtap-plugin-google" && npx tsx "$REC" --provider google --mode "$mode" --out "$TMP/google.$mode.ndjson" )
done

cat "$TMP/google.stream.ndjson" "$TMP/google.send.ndjson" > "$OUT_DIR/google-wire.recorded.ndjson"
rm -rf "$TMP"
echo "done — $OUT_DIR/google-wire.recorded.ndjson" >&2
