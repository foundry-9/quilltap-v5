# Setupphase survey — the IMPORT-DEAD / re-aim census, the two ahead-of-pin riders, the vendor set, and the lane-start preflight (round pinned at v4 `e8e17b2d9`)

**Date:** 2026-10-19. **Read-only survey** (nothing built, run or written
except this file).

**Pin:** v4 `e8e17b2d92095bda008f03bb963eb4b1c470e680` (`main`, 2026-10-18
14:21 −0500, "Fix: restore and import keep chat outfits (bug 252) (#101)").
**Baseline:** `25b8fab6a`. **Span:** 14 commits, `5cb0e0190` … `e8e17b2d9`
(182 files, +10,238 / −271).

**v4 facts** come only from `git -C ~/source/quilltap-server show
<sha>:<path>` and `git diff 25b8fab6a e8e17b2d9`. The live checkout is at
`a6292f169`, past the pin, and carries the Inventory. **v5 line numbers** are
at v5 `main` (`fb4a4c264`). Port facts come from the shipped hunks, never from
commit messages.

---

## §A — The IMPORT-DEAD / re-aim census at the pin

### A.0 Verdict

**IMPORT-DEAD: zero.** No value export was deleted or renamed in
`lib/ app/ packages/ plugins/ migrations/`. No file was deleted, renamed or
copied (`git diff --name-status --diff-filter=DRC` is empty).

Three families still break at the pin, though, for reasons no single lane
owns:

1. **`outfit_change_effects_tier3_equivalence`** — a positional signature
   change. `recordPendingWardrobeAnnouncement` gained a leading `repos`
   argument. The oracle's two `record` steps now pass the context object as
   `repos` and a string as `context`. Nothing throws. The turn set silently
   never gains B, so every later step's `set` and the flushes' `jobs`
   diverge.
2. **`wardrobe_tools_equivalence`** — the context has no `provenance`. Every
   outfit-changing op gains one captured v4 WARN that v5 never emits.
3. **Every `createBackup` regen** — an unguarded raw read of a
   migration-only table. `backup-service.ts:291` throws
   `no such table: chat_wardrobe_history` on every oracle fixture. This kills
   `system_backup_equivalence`, `backup_uuid_remap_equivalence`, and the six
   archive builders behind `system_restore*` / `restore_vintage_state`.

**Recommendation:** no stage-0 REAIM lane for #1 and #2. Each is the history
keystone's own port surface, and re-aiming it ahead of the port only turns it
red against unported v5. #3 is the one REAIM candidate, and the decision rule
is in §A.4.

### A.1 Exports deleted or renamed in `git diff 25b8fab6a e8e17b2d9 -- lib app packages plugins migrations`

Every `^-.*export` line in the diff, with what replaced it at the pin:

| Removed line (diff) | At the pin | Runtime effect | Oracle hits (`ggrep -ra` over `harness/oracle/`) |
|---|---|---|---|
| `app/api/v1/chats/[id]/actions/index.ts:31` `export { handleGetOutfit, handleGetOutfitSummary, handleEquipSlot } from './outfit'` | The same line, widened with `handleGetWardrobeHistory, handleWardrobeRollback` | None. Every old name survives. | `wardrobe-routes.test.ts:319-328,434-436`, `outfit-change-effects-tier3.ts:141`, `wardrobe-wear-commit-tier2.ts:143`. They import `outfit` directly, not the barrel. |
| `lib/pascal/wardrobe-effects.ts` `export interface ApplyWardrobeEffectsParams` | `:77` `export interface ApplyWardrobeEffectsParams extends WardrobeTurnContainers` (`pendingWardrobeHistory?` added) | Type only, erased by tsx. | 0 |
| `lib/tools/handlers/wardrobe-handler-shared.ts` `export interface WardrobeToolContext` | `:17` `export type WardrobeToolContext = Omit<OutfitChangeContext, 'historyIntent'>`, which makes `provenance` REQUIRED | Type only. The runtime consequence is A.2's. | 1, a comment (`wardrobe-tools.ts:31`) |
| `lib/wardrobe/outfit-change-effects.ts` `export interface OutfitChangeContext` | `:45` re-declared: `provenance: WardrobeHistoryProvenance` (`:50`, required), `historyIntent?`, extends `WardrobeTurnContainers` | Type only. Runtime consequence in A.2. | 0 |

**Renamed or rewritten members** (no `export` line moved):

- **`WearablePool.findByTitle`** (`lib/wardrobe/pool.ts`). The body now
  delegates to `matchWearableTitle` (`lib/wardrobe/wearable-ref.ts:30-34`).
  The rule is unchanged: trimmed, case-insensitive, own items first, then the
  non-own wearable set.
  - Callers: `wearable-pool.ts:303` → `wearable_pool_equivalence.rs`
    (+`wearable_pool.rs`); `wearable-pool-tier2.ts:179-180` →
    `wearable_pool_tier2_equivalence.rs`.
  - `item-mutations-tier2.ts:170` is a local helper of the same name, not
    the pool's.
  - **Survives** (contract identical).

**Barrel / specifier census:** zero hits for any of the four names as an
import specifier.

### A.2 The `OutfitChangeContext` shape break (`05005c562` + `4ae20977f`)

**v4 at the pin.** What each access does at runtime when a caller omits
`provenance`:

- `outfit-change-effects.ts:131`
  `recordPendingWardrobeAnnouncement(repos, context, sourceContext)`
  - The NEW leading `repos` argument is the positional break.
  - `:136` builds
    `intent = context.historyIntent ?? { kind: 'append', provenance: context.provenance }`.
    A missing `provenance` gives `{ kind: 'append', provenance: undefined }`
    and does not throw here.
  - `:137-145`: with a turn Set, the character joins the Set. If the context
    carries `pendingWardrobeHistory`, the intent waits in that map. If it does
    NOT, `settleHistory` runs **immediately** (`:144`).
  - Without a Set, `:148` schedules the announcement, then `:149` settles.
- `chat-wardrobe-history.repository.ts:177` `settle` wraps everything in
  `try`.
  - `settleAppend` destructures `intent.provenance` (`:207`), which throws
    `TypeError` on `undefined`.
  - A missing `chat_wardrobe_history` table throws on the `db.prepare` inside
    `runAtomically`.
  - Both are caught at `:186`, which logs
    **`log.warn('Wardrobe history settle failed; the outfit change stands unrecorded', {chatId, characterId, intent, error})`**
    through `logger.child({ module: 'wardrobe-history' })` (`:51`).
  - The child shares the root logger's `transports` array **by reference**
    (`lib/logger.ts:83-86`), so an in-place `transports.splice` capture DOES
    see it.
- `outfit-change-effects.ts:75`, the WARN `Failed to settle wardrobe history`,
  fires only if `settle` itself throws. `settle` never throws, so this line
  never fires.
- `lib/wardrobe/history/rollback.ts:179` `context.provenance.source` is the
  ONE hard-throwing access. It is new, and no oracle reaches it.
- `apply-outfit-selections.ts:544` calls
  `await repos.wardrobeHistory.settle(...)` **outside any `try`**.
  - It throws only if `repos` lacks `wardrobeHistory`.
  - Both oracle callers pass the real `getRepositories()`, which has it
    (`repositories/index.ts` `wardrobeHistory: new ChatWardrobeHistoryRepository(chatsRepo)`).
  - So these calls get the caught WARN, not a throw.
- **The pin's own callers** build contexts this way:
  - `tool-executor.ts:736-737`:
    `provenance: { source: 'character', ...(characterId ? { actorCharacterId } : {}) }, ...turnContainersOf(context)`
  - `tool-execution.service.ts:321` `createToolContext` →
    `...newWardrobeTurnContainers()`, i.e. BOTH containers.
  - `wardrobe-effects.ts:226`:
    `provenance: { source: 'pascal', actorCharacterId: characterId, toolName }`
  - Brahma (`one-shot.service.ts`, `orchestrator.service.ts`) uses
    `...newWardrobeTurnContainers()`.

**Every oracle case that builds or threads one** (`ggrep -ra` over
`pendingWardrobeAnnouncements | OutfitChangeContext | applyWardrobeEffects |
commitEquippedOutfit | applyOutfitSelections | createToolContext |
WardrobeToolContext | notifyWardrobeChanged | recordPendingWardrobeAnnouncement
| flushPendingWardrobeAnnouncements`, 20 files):

| Case (file:line) | What it builds / calls | At the pin | Family fed |
|---|---|---|---|
| `cases/outfit-change-effects-tier3.ts:201-208` (`record` op) | `effects.recordPendingWardrobeAnnouncement({userId, chatId, characterId, ...set}, step.context)`, the OLD two-argument call | **BREAKS.** v4 binds `repos` = the context object, `context` = the string (`'wardrobe-wear-handler'`), `sourceContext` = `undefined`. `''.pendingWardrobeAnnouncements` is undefined, so it takes the schedule leg with `args` = a string. `enqueueWardrobeOutfitAnnouncement(undefined, {chatId: undefined, characterId: undefined})` then fails into the captured `Failed to schedule wardrobe outfit announcement` WARN with `undefined` fields, or writes a keyless job. **Character B never joins the shared `turnSet`.** `settleHistory(ctxObj, …)` then throws on `ctxObj.wardrobeHistory` and is caught (`:75` WARN, not in `CAPTURED`). Corpus steps hit: `record_on2_b_with_set` (`outfit-change-effects-tier3.json:130-136`) and `record_on2_a_without_set_schedules` (`:138-144`). Because `turnSet` is shared and dumped on every line, every LATER step's `set`, and the `flush` steps' `jobs`, diverge too. | `outfit_change_effects_tier3_equivalence.rs` |
| `cases/outfit-change-effects-tier3.ts:189-197` (`notify`), `:230-240` (`flush`/`flushNoSet`), `:247` (`equip`) | No `provenance`, Set only | Survives. `notify` takes the immediate-settle fallback; the WARN is filtered out by `CAPTURED` (`:68-71`). `flush` settles a defaulted intent; a missing table gives a caught WARN, not captured. `equip` runs the real route with `operatorWardrobeProvenance(chat)`. | same family |
| `cases/wardrobe-tools.ts:176,184` | `base = { userId, chatId, characterId, pendingWardrobeAnnouncements }`, with no `provenance` and no `pendingWardrobeHistory` | **REDS.** It captures EVERY v4 line (`:120-136`, `LOG_LEVEL=debug` `:103`), and the Rust side compares per-op `logs` (`wardrobe_tools_equivalence.rs:399-413`). Every op that changes an outfit (the 7 `wardrobe_wear`, 8 `wardrobe_take_off`, `wardrobe_create` with `equip_now`, and archive-of-worn) goes `finalizeWardrobeMutation` → `notifyWardrobeChanged` → Set present but no map → immediate `settle` → caught `Wardrobe history settle failed; …` WARN. That is one extra v4 line per such op. | `wardrobe_tools_equivalence.rs` |
| `cases/wardrobe-tools-avatar-trigger.ts:112-118` | Same shape (`ctx` with Set, no `provenance`) | Survives. `LOG_LEVEL=error` (`:67`) suppresses the WARN, and it compares `successes` + `pendingAnnouncements` only. | `wardrobe_tools_avatar_trigger_equivalence.rs` |
| `cases/pascal-wardrobe-effects.ts:163-170` | REAL `applyWardrobeEffects` with an optional Set | Survives. The pin supplies `provenance` itself (`wardrobe-effects.ts:226`), and the settle WARN carries no `context` field, so the filter `fields.context === 'pascal.wardrobe-effects' \|\| COMMITTED` (`:55-56,173`) drops it. | `pascal_wardrobe_effects_equivalence.rs` |
| `cases/pascal-side-effects.test.ts:56,71,167-168,212-215` | `applyWardrobeEffects` is `jest.mock`ed; the case dumps its argument (Set sorted) | Survives. The pin forwards `...turnContainersOf(params)` (`side-effects.ts`), which passes only the containers that are present, so the dumped argument is the same. | `pascal_side_effects_equivalence.rs` |
| `cases/wardrobe-wear-commit-tier2.ts:142-143,158,175,189` | `commitEquippedOutfit`, real `applyOutfitSelections(…, repos)`, the real `handleEquipSlot` | Survives. Settle's WARN is not in `CAPTURED` (`:66-85`). `:544`'s settle has the real repos. The `ledger` / `equippedOutfit` dumps are untouched. | `wardrobe_wear_commit_tier2_equivalence.rs` |
| `cases/chat-dialogs-llm-choose-tier3.test.ts:376-377` | Real `applyOutfitSelections(c.chatId, …, getRepositories())` | Survives. No log capture; settle is caught. | `outfit_llm_choose_tier3_equivalence.rs` |
| `cases/wardrobe-routes.test.ts:319-328,434-436` | `handleGetOutfit`, `handleEquipSlot` (route handlers) | Survives. The equip handler adds a `chats.findById` and a caught settle; no capture. | `wardrobe_routes_equivalence.rs` |
| `cases/brahma-console-tier3.test.ts:427` | `toolContext: { …, operatorSurface: true, pendingWardrobeAnnouncements: new Set() }` | Survives. Brahma's wardrobe tools are all `false`, so no outfit change. | `brahma_console_tier3_equivalence.rs` |
| `cases/search-tools-search.test.ts:323` | Executor context with a Set | Survives (the `search` tool only). | `search_tools_equivalence.rs` |
| `cases/tool-execution-process-tier3.test.ts:105`, `text-tool-loop-tier3.test.ts:308`, `native-tool-loop-tier3.test.ts:296`, `carina-query-tier3.test.ts:240` (comment) | v4's REAL `createToolContext(...)` | Survives. The pin's `createToolContext` mints both containers itself (`tool-execution.service.ts:321`). | `tool_execution_process_tier3_equivalence.rs`, `text_tool_loop_tier3_equivalence.rs`, `native_tool_loop_tier3_equivalence.rs`, `carina_query_tier3_equivalence.rs` |
| `cases/wardrobe-wear-stats-tier2.ts:4` | Comment only | n/a | — |
| `fixtures/build-outfit-change-effects-fixture.ts:5`, `build-pascal-wardrobe-effects-fixture.ts:6`, `build-pascal-run-custom-fixture.ts:25` | Comments only (the builders call no effects API) | n/a | — |

**The re-aim, for whichever lane owns it.** Match the pin's own callers,
calling v4's REAL exports and reimplementing nothing.

- `outfit-change-effects-tier3.ts:201` →
  `effects.recordPendingWardrobeAnnouncement(repos, { …, provenance: { source: 'character', actorCharacterId: <character> } , ...(withSet ? { pendingWardrobeAnnouncements: turnSet, pendingWardrobeHistory: turnHistory } : {}) }, step.context ?? '')`.
  - Add `provenance` (and the map) to `:189-197`'s `notify` and the map to
    `:230-233`'s `flush`.
  - A per-turn `newWardrobeTurnContainers()` is exported from
    `outfit-change-effects.ts` (the re-export after `:51`) and from
    `lib/wardrobe/turn-containers.ts:22`.
- `wardrobe-tools.ts:184` →
  `base = { userId, chatId, characterId, provenance: { source: 'character', actorCharacterId: characterId }, ...newWardrobeTurnContainers() }`,
  which mirrors `tool-executor.ts:736-737`.
  - The case dumps `pendingAnnouncements` at `:263`. Use ONE pair of
    containers across ops, as the Set is today.
  - With the map present, the intent WAITS (no settle in the op), so no new
    line appears and the pre-history bytes hold.
- `wardrobe-tools-avatar-trigger.ts:113-118`: the same, for fidelity (no
  output change).

### A.3 Other API shape changes in the span that oracle code calls

| Change (v4 at pin) | Oracle callers | Effect |
|---|---|---|
| **`backup-service.ts:291` `globalRepos.wardrobeHistory.findAll()` — UNGUARDED.** It reads through `rawQuery` (`database/manager.ts:333` → `backends/sqlite/backend.ts:819-844`, which logs ERROR `Raw query failed` and RETHROWS). `chat_wardrobe_history` is created ONLY by `add-chat-wardrobe-history-table-v1` (generateDDL never renders it, the TWELFTH D23 move). No oracle fixture runs that migration: `harness/oracle/lib/v4-migrations.ts` runs narrowed lists, and the full-registry runners are A.3's last row. Compare `wardrobeWear.findAll()` at `:287`: an `AbstractBaseRepository` read on a generateDDL table, which survives. | `createBackup(` at `cases/system-backup.test.ts:488` (in a loop), `cases/backup-uuid-remap.test.ts:210`, and the builders `fixtures/build-restore-archive-orphan-links.test.ts:201`, `build-restore-archive-bug117.test.ts:352`, `build-restore-archives-compact.test.ts:152`, `build-restore-archives-dedupe.test.ts:288,341`, `build-restore-archives.test.ts:171`, `build-restore-archives-memory-graph.test.ts:392` | **REGEN DIES** (`createBackup` rejects `no such table: chat_wardrobe_history`). Families: `system_backup_equivalence.rs`, `backup_uuid_remap_equivalence.rs`, and the committed archives the six builders make for `system_restore_equivalence.rs` / `system_restore_guards_equivalence.rs` / `system_restore_state.rs` / `restore_vintage_state.rs`. The builders only bite if a lane rebuilds an archive. **The precedent idiom is already in the tree:** `system-backup.test.ts:342-349` `plantWardrobeWear()` runs v4's REAL `WARDROBE_WEAR_STATS_DDL` through `rawQuery`. The twin is `CHAT_WARDROBE_HISTORY_DDL` (`lib/database/backends/sqlite/chat-wardrobe-history-ddl.ts:18`, exported at the pin: the table + two UNIQUE indexes). Once the table is planted, v4 emits `data/wardrobe-history.json` `[]` and `manifest.counts.wardrobeHistory: 0` (`backup-service.ts:502,710`). Unported v5 reds on those, so these families go green only with the carriers port. |
| `ndjson-writer.ts` `streamChats` → `wardrobeHistory.findByChatId(id)` (repository `:392`, `rawQuery`) inside `try` | `system-export.test.ts`, `system-import*.test.ts`, `qtap-schema-validate.test.ts`, `build-qtap-import-two-link-blob.ts` (any chat export) | Survives the throw. Per exported chat, a missing table logs ERROR `Raw query failed` + WARN `Failed to load outfit history for export`, and `counts` is bumped only per row, so it is unchanged. None of those cases captures logs. **`character-archive-tier2.test.ts`** exports `type: 'characters'` (`archive-service.ts:492-498`), so it never reaches `streamChats` and is unaffected. |
| `delete-service.ts` `clearFormat3Entities` list gains `'chat_wardrobe_history'`; `DELETE FROM` is inside `try` | `system-restore.test.ts` (`replace` cases), `store-name-reconcile.ts` | Survives. A missing table logs ERROR `Raw query failed` (manager) + DEBUG `Skipping table truncate…`. The restore census whitelists `restore.ts` messages only; verify at regen. |
| `cascade-delete.ts` `wardrobeHistory.deleteByCharacter` (repository `:364-367`, `rawQuery`) inside `try` → ERROR `Failed to drop outfit history for character <id>` | `characters-mutations.test.ts:1181`, `characters-reads.test.ts:466`, `memory-cascade-tier2.ts`, `reset-builtins.test.ts` | Survives. Caught, and none captures logs. |
| `recordPendingWardrobeAnnouncement(repos, …)`, `flushPendingWardrobeAnnouncements(context, repos?)` | A.2 | A.2 |
| `resolveMetadata` (`app/api/v1/custom-tools/route.ts`, module-private) now returns `{ metadata, characterName? }`; the preview body gains `character` | No direct caller. `pascal-workbench-route.test.ts` and `query-param-semantics.test.ts` drive the route | Behaviour drift (the Pascal `{{character}}` lane's), not a load break. |
| `WearablePool.findByTitle` → `matchWearableTitle` | A.1 | Survives |
| `summarizeHistorySlots`, `resolveHistorySlots`, `changedByLabel`, `operatorWardrobeProvenance`, `rollbackWardrobe`, `newWardrobeTurnContainers`, `turnContainersOf`, `remapCharacterKeyedChatFields`, `matchWearableTitle` (all NEW) | 0 | — |
| `postPascalResult({…, announcerCharacterId?})`, `runCustomTool({…, characterName?})`, `renderTemplate` gains `character` | `pascal-writers.ts`, `pascal-run-custom*.ts`, `pascal-simulate.ts`, `pascal-llm-consult.ts` | Additive optionals. Behaviour drift where a definition uses `{{character}}` (the Pascal lane's). |
| The wardrobe tool list gains `wardrobe_rollback` / `wardrobe_history` (`tool-executor.ts` BUILT_IN_TOOLS, `streaming.service.ts` / `pseudo-tool.service.ts` flags, `lib/tools/index.ts`, text-block/simple-json prompts and parsers) | `tool-build.test.ts`, `pseudo-tool-prompts.ts`, `pseudo-tool-parsers.ts`, `tool-dispatch.ts`, and the four `streaming.service` importers | Behaviour drift (the tools lane's): the tool arrays and prompts grow. |
| **The full-registry migration runners** see three NEW migrations (`migrations/scripts/index.ts:448-450,916-919,1362-1365`: `add-chat-wardrobe-history-table-v1`, `-tool-name-v1`, `seed-…-v1`) | `cases/migration-index-backfill.ts:265-266` (items 9/10: "v4's runner on a backfilled v5 copy is a no-op"), `provision/migrations-first.ts:64-65`, `provision/dump-migration-indexes.ts` → `provisioning_equivalence.rs`, `migration_index_backfill_equivalence.rs`, `host_boot_fresh_indexes.rs`, `services/provisioning/migration_indexes.json` | The expected D23-#12 tripwire, owned by the history keystone lane (the boot ensure, per the 2026-10-19 ruling). **Flag:** a re-dump of `migration_indexes.json` at the pin would pick up `idx_chat_wardrobe_history_entry` / `_current` (both UNIQUE, from the DDL). Under the ruling, the table's ensure creates them, so the dumper or the artifact must not carry them. `zero-rerun.ts:137` runs a NARROWED list (`P4D288_SPAN_MIGRATIONS` + `CHAT_SETTINGS_4_11_MIGRATIONS`) and is unaffected unless widened. |

### A.4 Conclusion

**Count at the pin:**

- **0 IMPORT-DEAD names.**
- **1 family garbles silently:** `outfit_change_effects_tier3` (2 `record`
  steps, then the cascade).
- **1 family reds on an extra captured line:** `wardrobe_tools`.
- **2 case families plus 6 committed-archive builders die on regen:** the
  `createBackup` class — `system_backup`, `backup_uuid_remap`, and the
  `system_restore*` / `restore_vintage_state` archive builders.

Everything else in A.2 / A.3 survives, or reds only on drift its own lane
ports.

**A stage-0 harness-only REAIM lane (P4.D314 precedent) — decision rule:**

- **Precedent.** P4.D314's mandate was "make the round's oracle side LOAD at
  the pin" for v4-deleted exports used by **18 families across many lanes**
  (`p4.d314-*.md:114-131`; Tier 1 item 1 is a red-first record of each
  failure shape, `:420-433`).
- **#1 and #2 are not that.** They load fine. Their fix is to call v4's NEW
  signature and supply `provenance` and the history map exactly as the pin's
  callers do. That is the history keystone's port surface, so re-aiming it
  ahead of the v5 port only moves which side is red.
  - **→ The outfit-history keystone lane re-aims both in its own Tier 1**
    (A.2's recipe).
- **#3 is the loadability class.** Planting `CHAT_WARDROBE_HISTORY_DDL`
  makes `createBackup` run. The planted families still red on the carriers'
  `wardrobe-history.json` / `counts.wardrobeHistory` until v5 ports them.
  - **If ONE lane owns both the outfit-history carriers (`7953da3a0` +
    `4ae20977f`'s `actorToolName`) and bug 252 (`e8e17b2d9`, which drives
    `backup_uuid_remap`):** fold the plant into that lane's Tier 1. No
    REAIM lane.
  - **If they are split:** order a one-commit `REAIM:` — the plant at 2 case
    sites, plus the 6 builders only if any lane rebuilds an archive. Both
    lanes stack on it.
  - Either way, the plant runs v4's REAL exported DDL through `rawQuery` (the
    `plantWardrobeWear` idiom at `system-backup.test.ts:342-349`), on the
    per-run copy, never `ensureCollection`.

---

## §B — The two riders ported AHEAD of the pin (each from a lane-unique second pin)

### B1 — Bug 254: progressions built on every turn (`102a23648`)

**The ONE guard change** (`git show 102a23648 -- lib/chat/context-manager.ts`):

- **At the pin `e8e17b2d9`:** `context-manager.ts:2162`
  `if (!isContinueMode) { progressionsLLMContext = await buildProgressionsSection({…}) }`,
  with the comment "Continue mode is skipped deliberately — the model is
  finishing its own sentence…".
- **At `102a23648`:** `:2167`
  `const progressionsLLMContext = await buildProgressionsSection({ character, ...(respondingParticipant ? { loadEvents: loadChatEventsForCadence } : {}), respondingParticipantId: respondingParticipant?.id ?? null, nowMs: Date.now(), timezone: options.timezone })`
  — unconditional.
  - New comment: "Built on every turn, continue mode included. That mode is
    every turn without a new user message — a chained speaker, a nudge, an
    autonomous room — and the design of record wants the report on all of
    them; each progression's own cadence decides whether this turn mentions
    it."
- **The Core whisper keeps its skip:** `102a23648:1898`
  `if (respondingParticipant && !isContinueMode) {`.
- **The `isContinueMode` doc comment** (`:476-483`) is rewritten: "Continue
  mode: any turn built without a new user message — the Salon's Continue, a
  nudge, every chained speaker after the first, and every autonomous-room
  turn. Never a model finishing its own sentence. The Aurora Core whisper
  skips these turns by design; the other trailing sections (recall, mail,
  progressions, inventory, inform) are built on them as on any turn."
- **⚠ The same hunk is also bug 253.** It adds `const inventoryLLMContext =
  (await buildInventorySection({ chat, characterId: character.id, repos:
  getRepositories() })) ?? ''` (`:2185-2186`), its `import` (`:157`), and its
  slot in `trailingContextSections` (`:2241-2249`, between
  `progressionsLLMContext` and `informBlock`).
  - The rider ports ONLY the progressions half. The inventory half is the
    Inventory round's.
  - The comment's word "inventory" in the doc string must not be ported as a
    claim v5 makes before then. Port the sentence with "(recall, mail,
    progressions, inform)" or flag it.

**v5's twin:**

- `crates/quilltap-core/src/services/build_context.rs:2907-2943`. The guard
  is at `:2916` `let progressions_llm_context = if input.is_continue_mode {
  String::new() } else { … build_progressions_section(…) }`, and the stale
  comment is at `:2914-2915`.
- The Core-whisper skip that STAYS: `:2763-2768`
  (`Some(rp) if !input.is_continue_mode =>`).
- `:2969` says "NO continue-mode gate, unlike progressions above" — it must
  change.
- The `is_continue_mode` field (`:559`) has no doc comment. Give it v4's.
- The oracle case's own comment `harness/oracle/cases/build-context-tier3.test.ts:148-150`
  repeats the retired v4 claim ("the model is finishing its own…"). It is a
  binary-flagged file to BSD grep — use `ggrep -a`.

**The op that flips** (not a tier-3 "op" name in Rust — a corpus row):

- `harness/oracle/fixtures/build-context-tier3.json:1564`
  `"name": "progressions_continue_mode"`, with `characterId` `…0a3` (Charlie)
  and `"isContinueMode": true` at `:1580`.
- Its sibling `progressions_normal_turn` is at `:1546`.
- Family: `crates/quilltap-harness/tests/build_context_tier3_equivalence.rs`.
  `SpecOp.is_continue_mode` (`:133-135`) is threaded at `:1036`, and the
  whole `BuiltContext` is compared at `:1247-1255` ("built context
  diverges").
- Only Charlie carries progressions (`fixtures/build-context-tier3-fixture.ts:308-330`).
  The corpus's other two continue-mode rows, `narration_anchor_continue_mode`
  and `mail_letter_continue_mode`, are character `…0a1` with no
  progressions, so they stay byte-identical. **Exactly one row flips.**

**Is a regen from a lane-unique pin at `102a23648` contaminated by
Inventory bytes?** **Clean.**

- `buildInventorySection` (`102a23648:lib/inventory/prompt-section.ts:55-77`)
  first checks `if (equippedIdsOf(chat, characterId).length === 0) return
  null;`. That means no pool read and no log line.
- `equippedIdsOf` (`lib/inventory/equipped-set.ts:24-26`) is
  `normalizeEquippedInventory(chat.equippedInventory)[characterId] ?? []`.
  `normalizeEquippedInventory` (`chats.repository.ts:1010-1026`) returns `{}`
  for NULL / undefined / a non-object / unparseable text, and `{}` for `{}`.
  A character with no entry gets `[]`.
- The repo's `getEquippedInventory` (`chats.repository.ts:781-791`) returns
  `{}` on NULL (it is not called on this path).
- `null ?? ''` gives `''`, and `trailingContextSections` drops falsy entries
  (`.filter((section): section is string => !!section)`). An Inventory-less
  turn is therefore byte-identical, as the module doc promises ("Empty is
  absent").
- The fixture is rebuilt at the regen pin by v4's `initializeDatabase` /
  `generateDDL` (`build-context-tier3-fixture.ts:186-246`), so its `chats`
  gains `equippedInventory` (NULL). v4 does not auto-ALTER:
  `generateAlterStatements` (`schema-translator.ts:528`) has no runtime
  caller. v5 reads `chats` by an explicit 103-column list
  (`crates/quilltap-core/src/db/chats_read.rs:65,443,504`), so the extra
  column is invisible. Nothing plants `equippedInventory`.
- **Residual risk (prove it, do not argue it):** `e8e17b2d9..102a23648` also
  rewrites `lib/wardrobe/folders.ts` (255 lines) and
  `vault-overlay/parsers.ts` (`resolveWardrobeItemId` →
  `isItemDocumentPath(WARDROBE_FOLDER_RULES, …)`). buildContext's wardrobe
  live-clothing resolution reads through them.
  - Proof: regen the family at `e8e17b2d9` AND at `102a23648` with the
    unchanged corpus. The NDJSONs must differ in exactly the
    `progressions_continue_mode` row.
  - That A/B is also the red-first record.
  - No `package.json` dependency moves `e8e17b2d9..102a23648` (version stamp
    only: `4.11.0-dev.26` → `.36`).

### B2 — The `Inventory/` archive keep (`4fbdad186`)

**The hunk** (`git show 4fbdad186 -- lib/characters/archive-service.ts`,
+19 / −13), transcribed:

- `:59` import:
  `import { CHARACTER_INVENTORY_FOLDER, CHARACTER_WARDROBE_FOLDER } from '@/lib/mount-index/character-vault';`
  (was `CHARACTER_WARDROBE_FOLDER` alone).
- **NEW `:129`:**
  `const KEPT_FOLDERS: readonly string[] = [CHARACTER_WARDROBE_FOLDER, CHARACTER_INVENTORY_FOLDER];`
  with the doc comment "Vault folders kept whole, with everything beneath
  them."
- The keep-set doc comment (above `:131` `KEPT_MANAGED_PATHS`) now reads
  "…plus everything under `Wardrobe/` and `Inventory/` (checked by prefix,
  not listed here — an archived character's Inventory instances must stay
  countable as an archetype's dependents)." `KEPT_MANAGED_PATHS` itself is
  UNCHANGED.
- `pruneVault` (`:804`):
  - `:824`
    `const keptPrefixes = KEPT_FOLDERS.map((folder) => \`${folder.toLowerCase()}/\`);`
    (was `const wardrobePrefix = …`).
  - `:828`
    `return KEPT_MANAGED_PATHS.has(path) || keptPrefixes.some((prefix) => path.startsWith(prefix));`
  - `:855` `await pruneEmptyFolders(mountPointId, survivors, keptPrefixes);`
- `pruneEmptyFolders` (`:879`):
  - The parameter is now `keptPrefixes: readonly string[]`.
  - The `wardrobeFolder` local is gone.
  - `:891-893` is the skip:
    `folderPath === '' || keptPrefixes.some((prefix) => folderPath === prefix.slice(0, -1) || folderPath.startsWith(prefix))`.
  - The doc says both folders "survive even when empty".
- **`CHARACTER_INVENTORY_FOLDER`** is
  `export { INVENTORY_FOLDER as CHARACTER_INVENTORY_FOLDER } from '@/lib/inventory/folders'`
  (`character-vault.ts:34`; `INVENTORY_FOLDER = 'Inventory'`,
  `lib/inventory/folders.ts:45`).
  - That is the ONLY Inventory module the archive service reaches: a
    string constant, through a re-export.
  - The change is purely the keep-set / prefix rule. No other Inventory
    import or call is involved.
- **`ARCHETYPE_IN_USE` on archive:** none. The archive service neither
  refuses nor warns for archetypes referenced by others' instances. The
  keep's stated purpose is the reverse direction: archived characters'
  instances stay COUNTABLE as dependents.
  - The refusal lives on the archetype side:
    `InventoryArchetypeInUseError` (`lib/inventory/errors.ts`,
    `'ARCHETYPE_IN_USE'`, 409, message
    `` `${dependents} ${dependents === 1 ? 'object derives' : 'objects derive'} from it` ``)
    and `InventoryArchivedDependentsError` (`'ARCHIVED_DEPENDENTS'`, 409,
    "Instances live in archived characters' vaults, which cannot be
    changed: …"), raised by forced delete / detach-all.
  - `dependents.ts:113` marks `ownerArchived: !!character.archivedAt` and
    logs `archivedHolders`.
  - All of that is the Inventory round's. The rider carries none of it.

**v5's twin** (`crates/quilltap-core/src/services/character_archive/service.rs`):

- `KEPT_MANAGED_PATHS: [&str; 10]` `:264-275` (unchanged by the rider). Its
  doc at `:253-263` says "plus everything under `Wardrobe/`" — this must
  gain `Inventory/`.
- `CHARACTER_WARDROBE_FOLDER` `:277-278`. Add `CHARACTER_INVENTORY_FOLDER`
  (`"Inventory"`) and a `KEPT_FOLDERS` slice.
- `prune_vault`'s prefix rule: `:1365` builds `wardrobe_prefix`, and `:1372`
  is `managed.contains(&path) || path.starts_with(&wardrobe_prefix)`.
- `prune_empty_folders(mount, mount_point_id, survivors, wardrobe_prefix: &str)`
  `:1446-1480`; the skip is at `:1469-1474`. Its signature becomes a prefix
  slice.
- The call `:1416` and the unit test `:1581`
  (`prune_empty_folders(mount, &mp, &[], "Wardrobe/")`, in
  `p4149_prune_folders_tests`) follow.
- The doc at `:1306-1310` / `:1446-1449` needs `Inventory/`.

**The harness family driving v4's REAL archive service:**

- `crates/quilltap-harness/tests/character_archive_tier2_equivalence.rs`,
  oracle `harness/oracle/cases/character-archive-tier2.test.ts`, corpus
  `harness/oracle/fixtures/character-archive.json` (keys `sable`, `tor`,
  `quill`, `chatSeated`, `chatRemoved`, `memories`; **zero `Inventory`
  occurrences**).
- The fixture is COMMITTED and extended by mutation
  (`crates/quilltap-web/tests/fixtures/character-archive-{main,mount}.db`;
  `build-character-archive-fixture.ts`, then
  `extend-character-archive-twice-linked-blob.ts`, then
  `extend-character-archive-profile-and-avatars.ts` — the header at `:34-41`
  says "extend by mutation, never rebuild").
- It compares the result, the DECRYPTED bundle, `digestProbe`, and a
  whole-table dump of BOTH partitions.
- Related readers, not this rider's: `archive_reencrypt_tier2_equivalence.rs`,
  `characters_mutations_equivalence.rs`, `memory_retire_tier2_equivalence.rs`.

**Is a regen from a lane-unique pin at `4fbdad186` clean?**
**Clean in expectation, and the corpus does not yet exercise the hunk.**

- `4fbdad186 --stat` over the archive's transitive imports:
  - `lib/export/` and `lib/import/` are untouched (the archive's
    `ndjson-writer` / `quilltap-import-stream` / `execute` are the pin's).
  - `lib/characters/archive-service.ts` is the hunk.
  - `lib/mount-index/character-vault.ts` is +3 (the re-export).
  - `lib/database/repositories/chats.repository.ts` is +93 (the
    `equippedInventory` reader/writer, patch-only).
  - `doc-mount-documents.repository.ts` is +44 (an additive
    `findManyByMountPointsInFolder`).
  - The vault-overlay refactor: `parsers.ts` +14 / −…, `vault-projection.ts`
    +9 (`ext` option, default `.md`), `wardrobe-writes.ts` −386 into the new
    `item-folder-ops.ts`, `inventory-readers.ts` / `-writes.ts` new.
  - Also `lib/file-storage/wardrobe-image-bridge.ts`, and
    `lib/backup/restore/{restore,uuid-remap}.ts` (backup, not archive).
- The committed fixture has no `equippedInventory` column, and v4 never
  auto-ALTERs (B1's note), so chat reads carry no new key.
- The character export is `type: 'characters'` (`archive-service.ts:492-498`),
  so the outfit-history `streamChats` addition is also out of this family's
  path.
- **The real risk** is the wardrobe-writes / parsers refactor on the
  rehydrate path. Prove it by A/B: regen at `e8e17b2d9` and at `4fbdad186`
  with the unchanged corpus. They must be byte-identical modulo the minted
  / blinded fields.
- **Then** add the case that exercises the hunk. Do it as an
  extend-by-mutation recipe planting an `Inventory/` folder plus a link
  under one archived character's vault (raw mount-index rows, or v4's
  `writeDatabaseDocument` at the pin — never an Inventory writer). At
  `e8e17b2d9` it is pruned; at `4fbdad186` it is kept. That pair is the
  red-first record.
- An empty `Inventory/` folder row with no link exercises the "survives even
  when empty" arm.

---

## §C — The vendor set at the pin

`git diff --stat 25b8fab6a e8e17b2d9 -- help/ docs/ public/schemas/ README.md
.claude/ .github/ docker/ package.json packages/quilltap/package.json
CLAUDE.md`: **24 files, +1,858 / −22.** `.github/` and `docker/` do not move.

### C.1 `help/`

**131 files at the pin, 131 at the baseline**
(`git ls-tree -r --name-only <sha> -- help | wc -l`), with the same name set.
Four pages move (+60 / −10):

| Page | +/− | By commit | New headings |
|---|---|---|---|
| `help/character-gallery.md` | +13 / −0 | `1cfc9b504` | `## Wardrobe Pictures — likenesses of every garment within reach` |
| `help/custom-tools.md` | +12 / −3 | `5cb0e0190` (1/1), `a05c6f755` (11/2) | `## Letting the character say it — \`announceAsCharacter\`` |
| `help/pascals-workbench.md` | +5 / −3 | `5cb0e0190` (3/2), `a05c6f755` (2/1) | — |
| `help/wardrobe.md` | +30 / −4 | `5cb0e0190` (1/0), `f46cf40a2` (29/4), `4ae20977f` (1/1) | `## Looking Back --- the Outfit Record` |

No `url:` / `title:` front matter moves.

- **v5's vendored copy:** the top-level `help/` (131 files). All four pages
  are byte-equal to `git show 25b8fab6a:help/<p>` today (`cmp`).
- **Guards** (the P4.D313 set):
  - `crates/quilltap-harness/tests/help_tree_embed_guard.rs` (the md5 table
    vs the on-disk tree, `embedded_table_equals_the_on_disk_help_tree` `:113`;
    count 131).
  - `crates/quilltap-host/tests/host_help_docs_boot.rs` (131 rows, `:109`).
  - `help_tree_equivalence.rs` and `help_section_size_equivalence.rs`
    (regenerate from the pin — three new H2 sections).
- **Rule (P4.D313 R-1):** every byte comes from
  `git show e8e17b2d9:help/<p>`, never the working tree. The live tree is at
  133 pages and carries Inventory edits to `custom-tools.md`,
  `pascals-workbench.md` and `wardrobe.md`.

### C.2 `docs/` → v5 `docs/v4/`

v4 `docs/` is 490 files at the baseline and 495 at the pin. v5 `docs/v4/` is
491 tracked; it includes the CLI mirror `packages-quilltap-README.md`. All 8
M paths below are byte-equal to the baseline today (`cmp` against
`git show 25b8fab6a:docs/<p>`).

| v4 path (status) | +/− | Commits | v5 twin | Action |
|---|---|---|---|---|
| `docs/CHANGELOG.md` (M) | +65 / −0 | all 14 | `docs/v4/CHANGELOG.md` | Overwrite at the pin. It CARRIES the Inventory-spec lines of `54bc17a3c` / `f5db10034`; vendor it byte-exact anyway. |
| `docs/FEATURES.md` (M) | +1 / −1 | `5cb0e0190` | `docs/v4/FEATURES.md` | overwrite |
| `docs/developer/API.md` (M) | +63 / −1 | `8d8618819`, `4ae20977f`, `1cfc9b504` | `docs/v4/developer/API.md` | overwrite |
| `docs/developer/BACKGROUND_JOBS_CHILD.md` (M) | +2 / −2 | `f46cf40a2` (the `settle` buffered-write override; the read-your-writes paragraph) | `docs/v4/developer/BACKGROUND_JOBS_CHILD.md` | Overwrite. The code half (`child-repositories-proxy.ts` +9) is the history keystone's. |
| `docs/developer/DDL.md` (M) | +49 / −0 | `83387d0e5`, `4ae20977f` | `docs/v4/developer/DDL.md` | overwrite |
| `docs/developer/bugs.md` (M) | +1 / −0 | `e8e17b2d9` (row 252) | `docs/v4/developer/bugs.md` | overwrite |
| `docs/developer/bugs/fixed/bug-252-restored-outfit-keys-stale.md` (A) | +99 | `e8e17b2d9` | `docs/v4/developer/bugs/fixed/bug-252-restored-outfit-keys-stale.md` (absent) | add |
| `docs/developer/features/ROADMAP.md` (M) | +7 / −0 | `5cb0e0190`, `fcab3f112`, `f46cf40a2`, `54bc17a3c`, `f5db10034` | `docs/v4/developer/features/ROADMAP.md` | Overwrite (it carries Inventory roadmap lines; byte-exact). |
| `docs/developer/features/complete/pascal-custom-tools.md` (M) | +2 / −0 | `a05c6f755` | same under `docs/v4/` | overwrite |
| `docs/developer/features/complete/pascal-wardrobe-effect-picker.md` (A) | +248 | `5cb0e0190` | absent | add |
| `docs/developer/features/complete/wardrobe-outfit-history.md` (A; born `fcab3f112` at `features/`, R092 to `complete/` in `f46cf40a2`, edited by `83387d0e5` / `05005c562` / `4ae20977f`) | +737 | — | absent | add at the FINAL `complete/` path |
| `docs/developer/features/inventory.md` (A) | +280 | `54bc17a3c`, `f5db10034` | absent | **DEFER.** It moves to `features/complete/inventory.md` in `4fbdad186` (R088), past the pin. Vendor it in the Inventory round at its final path. |
| `docs/developer/features/inventory-tools-and-pascal.md` (A) | +196 | `f5db10034` | absent | **DEFER.** It moves to `features/complete/` in `e1872c94f` (R081), and both are edited again in `102a23648`. Inventory round. |

After this round: 491 + 3 = **494 tracked** under `docs/v4/` (re-measure
with `git ls-files docs/v4 | wc -l`). With
the two deferred, the mirror is NOT a full `docs/` copy of the pin: record
the two paths as deliberate deferrals, not drift.

Not vendored, all NO-PORT infra:

- `README.md` badge line 11 (`version-4.11.0--dev.10` → `--dev.26`, the
  sum of eleven commits' bumps).
- `CLAUDE.md` +1 (`f46cf40a2`: the "Outfit history is written only by
  `wardrobeHistory.settle`…" chokepoint bullet). It is v4's own CLAUDE.md;
  the rule's substance lands as the history lane's code + comments.
- `.claude/commands/update-documentation.md` +3 / −1 (`5cb0e0190`,
  `fcab3f112`, `f46cf40a2`).

### C.3 `public/schemas/` (byte-guard only; the CONTENT is owned elsewhere)

| v4 file | Baseline (bytes, md5) | Pin (bytes, md5) | Moved by | Owner | v5 copy + guards |
|---|---|---|---|---|---|
| `public/schemas/qtap-custom-tool.schema.json` | 43,493, `0f0f2f49e30f4c284538204048e0bec2` | **44,021, `cc205f92e1a7cbf9a485b49e31e6d413`** | `a05c6f755` (`announceAsCharacter` boolean; `{{character}}` added to three placeholder descriptions) | The Pascal `{{character}}` / announce lane | `apps/web/public/schemas/qtap-custom-tool.schema.json` (md5 = the baseline's today); `crates/quilltap-harness/tests/public_schemas_vendor_guard.rs:53` `bytes: 43_493` → `44_021` |
| `public/schemas/qtap-export.schema.json` | 105,774, `910063dd069bd581cff3e22d531976cc` | **108,678, `1d8e6546fbdb6ad3a48fee15d2932911`** | `7953da3a0` (the `chats.wardrobeHistory` array, `counts.wardrobeHistory`, NEW `$defs.WardrobeHistory`), `4ae20977f` (`actorToolName`) | The outfit-history carriers lane (the `.qtap` validator would refuse a bundle with `wardrobe_history` records until it lands — the live-data class P4.D313 closed for `invokedBy: "trigger"`) | `crates/quilltap-core/src/generators/qtap-export.schema.json` (md5 = the baseline's today); `crates/quilltap-core/src/generators/qtap_schema.rs:197` `105_774` → `108_678`; `crates/quilltap-harness/tests/qtap_schema_embed_guard.rs:76` `VENDORED_BYTES` → `108_678`; `qtap_schema_validate_equivalence.rs` rows for a `wardrobe_history` record (red-first) |

**Recommendation:** the vendor lane only re-checks the guards. The owning
lanes copy each file whole from `git show e8e17b2d9:` and move the
constants. If the planner keeps P4.D313's shape (the vendor lane copies,
the owners consume), the guards and the copy must land in ONE commit with
the constants.

### C.4 NO-PORT candidates in the span (with the evidence each ratification needs)

- **`fcab3f112`** (docs(wardrobe): spec). `git show --stat` is 4 files, all
  docs/infra: `.claude/commands/update-documentation.md` +1, `CHANGELOG.md`
  +4, `ROADMAP.md` +1, `docs/developer/features/wardrobe-outfit-history.md`
  +663. **Zero `lib/ app/ components/ migrations/` files.** The spec reaches
  v5 at its final path through C.2's `complete/` row.
- **`54bc17a3c`** (Inventory spec). 3 files: `CHANGELOG.md` +4,
  `ROADMAP.md` +4, `features/inventory.md` +280. Docs-only. Its spec is
  DEFERRED to the Inventory round (C.2); its CHANGELOG / ROADMAP lines ride
  this round's byte-exact copies.
- **`f5db10034`** (Inventory tools spec). 4 files: `CHANGELOG.md` +3 / −1,
  `ROADMAP.md` +3 / −1, `features/inventory-tools-and-pascal.md` +196,
  `features/inventory.md` +2 / −1. Docs-only, deferred as above.
- **The docs/infra-only halves of the code commits:**
  - The README badge (`5cb0e0190`, `a05c6f755`, `83387d0e5`, `05005c562`,
    `ebb480edd`, `8d8618819`, `7953da3a0`, `f46cf40a2`, `4ae20977f`,
    `1cfc9b504`, `e8e17b2d9`; the net diff is the one line, `README.md:11`).
  - `docs/CHANGELOG.md` (all).
  - `ROADMAP.md` (`5cb0e0190`, `f46cf40a2`).
  - `docs/FEATURES.md` (`5cb0e0190`; +1 / −1 in the Pascal paragraph,
    `:110`).
  - `.claude/commands/update-documentation.md` (`5cb0e0190`, `f46cf40a2`).
  - v4's `CLAUDE.md` (`f46cf40a2`, +1).
  - `BACKGROUND_JOBS_CHILD.md` (`f46cf40a2`; +2 / −2 at `:66` and `:181`,
    documenting `settle` among the write overrides — vendored as docs; the
    behaviour is the keystone's).
  - `packages/theme-storybook/{package.json,package-lock.json}` (`1cfc9b504`,
    1.0.73 → 1.0.74) and `packages/theme-storybook/src/css/qt-components.css`
    +22 (the storybook copy of `_content.css`). The ledger records
    theme-storybook as NO-PORT by precedent (`drift-ledger.md:1522`).
  - `lib/tools/__tests__/__snapshots__/*.snap` and every `__tests__/**`
    file (v4's own tests).

### C.5 The dependency check

`git diff 25b8fab6a e8e17b2d9 -- package.json package-lock.json
packages/quilltap/package.json 'plugins/*/package.json'
packages/theme-storybook/package*.json` contains **version stamps only**:

- `package.json` `"version"`: `4.11.0-dev.10` → **`4.11.0-dev.26`**.
- `package-lock.json` root + `packages[""]`: `4.11.0-dev.10` →
  **`4.11.0-dev.21`**. The lockfile stamp LAGS `package.json`; grep
  `package.json`, not the lock, for the pin's version.
- `packages/quilltap/package.json`: `4.11.0-dev.10` → `4.11.0-dev.26`.
- `packages/theme-storybook/package.json` and its lock: `1.0.73` → `1.0.74`.
- `plugins/*/package.json`: no change.
- **No dependency, devDependency or resolved-package line moves.** The pin
  shares the baseline's tree (`openai` 7.34.0, `zod` 4.6.7, plugin-utils
  2.7.1), so `provider_sdk_version_guard` / `zod_version_guard` stay put.

---

## §D — Lane-start preflight facts (every order)

- **Pin sha:**
  `git -C ~/source/quilltap-server rev-parse e8e17b2d9` =
  **`e8e17b2d92095bda008f03bb963eb4b1c470e680`**.
  - On `main` / `origin/main` (`git branch -a --contains`).
  - Committed 2026-10-18 14:21:39 −0500.
- **Version stamp:** `git show e8e17b2d9:package.json | grep '"version"'` gives
  **`"version": "4.11.0-dev.26",`**. The lockfile says `4.11.0-dev.21`; do not
  use it as the stamp check.
- **Live checkout:** HEAD `a6292f1692c3e40b2e670b0ddcf1f0c4404d0b6b`, clean.
  It is 6 commits past the pin: `4fbdad186`, `e1872c94f`, `102a23648`,
  `1d0a31132`, `cad017599`, `a6292f169`.
- **The second pins:**
  - B1: `102a236481c6cec2869d02b74d7eea60fe49dfaf`
    (`package.json` `4.11.0-dev.36`).
  - B2: `4fbdad1864583b67805468e4122cd96fba51a486` (`4.11.0-dev.33`).
  - Each lives at its own lane-unique path, e.g.
    `/tmp/qt-v4-pin-<order>-102a23648`, beside the round pin's.
- **The three symlink classes** (`drift-ledger.md` §5.1, `:466-523`), all
  untracked and absent from a bare worktree:
  1. `ln -sfn ~/source/quilltap-server/node_modules "$PIN/node_modules"`
  2. `ln -sfn ~/source/quilltap-server/packages/quilltap/node_modules "$PIN/packages/quilltap/node_modules"`
  3. `for d in ~/source/quilltap-server/plugins/dist/*/; do [ -d "$d/node_modules" ] && ln -sfn "$d/node_modules" "$PIN/plugins/dist/$(basename "$d")/node_modules"; done`

  Then `cd "$PIN"` for EVERY tsx/jest oracle and builder. Clean up with
  `git -C ~/source/quilltap-server worktree remove --force "$PIN"`.
- **Does the live `node_modules` match the pin's lockfile?** Yes, as a
  strict superset. `npm ci --offline` in the pin is NOT required, though it
  stays the safe default.
  - **Lockfile diff pin → live HEAD:** `package-lock.json` +14 / −4. That is
    the version stamps plus ONE added package,
    `node_modules/@xterm/headless` 6.0.0 (`a6292f169`, root
    `"@xterm/headless": "^6.0.0"`).
  - `packages/quilltap/package.json` moves only its stamp (`dev.26` →
    `dev.38`). No `plugins/*/package*.json` moves.
  - **Measured against the INSTALLED tree:** comparing
    `node_modules/.package-lock.json` (stamped `4.11.0-dev.38`) with
    `git show e8e17b2d9:package-lock.json`'s `packages` gives:
    - 0 version differences;
    - 1 extra installed package (`@xterm/headless`);
    - 131 absent from the install (all optional platform binaries:
      `@esbuild/*`, `@emnapi/*`, …, normal for a darwin-arm64 install).
  - So the symlinked tree resolves every pin import to the pin's own
    versions. The one extra package is imported by no code at the pin.
  - **Recommend:** keep `npm ci --offline --no-audit --no-fund` in the pin as
    the safe default (the 2026-10-07 dependency-trap rule), because the
    human may `npm install` past the Inventory mid-round. Record the
    zero-diff measurement as the reason the symlink is acceptable if a lane
    skips it, and re-measure before trusting it.
  - The B1 / B2 second pins carry the same tree (C.5: no dependency moves
    `e8e17b2d9..102a23648`).
- **`provider_sdk_version_guard`** is the tripwire if the live tree ever
  moves an SDK.
- **The §2 freshness probe** must still be re-run at lane start: v4 HEAD is
  past the pin by design, so it reads PIN REQUIRED, never a STOP.
