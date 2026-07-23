#!/usr/bin/env bash
# Regenerate the request-envelope oracle NDJSON by driving each v4 provider
# plugin's REAL streamMessage AND its REAL sendMessage, intercepting the outgoing
# fetch to capture the built request (wave 4 / W4.7c part 2; the non-streaming
# `send` half is P4.11 unit 1). Google is EXCLUDED from this corpus — the genai
# SDK reframes the request into a wire body this sans-IO builder does not produce;
# the google request LOGIC is verified separately (request-builder-google.ts) and
# its wire bytes by `regenerate-google-wire.sh`.
#
# Every provider is recorded in BOTH modes and the halves are concatenated
# stream-first, so the streaming block stays a contiguous, diffable prefix.
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

PROVIDERS=(
  "anthropic:qtap-plugin-anthropic"
  "deepseek:qtap-plugin-deepseek"
  "z-ai:qtap-plugin-z-ai"
  "openrouter:qtap-plugin-openrouter"
  "ollama:qtap-plugin-ollama"
  "openai:qtap-plugin-openai"
  "grok:qtap-plugin-grok"
  "openai-compatible:qtap-plugin-openai-compatible"
)

run() {
  local provider="$1" plugin="$2" mode="$3"
  echo "recording request envelopes for $provider [$mode]…" >&2
  ( cd "$V4/plugins/dist/$plugin" && npx tsx "$REC" --provider "$provider" --mode "$mode" --out "$TMP/$provider.$mode.ndjson" )
}

for mode in stream send; do
  for entry in "${PROVIDERS[@]}"; do
    run "${entry%%:*}" "${entry##*:}" "$mode"
  done
done

: > "$OUT_DIR/request-envelopes.recorded.ndjson"
for mode in stream send; do
  for entry in "${PROVIDERS[@]}"; do
    cat "$TMP/${entry%%:*}.$mode.ndjson" >> "$OUT_DIR/request-envelopes.recorded.ndjson"
  done
done

rm -rf "$TMP"
echo "done — $OUT_DIR/request-envelopes.recorded.ndjson" >&2
