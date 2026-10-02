# Survey — P4.136: the API-key reads' fallback lines, the unscoped re-resolutions, and the engine thaw (P4.133 review items d + h)

**Date:** 2026-10-01 · **v4:** `f6426e196` (clean) · **v5 `main`:** `6d44cfae2`
**Kind:** read-only measurement. Nothing was built or run. Every path below is
relative to its repo root (v4 = `~/source/quilltap-server`, v5 = this repo).

**The finding in one line.** Item (d) stands as recorded — six v5 key reads
(the review's five plus Carina's older one) fold a database error into "no
key" with no line where v4's 4-arg `safeQuery` logs ERROR `Error finding API
key by ID` / `… by ID and user ID` with `{collection: 'connection_profiles',
keyId[, userId], error}` — and the cheap path folds a SECOND v4 line (the
profile read's `Error finding entity by ID`) with it. Item (h)'s premise is
**REFUTED by measurement**: v4's route `context.repos` is the UNSCOPED
container (`lib/api/middleware/context.ts:116` → `getRepositoriesSafe()` →
`getRepositories()`), so both the Scenario Builder route and the
connection-profile test-message read the UNSCOPED `findApiKeyById`, exactly as
v5 does; what remains of (h) is a *cadence* divergence (v5 reads the key twice
where v4 reads it once and passes it) plus the test-message's read error
answering 500 where v4 logs and answers 404.

---

## §A v4 (the oracle)

### A1 The repository reads (`lib/database/repositories/connection-profiles.repository.ts`)

The repository is `class ConnectionProfilesRepository extends
TaggableBaseRepository<ConnectionProfile>` (`:24`) constructed with
`super('connection_profiles', ConnectionProfileSchema)` (`:26`) — so **every
API-key line carries `collection: 'connection_profiles'`, never `api_keys`**
(the `safeQuery` wrapper injects `this.collectionName`, A2).

| fn | lines | `safeQuery` call (verbatim) | mode |
|---|---|---|---|
| `getApiKeysByUserId(userId)` (⚠ the order calls it `findApiKeysByUserId`; the real name is `getApiKeysByUserId`) | `:218-244` | `'Error finding API keys by user ID', { userId }, []` (`:241-243`); per-row `safeParse` miss WARNs `'API key validation failed'` `{keyId, userId, error}` (`:228-233`) | 4-arg = fallback `[]` |
| `findApiKeyById(id)` | `:249-265` | `collection.findOne({ id })` → `ApiKeySchema.parse(doc)`; `'Error finding API key by ID', { keyId: id }, null` (`:262-264`) | 4-arg = fallback `null` |
| `findApiKeyByIdAndUserId(id, userId)` | `:270-288` | `collection.findOne({ id, userId })` → `ApiKeySchema.parse(doc)`; `'Error finding API key by ID and user ID', { keyId: id, userId }, null` (`:285-287`) | 4-arg = fallback `null` |

All three first `await this.getApiKeysCollection()` (`:205-215`), itself a
**3-arg (rethrow)** `safeQuery(…, 'Failed to get API keys collection', {})`
that runs `ensureCollection('api_keys', ApiKeySchema)`. Two consequences:
(1) a missing `api_keys` TABLE is *healed*, not an error — `ensureCollection`
creates it, `findOne` misses, the answer is a silent `null` (so "no such
table" is NOT a v4-reachable arm); (2) a failure inside the ensure logs TWO
lines (`Failed to get API keys collection` then the outer one).

`ApiKeySchema` (`lib/schemas/profile.types.ts:24-34`): `id`/`userId` UUIDs,
`label: z.string()`, `provider: ProviderEnum`, `key_value: z.string()`,
`isActive: z.boolean().default(true)`, `lastUsed` nullable-optional
timestamp, `createdAt`/`updatedAt` timestamps. A row whose cell fails the
parse THROWS inside the `safeQuery` → the line fires with a Zod message and
the answer is `null`.

### A2 `safeQuery` — the two layers

- `lib/database/repositories/base.repository.ts:76-104`: three overloads; the
  implementation builds `const enrichedContext = { collection:
  this.collectionName, ...context }` (`:101`) and delegates to the standalone
  fn; 4 args = fallback mode (`rest.length > 0`).
- `lib/database/repositories/safe-query.ts:51-72` (⚠ the order's path
  `lib/database/safe-query.ts` does not exist; the file is under
  `repositories/`): `catch (error)` → `logger.error(errorMessage, { ...context,
  error: extractErrorMessage(error), ...(strict ? { strictFailures: true } : {})
  })` (`:64-68`); fallback returned only when `rest.length > 0 && !strict`
  (`:69`), else rethrow. **Level: ERROR. Fields, in order: `collection`, then
  the call's context keys (`keyId`[, `userId`]), then `error`.**
- `extractErrorMessage` (`:23-25`): `error instanceof Error ? error.message :
  String(error)` — the bare message (a SQLite driver sentence, or a ZodError's
  `message`, which is the issues JSON).
- `strictRepositoryFailuresActive()` (`strict-failures.ts`) is entered only by
  the importer (`lib/import/quilltap-import/preview.ts`, `execute.ts` — the
  only two `withStrictRepositoryFailures(` callers); none of this order's
  sites runs inside it.
- **Per-access**: every call logs its own line; there is no once-only gate.

### A3 The five (six) v4 call sites each v5 site twins

| # | v4 site | read | v4 line a failed read logs | what the caller then does |
|---|---|---|---|---|
| 1 | `lib/services/chat-message/participant-resolver.service.ts:236-242` — `if (connectionProfile.apiKeyId) { const apiKeyData = await repos.connections.findApiKeyById(connectionProfile.apiKeyId); if (apiKeyData) { apiKey = apiKeyData.key_value } }` | UNSCOPED `findApiKeyById` | `Error finding API key by ID {collection: 'connection_profiles', keyId, error}` | `apiKey = ''` → the orchestrator's requires-gate |
| 2 | `lib/services/dangerous-content/gatekeeper.service.ts:378-382` — `const apiKey = await getApiKeyForCheapLLMSelection(cheapLLMSelection, userId); if (apiKey === null) { logger.warn('[Gatekeeper] No API key available for classification, failing safe'); return safeFallback }` | `getApiKeyForCheapLLMSelection` → `getApiKeyForConnectionProfile` (`lib/services/api-key.service.ts:23-32`): `repos.connections.findById(profileId)` (UNSCOPED, a fallback `_findById`) then `findApiKeyByIdAndUserId(profile.apiKeyId, userId)` (SCOPED) | **TWO possible lines**: the profile read's `Error finding entity by ID {collection: 'connection_profiles', id, error}` (v5 home: `db::fallback::find_by_id_or_none`), or the key read's `Error finding API key by ID and user ID {collection, keyId, userId, error}` | `null` → the WARN + `safeFallback` |
| 3 | `lib/memory/cheap-llm-tasks/core-execution.ts:309-312` — `const apiKey = await getApiKeyForCheapLLMSelection(selection, userId); if (apiKey === null) { throw new Error('No API key available for cheap LLM provider') }` | the same pair as #2 | the same two lines as #2 | `null` → throw |
| 4 | `lib/services/agent-loop/one-shot-loop.ts:127` `apiKey: string` (an INPUT); destructured `:156`; sent `:219-222` `streamMessage({ …, apiKey, … })` | **none — the loop reads no key**; its callers resolve ONCE: `app/api/v1/scenario-builder/route.ts:63` `const keyResolution = await resolveConnectionProfileApiKey(repos, connectionProfile)` → `:155` `apiKey: keyResolution.apiKey` into `runScenarioBuilder` (`scenario-builder.service.ts:72` `apiKey: string`, `:242` into the loop); `brahma-console/one-shot.service.ts:78` → `:82` `const apiKey = keyResolution.apiKey` → `:160` into the loop | `resolveConnectionProfileApiKey` (`api-key.service.ts:86-102`) calls `repos.connections.findApiKeyById(profile.apiKeyId)` — the caller's `repos`, which for BOTH callers is the UNSCOPED container (A4) → `Error finding API key by ID {collection, keyId, error}` then `{ ok: false, reason: 'api-key-not-found' }` → the route 400s / Brahma `{ok:false, detail}` | the loop never sees a failure |
| 5 | `lib/chat/file-attachment-fallback.ts:434-439` — `if (imageDescProfile.apiKeyId) { const apiKey = await repos.connections.findApiKeyByIdAndUserId(imageDescProfile.apiKeyId, userId); if (apiKey) { apiKeyValue = apiKey.key_value } }` | SCOPED | `Error finding API key by ID and user ID {collection, keyId, userId, error}` | `apiKeyValue || ''` |
| 6 | `lib/services/carina/carina.service.ts:516-519` — `if (connectionProfile.apiKeyId) { const apiKeyData = await repos.connections.findApiKeyById(connectionProfile.apiKeyId); if (apiKeyData) apiKey = apiKeyData.key_value; }` | UNSCOPED | `Error finding API key by ID {collection, keyId, error}` | `''` |

### A4 Item (h): v4's route `repos` are NOT user-scoped — measured

- `lib/api/middleware/context.ts:57-65`: `RequestContext { user: User; repos:
  RepositoryContainer; session }`; built at `:116` `const repos = await
  getRepositoriesSafe();` → `lib/repositories/factory.ts:126-131`
  `getRepositoriesSafe()` → `getRepositories()`; `factory.ts:28` `export type
  RepositoryContainer = DatabaseRepositoryContainer` (the unscoped
  `lib/database/repositories/index.ts:112` container). `getUserRepositories`
  (`lib/repositories/user-scoped.ts:632`) is exported from the factory (`:144`)
  but NOT used by the middleware; its callers are 18 files (api-keys route,
  system/tools, files delete, the backup/import/export family, web search,
  auto-associate, …) — **neither `app/api/v1/scenario-builder/route.ts` nor
  `app/api/v1/connection-profiles/route.ts` is among them** (grep over both
  files and `lib/api/middleware/`: zero hits).
- So **`route.ts:63`'s `resolveConnectionProfileApiKey(repos, …)` reads the
  UNSCOPED `findApiKeyById`.** The route does an ownership check by hand on
  the PROFILE — `:52-56` `if (!connectionProfile || connectionProfile.userId
  !== user.id) … return notFound('Connection profile')` — and none on the
  key. (Its `:68` comment "`repos.characters` is user-scoped" is the same
  false belief v5's `api/scenario_builder.rs:357-360` already records as
  measured false by the route family.)
- **The connection-profile test-message** (`app/api/v1/connection-profiles/
  route.ts:419-432`): `const { user, repos } = context;` … `if (apiKeyId) {
  const apiKey = await repos.connections.findApiKeyById(apiKeyId); if
  (!apiKey) { return notFound('API key'); } decryptedKey = apiKey.key_value;
  }` — UNSCOPED; a failed read logs `Error finding API key by ID` and answers
  `null` → **404 `API key`**. `user` is destructured and unused for the key.
  Siblings in the same file read the same way: `handleCreate` `:252-255` and
  `handleTestConnection` `:360-363`.
- Brahma's one-shot (`one-shot.service.ts:49` `repos: ReturnType<typeof
  getRepositories>`) and Carina (`carina.service.ts:37` imports
  `getRepositories`; `:363` `runBrahmaQuery({ repos, … })`) are unscoped too.
- The user-scoped wrapper, for the record: `lib/repositories/user-scoped.ts:
  250-252` `findApiKeyById(id) { return this.baseRepo.findApiKeyByIdAndUserId(
  id, this.userId) }` — it is what the api-keys ROUTE sees, not these two.

---

## §B v5 on `main`

### B1 The six folding sites

| # | v5 site | read | the fold | v4 twin → the line it should log |
|---|---|---|---|---|
| 1 | `crates/quilltap-core/src/services/participant_resolver.rs:481-489` — `db.read_main(move \|conn\| crate::db::api_keys::find_by_id(conn, &api_key_id)).ok().flatten().map(\|k\| k.key_value)` | UNSCOPED `api_keys::find_by_id` (`db/api_keys.rs:242-254`) | `.ok()` drops the `Err`; the comment at `:474-480` even says "(or a read error — v4's `safeQuery` answers `null`)" — the fold is DOCUMENTED, the line is not | A3 #1 → `Error finding API key by ID {collection=connection_profiles, keyId, error}` |
| 2 | `services/dangerous_content/gatekeeper.rs:543-556` — `db.read_main(move \|conn\| get_api_key_for_cheap_llm_selection(conn, &selection, &uid)).ok().flatten()`; then the WARN `"[Gatekeeper] No API key available for classification, failing safe"` (`:557-561`) | `api_key_service::get_api_key_for_cheap_llm_selection` (`api_key_service.rs:111-125`) → `get_api_key_for_connection_profile` (`:45-60`): `connection_profiles::find_by_id(conn, profile_id)?` then `api_keys::find_by_id_and_user_id(conn, api_key_id, user_id)?` — BOTH `?` propagate as one `Err` | the comment `:539-542` names it: "a read error — v4's repository reads are fallback `safeQuery`s" | A3 #2 → `Error finding entity by ID {collection=connection_profiles, id}` for the profile read (home fn EXISTS: `db::fallback::find_by_id_or_none`), `Error finding API key by ID and user ID {collection, keyId, userId}` for the key read |
| 3 | `services/cheap_llm_exec.rs:704-731` — `resolve_api_key`: `handle.db.read_main(move \|conn\| get_api_key_for_cheap_llm_selection(…)).ok().flatten()` → `None => Err(CompletionError::new("No API key available for cheap LLM provider"))` | the same resolver as #2 | `:715-716` "a read error answers `null` there too, and lands on the same throw" | the same two lines as #2 |
| 4 | `services/agent_loop/one_shot_loop.rs:519-533` — `match deps.db.read_main(move \|c\| Ok::<_, DbError>(resolve_connection_profile_api_key(c, &provider, api_key_id.as_deref()))) { Ok(ProfileApiKeyResolution::Ok(key)) => key, _ => String::new() }` | `resolve_connection_profile_api_key` (`api_key_service.rs:237-259`): `match api_keys::find_by_id(conn, id) { Ok(Some(key)) => Ok(key.key_value), _ => Failed(ApiKeyNotFound) }` — the `Err` and the miss share one arm (`:255-258`) | the `_ =>` arm folds a pool failure, a read `Err` AND a gate refusal to `''`; the comment `:509-518` records the second resolution as forced by "the frozen engine's build request" | v4 has NO read here (A3 #4) — the right fix is to PASS the key (B4); a line here would be one v4 never logs |
| 5 | `services/file_fallback.rs:949-960` `resolve_api_key`: `deps.db.read_main(move \|c\| api_keys::find_by_id_and_user_id(c, &api_key_id, &user_id)).ok().flatten().map(\|k\| k.key_value)`; called ONCE at `:677` `resolve_api_key(deps, profile).await.unwrap_or_default()`; sent `:757` | SCOPED | `.ok()` | A3 #5 → `Error finding API key by ID and user ID {collection, keyId, userId}` |
| 6 | `services/carina_query.rs:371-378` — `db.read_main(\|c\| api_keys::find_by_id(c, &api_key_id)).ok().flatten().map(\|k\| k.key_value).unwrap_or_default()` (predates P4.133; the comment `:368-370` names `carina.service.ts:517`) | UNSCOPED | `.ok()` | A3 #6 → `Error finding API key by ID {collection, keyId}` |

Two more unscoped folds on the same composite, neither in the review's list
but on the (h) path: `api/scenario_builder.rs:336-349` (`.unwrap_or(Failed(
ApiKeyNotFound))` on the pool `Err`; the inner read `Err` folds inside the
composite) and `services/brahma_console/mod.rs:200-210` (`Err(_) =>
Failed(ApiKeyNotFound)`). Both reach v4's OUTCOME (`api-key-not-found` →
400 / `{ok:false}`) without v4's line.

What a corrupt row does on each side (for §C): v5's `marshal_row`
(`db/api_keys.rs:225-237`) binds nine columns by type — `is_active: r.get::<_,
i64>(5)? != 0`, `key_value: r.get(4)?` (a `String`, so a NULL cell is
`InvalidColumnType`) — so a non-integer `isActive` or a NULL `key_value`
answers `Err(DbError::Sqlite(InvalidColumnType…))`; a DROPPED column answers
`no such column`. v4 reads the same row as a document and `ApiKeySchema.parse`
throws a ZodError. **Same line, different `error=` bytes** — the pin must
match the fields and treat `error=` as a prefix (the understudy precedent,
B3). A dropped TABLE is NOT comparable: v4 heals it (A1).

### B2 How the api_key_service resolvers surface a read error today

- `get_api_key_for_cheap_llm_selection` (`api_key_service.rs:111-125`) and
  `get_api_key_for_connection_profile` (`:45-60`) return `Result<Option<
  String>, DbError>` — the `Err` is SURFACED, honestly, to the caller; the
  fold is at the two callers (B1 #2, #3). Note the two reads are of different
  v4 shapes (profile: `_findById`; key: scoped), so the resolver cannot log
  one line for both — the home fns must wrap EACH read.
- `resolve_connection_profile_api_key` (`:237-259`) is infallible by
  signature and folds the read `Err` into `Failed(ApiKeyNotFound)` at
  `:255-258` — the ONE place a home-fn wrap would serve every composite caller
  (the one-shot loop, Brahma, the Scenario Builder prepare, and P4.133's other
  callers) at once.
- The canned seams: `CANNED_CHEAP_LLM_KEY` (`:79-84`) is consulted FIRST in
  `get_api_key_for_cheap_llm_selection` (`:116-118` `if let Some(canned) …
  return Ok(Some(canned))`) — **before any read**, so an armed
  `test_support::CannedCheapLlmKey` (`test_support.rs:356-372`) makes a
  planted row invisible on the cheap path. `CANNED_REQUIRES_API_KEY`
  (`:87-100`; `CannedRequiresApiKey`, `test_support.rs:374-390`) short-circuits
  only the predicate, not a read.

### B3 The existing v5 HOME for this exact shape — and it is NOT in `db/fallback.rs`

`services/dangerous_content/provider_routing.rs:122-143` (P4.124):

```rust
/// v4 `repos.connections.findApiKeyByIdAndUserId` as its callers see it —
/// `safeQuery(…, 'Error finding API key by ID and user ID', { keyId, userId },
/// null)` with the repository's `collection` (`connection_profiles`) injected
/// first — …
fn find_api_key_or_none(read: impl FnOnce() -> Result<Option<ApiKey>, DbError>, key_id: &str, user_id: &str) -> Option<String> {
    match read() {
        Ok(key) => key.map(|k| k.key_value),
        Err(error) => {
            tracing::error!(
                target: "quilltap::db",
                collection = "connection_profiles",
                keyId = %key_id,
                userId = %user_id,
                error = %error,            // ⚠ not `error_text(&error)`
                "Error finding API key by ID and user ID"
            );
            None
        }
    }
}
```

Its two callers: `ConnApiKeys::resolve` (`:104-110`) and `DbApiKeys::resolve`
(`:151-162`), both over `find_by_id_and_user_id`; `try_resolve` on both is
"Never `Err`" (`:112-118`, `:164-167`). It is `fn`, private, and the ONLY
scoped-line emitter; there is NO `Error finding API key by ID` (unscoped)
emitter anywhere in v5 (grep over core/host/web `src`: the literal appears
only at `provider_routing.rs:122` (doc) and `:141`).

⚠ **`error = %error` renders `DbError::Sqlite`'s `Display`, which `db/fallback.rs:24-39`
documents as the `sqlite error: ` prefix v4 never logs** — the smalls round
fixed every `db::fallback` line to render through `error_text(&error)` and
this home, living outside the module, was not swept. Folding it onto
`db/fallback.rs` fixes that for free (and a pin on the bare message, the
`a_sqlite_failure_renders_v4s_bare_message` shape at `fallback.rs:348-365`,
should ride).

**The pins** (`services/dangerous_content/understudy.rs:518-556`,
`a_failed_api_key_read_logs_the_repository_line`): the plant is NOT a corrupt
row — it is (1) an in-memory connection with NO `api_keys` table
(`rusqlite::Connection::open_in_memory()`; the read fails with `no such table`
— a v4-unreachable cause, see A1, used only to force the line), asserting
`hit[0].starts_with("ERROR ") && contains("collection=connection_profiles") &&
contains("keyId=k-1") && contains("userId=u-1")` (the `error=` bytes are NOT
pinned); (2) the pooled form over `db_with_a_failing_read_pool()` (`:410-421`:
open, then `remove_file` the main DB so every checkout fails); (3) the silence
leg: `CREATE TABLE api_keys (…)` then a miss logs nothing (`:545-556`).

`crates/quilltap-core/src/db/fallback.rs` — every home fn and its line:

| fn | line | v4 shape |
|---|---|---|
| `error_text(&DbError) -> String` | `:34-39` | `extractErrorMessage` — `DbError::Sqlite(e) => e.to_string()` (bare), others `to_string()` |
| `find_by_id_or_none(collection, id, read)` | `:43-58` | `Error finding entity by ID {collection, id, error}` |
| `find_all_or_empty(collection, read)` | `:62-75` | `Error finding all entities {collection, error}` |
| `find_by_filter_or_empty` | `:84-97` | `Error finding entities by filter` |
| `find_one_by_filter_or_none` | `:102-115` | `Error finding entity by filter` |
| `joined_file_links_or_empty` | `:124-138` | `Error querying joined file links` |
| `document_by_mount_point_and_path_or_none` | `:144-160` | `Error finding document by mount point and path` |
| `delete_with_gc_or_false` | `:168-182` | `Error deleting file link with GC` |
| `ensure_table_or_log` | `:201-219` | `Failed to ensure {} table in {} database` |
| `ensure_collection_or_log` | `:228-251` | `Failed to ensure collection` + `Failed to ensure collection exists` |
| `seed_built_in_templates_or_log` | `:259-272` | `Error seeding built-in roleplay templates` |
| `sweep_orphaned_store_children_or_default` | `:280-292` | `Error sweeping orphaned store children` |

Conventions (`:18-20`): `target: "quilltap::db"` is v4's `Repository` logger
as the differentials map it (`danger_routing_equivalence.rs:217` `"Repository"
=> "quilltap::db"`); `collection` first, then the context keys, then `error =
%error_text(&error)`.

**The guard** (`crates/quilltap-harness/tests/fallback_home_guard.rs`):
`HOME_MESSAGES` (`:26-40`, twelve literals) must each appear as a whole string
literal in the production zone of `db/fallback.rs` **exactly once** (`:75-80`
`home_seen == HOME_MESSAGES.len()`) and **nowhere else** in core `src`
(`:81-85`, test modules stripped by braces). **`provider_routing.rs:141`'s
literal does NOT trip it today** — neither API-key message is in
`HOME_MESSAGES`. The moment the order adds `"Error finding API key by ID"` and
`"Error finding API key by ID and user ID"` to `HOME_MESSAGES`, the guard
makes `provider_routing.rs:141` an offender: `find_api_key_or_none` must MOVE
into `db/fallback.rs` (two fns: `find_api_key_by_id_or_none(key_id, read)` and
`find_api_key_by_id_and_user_id_or_none(key_id, user_id, read)`, `collection
= "connection_profiles"` hard-coded in each, the understudy pins re-aimed at
the home's callers). The doc comment at `provider_routing.rs:122` quotes the
literal in a `///` line, which the guard's `production_zone` drops (comments
dropped — `:12`), so a doc mention is safe; a message literal is not.

### B4 Item (h)(1): where the Scenario Builder's already-resolved key would travel

The chain today, every struct INTERNAL (none in `api/types.rs` — grep for
`ScenarioBuilderBuildRequest` / `RunScenarioBuilderOptions` in `types.rs`:
zero hits; the wire variant is `Request::ScenarioBuilderBuild { run_id, body:
Value }`, `types.rs:4027-4031`, and must not change):

1. `api/scenario_builder.rs:293-480` `scenario_builder_prepare(db, user_id,
   body) -> Result<(Value, ScenarioBuilderInput), Response>`: resolves the
   composite at `:336-349`, refuses on `Failed` (`:352-354`), and **drops the
   `Ok(key)`** — its own comment `:350-352`: "the one-shot loop resolves this
   same composite over this same row, because the build request (constructed
   by the frozen engine) has no key field". Returns `(profile, input)`
   (`:480`).
2. `api/engine.rs:6566-6587`: `let (profile, input) = scenario_builder_prepare(
   &db, SINGLE_USER_ID, &body)` → `ScenarioBuilderBuildRequest { user_id:
   SINGLE_USER_ID.to_string(), run_id, connection_profile: profile, input,
   web_search_configured, abort }` (`:6580-6587`) → `driver.build(req)`.
   `ScenarioBuilderBuildRequest` is `api/scenario_builder.rs:77-89` (`user_id`,
   `run_id`, `connection_profile: Value`, `input`, `web_search_configured`,
   `abort`) — **the field that would carry the key goes HERE** (`api_key:
   String`), with `scenario_builder_prepare` returning it as a third tuple
   element (or folded into `ScenarioBuilderInput`, `services/scenario_builder/
   mod.rs:125-141` — v4 keeps `apiKey` OUTSIDE `input`, `route.ts:155` beside
   `input: {…}`, so the sibling field is the faithful shape).
3. The host driver `quilltap-host/src/spine.rs:2614` (`impl … ScenarioBuilderDriver`)
   constructs `RunScenarioBuilderOptions { user_id: &req.user_id,
   connection_profile: &req.connection_profile, input: &req.input, now,
   synthetic_chat_id: None }` (`:1874-1880`). The struct is
   `services/scenario_builder/mod.rs:158-169` — a second `api_key: &str`
   field (v4 `scenario-builder.service.ts:72` `apiKey: string`).
4. `services/scenario_builder/mod.rs:431-445` → `RunOneShotToolLoopOptions {
   user_id, chat_id, connection_profile, system_prompt, user_message, tools,
   tool_context, … }` — the struct (`one_shot_loop.rs:148-172`) has NO
   `api_key` field; v4's has (`one-shot-loop.ts:127`). Adding `api_key: &'a
   str` and deleting the re-resolution at `:519-533` (the key is then used at
   `:343` `stream_message_keyed(provider, base_url, api_key, …)` via
   `run_stream` `:299`, `:609`) makes the loop v4's shape.

Constructor census for the thaw (what a required field crosses):
`RunOneShotToolLoopOptions {` — 3 constructions + the destructure
(`brahma_console/mod.rs:313`, `scenario_builder/mod.rs:438`,
`quilltap-harness/tests/brahma_console_tier3_equivalence.rs:865`;
`one_shot_loop.rs:488` destructures); `RunScenarioBuilderOptions {` — 3
(`spine.rs:1874`, `services/scenario_builder/mod.rs:707` (an in-module test),
`quilltap-harness/tests/scenario_builder_tier3_equivalence.rs:550`);
`ScenarioBuilderBuildRequest {` — 1 (`engine.rs:6580`); the five web-test
`impl ScenarioBuilderDriver` (`scenario_builder_routes_equivalence.rs:286`,
`scenario_builder_disconnect.rs:215`, `scenario_builder_midstream_failure.rs:
41,204,230`) RECEIVE the request and need no change.

**The Brahma one-shot caller does NOT pass the key either**:
`services/brahma_console/mod.rs:192-216` resolves the composite (unscoped,
`Err(_) => Failed(ApiKeyNotFound)` `:209`), refuses on `Failed` (`:211-215`),
drops the `Ok(key)`, and the loop re-resolves — the same double read. v4
`one-shot.service.ts:78-82` resolves once and passes `apiKey` (`:160`). So
the thaw is ONE change to the loop's options + TWO callers, not a Scenario
Builder special case.

### B5 Item (h)(2): the connection test-message

`api/settings.rs:2528-2551` `connection_test_message(db, provider, api_key_id,
base_url, model_name, parameters, completion)`:

```rust
let key = match db.read_main(move |conn| api_keys::find_by_id(conn, &akid_owned)) {
    Ok(v) => v,
    Err(e) => return internal(e),      // :2544-2547
};
let Some(key) = key else { return not_found("API key"); };   // :2548-2550
```

- UNSCOPED `find_by_id` — **FAITHFUL** (A4: v4's `repos.connections.
  findApiKeyById` on the unscoped container). The order's "v4 uses user-scoped
  reads from `getUserRepositories`" is false; **no scoping change is owed**.
  (For the record, the scoped twin exists — `api_keys::find_by_id_and_user_id(
  conn, id, user_id)`, `db/api_keys.rs:257-275`, and `api_key_service::
  find_api_key_by_id_scoped` `:282-292` — but `user_id` is NOT in scope at
  this fn: neither `connection_test_message`'s signature nor the trait method
  `ProviderActionsDriver::connection_test_message(&self, profile: Value)`
  (`api/provider_actions.rs:90`, impl `:519-543`) carries one, where the
  sibling `api_key_test` / `model_fetch` do (`:91-103`); the wire variant
  `Request::ConnectionProfileTestMessage { profile }` (`types.rs:184-188`)
  carries none. Scoping it would be a boundary change for a divergence that
  does not exist.)
- **The real divergence is the error arm**: a read `Err` answers
  `internal(e)` (500) where v4's fallback `safeQuery` logs `Error finding API
  key by ID {collection=connection_profiles, keyId}` and answers `null` →
  `notFound('API key')` (404). The same `Err(e) => return internal(e)` shape
  sits on the file's sibling reads at `:1283-1286` (create, v4 `:252-255`),
  `:1576-1579` (test-connection, v4 `:360-363`) and `:2481-2484`; a home fn
  `find_api_key_by_id_or_none` turns each into the 404 + the line (the P4.124
  `Err(_)`→`None` precedent, with the line this time).

---

## §C Families + proof

### C1 Which families run each site, and whether a planted arm can ride

| site | family (`--show` name) | fixture | the v4 side's key resolver | verdict for a planted-corrupt-row arm |
|---|---|---|---|---|
| participant resolver (B1 #1) | `orchestrator_tier3_equivalence` (`QT_ORACLE_ORCHESTRATOR`, `QT_FIXTURE_ORCH_MAIN/MOUNT` built by `build-orchestrator-fixture.ts`); `participant_resolver_tier2_equivalence` (projects NO `apiKey` — `harness/oracle/cases/participant-resolver.ts` has no `apiKey` field) | per-run `/tmp` build | `orchestrator-tier3.test.ts:440-448` mocks `getApiKeyForCheapLLMSelection`/`getApiKeyForProfile → 'test-key'` but NOT `findApiKeyById` — the Salon stream's key is real both sides (the family records per-call keys, `orchestrator_tier3_equivalence.rs:898-1002` `recorded_api_keys`) | a corrupt `api_keys` row breaks every case that resolves that profile — **a unit capture pin is the right home** (a `captured_with` test in `participant_resolver.rs` over a one-row DB with a text `isActive`, asserting the ERROR line's target/fields and `api_key == None`) |
| gatekeeper (B1 #2) | `danger_gatekeeper_tier3_equivalence` (`QT_ORACLE_DANGER_GATEKEEPER`, `QT_FIXTURE_GATEKEEPER`) — compares captured lines (`:413-447`) | per-run build | `danger-gatekeeper.test.ts:154-157` mocks `getApiKeyForCheapLLMSelection: async () => 'test-key'` → v5 arms `CannedCheapLlmKey` | the mock sits ABOVE the read on both sides — a planted row is INVISIBLE unless the case lifts the mock (`requireActual`, the title-update pattern) AND v5 does not arm the seam for that case. Either lift per case or pin at the unit level |
| cheap path (B1 #3) | `title_update_tier3_equivalence` (`QT_ORACLE_TITLE_UPDATE`; committed `quilltap-web/tests/fixtures/cost-background-main.db`) — the `profile_bound_key_sent` / `no_key_refuses` cases are LIFTED (`title_update_tier3_equivalence.rs:631-639`; `title-update-tier3.test.ts:431-434` `liftKeyMock → requireActual`) and v5 does NOT arm the seam there (the file is absent from the `CannedCheapLlmKey` armers list); also `cheap_llm_fallback_equivalence` (arms the seam) | committed pair | lifted on two cases | **the natural differential home**: a third lifted case whose fixture copy corrupts the profile's key row before the run (both sides `UPDATE api_keys SET isActive='x' WHERE id=…` on their copies — the oracle case already holds a `liftKeyMock` switch) proves the line AND the `No API key available for cheap LLM provider` outcome. Mind the committed-fixture rule: corrupt a COPY in the case, never the committed file |
| one-shot loop (B1 #4) | `scenario_builder_tier3_equivalence` (`QT_ORACLE_SBT3`, fixtures from `build-doc-opacity-fixture.ts`; compares targets `…::scenario_builder`, `…::mount_pool`, `…::agent_loop::one_shot_loop` — `:93-97`); `scenario_builder_routes_equivalence` (web; `LOG_TARGETS` `:110-115` = the route/capabilities/SSE targets, an equality over the whole per-case list `:55-61`); `brahma_console_tier3_equivalence` (`QT_ORACLE_BRAHMA`, `QT_FIXTURE_BRAHMA_MAIN/MOUNT`) | per-run builds | NONE of the three v4 cases mocks `api-key.service` (zero hits in `scenario-builder-tier3.test.ts`, `scenario-builder-routes.test.ts`, `brahma-console-tier3.test.ts`) — the key is real both sides | after the thaw the loop reads NOTHING, so there is no line to pin there; the proof is that the KEY the loop sends is the one the gate resolved (the tier-3 family's recorded stream keys — add a `stream_keys` assert if absent) and that `api_keys` is read ONCE per build (a read-count pin, or a plant that corrupts the row AFTER prepare — awkward; the once-read is better proven by deleting the re-resolution and letting the families stay green). The routes family's `quilltap::db` target is NOT in `LOG_TARGETS`, so a prepare-time plant's line must be pinned by a unit test or by adding `quilltap::db` to that family's targets (which would also start comparing every other repository line — measure before widening) |
| file fallback (B1 #5) | `file_attachment_tier3_equivalence` (`QT_ORACLE_FILE_ATTACHMENT`, `QT_FIXTURE_FILE_ATTACH_MAIN/MOUNT`) — compares no log lines (no capture in the file) | per-run build | no mock | unit capture pin in `file_fallback.rs` |
| Carina (B1 #6) | `carina_query_tier3_equivalence` (`QT_ORACLE_CARINA_QUERY`, `QT_FIXTURE_CARINA_MAIN/MOUNT`) — no log capture | per-run build | no mock | unit capture pin in `carina_query.rs` |
| test-message (B5) | `settings_wire_actions` (harness; `QT_FIXTURE_SETTINGS=/tmp/qt-settings-fixture.db` built by `build-settings-fixture.ts`; `test_message_maps_response` `:193-215` composes `settings::connection_test_message` over a `CannedCompletion`) — **no family drives the REST edge of `test-message`** (grep `test-message` over `crates/*/tests`: zero; `settings_routes_equivalence` does not reach it) | per-run build | n/a (v5-only composition) | a new `settings_wire_actions` case: a copy of the fixture with the key row corrupted → `not_found("API key")` (404) + the captured ERROR line (`captured_with` works here: the test calls the handler on its own thread) |

The P4.124 precedent for the SHAPE of the pin: `understudy.rs:518-556`
(B3) — `starts_with("ERROR ")`, `contains("collection=connection_profiles")`,
`contains("keyId=…")`, `contains("userId=…")`, and a silence leg over a table
that reads. For the bare-message rule, `fallback.rs:348-365` pins the exact
line `error=no such table: no_such_table` — the new home fns should carry the
same exact-bytes pin over a corrupt-cell plant (`error=Invalid column type
Text at index: 5, name: isActive` — read rusqlite's rendering before writing
it; `tracing-percent-field-renders-unquoted`: `%` → unquoted).

### C2 Censuses a new home fn or a new engine field could move

| census | file | what it counts | moves? |
|---|---|---|---|
| `fallback_home_guard` | `quilltap-harness/tests/fallback_home_guard.rs` | each `HOME_MESSAGES` literal exactly once in `db/fallback.rs`, zero elsewhere | **YES** — add both API-key literals; `provider_routing.rs:141` becomes an offender until `find_api_key_or_none` moves home (B3) |
| `dispatch_wrong_type_census` | `quilltap-web/tests/dispatch_wrong_type_census.rs` (441; `:2533-2564` the running tally) | typed fields on `api/types.rs`'s `Request` variants | **NO** — the key field lands on `ScenarioBuilderBuildRequest` (`api/scenario_builder.rs:77`), `RunScenarioBuilderOptions`, `RunOneShotToolLoopOptions`; `types.rs` is untouched (B4). Confirmed: `types.rs` references none of the three structs |
| `doc_mount_fallback_sites_census` | `quilltap-harness/tests/doc_mount_fallback_sites_census.rs` | call sites of `METHODS` (`:68-72`: `find_by_mount_point_and_path`, `find_by_mount_point_id`, + two twins) with their class | **NO** — an API-key home fn is none of its four methods |
| `stream_watchdog_wrap_census` | `quilltap-harness/tests/stream_watchdog_wrap_census.rs` | production `.stream_message(` / `.stream_message_keyed(` (`:137-141`) vs `watch_stream(` (`:142`) per file | **NO** — the loop's `:343` call is unchanged; only its `api_key` SOURCE moves |

Also worth a pre-check: `participant_status_home_guard` (another
one-home guard, same scanner) — unaffected, different literals.

---

## §D Traps

1. **The canned seams short-circuit BEFORE the read.** `get_api_key_for_cheap_
   llm_selection` returns the canned key at `api_key_service.rs:116-118`
   before `selection.is_local`, before the profile read. A planted-row arm on
   the cheap path (B1 #2/#3) that arms `CannedCheapLlmKey` proves nothing;
   the gatekeeper family arms it for EVERY case (its v4 twin mocks for every
   case). Lift per case (both sides) or pin at the unit level. The
   `CannedRequiresApiKey` seam is irrelevant to the read but will be armed in
   the same families — it does not hide a read error.
2. **`collection` is `connection_profiles`, not `api_keys`** (A1). A home fn
   that takes `collection` as a parameter invites the wrong value; hard-code
   it, as `provider_routing.rs:136` does.
3. **Two v4 lines on the cheap path, not one** (A3 #2): the profile read is
   `_findById`'s `Error finding entity by ID {collection, id}` (home fn
   exists) and the key read is the scoped line. `get_api_key_for_connection_
   profile`'s two `?`s (`api_key_service.rs:50`, `:58`) must become two home
   wraps with two different lines — the resolver's `Result` signature can then
   go infallible, which changes every caller's `.ok().flatten()`.
4. **`%` sigil / bare message.** `provider_routing.rs:140` writes `error =
   %error` (the `sqlite error: ` prefix); the home convention is `error =
   %error_text(&error)` (`fallback.rs:24-33`). A pin copied from a dogfood
   transcript or written `error="…"` is wrong twice over
   (`tracing-percent-field-renders-unquoted`: `%` renders UNQUOTED under the
   shared `test_support` visitor).
5. **`error=` bytes differ between sides on every reachable plant** (B1,
   last paragraph): v4 logs a ZodError message for a corrupt cell; v5 logs
   rusqlite's. A differential that compares `quilltap::db` lines must
   normalise `error` (the `danger_routing_equivalence.rs:24` precedent:
   "`error` normalised") or the arm is red on bytes v5 cannot match.
6. **A dropped TABLE is not a v4 arm** (A1): `ensureCollection` heals it
   silently. The understudy test's `open_in_memory` plant forces v5's line
   with a cause v4 never logs; fine for a line-SHAPE pin, wrong for a
   two-sided arm.
7. **Per-access cadence.** Every v4 line here fires on EVERY call (`safe-query.ts:
   64`), and v5's reads sit at the same cadence (once per resolve / classify /
   cheap task / description / Carina answer) — EXCEPT the one-shot loop, where
   v5 reads once more than v4 (B4); after the thaw, zero. The cheap fallback
   chain re-enters per selection on both sides (`core-execution.ts:17`), so a
   corrupt row logs once per selection tried.
8. **Thread-scoping** (`a-process-global-test-seam-must-be-thread-scoped`):
   the capture rigs (`test_support::captured_with`, `:123-131`, a
   `set_default` subscriber) see only the calling thread; the gatekeeper
   family's reads run under `rt.block_on` on the test thread (fine); the web
   routes family's reads run on server workers (invisible to
   `captured_with` — it uses its own process-global `StructuredCapture`,
   `scenario_builder_routes_equivalence.rs:61-66`). A unit pin for the loop /
   Brahma / Scenario Builder must call the function directly on the test
   thread.
9. **The thaw's error path.** v4's loop never fails on the key; v5's `_ =>
   String::new()` arm (B1 #4) silently sends `''` when the SECOND read fails.
   After the thaw there is no second read — do not replace the arm with a
   log line v4 does not have.
10. **The (h) premise.** Do not "fix" the two unscoped reads to scoped: v4's
    `context.repos` is unscoped (A4) and the routes' tier-2/route families
    would start diverging (a key owned by another user, readable in v4,
    refused in v5). The P4.133 survey's A1 hedge ("user-scoped where the
    caller passes `getUserRepositories`") resolves to UNSCOPED for both routes.
11. **`fallback_home_guard` counts the home's literal exactly once** — a home
    fn whose doc comment repeats the message in a `///` line is safe (comments
    dropped), but a second `tracing::error!` with the same literal in the same
    file (e.g., a `_scoped` and `_unscoped` variant sharing a message) trips
    `home_seen == len`. The two API-key messages are different strings, so
    two fns are exactly one literal each.

---

## What the order should say

**Tier 1 — the home (core, `db/fallback.rs`):**
- Add `find_api_key_by_id_or_none(key_id, read) -> Option<ApiKey>` (line
  `Error finding API key by ID {collection="connection_profiles", keyId,
  error}`) and `find_api_key_by_id_and_user_id_or_none(key_id, user_id, read)`
  (`… and user ID {collection, keyId, userId, error}`), both `target:
  "quilltap::db"`, `error = %error_text(&error)`; return the whole `ApiKey`
  (callers take `key_value`). Move `provider_routing.rs:122-143`'s body
  onto the scoped home fn (its two callers delegate; the `%error` prefix
  defect closes with it); add both literals to `fallback_home_guard`'s
  `HOME_MESSAGES` red-first (`provider_routing.rs:141` is the designed
  offender); re-aim the understudy pins' expectations only if their bytes
  change (they pin fields, not the error).
- Pins in `fallback.rs`'s tests: the capture + silence legs per fn, and the
  bare-message exact line over a corrupt-cell plant.

**Tier 2 — the six folds (core):** route each read through the matching
home fn — `participant_resolver.rs:481-489` and `carina_query.rs:371-378`
(unscoped line); `file_fallback.rs:949-960` (scoped line); the two cheap
callers via `get_api_key_for_connection_profile` wrapping its profile read in
`find_by_id_or_none("connection_profiles", …)` and its key read in the scoped
home fn (its `Result` becomes infallible; `gatekeeper.rs:543-556` and
`cheap_llm_exec.rs:704-731` lose their `.ok().flatten()`);
`resolve_connection_profile_api_key`'s `:255-258` arm through the unscoped
home fn (serving the loop, Brahma, Scenario Builder prepare and every P4.133
composite caller at once); `api/settings.rs`'s four `Err(e) => return
internal(e)` arms (`:1283`, `:1576`, `:2481`, `:2544`) through the unscoped
home fn so a read error is v4's logged 404. Each with a `captured_with` unit
pin (a one-row DB, a text `isActive`) asserting level/target/fields and the
caller's v4 outcome (`None` / `''` / the WARN / the throw / 404).

**Tier 3 — the thaw (core + host):** `api_key: String` on
`ScenarioBuilderBuildRequest` (`api/scenario_builder.rs:77`) filled from
`scenario_builder_prepare`'s `Ok(key)` (a third tuple element; `engine.rs:
6580` passes it through); `api_key: &str` on `RunScenarioBuilderOptions`
(`services/scenario_builder/mod.rs:158`; `spine.rs:1874` fills it) and on
`RunOneShotToolLoopOptions` (`one_shot_loop.rs:148`; v4 `one-shot-loop.ts:
127`); delete the loop's re-resolution `:509-533`; Brahma passes the key it
resolved at `brahma_console/mod.rs:200-210`. `api/types.rs` untouched
(`dispatch_wrong_type_census` 441 → 441, state it). Proof: the three loop
constructors + the two `RunScenarioBuilderOptions` test constructors updated;
`scenario_builder_tier3`, `scenario_builder_routes`, `brahma_console_tier3`,
`brahma_orchestrator_tier3` green at the pin with no regen (no wire change);
a read-count or recorded-stream-key assert that the loop sends the gate's
key. Record (h)(2) as **NO-PORT on scoping** (v4 unscoped, A4) and close the
500→404 arm under Tier 2.

**Differential rider (optional):** one lifted `title_update_tier3` case over
a corrupted copy of the key row on both sides, with `error` normalised,
proving the cheap path's line + outcome through v4's real resolver.
