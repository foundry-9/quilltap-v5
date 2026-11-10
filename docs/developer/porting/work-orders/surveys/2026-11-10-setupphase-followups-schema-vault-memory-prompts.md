# Survey B — schema / vault keys / character kind / memory / prompts / search / Brahma lookups (the 2026-11-10 follow-ups smalls round)

Read-only. v5 main @ `8ad1a0640`; v4 @ the pin `624d0d25c` (`git show` / `git grep` only). Orders covered: P4.D341, P4.D342, P4.D349, P4.D351, P4.D362, P4.D363, P4.D369, P4.D370, P4.D371, P4.D372, P4.D373, P4.D374, P4.D375, P4.168 (+ P4.D350's one finding), the `phase-4.md:7486-7496` "Vault / templates / memory" list, and the three named rows of `status-log.md:202711-202729`.

**Tally:** OPEN 13 · CLOSED 3 · NO-ACTION 13 · NEEDS-RULING 6 (the 2 assigned + 3 file / don't-file drafts + 1 scope question inside B-5) · DEFER-TO-CATCH-UP 13.

Nothing was built or run. Every "measured" below is a read of the source / the pin; anything a lane must still measure by running is marked **MEASURE**.

---

## ⚖ Ruling 1 — `NON_BOOLEAN_CHARACTER_AGENT_DEFAULT` (confirm the recorded divergence?)

- **v4 at the pin:** `lib/services/chat-message/agent-mode-resolver.service.ts:71-74` — `if (character?.defaultAgentModeEnabled !== null && … !== undefined) { enabled = character.defaultAgentModeEnabled; enabledSource = 'character' }`. No type test, so a hand-built `{defaultAgentModeEnabled: "yes"}` yields `enabled: "yes"`, `enabledSource: 'character'`. The project tier (`:77-80`) and the chat tier (`:83-86`) have the same shape.
- **v5:** `crates/quilltap-core/src/services/agent_mode.rs:151-157` — `character.get("defaultAgentModeEnabled").and_then(Value::as_bool)`; a non-boolean is "unset" and the lower tier stands. `ResolvedAgentMode.enabled` is a Rust `bool`; the project / chat tiers arrive as `Option<bool>` (`:140-141`).
- **The pin:** `crates/quilltap-harness/tests/agent_mode_flavor_equivalence.rs:29-45` (the const, seven ids `roleplay-- / pre-kind-- / assistant-- / robot-- / capital-assistant-- / kind-null-- / kind-five--character-yes`), compared both ways at `:128-147` (v4 must still show `enabled: "yes"` + `character`; v5 must show `global` + `false`; `flavor` / `maxTurns` compared strictly), count asserted `:194`. Oracle row source: `harness/oracle/cases/agent-mode-flavor.ts:85-86` (`characterDefault: 'yes'` — a hand-built object passed straight to the resolver, no repository).
- **Reachability in v4 production (read, not run):** NOT reachable. `defaultAgentModeEnabled` is a `characters` ROW column (`lib/schemas/character.types.ts:209` `z.boolean().nullable().optional()`; PUT schema `app/api/v1/characters/[id]/handlers/put.ts:61` the same; it is not a vault-managed field). Every repository read hydrates it through `SQLiteCollection.hydrateRow` (`lib/database/backends/sqlite/backend.ts:446-453`): NULL → `undefined`, a number → `=== 1`, anything else → `Boolean(value)`. So a stored TEXT `'yes'` reaches the resolver as `true`, never as `"yes"`; and a non-boolean that did survive would fail `CharacterSchema`. Only the tier-1 driver can produce the string.
- **What converging would change:** `ResolvedAgentMode.enabled` (and the `[AgentMode] Resolved agent mode` INFO's `enabled`, the chat GET / toggle wire) would have to carry an arbitrary JSON value instead of a `bool`, through `api/salon.rs:321`, `services/chat_admin.rs`, turn extras and the tool slate — for a shape no v4 read can produce.
- **Adjacent, unmeasured (not part of the ruling):** the REAL-data twin is a TEXT / REAL cell in that INTEGER column. v4 answers `Boolean(cell)`; v5's `characters_read.rs:197` `put_opt_bool(row.get(12)?)` is a typed `Option<bool>` read (a TEXT cell is a rusqlite type error). Unreachable from any v4 writer; sits on the character row reader `1dc11e947` re-keys → leave to the catch-up.
- **Recommendation: CONFIRM — keep the divergence, keep the both-ways pin.** No work.

## ⚖ Ruling 2 — `put_zero_one_flag` TEXT `'0.0'` → `true` (file the v4 issue or not?)

- **v5 (CONVERGED, nothing to change):** `crates/quilltap-core/src/db/chats_messages_read.rs:172-186` (`Text(s) => !s.is_empty()`, `Blob => true`, `Integer == 1`, `Real == 1.0`, NULL → key omitted); the write codec `db/chats_messages.rs:1202` `zero_one_stored` (`Some(false)` → `0.0`). Pinned: `harness/oracle/fixtures/chats-messages-read-tier2.json:217-225` row `e1000001-0000-4000-8000-0000000362f0` (explicit `false` for both flags, read back `true` on both sides); `chats-messages-tier2.json` stores `'1.0'` / `'0.0'`. Lane record `status-log.md:198817-198820`.
- **v4 at the pin:** the row schema `lib/database/repositories/chats-messages.ops.ts:76-77` declares both flags `z.union([z.boolean(), z.number().transform(v => v === 1)]).nullable().optional()`; generateDDL renders a union as bare `TEXT` (v5's dump: `crates/quilltap-core/src/services/provisioning/fresh_schema.json` — `"isSilentMessage" TEXT`, `"isEstablishedFact" TEXT`), while the migrations add `INTEGER DEFAULT NULL` (`migrations/scripts/add-silent-message-field.ts:48`, `add-established-fact-field-v1.ts:42`). The hydrator `lib/database/backends/sqlite/backend.ts:413-453`: both names start with `is` → boolean column → a string cell → `Boolean(value)` → `'0.0'` is truthy.
- **Reach (read, not run):** v4's own writers never store an explicit `false` — `message-finalizer.service.ts:539` (`… || undefined`), `:740` (`… || null`), `transcript-projection.ts:238-239` (`event.is… || null`), `apply-chat-continuation.ts:237-238` (`source.is… ?? null`, a re-read value). So the bug needs a carrier or caller that writes `false` (`addMessage` with an explicit `false`, an import of a hand-edited bundle) on a generateDDL-FRESH instance. Latent; Low. v5 shares it by convergence on a v5-provisioned (fresh-shape) instance; v5 moves when v4 fixes it.
- **No draft text exists** (the lane record and the order header say "drafted" but hold only the finding paragraph — grep of `docs/` and `work-orders/surveys/` for `Boolean('0.0')` finds the status-log, the ledger, phase-4 and the order header, no issue body). A draft follows.
- **Recommendation: FILE** (cheap, v4 is the oracle and the fix is one line there). The only ruling is file / don't file; no v5 work either way.

### DRAFT (NOT filed) — for `foundry-9/quilltap-server`, the Bug form

> Proposed form fields: Severity **Low**, Provenance **Original to v4**, v5 status **Converged (v5 reproduces it; moves when v4 fixes it)**. Measured at v4 `624d0d25c` (`4.13.0-dev.8`) through v4's REAL `addMessage` → `getMessages` in the port's tier-2 oracle (`chats-messages-read-tier2`, P4.D362).

**Title:** On a freshly created instance, a chat message stored with `isSilentMessage: false` or `isEstablishedFact: false` reads back `true`

**What happened**

- `lib/database/repositories/chats-messages.ops.ts:76-77` declares both flags as `z.union([z.boolean(), z.number().transform(v => v === 1)])`. The DDL generator renders a union as a bare `TEXT` column, so an instance whose `chat_messages` table was created by `ensureCollection` (not by the migrations, which add `INTEGER DEFAULT NULL`) has TEXT affinity on both columns.
- A stored `false` is bound as the number `0` and lands in the TEXT column as the string `'0.0'` (a `true` lands as `'1.0'`).
- On read, `SQLiteCollection.hydrateRow` (`lib/database/backends/sqlite/backend.ts:446-453`) treats both as boolean columns (their names start with `is`): a number compares `=== 1`, anything else falls to `Boolean(value)`. `Boolean('0.0')` is `true`.

So an explicit `false` round-trips to `true` on a fresh-shape table and to `false` on a migrated one.

**Steps to reproduce**

1. Boot a new instance (the table is created by generateDDL).
2. `addMessage` a row with `isSilentMessage: false, isEstablishedFact: false`.
3. `getMessages` the chat.

**Measured output**

`PRAGMA table_info(chat_messages)`: both columns `TEXT`. Stored cells `'0.0'` / `'0.0'`. Read back: `isSilentMessage: true`, `isEstablishedFact: true`.

**Expected output**

`false` for both (as on a migrated instance, where the cells are INTEGER `0`).

**Notes**

- Latent today: the app's own writers store `true` or NULL, never `false` (`message-finalizer.service.ts:539` / `:740`, `transcript-projection.ts:238-239`, `apply-chat-continuation.ts:237-238`), so it needs an import / restore / direct caller that carries an explicit `false`.
- A one-line fix in the hydrator's fallback arm (parse a numeric string before `Boolean`), or declaring the two columns so the generator renders `INTEGER`, closes it. The second also removes the fresh-vs-migrated affinity split.

---

## ⚖ Three more file / don't-file drafts sitting in my orders (text exists, none filed)

| # | Draft | Where the text is | Recommendation |
|---|---|---|---|
| F-1 | P4.D341 R-3: `POST /api/v1/memories` answers 400 AFTER writing the memory when the post-admission keystone mark is refused (race only; staged with an `AFTER INSERT` trigger) | `status-log.md:196599-196607` | FILE (Low). v5 carries v4's bytes; pinned by three rows. |
| F-2 | P4.D363 R-Z: `isCharacterAuthoredMessage` (truthiness) vs `chatMessageFtsEligibilitySql` (`IS NULL`) disagree on empty-string authors | `status-log.md` "v4 filing CANDIDATE (R-Z…)" under the P4.D363 record (~`:199027`) | FILE (Low, latent). Retires `EMPTY_AUTHOR_TRUTHINESS_VS_IS_NULL`. |
| F-3 | P4.D375 `LEXICAL_WINDOW_VS_DATE_PARSE`: `findIdsInEventWindow` compares event times as TEXT; a schema-valid no-millis `occurredAt` on a window's last second is left out | `status-log.md` ~`:200276` (one sentence — needs expanding to the Bug form) | FILE after the 💸-free count on a Friday copy (the lane's R-3 `GLOB` query) — or file now as latent. |

---

## OPEN

### B-1 — P4.D351 item 16: the whole-entity `RoleplayTemplateSchema` twin (the other twelve keys)   [OPEN, M]
- Source: P4.D351 header OPEN ("PARTIAL"); lane record `status-log.md` P4.D351 "Item 16"; order item 16 (`work-orders/p4.d351-*.md:1024-1042`).
- v5 site: `crates/quilltap-core/src/api/roleplay_templates.rs:165` `template_carrier_keys` validates only `narrationDelimiters` / `unmarkedStyle` / `markdownAsterisks`; the carriers keep serde coercion for the rest — `services/quilltap_import/entities.rs:482-512` (+ `:307-330`), `services/backup/restore/orchestrator.rs` step 11 (~`:1085-1131`). No `zod_roleplay_template_issues` exists (`api/zod_issues.rs` has `zod_prompt_template_issues:1435`, `zod_unmarked_style_issues:1497`).
- v4 site: `lib/schemas/template.types.ts:287-316` (`id, userId, name(min1,max100), description(max500, nullable), systemPrompt(min1), isBuiltIn, tags(uuid[]), delimiters(TemplateDelimiterSchema[]), renderingPatterns(RenderingPatternSchema[] + the `className` refine), dialogueDetection, narrationDelimiters, unmarkedStyle, markdownAsterisks, createdAt, updatedAt`).
- Done: `zod_roleplay_template_issues(row)` in schema key order with v4's `ZodError.message` bytes as the warning tail, run by the `.qtap` import and BOTH restore arms before the write (the P4.161 shape); the `Broken Template` row of `SERDE_ARM_DIVERGENCES` RETIRES (`crates/quilltap-harness/tests/system_import_state.rs:718-723`, asserted again `:2634`; the count asserts `:2693` / `:2838` move 8 → 7).
- Differential: `system_import_state` (`execute_named_item_failures`; oracle `harness/oracle/cases/system-import-execute.test.ts:1704`), `system_restore_state` / `system_restore_equivalence` (the derived Novel archive), unit tests per issue shape.
- Traps: Zod 4.6.7 `.max()` counts code points (memory note `carrier-oracle-measurement-gotchas`); v4 logs `Data validation failed` + rethrow + `Error creating roleplay template` (already ported for the three keys — reuse); `restore-census-whitelist` if a new restore line appears; `zod_issues.rs` is append-shared (a new `ZodIssue` variant needs arms in `path()` / `message()` / `zod_issue_aborts`).

### B-2 — P4.D351 Tier 3: POST's `Invalid delimiters: malformed` for every delimiter parse failure   [OPEN, M]
- v5 site: `crates/quilltap-core/src/api/roleplay_templates.rs:745-762` (`serde_json::from_value::<TemplateDelimiter>` → `Err("malformed")`; only the wrap-flag type miss answers Zod's sentence).
- v4 site: `app/api/v1/roleplay-templates/route.ts:107-110` — `z.array(TemplateDelimiterSchema).optional().safeParse(body.delimiters)` → `` `Invalid delimiters: ${issues[0]?.message ?? 'malformed'}` ``.
- Done: a `zod_template_delimiter_issues` twin (the discriminated union's first issue: not-an-array, bad `kind`, missing / empty `name`, `buttonName`, `delimiters` string-or-pair, `style`) answering issue 0's message. B-1 needs the same twin for the `delimiters` key — ONE change with B-1.
- Differential: `roleplay_templates_routes_equivalence` (+ rows; oracle `harness/oracle/cases/roleplay-templates-routes.test.ts`, jest over the groups-projects pair via the `/tmp` mirror — recipe in the test header `:9-23`).
- Trap: v5 defaults a missing `kind` to `wrap` before the parse (`:745-748`) — MEASURE whether v4's discriminated union does the same or refuses.

### B-3 — P4.D351 Tier 3: the PUT schema's OLDER keys have no parse arm   [OPEN, M]
- v5 site: `api/roleplay_templates.rs:470-500` (the comment "The older keys' parse arms are not ported"); only the three P4.D351 keys push `parse_issues`.
- v4 site: `app/api/v1/roleplay-templates/[id]/route.ts:22-38` `updateRoleplayTemplateSchema` (`name` min1 max100, `description` max500 nullable, `systemPrompt` min1, `tags` `z.uuid()[]`, `delimiters` `TemplateDelimiterSchema[]`, `renderingPatterns` `z.any()[]`, `dialogueDetection` any) → `.parse(body)` at `:101` → the middleware's `{error:'Validation error', details}`.
- Done: the seven older keys' issues appended in schema key order through `Response::validation_error` before any write. Shares B-2's delimiter twin.
- Differential: `roleplay_templates_routes_equivalence` (the `update_*` rows).
- Trap: the existing post-regen "repository re-validate" already answers `invalid_type` / `too_small` for `name` / `systemPrompt` — a parse arm now fires FIRST and must not double-report; `z.uuid()` is the RFC gate (`zod_uuid_ok`).

### B-4 — `PATTERNS_NOT_ARRAY_CREATE`: a non-array `renderingPatterns` on POST   [OPEN, S]
- v5 site: `api/roleplay_templates.rs:351` (`unwrap_or_default()` → regen → 201). v4: `route.ts:122-123` — `body.renderingPatterns || []`; `"x".length === 1` → no regen → the repository's Zod parse fails → 500 `Failed to create roleplay template`.
- Done: either converge (500 with v4's lines) or the named both-ways carve with ONE measured row `create_patterns_not_array`. Recommend converge — it rides B-1's `renderingPatterns` arm. Same family as B-2.

### B-5 — `CHATS_BY_CHARACTER_RETHROW_LINE_ABSENT` + the `?` callers of `chats_read::find_by_character_id`   [OPEN, M — with a scope question]
- Source: P4.D375 Tier 3 item 23 (order `:895`), lane record carve; `services/chat_window.rs:10-22`.
- v4 site: `lib/database/repositories/chats.repository.ts:178-189` — `findByCharacterId` = a RETHROW-mode `safeQuery('Failed to find chats by character ID', {characterId})` around `findByFilter`, which is itself a FALLBACK read (`base.repository.ts:283-297`: `Error finding entities by filter {collection, error}` → `[]`). So OUTSIDE `withStrictRepositoryFailures` a failed read is one ERROR line and `[]`; INSIDE it is the filter line (`strictFailures: true`), then `Failed to find chats by character ID`, then the throw.
- v5 site: `crates/quilltap-core/src/db/chats_read.rs:666` returns `Result`; only `services/chat_window.rs:113` wraps it (`fallback::find_by_filter_strict_aware`, no rethrow line). Seventeen other callers `?` it or match the `Err` themselves: `api/characters.rs:385, :518, :1475`; `services/character_enrichment.rs:220, :271`; `tools/self_inventory.rs:1379`; `tools/brahma_lookup.rs:459`; `db/conversation_search.rs:75`; `subprompts/fanout.rs:147`; `generators/rename.rs:650`; `generators/refresh_archive.rs:23`; `photos/avatar_rolls_service.rs:305, :560`; `services/character_archive/service.rs:1169`; `services/cascade_delete.rs:58`; `services/search_replace.rs:163`.
- Done (the home): `db::fallback::chats_by_character_or_empty(character_id, read)` = `find_by_filter_strict_aware("chats", …)` + under the strict scope v4's rethrow ERROR; `chat_window.rs` re-pointed; the carve in `crates/quilltap-harness/tests/chat_window_equivalence.rs:64-66, :104, :275-279` retires (the test already reds "retire it" when v5 logs the line); `fallback_home_guard` HOME_MESSAGES +1.
- ⚖ **Scope question (planner / human):** converting the callers means a failed chats read becomes "no chats" for a character DELETE cascade, a rename, search-and-replace and the archive — destructive on a transient failure (the P4.168 B5 / B6 precedent kept two such reads STRICT, pinned both ways). **Recommend:** land the home + the line; convert the read-only callers (the three `api/characters.rs` counts, both `character_enrichment` counts, `self_inventory`, `brahma_lookup`, `conversation_search`, `subprompts/fanout`); keep `cascade_delete`, `rename`, `search_replace`, `character_archive`, `avatar_rolls_service` STRICT under ONE named both-ways carve; each caller MEASURED for whether v4's call sits inside a strict scope before it moves.
- Differential: `chat_window_equivalence` (the two strict ops); the callers' own families (`characters_reads`, `brahma_lookup_tools`, `search_tools`) need a failing-read row each (a throwing plant that fails BOTH reads — memory note `throwing-plant-must-fail-both-reads`; `brahma_lookup_tools` already has `*_fallback_*` rows).
- Trap: `db/fallback.rs` is append-shared across areas; the strict flag is a thread-local (`fallback.rs:658`) — a line logged inside a `Db::write` closure is invisible to `captured_with` (memory note `capture-rig-writer-thread`).

### B-6 — P4.168's recorded NOTE: the outer `Error loading memory batch` line inside the strict scope   [OPEN, S]
- v5 site: `crates/quilltap-core/src/db/memories_read.rs:632-662` `find_by_character_id_in_batches` logs only the inner filter line (`log_find_by_filter_line`) and propagates.
- v4 site: `lib/database/repositories/memories.repository.ts:188-197` — a FALLBACK `safeQuery('Error loading memory batch', {characterId, skip, batchSize}, [])` around `findByFilter`.
- Done: inside the strict scope log v4's outer line (keys `collection, characterId, skip, batchSize, error`, `strictFailures`) after the inner one, then `Err`. The `MAINTENANCE_BATCH_LOADER_STRICT` ruling (keep strict outside the scope too) STANDS — do not converge the truncation.
- Differential: `maintenance_ops_tier2` (`corpus_read_fails_strict`) — MEASURE whether v4's maintenance run actually enters `withStrictRepositoryFailures` at the pin; if no v4 caller does, this is a unit pin only (and may be NO-ACTION).
- Rides B-5 (same class, disjoint file).

### B-7 — the `lm_window_narrow_below_floor` row   [OPEN, S–M, harness only]
- Source: `status-log.md:202725` (the unifier's note; no row of that name exists anywhere — grep of `crates/`, `harness/` empty). `crates/quilltap-harness/tests/brahma_lookup_tools_equivalence.rs:77-82` records `UNION_RED_H2_WINDOW` "MEASURED EMPTY … no candidate here sits between the relaxed and the default floor", so the ≤14-day floor relaxation is never the deciding term on `lookup_memories`.
- v5 site: `crates/quilltap-core/src/services/recall/score.rs:78` `TOOL_WINDOW_FLOOR_MAX_DAYS = 14.0`, `:141-147` `relax_floor`; the tool `crates/quilltap-core/src/tools/brahma_lookup.rs` (`execute_lookup_memories_tool`). v4: `lib/memory/recall/score.ts` (`relaxFloor`, `:130` `floorRelaxed`), the lookup handler in `lib/tools/handlers/` (the `lookup_memories` executor).
- Done: one corpus row `lm_window_narrow_below_floor` in `harness/oracle/cases/brahma-lookup-tools.test.ts` (beside `:112-114`) — a ≤14-day window holding a planted memory whose canned cosine sits BELOW the provider's default floor and above 0; it must appear on both sides; a twin row with a 15-day window must drop it. Mutation: `relax_floor` forced `false` reds the first.
- Traps: plants need Float32-LE vector BLOBs and a `cannedEmbeddings` entry (memory note `memory-search-plants-need-vectors`); v4's index singleton needs `reloadAll` after a raw plant (`recall-lane-oracle-gotchas`); the production runner only took the vector path after the unifier's `d6202b2d3`.

### B-8 — P4.D374's recorded GAP: the `.qtap` reconcile arm for `brahmaConnectionProfileId` has no differential   [OPEN, S–M, harness only]
- v5 site: `crates/quilltap-core/src/services/quilltap_import/reconcile.rs:618-628`. v4: `lib/import/quilltap-import/reconcile.ts:524-529`.
- Why it has none: `qtap_import_equivalence` never reaches project reconcile; `system_import_*`'s fixture fails every connection-profile import on both sides (the standing `multiCharacterPrefill` finding), so a row there is an equivalent mutant.
- Done: one `system_import_state` case with a project carrying the key and a connection profile that DOES map (MEASURE which conflict strategy / fixture shape gets an `idMaps.connectionProfiles` entry at the pin), plus the unknown-id leg (key left as is). Mutation: skip the remap → red.
- Meeting point: `system_import_state.rs` / `system-import-execute.test.ts` are also B-1's — same lane, or sequence.

### B-9 — P4.D374 Tier 3 item 24: the instance-setting handlers' two lines (three routes)   [OPEN, S–M]
- v4 site: `lib/api/instance-setting-handlers.ts:67` DEBUG `` `[Settings v1] ${Label} settings fetched` `` and `:89` INFO `` `… settings updated (instance-wide)` ``, each with `logSummary(...)`; used by `app/api/v1/settings/brahma-console/route.ts:22`, `data-retention/route.ts:22`, `taboo/route.ts:25`.
- v5 site: none of the six lines exists (`grep 'settings fetched\|instance-wide' crates/quilltap-core/src/api/settings.rs` — only the general-state arms, which still use `eprintln!` at `:3391, :3424, :3428, :3462, :3466`; the Taboo / Brahma arms are dispatched from `api/engine.rs:4975, :4988`).
- Done: the six lines with each route's `logSummary` keys, capture-pinned in the three routes families; the five general-state `eprintln!`s replaced by v4's `logger.error` / `logger.info` lines (`app/api/v1/settings/general-state/route.ts:28, :42, :47, :56, :59`).
- ⚠ Meeting point: `api/settings.rs` belongs to whichever survey owns settings; `3cc12d346` (past the pin) moves the Brahma settings SCHEMA (`maxAgentTurns`), not the handler factory — the lines are stable. If the settings survey has a lane, hand this to it.

### B-10 — P4.D371 Tier 3 item 22: External Prompt's snake_case tracing fields   [OPEN, S]
- v5 site: `crates/quilltap-core/src/generators/external_prompt.rs:306-311, :319, :342, :367, :437, :481, :508` (`character_id`, `connection_profile_id`, `system_prompt_id`, `scenario_id`, `max_tokens`).
- v4 site: `lib/services/external-prompt-generator.service.ts:95-98, :111, :126, :133, :138, :167-170, :201, :207-210, :236, :244` (camelCase keys).
- Done: v4's keys and field sets per line; `external_prompt_tier3_equivalence`'s `logLines` comparand (P4.D371 item 16) grown to compare every line, not a whitelist (memory note `mutation-restore-mtime-and-capture-whitelists`).
- Note: not on a surface `3a79f744c` / `1dc11e947` rewrite (the service's own lines); safe now.

### B-11 — V12: the chokepoint doc comment sits on the thin wrapper   [OPEN, S, comment only]
- v5 site: `crates/quilltap-core/src/db/chat_settings.rs:2328-2353` — the full "v4 `ChatSettingsRepository.updateForUser` — the upsert chokepoint" doc is attached to `update_for_user_returning_id` (a three-line delegate), while the body lives in `update_for_user_with_repairs` (doc `:2364-2368`, five lines). The phase-4 cite "~2477" has drifted; no other misplaced doc comment was found in `:2150-2800`. (The V-numbered review note itself is not in the repo — this is the reading that fits.)
- Done: move the long doc to `update_for_user_with_repairs`; one line on the wrapper. Ride any lane that owns `db/chat_settings.rs` (none of mine does — hand to the settings lane or drop).

### B-12 — P4.D341 Tier 2 item 24: the first-boot built-in seed rewrite on the REAL `migration-vintage/` copy   [OPEN, S–M, test only]
- v5 site: `crates/quilltap-host/tests/host_boot_p4d341_keystone_schema.rs` (arm (b) pins the second-boot half on a synthetic pre-round instance; `TEMPLATE_SEED_AFTER_ENSURES` at `:19, :246`; `crates/quilltap-host/src/host.rs:2452`). Fixture: `crates/quilltap-web/tests/fixtures/migration-vintage/{quilltap.db, quilltap-mount-index.db, quilltap-llm-logs.db}`.
- v4 site: `lib/startup/seed-initial-data.ts` → `repos.roleplayTemplates.seedBuiltInTemplates()` (the `*` → `"*"` narration rewrite, measured by `builtin_templates_equivalence`'s `stale-builtin` state).
- Done: a host-boot arm over a COPY of the trio — the first boot rewrites the built-ins' narration cells once, the second rewrites nothing (or only `updatedAt`, as `stale-builtin` decided).
- Trap: MEASURE first that the vintage main carries built-in template rows at all (not checkable read-only — the file is encrypted); if it does not, the item is NO-ACTION and the dogfood first boot is the only measure (the order header already says so).

### B-13 — P4.D342 deferral (2): v4's inner-circle / keystone cache cases as Rust unit tests   [OPEN, optional, S–M]
- v4 site: `__tests__/unit/lib/memory/frozen-archive-cache.test.ts:288-440` — nine `it`s (`:311, :327, :341, :354, :365, :377, :395, :414, :422`).
- v5 site: `crates/quilltap-core/src/services/frozen_archive.rs` test module (`:1057-1553` ports the first 21 cases; none of the nine).
- Coverage today: the tier-2 differential over the real repositories (`frozen_archive_tier2_equivalence`: the Family / Crew / Club draw, trim, lift, signature misses). Recommend: include only if a lane already owns `frozen_archive.rs`; otherwise leave out.

---

## CLOSED (by measurement)

- **C3 — the template GET sends `narrationDelimiters: null` literally.** `crates/quilltap-core/src/db/roleplay_templates.rs:811-817` ALWAYS inserts the key, decoded (`null` = unmarked prose); the SPA's `apps/web/src/app/punctuation-lint/prose-mode.ts:26-31` tests `=== null`; pinned by `roleplay_templates_routes_equivalence.rs:499+` and LIVE in `apps/web/e2e/salon-roleplay-template-flow.spec.ts:59, :62` (`P4D351_SERVER_LANDED = true`, `P4D341_LANDED = true`).
- **P4.D349's "job-side backfill DEBUG capture pin".** Closed at the `236c04e35` unification's Orrery widen: `harness/oracle/fixtures/headshoulders.json:227` row `not_humanoid`, and `headshoulders_backfill_tier3_equivalence.rs:648` compares every `[HeadShouldersBackfill]` line (v4 `lib/background-jobs/handlers/character-headshoulders-backfill.ts:62`). (Residue: the test header `:23` still says "six" lines.)
- **P4.D349's headshoulders builder widen** — struck in the order header ("DONE at unify").

## NO-ACTION

- **P4.D342 deferral (1)** — no differential plant for the keystone read's failure leg at the archive's two call sites. The fallback lives in the reader (`db/fallback.rs:1927` `keystones_or_empty`), unit-pinned with exact lines at `db/memories_read.rs:2619, :2632, :2663, :2674`; the call sites consume it unwrapped (`frozen_archive.rs:585`). A plant that fails only `findKeystones` (v4 `memories.repository.ts:1194-1223`, a `SELECT *`) and both sides alike does not exist (every memory read selects the column).
- **P4.D349 R-8** — `update_scene_state`'s twin unexercised: no v5 producer of `SceneStateInput`.
- **P4.D341 F-b** (a BLOB `keystone` cell) — unreachable; **F-a** closed its item 25.
- **P4.D363** `DEPENDENCY_PENDING`, `REINDEX_STAMP_IS_FIDELITY`, `RUNNER_CREATES_LEDGER_TABLES`, `EMPTY_AUTHOR_TRUTHINESS_VS_IS_NULL` — ruled, pinned both ways (the last retires if F-2 is filed and fixed).
- **P4.D372** `SEND_REFINE_ISSUE_LIST` (v5 answers v4's FIRST Zod issue; pinned both ways in `send_message_schema_equivalence`), item 28 (the wrong-type `isEstablishedFact` — the standing dispatch `V4::BodyParse` class, census-classified), item 29 (unreachable).
- **P4.D374** — the four `<tool> failed` WARNs, `Chat lookup failed while labelling results`, the two inline-profile WARNs: unreachable in v4 at the pin (every read is a fallback read); item 26 (no remap of `brahmaConsole.inlineConnectionProfileId` on `.qtap` import) is v4-faithful.
- **P4.D375** — item 24 (`buildWindowCensus`'s UTC arm unreachable); the unbuilt recall-assemble "summary fills the cap" row (the cap is pinned with 7 unsummarized chats); `floorRelaxed` unreachable in the replay.
- **P4.D371** item 17 `WARN_UNREACHABLE_IN_ORACLE` (unit-pinned); item 21.
- **P4.D370** Tier 3 16–20 (records).
- **P4.D373** Tier 3 16–22 (records / NO-PORT).
- **P4.D362**, **P4.D363 H1–H3**, **P4.D369 H1–H5**, **P4.D370 H8**, **P4.D372 H1–H7**, **P4.D374 H1 / H3 / H6 / H8**, **P4.D375 H1–H5** — discharged at unification (each header says so).
- **P4.168** B5 / B6 strict rulings, B1-1, C-18 — stand as recorded.

## DEFER-TO-CATCH-UP (v4 past the pin rewrites the surface — `drift-ledger.md` §3 rows cited)

| Item | Why a smalls fix now is thrown away |
|---|---|
| P4.D349 item 15's flip differential row (the recompiled stack's `## Physical Appearance` after a `humanoid` flip) | `3a79f744c` — identity-stack v3 (`IDENTITY_STACK_BUILDER_VERSION` 3; the physical label and "(best used: …)" dropped) |
| P4.D349's tier-1 `image-gen-leaves` / `appearance-sanitize-gate` rows | `3a79f744c` — `appearance-resolution.ts` / `prompt-expansion.ts` / `image-scene-tasks.ts` rewritten (`selectedDescriptionId` / `physicalDescriptionName` removed) |
| P4.D349's avatar outfit rows | `3a79f744c` + the `chat_avatar_timeline` programme (D23 move 25) |
| P4.D349 Tier 2 item 24 (avatar-job / story-background / identity-compiler builder widens) | `3a79f744c` (the same three families' comparands move) |
| P4.D349 R-6 (the pronoun-read WARN's both-sides throwing plant) | `1dc11e947` — `findById` now throws on a present-but-unparseable keystone; the read the WARN wraps changes. Unit-pinned meanwhile (`an_unreadable_character_warns_and_lists_unfiltered`). |
| P4.D349's "standing gap": v4's `parseJsonVaultFile` WARNs unported; the PUT's physical sub-object Zod arms | `1dc11e947` — `vault-overlay/` deleted → `lib/characters/vault/*`; Character API schemas derived + strict |
| V14 — `CHARACTER_BOOLEAN_COLUMNS` lacks `canDrawDiagrams` (`crates/quilltap-core/src/db/store_name_reconcile.rs:660-676`) | `1dc11e947` — `canDrawDiagrams` is one of the 19 managed columns that leave the row schema. At the pin the only observable effect is a BLOB cell in that column (v4 `Boolean(Buffer)` → row kept; v5 → row dropped at `:724-748`) — unreachable. (A one-line add if the planner wants the pin's truth anyway.) |
| `RAW_CHARACTER_ROW_UNVALIDATED` (`crates/quilltap-harness/tests/reach_equivalence.rs:204-224`; `harness/oracle/fixtures/reach.json:935`) | `1dc11e947` — the repository re-keys onto `CharacterRowSchema`; which rows `findByIdRaw` refuses changes |
| P4.D369's unported repository ERROR `Error creating character` on a refused import | `1dc11e947` (the repository's create path) |
| P4.D350's finding (v5's chat read admits a non-`CHARACTER` participant; v4 `lib/schemas/chat.types.ts:742, :817, :868` refuses → 404) and P4.D375's #7 (v5 validates no chat row) | The fix is a whole `ChatMetadataSchema` row twin in `db/chats_read.rs`; `348172b9c` adds three opening-state `chats` columns and `b941280d5` widens the `turnMode` enum. Unreachable from v4-written data. Do it once, after those land. |
| P4.D370 Tier 2 item 14 (an in-character agent-mode row on `enclave_step_tier3` / `primary_stream_tier3`) | Needs a builder rebuild of a shared pair; `3cc12d346` (the Salon agent-mode cap) and the provider prompt-cache commits (`b9b9621e8` …) re-record those corpora. Pinned meanwhile on `orchestrator_tier3` + tier 1. |
| P4.D373's unadded capstone dress row | `348172b9c` / `3db965416` — the Green Room rewritten on the Salon spine |
| P4.D371 Tier 3 item 19 (v4's later Optimizer work) | `4089469a8`, `c874ae9ba` — by definition the catch-up's |

---

## Proposed lanes (disjoint source ownership)

- **L-B1 "roleplay-template Zod twins"** — B-1, B-2, B-3, B-4 (ONE change: they share the delimiter twin). Owns `api/roleplay_templates.rs`, the template functions of `services/quilltap_import/entities.rs` (`:300-340`, `:480-515`), restore step 11 in `services/backup/restore/orchestrator.rs`, an APPENDED block in `api/zod_issues.rs`; families `roleplay_templates_routes`, `system_import_state` (the `Broken Template` row + counts), `system_restore_state` / `system_restore_equivalence`. Size M–L. Optionally carries **B-8** (same import family files).
- **L-B2 "repository-fallback round 6: chats by character + the memory batch line"** — B-5 (after the scope answer), B-6. Owns `db/chats_read.rs`, `services/chat_window.rs`, `db/memories_read.rs` (`find_by_character_id_in_batches` only), an APPENDED home in `db/fallback.rs`, and one-line edits at the converted callers (`api/characters.rs`, `services/character_enrichment.rs`, `tools/self_inventory.rs`, `tools/brahma_lookup.rs`, `db/conversation_search.rs`, `subprompts/fanout.rs`); families `chat_window`, `fallback_home_guard`, the callers' families. Size M.
- **L-B3 "Brahma lookup + import harness rows"** (harness only) — B-7, and B-8 if L-B1 does not take it. Owns `harness/oracle/cases/brahma-lookup-tools.test.ts`, `crates/quilltap-harness/tests/brahma_lookup_tools_equivalence.rs`. Size S–M.
- **L-B4 "log-line smalls"** — B-10 (`generators/external_prompt.rs` + `external_prompt_tier3_equivalence`), B-9 (`api/settings.rs` + the Taboo / data-retention / Brahma arms — ONLY if no settings-area lane exists; otherwise hand B-9 and B-11 to it). Size S–M.
- **Standalone test items:** B-12 (`crates/quilltap-host/tests/host_boot_p4d341_keystone_schema.rs`), B-13 (`services/frozen_archive.rs` tests) — attach to any lane with room, or leave out.

## Cross-area meeting points

- `crates/quilltap-core/src/api/zod_issues.rs` — append-shared (L-B1 adds two twins); any other area adding a `ZodIssue` variant must add its own arms.
- `crates/quilltap-core/src/db/fallback.rs` + `fallback_home_guard`'s HOME_MESSAGES count — append-shared (L-B2 +1 or +2).
- `services/backup/restore/orchestrator.rs`, `services/quilltap_import/entities.rs`, `system_import_state.rs`, `system-import-execute.test.ts`, `system_restore_state` — the carriers survey's files; L-B1 touches the template step / rows only. Census counts in those families (`seen` / fresh-store / import-state 82 / 20) move with any added case — one owner per family per round.
- `api/settings.rs`, `db/chat_settings.rs` — the settings area's (B-9, B-11).
- `api/characters.rs`, `services/character_enrichment.rs`, `tools/self_inventory.rs`, `tools/brahma_lookup.rs` — L-B2's one-line caller edits; another area may own these files for larger work (sequence L-B2 after it, or let that lane apply the edit).
- `harness/oracle/cases/brahma-lookup-tools.test.ts` — only L-B3.

## Recommend leaving OUT of the round

- Every DEFER-TO-CATCH-UP row above (thirteen) — each sits on code `1dc11e947` / `3a79f744c` / `348172b9c` / `3cc12d346` / `4089469a8` rewrites.
- B-13 and B-12 unless a lane has room (B-12 is also the dogfood pass's first-boot row).
- B-11 unless a lane already owns `db/chat_settings.rs`.
- The destructive callers of B-5 (cascade delete, rename, search-and-replace, archive, avatar rolls) — keep strict, carve by name.
- P4.D342 deferral (1), `SEND_REFINE_ISSUE_LIST`, and the unreachable-WARN records — no differential shape exists or the divergence is ruled.
