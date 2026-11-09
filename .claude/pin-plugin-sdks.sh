#!/usr/bin/env bash
# Pin-only plugin + CLI dependency install for a v4 oracle pin worktree
# (human rulings, 2026-11-02; generalized past the `236c04e35` round 2026-11-04).
#
# Replaces symlink classes (2) and (3) of the order pin recipes. History: live
# v4 `9f761e157` re-installed the live plugins/dist/*/node_modules past what was
# then the `236c04e35` pin (openai 7.45.1, @anthropic-ai/sdk 1.3.0,
# @google/genai 2.32.0, @openrouter/sdk 1.5.21, ...), so symlinking them into a
# pin ran every plugin-dir oracle on post-pin SDKs.
#
# At the `624d0d25c` pin the declared floors are openai 7.47.0,
# @anthropic-ai/sdk 1.4.0, @google/genai 2.32.0, @openrouter/sdk 1.5.34 and
# @quilltap/plugin-utils 2.7.3 — equal to `provider_sdk_version_guard`'s
# constants since P4.D364, which re-recorded every provider corpus at them. The live packages/quilltap/node_modules had
# the same problem (the v13 driver, SQLite3MC 2.4.0).
#
# The pin's plugin dirs ship package.json but NO lockfile, and a plain offline
# `npm install` resolves each caret range to the HIGHEST cached version (a
# post-pin one). So this installs each declared dependency at its range FLOOR,
# exactly, inside the PIN only. It never touches the live checkout and never
# writes a lockfile or package.json.
#
# Whenever the pin moves, re-check that the floors printed at the end still
# equal the versions `provider_sdk_version_guard` records.
#
# npm cache: each install tries `--offline` first. If the cache lacks a floor
# packument (ENOTCACHED — every lane after P4.D354 hit it), it retries once with
# `--prefer-offline`, which fetches only the missing public metadata. The
# result is still the exact floor version, installed pin-only.
#
# Usage: pin-plugin-sdks.sh <PIN> <COMMIT>
#   PIN     the detached pin worktree (after its root `npm ci --offline` step)
#   COMMIT  the v4 commit the round is pinned at; the script refuses a pin
#           whose HEAD is anywhere else
set -euo pipefail
PIN=${1:?usage: pin-plugin-sdks.sh <pin worktree> <pinned v4 commit>}
COMMIT=${2:?usage: pin-plugin-sdks.sh <pin worktree> <pinned v4 commit>}
export PATH=$HOME/.nvm/versions/node/v24.13.1/bin:$PATH

want=$(git -C "$PIN" rev-parse --verify "$COMMIT^{commit}") \
  || { echo "unknown commit in the pin: $COMMIT" >&2; exit 1; }
[ "$(git -C "$PIN" rev-parse HEAD)" = "$want" ] \
  || { echo "pin is not at $COMMIT" >&2; exit 1; }

# The declared dependencies of a package dir, each pinned to its range floor.
floor_specs() {
  node -e '
    const p = require(require("path").join(process.argv[1], "package.json"));
    console.log(Object.entries(p.dependencies || {})
      .map(([k, v]) => k + "@" + v.replace(/^[\^~]/, "")).join(" "));
  ' "$1"
}

# floor_install <dir> <label> [extra npm flags...]
floor_install() {
  local dir=$1 label=$2 specs log
  shift 2
  specs=$(floor_specs "$dir")
  [ -n "$specs" ] || return 0
  # A symlink left by the old recipe would make npm write into the LIVE checkout.
  if [ -L "$dir/node_modules" ]; then rm "$dir/node_modules"; fi
  echo "== $label: $specs"
  log=$(mktemp)
  # shellcheck disable=SC2086
  if ! (cd "$dir" && npm install --offline --no-save --no-package-lock --no-audit \
          --no-fund "$@" $specs >"$log" 2>&1); then
    if grep -q ENOTCACHED "$log"; then
      echo "   (cache miss — retrying with --prefer-offline)"
      # shellcheck disable=SC2086
      (cd "$dir" && npm install --prefer-offline --no-save --no-package-lock \
         --no-audit --no-fund "$@" $specs >/dev/null)
    else
      cat "$log" >&2
      rm -f "$log"
      return 1
    fi
  fi
  rm -f "$log"
}

for d in "$PIN"/plugins/dist/*/; do
  floor_install "$d" "$(basename "$d")" --ignore-scripts
done

# The CLI package. Install scripts stay ON here: the driver, sharp,
# @napi-rs/canvas and node-pty are native.
floor_install "$PIN/packages/quilltap" "packages/quilltap"

node -e '
  const m = process.argv[1] + "/packages/quilltap/node_modules/better-sqlite3-multiple-ciphers";
  const db = new (require(m))(":memory:");
  console.log("cli driver", require(m + "/package.json").version,
    "| sqlite", db.prepare("select sqlite_version() v").get().v,
    "| cipher", db.pragma("cipher", { simple: true }));
' "$PIN"
node -e '
  const pin = process.argv[1];
  const v = (d, m) => {
    try { return require(`${pin}/plugins/dist/${d}/node_modules/${m}/package.json`).version; }
    catch { return "absent"; }
  };
  console.log("openai", v("qtap-plugin-openai", "openai"),
    "| anthropic", v("qtap-plugin-anthropic", "@anthropic-ai/sdk"),
    "| genai", v("qtap-plugin-google", "@google/genai"),
    "| openrouter", v("qtap-plugin-openrouter", "@openrouter/sdk"),
    "| plugin-utils", v("qtap-plugin-openai", "@quilltap/plugin-utils"));
' "$PIN"
