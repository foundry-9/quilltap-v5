# SURVEY S3 — `ce2f1dabf` (#77) — fresh survey 2026-09-25 at that sha and on v5 main `2aed9a552`

Read-only survey. v4 read via `git show ce2f1dabf[:path]` only. The v4 version moves `4.10.0-dev.88` → `-dev.90`
(`package.json`, `packages/quilltap/package.json`, README badge). 53 files, +2,256/−130.

**Server-side file list (in scope):**
`app/api/v1/chats/[id]/actions/index.ts` (+1), `actions/retry-image-uncensored.ts` (NEW, 227),
`actions/story-background.ts` (+13/−2), `handlers/post.ts` (+2), `chats/[id]/messages/[messageId]/route.ts` (+98/−1),
`chats/[id]/route.ts` (+1, doc comment only), `app/api/v1/messages/[id]/route.ts` (+14/−55, refactor),
`lib/background-jobs/handlers/story-background.ts` (+84/−6), `lib/background-jobs/queue-service.ts` (+7),
`lib/services/chat-message/danger-orchestrator.service.ts` (+13/−16), `chat-message/index.ts` (+1),
`chat-message/orchestrator.service.ts` (+8/−3), `chat-message/regenerate-swipe-stream.ts` (NEW, 76),
`chat-message/regenerate-swipe.service.ts` (+26/−3), `chat-message/tool-execution.service.ts` (+7/−2),
`chat-message/types.ts` (+6), `dangerous-content/image-failover.ts` (+16/−2), `dangerous-content/resolver.service.ts` (+13),
`dangerous-content/retry-uncensored.ts` (NEW, 252), `lantern-notifications/writer.ts` (+85),
`lib/tools/handlers/image-generation-handler.ts` (+8/−1).
Docs: `docs/developer/API.md` (+50), `help/chat-message-actions.md` (+4), `help/story-backgrounds.md` (+4),
`help/the-concierge.md` (+14/−2), `docs/developer/features/concierge-overhaul{,-phase-5-salon-polish}.md`, `docs/CHANGELOG.md`,
`.claude/commands/update-documentation.md`.

---

## A. v4, per file (post-commit line numbers at `ce2f1dabf`)

### A.1 `app/api/v1/chats/[id]/handlers/post.ts` — the chat action map (+2)

Import `handleRetryImageUncensored` added (between `handleRegenerateBackground` and `handleReclassifyDanger`, :34).
`handlePost` (:65) looks the chat up FIRST (`notFound('Chat')`, :73-76), then `dispatchAction(req, {…})` with NO fallback
(:78-127). The map, whole, in literal key order (post-commit, **48 keys**; the new one is #24):

```
 1 regenerate-title        2 rebuild-summary        3 add-tag                4 remove-tag
 5 impersonate             6 set-active-speaker     7 turn                   8 add-participant
 9 update-participant     10 remove-participant    11 rebuild-system-prompt 12 bulk-reattribute
13 set-avatar             14 remove-avatar         15 add-tool-result       16 queue-memories
17 extract-memories-dry-run 18 recall-replay       19 update-tool-settings  20 rng
21 run-tool               22 toggle-agent-mode     23 regenerate-background 24 retry-image-uncensored   ← NEW (:102)
25 reclassify-danger      26 equip                 27 toggle-avatar-generation 28 regenerate-avatar
29 render-conversation    30 active-document       31 open-documents        32 recent-documents
33 open-document          34 close-document        35 read-document         36 resolve-document
37 write-document         38 rename-document       39 delete-document       40 announcement
41 inform                 42 cancel-inform         43 announcement-preview  44 impersonation-voice-preview
45 send-mail              46 merge-conversation    47 scenario              48 save-image
```

Thunk: `'retry-image-uncensored': () => handleRetryImageUncensored(req, chatId, chat, ctx)` — note it gets the loaded
`chat` (like `regenerate-background`). `actions/index.ts:62` adds `export { handleRetryImageUncensored } from './retry-image-uncensored';`.
`chats/[id]/route.ts:39` doc line only: `POST /api/v1/chats/[id]?action=retry-image-uncensored - Redraw a picture (or the story background) on the Concierge's uncensored desk`.

### A.2 `app/api/v1/chats/[id]/actions/retry-image-uncensored.ts` (NEW, 227 lines)

Schema (:35-38) verbatim:
```ts
const retryImageSchema = z.union([
  z.object({ toolMessageId: z.string().min(1) }),
  z.object({ kind: z.literal('background') }),
]);
```
`StoredToolContent { toolName?, arguments?: Record<string,unknown>, provider?, model? }` (:40-46);
`parseToolContent(content)` (:48-55) — `JSON.parse`, non-object/throw → `null`.

`export async function handleRetryImageUncensored(req, chatId, chat: ChatMetadata, ctx: RequestContext): Promise<NextResponse>` (:57-227).
Order of checks (the order IS the contract):

1. `retryImageSchema.safeParse(await req.json().catch(() => ({})))` — fail → **400** `badRequest('Expected { toolMessageId } or { kind: "background" }')`
   → body `{"error":"Expected { toolMessageId } or { kind: \"background\" }"}` (:64-67). Malformed JSON → `{}` → this 400.
   Zod union semantics: first branch wins; object branches STRIP unknown keys, so `{toolMessageId:'x', kind:'background'}` is a
   PICTURE retry; `{toolMessageId:'', kind:'background'}` is a BACKGROUND retry; `{toolMessageId: 5}` alone → 400.
2. `chatSettings = await repos.chatSettings.findByUserId(user.id)` (:68) — read BEFORE branching.
3. **Background arm** — `if ('kind' in parsed.data)` (:71-88):
   - `imageProfileId = await resolveImageProfileForChat(user.id, chat, chatSettings, repos)` (:72);
   - `gate = await resolveImageRetryUnderstudy({ userId, chat, chatSettings, excludeProfileIds: [imageProfileId] })` (:73-78);
   - `!gate.ok` → `logger.info('[DangerousContent] Uncensored background retry refused', { chatId, reason })` (:80) → **409** `conflict(gate.reason)` → `{"error":"locked"}` / `{"error":"no-understudy"}`;
   - else `logger.info('[DangerousContent] Queueing a story background on the uncensored desk', { chatId, understudyProfileId })` (:83-86)
     → `return handleRegenerateBackground(chatId, chat, ctx, { forceUncensored: true })` (:87). **The 409 gate precedes
     every one of `regenerate-background`'s 400s** (not-enabled / no image profile / no characters), and the INFO line fires
     even when `handleRegenerateBackground` then 400s.
4. **Picture arm** (:90-226):
   - `messages = await repos.chats.getMessages(chatId)`; `toolMessage = messages.find(m => m.type === 'message' && m.id === toolMessageId)` (:91-94);
     `!toolMessage || toolMessage.role !== 'TOOL'` → **404** `notFound('Tool message')` → `{"error":"Tool message not found"}` (:95-97).
   - `stored = parseToolContent(toolMessage.content)`; `stored?.toolName !== 'generate_image' || !stored.arguments` → **400**
     `badRequest('Only generate_image pictures can be retried uncensored')` (:98-101).
   - `gate = await resolveImageRetryUnderstudy({ userId, chat, chatSettings, excludeProfileIds: [chat.imageProfileId], trail: toolMessage.routeTrail, answeredBy: { provider: stored.provider, modelName: stored.model } })` (:103-110).
     NB the picture arm excludes `chat.imageProfileId` (the raw column), the background arm the RESOLVED profile.
   - `!gate.ok` → `logger.info('[DangerousContent] Uncensored picture retry refused', { chatId, toolMessageId, reason })` (:113-117) → **409** `conflict(reason)`.
   - `logger.info('[DangerousContent] Retrying a picture on the uncensored desk', { chatId, toolMessageId, understudyProfileId, understudyName })` (:122-127).
   - `result = await executeImageGenerationTool(stored.arguments, { userId, profileId: understudy.profile.id, chatId, callingParticipantId: toolMessage.participantId ?? undefined, primaryVia: 'concierge' })` (:129-135).
   - `!result.success || !result.images || result.images.length === 0` →
     `logger.warn('[DangerousContent] Uncensored picture retry did not produce an image', { chatId, toolMessageId, error: result.error, message: result.message })` (:138-143)
     → **502** `errorResponse(result.message || 'The uncensored desk could not produce the picture', 502, { code: result.error })`
     → body `{"error": <msg>, "details": {"code": <error>}}`; with `result.error` undefined `details` is `{}` (the key survives because `details !== undefined`). Nothing is saved.
   - Trail (:149-154): `priorTrail = (toolMessage.routeTrail ?? []).filter(a => a.outcome !== 'answered')`;
     `routeTrail = result.routeTrail?.length > 0 ? [...priorTrail, ...result.routeTrail] : composeRetryRouteTrail(toolMessage.routeTrail, understudy.profile, 'image')`.
   - `generatedImages: GeneratedImage[]` (:156-165) = `{ id, filename, filepath: img.filepath ?? img.url, mimeType: img.mimeType || 'image/png', size: img.size || 0, width, height, sha256 }` (undefined keys drop on the wire).
   - `retryToolMessage: ToolMessage` (:166-177) = `{ toolName:'generate_image', success:true, content: \`Generated ${n} image(s)\`, arguments: stored.arguments, metadata: { provider: result.provider, model: result.model, expandedPrompt: result.expandedPrompt, routeTrail } }` — NO `callId`/`anchorOffset`/`seq`.
   - `participant = toolMessage.participantId ? chat.participants.find(p => p.id === toolMessage.participantId) : undefined` (:179-181).
   - **Filed +1 ms**: `createdAt = new Date(new Date(toolMessage.createdAt).getTime() + 1).toISOString()` (:184).
   - `saveToolMessages(repos, chatId, user.id, [retryToolMessage], generatedImages, participant?.characterId ?? undefined, toolMessage.participantId ?? undefined, undefined, { createdAt })` (:185-195) — whisperContext `undefined`.
   - Announcement (:200-212): `refused = (toolMessage.routeTrail ?? []).find(a => a.outcome === 'refused')`; if found →
     `postConciergeRefusalAnnouncement({ chatId, kind: 'refusal-rerouted', details: { refusingProvider: refused.provider, refusingModel: refused.modelName, answeringProfileName: understudy.profile.name, purpose: 'tool' } })`.
     A soft refusal (no `refused` row, e.g. `routeTrail: null`) → no announcement.
   - `logger.info('[DangerousContent] Uncensored picture retry posted', { chatId, originalToolMessageId: toolMessageId, toolMessageId: firstToolMessageId, imageCount, announced: !!refused })` (:214-220).
   - **200** `successResponse({ toolMessageId: firstToolMessageId, images: generatedImages, routeTrail })` (:222-226). Not 201.
   - Chat state never written (test pins `chats.update` / `chats.setConciergeMode` not called).

### A.3 `app/api/v1/chats/[id]/actions/story-background.ts` (+13/−2)

`handleRegenerateBackground(chatId, chat, ctx, options: { forceUncensored?: boolean } = {})` (:71-150). `forceUncensored = options.forceUncensored === true` (:84).
Enqueue payload spreads `...(forceUncensored ? { forceUncensored: true } : {})` LAST, after `projectId` (:119) — so a plain
regenerate's payload is byte-unchanged. Log field bags:
- `logger.info('[Chats v1] Queued story background regeneration', { chatId, jobId, imageProfileId, characterCount, forceUncensored })` (:123-129) — `forceUncensored` is a NEW 5th key, present (false) on a PLAIN regenerate too.
- `logger.info('[Chats v1] Story background generation already in progress', { chatId, jobId, imageProfileId, forceUncensoredRequested: forceUncensored })` (:131-136) — new 4th key.
Response unchanged: `{ message, queued: true, jobId }`. **Dedupe trap:** `enqueueStoryBackgroundGeneration` (queue-service.ts:1012-1039) reuses ANY pending/processing story-background job for the chat, so a forced request that hits an in-flight plain job answers 200 "already in progress" and the flag is silently dropped.

### A.4 `app/api/v1/chats/[id]/messages/[messageId]/route.ts` (+98/−1)

Imports widen: `conflict, created` added; `regenerateMessageAsSwipe, streamSwipeRegeneration` from `@/lib/services/chat-message`;
`composeRetryRouteTrail, resolveTextRetryUnderstudy` from retry-uncensored; `MessageEvent` type.
Doc header (:13-16) adds the action. `withActionDispatch` list, whole, in order (:462-470):
```
'override-danger-flag': handleOverrideDangerFlag,     (:49)
'resolve-external-turn': handleResolveExternalTurn,   (:110)
'cancel-external-turn': handleCancelExternalTurn,     (:251)
'save-image': handleSaveImage,                        (:297)
'retry-uncensored': handleRetryUncensored,            (:385)  ← NEW, 5th
```
No default handler → unknown/bare/absent answer `dispatchAction`'s envelope with `availableActions` in this order.

`async function handleRetryUncensored(req, { user, repos }: RequestContext, { id, messageId })` (:385-460). NO try around the
reads (a repo throw → middleware 500). Order:
1. `chat = await repos.chats.findById(id)` → **404** `notFound('Chat')` → `{"error":"Chat not found"}` (:390-393).
2. `allMessages = (await repos.chats.getMessages(id)).filter(m => m.type === 'message')`; `targetMessage = allMessages.find(m => m.id === messageId)` → **404** `notFound('Message')` (:395-401).
3. `targetMessage.role !== 'ASSISTANT'` → **400** `'Only assistant messages can be retried'` (:402-404) — NB NOT the swipe edge's "can be swiped".
4. `targetMessage.systemSender` truthy → **400** `'Staff and system messages cannot be regenerated'` (:405-407).
5. `chatSettings = await repos.chatSettings.findByUserId(user.id)`; `gate = await resolveTextRetryUnderstudy({ repos, userId, chat, chatSettings, targetMessage })` (:409-416);
   `!gate.ok` → `logger.info('[DangerousContent] Uncensored retry refused', { chatId: id, messageId, reason })` (:418-422) → **409** `conflict(reason)` (`{"error":"locked"}` checked first, then `{"error":"no-understudy"}`).
6. `options = { repos, userId, chat, targetMessage, allMessages, activeUserParticipantId: chat.activeTypingParticipantId ?? null, profileOverride: understudy, routeTrail: composeRetryRouteTrail(targetMessage.routeTrail, understudy.profile, 'connection') }` (:427-436).
7. `logger.info('[DangerousContent] Retrying a turn on the uncensored desk', { chatId: id, messageId, understudyProfileId, understudyName })` (:438-443).
8. `req.nextUrl?.searchParams.get('stream') === '1'` → `return streamSwipeRegeneration(options, '[DangerousContent] Uncensored retry:')` (:445-447) — every refusal above is a JSON error BEFORE the stream opens.
9. else `try { newSwipe = await regenerateMessageAsSwipe(options); return created({ message: newSwipe }) }` → **201** `{message}` (:449-451);
   `catch` → `logger.error('[DangerousContent] Uncensored retry failed', { chatId: id, messageId, error: <message> }, err)` (:453-457) → **500** `serverError(error.message || 'Failed to retry uncensored')`.
No body is read at all (only `?stream=`). The chat's Concierge state is never written.

### A.5 `lib/services/chat-message/regenerate-swipe-stream.ts` (NEW, 76) + `app/api/v1/messages/[id]/route.ts` (refactor)

`export function streamSwipeRegeneration(options: Omit<RegenerateSwipeOptions,'onProgress'>, logContext: string): NextResponse` (:29-76).
Body is the old `handleGenerateSwipeStreaming` `ReadableStream` moved verbatim: `onProgress` → `status` → `encodeStatusEvent`, `delta` → `encodeContentChunk`, else `encodeReasoningChunk`;
success → `data: ${JSON.stringify({ done: true, message: newSwipe })}\n\n`; catch → `logger.error(\`${logContext} Streaming swipe generation failed\`, { messageId: options.targetMessage.id, chatId: options.chat.id }, err)` (:55-59)
+ `encodeErrorEvent(encoder, 'Failed to generate alternative response', 'regenerate_failed', <message>)`; `finally safeClose`; `sseStreamResponse(stream)`.
`chat-message/index.ts:65` exports it. `messages/[id]/route.ts` `handleGenerateSwipeStreaming` (:308-324) now calls it with `'[Messages API v1]'`.
**Wire diff (old vs new):** identical frames and headers. The error line renders `'[Messages API v1] Streaming swipe generation failed'` both before and after;
the field bag's `messageId` was the route param and is now `targetMessage.id` — the same value, since the target was resolved by that id.
**NO wire change** on the swipe edge. The retry leg's error line is `'[DangerousContent] Uncensored retry: Streaming swipe generation failed'`.

### A.6 `lib/services/chat-message/regenerate-swipe.service.ts` (+26/−3)

`RegenerateSwipeOptions` (:59-85) gains:
`profileOverride?: { profile: ConnectionProfile; apiKey: string }` (:82) and `routeTrail?: RouteAttempt[] | null` (:84).
In `regenerateMessageAsSwipe` (:91): `connectionProfile = profileOverride?.profile ?? participantResult.connectionProfile`, `apiKey = profileOverride?.apiKey ?? participantResult.apiKey` (:127-128);
`if (profileOverride) logger.info('[RegenerateSwipe] Regenerating on an override profile', { chatId, targetMessageId, responderProfileId, overrideProfileId, overrideProfileName })` (:129-137) (service logger `RegenerateSwipeService`).
The override feeds EVERYTHING downstream of that line: `buildMessageContext({… connectionProfile …})` (:196 — context formatted/limited for the override), `createLLMProvider(connectionProfile.provider, baseUrl)` (:226), `profileParams(connectionProfile)` (:227), `model` (:247), `apiKey` (:252), the watchdog log context (:255-256), and the swipe row's `provider`/`modelName` (:329-330). `cacheKey` stays `character.id`.
Swipe row: `...(routeTrail && routeTrail.length > 0 ? { routeTrail } : {})` (:334) appended AFTER `createdAt: targetMessage.createdAt`. A plain re-roll writes no trail (unchanged).

### A.7 `lib/services/dangerous-content/retry-uncensored.ts` (NEW, 252)

Service logger `createServiceLogger('ConciergeRetryUncensored')`. "Reads only."
- `export type RetryUncensoredRefusal = 'locked' | 'no-understudy'` (:53); `RetryUnderstudyResult<U> = {ok:true, understudy:U} | {ok:false, reason}` (:55-57).
- `export function mayRetryUncensored(chat: Pick<ChatMetadataBase,'conciergeMode'>): boolean` (:62-64) = `conciergeStateMayFailOver(getConciergeState(chat))` — i.e. `state !== 'locked'`. **Off duty does NOT bar it**; exempt chat types (help/brahma) are NOT barred here either (only the policy's `desk` is replaced).
- `export interface AnsweredBy { provider?: string|null; modelName?: string|null }` (:67-70).
- `retryPolicy(chatSettings, chat)` (:77-82) = `{ ...resolveConciergeSettings(chatSettings, chat), desk: resolveConfiguredConciergeDesk(chatSettings) }`.
- `sameModelIds(profiles, answeredBy)` (:85-93): `[]` unless BOTH `provider` and `modelName` truthy; else ids with equal provider AND modelName.
- `trailProfileIds(trail, kind)` (:96-103): `(a.profileKind ?? 'connection') === kind` → `profileId`.
- `export async function resolveTextRetryUnderstudy({ repos, userId, chat, chatSettings, targetMessage })` (:112-170):
  1. `!mayRetryUncensored(chat)` → `logger.info('Uncensored text retry refused: the chat is Locked', { chatId, messageId })` → `{ok:false, reason:'locked'}` (no lookup at all);
  2. `exclude = Set(trailProfileIds(targetMessage.routeTrail, 'connection'))`;
  3. responder: participant by `targetMessage.participantId`; if `participant.characterId` → `repos.characters.findById` → `exclude.add(resolveConnectionProfile(participant, character))`; catch → `logger.debug('Could not resolve the responder profile to exclude from the retry', { chatId, messageId, error })`;
  4. `repos.connections.findAll()` → `sameModelIds(connections, targetMessage)` (the MESSAGE's own `provider`/`modelName`); catch → `logger.debug('Could not list connection profiles to exclude the answering model from the retry', { chatId, messageId, error })`;
  5. `resolveUncensoredTextUnderstudy({ userId, conciergePolicy: retryPolicy(…), exclude: [...exclude] })` (no `turnAttachmentMimeTypes`, no `filter`);
  6. `logger.debug('Resolved the uncensored text retry understudy', { chatId, messageId, excluded, understudyProfileId: id ?? null })`;
  7. `understudy ? {ok:true, understudy} : {ok:false, reason:'no-understudy'}`.
- `export async function resolveImageRetryUnderstudy({ userId, chat, chatSettings, excludeProfileIds: Array<string|null|undefined>, trail?, answeredBy? })` (:178-226):
  Locked → `logger.info('Uncensored image retry refused: the chat is Locked', { chatId })` → locked;
  `exclude = [...new Set([...excludeProfileIds.filter(non-empty string), ...trailProfileIds(trail,'image')])]`;
  if `answeredBy.provider && modelName` → `getRepositories().imageProfiles.findAll()` (NB global repos, not the request's) → append same-model ids not already present; catch → `logger.debug('Could not list image profiles to exclude the answering model from the retry', { chatId, error })`;
  `resolveUncensoredImageUnderstudy({ userId, conciergePolicy: retryPolicy(…), exclude })`;
  `logger.debug('Resolved the uncensored image retry understudy', { chatId, excluded, understudyProfileId })`.
- `export function composeRetryRouteTrail(priorTrail, answering: {id,name,provider,modelName}, profileKind: 'connection'|'image'): RouteAttempt[]` (:234-252):
  `prior = (priorTrail ?? []).filter(a => a.outcome !== 'answered')` then ONE appended row, key order
  `{ profileId, profileName, provider, modelName, via: 'concierge', outcome: 'answered', ...(profileKind === 'image' ? { profileKind } : {}) }`. Never empty/NULL.

### A.8 `lib/services/dangerous-content/resolver.service.ts` (+13)

`export function resolveConfiguredConciergeDesk(globalSettings: ConciergeSettingsCarrier | null | undefined): ResolvedConciergeDesk` (:187-191)
= `deskFrom(readConciergeSettings(globalSettings))` — `deskFrom` (:152-159) maps `uncensoredText/Image/VisionProfileId`, `imagePromptProfileId` → `textProfileId/imageProfileId/visionProfileId/imagePromptProfileId` (`?? null`). All of `readConciergeSettings`, `deskFrom`, `ConciergeSettingsCarrier`, `ResolvedConciergeDesk` are #76's.

### A.9 `lib/services/chat-message/danger-orchestrator.service.ts` (+13/−16) — the Unmoderated branch

`resolveMessageDangerState` (:43). BEFORE (at `3b463d6b1`), in `if (conciergePolicy.routeDirect && !isContinueMode && content) {`:
```ts
const categories = chat.dangerCategories && chat.dangerCategories.length > 0 ? chat.dangerCategories : ['unspecified']
dangerFlags = categories.map(cat => ({ category: cat, score: 1.0, userOverridden: false, wasRerouted: false }))
if (!effectiveProfile.isDangerousCompatible) { … if (routeResult.rerouted) { …
  dangerFlags = markFlagsAsRerouted(dangerFlags, provider, modelName) … } }
return { conciergePolicy, dangerFlags, effectiveProfile, effectiveApiKey }
```
AFTER: the synthesis and the `markFlagsAsRerouted` call are deleted; comment block (:74-82); the return (:114-120) is
`{ conciergePolicy, dangerFlags: undefined, routedDirect: true, effectiveProfile, effectiveApiKey }`. The Moderated path's
return (:215-221) gains `routedDirect: false`. The pre-screen branch (still calling `markFlagsAsRerouted` at :174) is untouched.
File doc comment line 5 reworded. **Consequence:** an Unmoderated turn's USER message no longer gets `dangerFlags` written
(the orchestrator's attach step is guarded on a non-empty list), the log `'[DangerousContent] Rerouted to uncensored provider (Unmoderated chat)'` is unchanged.

### A.10 `lib/services/chat-message/types.ts` (+6) and `orchestrator.service.ts` (+8/−3)

`DangerResolutionResult` (:322-333) gains REQUIRED `routedDirect: boolean` (:330), between `dangerFlags?` and `effectiveProfile`.
orchestrator (:551-556): `let dangerFlags` → `const dangerFlags = dangerState.dangerFlags`; new
`const contentTreatedAsDangerous = (dangerFlags?.length ?? 0) > 0 || dangerState.routedDirect`.
`isDangerousRouted: contentTreatedAsDangerous || streamingState.effectiveProfile.id !== connectionProfile.id` (:1492-1493, was the flags test);
`const contentWasFlaggedDangerous = contentTreatedAsDangerous` (:1628, feeds `attemptEmptyResponseRecovery`). So failover behaviour on an Unmoderated turn is PRESERVED; only the persisted flags move.

### A.11 `lib/services/chat-message/tool-execution.service.ts` (+7/−2)

`saveToolMessages(repos, chatId, userId, toolMessages, generatedImagePaths, characterId?, participantId?, whisperContext?, options?: { createdAt?: string })` (:185-198);
row `createdAt: options?.createdAt ?? new Date().toISOString()` (:241). Every other caller unchanged. (The row's `routeTrail` spread + the debug line `'Writing a Concierge route trail on a TOOL message'` are #73's, :244-255.)

### A.12 `lib/background-jobs/queue-service.ts` (+7)

`StoryBackgroundGenerationPayload` (:185-203) gains `forceUncensored?: boolean` (:202) after `projectId`. No enqueue change.

### A.13 `lib/services/dangerous-content/image-failover.ts` (+16/−2)

`ImageFailoverContext` gains `announceUnresolvedRefusal?: boolean` (:97, default true). Two sites gated `if (ctx.announceUnresolvedRefusal !== false)`:
the Locked exit's `announce(…, 'refusal-not-permitted', primary.profile, undefined, 'locked')` (:282-284) and the no-understudy exit's
`announce(…, 'refusal-no-understudy', primary.profile)` (:319-321). The `ledger(...)` call and the rethrow on both exits are UNGATED;
the WARN `'Refusal not rerouted: no uncensored understudy is available'` still fires. The success exit's `refusal-rerouted` is ungated. Doc (:24-26).

### A.14 `lib/tools/handlers/image-generation-handler.ts` (+8/−1)

`ImageToolExecutionContext` gains `primaryVia?: RouteAttemptVia` (:83). Step 7 (:1283-1291) passes
`context.primaryVia ?? (finalProfile.id !== imageProfile.id ? 'concierge' : 'primary')` into `generateImagesWithProvider(…, primaryVia, chatForOverride)` (:350-359), which hands it to `generateImageWithConciergeFailover(…, { userId, chatId, purpose:'tool', conciergePolicy, chat, primaryVia })` (:430). (The positional param + chokepoint are #73's.)

### A.15 `lib/services/lantern-notifications/writer.ts` (+85)

`export interface LanternRefusalKind { kind: 'background-refused'; provider: string; modelName: string }` (:42-49) — deliberately NOT a member of `LanternNotificationKind`.
TEXT verbatim:
- `buildLanternRefusalContent(refusal)` (:177-179):
  `` `The Lantern's usual painter (${refusal.provider} ${refusal.modelName}) would not take the scene — called it improper and downed brushes. The backdrop stays as it was.` `` (em dash U+2014, curly-free ASCII apostrophe).
- `buildLanternRefusalOpaqueContent(refusal)` (:181-183):
  `` `Story background refused by ${refusal.provider} ${refusal.modelName} on content grounds; the previous backdrop is unchanged.` ``
`export async function postLanternRefusalNotification({ chatId, refusal, routeTrail = null }): Promise<MessageEvent|null>` (:201-247). NOT gated by the image-alert setting. Never throws.
- chat missing → `logger.debug('[LanternNotification] Refusal bubble skipped: chat not found', { context: 'lantern-notifications', chatId })` → `null`;
- row, key order: `{ type:'message', id: randomUUID(), role:'ASSISTANT', content, opaqueContent, attachments: [], createdAt: now, participantId: null, systemSender:'lantern', systemKind: refusal.kind, ...(routeTrail?.length ? { routeTrail } : {}) }` → `repos.chats.addMessage`;
- `logger.info('[LanternNotification] Background refusal posted', { context:'lantern-notifications', chatId, messageId, provider, modelName, routeTrailLength: routeTrail?.length ?? 0 })` (:230-237);
- catch → `logger.error('[LanternNotification] Failed to post background refusal', { context, chatId, error: getErrorMessage(error) }, error)` → `null`.

### A.16 `lib/background-jobs/handlers/story-background.ts` (+84/−6)

`handleStoryBackgroundGeneration(job)` (:121). Imports `postLanternRefusalNotification`, `composeRetryRouteTrail`, `resolveImageRetryUnderstudy`, `ImageUnderstudy`.
1. **Forced lookup** (:232-250), placed after `conciergePolicy`/`isDangerousChat`/`hasUncensoredImageProvider` and AFTER the cheap-LLM "No connection profiles" early return:
   `if (payload.forceUncensored) { gate = resolveImageRetryUnderstudy({ userId: job.userId, chat, chatSettings: chatSettings ?? null, excludeProfileIds: [imageProfile.id] }) }`;
   `!gate.ok` → `logger.info('[StoryBackground] Uncensored retry abandoned at run time', { context:'background-jobs.story-background', jobId, chatId, reason })` → `return` (job COMPLETES, nothing written).
2. `uncensoredImageTarget = forcedUnderstudy !== null || (isDangerousChat && hasUncensoredImageProvider && conciergePolicy.routeDirect)` (:251-253) — so a forced job crafts the CANDID prompt. The existing `'[StoryBackground] Concierge policy resolved'` debug bag is unchanged (`uncensoredImageTarget` reflects the force).
3. Primary seat (:510-534): `if (forcedUnderstudy) { primaryImageProfile = understudy.profile; primaryImageKey = understudy.apiKey; logger.info('[StoryBackground] "Try uncensored": painting on the uncensored understudy', { context, jobId, profileId, profileName }) } else if (uncensoredImageTarget) { …existing routeDirect branch… }`.
4. Chokepoint ctx (:765-775) adds `primaryVia: forcedUnderstudy ? 'concierge' : 'primary'` and **`announceUnresolvedRefusal: false` UNCONDITIONALLY** (every Lantern backdrop, forced or not).
5. **Refusal completes the job** (:776-798): in the catch, `trail = getConciergeTrail(error)`; `refused = trail?.find(outcome==='refused')`; `if (refused && !trail.some(outcome==='answered'))` →
   `logger.info('[StoryBackground] Painter refused the scene; posting the Lantern\'s refusal', { context, jobId, chatId, refusingProvider, refusingModel, conciergeTrail: trail.map(a => ({ profileName, outcome })) })`
   → `await postLanternRefusalNotification({ chatId, refusal: { kind:'background-refused', provider: refused.provider, modelName: refused.modelName }, routeTrail: trail })` → `return`.
   Refusal = the FIRST `refused` row (which after an understudy that failed-not-refused is still the primary). Otherwise the old path: `logger.error('[StoryBackground] Image generation failed', {…})` + `throw new Error('Image generation failed[ after Concierge reroute]: …')` (:799-813) — job fails.
6. Success notification (:996-1004): `routeTrail: forcedUnderstudy && failover.trail.length === 0 ? composeRetryRouteTrail(null, activeImageProfile, 'image') : failover.trail`.

### A.17 Docs (`docs/developer/API.md`, `help/**`)

- API.md +32 at :2921 (`retry-image-uncensored`, after `regenerate-background`) and +18 at :3820 (`retry-uncensored`, after `override-danger-flag`). **Prose/hunk mismatches:** API.md says the picture retry excludes "the chat's image profile and every image profile on the message's trail" — omits the same-provider+model exclusion (review-round addition). It says 409 `no-understudy` "when no uncensored image profile is available" — also returned for Locked-before-lookup ordering (fine). It does not mention the 502 `details.code` bag or the 400 for malformed/union-failing bodies' exact sentence.
- `help/chat-message-actions.md` +4: new `#### Try Uncensored` subsection (shield icon; absent on Locked).
- `help/story-backgrounds.md` +4: new troubleshooting bullet pair "**The painter refused the scene:**" (no Tasks-Queue failure; Try uncensored on the Lantern's note).
- `help/the-concierge.md` +14/−2: Lantern keeps its own counsel paragraph; new `### Try uncensored` section (three bullets + the refusal sentence *There is no uncensored desk to send this to — appoint one under Settings → The Concierge.*); "Not Dangerous" paragraph now says the blur lifts and the chips stay struck through; "What Changes in an Unmoderated Chat" first bullet now "No per-message classification, and no per-message badges." (`the-concierge.md` is ADDED by #76 — v5 has no copy.)

### A.18 Client files that call the NEW actions / read NEW wire fields (contract to pin; client survey covers the rest)

- `app/salon/[id]/concierge-retry.ts` (NEW): `retryUncensoredTurnUrl(chatId, messageId)` = `` `/api/v1/chats/${chatId}/messages/${messageId}?action=retry-uncensored&stream=1` `` (always streamed; POST, no body);
  `describeRetryRefusal(error)` maps the 409 body's `error`: `'no-understudy'` → `'There is no uncensored desk to send this to — appoint one under Settings → The Concierge.'`; `'locked'` → `'This conversation is Locked to the usual desks; set it to Moderated should you wish the Concierge to take things elsewhere.'`; else `null`;
  `isLanternBackgroundRefusal(m)` = `systemSender === 'lantern' && systemKind === 'background-refused'`.
- `app/salon/[id]/hooks/useRegeneration.ts`: `RegenerateOptions { url?: string }`; `fetch(options?.url ?? /messages/${id}?action=swipe&stream=1)`; on `!res.ok` a 409 reads `info.error` through `describeRetryRefusal`. Consumes the SSE frames exactly as the swipe (`status`/`content`/`reasoning`/`done.message`/`error`).
- `app/salon/[id]/hooks/useConciergeRetry.ts` (NEW): `POST /api/v1/chats/${chatId}?action=retry-image-uncensored`, `Content-Type: application/json`, body `{ toolMessageId }` or `{ kind: 'background' }`. Reads ONLY `res.ok`, `res.status === 409`, and `info.error` (never the 200's `toolMessageId`/`images`/`routeTrail`) — the picture arrives via `fetchChat()`, the backdrop via `startBackgroundPolling()` + `notifyQueueChange()`.
- `components/chat/ToolMessage.tsx`: `onTryUncensored(message.id)` when `toolData.toolName === 'generate_image'` (reads the TOOL content's `toolName`).
- `app/salon/[id]/components/system-message-labels.ts`: `'background-refused': 'backdrop refused'`; content-sniff `c.includes('would not take the scene')` → `'background-refused'`; importance `lantern['background-refused'] = 'high'`.
- `MessageRow.tsx`: "Not Dangerous" blur lift reads `dangerFlags[].userOverridden` (existing field; CLIENT-ONLY — no server hunk touches `override-danger-flag`). Retry buttons hidden when `getConciergeState(chat) === 'locked'` (reads `conciergeMode`/`conciergeState`, #75's).

### A.19 Server tests (new/changed) and case names

- `__tests__/unit/api/retry-image-uncensored-action.test.ts` (NEW, top-level `it`, no `describe`; mocks the gate, generator, save, announcer, regenerate-background): `404 for an unknown tool message` · `400 for a tool that is not generate_image` · `409 no-understudy when there is nobody to send it to` · `redraws on the understudy and posts a new TOOL message beside the original, trail via the Concierge` (pins `excludeProfileIds:['img-profile-1']`, `primaryVia:'concierge'`, `callingParticipantId`, `options.createdAt = '2026-09-25T12:00:00.001Z'`, trail `[[img-profile-1,refused,primary],[desk-img,answered,concierge]]`, `refusal-rerouted`/`purpose:'tool'`, no `chats.update`/`setConciergeMode`) · `a soft refusal (a picture that "succeeded") is redrawn without an announcement` · `reports a failed redraw without posting anything` (502) · `queues the Lantern's backdrop with forceUncensored` · `409 locked for the backdrop on a Locked chat`.
- `__tests__/unit/app/api/v1/chats/[id]/messages/[messageId]/retry-uncensored.test.ts` (NEW, top-level `it`): `409 no-understudy when there is nobody to send it to` · `409 locked on a Locked chat` · `regenerates as a swipe on the understudy, trail via the Concierge, and leaves the chat's state alone` · `narrates on the regeneration stream when asked` · `404 for an unknown message` · `refuses a Staff message`.
- `__tests__/unit/lib/services/dangerous-content/retry-uncensored.test.ts` (NEW): `mayRetryUncensored` › `refuses only a Locked chat`; `resolveTextRetryUnderstudy` › `refuses a Locked chat without looking for anyone` · `says no-understudy when nobody can take it` · `excludes the responder's profile and every profile on the trail` · `works whatever the duty roster says: off duty still offers the configured desk`; `resolveImageRetryUnderstudy` › `refuses a Locked chat` · `excludes the chat's image profile and the image profiles on the trail only` · `excludes every image profile on the model that drew the original` · `offers the configured image desk while the Concierge is off duty`; `composeRetryRouteTrail` › `keeps the original's failures and ends on the understudy, via the Concierge` · `is a single Concierge row when the original had no trail`.
- `__tests__/unit/app/api/v1/messages/[id]/route.test.ts` (changed): mocks re-pointed to `regenerate-swipe.service`/`streaming.service`/`request-helpers`, the real `streamSwipeRegeneration` runs over the fakes — no case names change.
- `__tests__/unit/lantern-notifications-writer.test.ts`: new `describe('postLanternRefusalNotification')` › `posts a background-refused Lantern bubble with no attachment, whatever the image-alert setting says` · `never throws`.
- `__tests__/unit/lib/background-jobs/handlers/story-background-uncensored-target.test.ts`: RENAMED `does not reroute while the Concierge is off duty, keeps even an Unmoderated chat's prompt concealed, and the Lantern says so` (was `…and says why`); RENAMED `completes the job with one Lantern refusal bubble, and no Concierge bubble, when there is no uncensored understudy` (was `fails the job, and says so, when …`); NEW `"Try uncensored" paints on the understudy first, crafts candidly, and marks the trail via the Concierge` · `"Try uncensored" abandons quietly when there is no understudy at run time`.
- `__tests__/unit/lib/services/chat-message/danger-orchestrator.service.test.ts`: RENAMED `routes Unmoderated chats direct to the uncensored desk without synthesizing flags` (was `synthesizes flags for Unmoderated chats and routes them direct…`); NEW `marks an Unmoderated turn as routed direct even when the profile is already uncensored-compatible`.
- `__tests__/unit/lib/services/dangerous-content/image-failover.test.ts`: NEW `stays silent on an unresolved refusal when the caller reports it itself (the Lantern)` · `stays silent on a Locked refusal when the caller reports it itself`.
- Client tests (out of scope, listed): `app/salon/concierge-retry.test.ts` (`concierge-retry helpers` › 3 cases), `app/salon/components/MessageRow.concierge.test.tsx` (`MessageRow — "Not Dangerous" clears the blur` › 4; `MessageRow — "Try uncensored"` › 5), `system-message-labels.test.ts` (+1 expectation).

---

## B. v5 counterparts on main `2aed9a552`

| Unit | v5 file(s) + lines | What v5 does now |
|---|---|---|
| Chat POST action list | `crates/quilltap-web/src/wardrobe_routes.rs:416-468` `CHAT_POST_ACTIONS` (47 entries, v4's pre-#77 order verbatim, `regenerate-background` at #23 then `reclassify-danger`); `chat_action_post` :474-575 (`dispatch_required_action`; serves only `equip`/`regenerate-avatar`/`inform`/`cancel-inform` on REST; every other known action → 400 "…ride POST /api/dispatch"); the `// v4 dispatchAction(req, { …forty-seven… })` comment at :479. | Needs `"retry-image-uncensored"` inserted after `"regenerate-background"` (→ 48) + the comment's count. REST serving the action is optional (the pointer sentence names only the four served). |
| `retry-image-uncensored` verb | NONE. `api/types.rs:2397-2406` `ChatRegenerateBackground { chat_id }` is the sibling; `api/chat_media.rs:1342-1475` `chat_regenerate_background` (validate → resolve profile → present characters → `enqueue_story_background_generation`); engine arm `api/engine.rs:4944`. | No retry verb, no `force_uncensored` option; the background edge does not log v4's two `[Chats v1]` INFO lines (grep: absent). A picture retry needs the image-generation driver (host-side, like `quilltap-host/src/images_generate.rs`) — the chat_media verb is DB-only. |
| Message edge `chats/{id}/messages/{messageId}` | `quilltap-web/src/lib.rs` — **no** such route (only `/api/v1/chats/{id}` :437 and `/api/v1/messages/{id}` :370). Message actions are RPC-only: `api/types.rs:2080-2100` `MessageResolveExternalTurn` / `MessageCancelExternalTurn` / `MessageSaveImage` (P4.6ab), engine arms `api/engine.rs:4427-4449`. | No `availableActions` envelope is ever emitted for this edge. |
| `override-danger-flag` | grep across `crates/`, `apps/web/src`, `harness/`: ZERO hits (only `docs/developer/porting/drift-ledger.md` and `work-orders/p4.6ab-courier-images-server.md:213` "verify at lane start"). `db/chats_messages.rs:236` has the `user_overridden` field only. | Never ported — confirmed. |
| `retry-uncensored` message verb | NONE. Nearest: `Request::MessageSwipe { message_id, swipe_index, stream: bool }` (`api/types.rs:271-293`), `api/salon.rs:1197-1256` `message_swipe_generate` (gate: 404 Message → 400 `Only assistant messages can be swiped` → 400 Staff → `driver.generate_swipe(req)` → `progress.emit_done` / `emit_error`), engine `api/engine.rs:1829-1868` (`SwipeProgressEmitter::from_flag(stream, &message_id, event_sender)`). | The narration channel (`EventPayload::SwipeProgress(SwipeProgressPayload{frame})`, `types.rs:5198,5270`, scope-tagged by the message id) can be reused as-is. |
| SSE extraction (`regenerate-swipe-stream.ts`) | `quilltap-web/src/messages_swipe_routes.rs:1-218` (`messages_post`, `stream=1` → `generator_sse::stream_frames(…, swipe_frame, …)`); `services/regenerate_swipe.rs:241-253` `emit_done`/`emit_error` (v4's frame bytes). | Nothing to port for the swipe edge (no wire change). v5 does NOT emit v4's `'[Messages API v1] Streaming swipe generation failed'` ERROR line (grep: absent) — pre-existing absent line; the retry leg would add a second prefix. |
| `regenerate-swipe` `profileOverride` + `routeTrail` | `services/regenerate_swipe.rs:284-340` `RegenerateSwipeOptions` (no override, no trail); body :367-1000 — `connection_profile = resolution.connection_profile` (:439), provider/base_url/model (:739-741), swipe row `provider`/`modelName`/`createdAt` (:958-968). The **API key is not in core**: `StreamingCompletionProvider::stream_message(provider, base_url, params)` (`model/stream.rs:476-481`) — the host resolves keys. `model_context_limit` is injected by the host from the RESPONDER: `quilltap-host/src/spine.rs:1248-1310` `run_swipe` → `preresolve_provider_model(chat_id, target_pid, …)` → `registry_inputs`. Driver trait `api/chat_send.rs:57-95` `SwipeGenerateRequest`/`SwipeGenerateDriver`, impl `spine.rs:2597`. | No override. An override must thread through core (profile) AND host (context limit + key resolution), or the context is budgeted for the wrong model. |
| `retry-uncensored.ts` | NONE. No understudy resolver (`resolveUncensored*Understudy` is #73), no `get_concierge_state` three-state (v5 `services/dangerous_content/chat_override.rs:105` is the P4.D141 FOUR-state), no `conciergeStateMayFailOver`. | Wholly new; every dependency arrives with #73/#75/#76. |
| `resolveConfiguredConciergeDesk` | `services/dangerous_content/resolver.rs` (255 lines) — `resolve_dangerous_content_settings` (:107), the pre-#76 `DangerousContentSettings` mode resolver. | NONE; needs #76's `readConciergeSettings`/`deskFrom`. |
| danger-orchestrator synthesized flags | `services/orchestrator.rs:1386-1471` (the synthesis on `is_dangerous_chat && mode != "OFF" && !continue && content`, `markFlagsAsRerouted` inline :1450-1463, `content_was_flagged_dangerous = true` :1399); attach to the USER message :1793-1812; `is_dangerous_routed = content_was_flagged_dangerous \|\| did_reroute` :2874; `content_was_flagged_dangerous` also at :3287/:3302/:3640/:3656 (empty-response recovery). v5 has NO separate danger-orchestrator module and NO pre-screen branch (the classify is an injected seam, :1320-1327). | After #77, the ONLY v5 flag writer disappears: the attach step at :1793-1812 becomes dead unless the #73–#76 pre-screen branch lands. `content_was_flagged_dangerous` must be kept true on the direct route (v4's `routedDirect`). |
| `saveToolMessages` `createdAt` | `services/tool_execution.rs:701-770` `save_tool_messages(writer, chat_id, _user_id, tool_messages, generated_image_paths, character_id, participant_id, whisper_context)`; `createdAt` = `clock::now_iso()` (:743). Content builder `build_tool_message_content` (~:663-690) already writes `provider`/`model`/`prompt`. | No `options`; ALSO no `routeTrail` spread on the TOOL row (that is #73's) — the retry needs both. |
| Queue payload `forceUncensored` | `services/queue_service.rs:660-710` `enqueue_story_background_generation(db, user_id, chat_id, image_profile_id, character_ids, scene_context, project_id)` — builds the map key by key; `services/story_background_job.rs:54-100` `StoryBackgroundPayload { chat_id, image_profile_id, character_ids, scene_context, project_id }` + `from_json`. | Neither the enqueue nor the decoder knows the key. |
| `announceUnresolvedRefusal` | NONE — v5 has no `generateImageWithConciergeFailover`; `services/image_job_common.rs:185-300` `generate_with_reroute` + `RerouteHandler::StoryBackground` (bug 133 gate, P4.D178). | Needs #73's chokepoint first. |
| Lantern refusal writer | `services/lantern_notifications.rs` (290 lines): `LanternNotificationKind` (:35, `avatar`/`background`/`character-image`), `build_content`/`build_opaque_content` (:78/:108), `is_lantern_image_alert_enabled` (:140), `post_lantern_image_notification` (:172; row with `systemKind` :224). | No refusal kind, no refusal writer, no `routeTrail` on Lantern rows (#73). |
| Story-background refusal completes | `services/story_background_job.rs:196` `handle_story_background_generation`; `uncensored_image_target = is_dangerous_chat && has_uncensored_image_provider` (:304, pre-#75 formula, no `routeDirect`); step 10 `common::generate_with_reroute(…, "Image generation failed", …).await?` (:684-715) — an error propagates and the JOB FAILS. | No forced seat, no refusal bubble, no abandoned-at-run-time arm. |
| Image tool `primaryVia` | `tools/generate_image.rs:392-397` `ImageToolExecutionContext { user_id, profile_id, chat_id, calling_participant_id }`; `execute_image_generation_tool` :1766. | No `primary_via`; the trail plumbing is #73's. |
| `RouteAttempt` wire | `services/route_trail.rs:151-185` — `profileId, profileName, provider, modelName, via, outcome, trigger?, evidence?, detail?`; evidence two-valued (:103). | No `profileKind` (#73); `composeRetryRouteTrail`'s image row appends `profileKind` LAST. |
| Concierge refusal announcement | `services/concierge_notifications.rs` — `build_danger_content` (:160), manual kinds (:286-330), `post_concierge_manual_announcement` (:395), `post_concierge_danger_announcement` (:407). | No `postConciergeRefusalAnnouncement` / `refusal-rerouted` (#73). |
| Action censuses | `crates/quilltap-web/tests/web_edge_action_sites_census.rs` — counts `"action"` string literals outside `query.rs`; `ALLOWED = [("system_data_routes.rs", 1)]` — a list entry `"retry-image-uncensored"` is NOT an `"action"` literal, so it does NOT move. `crates/quilltap-web/tests/action_dispatch_edges.rs` (P4.D220) — seven edges (restore, files/{id}, chats/{id}/files, text-replacements, images, terminals, unlock); no chat-POST list — does NOT move. `crates/quilltap-web/tests/query_param_semantics_equivalence.rs:373-379` `chat_item_post` endpoint (probe action `equip`), bytes cross-compared against v4's REAL route for the `__bare`/`__unknown`/`empty_then_known` shapes → **the `availableActions` array MOVES (48)**; oracle `harness/oracle/cases/query-param-semantics.test.ts`, env `QT_ORACLE_QUERY_PARAM_SEMANTICS`. `crates/quilltap-web/tests/dispatch_wrong_type_census.rs` — enumerates typed fields on every `Request` variant: a new `ChatRetryImageUncensored` / `MessageRetryUncensored` variant ADDS rows (memory note: a new verb with an `*_id` field moves it). The P4.98 tri-state / `request_envelope` census (26 variants) moves only if the new body keys go tri-state. No test counts `CHAT_POST_ACTIONS.len()` literally (grep: none). |

---

## C. Families, oracle cases, fixtures

| Family (test file) | Oracle case / fixture | What MOVES at `ce2f1dabf` |
|---|---|---|
| `crates/quilltap-web/tests/query_param_semantics_equivalence.rs` (`chat_item_post`) | `harness/oracle/cases/query-param-semantics.test.ts` (imports v4's real `chats/[id]/route.ts`) | `availableActions` gains `retry-image-uncensored` at index 23 → RED until `CHAT_POST_ACTIONS` moves. The ONLY recorded chat-POST list. |
| `crates/quilltap-harness/tests/chat_admin_routes_equivalence.rs` | `chat-admin-routes.test.ts`, `build-chat-admin-fixture.ts` | Mentions `toggle-agent-mode` arms only; no envelope — unmoved (verify). |
| `crates/quilltap-harness/tests/cost_background_routes_equivalence.rs` (the P4.6ao `regenerate-background` job-row diff) | `cost-background-routes.test.ts`, `build-cost-background-fixture.ts` | Payload byte-unchanged for a plain regenerate (the spread is conditional). v4 now logs `forceUncensored: false` on the queued INFO line — only matters if a capture pins that line (v5 does not emit it). Target-pin regen should be neutral. |
| `regenerate_swipe_tier3_equivalence.rs` | `regenerate-swipe-tier3.test.ts` | No override/trail in any case → neutral. Needs NEW override arms (profile swap, context limit, `provider`/`modelName` on the row, trail spread after `createdAt`). |
| `salon_swipe_generate_equivalence.rs`; `crates/quilltap-web/tests/messages_swipe_sse_route.rs`, `message_swipe_stream_dispatch_wire.rs`, `swipe_spine/` | `salon-swipe-generate.test.ts` | Refactor-only → neutral (confirm by regen; the v4 unit test was re-mocked, not re-asserted). |
| `story_background_job_tier3_equivalence.rs` | `story-background-job.test.ts` + `fixtures/story-background-job.json` (17 chats; `moderation_recraft`, `moderation_recraft_fails`, `moderation_already_candid`, `flagged_no_profile_moderation` use `Blocked Images`/`blocked-model`) | Already moved by #73/#75/#76 (chokepoint, `routeDirect`, settings). #77 adds: a refused-and-unanswered case now COMPLETES with a `background-refused` Lantern row (+ ledger row) and NO `refusal-no-understudy` Concierge row, where it previously FAILED the job. New `payload.forceUncensored` arms needed (forced success w/ `via:'concierge'` trail; abandoned at run time Locked / no-understudy). Fixture carries `dangerousContentSettings` (widen per #76). |
| `post_office_concierge_lantern_suparna_equivalence.rs` | `post-office-concierge-lantern-suparna.ts` | New builder rows for `buildLanternRefusalContent` / `buildLanternRefusalOpaqueContent` (tier-1 bytes). Existing rows unmoved. |
| `orchestrator_tier3_equivalence.rs` | `orchestrator-tier3.test.ts` + `fixtures/build-orchestrator-fixture.ts`, `orchestrator-tier3.json` | Measured: NO scenario sets `isDangerousChat`/`conciergeOverride` for the Salon spine (fixture supports the keys at :133-135/:491-492; the Rust test comments at :634/:2567-2573 say "every salon chat is not dangerous → no-op"). So the flag-synthesis branch is UNCOVERED — the #77 deletion cannot redden anything; an Unmoderated scenario must be ADDED to prove "no USER-row `dangerFlags`" + `isDangerousRouted` still true. |
| `primary_stream_tier3_equivalence.rs` | `primary-stream-tier3.test.ts` (one `dangerMode === 'AUTO_ROUTE'` ref, :601) | `is_dangerous_routed` is an input (:259/:1175) — the direct-route arm should be driven with it true and no flags. |
| `danger_resolver_equivalence.rs`, `danger_trigger_equivalence.rs`, `danger_routing_equivalence.rs` | `danger-resolver.ts`, `danger-trigger.test.ts`, `danger-routing.test.ts` | Move with #75/#76; #77 adds `resolveConfiguredConciergeDesk` (tier-1 rows: off-duty / Locked / exempt chat still yield the configured desk). |
| `tool_execution_tier2_equivalence.rs`, `tool_execution_process_tier3_equivalence.rs` | `tool-execution-tier2.ts` (5 `createdAt` refs), `tool-execution-process-tier3.test.ts` | Existing rows unmoved (`options` absent). Needs an arm with `options.createdAt` (+1 ms) and — from #73 — the TOOL row `routeTrail`. |
| `image_generation_tier3_equivalence.rs` | `image-generation.test.ts` | `primaryVia` absent → unmoved; a `primaryVia:'concierge'` arm is new. |
| `chats_messages_ops_tier2_equivalence.rs`, `salon_reads_equivalence.rs` | `salon-reads.test.ts` | Carry `dangerFlags` reads — unmoved by #77 (reads). |
| NEW family (proposed): `retry_uncensored_*` | none exists | Needs v4's REAL `retry-uncensored.ts`, both route handlers, and the Lantern writer over a fixture with a Locked chat, an off-duty Concierge with a configured desk, a same-provider+model twin profile, a trail-carrying TOOL row and ASSISTANT row, and a soft-refusal (`routeTrail: null`) row. v4's own unit tests all MOCK the gate — not usable as oracle. |
| Help | `help_tree_equivalence` (+ the embed guard; vendored count 129 per CLAUDE.md) | Three pages change; `the-concierge.md` does not exist in v5 until #76's re-vendor. Ledger P4.D227 re-vendors the whole tree at `08c49319d`. |

---

## D. Traps

1. **Stacked prerequisites (#73–#76) — the exact symbols #77 calls:**
   - #73 `8bd080267`: `resolveUncensoredTextUnderstudy`, `resolveUncensoredImageUnderstudy`, `TextUnderstudy`/`ImageUnderstudy` (`understudy.ts`); `generateImageWithConciergeFailover`, `getConciergeTrail`, `attachTrail`, `announce(...)`, `ImageFailoverContext.primaryVia` (`image-failover.ts`); `postConciergeRefusalAnnouncement` + `'refusal-rerouted'`/`purpose:'tool'`; `RouteAttemptVia`, `RouteAttempt.profileKind`, `routeTrail` on TOOL + Lantern rows and in `saveToolMessages`; `generateImagesWithProvider`'s `primaryVia` param; the rewritten story-background/image-tool around the chokepoint.
   - #74 `49059fb14`: the `ledger(...)` call on the image-failover exits (still runs with `announceUnresolvedRefusal:false`).
   - #75 `4d370a90f`: `getConciergeState`, `conciergeStateMayFailOver`, `chats.conciergeMode`, `shouldUseUncensoredRoute` three-state, `setConciergeMode` (asserted NOT called).
   - #76 `3b463d6b1`: `readConciergeSettings`, `resolveConciergeSettings(chatSettings, chat)` → `ResolvedConciergePolicy` with `.desk`/`.routeDirect`/`.state`/`.source`, `deskFrom`, `ConciergeSettingsCarrier`, `ResolvedConciergeDesk`, `chat_settings.conciergeSettings`; and `help/the-concierge.md`.
   - Pre-existing (not in the chain): `resolveImageProfileForChat`, `resolveConnectionProfile(participant, character)`, `executeImageGenerationTool`, `isParticipantPresent`.
2. **Commit prose vs hunks.** (a) The first commit bullet says the swipe's trail "ends via the Concierge" — true, but `composeRetryRouteTrail` also DROPS the original's `answered` row (only failures kept). (b) API.md's exclusion list omits the provider+model exclusion added in the review commit. (c) "the job completes instead of failing" is ONLY for a trail with a `refused` row and no `answered` row; a failed-not-refused primary still throws. (d) "Concierge's own unresolved refusal bubble is suppressed" — for EVERY Lantern backdrop (`announceUnresolvedRefusal:false` is unconditional), not only forced ones; and only the two unresolved kinds — `refusal-rerouted` still posts. (e) "409 locked / no-understudy" — for the image background arm the 409 precedes `regenerate-background`'s own 400s.
3. **The message-action ORDER.** v4's list is `['override-danger-flag','resolve-external-turn','cancel-external-turn','save-image','retry-uncensored']`. v5 has NO REST leg for `chats/{id}/messages/{messageId}` and has never ported `override-danger-flag`, so v5 emits no `availableActions` for this edge at all — the order question is **moot today** and needs no pinned divergence as long as the new verb stays RPC-only (as P4.6ab's three did). If a lane adds the REST edge, it must either port `override-danger-flag` or carry a both-ways pinned divergence (v4's first entry absent).
4. **`override-danger-flag` / "Not Dangerous" blur lift is CLIENT-ONLY** — confirmed: the message route hunk only ADDS the retry handler; `handleOverrideDangerFlag` (:49-99) is byte-untouched. But note the interaction: after #77 an Unmoderated chat's USER rows carry NO `dangerFlags`, so "Not Dangerous" has nothing to override there.
5. **Two 400 sentences, one guard.** The retry says `'Only assistant messages can be retried'`; the swipe edge says `'…can be swiped'`; the service backstop says `'…can be regenerated'`. v5's `RegenError::NotAssistant` is the backstop's text — the retry's route sentence needs its own home.
6. **The override is two-sided in v5.** v4's `profileOverride` changes the context build (limit/formatting), the provider, params, model, key and the row. v5 splits those: core owns the context build and the row, the HOST owns `model_context_limit` (`spine.rs:1284` from the responder) and key resolution (`stream_message` takes no key). A core-only port budgets context for the RESPONDER's model — a silent divergence no tier-2 comparand of the row would catch unless the case's two profiles have different limits.
7. **+1 ms filing** is computed from the ORIGINAL's `createdAt` via JS `Date` — ISO with millisecond precision, `Z` suffix; a stored `createdAt` without ms still reformats to `.001Z`-style. The swipe path keeps `createdAt` UNCHANGED (grouping), the picture path moves it by +1 ms.
8. **502 `details` shape**: `{ code: result.error }` → `details: {}` when `error` is undefined (key present, object empty). Any v5 envelope must keep the empty object, not drop `details`.
9. **Union decode** (A.2 step 1): first-branch-wins with key stripping; `'kind' in parsed.data` discriminates. A v5 decode that checks `kind` first inverts `{toolMessageId, kind:'background'}`.
10. **Exclusion sets differ by arm**: picture = `[chat.imageProfileId]` (raw column, may be null) + image-kind trail ids + same provider/model of the TOOL content's `provider`/`model`; background (route) = the RESOLVED `imageProfileId`; background (job, re-check) = `imageProfile.id` from the payload's profile, no trail, no answeredBy; text = the responder's `resolveConnectionProfile` + connection-kind trail ids + same provider/modelName as the MESSAGE row. `resolveImageRetryUnderstudy` reads `getRepositories()` directly (not the request's repos).
11. **Dedupe drops the force** (A.3): a forced background that finds a pending plain job answers 200 "already in progress" with `forceUncensoredRequested: true` logged and paints moderated.
12. **The job re-checks at run time** and RETURNS quietly (completed job, no bubble) on Locked/no-understudy; it runs AFTER the cheap-LLM early return, so a missing cheap LLM still wins.
13. **`routedDirect` is REQUIRED on `DangerResolutionResult`**; v5 has no such struct (flags are inline). The v5 port is: stop synthesizing/attaching, keep `content_was_flagged_dangerous` true on the direct route. v5's CURRENT gate is the pre-#75 `is_dangerous_chat && mode != "OFF"`; v4's is `conciergePolicy.routeDirect && !isContinueMode && content` — the #75/#76 port must land first or this hunk has no clean home. This is the one hunk portable in isolation (delete the synthesis + attach, keep the boolean), but doing it before #76 would leave v5's four-state resolver producing a direct route with no flags — behaviour-equivalent for failover, so safe.
14. **Log-line counts to capture-pin (all v5-absent):** 5 in `retry-image-uncensored.ts`, 4 in the message route, 1 in `regenerate-swipe-stream.ts`, 1 in `regenerate-swipe.service.ts`, 7 in `retry-uncensored.ts` (2 INFO, 5 DEBUG), 3 in the Lantern refusal writer, 3 new in `story-background.ts`, 2 bag changes in the `[Chats v1]` pair (which v5 does not emit at all — pre-existing absent lines).
15. **`composeRetryRouteTrail` key order** — `profileKind` is appended AFTER `outcome`, not where #73 places it on ordinary rows (check #73's writer order before sharing one serializer); v5's `RouteAttempt` has no `profileKind` field yet.

---

## E. Open questions

1. **Transport shape for the two new verbs.** Chat POST actions ride `/api/dispatch` in v5; should `retry-image-uncensored` also be REST-served on `chat_action_post` (it would widen that edge's pointer sentence) or stay dispatch-only like `regenerate-background`? And should `retry-uncensored` stay RPC-only (keeps the §D.3 order moot) — with its narration on the existing `SwipeProgress` channel keyed by the target message id?
2. **Picture retry host driver.** The action runs image generation SYNCHRONOUSLY inside a request. v5's `ChatRegenerateBackground` is DB-only; the retry needs a host driver seam (the `images_generate.rs` precedent) — new trait or reuse?
3. **`profileOverride` across the core/host split** (§D.6): does the order widen `SwipeGenerateRequest` with an override that the host resolves (limit + key), or resolve the understudy host-side entirely?
4. **Does the existing `swipeProgress` scope (`progressId` = message id) collide** if an operator presses refresh and Try-uncensored on the same line concurrently? v4 has two independent streams; v5 has one broadcast keyed by the message id.
5. **Oracle for the retry family**: v4's unit tests mock every seam. A real-module jest oracle needs the #73 understudy resolver + #76 settings on the fixture — is it one new family or arms on `regenerate_swipe_tier3` + `image_generation_tier3` + `story_background_job_tier3`?
6. **`routeTrail` on the swipe row vs the #73 import/export schema** — the `via:'concierge'` answering row with `profileKind` must pass the widened `chats-messages.ops` zod / `qtap-export.schema.json`; confirm #73's widening covers an `answered` row carrying `profileKind` (it is new for `answered` rows written by a retry).
7. **Exempt chat types**: `mayRetryUncensored` does not exclude help/brahma chats and `retryPolicy` restores the desk. Is a retry on a help chat reachable in v4's client (the buttons live in the Salon only)? If the verb is dispatch-reachable in v5, record whether to follow v4 (allowed) or refuse.
8. **Ledger interaction**: a forced background refused by the UNDERSTUDY (primaryVia 'concierge') — does #74's ledger count it and could #74's auto-switch then flip a Moderated chat to Unmoderated, contradicting "the retry never changes the chat's state"? (The picture path's `executeImageGenerationTool` also runs through the chokepoint's ledger.) Not resolvable from #77's hunks alone.
9. **Dedupe drop (§D.11)** — follow v4 silently, or file upstream?
10. **`announceUnresolvedRefusal:false` on every Lantern backdrop** — v5's `story_background_job_tier3` recorded the Concierge `refusal-no-understudy` row since #73; confirm at the target pin which cases lose it and gain the Lantern row (and that the `flagged_no_profile_moderation` / `moderation_recraft_fails` cases flip from failed-job to completed-job).
