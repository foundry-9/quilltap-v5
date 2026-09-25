# SURVEY S2 — Part 2 of 4 — §A for `3b463d6b1` ("Concierge overhaul phase 4: the Concierge's own Settings tab (#76)", 183 files, `4.10.0-dev.88`)

Line numbers are post-commit at `3b463d6b1`. Three sub-commits: the tab + settings object; the `-info` tone removal; the review fixes (`readCurrentConciergeOnDuty` at refusal time, off-duty POST ignore, direct routing for Aurora/Lantern, `chatId` into the vision fallback).

## A. v4, per file (#76)

### A.1 `lib/schemas/settings.types.ts` (+111/−) — the new settings object
- `CheapLLMSettingsSchema` LOSES `imagePromptProfileId` (keys now: `strategy, userDefinedProfileId, defaultCheapProfileId, fallbackToLocal, embeddingProvider, allowCheapFallback`).
- `DangerousContentModeEnum` + `DangerousContentSettingsSchema` + type DELETED. `DangerousContentDisplayModeEnum = z.enum(['SHOW','BLUR','COLLAPSE'])` KEPT (386).
- New, 389–460, verbatim:
```ts
export const ConciergeNewChatStateEnum = z.enum(['moderated', 'unmoderated']);
export type ConciergeNewChatState = z.infer<typeof ConciergeNewChatStateEnum>;
export const ConciergeDisplaySettingsSchema = z.object({
  mode: DangerousContentDisplayModeEnum.default('SHOW'),
  showWarningBadges: z.boolean().default(true),
});
export const ConciergePreScreenSettingsSchema = z.object({
  enabled: z.boolean().default(false),
  threshold: z.number().min(0).max(1).default(0.7),
  scanTextChat: z.boolean().default(true),
  scanImagePrompts: z.boolean().default(true),
  scanImageGeneration: z.boolean().default(false),
  customClassificationPrompt: z.string().nullable().optional(),
  summaryClassification: z.boolean().default(false),
});
export const ConciergeSettingsSchema = z.object({
  enabled: z.boolean().default(true),
  uncensoredTextProfileId: UUIDSchema.nullable().optional(),
  uncensoredImageProfileId: UUIDSchema.nullable().optional(),
  uncensoredVisionProfileId: UUIDSchema.nullable().optional(),   // was ChatSettings.uncensoredImageDescriptionProfileId
  imagePromptProfileId: UUIDSchema.nullable().optional(),        // was cheapLLMSettings.imagePromptProfileId
  autoSwitchAfterRefusals: z.number().int().min(0).max(10).default(2),
  newChatsStartAs: ConciergeNewChatStateEnum.default('moderated'),
  display: ConciergeDisplaySettingsSchema.default({ mode: 'SHOW', showWarningBadges: true }),
  preScreen: ConciergePreScreenSettingsSchema.default({ enabled: false, threshold: 0.7, scanTextChat: true, scanImagePrompts: true, scanImageGeneration: false, summaryClassification: false }),
});
export type ConciergeSettings = z.infer<typeof ConciergeSettingsSchema>;
```
- `ChatSettingsSchema`: `uncensoredImageDescriptionProfileId` DELETED (was after `imageDescriptionProfileId`); `dangerousContentSettings` REPLACED in place (between `storyBackgroundsSettings` and `autoLockSettings`, 722) by `conciergeSettings: ConciergeSettingsSchema.default({ enabled: true, autoSwitchAfterRefusals: 2, newChatsStartAs: 'moderated', display: { mode: 'SHOW', showWarningBadges: true }, preScreen: { enabled: false, threshold: 0.7, scanTextChat: true, scanImagePrompts: true, scanImageGeneration: false, summaryClassification: false } })`. ⚠ The `.default()` literal omits the four profile-id keys and `customClassificationPrompt` — so the generateDDL `DEFAULT '...'` JSON and a Zod-materialized default row differ in key set from `DEFAULT_CONCIERGE_SETTINGS` (see Part 4 §D).
- `lib/schemas/types.ts` re-export list: `DangerousContentModeEnum`/`DangerousContentSettingsSchema`/`DangerousContentMode`/`DangerousContentSettings` → `ConciergeSettingsSchema`/`ConciergeSettings`.

### A.2 `lib/schemas/chat.types.ts` (−40): `conciergeOverride` DELETED from BOTH `ChatMetadataSchema` and `ChatMetadataBaseSchema`; comments say the column was dropped by `drop-chat-concierge-override-v1` and bundles derive via `withConciergeModeFromLegacy` "before the row reaches this schema".

### A.3 `lib/services/dangerous-content/resolver.service.ts` (257 lines, rewritten) — quoted whole (semantics)
```ts
export const DEFAULT_AUTO_SWITCH_AFTER_REFUSALS = 2                                   // :29
export const DEFAULT_CONCIERGE_SETTINGS: ConciergeSettings = {                        // :32
  enabled: true, uncensoredTextProfileId: null, uncensoredImageProfileId: null,
  uncensoredVisionProfileId: null, imagePromptProfileId: null,
  autoSwitchAfterRefusals: DEFAULT_AUTO_SWITCH_AFTER_REFUSALS, newChatsStartAs: 'moderated',
  display: { mode: 'SHOW', showWarningBadges: true },
  preScreen: { enabled: false, threshold: 0.7, scanTextChat: true, scanImagePrompts: true,
               scanImageGeneration: false, customClassificationPrompt: null, summaryClassification: false },
}
export interface ResolvedPreScreen { enabled; threshold; scanTextChat; scanImagePrompts; scanImageGeneration; customClassificationPrompt: string|null }   // :56
export interface ResolvedConciergeDesk { textProfileId; imageProfileId; visionProfileId; imagePromptProfileId }  // all string|null   :66
export type ConciergePolicySource = 'global' | 'default' | 'chat-locked' | 'chat-unmoderated' | 'chat-type-exempt' | 'off-duty'   // :73
export interface ResolvedConciergePolicy {                                             // :85
  onDuty: boolean            // global switch on AND chat type not exempt
  state: ConciergeState      // 'moderated' when no chat supplied
  failoverAllowed: boolean   // on duty and not Locked (Unmoderated keeps it as a safety net)
  routeDirect: boolean       // on duty and Unmoderated
  preScreen: ResolvedPreScreen   // on duty, Moderated, preScreen.enabled
  summaryClassification: boolean // on duty, Moderated, opted in
  autoSwitchAfterRefusals: number // 0 unless on duty and Moderated
  desk: ResolvedConciergeDesk
  display: ConciergeDisplaySettings
  newChatsStartAs: ConciergeNewChatState
  source: ConciergePolicySource
}
export type ConciergeSettingsCarrier = { conciergeSettings?: ConciergeSettings | null }   // :117
const NO_PRE_SCREEN = { enabled:false, threshold:1.0, scanTextChat:false, scanImagePrompts:false, scanImageGeneration:false, customClassificationPrompt:null }
const NO_DESK = { textProfileId:null, imageProfileId:null, visionProfileId:null, imagePromptProfileId:null }

export function readConciergeSettings(globalSettings: ConciergeSettingsCarrier|null|undefined): ConciergeSettings {   // :139
  const stored = globalSettings?.conciergeSettings
  if (!stored) return DEFAULT_CONCIERGE_SETTINGS
  return { ...DEFAULT_CONCIERGE_SETTINGS, ...stored,
           display:   { ...DEFAULT_CONCIERGE_SETTINGS.display,   ...(stored.display ?? {}) },
           preScreen: { ...DEFAULT_CONCIERGE_SETTINGS.preScreen, ...(stored.preScreen ?? {}) } }
}
function deskFrom(settings) { return { textProfileId: settings.uncensoredTextProfileId ?? null, imageProfileId: settings.uncensoredImageProfileId ?? null, visionProfileId: settings.uncensoredVisionProfileId ?? null, imagePromptProfileId: settings.imagePromptProfileId ?? null } }   // :152
function preScreenFrom(preScreen) {                                                    // :161
  if (!preScreen.enabled) return { ...NO_PRE_SCREEN, threshold: preScreen.threshold, customClassificationPrompt: preScreen.customClassificationPrompt ?? null }  // the summary classifier still reads threshold + prompt
  return { enabled:true, threshold, scanTextChat, scanImagePrompts, scanImageGeneration, customClassificationPrompt: preScreen.customClassificationPrompt ?? null }
}
export function resolveConciergeSettings(globalSettings, chat?: (ChatLike & { chatType? })|null): ResolvedConciergePolicy {   // :193
  const settings = readConciergeSettings(globalSettings)
  const state = chat ? getConciergeState(chat) : 'moderated'
  const inert = (source) => ({ onDuty:false, state, failoverAllowed:false, routeDirect:false, preScreen: NO_PRE_SCREEN,
    summaryClassification:false, autoSwitchAfterRefusals:0, desk: NO_DESK, display: { mode:'SHOW', showWarningBadges:false },
    newChatsStartAs: settings.newChatsStartAs, source })
  if (chat && isModerationExemptChatType(chat.chatType)) return inert('chat-type-exempt')          // 1. exempt (help, brahma)
  if (!settings.enabled) return inert('off-duty')                                                    // 2. off duty
  if (state === 'locked') return { ...inert('chat-locked'), onDuty: true, display: settings.display } // 3. Locked keeps onDuty + global display
  if (state === 'unmoderated') return { onDuty:true, state, failoverAllowed:true, routeDirect:true, preScreen: NO_PRE_SCREEN,
    summaryClassification:false, autoSwitchAfterRefusals:0, desk: deskFrom(settings),
    display: { ...settings.display, showWarningBadges:false }, newChatsStartAs: settings.newChatsStartAs, source:'chat-unmoderated' }   // 4.
  return { onDuty:true, state, failoverAllowed:true, routeDirect:false, preScreen: preScreenFrom(settings.preScreen),   // 5. Moderated
    summaryClassification: settings.preScreen.summaryClassification,
    autoSwitchAfterRefusals: settings.autoSwitchAfterRefusals ?? DEFAULT_AUTO_SWITCH_AFTER_REFUSALS,
    desk: deskFrom(settings), display: settings.display, newChatsStartAs: settings.newChatsStartAs,
    source: globalSettings?.conciergeSettings ? 'global' : 'default' }
}
```
Named questions: `onDuty`, `failoverAllowed`, `routeDirect`, `preScreen.enabled/scanTextChat/scanImagePrompts/scanImageGeneration/threshold/customClassificationPrompt`, `summaryClassification`, `autoSwitchAfterRefusals`, `desk.{text,image,vision,imagePrompt}ProfileId`, `display.{mode,showWarningBadges}`, `newChatsStartAs`, `state`, `source`. Pure — no I/O. `LOCKED_DANGEROUS_CONTENT_SETTINGS`/`DEFAULT_DANGEROUS_CONTENT_SETTINGS`/`ResolvedDangerousContentSettings` are GONE.

### A.4 `lib/services/dangerous-content/legacy-concierge-settings.ts` (NEW, 154 lines) — whole
```ts
export interface LegacyDangerousContentSettings { mode?; threshold?; scanTextChat?; scanImagePrompts?; scanImageGeneration?; uncensoredTextProfileId?; uncensoredImageProfileId?; displayMode?; showWarningBadges?; customClassificationPrompt?; autoSwitchAfterRefusals? }   // :27, every field optional
export interface LegacyConciergeSources { dangerousContentSettings?: …|null; uncensoredImageDescriptionProfileId?: string|null; cheapLLMSettings?: { imagePromptProfileId?: string|null }|null; hasUnmoderatedChats?: boolean }   // :42
export interface MigratedConciergeSettings { enabled; uncensoredTextProfileId: string|null; uncensoredImageProfileId; uncensoredVisionProfileId; imagePromptProfileId; autoSwitchAfterRefusals; newChatsStartAs: 'moderated'; display: { mode: 'SHOW'|'BLUR'|'COLLAPSE'; showWarningBadges }; preScreen: { enabled; threshold; scanTextChat; scanImagePrompts; scanImageGeneration; customClassificationPrompt: string|null; summaryClassification } }   // :51 (key ORDER = the JSON.stringify order written by the migration)
const DISPLAY_MODES = new Set(['SHOW','BLUR','COLLAPSE'])
function clampThreshold(v) { return typeof v==='number' && v>=0 && v<=1 ? v : 0.7 }
function clampAutoSwitch(v) { return typeof v==='number' && Number.isInteger(v) && v>=0 && v<=10 ? v : 2 }
function bool(v, fallback) { return typeof v==='boolean' ? v : fallback }
function idOrNull(v) { return typeof v==='string' && v.length>0 ? v : null }
export function mapLegacyConciergeSettings(sources): MigratedConciergeSettings {        // :94
  const dc = sources.dangerousContentSettings ?? {}
  const mode = dc.mode==='DETECT_ONLY' || dc.mode==='AUTO_ROUTE' ? dc.mode : 'OFF'      // any other string = OFF
  const classifierWasOn = mode !== 'OFF'
  const enabled = classifierWasOn || sources.hasUnmoderatedChats === true
  return { enabled,
    uncensoredTextProfileId: idOrNull(dc.uncensoredTextProfileId), uncensoredImageProfileId: idOrNull(dc.uncensoredImageProfileId),
    uncensoredVisionProfileId: idOrNull(sources.uncensoredImageDescriptionProfileId),
    imagePromptProfileId: idOrNull(sources.cheapLLMSettings?.imagePromptProfileId),
    autoSwitchAfterRefusals: clampAutoSwitch(dc.autoSwitchAfterRefusals), newChatsStartAs: 'moderated',
    display: { mode: DISPLAY_MODES.has(dc.displayMode ?? '') ? dc.displayMode : 'SHOW', showWarningBadges: bool(dc.showWarningBadges, true) },
    preScreen: { enabled: classifierWasOn, threshold: clampThreshold(dc.threshold), scanTextChat: bool(dc.scanTextChat, true),
      scanImagePrompts: bool(dc.scanImagePrompts, true), scanImageGeneration: bool(dc.scanImageGeneration, false),
      customClassificationPrompt: typeof dc.customClassificationPrompt==='string' && dc.customClassificationPrompt.length>0 ? dc.customClassificationPrompt : null,
      summaryClassification: classifierWasOn } }
}
export type SettingsWithLegacyConcierge<T> = T & LegacyConciergeSources & { conciergeSettings?: MigratedConciergeSettings | Record<string,unknown> | null }   // :130
export function withConciergeSettingsFromLegacy<T extends object>(settings, hasUnmoderatedChats: boolean) {   // :140
  if (settings.conciergeSettings) return settings      // truthy check — an existing object wins, `null`/absent is translated
  return { ...settings, conciergeSettings: mapLegacyConciergeSettings({ dangerousContentSettings: settings.dangerousContentSettings, uncensoredImageDescriptionProfileId: settings.uncensoredImageDescriptionProfileId, cheapLLMSettings: settings.cheapLLMSettings, hasUnmoderatedChats }) }
}
```
Rules: OFF → `enabled:false`, preScreen off, summary off; DETECT_ONLY/AUTO_ROUTE → `enabled:true`, preScreen on, summaryClassification on ("DETECT_ONLY gaining failover is the deliberate behaviour change of record"); OFF + any Unmoderated chat → `enabled:true` with preScreen off. The legacy keys are left on the record "for the repository's schema to strip".

### A.5 `lib/services/dangerous-content/current-state.ts` (+30) — `readCurrentConciergeOnDuty(userId, snapshot: boolean) → Promise<boolean>` (62)
`!userId` → snapshot; `onDuty = readConciergeSettings(await getRepositories().chatSettings.findByUserId(userId)).enabled`; `if (onDuty !== snapshot) logger.info('Concierge on-duty switch changed since the request began', { userId, snapshot, current: onDuty })`; catch → `warn('Could not re-read the Concierge on-duty switch; using the snapshot', { userId, snapshot, error })` → snapshot.

### A.6 `lib/services/dangerous-content/image-failover.ts` (#76 hunk)
- `ImageFailoverContext.settings: DangerousContentSettings` → `conciergePolicy: ResolvedConciergePolicy`. `announce(..., reason?: 'locked')`.
- Refusal log bag: `mode: ctx.settings.mode` → `conciergeSource: ctx.conciergePolicy.source, conciergeState: ctx.conciergePolicy.state`.
- Step 4 after the Locked check: `const failoverAllowed = (ctx.conciergePolicy.failoverAllowed || ctx.conciergePolicy.onDuty) && await readCurrentConciergeOnDuty(ctx.userId, ctx.conciergePolicy.onDuty); if (!failoverAllowed) { logger.info('Refusal not rerouted: the Concierge is off duty', { ...logContext, conciergeSource }); await ledger(…, false); throw attachTrail(primaryError, trail) }` — **NO announcement off duty** (the `refusal-not-permitted`/mode announcement is gone; the `|| onDuty` term lets a chat UNLOCKED mid-call fail over though its snapshot policy said Locked). Understudy lookup passes `conciergePolicy`.

### A.7 `lib/services/chat-message/provider-failover.service.ts` (#76 hunk)
- Every `dangerSettings` option/field → `conciergePolicy: ResolvedConciergePolicy` (`AttemptEmptyResponseRecoveryOptions`, `AttemptUncensoredRetryOptions`, `AttemptHardErrorFailoverOptions`).
- Empty path (304–305): `if (empty && !lockedOut && conciergePolicy.failoverAllowed && await readCurrentConciergeOnDuty(userId, conciergePolicy.onDuty))` → uncensored retry.
- Hard-error path (1028–1031): `if (opts.conciergePolicy?.failoverAllowed && await readCurrentConciergeOnDuty(context.userId, opts.conciergePolicy.onDuty))` → retry; the not-permitted log: `'[Failover] Refusal not rerouted to an uncensored profile: the Concierge policy does not permit it', { chatId, conciergeSource, conciergeState }`.
- `adaptMessagesForProfile(formattedMessages, reroute, repos, userId, { chatId }, { chatId })` — the new 6th arg (options with chatId for the vision fallback).
- `getEmptyResponseReason` text: "…Consider configuring an uncensored text profile in the Concierge settings so refused content can be rerouted to an uncensored provider." (was "…Consider enabling Auto-Route mode in the Concierge settings to automatically reroute dangerous content to an uncensored provider.").

### A.8 `lib/services/concierge-notifications/writer.ts` (#76): `ConciergeRefusalDetails.reason?: 'locked'` only; `refusal-not-permitted` now ALWAYS the Locked sentences (both voices; the "His present instructions forbid him… were he set to Auto-Route" and "The Concierge mode does not permit rerouting…" sentences DELETED). Doc: "An off-duty Concierge announces nothing at all."

### A.9 `lib/services/dangerous-content/{provider-routing,understudy,gatekeeper}.service.ts`
- `resolveProviderForDangerousContent(originalProfile, originalApiKey, conciergePolicy, userId, turnAttachmentMimeTypes=[])` (70): gate `if (!conciergePolicy.routeDirect && !conciergePolicy.failoverAllowed)` → `logger.debug('[DangerousContent] Rerouting not permitted by Concierge policy', { conciergeSource, conciergeState })`, `reason: \`Concierge policy (${source}) does not permit rerouting\`` (was `` `Mode is ${settings.mode}, no rerouting` ``); `configured = understudy.profile.id === conciergePolicy.desk.textProfileId`. Image twin (153) identical with `'[DangerousContent] Image rerouting not permitted by Concierge policy'` and `desk.imageProfileId`.
- `understudy.ts`: `UnderstudyLookup.settings` → `conciergePolicy` ("only its `desk` is read"); explicit ids from `desk.textProfileId` / `desk.imageProfileId`.
- `gatekeeper.service.ts`: `classifyContent(content, cheapLLMSelection, userId, conciergePolicy, chatId?)` and internals read `conciergePolicy.preScreen.threshold` and `preScreen.customClassificationPrompt`.

### A.10 `lib/services/dangerous-content/refusal-ledger.ts` (#76): `threshold = conciergePolicy.autoSwitchAfterRefusals` (the policy folds "off", "off duty", "not Moderated" into 0); `decision = { chatId, count, threshold, conciergeSource, onDuty }`; `debug('Auto-switch check: the auto-switch is off for this chat', decision)`; the separate mode check is gone. `DEFAULT_AUTO_SWITCH_AFTER_REFUSALS` import dropped.

### A.11 `app/api/v1/settings/chat/route.ts` (+58/−)
- 22 `findRetiredConciergeKeys(body)`: pushes `'dangerousContentSettings'` if `typeof b.dangerousContentSettings !== 'undefined'`; `'uncensoredImageDescriptionProfileId'` likewise; `'cheapLLMSettings.imagePromptProfileId'` if `cheapLLMSettings` is an object with that key defined. (Presence test: an explicit `null` also trips it.)
- PUT (361–366), BEFORE destructuring: `if (retired.length > 0) { logger.warn('[Settings v1] Rejected a PUT carrying retired Concierge settings', { userId, retired }); return badRequest(\`Invalid settings: ${retired.join(', ')} ${retired.length === 1 ? 'was' : 'were'} replaced by conciergeSettings\`) }` → `{ error: 'Invalid settings: dangerousContentSettings was replaced by conciergeSettings' }` (400; join order = the three checks' order).
- `updateChatSettings` (195–206): `parsed = ConciergeSettingsSchema.safeParse(conciergeSettings); if (!parsed.success) throw new Error(\`Invalid conciergeSettings: ${issues.map(i => \`${i.path.join('.')}: ${i.message}\`).join('; ')}\`)`; `logger.debug('[Settings v1] Updating Concierge settings', { userId, enabled, preScreen: parsed.data.preScreen.enabled, summaryClassification })`; `updateData.conciergeSettings = parsed.data` (defaults MATERIALIZED — a partial object is stored whole). The outer catch (437–444): `status = errorMessage.includes('Invalid') ? 400 : 500` → `badRequest(errorMessage)`.
- `uncensoredImageDescriptionProfileId` param/assignment REMOVED from `updateChatSettings`.

### A.12 `app/api/v1/chats/route.ts` (#76)
- `requestedConciergeStateAtCreation(requested, chatSettings, chat)` (418): `policy = resolveConciergeSettings(chatSettings, chat)`; if `requested`: `if (!policy.onDuty && requested !== 'moderated') { logger.warn('[Chats v1] Ignoring a requested Concierge state: the Concierge is off duty', { chatId, requested, conciergeSource }); return undefined }` else return requested; else `fallback = policy.onDuty ? policy.newChatsStartAs : undefined; logger.debug('[Chats v1] No Concierge state requested at creation; using the default', { chatId, newChatsStartAs, conciergeSource, applied: fallback ?? 'none' })`. Called at 1455 with the already-loaded `chatSettings`.
- Greeting (787–800): ONE `conciergePolicy = resolveConciergeSettings(await repos.chatSettings.findByUserId(userId), chatRow)`; `logger.debug('[Chats v1] Resolved Concierge policy for greeting', { chatId, conciergeSource, conciergeState: policy.state, routeDirect, failoverAllowed })`. `generateViaUncensoredDesk(trigger)`: `permitted = trigger==='chat-state' ? routeDirect : failoverAllowed`; if not → `debug('[Chats v1] Concierge policy does not permit the uncensored desk for this greeting', { chatId, trigger, conciergeSource })` → null; the info line's `settingsSource` key → `conciergeSource`. Attempt 0: `if (conciergePolicy.routeDirect)` (910). Attempt 3 still `mayFailOver(chatRow)` (994).

### A.13 `app/api/v1/images/route.ts`
- `conciergePolicy = resolveConciergeSettings(chatSettings ?? null, chatForConcierge)`; `debug('[Images v1] Generate: resolved Concierge policy', { chatId, conciergeSource, conciergeState, preScreen: policy.preScreen.enabled, withChat })`.
- NEW direct block: `if (conciergePolicy.routeDirect) { direct = await resolveUncensoredTextUnderstudy({ userId, conciergePolicy, exclude:[profile.id], filter: supportsImageGeneration }); if (direct) { info('[Images v1] Unmoderated chat routed direct to uncensored connection profile', { userId, chatId, originalProfileId, uncensoredProfileId, uncensoredProfileName }); profile = direct.profile } else debug('[Images v1] Unmoderated chat has no uncensored image-capable profile; using original', { userId, chatId }) }`.
- Pre-screen: `if (conciergePolicy.preScreen.enabled && conciergePolicy.preScreen.scanImagePrompts)`; the flagged log's `mode` → `conciergeSource`; reroute gate `conciergePolicy.failoverAllowed`. Failover ctx `conciergePolicy`, understudy lookup `conciergePolicy`.

### A.14 `app/api/v1/memories/route.ts:1096`: `readConciergeSettings(settings).uncensoredTextProfileId ?? null` (regenerate-all's uncensored cheap pick).

### A.15 Other `app/api/**` hunks
- `chats/[id]/actions/impersonation-voice-preview.ts`: inside `shouldUseUncensoredRoute(chat)`: `conciergePolicy = resolveConciergeSettings(chatSettings, chat)`; `debug('[Chats v1] Impersonation voice preview: Unmoderated chat, checking uncensored route', { chatId, conciergeSource, routeDirect })`; gate `conciergePolicy.routeDirect && !profile.isDangerousCompatible`.
- `chats/[id]/actions/memories.ts` (dry run): `conciergePolicy = resolveConciergeSettings(chatSettings, chat)` → ctx key `conciergePolicy`.
- `chats/[id]/files/route.ts`: `ensureImageDescription(repos, userId, chatId, blob)` → `generateImageDescription(fileAttachment, repos, userId, { chatId })`.
- `chats/[id]/messages/[messageId]/route.ts` (resolve-external-turn): `resolveDangerousContentSettings(chatSettings)` (NO chat) → `resolveConciergeSettings(chatSettings, chat)` (WITH chat) into `MemoryChatSettings.conciergePolicy`.

### A.16 `lib/**` consumers with behaviour beyond a rename (details; the sweep table below indexes all)
- `background-jobs/handlers/character-avatar.ts`: `debug('[CharacterAvatar] Concierge policy resolved', { context:'background-jobs.character-avatar', jobId, conciergeSource, preScreen, scanImagePrompts })`; NEW `if (conciergePolicy.routeDirect)` → `resolveImageProviderForDangerousContent(imageProfile, apiKey, conciergePolicy, userId)` before any call, `info('[CharacterAvatar] Unmoderated chat: routed direct to the uncensored desk', { context, jobId, rerouted, profile })`; `else if (preScreen.enabled && preScreen.scanImagePrompts)` → classify; flagged log `mode` → `conciergeSource`; reroute on `failoverAllowed`.
- `chat-danger-classification.ts`: `resolveConciergeSettings(chatSettings, chat)` (WITH chat now); `if (!conciergePolicy.summaryClassification) { debug('[ChatDangerClassification] Summary classification is off for this chat, skipping', { jobId, chatId, conciergeSource, conciergeState }); return }`; classify with policy; verdict `threshold: conciergePolicy.preScreen.threshold`.
- `context-summary.ts` handler: chain gate `if (!conciergePolicy.summaryClassification) debug('[ContextSummary] Summary classification off for this chat; not chaining', { jobId, chatId, conciergeSource }) else enqueue` (resolved WITH `chat`).
- `scene-state-tracking.ts`: `uncensoredProfileId = conciergePolicy.desk.imagePromptProfileId`; pre-classify gate `!isDangerousChat && uncensoredLLMSelection && conciergePolicy.preScreen.enabled`; `uncensoredFallback = conciergePolicy.failoverAllowed ? { conciergePolicy, availableProfiles } : undefined`; `debug('[SceneStateTracking] Concierge policy for scene-state update', { jobId, chatId, conciergeSource, preScreen, failoverAllowed, hasUncensoredFallback })`.
- `story-background.ts`: `hasUncensoredImageProvider = Boolean(conciergePolicy.desk.imageProfileId)`; `uncensoredImageTarget = isDangerousChat && hasUncensoredImageProvider && conciergePolicy.routeDirect`; `debug('[StoryBackground] Concierge policy resolved', { context:'background-jobs.story-background', jobId, conciergeSource, conciergeState, uncensoredImageTarget })`; `uncensoredProfileId = conciergePolicy.desk.imagePromptProfileId`; NEW pre-route: `if (uncensoredImageTarget) { route = resolveImageProviderForDangerousContent(imageProfile, apiKey, conciergePolicy, userId); primaryImageProfile/Key = route.rerouted ? … ; info('[StoryBackground] Unmoderated chat: routed direct to the uncensored desk', { context, jobId, rerouted, profileId }) }` — the crafter gets `provider: primaryImageProfile.provider` and the failover primary is `{ profile: primaryImageProfile, apiKey: primaryImageKey }`; failure log `dangerMode` → `conciergeSource`.
- `title-update.ts`, `memory-extraction.ts`, `carina-memory-extraction.ts`, `lib/chat/context-summary.ts` (adds `debug('[Context Summary] Unmoderated chat; resolving uncensored cheap LLM', { chatId, conciergeSource, routeDirect })`), `pascal/llm-consult.ts`, `memory-processor.ts`, `answer-confirmation.service.ts`, `pre-compute.service.ts`, `message-finalizer.service.ts`, `compression.ts`, `cheap-llm-tasks/types.ts`: `dangerSettings` → `conciergePolicy` pass-through.
- `scheduled-danger-scan.ts`: `wantsSummaryClassification(settings) = readConciergeSettings(settings).enabled && …preScreen.summaryClassification`; pre-check `debug('Danger scan scheduler pre-check', { users, summaryClassificationUsers })`, `info("Danger scan scheduler not started — no user has the Concierge's summary classification on")`, `warn('Could not check Concierge settings, skipping danger scan scheduler', { error })`; per user `debug('Skipping user without summary classification', { userId })`.
- `llm/cheap-llm.ts` `resolveUncensoredCheapLLMSelection(standardSelection, isDangerousChat, conciergePolicy|undefined, availableProfiles)`: gate `!isDangerousChat || !conciergePolicy || !conciergePolicy.routeDirect` → `debug('[CheapLLM] Uncensored cheap LLM selection not applicable; using standard selection', { isDangerousChat, conciergeSource, routeDirect })`; explicit `desk.textProfileId` → `debug('[CheapLLM] Using configured uncensored text profile for cheap LLM', { profileId })`.
- `memory/cheap-llm-tasks/core-execution.ts` `shouldAttemptUncensoredFallback`: `!conciergePolicy.failoverAllowed` → null; `!desk.textProfileId` → null.
- `chat-message/danger-orchestrator.service.ts`: `debug('[DangerousContent] Resolved Concierge policy for message send', { chatId, conciergeSource, conciergeState, routeDirect, preScreen, scanTextChat })`; direct arm `if (conciergePolicy.routeDirect && !isContinueMode && content)` (no `shouldUseUncensoredRoute` import), inner `if (!effectiveProfile.isDangerousCompatible)` → reroute, `info('[DangerousContent] Rerouted to uncensored provider (Unmoderated chat)', …)`, else-arms `debug('[DangerousContent] Unmoderated chat not rerouted', { chatId, reason })` / `debug('[DangerousContent] Unmoderated chat already on an uncensored-compatible profile', { chatId, profile })`; pre-screen arm `preScreen.enabled && preScreen.scanTextChat && …`; flagged log `mode` → `conciergeSource`; reroute on `failoverAllowed`. Result key `conciergePolicy`.
- `memory-trigger.service.ts` `triggerChatDangerClassification`: `if (!conciergePolicy.summaryClassification) { debug('[DangerClassification] Summary classifier not on duty; skipping enqueue', { chatId, conciergeSource, conciergeState }); return }`.
- `orchestrator.service.ts`: `uncensoredFallbackOptions: (conciergePolicy?.routeDirect && cheapLLMSelection) ? { conciergePolicy, availableProfiles, isDangerousChat:true } : undefined` (was `shouldUseUncensoredRoute(chat) && dangerSettings && cheapLLMSelection`); the empty-response warn bag `dangerMode` → `conciergeSource, conciergeState`.
- `image-gen/appearance-resolution.ts` `sanitizeAppearancesIfNeeded(appearances, conciergePolicy, …)`: step 1 `if (!preScreen.enabled && !routeDirect) { debug('[AppearanceResolution] Appearance sanitization not applicable under this Concierge policy', { context:'image-gen.appearance-resolution', chatId, conciergeSource }); return appearances }`.
- `chat/file-attachment-fallback.ts`: `getUncensoredImageDescriptionProfile(repos, userId, chatId?)` reads `chat = chatId ? await repos.chats.findById(chatId).catch(()=>null) : null; policy = resolveConciergeSettings(chatSettings, chat); id = policy.desk.visionProfileId`; none → `debug('Uncensored vision fallback unavailable for this chat', { userId, chatId, conciergeSource })`. New `ImageDescriptionOptions { chatId? }` threaded through `generateImageDescription(file, repos, userId, options={})` / `processFileAttachmentFallback(…, options={})` / `message-attachment-adapter.adaptMessagesForProfile(…, logContext={}, options={})`; callers: `context-builder.service.ts` (`loadAndProcessFiles` + `rehydrateUserAttachments` gains `chatId`; two sites pass `{ chatId: chat.id }`), `photos/auto-describe-attachment.ts` (`AutoDescribeInput.chatId?`), `doc-edit/photo-handlers.ts:587` (`chatId: context.chatId ?? null`), `chats/[id]/files/route.ts`.
- `tools/handlers/image-generation-handler.ts`: `ConciergeChat` gains `chatType?`; `loadSettingsAndBuildCheapLLM` → `{ conciergePolicy, cheapLLMSelection }` with `debug('[Image Generation] Concierge policy resolved', { conciergeSource, conciergeState, preScreen, failoverAllowed, routeDirect })`; cheap-LLM build gated on `preScreen.enabled && (scanImagePrompts || scanImageGeneration)`; NEW 5a `if (conciergePolicy.routeDirect) { imagePromptDangerous = true; try { routeResult = resolveImageProviderForDangerousContent(…); if rerouted { effectiveImageProfile = { ...routeResult.imageProfile, apiKey }; reload via loadAndValidateProfile; info('[Image Generation] Unmoderated chat routed direct to uncensored image provider', { chatId, originalProfile, uncensoredProfile, reason }) } else debug('[Image Generation] Unmoderated chat has no uncensored image provider; using original', { chatId, reason }) } catch { error('[Image Generation] Direct uncensored routing failed, continuing on the original profile', { chatId, error }) } }`; 5b/6b gates on `preScreen.enabled && preScreen.scanImagePrompts` / `scanImageGeneration`; reroutes on `failoverAllowed`; `routesDangerousToUncensored = routeDirect && Boolean(desk.imageProfileId)`; `expandPromptWithDescriptions(…, imagePromptProfileId?)` new last param fed `conciergePolicy.desk.imagePromptProfileId` (crafter used only `if (isDangerous && imagePromptProfileId)`).
- `database/repositories/chat-settings.repository.ts`: default row `conciergeSettings: DEFAULT_CONCIERGE_SETTINGS` (import from the resolver) replaces the `dangerousContentSettings` literal.
- `foundry/subsystem-defaults.ts`: concierge `description: 'Who gets asked when the usual providers refuse, and how flagged content is shown'`, `href: '/settings?tab=concierge'`; `CHILD_SUBSYSTEM_IDS` inserts `'concierge'` after `'salon'`.
- `help-guide/categories.ts`: `content-routing.documents: ['the-concierge', 'story-backgrounds', 'scene-state-tracker']`; `URL_CATEGORY_MAP` gains `{ pattern: '/settings?tab=concierge', categoryId: 'content-routing' }` inserted after the `templates` row, before `images`.
- `almanack/phase2-machinery.ts` `collectImagePromptLLMInfo`: `readConciergeSettings(chatSettings).imagePromptProfileId`. `phase3-ledgers.ts`: `FeatureConfigInfo.dangerousContent` → `concierge: { enabled, newChatsStartAs, autoSwitchAfterRefusals, display:{mode, showWarningBadges}, desk:{textProfileSet, imageProfileSet, visionProfileSet, imagePromptProfileSet}, preScreen:{enabled, threshold, scanTextChat, scanImagePrompts, scanImageGeneration, customClassificationPrompt: boolean}, summaryClassification }` via `conciergeFeatureConfig(chatSettings)`; `uncensoredImageDescriptionProfileConfigured` → `uncensoredVisionProfileConfigured: !!readConciergeSettings(chatSettings).uncensoredVisionProfileId`. `render.ts` (662–690): `'#### The Concierge'`, lines `- **On Duty**: yes/no`, `- **New Chats Start As**: …`, `- **Auto-Switch After Refusals**: never|N`, `- **Display**: MODE (warning badges: yes/no)`, `- **Uncensored Desk**: text pinned|auto-detect, image pinned|auto-detect, vision pinned|none, image-prompt crafter pinned|none`, `- **Pre-Screen**: …`, `- **Pre-Screen Threshold**: …`, `- **Scan Text Chat**`, `- **Scan Image Prompts**`, `- **Scan Image Generation**`, `- **Custom Classification Prompt**: yes/no`, `- **Summary Classification**: yes/no` + blank; the Image Description section keeps its label `- **Uncensored fallback configured**: …` reading the new flag. The `render.test.ts.snap` moved (+15/−).
- `tools/handlers/help-settings-handler.ts`: `case 'chat'` DROPS `dangerousContentSettings`; NEW `case 'concierge'` → `{ conciergeSettings: readConciergeSettings(settings) }` (or `{ message: 'No chat settings configured yet' }`); `overview` gains `conciergeOnDuty: readConciergeSettings(settings).enabled` (after `contextCompression`); invalid-input error `'Invalid input: category is required and must be one of: overview, chat, concierge, connections, embeddings, images, appearance, templates, system'`.
- **Tool-definition bytes** — `help-settings-tool.ts`: enum `['overview','chat','concierge','connections','embeddings','images','appearance','templates','system']` (`concierge` inserted after `chat`); description BEFORE: `…"chat" returns token display, context compression, memory cascade, timestamps, agent mode, and content settings. "connections" returns…`; AFTER: `…"chat" returns token display, context compression, memory cascade, timestamps, and agent mode settings. "concierge" returns the Concierge settings: whether he is on duty, the uncensored desk profiles, the refusal auto-switch, display of flagged content, and the optional classifier pre-screen. "connections" returns…`. `help-navigate-tool.ts` url description example BEFORE `"/settings?tab=chat&section=dangerous-content"` AFTER `"/settings?tab=concierge&section=uncensored-desk"`. `legacy/text-block-prompt.ts` HELP_NAVIGATE example likewise. Count stays 59.
- `startup/prettify.ts`: `'add-concierge-settings-v1': "Moving the Concierge's papers into his own office"`, `'drop-chat-concierge-override-v1': "Retiring the Concierge's old brass switch"`. `instrumentation.ts`: comment only.
- `backup/restore/restore.ts` (381–395): `backupHasUnmoderatedChats = (data.chats||[]).some(c => getConciergeState(withConciergeModeFromLegacy(c)) === 'unmoderated')`; per settings row `settings = withConciergeSettingsFromLegacy(rawSettings, backupHasUnmoderatedChats)`; if changed `debug('Translated pre-4.10 Concierge settings for restore', { settingsId, backupHasUnmoderatedChats })`; then `chatSettings.create(settingsData, { id })`.
- `backup/restore/uuid-remap.ts` (292–410): `remapFields(settings, ['id','imageDescriptionProfileId','defaultRoleplayTemplateId'])` (`uncensoredImageDescriptionProfileId` dropped from the list); same `backupHasUnmoderatedChats`; `remapped.conciergeSettings = { ...concierge, uncensoredTextProfileId?, uncensoredImageProfileId?, uncensoredVisionProfileId?, imagePromptProfileId? each remapped when truthy }` where `concierge = withConciergeSettingsFromLegacy(settings, flag).conciergeSettings`; cheapLLMSettings remap loses `imagePromptProfileId`; the `dangerousContentSettings` remap block DELETED.

### A.17 Migrations (#76) — VERBATIM DDL + backfill
**`migrations/scripts/add-concierge-settings.ts`** (187): `id 'add-concierge-settings-v1'`, `introducedInVersion '4.10.0'`, `dependsOn: ['add-dangerous-content-fields-v1', 'add-chat-concierge-mode-v1']`; re-exports `mapLegacyConciergeSettings`.
- `shouldRun`: SQLite && `chat_settings` exists; column missing → true; else `SELECT COUNT(*) AS n FROM "chat_settings" WHERE "conciergeSettings" IS NULL OR "conciergeSettings" = ''` → `n > 0`.
- `run`: `addColumnIfMissing('chat_settings', 'conciergeSettings', 'TEXT DEFAULT NULL')`; `col(name) = colExists ? "name" : NULL AS "name"`; rows = `SELECT "id", "userId", ${col('dangerousContentSettings')}, ${col('uncensoredImageDescriptionProfileId')}, ${col('cheapLLMSettings')} FROM "chat_settings" WHERE "conciergeSettings" IS NULL OR "conciergeSettings" = ''`; `canSeeChatModes = table chats && column conciergeMode`; per row `hasUnmoderatedChats = canSeeChatModes ? (SELECT COUNT(*) AS n FROM "chats" WHERE "userId" = ? AND "conciergeMode" = 'unmoderated') > 0 : false`; `migrated = mapLegacyConciergeSettings({ dangerousContentSettings: parseJson(row.dangerousContentSettings), uncensoredImageDescriptionProfileId: row…, cheapLLMSettings: parseJson(row.cheapLLMSettings), hasUnmoderatedChats })`; `debug("Translated one user's Concierge settings", { context:'migration.add-concierge-settings', settingsId, enabled, preScreen, hasUnmoderatedChats })`; `UPDATE "chat_settings" SET "conciergeSettings" = ? WHERE "id" = ?` with `JSON.stringify(migrated)` (key order = `MigratedConciergeSettings` literal order: enabled, uncensoredTextProfileId, uncensoredImageProfileId, uncensoredVisionProfileId, imagePromptProfileId, autoSwitchAfterRefusals, newChatsStartAs, display{mode, showWarningBadges}, preScreen{enabled, threshold, scanTextChat, scanImagePrompts, scanImageGeneration, customClassificationPrompt, summaryClassification}); `reportProgress(i+1, n, 'settings')`. `parseJson` returns null on non-string/empty/invalid/non-object. Logs: `debug('Backfilling the Concierge settings', { context, candidates, canSeeChatModes })`, `info('Added the Concierge settings column and backfilled it', { context, columnsAdded, rowsBackfilled, durationMs })`, `error('Failed to add the Concierge settings column', …)`; message `` `Added ${columnsAdded} Concierge settings column(s); backfilled ${rowsBackfilled} settings row(s)` ``. **Every row is written** (even all-default), unlike the mode migration. The old columns are NOT dropped.

**`migrations/scripts/drop-chat-concierge-override.ts`** (90): `id 'drop-chat-concierge-override-v1'`, `4.10.0`, `dependsOn: ['add-chat-concierge-mode-v1']`; `shouldRun` = SQLite && `chats` exists && `sqliteColumnExists('chats','conciergeOverride')`; `run`: `debug('Dropping the legacy conciergeOverride column from chats', { context:'migration.drop-chat-concierge-override' })`; `getSQLiteDatabase().exec('ALTER TABLE "chats" DROP COLUMN "conciergeOverride"')`; `info('Dropped the legacy conciergeOverride column from chats', { context, durationMs })`; message `'Dropped chats.conciergeOverride'`, `itemsAffected: 1`. (SQLite ≥ 3.35; v5's vendored 3.53.2 qualifies.)

**`migrations/scripts/index.ts`**: imports 319–320; array (419–835): `… addChatRefusalLedgerMigration (828), addChatConciergeModeMigration (830), addConciergeSettingsMigration (832), dropChatConciergeOverrideMigration (834)` — the LAST four entries; exports 1234/1236.

### A.18 `public/schemas/**` — NOT touched by #76 (the export schema still carries `conciergeOverride` deprecated + the three mode keys from #75; no `conciergeSettings` in the export schema's chat-settings section was added by #76 — verify against `chatSettings` in that schema when re-vendoring).

### A.19 `help/**` (#76: 12 files, +284/−382)
| file | gist |
|---|---|
| `dangerous-content.md` | DELETED (352 lines) |
| `the-concierge.md` | ADDED (237 lines). Headings: The Concierge; Where His Papers Used to Be; What Is Never Moderated; The Three Postures of a Chat (Moderated / Unmoderated / Locked / Former names / Announcements and marks); Card One: On Duty; Card Two: The Uncensored Desk (Choosing an uncensored provider); Card Three: When a Provider Refuses (The refusal rule / When a picture is refused / When the Concierge switches a chat); Card Four: Display; Card Five: Pre-screening (Advanced) (How the classifier decides / Reading each chat's summary); Story Background Prompts; What Changes in an Unmoderated Chat; Badges Without the Uncensored Desk; Quick-Hide Integration; In-Chat Settings Access; In-Chat Navigation; Related Topics |
| `settings.md` (+26/−) | the Concierge tab added to the tab list/order |
| `chat-settings-ai-services.md` (12), `provider-recommendations.md` (15) | the vision fallback + crafter pickers moved to the Concierge tab |
| `chat-settings.md`, `chats.md`, `image-generation-profiles.md`, `memory-regenerate.md`, `quick-hide.md`, `scene-state-tracker.md`, `story-backgrounds.md` | cross-link/slug swaps to `the-concierge` and `/settings?tab=concierge` |
Tree count stays 129 (one deleted, one added).

### A.20 Client files reading NEW/RENAMED wire fields (#76) — for the contract pin
| file | fields |
|---|---|
| `app/salon/[id]/components/MessageRow.tsx`, `VirtualizedMessageList.tsx` | `conciergeDisplay?: ChatSettings['conciergeSettings']['display']`; `showDangerBadges = hasDangerFlags && conciergeDisplay?.showWarningBadges !== false` |
| `components/chat/ChatSidebar.tsx`, `ConciergeOffDutyHint.tsx` | `settings.conciergeSettings?.enabled !== false` (select disabled off duty, hint links to the tab) |
| `components/new-chat/hooks/useNewChat.ts` | `onDuty = settings?.conciergeSettings?.enabled !== false`; default `onDuty && conciergeSettings?.newChatsStartAs === 'unmoderated' ? 'unmoderated' : 'moderated'`; POST includes `conciergeState` when `state.conciergeState !== 'moderated' \|\| conciergeServerDefault !== 'moderated'` |
| `components/settings/chat-settings/hooks/useChatSettings.ts`, `types.ts` | `conciergeSettings?: ConciergeSettings`; `handleConciergeUpdate` deep-merges `display`/`preScreen` over `DEFAULT_CONCIERGE_SETTINGS` + stored and PUTs `{ conciergeSettings: next }` (whole object) |
| `components/settings/tabs/ConciergeTabContent.tsx` + `concierge-settings/{OnDutyCard,UncensoredDeskCard,RefusalsCard,DisplayCard,PreScreeningCard}.tsx` | reads every key: `enabled`, `uncensoredText/Image/VisionProfileId`, `imagePromptProfileId`, `autoSwitchAfterRefusals`, `newChatsStartAs`, `display.mode/showWarningBadges`, `preScreen.*` |
| `components/settings/chat-settings/ImageDescriptionSettings.tsx` | loses the uncensored picker |
| `components/help-chat/hooks/useHelpChatStreaming.ts`, `app/foundry/concierge/page.tsx`, `app/settings/SettingsView.tsx`, `app/salon/new/NewChatPageClient.tsx` | tab/slug wiring |
| `DangerousContentSettings.tsx` | DELETED (385 lines); `ChatTabContent.tsx` −19 |

### A.21 Test files (#76) — server-unit case names
- `app/api/v1/chats/route.concierge-state` `newChatsStartAs`: `applies the operator default when the request names no state`; `lets an explicit Moderated request beat an Unmoderated default`; `lets an explicit Locked request beat an Unmoderated default`; `does nothing with a Moderated default`; `ignores the default while the Concierge is off duty`; `it.each(['unmoderated','locked'])`; `sends an Unmoderated chat to the frank desk first`.
- `app/api/v1/settings/chat/route.concierge` (NEW) `PUT /api/v1/settings/chat — conciergeSettings`: `round-trips a conciergeSettings object, filling defaults`; `rejects a malformed conciergeSettings with 400`; `it.each([…])` (each retired key → 400); `still accepts cheapLLMSettings without the crafter`.
- `background-jobs/chat-danger-classification`: `skips if summary classification is off`; `skips if the Concierge is off duty`; `skips when there are no stored Concierge settings (summary classification defaults off)`; `hands the classifier the policy resolved for this chat`.
- `background-jobs/context-summary-chaining`: `does not chain if summary classification is off`; `does not chain if the Concierge is off duty`; `it.each(['unmoderated','locked'])('does not chain for a %s chat')`.
- `background-jobs/scheduled-danger-scan`: `skips users without summary classification`; `skips users whose Concierge is off duty, even with summary classification ticked`; `skips users with no conciergeSettings (summary classification defaults off)`; `sweeps only the users who opted in`; `skips Unmoderated and Locked chats`; `scheduleDangerScan` { `does not start when no user has summary classification on`; `starts when some user has summary classification on`; `does not start when the settings cannot be read` }.
- `image-gen/appearance-resolution` `Sanitization skipped when the policy neither pre-screens nor routes direct` { `it.each([…])`; `should return unchanged when the Concierge is off duty`; `should sanitize an Unmoderated chat whose scene cannot reach the uncensored desk` }.
- `lib/background-jobs/handlers/character-avatar-sha256`: `sends an Unmoderated chat straight to the uncensored desk before the call`. `scene-state-tracking`: `pre-classifies only when the pre-screen is on`; `offers the uncensored fallback only where the policy allows failover`. `story-background-uncensored-target`: `crafts candidly for an operator-Unmoderated chat with the pre-screen off (real predicate + resolver)`; `conceals even an Unmoderated chat while the Concierge is off duty (real predicate + resolver)`; `reroutes a Moderated chat while the Concierge is on duty, resending the concealed prompt unchanged`; `does not reroute while the Concierge is off duty, keeps even an Unmoderated chat's prompt concealed, and says why`; `sends an Unmoderated chat straight to the uncensored desk: the ordinary painter never sees the candid prompt`.
- `lib/backup/restore-field-fidelity`: `derives conciergeMode from a legacy conciergeOverride ('UNCENSORED') before the schema strips it`; `translates pre-4.10 Concierge settings into conciergeSettings through restore`; `leaves a 4.10 archive's conciergeSettings exactly as stored`.
- `lib/chat/file-attachment-fallback`: `it.each([…])` (Locked/exempt chats get no vision fallback); `never falls back to the uncensored vision profile while the Concierge is off duty`.
- `lib/help-guide/categories`: `should return content-routing for /settings?tab=concierge`. `lib/import/concierge-legacy-import` (NEW) `executeImport — legacy conciergeOverride`: `derives Locked from conciergeOverride 'OFF' on a fresh insert`; `derives Unmoderated from conciergeOverride 'UNCENSORED' on the duplicate path`; `derives Unmoderated (the Concierge, classifier) from a flagged chat with no override`; `the schema no longer declares conciergeOverride, which is why derivation must come first`.
- `lib/pascal/llm-consult`: `hands the resolved Concierge policy and profile list to the executor`. `danger-orchestrator.service`: `returns the original profile unchanged when the Concierge is off duty`; `does not pre-screen a Moderated chat when the pre-screen is off`; `synthesizes flags for Unmoderated chats and routes them direct to the uncensored desk`. `provider-failover-refusal`: `does not ask the uncensored desk when the Concierge was sent off duty mid-turn`; `does not ask the uncensored desk when the policy does not allow failover`; `never asks the uncensored desk on a Locked chat, even with the Concierge on duty, and says why`.
- `dangerous-content/concierge-state-presentation`: `leaves the danger base rule unsuffixed and names the one modifier`. `image-failover`: `refused while the Concierge is off duty → no announcement, rethrow with trail`; `refused after the Concierge was sent off duty mid-call → no understudy, no announcement`; `a chat unlocked while the provider was thinking fails over, though its snapshot policy was Locked`. `provider-routing` `policy-based routing` ×2 (text/image): `returns original profile when the Concierge is off duty`; `returns original profile when the chat is Locked`; `reroutes an Unmoderated chat (the policy routes direct)`. `refusal-ledger`: `never switches while the Concierge is off duty`. `understudy`: `never reads the policy gates: the same answer off duty, Moderated, Unmoderated and Locked`.
- `dangerous-content/resolver` (rewritten): `DEFAULT_CONCIERGE_SETTINGS` { `is on duty with the pre-screen and summary classifier off`; `switches after the default number of refusals and starts chats Moderated`; `shows content with warning badges` }; `readConciergeSettings` { `returns the defaults when there is no settings row`; `returns the defaults when conciergeSettings is missing`; `fills gaps in nested objects from the defaults` }; `resolveConciergeSettings` { `enabled: false (off duty)` { `allows nothing anywhere, with or without a chat`; `still reports the state and newChatsStartAs` }; `Locked` { `allows no failover, no pre-screen, no auto-switch, and empties the desk`; `keeps the global display settings` }; `Unmoderated` { `routes direct, keeps failover as a safety net, and never pre-screens or auto-switches`; `stands the configured desk behind the chat`; `hides warning badges` }; `Moderated` { `uses the global values`; `treats a missing chat as the global Moderated default`; `treats a chat with no conciergeMode as Moderated`; `does not pre-screen unless the pre-screen is enabled, but keeps threshold and prompt`; `reads summaryClassification off the global opt-in`; `honours autoSwitchAfterRefusals: 0 (never)` }; `exempt chat types` { `it.each(['help','brahma'])('%s chats get nothing, whatever the settings and state')` }; `missing conciergeSettings` { `resolves the defaults with source "default"` } }.
- `tools/handlers/help-settings-handler`: `returns the Concierge settings, defaults filled, for the "concierge" category`. `tools/help-navigate-tool`: `returns true for /settings?tab=concierge&section=uncensored-desk`. `tools/image-generation-concierge-failover`: `with the Concierge off duty, fails with the trail and announces nothing`; `routes an Unmoderated chat straight to the uncensored desk, with no refusal to announce`. `llm-cheap-llm`: `should return standard selection when the Concierge policy is undefined`; `…when the Concierge is off duty`; `…for a Moderated chat (the policy does not route direct)`.
- `migrations/add-concierge-settings` (NEW): `mapLegacyConciergeSettings` { `OFF → off duty, pre-screen and summary classification off`; `it.each(['DETECT_ONLY','AUTO_ROUTE'])('%s → on duty with pre-screen and summary classification')`; `carries the scans, threshold, prompt, desk, display and auto-switch across`; `no dangerousContentSettings at all reads as the retired default (OFF) with schema defaults`; `OFF with an Unmoderated chat stays on duty so that chat keeps its desk, pre-screen off` }; `add-concierge-settings-v1` { `adds the column and backfills every row from its own sources`; `keeps an OFF user with an Unmoderated chat on duty`; `tolerates a database with none of the legacy columns`; `is idempotent: a second run has nothing to do and changes nothing` }. `migrations/drop-chat-concierge-override` (NEW): `runs after the mode backfill`; `drops the column when present`; `has nothing to do once the column is gone`.
- `services/chat-danger-trigger`: `skips when the operator has not opted into the summary classifier`; `skips when the Concierge is off duty`; `skips when no Concierge settings are stored (summary classifier off by default)`.
- Also touched: `almanack/__snapshots__/render.test.ts.snap`, `tool-definitions-snapshot.test.ts.snap`, `helpers/almanack/fixture.ts`, `memory-recap`, `title-update`, `context-summary-*`, `gatekeeper*`, `message-finalizer`, `pre-compute`, `refusal-ledger-integration`, `carina-memory-extraction` (mock key renames `resolveDangerousContentSettings` → `resolveConciergeSettings`; mocks return `{ onDuty:false, routeDirect:false, failoverAllowed:false, source:'off-duty' }`).

### A.22 THE CONSUMER SWEEP (#76) — every read that became `resolveConciergeSettings(...)` / `readConciergeSettings(...)`
| # | v4 file:line (post) | question / field | BEFORE | AFTER |
|---|---|---|---|---|
| 1 | `app/api/v1/chats/[id]/actions/impersonation-voice-preview.ts:88-94` | `routeDirect` | `resolveDangerousContentSettings(chatSettings, chat).settings.mode === 'AUTO_ROUTE' && !profile.isDangerousCompatible` | `resolveConciergeSettings(chatSettings, chat).routeDirect && !profile.isDangerousCompatible` |
| 2 | `…/actions/memories.ts:131` (dry run) | pass-through | `{ settings: dangerSettings } = resolveDangerousContentSettings(chatSettings, chat)` → ctx `dangerSettings` | `conciergePolicy = resolveConciergeSettings(chatSettings, chat)` → ctx `conciergePolicy` |
| 3 | `…/messages/[messageId]/route.ts:180` | pass-through (memory trigger) | `resolveDangerousContentSettings(chatSettings)` (no chat) | `resolveConciergeSettings(chatSettings, chat)` |
| 4 | `app/api/v1/chats/route.ts:787` (greeting) | `routeDirect` (attempt 0), `failoverAllowed` (content-filter) | per attempt `resolved.settings.mode !== 'AUTO_ROUTE'`; attempt 0 `shouldUseUncensoredRoute(chatRow)` | ONE policy; `permitted = trigger==='chat-state' ? routeDirect : failoverAllowed`; attempt 0 `conciergePolicy.routeDirect` |
| 5 | `app/api/v1/chats/route.ts:418` (create) | `onDuty`, `newChatsStartAs` | (none — request only) | `requestedConciergeStateAtCreation` |
| 6 | `app/api/v1/images/route.ts:221` | `routeDirect` (NEW direct block), `preScreen.enabled && preScreen.scanImagePrompts`, `failoverAllowed` | `dangerSettings.mode !== 'OFF' && dangerSettings.scanImagePrompts`; `mode === 'AUTO_ROUTE'` | as named |
| 7 | `app/api/v1/memories/route.ts:1096` | `uncensoredTextProfileId` | `settings?.dangerousContentSettings?.uncensoredTextProfileId ?? null` | `readConciergeSettings(settings).uncensoredTextProfileId ?? null` |
| 8 | `lib/background-jobs/handlers/carina-memory-extraction.ts:120` | pass-through | `resolveDangerousContentSettings(chatSettings, chat).settings` | `resolveConciergeSettings(chatSettings, chat)` |
| 9 | `…/character-avatar.ts:226` | `routeDirect` (NEW pre-route), `preScreen.enabled && scanImagePrompts`, `failoverAllowed` | `mode !== 'OFF' && scanImagePrompts`; `mode === 'AUTO_ROUTE'` | as named |
| 10 | `…/chat-danger-classification.ts:137` | `summaryClassification`; `preScreen.threshold` | `resolveDangerousContentSettings(chatSettings).settings.mode === 'OFF'` → bail | `resolveConciergeSettings(chatSettings, chat).summaryClassification` false → bail |
| 11 | `…/context-summary.ts:80` (handler chain) | `summaryClassification` | `mode !== 'OFF'` → enqueue | `summaryClassification` → enqueue |
| 12 | `…/memory-extraction.ts:94` | pass-through | `.settings` | policy |
| 13 | `…/scene-state-tracking.ts:76,88,134,239` | `routeDirect` (via cheap-llm), `desk.imagePromptProfileId`, `preScreen.enabled`, `failoverAllowed` | `chatSettings?.cheapLLMSettings?.imagePromptProfileId`; `mode !== 'OFF'` ×2 | as named |
| 14 | `…/story-background.ts:204,209,214,280,478` | `desk.imageProfileId`, `routeDirect`, `desk.imagePromptProfileId`, NEW pre-route | `Boolean(dangerSettings.uncensoredImageProfileId)`; `mode === 'AUTO_ROUTE'`; `cheapLLMSettings?.imagePromptProfileId` | as named |
| 15 | `…/title-update.ts:106` | pass-through to cheap-llm | `.settings` | policy |
| 16 | `lib/background-jobs/scheduled-danger-scan.ts:51,134` | `enabled && preScreen.summaryClassification` | `resolveDangerousContentSettings(settings).settings.mode !== 'OFF'` | `readConciergeSettings(settings).enabled && …preScreen.summaryClassification` |
| 17 | `lib/backup/restore/restore.ts:385`, `uuid-remap.ts:295-308` | translation | (raw copy) | `withConciergeSettingsFromLegacy(…, backupHasUnmoderatedChats)` |
| 18 | `lib/chat/context-summary.ts:361` | pass-through | `.settings` | policy |
| 19 | `lib/chat/file-attachment-fallback.ts:187` | `desk.visionProfileId` | `chatSettings?.uncensoredImageDescriptionProfileId` | `resolveConciergeSettings(chatSettings, chat).desk.visionProfileId` (chat by `chatId`) |
| 20 | `lib/database/repositories/chat-settings.repository.ts:222` | default row | `dangerousContentSettings: {mode:'OFF',…}` | `conciergeSettings: DEFAULT_CONCIERGE_SETTINGS` |
| 21 | `lib/image-gen/appearance-resolution.ts:389` | `preScreen.enabled \|\| routeDirect` | `dangerSettings.mode === 'OFF'` → return | `!preScreen.enabled && !routeDirect` → return |
| 22 | `lib/llm/cheap-llm.ts:299,308` | `routeDirect`, `desk.textProfileId` | `mode === 'OFF'`; `dangerSettings.uncensoredTextProfileId` | as named |
| 23 | `lib/memory/cheap-llm-tasks/core-execution.ts:421,424,433` | `failoverAllowed`, `desk.textProfileId` | `mode !== 'AUTO_ROUTE'`; `uncensoredTextProfileId` | as named |
| 24 | `lib/memory/memory-processor.ts:394,398` | pass-through | `ctx.dangerSettings` | `ctx.conciergePolicy` |
| 25 | `lib/pascal/llm-consult.ts:92,95,111` | pass-through | `resolveDangerousContentSettings(chatSettings, chat).settings` | policy |
| 26 | `lib/services/chat-message/danger-orchestrator.service.ts:62,74,84,108,113,160` | `routeDirect`, `preScreen.enabled && scanTextChat`, `failoverAllowed` | `shouldUseUncensoredRoute(chat) && mode !== 'OFF'`; inner `mode === 'AUTO_ROUTE'`; `mode !== 'OFF' && scanTextChat`; `mode === 'AUTO_ROUTE'` | as named |
| 27 | `…/memory-trigger.service.ts:178` | `summaryClassification` | `mode === 'OFF'` → return | `!summaryClassification` → return |
| 28 | `…/orchestrator.service.ts:1213` | `routeDirect` | `shouldUseUncensoredRoute(chat) && dangerSettings && cheapLLMSelection` | `conciergePolicy?.routeDirect && cheapLLMSelection` |
| 29 | `…/provider-failover.service.ts:304,1028` | `failoverAllowed` + `readCurrentConciergeOnDuty` | `dangerSettings.mode === 'AUTO_ROUTE'` | `failoverAllowed && await readCurrentConciergeOnDuty(userId, onDuty)` |
| 30 | `lib/services/dangerous-content/image-failover.ts:265` | `failoverAllowed \|\| onDuty` + on-duty re-read | `ctx.settings.mode !== 'AUTO_ROUTE'` → announce not-permitted | off duty → NO announcement |
| 31 | `…/provider-routing.service.ts:78,158` | `routeDirect \|\| failoverAllowed`; `desk.text/imageProfileId` | `settings.mode !== 'AUTO_ROUTE'` | as named |
| 32 | `…/refusal-ledger.ts:193-195` | `autoSwitchAfterRefusals` | `settings.autoSwitchAfterRefusals ?? 2`; `mode !== 'AUTO_ROUTE'` | `conciergePolicy.autoSwitchAfterRefusals` |
| 33 | `…/gatekeeper.service.ts:274,385,442` | `preScreen.threshold`, `preScreen.customClassificationPrompt` | `settings.threshold`, `settings.customClassificationPrompt` | as named |
| 34 | `…/understudy.ts:108,187` | `desk.textProfileId`, `desk.imageProfileId` | `settings.uncensoredTextProfileId/…ImageProfileId` | as named |
| 35 | `lib/tools/almanack/phase2-machinery.ts:359` | `imagePromptProfileId` | `chatSettings?.cheapLLMSettings?.imagePromptProfileId` | `readConciergeSettings(chatSettings).imagePromptProfileId` |
| 36 | `…/phase3-ledgers.ts:665,704,764` | whole object → `concierge` block; `uncensoredVisionProfileId` | `chatSettings?.dangerousContentSettings` fields; `uncensoredImageDescriptionProfileId` | `readConciergeSettings(chatSettings)` |
| 37 | `lib/tools/handlers/help-settings-handler.ts:124,217` | whole object; `enabled` | `settings.dangerousContentSettings` (in `chat`) | `readConciergeSettings(settings)` (`concierge` category); `conciergeOnDuty` in overview |
| 38 | `lib/tools/handlers/image-generation-handler.ts:1131,1143,756,798,882,1062,1074,1090,2277` | `routeDirect` (NEW 5a), `preScreen.enabled && scanImagePrompts/scanImageGeneration`, `failoverAllowed`, `desk.imageProfileId`, `desk.imagePromptProfileId` | `mode !== 'OFF' && scan…`; `mode === 'AUTO_ROUTE'`; `Boolean(uncensoredImageProfileId)`; `cheapLLMSettings?.imagePromptProfileId` | as named |
| 39 | `lib/services/chat-message/{answer-confirmation,pre-compute,message-finalizer,primary-stream,types}.ts`, `lib/chat/context/compression.ts`, `lib/memory/cheap-llm-tasks/types.ts` | type/field rename only | `dangerSettings: DangerousContentSettings` | `conciergePolicy: ResolvedConciergePolicy` |
