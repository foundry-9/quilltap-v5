# Survey — memory EXTRACTION across `4a38222e6` → `64743ee13` (Phase D + bugs 206/207/212/213/222/238/245 + the INTER_CHARACTER retirement)

Read-only planning survey, 2026-10-13. v4 = `~/source/quilltap-server`, read ONLY
through `git show 64743ee13:<path>` (the PIN, `4.10.0-dev.175`), `git diff
4a38222e6 64743ee13 -- <paths>` / `git show <sha> -- <paths>` (the span) and
`git show 4a38222e6:<path>` (the PRE shape v5 ported). **Every v4 line number is
AT THE PIN** unless marked `(pre)`. v5 = `main` at `a7c6f0d03`; v5 line numbers
are on that tree. Baseline `4a38222e6`. Nothing here is taken from commit prose
(drift ledger §5.3); every claim is from shipped hunks. The ledger's §1 (LIVE-DATA
HAZARDS, Schema / D23) and the §3 rows for `ac6348c5f`, `93c5af6ad`, `64743ee13`,
`c2795733f`, `e0ac7493a`, `124c8aca3`, `b045dd986` were read first and are built
on, not restated; corrections are in §0.1.

**Scope (this survey):** `lib/memory/extraction/**` whole (22 production files +
4 co-located tests), the extraction satellites it calls or is called from —
`lib/services/chat-message/turn-transcript.ts` (bug 245's builder half),
`lib/schemas/memory-extraction-cursor.types.ts` + the cursor repository (as a
CONTRACT the keystone owns), the three job handlers (`memory-extraction.ts`,
`carina-memory-extraction.ts`, `memory-regenerate-chat.ts`), the enqueue side of
`queue-service.ts`, `maintenance/fold-catchup.ts` + `scheduled-maintenance.ts`
steps 8–9, the fold hook in `lib/chat/context-summary.ts`, the turn-end trigger
(`memory-trigger.service.ts`), the chat `queue-memories` / `extract-memories-dry-run`
actions, the extraction-limits / extraction-mode settings and their routes, and
the `INTER_CHARACTER_MEMORY` retirement. **Out of scope (siblings):** admission
internals (`lib/memory/admission/*`, `candidate.ts` beyond the seam), the time
module (`lib/memory/time/*`), recall (`lib/memory/recall/*`), retire / repair,
maintenance, the TF-IDF tuning, the bundle carriers' wiring (backup / restore /
`.qtap` — only `cursor-bundle.ts`'s contract is here), the schema keystone (the
two migrations, `fresh_schema.json`, the boot ensures), and the SPA.

---

## 0. The span for extraction, in one paragraph

Six commits move extraction. `c2795733f` (#90) fixed the audit's extraction
bugs on the OLD modules — bug 206/207 (`fold-window.ts`: slices of 60, ≤ 4 per run,
`continue:<id>` follow-ups, a NEW `chats.episodeExtractionWatermarkMessageId`),
bug 212 (the extraction CLOCK rendered in the memory-calendar zone, the episode
CLOCK line gaining the date stamp), bug 213 (the episode pass's uncensored
fallback) and bug 222 (Carina carries the project / summary). `e0ac7493a`
(Phase A) removed the CLOCK's "Current in-story time" line and the episode
prompt's `participants` field, renamed the parser's output `MemoryDraft` behind a
Zod `MemoryCandidateSchema`, and added a fold-episode catch-up job (bug 232).
`ac6348c5f` (Phase D) **deleted every one of those modules** —
`memory-processor.ts`, `fold-other-pass.ts`, `fold-episode-pass.ts`,
`fold-window.ts`, the two catch-ups, `extraction-route.ts`,
`handlers/inter-character-memory.ts`, the extraction half of
`cheap-llm-tasks/memory-tasks.ts` — and rebuilt extraction as ONE pipeline under
`lib/memory/extraction/`: one `ExtractionContext` per unit of work, four passes
(`self-turn` / `other-turn` / `other-fold` / `episode-fold`) through ONE pass
runner into admission, fold cursors in a NEW `memory_extraction_cursors` table
(both chats watermark columns DROPPED), a live rate-limit counter, a
`MEMORY_EXTRACTION` payload with `pass` / `cursor` / `ignoreIdle` / `backfill`,
and the `INTER_CHARACTER_MEMORY` job type retired. `93c5af6ad` fixed bug 238 (an
undated fold-OTHER draft dates at the observer's last line). `b045dd986`
(Phase G) moved the rate-limit knobs onto the Extraction Grain card and capped
`maxPerHour` at 10,000. `64743ee13` fixed bug 245: a forward-only `turn` cursor,
"EARLIER IN THIS TURN" context, autonomous speakers bounded by the previous
anchor, and a third bundle cursor key. **The model-visible prompt BODIES (SELF,
OTHER, fold grain block, the five shared blocks) are byte-identical to the
pre-shape** (diff-verified, §1.12); what moves model-visibly is the CLOCK block,
the episode prompt (one line removed, the CLOCK line re-stamped), the turn
transcript's new "EARLIER IN THIS TURN" section, the fold-OTHER cap (now clamped
by the output budget), the episode pass's eligible-message filter, and the OTHER
subject list (rate-limited characters stay subjects).

### 0.1 Corrections to the ledger / brief

- **The brief's "`124c8aca3`'s continue-turn fix (bug 200?)"** — `124c8aca3`
  touches extraction NOT AT ALL. Bug 199 is `buildContext`'s trailing-only recall
  list (recall sibling), bug 200 the gate's orphan widen (admission sibling), bug
  201 the chat wipe (retire sibling). The extraction-side "continue turn" problem
  is bug 245 (`64743ee13`): a continue / nudge / Skip re-closes the SAME turn
  (same opener), the dedupe key only collapses in-flight jobs, so pre-245 every
  re-close re-extracted the whole turn. §3 D13 is its port.
- **"Phase A's fold-episode catch-up job" (bug 232)** does not survive as its own
  module: Phase D replaced it with the cursor-keyed `runFoldCatchup(ctx,
  'episode-fold')` + sweep step 9 (`fold-catchup.ts`). Port only the pin shape.
- **Ledger `ac6348c5f` row: "a per-pass cap `min(base, ceil(maxTokens / 8000))`
  incl. fold OTHER"** — true for SELF / OTHER / fold OTHER; the EPISODE pass does
  NOT go through `passCap` (`passes/episode-fold.ts:48` uses `FOLD_EPISODE_CAP`
  raw) although `parse.ts:39` claims it does. Code wins: the episode cap is 2,
  unclamped.
- **Ledger: "the rate limit … now applies to the episode pass"** — and, unstated,
  the episode pass DEFERS (cursor held) when every present character is at the
  cap (`deferOnRateLimit`, `fold.ts:156`); with only some at the cap it writes for
  the rest.
- **Ledger: "memory `createdAt` = the source message's time on every path"** —
  precisely: the turn passes stamp the SLICE's own last message (pre v5: the
  turn-wide latest assistant time, `memory_extraction_job.rs:339-361`); the
  episode pass stamps the stretch's NEWEST message (pre: `occurredAt`, the EVENT
  time — `fold-episode-pass.ts (pre):144`).
- **`withMemoryTimeZone`** (`prompts/shared.ts:82`) is exported but has NO
  production caller at the pin (the context carries the zone); port as dead or
  skip — do not wire it.
- **v4 checkout moved during this survey:** `main` is now `83334b9a5`
  ("Release checklist 4: API standard audit…"), ONE commit past the pin; it
  touches no file in this slice (`git diff --stat 64743ee13 main --
  lib/memory lib/background-jobs app/api/v1/chats/[id]/actions/memories.ts` is
  empty). Recorded for `/driftcheck`, not this round (the 2026-10-09 standing
  ruling: work follows the pin).

---

## 1. v4 AT THE PIN — `lib/memory/extraction/` and its satellites

File list (line counts at the pin): `catchup.ts` 249, `context.ts` 316,
`cursor-bundle.ts` 124, `cursors.ts` 66, `dry-run.ts` 149, `fold.ts` 217,
`index.ts` 100, `parse.ts` 192, `passes/episode-fold.ts` 118,
`passes/other-fold.ts` 89, `passes/other-turn.ts` 96, `passes/self-turn.ts` 60,
`passes/subjects.ts` 47, `prompts/episode.ts` 52, `prompts/other.ts` 282,
`prompts/self.ts` 221, `prompts/shared.ts` 215, `rate-limit.ts` 86,
`route.ts` 80, `run.ts` 390, `turn.ts` 188, `window.ts` 347; tests
`__tests__/clock-zone.test.ts` 163, `consent-regression.test.ts` 90,
`prompt-tags.test.ts` 198, `self-prompt-user-controlled.test.ts` 197.

### 1.1 `index.ts` (100) — the public surface

Re-exports (`:13-100`): `buildExtractionContext`, `clockAt`,
`pickExtractionConnectionProfileId`, `ExtractionContextError`, types
`BuildExtractionContextOptions` / `CastMember` / `ExtractionContext`;
`resolveExtractionRoute` / `ExtractionRoute`; `createRateLimiter`,
`rateLimitDecision`, `applyImportanceFloor`, `RateLimiter`, `RateLimitDecision`;
`liveCursorStore`, `memoryCursorStore`, `CursorStore`, `ExtractionCursor`;
`runPass`, `emptyPassResult`, `mergePassResult`, `AdmittedCandidate`,
`DraftBatch`, `ExtractionPass`, `ExtractionPassName`, `PassCall`, `PassResult`;
`selfTurnPass`, `otherTurnPass`, `otherFoldPass`, `episodeFoldPass`,
`MAX_FRAGMENT_LINKS`; `runTurnExtraction`, `runTurnPasses`,
`runCarinaExtraction`, `carinaTranscript`, `turnTranscriptFor`,
`TurnExtractionResult`, `TurnLocator`; `runFoldPass`,
`FOLD_MAX_WINDOW_MESSAGES`, `FOLD_MAX_SLICES_PER_JOB`, `FoldResult`,
`RunFoldInput`; `runFoldCatchup`, `findFoldCatchupCandidates`,
`idleChatsForCatchup`, `isChatActiveForCatchup`, `foldEndIndex`,
`isEpisodeBacklogBehind`, `isEpisodeBacklogBehindByAnchors`,
`FOLD_CATCHUP_IDLE_MS`, `FOLD_CATCHUP_LOOKBACK_MS`, `FOLD_CATCHUP_MAX_CHATS`;
`runExtractionDryRun`, `toExtractedCandidate`, `DryRunHooks`,
`ExtractedCandidate`; `HARD_CANDIDATE_CAP`, `EVENT_EXTRA_SLOT`,
`EPISODE_TARGETING`, `passCap`, `capWithEventSlot`, `coerceMemoryDraft`,
`parseSelfDrafts`, `parseOtherDraftsBySubject`, `parseEpisodeDrafts`;
`isFoldEligibleMessage`, `messagesPastCursor`, `foldPassRange`,
`sliceFoldWindow`, `stretchWindow`, `turnWindow`, `renderStretch`,
`renderTurnContext`, `ORDERED_TURN_TRANSCRIPT_HEADING`,
`STRETCH_MESSAGE_CHAR_CAP`; `buildSelfTurnMessages`, `buildOtherTurnMessages`,
`buildOtherFoldMessages`, `buildEpisodeFoldMessages`, `FOLD_EPISODE_CAP`;
`BUNDLE_CURSOR_FIELDS`, `withBundleCursorFields`, `takeBundleCursors`,
`restoreBundleCursors`. `lib/memory/index.ts:36-42` re-exports the run entry
points again.

### 1.2 `context.ts` (316) — one context per unit of work

A "unit of work" (`:5`) = one closed turn, one Carina answer, one fold, one
catch-up job, or one dry run. Built ONCE and handed to every pass.

- `CastMember` (`:39-50`): `{ characterId, participantId: string|null, name,
  pronouns, isUser, present, character }`.
- `ExtractionContext` (`:52-84`): `chat`, `userId`, `clock: MemoryClock &
  {timelineMode}` (`{ timeZone, nowMs: Date.now(), timelineMode: chat.timelineMode
  ?? 'realtime' }`, `:240`), `settings: MemorySettings['extraction']`
  (`{limits, mode}`), `cast: Map<characterId, CastMember>`, `userCharacter`,
  `speakerNames`, `canonForSelf(holderId)` and `canonFor(holderId, aboutId)`
  (MEMOISED per unit, `:233-280`), `project: {id, description}`, `orienting:
  {projectDescription, chatContextSummary}`, `route`, `rateLimit`, `cursors`,
  `dryRun`, `backfill`, `dryRunBook`, `inAutonomousRoom` (`chat.chatType ===
  'autonomous'`), `messages()` (the chat's `type:'message'` rows, read once —
  `loadChatMessageEvents`, lazily, `:295-300`).
- `BuildExtractionContextOptions` (`:86-107`): `dryRun?`, `connectionProfile?`,
  `connectionProfileId?`, `chat?`, `messages?`, `chatContextSummary?` (overrides
  `chat.contextSummary` — the fold passes `newSummary`), `extraCharacterIds?`
  (Carina's answerer), `cursors?`, `backfill?`.
- `ExtractionContextError` (`:109-114`), `reason: 'no-chat' |
  'no-connection-profile'`; messages VERBATIM: `` `Chat not found: ${chatId}` ``
  (`:173`), `` `No connection profile for extraction on chat ${chatId}` `` (`:181`).
- `pickExtractionConnectionProfileId(chat)` (`:117-123`): the first
  `CHARACTER` seat with `controlledBy !== 'user'` and a `connectionProfileId`,
  else ANY seat with one, else null.
- `clockAt(ctx, nowIso)` (`:126-128`) → `{ nowIso, timelineMode, timeZone }`.
- `buildExtractionContext(chatId, userId, opts)` (`:166-316`), in order:
  chat (`opts.chat ?? repos.chats.findById`; absent → throw `no-chat`); profile
  (`opts.connectionProfile` else `findById(opts.connectionProfileId ??
  pickExtractionConnectionProfileId(chat))`; absent → throw
  `no-connection-profile`); in parallel `chatSettings.findByUserId(userId)`
  (NULLABLE — no throw), `connections.findByUserId(userId)`,
  `resolveMemorySettings(userId)`, `refreshMemoryTimeZone()` (`:184-189`); the
  cast — every `CHARACTER` seat with a `characterId`, deduped by characterId,
  hydrated via `characters.findById` (a throw → DEBUG + left out, `:130-144`),
  `isUser = controlledBy === 'user'`, `present = isParticipantPresent(status)`,
  then `extraCharacterIds` not already seated (`participantId: null, isUser:
  false, present: false`); `userCharacter` = the first `CHARACTER` seat with
  `controlledBy === 'user'` and a characterId, looked up in the cast;
  project description (`projects.findById(chat.projectId)?.description`, a throw
  → DEBUG + null, `:147-159`) in parallel with `resolveSpeakerNames(chat)`; the
  route (`resolveExtractionRoute`); the rate limiter — `createRateLimiter(opts.backfill
  ? { ...limits, enabled: false } : limits)` (`:287-289`); cursors —
  `opts.cursors ?? (dryRun ? memoryCursorStore(chatId) : liveCursorStore(chatId))`.
- SELF canon (`:245-261`): `loadCanonForSelfWithCommonplace({ id, name:
  c?.name ?? member.name ?? holderId, manifesto, personality, description,
  identity, mountPointId: c?.characterDocumentMountPointId ?? null })
  .then(renderSelfCanonBlock)`. OTHER canon (`:262-280`, key
  `` `${holderId}\u0000${aboutId}` ``):
  `loadCanonForObserverAboutSubject({ characterId: holderId, mountPointId:
  holder?.characterDocumentMountPointId ?? null }, { id: aboutId, name:
  about?.name ?? aboutId, identity, description })` → `{ block:
  renderOtherCanonBlock(canon), source: canon.source }`. `canon.ts` is UNCHANGED
  across the span (no diff); only the call COUNT moves (once per unit, not per
  call — the `[Canon]` debug lines are emitted once per (holder) / (holder,
  about) per unit of work).
- Logs: DEBUG `'[Memory] Character unavailable to extraction; leaving it out'`
  `{ chatId, characterId, error }` (`:137`); DEBUG `'[Memory] Project description
  unavailable; continuing without it'` `{ chatId, projectId, error }` (`:152`);
  DEBUG `'[Memory] Extraction context built'` `{ chatId, dryRun, backfill, cast:
  cast.size, userCharacterId, projectId, hasProjectDescription, timeZone,
  otherPass: settings.extraction.mode.otherPass, rateLimited:
  settings.extraction.limits.enabled }` (`:303-314` — NB `rateLimited` reads the
  RAW limits, not the backfill-disabled ones). `resolveMemorySettings` logs
  DEBUG `'[MemorySettings] Resolved memory settings'` `{ userId, maintenanceEnabled,
  perCharacterCap, capOverrides, consolidationEnabled, otherPass }`
  (`lib/memory/settings.ts:107-114`) — once per context.

### 1.3 `route.ts` (80) — the cheap-LLM route, once per unit

`resolveExtractionRoute({ chat, connectionProfile, chatSettings,
availableProfiles })` (`:47-80`), never throws:
- `base = getCheapLLMProvider(connectionProfile, buildCheapLLMConfig(chatSettings),
  availableProfiles, false)` — `buildCheapLLMConfig` (`lib/llm/cheap-llm.ts:142-154`)
  = `DEFAULT_CHEAP_LLM_CONFIG` when no settings, else `{ ...DEFAULT,
  strategy, fallbackToLocal, userDefinedProfileId: s.userDefinedProfileId ??
  undefined, defaultCheapProfileId: s.defaultCheapProfileId ?? undefined }`.
  **Pre (`memory-processor.ts (pre):211-216`):** `toCheapLLMConfig` mapped only
  `strategy`, `userDefinedProfileId || undefined`, `fallbackToLocal` — no default
  cheap profile, and `''` dropped.
- `uncensoredRoute = shouldUseUncensoredRoute(chat)`; in a `try`:
  `conciergePolicy = resolveConciergeSettings(chatSettings, chat)`, `cheapLLM =
  resolveUncensoredCheapLLMSelection(base, uncensoredRoute, conciergePolicy,
  availableProfiles)`, `uncensoredFallback = { conciergePolicy, availableProfiles,
  isDangerousChat: uncensoredRoute }` (pre: no `isDangerousChat` key). Catch →
  WARN `'[Memory] Could not resolve the Concierge policy for extraction; running
  without the uncensored fallback'` `{ chatId, error }`, fallback null.
- `maxTokens = resolveMaxTokens(connectionProfile)` — the UNIT's profile.
- DEBUG `'[Memory] Extraction route resolved'` `{ chatId, provider, model,
  uncensoredRoute, hasUncensoredFallback, maxTokens }`.
- `isDangerousChat` matters downstream: `core-execution.ts:431`
  (`if (currentProfile?.isDangerousCompatible && !uncensoredFallback?.isDangerousChat)
  return null` — the empty-answer retry) and `:732` (the llm-log danger flag).

### 1.4 `rate-limit.ts` (86) — a live counter

- `RateLimitDecision` (`:20-23`): `{mode:'allow'} | {mode:'throttle', floor,
  recentCount, cap} | {mode:'skip', recentCount, cap}`.
- `rateLimitDecision(recentCount, limits)` (`:33-41`, pure): disabled → allow;
  `recentCount >= maxPerHour` → skip; `softStart = floor(maxPerHour ×
  softStartFraction)`; `recentCount >= softStart` → throttle(`softFloor`).
- `applyImportanceFloor(drafts, floor)` = `d.importance >= floor`.
- `createRateLimiter(limits, readBaseline = countRecentAdmissions)` (`:52-86`):
  per-character baseline read ONCE per unit (`readBaseline(characterId,
  Date.now() - RATE_LEDGER_WINDOW_MS)`; a rejection → WARN `'[Memory] Rate-limit
  lookup failed; allowing extraction'` `{ characterId, error }`, 0); `decide` =
  baseline + `landed[characterId]`; `record(characterId, inserted = 1)` adds to
  `landed` only when enabled, the baseline has been read, `inserted > 0`.
- The baseline source is ADMISSION's `countRecentAdmissions`
  (`admission/rate-ledger.ts:49-61`, window `RATE_LEDGER_WINDOW_MS = 3_600_000`):
  an in-memory per-character list of AUTO inserts the PARENT saw land
  (`recordAdmissionInsert`, called from `admit.ts:404` only when
  `admissionInserted && row.source === 'AUTO' && !request.backfill`). A restart
  forgets. **Pre (v5 today):** the count was `memories` rows by `createdAt` in the
  last wall-clock hour (`memory_processor.rs:633-670` →
  `memories_read::count_created_since`).

### 1.5 `cursors.ts` (66) + the cursor schema + repository (the CONTRACT)

- `lib/schemas/memory-extraction-cursor.types.ts` (48): `ExtractionCursorPassEnum
  = z.enum(['other-fold', 'episode-fold', 'turn'])` (`:22`; `'turn'` added by
  `64743ee13`); `FoldCursorPass = Exclude<ExtractionCursorPass, 'turn'>` (`:26`);
  `EXTRACTION_CURSOR_PASSES` (`:28`); `MemoryExtractionCursorSchema = { id:
  z.string().min(1), chatId: UUIDSchema, pass, messageId: z.string().min(1),
  createdAt, updatedAt }` (`:30-39` — `chatId` is a UUID: fixtures need UUID chat
  ids); `extractionCursorId(chatId, pass) = `${chatId}:${pass}`` (`:43-45`);
  `ExtractionCursorMap = Partial<Record<ExtractionCursorPass, string>>`.
- `lib/database/repositories/memory-extraction-cursors.repository.ts` (151):
  reads THROW on failure (`safeQuery` with no fallback — the header `:9-11`: a
  failed read taken for "no cursor" would re-read history and make the sweep
  enqueue every idle chat): `findByChatId(chatId) → ExtractionCursorMap`,
  `findCursor(chatId, pass) → string|null`, `findAllByChat() → Map<chatId, map>`;
  writes: `setCursor(chatId, pass, messageId)` — ONE upsert (`:110-114`):
  `INSERT INTO "memory_extraction_cursors" ("id","chatId","pass","messageId","createdAt","updatedAt")
  VALUES (?,?,?,?,?,?) ON CONFLICT("id") DO UPDATE SET "messageId" =
  excluded."messageId", "updatedAt" = excluded."updatedAt"` + DEBUG
  `'[ExtractionCursors] Cursor set'` `{ chatId, pass, messageId }`;
  `clearCursor(chatId, pass) → boolean` (fallback false; DEBUG `'[ExtractionCursors]
  Cursor cleared'` `{ chatId, pass, removed }`); `clearForChat(chatId) → number`
  (fallback 0). safeQuery messages: `'Error finding extraction cursors for chat'`,
  `'Error finding extraction cursor'`, `'Error listing extraction cursors'`,
  `'Error setting extraction cursor'`, `'Error clearing extraction cursor'`,
  `'Error clearing extraction cursors for chat'`.
- `cursors.ts`: `ExtractionCursor { chatId, pass, afterMessageId: string|null }`;
  `CursorStore { read(pass), advance(pass, messageId), clear(pass) }`;
  `liveCursorStore(chatId)` (`:33-47`, `advance` → `setCursor` + DEBUG
  `'[ExtractionCursors] Advanced'` `{ chatId, pass, messageId }` — so a live
  advance logs TWO debug lines); `memoryCursorStore(chatId, initial = {})`
  (`:50-66`, the dry run's, + `snapshot()`).

### 1.6 `run.ts` (390) — THE pass runner (the extraction → admission seam)

- `ExtractionPassName = 'self-turn' | 'other-turn' | 'other-fold' | 'episode-fold'` (`:26`).
- `DraftBatch` (`:33-44`): `{ holders: CastMember[], about: CastMember|null,
  drafts: MemoryDraft[], floor?: {value, reason}|null, source: { messageId,
  createdAt }, anchors: { referenceIso, fallbackIso } }`.
- `PassCall` (`:47-57`): `{ label, holders, messages: LLMMessage[],
  logCharacterId?, parse(raw) → DraftBatch[] }`.
- `ExtractionPass` (`:59-69`): `{ name, candidatePass, taskType, calls(ctx,
  window), afterBatch?(ctx, window, admitted) }`.
- `AdmittedCandidate` (`:72-79`): `{ candidate, occurredAt, narrativeTime,
  holderName, aboutName, outcome: AdmissionOutcome }`.
- `PassResult` (`:81-99`): `{ pass, calls, timedOut, failed, admitted,
  createdIds, reinforcedIds, usage{promptTokens, completionTokens, totalTokens},
  debugLogs, rateDeferred }`; `emptyPassResult`, `mergePassResult` (sums,
  concatenations, `rateDeferred ||=`).
- `runPass(pass, ctx, window, { deferOnRateLimit? })` (`:158-249`):
  1. `calls = await pass.calls(ctx, window)`.
  2. `deferOnRateLimit` (folds only): for each call with holders, if EVERY
     holder's `decide` is `skip` → `rateDeferred = true`, debug line
     `` `[Memory] ${call.label} deferred — every holder is at the hourly cap; the window is left unread` ``,
     INFO `'[Memory] Pass deferred by the rate limit; window left unread'`
     `{ chatId, pass, call }`, RETURN (no call made).
  3. Per call: decide each holder, push `describeLimit` line —
     `` `[Memory] SKIPPED extraction for ${name} — rate limit reached (${recentCount}/${cap} memories in last hour)` ``
     or `` `[Memory] THROTTLING ${name} — ${recentCount}/${cap}; floor ${floor}` ``
     (**per CALL**, so a throttled character's line appears once for its SELF
     call AND once for its OTHER call — pre once per turn). All holders skip →
     DEBUG `'[Memory] Call skipped: every holder is rate-limited'` `{ chatId, pass,
     call }`, next call.
  4. `calls++`; `executeCheapLLMTask(ctx.route.cheapLLM, call.messages, ctx.userId,
     raw => call.parse(raw), pass.taskType, ctx.chat.id, undefined,
     ctx.route.uncensoredFallback ?? undefined, ctx.route.maxTokens,
     call.logCharacterId)` (`:199-210`); usage summed.
  5. Failure: `timedOut++` or `failed++`; debug
     `` `[Memory] ${call.label} ${timedOut ? 'was LOST to a timeout, not refused' : 'failed'}: ${error}` ``;
     WARN `'[Memory] Extraction call failed'` `{ chatId, pass, call, timedOut, error }`.
  6. Success: per batch `admitBatch`, then `pass.afterBatch(ctx, window, admitted)`
     when any admitted.
  7. DEBUG `'[Memory] Pass complete'` `{ chatId, pass, dryRun, calls, timedOut,
     candidates, created, reinforced }`.
- `admitBatch` (`:251-294`): the batch floor first (hybrid OTHER) — debug
  `` `[Memory] ${floor.reason} dropped ${n} candidate(s) (${callLabel}${aboutName ? ` about ${aboutName}` : ''}) below importance ${floor.value}` ``;
  then per holder: skip → nothing; throttle → floor again, debug
  `` `[Memory] Throttle dropped ${n} candidate(s) for ${holder.name}${aboutName ? ` about ${aboutName}` : ''} below importance ${rl.floor}` ``;
  label = about ? `` `${callLabel} — ${holder.name} about ${about.name}` `` : holders > 1 ?
  `` `${callLabel} — for ${holder.name}` `` : callLabel; debug
  `` `[Memory] ${label}: ${kept.length} candidate(s)` ``; each draft → `admitDraft`.
- `admitDraft` (`:296-390`), the SEAM:
  `anchorTimeMs = anchorTimeMsFrom([referenceIso, fallbackIso])`; `anchorIso =
  firstValidIso(referenceIso, fallbackIso) ?? new Date(anchorTimeMs).toISOString()`;
  `fallbackIso = firstValidIso(batch.anchors.fallbackIso) ?? anchorIso`;
  `resolveEpisodicAnchors({ when: draft.when, referenceIso: anchorIso, fallbackIso,
  timelineMode, narrativeTime: draft.narrativeTime, timeZone: ctx.clock.timeZone })`
  (time sibling); `candidateFromDraft(draft, { holderId, aboutId }, { source:
  'AUTO', witnessedContext: inAutonomousRoom ? 'autonomous_room' :
  'user_present', chatId, projectId: ctx.project.id, sourceMessageId,
  anchorTimeMs, pass: pass.candidatePass })` (candidate sibling) — null → debug
  `` `[Memory] ${label} — skipped a draft with no usable text` ``;
  `createdAt = firstValidIso(batch.source.createdAt) ?? undefined`;
  `admitCandidate(candidate, { occurredAt, narrativeTime, sourceMessageTimestamp:
  createdAt }, { userId, dryRun?, backfill?, dryRunBook? })` in a `try` — a throw
  costs THAT candidate only: WARN `'[Memory] Admission failed for one candidate;
  continuing'` `{ chatId, pass, holderId, error }` + debug
  `` `[Memory] ${label} — admission failed (${message}): ${candidate.summary}` ``.
  Then: `admissionInserted(outcome)` → `ctx.rateLimit.record(holder)` and (live)
  `createdIds.push`; `reinforce` or `promote-and-reinforce` with `withFootnotes`
  (live) → `reinforcedIds.push` (an `absorb` is NOT counted); debug line = dry run
  `` `[Memory] DRY-RUN ${label} — would ${action.kind}: ${candidate.summary}` `` else
  `describeAdmission(outcome, label, candidate.summary)` (admission's; its bytes:
  `` `[Memory] ${head} — ${summary}${neighbours ? ` · ${notes.join('; ')}` : ''}` `` with
  heads `Created ${label}` / `Created ${label} (linked to ${n} related, ${reason})` /
  `REINFORCED ${label}: ${id} (count ${n}, +${k} details)` / `REINFORCED ${label}:
  ${id} (promoted from cold, count ${n})` / `SKIPPED near-duplicate ${label}:
  absorbed into ${id} (similarity ${c.toFixed(3)}[, via ${from}])` / `SKIPPED
  ${label} — embedding generation failed: ${note}` / `SKIPPED ${label} — no usable
  text` — `admission/describe.ts:18-51`).

### 1.7 The four passes (`passes/*`, 410 lines)

**`self-turn.ts`** (`selfTurnPass(candidatePass = 'self-turn')`, name
`'self-turn'`, taskType `'memory-extraction-self'`): turn windows only; `cap =
passCap(HARD_CANDIDATE_CAP, maxTokens)`; ONE call per `transcript.characterSlices`
entry whose holder is in the cast (`buildSelfTurnMessages` null → skipped);
`sourceMessageId = slice.contributingMessageIds.at(-1) ??
transcript.latestAssistantMessageId`; `sourceTime =
firstValidIso(slice.lastMessageCreatedAt, transcript.turnTimestamp)`; label
`` `SELF memory for ${holder.name}${slice.isUserControlled ? ' (user-controlled)' : ''}` ``;
batch `{ holders:[holder], about:null, drafts: parseSelfDrafts(raw, cap), source:
{ messageId, createdAt: sourceTime }, anchors: { referenceIso: sourceTime ??
window.nowIso, fallbackIso: sourceTime ?? window.nowIso } }`. **SELF covers
`characterSlices` only — with a bound (bug 245) that is the speakers with NEW lines.**

**`other-turn.ts`** (`otherTurnPass()`, name/candidatePass `'other-turn'`, taskType
`'memory-extraction-other'`): `mode.otherPass === 'fold'` → no calls; `hybrid` →
batch floor `{ value: perTurnOtherFloor, reason: 'Hybrid floor (the rest is left
to the fold-grain pass)' }`; `cap = passCap(HARD_CANDIDATE_CAP, maxTokens)`.
Bug 245's observer rules (`:36-58`): `allSlices = [...priorSlices,
...characterSlices]`, observers deduped by `` `${isUserControlled?'user':'ai'}:${characterId}` ``
(the LAST slice per key wins — `Map.set` overwrites), speakers = their cast
members, `pool = subjectPool(ctx, speakers)`, `wholeTurn =
withPriorAsMaterial(transcript)`. Per observer: `sawEarlierPart` (in
`priorSlices`) and NO other character has new lines → skip; view = `sawEarlierPart
? transcript : wholeTurn`; `subjects = subjectInputsFor(ctx, observer, pool)`,
empty → skip; `ownNew = newSpeakerIds.has(observer) ? slice : null`;
`sourceMessageId = ownNew?.contributingMessageIds.at(-1) ??
latestAssistantMessageId`; `sourceTime = firstValidIso(ownNew?.lastMessageCreatedAt
?? null, turnTimestamp)`; label `` `OTHER memory ${observer.name}${user ? ' (user-controlled)' : ''} → ${subjects.length} subject(s)` ``;
parse `parseOtherDraftsBySubject(raw, subjectIds, cap)` → one batch per subject
(`about: s.member`, the floor, the same source / anchors).

**`subjects.ts`**: `subjectPool(ctx, speakers)` = speakers then
`ctx.userCharacter`, deduped by characterId, speakers first;
`subjectInputsFor(ctx, observer, pool)` — every pool member but the observer,
each `{ member, input: { id, name, pronouns, isUser, canonBlock }, canonSource }`
via `ctx.canonFor`. **Rate-limited characters are NOT removed from the pool**
(pre removed them, `memory-processor.ts (pre):447-470`).

**`other-fold.ts`** (`otherFoldPass()`, name `'other-fold'`, taskType
`'memory-extraction-other'`): stretch windows only; `cap =
passCap(mode.foldCandidatesPerSubject, maxTokens)` (pre: `max(1, n)`, NOT
clamped); `stretchSpeakers(ctx, messages)` (`:22-38`) — cast members with a
`participantId`, matched by `m.participantId`, in order of first line, each one's
LAST message; `pool = subjectPool(...)`; `speakers.length === 0 || pool.length <
2` → no calls; per speaker: subjects (empty → skip), `last =
lastMessage.get(observer) ?? window.messages.at(-1)`, `sourceTime =
firstValidIso(last?.createdAt)`; label `` `fold OTHER memory ${observer.name} → ${n} subject(s)` ``;
`buildOtherFoldMessages({ stretch: window.rendered, observer, subjects, cap,
inAutonomousRoom, orienting, clock: clockAt(ctx, window.nowIso) })`; batches
`source: { messageId: last?.id ?? null, createdAt: sourceTime }`, `anchors: {
referenceIso: sourceTime ?? window.nowIso, fallbackIso: sourceTime ??
window.nowIso }` — **bug 238 (`93c5af6ad`)**: the fallback was `window.nowIso`.

**`episode-fold.ts`** (`episodeFoldPass()`, name/candidatePass `'episode-fold'`,
taskType `'fold-episode-extraction'`; `MAX_FRAGMENT_LINKS = 8`): stretch windows;
holders = cast members with a `participantId` AND `present` (`:39`); none → no
call; ONE call, label `'episode memory'`, NO `logCharacterId`;
`sourceMessageId = lastStamped?.id ?? messages.at(-1)?.id ?? null`; parse →
`parseEpisodeDrafts(raw, FOLD_EPISODE_CAP)` → ONE batch PER EPISODE `{ holders,
about: null, drafts:[draft], source: { messageId, createdAt: window.nowIso },
anchors: { referenceIso: window.nowIso, fallbackIso: window.startIso } }`.
`afterBatch` (`:59-85`): only INSERTED outcomes are this run's episodes (a
reinforce / absorb names an existing row — "review of PR 94"); their ids join
`writtenEpisodeIds` (the pass instance remembers them for the whole run); dry run
stops there; per episode `linkFragments(holderId, memory.id,
memory.relatedMemoryIds, windowMessageIds, writtenEpisodeIds)` in a `try` —
WARN `'[Memory] Failed to link episode fragments'` `{ chatId, characterId,
episodeId, error }`. `linkFragments` (`:95-118`): `memories.findByCharacterAndSourceMessageIds(characterId,
windowMessageIds)` minus written episodes and itself, `.slice(0, 8)`;
`newlyLinked` = fragments not already naming the episode; `linkTo` = fragment ids
not in the episode's links OR newly linked; `linkBothWays(characterId,
episodeId, linkTo)` (admission's delta writer, bug 231); DEBUG `'[Memory] Episode
fragments linked'` `{ characterId, episodeId, fragments, newlyLinked }`.

### 1.8 `turn.ts` (188) — the turn grain + bug 245's cursor

- `TurnLocator { turnOpenerMessageId, extractionAnchorMessageId?,
  extractAfterMessageId? }` (`:24-33`); `TurnExtractionResult = PassResult & {
  sourceMessageId (= latestAssistantMessageId), hadContributions
  (characterSlices.length > 0) }`.
- `turnTranscriptFor(ctx, locator)` (`:48-58`) → `buildTurnTranscript(await
  ctx.messages(), chat.participants, castCharacters, { turnOpenerMessageId,
  extractionAnchorMessageId ?? null, extractAfterMessageId ?? null,
  userCharacterId/Name/Pronouns from ctx.userCharacter })`.
- `runTurnPasses(ctx, transcript, { candidatePass? })` (`:61-92`): no
  contributions → debug `'[Memory] Turn has no character contributions — nothing to
  extract'`, return; `runPass(selfTurnPass(candidatePass ?? 'self-turn'))`;
  `otherPass === 'fold'` → debug `'[Memory] OTHER pass runs at fold grain only
  (otherPass=fold); skipping per-turn OTHER'` else `runPass(otherTurnPass())`;
  DEBUG `'[Memory] Turn extraction complete'` `{ chatId, turnOpenerMessageId,
  dryRun, candidates, created, reinforced, timedOut }`.
- `liveTurnBound(ctx)` (`:101-104`): dry run or backfill → null; else
  `cursors.read('turn').afterMessageId`.
- `advanceTurnCursor(ctx, messageId)` (`:111-121`): dry run or null → no-op;
  `current === messageId` → no-op; when `current` is set and
  `indexOf(current) > indexOf(messageId)` (chat order; a missing `current` is
  -1) → no-op (FORWARD-ONLY — a regenerate's turns and an autonomous room's
  speakers commit out of order); else `cursors.advance('turn', messageId)`.
- `runTurnExtraction(ctx, locator)` (`:128-143`): bound =
  `locator.extractAfterMessageId ?? liveTurnBound(ctx)`; transcript; when the
  bound is set and `priorSlices` is non-empty, DEBUG `'[Memory] Turn read past
  lines already extracted'` `{ chatId, turnOpenerMessageId, extractAfterMessageId,
  priorSpeakers, newSpeakers }`; `runTurnPasses`; `advanceTurnCursor(ctx,
  transcript.latestAssistantMessageId)` — ALWAYS (also after a no-contribution
  turn, also for a backfill).
- `carinaTranscript(ctx, message, answererId)` (`:150-172`): answerer not in
  cast or empty trimmed answer → null; `{ turnOpenerMessageId: null,
  userMessage: question.trim() || null, userCharacterId: undefined, ...,
  characterSlices: [{ characterId, characterName, characterPronouns, text:
  answer (trimmed), contributingMessageIds: [message.id], lastMessageCreatedAt:
  message.createdAt ?? null }], latestAssistantMessageId: message.id,
  turnTimestamp: message.createdAt ?? null }`.
- `runCarinaExtraction(ctx, message, answererId)` (`:179-188`):
  `runTurnPasses({ ...ctx, userCharacter: null }, transcript, { candidatePass:
  'carina' })` — the OTHER pass runs but finds no subject (the pool is the
  answerer alone); in `fold` mode its debug line still lands.

### 1.9 `window.ts` (347) — windows, renderers, the fold rules

- `TranscriptWindow` (`:46-64`): `{ kind:'turn', transcript, nowIso }` |
  `{ kind:'stretch', messages, rendered: StretchMessage[], nowIso, startIso,
  coverThroughMessageId }`. `STRETCH_MESSAGE_CHAR_CAP = 1500`.
- `firstValidIso(...isos)` — first truthy that `Date.parse`s finite, else null.
- `stretchWindow(messages, speakerNames, coverThroughMessageId, nowMs)`
  (`:77-98`): speaker via `speakerLabel(m, speakerNames)`; `nowIso` = newest
  stamped `createdAt` else now; `startIso` = oldest stamped else `nowIso`.
- `turnWindow(transcript, nowMs)` → `nowIso: transcript.turnTimestamp ?? now`.
- `renderStretch` (`:106-116`): `` `${stamp}${m.speaker}: ${content}` ``, stamp
  `` `[${createdAt.slice(0, 16).replace('T', ' ')}] ` `` (absent when falsy),
  content cut at 1500 + `…`, joined `'\n\n'` — byte-identical to v5's
  `render_fold_window` (`memory_tasks.rs:988-1014`).
- `ORDERED_TURN_TRANSCRIPT_HEADING` (`:123-124`, unchanged) and **NEW**
  `PRIOR_TURN_CONTEXT_HEADING` (`:130-131`) VERBATIM:
  `EARLIER IN THIS TURN (already noted — context only; form no memory from these lines, only from the TURN TRANSCRIPT below):`
- `renderTurnContext(transcript)` (`:138-223`) — the roster is built from
  `[...priorSlices, ...characterSlices]` deduped by `user|ai:characterId` (first
  occurrence wins, so a prior speaker's pronouns label it); USER line / CHARACTER
  lines as pre; the standalone opener `"${userLabel} says:\n\"${userMessage}\""`
  only when `userMessage !== null && !hasUserSlice` (`hasUserSlice` over the
  ROSTER, prior included); body = material slices; heading ORDERED when
  `userMessage !== null || hasUserSlice` else `'TURN TRANSCRIPT:'`; the earlier
  block:
  ```
  `${roster}

  ${earlier}${heading}

  ${sections}`   where earlier = priorSlices.length > 0
      ? `${PRIOR_TURN_CONTEXT_HEADING}\n\n${priorSlices.map(renderSlice).join('\n\n')}\n\n`
      : ''
  ```
  With no prior slices the output is byte-identical to pre. ⚠ For a PLAIN human
  (no user character) with a bound inside the turn, the opener is NOT a slice, so
  `transcript.userMessage` still renders as a standalone section UNDER the turn
  heading (material), not under EARLIER — v4's shape, port it as-is (§9 O8).
- Fold rules: `isFoldEligibleMessage(m)` (`:230-236`) — `type === 'message'`,
  role USER|ASSISTANT, no `systemSender`, a `participantId`, non-blank string
  content; `messagesPastCursor(all, cursor)` (missing / vanished cursor → all);
  `windowPastCursor` (cut a window by chat order; unknown ids kept — `?? Infinity`);
  `sliceFoldWindow(eligible, maxPerSlice, finalCoverThroughId)` (`:288-306` —
  every slice but the last covers through its own last message, the last through
  `finalCoverThroughId ?? its last`); `foldPassRange(window, all, cursor)`
  (`:327-347`) — no window / no cursor / cursor not in chat → the window,
  `applied:false`; window end not in `all` → `windowPastCursor` fallback; end at or
  before cursor → `[]`; else `all.slice(cursorIdx + 1, endIdx + 1)` (READS THE GAP
  before the window), `gap` = rows not in the window.

### 1.10 `fold.ts` (217) — the fold runner

`FOLD_MAX_WINDOW_MESSAGES = 60`, `FOLD_MAX_SLICES_PER_JOB = 4`;
`RunFoldInput { windowMessages, coverThroughMessageId?, maxSlices? }`;
`FoldResult = PassResult & { skippedReason?, slices, slicesDeferred,
cursorAdvancedTo }`. `runFoldPass(ctx, passName: FoldCursorPass, input)`
(`:90-217`), best-effort (never throws):
1. `other-fold` and `otherPass === 'turn'` → DEBUG `'[Memory] otherPass=turn;
   fold-grain OTHER pass not run'` `{ chatId }`, skip `'mode-turn'`.
2. Empty window → `'empty-window'`.
3. Cursor read (a THROW lands in the outer catch → `'error'`); when set,
   `foldPassRange`; applied → DEBUG `'[Memory] Fold window cut to what lies past
   the cursor'` `{ chatId, pass, cursor, before, after, gap }`; empty →
   `'behind-cursor'`.
4. `coverThrough = input.coverThroughMessageId ?? window.at(-1).id`; eligible =
   filter; `allSlices = sliceFoldWindow(eligible, 60, coverThrough)`; `slices =
   allSlices.slice(0, maxSlices ?? 4)`; deferred count → INFO `'[Memory] Fold
   backlog wider than one run; the rest waits for the next fold or catch-up'`
   `{ chatId, pass, slicesRead, slicesDeferred }`.
5. No slices → `'nothing-to-observe'` AND the cursor ADVANCES to `coverThrough`.
6. Per slice: `runPass(pass, ctx, stretchWindow(slice.messages, speakerNames,
   slice.coverThroughMessageId), { deferOnRateLimit: true })`; `rateDeferred` →
   `'rate-limited'`, `slicesDeferred += remaining`, INFO `'[Memory] Fold slice
   deferred by the rate limit; cursor not advanced past it'` `{ chatId, pass,
   slice, slices }`, break; else `slices++`; `timedOut > 0` → WARN `'[Memory] Fold
   slice lost to a timeout; cursor not advanced past it'` `{ chatId, pass, slice,
   slices }`, break; else `advance(pass, slice.coverThroughMessageId)` (a throw →
   WARN `'[Memory] Failed to advance extraction cursor'` `{ chatId, pass, error }`).
7. All read slices made no call and none timed out → `'nobody-present'`
   (episode) / `'nothing-to-observe'` (other).
8. INFO `'[Memory] Fold pass complete'` `{ chatId, pass, dryRun, slices, calls,
   candidates, created, reinforced, cursorAdvancedTo }`.
9. Catch → WARN `'[Memory] Fold pass failed (non-fatal)'` `{ chatId, pass,
   slicesRead, cursorAdvancedTo, error }`, `'error'`, `slicesDeferred += unread`.

The episode pass is ONE pass instance per run (`passFor` once, `:148`) so
`writtenEpisodeIds` spans every slice.

### 1.11 `catchup.ts` (249) — idle catch-ups for both fold passes

Constants unchanged from pre: `FOLD_CATCHUP_IDLE_MS = 2h`, `LOOKBACK_MS = 30d`,
`MAX_CHATS = 50`. `idleChatsForCatchup(chats, now)` — not help-like
(`isHelpLikeChatType`), `messageCount >= 2`, finite `lastMessageAt` with idle >
2h and < 30d, most recent first. `isChatActiveForCatchup` (≤ 2h). `foldEndIndex`
(last index in `messages` whose id is in `summaryAnchorMessageIds`, else -1).
`isEpisodeBacklogBehind(messages, anchors, cursor)` (vanished cursor counts as
behind). `isEpisodeBacklogBehindByAnchors(anchors, cursor)` — `{ behind:
anchors.lastIndexOf(cursor) < anchors.length - 1, foldEndMessageId: anchors.at(-1) }`.
`findFoldCatchupCandidates(pass, { now?, limit?, chats?, cursors? })`
(`:133-205`): `other-fold` reads `getMemoryExtractionModeSettings()`; `turn` →
DEBUG `'[FoldCatchup] otherPass=turn; no OTHER catch-up needed'` (no fields),
empty; chats = `options.chats ?? findAll()`; cursors = `options.cursors ??
memoryExtractionCursors.findAllByChat()`; episode prefilter = has anchors; per
chat: episode → behind-by-anchors, `anchorMessageId = foldEndMessageId`; other →
load messages (a throw → WARN `'[FoldCatchup] Could not inspect chat; skipping'`
`{ chatId, error }`), last message equal to the cursor → skip, `anchorMessageId =
last.id`; beyond `limit` → `deferred++` (other: counted WITHOUT reading messages);
no seat profile → DEBUG `'[FoldCatchup] No connection profile on any seat;
skipping chat'` `{ chatId, pass }`; DEBUG `'[FoldCatchup] Selection complete'`
`{ pass, scanned, idle, selected, deferred }`. `runFoldCatchup(ctx, pass, {
ignoreIdle?, now?, maxSlices? })` (`:215-249`): active and not `ignoreIdle` →
DEBUG `'[FoldCatchup] Chat active again since the sweep; leaving it to the fold'`
`{ chatId, pass }`, `'not-idle'`; other → `messagesPastCursor` (empty →
`'behind-cursor'`), DEBUG `'[FoldCatchup] Running OTHER catch-up'` `{ chatId,
windowMessages, cursor }`, `runFoldPass(ctx, pass, { windowMessages,
coverThroughMessageId: messages.at(-1).id, maxSlices })`; episode → not behind →
`'never-folded'` / `'behind-cursor'`, DEBUG `'[FoldCatchup] Running episode
catch-up'` `{ chatId, foldedMessages, cursor, foldEnd }`, `runFoldPass(ctx, pass,
{ windowMessages: messages.slice(0, endIndex + 1), maxSlices })`.

### 1.12 Prompts (`prompts/*`, 770) — model-visible text

**Diff-verified unchanged from `4a38222e6:lib/memory/cheap-llm-tasks/memory-tasks.ts`**
(extracted function bodies `diff`-ed byte for byte): `selfBodyForCap`,
`otherBodyForCap` (incl. the `${grainBlock}` prefix rule), `FOLD_GRAIN_OTHER_BLOCK`,
`ORIENTING_CONTEXT_SKIP_BULLET`, `EVENT_INSTRUCTION_BLOCK`,
`AGREEMENTS_INSTRUCTION_BLOCK`, `TAGS_INSTRUCTION_BLOCK`,
`AUTONOMOUS_ROOM_USER_ABSENCE_CLAUSE`, `FIRST_PERSON_USER_CLAUSE`,
`renderOrientingContext` (`PROJECT:` / `STORY SO FAR:` lines, 1500-char cut,
header `ORIENTING CONTEXT — background only, never a source of memories`), the
SELF / OTHER system-prompt assembly
(`${preamble}${body}\n\n${clockBlock}${orientingBlock}CONTEXT\nSUBJECT|OBSERVER: …`),
the OTHER subjects block (`SUBJECT ${i+1}: ${label}${isUser ? ' (the
user-controlled character)' : ''}\n${canonBlock}` joined `'\n\n'`), the fold
OTHER user message `` `CONVERSATION STRETCH (${n} messages, in the order spoken):\n\n${renderStretch(stretch)}` ``.
v5's generated `memory_tasks/prompt_text.rs` constants (`SELF_BODY_*`,
`OTHER_BODY_*`, `FOLD_GRAIN_OTHER_BLOCK`) stay byte-correct; only the generator's
source file moves (§5).

**Changed (model-visible):**

1. `renderClockStamp(clock)` (`shared.ts:90-95`, bug 212 / `c2795733f`):
   `` `${zonedDateStamp(ms, tz)} (${WEEKDAYS[zonedParts(ms, tz).weekday]}), ${clock.nowIso}` ``,
   `tz = clock.timeZone ?? getMemoryTimeZone()`; a non-finite `nowIso` → the raw
   string. Pre: `` `${nowIso.slice(0, 10)} (${WEEKDAYS[getUTCDay()]}), ${nowIso}` ``.
2. `renderClockBlock(clock)` (`shared.ts:98-110`) — lines `CLOCK`,
   `` `Current date/time: ${stamp}` ``, `` `Timeline mode: ${timelineMode}` ``,
   `Resolve relative time phrases against this clock; emit "when" as an absolute date (YYYY-MM-DD) whenever you can.`,
   joined `'\n'` + `'\n\n'`. The pre `Current in-story time: …` line is GONE
   (`e0ac7493a`; `ExtractionClock.narrativeNow` removed; `timeZone?` added).
3. The EPISODE prompt (`prompts/episode.ts:16-39`) at the pin, VERBATIM — the
   pre `  participants  names of those involved` line between `entities` and
   `importance` is REMOVED (`e0ac7493a`); everything else identical:
   ```
   You are consolidating a batch of roleplay conversation turns into EPISODE records — coherent, dated accounts of specific things that happened.

   An episode is a real occurrence at a specific time and/or place: an outing, a visit, an arrival, a completed undertaking, a notable incident. It is NOT a standing fact, an opinion, a mood, or an ongoing thread — only something that happened.

   Read the dated turns below. Return 0–${FOLD_EPISODE_CAP} episodes. Most windows contain none — return [] freely. Only emit an episode when the turns actually depict or recount a specific occurrence worth remembering as an event.

   For each episode:
     narrative     2–3 sentences, past tense, third person, using participant
                   names. The prose must ITSELF name the place and the time
                   ("On July 14th, Amy and Charlie visited Lighthouse Point
                   and bought the brass sextant…").
     summary       3–8 words, lowercase, no punctuation
     when          when it happened: an absolute date (YYYY-MM-DD, resolved
                   against the message timestamps and the CLOCK line) whenever
                   possible, otherwise the phrase as stated
     narrativeTime the in-story time phrase, only when the story runs on a
                   fictional timeline ("the third night at sea")
     entities      2–6 proper nouns: places, people, named things
     importance    0.20–1.00 (0.9 = a day the participants will retell for
                   years; 0.5 = a pleasant but ordinary outing)

   An episode records what was said and done, not what was agreed: attribute proposals and conditions to their speaker, and record an agreement only where the agreeing party's own assent appears in the window.

   Return a JSON array only. No prose, no code fences. If nothing qualifies, return [].
   ```
4. The episode USER message (`episode.ts:42-52`):
   `` `${clockLine}\n\nDATED TURNS:\n\n${renderStretch(stretch)}` `` with
   `` clockLine = `CLOCK: current date/time ${renderClockStamp(clock)}; timeline mode ${clock.timelineMode}` ``
   (pre: `current date/time ${clock.nowIso}` and an optional `; current in-story
   time …` suffix). **Every episode call's bytes move**, even in UTC.
5. The turn transcript's `EARLIER IN THIS TURN` section and roster (§1.9).
6. The SELF / OTHER `${maxMemories}` / `${perSubjectCap}` numbers follow
   `passCap` — for SELF / per-turn OTHER identical to pre's
   `resolveMaxMemories`; for fold OTHER now `min(foldCandidatesPerSubject,
   ceil(maxTokens / 8000))` (a 4,096-token profile states `up to 1` where pre
   stated `up to 3`).

`buildSelfTurnMessages(input)` (`self.ts:202-221`) — null when the holder has no
slice in `characterSlices`; label `formatNameWithPronouns(name, pronouns)`;
first-person clause from `target.isUserControlled`; user message
`renderTurnContext(transcript)`. `buildOtherTurnMessages` /
`buildOtherFoldMessages` (`other.ts:258-282`) take `OtherPromptInput { observer:
{name, pronouns}, subjects, cap, inAutonomousRoom, orienting?, clock? }`.

### 1.13 `parse.ts` (192) — drafts

`HARD_CANDIDATE_CAP = 2`, `EVENT_EXTRA_SLOT = 1`, `TOKENS_PER_CANDIDATE_SLOT =
8000`, `DRAFT_ENTITY_CAP = 8`. `passCap(base, maxTokens)` =
`max(1, min(max(1, floor(base)), ceil((maxTokens ?? 8000) / 8000)))`.
`capWithEventSlot(drafts, cap)` (`isAnchoredEvent`: `kind === 'episodic' &&
(when || entities.length)`). `parseJsonItems(raw)` — `JSON.parse(stripCodeFences(raw))`,
array or `[parsed]`, a parse error → null. `coerceMemoryDraft(item)` (`:88-109`):
content / summary (string, else `JSON.stringify` of a truthy value), keywords
(strings only, untrimmed), `importance: coerceImportance(item.importance)`
(finite number clamped [0,1], else 0.5 — candidate.ts), `targeting:
normaliseTargeting(item)`, `kind` (`'episodic'` else `'semantic'`), `when`
(trimmed non-blank), `entities` (array → trimmed non-blank strings ≤ 8);
null when `!content && !summary`. `parseSelfDrafts` — a non-object element
(incl. `null`) is SKIPPED (pre: a `null` element threw inside the whole-function
`try` → the entire answer became `[]`). `parseOtherDraftsBySubject(raw,
subjectIds, cap)` — integer 1-based `subjectIndex`, per-bucket collect cap `cap +
1`, then `capWithEventSlot` on every bucket. `EPISODE_TARGETING = { temporal:
'past', scope: 'narrow', context: 'history' }`; `parseEpisodeDrafts(raw, cap)`:
narrative trimmed non-blank, summary trimmed else `narrative.slice(0, 60)`,
entities ≤ 8, `keywords = entities.map(toLowerCase)`, importance finite → clamp
[0.2, 1] else 0.6 (pre: `typeof === 'number'` without the finite check),
`targeting: EPISODE_TARGETING`, `kind: 'episodic'`, `when`, `narrativeTime`
(trimmed). Rows keep their keyword bytes: `toAdmissionRow` appends
`[temporal, 'scope: '+scope, context]` after the free words (candidate.ts:266-268)
— identical to pre's `applyTargetingTags` and the episode pass's literal.

### 1.14 `dry-run.ts` (149) — the live pipeline, writing nothing

`ExtractedCandidate` (`:30-52`): `{ pass: 'SELF'|'OTHER', extractionPass,
decision, decisionTargetId, characterId, characterName, aboutCharacterId,
aboutCharacterName, sourceMessageId, content, summary, keywords, importance,
kind, when, occurredAt, narrativeTime, entities }` — `toExtractedCandidate`
(`:54-78`, content / summary / keywords from `toAdmissionRow`).
`runExtractionDryRun(ctx, { concurrency?, hooks? })` (`:95-149`): `!ctx.dryRun`
→ throw `'runExtractionDryRun needs a dry-run extraction context'`; dry-run book
from `previewChatWipe(chatId)` (retire sibling; a throw → WARN `'[Memory] Dry run
could not preview the chat wipe; deciding against current rows'` `{ chatId, error
}`); `listExtractionTurns(messages, chatType)`; `concurrency ?? 4` workers over
`runTurnExtraction` (a throw → WARN `'[Memory] Dry-run turn failed'` `{ chatId,
index, error }` + `onTurnError`); then `runFoldCatchup(ctx, 'other-fold' |
'episode-fold', { ignoreIdle: true, maxSlices: Infinity })`; INFO `'[Memory]
Extraction dry run complete'` `{ chatId, turns, candidates }`.

### 1.15 `cursor-bundle.ts` (124) — cursors in bundles (contract for the carriers sibling)

Logger: `logger.child({ module: 'memory:extraction:cursor-bundle' })` (memory
note: child bindings are invisible to the oracle capture). `BUNDLE_CURSOR_FIELDS
= { 'other-fold': 'otherExtractionWatermarkMessageId', 'episode-fold':
'episodeExtractionWatermarkMessageId', turn: 'turnExtractionWatermarkMessageId' }`
(`:29-33`, ENTRIES order = emission order). `cursorsToBundleFields(map)` —
absent passes OMITTED. `loadBundleCursors()` — WARN `'[ExtractionCursors] Could
not read the cursor table for a bundle; reading per chat'` `{ error }` → null.
`withBundleCursorFields(chat, preloaded?)` — `{ ...chat, ...fields }` (fields
APPENDED after the chat's keys); per-chat read failure WARN `"[ExtractionCursors]
Could not read a chat's cursors for a bundle; leaving them out"` `{ chatId, error }`.
`takeBundleCursors(chat)` strips all three keys, keeps non-empty strings.
`restoreBundleCursors(chatId, cursors)` — per-pass `setCursor`, WARN
`'[ExtractionCursors] Could not restore a bundled cursor; the pass will re-read'`
`{ chatId, pass, error }`; DEBUG `'[ExtractionCursors] Restored bundled cursors'`
`{ chatId, written }`. Callers: `backup-service.ts:194/204`, `restore.ts:265/274`,
`export/ndjson-writer.ts:349/382` (preloads only for > 1 chat),
`import-entities.ts:419/431`.

### 1.16 Satellites

**`lib/services/chat-message/turn-transcript.ts`** (bug 245 + Phase D): NEW
`loadChatMessageEvents(chatId)` (the `type:'message'` rows); NEW
`findAutonomousTurnAnchorMessageId(messages)` (latest ASSISTANT, no
`systemSender`, not `isSilentMessage`, with `participantId`) — the trigger's old
inline loop, unchanged semantics; NEW `ExtractionTurnLocator`; NEW
`PRIOR_CONTEXT_MAX_MESSAGES = 8`; NEW `listExtractionTurns(messages, chatType)`
— autonomous: one turn per anchor line, `extractAfterMessageId` = the previous
anchor (first: null); else a null-opener GREETING turn when any anchor-like line
precedes the first non-staff USER message, then one per non-staff USER message;
NEW `withPriorAsMaterial(transcript)` (prior slices folded back into their keyed
slices, prior first); `TurnTranscript.priorSlices?`; `buildTurnTranscript` gains
`extractAfterMessageId`: lines collected as before, then `boundIndex =
lines.findIndex(id === bound)`; material = after it; prior = up-to-and-incl. it,
`.slice(-8)`; the opener's user slice goes to PRIOR when bound ≥ 0, else leads the
material; `priorSlices` key only when non-empty. `latestAssistantMessageId` /
`turnTimestamp` still cover the WHOLE turn.

**`lib/background-jobs/queue-service.ts`** — `MemoryExtractionPayload`
(`:55-112`): `chatId`, `turnOpenerMessageId: string|null`,
`extractionAnchorMessageId?`, `extractAfterMessageId?` (NEW, 245),
`connectionProfileId`, `pass?: 'turn'|'other-fold'|'episode-fold'`, `cursor?`,
`ignoreIdle?`, `backfill?`, deprecated `foldOtherCatchup?` /
`foldOtherIgnoreIdle?` / `foldEpisodeCatchup?`. `memoryExtractionPassOf(p)`
(`:118-123`): `p.pass` if TRUTHY (any string passes through unvalidated),
else `foldEpisodeCatchup` truthy → episode, `foldOtherCatchup` truthy → other,
else `'turn'`. `enqueueMemoryExtraction(userId, payload, options?)` (`:555-591`):
`skipDedupCheck` → straight `enqueueJob`; else PENDING + PROCESSING jobs of the
user, match `type === 'MEMORY_EXTRACTION' && chatId === && turnOpenerMessageId ===
(STRICT) && (anchor ?? null) === && passOf === passOf && (cursor ?? null) ===`;
lookup failure → WARN `'[MemoryExtraction] Failed to check for existing jobs
during enqueue'` `{ error }`, enqueue anyway. `enqueueMemoryExtractionBatch`
(`:1291-1345`): entries `string|null|{turnOpenerMessageId,
extractionAnchorMessageId?, extractAfterMessageId?}`; payload `{ chatId,
turnOpenerMessageId, extractionAnchorMessageId, ...(extractAfterMessageId ? {
extractAfterMessageId } : {}), connectionProfileId, backfill: true }` (NEW
`backfill`); INFO `'Memory extraction batch enqueued'` `{ chatId, jobCount }`.

**`handlers/memory-extraction.ts`** (195): `finishExtractionJob(job, ctx,
result, { taskLabel, debugMessageId? })` (`:45-96`) — `timedOut > 0` → ERROR
`'[MemoryExtraction] Extraction passes lost to a cheap-LLM timeout; failing the job
for retry'` `{ jobId, chatId, task: taskLabel, passesLostToTimeout }` and THROW
`CheapLLMTaskLostError(taskLabel, `${timedOut} pass(es) timed out`)` → lastError
`` `Cheap LLM task "${taskLabel}" timed out and was not retried successfully: ${n} pass(es) timed out` ``;
debug logs → `chats.updateMessage(chatId, debugMessageId, { debugMemoryLogs:
withRecallLines(existing, lines) })` (recall sibling's helper keeps
`RECALL_DEBUG_PREFIX` / `RECALL_RING_PREFIX` lines FIRST), failure WARN
`'[MemoryExtraction] Failed to store debug logs'` `{ jobId, error }`; usage → cost
event on `ctx.route.connectionProfile` (WARN `'[MemoryExtraction] Failed to emit
token tracking event'` `{ jobId, error }`). `extractionContextForJob(job, chatId,
profileId, extra)` (`:102-119`) — profile missing → throw `` `Connection profile
not found: ${id}` ``; `no-chat` → WARN `'[MemoryExtraction] Chat not found at
job execution; skipping'` `{ jobId, chatId }`, null; other context errors
rethrow. `handleMemoryExtraction` (`:121-195`): context with `backfill:
payload.backfill === true`; non-turn pass → `ignoreIdle = ignoreIdle === true ||
foldOtherIgnoreIdle === true`, `runFoldCatchup`, INFO `'[MemoryExtraction] Fold
catch-up processed'` `{ jobId, chatId, pass, skippedReason ?? null, slices,
created, reinforced, cursorAdvancedTo, slicesDeferred }`, `finishExtractionJob(…,
{ taskLabel: `${pass}-catchup` })` (NO debug message), then when `slicesDeferred
> 0 && cursorAdvancedTo && skippedReason !== 'rate-limited'` →
`enqueueMemoryExtraction(job.userId, { chatId, turnOpenerMessageId: null,
extractionAnchorMessageId: null, connectionProfileId, pass, cursor:
cursorAdvancedTo, ...(ignoreIdle ? {ignoreIdle:true} : {}), ...(backfill ?
{backfill:true} : {}) })` + DEBUG `'[MemoryExtraction] Fold catch-up continues
in a follow-up job'` `{ jobId, chatId, pass, from, slicesDeferred }`; turn →
`runTurnExtraction(ctx, { opener, anchor ?? null, extractAfter ?? null })`;
`!hadContributions` → INFO `'[MemoryExtraction] Turn has no character
contributions; skipping'` `{ jobId, chatId, turnOpenerMessageId }`, return (no
finish); INFO `'[MemoryExtraction] Turn processed'` `{ jobId, chatId,
turnOpenerMessageId, extractionAnchorMessageId ?? null, candidates, created,
reinforced }`; finish with `taskLabel: 'memory-extraction'`, `debugMessageId:
result.sourceMessageId`. **Gone:** the `Chat settings not found for user: …`
throw and the `'[MemoryExtraction] Processing did not succeed'` WARN.

**`handlers/carina-memory-extraction.ts`** (74): context with
`extraCharacterIds: [answererId]`; answerer not in cast → WARN
`'[CarinaMemoryExtraction] Answerer character not found; skipping'` `{ jobId,
chatId, answererId }`; message not found → WARN `'[CarinaMemoryExtraction] Carina
message not found; skipping'` `{ jobId, chatId, carinaMessageId }`; null result →
INFO `'[CarinaMemoryExtraction] Carina message has no answer text; skipping'`
`{ jobId, chatId, carinaMessageId }`; INFO `'[CarinaMemoryExtraction] Reference
answer processed'` `{ jobId, chatId, carinaMessageId, answererId, projectId,
created, reinforced }`; finish `taskLabel: 'carina-memory-extraction'`,
`debugMessageId: result.sourceMessageId`. The chat-missing WARN is now
`[MemoryExtraction]`'s; the lost-timeout ERROR is `[MemoryExtraction]`'s with
`task: 'carina-memory-extraction'`.

**`handlers/memory-regenerate-chat.ts`** (117): chat missing →
`retireChatMemories` + INFO `'[MemoryRegenerateChat] Chat no longer exists; deleted
orphan memories'` `{ jobId, chatId, deleted, spared, vectorsRemoved }`; profile
missing → throw `` `Connection profile not found for memory regeneration: ${id}` ``;
`retireChatMemories(chatId)`; `memoryExtractionCursors.clearForChat` + DEBUG
`'[MemoryRegenerateChat] Cleared extraction cursors'` `{ jobId, chatId,
clearedCursors }`; per `listExtractionTurns` turn `enqueueMemoryExtraction(…, {
chatId, turnOpenerMessageId, extractionAnchorMessageId ?? null,
...(extractAfterMessageId ? {…} : {}), connectionProfileId, backfill: true }, {
skipDedupCheck: true })`; catch-ups `otherPass === 'turn' ? ['episode-fold'] :
['other-fold', 'episode-fold']`, each `{ chatId, turnOpenerMessageId: null,
extractionAnchorMessageId: `regenerate:${job.id}`, connectionProfileId, pass,
ignoreIdle: true, backfill: true }` with `skipDedupCheck`; INFO
`'[MemoryRegenerateChat] Wiped chat and enqueued its rebuild'` `{ jobId, chatId,
chatType, deleted, spared, vectorsRemoved, turnJobs, catchups }`.

**`maintenance/fold-catchup.ts`** (121, replaces `fold-other-catchup.ts`, 71
(pre)) — `logger.child({ module: 'maintenance.fold-catchup' })`;
`enqueueFoldCatchups(pass, { now?, limit?, chats?, cursors? })`: selection;
`chatsWithCatchupInFlight(pass, userIds)` — chats with a PENDING / PROCESSING
`MEMORY_EXTRACTION` of the same `passOf` (bug 232: one chain per chat and pass);
read failure → WARN `'Could not read in-flight jobs; relying on the enqueue
dedupe'` `{ pass, error }`; in-flight → DEBUG `'Catch-up already queued or
running for chat; not starting another'` `{ pass, chatId }`; enqueue `{ chatId,
turnOpenerMessageId: null, extractionAnchorMessageId: anchorMessageId,
connectionProfileId, pass }` (DEDUPED), DEBUG `'Enqueued fold catch-up'` `{ pass,
chatId, anchor }`, failure WARN `'Failed to enqueue fold catch-up'` `{ pass,
chatId, error }`; tail INFO `'Fold catch-up capped; remaining chats wait for the
next sweep'` `{ pass, enqueued, alreadyRunning, deferred, limit }` or DEBUG `'Fold
catch-up sweep done'` `{ pass, enqueued, alreadyRunning }`; returns `{ enqueued,
deferred }`. **`scheduled-maintenance.ts`** steps 8–9 (`:267-291`): one
`chats.findAll()` + `memoryExtractionCursors.findAllByChat()` shared (failure WARN
`'Could not list chats for the fold catch-ups; each will read its own'` `{ error
}`); `runSweep(summary, 'fold-other-catchup', 'Fold-grain OTHER catch-up failed —
continuing', …)` and `runSweep(summary, 'fold-episode-catchup', 'Fold episode
catch-up failed — continuing', …)`; summary gains `foldEpisodeCatchup: {
enqueued, deferred }` (and the TF-IDF sibling's `builtinRefit`); the closing
`moduleLogger.info('Scheduled maintenance pass complete', summary)`.

**`lib/chat/context-summary.ts`** fold hook (`:552-574`): `foldTranscript` =
all `type:'message'` rows; `foldWindowMessages = turnsToFold.flatMap(t =>
t.messages)` (the fold's turns already exclude non-USER/ASSISTANT and staff rows,
`:265-287`); ONE `buildExtractionContext(chatId, userId, { chat, messages:
foldTranscript, connectionProfile, chatContextSummary: newSummary })`, then
`runFoldPass(ctx, 'episode-fold', { windowMessages })` then `runFoldPass(ctx,
'other-fold', { windowMessages })`; one catch: ERROR `'[Context Summary] Fold
memory passes failed:'` `{ chatId }` + error (replacing the two pre lines `Fold
episode pass failed:` / `Fold OTHER pass failed:`). Not backfill — the rate limit
applies to the fold.

**Turn-end trigger** (`memory-trigger.service.ts:58-91`) — behaviour UNCHANGED
(the inline anchor walk became `findAutonomousTurnAnchorMessageId`; the
`MemoryChatSettings` type loses `conciergePolicy` / `isDangerousChat`). Callers:
`message-finalizer.service.ts:533`, the Courier resolve route
(`messages/[messageId]/route.ts:199`), `help-chat/orchestrator.service.ts:655`;
Brahma never calls it. The swipe (`regenerate-swipe.service.ts:342-348`) RETIRES
the swiped message's memories (`retireMemories(ids, { reason: 'swipe-regenerate'
})`, retire sibling) and enqueues no extraction itself (the re-finalize triggers).

**`app/api/v1/chats/[id]/actions/memories.ts`** (275): `resolveExtractionProfileId(chat,
ctx)` (`:36-54`) — `pickExtractionConnectionProfileId(chat)`, else
`cheapLLMSettings.defaultCheapProfileId`, else (`strategy === 'USER_DEFINED'`)
`userDefinedProfileId`, each only when the profile exists with `userId ===
user.id`. `handleQueueMemories` (`:65-113`): no profile → WARN `'[Chats v1] No
connection profile for memory extraction'` `{ userId, chatId }` + 400 `'No
connection profile available for memory extraction. Give a seat a profile or set
a cheap LLM profile in settings.'` (pre: WARN `'[Chats v1] No valid cheap LLM
configured'` + `'No valid cheap LLM configured. Please set a cheap LLM profile in
settings.'`); `listExtractionTurns`; empty → 400 autonomous `'No assistant messages
found in this chat — nothing to extract memories from.'` / else `'No messages found
in this chat — nothing to extract memories from.'` (pre `'No user messages
found…'`); INFO `'[Chats v1] Queueing per-turn memory extraction jobs'` `{ chatId,
chatType ?? null, turnCount }`; batch `{ priority: 0 }`; `{ success, jobCount,
chatId, turnCount }`. `handleExtractMemoriesDryRun` (`:136-275`) — NDJSON
`start` / `candidate` / `turn` / `turn-error` / `fold` / `done` / `fatal` / `ping`
(5 s heartbeat), `?concurrency` (`'concurrency must be a positive integer'`, cap
32), context `{ chat, connectionProfileId, dryRun: true, backfill: true }`
(`ExtractionContextError` → 400 with its message), INFO `'[Chats v1] Streaming
dry-run memory extraction'` `{ chatId, concurrency, connectionProfileId, projectId }`,
DEBUG `'[Chats v1] Dry-run stream finished'` `{ chatId, turnCount, candidates }`,
ERROR `'[Chats v1] Dry-run stream failed'` `{ chatId }`.

**Settings** — `memoryExtractionLimits` lives ONLY in `instance_settings`
(`lib/instance-settings/index.ts:42,189-199`; Phase A's
`move-memory-maintenance-settings-v1` DROPS the `chat_settings` column of the
same name and the `memoryExtractionConcurrency` column + instance key);
`MemoryExtractionLimitsSchema` (`settings.types.ts:315-324`): `enabled` false,
`maxPerHour` `int().positive().max(10_000)` default 20 (`.max` NEW in
`b045dd986`), `softStartFraction` [0,1] 0.7, `softFloor` [0,1] 0.7.
`MemoryExtractionModeSettingsSchema` (`:294-301`, unchanged): `otherPass`
`'turn'|'fold'|'hybrid'` default `'hybrid'`, `perTurnOtherFloor` [0,1] 0.75,
`foldCandidatesPerSubject` `int().positive()` 3. Routes
(`app/api/v1/memories/route.ts`): `extraction-limits-config` GET / POST and
`extraction-mode-config` GET / POST now parse through `patchSchemaOf(...)`
(`:173`, `:179` — bug 244: omitted keys stay omitted; pre the limits schema was
hand-written `int().min(1).max(10000)` and the mode schema `.partial()` — which
CLOBBERED omitted keys with defaults); `extraction-concurrency` GET / POST
REMOVED (unknown action → 400), the processor's no-op
`setMemoryExtractionConcurrencyOverride` deleted (`processor.ts (pre):63-80`).
The SPA's Extraction Grain card (`components/tools/memory-extraction-grain-card.tsx`
+ NEW `memory-extraction-limits-section.tsx`, 97) carries the limits (Phase G);
pre there was NO limits UI at all.

---

## 2. What v4 DELETED in the span that v5 still carries

| v4 (pre) module / export | deleted by | v5 twin on `a7c6f0d03` |
|---|---|---|
| `lib/memory/memory-processor.ts` (779) — `processTurnForMemory`, `TurnMemoryExtractionContext`, `writeCandidate`, `CandidateWriteContext`, `resolveExtractionRateLimit`, `applyImportanceFloor`, `toCheapLLMConfig` | `ac6348c5f` | `services/memory_processor.rs` (1,667): canon loaders `:80-330` (STAY — §4), extract fns `:332-480`, `MemoryExtractionLimits` `:482-489`, `CheapLlmSettings` / `to_cheap_llm_config` `:493-515`, `TurnMemoryExtractionContext` `:517-554`, `ExtractedCandidate` `:556-576`, result `:588-630`, `RateLimitDecision` / `resolve_extraction_rate_limit` `:620-670`, `apply_importance_floor` `:672-682`, `RunState` / `WriteArgs` / `CandidateWriteContext` `:684-767`, `resolve_candidate_anchors` `:769-793`, `write_candidate` `:795-947`, `resolve_other_pass_mode` `:969-995`, `process_turn_for_memory` `:997-1035`, `run_passes` `:1037-1543` |
| `cheap-llm-tasks/memory-tasks.ts` extraction half — `extractSelfMemoriesFromTurn`, `extractOtherMemoriesFromTurn`, `extractOtherMemoriesFromFold`, `extractEpisodesFromFold`, `parseMemoryCandidateArray`, `parseOtherCandidatesBySubject`, `parseFoldEpisodes`, `coerceMemoryCandidate`, `applyTargetingTags`, `resolveMaxMemories`, `capCandidates`, `FoldEpisode`, `FoldEpisodeMessage`, `FoldOtherMessage`, `MemoryCandidate` type | `ac6348c5f` (the non-extraction half moved to `recall/query.ts` + `recall/recap-summary.ts`, recall sibling) | `memory_tasks.rs` (1,240): clock `:81-136`, `resolve_max_memories` `:138-143`, `MemoryCandidate` `:149-178`, prompt assembly `:228-366`, transcript structs `:370-409`, `render_turn_context` `:418-512`, builders `:514-590`, `strip_code_fences` `:592` (shared — KEEP), `apply_targeting_tags` `:618`, `coerce_memory_candidate` `:668-756`, parsers `:757-853`, fold episode `:855-1051`, fold OTHER builder `:1053-1086`; `memory_tasks/prompt_text.rs` (411, generated); the memory processor's async wrappers `memory_processor.rs:332-480` |
| `lib/memory/fold-other-pass.ts` (634) — `runFoldOtherPass`, `runFoldOtherCatchup`, `findFoldOtherCatchupCandidates`, `isFoldOtherEligibleMessage`, `messagesPastWatermark` | `ac6348c5f` | `services/fold_other_pass.rs` (1,194): consts `:100-111`, inputs `:116-160`, eligibility `:186-203`, `messages_past_watermark` `:205-226`, `run_fold_other_pass` `:239-706`, `advance_watermark` `:707-746`, sweep `enqueue_fold_other_catchups` `:748-905`, selection `:906-1033`, `run_fold_other_catchup` `:1034-1194` |
| `lib/memory/fold-episode-pass.ts` (224) — `runFoldEpisodePass` | `ac6348c5f` | `services/fold_episode_pass.rs` (490): `MAX_FRAGMENT_LINKS` `:77`, `FoldWindowMessage` `:82-93`, input / result `:95-111`, `run_fold_episode_pass` `:141-327`, `write_episode_for_character` `:328-490` |
| `lib/memory/fold-window.ts` (bug 206/207, `c2795733f`) | `ac6348c5f` | NONE (v5 never ported `c2795733f`) — v5 still TRIMS to the newest 60 and advances past the rest (`fold_other_pass.rs:345-358`) |
| `maintenance/fold-other-catchup.ts` (71) — `enqueueFoldOtherCatchups` | `ac6348c5f` | inside `fold_other_pass.rs:748-905`; sweep step 8 `scheduled_maintenance.rs:367-381` |
| Phase A's fold-episode catch-up job (`continue-episode:<wm>`) | `ac6348c5f` | NONE |
| `lib/memory/extraction-route.ts` (`c2795733f`) | `ac6348c5f` | NONE (v5 resolves the route per call site: `memory_processor.rs:1056-1083`, `context_summary.rs:313-366`, `memory_extraction_job.rs` / `carina_memory_extraction.rs` build the inputs) |
| `handlers/inter-character-memory.ts` — `handleInterCharacterMemory` (an empty drain), type `INTER_CHARACTER_MEMORY` | `ac6348c5f` (+ migration `drop-inter-character-memory-jobs-v1`) | `job_runner.rs:144` (`KNOWN_JOB_TYPES`), `api/system_data.rs:159-161` (estimate), `:182` (`'Character Memory'`), `:639` (`JOB_TYPES`), `api/memories.rs:1589` (regenerate-all clear list), `realtime/job_topics.rs:98`, `services/activity_kinds.rs:83`. (Never registered as a handler in v5 — the loud fallback answers.) |
| `chats.otherExtractionWatermarkMessageId` (column) | `ac6348c5f` (migration) | readers / writers: `db/chats_read.rs:96,322`, `db/chats.rs:417,886,999`, `db/chats.rs:1735-1750` (`set_other_extraction_watermark`), `fold_other_pass.rs:307,725,1005,1084`, `backup/uuid_remap.rs:829`, `provisioning/mod.rs:798` (test), fixtures `enclave/lifecycle.rs:1031`, `enclave/step.rs:1546`, `almanack/phase3_ledgers.rs:1192`; the P4.D265 ensure `db/memory_tiers_repair.rs:105` (keystone — RULED retired) |
| `MemoryChatSettings.conciergePolicy` / `.isDangerousChat` | `ac6348c5f` | cosmetic — v5's trigger takes ids only (`message_finalizer.rs:1500-1504`) |
| `toCheapLLMConfig` (no default cheap profile) | `ac6348c5f` | `memory_processor.rs:493-515` + `memory_extraction_job.rs:505-521` + `carina_memory_extraction.rs:368` (three copies of the settings decode) |
| `extraction-concurrency` route + `get/setMemoryExtractionConcurrency` + `setMemoryExtractionConcurrencyOverride` | `e0ac7493a` | `api/memories.rs:1476-1505`, `api/types.rs:1724-1729` (`MemoryExtractionConcurrencyGet` / `Set`), `db/instance_settings.rs:527` (+ its getter / setter) |
| `chat_settings.memoryExtractionLimits` column | `e0ac7493a` | `db/chat_settings.rs:1055,1119,1409,1500` + schema dumps (keystone / settings sibling) |

---

## 3. Behaviour deltas a port must hit (NUMBERED)

Context and route

**D1 — One extraction context per unit of work.** v4 `context.ts:166-316`. v5
builds the equivalent inline four times (`memory_extraction_job.rs:232-396`,
`carina_memory_extraction.rs:62-243`, `context_summary.rs:291-366` +
`:1004-1098`, `fold_other_pass.rs:278-706`). Port ONE `build_extraction_context`:
cast (seat dedupe by characterId, hydrate once, `present`, `isUser`, extra
characters), `userCharacter`, project description for EVERY unit (Carina — bug
222; the fold hook — v5 passes `project_description: None`,
`context_summary.rs:358`), speaker names, settings, zone, route, limiter,
cursors, lazy messages. The chat-settings row becomes OPTIONAL: drop the `Chat
settings not found for user: …` throw (`memory_extraction_job.rs:146-150`,
Carina's twin `carina_memory_extraction.rs:94`). Canon memoised per unit (`[Canon]` debug lines once per
(holder) / (holder, about) per unit — a tier-3 log comparand).

**D2 — The route.** v4 `route.ts:47-80` + `cheap-llm.ts:142-154`. v5
`to_cheap_llm_config` (`memory_processor.rs:499-515`) sets
`default_cheap_profile_id: None` and filters `''`; port `buildCheapLLMConfig`
(default cheap profile honoured; `?? undefined` keeps `''` — measure what
`get_cheap_llm_provider` does with `Some("")`). The fallback carries
`is_dangerous_chat: Some(should_use_uncensored_route(chat))` (v5 passes `None`,
`memory_processor.rs:1079`, `context_summary.rs:341`), which re-arms
`cheap_llm_exec.rs:125` and `:1434`. EVERY pass incl. the EPISODE pass gets the
fallback and `maxTokens` (bug 213; v5's episode call has neither,
`fold_episode_pass.rs` → `executor` with defaults). The fold passes resolve their
OWN route from the context — not the fold's `selection` (`context_summary.rs:1055`,
`:1097`). Log lines §1.3.

**D3 — Settings read.** `resolveMemorySettings` (one DEBUG line per context);
limits from the instance key (v5's host already reads it: `spine.rs:3284-3288`;
the core takes it as a parameter — fold it into the context); backfill DISABLES
the limiter.

Passes, parse, admission seam

**D4 — Pass caps.** `passCap(base, maxTokens)` for SELF / per-turn OTHER
(= v5 `resolve_max_memories`, `memory_tasks.rs:138-143`, same values) and fold
OTHER (`passCap(foldCandidatesPerSubject, maxTokens)` — v5
`build_fold_other_extraction_messages` uses `per_subject_cap.max(1)`,
`memory_tasks.rs:1053-1086`; model-visible). Episode: raw 2.

**D5 — Parse → `MemoryDraft` → `candidateFromDraft`.** v4 `parse.ts`,
`candidate.ts:219-241`. v5 `coerce_memory_candidate` (`memory_tasks.rs:668-756`)
+ `write_candidate`'s `if v == 0.0 { 0.5 }` (`memory_processor.rs:846-853`):
importance clamped [0,1] by the parser and a 0 STAYS 0; whitespace-only content /
summary falls back to the other; both blank → the "no usable text" debug line;
a `null` element in a SELF array is skipped (pre / v5: the whole answer `[]` —
v5's `Result<…, ()>` arm); targeting carried apart and appended by
`toAdmissionRow` (same row bytes); episode importance requires `isFinite`.

**D6 — The subject pool.** v4 `subjects.ts`, `other-turn.ts:36-58`. v5 builds
subjects from ALLOWED slices only (`memory_processor.rs:1161-1222`): a
rate-limit-SKIPPED character must now remain a SUBJECT of others' OTHER calls
(model-visible), and the `[Memory] Skipped duplicate user subject for …` debug
line (`:1195-1198`) is gone.

**D7 — The live rate limit.** v4 `rate-limit.ts` + `admission/rate-ledger.ts`.
v5 counts `memories.createdAt` rows (`memory_processor.rs:633-670`) — replace
with admission's process-global ledger (§7 seam) + the per-unit `landed`
counter; `decide` per holder per CALL (lines §1.6 step 3 — two THROTTLING lines
per throttled speaker per turn); skip = no call for that holder (others in a
multi-holder call still admitted); folds DEFER (`rateDeferred`, cursor held,
`'rate-limited'`, no continuation job); the EPISODE pass is now limited.

**D8 — Debug-log text (persisted on `debugMemoryLogs` → a tier-2 comparand).**
Pre (v5) → pin: `SELF for X[ (user-controlled)]: N candidate(s)` → `SELF memory for
X[ (user-controlled)]: N candidate(s)`; `OTHER observer=X[ …] subject=Y canon=SRC:
N candidate(s)` → `OTHER memory X[ …] → K subject(s) — X about Y: N candidate(s)`
(canon source NO LONGER in the line); `SELF extraction for X was LOST to a timeout,
not refused: E` + `SELF extraction failed for X: E` → ONE line `SELF memory for X
was LOST to a timeout, not refused: E` (or `… failed: E`); `OTHER extraction
{was LOST to a timeout|failed} (X → K subject(s)): E` → `OTHER memory X → K
subject(s) {was LOST…|failed}: E`; `Throttle dropped N SELF|OTHER candidate(s)
for X[ about Y] …` → no `SELF`/`OTHER` word; `Hybrid floor dropped N OTHER
candidate(s) for X about Y below importance F (left to the fold-grain pass)` →
`Hybrid floor (the rest is left to the fold-grain pass) dropped N candidate(s)
(OTHER memory X → K subject(s) about Y) below importance F`; `DRY-RUN L — would
write: S` → `DRY-RUN L — would <kind>: S`; the gate's five outcome lines
(`memory_processor.rs:893-946`) → `describeAdmission`'s (§1.6); NEW lines
`L — skipped a draft with no usable text`, `L — admission failed (M): S`, `C
deferred — every holder is at the hourly cap; the window is left unread`. The
debug lines now also come from the FOLD passes (the catch-up job does not persist
them — `finishExtractionJob` with no `debugMessageId`).

**D9 — The admission seam replaces `create_memory_with_gate`.** v4
`run.ts:296-390`. v5 `write_candidate` (`memory_processor.rs:795-947`) calls the
gate and `?`-propagates a `DbError`, which aborts the WHOLE turn
(`process_turn_for_memory` → `success: false`). Pin: a per-candidate
try/catch (WARN + debug line), the run continues. `createdIds` = inserted
(`insert` / `insert-linked`); `reinforcedIds` = `reinforce` +
`promote-and-reinforce` with footnotes (NOT `absorb`). `witnessedContext`,
`chatId`, `projectId`, `sourceMessageId`, `anchorTimeMs`, `pass` provenance.

**D10 — Anchors and timestamps.** v4 `run.ts:305-335`, passes' `source` /
`anchors`. v5's anchor chain (`memory_processor.rs:810-818`: slice →
`ctx.source_message_timestamp` (the turn-wide latest assistant time) →
`turn_timestamp` → now) and the gate's `sourceMessageTimestamp:
ctx.source_message_timestamp` (`:869`) → pin: `referenceIso = fallbackIso =
slice.lastMessageCreatedAt ?? turnTimestamp ?? window.nowIso`; the row's
`createdAt` (`sourceMessageTimestamp`) = the SLICE's own last message time (or,
for an OTHER observer with no new line, `turnTimestamp`); `when` resolved in
`ctx.clock.timeZone` (time sibling's `resolveEpisodicAnchors`; realtime keeps an
EXPLICIT `narrativeTime` — only episode drafts carry one). Episode: ref =
newest, fallback = oldest, `createdAt` = newest (pre / v5: `occurredAt`).

**D11 — The CLOCK.** v4 `shared.ts:90-110`, `episode.ts:46-47`. v5
`render_clock_block` (`memory_tasks.rs:104-136`): zoned date + weekday, drop the
in-story line (dead in v5 too — `narrative_now: None` everywhere); carry
`time_zone` on `ExtractionClock` (`memory_tasks.rs:81-89`); the episode CLOCK line
gains the stamp (`memory_tasks.rs:1016-1051`); the turn clock instant becomes
`transcript.turnTimestamp ?? now` (v5 `ctx.source_message_timestamp ??
turn_timestamp`, `:1096-1110`; Carina: `message.createdAt ?? now`, v5
`carina createdAt ?? chat.createdAt`).

Turn grain (bug 245)

**D12 — The `turn` cursor.** v4 `turn.ts:101-143`. NEW in v5: live jobs read the
`turn` cursor as the bound (dry run / backfill never read it); ALWAYS advance it
forward-only to `latestAssistantMessageId` after the passes (dry run never). A
stale / foreign bound is ignored by the builder.

**D13 — The bounded transcript.** v4 `turn-transcript.ts` (§1.16), `window.ts:130-223`,
`other-turn.ts:36-66`. v5 `build_turn_transcript` (`turn_transcript.rs:106-286`)
+ `TurnTranscript` (`memory_tasks.rs:391-409`) + `render_turn_context`
(`:418-512`): `extract_after_message_id`, `prior_slices`, `PRIOR_CONTEXT_MAX_MESSAGES
= 8`, opener to prior, the EARLIER section + combined roster, SELF only for
material slices, the OTHER observer rules (`sawEarlierPart`, `withPriorAsMaterial`
view, the source for an observer with no new line), `listExtractionTurns`,
`findAutonomousTurnAnchorMessageId` (v5's inline walk `message_finalizer.rs:1510-1539`
is equivalent — hoist it).

**D14 — Carina.** v4 `turn.ts:150-188` + the handler. v5
`carina_memory_extraction.rs`: provenance `pass: 'carina'` (candidate sibling's
`CandidatePass::Carina`), project id + description + summary from the context
(`:215-217` are `None`), the log re-homing (§1.16), `lastError` text
(`task "carina-memory-extraction"`, `"1 pass(es) timed out"`), the `Processing
did not succeed` WARN gone, the chat-settings throw gone, the slice's
`lastMessageCreatedAt` = `message.createdAt` (v5 `None`, `:186`).

Fold grain

**D15 — The cursor table replaces the watermark column.** v4 `cursors.ts` +
repository. v5 reads / writes `chats.otherExtractionWatermarkMessageId`
(§2 row). Every read through `CursorStore` (`read` THROWS on failure — v5's
column read never fails separately); every advance through `set_cursor`
(upsert); the episode pass gains a cursor (v5 has none — bug 207's whole point).

**D16 — The fold runner.** v4 `fold.ts`. v5's fold OTHER window logic
(`fold_other_pass.rs:303-358`): `foldPassRange` reads the GAP before the window
(v5 cuts the window only), skip reason `'behind-cursor'` (v5
`'behind-watermark'`), slices of 60 with per-slice cursor advance (v5 trims to
the newest 60 and advances the watermark past ALL — bug 206), ≤ 4 slices per run
(dry run ∞), a timed-out slice stops the run, `'nothing-to-observe'` still
advances, `'nobody-present'` for the episode, the eight log lines (§1.10). Both
passes now go through it (the episode pass had no window logic at all).

**D17 — Fold OTHER specifics.** v4 `other-fold.ts`. Bug 238 fallback; speakers
from the cast by `participantId`; the PROJECT line in the orienting footer (v5
`None`); label `fold OTHER memory X → K subject(s)`; the candidate pass
`'other-fold'`; the `[FoldOtherPass]` lines (`fold_other_pass.rs:264-701`) are
ALL replaced by `[Memory]` lines (none survives verbatim).

**D18 — Episode specifics.** v4 `episode-fold.ts`, `parse.ts:166-192`,
`episode.ts`. v5 `fold_episode_pass.rs:141-490`: holders = present CAST members
(characterId-deduped, hydrated only — v5 iterates raw participants); the
eligibility filter (rows without `participantId` or with blank content no longer
reach the prompt — model-visible; measure whether plain-human USER rows carry a
`participantId`, §9 O6); slicing + cursor; one candidate per (episode, holder)
through admission (rate limit, dry run, `pass: 'episode-fold'`); fragment links
via admission's `link_both_ways`, only for INSERTED episodes, never to a sibling
episode of the same run (v5 links after ANY outcome with a memory and writes
whole arrays, `:328-490`); the prompt (participants line, CLOCK stamp); the
`[FoldEpisodePass]` lines (`:240,296,306`) → `[Memory]` lines.

**D19 — Catch-ups for both passes.** v4 `catchup.ts`, `fold-catchup.ts`,
`scheduled-maintenance.ts:267-291`. v5 has the OTHER half only
(`fold_other_pass.rs:748-1194`, sweep step 8 `scheduled_maintenance.rs:367-381`,
summary field `:114-116`, INFO `:406-446`): add the episode selection (by
anchors), `runFoldCatchup` for both, step 9, the shared chat / cursor read, the
in-flight chain guard (bug 232), `[FoldCatchup]` + `maintenance.fold-catchup`
lines, summary `foldEpisodeCatchup`.

**D20 — The fold hook.** v4 `context-summary.ts:552-574`. v5 `ContextSummarySeams`
(`context_summary.rs:225-282`) has `run_fold_episode_pass` + `run_fold_other_pass`
seams; collapse to ONE seam method (`run_fold_memory_passes(input)`: build the
context once, episode then other) and the ONE ERROR line `[Context Summary] Fold
memory passes failed:` — REACHABLE now (a context-build throw: `resolveSpeakerNames`,
the cursor read is inside `runFoldPass`'s catch so not that one; measure). Noop
seams stay no-op.

Jobs

**D21 — The payload.** v4 `queue-service.ts:55-123`. v5
`MemoryExtractionPayload` (`memory_extraction_job.rs:54-108`) has
`fold_other_catchup` (truthy) + `fold_other_ignore_idle` (strict): add `pass`,
`cursor`, `ignoreIdle` (strict), `backfill` (strict), `extractAfterMessageId`,
`foldEpisodeCatchup` (truthy), and `passOf` (episode legacy flag BEFORE other).
**Hazard §1 (3):** v4-written catch-up rows (`pass: 'other-fold' |
'episode-fold'`, null opener) are today misread by v5 as turn jobs.

**D22 — The handler.** v4 `memory-extraction.ts`. v5
`handle_memory_extraction` (`:119-500`): the pass branch (both passes; the INFO
line's keys; `taskLabel = `${pass}-catchup`` — v5 `"fold-other-catchup"`, `:207`),
the continuation enqueue (keyed on the advanced cursor; never after a rate
deferral; carries `ignoreIdle` / `backfill`), the turn branch through
`runTurnExtraction` with `extractAfterMessageId`, the missing log lines (v5 emits
NONE of `Chat not found at job execution; skipping`, `Turn has no character
contributions; skipping`, `Turn processed`, `Failed to store debug logs`, `Failed
to emit token tracking event` — the debug-log write and cost event are `let _ =`,
`:455-497`), the lost-timeout ERROR's fields (`task` replaces `error`) and
`lastError` text, the debug-log write KEEPING the recall record lines first
(`withRecallLines` — v5 overwrites, `:458`).

**D23 — Enqueue + dedupe.** v4 `enqueueMemoryExtraction`. v5
`enqueue_memory_extraction_with` (`queue_service.rs:443-538`): dedupe on `(chat,
opener STRICT, anchor ?? null, passOf, cursor ?? null)` (v5's `foldOtherCatchup`
term), `skipDedupCheck` (v5 "not modeled"), the payload key orders per caller
(§7.3). Batch: `backfill: true` + conditional `extractAfterMessageId`
(`queue_service.rs:587-650`).

**D24 — queue-memories.** v4 `actions/memories.ts:36-113`. v5
`chat_queue_memories` (`api/memories.rs:1947-2075`): profile resolution
(seat → default cheap → USER_DEFINED, `userId` match), the 400 texts, the WARN /
INFO lines (v5 has neither), `listExtractionTurns` (the greeting turn; the
autonomous `extractAfterMessageId`), `backfill`.

**D25 — The regenerate rebuild (`MEMORY_REGENERATE_CHAT`).** v4 §1.16. v5 has NO
handler (loud fallback; `queue_service.rs:448` records P4.D270 R-I's deferral).
If ported: wipe via the retire sibling, `clear_for_chat`, turn jobs (backfill,
`skipDedupCheck`, bounds), catch-ups (`regenerate:<jobId>` anchor). See §9 O3.

**D26 — `INTER_CHARACTER_MEMORY` retired.** Remove from every v5 list (§2 row);
the boot ensure (`drop-inter-character-memory-jobs-v1`, keystone) deletes queued
rows; the regenerate-all clear list loses it (`api/memories.rs:1589`; pin
`route.ts:1284-1287`).

**D27 — Settings surfaces.** `extraction-concurrency` retired
(`api/memories.rs:1476-1505`, `api/types.rs:1724-1729`, `db/instance_settings.rs:527`);
`extraction-limits-config` POST through a patch schema with `maxPerHour`
`int().positive().max(10_000)` (Zod issue bytes for `0` move from `>=1` to `>0`
— measure v5's `validate_extraction_limits_config`); `extraction-mode-config` POST
loses the default clobber (bug 244 — v5's R-A "faithful clobber",
`api/memory_consolidation.rs:110-133`, trips by design; the settings / API sibling
owns `PARTIAL_DEFAULTS_CLOBBER`'s retirement, but the extraction-mode arm is on
the Extraction Grain card).

**D28 — Dry run.** v4 `dry-run.ts` + the NDJSON route. v5: a standing deferral
(`turn_transcript.rs:71-73`; the web layer only lists the action name,
`quilltap-web/src/wardrobe_routes.rs:627`; CLI `memory-diff` is in `SUBCOMMANDS`,
`quilltap-cli/src/main.rs:57`). See §9 O4.

---

## 4. v5 on `main` today — per file

- **`services/memory_processor.rs`** (1,667) — DIES except the canon loaders
  `:80-330` (`read_vault_text_file`, `read_vault_text_file_conn`,
  `load_commonplace_canon`, `load_canon_for_self_with_commonplace`,
  `load_canon_for_observer_about_subject`, `load_hand_canon`), which three
  consumers use (extraction, consolidation `include_commonplace: false`) — move
  them to a canon home (e.g. `services/canon_loaders.rs`) or keep the file as the
  canon module. Everything `:332-1543` is replaced; the `other_pass_mode_tests`
  (`:1545-1667`) re-home with the mode read (the WARN arm R-D is v5-only).
- **`memory_tasks.rs`** (1,240) + **`memory_tasks/prompt_text.rs`** (411) —
  the prompt / parse leaves are REWRITTEN into the pin's shape (draft parsers,
  `pass_cap`, the clock, the earlier section, the episode prompt). Keep
  `strip_code_fences` (shared) and the generated body constants (regenerate from
  the four pin files). `FoldEpisode.participants` dies; `ExtractionGrain`
  survives; `FoldEpisodeMessage` / `FoldOtherMessage` become `StretchMessage`.
- **`services/turn_transcript.rs`** (338) — grows `extract_after_message_id`,
  prior slices, `list_extraction_turns`, `find_autonomous_turn_anchor_message_id`,
  `with_prior_as_material`, `load_chat_message_events`; the structs in
  `memory_tasks.rs:370-409` gain `prior_slices: Option<Vec<…>>` (absent ≠ empty).
- **`services/memory_extraction_job.rs`** (543) — rewritten around the context:
  payload `:54-108`, handler `:119-500`; `cheap_settings_from` `:505-521` and
  `limits_from_value` `:533-543` die into the context / settings.
- **`services/carina_memory_extraction.rs`** (385) — collapses to v4's 74-line
  shape over the context (`:62-343` replaced).
- **`services/fold_other_pass.rs`** (1,194) — DIES into the pass + fold runner +
  catch-up modules (its census-tested single watermark writer goes too).
- **`services/fold_episode_pass.rs`** (490) — DIES into the episode pass.
- **`services/context_summary.rs`** (1,532) — the two seam methods
  (`:265-282`), `FoldOtherHookInput` (`:291-306`), `run_fold_other_hook`
  (`:313-366`), the three impls' fold arms (`:390-396`, `:440-470`, `:571-600`),
  the call site `:1004-1098` → one hook.
- **`services/scheduled_maintenance.rs`** (719) — step 9 + the shared read
  (`:367-381`), summary field (`:114-121`, `:206`), INFO fields (`:406-446`).
- **`services/queue_service.rs`** — `MemoryExtractionEnqueue` / dedupe
  `:443-560`, batch `:562-650`.
- **`services/message_finalizer.rs`** `:1500-1550` (the trigger — anchor walk
  hoisted), callers `:1402`, `courier_transport.rs:673`,
  `help_chat/orchestrator.rs:1322` (unchanged payloads).
- **`api/memories.rs`** — queue-memories `:1947-2075`; concurrency `:1476-1505`
  (dies); limits `:1436-1474` (schema); regenerate-all clear list `:1580-1596`.
- **`api/memory_consolidation.rs`** `:100-133` (extraction-mode config, bug 244).
- **`api/types.rs`** `:1718-1729`, `:2868-2873`.
- **`quilltap-host/src/spine.rs`** `:3220-3335` — the two job wrappers
  (`read_memory_extraction_limits` folds into the context; the Carina wrapper's
  "limits: None" comment is stale); registration `:3927`, `:3934` (+ a
  `MEMORY_REGENERATE_CHAT` row if O3 ports it).
- **`episodic.rs`** (545) — the time sibling's; extraction calls
  `resolve_when_phrase` (`:231`) today via `resolve_candidate_anchors`; at the pin
  it calls the time module's `resolve_episodic_anchors` with a zone.
- **`db/memories_read.rs:1124`** `count_created_since` — loses its extraction
  caller (keep if others use it).

---

## 5. Harness

### 5.1 Existing families in this slice, at a pin regen

| family (Rust test → oracle case → builder) | tier | at `64743ee13` | why |
|---|---|---|---|
| `memory_tasks_equivalence` (632) → `memory-tasks-tier1.test.ts` | 1 | **IMPORT-DEAD** | `@/lib/memory/cheap-llm-tasks/memory-tasks` (`extractSelfMemoriesFromTurn`, `extractOtherMemoriesFromTurn`, `extractOtherMemoriesFromFold`) deleted |
| `memory_processor_tier3_equivalence` (782) → `memory-processor-tier3.test.ts` → `build-memory-processor-fixture.ts` | 3 | **IMPORT-DEAD** | `@/lib/memory/memory-processor` (`processTurnForMemory`); builder imports `@/lib/database/repositories/vector-indices.repository` (deleted, Phase B) |
| `fold_episode_tier3_equivalence` (760) → `fold-episode-tier3.test.ts` → `build-fold-episode-fixture.ts` | 3 | **IMPORT-DEAD** | `@/lib/memory/fold-episode-pass` (`runFoldEpisodePass`); builder `@/lib/schemas/vector-indices.types` + the vector repository |
| `fold_other_pass_tier3_equivalence` (932) → `fold-other-pass-tier3.test.ts` → `build-fold-other-pass-fixture.ts` | 3 | **IMPORT-DEAD** | `@/lib/memory/fold-other-pass` (`runFoldOtherPass`, `findFoldOtherCatchupCandidates`), `@/lib/background-jobs/maintenance/fold-other-catchup` (`enqueueFoldOtherCatchups`); builder plants `chats.otherExtractionWatermarkMessageId` (`:279`) + `vector-indices.types` (`:328`) |
| `memory_pipeline_jobs_tier3_equivalence` (853) → `memory-pipeline-jobs-tier3.test.ts` → `build-memory-pipeline-jobs-fixture.ts` | 3 | **IMPORT-DEAD (builder)** then RED | case imports are live (`handlers/memory-extraction`, `handlers/context-summary`); builder `:110` imports the deleted vector repository; comparands `vector_indices` / `vector_entries` die; every debug-log line, payload, lastError moves |
| `carina_memory_extraction_tier3_equivalence` (443) → `carina-memory-extraction-tier3.test.ts` → `build-carina-memory-extraction-fixture.ts` | 3 | **IMPORT-DEAD (builder)** then RED | builder `:135,231` `VectorIndicesRepository`; the case's `@/lib/instance-settings` mock spreads `actual` (so `resolveMemorySettings`' other getters stay real — fine); RED on project, provenance, createdAt, debug lines, lastError, log owners |
| `context_summary_service_tier3_equivalence` (1,303) → `context-summary-service-tier3.test.ts` → `build-context-summary-service-fixture.ts` | 3 | **IMPORT-DEAD (builder)** then RED | builder `:166-193` `vector-indices.types`; the fold-memory calls' prompts / cursors / ERROR line move (the recall / summary siblings share this family) |
| `turn_transcript_equivalence` (227) → `turn-transcript.ts` | 1 | **GREEN predicted** | module live; no corpus row carries a bound; rows without `extractAfterMessageId` are byte-identical (`priorSlices` key omitted) — GROW it |
| `maintenance_ops_tier2_equivalence` (314) → `maintenance-ops-tier2.ts` → `build-maintenance-ops-fixture.ts` | 2 | RED | `runScheduledMaintenance()` summary gains `foldEpisodeCatchup` (+ sibling `builtinRefit`); the shared `memoryExtractionCursors.findAllByChat()` on a builder fixture with no cursor table — measure whether `getCollection` creates it or the WARN / failures arm fires |
| `other_extraction_watermark_writers_census` (107) | census | **RETIRE** | the column is dropped; REPLACE with a `memory_extraction_cursors` writers census (only the cursor repository's upsert / clears write the table) |
| `canon_commonplace_tier2_equivalence`, `canon_scenario_equivalence` | 2 / 1 | GREEN | `canon.ts` unchanged |
| `episodic_equivalence` | 1 | (time sibling) | `lib/memory/episodic.ts` live but −205 |
| `regenerate_swipe_tier3_equivalence` | 3 | (recall sibling) | imports `frozen-archive-cache` / `memory-recap` (deleted) |

Shared trap: the v5 prompt generator `harness/oracle/scripts/extract-memory-task-prompts.py`
reads ONE file (`cheap-llm-tasks/memory-tasks.ts`, deleted); re-point it at
`lib/memory/extraction/prompts/{self,other,shared,episode}.ts` (the bodies are
unchanged, so the regenerated `prompt_text.rs` must come out byte-identical for
SELF / OTHER / grain — a free self-check — and the episode body minus one line).

### 5.2 New / re-aimed families (proposed)

- **tier-1 `extraction-prompts`** (re-aims `memory_tasks_equivalence`): v4's
  REAL `buildSelfTurnMessages`, `buildOtherTurnMessages`,
  `buildOtherFoldMessages`, `buildEpisodeFoldMessages`, `renderTurnContext`
  (incl. `priorSlices`), `renderStretch`, `renderClockBlock` /
  `renderClockStamp` (zones: UTC, America/Chicago, a DST edge, Pacific/Kiritimati
  date-line, a malformed `nowIso`), `passCap` (maxTokens 0 / undefined / 4096 /
  8000 / 8001 / 64000 × base 1 / 2 / 3 / 7.9), `parseSelfDrafts` /
  `parseOtherDraftsBySubject` / `parseEpisodeDrafts` (the `null` element, a `0`
  importance, `1.7`, `-0.2`, `NaN`-ish strings, whitespace content, 9 entities,
  out-of-range / non-integer `subjectIndex`, the event slot at fold cap 3). No
  model; pure. Copy `memory-tasks-tier1.test.ts`'s rig.
- **tier-1 `turn-transcript` growth**: bound inside / outside / at the last
  line, user-controlled opener → prior, `.slice(-8)` cap with 10 prior lines,
  `listExtractionTurns` (greeting before first user, staff-only greeting, no
  user, empty, autonomous anchors + `extractAfterMessageId`),
  `withPriorAsMaterial`, `findAutonomousTurnAnchorMessageId`.
- **tier-3 `extraction-turn`** (re-aims `memory_processor_tier3`): v4's REAL
  `buildExtractionContext` + `runTurnExtraction` / `runCarinaExtraction` over a
  rebuilt fixture (cursor table; UUID chat ids — the cursor schema's `chatId` is
  `UUIDSchema`), mocked LLM by recorded call key; ops: a 3-character turn; a
  reopened turn with the `turn` cursor mid-turn (SELF only new speaker; the
  earlier speaker observes new lines only); a turn read to its end; a foreign
  cursor; backfill (whole turn, cursor forward-only); a throttled + a skipped
  character (subject retained; THROTTLING lines per call); hybrid / fold / turn
  modes; a `0` importance draft; an admission throw for one candidate; a
  timed-out SELF call; autonomous provenance; the default cheap profile
  selection; an Unmoderated chat (fallback `isDangerousChat`). Compare
  `TurnExtractionResult` (debug lines byte-for-byte), `memories`,
  `memory_extraction_cursors`, the captured `[Memory]` / `[MemorySettings]` /
  `[ExtractionCursors]` / `[Canon]` lines. Closest recipe:
  `memory_processor_tier3_equivalence.rs` header + `fold_other_pass_tier3`'s
  `global_capture` + per-op settings plants.
- **tier-3 `extraction-fold`** (re-aims `fold_other_pass_tier3` +
  `fold_episode_tier3`): `runFoldPass` both passes (150-message chat → 60/60/30
  over two runs; a timeout on slice 2; a rate-deferred slice; a partially capped
  episode holder set; `behind-cursor`, `nothing-to-observe` advance,
  `nobody-present`, `mode-turn`; an episode REINFORCE not linked; two episodes in
  one run not linked to each other), `findFoldCatchupCandidates` both passes,
  `runFoldCatchup`, `enqueueFoldCatchups` (in-flight guard), the handler's pass
  branch + continuation (incl. legacy `foldOtherCatchup` / `foldEpisodeCatchup`
  rows). Compare results, `memories` (+ `relatedMemoryIds`),
  `memory_extraction_cursors`, `background_jobs` payload BYTES, lines.
- **tier-3 `extraction-jobs`** (re-aims `memory_pipeline_jobs_tier3` +
  `carina_memory_extraction_tier3`): the three handlers end to end incl.
  `lastError`, `debugMemoryLogs` with a planted recall record (kept first), the
  cost event, `llm_logs` (P4.144's comparand), the missing-chat-settings row now
  succeeding.
- **tier-2 `extraction-enqueue`**: `enqueueMemoryExtraction` dedupe matrix
  (strict opener: a stored `undefined` key vs incoming `null`; `pass` legacy
  flags; cursor term), `enqueueMemoryExtractionBatch` bytes, queue-memories
  route (texts, profile resolution, greeting turn).
- **tier-2 cursor repository** — belongs to the keystone (§7.1); this lane
  consumes it.

### 5.3 Fixture rules the lanes will hit

- Rebuild builders through v4's real `MigrationRunner` at the pin (memory note
  "rebuilt fixture pair vintage") so `memory_extraction_cursors` exists in its
  MIGRATION shape (UNIQUE + FK) on migrated pairs and the bare generateDDL shape
  on fresh ones.
- Every builder that used `repos.vectorIndices` must plant vectors on
  `memories.embedding` (Float32 LE — memory note "memory search plants need
  vectors").
- The `cursor-bundle` WARNs are on a CHILD logger (`module:
  'memory:extraction:cursor-bundle'`) — the capture needs the child-binding fix
  (memory note "child-logger bindings invisible to the capture").
- v4's fold uses `Date.now()` in `stretchWindow` / `turnWindow` only when no
  message is stamped — stamp every fixture message.

---

## 6. v4 jest tests at the pin covering this slice (mirror these)

| path | cases (approx.) |
|---|---|
| `lib/memory/extraction/__tests__/clock-zone.test.ts` | 10 (zone on every pass's CLOCK; `clockAt`; `withMemoryTimeZone`) |
| `lib/memory/extraction/__tests__/consent-regression.test.ts` | 4 (AGREEMENTS block, ordered heading) |
| `lib/memory/extraction/__tests__/prompt-tags.test.ts` | 14 (targeting normalisation into row keywords) |
| `lib/memory/extraction/__tests__/self-prompt-user-controlled.test.ts` | 10 (first-person clause) |
| `__tests__/unit/lib/memory/extraction/context-and-carina.test.ts` | 11 (one hydrate per character; seat profile; bug 222; bug 213 `isDangerousChat`; **bug 245 ×5**) |
| `__tests__/unit/lib/memory/extraction/turn-passes-regression.test.ts` | 16 |
| `__tests__/unit/lib/memory/extraction/turn-passes-zone.test.ts` | 1 |
| `__tests__/unit/lib/memory/extraction/run-pass-invariants.test.ts` | 5 (dry run = live; live counter; fold caps + event slot) |
| `__tests__/unit/lib/memory/extraction/other-turn-mode.test.ts` | 6 |
| `__tests__/unit/lib/memory/extraction/other-fold-pass.test.ts` | 21 |
| `__tests__/unit/lib/memory/extraction/episode-fold-pass.test.ts` | 33 |
| `__tests__/unit/lib/memory/extraction/fold-catchup.test.ts` | 22 |
| `__tests__/unit/lib/memory/extraction/fold-cursor-slicing.test.ts` | 2 (60/60/30; timeout on slice 2) |
| `__tests__/unit/lib/memory/extraction/review-94-fixes.test.ts` | 8 (rate-deferred slices; mid-run throw; backfill; `listExtractionTurns` ×4) |
| `__tests__/unit/lib/services/chat-message/turn-transcript.test.ts` | 19 (bug 245 ×5) |
| `__tests__/unit/background-jobs/memory-extraction-handler.test.ts` | 16 |
| `__tests__/unit/background-jobs/memory-extraction-catchup-continuation.test.ts` | 7 |
| `__tests__/unit/background-jobs/carina-memory-extraction-handler.test.ts` | 3 |
| `lib/background-jobs/handlers/__tests__/carina-memory-extraction.test.ts` | 12 |
| `__tests__/unit/background-jobs/maintenance/fold-catchup.test.ts` | 9 |
| `__tests__/unit/lib/background-jobs/memory-regenerate-chat-fold-catchup.test.ts` | 7 |
| `__tests__/unit/lib/chat/context-summary-fold-title.test.ts` | 4 |
| `__tests__/unit/api/memories-dry-run-project.test.ts` | 6 |
| `__tests__/unit/lib/database/migration/add-memory-extraction-cursors-v1.integration.test.ts` | 5 (keystone's) |
| `__tests__/helpers/extraction-context.ts` | helper (167) — the fake context the unit tests build |

Deleted with the span (do not mirror): `memory-processor{,-regression,-other-mode}.test.ts`,
`fold-{other,episode}-pass.test.ts`, `fold-other-catchup.test.ts`,
`memory-regenerate-chat-fold-other.test.ts`, `inter-character-memory-handler.test.ts`,
`cheap-llm-tasks/__tests__/memory-extraction-tags.test.ts`,
`memory-consent-regression.test.ts`, `memory-user-controlled.test.ts`.

---

## 7. Cross-lane meeting points and proposed C1 spellings

### 7.1 NEEDS from the schema KEYSTONE

- The `memory_extraction_cursors` table (both shapes) + the boot ensure that
  copies the two watermark columns and DROPS them (RULED; retires
  `memory_tiers_repair.rs:105`'s arm) — until it lands, nothing here can read a
  cursor. The `chats` struct loses `other_extraction_watermark_message_id`
  (`db/chats.rs:417`) and the read (`chats_read.rs:96,322`).
- The cursor REPOSITORY (proposed `crate::db::memory_extraction_cursors`):
  ```rust
  #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
  pub enum ExtractionCursorPass { OtherFold, EpisodeFold, Turn }   // v4 enum order
  impl ExtractionCursorPass {
      pub const ALL: [Self; 3];                       // other-fold, episode-fold, turn
      pub fn as_str(self) -> &'static str;            // "other-fold" | "episode-fold" | "turn"
      pub fn parse(s: &str) -> Option<Self>;
      pub fn bundle_field(self) -> &'static str;      // the three legacy chat keys
      pub fn as_fold(self) -> Option<FoldCursorPass>;
  }
  #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
  pub enum FoldCursorPass { OtherFold, EpisodeFold }
  impl From<FoldCursorPass> for ExtractionCursorPass { … }
  pub fn extraction_cursor_id(chat_id: &str, pass: ExtractionCursorPass) -> String; // "<chatId>:<pass>"
  #[derive(Clone, Debug, Default, PartialEq, Eq)]
  pub struct ExtractionCursorMap { pub other_fold: Option<String>, pub episode_fold: Option<String>, pub turn: Option<String> }
  // reads THROW (no fallback): DbError, never Ok(None) on failure
  pub fn find_by_chat_id(conn: &Connection, chat_id: &str) -> Result<ExtractionCursorMap, DbError>;
  pub fn find_cursor(conn: &Connection, chat_id: &str, pass: ExtractionCursorPass) -> Result<Option<String>, DbError>;
  pub fn find_all_by_chat(conn: &Connection) -> Result<BTreeMap<String, ExtractionCursorMap>, DbError>;
  // writer verbs (single-writer): the one upsert + the two clears, with v4's DEBUG lines on the caller thread
  impl MainWriter { fn set_extraction_cursor(&self, chat_id, pass, message_id, now_iso) -> Result<(), DbError>;
                    fn clear_extraction_cursor(&self, chat_id, pass) -> Result<bool, DbError>;   // fallback false
                    fn clear_extraction_cursors_for_chat(&self, chat_id) -> Result<usize, DbError>; } // fallback 0
  ```
- `instance_settings`: the limits getter typed (`MemoryExtractionLimits` with the
  10,000 max) and the concurrency key GONE.
- `BackgroundJobType` without `INTER_CHARACTER_MEMORY` (+ the job-row delete
  ensure).

### 7.2 NEEDS from the ADMISSION / CANDIDATE / TIME / RETIRE / RECALL siblings (the seams, by name)

```rust
// crate::memory_candidate  (candidate sibling)
pub struct MemoryDraft { content: Option<String>, summary: Option<String>, keywords: Vec<String>,
    importance: f64, targeting: TargetingTags, kind: MemoryKind, when: Option<String>,
    entities: Option<Vec<String>>, narrative_time: Option<String> }
pub enum CandidatePass { SelfTurn, OtherTurn, OtherFold, EpisodeFold, Carina, Manual, Consolidation, Import }
pub struct CandidateSubject { holder_id: String, about_id: Option<String> }
pub struct CandidateProvenance { source: MemorySource, witnessed_context: Option<WitnessedContext>,
    chat_id: Option<String>, project_id: Option<String>, source_message_id: Option<String>,
    anchor_time_ms: f64, pass: CandidatePass }
pub fn candidate_from_draft(d: &MemoryDraft, s: CandidateSubject, p: CandidateProvenance) -> Option<MemoryCandidate>;
pub fn anchor_time_ms_from(isos: &[Option<&str>], fallback_ms: f64) -> f64;
pub fn coerce_importance(raw: Option<&Value>) -> f64;
pub fn normalise_targeting(raw: &Value, fallback: PartialTargeting) -> TargetingTags;
pub struct ResolvedCandidateTime { occurred_at: Option<String>, narrative_time: Option<String>,
    source_message_timestamp: Option<String> }
pub fn to_admission_row(c: &MemoryCandidate, r: &ResolvedCandidateTime) -> AdmissionRow;

// crate::services::admission  (admission sibling) — THE extraction→admission seam
pub struct AdmitOptions<'a> { pub user_id: &'a str, pub dry_run: bool, pub backfill: bool,
    pub dry_run_book: Option<&'a DryRunBook> }
pub async fn admit_candidate<E: EmbeddingProvider>(db: &Db, embedding: &E, c: &MemoryCandidate,
    t: &ResolvedCandidateTime, o: &AdmitOptions<'_>) -> Result<AdmissionOutcome, AdmissionError>;
pub fn admission_inserted(o: &AdmissionOutcome) -> bool;          // insert | insert-linked
pub fn describe_admission(o: &AdmissionOutcome, label: &str, summary: &str) -> String;
pub const RATE_LEDGER_WINDOW_MS: i64 = 3_600_000;
pub fn count_recent_admissions(character_id: &str, since_ms: i64) -> usize; // process-global ledger
pub async fn link_both_ways(db: &Db, character_id: &str, memory_id: &str, link_to: &[String]) -> Result<(), DbError>;
pub fn create_dry_run_book(retiring: HashSet<String>) -> DryRunBook;

// crate::memory_time  (time sibling)
pub fn resolve_episodic_anchors(i: &EpisodicAnchorInput<'_>) -> EpisodicAnchors; // + time_zone: Option<&str>
pub fn refresh_memory_time_zone(db: &Db) -> String;
pub fn zoned_date_stamp(ms: i64, tz: &str) -> String;  pub fn zoned_weekday(ms: i64, tz: &str) -> usize;

// retire sibling: preview_chat_wipe(db, chat_id) -> Result<HashSet<String>, DbError>;
//                  retire_chat_memories(db, chat_id) -> RetireReport   (regenerate, O3)
// recall sibling:  with_recall_lines(existing: &[String], lines: &[String]) -> Vec<String>
```
The admission sibling's `AdmitOptions` has NO idempotency key in-process (v4's
`jobId:content:n` key is the job child's host-RPC concern); v5's retry
idempotency is already recorded as reached "by a different route"
(`memory_extraction_job.rs:398-421`). The admission lane must keep
`record_admission_insert` for AUTO, non-backfill inserts — the limiter's baseline.

### 7.3 PROVIDES (proposed spellings for this lane)

Module layout (pure leaves vs services, the v5 split):
- `crate::memory_extraction` (pure): `window` (`TranscriptWindow`,
  `StretchMessage`, `first_valid_iso`, `stretch_window`, `turn_window`,
  `render_stretch`, `render_turn_context`, `ORDERED_TURN_TRANSCRIPT_HEADING`,
  `PRIOR_TURN_CONTEXT_HEADING`, `is_fold_eligible_message`,
  `messages_past_cursor`, `window_past_cursor`, `slice_fold_window`,
  `fold_pass_range`), `parse` (`HARD_CANDIDATE_CAP`, `EVENT_EXTRA_SLOT`,
  `pass_cap`, `cap_with_event_slot`, `coerce_memory_draft`, `parse_self_drafts`,
  `parse_other_drafts_by_subject`, `parse_episode_drafts`, `EPISODE_TARGETING`),
  `prompts::{shared, self_turn, other, episode}` (+ the generated `prompt_text`),
  `rate_limit::rate_limit_decision`. The old `memory_tasks.rs` extraction half
  retires (`strip_code_fences` moves to `llm_json` or stays as the one shared
  helper).
- `crate::services::extraction`: `context` (`ExtractionContext`, `CastMember`,
  `BuildExtractionContextOptions`, `build_extraction_context`,
  `ExtractionContextError { NoChat, NoConnectionProfile }`,
  `pick_extraction_connection_profile_id`, `clock_at`), `route`
  (`ExtractionRoute`, `resolve_extraction_route`), `rate_limit`
  (`ExtractionRateLimiter::{new(limits, baseline_fn), decide, record}`), `cursors`
  (`CursorStore` as an ENUM `{ Live { chat_id }, Memory(ExtractionCursorMap) }` —
  avoids an async trait), `run` (`ExtractionPassName`, `DraftBatch`, `PassCall`,
  `ExtractionPass` as an enum `{ SelfTurn { candidate_pass }, OtherTurn,
  OtherFold, EpisodeFold { written_episode_ids } }`, `AdmittedCandidate`,
  `PassResult`, `run_pass`, `merge_pass_result`), `turn` (`TurnLocator`,
  `TurnExtractionResult`, `run_turn_extraction`, `run_turn_passes`,
  `run_carina_extraction`, `carina_transcript`), `fold` (`FoldResult`,
  `FoldSkipReason { ModeTurn, EmptyWindow, BehindCursor, NothingToObserve,
  NobodyPresent, RateLimited, Error, NotIdle, NeverFolded }` rendering v4's
  strings, `run_fold_pass`, `FOLD_MAX_WINDOW_MESSAGES = 60`,
  `FOLD_MAX_SLICES_PER_JOB = 4`), `catchup` (`find_fold_catchup_candidates`,
  `run_fold_catchup`, the three constants), `dry_run` (if ported),
  `cursor_bundle` (`with_bundle_cursor_fields`, `take_bundle_cursors`,
  `restore_bundle_cursors`, `load_bundle_cursors` — consumed by the carriers
  sibling).
- The job payload (`services/memory_extraction_job.rs`):
  ```rust
  #[derive(Clone, Copy, Debug, PartialEq, Eq)]
  pub enum MemoryExtractionPass { Turn, OtherFold, EpisodeFold }   // "turn" | "other-fold" | "episode-fold"
  #[derive(Clone, Debug, Default)]
  pub struct MemoryExtractionPayload {
      pub chat_id: String,
      pub turn_opener_message_id: Option<String>,
      pub extraction_anchor_message_id: Option<String>,
      pub extract_after_message_id: Option<String>,
      pub connection_profile_id: String,
      pub pass: Option<String>,            // RAW: v4 passes any truthy string through (§9 O9)
      pub cursor: Option<String>,
      pub ignore_idle: bool,               // === true
      pub backfill: bool,                  // === true
      pub legacy_fold_other_catchup: bool, // JS-truthy
      pub legacy_fold_other_ignore_idle: bool, // === true
      pub legacy_fold_episode_catchup: bool,   // JS-truthy
      pub job_id: Option<String>,          // host-filled
  }
  impl MemoryExtractionPayload { pub fn decode(v: &Value) -> Self; pub fn pass_of(&self) -> PassOf; }
  pub enum PassOf { Known(MemoryExtractionPass), Unknown(String) }
  ```
  and the enqueue struct grows (`queue_service.rs`):
  ```rust
  pub struct MemoryExtractionEnqueue<'a> {
      pub chat_id: &'a str, pub turn_opener_message_id: Option<&'a str>,
      pub extraction_anchor_message_id: Option<&'a str>,
      pub extract_after_message_id: Option<&'a str>,
      pub connection_profile_id: &'a str,
      pub pass: Option<MemoryExtractionPass>, pub cursor: Option<&'a str>,
      pub ignore_idle: bool, pub backfill: bool, pub skip_dedup_check: bool,
  }
  ```
  Payload KEY ORDERS (stored bytes): trigger `{chatId, turnOpenerMessageId,
  extractionAnchorMessageId, connectionProfileId}`; sweep `{chatId,
  turnOpenerMessageId:null, extractionAnchorMessageId, connectionProfileId, pass}`;
  continuation `{chatId, turnOpenerMessageId:null, extractionAnchorMessageId:null,
  connectionProfileId, pass, cursor[, ignoreIdle:true][, backfill:true]}`;
  regenerate turn `{chatId, turnOpenerMessageId, extractionAnchorMessageId[,
  extractAfterMessageId], connectionProfileId, backfill:true}`; regenerate
  catch-up `{chatId, turnOpenerMessageId:null, extractionAnchorMessageId:
  "regenerate:<jobId>", connectionProfileId, pass, ignoreIdle:true,
  backfill:true}`; batch `{chatId, turnOpenerMessageId, extractionAnchorMessageId[,
  extractAfterMessageId], connectionProfileId, backfill:true}`.
- The fold seam on `ContextSummarySeams`: replace `run_fold_episode_pass` +
  `run_fold_other_pass` with ONE `run_fold_memory_passes(&self, input:
  FoldMemoryPassesInput)` (`{ chat_id, user_id, chat: Value, fold_transcript:
  Vec<Value>, window_messages: Vec<Value>, connection_profile: Value,
  chat_context_summary: String }`).

### 7.4 Wire (`api/types.rs`)

- REMOVE `Request::MemoryExtractionConcurrencyGet` / `MemoryExtractionConcurrencySet { concurrency }`
  (`:1724-1729`) and their engine / web arms (v4 answers the unknown-action 400).
- `MemoryExtractionLimitsSet { config }` / `MemoryExtractionModeConfigSet { config }`
  keep their shape; validation bytes move (D27).
- `ChatQueueMemories { chat_id }` unchanged; response unchanged; 400 texts move.
- If the dry run ports (O4): NEW `Request::ChatExtractMemoriesDryRun { chat_id,
  concurrency: Option<String> }` with an NDJSON stream on the Event channel (the
  `?action=extract-memories-dry-run` POST), events `start` / `candidate` / `turn` /
  `turn-error` / `fold` / `done` / `fatal` / `ping`.

---

## 8. SPA / help / docs / CLI touch points

- **SPA** (sibling-owned, named here): v4 `components/tools/memory-extraction-grain-card.tsx`
  (+5) + NEW `memory-extraction-limits-section.tsx` (97 — fields `maxPerHour`
  "Notes per hour" 1–10,000 step 1, `softStartFraction` "Begin choosing at" 0–1
  step 0.05, `softFloor` "Importance floor" 0–1 step 0.05, heading "Rate limits",
  the enabled toggle, `'extraction limits'` noun, error
  `` `${label} must be ${integer ? 'a whole number' : 'a number'} between ${min} and ${max}` ``)
  → v5 `apps/web/src/app/screens/settings/memory/memory-extraction-grain-card.ts`
  (234; has NO limits section; the contract already carries
  `MemoryExtractionLimitsGet` / `Set`, `core/core-contract.ts:6123-6128`, unused).
  `MemoryExtractionConcurrencyGet/Set` have no SPA consumer (contract-only).
- **help/**: `memory-regenerate.md` (ac6348c5f, b045dd986), `chat-participants.md`
  (ac6348c5f), `episodic-memory.md` (c2795733f, e0ac7493a, b045dd986),
  `chat-settings.md` (c2795733f), `settings.md` (b045dd986 — the Extraction Grain
  card's limits). Re-vendored by the help sibling.
- **docs**: `docs/developer/API.md` (dry-run wire, `decision` / `extractionPass`,
  the `fold` event), `DDL.md` (the cursor table), `BACKGROUND_JOBS_CHILD.md`,
  `features/complete/memory-system-refactor.md` §3.7 — `docs/v4/` re-vendor.
- **`public/schemas/qtap-export.schema.json`**: chat
  `episodeExtractionWatermarkMessageId` (c2795733f), both cursor keys' DESCRIPTIONS
  (ac6348c5f), `turnExtractionWatermarkMessageId` (64743ee13) — carriers sibling.
- **CLI**: `quilltap memory-diff` (v4 `packages/quilltap/lib/memory-diff-command.js`
  prints `fold` lines since Phase D; `--instance` / `--data-dir` / `--passphrase`
  since bug 239) — v5 lists the subcommand (`quilltap-cli/src/main.rs:57`) and its
  completions (`help/completion/{fish,zsh}.template`); it rides O4.

---

## 9. Open points for the order (observations / questions — NOT rulings)

- **O1 — Size.** This slice is the largest single lane of the round: ~3,600 v4
  lines across 22 modules + 6 satellites, replacing ~5,600 v5 lines in 8 files,
  plus 5 tier-3 / 2 tier-1 families re-aimed. Suggested split (file-disjoint,
  ordered): **E-a** pure leaves (window / parse / prompts / clock render / `pass_cap`
  + the generator re-point + tier-1 families) → **E-b** context + route + rate
  limiter + cursor store + pass runner + the four passes + turn grain (bug 245) +
  the turn-transcript builder + handlers' turn / Carina branches → **E-c** fold
  runner + catch-ups + sweep step 9 + the fold hook + the handler's pass branch +
  continuation + enqueue / dedupe + queue-memories. E-b/E-c both need the admission
  seam LANDED (or frozen as a contract with a stub-free shim — "no stubs" is a
  hard stop, so the order must sequence admission first or let E-b own a thin
  adapter over today's gate that the admission lane replaces).
- **O2 — The interim gate.** If admission lands in the same round as a parallel
  lane, E-b's `run_pass` can only be differentially verified once
  `admit_candidate` exists (every debug line is `describeAdmission`'s). Planner:
  keystone → admission → extraction, or extraction AFTER admission in a later
  wave.
- **O3 — `MEMORY_REGENERATE_CHAT` / `MEMORY_REGENERATE_ALL` handlers.** v5 still
  defers both (loud fallback; `?action=regenerate-all` enqueues a job that dies).
  At the pin the regenerate is the main extraction re-enqueuer and the ONLY clearer
  of cursors besides restore. Port with this round (it now composes cleanly:
  retire sibling + `clear_for_chat` + `list_extraction_turns`) or keep the recorded
  deferral? Needs a ruling.
- **O4 — The dry run + `memory-diff`.** Standing v5 deferral. At the pin it is
  "the live pipeline with `dryRun: true`", so once E-b/E-c land the marginal cost
  is the NDJSON stream + the dry-run book (admission) + `previewChatWipe` (retire).
  Candidate for a **tier-3 loud refusal deferral** if the round is too big —
  name it explicitly in the order either way.
- **O5 — The fold's cheap-LLM selection.** At the pin the fold passes route through
  `buildCheapLLMConfig(chatSettings)` (default cheap profile honoured) while the
  summary itself keeps its own `cheapLLM`; v5 hands both passes the summary's
  selection. Measure on the Friday copy whether the two selections differ when
  `defaultCheapProfileId` is set (they can — that is the point of D2).
- **O6 — Episode eligibility.** `isFoldEligibleMessage` requires a
  `participantId`. The fold's turns already exclude staff / tool rows, so the only
  rows the episode pass newly drops are participant-less USER / ASSISTANT rows and
  blank ones. Measure whether a plain-human chat's USER rows carry a
  `participantId` in v4 / v5 — if not, the episode prompt stops seeing the human's
  lines (a v4 behaviour; port faithfully, but the dogfood should know).
- **O7 — Fold OTHER cap collapse.** `passCap(foldCandidatesPerSubject, maxTokens)`
  states `up to 1` on any profile with `maxTokens ≤ 8000` (most chat profiles).
  Faithful port; flag for the dogfood (fewer fold-OTHER memories than v5 today).
- **O8 — Plain-human opener with a bound.** `renderTurnContext` renders
  `transcript.userMessage` as MATERIAL under the turn heading even when the bound
  moved the turn past it (only a user-character opener moves to EARLIER). Port
  as-is; worth a v4 bug filing? (Observation for the human.)
- **O9 — Unvalidated `pass`.** `memoryExtractionPassOf` returns any truthy
  `payload.pass`; an unknown string runs `runFoldCatchup` with that string as the
  cursor key and takes the EPISODE branch (`catchup.ts:228` only tests
  `'other-fold'`). Faithful (raw `Option<String>`) vs a refusal — a ruling; v4 never
  writes such a row.
- **O10 — `turnOpenerMessageId` strict dedupe.** `===` distinguishes a stored
  ABSENT key from `null`; every v4 writer materialises the key, so only a
  hand-planted row differs. Keep strict (v5's `as_str` collapses both — fix).
- **O11 — `hadContributions === false` skips `finishExtractionJob`.** A turn with
  no new lines persists no debug lines and books no cost (no calls were made), but
  the `turn` cursor still advanced (`runTurnExtraction` advances before returning).
  Faithful; the tier-3 corpus should pin it.
- **O12 — Rate-ledger restart semantics.** v4's ledger is process memory; v5 is
  in-process too, so a v5 restart forgets the hour exactly as v4's parent does.
  But on a SHARED instance (v4 and v5 alternating) neither sees the other's
  inserts — the pre-pin DB count did. Accept (v4 has the same blind spot across its
  own restarts) — note for the hazards block.
- **O13 — `withMemoryTimeZone` / `narrativeNow`.** Both are dead at the pin;
  propose NOT porting `withMemoryTimeZone` and deleting `narrative_now` outright
  (the `memory_recap::distill::ExtractionClock` twin — recall sibling — carries
  its own).
- **O14 — The `[Context Summary] Fold memory passes failed:` ERROR** — reachable
  only through a context-build throw (`resolveSpeakerNames`, the settings getters
  if they ever throw). Measure before claiming "unreachable" (the P4.D270 R-G
  precedent claimed the old line unreachable).
- **O15 — Hazard interplay.** Until the keystone's cursor ensure lands, any v5
  build that runs this lane's code against a v4-migrated instance has no
  `chats.otherExtractionWatermarkMessageId` — the lane must not ship ahead of the
  keystone (ordering, not a ruling).
- **O16 — `canon` call count.** Memoisation changes how many `[Canon]` DEBUG lines a
  unit emits (once per pair per unit); the v5 canon loaders log per call. The
  tier-3 capture will show it — port the memo, not a per-call log.
