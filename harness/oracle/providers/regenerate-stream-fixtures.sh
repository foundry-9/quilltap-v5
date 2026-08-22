#!/usr/bin/env bash
# Regenerate every stream-decoder oracle NDJSON from the committed wire
# transcripts by driving v4's REAL plugin parsers (wave 4 / W4.7b).
#
# For each (provider, decoder) it runs record-stream-fixtures.mjs FROM the
# plugin directory (so the plugin's own node_modules resolve its SDK +
# @quilltap/plugin-utils), reading fixtures/streams/<decoder>/cases.json +
# *.wire and writing the recorded normalized StreamChunk NDJSON to
# fixtures/streams/<decoder>/<provider>.recorded.ndjson (committed).
#
# Usage:
#   V4=~/source/quilltap-server V5=<repo-root> bash regenerate-stream-fixtures.sh
# Requires Node 24 (see [[oracle-node-abi-gotcha]] — no DB here, but standardise).
set -euo pipefail

V4="${V4:-$HOME/source/quilltap-server}"
V5="${V5:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)}"
REC="$V5/harness/oracle/providers/record-stream-fixtures.mjs"
STREAMS="$V5/harness/oracle/fixtures/streams"

# provider → (plugin-dir, decoder)
run() {
  local provider="$1" plugin="$2" decoder="$3"
  echo "recording $provider ($decoder)…" >&2
  # Run under tsx: some plugins import sibling `.ts` modules (e.g. z-ai's
  # `./models`) that plain `node`'s ESM loader cannot resolve extensionless.
  ( cd "$V4/plugins/dist/$plugin" && \
    npx tsx "$REC" \
      --provider "$provider" \
      --cases "$STREAMS/$decoder/cases.json" \
      --fixtures-dir "$STREAMS/$decoder" \
      --out "$STREAMS/$decoder/$provider.recorded.ndjson" )
}

run deepseek   qtap-plugin-deepseek   chat_completions_sse
run z-ai       qtap-plugin-z-ai       chat_completions_sse
run openrouter qtap-plugin-openrouter chat_completions_sse
run openai-compatible qtap-plugin-openai-compatible chat_completions_sse
run nanogpt    qtap-plugin-nanogpt    chat_completions_sse
run openai     qtap-plugin-openai     responses_api_sse
run grok       qtap-plugin-grok       responses_api_sse
run anthropic  qtap-plugin-anthropic  anthropic_sse
run google     qtap-plugin-google     google_parts
run ollama     qtap-plugin-ollama     ollama_ndjson

echo "done — recorded NDJSON written under $STREAMS/*/*.recorded.ndjson" >&2
