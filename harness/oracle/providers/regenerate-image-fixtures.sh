#!/usr/bin/env bash
# Regenerate the image-dialect oracle NDJSON by driving each v4 image-provider
# plugin's REAL generateImage with global.fetch mocked to a committed wire payload
# (W4.7f). Captures the built request + the parsed result / thrown error string.
#
# Usage: V4=~/source/quilltap-server V5=<repo-root> bash regenerate-image-fixtures.sh
# Requires Node 24, run under tsx.
set -euo pipefail

V4="${V4:-$HOME/source/quilltap-server}"
V5="${V5:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)}"
REC="$V5/harness/oracle/providers/record-image-fixtures.mjs"
OUT_DIR="$V5/harness/oracle/fixtures/image-dialects"
TMP="$(mktemp -d)"
mkdir -p "$OUT_DIR"

run() {
  local provider="$1" plugin="$2"
  echo "recording image dialect for $provider…" >&2
  ( cd "$V4/plugins/dist/$plugin" && npx tsx "$REC" --provider "$provider" --out "$TMP/$provider.ndjson" )
}

run openai     qtap-plugin-openai
run google     qtap-plugin-google
run grok       qtap-plugin-grok
run openrouter qtap-plugin-openrouter
run z-ai       qtap-plugin-z-ai
run nanogpt    qtap-plugin-nanogpt

cat "$TMP/openai.ndjson" "$TMP/google.ndjson" "$TMP/grok.ndjson" \
    "$TMP/openrouter.ndjson" "$TMP/z-ai.ndjson" "$TMP/nanogpt.ndjson" \
  > "$OUT_DIR/image-dialects.recorded.ndjson"
rm -rf "$TMP"
echo "done — $OUT_DIR/image-dialects.recorded.ndjson" >&2
