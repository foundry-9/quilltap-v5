# SURVEY S2 — `4d370a90f` (#75) + `3b463d6b1` (#76) — fresh survey 2026-09-25 at those shas and on v5 main `2aed9a552`

**Part 1 of 4 — §A for `4d370a90f` ("Concierge overhaul phase 3: three states (Moderated, Unmoderated, Locked) (#75)", 108 files, `4.10.0-dev.86`).** Parts: 2 = §A for #76 + the consumer sweep; 3 = §B/§C (v5 map, families); 4 = §D/§E.

Read-only survey; hunks read via `git show`. Post-commit line numbers below are **at `3b463d6b1`** (the later sha; #76 did not move the #75 modules except where noted). ⚠ Standing precondition from the drift ledger: #75 is stacked on #73 (`8bd080267`, refusal-driven failover) and #74 (`49059fb14`, refusal ledger), BOTH UNPORTED in v5 — every #75 hunk that touches `refusal-ledger.ts`, `image-failover.ts`, `understudy.ts`, `refusal-*` announcement kinds, `readCurrentConciergeState` assumes that substrate exists (see Part 3 §B and Part 4 §D).

## A. v4, per file (#75)

### A.1 `lib/schemas/chat.types.ts` (+49) — the three enums + three columns
- New, lines 112–126 at the sha (verbatim):
```ts
export const ConciergeModeSchema = z.enum(['moderated', 'unmoderated', 'locked']);
export type ConciergeMode = z.infer<typeof ConciergeModeSchema>;
export const ConciergeModeSetBySchema = z.enum(['operator', 'concierge']);
export type ConciergeModeSetBy = z.infer<typeof ConciergeModeSetBySchema>;
export const ConciergeModeReasonSchema = z.enum(['manual', 'refusals', 'classifier', 'migration']);
export type ConciergeModeReason = z.infer<typeof ConciergeModeReasonSchema>;
```
- `ChatMetadataSchema` AND `ChatMetadataBaseSchema` (the one `ChatsRepository` registers: `super('chats', ChatMetadataBaseSchema)` at `chats.repository.ts:67`) both gain, immediately AFTER `dangerClassifiedAtMessageCount` and BEFORE `answerConfirmationOverride`:
```ts
  conciergeMode: ConciergeModeSchema.nullable().optional(),
  conciergeModeSetBy: ConciergeModeSetBySchema.nullable().optional(),
  conciergeModeReason: ConciergeModeReasonSchema.nullable().optional(),
```
  (#75 keeps `conciergeOverride: z.enum(['OFF', 'UNCENSORED']).nullable().optional()` in both, re-commented LEGACY; #76 deletes it — Part 2.) No `.default()` on any of the three ⇒ `generateDDL` emits `"conciergeMode" TEXT` etc. with NO DEFAULT (v4's own DDL.md hunk says so: "NULL reads as 'moderated' (a fresh database's schema-generated column has no default)").
- Zod order at `3b463d6b1` (`ChatMetadataBaseSchema` 1316–1341): `isDangerousChat, dangerScore, dangerCategories, dangerClassifiedAt, dangerClassifiedAtMessageCount, conciergeMode, conciergeModeSetBy, conciergeModeReason, answerConfirmationOverride, sceneState, …`.

### A.2 `lib/schemas/settings.types.ts` (#75: comment only)
- `autoSwitchAfterRefusals` doc comment re-worded ("…on a Moderated chat, the Concierge switches it to Unmoderated. 0 = never."). No shape change in #75.

### A.3 `lib/services/dangerous-content/chat-override.ts` (203 lines, rewritten) — exports with line ranges
| line | export | rule |
|---|---|---|
| 43 | `type ConciergeOverrideValue = 'OFF' \| 'UNCENSORED'` | kept, "No longer written" |
| 50 | `type ConciergeState = ConciergeMode` | wire values of `conciergeState` |
| 53 | `type ConciergeProvenance = ConciergeModeSetBy \| null` | |
| 56 | `CONCIERGE_STATES: readonly ConciergeState[] = ['moderated','unmoderated','locked']` | control order |
| 60–70 | `type ChatLike = { conciergeMode?, conciergeModeSetBy?, conciergeModeReason?, conciergeState?, conciergeSetBy?, conciergeReason? }` | columns OR a server-derived payload; **columns first** |
| 78 | `getConciergeState(chat)` | `const mode = chat?.conciergeMode ?? chat?.conciergeState; return mode === 'unmoderated' \|\| mode === 'locked' ? mode : 'moderated';` — NULL/absent/unknown ⇒ `'moderated'`; **the legacy pair is never read** |
| 88 | `getConciergeProvenance(chat)` | `if (getConciergeState(chat) === 'moderated') return null; const by = chat?.conciergeModeSetBy ?? chat?.conciergeSetBy; return by === 'concierge' \|\| by === 'operator' ? by : 'operator';` — an unknown/NULL setBy on a non-Moderated chat reads as **`'operator'`** |
| 95 | `getConciergeReason(chat)` | `null` for Moderated; else `chat?.conciergeModeReason ?? chat?.conciergeReason ?? null` |
| 106 | `conciergeStateUsesUncensoredRoute(state)` | `state === 'unmoderated'` |
| 115 | `shouldUseUncensoredRoute(chat)` | delegates |
| 124 | `shouldShowDangerStyling(chat)` | `=== 'unmoderated'` (provenance never changes colour) |
| 134 | `isClassifierOnDuty(chat)` | `=== 'moderated'` ("May the Concierge move this chat?") |
| 146 | `conciergeStateMayFailOver(state)` | `state !== 'locked'` |
| 155 | `mayFailOver(chat)` | delegates; chatless ⇒ Moderated ⇒ true |
| 160 | `interface ConciergeModeColumns { conciergeMode; conciergeModeSetBy: ...\|null; conciergeModeReason: ...\|null }` | |
| 176 | `deriveConciergeModeFromLegacy({conciergeOverride?, isDangerousChat?})` | table below, in this ORDER |
| 197 | `withConciergeModeFromLegacy<T>(chat)` | `if (chat.conciergeMode != null) return chat; return { ...chat, ...deriveConciergeModeFromLegacy(chat) };` |

`deriveConciergeModeFromLegacy` rule by rule (first match wins):
1. `conciergeOverride === 'UNCENSORED'` → `{unmoderated, operator, migration}`
2. `conciergeOverride === 'OFF'` → `{locked, operator, migration}`
3. `isDangerousChat === true` → `{unmoderated, concierge, classifier}`
4. else → `{moderated, null, null}`

`withConciergeModeFromLegacy` uses `!= null` (so `conciergeMode: null` in a bundle IS derived; only a present string is kept). Note the legacy `conciergeOverride` type is `ConciergeOverrideValue | string | null` (any string accepted, only the two literals matter).

### A.4 `lib/services/dangerous-content/manual-flip.ts` (224 lines, rewritten) — `applyConciergeFlip` whole flow
Signature (84): `applyConciergeFlip(chatId, requested: ConciergeState, chat: Pick<ChatMetadata,'conciergeMode'|'conciergeModeSetBy'|'conciergeModeReason'>, options: ApplyConciergeFlipOptions = {}) → Promise<{ newState, changed }>`.
`ApplyConciergeFlipOptions` (54): `{ by?: ConciergeModeSetBy; reason?: Exclude<ConciergeModeReason,'migration'>; refusals?: ConciergeAutoFlagDetails; classification?: ConciergeDangerDetails }`. `MODERATION_REFUSALS_CATEGORY` and `currentConciergeState`/`ConciergeUIState` are DELETED.

Flow, in order:
1. `by = options.by ?? 'operator'`; `reason = options.reason ?? 'manual'`; `current = getConciergeState(chat)`; `currentBy = getConciergeProvenance(chat)`; `currentReason = getConciergeReason(chat)`.
2. `nextBy = requested==='moderated' ? null : requested==='locked' ? 'operator' : by`; `nextReason = requested==='moderated' ? null : requested==='locked' ? 'manual' : reason`.
3. **Same state:** if `currentBy === nextBy && (currentReason === nextReason || requested === 'moderated')` → `logger.debug('Concierge flip is a no-op: state and provenance already match', { chatId, state: current, by: currentBy, reason: currentReason })`, return `{newState: requested, changed:false}`. Else if `by === 'concierge'` → `logger.debug('Concierge flip skipped: the Concierge does not re-attribute a state the operator chose', { chatId, state: current, by: currentBy })`, `changed:false`. Else (operator adopting) → `repos.chats.setConciergeMode(chatId, { conciergeMode: current, conciergeModeSetBy: nextBy, conciergeModeReason: nextReason })` (NO expected), `logger.info('Concierge state provenance updated', { chatId, state: current, fromBy: currentBy, toBy: nextBy, fromReason: currentReason, toReason: nextReason })`, **no announcement**, `changed:true`.
4. **Concierge may only move Moderated→Unmoderated:** `if (by==='concierge' && (current!=='moderated' || requested!=='unmoderated'))` → `logger.warn('Concierge flip refused: the Concierge may only move a Moderated chat to Unmoderated', { chatId, from: current, to: requested, reason })`, return `{newState: current, changed:false}`.
5. `if (by==='concierge' && process.env.QUILLTAP_JOB_CHILD==='1')` → `logger.warn('Concierge flip refused in the job child; the parent decides', { chatId, to: requested, reason })`, `{newState: current, changed:false}`.
6. `written = await repos.chats.setConciergeMode(chatId, { conciergeMode: requested, conciergeModeSetBy: nextBy, conciergeModeReason: nextReason }, by==='concierge' ? current : undefined)` — operator unconditional, Concierge compare-and-set. `if (!written)` → `logger.info('Concierge flip abandoned: the chat changed state since it was read', { chatId, expected: current, to: requested, by, reason })`, `{newState: current, changed:false}`.
7. `switch (requested)`:
   - `'moderated'`: `repos.chats.update(chatId, { isDangerousChat:false, dangerScore:null, dangerCategories:[], dangerClassifiedAt:null, dangerClassifiedAtMessageCount:null })` → `repos.chats.resetModerationRefusalLedger(chatId)` → `postConciergeManualAnnouncement({ chatId, kind: 'set-moderated' })`. (Note: `update` is a whole-row `_update`; the three Concierge columns are patch-only so it cannot rewind step 6.)
   - `'unmoderated'`: `by==='operator'` → `postConciergeManualAnnouncement({ chatId, kind:'set-unmoderated' })`; else `reason==='classifier'` → `postConciergeDangerAnnouncement({ chatId, details: options.classification })`; else → `postConciergeManualAnnouncement({ chatId, kind:'auto-unmoderated', details: options.refusals })`.
   - `'locked'`: `postConciergeManualAnnouncement({ chatId, kind:'set-locked' })`.
8. `logger.info(by==='concierge' ? 'Concierge state switched by the Concierge' : 'Concierge state switched by the operator', { chatId, from: current, to: requested, by, reason: nextReason })`; return `{newState: requested, changed:true}`.

Every logger here is `createServiceLogger('ConciergeManualFlip')`.

### A.5 `lib/services/dangerous-content/classifier-switch.ts` (NEW, 67 lines)
`maybeSwitchAfterClassification(chatId, verdict: ConciergeDangerDetails|null|undefined) → Promise<{switched:boolean}>` (33). Logger `ConciergeClassifierSwitch`. Never throws.
1. `QUILLTAP_JOB_CHILD==='1'` → `warn('Classifier switch refused in the job child; the parent decides', { chatId })` → false.
2. `chat = getRepositories().chats.findById(chatId)`; none → `debug('Classifier switch skipped: chat not found', { chatId })` → false.
3. `!isClassifierOnDuty(chat)` → `info('Classifier switch skipped: the chat is no longer Moderated', { chatId, state: getConciergeState(chat) })` → false.
4. `result = applyConciergeFlip(chatId, 'unmoderated', chat, { by:'concierge', reason:'classifier', classification: verdict ?? undefined })`; `info('Classifier verdict applied', { chatId, switched: result.changed })` → `{switched: result.changed}`.
5. catch → `error('Classifier switch failed', { chatId, error: getErrorMessage(error) })` → false.

### A.6 `lib/services/dangerous-content/current-state.ts` (NEW in #75, 52 lines; #76 adds `readCurrentConciergeOnDuty` → 82 lines)
`readCurrentConciergeState(chatId: string|null|undefined, snapshot?: ConciergeState|null) → Promise<ConciergeState>` (28). Logger `ConciergeCurrentState`.
- `fallback = snapshot ?? 'moderated'`; `!chatId` → fallback.
- `chat = getRepositories().chats.findById(chatId)`; none → `debug('Current Concierge state: chat not found; using the snapshot', { chatId, snapshot: fallback })` → fallback.
- `state = getConciergeState(chat)`; `if (state !== fallback) info('Concierge state changed since the request began', { chatId, snapshot: fallback, current: state })`; return state.
- catch → `warn('Could not re-read the Concierge state; using the snapshot', { chatId, snapshot: fallback, error })` → fallback.

### A.7 `lib/services/dangerous-content/concierge-state-presentation.ts` (#75 rewrite; #76 drops `info`)
- `ConciergeTone` at #75: `'danger' | 'muted' | 'info' | 'success'` (#76: `'danger' | 'muted' | 'success'`).
- `CHANGE_HINT = "Change it from the Salon sidebar's Chat section."` (42).
- THE table (50), verbatim:
```ts
  moderated: { label: 'Moderated', icon: 'eye', tone: 'success',
    detail: 'The Concierge sends everything to the usual providers first, and to the uncensored desk only when one of them refuses. After enough refusals he moves the whole chat himself.', hint: CHANGE_HINT },
  unmoderated: { label: 'Unmoderated', icon: 'eye-off', tone: 'danger',
    detail: 'You have opened the uncensored door yourself. Nothing here goes near a moderated provider.', hint: CHANGE_HINT },
  locked: { label: 'Locked', icon: 'shield', tone: 'muted',
    detail: 'Only the usual providers, ever. If one refuses, the refusal stands. For the chat that must never reach an uncensored model.', hint: CHANGE_HINT },
```
- `NUMBER_WORDS = ['no','one','two','three','four','five','six','seven','eight','nine','ten']` (74).
- `conciergeMovedDetail(reason, refusalCount?)` (81): `reason==='refusals'` → `n = refusalCount ?? 0; counted = n > 0 ? \` after ${n < NUMBER_WORDS.length ? NUMBER_WORDS[n] : n} ${n === 1 ? 'refusal' : 'refusals'}\` : ' after the usual providers refused it'`; returns `` `The Concierge moved this chat to the uncensored desk${counted}. Set it back to Moderated if you disagree.` ``; any other reason → `'The Concierge moved this chat to the uncensored desk on reading the conversation. Set it back to Moderated if you disagree.'`.
- `conciergeToneSuffix(tone)` (98): `'muted'→'-muted'`, (#75 also `'info'→'-info'`), else `''`. `conciergeToneTextClass` (108): muted→`qt-text-muted`, success→`qt-text-success`, default→`qt-text-danger` (#75 also info→`qt-text-info`).
- `interface ConciergeProvenanceNote { setBy?; reason?; refusalCount? }` (129).
- `describeConciergeState(state, provenance: ConciergeProvenanceNote = {}, dangerCategories?)` (144): `byConcierge = state==='unmoderated' && provenance.setBy==='concierge'`; `detail = byConcierge ? conciergeMovedDetail(provenance.reason, provenance.refusalCount) : presentation.detail`; `categories = byConcierge && provenance.reason !== 'refusals' && dangerCategories?.length ? dangerCategories : null`; returns `{ title: label, detail, categories, hint }`. ⚠ signature change: the second positional arg is now the provenance note; `dangerCategories` moved to third.

### A.8 `lib/services/dangerous-content/resolver.service.ts` (#75 state; #76 rewrites it wholesale — Part 2)
- `ResolvedDangerousContentSettings.source: 'global' | 'default' | 'chat-locked' | 'chat-unmoderated' | 'chat-type-exempt'` (was `chat-vouched`/`chat-uncensored`).
- `VOUCHED_SAFE_DANGEROUS_CONTENT_SETTINGS` RENAMED `LOCKED_DANGEROUS_CONTENT_SETTINGS` (same values: `mode:'OFF', threshold:1.0, scanTextChat:false, scanImagePrompts:false, scanImageGeneration:false, displayMode:'SHOW', showWarningBadges:false, autoSwitchAfterRefusals:0`).
- `resolveDangerousContentSettings(globalSettings, chat?: { conciergeMode?: ConciergeState|null; chatType? })`: exempt chatType → LOCKED/`chat-type-exempt`; `state = chat ? getConciergeState(chat) : 'moderated'`; `locked` → LOCKED/`chat-locked`; `unmoderated` → `{...global, mode:'AUTO_ROUTE', threshold:1.0, scanTextChat:false, scanImagePrompts:false, scanImageGeneration:false, showWarningBadges:false}`/`chat-unmoderated`; else global/`global` or DEFAULT/`default`. Order: exempt → locked → unmoderated (the #75 test "moderation-exempt chat types win over the Unmoderated state").

### A.9 `lib/services/dangerous-content/image-failover.ts` (#73 substrate; #75 hunk)
- `ImageFailoverContext` gains `chat?: { conciergeMode?: ConciergeState|null } | null` ("Absent (the dialog) reads as Moderated").
- `announce(ctx, kind, refusing, answeringProfileName?, reason?: 'locked'|'mode')` spreads `...(reason ? { reason } : {})` into details.
- Step 4 gains, BEFORE the mode check: `const conciergeState = await readCurrentConciergeState(ctx.chatId, getConciergeState(ctx.chat)); if (!conciergeStateMayFailOver(conciergeState)) { logger.info('Refusal not rerouted: the chat is Locked', { ...logContext, conciergeState }); await announce(ctx,'refusal-not-permitted', primary.profile, undefined, 'locked'); await ledger(ctx, primary.profile, verdict, false); throw attachTrail(primaryError, trail) }`.
- Callers pass `chat`: `story-background.ts:690` (`chat`), `character-avatar.ts:380` (`chat`), `image-generation-handler.ts` (`chatForOverride` threaded as a new last param `chat?: ConciergeChat|null` of `generateImagesWithProvider`), `images/route.ts:348` (`chat: chatForConcierge`).

### A.10 `lib/services/chat-message/provider-failover.service.ts` (#75 hunk)
- `AttemptEmptyResponseRecoveryOptions` + `AttemptHardErrorFailoverOptions` gain `conciergeState?: ConciergeState` (snapshot; re-read at refusal time). `orchestrator.service.ts:1629` passes `conciergeState: getConciergeState(chat)`; `primary-stream.service.ts:400` likewise into `attemptHardErrorFailover`.
- Empty-response path (279–298 at sha): `const lockedOut = state.fullResponse.trim().length === 0 && !conciergeStateMayFailOver(await readCurrentConciergeState(chatId, conciergeState))`; `if (empty && lockedOut && turnRefusal)` → `logger.info('[EmptyResponse] Refusal not rerouted: the chat is Locked', { chatId, provider: turnRefusal.profile.provider, model: turnRefusal.profile.modelName })` + `postConciergeRefusalAnnouncement({ chatId, kind:'refusal-not-permitted', details:{ refusingProvider, refusingModel, purpose:'text', reason:'locked' } })`. The uncensored retry then requires `!lockedOut` (#75: `&& dangerSettings.mode === 'AUTO_ROUTE'`).
- Hard-error path (1012–1026): before the mode gate, `if (!conciergeStateMayFailOver(await readCurrentConciergeState(chatId, opts.conciergeState)))` → `logger.info('[Failover] Refusal not rerouted to an uncensored profile: the chat is Locked', { chatId })` + the same `refusal-not-permitted`/`locked` announcement + `recordTextRefusal(chatId, refusingProfile, refusal.evidence, false)` + `return walkFallbackChain(opts, openingAttempt)`.

### A.11 `lib/services/concierge-notifications/writer.ts` (#75 hunk; exports at sha: `ConciergeManualKind` 183, `buildAutoFlagContent` 210, `buildAutoFlagOpaqueContent` 221, `buildManualContent` 230, `buildManualOpaqueContent` 243, `postConciergeManualAnnouncement` 263, `postConciergeDangerAnnouncement` 314, `ConciergeRefusalKind` 379, `ConciergeRefusalDetails` 387, `buildRefusalContent` 420, `buildRefusalOpaqueContent` 434, `postConciergeRefusalAnnouncement` 447)
- `ConciergeManualKind` = `'set-moderated' | 'set-unmoderated' | 'set-locked' | 'auto-unmoderated'` (retired: `manual-flagged`, `manual-safe`, `manual-resumed`, `manual-vouched`, `manual-uncensored`, `auto-flagged-refusals` — "Transcripts written before phase 3 carry bubbles of the retired kinds … they are plain messages and render as they always did").
- `buildAutoFlagContent` last sentence: `${declined} The Concierge has taken the liberty of moving the whole affair to the uncensored desk; you may set it back to Moderated from the sidebar whenever you wish.` (was "move it back").
- `buildAutoFlagOpaqueContent`: `${counted} ${noun}${last}. The Concierge switched this chat to Unmoderated; change it in the sidebar.` (was Flagged).
- `buildManualContent` verbatim:
  - `set-moderated`: "By the operator's own hand, the conversation is Moderated once more. The house's usual providers are asked first; should one of them decline a matter on grounds of propriety, the Concierge will quietly take it across the street. His ledger of refusals is wiped clean."
  - `set-unmoderated`: "By the operator's own hand, the Concierge has been sent away and the uncensored door stands open. Nothing is to be examined, nothing softened; the conversation and its errands go henceforth to the frank desk, entirely on the operator's own recognizance." (identical bytes to the retired `manual-uncensored`)
  - `set-locked`: "The operator has locked the present company to the house's usual desks. Should one of them decline a matter, the refusal stands: the Concierge will take nothing elsewhere, nor move the conversation of his own accord."
  - `auto-unmoderated`: `buildAutoFlagContent(details)`.
- `buildManualOpaqueContent` verbatim:
  - `set-moderated`: 'Operator advisory: this conversation is Moderated. Ordinary providers are asked first; a content refusal is retried on an uncensored provider.'
  - `set-unmoderated`: 'Operator advisory: this conversation has been manually set to Unmoderated. It goes to the uncensored providers only; no classification or scanning will run, and prompts go out unaltered.'
  - `set-locked`: 'Operator advisory: this conversation is Locked to the ordinary providers. A content refusal stands; nothing is rerouted and the Concierge will not switch the chat.'
  - `auto-unmoderated`: `buildAutoFlagOpaqueContent(details)`.
- `ConciergeRefusalDetails.reason?: 'locked' | 'mode'` (#75; #76 narrows to `'locked'`). `buildRefusalContent('refusal-not-permitted')` with `reason==='locked'`: `` `The Concierge observes that ${house} (${painter}) declined ${voiced} on grounds of propriety. This conversation is Locked to the usual desks, so the refusal stands; set it to Moderated should you wish him to take such things elsewhere.` ``; opaque: `` `Provider ${who} refused ${plain} on content grounds. This chat is Locked, so it was not rerouted; set it to Moderated to allow an uncensored retry.` ``. `postConciergeRefusalAnnouncement`'s debug bag gains `reason: details.reason`.

### A.12 `lib/services/dangerous-content/refusal-ledger.ts` (#74 substrate; #75 hunk)
- `runAutoSwitchCheck`: `state !== 'monitored'` → `!isClassifierOnDuty(chat)` with `debug('Auto-switch check skipped: the chat is not Moderated', { chatId, state })`; the fresh re-read: `!fresh || !isClassifierOnDuty(fresh)` → `info('Auto-switch abandoned: the chat left Moderated during the check', { ...decision, state: freshState })`; the flip is `applyConciergeFlip(chatId, 'unmoderated', fresh, { by:'concierge', reason:'refusals', refusals:{…} })`; `info('The Concierge switched a chat to Unmoderated after repeated refusals', { ...decision, changed, lastProvider })`.

### A.13 `lib/database/repositories/base.repository.ts` (+24) — `patchOnlyFields()`
- 343: `protected patchOnlyFields(): readonly string[] { return []; }`.
- `_update` (384): after `validated = this.validate(updated)`: `let toSet = validated; const skipped = this.patchOnlyFields().filter((field) => !(field in data)); if (skipped.length > 0) { toSet = { ...toSet }; for (const field of skipped) delete toSet[field]; }` then `collection.updateOne({ id }, { $set: toSet })`; returns `validated` (the FULL merged row incl. the skipped fields as read from the snapshot). Semantics: a whole-row update omits patch-only fields from `$set` unless the caller's `data` names them (by key presence, `in`, not by value — `{ conciergeMode: undefined }` counts as named).

### A.14 `lib/database/repositories/chats.repository.ts` (+105)
- 50: `const CONCIERGE_MODE_FIELDS = ['conciergeMode', 'conciergeModeSetBy', 'conciergeModeReason']`; 560: `protected override patchOnlyFields() { return CONCIERGE_MODE_FIELDS; }`.
- 574 `setConciergeMode(chatId, columns: { conciergeMode: 'moderated'|'unmoderated'|'locked'; conciergeModeSetBy: 'operator'|'concierge'|null; conciergeModeReason: 'manual'|'refusals'|'classifier'|'migration'|null }, expected?: 'moderated'|'unmoderated'|'locked') → Promise<boolean>`:
  - `filter = { id: chatId }`; `if (expected) filter.$or = expected === 'moderated' ? [{ conciergeMode: 'moderated' }, { conciergeMode: null }] : [{ conciergeMode: expected }]` (**NULL counts as moderated**).
  - `written = safeQuery(async () => (await collection.updateOne(filter, { $set: columns })).matchedCount > 0, 'Failed to write the Concierge state', { chatId }, false)`.
  - `logger.debug('Concierge state write', { chatId, ...columns, expected, written })` (bag order: chatId, conciergeMode, conciergeModeSetBy, conciergeModeReason, expected, written). ⚠ `updateOne` with a raw `$set` — no `updatedAt` mint, no Zod validation. In the job child it is a buffered `set*` write and "returns nothing meaningful".
- 618 `setDangerClassification(chatId, telemetry: { isDangerousChat: boolean; dangerScore: number|null; dangerCategories: string[]; dangerClassifiedAt: string; dangerClassifiedAtMessageCount: number }, verdict?: { score; threshold; categories: Array<{category; score; label?}>; source?: 'moderation'|'llm'; providerName? } | null) → Promise<ChatMetadata|null>`: `updated = await this.update(chatId, telemetry)` (whole-row; the Concierge columns patch-only), `logger.debug('Chat danger classification recorded', { chatId, isDangerousChat, dangerScore, hasVerdict: !!verdict })`. `verdict` is NOT stored — it rides the buffered write's third arg to the parent hook.
- `resetModerationRefusalLedger` (722) doc: "when the operator returns a chat to Moderated".

### A.15 `lib/background-jobs/host/job-dispatcher.ts` (+54)
- 390: `await runClassifierSwitchChecks(writes, jobId)` after `runRefusalLedgerChecks`, still inside the apply chain.
- 447: `export const DANGER_CLASSIFICATION_WRITE = 'chats.setDangerClassification'`.
- 454 `chatsWithDangerVerdicts(writes) → Map<chatId, ConciergeDangerDetails|null>`: per write with that method, `[chatId, telemetry, verdict] = w.args`; skip unless `typeof chatId==='string' && chatId`; `dangerous = telemetry object && telemetry.isDangerousChat === true`; skip unless dangerous; `map.set(chatId, verdict object ? verdict : null)` (last wins).
- 476 `runClassifierSwitchChecks`: empty map → return; `log.debug('Child batch recorded a dangerous classification; running the classifier switch', { jobId, chatIds })`; dynamic-import `maybeSwitchAfterClassification`, call per chat sequentially; catch → `log.error('Classifier switch failed after a committed batch', { jobId, error })`. Logger `logger.child({ module: 'jobs:dispatcher' })`.

### A.16 `lib/background-jobs/handlers/chat-danger-classification.ts` (#75 hunk)
- Import swap: `postConciergeDangerAnnouncement` → `maybeSwitchAfterClassification`.
- The final write becomes `verdict = result.isDangerous ? { score, threshold: dangerSettings.threshold, categories: result.categories, source, providerName } : null; await repos.chats.setDangerClassification(payload.chatId, { isDangerousChat, dangerScore, dangerCategories: categories.map(c=>c.category), dangerClassifiedAt: now, dangerClassifiedAtMessageCount: finalMessageCount }, verdict)`.
- `if (verdict && process.env.QUILLTAP_JOB_CHILD !== '1') { const { switched } = await maybeSwitchAfterClassification(payload.chatId, verdict); logger.debug('[ChatDangerClassification] Dangerous verdict applied in the parent', { jobId: job.id, chatId, switched }) }` — the announcement now comes from the flip, not the handler. `isClassifierOnDuty` early-bail comment: only Moderated.

### A.17 `app/api/v1/chats/route.ts` (#75 hunks)
- `createChatSchema.conciergeState: ConciergeModeSchema.optional()` (149).
- `applyRequestedConciergeState(chat, requested, progress, repos) → Promise<ConciergeColumns>` (381): `asCreated = { conciergeMode: chat.conciergeMode ?? null, conciergeModeSetBy: …, conciergeModeReason: … }`; `!requested || requested === 'moderated'` → asCreated; `progress.status('Briefing the Concierge…')`; `result = applyConciergeFlip(chat.id, requested, chat)`; `logger.debug('[Chats v1] Applied Concierge state at creation', { chatId, requested, changed })`; `!result.changed` → asCreated; else re-read `repos.chats.findById` and return its three columns (or asCreated if gone). `type ConciergeColumns = Pick<ChatMetadata, 'conciergeMode'|'conciergeModeSetBy'|'conciergeModeReason'>` (449).
- `handleCreate`: `return created({ chat: { ...chat, ...conciergeColumns, participants: enrichedParticipants } })` — the create response carries the POST-flip columns (the raw column names `conciergeMode`/`conciergeModeSetBy`/`conciergeModeReason`, NOT the derived `conciergeState`).
- Greeting: attempt 0 keyed on `shouldUseUncensoredRoute(chatRow)` (#76 → `conciergePolicy.routeDirect`); attempt 3: `if (contentFilterHit && !uncensoredDeskTried && mayFailOver(chatRow))` (994) — **the Locked skip**.

### A.18 `app/api/v1/chats/[id]/handlers/get.ts` (+9)
- 331: `const conciergeLedger = await repos.chats.getModerationRefusalLedger(chatId);` (a #74 repo method).
- Payload delta, in order (398–403): `isDangerousChat`, `dangerCategories` unchanged; **`conciergeOverride` REMOVED**; added `conciergeState: getConciergeState(chatMetadata)`, `conciergeSetBy: getConciergeProvenance(chatMetadata)`, `conciergeReason: getConciergeReason(chatMetadata)`, `conciergeRefusalCount: conciergeLedger.count`; then `documentEditingMode` …

### A.19 `app/api/v1/chats/[id]/helpers.ts` / `schemas.ts`
- `chatUpdateRequestSchema.conciergeState: ConciergeModeSchema.optional()` (schemas 129) — the four retired values → Zod `.parse` throws → middleware `validationError` 400. Comment: "The retired four-state values … are rejected with 400."
- `processChatUpdates` (helpers 602): `if (validatedData.conciergeState) { flipResult = await applyConciergeFlip(chatId, validatedData.conciergeState, updatedChat); if (flipResult.changed) { refetch } }` — unchanged mechanics.

### A.20 `app/api/v1/chats/[id]/actions/danger-classification.ts` (comment only)
- `handleReclassifyDanger` still `repos.chats.update(chatId, { isDangerousChat:null, dangerScore:null, … })` — "This never moves the chat's Concierge state (only `applyConciergeFlip` does)". Because the columns are patch-only, this whole-row update leaves `conciergeMode` alone.

### A.21 List payloads (+2 keys each, after `conciergeState`, before `dangerCategories`)
- `characters/[id]/handlers/get.ts:221-222`, `projects/[id]/actions/chats.ts:104-105`, `lib/services/chat-enrichment.service.ts` (`EnrichedChatSummary.conciergeSetBy: ConciergeProvenance`, `conciergeReason: ConciergeModeReason|null`; `enrichChatForList` 619–620), `lib/services/home-data.service.ts:74-75`, `lib/chat-utils.ts` (`SalonChatShape`/`CharacterChatShape` + both `transform*ToCardData`): `conciergeSetBy: getConciergeProvenance(chat)`, `conciergeReason: getConciergeReason(chat)`.

### A.22 `lib/services/chat-message/orchestrator.service.ts` / `primary-stream.service.ts`
- Both import `getConciergeState` and pass `conciergeState: getConciergeState(chat)` into the failover options (orchestrator 1629 for `attemptEmptyResponseRecovery`; primary-stream 400 for `attemptHardErrorFailover`).

### A.23 `lib/tools/handlers/image-generation-handler.ts` (#75)
- `type ConciergeChat = { conciergeMode?: ConciergeState|null }` (#76 adds `chatType?`); `chatForOverride: ConciergeChat|null`; warn text `'[Image Generation] Could not load chat for its Concierge state'` (was "…for Concierge override check"); `chat` threaded into `generateImageWithConciergeFailover` ctx.

### A.24 `lib/background-jobs/handlers/story-background.ts` / `character-avatar.ts` (#75)
- Only `chat` added to the failover ctx (+ comment edits). `uncensoredImageTarget` comment: "an uncensored-route chat under Detect Only".

### A.25 `lib/backup/restore/restore.ts`, `lib/import/quilltap-import/import-entities.ts` (#75)
- restore: `repos.chats.create(withConciergeModeFromLegacy(stripScenarioSeededSummary(chatData)), { id: chat.id })`.
- import: BOTH `repos.chats.create` sites (duplicate-title path and the preserveIds path) wrap `withConciergeModeFromLegacy(stripScenarioSeededSummary(chatData))`.

### A.26 `lib/startup/prettify.ts`
- `'add-chat-concierge-mode-v1': "Re-lettering the Concierge's three positions on every chat"`.

### A.27 `migrations/scripts/add-chat-concierge-mode.ts` (NEW, 182 lines) — DDL + backfill VERBATIM
- `id: 'add-chat-concierge-mode-v1'`, `introducedInVersion: '4.10.0'`, `dependsOn: ['add-chat-refusal-ledger-v1']`.
- `legacyRowFilter()` = `[ colExists('conciergeOverride') ? \`"conciergeOverride" IN ('OFF', 'UNCENSORED')\` : null, colExists('isDangerousChat') ? \`"isDangerousChat" = 1\` : null ].filter(Boolean).join(' OR ')`.
- `shouldRun`: SQLite && `chats` exists; true if any of the three columns missing; else `filter` empty → false; else `SELECT COUNT(*) AS n FROM "chats" WHERE "conciergeModeSetBy" IS NULL AND (${filter})` → `n > 0`.
- `run`: `addColumnIfMissing('chats', 'conciergeMode', "TEXT DEFAULT 'moderated'")`, `addColumnIfMissing('chats', 'conciergeModeSetBy', 'TEXT DEFAULT NULL')`, `addColumnIfMissing('chats', 'conciergeModeReason', 'TEXT DEFAULT NULL')`; rows = `SELECT "id", ${hasOverride ? '"conciergeOverride"' : 'NULL AS "conciergeOverride"'}, ${hasLabel ? '"isDangerousChat"' : 'NULL AS "isDangerousChat"'} FROM "chats" WHERE "conciergeModeSetBy" IS NULL AND (${legacyFilter})` (or `[]` when no legacy column); per row `derived = deriveConciergeModeFromLegacy({ conciergeOverride: row.conciergeOverride, isDangerousChat: row.isDangerousChat === 1 })`; `if (derived.conciergeMode !== 'moderated') UPDATE "chats" SET "conciergeMode" = ?, "conciergeModeSetBy" = ?, "conciergeModeReason" = ? WHERE "id" = ?` → `rowsBackfilled++`; `reportProgress(i+1, rows.length, 'chats')`.
- Logs: `debug('Backfilling Concierge mode from the legacy pair', { context:'migration.add-chat-concierge-mode', candidates })`; `info('Added the Concierge mode columns to chats and backfilled them', { context, columnsAdded, rowsBackfilled, durationMs })`; `error('Failed to add the Concierge mode columns', { context, error })`. Result message: `` `Added ${columnsAdded} Concierge mode column(s); backfilled ${rowsBackfilled} chat(s)` ``.
- ⚠ Ordering subtlety: the WHERE filter selects rows where `conciergeOverride IN ('OFF','UNCENSORED') OR isDangerousChat = 1`, but derivation gives `conciergeOverride` precedence; a row with `OFF` AND `isDangerousChat=1` becomes **locked** (the label is ignored). Rows with `conciergeModeSetBy` NOT NULL are never touched (re-run safe; an app-placed row wins).
- `migrations/scripts/index.ts`: import at 317; array entry at 830 (after `addChatRefusalLedgerMigration` 828; array ends 835); export at 1232.

### A.28 `public/schemas/qtap-export.schema.json` (+15)
- `conciergeOverride`: `"enum": ["OFF","UNCENSORED",null], "deprecated": true, "description": "DEPRECATED (4.10): no longer written; superseded by conciergeMode. Kept so a bundle from before the three Concierge states can be imported: when a chat carries no conciergeMode, the importer derives it from this and isDangerousChat — 'UNCENSORED' → unmoderated (operator), 'OFF' → locked (operator), NULL with isDangerousChat true → unmoderated (concierge, classifier), otherwise moderated."`
- Added after it, in order: `conciergeMode` `{"enum":["moderated","unmoderated","locked",null],"description":"The chat's Concierge state (4.10). NULL reads as 'moderated'. 'moderated' = ordinary providers first, uncensored on refusal, the Concierge may switch the chat; 'unmoderated' = the uncensored desk only; 'locked' = ordinary providers only, a refusal stands."}`, `conciergeModeSetBy` `{"enum":["operator","concierge",null],"description":"Who put the chat in its conciergeMode. NULL when Moderated."}`, `conciergeModeReason` `{"enum":["manual","refusals","classifier","migration",null],"description":"Why the chat is in its conciergeMode. NULL when Moderated."}`; then `answerConfirmationOverride`. (#76 does NOT touch this file — `conciergeOverride` stays in the export schema as deprecated.)

### A.29 `help/**` (#75: 8 files, +61/−57)
| file | gist |
|---|---|
| `autonomous-rooms.md` | one-line rename of the state words |
| `chats.md` | New Chat form: three postures (Moderated default / Unmoderated / Locked); "a chat opened Unmoderated goes to the frank desk first, Locked is never rerouted"; only Unmoderated wears the mark and vanishes under quick-hide |
| `dangerous-content.md` | 80-line rewrite of the states section to three states (DELETED whole in #76) |
| `homepage.md` | mark comes in TWO shades: red = Unmoderated (whoever set it), grey = Locked; Moderated wears none |
| `image-generation-profiles.md`, `scene-state-tracker.md`, `story-backgrounds.md` | state-word swaps |
| `quick-hide.md` | Unmoderated goes behind the curtain; Locked and Moderated stay put |

### A.30 Client files reading NEW/RENAMED wire fields (#75) — for the contract pin
| file | fields read |
|---|---|
| `app/salon/[id]/types.ts` | `conciergeState?: ConciergeState; conciergeSetBy?: ConciergeProvenance; conciergeReason?: ConciergeModeReason\|null; conciergeRefusalCount?: number` (chat GET) |
| `app/salon/[id]/SalonView.tsx` | `chat.conciergeState/conciergeSetBy/conciergeReason/conciergeRefusalCount/dangerCategories`; `getConciergeState(chat)`; `if (conciergeState === 'moderated') return null` |
| `components/chat/ChatSidebar.tsx` | `conciergeState` prop; `describeConciergeState(conciergeState, conciergeProvenance).detail`; `shouldShowDangerStyling({ conciergeState })` |
| `components/chat/ConciergeMark.tsx` | props `conciergeState, conciergeSetBy, conciergeReason, dangerCategories`; `describeConciergeState(state, { setBy, reason }, categories)` |
| `components/chat/ChatCard.tsx`, `components/homepage/{RecentChatItem,types}.tsx`, `app/prospero/[id]/{ChatsSection,types}.tsx` | list rows: `conciergeSetBy`, `conciergeReason` |
| `components/new-chat/hooks/useNewChat.ts` | POST body `conciergeState: 'moderated'` default; `if (state.conciergeState !== 'moderated')` include |
| `components/providers/quick-hide-provider.tsx`, `components/settings/chat-settings/DangerousContentSettings.tsx` | state-word swaps only |

### A.31 Test files (#75) and their case names (server units; the Rust port mirrors them)
- `__tests__/unit/app/api/v1/chats/[id]/handlers/get.test.ts`: `projects the Concierge state, provenance and refusal tally, and no conciergeOverride`; `reads a chat with no Concierge columns as Moderated with no provenance`.
- `…/chats/route.concierge-state.test.ts`: `does not touch the Concierge when Moderated is requested`; `it.each(['unmoderated','locked'])` (applies at creation); `reports the applied state in the create response, not the row as first inserted`; `rejects a state outside the three`; `it.each(['monitored','flagged','vouched','uncensored'])` (400); `sends an Unmoderated chat to the frank desk first, even under a global OFF`; `never tries the uncensored desk for a Locked chat, content filter or no`; `does try the uncensored desk for a Moderated chat whose greeting hits a content filter`.
- `…/background-jobs/chat-danger-classification.test.ts`: `carries the dangerous verdict on the telemetry write and, in the parent, asks the classifier switch`; `leaves the switch to the parent when running in the job child`; `never asks the switch for a safe verdict`.
- `…/lib/background-jobs/child-proxy-refusal-ledger.test.ts`: `buffers the classifier's verdict as telemetry and leaves the switch to the parent hook`.
- `…/lib/database/repositories/base-repository-patch-only.test.ts` (NEW): `AbstractBaseRepository._update — patch-only fields` { `without patch-only fields, a stale snapshot rewinds a newer value (the hazard)`; `leaves a patch-only field out of the $set when the patch does not name it`; `writes a patch-only field when the patch names it` }; `ChatsRepository — the Concierge state columns are patch-only` { `declares conciergeMode, conciergeModeSetBy and conciergeModeReason` }; `ChatsRepository.setConciergeMode — compare-and-set` { `writes unconditionally without an expected state`; `treats NULL as Moderated when Moderated is expected`; `reports a miss when the stored state no longer matches` }.
- `…/lib/services/chat-message/provider-failover-refusal.test.ts`: `reads the state at refusal time: a chat locked while the provider was thinking is not rerouted`; `never asks the uncensored desk on a Locked chat, even under Auto-Route, and says why`.
- `…/provider-failover.service.test.ts` `a Locked chat`: `never asks the uncensored understudy, and says the refusal stands`; `reads the state at refusal time: a chat locked mid-turn is not rerouted`; `posts nothing for a plain empty body on a Locked chat`.
- `…/dangerous-content/chat-override.test.ts`: `CONCIERGE_STATES` { `lists the three states in control order` }; `getConciergeState` { `returns 'moderated' for a null/undefined chat or a NULL column`; `ignores the legacy pair entirely`; `reads a server-derived payload (conciergeState) when the column is absent`; `prefers the column over a derived payload value` }; `describe.each(TABLE)('mode=$mode setBy=$setBy')` { `derives the state`; `derives the provenance`; `answers shouldUseUncensoredRoute and its state-only twin`; `answers shouldShowDangerStyling (provenance never changes the colour)`; `answers isClassifierOnDuty`; `answers mayFailOver and its state-only twin` }; `getConciergeReason` { `is null for Moderated, whatever is stored`; `returns the stored reason otherwise` }; `mayFailOver` { `reads a chatless call as Moderated` }; `deriveConciergeModeFromLegacy` (`it.each` over the four-row table); `withConciergeModeFromLegacy` { `derives the state for a chat that carries none`; `leaves a chat that already carries a state untouched` }.
- `…/classifier-switch.test.ts` (NEW): `moves a chat that is still Moderated, with the verdict for the announcement`; `it.each(['locked','unmoderated'])('leaves a chat the operator made %s while the classifier was thinking')`; `does nothing for a missing chat`; `refuses to run in the job child`.
- `…/concierge-state-presentation.test.ts`: `covers all three states, each with a detail sentence and the same hint`; `keeps the helper sentences verbatim`; `gives every state a distinct label, icon and tone`; `falls through to the base for success (Moderated draws no badge and no mark)`; `it.each(ALL_STATES)('reads %s straight off the table with no provenance')`; `uses the operator's sentence for Unmoderated set by the operator`; `names the refusal count when the Concierge moved the chat after refusals`; `still reads sensibly when the refusal count is unknown`; `says the classifier's reading when the Concierge moved the chat on the conversation`; `surfaces categories only for the classifier's own move`; `it.each(['moderated','locked'])` (no categories).
- `…/current-state.test.ts` (NEW): `prefers the stored state over the snapshot (locked mid-flight)`; `falls back to the snapshot without a chat id, a chat, or a working read`; `reads a missing snapshot as Moderated`.
- `…/image-failover.test.ts`: `refused on a Locked chat → refusal-not-permitted (reason locked), never resolves an understudy`; `reads the state at refusal time: a chat locked while the provider was thinking never fails over`; `refused on an Unmoderated chat still asks another uncensored understudy`.
- `…/manual-flip.test.ts`: `it.each(['moderated','unmoderated','locked'])` (no-op on match); `reads a NULL column as Moderated (no-op)`; `it.each([…])` (transition table); `empties the refusal ledger only on a return to Moderated`; `provenance` { `updates provenance silently when the operator adopts the Concierge’s switch`; `never lets the Concierge re-attribute the operator’s choice` }; `the Concierge's own switch` { `announces nothing when the compare-and-set misses (the operator changed the chat meanwhile)`; `refuses to run in the job child, where a buffered write cannot report whether it landed`; `refusals: Moderated → Unmoderated with provenance and the auto-unmoderated announcement`; `classifier: Moderated → Unmoderated with the verdict's announcement`; `it.each([…])` (refused moves) }.
- `…/refusal-ledger.test.ts`: `switches a Moderated Auto-Route chat to Unmoderated exactly once, on the N-th refusal`; `reads the classifier telemetry as no reason to skip: a Moderated chat labelled dangerous still switches`; `an operator return to Moderated empties the ledger, so the next refusal starts afresh`.
- `…/resolver.test.ts`: `per-chat Locked state` { `returns LOCKED settings and source="chat-locked" for a Locked chat`; `respects global settings for a Moderated chat`; `still returns Locked even if no global settings were configured`; `LOCKED settings have mode OFF, all scans disabled and the auto-switch off`; `Locked wins over a global AUTO_ROUTE`; `ignores the legacy conciergeOverride column` }; `per-chat Unmoderated state` { `moderation-exempt chat types win over the Unmoderated state` }.
- `__tests__/unit/migrations/add-chat-concierge-mode.test.ts` (NEW): `follows the refusal ledger and is dated 4.10.0`; `adds the three columns with their defaults`; `backfills every row by the legacy table`; `leaves the legacy column untouched`; `reports progress per candidate row`; `is idempotent: once run, it neither needs to run nor changes anything`; `runs again when a previous run added the columns but died before the backfill`.
- `lib/background-jobs/host/__tests__/job-dispatcher-apply.test.ts` `applyWritesUnsafe — classifier-switch commit hook`: `asks the switch, after commit, for each chat the batch classified dangerous, with its verdict`; `does not run when the main partition fails to commit`; `a failing switch never fails the committed job`.
- Also touched (state-word swaps): `route.greeting-stall`, `scene-state-tracking` (`calls resolveUncensoredCheapLLMSelection for Unmoderated chats`), `story-background-uncensored-target` (5 cases renamed: candid crafting for an operator-Unmoderated chat under global OFF; Unmoderated bound for the uncensored provider stays accurate; Moderated under Auto-Route resends the concealed prompt; Detect Only keeps even an Unmoderated prompt concealed; Unmoderated resends the already-candid prompt), `danger-orchestrator` (`synthesizes flags for Unmoderated chats and reroutes in AUTO_ROUTE mode`), `pre-compute` (`routes through the uncensored cheap-LLM selection in Unmoderated chats`), `refusal-ledger-integration`, `chat-danger-trigger`, `get.dispatch`.
- Client-only tests skipped here: `concierge-mark`, `homepage-components`, `quick-hide-provider`, `NewChatForm`, `useNewChat.request-body`.
- `scripts/concierge-four-state-test.sh` renamed `concierge-three-state-test.sh` (dogfood script, not a unit).
