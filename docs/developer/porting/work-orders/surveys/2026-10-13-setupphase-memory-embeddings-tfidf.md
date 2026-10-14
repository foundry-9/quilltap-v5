# Survey — EMBEDDINGS, the BUILTIN TF-IDF provider, and the VECTOR consumers outside memory logic

**Date:** 2026-10-13. **v4 pin:** `64743ee13` (`4.10.0-dev.175`). **Oracle baseline (what v5 ported):**
`4a38222e6`. **v5 `main`:** `a7c6f0d03`. Read-only survey for the `64743ee13` memory-refactor + scenarios
catch-up round. Every v4 line number is AT THE PIN unless marked `(pre)` (= `4a38222e6`). Every claim is
derived from shipped hunks (drift ledger §5.3), not commit prose.

**Slice:** `4ae0c4ca6` whole (Built-in TF-IDF tuning + bug 243); the embedding / vector half of Phase B
(`af48f6cc2`: vectors live only on `memories.embedding`, the EMBEDDING_GENERATE / reindex / refit / reapply
jobs, the dimension reconcile, the character optimizer's vector source, the anchor probe); the embedding text
per provider kind; the embedding service + provider seam; the `embeddings` route's `batchSize`; and the
harness families for all of it. **Not this slice** (named only as meeting points): the memories repository
itself and the `retire-memory-vector-entries-v1` boot ensure (the schema keystone), admission / the
`decideAdmission` decider (Phase C lane), recall gather/score (Phase E lane), consolidation + maintenance
(Phase F lane), retire / cascade / archive deletes (Phase B retire lane), the backup / restore / `.qtap`
carriers (carriers lane), the SPA maintenance card (SPA lane), the memory-calendar zone (`refreshMemoryTimeZone`,
the time lane).

---

## 0. The span for this slice, in one paragraph

Eight of the nineteen commits touch this slice. **`c2795733f`** (bug 204) made `EMBEDDING_GENERATE` and the
TF-IDF refit corpus embed the canonical ANCHORED text (`embeddingTextFor(memory, zone)`) instead of the plain
`summary\n\ncontent` concat. **`e0ac7493a`** moved the time helpers (`refreshMemoryTimeZone`) the jobs call and
re-ranked the character optimizer (`sortBySalience`, Phase A). **`af48f6cc2`** (Phase B) retired the
`vector_entries` / `vector_indices` mirror: `lib/embedding/vector-store.ts` became a pure in-process CACHE loaded
from `SELECT id, embedding, tier FROM memories …` (multi-length, no persistence, no `save()`), a new
`lib/memory/index-store/` (`memoryIndex` + `embedding-text.ts` + `backfill.ts`) is the only writer of
`memories.embedding`/`tier`, `VectorIndicesRepository` and `vector-indices.types.ts` were DELETED, the
EMBEDDING_GENERATE memory branch writes the row only, the `all` reindex stops deleting stores, reapply drops
`vector_entries` from its table walk, the dimension reconcile loses its two vector arms + two result keys, and
`?action=embeddings` loses `batchSize`. **`8af764514`** (Phase C) re-pointed the anchor probe at admission's
own decider. **`93c5af6ad`** (bug 235) put the vector-store manager / interactive counter / refit debounce map
on `processSingleton` (Next.js bundle copies — NO-PORT) and (bug 239) gave `quilltap anchor-probe` the lock-file
port resolver. **`584d28164`** (Phase E) moved `generateMissingEmbeddings` / `rebuildVectorIndex` out of the
deleted `memory-service.ts` into `index-store/backfill.ts`. **`4ae0c4ca6`** — the slice's core — added
`lexicalEmbeddingTextFor` (BUILTIN embeds drop the anchor line, passed by every memory writer as
`lexicalText`), `isLexicalEmbeddingProvider` (`lib/embedding/cosine-scale.ts`), `ADMISSION_THRESHOLDS_TFIDF`
(0.80/0.40/0.20) + `admissionThresholdsForProvider`, `tfidfClusterThreshold` (0.30) + `resolveClusterThreshold`,
the refit-when-due policy (`lib/embedding/builtin-refit-policy.ts`) with three new repository reads, a boot
PHASE 3.8 check and a maintenance-sweep step 10, the anchor probe's `lexicalProfile` + `note`, the NINTH D23
move (`tfidf_vocabularies.textShapeVersion`, migration `add-tfidf-text-shape-version-v1`), and bug 243 (the
forked job child's plugin-load race). **`b045dd986`** (Phase G) added `enqueueMissingEmbeddings` (the batch
backfill behind `?action=backfill-embeddings` / repair) and `backgroundJobs.findActiveByType`'s second caller
`enqueueEmbeddingGenerateMany`, and edited `help/embedding-profiles.md` once more. `104e986c7`, `48790fb73`,
`9ec4fa2e9`, `64743ee13` touch only comments / unrelated files here.

**Ledger corrections (§3 rows `4ae0c4ca6` / `af48f6cc2` and §1 "Schema / D23")** — measured, not assumed:

1. **generateDDL's `textShapeVersion` is `REAL DEFAULT 0`, not `INTEGER DEFAULT 0`.** v4's
   `mapToSQLiteType` (`lib/database/schema-translator.ts:317-337`) emits `INTEGER` only when BOTH `min` and `max`
   are integers; `getNumberConstraints` (`:151-176`) reads only `greater_than`/`less_than` checks, so
   `z.number().int().nonnegative().default(0)` has `min = 0`, no `max` → `REAL`. Precedent in v5's own committed
   dump: `runMilestonesAnnounced: z.number().int().nonnegative().default(0)` (`lib/schemas/chat.types.ts:1170`) is
   `"runMilestonesAnnounced" REAL DEFAULT 0` in `crates/quilltap-core/src/services/provisioning/fresh_schema.json`.
   So the two v4 shapes differ in TYPE as well as position: **fresh = `"textShapeVersion" REAL DEFAULT 0` between
   `"fittedAt"` and `"createdAt"`; migrated = `"textShapeVersion" INTEGER DEFAULT 0` appended LAST.** Confirm by
   the D23 re-dump (never by hand).
2. **The refit corpus text needs NO change in v5.** v5's `embedding_refit_job.rs:116-126` builds
   `format!("{summary}\n\n{content}")`. `c2795733f` moved v4 to the anchored text, and `4ae0c4ca6` moved it BACK
   to `lexicalEmbeddingTextFor` = `summary\n\ncontent`. Net span delta on the corpus TEXT: zero. The deltas are
   the corpus ENUMERATION, the `textShapeVersion` stamp, and the log lines (§3 items 8-11).
3. **v5's `EMBEDDING_GENERATE` MEMORY text is right for BUILTIN and wrong for neural.** v5
   (`embedding_generate_job.rs:29-33`, `:322-326`) embeds the plain concat with a header claiming "v4's handler
   predates the episodic anchors and was never updated". `c2795733f` (bug 204) updated it: at the pin the handler
   embeds `embeddingTextFor(memory, zone)` (anchored) with `lexicalText: lexicalEmbeddingTextFor(memory)`
   (`embedding-generate.ts:151-169`). For a BUILTIN profile the net vector equals v5's; for any neural profile
   with an anchored memory it differs.
4. **The hazard "the optimizer … error[s]" (§1 hazard 2) is softer for the optimizer:** v5's
   `semantic_candidates` (`generators/optimizer.rs:1619-1662`) maps a `CharacterVectorStore::load` error to `Err`,
   which v4's catch arm turns into the text-search fallback WARN — a silent degrade, not a failure.
5. The `TfidfVocabularySchema` doc comment (`lib/schemas/embedding-job.types.ts:51-55`) cites a
   `BUILTIN_TEXT_SHAPE_VERSION` in `builtin-refit-policy` — no such symbol exists; the constant is
   `LEXICAL_TEXT_SHAPE_VERSION` in `lib/memory/index-store/embedding-text.ts:57` (re-exported by the policy,
   `builtin-refit-policy.ts:41`). A v4 doc nit; port the real name.

---

## 1. v4 AT THE PIN — every module in the slice

### 1.1 `lib/memory/index-store/embedding-text.ts` (77 lines) — the embedding text, per provider kind

```ts
export interface EmbeddingTextSource extends EpisodicAnchorView { summary: string; content: string }
export function embeddingTextFor(memory: EmbeddingTextSource, timeZone?: string): string        // :38-49
export const LEXICAL_TEXT_SHAPE_VERSION = 1                                                      // :57
export function lexicalEmbeddingTextFor(memory: Pick<EmbeddingTextSource,'summary'|'content'>): string // :75-77
```

- `lexicalEmbeddingTextFor` = `` `${memory.summary}\n\n${memory.content}` `` — built from the FIELDS, never by
  trimming the canonical text (content ending in `(place: …)` keeps its last line).
- `embeddingTextFor` = `lexicalEmbeddingTextFor(memory)` + (`\n` + `buildMemoryAnchorLine({occurredAt ?? null,
  narrativeTime ?? null, entities ?? null}, timeZone)` when non-empty). The two cannot drift (one builds on the
  other).
- `buildMemoryAnchorLine` (`lib/memory/episodic.ts:52-81`): `(when: YYYY-MM-DD · story time: … · place: A, B,
  C, D)`; parts joined by `' · '`; `when` is `zonedDateStamp(Date.parse(occurredAt), timeZone ??
  getMemoryTimeZone())` (bug 212 — the memory calendar zone; the time lane owns it), kept only if it matches
  `/^\d{4}-\d{2}-\d{2}$/`; entities trimmed, empties dropped, first 4; `''` when no part. `ANCHOR_LABELS =
  ['when','story time','place']` (`:34`) is a refactor of the same literals — no byte change.
- **What goes into the vector, per provider kind** (every MEMORY writer passes both texts — §1.3):

  | Provider | Vector input |
  |---|---|
  | `BUILTIN` (TF-IDF) | `lexicalText` = `summary\n\ncontent` |
  | every other provider (OPENAI, OLLAMA, OPENROUTER, NANOGPT, …) | `embeddingTextFor(m, zone)` = `summary\n\ncontent` + `\n(when: … · place: …)` when anchored |
  | non-memory texts (help sections, recall queries, chunks, the probe's anchor-free side) | the text as given, no `lexicalText`, both kinds |

### 1.2 `lib/embedding/cosine-scale.ts` (23 lines, NEW in `4ae0c4ca6`) — pure, client-safe

```ts
const LEXICAL_EMBEDDING_PROVIDERS: ReadonlySet<string> = new Set(['BUILTIN'])          // :18
export function isLexicalEmbeddingProvider(provider: string | null | undefined): boolean {
  return !!provider && LEXICAL_EMBEDDING_PROVIDERS.has(provider)                       // :21-23
}
```

Callers at the pin (the ONE scale predicate): `settings-defaults.ts:73` (admission thresholds) and `:88`
(cluster threshold), `scoring/relevance.ts:39` (recall floor), `recall/score.ts:75` (R1 boost gate off),
`builtin-refit-policy.ts:188`, `anchor-gate-probe.ts:297-298`. `embedding-service.ts:283` still dispatches on
`profile.provider === 'BUILTIN'` (provider routing, not scale) and so do `embedding-refit.ts:56`,
`embedding-job-scheduler.ts:71`, `quilltap-import/execute.ts:405`, the reconcile's skip — those are NOT
scale questions and stay literal.

### 1.3 `lib/embedding/embedding-service.ts` (661 lines) — the `lexicalText` seam

```ts
export interface EmbeddingCallOptions { priority?: EmbeddingPriority; lexicalText?: string }   // :93-102
async function generateBuiltinEmbedding(text, profile, lexicalText?): Promise<EmbeddingResult> // :199-264
export async function generateEmbedding(text, profile, userId, opts: Pick<EmbeddingCallOptions,'lexicalText'> = {}) // :275-295
export async function generateEmbeddingForUser(text, userId, profileId?, opts: EmbeddingCallOptions = {}) // :308-316
```

- `generateEmbedding`: `profile.provider === 'BUILTIN'` → `generateBuiltinEmbedding(text, profile,
  opts.lexicalText)`; every other provider → `generateApiEmbedding(text, profile, userId)` (lexicalText DROPPED —
  neural vectors byte-identical to pre-`4ae0c4ca6`).
- `generateBuiltinEmbedding` after `loadState`: when `lexicalText !== undefined`, DEBUG
  `[Embedding] BUILTIN embed: using the caller's lexical text` `{ context: 'generateBuiltinEmbedding', profileId,
  textChars: text.length, lexicalChars: lexicalText.length }` (JS `.length` = UTF-16 units); then
  `embeddingProvider.generateEmbedding(lexicalText ?? text)`. Not-fitted error unchanged: `Built-in embedding
  profile not yet fitted. Please wait for the vocabulary to be built from your memories.` (provider `BUILTIN`).
  The BUILTIN provider still comes from `providerRegistry.createEmbeddingProvider('BUILTIN')` (bug 243's race
  site — §1.12).
- `runEmbeddingForUser` (`:318-360`): profile resolution unchanged; forwards `{ lexicalText: opts.lexicalText }`
  only (priority stays local).
- `93c5af6ad` (bug 235): `interactive` counter on `processSingleton('embedding.interactivePending')` (`:105`) —
  NO-PORT (bundle-copy problem; v5 ignores the priority lane already, `embedding_provider.rs` module doc).
- `EMBEDDING_MAX_CHARS = 128 * 1024` (`:76`) unchanged.

**Every `lexicalText` caller at the pin** (all memory writers; grep `lexicalEmbeddingTextFor`):
`embedding-generate.ts:168`, `admission/admit.ts:145` (`embedRow` — `call` reused for the retry),
`admission/apply.ts:85` (`reembedRow`), `admission/neighbours.ts:144` (unindexed-neighbour re-embed),
`admission/update-memory.ts:143` (`updateMemory`, opts in the 4th position after `undefined` profileId),
`consolidation.ts:847` (digest embed), `index-store/backfill.ts:58` (`generateMissingEmbeddings`),
`anchor-gate-probe.ts:213` (anchored side only). The refit corpus uses `lexicalEmbeddingTextFor` directly
(`embedding-refit.ts:97`).

### 1.4 `lib/embedding/builtin-refit-policy.ts` (203 lines, NEW in `4ae0c4ca6`) — refit-when-due

Module logger: `logger.child({ module: 'embedding:builtin-refit-policy' })` (`:43`).

```ts
export const REFIT_MIN_NEW_MEMORIES = 25                                         // :46
export const REFIT_DRIFT_FRACTION = 0.1                                          // :49
export type BuiltinRefitReason = 'unfitted' | 'text-shape' | 'drift'             // :51
export interface RefitNeedInput { fittedAt: string|null; textShapeVersion?: number|null; newSinceFit: number; totalMemories: number }
export function driftThreshold(totalMemories: number): number                    // :65-67  max(25, Math.ceil(0.1 * total))
export function evaluateRefitNeed(input: RefitNeedInput): BuiltinRefitReason | null // :70-75
export interface RefitAssessment extends RefitNeedInput { reason: BuiltinRefitReason | null }
export async function assessBuiltinRefit(profile: Pick<EmbeddingProfile,'id'>): Promise<RefitAssessment> // :82-97
export interface RefitInHand { type: 'EMBEDDING_REFIT'|'EMBEDDING_REINDEX_ALL'; status: 'PENDING'|'PROCESSING' }
export async function refitInHand(): Promise<RefitInHand | null>                 // :109-117
export interface RefitDecision { reason: BuiltinRefitReason|'forced'|null; enqueued: boolean; heldBy?: 'nothing-to-fit'|RefitInHand }
export async function enqueueRefitIfDue(profile: Pick<EmbeddingProfile,'id'|'userId'>, opts: { force?: boolean; source: string }): Promise<RefitDecision> // :138-172
export interface BuiltinRefitCheck { profileId: string|null; reason: BuiltinRefitReason|null; enqueued: boolean }
export async function checkBuiltinRefit(): Promise<BuiltinRefitCheck>            // :185-203
```

**`evaluateRefitNeed` (pure, ordered):** `!fittedAt` (null OR `''`) → `'unfitted'`; `(textShapeVersion ?? 0) <
LEXICAL_TEXT_SHAPE_VERSION` (1) → `'text-shape'`; `newSinceFit >= driftThreshold(totalMemories)` → `'drift'`;
else `null`. `Math.ceil(0.1 * t)` agrees with integer `ceil(t/10)` for every `t` in 0..200,000 (measured);
port it as the f64 expression anyway.

**`assessBuiltinRefit`:** `tfidfVocabularies.findByProfileId(profile.id)`; `fittedAt = vocabulary?.fittedAt ??
null`; `textShapeVersion = vocabulary?.textShapeVersion ?? 0`; `{ total, createdSince } =
memories.countCorpusSince(fittedAt)`; DEBUG `Assessed the TF-IDF vocabulary` `{ profileId, fittedAt,
textShapeVersion, newSinceFit, totalMemories, driftThreshold, reason }` (that key order: `profileId`, then
`...input` = `fittedAt, textShapeVersion, newSinceFit, totalMemories`, then `driftThreshold`, `reason`).

**`refitInHand`:** for `type` in `['EMBEDDING_REFIT', 'EMBEDDING_REINDEX_ALL']` (that order):
`backgroundJobs.findActiveByType(type)`; the first PENDING, else the first PROCESSING → `{ type, status }`; none
→ `null`.

**`enqueueRefitIfDue` (serialised process-wide):** `refitChain = processSingleton('embedding.builtinRefitChain',
…)`; `result = refitChain.tail.then(run, run)` — each call waits for the previous to settle (success or failure).
`run`:
1. `reason = force ? 'forced' : null`. If NOT forced: `assessment = assessBuiltinRefit(profile)`; `!reason` →
   `{ reason: null, enqueued: false }`; `totalMemories === 0` → DEBUG `TF-IDF refit due but there are no
   memories to fit` `{ profileId, reason, source }` → `{ reason, enqueued: false, heldBy: 'nothing-to-fit' }`.
   (A FORCED call skips both — it enqueues even on an empty corpus; the refit handler then logs "No memories
   found".)
2. `inHand = refitInHand()`; `inHand && (!force || inHand.status === 'PENDING')` → INFO `TF-IDF refit due but one
   is already in hand; not enqueueing another` `{ profileId, reason, inHand: { type, status }, source }` →
   `{ reason, enqueued: false, heldBy: inHand }`. (A forced call is held only by a PENDING job; a PROCESSING one
   may already have read the corpus.)
3. `enqueueEmbeddingRefit(profile.userId, { profileId: profile.id, triggerReindex: true })`; INFO `TF-IDF
   vocabulary refit enqueued` `{ profileId, reason, source }` → `{ reason, enqueued: true }`.

`source` values at the pin: `'check'` (from `checkBuiltinRefit`) and `'scheduleRefit'`.

**`checkBuiltinRefit` (never throws; boot + maintenance):** `embeddingProfiles.findInstanceDefault()`; `!profile
|| !isLexicalEmbeddingProvider(profile.provider)` → `{ profileId: profile?.id ?? null, reason: null, enqueued:
false }`; else `enqueueRefitIfDue(profile, { source: 'check' })` → `{ profileId, reason: decision.reason ===
'forced' ? null : decision.reason, enqueued }`. Catch → WARN `TF-IDF refit check failed` `{ error }` → `{
profileId: null, reason: null, enqueued: false }`.

### 1.5 `lib/embedding/embedding-job-scheduler.ts` (137 lines)

- `scheduleRefit(userId, profileId?, options: ScheduleRefitOptions = {})` (`:51-122`): profile =
  `findById(profileId)` or `findDefault(userId)`; none or `provider !== 'BUILTIN'` → return silently. Debounce key
  `${userId}:${profile.id}`; `options.force` → `forcedRefits.add(key)` (a `processSingleton('embedding.refitForced')`
  Set — a forced call keeps the collapsed check forced); clear any timer; DEBUG `[EmbeddingScheduler] Refit check
  scheduled (debounced)` `{ context: 'scheduleRefit', userId, profileId, forced }`; `setTimeout(5000)`:
  `forced = forcedRefits.delete(key)`; `decision = enqueueRefitIfDue(profile, { force: forced, source:
  'scheduleRefit' })`; `!decision.reason` → INFO `[EmbeddingScheduler] Refit not needed; the vocabulary is current`
  `{ context, userId, profileId }`; `decision.enqueued` → INFO `[EmbeddingScheduler] Refit job scheduled
  (debounced)` `{ context, userId, profileId, reason }` (reason may be `'forced'`); a due-but-held decision logs
  nothing here (the policy already did); catch → ERROR `[EmbeddingScheduler] Failed to schedule refit job` `{
  context, userId, profileId, error }`.
- `handleEntityDeletion(entityType, entityId)` (`:130-137`) — unchanged (`embeddingStatus.deleteByEntity`).
- Callers: `memories/route.ts:629` (manual create, `scheduleRefit(user.id)`), `memories/[id]/route.ts:133`
  (PUT) and `:177` (DELETE), `quilltap-import/execute.ts:408` (`{ force: true }`). The profile routes and the
  system tool's direct `enqueueEmbeddingRefit` are explicit requests and untouched (spec §6).

### 1.6 `lib/background-jobs/handlers/embedding-refit.ts` (157 lines)

Same structure as `(pre)`; deltas:
- Corpus enumeration: `characterIds = memories.findDistinctCharacterIds()` (`SELECT DISTINCT characterId FROM
  memories WHERE characterId IS NOT NULL`, empties filtered); for each, `memories.findByCharacterId(id)` pushed in
  order (`:70-75`). The `(pre)` `characters.findByUserId` + INFO `[EmbeddingRefit] No characters found, skipping
  refit` branch is GONE. **Order matters:** the fit's vocabulary indices follow document order.
- NEW DEBUG `[EmbeddingRefit] Corpus gathered` `{ context: 'handleEmbeddingRefit', jobId, characters, memories }`
  (`:76-81`).
- Documents: `allMemories.map(m => lexicalEmbeddingTextFor(m))` then help docs `` `${doc.title}\n\n${doc.content}` ``
  (unchanged).
- `upsertByProfileId(profileId, { …, fittedAt: state.fittedAt, textShapeVersion: LEXICAL_TEXT_SHAPE_VERSION })`
  (`:126-136`) — the ONLY writer of a non-zero `textShapeVersion`.
- Full log set (v5 emits NONE of these today): INFO `[EmbeddingRefit] Starting vocabulary refit` `{ context,
  jobId, profileId }`; throw `Embedding profile not found: ${profileId}`; WARN `[EmbeddingRefit] Profile is not
  BUILTIN, skipping refit` `{ context, jobId, profileId, provider }`; DEBUG `Corpus gathered`; INFO
  `[EmbeddingRefit] No memories found, skipping refit` `{ context, jobId, profileId }`; WARN `[EmbeddingRefit]
  Failed to load help docs for corpus, continuing without them` `{ context, error }`; throw `Failed to get TF-IDF
  state after fitting`; INFO `[EmbeddingRefit] Vocabulary fitted and saved` `{ context, jobId, profileId,
  vocabularySize, documentCount, avgDocLength: state.avgDocLength.toFixed(2) }` (a STRING); INFO
  `[EmbeddingRefit] Enqueued reindex job` `{ context, jobId, profileId }`. `getTfIdfVectorizer`'s ERROR
  `[EmbeddingRefit] Failed to load TF-IDF vectorizer` / throw `Built-in embedding plugin not available` is the
  dynamic plugin import — NO-PORT (v5 links `crate::tfidf`).

### 1.7 `lib/background-jobs/handlers/embedding-generate.ts` (641 lines) — MEMORY branch `:125-224`

1. `memories.findById(entityId)`; missing → WARN `[EmbeddingGenerate] Memory not found` + `markAsFailed(…, 'Memory
   not found', userId)` (unchanged).
2. `memoryTimeZone = await refreshMemoryTimeZone()`; `textToEmbed = embeddingTextFor(memory, memoryTimeZone)`
   (`c2795733f`); NEW DEBUG `[EmbeddingGenerate] Built memory embedding text` `{ context:
   'handleEmbeddingGenerate', memoryId, textLength: textToEmbed.length }`.
3. `skipIfOversize(textToEmbed, …)` — the guard reads the ANCHORED text (empty → `Empty input — nothing to
   embed`; > `EMBEDDING_MAX_CHARS` → `Oversize: N chars exceeds 131072-char cap`), unchanged messages.
4. `generateEmbeddingForUser(textToEmbed, job.userId, payload.profileId, { priority: 'background', lexicalText:
   lexicalEmbeddingTextFor(memory) })`.
5. `memoryIndex.upsert(memory.characterId, [{ id, embedding, tier: memory.tier === 'cold' ? 'cold' : 'hot' }])` —
   writes `memories.embedding` ONLY (`updateForCharacter(characterId, id, { embedding })`; the tier rides only into
   the cache). The `(pre)` `vectorRepo.entryExists/updateEntryEmbedding/addEntry/saveMeta` + `unloadStore` are GONE.
6. `markAsEmbedded`, INFO `[EmbeddingGenerate] Embedding generated successfully` `{ context, jobId, memoryId,
   characterId, dimensions }`; catch arm + `isPermanentEmbeddingError` unchanged.

The CONVERSATION_CHUNK / HELP_DOC / MOUNT_CHUNK branches are UNCHANGED in the span (no `lexicalText` — they are
not memories).

### 1.8 `lib/background-jobs/handlers/embedding-reindex.ts` (335 lines)

Only Phase B's hunk: the `scope === 'all'` per-character `vectorStoreManager.deleteStore(characterId)` loop
(`(pre) :240-251`) is GONE, replaced by a comment (`:240-244`): "Nothing to clear: the memory index is the rows
themselves … A search mid-reindex scores the vectors that match the query's length and skips the rest." It
imports `memoryIndex` (`:34`) and never calls it. Everything else (cancel + `markAllPendingByProfileId` in `all`
scope, the four phases, `BATCH_SIZE = 200`, the summary INFO) unchanged.

### 1.9 `lib/embedding/reapply-profile.ts` (433 lines)

`MAIN_DB_TABLES = ['memories', 'conversation_chunks', 'help_docs', 'help_doc_chunks']` (`:48`, was 5 with
`vector_entries`); both walks `if (!tableExists(main, table)) continue` (`:234-235`, `:343-344`); the post-pass
cache drop is `memoryIndex.reloadAll()` with WARN text `[ReapplyProfile] Failed to drop the memory index caches`
(was `… Failed to unload vector stores`) `{ error }` (`:404-410`).

### 1.10 `lib/startup/reconcile-embedding-dimensions.ts` (327 lines)

`EmbeddingDimensionReconcileResult` loses `vectorEntriesDeleted` and `vectorIndexMetaFixed` (and `EMPTY_RESULT`
with them); the two vector arms (`(pre) :178-201` — `DELETE FROM "vector_entries" WHERE …`, `UPDATE
"vector_indices" SET dimensions = ?`, the `unloadAll`) are GONE; the INFO `Embedding dimension reconcile found
non-conforming vectors` now fires only on `totalMismatched > 0` with `{ profileId, targetDimensions,
...mismatched, reindexEnqueued }` (the two vector keys removed). `restore.ts`'s DEBUG `Post-restore embedding
reconcile complete` drops the same two keys.

### 1.11 `lib/embedding/vector-store.ts` (494 lines) + `lib/memory/index-store/memory-index.ts` (259 lines) — the vector SOURCE

**vector-store (Phase B rewrite; "a cache, never a copy"):**
- `CharacterVectorStore.load()` (`:115-147`): clears; `memories.findIndexRowsByCharacterId(characterId)` (the
  keystone's read: `SELECT id, embedding, tier FROM memories WHERE characterId = ? AND embedding IS NOT NULL`,
  undecodable / empty rows skipped with WARN `Skipped memory embeddings that could not be decoded {characterId,
  undecodable}`, DEBUG `Read memory index rows {characterId, rows}`, safeQuery fallback `[]` + ERROR `Error reading
  memory index rows`); each row `put` with `metadata { memoryId, characterId, tier: row.tier }` and `createdAt =
  loadedAt`; DEBUG `Loaded vector index from memory rows` `{ context: 'CharacterVectorStore.load', characterId,
  entries, cold, dimensions }` (`cold` = count of cold ROWS loaded — `(pre)` it was the cold-ID set size); catch →
  ERROR `Error loading vector index from memory rows` `{ context, characterId, error }` and an EMPTY store (never
  throws). The `(pre)` WARN `Could not read cold memory ids; treating every vector as hot` and `loadColdIds` are GONE.
- **Multi-length:** entries of ANY length are held; `lengthCounts: Map<len, count>`; `getDimensions()` = the MOST
  COMMON length (ties → first-inserted length in Map order; `null` when empty); `countWithDimensions(d)`.
  `addVector` never throws on a mismatch (the `(pre)` `Vector dimension mismatch: expected …` throws are gone);
  `updateVector`, `save()`, the dirty sets, `deleteStore`, `saveAll`, `saveStore` are DELETED.
- `search(q, limit = 10, filter?)` (`:222-252`): empty → `[]`; `countWithDimensions(q.length) === 0` → WARN `Query
  vector dimension mismatch — search embedding profile likely differs from index` `{ context:
  'CharacterVectorStore.search', characterId, storedDimensions: getDimensions(), queryDimensions }` → `[]`; linear
  path when `size < 1000 || limit * 4 >= size`, else the heap path. Per entry: filter first, then the length skip,
  then cosine; linear = stable sort desc + slice.
- `tierOf(id)` NEW; `setTier` unchanged (DEBUG `Re-stamped vector tiers`); `getAllEntries`, `clear`.
- `VectorStoreManager` (`:403-470`): memoised loads (`loading` map), `getLoadedStore` (never starts a load),
  `unloadStore`, `unloadAll`; `getVectorStoreManager()` on `processSingleton` (bug 235).

**memory-index (`memoryIndex`, the only importer of the cache; logger child `memory:index-store`):**
`search(cid, vec, { limit, hotOnly, exclude? })` (filter = exclude-set + `isHotVector` when `hotOnly`);
`shape(cid, qDims) → { size, dominantDimensions, matching }`; `entries(cid)`; `upsert(cid, entries[])` (row write
`updateForCharacter(cid, id, { ...alongside, embedding })` per entry, then cache when loaded; DEBUG `Upserted
memory vectors {characterId, count, withFields, cached}`; a `null` row → DEBUG `Upsert found no row; vector not
cached`); `indexInserted`; `setTier` (`memories.updateTierBulk` + cache; DEBUG `Moved memories between tiers
through the index`); `remove` (DEBUG `Evicted retired memories from the index`); `reload` (DEBUG `Dropped memory
index cache {characterId, dropped}`); `reloadAll` (DEBUG `Dropped every memory index cache`). Cache-mutation
failures → WARN `Memory index cache update failed; dropped the cache so the next search reloads it {characterId,
what, error}`. **The keystone / index lane owns this module**; this slice consumes `upsert`, `search`, `shape`,
`entries`, `reload`, `reloadAll`.

**Every `memoryIndex` call site at the pin outside `lib/memory/`:** `embedding-generate.ts:174` (upsert),
`character-optimizer.service.ts:737` (search `{ limit: 500, hotOnly: true }`), `reapply-profile.ts:405`
(reloadAll), `restore.ts:371` (reloadAll), `cascade-delete.ts:387` / `archive-service.ts:770` (reload — the retire
lane), `child-entry.ts:230,244` + `job-dispatcher.ts:868` (child/parent cache plumbing — NO-PORT).

### 1.12 `lib/background-jobs/child/plugin-readiness.ts` (77 lines, NEW) + `child-entry.ts` — bug 243

`PLUGIN_READY_TIMEOUT_MS = 60_000`; `startPluginInitialization(init)` (idempotent, never rejects);
`whenPluginsReady(timeoutMs)` races the settle against a timer, latches `done = true` on timeout; DEBUG `Job
waiting for the plugin registry to load` / `Plugin registry loaded; running the job`; WARN `Plugin registry still
loading; running the job anyway, and not waiting for it again` `{ timeoutMs }` (logger child
`jobs:child:plugin-readiness`). `child-entry.ts:176` `await whenPluginsReady()` before every handler. Root cause
(bug-243 doc): the forked child's `providerRegistry` was empty for its first jobs, so
`generateBuiltinEmbedding → providerRegistry.createEmbeddingProvider('BUILTIN')` threw `Provider 'BUILTIN' not
found in registry`, and BUILTIN instances' boot help-doc sections were skipped. **NO-PORT evidence (v5 cannot
have the race):**
- `crates/quilltap-core/src/services/job_runner.rs:5-20` — v5 runs handlers IN-PROCESS; no child, no fork, no
  async plugin load ("In Rust it dissolves").
- `services/embedding_provider.rs:221-227` — BUILTIN dispatches to `generate_builtin_embedding` BEFORE any registry
  lookup; `services/builtin_embedding.rs:63` constructs `crate::tfidf::BuiltinEmbeddingProvider::new(true)`
  directly (no registry).
- `provider_manifest/mod.rs:526-529` — `Registry::built_in() -> &'static Registry` over a static
  `BUILT_IN_REGISTRY`; the only `Provider '{name}' not found in registry` site
  (`embedding_provider.rs:253`) is reachable only for unknown non-BUILTIN names.
- `services/embedding_refit_job.rs:139` — `TfIdfVectorizer::new(true)` from `crate::tfidf`, no dynamic plugin import.
- Probe to RATIFY (measure, don't assume): first boot of a fresh BUILTIN instance — every help doc gets a vector and
  no `not found in registry` line appears.

### 1.13 `migrations/scripts/add-tfidf-text-shape-version-v1.ts` (73 lines) — the NINTH D23 move

`id 'add-tfidf-text-shape-version-v1'`, `introducedInVersion '4.10.0'`, `dependsOn ['sqlite-initial-schema-v1',
'create-embedding-tables-v1']`, registered second-to-last at the pin (`migrations/scripts/index.ts:900-901`:
after `dropInterCharacterMemoryJobsMigration`, before `104e986c7`'s `dropHousekeepingConsolidationJobsMigration`). `shouldRun`: SQLite ∧ table `tfidf_vocabularies` exists ∧ column missing.
`run`: `addColumnIfMissing('tfidf_vocabularies', 'textShapeVersion', 'INTEGER DEFAULT 0')` (DEBUG `Adding column
{context: 'migrations.database-utils', table, column}` + `ALTER TABLE "tfidf_vocabularies" ADD COLUMN
"textShapeVersion" INTEGER DEFAULT 0`); INFO `Added textShapeVersion column to tfidf_vocabularies` `{ context:
'migration.add-tfidf-text-shape-version-v1', durationMs }`; result message identical; catch → ERROR `Failed to add
textShapeVersion column` `{ context, error }`, message `Failed to add textShapeVersion column`. **No backfill** —
existing rows read 0 ("fitted with anchor lines, refit once"). `prettify.ts:168` label `Noting which edition of the
word-ledger each index was drawn from…`. Schema field: `textShapeVersion: z.number().int().nonnegative().default(0)`
between `fittedAt` and `createdAt` (`embedding-job.types.ts:56`) → generateDDL `REAL DEFAULT 0` (§0 correction 1).
Repository `create` validates → writes `0` explicitly when the caller omits it; `update` writes it only when passed.

### 1.14 The new repository reads (`4ae0c4ca6`)

| v4 | SQL shape | fallback (safeQuery) |
|---|---|---|
| `backgroundJobs.findActiveByType(type)` (`background-jobs.repository.ts:98-115`) | `findByFilter({ type, status: { $in: ['PENDING','PROCESSING'] } }, { sort: { createdAt: 1 } })` | `[]`, ERROR `Error finding active background jobs by type {type}` |
| `embeddingProfiles.findInstanceDefault()` (`embedding-profiles.repository.ts:75-82`) | `findOneByFilter({ isDefault: true })` — ANY user, first match | `null`, ERROR `Error finding the instance default embedding profile` |
| `memories.countCorpusSince(since)` (`memories.repository.ts:980-996`) | `count({})` and, when `since`, `count({ createdAt: { $gte: since } })` (string compare) — else 0 | `{ total: 0, createdSince: 0 }`, ERROR `Error counting the memory corpus {since}` |

`findActiveByType('EMBEDDING_GENERATE')` has a second caller from Phase G: `queue-service.ts:1139`
`enqueueEmbeddingGenerateMany` (§1.18).

### 1.15 Boot and the maintenance sweep

- `instrumentation.ts:995-1013` — PHASE 3.8, right after the 3.7 dimension reconcile: dynamic import of
  `./lib/embedding/builtin-refit-policy`; `void checkBuiltinRefit()` (fire-and-forget); import failure → WARN
  `Failed to import the TF-IDF refit check` `{ context: 'instrumentation.register', error }` (unreachable in v5).
- `lib/background-jobs/scheduled-maintenance.ts:292-297` — step 10, after the two fold catch-ups:
  `runSweep(summary, 'builtin-refit', 'TF-IDF refit check failed — continuing', () => checkBuiltinRefit(), r =>
  summary.builtinRefit = { reason: r.reason, enqueued: r.enqueued })`; `MaintenanceSweepSummary.builtinRefit: {
  reason: string|null; enqueued: boolean }` (initialised `{ reason: null, enqueued: false }`, placed after
  `foldEpisodeCatchup`, before `failures`). `checkBuiltinRefit` never throws, so the `builtin-refit` failure key is
  unreachable. The end INFO `Scheduled maintenance pass complete` carries the whole summary (so `builtinRefit`).

### 1.16 The thresholds and the cluster resolver — `lib/memory/settings-defaults.ts` (103 lines)

Definitions only (admission / maintenance / consolidation USE them — sibling lanes):

```ts
export interface AdmissionThresholds { nearDuplicate; reinforce; related; dateGuardDays; footnoteCap }  // :27-37
export const ADMISSION_THRESHOLDS = { nearDuplicate: 0.90, reinforce: 0.85, related: 0.70, dateGuardDays: 7, footnoteCap: MAX_REINFORCEMENT_FOOTNOTES /* 8 */ } // :45-51
export const ADMISSION_THRESHOLDS_TFIDF = { ...ADMISSION_THRESHOLDS, nearDuplicate: 0.80, reinforce: 0.40, related: 0.20 } // :61-66
export function admissionThresholdsForProvider(provider): AdmissionThresholds   // :72-74
export function resolveClusterThreshold(settings: {clusterThreshold, tfidfClusterThreshold}, provider, override?: number): number // :82-89
export const MEMORY_SETTING_DEFAULTS = { maintenance, consolidation, recall, extractionMode, extractionLimits, admission, admissionTfidf } // :92-100
```

`resolveClusterThreshold`: `typeof override === 'number'` → override; else lexical → `tfidfClusterThreshold`,
else `clusterThreshold`. `MemoryConsolidationSettingsSchema.tfidfClusterThreshold: z.number().min(0).max(1)
.default(0.30)` sits right after `clusterThreshold` (`settings.types.ts:263`). Users at the pin:
`admit.ts:356,437` (+ DEBUG field `thresholdScale: thresholds === ADMISSION_THRESHOLDS ? 'neural' : 'tfidf'`),
`maintenance/run.ts:276` (fold-near-duplicates), `anchor-gate-probe.ts:121,145`, `consolidation.ts:617`
(`settings.clusterThreshold` OVERWRITTEN with the resolved value before the DEBUG `Consolidation starting {…,
clusterThreshold, embeddingProvider, …}`; the provider is `embeddingProfiles.findDefault(userId)?.provider`, a read
failure → DEBUG `Could not read the default embedding profile; clustering on the neural scale` and neural).
`resolveMemorySettings` gains `admissionTfidf` (`settings.ts:118`). None of these reach a wire.

### 1.17 `lib/memory/anchor-gate-probe.ts` (314 lines) — the probe

Logger `createServiceLogger('AnchorGateProbe')`; `PROBE_TOP_K = 5`; `limit = min(200, max(1, input.limit ?? 50))`.
Pin flow (Phase B + C + `4ae0c4ca6`):
- `thresholds = admissionThresholdsForProvider(null)` (neural), `provider = null`; `memoryTimeZone = await
  refreshMemoryTimeZone()`.
- `embed(text, lexicalText?)` counts first (`embeddingsGenerated++`), calls `generateEmbeddingForUser(text, userId,
  embeddingProfileId, { priority: 'background', ...(lexicalText !== undefined ? { lexicalText } : {}) })`; on the
  FIRST result: `provider = result.provider ?? null; thresholds = admissionThresholdsForProvider(provider)`.
- `hasAnchors(memory, tz)` = `embeddingTextFor(memory, tz) !== embeddingTextFor({summary, content})`.
- Row literal key order: `memoryId, summary, createdAt, hasAnchorLine, anchoredBest, anchoredBestId,
  anchorFreeBest, anchorFreeBestId, anchorFreeAction, anchoredAction` (`:199-210`; the two `*Action` keys NEW,
  `AdmissionAction['kind'] | null`).
- Per row: `anchoredVec = embed(embeddingTextFor(memory, tz), lexicalEmbeddingTextFor(memory))`; `shape =
  memoryIndex.shape(cid, anchoredVec.length)`; `shape.size > 0 && shape.matching === 0` → WARN `Probe embedding
  dimension does not match the store; skipping row` `{ memoryId, probe, store: shape.dominantDimensions }` (was
  `storeDimensions !== null && len !== storeDimensions`). `freeVec = anchored ? anchorFreeEmbedding(memory) :
  anchoredVec` (no `lexicalText` — the anchor-free text IS the lexical text). Searches are `memoryIndex.search(cid,
  vec, { limit: 20, hotOnly: false })`. The anchored and anchor-free decisions come from `decideAdmission({
  candidate: { holderId, aboutId, kind, source, occurredAt }, neighbours, thresholds })` (Phase C lane);
  `folds = admissionReinforced(decision)`; `nearDuplicate = isNearDuplicateDecision(decision, thresholds)`.
- Result key order: `characterId, sampled, anchoredRows, thresholds: { reinforce, nearDuplicate } (the provider's),
  anchored, anchorFree, crossedOnlyWithoutAnchors, embeddingsGenerated, lexicalProfile, note, rows`.
  `lexicalProfile = isLexicalEmbeddingProvider(provider)`; `note` = that ? `'Built-in (TF-IDF) profile: anchor
  lines are never embedded, so both sides embed the same text; the comparison is not measurable here. Decisions
  show what admission would do.'` : `null`. **Edge:** an empty sample (or every embed failing) leaves `provider =
  null` → neural thresholds, `lexicalProfile: false`, `note: null` even on a BUILTIN instance.
- INFO `Anchor gate probe complete` `{ characterId, sampled, anchoredRows, anchored, anchorFree,
  crossedOnlyWithoutAnchors, embeddingsGenerated }` (unchanged). The route (`memories/route.ts:685-730`) is
  unchanged (Zod `characterId` uuid, `limit` int 1–200 optional; 404 `Character`; ERROR `[Memories API] Anchor gate
  probe failed` + 500 `Anchor gate probe failed`).
- CLI `packages/quilltap/lib/anchor-probe-command.js` (222 lines): `:211` `if (result.note) console.log(
  `${YELLOW}${result.note}${RESET}`)` (after the `Embeddings` line); Phase C relabels — `would fold` / `near-dup
  absorbed` rows (no threshold numbers), `Fold only without anchors`, `printRows` marks `▸` when
  `folds(anchorFreeAction) && !folds(anchoredAction)` (`folds` = `reinforce|absorb|promote-and-reinforce`) and the
  legend `= would fold into an older row only without the anchor line (reinforce ≥ ${fmt(thresholds?.reinforce,
  2)})`; help text rewritten; bug 239's `--data-dir/--instance/--passphrase` + `resolveServerPort` (sibling).

### 1.18 `lib/memory/index-store/backfill.ts` (150 lines) — the API's two index ops + the batch backfill

- `generateMissingEmbeddings(characterId, { userId, embeddingProfileId?, onProgress? })` (`:29-74`): rows via
  `findByCharacterId`, filter `!m.embedding || m.embedding.length === 0`; `tz = refreshMemoryTimeZone()`; per row
  `generateEmbeddingForUser(embeddingTextFor(m, tz), userId, embeddingProfileId, { priority: 'background',
  lexicalText: lexicalEmbeddingTextFor(m) })` → `memoryIndex.upsert(cid, [{ id, embedding, tier }])`; catch → WARN
  `` `[Memory] Failed to generate embedding for memory ${memory.id}` `` (the id IS in the message) `{ characterId,
  memoryId, userId, error: String(error) }`, `failed++`; end DEBUG `[Memory] Backfilled missing embeddings` `{
  characterId, processed, failed, skipped }`. **No `batchSize`, no periodic save.**
- `rebuildVectorIndex(characterId)` (`:85-101`): `memoryIndex.reload(cid)`; `entries = memoryIndex.entries(cid)`
  (re-reads the rows); `coldIndexed` = cold entries; `failed = memories.countWithoutEmbedding(cid)` (rows with
  `embedding IS NULL`); DEBUG `[Memory] Rebuilt vector index from memory rows` `{ characterId, indexed,
  coldIndexed, withoutEmbedding }`; returns `{ indexed: entries.length, failed }`. Nothing is written. **`failed`
  now means "rows without an embedding"** (was "add failures").
- `enqueueMissingEmbeddings({ userId, characterId?, limit })` (`:108-150`, Phase G): `findIdsWithoutEmbedding`
  (`embedding IS NULL … ORDER BY createdAt DESC LIMIT ?`); none → `{ status:'ok', enqueued: 0, remaining: 0 }`; no
  default profile → `{ status: 'no-profile', remaining }`; else `enqueueEmbeddingGenerateMany(userId, payloads)`
  (one `findActiveByType('EMBEDDING_GENERATE')`, skip busy / duplicate entity ids, ONE `createBatch`, INFO
  `Embedding generate batch enqueued {requested, queued, alreadyQueued}` or DEBUG `Embedding batch: every entity
  already has a job {requested}`, `publishRealtime('jobs')` + `ensureProcessorRunning()` when any queued); DEBUG
  `[Memory backfill] Embedding jobs enqueued {characterId ?? '(all)', enqueued, remaining}`. Callers:
  `?action=backfill-embeddings` (`route.ts:1101`) and the repair action (Phase G lane).

### 1.19 The Memories API embeddings actions (`app/api/v1/memories/route.ts`)

- `generateEmbeddingsSchema = z.object({ characterId: z.uuid('Character ID is required') })` (`:150-152`) —
  **`batchSize` REMOVED** (`(pre) :112` `z.number().min(1).max(50).prefault(10)`). Zod strips unknown keys: a body
  with `batchSize: 999` (or `"x"`, or `0`) now SUCCEEDS where it was a 400.
- `handleGenerateEmbeddings` (`:1136-1182`): unchanged response shapes (`All memories already have embeddings` /
  `Embedding generation complete` + `processed, failed, skipped, total, remaining`); calls
  `generateMissingEmbeddings(characterId, { userId })`.
- `handleRebuildIndex` (`:1226-1244`): unchanged shape (`Vector index rebuilt successfully`, `indexed`, `failed`) —
  new SEMANTICS per §1.18.
- `handleEmbeddingStatus` unchanged.
- `handleBackfillStart` (`:1081-1134`): `backfillStartSchema` unchanged (`batchSize` int 1–2000 prefault 500); body
  now via `enqueueMissingEmbeddings`; `(pre)` per-row `enqueueEmbeddingGenerate` + WARN `[Memories API] Failed to
  enqueue backfill embedding job` GONE; `(pre)` `enqueued` counted every non-throwing call (deduped ones too) — pin
  counts only fresh jobs; the profile check now happens AFTER the missing read (unchanged order) but a
  `no-profile` answer carries `remaining`. Same messages.
- Manual create / PUT / DELETE keep `scheduleRefit(user.id)` fire-and-forget with their WARNs `[Memories API]
  Failed to schedule refit` / `[Memories API v1] Failed to schedule refit after update|delete`.

### 1.20 The character optimizer's vector source (`lib/services/character-optimizer.service.ts:734-741`)

`memoryIndex.search(characterId, embeddingResult.embedding, { limit: 500, hotOnly: true })` replaces
`getCharacterVectorStore(characterId).search(…, 500, isHotVector)` — same filter-before-top-K semantics; the
vectors now come from `memories.embedding` and a multi-length store scores only same-length entries. The
`rankMemoriesByWeight` → `sortBySalience` swap (`:774-775`) is Phase A's (scoring lane).

### 1.21 `lib/import/quilltap-import/execute.ts:400-415`

BUILTIN default after a memory import → `scheduleRefit(userId, profile.id, { force: true })` (was unforced):
imported rows keep their original `createdAt`, so drift cannot see them. WARN `Failed to schedule vocabulary refit
after import` unchanged.

---

## 2. What v4 DELETED in the span that v5 still carries

| v4 deleted | v5 twin still on `main` |
|---|---|
| `lib/schemas/vector-indices.types.ts` + `lib/database/repositories/vector-indices.repository.ts` (`af48f6cc2`) | `crates/quilltap-core/src/db/vector_indices.rs` (whole, 347 lines); `db/mod.rs:150` (`pub mod vector_indices`), `:565-569` (`Writer::vector_indices()`); `harness … embedding_blob_binding_guard.rs:106-110` census row |
| `CharacterVectorStore.save` / `updateVector` / dirty sets / `loadColdIds`; `VectorStoreManager.deleteStore/saveAll/saveStore`; the meta-row dimension | `db/vector_store.rs:110-175` (`load` over `VectorIndicesRepository` + `find_cold_ids_by_character_id` + meta dims), `:309-345` (`add_vector` dim-throw), `:369-386` (`update_vector`), `:392-430` (`flush`), the `added/removed/updated` fields `:104-107`, `push_unique` |
| EMBEDDING_GENERATE's vector-table write + `unloadStore` | `services/embedding_generate_job.rs:375-398` |
| reindex `all`'s per-character `deleteStore` | `services/embedding_reindex_job.rs:290-302` (+ doc `:84-88`) |
| reapply's `vector_entries` walk | `services/embedding_reapply_profile.rs:48-58` (`MAIN_DB_TABLES: [&str; 5]`) |
| reconcile's two vector arms + `vectorEntriesDeleted` / `vectorIndexMetaFixed` | `services/embedding_dimension_reconcile.rs:11-14` (doc), `:134-137` (fields), `:317-337` (arms), `:376-390` (touched + INFO keys); `quilltap-host/src/host.rs:1229-1230` (boot INFO keys); `services/backup/restore/mod.rs:110-111` + `orchestrator.rs:2143-2144, 2163-2164` (restore record — carriers lane) |
| `memory-service.ts` `generateMissingEmbeddings({batchSize})` / `rebuildVectorIndex` (Phase E moved, Phase B rewrote) | `services/memory_service.rs:1727-1895` (`generate_missing_embeddings`, `rebuild_vector_index`, `flush_store`) |
| `?action=embeddings`'s `batchSize` | `api/memories.rs:1836-1856` (parse + 1–50 400), `api/types.rs:1758-1762` (`MemoryGenerateEmbeddings.batch_size`), `api/engine.rs:3944-3957`, `apps/web/src/app/core/core-contract.ts:6089-6093` |
| the probe's `MERGE_THRESHOLD` / `NEAR_DUPLICATE_THRESHOLD` bands, `buildMemoryEmbeddingText` | `services/anchor_gate_probe.rs:19-20, 68, 115-147, 196-214, 409, 455-456, 555-556` |
| `embedding-refit.ts`'s `characters.findByUserId` enumeration + `No characters found` | `services/embedding_refit_job.rs:77-101` |
| the `cold ids` WARN `Could not read cold memory ids; treating every vector as hot` | `db/vector_store.rs:126-138` |
| the archive's `deleteVectorStore` step | `services/character_archive/service.rs:1219`, `:1271-1290` (retire lane) |
| cascade's `deleteStore` | `services/cascade_delete.rs:23`, `:376` (retire lane — §1 hazard 2's half-delete) |
| delete-all's two tables | `services/delete_all.rs:205-206` (carriers lane) |
| backup `vector-entries.json` / `vector-index-metas.json` + restore 22j | `services/backup/collect.rs:118-119, 345-, 521-522, 646-672, 733-734, 842-843, 886-887, 971-995`; `manifest.rs:80-81`; `staging.rs:74-75, 117-118`; `restore/archive.rs:185-186, 232-233`; `restore/orchestrator.rs:1766-1800, 4347-4348, 4439-4440, 4602-4603`; `restore/mod.rs:74-75`; `uuid_remap.rs:353-365, 494-496`; `restore/preview.rs:58-59` (carriers lane; v4 keeps READING a legacy `vector-entries.json` to fill row-less embeddings — `legacyVectorsAdopted`) |
| `getTfIdfVectorizer`'s plugin import | none (v5 never had it) |

---

## 3. The behaviour deltas a port must hit (numbered)

**A. The lexical text seam (`4ae0c4ca6`)**

1. **`lexical_embedding_text_for` + `LEXICAL_TEXT_SHAPE_VERSION = 1`** — v4 `embedding-text.ts:57, 75-77`. v5: new
   (proposed `crate::memory_index::embedding_text`, §7); `crate::episodic::build_memory_embedding_text`
   (`episodic.rs:114-126`) already builds `base` the same way — re-express it on the lexical builder so the two
   cannot drift.
2. **`is_lexical_embedding_provider`** — v4 `cosine-scale.ts:18-23` (`!!provider` → an empty string is NOT
   lexical). v5: new pure module; re-point the literal scale checks at `memory_weighting.rs:204`
   (`default_min_cosine_for_provider` — the recall-floor lane may move this) and `memory_service.rs:777` (R1 gate
   off — Phase E lane). Provider-ROUTING checks (`embedding_provider.rs:225`, `embedding_refit_job.rs:51`,
   `embedding_dimension_reconcile.rs:294`, `quilltap_import/mod.rs:1281`, `api/embedding_profiles.rs:112, 618,
   725`) stay literal, as v4's do.
3. **The provider seam carries `lexical_text`** — v4 `embedding-service.ts:93-102, 199-264, 275-295, 308-360`. v5:
   `model/embedding.rs:67-78` (the trait), `:86-96` (`Arc<T>` forward), `:172-200`, `:206-220`, `:226-290`
   (canned / none / dyn / erased); `services/embedding_provider.rs:183-235` (resolution + BUILTIN dispatch),
   `:605-627` (impl); `services/builtin_embedding.rs:20-90` (`generate_builtin_embedding(db, text, profile)` gains
   `lexical_text: Option<&str>`, emits the DEBUG with UTF-16 lengths via `crate::jsstr::utf16_len`, embeds
   `lexical_text.unwrap_or(text)`). The API path IGNORES it. Proposed shape §7.2.
4. **Every MEMORY writer passes `lexical_text`** — v4 §1.3 list. v5 sites in THIS slice:
   `embedding_generate_job.rs:349-356` (memory_try), `memory_service.rs:1768-1776` (generate_missing →
   `backfill`), `anchor_gate_probe.rs` (anchored embed). Sibling sites (pass-through, each lane's): admission
   embed/re-embed/neighbour re-embed/update (`memory_gate.rs`, `memory_merge.rs` → Phase C), consolidation digests
   (`consolidation.rs` → Phase F). A writer that forgets it silently embeds the anchored text on BUILTIN — **a
   census guard is the pin** (§5.6).
5. **EMBEDDING_GENERATE MEMORY text = `embedding_text_for(memory, zone)`, + the new DEBUG** — v4
   `embedding-generate.ts:151-163`. v5 `embedding_generate_job.rs:311-326` (plain concat → anchored, zone from the
   time lane's resolver), oversize guard on the anchored text (`:342-348`), DEBUG `[EmbeddingGenerate] Built memory
   embedding text {context, memoryId, textLength}` (UTF-16 length). Rewrite the module doc `:29-33` (false since
   `c2795733f`).

**B. Vectors only on the row (Phase B, this slice's consumers)**

6. **EMBEDDING_GENERATE writes the row only** — v4 `:174-176`. v5 `embedding_generate_job.rs:358-398`: keep the
   `update_for_character(cid, mid, {embedding})` write (`:358-373`, identical patch), DELETE the vector-table block
   (`:375-398`); `markAsEmbedded` + INFO unchanged. Route the row write through the index lane's
   `memory_index::upsert` if it exists (it emits `Upserted memory vectors`), else keep the direct repo write — the
   only DB effect is the `embedding` column.
7. **Reindex `all` clears nothing for memories** — v4 `embedding-reindex.ts:240-244`. v5
   `embedding_reindex_job.rs:290-302` deleted outright; doc `:84-88`, `:79-83` rewritten.
8. **Reapply walks four main tables** — v4 `reapply-profile.ts:48`, WARN text `[ReapplyProfile] Failed to drop the
   memory index caches` (unreachable in v5 — no cache; record as a documented no-op as `:27` does today). v5
   `embedding_reapply_profile.rs:48-58` → `[&str; 4]` (the boot ensure drops the table, so the old entry is dead
   but would skip anyway via `table_exists`; delete it for fidelity and the help-text match).
9. **Dimension reconcile loses its vector arms + two result fields** — v4 §1.10. v5
   `embedding_dimension_reconcile.rs:134-137, 317-337, 376-390` + tests `:607-718, 896, 918, 974`; host INFO
   `host.rs:1229-1230` drops `vector_entries_deleted` / `vector_index_meta_fixed`; the INFO fires only on
   `total_mismatched > 0`. Restore record fields → carriers lane.
10. **The vector source = `memories.embedding` (+ `tier`) rows; multi-length store** — v4 §1.11. v5
    `db/vector_store.rs` rewritten (index lane / keystone owns; this slice consumes): `load` from the keystone's
    `find_index_rows_by_character_id`; `dimensions()` → most-common length; `search_filtered`'s guard → "no entry of
    the query's length" + the WARN `Query vector dimension mismatch — search embedding profile likely differs from
    index` (v5 has NO such WARN today — `vector_store.rs:233-237` returns `[]` silently, a pre-existing gap); a
    `shape(query_len)` accessor; `add_vector` never errors. Consumers in this slice: optimizer
    (`generators/optimizer.rs:1639-1648` — no code change if `load` keeps its signature), the probe (#16), the
    backfill/rebuild (#13-14).
11. **`?action=embeddings` drops `batchSize`** — v4 `route.ts:150-152`. v5 `api/memories.rs:1843-1856`: delete the
    1–50 check and the `batch_size` parameter; `api/types.rs:1758-1762` — keep `#[serde(default)] batch_size:
    Option<i64>` ACCEPTED-AND-IGNORED **or** delete the field (then confirm `Request` does not
    `deny_unknown_fields`, else a stale SPA body 400s where v4 200s); SPA `core-contract.ts:6092` drop the field;
    `dispatch_wrong_type_census.rs:1139` row follows.
12. **`generate_missing_embeddings`** — v4 `backfill.ts:29-74`. v5 `memory_service.rs:1727-1820` → moves with the
    Phase E deletion of `memory_service.rs` (proposed `crate::memory_index::backfill`): anchored text (zone) +
    `lexical_text`; row write through the index (no store, no `batch_size`, no flush); WARN message carries the id
    (`[Memory] Failed to generate embedding for memory {id}` — v5's `:1796-1802` drops it, a pre-existing
    divergence); end DEBUG `[Memory] Backfilled missing embeddings`.
13. **`rebuild_vector_index` is a read** — v4 `backfill.ts:85-101`. v5 `memory_service.rs:1826-1879`: no delete, no
    add, no flush; `indexed` = decodable rows loaded, `failed` = `count_without_embedding(cid)`
    (`memories_read.rs:1137`), DEBUG `[Memory] Rebuilt vector index from memory rows {characterId, indexed,
    coldIndexed, withoutEmbedding}`. Edge: a ZERO-LENGTH blob row counts in neither (`IS NULL` false; undecodable).
14. **Backfill-start via the batch enqueue** (Phase G, `b045dd986`) — v4 §1.18 / §1.19. v5
    `api/memories.rs:1750-1830` (`memory_backfill_start`): counts fresh jobs only, ONE `create_batch`, dedupe
    against active `EMBEDDING_GENERATE` rows via `find_active_by_type`. **Owner to rule** (§9-4): it shares
    `find_active_by_type` with #19.

**C. The refit policy (`4ae0c4ca6`)**

15. **`tfidf_vocabularies.textShapeVersion`** — read (`find_by_profile_id` → `text_shape_version`, NULL / absent →
    0, accept REAL and INTEGER storage), written by the refit (1) and by `create` (0 when omitted — v4's Zod
    default). v5 `db/tfidf_vocabulary.rs:68-104` (`TvReadRow`), `:109-127` (`TvCreate`), `:138-150` (`TvUpdate`),
    `:166-192` (`create` INSERT list), `:194-260` (`update`), `:265-300` (`upsert_by_profile_id` forwards it). The
    boot ensure (keystone, §7.4) must run before the first INSERT names the column.
16. **The anchor probe** — v4 §1.17. v5 `services/anchor_gate_probe.rs` (736 lines): one loaded snapshot reused for
    shape + both searches (do NOT reload per row); the `shape.size > 0 && matching == 0` skip with `store =
    dominant dims`; anchored embed with `lexical_text`; provider-picked thresholds from the FIRST embed result;
    decisions via the Phase C decider (`anchored_action` / `anchor_free_action`, bands from `folds` /
    `near_duplicate`); `lexical_profile` + `note` (byte-exact string, §1.17); result + row key order per §1.17;
    the zone for `has_anchors` / the anchored text. Route unchanged (`api/memories.rs:2094-2195`); the 500-arm unit
    test (`:2400-2430`, `DROP TABLE "vector_entries"` → `no such table: vector_entries`) must find a new trigger
    (v4's load never throws — the arm is reachable only by an unexpected throw).
17. **CLI `quilltap anchor-probe`** — v4 §1.17 last bullet. v5 `crates/quilltap-cli/src/anchor_probe_cmd.rs`
    (438 lines): the YELLOW `note` line after `Embeddings` (`~:368`); `would fold` / `near-dup absorbed` /
    `Fold only without anchors` labels (`:376-400`); `print_rows` (`:186-250`) keyed on `anchoredAction` /
    `anchorFreeAction` with the new legend; `help/anchor_probe_help.txt` + `main_help.txt:19` ("Memory Gate" →
    admission) + the three completion templates. Port-resolution flags are bug 239's lane.
18. **The refit handler** — v4 §1.6. v5 `services/embedding_refit_job.rs:54-173`: enumerate by
    `memories_read::find_distinct_character_ids` (`memories_read.rs:1273`, exists) + `find_by_character_id` per id,
    in that order (vocabulary index order depends on it); drop the characters read (and its mount-index
    requirement, `:77-95`); stamp `text_shape_version = LEXICAL_TEXT_SHAPE_VERSION`; emit the nine v4 lines (v5
    emits none — the `Corpus gathered` DEBUG is new, the rest were already owed). `TfIdfVectorizer` / corpus text
    unchanged (§0 correction 2).
19. **The new repository reads** — v4 §1.14. v5: `db/background_jobs.rs` + `find_active_by_type(job_type) ->
    Vec<BackgroundJob>` (`WHERE type = ?1 AND status IN ('PENDING','PROCESSING') ORDER BY createdAt ASC`, fallback
    `[]` + v4's ERROR); `db/embedding_profiles.rs` + `find_instance_default(conn) -> Option<EmbeddingProfileRow>`
    (`WHERE isDefault = 1 LIMIT 1`, rowid order — model it on `find_default` `:126-140`; fallback `None`);
    `db/memories_read.rs` + `count_corpus_since(conn, since: Option<&str>) -> CorpusCounts { total, created_since }`
    (`COUNT(*)`; `COUNT(*) WHERE createdAt >= ?1` only when `Some`; fallback `{0,0}` + ERROR `Error counting the
    memory corpus`).
20. **`builtin_refit_policy`** — v4 §1.4 whole: pure `evaluate_refit_need` / `drift_threshold`, `assess`,
    `refit_in_hand`, `enqueue_refit_if_due` (serialised — a process-wide `tokio::sync::Mutex<()>` held across
    check-then-enqueue is v5's `refitChain`), `check_builtin_refit`; the five log lines byte-exact with v4's key
    order; the target = the child logger's `module`.
21. **Boot PHASE 3.8** — v4 `instrumentation.ts:995-1013`. v5 `quilltap-host/src/host.rs` right after
    `reconcile_embedding_dimensions_at_boot` (`:1196-1260`): spawn `check_builtin_refit` fire-and-forget (it never
    fails). The import-failure WARN has no v5 analogue.
22. **Maintenance step 10** — v4 `scheduled-maintenance.ts:292-297`. v5
    `services/scheduled_maintenance.rs`: `MaintenanceSweepSummary` (`:80-121`) + `builtin_refit: BuiltinRefitSweep {
    reason: Option<&'static str>, enqueued: bool }` after the fold catch-ups; the step after step 8/9 (`:367-382`,
    step 9 is Phase F/D's); `builtinRefitJson` in the end INFO (`:440-450`, as `{"reason":…,"enqueued":…}`);
    module doc list.
23. **`scheduleRefit`'s timer-side** — the 5 s debounce stays a host-cadence seam (`embedding_refit_job.rs:7-13`,
    `quilltap_import/mod.rs:1262-1285`, `api/memories.rs:598, 663`). What is now portable without a timer is the
    DECISION: `enqueue_refit_if_due(profile, force, "scheduleRefit")` + the two `[EmbeddingScheduler]` INFOs. See
    §9-1 for the ruling this needs.
24. **Import forces the refit** — v4 `execute.ts:408`. v5 `quilltap_import/mod.rs:1262-1285` (the P4.D62
    deferral): a forced `enqueue_refit_if_due` after the import's `Db::write` returns (it cannot run inside the
    closure — the existing comment's finding). §9-1.

**D. Thresholds + cluster resolver (definitions)**

25. **`AdmissionThresholds`, `ADMISSION_THRESHOLDS`, `ADMISSION_THRESHOLDS_TFIDF`,
    `admission_thresholds_for_provider`, `resolve_cluster_threshold`, `tfidf_cluster_threshold`** — v4 §1.16. v5
    today: the neural constants live in `services/memory_gate.rs` (`MERGE_THRESHOLD` 0.85, `NEAR_DUPLICATE_THRESHOLD`
    0.90, the related band); `db/instance_settings.rs:696-800, 968-980` (consolidation settings: add
    `tfidf_cluster_threshold: f64` default 0.30, `min 0 max 1`, serialized right after `clusterThreshold`, parsed in
    the merge-patch arm); `services/consolidation.rs:626-637` (settings read), `:1296` (DEBUG), `:1405`
    (threshold use) → Phase F lane consumes `resolve_cluster_threshold`. SPA field → SPA lane.

**E. NO-PORTs to ratify**

26. Bug 243 (§1.12 evidence). 27. Bug 235's `processSingleton` for the embedding counter, the store manager, the
    refit debounce/forced maps and `refitChain` (one Rust process, statics are already process-wide). 28. The job
    child's `memoryIndex.reload` / `indexInserted` plumbing (`child-entry.ts:230, 244`, `job-dispatcher.ts:868`,
    `host/vector-store-invalidation.ts`) and v5's `write_apply.rs:36` doc — v5 has no cache to invalidate.
    29. `getTfIdfVectorizer`'s dynamic import + its ERROR / throw. 30. `instrumentation.ts`'s `Failed to import the
    TF-IDF refit check` WARN.

---

## 4. v5 on `main` (`a7c6f0d03`) — per file

| File | What it does | Changes / dies |
|---|---|---|
| `crates/quilltap-core/src/db/vector_indices.rs` (347) | the two-table repo | **DIES** whole; `db/mod.rs:150, 565-569` with it |
| `db/vector_store.rs` (601) | in-memory store over the two tables, flush to them | **REWRITE** (index lane): load from rows, multi-length, no flush/dirty sets, `shape`, WARN on no-matching-length; `Tier` / `VectorSearchResult` / `search_filtered` / `set_tier` / `is_hot` survive; tests `:445-601` rewritten |
| `db/tfidf_vocabulary.rs` (342) | TF-IDF vocab repo + `find_by_profile_id` read | `:68-104`, `:109-150`, `:166-300` gain `text_shape_version` (§3-15) |
| `db/background_jobs.rs` | jobs repo | + `find_active_by_type` |
| `db/embedding_profiles.rs` | profiles reads | + `find_instance_default` |
| `db/memories_read.rs` | memory reads | + `count_corpus_since` (keystone may host; §7.3) |
| `model/embedding.rs` (348) | the embedding trait + canned/none/erased | trait gains the call options (§7.2); every impl/forwarder |
| `services/embedding_provider.rs` (1056) | API provider + BUILTIN dispatch | `:183-235` thread `lexical_text` to the BUILTIN branch only; `:605-627` |
| `services/builtin_embedding.rs` (90) | BUILTIN glue | `:20` signature + DEBUG + `lexical_text.unwrap_or(text)` at `:66-72` |
| `services/embedding_generate_job.rs` (1410) | EMBEDDING_GENERATE | `:29-33` doc false; `:311-326` anchored text + DEBUG; `:349-356` `lexical_text`; `:375-398` DIES; tests `:1037-1410` adjust (no vector tables in fixtures) |
| `services/embedding_reindex_job.rs` (713) | EMBEDDING_REINDEX_ALL | `:79-88` doc; `:290-302` DIES |
| `services/embedding_refit_job.rs` (202) | EMBEDDING_REFIT | `:54-173` rewritten per §3-18 (`is_builtin_profile` `:50-52` stays) |
| `services/embedding_reapply_profile.rs` (539) | Matryoshka reapply | `:48-58` → 4 tables; module doc `:7` |
| `services/embedding_dimension_reconcile.rs` (1032) | boot / restore reconcile | `:11-14`, `:134-137`, `:317-337`, `:376-390` + tests |
| `services/anchor_gate_probe.rs` (736) | the probe | §3-16 — near-total rewrite (decider, lexical, shape, result keys) |
| `services/memory_service.rs` | generate_missing / rebuild (`:1727-1895`) | moved + rewritten (§3-12/13); rest of file is Phase E's |
| `services/scheduled_maintenance.rs` (719) | daily pass | step 10 + summary field + end-log key (§3-22) |
| `services/quilltap_import/mod.rs` | `.qtap` import | `:1262-1285` deferral → forced refit (§3-24, §9-1) |
| `generators/optimizer.rs` (2260) | character optimizer | `:1639-1648` unchanged if `load` keeps its signature; Phase A's ranking is another lane |
| `api/memories.rs` | embeddings / rebuild / backfill / probe | `:1836-1929` (batchSize), `:1931-1953` (rebuild semantics), `:1750-1830` (backfill, §9-4), `:2400-2430` test |
| `api/types.rs:1758-1762`, `api/engine.rs:3944-3957` | wire | `batch_size` (§3-11) |
| `memory_weighting.rs:196-208` | recall floor | literal → `is_lexical_embedding_provider` (or the scoring lane's move) |
| `episodic.rs:114-126` | `build_memory_embedding_text` | re-expressed on `lexical_embedding_text_for`; zone (time lane) |
| `quilltap-host/src/host.rs` | boot | `:1229-1230` keys go; PHASE 3.8 after `:1260` |
| `quilltap-cli/src/anchor_probe_cmd.rs` (438) + `help/anchor_probe_help.txt`, `help/main_help.txt:19`, `help/completion/*.template` | CLI | §3-17 |
| `help/embedding-profiles.md` | vendored help | re-vendor at the pin (§8) |

**Every v5 reader / writer of `vector_entries` / `vector_indices` OUTSIDE the memories repository** (production
code; tests listed after):

| # | Site | R/W | Owner lane |
|---|---|---|---|
| 1 | `db/vector_indices.rs` (whole) | R+W | keystone (dies) |
| 2 | `db/vector_store.rs:124-127` (`load` reads meta + entries), `:392-430` (`flush` writes) | R+W | index lane |
| 3 | `db/mod.rs:150-151, 565-569` | accessor | keystone |
| 4 | `services/embedding_generate_job.rs:375-398` | W (entry + meta) | **this slice** |
| 5 | `services/embedding_reindex_job.rs:290-302` | W (delete per character) | **this slice** |
| 6 | `services/embedding_reapply_profile.rs:52-58` (+ the walk `:286`, `:328-344`) | R+W (truncate) | **this slice** |
| 7 | `services/embedding_dimension_reconcile.rs:321-337` | W (delete / snap) | **this slice** |
| 8 | `services/anchor_gate_probe.rs:407-409` (`load`, `dimensions()`) | R | **this slice** |
| 9 | `generators/optimizer.rs:1639-1642` (`load` + `search_filtered`) | R | **this slice** (signature only) |
| 10 | `services/memory_service.rs:1747` (generate_missing load), `:1836-1846` (rebuild delete + load), `:1884-1892` (`flush_store`) | R+W | **this slice** (backfill) |
| 11 | `services/memory_service.rs:137-139`, `:260-267` (`*_with_vectors` delete + flush), `:715` (search load) | R+W | retire lane / Phase E |
| 12 | `services/memory_gate.rs:443` (search), `:823-825`, `:1036-1042` (add + flush) | R+W | Phase C |
| 13 | `services/housekeeping.rs:547-551` | W | Phase F |
| 14 | `services/memory_dedup.rs:459-463` | W | Phase F (the dedup tool retires) |
| 15 | `services/consolidation.rs:921-1025` (load, stage, flush) | R+W | Phase F |
| 16 | `services/cascade_delete.rs:23, 376` | W (delete) | retire lane |
| 17 | `services/character_archive/service.rs:74, 1219, 1271-1290` | W (delete) | retire lane |
| 18 | `services/delete_all.rs:205-206` | W (wipe list) | carriers lane |
| 19 | `services/backup/collect.rs:646-672, 971-995` (+ fields/clears) | R | carriers lane |
| 20 | `services/backup/restore/orchestrator.rs:1766-1800` (22j) | W | carriers lane |
| 21 | `services/backup/{manifest,staging,uuid_remap}.rs`, `restore/{archive,preview,mod}.rs` | carrier shapes | carriers lane |
| 22 | `quilltap-host/src/host.rs:1229-1230` | log keys | **this slice** |
| 23 | `services/provisioning/fresh_schema.json:35-36, 85-86`; `migration_indexes.json:58-59` | DDL | keystone (D23) |
| — | docs only: `lib.rs:115`, `db/memories.rs:828`, `db/help_docs.rs:72`, `write_apply.rs:36` | — | sweep |

Tests that create / read the tables in-crate: `services/housekeeping.rs:631-811, 1054-1071`, `memory_gate.rs:1589-
2340`, `memory_dedup.rs:696-`, `memory_service.rs:1898-2149`, `embedding_dimension_reconcile.rs:607-1025`,
`anchor_gate_probe.rs:606`, `api/memories.rs:2412-2426`, `realtime/publish_sites.rs:810-813`. Harness:
`anchor_gate_probe_tier3`, `backup_uuid_remap`, `carina_memory_extraction_tier3`, `character_archive_tier2`,
`consolidation_service_tier3`, `context_summary_service_tier3`, `embedding_blob_binding_guard` (`:106-110`),
`embedding_generate_jobs`, `embedding_reapply`, `embedding_remainder`, `fold_episode_tier3`, `memory_cascade_tier2`,
`memory_gate_tier3`, `memory_housekeeping_tier2`, `memory_pipeline_jobs_tier3`, `memory_processor_tier3`,
`memory_watermark_tier3`, `regenerate_swipe_tier3`, `system_restore_state`, `vector_indices_tier2`,
`vector_store_tiers`, `write_apply`, `write_partition`; web `quilltap-web/tests/characters_action_route.rs`.

---

## 5. Harness

Tiers: (1) exact pure; (2) DB-state; (3) mocked-LLM / recorded-embedding. "IMPORT-DEAD" = the oracle (or its
fixture builder) imports a module / export v4 deleted, so it cannot run at the pin.

### 5.1 Families in this slice — predicted state at the pin

| Family (Rust test ↔ oracle case ↔ builder) | Tier | At the pin | Why |
|---|---|---|---|
| `vector_indices_tier2_equivalence.rs` ↔ `cases/vector-indices.ts` ↔ `fixtures/build-vector-indices-fixture.ts` | 2 | **IMPORT-DEAD** | `@/lib/database/repositories/vector-indices.repository` (`:88`) deleted. **RETIRE** the family (both tables gone; `vector-indices-tier2.json` with it) |
| `vector_store_tiers_equivalence.rs` ↔ `cases/vector-store-tiers.ts` | 2 | **DEAD at runtime** | imports resolve, but `builder.save()` (`:114`) is gone (TypeError), and CAPTURED names `Loaded vector index` / `Could not read cold memory ids…` changed. Rewrite as a memory-index-load family (rows → store, multi-length, tiers) — index lane's |
| `embedding_generate_jobs_equivalence.rs` ↔ `cases/embedding-generate-jobs.test.ts` ↔ `fixtures/build-embedding-generate-fixture.ts` | 3 | **builder IMPORT-DEAD; case RED** | builder `:137-138` imports `vector-indices.repository`; the case dumps `vector_entries` / `vector_indices` (`:312-313`); anchored memory text no longer matches the recorded canned keys; new DEBUG line |
| `embedding_remainder_equivalence.rs` ↔ `cases/embedding-remainder.test.ts` ↔ `fixtures/build-embedding-remainder-fixture.ts` | 2/3 | **builder IMPORT-DEAD; case RED** | builder `:288-289` imports `vector-indices.repository` (and plants `legacyRewrites` on `vector_entries` `:543`); reconcile result loses two keys; reindex `all` no longer deletes |
| `embedding_reapply_equivalence.rs` ↔ `cases/embedding-reapply.test.ts` ↔ `fixtures/build-ep-mgmt-fixture.ts` | 2 | **builder IMPORT-DEAD; case RED** | the ep-mgmt builder uses `repos.vectorIndices`; the case's `MAIN_TABLES` (`:72`) lists `vector_entries` |
| `embedding_refit_tier3_equivalence.rs` ↔ `cases/embedding-refit-tier3.test.ts` ↔ `fixtures/build-embedding-refit-fixture.ts` | 3 | **RED** | the builder's `ensureCollection('tfidf_vocabularies', TfidfVocabularySchema)` (`:202`) now creates `textShapeVersion REAL DEFAULT 0`; v4 stamps 1, v5 leaves 0; v4's corpus is by distinct memory characterId (same rows on this fixture unless it holds a vault-less character — measure) |
| `tfidf_vocabulary_tier2_equivalence.rs`, `tfidf_vocabulary_upsert_tier2_equivalence.rs` (+ builders) | 2 | **GREEN-likely, measure** | both dumps are `PRAGMA table_info`-driven over the SAME pin-built fixture (`:246-250`, `:199-202`), so the new column appears on both sides; v4 `create` writes 0, v5's INSERT omits it → SQL DEFAULT 0 (REAL); confirm the cell renders identically (`0` vs `0.0`). Add rows (§5.3) |
| `tfidf_vectorizer_equivalence.rs` ↔ `cases/tfidf-vectorizer.ts` | 1 | GREEN | plugin unchanged in the span (`git diff --stat 4a38222e6 64743ee13 -- plugins/` empty) |
| `embedding_provider_tier3_equivalence.rs` ↔ `cases/embedding-provider-tier3.test.ts` | 3 | GREEN | calls pass no `lexicalText`; add rows (§5.3) |
| `embedding_status_tier2_equivalence.rs`, `embedding_profiles_tier2_equivalence.rs`, `embedding_profiles_routes_equivalence.rs` | 2 | GREEN | `embedding-status.repository` unchanged; profile routes untouched; `findInstanceDefault` is additive |
| `anchor_gate_probe_tier3_equivalence.rs` ↔ `cases/anchor-gate-probe-tier3.test.ts` (+ `fixtures/anchor-gate-probe-cases.json`) | 3 | **RED** (imports live) | runs REAL through the fixture's fitted BUILTIN profile, so at the pin: `lexicalProfile: true`, the `note`, TF-IDF thresholds (0.40 / 0.80), `*Action` keys, both sides embedding the same text; `probe_dim_mismatch` plants on `vector_indices` (now unread — rewrite to plant a different-width `memories.embedding`); `route_vector_entries_dropped` (`DROP TABLE vector_entries`) no longer reaches a 500; the committed pair's vectors must be ON the rows (check the episodic-recall pair before the regen) |
| `character_optimizer_tier3_equivalence.rs` ↔ `cases/character-optimizer-tier3.test.ts` | 3 | **RED** | Phase A's `sortBySalience` ordering (scoring lane) + the vector source (rows); its fixture (`build-characters-fixture.ts`) uses `repos.vectorIndices` → check IMPORT-DEAD |
| `semantic_search_tuning_tier3_equivalence.rs` ↔ `cases/semantic-search-tuning-tier3.test.ts` | 3 | **IMPORT-DEAD** | imports `@/lib/memory/memory-service` (deleted, Phase E) + builder `getCharacterVectorStore` + `vector_entries` count — Phase E lane re-aims |
| `embedding_blob_binding_guard.rs` | guard | **RED once `vector_indices.rs` is deleted** | `ENCODE_CENSUS` row `:106-110`; v4's `manager.ts:118-126` now registers FOUR tables (vector_entries gone) — drop the row, keep the guard |
| `maintenance_ops_tier2_equivalence.rs` ↔ `cases/maintenance-ops-tier2.ts` | 2 | **RED** | summary gains `builtinRefit` (and `foldEpisodeCatchup`, Phase D/F's) |
| `qtap_import_equivalence.rs` | 2 | GREEN for this slice | the forced refit is debounced 5 s in v4 → no row at dump time (the existing deferral's finding still holds) |
| `embedding_wire`, `embedding_vector`, `average_embeddings`, `legacy_embedding`, `index_keyed_embedding`, `cold_chunk_reembed_tier2` | 1/2 | GREEN | untouched (`literal-boost.ts` is a doc-comment hunk) |
| `provisioning_equivalence` (`QT_FRESH_SCHEMA_LIVE`) | 2 | RED until the D23 re-dump | `tfidf_vocabularies` gains the REAL column; the vector tables + 2 indexes go |
| `migration_index_backfill_equivalence` | 2 | RED until re-dump | `idx_vector_{entries,indices}_characterId` leave `migration_indexes.json` |

### 5.2 New families to add (and the closest recipe to copy)

1. **`builtin_refit_policy_equivalence`** (tier 1 + tier 2): (a) tier-1 `evaluate_refit_need` / `drift_threshold`
   over a corpus mirroring v4's `builtin-refit-policy.test.ts` (fittedAt null / `''`, version 0 / 1 / 2 / null,
   drift at the bar ±1 across totals 0, 24, 25, 249, 250, 251, 10,000) plus `is_lexical_embedding_provider`
   (`BUILTIN`, `builtin`, `''`, null, `OPENAI`) and `lexical_embedding_text_for` (content ending `(place: X)`);
   copy `tfidf_vectorizer_equivalence.rs` (tsx tier-1). (b) tier-2 `enqueue_refit_if_due` / `check_builtin_refit`
   over planted DBs: no vocabulary; version-0 row; drift exactly at the bar; 0 memories (`nothing-to-fit`);
   in-hand PENDING refit; in-hand PROCESSING reindex with `force` (enqueues) and without (held); non-BUILTIN
   default; no default; a default owned by another user (instance-wide). Compare decision + `background_jobs`
   dump (id/time masked) + the five captured lines. Copy `embedding_refit_tier3_equivalence.rs` + the logger
   splice from `anchor-gate-probe-tier3.test.ts`. v4 jest mocks the queue — the oracle must use the REAL
   `enqueueEmbeddingRefit` with `ensureProcessorRunning` stubbed (the refit-tier3 precedent).
2. **`tfidf_text_shape_ensure_equivalence`** (tier 2, keystone may own): v4's REAL `add-tfidf-text-shape-version-v1`
   through the narrowed `MigrationRunner` vs v5's ensure, three shapes: table absent (no-op), column absent (append
   `INTEGER DEFAULT 0`), fresh-schema column present (`REAL`, no-op); `table_info` + `sqlite_master` byte-equal.
   Copy `chat_informs_permanent_ensure_equivalence.rs`.
3. **`lexical_text_writers_guard`** (census): every v5 call that embeds a MEMORY passes `lexical_text` (grep the
   provider call sites whose text comes from `embedding_text_for`); the census pins the writer list (§1.3 at the
   pin = 8 sites). Copy the `activity_span_sites_guard.rs` shape (`:67`).

### 5.3 Corpus rows to ADD to existing families

- `embedding_provider_tier3`: a BUILTIN call with `lexicalText` ≠ `text` (the vector must equal the
  lexical-text vector; the DEBUG `[Embedding] BUILTIN embed: using the caller's lexical text` with
  `textChars`/`lexicalChars` incl. a non-BMP char); an OPENAI call with `lexicalText` (wire body carries `text` —
  proves the API path ignores it).
- `embedding_generate_jobs` (rebuilt fixture, no vector tables): an anchored memory under a NEURAL profile (canned
  key = anchored text) and under BUILTIN (vector = lexical); the `Built memory embedding text` DEBUG; an
  oversize anchored text whose lexical text is under the cap (guard reads the anchored one).
- `embedding_refit_tier3`: a vault-less character's memories in the corpus (enumeration by memories); the stamped
  `textShapeVersion = 1`; the nine log lines; a pre-existing version-0 row upserted.
- `tfidf_vocabulary*_tier2`: create without the field (0), create with 1, update passing it, update omitting it.
- `embedding_remainder`: reconcile result without the two keys; the INFO only on `totalMismatched > 0`.
- `anchor_gate_probe_tier3`: the BUILTIN rows as above; an empty-sample case (`lexicalProfile: false` on a
  BUILTIN instance); a multi-length store (`matching == 0` skip vs `size == 0` proceed); for a NEURAL case the
  family needs a scripted embedding seam (v4 `fetch` scripting as `embedding-provider-tier3` does) — see §9-6.
- `maintenance_ops_tier2`: `builtinRefit` `{reason:null, enqueued:false}` on a neural instance; `{reason:'drift',
  enqueued:true}` on a BUILTIN instance with a drifted corpus.

### 5.4 v4 side of the regen

PIN REQUIRED at `64743ee13` (ledger §5.1): a pinned worktree with its own `npm ci --offline`. Rebuild every
builder above at the pin (the IMPORT-DEAD ones must first drop `repos.vectorIndices` / `VectorIndicesRepository` —
plant vectors on `memories.embedding` instead). The committed `*-main.db` pairs these families read widen through
v4's real migrations at the pin (keystone's job) — after which `vector_entries` is GONE from them, so any plant
`INSERT INTO vector_entries` (e.g. `cases/search-tools-search.test.ts:171-173`, Phase E's) fails.

---

## 6. v4 jest tests at the pin covering this slice

| Test | ≈ cases | Mirror for |
|---|---|---|
| `__tests__/unit/lib/embedding/builtin-refit-policy.test.ts` | 16 | §5.2-1 (the whole policy incl. serialisation and `force`) |
| `__tests__/unit/lib/embedding/builtin-lexical-text.test.ts` | 5 | `lexical_embedding_text_for`, the BUILTIN service path |
| `__tests__/unit/lib/memory/provider-thresholds.test.ts` | 6 | thresholds + `admission_thresholds_for_provider` |
| `__tests__/unit/lib/memory/consolidation-provider-threshold.test.ts` | 3 | `resolve_cluster_threshold` |
| `__tests__/unit/lib/memory/anchor-gate-probe-lexical.test.ts` | 2 | `lexicalProfile` / `note` |
| `__tests__/unit/lib/background-jobs/handlers/embedding-memory-anchor-text.test.ts` | 2 | anchored text + `lexicalText` in EMBEDDING_GENERATE |
| `__tests__/unit/lib/background-jobs/handlers/embedding-generate-extended.test.ts` | 14 | the four branches |
| `lib/background-jobs/handlers/__tests__/embedding-reindex.test.ts` | 3 | reindex (no store deletes) |
| `__tests__/unit/lib/background-jobs/child-plugin-readiness.test.ts` | 6 | bug 243 (NO-PORT evidence only) |
| `__tests__/unit/lib/embedding/vector-store.test.ts` | 36 | the multi-length cache (index lane) |
| `__tests__/unit/lib/embedding/vector-store-process-singleton.test.ts` | 2 | bug 235 (NO-PORT) |
| `__tests__/unit/lib/memory/memory-index-and-retire.test.ts` | 26 | index ops (index / retire lanes) |
| `__tests__/unit/lib/memory/index-store-backfill.test.ts` | 1 | `rebuild_vector_index` |
| `__tests__/unit/lib/embedding/embedding-service.test.ts` | 11 | the service |
| `__tests__/unit/lib/import/import-memory-embeddings.test.ts` | 8 | the forced refit after import |
| `__tests__/unit/lib/startup/reconcile-embedding-dimensions.test.ts` | 7 | the narrowed reconcile |
| `__tests__/unit/lib/database/migration/retire-memory-vector-entries-v1.test.ts` | (keystone) | the vector retirement |
| `__tests__/unit/api/memories-edit-embedding-failure.test.ts` | 1 | (Phase C) |
| `__tests__/unit/lib/backup/restore-embedding-reconcile.test.ts` | 6 | (carriers) |

No jest test exists for `add-tfidf-text-shape-version-v1` itself — the ensure family (§5.2-2) is the only pin.

---

## 7. Cross-lane meeting points and PROPOSED Rust spellings

### 7.1 NEEDS from the schema keystone

- The D23 re-dump at `64743ee13`: `fresh_schema.json` with `"textShapeVersion" REAL DEFAULT 0` (verify) and
  without the two vector tables / two `createdAt` indexes; `migration_indexes.json` without the two `characterId`
  indexes.
- The `add-tfidf-text-shape-version-v1` boot ensure — an UNSTAMPED, table-gated `ALTER TABLE "tfidf_vocabularies"
  ADD COLUMN "textShapeVersion" INTEGER DEFAULT 0` when the column is missing, with v4's INFO / ERROR lines (pure
  DDL, idempotent — the P4.D249 `chat_informs.permanent` precedent; NOT a `migrations_state` gate). Added to the
  restore mirror-boot list (memory note "Vintage restore mirror-boot list").
- The `retire-memory-vector-entries-v1` ensure (RULED: mirror) — runs BEFORE any of this slice's reads assume
  the tables are gone; and the index-row read `find_index_rows_by_character_id(conn, cid) -> Vec<IndexRow { id,
  embedding: Vec<f32>, tier: Tier }>` (memories repository).
- `count_corpus_since` (if the keystone hosts memory reads); `count_without_embedding` / `find_ids_without_embedding`
  (exist).

### 7.2 The provider seam (this slice PROVIDES; every memory-writing lane consumes)

Recommended (mirrors v4's `EmbeddingCallOptions`; one mechanical sweep, no silent drop through a wrapper):

```rust
// crate::model::embedding
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EmbeddingCallOptions<'a> {
    pub priority: EmbeddingPriority,
    /// v4 `lexicalText`: what a lexical (BUILTIN) profile embeds instead of `text`.
    pub lexical_text: Option<&'a str>,
}
impl<'a> EmbeddingCallOptions<'a> {
    pub const fn interactive() -> Self;            // v4 default lane
    pub const fn background() -> Self;
    pub const fn with_lexical_text(self, t: &'a str) -> Self;
}
pub trait EmbeddingProvider {
    fn generate_embedding_for_user(&self, text: &str, user_id: &str, profile_id: Option<&str>,
        opts: EmbeddingCallOptions<'_>) -> impl Future<Output = Result<EmbeddingResult, EmbeddingError>> + Send;
}
```

Every forwarder (`Arc<T>`, `DynEmbeddingProvider`, `ErasedEmbeddingProvider`) forwards `opts` whole. Canned /
recording doubles key on `text` (v4's oracle mocks receive `(text, ...rest)` and key on `text`) and RECORD
`lexical_text` for assertions. Blast radius: 17 impls (`rg 'EmbeddingProvider for'`) and ~40 call sites (§4).
Alternative (smaller diff, riskier): a defaulted 5-arg method — rejected unless every forwarder overrides it, since
a forgetting wrapper silently embeds anchored text on BUILTIN.

`services::builtin_embedding::generate_builtin_embedding(db: &Db, text: &str, lexical_text: Option<&str>, profile:
&EmbeddingProfileRow) -> Result<EmbeddingResult, EmbeddingError>`.

### 7.3 Pure helpers (this slice PROVIDES)

```rust
// crate::cosine_scale   (new top-level pure module; v4 lib/embedding/cosine-scale.ts)
pub const LEXICAL_EMBEDDING_PROVIDERS: [&str; 1] = ["BUILTIN"];
pub fn is_lexical_embedding_provider(provider: Option<&str>) -> bool; // Some(p) && !p.is_empty() && listed

// crate::memory_index::embedding_text   (v4 lib/memory/index-store/embedding-text.ts; the index lane owns crate::memory_index)
pub const LEXICAL_TEXT_SHAPE_VERSION: i64 = 1;
pub fn lexical_embedding_text_for(summary: &str, content: &str) -> String;
pub fn embedding_text_for(summary: &str, content: &str, anchors: Option<&EpisodicAnchorView>, time_zone: Option<&str>) -> String;
```

(`crate::episodic::build_memory_embedding_text` becomes a thin alias or is retired; the zone argument is the time
lane's.)

### 7.4 Thresholds (DEFINITIONS here; admission / maintenance / consolidation lanes consume)

```rust
// crate::memory_settings   (v4 lib/memory/settings-defaults.ts — or wherever Phase A's settings façade lands)
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AdmissionThresholds { pub near_duplicate: f64, pub reinforce: f64, pub related: f64, pub date_guard_days: f64, pub footnote_cap: usize }
pub const ADMISSION_THRESHOLDS: AdmissionThresholds;        // 0.90 / 0.85 / 0.70 / 7 / 8
pub const ADMISSION_THRESHOLDS_TFIDF: AdmissionThresholds;  // 0.80 / 0.40 / 0.20 / 7 / 8
pub fn admission_thresholds_for_provider(provider: Option<&str>) -> &'static AdmissionThresholds;
pub fn resolve_cluster_threshold(cluster_threshold: f64, tfidf_cluster_threshold: f64, provider: Option<&str>, override_: Option<f64>) -> f64;
```

`&'static` return so the Phase C DEBUG's `thresholdScale` (`thresholds === ADMISSION_THRESHOLDS ? 'neural' :
'tfidf'`) can be a pointer/eq test — or return an enum `ThresholdScale { Neural, Tfidf }` beside it. Settings:
`db::instance_settings::MemoryConsolidationSettings.tfidf_cluster_threshold: f64` (default 0.30; JSON key
`tfidfClusterThreshold` right after `clusterThreshold`).

### 7.5 The refit policy (this slice PROVIDES; boot, maintenance, import, memories API consume)

```rust
// crate::services::builtin_refit_policy   (v4 lib/embedding/builtin-refit-policy.ts)
pub const REFIT_MIN_NEW_MEMORIES: u64 = 25;
pub const REFIT_DRIFT_FRACTION: f64 = 0.1;
pub enum BuiltinRefitReason { Unfitted, TextShape, Drift }          // as_str: "unfitted" | "text-shape" | "drift"
pub enum RefitDecisionReason { Due(BuiltinRefitReason), Forced }    // "forced"
pub struct RefitNeedInput { pub fitted_at: Option<String>, pub text_shape_version: f64, pub new_since_fit: u64, pub total_memories: u64 }
pub fn drift_threshold(total_memories: u64) -> u64;                 // max(25, (0.1 * total as f64).ceil())
pub fn evaluate_refit_need(input: &RefitNeedInput) -> Option<BuiltinRefitReason>;
pub fn assess_builtin_refit(conn: &Connection, profile_id: &str) -> RefitAssessment;   // logs "Assessed the TF-IDF vocabulary"
pub struct RefitInHand { pub job_type: &'static str, pub status: &'static str }
pub fn refit_in_hand(conn: &Connection) -> Option<RefitInHand>;
pub enum RefitHeldBy { NothingToFit, InHand(RefitInHand) }
pub struct RefitDecision { pub reason: Option<RefitDecisionReason>, pub enqueued: bool, pub held_by: Option<RefitHeldBy> }
pub async fn enqueue_refit_if_due(db: &Db, profile_id: &str, user_id: &str, force: bool, source: &str) -> Result<RefitDecision, DbError>;
pub struct BuiltinRefitCheck { pub profile_id: Option<String>, pub reason: Option<BuiltinRefitReason>, pub enqueued: bool }
pub async fn check_builtin_refit(db: &Db) -> BuiltinRefitCheck;      // never fails
```

Serialisation: a `static REFIT_CHAIN: tokio::sync::Mutex<()>` held across assess → in-hand → enqueue (v4's
`refitChain`; v4 chains on failure too — the mutex releases on drop either way). The enqueue reuses
`queue_service::enqueue_embedding_refit(db, user_id, profile_id, true)` (`queue_service.rs:1181-1192`).

### 7.6 Repository additions (this slice PROVIDES)

`BackgroundJobsRepository::find_active_by_type(&self, job_type: &str) -> Result<Vec<BackgroundJob>, DbError>` (+ a
fallback wrapper with v4's ERROR); `embedding_profiles::find_instance_default(conn) -> Result<Option<EmbeddingProfileRow>,
DbError>`; `memories_read::count_corpus_since(conn, since: Option<&str>) -> CorpusCounts` (fallback built in);
`tfidf_vocabulary::{TvReadRow, TvCreate, TvUpdate}.text_shape_version` (`f64` read via `Value`-tolerant get —
REAL on fresh, INTEGER on migrated).

### 7.7 Wire changes (exact)

- `api/types.rs` `Request::MemoryGenerateEmbeddings { character_id, batch_size }` — `batch_size` becomes ignored or
  is removed (§3-11); no Response change (`Response::Memory` JSON shapes unchanged).
- The anchor-probe result JSON gains `rows[].anchoredAction`, `rows[].anchorFreeAction`, `lexicalProfile`, `note`
  and its `thresholds` follow the provider (the `Response::Memory` JSON; serde struct `AnchorGateProbeResult` /
  `AnchorGateProbeRow` field order per §1.17).
- No new Request variant in this slice (the refit check has no route; the profile `?action=refit` / reindex routes
  are unchanged).

### 7.8 What this slice PROVIDES to siblings

`EmbeddingCallOptions` (§7.2) to Phase C (admission embed/re-embed/update), Phase F (consolidation digests), the
index lane (backfill); `is_lexical_embedding_provider` to Phase A (recall floor), Phase E (R1 gate), Phase C/F
(thresholds); the threshold table + `resolve_cluster_threshold` to Phase C / F; `find_active_by_type` to Phase G's
batch enqueue; `count_corpus_since` + `find_instance_default` (single consumer, here).

---

## 8. SPA / help / docs / CLI touch points

- **SPA** (`apps/web/src/app/…`): `core/core-contract.ts:6089-6093` (`MemoryGenerateEmbeddingsRequest.batchSize`
  — drop); the consolidation settings' `tfidfClusterThreshold` field — v4 moved it into
  `components/tools/memory-consolidation-particulars.tsx:14, 28` (Phase G's maintenance card; label `Cluster
  threshold (Built-in)`, help `The same measure when the default embedding profile is the Built-in TF-IDF one,
  which reckons likeness far more sparingly.`, min 0 max 1 step 0.01) — v5's `screens/settings/memory/memory-
  consolidation-card*` → SPA lane; `core-contract.ts:6176, 6322` (`clusterThreshold` types gain the sibling key).
  `memory/memory.api.ts:137` (`batchSize = 500`) is the BACKFILL default — unchanged.
- **Help** (`help/`, re-vendor at the pin): `help/embedding-profiles.md` (`af48f6cc2`: the reapply table list
  without `vector_entries` + `help_doc_chunks`; "does not delete vector stores" dropped; `93c5af6ad`: "Nothing is
  forgotten mid-re-embed"; `4ae0c4ca6`: NEW section "The Built-in Profile, Tailored to Its Temperament", five
  bullets; `b045dd986`: its link → `/settings?tab=memory&section=memory-maintenance`). `help/memory-consolidation.md`
  got one line in `4ae0c4ca6` and is DELETED at the pin (`b045dd986`) — the help lane.
- **Docs** (`docs/v4/`): NEW `developer/features/builtin-tfidf-memory-tuning.md`; `developer/bugs/fixed/
  bug-243-child-registry-race.md`; `DDL.md` (+1 line, the column; the vector tables "retired"); `API.md`.
- **CLI**: `crates/quilltap-cli/src/anchor_probe_cmd.rs`, `help/anchor_probe_help.txt`, `help/main_help.txt:19`,
  `help/completion/{bash,fish,zsh}.template` (anchor-probe description "Memory Gate" → admission); bug 239's flags
  are another lane. No `quilltap` command reads vector tables or TF-IDF (`rg` empty in `quilltap-cli/src`).

---

## 9. Open points for the order (observations / questions — not rulings)

1. **`scheduleRefit` on manual create / edit / delete and on import.** v5 never ported `scheduleRefit` (a 5 s
   debounce = host cadence). At the pin the debounce wraps a cheap decision (`enqueue_refit_if_due`) that is
   almost always "not needed". Options: (a) keep it a host seam (status quo; only boot + daily sweep refit, which
   catches AUTO and manual drift within a day; imports wait for the next sweep unless drift trips — but imports
   keep old `createdAt`, so a large import is invisible until the text-shape/unfitted reasons or the sweep's
   drift); (b) a host-side debouncer calling the core function (exact v4 timing; the differential still cannot
   see the row — v4 enqueues 5 s later); (c) call `enqueue_refit_if_due` immediately (row visible at dump time →
   a recorded divergence). The import's `force` matters most (no other path sees imported rows). Needs a ruling.
2. **D23 type of `textShapeVersion`.** This survey's derivation says the fresh shape is `REAL DEFAULT 0` (§0
   correction 1); the ledger says INTEGER. The re-dump decides; the ensure must then carry REAL (fresh) vs INTEGER
   (migrated) and every reader must accept both storage classes.
3. **Ownership of the vector store / `memory_index` rewrite.** This survey treats `db/vector_store.rs` +
   `memoryIndex` as the keystone / index lane's and consumes `load` / `search_filtered` / `shape`. If the planner
   gives it to this lane, add §1.11 whole (incl. the ≥1000-entry heap path v5 never ported — tie order can differ
   only there) and the `vector_store_tiers` rewrite.
4. **Backfill-start (`?action=backfill-embeddings`) + `enqueueEmbeddingGenerateMany`** (Phase G) — embedding-job
   plumbing that rides `find_active_by_type`. Here or the Phase G API lane?
5. **`?action=embeddings` `batchSize`:** keep the wire field accepted-and-ignored (zero SPA churn, mirrors Zod's
   strip) or delete it (then check `Request`'s unknown-field policy — a stale client body must not 400).
6. **The anchor probe's neural path is untestable with the current oracle.** The family embeds through REAL
   TF-IDF; at the pin BUILTIN makes the anchored and anchor-free sides identical (`note`), so the probe's whole
   point (anchored vs anchor-free) is unmeasured. A neural case needs a scripted provider on both sides (the
   `embedding-provider-tier3` `fetch` script, or v4's `generateEmbeddingForUser` mock keyed by text + a canned
   Rust provider). Also: the probe depends on Phase C's `decideAdmission` — order this lane after (or with a
   handoff from) the admission lane.
7. **No-cache semantics of v4's index DEBUG lines.** `Upserted memory vectors {…, cached}` and `Dropped memory
   index cache {…, dropped}` describe cache state v5 does not have. Mask `cached` / `dropped` in captures, emit
   them as 0/false, or not emit the lines at all — a ruling for the index lane that this slice's
   EMBEDDING_GENERATE / backfill captures inherit.
8. **The DEBUG `[EmbeddingGenerate] Built memory embedding text` and the zone.** EMBEDDING_GENERATE now needs the
   memory-calendar zone (`refreshMemoryTimeZone`) — the time lane's resolver must be callable from the job runner
   (host zone threading, P4.119 / P4.140 precedent).
9. **Pre-existing v5 gaps this slice will touch** (fix in passing or record): `vector_store.rs` search's missing
   WARN on a dimension mismatch; `generate_missing_embeddings`'s WARN dropping the id from the message; the refit
   handler emitting none of v4's nine lines.
10. **Ratify NO-PORTs** (§3 E): bug 243 (measure on a fresh BUILTIN instance's first boot: every help doc
    embedded, no registry-miss line), bug 235's singletons, the child cache plumbing, the dynamic plugin import.
11. **Sizing.** The slice splits cleanly in two if needed: (A) the provider seam + EMBEDDING_GENERATE + reindex /
    reapply / reconcile + backfill/rebuild + `batchSize` (mechanical, wide: ~40 call sites through §7.2); (B) the
    refit policy + `textShapeVersion` + refit handler + boot / sweep / import hooks + the probe + CLI (deep,
    narrow). (A) is a prerequisite for every memory-writing lane's `lexical_text` pass-through — consider it a
    keystone-adjacent first step or a HANDOFF.
12. **Live-data note.** A BUILTIN instance migrated by v4 past `4ae0c4ca6` carries `textShapeVersion = 1` only after
    v4's boot refit ran; one migrated but not yet refit reads 0 — the first v5 boot with the policy enqueues a
    full refit + reindex (cost: one re-embed of the corpus, no provider spend on BUILTIN). A neural default is a
    no-op; check the dogfood copy's default embedding provider before its first boot on this build.
