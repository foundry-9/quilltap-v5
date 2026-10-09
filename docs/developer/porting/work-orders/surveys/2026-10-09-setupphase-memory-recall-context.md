# Survey — the memory programme's READ / RECALL side: `3f7320138` (F2, F4-consumer, F5, F6, F7) + `f7d8064be` (recall multiplier retuning, R1–R7) + the read half of `d58548051` (hot-only recall, digests, provenance, archived labels, list `?tier=`)

Read-only planning survey, 2026-10-09. v4 = `~/source/quilltap-server` main HEAD `01a83539d` (`4.10.0-dev.143`, tree clean); v5 = `quilltap-v5` main at `96cfdaaf7`. Every fact below is from the hunks (`git show <sha> -- <path>`) and the HEAD files (`git show 01a83539d:<path>`), never the commit prose. Line numbers are **HEAD** unless marked. For every file in this cluster, HEAD == the file as `d58548051` left it (`git log d58548051..01a83539d` touches none of them; `70f9b495c` touches only `memory-gate.ts` + the repository's new `incrementReinforcement`, both OUT of scope here).

**Out of scope (other surveys):** the gate (F1/F3/bug 182), housekeeping demotion + F8/F9 + `mergeSimilar` retirement, the consolidation engine/job/triggers/config actions, the fold-OTHER extraction grain, the schema/migration/D23 re-dump + every repository WRITE, the backup/restore/`.qtap` carriers, the SPA, the `help/` + `docs/v4/` vendoring. They are only LOCATED where this cluster consumes them (§7 is the draft KEYSTONE contract).

---

## 1. The ranking engine — `recall-tags.ts` + NEW `recall-tuning.ts` (`f7d8064be`)

### 1.1 v4 at HEAD — `lib/memory/recall-tags.ts`

- `:29-31` type-only import of `RecallMultiplierTable`, `ResolvedRecallTuning` from `./recall-tuning` (a value import would be a circular-init trap).
- `RecallContext` gains `:155 tuning?: ResolvedRecallTuning` and `:160 presentParticipantNames?: readonly string[]`.
- `RECALL_MULTIPLIERS` `:167-220` — **key order is load-bearing** (it drives `describeRecallTuning`'s iteration and the override schema): `scopeNarrowSameProject 1.15, scopeNarrowCrossProjectDownWeight 0.15, temporalPast 0.85, temporalMoment 0.7, contextMatch 1.1, participantPresent 1.2, recentlyWhispered 0.6, temporalPastRetrospective 1.15, temporalMomentRetrospective 1.0, occurredWithinWindow 1.3, freshEvent24h 1.3 (was 1.6), freshEvent48h 1.15 (was 1.35)` (`:218-219`).
- `:228 MULTIPLIER_CLAMP = {min 0, max 4}` unchanged.
- `:243 interface BoostGate { abs: number|null; margin: number|null; ramp: number }`.
- `:250 boostGateThreshold(gate, bestCosine)` = `Math.max(gate.abs ?? -Infinity, gate.margin === null ? -Infinity : bestCosine - gate.margin)`.
- `:259 boostGateStrength(gate, cosine, bestCosine)`: `cosine >= threshold → 1`; `ramp <= 0 → 0`; else `Math.min(1, Math.max(0, (cosine - (threshold - ramp)) / ramp))`.
- `:276 RECALL_TUNING_DEFAULTS` (key order): `boostGate {abs 0.45, margin 0.15, ramp 0.1}`, `boostCap 1.4`, `freshBypassesGate false`, `windowBypassesGate false`, `specificAnchors true`, `anchorMinHits 1`, `anchorOrder 'rarest'`, `backgroundReserve 0`.
- `:288 recallTuningOf(ctx)` = `ctx.tuning ?? { multipliers: RECALL_MULTIPLIERS, ...RECALL_TUNING_DEFAULTS }`.
- The seven multiplier fns (`scopeProjectMultiplier`, `temporalMultiplier`, `occurredWithinMultiplier`, `freshEventMultiplier`, `contextMultiplier`, `participantMultiplier`, `recentlyWhisperedMultiplier`) each gain a trailing `multipliers: RecallMultiplierTable = RECALL_MULTIPLIERS` param and read their value from it (labels unchanged).
- `:568 interface RecallRelevance { cosine; bestCosine }`.
- `:593 combineRecallMultipliers(memory, ctx, relevance?)`: scope-exclude short-circuit unchanged; `parts = [scope, temporal, context, participant, recent, window, fresh]`; `fired = parts.flatMap(p => p.fired)` (same order as before); `product` = the same left-assoc 7-factor product; then
  ```
  boost = Π p.multiplier where > 1   (in parts order)
  penalty = Π p.multiplier where ≤ 1 (incl. exactly 1)
  cap = tuning.boostCap; gate = tuning.boostGate
  strength = gate && relevance ? boostGateStrength(gate, relevance.cosine, relevance.bestCosine) : 1
  ungated = 1; if freshBypassesGate && fresh>1: ungated *= fresh; if windowBypassesGate && window>1: ungated *= window
  gatedBoost = boost / ungated
  combined = product
  if (boost > cap || (strength < 1 && gatedBoost > 1)) {
    scaled = 1 + (Math.min(gatedBoost, cap) - 1) * strength
    combined = penalty * Math.min(scaled * ungated, cap)
    if (boost > cap) fired.push(`cap${cap}`)                       // :651 — JS String(cap): "cap1.4"
    if (strength < 1 && gatedBoost > 1) fired.push(`gate×${strength.toFixed(2)}`)  // :652 — U+00D7
  }
  multiplier = Math.max(0, Math.min(4, combined))
  ```
  **The cap applies with NO relevance passed** (only the gate needs `relevance`): any boost product > 1.4 is now capped + labelled on every caller, including pure ones.
- `TurnRecallContextInput` gains `:692 presentParticipantNames?`, `:694 tuning?`; `buildTurnRecallContext` `:703-718` copies each only when `!== undefined` (after `turnRetrospective`).

### 1.2 v4 at HEAD — NEW `lib/memory/recall-tuning.ts` (207 lines, pure, `f7d8064be`)

- `:31-65 ResolvedRecallTuning { multipliers, boostGate|null, boostCap, freshBypassesGate, windowBypassesGate, specificAnchors, anchorMinHits, anchorOrder, backgroundReserve }`.
- `:67 DEFAULT_RECALL_TUNING` = frozen `{ multipliers: {...RECALL_MULTIPLIERS}, ...RECALL_TUNING_DEFAULTS }`.
- `:72-77` multiplier override shape: every `RECALL_MULTIPLIERS` key → `z.number().min(0).max(10).optional()`; `:79 gateConstant = z.number().min(0).max(1).nullable().optional()`.
- `:86-100 RecallTuningInputSchema` (`.strict()` at both levels; key order): `boostGateAbs`, `boostGateMargin`, `boostGateRamp` (gateConstant), `boostCap` (`number.min(1).max(10).optional()`), `freshBypassesGate`, `windowBypassesGate` (boolean opt), `multipliers` (`z.object(shape).strict().optional()`), `specificAnchors` (bool opt), `anchorMinHits` (`int.min(1).max(100)`), `anchorOrder` (`enum ['rarest','distiller']`), `backgroundReserve` (`number.min(0).max(0.5)`).
- `:109-117 RecallReplaySignalsSchema` (NOT strict — unknown keys stripped): `keywords: string[]` (REQUIRED), `temporal?: enum(TEMPORAL_VALUES)`, `context?: enum(CONTEXT_VALUES)`, `paraphrase?: string`, `retrospective?: boolean`, `timeRange?: {from,to}|null`, `entities?: string[]`.
- `:123 gateTerm(value, fallback)`: `undefined → fallback`; `typeof number && > 0 → value`; else (0/null) `null`.
- `:133 resolveRecallTuning(input?)`: falsy → `DEFAULT_RECALL_TUNING`; gate = `abs===null && margin===null ? null : {abs, margin, ramp: gateTerm(ramp, 0.1) ?? 0}`; multipliers merged over defaults (`typeof value === 'number'` only); other knobs `??` default. **Return key order**: `multipliers, boostGate, boostCap, freshBypassesGate, windowBypassesGate, anchorMinHits, anchorOrder, specificAnchors, backgroundReserve` (anchorMinHits/anchorOrder BEFORE specificAnchors — differs from the defaults object; matters only if a lane serializes it).
- `:166 selectSpecificAnchors(candidates, presentNames, max=3, {minHits, order})`: `minHits = Math.max(1, minHits ?? 1)`; present = lower+trim set (empties dropped); eligible = `count >= minHits && !present.has(phrase.trim().toLowerCase())`; `rarest` → stable sort `count asc, index asc`; `slice(0, max)`.
- `:184 describeRecallTuning(t)` → comma-joined parts or `'defaults'`: gate differs (by `JSON.stringify`) → `gate abs=${abs ?? 'off'} margin=${margin ?? 'off'} ramp=${ramp}` or `gate off`; `cap=${boostCap}`; `freshBypassesGate=…`; `windowBypassesGate=…`; each differing multiplier `key=value` in table order; anchors → `specificAnchors(minHits=N, order)` or `specificAnchors=false`; `backgroundReserve=…`. All numbers via JS `String(n)`.

### 1.3 v5 today

- `crates/quilltap-core/src/recall_tags.rs`: constants as free `pub const` (`:144-175`; `FRESH_EVENT_24H 1.6` `:174`, `FRESH_EVENT_48H 1.35` `:175`); clamp `:184-185`; `RecallMultiplier.fired: Vec<&'static str>` `:208`, `CombinedRecallAdjustment.fired: Vec<&'static str>` `:228`; `RecallContext<'a>` `:251-292` (no `tuning`, no `present_participant_names`); `combine_recall_multipliers(memory, ctx)` `:591-646` (plain clamp, no relevance). Ported P4.d13 unit 2 / P4.d26 unit 2.
- `services/memory_service.rs`: `RecallAdjustment.fired: Vec<String>` `:296` (already owned strings — the dynamic labels fit), `RecallContextInput` `:306-339` (no tuning / names), `apply_recall_multipliers` `:828`.
- No `recall_tuning.rs`; no `RecallMultiplierTable`; no `pascal`-independent JS `String(number)` in this module — reuse `crate::pascal::js_value::number_to_string` (`:159`) for `cap${cap}` / describe, `crate::jsnum::to_fixed` (`jsnum.rs:33`) for `gate×`.
- **Type ripple:** the two dynamic labels force `fired` to an owned/`Cow` type through `recall_tags.rs` (all 7 fns + tests), `apply_recall_multipliers`, and `recall_tags_equivalence.rs`.

### 1.4 Ledger / prose corrections

- Ledger "gate only when `relevance` is passed" is right, but it omits that the **cap** fires without relevance — so `recall_tags_equivalence`'s existing `combine` rows whose boosts multiply past 1.4 redden even though no caller passes relevance there (e.g. `narrow✓ 1.15 × ctx✓ 1.1 × present↑ 1.2 = 1.518` → `1.4` + `cap1.4`).
- `RecallContext.tuning`'s doc comment ("Absent → today's constants, byte-identical ranking", `recall-tags.ts:151-154`) is FALSE at HEAD: absent → `RECALL_TUNING_DEFAULTS` (cap 1.4, gate on). Port the behaviour, not the comment (ledger §5.3 already notes this).

---

## 2. `searchMemoriesSemantic` + `markMemoriesAccessed` (`3f7320138` F6, `f7d8064be` R1–R6, `d58548051` hot-only)

### 2.1 v4 at HEAD — `lib/memory/memory-service.ts`

- Options (`:771-868`): `source?: 'AUTO'|'MANUAL'|'CONSOLIDATED'` (`d58548`); NEW `excludeMemoryIds?: ReadonlySet<string>`, `headSize?: number`, `weightClockMs?: number`, `embeddingMemo?: Map<string, EmbeddingResult>` (`f7d8`); NEW `includeCold?: boolean` (`:867`, `d58548`; doc: "every recall path reads hot rows only… The `search` tool and the Commonplace Book UI pass true").
- `:883-891 embed(text)`: memo key `` `${embeddingProfileId ?? 'default'}|${text}` ``; hit → cached result; else `generateEmbeddingForUser(text, userId, embeddingProfileId)` then `memo?.set`. Used for the main query AND extra probes (NOT the anchor-free/probe module).
- `:951-958` `includeCold = options.includeCold === true`; `hasExclusions`; `vectorFilter = (hasExclusions || !includeCold) ? meta => (!hasExclusions || !excluded.has(meta.memoryId)) && (includeCold || isHotVector(meta)) : undefined` — **filtered inside the vector scan, before top-K**.
- `:963-968` TF-IDF gate-off: `if (recallContext && !recallContext.tuning && embeddingResult.provider === 'BUILTIN')` → `recallContext = {...recallContext, tuning: {...recallTuningOf(recallContext), boostGate: null}}` + **debug `[Memory] TF-IDF embeddings: recall boost gate off` `{characterId}`** (fires on EVERY recall-context search under BUILTIN).
- `:969 tuning = recallContext ? recallTuningOf(recallContext) : undefined` (tool / API / first-message / Carina / voice paths have no recall context → `tuning` undefined).
- `:970 weightClock = weightClockMs !== undefined ? new Date(weightClockMs) : undefined`.
- `:973-976` main search `vectorStore.search(emb, limit*3, vectorFilter)`; probes `:988` same filter.
- `:1025-1029 entityPhrases` = `entityAnchors.map(trim).filter(p => !!p && p.length >= 2)` — **filter THEN slice** (was `slice(0,3)` then filter — v5 `memory_service.rs:595` still has the old order: `take(3)` then filter).
- `:1030-1038 hitsFor(phrase)` caches per phrase: `searchByContent(characterId, phrase).filter(m => !excluded?.has(m.id) && (includeCold || m.tier !== 'cold'))`.
- `:1039-1053` `if (tuning?.specificAnchors)`: count `hitsFor` over **every** entity phrase (not 3), `selectSpecificAnchors(counted, recallContext?.presentParticipantNames ?? [], 3, {minHits, order})`, **debug `[Memory] Specific entity anchors chosen` `{characterId, counted: [{phrase,count}], chosen}`** (fires even with `[]`); else `anchorPhrases.push(...entityPhrases.slice(0,3))`. Union loop `:1059-1069` now reads `hitsFor` (cached).
- `:1105 rawCosine = Map(id → vr.score)` (pre-literal-boost; extras carry their computed cosine).
- `:1111` hydrate drop: `if (!includeCold && memory.tier === 'cold') return null`.
- `:1122 calculateEffectiveWeight(memory, undefined, weightClock)`.
- `:1146 bestCosine = results.reduce((best,r) => Math.max(best, rawCosine.get(id) ?? r.score), 0)` — over the pool AFTER the minScore / minImportance / source / about filters, BEFORE the window filter.
- `:1152-1178` window: when `windowHits.length >= limit` on the injector path, also records `outOfWindow = results.filter(!inWindow)` (R6 input).
- `:1194-1206 adjust(pool, extraFired=[])`: `combineRecallMultipliers(r.memory, recallContext, {cosine: rawCosine ?? r.score, bestCosine})`, `fired: [...adj.fired, ...extraFired]`, sorted `byBlendedAfter` (`:741`, `(b.blendedAfter ?? 0) - (a.blendedAfter ?? 0)`).
- `:1218-1230` expansion now passes `{minImportance, source, excludeMemoryIds, includeCold}`, `bestCosine`, `weightClock`.
- `:1236-1253` R6: only when `gate && tuning.backgroundReserve > 0 && outOfWindow.length > 0 && options.headSize`: threshold, `present` = ids in results, background = `adjust(outOfWindow.filter(!present && cosine >= threshold), ['bg↺'])`, **debug `[Memory] Background reservation (R6)` `{characterId, threshold, qualifying, reserve: Math.floor(headSize*backgroundReserve)}`**, `reserveBackgroundSlots` (`:752`, pure: `reserve = floor(headSize*fraction)`, `taken = min(reserve, background.length)`, `0 → ranked`, else head = `[...ranked.slice(0, headSize-taken), ...background.slice(0,taken)].sort(byBlendedAfter)`, tail likewise). **Inert at defaults** (reserve 0).
- `:1263 return results.slice(0, limit)` — **no access bump** (`3f7320`); the catch-arm `:1266` WARN unchanged; the fallback `return searchMemoriesText(...)` — **no bump**.
- `expandRelatedMemories` `:1290-1360`: neighbour skip `if (filters.excludeMemoryIds?.has(neighborId)) continue` (`:1312`, AFTER the in-pool/neighbourSet checks); `:1329 if (!filters.includeCold && memory.tier === 'cold') continue`; `:1335` weight clock; `:1339 combineRecallMultipliers(memory, recallContext, {cosine: cosineScore, bestCosine})` — a neighbour is gated on its OWN cosine; label `related↗` appended.
- `:1373 export function markMemoriesAccessed(characterId, memoryIds)` (was private `bumpAccessTimes`): dedup `Array.from(new Set(ids.filter(id => typeof id === 'string' && id.length > 0)))`; empty → return; **debug `[Memory] Marking memories accessed` `{characterId, count}`**; `repos.memories.updateAccessTimeBulk(characterId, ids)` fire-and-forget; failure **WARN `[Memory] Failed to bump lastAccessedAt for used memories` `{characterId, count, error}`** (message text changed from `…for retrieved memories`).
- `searchMemoriesText` `:1395`: `options.includeCold?`; `:1447 if (!options.includeCold) memories = memories.filter(m => m.tier !== 'cold')` FIRST among the filters; its weight is `calculateEffectiveWeight(memory)` — **wall clock; ignores `weightClockMs`**.

### 2.2 Callers' `includeCold` / stamping matrix (HEAD)

| caller | includeCold | stamps (F6) |
|---|---|---|
| context-manager dynamic head `:1525` | false | archive+head delivered ids `:1608` |
| context-manager inter-char relevance `:1811` | false | inter-char delivered ids `:1846` |
| pre-compute proactive `:329` | false | none (buildContext stamps what it formats) |
| first-message-context `:145` | false | `selected` per participant `:220` |
| Carina `carina.service.ts:218` | false | `formatted.debugMemories` ids `:242` |
| voice-rewrite `recallForSeed` `:69` | false | `formatted.debugMemories` ids `:88` (after the `!formatted.content` return) |
| search tool `search-scriptorium-handler.ts:261` | **true** `:278` | post-`limit` memory ids `:521-531` (only when `context.characterId`) |
| Memories API search `route.ts:553` | **true** `:560` | **none** |
| recall-replay old/new `:284/:297` | false | **none** |

### 2.3 v5 today — `crates/quilltap-core/src/services/memory_service.rs`

- `SemanticSearchOptions` `:344-394` (no exclude / head_size / weight clock / memo / include_cold). `search_memories_semantic` `:399-448` bumps at `:430` (vector) and `:444` (fallback, except dim-mismatch); `bump_access_times` `:1221` (private, no dedup, no lines). `try_semantic` `:454`; vector search `:518` (no filter param — `db/vector_store.rs:148 search(query, limit)`); anchor loop `:595` (`take(3)` first); hydrate/blend `:640-700` (`options.now_ms`); recall loop + expand `:745-810`; `expand_related_memories` `:889` (no exclude/cold/relevance); `search_memories_text` `:1025` (no cold filter). Store is loaded per search from the read pool (`:500-504`), so a load-time tier stamp is always current in v5 (no `setTier` needed on the recall path).

### 2.4 Corrections

- Ledger "`searchMemoriesSemantic` + the text fallback STOP stamping" ✓; "the Memories API search and recall-replay stamp NOTHING" ✓.
- **Under BUILTIN (every committed TF-IDF fixture) the R1 gate is OFF in every live path** — only an explicit replay `tuning` turns it on there. A differential that wants R1 must script a non-`BUILTIN` provider label (§9).

---

## 3. Pool sizing — `sizeMemoryPools` (F7, `3f7320138`)

### 3.1 v4 at HEAD — `lib/chat/context/memory-injector.ts`

- Floors kept: `:42 DYNAMIC_HEAD_TOKEN_BUDGET 200`, `:44 DYNAMIC_HEAD_DEFAULT_SIZE 5`, `:53 RETRO_HEAD_TOKEN_BUDGET 600`, `:55 RETRO_HEAD_SIZE 10`. NEW `:58 DYNAMIC_HEAD_BUDGET_RATIO 0.15`, `:60 DYNAMIC_HEAD_MAX_TOKEN_BUDGET 1200`, `:62 DYNAMIC_HEAD_MAX_SIZE 15`, `:64 HEAD_TOKENS_PER_ENTRY 40` (private), `:67 FROZEN_ARCHIVE_MIN_SIZE 25`, `:69 FROZEN_ARCHIVE_MAX_SIZE 60`, `:71 ARCHIVE_TOKENS_PER_ENTRY 60` (private).
- `:103 sizeMemoryPools(memoryBudget, retrospective)`:
  `budget = max(0, floor(memoryBudget))`; `baseHeadTokens = min(budget, clamp(round(budget*0.15), 200, 1200))`; `baseHeadEntries = clamp(round(baseHeadTokens/40), 5, 15)`; retro: `headTokenBudget = min(budget, max(600, base*2))`, `headEntries = max(10, baseEntries*2)`; `archiveSize = clamp(round(max(0, budget-baseHeadTokens)/60), 25, 60)`; return `{headTokenBudget, headEntries, archiveTokenBudget: max(0, budget - headTokenBudget), archiveSize}` (JS `Math.round` — use `jsnum::math_round`).
- At the 2,000 floor: ordinary 300 tok / 8 entries / archive 28 / archiveTokens 1700; retro 600 / 16 / 1400. At 8,000: 1200 / 15 / 60 / 6800; retro 2400 / 30.
- `pre-compute.service.ts:48 PROACTIVE_RECALL_POOL_SIZE = FROZEN_ARCHIVE_MAX_SIZE + DYNAMIC_HEAD_MAX_SIZE * 2` = **90**; `:338 limit: PROACTIVE_RECALL_POOL_SIZE` (was 20); `:360 return { memories: memoryResults, … }` (no `.slice(0, 10)`).

### 3.2 v5 today

`memory_injector.rs:73-83` the four floors only. `pre_compute.rs:393 limit: Some(20)`, `:440 take(10)` (`finish_outcome`). `build_context.rs:2455` archive call (`size None`), `:2643-2650` fallback limit `(RETRO|DEFAULT) * 3`, `:2685-2694` fixed budgets, `:2730-2738` fixed `max_entries`. No pools debug line.

---

## 4. The frozen archive (`3f7320138` F5 + `d58548051` digests)

### 4.1 v4 at HEAD — `lib/memory/frozen-archive-cache.ts` (207 lines)

- `:41 FROZEN_ARCHIVE_SIZE 25`, `:44 FROZEN_ARCHIVE_CACHE_MAX_ENTRIES 64`, `:47 FROZEN_ARCHIVE_DIGESTS_PER_PRESENT 3`, `:50 FROZEN_ARCHIVE_SELF_DIGESTS 5`, `:55 POOL_FACTOR 4`.
- `:57-64 CacheEntry {characterId, generation, size, presence, memories}`; `:67` one insertion-ordered `Map` (first key = LRU); `:69 cacheKey = characterId + '\u0000' + chatId`.
- `:79 getOrComputeFrozenArchive(characterId, chatId, compactionGeneration, {size?, presentCharacterIds?})`: `present = [...new Set(ids)].filter(id => id && id !== characterId).sort()` (JS default sort = UTF-16 code-unit order, NOT localeCompare); `presence = present.join(',')`; hit iff `generation === && size === && presence ===` → delete+re-set (recency) and return; miss → compute, delete+set, evict oldest while `size > 64`; **debug `[FrozenArchive] Computed archive` `{characterId, chatId, compactionGeneration, size, presentCharacters, returned, cacheEntries}`**. **Presence is a freshness check, not part of the key** (a presence change replaces that chat's entry).
- `:130 invalidateFrozenArchive(characterId)` drops every chat's entry for the character; debug `[FrozenArchive] Invalidated archives for character` `{characterId, dropped}` only when `dropped > 0`. `:144 invalidateAllFrozenArchives()` clears; debug `[FrozenArchive] Invalidated all archives` `{dropped}` when > 0.
- `:157 computeFrozenArchive(characterId, size, present)`: `poolSize = max(size, size*4)`; `chosen` Map; `take(list)` stops at `size`, first-seen wins; for each present id → `findHotDigests(characterId, otherId, 3)`; then `findHotDigests(characterId, 'self', 5)`; `digestCount`; if `chosen.size < size`: `findMostImportant(characterId, poolSize)` → drop chosen → `calculateEffectiveWeight(memory)` (wall clock) → stable sort desc → `take`; **debug `[FrozenArchive] Composed archive` `{characterId, size, presentCharacters, digests, filled}`**; result sorted `a.id.localeCompare(b.id)`.
- So digests are chosen FIRST but **printed in id order** — the archive output is not "digests first".
- Invalidation callers (HEAD): `housekeeping.ts:421` (only when `vectorStoreDirty`), `memory-dedup.ts:360`, `consolidation.ts:1038` (non-dry-run with creates/updates/tierMoves), and the parent's `job-dispatcher.ts:256 → :671 invalidateFrozenArchivesForCompletedJob(job)`: types `MEMORY_HOUSEKEEPING`, `MEMORY_CONSOLIDATION` (`:649`); `payload.dryRun === true → skip`; `payload.characterId` string → per-character, else **all**; debug `Invalidated frozen memory archives after job` `{jobId, type, characterId: id ?? 'all'}`; catch WARN `Failed to invalidate frozen memory archives after job` `{jobId, error}`. The hook fires on EVERY non-dry-run completion of those types (even when nothing changed).
- context-manager `:1398-1405`: `ordinarySizing`, `retroSizing`; archive call `(character.id, chat.id, compactionGen, {size: ordinarySizing.archiveSize, presentCharacterIds: [...participantCharacters.keys()].filter(id => id !== character.id)})`.

### 4.2 v5 today — `services/frozen_archive.rs` (298 lines, P4.6ay)

Character-only key `HashMap<String, CacheEntry{generation, memories}>` `:40-48`; `get_or_compute_frozen_archive(db, character_id, generation, size, now_ms)` `:57`; `invalidate_frozen_archive` `:91` removes one key and has **no production caller**; no LRU, no presence, no digests, no lines; `rank_frozen_archive` `:135` (pure; the fill re-rank is reusable). `find_most_important` is `ORDER BY importance DESC` (`db/memories_read.rs:531`, KEYSTONE).

---

## 5. Context delivery — buildContext, inter-character, first-message, recap, Carina, voice, search tool, optimizer

### 5.1 v4 at HEAD

**context-manager.ts**
- `:251 INTER_CHAR_DIGEST_LIMIT = 2`.
- `:1510-1511` (fallback dynamic-head path only) `presentParticipantNames = [character.name, ...participantCharacters.values().map(c => c.name)].filter(non-empty string)` → `buildTurnRecallContext({..., presentParticipantNames, nowMs})`.
- `:1537 limit: (fallbackRetro ? retroSizing : ordinarySizing).headEntries * 3`.
- `:1557-1560` `turnSizing`, `dynamicHeadBudget = turnSizing.headTokenBudget`, `archiveBudget = turnSizing.archiveTokenBudget`; `formatDynamicMemoryHead(..., {maxTokens, maxEntries: turnSizing.headEntries})`.
- `:1586-1597` **debug `[ContextManager] Memory pools sized` `{chatId, characterId, memoryBudget, retrospective, headTokenBudget, headEntries, archiveTokenBudget, archiveSize (ordinary), headUsed, archiveUsed}`**.
- `:1608-1613 markMemoriesAccessed(character.id, [...archiveFormatted.debugMemories, ...headFormatted.debugMemories].map(memoryId).filter(non-empty))` — after `whisperedMemoryIds`, before `sections`.
- `:1785-1800` inter-char: `digestLists = Promise.all(otherIds.map(id => findHotDigests(character.id, id, 2)))`; `partitionMemories = findByCharacterAboutCharacters(...)` (hot, KEYSTONE); `interCharacterMemories = [...digestLists.flat(), ...partition]` deduped by id first-seen; `:1831 interCharacterLoadedCount` includes digests (→ the `Inter-character memory retrieval complete` debug `loadedCount`); `:1846 markMemoriesAccessed(character.id, formatted.debugMemories ids)` inside the `if (… length > 0)` block.

**memory-injector.ts**
- `:194 formatDigestProvenance(memory)`: `source !== 'CONSOLIDATED' → ''`; `count = consolidatedFrom?.length ?? 0`; `count <= 0 → ''`; `` ` (from ${count} ${count === 1 ? 'note' : 'notes'})` ``.
- Suffix sites (between body/summary and `meta`): inter-char `:587`, frozen archive `:660`, dynamic head `:760`. **NOT `formatMemoriesForContext` (Carina)** — the ledger's "digests … carry the model-visible suffix" overgeneralises.
- Debug `memoryId` added (first key): `formatMemoriesForContext` `:462`, inter-char `:598` (`DebugInterCharacterMemoryInfo.memoryId?` `:277`), frozen archive `:670`; the head already had it `:769`.
- Inter-char importance half `:561-567`: sort `(isDigest(b) - isDigest(a)) || (b.weight - a.weight)`; the relevance half is unchanged.

**first-message-context.ts** (`loadMemoriesForParticipant` `:100-221`)
- `:112-119` `allAbout = findByCharacterAboutCharacter(...).filter(m => m.tier !== 'cold')`; `recentMemories = [...digests, ...non-digests]` (stable) — affects only the take-3 `:122`.
- semantic search unchanged (hot by default); text fallback `:193 if (memory.tier === 'cold') continue`.
- `:208-220` `selected = entries.sort(importance desc).slice(0, limit)`; `markMemoriesAccessed(speakingCharacterId, selected ids)`; return values.

**memory-recap.ts** `:248 digestsFirst(memories)` (stable partition; returns the input when 0 or all digests); `:301-304` `tiered = {...fetched, high: digestsFirst(fetched.high)}` — **high bucket only**.

**carina.service.ts** `:242-245` `markMemoriesAccessed(characterId, formatted.debugMemories ids)` after the `!formatted.content || memoriesUsed === 0` return. **voice-rewrite-core.ts** `:88-91` same pattern after `if (!formatted.content) return ''`.

**search-scriptorium-handler.ts**: `:278 includeCold: true`; metadata `:294 archivedLabel: memoryArchivedLabel(mr.memory)` — key AFTER `source`, BEFORE `occurredAt` (absent for hot rows: `undefined` dropped); `:521-531` post-sort/slice stamp of `limitedResults.filter(sourceType==='memory').map(metadata.memoryId).filter(string)` when `context.characterId`; `:583 memoryArchivedLabel(memory)`: `tier !== 'cold' → undefined`; `supersededById` truthy → `` `(archived — superseded by ${id})` `` (U+2014), else `'(archived)'`; formatter `:614-615` header `[Result ${i} - Memory${archived ? ' ' + label : ''}] (Importance: …`. `search-scriptorium-tool.ts:101-103` only widens the result TYPE (`source` union + `archivedLabel?`) — the tool has no `source` input, so "the tool accepts `CONSOLIDATED`" (ledger) is type-only; its description is unchanged.

**character-optimizer.service.ts** `:738 vectorStore.search(emb, 500, isHotVector)`; `:762-763` post-filter `candidateMemories.filter(m => m.tier !== 'cold')` after all three branches (semantic / text / no-query).

**pre-compute + orchestrator** (`f7d8064be`): `RunPreContextPreComputeOptions.presentParticipantNames?` `:70`; `proactiveRecallTask` threads it into `buildTurnRecallContext` `:316`; `orchestrator.service.ts:1136-1137` builds `[character.name, ...participantCharacters.values().map(c=>c.name)].filter(non-empty)` and passes it `:1153`.

### 5.2 v5 today

- `services/build_context.rs`: archive `:2455`; fallback recall ctx `:2591-2615` (no names); search `:2632`; budgets `:2685-2694`; archive/head format `:2728-2738`; `whispered_memory_ids` `:2740`; no stamp; inter-char `:2842-2900` (`find_by_character_about_characters` `:2849`, relevance search `:2865`); `injector_memory_from_json` `:1120` (no `source` / `consolidatedFrom`); `debug_memory_json` `:3992` already emits `memoryId` first when `Some`; `DebugInterCharacterOut` `:284`.
- `memory_injector.rs`: `InjectorMemory` `:244-265` (no source/consolidatedFrom); `DebugMemoryInfo.memory_id` `:339`; `DebugInterCharacterMemoryInfo` `:353` (no `memory_id`); `memory_id: None` at `:655` (formatMemoriesForContext) and `:921` (archive); line templates `:837` (inter), `:911` (archive), `:1043` (head); inter importance-half sort `~:790-800`.
- `services/first_message_context.rs` `load_for_participant` `:140-243` (no cold filter, no digests partition, no stamp).
- `services/memory_recap/mod.rs:665-679` reads `find_recent_by_importance_tier` (no `digests_first`).
- `services/carina_query.rs` `load_carina_memory_recall` `:1076-1150`; second `injector_memory_from_json` `:1362` (duplicate of build_context's).
- `services/announcer/voice_rewrite_core.rs` `recall_for_seed` `:107-168`.
- `tools/search.rs`: options `:366-380`; results `:383-389`; sort/truncate `:564-565`; `memory_result` `:805-857`; formatter memory arm `~:940`.
- `generators/optimizer.rs:1641-1650` vector search 500 unfiltered + `find_by_character_about_character`; second about-self read `:1807`.
- `services/pre_compute.rs:79` input struct (no names); `services/orchestrator.rs:2344-2376` (no names; `participant_characters: HashMap<String, Value>` `:1557` — iteration order is irrelevant here: names go through a Set, ids are sorted).
- `services/job_runner.rs:483` the completed-job hint site (where v4's `invalidateFrozenArchivesForCompletedJob` hook belongs).

---

## 6. Instruments — recall-replay (API + service + CLI), the F2 anchor-gate probe, the memories list/search API

### 6.1 v4 at HEAD — `lib/memory/recall-replay.ts` (349 lines)

- `:51 REPLAY_EMBEDDING_MEMO_MAX = 500`; `:52` process-lifetime `Map`; `boundedMemo()` `:54` evicts oldest **while `size > 500` at the START of each run** (a run may grow it past 500).
- Result (key order) `:83-111`: `chatId, characterId, characterName, turnIndex, totalTurns, signals, query, clockIso, oldPath, newPath, oldHeadSize, newHeadSize, tuning, signalsPinned, asOf, excludedAfterAsOf`.
- Input `:113-136` adds `memoryBudget?`, `tuning?: RecallTuningInput`, `signals?: MemorySearchExtraction`, `asOf?: boolean`.
- `:205-220` pinned signals → **debug `Recall replay using pinned signals` `{chatId, turnIndex}`** (logger = `createServiceLogger('RecallReplay')`), else distill as before.
- `:232-242` `presentParticipants = participants.filter(status !== 'removed' && characterId)`; `tuning = resolveRecallTuning(input.tuning)`; `tuningSummary = describeRecallTuning(tuning)`; `presentParticipantNames` resolved via `findByIdRaw` **only when `tuning.specificAnchors`**.
- `:248-258` asOf: `openingMessage = turn.messages.find(role==='USER') ?? turn.messages[0]`; `asOfIso = asOf ? opening?.createdAt ?? null : null`; `weightClockMs = asOfIso ? Date.parse(clockIso) : undefined`; `newer = countCreatedSince(id, asOfIso)`; `recent = newer > 0 ? findRecent(id, newer) : []`; `excludeMemoryIds = Set(recent.filter(createdAt >= asOfIso).ids)` (all tiers); **debug `Recall replay corpus cutoff` `{chatId, asOfIso, excluded}`**.
- `:261-266` `newHeadSize = memoryBudget > 0 ? sizeMemoryPools(memoryBudget, retro).headEntries : retro ? 10 : 5`; `limit = input.limit ?? Math.max(25, newHeadSize + 10)`.
- OLD path `:284-292` `{userId, limit, minImportance: 0.3, recallContext: baseCtx, excludeMemoryIds, weightClockMs, embeddingMemo}` — runs under the cap14 defaults (no tuning ever reaches it; TF-IDF → gate off).
- NEW path `:297-316` adds `turnRetrospective`, `...(input.tuning ? {tuning} : {})`, `...(names ? {presentParticipantNames} : {})`, `entityAnchors`, `occurredWithin`, `extraProbes`, `excludeMemoryIds`, `headSize: newHeadSize`, `weightClockMs`, `embeddingMemo`.
- INFO `Recall replay complete` `{chatId, characterId, turnIndex, retrospective, oldCandidates, newCandidates, newHeadSize, tuning, signalsPinned, asOfIso}`.
- `oldPath: toRows(old, 5)`, `newPath: toRows(new, newHeadSize)`, `oldHeadSize: 5`.

**Route `app/api/v1/chats/[id]/actions/recall-replay.ts`** (129 lines): body parse failure → 400 `Body must be JSON`; `memoryBudget` `:55-58` = number, finite, `> 0` → `Math.floor` (so `0.5` floors to 0 and the run treats it as absent); `:60-63 RecallTuningInputSchema.optional().safeParse(body.tuning)` → `validationError` (400 `{error:'Validation error', details: issues}`); `:64-67` signals likewise; `:68-70` `asOf !== undefined && typeof !== 'boolean'` → 400 `asOf must be a boolean` (**`null` included**); **debug `[Chats v1] Recall replay requested` `{chatId, turnIndex, tuning: parsed ?? null, signalsPinned, asOf}`**; then the chat-settings / profile / cheap-LLM ladder runs **even when signals are pinned**; catch → WARN `[Chats v1] Recall replay failed` `{chatId, error}` + 400 message.

**CLI `packages/quilltap/lib/recall-replay-command.js`** (324 lines): help `:21-72` (new `--limit` wording; `--memory-budget`, `--tuning`, `--tuning-file`, `--signals-from`, `--as-of`; two new examples); `--memory-budget` `:106-114` `Number(arg)`, `Number.isInteger && >= 1` else `Error: --memory-budget must be a positive integer`; `loadTuning` `:204` (`Error: pass --tuning or --tuning-file, not both`; `Could not parse ${what} as JSON: ${V8 message}`; `Could not read ${what} ${path}: ${Node fs message}`); `loadSignals` `:218` (`${path} has no saved signals (its distillation failed); replay without --signals-from`, `${path} is a replay of chat X, not Y`, `… of turn N, not M` — `saved.data ?? saved`); body keys in order `turnIndex, characterId, limit, memoryBudget, tuning, signals, asOf`; failure prints each `payload.details` issue as `  ${path.join('.') || '(body)'}: ${message}`; header adds `Harness   signals pinned|fresh distillation · as of X (N later memories left out)|whole corpus` and `Tuning    defaults|<summary> (new path only)`; `printPath(label, rows, headSize)` header `(${n} candidates, head ${headSize})`.

**F2 — `lib/memory/anchor-gate-probe.ts`** (240 lines, `3f7320138`, measurement only, untouched by `d58548`): `PROBE_TOP_K 5`, `fetchK 20`; `limit = min(200, max(1, limit ?? 50))`; sample `findRecent(characterId, limit)` (all tiers); store unfiltered (cold included); embeddings `priority: 'background'`, counted; anchored vs anchor-free text via `buildMemoryEmbeddingText(summary, content[, memory])`; neighbour older iff `createdAt` parse `<` the row's; anchor-free neighbour vector = stored embedding when the neighbour has no anchor line and lengths match, else re-embed (cached); result keys `characterId, sampled, anchoredRows, thresholds{reinforce 0.85, nearDuplicate 0.90}, anchored{reinforce, nearDuplicate}, anchorFree{…}, crossedOnlyWithoutAnchors, embeddingsGenerated, rows[{memoryId, summary, createdAt, hasAnchorLine, anchoredBest, anchoredBestId, anchorFreeBest, anchorFreeBestId}]`; WARN `Probe embedding dimension does not match the store; skipping row` `{memoryId, probe, store}`; WARN `Probe failed for row; continuing` `{memoryId, error}`; INFO `Anchor gate probe complete` `{characterId, sampled, anchoredRows, anchored, anchorFree, crossedOnlyWithoutAnchors, embeddingsGenerated}` (logger `createServiceLogger('AnchorGateProbe')`).
- Route `route.ts:596-633` `POST ?action=anchor-gate-probe`: body `req.json().catch(() => ({}))`; `z.object({characterId: z.string().uuid(), limit: int 1..200 optional})` → `validationError`; `findById` miss → 404 `Character`; debug `[Memories API] Running anchor gate probe` `{characterId, limit}`; success → the result bare (`successResponse` = raw JSON); catch → ERROR `[Memories API] Anchor gate probe failed` `{characterId, error}` + 500 `Anchor gate probe failed`.
- CLI `anchor-probe-command.js` (191 lines): help `:19-44`; flags `--limit` (1..200, `Error: --limit must be between 1 and 200`), `--port`, `--json`, `-h/--help`; unknown → `Unknown option: X`; no id → help + exit 1; two ids → `Error: only one characterId may be specified`; stderr `Probing anchor gate for character X via URL`; `Probe failed (status N): …`; table output `:175-188`. `bin/quilltap.js`: `SUBCOMMANDS` gains `'anchor-probe'` after `'recall-replay'`; top-level help line `anchor-probe <characterId>    Probe whether episodic anchor lines suppress Memory Gate reinforcement` after `recall-replay`.

**Memories list / search API (`route.ts`, `d58548051`)**: list `:298-301` `source` widened to CONSOLIDATED; `tier = 'hot'|'cold'` else undefined; paginated path passes `tier` to `findByCharacterIdPaginated` (KEYSTONE — `findByFilter {tier}` renders `"tier" = ?`, so a NULL-tier row is NOT listed under `?tier=hot`); legacy path `:366-368` `(m.tier ?? 'hot') === tier` (a NULL-tier row IS hot). Search `searchMemorySchema.source` enum gains `CONSOLIDATED` `:96`; `handleSearch` passes `includeCold: true` `:560`.

**Completion templates** (all three commits): `3f7320` adds `anchor-probe` everywhere + recall-replay `--memory-budget`; `f7d8` adds `--tuning --tuning-file --signals-from` (valued) + `--as-of`; `d58548` adds memories `consolidate`, `--tier (hot cold)`, `--dry-run`, `--source … CONSOLIDATED` (memories AND db in fish/zsh), `vf_memories` gains `--tier`.

### 6.2 v5 today

- `services/recall_replay.rs` (449 lines, P4.d13): limit default 25 `:119`; no memo / tuning / signals / asOf / budget; result lacks the 6 new keys; heads hard-coded `:439-446`; header doc still claims "the search path's `lastAccessedAt` bumps" `:10-12`.
- `api/recall_replay.rs` (157): only `turn_index/character_id/limit` `:43-52`; no requested-debug; `Request::ChatRecallReplay` `api/types.rs:2910-2918` (`Option<Value>` — **serde maps JSON `null` to `None`**, which would silently accept `asOf: null` / `tuning: null` / `signals: null` that v4 refuses → use `double_option` as `api/memories.rs:569` does); engine arm `api/engine.rs:1100-1117`; host driver `quilltap-host/src/spine.rs:2816`.
- `api/memories.rs`: `memory_list` `:130` (source filter `AUTO|MANUAL` `:150`; no tier); `memory_search` `:741` (bag `source: Option<String>` unvalidated — pre-existing leniency; no `include_cold`); `Request::MemoryList` `types.rs:1618` (no `tier`); no anchor-probe.
- CLI `quilltap-cli/src/main.rs:47-61 SUBCOMMANDS` (no anchor-probe), dispatch `:246-254` (`memories` → `not_yet_available` CONFIRMED — the verb is recognised only); `recall_replay_cmd.rs` (451) at the BASELINE help (no `--memory-budget`), `print_path(label, rows)` `:162` (no head), posts `/api/dispatch` `chatRecallReplay` `:291`; `help/main_help.txt` (no anchor-probe line); `help/completion/{bash,zsh,fish}.template` at baseline; tests `tests/cli_differential.rs:4472-4550` (recall-replay cases + canned stub `:316`), `tests/completion_behavior.rs` (help-flags ⊆ templates guard; `subcommands()` parses `SUBCOMMANDS` — no comments inside the array).
- Helpers to reuse: `quilltap_core::jsstr::v8_json_parse_message` (`jsstr.rs:202`, P4.154) for `--tuning` parse errors; `jsnum::number_from_str` for `Number(arg)`; `Response::validation_error(details)` (`types.rs:4726`) for the zod refusals.

---

## 7. DRAFT KEYSTONE contract (what this cluster consumes from the data-layer lane)

Name-level; the keystone owns `db/memories_read.rs`, `db/memories.rs`, `db/vector_store.rs`, the schema/D23/boot-ensure, and fixture upgrades.

1. **Row keys**: every memory read returns `tier` (`'hot'|'cold'`, may be NULL on unhealed rows), `supersededById`, `consolidatedFrom` (hydrated JSON array), `consolidatedAt`. Consumers test cold as `get("tier") == Some("cold")` (JS `m.tier !== 'cold'` semantics: NULL/absent = hot).
2. `memories_read::HOT_TIER_SQL = "COALESCE(tier, 'hot') = 'hot'"` (pub const).
3. `find_most_important(conn, character_id, limit)` → F4: `WHERE characterId = ? AND HOT ORDER BY reinforcedImportance DESC, COALESCE(lastReinforcedAt, createdAt) DESC, id ASC LIMIT ?`; `limit <= 0 → []`; debug `Fetched most important memories {characterId, limit, returned}`; per-row validation drop WARN `Memory failed validation in most-important fetch {memoryId, error}`.
4. `find_hot_digests(conn, character_id, subject: DigestSubject /* Self_ | Any | About(&str) */, limit) -> Vec<Value>`: `source = 'CONSOLIDATED' AND HOT`, `self → aboutCharacterId = characterId`, same ORDER BY as (3), `limit <= 0 → []`; validation WARN context `hot digest fetch`.
5. `find_recent_by_importance_tier`: each of the three reads adds **`tier = 'hot'` (exact equality — v4's `findByFilter`, NULL excluded)**, not `HOT_TIER_SQL`.
6. `find_by_character_about_characters`: `HOT_TIER_SQL` inside the ranked CTE (the per-character `rn` counts hot rows only).
7. `find_by_character_id_paginated(..., tier: Option<&str>)` → `"tier" = ?` exact.
8. `find_cold_ids_by_character_id(conn, character_id) -> Vec<String>` (`tier = 'cold'`).
9. `CharacterVectorStore`: per-entry tier stamped at `load` from (8) (failure → everything hot + WARN `Could not read cold memory ids; treating every vector as hot {context:'CharacterVectorStore.load', characterId, error}`; debug `Loaded vector index {context, characterId, entries, cold}`), plus `pub fn search_filtered(&self, query, limit, filter: impl Fn(&str /*id*/, bool /*is_cold*/) -> bool)` applied BEFORE scoring/top-K (v4 `searchLinear`/`searchHeap` skip filtered entries first), and `pub fn is_hot(id)`. (If the keystone declines, lane R1 can own `vector_store.rs`'s `search_filtered` — but `set_tier` for housekeeping/consolidation must live in the same file.)
10. `memories::update_access_time_bulk` unchanged (HEAD `memories.repository.ts:715`, untouched by all three commits).
11. **Fixture upgrade** — every committed memory-bearing `.db` fixture the families below open (`episodic-recall-{main,mount}.db`, `memories-{main,mount}.db`, `character-generators-{main,mount}.db`, …) must carry the four `memories` columns (via `runV4Migrations` in the builders), or `HOT_TIER_SQL` errors ("no such column: tier") on BOTH sides.
12. `MemorySource` validators accept `CONSOLIDATED` (P4.161's ported `MemorySchema`).

---

## 8. Proposed lane split (FILE-DISJOINT) + unit decomposition

Order: **KEYSTONE → R1 → (R2 ∥ R3)**. R1 can start beside the keystone against §7 as a HANDOFF contract, but its tier-3 gate needs (9) + (11).

### Lane R1 — "recall engine" (pure ranking + `searchMemoriesSemantic`)

Owns: `crates/quilltap-core/src/recall_tags.rs`, NEW `recall_tuning.rs`, NEW `memory_pools.rs` (v5 home for `sizeMemoryPools` + the seven new constants + `PROACTIVE_RECALL_POOL_SIZE` — imports the four existing floors from `memory_injector.rs` read-only, so R2 keeps `memory_injector.rs` whole), `services/memory_service.rs`, `lib.rs` (two `mod` lines); harness `tests/recall_tags_equivalence.rs`, NEW `tests/recall_tuning_equivalence.rs`, NEW `tests/semantic_search_tuning_tier3_equivalence.rs`; oracle `harness/oracle/cases/recall-tags.ts`, NEW `recall-tuning.ts`, NEW `semantic-search-tuning-tier3.test.ts`.

1. **R1.1** `recall_tuning.rs` pure: `ResolvedRecallTuning`, `RecallMultiplierTable` (12 fields, table order), `DEFAULT_RECALL_TUNING`, `gate_term`, `resolve_recall_tuning`, `select_specific_anchors`, `describe_recall_tuning`, and the two zod-faithful validators `validate_recall_tuning_input(&Value) -> Result<RecallTuningInput, Value /*issues*/>` / `validate_recall_replay_signals` (the issue arrays byte-exact; signals output in SHAPE key order, unknown keys stripped). Tier-1 vs v4's real functions + `safeParse`.
2. **R1.2** `recall_tags.rs`: fresh 1.3/1.15; multipliers param on the 7 fns; `BoostGate`, `boost_gate_threshold`, `boost_gate_strength`, `RECALL_TUNING_DEFAULTS`, `recall_tuning_of`, `RecallRelevance`; `combine_recall_multipliers(memory, ctx, relevance: Option<RecallRelevance>)` with the boost/penalty split + `cap{n}` / `gate×{n.toFixed(2)}` labels (owned strings); `RecallContext.{tuning, present_participant_names}`. Tier-1: regrow `recall-tags.ts` (existing combine rows + new relevance/tuning/bypass rows + threshold/strength rows).
3. **R1.3** `memory_pools.rs` `size_memory_pools` (+ `MemoryPoolSizing`, constants, `PROACTIVE_RECALL_POOL_SIZE = 90`). Tier-1 (grid incl. 0, 1, 1999, 2000, 2001, 8000, 8333, 1e6, fractional).
4. **R1.4** `memory_service.rs` options: `exclude_memory_ids`, `head_size`, `weight_clock_ms`, `embedding_memo: Option<Arc<Mutex<EmbeddingMemo>>>` (key `profile|text`, also used by probes), `include_cold`; `RecallContextInput.{tuning, present_participant_names}`; the hot/exclude vector filter (§7.9); hydrate cold drop; text-fallback cold filter (text path keeps the WALL clock); expansion exclude/cold/relevance/weight-clock.
5. **R1.5** the ranking knobs inside `try_semantic`: TF-IDF gate-off + its debug line; R4 `hits_for` cache + counted/chosen + its debug line; the non-R4 filter→slice order fix; raw-cosine map; `best_cosine`; relevance into every `combine`; R6 `reserve_background_slots` (pure, tier-1) + its debug line.
6. **R1.6** `pub async fn mark_memories_accessed(db: &Db, character_id: &str, memory_ids: &[String])` (dedup first-seen, empty-string drop, debug/WARN lines, non-fatal) and delete both internal bump sites (`:430`, `:444`) + `bump_access_times`. **HANDOFF**: until R2 lands, `announcer_tier3` / `in_scene_voiced_tier3`'s `bumped` dumps differ (search no longer stamps; the consumer stamp arrives in R2) — record as "red until R2", resolved at union.
7. **R1.7** NEW tier-3 `semantic_search_tuning_tier3_equivalence`: v4's REAL `searchMemoriesSemantic` over a planted copy of the episodic fixture with a scripted embedding mock that reports a NON-`BUILTIN` provider (to exercise R1) and one BUILTIN case (gate-off line); cases mirror `memory-service-recall-tuning.test.ts` (exclude, weight clock, R4 counts/present names/zero-hit, R6 reserve under a tuning, memo hits, R5 floor) + `memory-service-hot-tier.test.ts` (hot filter default, literal hit can't smuggle cold, includeCold, cold+exclude); emits results + recallAdjustment + the debug lines.

### Lane R2 — "context delivery" (every consumer that formats or stamps)

Owns: `memory_injector.rs`, `services/frozen_archive.rs`, `services/build_context.rs`, `services/pre_compute.rs`, `services/orchestrator.rs` (the names hunk only), `services/first_message_context.rs`, `services/carina_query.rs`, `services/announcer/voice_rewrite_core.rs`, `services/memory_recap/mod.rs`, `tools/search.rs`, `generators/optimizer.rs`, and (if no other lane claims it) the one-call hook in `services/job_runner.rs:483`; harness `memory_injector_equivalence.rs` + `harness/oracle/cases/memory-injector.ts`, NEW `frozen_archive_tier2_equivalence.rs` + oracle, and the growth of `first_message_context`, `search_tools`, `precompute`, `carina_query_tier3`, `announcer_tier3`, `in_scene_voiced_tier3`, `character_optimizer_tier3` families (+ their oracle cases).

1. **R2.1** `memory_injector.rs`: `format_digest_provenance`; `InjectorMemory.{source: Option<String>, consolidated_from_len: usize}` (fold both `injector_memory_from_json` copies — `build_context.rs:1120`, `carina_query.rs:1362` — into ONE `pub(crate)` here: DRY); suffix at the three sites only; `memory_id` Some at `:655` / `:921` + `DebugInterCharacterMemoryInfo.memory_id`; inter importance-half digest sort. Tier-1: regrow `memory-injector.ts` (provenance rows incl. 0/1/14 notes and non-digest; debug memoryId; inter digest sort).
2. **R2.2** `frozen_archive.rs`: key `(character, chat)`, freshness generation+size+presence (presence = deduped, responder-dropped, **code-unit-sorted**, `,`-joined), LRU 64 insertion-ordered, digest composition (§7.4 + F4 §7.3), the two debug lines, `invalidate_frozen_archive` (all chats) + `invalidate_all_frozen_archives` + `pub fn invalidate_frozen_archives_for_completed_job(job_type: &str, payload: &Value)` (types `MEMORY_HOUSEKEEPING`/`MEMORY_CONSOLIDATION`, dryRun skip, characterId else all, v4's debug line). Rust unit tests mirror `frozen-archive-cache.test.ts` (16 cases incl. the five `digest composition`) + `job-dispatcher-frozen-archive.test.ts` (4); NEW tier-2 family drives v4's real `getOrComputeFrozenArchive` over a planted fixture (digests about present characters, self digests, cold rows, a digest also in the fill pool, size cap) and compares the returned ids/order.
3. **R2.3** `build_context.rs`: sizing (ordinary/retro), archive call with `chat.id` + present ids + `archiveSize`, fallback limit, budgets + `max_entries`, the pools debug line, `mark_memories_accessed` for archive+head, inter-char digests (2 per other, first-seen dedup) + loadedCount + inter stamp, fallback `present_participant_names`.
4. **R2.4** `pre_compute.rs` (`PROACTIVE_RECALL_POOL_SIZE`, no `take(10)`, `ProactiveRecallInput.present_participant_names`) + `orchestrator.rs` (build + thread the names).
5. **R2.5** `first_message_context.rs` (cold filter, digests-first take-3, text-fallback cold skip, per-participant stamp) + `memory_recap/mod.rs` (`digests_first` on high).
6. **R2.6** Carina + voice-rewrite stamps; `tools/search.rs` (`include_cold: true`, `archivedLabel`, header label, post-limit stamp); `optimizer.rs` hot-only (vector filter + post-filter on all three branches).
7. **R2.7** job_runner hook call (or HANDOFF to its owner): after `topics_for_completed_job` at `:483`, call `invalidate_frozen_archives_for_completed_job`. Plus the direct calls belong to the housekeeping / dedup / consolidation lanes (§10).

### Lane R3 — "instruments" (replay, F2 probe, memories list/search API, CLI)

Owns: `services/recall_replay.rs`, `api/recall_replay.rs`, NEW `services/anchor_gate_probe.rs` (+ `services/mod.rs` line), `api/memories.rs` (**only** `memory_list`, `memory_search`, the NEW anchor-probe handler — see §10 for the shared-file problem), the cluster's `api/types.rs` variants (`ChatRecallReplay` growth, NEW `MemoryAnchorGateProbe`, `MemoryList.tier`) + their `api/engine.rs` arms (append-only `// === … ===` blocks), `quilltap-host/src/spine.rs` driver (if the input struct grows), `quilltap-cli/src/{main.rs, recall_replay_cmd.rs, NEW anchor_probe_cmd.rs, help/main_help.txt, help/completion/{bash,zsh,fish}.template}`, `quilltap-cli/tests/{cli_differential.rs, completion_behavior.rs}`; harness `recall_replay_equivalence.rs`, `memories_routes_equivalence.rs`, NEW `anchor_gate_probe_tier3_equivalence.rs` + oracle cases.

1. **R3.1** route: `memory_budget` coercion; tuning/signals validation via R1.1 (double-option so `null` refuses); `asOf` 400; the requested debug line; types + engine arm.
2. **R3.2** service: memoryBudget/newHeadSize/oldHeadSize; default limit; resolve/describe; pinned signals + its debug line; asOf (count → findRecent → `>=` filter, all tiers) + weight clock + cutoff debug line; names when `specific_anchors`; static bounded memo (500, trimmed at run start); new-path options; INFO fields; the six new result keys in v4's order. Grow `recall_replay_equivalence` (memoryBudget 0.5/2000/8000, tuning strict refusal + nested `multipliers` unknown key, signals pinned + stripped key, asOf with planted newer rows, `asOf: null` 400).
3. **R3.3** F2 probe service + route arm (uuid gate per P4.162's RFC helper; 404/500 + lines). NEW tier-3 family (BUILTIN embeddings — deterministic).
4. **R3.4** `memory_list` `tier` (both paths, keeping v4's paginated-exact vs legacy-NULL-is-hot split) + `CONSOLIDATED`; `memory_search` `include_cold: true` + `CONSOLIDATED`. Grow `memories_routes_equivalence`.
5. **R3.5** CLI recall-replay (five flags, help, loadTuning/loadSignals, details lines, header lines, `head N`); NEW anchor-probe command; `SUBCOMMANDS` + dispatch + main help; the three templates **whole** (including `d58548051`'s memories `consolidate` / `--tier` / `--dry-run` / `CONSOLIDATED` lines — the templates must not be split with the consolidation cluster); `completion_behavior.rs` help-source row for anchor-probe; Tier R cases (help, no-id, two ids, bad limit, unknown option, table, json, server error, unreachable for anchor-probe; `--memory-budget` `abc|0|1.5|1e3|0x10`, `--tuning` bad JSON, both tuning flags, missing tuning file, signals-from mismatch chat/turn/no-signals, `--as-of`, Harness/Tuning header, a 400 with `details`) with the stub server answering both v4's REST URL and v5's `/api/dispatch`.

---

## 9. Differential plan — families predicted to redden at `01a83539d` (this cluster's causes only)

| family (env) | why | confidence |
|---|---|---|
| `recall_tags_equivalence` (`QT_ORACLE_RECALL_TAGS`) | fresh 1.3/1.15; cap 1.4 + `cap1.4` on existing combine rows | certain |
| `precompute_equivalence` (`QT_ORACLE_PRECOMPUTE`) | limit 90 + no slice(10); cap; fresh; R4 anchor choice; gate-off line | certain |
| `recall_replay_equivalence` (`QT_ORACLE_RECALL_REPLAY`) | six new result keys; default limit `max(25, head+10)`; cap/fresh/R4 on new path | certain |
| `memory_injector_equivalence` (`QT_ORACLE_MEMORY_INJECTOR`) | debug `memoryId` on archive / formatMemories / inter-char | likely |
| `announcer_tier3_equivalence`, `in_scene_voiced_tier3_equivalence` (`bumped` dump) | search stops stamping; `recallForSeed` stamps only delivered rows | likely (when results > delivered) |
| Tier R `cli_differential` | recall-replay help/table, launcher help (anchor-probe line), completion bash/zsh/fish | certain |
| `search_tools_equivalence`, `first_message_context_equivalence`, `carina_query_tier3_equivalence`, `character_optimizer_tier3_equivalence`, `memories_routes_equivalence`, `build_context_tier3_equivalence`, `orchestrator_tier3_equivalence` | no cold rows / digests / memories in today's corpora → green on THIS cluster's account (the keystone's four new row keys + fixture re-dump redden the routes/read families on its own account) | likely green |

New families: `recall_tuning_equivalence` (tier-1), `semantic_search_tuning_tier3_equivalence` (tier-3, scripted non-BUILTIN embeddings), `frozen_archive_tier2_equivalence`, `anchor_gate_probe_tier3_equivalence`; growth rows listed per unit above. v4 jest tests to mirror: `recall-tuning.test.ts` (27 cases), `recall-tags.test.ts`, `recall-tags-retrospective.test.ts`, `memory-service-recall-tuning.test.ts` (15), `memory-service-hot-tier.test.ts` (4), `memory-recall-housekeeping-fixes.test.ts` §F7 (5), `frozen-archive-cache.test.ts` (16), `job-dispatcher-frozen-archive.test.ts` (4), `digest-provenance.test.ts` (2), `memory-recap.test.ts` "puts consolidated digests ahead…", `search-scriptorium-archived-label.test.ts` (4), `search-scriptorium-handler.test.ts` (stamp), `first-message-context.test.ts`, `pre-compute.service.test.ts`, `recall-replay.test.ts`, route `recall-replay.test.ts` (R7 block, 5), `context-management.test.ts`.

---

## 10. Meeting points with other clusters

- **KEYSTONE (data layer)**: all of §7. Biggest dependency is §7.11 (fixtures carry the tier columns) — without it every recall family errors on `COALESCE(tier,…)` on both sides.
- **Housekeeping / dedup / consolidation lanes** consume R2's `invalidate_frozen_archive(character_id)` (`housekeeping.rs` when the vector store was dirtied, `memory_dedup.rs` non-dry-run, the consolidation executor) and `invalidate_all_frozen_archives()`; the job-runner hook (`invalidate_frozen_archives_for_completed_job`) sits in whichever lane owns `services/job_runner.rs` this round (R2 by default). The consolidation engine does NOT call `searchMemoriesSemantic` (no caller in `consolidation*.ts`); its tier moves use the keystone's `set_tier` (§7.9).
- **Gate cluster**: none from this cluster (the gate reads both tiers via the store directly; F1/F3/bug 182 are its own).
- **Fold-OTHER / extraction cluster**: none.
- **Carriers**: none (the list/search API is R3; import/restore validate `CONSOLIDATED` through the keystone's `MemorySchema`).
- **SPA cluster** consumes R3's `MemoryList.tier` dispatch field (`apps/web/src/app/core/core-contract.ts:5757` + `memory/memory.api.ts:55`) and `MemorySearch` with `source: CONSOLIDATED`.
- **Shared files needing an explicit ruling**: `api/memories.rs` is ALSO edited by the housekeeping cluster (`memory_housekeep*` gain `demoted`/`coldCount`/`demotedIds`/`wouldDemote`) and the consolidation cluster (`consolidate`, `consolidation-config`, `extraction-mode-config`) — either one owner or strictly disjoint fn regions; `api/types.rs` + `api/engine.rs` are edited by every cluster that adds a verb (append-only marked blocks, per the frozen-engine-seam note); `quilltap-cli` completion templates must be owned by ONE lane (R3) even though `d58548051`'s memories lines belong to the consolidation feature; `services/mod.rs` / `lib.rs` one-line `mod` adds.
- **Help/docs vendor lane**: `help/memory-recall-relevance.md` (all three commits), `help/episodic-memory.md` (`f7d8`), `help/memory-housekeeping.md`, `docs/v4/…/API.md`, `CLI.md` — not R1–R3 files.

---

## 11. Open questions / measurements before coding

1. Does a NULL `tier` cell reach v4's JS as `null` and FAIL `MemorySchema` (`tier: MemoryTierEnum.default('hot')` defaults only `undefined`) — i.e. is a NULL-tier row dropped rather than "hot"? Decides whether `HOT_TIER_SQL`'s COALESCE and the JS `!== 'cold'` checks ever see NULL. (Keystone measures; affects R3.4's two list paths.)
2. Zod 4 issue bytes for `RecallTuningInputSchema` (top-level and nested `multipliers` `unrecognized_keys`, `boostCap` out of range, `anchorOrder` enum, `null` tuning) and `RecallReplaySignalsSchema` (missing `keywords`); and whether the parsed `signals` echoed in the response is in SHAPE key order with unknown keys stripped. Capture from the oracle.
3. `createServiceLogger('RecallReplay' | 'AnchorGateProbe')` structured-key layout (the `context`/`service` key name and position) for the capture-pinned lines.
4. `jsnum::to_fixed` tie behaviour on the `gate×` label (e.g. strength exactly 0.125/0.625 — JS rounds half up on the exact decimal value).
5. v5's dispatch error envelope: does a `validation_error` reach the CLI as `payload.details` so v4's per-issue lines print identically through `/api/dispatch`?
6. Whether the `debugLLMRequest` SSE frame (`services/chat_events.rs`, `orchestrator.rs:3185`) now carries `memoryId` on archive entries — it will (via `debug_memory_json`), matching v4; confirm no SPA contract type pins the old shape.
7. The embedding memo in v5: `EmbeddingResult` must be `Clone` (carrying the provider label the gate-off reads); key uses the profile id or `'default'`.
8. `recall-replay` under `--signals-from`: v4 still requires chat settings + a profile (400s otherwise) — keep that order in v5.
9. Ownership of `services/job_runner.rs` and `api/memories.rs` this round (see §10).
