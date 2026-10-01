# Survey: boot hardness (dogfood #134(b)): `seed_built_ins` step by step against v4's `instrumentation.ts`

Date: 2026-09-30. Surveyor: read-only agent. v4 `~/source/quilltap-server` at `main` = `ca363178d` (clean). v5 `/Users/csebold/source/quilltap-v5` on `main` = `735cf568e`. All line numbers are at those two commits.

**Headline.**

1. **v5 has one writer closure, not one per step.** `seed_built_ins` (`host.rs:1187-1785`) runs **one** `write_blocking` closure holding **29 steps**. 22 of them propagate with `?`. Writes are autocommit with no outer transaction (`db/runtime.rs:209-223`), so a `match` guard can go inline on each step **without splitting the closure** (§B.2).
2. **The #134 failure sits inside a single core call, not a host step.** It is a sub-step of `builtin_mounts::ensure_builtin_mounts` (core `services/builtin_mounts.rs:84-174`), which chains **eight** fallible sub-steps. Five of them have homes in v4's repository **lazy init** or a guarded boot phase. A guard-in-place therefore needs a **core** split of `builtin_mounts.rs`. The host cannot guard a sub-step it cannot see (§B.3).
3. **Count.** **8** v5-fatal steps or sub-steps have a counterpart that v4 guards (or runs lazily). **~24** have a v4 **migration** counterpart, and a migration's `run()` failure is **fatal in v4** (`process.exit(1)`). Those must stay fatal, with one premise split in §D.3.
4. **One existing v5 comment is FALSE.** The P4.D184 avatar-collapse guard (`host.rs:1625-1631`) says v4's runner "records the failure … and the instance boots on". In fact v4's runner **breaks**, and `instrumentation.ts` then calls `process.exit(1)` (§A.2). So v5 already softens one v4-fatal step. It is flagged for a ruling here, not fixed.
5. **v4 behaviour under the #134 plant is already on record.** P4.131's mail plants record v4's `Failed to ensure doc_mount_file_links table in mount index database` ERROR, per read, from `ensureTable` (`mail_carina_tools_equivalence.rs:955-963`). v4 boots and degrades per read.

---

## A. v4 at `ca363178d`

### A.1 `instrumentation.ts` `register()`: the boot sequence (1000 lines)

The outer `try` runs from `:126` to `:984`. Its catch (`:984-999`) logs ERROR `'Fatal error initializing services'` `{context:'instrumentation.register', error, stack}`, then `startupState.setPhase('failed')` and `startupProgress.publish({rawLabel:'subsystem:errored', level:'error', detail})`. It does **not** throw, so the server still starts.

Every guarded catch below logs `{context: 'instrumentation.register', error: <message string>}` unless stated otherwise. "Outer" means the step has no catch of its own and a throw falls to the outer catch.

| # | Phase / lines | Call | Guard | Sentence (level) | On failure |
|---|---|---|---|---|---|
| 1 | -1 `:31-42` | `materializeDataDirFiles` | own try | `console.warn('[startup] cloud-file pre-materialization failed (continuing):', msg)` | continue |
| 2 | -0.5a `:47-87` | `provisionDbKey` (+ legacy `provisionPepper` in try → silent `catch {}`) | **none** for `provisionDbKey` | — | `register()` throws (sits before the outer try) |
| 3 | `:102-109` | locked mode | — | INFO `Server entering locked mode — passphrase required to proceed` | `return` (deferred to unlock) |
| 4 | `:123-124` | `enforceSingleUserMode()` | **none**, before the outer try | — | throws out of `register` |
| 5 | 0 `:126-266` | `ensureDataDirectoriesExist` + legacy copy | legacy copies in own try → ERROR `Failed to copy legacy database` / `Failed to copy legacy files` | | dirs: outer; copies: continue |
| 6 | -0.5b `:268-380` | plaintext→cipher conversion | main: ERROR `Main database encryption conversion FAILED` → **`process.exit(1)`** (`:309-315`); llm-logs / mount: WARN `… encryption conversion failed — continuing` (`:339`, `:368`); header unreadable: WARN `Skipping … encryption check — header read failed; will retry on next restart` | | main: **FATAL**; others continue |
| 7 | `:384-402` | `checkVersionGuard` | — | ERROR `Version guard: database was modified by a newer Quilltap version` `{currentVersion, highestVersion}` | `setVersionGuardBlock` + `return` (health 409 `version-blocked`) |
| 8 | 1 `:407-438` | `MigrationRunner.runMigrations()` | runner-internal (§A.2) | ERROR `Migrations failed - cannot start server` `{failedMigrations, error, migrationsRun, migrationsSkipped}` | **`process.exit(1)`** |
| 9 | `:444-450` | `storeCurrentVersion`, `migrationRunner.cleanup` | none | — | outer |
| 10 | 1.1 `:457-467` | `repairTextEmbeddings` | own | WARN `TEXT embedding repair failed, continuing startup` | continue |
| 11 | 1.25 `:472-485` | `seedInitialData` | own (+ inner, §A.4) | WARN `Error during initial data seeding, continuing startup` | continue |
| 12 | 1.5 `:489-532` | plugin auto-upgrade | own | WARN `Error during plugin auto-upgrade, continuing startup` | continue |
| 13 | 2 `:536-555` | `initializePlugins` | result-checked; a throw is outer | ERROR `Plugin system initialization failed` `{stats, errors}` | continue (throw → outer) |
| 14 | 3 `:560-566` | `fileStorageManager.initialize` | none | — | outer |
| 15 | 3.25 `:570-579` | `reconcileFilesystem` | own | WARN `Error during filesystem reconciliation, continuing startup` | continue |
| 16 | 3.2 `:600-651` | vault backfill `.then` chain (incl. `enqueueHeadShouldersBackfill`, `:635-638`); fire-and-forget | `.catch` + import try | WARN `Error during character vault backfill or physical-file migration` / WARN `Failed to import character vault backfill module` | continue |
| 17 | 3.3 `:659-672` | `scanAllMountPoints` (f&f) | `.catch` + try | WARN `Document mount point scan failed` / WARN `Error initializing document mount point scanner, continuing startup` | continue |
| 18 | 3.3b `:682-690` | `docMountFileLinks.sweepOrphanedStoreChildren()` | own (and `withRawDb` fallback inside, §A.3) | WARN `Error reaping orphaned doc-store children, continuing startup` | continue |
| 19 | 3.4a `:703-711` | `backfillProjectStores` | own | WARN `Error during project store backfill, continuing startup` | continue |
| 20 | 3.4 `:721-731` | `ensureProjectScenariosForAllProjects` | own | WARN `Error ensuring project scenario infrastructure, continuing startup` | continue |
| 21 | 3.4b `:742-750` | `backfillGroupStores` | own | WARN `Error during group store backfill, continuing startup` | continue |
| 22 | `:751-761` | `ensureGroupScenariosForAllGroups` | own | WARN `Error ensuring group scenario infrastructure, continuing startup` | continue |
| 23 | `:766-776` | `ensureGeneralScenariosFolder` | own | WARN `Error ensuring general scenarios folder, continuing startup` | continue |
| 24 | `:782-797` | `ensureGeneralStateFile` | own; INFO `Seeded general state.json in the Quilltap General mount` when it seeds | WARN `Error ensuring general state.json, continuing startup` | continue |
| 25 | 3.5 `:802-854` | schedulers. Nested: `reconcileAutonomousRunsAtStartup` (own: WARN `Failed to reconcile autonomous-room runs at startup, continuing`), `startMountWatchers` (`.catch`: WARN `Mount point watchers failed to start`) | block try | WARN `Error starting background schedulers, continuing startup` | continue |
| 26 | 3.6 `:865-880` | `reconcileConversationRendering` (f&f) | `.catch` + import try | WARN `Conversation render reconciliation failed` / WARN `Failed to import conversation render reconciliation module` | continue |
| 27 | 3.65 `:892-902` | `reconcileChatMessageFts` (awaited) | own; also total inside (`reconcile-chat-message-fts.ts:102` WARN `Chat message FTS reconciliation failed; search may be degraded`) | WARN `Chat message FTS reconciliation failed` | continue |
| 28 | 3.66 `:914-922` | `ensureHelpDocsSynced` (awaited) | own | WARN `Help doc reconciliation failed` | continue |
| 29 | 3.7 `:935-950` | `reconcileEmbeddingDimensions` (f&f) | `.catch` + import try | WARN `Embedding dimension reconciliation failed` / WARN `Failed to import embedding dimension reconciliation module` | continue |
| 30 | 4 `:955-961` | `setPhase('complete')`, `markReady()` | — | INFO `All services initialized successfully` `{migrationsComplete}` | — |

The #134 finding cites `:742-797` (and the walk cites `:767-797`). Both ranges are correct at `ca363178d` and cover rows 21–24.

### A.2 What is FATAL in v4

- **`provisionDbKey`** and **`enforceSingleUserMode`** sit outside the outer try (rows 2 and 4).
- **The main-DB cipher conversion**: an explicit `process.exit(1)` (`:315`).
- **The migration runner** (`:417-431` → `process.exit(1)`). Inside the runner (`migrations/index.ts`):
  - A `shouldRun()` throw is **NOT fatal**. It logs ERROR `Error checking if migration should run` `{context:'migrations.runMigrations', migrationId, error}` and the runner skips and **continues** (`:131-148`).
  - `run()` returning `success:false` logs ERROR `Migration failed` `{context, migrationId, error, message}` and **breaks** (`:162-181`).
  - `run()` throwing logs ERROR `Migration threw an exception` and breaks (`:183-205`).
  - Either break sets `allSucceeded=false` (`:210`), which leads to `process.exit(1)`.
  - A migration already in `migrations_state` is **skipped before `shouldRun`** (`:126-129`), so it can never fail.
- **The instance lock**: the DB connect throws on conflict (`lib/database/backends/sqlite/backend.ts:520-547`: ERROR `Cannot open database: another instance holds the lock` + `setInstanceLockConflict`). It is reached through the migration runner, whose `waitForDatabaseReady` failure ends in `process.exit(1)` too.
- **Version guard**: a block, not a crash (`return`, health 409).
- **Rows 9 / 13 / 14** (`storeCurrentVersion`, a plugin-init throw, file storage) fall to the **outer** catch. Phase becomes `failed`, but **the server still serves**: `/api/health` treats `failed` as past startup and runs its service checks (`app/api/health/route.ts:164-174` lists only `pending|migrations|seeding|plugin-updates|plugins|file-storage` as 503). `ensureServerReady` waits 30 s, then WARNs `Server startup not complete after 30s, proceeding anyway` and proceeds (`lib/api/middleware/context.ts:26-34`).

**`collapse-duplicate-avatar-rolls-v1`** (`migrations/scripts/collapse-duplicate-avatar-rolls-v1.ts:642-657`) catches and logs ERROR `Failed to collapse duplicate avatar rolls`, then returns `success:false`. The runner treats that as `Migration failed`: it breaks and exits (1). v4 does **not** "boot on" (see §D.3).

### A.3 Repository lazy-init repairs: where v4 runs them, and what a throw does

| Helper (`lib/database/repositories/mount-index-case-repair.ts`) | Called from (lazy `onTableEnsured`) |
|---|---|
| `ensureFolderNocaseUniqueIndex` | `doc-mount-folders.repository.ts:40` |
| `ensureLinkNocaseUniqueIndex` (the #134 SELECT) | `doc-mount-file-links.repository.ts:408` |
| `ensureLinkGroupColumn` | `doc-mount-file-links.repository.ts:400`, `doc-mount-documents.repository.ts:59` (also the migration `add-doc-mount-link-groups-v1`) |
| `repairMountPointNameCollisions` | `doc-mount-points.repository.ts:69` |

None of these run in `instrumentation.ts`. `AbstractDedicatedDbRepository.ensureTable` (`dedicated-db.repository.ts:134-152`) works like this:

1. It runs `generateDDL` + `onTableEnsured` inside a try.
2. On a throw it logs ERROR `` `Failed to ensure ${collectionName} table in ${DB_LABELS[dbTarget]} database` `` `{error: extractErrorMessage(error)}` and **rethrows**. `DB_LABELS` is `mountIndex → 'mount index'` (`:54-57`).
3. `tableEnsured` stays false, so this **retries and re-logs on every access**.
4. The rethrow lands in the caller's `safeQuery` (`withRawDb`, `:200-226`), which logs `errorMessage {collection, …context, error}` at ERROR (`safe-query.ts:64-66`) and answers the fallback (or rethrows in `'rethrow'` mode).

So in v4 a damaged mount index **never touches the boot**: it degrades per read. Under the #134 plant v4's boot-time reads are:

- Row 18: the sweep → `Failed to ensure doc_mount_file_links table in mount index database` + `Error sweeping orphaned store children` (`doc-mount-file-links.repository.ts:1419-1434`) → the `{0,0,0}` fallback → the boot continues.
- Row 23 / 24 reads, the same shape.

A `DROP TABLE` plant is **not** a failure in v4: `generateDDL`'s `CREATE TABLE IF NOT EXISTS` self-heals it (P4.131's finding). Only a column rename damages v4.

`help_docs` is lazy in v4 too (`HelpDocsRepository extends AbstractBaseRepository`, `help-docs.repository.ts:24-26`). Its `getCollection` → `safeQuery('Failed to ensure collection exists')` with no fallback, so it rethrows to the caller (`base.repository.ts:113-124`).

### A.4 The named subsystems: v4's home, guard sentence, level

| Subsystem | v4 home | Guard sentence (level) |
|---|---|---|
| Built-in roleplay templates | boot, inside `seedInitialData` (`seed-initial-data.ts:56-63`) | ERROR `Failed to seed built-in roleplay templates` `{context:'seed-initial-data', error}`; seeding continues. Outer: ERROR `Error during initial data seeding` (`:130-135`), then row 11's WARN |
| Built-in prompts seeder | lazy (row-level refresh on read; P4.D237) | not a boot step |
| Help docs sync / reconcile | boot 3.66 (row 28) + lazy `ensureHelpDocsSynced` | WARN `Help doc reconciliation failed` |
| Avatar-roll collapse | **migration** | ERROR `Failed to collapse duplicate avatar rolls` then `Migration failed`: **FATAL** (exit 1) |
| Bug-132 heal (`clear-generated-image-placeholder-descriptions-v1`) | migration | runner semantics (§A.2) |
| Refusal ledger columns (`add-chat-refusal-ledger-v1`, in `add-chat-concierge-mode.ts`) | migration | runner semantics |
| FTS5 reconciler | boot 3.65 (row 27); objects come from migration `create-chat-message-fts-v1` (in `compress-chat-message-text.ts`) | WARN, two sentences (row 27) |
| Embedding-dimension reconcile | boot 3.7 (row 29) | WARN `Embedding dimension reconciliation failed` |
| `state.json` ensure | boot (row 24) | WARN `Error ensuring general state.json, continuing startup` |
| Cold-chunk re-embed | part of 3.6 since `f7f3d7bf0` (row 26) | WARN `Conversation render reconciliation failed` |
| Conversation render reconcile | boot 3.6 (row 26) | as above |
| Head-and-shoulders enqueue | 3.2 chain (row 16) | WARN `Error during character vault backfill or physical-file migration` |
| Orphaned store-children reap | boot 3.3b (row 18) + daily maintenance | WARN (outer) / ERROR `Error sweeping orphaned store children` (inner fallback) |

---

## B. v5 on `main` (`735cf568e`)

### B.1 The boot sequence, side by side

**Fatal before `assemble`:**

- `Host::start` (`host.rs:228`): `NoRuntime` at `:229`, `CoreEngine::boot` mapped to `HostError::Boot` at `:280`.
- `CoreEngine::boot` (`quilltap-core/src/api/engine.rs:769`): `PepperMismatch` `:775`.
- `open_ready` (`engine.rs:7100-7134`): `pre_open` (the lock, `host.rs:489-508`) → `BootError::Assemble`; `MissingMainDb`; `Db::open` → `BootError::Db`; `assemble` → `BootError::Assemble`.
- These map to v4 rows 2 / 6 / 8 and the lock. All are fatal in v4 too. **Keep them as they are.**

**`assemble` (`host.rs:510`):**

| v5 # | Line | v5 call | v5 now | v5 sentence | v4 counterpart (§A) | v4 verdict |
|---|---|---|---|---|---|---|
| 1 | 1196 | `builtin_templates::seed_built_in_templates` | **`?`** | — | A.4 templates: ERROR `Failed to seed built-in roleplay templates` `{context:'seed-initial-data', error}` | **GUARD** (ERROR) |
| 2 | 1205 | `fictional_clock_anchor_repair::anchor_fictional_clock_bases` | **`?`** | — | migration `anchor-fictional-clock-base-v1` | fatal (run arm) |
| 3 | 1217 | `character_archive_repair::ensure_character_archive_columns` | **`?`** | — | migration `add-character-archive-fields-v1` | fatal |
| 4 | 1227 | `chat_settings_composer_repair::ensure_chat_settings_composer_columns` | **`?`** | — | three migrations (`add-composer-emoji-field-v1` …) | fatal |
| 5 | 1242 | `…impersonation_voice_repair::ensure_…_column` | **`?`** | — | `add-impersonation-voice-rewrite-field-v1` | fatal |
| 6 | 1255 | `connection_profiles_prefill_repair::ensure_…` | **`?`** | — | `add-profile-multi-character-prefill-field-v1` | fatal |
| 7 | 1267 | `connection_profiles_fallback_repair::ensure_…` | **`?`** | — | `add-profile-fallback-fields-v1` | fatal |
| 8 | 1277 | `chat_messages_route_trail_repair::ensure_…` | **`?`** | — | `add-route-trail-message-column-v1` | fatal |
| 9 | 1286 | `chats_cycle_order_repair::ensure_…` | **`?`** | — | `add-cycle-order-column-v1` | fatal |
| 10 | 1301 | `files_generation_key_repair::ensure_…_and_index` | **`?`** | — | `add-file-generation-key-column-v1` | fatal |
| 11 | 1317 | `chats_transcript_version_repair::ensure_…` | **`?`** | — | `add-transcript-version-column-v1` | fatal |
| 12 | 1331 | `chats_moderation_refusal_ledger_repair::ensure_…` | **`?`** | — | `add-chat-refusal-ledger-v1` | fatal |
| 13 | 1347 | `thinking_prefill_retire_heal::retire_prefill_on_thinking_profiles` | **`?`** | INFO on `Ran` | `retire-prefill-on-thinking-profiles-v1` | fatal on run; **shouldRun arm guarded** (§D.3) |
| 14 | 1375 | `chat_activity_recompute_heal::recompute_chat_last_message_at` | **`?`** | INFO | `recompute-chat-last-message-at-v1` | same split |
| 15 | 1412 | `folders_unique_path_repair::ensure_folders_unique_path_index` | **`?`** | INFO | `collapse-duplicate-folders-v1` | same split |
| 16 | 1438 | `help_doc_chunks_repair::ensure_help_doc_chunks_table` | **`?`** | — | `create-help-doc-chunks-table-v1` | fatal |
| 17 | 1445 | `help_doc_chunks_repair::ensure_help_docs_table` | **`?`** | — | **lazy** `HelpDocsRepository` ensureCollection (A.3) | **GUARD** (v4 never boots it) |
| 18 | 1465 | `conversation_render_reconcile::reconcile_conversation_rendering` | total | inner WARNs (`Failed to scan for incomplete conversations; skipping reconciliation` …) | row 26 | already guarded |
| 19 | 1503 | `chat_informs::ensure_chat_informs_table` | **`?`** | — | `add-chat-informs-table-v1` | fatal |
| 20 | 1534 | `chat_message_fts_reconcile::reconcile_chat_message_fts` | `let _` (total) | inner WARN `Chat message FTS reconciliation failed; search may be degraded` | row 27 | already guarded |
| 21 | 1561 | `scenario_seeded_summary_heal::clear_…` | **`?`** | INFO | `clear-scenario-seeded-chat-summaries-v1` | fatal / shouldRun split |
| 22 | 1603 | `files_sha256_realign_heal::realign_file_entry_sha256` | **`?`** | INFO | `realign-file-entry-sha256-v1` | fatal / split |
| 23 | 1640 | `avatar_rolls_collapse_heal::collapse_duplicate_avatar_rolls` | **guarded** | ERROR `Failed to collapse duplicate avatar rolls` `{context:"migrations.collapse-duplicate-avatar-rolls", error = %e}` | migration: **FATAL** in v4 | ⚠ v5 is SOFTER than v4 (§D.3) |
| 24 | 1708 | `generated_image_placeholder_heal::clear_…` | **`?`** | INFO | `clear-generated-image-placeholder-descriptions-v1` | fatal / split |
| 25 | 1722 | `builtin_mounts::ensure_builtin_mounts` (core `:84`) | **`?`**, 8 sub-steps → | | | |
| 25a | core 103 | `ensure_mount_index_tables` DDL | `?` | — | provisioning migrations' `ensureMountIndexTables` (+ lazy `generateDDL`) | fatal |
| 25b | core 147 | `ensure_folder_nocase_unique_index` | `?` | — | **lazy** folders repo → ERROR `Failed to ensure doc_mount_folders table in mount index database` | **GUARD** |
| 25c | core 148 | `ensure_link_nocase_unique_index` → `repair_link_case_collisions` (**the #134 failure**) | `?` | — | **lazy** links repo → ERROR `Failed to ensure doc_mount_file_links table in mount index database` | **GUARD** |
| 25d | core 149 | `repair_mount_point_name_collisions` | `?` | — | **lazy** mount-points repo → ERROR `Failed to ensure doc_mount_points table in mount index database` | **GUARD** |
| 25e | core 157 | `ensure_link_group_column` | `?` | — | lazy (links + documents repos) **and** migration `add-doc-mount-link-groups-v1` | GUARD (lazy is v4's live home on an instance whose ledger has the migration) |
| 25f | core 162 | `doc_mount_file_links::sweep_orphaned_link_content` | `?` | — | migration `add-doc-mount-link-groups-v1` step 2 | fatal (ledger-gated in v4) |
| 25g | core 171 | `doc_mount_file_links::sweep_orphaned_store_children` | `?` | — | **boot 3.3b** (row 18) | **GUARD** (WARN `Error reaping orphaned doc-store children, continuing startup`) |
| 25h | core 106 | `ensure_one_mount` ×3 | `?` | — | provisioning migrations (`provision-general-mount`, `-user-uploads-`, `-lantern-backgrounds-`) | fatal (ledger-gated in v4) |
| 26 | 1723 | `builtin_mounts::ensure_general_scenarios_folder` | **`?`** | — | row 23 | **GUARD** (WARN `Error ensuring general scenarios folder, continuing startup`) |
| 27 | 1728 | `general_state::ensure_general_state_file` | guarded | WARN `Error ensuring general state.json, continuing startup` `{error = %e}` | row 24 | existing pattern; ⚠ missing v4's `context` field and renders `sqlite error: …` (§D.4) |
| 28 | 1772 | `headshoulders_backfill_enqueue::enqueue_headshoulders_backfill` | `let _` | inner WARNs | row 16 | already guarded (sentence differs, P4.82's recorded divergence) |
| — | 1784-1785 | closure exit | `.join()` → `built-in seed thread panicked` / `built-in seed failed: {e}` | | | |
| 29 | 536 | `seed_sample_content` (`:1793`) | `?`, but only on thread panic / `WriterGone` (inner report swallowed) | — | row 11 (`seedFromImports` / `seedAvatars`) | effectively guarded |
| 30 | 552 | `reconcile_help_docs_at_boot` (`:1091`) | guarded | WARN `Help doc reconciliation failed` `{context:"instrumentation.register", error}` (target `quilltap::boot`) | row 28 | existing pattern, v4-exact |
| 31 | 557 | `reconcile_embedding_dimensions_at_boot` (`:1124`) | `?`, but the closure body is infallible, so only panic / `WriterGone` | INFO lines | row 29: WARN `Embedding dimension reconciliation failed` | **GUARD** the residual (panic / `WriterGone`) with v4's sentence |

These steps are never fallible at boot: `TerminalManager`, `spine.build`, `HandlerRegistry`, the pump tasks (`:561-700+`, no `?` measured over `:700-1075`).

**Tallies.**

- **v5-fatal steps that v4 guards or runs lazily: 8.** #1, #17, 25b, 25c, 25d, 25e, 25g, #26. Add #31's residual if it counts (9).
- **v5-fatal steps that are v4 migrations (keep fatal): 19 host-level** (#2–#16, #19, #21, #22, #24) plus core 25a, 25f and 25h.
- **Already guarded: 7** (#18, #20, #23, #27, #28, #29, #30).

### B.2 The `write_blocking` fresh-thread idiom

- The sites are `:1082` (doc), `:1120-1130` (dims), `:1194` (seed) and `:1800` (sample). Each spawns an OS thread, calls `db.write_blocking(|ws| …)` and joins.
- `write_blocking` (`db/runtime.rs:209-223`) ships `FnOnce(&mut WriterSet) -> Result<T, DbError>` to the writer task and returns its result.
- **One `?` inside the seed closure ends the whole closure.** Every later step is skipped, and the `Err` becomes `built-in seed failed: {e}` → `BootError::Assemble`.
- **29 steps share ONE closure.** **No outer transaction**: autocommit; only #14, #15 and #22 open their own scoped `unchecked_transaction`/`transaction()`, which roll back on drop. A failed statement leaves the connection usable.
- **Conclusion: no closure split is needed.** An inline `match … { Err(e) => warn!/error!(…) }` per guarded step is sufficient and keeps the writer-thread hop count unchanged. A split would add **N** writer round trips and change ordering visibility for no fidelity gain.

### B.3 The failing call, and the other boot-time repairs that v4 runs lazily

- **`quilltap_core::db::mount_index_case_repair::repair_link_case_collisions`** (`quilltap-core/src/db/mount_index_case_repair.rs:365`, private) runs `SELECT id, mountPointId, relativePath, fileName, createdAt FROM doc_mount_file_links` (`:367-369`).
  - It is reached from `pub fn ensure_link_nocase_unique_index` (`:451-452`), called from `builtin_mounts.rs:148`.
  - Error bytes: `DbError::Sqlite`'s `Display` gives `sqlite error: no such column: relativePath`. The finding's full served message was `engine assembly failed: built-in seed failed: sqlite error: no such column: relativePath`.
- **Other v5 boot steps whose v4 home is lazy:** 25b, 25d, 25e (the three sibling helpers) and #17 (`help_docs`). v5's comments call the collapse "a documented non-divergence" (`builtin_mounts.rs:136-146`, `mount_index_case_repair.rs:18-28`) — on cadence, yes; on failure semantics, no.
  - Also stale: `builtin_mounts.rs:163-170` (P4.31) says "v4 has no such pass" for `sweep_orphaned_store_children`. v4 now runs it at boot 3.3b (row 18, bug 9).
- **Recommendation: GUARD IN PLACE, do not move.** CLAUDE.md's no-new-boot-steps rule cuts both ways: moving these to a v5 lazy init would be a new mechanism. Guard each sub-step at its boot site with v4's sentence for that failure:
  - lazy-init helpers: v4's `ensureTable` ERROR;
  - 25g: v4's 3.3b WARN.

  That needs `ensure_mount_index_tables` split so each helper is individually guardable, with 25a / 25f / 25h still propagating.

### B.4 How a guarded-but-failed step surfaces

- `StartupStatus` (`quilltap-web/src/state.rs:15-28`) has only `Running | LockConflict | Failed`.
- `boot_startup_status` (`quilltap-web/src/lib.rs:697-704`) → `classify_boot_failure` (`:716-737`, web 0.0.205) logs ERROR `Startup failed: the engine did not assemble` / `Startup refused: the instance lock is held elsewhere` on `quilltap::boot`.
- v5 has **no per-step degraded flag**, and **neither does v4.** `startupState` has a single `phase` + `isReady` (`lib/startup/startup-state.ts:31,180,212`). A guarded step surfaces in v4 **only as its WARN/ERROR log line**, and `/api/health` stays `healthy`.
- So a guarded step in v5 needs **no web change**: the engine assembles, the status is `Running`, and the log line is the whole surface. That matches v4.
- **Out of scope, recorded:** v4's outer-catch phase `failed` still **serves** (§A.2). v5's `Failed` answers 503 to every dispatch. Reproducing that would mean serving without an engine, which v5's model cannot do.

### B.5 Existing tests that can pin a planted-column boot

- **`quilltap-host/tests/`** (15 boot binaries): `host_boot.rs`, `host_boot_avatar_rolls_collapse.rs`, `host_boot_chat_message_fts.rs`, `host_boot_p4d182_columns.rs`, `host_boot_p4d225_columns.rs`, `host_builtin_seeds.rs`, `host_help_boot_order.rs`, `host_help_docs_boot.rs`, `host_headshoulders_backfill.rs`, `host_generated_image_placeholder_heal.rs`, `host_scenario_seeded_summary_heal.rs`, `host_sample_content_seed.rs`, `host_lock_ordering.rs`, `host_cadence.rs`, `host_llm_log_cleanup.rs`.
  - They all hand-roll their instance with `Writer::open_writable` + reduced DDL and the test pepper `dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=`. None copies a committed fixture.
  - **Capture:** `host_help_boot_order.rs:7-44` installs `quilltap_core::test_support::CaptureLayer` as the **process-global** default. Boot logs fire on the writer thread and the spawned seed thread, which a thread-scoped capture cannot see. Rule: one global subscriber per test binary.
  - `host.rs` has **no** `#[cfg(test)]` module.
- **`quilltap-web/tests/lock_conflict_boot_status.rs`**: `foreign_hostname_fresh_heartbeat_refuses_the_boot_with_v4s_sentence` (`:29`) and `a_failure_after_acquisition_is_not_a_conflict_with_ourselves` (`:97`) pin `classify_boot_failure`.
- **`quilltap-web/tests/common/mod.rs`**: `materialize_*_instance` (`:96-479`) copies committed pairs into a tempdir `data/` under `TEST_PEPPER` (`:17`). `store_unavailable_envelope.rs` is the closest **plant-on-a-materialized-copy-then-boot** precedent (P4.23).
- **e2e**: `apps/web/e2e/*` boot `webBinary()`, e.g. `character-avatar-rolls-flow.spec.ts:122`. No planted-boot beat exists.

---

## C. Proof shape

There is no v4 oracle for "boot survives". v4's evidence is its source (§A) plus its recorded per-read behaviour under the same plant.

**(i) Rust, per guarded step: RECOMMENDED.** One host test binary, `quilltap-host/tests/host_boot_hardness.rs`, with a process-global `CaptureLayer`. One arm per guarded step:

1. Copy a committed pair to a tempdir as `data/quilltap.db` + `data/quilltap-mount-index.db`, keyed with `TEST_PEPPER`, with `HostConfig.env_pepper`.
2. Plant the column rename on the COPY with `Writer::open_writable`. Arms:
   - `doc_mount_file_links.relativePath → relativePath_x` (25c, the #134 plant; also exercises 26 / 27 downstream);
   - `doc_mount_folders.name → name_x` (25b; note that `path_x` would also break 25h's `ensure_folder_path` → keep 25h fatal and choose `name`);
   - `doc_mount_points.name → name_x` (25d);
   - for 25e, a column the link-group ensure reads;
   - for 25g, `doc_mount_folders.mountPointId → …` (the reap's join);
   - for #1, `roleplay_templates` (a column the seed binds);
   - for #17, `help_docs` (a column its ensure / `CREATE INDEX` touches);
   - for #26, the general pointer's folder table.
3. `Host::start(...)` must be `Ok`, and `dispatch(Request::Health)` must report `ready` (the `host_boot_avatar_rolls_collapse.rs:257-264` `boot()` helper).
4. Assert v4's sentence, level, target and `error` field **bytes**: the bare SQLite message, not `sqlite error: …` (§D.4).
5. Assert the step's effect is absent (e.g. no NOCASE index recreated), and that the **next** step still ran (e.g. #27's `state.json` line, or a later-step artefact). That proves "continue", not just "no crash".

**Red-first:** each arm panics at `Host::start(...).unwrap()` on main as it stands. **Keep one arm per v4-fatal class** (e.g. drop `chat_settings` → #4 still fails the boot) so nothing over-softens.

**Substrate: `crates/quilltap-web/tests/fixtures/post-office-main.db` + `post-office-mount.db`** (164 KB + 188 KB).

- It is small, has a populated mount index (six character vaults + a seeded letter, per `post-office-main.db.meta.json`), and is the pair P4.131's mail plants were built on. So (iii)'s live row and (i) share a substrate.
- Path from the host crate: `CARGO_MANIFEST_DIR/../quilltap-web/tests/fixtures/`.
- If the lane prefers to stay in `quilltap-web/tests`, add a `materialize_post_office_instance` beside `materialize_salon_instance` in `common/mod.rs`. That boots through `boot_startup_status`, which additionally pins `StartupStatus::Running`.
- ⚠ Check the pair's vintage first (the fixture-vintage heal rounds). A missing column there is fatal for the **migration** steps and would mask the arm. Measure with one unplanted boot arm.

**(ii) v4-side jest capture of `register()`: NOT FEASIBLE / not needed.**

- No harness case or v4 test drives `instrumentation.ts` (grep: `harness/oracle/cases/*` only mocks `startupState`; zero v4 tests import `@/instrumentation`).
- `register()` imports Next runtime state and calls `process.exit`.
- The sentences are literal strings, so transcribe them with file:line and a source-literal pin.
- The **closest recipe** for v4's behaviour under the plant is `harness/oracle/cases/mail-tools.test.ts`'s rename plants. Its recorded NDJSON already carries v4's `Failed to ensure doc_mount_file_links table in mount index database` and `Failed to ensure doc_mount_folders table in mount index database` lines (catalogued at `mail_carina_tools_equivalence.rs:955-963` as excluded "plant artefacts", **because v5 had no ensure step**).
- The lane can promote those two exclusions to shared messages once v5 logs them at boot, but **only** as a count-tolerant compare: v4 logs per access, v5 once per boot (§D.5).

**(iii) Live row (dogfood).** P4.131's F1 plant on the Friday copy (`doc_mount_file_links.relativePath → relativePath_x`, server stopped, `quilltap db --write`):

- v5 **boots**;
- the boot log carries the guarded ERROR;
- `list_mail` as Friday answers with v4's fallback lines (bare message, `quilltap::db`);
- rename back afterwards.

This discharges the walk's BLOCKED F1.

---

## D. Traps and premises

1. **Closure granularity.** One closure, autocommit, so guard inline (§B.2). But the eight `builtin_mounts` sub-steps live in **one core fn**. The host cannot guard 25b–25e and 25g without a core split: a pub per-step entry, or `ensure_builtin_mounts` taking a per-step error sink. Wrapping the whole `ensure_builtin_mounts` call in one host guard would also swallow 25a / 25f / 25h (v4-fatal) and skip the three mount provisions. **Do not do that.**
2. **Do NOT soften v4-fatal steps.** The 19 re-homed migration steps, 25a, 25f and 25h stay `?`, as do the lock, `MissingMainDb`, `PepperMismatch` and `Db::open`.
3. **The migration split, and a false premise.**
   - v4 skips a migration already in `migrations_state` **before** `shouldRun`. A `shouldRun` throw is a guarded ERROR `Error checking if migration should run` + skip.
   - v5's column ensures have **no** ledger gate: they test `pragma_table_info` each boot. v5's data heals check the ledger first (#13 / #14 / #21 / #22 / #24, per their module headers).
   - So on a ledger-complete instance like Friday, v4 can never fail on any of these, while v5's column ensures can, e.g. on a missing table. That is a real but separate divergence: **record it, do not fold it into this order**. Each heal's detection query maps to v4's guarded `shouldRun` arm; mapping it would need the core heals to return a detection-vs-run error kind.
   - **P4.D184's comment at `host.rs:1625-1631` is FALSE.** v4 exits (1) on that migration's `success:false` (§A.2). v5 is softer than v4 here. **Escalate for a ruling** (keep, as "safer", recorded; or make it `?`). Do not silently change it.
4. **Error-text bytes.** v4 logs `error instanceof Error ? error.message : String(error)` / `extractErrorMessage`, the driver's bare sentence. v5's `%e` on `DbError` renders `sqlite error: …`. The existing #27 guard already has this defect. It also lacks v4's `context: 'instrumentation.register'` field, which #30 carries. `db::fallback::error_text` (`quilltap-core/src/db/fallback.rs:32`) is **private**: expose it (core) or match `DbError::Sqlite` in the host. That is the P4.131-unification "v5 `Display` prefix on a v4 line" trap.
5. **Wording and level per step.**
   - Instrumentation-guarded steps (25g, #26, #31's residual) use **WARN** with `… continuing startup` or the phase's own words (row 18 / 23 / 29 exact).
   - Templates (#1) use **ERROR** `Failed to seed built-in roleplay templates` with `context:'seed-initial-data'`. **Not** `continuing startup`.
   - The lazy-init helpers (25b–25e) use **ERROR** `` Failed to ensure <table> table in mount index database `` `{error}`. That is v4's `ensureTable` line, logged in v4 **per access, not once**. Record the cadence divergence, or the oracle promotion in §C(ii) will miscount.
   - #17 `help_docs` uses **ERROR** `Failed to ensure collection exists` `{collection:'help_docs', error}` (`base.repository.ts:115-121`).
   - The target is `quilltap::boot` for instrumentation lines. For the repository lines the lane must pick between `quilltap::db` (the `db::fallback` home's target) and `quilltap::boot`; recommend `quilltap::db` + a home fn so `fallback_home_guard` stays the sole owner.
6. **v5-only steps.** 25e's migration half and 25f exist in v4 only as a ledger-gated migration. **None of the guarded set is v5-only**, so no v5-invented sentence is needed. If the lane guards anything without a v4 counterpart, give it a v5 sentence and record it as such.
7. **Ordering constraints.**
   - #26 and #27 read `doc_mount_folders` / `doc_mount_file_links`, so they will re-fail under the 25b / 25c plants. That is fine: they are guarded, and v4 hits the same per read.
   - #23 and #24 sit **before** `ensure_builtin_mounts` and read links. Under the #134 plant on a **non**-ledger-complete fixture, #23 (guarded) and #24 (`?`) could fail first. On Friday the ledger rows exist, so they skip. Make the test arms either seed the ledger rows or assert which step failed.
   - Help reconcile (#30) needs `help_docs` (#17) and `help_doc_chunks` (#16). A guarded #17 failure leaves #30 to log its own guarded WARN. Embedding dims (#31) run after #30.
8. **E2E boot timing.** Guarding adds no work, but a guarded failing step now lets the boot **finish**. An e2e beat that relied on a 503 has none to rely on (measured: no beat plants a boot failure).
9. **Ownership overlap with the #133 lane** (`spine.rs` + `images_generate.rs`): **independent files.** `host.rs` never constructs `DbProviderKeys`. Grep shows it only in `spine.rs:206/208/388/3657` and `images_generate.rs:37/60`. `host.rs` uses only `SpineFactory::build` (`:574`). There is no shared hunk to pin. Both lanes bump `quilltap-host`'s version, so a Cargo.toml/Cargo.lock conflict is expected at unify and is a trivial unifier merge.

---

## E. Ownership

- **HOST:** `crates/quilltap-host/src/host.rs`:
  - `seed_built_ins` guards #1, #17, #26 and the 25b–e/g sub-steps via the core split;
  - fix #27's field/bytes;
  - `reconcile_embedding_dimensions_at_boot` residual guard (#31);
  - correct the P4.D184 comment only per the ruling.

  New `crates/quilltap-host/tests/host_boot_hardness.rs` (or the web variant below). No `boot*.rs` split is needed.
- **CORE (unavoidable, minimal):**
  - `crates/quilltap-core/src/services/builtin_mounts.rs`: split `ensure_mount_index_tables` so 25b–25e and 25g are separately callable or guardable; keep 25a / 25f / 25h propagating; fix the stale P4.31 "v4 has no such pass" comment and the "non-divergence" note at `:136-146`.
  - `crates/quilltap-core/src/db/mount_index_case_repair.rs`: module doc `:18-28`.
  - Optionally `crates/quilltap-core/src/db/fallback.rs`: a pub `error_text` or a home fn for v4's `ensureTable` line, plus its `fallback_home_guard` allowance (harness).
- **WEB:** none required (§B.4). Optional: a `materialize_post_office_instance` in `crates/quilltap-web/tests/common/mod.rs` + `crates/quilltap-web/tests/boot_hardness.rs` if the proof lives there.
- **HARNESS:**
  - optional promotion of the two `Failed to ensure …` exclusions in `crates/quilltap-harness/tests/mail_carina_tools_equivalence.rs:955-963` (count-tolerant);
  - the `fallback_home_guard` if a home fn is added.
- **Versions:** `quilltap-host` 0.0.168 → bump; `quilltap-core` 0.0.1128 → bump; `quilltap-web` 0.0.205 (only if it is touched); `quilltap-harness` 0.0.1051 (only if it is touched).
- **Docs:** CHANGELOG (terse); a `status-log.md` lane record; `dogfood-findings.md` #134(b) → FIXED (unifier's call); P4.131's F1 row re-queued for the live pass.
