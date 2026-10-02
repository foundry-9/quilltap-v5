# Survey — P4.139: the API-key read class (the census + its guard, the api-keys routes, `marshal_row`'s `isActive`, the Scenario Builder prepare's profile read, the three outside composite callers, a host-level keyed pin, the headshoulders arm, the DRY nits)

**Date:** 2026-10-02 · **v4:** `f6426e196` (tree dirty by the three recorded docs paths) · **v5 main:** `cb9ecf256` · **Kind:** read-only measurement — `ggrep` of every `api_keys::` call in core/host/web `src` and of every v4 `findApiKeyById` / `findApiKeyByIdAndUserId` / `getApiKeysByUserId` / `getAllApiKeys` caller in `app/` + `lib/`; reads of each v5 site and its v4 twin; v4's SQLite hydrate (`backends/sqlite/backend.ts`); a `node -e` probe of the hydrate's boolean branch over literal values. Nothing was built or run in either repo.

## The finding in one line

**Every v4 API-key read is a FALLBACK read** — all three repository methods are 4-arg `safeQuery`s (`connection-profiles.repository.ts:218-288`) and no v4 caller reads `api_keys` inside the importer's strict scope — so the census has NO "v4 propagates" class at all: of v5's **42 call sites** (not ~20, not ~38) **13 are already through a home, 25 skip it** (500 / a failed job / a failed generator / a silent fold where v4 logs and answers `null` / `[]`), **2 are not a home-wrap at all but a different divergence** (the pricing context's eager every-turn read; the export's unscoped label-only read), and **2 are repository-shaped wrappers with no production caller**. Four of the 25 need a home that does not exist yet (`Error finding API keys by user ID`, a `db/fallback.rs` §S handoff to P4.142); the api-keys routes have **no scoping divergence** (`getUserRepositories` is a dead import in v4's `[id]/route.ts`); `marshal_row`'s `isActive` diverges in BOTH directions (v5 refuses NULL / text / REAL / BLOB cells v4 hydrates, and reads `2` as `true` where v4's `value === 1` reads `false`) and that divergence reaches the web-search key pick; and `get_api_keys_by_user_id` fails the WHOLE list on one bad row where v4 drops that row with a WARN — which today makes `delete_all` delete NO keys.

---

## §A The census (item 1)

### A1 v4's three reads and their lines (re-verified at `f6426e196`)

`lib/database/repositories/connection-profiles.repository.ts` (repository built with `'connection_profiles'`, so `safeQuery` injects `collection: 'connection_profiles'` first — `base.repository.ts:101`; logger is the root `@/lib/logger`, `:9`; level ERROR, fields `collection`, the call's context keys, `error = extractErrorMessage(error)`, `safe-query.ts:64-68`):

| v4 method | lines | line (verbatim) + context | fallback |
|---|---|---|---|
| `getApiKeysByUserId(userId)` | `:220-244` | `'Error finding API keys by user ID', { userId }` | `[]` |
| ↳ per-row `ApiKeySchema.safeParse` miss | `:226-233` | `logger.warn('API key validation failed', { keyId: (doc as any).id, userId, error: result.error.message })` — a direct WARN, NOT through `safeQuery`, so NO `collection` field; the row is DROPPED, the rest returned | — |
| `findApiKeyById(id)` | `:249-265` | `'Error finding API key by ID', { keyId: id }` | `null` |
| `findApiKeyByIdAndUserId(id, userId)` | `:270-288` | `'Error finding API key by ID and user ID', { keyId: id, userId }` | `null` |

All three first call `getApiKeysCollection()` (`:203-214`), a 3-arg (rethrow) `safeQuery(…, 'Failed to get API keys collection', {})` — on a collection failure v4 logs TWO lines (that one, then the outer). v5 has no collection layer; the outer line alone is the recorded divergence on `MainReads` (`services/api_key_service.rs:43-52`).

The user-scoped wrapper (`lib/repositories/user-scoped.ts:246-274`): `getAllApiKeys()` → `getApiKeysByUserId(this.userId)`; `findApiKeyById(id)` → **`findApiKeyByIdAndUserId(id, this.userId)`** (the SCOPED line); `update/delete/recordApiKeyUsage` pre-check through it. The strict scope (`withStrictRepositoryFailures`) is entered only by the importer (`lib/import/quilltap-import/{preview,execute}.ts`), and **no importer path reads `api_keys`** (v4: zero hits under `lib/import/quilltap-import/`; v5 `services/quilltap_import/profiles.rs:301,445,564` forces `apiKeyId` to null and reads nothing).

**Consequence:** there is no v4 call whose DB error propagates. "PROPAGATE-OK" in the brief's sense is EMPTY by measurement; the guard's allow-list is instead the v5 sites that have no v4 twin or are ruled divergences (§A4).

### A2 v5's existing homes (`crates/quilltap-core/src/db/fallback.rs`, P4.142-owned)

`find_api_key_by_id_or_none(key_id, read)` (`:200-216`, unscoped line) and `find_api_key_by_id_and_user_id_or_none(key_id, user_id, read)` (`:221-243`, scoped line) — `target: "quilltap::db"`, `collection = "connection_profiles"` hard-coded, `error = %error_text(&error)` (the bare driver sentence). **There is NO home for `Error finding API keys by user ID`.** `fallback_home_guard.rs:26-46` lists the two API-key literals.

### A3 The table — every v5 `api_keys::` call site (core/host/web `src`, production zone)

`ggrep -rn 'api_keys::find_\|api_keys::get_'` over the three `src` trees returns 44 lines outside `db/fallback.rs`; two are doc comments (`services/embedding_provider.rs:35`, `services/api_key_service.rs:20`), leaving **42 call sites**. Plus three raw-SQL reads of `"api_keys"` that do not go through the module (§A5). Line legend: **U** = `Error finding API key by ID`, **S** = `… by ID and user ID`, **L** = `Error finding API keys by user ID` (+ the per-row WARN).

| # | v5 site | v5 read | v4 twin | v4 line | v5 on a DB error TODAY | v4 outcome | class |
|---|---|---|---|---|---|---|---|
| 1 | `api/wardrobe.rs:1008-1013` (preview-avatar) | scoped | `app/api/v1/wardrobe/preview-avatar/route.ts:78-84` | S | `Err(e) => internal(e)` → **500** | `null` → **400** `API key for image profile is missing or invalid` | NEEDS-HOME |
| 2 | `api/embedding_profiles.rs:83` `enrich_api_key` (`?`, callers `:184,214,379,684`) | unscoped | `lib/api/middleware/enrichment.ts:51-69` `enrichWithApiKey` (`embedding-profiles/route.ts:52,336`, `[id]/route.ts:37`) | U | `?` → the whole GET/list/create/update **500** (`:214` is inside the LIST loop: one bad key 500s every profile) | `apiKey: null` for that profile, 200 | NEEDS-HOME |
| 3 | `api/embedding_profiles.rs:308` (create) | unscoped | `embedding-profiles/route.ts:300-303` | U | `internal_fixed("Failed to create embedding profile", e)` → 500 | 404 `API key` | NEEDS-HOME |
| 4 | `api/embedding_profiles.rs:501` (update) | unscoped | `embedding-profiles/[id]/route.ts:106-109` | U | `internal_fixed(…update…)` → 500 | 404 | NEEDS-HOME |
| 5 | `api/images.rs:1845-1851` (generate) | unscoped | `app/api/v1/images/route.ts:325-331` | U | `db_error_response(e)` → **500** | `decryptedKey = ''`, continues to the provider | NEEDS-HOME |
| 6 | `api/image_profiles.rs:102` `enrich_api_key` (`?`, callers `:141,389,566`) | unscoped | `enrichWithApiKey` (`image-profiles/route.ts:54,528`, `[id]/route.ts:46,180`) | U | `?` → 500 | `null`, 200 | NEEDS-HOME |
| 7 | `api/image_profiles.rs:328` (create) | unscoped | `image-profiles/route.ts:496-499` | U | `internal(e)` → 500 | 404 | NEEDS-HOME |
| 8 | `api/image_profiles.rs:475` (update) | unscoped | `image-profiles/[id]/route.ts:116-119` | U | `internal(e)` → 500 | 404 | NEEDS-HOME |
| 9 | `api/image_profiles.rs:1056` (models action) | unscoped | `image-profiles/route.ts:146-151` | U | `server_error()` → 500 `Failed to fetch models` | 404 `API key` | NEEDS-HOME |
| 10 | `api/settings.rs:230` `enrich_with_api_key` | unscoped | `enrichWithApiKey` / `[id]/route.ts:42` | U | home | — | ALREADY-HOME |
| 11 | `api/settings.rs:1285-1287` (profile create) | unscoped | `connection-profiles/route.ts:252` | U | home → 404 | — | ALREADY-HOME |
| 12 | `api/settings.rs:1577-1579` (profile update) | unscoped | `connection-profiles/[id]/route.ts:207` | U | home → 404 | — | ALREADY-HOME |
| 13 | `api/settings.rs:2358-2360` (`model_fetch`) | scoped | `app/api/v1/models/route.ts:77` | S | home → 404 | — | ALREADY-HOME |
| 14 | `api/settings.rs:2485-2487` (test-connection) | unscoped | `connection-profiles/route.ts:360` | U | home → 404 | — | ALREADY-HOME |
| 15 | `api/settings.rs:2548-2550` (test-message) | unscoped | `connection-profiles/route.ts:428` | U | home → 404 | — | ALREADY-HOME |
| 16 | `api/settings.rs:2640` `api_key_list` | `get_api_keys_by_user_id` | `app/api/v1/api-keys/route.ts:67-68` (`getUserRepositories(user.id).connections.getAllApiKeys()`) | L (+ per-row WARN) | `internal(e)` → **500**; ONE marshal-failing row fails the whole list (`db/api_keys.rs:288-292` `let key = r?`) | `[]` → 200 `{apiKeys: [], count: 0}`; a bad row is dropped with the WARN and the rest listed | NEEDS-HOME (**new home**, §S.1) + per-row drop |
| 17 | `api/settings.rs:2707` `api_key_update` | unscoped | `app/api/v1/api-keys/[id]/route.ts:95-98` | U | `internal(e)` → 500 | 404 `API key` | NEEDS-HOME (§B) |
| 18 | `api/settings.rs:2744` `api_key_delete` | unscoped | `[id]/route.ts:164-167` | U | `internal(e)` → 500 | 404 | NEEDS-HOME (§B) |
| 19 | `api/settings.rs:2769` `api_key_test` | scoped | `[id]/route.ts:201-205` | S | `internal(e)` → 500 | 404 | NEEDS-HOME (§B) |
| 20 | `generators/external_prompt.rs:335-336` | scoped | `lib/services/external-prompt-generator.service.ts:104` | S | `?` → the generator FAILS | `null` → `''`, generation proceeds | NEEDS-HOME |
| 21 | `generators/optimizer.rs:1991-1993` | scoped | `lib/services/character-optimizer.service.ts:816` | S | `.map_err(db_msg)?` → fails | `''` | NEEDS-HOME |
| 22 | `generators/wizard.rs:741-743` (primary) | scoped | `lib/services/character-wizard.service.ts:728` (+ `:993` the second entry point) | S | `.map_err(db_msg)?` → fails | `''` | NEEDS-HOME |
| 23 | `generators/wizard.rs:807-809` (vision) | scoped | `character-wizard.service.ts:767` (+ `:1032`) | S | fails | `''` | NEEDS-HOME |
| 24 | `generators/ai_import.rs:1535-1537` | scoped | `lib/services/ai-import.service.ts:833` | S | fails | `''` | NEEDS-HOME |
| 25 | `services/file_fallback.rs:954-956` | scoped | `lib/chat/file-attachment-fallback.ts:436` | S | home | — | ALREADY-HOME |
| 26 | `services/chat_enrichment.rs:457` `get_connection_profile` (`?`) | unscoped | `lib/services/chat-enrichment.service.ts:392-397` (`Repos = RepositoryContainer`, `:33` — unscoped) | U | `?` → the enriched-chat read Errs (caller `:510`) | `apiKey: null`, profile still returned | NEEDS-HOME |
| 27 | `services/chat_participants.rs:467` `enrich_with_api_key` (`?`, caller `:512`) | unscoped | `app/api/v1/chats/[id]/helpers.ts:78` `enrichWithApiKey` (`Repos = RepositoryContainer`, `:31`) | U | `?` → Errs | `null` | NEEDS-HOME |
| 28 | `services/dangerous_content/provider_routing.rs:105-109` (`ConnApiKeys`) | scoped | `gatekeeper.service.ts:168`, `understudy.ts:76`, the image jobs / generate tool via `ApiKeyResolver` | S | home | — | ALREADY-HOME |
| 29 | `services/dangerous_content/provider_routing.rs:148-152` (`DbApiKeys`) | scoped | same | S | home | — | ALREADY-HOME |
| 30 | `services/chat_create.rs:2520-2531` (greeting) | unscoped | `app/api/v1/chats/route.ts:700-706` | U | the `_` arm already WARNs `[Chats v1] Connection profile is missing its API key` and returns `NO_GREETING` — **v4's outcome, MINUS the ERROR line** | line, then the WARN | NEEDS-HOME (line only) |
| 31 | `services/embedding_provider.rs:279-283` | scoped | `lib/embedding/embedding-service.ts:118-129` `getApiKeyForProfile` | S | `.map_err(\|e\| ApiError::Plain(e.to_string()))?` → a raw `sqlite error: …` embedding error | `null` → the `No API key found for … embedding profile` refusal | NEEDS-HOME |
| 32 | `services/qtap_export/mod.rs:466` `resolve_api_key_label` → `find_label_by_id` | **unscoped, label column only** | `lib/export/ndjson-writer.ts:97-108` `resolveApiKeyLabel(repos)` with `repos = getUserRepositories(userId)` (`:178,283,413`) → **SCOPED** `findApiKeyByIdAndUserId`, full-row parse | S | `.ok().flatten()` silent | a corrupt row (e.g. BLOB `key_value`) fails the PARSE → line + `undefined` label; v5 reads only `label` and KEEPS it; a foreign-owned key: v4 no label, v5 label | DIVERGENT-OTHER (scoping + column set) |
| 33 | `services/participant_resolver.rs:252-254` | unscoped | `participant-resolver.service.ts:238` | U | home | — | ALREADY-HOME |
| 34 | `services/delete_all.rs:502` `collect_summary` | `get_api_keys_by_user_id` | `lib/backup/restore/delete-service.ts:290-303` (`getUserRepositories`, `:287`) | L | `.unwrap_or_default()` silent | line + `[]` (count 0) | NEEDS-HOME (new home, line only) |
| 35 | `services/delete_all.rs:583` (the delete loop) | `get_api_keys_by_user_id` | `delete-service.ts:375-387` | L | `.unwrap_or_default()` silent; **with one bad row the WHOLE list is `[]` and NO key is deleted** | line on a query error; a bad row is dropped and every OTHER key deleted | NEEDS-HOME (new home) + per-row drop (outcome) |
| 36 | `services/carina_query.rs:872-874` `carina_api_key` | unscoped | `carina.service.ts:517` | U | home (P4.136 item 4) | — | ALREADY-HOME — **no change; P4.140's file untouched** |
| 37 | `services/orchestrator.rs:621-626` `build_pricing_context` | scoped | `lib/llm/pricing-fetcher.ts:309-322` `getApiKeyForProvider` | S | `if let Ok(Some(..))` silent | see §A4 — **a cadence divergence, not a missing wrap** | DIVERGENT-OTHER (P4.140's file) |
| 38 | `services/api_key_service.rs:100-102` (`get_api_key_for_connection_profile`) | scoped | `lib/services/api-key.service.ts:30` | S | home | — | ALREADY-HOME |
| 39 | `services/api_key_service.rs:117` `find_active_api_key_for_provider` (`?`) | `get_api_keys_by_user_id` | callers: host `spine.rs:204-215` `DbProviderKeys` (**no v4 twin** — "no v4 model call scans", its own doc) and host `spine.rs:234-247` `DbSearchApiKeys` → v4 `lib/tools/handlers/web-search-handler.ts:88-111` (`getUserRepositories(userId).connections.getAllApiKeys()`) | L (search only) | both callers `.ok().flatten()` silent; one bad row hides EVERY key | search: line + `[]` → `null`; a bad row is dropped, other keys still found | NEEDS-HOME for the search caller (new home) + per-row drop; NO-V4-COUNTERPART for `DbProviderKeys` |
| 40 | `services/api_key_service.rs:299-301` (`resolve_connection_profile_api_key`) | unscoped | `api-key.service.ts:98` | U | home | — | ALREADY-HOME |
| 41 | `services/api_key_service.rs:323` `get_all_api_keys` | `get_api_keys_by_user_id` | user-scoped `getAllApiKeys` (wrapper twin) | — | returns `Result` | — | WRAPPER — **no production caller** (grep: zero callers) |
| 42 | `services/api_key_service.rs:333` `find_api_key_by_id_scoped` | scoped | user-scoped `findApiKeyById` (wrapper twin) | — | returns `Result`; callers `update/delete/record_api_key_usage_scoped` `:347,361,375` — themselves called only from the module's own test `:562-605` | — | WRAPPER — no production caller |

**Totals:** ALREADY-HOME 13 (#10-15, 25, 28, 29, 33, 36, 38, 40); NEEDS-HOME 25 (#1-9, 16-24, 26, 27, 30, 31, 34, 35, 39-search) of which **4 need the new L home** (#16, 34, 35, 39); DIVERGENT-OTHER 2 (#32, 37); WRAPPER-NO-CALLER 2 (#41, 42).

### A4 The two DIVERGENT-OTHER rows

- **#37 the pricing context (`services/orchestrator.rs:590-631`, P4.140-owned).** v4 reads a key ONLY in `fetchProviderPricing` → `getApiKeyForProvider` (`pricing-fetcher.ts:309-322`): the FIRST profile with `provider === X && apiKeyId`, and only on the refresh path (a cache miss) — v5's fetcher consults a key for OPENROUTER alone (`services/pricing_fetcher/mod.rs:605-611`). v5 instead pre-reads the key of EVERY key-naming profile on EVERY non-Courier turn (`orchestrator.rs:1995` → `build_pricing_context`). **Wrapping `:623` in the scoped home would log, every turn, for keys v4 never reads** (an OPENAI profile's corrupt key; a second OPENROUTER profile; a cache hit). The faithful port moves the read to the fetch point: `PricingContext` carries the profiles plus a lazy scoped resolver (or `db` + `user_id`), and `pricing_fetcher/mod.rs:605-611` resolves the FIRST OPENROUTER key through `find_api_key_by_id_and_user_id_or_none` only when it actually fetches. That edits `orchestrator.rs` (P4.140) and `pricing_fetcher/mod.rs` — **recommend a loud Tier-3 deferral, or a §S hunk for P4.140** (§S.3).
- **#32 the export label.** Faithful = `find_api_key_by_id_and_user_id_or_none(id, user_id, || api_keys::find_by_id_and_user_id(main, id, user_id)).map(|k| k.label)`. Needs `user_id` at `resolve_api_key_label`'s three callers (`services/qtap_export/records.rs` — the `sanitizeProfile` call sites; the export already runs per user). Tier 2.

### A5 Out of the `api_keys::` grep but in the class

`almanack/phase2_machinery.rs:96-101` (`configured_providers`), `:169-175` (`collect_models`), `:236-250` (usage) read `"api_keys"` by raw SQL through `almanack::db::main_rows`. v4 `lib/tools/almanack/phase2-machinery.ts:149-150,189-191` read through `getUserRepositories(userId).connections.getAllApiKeys()` (the L line + per-row drop) and filter `key.isActive` in JS (`:196`); v5 filters `"isActive" = 1` in SQL — which DROPS a NULL / text `isActive` row v4 reads as active (§C). The usage aggregate (`:236`) has no repository twin (v4's own raw SQL there — not measured further). **Tier 3, named** — an almanack lane, not this one.

### A6 The guard — design

**A NEW file `crates/quilltap-harness/tests/api_key_read_sites_census.rs`** (not `fallback_home_guard.rs`, P4.142's), in the shape of `doc_mount_fallback_sites_census.rs` (rows `(file, enclosing fn, method, class)` in source order, an `EXPECTED` table, a `COUNTS` tuple, a synthetic-source scanner test, `QT_CENSUS_PRINT=<file>` to dump measured rows). Specifics:

- **Roots:** `crates/quilltap-core/src`, `crates/quilltap-host/src`, `crates/quilltap-web/src` via `source_census::workspace_rust_sources`; `production_zone` + `code_only` (string literals blanked — these are CALLS, not messages; memory note `a-source-census-needs-a-lexer-and-must-keep-literals` is about the message guards, the inverse case).
- **Methods:** `find_by_id`, `find_by_id_and_user_id`, `get_api_keys_by_user_id`, `find_label_by_id` **only when the path segment before `::` is `api_keys`** (`connection_profiles::find_by_id` etc. share the name — anchoring on the receiver segment is mandatory). A second assertion fails on any `use …::api_keys::{…find_…|get_…}` import (the bare-name escape hatch).
- **Classes:** `home` (the call's statement head contains `find_api_key_by_id_or_none` / `find_api_key_by_id_and_user_id_or_none` / the new L home, or the lane's two helpers §F2); `internal` (inside `db/api_keys.rs`, `db/fallback.rs`, and the bodies of the lane's helpers); `no-v4-counterpart` (`DbProviderKeys::key_for` — a named `OVERRIDES` row); `wrapper-no-caller` (#41, #42 — or delete them, §F); `recorded-divergence` (#37 if deferred, #32 if deferred — each naming its deferral); `fallback-in-v4` — **the conversion list; its count MUST be 0 at lane close** (pinned in `COUNTS`), so a new raw read anywhere is red until its author classifies it.
- **Red-first:** on `main` the census's `fallback-in-v4` count is 25 (21 if the L home is not landed and #16/34/35/39 are recorded as `awaiting-L-home`) — record the red, then green.
- Mutation proof: add a raw `api_keys::find_by_id(conn, id)?` to any route → the census names it.

---

## §B The api-keys routes (item 2)

**v4** `app/api/v1/api-keys/[id]/route.ts`: `import { getUserRepositories }` at `:13` is **NEVER CALLED** (grep: the import line is the file's only hit); every handler destructures `{ user, repos }` from the context, which is the UNSCOPED container (`lib/api/middleware/context.ts:116` → `getRepositoriesSafe()` → `getRepositories()`, measured in the P4.136 survey §A4). So:

- `GET` `:61` `repos.connections.findApiKeyById(id)` — UNSCOPED (v5 has no `api_key_get` verb and no REST route for it; `api/types.rs` carries only `ApiKeyUpdate`/`ApiKeyDelete`/`ApiKeyTest` `:201-217` — out of scope, noted).
- `PUT` `:95` `findApiKeyById(id)` — **UNSCOPED** → `!existingKey` → `notFound('API key')` (`:96-98`).
- `DELETE` `:164` `findApiKeyById(id)` — **UNSCOPED** → 404 (`:165-167`).
- `POST ?action=test` → `handleTestKey` `:201` `findApiKeyByIdAndUserId(id, user.id)` — **SCOPED** → 404 (`:203-205`).
- Each handler's outer `catch` (`'[API Keys v1] Error updating key'` etc. → `serverError(…)`) is **UNREACHABLE for a read error** — the read is a fallback that never throws.

**v5** `crates/quilltap-core/src/api/settings.rs`: `api_key_update` `:2699` reads `api_keys::find_by_id` (UNSCOPED) at `:2707`; `api_key_delete` `:2742` UNSCOPED at `:2744`; `api_key_test` `:2760` SCOPED at `:2769`. **Scoping: IDENTICAL on all three — NO divergence in either direction.** The only divergence is the read-error arm: `Err(e) => return internal(e)` (`:2710`, `:2747`, `:2772`) → 500 `sqlite error: …` where v4 logs U / U / S and answers 404 `API key not found`.

The collection route (`app/api/v1/api-keys/route.ts:67-68`) is the one that IS user-scoped (`getUserRepositories(user.id)`) — v5's `api_key_list(db, user_id)` reads by user id too (#16).

**Proof homes and red-first:**

1. `crates/quilltap-harness/tests/settings_wire_actions.rs` — grow `a_corrupt_key_row_is_v4s_logged_404_on_all_five_key_routes` (`:231-334`, rename to `…_on_all_eight_key_routes`) over its existing `open_db_with_a_corrupt_openai_key()` copy (`:337-364`, BLOB `key_value` on `OPENAI_KEY`): `api_key_update(&db, OPENAI_KEY, Some("x"), None, None)` and `api_key_delete(&db, OPENAI_KEY)` → `{kind: NotFound, error: "API key not found"}` + exactly the unscoped line; `api_key_test(&db, USER_A, OPENAI_KEY, None, &CannedValidator(Ok(true)))` → the same 404 + the SCOPED line (`userId={USER_A}`). **Red-first: 3 arms**, each `Internal "sqlite error: Invalid column type Blob at index: 4, name: key_value"` on `main`. Plus a list arm (`api_key_list(&db, USER_A)` → 200 with the OpenAI key ABSENT, the other fixture keys present, and one `WARN quilltap::db API key validation failed keyId=… userId=… error=…` line) — red-first (500) on `main`.
2. `crates/quilltap-harness/tests/settings_routes_equivalence.rs` + `harness/oracle/cases/settings-routes.test.ts` (two-sided over v4's REAL routes) — a new per-case seed `corruptApiKey: <id>` applied with a raw `UPDATE api_keys SET key_value = x'00000000' WHERE id = ?` on the case's work copy on BOTH sides (the oracle already seeds per case, `:184-197`), and three rows: `ak_update_corrupt` (PUT), `ak_delete_corrupt` (DELETE), `ak_list_corrupt` (GET). ⚠ Do NOT give the PUT/DELETE rows `after: 'apiKeys'` unless the per-row drop (§C2) lands in the same commit — the refetch would list the corrupt row and 500 on v5 for a reason that is not the row's. **Red-first: 3 rows.** `api_key_test` is not in this family (its validator is a seam) — `settings_wire_actions` is its only home.

Mutation: M — `Err(e) => internal(e)` restored on any of the three → its wire arm reds (`Internal` vs `NotFound`).

---

## §C `marshal_row`'s `isActive` and the per-row drop (item 3)

### C1 `isActive`

- **v5** `crates/quilltap-core/src/db/api_keys.rs:225-237`: `is_active: r.get::<_, i64>(5)? != 0`. NULL → `InvalidColumnType` (the whole read `Err`); TEXT → `Err`; REAL (non-integral) → `Err`; BLOB → `Err`; INTEGER `n` → `n != 0`.
- **v4 schema** `lib/schemas/profile.types.ts:30` `isActive: z.boolean().default(true)` — no coercion in Zod. **v4 hydrate** `lib/database/backends/sqlite/backend.ts:412-450`: `isBoolean = this.booleanColumns.has(key) || key.startsWith('is') || …`; then `value === null → undefined` (→ Zod default `true`), `typeof value === 'number' → value === 1`, else `Boolean(value)`. Probe (`node -e` over the branch, literal values): `null → default true`, `0 → false`, `1 → true`, `2 → false`, `-1 → false`, `1.5 → false`, `"x" → true`, `"" → false`, `"0"/"1" → true` (unreachable: INTEGER affinity stores them as integers), a Buffer → `true`.
- **Divergence, both directions:**
  - v5 STRICTER: NULL, non-numeric text, non-integral REAL, BLOB — v5 fails the read (→ the home's line + "no key" on every key read; the WHOLE list on `get_api_keys_by_user_id`), v4 hydrates and USES the key.
  - v5 LOOSER: any integer other than 0/1 — v5 `true`, v4 `false`.
- **Reach:** every key read (the row is refused); and `is_active` itself is consumed by `find_active_api_key_for_provider` (`api_key_service.rs:117-121` — the web-search key pick, v4 `web-search-handler.ts:96-98` `key.isActive`), the masked list/enrich outputs (`settings.rs:2622`, `:234`, `image_profiles.rs:103-108`, `embedding_profiles.rs:84-89`), and the almanack's SQL `= 1` filter (§A5). Low reach — v4 writes only booleans (→ 0/1) — but a foreign or hand-edited row costs v5 the key where v4 sends it.
- **Fix:** decode through `r.get_ref(5)?` with v4's branch: `Null → true`, `Integer(n) → n == 1`, `Real(f) → f == 1.0`, `Text(t) → !t.is_empty()`, `Blob(_) → true`. The struct doc (`:65-66`) and the module doc (`:26`) updated. (Precedent for v4's `=== 1`: `db/chats_read.rs:113`, `db/chat_settings.rs:1381` `is_none_or(|v| v == 1)` for a default-true column.)
- **Pin (two-sided):** `crates/quilltap-harness/tests/api_keys_tier2_equivalence.rs` + `harness/oracle/cases/api-keys.ts` + `harness/oracle/fixtures/{api-keys-tier2.json,build-api-keys-fixture.ts}` (the fixture is built into `/tmp`, NOT committed). The builder already raw-INSERTs one malformed row (`build-api-keys-fixture.ts:73-84`); add raw rows with `isActive` = NULL, `2`, `'x'`, `''`, `1.5`, `x'00'`, and a new op `readIsActive` (by literal id, `findApiKeyById`) whose recorded `{id, isActive}` both sides emit and the Rust side compares. **Red-first: 6 rows** (five `Err` on v5 → no row; `2` → `true` vs `false`). Plus a unit pin in `db/api_keys.rs` over the same six cells (Rust-side bytes).
- ⚠ The P4.136 record's "a text `isActive` is NOT a v4 read error" is RIGHT and stays the reason the key-read plants use a BLOB `key_value`, never `isActive`.

### C2 The per-row drop (`get_api_keys_by_user_id`, `db/api_keys.rs:282-296`)

v4 drops a `safeParse`-failing row with the WARN (§A1) and returns the rest; v5 `let key = r?` fails the whole call on the first marshal error (and its doc `:277-279` calls a corrupt table "not in scope"). Fix: marshal per row; on a row error read the row's `id` through `get_ref(0)` (v4's `(doc as any).id` — `undefined` renders as an absent field) and emit `tracing::warn!(target: "quilltap::db", keyId = …, userId = %user_id, error = %…, "API key validation failed")` (no `collection` — v4's line is not a `safeQuery` line), then continue. This literal lives in `db/api_keys.rs` (not a home message; `fallback_home_guard` unaffected). Reach: #16 (list 500 → 200), #34/#35 (`delete_all` deletes the other keys again), #39 (web search finds the other keys). Pin: `api_keys_tier2` gains a BLOB-`key_value` row for user A and its `getByUser` `expectLabels` stays the healthy set — **red-first** (v5's getByUser aborts) — plus the wire/route arms of §B.

---

## §D The Scenario Builder prepare's profile read (item 4)

- **v4** `app/api/v1/scenario-builder/route.ts:52` `repos.connections.findById(body.connectionProfileId)` → `BaseRepository.findById` → `_findById` (`base.repository.ts:204-205,247-258`): fallback `safeQuery(…, 'Error finding entity by ID', { id }, null)` → ERROR `{collection: 'connection_profiles', id, error}`, then `:53-55` DEBUG `Scenario Builder profile not found for user {profileId}` and 404 `Connection profile not found`.
- **v5** `crates/quilltap-core/src/api/scenario_builder.rs:321-336`: `db.read_main(|c| connection_profiles::find_by_id(c, &pid)).ok().flatten().filter(userId)` — the 404 and the DEBUG are v4's, the ERROR is absent. (`connection_profiles::marshal_cp_row` `db/connection_profiles.rs:713-718` reads `name` as `String`, so a BLOB `name` IS a v5 read error — the plant reaches the arm.)
- **Home:** the EXISTING `db::fallback::find_by_id_or_none("connection_profiles", &pid, || db.read_main(…))` (`fallback.rs:43-58`) — no new home.
- **Proof:** `crates/quilltap-web/tests/scenario_builder_routes_equivalence.rs` (two-sided over v4's REAL route) already compares repository ERRORs on its `repoLines` channel (the P4.D231 BLOB-named group case, header `:21-32`) and installs a process-global `StructuredCapture` over `quilltap::db`. New plant in `harness/oracle/fixtures/scenario-builder-routes.json`: a clone of the tools-on profile with `name = x'00'`; new case "an unreadable connection profile is the repository ERROR, then 404". **Red-first: 1 row, on `repoLines` only** (status, body and the DEBUG already agree). Plus a core unit pin with `crate::test_support::captured_with` (`test_support.rs`; the thread-scoped capture works because `scenario_builder_prepare` is sync on the test thread). Mutation: restore `.ok().flatten()` → the `repoLines` row reds.
- **Adjacent, same file, NOT an API-key read:** the cast read `:369-373` folds a failed `characters_read::find_by_id` silently where v4's `characters.findById` (`route.ts:70-80`) is a fallback that logs `Error finding entity by ID {collection: 'characters', …}`; the family's unreadable-character case pins the ROUTE WARN's absence, not the repository line. P4.142's character-overlay class — flagged, not claimed.

---

## §E The three outside composite callers (item 5)

| site | today | v4 twin |
|---|---|---|
| `services/help_chat/orchestrator.rs:695-707` | `db.read_main(\|c\| Ok(resolve_connection_profile_api_key(c, …))).unwrap_or(Failed(ApiKeyNotFound))` | `lib/services/help-chat/orchestrator.service.ts:217` `resolveConnectionProfileApiKey(repos, connectionProfile)` |
| `services/brahma_console/orchestrator.rs:355-367` | the same shape | `lib/services/brahma-console/orchestrator.service.ts:192` |
| `services/fallback_repos.rs:70-87` `DbFallbackRepos::resolve_api_key` | `self.db.read_main(…)` → `Err(_) => Err(ApiKeyNotFound)` silent | `lib/services/chat-message/provider-failover.service.ts:28` (`resolveConnectionProfileApiKey`) |

On a POOL failure (the checkout itself fails) v5 folds to `ApiKeyNotFound` with NO line; an in-pool read error already logs through the composite's own wrap (`api_key_service.rs:299-301`). v4, for its analogue (a `getCollection` failure inside `getApiKeysCollection`), logs **two** lines: `Failed to get API keys collection {collection: 'connection_profiles', error}` (3-arg, rethrown) then `Error finding API key by ID {collection, keyId, error}`. After the fix (pass `db` — it is `MainReads`, `api_key_service.rs:69-77` — instead of the `read_main(|c| Ok(…))` wrapper) v5 emits the SECOND line only: the divergence `MainReads`' doc already records. Outcome unchanged (`ApiKeyNotFound` → `describe()`).

**Pin shape:** `captured_with` over a `db_with_a_failing_read_pool()` (`understudy.rs:410-421` / `api_key_service.rs:434-446`): `DbFallbackRepos::resolve_api_key` directly → `Err(ApiKeyNotFound)` + exactly one `Error finding API key by ID … keyId=<id> error=<pool message>` line (the `error` bytes are whatever `Db::read_main` renders for a missing file — measure, then pin exactly). **Measured caveat:** in `help_chat/orchestrator.rs` the profile read at `:683-686` runs through the same pool FIRST and propagates (`.map_err(|e| HelpSendError::new(e.to_string()))?`), so the key read's pool-failure arm is unreachable there without a mid-function pool failure — that site's change is structural (proven by the type: the closure is gone), recorded as such. Brahma's caller receives the profile, so its arm is reachable only if nothing earlier touched the pool — measure in-lane; if unreachable, record it the same way. (The help-chat profile read's own fold is a SEPARATE divergence — v4's `findById` at `orchestrator.service.ts:212` is a fallback → `null` → `'Connection profile not found'` + the `_findById` line; v5 answers the raw `DbError` text — P4.142's class, flagged.)

---

## §F The DRY helpers and test-plumbing nits (item 8)

From the `f6426e196` unification §3 item 10 and measurement:

1. **The repeated key-read pattern** — `find_api_key_by_id_or_none(id, || db.read_main(|c| api_keys::find_by_id(c, id)))` at `api/settings.rs:1285,1577,2485,2548`, `participant_resolver.rs:252`, `carina_query.rs:872`, `api_key_service.rs:299`; the scoped twin at `settings.rs:2358`, `file_fallback.rs:954`, `api_key_service.rs:100`, `provider_routing.rs:105,148`. **Two helpers in `services/api_key_service.rs`** (this lane's file — NOT `db/fallback.rs`): `pub fn read_api_key<R: MainReads>(reads: &R, id: &str) -> Option<ApiKey>` and `pub fn read_api_key_scoped<R: MainReads>(reads: &R, id: &str, user_id: &str) -> Option<ApiKey>`, each the home over `reads.read_main_with(…)`. Every NEEDS-HOME conversion of §A3 then becomes one call; the census classifies the helpers' bodies `internal` and their calls `home`. (Carina's `:872` sits in P4.140's file — leave it; it is already correct.)
2. **`MainReads` lives in a service module** (`services/api_key_service.rs:53-77`) though it is a db-layer abstraction. Candidate home `db/runtime.rs` (beside `Db`). Low value, touches every importer — Tier 3 unless the helpers make the move free.
3. **`test_plants` sits in the production file** (`db/fallback.rs:585-642`, `#[cfg(test)] pub(crate) mod test_plants`) — P4.142's file. Natural home: `db/api_keys.rs` (`#[cfg(test)] pub(crate) mod test_plants`), with its four importers re-pointed (`file_fallback.rs:1553`, `participant_resolver.rs:669`, `carina_query.rs:1370` [P4.140], `api_key_service.rs:488-490`) and `fallback.rs:537-539`'s own use (P4.142). **§S.2 handoff**, or Tier 3.
4. **The duplicated `api_keys` DDL** — `fallback.rs:595-598` (`API_KEYS_DDL`), `services/dangerous_content/understudy.rs:554` (a REDUCED, reordered DDL — deliberate? it plants a table without `NOT NULL`s; measure before folding), `services/embedding_provider.rs:664`, `services/api_key_service.rs:391`; plus hand INSERTs at `cheap_llm_exec.rs:3085`, `chat_create.rs:3751`, `dangerous_content/gatekeeper.rs:928`. Fold onto the one `API_KEYS_DDL` (it moves with item 3).
5. **The duplicated `db_with_a_failing_read_pool`** — `understudy.rs:410-421` and `api_key_service.rs:434-446` (byte-identical bodies). Fold onto `test_plants`.
6. **Two wrappers with no production caller** — `api_key_service.rs:319-380` (`get_all_api_keys`, `find_api_key_by_id_scoped`, `update/delete/record_api_key_usage_scoped`), used only by the module's own test `:562-605`. Either delete (YAGNI) or classify `wrapper-no-caller` in the census. Ask; default keep + classify.
7. **An own arm for the headshoulders skip** — §G.
8. **Stale docs:** `db/api_keys.rs:239-241` ("a malformed row is a seam — the corpus never reads a malformed row by id") and `:277-279` ("a corrupt table is not in scope") — both now in scope; `host/src/spine.rs:226-233` (§H item 6).

---

## §G The host-level keyed pin through the Scenario Builder spine (item 6)

- **Path, measured:** `api/scenario_builder.rs:298-353` returns `(profile, api_key, input)`; `api/engine.rs:6580-6588` fills `ScenarioBuilderBuildRequest { …, api_key, … }`; `quilltap-host/src/spine.rs:1874-1881` fills `RunScenarioBuilderOptions { api_key: &req.api_key, … }`; the loop sends it with `stream_message_keyed`. Nothing pins it: `scenario_builder_tier3`'s constructor passes a literal; `brahma_console_tier3_equivalence.rs:320-354,826` records Brahma's keys only.
- **There is no Scenario Builder harness under `crates/quilltap-host/tests/`** (16 files, none drives the spine). The real `quilltap_host::spine::ChatSpine` IS driven end to end by `crates/quilltap-web/tests/scenario_builder_dispatch_wire.rs` through `scenario_builder_spine/mod.rs` (`bundle` `:252-327` builds the real `ChatSpine` and, with no canned driver, hands it as the `scenario_builder` driver `:322-325`). That is the home.
- **The arm:** in `scenario_builder_spine/mod.rs` a `KeyedSceneStream` (the `SceneStream` reply, overriding `stream_message_keyed` to push the key into an `Arc<Mutex<Vec<String>>>` — the default method `model/stream.rs:540-549` ignores the key, and `Arc<T>` forwards it `:571-579`) and a `Canned::KeyedScene` variant exposing the recorder; in `scenario_builder_dispatch_wire.rs` a plant (`planted_instance` `:69-120`): a keyed clone of the tools-on profile with `provider = 'ANTHROPIC'` (a REQUIRING provider) and `apiKeyId` → a planted `api_keys` row `key_value = 'synthetic-sb-spine-key'` (create the table with v4's DDL `IF NOT EXISTS` if the chat-send pair lacks it — the P4.D231 groups precedent); a test running one build and asserting **every** recorded key equals `synthetic-sb-spine-key` and at least one call was recorded.
- **Mutations:** `api_key: ""` at `spine.rs:1877` → the recorded `[""]` reds it; `api_key: String::new()` at `engine.rs:6584` → reds it. (The OLLAMA profile the family uses today resolves `Ok("")` and could not tell.)

## §H The headshoulders skip arm (item 7)

- `services/headshoulders_backfill_job.rs:269-278`: P4.136's approved spill — `get_api_key_for_cheap_llm_selection(db, …)` is infallible, so a read error is the home's line, then `WARN [HeadShouldersBackfill] No API key for cheap LLM selection, skipping`, `Ok(())`. Pinned only by the shared resolver's unit pins + the type change; `headshoulders_backfill_tier3` has no arm for it.
- **Home:** `crates/quilltap-harness/tests/headshoulders_backfill_tier3_equivalence.rs` + `harness/oracle/cases/headshoulders-backfill-tier3.test.ts` + `harness/oracle/fixtures/headshoulders.json` (`handlerCases` `:200-…`). Fixture: the COMMITTED `crates/quilltap-web/tests/fixtures/headshoulders-{main,mount}.db` (copied per case, `:135-136`) — NOT rebuilt; the plant is in-case. New case `corrupt_key` (user 1 — the `Cheap Mock` profile bound to key `a0000082-0000-4000-8000-000000000001`, a seeded character, a `corruptKey: true` spec field) → both sides `UPDATE api_keys SET key_value = x'00000000' WHERE id = '<that id>'` on the case copy. Comparands: `threw: false`, `calls: []`, the existing `[HeadShouldersBackfill]` lines = the WARN alone, and a NEW `dbLines` channel (the `title-update-tier3.test.ts` P4.136 mechanism: spy the registry generation's root `logger.error` filtered to the three key-read messages; Rust side through `global_capture`, `error` normalised — v4 a ZodError dump, v5 rusqlite's sentence) = `[S line]`.
- **Not red-first on `main`** (P4.136 already landed the behaviour); its mutation proof is the pre-P4.136 shape restored (`.map_err(…)?` on the resolver's old `Result` → v5 `threw: true` → reds).
- **Stale header, same file:** `:24-31` "The oracle's `apiKey` field is deliberately NOT a comparand … v5's `CompletionProvider` boundary resolves the key BELOW itself, so no key reaches the seam" — FALSE since P4.133: `generate_one(…, &api_key, …)` (`headshoulders_backfill_job.rs:335-355`) → `generators/wizard.rs:1118-1137` → `send_message_keyed` (`:443`). The key could now be a comparand (Tier 2: a keyed recorder, `apiKey` no longer subtracted at `:408-419`).

---

## What the recorded description got wrong

1. **"A possible scoping divergence" on the api-keys routes (P4.136 Deferred; the unification's OPEN list) — REFUTED.** v4's `app/api/v1/api-keys/[id]/route.ts:13` imports `getUserRepositories` but never calls it; PUT `:95` and DELETE `:164` read the UNSCOPED `findApiKeyById` and the test `:201` the SCOPED one — exactly v5's `settings.rs:2707` / `:2744` / `:2769`. The only divergence is the 500.
2. **"~20 raw `api_keys::find_by_id*` reads still skip the homes" — an UNDERCOUNT.** Measured: 42 call sites; 25 skip a home v4 has (§A3), 4 of them need a home v5 does not have yet; 2 more are a different divergence; 2 are dead wrappers. The brief's "~38" is also short (44 grep lines, 2 of them doc comments).
3. **`services/orchestrator.rs:623` listed as a raw read to wrap — WRONG shape.** It is a CADENCE divergence (§A4): wrapping it would emit lines v4 never emits. Faithful = a lazy read at the fetch point.
4. **`services/carina_query.rs:873` in the census — already home** (P4.136 item 4, `carina_api_key`); no change is owed in P4.140's file.
5. **The "three outside composite callers' pool-failure folds" are not all pinnable at the caller:** help-chat's key-read pool arm is unreachable (its profile read fails first through the same pool and propagates with a NON-v4 message — a separate divergence).
6. **`quilltap-host/src/spine.rs:226-233`** says v4's `getSearchProviderApiKey` "catches, logs, and returns `null`, so a read failure is indistinguishable from 'no key' on both sides" — v4's catch (`web-search-handler.ts:105-110`) is UNREACHABLE for a DB error (`getAllApiKeys` is a fallback that never throws); v4 logs the repository's `Error finding API keys by user ID` (and per-row WARNs), v5 logs nothing.
7. **`headshoulders_backfill_tier3_equivalence.rs:24-31`** — "no key reaches the seam" — stale since P4.133 (§H).
8. **`db/api_keys.rs:277-279`** — "v4's `safeQuery` fallback (`[]` on a query error) maps to the `Err` propagating here; a corrupt table is not in scope" — the fallback IS in scope for four callers, and a corrupt ROW (not table) costs v5 the whole list where v4 drops one row.
9. **The brief's "v5's update/delete read `find_by_id` (unscoped) but v4 may read scoped-by-user"** — measured: v4 reads unscoped too (item 1).
10. Re-verified and RIGHT: every v4 key line carries `collection: 'connection_profiles'`; "a text `isActive` is NOT a v4 read error" (the BLOB `key_value` plant stays); v4 logs TWO lines on the collection-failure arm (the `MainReads` doc).

## Proposed tiered deliverables

### Tier 1 — must land

1. **The two helpers** `read_api_key` / `read_api_key_scoped` in `services/api_key_service.rs` (§F1), with `captured_with` pins (line + silence).
2. **The 21 NEEDS-HOME conversions that need no new home** (§A3 #1-9, 17-24, 26, 27, 30, 31) through the helpers / existing homes, each at v4's outcome: wardrobe 400; images `''`; the profile routes 404 / `null`-enrich (the three `enrich_*` copies folded onto ONE infallible `enrich_with_api_key`, `settings.rs:230`'s, reused by `image_profiles.rs` / `embedding_profiles.rs`); the generators `''`; chat enrichment / participants `null`; the greeting's line before its WARN; the embedding provider's `No API key found …` refusal. Proof per class: `captured_with` unit pins over `test_plants::db_with_api_keys` (`true` = BLOB `key_value`) asserting the exact line AND the caller's outcome; two-sided rows where the family supports an in-case plant (§B for the api-keys routes; the lane decides per family for `image_profiles_routes_equivalence` / `embedding_profiles_routes_equivalence` / `wardrobe_routes_equivalence` / `images_routes_equivalence` — each a red-first row where taken).
3. **The api-keys routes** (§B): the three arms through the homes; `settings_wire_actions` +3 arms (red-first 3); `settings_routes_equivalence` +2 rows (`ak_update_corrupt`, `ak_delete_corrupt`, no `after`) with the `corruptApiKey` seed on both sides (red-first 2).
4. **`isActive`** (§C1): `get_ref` decode; `api_keys_tier2` + `readIsActive` op + six planted cells (red-first 6); a unit pin.
5. **The per-row drop** (§C2) with v4's WARN; `api_keys_tier2` BLOB-row `getByUser` (red-first); the `settings_wire_actions` list arm.
6. **The Scenario Builder profile read** (§D) through `find_by_id_or_none`; `scenario_builder_routes_equivalence` +1 case (red-first on `repoLines`); a unit pin.
7. **The three composite callers** (§E) pass `db`; a `DbFallbackRepos::resolve_api_key` pool-failure pin; help-chat recorded structural.
8. **The census guard** `api_key_read_sites_census.rs` (§A6), red-first on `main` (`fallback-in-v4` = 25), green at 0 (or at the named `awaiting-L-home` rows if Tier 2 item 9 does not land).
9. Mutation proofs: a helper's home call removed → its pins red; `Err(e) => internal(e)` restored on `api_key_update` → the wire arm reds; `!= 0` restored → the `isActive` rows red; `.ok().flatten()` restored in the prepare → the `repoLines` row reds; a raw read added → the census reds.

### Tier 2 — should land

10. **The L home** (§S.1, P4.142 lands the fn + guard literal; this lane stacks on it): #16 `api_key_list` → 200 `[]`; #34/#35 `delete_all`; #39 `DbSearchApiKeys` via a `find_active_api_key_for_provider_or_none` beside the propagating original (host `spine.rs:238` — a host bump), `DbProviderKeys` untouched (`no-v4-counterpart`); the route row `ak_list_corrupt` and the wire list arm's query-error leg.
11. **The host-level keyed pin** (§G) in `scenario_builder_dispatch_wire.rs` + `scenario_builder_spine/mod.rs`, with both mutations.
12. **The headshoulders `corrupt_key` arm** (§H) with the `dbLines` channel; the stale header corrected.
13. **#32 the export label** made scoped + full-row through the scoped home.
14. The DRY folds §F4/§F5 that live in this lane's files (`api_key_service.rs`, `understudy.rs`, `embedding_provider.rs`) once §S.2 decides `test_plants`' home.

### Tier 3 — loud deferrals

15. **#37 the pricing context's cadence** (§A4) — P4.140's file; the hunk shape recorded in §S.3.
16. The almanack's raw `api_keys` SQL and its `isActive = 1` filter (§A5).
17. `MainReads`' move to `db/runtime.rs` (§F2); `test_plants` out of `fallback.rs` if §S.2 is declined (§F3).
18. The help-chat profile read's raw-`DbError` answer and the Scenario Builder cast read's missing `characters` line (§D, §E) — P4.142's class.
19. v4's `GET /api/v1/api-keys/[id]` — no v5 verb (§B); the dead scoped wrappers' deletion (§F6) if the human prefers.

## Files the lane would edit

Source (core):
- `crates/quilltap-core/src/db/api_keys.rs` (`marshal_row` `isActive`; the per-row drop + WARN; docs; optionally `test_plants`)
- `crates/quilltap-core/src/services/api_key_service.rs` (the two helpers; `find_active_api_key_for_provider_or_none` [Tier 2]; tests)
- `crates/quilltap-core/src/api/settings.rs` (`api_key_update` / `_delete` / `_test`; `api_key_list` [Tier 2]; `enrich_with_api_key` made the shared one)
- `crates/quilltap-core/src/api/wardrobe.rs`
- `crates/quilltap-core/src/api/images.rs`
- `crates/quilltap-core/src/api/image_profiles.rs`
- `crates/quilltap-core/src/api/embedding_profiles.rs`
- `crates/quilltap-core/src/api/scenario_builder.rs` (the profile read only)
- `crates/quilltap-core/src/generators/external_prompt.rs`, `optimizer.rs`, `wizard.rs`, `ai_import.rs`
- `crates/quilltap-core/src/services/chat_enrichment.rs` (the key read at `:457` only — see §S.4)
- `crates/quilltap-core/src/services/chat_participants.rs` (`enrich_with_api_key` `:460-475` only)
- `crates/quilltap-core/src/services/chat_create.rs` (`:2520-2531` only)
- `crates/quilltap-core/src/services/embedding_provider.rs`
- `crates/quilltap-core/src/services/delete_all.rs` [Tier 2]
- `crates/quilltap-core/src/services/qtap_export/mod.rs` + `records.rs` [Tier 2]
- `crates/quilltap-core/src/services/help_chat/orchestrator.rs` (`:695-707` only)
- `crates/quilltap-core/src/services/brahma_console/orchestrator.rs` (`:355-367` only)
- `crates/quilltap-core/src/services/fallback_repos.rs`
- `crates/quilltap-core/src/services/headshoulders_backfill_job.rs` (tests only, if a unit pin is added)
- `crates/quilltap-core/src/services/dangerous_content/understudy.rs` (tests only — §F4/§F5) [Tier 2]

Source (host) [Tier 2 only]: `crates/quilltap-host/src/spine.rs` (`DbSearchApiKeys` `:234-247` + its doc `:226-233`).

Harness / web tests (edit or new):
- NEW `crates/quilltap-harness/tests/api_key_read_sites_census.rs`
- `crates/quilltap-harness/tests/settings_wire_actions.rs`
- `crates/quilltap-harness/tests/settings_routes_equivalence.rs`
- `crates/quilltap-harness/tests/api_keys_tier2_equivalence.rs`
- `crates/quilltap-harness/tests/headshoulders_backfill_tier3_equivalence.rs` [Tier 2]
- `crates/quilltap-web/tests/scenario_builder_routes_equivalence.rs`
- `crates/quilltap-web/tests/scenario_builder_dispatch_wire.rs` + `crates/quilltap-web/tests/scenario_builder_spine/mod.rs` [Tier 2]
- the lane's choice among `image_profiles_routes_equivalence.rs`, `embedding_profiles_routes_equivalence.rs`, `wardrobe_routes_equivalence.rs`, `images_routes_equivalence.rs` (harness) for two-sided rows

Oracle cases / fixtures (all `/tmp`-built or JSON spec; **NO committed `.db` pair is rebuilt**):
- `harness/oracle/cases/settings-routes.test.ts` (the `corruptApiKey` seed + 3 rows)
- `harness/oracle/cases/api-keys.ts`, `harness/oracle/fixtures/api-keys-tier2.json`, `harness/oracle/fixtures/build-api-keys-fixture.ts`
- `harness/oracle/cases/scenario-builder-routes.test.ts`, `harness/oracle/fixtures/scenario-builder-routes.json`
- `harness/oracle/cases/headshoulders-backfill-tier3.test.ts`, `harness/oracle/fixtures/headshoulders.json` [Tier 2]

Families REGENERATED (from the round pin): `settings_routes_equivalence`, `api_keys_tier2`, `scenario_builder_routes`, `headshoulders_backfill_tier3` [Tier 2], plus any profile-route family the lane grows. Families RUN neutral: `settings_wire_actions`, `fallback_home_guard`, `doc_mount_fallback_sites_census`, `dispatch_wrong_type_census`, `character_wizard_tier3`, `character_optimizer_tier3`, `external_prompt_tier3`, `ai_import_tier3`, `embedding_provider_tier3`, `initial_greeting_equivalence`, `chat_create_capstone`, `embedding_profiles_routes`, `image_profiles_routes`, `images_routes`, `wardrobe_routes`, `brahma_console_tier3`, `brahma_orchestrator_tier3`, `scenario_builder_tier3`, `scenario_builder_dispatch_wire`, `web_search_tool_equivalence`, `title_update_tier3`, `participant_resolver_tier2`.

Docs: `docs/CHANGELOG.md`, `docs/developer/porting/status-log.md` (append-only), the order's own header.

## Files the lane must READ but not edit

- `crates/quilltap-core/src/db/fallback.rs`, `crates/quilltap-harness/tests/fallback_home_guard.rs` (P4.142 — §S.1/§S.2)
- `crates/quilltap-core/src/services/orchestrator.rs`, `services/carina_query.rs`, `services/build_context.rs` (P4.140 — §S.3)
- `crates/quilltap-core/src/services/pricing_fetcher/mod.rs` (§A4 — only if §S.3 is taken)
- `crates/quilltap-core/src/api/engine.rs` (`:6560-6590` — read for §G, NOT edited)
- `crates/quilltap-core/src/db/connection_profiles.rs` (`marshal_cp_row`), `crates/quilltap-core/src/test_support.rs`, `crates/quilltap-harness/tests/source_census/mod.rs`, `doc_mount_fallback_sites_census.rs` (the census shape)
- `crates/quilltap-core/src/almanack/phase2_machinery.rs` (§A5)
- v4: `lib/database/repositories/{connection-profiles,base}.repository.ts`, `safe-query.ts`, `lib/repositories/user-scoped.ts:240-280`, `lib/database/backends/sqlite/backend.ts:395-460`, `lib/schemas/profile.types.ts:24-34`, `app/api/v1/api-keys/{route.ts,[id]/route.ts}`, and each twin in §A3.

## Cross-lane adjacencies / risks

- **§S.1 → P4.142 (owns `db/fallback.rs` + `fallback_home_guard.rs`): a NEW home** — `pub fn find_api_keys_by_user_id_or_empty(user_id: &str, read: impl FnOnce() -> Result<Vec<super::api_keys::ApiKey>, DbError>) -> Vec<super::api_keys::ApiKey>` logging `tracing::error!(target: "quilltap::db", collection = "connection_profiles", userId = %user_id, error = %error_text(&error), "Error finding API keys by user ID")` and answering `Vec::new()`; plus `"Error finding API keys by user ID"` in `HOME_MESSAGES` (`home_seen` +1) and its capture + silence pin. If P4.142 lands it, this lane stacks Tier 2 item 10 on it (the "stack a rider on a sibling's branch" pattern); otherwise item 10 is a named deferral and the census records #16/34/35/39 as `awaiting-L-home`.
- **§S.2 → P4.142: `test_plants`' home.** Moving `db::fallback::test_plants` into `db/api_keys.rs` touches `fallback.rs:537-539,585-642` (P4.142) and `carina_query.rs:1370` (P4.140, a test-module import line). Either P4.142 does the move, or the lane leaves it and the DRY folds stay Tier 3.
- **§S.3 → P4.140: the pricing context** (`orchestrator.rs:590-631` + `pricing_fetcher/mod.rs:605-611`). Recorded as Tier 3 here; if P4.140 wants it, the hunk is: drop the `api_keys` pre-read loop `:616-626`; give `PricingContext` a lazy scoped resolver; resolve the FIRST `OPENROUTER` `apiKeyId` through `find_api_key_by_id_and_user_id_or_none` inside the fetch arm. Neither `orchestrator.rs:623`'s file nor `carina_query.rs` is edited by this lane (`carina_query.rs:873` needs nothing).
- **§S.4 → P4.142's `chatGet`/`listChats` item:** `services/chat_enrichment.rs` (`get_connection_profile`, `:448-470`) and `services/chat_participants.rs` (`:460-475`) sit on the chat-GET path P4.142 is reworking for the renamed mount-index column. This lane touches ONLY the two key reads (and leaves the `connection_profiles::find_by_id(…)?` at `chat_enrichment.rs:452` — P4.142's fallback class). Coordinate the hunks; the unifier's pick order should land P4.142 first.
- **P4.140 (`fileProcessing` frame) / P4.141 (model layer):** no shared file. P4.141's non-streaming timeout wording may touch the generators' completion paths — this lane touches only their key reads.
- **P4.143 (`dumpFileFacts`, import mask, restore serde):** no shared file; `delete_all.rs` is near the restore family but not in its list.
- **P4.144:** `harness/tools/recipe_sweep.py` (`ALIAS_ASSIGN`) — the new/regenerated families' recipe headers must use the sanctioned `V5W=${V5W:-$HOME/source/quilltap-v5}` literal so its `--self-test` stays green.
- **Census trips (memory `a-new-harness-test-can-trip-a-source-census`):** the new census scans the same trees as `fallback_home_guard` and `doc_mount_fallback_sites_census`; neither should move (no new home literal outside `fallback.rs`; no doc-mount read touched) — run both. `dispatch_wrong_type_census` unmoved (no `api/types.rs` change).
- **`settings_routes_equivalence`'s `after` refetch** (§B) — the ordering trap between the corrupt rows and the per-row drop.

## Versions / bumps

- **core** — yes (every Tier 1 item).
- **harness** — yes (the census, `settings_wire_actions`, `settings_routes_equivalence`, `api_keys_tier2`, headshoulders).
- **web** — yes (tests only: `scenario_builder_routes_equivalence.rs`; Tier 2 `scenario_builder_dispatch_wire.rs` + `scenario_builder_spine/mod.rs`).
- **host** — only if Tier 2 item 10's `DbSearchApiKeys` change lands (`spine.rs`); otherwise none.
- cli, tauri, fixture-sanitizer, SPA — none. `quilltap-sqlite3mc-sys` — never.
