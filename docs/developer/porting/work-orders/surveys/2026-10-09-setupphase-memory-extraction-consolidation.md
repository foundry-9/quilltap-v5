# Survey — v4 `d58548051` "feat(memory): consolidation and hot/cold tiers": the EXTRACTION GRAIN + the CONSOLIDATION ENGINE

Read-only planning survey, 2026-10-09. v4 = `~/source/quilltap-server` main,
HEAD `01a83539d` (`4.10.0-dev.143`, tree clean; read with `git show`, never
checked out); v5 = main at `96cfdaaf7`.

Scope (this slug, `memory-extraction-consolidation`): the two halves of
`d58548051` that are NOT the data layer and NOT recall/housekeeping —
(A) the extraction grain (per-turn OTHER `hybrid` floor, the fold-grain OTHER
pass + its fold hook + idle catch-up + regenerate rebuild, the watermark, the
Commonplace canon read, the dedupe, the mode settings consumer) and (B) the
consolidation engine (clustering / plan / tasks / service / triggers / job /
vault mirror / API actions) plus the small riders the brief assigned (Brahma
prompt paragraph, character optimizer hot-only, activity / topic / task-type
table rows). **Out of scope** (other surveys): the D23 re-dump + migration +
boot ensure, the memory row type / every hot-only repository read, the gate's
cold-tier arms, housekeeping demotion, frozen archive / recap / inter-character
digests-first, the scriptorium `search` labels, backup / restore / `.qtap`
carriers, the SPA, the CLI, `help/`.

**Later commits:** of the twelve commits after `d58548051`, only `70f9b495c`
(bug 182 — `memory-gate.ts`, `reinforced-importance.ts`, the repository's
atomic increment) and `7e9eaf42c` (TESTS ONLY for this cluster:
`fold-other-catchup.test.ts` 69, `handlers/memory-consolidation.test.ts` 122,
`consolidation-triggers.test.ts` 198) touch files near this cluster
(`git diff --stat d58548051 01a83539d -- lib/memory lib/background-jobs
lib/instance-settings …` = memory-gate.ts + reinforced-importance.ts only). So
every file below is byte-identical between `d58548051` and HEAD; line numbers
are HEAD.

Everything below is from the HUNKS and the post-commit files, not the commit
prose. Measurements are marked **MEASURED** (one tsx script under
`$TMPDIR/setupphase-survey-memory-extraction-consolidation/m/`, writing nothing
in either repo).

---

## 0. Headline findings (read first)

1. **v4 bug — the two settings POSTs clobber stored values with defaults
   (MEASURED).** `app/api/v1/memories/route.ts` builds
   `consolidationConfigSchema = MemoryConsolidationSettingsSchema.partial()` /
   `extractionModeConfigSchema = MemoryExtractionModeSettingsSchema.partial()`
   and "drops undefined keys so a partial write never clobbers stored values".
   Under Zod 4 `.partial()` over `.default()` fields STILL APPLIES THE DEFAULT
   for an absent key: `MemoryConsolidationSettingsSchema.partial()
   .safeParse({enabled:true})` → `data` = all nine keys
   (`{"enabled":true,"connectionProfileId":null,"clusterThreshold":0.72,
   "minClusterSize":3,"maxClusterSize":30,"matureAfterDays":7,
   "maxClustersPerRun":40,"watermark":150,"coldRetentionDays":null}`);
   `MemoryExtractionModeSettingsSchema.partial().safeParse({})` →
   `{"otherPass":"hybrid","perTurnOtherFloor":0.75,"foldCandidatesPerSubject":3}`.
   Nothing is `undefined`, so EVERY POST rewrites every field, and the v4 SPA
   cards post single-field patches (`memory-extraction-grain-card.tsx:101/112/136`,
   `memory-consolidation-card.tsx:117`): changing the per-turn floor silently
   resets `otherPass` to `hybrid`; toggling `enabled` resets a tuned
   `clusterThreshold`. The INFO line's `fields` therefore always lists all keys.
   **Needs a human ruling (port v4's bytes vs fix v5) and a v4 bug filing**
   before the lane writes the two setters.
2. **The OTHER canon reads Commonplace too, not just SELF.** The ledger row
   says "self-canon now reads `Commonplace/<Subject>.md`". False shape: SELF
   reads `Commonplace/Self.md` (`loadCanonForSelfWithCommonplace`), and the
   per-turn AND fold-grain OTHER passes read `Commonplace/<Subject>.md`
   because `loadCanonForObserverAboutSubject`'s new `includeCommonplace`
   option DEFAULTS TRUE (`canon.ts:255-271`; only consolidation passes
   `false`). Both are model-visible extraction prompt input, both inert until a
   consolidation run has written a mirror file.
3. **Carina extraction is affected; the ledger misses it.**
   `handlers/carina-memory-extraction.ts:21` imports `processTurnForMemory`
   (v5 `carina_memory_extraction.rs:243` calls `process_turn_for_memory`), so
   the mode lookup + its debug line + the SELF Commonplace read ride Carina's
   pass too. `carina_memory_extraction_tier3` is a predicted-red candidate.
4. **Two ledger-predicted reds are wrong for this cluster.**
   `fold_episode_tier3` drives `runFoldEpisodePass` DIRECTLY
   (`fold-episode-tier3.test.ts:329`) and `fold-episode-pass.ts` is untouched
   by the commit — it reddens only through the keystone's new memory-row
   defaults (`tier`, `consolidatedFrom`), not through the fold hook.
   `maintenance_sweep_tier2` drives `collapseStaleChatAssets` only
   (`maintenance-sweep-tier2.ts:76`); the family that runs
   `runScheduledMaintenance` is **`maintenance_ops_tier2`**
   (`maintenance-ops-tier2.ts:114`) — that one gains the `foldOtherCatchup`
   summary key.
5. **`memory_pipeline_jobs_tier3`'s canned LLM will ANSWER the fold-grain
   call.** Its classifier (`memory-pipeline-jobs-tier3.test.ts:220-236`)
   falls through to the `\nCONTEXT\n(SUBJECT|OBSERVER): ` regex, which the
   fold prompt (GRAIN block + the same OTHER body) matches → classified
   `other`, answered with the per-turn OTHER rule → the fold OTHER pass WRITES
   memories in the oracle on every CONTEXT_SUMMARY op. That family becomes the
   natural end-to-end check of the fold hook; it cannot stay green without it.
   `context-summary-service-tier3.test.ts`'s classifier
   (`:207-229`, `startsWith` only) does NOT recognise the GRAIN prompt → v4's
   call fails as unrecognised (WARN `[FoldOtherPass] OTHER extraction failed`)
   and the watermark STILL advances (only timeouts hold it) → a `chats` column
   write either way.
6. **No v5 analog needed for `patchOnlyFields`.** v4 makes
   `otherExtractionWatermarkMessageId` patch-only (`chats.repository.ts:560-564`)
   because v4's `update` is read-merge-write-whole-row. v5's `ChatUpdate`
   (`db/chats.rs:514`, `update` `:1035`) is column-scoped — only `Some` fields
   are set — so a stale write can never rewind the watermark. Record, don't port.
7. **`MEMORY_REGENERATE_CHAT` has no v5 handler** (`spine.rs:3848-3920`
   registers neither `MEMORY_REGENERATE_CHAT` nor `MEMORY_REGENERATE_ALL`;
   `job_runner.rs:159` lists it in `KNOWN_JOB_TYPES` → the loud fallback).
   `rebuildFoldGrainObservations` has nowhere to land → DEFER by name with the
   handler port.
8. **The consolidation watermark trigger has no v5 hook shape.** v4 runs it in
   the dispatcher's post-commit hook over the child's buffered `writes`
   (`method === 'memories.create'`, `args[0].characterId`). v5 has no child and
   no write list (`job_runner.rs:438-485` sees only the outcome). A design
   ruling is needed (§B.6.4) — feature is OFF by default, so this can ship as a
   recorded, ruled shape.

---

# PART A — the extraction grain

## A.1 v4 at HEAD — file:line facts

### A.1.1 Settings — `lib/schemas/settings.types.ts` + `lib/instance-settings/index.ts`

- `settings.types.ts` (new block after `MemoryRecallSettings`):
  `MemoryExtractionModeSettingsSchema = z.object({ otherPass:
  z.enum(['turn','fold','hybrid']).default('hybrid'), perTurnOtherFloor:
  z.number().min(0).max(1).default(0.75), foldCandidatesPerSubject:
  z.number().int().positive().default(3) })`. Key order = that order (MEASURED
  `parse({})` → `{"otherPass":"hybrid","perTurnOtherFloor":0.75,
  "foldCandidatesPerSubject":3}`).
- `instance-settings/index.ts`: `KEY_MEMORY_EXTRACTION_MODE =
  'memoryExtractionMode'`; `DEFAULT_MEMORY_EXTRACTION_MODE_SETTINGS =
  Schema.parse({})`; `getMemoryExtractionModeSettings()` =
  `readJsonSetting(key, schema, defaults)` — `:140-155`: `raw === null` →
  defaults; `schema.parse(JSON.parse(raw))` throws → WARN
  `` `[InstanceSettings] ${key} failed to parse — using defaults` `` `{error}` →
  defaults (it NEVER throws for a bad value; only the underlying `readSetting`
  can throw). `setMemoryExtractionModeSettings(partial)` = `{...current,
  ...value}` → `writeJsonSetting` (`:161-165`: `schema.parse(value)` then
  `writeSetting(key, JSON.stringify(validated))`, returns validated).
- No DDL (instance_settings is a k/v table). **Keystone-owned accessor** (§C).

### A.1.2 The per-turn pass — `lib/memory/memory-processor.ts`

- `:26-34` import swap `loadCanonForSelf` → `loadCanonForSelfWithCommonplace`;
  `:49` `getMemoryExtractionModeSettings` import.
- Visibility only: `RateLimitDecision` / `resolveExtractionRateLimit` (`:57`),
  `applyImportanceFloor` (`:93`), `writeCandidate` (`:255`), `WriteOptions`
  become exported (the fold pass reuses them).
- `:219-235` NEW `CandidateWriteContext { chatId, userId, projectId?,
  sourceMessageTimestamp?, timelineMode?, dryRun?, inAutonomousRoom?,
  transcript?: { turnTimestamp? } }`; `WriteOptions.ctx` retyped to it;
  `writeCandidate`'s anchor chain becomes `opts.sourceMessageCreatedAt ??
  opts.ctx.sourceMessageTimestamp ?? opts.ctx.transcript?.turnTimestamp ??
  new Date().toISOString()` (optional chain added — the fold pass passes
  `transcript: { turnTimestamp: clock.nowIso }`).
- `:521-531` SELF canon: `renderSelfCanonBlock(await
  loadCanonForSelfWithCommonplace({ id, name, manifesto, personality,
  description, identity, mountPointId:
  observerCharacter?.characterDocumentMountPointId ?? null }))`.
- `:598-623` mode resolution, BETWEEN the SELF loop and the OTHER loop:
  `let otherPassMode = 'turn'; let perTurnOtherFloor = 0;` try `{ otherPass,
  perTurnOtherFloor } = await getMemoryExtractionModeSettings()` catch → WARN
  `'[Memory] Extraction-mode lookup failed; keeping per-turn OTHER pass'`
  `{ chatId, error }` (the default on FAILURE is `turn`, not `hybrid`). Then
  ALWAYS debug `'[Memory] OTHER pass mode resolved'` `{ chatId, otherPassMode,
  perTurnOtherFloor: otherPassMode === 'hybrid' ? perTurnOtherFloor : null }`.
  `runPerTurnOther = otherPassMode !== 'fold'`; when false,
  `debugLogs.push('[Memory] OTHER pass runs at fold grain only (otherPass=fold);
  skipping per-turn OTHER')` (persisted debug log — DB-visible).
- `:633` `for (const observer of runPerTurnOther ? allowedSlices : [])`.
- `:651` per-subject canon: `loadCanonForObserverAboutSubject(observerVault,
  subject)` — UNCHANGED call, but the callee now appends the observer's
  `Commonplace/<subject>.md` (finding 2).
- `:697-716` per subject: `let flooredCandidates = rawCandidates; if (hybrid) {
  flooredCandidates = applyImportanceFloor(rawCandidates, perTurnOtherFloor);
  if dropped → debugLogs.push(`[Memory] Hybrid floor dropped ${n} OTHER
  candidate(s) for ${observer.characterName} about ${subject.subjectName} below
  importance ${perTurnOtherFloor} (left to the fold-grain pass)`) }` THEN the
  throttle floor runs over `flooredCandidates` (its "dropped" count is now
  `flooredCandidates.length - candidates.length`). `applyImportanceFloor` =
  `(c.importance ?? 0.5) >= floor`.
- The SELF pass is NOT floored by the mode.

### A.1.3 The prompt — `lib/memory/cheap-llm-tasks/memory-tasks.ts`

- `:452-463` `FOLD_GRAIN_OTHER_BLOCK` (verbatim, 6 lines):
  `GRAIN — READ THIS FIRST` / `What follows is a STRETCH of conversation
  spanning many turns, not a single` / `exchange. State each thread ONCE, as it
  stands at the end of the stretch:` / `the settled outcome, not each beat on
  the way there. If a subject changed` / `their mind mid-stretch, record where
  they landed. Where the instructions` / `below say "this exchange" or "this
  turn", read "this stretch".`
- `:465-467` `otherBodyForCap(perSubjectCap, grain = 'turn')`: prefix
  `${FOLD_GRAIN_OTHER_BLOCK}\n\n` only for `'fold'`; the turn body is
  byte-identical to before.
- `:633-651` `getOtherMemoryExtractionPrompt(..., clock?, grain = 'turn')`
  threads `grain` into `otherBodyForCap`.
- `:1043` `export type FoldOtherMessage = FoldEpisodeMessage`
  (`{ speaker, content, createdAt }`).
- `:1055-1108` NEW `extractOtherMemoriesFromFold(windowMessages, observer
  {id,name,pronouns}, subjects, perSubjectCap, selection, userId,
  uncensoredFallback?, chatId?, resolvedMaxTokens?, inAutonomousRoom=false,
  orienting?, clock?)`:
  - empty window or no subjects → `{ success: true, result: Map(each subject →
    []), usage: undefined }` (no call);
  - `cap = Math.max(1, perSubjectCap)` (NOT token-clamped, unlike per-turn's
    `resolveMaxMemories`);
  - rendered message = `${stamp}${m.speaker}: ${content}` joined `\n\n`, where
    `stamp = m.createdAt ? `[${createdAt.slice(0,16).replace('T',' ')}] ` : ''`
    and `content = m.content.length > 1500 ? `${m.content.slice(0,1500)}…` :
    m.content` (UTF-16 units; `…` U+2026);
  - system = `getOtherMemoryExtractionPrompt(cap, formatNameWithPronouns(observer),
    subjects, inAutonomousRoom, orienting, clock, 'fold')`;
  - user = `` `CONVERSATION STRETCH (${n} messages, in the order spoken):\n\n${rendered}` ``;
  - `executeCheapLLMTask(selection, messages, userId, content =>
    parseOtherCandidatesBySubject(content, subjects, cap),
    'memory-extraction-other', chatId, undefined, uncensoredFallback,
    resolvedMaxTokens, observer.id)` — SAME task type as per-turn (so the LLM
    log type, activity kind and timeout class are unchanged).
- `cheap-llm-tasks/index.ts` re-exports; no new task type for this half.

### A.1.4 Canon — `lib/memory/cheap-llm-tasks/canon.ts` (322 lines)

- `:61` `CANON_BLOCK_TOKEN_CAP = 2500`; `:64` `COMMONPLACE_LABEL = '[FROM THE
  COMMONPLACE BOOK]'`.
- `CanonSource.commonplace?: string | null`, `SelfCanon.commonplace?`.
- `:108-115` `appendCommonplace(block, commonplace)`: `digest =
  commonplace?.trim()`; empty → `block`; `room = 2500 - estimateTokens(block) -
  estimateTokens(LABEL) - 2`; `fitted = truncateToTokenBudget(digest, room)`;
  null → `block`; else `${block}\n${LABEL}\n${fitted}`. (`estimateTokens` =
  `lib/tokens/token-counter.ts:81-93`: `ceil(ceil(len/3.5?)*1.05)` with the
  DEFAULT chars-per-token — v5 `token_estimation::estimate_tokens(text,
  DEFAULT_CHARS_PER_TOKEN)`, UTF-16 length.)
- `:123-146` `renderSelfCanonBlock`: `hasDigest = !!commonplace?.trim()`;
  `body = lines ? lines.join('\n') : hasDigest ? '' : NO_CANON_FALLBACK`;
  `head = body ? `ALREADY ESTABLISHED about ${name}\n${body}` : `ALREADY
  ESTABLISHED about ${name}``; return `appendCommonplace(head, commonplace)`.
  ⚠ edge: digest present but `truncateToTokenBudget` → null ⇒ the block is the
  bare head line with NO fallback line.
- `:153-169` `renderOtherCanonBlock`: the vault / identity / description arms
  unchanged, a NEW 4th arm `else if (commonplace?.trim()) return
  appendCommonplace(`ALREADY ESTABLISHED about ${name}`, commonplace)`; else
  fallback; every arm's result goes through `appendCommonplace`.
- `:181-194` `loadCanonForSelf` gains `commonplace?` and spreads it only when
  truthy.
- `:201-228` NEW `loadCommonplaceCanon(holder {characterId, mountPointId},
  subject 'self' | {id,name})`: no mount → null; `path =
  commonplacePathFor(subject)`; `readVaultTextFile(mountPointId, path,
  characterId)` → `commonplaceFileToCanonText(raw)`; when text → debug
  `'[Canon] Commonplace digest loaded into canon'` `{ characterId, path, chars:
  text.length }`; catch → debug `'[Canon] Commonplace digest unavailable; canon
  continues without it'` `{ characterId, path, error }` → null.
- `:233-247` NEW `loadCanonForSelfWithCommonplace({...card fields,
  mountPointId})` = `loadCanonForSelf({...character, commonplace: await
  loadCommonplaceCanon({characterId: id, mountPointId}, 'self')})`.
- `:255-271` `loadCanonForObserverAboutSubject(observer, subject, options = {})`
  → `hand = loadHandCanon(...)` (`:274-322`, the old body verbatim); `if
  (options.includeCommonplace === false) return hand`; `commonplace =
  loadCommonplaceCanon(observer, {id, name})`; `commonplace ? {...hand,
  commonplace} : hand`.

### A.1.5 Commonplace file helpers used by canon — `lib/memory/commonplace-file.ts` (166 lines, pure)

(Shared with Part B — see §B.1.5 for the renderer.)
- `:26` `COMMONPLACE_FOLDER = 'Commonplace'`, `:29` `COMMONPLACE_FRONTMATTER_TYPE
  = 'commonplace-digest'`, `:32` `COMMONPLACE_SELF_STEM = 'Self'`, `:35`
  `COMMONPLACE_FILE_TOKEN_BUDGET = 1500`.
- `:42-52` `commonplacePathFor('self')` → `Commonplace/Self.md`; else `stem =
  sanitizeFileName(name)`; `stem.toLowerCase() === 'self'` → `` `${stem}
  (${id.slice(0,8)})` ``; → `Commonplace/${stem}.md`.
- `:127-143` `commonplaceFileToCanonText(raw)`: falsy → null; if
  `startsWith('---\n')`: `close = indexOf('\n---', 4)`; if found `after =
  indexOf('\n', close + 4)`, `body = after >= 0 ? slice(after+1) : ''`; then
  drop every line matching `/^#\s/`, `join('\n').trim()`; empty → null.
- `:149-166` `truncateToTokenBudget(text, budget)`: `budget <= 0` → null; fits
  → text; else keep whole lines while `used + estimateTokens(`${line}\n`) <=
  budget`; any kept → `kept.join('\n')`; else proportional cut `text.slice(0,
  max(0, floor(len * budget/max(1,estimateTokens(text))) - 1)).trimEnd()` +
  `…`, or null when empty.

### A.1.6 The fold-grain pass — `lib/memory/fold-other-pass.ts` (634 lines, NEW)

Constants `:71-80`: `FOLD_OTHER_MAX_WINDOW_MESSAGES = 60`,
`FOLD_OTHER_CATCHUP_IDLE_MS = 2h`, `FOLD_OTHER_CATCHUP_LOOKBACK_MS = 30d`,
`FOLD_OTHER_CATCHUP_MAX_CHATS = 50`.

`RunFoldOtherPassInput` (`:82-100`): `chatId, userId, windowMessages:
MessageEvent[], cheapLLM, cheapMaxTokens?, uncensoredFallback?, timelineMode,
projectId?, inAutonomousRoom, projectDescription?, chatContextSummary?,
coverThroughMessageId?, dryRun?`. `FoldOtherPassResult` (`:102-114`):
`skippedReason?, observers, memoriesWritten, memoriesReinforced,
candidatesProposed, passesLostToTimeout, watermarkAdvancedTo, usage{prompt,
completion,total}, extractedCandidates?`.

- `:130-136` `isFoldOtherEligibleMessage(m)`: `type === 'message'`, role
  `USER|ASSISTANT`, no `systemSender`, has `participantId`, string content
  with non-empty trim.
- `:143-151` `messagesPastWatermark(all, wm)`: no wm → all; wm not found → all
  ("better to re-read the tail once than to go blind forever"); else after it.
- `:156-434` `runFoldOtherPass` — WHOLE body in try/catch; catch → WARN
  `'[FoldOtherPass] Fold-grain OTHER pass failed (non-fatal)'` `{chatId,
  error}`, `skippedReason = 'error'` — **it never throws**, so
  context-summary's ERROR line (A.1.7) fires only on a throw BEFORE the call.
  Steps:
  1. mode `turn` → debug `'[FoldOtherPass] otherPass=turn; fold-grain pass not
     run'` `{chatId}` → `skippedReason 'mode-turn'`. `perSubjectCap =
     max(1, foldCandidatesPerSubject)`.
  2. `chats.findById` null → `'no-chat'`.
  3. Watermark filter (`:174-192`) only when the chat HAS a watermark and the
     window is non-empty: `all = getMessages(chatId).filter(type==='message')`;
     if the watermark is found: keep window rows whose index in `all` >
     watermark index (unknown id → `Infinity` → kept); debug
     `'[FoldOtherPass] Applied watermark to window'` `{chatId, watermark,
     before, after}`; empty → `'behind-watermark'`.
  4. `coverThrough = input.coverThroughMessageId ?? window.at(-1)?.id ?? null`
     (computed on the FILTERED, un-eligibility-filtered window).
  5. `eligible = window.filter(isFoldOtherEligibleMessage)`; `trimmed =
     eligible.slice(-60)`; trimmed → debug `'[FoldOtherPass] Window trimmed to
     newest messages'` `{chatId, eligible, kept}`.
  6. Characters: every CHARACTER participant with `characterId` →
     `characters.findById` (overlay); throw → debug `'[FoldOtherPass] Character
     unavailable; leaving it out'` `{chatId, characterId, error}`.
     `userCharacter = resolveUserCharacterParticipant(participants,
     participantCharacters)`.
  7. Observers = characters who SPOKE in `trimmed`, first-appearance order
     (`seat.controlledBy === 'user'` → `isUser`); `lastMessageByCharacter` =
     each character's LAST trimmed message. Subjects = observers + the user
     character if silent (appended last, `isUser: true`).
  8. debug `'[FoldOtherPass] Pass set up'` `{chatId, mode, perSubjectCap,
     windowMessages, observers, subjects}`.
  9. `trimmed.length === 0 || observers === 0 || subjects < 2` →
     `skippedReason 'nothing-to-observe'` AND **the watermark is advanced
     anyway** (`advanceWatermark(coverThrough)`).
  10. `resolveSpeakerNames(chat)`; rendered `{speaker: speakerLabel(m, names),
      content: m.content ?? '', createdAt: m.createdAt ?? null}`;
      `clock = {nowIso: lastStamped?.createdAt ?? new Date().toISOString(),
      timelineMode}`; `orienting = {projectDescription ?? null,
      chatContextSummary ?? null}`; `limits = getMemoryExtractionLimits()`;
      `writeCtx = {chatId, userId, projectId ?? null, timelineMode,
      inAutonomousRoom, dryRun, transcript: {turnTimestamp: clock.nowIso}}`.
  11. Per observer: `rl = resolveExtractionRateLimit(observer.id, limits)`;
      `skip` → debug `'[FoldOtherPass] Observer rate-limited; skipping'`
      `{chatId, observer: name, recentCount, cap}`. Subjects ≠ observer each
      get `loadCanonForObserverAboutSubject(observerVault, {id, name,
      identity, description})` (Commonplace INCLUDED) → `canonBlock =
      renderOtherCanonBlock(canon)`, `canonSource = canon.source`. Zero
      resolved → continue. `extractOtherMemoriesFromFold(rendered, observer,
      resolved, perSubjectCap, cheapLLM, userId, uncensoredFallback, chatId,
      cheapMaxTokens, inAutonomousRoom, orienting, clock)`. Usage summed.
      `!success` → `timedOut` counts `passesLostToTimeout`; WARN
      `'[FoldOtherPass] OTHER extraction failed'` `{chatId, observer: name,
      timedOut: bool, error}`; continue.
  12. Per subject: `proposed = (bySubject.get(id) ?? []).slice(0,
      perSubjectCap)` (the parser's +1 anchored slot is cut); throttle floor;
      `candidatesProposed += kept`; debug `'[FoldOtherPass] Candidates for
      pair'` `{chatId, observer, subject, canon: canonSource, proposed, kept}`;
      each → `writeCandidate({characterId: observer.id, characterName,
      aboutCharacterId: subject.id, aboutCharacterName, pass: 'OTHER',
      candidate, passLabel: `fold OTHER memory ${observer} about ${subject}`,
      ctx: writeCtx, sourceMessageId: lastMessage?.id ?? null,
      sourceMessageCreatedAt: lastMessage?.createdAt ?? null, ...})` where
      `lastMessage = lastMessageByCharacter.get(observer.id) ?? trimmed.at(-1)`.
  13. `passesLostToTimeout === 0` → `advanceWatermark`; else WARN
      `'[FoldOtherPass] Passes lost to timeout; watermark not advanced'`
      `{chatId, passesLostToTimeout}`.
  14. INFO `'[FoldOtherPass] Fold-grain OTHER pass complete'` `{chatId,
      observers, candidatesProposed, memoriesWritten, memoriesReinforced,
      watermarkAdvancedTo}`.
- `:436-453` `advanceWatermark`: no id or dryRun → noop;
  `chats.update(chatId, {otherExtractionWatermarkMessageId: id})`; debug
  `'[FoldOtherPass] Watermark advanced'` `{chatId, messageId}`; catch → WARN
  `'[FoldOtherPass] Failed to advance watermark'` `{chatId, error}`.
- `:479-542` `findFoldOtherCatchupCandidates({now, limit})`: mode `turn` →
  debug `'[FoldOtherCatchup] otherPass=turn; no catch-up needed'` → empty.
  `chats.findAll()` → filter `!isHelpLikeChatType(chatType)`, `(messageCount ??
  0) >= 2`, `lastMessageAt` parses finite, `2h < now - last < 30d`; sort
  `last` DESC. Per chat: past `limit` → `deferred++` (messages never read);
  else `getMessages` (type message), `last` none → skip; `watermark ===
  last.id` → skip; `pickConnectionProfileId(chat)` (`:544-550`: first
  CHARACTER seat with `controlledBy !== 'user'` and a `connectionProfileId`,
  else any seat with one) null → debug `'[FoldOtherCatchup] No connection
  profile on any seat; skipping chat'` `{chatId}`; push `{chatId, userId,
  lastMessageId, connectionProfileId}`; catch → WARN `'[FoldOtherCatchup]
  Could not inspect chat; skipping'` `{chatId, error}`. debug
  `'[FoldOtherCatchup] Selection complete'` `{scanned, idleAndRecent,
  selected, deferred}`.
- `:568-634` `runFoldOtherCatchup({chatId, userId, connectionProfile,
  cheapLLMSettings, availableProfiles, ignoreIdle?, now?})`: no chat →
  `'no-chat'`; `!ignoreIdle && finite(lastAt) && now - lastAt <= 2h` → debug
  `'[FoldOtherCatchup] Chat active again since the sweep; leaving it to the
  fold'` → `'not-idle'`; `window = messagesPastWatermark(messages(type
  message), watermark)`; empty → `'behind-watermark'`; `cheapLLM =
  getCheapLLMProvider(profile, {strategy, userDefinedProfileId ?? undefined,
  fallbackToLocal}, availableProfiles, false)`; Concierge: `policy =
  resolveConciergeSettings(chatSettings, chat)`; `cheapLLM =
  resolveUncensoredCheapLLMSelection(cheapLLM, shouldUseUncensoredRoute(chat),
  policy, availableProfiles)`; project description (read failure → null);
  debug `'[FoldOtherCatchup] Running catch-up pass'` `{chatId,
  windowMessages, watermark}`; `runFoldOtherPass({..., cheapMaxTokens:
  resolveMaxTokens(profile), uncensoredFallback: {conciergePolicy,
  availableProfiles}, timelineMode ?? 'realtime', projectId ?? null,
  inAutonomousRoom: chatType === 'autonomous', projectDescription,
  chatContextSummary: chat.contextSummary ?? null, coverThroughMessageId:
  messages.at(-1)?.id ?? null})`.

### A.1.7 The fold hook — `lib/chat/context-summary.ts`

- `:24` import; `:571-594`, AFTER the episode pass (`:549-567`) and BEFORE the
  summary cost event: `windowMessages = turnsToFold.flatMap(t => t.messages)`
  (full `MessageEvent`s incl. `type`/`systemSender`), `chatSettingsForOther =
  repos.chatSettings.findByUserId(userId)`, `otherPolicy =
  resolveConciergeSettings(chatSettingsForOther, chat)`, `runFoldOtherPass({
  chatId, userId, windowMessages, cheapLLM, cheapMaxTokens:
  resolveMaxTokens(connectionProfile), uncensoredFallback: availableProfiles ?
  {conciergePolicy: otherPolicy, availableProfiles} : undefined, timelineMode:
  chat.timelineMode ?? 'realtime', projectId: chat.projectId ?? null,
  inAutonomousRoom: chatType === 'autonomous', chatContextSummary:
  newSummary })`; catch → ERROR `'[Context Summary] Fold OTHER pass failed:'`
  `{chatId}` + the Error. No help-chat exclusion at the hook (help chats fold
  too; measure what a help fold's participants yield).
- v4 jest tests mock it (`context-summary-fold-title.test.ts`,
  `-speaker-names.test.ts`: `jest.mock('@/lib/memory/fold-other-pass', () =>
  ({ runFoldOtherPass: jest.fn(async () => ({})) }))`) — the HARNESS oracles do
  not (finding 5).

### A.1.8 The MEMORY_EXTRACTION handler branch — `handlers/memory-extraction.ts`

- `:14` import; `:44-72` AFTER the connection-profile throw (`Connection
  profile not found: ${id}`) and the chat-settings throw (`Chat settings not
  found for user: ${userId}`), BEFORE the chat read: `if
  (payload.foldOtherCatchup)` → `availableProfiles =
  connections.findByUserId(userId)`; `runFoldOtherCatchup({chatId, userId,
  connectionProfile, cheapLLMSettings: chatSettings.cheapLLMSettings,
  availableProfiles, ignoreIdle: payload.foldOtherIgnoreIdle === true})`; INFO
  `'[MemoryExtraction] Fold-grain OTHER catch-up processed'` `{jobId, chatId,
  skippedReason: ?? null, observers, created, reinforced,
  watermarkAdvancedTo}`; `passesLostToTimeout > 0` → throw
  `new CheapLLMTaskLostError('fold-other-catchup', `${n} pass(es) timed out`)`
  ⇒ job error bytes `Cheap LLM task "fold-other-catchup" timed out and was not
  retried successfully: N pass(es) timed out` (`core-execution.ts:819-826`;
  v5 bytes at `cheap_llm_exec.rs:406`); return.

### A.1.9 The enqueue side — `lib/background-jobs/queue-service.ts`

- `:76-89` `MemoryExtractionPayload` gains `foldOtherCatchup?: boolean`,
  `foldOtherIgnoreIdle?: boolean`.
- `:539-543` dedupe predicate gains `&& (existing.foldOtherCatchup ?? false)
  === (payload.foldOtherCatchup ?? false)`. (`skipDedupCheck` arm `:526-528`
  pre-existing.) Payload stored = the caller's object literal: the sweep's
  `{chatId, turnOpenerMessageId: null, extractionAnchorMessageId:
  lastMessageId, connectionProfileId, foldOtherCatchup: true}`
  (`fold-other-catchup.ts:41-47`); the regenerate's adds `foldOtherIgnoreIdle:
  true` and `extractionAnchorMessageId: `regenerate:${job.id}`` with
  `{skipDedupCheck: true}`.

### A.1.10 The sweep — `maintenance/fold-other-catchup.ts` (71) + `scheduled-maintenance.ts`

- `enqueueFoldOtherCatchups({now?, limit?})`: per candidate
  `enqueueMemoryExtraction(userId, {...as above})` → debug `'Enqueued
  fold-grain OTHER catch-up'` `{chatId, lastMessageId}`; catch → WARN `'Failed
  to enqueue fold-grain OTHER catch-up'` `{chatId, error}`. `deferred > 0` →
  INFO `'Fold-grain OTHER catch-up capped; remaining chats wait for the next
  sweep'` `{enqueued, deferred, limit}`, else debug `'Fold-grain OTHER
  catch-up sweep done'` `{enqueued}`. Logger child `module:
  'maintenance.fold-other-catchup'`. Returns `{enqueued, deferred}`.
- `scheduled-maintenance.ts:82` `MaintenanceSweepSummary.foldOtherCatchup:
  {enqueued, deferred}` (key AFTER `orphanedThumbnailsSwept`, BEFORE
  `failures`); `:199` init `{0,0}`; `:256-260` sweep **8**, LAST, after the
  orphaned thumbnails: `runSweep(summary, 'fold-other-catchup', 'Fold-grain
  OTHER catch-up failed — continuing', …)` — failure name
  `fold-other-catchup` pushed to `failures`.

### A.1.11 The regenerate rebuild — `handlers/memory-regenerate-chat.ts` (164)

- `:56` after `deleteMemoriesByChatIdWithVectors`: `rebuildFoldGrainObservations
  (job, payload, chat.otherExtractionWatermarkMessageId ?? null)` (`:128-164`):
  mode `turn` → debug `'[MemoryRegenerateChat] Extraction mode is per-turn; no
  fold-grain rebuild'` `{jobId, chatId}`; else `chats.update(chatId,
  {otherExtractionWatermarkMessageId: null})` ONLY when a watermark existed,
  enqueue the catch-up above, debug `'[MemoryRegenerateChat] Reset fold-grain
  watermark and enqueued catch-up'` `{jobId, chatId, previousWatermark,
  otherPass}`. **v5: no handler (finding 7) → DEFER.**

## A.2 v5 today — counterparts

| v4 | v5 | state |
|---|---|---|
| mode settings accessor | `db/instance_settings.rs` (template: `get_memory_recall_settings` `:196`, `read_json_setting`, parse fn `:210`) | MISSING (keystone) |
| `memory-processor.ts` | `services/memory_processor.rs` (1,199; P4.6bj) — SELF canon `:927`, OTHER loop `:1042-1199`, throttle `:1138-1155`, `write_candidate` `:531` takes `&TurnMemoryExtractionContext` | no mode read, no hybrid floor, no Commonplace canon, write ctx not generalized |
| OTHER canon loader | `memory_processor.rs:110-163` `load_canon_for_observer_about_subject` (private) + `read_vault_text_file` `:76` | hand canon only |
| `canon.ts` renderers | `canon.rs` (132) — `SelfCanon`/`CanonSource` without `commonplace`; `render_*` use Rust `trim()` | no Commonplace arm / cap |
| `commonplace-file.ts` | — | MISSING |
| prompt grain | `memory_tasks.rs:258` `other_body_for_cap`, `:310` `get_other_memory_extraction_prompt`, `:531` `build_other_extraction_messages`; text in `memory_tasks/prompt_text.rs` | no `grain`, no fold builder |
| `fold-other-pass.ts` | — (template: `services/fold_episode_pass.rs` 490, P4.36/P4.144) | MISSING |
| fold hook | `services/context_summary.rs` `ContextSummarySeams` trait `:220-265` (impls `NoopSeams` `:271`, `FoldEpisodePassSeams` `:310-363`, `RealContextSummarySeams` `:368-480`, all in this file); episode call `:880-925`; window built from `rows_by_id` raw `Value`s | no fold-OTHER seam |
| extraction handler | `services/memory_extraction_job.rs` (438) — `MemoryExtractionPayload` `:53-85` (4 fields), `handle_memory_extraction` `:90`; host wrapper `quilltap-host/src/spine.rs:3228-3275` decodes and passes `read_memory_extraction_limits(db)` | no catch-up branch |
| enqueue + dedupe | `services/queue_service.rs:308-358` `enqueue_memory_extraction(db, user, chat, opener, anchor, profile)` — JSON literal of 4 keys; `skipDedupCheck` "not modeled" | no fold flags |
| sweep | `services/scheduled_maintenance.rs` — `MaintenanceSweepSummary` `:80-118`, sweep 7 `:348-361`, the summary INFO `:380+` | no sweep 8 |
| speaker names / user char | `services/speaker_names.rs:82/117`, `services/turn_transcript.rs:74` | present |
| `resolveMaxTokens(profile)` | `context_budget.rs:100 resolve_max_tokens(profile_max_tokens, model_class)` | present |
| `getCheapLLMProvider` / uncensored | `cheap_llm.rs:263/344` | present |
| regenerate-chat handler | none (`spine.rs:3848-3920`) | ABSENT → DEFER |

Ported by: memory processor + extraction job P4.6bj (and the episodic-recall
campaign); fold episode pass P4.36 (+ P4.144 log lines); context summary W4.6b /
Round-3 Group 7; scheduled maintenance P4.24/P4.D-series.

## A.3 What the ledger / commit prose got wrong (extraction half)

- "self-canon now reads `Commonplace/<Subject>.md`" — conflated (finding 2):
  SELF reads `Commonplace/Self.md`; OTHER reads `Commonplace/<Subject>.md`.
- "ERROR `[Context Summary] Fold OTHER pass failed:` on failure" — that line
  fires only if `chatSettings.findByUserId` / `resolveConciergeSettings` throws;
  the pass's own failures are WARNs inside it (`:427`, `:356`, `:411`, `:448`).
- "a NEW cheap-LLM call per observer" — only when ≥ 1 observer spoke in the
  eligible window AND ≥ 2 subjects; otherwise the fold writes ONLY the
  watermark (`nothing-to-observe`), still a `chats` UPDATE.
- Predicted-red list: add `carina_memory_extraction_tier3`,
  `maintenance_ops_tier2`, `courier_images_routes` (its summary-check fold);
  drop `maintenance_sweep_tier2`; `fold_episode_tier3` reddens only via
  keystone row defaults (finding 4).
- Not wrong but load-bearing: "the per-turn OTHER candidates below 0.75 are
  DROPPED on every turn" is exactly true — and the fold pass that would
  re-capture them only runs at a fold or at the 2-hour idle sweep, so a
  v5 build without the fold half LOSES those observations outright. **The two
  must land together** (the floor without the fold pass is a regression).

## A.4 Differential plan (extraction half)

Existing families, predicted at the target:
- `memory_processor_tier3_equivalence` — RED wherever a corpus OTHER candidate
  has importance < 0.75 (the fixture's seed memories span 0.3–0.9; the canned
  OTHER answers must be measured) and on every op's persisted debug logs
  (`[Memory] Hybrid floor dropped …`). Mode is read by v4's REAL
  `getMemoryExtractionModeSettings` (the case does not mock
  `@/lib/instance-settings`) → defaults to `hybrid` on a fixture with no row.
- `memory_pipeline_jobs_tier3_equivalence` — RED: hybrid floor + the fold
  OTHER call ANSWERED by the `other` rule on every CONTEXT_SUMMARY op
  (finding 5) + the watermark column.
- `context_summary_service_tier3_equivalence` — RED: watermark write; plus an
  unrecognised-prompt failure unless the corpus grows a `fold-other` rule
  (recommended: add one, so the fold-OTHER call is pinned, not merely failed).
- `orchestrator_tier3`, `help_chat_orchestrator_tier3`, enclave step,
  `courier_images_routes` — RED wherever their in-loop summary check folds
  (they drive `FoldEpisodePassSeams`); measure which corpora fold.
- `carina_memory_extraction_tier3` — measure (mode debug line is tracing-only;
  red only if its synthetic transcript yields OTHER subjects).
- `canon_scenario_equivalence` (tier-1) — stays green on old rows; GROW it
  with Commonplace rows (both renderers × digest present/absent × budget
  exhausted × the SELF no-fields-with-digest arm × the OTHER 4th arm).
- `memory_tasks_equivalence` (tier-1) — grow with `grain: 'fold'` prompt bytes
  + the fold user-message builder (stamp slice, 1500-unit cut with `…`).
- `maintenance_ops_tier2_equivalence` — RED: the summary gains
  `foldOtherCatchup`; plus any wall-clock-eligible chat enqueues a
  MEMORY_EXTRACTION catch-up row (the case uses `runScheduledMaintenance()`
  with wall-clock cutoffs — measure the fixture's `lastMessageAt`s against
  2 h / 30 d).
- `instance_settings_json_warns_equivalence` — grow with the
  `memoryExtractionMode` key (the WARN's `${key}` bytes).

New cases:
- **tier-1 `commonplace-file`** (path / canon-text / truncate; renderer in B).
- **tier-3 `fold-other-pass`** — v4's REAL `runFoldOtherPass` +
  `runFoldOtherCatchup` + `findFoldOtherCatchupCandidates` over a fixture
  (multi-character chats with/without watermarks, a deleted-watermark chat, a
  help chat, a 1-message chat, a user-character seat, a Commonplace mirror in
  an observer vault, a rate-limited observer, a timed-out call, a 70-message
  window), mocked LLM by prompt class, diff `memories` / `chats` /
  `llm_logs` + the captured `[FoldOtherPass]` / `[FoldOtherCatchup]` lines.
  Mirror v4 jests: `fold-other-pass.test.ts` (194),
  `memory-processor-other-mode.test.ts` (109), `canon-commonplace.test.ts`
  (221), `fold-other-catchup.test.ts` (69, `7e9eaf42c`).
- **tier-2 enqueue** — the catch-up payload bytes + the dedupe term (a
  per-turn job and a catch-up job with the same `(chat, null, anchor)` both
  enqueue).

## A.5 Proposed units — EXTRACTION lane (file-disjoint)

| # | unit | v5 files (owned) |
|---|---|---|
| E1 | prompt grain: `FOLD_GRAIN_OTHER_BLOCK`, `grain` param on `other_body_for_cap` / `get_other_memory_extraction_prompt`, NEW `build_fold_other_extraction_messages` + `extract_other_memories_from_fold` (cap `max(1,n)`, no token clamp, task type `memory-extraction-other`, `chat_id` + observer id) | `memory_tasks.rs`, `memory_tasks/prompt_text.rs`, `services/memory_processor.rs` (the async wrapper beside `extract_other_memories_from_turn`) |
| E2 | Commonplace canon: `commonplace` on `SelfCanon`/`CanonSource`, `CANON_BLOCK_TOKEN_CAP`, `append_commonplace`, both renderers' new arms; `load_commonplace_canon`, `load_canon_for_self_with_commonplace`, `include_commonplace` on the OTHER loader (made `pub` for B) + the two `[Canon]` debug lines | `canon.rs`, `services/memory_processor.rs` (loader) — consumes keystone `commonplace_file.rs` |
| E3 | per-turn: mode read (DB, failure → `turn` + WARN), the debug line, the `fold`-mode skip + its debug-log line, the hybrid floor before the throttle + its debug-log line, SELF canon via E2 | `services/memory_processor.rs` |
| E4 | `CandidateWriteContext` generalization of `write_candidate` (transcript optional) — pure refactor, proven by E3/E5 diffs | `services/memory_processor.rs` |
| E5 | `services/fold_other_pass.rs` NEW: eligibility, `messages_past_watermark`, `run_fold_other_pass` (all lines), `advance_watermark` | NEW `services/fold_other_pass.rs`, `services/mod.rs` (one line) |
| E6 | fold hook: a `run_fold_other_pass` method on `ContextSummarySeams` (Noop no-op; `FoldEpisodePassSeams` + `RealContextSummarySeams` LIVE), called after the episode pass with the raw window `Value`s, `resolve_max_tokens(profile)`, the Concierge policy, `new_summary`; ERROR line on a pre-call throw | `services/context_summary.rs` |
| E7 | catch-up: `find_fold_other_catchup_candidates`, `pick_connection_profile_id`, `run_fold_other_catchup`; the MEMORY_EXTRACTION branch (+ payload decode of the two flags, INFO line, the lost-error) | `services/fold_other_pass.rs`, `services/memory_extraction_job.rs` |
| E8 | sweep 8: `enqueue_fold_other_catchups` + summary field + failure name + the summary INFO's new fields | NEW `services/fold_other_catchup_sweep.rs` (or inside `fold_other_pass.rs`), `services/scheduled_maintenance.rs` |
| E9 (DEFER) | regenerate rebuild — recorded deferral naming `memory-regenerate-chat.ts:128-164`, lands with the MEMORY_REGENERATE_CHAT handler | — |
| R1 (rider) | Brahma SQL prompt paragraph (`brahma-sql-prompt.ts:62` — the `source` triple + the `tier`/`supersededById`/`consolidatedFrom` clause, verbatim) | `services/brahma_console/prompt_text.rs` (`:59`) |
| R2 (rider) | character optimizer hot-only: vector `search(…, 500, isHotVector)` + `candidateMemories.filter(m => m.tier !== 'cold')` after all three arms (`character-optimizer.service.ts:738/763`) | `generators/optimizer.rs` (`:1644`, `:1777-1810`) — consumes the keystone/recall `search_filtered` + `tier` |

R1/R2 are recall-side riders the brief parked here; either lane can carry them
(they share no file with the rest). Default: the extraction lane.

---

# PART B — the consolidation engine (PORT-NEW)

## B.1 v4 at HEAD — file:line facts

### B.1.1 Settings — `settings.types.ts` + `instance-settings/index.ts`

`MemoryConsolidationSettingsSchema` (key `instance_settings
['memoryConsolidation']`), declaration order = wire order (MEASURED
`parse({})`): `enabled` bool `false`; `connectionProfileId` string nullable
`null`; `clusterThreshold` number [0,1] `0.72`; `minClusterSize` int ≥2 `3`;
`maxClusterSize` int ≥2 `30`; `matureAfterDays` number ≥0 `7`;
`maxClustersPerRun` int >0 `40`; `watermark` int >0 `150`;
`coldRetentionDays` int >0 nullable `null`. Accessors
`getMemoryConsolidationSettings` / `setMemoryConsolidationSettings(partial)`
(merge `{...current, ...value}` → validated write). Zod int message MEASURED:
`{"expected":"int","format":"safeint","code":"invalid_type","path":
["minClusterSize"],"message":"Invalid input: expected int, received number"}`.
`coldRetentionDays` is consumed by HOUSEKEEPING (other cluster), not here.

### B.1.2 Clustering — `lib/memory/consolidation-clustering.ts` (391, pure, no logs)

- `:38-41` `DAY_MS`, `EPISODIC_WINDOW_MS = DAY_MS`.
- `ClusterItem {id, embedding: Float32Array, kind, isDigest, eventTimeMs|null,
  createdAtMs, consideredAtMs|null, weight}`; `ClusterParams {threshold,
  maxClusterSize, episodicWindowMs?}`; `Cluster {kind, digestId|null,
  memberIds, score}`.
- `:82-87` `cosine(a,b)`: length mismatch or 0 → `-1`; else plain dot (f64
  accumulation of f32 elements).
- `:113-121` `clusterBucket` — semantic group first, then episodic; never mixed.
- `:123-300` `clusterGroup`: `maxSize = max(1, maxClusterSize)`. Pairwise
  `sim = new Float32Array(n*n)` (**values rounded to f32** — Rust must store
  `as f32` and compare `(s as f64) < threshold`), diagonal 1.
  `pairTimeOk` only for episodic, null times always OK, `|a-b| <= window`.
  **Step 1 digest attraction:** for each non-digest row, its best neighbour
  over ALL items (rows AND digests) with `s >= threshold` and time OK, strict
  `>` for ties (first index wins); if that best is a digest → attraction
  `{row, digest, s}`. `attractions.sort((a,b) => b.s - a.s)` (stable). Apply:
  skip if `members.length + 1 >= maxSize`; skip if the episodic span would
  exceed the window; else join. Result clusters for digests with ≥1 member:
  `score = digest.weight + Σ member weights`, digest order = index order.
  **Step 2 average-linkage** over the un-attracted rows: `M` Float32Array
  copy; `eligible(a,b)` (`a≠b`, both active, `size[a]+size[b] <= maxSize`,
  `M >= threshold`, episodic span fits); `best`/`bestVal` (Float64Array of f32
  values) cache with strict `>`; loop: pick the active `a` with the highest
  `bestVal` (strict `>` — lowest index on ties); stale cache → `recompute(a)`;
  merge b into a with Lance–Williams `(sa*M[a,c] + sb*M[b,c])/(sa+sb)` (f64
  math, stored to f32), `members[a] = members[a].concat(members[b])`, spans
  merged, `recompute(a)` + every `c` whose best was a or b. Output active
  clusters in index order (singletons INCLUDED).
- `:331-338` `effectiveMinClusterSize(createdAts, {minClusterSize,
  matureAfterDays, nowMs})`: `floor = max(2, floor(minClusterSize))`; any
  member created before `now - 2*matureAfterDays*DAY` → `2` else `floor`
  (the documented reinterpretation — spec §9 C3).
- `:345-350` `clusterHasNewMaterial`: any `consideredAtMs === null` → true;
  else any `createdAtMs > min(considered)`.
- `:357-391` `selectClusters`: per cluster: rows = memberIds mapped through
  `itemsById` (missing dropped); no new material → `stale`; `size = rows +
  (digestId ? 1 : 0)`; `< floor` → `belowMin`; else `qualified`.
  `qualified.sort((a,b) => b.score - a.score)` (stable); `budget =
  max(0, floor(maxClusters))`; `selected = slice(0,budget)`, `deferred =
  slice(budget)`.

### B.1.3 Plan — `lib/memory/consolidation-plan.ts` (476, pure)

- `:50` `DIGEST_REINFORCEMENT_CAP = 50`; `:53` `IMPORTANCE_FLOOR = 0.2`;
  `:56-57` `MAX_DIGEST_ENTITIES = 12`, `MAX_FREE_KEYWORDS = 8`.
- Helpers: `eventTimeIso = occurredAt ?? createdAt`; `referenceTimeIso` =
  `lastReinforcedAt` when it parses later than `createdAt`, else `createdAt`;
  `sharedValue` (`:155-160` — first falsy → null; all `(v ?? null) === first`);
  `deriveWitnessedContext` (`:166-174` — shared; else `'user_present'` if any;
  else null); `clampDigestImportance` (`:177-184` — band [0.2,1] then
  [min,max] of members' `reinforcedImportance ?? importance`);
  `normalizeDigestKeywords` (`:207-243` — lowercase/trim; `scope:` prefix →
  `SCOPE_VALUES`; `TEMPORAL_VALUES`; `CONTEXT_VALUES`; free words deduped,
  first 8; then `temporal` (model's, else episodic → `'past'`, else members'
  majority, fallback `'present'`), `` `scope: ${scope}` `` (fallback `'wide'`),
  `context` (fallback `'information'`); `majority` = first value reaching the
  strictly highest count in Map insertion order); `union(lists, cap?)` dedupes
  by `toLowerCase()`, keeps first spelling.
- `:268-476` `planConsolidationWrites({characterId, clusters, rowIndex, nowIso,
  newId})`:
  - Pass 1: per cluster per digest `id = (k===0 && existingDigest) ?
    existing.id : newId()`; `supersededBy` / `digestOfMember` Maps (insertion
    order matters for `tierMoves` — a re-`set` keeps the ORIGINAL position,
    so Rust needs `IndexMap::insert` semantics); contradictions: `target =
    digestOfMember(newer) ?? newer`, `target === older` skip, else
    `supersededBy.set(older, target)`. `remap` follows ≤ 5 hops.
  - Pass 2 per digest: `kind = clusterKind === 'episodic' ? 'episodic' :
    digest.kind`; `rows = existing ? [existing, ...members] : members`;
    `reinforcementCount = min(50, max(1, Σ(count ?? 1)))`;
    `reinforcedImportance = calculateReinforcedImportance(importance, count)`
    (v5 `memory_gate.rs:45`); links = `union([rows.flatMap(related).map(remap)
    .filter(id !== self && (rowIndex.has || planned digest))])`;
    `earliestRow = [...rows].sort((a,b) =>
    eventTimeIso(a).localeCompare(eventTimeIso(b)))[0]` (**ICU
    `localeCompare`** — use v5 `collation::locale_compare`); `occurredAt` /
    `narrativeTime` only for episodic; keywords / entities (cap 12) / tags
    unions; `chatId`/`projectId` = `sharedValue`; `consolidatedFrom =
    union([existing?.consolidatedFrom ?? [], memberIds])`;
    `lastReinforcedAt = isUpdate ? nowIso : memberRows.map(referenceTimeIso)
    .sort().at(-1) ?? nowIso` (code-unit sort).
  - Update `patch` key order: `content, summary, keywords, tags, importance,
    reinforcementCount, reinforcedImportance, lastReinforcedAt,
    relatedMemoryIds, consolidatedFrom, consolidatedAt: nowIso, occurredAt,
    narrativeTime, entities, chatId, projectId, witnessedContext`.
  - Create `data` order: `characterId, aboutCharacterId, chatId, projectId,
    content, summary, keywords, tags, importance, source: 'CONSOLIDATED',
    witnessedContext, occurredAt, narrativeTime, entities, kind,
    sourceMessageId: null, lastAccessedAt: null, reinforcementCount,
    lastReinforcedAt, relatedMemoryIds, reinforcedImportance, tier: 'hot',
    supersededById: null, consolidatedFrom, consolidatedAt: nowIso`.
  - Pass 3: `tierMoves` grouped by `remap(target)` in `supersededBy` order
    (digests never move); `considered = union(keepStandalone not superseded)`;
    `linkRewrites` for every `rowIndex` row (insertion = load order) that is
    neither superseded nor a digest and has a superseded link → `union(links
    .map(remap).filter(≠ self))`.

### B.1.4 Tasks (the one new prompt family) — `cheap-llm-tasks/consolidation-tasks.ts` (379)

- `:38` `CONSOLIDATION_TASK_TYPE = 'memory-consolidation'`; `:41` max tokens
  `4000`; `:44` member char cap `1200`.
- Schemas `:50-76`: digest `{content trim min1, summary trim min1, keywords
  string[] default [], importance number [0,1], kind enum default 'semantic',
  occurredAt string nullable optional, memberIds (trim min1)[] min1}`;
  contradiction `{olderId, newerId trim min1, note default ''}`; output
  `{digests[], keepStandalone default [], contradictions default []}`.
- `:101-159` `validateConsolidationOutput(raw, handles)` — reasons
  (verbatim): `` `schema: ${path.join('.') || '(root)'} ${issue.message}` ``
  (FIRST issue only; Zod 4 message text — v5 `api/zod_issues.rs`),
  `` `digest ${d+1} names unknown member "${h}"` ``, `` `member "${h}" listed
  twice (${prior} and digest ${d+1})` ``, `` `keepStandalone names unknown
  member "${h}"` ``, `` `member "${h}" listed twice (${prior} and
  keepStandalone)` `` (a repeat INSIDE keepStandalone is ignored);
  `unlisted` (input order) appended to `keepStandalone`; contradictions
  filtered to known handles, `older !== newer`.
- `:170-240` `CONSOLIDATION_SYSTEM_PROMPT` — 71 lines, byte-stable, the
  "binder of a character's Commonplace Book" text (copy verbatim; the tier-1
  family pins it).
- `:268-277` `voiceLine`: self → `` `VOICE: first person — the subject is the
  holder, ${holderName}; write "I …".` ``; other → `` `VOICE: third person —
  write about ${subjectName} by name.` ``; none → `VOICE: third person — these
  notes have no single subject; name whoever each fact concerns.`
- `:279-282` `oneLine` = `replace(/\s*\n\s*/g,' ').trim()` then 1200-unit cap
  + `…`.
- `:285-316` user message lines: `CONTEXT`, `HOLDER: …`, `SUBJECT: X (the
  holder)` | `SUBJECT: X` | `SUBJECT: (none — general notes)`, voice line,
  `CLUSTER: EPISODE — dated notes of something that happened within one day.`
  | `CLUSTER: standing facts, preferences, and threads.`, `''`; canon block
  (trimmed) + `''` when present; `EXISTING DIGEST`, oneLine, `''` when
  present; `NOTES (oldest first)`; per member `` `${handle} | ${when ?
  when.slice(0,10) : 'undated'} | ${importance.toFixed(2)} |
  ${reinforcementCount} | ${oneLine(content)}` ``; joined `\n`.
- `:323-337` `parseConsolidationResponse` — `parseLLMJsonObject` throw →
  `` `unparseable JSON: ${message}` `` (never throws; v5
  `generators/llm_json.rs:234`).
- `:344-379` `consolidateMemoryCluster` — debug `'[Consolidation] Sending
  cluster to model'` `{characterId, bucket, clusterKind, members,
  hasExistingDigest, provider, model}`; `executeCheapLLMTask(selection,
  [system, user], userId, parser, 'memory-consolidation', chatId undefined,
  undefined, uncensoredFallback, 4000, characterId)`.
- `core-execution.ts:264` `'memory-consolidation': 'MEMORY_EXTRACTION'` (LLM
  log type) and `:466` `'memory-consolidation': 'memory'` (activity). No new
  timeout override.

### B.1.5 Commonplace file render — `commonplace-file.ts:74-121` (pure)

`renderEntry`: `content.replace(/\s*\n\s*/g,' ').trim()`; episodic with
`occurredAt` → `` `- [${occurredAt.slice(0,10)}] ${text}` ``, else `` `- ${text}` ``.
`renderCommonplaceFile`: heading `# Self` | `` `# ${subjectName.trim() ||
'Unnamed'}` ``; digests sorted `reinforcedImportance` DESC (stable); lines
`[heading, '']`, `tokens = estimateTokens(heading)`; add each line until
`written > 0 && tokens + cost > 1500`; frontmatter `---` / `type:
commonplace-digest` / `` `subjectCharacterId: ${JSON.stringify(id)}` `` /
`` `updatedAt: ${JSON.stringify(iso)}` `` / `---`; content =
`${frontmatter}\n${lines.join('\n')}\n`; returns `{content, entriesWritten,
entriesDropped}`.

### B.1.6 The vault bridge — `lib/file-storage/commonplace-digest-vault-bridge.ts` (171)

Child → host-RPC (`QUILLTAP_JOB_CHILD`, NO-PORT). In-process: `findByIdRaw`
null → debug `'Skipping Commonplace mirror: holder not found'` → `{written 0,
unchanged 0, skipped: files, skippedReason 'missing-character'}`; archived →
debug `'Skipping Commonplace mirror for archived character'` →
`'archived'`; `getCharacterVaultStore` null (`character-vault-bridge.ts:58-82`:
overlay `findById`, `mountType 'database'`, `storeType 'character'`; throw →
WARN `'Failed to resolve character vault store'`) → debug `'Skipping
Commonplace mirror: holder has no character vault'` → `'no-vault'`. Files
with ≥ 1 digest: `ensureFolderPath(mp, 'Commonplace')` once; `skipped +=`
empty files; per file render, `readDatabaseDocumentIfExists`; equal after
`stripUpdatedAt` (`/^updatedAt: .*$/m` → `''`) → `unchanged++`; else
`writeDatabaseDocument` → `written++`, debug `'Wrote Commonplace digest
mirror'` `{context, characterId, path, entriesWritten, entriesDropped}`;
per-file catch → `skipped++`, WARN `'Failed to write a Commonplace digest
mirror'`; outer catch → WARN `'Commonplace mirror failed'`; finally debug
`'Commonplace mirror complete'` `{context, characterId, written, unchanged,
skipped, skippedReason?}`. `context = 'file-storage.commonplace-digest-
vault-bridge'`.

### B.1.7 The service — `lib/memory/consolidation.ts` (1,067)

Logger child `module: 'memory:consolidation'`. Constants `:99`
`CONSOLIDATION_LOAD_PAGE_SIZE = 250`, `:106` `CONSOLIDATION_MAX_BUCKET_ROWS =
2000`. Report types `:115-226` (key orders below). `runConsolidation(characterId,
opts {dryRun, maxClustersPerRun, settings partial, userId, timeBudgetMs,
trigger, now})` `:711-1067`:

1. `settings = {...stored, ...opts.settings}`; `maxClusters =
   opts.maxClustersPerRun ?? settings.maxClustersPerRun`.
2. `findByIdRaw` null → INFO `'Consolidation skipped: character not found'`
   `{characterId, trigger}` → report `skippedReason 'character-not-found'`
   (characterName null); archived → INFO `'Consolidation skipped: character is
   archived'` → `'archived'` (name set). Holder = overlay `findById` ?? raw
   (throw → raw). `userId = opts.userId ?? holderRaw.userId`.
3. debug `'Consolidation starting'` `{characterId, trigger, dryRun,
   maxClusters, clusterThreshold, minClusterSize, maxClusterSize,
   matureAfterDays, timeBudgetMs: ?? null}`.
4. **Load** `:313-377` (`findByCharacterIdInBatches(id, 250)`, id ASC): every
   row → `rowIndex`; skip `(tier ?? 'hot') !== 'hot'`; `CONSOLIDATED` with an
   embedding → bucket digests (else `noEmbeddingSkipped`); non-`AUTO` skipped
   silently (MANUAL never); `createdAt` unparsable or `> now -
   matureAfterDays*DAY` → `immatureSkipped`; no embedding →
   `noEmbeddingSkipped`; else bucket rows + `candidates`. Bucket key `none` |
   `self` | `` `other:${about}` ``; `subjectExists = key !== 'none'`.
   `capBucketRows` `:380-390` (never-considered first, then
   `reinforcedImportance` DESC, first 2,000) → `bucketRowsCapped`.
5. **Cluster** per bucket with rows: items = `[...digests, ...rows]` (digests
   FIRST — index order matters for ties) → `toClusterItem` (`kind ??
   'semantic'`, `weight = reinforcedImportance ?? importance ?? 0.5`,
   `createdAtMs` unparsable → 0); debug `'Bucket clustered'` `{characterId,
   bucket, rows, digests, clusters}`; `selectClusters` → stats.
   `belowMinConsidered` = never-considered members of below-min clusters.
6. **Route** (only when ≥ 1 selected) `:399-425`: `chatSettings` + profiles;
   configured profile found → debug `'Consolidation using its configured
   connection profile'` `{connectionProfileId, provider, model}` →
   `selectionFromProfile(profile, {localBaseUrlFallback: true})`; missing →
   WARN `'Configured consolidation profile no longer exists; falling back to
   the cheap LLM'` `{connectionProfileId}`; else
   `selectCheapLLMFromProfiles(profiles, buildCheapLLMConfig(chatSettings))`;
   null → WARN `'Consolidation has clusters to fold but no model to fold them
   with'` `{characterId}` (→ `skippedReason 'no-llm'` at the end).
   `describeBuckets` `:465-542` for selected bucket keys: self → canon =
   `renderSelfCanonBlock(loadCanonForSelf(card fields))` (NO Commonplace);
   none → name `'(no subject)'`, canon null; other → overlay `findById` then
   `findByIdRaw`; missing → name `'an unnamed acquaintance'`,
   `subjectExists = false`; else OTHER canon with `{includeCommonplace:
   false}`; throw → debug `'Subject canon unavailable for consolidation;
   continuing without it'` `{characterId, subjectId, error}`.
7. **Consolidate** per selected cluster `:850-966`: time budget `Date.now() -
   startedMs > timeBudgetMs` → `budgetExhausted`, `clustersDeferred +=
   remaining`, INFO `'Consolidation time budget spent; remaining clusters
   wait for the next run'` `{characterId, remaining, timeBudgetMs}`, break.
   Members sorted by `eventTimeMs ?? 0` ASC; handles `m1…mN`; member `when =
   occurredAt ?? createdAt`, `importance = reinforcedImportance ??
   importance`, `reinforcementCount ?? 1`. Report pushed with `status
   'failed'` first. `routeForCluster` `:428-462` (Concierge over the members'
   chats incl. the existing digest's: first `locked` wins, else first
   `unmoderated`; `dangerous = shouldUseUncensoredRoute(policyChat)`;
   failover only when `policy.failoverAllowed`; chat read errors → null,
   cached). Call → `!success || !result` → `failed`, `error = result.error ??
   'no result'`, `clustersFailed++`, `timedOut` → `clustersLostToTimeout++`,
   WARN `'Consolidation call failed; cluster skipped'` `{characterId, bucket,
   members, timedOut, error}`; invalid → `'invalid'`, WARN `'Consolidation
   answer failed validation; cluster skipped'` `{…, reason}`; ok → outcome
   (handles → ids), `status = dryRun ? 'planned' : 'written'`, debug `'Cluster
   consolidated'` `{characterId, bucket, clusterKind, members, digests,
   standalone, unlisted, contradictions, policyChatId}`.
8. **Plan** with `newId = crypto.randomUUID`. Non-dry: embed every planned
   digest via `buildMemoryEmbeddingText(summary, content, {occurredAt,
   narrativeTime, entities})` (v5 `episodic.rs:114`) with priority
   `'background'`; a failure → WARN `'Digest embedding failed; its cluster is
   skipped'` → that cluster `'embedding-failed'` / `'digest embedding
   failed'`, `clustersFailed++`; RE-PLAN (fresh UUIDs) without it.
   ⚠ `outcomes[digest.clusterIndex].embeddings[k] = …` indexes `outcomes` by
   the FIRST plan's cluster index — correct because the first plan is built
   from `outcomes` itself.
9. Stats from the final plan; `rowsMarkedConsidered = |plan.considered ∪
   belowMinConsidered|`.
10. **Execute** (non-dry) `:561-640`: vector store open failure → WARN
    `'Vector store unavailable during consolidation; digests are written
    without vectors'`; creates → `memories.create(data, {id, createdAt: now,
    updatedAt: now})` THEN `updateForCharacter(…, {embedding})` + vector
    add/update (WARN `'Failed to stage digest vector'`), debug `'Digest
    created'` `{characterId, memoryId, members}`; updates →
    `updateForCharacter({...patch, embedding})`, debug `'Digest revised in
    place'` `{characterId, memoryId, newMembers}`; tier moves →
    `updateTierBulk(characterId, ids, 'cold', {supersededById, consolidatedAt:
    now})` + `store.setTier`; `markConsidered(characterId, uniq(considered ∪
    belowMin), now)`; link rewrites → `updateForCharacter({relatedMemoryIds})`;
    `store.save()` (WARN `'Failed to save vector store after
    consolidation'`). Then if `creates+updates+tierMoves > 0`:
    `invalidateFrozenArchive(characterId)`, `publishRealtime('memories')`,
    `mirrorTouchedBuckets` `:643-700` (per touched non-`none`, existing
    subject bucket: the bucket's LOADED digests (revised content if updated)
    then new creates → bridge; throw → WARN `'Commonplace mirror failed after
    consolidation (digests are still written)'`).
11. INFO `'Consolidation run complete'` `{characterId, characterName, trigger,
    dryRun, candidates, digestsLoaded, clustersFound, clustersSelected,
    clustersSucceeded, clustersFailed, clustersDeferred, digestsCreated,
    digestsUpdated, membersSuperseded, rowsMarkedConsidered, linksRewired,
    mirrorFilesWritten, budgetExhausted, durationMs}`.

Report key order (the dry-run API wire): `characterId, characterName, dryRun,
trigger, startedAt, finishedAt, settings, clusters, stats, [skippedReason]`;
`settings` = stored-parse order with overrides in place; cluster report
`bucket{kind, subjectCharacterId, subjectName}, clusterKind, memberIds,
memberContents, existingDigestId, existingDigestContent, digests,
keepStandalone, contradictions{olderId,newerId,note}, status, [error]`;
digest report `id, action, content, summary, keywords, importance,
reinforcedImportance, reinforcementCount, kind, occurredAt, memberIds`; stats
= `emptyStats()` order (`:245-274`, 25 keys).

### B.1.8 Triggers — `lib/memory/consolidation-triggers.ts` (198)

- `:39` `WATERMARK_CHECK_DEBOUNCE_MS = 10 min`, `:42`
  `WATERMARK_RUN_THROTTLE_MS = 6 h`, `:45` `SCHEDULED_MIN_UNCONSIDERED = 2`;
  module-global `lastWatermarkCheck` Map.
- `characterIdsFromMemoryWrites(writes)` — child write-buffer shape (NO-PORT
  as such; see B.6.4).
- `ranRecently` `:78-87`: `findRecentByType('MEMORY_CONSOLIDATION', 50)` any
  same-character non-dry job in COMPLETED/PROCESSING/PENDING with `now -
  updatedAt < 6h`.
- `maybeEnqueueConsolidationForCharacters(ids, now)` `:93-146`: disabled →
  return; per id: debounce (set BEFORE the count); `countUnconsideredHot <=
  watermark` → debug `'Consolidation watermark not reached'` `{characterId,
  unconsidered, watermark}`; ranRecently → debug `'Consolidation watermark
  reached but a recent run is inside the throttle window'` `{…, throttleMs}`;
  missing/archived → skip; `enqueueMemoryConsolidation(userId, {characterId,
  trigger: 'watermark'})`; INFO `'Consolidation watermark reached; run
  enqueued'` `{characterId, unconsidered, watermark, jobId}`; outer catch →
  WARN `'Consolidation watermark check failed (non-fatal)'` `{characterIds,
  error}`. `maybeEnqueueConsolidationAfterCommit(jobType, writes)` returns for
  `MEMORY_CONSOLIDATION`.
- `runScheduledConsolidation` `:168-198`: disabled → debug `'Scheduled
  consolidation skipped: disabled'`; per `findDistinctCharacterIds()`:
  `charactersScanned++`, missing/archived skip, `countUnconsideredHot < 2`
  skip, enqueue `{characterId, trigger: 'scheduled'}`; per-id catch → WARN
  `'Failed to enqueue scheduled consolidation for character'`; INFO
  `'Scheduled consolidation pass complete'` `{charactersEnqueued,
  charactersScanned}`. Logger child `module: 'memory:consolidation-triggers'`.

### B.1.9 Queue + job + dispatcher

- `queue-service.ts:650-711` `MemoryConsolidationPayload {characterId,
  dryRun?, maxClustersPerRun?, trigger}`; `enqueueMemoryConsolidation(userId,
  payload, options?)`: dedupe over PENDING+PROCESSING `MEMORY_CONSOLIDATION`
  same `characterId` AND same `dryRun === true` flag → debug `'[Consolidation]
  Job already in flight for character; not enqueuing another'`
  `{characterId, existingJobId, trigger, dryRun}`; lookup failure → WARN
  `'[Consolidation] Failed to check for existing jobs during enqueue'`
  `{characterId, error}` (fall through); `enqueueJob(…, {...options,
  maxAttempts: options?.maxAttempts ?? 1})`.
- `handlers/memory-consolidation.ts` (101): `:36`
  `CONSOLIDATION_JOB_TIME_BUDGET_MS = 7 min`; no characterId → WARN
  `'Consolidation job has no characterId; nothing to do'` `{jobId}`;
  `trigger ?? 'manual'`; non-manual + disabled → debug `'Consolidation
  disabled; automatic job exits without running'` `{jobId, characterId,
  trigger}`; `runConsolidation(…, {dryRun: payload.dryRun === true,
  maxClustersPerRun only if defined, userId: job.userId, timeBudgetMs,
  trigger})`; dry run → one INFO `'[Dry run] Consolidation cluster'` per
  cluster `{jobId, characterId, bucket: `${kind}:${subjectName}`,
  clusterKind, status, members, existingDigestId, digests: [{action,
  summary, members}], keepStandalone (count), contradictions (count),
  error}`; INFO `'Consolidation job complete'` `{jobId, characterId,
  trigger, dryRun, skippedReason, digestsCreated, digestsUpdated,
  membersSuperseded, clustersFailed, clustersDeferred, durationMs}`. Logger
  `module: 'jobs:memory-consolidation'`. A lost cluster does NOT fail the job.
- `handlers/index.ts:59` registration; `job.types.ts:37` enum (after
  `MEMORY_HOUSEKEEPING`); `activity-kinds.ts:45` `MEMORY_CONSOLIDATION:
  'memory'`, `:89` chip title (SPA-only in v5:
  `apps/web/src/app/layout/activity-kinds.ts:44`).
- `job-dispatcher.ts:257-259` post-commit
  `checkConsolidationWatermarkAfterCommit` (debug `'Consolidation watermark
  check skipped after job'` on error); `:649`
  `JOBS_INVALIDATING_FROZEN_ARCHIVE` += `MEMORY_CONSOLIDATION` (the hook
  `:671-695` skips `dryRun` payloads; debug `'Invalidated frozen memory
  archives after job'`); `:771-772` vector-store invalidation set +=
  `memories.updateTierBulk` (v5 loads the store per use — no cache, NO-PORT).
- `host-rpc-dispatcher.ts` / `ipc-types.ts` — NO-PORT (no child).
- `realtime/job-topics.ts:91-97`: `MEMORY_CONSOLIDATION` → `dryRun === true`
  → `[]`, else `[{topic:'memories'}, {topic:'mountPoints'}]` (both
  collection-wide).
- `scheduled-housekeeping.ts:41` `HOUSEKEEPING_AFTER_CONSOLIDATION_DELAY_MS =
  30 min`; `:136-155` consolidation FIRST (dynamic import; throw → WARN
  `'Scheduled consolidation pass failed; housekeeping continues'`);
  `enqueued > 0` → `options = {scheduledAt: now + 30 min}` + debug
  `'Housekeeping deferred behind consolidation'` `{consolidationEnqueued,
  delayMs}`; `:171` passed to every `enqueueMemoryHousekeeping`.

### B.1.10 API — `app/api/v1/memories/route.ts`

- GET `consolidation-config` / `extraction-mode-config` → `{success: true,
  settings}`.
- POST `consolidation-config` / `extraction-mode-config`: `safeParse` →
  `validationError` (400); patch = defined entries (see finding 1 — that is
  ALL keys); `set…(patch)`; re-read; INFO `'[Memories API] Consolidation
  settings updated (instance-wide)'` `{enabled, fields}` /
  `'[Memories API] Extraction-mode settings updated (instance-wide)'`
  `{otherPass, fields}`; `{success: true, settings}`.
- POST `consolidate` (`:939-993`): body `req.json().catch(() => ({}))`;
  `consolidateSchema {characterId: z.uuid('Character ID is required'),
  dryRun?, maxClustersPerRun? int [1,1000], clusterThreshold? [0,1]}`;
  `characters.findById` null → `notFound('Character')` (404 `Character not
  found`). Dry run: `maxClusters = min(maxClustersPerRun ?? 10, 40)`, debug
  `'[Memories API] Consolidation dry run (in-process)'` `{characterId,
  maxClusters, clusterThreshold}`, `runConsolidation(…, {dryRun: true,
  maxClustersPerRun: maxClusters, settings: clusterThreshold !== undefined ?
  {clusterThreshold} : undefined, userId: user.id, timeBudgetMs: 120_000,
  trigger: 'manual'})` → `{success: true, dryRun: true, report}`; throw →
  ERROR `'[Memories API] Consolidation dry run failed'` → 500 `Consolidation
  dry run failed`. Real: `enqueueMemoryConsolidation(user.id, {characterId,
  maxClustersPerRun, trigger: 'manual'})` (`clusterThreshold` IGNORED for a
  job) → INFO `'[Memories API] Consolidation job enqueued'` `{characterId,
  jobId}` → `{success: true, dryRun: false, jobId}`; throw → ERROR → 500
  `Failed to enqueue consolidation`. ⚠ A dry run is NOT free — it makes the
  model calls.

## B.2 v5 today — counterparts

Nothing of the engine exists. Reusable pieces:
`memory_gate.rs:45 calculate_reinforced_importance`; `recall_tags.rs`
(`parse_targeting_tags :296`, the tag enums `:52-120` — check a `from_str`
per axis exists for the vocab sets); `episodic.rs:114
build_memory_embedding_text`; `generators/llm_json.rs:234
parse_llm_json_object`; `collation.rs:51 locale_compare`; `jsnum.rs:33
to_fixed`; `jsstr.rs` UTF-16 slicing; `api/zod_issues.rs` (first-issue
message); `cheap_llm.rs:235 selection_from_profile`, `:344
resolve_uncensored_cheap_llm_selection`; the `selectCheapLLMFromProfiles`
twin used by `services/headshoulders_backfill_job.rs:259
build_cheap_llm_selection`; `dangerous_content/chat_override.rs:162
get_concierge_state`; `dangerous_content::resolver::resolve_concierge_settings`;
`db/vector_store.rs` (`load :80`, `has_vector :131`, `add_vector :187`,
`update_vector :240`, `flush :263`; NO tier metadata, NO filtered search);
`db/memories_read.rs:332 find_by_character_id_in_batches` (id ASC, returns
`Vec<Vec<Value>>`; explicit column list at `:60` — keystone adds the four
columns), `:941 find_distinct_character_ids` (returns `Vec`, not `Result`);
`db/memories.rs` `MemCreate :67` / `MemUpdate :211` / `create :259` /
`update_for_character :462`; `services/frozen_archive.rs:91
invalidate_frozen_archive` (no production caller today — the recall lane adds
some); `realtime::publish_realtime`; `services/conversation_summary_vault_bridge.rs`
(the in-process write template: archived guard `:336-344`, private
`resolve_vault_mount_id :250` (raw pointer + mount-exists check + WARN
`Failed to resolve character vault store`), `links.ensure_folder_path` /
`write_database_document` inside ONE mount-index write `:353-381`);
`db/database_store.rs:149 read_database_document`;
`BackgroundJobsRepository::find_recent_by_type` (used by `memory_gate.rs:1061`).
Job plumbing: `job_runner.rs:142 KNOWN_JOB_TYPES`, `api/system_data.rs:635
JOB_TYPES` (the enqueue gate), `services/activity_kinds.rs:86`,
`services/cheap_llm_exec.rs:1733 TASK_TYPE_ACTIVITY`,
`services/llm_logging.rs:420 map_task_type_to_log_type`,
`realtime/job_topics.rs:96-112`, host registration `spine.rs:3848-3920`,
scheduled housekeeping `queue_service.rs:1200 run_scheduled_housekeeping`
(`enqueue_memory_housekeeping :113` has NO `scheduled_at` option;
`enqueue_job :68` / `enqueue_job_with_priority :255` both stamp `now`).
API: `api/memories.rs` (2,000 lines — the housekeeping/recall cluster edits
`memory_list` / `memory_search` / `memory_housekeep*` here), Request variants
`api/types.rs:1705-1710` pattern, dispatch `api/engine.rs:3842-3848` pattern;
the SPA contract `apps/web/src/app/core/core-contract.ts:5958` pattern.

## B.3 What the ledger / commit prose got wrong (engine half)

- Nothing false in the engine description; two precisions: the scheduled
  housekeeping deferral applies only when `charactersEnqueued > 0` (a
  disabled consolidation never defers), and the `MEMORY_CONSOLIDATION`
  job-topics arm returns `[]` for a dry run.
- Finding 1 (the settings POST clobber) is not in the ledger.
- The `memoryConsolidation.coldRetentionDays` field is listed under
  consolidation settings but is read by housekeeping (other cluster).

## B.4 Differential plan (engine half)

No existing family covers the engine. Existing families that move:
`activity_tables_equivalence` (JOB_TYPE row + TASK_TYPE row — red by
design), `task_type_log_mapping_equivalence` (the census requires a
`memory-consolidation` corpus row), `realtime_topics_equivalence` (iterates
`BackgroundJobTypeEnum.options` `realtime-topics.ts:61-65` → a new type, plus a
dry-run row), `brahma_console_tier3_equivalence` (the system-prompt bytes, R1),
`character_optimizer_tier3_equivalence` (R2 — red only with cold rows / a
stamped vector store; rebuild the fixture at target and plant cold rows),
`memories_routes_equivalence` (new actions — add rows),
`instance_settings_json_warns_equivalence` (the `memoryConsolidation` key).

New cases (mirror v4's jests: `consolidation-clustering.test.ts` 243,
`consolidation-plan.test.ts` 334, `consolidation-tasks.test.ts` 194,
`consolidation-service.test.ts` 281, `consolidation-triggers.test.ts` 198,
`handlers/memory-consolidation.test.ts` 122,
`commonplace-digest-vault-bridge.test.ts` 108,
`memories-consolidation-actions.test.ts` 193):
- **tier-1 `consolidation-clustering`** — seeded random unit vectors (dims
  8/16/384), engineered near-threshold pairs (f32 rounding at 0.72), digest
  seeds, episodic windows incl. null times, `maxClusterSize` 2/3/30, ties;
  `selectClusters` over budgets 0/1/40, stale, belowMin, the 2×maturity
  floor. Exact ids + scores (1e-12).
- **tier-1 `consolidation-plan`** — deterministic `newId` (`d1, d2…`),
  creates / updates / contradictions incl. chains and self-targets, link
  rewiring, keyword normalization, `localeCompare` ties on mixed ISO formats,
  `sharedValue` / witnessed arms. Emit the full plan (Maps as ordered arrays).
- **tier-1 `consolidation-tasks`** — `CONSOLIDATION_SYSTEM_PROMPT` bytes,
  user messages (three buckets × two kinds × canon / digest present, 1200-unit
  cut, `toFixed(2)`), `parseConsolidationResponse` over a validation corpus
  (every reason string, Zod first-issue messages, code fences, unparseable).
- **tier-1 `commonplace-file`** render (shared with A).
- **tier-3 `consolidation-service`** — v4's REAL `runConsolidation` over a
  two-DB fixture (a character with ~40 mature AUTO rows in self / other /
  none buckets with designed embeddings, an existing digest, MANUAL rows,
  immature rows, a cold row, a Locked and an Unmoderated chat, a vault),
  canned answers per cluster (valid, invalid, unparseable, timed-out),
  embedding mock (one canned vector, inputs RECORDED — the P4.36 practice),
  `opts.now` pinned; dry-run report diff (normalise `durationMs`,
  `finishedAt`) and real-run tier-2 diff of `memories` / `vector_entries` /
  `llm_logs` / `doc_mount_*` with minted-id remap. Second-run row: mirror
  `unchanged`.
- **tier-2 `consolidation-triggers`** — scheduled pass + watermark over
  `background_jobs` (enabled/disabled, throttle, in-flight dedupe, dry-run
  dedupe split, archived), and the housekeeping 30-min `scheduledAt`.
- **routes** — the three actions' bytes (400 / 404 / 500 arms, the dry-run
  envelope, `{jobId}`), and the settings POSTs per the finding-1 ruling.

## B.5 Proposed units — CONSOLIDATION lane

Sizing: v4 has ~2,850 lines of engine source (clustering 391, plan 476, tasks
379, service 1,067, triggers 198, handler 101, bridge 171, API ~110 + queue /
housekeeping hunks ~110). Estimate ~2,600 Rust lines + ~5 harness families.
ONE new prompt family (a fixed system prompt + one user-message builder).
Pure tier-1-portable: clustering, plan, the task validator / prompt builder,
the file renderer — roughly 45 % of the code.

| # | unit | v5 files (owned) |
|---|---|---|
| C1 | clustering (pure) | NEW `quilltap-core/src/consolidation_clustering.rs` (+ `lib.rs` mod line) |
| C2 | plan (pure) | NEW `quilltap-core/src/consolidation_plan.rs` |
| C3 | tasks: schemas-as-validator, reasons, system prompt, user message, parse, `consolidate_memory_cluster` over `CheapLlmTaskExecutor` (task type, 4000 max tokens, character id, the debug line) | NEW `quilltap-core/src/consolidation_tasks.rs` (+ `consolidation_tasks/prompt_text.rs`) |
| C4 | vault bridge (in-process only): guards, `ensure_folder_path` once, unchanged-compare, per-file write, every line | NEW `services/commonplace_digest_vault_bridge.rs`; touches `services/conversation_summary_vault_bridge.rs` ONLY to make `resolve_vault_mount_id` `pub(crate)` (or duplicate it — lane's choice, recorded) |
| C5 | service: load / cap / cluster / select / route (profile + Concierge) / describe buckets / consolidate / embed + re-plan / execute / mirror / report + every line; in-job vs in-process announce (B.6.3) | NEW `services/consolidation.rs` |
| C6 | queue + handler: `enqueue_memory_consolidation` (dedupe, maxAttempts 1, payload key order `characterId, [dryRun], [maxClustersPerRun], trigger`), `handle_memory_consolidation` (7-min budget, gates, dry-run lines, completion line) + host registration + the job-topics arm | `services/queue_service.rs` (new fn only), NEW `services/memory_consolidation_job.rs`, `realtime/job_topics.rs`, `quilltap-host/src/spine.rs` (ONE `vec!` entry — append-only) |
| C7 | scheduled: `run_scheduled_consolidation` + housekeeping-first deferral (`scheduled_at` on `enqueue_memory_housekeeping`) + lines | NEW `services/consolidation_triggers.rs`, `services/queue_service.rs` (`run_scheduled_housekeeping`, `enqueue_memory_housekeeping` option) |
| C8 | watermark trigger (`maybe_enqueue_consolidation_for_characters`, debounce map, throttle) + the post-job hook shape per the B.6.4 ruling | `services/consolidation_triggers.rs`, `services/job_runner.rs` (the post-completion call) |
| C9 | API: GET/POST `consolidation-config`, GET/POST `extraction-mode-config`, POST `consolidate` | NEW `api/memory_consolidation.rs`, `api/types.rs` (+3 variants, append), `api/engine.rs` (+arms, append), `api/mod.rs` |

C9 owns the extraction-mode setters too (they are API, not extraction); the
extraction lane only READS the mode.

## B.6 Meeting points + open questions

### B.6.1 From the KEYSTONE (data layer — must land first or be frozen as a contract)

- `db/memories.rs`: `MemCreate` gains `tier: String` ("hot" default),
  `superseded_by_id: Option<String>`, `consolidated_from: Vec<String>`,
  `consolidated_at: Option<String>`; `source` accepts `"CONSOLIDATED"` in
  P4.161's `parse_create_memory` validator; `MemUpdate` gains `tier:
  Option<String>`, `superseded_by_id: Option<Option<String>>`,
  `consolidated_from: Option<Vec<String>>`, `consolidated_at:
  Option<Option<String>>`; NEW `update_tier_bulk(&self, character_id, ids:
  &[String], tier: &str, superseded_by_id: Option<Option<&str>>,
  consolidated_at: Option<&str>) -> Result<i64>` (chunked, `updatedAt` = now,
  scoped by character, debug `Moved memories between tiers`
  `{characterId, tier, requested, changed, supersededById}`) and NEW
  `mark_considered(&self, character_id, ids, when) -> Result<i64>` (also
  stamps `updatedAt` — v4's `updateMany` does, `base.repository.ts:526-540`).
- `db/memories_read.rs`: the four columns in the explicit SELECT list (`:60`)
  and hydration (`consolidatedFrom` JSON-parsed like `keywords`); NEW
  `count_unconsidered_hot(conn, character_id) -> Result<i64>` (SQL verbatim:
  `… WHERE characterId = ? AND COALESCE(tier,'hot') = 'hot' AND consolidatedAt
  IS NULL AND source != 'CONSOLIDATED'`; v4 fallback 0 on error).
- `db/chats*.rs`: `otherExtractionWatermarkMessageId` hydrated on the chat
  read; `ChatUpdate.other_extraction_watermark_message_id:
  Option<Option<String>>`.
- `db/instance_settings.rs`: `MemoryConsolidationSettings` +
  `get_/set_memory_consolidation_settings`, `MemoryExtractionModeSettings` +
  `get_/set_memory_extraction_mode_settings` (Zod-faithful parse fns, the
  `[InstanceSettings] <key> failed to parse — using defaults` WARN, `to_json`
  in declaration order).
- `db/vector_store.rs` (keystone OR the recall lane — NAME ONE): per-entry
  tier stamped at `load` from `find_cold_ids_by_character_id` (failure → all
  hot + WARN `Could not read cold memory ids; treating every vector as hot`),
  `set_tier(ids, tier)` (+ its debug line), a filtered search
  `search_hot(query, limit)` (R2 consumes it; consolidation needs only
  add/update/has/flush).
- Job kind: `MEMORY_CONSOLIDATION` in `job_runner.rs KNOWN_JOB_TYPES`,
  `api/system_data.rs JOB_TYPES`, `activity_kinds.rs` row; the task type
  `memory-consolidation` in `cheap_llm_exec.rs TASK_TYPE_ACTIVITY` and
  `llm_logging.rs map_task_type_to_log_type` (→ `MEMORY_EXTRACTION`). (Pure
  table rows — keystone keeps the activity/task-type censuses whole.)
- `queue_service.rs` plumbing the extraction lane consumes:
  `enqueue_memory_extraction` gains the catch-up flags — proposed signature
  `enqueue_memory_extraction_with(db, user_id, &MemoryExtractionEnqueue {
  chat_id, turn_opener_message_id, extraction_anchor_message_id,
  connection_profile_id, fold_other_catchup: bool, fold_other_ignore_idle:
  bool, skip_dedupe: bool })` — payload literal appends
  `"foldOtherCatchup": true` (then `"foldOtherIgnoreIdle": true`) ONLY when
  set, dedupe term `(existing.foldOtherCatchup ?? false) == incoming`; the old
  6-arg fn stays as a wrapper (callers untouched). If the keystone declines
  queue_service.rs, the extraction lane takes this unit and C6/C7 add their
  fns in the same file after it (serialize the two lanes' edits there).
- NEW shared pure leaf `commonplace_file.rs` (path, canon-text, truncate,
  render) — **recommended in the keystone** so E2 and C4 compile in parallel;
  otherwise E-lane owns it and C4 waits.

### B.6.2 Extraction ⇄ consolidation contract (fn level)

- C5 consumes from E2: `pub fn load_canon_for_observer_about_subject(db,
  observer_mount_point_id: Option<&str>, observer_character_id: &str, subject:
  &CanonSubject, include_commonplace: bool) -> CanonSource` and
  `canon::render_self_canon_block` / `render_other_canon_block` /
  `load_canon_for_self` (unchanged signatures; `SelfCanon.commonplace` /
  `CanonSource.commonplace` default `None`). Consolidation passes
  `include_commonplace = false` and never sets SELF commonplace.
- E2 consumes from the shared leaf: `commonplace_path_for(Subject::SelfFile |
  Subject::Other{id,name}) -> String`, `commonplace_file_to_canon_text(Option<&str>)
  -> Option<String>`, `truncate_to_token_budget(&str, i64) -> Option<String>`.
- C4 consumes `render_commonplace_file(&CommonplaceFileInput) ->
  RenderedCommonplace {content, entries_written, entries_dropped}`.
- Nothing else crosses: the extraction lane never touches consolidation
  files; the consolidation lane never touches `memory_processor.rs` /
  `memory_tasks.rs` / `context_summary.rs` / `memory_extraction_job.rs` /
  `scheduled_maintenance.rs`.

### B.6.3 With the recall / housekeeping clusters

- `invalidate_frozen_archive(character_id)` (exists) — C5 calls it on the
  in-process path; the recall lane owns `frozen_archive.rs`. v4's
  job-completion invalidation (`JOBS_INVALIDATING_FROZEN_ARCHIVE`, skip dry
  runs) — whoever ports F5's `invalidateFrozenArchivesForCompletedJob` for
  MEMORY_HOUSEKEEPING adds MEMORY_CONSOLIDATION in the same set.
- v5 has no child, so `run_consolidation` runs in-process on BOTH paths; v4's
  in-job `publishRealtime('memories')` + invalidate are child no-ops covered
  by the dispatcher. Ruling needed: thread an `in_job: bool` so the job path
  publishes only via `topics_for_completed_job` (`memories` + `mountPoints`)
  and invalidates once, or accept the coalesced double `memories` hint
  (record it).
- Housekeeping's `coldRetentionDays` sweep, `count_hot_by_character_id`,
  `find_hot_digests`, `find_cold_ids_by_character_id`,
  `find_expired_cold_ids`, `find_superseded_by` — NOT consumed by this
  cluster (recall / housekeeping / keystone own them).
- `spine.rs`: the housekeeping cluster edits `MemoryHousekeepingHandler`
  (`:2932+`); C6 adds one `job_handlers` entry (append-only) — unifier
  merges.
- `api/memories.rs`: owned by the housekeeping/recall cluster; this cluster
  puts its handlers in NEW `api/memory_consolidation.rs`.
- SPA: the Memory-tab cards, the report dialog, the grain card, the chip
  title, `core-contract.ts` request types — the SPA lane consumes C9's
  Request names (`memoryConsolidationConfigGet/Set`,
  `memoryExtractionModeConfigGet/Set`, `memoryConsolidate` — proposed).
- CLI `quilltap memories consolidate` — the CLI cluster (v5's `memories` verb
  is unported).

### B.6.4 Open questions / measurements before coding

1. **Finding 1 ruling** (port the clobber vs fix) + a v4 bug filing.
2. **Watermark hook shape.** v4 reacts to the child's buffered
   `memories.create` writes after commit. Options for v5: (a) a poll-scoped
   recorder (the `services/activity_registry.rs` `Attributed` thread-local
   technique) set by every `memories.create` CALL SITE inside a job — needs a
   census (gate INSERT arm in `memory_gate.rs`, the fold episode pass, the
   fold OTHER pass via the gate, Carina, inter-character) and touches files
   other lanes own; (b) handlers return created character ids in
   `JobOutcome`; (c) an SQL window (`createdAt >= job.startedAt`) — inexact
   under concurrent jobs. Feature OFF by default; pick one and record the
   divergence.
3. `memory_processor_tier3` / `memory_pipeline_jobs_tier3` /
   `carina_memory_extraction_tier3` corpora: count OTHER candidates < 0.75
   and decide whether to add a `memoryExtractionMode` fixture row per op
   (`turn` / `fold` / `hybrid` coverage) — v4 reads the REAL setting.
4. Which orchestrator / help-chat / enclave / courier corpora fold, and what
   their fold OTHER call yields (≥ 2 subjects?); help chats are NOT excluded
   at the hook.
5. `maintenance_ops_tier2`: the fixture's `lastMessageAt`s vs wall-clock 2 h /
   30 d (any catch-up enqueue is wall-clock dependent — pin `now` or assert
   none).
6. `NoopSeams` users: any family on `NoopSeams` whose v4 oracle does not mock
   `fold-other-pass` will need the live seam.
7. Clustering f32 fidelity: confirm v4 embeddings arrive as `Float32Array`
   (BLOB) and that v5 hands the clusterer the same f32s (the `{"0":…}` JSON
   shape is display only).
8. `recall_tags.rs`: confirm per-axis value sets match
   `TEMPORAL_VALUES` / `SCOPE_VALUES` / `CONTEXT_VALUES` exactly.
9. Concierge: v5 `get_concierge_state(Some(&chat))` returns the same four
   states v4's `getConciergeState` does for the locked/unmoderated test.
10. Whether the job path should keep v4's per-cluster `Date.now()` budget
    check verbatim (the 7-min budget vs v5's stuck-job window — confirm v5's
    stuck sweep is also 10 min).
