#!/usr/bin/env bash
# Regenerate the tool-wire oracle NDJSON by driving v4's REAL plugin tool-wire
# methods (formatTools / parseToolCalls / hasTextToolMarkers / parseTextToolCalls
# / stripTextToolMarkers) over the committed corpus (wave 4 / W4.7c).
#
# Drives one provider per formatTools/parseToolCalls tool_format branch
# (anthropic / openai / google) plus deepseek (an OpenAI-format provider over a
# recorded escaped-quote rawResponse). The per-provider dispatch for every OTHER
# provider is proven by provider_registry_equivalence (the tool_format rows).
#
# Each recorded line carries { kind, provider, case, input, result }, so the Rust
# `tool_wire_equivalence` differential reads the input, recomputes, and diffs.
#
# Usage:
#   V4=~/source/quilltap-server V5=<repo-root> bash regenerate-tool-wire.sh
# Requires Node 24; run under tsx (plugins import sibling .ts modules).
set -euo pipefail

V4="${V4:-$HOME/source/quilltap-server}"
V5="${V5:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)}"
REC="$V5/harness/oracle/providers/record-tool-wire.mjs"
OUT_DIR="$V5/harness/oracle/fixtures/tool-wire"
TMP="$(mktemp -d)"
mkdir -p "$OUT_DIR"

run() {
  local provider="$1" plugin="$2"
  echo "recording tool-wire for $provider…" >&2
  ( cd "$V4/plugins/dist/$plugin" && \
    npx tsx "$REC" --provider "$provider" --out "$TMP/$provider.ndjson" )
}

run anthropic qtap-plugin-anthropic
run openai    qtap-plugin-openai
run google    qtap-plugin-google
run deepseek  qtap-plugin-deepseek

cat "$TMP/anthropic.ndjson" "$TMP/openai.ndjson" "$TMP/google.ndjson" "$TMP/deepseek.ndjson" \
  > "$OUT_DIR/tool-wire.recorded.ndjson"
rm -rf "$TMP"
echo "done — $OUT_DIR/tool-wire.recorded.ndjson" >&2
