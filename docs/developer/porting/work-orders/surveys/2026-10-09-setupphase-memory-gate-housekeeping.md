# Survey — the WRITE side of v4's memory programme: gate reinforcement, housekeeping demotion, dedup fold

Read-only planning survey, 2026-10-09. v4 = `~/source/quilltap-server` main, HEAD `01a83539d`; v5 = main at `96cfdaaf7`.
Slug `memory-gate-housekeeping`. Commits: `3f7320138` (F1/F3/F8/F9 + F5's
invalidation CALLS), `70f9b495c` (bug 182), `d58548051` (write half: demotion,
retention, gate cold-tier). Everything below is from the hunks and the HEAD
files (`git show 01a83539d:<path>` ≡ the working tree, clean). **Out of scope**
(other surveys): the frozen-archive cache SHAPE (recall), F2 anchor probe, F4/F6/F7,
the consolidation engine + triggers + settings accessors, the OTHER-pass
fold grain, carriers (backup/restore/import), the D23 schema move itself (keystone),
the SPA.

The net span `f5e953a3f..01a83539d` is what a lane ports — `3f7320138`'s
housekeeping merge pass (F9 half) is **deleted again by `d58548051`**; at HEAD
the merge module survives ONLY under dedup.

---

## 1. v4 at HEAD — file:line facts

### 1.1 `lib/memory/reinforced-importance.ts` (NEW, `70f9b495c`, 14 lines)

`calculateReinforcedImportance(base, count) = Math.min(1.0, base + Math.log2(count + 1) * 0.05)` — byte-identical to the
baseline formula, moved so the repository can call it; `memory-gate.ts:30,126` re-exports it (so
`harness/oracle/cases/memory-name-helpers.ts`'s import still resolves — that family stays green).

### 1.2 `lib/memory/memory-gate.ts` (1046 lines)

Constants: `NEAR_DUPLICATE_THRESHOLD = 0.90` `:44`, `MERGE_THRESHOLD = 0.85` `:47`, `RELATED_THRESHOLD = 0.70` `:50`,
**`MAX_REINFORCEMENT_FOOTNOTES = 8` `:58` (`3f7320138`)**, `GATE_TOP_K = 5` `:61`, **`MAX_DIGEST_HOPS = 3` `:171` (`d58548051`)**.
`GateResult.debugInfo` strings are dropped by `createMemoryWithGate` (never logged/returned) → not observable; v5 carries no debugInfo — keep it that way.

**Cold-tier gate (`d58548051`)**

- `resolveColdForAbsorb(memory, debugInfo)` `:186-216`: hot → `{memory, redirected:false}` untouched. Cold:
  loop `hop < 3 && current.tier==='cold' && current.supersededById`: `findByIds([supersededById])`;
  missing → `logger.debug('[MemoryGate] Superseding digest missing; falling back to cold row', {memoryId, supersededById})`, `break`;
  found → `current = digest; redirected = true`. After the loop: **`current.tier==='cold' && !current.supersededById` → `promoteColdMemory(current)`** and return `{promoted, redirected}`; else `{current, redirected}`.
  ⚠ The doc comment says a vanished digest's row "is then treated as a non-superseded cold row" — **false**: its `supersededById` is still set, so it is NOT promoted (the jest test `gate-cold-tier.test.ts:109-116` pins `updateTierBulk` NOT called). Port the code. A found digest that is itself cold + unsuperseded IS promoted (and `redirected` stays true).
- `promoteColdMemory(memory)` `:224-241`: `repos.memories.updateTierBulk(characterId, [id], 'hot', { supersededById: null })`; then `store.setTier([id],'hot')` in try → catch `logger.warn('[MemoryGate] Failed to re-stamp vector tier after promotion', {memoryId, error: String(error)})`; then `logger.debug('[MemoryGate] Promoted cold memory to hot on re-observation', {memoryId, characterId})`; returns `{...memory, tier:'hot', supersededById:null}` (local view — row's `updatedAt` not refreshed).
- `redirectColdLink(memory)` `:248-258`: same ≤3-hop walk, never writes, never promotes; missing digest → stop silently.
- `runMemoryGateInner` `:260-419`:
  - date guard `:332-350`: `distinctOccasion` → `linkTarget = redirectColdLink(bestMemory)`; `INSERT_RELATED` with `[{memory: linkTarget, similarity: best.score}]`.
  - `>= 0.90` `:352-364`: `resolveColdForAbsorb` → `SKIP_NEAR_DUPLICATE {existingMemory: target, similarity}`.
  - `>= 0.85` `:366-389`: `resolveColdForAbsorb`; **`redirected` → `SKIP_NEAR_DUPLICATE` (count-only absorption on the digest, no footnotes)**; else `REINFORCE {existingMemory: target}` (a promoted row reinforces normally).
  - related band `:392-415`: raw band matches → each `redirectColdLink` → collapse into a `Map` keyed by target id: **insertion position = first-seen, similarity = max** (`!prior || match.similarity > prior.similarity`); `relatedMatches = Array.from(map.values())`.
  - A cold match seen only in the related band is never promoted (test `:130`).

**Reinforcement (`3f7320138` F1/F3, rewritten by `70f9b495c`)**

- `countReinforcementFootnotes(content)` `:426-432`: count of `content.split('\n')` lines with `startsWith('[+] ')`.
- `appendCappedFootnotes(content, details)` `:439-447`: `room = max(0, 8 - count)`; `appended = details.slice(0, room)`; none → `{content, appended: []}`; else `` `${content}\n${appended.map(d => `[+] ${d}`).join('\n')}` ``.
- `patchMemory(memory, patch)` `:457-469`: `updateForCharacter`; `null` → null; `undefined` (job child buffered) → debug `'[MemoryGate] Memory patch buffered (job child); using local view' {memoryId, fields}` + `{...memory, ...patch}` (**NO-PORT arm**); else the updated row.
- `reembedMemory(memory, userId, profileId?)` `:476-515`: ONE `generateEmbeddingForUser(buildMemoryEmbeddingText(memory.summary, memory.content, memory), userId, profileId, {priority:'background'})` — **no retry**; `updateForCharacter(id, {embedding})`; store `hasVector ? updateVector : addVector(id, emb, {memoryId, characterId})`; `save()`; `logger.debug('[MemoryGate] Re-embedded memory', {memoryId})` → true. The whole block in one try: `logger.warn('[MemoryGate] Failed to re-embed memory', {memoryId, error: String(error)})` → false (non-fatal, row/store write failures included). Replaces the baseline inline block's WARN `'[MemoryGate] Failed to re-embed reinforced memory'`.
- `countReinforcement(memory)` `:528-548` (`70f9b495c`): `at = new Date().toISOString()`; `incrementReinforcement(characterId, id, at)`; `null` → null; `undefined` (child) → debug `'[MemoryGate] Reinforcement buffered (job child); using local view' {memoryId, reinforcementCount}` + snapshot+1 (**NO-PORT arm**); else `{reinforcementCount, reinforcedImportance, lastReinforcedAt: at}`.
- `absorbNearDuplicate(existing)` `:558-574` (F1, `3f7320138`; count path `70f9b495c`): `countReinforcement`; null → `logger.warn('[MemoryGate] Failed to reinforce near-duplicate memory', {memoryId, characterId})`, return `existing`; else `logger.debug('[MemoryGate] Near-duplicate absorbed as reinforcement', {memoryId, reinforcementCount, reinforcedImportance})` and return `{...existing, ...counted}` (text, links, vector untouched; returned `updatedAt` is the SNAPSHOT's).
- `reinforceMemory(existing, candContent, candSummary, userId, profileId?, anchors?)` `:589-685`:
  1. `allNovel = extractNovelDetails(candContent, existing.content)`; `atCap = countReinforcementFootnotes(existing.content) >= 8` (pre-append).
  2. `{content: newContent, appended: novelDetails} = appendCappedFootnotes(existing.content, allNovel)`; `novelDetails.length < allNovel.length` → `logger.debug('[MemoryGate] Reinforcement footnote cap reached; dropping extra details', {memoryId, offered, appended, cap: 8})`.
  3. `updateData = {}` — **count / lastReinforcedAt / reinforcedImportance are NOT in the patch** (`70f9b495c`); `content` only when changed.
  4. Anchors: `occurredAt` / `narrativeTime` fill when candidate truthy and existing falsy; entities: candidate filtered `typeof e==='string' && e.trim().length>0`, `fresh` = not in existing (lowercase), `merged = [...existing, ...fresh].slice(0,12)`; **write + `anchorsChanged=true` ONLY when `merged.length > existing.length`** (`3f7320138`; a 12-entity row never "changes").
  5. **`countReinforcement(existing)` FIRST** → null → `logger.warn('[MemoryGate] Failed to update memory during reinforcement', {memoryId, characterId})`, return `{memory: existing, novelDetails}` (no patch attempted).
  6. `updatedMemory = {...existing, ...counted}`; if `updateData` non-empty → `patchMemory(updatedMemory, updateData)` (a SECOND write, its own `updatedAt`) → null → same WARN, return `{memory: updatedMemory, novelDetails}` (count already landed); else `updatedMemory = {...patched, ...counted}`.
  7. `!atCap && (contentChanged || anchorsChanged)` → `reembedMemory(updatedMemory, …)` (a third write). At cap: anchors land, vector frozen.
  8. Returns `{memory, novelDetails}` — `novelDetails` = the APPENDED (capped) list.
- `linkRelatedMemories` `:694-725` unchanged.
- `deleteMemoriesWithUnlinkBatch(ids, options = {})` `:831-935` (`3f7320138` F8/F9):
  - `options.currentLinks?: ReadonlyMap<id, {characterId, relatedMemoryIds}>` — DB candidates whose id is in the map are dropped and the map's entries are APPENDED after them (`:857-865`); scrub logic unchanged.
  - Count: per character `result = bulkDelete(...)`; finite number → `deleted += result`, else `deleted += ids.length; countSource = 'resolved-ids'` (`:905-915`). Default `countSource = 'repository'`.
  - **Log fields now `{requested, deleted, countSource, neighboursTouched, charactersAffected, durationMs}`** (`:917-928`) on both `'[MemoryGate] deleteMemoriesWithUnlinkBatch touched an unusually large neighbour set'` (warn, `>= 200`) and `'… complete'` (debug). The baseline had no `countSource` key.

### 1.3 `lib/memory/memory-service.ts` — `createMemoryWithGate` + the watermark

- `:63-` `maybeEnqueueHousekeeping`: `:78` **`countHotByCharacterId`** (was `countByCharacterId`, `d58548051`); the rest unchanged.
- `:420-432` `SKIP_NEAR_DUPLICATE` → `absorbed = absorbNearDuplicate(decision.existingMemory)`; outcome `{memory: absorbed, action: 'SKIP_NEAR_DUPLICATE', similarity}` (`3f7320138`). For a redirect, `memory` is the DIGEST — `memory-processor.ts:326-330`'s debug text `absorbed into ${outcome.memory?.id}` and `fold-episode-pass.ts:155-193` (fragment links onto `memory.id`) follow it.
- `:444-461` `REINFORCE` → `reinforceMemory(...)` (anchors ride along); `memory-processor.ts:341` logs `(count ${outcome.memory?.reinforcementCount ?? 1})` — now the row's at-write-time count.
- `CreateMemoryOptions.source` gains `'CONSOLIDATED'` (`:260`). `create` stamps `tier: data.tier ?? 'hot'`, `consolidatedFrom: data.consolidatedFrom ?? []` (repository `:490-497`).

### 1.4 `lib/database/repositories/memories.repository.ts` — the verbs this cluster calls (keystone-owned)

- `:33` `HOT_TIER_SQL = "COALESCE(tier, 'hot') = 'hot'"`.
- `:571-606` **`incrementReinforcement(characterId, memoryId, at)`** (`70f9b495c`) — rethrow-mode `safeQuery` (`'Error reinforcing memory'`, ctx `{collection:'memories', characterId, memoryId}`); `getRawDatabase()` null → throw `'Memory reinforcement: the main database is not initialized'`; one `db.transaction`:
  `SELECT "importance", "reinforcementCount" FROM "memories" WHERE "id" = ? AND "characterId" = ?` → none → null;
  `count = (row.reinforcementCount ?? 1) + 1`; `ri = calculateReinforcedImportance(row.importance ?? 0.5, count)`;
  `UPDATE "memories" SET "reinforcementCount" = ?, "reinforcedImportance" = ?, "lastReinforcedAt" = ?, "updatedAt" = ? WHERE "id" = ?` (`updatedAt = getCurrentTimestamp()`, a DIFFERENT clock read than `at`).
  null → `logger.warn('Memory not found for reinforcement', {memoryId, characterId})`; else `logger.debug('Memory reinforcement counted', {memoryId, characterId, reinforcementCount, reinforcedImportance})` → `{reinforcementCount, reinforcedImportance}`.
- `:758-773` `countHotByCharacterId` — `SELECT COUNT(*) AS n FROM memories WHERE characterId = ? AND COALESCE(tier, 'hot') = 'hot'`; fallback 0 (`'Error counting hot memories for character'`).
- `:874-907` **`updateTierBulk(characterId, ids, tier, extra = {})`** — fallback 0 (`'Error moving memories between tiers'`, ctx `{characterId, tier, count}`); empty → 0; per `SQLITE_VARIABLE_CHUNK_SIZE` chunk `updateMany({characterId, id: {$in: chunk}}, {tier, ...extra, updatedAt})` (base `updateMany` re-stamps `updatedAt = getCurrentTimestamp()`, `base.repository.ts:526-540`); `changed += modifiedCount`; `logger.debug('Moved memories between tiers', {characterId, tier, requested, changed, supersededById: extra.supersededById ?? undefined})` — **the key is dropped for demotion (`{}`) AND promotion (`null ?? undefined`)**, present only for the consolidator.
- `:947-966` **`findExpiredColdIds(characterId, olderThan)`** — fallback `[]`:
  `SELECT id FROM memories WHERE characterId = ? AND tier = 'cold' AND supersededById IS NOT NULL AND source != 'MANUAL' AND updatedAt < ?` (string compare; demotion/supersession and any later scrub write reset the clock).
- `findByCharacterIdInBatches` (`:129`, `ORDER BY id`) and `findByCharacterId` (`:110`, no tier filter) unchanged.

### 1.5 `lib/embedding/vector-store.ts` (`d58548051`)

`VectorMetadata.tier?` stamped at `load()` from `findColdIdsByCharacterId` (`:117`, `loadColdIds` `:493-504` warn `'Could not read cold memory ids; treating every vector as hot'`); `setTier(ids, tier)` `:506-522` re-stamps in-memory entries, `logger.debug('Re-stamped vector tiers', {context:'CharacterVectorStore.setTier', characterId, tier, requested, changed})`; `isHotVector` `:89`. The parent's cached store is reloaded after a child commit containing `memories.updateTierBulk` (`job-dispatcher.ts:772`).

### 1.6 `lib/memory/housekeeping.ts` (475 lines — net `d58548051` rewrite; `3f7320138`'s pass-2 fold is GONE)

- Options `:31-50`: `mergeSimilar` "Retired… accepted, ignored"; `userId` still accepted (unused now). Defaults `:98-106` unchanged: `maxMemories 2000, maxAgeMonths 6, maxInactiveMonths 6, minImportance 0.3, mergeSimilar false, mergeThreshold 0.9, dryRun false`.
- `HousekeepingResult` `:55-83` key order: `deleted, demoted, merged, kept, totalBefore, totalAfter, coldCount, capUsed, deletedIds, demotedIds, mergedIds, details`. `HousekeepingDetail.action` gains `'demoted'` `:90`.
- `isProtectedMemory` `:123-131`: `source === 'MANUAL' || 'CONSOLIDATED'` → protected; else `calculateProtectionScore(...).score >= 0.5`.
- `shouldDeleteMemory` `:136-175` unchanged (its "delete" verdict now means demote; reason strings unchanged: `Low importance (${pct}%) and old (${age} months)` / `…, old (…), and inactive (… months)` / `protected` / `within retention policy`).
- `runHousekeeping` `:195-436`:
  - load all via `findByCharacterIdInBatches(…, 250)`; `memories = all.filter(m => m.tier !== 'cold')`; `coldCount = all - hot`; `totalBefore = hot`.
  - **No empty early-return** (removed): `logger.debug('[Housekeeping] Starting sweep', {characterId, hot, cold, cap, dryRun})` `:241`.
  - `opts.mergeSimilar` → `logger.debug('[Housekeeping] mergeSimilar is retired in favour of consolidation; skipping the merge pass', {characterId})` `:249-253`.
  - Pass 1 `:276-304` over importance-desc / createdAt-desc sort: `demoteSet.add` + detail `{action:'demoted', reason, summary}` or `{action:'kept', …}`.
  - Pass 3 `:312-356` (only when `memories.length > 0`): pre-check unchanged; tail walk; demote → **MUTATES the pass-1 detail in place** (`existing.action = 'demoted'; existing.reason = \`Exceeded hot-memory limit (${max})\``) — the baseline PUSHED a second `deleted` entry beside the `kept` one. The `else push` arm is dead (pass 1 records every hot row).
  - `demotedIds = Array.from(demoteSet)` (insertion order: pass-1 then pass-3).
  - Retention `:361-374`: `retentionDays = (await getMemoryConsolidationSettings()).coldRetentionDays` (read on EVERY sweep, dry runs included); `> 0` → `cutoff = new Date(now - days*86_400_000).toISOString()`; `expiredIds = findExpiredColdIds(characterId, cutoff)`; each → detail `{memoryId, action:'deleted', reason: \`Superseded archive row older than the ${days}-day retention window\`}` (**no `summary` key**).
  - Apply demotion `:376-394`: non-dry & any → `updateTierBulk(characterId, demotedIds, 'cold')`; `setTier` in try → `logger.warn('[Housekeeping] Failed to re-stamp vector tier after demotion', {characterId, error})`; `result.demoted/demotedIds`; dirty. Dry → counts only.
  - Apply retention `:396-414`: non-dry & any → `deleteMemoriesWithUnlinkBatch(expiredIds)`; store `removeVector` each + `save()` in try → `logger.warn('[Housekeeping] Failed to clean up vector store', {characterId, error})`; `result.deleted = deletedCount`, `deletedIds = expiredIds`; dirty. Dry → `deleted = expiredIds.length`.
  - `vectorStoreDirty` → `invalidateFrozenArchive(characterId)` `:416-422`.
  - `kept = totalBefore - demotedIds.length`; `totalAfter = kept`; `logger.debug('[Housekeeping] Sweep computed', {characterId, dryRun, demoted, deleted, hotAfter})` `:427-433`.
  - `merged` always 0, `mergedIds` always `[]`.
- `needsHousekeeping` `:455-475`: `countHotByCharacterId`; `>= max*0.8` → true; `count > 0` → preview → `preview.demoted + preview.deleted > 0`.

### 1.7 `lib/memory/memory-merge.ts` (NEW `3f7320138`, 176 lines; unchanged by `d58548051` — dedup-only at HEAD)

- `planMemoryMerge(survivor, losers, excludeIds = [])` `:75-129` — PURE:
  `excluded = excludeIds ∪ {survivor} ∪ losers`; content grows loser-by-loser: `novel = extractNovelDetails(loser.content, content)` → `appendCappedFootnotes(content, novel)` (cap counts the survivor's existing footnotes); `contentChanged`;
  `reinforcementCount = survivor.rc ?? 1 + Σ(loser.rc ?? 1)`;
  links = `[...survivor.links, ...losers.flatMap(links)]` minus excluded, insertion-ordered Set;
  `occurredAt` = earliest by `Date.parse` (finite only; first wins ties);
  `lastReinforcedAt` = **lexicographic** max of non-empty strings (`.sort().pop()`, UTF-16 order — NOT a date compare);
  patch keys in order: `content` (if changed), `reinforcementCount` + `reinforcedImportance` (if count moved; `calculateReinforcedImportance(survivor.importance, rc)` — absolute write, NOT the atomic increment), `relatedMemoryIds` (if `!sameIds` — order-insensitive, length-sensitive), `occurredAt` (if truthy and differs), `lastReinforcedAt` (if truthy and differs).
  Returns `{survivorId, patch, mergedDetails, contentChanged}`.
- `applyMemoryMerge(survivor, plan, {userId?, embeddingProfileId?})` `:139-176`: empty patch → `logger.debug('[MemoryMerge] Nothing to fold into survivor', {survivorId})`, return survivor; `patchMemory` → null → `logger.warn('[MemoryMerge] Failed to update survivor', {survivorId, characterId})` → null; `needsReembed = contentChanged || patch.occurredAt !== undefined` → userId ? `reembedMemory(updated, …)` : `logger.warn('[MemoryMerge] Survivor changed but no userId to re-embed with', {survivorId})`; `logger.debug('[MemoryMerge] Folded losers into survivor', {survivorId, fields: Object.keys(patch), mergedDetails: count, reembedded: needsReembed && !!userId})` (true even when the re-embed failed); returns `updated`.
- Header `:18-25`'s "a plan's links exclude only its own group" is what the code does; dedup's comment `memory-dedup.ts:264-266` ("recomputed against the full removal set") is **stale** — both calls pass no `excludeIds`.

### 1.8 `lib/tools/memory-dedup.ts` (469 lines; `3f7320138` only)

- `deduplicateCharacterMemories(characterId, name, threshold, dryRun, userId?)` `:157`; reads `findByCharacterId` — **ALL tiers, digests included** (no tier filter; see §6 Q8).
- Per cluster `:264-270`: `preview = planMemoryMerge(survivor, discards)`; `mergedDetailCount = preview.mergedDetails.length` (now CAPPED and judged against the growing content — the baseline deduped details across discards by lowercase key, uncapped); `allMerges.push({survivor, losers})`.
- Apply `:292-361` (`!dryRun && allRemoveIds.length > 0`): per merge `try { plan = planMemoryMerge(survivor, losers); applied = applyMemoryMerge(survivor, plan, {userId}) } catch → logger.warn('[MemoryDedup] Failed to fold discards into survivor', {context:'memory-dedup.deduplicateCharacterMemories', characterId, survivorId, error})`; applied → `survivorLinks.set(id, {characterId, relatedMemoryIds})` else every loser → `keptIds`; `keptIds.size > 0` → `removeIds = all - kept` + `logger.warn('[MemoryDedup] Some folds failed; their discards were kept', {context, characterId, kept})`; `deleteMemoriesWithUnlinkBatch(removeIds, {currentLinks: survivorLinks})`; `logger.info('[MemoryDedup] Bulk deleted memories', {context, characterId, requested, deleted})`; vector cleanup (warn `'[MemoryDedup] Failed to clean up vector store'`); **`invalidateFrozenArchive(characterId)`** `:360`.
- `removedCount = removeIds.length` (post-fold); `finalCount = original - removed`. The baseline's explicit `updatedAt: now` patch is gone (repository stamps it anyway).
- `deduplicateAllMemories(userId, threshold, dryRun)` `:388` passes `userId` through; info lines `'Starting deduplication for all characters'` `:393`, `'Deduplication complete'` `:448` unchanged. Called from the PARENT (`app/api/v1/system/tools/route.ts:1108/1142`).

### 1.9 The job handler, outcome cache, dispatcher hook

- `lib/background-jobs/handlers/memory-housekeeping.ts` (143 lines): `:70-76` `runHousekeeping(cid, {userId, maxMemories?, mergeThreshold?, mergeSimilar?, dryRun?})`; `:82-90` NaN guard debug `'[Housekeeping] Non-finite deleted count; using deleted id count' {jobId, characterId, reported, deletedIds}` (`3f7320138`, child-only); `:100-107` **`recordHousekeepingOutcome(cid, result.demoted + result.deleted, totalBefore, capUsed)`** (non-dry); `:109-122` `logger.info('[Housekeeping] Completed sweep for character', {jobId, userId, characterId, reason: reason ?? 'unknown', dryRun: dryRun ?? false, totalBefore, totalAfter, demoted, deleted, cold, merged, kept})`; `:124-129` `logger.error('[Housekeeping] Sweep failed for character', {jobId, userId, characterId, error})` and continue; `:44-49` `logger.error('[Housekeeping] Failed to enumerate characters for user', {jobId, userId, error})` and return; `:134-142` `logger.info('[Housekeeping] Job complete', {jobId, userId, charactersSwept, totalDemoted, totalDeleted, totalMerged, reason})`.
- `lib/memory/housekeeping-outcome-cache.ts:46-55`: `safeDeleted = Number.isFinite(deleted) ? deleted : 0` (`3f7320138`); doc: "effective when it demoted (or deleted)".
- `lib/background-jobs/host/job-dispatcher.ts` (logger `child({module:'jobs:dispatcher'})` `:43`): `:256` `void invalidateFrozenArchivesForCompletedJob(job)` after the realtime topic hints, inside the success try; `:649` `JOBS_INVALIDATING_FROZEN_ARCHIVE = {'MEMORY_HOUSEKEEPING','MEMORY_CONSOLIDATION'}`; `:671-695`: not in set → return; `payload.dryRun === true` → return; `characterId` string → `invalidateFrozenArchive(cid)` else `invalidateAllFrozenArchives()`; `log.debug('Invalidated frozen memory archives after job', {jobId, type, characterId: cid ?? 'all'})`; catch `log.warn('Failed to invalidate frozen memory archives after job', {jobId, error})`. **Fires on EVERY non-dry completion, changed or not** — `runHousekeeping`'s own call fires only when dirty. Observable: the frozen archive re-ranks by `Date.now()` on recompute. (`:259` consolidation watermark is the consolidation cluster's.)
- `lib/background-jobs/scheduled-housekeeping.ts` (consolidation first + 30-min deferral) — scheduler/consolidation cluster.

### 1.10 Memories API — `app/api/v1/memories/route.ts`

- `housekeepingOptionsSchema` `:99-108` unchanged (`characterId` uuid; `maxMemories` 10–10000; `maxAgeMonths`/`maxInactiveMonths` 1–120; `minImportance` 0–1; `mergeSimilar` bool; `mergeThreshold` 0.8–1; `dryRun`).
- `handleHousekeep` `:631-678` → `{success, dryRun, result: {demoted, deleted, merged, kept, totalBefore, totalAfter, coldCount, demotedIds, deletedIds, mergedIds, details?}}` (`details` only on dryRun).
- `handleHousekeepPreview` `:701-763` (query parsing unchanged, `mergeSimilar` still parsed) → `{success, preview: {wouldDemote, wouldDelete, wouldMerge, wouldKeep, totalBefore, totalAfter, coldCount, details}}`.
- `handleCharacterMemoryCounts` `:1004` still `countByCharacterId` (all tiers) — unchanged.
- SPA wire needs (not this cluster's code): `components/memory/housekeeping-dialog.tsx` reads `preview.wouldDemote`, `wouldDelete`, `coldCount?`, `details[].action ∈ {deleted, demoted, merged, kept}`, `result.demoted`; it no longer SENDS `mergeSimilar` (preview query or POST body); `components/tools/memory-housekeeping-card.tsx` drops the `mergeSimilar` toggle (the config schema `:122-128` still accepts it).

---

## 2. v5 today — counterparts

| v4 | v5 | State |
|---|---|---|
| `calculateReinforcedImportance` | `crates/quilltap-core/src/memory_gate.rs:45` | ✓ (tier-1 via `memory_name_helpers_equivalence`) |
| `MAX_REINFORCEMENT_FOOTNOTES`, `countReinforcementFootnotes`, `appendCappedFootnotes` | — | MISSING |
| `memory-merge.ts` | — | MISSING |
| `absorbNearDuplicate` / `countReinforcement` | `services/memory_gate.rs:276-287` `SkipNearDuplicate` arm writes NOTHING (pinned by unit test `near_duplicate_skips_without_writing`) | MISSING |
| `reinforceMemory` | `services/memory_gate.rs:657-856` — uncapped footnotes (:679-688); `existing_count + 1.0` from the snapshot (:675) written ABSOLUTELY in ONE patch with content/anchors (:760-800) → **bug 182's lost update** (the writer serializes writes, not the read-modify-write); entity union sets `anchors_changed` whenever `fresh` is non-empty even when `truncate(12)` leaves the length unchanged (:741-757); re-embed via `generate_with_retry` (TWO provider calls, :803-806) with WARN `'[MemoryGate] Failed to re-embed reinforced memory'` (:823) and `?`-propagated row/store writes — P4.88's recorded pre-existing divergences, both fixable by a shared `reembed_memory` | STALE |
| gate cold tier (`resolveColdForAbsorb`, promote, link redirect, related collapse) | `run_memory_gate_inner` :392-534 (read-only; related band = raw list :503-513) | MISSING |
| `maybeEnqueueHousekeeping` hot count | `services/memory_gate.rs:1046` `memories_read::count_by_character_id` | STALE |
| `MemoryGateOutcome.memory` (absorbed / redirected / promoted) | `MemoryGateOutcome.memory_id` :87-107 = `id_of(existing)` | needs the TARGET id after redirect |
| `deleteMemoriesWithUnlinkBatch` | `db/memories.rs:815-923` `delete_many_with_unlink` — real count; log fields lack `countSource`; no `current_links` | PARTIAL |
| `runHousekeeping` / preview / needs | `services/housekeeping.rs` (828 lines; P4.d5∥P4.d6∥P4.6ay; P4.d27 dim skip) — baseline shape: merge pass 2 (:324-444), deletes (:497-536), `Exceeded memory limit` (:490) as a PUSHED second detail, early return on empty (:283), MANUAL-only protection (:185-191), no tier filter, no retention, no logs, no frozen-archive invalidation; `needs_housekeeping` all-tier count + `deleted > 0` (:552-571) | STALE |
| `HousekeepingResult` | `services/housekeeping.rs:113-125` — no `demoted` / `cold_count` / `demoted_ids`; `action: &'static str` ∈ deleted/merged/kept | STALE |
| dedup | `services/memory_dedup.rs` (580 lines; P4.43 unit 6) — baseline shape: `MemRow` slim read (:127-158 — id, content, summary, importance, embedding only), uncapped cross-discard details (:322-353), survivor content patch + delete in ONE `db.write` (:378-395), no fold, no re-embed, no provider, no `invalidate_frozen_archive`, no info lines (only the per-character ERROR :502) | STALE |
| dedup API | `api/memory_maintenance.rs:34-59` (`memory_dedup(db, user_id, threshold, preview)`); dispatch `api/engine.rs:3178-3191` `ready_db()` — no embedding provider | STALE |
| housekeep API | `api/memories.rs:832-882` (preview: `wouldDelete, wouldMerge, wouldKeep, totalBefore, totalAfter, details`), `:907-955` (housekeep: `deleted, merged, kept, totalBefore, totalAfter, deletedIds, mergedIds, details?`); `HousekeepBag` has no Zod bounds — bespoke `Invalid housekeeping options: {e}` (pre-existing, unrecorded) | STALE |
| frozen-archive invalidation | `services/frozen_archive.rs:91` `invalidate_frozen_archive` (character-keyed cache; **NO production caller**); no `invalidate_all_frozen_archives` | recall cluster reshapes; this cluster CALLS |
| dispatcher completion hook | `services/job_runner.rs:439-485` (`topics_for_completed_job` loop :483) | MISSING |
| handler | `quilltap-host/src/spine.rs:2937-3040` `MemoryHousekeepingHandler` — records `result.deleted` (:3027-3032); **none of v4's four log lines**; per-character `if let Ok` swallow; enumerate failure returns silently | STALE |
| outcome cache | `services/housekeeping_outcome_cache.rs:42-52` — no finite guard | STALE (trivial) |
| vector store | `db/vector_store.rs` — loaded FRESH per operation (status-log `:44735`), no metadata, no cache → v4's `setTier` and the dispatcher's vector-store reload have no v5 object to act on | by construction |

---

## 3. What the ledger / commit prose got wrong

1. **`3f7320138` F8 "NO-PORT (v5's `delete_many_with_unlink` returns a real count)"** — half wrong. The NaN guard is child-only, but F8 also ADDED `countSource` to BOTH batch log lines (`memory-gate.ts:917-928`), an observable field every v5 caller emits. v4's value is `'repository'` in the parent and `'resolved-ids'` in the job child (the housekeeping job's retention delete). Needs a ruling (§6 Q1).
2. **`3f7320138` "the cap pass skips merge survivors" / WARN `Some merges failed…`** — both are housekeeping-merge artefacts `d58548051` deleted; at HEAD only dedup's `'[MemoryDedup] Some folds failed; their discards were kept'` exists.
3. **`3f7320138` "v5: direct calls suffice" (the dispatcher hook)** — not quite: the hook invalidates on every non-dry `MEMORY_HOUSEKEEPING` / `MEMORY_CONSOLIDATION` completion (per-character, or ALL for a user-wide sweep) even when nothing changed; `runHousekeeping` invalidates only when dirty. A recompute re-ranks at a later `Date.now()`, so the difference reaches the prompt. Port the hook in `job_runner.rs`.
4. **`d58548051` "the opt-in `coldRetentionDays` sweep of superseded cold AUTO rows"** — the SQL is `source != 'MANUAL'`: superseded cold **CONSOLIDATED** digests (re-consolidated) are deleted too. The schema comment (`settings.types.ts:236`) says "AUTO" as well; port the SQL.
5. **`d58548051` "reason `Exceeded memory limit (N)` → `Exceeded hot-memory limit (N)`"** — true, but the bigger move is unmentioned: the cap pass now MUTATES the pass-1 `kept` detail in place instead of pushing a second entry, so `details` loses one entry per cap demotion.
6. The `resolveColdForAbsorb` doc comment (vanished digest "treated as a non-superseded cold row") is false against its own code + test — no promotion.
7. `70f9b495c` row: "reads by `(id, characterId)`, writes …" — the UPDATE's WHERE is `"id" = ?` only (harmless after the gated SELECT). Otherwise accurate.

---

## 4. Differential plan

### 4.1 Predicted to redden at the target pin

| Family | Why | Recipe (test header) |
|---|---|---|
| `memory_gate_tier3_equivalence` | `skip_near_duplicate` now writes count / `reinforcedImportance` / `lastReinforcedAt`; D23 columns in the dump | `QT_ORACLE_GATE`, `QT_FIXTURE_GATE`; fixture `build-memory-gate-fixture.ts`, case `memory-gate-tier3.test.ts`, spec `memory-gate-tier3.json` (header :27-43) |
| `memory_housekeeping_tier2_equivalence` | merge retired (H4's merge rows stay), delete → demote (tier column), result keys, details shape, `Exceeded hot-memory limit`; **the fixture MUST be rebuilt at HEAD's schema** — v4 HEAD over the old fixture silently no-ops (`updateTierBulk` fallback 0, `findExpiredColdIds` `[]`, both logging errors) | `QT_ORACLE_MEMHOUSEKEEPING`, `QT_FIXTURE_MEMHOUSEKEEPING`; `build-memory-housekeeping-fixture.ts`, `memory-housekeeping-tier2.ts` (header :46-56) |
| `memory_dedup_equivalence` | fold: summed `reinforcementCount`, `reinforcedImportance`, unioned links, earliest `occurredAt`, re-embed, capped `mergedDetailCount`; `MEM_COLS` (case :86-101) omits the count columns — widen | `QT_ORACLE_MEMORY_DEDUP`; committed `crates/quilltap-web/tests/fixtures/embedding-profiles-{main,mount}.db`; `memory-dedup.test.ts` header :20-29 |
| `memories_routes_equivalence` | `create_near_duplicate` (routes case :312-327; Rust :712-728) bumps the count in body + table; `housekeep_preview` / `housekeep_dryrun` / `housekeep_run` (config case :328-360) gain `wouldDemote`/`coldCount` / `demoted`/`coldCount`/`demotedIds` | `QT_ORACLE_MEMORIES_ROUTES`, `QT_ORACLE_MEMORIES_CONFIG` (header :30-46) |
| `context_summary_service_tier3_equivalence`, `fold_episode_tier3_equivalence` | MEASURE: the context-summary oracle's embedding mock is text-independent (case :350-372), so a second episode in one window scores cosine 1.0 → `SKIP_NEAR_DUPLICATE` → now a count write. (Also reddened by the OTHER-pass cluster.) | `QT_ORACLE_CTXSUM`/`QT_FIXTURE_CTXSUM{,_MOUNT}`; `QT_ORACLE_FOLD_EPISODE`/`QT_FIXTURE_FOLD_EPISODE_{MAIN,MOUNT}` |
| `memory_watermark_tier3_equivalence` | D23 columns only, unless a cold row is seeded (do: proves `countHot`) | `QT_ORACLE_WATERMARK`, `QT_FIXTURE_WATERMARK` (header :16-31) |
| `carina_memory_extraction_tier3`, `memory_processor_tier3`, `memory_pipeline_jobs_tier3` | serial REINFORCE reaches the same final count; red only via D23 columns or a corpus row at the footnote cap / SKIP band — measure | `QT_ORACLE_CARINA_MEM`…, `QT_ORACLE_PROCESSOR`…, `QT_ORACLE_MPJ`… |
| core unit tests | `services/memory_gate.rs` `near_duplicate_skips_without_writing` (red by design), `a_failed_re_embed_warns_without_failing_the_reinforcement` (:1481, message moves); `services/housekeeping.rs` tests (:824 `Exceeded memory limit (3)`); `db/memories.rs` `delete_many_with_unlink_logs_v4s_complete_debug` if `countSource` lands; `job_runner.rs` housekeeping handler test (:1086-1146) | — |
| stays green | `memory_delete_tier2_equivalence` (logs not compared), `memory_name_helpers_equivalence` (re-export), `extract_novel_details_equivalence` | — |

### 4.2 New cases

- **Tier-1** `memory_merge_plan_equivalence` + oracle `harness/oracle/cases/memory-merge-plan.ts` importing v4's REAL `planMemoryMerge`, `appendCappedFootnotes`, `countReinforcementFootnotes`, `MAX_REINFORCEMENT_FOOTNOTES`: rows for 0/7/8/9 existing footnotes, two losers sharing a detail, `\r\n` content, count sums with `undefined` counts, links with duplicates / excluded ids / order-insensitive equality, `occurredAt` unparseable + ties, `lastReinforcedAt` lexicographic-vs-chronological disagreement (e.g. `2026-1…` vs `2026-09…` offsets / `Z` vs `+00:00`), patch key order.
- **Tier-3 gate** (extend `memory-gate-tier3.json`): absorb (count moves, content/vector untouched), cap reached mid-append (`offered > appended`, re-embed), at-cap reinforce with an anchor fill (no re-embed, anchors land), 12-entity row + fresh entity (no anchor change, no re-embed), REINFORCE on a missing row; cold tier mirroring `lib/memory/__tests__/gate-cold-tier.test.ts:79-145`: superseded → digest (SKIP and REINFORCE bands), missing digest (no promote), demoted row promoted (REINFORCE at 0.87, SKIP at 0.95), related band collapse to one digest, related-only cold not promoted, a 4-hop chain (stops at 3), a cold-unsuperseded digest at the end of a hop (promoted), date guard onto a superseded cold row. Needs the keystone's tier columns in the fixture builder.
- **Tier-2 housekeeping** (rebuild + extend corpus): cold rows excluded from cap + reported in `coldCount`; CONSOLIDATED protected over cap; retention on with `instance_settings['memoryConsolidation'] = {coldRetentionDays: N}` seeded — superseded cold AUTO + CONSOLIDATED deleted, cold MANUAL and unsuperseded cold kept, `updatedAt` boundary; dry-run of both; zero-hot character (no early return — retention still runs); `mergeSimilar: true` accepted/ignored.
- **Tier-3 dedup**: mock `generateEmbeddingForUser` with a canned vector in the oracle (jest `doMock` over `@/lib/embedding/embedding-service`, keeping `cosineSimilarity` actual) and pass a `CannedEmbeddingProvider` Rust-side — makes the re-embed deterministic; one failed-fold cluster (survivor id missing → patch null → losers kept, links to them survive).
- **Bug 182**: a core unit test — two concurrent `create_memory_with_gate` REINFORCE calls on one row from the same snapshot both count (+2). Mirror `__tests__/unit/lib/background-jobs/child-proxy-memory-housekeeping.test.ts:135-230` (the replay half is NO-PORT; the "refuses another character's row" assertion ports to the increment verb — keystone).
- **Completion hook**: unit test mirroring `lib/background-jobs/host/__tests__/job-dispatcher-frozen-archive.test.ts` (single character / user-wide / dryRun / MEMORY_EXTRACTION / none).
- v4 jest suites to mirror: `__tests__/unit/lib/memory/memory-recall-housekeeping-fixes.test.ts` (F1 :116-160, F3 :161-222, F8 :272-335, F9 :337-410), `__tests__/unit/lib/memory/housekeeping-demotion.test.ts` (:93-172), `__tests__/unit/lib/memory/housekeeping.test.ts` (:128-250), `lib/memory/__tests__/gate-cold-tier.test.ts`. (`__tests__/unit/api/memories-consolidation-actions.test.ts` mocks housekeeping to `{}` — consolidation cluster's, nothing to mirror here.)

---

## 5. Proposed unit decomposition (one lane, after the KEYSTONE)

| Unit | Content | v5 files owned |
|---|---|---|
| **U1 pure** | `MAX_REINFORCEMENT_FOOTNOTES`, `count_reinforcement_footnotes`, `append_capped_footnotes`; NEW `memory_merge.rs` `plan_memory_merge` (+ `MemoryMergePlan`); tier-1 family | `crates/quilltap-core/src/memory_gate.rs`, NEW `crates/quilltap-core/src/memory_merge.rs`, `crates/quilltap-core/src/lib.rs` (mod line), NEW `crates/quilltap-harness/tests/memory_merge_plan_equivalence.rs`, NEW `harness/oracle/cases/memory-merge-plan.ts` |
| **U2 reinforcement** (F1 + F3 + bug 182) | `count_reinforcement` (over keystone `increment_reinforcement`), `absorb_near_duplicate`, `reinforce_memory` rewrite (cap, count-first, content/anchor-only patch, entity length guard, at-cap no re-embed), shared `patch_memory` / `reembed_memory` (ONE provider call; v4's debug/warn lines; non-fatal row/store writes); `SkipNearDuplicate` arm absorbs; outcome carries the post-count `reinforcement_count` | `crates/quilltap-core/src/services/memory_gate.rs` (+ its unit tests); `crates/quilltap-harness/tests/memory_gate_tier3_equivalence.rs`, `harness/oracle/cases/memory-gate-tier3.test.ts`, `harness/oracle/fixtures/memory-gate-tier3.json`, `harness/oracle/fixtures/build-memory-gate-fixture.ts` |
| **U3 gate cold tier** | `resolve_cold_for_absorb`, `promote_cold_memory`, `redirect_cold_link`, related-band digest collapse (insertion-ordered, max similarity), the redirected-REINFORCE → count-only path, the date-guard link redirect, the three log lines; `maybe_enqueue_housekeeping` → hot count; outcome `memory_id` = the target | same files as U2 (sequential) |
| **U4 housekeeping demotion** | the §1.6 rewrite: hot filter, `cold_count`, demote via `update_tier_bulk`, retired-merge debug, in-place detail mutation, retention via `find_expired_cold_ids` + the delete chokepoint + vector removal (warn on failure), `invalidate_frozen_archive` when dirty, the three debug lines, `needs_housekeeping` (hot count, `demoted + deleted`); delete pass 2 + its P4.d27 dim skip | `crates/quilltap-core/src/services/housekeeping.rs` (+ tests); `crates/quilltap-harness/tests/memory_housekeeping_tier2_equivalence.rs`, `harness/oracle/cases/memory-housekeeping-tier2.ts`, `harness/oracle/fixtures/memory-housekeeping-tier2.json`, `harness/oracle/fixtures/build-memory-housekeeping-fixture.ts` |
| **U5 surfaces** | API housekeep + preview wire (§1.10 key orders); the spine handler (`demoted + deleted` record; the four log lines; drop `merge_*` only if ruled); `invalidate_frozen_archives_for_completed_job(job_type, payload)` + its two lines in the completion block; outcome-cache finite guard | `crates/quilltap-core/src/api/memories.rs` (housekeep arms :805-955 only), `crates/quilltap-host/src/spine.rs` (`MemoryHousekeepingHandler` :2932-3040 only), `crates/quilltap-core/src/services/job_runner.rs` (completion block :439-485 only), `crates/quilltap-core/src/services/housekeeping_outcome_cache.rs`; `memories_routes_equivalence.rs` + `harness/oracle/cases/memories-config.test.ts` (regen only) |
| **U6 dedup fold** | full-Memory read, `plan_memory_merge` preview count, fold-first `apply_memory_merge` (in `memory_merge.rs` or `services/memory_dedup.rs`), kept losers + warn, info lines, `invalidate_frozen_archive`; provider plumbing | `crates/quilltap-core/src/services/memory_dedup.rs`, `crates/quilltap-core/src/api/memory_maintenance.rs`, `crates/quilltap-core/src/api/engine.rs` (the two `MemoryDedup*` arms :3178-3191 only — frozen-engine rule: smallest diff), `crates/quilltap-harness/tests/memory_dedup_equivalence.rs`, `harness/oracle/cases/memory-dedup.test.ts` |
| **U7 F8 log field** | `countSource` on both batch lines (+ `current_links` if ruled) | `crates/quilltap-core/src/db/memories.rs` — **keystone-owned file**: fold into the keystone, or land after it |

---

## 6. Meeting points + open questions

### 6.1 Draft contract — what this cluster needs from the KEYSTONE (schema + repo verbs)

- D23 columns + `MemCreate { tier: String /* "hot" */, superseded_by_id: Option<String>, consolidated_from: Vec<String> /* [] */, consolidated_at: Option<String> }` (create stamps v4's defaults); `MemUpdate` gains `tier: Option<String>`, `superseded_by_id: Option<Option<String>>`, `consolidated_at: Option<Option<String>>`, `consolidated_from: Option<Vec<String>>`.
- Net Memory JSON (`memories_read::marshal_row`) carries `tier`, `supersededById`, `consolidatedFrom`, `consolidatedAt` in `MemorySchema` order (the gate reads `tier` / `supersededById` off `find_by_ids` rows; NULL/absent tier ⇒ hot, matching `m.tier !== 'cold'`).
- `MemoriesRepository::increment_reinforcement(&self, character_id: &str, memory_id: &str, at: &str) -> Result<Option<(f64 /*reinforcementCount*/, f64 /*reinforcedImportance*/)>, DbError>` — §1.4 SQL, both statements in one writer closure (a SAVEPOINT is optional — the writer is single-threaded); the WARN/debug lines; rethrow-mode error line `Error reinforcing memory` (P4.149 fallback-module style).
- `MemoriesRepository::update_tier_bulk(&self, character_id: &str, ids: &[String], tier: &str, extra: TierExtra { superseded_by_id: Option<Option<String>>, consolidated_at: Option<Option<String>> }) -> Result<i64, DbError>` — chunked, `updatedAt` minted, the `Moved memories between tiers` debug (key dropped unless `Some(Some(_))`), FALLBACK 0 + `Error moving memories between tiers {collection, characterId, tier, count}`.
- `memories_read::count_hot_by_character_id(conn, character_id) -> Result<i64>` (fallback 0 semantics) and `memories_read::find_expired_cold_ids(conn, character_id, older_than: &str) -> Result<Vec<String>>` (fallback `[]`), §1.4 SQL.
- `delete_many_with_unlink` `countSource` (U7) — if the keystone owns `db/memories.rs`.

### 6.2 From other clusters

- **Recall** (frozen-archive shape): `services::frozen_archive::invalidate_frozen_archive(character_id: &str)` (all chats of the character) and NEW `invalidate_all_frozen_archives()`, each with v4's `[FrozenArchive] Invalidated …` debug. This cluster calls them from `run_housekeeping` (dirty), `deduplicate_character_memories` (apply), and the job-completion hook. Recall also owns the vector-store tier stamp (`find_cold_ids_by_character_id` at `CharacterVectorStore::load`) — the write side needs nothing from it beyond writing the column.
- **Consolidation**: `get_memory_consolidation_settings(conn) -> MemoryConsolidationSettings { cold_retention_days: Option<i64>, … }` (v4 `readJsonSetting` → defaults on absent/invalid); it shares `job_runner.rs`'s completion block (`checkConsolidationWatermarkAfterCommit`) — one owner or sequence; it consumes `update_tier_bulk` + `invalidate_frozen_archive`; `MEMORY_CONSOLIDATION` joins the hook's set. `calculate_reinforced_importance` stays in `memory_gate.rs` (`consolidation-plan.ts:37` imports it from the gate).
- **OTHER-pass / fold**: `fold_episode_pass.rs` / `memory_processor.rs` consume `MemoryGateOutcome.memory_id` — after U3 it is the DIGEST on a redirect (fragment links land on the digest, as v4).
- **SPA**: wire only (§1.10).

### 6.3 Open questions / measurements before coding

1. **`countSource` ruling**: v5 is in-process ⇒ always `"repository"`; v4 says `"resolved-ids"` for the housekeeping JOB's retention delete. Record a divergence, or mimic per call site?
2. **`current_links`**: in-process the survivor's patched list IS the DB row by the time the delete runs — the override changes only scrub ORDER (minted `updatedAt`s). NO-PORT with a reason, or port the parameter?
3. **`setTier` / vector reload**: no cached store in v5 — the `Re-stamped vector tiers` debug and both `Failed to re-stamp …` warns have nothing to act on. Rule NO-PORT (log-only divergence) or emit the debug from a computed load?
4. **Dedup's provider**: `ready_memory_embedding()` refuses when the embedding seam is unassembled; v4 runs dedup and warns per changed survivor. Use the optional-provider shape (`ready_db_and_memory_embedding`) and decide what a `None` seam logs. Measure what HEAD's dedup does on the committed `embedding-profiles` fixture today (default profile OPENAI, no key?) before choosing the oracle mock.
5. Measure whether `context_summary_service_tier3` / `fold_episode_tier3` reach `SKIP_NEAR_DUPLICATE` (§4.1).
6. Re-pin + rebuild the housekeeping, gate, watermark fixtures at HEAD (tier columns; housekeeping also needs `instance_settings` for the retention read — absent table ⇒ v4 falls back to defaults? measure).
7. Rule the spine's `merge_threshold` / `merge_similar` plumbing: accepted and ignored at HEAD — keep passing (faithful) or drop.
8. **v4 behaviours to port faithfully, possibly worth filing upstream**: (a) dedup reads ALL tiers — it can delete a CONSOLIDATED digest (orphaning its members' `supersededById`; the gate then falls back to the cold row) or fold a hot loser into a COLD survivor (the hot row vanishes from recall); (b) retention deletes superseded cold CONSOLIDATED rows, contra "AUTO" in the docs; (c) `resolveColdForAbsorb`'s doc vs code (§3.6).
9. Pre-existing v5 gaps surfaced (not this round's drift; rider candidates): the housekeep POST bag skips v4's `housekeepingOptionsSchema` bounds (`maxMemories` 10–10000, `mergeThreshold` 0.8–1, months 1–120 → v4 `validationError`) and answers a bespoke sentence; the spine handler lacks v4's four log lines; `run_housekeeping` swallows the vector-cleanup failure without v4's WARN; dedup lacks `Starting deduplication…` / `Bulk deleted memories` / `Deduplication complete`.
10. `lastReinforcedAt` in `plan_memory_merge` is a STRING max — do not route it through `iso_to_ms`; `occurredAt` uses `js_date_parse_ms` (not the strict `iso_to_ms`).
