# SURVEY SDK+RIDERS — `6d0f88d65` + `83d0c969b` + `a8292547a` — fresh survey 2026-09-25 and on v5 main `2aed9a552`

Read-only survey. v4 checkout `~/source/quilltap-server` at HEAD `08c49319d`, clean. Every v4 read was
`git show` / `git archive` into the scratchpad, or a `node_modules/` file read. The one thing run on the v5
side was `cargo test -p quilltap-harness --test provider_sdk_version_guard`, which only reads (result in §B.1).

**Headline facts the order must carry**

1. **The regen event began at #73 (`8bd080267`), not at `6d0f88d65`.** The human's `node_modules` were
   already partly updated when #73 was built. #73's rebuilt `grok` / `openai` / `z-ai` bundles already stamp
   `openai` **7.23.0**, and its `openrouter` bundle stamps `@openrouter/sdk` **1.3.27**, a 50,212-line bundle
   diff. `6d0f88d65` only moves the manifests and rebuilds the remaining 11 bundles. In practice this makes no
   difference: **no v5 recorder reads a bundle for request bytes.** Every provider recorder imports the
   plugin's `.ts` source, and the SDK is resolved from the plugin's own `node_modules` (see §F.1).
2. **The guard is RED on main right now.** Its installed-vs-recorded half fails on 9 locations. Its corpora
   half still passes. So any `--workspace` gate run with the checkout present carries one red until this lands.
3. **One guard constant covers TWO corpora owned by different lanes.** `RECORDED_OPENAI_SDK` and
   `RECORDED_OPENROUTER_SDK` are asserted over both `request-envelopes` (the riders lane) and
   `image-dialects` (the #73 substrate lane re-records it). Whichever lane bumps the constants turns the other
   corpus red until that corpus is re-recorded (§B.5).
4. **KaTeX pin lag, not an SDK issue.** v5 `apps/web` pins top-level `katex` at **0.18.4**. v4 has been on
   0.18.7 since `6b0615807` (2026-09-21) and is now on **0.18.9**. The math fixture bytes come from the nested
   `rehype-katex/node_modules/katex` **0.16.47**, which is identical in both trees, so the bytes are neutral.
   The capture tool's own rule still says "bump it in step and recapture" (§A.5).

---

## A. `6d0f88d65` — the version deltas

The commit (2026-09-25 11:07, "npm update -S on the root, packages/, and all 15 plugins") touches 51 files:
README, `docs/CHANGELOG.md`, root `package.json` + `package-lock.json`, `packages/plugin-types/package-lock.json`,
`packages/plugin-utils/{package.json,package-lock.json,src/version.generated.ts}`,
`packages/quilltap/package.json`, `packages/theme-storybook/package-lock.json`, and under `plugins/dist/*` the
`index.js` / `manifest.json` / `package.json` files.
**There is NO `lib/`, `app/`, `help/`, DDL, `__tests__/` or `zod` hunk.** `zod` does not appear in the lock
diff. The installed version is 4.6.5, the same as `zod_version_guard`.

### A.1 Root `package.json` (`4.10.0-dev.90` → `dev.91`)

| dep | before → after | section |
|---|---|---|
| `openai` | ^7.20.0 → **^7.23.0** | deps |
| `@openrouter/sdk` | ^1.3.11 → **^1.3.28** | devDeps; `trustedDependencies` key moved to `@openrouter/sdk@1.3.28` |
| `@quilltap/plugin-utils` | ^2.6.1 → ^2.6.2 | deps |
| `next` / `eslint-config-next` | ^16.3.5 → ^16.3.6 | deps / devDeps |
| `katex` | ^0.18.7 → ^0.18.9 | deps |
| `create-quilltap-theme` | ^2.0.19 → ^2.0.20 | deps |
| `@quilltap/plugin-types` | ^2.8.0 (unchanged here; #73 moved it) | deps |

Root lock moves (full list): `openai` 7.20.0→7.23.0, `@openrouter/sdk` 1.3.11→1.3.28, `next` + `@next/env` +
`@next/eslint-plugin-next` + `@next/swc-*` ×8 16.3.5→16.3.6, `eslint-config-next`, `katex` 0.18.7→0.18.9,
`yjs` 13.6.32→.33, `lib0` 0.2.117→.118, `copy-anything` 4.1.1→.2, `browserslist`, `baseline-browser-mapping`,
`caniuse-lite`, `electron-to-chromium`, `node-releases`, nested `ignore` 7.0.9→.10. The lock's own version
stamp jumps 4.10.0-dev.80 → dev.91 (it had lagged). `better-sqlite3` does not move.

### A.2 `packages/`

| package | change |
|---|---|
| `packages/plugin-utils` | 2.6.2 → **2.6.3**; dep `@quilltap/plugin-types` ^2.7.0 → ^2.8.0; devDep `openai` ^7.20.0 → ^7.23.0; `PLUGIN_UTILS_VERSION` '2.6.2' → '2.6.3' (the only `src/` hunk) |
| `packages/plugin-types` | lockfile only (its 2.8.0 bump is #73's: `ModerationRejectionError`) |
| `packages/theme-storybook` | lockfile only |
| `packages/quilltap` | version `4.10.0-dev.90` → `dev.91` only |

v4's `jest.config.ts:54-55` maps `@quilltap/plugin-utils` to `packages/plugin-utils/src`, so jest oracles see
2.6.3 source. The only change there is the version constant. Grepping v5 for `2.6.2` / `2.6.3` /
`PLUGIN_UTILS_VERSION` in `crates/` and `harness/oracle/` finds nothing, so nothing in v5 records it.

### A.3 All 15 plugins (`package.json` + `manifest.json` patch bump)

| plugin | version | dep moves |
|---|---|---|
| anthropic | 1.0.60 → 1.0.61 | plugin-types ^2.7.0→^2.8.0, plugin-utils ^2.6.1→^2.6.2 |
| builtin-embeddings | 1.0.19 → 1.0.20 | plugin-types ^2.8.0 |
| curl | 1.0.29 → 1.0.30 | types ^2.8.0, utils ^2.6.2 |
| deepseek | 1.0.26 → 1.0.27 | types, utils, **openai ^7.20.0→^7.23.0** |
| default-system-prompts | 1.1.22 → 1.1.23 | types, utils |
| google | 1.1.54 → 1.1.55 | utils ^2.6.2 (types already ^2.8.0 from #73) |
| grok | 1.0.57 → 1.0.58 | utils, **openai ^7.23.0** |
| mcp | 1.1.42 → 1.1.43 | **`@modelcontextprotocol/sdk` ^1.30.0→^1.30.1**, types, utils |
| nanogpt | 1.2.7 → 1.2.8 | types, utils, **openai ^7.23.0** |
| ollama | 1.0.51 → 1.0.52 | types, utils |
| openai-compatible | 1.0.47 → 1.0.48 | types, utils, **openai ^7.23.0** |
| openai | 1.0.65 → 1.0.66 | utils, **openai ^7.23.0** |
| openrouter | 1.0.65 → 1.0.66 | **`@openrouter/sdk` ^1.3.11→^1.3.28**, utils |
| search-serper | 1.0.24 → 1.0.25 | types, utils |
| z-ai | 1.1.30 → 1.1.31 | utils, **openai ^7.23.0** |

Nothing in v5 records a plugin or manifest version. A grep of `crates/`, `harness/oracle/` and `apps/web/src`
for every old version string finds only an unrelated `1.2.7` inside the SQLite amalgamation.

### A.4 Rebuilt bundles and what the bundle diff shows

`6d0f88d65` rebuilt 11 `index.js` bundles: **anthropic, curl, deepseek, default-system-prompts, google, mcp,
nanogpt, ollama, openai-compatible, openrouter, search-serper**. It did NOT rebuild `grok`, `openai` or `z-ai`,
because #73 had already rebuilt them at 7.23.0. It also did not rebuild `builtin-embeddings`, which bundles no
SDK and was last built in February.

The SDK stamp in each bundle at three commits (`openai` `VERSION`; `or:` is `@openrouter/sdk` `sdkVersion`).
Every bundle except the SDK-free `builtin-embeddings` inlines `openai` through `@quilltap/plugin-utils`:

| bundle | `b0b6656b5` | `8bd080267` (#73) | `6d0f88d65` |
|---|---|---|---|
| grok, openai, z-ai | 7.20.0 | **7.23.0** | 7.23.0 (not rebuilt) |
| openrouter | 7.20.0, or:1.3.11 | 7.20.0, **or:1.3.27** | 7.23.0, **or:1.3.28** |
| the other 10 SDK-bearing bundles | 7.20.0 | 7.20.0 | 7.23.0 |

**What the diff contains, taking the anthropic bundle as the example (330 hunks).** Nearly all of it is
mechanical, from openai 7.21's "preserve model choices and improve request handling":

- Every resource method now wraps its options as
  `resolveResourceRequestOptionsN(options, (options2) => ({ ...options2, __security }))`. The effect is that
  options may be a Promise. The headers, bodies and paths stay the same.
- `VERSION2` goes from "7.20.0" to "7.23.0". That value is `x-stainless-package-version`.

Inside the `OpenAI` client class, only four lines change:

- `maxRetries` is now passed through `_OpenAI_normalizeRetries`. That function returns `value` when it is a
  safe integer ≥ 0, and 2 otherwise.
- `retriesRemaining` is clamped to `maxRetries`.
- `logger` is passed through `_OpenAI_sanitizeLogger`, which redacts credentials in the SDK's own log lines.

**Nothing changes `buildHeaders`, the `X-Stainless-*` set, `User-Agent`, the default query or the body
defaults.** The only other change is a doc comment on the external-storage delete.

OpenRouter bundle, 1.3.27 → 1.3.28: the `sdkVersion` / `userAgent` stamp changes, and one Intern-daemon
matcher drops 429 from its `jsonErr` list. The `HTTP-Referer` / `X-OpenRouter-Title` /
`X-OpenRouter-Categories` encoders are the same at 1.3.11 and at 1.3.28 (the count grows 88→90 with two new
endpoints). The 1.3.11 → 1.3.27 jump is inside #73's 50k-line bundle diff and was not audited line by line.
**The re-record is the proof.**

### A.5 Changelogs

`node_modules/openai/CHANGELOG.md`, 7.20.0 → 7.23.0:

- **7.21.0** (2026-09-22):
  - feat: session environment reset events (the realtime API, which v4 does not use).
  - fix: **"preserve model choices and improve request handling" (#2787)**. This is the options-promise
    wrapper above. It is the only entry that could touch request bytes, and the bundle diff shows no body or
    header change.
  - chore: docs.
- **7.22.0**: GPT-6 Sol / Luna model identifiers. These are type-only; v5's image and model tables are v4's
  own, not the SDK's.
- **7.23.0**: GCP external storage (admin API), the GPT-Rosalind model id (types), and documented Chat
  Completions `seed` bounds (docs only).

**No entry moves pricing, retry headers, `X-Stainless-*`, timeouts or the body defaults.**

`@openrouter/sdk` ships no CHANGELOG (the package has `FUNCTIONS.md`, `README.md`, `RUNTIMES.md`,
`_speakeasy/`). Evidence from package metadata and schemas at the installed 1.3.28:

- `esm/models/model.js`: `Model$inboundSchema` declares the same 20 keys, with the same 11-entry `remap$` table
  (`alias_target`… `top_provider`). It is identical to both of v5's transcriptions: `pricing_fetcher/mod.rs:351`
  `OPENROUTER_MODEL_REMAP`, and `image_dialects.rs` `MODEL_KEYS` / `ARCH_KEYS` (arch keys `input_modalities`,
  `instruct_type`, `modality`, `output_modalities`, `tokenizer`).
- `esm/funcs/modelsList.js:128-146`: the page rule is unchanged (`limit ?? 500`, stop when
  `results.length < limit`, `offset + results.length`), which matches `OPENROUTER_PAGE_LIMIT = 500` and
  `openrouter_next_page_offset`.

**KaTeX.** v4 now has top-level `katex` 0.18.9, but `rehype-katex` 7.0.1 still nests its own `katex` 0.16.47.
The v5 SPA has `katex` 0.18.4, with the same `rehype-katex` 7.0.1 and nested 0.16.47.
`apps/web/tooling/capture-markdown-fixtures.mts:28-36` says the math bytes come from the nested copy, and that
the top-level pin "keeps the two trees resolving alike, so bump it in step and recapture". The pin has lagged
since `6b0615807` (0.18.7). This is housekeeping, not a byte change.

### A.6 Installed versions (read 2026-09-25; `node` on PATH = `/usr/local/bin/node` v24.13.1)

| location | openai | @openrouter/sdk | @anthropic-ai/sdk | @google/genai | @modelcontextprotocol/sdk | plugin-types | plugin-utils |
|---|---|---|---|---|---|---|---|
| root `node_modules` | **7.23.0** | **1.3.28** | — | — | — | 2.8.0 | 2.6.2 |
| `packages/plugin-utils/node_modules` | 7.23.0 | — | — | — | — | 2.8.0 | n/a |
| `qtap-plugin-anthropic` | — | — | 0.115.0 | — | — | 2.8.0 | 2.6.2 |
| `qtap-plugin-google` | — | — | — | 1.52.0 | — | 2.8.0 | 2.6.2 |
| `qtap-plugin-{deepseek,grok,nanogpt,openai,openai-compatible,z-ai}` | **7.23.0** ×6 | — | — | — | — | 2.8.0 | 2.6.2 |
| `qtap-plugin-openrouter` | — | **1.3.28** | — | — | — | 2.8.0 | 2.6.2 |
| `qtap-plugin-mcp` | — | — | — | — | 1.30.1 | 2.8.0 | 2.6.2 |
| `qtap-plugin-{curl,default-system-prompts,ollama,search-serper}` | — | — | — | — | — | 2.8.0 | 2.6.2 |
| `qtap-plugin-builtin-embeddings` | — | — | — | — | — | 2.8.0 | — |

Every copy of `openai` on disk (8 copies) is 7.23.0, and both copies of `@openrouter/sdk` are 1.3.28.
**`@quilltap/plugin-utils` 2.6.3 is installed nowhere**, because every plugin declares ^2.6.2 and resolves
2.6.2. The two unchanged SDKs are `@anthropic-ai/sdk` 0.115.0 and `@google/genai` 1.52.0, which is what the
guard records.

---

## B. v5's guard + the corpora that carry SDK stamps

### B.1 `crates/quilltap-harness/tests/provider_sdk_version_guard.rs` (P4.106 item 9)

Constants:

```rust
const RECORDED_OPENAI_SDK: &str = "7.20.0";
const RECORDED_ANTHROPIC_SDK: &str = "0.115.0";
const RECORDED_GOOGLE_GENAI_SDK: &str = "1.52.0";
const RECORDED_OPENROUTER_SDK: &str = "1.3.11";
const SDKS: [(&str, &str); 4] = [("openai",…),("@anthropic-ai/sdk",…),("@google/genai",…),("@openrouter/sdk",…)];
```

The locator is `QT_V4_CHECKOUT` (default `$HOME/source/quilltap-server`). If there is no checkout, no root
`node_modules`, or no `plugins/dist/*/node_modules`, the test prints a loud `SKIP:`.

- **Half 1, `every_installed_provider_sdk_matches_the_recorded_version`.** It walks `install_locations()`: the
  root `node_modules`, then each `plugins/dist/*/node_modules` in sorted order. Every `<loc>/<pkg>/package.json`
  that exists must equal its constant, and each SDK must be installed in at least one location. **It does not
  check** `packages/plugin-utils/node_modules`, `@modelcontextprotocol/sdk`, `plugin-types`, `plugin-utils`,
  or the Node version.
  **Run on main `2aed9a552` just now: FAILED** on 9 locations:
  - root `openai` 7.23.0 and root `@openrouter/sdk` 1.3.28;
  - `openai` 7.23.0 under deepseek, grok, nanogpt, openai, openai-compatible and z-ai;
  - `@openrouter/sdk` 1.3.28 under openrouter.
- **Half 2, `the_recorded_corpora_carry_exactly_the_recorded_sdk_versions`.** For every recorded row, it
  `collect()`s each object that has a `headers` map. It reads three stamps:
  - `x-stainless-package-version`, attributed to anthropic when the sibling `url` contains `anthropic.com` and
    to openai otherwise;
  - the OpenRouter version token from a `user-agent` of the form `speakeasy-sdk/typescript <v> … @openrouter/sdk`;
  - the `google-genai-sdk/<v>` token in `x-goog-api-client`.

  `assert_all(corpus, sdk, stamps, recorded, present)` checks two things: every stamp equals the constant, and
  the SDK is **present iff expected** (a presence check, not a count). Expected presence:

  | corpus | openai | anthropic | openrouter | google |
  |---|---|---|---|---|
  | request-envelopes | true | true | true | false |
  | image-dialects | true | false | true | false |
  | google-wire | false | — | — | true |

  **Run: ok.** The corpora still read 7.20.0 and 1.3.11.

Stale prose to fix when the constants move: the "Measured 2026-09-22 from the `f45a517a9` pin" paragraph
(lines ~28-37) records 7.20.0 / 1.3.11 and the per-corpus counts.

### B.2 Stamp census, over every file under `harness/` and `crates/` except `target/`

| file | openai 7.20.0 | OR 1.3.11 | stainless-pkg | speakeasy UA | genai token | runtime-version |
|---|---|---|---|---|---|---|
| `harness/oracle/fixtures/request-envelopes/request-envelopes.recorded.ndjson` (367 rows) | 216 | 14 | 260 (216 × 7.20.0 + 44 × 0.115.0) | 14 | 0 | 260 × `v24.13.1` |
| `harness/oracle/fixtures/image-dialects/image-dialects.recorded.ndjson` (150 rows) | 8 | 3 | 8 | 3 | 0 | 8 × `v24.13.1` |
| `harness/oracle/fixtures/request-envelopes/google-wire.recorded.ndjson` (22 rows) | 0 | 0 | 0 | 0 | 22 (`google-genai-sdk/1.52.0 gl-node/v24.13.1`) | — |
| `provider_sdk_version_guard.rs` | constants + prose only | | | | | |

No other corpus carries an SDK stamp. That includes `google-request`, `streams/*`, `response-bodies`,
`tool-wire`, `moderation-wire` and `web-search-wire`. `google_parts.rs` cites `@google/genai@1.52.0` in prose.

Attribution by provider:

- **request-envelopes:**
  - anthropic: 44 × 0.115.0
  - openai-SDK providers: deepseek 28, grok 20, nanogpt 70, openai 30, openai-compatible 42, z-ai 26 (all 7.20.0)
  - openrouter: 14 speakeasy UAs
  - ollama: no SDK stamp
  - also: 352 × `user-agent: Quilltap/unknown`
- **image-dialects:** openai 4 and z-ai 4 (7.20.0), openrouter 3 speakeasy. Grok's image path records no
  headers row with a stainless stamp.

The Stainless header set is `x-stainless-{arch:arm64, lang:js, os:MacOS, package-version, retry-count:0,
runtime:node, runtime-version:v24.13.1}`, plus `x-stainless-timeout:300` on the anthropic rows only.

**How the families treat stamps:**

- `request_builder_equivalence.rs:243-246, 670-700` compares only a **subset**: the headers v5 models must
  appear in v4's recorded set. `user-agent` and auth are normalized in
  `provider_header_common/mod.rs:114-135`.
- `x-stainless-*` is deliberately not compared.
- The OpenRouter SDK-send path's speakeasy UA is a documented OPENROUTER-only divergence (it asserts that only
  OPENROUTER overrides the UA).
- `image_dialects_equivalence.rs:583` does not compare the UA.

So **the stamps themselves are pinned only by the guard.** A re-record moves the families only if body, url or
header-name bytes move.

### B.3 `openrouter_sdk_pricing_equivalence` (P4.D33)

- Test: `crates/quilltap-harness/tests/openrouter_sdk_pricing_equivalence.rs`. The env var is
  `QT_ORACLE_OPENROUTER_SDK_PRICING`; without it the test prints `SKIP:`.
- Oracle case: `harness/oracle/cases/openrouter-sdk-pricing.test.ts`. It is a jest test with the real SDK in
  the loop, loaded as `require(`${cwd}/node_modules/@openrouter/sdk/esm/index.js`)` behind a `jest.mock`
  un-mapping. That means it reads the **root** 1.3.28 install.
- Recipe (test header lines 21-40):
  - Node `~/.nvm/versions/node/v24.13.1/bin`.
  - `STAGE=/private/tmp/qt-openrouter-sdk-pricing-oracle`.
  - Copy the case into it, then from the checkout run
    `npx jest --silent --watchman=false --roots "$PWD" --roots "$STAGE" --transformIgnorePatterns "node_modules/(?!(@openrouter/sdk|jose)/)" -- openrouter-sdk-pricing`
    with `QT_ORACLE_OUT=/tmp/oracle-openrouter-sdk-pricing.ndjson`.
- **The oracle is not committed.** A "re-record at 1.3.28" is just running it and then the test by name. There
  is nothing to commit unless it goes red.
- The v5 side has **no generator**. The tables are hand-transcriptions, and §A.5 confirms both are identical to
  1.3.28's schema:
  - `OPENROUTER_MODEL_REMAP` (11 pairs), `OPENROUTER_PAGE_LIMIT = 500` and `openrouter_next_page_offset`, all
    in `crates/quilltap-core/src/services/pricing_fetcher/mod.rs:343-400`;
  - `openrouter_sdk_project` (`MODEL_KEYS` 20 / `ARCH_KEYS` 5) in `crates/quilltap-core/src/model/image_dialects.rs:1543-1615`.

  The order could add a tripwire that reads `node_modules/@openrouter/sdk/esm/models/model.js`, but it is not
  required.
- Stale prose: the test header says "`@openrouter/sdk` 1.2.2 in the loop". That has been wrong since at least
  1.3.11; correct it to 1.3.28.

### B.4 MCP

There is no MCP client in v5. `@modelcontextprotocol/sdk` appears only in `apps/web/package-lock.json` at
1.26.0, as the SPA's own transitive dev tooling, which is unrelated. The other mentions are
`qtap-plugin-mcp` as a plugin-config row name:
- `almanack/phase2_machinery.rs:400-403` `collect_mcp_servers`;
- `qtap_export/bundled-plugin-secret-keys.json`;
- the almanack and system-data fixture builders, and `uuid-remap-corpus.json`.

**No family records the MCP SDK version, so 1.30.0 → 1.30.1 has no v5 obligation.**

### B.5 The provider corpora, their recipes, and which lane owns each re-record

Every recorder `cd`s into `$V4/plugins/dist/<plugin>` and imports that plugin's **`.ts` source**
(`provider.ts` / `image-provider.ts` / …). The SDK and `@quilltap/plugin-utils` therefore resolve from that
plugin's own `node_modules`. The header on every regen script reads
`V4=~/source/quilltap-server V5=<repo-root> bash <script>`, "Requires Node 24, run under tsx".

| corpus (committed) | recorder / regen script | consumer tests | stamps? | SDK in loop? | #73 plugin-source exposure | owner |
|---|---|---|---|---|---|---|
| `request-envelopes/request-envelopes.recorded.ndjson` (one file, 9 providers, stream + send) | `record-request-envelopes.mjs` / `regenerate-request-envelopes.sh` | `request_builder_equivalence`, `tool_wire_call_site`, `provider_sdk_version_guard`; mentioned in `request_builder/mod.rs` | **YES** | yes | #73's `provider.ts` hunks are **response-side only** (finish reasons, `response.incomplete`, `blockReason`), so no request-side change | **RIDERS** |
| `request-envelopes/google-wire.recorded.ndjson` | `regenerate-google-wire.sh` (`record-request-envelopes.mjs --provider google`) | `request_builder_google_wire_equivalence`, guard; named in `recipe_sweep.py:1259,1306` | yes (genai 1.52.0, **not moving**) | genai | google `provider.ts` changed on the response side only | neither (optional neutrality check) |
| `request-envelopes/google-request.recorded.ndjson` | `record-google-request.mjs` | `request_builder_google_equivalence` | no | genai (mocked fetch) | response-side only | neither |
| `image-dialects/image-dialects.recorded.ndjson` (one file, 6 providers) | `record-image-fixtures.mjs` / `regenerate-image-fixtures.sh` (openai, google, grok, openrouter, z-ai, nanogpt) | `image_dialects_equivalence`, guard | **YES** | openai + OR SDK | **#73 changes 5 of 6 `image-provider.ts`** (`ModerationRejectionError` mapping: google, grok, openai, openrouter, z-ai). Rows already carry an `isModeration` key. | **SUBSTRATE** (its re-record will stamp 7.23.0 / 1.3.28 whether it wants to or not) |
| `response-bodies/response-bodies.recorded.ndjson` (one file, 10 providers) | `record-response-bodies.mjs` / `regenerate-response-bodies.sh` | `response_parse_equivalence` | no | yes (SDK unwraps) | #73 changes `sendMessage` finish reasons (openai, grok, openrouter, google) | **SUBSTRATE** |
| `streams/<decoder>/<provider>.recorded.ndjson` (**one file per provider**) | `record-stream-fixtures.mjs` / `regenerate-stream-fixtures.sh` | `stream_decoders_equivalence` | no | yes (SDK stream parsers) | #73 moves `responses_api_sse/{openai,grok}` (`response.incomplete`), `chat_completions_sse/openrouter` (finish_reason), `google_parts/google` (`blockReason`) | those 4 → **SUBSTRATE**; `chat_completions_sse/{deepseek,z-ai,openai-compatible,nanogpt}` are openai-SDK driven, untouched by #73 → **RIDERS neutrality**; anthropic and ollama SDKs did not move |
| `tool-wire/tool-wire.recorded.ndjson` | `record-tool-wire.mjs` / `regenerate-tool-wire.sh` | `tool_wire_equivalence` | no | no (pure formatTools / parse) | none | neither |
| `moderation-wire/…` | `record-moderation-wire.mjs` | `moderation_wire_equivalence` | no | **no** (`moderation-provider.ts` uses raw `fetch`) | none | neither |
| `web-search-wire/…` | `record-web-search-wire.mjs` | `web_search_wire_equivalence` | no | no (serper `fetch`) | none | neither |
| embedding wire (jest `cases/embedding-wire.test.ts`) | — | `embedding_wire_equivalence` | no | openai / nanogpt use raw `fetch`; OpenRouter's SDK is mocked | none | neither |
| provider manifests `crates/quilltap-core/src/provider_manifest/manifests/*.json` | `gen-provider-manifests.mjs` (**reads the BUILT `plugins/dist/<dir>/index.js`**, line 208, + `manifest.json`) | `provider_registry_equivalence` | no | bundle metadata | #73 rebuilt 5 bundles; the deps commit rebuilt 11 | RIDERS neutrality check (expect byte-identical: metadata did not move); coordinate with the substrate lane |
| OpenRouter pricing oracle (not committed) | `cases/openrouter-sdk-pricing.test.ts` (jest) | `openrouter_sdk_pricing_equivalence` | no | **yes, 1.3.28** | #73 did not touch `pricing-fetcher.ts` | **RIDERS** |

**The cross-lane coupling that decides the order.** One constant, `RECORDED_OPENAI_SDK` /
`RECORDED_OPENROUTER_SDK`, gates both request-envelopes (riders) and image-dialects (substrate). If riders
re-records request-envelopes and bumps the constants, the guard's image-dialects arms go red until the
substrate lane re-records. If the substrate lane re-records first, image-dialects goes red against the
unbumped constants. Options:

- (a) the riders lane owns the constant bump, and the substrate lane is told its image-dialects re-record
  completes it; the unifier merges both before the gate;
- (b) the riders lane also re-records image-dialects at the substrate's source (wrong: its rows move with #73);
- (c) the constant bump is a unification wire.

`request_builder_equivalence` itself should stay green after the re-record unless body or url bytes moved, since
stamps are excluded (§B.2).

---

## C. `83d0c969b` (bug 170) — ratification evidence

Files, 9 in all:

- `README.md` (badge dev.78 → dev.79);
- the NEW `__tests__/unit/app/salon/hooks/useImpersonation.set-active-speaker.test.tsx` (+55);
- `app/salon/[id]/hooks/useImpersonation.ts` (±1);
- `docs/CHANGELOG.md` (+7), `docs/developer/bugs.md` (+2/−1), and the NEW
  `docs/developer/bugs/fixed/bug-170-speaker-switch-wrong-method.md` (+61);
- `package-lock.json` (dev.72 → dev.79, lock lag), `package.json` and `packages/quilltap/package.json`
  (dev.78 → dev.79).

**The one code hunk is `lib/`-free:** `app/salon/[id]/hooks/useImpersonation.ts` `@@ -150,7 +150,7`,
`method: 'PUT'` → `method: 'POST'`, inside `handleSetActiveSpeaker`'s
`fetch(`/api/v1/chats/${chatId}?action=set-active-speaker`, …)` (line 153 at the commit).

**v5 has no HTTP method to get wrong:**

- `apps/web/src/app/screens/salon/salon-conversation.ts:454` binds `(selectSpeaker)="onSelectSpeaker($event)"`.
  Lines 2583-2592:
  ```ts
  protected async onSelectSpeaker(participantId: string): Promise<void> {
    …
      const data = await this.core.dispatchData({
        type: 'chatSetActiveSpeaker',
        chatId,
        participantId,
      });
  ```
- `apps/web/src/app/core/core-client.ts:63`: "{@link dispatch} → the typed action route (`POST /api/dispatch`
  / the `dispatch` command)". `core-transport.ts:81`: "Send one request to `POST /api/dispatch`". Inside
  Tauri, this is IPC with no method at all.
- `apps/web/src/app/core/core-contract.ts:185-186` declares
  `interface ChatSetActiveSpeakerRequest { type: 'chatSetActiveSpeaker'; … }`.
- `crates/quilltap-core/src/api/types.rs:351-356`:
  `/// Set the active typing/speaking participant (v4 \`POST …?action=set-active-speaker\`).`
  `ChatSetActiveSpeaker { chat_id: String, participant_id: String }`. It is dispatched at
  `api/engine.rs:1924` to `api/salon.rs:1924-1925`
  `pub async fn chat_set_active_speaker(db: &Db, chat_id: &str, participant_id: &str) -> Response`.
- The REST edge serves it on POST exactly as v4 does: `crates/quilltap-web/src/wardrobe_routes.rs:419`
  `const CHAT_POST_ACTIONS` contains `"set-active-speaker"` (line 425). No PUT list does.

**Mirror delta.** `diff -rq docs/v4/developer/bugs <v4 08c49319d docs/developer/bugs>` reports exactly one
missing file, `fixed/bug-170-speaker-switch-wrong-method.md`; v5 has 169 files in `fixed/`. `bugs.md` differs in
exactly two hunks:

- line 8, the Status sentence "1–169" → "1–170" plus the bug-170 sentence;
- line 1160, the new table row (`| 170 | [switching the Salon's "speaking as" seat sends the wrong HTTP method]… | Medium | … | Unchecked |`).

The only v4 commit touching `docs/developer/bugs*` in `b0b6656b5..08c49319d` is `83d0c969b`. The bug file's
own row reads `| **v5 status** | Unchecked. The v5 client should be checked for the same method on
set-active-speaker |`. The answer is **not affected**, for the reasons above. A candidate upstream note is to
flip it to "Not affected (dispatch, no HTTP method)", but that belongs to the human.

## D. `a8292547a` — ratification evidence

Nine files, +1,008, **all docs or tooling prose**:

- `.claude/commands/update-documentation.md` (+6 index lines);
- `docs/CHANGELOG.md` (+7, "#### Docs: Concierge overhaul specs");
- `docs/developer/features/ROADMAP.md` (+1);
- `docs/developer/features/concierge-overhaul.md` (+71);
- `docs/developer/features/concierge-overhaul-phase-1-refusal-failover.md` (+283);
- `docs/developer/features/concierge-overhaul-phase-2-refusal-ledger.md` (+159);
- `docs/developer/features/concierge-overhaul-phase-3-three-states.md` (+179);
- `docs/developer/features/concierge-overhaul-phase-4-concierge-tab.md` (+206);
- `docs/developer/features/concierge-overhaul-phase-5-salon-polish.md` (+96).

There is no `lib/`, `app/`, `help/`, `packages/`, DDL or version-stamp hunk.

**Features mirror** (`diff -rq docs/v4/developer/features` against v4 `08c49319d`). **Six files are only in
v4:** the six overhaul specs. **Seven files differ:**

| file | last v4 commits since `b0b6656b5` |
|---|---|
| `ROADMAP.md` (1 line: `- [ ] Concierge overhaul: … see [concierge-overhaul.md]`) | `a8292547a` |
| `complete/concierge-default-at-creation.md`, `complete/concierge-four-state.md`, `complete/concierge-list-marks.md` | `4d370a90f` (#75) |
| `complete/dangerous.md` | `3b463d6b1` (#76) |
| `complete/message-route-trail.md` | `8bd080267` (#73) |
| `scenario-builder.md` | `08c49319d` |

v5 also has an untracked `docs/v4/developer/features/.DS_Store`, which is not in git and harmless.

The overhaul specs were edited after `a8292547a`:

- phase-1 and phase-2 by `8bd080267` and `49059fb14`;
- phase-3 by `4d370a90f`;
- phase-4 by `4d370a90f` and `3b463d6b1`;
- phase-5 by `ce2f1dabf`;
- the overview by `49059fb14`, `4d370a90f`, `3b463d6b1` and `ce2f1dabf`.

**So "mirror at HEAD" and "mirror at `a8292547a`" are different bytes.** The ledger (line ~164-167) says the
refresh takes them "as later edited by #73–#77" and rides the catch-up's unification. A riders lane that
refreshes the mirror should copy at whatever pin the round adopts, not at `a8292547a`.

**CHANGELOG mirror.** `docs/v4/CHANGELOG.md` was last refreshed at `a013ae093` (the `89fcc3c0d` round). Its top
entry is "Fixed: document tools listed character vaults … (bug 153)". Everything from that entry down is
byte-identical to v4 `08c49319d`: I diffed the tail starting at v4 line 978 against the mirror from line 7, and
the diff was empty. **v4 has 48 `####` entries above it, all under `### 4.10-dev`.** The three in scope are:

- line 26, "Changed: dependency update across the app, packages and plugins" (`6d0f88d65`);
- line 320, "Docs: Concierge overhaul specs" (`a8292547a`);
- line 327, "Fixed: switching the Salon's 'speaking as' seat raised 'Unknown action' (bug 170)".

The ledger already names this lag as a separate housekeeping item.

## E. Tier R

- **Test:** `crates/quilltap-cli/tests/cli_differential.rs`. It is env-gated on `QT_V4_CHECKOUT`; with no
  checkout it prints `SKIP: CLI differential: …`.
- **Recipe** (header lines 1-18):
  ```
  QT_V4_CHECKOUT=~/source/quilltap-server QT_NODE=$N/node \
    cargo test -p quilltap-cli --test cli_differential
  ```
  Here `$N` is Node 24 (`~/.nvm/versions/node/v24.13.1/bin`). The oracle binary is
  `$QT_V4_CHECKOUT/packages/quilltap/bin/quilltap.js` (line 1818). It also reads
  `packages/quilltap/lib/instances.js` (line 4325-4326).
- **Oracle's imports.** `bin/quilltap.js` requires only `../lib/*` (lock-helpers, completion, docs,
  file-verify, instances, db-commands, logs, maintenance, memories, memory-diff, migrations, recall-replay,
  sync, theme, download-manager, native-modules, db-helpers) and Node builtins. `lib/db-helpers.js` requires
  `better-sqlite3` / `better-sqlite3-multiple-ciphers`.
- **Intersection with the two file lists.** Neither `6d0f88d65` nor `08c49319d` touches
  `packages/quilltap/{bin,lib}/`. Neither moves `better-sqlite3` in the lock. Both bump only
  `packages/quilltap/package.json`'s `version`. That value is read solely by `getVersion()` (bin line 22-24),
  and it is printed only on `-v/--version` (line 321-325).
- **Tier R has no `--version` case.** Its help cases are `--help`, `db --help`, `docs --help`,
  `instances --help` and `completion --help`, and `printHelp()` prints no version string. **Tier R is confirmed
  unaffected.** The same goes for `83d0c969b` (the same version-only bump). Re-running at HEAD should stay 266/0.

## F. Traps + open questions

1. **Regens do not depend on the pin, but they do depend on the checkout.** Every recorder resolves the SDK from
   the plugin dir's `node_modules` (or the root, for jest), and those are shared by every pin. A pinned worktree
   symlinks them from the live checkout. A regen at `b0b6656b5`, at `08c49319d` or anywhere between stamps
   7.23.0 / 1.3.28 today. Corollaries:
   - a "pre-bump" corpus can no longer be produced;
   - a substrate-lane regen of image-dialects will carry the new stamps even if that lane never mentions the
     SDK.
2. **The Node stamp is a separate trap, and the guard does not cover it.** Three places record
   `x-stainless-runtime-version: v24.13.1`: request-envelopes ×260, image-dialects ×8, and google-wire ×22 (as
   `gl-node/v24.13.1`). The regen scripts call bare `npx tsx`, so the stamp is whatever `node` is first on PATH.
   - In this environment `/usr/local/bin/node` is v24.13.1, which is correct.
   - nvm also has v24.18.0 and v24.19.0, and Homebrew has v26.8.1 (`oracle-node-abi-gotcha`).
   - A regen under any of those churns 290 lines. The value comparisons exclude those lines, so no test catches
     it, but the corpus diff becomes noise that hides real moves.

   The order should pin `PATH=$HOME/.nvm/versions/node/v24.13.1/bin:$PATH` for every regen, and should require
   `grep -c v24.13.1` to stay equal before and after. It may add a `RECORDED_NODE` arm to the guard (that would
   be an order decision, not required).
3. The guard's installed half is **red on main now**, which is a standing red for every `--workspace` gate with
   the checkout present until the constants move. The corpora half flips red the moment either lane re-records
   without the bump (§B.5 coupling).
4. **Expected re-record diff in request-envelopes:**
   - stamp-only: 216 × `7.20.0`→`7.23.0` and 14 × `1.3.11`→`1.3.28`;
   - anthropic 0.115.0 and ollama unchanged.

   Anything else, such as a body key, a header name or `x-stainless-timeout`, is a real SDK move to
   investigate. The prime suspects are OpenRouter 1.3.11→1.3.28 (16 SDK releases, never audited) and openai
   7.21's options-promise wrapper.
5. **Neutrality set for the riders lane:**
   - re-run `chat_completions_sse` for deepseek, z-ai, openai-compatible and nanogpt (expect byte-identical);
   - re-run the OpenRouter pricing oracle and the test by name;
   - re-run `gen-provider-manifests.mjs` (expect byte-identical manifests; it reads the rebuilt bundles);
   - optionally run google-wire (genai did not move; expect identical).

   Do not touch the four #73-moved stream files, image-dialects or response-bodies. Those are the substrate
   lane's, and response-bodies / image-dialects are single concatenated files that cannot be split per
   provider.
6. The memory note `a-pinned-regen-cannot-prove-an-sdk-bump-the-plugin-dirs-never-installed` still applies,
   but here the plugin dirs are installed at the new versions (§A.6). The one gap is that
   `@quilltap/plugin-utils` **2.6.3 is installed nowhere** (2.6.2 everywhere). Its only change is the version
   constant, so this is harmless and needs no `npm install`.
7. **Stale prose to correct in the lane:**
   - the guard's "Measured 2026-09-22" paragraph;
   - the `openrouter_sdk_pricing_equivalence.rs` header saying "1.2.2";
   - `capture-markdown-fixtures.mts:28-30`, which says "0.18.4 since v4's `d339bad8`"; v4 is at 0.18.9.
8. **KaTeX (open question for the order).** Bump `apps/web` `katex` 0.18.4 → 0.18.9 and recapture the markdown
   fixtures (expect math bytes identical, since the nested 0.16.47 is unchanged), or record it as a named
   deferral. The lag predates this commit (`6b0615807`). **Do not run `npm install` in either repo without the
   human.** A v5 `apps/web` pin bump means `npm ci` in v5, and the lane can do that with approval.
9. The `yjs` / `lib0` / `next` moves have no v5 comparand. A grep found no oracle case over them.
10. **Tier R is safe at HEAD.** The only oracle-read file either commit touches is
    `packages/quilltap/package.json`'s `version`, and no case reads it.
11. **Ledger accuracy check.** The ledger's `83d0c969b` row says "the `4.10.0-dev.79` stamp", which is correct.
    The `6d0f88d65` row lists the root and plugin moves correctly. It does not say that #73 already carried
    grok, openai and z-ai to 7.23.0 and openrouter to 1.3.27, or that `plugin-utils` 2.6.3 is not installed.
    Worth one sentence in the order.
