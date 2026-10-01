# Survey — P4.133: send the connection profile's own API key (dogfood #133)

**Date:** 2026-09-30 · **v4:** `ca363178d` (clean) · **v5 `main`:** `735cf568e`
**Kind:** read-only measurement. Nothing was built or run. Every path below is
relative to its repo root (v4 = `~/source/quilltap-server`, v5 = this repo).

**The finding in one line.** v4 sends the key the effective profile names. v5
resolves that key in most places too, then drops it before the wire. The only
wire key v5 sends from a chat/completion call is a provider scan,
`DbProviderKeys::key_for(provider)` (`crates/quilltap-host/src/spine.rs:206-219`),
which returns the first active key for the provider in insertion order.

---

## §A v4 (the oracle)

### A1 The resolvers (`lib/services/api-key.service.ts`)

| fn | line | rule |
|---|---|---|
| `getApiKeyForConnectionProfile(profileId, userId)` | `:23-32` | `findById(profileId)`; `if (!profile?.apiKeyId) return null`; `findApiKeyByIdAndUserId(apiKeyId, userId)` (**user-scoped**) → `key_value ?? null` |
| `getApiKeyForCheapLLMSelection(selection, userId)` | `:39-46` | `isLocal` → `''`; no `connectionProfileId` → `null`; else the fn above |
| `describeProfileApiKeyFailure` | `:67-71` | `'No API key configured for this connection profile'` / `'API key not found'` |
| `resolveConnectionProfileApiKey(repos, profile)` | `:86-102` | `!acceptsApiKey(provider)` → `{ok,''}`, with no lookup; no `apiKeyId` → `requiresApiKey` ? `{fail,'no-api-key-configured'}` : `{ok,''}`; `findApiKeyById(apiKeyId)` (the caller's `repos`: user-scoped where the caller passes `getUserRepositories`, unscoped where it passes `getRepositories`); missing row → `{fail,'api-key-not-found'}`; else `{ok,key_value}` |

- **`isActive` is NEVER consulted on the profile-bound path.** Both repo reads
  (`lib/database/repositories/connection-profiles.repository.ts:250-266`
  `findApiKeyById`, `:272-288` `findApiKeyByIdAndUserId`) are `findOne({id[,userId]})`
  plus `ApiKeySchema.parse`, inside a 4-arg `safeQuery` with a `null` fallback.
  So a profile bound to an **inactive** key still sends it, and a read error
  reads as "missing". Only the provider-scan sites filter `isActive` (A3).
- There is no fallback to a provider scan anywhere on a profile-bound path. A
  missing or dangling key is either `''` (bare) or a refusal, depending on the
  site (A2).

### A2 Every production LLM caller: which key it sends

`PB` = profile-bound (follows the profile's `apiKeyId`). `SCAN` = provider scan.

| # | surface | v4 site | resolver → key on the wire | missing/dangling key |
|---|---|---|---|---|
| 1 | Salon turn, primary | `lib/services/chat-message/participant-resolver.service.ts:236-242` (UNSCOPED `findApiKeyById`, no `isActive`) → `orchestrator.service.ts:431-439` gate → `streaming.effectiveApiKey` | PB | row missing → `''`; then `requiresApiKey && !rawApiKey` → **throws `'No API key configured for this connection profile'`** (`:433-434`); `acceptsApiKey` false → `''` |
| 2 | primary stream + its tool-unsupported retry | `primary-stream.service.ts:224`, `:288` `apiKey: streaming.effectiveApiKey` | PB (state) | — |
| 3 | native / text tool loops | `native-tool-loop.service.ts:340`, `:421`; `text-tool-loop.service.ts:390` (`streaming.effectiveApiKey`) | PB (state) | — |
| 4 | request-limit recovery | `recovery.service.ts:303` (`apiKey` from its ctx = state) | PB (state) | — |
| 5 | Concierge pre-call reroute (Unmoderated) | `dangerous-content/provider-routing.service.ts:86,111,125,135` → `understudy.ts:70-85` `decryptKey` (scoped `findApiKeyByIdAndUserId`) | PB (the **understudy's** key) | candidate with a `null` key is skipped (`understudy.ts:112-113`, `:151-152`) |
| 6 | uncensored retry / failover chain | `provider-failover.service.ts:813` `resolveConnectionProfileApiKey(repos, understudy)` → `:665` `restreamInto` `apiKey: opts.apiKey` | PB (the candidate's) | `!ok` → WARN `'[Failover] Understudy has no usable API key; moving on'`, `auth` attempt, `continue` (`:814-823`) |
| 7 | "Try uncensored" (regenerate on the understudy) | `app/api/v1/chats/[id]/messages/[messageId]/route.ts:434` `profileOverride: understudy` → `regenerate-swipe.service.ts:128` `profileOverride?.apiKey ?? participantResult.apiKey` → `:252` | PB (understudy / participant) | — |
| 8 | greeting (+ its ladder) | `app/api/v1/chats/route.ts:700-708` (UNSCOPED `findApiKeyById`) → `lib/chat/initial-greeting.ts:130,184` | PB | dangling → WARN `'[Chats v1] Connection profile is missing its API key'`, `NO_GREETING` |
| 9 | cheap LLM (memory, title, compression, scene state, image-prompt crafting, voice rewrite, Pascal `llm-consult`, …) | `lib/memory/cheap-llm-tasks/core-execution.ts:309-312` `getApiKeyForCheapLLMSelection`; the cheap fallback chain re-enters per selection (`:17` `buildCheapFallbackSelections`) | PB (via `selection.connectionProfileId`) | `null` → **throws `'No API key available for cheap LLM provider'`** |
| 10 | Concierge classify (cheap LLM) | `dangerous-content/gatekeeper.service.ts:378-381` | PB (selection) | `null` → WARN `'[Gatekeeper] No API key available for classification, failing safe'`, `safeFallback` |
| 11 | head-and-shoulders backfill | `background-jobs/handlers/character-headshoulders-backfill.ts:107` | PB (selection) | skip |
| 12 | Carina | `services/carina/carina.service.ts:517` (unscoped `findApiKeyById`) → `:676` | PB | `''` |
| 13 | help chat | `help-chat/orchestrator.service.ts:217` `resolveConnectionProfileApiKey` → `:361` | PB (bug-81 gate) | refuses with `describeProfileApiKeyFailure` |
| 14 | Brahma one-shot / orchestrator | `brahma-console/one-shot.service.ts:78`; `orchestrator.service.ts:192` → `:343` | PB (bug-81 gate) | refuses |
| 15 | Scenario Builder | `app/api/v1/scenario-builder/route.ts:63` → `agent-loop/one-shot-loop.ts:219` | PB (bug-81 gate) | 400 |
| 16 | generators | `character-wizard.service.ts:728,767,993,1032`; `character-optimizer.service.ts:816`; `ai-import.service.ts:833`; `external-prompt-generator.service.ts:104` (scoped) | PB | per site |
| 17 | image description (file-attachment fallback) | `lib/chat/file-attachment-fallback.ts:436` → `:524` | PB (image-desc profile) | `''` |
| 18 | connection-profile test-message | `app/api/v1/connection-profiles/route.ts:~428-438` → `:464` `sendMessage(requestParams, decryptedKey)` | PB (body's `apiKeyId`) | 404 |
| 19 | image generation (tool / avatar / story bg) | `lib/tools/handlers/image-generation-handler.ts:307,1362,1406`; `background-jobs/handlers/character-avatar.ts:136`; `story-background.ts:154` | PB (image profile) | per site |
| 20 | embeddings | `lib/embedding/embedding-service.ts:125` | PB (embedding profile) | — |
| 21 | moderation auto-detect | `gatekeeper.service.ts:154-180` | **first PROFILE** with `provider === 'OPENAI'`, then its `apiKeyId` (scoped) | `null` → LLM path |
| 22 | pricing fetch | `lib/llm/pricing-fetcher.ts:310-322` | first PROFILE for the provider with an `apiKeyId`, then the key | `null` |
| 23 | web search | `lib/tools/handlers/web-search-handler.ts:94-98` | **SCAN** `getAllApiKeys().find(provider === X && isActive)` | v4's "No API key configured …" |
| 24 | Almanack model collection | `lib/tools/almanack/phase2-machinery.ts:195-199` | **SCAN** (first active per provider) | — |
| 25 | key auto-association (key/profile create) | `lib/api-keys/auto-associate.ts:65,81` | **SCAN** (`provider && isActive`) | — |

### A3 v4's legitimate provider scans (v5 may keep a scan ONLY here)

API-key-table scans: **#23** `web-search-handler.ts:97`, **#24**
`phase2-machinery.ts:197`, **#25** `auto-associate.ts:81`. Profile-table scans
that then follow a profile's `apiKeyId`: **#21** `gatekeeper.service.ts:163`,
**#22** `pricing-fetcher.ts:315`. **No LLM chat or completion call in v4 scans.**

### A4 Bug-81 history

- `git log --oneline -S resolveConnectionProfileApiKey -- lib/ app/` →
  `d1c06cd9d` (Scenario Builder adopts it) and `b0eea4642` (`release: 4.9.0`,
  the squash that holds the original fix). The fix commit itself is squashed.
- `docs/developer/bugs/fixed/bug-81-oac-cannot-hold-an-api-key.md`: the gate
  decides two questions. `requiresApiKey` decides whether to refuse with no key.
  `acceptsApiKey` decides whether a stored key may be forwarded at all. A
  dangling `apiKeyId` fails loudly even on an accept-only provider. The Salon
  site (`orchestrator.service.ts:431-439`) keeps its **inline** two-question
  gate over the participant resolver's raw key and is NOT the composite
  (`resolveConnectionProfileApiKey`). So on the Salon a dangling id gives `''`
  and then the requires-gate, never `'api-key-not-found'`.
- v5 twin: `crates/quilltap-core/src/services/api_key_service.rs:202-225`
  (`resolve_connection_profile_api_key`, an exact port; UNSCOPED
  `api_keys::find_by_id`; a read error folds to `ApiKeyNotFound`). See B2.

---

## §B v5 on `main`

### B1 The host seam

**`spine.rs` module header, seam paragraph (verbatim, `:35-41`):**

> - **Provider→key resolution** ([`DbProviderKeys`]): the streaming/completion
>   seams carry only `(provider, base_url)` — v4 resolves the key by FOLLOWING
>   the effective profile's `apiKeyId`. The host key source scans for the
>   user's first active key for the provider
>   (`api_key_service::find_active_api_key_for_provider`, the
>   web-search/moderation style). Divergence is possible only when one user
>   holds several keys for the same provider.

The paragraph continues at `:43-60` with the bug-81 note: *"the orchestrator's
`effective_api_key` starts empty, and [`DbProviderKeys`] scans by provider with
no capability question asked — so a stored OAC key has always been
forwarded"*. It is pinned by
`api_key_service::tests::provider_scan_is_capability_blind`
(`api_key_service.rs:~561-580`).

- **Trait:** `ProviderKeySource { fn key_for(&self, provider: &str) -> Option<String> }`,
  **in CORE** at `crates/quilltap-core/src/model/streaming_provider.rs:76-78`.
  Impls are `HashMap<String,String>` (`:80`) and `SingleKey` (`:86-94`); its doc
  (`:33-44`, `:72-75`) describes the same seam. Host impl: `DbProviderKeys`
  (`spine.rs:206-219`).
- **Every `key_for` consumer** (two, both of which build the wire):
  - `WireStreamingProvider::prepare`, **core**:
    `streaming_provider.rs:286` (`self.keys.key_for(provider).unwrap_or_default()`,
    then `apply_auth`).
  - `WireCompletionProvider::send_inner`, **host**: `spine.rs:308`.
- **Every construction with `DbProviderKeys`:** `spine.rs:386-388`
  (`WireConfig::completion`, the factory behind all sixteen executor/job
  sites); `spine.rs:3657` (`ProductionSpineFactory`: streaming via
  `ProviderIo::streaming_provider`, `providers.rs:247-262`);
  `images_generate.rs:59-60`. The last one is the **Concierge classifier's
  cheap-LLM completion** for `?action=generate`, NOT the image desk. Test-only
  constructions: `spine.rs:4260`, `providers.rs:486`.
- **The boundary types carry no key and no profile id.**
  - `StreamingCompletionProvider::stream_message(provider, base_url, params)` at
    `crates/quilltap-core/src/model/stream.rs:518-525`.
  - `CompletionProvider::send_message(provider, base_url, params)` at
    `crates/quilltap-core/src/model/completion.rs:251-257`, plus the defaulted
    `send_message_with_anchor` (`:274-284`).
  - `StreamParams` (`stream.rs:469-506`) and `CompletionParams`
    (`completion.rs:123-~165`) have no `api_key`, `api_key_id` or `profile_id`
    field and do **not** derive `Default`.
  - grep: no `api_key_id` or `profile_id` anywhere in `model/`.
    `effective_api_key` exists only in `services/` (B3).

### B2 `resolve_connection_profile_api_key` (core, `api_key_service.rs:202`)

Returns `ProfileApiKeyResolution::{Ok(String), Failed(ProfileApiKeyFailure)}`.
**Every caller only VALIDATES it and drops the key:**

| caller | line | what happens to `Ok(key)` |
|---|---|---|
| Scenario Builder | `crates/quilltap-core/src/api/scenario_builder.rs:337-351` | dropped (only `Failed` is read → 400) |
| help chat | `services/help_chat/orchestrator.rs:694-712` | dropped. Comment: *"The resolved value is unused (the host streaming provider resolves keys internally)"* |
| Brahma one-shot | `services/brahma_console/mod.rs:192-216` | dropped |
| Brahma orchestrator | `services/brahma_console/orchestrator.rs:350-372` | dropped |
| failover chain | `services/fallback_repos.rs:70-89` → `provider_failover.rs:1361` | stored in `state.effective_api_key` on success only (`:1492`). Never reaches the wire |

### B3 `effective_api_key` and the call sites

- **Start:** `services/orchestrator.rs:1407`
  `let mut effective_api_key = resolution.api_key.clone().unwrap_or_default();`.
  `resolution.api_key` is **always `None`**
  (`services/participant_resolver.rs:139` doc, `:484` `api_key: None`; module
  doc `:54-61` "host-side deferred seam"). So the value starts as `""`.
- **Writes:**
  - `orchestrator.rs:1457` (danger reroute, `route.api_key`, profile-bound via
    `DbApiKeys`/`find_by_id_and_user_id`, `dangerous_content/provider_routing.rs:105-110`)
  - `orchestrator.rs:1484` (into `StreamingState`)
  - `provider_failover.rs:882` (uncensored retry success, `understudy.api_key`)
  - `provider_failover.rs:1492` (chain success)
- **Reads:** `primary_stream.rs:1623` → `RecoveryContext.api_key`
  (`recovery.rs:59`). Nothing reads that field again: `recovery.rs:390` streams
  with `(provider_name, base_url, params)`.
- **Verdict: dead.** It is written four times and never reaches a wire. No tool
  loop, restream or recovery passes it on.

`D` marks a row where v5 diverges from v4 (A2). "Drop" means the key is
resolved and then discarded. "Scan" means the host's provider scan picks the
wire key.

| # | v5 call site (the wire call) | key the core holds | wire key | D |
|---|---|---|---|---|
| 1 | `primary_stream.rs:1268` | `""` (resolver deferred) | Scan | **D** (also missing v4's requires-gate `orchestrator.service.ts:433`: v5 never throws `'No API key configured for this connection profile'`) |
| 2 | primary tool-unsupported retry (same fn) | `""` | Scan | **D** |
| 3 | `native_tool_loop.rs:491`, `:631`; `text_tool_loop.rs:715` | state (unused) | Scan | **D** |
| 4 | `recovery.rs:390` | `ctx.api_key` (unused) | Scan | **D** |
| 5 | danger reroute → `primary_stream.rs:1268` on the rerouted profile | `route.api_key` (PB) → dropped | Scan of the **uncensored provider** | **D** |
| 6 | `provider_failover.rs:1138` (`restream_into`, `:1097-1107`, takes **no key**); callers `:411`, `:866`, `:1415` | candidate key (PB) → dropped | Scan of the candidate's provider | **D** |
| 7 | `regenerate_swipe.rs:823`. `SwipeProfileOverride` (`:341-346`) has **no `api_key`**; host `spine.rs:1293-1309` comment: *"(The key is the transport's, per provider — nothing to resolve for it.)"* | none | Scan | **D** |
| 8 | `initial_greeting.rs:217`. `GreetingRequest.api_key` (`:67`) is filled at `chat_create.rs:2519-2540`, `:2714` and **unused by the stream** | PB → dropped | Scan | **D** (the dangling-key WARN is ported: `chat_create.rs:2531-2537`) |
| 9 | `cheap_llm_exec.rs:745`, `:791`, `:827` (`send_to_provider`, `:694`) | none. Module doc `:8-14` defers `getApiKeyForCheapLLMSelection` | Scan | **D** (also no `'No API key available for cheap LLM provider'` throw) |
| 10 | `dangerous_content/gatekeeper.rs:577` | none (`:18-22`, `:408-411` defer it) | Scan | **D** (also no `[Gatekeeper] No API key available…` WARN/safe-fallback) |
| 11 | head-and-shoulders: `headshoulders_backfill_job.rs:271-285` resolves (null gate only, `let _ = &api_key`) → cheap executor | dropped | Scan | **D**. ⚠ Its comment *"v5's completion boundary resolves the key itself from the profile"* (`:282-284`) is **false** |
| 12 | `carina_query.rs:1206` | `_api_key` (`:375-384`), dropped | Scan | **D** |
| 13 | `help_chat/orchestrator.rs:1413` | gate only (B2) | Scan | **D** |
| 14 | `brahma_console/orchestrator.rs:967`; one-shot via `agent_loop/one_shot_loop.rs:341` | gate only | Scan | **D** |
| 15 | Scenario Builder → `one_shot_loop.rs:341` | gate only | Scan | **D** |
| 16 | `generators/wizard.rs:440`, `:576` (keys read at `:727`, `:789`); `optimizer.rs:1078` (`:1952`); `ai_import.rs:1087` (`:1516`); `external_prompt.rs:432` (`:290`; doc `:19-25` records the drop) | read → dropped | Scan | **D** |
| 17 | `file_fallback.rs:755` (`_api_key` at `:675`) | dropped | Scan | **D** |
| 18 | `api/settings.rs:2579` (`connection_test_message`, `:2528`; resolves `decrypted_key` at `:2541-2552` for validation only) | dropped | Scan | **D** |
| 19 | image gen: `ImageProvider::generate_image(provider, api_key, params)` (`model/image.rs:241-248`), key passed explicitly (`tools/generate_image.rs`, `image_job_common.rs`, `dangerous_content/image_failover.rs:362,531`) | PB | PB | — |
| 20 | embeddings: `services/embedding_provider.rs:274-312` (`profile.api_key_id` → `find_by_id_and_user_id`) | PB | PB | — |
| 21 | moderation: `dangerous_content/moderation_wire.rs:158-176` (first OPENAI profile → `apiKeyId`) | PB | PB | — |
| 22 | pricing: `services/pricing_fetcher/mod.rs:609-610` | PB | PB | — |
| 23 | web search: `DbSearchApiKeys` (`spine.rs:233-252`) | SCAN | SCAN | — (v4's scan; KEEP) |
| 24/25 | Almanack collect / auto-associate | — | — | v5 auto-associate is a tracked deferral (`api/settings.rs:~2645`); not in scope |

**Secondary divergences the fix will expose (decide in the order):**

- **(i)** A profile with **no** `apiKeyId` on a provider that has a stored key:
  v5 sends the scanned key, v4 sends `''`. This matters most for OAC; OLLAMA's
  manifest `auth: none` injects nothing either way.
- **(ii)** A profile bound to an **inactive** key: v4 sends it, while v5's scan
  skips it and picks another active key.
- **(iii)** The four missing v4 refusals and log lines: rows 1, 9 and 10 above,
  plus the cheap null-key throw.

### B4 Does a CORE trait or type have to change? **Yes.**

The host only ever sees `(provider, base_url, params)`. Neither a profile id
nor a key crosses `stream_message` or `send_message`, so
`key_for_profile(profile_id)` cannot live in the host alone. And
`WireStreamingProvider` (the streaming wire) is itself **core**
(`model/streaming_provider.rs:204`). Three candidate shapes, all in
`crates/quilltap-core/src/model/` (shared contract):

| shape | blast radius | note |
|---|---|---|
| **A (recommended):** defaulted trait methods `stream_message_keyed(provider, base_url, api_key: &str, params)` / `send_message_keyed(…)` that delegate to the unkeyed method, overridden by `WireStreamingProvider` (core) and `WireCompletionProvider` (host) | `stream.rs`, `completion.rs`, `streaming_provider.rs`, `spine.rs`; every **call site** listed in B3 | Precedent: `send_message_with_anchor` (P4.D106, `completion.rs:259-284`). The 37 stream impls (27 files) and 41 completion impls (32 files) are untouched. ⚠ The `Arc<T>` impls (`stream.rs:533`, `completion.rs:289`) must **forward the new method explicitly**: the defaulted body resolves against Arc's own method and drops the override (see the comment at `completion.rs:~300`) |
| B: `api_key: Option<String>` field on `StreamParams` / `CompletionParams` | 43 `StreamParams {` literals in 22 files + 19 `CompletionParams {` literals in 14 files, no `Default` derive | Crosses lane ownership: the literals include `generators/**`, `services/orchestrator.rs` and harness tests |
| C: a new required param on the trait method | 78 impls, incl. the test impls in `services/build_context.rs:4301`, `:4612` | Collides with the narration lane's `build_context.rs`. Avoid |

Under A, keep `ProviderKeySource` for the differential's canned key maps. The
production `DbProviderKeys` scan then has **no legitimate caller left**,
because no v4 LLM call scans (A3). Delete it, or keep it only as the unkeyed
fallback the canned impls use. `DbSearchApiKeys` stays.

### B5 Seating the mocks (for the posed proof)

- **`harness/tools/refusal-server.py`.** The mode is chosen by **model name**
  (`MODES`, `:69-73`, `mode_for(req.get("model"))` at `:226`), or by
  `QT_REFUSE_MODE` for unknown names. **There is no `--mode` flag.**
  `unauthorized` answers 401 for **every** request on that model name,
  whatever key it carries. The capture (`:228-230`) writes the body only;
  **headers are not recorded**. So the posed "first key 401, second key
  answers" proof needs a harness-tool edit: a key gate (e.g. 401 unless
  `Authorization == Bearer $QT_REFUSE_KEY`) and/or `Authorization` in the
  capture line.
- **Playwright** (`apps/web/e2e/global-setup.ts`). The fixture's
  `OPENAI_COMPATIBLE` profile is pointed at `MOCK_LLM_PORT` (`:499-505`). Key
  rows are seeded by `runCliWrite` `INSERT INTO api_keys` (`:535-540` SERPER,
  `:548-553` NANOGPT), and `api_keys` is in the user-id rewrite list (`:276`).
  `support/mock-llm.ts` never reads `Authorization`, so no e2e beat sees a key.
- **quilltap-web tests.** The closest production-assembly home is
  `crates/quilltap-web/tests/impersonation_voice_preview_wire.rs` (and
  `generators_wizard_routes.rs`). Both boot `ProductionSpineFactory` (`:78` /
  `:45`) over a fixture whose seat profile points at `http://127.0.0.1:1/v1`.
  Swap that for a header-capturing `TcpListener` (template:
  `spine.rs:4242-4256`, `profile_timeout_tests`) and plant two keys. The
  fixture builders are `crates/quilltap-web/tests/common/mod.rs`:
  `materialize_salon_instance` (`:479`), `materialize_generators_instance`
  (`:438`) and `materialize_fixture_instance` (`:96`).
- **Host unit:** a `WireCompletionProvider` + `TcpListener` test in
  `spine.rs`'s test module is the cheapest red-first proof of the host override.
  ⚠ `host_gateway.rs:609-670` censuses `WireCompletionProvider::new(` (exactly
  **2** code hits in `spine.rs`) and `WireStreamingProvider::new(` (exactly 1 in
  `providers.rs`), and forbids them in `host.rs`, `avatar_preview.rs`,
  `almanack_services.rs` and `lib.rs`. A new construction in a test trips it.
  Reuse the existing ones or update the counts deliberately.

---

## §C Families that can (and cannot) see the key choice

| family | sees the key? | why |
|---|---|---|
| `request_builder_equivalence` / `request_builder_google*` (the `request-envelopes` corpus, `harness/oracle/providers/record-request-envelopes.mjs`) | **No** | The auth secret is normalized to a placeholder (`request_builder_equivalence.rs:39-46`, `:242-250`, `:698`). The corpus calls a builder with a fixed key |
| `text_http_errors_equivalence` | No | Classifies error bodies; `invalid_api_key_401` is a canned row |
| `tool_wire_call_site` | No | `TestKeys` returns one key for every provider (`:107-108`) |
| `primary_stream_tier3_equivalence` | **No, today** | Rust side: `FallbackKeys` always returns `"test-key"` (`:1907-1910`). v4 side: the mock's `_apiKey` is ignored (`primary-stream-tier3.test.ts:449`), and the canned understudy keys are constants (`:581`, `:604`, `:759`). **Closest home**: its ordered `stream_calls` comparand (`:379-387`, `:471-479`; P4.97) already records the option bag per call. Add `apiKey` there and have the Rust recorder capture the keyed method's argument |
| `orchestrator_tier3_equivalence` | **No, today** | The mock's `_apiKey` is ignored (`orchestrator-tier3.test.ts:310`, `:380`). `getApiKeyForCheapLLMSelection` is mocked to `'test-key'` (`:427-432`). But the `api_keys` table **is** seeded and read for real on both sides (W4.10a: `:136-139`, Rust `:229-230`, `:1467-1474`), and v4's participant resolver runs real with the unscoped `findApiKeyById`. **Best home for the Salon + danger-reroute proof** |
| `danger_routing_equivalence` | Partly | Compares the resolved `apiKey` per route (`:10`, `CannedApiKeys` `:79-86`). It already proves the reroute picks the profile-bound key, and the drop happens after it |
| cheap-LLM tier-3 families (`title-update-tier3`, `danger-gatekeeper`, `orchestrator-tier3`, …) | No | v4 mocks `getApiKeyForCheapLLMSelection` to a constant (`title-update-tier3.test.ts:405-410`, `danger-gatekeeper.test.ts:154-156`). A key-visible arm must lift that mock, so the null-key throw is unreachable in every existing oracle |
| `api_keys_tier2_equivalence`, `connection_profiles_tier2_equivalence` | No | Repo CRUD; no LLM call |
| `help_chat_orchestrator_tier3`, `brahma_orchestrator_tier3`, scenario-builder, carina, regenerate-swipe, initial-greeting tier-3 | No | Same mock shape (the `_apiKey` is ignored). Each can grow a recorded `apiKey` in the same way |

**Proof shape (proposed).**

- **(1) Tier-3 Salon.** In `orchestrator-tier3` / `orchestrator_tier3_equivalence`,
  add a case whose fixture seeds **two active keys for one provider**
  (`k-first` inserted first, `k-bound` second) with the participant's profile
  `apiKeyId → k-bound`. Both mocks record the key they are handed (v4: the
  `streamMessage` mock's second arg, flowing through the REAL
  participant-resolver → orchestrator gate). Expect `k-bound` on every leg.
  Add a danger-reroute arm (the uncensored profile bound to a third key) and a
  failover-chain arm in `primary_stream_tier3` (understudy bound to its own key;
  expect the understudy's key, not the primary's). Red-first: today v5 records
  nothing, or the scan.
- **(2) Inactive and keyless arms (B3 ii/i).** A profile bound to an
  `isActive = 0` key (v4 sends it). An OAC profile with no `apiKeyId` while an
  OAC key row exists (v4 sends `''`).
- **(3) Cheap LLM.** Lift the `getApiKeyForCheapLLMSelection` mock in one
  cheap family (`title-update-tier3` is the smallest) and record the key, plus
  a null-key arm for the throw.
- **(4) Production assembly.** A quilltap-web test in the
  `impersonation_voice_preview_wire.rs` pattern, over a header-capturing
  listener: `Authorization: Bearer k-bound`.
- **(5) Dogfood.** `refusal-server.py` gains a key gate, `k-first` gets the 401
  and the bound profile answers.

Jest traps that apply (memory):

- `jest-oracle-empty-provider-registry`: `acceptsApiKey` / `requiresApiKey`
  read the registry, which is empty in a jest oracle, so `?? true` fires.
  Initialize the plugins an arm needs, or the requires-gate fires
  spuriously.
- `jest-oracle-plugin-factory-is-mocked`: `createLLMProvider` is a bare
  `jest.fn()`.
- `jest-domock-survives-resetmodules`: lifting the api-key mock must be
  per-case, not a leftover.

---

## §D Traps and premises

1. **`effective_api_key` is dead** (B3). It starts as `""`, is written four
   times, and is never sent. The fix has to **seed** it (v4 seeds it in the
   participant resolver: `participant-resolver.service.ts:236-242`, unscoped,
   no `isActive`) and then **send** it. Seeding in
   `participant_resolver.rs:484` keeps `orchestrator.rs:1407` unchanged.
2. **The understudy and uncensored legs must send the UNDERSTUDY's key.**
   `restream_into` (`provider_failover.rs:1097`) takes no key today, and the
   chain only writes `state.effective_api_key` **after** success (`:1492`). The
   candidate's key has to be passed into the restream itself (`:1415`, `:866`,
   `:411`), not read off state.
3. **Cheap-LLM key = `get_api_key_for_cheap_llm_selection`** (core,
   `api_key_service.rs:84-95`, already ported and used once, at
   `headshoulders_backfill_job.rs:272`). ⚠ It needs a `Db` + user id.
   `CheapLlmTaskExecutor::with_logging` carries both (`cheap_llm_exec.rs:479-522`),
   but a bare `::new()` carries neither. P4.68 measured no bare production
   caller, and `bare_cheap_llm_executor_guard` pins that. Test and differential
   executors are bare: they must keep working with canned providers that ignore
   the key, and the v4 null-key throw cannot fire there.
4. **Inactive or deleted key, v4 exactly.** Inactive: sent, on every
   profile-bound path. Deleted (dangling): Salon gives `''` → the requires-gate
   throw (`orchestrator.service.ts:433-434`), or a bare send on accept-only
   providers. Greeting gives WARN + `NO_GREETING`. Failover chain / Brahma /
   help / Scenario Builder give `'api-key-not-found'`. Cheap LLM gives `null` →
   throw. Understudy discovery skips the candidate.
5. **Image desks: yes, v4 binds image profiles to a key**
   (`image-generation-handler.ts:307` `findApiKeyByIdAndUserId(imageProfile.apiKeyId, userId)`),
   and **v5 already matches**: the key is an explicit `ImageProvider` argument
   (`model/image.rs:245`). `images_generate.rs:59-60`'s `DbProviderKeys` feeds
   only the **Concierge classifier's cheap-LLM completion**. Under shape A that
   file needs no edit: the key travels with `classify_content`'s keyed call in
   `gatekeeper.rs`.
6. **Tauri builds no providers of its own.** `quilltap-tauri/src` has no
   `DbProviderKeys`, `ProviderIo` or spine. It boots through
   `quilltap_web::production_host_config` (`crates/quilltap-web/src/lib.rs:264-272`),
   the same `ProductionSpineFactory`. The CLI also builds no spine.
7. **Stale comments to correct in the same change** (each claims the drop is
   harmless):
   - `spine.rs:35-60`
   - `spine.rs:1296` ("nothing to resolve for it")
   - `streaming_provider.rs:33-44`, `:72-75`
   - `participant_resolver.rs:54-61`, `:139`
   - `cheap_llm_exec.rs:8-14`
   - `gatekeeper.rs:18-22`, `:408-411`
   - `headshoulders_backfill_job.rs:282-284` (false)
   - `external_prompt.rs:19-25`
   - `carina_query.rs:371-374`
   - `help_chat/orchestrator.rs:694-697`
   - `brahma_console/mod.rs:192-197`
   - `brahma_console/orchestrator.rs:350-353`
   - `file_fallback.rs:673-674`

   `api_key_service::tests::provider_scan_is_capability_blind` pins the scan
   and has to be retired or repointed.
8. **Bug 81 interplay.** With the key threaded, v4's Salon gate
   (`acceptsApiKey`/`requiresApiKey` over the raw key) becomes portable. It
   belongs after the courier check (`orchestrator.service.ts:422-439`), which
   is an `orchestrator.rs` hunk (see the overlap below). Do NOT route the Salon
   through `resolve_connection_profile_api_key`: v4 deliberately does not
   (A4).

**Collision check against the parallel lane** (`build_context.rs`,
`message_context.rs`, `memory_tasks.rs`, `generators/**`, `quilltap-host/src/host.rs`,
and per the sibling surveys `orchestrator.rs`):

- **`services/orchestrator.rs`: OVERLAP (same file, different hunks).** The
  narration-anchor survey (`survey-narration-anchor-p4d243.md`) touches
  `:1224`, `:1836-1843`, `:2534` and `:4446-4499`. This lane needs at most the
  v4 requires-gate near `:1407-1440`, and none at all if seeding lives in
  `participant_resolver.rs` and the gate is deferred.
- **`generators/{wizard,optimizer,ai_import,external_prompt}.rs`: OVERLAP.**
  The prompts-generators survey (P4.D241) edits `ai_import.rs`, `optimizer.rs`
  and `external_prompt.rs` (prompt text). This lane edits only the four send
  call sites (`wizard.rs:440,576`, `optimizer.rs:1078`, `ai_import.rs:1087`,
  `external_prompt.rs:432`). Recommend stacking those four as a rider after
  P4.D241, or splitting them out.
- **`build_context.rs`, `message_context.rs`, `memory_tasks.rs`, `host.rs`: no
  edit under shape A.** `build_context.rs` holds two test `CompletionProvider`
  impls that only shape C would touch. `host.rs` builds no wire provider
  (census, B5).
- **Model boundary types** (`model/stream.rs`, `model/completion.rs`,
  `model/streaming_provider.rs`): owned by this lane. No sibling survey names
  them.

---

## §E Ownership (shape A)

**CORE** (`crates/quilltap-core/src/`):

- `model/stream.rs`: keyed default method + `Arc` forward
- `model/completion.rs`: same
- `model/streaming_provider.rs`: the override; the doc
- `services/participant_resolver.rs`: seed `api_key` as v4 does
- `services/primary_stream.rs`: `:1268`, the retry; `:1623`
- `services/native_tool_loop.rs`: `:491`, `:631`
- `services/text_tool_loop.rs`: `:715`
- `services/recovery.rs`: `:390`
- `services/provider_failover.rs`: `restream_into` gains the key; `:411`, `:866`, `:1415`
- `services/regenerate_swipe.rs`: `SwipeProfileOverride.api_key`; `:823`
- `services/initial_greeting.rs`: `:217`
- `services/cheap_llm_exec.rs`: `send_to_provider`; the null-key throw
- `services/dangerous_content/gatekeeper.rs`: `:577`; the null-key WARN/safe-fallback
- `services/headshoulders_backfill_job.rs`: comment only
- `services/carina_query.rs`: `:1206`
- `services/help_chat/orchestrator.rs`: `:1413`
- `services/brahma_console/{mod.rs, orchestrator.rs}`: `:967`
- `services/agent_loop/one_shot_loop.rs`: `:341`, takes the key
- `api/scenario_builder.rs`: pass the resolved key
- `services/file_fallback.rs`: `:675`, `:755`
- `api/settings.rs`: `:2579`
- `services/api_key_service.rs`: retire or repoint the scan pin
- `services/orchestrator.rs`: **only** the optional requires-gate; flagged above
- `generators/{wizard,optimizer,ai_import,external_prompt}.rs`: **rider**;
  flagged above

**HOST** (`crates/quilltap-host/src/`):

- `spine.rs`: `WireCompletionProvider` override; header seam paragraph; `:1296`
  + the Try-uncensored override key; `DbProviderKeys` deleted or narrowed
- `providers.rs`: only if `streaming_provider` loses `K`
- `images_generate.rs`: no edit under A; only if `DbProviderKeys` is deleted
  (it names it at `:37`, `:60`)
- `host_gateway.rs`: census counts, only if constructions move

**HARNESS:**

- `crates/quilltap-harness/tests/{orchestrator_tier3_equivalence.rs, primary_stream_tier3_equivalence.rs}`
  plus their oracles `harness/oracle/cases/{orchestrator-tier3,primary-stream-tier3}.test.ts`
  (record `apiKey`) and the fixture-spec builders the cases seed `api_keys`
  through
- one cheap family (`title_update_tier3` + `title-update-tier3.test.ts`) for
  the selection-key arm
- `harness/tools/refusal-server.py`: key gate + header capture (dogfood
  instrument)

**WEB tests:** a new or extended test beside
`crates/quilltap-web/tests/impersonation_voice_preview_wire.rs`. It may touch
`tests/common/mod.rs` if a two-key fixture builder is added there.

**e2e:** none required. `support/mock-llm.ts` cannot see headers.

**Version bumps owned:** core (`0.0.1128` → next), host (`0.0.168` → next),
harness (`0.0.1051` → next), web (`0.0.205` → next, only if a web test or
`tests/common` changes). No SPA, cli or tauri bump. `CHANGELOG` entry +
`status-log.md` round record. Close dogfood #133 in `dogfood-findings.md` at
unification.
