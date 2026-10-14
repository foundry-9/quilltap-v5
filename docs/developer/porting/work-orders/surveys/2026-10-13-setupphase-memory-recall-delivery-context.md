# Survey: memory RECALL, DELIVERY, and the context build's memory and trailing sections

This is a read-only planning survey, dated 2026-10-13.

- **v4 pin:** `64743ee13` (`4.10.0-dev.175`). v4 declares the memory refactor CLOSED here.
- **Oracle baseline:** `4a38222e6`. This is the shape v5 ported.
- **v5:** `main` at `a7c6f0d03`.

**How v4 was read.** Only through `git show 64743ee13:<path>` (the pin), `git diff 4a38222e6 64743ee13 -- <paths>`
(the span) and `git show 4a38222e6:<path>`, which the line numbers mark `(pre)`. Every other v4 line number is AT THE PIN.

**Source rule.** Nothing here comes from commit prose (ledger §5.3). The drift ledger's §1 and its §3 rows for
`584d28164`, `124c8aca3`, `69d38dd60`, `93c5af6ad`, `4ae0c4ca6` and `64743ee13` were read first. They are not restated
here. Where the hunks disagree with them, §0.1 says so.

> ⚠ **The v4 checkout is DIRTY.** The brief says it is clean, but `git status --short` in `~/source/quilltap-server`
> shows nine modified files: `app/api/v1/chats/[id]/actions/{impersonation-voice-preview,inform,rebuild-summary,
> story-background}.ts`, `…/handlers/get.ts`, `connection-profiles/[id]/route.ts`, `memories/route.ts`,
> `plugins/[name]/route.ts`, `lib/mount-index/scenario-item-route-factory.ts`.
>
> Nothing in this survey reads the working tree. Every regen still needs the pinned worktree (§5.1 PIN REQUIRED), and
> the next `/driftcheck` should record the dirt.

**In scope:**

- `lib/memory/recall/*` (all 19 modules at the pin);
- `lib/memory/scoring/rank.ts` (NEW in Phase E);
- `lib/chat/context-manager.ts`'s recall, trailing and mail regions;
- the DELETED `lib/chat/context/memory-injector.ts` and `lib/memory/memory-service.ts` (their recall halves);
- `lib/chat/context/scene-state.ts` (the move target);
- the delivery writers: the finalizer's recall record, `mailbox.markMailAlertsDelivered`, and the preserve-partial,
  tool-only and turn-skip marks;
- the proactive pass (`pre-compute.service.ts`) and `context-builder.service.ts` threading;
- recall's other consumers: Carina, the voice rehearsals, the first-message greeting, the `search_scriptorium` and
  `self_inventory` tools, and the Memories API search;
- `recall-replay` (service, route and CLI);
- the probe fixture.

**Out of scope (NEEDS from siblings, §7):**

- the scoring maths (`salience`, `reinforcedImportance`, `filterImportance`), which is Phase A;
- the time and calendar module (`lib/memory/time/*`, bug 212);
- the index store (`memoryIndex.search/shape`), which is Phase B;
- the repository read changes (bug 215's recap bands);
- admission, extraction, maintenance, the carriers, and the schema keystone;
- the anchor probe (§1.14 — Phase E does not touch it).

---

## 0. The span for this slice

Six of the nineteen commits matter here.

**`124c8aca3` (bug 199)** put recall, mail and the Core whisper on the no-new-user-message branch.

**`69d38dd60` (bug 227) rewrote that an hour later:**
- ONE `trailingContextSections` list serves both branches.
- The mail `alerted` flip is deferred to delivery (`mailAlertsOwed`).
- A swipe skips the mail check.

**`c2795733f` landed the targeted recall fixes ahead of Phase E:**
- bug 212's one memory calendar (the distiller's TODAY line and `timeRange` normalisation);
- bug 214's text fallback honouring every recall option;
- bug 215's recap bands on `reinforcedImportance`;
- bug 219 (GET `/memories/[id]` no longer stamps);
- bug 224 (the inter-character section deduped against the archive and the head).

**`e0ac7493a` (Phase A)** renamed `effectiveWeight`/`rawWeight` to **salience** throughout. Recall consumes that. It
also set every prompt path's importance floor to `RECALL_MIN_IMPORTANCE = 0.3`.

**`584d28164` (Phase E) is the bulk of this slice:**
- It rebuilt recall as `query → gather → hydrate → score → select → assemble → render → deliver` under
  `lib/memory/recall/`, and deleted `memory-service.ts` and `memory-injector.ts`.
- `context-manager.ts` went from 2,891 lines to 2,372.
- There is **one query embedding per turn**, and one search feeds every pool. Cross-pool dedup is enforced: archive,
  then head, then inter-character.
- **Every model-visible recall line lost its debug marginalia.** The `[m_xxxx]` tags; the importance, relevance and
  weight numbers; the `recall:` labels; and the targeting tags in the keyword list are all gone.
- A new delivery stage, run AFTER the request body is built:
  - stamps `lastAccessedAt` and appends the ring only for lines still in the text;
  - writes a `[Recall]` trace record to the assistant message's `debugMemoryLogs`.
- `recall-replay` grew `fallbackPath`, `ring`, `ringSource`, `pools` and `trace`, and its rows say `salience` instead of
  `rawWeight`.

**`4ae0c4ca6`** swapped `score.ts`'s `provider === 'BUILTIN'` test for `isLexicalEmbeddingProvider` (the same set
today). **`ac6348c5f`** moved `extractMemorySearchKeywords` into `recall/query.ts` and the recap summariser into
`recall/recap-summary.ts`, both verbatim. **`93c5af6ad` and `64743ee13`** touch this slice only through the CLI's
`--instance` flags (bug 239) and the turn transcript.

### 0.1 Corrections to the ledger (from the hunks)

1. **The Memories API search behaves as it did.** §1 and the `584d28164` row say it "stamps nothing". It already
   stamped nothing at the baseline: the `(pre)` `route.ts` had only a stale comment claiming `searchMemoriesSemantic`
   bumped access times, and `searchMemoriesSemantic` had stopped stamping since `3f7320138`.
   - At the pin, the hunk is a comment plus `minImportance: minImportance ?? TOOL_MIN_IMPORTANCE`. That constant is 0,
     the same as no filter.
   - v5's `api/memories.rs:905-930` already stamps nothing. Its rows move only through the ranking change (salience)
     and the text-fallback change.
2. **There is one preserve-partial site, not two.** The `69d38dd60` row says the orchestrator passes `mailAlertsOwed`
   "to the preserve-partial options at both sites". At the pin there is ONE `makePreservePartialOnError` construction
   (`orchestrator.service.ts:1480-1491`, `:1490`). The other is the `handleTurnSkip` parameter (`:1731`, `:2081`,
   `:2104`). The four `via` values stand.
3. **Line numbers for the v5 site of bug 199.** The `124c8aca3` row cites `services/build_context.rs:3937-3955`. On
   `a7c6f0d03`:
   - the trailing-only branch is **`:3934-3966`**;
   - the new-message list it must merge with is **`:3875-3911`**.
4. **What the keyword tag actually loses.** The `584d28164` row says "targeting keywords" are lost. Precisely, the
   keyword list now passes through `humanKeywords`, which drops every keyword equal to a TEMPORAL value, a CONTEXT value
   or `scope: <narrow|wide>`.
   - That also drops a GENUINE keyword spelled `history`, `banter`, `trivia`, `information`, `philosophy`,
     `relationships` or `mannerisms`.
   - Ordinary keywords stay. The tag is spelled ` _(keywords: …)_` and appears only when one survives.
5. **The archive's token budget changed, not only its composition.** On a retrospective turn the `(pre)` archive
   budget was `turnSizing.archiveTokenBudget`, i.e. budget minus the RETRO head tokens. The archive therefore shrank on
   retro turns, which broke its byte stability.
   - At the pin it is always `ordinarySizing.archiveTokenBudget`.
   - The head instead borrows `min(retroHead, max(ordinaryHead, memoryBudget − archiveTokens))`.
   - P4.D268 ported the `(pre)` rule (`build_context.rs:2697`).
6. **The anti-repetition ring is no longer gated on the whisper post.** `(pre)` wrote the ring inside
   `if (personaWhisper) … if (posted)`; v5 still does (`build_context.rs:3620-3645`).
   - At the pin, `deliverRecall` writes it whenever `character.id && llmRecallText`. A failed whisper post no longer
     costs the ring.
   - A turn whose only recall section is the retrospective mini-recap (empty persona whisper) now records its
     signature, which `(pre)` silently skipped.

---

## 1. v4 AT THE PIN — the modules

Line counts are at the pin. Every function listed is exported unless noted. Log lines are VERBATIM, with level and
fields in emission order.

### 1.1 `lib/memory/recall/types.ts` (112 lines; stage contracts, pure)

- `HitSource = 'vector' | 'probe' | 'anchor' | 'literal' | 'text'` (`:18`).
- `RawHit { id; score; via: HitSource; memory?: Memory }` (`:26-32`). `memory` is present when the strategy already
  read the row (anchors and the text fallback).
- `HydratedHit { memory; rawScore; score; usedEmbedding }` (`:35-42`). `rawScore` is the cosine as gathered, which R1
  reads. `score` is the same value after the tool's literal boost.
- `RecallAdjustment { multiplier; fired: string[]; blendedBefore; blendedAfter }` (`:45-54`).
- `RecallCandidate { memory; score; usedEmbedding; salience; recallAdjustment? }` (`:60-69`). **This replaces
  `SemanticSearchResult`.** `effectiveWeight` and `rawWeight` are GONE, and `salience` is reinforcedImportance × decay
  at the search clock.
- `RecallStage = 'query'|'gather'|'hydrate'|'score'|'select'|'assemble'|'render'|'deliver'` (`:72-80`).
- `RecallTraceEntry { stage; label?: string; detail: Record<string, unknown> }` (`:83-88`); `RecallTrace = …[]`.
- `traceStage(trace, stage, detail, label?)` (`:93-100`) pushes `label ? {stage,label,detail} : {stage,detail}`. Key
  order is `stage, label, detail`, and `label` is ABSENT, not null, when unset.
- `RenderedLine { memoryId; line; body }` (`:103-109`).
- `RecallPool = 'archive'|'head'|'interCharacter'|'recap'|'relevant'` (`:112`).

### 1.2 `lib/memory/recall/query.ts` (525 lines)

**`:1-338`: `extractMemorySearchKeywords`, moved verbatim** from `cheap-llm-tasks/memory-tasks.ts` (pre `:1444`) in
Phase D.
- The prompt (`:46-83`) is byte-identical to pre `:777-814` (diffed).
- The only behavioural hunks are bug 212's, from the time slice:
  - TODAY is `zonedDateStamp(nowMs, memoryTimeZone)` + `WEEKDAYS[zonedParts(...).weekday]`. It was
    `localDateStamp(new Date(nowMs))`.
  - `memoryTimeZone = clock?.timeZone ?? await refreshMemoryTimeZone()`.
  - The deterministic scan is `resolveReferenceWindow(text, { timeZone, nowMs })`. It was
    `resolveDayReference(text, new Date(nowMs))`.
  - `timeRange` goes through `normaliseTimeRange({from,to},{timeZone})`, which gives local-day bounds. Pre used the
    `/^\d{4}-\d{2}-\d{2}/` + `T00:00:00.000Z`/`T23:59:59.999Z` UTC widening.
- `ExtractionClock` now carries `timeZone`.
- The debug lines are unchanged:
  - `[MemorySearchKeywords] Day reference is future-pointing; leaving LLM signals alone` `{chatId, characterId, matched}`;
  - `[MemorySearchKeywords] Deterministic day reference resolved` `{chatId, characterId, matched, from, to,
    overrodeLlmTimeRange, llmRetrospective}`.
- The task name is `'memory-keyword-extraction'`. `latency` defaults to `'background'`.
- `cheap-llm-tasks/index.ts:37` re-exports `extractMemorySearchKeywords` and `MemorySearchExtraction` from
  `../recall/query`.

**Stage 1, new in `584d28164`:**

- `EmbeddedText { text; embedding: EmbeddingResult | null }` (`:345-348`).
- `RecallQuery` (`:354-370`): `{ text, embedding: EmbeddingResult|null, probes: EmbeddedText[], signals:
  MemorySearchExtraction|null, anchors: string[], window: {from,to}|null, retrospective: boolean, embeddingError? }`.
- `BuildRecallQueryInput` (`:373-403`): `{ userId, embeddingProfileId?, text, signals?, distill?: { messages,
  characterName, selection, chatId?, characterId?, clock?, latency? }, embeddingMemo?, skipEmbedding?, probes? }`.
- `recallQueryText(signals, fallback)` (`:406-410`) returns the paraphrase, else `keywords.join(' ')` when there are
  keywords, else the fallback.
- **`buildRecallQuery(input, trace = [])` (`:419-525`)** never throws:
  1. `distilled` is `'given'` (signals were handed in), `'ran'`, `'failed'` or `'none'`. It distils only when no
     signals were given, `input.distill` is set and `input.text.trim()` is non-empty.
  2. On a throw it logs DEBUG `[Recall] Query distillation failed; using the recent-window text`
     `{characterId, error}`.
  3. `text = recallQueryText(signals, input.text).trim()` and `retrospective = signals?.retrospective === true`.
  4. Probe texts are `input.probes ?? buildRetrospectiveProbes(signals, retrospective) ?? []`, trimmed, non-empty,
     `.slice(0, 2)`.
  5. The memo key is `` `${embeddingProfileId ?? 'default'}|${value}` ``. Each embed goes through
     `generateEmbeddingForUser(value, userId, embeddingProfileId)`, which passes NO priority option.
  6. When `text && !skipEmbedding`, it embeds the main text. A failure logs WARN `[Recall] Query embedding failed;
     recall falls back to text search` `{userId, query: text.substring(0,100), error}` and sets `embeddingError`.
  7. **Probes are embedded ONLY when the main embed succeeded.** A probe failure pushes `{text, embedding: null}` and
     logs DEBUG `[Recall] Probe embedding failed; skipping probe` `{probe: probe.substring(0,80), error}`. With no main
     embedding, every probe text is pushed with `embedding: null`, because the text fallback still reads the probe
     words.
  8. `anchors = (signals?.entities ?? []).map(trim).filter(len>0)`; `window = signals?.timeRange ?? null`.
  9. Trace `query` `{distilled, textLength, embedded, embeddingCalls: embedding ? 1 + probes with embedding : 0,
     probes: probes.length, anchors: anchors.length, window, retrospective}`.

### 1.3 `lib/memory/recall/gather.ts` (303 lines)

- `GatherOptions` (`:32-47`): `{ limit, includeCold, excludeIds?, literalPhrase?, tuning?, presentParticipantNames?,
  minScore?, forceText? }`.
- `Gathered` (`:49-60`): `{ strategy: 'vector'|'text', hits: RawHit[], minScore, provider: string|null, literalPhrase:
  string|null, literalHitIds: ReadonlySet<string> }`.
- `dimensionMismatchWarned` (`:67`): a module-level `Set`, keyed `` `${characterId}:${dominantDimensions}->${queryDims}` ``.
- `TEXT_SEARCH_STOP_WORDS` (`:70-80`) is identical to pre's local `STOP_WORDS`.
- `significantQueryWords(text)` (`:82-84`): `text.toLowerCase().split(/\s+/).filter(w => w.length > 2 && !stop.has(w))`.
- `scoreTextMatch(memory, text)` (`:87-107`) is the pre text formula verbatim:
  - 0.4 for a phrase in the summary;
  - 0.3 for a phrase in the content;
  - 0.2 × the content word share;
  - 0.1 × the summary word share;
  - 0.1 × matching keywords / max(len, 1);
  - `min(score, 1)`.
- **`gather(characterId, query, opts, trace=[], label?)` (`:118-253`):**
  1. No embedding, or `forceText`, goes to `gatherText(..., embedding ? 'forced' : 'no embedding')`.
  2. `memoryIndex.shape(characterId, dims)`. When `size > 0 && matching === 0`, it logs the WARN (once per key)
     `[Memory] Embedding dimension mismatch — recall is degraded to TEXT search (embeddings ignored). Reindex this
     character or restore the matching embedding profile.` `{characterId, query: text.substring(0,100),
     storedDimensions, queryDimensions, provider}` and goes to `gatherText(..., 'dimension mismatch')`.
     - **Delta vs pre:** pre tested `vectorStore.getDimensions() !== null && len !== stored`, and its WARN carried
       `userId` and `embeddingProfileId` as well. A mixed-dimension index with some matching rows now searches by
       vector.
  3. `memoryIndex.search(characterId, vec, { hotOnly: !includeCold, exclude, limit: limit*3 })` gives hits with
     `via: 'vector'`.
  4. Probes with an embedding of the same length union in. A memory keeps its best cosine, and its `via` stays
     whatever it entered with (`'probe'` for new ids).
  5. Literal and anchor hits:
     - `literalPhrase = opts.literalPhrase ? getLiteralPhrase(query.text) : null`.
     - The anchor phrases are the whole `query.text.trim()` when literal, then the chosen entity anchors.
     - Under `tuning.specificAnchors`, R4 `selectSpecificAnchors(counted, presentParticipantNames, 3, {minHits,
       order})` logs DEBUG `[Memory] Specific entity anchors chosen` `{characterId, counted, chosen}`. That line is
       unchanged.
     - Otherwise the first 3.
     - `searchByContent` hits are filtered by `exclude` and the cold tier, and cached per phrase.
     - Direct hits not already in the pool with a same-length embedding are pushed at
       `cosineSimilarity(queryVec, m.embedding)`, with `via: literal ? 'literal' : 'anchor'` and the memory attached.
  6. `minScore = opts.minScore ?? defaultMinCosineForProvider(provider)`.
  7. Trace `gather` `{strategy:'vector', vector, probes: usableProbes.length, probeAdded, anchors: chosenAnchors
     (array), anchorAdded, literal: bool, pool}`.
- **`gatherText` (`:260-303`):**
  - The probes are `[query.text, ...probe texts]`, non-empty. For each probe it takes the phrase hits, plus per-word
    hits when there are fewer than `limit` phrase hits.
  - It then adds hits for `cleanAnchors(anchors).slice(0,3)`.
  - Each hit's score is `max over probes of scoreTextMatch`, with `via: 'text'` and the memory attached.
  - Trace `gather` `{strategy:'text', reason, probes, anchors (array), pool}`.
  - It returns `minScore 0, provider null, literalPhrase null, literalHitIds ∅`.
  - **There are NO exclude or cold filters at gather on the text path.** Hydrate applies them.

### 1.4 `lib/memory/recall/hydrate.ts` (107 lines)

- `HydrateFilters { includeCold; excludeIds?; minImportance?; source?; aboutCharacterId? }` (`:22-28`).
- `passesFilters(memory, f)` (`:31-38`), in this order:
  1. excluded;
  2. cold when `!includeCold`;
  3. `filterImportance(memory) < minImportance`, i.e. `reinforcedImportance ?? importance`. This is the Phase A
     scoring change.
  4. source;
  5. `aboutCharacterId`.
- `hydrate(gathered, filters, trace, label)` (`:41-74`):
  - One `memories.findByIds(missing)` for the hits without a row.
  - Rows are kept in pool order.
  - The literal boost applies when `literalHitIds.has(id) || containsLiteralPhrase(content|summary, phrase)`:
    `score = applyLiteralBoost(raw)`.
  - `usedEmbedding = strategy === 'vector'`.
  - Trace `hydrate` `{read: rows.length, kept, dropped}`.
- `hydrateNeighbours(ranked, limit, characterId, filters)` (`:82-107`):
  - Seeds are the top `limit` ranked rows' `relatedMemoryIds`.
  - At most `RELATED_EXPANSION.maxPerHit` per seed and `maxTotal` overall, skipping ids in the pool, seen or
    excluded.
  - One `findByIds`, then keep `characterId === characterId && passesFilters(m, {...filters, aboutCharacterId:
    undefined})`.

### 1.5 `lib/memory/recall/score.ts` (113 lines; pure)

- `ScoreOptions { limit; recallContext?; window; nowMs }` (`:23-30`).
- `Scored { strategy; ranked; context; bestCosine; outOfWindow: RankInput[]; windowMode: 'none'|'filter'|'hard'|'soft' }`
  (`:32-42`).
- `score(hydrated, gathered, opts, trace, label)` (`:51-113`):
  1. The floor is `text ? score > 0 : score >= minScore`, applied to `h.score`, the BOOSTED value. That matches pre,
     which floored the literal-boosted `cosineScore`.
  2. `salience = salience(memory, {nowMs}).salience` and `bestCosine = max(rawScore)` over the floored pool.
  3. R1's gate is off (`tuning: {...recallTuningOf(ctx), boostGate: null}`) for a TEXT strategy, and for
     `!ctx.tuning && isLexicalEmbeddingProvider(provider)`. Pre did only the second, logging DEBUG `[Memory] TF-IDF
     embeddings: recall boost gate off`. **That line is GONE.**
  4. The window comes from `parseWindow` (both bounds finite and `from <= to`):
     - with no context it is a plain filter (`'filter'`);
     - with `hits >= limit` it is `'hard'`, where `outOfWindow` (vector only) is the rest;
     - otherwise it is `'soft'`, with `occurredWithin` folded into the rank context.
  5. Ranking is `rankForRecall(candidates, rankContext, bestCosine)` when there is a context, else
     `rankByBlend(candidates)`.
  6. Trace `score` `{floor: text ? '>0' : minScore, floored, bestCosine: Math.round(best*1000)/1000, window: mode,
     excluded: candidates − ranked, ranked}`.

  The **text strategy now ranks under the full recall context** (window, multipliers; bug 214 plus Phase E). Pre's
  text fallback did none of it, and also ignored `occurredWithin`, `excludeMemoryIds`, probes, anchors and the weight
  clock.

### 1.6 `lib/memory/scoring/rank.ts` (94 lines; NEW in `584d28164`, pure)

- `RankInput { memory; score; rawScore; salience; usedEmbedding }`.
- `byBlendedAfter(a, b)`: `(b.blendedAfter ?? 0) − (a.blendedAfter ?? 0)`.
- `rankForRecall(candidates, ctx, bestCosine, extraFired=[])`:
  - `blendedBefore = computeRankingBlend(score, salience)`;
  - `adj = combineRecallMultipliers(memory, ctx, {cosine: rawScore, bestCosine})`;
  - excluded rows are dropped;
  - `fired: [...adj.fired, ...extraFired]`, `blendedAfter = before × multiplier`;
  - `.sort(byBlendedAfter)`.
- `rankByBlend(candidates)` maps to `{memory, score, usedEmbedding, salience}` (NO `recallAdjustment`) and sorts by
  `computeRankingBlend(b) − computeRankingBlend(a)`.
- `reserveBackgroundSlots(ranked, background, headSize, fraction)` is pre's verbatim, retyped. The R6 DEBUG
  `[Memory] Background reservation (R6)` `{characterId, threshold, qualifying, reserve}` is **GONE**; the `select`
  trace replaces it.

### 1.7 `lib/memory/recall/select.ts` (102 lines; pure)

- `SelectOptions { limit; headSize?; queryEmbedding: Float32Array|null; nowMs }`.
- `wantsExpansion(scored)` is `strategy === 'vector' && context?.expandRelated === true`.
- `selectRanked(scored, neighbours, opts, trace, label)` returns `{candidates, ranked}`:
  1. **Expansion.** It runs when there is a context, expansion is wanted, there is a query vector, and there are
     neighbours. Neighbours with a same-length embedding are scored at the cosine and
     `salience(m, {nowMs})`, then go through `rankForRecall(..., ['related↗'])`. The floor is NOT re-applied. The
     union is then sorted by `byBlendedAfter`.
  2. **R6.** It runs when there is a context, a gate, `backgroundReserve > 0`, an out-of-window remainder and a
     `headSize`:
     - `threshold = boostGateThreshold(gate, bestCosine)`;
     - background is `rankForRecall(outOfWindow not present && rawScore >= threshold, …, ['bg↺'])`;
     - `reserved = min(floor(headSize × reserve), background.length)`.
  3. The cut is `.slice(0, limit)`.
  4. Trace `select` `{expanded, reserved, cut, returned}`.
- `select()` returns `.candidates`.

### 1.8 `lib/memory/recall/search.ts` (169 lines)

- `RecallSearchOptions` (`:24-47`): `{ limit?=20, minScore?, minImportance?, source?, aboutCharacterId?, includeCold?,
  excludeIds?, literalPhrase?, recallContext?, headSize?, nowMs?, forceText? }`.
- `RecallSearchResult { candidates; ranked; strategy; trace }`.
- **`searchRecall(characterId, query, options, trace=[], label?)` (`:61-126`):**
  - `run(forceText)` is gather → (an empty vector pool returns early with `empty: true`) → hydrate → score →
    neighbours (if wanted) → `selectRanked`.
  - With an embedding and no `forceText`, it tries the vector run first:
    - A non-empty result is returned as it is.
    - An empty one traces `gather` `{strategy:'vector', pool:0, fallback:'text'}`.
    - A throw logs WARN `[Memory] Semantic search failed, falling back to text search` `{characterId, query:
      query.text.substring(0,100), error: String(error)}`. Pre also carried `userId`. It then traces `gather`
      `{strategy:'vector', failed: String(error), fallback:'text'}`.
  - An empty `query.text` returns empty with `strategy: 'text'`. Otherwise `run(true)`.
  - **Nothing here stamps.**
- `SearchMemoriesOptions` adds `{ userId, embeddingProfileId?, window?, anchors?, probes?, embeddingMemo? }`.
- `searchMemories(characterId, text, options)` (`:146-169`) is the one-shot form for the tool, the API, Carina, the
  voice rehearsals and the greeting:
  - `buildRecallQuery({userId, embeddingProfileId, text, embeddingMemo, probes})`, so NO distil;
  - then overrides `window = options.window ?? null`, `anchors = options.anchors ?? []`;
  - then `searchRecall`, returning `.candidates`.

### 1.9 `lib/memory/recall/assemble.ts` (617 lines)

**Pool sizing.**
- The constants (`:72-87`) are identical to pre `memory-injector.ts:42-71`.
- `PROACTIVE_RECALL_POOL_SIZE = FROZEN_ARCHIVE_MAX_SIZE + DYNAMIC_HEAD_MAX_SIZE*2` (`:90`), which is 90. It moved here
  from pre-compute, which re-exports it.
- `INTER_CHAR_PER_CHARACTER_LIMIT=5`, `INTER_CHAR_DIGEST_LIMIT=2`, `INTER_CHAR_RELEVANCE_PER_CHARACTER_LIMIT=5`,
  `RETRO_MINI_RECAP_MAX_ENTRIES=5` and `MEMORY_RECAP_PHASE_TIMEOUT_MS=60_000` (`:93-105`) all moved from
  `context-manager.ts`, unchanged.
- `sizeMemoryPools` (`:125-140`) is pre's verbatim.

**`PreRecall { query; candidates; ranked?; trace? }` (`:147-153`).**

**`RecallPlan` (`:155-204`):**
`{ userId, embeddingProfileId?, provider?, character:{id,name}, chat:{id, projectId?, compactionGeneration?,
commonplaceRecallHistory?, timelineMode?}, memoryBudget, minImportance, skipMemories, present:[{id,name}],
interCharacterSubjects:[{id,name}], currentStateTokens, recallSettings:{scopePolicy, expandRelated,
perTurnConversationSummaries}, recentWindow, distill?:{messages, selection, clockIso}, preRecall?, recap?:{selection,
uncensoredFallback?, maxContext?, fallbackQuery} | null, conversations?:{maxContext?, standingIds: () =>
Promise<Set<string>>} | null, onStatusChange?, replay?:{excludeIds?, nowMs?, tuning?, ring?} }`.

**`RecallBundle` (`:206-237`):**
`{ query; path: 'skipped'|'pre-searched'|'two-pool'; pool; relevancePool; sections:{recap?, relevant?,
interCharacter?, relevantConversations?, retrospective?}; tokens:{recap, relevant, interCharacter};
lines: Partial<Record<RecallPool, RenderedLine[]>>; pools:{archive, head, interCharacter, recap}; retroSignature;
debugMemories; debugInterCharacterMemories; memoriesIncluded; interCharacterIncluded; trace; warnings }`.
`emptyRecallBundle()` (`:239-257`) builds it empty.

**`assembleRecall(plan)` (`:265-461`)**, ORDER MATTERS:

1. **The query (`:276-333`), when `!skipMemories`:**
   - The **`pre-searched`** path is taken only when `preRecall && preRecall.query.text && preRecall.candidates.length >
     0`. `pool = candidates`, `relevancePool = ranked ?? candidates`, and `trace.push(...preRecall.trace)`.
   - Otherwise, when `recentWindow` is non-empty, it takes the **`two-pool`** path:
     - `buildRecallQuery({userId, embeddingProfileId, text: recentWindow, distill: plan.distill ? {messages,
       characterName, selection, chatId, characterId, clock:{nowIso: clockIso, timelineMode: chat.timelineMode ??
       'realtime'}, latency:'interactive'} : undefined}, trace)`.
     - Then `buildTurnRecallContext({chat, recallSettings, turnContext, turnTemporal, turnRetrospective,
       presentAboutCharacterIds: presentIds, presentParticipantNames, nowMs, ...tuning})`, plus
       `recentlyWhisperedIds = replay.ring` when replaying.
     - Then `searchRecall(..., {limit: sizing.headEntries*3, minImportance, recallContext, excludeIds, nowMs (replay
       only)}, trace, 'head')`.
   - A throw pushes the warning `Failed to retrieve memories: ${message}`.
   - ⚠ **An empty or failed proactive pass discards its signals.** The two-pool branch distils again, interactively.
     Pre kept `options.recallSignals` as `turnRecallSignals` when the fallback distil failed.
2. **The archive (`:338-372`):**
   - `getOrComputeFrozenArchive(characterId, chat.id, compactionGeneration ?? 0, {size: ordinarySizing.archiveSize,
     presentCharacterIds: presentIds minus self, exclude: replay.excludeIds})`.
   - `subject = buildMemorySubjectContext(characterId, [...archive, ...pool.map(c=>c.memory)])`. This is the ONE subject
     lookup per turn, so the v5 call-site guard re-aims here.
   - `renderArchive(archive, ordinarySizing.archiveTokenBudget, provider, subject)`. It always uses the ORDINARY
     budget (correction 0.1.5).
   - `lines.archive`, `pools.archive` and `debugMemories += archive`.
   - Trace `assemble`(`archive`) `{size, held, rendered, cutByBudget, tokenBudget}`.
   - A throw pushes the same `Failed to retrieve memories:` warning.
3. **The recap (`:376-407`)**, when `plan.recap`:
   - `onStatusChange('generating_recap', \`Recalling ${name}'s memories...\`)`.
   - `useTurnQuery = !!query?.embedding && query.text.length > 0`.
   - `withTimeout(generateMemoryRecap({characterId, characterName, selection, userId, chatId, uncensoredFallback,
     maxContext, relevanceQuery: useTurnQuery ? query.text : fallbackQuery, relevanceEmbedding: useTurnQuery ?
     query.embedding.embedding : null, embeddingProfileId, excludeIds: new Set(pools.archive)}), 60_000, \`Memory recap
     exceeded its 60000ms phase budget\`)`.
   - Any content goes to `sections.recap`, `tokens.recap = estimateTokens(content, provider)` and
     `pools.recap = recap.memoryIds`.
   - Trace `assemble`(`recap`) `{rows, reusedQuery, excludedAsArchive}`.
   - A throw pushes the warning `Failed to generate memory recap: …` and logs ERROR `[Recall] Memory recap generation
     failed` `{characterId}` (pre: `[ContextManager] Memory recap generation failed`).
   - Then it **always** calls `onStatusChange('assembling_context', 'Assembling context...')`.
   - ⚠ **Order change vs pre:** pre ran the recap (status, embedding, cheap-LLM) BEFORE the head's distil and search.
     The pin runs it after the head query, the search and the archive. The SSE status frames keep their relative order
     because the distil emits none; only the timing and the call order move.
4. **The head (`:410-443`):**
   - The retro head budget is `min(retroHead, max(ordinaryHead, memoryBudget − archiveTokens))`; otherwise the
     ordinary head budget.
   - The candidates are the pool minus archive ids. **The recap's rows stay eligible.**
   - `renderHead(candidates, provider, subject, {maxTokens, nowMs, maxEntries: turnSizing.headEntries})`.
   - Trace `assemble`(`head`) `{pool, removedAsArchive, rendered, cutByBudget: min(candidates, entries) − rendered,
     entries, tokenBudget, retrospective}`.
   - `sections.relevant = [archive, head].filter(Boolean).join('\n\n') || undefined`;
     `tokens.relevant = archive + head`; `memoriesIncluded = |pools.archive| + |pools.head|`.
5. **Inter-character (`:447-453` → `assembleInterCharacter` `:463-524`)**, when `!skipMemories && subjects.length`:
   - `budget = floor((memoryBudget − tokens.relevant − currentStateTokens) / 2)`; it returns when that is ≤ 0.
   - `delivered = archive ∪ head`.
   - **Importance half:** per subject, `findHotDigests(self, otherId, 2)`, then
     `findByCharacterAboutCharacters(self, others, 5)`. Each row is dropped if it is in `replay.excludeIds` (silently)
     or already delivered (`skipped++`), and deduped by id.
   - **Relevance half:** `bundle.relevancePool` filtered to `aboutCharacterId ∈ others`. Delivered rows count as
     `skipped++`. At most 5 per subject, taken in ranked order.
     - Pre ran ONE `searchMemoriesSemantic` per other character over the RAW recent-window `memorySearchQuery`, with
       NO recall context, `limit 5`, `minImportance` and `aboutCharacterId`.
     - The pin is a FILTER of the turn's one search, read past the head's cut. Bug 224 is in here too.
   - `renderInterCharacter(importance, names, budget, provider, relevance, nowMs)`.
   - Trace `assemble`(`interCharacter`) `{subjects, importance, relevance, skippedAsDelivered, rendered, tokenBudget}`.
   - A throw pushes the warning `Failed to retrieve inter-character memories: …`.
   - **The pre WARN `[ContextManager] Inter-character relevance search failed` is gone.**
6. **The conversation lists (`:456-458` → `assembleConversations` `:526-617`)**, when `plan.conversations && query`:
   - **The per-turn list.** It runs when there is an embedding, `perTurnConversationSummaries` is on and there is no
     recap section:
     - `searchVaultConversationSummaries({characterId, query: query.text, userId, embeddingProfileId,
       precomputedEmbedding: embedding, limit: rampLimit(maxContext, 3, 10, 4000, 32000), excludeConversationId: chat.id,
       timeRange: query.window})`, minus the standing fold ids.
     - It renders `${renderRelevantConversationsBlock(filtered)}\n\n${READ_CONVERSATION_CALL_NOTE}`.
     - Trace `assemble`(`conversations`) `{matched, listed}`.
     - A failure logs WARN `[Recall] Per-turn conversation-summary search failed (non-fatal)` `{chatId, characterId,
       error}`. Pre: `[CommonplaceWhisper] …`, plus DEBUG `[CommonplaceWhisper] Per-turn conversation summaries listed`,
       which is now GONE.
   - **The retrospective mini-recap**, when `query.retrospective`:
     - `signature = [window ? \`${from}→${to}\` : '∅', ...entities.map(lower).sort()].join('|')`.
     - If the signature is among the recent ones, it traces `{suppressed: true, signature}` and returns. Pre's DEBUG
       `[CommonplaceWhisper] Retrospective mini-recap suppressed (recent same-signature block)` is GONE.
     - `miniQuery = [query.text, entities.join(', ')].filter(trim).join(' — ')`.
     - `reuse = miniQuery === query.text && embedding !== null`. That is the ONE extra embedding on an entity-bearing
       retro turn.
     - Pre built the query from `signals.paraphrase ?? memorySearchQuery`. At the pin it is
       `query.text = paraphrase ?? keywords.join ?? recentWindow`, so it differs when there are keywords but no
       paraphrase.
     - Limit 5, minus standing ids, minus ids already listed; sets `retroSignature`.
     - Trace `{matched, listed, signature, reusedQuery}`.
     - A failure logs WARN `[Recall] Retrospective mini-recap failed (non-fatal)` `{chatId, characterId, error}`.

**`assembleRecall` READS ONLY.** It never stamps and never writes the ring (pinned by
`assemble-deliver.test.ts` "writes nothing").

### 1.10 `lib/memory/recall/render.ts` (322 lines; THE MODEL-VISIBLE TEXT, VERBATIM)

**`DebugMemoryInfo { memoryId?, summary, importance, score, salience, recallMultiplier?, recallFired?, blendedBefore?,
blendedAfter? }`** (`:25-41`). Pre had `effectiveWeight` and `rawWeight?` in place of `salience`.
`DebugInterCharacterMemoryInfo { memoryId?, aboutCharacterName, summary, importance }` is unchanged.

`RenderedSection<D> { content; tokenCount; memoriesUsed; lines: RenderedLine[]; debugMemories: D[] }`. An empty
section is all-empty.

**The helpers:**
- `formatMemorySubjectPrefix` (`:85-93`) is unchanged: `''`, `About <Name>: ` or `About another character: `.
- `formatDigestProvenance` (`:96-101`) is unchanged: ` (from N note|notes)` for a CONSOLIDATED row with N > 0.
- `isTargetingKeyword(kw)` (`:104-108`): `k = kw.trim().toLowerCase()`. A `scope:` prefix is checked as
  `SCOPE_VALUES.has(rest.trim())`; anything else as `TEMPORAL_VALUES.has(k) || CONTEXT_VALUES.has(k)`.
- `humanKeywords(kws)` (`:111-115`): trimmed strings, non-empty, not targeting.
- `formatKeywordsTag(kws)` (`:118-121`): ` _(keywords: a, b)_`, or `''`.
- `ageTag(memory, now)` (`:123-127`): `[${formatMemoryAge(memory, now)} · ${narrativeTime}]` or `[${age}]`.
  `formatMemoryAge` is pre's `formatRelativeAge` body verbatim, now in `lib/memory/time/age-label.ts`.

**The four sections.** Every line's token cost is `estimateTokens(line + '\n', provider)`. The header's tokens are
counted first, and a line that would exceed the budget BREAKS the loop. Each rendered line pushes
`{memoryId, line, body}` and one debug entry.

| Section | Header | `(pre)` line | **Pin line** |
|---|---|---|---|
| `renderArchive(memories, maxTokens, provider, subject)` `:134-159` (input order; `body = summary.trim() \|\| content.trim()`, skip empty; debug `{memoryId, summary, importance, score:0, salience:0}`) | `## Memory Anchors` | `- ${prefix}${summary}${prov} _(importance 0.80 · keywords: a, b)_` | **`- ${prefix}${body}${prov}${formatKeywordsTag}`** |
| `renderHead(candidates, provider, subject, {maxTokens, maxEntries, nowMs?})` `:167-219` (sort below; skip empty body) | `Most relevant memories for this turn:` (header + `\n` + lines; no bullet) | `[m_abcd] [age · narr] ${prefix}${summary}${prov} _(importance … · relevance … · weight … · recall: narrow✓ past↓ · keywords: …)_` | **`${ageTag} ${prefix}${body}${prov}${formatKeywordsTag}`** |
| `renderRelevant(candidates, maxTokens, provider, subject)` `:225-255` (Carina; `body = content.trim() \|\| summary`, no skip; `now = new Date()`) | `## Relevant Memories` | `- [age · narr] ${prefix}${content} _(importance … · relevance … · weight … · keywords: …)_` | **`- ${ageTag} ${prefix}${body}${formatKeywordsTag}`** (still NO provenance) |
| `renderInterCharacter(importanceMemories, characterNames, maxTokens, provider, relevance=[], nowMs=Date.now())` `:263-322` | `## Memories About Other Characters` | `- About ${name}: [age] ${body}${prov} _(importance … · relevance …? · weight … · keywords: …)_` | **`- About ${name}: [${formatMemoryAge}] ${body}${prov}${formatKeywordsTag}`** |

**The orderings:**

- **`renderHead`:** when both rows have `recallAdjustment.blendedAfter` it compares `b − a`. Otherwise
  `salienceDiff = b.salience − a.salience`; if `|diff| > 0.05` it uses that, else `b.score − a.score`. Then
  `.slice(0, maxEntries)`. Pre used the floored `effectiveWeight` in place of `salience`, so the order MOVES as well as
  the text. The debug entry is `{memoryId, summary, importance, score, salience, recallMultiplier, recallFired,
  blendedBefore, blendedAfter}`; undefined keys drop out in JSON.
- **`renderRelevant`:** the same 0.05-epsilon salience-then-score comparator, without the `blendedAfter` arm.
- **`renderInterCharacter`:** characters are keyed with the relevance characters first, then importance-only ones,
  each in first-seen order.
  - Within a character: the relevance rows are sorted by `score` descending. The importance rows (minus relevance ids)
    are sorted digests-first, then `salience(m, {nowMs})` descending. Pre used `calculateEffectiveWeight(m)` at
    wall-clock.
  - The two are interleaved relevance-first. The token break is per line, so a later character can still fit after an
    earlier one breaks: the `break` leaves only the inner loop, as in pre.
  - `name = characterNames.get(id) || 'Unknown'`.

### 1.11 `lib/memory/recall/deliver.ts` (198 lines; the ONLY writer of "what was shown")

- `RECALL_DEBUG_PREFIX = '[Recall]'` (`:31`) and `RECALL_RING_PREFIX = '[Recall:ring]'` (`:33`).
- **`stampAccessed(characterId, ids)` (`:41-57`)** is the ONE `lastAccessedAt` writer at the pin; `updateAccessTime`
  has no caller left.
  - Ids are deduped, non-empty strings. None returns.
  - DEBUG `[Recall] Stamping delivered memories accessed` `{characterId, count}`. Pre: `[Memory] Marking memories
    accessed`.
  - `updateAccessTimeBulk(...).catch(warn)` inside try/catch. The WARN is `[Recall] Failed to stamp lastAccessedAt on
    delivered memories` `{characterId, count, error}`.
  - The WARN stays **unreachable through v4's real code**: `updateAccessTimeBulk` is still a fallback `safeQuery`
    returning 0 (`memories.repository.ts:940-956`), the same as the P4.D268 negative pin.
- `SURVIVAL_WORD_SHARE = 0.6` (`:64`).
- **`survivingLines(lines, deliveredText, {compressed?})` (`:77-93`):**
  - Empty text gives `[]`.
  - `normalise(s) = s.toLowerCase().replace(/\s+/g,' ').trim()`.
  - A line survives when `normalise(deliveredText).includes(normalise(line.line))` and the rendered line is non-empty.
    The WHOLE line, not the body.
  - Only when `compressed`: the body's distinct significant words (after
    `body.replace(/[^\p{L}\p{N}\s'-]/gu, ' ')`, at least 3 words) must reach ≥ 0.6 overlap with the significant words
    of ONE delivered line (the same Unicode strip).
- `RecallDelivery { delivered: Partial<Record<RecallPool, string[]>>; lost: string[]; debugLog: string[] }`.
- `DeliverOptions { characterId; chat:{id, commonplaceRecallHistory?}; deliveredText; persistRing? }`.
- `traceLines(trace)` (`:115-119`): `` `[Recall] ${stage}${label ? ` (${label})` : ''}: ${JSON.stringify(detail)}` ``.
- **`deliverRecall(bundle, opts)` (`:125-182`):**
  1. `untouched(section) = !!section && deliveredText.includes(section.trim())`. The archive and the head are both
     judged by `sections.relevant`; inter-character by `sections.interCharacter`.
  2. For each `[pool, lines]` of `Object.entries(bundle.lines)`, in **insertion order**: archive (step 2), head (4),
     interCharacter (5). A pool with no lines is skipped. `delivered[pool] = unique kept ids`, and the rest go to
     `lost`.
  3. `stampAccessed(characterId, all delivered)`. **The recap's rows are never stamped.**
  4. `head = delivered.head ?? []`. `retroDelivered = !!retro.trim() && deliveredText.includes(retro.trim())`, and
     `signature = retroDelivered ? bundle.retroSignature : null`.
  5. If `persistRing !== false && (head.length || signature)`:
     `next = appendRecallTurn(chat.commonplaceRecallHistory, head)`, then `appendRetroSignature`, then
     `chats.update(chat.id, {commonplaceRecallHistory: next})`. A failure logs WARN `[Recall] Failed to persist the
     anti-repetition ring` `{chatId, error}` (pre: `[CommonplaceWhisper] Failed to persist recall history`).
  6. **The record:**
     ```
     debugLog = [ ...traceLines(bundle.trace),
                  `[Recall] deliver: ${counts || 'nothing'}${lost.length ? `; ${lost.length} cut by compression` : ''}`,
                  `[Recall:ring] ${head.join(' ')}`.trimEnd() ]
     ```
     `counts` is `` `${pool} ${n}` `` joined with `', '` over the delivered entries.
  7. DEBUG `[Recall] Delivered` `{chatId, characterId, delivered: {pool: count…}, lost: lost.length}`.
- `ringFromDebugLog(lines)` (`:185-189`): the first line that `startsWith('[Recall:ring]')`, sliced, trimmed and split
  on `\s+`, non-empty. No such line returns `null`.
- `withRecallLines(existing, lines)` (`:195-198`): `[...existing lines starting with '[Recall]' or '[Recall:ring]',
  ...lines]`. This is the memory-extraction job's way of appending AFTER the record instead of overwriting it.

### 1.12 `lib/memory/recall/replay.ts` (405 lines; was `lib/memory/recall-replay.ts`, 349 lines)

**The wire shape, in exact key order:**

- **`RecallReplayResult`** (`:87-125`, built `:382-404`):
  `chatId, characterId, characterName, turnIndex, totalTurns, signals, query, clockIso, oldPath, newPath,
  fallbackPath, oldHeadSize, newHeadSize, tuning, signalsPinned, asOf, excludedAfterAsOf, ring, ringSource, pools,
  trace`.
- **`RecallReplayRow`** (`:69-85`, `toRows` `:152-169`):
  `memoryId, summary, kind ('semantic' default), occurredAt|null, narrativeTime|null, createdAt, keywords|[], cosine,
  salience|null, blendedBefore|null, multiplier|null, fired|[], blendedAfter|null, selected`.
  `rawWeight` became `salience`, in the same position.

**The behaviour deltas:**
1. The query is `buildRecallQuery({userId, text: fallbackText (the last window message's content), signals, embeddingMemo})`.
   `query.text.trim()` empty throws `Could not derive a recall query for this turn`. The text choice is the same as
   pre, and so is the error.
2. NEW `ringAsOf(messages, beforeMessageId)` (`:179-191`): the ring rebuilt from `ringFromDebugLog` on every message
   BEFORE the turn's opening message.
   - Empty rings are skipped but count as `sawRecord`. The last `RECALL_HISTORY_TURNS = 3` are kept. With no record at
     all it returns `null`.
   - `ringSource = reconstructed ? 'debug-log' : 'chat'`.
   - When reconstructed, the chat used for the context is `{...chat, commonplaceRecallHistory: {turns: reconstructed}}`.
3. The OLD path is `{...query, probes: [], anchors: [], window: null}`, the same embedding, with
   `searchRecall(..., {limit, minImportance: RECALL_MIN_IMPORTANCE, recallContext: baseCtx, excludeIds, nowMs}, [],
   'old')`. Pre used 0.3, the same value.
4. The NEW path uses `newCtx` (`turnRetrospective`, the tuning when given, and the present names when
   `specificAnchors`), with `{limit, minImportance, recallContext, excludeIds, headSize: newHeadSize, nowMs}` into
   `trace` labelled `'head'`.
5. **NEW `fallbackPath`** is `searchRecall(..., {...newSearch, forceText: true}, [], 'fallback')`, and its rows are cut
   at `newHeadSize`.
6. **NEW**: one `assembleRecall` over the live pool:
   - `memoryBudget: input.memoryBudget ?? 2000` (`REPLAY_DEFAULT_MEMORY_BUDGET`);
   - `present: [self, ...others]`, `interCharacterSubjects: others`, `currentStateTokens: 0`;
   - `recentWindow: fallbackText`;
   - `preRecall: {query, candidates: new.candidates, ranked: new.ranked}`;
   - `replay: {excludeIds: excludeMemoryIds ?? new Set(), nowMs, ring: newCtx.recentlyWhisperedIds}`;
   - NO recap and NO conversations. The archive's `exclude` is always passed, so the replay archive bypasses the cache.
   - Then `trace.push(...bundle.trace)` and `pools = bundle.pools`.
   - **`assembleRecall` reads `findHotDigests` and `findByCharacterAboutCharacters` for the others and computes the
     archive.** The replay is read-only and never delivers.
7. `presentCharacters` are now ALWAYS read with `findByIdRaw` for every present participant. Pre read them only under
   `specificAnchors`.
8. `ring: [...(newCtx.recentlyWhisperedIds ?? [])]`. The INFO `Recall replay complete` gains `ringSource`, and the
   `oldCandidates`/`newCandidates` counts now come from `.candidates`.

**The route** `app/api/v1/chats/[id]/actions/recall-replay.ts` changed only its imports (`recall/replay`,
`recall/tuning`), so the request body and validation are unchanged.

**The CLI** `packages/quilltap/lib/recall-replay-command.js`:
- prints a `Ring` line (`… recently whispered · as of the turn, from its recall records` or the yellow `the chat's
  current ring (no recall records that early)`);
- adds a `TEXT FALLBACK (what the new path returns with no usable vector)` table and a `STAGE TRACE` block
  (`printTrace`: `stage.padEnd(9)` + `(label)` + `k=v` with JSON for objects);
- adds `-d/--data-dir`, `-i/--instance` and `--passphrase` (accepted and ignored) through
  `lock-helpers.resolveServerPort` (bug 239, `93c5af6ad`);
- now has `--port` default `undefined`, reading the lock file's port, else 3000;
- exports `parseFlags`.

`printPath`'s columns are unchanged and never printed the weight.

### 1.13 Moves with no behaviour delta (diffed)

- **`recall/tags.ts`** (was `recall-tags.ts`, R095). Comments only, plus `DEFAULT_RECALL_TUNING` now defined HERE
  (`Object.freeze({multipliers: {...RECALL_MULTIPLIERS}, ...RECALL_TUNING_DEFAULTS})`), with
  `recallTuningOf(ctx) = ctx.tuning ?? DEFAULT_RECALL_TUNING`.
  - `RECALL_MULTIPLIERS.occurredWithinWindow` is still 1.3.
  - `buildRetrospectiveProbes` is unchanged; it now feeds `buildRecallQuery`.
- **`recall/tuning.ts`** (R095). It re-exports `DEFAULT_RECALL_TUNING`. The schemas (`RecallTuningInputSchema`,
  `RecallReplaySignalsSchema`), `resolveRecallTuning`, `describeRecallTuning` and `selectSpecificAnchors` are
  unchanged.
- **`recall/history.ts`** (R098): a comment only.
- **`recall/subject.ts`** (R093): the type import only.
- **`recall/conversations.ts`** (R096): the window soft boost now reads `RECALL_MULTIPLIERS.occurredWithinWindow`
  (1.3) instead of a local `1.3`, the same value.
- **`recall/archive.ts`** (R090, was `frozen-archive-cache.ts`). Three changes:
  - a new `exclude` option computes WITHOUT touching the cache (replay `asOf`);
  - `processSingleton('memory.frozenArchiveCache', …)` (bug 235; a no-op for v5's process-wide cache);
  - the fill re-rank is `sortBySalience(...)` (Phase A) instead of `calculateEffectiveWeight` — **the archive's
    membership and order move.**
- **`recall/recap.ts`** (R074, was `memory-recap.ts`):
  - `generateMemoryRecap(req: MemoryRecapRequest)` is now ONE object. The fields are `characterId, characterName,
    selection, userId, chatId?, uncensoredFallback?, maxContext?, relevanceQuery?, relevanceEmbedding?,
    embeddingProfileId?, excludeIds?`.
  - `findRecentByImportanceTier(characterId, {high: 20+|exclude|, medium: 10+|exclude|, low: 5+|exclude|})`, then
    keep the rows not excluded and `.slice(0, 20/10/5)`.
  - `buildConversationRecallLists` passes `precomputedEmbedding: relevanceEmbedding ?? null`.
  - The recent and relevant ramps use the `RELEVANT_CONVERSATIONS_*` constants, which have the same values
    (3/10/4000/32000).
  - `MemoryRecapResult` gains `memoryIds`. These are the narrative's rows when a narrative was produced, else `[]`.
  - `formatMemoryAge` replaces `formatRelativeAge`.
- **`recall/recap-summary.ts`** (`summarizeMemoryRecap`): moved verbatim (Phase D).
- **`lib/chat/context/scene-state.ts`** (167 lines): `formatCurrentSceneState`, `formatSummaryForContext` and
  `SceneStateEmissionEntry` are **byte-identical** to pre `memory-injector.ts` (diffed function by function).
  `context/index.ts:8` does `export * from './scene-state'`.

### 1.14 Consumers re-pointed by `584d28164`

- **`lib/chat/first-message-context.ts`.** `searchMemories(speaking, "name - description", {userId,
  embeddingProfileId, limit: 8, minScore: 0.4})`. Pre was the same, through `searchMemoriesSemantic`.
  - **The final sort is now salience** (`salienceById`, computed at `nowMs = Date.now()` once per call). Pre sorted by
    `importance`.
  - `stampAccessed(selected ids)`.
- **`lib/services/announcer/voice-rewrite-core.ts` `recallForSeed`.** `searchMemories(id, seed, {userId, limit 20,
  minImportance: RECALL_MIN_IMPORTANCE (0.3)})`, then `renderHead(results, provider, subject, {maxTokens: 200
  (RECALL_HEAD_TOKEN_BUDGET), maxEntries: 12})`, then `stampAccessed(formatted.lines ids)`, then
  `buildCommonplaceLLMContext({relevant})`.
  - **The rehearsal's request bytes move**: no `[m_]` tag and no meta, and the order follows salience.
  - The WARN `${logContext} Memory recall failed; proceeding without` is unchanged.
- **`lib/services/carina/carina.service.ts` `loadCarinaMemoryRecall` (`:213-245`).** `searchMemories(id, question,
  {userId, embeddingProfileId: undefined, limit: 12, minImportance: 0.3})`, then `renderRelevant(results, 1200,
  provider, subject)`, then `stampAccessed(lines)`.
  - **The answerer's request bytes move.**
- **`lib/tools/handlers/search-scriptorium-handler.ts`.** `searchCharacterMemories(characterId, query, {userId,
  embeddingProfileId, limit, minImportance (default TOOL_MIN_IMPORTANCE 0), literalPhrase: true, aboutCharacterId,
  window: timeWindow, includeCold: true})`.
  - The result metadata is `importance: filterImportance(m)` and `salience: mr.salience`, replacing
    `effectiveWeight`. `stampAccessed(returned ids)`.
  - The importance BAND in `formatSearchScriptoriumResults` is `importanceBand()`: high ≥ 0.7, **low < 0.3** (pre
    `< 0.4`). That and the calendar-zoned `since`/`until` (bug 212) are Phase A; the scoring and time lanes own them,
    and the tool's text moves with them.
  - **The tool's text fallback now applies the `window` filter.** Pre's text path dropped it (bug 214).
- **`lib/tools/handlers/self-inventory/{builders,formatters}.ts` (Phase A).** `loadedMemories.semanticMemories[*]` is
  `{summary, importance, score, salience}`, read from `builtContext.debugMemories`. The text is
  `- [importance X, score Y, salience Z] summary` (was `weight Z`). Archive rows carry `salience 0.00`, as they carried
  `weight 0.00`.
- **`app/api/v1/memories/route.ts` `handleSearch` (`:640-685`).** `searchMemories(id, query, {userId, limit,
  minScore, minImportance ?? 0, source, includeCold: true})`. The bytes move only by ranking (§0.1.1).
- **`lib/services/commonplace-notifications/relevant-conversations-refresh.ts`:** imports only.
- **`lib/memory/anchor-gate-probe.ts`:** NO Phase E hunk. It moves with Phases A–C and `4ae0c4ca6` (the index store,
  admission's `decideAdmission`, `lexicalProfile`/`note`). **It has no recall dependence.** v5's
  `services/anchor_gate_probe.rs` belongs to the admission and index lanes.

### 1.15 `lib/chat/context-manager.ts` AT THE PIN (2,372 lines; pre 2,891)

**What moved where:**
- the two-pool head, the inter-character block, the recap call, the per-turn conversation list, the retro
  mini-recap, and `markMemoriesAccessed` → `recall/assemble.ts` and `deliver.ts`;
- the ring write (pre `:2578-2598`, inside `if (posted)`) → `deliver.ts`;
- the formatters → `recall/render.ts` and `context/scene-state.ts`;
- `MEMORY_RECAP_PHASE_TIMEOUT_MS` and the `INTER_CHAR_*` constants → `assemble.ts`.

**Order of the buildContext recall region:**

1. `memorySearchQuery = buildRecentWindowQuery(existingMessages, newUserMessage)` (`:1233`; the function at
   `:630-655` is unchanged). Then `recallSettings = await getMemoryRecallSettings()` (`:1234`).
2. Scene state (`## Current State`, `:1240-1355`). It now runs BEFORE recall; the inter-character budget needs
   `currentStateTokens`.
3. `commonplaceTargetParticipantId` and `foldWhisperConversationIds()` (hoisted here from pre `:2365`).
4. **Recall (`:1378-1475`).**
   - `presentCharacters` is a Map, responder first, then `participantCharacters` not already present.
   - `interCharacterSubjects` are the CHARACTER participants ≠ the responder, only when `isMultiCharacter && character.id
     && participantCharacters && allParticipants`.
   - `recapRelevanceQuery = newUserMessage || last existing content || chat.scenarioText || chat.contextSummary || ''`
     (`:1396`).
   - `recall = character.id ? await assembleRecall({...}) : emptyRecallBundle()`, with `distill.messages` = the last 12
     existing messages (assistant ones go through `stripToolArtifacts`; empty ones are dropped) plus the new user
     message, `selection: cheapLLMSelection`, `clockIso: new Date().toISOString()`, `recap` only when
     `generateMemoryRecap && cheapLLMSelection`, and `conversations` always.
   - `if (!character.id) onStatusChange('assembling_context')`.
   - `warnings.push(...recall.warnings)`.
   - DEBUG `[ContextManager] Recall assembled` `{chatId, characterId, path, durationMs, archive, head, interCharacter,
     recap}`. `durationMs` is nondeterministic.
   - Pre's DEBUG lines `[ContextManager] Memory pools sized` and `[ContextManager] Inter-character memory retrieval
     complete` are **GONE**, as is `[ContextManager] Dynamic-head keyword distillation failed; using raw query`
     (replaced by `[Recall] Query distillation failed…`).
5. Knowledge (2c, unchanged).
6. Phase-2 memory compression (`:1527-1630`) is **unchanged**. It may rewrite `memoryContent` and
   `interCharacterMemoryContent`, which is what makes deliver's `compressed` flags true.
7. `cmpbParts` (`:1974-1983`): `currentState, recap, relevant (possibly compressed), interChar, knowledge,
   relevantConversations`.
   - `personaWhisper` posts to the transcript, then the scene cache, then the stale-whisper sweep. **The ring write is
     removed from this block.**
   - `llmRecallText = buildCommonplaceLLMContext({...cmpbParts, retrospectiveRecall})` (`:1985`).
   - `buildCommonplaceLLMContext` (`commonplace-notifications/writer.ts:146-178`, UNCHANGED) trims each section and
     prefixes the fixed lead-ins:
     - `Here is the present state of the scene:`;
     - `You remember the gist of what has happened so far:`;
     - `You remember the following entries that bear on this moment:`;
     - `You also recall about the others present:`;
     - `You also recall the following from your knowledge base:`;
     - `You also recall these past conversations that bear on the present:`;
     - `The past is being referenced — these past conversations cover the period in question:`.

     It joins them with `\n\n` and trims.
8. The retro whisper (`retrospective-recall`, `:2079-2094`) is unchanged.
9. **Mail (`:2095-2141`, bug 227):** see §1.16.
10. Progressions (`:2161`), the turn note (`:2180`), the scene anchor (`:2189`), and the inform DEBUG `[Inform]
    Delivering inform block as a trailing context section` `{chatId, participantId, onNewUserMessage, rowIds}`
    (`:2212-2219`, P4.D254, unchanged).
11. **ONE list (`:2228-2235`):**
    ```ts
    const trailingContextSections = [coreWhisperLLMContext, llmRecallText, suparnaMailLLMContext,
      progressionsLLMContext, informBlock, turnSkipInstruction].filter(Boolean)
    ```
12. **New user message (`:2237-2254`):** `content = sections.length ? \`${newUserMessage}\n\n---\n\n${sections.join('\n\n---\n\n')}\` : newUserMessage`,
    with `metadata: {isUserTurn: true}`. This is unchanged in order.
13. **`else` (`:2255-2283`)**, i.e. continue, chained and autonomous turns, and an empty-string `newUserMessage`:
    - if `llmRecallText`, DEBUG `[CommonplaceWhisper] Delivering recall in the trailing-only message (no new user
      message)` `{chatId, participantId: respondingParticipant?.id, length: llmRecallText.length}`;
    - `trailingOnly = [userNarrationAnchor, ...trailingContextSections].filter(Boolean)`, pushed as ONE `role: 'user'`
      message joined by `\n\n---\n\n` **only when non-empty**.
    - Pre (`(pre) :2802-2815`) was `[anchor, progressions, inform, turnSkip]`, pushed whenever any of the four existed.
14. **Delivery (`:2285-2295`):** `recallDelivery = character.id && llmRecallText ? await deliverRecall(recall, {characterId,
    chat, deliveredText: llmRecallText}) : null`.
    - The delivered text is the WHOLE Commonplace LLM text, current state included.
    - A turn with a current state but no memories still gets a record, `[Recall] deliver: nothing` and a bare
      `[Recall:ring]`, and so does a swipe.
15. **The returned `BuiltContext` (`:2302-2337`)**, in key order:
    `messages, tokenUsage, budget, includedSummary, memoriesIncluded, messagesIncluded, messagesTruncated,
    informRowIds, **mailAlertsOwed** (always present; `null` when none), warnings, debugMemories (`salience` shape),
    debugInterCharacterMemories?, debugKnowledge?, debugMemoryRecap?, debugRelevantConversations?,
    **recallDebugLog?** (`recallDelivery?.debugLog`), debugSummary?, debugSystemPrompt, originalSystemPrompt,
    compressionApplied, compressionDetails?`.

**`BuildContextOptions`:** `preSearchedMemories`, `preSearchedQueryEmbedding` and `recallSignals` are REPLACED by
`preRecall?: PreRecall | null` (`:455-463`). `minMemoryImportance` defaults to `RECALL_MIN_IMPORTANCE` (0.3; pre 0.5)
(`:674`). `context-builder.service.ts:1084` passes `RECALL_MIN_IMPORTANCE` (pre 0.5).

### 1.16 The delivery writers outside `context-manager.ts`

- **`lib/post-office/mailbox.ts:288-331` (NEW):**
  - `interface MailAlertsOwed { vaultId: string; paths: string[] }`.
  - `markMailAlertsDelivered(owed, {chatId, via})`: a null or empty `owed` returns. For each path it calls
    `markAlerted(vaultId, path)`.
    - A failure logs WARN `markMailAlertsDelivered: could not mark letter alerted` `{chatId, vaultId, path, error}`.
    - It then logs DEBUG `markMailAlertsDelivered` `{chatId, vaultId, via, requested, marked}`.
  - The logger is `createServiceLogger('PostOffice:Mailbox')`.
  - `markAlerted` (`:274-286`) is unchanged. A NOT_FOUND logs WARN `markAlerted: letter no longer present` and returns;
    anything else rethrows.
- **The buildContext mail block (`:2110-2141`):**
  - `isRegeneration = (regenerationOfMessageIds?.length ?? 0) > 0`; the block runs only when `mailVaultId &&
    !isRegeneration`.
  - It calls `collectUnalertedMail`, then `postSuparnaMailWhisper` (still at build time), then
    `suparnaMailLLMContext = buildSuparnaMailLLMContext(unalerted)` and `mailAlertsOwed = {vaultId, paths}`.
  - DEBUG `[Suparna] Mail announced; alerted flip deferred to delivery` `{chatId, characterId, vaultId, letters}`.
  - A failure logs WARN `[Suparna] Mail check failed; turn continues` `{chatId, characterId, vaultId, error}`.
  - `surfaceOperatorMailForChat` (operator mail, `:2142-2151`) still marks at build time, unchanged.
- **The finalizer (`message-finalizer.service.ts:290-323`)**, after the message and route-trail save:
  - if `builtContext.recallDebugLog?.length`, `chats.updateMessage(chatId, assistantMessageId, {debugMemoryLogs:
    recallDebugLog})`. A failure logs WARN (`MessageFinalizer` service logger) `Failed to store the recall record on
    the assistant message` `{chatId, messageId, error}`.
  - Then the inform consumption (unchanged).
  - Then `markMailAlertsDelivered(builtContext.mailAlertsOwed, {chatId, via: 'finalizer'})`.
- **`primary-stream.service.ts:74-78, :139`:** `MakePreservePartialOnErrorOptions.mailAlertsOwed?` is marked `via:
  'preserved-partial'` after the inform consume. **No recall record on this path.**
- **`orchestrator.service.ts`:**
  - `:1490` (the preserve-partial options);
  - `:1798` `via: 'tool-only'` after a tool-only save. **No recall record.**
  - `handleTurnSkip` (`:2081`, `:2104`) `via: 'turn-skip'` before the Host announcement;
  - `:1169`/`:1245` thread `preRecall` from `runPreContextPreCompute` into `buildMessageContext`.
- **The swipe path** (`regenerate-swipe.service.ts`) never reaches the finalizer:
  - recall IS delivered (stamps, ring), but **no recall record is stored for a swipe**;
  - the mail block is skipped.
- **`lib/background-jobs/handlers/memory-extraction.ts:62-74`** (extraction lane): `debugMemoryLogs:
  withRecallLines(message?.debugMemoryLogs, result.debugLogs)`. Pre overwrote the column. The Carina extraction
  handler no longer writes `debugMemoryLogs` at all (Phase D).
- **`collapse-stale-chat-caches.ts`** still NULLs `debugMemoryLogs` on stale chats. That makes `ringAsOf` fall back to
  `ringSource: 'chat'` for collapsed chats.

### 1.17 `pre-compute.service.ts` `proactiveRecallTask` (the PreRecall producer)

- The window and status frames are unchanged: `recalling_keywords` / `Analyzing recent conversation...`, then
  `recalling_memories` / `Searching ${name}'s memories...`.
- The distil is unchanged.
- No keywords returns `undefined`.
- Then `buildRecallQuery({userId, text: keywords.join(' '), signals}, trace)`. The text equals pre's `searchQuery`, and
  the probes are pre's `extraProbes`.
- `buildTurnRecallContext({..., turnRetrospective: query.retrospective, presentAboutCharacterIds,
  presentParticipantNames, nowMs: Date.now()})`.
- `searchRecall(character.id, query, {limit: 90, minImportance: RECALL_MIN_IMPORTANCE, recallContext}, trace, 'head')`
  returns `{query, candidates, ranked, trace}`.
- A throw logs WARN (`PreContextPreCompute`) `Proactive memory recall: memory search failed; the turn recalls with the
  query alone` `{chatId, characterId, error}` and returns `{query, candidates: [], trace}`. Pre's text was `…, falling
  back to default`.
- `PreContextPreComputeResult` is now `{cachedCompressionResponse, preRecall, stopKeepAlive}`.

---

## 2. What v4 DELETED in the span that v5 still carries (recall half)

| v4 deleted / renamed (pre) | v5 twin on `a7c6f0d03` |
|---|---|
| `memory-service.ts` `searchMemoriesSemantic` (pre `:771-1288`), `expandRelatedMemories` (`:1290-1357`), `searchMemoriesText` (`:1395-1514`), `reserveBackgroundSlots` (`:752-764`), `SemanticSearchResult`/`SearchQueryEmbedding`/`captureQueryEmbedding` | `services/memory_service.rs:288-306` (`SemanticSearchResult` with `effective_weight`/`raw_weight`), `:310-319` `RecallAdjustment`, `:327-357` `RecallContextInput`, `:361-366` `SearchQueryEmbedding`, `:370-437` `SemanticSearchOptions`, `:443-501` `EmbeddingMemoMap`, `:513-567` `search_memories_semantic`, `:573` `DIMENSION_MISMATCH_WARNED`, `:581-609` `embed_text`, `:639-664` `reserve_background_slots`, `:669-1243` `try_semantic`, `:1245` `apply_recall_multipliers`, `:1325` `expand_related_memories`, `:1457-1594` `search_memories_text`, `:1596` `STOP_WORDS` |
| `memory-service.ts` `markMemoriesAccessed` (`:1373-1385`) | `services/memory_access.rs:30` `mark_memories_accessed` (DEBUG `[Memory] Marking memories accessed`) |
| `context/memory-injector.ts` whole (822): `formatMemoryMetadataTag`, `formatMemoriesForContext`, `formatInterCharacterMemoriesForContext`, `formatFrozenMemoryArchive`, `formatDynamicMemoryHead`, `DebugMemoryInfo` (effectiveWeight), `sizeMemoryPools` + constants | `memory_injector.rs` (1,534): `:89-160` `MetadataTagOpts`/`format_memory_metadata_tag`, `:672` `format_memories_for_context`, `:785` `format_inter_character_memories_for_context`, `:987` `format_frozen_memory_archive`, `:1061` `format_dynamic_memory_head` (`[m_…]` at `:1124-1125`), `:430` `DebugMemoryInfo`, `:262-345` `InjectorMemory` (`effective_weight()` `:303`), `:405-421` `RecallAdjustment`/`InjectorResult` (`effective_weight`, `raw_weight`); `memory_pools.rs` (137; sizing + `PROACTIVE_RECALL_POOL_SIZE` `:51`) |
| `memory-injector.ts` scene-state + summary formatters → `context/scene-state.ts` (verbatim) | `memory_injector.rs:488-664` (scene state), `:1208` `format_summary_for_context`. STAY, maybe relocate. |
| `frozen-archive-cache.ts` → `recall/archive.ts` | `services/frozen_archive.rs` (1,123; `:231` `get_or_compute_frozen_archive` — no `exclude`; `:427` `rank_by_effective_weight`) |
| `memory-recap.ts` → `recall/recap.ts` (positional → `MemoryRecapRequest`, `excludeIds`, `relevanceEmbedding`, `memoryIds`) | `services/memory_recap/mod.rs` (946; `:640-652` `MemoryRecapParams`, `:55-57` `MemoryRecapResult {content}`, `:671` `generate_memory_recap`) |
| `recall-replay.ts` → `recall/replay.ts` | `services/recall_replay.rs` (670; `:148` `"rawWeight"`, `:652-668` result `json!`) |
| `recall-tags.ts`, `recall-tuning.ts`, `recall-history.ts`, `memory-subject.ts`, `conversation-summary-search.ts` (renames, no behaviour) | `recall_tags.rs` (1,259), `recall_tuning.rs` (716), `recall_history.rs` (211), `services/memory_subject.rs` (218), `services/memory_recap/mod.rs:105`/`:171` (vault conversation search). No change needed beyond §3. |
| `cheap-llm-tasks/memory-tasks.ts` `extractMemorySearchKeywords` → `recall/query.ts` | `services/memory_recap/distill.rs` (681; `:348` `distill_memory_search`, `:388` `parse_extraction`, `:69` `DistilledSearch`). Bug 212's zone is the time lane's. |
| `pre-compute.service.ts` `ProactiveRecallOutcome {memories, signals, queryEmbedding}` | `services/pre_compute.rs:119-130` (same), `:204` `proactive_recall_task` |
| context-manager `INTER_CHAR_*` consts, `MEMORY_RECAP_PHASE_TIMEOUT_MS`, `RETRO_MINI_RECAP_MAX_ENTRIES`, the two-pool / inter-char / per-turn-list / mini-recap / ring blocks | `services/build_context.rs:173-178` (consts), `:1594`, `:1606`; `:1231` `persist_recall_history`; `:1746` `resolve_retrospective_mini_recap`; `:2303-2372` recap; `:2374-2805` two-pool + archive + head + stamp; `:2840-3000` inter-character; `:3476-3593` per-turn list + mini-recap; `:3620-3645` ring write |

---

## 3. The behaviour deltas a port must hit

Each delta names its v4 pin site, then the v5 landing site on `a7c6f0d03`.

### Query and search

1. **One query per turn.** `buildRecallQuery` (`query.ts:419-525`) embeds the main text and ≤ 2 probes once. Every
   later stage reads `RecallQuery`.
   - The fallback-head distil and embedding (`build_context.rs:2476-2683`) and the proactive embedding
     (`pre_compute.rs:~370-420`) both funnel into it.
   - `search_memories_semantic`'s `embed_text` and `capture_query_embedding` out-param (`memory_service.rs:513-609`)
     DIE.
2. **The text strategy honours every option** (`gather.ts:260-303` + `score.ts`): probes, anchors, max-over-probes
   scoring, exclude, cold, importance, source, about, the window (filter, hard or soft), the full recall multipliers
   with R1's gate off, and salience at `nowMs`.
   - v5's `search_memories_text` (`memory_service.rs:1457-1594`) does none of these. It becomes `gather_text` plus the
     shared stages.
3. **The dimension test** (`gather.ts:130-144`) is `size > 0 && matching === 0` on the index shape. The WARN loses
   `userId`/`embeddingProfileId`. v5: `memory_service.rs:~730-760`.
4. **The search-failure WARN** (`search.ts:115`) loses `userId`. **A query-embedding failure** logs the NEW WARN
   `[Recall] Query embedding failed; recall falls back to text search` instead (`query.ts:477`). v5:
   `memory_service.rs:553-562`.
5. **Retired log lines:** `[Memory] TF-IDF embeddings: recall boost gate off` (v5 `memory_service.rs:778`) and
   `[Memory] Background reservation (R6)` (v5 `:1211`). The probe-failure DEBUG becomes `[Recall] Probe embedding
   failed; skipping probe` `{probe, error}` (v5 `:828`, which also carried `characterId`). `[Memory] Specific entity
   anchors chosen` STAYS (v5 `:942`).
6. **Importance filters** read `reinforcedImportance ?? importance` (`hydrate.ts:34`). This is the Phase A scoring
   lane's, and it changes recall membership. v5: `memory_service.rs` minImportance filters and `expand_related_memories`.
7. **`RecallCandidate.salience` replaces `effective_weight`/`raw_weight`** in every carrier:
   - `memory_service.rs:288-306`;
   - `memory_injector.rs:405-421`;
   - `tools/search.rs:393`, `:824-839` (`"effectiveWeight"` becomes `"salience"`; `importance` becomes `filterImportance`);
   - `tools/self_inventory.rs:320-321`, `:1247`, `:2645-2648` (`weight` becomes `salience` in the text);
   - `services/tool_execution.rs:178-179`;
   - `build_context.rs:4060-4089` `debug_memory_json` (`effectiveWeight`/`rawWeight` become `salience`);
   - `recall_replay.rs:148`.
8. **The trace.** Every stage appends one entry with v4's key order (§1.2–1.11). Numbers are JS-formatted, and
   `bestCosine` uses `Math.round(x*1000)/1000`.

### Assembly and rendering

9. **Archive budget pinned to ordinary; the head borrows** (`assemble.ts:356`, `:416-418`). v5: `build_context.rs:2691-2697`.
10. **The recap is reordered and narrowed** (`assemble.ts:376-407`):
    - it runs after the head query and the archive;
    - it reuses the turn query's text and vector;
    - it excludes archive ids (fetching extra rows per tier);
    - it reports `memoryIds`;
    - its ERROR line is re-prefixed `[Recall]`.

    v5: `build_context.rs:2303-2372`; `memory_recap/mod.rs:640-700` (params and the tier fetch with limits);
    `:55-57` (result).
11. **Inter-character relevance is a filter of the turn's ranked pool, past the head's cut** (`assemble.ts:493-506`).
    Both halves skip archive and head ids, with a `skippedAsDelivered` count (bug 224). The per-character searches DIE.
    v5: `build_context.rs:2905-2945` (`search_memories_semantic` per other id), `:2947` formatter, `:2967` stamp.
12. **`renderInterCharacter` ranks the importance half by salience at `nowMs`** (`render.ts:295-303`). v5:
    `memory_injector.rs:785-985`.
13. **Model-visible text** (`render.ts:134-322`; the table in §1.10):
    - no `[m_xxxx]` tag;
    - no ` _(importance … · relevance … · weight … · recall: …)_` marginalia;
    - keywords filtered by `humanKeywords`;
    - the head sort uses salience, not floored weight.

    This applies on **every** recall-carrying request: the Salon turn, the chained/continue trailing message, Carina's
    `## Relevant Memories`, and both voice rehearsals. v5: `memory_injector.rs:109-160` (`format_memory_metadata_tag`
    DIES), `:672`, `:785`, `:987`, `:1061`; the callers `carina_query.rs:1116-1143` and
    `voice_rewrite_core.rs:107-176`.
14. **The per-turn list and the mini-recap ride the turn's query** (`assemble.ts:526-617`):
    - the per-turn list needs `query.embedding`;
    - the mini-recap query is `query.text` + entities, reusing the vector when there are no entities;
    - its suppression and listed DEBUG lines become trace entries;
    - the failure WARNs are re-prefixed `[Recall]`.

    v5: `build_context.rs:3476-3593`, `:1746-1850`.

### Delivery

15. **Delivery is stamped after the body is built, not at format time** (`deliver.ts:125-182`;
    `context-manager.ts:2285-2295`):
    - only lines that survive are stamped;
    - the recap is never stamped;
    - the ring holds the delivered HEAD ids only;
    - the ring write is no longer gated on the whisper post (§0.1.6);
    - the retro signature is written only when the mini-recap text is in the delivered text.

    v5: `build_context.rs:2765-2778` (archive + head stamp), `:2967` (inter-char stamp), `:3620-3645` (ring inside
    `if posted`), and `memory_access.rs` (DEBUG text).
16. **The recall record** (`deliver.ts:170-174`) goes into `BuiltContext.recallDebugLog`. The finalizer writes it to
    the assistant message's `debugMemoryLogs` (`message-finalizer.service.ts:290-302`).
    - v5 `BuiltContext` (`build_context.rs:329-376`) has no such field, and `message_finalizer.rs` has no write (the
      slot goes between the route-trail write `:996-1040` and the inform consume `:1047-1095`).
    - The extraction job must then APPEND via `withRecallLines` (v5 `memory_extraction_job.rs:458` overwrites; the
      extraction lane's). The Carina extraction write (`carina_memory_extraction.rs:290`) is retired by Phase D (the
      extraction lane).
17. **Compression-aware survival** (`survivingLines`, `deliver.ts:77-93`). It is a pure function. v5's Phase-2 memory
    compression is a TRACKED DEFERRAL (`build_context.rs:3057-3090`), so in production every section is "untouched" and
    only the verbatim arm fires. Port both arms and pin the word-overlap arm tier-1.
18. **Debug-line re-prefixes** (exact texts in §1.9–1.11, §1.15):
    - `[Memory] Marking memories accessed` → `[Recall] Stamping delivered memories accessed`
      (`memory_access.rs:30-60`, `build_context.rs`);
    - NEW DEBUG `[Recall] Delivered` and `[ContextManager] Recall assembled`;
    - RETIRED `[ContextManager] Memory pools sized` (v5 `build_context.rs:2738-2752`) and `[ContextManager]
      Inter-character memory retrieval complete` (`:2985-2998`);
    - `[CommonplaceWhisper] Failed to persist recall history` → `[Recall] Failed to persist the anti-repetition ring`.

### Trailing sections and mail

19. **ONE trailing list; the no-new-message branch carries everything** (`context-manager.ts:2228-2283`, bugs
    199/227). The order is `[core, recall, mail, progressions, inform, turnSkip]`; the trailing-only message is
    `[anchor, ...list]`, pushed only when non-empty; NEW DEBUG `[CommonplaceWhisper] Delivering recall in the
    trailing-only message (no new user message)`.
    - v5 has two hand-kept copies: `build_context.rs:3875-3911` (correct order) and `:3934-3966` (the pre-fix
      four-item list). **v5 reproduces bug 199 today.**
20. **The mail flip is deferred to delivery** (`context-manager.ts:2110-2141`; `mailbox.ts:288-331`):
    - `BuiltContext.mailAlertsOwed`;
    - **a swipe skips the mail check**;
    - four `via` marks: finalizer, preserved-partial, tool-only and turn-skip.

    v5 `suparna_mail.rs:66-107` flips at collect time (`:95-104`). `build_context.rs:3677-3705` has no
    regeneration gate. `post_office/mailbox.rs:395` `mark_alerted` gains the sibling `mark_mail_alerts_delivered`. The
    marks go at `message_finalizer.rs:1047-1095`, `primary_stream.rs:677-860` (`inform_row_ids` sibling),
    `orchestrator.rs:3469` (turn skip) and `:3951` (tool-only).
21. **`BuiltContext` wire order:** `mailAlertsOwed` (always, null) goes after `informRowIds`; `recallDebugLog?` goes
    after `debugRelevantConversations`. `build_context_tier3` compares this JSON byte-for-byte.

### Options and thresholds

22. **The `BuildContextOptions` swap.** `preSearchedMemories`, `preSearchedQueryEmbedding` and `recallSignals` become
    `preRecall`. The pre-searched path is taken only with a non-empty query text and non-empty candidates; otherwise
    the signals are discarded and a full distil runs (§1.9 step 1). v5: `build_context.rs:620-633`,
    `orchestrator.rs:4483-4492`, `:4630`, `regenerate_swipe.rs:690-692` and `message_context.rs:2492-2494`.
23. **The importance floor for the head and proactive pass is 0.3** (Phase A constant; `context-manager.ts:674`,
    `context-builder.service.ts:1084`). v5: `orchestrator.rs:4595` (`0.5`), `pre_compute.rs:~410` (`0.3`, unchanged).
24. **First-message order is salience, not importance** (`first-message-context.ts:221-224`). v5:
    `first_message_context.rs:247-256`.
25. **Search tool:**
    - metadata `salience` and `importance: filterImportance`;
    - the window applies on the text fallback;
    - the importance band of 0.3 (the Phase A lane's).

    v5: `tools/search.rs:366-396`, `:820-850`, `:985-990`.

### Replay and Memories API

26. **Replay** (§1.12): the rows' `salience`, `fallbackPath`, `ring`/`ringSource` from `ringAsOf`, `pools`, `trace`,
    an always-read present-character list, and an `assembleRecall` call. v5: `services/recall_replay.rs:96-160`,
    `:185-669`; the CLI `quilltap-cli/src/recall_replay_cmd.rs` (`print_path` `:196`, plus a fallback table, trace and
    ring line, and the bug-239 flags).
27. **The Memories API search** is unchanged in shape; only its ranking moves (§0.1.1). The GET `/memories/[id]` stamp
    (bug 219) is the memories-API lane's: v5 `api/memories.rs:405` still stamps, which breaks "stampAccessed is the
    ONE writer".

---

## 4. v5 on `main` today (`a7c6f0d03`)

| File | Today | At the pin |
|---|---|---|
| `crates/quilltap-core/src/services/memory_service.rs` (2,151) | The search half `:280-1660`. The cascade-delete half `:89-275` and the backfill `:1662-1900` belong to the retire and index lanes. | The search half is REPLACED by the pipeline. `RecallContextInput` (`:327-357`) can survive as the owned form of the recall context. Delete `SemanticSearchResult`, `SearchQueryEmbedding`, `search_memories_semantic`, `try_semantic`, `search_memories_text`, `expand_related_memories`, `apply_recall_multipliers`, `embed_text`. `EmbeddingMemoMap` (`:443-501`) MOVES to the query stage. |
| `memory_injector.rs` (1,534) | The formatters plus scene state and summary. | The memory formatters DIE in favour of `render`. `format_memory_metadata_tag` (`:89-160`) DIES. Scene state (`:488-664`) and the summary (`:1208`) STAY, optionally moving to `scene_state.rs`. The `memory_is_cold`/`memory_is_digest` helpers (`:394-403`) stay; other callers use them (`tools/search.rs:891`, `optimizer.rs:1815`, `first_message_context.rs:169/233`, `memory_recap/mod.rs:658`). |
| `memory_pools.rs` (137) | `size_memory_pools` + constants | UNCHANGED values. The module stays, or moves under the recall assemble module. |
| `recall_tags.rs` (1,259) | The multiplier table, `combine_recall_multipliers`, `RecallContext` | No behaviour delta. ADD `SCOPE_VALUES` (or reuse `ScopeTag::from_kw`) for `is_targeting_keyword`. |
| `recall_tuning.rs` (716) | Schemas, resolve/describe, `select_specific_anchors` (`:208`) | No behaviour delta (§1.13). |
| `recall_history.rs` (211) | Ring parse and append | No delta. `RECALL_HISTORY_TURNS` feeds `ring_as_of`. |
| `services/frozen_archive.rs` (1,123) | Cache + compute (`:321`), effective-weight rank `:427` | Gains `exclude` (compute, bypassing the cache). The rank becomes `sort_by_salience` (the scoring lane's function). |
| `services/memory_recap/mod.rs` (946), `distill.rs` (681), `prompt_text.rs` | Recap, vault conversation search, distil | The recap takes `exclude_ids` + `relevance_embedding` and returns `memory_ids`. The distil is unchanged here (the zone is the time lane's). |
| `services/memory_subject.rs` (218) | `build_memory_subject_context` | Unchanged. Its one per-turn call site moves into assemble. |
| `services/memory_access.rs` (172) | `mark_memories_accessed` | Renamed and re-texted: `stamp_accessed` with `[Recall] Stamping delivered memories accessed`. The ONE writer. |
| `services/build_context.rs` (5,428) | `:2303-3000` (recap, two-pool, inter-char), `:3476-3645` (lists, ring), `:3677-3705` (mail), `:3845-3966` (trailing), `:3981-4045` (return), `:4060-4089` (debug JSON) | Most of `:2303-3000` and `:3476-3645` collapse into ONE `assemble_recall` call plus `deliver_recall` after the trailing build. The mail block defers. ONE trailing list. The fields (`:620-633` input, `:329-376` output) change. |
| `services/pre_compute.rs` (630) | `ProactiveRecallOutcome` `:119-130`, task `:204-430` | Returns `Option<PreRecall>`. The WARN text changes. |
| `services/recall_replay.rs` (670) + `api/recall_replay.rs` (248) + `api/types.rs:3041-3071` (`ChatRecallReplay`), `:4565` (`Response::RecallReplay(Value)`) | Old/new tables only | The result grows (§7.4). The request variant is UNCHANGED. |
| `services/message_finalizer.rs` (1,986) | Inform consume `:1047-1095` | + the recall-record write before it, + the mail mark after it. |
| `services/primary_stream.rs`, `services/orchestrator.rs` | Inform ids on preserve-partial (`primary_stream.rs:677-700`, `:826-860`); turn skip `orchestrator.rs:3469`/`:3852`; tool-only `:3951` | + `mail_alerts_owed` threading + marks. `orchestrator.rs:4595` min importance becomes 0.3. |
| `services/suparna_mail.rs` (156), `post_office/mailbox.rs:395` | Flip at collect | Split: collect + LLM context returns the owed paths; flip moves to `mark_mail_alerts_delivered`. |
| `services/carina_query.rs:1073-1145` | `format_memories_for_context` + stamp | `render_relevant` + `stamp_accessed`. |
| `services/announcer/voice_rewrite_core.rs:107-176` | `format_dynamic_memory_head` + stamp | `render_head` (200 / 12) + `stamp_accessed`. |
| `services/first_message_context.rs:101-266` | Importance sort | Salience sort; `search_memories`. |
| `tools/search.rs` (1,214), `tools/self_inventory.rs` | `effectiveWeight` | `salience` (§3.25, §3.7). |
| `api/memories.rs:905-930` | Search | `search_memories` (no stamp, unchanged). |
| `services/anchor_gate_probe.rs` (736) | | NOT this lane (§1.14). |
| `quilltap-cli/src/recall_replay_cmd.rs` (627) | Two tables | + ring line, fallback table, stage trace, `-d/-i/--passphrase`. |

---

## 5. Harness

**Tiers:** 1 = exact pure function; 2 = DB state; 3 = mocked LLM.

### 5.1 IMPORT-DEAD at the pin

These cases import, `doMock` or `requireActual` a path v4 deleted. A `jest.doMock` of a non-existent module throws
`Cannot find module` at registration unless `{virtual:true}`; confirm this on the first regen.

| Harness family | Oracle case: the dead reference | Tier |
|---|---|---|
| `memory_injector_equivalence` | `cases/memory-injector.ts:60` `@/lib/chat/context/memory-injector`, `:63` type from `@/lib/memory/memory-service` | 1 |
| `memory_pools_equivalence` | `cases/memory-pools.ts:35` memory-injector (`:36` pre-compute's `PROACTIVE_RECALL_POOL_SIZE` still resolves) | 1 |
| `semantic_search_tuning_tier3_equivalence` | `cases/semantic-search-tuning-tier3.test.ts:188`, `:233` memory-service; `:189` `recall-tuning` | 2/3 |
| `frozen_archive_tier2_equivalence` | `cases/frozen-archive-tier2.test.ts:133` `frozen-archive-cache` | 2 |
| `build_context_tier3_equivalence` | `cases/build-context-tier3.test.ts:386` `await import('@/lib/memory/frozen-archive-cache')` (`resetFrozenArchiveCacheForTests`). Its `preSearchedMemories`/`recallSignals`/`preSearchedQueryEmbedding` options (`:508-568`) are silently IGNORED at the pin. | 3 |
| `orchestrator_tier3_equivalence` | `cases/orchestrator-tier3.test.ts:547-548` `doMock('@/lib/memory/memory-recap', requireActual…)` | 3 |
| `regenerate_swipe_tier3_equivalence`, `salon_swipe_generate_equivalence` | `regenerate-swipe-tier3.test.ts:266-271`, `salon-swipe-generate.test.ts:256-261`: memory-recap + frozen-archive-cache | 3 |
| `context_feeders_leaves_equivalence` | `cases/context-feeders-leaves.ts:29` `conversation-summary-search` | 1 |
| `vault_conv_search_equivalence` | `cases/vault-conv-search.ts` `conversation-summary-search` | 2 |
| `recent_conversations_block_equivalence` | `cases/recent-conversations-block.ts` memory-recap | 1 |
| `distill_search_extraction_equivalence` | `cases/memory-search-extraction.test.ts:109` `cheap-llm-tasks/memory-tasks` (re-point to `recall/query` or the `cheap-llm-tasks` barrel) | 3 |
| `recall_tags_equivalence` | `cases/recall-tags.ts:47-48` `recall-tags`, `recall-tuning` | 1 |
| `recall_tuning_equivalence` | `cases/recall-tuning.ts:31` | 1 |
| `recall_history_equivalence` | `cases/recall-history.ts:33` | 1 |

Siblings' families that are also dead on `memory-service` or `memory-weighting`:
- `ranking_blend_equivalence`, `memory_weighting_equivalence` (scoring lane);
- `memories_tier2`, `memory_cascade_tier2`, `memory_gate_tier3`, `memory_watermark_tier3` (other lanes);
- the fixture builders `build-episodic-recall-fixture.ts`, `build-memories-web-fixture.ts` (memory-service).

Every recall oracle that seeds vectors through `vector_entries` needs `memories.embedding` at the pin (Phase B; memory
note "Memory search plants need vectors").

The tier-1 rename-only families (`recall_tags`, `recall_tuning`, `recall_history`, `recent_conversations_block`,
`vault_conv_search`, `context_feeders_leaves`) **should go GREEN once their import paths are edited**. They are a
path-only re-aim with no v5 code change.

### 5.2 RED at the pin (bytes or shape move)

**Request bytes with memories (tier 3):**
- **`build_context_tier3_equivalence`**: 28 of 70 ops run with memories on, and 31 memories are seeded. Every one of
  them moves:
  - the archive/head/inter-char text;
  - `debugMemories` keys;
  - `mailAlertsOwed: null` + `recallDebugLog`;
  - the `whispers` row's `commonplaceRecallHistory` (the ring now written after delivery; written even when the post
    did not happen);
  - the continue op `empty_new_user_message_single` (the trailing-only message now carries recall);
  - the pre-searched ops (`preRecall` shape);
  - the retro ops (the mini-recap query text);
  - the recap ops (`excludeIds` changes the cheap-LLM recap prompt bytes, so the `completionRules` keyed on exact
    messages need re-recording).
- **`carina_query_tier3_equivalence`**: `## Relevant Memories` text and order; the stamp DEBUG line.
- **`in_scene_voiced_tier3_equivalence`**, **`announcer_tier3_equivalence`**: `recallForSeed`'s head (no `[m_]`/meta;
  salience order); the stamp DEBUG.
- **`orchestrator_tier3_equivalence`**: `preRecall` threading. `debugMemoryLogs` appears on the assistant row whenever
  `llmRecallText` is non-empty (current state alone suffices). The fixture seeds NO memories (`build-orchestrator-
  fixture.ts:614`). Expect the tool-only and turn-skip mail marks only if a letter is seeded (none today).
- **`enclave_step_tier3_equivalence`**: autonomous turns take the trailing-only branch. The Core whisper is skipped in
  continue mode only, so any Core, recall or mail section now rides there. `debugMemoryLogs` lands on rows.
- **`message_finalizer_tier3_equivalence`**: the recall-record write and the `markMailAlertsDelivered` DEBUG.
- **`primary_stream_tier3_equivalence`**: the preserved-partial mail mark.
- **`regenerate_swipe_tier3` / `salon_swipe_generate`** (once re-aimed): a swipe skips mail and delivers recall (stamps,
  ring) with no record.

**Shapes and logs:**
- **`recall_replay_equivalence`** (tier 2/3): rows (`salience`), the five new keys, ranking.
- **`precompute_equivalence`**: `PreContextPreComputeResult` (`preRecall`), the WARN text.
- **`first_message_context_equivalence`**: order (salience) and the stamp DEBUG.
- **`search_tools_equivalence`**: metadata `salience`/`importance`, the stamp DEBUG, the text-fallback window.
- **`memories_routes_equivalence`**: search ranking only, if its corpus distinguishes salience from floored weight.
- **`memory_subject_call_site_guard`** (source guard): it re-aims `services/build_context.rs` at the assemble module,
  per pin call sites `assemble.ts:353`, `carina.service.ts:230`, `voice-rewrite-core.ts:82`.

**Predicted GREEN:**
- `skip_signal_equivalence` (`buildTurnSkipInstruction` still exported, `context-manager.ts:559`);
- `post_office_commonplace_equivalence` (`writer.ts` unchanged);
- `knowledge_injector_equivalence`, `user_narration_anchor_equivalence`, `inform_block_equivalence`.

The `p4_d265_wire_contract` replay request decode stays GREEN because the request is unchanged.

### 5.3 Families to ADD (with the closest recipe)

1. **`recall_probe_fixture_equivalence`** (tier 1, NEW): consumes v4's `__tests__/fixtures/recall-probes/probe-set.json`.
   - **What the fixture is:** 18,012 lines. One synthetic character, `0b1c2d3e-…f41d`, with 158 memories carrying
     64-dimension vectors whose cosines to each query are set by construction. There are 15 embedding-table entries
     (each query and probe), and 9 turns (A, B, C, F1, F2, M, O2, O3, O4) with 5–18 `expected` rows each,
     `{id, score, salience, blendedAfter, fired, usedEmbedding}`. `goldAtHead` is 24 of `goldTotal` 44.
   - It exercises R1/R2, R4 anchors, probes, the soft window and the hard window (O3), expansion, the ring,
     cross-project scope under both policies, the fresh boost, cold rows, and the text fallback (O4's query embeds at
     a different length).
   - The rows were captured from pre-Phase-E `searchMemoriesSemantic` under `cap14` after Phases A–D. v4's own
     `probe-fixture.test.ts` (3 cases) asserts `searchRecall` reproduces every row at 1e-9.
   - **Oracle:** a jest case beside v4's test. It reuses `__tests__/fixtures/recall-probes/harness.ts` `probeWorld`,
     mocks `factory`, `embedding-service` and `index-store` the same way, and emits NDJSON per turn: candidates + trace.
   - **Rust:** run `search_recall` over the same JSON through an in-memory reads seam.
   - ⚠ The ids are NOT UUIDs (`10000001-0000-4000-8000-ax0000000001`; `chatId: 'chat-older'`). v5's raw memory reads
     DROP non-UUID rows (memory note "Memory raw reads drop non-UUID fixtures"). The family must not round-trip through
     the DB unless the ids are remapped (§9.2).
   - Recipe to copy: `recall_tuning_equivalence` (tier 1, JSON corpus).
2. **`recall_render_equivalence`** (tier 1): `renderArchive`/`renderHead`/`renderRelevant`/`renderInterCharacter` +
   `humanKeywords`/`isTargetingKeyword`/`formatKeywordsTag`.
   - Feed it the pin's `render.test.ts` corpus shapes, including `CONTEXT_VALUES` collisions (`history`) and
     `scope:  Narrow`.
   - It replaces the formatter rows of `memory_injector_equivalence`. Scene state and summary re-aim to a
     `scene-state` oracle importing `@/lib/chat/context/scene-state`.
   - Recipe: `memory_injector_equivalence`.
3. **`recall_deliver_equivalence`** (tier 1):
   - `survivingLines` (verbatim, case/whitespace, the 3-word floor, the 0.6 share, the Unicode strip);
   - `traceLines`;
   - `deliverRecall`'s `debugLog` composition from a planted bundle (stamps and ring through the tier-2 rig);
   - `ringFromDebugLog`, `withRecallLines`, `ringAsOf`.
4. **`recall_assemble_tier3`** — or fold into `build_context_tier3` (§9.6):
   - one-embedding counts (pin `assemble-deliver.test.ts`);
   - bug-224 dedup;
   - relevance past the cut;
   - the recap keeping its rows in the head;
   - the retro mini-recap reuse;
   - "writes nothing until delivery".
5. **`build_context_tier3` growth:**
   - ops with a seeded letter (closes the MEASURED GAP at `build_context_tier3_equivalence.rs:51`): new-message, continue,
     swipe (skipped), and a provider failure (the letter stays unalerted);
   - a continue op with memories + Core whisper + mail + inform, pinning the bug-199 order;
   - a `stamps` row (memory ids whose `lastAccessedAt` became non-null, mask the time);
   - `recallDebugLog` compared with `[ContextManager] Recall assembled`'s `durationMs` masked.

---

## 6. v4 jest tests at the pin a lane should mirror

| Test | Cases | Covers |
|---|---|---|
| `__tests__/unit/lib/memory/recall/probe-fixture.test.ts` | 3 (+ 9 `it.each` rows) | `searchRecall` over the probe set; `cap14` constants |
| `__tests__/unit/lib/memory/recall/assemble-deliver.test.ts` | 15 | one embedding, bug 224, relevance past the cut, recap rows in the head, retro reuse, fallback search, "writes nothing", deliver stamps/ring/recap/mini-recap, byte-stable archive on retro |
| `__tests__/unit/lib/memory/recall/render.test.ts` | 32 | all four renderers, marginalia gone, `humanKeywords` |
| `__tests__/unit/lib/memory/recall/recall-stages.test.ts` | 10 | score (R1 off for text, salience clock, exclude policy), select (expansion, no text expansion, cut), deliver survival, ring readback + `withRecallLines` |
| `__tests__/unit/lib/memory/recall/build-recall-query.test.ts` | 7 | `recallQueryText`, embed-once, probes, failure, memo, distil fallback, empty |
| `__tests__/unit/lib/memory/recall/search-text-fallback.test.ts` | 5 | bug 214 |
| `…/recall/search-{hot-tier,hybrid-recall,recall-tuning}.test.ts` | 4 / 5 / 15 | renamed from `memory-service-*` |
| `lib/memory/recall/__tests__/query-{day-reference,keywords}.test.ts` | 12 / 6 | distiller (bug 212 is the time lane's) |
| `__tests__/unit/lib/memory/recall-replay.test.ts` | 31 | replay incl. `ringAsOf`, `fallbackPath`, pools, trace |
| `__tests__/unit/app/api/v1/chats/[id]/actions/recall-replay.test.ts` | 22 | the route (only imports moved) |
| `__tests__/unit/context-management.test.ts` | 90 | `:1448-1585` trailing sections both paths + Core whisper; `:1587-1685` mail (bugs 199/227: rides, owes, swipe skips); `:1686-1760` recall on a continue turn (stamped, ring, debug record) |
| `__tests__/unit/lib/services/chat-message/message-finalizer.service.test.ts` | 15 | recall record + mail mark |
| `__tests__/unit/lib/services/chat-message/primary-stream.service.test.ts` | 19 | preserved-partial mail mark |
| `lib/post-office/__tests__/mailbox.test.ts` | 21 | `markMailAlertsDelivered` |
| `__tests__/unit/lib/services/chat-message/pre-compute.service.test.ts` | 19 | `preRecall` |
| `__tests__/unit/lib/memory/memory-recap.test.ts` | 31 | `MemoryRecapRequest`, `excludeIds`, `memoryIds`, embedding reuse |
| `__tests__/unit/lib/memory/frozen-archive-cache.test.ts` | 16 | `exclude`, salience fill |
| `__tests__/unit/lib/chat/context/scene-state.test.ts` | 14 | moved formatters |
| `__tests__/unit/lib/chat/first-message-context.test.ts` | 18 | salience order, `stampAccessed` |
| `lib/services/carina/__tests__/carina.service.test.ts` | 57 | `renderRelevant` |
| `__tests__/unit/lib/tools/handlers/search-scriptorium-handler.test.ts` | 18 | salience metadata |
| `__tests__/unit/lib/memory/memory-subject.test.ts`, `digest-provenance.test.ts`, `ranking-blend.test.ts` | 4 / 2 / 13 | — |

---

## 7. Cross-lane meeting points and PROPOSED C1 spellings

### 7.1 NEEDS from siblings (this lane consumes; does not port)

**From the scoring lane (Phase A):**
- `salience(memory, now_ms) -> f64` (reinforcedImportance × decay, no floor) + `sort_by_salience`;
- `compute_ranking_blend(relevance, salience)` (0.75/0.25);
- `filter_importance(memory)`;
- `RECALL_MIN_IMPORTANCE = 0.3`, `TOOL_MIN_IMPORTANCE = 0`, `importance_band`;
- `default_min_cosine_for_provider`;
- `is_lexical_embedding_provider(provider)` (`{BUILTIN}`; the TF-IDF lane).

`rank.ts` (`rank_for_recall`, `rank_by_blend`, `reserve_background_slots`, `by_blended_after`) is NEW in Phase E;
**this lane can own it** unless the scoring lane claims `memory_scoring::rank`.

**From the time lane:**
- `format_memory_age(memory, now_ms)` (pre `formatRelativeAge` body);
- `refresh_memory_time_zone`, `normalise_time_range`, `resolve_reference_window`, zoned TODAY (the distiller).

**From the index lane (Phase B):**
- `memory_index::search(character_id, &[f32], MemoryIndexSearch { limit, hot_only, exclude: Option<&HashSet<String>> }) -> Vec<MemoryIndexHit{ id, score }>`;
- `memory_index::shape(character_id, query_dims) -> MemoryIndexShape { size, dominant_dimensions: Option<usize>, matching }`.

Recall stops touching `db/vector_store.rs` / `vector_indices.rs` entirely.

**From the repositories (keystone / bug 215):**
- `memories_read::find_by_ids`, `search_by_content`, `find_hot_digests(self, about, 2)`,
  `find_by_character_about_characters(self, others, 5)`;
- `find_recent_by_importance_tier(conn, id, high, medium, low)` (`memories_read.rs:707`) banded on
  `reinforcedImportance` (bug 215, `memories.repository.ts:485-…`);
- `update_access_time_bulk` (`db/memories.rs:674`);
- `count_created_since`, `find_recent` (replay).

### 7.2 PROVIDES to siblings

- `stamp_accessed`, the ONE `lastAccessedAt` writer. The memories-API lane removes `api/memories.rs:405`.
- `with_recall_lines`, which the extraction lane's job uses when it writes `debugMemoryLogs`.
- `search_memories` (the one-shot), for the tool, API, Carina, voice and greeting lanes.
- `RecallCandidate` (replacing `SemanticSearchResult`) for every caller.
- `MailAlertsOwed` + `mark_mail_alerts_delivered` for the orchestrator, finalizer and primary-stream sites (the same
  lane or a small Post Office lane).

### 7.3 Proposed spellings

**The recall pipeline entry.** A new directory `crates/quilltap-core/src/services/recall/`, mirroring v4's stages:

```rust
pub mod types;    // HitSource, RawHit, HydratedHit, RecallAdjustment, RecallCandidate,
                  // RecallStage, RecallTraceEntry, RecallTrace = Vec<RecallTraceEntry>, RenderedLine, RecallPool,
                  // fn trace_stage(&mut RecallTrace, RecallStage, serde_json::Map<String, Value>, Option<&str>)
pub mod query;    // RecallQuery { text, embedding: Option<EmbeddingResult>, probes: Vec<EmbeddedText>,
                  //   signals: Option<DistilledSearch>, anchors: Vec<String>, window: Option<TimeWindow>,
                  //   retrospective: bool, embedding_error: Option<String> }
                  // BuildRecallQueryInput<'a> { user_id, embedding_profile_id, text, signals, distill:
                  //   Option<RecallDistill<'a>>, embedding_memo: Option<EmbeddingMemo>, skip_embedding, probes }
                  // pub async fn build_recall_query<E, C>(db, embedding: &E, completion: &C, executor,
                  //   input: &BuildRecallQueryInput<'_>, trace: &mut RecallTrace) -> RecallQuery
                  // pub fn recall_query_text(signals: Option<&DistilledSearch>, fallback: &str) -> String
pub mod gather;   // GatherOptions, Gathered { strategy: RecallStrategy, hits, min_score, provider, literal_phrase,
                  //   literal_hit_ids }, gather(..), gather_text(..), significant_query_words, score_text_match
pub mod hydrate;  // HydrateFilters, passes_filters, hydrate, hydrate_neighbours
pub mod score;    // ScoreOptions, Scored { .., window_mode: WindowMode }, score(..)   (pure)
pub mod select;   // SelectOptions, Selected { candidates, ranked }, select_ranked, wants_expansion (pure)
pub mod search;   // RecallSearchOptions, RecallSearchResult { candidates, ranked, strategy, trace },
                  // pub async fn search_recall(db, character_id, &RecallQuery, &RecallSearchOptions,
                  //   trace: &mut RecallTrace, label: Option<&str>) -> Result<RecallSearchResult, DbError>
                  // pub async fn search_memories<E>(db, embedding, character_id, text, &SearchMemoriesOptions)
                  //   -> Result<Vec<RecallCandidate>, DbError>
pub mod assemble; // RecallPlan<'a>, PreRecall { query, candidates, ranked: Option<Vec<_>>, trace: RecallTrace },
                  // RecallBundle, RecallPath::{Skipped, PreSearched, TwoPool}, empty_recall_bundle,
                  // pub async fn assemble_recall<E, C>(db, embedding: &E, completion: &C,
                  //   executor: &CheapLlmTaskExecutor, plan: &RecallPlan<'_>) -> RecallBundle
pub mod render;   // RenderedSection<D>, DebugMemoryInfo { memory_id, summary, importance, score, salience,
                  //   recall_multiplier, recall_fired, blended_before, blended_after }, render_archive,
                  //   render_head(.., RenderHeadOptions { max_tokens, max_entries, now_ms }),
                  //   render_relevant, render_inter_character, human_keywords, is_targeting_keyword,
                  //   format_keywords_tag   (pure; take `chars_per_token: f64`, §9.3)
pub mod deliver;  // see 7.4
pub mod replay;   // RecallReplayResult, RecallReplayRow, RingSource, ring_as_of, run_recall_replay (moves from
                  // services/recall_replay.rs)
```

`rank.rs` (`rank_for_recall(&[RankInput], &RecallContext, best_cosine, extra_fired: &[&str]) -> Vec<RecallCandidate>`,
`rank_by_blend`, `reserve_background_slots`, `by_blended_after`) lives under `services/recall/` or `memory_scoring/`,
by the planner's choice.

`RecallCandidate { memory: Value, score: f64, used_embedding: bool, salience: f64, recall_adjustment:
Option<RecallAdjustment> }`. Keep `memory: Value`, as `SemanticSearchResult` does today, so the callers don't re-model.

**The recall record (`debugMemoryLogs`).** In `services/recall/deliver.rs`:

```rust
pub const RECALL_DEBUG_PREFIX: &str = "[Recall]";
pub const RECALL_RING_PREFIX: &str = "[Recall:ring]";
pub const SURVIVAL_WORD_SHARE: f64 = 0.6;
pub struct RecallDelivery {
    /// Insertion order = archive, head, interCharacter (v4's `Object.entries(bundle.lines)`).
    pub delivered: Vec<(RecallPool, Vec<String>)>,
    pub lost: Vec<String>,
    /// The lines for the assistant message's `debugMemoryLogs`.
    pub debug_log: Vec<String>,
}
pub struct DeliverOptions<'a> { pub character_id: &'a str, pub chat_id: &'a str,
    pub commonplace_recall_history: &'a Value, pub delivered_text: &'a str, pub persist_ring: bool }
pub async fn deliver_recall(db: &Db, bundle: &RecallBundle, opts: &DeliverOptions<'_>) -> RecallDelivery;
pub async fn stamp_accessed(db: &Db, character_id: &str, memory_ids: &[String]);
pub fn surviving_lines<'l>(lines: &'l [RenderedLine], delivered_text: &str, compressed: bool) -> Vec<&'l RenderedLine>;
pub fn trace_lines(trace: &[RecallTraceEntry]) -> Vec<String>;
pub fn ring_from_debug_log(lines: Option<&[String]>) -> Option<Vec<String>>;
pub fn with_recall_lines(existing: Option<&[String]>, lines: &[String]) -> Vec<String>;
```

`RecallTraceEntry { stage: RecallStage, label: Option<String>, detail: serde_json::Map<String, Value> }`, serialized
with `#[serde(skip_serializing_if = "Option::is_none")]` on `label`. The detail maps are built in v4's key order
(preserve_order), with numbers through `db::js_number_to_json`. `trace_lines` renders
`JSON.stringify(detail)` bytes via `serde_json::to_string` (memory note "serde_json Map == ignores key order").

`RecallStage` / `RecallPool` serialize as `"query"…"deliver"` and as
`"archive"|"head"|"interCharacter"|"recap"|"relevant"`.

**`BuiltContext`** (`build_context.rs:329`):
- `pub mail_alerts_owed: Option<MailAlertsOwed>` (`#[serde(rename = "mailAlertsOwed")]`, ALWAYS serialized; `None` →
  `null`), placed right after `inform_row_ids`;
- `#[serde(rename = "recallDebugLog", skip_serializing_if = "Option::is_none")] pub recall_debug_log:
  Option<Vec<String>>` right after `debug_relevant_conversations`.

**`BuildContextInput`:** `pre_searched_memories`, `recall_signals` and `pre_searched_query_embedding` →
`pub pre_recall: Option<PreRecall>`.

**Post Office** (`post_office/mailbox.rs`):
- `#[derive(Serialize)] #[serde(rename_all = "camelCase")] pub struct MailAlertsOwed { pub vault_id: String, pub paths: Vec<String> }`;
- `pub enum MailAlertsVia { Finalizer, PreservedPartial, ToolOnly, TurnSkip }` (`"finalizer"`, `"preserved-partial"`,
  `"tool-only"`, `"turn-skip"`);
- `pub async fn mark_mail_alerts_delivered(db: &Db, owed: Option<&MailAlertsOwed>, chat_id: &str, via: MailAlertsVia)`;
- `suparna_mail::resolve_suparna_mail_context_in_zone` → returns `(String, Vec<DeliveredLetterSummary>,
  Option<MailAlertsOwed>)` and flips nothing.

### 7.4 The recall-replay response growth (wire)

**`api/types.rs`: NO new variant.** `Request::ChatRecallReplay` (`:3041-3071`) is unchanged, and
`Response::RecallReplay(serde_json::Value)` (`:4565`) stays opaque.

The growth lives in the VALUE. I propose a typed serializer in `services/recall/replay.rs`:

```rust
#[derive(Serialize)] #[serde(rename_all = "camelCase")]
pub struct RecallReplayResult {
    chat_id, character_id, character_name, turn_index, total_turns,
    signals: Value, query: String, clock_iso: String,
    old_path: Vec<RecallReplayRow>, new_path: Vec<RecallReplayRow>,
    fallback_path: Vec<RecallReplayRow>,          // NEW — between newPath and oldHeadSize
    old_head_size, new_head_size, tuning: String, signals_pinned: bool,
    as_of: Option<String>, excluded_after_as_of: usize,
    ring: Vec<String>,                            // NEW
    ring_source: RingSource,                      // NEW: "debug-log" | "chat"
    pools: RecallPools,                           // NEW: { archive, head, interCharacter, recap }
    trace: Vec<RecallTraceEntry>,                 // NEW
}
pub struct RecallReplayRow { memory_id, summary, kind, occurred_at: Option<String>, narrative_time: Option<String>,
    created_at, keywords: Vec<String>, cosine: f64, salience: Option<f64> /* was rawWeight */, blended_before:
    Option<f64>, multiplier: Option<f64>, fired: Vec<String>, blended_after: Option<f64>, selected: bool }
```

Numbers go through `js_number_to_json`. The CLI reads the same keys.

---

## 8. SPA, help, docs and CLI touch points

- **SPA: none.** `debugMemoryLogs` is typed but never rendered in v4 (`app/salon/[id]/types.ts:19`). v5's SPA reads
  neither `debugMemories` nor `effectiveWeight` (grep empty).
- **Help** (the help lane re-vendors):
  - `help/memory-recall-relevance.md` gains `## One Question, Asked Once` and `## What Reaches the Character, and What
    Is Merely Noted` (`584d28164`);
  - `help/episodic-memory.md` (the replay sentence);
  - `help/post-office.md` (`69d38dd60`: "whether the character is answering you or simply carrying on unprompted", and
    the failed-turn re-announcement);
  - `help/chat-participants.md`, `memory-regenerate.md`, `system-tools.md` (`124c8aca3`, one line each).
- **`docs/v4/`:** `SYSTEM_FLOWCHARTS.md`, `API.md`, `features/complete/memory-system-refactor.md`, bugs 199/214/224/227.
- **CLI:** `quilltap recall-replay` (§1.12; v5 `quilltap-cli/src/recall_replay_cmd.rs`, Tier R). The `--instance`
  flags may land with the bug-239 rider in the CLI lane.

---

## 9. Open points for the order (observations and questions, not rulings)

1. **Size and splitting.** About 3,300 v4 lines in scope, plus five large v5 rewrite sites. A natural three-way split,
   file-disjoint except `build_context.rs`:
   - **(A) the recall engine**: `services/recall/{types,query,gather,hydrate,score,select,search}` + `rank`, the probe
     family, the semantic-search re-aim, and retiring `memory_service.rs`'s search half.
   - **(B) assemble/render/deliver + the context build**: `assemble`, `render`, `deliver`, the recap/archive/subject
     edits, `build_context.rs` collapse + ONE trailing list + `BuiltContext` fields, the finalizer record, the pre-compute
     `PreRecall`, the consumers (Carina, voice, greeting, search tool, self_inventory).
   - **(C) instruments**: replay + CLI.

   **The bug-227 mail deferral** (`suparna_mail.rs`, `mailbox.rs`, finalizer/primary-stream/orchestrator marks) is
   self-contained and could be a small lane or ride B.

   B depends on A's `RecallCandidate`/`RecallQuery`/`search_recall`. A frozen-contract handoff (the §7.3 spellings) lets
   them run in parallel.
2. **The probe fixture's ids are not UUIDs.** That needs a ruling:
   - (a) run v5's search stages over a `RecallReads` trait (index search/shape, `find_by_ids`, `search_by_content`) with
     a DB impl and an in-memory JSON impl; or
   - (b) remap the ids to UUIDs in a v5 copy of the fixture and plant it through a real fixture DB (the
     `build-episodic-recall` pattern).

   Option (a) also gives the probe family tier-1 speed. Should v5 vendor `probe-set.json` + `harness.ts` (18k lines),
   or read them from the pinned v4 worktree at run time?
3. **Token estimation by provider.** v4's renderers take `provider` (`estimateTokens(text, provider)`: GOOGLE 3.8,
   default 3.5). v5's formatters use `DEFAULT_CHARS_PER_TOKEN` only (`memory_injector.rs:47-52`), while `build_context.rs`
   already threads `cpt` (`:1866`). This is a pre-existing, unrecorded divergence for Google/Gemini profiles. Fix it in
   the new `render` (a `chars_per_token` parameter) or record it?
4. **Phase-2 memory compression is still deferred** (`build_context.rs:3057`). `survivingLines`' word-overlap arm and
   deliver's `compressed` flags are dead in production until it lands. Port and pin them tier-1 now (§3.17), or treat
   them as a loud tier-3 deferral? This survey recommends porting, because they are pure.
5. **The swipe path stores no recall record** (v4 at the pin), yet it stamps and writes the ring. `ringAsOf` then sees
   no record for that turn. Port it faithfully; consider filing it upstream as a v4 observation.
6. **Fold the recall assertions into `build_context_tier3`, or add a separate `recall_assemble_tier3`?** The existing
   family's 70-op corpus and its `getOrComputeFrozenArchive → []` mock (the archive is untested there) suggest a
   dedicated tier-3 for assemble and deliver, plus keeping `build_context_tier3` for the integration bytes.
7. **`humanKeywords` drops genuine keywords** equal to a context or temporal value (`history`, `present`, `past`…;
   §0.1.4). Port it as-is; possibly a v4 filing.
8. **The proactive pass's signals are discarded on an empty pool** (§1.9 step 1), costing a second interactive distil.
   That is faithful v4 behaviour; note it, don't fix it.
9. **`[ContextManager] Recall assembled` carries `durationMs`.** Any capture compare must mask it, as with the other
   timing fields.
10. **Ledger hygiene.** Record the corrections in §0.1 and the dirty v4 checkout when the next `/driftcheck` runs.
11. **The `GET /memories/[id]` stamp (bug 219)** is the memories-API lane's. This lane's "one writer" claim depends on
    it landing in the same round. A census guard (every `update_access_time*` caller is `stamp_accessed`) would pin
    both.
12. **The anchor probe (P4.D269) has no recall dependence at the pin.** It moves only with the index/admission/TF-IDF
    lanes, so leave it out of this lane.
13. **Dependency order inside the round.** This lane cannot be green before the scoring lane's `salience` and the index
    lane's `memory_index::{search, shape}`. Its tier-3 regen also needs the keystone's D23 re-dump: the vector tables
    gone and `memories.embedding` as the only vector home.

    If those lanes run in parallel, A needs stand-ins with the frozen signatures in §7.1 (the keystone-swarm shim rule:
    shims keep the old failure contract).
