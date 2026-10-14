# Survey — MEMORY MAINTENANCE (server side): the one `MEMORY_MAINTENANCE` job across `4a38222e6` → `64743ee13`

Read-only planning survey, **2026-10-13**. v4 = `~/source/quilltap-server`
read ONLY through `git show 64743ee13:<path>` (the PIN, `4.10.0-dev.175`) and
`git diff 4a38222e6 64743ee13 -- <paths>` / `git show <sha> -- <paths>` (the
span). v5 = `main` at **`a7c6f0d03`**. The oracle baseline (what v5 ported)
is **`4a38222e6`**; for every module in this slice it is byte-identical to
`01a83539d` (where P4.D266 / P4.D271 ported it — `git diff --stat 01a83539d
4a38222e6 -- lib/memory lib/background-jobs/handlers/memory-{housekeeping,consolidation}.ts
lib/background-jobs/scheduled-housekeeping.ts lib/tools/memory-dedup.ts` is
two unrelated `queue-service.ts` lines), so **`(pre)` = `4a38222e6` = the
shape v5 carries**. Every other v4 line number is AT THE PIN. Nothing here
comes from commit prose (ledger §5.3); the ledger's §1 (17–553) and its §3
rows `104e986c7` (`:608`), `b045dd986` (`:609`), `48790fb73` (`:610`),
`64743ee13` (`:612`), `4ae0c4ca6` (`:607`) were read first and are built on,
not restated — corrections are called out as **⚑ LEDGER CORRECTION**.

**Scope:** `lib/memory/maintenance/*` whole; the `MEMORY_MAINTENANCE` handler,
its queue helpers, the dispatcher's post-commit hooks, job type / activity /
topic tables; `lib/memory/consolidation.ts` as the `consolidate` phase calls
it (the loader moved out, bug 246, `tfidfClusterThreshold`) and
`cheap-llm-tasks/consolidation-tasks.ts`'s span hunks; the Memories API's
`maintain` / `maintenance-status` / `repair` / `maintenance-config` /
`consolidation-config` / `extraction-mode-config` actions; `lib/memory/repair.ts`;
`instance_settings.memoryMaintenance` + `memoryConsolidation` as maintenance
reads them; `patchSchemaOf` (bug 244); the `drop-housekeeping-consolidation-jobs-v1`
job-row migration; what v4 deleted and its v5 twins. **Out of scope (NEEDS
only, §7):** admission (`foldInto` / `applyFold` / `withAdmissionLock` /
`occasionsAreDistinct`, Phase C), `retireMemories` + the deletion chokepoint
(Phase B), the memory index (`memoryIndex.setTier` / `upsert` /
`indexInserted`, Phase B), `protection()` scoring (Phase A), the memory clock,
the schema keystone's D23 re-dump + the settings move migration
(`move-memory-maintenance-settings-v1`), extraction (Phases D / bug 245), the
SPA / CLI / `help/` (pointers only, §8).

---

## 0. The span for this slice, in one paragraph

Eight of the nineteen commits touch maintenance, but **two own it**:
`104e986c7` (Phase F) replaced `MEMORY_HOUSEKEEPING` + `MEMORY_CONSOLIDATION`
with ONE `MEMORY_MAINTENANCE` job per character that runs five ordered,
individually skippable phases over ONE corpus read — fold-near-duplicates →
consolidate → demote → retire → mirror (`lib/memory/maintenance/`, 9 files,
1,500 lines) — answered every trigger through ONE parent-side ledger
(`shouldEnqueue`), owned the daily sweep in ONE scheduler, deleted the
housekeeping service, its outcome cache, the consolidation triggers, the
scheduled-housekeeping driver, both handlers and the System-Tools dedup
actions, moved consolidation's loader out of `consolidation.ts` (which keeps
the planner + model route + plan executor), dropped the three never-persisted
retention knobs, added `memoryMaintenance.phases`, `?action=maintain` /
`maintenance-status`, and the job-row migration; `b045dd986` (Phase G)
retired the `housekeep` / `housekeep-sweep` / `consolidate` /
`housekeeping-config` actions (`maintain` covers them; `withImpliedPhases`),
added `?action=repair` (`lib/memory/repair.ts`), and fixed **bug 244**
(`patchSchemaOf`), which flips v5's ruled `PARTIAL_DEFAULTS_CLOBBER` table.
Around them: `c2795733f` (bugs 203–226) supplies the per-character dedupe (bug
221), the demotable-only cap (bug 210), demoted rows re-entering consolidation
(bug 211), the mirror-after-commit (bug 208), the threshold override (bug 223)
and `supersededAt` (bug 225) that the phases encode; `e0ac7493a` (Phase A)
moved the knobs to `instance_settings.memoryMaintenance`; `af48f6cc2` (Phase B)
made `retireMemories` the only deleter; `8af764514` (Phase C) gave the fold its
`foldInto` / `applyFold` / admission lock and created the short-lived
`admission/after-insert.ts`; `4ae0c4ca6` added
`memoryConsolidation.tfidfClusterThreshold` (0.30) and per-provider fold
thresholds; `48790fb73` + `9ec4fa2e9` changed only SPA / CLI labels on Phase
G surfaces; `64743ee13` (bug 246) gave each consolidation call
`max(4000, resolveMaxTokens(routed profile))` and an "empty answer" verdict.

---

## 1. v4 AT THE PIN — every module in the slice

### 1.1 `lib/memory/maintenance/types.ts` (184 lines) — vocabulary, client-safe

- `:15` `export const MAINTENANCE_PHASES = ['fold-near-duplicates', 'consolidate', 'demote', 'retire', 'mirror'] as const` — the ONE fixed order; `:17` `MaintenancePhase`.
- `:24-28` `withImpliedPhases(phases)` — `asked.has('consolidate') ⇒ asked.add('mirror')`; returns `MAINTENANCE_PHASES.filter(asked.has)` (canonical order, deduped). **Called ONLY by the API** (`route.ts:746`); `runMaintenance` does NOT apply it (a request naming `['consolidate']` alone skips `mirror` as `not-requested`).
- `:31` `MaintenanceReason = 'scheduled' | 'watermark' | 'manual'`.
- `:33-49` `MaintenanceRequest { characterId; phases?; reason; exemptPhases?; dryRun?; settings?: MaintenanceRunSettings }`.
- `:52-59` `MaintenanceRunSettings { consolidation?: Partial<MemoryConsolidationSettings>; maxClustersPerRun?: number; cap?: number }`.
- `:62-69` `MaintenanceContext { userId?; timeBudgetMs?; now?: Date }`.
- `:71` `PhaseStatus = 'ran' | 'skipped'`; `:74-80` `PhaseSkipReason = 'not-requested' | 'maintenance-disabled' | 'consolidation-disabled' | 'phase-disabled' | 'no-retention-window' | 'nothing-touched'`.
- `:82-87` `PhaseOutcome { phase; status; skippedBecause?; durationMs }`.
- `:90-95` `NearDuplicateGroup { survivorId; absorbedIds: string[]; minCosine }`.
- `:97-109` `FoldPhaseReport`, `:111-125` `DemotePhaseReport`, `:127-131` `RetirePhaseReport`, `:133-139` `MirrorPhaseReport`, `:141-158` `MaintenanceReport` (`skippedReason?: 'character-not-found' | 'archived'`), `:164-184` `MaintenanceSummary` (the row-free report the job hands back and the ledger keeps).
- **Wire key orders** (JSON.stringify follows literal construction, NOT the interface): see §1.4 for each object as `run.ts` / `demote.ts` build it.

### 1.2 `lib/memory/maintenance/index.ts` (22) — the barrel

`export *` types; `loadCorpus, MaintenanceCorpus, CORPUS_PAGE_SIZE`;
`planNearDuplicateFolds, applyNearDuplicateFolds, isFoldEligible`;
`bucketCandidates, CONSOLIDATION_MAX_BUCKET_ROWS`; `planDemotion,
planRetention, isDemotable`; `runMaintenance, resolvePhases,
summarizeMaintenance, announceMaintenance`; `shouldEnqueue,
recordMaintenanceOutcome, getMaintenanceLedgerStatus,
resetMaintenanceLedgerForTests, type LedgerStatus`; `maybeEnqueueMaintenance,
maybeEnqueueMaintenanceAfterCommit, characterIdsFromMemoryWrites`;
`scheduleMemoryMaintenance, stopMemoryMaintenanceScheduler,
runScheduledMemoryMaintenance`. (`exemptPhasesFor`, `FOLD_BUDGET_SHARE`,
`applyFoldsToCorpus`, `foldNearDuplicateGroups`, `runConsolidatePhase`,
`applyConsolidationToCorpus`, `takeWatermarkCheck`, `noteEnqueued`,
`runStartupTick`, the ledger constants are exported from their files only.)

### 1.3 `lib/memory/maintenance/corpus.ts` (119) — the ONE corpus read

- `:20` `module: 'memory:maintenance:corpus'`; `:23` `CORPUS_PAGE_SIZE = 250`.
- `:40-42` `tierOf(m) = m.tier === 'cold' ? 'cold' : 'hot'` (NULL tier ⇒ hot).
- `:48-61` `loadCorpus(characterId, {tiers?, sources?, pageSize?})` — async generator over `repos.memories.findByCharacterIdInBatches(characterId, pageSize)`; per-page filter (`source ?? 'AUTO'`); `setImmediate` yield between pages (Node scheduling — no DB effect; v5's `housekeeping.rs` header already records this NO-PORT).
- `:64-119` `class MaintenanceCorpus` — a `Map<id, Memory>` in **read order** (v5: `ORDER BY id ASC`, `memories_read::find_by_character_id_in_batches`). `static load` — DEBUG `'Maintenance corpus loaded' {characterId, rows}`; `all()`, `hot()`, `get`, `put` (replace keeps position / new id appends), `patch(id, partial)` (no-op for a gone id), `remove(ids)` — deletes, then strips every removed id from every survivor's `relatedMemoryIds` (mirrors `retireMemories`' scrub). **Order is a comparand** (fold buckets, demotion ties, consolidation buckets all iterate it): a Rust port must use an insertion-ordered map and `shift_remove`, never `swap_remove` (memory note "swap-remove reorders").

### 1.4 `lib/memory/maintenance/run.ts` (385) — `runMaintenance`

- `:52` `module: 'memory:maintenance'`; `:55` `FOLD_BUDGET_SHARE = 0.4`; `:57-59` `isJobChild()` = `QUILLTAP_JOB_CHILD === '1'`.
- `:62-68` `exemptPhasesFor(request)` — `request.exemptPhases` if present; else `reason !== 'manual'` ⇒ ∅; else the named phases (all five when none / empty).
- `:77-104` **`resolvePhases(request, settings)`** → `Map<phase, skipReason | null>`, per phase in order:
  1. explicit list (non-empty) lacking the phase ⇒ `not-requested`;
  2. `consolidate`: not exempt AND `!maintenance.consolidation.enabled` ⇒ `consolidation-disabled` (the maintenance master switch and the toggles do NOT gate consolidate);
  3. `mirror`: never gated by settings (only `not-requested`, later `nothing-touched`);
  4. `fold-near-duplicates` / `demote` / `retire`: not exempt ⇒ `!maintenance.enabled` → `maintenance-disabled`, else toggle off (`phases.foldNearDuplicates` / `.demote` / `.retire`) → `phase-disabled`;
  5. then (exempt or not) `retire` with `!(coldRetentionDays > 0)` ⇒ `no-retention-window`.
- `:107-152` `summarizeMaintenance(report)` — **key order**: `characterId, reason, dryRun, finishedAt, durationMs, [skippedReason], phases, [fold {eligible, groups (=count), folded, groupsFailed, groupsDeferred}], [consolidation {digestsCreated, digestsUpdated, membersSuperseded, clustersFailed, clustersDeferred, [skippedReason]}], [demote = report.demote minus demotedIds], [retire {retentionDays, retired}], [mirror {subjects (=count), filesWritten, deferredToCommit}]`.
- `:155-163` `changedRows(report)` — `fold.folded > 0 || (digestsCreated + digestsUpdated + membersSuperseded + rowsMarkedConsidered) > 0 || demote.demoted > 0 || retire.retired > 0`.
- `:171-186` **`announceMaintenance(summary, {rowsChanged, mirrored})`** — returns at once in the child or for a dry run; else `rowsChanged ⇒ invalidateFrozenArchive(characterId) + publishRealtime('memories')`; `mirrored ⇒ publishRealtime('mountPoints')`; `recordMaintenanceOutcome(summary)`. Catch → WARN `'Maintenance announce failed (non-fatal)' {characterId, error}`.
- `:189-385` **`runMaintenance(request, ctx = {})`** — throws only when the corpus cannot be read:
  - `:199-208` report literal **key order**: `characterId, characterName (null), reason, dryRun, startedAt, finishedAt, durationMs, phases`; later keys append in execution order: `skippedReason` (skip arm), `fold`, `consolidation`, `demote`, `retire`, `mirror` (only phases that RAN). `finish()` rewrites `finishedAt` / `durationMs` in place.
  - `:217-225` RAW holder read (`findByIdRaw`): missing ⇒ `skippedReason 'character-not-found'`, archived (`archivedAt`) ⇒ `'archived'` + `characterName`; all five phases pushed `{phase, status: 'skipped', durationMs: 0}` — **no `skippedBecause` key** on this arm; INFO `'Maintenance skipped' {characterId, reason, skippedReason}`.
  - `:228-232` overlay `findById` with raw fallback on throw / null; `userId = ctx.userId ?? holderRaw.userId`.
  - `:235-238` `resolveMemorySettings(userId)` (emits the façade's DEBUG `[MemorySettings] Resolved memory settings`); `request.settings.consolidation` spread over `settings.maintenance.consolidation` (so a run override `{enabled: true}` lifts `consolidation-disabled`).
  - `:242-247` DEBUG `'Maintenance starting' {characterId, reason, dryRun, phases: <the phases that run>}`.
  - `:249-251` corpus loaded only when any non-mirror phase runs.
  - `:253-262` `timed(phase, body)` — a skipped phase pushes `{phase, status:'skipped', skippedBecause, durationMs: 0}`; a ran phase `{phase, status:'ran', durationMs}` AFTER its body (key order `phase, status, durationMs`).
  - **Phase 1 fold** `:265-298`: default embedding profile's provider (`repos.embeddingProfiles.findDefault(userId)`; failure DEBUG `'Could not read the default embedding profile; folding on the neural scale' {characterId, error}`) → `admissionThresholdsForProvider(provider)` → `planNearDuplicateFolds(corpus.all(), thresholds, nowMs)`; `report.fold = {eligible, deferred, groups, folded: dryRun ? Σabsorbed : 0, groupsFailed: 0, groupsDeferred: 0}` (**key order as written**); dry run or no groups ⇒ `applyFoldsToCorpus(corpus, groups, null)`; else `deadlineAt = startedMs + timeBudgetMs × 0.4` (when a budget exists) → `foldNearDuplicateGroups({characterId, groups, userId, deadlineAt})` → `folded = retiredIds.length`, `groupsFailed`, `groupsDeferred`, then `applyFoldsToCorpus(corpus, groups, applied)`.
  - **Phase 2 consolidate** `:302-320`: `runConsolidatePhase(corpus, holder, {dryRun, settings: request.settings?.consolidation, maxClustersPerRun: request.settings?.maxClustersPerRun, userId, timeBudgetMs: max(0, budget − elapsed), trigger: reason, now, matureAfterDays: settings.maintenance.consolidation.matureAfterDays})`; `report.consolidation = run.report`; `touchedSubjects = run.touchedSubjects`.
  - **Phase 3 demote** `:323-339`: `cap = request.settings?.cap ?? settings.maintenance.capFor(characterId)`; `planDemotion(corpus.all(), cap, nowMs)`; live with ids ⇒ `memoryIndex.setTier(characterId, demotedIds, 'cold')`; corpus patched `{tier:'cold'}` both modes; DEBUG `'Demotion planned' {characterId, cap, demotable, protected, demoted, highestScoreDemoted}`.
  - **Phase 4 retire** `:342-352`: `retentionDays = coldRetentionDays`; `planRetention(corpus.all(), retentionDays, nowMs)`; live with ids ⇒ `retireMemories(expired, {reason: 'retention'})` (its report's `retiredIds` win — a digest's last member is spared); `corpus.remove(retiredIds)`; `report.retire = {retentionDays, retired, retiredIds}`.
  - **Phase 5 mirror** `:355-361`: when it would run and `touchedSubjects` is empty ⇒ `nothing-touched`; else `deferred = !dryRun && isJobChild()`; `filesWritten = dryRun || deferred ? 0 : await mirrorCommonplaceForSubjects(characterId, touchedSubjects, now.toISOString())`; `report.mirror = {subjects: touchedSubjects, filesWritten, deferredToCommit: deferred}`; deferred ⇒ DEBUG `'Commonplace mirror deferred to the commit hook' {characterId, subjects: <count>}`.
  - `:363-369` `finish()`; live in-process ⇒ `announceMaintenance(summarize(report), {rowsChanged: changedRows, mirrored: filesWritten > 0})`.
  - `:371-383` INFO **`'Maintenance run complete'`** `{characterId, characterName, reason, dryRun, ran: [phases], folded, digestsCreated, membersSuperseded, demoted, retired, durationMs}` (each `?? null`).

### 1.5 `lib/memory/maintenance/fold-near-duplicates.ts` (304) — phase 1, LLM-free

- `:43` `module: 'memory:maintenance:fold'`; `:46` `YIELD_EVERY = 200` (NO-PORT, scheduling); `:53` `FOLD_MAX_BUCKET_ROWS = 2000`.
- `:56-63` `isFoldEligible(m)` = `policyProtectionOf(m) === null` (not MANUAL / CONSOLIDATED / cold) `&& source === 'AUTO' && embedding instanceof Float32Array && length > 0`.
- `:65-68` bucket key `${aboutCharacterId || characterId}|${kind ?? 'semantic'}|${embedding.length}` (empty-string `aboutCharacterId` reads as the holder).
- `:73-128` **`planNearDuplicateFolds(rows, {nearDuplicate, dateGuardDays}, nowMs)`** → `{eligible, deferred, groups}`: buckets in first-seen order; a bucket < 2 rows skipped; > 2000 ⇒ newest 2000 by `createdAt` desc (stable), `deferred += excess`; order by `protection(m, {nowMs}).score` DESC, ties `createdAt` ASC; **survivor-first, non-transitive**: each unclaimed row claims every LATER unclaimed row not `occasionsAreDistinct(survivor.occurredAt, other.occurredAt, dateGuardDays)` with `cosineSimilarity ≥ nearDuplicate`; `minCosine` starts at 1. Thresholds: neural `nearDuplicate 0.90`, TF-IDF (`BUILTIN`) `0.80`; `dateGuardDays 7` (`settings-defaults.ts:45-74`). The old dedup's 0.80 default / O(n²) transitive union-find / own survivor score are GONE.
- `:131-147` `AppliedNearDuplicateFolds {survivors, retiredIds, groupsFailed, groupsDeferred}`, `ApplyNearDuplicateFoldsRequest {characterId, groups, userId, deadlineAt?}`.
- `:160-198` `foldOneGroup` — under `withAdmissionLock(characterId, …)` (= admission's `withCharacterLock`, `admit.ts:270`): re-read `findByIds([survivor, …absorbed])` filtered to the holder; survivor missing / ineligible or no absorbed still eligible ⇒ DEBUG `'Near-duplicate group no longer applies; skipped' {characterId, survivorId, survivorPresent, absorbedStill}` → null; `applyFold(survivor, foldInto(survivor, absorbed, FOLD_CAPS), {userId})`, throw ⇒ WARN `'Near-duplicate fold failed; its rows are kept' {characterId, survivorId, error}` → null; then `retireMemories(absorbed ids, {reason: 'near-duplicate-fold'})` INSIDE the lock.
- `:207-256` **`applyNearDuplicateFolds(request)`** — child ⇒ throws `'applyNearDuplicateFolds runs in the parent; the job child folds through host-RPC (foldNearDuplicates)'`; per group: past `deadlineAt` ⇒ `groupsDeferred = groups.length − i`, INFO `'Near-duplicate fold budget spent; the remaining groups wait for the next run' {characterId, deferred}`, break; null ⇒ `groupsFailed++`; any retired ⇒ `notifyChild('vectorStore', characterId)` (child-cache coherence, DEBUG `'Could not tell the job child to drop its memory index cache'` on failure — **NO-PORT** in v5, one process); DEBUG `'Near-duplicate folds applied' {characterId, groups, landed, failed, deferred, retired}`.
- `:262-266` `foldNearDuplicateGroups` — parent direct, child via host-RPC `foldNearDuplicates` (`host-rpc-dispatcher.ts` case; **NO-PORT** transport, v5 calls direct).
- `:273-303` `applyFoldsToCorpus(corpus, groups, applied)` — live: put each survivor (keeping the corpus vector when the returned row lacks one), remove retired; dry: re-plan `foldInto` on the corpus rows → `{...survivor, ...plan.patch, content: plan.content, relatedMemoryIds: [...old, ...plan.addLinks], reinforcementCount: (count ?? 1) + plan.reinforcementDelta, reinforcedImportance: calculateReinforcedImportance(importance, count), lastReinforcedAt: plan.reinforcedAt ?? old}`, remove absorbed.

### 1.6 `lib/memory/maintenance/consolidate.ts` (183) — phase 2, the loader half

- `:27` `module: 'memory:maintenance:consolidate'`; `:34` `CONSOLIDATION_MAX_BUCKET_ROWS = 2000` (moved from `consolidation.ts`).
- `:38-43` bucket key: no `aboutCharacterId` ⇒ `'none'`, `=== holderId` ⇒ `'self'`, else `` `other:${about}` ``.
- `:46-56` `capBucketRows` — over 2000: never-considered (`consolidatedAt` null) first, then `reinforcedImportance` DESC (`?? 0`), stable.
- `:62-134` **`bucketCandidates(rows, holderId, matureCutoffMs)`** → `{rowIndex, buckets, stats}` — every row into `rowIndex {id, relatedMemoryIds ?? []}` and `stats.rowsLoaded++`; CONSOLIDATED: cold skipped, no embedding ⇒ `noEmbeddingSkipped++`, else bucket `digests` + `digestsLoaded++`; non-AUTO skipped; **cold AUTO with `supersededById` skipped, cold AUTO without it re-enters as a candidate (`demotedCandidates++`, bug 211)**; `createdAt` non-finite or `> matureCutoffMs` ⇒ `immatureSkipped++`; no embedding ⇒ `noEmbeddingSkipped++`; else bucket `rows` + `candidates++`; then cap each bucket (`bucketRowsCapped += cut`). Bucket ref: `{kind, subjectCharacterId: key==='none' ? null : about, subjectName: ''}`, `aboutCharacterId` likewise.
- `:137-154` `runConsolidatePhase(corpus, holder, opts & {matureAfterDays, now})` — `matureCutoffMs = now − matureAfterDays × DAY_MS`; DEBUG `'Consolidation candidates bucketed' {characterId, candidates, demotedCandidates, digests, buckets}`; `consolidate({holder, rowIndex, buckets, stats}, opts)` then `applyConsolidationToCorpus(corpus, run.plan, run.belowMinConsidered, now.toISOString())`.
- `:157-183` `applyConsolidationToCorpus` — creates put `{...data, id, embedding ?? null, tier: 'hot', createdAt, updatedAt: nowIso}`; updates patched (+ embedding); tier moves `{tier:'cold', supersededById, supersededAt: nowIso, consolidatedAt: nowIso}`; `considered ∪ belowMinConsidered` ⇒ `consolidatedAt`; link rewrites.

### 1.7 `lib/memory/maintenance/demote.ts` (70) — phases 3 and 4, pure planners

- `:28-30` `isDemotable(m)` = hot AND `source === 'AUTO'` (MANUAL / digests never demoted, never counted — bug 210).
- `:33-57` **`planDemotion(rows, cap, nowMs)`** — literal **key order** `cap, hotTotal, demotable, protected (= hot − demotable), excess = max(0, demotable − cap), demoted, demotedIds, highestScoreDemoted (null)`; excess 0 ⇒ return; else score each demotable by `protection(m, {nowMs}).score`, sort ASC, ties `Date.parse(createdAt)` ASC (a NaN comparator result is +0 in V8 — stable), take exactly `excess`; `highestScoreDemoted` = last taken's score. **"Exactly demotable − cap, lowest protection first"** — NOT the old "unprotected + old + inactive + below floor" retention pass.
- `:60-70` **`planRetention(rows, retentionDays, nowMs)`** — cold AND `supersededById` AND `source === 'AUTO'` AND `Date.parse(supersededAt ?? updatedAt) < nowMs − retentionDays × 86_400_000` (finite).

### 1.8 `lib/memory/maintenance/ledger.ts` (282) — parent-only, in-memory

- `:27` `module: 'memory:maintenance:ledger'`. Constants: `:30` `CAP_WATERMARK_FRACTION = 0.9`; `:33` `WATERMARK_CHECK_DEBOUNCE_MS = 2 min`; `:36` `DEMOTE_THROTTLE_MS = 15 min`; `:39` `INEFFECTIVE_BACKOFF_MS = 60 min`; `:42` `CONSOLIDATE_THROTTLE_MS = 6 h`; `:45` `MIN_EFFECTIVE_DEMOTIONS = 10`; `:47` `EXCESS_RATIO_THRESHOLD = 0.01`.
- `:49-65` `PhaseRecord {at, reason}`; `LedgerEntry {lastRun: Partial<Record<phase, PhaseRecord>>, lastSummary, lastDemotion {at, demoted, excess} | null, lastEnqueued: Partial<Record<phase, ms>>, lastWatermarkCheck}`; `:73-77` `processSingleton('memory.maintenanceLedger', …)` (bug-235 Next.js workaround — v5: a process-global `OnceLock<Mutex<…>>`, the `housekeeping_outcome_cache.rs` precedent).
- `:100-131` `ensureSeeded()` — once per process: `backgroundJobs.findRecentByType('MEMORY_MAINTENANCE', 100)` (`ORDER BY updatedAt DESC`); per job with status COMPLETED or PROCESSING, string `payload.characterId`, finite `Date.parse(updatedAt)`: `reason = payload.reason ?? 'manual'`, `phases = Array.isArray(payload.phases) ? payload.phases : all five`; `lastRun[phase]` takes the later `at`. DEBUG `'Maintenance ledger seeded from recent jobs' {jobs, characters}`; failure WARN `'Maintenance ledger could not seed from recent jobs' {error}` and stays unseeded.
- `:134-151` `recordMaintenanceOutcome(summary)` — dry run or `skippedReason` ⇒ nothing; `at = Date.parse(finishedAt) || Date.now()`; every RAN phase ⇒ `lastRun[phase] = {at, reason}`; `lastSummary = summary`; `summary.demote` ⇒ `lastDemotion {at, demoted, excess}`; DEBUG `'Maintenance outcome recorded' {characterId, reason, ran, demoted}`.
- `:154-163` `WatermarkReading {demotable, cap, unconsidered, consolidationWatermark, maintenanceEnabled, consolidationEnabled}`.
- `:171-176` `lastDemotionIneffective` — within the hour, `demoted < max(10, floor(excess × 0.01))` (a no-excess demotion therefore IS ineffective).
- `:179-182` `ranWithin(entry, phase, window, now)` — `max(lastRun.at, lastEnqueued)`.
- `:196-231` **`shouldEnqueue(characterId, reason, reading?, nowMs)`** — non-watermark ⇒ all five; watermark without reading ⇒ null; seed; **cap watermark** `maintenanceEnabled && demotable ≥ floor(cap × 0.9)` → throttled 15 min (DEBUG `'Cap watermark reached but a demotion ran recently' {characterId, demotable, cap}`) / ineffective back-off (DEBUG `'Cap watermark reached but the last demotion was ineffective; backing off' {characterId, lastDemotion}`) / else add `fold-near-duplicates, demote, retire`; **consolidation watermark** `consolidationEnabled && unconsidered > watermark` → 6 h throttle (DEBUG `'Consolidation watermark reached but a consolidation ran recently' {characterId, unconsidered}`) / else add `consolidate, mirror`; returns canonical order or null.
- `:237-242` `takeWatermarkCheck` (2-min debounce, stamped on take); `:245-248` `noteEnqueued`.
- `:251-282` `LedgerStatus {characterId, lastRun {phase: {at ISO, reason}}, lastSummary, lastDemotion {at ISO, demoted, excess, effective} | null}`; `getMaintenanceLedgerStatus(characterId?)` — seeds; ids = the one asked (an unknown id answers an EMPTY entry, not created) or every entry in insertion order; `effective = !lastDemotionIneffective(entry, min(now, lastDemotion.at))` (evaluated AT the demotion — the hour window always applies).

### 1.9 `lib/memory/maintenance/scheduler.ts` (141) — the daily sweep (parent)

- `:18` `module: 'memory:maintenance:scheduler'`; `:21` `MAINTENANCE_INTERVAL_MS = 24 h`; `:24` `STARTUP_GRACE_MS = 5 min`; `:27` `RECENT_RUN_WINDOW_MS = 20 h` (v5 `services/job_scheduler.rs:34-43` already carries all three).
- `:36-54` `scheduleMemoryMaintenance(intervalMs)` — idempotent; interval tick → `runScheduledMemoryMaintenance()` (ERROR `'Scheduled maintenance tick failed' {error}`); INFO `'Maintenance scheduler started' {intervalMs, startupGraceMs}`; startup timeout → `runStartupTick()` (ERROR `'Startup maintenance tick failed' {error}`).
- `:57-64` `stopMemoryMaintenanceScheduler` — clears BOTH timers (the startup tick too — PR-97 review); INFO `'Maintenance scheduler stopped'`.
- `:71-94` `runStartupTick(nowMs)` — `findRecentByType('MEMORY_MAINTENANCE', 50)`; a COMPLETED job with `payload.reason === 'scheduled'` and `updatedAt ≥ now − 20 h` ⇒ INFO `'Skipping startup maintenance tick — a scheduled run completed recently' {jobId, completedAt}` and return; read failure ⇒ WARN `'Recent-run check failed; running the startup maintenance tick anyway' {error}`.
- `:101-141` **`runScheduledMemoryMaintenance()`** → `{charactersScanned, jobsEnqueued}` — `resolveMemorySettings()` (no user); both `maintenance.enabled` and `consolidation.enabled` off ⇒ DEBUG `'Scheduled maintenance skipped: maintenance and consolidation are both off'`; for each `memories.findDistinctCharacterIds()`: `charactersScanned++`, `findByIdRaw`, skip missing / archived, group by `character.userId` (Map insertion order; WARN `'Failed to read a character for scheduled maintenance' {characterId, error}`); per owner `enqueueMemoryMaintenanceMany(userId, ids, {reason: 'scheduled'})` (WARN `'Failed to enqueue scheduled maintenance for a user' {userId, characters, error}`); INFO `'Scheduled maintenance pass complete' {charactersScanned, jobsEnqueued}`. **No per-character backlog gate** (the old `SCHEDULED_MIN_UNCONSIDERED = 2` is gone) and **no per-user `autoHousekeepingSettings.enabled` gate** — every live holder of memories gets a job; the job resolves its phases.

### 1.10 `lib/memory/maintenance/triggers.ts` (110) — the watermark half (parent)

- `:18` `module: 'memory:maintenance:triggers'`.
- `:27-38` `characterIdsFromMemoryWrites(writes)` — `memories.create` writes' `args[0].characterId`, deduped, first-seen order.
- `:45-95` **`maybeEnqueueMaintenance(characterIds, nowMs)`** — debounce first (`takeWatermarkCheck` per id, BEFORE any read); `resolveMemorySettings()`; both off ⇒ return; per due id: `countCapBreakdown(id).demotable`, `countUnconsideredHot(id)` only when consolidation is on (else 0); `shouldEnqueue(id, 'watermark', {demotable, cap: capFor(id), unconsidered, consolidationWatermark: consolidation.watermark, maintenanceEnabled, consolidationEnabled}, nowMs)`; null ⇒ DEBUG `'Maintenance watermarks not due' {characterId, demotable, unconsidered}`; missing / archived character ⇒ skip silently; `enqueueMemoryMaintenance(userId, {characterId, reason: 'watermark', phases})`; `noteEnqueued`; INFO `'Maintenance watermark reached; run enqueued' {characterId, phases, demotable, unconsidered, jobId}`. Catch ⇒ WARN `'Maintenance watermark check failed (non-fatal)' {characterIds, error}`.
- `:102-110` `maybeEnqueueMaintenanceAfterCommit(jobType, writes)` — `MEMORY_MAINTENANCE` ⇒ return; ids from writes; none ⇒ return.
- **Callers:** `admission/admit.ts:405-412` — after every admission that INSERTED a row (any source, backfill included), `void import('../maintenance/triggers').then(m => m.maybeEnqueueMaintenance([characterId]))`, catch DEBUG `'Maintenance watermark check skipped after admission' {characterId, error}` (this replaces `admission/after-insert.ts`'s `maybeEnqueueHousekeeping`); `job-dispatcher.ts:671-683` `checkMaintenanceWatermarkAfterCommit` (catch DEBUG `'Maintenance watermark check skipped after job' {jobId, error}`). **⚑ Finding:** at the pin every child memory insert goes through admission over host-RPC (`admit.ts:225-234` → `admitPrepared` in the parent, `:324`), so the only `memories.create` a child batch still buffers is consolidation's digest create (`consolidation.ts:484`) — and that is a `MEMORY_MAINTENANCE` batch, excluded. `characterIdsFromMemoryWrites` therefore returns `[]` for every reachable job batch; the commit-hook path is a belt-and-braces no-op and the admission call is the live trigger (see §3 Δ19 / §9 P1).

### 1.11 `lib/background-jobs/handlers/memory-maintenance.ts` (141) — the handler

- `:30` `MAINTENANCE_JOB_TIME_BUDGET_MS = 7 min` (inside the 10-min stuck window); `:41` `MAINTENANCE_FAIL_AFTER_PLAN_ENV = 'QUILLTAP_TEST_MAINTENANCE_FAIL_AFTER_PLAN'`; `:44` `MAINTENANCE_DEFER_MS = 2 min`; `:46` `module: 'jobs:memory-maintenance'`.
- `:49-54` `enqueuedBefore(a, b)` — `createdAt` (finite, unequal) else `a.id < b.id`.
- `:61-80` `findOtherRunningMaintenance(job)` — `findByUserId(job.userId, 'PROCESSING')` (LIMIT 100, `createdAt DESC`), another `MEMORY_MAINTENANCE` for the same `payload.characterId` enqueued before this one; read failure ⇒ WARN `'Could not check for a concurrent maintenance run; running anyway' {jobId, error}` → null.
- `:82-141` `handleMemoryMaintenance(job)` — no `characterId` ⇒ WARN `'Maintenance job has no characterId; nothing to do' {jobId}`, completes; an older run in progress ⇒ `enqueueMemoryMaintenance(job.userId, payload, {scheduledAt: now + 2 min})` and INFO `'Another maintenance run is under way for the character; deferred' {jobId, characterId, runningJobId, successorId}`, completes (bug 221); else `runMaintenance({characterId, reason: payload.reason ?? 'manual', [exemptPhases], [phases], [settings]}, {userId: job.userId, timeBudgetMs: 7 min})`; `summarizeMaintenance`; the test flag (`=== '1'`): non-production ⇒ throw `'Maintenance failed after planning (test flag)'`, production ⇒ WARN `'Ignoring the maintenance fail-after-plan test flag in production' {jobId, env}`; `setJobResult(summary)`; INFO `'Maintenance job complete' {jobId, characterId, reason, ran, folded, digestsCreated, demoted, retired, durationMs}`.
- `handlers/index.ts:55` registers `MEMORY_MAINTENANCE: handleMemoryMaintenance` (the old two entries gone).

### 1.12 `lib/background-jobs/queue-service.ts` — the maintenance enqueue (`:325-343`, `:635-785`)

- `:329-343` `MemoryMaintenancePayload {characterId; phases?; reason; exemptPhases?; settings?: MaintenanceRunSettings}` — **no `dryRun`** (a dry run never becomes a job).
- `:636-639` `unionPhases(a, b)` — either undefined ⇒ undefined (all phases absorbs).
- `:646-650` `exemptPhasesOf(payload)` — the `exemptPhasesFor` rule, again.
- `:653-666` `widenPayload(prior, payload)` — `{...prior, [phases: union], reason: payload.reason === 'manual' ? 'manual' : prior.reason, exemptPhases: [...new Set(prior exempt ++ new exempt)], [settings: {...prior.settings, ...payload.settings}]}`, `delete phases` when the union is undefined. **Key order** = prior's keys first (spread keeps positions), new keys appended.
- `:672-703` `enqueueMaintenanceAgainst(userId, payload, pending, options)` — `normalized = {...payload, exemptPhases: exemptPhasesOf(payload)}`; a PENDING job for the same `characterId` ⇒ `backgroundJobs.update(id, {payload: widened})`, DEBUG `'[Maintenance] Pending job widened to the requested phases' {characterId, existingJobId, phases: widened.phases ?? 'all', exemptPhases, reason}` (root logger), return its id; else `enqueueJob(userId, 'MEMORY_MAINTENANCE', normalized, {...options, maxAttempts: options.maxAttempts ?? 1})` and push a stub into `pending`.
- `:706-718` `findPendingMaintenance(userId, characterId?)` — `findByUserId(userId, 'PENDING')` filtered to the type; catch WARN `'[Maintenance] Failed to check for pending jobs during enqueue' {characterId, error}` → null (safeQuery already answers `[]` on a failed read, so this WARN is near-unreachable).
- `:739-746` `enqueueMemoryMaintenance(userId, payload, options?)` — **only PENDING absorbs; a PROCESSING job never does** (the handler defers instead).
- `:753-765` `enqueueMemoryMaintenanceMany(userId, characterIds, payload, options?)` — one pending read per owner; ids deduped.
- `:772-785` `enqueueMemoryMaintenanceForUser(userId, payload, options?)` — live (`!archivedAt`) characters of `characters.findByUserId(userId)`; DEBUG `'[Maintenance] User-wide run fanned out per character' {userId, reason, characters, jobs}`.
- **Stored payload bytes** (tier-2 comparands): API `{"reason":"manual"[,"phases":[…]][,"settings":{…}],"characterId":…,"exemptPhases":[…]}`; scheduler `{"reason":"scheduled","characterId":…,"exemptPhases":[]}`; watermark `{"characterId":…,"reason":"watermark","phases":[…],"exemptPhases":[]}`; defer re-enqueue = the job's own payload (exemptPhases already present, kept in place). `maxAttempts 1`; `scheduledAt` now or +2 min.

### 1.13 Dispatcher post-commit hooks (`lib/background-jobs/host/job-dispatcher.ts`)

Success arm `:246-266`, in order after `markCompleted` + INFO `'Job completed'` + `publishRealtime('jobs')`: `topicsForCompletedJob` hints → `invalidateFrozenArchivesForCompletedJob(job)` → `checkMaintenanceWatermarkAfterCommit(job, writes)` → `recordMaintenanceAfterCommit(job, msg.result)` → `mirrorCommonplaceAfterCommit(job, writes)` (all `void`).
- `:663` `JOBS_INVALIDATING_FROZEN_ARCHIVE = new Set(['MEMORY_MAINTENANCE'])`.
- `:691-704` `recordMaintenanceAfterCommit` — type `MEMORY_MAINTENANCE` and an object result ⇒ `recordMaintenanceOutcome(result)`; catch DEBUG `'Maintenance outcome not recorded after job' {jobId, error}`.
- `:710-722` mirror hook — catch WARN `'Commonplace mirror skipped after job' {jobId, error}` (the hook body `commonplace-mirror.ts:204-228`: non-mirroring type or `payload.dryRun === true` ⇒ return; no touched bucket ⇒ DEBUG `'Committed maintenance batch touched no mirrored bucket' {jobType}`; per holder `mirrorCommonplaceForSubjects(holderId, subjects, updatedAt)`; WARN `'Commonplace mirror after commit failed (non-fatal)' {jobType, error}`).
- `:760-779` `invalidateFrozenArchivesForCompletedJob` — type gate; `dryRun === true` ⇒ return; **a missing / non-string / EMPTY `characterId` now RETURNS SILENTLY** (pre `:671-695 (pre)` called `invalidateAllFrozenArchives()` and logged `characterId: 'all'`); DEBUG `'Invalidated frozen memory archives after job' {jobId, type, characterId}`; WARN `'Failed to invalidate frozen memory archives after job' {jobId, error}`.
- `ipc-types.ts:103-112` `ChildJobResultMessage.result` + `child-repositories-proxy.ts:91,167-175` `setJobResult` — the child→parent summary carrier. **The result is NOT persisted** (`markCompleted(jobId)` takes no result): the summary lives only in the in-memory ledger.

### 1.14 Job tables

- `schemas/job.types.ts:19-43` `BackgroundJobTypeEnum` — **23 values** (pre 25): `INTER_CHARACTER_MEMORY` (Phase D), `MEMORY_HOUSEKEEPING`, `MEMORY_CONSOLIDATION` removed; `'MEMORY_MAINTENANCE'` sits where `MEMORY_HOUSEKEEPING` sat (after `CONVERSATION_RENDER`, before `MEMORY_REGENERATE_CHAT`).
- `activity-kinds.ts:38-46` — `MEMORY_MAINTENANCE: 'memory'` after `MEMORY_REGENERATE_ALL`, before `CARINA_MEMORY_EXTRACTION` (the `MEMORY_HOUSEKEEPING` / `MEMORY_CONSOLIDATION` / `INTER_CHARACTER_MEMORY` rows gone).
- `realtime/job-topics.ts:85-90` `MEMORY_MAINTENANCE` ⇒ `[{topic:'memories'}, {topic:'mountPoints'}]` **unconditionally** (the old consolidation `dryRun === true ⇒ []` arm is GONE; housekeeping's memories-only arm gone).

### 1.15 `lib/memory/consolidation.ts` (914; pre 1,067) — the planner + model route

The `consolidate` phase calls ONE entry: `:571-914` **`consolidate(input: ConsolidationInput, opts: RunConsolidationOptions)`** → `ConsolidationRun {report, plan, belowMinConsidered, touchedSubjects}`. What moved relative to v5's `run_consolidation` (= pre `runConsolidation`, `:524 (pre)`):
- **Loader removed** — holder read + archived / not-found skips (pre INFO `'Consolidation skipped: character not found'` / `'…is archived'`, `skippedReason 'character-not-found' | 'archived'`) and the paged load + bucketing + cap are now `maintenance/run.ts` + `maintenance/consolidate.ts`. `ConsolidationReport.skippedReason` narrows to `'no-llm'` (`:191`). `CONSOLIDATION_LOAD_PAGE_SIZE` deleted, `CONSOLIDATION_MAX_BUCKET_ROWS` moved.
- **Announce removed** — no vector-store staging (pre WARNs `'Vector store unavailable during consolidation; digests are written without vectors'`, `'Failed to stage digest vector'`, `'Failed to save vector store after consolidation'` all gone — Phase B's index owns vectors), no in-run mirror (pre WARN `'Commonplace mirror failed after consolidation (digests are still written)'` gone; `stats.mirrorFilesWritten` gone), no frozen-archive / realtime announce.
- `:148-177` `ConsolidationStats` — gains `demotedCandidates` (after `candidates`); loses `mirrorFilesWritten`.
- `:231-274` NEW exports `createConsolidationStats`, `hasEmbedding`, `eventTimeMs`; `:220-227` `ConsolidationBucket`; `:545-563` `ConsolidationInput`, `ConsolidationRun`.
- `:581-617` settings = `{...getMemoryConsolidationSettings(), ...opts.settings}`; **`settings.clusterThreshold = resolveClusterThreshold(settings, defaultProfileProvider, opts.settings?.clusterThreshold)`** (`settings-defaults.ts:82-89`: an explicit numeric override wins; else `BUILTIN` ⇒ `tfidfClusterThreshold`, else `clusterThreshold`) — so the REPORTED `settings.clusterThreshold` is the RESOLVED one; profile-read failure DEBUG `'Could not read the default embedding profile; clustering on the neural scale' {characterId, error}`.
- `:619-630` DEBUG `'Consolidation starting'` gains `embeddingProvider` (after `clusterThreshold`).
- `:326-336` **bug 246** `profileMaxTokensFor(route, selection)` — the ROUTED selection's `connectionProfileId` looked up in `route.availableProfiles` ⇒ `resolveMaxTokens(profile)` (`model-context-data.ts:371-391`: `maxTokens > 0` → model class `maxOutput` → `DEFAULT_MAX_TOKENS` 8000; v5 `context_budget.rs:100` `resolve_max_tokens`), else undefined; passed as `profileMaxTokens` (`:754-758`).
- `:473-521` `executePlan` — creates `repos.memories.create({…data, embedding}, {id, createdAt, updatedAt: nowIso})` + `memoryIndex.indexInserted` (DEBUG `'Digest created'`); updates with an embedding via `memoryIndex.upsert(… alongside: patch)` else `updateForCharacter` (DEBUG `'Digest revised in place'`); tier moves via `memoryIndex.setTier(characterId, ids, 'cold', {supersededById, consolidatedAt: nowIso})` (the repository's `setTier` stamps `supersededAt` with `supersededById` and nulls it when the link is cleared — `memories.repository.ts:1158-1164`, bug 225); `markConsidered`; link rewrites.
- `:527-538` `touchedSubjects` — `aboutCharacterId` of each created / updated digest's bucket (the `'none'` bucket excluded).
- `:833-848` digests embed with `refreshMemoryTimeZone()` + `lexicalText: lexicalEmbeddingTextFor(digest)` (time / index slices).
- `:891-912` INFO `'Consolidation run complete'` field list unchanged in name (`characterId, characterName, trigger, dryRun, candidates, demotedCandidates, digestsLoaded, clustersFound, clustersSelected, clustersSucceeded, clustersFailed, clustersDeferred, digestsCreated, digestsUpdated, membersSuperseded, rowsMarkedConsidered, linksRewired, budgetExhausted, durationMs`) — diff against v5's list (`services/consolidation.rs:1840`) for `demotedCandidates` and any `mirrorFilesWritten`.
- Unchanged: the model route (`:298-324`, DEBUG `'Consolidation using its configured connection profile'`, WARN `'Configured consolidation profile no longer exists; falling back to the cheap LLM'`), Concierge routing (`:339-373`), `describeBuckets` (`:376-454`), the cluster loop's lines (`'Bucket clustered'`, WARN `'Consolidation has clusters to fold but no model to fold them with'`, INFO `'Consolidation time budget spent; remaining clusters wait for the next run'`, WARN `'Consolidation call failed; cluster skipped'`, WARN `'Consolidation answer failed validation; cluster skipped'`, DEBUG `'Cluster consolidated'`, WARN `'Digest embedding failed; its cluster is skipped'`).

### 1.16 `lib/memory/cheap-llm-tasks/consolidation-tasks.ts` (395) — the ONE LLM prompt family

- The system prompt, the user-message builder, the schema and the validator are **byte-unchanged** in the span (only a doc-comment path `recall-tags.ts → recall/tags.ts`). v5's GENERATED `consolidation_tasks/prompt_text.rs` stays valid.
- `:46` `CONSOLIDATION_MIN_MAX_TOKENS = 4000` (was the private ceiling `CONSOLIDATION_MAX_TOKENS`); `:49-50` `CONSOLIDATION_EMPTY_ANSWER_REASON = 'empty answer: the model returned no text (a reasoning model may have spent the whole output budget thinking)'`.
- `:336-338` `parseConsolidationResponse` — `!content.trim()` ⇒ `{ok: false, reason: CONSOLIDATION_EMPTY_ANSWER_REASON}` BEFORE the JSON parse.
- `:355-395` `consolidateMemoryCluster(input, selection, userId, {characterId?, uncensoredFallback?, profileMaxTokens?})` — `maxTokens = max(4000, profileMaxTokens ?? 0)`; DEBUG `'[Consolidation] Sending cluster to model'` (root logger) gains `maxTokens` as its LAST field; `executeCheapLLMTask(…, maxTokens, characterId)`. **Tier-3 comparand:** the provider request's `max_tokens` is now 8000 (or the profile's own) for every call routed to a known profile — every canned-LLM consolidation row's request bytes move.

### 1.17 `lib/memory/repair.ts` (98) — graph repair (`?action=repair`)

- `:20` `module: 'memory:repair'`; `:23` `REPAIR_EMBEDDING_BATCH = 2000`.
- `:25-45` `MemoryGraphRepairReport {dryRun, characterId (null = whole instance), found {related {links, rows}, consolidatedFrom {links, digests}, supersededBy, missingEmbeddings}, repaired?: DanglingScrubReport & {embeddingJobsQueued, embeddingSkippedReason?: 'no-embedding-profile'}}` (key order as listed).
- `:47-98` `repairMemoryGraph({userId, characterId?, dryRun})` — `memories.findDanglingLinks(scope)` (`memories.repository.ts:1290-1351`: `json_each` + `NOT EXISTS` per column, `supersededById` scan, holder-scoped in variable chunks; **no fallback — a failed scan throws**, DEBUG `'Scanned for dangling memory links' {holders, related, consolidatedFrom, supersededBy}`); `countWithoutEmbedding(characterId?)`; live ⇒ `scrubDanglingMemoryLinks(links)` (`retire.ts:480-528`: per-row `removeRelatedMemoryIds`, a digest whose dangling count ≥ `json_array_length` retired via `retireMemories(…, {reason: 'graph-repair'})` else `removeConsolidatedMembers`, orphaned members `memoryIndex.setTier(…, sameTier, {supersededById: null})`, DEBUG `'Scrubbed dangling memory links'`, `announceRetirement`) + `enqueueMissingEmbeddings({userId, characterId, limit: 2000})` (`index-store/backfill.ts`); INFO `'Memory graph repair' {dryRun, characterId: id ?? 'all', found, repaired: … ?? null}`.

### 1.18 Settings at the pin (`lib/schemas/settings.types.ts`, `lib/instance-settings/index.ts`, `lib/memory/settings*.ts`)

- `settings.types.ts:135-136` `MEMORY_MAINTENANCE_CAP_MIN = 100`, `_MAX = 100_000`; `:139-143` `MemoryMaintenanceCapSchema = z.number().int().min(100).max(100000)`; `:149` `MemoryMaintenanceCapOverridesSchema = z.record(z.string(), cap)`.
- `:153-160` `MemoryMaintenancePhaseTogglesSchema = z.object({ foldNearDuplicates: z.boolean().default(true), demote: z.boolean().default(true), retire: z.boolean().default(true) })`.
- `:170-185` **`MemoryMaintenanceSettingsSchema = z.object({ enabled: z.boolean().default(false), perCharacterCap: cap.default(2000), perCharacterCapOverrides: overrides.default({}), phases: toggles.prefault({}) })`** — key order `enabled, perCharacterCap, perCharacterCapOverrides, phases {foldNearDuplicates, demote, retire}`; `.prefault({})` ⇒ a stored value WITHOUT `phases` reads every phase ON. The retired `autoMergeSimilarThreshold` / `mergeSimilar` keys are stripped (non-strict object).
- `:252-276` `MemoryConsolidationSettingsSchema` — key order `enabled (false), connectionProfileId (null), clusterThreshold (0.72), **tfidfClusterThreshold (z.number().min(0).max(1).default(0.30))**, minClusterSize (int ≥ 2, 3), maxClusterSize (int ≥ 2, 30), matureAfterDays (≥ 0, 7), maxClustersPerRun (int > 0, 40), watermark (int > 0, 150), coldRetentionDays (int > 0 | null, null)`. `coldRetentionDays` stays HERE (maintenance reads it as `settings.maintenance.coldRetentionDays`).
- `instance-settings/index.ts:46` `KEY_MEMORY_MAINTENANCE = 'memoryMaintenance'`; `:89` defaults = `parse({})`; `:266-272` `getMemoryMaintenanceSettings()` = `readJsonSetting` (unwritten ⇒ defaults silently; unparseable / invalid ⇒ defaults + the WARN v5's `read_json_setting` already ports); `:274-283` `setMemoryMaintenanceSettings(value)` = `writeJsonSetting({...current, ...value})` (validated, stored in schema order); `:219-235` the consolidation getter / setter (same shape).
- `memory/settings.ts:64-73` `resolveMemoryCap(settings, characterId)` = `perCharacterCapOverrides[id] ?? perCharacterCap ?? 2000`; `:81-83` `withCapFor`; `:90-92` `resolveMemoryMaintenance()`; `:99-130` `resolveMemorySettings(userId?)` → `{…, maintenance: {...withCapFor(maintenance), consolidation, coldRetentionDays: consolidation.coldRetentionDays}}` + DEBUG `[MemorySettings] Resolved memory settings {userId: id ?? null, maintenanceEnabled, perCharacterCap, capOverrides: <count>, consolidationEnabled, otherPass}` — the keystone's façade; maintenance is a consumer.
- `memory/settings-defaults.ts:45-74` `ADMISSION_THRESHOLDS` / `_TFIDF` + `admissionThresholdsForProvider`; `:82-89` `resolveClusterThreshold`; `:92-100` `MEMORY_SETTING_DEFAULTS`; `:103` `MEMORY_CAP_BOUNDS`. `embedding/cosine-scale.ts` `isLexicalEmbeddingProvider(p) = !!p && p === 'BUILTIN'`.
- **The three retention knobs removed** = the never-persisted `HousekeepingOptions` fields `maxAgeMonths` (6), `maxInactiveMonths` (6), `minImportance` (0.3) of the deleted `housekeeping.ts:24-36,90-96 (at 4ae0c4ca6)` (v5 `services/housekeeping.rs:61-110`, `api/types.rs:1689-1700` `MemoryHousekeepPreview.max_age_months` / `min_importance`). No settings key disappears for them; the retention pass they drove is gone.

### 1.19 `lib/schemas/patch-schema.ts` (42) — bug 244

`patchSchemaOf(schema)` — for each shape key, unwrap `default` / `prefault` wrappers (`_zod.def.type`), then `.optional()`; returns a NON-strict `z.object` (unknown keys stripped). Bounds, enums, nullability and field refinements kept. Used at the pin by `route.ts:136` (`maintain`'s `settings.consolidation`), `:165` (`maintenance-config` `phases`), `:173` / `:175` / `:177` / `:179` (extraction-limits / recall / consolidation / extraction-mode config), `text-replacement.types.ts:70` (`TextReplacementRulePatchSchema`).

### 1.20 The Memories API (`app/api/v1/memories/route.ts`, 1,388) — maintenance actions

Action tables at the pin: GET `:199-210` (`maintenance-status, embeddings, maintenance-config, extraction-limits-config, recall-config, consolidation-config, extraction-mode-config, backfill-embeddings, character-memory-counts, regenerate-all`); POST `:256-270` (`search, maintain, repair, embeddings, maintenance-config, extraction-limits-config, recall-config, consolidation-config, extraction-mode-config, backfill-embeddings, regenerate-all, anchor-gate-probe`). **Retired** (now `unknownActionResponse` 400 listing the table above — `lib/api/middleware/actions.ts:163-183`): GET `housekeep`, `housekeeping-config`, `extraction-concurrency`; POST `housekeep`, `housekeep-sweep`, `consolidate`, `housekeeping-config`, `extraction-concurrency`.

- `:129-141` `maintainSchema = z.object({ characterId: z.uuid().optional(), phases: z.array(z.enum(MAINTENANCE_PHASES)).min(1).optional(), dryRun: z.boolean().optional(), settings: z.object({ consolidation: patchSchemaOf(MemoryConsolidationSettingsSchema).optional(), maxClustersPerRun: z.number().int().min(1).max(1000).optional(), cap: MemoryMaintenanceCapSchema.optional() }).optional() })`.
- `:144-148` `repairSchema = z.object({ characterId: z.uuid().optional(), dryRun: z.boolean().optional() })`.
- `:163-168` `maintenanceConfigSchema = z.object({ enabled: z.boolean().optional(), phases: patchSchemaOf(togglesSchema).optional(), perCharacterCap: cap.optional(), perCharacterCapOverrides: overrides.optional() })`.
- `:731-733` `MAINTAIN_DRY_RUN_DEFAULT_CLUSTERS = 10`, `_MAX_CLUSTERS = 40`, `_TIME_BUDGET_MS = 120_000`.
- `:736-817` **`handleMaintain`** — body `req.json().catch(() => ({}))`; Zod failure ⇒ `validationError`; `phases = withImpliedPhases(parsed.phases)` when given; named character not found (overlay `findById`) ⇒ `notFound('Character')`; **dry run**: `mayCallModel = !phases || phases.includes('consolidate')`; no character AND mayCallModel ⇒ 400 `'A dry run that includes consolidation names one character (characterId)'`; `maxClustersPerRun = min(settings?.maxClustersPerRun ?? 10, 40)`; targets = the one character or the user's live characters; DEBUG `'[Memories API] Maintenance dry run (in-process)' {characterId: id ?? 'all', characters, phases: phases ?? 'all', maxClustersPerRun}`; each `runMaintenance({characterId, reason:'manual', dryRun:true, [phases], settings: {...settings, maxClustersPerRun}}, {userId, timeBudgetMs: 120000})`; answer `{success:true, dryRun:true, report}` (one) / `{success:true, dryRun:true, reports}` (all); catch ⇒ ERROR `'[Memories API] Maintenance dry run failed' {characterId}` + 500 `'Maintenance dry run failed'`. **Live**: payload `{reason:'manual', [phases], [settings]}` → `enqueueMemoryMaintenance(userId, {...payload, characterId})` or `enqueueMemoryMaintenanceForUser(userId, payload)`; INFO `'[Memories API] Maintenance enqueued' {userId, characterId: id ?? 'all', phases: phases ?? 'all', jobs}`; answer `{success:true, dryRun:false, jobId: jobIds[0] ?? null, jobIds}`; catch ⇒ ERROR `'[Memories API] Failed to enqueue maintenance' {characterId}` + 500 `'Failed to enqueue maintenance'`.
- `:819-833` `handleMaintenanceStatus` — `?characterId=` not found ⇒ 404; DEBUG `'[Memories API] Maintenance status read' {characterId: id ?? 'all', entries}`; `{success:true, ledger}`.
- `:835-858` `handleRepair` — `dryRun = false` default; 404 for an unknown character; `{success:true, report}`; catch ⇒ ERROR `'[Memories API] Memory graph repair failed' {characterId}` + 500 `'Memory graph repair failed'`.
- `:864-870` GET `maintenance-config` ⇒ `{success:true, settings}` (the whole parsed settings).
- `:872-899` POST `maintenance-config` — `req.json()`; Zod ⇒ `validationError`; `phases` merged into the STORED toggles (`{...current.phases, ...phases}`), every other key REPLACES (`perCharacterCapOverrides` whole-map replace); `setMemoryMaintenanceSettings`; INFO `'[Memories API] Memory maintenance settings updated' {userId, enabled, perCharacterCap, capOverrides: <count>, phases: <object>}`; `{success:true, settings}`.
- `:974-1000` consolidation-config — GET `{success, settings}`; POST `patchSchemaOf` parse, drop `undefined`, `setMemoryConsolidationSettings(patch)` (merges), re-read, INFO `'[Memories API] Consolidation settings updated (instance-wide)' {enabled, fields: Object.keys(patch)}` — **`fields` now lists ONLY the keys sent** (pre: all nine); `:1003-1020` extraction-mode-config likewise (`{otherPass, fields}`).
- `:1022-1050` `character-memory-counts` (bug 210, c2795733f) — `countCapBreakdown` per character → `{id, name, memoryCount: hot+cold, hotTotal, demotable, protected, cold}`, sorted `demotable DESC, memoryCount DESC` (the Maintenance card's list; the memories-API slice owns it).

### 1.21 `migrations/scripts/drop-housekeeping-consolidation-jobs-v1.ts` (67) — the job-row migration

`id 'drop-housekeeping-consolidation-jobs-v1'`, `introducedInVersion '4.10.0'`, `dependsOn ['sqlite-initial-schema-v1']`; `shouldRun` = SQLite AND `background_jobs` exists AND `SELECT COUNT(*) … WHERE "type" IN ('MEMORY_HOUSEKEEPING', 'MEMORY_CONSOLIDATION')` > 0; `run` = `DELETE FROM "background_jobs" WHERE "type" IN (…)` (ANY status); INFO `'Deleted retired housekeeping and consolidation jobs' {context: 'migration.drop-housekeeping-consolidation-jobs-v1', deleted}`; result message `` `Deleted ${n} retired housekeeping/consolidation job(s)` ``; failure ERROR `'Failed to delete housekeeping and consolidation jobs' {context, error}`. Registered `migrations/scripts/index.ts:443, :903, :1340`; startup label `lib/startup/prettify.ts` `'Retiring two charwomen in favour of one capable housekeeper…'`. NO DDL.

### 1.22 Scheduling at boot (`instrumentation.ts:849-851`)

PHASE 3.5 order: `scheduleCleanup()` → **`scheduleMemoryMaintenance()`** (replaces `scheduleHousekeeping()`) → `scheduleMaintenance()` (the daily DATA maintenance, which since `4ae0c4ca6` also calls `checkBuiltinRefit` — `scheduled-maintenance.ts:52`) → `scheduleDangerScan()`.

---

## 2. What v4 DELETED in the span that v5 still carries

| v4 deleted (pre lines) | Deleted by | v5 twin on `a7c6f0d03` |
|---|---|---|
| `lib/memory/housekeeping.ts` (475): `runHousekeeping` / `getHousekeepingPreview` / `needsHousekeeping`, the retention policy, the three knobs | `104e986c7` | `crates/quilltap-core/src/services/housekeeping.rs` (1,249, whole file: `PROTECTION_THRESHOLD :51`, `HousekeepingOptions :61-110`, `run_housekeeping :327`, `get_housekeeping_preview :593`, `needs_housekeeping :605`); `services/mod.rs:17,118` |
| `lib/memory/housekeeping-outcome-cache.ts` (77) | `104e986c7` | `services/housekeeping_outcome_cache.rs` (102); `services/mod.rs:119` |
| `lib/memory/consolidation-triggers.ts` (198) | `104e986c7` | `services/consolidation_triggers.rs` (458: debounce 10 min `:53`, throttle 6 h `:57`, `SCHEDULED_MIN_UNCONSIDERED = 2 :61`, the R-B mark log `:65-94`, `maybe_enqueue_consolidation_for_characters :125`, `check_consolidation_watermark_after_job[_at] :261/:266`, `run_scheduled_consolidation :335`); `services/mod.rs:77` |
| `lib/background-jobs/scheduled-housekeeping.ts` (195): per-USER `autoHousekeepingSettings.enabled` sweep, consolidation-first + the 30-min deferral | `104e986c7` | `services/queue_service.rs:1394-1494` (`HOUSEKEEPING_AFTER_CONSOLIDATION_DELAY_MS`, `run_scheduled_housekeeping`); host `quilltap-host/src/host.rs:2433-2482` (`should_run_startup_housekeeping`, `housekeeping_loop`), `:900-916` spawn, config field `housekeeping_interval_ms :110,:160,:277,:443` |
| `handlers/memory-housekeeping.ts` (143) | `104e986c7` | `quilltap-host/src/spine.rs:2929-3101` (`MemoryHousekeepingHandler`), imports `:149-150`, registration `:3918-3921` |
| `handlers/memory-consolidation.ts` (101) | `104e986c7` | `services/memory_consolidation_job.rs` (197); `quilltap-host/src/memory_consolidation.rs` (133); registration `spine.rs:3988-3995` |
| `queue-service.ts` `MemoryHousekeepingPayload` / `enqueueMemoryHousekeeping[Sweep]` / `MemoryConsolidationPayload` / `enqueueMemoryConsolidation` (pre `:296-708`) | `104e986c7` | `services/queue_service.rs:120-176` (`enqueue_memory_housekeeping[_at]`), `:178-296` (`MemoryConsolidationPayload`, `enqueue_memory_consolidation`) |
| `admission/after-insert.ts` (108, `maybeEnqueueHousekeeping`) — created by Phase C, deleted by F; its pre-span twin was `memory-gate.ts`'s watermark | `104e986c7` | `services/memory_gate.rs:1445-1545` (`HOUSEKEEPING_WATERMARK 0.9`, `WATERMARK_SWEEP_THROTTLE_MS 15 min`, `maybe_enqueue_housekeeping`), callers `:336`, `:350` |
| `lib/tools/memory-dedup.ts` (469) + `system/tools` `memory-dedup-preview` / `memory-dedup` (route −74) | `104e986c7` | `services/memory_dedup.rs` (1,117); `api/memory_maintenance.rs:21,32-60` (the dedup half — `conversation_summaries_regenerate` stays); `api/engine.rs:3190-3218`; `api/types.rs:1279-1296` (`MemoryDedupPreview` / `MemoryDedupRun`); `quilltap-web/src/system_data_routes.rs:7-8, :79, :95, :227-245 (dispatch_memory_maintenance), :373-386, :558-572`; `services/mod.rs:138` |
| `consolidation.ts` `runConsolidation` loader + announce + in-run mirror (pre `:524-…`) | `104e986c7` | `services/consolidation.rs:106,111` (constants), `:1204-1370` (holder read / skips / load / bucket / cap in `run_consolidation`), `:1099-1192` (`mirror_touched_buckets`) + its call `:1808`, `ConsolidationStats.mirror_files_written :304` |
| API `housekeep` (GET preview + POST), `housekeep-sweep`, `housekeeping-config` (GET/POST), `consolidate` | `b045dd986` (the alias retirement; `e0ac7493a` / `104e986c7` had re-pointed them) | `api/memories.rs:963-1356` (`hk_details`, `memory_housekeep_preview :985`, `validate_housekeep_options :1048`, `memory_housekeep :1106`, `memory_housekeep_sweep :1168`, `housekeeping_config_default :1180`, `memory_housekeeping_config_get :1191`, `validate_housekeeping_config :1228`, `memory_housekeeping_config_set :1302`; NOT `validate_extraction_limits_config :1271-1300`), imports `:37`, `:46`; `api/memory_consolidation.rs:139-289` (`ConsolidateOptions`, `parse_consolidate_options`, `memory_consolidate`); `api/engine.rs:3834-3871`, `:5614-5617`; `api/types.rs:1688-1712`, `:2874-2880` |
| `job-topics.ts` `MEMORY_HOUSEKEEPING` / `MEMORY_CONSOLIDATION` arms | `104e986c7` | `realtime/job_topics.rs:109-127` |
| `activity-kinds.ts` the two rows | `104e986c7` | `services/activity_kinds.rs:86-88` |
| `BackgroundJobTypeEnum` the two values | `104e986c7` | `services/job_runner.rs:142-168` `KNOWN_JOB_TYPES` (`:158`, `:159-161`); `api/system_data.rs:635-662` `JOB_TYPES` (25 values, the enqueue gate) |
| dispatcher `JOBS_INVALIDATING_FROZEN_ARCHIVE` the two types; the user-wide `invalidateAllFrozenArchives` arm | `104e986c7` | `services/frozen_archive.rs:90`, `:293-312` (`invalidate_frozen_archives_for_completed_job`) |
| `host-rpc` `writeCommonplaceDigestsToVault` | `104e986c7` | none (v5 never had the child) |

v5 has no twin for `child-repositories-proxy.ts` `setJobResult`, `ParentMemoryIndexPatchMessage`, `notifyChild('vectorStore')`, `processSingleton` — all child / Next.js transport, **NO-PORT**.

---

## 3. Behaviour deltas a port must hit (numbered; v4 pin site → v5 site)

1. **ONE job type.** `MEMORY_MAINTENANCE` replaces `MEMORY_HOUSEKEEPING` + `MEMORY_CONSOLIDATION` (`job.types.ts:35`) → `job_runner.rs:142-168` (`KNOWN_JOB_TYPES` loses `INTER_CHARACTER_MEMORY` too — Phase D's slice — and the two; gains `MEMORY_MAINTENANCE` at `:158`'s slot), `api/system_data.rs:637-662` (`JOB_TYPES` 25 → 23, same order), `activity_kinds.rs:86-88`, `job_topics.rs:109-127` (ONE arm, both hints, no dryRun test), `frozen_archive.rs:90` (`["MEMORY_MAINTENANCE"]`).
2. **The job-row migration** `drop-housekeeping-consolidation-jobs-v1` (§1.21) as a boot ensure (DELETE both retired types, any status; INFO line; no DDL) — differentially compared against v4's real `MigrationRunner`, added to `restore_vintage_state`'s boot-align (memory note "Vintage restore mirror-boot list"). Without it a v4-written `MEMORY_HOUSEKEEPING` row would fail v5's claim with the "No handler registered" loud fallback (`job_runner.rs:194-200`) once the type leaves `KNOWN_JOB_TYPES`.
3. **The five phases, fixed order, `resolvePhases`** (`run.ts:77-104`) — the four skip reasons + `nothing-touched`; `consolidate` gated ONLY by `consolidation.enabled`; `mirror` never by settings; `retire` needs `coldRetentionDays > 0` even when exempt. v5 site: NEW `services/memory_maintenance/run.rs`. Replaces v5's two gates (`spine.rs:2955-2966` "automatic triggers bail when auto-housekeeping is off", `memory_consolidation_job.rs:41-…` "automatic triggers bail when consolidation is disabled").
4. **Per-phase exemption** (`exemptPhasesFor`, `run.ts:62-68`; `exemptPhasesOf`, `queue-service.ts:646-650`) — a manual request exempts what it names (all when none); automatic exempts nothing; merged jobs union exemptions per phase.
5. **ONE corpus read** for every phase (`MaintenanceCorpus`, `corpus.ts:64-119`), kept in step by each phase's plan in BOTH modes (dry = live counts, invariant 10). v5 today reads twice (`housekeeping.rs` via `find_by_character_id_in_batches`, `consolidation.rs:1296`).
6. **Fold near-duplicates** (`fold-near-duplicates.ts`) replaces the dedup tool + housekeeping's retired merge: hot AUTO with a vector only; same subject / kind / dimension; date guard 7 d; cosine ≥ 0.90 neural / 0.80 `BUILTIN`; survivor-first by protection; ≤ 2000 rows per bucket (newest); writes through admission's `foldInto` + `applyFold` under the per-character admission lock, then `retireMemories(…, 'near-duplicate-fold')`; deadline at 40 % of the budget. v5 site: NEW `memory_maintenance/fold_near_duplicates.rs`; replaces `services/memory_dedup.rs` (union-find, 0.80 default, `[MemoryDedup]` lines all gone) and `memory_merge.rs`'s dedup use (P4.D266 — Phase C's `foldInto` supersedes `planMemoryMerge`).
7. **Consolidation loader moved** (`maintenance/consolidate.ts:62-134`) → `consolidation.rs:1258-1370` splits: the holder read / skips go to the run; the load / bucket / cap become `bucket_candidates` over the corpus; `run_consolidation` becomes `consolidate(input, opts)`.
8. **Bug 211 — demoted rows re-enter consolidation**: cold AUTO with `supersededById` NULL is a candidate (`demotedCandidates`) — v5 `consolidation.rs:1335-1337` skips every cold row today.
9. **`ConsolidationStats`** gains `demotedCandidates` (after `candidates`), loses `mirrorFilesWritten` (`consolidation.ts:148-177` vs v5 `consolidation.rs:280-343`); `skippedReason` narrows to `'no-llm'` (v5 `consolidation.rs:1258-1283` emits `character-not-found` / `archived` + two INFO lines that are now the maintenance run's `'Maintenance skipped'`).
10. **`tfidfClusterThreshold`** (0.30) — a new `memoryConsolidation` key between `clusterThreshold` and `minClusterSize` (stored JSON key order moves); `resolveClusterThreshold` picks it when the default embedding profile is `BUILTIN`, an explicit run override wins; the REPORT's `settings.clusterThreshold` carries the resolved value; `'Consolidation starting'` gains `embeddingProvider`. v5: `db/instance_settings.rs:690-830` (`MemoryConsolidationSettings` + `Input` + `to_json` + parse `:945`), `services/consolidation.rs:614-646` (`merge_settings` / `run_settings`), `:1288-1300` (the DEBUG).
11. **Bug 246** — `max(4000, resolveMaxTokens(routed profile))` per cluster call + `maxTokens` on `'[Consolidation] Sending cluster to model'` + the empty-answer reason before the JSON parse. v5: `consolidation_tasks.rs:59` (`CONSOLIDATION_MAX_TOKENS` → `CONSOLIDATION_MIN_MAX_TOKENS`), `:619-634` (`parse_consolidation_response` trim check), `:638-673` (`consolidate_memory_cluster` gains `profile_max_tokens: Option<i64>`), `services/consolidation.rs:1583` (the call: look up `selection.connection_profile_id` in the route's profiles; `context_budget::resolve_max_tokens(profile.max_tokens, profile.model_class)`).
12. **No in-run announce / mirror in `consolidate`** — the digests' frozen-archive invalidation, `memories` / `mountPoints` hints and the `Commonplace/` mirror are the maintenance run's / commit hook's (`run.ts:171-186, :355-369`; `commonplace-mirror.ts:111-228`). v5 `consolidation.rs:1099-1192, :1808` + R-C's `in_job` flag (`:411-414`) retire.
13. **Demote = exactly `demotable − cap`, lowest protection first** (`demote.ts:33-57`) — the cap counts hot AUTO only (bug 210); MANUAL / digests reported as `protected`; NOT the old retention pass + tail cap of `housekeeping.rs:207-…`. The writes are `memoryIndex.setTier(characterId, ids, 'cold')`.
14. **Retire** (`demote.ts:60-70`, `run.ts:342-352`) — cold superseded AUTO by `supersededAt ?? updatedAt` past `coldRetentionDays` through `retireMemories(…, 'retention')` (v5 `housekeeping.rs` retention sweep by `delete_many_with_unlink` + vector-store cleanup, non-MANUAL — pin narrows to AUTO, bug 225).
15. **The three retention knobs are gone** — no `maxAgeMonths` / `maxInactiveMonths` / `minImportance` anywhere (v5 `HousekeepingOptions`, `MemoryHousekeepPreview` request fields, `validate_housekeep_options`).
16. **The parent-side ledger** (`ledger.ts`) replaces the outcome cache AND consolidation's debounce map AND the durable `find_recent_by_type` throttles (v5 `memory_gate.rs:1504-1537`, `consolidation_triggers.rs:96-122`): cap watermark at **0.9** of the cap over **demotable** rows (v5 counts all hot rows, `memory_gate.rs:1497`), 15-min demote throttle, 1-h ineffective back-off (`max(10, 1 % of excess)`, a no-excess run counts as ineffective), 6-h consolidate throttle, **2-min** watermark debounce (v5 consolidation's is 10 min), seeded once from the last 100 `MEMORY_MAINTENANCE` rows.
17. **The watermark** asks for SUBSETS (`fold, demote, retire` and/or `consolidate, mirror`) through ONE `maybeEnqueueMaintenance`; its counts are `countCapBreakdown(id).demotable` and `countUnconsideredHot(id)` (the latter only when consolidation is on). v5 needs `count_cap_breakdown` (absent — `memories_read.rs:992` has only `count_hot_by_character_id`; c2795733f's repository slice) and has `count_unconsidered_hot :1005`.
18. **The trigger call sites**: after every admission INSERT (`admit.ts:405-412`) → v5's admission port (today `memory_gate.rs:336,:350`); and the commit hook (`job-dispatcher.ts:260`) → v5 `job_runner.rs:502-506`'s `check_consolidation_watermark_after_job` region call.
19. **⚑ The commit-hook watermark is unreachable at the pin** (§1.10 finding) — v5's R-B `WATERMARK_ROWID_WINDOW` machinery (`consolidation_triggers.rs:19-47, :65-94, :219-318`) existed to find "characters a job wrote" without a write list; with admission calling the watermark per insert, a faithful port of `maybeEnqueueMaintenanceAfterCommit` is "no reachable job type yields a character". Measure (§5.4), then retire the ruled table (§9 P1).
20. **The scheduler** (`scheduler.ts`) — per CHARACTER, every live holder of memories, gated by `maintenance.enabled || consolidation.enabled` (instance-wide), batched per owner; the startup tick reads `MEMORY_MAINTENANCE` rows (`reason === 'scheduled'`, 20 h). v5: `queue_service.rs:1394-1494` (per-USER, `chat_settings.autoHousekeepingSettings.enabled`, consolidation first + 30-min deferral, `SCHEDULED_MIN_UNCONSIDERED`) and `host.rs:2433-2482` (`MEMORY_HOUSEKEEPING` peek).
21. **Enqueue = per-character dedupe against PENDING only + WIDEN** (`queue-service.ts:653-703`): union phases (all absorbs), manual reason wins, exemptions unioned, settings shallow-merged; a PROCESSING job never absorbs; `maxAttempts 1`. v5 dedupes against PENDING + PROCESSING and returns the old id unchanged (`queue_service.rs:136-176`, `:202-296`; consolidation's dryRun-flag key is gone with `dryRun`).
22. **The handler defers** behind an OLDER PROCESSING run for the same character (+2 min re-enqueue of its own payload) — v5 has no such check.
23. **Time budget** 7 min per job; fold ≤ 40 %, consolidation the rest (`run.ts:292, :313`) — v5 `memory_consolidation_job.rs:36` budget applies to consolidation only.
24. **The summary + ledger** — the handler's `MaintenanceSummary` reaches the ledger after commit (`job-dispatcher.ts:263, :691-704`), never persisted on the row. v5's `JobOutcome::Completed(Some(v))` would MERGE into `payload.result` (`job_runner.rs:96-100`, `background_jobs.rs:472-…`) — the handler must answer `Completed(None)` and hand the summary over another way (§7.3).
25. **Mirror after commit** — in the job the run reports `{subjects, filesWritten: 0, deferredToCommit: true}` and the commit hook mirrors from committed rows; in-process (`maintain` is dry-run-only in-process, so only tests / a future CLI) the run mirrors itself. The bridge now also REMOVES an emptied bucket's file (`commonplace-digest-vault-bridge.ts`, `removed` count) — the deletion slice / bridge owner (`services/commonplace_digest_vault_bridge.rs`) — and refuses in the child with `skippedReason 'job-child'` (NO-PORT arm).
26. **Frozen-archive hook**: only `MEMORY_MAINTENANCE`; a payload with no / empty `characterId` invalidates NOTHING (v5 `frozen_archive.rs:300-303` invalidates all and logs `characterId=all`).
27. **API: `maintain`** (§1.20) — new action, `withImpliedPhases`, the dry-run character rule, the 10 / 40 / 120 s bounds, `{jobId, jobIds}`; **`maintenance-status`**; **`repair`**; **`maintenance-config`** (merge-toggles, replace-the-rest); the retired actions answer v4's `Unknown action` 400 with the pin's `availableActions` lists.
28. **Bug 244** (`patchSchemaOf`) — consolidation-config / extraction-mode-config POST keep stored values for omitted keys and the INFO's `fields` lists only sent keys; `maintain`'s `settings.consolidation` overrides only what it names. v5 `api/memory_consolidation.rs:72-133` ports the clobber (R-A) — reverse it; `memory_consolidation_routes_equivalence.rs:355-396` trips (§5).
29. **`maxPerHour` bound** `.max(10_000)` on `MemoryExtractionLimitsSchema` (`settings.types.ts:319`) — extraction-limits-config's validator (`api/memories.rs:1271-1300`); flagged here because Phase G shipped it with the maintenance API; the memories-API slice may own it.
30. **System Tools**: GET `memory-dedup-preview` / POST `memory-dedup` are unknown actions at the pin (400 listing `system/tools/route.ts:1161-1184`'s maps) → v5 `system_data_routes.rs:70-106` lists lose the two entries; the two arms go.
31. **Settings source**: maintenance reads `instance_settings.memoryMaintenance` (instance-wide) via the façade — never `chat_settings.autoHousekeepingSettings` (`spine.rs:2944-2950`, `memory_gate.rs:1476-1490`, `queue_service.rs:1450-1460`); the move itself (`move-memory-maintenance-settings-v1`) is the keystone's.

---

## 4. v5 on `main` today — per file in the slice

- **`crates/quilltap-core/src/services/housekeeping.rs`** (1,249) — v4 `housekeeping.ts` at `01a83539d`: retention policy + retired merge + cap tail + retention sweep, `to_fixed` reasons, `CountSource`. **DIES whole** (tests `:780-1249` with it). Reusable: nothing structural; `memory_weighting.rs:125` `calculate_protection_score` stays (scoring slice re-ports `protection()`).
- **`services/housekeeping_outcome_cache.rs`** (102) — **DIES**; its `OnceLock<Mutex<HashMap>>` shape is the ledger's model.
- **`services/consolidation_triggers.rs`** (458) — **DIES** (the module becomes `memory_maintenance/triggers.rs` + `scheduler.rs`); R-B's mark log and `characters_inserted_since` / `max_memory_rowid` (`:235-259`) retire with §9 P1.
- **`services/memory_consolidation_job.rs`** (197) — **DIES**, replaced by `services/memory_maintenance_job.rs`.
- **`services/consolidation.rs`** (1,869) — **SHRINKS**: constants `:106,:111` move / die; `ConsolidationTrigger :120-146` becomes `MaintenanceReason` (or is aliased — §7); `ConsolidationStats :280-343` (−`mirror_files_written`, +`demoted_candidates`); `ConsolidationReport :345-387` (`skipped_reason` narrows); `RunConsolidationOptions :389-430` (−`in_job`, keeps `elapsed`, `now_ms`); the R-E runner slot `:432-475` generalises to the maintenance runner; `mirror_touched_buckets :1099-1192` moves to the mirror phase / hook; `run_consolidation :1204-1869` loses `:1237-1370` (settings → holder → load → bucket → cap) and `:1800-1815` (mirror call) and takes `ConsolidationInput`. `resolve_model_route :647`, `route_for_cluster :705`, `describe_buckets :775`, `execute_plan :861` stay (execute_plan's writes move onto the Phase-B index seam — index slice).
- **`consolidation_tasks.rs`** (698) — `:59` budget const, `:619-634` parse, `:638-673` call (Δ11). `consolidation_tasks/prompt_text.rs` unchanged. `consolidation_clustering.rs` (640) unchanged; `consolidation_plan.rs` (933) — `normalizeDigestKeywords` now routes through `candidate.ts`'s `splitTargetingKeywords` / `normaliseTargeting` / `targetingKeywords` (`consolidation-plan.ts:200-221`) — the candidate slice's NEED; behaviour equal only if the shared normaliser matches the old inline one (measure).
- **`services/memory_dedup.rs`** (1,117), **`api/memory_maintenance.rs:1-60`** (dedup half; the summaries-regen half `:61-96` stays — rename the module's header) — **DIE**.
- **`services/queue_service.rs`** (2,171) — `:120-176`, `:178-296`, `:1394-1494` die; NEW `enqueue_memory_maintenance`, `enqueue_memory_maintenance_many`, `enqueue_memory_maintenance_for_user` (+ widen helpers). `enqueue_job_at :80` (`scheduled_at`) is reused. Tests around `:1802` (`seed_job(… "MEMORY_HOUSEKEEPING" …)`) re-seed.
- **`services/memory_gate.rs:1445-1545`** — `maybe_enqueue_housekeeping` (admission slice rewrites the file; its replacement CALLS `memory_maintenance::triggers::maybe_enqueue_maintenance(db, &[character_id], now)`).
- **`services/job_runner.rs`** — `KNOWN_JOB_TYPES :142-168`; the completion region `:487-507` (one call replaced / one added — §7.3); tests `:824`, `:963-971`, `:1192` name the old types.
- **`services/frozen_archive.rs:90, :293-312`** + its tests `:961-1117` (the recall slice owns the file; the constant + the empty-id arm are this slice's deltas).
- **`services/activity_kinds.rs:80-90`**, **`realtime/job_topics.rs:109-127`**, **`api/system_data.rs:635-662`**, **`realtime/publish_sites.rs:271-451`** (tests enqueue `MEMORY_HOUSEKEEPING` as a sample type — re-seed with a live type).
- **`db/instance_settings.rs`** — `KEY_MEMORY_CONSOLIDATION :680`, `MemoryConsolidationSettings :690-765` (+`tfidf_cluster_threshold`), `…Input :768-830`, `parse_memory_consolidation_settings :945`, getter / setter `:1130-1160`; NO `memoryMaintenance` key today (keystone adds `KEY_MEMORY_MAINTENANCE` + `MemoryMaintenanceSettings` — §7.1).
- **`db/chat_settings.rs`** — `find_auto_housekeeping_settings_by_user_id`, `find_all_scheduler_settings`'s `auto_housekeeping_settings` (keystone retires the columns; this slice drops its three readers: `spine.rs:2944`, `memory_gate.rs:1476`, `queue_service.rs:1450`).
- **`api/memories.rs`** — `:37`, `:46` imports; `:963-1356` (minus `:1271-1300`); `memory_character_counts :418-481` (bug 210 shape — memories-API slice); NEW handlers for `maintain` / `maintenance-status` / `repair` / `maintenance-config` (or a NEW `api/memory_maintenance.rs` — §7.4).
- **`api/memory_consolidation.rs`** (289) — `:1-24` header (R-A text) rewritten; `:72-97` / `:111-133` config sets move to patch semantics; `:139-289` (`consolidate`) DIES.
- **`api/types.rs`** — `:1279-1296`, `:1688-1712`, `:2861-2880` (variants), `:4545-4551` (`Response::MemoryMaintenance` doc names dedup); **`api/engine.rs`** — `:3190-3218`, `:3834-3871`, `:5598-5617`.
- **`crates/quilltap-host/src/spine.rs`** — `:149-150`, `:2929-3101`, `:3918-3921`, `:3988-3995`; **`quilltap-host/src/memory_consolidation.rs`** (133) → `memory_maintenance.rs` (the runner over `WireConfig`, installs core's process-global runner — R-E stands, renamed); **`quilltap-host/src/host.rs`** — `:61` imports, `:110,:160,:277,:443,:913` `housekeeping_interval_ms` (rename `memory_maintenance_interval_ms`), `:900-916` spawn, `:2433-2482` loop + startup peek; tests `quilltap-host/tests/memory_housekeeping_handler.rs` (DIES), `host_cadence.rs:232` (field rename).
- **`crates/quilltap-web/src/system_data_routes.rs`** — §2 row; tests `system_body_guards_equivalence.rs:344` (`MEMORY_HOUSEKEEPING` as the jobs-route sample), `realtime_hint_wire.rs:107`, `dispatch_wrong_type_census.rs:920-927, :1090-1111` (variants).

---

## 5. Harness — families, predicted fate at the pin, what to add

| Family (tier) | v4 import / dependency | Fate at `64743ee13` |
|---|---|---|
| `memory_housekeeping_tier2_equivalence` (2) + `cases/memory-housekeeping-tier2.ts` + `fixtures/build-memory-housekeeping-fixture.ts` | `@/lib/memory/housekeeping`; builder imports `vector-indices.repository` + `UPDATE vector_entries` | **IMPORT-DEAD** (module gone; builder dead with Phase B) — RETIRE |
| `memory_watermark_tier3_equivalence` (3) + `cases/memory-watermark-tier3.test.ts` + `build-memory-watermark-fixture.ts` | `@/lib/memory/housekeeping-outcome-cache`, `@/lib/memory/memory-service` | **IMPORT-DEAD** — RETIRE (its question moves to the new queue family) |
| `consolidation_triggers_tier2_equivalence` (2) + `cases/consolidation-triggers-tier2.test.ts` + `fixtures/consolidation-triggers-steps.json` | `@/lib/memory/consolidation-triggers`, `@/lib/background-jobs/scheduled-housekeeping`, `@/lib/background-jobs/handlers/memory-consolidation`, `enqueueMemoryConsolidation` | **IMPORT-DEAD** — RETIRE; **copy its step-runner recipe** (ONE copy, ordered steps, captured child loggers, final `background_jobs` rows with the `scheduledAt − createdAt` class) and its **"as of `spec.now`" seam** (the 2026-10-13 calendar time bomb: the runner fills an omitted `now`; `MaintenanceContext.now` is v4's own seam — use it on both sides) |
| `consolidation_service_tier3_equivalence` (3) + `cases/consolidation-service-tier3.test.ts` + `consolidation-corpus.ts` / `-answers.ts` / `consolidation-tier3.json` / `consolidation-service-runs.json` + `build-consolidation-fixture.ts` | named export `runConsolidation` (GONE — `consolidate` takes bucketed input); builder imports `vector-indices.repository` / `.types` | **IMPORT-DEAD** (TypeError at the call; builder dead) — RESHAPE into the maintenance consolidate tier-3 (below); rebuild the fixture through v4's real migrations at the pin (memory note "Rebuilt fixture pair vintage") |
| `memory_dedup_equivalence` (2) + `cases/memory-dedup.test.ts` + `memory-dedup-plants.json` | `@/lib/tools/memory-dedup`, `memory-gate`, `memory-merge` | **IMPORT-DEAD** — RETIRE |
| `memory_merge_plan_equivalence` (1) + `cases/memory-merge-plan.ts` | `memory-merge`, `memory-gate` | **IMPORT-DEAD** — the admission slice's (`foldInto` replaces it) |
| `memory_consolidation_routes_equivalence` (2) + `cases/memory-consolidation-routes.test.ts` + `memory-consolidation-routes.json` (fixture `quilltap-web/tests/fixtures/memories-{main,mount}.db`) | route module present | **RED** — the `consolidate` rows now 400 `Unknown action`; every config POST's INFO `fields` shrinks to the sent keys; **`PARTIAL_DEFAULTS_CLOBBER`'s three explicit rows trip by design** (`:355-396`); the fixture pair needs widening to the pin's columns |
| `memories_routes_equivalence` (2) (`cases/memories-config.test.ts` `:328-470`) | route present | **RED** rows `housekeep_*`, `housekeeping_config_*` (unknown action) — the memories-API slice reshapes; maintenance supplies the replacement rows |
| `consolidation_tasks_equivalence` (1) + `cases/consolidation-tasks.ts` | exports present | **RED** — the `''` and `'   '` parse rows (`:217-218`) now answer `CONSOLIDATION_EMPTY_ANSWER_REASON`; ADD a `maxTokens` row only if the case drives the call (it does not — tier-1 over the pure functions) |
| `consolidation_clustering_equivalence` (1) | unchanged module (comment only) | **GREEN** predicted |
| `consolidation_plan_equivalence` (1) | `consolidation-plan.ts` now imports `./scoring`, `./recall/tags`, `./candidate` | **GREEN or RED** — `normalizeDigestKeywords` delegates to the candidate normaliser; measure the keyword rows (`cases/consolidation-plan.ts:56,95,228,325-328`) |
| `memory_settings_schemas_equivalence` (1) + `cases/memory-settings-schemas.ts` | `MemoryConsolidationSettingsSchema` | **RED** — every parsed object gains `tfidfClusterThreshold`; ADD `MemoryMaintenanceSettingsSchema` + `MemoryMaintenancePhaseTogglesSchema` + `patchSchemaOf` rows (keystone may own the file) |
| `activity_tables_equivalence` (1) + `cases/activity-tables.ts` | `activity-kinds.ts`, `BackgroundJobTypeEnum` | **RED** (three rows gone, one added) |
| `realtime_topics_equivalence` (1) + `cases/realtime-topics.ts:69-80` | `job-topics.ts` | **RED** (the consolidation dryRun rows; ADD `MEMORY_MAINTENANCE` rows incl. `dryRun: true` → still both hints) |
| `system_jobs_collection_equivalence` (2) + `cases/system-jobs-collection.test.ts:200-235` | `MEMORY_HOUSEKEEPING` as the sample job type | **RED** (the type now fails the enum) — swap the sample type |
| `system_body_guards_equivalence` (quilltap-web) + `cases/system-body-guards.test.ts:110-125, :265-275` | `jest.doMock('@/lib/tools/memory-dedup', …)` with no `{virtual: true}` | **IMPORT-DEAD** (doMock of an unresolvable path throws) — drop the mock + the four `dedup_*` rows (or keep them as `Unknown action` rows) and swap `jobs_payload_null`'s type |
| `frozen_archive_tier2_equivalence` (2) + `cases/frozen-archive-tier2.test.ts` | `frozen-archive-cache` (renamed `recall/archive`) | **IMPORT-DEAD** — the recall slice's; its hook rows (`:33-34` housekeeping / consolidation / user-wide) re-aim at `MEMORY_MAINTENANCE` + the empty-id no-op |
| `maintenance_sweep_tier2` / `maintenance_ops_tier2` | `scheduled-maintenance.ts` (DATA maintenance) | GREEN unless `checkBuiltinRefit` (4ae0c4ca6) moves its rows — the TF-IDF slice |

**Families to ADD** (C1 names; each with a `*_equivalence.rs` + a case + a recipe header — copy the nearest):

1. `memory_maintenance_planning_equivalence` (tier 1, jest — the modules import repository factories): v4's REAL `resolvePhases` (every reason × exemption × switch × toggle × `coldRetentionDays` cell), `withImpliedPhases`, `exemptPhasesFor`, `planDemotion` (ties, NaN `createdAt`, excess 0, MANUAL / digest / cold rows), `planRetention` (`supersededAt` vs `updatedAt` fallback, non-finite), `planNearDuplicateFolds` (non-transitive chains, date guard, dim buckets, empty-string `aboutCharacterId`, the 2000-row bucket cap, both threshold sets), `bucketCandidates` (bug 211 rows, cap ordering), `summarizeMaintenance` (key orders), `shouldEnqueue` + `lastDemotionIneffective` (seed rows planted through a mocked `findRecentByType`), `resolveClusterThreshold`, `parseConsolidationResponse('')`. Copy: `consolidation_plan_equivalence.rs` + `cases/consolidation-plan.ts`.
2. `memory_maintenance_tier2_equivalence` (tier 2 DB-state, the LLM-free phases): `runMaintenance` over a fixture for fold / demote / retire (+ a phases-all run with consolidation disabled), dry vs live (invariant 10: the dry report's later-phase counts equal the live one's), DB-state diff of `memories` (tier, `supersededById`, retired rows, survivor `content` / `reinforcementCount` / `relatedMemoryIds`), the reports (time fields normalised), captured `memory:maintenance*` lines. Copy: `memory_housekeeping_tier2_equivalence.rs`'s fixture recipe, with a builder rebuilt WITHOUT `vector-indices` (embeddings on `memories.embedding`, Float32 LE BLOBs — memory note "Memory search plants need vectors").
3. `memory_maintenance_consolidate_tier3_equivalence` (tier 3 mocked LLM): `runMaintenance({phases: ['consolidate', 'mirror'], …})` over the rebuilt consolidation fixture with the canned answers — the reshaped `consolidation_service_tier3`; ADD bug-246 rows (a routed profile with `maxTokens` 12000 → request `max_tokens` 12000; one with none → 8000 via `DEFAULT_MAX_TOKENS`; an unknown selection → 4000) and the empty-answer row, a `BUILTIN` default profile row (`tfidfClusterThreshold`), a demoted-candidate row (bug 211). Copy: `consolidation_service_tier3_equivalence.rs`.
4. `memory_maintenance_queue_tier2_equivalence` (tier 2 step runner): `enqueueMemoryMaintenance` / `…Many` / `…ForUser` (widen, exempt union, PROCESSING never absorbs, payload bytes), `maybeEnqueueMaintenance` (debounce, both watermarks, throttles), `runScheduledMemoryMaintenance` + `runStartupTick`, `handleMemoryMaintenance` (no characterId, defer behind an older PROCESSING run with the +2 min class, a full run), the ledger after each step (`getMaintenanceLedgerStatus`). Copy: `consolidation_triggers_tier2_equivalence.rs` (its `CannedRunner` "as of spec.now" fill).
5. `memory_maintenance_routes_equivalence` (tier 2 routes): `maintain` (dry one / dry all LLM-free / dry all refused / live one / live all / bad phase / 404), `maintenance-status` (one / all / 404), `repair` (dry / live / 404 / bad body), `maintenance-config` GET / POST (toggle merge, overrides replace, bounds 100 / 100000), `consolidation-config` / `extraction-mode-config` with the bug-244 rows, the retired actions' 400 bodies. Reshape `memory_consolidation_routes_equivalence.rs` into it.
6. `memory_graph_repair_tier2_equivalence` (tier 2) — `findDanglingLinks` + `repairMemoryGraph` over planted dangling links (all three columns, cross-holder, scoped, all-members-gone digest → `graph-repair` retirement, missing embeddings → jobs queued). Could ride the retire / deletion slice if that lane owns `scrubDanglingMemoryLinks`.
7. A boot-ensure differential for `drop-housekeeping-consolidation-jobs-v1` (tier 2) — v4's REAL `MigrationRunner` (memory note "Oracle runner + SKIP traps") vs the v5 ensure over planted rows of both types × statuses.

**Retiring `PARTIAL_DEFAULTS_CLOBBER` by measurement (§5.4):** regenerate `memory-consolidation-routes` from the PIN worktree (`npm ci --offline` in the pin — memory note "pin node_modules drift"), dump `storedAfter` + the INFO `fields` for the three clobber rows with a `QT_DEBUG_*`-gated `eprintln!`; EXPECTED at the pin: `clobber_cold_retention` stores `{enabled: true, clusterThreshold: 0.5, tfidfClusterThreshold: 0.3, …, watermark: 300, coldRetentionDays: 90}` with `fields ["coldRetentionDays"]`; `clobber_null_cold_retention` keeps `enabled: true`, `coldRetentionDays: null`; `clobber_fold_candidates` keeps `otherPass: 'fold'`, `perTurnOtherFloor: 0.5`, `foldCandidatesPerSubject: 5`. If v4's rows match that (full convergence), replace the table with a positive both-ways pin (name it `PATCH_PRESERVES_STORED` or fold it into plain equality), retire the `api/memory_consolidation.rs:8-17` header and the SPA's `memory-consolidation-card.ts:78` comment; if any key still resets, split the table (a converged half + a still-divergent half). Record the measurement in the ledger row when the baseline moves.

---

## 6. v4 jest tests at the pin covering the slice (cases)

- `__tests__/unit/lib/memory/maintenance/run-maintenance.test.ts` — **18**: phase order + skipping (5 order / switches / toggles / merged exemption / retention window), mirror `nothing-touched`, archived holder, demote (exactly demotable − cap; protected-only fits; bug 211 re-entry), retire by `supersededAt` AUTO-only, fold (subject / occasion; profile scale TF-IDF; bounded bucket), dry = live (invariant 10), mirror after commit (parent writes once; child writes nothing; dry run nothing).
- `__tests__/unit/lib/memory/maintenance/ledger-and-triggers.test.ts` — **21**: `shouldEnqueue` (9), `maybeEnqueueMaintenance` (3, incl. "the commit hook looks at created memories, never at a maintenance job's own batch"), scheduler (2), lifecycle (1: stop cancels the startup tick), handler (6: summary handed back, hand-asked phases, defer behind an older run, another character / younger run, the test flag both ways).
- `__tests__/unit/background-jobs/memory-maintenance-enqueue.test.ts` — **9** (bug 221 fan-out; collisions; widening + exemption; PROCESSING never absorbs; bug 223 threshold carried).
- `__tests__/unit/api/memories-consolidation-actions.test.ts` — **21** (maintain's consolidate phase incl. the 40-cluster cap and "the retired alias actions are gone"; config round-trips incl. **bug 244 one-field write**; list tier / source filters; character-memory-counts; maintain; maintenance-status; repair).
- `__tests__/unit/lib/memory/memory-graph-repair.test.ts` — **6**; `lib/schemas/__tests__/patch-schema.test.ts` — **3**; `__tests__/unit/lib/memory/consolidation-provider-threshold.test.ts` — **3** (TF-IDF / neural / explicit override).
- `lib/memory/__tests__/consolidation-service.test.ts` — **10** (+2 bug 246: routed budget; unknown profile leaves the floor); `lib/memory/cheap-llm-tasks/__tests__/consolidation-tasks.test.ts` — **17** (+3 bug 246: empty answer; profile budget above the floor; never below); `consolidation-clustering.test.ts` 17, `consolidation-plan.test.ts` 18.
- `lib/background-jobs/host/__tests__/job-dispatcher-commonplace-mirror.test.ts` — **6**; `job-dispatcher-frozen-archive.test.ts` — **3**; `__tests__/unit/realtime/job-topics.test.ts` — **12**; `__tests__/unit/lib/memory/policy-protection.test.ts` — **11**; `memory-retention-clock.test.ts` — 5; `memory-recall-housekeeping-fixes.test.ts` — 8; `__tests__/unit/lib/background-jobs/child-proxy-memory-housekeeping.test.ts` — 4 (child-buffer semantics, NO-PORT in itself).
- SPA / CLI (pointers, §8): `components/memory/maintenance-dialog.test.tsx` 3, `components/tools/memory-maintenance-card.test.tsx` 4, `packages/quilltap/lib/__tests__/memories-maintain-command.test.js` 18.

---

## 7. Cross-lane meeting points + PROPOSED Rust spellings (C1)

### 7.1 NEEDS from the schema / settings KEYSTONE

- `instance_settings`: `KEY_MEMORY_MAINTENANCE = "memoryMaintenance"`; **proposed** `pub struct MemoryMaintenancePhaseToggles { pub fold_near_duplicates: bool, pub demote: bool, pub retire: bool }` (all `true` default) and `pub struct MemoryMaintenanceSettings { pub enabled: bool, pub per_character_cap: i64, pub per_character_cap_overrides: indexmap::IndexMap<String, i64>, pub phases: MemoryMaintenancePhaseToggles }` with `Default` = `parse({})`, `to_json` (v4 key order; `phases` nested), `parse_memory_maintenance_settings(&Value) -> Result<_, Vec<ZodIssue>>` (prefault semantics: absent / `undefined` `phases` ⇒ all-on; `null` `phases` ⇒ an issue), `get_memory_maintenance_settings(&Connection)`, `set_memory_maintenance_settings(&Connection, patch: &Value) -> Result<_, DbError>`, `pub const MEMORY_MAINTENANCE_CAP_MIN: i64 = 100; MAX = 100_000`, and `impl MemoryMaintenanceSettings { pub fn cap_for(&self, character_id: &str) -> i64 }` (= `resolveMemoryCap`). The keystone survey will also propose `MemoryMaintenanceSettings`; reconcile to ONE type (the planner's call).
- `MemoryConsolidationSettings.tfidf_cluster_threshold: f64` (0.30, between `cluster_threshold` and `min_cluster_size`) + its `Input` field + parse row; and **patch parsing** for `memoryConsolidation` / `memoryExtractionMode` / the toggles: proposed `fn parse_memory_consolidation_settings_patch(&Value) -> Result<serde_json::Map<String, Value>, Vec<ZodIssue>>` (present keys only, schema order — the `patchSchemaOf` port), or ONE generic `api::zod_issues::patch_of(schema_parser)` if the keystone ports `patchSchemaOf` generally.
- The settings façade: `memory::settings::resolve_memory_settings(db, user_id: Option<&str>) -> MemorySettings` with `maintenance: ResolvedMemoryMaintenance { settings: MemoryMaintenanceSettings, consolidation: MemoryConsolidationSettings, cold_retention_days: Option<i64> }` and its DEBUG line; `admission_thresholds_for_provider(Option<&str>) -> AdmissionThresholds`; `resolve_cluster_threshold(&MemoryConsolidationSettings, provider: Option<&str>, override_: Option<f64>) -> f64`; `is_lexical_embedding_provider(Option<&str>) -> bool`.
- Repository reads: `memories_read::count_cap_breakdown(conn, character_id) -> CapBreakdown { hot_total, demotable, protected, cold }` (bug 210 SQL, safeQuery fallback zeros); `count_unconsidered_hot :1005` (exists); `find_distinct_character_ids :1273` (exists); `find_by_character_id_in_batches` (exists); `find_hot_digests :1062` (exists); `find_dangling_links(conn, scope: Option<&[String]>) -> Result<DanglingMemoryLinks, DbError>` (NO fallback); `count_without_embedding(conn, Option<&str>)` (exists `:1137`, check the character arg); `memories.supersededAt` column (keystone D23).
- Job-row ensure: `db::retired_memory_jobs_repair::ensure_housekeeping_consolidation_jobs_dropped(conn)` (MIGRATION_ID `drop-housekeeping-consolidation-jobs-v1`) — keystone-owned if it holds every ensure; else this lane's.

### 7.2 NEEDS from sibling slices

- **Admission (Phase C):** `admission::with_admission_lock(character_id, f)` (async, per-character, the lock the fold shares), `admission::fold::fold_into(survivor, &absorbed, &FOLD_CAPS) -> FoldPlan`, `admission::apply::apply_fold(db, embedding, survivor, plan, user_id) -> Result<Option<Value>, …>` (re-embeds; returns the survivor WITH its vector), `admission::decide::occasions_are_distinct(a, b, date_guard_days) -> bool`, and the after-insert call site invoking `memory_maintenance::triggers::maybe_enqueue_maintenance`.
- **Retire (Phase B):** `retire::retire_memories(db, ids, RetireOptions { reason: RetireReason::{Retention, NearDuplicateFold, GraphRepair}, keep: None }) -> Result<RetireReport, DbError>` (`retired_ids` authoritative); `retire::scrub_dangling_memory_links(db, &DanglingMemoryLinks) -> DanglingScrubReport`.
- **Memory index (Phase B):** `memory_index::set_tier(db, character_id, &ids, Tier, extras: Option<SetTierExtras { superseded_by_id: Option<Option<String>>, consolidated_at: Option<String> }>)`, `upsert`, `index_inserted`; `memory_index::enqueue_missing_embeddings(db, user_id, character_id, limit) -> EnqueueMissing { status, enqueued }`.
- **Scoring (Phase A):** `scoring::protection(memory, now_ms) -> Protection { score, protected, … }`; `scoring::calculate_reinforced_importance` (the new `log2(max(1, count)) × 0.05` formula — Δ4 of the ledger's hazards); `policy_protection_of(memory) -> Option<PolicyProtection>`.
- **Commonplace mirror (bug 208 — retire / deletion slice or this lane; §9 P5):** `commonplace_mirror::mirror_commonplace_for_subjects(db, holder_id, &subjects, updated_at) -> usize` over `services/commonplace_digest_vault_bridge.rs` (+ the emptied-bucket removal).
- **Recall:** `frozen_archive::invalidate_frozen_archive(character_id)` (exists).

### 7.3 PROVIDES — proposed module + types (`crates/quilltap-core/src/services/memory_maintenance/`)

`services/maintenance.rs` (stale-chat assets) and `services/scheduled_maintenance.rs` (the DATA tick) already own the bare name — **use `memory_maintenance`** everywhere (tracing `module = "memory:maintenance"` etc. keeps v4's child names).

```rust
// memory_maintenance/types.rs
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum MaintenancePhase { FoldNearDuplicates, Consolidate, Demote, Retire, Mirror }
impl MaintenancePhase {
    pub const ALL: [Self; 5];                       // v4 MAINTENANCE_PHASES order
    pub fn as_str(self) -> &'static str;            // "fold-near-duplicates" …
    pub fn parse(s: &str) -> Option<Self>;
}
pub fn with_implied_phases(phases: &[MaintenancePhase]) -> Vec<MaintenancePhase>;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MaintenanceReason { Scheduled, Watermark, Manual }      // as_str / parse
pub struct MaintenanceRunSettings { pub consolidation: Option<serde_json::Map<String, Value>>, // a patch
                                    pub max_clusters_per_run: Option<f64>, pub cap: Option<i64> }
pub struct MaintenanceRequest { pub character_id: String, pub phases: Option<Vec<MaintenancePhase>>,
    pub reason: MaintenanceReason, pub exempt_phases: Option<Vec<MaintenancePhase>>,
    pub dry_run: bool, pub settings: Option<MaintenanceRunSettings> }
pub enum MaintenanceMode { InProcess, Job }          // v4 isJobChild(): Job ⇒ mirror deferred, no announce
pub struct MaintenanceContext { pub user_id: Option<String>, pub time_budget_ms: Option<i64>,
    pub now_ms: Option<i64>, pub mode: MaintenanceMode, pub elapsed: Option<ElapsedClock> }
pub enum PhaseStatus { Ran, Skipped }
pub enum PhaseSkipReason { NotRequested, MaintenanceDisabled, ConsolidationDisabled,
                           PhaseDisabled, NoRetentionWindow, NothingTouched }
pub struct PhaseOutcome { pub phase: MaintenancePhase, pub status: PhaseStatus,
    pub skipped_because: Option<PhaseSkipReason>, pub duration_ms: i64 }
pub struct NearDuplicateGroup { pub survivor_id: String, pub absorbed_ids: Vec<String>, pub min_cosine: f64 }
pub struct FoldPhaseReport { eligible, deferred, groups: Vec<NearDuplicateGroup>, folded, groups_failed, groups_deferred }
pub struct DemotePhaseReport { cap: i64, hot_total, demotable, protected, excess, demoted,
                               demoted_ids: Vec<String>, highest_score_demoted: Option<f64> }
pub struct RetirePhaseReport { retention_days: i64, retired, retired_ids: Vec<String> }
pub struct MirrorPhaseReport { subjects: Vec<String>, files_written: usize, deferred_to_commit: bool }
pub enum MaintenanceSkip { CharacterNotFound, Archived }
pub struct MaintenanceReport { character_id, character_name: Option<String>, reason, dry_run,
    started_at: String, finished_at: String, duration_ms: i64, skipped_reason: Option<MaintenanceSkip>,
    phases: Vec<PhaseOutcome>, fold: Option<FoldPhaseReport>, consolidation: Option<ConsolidationReport>,
    demote: Option<DemotePhaseReport>, retire: Option<RetirePhaseReport>, mirror: Option<MirrorPhaseReport> }
pub struct MaintenanceSummary { /* §1.4 key order */ }
// every report type: fn to_json(&self) -> Value in v4's LITERAL key order (§1.4 — not the interface order)

// memory_maintenance/run.rs
pub const FOLD_BUDGET_SHARE: f64 = 0.4;
pub fn exempt_phases_for(req: &MaintenanceRequest) -> BTreeSet<MaintenancePhase>;
pub fn resolve_phases(req: &MaintenanceRequest, s: &ResolvedMemoryMaintenance)
    -> Vec<(MaintenancePhase, Option<PhaseSkipReason>)>;
pub fn summarize_maintenance(r: &MaintenanceReport) -> MaintenanceSummary;
pub fn announce_maintenance(db: &Db, s: &MaintenanceSummary, rows_changed: bool, mirrored: bool);
pub async fn run_maintenance<C: CompletionProvider, E: EmbeddingProvider>(
    db: &Db, completion: &C, embedding: &E, executor: &CheapLlmTaskExecutor,
    req: MaintenanceRequest, ctx: MaintenanceContext) -> Result<MaintenanceRun, DbError>;
pub struct MaintenanceRun { pub report: MaintenanceReport, pub touched_subjects: Vec<String>, pub rows_changed: bool }

// corpus.rs: pub const CORPUS_PAGE_SIZE: i64 = 250; pub struct MaintenanceCorpus (IndexMap, shift_remove)
// fold_near_duplicates.rs: FOLD_MAX_BUCKET_ROWS = 2000; is_fold_eligible; plan_near_duplicate_folds;
//   apply_near_duplicate_folds(db, embedding, req) -> AppliedNearDuplicateFolds; apply_folds_to_corpus
// consolidate.rs: CONSOLIDATION_MAX_BUCKET_ROWS = 2000; bucket_candidates; run_consolidate_phase;
//   apply_consolidation_to_corpus
// demote.rs: is_demotable; plan_demotion(rows, cap, now_ms) -> DemotePhaseReport; plan_retention
// ledger.rs: the seven constants; WatermarkReading; LedgerStatus (to_json);
//   should_enqueue(db, character_id, reason, reading: Option<&WatermarkReading>, now_ms)
//     -> Option<Vec<MaintenancePhase>>; record_maintenance_outcome(&MaintenanceSummary);
//   take_watermark_check(id, now_ms) -> bool; note_enqueued(id, &phases, now_ms);
//   get_maintenance_ledger_status(db, Option<&str>) -> Vec<LedgerStatus>; reset_maintenance_ledger_for_tests()
// triggers.rs: maybe_enqueue_maintenance(db, &[String], now_ms) -> Vec<String>
//   (+ maybe_enqueue_maintenance_after_job(db, &BackgroundJob) — §9 P1)
// scheduler.rs: MAINTENANCE_INTERVAL_MS / STARTUP_GRACE_MS / RECENT_RUN_WINDOW_MS (re-export job_scheduler's);
//   should_run_startup_tick(db, now_ms) -> bool; run_scheduled_memory_maintenance(db)
//     -> Result<ScheduledMaintenanceResult { characters_scanned, jobs_enqueued }, DbError>
```

Queue (`services/queue_service.rs`): `pub struct MemoryMaintenancePayload` is risky for byte order — **propose payload helpers over `serde_json::Map` (preserve_order)**: `pub fn exempt_phases_of(payload: &Map) -> Vec<String>`, `pub fn widen_maintenance_payload(prior: &Map, new: &Map) -> Map`, `pub async fn enqueue_memory_maintenance(db, user_id, payload: Map, scheduled_at: Option<String>) -> Result<String, DbError>`, `enqueue_memory_maintenance_many(db, user_id, &[String], base: Map, scheduled_at) -> Result<Vec<String>, DbError>`, `enqueue_memory_maintenance_for_user(db, user_id, base: Map) -> Result<Vec<String>, DbError>`. Plus `BackgroundJobsRepository::update(id, &BjUpdate { payload: Some(..) })` (exists `:276`).

Handler: `services/memory_maintenance_job.rs` — `pub const MAINTENANCE_JOB_TIME_BUDGET_MS: i64 = 7 * 60 * 1000; MAINTENANCE_DEFER_MS = 2 * 60 * 1000; MAINTENANCE_FAIL_AFTER_PLAN_ENV`; `pub struct MemoryMaintenanceJobHandler { pub runner: Arc<dyn MaintenanceRunner> }` answering `JobOutcome::Completed(None)`; the summary + touched subjects handed to the completion region through a process-global **`memory_maintenance::job_results::stash(job_id, MaintenanceJobResult { summary, touched_subjects, rows_changed })` / `take(job_id)`** (the `setJobResult` analog — never persisted), consumed by ONE new call in `job_runner.rs`'s marked completion region: `memory_maintenance::after_job_completed(db, job)` = record the outcome + mirror the touched subjects (+ the frozen-archive invalidation stays the existing hook). Runner: rename `ConsolidationRunner` → **`MaintenanceRunner`** (`fn run(&self, db, request, ctx) -> MaintenanceRunFuture`), `install_maintenance_runner` / `maintenance_runner` / `clear_maintenance_runner`; host `quilltap-host/src/memory_maintenance.rs::memory_maintenance_job_handler(wire)`.

### 7.4 Wire (`api/types.rs`) — precise changes

REMOVE: `MemoryHousekeepPreview` (`:1689`), `MemoryHousekeep` (`:1701`), `MemoryHousekeepSweep` (`:1705`), `MemoryHousekeepingConfigGet` / `Set` (`:1707-1712`), `MemoryConsolidate` (`:2877`), `MemoryDedupPreview` / `MemoryDedupRun` (`:1286`, `:1293`) (the keystone / memories-API slice also removes `MemoryExtractionConcurrencyGet` / `Set` `:1725-1730`).
ADD (Request; `Response::Memory(Value)` carries every body, v4's exact JSON):
- `MemoryMaintain { options: serde_json::Value }` — RAW body (the `MemoryHousekeep` precedent: Zod issues computed in core);
- `#[serde(rename_all = "camelCase")] MemoryMaintenanceStatus { #[serde(default)] character_id: Option<String> }`;
- `MemoryRepair { options: serde_json::Value }`;
- `MemoryMaintenanceConfigGet`, `MemoryMaintenanceConfigSet { config: serde_json::Value }`.
`MemoryConsolidationConfigSet` / `MemoryExtractionModeConfigSet` keep their shape; semantics change (Δ28). `Response::MemoryMaintenance` (`:4551`) stays for the summaries-regen bodies — reword its doc (no dedup) or rename (planner). Engine arms: `api/engine.rs` replaces `:3190-3218`, `:3834-3871`, `:5614-5617`; handlers in a reshaped `api/memory_consolidation.rs` → proposed **`api/memory_maintenance_routes.rs`** (the existing `api/memory_maintenance.rs` keeps the summaries-regen half, or is renamed `api/conversation_summaries.rs`). The dry-run arm needs the installed `MaintenanceRunner` (R-E's typed refusal stays for an engine without one — after v4's validation and 404 arms).

---

## 8. SPA / help / docs / CLI touch points (pointers; other slices own them)

- **SPA (v4 pin):** `components/tools/memory-maintenance-card.tsx` (383), `memory-maintenance-ledger.tsx` (100; "Within the cap", muted, `48790fb73`), `memory-consolidation-particulars.tsx` (124; the `tfidfClusterThreshold` field "Cluster threshold (Built-in)" `:28`), `memory-extraction-limits-section.tsx` (97), `hooks/useMemorySettingsDocument.ts` (84); `components/memory/maintenance-dialog.tsx` (259), `maintenance-report.tsx` (271; "Clusters worth folding" = `clustersQualified`, "exempt"); DELETED `memory-housekeeping-card.tsx`, `memory-consolidation-card.tsx`, `memory-consolidation-report-dialog.tsx`, `memory-dedup-card.tsx`, `components/memory/housekeeping-dialog.tsx`; `MemorySearchTabContent.tsx` (old `section=` links open the new card). **v5:** `apps/web/src/app/screens/settings/memory/memory-{housekeeping,consolidation,dedup}-card.ts` (+ specs), `memory-consolidation-report-dialog.ts`, `memory-tab.ts`, `memory/housekeeping-dialog.ts`, `memory/memory.api.ts:18-230`, `core/core-contract.ts:2649-2650, :6012-6060, :6173-6293`, `layout/activity-kinds.ts` (the job-type table), `help/help-categories.ts`.
- **help/:** v4 `help/memory-maintenance.md` NEW (`url: /settings?tab=memory&section=memory-maintenance`); `memory-housekeeping.md` + `memory-consolidation.md` DELETED; `system-tools.md`, `cli-memories.md`, `settings.md`, `data-retention.md`, `memory-recall-relevance.md` + seven more edited. **v5:** `help/memory-consolidation.md`, `help/memory-housekeeping.md`, `help/system-tools.md`, `help/cli-memories.md` (re-vendor; `help_tree_equivalence`).
- **docs/v4/:** `API.md` (maintain / repair / retired aliases), `BACKGROUND_JOBS_CHILD.md`, `DDL.md`, `CLI.md`, the refactor spec → `features/complete/`.
- **CLI (v4):** `packages/quilltap/lib/memories-commands.js` — `memories maintain` (replaces `consolidate`), `memories status` reads `?action=maintenance-status` (`:1179`) and prints "within the cap" (`:1323`, `9ec4fa2e9`), `validate --repair` → `?action=repair`. **v5:** no `memories` verb family (`quilltap-cli/src/main.rs:55` lists the noun); completions `src/help/completion/{bash,zsh,fish}.template` name `consolidate` (`bash:320`, `zsh:541`, `fish:275`) — swap for `maintain` only when the verb family lands.

---

## 9. Open points for the order (observations / questions — not rulings)

- **P1 — retire `WATERMARK_ROWID_WINDOW` (P4.D271 R-B)?** At the pin every child memory insert is admission over host-RPC, written in the parent, which calls `maybeEnqueueMaintenance` per insert; the commit hook's `characterIdsFromMemoryWrites` sees only buffered `memories.create`, i.e. consolidation's own digests in a `MEMORY_MAINTENANCE` batch — excluded (§1.10). A faithful v5 port is "admission calls the watermark; the job-completion region adds nothing for memory writes". Proposed measurement: a v4 oracle row running a `MEMORY_EXTRACTION` batch through the real dispatcher (or a census over every `memories.create` caller at the pin: `admission/apply.ts:40` parent-only, `consolidation.ts:484` maintenance-only, `restore.ts:361` / `import-entities.ts:587` not jobs) showing `[]`. Ruling needed: retire the mark log + the ruled table, or keep a v5-only safety net (and re-rule it as a recorded divergence).
- **P2 — job-mode vs in-process mode in one process.** v5 runs every job in-process, so "the child" is a v5 choice. Proposal: `MaintenanceMode::Job` for handler runs ⇒ the summary reports `mirror.deferredToCommit: true, filesWritten: 0` exactly as v4's job, and the completion region mirrors + records (bytes the `maintenance-status` ledger exposes). The alternative (the handler announces itself) changes the ledger's `lastSummary.mirror` bytes. Planner ruling.
- **P3 — the summary hand-off.** v5's `JobOutcome::Completed(Some(v))` persists into `payload.result` (a v5-only capability); v4 never persists the summary. Use a process-global stash keyed by job id (§7.3), or widen `JobOutcome` with a non-persisted `side_result`? Either touches `job_runner.rs`'s marked region (append-only carve).
- **P4 — ONE `MemoryMaintenanceSettings` type** with the keystone survey; and whether `patchSchemaOf` is ported once generically (`zod_issues`) or per schema.
- **P5 — who owns `commonplace-mirror.ts` (bug 208) + the bridge's emptied-bucket removal + `mirrorDeletedDigestsAfterCommit`?** The mirror PHASE is this slice's; the after-delete refresh rides `retireMemories` (deletion slice). Split or one owner.
- **P6 — the fold depends on four admission exports and the admission lock**, the retire / demote / consolidate writes on Phase B's `memory_index` + `retire_memories`, ordering on Phase A's `protection()`. This lane cannot start until those C1 seams exist — schedule it AFTER (or as the last wave of) the admission + deletion + index keystones, or let it land against stubs only with an agreed contract (no stubs without agreement — CLAUDE.md).
- **P7 — size.** The slice is ~1,500 v4 lines of new module + the consolidation reshape + queue + handler + host loop + API + seven harness families (four retiring, three re-aimed). Suggested split: **(a)** core engine (`memory_maintenance/*` run / corpus / fold / consolidate / demote + `consolidate()` reshape + bug 246 + `tfidfClusterThreshold`, families 1–3); **(b)** jobs + scheduling (ledger, triggers, scheduler, queue, handler, host loop, job tables, the job-row ensure, families 4 + 7); **(c)** API + repair + bug 244 (families 5–6, `PARTIAL_DEFAULTS_CLOBBER` retirement) — (c) and (b) both need (a)'s types first.
- **P8 — the `maintain` dry run and model calls** reach the model in-process for up to 120 s per request through the host-installed runner (R-E). An engine without one: keep R-E's typed loud refusal (after v4's validation and 404 arms). A user-wide dry run limited to LLM-free phases needs no runner — still refuse when no runner, or run the LLM-free phases without one? (v4 runs them.)
- **P9 — `Response::MemoryMaintenance`'s name** now describes only the conversation-summaries regen bodies. Rename or reword.
- **P10 — `findOtherRunningMaintenance` under v5's runner.** v5's concurrency cap may already serialise jobs of one type per user; measure whether two `MEMORY_MAINTENANCE` jobs for one character can be PROCESSING together in v5 before porting the defer (port it anyway for parity of the +2 min successor row).
- **P11 — `null` vs absent `phases` in a stored `memoryMaintenance`** (`.prefault({})` reads absent as all-on; `null` fails the schema ⇒ the WHOLE settings fall to defaults with the WARN). Pin both in the schema family.
- **P12 — the INFO `phases` field** of `'[Memories API] Memory maintenance settings updated'` is an OBJECT — v5's `…Json` key convention applies (`phasesJson`).
- **Loud-refusal deferral candidates (tier 3):** none required — the only model-dependent surface (consolidation) already runs through the canned tier-3 machinery. The `QUILLTAP_TEST_MAINTENANCE_FAIL_AFTER_PLAN` test flag could be a documented NO-PORT (v5 has no buffered batch to drop; "fail after planning" would leave the writes already made) — ruling.
- **⚑ LEDGER CORRECTIONS:** (1) `104e986c7`'s "the `memory-dedup` TOOL" and §1's "the `memory-dedup` tool's retirement from the tool table" are the System-Tools route actions `memory-dedup-preview` / `memory-dedup` (`app/api/v1/system/tools/route.ts`, v5 `system_data_routes.rs:70-106`), NOT a model-visible tool: no LLM tool table / tool-count pin moves (v5's 57 tools are untouched). (2) "the 2,500 demotion cap" (brief) is not a constant — the default per-character cap is 2000 (bounds 100–100,000); 2,500 was the per-run cap Friday's backfill used (`48790fb73`'s spec: "Dry run at cap 2,500, then run for real… 913 (3,413 counted − 2,500)"). (3) `4ae0c4ca6`'s "boot + the maintenance pass check" for the TF-IDF refit is the DATA maintenance tick (`scheduled-maintenance.ts:52` `checkBuiltinRefit`, v5 `scheduled_maintenance.rs`), not the `MEMORY_MAINTENANCE` job. (4) `104e986c7`'s "a live `housekeep` enqueues" is transitional — `b045dd986` retires the action outright. (5) Bug 244's "audit v5's text-replacement PATCH": v5's `api/text_replacements.rs:352-420` `validate_patch` already leaves absent fields `None` — v5 was ALREADY in the post-fix shape there (a convergence, not a v5 defect; no table pins it).
