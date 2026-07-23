#!/usr/bin/env bash
# Regenerate the response-body oracle NDJSON (P4.13 unit 4 — the #24 close-out)
# by driving each v4 provider plugin's REAL sendMessage with the network mocked
# UNDERNEATH its SDK: fetch returns each corpus body, the SDK's own unwrap runs
# inside the loop, and the recorded output is the LLMResponse v4 built from
# those exact bytes. GOOGLE IS INCLUDED here (unlike request-envelopes): the
# genai SDK's response-side normalization is precisely what the corpus must
# capture; only its request-side reframing is out of scope for the sans-IO
# builder.
#
# All bodies are currently doc-derived (`synthetic: true`) — see the recorder
# header for the capture-tier upgrade path.
#
# Usage: V4=~/source/quilltap-server V5=<repo-root> bash regenerate-response-bodies.sh
# Requires Node 24, run under tsx.
set -euo pipefail

V4="${V4:-$HOME/source/quilltap-server}"
V5="${V5:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)}"
REC="$V5/harness/oracle/providers/record-response-bodies.mjs"
OUT_DIR="$V5/harness/oracle/fixtures/response-bodies"
TMP="$(mktemp -d)"
mkdir -p "$OUT_DIR"

PROVIDERS=(
  "anthropic:qtap-plugin-anthropic"
  "deepseek:qtap-plugin-deepseek"
  "z-ai:qtap-plugin-z-ai"
  "openrouter:qtap-plugin-openrouter"
  "ollama:qtap-plugin-ollama"
  "openai:qtap-plugin-openai"
  "grok:qtap-plugin-grok"
  "google:qtap-plugin-google"
  "openai-compatible:qtap-plugin-openai-compatible"
)

for entry in "${PROVIDERS[@]}"; do
  provider="${entry%%:*}" plugin="${entry##*:}"
  echo "recording response bodies for $provider…" >&2
  ( cd "$V4/plugins/dist/$plugin" && npx tsx "$REC" --provider "$provider" --out "$TMP/$provider.ndjson" )
done

: > "$OUT_DIR/response-bodies.recorded.ndjson"
for entry in "${PROVIDERS[@]}"; do
  cat "$TMP/${entry%%:*}.ndjson" >> "$OUT_DIR/response-bodies.recorded.ndjson"
done

rm -rf "$TMP"
echo "done — $OUT_DIR/response-bodies.recorded.ndjson" >&2
