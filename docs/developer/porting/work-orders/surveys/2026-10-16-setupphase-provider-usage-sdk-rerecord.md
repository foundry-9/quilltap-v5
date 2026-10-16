# Survey — PROVIDER LANE: the openai-compatible usage normalisation (`a8b2d2861`) and the dependency safe batch's SDK / zod moves (`eb5d42338`)

**Date:** 2026-10-16. **v4 pin:** `25b8fab6aa58636c6885ba63684c7f8fbfca9c3b` (`25b8fab6a`, `4.11.0-dev.10`, checkout
clean, HEAD == pin at survey time). **Oracle baseline (what v5 ported):** `83334b9a5`. **v5 `main`:** `56554d98c`.
Read-only survey for the next drift catch-up round's PROVIDER lane. Every v4 line number is AT THE PIN
(`git show 25b8fab6a:<path>`) unless marked `(a8b2d)` / `(eb5d4)` = the commit's own diff. Every claim is derived from
shipped hunks and from files read at the pin / in the live checkout (HEAD == pin), never from commit prose. v5 file:line
is at `56554d98c`. Nothing was built, run or edited besides this file; "predicted" marks every outcome I could not
execute (the lane must measure it).

**Slice:** (1) `a8b2d2861`'s one source hunk — `OpenAICompatibleProvider.normalizeUsage` in
`packages/plugin-utils/src/providers/openai-compatible.ts` — and everything that carries it (the three re-bundled
plugin `index.js`, the four jest cases, the plugin manifests / versions); (2) `eb5d42338`'s `openai` 7.30.0 → 7.34.0
(root lock + the plugin bundles), `zod` 4.6.5 → 4.6.7 and the rest of the lockfile; (3) the v5 provider layer that
consumes both — decoders, response parse, usage/cache plumbing into `llm_logs`, the enclave budget, the SPA display;
(4) every recorder / corpus / guard that resolves an SDK, and the stamp arithmetic of a re-record.
**Not this slice:** the Lexical 0.52 move (`b2793400f`, v4's client only — NO-PORT), `katex`, test/type deps, the
Pascal trigger-phrase / wardrobe-effects commits (their own lanes).

---

## 0. The span for this slice, in one paragraph

Two commits, 22 commits past the baseline overall (the rest are other lanes). **`a8b2d2861`** ("Release checklist 8",
2026-10-13, `4.10.0-dev.180`) changes ONE behavioural file: the base class `OpenAICompatibleProvider` in
`packages/plugin-utils` gains `protected normalizeUsage()`, and BOTH `sendMessage` and `streamMessage` route their
Chat Completions `usage` block through it. Quilltap's house rule — `promptTokens` / `totalTokens` EXCLUDE prompt-cache
reads — thereby reaches the one plugin that inherits the base send/stream loops unchanged: **`openai-compatible`**
(DeepSeek and NanoGPT override both methods and already did it their own way; they were rebuilt only because their
bundles inline the base class). The change has four v5-visible effects on OPENAI_COMPATIBLE alone: (a) the
non-streaming `usage` drops by `prompt_tokens_details.cached_tokens` and a `cacheUsage` appears; (b) the streaming
final chunk does the same; (c) the streaming final chunk gains `rawProviderUsage` (the whole `usage` object) whenever a
usage frame arrived; (d) a new DEBUG line fires when cached > 0. **`eb5d42338`** ("dependency safe batch",
2026-10-15, `4.11.0-dev.8`) moves `openai` 7.30.0 → **7.34.0** and `zod` 4.6.5 → **4.6.7** in the ROOT lock and
re-bundles **EIGHT** of the fifteen `plugins/dist/*/index.js` (the ledger says ten — §9) with the new root SDK; it
touches no `lib/` / `app/` file. Two measurements decide the lane's size: (1) **no recorder resolves the root
`openai`** — the six OpenAI-SDK plugin dirs keep their own `openai` **7.23.0** (untracked `node_modules`, untracked
lockfiles), the other nine never build a recorded request through it — so **no recorded stamp moves**; the guard needs
ONE constant moved (`INSTALLED_ROOT_OPENAI_SDK` 7.30.0 → 7.34.0), not a re-stamp of the corpora; (2) **v4 never
calls any SDK accumulator** (`runTools`, `chat.completions.stream`, `responses.stream`, `.totalUsage()` — zero hits
in `lib/ app/ packages/ plugins/dist/*/*.ts`), and v5's decoders read the last usage frame themselves, so SDK
7.34.0's `*_tokens_details` summing is **unreachable on both sides — there is NO usage-count convergence candidate
from the SDK** (§2.9 names the v5 lines). The genuinely new facts the ledger did not have: the live plugin dirs are
**already** at plugin-utils 2.7.1 (so every recorder run today replays the new behaviour against corpora recorded
under 2.6.3 — the OAC stream corpus is stale NOW), `provider_sdk_version_guard` and **`zod_version_guard` are both RED
on the live checkout right now**, and zod 4.6.7 carries one real behaviour change (an oversized array / map / set
now fails its size check BEFORE any element is parsed) that touches four hand-written v5 Zod twins (§1.9, §3 item 11).

---

## 1. v4 AT THE PIN — hunk by hunk

### 1.1 `packages/plugin-utils/src/providers/openai-compatible.ts` (a8b2d, +58 / −16) — `normalizeUsage`

Class header `:124` (`export class OpenAICompatibleProvider implements TextProvider`), `providerName` `:135`
(set `:180` / `:188` — the string constructor leaves `'OpenAICompatible'`), `logger` `:145`, assigned `:197` as
`createPluginLogger(`${this.providerName}Provider`)` — so the logger's plugin prefix, and the capture's `plugin`
field, is **`OpenAICompatibleProvider`** (matching the recorded `pluginErrorLog` rows).

The new method, verbatim, `:432-463`:

```ts
  /**
   * Normalizes a Chat Completions `usage` block into Quilltap's shape.
   *
   * OpenAI-compatible endpoints fold prompt-cache hits into `prompt_tokens`
   * (reported separately as `prompt_tokens_details.cached_tokens`). Quilltap's
   * convention is that `promptTokens` / `totalTokens` EXCLUDE cache reads, so
   * cached input never counts against token budgets; `cacheUsage` still
   * carries the cache reads for display. Endpoints that report no
   * `cached_tokens` are unaffected.
   */
  protected normalizeUsage(
    usage: OpenAI.Completions.CompletionUsage | null | undefined
  ): Pick<LLMResponse, 'usage' | 'cacheUsage'> {
    const cachedTokens = usage?.prompt_tokens_details?.cached_tokens ?? 0;
    if (cachedTokens > 0) {
      this.logger.debug(`${this.providerName} usage: excluding prompt-cache hits from prompt/total`, {
        context: `${this.providerName}Provider.normalizeUsage`,
        promptTokens: usage?.prompt_tokens ?? 0,
        cachedTokens,
      });
    }
    return {
      usage: {
        promptTokens: Math.max(0, (usage?.prompt_tokens ?? 0) - cachedTokens),
        completionTokens: usage?.completion_tokens ?? 0,
        totalTokens: Math.max(0, (usage?.total_tokens ?? 0) - cachedTokens),
      },
      ...(cachedTokens > 0
        ? { cacheUsage: { cacheReadInputTokens: cachedTokens, cachedTokens } }
        : {}),
    };
  }
```

**Reading it exactly (the port's contract):**

1. `cachedTokens` is the RAW `prompt_tokens_details.cached_tokens ?? 0` — `null`, `undefined`, absent `prompt_tokens_details`,
   absent `usage` all give `0`. It is **subtracted raw**: a negative `cached_tokens` (`-5`) is NOT `> 0`, so no
   `cacheUsage` and no DEBUG line — but `Math.max(0, prompt − (−5))` still ADDS 5 to `promptTokens` and `totalTokens`.
   (v5's other flavors pre-filter `> 0` before subtracting — `chat_completions_sse.rs:331-336` `cached_from_details`,
   `response_parse.rs:428-433` — so reusing them for OAC would silently diverge on a non-positive value; §3 item 3.)
2. `prompt_tokens`, `completion_tokens`, `total_tokens` each `?? 0`; only `prompt` and `total` are floored with `Math.max(0, …)`.
   `completionTokens` is never floored.
3. `total_tokens` is reduced by the same `cachedTokens` independently of `prompt_tokens` — so `total` can reach `0`
   while `prompt` does not (e.g. `total_tokens` absent → `0 − 800` → `0`).
4. `cacheUsage` = `{ cacheReadInputTokens: c, cachedTokens: c }` ONLY when `c > 0`; key order is
   `cacheReadInputTokens` THEN `cachedTokens`. When `c <= 0` the key is ABSENT (the spread of `{}`), not `undefined`.
5. The DEBUG line fires only when `c > 0`, ONCE per `normalizeUsage` call, and `promptTokens` in its context is the
   **raw** `usage.prompt_tokens ?? 0` (before subtraction), then `cachedTokens`. Message =
   `OpenAICompatible usage: excluding prompt-cache hits from prompt/total` (the string provider name; a programmatic
   `providerName` config would change it — the plugin passes none, `provider.ts` constructs the endpoint subclass with
   a bare URL string, see §1.4). `context` field = `OpenAICompatibleProvider.normalizeUsage`.
6. Typing nit: `Pick<LLMResponse,'usage'|'cacheUsage'>` — `usage` is therefore ALWAYS present in the returned object.

**Call site 1 — `sendMessage`** (`:472-503`), the changed lines, before (`83334b9a5`) → after (pin `:490`):

```ts
// BEFORE
        usage: {
          promptTokens: response.usage?.prompt_tokens ?? 0,
          completionTokens: response.usage?.completion_tokens ?? 0,
          totalTokens: response.usage?.total_tokens ?? 0,
        },
// AFTER  (:490)
        ...this.normalizeUsage(response.usage),
```

Key order of the returned `LLMResponse` is unchanged (`content, finishReason, usage, [cacheUsage], raw,
attachmentResults, [toolCalls]`); `usage` stays always-present (all-zero on a missing block), `cacheUsage` is new and
sits directly after `usage`.

**Call site 2 — `streamMessage`** (`:512-620`):

```ts
// BEFORE  (:527 area)
      let accumulatedUsage: { prompt_tokens?: number; completion_tokens?: number; total_tokens?: number } | null = null;
      ...
        if (hasUsage) {
          accumulatedUsage = {
            prompt_tokens: chunk.usage?.prompt_tokens,
            completion_tokens: chunk.usage?.completion_tokens,
            total_tokens: chunk.usage?.total_tokens,
          };
        }
      ...
      yield {
        content: '',
        done: true,
        usage: accumulatedUsage ? {
          promptTokens: accumulatedUsage.prompt_tokens ?? 0,
          completionTokens: accumulatedUsage.completion_tokens ?? 0,
          totalTokens: accumulatedUsage.total_tokens ?? 0,
        } : undefined,
        attachmentResults,
        ...

// AFTER  (:528, :539-545, :577-586)
      let accumulatedUsage: OpenAI.Completions.CompletionUsage | null = null;
      ...
        const hasUsage = chunk.usage;
        // Track usage when we get it (may come in a separate final chunk)
        if (hasUsage) {
          accumulatedUsage = chunk.usage ?? null;
        }
      ...
      yield {
        content: '',
        done: true,
        ...(accumulatedUsage
          ? {
              ...this.normalizeUsage(accumulatedUsage),
              rawProviderUsage: accumulatedUsage as unknown as Record<string, unknown>,
            }
          : {}),
        attachmentResults,
        // Only synthesized when the stream actually carried tool calls: ...
        ...(toolCalls.length > 0 ? { toolCalls, rawResponse: { choices: [ ... ] } } : {}),
      };
```

**Stream semantics (all unchanged except the three marked NEW):**

- `hasUsage = chunk.usage` is a JS TRUTHY test: `usage: {}` is truthy (→ zero usage + `rawProviderUsage: {}`), `usage: null` / absent is not. The LAST truthy frame wins (no summing — §1.7).
- NEW: the final chunk carries `rawProviderUsage` = the WHOLE last usage object (all provider keys incl.
  `prompt_tokens_details`, `completion_tokens_details`, unknown vendor keys) ONLY when a usage frame arrived. With no
  usage frame the key is ABSENT (contrast NanoGPT, whose override writes `(usage ?? null)` — an explicit `null`; and
  z-ai, same habit). The final-chunk `usage` key is likewise ABSENT with no frame (the old `usage: undefined` also
  vanished at `JSON.stringify`, so the serialised shape is unchanged for that case).
- NEW: `cacheUsage` rides the final chunk (after `usage`, before `rawProviderUsage`'s position is irrelevant — JSON
  order only; the harness compares parsed Values).
- The `toolCalls` / synthesized `rawResponse` arm is untouched: the `rawResponse` literal still has **no `usage` key**.
- `stream_options: { include_usage: true }` is still sent (`:420`); no wire change.

### 1.2 The re-bundled plugins — `plugins/dist/qtap-plugin-{openai-compatible,deepseek,nanogpt}/index.js` (a8b2d, +49 / −15 each)

The three diffs are **line-for-line identical** (measured: `diff` of the `+`/`-` bodies = empty) — the inlined base
class, esbuild output of the hunk in §1.1. The openai-compatible one, the interesting lines:

```js
@@ -21769,6 +21769,34 @@ var OpenAICompatibleProvider = class {
+  normalizeUsage(usage) {
+    const cachedTokens = usage?.prompt_tokens_details?.cached_tokens ?? 0;
+    if (cachedTokens > 0) {
+      this.logger.debug(`${this.providerName} usage: excluding prompt-cache hits from prompt/total`, {
+        context: `${this.providerName}Provider.normalizeUsage`,
+        promptTokens: usage?.prompt_tokens ?? 0,
+        cachedTokens
+      });
+    }
+    return {
+      usage: { promptTokens: Math.max(0, (usage?.prompt_tokens ?? 0) - cachedTokens), completionTokens: usage?.completion_tokens ?? 0, totalTokens: Math.max(0, (usage?.total_tokens ?? 0) - cachedTokens) },
+      ...cachedTokens > 0 ? { cacheUsage: { cacheReadInputTokens: cachedTokens, cachedTokens } } : {}
+    };
+  }
@@ -21791,11 +21819,7 @@   ...this.normalizeUsage(response.usage),
@@ -21836,11 +21860,7 @@  accumulatedUsage = chunk.usage ?? null;
@@ -21868,11 +21888,10 @@ ...accumulatedUsage ? { ...this.normalizeUsage(accumulatedUsage), rawProviderUsage: accumulatedUsage } : {},
```

Manifests / versions (a8b2d): openai-compatible `1.0.48 → 1.0.49`, deepseek `1.0.27 → 1.0.28`, nanogpt `1.2.9 →
1.2.10` (`manifest.json` + `package.json`). `@quilltap/plugin-utils` `^2.6.2 → ^2.7.1` in the root `package.json` and in
**14** plugin `package.json`s (NOT 15 — `qtap-plugin-builtin-embeddings` declares no plugin-utils; §9). Root
`package-lock.json`: `node_modules/@quilltap/plugin-utils` `2.6.3 → 2.7.1` (`plugin-utils-2.7.1.tgz` from the
registry); `packages/plugin-utils/package.json` + `version.generated.ts` `PLUGIN_UTILS_VERSION = '2.7.1'`
(`2.7.0` never appeared on the registry; `2.7.1` is the version-only republish — `packages/plugin-utils/CHANGELOG.md`
`[2.7.1]` / `[2.7.0]`).

**Only three bundles carry the base class** (measured at the pin, `grep -c normalizeUsage plugins/dist/*/index.js`):
openai-compatible 4, deepseek 4, nanogpt 4; every other bundle 0. Z.AI, OpenAI, Grok, OpenRouter, Ollama, Anthropic,
Google do NOT extend `OpenAICompatibleProvider` — **no other v5 provider flavor is touched.**

**The override evidence — DeepSeek and NanoGPT keep their own usage path (so behaviour moves for openai-compatible
ALONE).** Neither calls `normalizeUsage`; each overrides BOTH send paths and computes the subtraction from its own
`extractCacheUsage`:

```ts
// plugins/dist/qtap-plugin-deepseek/provider.ts  (pin)
:202  private extractCacheUsage(usage: DeepSeekUsage | null | undefined) {
:203    const hit = usage?.prompt_cache_hit_tokens;
:204    if (hit === undefined || hit <= 0) return undefined;
:205    return { cacheReadInputTokens: hit, cachedTokens: hit };
:208  override async sendMessage(...)   // usage: promptTokens: Math.max(0, prompt_tokens - (cacheUsage?.cacheReadInputTokens ?? 0)) :288-290; ...(cacheUsage ? { cacheUsage } : {}) :296
:308  override async *streamMessage(...) // same subtraction :429-431; ...(cacheUsage ? { cacheUsage } : {}) :437; NO rawProviderUsage

// plugins/dist/qtap-plugin-nanogpt/provider.ts  (pin)
:291  private extractCacheUsage(usage: NanoGPTUsage | null | undefined) {
:293    const read = usage.cache_read_input_tokens ?? usage.prompt_tokens_details?.cached_tokens ?? 0;
:302  override async sendMessage(...)   // cacheRead subtraction :331-336; cacheUsage :340
:354  override async *streamMessage(...) // subtraction :464-467; rawProviderUsage: (usage ?? null) :470; cacheUsage :471
```

and `plugins/dist/qtap-plugin-openai-compatible/provider.ts:67` is `class OpenAICompatibleEndpointProvider extends
OpenAICompatibleProvider` with `grep -n "sendMessage\|streamMessage"` → **zero** overriding methods (it overrides
`profileParamAllowlist`, `acceptsRepeatedSystemMessages`, `normalizeProfileParam` only). Confirmed.

### 1.3 `__tests__/unit/plugins/local-provider-params.test.ts` (a8b2d, +59) — the four cases, verbatim

The suite imports the base class from SOURCE (`@/packages/plugin-utils/src/providers/openai-compatible`, header
comment "exercised at its source rather than through the published copy") and mocks the SDK with
`jest.mock('openai', …)` (`:30-39` — a constructor returning `{ chat: { completions: { create } }, models: { list } }`;
`getCreateMock()` reaches the shared `create`). Fixtures at the pin: `okCompletion = { choices: [{ message: { content:
'hi' }, finish_reason: 'stop' }], usage: { prompt_tokens: 1, completion_tokens: 1, total_tokens: 2 } }` (`:42-45`),
`baseParams = { model: 'qwen3:8b', messages: [{ role: 'user', content: 'hi' }] }`. Shared input
(`:299-306`): `cachedUsage = { prompt_tokens: 1000, completion_tokens: 50, total_tokens: 1050, prompt_tokens_details: {
cached_tokens: 800 } }`. Provider = `new OpenAICompatibleProvider('http://localhost:8080/v1')`, key `''`.

| # | name (verbatim) | input | asserted output |
|---|---|---|---|
| 1 | `excludes cache hits from a non-streaming response and reports them in cacheUsage` | `create` → `{ ...okCompletion, usage: cachedUsage }`; `sendMessage` | `usage` `{ promptTokens: 200, completionTokens: 50, totalTokens: 250 }`; `cacheUsage` `{ cacheReadInputTokens: 800, cachedTokens: 800 }` |
| 2 | `excludes cache hits from the final stream chunk and keeps the raw block` | `create` → async iterable yielding `{ choices: [{ delta: { content: 'hi' } }] }` then `{ choices: [], usage: cachedUsage }`; `streamMessage`, take the `done` chunk | `final.usage` `{ 200, 50, 250 }`; `final.cacheUsage` `{ 800, 800 }`; `final.rawProviderUsage` `toEqual(cachedUsage)` |
| 3 | `leaves usage untouched, with no cacheUsage, when the endpoint reports no cache hits` | `create` → `okCompletion`; `sendMessage` | `usage` `{ promptTokens: 1, completionTokens: 1, totalTokens: 2 }`; `expect(response).not.toHaveProperty('cacheUsage')` |
| 4 | `never reports negative tokens when cached_tokens exceeds prompt_tokens` | `create` → `{ ...okCompletion, usage: { prompt_tokens: 10, completion_tokens: 0, total_tokens: 10, prompt_tokens_details: { cached_tokens: 20 } } }`; `sendMessage` | `usage` `{ promptTokens: 0, completionTokens: 0, totalTokens: 0 }` (the test does NOT assert `cacheUsage`; by the code it IS present: `{ cacheReadInputTokens: 20, cachedTokens: 20 }`) |

The sibling tests in the same `describe` (`:279` "reports no tool calls, and no rawResponse, on an ordinary stream")
already pin that the stream's `rawResponse` / `toolCalls` are absent on an ordinary turn — unchanged.

### 1.4 What the plugin passes the base class (for the recorder instantiations)

`record-stream-fixtures.mjs` builds `new OpenAICompatibleEndpointProvider('http://localhost:8080/v1')` (the subclass);
`record-response-bodies.mjs:57-60` builds `new OpenAICompatibleProvider({ baseUrl: 'http://localhost:8080/v1' })`
imported from the plugin's `provider.ts` (the RE-EXPORTED base, `provider.ts:17`, with an object config — fetch is mocked
under the SDK so the base URL is moot, but the two recorders do not instantiate the same class). `providerName` is
`'OpenAICompatible'` for both (string constructor `:180`; object config with no `providerName` `:188`).

### 1.5 The DEBUG line — verbatim, fields, level

| property | value |
|---|---|
| level | DEBUG (`this.logger.debug`) |
| message | `OpenAICompatible usage: excluding prompt-cache hits from prompt/total` |
| plugin prefix | `OpenAICompatibleProvider` (`createPluginLogger('OpenAICompatibleProvider')`) |
| context, in key order | `context: "OpenAICompatibleProvider.normalizeUsage"`, `promptTokens: <raw usage.prompt_tokens ?? 0>`, `cachedTokens: <c>` |
| fires | once per call, only when `c > 0`; not in DeepSeek / NanoGPT |

The recorders' `captureLogger` (`record-stream-fixtures.mjs:211-239`) forwards non-error levels straight to `console.*`
and captures ERROR only (`pluginErrorSink`) — a DEBUG capture needs a new sink (§4.7).

### 1.6 Lockfile moves — `eb5d42338` (root `package-lock.json`, `4.11.0-dev.6 → 4.11.0-dev.8`)

Measured with `jq` over `eb5d42338^:package-lock.json` vs `eb5d42338:package-lock.json`, every top-level
`node_modules/*` whose `version` moved:

| package | old → new | v5 relevance |
|---|---|---|
| **`openai`** | **7.30.0 → 7.34.0** | the SDK; §1.7 / §1.8 |
| **`zod`** | **4.6.5 → 4.6.7** | §1.9; `zod_version_guard` |
| `katex` | 0.18.10 → 0.19.0 | SPA is ProseMirror — none |
| `next` / `@next/*` / `eslint-config-next` | 16.4.0 → 16.4.2 | v4 client — none |
| `react` / `react-dom` / `@jest/react-is-19` | 19.3.0 → 19.3.1 | none |
| `@tanstack/{react-query,query-core,query-devtools,react-query-devtools,eslint-plugin-query}` | 5.104.1 → 5.104.3 | none |
| `@tanstack/react-virtual` / `virtual-core` | 3.14.13 → 3.14.20 / 3.17.11 → 3.19.5 | none |
| `@playwright/test` / `playwright` / `playwright-core` | 1.63.0 → 1.64.1 | v4's e2e — none (v5's Playwright is `apps/web`) |
| `@testing-library/jest-dom` | 6.9.1 → 7.0.1 | v4 tests |
| `@testing-library/user-event` | 14.6.7 → 14.6.8 | v4 tests |
| `@types/better-sqlite3` | 7.6.13 → 9.6.0; `@types/yauzl` 2.10.3 → 3.4.0; `@types/node` 24.19.1 → 24.19.2 | types only |
| `@typescript-eslint/*`, `typescript-eslint` | 8.71.1 → 8.71.2 | lint |
| `dotenv` | 17.4.2 → 18.0.8 | v4 tooling |
| `@internationalized/number` 3.6.8 → 3.6.9, `baseline-browser-mapping`, `bser`, `caniuse-lite`, `electron-to-chromium`, `fb-watchman`, `node-abi` 3.97.0 → 3.99.0, `node-releases`, `postcss` 8.5.29 → 8.5.30, `react-syntax-highlighter` 16.1.1 → 16.1.2, `seroval(-plugins)` 1.6.8 → 1.6.9, `update-browserslist-db` | patch | none |
| REMOVED: `@vitest/mocker`, `@vitest/spy`, `estree-walker` | 4.1.11 / 4.1.11 / 3.0.3 → absent | none |

Unmoved (checked): `@quilltap/plugin-utils` stays 2.7.1, `@quilltap/plugin-types` 2.8.0, `@openrouter/sdk` 1.4.25,
`sharp` 0.35.5, `pdf-parse` 2.4.5, `jsonrepair` 3.15.0. (Against the BASELINE `83334b9a5` the span also moves
`@quilltap/plugin-utils` 2.6.3 → 2.7.1 [a8b2d] and `lexical` 0.43.0 → 0.52.0 [`b2793400f`]; 92 top-level versions differ
baseline → pin in total.) Root `package.json` ranges: `openai ^7.23.0` (unchanged), `zod ^4.6.5` (unchanged),
`@quilltap/plugin-utils ^2.7.1`. `node_modules/openai` carries `peerDependencies.zod "^3.25 || ^4.0"` and
`@quilltap/plugin-utils` carries `peerDependencies.openai "^4.0.0 || ^5.0.0 || ^6.0.0 || ^7.0.0"` — so the base class
loads WHICHEVER `openai` its install location resolves (§2.6).

**The root lock has NO plugin-dir entries** (`packages` keys starting `plugins` = none), and every
`plugins/**/package-lock.json` is git-IGNORED (`.gitignore:170`). `git ls-files plugins/dist/qtap-plugin-openai-compatible/`
lists no lockfile; `git show 25b8fab6a:plugins/dist/qtap-plugin-openai-compatible/package-lock.json` → "exists on disk,
but not in '25b8fab6a'". So **`npm ci --offline` in a pinned worktree installs the ROOT tree only; the plugin dirs'
`node_modules` do not exist there and cannot be `npm ci`'d** (§4.1).

### 1.7 What `openai` 7.30.0 → 7.34.0 changed INSIDE the bundles (measured on `plugins/dist/qtap-plugin-curl/index.js`, representative)

Eight bundles are re-bundled with the SAME delta (each `+743 / −230` lines; hashes of the three clean groups equal,
the rest differ only by `normalizeRequestOptionsForQueryKeysNN` numeric suffixes): `anthropic`, `curl`,
`default-system-prompts`, `google`, `mcp`, `ollama`, `openrouter`, `search-serper`. Their `package.json` / `manifest.json`
patch bumps (a deps-only rebuild): anthropic 1.0.61 → 1.0.62, curl 1.0.30 → 1.0.31, default-system-prompts 1.1.25 →
1.1.26, google 1.1.55 → 1.1.56, mcp 1.1.43 → 1.1.44, ollama 1.0.52 → 1.0.53, openrouter 1.0.66 → 1.0.67, search-serper
1.0.25 → 1.0.26. The constant: `var VERSION = "7.30.0"` → `"7.34.0"` (named `VERSION2` in the anthropic and google
bundles, which carry a second `VERSION`). **The other seven bundles are NOT re-bundled** and still carry
`VERSION = "7.23.0"`: deepseek, grok, nanogpt, openai-compatible, openai, z-ai (plugin-local `openai`), and
`builtin-embeddings` (no `openai` at all). Measured at the pin, `grep -o 'VERSION[0-9]* = "7\.[0-9.]*"'`.

124 hunks in the curl bundle; **96 are pure `normalizeRequestOptionsForQueryKeysNN` renames** (the renumbering that a
new `Environments` helper triggers); the 28 others, by category:

**(a) The "streaming accumulator summing `*_tokens_details`" is `AbstractChatCompletionRunner.#calculateTotalUsage`**
(the `runTools` / `ChatCompletionRunner` total — NOT `ChatCompletionStream` and NOT a per-chunk accumulator).
Hunk `@@ -11228,6 +11244,26 @@`, verbatim:

```js
}, _AbstractChatCompletionRunner_calculateTotalUsage = function _AbstractChatCompletionRunner_calculateTotalUsage2() {
  const total = { completion_tokens: 0, prompt_tokens: 0, total_tokens: 0 };
  for (const { usage } of this._chatCompletions) {
    if (usage) {
      total.completion_tokens += usage.completion_tokens;
      total.prompt_tokens += usage.prompt_tokens;
      total.total_tokens += usage.total_tokens;
+     for (const key of ["completion_tokens_details", "prompt_tokens_details"]) {
+       const details = usage[key];
+       if (isObj(details)) {
+         const totalDetails = total[key] ?? (total[key] = {});
+         for (const name2 in details) {
+           if (!hasOwn(details, name2)) { continue; }
+           const count = details[name2];
+           if (typeof count === "number") {
+             Object.defineProperty(totalDetails, name2, {
+               value: (hasOwn(totalDetails, name2) ? totalDetails[name2] ?? 0 : 0) + count,
+               enumerable: true, configurable: true, writable: true });
+           }
+         }
+       }
+     }
    }
  }
  return total;
```

It sums `usage` across `this._chatCompletions` — the list of completions a RUNNER made (`runTools` loops). It is reached only
through `runner.totalUsage()` (`:10982`) / the `totalUsage` event (`:11015`). **v4 reaches it from nowhere**:
`git grep -nE '\.(runTools|runToolsStream)\(|chat\.completions\.(stream|parse)\(|responses\.(stream|parse)\(|totalUsage|\.finalChatCompletion|beta\.chat' 25b8fab6a -- lib app packages plugins/dist/*/*.ts plugins/dist/*/*/*.ts`
returns only unrelated `totalUsage` locals (tags routes, Brahma's own accumulator). Every v4 OpenAI-SDK call is
`client.chat.completions.create(...)` (plugin-utils `:477` / `:520`, deepseek, nanogpt, z-ai) or
`client.responses.create(...)` (openai `:495/:518/:558/:576`, grok `:420/:515`), iterated raw. **Unreachable.**

**(b) The URL-filename decode** (`getName`, `@@ -8686` and `@@ -8697`) — multipart upload filenames (`toFile` / image
edits), nothing on a Chat Completions request:

```js
-      return basename(new URL(url).pathname);
+      const filename = basename(new URL(url).pathname);
+      return filename === void 0 ? void 0 : basename(decodeURLFilename(filename));
...
+function decodeURLFilename(filename) {
+  try {
+    const decodedFilename = decodeURIComponent(filename);
+    if (/%(?:0[AD]|22)/i.test(decodedFilename)) { return filename; }
+    return decodedFilename.replace(/[\u0000-\u001F\u007F]/gu, encodeURIComponent);
+  } catch { return filename; }
+}
```

**(c) Redacted query keys** (`@@ -5891` `sensitiveQueryNames`): `+ "sig", "signature", "xgoogsignature",
"xgoogcredential"` — the SDK's own debug-log URL redaction set. No wire effect.

**(d) The version header:** `-var VERSION = "7.30.0"; +var VERSION = "7.34.0"` → feeds `x-stainless-package-version`
for requests built through THIS copy (§2.6: none of the recorded requests).

**(e) New API surface** (never called by Quilltap): `admin.organization.usage.decisions`, `APIKeys` additions,
`agents.environments.create` / `list` + `EnvironmentInfosPage`, `Vaults` additions, a `Voices.create` doc rewrite, a
`Decisions.create` doc rewrite, `Files` doc, `Templates` (68 changed lines).

**(f) The `Responses` helper layer** (`parseResponse` phase filter, `getOutputItemIdentityKeys`, `sanitizeResponseEvent`,
`replaceResponseOutputItem`, `accumulateOutputItemEvent`, `ResponseStream` +213/−… — an output-position / identity
index rewrite). `parseResponse` is `responses.parse`; `ResponseStream` is the `responses.stream()` helper whose own
`client.responses.create({ ...params, stream: true })` (bundle `:24766`) it wraps. v4's `openai` / `grok` plugins call
`responses.create(...)` directly and decode events in their own `buildRawResponse`; **unreachable**.

**Net: every 7.34.0 delta is outside a call v4 makes.** For the four recorded-stamp providers it is also outside the
recorder's resolution (§2.6).

### 1.8 What `npm ci --offline` in a pinned worktree installs, per location (read from the pin's files)

| location | declared by (pin) | `npm ci --offline` installs | what a recorder run there resolves |
|---|---|---|---|
| ROOT `node_modules` | root `package-lock.json` | `openai` **7.34.0**, `zod` **4.6.7**, `@openrouter/sdk` 1.4.25, `@quilltap/plugin-utils` 2.7.1, `@quilltap/plugin-types` 2.8.0 | the nine plugin dirs with no local `openai` (anthropic, builtin-embeddings, curl, default-system-prompts, google, mcp, ollama, openrouter, search-serper) |
| `plugins/dist/qtap-plugin-{deepseek,grok,nanogpt,openai-compatible,openai,z-ai}/node_modules` | `package.json` `"openai": "^7.23.0"`; lock git-ignored | **NOTHING** (no tracked lock; `npm ci` refuses without one; `npm install --offline` would resolve `^7.23.0` to the HIGHEST cached — plausibly **7.34.0**, which would break the 7.23.0 stamps) | own `openai` — live: **7.23.0** (installed 2026-09-25) |
| `plugins/dist/qtap-plugin-anthropic/node_modules` | `@anthropic-ai/sdk ^0.115.0` | nothing | own `@anthropic-ai/sdk` 0.115.0 |
| `plugins/dist/qtap-plugin-google/node_modules` | `@google/genai ^1.52.0` | nothing | own 1.52.0 |
| `plugins/dist/qtap-plugin-openrouter/node_modules` | (own `@openrouter/sdk`) | nothing | own 1.3.28 |
| every plugin's `node_modules/@quilltap/plugin-utils` | `^2.7.1` | nothing | live: **2.7.1** in all (installed 2026-10-13 23:04, the minute of `a8b2d2861`) |

The ledger's recipe symlinks the third class from the live checkout (`§5.1`); at pin == HEAD the live plugin dirs
are exactly what the pin's `package.json`s ask for. The danger direction is the opposite of what the ledger says:
**do not install in the plugin dirs** (a fresh `npm install` there moves `openai` to 7.34.0 and re-stamps all 242+8
OpenAI-SDK rows silently); verify and symlink (§4.1).

### 1.9 `zod` 4.6.5 → 4.6.7 — measured against the only 4.6.5 copy on this machine

`~/.npm/_cacache` is not readable directly; the live checkout holds 4.6.7 and `/Users/csebold/source/quilltap-website/
node_modules/zod` holds **4.6.5** (a registry install, `package.json` version-checked). `diff -rq` of the two trees
(declaration files and maps excluded):

| file | verdict |
|---|---|
| `v4/locales/en.js` | **IDENTICAL** — zero sentence changes (so no `Invalid input` / `Too big` / `Too small` / `expected string` text moved) |
| `v4/core/regexes.js` | **IDENTICAL** |
| `v4/core/errors.js`, `core.js`, `to-json-schema.js`, every other `v4/**` | identical (not in the diff list) |
| `v4/core/versions.js` | `patch: 5 → 7` |
| `v4/core/util.js` | **NEW** `export function sizeCaps(defChecks)` (the leading size checks that set an upper bound) |
| `v4/core/schemas.js` | **NEW** `capSize(inst, size)` + `arraySize` / `mapSize` / `setSize`, applied in `ZodArray`, `ZodMap`, `ZodSet` init |
| `v4/core/compile.js` | **NEW** `generateSizeGate` for the compiled (JIT) array / map / set paths |
| `src/v4/classic/tests/{array,map,set}.test.ts` | one new test each: "an upper size bound fails before any element is parsed" |

The behaviour change, verbatim (`schemas.js`):

```js
// an oversized container fails its leading size checks before any element is parsed
function capSize(inst, size) {
    const caps = util.sizeCaps(inst._zod.def.checks);
    if (!caps.length) return;
    const cap = Math.min(...caps.map((c) => c[0]));
    const parse = inst._zod.parse;
    inst._zod.parse = (payload, ctx) => {
        const n = size(payload.value);
        if (n > cap && !ctx?.skipChecks && ctx?.direction !== "backward") {
            for (const [max, ch] of util.sizeCaps(inst._zod.def.checks)) {
                if (n > max && ch._zod.def.when(payload)) {
                    const start = payload.issues.length;
                    ch._zod.check(payload);
                    // the value is still the raw input, so no later check may read it
                    payload.issues[start].continue = false;
                    util.attachSchema(payload.issues, start, inst);
                    return payload;
                }
            }
        }
        return parse(payload, ctx);
    };
}
```

`sizeCaps` walks `def.checks` from the front, stops at the first check whose `check` name does not match
`/_(length|size)$|_equals$/` ("a later check may rewrite the value"), and keeps those with a finite
`maximum ?? length ?? size >= 0`. **Consequence:** for `z.array(El).max(N)` fed `> N` elements, 4.6.5 parsed every
element (issues for each invalid element, in index order) and THEN appended the array's `too_big`; 4.6.7 emits ONLY the
array's `too_big` (path `[]`/the array's path, same `code`/`origin`/`maximum`/`inclusive`/message bytes) and parses NO
element. Also: later `.refine` checks on the array no longer run (`continue = false`). An oversized array whose
elements are all valid, a within-bounds array, and every string / number / object check are byte-identical.

**v4 schemas this reaches** (`git grep` at the pin, array + `.max`/`.length`): `app/api/v1/chats/route.ts:100` and
`[id]/schemas.ts:68` (`selectedSubpromptIds … .max(100)`), `app/api/v1/memories/route.ts:111` (`entities … .max(MEMORY_ENTITY_CAP)`),
`lib/memory/candidate.ts:165` (same cap, `.default([])`), `lib/scenario-builder/request-schema.ts:21,23`
(`characterIds` / `groupIds … .max(32)`), `lib/schemas/wardrobe.types.ts:219,230` (`itemIds … .min(1).max(500)`),
`app/api/v1/files/shared.ts:18` (`fileIds … .max(MAX_BATCH_SIZE)`), `lib/pascal/custom-tool.types.ts:660,987`
(`items` / `triggers` arrays), `lib/schemas/settings.types.ts:412`. v5's twins of the ones that emit TEXT are §2.10.

---

## 2. v5 TODAY (`56554d98c`) — per surface

### 2.1 The OpenAI-compatible decoder — `crates/quilltap-core/src/model/decoders/chat_completions_sse.rs` (768 lines)

- Module doc `:1-26`: "(openai-sdk flavor only — the raw fetch flavor never sets it)"; `Flavor::OpenAiCompatible` doc
  `:38-47`: "…and in its own shape: **no `usage` inside it, no reasoning, no cache usage**" — the cache clause is v4-STALE.
- Usage capture `:305-311`: `if let Some(usage) = chunk.get("usage") { if !usage.is_null() { self.usage_raw =
  Some(usage.clone()); } }` — last non-null frame wins; `{}` is kept (JS-truthy twin).
- `build_usage` `:318-375`. Absent-frame arm `:321-327` (`has_sdk_raw_response()` → all-zero usage else `(None, None)`;
  OAC → `(None, None)`: correct for v4's absent `usage`). Cache source `match` `:339-356`: **`Flavor::OpenAiCompatible =>
  (None, None)` at `:355`** — the stale arm. The shared tail `:357-374` computes
  `prompt_tokens: (prompt - cache_read).max(0)` where `cache_read = cached.unwrap_or(0)` and `cached` is the `> 0`-filtered
  `cached_from_details()` (`:331-336`) for ZAi / OpenRouterRaw. `get = |k| raw.get(k).and_then(|v| v.as_i64())` (`:329`) —
  integers only, float / string counts read as `None`→`0`.
- `build_done` `:487-540`: OAC arm `:498-506` sets only `raw_response` (tool calls) — **no `raw_provider_usage`**;
  `:519` z-ai / nanogpt set `raw_provider_usage = Some(usage_raw.clone().unwrap_or(Null))` (always-present key).
  `usage` / `cache_usage` come from `build_usage` at `:488`.
- `build_raw_response` `:395-416`: OAC choices-only literal with no `usage` key — matches v4 and stays.
- Flavor selection: `model/streaming_provider.rs:131-146` `decoder_selection` — `_ => ChatCompletionsFlavor::OpenAiCompatible`
  (`:141`) is the catch-all for any provider whose manifest names `ChatCompletionsSse` and is not one of
  deepseek / z-ai / openrouter / nanogpt — in practice only `OPENAI_COMPATIBLE`.

### 2.2 The non-streaming parse — `crates/quilltap-core/src/model/response_parse.rs` (1,453 lines)

- `ChatFlavor` `:82-106` (OpenAiCompatible doc "the base — content / finishReason / usage / raw only", `:84`);
  `i64_at` `:109`; `sub_floor` `:114` (`(a - b).max(0)`); `nanogpt_cache_usage` `:147-175`.
- `parse_chat_completions` `:256`; `usage = response.get("usage").cloned().unwrap_or(Null)` `:265`; the snake_case arm `:385-495`.
  **`:404 ChatFlavor::OpenAiCompatible => (None, normalize_oac_tool_calls(msg), None)`** — third element is the cache
  usage: always `None`. The shared tail `:474-494`: `read = cache_usage.and_then(|c| c.cache_read_input_tokens).unwrap_or(0)`;
  `prompt_tokens: sub_floor(i64_at(&usage,"prompt_tokens"), read)`, `total_tokens: sub_floor(.., read)`, `cache_usage` carried.
- `parse_for_provider_ex` `:1185-1210`: `None` (unknown provider) falls back to `ChatFlavor::OpenAiCompatible` (`:1208`) —
  a v5-only default; the OAC change therefore also applies to any unrecognised provider id (harmless: v4 would have
  thrown earlier). Dispatch table `model/provider_io.rs:55-64` `chat_parse_flavor`.
- Unit tests that exercise the OAC flavor: `response_parse.rs:1216-1227` (`chat_completions_base_minimal`, asserts
  `total_tokens == 12` with no cache) and `:1436-1452` (tool-call normalisation, `usage 1/1/2`). Neither contains a cached
  value; both stay green.

### 2.3 Where `cacheUsage` / `rawProviderUsage` go next

- `model/stream.rs:240-244` `StreamUsage {prompt_tokens, completion_tokens, total_tokens}` (i64); `:249-255`
  `StreamCacheUsage {cached_tokens, cache_discount, cache_creation_input_tokens, cache_read_input_tokens}` — `Copy`;
  `:300` `StreamChunk.raw_provider_usage: Option<Value>` ("an explicit `Some(Null)` vs `None` is preserved").
- Streaming leg tracker `services/primary_stream.rs:1165-1192` `LegUsage::observe`: keeps the last `usage`,
  last `cache_usage`, and `raw_provider_usage` **only when `is_object()`** (so NanoGPT's `Some(Null)` never overwrites,
  and OAC's new `Some(object)` WILL be kept). Log params `:1028-1156`; the log's cache block `:1152-1156`
  (`LogCacheUsage { cache_creation_input_tokens, cache_read_input_tokens }` from `c.cache_read_input_tokens`).
  Failover loop twin `services/provider_failover.rs:1131-1183`. Help chat `services/help_chat/orchestrator.rs:1431-1447`.
- `services/llm_logging.rs:263-298`: `usage` built "only when any token field is present"; `cache_usage` built only when
  `cache_creation_input_tokens.is_some() || cache_read_input_tokens.is_some()` (values passed as-is); `raw_provider_usage`
  `filter(|v| !v.is_null())` (`:298`). Writer `db/llm_logs.rs:549-599` (`rawProviderUsage` JSON column), read `:1229`.
  **`cached_tokens` alone is never logged** — only `cacheReadInputTokens` reaches the row (the OAC pair is equal, so the
  log is complete).
- Done event `services/message_finalizer.rs:1379` + `:1898-1905` `to_done_cache_usage` (creation + read only);
  `services/orchestrator.rs:3609/3651/3824/3899/4079`, `:5037`.
- Non-streaming completion `model/completion_provider.rs:352` carries `parsed.cache_usage`; its test `:557-580`
  (`composes_completion_and_carries_cache_usage`, cache read `Some(40)`) is the shape to copy.
- **Budget:** `enclave/step.rs:781-797` — "Cache-read tokens are excluded by default (the provider plugins subtract them
  from usage.totalTokens at the source); a room opts into counting every token with `budgetExcludeCacheHits = 0`, adding
  the stripped reads back", via `db/llm_logs.rs:1068-1080` `get_total_token_usage_for_run(run_id, include_cache_hits)`
  → `sum_usage_rows` (`:1119`) which re-adds `cacheUsage.cacheReadInputTokens`. **So the OAC port fixes BOTH halves**:
  the default exclusion (via `usage`) AND the opt-in add-back (via the now-present `cacheUsage` on OAC rows). The
  ledger's "lands there for free" covers only the first.
- Vision arrival check `llm/vision_request.rs:141-146` (mirror of v4 `lib/llm/vision-request.ts:106-110`):
  `cache_read = cache_read_input_tokens + cached_tokens` (v4 sums BOTH keys — a v4 double-count for every provider that
  sets both: DeepSeek, Z.AI, and now OAC). Mirrored faithfully; the OAC port makes the check see `2c` extra billed tokens.
  No action — recorded so a reader does not "fix" the sum.
- Pricing: `services/cost_events.rs:113-124` and `cost_estimation.rs` consume `prompt_tokens` (already cache-excluded);
  OPENAI_COMPATIBLE has no pricing row, so no cost family moves. `get_total_token_usage_since` (`db/llm_logs.rs:1102`) is
  the daily gate; same row shape.
- SPA: `apps/web/src/app/chat/llm-inspector-entry.ts:330-346` and `llm-log-viewer-modal.ts:191-207` render
  `cacheUsage.cacheReadInputTokens`; `core/core-contract.ts:5036,5118` types it; `chat/llm-logs.api.ts`. **No SPA edit
  needed** — an OAC log row will simply start showing the cache row.

### 2.4 The ERROR-line twin and the DEBUG gap — `model/plugin_catch_log.rs`

Header table `:11-20` (OPENAI_COMPATIBLE / DEEPSEEK / NANOGPT · both methods → `<Name> API error in <method>`,
`context, baseUrl` + error); OAC arms `:172-179` (`"OpenAICompatible API error in streamMessage"` /
`"OpenAICompatibleProvider.streamMessage"` and the send twin). The capture rig: `streaming_composer_equivalence.rs:380-415`
`assert_catch_lines` filters `ERROR quilltap::model::streaming_provider ` and renders each context key unquoted (`%`
sigil). **Nothing in v5 emits or captures a plugin-DEBUG line for any text provider** — the closest precedent is
`model/image_dialects.rs:176-182` (`tracing::debug!(context = "OpenAIImageProvider.validateAndNormalizeSize", model = %model,
size = %size, "Requesting an experimental OpenAI image resolution")`), recorded from a v4 plugin DEBUG and compared by
`image_dialects_equivalence`. The OAC DEBUG is the first text-provider one.

### 2.5 The harness — corpora, consumers, recorders

| corpus (committed) | rows | recorder → script | consumers |
|---|---|---|---|
| `harness/oracle/fixtures/streams/chat_completions_sse/{deepseek,z-ai,openrouter,openai-compatible,nanogpt}.recorded.ndjson` + `cases.json` (87 specs) + `*.wire` | OAC: 14 rows (basic, tools, tools-with-text, 5 midstream-error, 6 sdk-*) | `providers/record-stream-fixtures.mjs` ← `regenerate-stream-fixtures.sh` | `stream_decoders_equivalence.rs` (`load_recorded` reads EVERY `*.recorded.ndjson` in the dir; `chunk_to_oracle` `:105-170` serialises `rawProviderUsage` and `cacheUsage` — **a v5 chunk lacking `rawProviderUsage` where v4 has it is a red by design**), `streaming_composer_equivalence.rs` (`canonical_provider` `:122-135`; same reads, full compose path) |
| `fixtures/response-bodies/response-bodies.recorded.ndjson` | 60 rows; OAC: `plain`, `no-usage` (2) | `record-response-bodies.mjs` ← `regenerate-response-bodies.sh` | `response_parse_equivalence.rs` (compares `usage`, `cacheUsage`, `raw`…; coverage asserts at `:300-340` are per-NanoGPT) |
| `fixtures/request-envelopes/request-envelopes.recorded.ndjson` | 399 (OAC 44) | `record-request-envelopes.mjs` ← `regenerate-request-envelopes.sh` | `request_builder_equivalence.rs`, `tool_wire_call_site.rs`, `provider_sdk_version_guard.rs` |
| `fixtures/request-envelopes/google-wire.recorded.ndjson` (24) / `google-request.recorded.ndjson` (13) | | `regenerate-google-wire.sh` (the SAME `record-request-envelopes.mjs`) | `request_builder_google_wire_equivalence.rs`, guard |
| `fixtures/image-dialects/image-dialects.recorded.ndjson` | 185 | `record-image-fixtures.mjs` ← `regenerate-image-fixtures.sh` | `image_dialects_equivalence.rs`, guard |
| `tool-wire`, `web-search-wire`, `moderation-wire`, `text-http-errors` | | `record-tool-wire`, `record-web-search-wire`, `record-moderation-wire`, `record-text-errors` | their own equivalence tests |

The recorded OAC stream rows today (committed, recorded under plugin-utils 2.6.3): e.g.
`{"case":"openai-compatible-basic",…,"chunks":[{"content":"Hello","done":false},{"content":", world","done":false},{"content":"","done":true,"usage":{"promptTokens":11,"completionTokens":4,"totalTokens":15},"attachmentResults":{"sent":[],"failed":[]}}]}`
— **no `rawProviderUsage`**. Five of the 14 rows carry a usage frame (`basic` 11/4/15, `tools` 50/15/65,
`midstream-error-null` 6/2/8, `sdk-after-done` 9/3/12, `sdk-id-only` 9/3/12) and will GAIN `rawProviderUsage`; the
other nine (tools-with-text has no usage frame; the six error rows throw before the final chunk) stay byte-identical.
Recipes live in shell scripts and in the tests' doc headers: none of these is a sweep stage ("recording is a deliberate
by-hand step, never a sweep stage", `response_parse_equivalence.rs:30-34`) — which is why the previous round's sweep did
not see the stale corpus.

### 2.6 `provider_sdk_version_guard.rs` (443 lines) — stamps, what it reads at run time

Constants: `RECORDED_OPENAI_SDK = "7.23.0"` (`:87`), `RECORDED_ANTHROPIC_SDK = "0.115.0"` (`:89`),
`RECORDED_GOOGLE_GENAI_SDK = "1.52.0"` (`:91`), `RECORDED_OPENROUTER_SDK = "1.3.28"` (`:93`),
**`INSTALLED_ROOT_OPENAI_SDK = "7.30.0"` (`:99`)**, `INSTALLED_ROOT_OPENROUTER_SDK = "1.4.25"` (`:103`),
`RECORDED_NODE = "v24.13.1"` (`:109`); `SDKS` table `:112`.

- Test 1 `every_installed_provider_sdk_matches_the_recorded_version` (`:161`): locator `common::v4_root()`
  (`QT_V4_CHECKOUT` → `QT_V4_ROOT` → `$HOME/source/quilltap-server`); reads `<checkout>/node_modules/<sdk>/package.json`
  at the ROOT and at each `plugins/dist/*/node_modules` (`install_locations` `:145`); `expected_at` (`:132`) expects the
  `INSTALLED_ROOT_*` constants at the root and `RECORDED_*` everywhere else; SKIPs loudly with no checkout /
  no root `node_modules` / **no plugin-dir installs** (`:196-205`). **It reads exactly four packages — it does not read
  `@quilltap/plugin-utils` at all** (the ledger's "reads plugin-utils via the SDK stamp" is wrong; §9).
- Test 2 `the_recorded_corpora_carry_exactly_the_recorded_sdk_versions` (`:353`): `corpus_stamps` (`:316`) walks the
  three corpora; every object carrying `headers` + sibling `url` yields `x-stainless-package-version` (attributed to
  anthropic if the URL contains `anthropic.com`, else openai), the OpenRouter speakeasy UA, `x-goog-api-client`
  `google-genai-sdk/<v>` + `gl-node/<v>`; `assert_all` demands every stamp equal the constant AND presence per corpus.
- Test 3 `the_recorded_corpora_carry_the_recorded_node_runtime` (`:434`).
- Committed stamps measured now: `request-envelopes` 242 × `7.23.0` + 46 × `0.115.0` (+ the OpenRouter UA), `image-dialects`
  8 × `7.23.0`, `google-wire` 24 × `google-genai-sdk/1.52.0`.
- **Status at the pin on the live checkout (HEAD == pin): test 1 is RED** (root `openai` is 7.34.0, constant says 7.30.0);
  tests 2 and 3 are green. **`zod_version_guard.rs:96` `RECORDED_ZOD_VERSION = "4.6.5"` is RED** (live 4.6.7).

Resolution table for the recorders (live checkout, measured by `ls`/`jq` on `node_modules/**/package.json`, NOT by running
`require.resolve`; the lane must repeat it with `createRequire` — memory `provider-sdk-resolution-and-shared-recorders`):

| plugin dir | local `openai` | `@quilltap/plugin-utils` | recorded requests built through |
|---|---|---|---|
| deepseek, grok, nanogpt, openai-compatible, openai, z-ai | **7.23.0** | 2.7.1 | local `openai` 7.23.0 → the 242 + 8 stamps |
| anthropic | none (falls to ROOT 7.34.0) | 2.7.1 | `@anthropic-ai/sdk` 0.115.0 (46) |
| google | none (ROOT) | 2.7.1 | `@google/genai` 1.52.0 (24) |
| openrouter | none (ROOT) | 2.7.1 | `@openrouter/sdk` 1.3.28 (UA ×15 + ×3) |
| builtin-embeddings, curl, default-system-prompts, mcp, ollama, search-serper | none (ROOT) | 2.7.1 (not builtin-embeddings) | none uses `openai` for a recorded request (Ollama speaks `fetch`) |

### 2.7 How the recorders resolve (the headers, verbatim facts)

Every `regenerate-*.sh` does `( cd "$V4/plugins/dist/$plugin" && npx tsx "$REC" … )` — cwd = the plugin dir, the recorder
`import()`s that plugin's `provider.ts` (`pathToFileURL(resolve('provider.ts'))`). Bare specifiers (`openai`,
`@quilltap/plugin-utils`, `@anthropic-ai/sdk`) resolve from the plugin dir's `node_modules` first. The stream recorder
needs `npx tsx --tsconfig "$V4/tsconfig.json"` (it imports v4's `lib/services/dangerous-content/refusal.ts` and
`lib/llm/fallback/engine.ts` from `--v4 "$V4"`). All run under Node 24 (`~/.nvm/versions/node/v24.13.1/bin` — the guard's
`RECORDED_NODE`; any other Node churns all 320 runtime stamps). Mocks: `globalThis.fetch` replaced under the SDK
(stream: a `ReadableStream` of line-aligned wire bytes; bodies: one `Response`), so the REAL openai SDK 7.23.0 parses and the
REAL plugin-utils 2.7.1 class normalises — this is the house SDK-mock; it is also why the previous round's OAC rows
cannot be trusted to equal a re-record today.

### 2.8 The Rust-side home for the new arithmetic (candidates, not decisions)

`response_parse.rs` already owns `sub_floor` (`:114`) and `nanogpt_cache_usage` (`:147`); the streaming decoder already
calls into `response_parse` for NanoGPT (`chat_completions_sse.rs:349`). The natural single home is one `pub(crate)`
function next to them (§7.1) called by BOTH the decoder's `Flavor::OpenAiCompatible` arm and the OAC arm of
`parse_chat_completions` — v4 has ONE `normalizeUsage` serving both paths and two Rust copies would be two places to drift.

### 2.9 MEASURED: does v5 decode an OpenAI-flavor stream path ITSELF where v4 relies on the SDK accumulator?

**No, on either side — so SDK 7.34.0's `*_tokens_details` summing cannot change any count.**

- v4 never runs an SDK accumulator (§1.7a). Its OpenAI-flavor stream paths iterate the raw `Stream` of
  `ChatCompletionChunk` (`for await (const chunk of stream)`, plugin-utils `:537`; DeepSeek `:308+`; NanoGPT `:354+`;
  Z.AI) and keep the LAST usage frame (`accumulatedUsage = chunk.usage ?? null`, `:543`). The OpenAI/Grok plugins iterate
  Responses events from `responses.create` directly.
- v5 reproduces exactly that: the decoder keeps the last non-null usage frame itself at
  **`chat_completions_sse.rs:305-311`** (`self.usage_raw = Some(usage.clone())`), reads counts at **`:318-375`**
  (`build_usage`), and exposes the raw object at **`:519`** (z-ai / nanogpt) / the OAC gap at **`:355`** and **`:498-506`**.
  The Responses decoder reads `resp.get("usage")` of the terminal `response.completed` event itself
  (`responses_api_sse.rs:211-276`, `raw_provider_usage` `:276`). No SDK is involved on the v5 side at all (it speaks the
  wire).
- The ONLY place a summed `*_tokens_details` could surface is `runner.totalUsage()` — never called. Conclusion for the
  ledger's "usage-count convergence candidate": **measured NEGATIVE.** There is nothing to converge; do not add a row to
  the divergence register for it.
- The usage-count convergence that DOES exist is `a8b2d2861` itself (§3), and it is a PORT, not a convergence.

### 2.10 The v5 Zod twins that 4.6.7's `capSize` reaches (array too_big + element issues)

| v5 site | what it emits for `len > max` with invalid elements | v4 schema |
|---|---|---|
| `services/chat_create.rs:629-672` `check_opt_subprompt_ids` (doc `:610-619` asserts "the array's own `too_big` … comes LAST, after every element issue (`['', …100 more]` answers two issues in that order)", "MEASURED on Zod 4.5.4") | element issues THEN `too_big` (origin `array`, max 100) | `selectedSubpromptIds … .max(100)` (`chats/route.ts:100`) |
| `memory_candidate.rs:572-596` `array_of` with `cap` (`too_big_array` at `:591`) | `invalid_type` per element THEN `too_big_array` | `z.array(z.string()).max(MEMORY_ENTITY_CAP).default([])` (`lib/memory/candidate.ts:165`) |
| `api/memories.rs:675-700` (`entities`, `too_big_array` at `:694`) | element issues THEN `too_big_array` | `app/api/v1/memories/route.ts:111` |
| `services/scenario_builder/request_schema.rs:225-250` `uuid_array_field` (doc "Elements first, then the array's own size check"; `too_big_array` `:249`) | per-element uuid issues THEN `too_big_array` (32) | `lib/scenario-builder/request-schema.ts:21,23` |
| `api/wardrobe_route_factory.rs:494-510` `parse_move_body` | returns `Err(())` — no issue text — **unaffected** | `moveWardrobeItemsSchema` |
| `pascal/custom_tool_types.rs` | no array-max text (grep: only string / number `Too big`) | **unaffected** |
| `api/zod_issues.rs:457-470` `ZodIssue::too_big_array` constructor ("measured at 4.6.5") | the issue itself is byte-identical at 4.6.7 | — |

Corpus coverage today: `grep -rl "expected array to have"` over `harness/oracle/fixtures` and `apps/web/src` → **nothing**;
the only committed oversize row is `chat_cast_routes_equivalence.rs:1351` (`selectedSubpromptIds` = 101 VALID ids → a lone
`too_big`, identical on both zod versions). So nothing committed currently pins "oversize AND an invalid element", and
nothing will go red by itself — the lane must ADD the rows (§4.8) or the claim stays unmeasured.

---

## 3. THE BEHAVIOUR DELTAS A PORT MUST HIT (numbered)

1. **Non-streaming OAC `usage` = `normalizeUsage(response.usage)`.** `prompt = max(0, prompt_tokens − c)`,
   `total = max(0, total_tokens − c)`, `completion` unchanged, all `?? 0`; `c = prompt_tokens_details.cached_tokens ?? 0`
   RAW. v5 site: `response_parse.rs:404` + the shared tail `:474-494` — the OAC arm must produce a cache source and the tail's
   `read` must be the RAW `c` (not the `> 0`-filtered one) for this flavor (item 3).
2. **`cacheUsage` iff `c > 0`:** `{ cache_read_input_tokens: c, cached_tokens: c }` (both `Some(c)`, creation `None`, discount `None`).
   Absent otherwise, including `cached_tokens: 0`, `null`, a `prompt_tokens_details` without `cached_tokens`, and a negative.
3. **Raw-subtract quirk:** a non-positive `c` yields no `cacheUsage` but STILL shifts the counts (`c = −5` → `prompt+5`,
   `total+5`; `c = 0` → no change). v4 has no guard. Reusing `cached_from_details()` (`chat_completions_sse.rs:331`, filters `> 0`) would
   silently diverge here; the port needs an unfiltered read for the subtraction and a `> 0` test only for `cacheUsage` and the DEBUG line.
4. **Streaming final chunk:** when a usage frame arrived, `usage` = `normalizeUsage(last frame)`, `cacheUsage` iff `c > 0`,
   **`rawProviderUsage` = the last frame's whole `usage` object** (v5: `chunk.raw_provider_usage = self.usage_raw.clone()`
   in the OAC arm of `build_done`, `chat_completions_sse.rs:498-506`, ONLY `if Some` — never `Some(Null)` as the z-ai/nanogpt
   arm at `:519` does). No frame ⇒ no `usage`, no `cacheUsage`, no `rawProviderUsage` (the present `build_usage` absent arm
   `:321-327` is already right for OAC).
5. **DEBUG line** `OpenAICompatible usage: excluding prompt-cache hits from prompt/total` with context
   `OpenAICompatibleProvider.normalizeUsage`, `promptTokens` (RAW), `cachedTokens`; once per `normalizeUsage` call when `c > 0`;
   OPENAI_COMPATIBLE only. Needs a capture pin (§4.7).
6. **Stale docs to rewrite:** `chat_completions_sse.rs` module doc `:20-24`, `Flavor::OpenAiCompatible` doc `:38-47`,
   the `build_usage` doc `:313-317` ("the plain base leave[s] `usage` undefined" stays TRUE for the absent frame only),
   `build_done` OAC arm comment `:499-501` ("No reasoning, no raw_provider_usage"), `response_parse.rs:11-14` and the `:84` /
   `:401-403` comments ("No reasoning channel, no cache usage — v4's base `sendMessage` reads neither"), and
   `docs/developer/porting/provider-manifest.md` where it describes the OAC decoder (grep "openai-compatible" there).
7. **DeepSeek / NanoGPT / Z.AI / OpenRouter / Ollama / Anthropic / Google / OpenAI / Grok: no change.** The lane must prove it:
   every non-OAC row of every recorded corpus re-records byte-identical (§4.4).
8. **Downstream, free once the decoder moves:** the budget exclusion AND opt-in add-back (§2.3), the OAC log rows gain
   `cacheUsage` + `rawProviderUsage`, the Inspector shows the cache row, the vision-arrival check sums `2c` (v4 double-count, mirrored).
9. **plugin-utils 2.7.1 is already the live recorder state** — the committed OAC stream corpus is stale against a fresh
   re-record today (five rows gain `rawProviderUsage`). The re-record is therefore REQUIRED, not optional, whatever the lane
   decides about the code.
10. **`provider_sdk_version_guard`: one constant.** `INSTALLED_ROOT_OPENAI_SDK` `7.30.0 → 7.34.0`; every `RECORDED_*` and every
    corpus stamp unchanged (predicted; proven by the re-record in §4.3). The doc header's resolution table (`:36-71`) gains a
    2026-10-16 paragraph (the root moved again, plugin dirs unmoved, plugin-utils 2.7.1 live, eight bundles rebuilt).
11. **`zod_version_guard`:** move `RECORDED_ZOD_VERSION` to `"4.6.7"` ONLY with the §1.9 table recorded in its doc comment
    (the D211 shape), and with the `capSize` rows added to the four twins' families (§2.10 / §4.8). Zero sentence / regex
    change; one container-size behaviour change.
12. **Recorded-manifest / bundle consequences:** none for v5's generated manifests (`provider_manifest/manifests/*.json`
    carry no plugin versions — grep of `"version"` empty); `gen-provider-manifests.mjs` and `provider-registry.ts` LOAD the
    rebuilt bundles, so re-run both as a byte-identical proof (§4.9).

---

## 4. THE DIFFERENTIAL PLAN

### 4.1 The pin (lane-unique path; the dependency trap handled)

```bash
LANE=provider            # the lane's own token
PIN=/tmp/qt-v4-pin-$LANE-25b8fab6a
git -C ~/source/quilltap-server worktree list                       # no other lane's pin may share this path
git -C ~/source/quilltap-server worktree add --detach "$PIN" 25b8fab6a
# root: a REAL tree from the pin's own lock (NOT the symlink — it follows the live checkout)
(cd "$PIN" && npm ci --offline --no-audit --no-fund)                 # needs 7.34.0 / 4.6.7 tarballs in ~/.npm cache; ENOTCACHED => stop and tell the human
# packages/quilltap: symlink class 2 (unchanged by either commit)
ln -sfn ~/source/quilltap-server/packages/quilltap/node_modules "$PIN/packages/quilltap/node_modules"
# plugin dirs: symlink class 3 — DO NOT npm install here (no tracked lock; a fresh resolve would pick openai 7.34.0)
for d in ~/source/quilltap-server/plugins/dist/*/; do
  [ -d "$d/node_modules" ] && ln -sfn "$d/node_modules" "$PIN/plugins/dist/$(basename "$d")/node_modules"
done
```

**Verify, do not assume** (the lane's first checks; print the table into the round record):

```bash
cd "$PIN"
node -p "require('./node_modules/openai/package.json').version"                # 7.34.0
node -p "require('./node_modules/zod/package.json').version"                   # 4.6.7
node -p "require('./node_modules/@quilltap/plugin-utils/package.json').version" # 2.7.1
git -C "$PIN" rev-parse HEAD                                                    # 25b8fab6aa58636c6885ba63684c7f8fbfca9c3b
# per plugin dir (Node 24.13.1): resolve the BARE package, walk up to the dir whose package.json name matches
# (require.resolve('openai/package.json') throws ERR_PACKAGE_PATH_NOT_EXPORTED — memory note)
for d in "$PIN"/plugins/dist/*/; do node -e '
  const {createRequire}=require("module"),fs=require("fs"),path=require("path");
  const req=createRequire(path.join(process.argv[1],"x.js"));
  for (const pkg of ["openai","@quilltap/plugin-utils"]) { try {
    let p=path.dirname(req.resolve(pkg));
    while(!fs.existsSync(path.join(p,"package.json"))||JSON.parse(fs.readFileSync(path.join(p,"package.json"))).name!==pkg) p=path.dirname(p);
    console.log(path.basename(process.argv[1]),pkg,JSON.parse(fs.readFileSync(path.join(p,"package.json"))).version,p);
  } catch(e){ console.log(path.basename(process.argv[1]),pkg,"UNRESOLVED") } }' "$d"; done
```

**Predicted table:** deepseek / grok / nanogpt / openai-compatible / openai / z-ai → `openai 7.23.0` under the live plugin dir (symlink
realpath); the other nine → `openai 7.34.0` from the pin root (but note: a module INSIDE a symlinked plugin `node_modules`, e.g.
`@quilltap/plugin-utils`, resolves its own `openai` from the symlink's REAL path, i.e. the LIVE checkout's root — equal to the
pin's root only while the human has not re-installed past the pin; re-check right before each regen batch); `@quilltap/plugin-utils`
2.7.1 everywhere (not builtin-embeddings). If any of the six prints 7.34.0 the plugin dirs were reinstalled — **STOP**:
that is a 242+8-row re-stamp, a decision for the human, not a guard-constant move.
Also: `grep -c normalizeUsage "$PIN"/plugins/dist/qtap-plugin-openai-compatible/node_modules/@quilltap/plugin-utils/dist/index.js` ≥ 1
(the dist the recorders actually run). Never remove the pin until the end-of-lane sweep is done:
`git -C ~/source/quilltap-server worktree remove --force "$PIN"`. The empty-file trap (redirect truncates before a loud
failure) applies only to `> file` stdout recorders — the `regenerate-*.sh` write via `--out`, but check `wc -l` after each.

### 4.2 Red-first: measure what a re-record changes BEFORE touching v5 code

All from `$PIN`, with `PATH=$HOME/.nvm/versions/node/v24.13.1/bin:$PATH`, `V4="$PIN"`, `V5=~/source/quilltap-v5` (the SPACELESS symlink —
the sweep driver's regens break on the real path's space; the `.sh` scripts quote `$V5` but use the symlink anyway):

```bash
cd ~/source/quilltap-v5
V4="$PIN" V5=~/source/quilltap-v5 bash harness/oracle/providers/regenerate-stream-fixtures.sh   2> /tmp/regen-streams.log
git diff --stat harness/oracle/fixtures/streams
V4="$PIN" V5=~/source/quilltap-v5 bash harness/oracle/providers/regenerate-response-bodies.sh   2> /tmp/regen-bodies.log
V4="$PIN" V5=~/source/quilltap-v5 bash harness/oracle/providers/regenerate-request-envelopes.sh 2> /tmp/regen-env.log
V4="$PIN" V5=~/source/quilltap-v5 bash harness/oracle/providers/regenerate-google-wire.sh       2> /tmp/regen-gw.log
V4="$PIN" V5=~/source/quilltap-v5 bash harness/oracle/providers/regenerate-image-fixtures.sh    2> /tmp/regen-img.log
```

Run ONE script at a time and `git diff --stat` after each; never two regens concurrently (shared `$TMP` is `mktemp`, but the
corpora are shared files). Remaining recorders (`regenerate-text-errors.sh`, `regenerate-tool-wire.sh`,
`record-web-search-wire.mjs`, `record-moderation-wire.mjs`, `record-google-request.mjs`) — run them too as the cheap
"expect byte-identical" proof (the §R.4(k) shape). **Predicted outcome table:**

| corpus | predicted diff after re-record (BEFORE the new cases are added) | why |
|---|---|---|
| `streams/chat_completions_sse/openai-compatible.recorded.ndjson` | **5 rows change**: `+ "rawProviderUsage": {prompt_tokens…}` on `openai-compatible-basic`, `-tools`, `-midstream-error-null`, `-sdk-after-done`, `-sdk-id-only` final chunks; the other 9 identical | §1.1 — plugin-utils 2.7.1 live |
| `…/{deepseek,z-ai,openrouter,nanogpt}.recorded.ndjson`, all other `streams/*` | byte-identical | overrides / no base class |
| `response-bodies.recorded.ndjson` | byte-identical (OAC `plain` `usage 9/5/14` and `no-usage` carry no cached tokens; `normalizeUsage` returns the same object) | |
| `request-envelopes.recorded.ndjson`, `google-wire`, `image-dialects` | byte-identical; stamps unchanged (242 × 7.23.0, 46 × 0.115.0, 24 × genai 1.52.0, 8 × 7.23.0) | no recorder resolves root `openai`; request builders untouched |
| `text-http-errors`, `tool-wire`, `web-search-wire`, `moderation-wire` | byte-identical | |

**Any other moved byte is a FINDING to classify (header stamp vs request shape vs usage), never a "fix the corpus" — and
a moved `x-stainless-package-version` means a plugin dir resolved 7.34.0: stop (§4.1).**

### 4.3 Re-stamp the guard (one constant)

After §4.2 shows the stamp censuses unchanged: in `provider_sdk_version_guard.rs` set `INSTALLED_ROOT_OPENAI_SDK = "7.34.0"`
(`:99`), leave every `RECORDED_*`, extend the doc header with a dated paragraph, and consider the optional hardening in §7.4
(a plugin-utils stamp). Run `cargo test -p quilltap-harness --test provider_sdk_version_guard` by name — green at the pin.
For `zod_version_guard.rs`: `RECORDED_ZOD_VERSION = "4.6.7"` plus the §1.9 table in the doc comment (D211 shape); note its
failure text instructs a full regen of every Zod-transcribing family, which the lane satisfies by the §4.8 measurement and the
sweep (the zod move changes zero sentences).

### 4.4 The `normalizeUsage` differential — which tier, which mock

**Tier 1 (exact), by the house SDK mock.** Do NOT write a jest `jest.mock('openai')` oracle first: every existing
provider oracle drives the REAL plugin through the REAL SDK with `globalThis.fetch` mocked beneath it
(`record-response-bodies.mjs`, `record-stream-fixtures.mjs`), which also exercises the SDK's own chunk parse (a usage
object with `prompt_tokens_details` survives the SDK unchanged, proven by the recorded `zai-cached` row). The v4 jest
test (§1.3) stays the secondary witness — its four rows are inputs to the new corpus rows below, verbatim.

**(a) Non-streaming — add to `record-response-bodies.mjs` `case 'openai-compatible'` (`:132-136`),** bodies `choices:[{index:0,
message:{role:'assistant',content:'…'},finish_reason:'stop'}]` plus `usage`:

| case name | `usage` | expected v4 `response.usage` / `cacheUsage` |
|---|---|---|
| `cached-800-of-1000` (jest #1) | `{prompt_tokens:1000,completion_tokens:50,total_tokens:1050,prompt_tokens_details:{cached_tokens:800}}` | `200/50/250`; `{cacheReadInputTokens:800,cachedTokens:800}` |
| `cached-exceeds-prompt` (jest #4) | `{prompt_tokens:10,completion_tokens:0,total_tokens:10,prompt_tokens_details:{cached_tokens:20}}` | `0/0/0`; `{20,20}` present |
| `cached-equals-prompt` | `{100,10,110, cached 100}` | `0/10/10`; `{100,100}` |
| `cached-zero-present` | `{12,5,17, prompt_tokens_details:{cached_tokens:0}}` | `12/5/17`; no `cacheUsage` |
| `details-without-cached` | `{12,5,17, prompt_tokens_details:{audio_tokens:3}}` | `12/5/17`; none |
| `details-null` | `{…, prompt_tokens_details:null}` | unchanged; none |
| `cached-negative` | `{1000,50,1050, prompt_tokens_details:{cached_tokens:-5}}` | `1005/50/1055`; none (the raw-subtract quirk) |
| `total-missing` | `{prompt_tokens:900,completion_tokens:20, prompt_tokens_details:{cached_tokens:300}}` | `600/20/0`; `{300,300}` |
| `completion-details-ignored` | `{…cached 800, completion_tokens_details:{reasoning_tokens:7}}` | as #1 (the `raw` row carries the details) |
| (existing) `plain`, `no-usage` | | unchanged |

Jest #3 ("no cache hits → no `cacheUsage` key") is already the existing `plain` row.

**(b) Streaming — new `.wire` files + `cases.json` specs** (flavor `"OpenAiCompatible"`, provider `"openai-compatible"`,
`model:"local-model"`, `temperature:0.7`, copy the `openai-compatible-basic` spec at `cases.json`), wires in the exact shape of
`openai-compatible-basic.wire` (a `delta.content` frame, a `finish_reason:"stop"` frame, then the usage frame, `data: [DONE]`):

| wire | frames' usage | expected final chunk |
|---|---|---|
| `openai-compatible-cached.wire` (jest #2) | last frame `choices:[]`, `usage` as #1 | `usage 200/50/250`, `cacheUsage {800,800}`, `rawProviderUsage` = the object |
| `openai-compatible-cached-exceeds-prompt.wire` | `{10,0,10,cached 20}` | `0/0/0`, `{20,20}`, raw |
| `openai-compatible-cached-zero.wire` | `cached_tokens:0` | unchanged counts, **no** `cacheUsage`, raw present |
| `openai-compatible-cached-negative.wire` | `cached_tokens:-5` | `+5` quirk, no `cacheUsage`, raw present |
| `openai-compatible-usage-two-frames.wire` | frame A `{1000,50,1050,cached 800}`, later frame B `{500,20,520}` (no details) | LAST wins: `500/20/520`, no `cacheUsage`, raw = B |
| `openai-compatible-usage-empty-object.wire` | `"usage":{}` | zero usage, no cache, `rawProviderUsage: {}` (JS-truthy `{}`) |
| `openai-compatible-usage-null-then-none.wire` | `"usage":null` frames only | NO `usage`, NO `cacheUsage`, NO `rawProviderUsage` |
| `openai-compatible-cached-tools.wire` | tool-call stream (copy `openai-compatible-tools.wire`) + cached usage | usage/cache/raw as above AND `rawResponse` UNCHANGED (still no `usage` key inside it) |
| `openai-compatible-usage-details-extra.wire` | `{…, prompt_tokens_details:{cached_tokens:800,audio_tokens:2}, completion_tokens_details:{reasoning_tokens:4}, vendor_extra:{a:1}}` | raw carries every key verbatim |

All also run the three chunkings the decoder test already does (whole / per-frame / byte) and the composer's
whole / byte (`stream_decoders_equivalence.rs`, `streaming_composer_equivalence.rs`); the composer needs NO new
`canonical_provider` arm (`openai-compatible` exists, `:128`).

**(c) The DEBUG line** is a log-capture pin — §4.7.

**Out-of-domain note to record, not to port:** JS reads `prompt_tokens` / `cached_tokens` as doubles; a string
(`"800"`) would be coerced by `Math.max(0, 1000 - "800")` and would land in `cacheUsage.cachedTokens` as a STRING. v5 reads
`as_i64()` everywhere in this family (`chat_completions_sse.rs:329`, `response_parse.rs:109`), so a float / string count is
0. Same domain rule as the four other flavors; do not add a row that tests it (it would only prove the existing seam) — but
the survey of the real SDK says `prompt_tokens_details.cached_tokens` is an integer on the wire.

### 4.5 Rust-side shape the rows pin

See §7.1. Red-first order: (1) re-record the corpora from the pin WITHOUT the new rows → run `stream_decoders_equivalence`
and `streaming_composer_equivalence` → expect EXACTLY five red rows (each missing `rawProviderUsage`); (2) add the new
`.wire`/`cases.json`/body rows + re-record → expect the new rows red for usage / cache / raw; (3) port; (4) green + every
non-OAC row still identical.

### 4.6 The usage-count convergence measurement (the task's ask), as a lane step

Record in the round record, with the evidence of §1.7a / §2.9: (1) the `git grep` over the pin finding zero SDK-runner /
`.stream(` / `.parse(` / `totalUsage` callers in v4 production code; (2) `plugins/dist/*/index.js` — the `calculateTotalUsage`
hunk is in the eight re-bundled files only and none of the eight providers makes an OpenAI chat call; (3) the three v5 decoder lines
(`:305-311`, `:318-375`, `:519`) and `responses_api_sse.rs:211-276` reading usage without any accumulator. The row stays
`NO-PORT-RATIFIED` for the SDK half; `a8b2d2861` is the PORT.

### 4.7 The DEBUG capture pin (new rig, small)

v4 side: in `record-stream-fixtures.mjs` and `record-response-bodies.mjs`, add a `pluginDebugSink` beside `pluginErrorSink`
(`record-stream-fixtures.mjs:210-239`): the `debug` method pushes `{ plugin: prefix, message, context: {...baseContext,
...c} }` when a sink is set; per-case, emit `pluginDebugLog` ONLY when non-empty (so every pre-existing row stays
byte-identical — the P4.122 `pluginErrorLog` precedent). `record-response-bodies.mjs` has no logger bridge today: install
`globalThis.__quilltap_logger_factory` BEFORE the provider module loads (as the stream recorder does at `:239`).
Expect a `pluginDebugLog` of exactly one entry on each cached>0 row and none on cached ≤ 0 rows (`cached-negative`,
`cached-zero`, `exceeds` → yes: `c=20 > 0` fires).
v5 side: emit `tracing::debug!(context = "OpenAICompatibleProvider.normalizeUsage", promptTokens = …, cachedTokens = …,
"OpenAICompatible usage: excluding prompt-cache hits from prompt/total")` once from the ONE normalisation function
(§7.1) — field order `context`, `promptTokens`, `cachedTokens` (the rig renders v4's context keys in object order, `%`-sigil
values); assert in `streaming_composer_equivalence.rs` next to `assert_catch_lines` (copy its render rule; filter by
`DEBUG <target> `) and in `response_parse_equivalence.rs` (which currently has no log capture — use the existing capture
helper in `common/mod.rs` `capture_fields`; memory `capture-rig-writer-thread`: the closure must run on the capture
thread, and a sibling test can cache the callsite as "never" — use the global capture). Remember `tracing`'s `Interest`
cache: enable the DEBUG level in the capture subscriber BEFORE the first callsite hit.

### 4.8 The zod 4.6.7 rows (§1.9 / §2.10)

Oracle-side: for each of the four v5 twins add rows `array of N+1` where one element is invalid, in the v4 family's own
corpus (below), recorded through v4's REAL schema at the pin (zod 4.6.7). Predicted: ONE issue (the array's `too_big`).
- `selectedSubpromptIds`: `chat_create_capstone_equivalence.rs` + `harness/oracle/fixtures/chat-create-capstone.json` (builder
  `build-chat-create-capstone.ts`) — row `['', …100 more]` (the doc'd 2-issue case at `chat_create.rs:617-619` becomes 1 issue).
- `entities`: `memories_routes_equivalence.rs` / `memories-web.json` and `memories_tier2_equivalence.rs` /
  `memory_candidate` family — row with `CAP+1` entries and a non-string at index 0.
- `characterIds` / `groupIds`: `scenario_build_request_schema_equivalence.rs` (pure tsx oracle
  `harness/oracle/cases/scenario-build-request-schema.ts`, header-recipe, no fixture) — row of 33 ids with one invalid.
- v5 code change (predicted, per site): when `len > max` return ONLY the array `too_big` and skip element checks (and any later
  array-level refine); keep the within-bounds path as is. The doc comment at `chat_create.rs:610-619` must be rewritten to
  "MEASURED on Zod 4.6.7" with the new behaviour; `zod_issues.rs:457-459` "measured at 4.6.5" needs no change (issue bytes identical).
- The `sizeCaps` rule is wider than `.max`: it also catches `.length(n)` / `.size(n)` / `*_equals` leading checks and `Map`/`Set`.
  Grep says v4 uses none of those on a v5-transcribed schema (§1.9 list) — record the grep.
- Run every file in the 31-file zod-text list as the regression ring (§5.3).

### 4.9 Byte-identical proofs that cost minutes

`harness/oracle/providers/gen-provider-manifests.mjs` (loads the bundles; `git diff crates/quilltap-core/src/provider_manifest/manifests`
must be empty) and `harness/oracle/cases/provider-registry.ts` (loads the nine built bundles; `provider_registry_equivalence`
green) — both read bundles that this span rebuilt. Also re-run the sweep driver from the pin
(`harness/tools/recipe_sweep.py --run-all --v4 "$PIN"`, Python ≥ 3.12 `/opt/homebrew/bin/python3.13`, `--v5w ~/source/quilltap-v5`)
for the Zod-transcribing families; expect no red attributable to zod.

---

## 5. FIXTURES — what re-records, who consumes it (the shared-recorder trap)

### 5.1 Re-record set and expected movement

| fixture | moves? | consumers (all must be re-run) |
|---|---|---|
| `streams/chat_completions_sse/openai-compatible.recorded.ndjson` | YES (5 rows + the new rows) | `stream_decoders_equivalence`, `streaming_composer_equivalence` |
| `streams/chat_completions_sse/cases.json` + new `*.wire` | YES (new specs) | the same two — **adding specs reddens the composer if its `canonical_provider` lacks the tag** (history: P4.D83 and P4.D101 both hit it); the tag exists |
| `response-bodies/response-bodies.recorded.ndjson` | YES (new OAC rows; existing 60 identical) | `response_parse_equivalence` |
| `streams/{deepseek,z-ai,openrouter,nanogpt}…`, `responses_api_sse/*`, `anthropic_sse/*`, `google_parts/*`, `ollama_ndjson/*` | no | same two |
| `request-envelopes.recorded.ndjson`, `google-wire`, `google-request`, `image-dialects` | no (stamps proven) | `request_builder_equivalence`, `request_builder_google*_equivalence`, `tool_wire_call_site`, `image_dialects_equivalence`, `provider_sdk_version_guard` |
| `text-http-errors`, `tool-wire`, `web-search-wire`, `moderation-wire` | no | their equivalence tests |
| any new `pluginDebugLog` field | additive (only on cached>0 rows) | the two log-asserting tests |

### 5.2 The shared-recorder trap, applied

`record-request-envelopes.mjs` feeds BOTH `request-envelopes.recorded.ndjson` AND `google-wire.recorded.ndjson` (memory
`provider-sdk-resolution-and-shared-recorders`). This lane edits `record-stream-fixtures.mjs` and
`record-response-bodies.mjs` (the DEBUG sink); each feeds ONE corpus — but `record-stream-fixtures.mjs` feeds ten
`.recorded.ndjson` files across five decoder dirs. So: re-run the ENTIRE `regenerate-stream-fixtures.sh` (all ten), not just
the OAC line; and if the lane touches `record-request-envelopes.mjs` for any reason, re-run BOTH `regenerate-request-envelopes.sh`
and `regenerate-google-wire.sh` (P4.128's two-row residue is exactly this trap). `git log -S<case-name>` a recorder before
calling a grown corpus "drift".

### 5.3 The Zod-text regression ring (31 test files that carry Zod error text, `grep -lE 'ZodError|Invalid input|expected string|Too big|Too small'`)

`api_keys_tier2`, `avatar_image_profile_resolution`, `brahma_console_routes`, `chat_create_capstone`,
`chats_messages_ops_tier2`, `chats_tier2`, `create_wrap_lines`, `fallback_engine`, `headshoulders_backfill_tier3`,
`groups_tier2`, `groups_routes`, `help_chats_routes`, `images_routes`, `images_generate_route`, `memories_routes`,
`memories_tier2`, `memory_settings_schemas`, `message_finalizer_tier3`, `pascal_custom_tools_route`, `projects_tier2`,
`repository_zod_messages`, `scenario_builder_mount_pool`, `settings_routes`, `system_export`, `system_jobs_collection`,
`system_import_state`, `system_restore_state`, `title_update_tier3`, `wardrobe_transfers_tier2`, `zod_issues_home_guard`,
`zod_version_guard` (all `*_equivalence.rs` except where named). Only `image_profiles_routes_equivalence.rs` and
`scenario_build_request_schema_equivalence.rs` mention `Too big`/`too_big` directly; the array sentence appears in no corpus.
The gate for them is the sweep from the pin (zod 4.6.7 resolved from the pin's real root tree), expecting no red.

---

## 6. TRAPS (memory notes and ones found here)

1. **The pin's plugin `node_modules` are the LIVE ones** (symlinks; the locks are untracked). A pinned regen proves nothing
   about an SDK bump the live plugin dirs never installed (memory `a-pinned-regen-cannot-prove-an-sdk-bump…`, guard header
   `:19-23`). Today this works FOR us (live == pin) — re-verify per batch, because the human runs `npm install` in v4.
2. **`npm install` in a plugin dir moves `openai` to 7.34.0** (range `^7.23.0`, newest cached). Never run it there.
3. **Root `node_modules` symlink = HEAD's deps.** `rm "$PIN/node_modules"` BEFORE `npm ci --offline` (ledger §5.1); a symlinked root
   would silently pass the new `INSTALLED_ROOT_OPENAI_SDK` against the live tree.
4. **`require.resolve('openai/package.json')` throws `ERR_PACKAGE_PATH_NOT_EXPORTED`** — looks like "not installed" (memory).
   Resolve the bare package and walk up.
5. **A shared recorder feeds a sibling corpus** (§5.2).
6. **Spaceless path:** `--v5w ~/source/quilltap-v5` (memory `sweep-driver-spaced-path`); the real path's space breaks every regen
   that word-splits.
7. **Node 24.13.1 only** (the guard's `RECORDED_NODE`); `npx tsx` from the plugin dir.
8. **The `rawProviderUsage` key is conditional here and UNCONDITIONAL on NanoGPT/Z.AI.** The `chat_completions_sse.rs:519` arm must
   not be widened to OAC; the OAC arm is `if let Some(u) = &self.usage_raw`. A `Some(Value::Null)` on OAC would be a v5-only key
   (and `LegUsage::observe` ignores non-objects anyway, so the log would not notice — the harness would).
9. **The 14-vs-15 and 8-vs-10 counts in the ledger are wrong** (§9) — do not size review checklists off them.
10. **The vision-arrival `2c` double-count is v4's** (`lib/llm/vision-request.ts:106-110`); do not "fix" `llm/vision_request.rs:141-146`.
11. **Pre-filter reuse hazard** (§3 item 3): `cached_from_details()` and `sub_floor(…, read)` with a `> 0`-filtered `read` are
    right for Z.AI / OpenRouter and WRONG for OAC's negative case.
12. **Equivalent-mutant trap on the negative row:** `cached_tokens: −5` is out of any real server's domain; it is in the corpus
    only to make the "subtract raw" choice falsifiable. A port that filters `> 0` before subtracting passes every in-domain row.
13. **The guard SKIPs, never fails, with no plugin installs** (`:196-205`) — a bare pinned worktree without the third symlink
    class makes test 1 print SKIP and pass; check the log for `SKIP:` before believing green.
14. **Jest / tsx `@/` alias:** the stream recorder needs `--tsconfig "$V4/tsconfig.json"` from a plugin cwd (header `:42-47`).
15. **`ENOTCACHED`:** `npm ci --offline` needs the 7.34.0 / 4.6.7 tarballs in `~/.npm/_cacache`; the human's cache has HEAD's, but
    if absent, stop — do not fall back to the network without the human (`--prefer-offline` fetches).

---

## 7. CONTRACT CANDIDATES (PROPOSED Rust spellings — none expected on the boundary)

Nothing here crosses the `Request`/`Response`/`Event` boundary or the SPA contract: `cacheUsage` / `rawProviderUsage` are already
typed (`core-contract.ts:5036,5118`), already logged, already rendered. No `CONTRACT` change.

### 7.1 The one normalisation function (core-private)

```rust
// crates/quilltap-core/src/model/response_parse.rs (beside sub_floor / nanogpt_cache_usage)
/// v4 `OpenAICompatibleProvider.normalizeUsage` (plugin-utils 2.7.1, `openai-compatible.ts:442-463`).
/// `usage` is the raw Chat Completions block (`None` for an absent / null one). The DEBUG line fires here, once.
pub(crate) fn oac_normalize_usage(usage: Option<&Value>) -> (StreamUsage, Option<StreamCacheUsage>) {
    let get = |k: &str| usage.and_then(|u| u.get(k)).and_then(Value::as_i64).unwrap_or(0);
    // `?? 0`: absent / null -> 0. RAW (may be <= 0): it is subtracted unfiltered.
    let cached = usage
        .and_then(|u| u.get("prompt_tokens_details")).and_then(|d| d.get("cached_tokens"))
        .and_then(Value::as_i64).unwrap_or(0);
    if cached > 0 {
        tracing::debug!(
            context = "OpenAICompatibleProvider.normalizeUsage",
            promptTokens = get("prompt_tokens"),
            cachedTokens = cached,
            "OpenAICompatible usage: excluding prompt-cache hits from prompt/total"
        );
    }
    (
        StreamUsage {
            prompt_tokens: sub_floor(get("prompt_tokens"), cached),
            completion_tokens: get("completion_tokens"),
            total_tokens: sub_floor(get("total_tokens"), cached),
        },
        (cached > 0).then_some(StreamCacheUsage {
            cached_tokens: Some(cached),
            cache_read_input_tokens: Some(cached),
            ..Default::default()
        }),
    )
}
```

Call sites: the decoder's `build_usage` `Flavor::OpenAiCompatible` arm (`chat_completions_sse.rs:355` — return early with
`Some(usage_raw)` → this function; keep `(None, None)` for the absent frame), the OAC arm of `parse_chat_completions`
(`response_parse.rs:404` + tail `:474-494` — the OAC branch must NOT go through the shared `read`-from-cache subtraction), and
`build_done`'s OAC arm (`raw_provider_usage = self.usage_raw.clone()` when `Some`). Field-name spelling of the `tracing`
fields (`promptTokens`, `cachedTokens` camelCase) follows `image_dialects.rs:176`'s precedent of v4's context key names;
confirm against the capture rig's rendering before settling. Whether `tracing::debug!` lives in `response_parse` (a `model`
module with no `tracing` use today — `grep` shows only `image_dialects.rs`) or at the composer is the lane's call; the harness
filter must name the chosen target.

### 7.2 Consumers needing NO change (verified by reading)
`LegUsage::observe` (`primary_stream.rs:1165-1192`), `llm_logging.rs`, `message_finalizer.rs`, `provider_failover.rs`,
`enclave/step.rs`, `db/llm_logs.rs`, the SPA inspector.

### 7.3 Docs to touch
`provider-manifest.md` (OAC decoder description), the `Flavor` / `ChatFlavor` rustdoc, `plugin_catch_log.rs` header ("ERROR
(and one WARN)" → add the DEBUG note or point to the new home), CHANGELOG (terse, plain American English), the ledger rows
`a8b2d2861` / `eb5d42338` → ABSORBED at unification (the unifier's job).

### 7.4 Optional guard hardening (recommended, small; the lane/human decides)
`provider_sdk_version_guard` cannot see plugin-utils. Add `RECORDED_PLUGIN_UTILS = "2.7.1"` and a fifth location check
(each `plugins/dist/*/node_modules/@quilltap/plugin-utils/package.json`), plus a corpus content tripwire (the recorded OAC stream
final chunks carrying a usage frame must carry `rawProviderUsage` — proving the corpus was recorded under ≥ 2.7.1). Rationale:
the only reason the committed OAC corpus is stale today is that nothing compares the recorders' plugin-utils version to the
corpus's. Without it, the next plugin-utils republish repeats this silently.

---

## 8. SUGGESTED LANE SHAPE (for the ordering agent)

One lane, one order, serial inside, from `main` at the round's pin `25b8fab6a` (or whatever the round pins — the survey's facts
hold for any pin ≥ `eb5d42338`, ≤ the first v4 commit that touches plugin-utils / the openai-compatible bundle again):

1. Pin + resolution table (§4.1) — if a plugin dir moved, STOP and report.
2. Red-first re-record (§4.2) — classify the five OAC row moves; prove the rest byte-identical.
3. Add the §4.4 rows (+ the §4.7 DEBUG sink) → re-record → red.
4. Port §7.1 (decoder arm, parse arm, `build_done`, DEBUG) → green on the two stream families + `response_parse_equivalence`.
5. Guard constants (§4.3) incl. the zod guard doc table; optional §7.4.
6. Zod 4.6.7 rows + the four twins (§4.8).
7. Docs / stale-comment sweep (§3 item 6), `gen-provider-manifests` + `provider-registry` byte-identical proofs (§4.9).
8. Gate: `cargo test -p quilltap-harness --test <each named family>` by name; the sweep from the pin for Zod-text families; clippy
   both feature sets; `cargo test --workspace` (background + completion notification only; never a `pgrep` poll — CLAUDE.md).

Sizing: small. Core hunks: three arms + one function (~60 lines) + four Zod-twin early returns; the weight is the corpus and the
measurements. No new D23 re-dump, no schema, no SPA, no help/ change from this slice.

---

## 9. WHAT THE LEDGER ROWS GOT WRONG (or left unmeasured)

1. **`eb5d42338` — "RE-BUNDLED into all ten `plugins/dist/*/index.js`" is wrong: EIGHT** (anthropic, curl, default-system-prompts,
   google, mcp, ollama, openrouter, search-serper). The other seven (deepseek, grok, nanogpt, openai-compatible, openai, z-ai,
   builtin-embeddings) were not rebuilt and still carry `VERSION = "7.23.0"` (or no openai). Same for §1 "Regen, dependencies, help"
   (`ledger:207-211` "all ten").
2. **"the streaming accumulator now SUMMING `completion_tokens_details` / `prompt_tokens_details` across chunks"** — it is
   `AbstractChatCompletionRunner.#calculateTotalUsage` (the `runTools` total across a runner's completions), not a
   chunk accumulator, and **unreachable** from v4 (zero runner / `.stream()` / `.parse()` callers). The row's "usage-count
   convergence candidate" measures **negative**; v5's decoders never rely on an SDK accumulator (§2.9).
3. **"the recorders must replay 7.34.0, and the corpora re-record with the new stamp" / "re-record, never pin the old stamp"
   (`ledger:211-213`, `eb5d42338` row's "the pin's plugin dirs must resolve 7.34.0")** — wrong direction. No recorder resolves the
   root `openai`; the six OpenAI-SDK plugin dirs stay at 7.23.0 and **must** (else 250 stamps churn); every `RECORDED_*` and
   every corpus stamp stays. The guard needs `INSTALLED_ROOT_OPENAI_SDK = "7.34.0"` alone (the P4.D260 R-F shape). The plugin
   dirs cannot be `npm ci`'d at a pin at all (untracked locks).
4. **`a8b2d2861` — "the pin's own `npm ci --offline` tree is at 2.6.3; at the NEXT pin those [plugin dirs] must be 2.7.1 or a
   regen silently replays 2.6.3"** — the live plugin dirs have been at **2.7.1 since 2026-10-13 23:04**; the silent hazard runs the
   other way: every regen today replays 2.7.1 against corpora recorded under 2.6.3 (the OAC stream corpus is stale now, five rows).
   The root lock at the BASELINE is 2.6.3, at the pin 2.7.1 — that is the only true half.
5. **`a8b2d2861` — "`provider_sdk_version_guard.rs` reads plugin-utils via the SDK stamp — check the 2.7.1 bump does not trip
   it"** — the guard reads FOUR packages and no plugin-utils at all; it cannot be tripped by 2.7.1 and cannot detect drift in it (§7.4).
6. **`a8b2d2861` — "@quilltap/plugin-utils `^2.6.2` → `^2.7.1` in … ALL 15 `plugins/dist/*/package.json`"** — 14 (builtin-embeddings
   declares none). (`ledger:200-202` says "all fifteen".)
7. **`a8b2d2861` — "v5's budget arithmetic reads the normalised `StreamUsage`, so the fix lands there for free once the decoder
   moves"** — true for the default exclusion; the opt-in `budgetExcludeCacheHits = 0` add-back reads `cacheUsage.cacheReadInputTokens`
   off the `llm_logs` row (`db/llm_logs.rs:1068`, `enclave/step.rs:781-797`) and is fixed by the SAME port only because the port
   also adds `cacheUsage`. Both halves ride one arm; neither is "free" — a decoder that normalised `usage` but skipped `cacheUsage`
   would break the add-back.
8. **`a8b2d2861` row — "`response-bodies.recorded.ndjson` none either"** is right; but it understates the stream side: the existing
   five usage-carrying OAC stream rows change on re-record (`rawProviderUsage`), independent of any new case.
9. **`eb5d42338` row — "zod 4.6.7 → the Zod-message families (re-run, expect no move)"** — the sentence / regex tables DO NOT move (verified
   byte-identical), but 4.6.7 carries a behaviour change (`capSize`, §1.9) that reaches four hand-written v5 twins, and
   `zod_version_guard` (`RECORDED_ZOD_VERSION = "4.6.5"`) is already RED against the live 4.6.7 — the row names neither.
10. **Ledger `§1`/`eb5d42338` — "`provider_sdk_version_guard`'s `INSTALLED_ROOT_OPENAI_SDK = "7.30.0"` and the recorded stamps trip at a
    pin past `eb5d42338` BY DESIGN"** — only the INSTALLED_ROOT constant trips; the recorded stamps do not (the corpora half stays green
    with no change). It is red today on the live checkout (root = 7.34.0).
11. **`a8b2d2861` row — "`sendMessage` … `usage` stays ALWAYS present"** ✓, and **"`streamMessage` … previously `usage: undefined`, never
    `rawProviderUsage`"** ✓ — hunks agree. **The DEBUG line's `promptTokens` is the RAW prompt count** (before subtraction) — the row
    abbreviates it as "promptTokens"; unambiguous only from the hunk.
12. **Test case 4 ("cached 20 > prompt 10 → all 0")** asserts `usage` only; by the code a `cacheUsage` `{20,20}` IS produced (the row implies
    nothing about it either way) — pin it in the corpus (§4.4a).
13. **Not in either row, found here:** the raw-subtract quirk for non-positive `cached_tokens` (§1.1.1, §3.3); the `2c` vision-arrival
    sum (§2.3); the unconditional-vs-conditional `rawProviderUsage` split between NanoGPT / Z.AI and OAC; the absence of any text-provider
    DEBUG capture in v5 (§2.4); the `ENOTCACHED` offline-install dependency.
