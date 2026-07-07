#!/usr/bin/env bash
# Regenerate the request-envelope oracle NDJSON by driving each v4 provider
# plugin's REAL streamMessage and intercepting the outgoing fetch to capture the
# built request (wave 4 / W4.7c part 2). Google is EXCLUDED — the genai SDK
# reframes the request into a wire body this sans-IO builder does not produce; the
# google request LOGIC is verified separately (request-builder-google.ts).
#
# Usage: V4=~/source/quilltap-server V5=<repo-root> bash regenerate-request-envelopes.sh
# Requires Node 24, run under tsx.
set -euo pipefail

V4="${V4:-$HOME/source/quilltap-server}"
V5="${V5:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)}"
REC="$V5/harness/oracle/providers/record-request-envelopes.mjs"
OUT_DIR="$V5/harness/oracle/fixtures/request-envelopes"
TMP="$(mktemp -d)"
mkdir -p "$OUT_DIR"

run() {
  local provider="$1" plugin="$2"
  echo "recording request envelopes for $provider…" >&2
  ( cd "$V4/plugins/dist/$plugin" && npx tsx "$REC" --provider "$provider" --out "$TMP/$provider.ndjson" )
}

run anthropic  qtap-plugin-anthropic
run deepseek   qtap-plugin-deepseek
run z-ai       qtap-plugin-z-ai
run openrouter qtap-plugin-openrouter
run ollama     qtap-plugin-ollama
run openai     qtap-plugin-openai
run grok       qtap-plugin-grok

cat "$TMP/anthropic.ndjson" "$TMP/deepseek.ndjson" "$TMP/z-ai.ndjson" \
    "$TMP/openrouter.ndjson" "$TMP/ollama.ndjson" "$TMP/openai.ndjson" "$TMP/grok.ndjson" \
  > "$OUT_DIR/request-envelopes.recorded.ndjson"
rm -rf "$TMP"
echo "done — $OUT_DIR/request-envelopes.recorded.ndjson" >&2
