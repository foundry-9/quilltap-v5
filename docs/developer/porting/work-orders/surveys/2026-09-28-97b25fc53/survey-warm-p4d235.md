# Survey — `f7f3d7bf0` WARM-EMBEDDINGS half (cold tier removed) + retention/settings/route/client bits

Dated 2026-09-28. v4 `main` @ `97b25fc53` (clean); baseline `acadcc7cd`; target `f7f3d7bf0` (4.10.0-dev.96).
READ-ONLY survey. Hunks read with `git show f7f3d7bf0 -- <path>`; line numbers are POST-commit at `f7f3d7bf0`.

---

## A. v4, hunk by hunk

### A1. `lib/database/repositories/conversation-chunks.repository.ts` (−40)
- **Removed:** `ConversationChunksRepository.clearEmbeddingsForChat(chatId: string, olderThan?: string): Promise<number>`
  (was `f7f3d7bf0^` :241–279, method at :261). Body: `safeQuery` over
  `UPDATE conversation_chunks SET embedding = NULL, updatedAt = ? WHERE chatId = ? AND embedding IS NOT NULL[ AND updatedAt < ?]`,
  params `[now, chatId, olderThan?]`, error message `'Error clearing chunk embeddings for chat'` ctx `{chatId, olderThan}`, fallback `0`.
- **Callers before:** exactly ONE — `collapse-stale-chat-caches.ts:120` (`git grep f7f3d7bf0^ -- lib app`). **After:** none (method gone). Post-commit the class goes `deleteByChatId` (:232–239) → `updateEmbedding` (:241).

### A2. `lib/background-jobs/maintenance/collapse-stale-chat-caches.ts` (65 lines)
- Header :1 `Stale-chat cache collapse + conversation-chunk cold-tiering` → `Stale-chat cache collapse`.
- Header bullet list: `chats.renderedMarkdown` line removed (column-drop half's concern, but it is in THIS file).
- New doc :24–30 — chunk embeddings "deliberately NEVER touched here" (~16 MB measured). NEVER-touched list gains `` `conversation_chunks.embedding` ``.
- `StaleChatCacheCollapseSummary` loses `chunkEmbeddingsCleared`.
- `collapseOneChat(chat)` — signature narrows from `(chat, repos, cutoffIso)` to `(chat)`; returns `{chatRows, messageRows}`.
  - Chats UPDATE (post :84–91) is now
    `UPDATE chats SET compressionCache = NULL, compiledIdentityStacks = NULL WHERE id = ? AND (compressionCache IS NOT NULL OR compiledIdentityStacks IS NOT NULL)`
    (renderedMarkdown gone from BOTH the SET and the guard — **this is the column-drop half's SQL, same statement**).
  - Messages UPDATE unchanged (:~95–105).
  - Step 3 (the `clearEmbeddingsForChat(chat.id, cutoffIso)` call) DELETED.
- `collapseStaleChatCaches(now)` :123 — `cutoffIso` gone; `const cutoffMs = retentionCutoff(await resolveStaleChatDays(), now).getTime();` (:127).
- **Log lines (verbatim):**
  - :109 `moduleLogger.info('Collapsed stale chat caches', { chatId, chatRows, messageRows })` — gate now `chatRows > 0 || messageRows > 0` (before: `|| chunkEmbeddings > 0`, and the field `chunkEmbeddings` was in the object). **Field removed.**
  - :149 `moduleLogger.warn('Failed to collapse stale chat caches — continuing', { chatId, error })` — unchanged.
  - :158 `moduleLogger.info('Stale-chat cache collapse complete', { ...summary })` — unchanged text; the spread summary LOSES `chunkEmbeddingsCleared`.
- **What the stale sweep still does:** image/asset collapse (`collapse-stale-chat-assets.ts`, unchanged code) + cache collapse of `compressionCache`, `compiledIdentityStacks`, and `chat_messages.{rawResponse, reasoningContent, reasoningSegments, renderedHtml, debugMemoryLogs}`. **No longer:** chunk embedding NULLing; `renderedMarkdown` NULLing.

### A3. `collapse-stale-chat-assets.ts` (10) — COMMENT ONLY
`isStale` doc (:87–93) now names only "(asset collapse, cache collapse)" and says chunk embeddings are not stale-gated. No code change.

### A4. `retention-constants.ts` (9) — COMMENT ONLY
Constants unchanged: `COMPLETED_JOB_RETENTION_DAYS = 7` (:22), `DEAD_JOB_RETENTION_DAYS = 30` (:30), `STALE_CHAT_RETENTION_DAYS = 30` (:42), `resolveStaleChatDays()` (:51), `retentionCutoff`. **Nothing removed or renamed.**

### A5. `scheduled-maintenance.ts` (12)
- Doc bullet 3 rewritten (comment).
- `MaintenanceSweepSummary.caches` loses `chunkEmbeddingsCleared` (:71–76 post); the initializer (:187–192) and the `runSweep(... 'caches' ...)` mapper (:217–225) lose it.
- Comment `// 3. Stale-chat cache collapse + conversation-chunk cold-tiering.` → `// 3. Stale-chat cache collapse.`
- Log: `moduleLogger.info('Scheduled maintenance pass complete', summary)` (:263) — text unchanged, the object loses `caches.chunkEmbeddingsCleared`. `'Stale-chat cache collapse failed — continuing'` unchanged.

### A6. Settings / route / client — ALL COMMENT-ONLY (⚠ refutes the ledger's "copy change" row)
- `lib/schemas/settings.types.ts` (:234–250): doc comments only. `DataRetentionSettingsSchema = z.object({ staleChatDays: z.number().int().min(1).max(3650).default(30) })` UNCHANGED. **No key removed.**
- `lib/instance-settings/index.ts` (:230–236): doc of `getDataRetentionSettings` only.
- `app/api/v1/settings/data-retention/route.ts` (:7–13): module doc only. **No response/validation change.**
- `components/settings/chat-settings/DataRetentionSettings.tsx` (:11–17): JSDoc only. **No rendered copy string changes.**
- The user-visible copy change is in **`help/data-retention.md`** (not in my file list but it is the real "copy change"): the first paragraph drops "and the mathematical fingerprints (embeddings)…"; a new paragraph "The mathematical fingerprints (embeddings) … Quilltap keeps every one of them warm…"; the "Conversation embeddings" swept-bullet removed; the never-touched list gains "conversation embeddings"; `## Search while a chat is cold` → `## Search stays warm, always` with a new body. Four other help pages also change (`chat-settings.md`, `chats.md`, `embedding-profiles.md`, `scriptorium.md`) — whole-commit help re-vendor.

### A7. `lib/scriptorium/cold-chunk-reembed.ts` (23) — COMMENT ONLY
Header retitled `Un-embedded conversation-chunk re-embedding (re-index on demand)`, cause list generalized (provider down, render died, pre-upgrade leftovers). **No code change** — the Salon-open re-embed stays wired and behaves identically. (Ledger's "now treats stale chats like any other" is true only in prose; it never distinguished stale.)

### A8. `lib/startup/reconcile-embedding-dimensions.ts` (96)
- Module doc :14–16 (conversation_chunks now "re-embedded via reindex; stale chats are treated like any other chat").
- `EmbeddingDimensionReconcileResult` and `EMPTY_RESULT` lose `staleChunkEmbeddingsCleared`.
- `countNonconforming` comment (:103–108) — NULL chunks remain NOT counted (render reconcile owns that gap). Code unchanged.
- **Deleted:** the `clearStaleChatNonconformingChunks(db, targetDim)` step in `runReconcile` and the whole function (old :301–347: DISTINCT candidate chatIds with a non-conforming chunk → `isStale` per chat → `UPDATE conversation_chunks SET embedding = NULL WHERE chatId = ? AND <NONCONFORMING>`; note it did NOT stamp updatedAt).
- `countNonconformingLiveChunks` → **renamed** `countNonconformingChunks` (:273–287). **SQL byte-identical** (`WHERE NONCONFORMING AND "chatId" IN (SELECT "id" FROM "chats") AND NOT_FAILED`). The behaviour change is purely that stale chats' non-conforming chunks are no longer NULLed first, so they are now COUNTED → reindex enqueued.
- `touchedAnything` (:243–246) loses `|| staleChunkEmbeddingsCleared > 0`.
- **Log lines:** `logger.info('Embedding dimension reconcile found non-conforming vectors', { profileId, targetDimensions, vectorEntriesDeleted, vectorIndexMetaFixed, ...mismatched, reindexEnqueued })` (:249) — **field `staleChunkEmbeddingsCleared` removed** (was between `vectorIndexMetaFixed` and the spread). Unchanged: `'Embedding dimension reconcile: corpus conforms'` debug (:258), `'Embedding dimension reconcile failed'` error (:124), `'Default embedding profile has no fixed dimension; cannot enforce conformance'` warn (:164).
- Order of work post: vector_entries DELETE → vector_indices snap → (unloadAll) → counts memories / conversation_chunks / help_docs / mount chunks → enqueue reindex → log.

### A9. `lib/background-jobs/handlers/embedding-reindex.ts` (23)
- Doc :17–20 rewritten. Imports of `isStale`, `resolveStaleChatDays`, `retentionCutoff` removed.
- `let staleChatsSkipped = 0;` removed; in Phase 3 (:266–283) the `staleCutoffMs` resolve and the `if (await isStale(...)) { staleChatsSkipped++; continue; }` skip are removed — every `repos.chats.findByUserId(job.userId)` chat is walked.
- Log `logger.info('[EmbeddingReindexAll] Reindex jobs enqueued', {context, jobId, profileId, scope, targetDim, helpDocCount, memoryCount, chunkCount, mountChunkCount, helpDocsSkipped, memoriesSkipped, chunksSkipped, mountChunksSkipped, failedSkipped, totalEnqueued})` (:325–341) — **`staleChatsSkipped` removed** (was between `mountChunksSkipped` and `failedSkipped`).
- NB: `enqueue()` (:162–188) in `mismatched-dim` scope skips only rows whose embedding matches the dim — a NULL chunk does not match, so a mismatched-dim reindex ALSO re-embeds NULL (formerly cold) chunks of stale chats. In `all` scope every chunk is enqueued.

### A10. `lib/startup/reconcile-conversation-rendering.ts` (69) — embedding side
- Doc :37–45: "This reconcile now heals STALE chats too … On the first boot after that change shipped, this scan will find every previously cold-tiered chat (NULL embeddings left over from the old sweep) and enqueue a one-time re-embed backfill for each of them; `enqueueConversationRender` dedupes…" — **THIS is the hunk that makes "first boot re-embeds the backlog" true**, together with the removal of the stale gate below.
- Imports of `isStale`/`resolveStaleChatDays`/`retentionCutoff` removed (`getRepositories` import stays — still used for the profile resolve at :175).
- `ConversationRenderReconcileResult` loses `skippedStale`.
- Loop (:209–228): the whole staleness block DELETED, including the warn **`'Staleness check failed during reconciliation; skipping chat'`** `{chatId, error}`. Every selected row now goes straight to `enqueueConversationRender(row.userId, { chatId: row.chatId })`.
- `updatedAt` STILL rides in the SELECT and in `IncompleteChatRow` (:84, :143) — now vestigial (the doc sentence explaining it was deleted).
- **Log lines post (verbatim):** `'No SQLite database available; skipping conversation render reconciliation'` (debug :161); `'Failed to resolve default embedding profile; FAILED-status exclusion disabled'` {error} (warn :178); `'Failed to scan for incomplete conversations; skipping reconciliation'` {error} (warn :194); `'Conversation render reconciliation: found incomplete conversations'` {count} (info :205, only when rows>0); `'Failed to enqueue conversation render during reconciliation'` {chatId, error} (warn :219); `'Conversation render reconciliation complete'` {incompleteChats, enqueued, reused, failed} (info :230 — **`skippedStale` removed**).
- Arm (A) (:87–96) changed `c."renderedMarkdown" IS NULL` → `NOT EXISTS (SELECT 1 FROM "conversation_chunks" cc0 WHERE cc0."chatId" = c."id")` — the column-drop half's hunk, but it lives in the same SQL const; one lane must own the file. Arms (B)/(C) unchanged.
- Cost driver of the backlog: `handleConversationRender` (post :58–87) enqueues `EMBEDDING_GENERATE` for every chunk with `!chunk.embedding` (not only the newest; `fullReembed` false) → **one provider embed call per un-embedded, non-FAILED, within-cap chunk**, plus one CPU re-render per chat.

### A11. `instrumentation.ts` (+5) / `lib/startup/prettify.ts` (+1)
- `instrumentation.ts` :927–931 — PHASE 3.7 comment only ("…enqueues a mismatched-dim reindex for anything that needs re-embedding (stale chats included…)"). No code change; still fire-and-forget.
- `prettify.ts` :202 — new label `'drop-chat-rendered-markdown-v1': 'Clearing the duplicate transcripts from the shelves…'` (belongs with the migration — sibling half; cited for completeness).

### A12. Tests (ready-made oracle shapes)
- `conversation-render.test.ts`: `characters.findById` → `findByIdRaw` mock; renamed test "renders, stores chunks (not the Markdown)…"; `characterNames.has('participant-2')` false (no-character seat left to the renderer's own 'User'); `mockUpdateChat` NOT called. (Render-on-demand half.)
- `collapse-stale-chat-caches.test.ts`: repos mock is `{ chats }` only; chats SQL contains `compressionCache = NULL`, `compiledIdentityStacks = NULL`, **not** `renderedMarkdown`; **no rawQuery call contains `conversation_chunks`**; summary has no `chunkEmbeddingsCleared`; idempotent second pass unchanged.
- `restore-embedding-reconcile.test.ts`: `CLEAN_RECONCILE` drops `staleChunkEmbeddingsCleared` (shape only).
- DELETED `conversation-chunks-clear-embeddings.test.ts` (4 cases: no cutoff, cutoff guard, 0 changes, throw→0).
- `reconcile-conversation-rendering.test.ts`: `isStale`/retention mocks gone; `skippedStale` gone from every expected result; NEW "keys arm (A) on messages with no conversation_chunks rows at all" (SQL `not.toContain('renderedMarkdown')`, contains `NOT EXISTS`, `"conversation_chunks" cc0`, role IN); "enqueues a render for a stale chat too" → `{incompleteChats:2, enqueued:2, reused:0, failed:0}` with `chat-quiet` enqueued; the "staleness cannot be determined" case DELETED.
- `reconcile-embedding-dimensions.test.ts`: stale mocks gone; "counts non-conforming chunks on stale AND live chats alike, excluding only orphans" → stale row's embedding **not** NULL, `mismatched.conversationChunks === 2` (was 1), `reindexEnqueued === true`.
- `embedding-reindex.test.ts`: stale mocks gone; both scopes now `toContain('cc-stale-old')`.

### A13. What the commit message claims that the hunks do NOT do
1. "the startup render reconcile, the embedding reindex and the dimension reconcile now treat stale chats like any other" — true; but `cold-chunk-reembed.ts` (which the ledger lists as changed behaviour) is **comment-only**.
2. The ledger row's "`retention-constants.ts`, `settings.types.ts` … drop the cold-tier" and "The data-retention route + `DataRetentionSettings` copy change" — **all comment/JSDoc only**: no schema key, no validation, no response shape, no rendered string changes. The only user-visible copy change is `help/data-retention.md` (+4 other help pages).
3. "so the first boot re-embeds the backlog" — happens via the render reconcile's arm (B) + removed stale gate (A10), NOT via the dimension reconcile (NULL chunks are still not counted there, A8). The dim reconcile only adds stale chats' *non-conforming* (wrong-dim, non-NULL) chunks.
4. The message does not mention the per-chat debug `'[ConversationRender] Chat has no events, nothing to render'` (new, render-on-demand half), nor that `updatedAt` is now dead weight in the reconcile SELECT.
5. `85% of chunks on Friday` / `~14 MB` (msg) vs `~16 MB` (code comment, spec doc) — prose inconsistency, harmless.

---

## B. v5 on `main` — the sites

| Concern | v5 file:line |
|---|---|
| `clear_embeddings_for_chat(chat_id, now_iso, older_than)` | `crates/quilltap-core/src/db/conversation_chunks.rs:492–535` (doc :12 names `clearEmbeddingsForChat` in the module header). **Callers:** `services/collapse_stale_chat_caches.rs:131` + `crates/quilltap-harness/tests/conversation_chunks_tier2_equivalence.rs:383` — no others. (`doc_mount_chunks.rs:491 clear_embeddings_by_link_id` / `mount_index/embedding_scheduler.rs:156` are the embed:false MOUNT path — UNRELATED; the ledger's listing of `embedding_scheduler.rs` is a false hit.) `find_cold_chunk_ids_by_chat_id` (:537–557) stays (cold_chunk_reembed uses it). |
| Cache collapse | `services/collapse_stale_chat_caches.rs` (183 lines): module doc :1–41; summary field `chunk_embeddings_cleared` :83–84; `collapse_one_chat(db, chat_id, now_iso, cutoff_iso)` :90–140 (chats UPDATE :106–113 incl. `renderedMarkdown`; step 3 cold-tier :127–135); `collapse_stale_chat_caches` :147–183 (cutoff_iso/now_iso :151–153; accumulation :170–176). **v5 has NONE of v4's three log lines** (`Collapsed stale chat caches`, `Failed to collapse stale chat caches — continuing` — the `Err(_)` arm :178 is silent, `Stale-chat cache collapse complete`) — pre-existing absent lines. |
| Scheduled maintenance | `services/scheduled_maintenance.rs`: doc :13; `caches_chunk_embeddings_cleared` :93, init :197, mapper :239 (comment) / :246, log field :398 in `"Scheduled maintenance pass complete"` (:387–405). |
| Staleness gate | `services/maintenance.rs:218–224` doc (names chunk cold-tiering + the reconcile), `is_stale` :229, `is_stale_conn` :236, :315 comment. Stays live (asset collapse :326, cache collapse :162, Almanack `almanack/phase3_ledgers.rs:833`). |
| Retention window | `services/queue_service.rs:951` doc (`resolve_stale_chat_days`), `:962 resolve_stale_chat_days_conn`. |
| Render reconcile | `services/conversation_render_reconcile.rs` (591): module doc :38–46 (STALE excluded), `use` :75–76 (`is_stale_conn`, `resolve_stale_chat_days_conn`, `retention_cutoff_iso`) + `iso_to_ms` :77; SQL const :104–148 (arm A `renderedMarkdown IS NULL` :107); `ReconcileResult.skipped_stale` :161–162; `IncompleteChat.updated_at` :169–172; `reconcile_conversation_rendering(main, now_ms)` :184 — stale block :222–254 incl. the warn `"Staleness check failed during reconciliation; skipping chat"` :247–252; tests `stale_chats_are_skipped_not_healed` :~481–501, `skipped_stale` asserts :447, :498; test DDL has `chats.renderedMarkdown` (sibling half). `now_ms` becomes DEAD after the stale block goes (only use is the cutoff). v5 resolves the profile via `db/embedding_profiles.rs:154 pick_reembed_profile_id` (marked-default only — matches v4 post). |
| Render reconcile boot call + log | `crates/quilltap-host/src/host.rs:1418–1454` inside `seed_built_ins` (:1155…) — comment :1418–1429 (STALE excluded), call :1430–1434, log `"Conversation render reconciliation complete"` :1444–1453 with `skipped_stale` :1450, comment :1436–1443. ⚠ v5 has no `"…: found incomplete conversations" {count}` pre-loop line (pre-existing; the dogfood #-finding comment admits v4 has two lines). |
| Boot ORDER | `host.rs` `assemble`: `seed_built_ins(db)?` :499 (render reconcile = v4 PHASE 3.6, inside it) → `reconcile_help_docs_at_boot` :521 (3.66) → `reconcile_embedding_dimensions_at_boot` :525 (3.7). Matches v4. **`spine.rs` runs none of these** (its only hit :1680 is an unrelated comment) — the ledger's `quilltap-host/src/spine.rs` surface is a false hit; the boot owner is `host.rs`. |
| Dim reconcile | `services/embedding_dimension_reconcile.rs` (1106): doc :15–18; `use` :79–81; `stale_chunk_embeddings_cleared` :140–141; `reconcile_embedding_dimensions(main, mount, now_ms)` :259 / `run_reconcile` :277; stale step :339–345; `count_nonconforming_live_chunks` :417–440 (rename to `count_nonconforming_chunks`); `touched_anything` :382–385; INFO log field :397; `clear_stale_chat_nonconforming_chunks` :443–512 (delete); test `nulls_stale_chunks_and_counts_only_live_ones` :833–871 (assert :857). `now_ms` becomes DEAD (sole use :344/:491–495). Callers: `host.rs:1113`, `services/backup/restore/orchestrator.rs:1350`, `harness/tests/embedding_remainder_equivalence.rs:775`. |
| Dim reconcile boot log | `host.rs:1091–1146` `reconcile_embedding_dimensions_at_boot` — comment :1101–1103 ("converge stale chats to the cold tier"), log `"Embedding dimension reconciliation complete"` field `stale_chunk_embeddings_cleared` :1126. |
| Reindex job | `services/embedding_reindex_job.rs` (650): doc :60–64; `stale_chats_skipped` :175–176; log field :404 in `"[EmbeddingReindexAll] Reindex jobs enqueued"` (:391–409); phase-3 stale cutoff :499–507 and skip :516–522. |
| Cold re-embed | `services/cold_chunk_reembed.rs:1–21` module doc (retitle per v4 A7; code unchanged). Salon call site `api/salon.rs:184–197` comment ("if the maintenance sweep cold-tiered…"). |
| Instance-settings / retention (no behaviour change) | `db/instance_settings.rs:44–55` (doc :46 names cold-tiering), `validate_stale_chat_days` :249, `get_data_retention_settings` :268, `set_data_retention_settings` :292; core `api/settings.rs:2799 data_retention_settings_get`; web `crates/quilltap-web/src/lib.rs:405–409` → `text_replacements_routes.rs:191–…` (`data_retention_settings_get` :226, `_put`). No edits required beyond comments. |
| SPA | `apps/web/src/app/screens/settings/chat/data-retention-settings.ts` — JSDoc :10–21 (names "rendered markdown … cold-tier chunk embeddings"); template copy :31–37 matches v4's UNCHANGED JSX (no copy edit). DTO twin: `apps/web/src/app/core/core-contract.ts:6431–6451` (`DataRetentionSettingsDto {staleChatDays}` + the update input), client `core/core-client.ts:268–276` (`dataRetentionSettingsUpdate`). Spec `data-retention-settings.spec.ts`. No field v4 removed → nothing the SPA writes can trip a strict PUT. |
| Help | vendored `help/data-retention.md` (+ the other four pages) — re-vendor byte-copy; 129-file count unchanged (no add/delete). |

---

## C. Families + fixtures that move

| Family | Recipe header / env | Moves how |
|---|---|---|
| `crates/quilltap-harness/tests/collapse_stale_chat_caches_tier2_equivalence.rs` (290) | `QT_ORACLE_RETENTION_CACHES` + `QT_FIXTURE_RETENTION_CACHES`; fixture `harness/oracle/fixtures/build-retention-caches-fixture.ts` (spec `retention-caches.json`), case `harness/oracle/cases/collapse-stale-chat-caches-tier2.ts` (header :33–43) | Summary loses `chunkEmbeddingsCleared`; the `chunks` projection must show the stale chat's OLD embedded chunk **spared** (was cold-tiered with a minted updatedAt); the P4.D25 warmth-window design (cutoff-stamped / in-window chunks) becomes moot — all four chunks survive. Case projection selects `renderedMarkdown` (:80) and the builder writes it (:110, :178) → column-drop coupling. |
| `conversation_chunks_tier2_equivalence.rs` (437) | `QT_ORACLE_CONVERSATION_CHUNKS`; builder `fixtures/build-conversation-chunks-fixture.ts`, case `cases/conversation-chunks-tier2.ts` (header :45–57) | The case calls `repo.clearEmbeddingsForChat(` (:121) — **at the target pin that is a TypeError and the oracle dies** (red-first for free). Spec `fixtures/conversation-chunks-tier2.json` carries **5** `clearEmbeddings` ops; the case's normalization note (:22) and the Rust `Op::ClearEmbeddings` arm (:378–389) + `CLEAR_NOW_ISO` (:159–162) + `got_cleared` go. |
| `maintenance_ops_tier2_equivalence.rs` (298) | `QT_ORACLE_MAINT_OPS` (+ `QT_FIXTURE_MAINT_OPS_MAIN/MOUNT`); builder `fixtures/build-maintenance-ops-fixture.ts`, case `cases/maintenance-ops-tier2.ts` (header :21–33) | Summary JSON projects `"chunkEmbeddingsCleared": summary.caches_chunk_embeddings_cleared` (:188) — v4's `runScheduledMaintenance()` summary at target lacks the key → red until removed. Not in the orchestrator's brief but it moves. |
| `embedding_remainder_equivalence.rs` (901) — **SHARED with the column-drop half** | `QT_ORACLE_EMBEDDING_REMAINDER`, jest oracle `cases/embedding-remainder.test.ts` over the COMMITTED pair `crates/quilltap-web/tests/fixtures/embedding-remainder-{main,mount}.db` + spec `fixtures/embedding-remainder.json` (header :96–117); `TZ=UTC` | Phase 1 reconcile deserializes `v["skippedStale"].as_u64().unwrap()` (:450 — panics on a target-pin oracle) and guards `want_reconcile.skipped_stale >= 1` (:655–665); phase 2b deserializes `staleChunkEmbeddingsCleared` (:481) and guards `any(|r| r.stale_chunk_embeddings_cleared > 0)` (:733). At target the stale corpus chats are ENQUEUED (expect `enqueued` to rise by the stale count; the header's `{incompleteChats:5, enqueued:3, reused:1, skippedStale:1}` becomes `{…, enqueued:4, reused:1}`-shaped — MEASURE, don't predict), the stale chunk non-conforming arm now COUNTS (+1 conversationChunks, reindex enqueued), and phase 3's reindex enqueues `cc-stale-*` rows. The `chats` table diff also compares `renderedMarkdown` byte-exact (header :36–40) — the sibling's concern, same family. The oracle spec's "every counter non-zero somewhere" guards need the two stale counters retired. |
| `cold_chunk_reembed_tier2_equivalence.rs` (205) | `QT_ORACLE_COLD_REEMBED` + `QT_FIXTURE_COLD_REEMBED`; builder `fixtures/build-cold-reembed-fixture.ts`, case `cases/cold-chunk-reembed-tier2.ts` (header :15–31) | v4 code UNCHANGED → expected NEUTRAL; regen at target as a neutrality proof only (fixture at baseline, see D4). |
| `maintenance_sweep_tier2_equivalence.rs` (assets; `cases/maintenance-sweep-tier2.ts`) | — | Neutral (asset collapse comment-only). |
| `quilltap-host/tests/host_boot.rs`, `host_cadence.rs` | — | **No reference** to stale/cold/reconcile fields (grep). `host_help_boot_order.rs:69` keys on the `"Embedding dimension reconciliation"` message PREFIX — survives a field removal. |
| settings / data-retention | `crates/quilltap-harness/tests/settings_routes_equivalence.rs`, `instance_settings_json_warns_equivalence.rs`, `crates/quilltap-web/tests/data_retention_web_routes.rs` | **Neutral** (no schema/route change). |
| core unit tests | `conversation_render_reconcile.rs` `stale_chats_are_skipped_not_healed` + the `skipped_stale` asserts; `embedding_dimension_reconcile.rs:833 nulls_stale_chunks_and_counts_only_live_ones`; `collapse_stale_chat_caches.rs` has no unit tests; `realtime/publish_sites.rs:1450–1480` (sweep silence pin — survives, it only needs `chat_rows_cleared > 0`); `source_census/mod.rs:269` names the collapse file's clean UPDATE (re-check the census if the SQL text changes). | Invert to v4's new shapes (A12). |
| help | `help_tree_equivalence` et al. | Five pages re-vendored; count stays 129. |

---

## D. Traps + premises to measure

1. **First-boot re-embed cost (the headline).** Trigger in v5 = the render reconcile in `host.rs:1430` (inside `seed_built_ins`, before the job pump) once its stale gate is gone; each selected chat → `CONVERSATION_RENDER` → `conversation_render_job.rs:286` enqueues an `EMBEDDING_GENERATE` for every chunk with `!has_embedding`. Cost driver = **count of un-embedded, non-FAILED, 1..EMBEDDING_MAX_CHARS chunks on non-orphan chats** (arm B), × the default profile's per-token price. Last measured (dogfood-findings.md:408–455, 2026-07-28): **9,652/11,357 chunks unembedded, 9,098 cold-tiered by design; ~598 incomplete chats; pre-fix boot ≈ 671 renders + ~9,609 embeddings ≈ ~$2 OpenAI (text-embedding-3-large).** Today's number is unknown — Bug 17's arm C and two months of chat have moved it.
   - **Measure before booting (free):** on the copy, `SELECT COUNT(*) FROM conversation_chunks cc WHERE embedding IS NULL AND LENGTH(qt_text(content)) BETWEEN 1 AND 131072 AND chatId IN (SELECT id FROM chats) AND NOT EXISTS (…FAILED for default…)` and the distinct-chat count (the reconcile SQL itself, via `quilltap db` or a harness read).
   - **Gate/measure the boot at zero spend:** the "bad API key is a free provider-wire proof" memory applies, or boot with no `isDefault` embedding profile — the reconcile still logs `incomplete_chats`/`enqueued` and the renders run, but the render handler enqueues no embeds (and the dim reconcile skips `no-profile`). Re-point the default afterwards for the paid drain.
   - ⚠ **v4 may already have paid this on live Friday.** v4 shipped dev.96 on 2026-09-26; if the human has run v4 on the live instance since, the backlog is ALREADY warm in the next Friday copy, and the v5 boot proof reduces to "enqueues ~nothing". Worse, an UNPORTED v5 running its nightly sweep over that copy would cold-tier it all over again (v5 `collapse_stale_chat_caches.rs:131`) — moot only because v5 main can't read a post-dev.96 copy anyway (the `renderedMarkdown` DROP). Probe the copy's NULL-embedding count first; record which case the walk is in.
2. **Retired-setting write by the SPA:** none. v4 removed no setting/key/field in this half; `dataRetention.staleChatDays` and its 1..3650 int validation are unchanged. Nothing can trip a strict PUT.
3. **Other callers of `clear_embeddings_for_chat`:** only the cache collapse (:131) and the `conversation_chunks_tier2` harness op. Deleting it is clean; `find_cold_chunk_ids_by_chat_id` must STAY (cold re-embed).
4. **Dead parameters after the deletions:** `now_ms` in `reconcile_embedding_dimensions`/`run_reconcile` (callers host.rs:1113, restore orchestrator.rs:1350, embedding_remainder :775) and in `reconcile_conversation_rendering` (host.rs:1431, embedding_remainder, unit tests); `IncompleteChat.updated_at` (v4 keeps the column in the SELECT — keep reading it for fidelity or drop with a note; either way clippy's dead_code will flag an unread field); the three `use` lines for `is_stale_conn`/`resolve_stale_chat_days_conn`/`retention_cutoff_iso` in both reconcilers; `iso_from_unix_ms`/`now_iso` in the collapse. Decide signature changes deliberately (a cross-crate API change).
5. **P4.D233 F1 fixture rule:** every `/tmp`-built fixture here is built through v4's own `ensureCollection` + `ChatMetadataSchema` (`build-retention-caches-fixture.ts` also raw-UPDATEs `renderedMarkdown` at :178; maintenance-ops / cold-reembed / conversation-chunks builders likewise create `chats`). **Built at the target pin they lack `chats.renderedMarkdown`**, and v5 main (still binding that column) cannot use them; the retention builder would even throw at :178. Build fixtures at `acadcc7cd`, run oracles at `f7f3d7bf0` — or land the column-drop half FIRST, then rebuild at target. The committed `embedding-remainder-*.db` pair is baseline vintage (has the column): the target oracle over it is fine for v4 (Zod strips the extra) but the sibling's column-drop will need that pair NARROWED.
6. **File-ownership overlap with the sibling half:** `collapse_stale_chat_caches.rs` (the chats UPDATE's `renderedMarkdown` is the sibling's; step 3 is mine — same statement/function), `conversation_render_reconcile.rs` (arm A is the sibling's, the stale gate mine — same const and function), `embedding_remainder_equivalence.rs` + its oracle (both halves' assertions). Recommend ONE lane owns all three, or stack mine on the column drop.
7. **Log-line fidelity:** removing `skipped_stale` / `stale_chunk_embeddings_cleared` / `stale_chats_skipped` / `caches_chunk_embeddings_cleared` fields is the whole port for the log side. Optional pre-existing riders surfaced by this survey: the collapse's three absent v4 lines; the render reconcile's absent pre-loop `found incomplete conversations` line (v5 logs completion from the host, gated `incomplete_chats > 0`, which matches v4's early return).
8. **Behaviour to remember in the reindex job:** `embedding_reindex_job.rs:519` `is_stale(...)?` currently PROPAGATES an error (a v5 nuance) — goes away with the skip.
9. **Ledger corrections to feed back:** (a) settings/route/component hunks are comment-only; (b) `cold-chunk-reembed.ts` is comment-only; (c) `spine.rs` and `mount_index/embedding_scheduler.rs` are false-hit surfaces (boot owner = `host.rs`); (d) `maintenance_ops_tier2` moves and is not in the ledger's family list; (e) `host_boot`/`host_cadence` do not move.

---

## E. Size + tier

- **Core (this half):** deletion-dominated, ~6 files — `db/conversation_chunks.rs` (−~45), `collapse_stale_chat_caches.rs` (−~25 + doc), `scheduled_maintenance.rs` (−4), `embedding_dimension_reconcile.rs` (−~75 + rename + test inversion), `conversation_render_reconcile.rs` (−~35 + test inversion), `embedding_reindex_job.rs` (−~15), `host.rs` (2 log fields + 2 comments), doc-only touches in `cold_chunk_reembed.rs`, `maintenance.rs`, `queue_service.rs`, `instance_settings.rs`, `api/salon.rs`, SPA `data-retention-settings.ts` JSDoc; five help pages re-vendored. ≈ 250 lines net removed, ~40 added.
- **Harness:** four families move (collapse, conversation_chunks, maintenance_ops, embedding_remainder) + one neutral regen (cold_chunk_reembed), each regenerated at `f7f3d7bf0` with fixtures at `acadcc7cd`; red-first is free on three of them (TypeError / missing keys).
- **Estimate:** a small–medium lane, ~0.5 day of agent time alone; more if it absorbs `embedding_remainder` jointly with the column drop.
- **Tier:** the core deletions are Sonnet-grade mechanical work, but the lane should be **Opus**: `embedding_remainder`'s corpus/guard rework, the fixture-pin choreography (F1), the shared-file ownership with the column-drop half, and the first-boot cost measurement all need judgement. Suggested split: one Opus lane owning this half + the three shared files (stacked on or merged with the column drop), with a Sonnet sub-agent for the doc/comment sweep and the help re-vendor.

---

## Summary (10 lines)
1. v4 removes `ConversationChunksRepository.clearEmbeddingsForChat` (sole caller: the cache collapse) and the collapse's step 3; the sweep keeps image + cache collapse (`compressionCache`, `compiledIdentityStacks`, five message columns).
2. `chunkEmbeddingsCleared` leaves both summaries and two log lines; `skippedStale`, `staleChunkEmbeddingsCleared`, `staleChatsSkipped` leave the render reconcile, dim reconcile and reindex (fields + one warn deleted).
3. The dim reconcile's `clearStaleChatNonconformingChunks` is deleted; `countNonconformingLiveChunks` → `countNonconformingChunks` with byte-identical SQL.
4. **Surprise:** the settings schema, instance-settings, data-retention route and `DataRetentionSettings.tsx` hunks are ALL comment-only — no key, validation, response or rendered-copy change; the real copy change is `help/data-retention.md`. `cold-chunk-reembed.ts` is comment-only too.
5. The first-boot backlog re-embed comes from the render reconcile (arm B + the deleted stale gate), not the dim reconcile, which still ignores NULL chunks.
6. Cost driver = un-embedded non-FAILED in-cap chunks (last measured 9,652/11,357, ~$2 on text-embedding-3-large); measure with the reconcile SQL on the copy, and gate the boot with no default profile or a bad key.
7. ⚠ v4 may already have warmed live Friday since 2026-09-26, so probe the copy first; an unported v5 would cold-tier it again.
8. v5 sites: `db/conversation_chunks.rs:492`, `collapse_stale_chat_caches.rs:83/127–135`, `scheduled_maintenance.rs:93/246/398`, `embedding_dimension_reconcile.rs:141/339–345/417/443–512`, `conversation_render_reconcile.rs:162/222–254`, `embedding_reindex_job.rs:176/404/499–522`, `host.rs:1126/1450`; the boot owner is `host.rs`, not `spine.rs`.
9. Families that move: collapse_stale_chat_caches, conversation_chunks (its oracle TypeErrors at target), maintenance_ops (not in the ledger), embedding_remainder (shared with the column drop); cold_chunk_reembed is neutral; host_boot/host_cadence don't move.
10. Fixtures at `acadcc7cd`, oracles at `f7f3d7bf0` (F1). Three files overlap with the column-drop half, so one Opus lane should own them; the net change is small and deletion-heavy.
