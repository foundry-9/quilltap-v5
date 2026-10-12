# Survey — bug 189 (item pictures under the chat's Concierge), the shared image attempt + generated-file row, the physical-description pick, bug 196 (Almanack image spend), bug 197's vision request (2026-10-12)

Read-only planning survey for the wardrobe catch-up round's **images / prompts /
Almanack** lane. v4 read at the PIN `4a38222e6` (`git show 4a38222e6:<path>`;
per-commit hunks via `git show <sha> -- <path>`; the span via `git diff 3e6e2f590
4a38222e6`); **every v4 line number below is AT THE PIN.** v5 read on `main`
(`f84d34150`). Scope: the server surfaces of `7a8d6dacc`'s bug 189 and
`9ecebf986`'s phases (d) PROMPTS and (e) IMAGES as the ledger's row maps them
(`drift-ledger.md:461`), plus bug 196. **Out of scope here** (other lanes): the
wearable pool / `WARDROBE_TOOL_TABLE` / route factory / `wear-ops` (phases a–c),
the backup carriers, `wardrobe/preview-avatar`'s rewrite and the `chats` POST /
`toggle-avatar-generation` callers (ledger (b) ROUTES), every SPA component, and
`help/` + `docs/v4/` (the docs-vendor lane). Where this lane's files are ALSO
touched by another lane's phase, §M names the meeting point.

**Confirmed from the stats, not the prose:** of the span's seven commits
(`7a8d6dacc`, `9ecebf986`, `e23fb095b`, `0841000c7`, `c19e5d0a9`, `0fc8ce709`,
`4a38222e6`), ONLY `7a8d6dacc` and `9ecebf986` touch this lane's paths.
`git show --stat e23fb095b -- <scope paths>` is EMPTY; `4a38222e6`'s only
hits under `lib/tools` are `wardrobe-list-handler.ts` / `wardrobe-read-handler.ts`
/ `wardrobe-list-tool.ts` / `wardrobe-read-tool.ts` (the folders lane's `folder`
key) — nothing of this lane's. The span stat for the scope paths: 41 files,
+2,466 / −1,709 (`--stat` reproduced in §0).

**Conventions.** v4 paths are relative to `~/source/quilltap-server`; v5 paths
to `crates/quilltap-core/src` unless prefixed `quilltap-harness/`,
`quilltap-host/`, `harness/`. "v4 L" = line at the pin.

---

## 0. Commit shapes (from `--stat`)

- `7a8d6dacc` on this lane's paths: 3 files, +83 / −9 —
  `lib/wardrobe/item-image-generation.ts` (+86 / −7, bug 189 whole),
  `lib/background-jobs/handlers/wardrobe-item-image.ts` (+4 / −1: ONE new
  argument line + a header sentence), `lib/background-jobs/queue-service.ts`
  (+1 / −1: the `WardrobeItemImageGenerationPayload.chatId` doc comment only —
  the field already existed). `lib/wardrobe/tool-image-generation.ts` is NOT in
  the span at all: the tools already enqueue `{ chatId, characterId, itemId }`
  (v4 L100-104) and v5 already carries it (`services/tool_image_generation.rs:268-272`).
- `9ecebf986` on this lane's paths: 26 files, +1,982 / −1,152. NEW:
  `lib/image-gen/image-attempt.ts` (208), `lib/files/generated-file-row.ts`
  (128), `lib/characters/physical-description.ts` (82), `lib/llm/vision-request.ts`
  (382), and the three MOVED files `lib/image-gen/{avatar-prompt,avatar-cache}.ts`
  + `lib/image-gen/__tests__/avatar-{cache,prompt-aesthetic,prompt-gender}.test.ts`
  (`-M50%`: `avatar-cache.ts` similarity 99% — the `@module` line only; the three
  tests 100%; `avatar-prompt.ts` 76%, §4). REWRITTEN call sites: `character-avatar.ts`
  (−113 net), `story-background.ts` (−108), `image-generation-handler.ts` (−90),
  `item-image-generation.ts` (−39), `file-attachment-fallback.ts` (−313),
  `image-analysis.ts` (−180). Smalls: `appearance-resolution.ts` (+8 / −27),
  `system-prompt-builder.ts` (+2 / −2), `character-headshoulders-backfill.ts`
  (+2 / −8), `scene-state-tracking.ts` (+2 / −1), `wardrobe-announcement.ts`
  (pool — not this lane), `pronoun-gender.ts` (a comment), `profile-resolution.ts`
  (+71: NEW `resolveAvatarImageProfile`, §9), `llm-logs.repository.ts` (+9 / −3),
  `llm-log.types.ts` (+12), `phase6-wire-records.ts` (+3 / −1),
  `tool-definitions-snapshot.test.ts.snap` (−4: `include_presets`, the tools
  lane's). Also in the span but NOT in the ledger's (d)/(e) paragraphs:
  `lib/memory/cheap-llm-tasks/image-scene-tasks.ts` (+2 / −1),
  `lib/services/external-prompt-generator.service.ts` (+2 / −1),
  `lib/startup/enqueue-headshoulders-backfill.ts` (+2 / −3), `lib/wardrobe/
  item-image-prompt.ts` (+5 / −11) — all four are `pickPhysicalDescription` /
  `soloFigureIntro` / `withArtDirection` callers (§5, §4).
- **Unchanged in the span (verified `--stat` EMPTY):** every file under
  `lib/services/dangerous-content/` (the failover chokepoint, the resolver, the
  provider routing — bug 189 is pure WIRING of what exists), `lib/files/
  llm-image-budget.ts`, `lib/llm/image-transport.ts`, `lib/llm/
  connection-profile-utils.ts`, `lib/image-gen/params-builder.ts`,
  `lib/files/webp-conversion.ts`, `lib/image-gen/aesthetic.ts`.

---

## 1. Bug 189 — `generateWardrobeItemImage(…, chatId?)` (`lib/wardrobe/item-image-generation.ts`)

### v4 at the pin
- Header L1-24 (rewritten by both commits): step 3 now reads "A tool-queued
  picture carries the chat it was asked for in, and that chat's Concierge state
  governs it (bug 189): Locked never fails over, Unmoderated routes direct to the
  uncensored desk, and a refusal is ledgered and announced there." Step 4
  `decodeProviderImage → addWardrobeItemImage`. The attempt closure is
  `makeLoggedImageAttempt`'s; "its log rows are `WARDROBE_ITEM_IMAGE`, which the
  Almanack counts as image spend (bug 196)."
- Imports L26-54: `+ decodeProviderImage, makeLoggedImageAttempt` (`image-attempt`),
  `+ resolveImageProviderForDangerousContent`, `+ componentGraph, loadWearablePool`
  (`lib/wardrobe/pool` — the POOL lane's module), `+ ChatMetadata`; `−
  createImageProvider`, `− logLLMCall`, `− buildImageGenParams`, `− convertToWebP`,
  `− hydrateComponentGraph`, `− sharedWardrobeTiersForCharacter`.
- `args` L200-214: `chatId?: string | null` — "The chat a tool-queued picture was
  asked for in. Its Concierge state governs the call; null (the editor) reads as
  Moderated with no ledger."
- NEW `loadChat(repos, chatId)` L169-183: `repos.chats.findById(chatId)` in a
  try/catch; the catch logs **WARN `[WardrobeItemImage] Chat unreadable; the
  failover will re-read its Concierge state`** `{ context: 'wardrobe.item-image-generation',
  chatId, error }` and returns null. ⚠ **Reachability:** `ChatsRepository.findById`
  is `this._findById(id)` (`chats.repository.ts:144`), which is a `safeQuery`
  (`base.repository.ts:247-258`) returning `null` + v4's fallback ERROR line
  `Error finding entity by ID` — so the WARN is a DEAD ARM in production v4 (the
  catch never fires). Port as v5's `db::fallback::find_by_id_or_none("chats", …)`
  idiom (14 existing sites) with the WARN as the fallback's HOME line, exactly
  the P4.149/P4.163 "RETHROW lines as homes" class.
- L252-263: `const chatId = args.chatId ?? null; const chat = chatId ? await
  loadChat(repos, chatId) : null; const chatSettings = await
  repos.chatSettings.findByUserId(userId); const conciergePolicy =
  resolveConciergeSettings(chatSettings ?? null, chat);` then **DEBUG
  `[WardrobeItemImage] Concierge policy resolved`** `{ context, itemId, chatId,
  chatFound: !!chat, conciergeState: conciergePolicy.state, routeDirect:
  conciergePolicy.routeDirect }`. (Pre-span: `resolveConciergeSettings(chatSettings
  ?? null, null)` and NO line.)
- L265-287 the direct route: `let primaryProfile = profile; let primaryKey =
  apiKey.key_value; if (conciergePolicy.routeDirect) { const routeResult = await
  resolveImageProviderForDangerousContent(profile, apiKey.key_value,
  conciergePolicy, userId); if (routeResult.rerouted) { primaryProfile =
  routeResult.imageProfile; primaryKey = routeResult.apiKey; }` then **INFO
  `[WardrobeItemImage] Unmoderated chat: routed direct to the uncensored desk`**
  `{ context, itemId, chatId, rerouted: routeResult.rerouted, profile:
  primaryProfile.name }` — logged whether or not it rerouted (the avatar job's
  shape, `character-avatar.ts`; v5 already has that shape at
  `services/character_avatar_job.rs:451-485`). `resolveImageProviderForDangerousContent`
  itself (unchanged, `provider-routing.service.ts:153-230`) logs its own
  `[DangerousContent] Rerouting to uncensored image profile` / `No uncensored
  image provider available, sending to original` / `Image provider routing
  failed, using original` lines — already ported.
- L289-303 the attempt: `makeLoggedImageAttempt({ userId, logType:
  'WARDROBE_ITEM_IMAGE', prompt, primaryProfileId: profile.id, chatId,
  characterId: home.characterId ?? null, params: { overrides: { n: 1, style:
  'natural' }, orientation, logContext: { context: LOG_CONTEXT, itemId:
  home.item.id } } })`. Note `primaryProfileId` is the RESOLVED profile (not the
  direct-routed `primaryProfile`), so a direct-routed Unmoderated call logs its
  first row as `(Concierge reroute)` — same as the avatar job's
  `effectiveImageProfile` semantics. **New in the row: `chatId`** (`logLLMCall({
  chatId: options.chatId ?? undefined, … })`) — v5 writes `chat_id: None`
  (`wardrobe_item_image_generation.rs:387`). The pre-span closure's `logContext`
  carried `profileId: attemptProfile.id`; `makeLoggedImageAttempt` now adds
  `profileId` + `rerouted` itself (§2).
- L305-318 the failover call: `generateImageWithConciergeFailover({ profile:
  primaryProfile, apiKey: primaryKey }, attempt, { userId, chatId, chat, purpose:
  'wardrobe', conciergePolicy, primaryVia: primaryProfile.id !== profile.id ?
  'concierge' : 'primary' })`. Pre-span: `{ userId, chatId: null, purpose:
  'wardrobe', conciergePolicy }`. `ImageFailoverContext` (unchanged,
  `image-failover.ts:65-99`) already has `chat?`, `primaryVia?`,
  `announceUnresolvedRefusal?` — v5 `ImageFailoverContext` has them all
  (`services/dangerous_content/image_failover.rs:169-199`). The chokepoint's own
  behaviour with a chat (unchanged, L254-320): the Locked re-read
  (`readCurrentConciergeState(ctx.chatId, getConciergeState(ctx.chat))`), INFO
  `Refusal not rerouted: the chat is Locked`, the `refusal-not-permitted`
  announcement with `reason: 'locked'`, the ledger row (`recordModerationRefusal`
  — `kind: 'image', purpose: 'wardrobe'`), the understudy try, the
  `refusal-rerouted` / `refusal-no-understudy` announcements — all gated on
  `ctx.chatId` being non-null. **With bug 189 a tool-queued picture now
  WRITES to `moderation_refusals` and posts Concierge announcement rows into
  the chat on a refusal** — new tier-2 state the item-image family must dump.
- L320-330 catch: WARN `[WardrobeItemImage] Wardrobe item image generation
  failed` `{ context, itemId, error, refused, conciergeTrail: trail?.map(({
  profileName, outcome, detail })) }` — unchanged in words (v5
  `wardrobe_item_image_generation.rs:464-470` carries it as `conciergeTrailJson`).
- L332-335: `const decoded = await decodeProviderImage(failover.result,
  'wardrobe'); if (!decoded) throw new WardrobeImageGenerationError('The image
  provider returned no picture', failover.trail.length ? failover.trail : null,
  false)`. Sentence unchanged; the decode now logs `[ImageAttempt]` DEBUG lines
  (§2) and names the file `wardrobe_<Date.now()>.<ext>` (was `wardrobe.<ext>`) —
  the bridge's `written.leafName`, not this filename, becomes
  `originalFilename` (`item-images.ts:216-221`), so the filename is NOT
  row-visible for wardrobe pictures.
- L337-347: `addWardrobeItemImage(repos, home, { userId, kind: 'generated',
  content: decoded.buffer, contentType: decoded.mimeType, width: decoded.width,
  height: decoded.height, generationPrompt: prompt, generationModel:
  failover.profile.modelName, generationRevisedPrompt: decoded.revisedPrompt })`
  — `width`/`height` were `converted.width ?? null`; `decodeProviderImage`
  already nulls them.
- `resolveComponentLeaves` L100-141: now `loadWearablePool(repos,
  home.characterId, [])` + `componentGraph(pool, home.item.componentItemIds)`
  (the POOL lane's API — §M1) and a NEW DEBUG `[WardrobeItemImage] Outfit
  expansion stopped short` `{ context, itemId, cycles, truncated }` when
  `expandComposites` reports cycles or truncation. The `[hydrateComponentGraph]`
  lines the v5 harness prefixes on (`LOG_PREFIXES` in the tier-3 test) VANISH
  with the pool.
- Unchanged: `NO_WARDROBE_IMAGE_PROFILE_MESSAGE` L59-60, both error classes
  L63-83, the result shape L85-94, `resolveOwner` L147-162 (WARN `Owner
  unreadable; falling back to a catalogue shot`), `resolveProjectAestheticMount`
  L186-192, the tombstone refusal L219, the profile/key resolution L221-225,
  the `Generating wardrobe item image` DEBUG L240-250, the success INFO L349-358,
  the return L360-369.

### The job handler (`lib/background-jobs/handlers/wardrobe-item-image.ts`)
- L74-80: `generateWardrobeItemImage(repos, { userId: job.userId, home,
  containerId: payload.characterId, chatId: payload.chatId ?? null })` — the
  ONE functional line of the commit here. Every log line unchanged (L40-46
  `Starting tool-queued wardrobe item image` with `chatId: payload.chatId`,
  L57 `Item no longer in the character's wardrobe; nothing to draw`, L66 `Item
  archived since the tool call; skipping`, L82 `Tool-queued wardrobe item image
  complete`, L93 `No usable image profile; skipping`, L101 `Owner archived
  since the tool call; skipping`, L109 `Provider would not draw the item`).

### v5 main
- `services/wardrobe_item_image_generation.rs` (630 lines):
  `GenerateWardrobeItemImageArgs` L136-144 has `user_id / home / container_id /
  image_profile_id` — NO `chat_id`. L337-340 `resolve_concierge_settings(
  chat_settings.as_ref(), None)`; the attempt closure L346-411 is a HAND-ROLLED
  copy (params builder + `log_llm_call` with `chat_id: None` at L387 + `success_content`
  L592-607); L426-440 `ImageFailoverContext { chat_id: None, chat: None,
  purpose: ImagePurpose::Wardrobe, primary_via: RouteAttemptVia::Primary, … }`;
  NO direct route; L482-510 the decode is inline (`convert_to_webp(…,
  "wardrobe.<subtype>")`). `resolve_component_leaves` L156-224 is on the
  pre-pool shape (`wardrobe_tiers::shared_wardrobe_tiers_for_character` +
  `tools::wardrobe_shared::hydrate_component_graph`) — the POOL lane rewrites
  this body; this lane adds the `Outfit expansion stopped short` line (§M1).
- `services/wardrobe_item_image_job.rs` (231 lines): L66 reads `chatId` from
  the payload for the starting line only; L130-135 builds the args WITHOUT it;
  the module header L20 records the deferral in so many words ("the payload's
  `chatId` is never forwarded"). The payload struct carries it
  (`services/tool_image_generation.rs:268-272`, `queue_service.rs`).
- The machinery bug 189 wires is ALL present from the avatar job:
  `resolve_concierge_settings(global, chat: Option<&Value>)`
  (`dangerous_content/resolver.rs:327-330`),
  `resolve_image_provider_for_dangerous_content(conn, api_keys, &RouteProfile,
  key, &policy, user_id)` (`provider_routing.rs:274-281`), the `route_direct`
  arm + its INFO line at `character_avatar_job.rs:451-485`, `primary_via`
  computed at `:642`, `ImageFailoverContext.chat` at `image_failover.rs:185`.
- Harness: `quilltap-harness/tests/wardrobe_item_image_generation_tier3_equivalence.rs`
  (491 lines; oracle `harness/oracle/cases/wardrobe-item-image-generation.test.ts`
  + fixture `harness/oracle/fixtures/wardrobe-item-image-generation-tier3.json`,
  builder `build-wardrobe-item-image-generation-fixture.ts`): 23 scenarios
  (`worn_designated`, `catalogue_general_default`, `hair_worn`,
  `outfit_worn_with_general_component`, `project_catalogue`, `override_profile`,
  `override_unknown_falls_to_designated`, `designated_unknown_falls_to_default`,
  `no_profile`, `key_row_gone`, `refused_off_duty`, `refused_rerouted`,
  `refused_no_understudy`, `provider_failure`, `no_picture`,
  `archived_owner_before_spend`, and the seven `job_*` ops) — compares the
  result JSON, the DB dump (files + the item's `imageFileId` + `llm_logs`: "ONE
  `WARDROBE_ITEM_IMAGE` row per provider" call), and the captured lines with
  prefixes `[WardrobeItemImage]` + `[hydrateComponentGraph]`. The `job_*` ops
  pass `chatId: spec.jobChatId` = `c0c0c0c0-0001-4000-8000-000000000001`; the
  builder plants users / api keys / image profiles / chat_settings / two
  characters / mount points / a project / items — **NO `chats` row** (grep of
  the builder's `.create(`/`insertOne` sites: none for chats), so under the pin
  the job ops resolve `chatFound: false` → Moderated. **Predicted RED on the
  pin, every scenario:** the new `Concierge policy resolved` DEBUG line (missing
  on v5), and on the seven `job_*` rows the `llm_logs.chatId` cell
  (`c0c0c0c0-…` vs NULL). The `refused_*` rows additionally lose nothing
  (editor ops, `chatId: null`). To exercise bug 189 PROPERLY the port must grow
  the fixture: a planted Moderated chat (ledger + `refusal-rerouted` announcement
  rows on `job_refused`), a Locked chat (never fails over; `refusal-not-permitted`
  with `reason: 'locked'`), an Unmoderated chat (the direct route + its INFO
  line + `primaryVia: 'concierge'` in the trail), and an unreadable-chat row for
  the fallback home. The `[hydrateComponentGraph]` prefix must leave
  `LOG_PREFIXES` with the pool (§M1).

---

## 2. The shared attempt + decode — NEW `lib/image-gen/image-attempt.ts`

### v4 at the pin (whole file, 208 lines)
- `ImageAttemptLogType = ImageSpendLogType` (L42; §7). `LoggedImageAttemptOptions`
  L47-69: `userId`, `logType`, `prompt`, `primaryProfileId`, `chatId?`,
  `characterId?`, `params?: Omit<BuildImageGenParamsOptions, 'profile' |
  'prompt'>`, `prebuilt?: { profileId, params }` ("the avatar job derives its
  cache key from them — reused verbatim when that profile is the one asked").
- `makeLoggedImageAttempt` L75-141. `record` L78-104: `logLLMCall({ userId, type:
  logType, chatId: options.chatId ?? undefined, characterId: options.characterId
  ?? undefined, provider, modelName, imageProfileId: profile.id, request: {
  messages: [{ role: 'user', content: prompt }] }, response, durationMs })`,
  catch → **WARN `[ImageAttempt] Failed to record the image call in llm_logs`**
  `{ context: 'image-gen.image-attempt', logType, profileId, error }`. The
  closure L106-140: `rerouted = profile.id !== primaryProfileId`; params =
  prebuilt when `prebuilt.profileId === profile.id` else
  `buildImageGenParams({ ...options.params, profile, prompt, logContext: {
  ...options.params?.logContext, profileId: profile.id, rerouted } }).params`;
  **DEBUG `[ImageAttempt] Calling image provider`** `{ context, logType,
  profileId, provider, model, rerouted, reusedPrebuiltParams }`; on success
  `record(…, { content: response.images?.[0]?.revisedPrompt || \`Generated
  ${response.images?.length ?? 0} image(s)${rerouted ? ' (Concierge reroute)' :
  ''}\` })` — **the `llm_logs` content is byte-unchanged at every call site**;
  on throw `record(…, { content: '', error })` then rethrow. ⚠ The record is
  AWAITED on both arms; the `generate_image` tool's pre-span closure was
  fire-and-forget (`logLLMCall(...).catch(...)`, WARN `[Image Generation]
  Failed to log image generation to LLM Inspector`) — that WARN is GONE and the
  llm_logs row now lands BEFORE the files row (ordering, §3c).
- `DecodedProviderImage` L144-155 `{ buffer, mimeType, filename, width, height,
  sha256, revisedPrompt }`; `decodeProviderImage(response, filenameStem, index =
  0)` L162-208: `raw = image?.data || image?.b64Json`; none → **DEBUG
  `[ImageAttempt] Provider response carried no image data`** `{ context, index,
  imageCount, hasUrl }` → null; else `convertToWebP(Buffer.from(raw, 'base64'),
  providerMimeType, \`${filenameStem}_${Date.now()}.${providerExt}\`)`,
  `sha256OfBuffer(converted.buffer)`, `revisedPrompt: image.revisedPrompt ||
  null`, **DEBUG `[ImageAttempt] Decoded provider image`** `{ context, index,
  providerMimeType, storedMimeType, bytes, width, height }`.
- The log-context delta at every site: the pre-span `.concierge-route` /
  `.concierge-reroute` CONTEXT STRINGS are gone; one context + `profileId` +
  `rerouted` keys in `buildImageGenParams`'s own log line.

### v5 main
- No `image_attempt` module. The closure exists FOUR times: `services/
  image_job_common.rs:339-400` (`generate_job_image` — the avatar + Lantern
  jobs' shared builder, with `requested_log_context` / `other_log_context`
  TWO-string args L351-352 and `job_success_content` + the `(Concierge reroute)`
  suffix at L388), `tools/generate_image.rs:2651-2707` (`tools.generate_image`
  vs `tools.generate_image.concierge-reroute` at L2671-2673, `log_image_generation`
  L2557 — awaited, so v5 is ALREADY on the awaited ordering),
  `services/wardrobe_item_image_generation.rs:346-411`, and the host's
  preview renderer (`quilltap-host/src/avatar_preview.rs`, 344 lines — the
  ROUTES lane's, §M4). The decode tail is inline at each: avatar
  `character_avatar_job.rs:715-745` (TWO WARNs `[CharacterAvatar] No images
  returned from provider` / `Generated image has no data`, pinned by the
  in-crate tests at `:1776-1849`), Lantern `story_background_job.rs:991-1012`
  (returns `Ok(())` SILENTLY on both arms — v5 never carried v4's two
  `[StoryBackground]` WARNs; the port lands v4's ONE new line), tool
  `generate_image.rs:2823-2843` (`generated_<now_ms>.<ext>`; an empty `data`
  decodes to an empty buffer — v4's new `STORAGE_ERROR` sentence `The image
  provider returned an image with no data` has NO v5 arm; v5's STORAGE_ERROR
  arms at `:2875-2896` are the save failures), wardrobe `:482-510`.
- `source_census`-class guard `quilltap-harness/tests/lora_log_anchor_guard.rs:62-86`
  asserts v5 STILL CONTAINS the literals `tools.generate_image.concierge-reroute`,
  `background-jobs.character-avatar.concierge-route`,
  `background-jobs.story-background.concierge-reroute` — **it reddens the
  moment the port retitles them** and must be rewritten to the post-pin anchors
  (one context per site + `profileId`/`rerouted` keys); `params_builder.rs`'s
  doc comment ("nine literals across the five consolidated sites") follows.

---

## 3. The four call sites

### 3a. `lib/background-jobs/handlers/character-avatar.ts`
- L334-347: `makeLoggedImageAttempt({ userId: job.userId, logType:
  'IMAGE_GENERATION', prompt, primaryProfileId: effectiveImageProfile.id,
  chatId: payload.chatId, characterId: payload.characterId, params: {
  overrides: { n: 1, style: 'natural' }, orientation: 'portrait', logContext: {
  context: 'background-jobs.character-avatar', jobId: job.id } }, prebuilt: {
  profileId: imageProfile.id, params: avatarParams } })`. Semantics kept:
  `rerouted` keys on the EFFECTIVE (direct-routed) profile, the prebuilt reuse
  on the REQUESTED one. Pre-span rebuild context
  `'background-jobs.character-avatar.concierge-route'` → gone.
- L392-402: `decodeProviderImage(generationResponse, \`avatar_${character.name.replace(/[^a-zA-Z0-9]/g, '_')}\`)`;
  null → **WARN `[CharacterAvatar] No image data returned from provider`** `{
  context: 'background-jobs.character-avatar', jobId, imageCount:
  generationResponse.images?.length ?? 0 }` → return. Replaces the TWO WARNs
  `No images returned from provider` / `Generated image has no data`.
  Filename `avatar_<name>_<ms>.<ext>` unchanged.
- L442-463: `createGeneratedFileRow(repos, { id: fileId, userId, sha256:
  decoded.sha256, originalFilename: decoded.filename, stored: written, width,
  height, linkedTo: [payload.chatId, payload.characterId], tags:
  [payload.characterId], generation: { prompt, model:
  effectiveImageProfile.modelName, revisedPrompt: decoded.revisedPrompt },
  generationKey: cacheKeys.key })` — `description: null`, `projectId: null`,
  `folderPath: null`, `source: 'GENERATED'`, `category: 'IMAGE'` now by
  default; the ROW IS BYTE-IDENTICAL. New DEBUG `[GeneratedFileRow] Created
  image file row` (§6).
- v5: `services/character_avatar_job.rs` (2,141 lines) — `generate_job_image`
  at ~`:632-680` with the two contexts; the decode `:715-745` with the two
  WARNs; the files row written through the avatar write path. Harness
  `avatar_job_tier3_equivalence.rs` (1,292 lines; `QT_ORACLE_AVATAR`) dumps
  `files` (`size` blanked) + `llm_logs` + a `concierge_footprint` — rows
  unchanged ⇒ **GREEN on the pin**; the in-crate tests at
  `character_avatar_job.rs:1776-1849` (`one(&lines, "[CharacterAvatar] No
  images returned from provider")`, `… Generated image has no data`) go RED
  when the WARNs are collapsed — rewrite them to the one line + `imageCount`.
  `__tests__/unit/lib/background-jobs/handlers/character-avatar-sha256.test.ts`
  (6 cases, incl. `sends an Unmoderated chat straight to the uncensored desk
  before the call`) changed only its import/mock path.

### 3b. `lib/background-jobs/handlers/story-background.ts`
- L707-718: `makeLoggedImageAttempt({ userId, logType: 'IMAGE_GENERATION',
  prompt: finalPrompt!, primaryProfileId: imageProfile.id, chatId:
  payload.chatId, params: { overrides: { n: 1, style: 'natural' }, orientation:
  'landscape', logContext: { context: 'background-jobs.story-background', jobId,
  chatId } } })` — no `characterId`, no prebuilt; `primaryProfileId` is the
  REQUESTED profile (`imageProfile.id`), so a direct-routed Unmoderated call's
  first row says `(Concierge reroute)` — v5 `story_background_job.rs:864-890`
  already keys the suffix on `image_profile_id` the same way (its comment says
  so).
- L790-797: `decodeProviderImage(generationResponse, 'story_background')`; null
  → **WARN `[StoryBackground] No image data returned from provider`** `{
  context: 'background-jobs.story-background', jobId, imageCount }` → return
  (replaces `No images returned from provider` / `Generated image has no data`).
- L867-885: `createGeneratedFileRow(repos, { id: fileId, userId, sha256,
  originalFilename, stored: { storageKey, storedMimeType, sizeBytes: storedSize
  }, width, height, linkedTo, generation: { prompt: finalPrompt, model:
  activeImageProfile.modelName, revisedPrompt }, projectId: folderProjectId,
  folderPath: fileFolderPath })` — `tags: []` by default; row byte-identical.
- **The physical-description pick (§5) changes TWO prompt sites here:**
  `buildBasicEnumeration` L63-68 was `medium || short || long || fullDescription
  || name` (NO `completePrompt`) → `pickPhysicalDescription(primary, 'scene') ||
  char.name` = `medium ‖ short ‖ long ‖ complete ‖ full ‖ name` — a character
  with ONLY `completePrompt` was enumerated by NAME, now by the prompt; and the
  per-character fallback L491-494 was `medium || short || name` → the `scene`
  order (long / complete / full now used before the name). Both are
  MODEL-VISIBLE in the crafted background prompt and the scene-derivation input.
- v5: `story_background_job.rs:1505-1525` `build_basic_enumeration` carries the
  pre-span four-key order (its doc comment quotes it); `:603-614` the fallback
  carries `medium ‖ short ‖ name`. Harness `story_background_job_tier3_equivalence.rs`
  (1,189 lines; `QT_ORACLE_STORY`) dumps `files` + `llm_logs` + compares the
  derive-scene / craft prompts → **RED on the pin IF the corpus has a cast
  member whose only variants are `completePrompt` or `long`/`full`-only**
  (measure: grep the `story-background-job` fixture's `physicalDescription`
  cells); the missing WARN is only visible if the capture compares it (2
  capture mentions — check).

### 3c. The `generate_image` tool — `lib/tools/handlers/image-generation-handler.ts`
- L348-359: `makeLoggedImageAttempt({ userId, logType: 'IMAGE_GENERATION',
  prompt: toolInput.prompt, primaryProfileId: imageProfile.id, chatId: chatId
  ?? null, params: { overrides: toolInputOverrides(toolInput), orientation:
  requestedOrientation(toolInput), logContext: { context:
  'tools.generate_image', chatId } } })`. The pre-span closure's
  `tools.generate_image.concierge-reroute` context and its fire-and-forget log
  (+ WARN `[Image Generation] Failed to log image generation to LLM Inspector`)
  are gone; the row content unchanged.
- L397-409: per image `decodeProviderImage(generationResponse, 'generated',
  index)`; null → `throw new ImageGenerationError('STORAGE_ERROR', 'The image
  provider returned an image with no data')` (NEW sentence; pre-span an empty
  `data` fell into `convertToWebP(Buffer.from(''))` and surfaced as whatever
  that threw); `saveGeneratedImage(decoded, …, { prompt, revisedPrompt:
  decoded.revisedPrompt ?? undefined, model, provider, routeTrail })`.
- `saveGeneratedImage` L125-200: takes `DecodedProviceImage`; L172-187
  `createGeneratedFileRow(repos, { id: fileId, userId, sha256, originalFilename,
  stored: written, width, height, linkedTo, tags: inheritedTags, generation: {
  prompt, model, revisedPrompt } })` — `description: null` kept; filename
  `generated_<ms>.<ext>` unchanged.
- v5: `tools/generate_image.rs` (3,454 lines): the closure `:2651-2707` (log
  awaited — already v4's post-pin ORDER), the save `:2823-2862`
  (`save_generated_image` into the writer; `description: None` at `:1318/:1354`).
  Harness `image_generation_tier3_equivalence.rs` (1,338 lines) dumps `files` +
  `llm_logs` + `background_jobs`, NO capture ⇒ **GREEN on the pin** unless its
  corpus carries an image with empty `data` (then the new STORAGE_ERROR text).
  `tool_image_generation_equivalence.rs` is the WARDROBE tools' picture
  DECISION (tier 1, `tool-image-generation.ts`, unchanged) ⇒ GREEN.

### 3d. Wardrobe item pictures — §1; and `lib/wardrobe/item-images.ts:216-230`
`addWardrobeItemImage` now mints the row through `createGeneratedFileRow(repos,
{ userId, sha256: written.sha256, originalFilename: written.leafName, stored:
written, width, height, linkedTo: [home.item.id], tags: [home.item.id], source:
SOURCE_BY_KIND[input.kind], generation: {...} })` — `description` DROPPED from
the row ("The mount link carries the caption; the `files` row carries no label
(bug 132)"). ⚠ **Row-visible:** pre-span the wardrobe picture's `files.description`
carried the caption (`description` was built at `:211-213` and written); now
NULL. The `wardrobe_item_images_tier2_equivalence` family (the IMAGES lane's
P4.D263 file `services/wardrobe_item_images/`) reddens on `files.description`
for every generated/uploaded picture — §M3 names the owner. The same hunk adds
`linkWardrobeItemImage` (PORT-NEW `?action=link-image`, the ROUTES lane's).

---

## 4. The avatar prompt family MOVES — `lib/wardrobe/{avatar-prompt,avatar-cache}.ts → lib/image-gen/`

- `git diff -M50% 9ecebf986^ 9ecebf986`: `avatar-cache.ts` similarity 99% (the
  `@module` line); the three tests 100%; `avatar-prompt.ts` 76%. **PROMPT TEXT
  BYTE-IDENTICAL** — verified on the hunk: the intro `Solo portrait of a single
  ${subjectNoun}: ${name}. Show exactly one figure` + `, head-and-shoulders
  crop, three-quarter view.` / `. Close-up headshot cropped at the collarbone —
  only the face, neck, and bare shoulders are visible; the chest and torso are
  outside the frame.` is now `soloFigureIntro('portrait', …)` (L104-106:
  `` `Solo ${medium} of a single ${subjectNoun}: ${name}. Show exactly one figure` ``,
  "returned without closing punctuation") + the same tails (L221-224); the
  aesthetic preamble `Art direction (apply this overall style): ${capped}\n\n${prompt}`
  with the 600-char cap is `withArtDirection(prompt, aesthetic)` (L119-126;
  `FIGURE_AESTHETIC_MAX_CHARS = 600` L112; the private
  `AVATAR_AESTHETIC_MAX_CHARS` gone); `prompt = withArtDirection(prompt,
  options.characterAesthetic)` L235.
- `buildFigureIdentityBlock` L78-96: the pick is now `pickPhysicalDescription(
  character.physicalDescription, framing)` (L83). Pre-span: `order.find((text)
  => !!text) || ''` then ONE `.trim()` — the first TRUTHY variant, trimmed. Post:
  the first variant whose TRIM is non-empty (§5). Delta only for a
  whitespace-only variant ahead of a real one (pre: picked, trimmed to `''` →
  `hasAppearance` false; post: skipped). The two orders are unchanged
  (`head-and-shoulders` / `full-length`, §5).
- The pool: `resolveEquippedOutfitForCharacter(await loadWearablePool(repos,
  character.id, projectMountPointIds ?? []), equippedSlots)` L165-169 — the
  POOL lane's (§M1); the comment "No project context reads as no project tier —
  never a chat lookup."
- `lib/wardrobe/item-image-prompt.ts` L103-106 / L126-130 consume
  `soloFigureIntro('picture', …)` + `withArtDirection(…)`; its private
  `AESTHETIC_MAX_CHARS` is gone; text byte-identical (`Solo picture of a single
  X: name. Show exactly one figure. ${framing}.`).
- `pronoun-gender.ts` L10: comment path only.
- v5: `services/avatar_prompt.rs` (323 lines: `build_character_avatar_prompt`,
  `build_figure_identity_block` L247-300 with the two `const` orders and the
  "first TRUTHY then `js_trim`" pre-span semantics spelled out at L267-268),
  `services/avatar_cache.rs` (607), `services/wardrobe_item_image_prompt.rs`
  (238; its own `AESTHETIC_MAX_CHARS = 600` L38 + UTF-16 cap L224-230; the
  intro literal L180). Rust has no module move to make; the port is (i) the
  pick through the shared `pick_physical_description` (§5) and (ii) optionally
  factoring `solo_figure_intro` / `with_art_direction` so the two prompts share
  one home as v4's do (`avatar_prompt.rs` would export them;
  `wardrobe_item_image_prompt.rs` consumes). Harness: `avatar_cache_key_equivalence`
  + `wardrobe_item_image_prompt_equivalence` (the fixture's `shortPrompt: ""`
  cells are EMPTY, not whitespace) + the avatar-prompt rows inside
  `avatar_job_tier3` ⇒ **GREEN on the pin**. v4 tests moved: `lib/image-gen/
  __tests__/avatar-cache.test.ts` (19), `avatar-prompt-aesthetic.test.ts` (4),
  `avatar-prompt-gender.test.ts` (3); `__tests__/unit/lib/{wardrobe → image-gen}/
  avatar-prompt.test.ts` (11; mocks `@/lib/wardrobe/pool.loadWearablePool`
  now, `mockResolve.mockReturnValue` — the resolver went SYNCHRONOUS).

---

## 5. NEW `lib/characters/physical-description.ts` — `pickPhysicalDescription(desc, profile)`

### v4 at the pin (82 lines, pure, client-safe)
- `PhysicalDescriptionVariants = Partial<Pick<PhysicalDescription,
  'headAndShouldersPrompt' | 'shortPrompt' | 'mediumPrompt' | 'longPrompt' |
  'completePrompt' | 'fullDescription'>>` L20-23. Profiles L42-47.
- **The FIVE orders, verbatim (L51-57):**
  - `'head-and-shoulders'`: `headAndShouldersPrompt, mediumPrompt, shortPrompt, longPrompt, completePrompt, fullDescription`
  - `'full-length'`: `completePrompt, longPrompt, mediumPrompt, shortPrompt, fullDescription, headAndShouldersPrompt`
  - `scene`: `mediumPrompt, shortPrompt, longPrompt, completePrompt, fullDescription`
  - `'self-image'`: `shortPrompt, mediumPrompt, longPrompt, completePrompt, fullDescription`
  - `fullest`: `fullDescription, completePrompt, longPrompt, mediumPrompt, shortPrompt`
- `pickPhysicalDescription` L64-74: `if (!desc) return ''; for (key of
  ORDER[profile]) { const text = desc[key]?.trim(); if (text) return text; }
  return ''`. **Two semantics every caller inherits:** (i) the WINNER IS
  TRIMMED; (ii) a WHITESPACE-ONLY variant is SKIPPED (pre-span `||` chains took
  it as truthy). (iii) a non-string variant would throw on `.trim()` — v4's
  Zod shapes make every variant a string, so moot except for the external
  prompt generator's `to_js_string` tolerance (v5 `generators/external_prompt.rs:266-280`
  stringifies any JS-truthy value — the new v4 would `.trim()` a number and
  throw; corpus check).
- `hasPhysicalDescription(desc, profile)` L77-82 = `pick !== ''`.
- Test `__tests__/unit/lib/characters/physical-description.test.ts` (3: `falls
  through in order, skipping blanks and trimming`, `never uses the
  head-and-shoulders crop for a scene figure`, `answers empty for no description
  at all`).

### The TEN callers at the pin and what each changed
| v4 site | profile | pre-span chain | behaviour delta |
|---|---|---|---|
| `image-gen/avatar-prompt.ts:83` (§4) | `framing` (`head-and-shoulders` / `full-length`) | same orders, truthy-then-trim | whitespace-only skip |
| `image-gen/appearance-resolution.ts:172` `buildDefaultAppearances`, `:207` `mapResolutionResults`, `:256` `resolveCharacterAppearances` | `full-length` then `\|\| char.characterName` | `complete ‖ long ‖ medium ‖ short ‖ name` | **`fullDescription` → `headAndShouldersPrompt` now tried BEFORE the name** (model-visible in story-background / scene prompts when only those exist) + trim/whitespace |
| `background-jobs/handlers/story-background.ts:66` `buildBasicEnumeration` | `scene` + `\|\| name` | `medium ‖ short ‖ long ‖ fullDescription ‖ name` | **`completePrompt` joins (between long and full)**; trim |
| `story-background.ts:494` per-character fallback | `scene` + `\|\| name` | `medium ‖ short ‖ name` | **long / complete / full now used before the name** |
| `chat/context/system-prompt-builder.ts:248` `buildIdentityStack` | `self-image` | `short ‖ medium ‖ long ‖ complete ‖ full ‖ ''` | ORDER UNCHANGED (the ledger's claim holds); trim + whitespace-only skip — the `## Physical Appearance` paragraph's text is now TRIMMED (a trailing newline in `shortPrompt` changes the system prompt bytes) |
| `background-jobs/handlers/scene-state-tracking.ts:214` | `scene` | `medium ‖ short ‖ ''` | long / complete / full now used |
| `background-jobs/handlers/character-headshoulders-backfill.ts:67` seed | `scene` | same five, `(…\|\| '').trim()` once | whitespace-only no longer wins-then-empties |
| `startup/enqueue-headshoulders-backfill.ts:74` `hasSeed` | `hasPhysicalDescription(pd,'scene')` | `Boolean(a‖b‖c‖d‖e)` untrimmed | whitespace-only now `false` (skipped) |
| `memory/cheap-llm-tasks/image-scene-tasks.ts:916` `resolveAppearance` preview | `scene` + `\|\| '(no description text)'` | `medium ‖ short` | long / complete / full now shown in the picker text |
| `services/external-prompt-generator.service.ts:138` | `fullest` + `\|\| undefined` | `full ‖ complete ‖ long ‖ medium ‖ short ‖ undefined` | order unchanged; trim + whitespace skip |

### v5 main — every pick is an inline chain (ten homes, one per caller)
`services/avatar_prompt.rs:251-282` (two `const` arrays, truthy-then-`js_trim`);
`services/appearance_resolution.rs:125-135` `select_phys_desc` (doc: "v4's
`physDesc` selection: `complete || long || medium || short || name`" — the
struct `PhysicalDescription` at `:35-41` has NO `full_description` /
`head_and_shoulders_prompt` fields, so the port WIDENS the struct and its three
builders incl. `story_background_job.rs:1537-1548` `physical_description_from_value`);
`system_prompt.rs:355-375` (the `self-image` chain, `Some(s) if !s.is_empty()`,
UNTRIMMED) + `services/system_prompt_compiler.rs:77` (the same `PhysicalDescription`
value marshaled); `story_background_job.rs:603-614` + `:1505-1525`;
`services/headshoulders_backfill_job.rs:145-165` `seed_text` (the header
documents the pre-span "trim ONCE on the winner" asymmetry — that note FLIPS);
`services/headshoulders_backfill_enqueue.rs:150-170` (`any(!is_empty)`,
"NOT trimmed here (see the module header's asymmetry note)");
`services/image_scene_tasks.rs:366-372` (`medium ‖ short ‖ "(no description
text)"`); `generators/external_prompt.rs:266-280` `most_detailed_description`.
**No v5 counterpart for `scene-state-tracking.ts:214`:** v5 ports the cheap-LLM
TASK (`services/scene_state_tracking/mod.rs`, header L1-14: "its runner dispatch
row is W4.8"; the job wrapper is UNPORTED — `KNOWN_JOB_TYPES` at
`services/job_runner.rs:154` lists it, no handler registers it). Record the
order it would take (`scene`) for whoever ports the handler.

**Shape to port:** ONE `characters::physical_description::pick_physical_description(
desc: &Value | &PhysicalDescription, profile)` with the five `const` orders +
`has_physical_description`, replacing the ten chains; tier-1 exact against v4's
real function over a corpus that includes whitespace-only, trailing-newline,
`completePrompt`-only, `fullDescription`-only and `headAndShouldersPrompt`-only
characters for every profile (the bug-class the ten inline chains hid).

**Harness predictions on the pin:** `system_prompt_equivalence` (602 lines;
`QT_ORACLE_SYSTEM_PROMPT`; the corpus's `shortPrompt: '', mediumPrompt: ''`
row at `cases/system-prompt.ts:231` is EMPTY not whitespace) ⇒ GREEN unless a
variant carries leading/trailing whitespace — grep `cases/system-prompt.ts` for
`\n'` / `' ` inside prompt literals before trusting it;
`appearance_sanitize_gate_tier3` + `story_background_job_tier3` ⇒ RED iff a
corpus cast member has only `complete`/`full`/`headAndShoulders` variants (§3b);
`headshoulders_backfill_tier3` / `_enqueue` / `external_prompt_tier3` ⇒ GREEN
(no whitespace-only rows found in `headshoulders.json` / `external-prompt-tier3.json`);
`characters_physical_tier2` is the vault WRITE path — unaffected. Every one of
these corpora should GROW the whitespace / rare-variant rows red-first.

---

## 6. NEW `lib/files/generated-file-row.ts` — `createGeneratedFileRow`

- Header L1-28 (the two rules: "Stored, not sent" — `mimeType`/`size` from the
  bridge's `storedMimeType`/`sizeBytes`; "No label on a generated picture" —
  `description` null unless the caller has a real one, bug 132).
- `StoredImageBytes { storageKey, storedMimeType, sizeBytes }` L37-41;
  `GeneratedFileRowInput` L43-69 (`id?`, `sha256`, `originalFilename`, `stored`,
  `width?`, `height?`, `linkedTo`, `tags?`, `source?` default `'GENERATED'`,
  `generation?: { prompt?, model?, revisedPrompt? }`, `generationKey?`,
  `description?`, `projectId?`, `folderPath?`).
- L86-128: `id = input.id ?? randomUUID()`; `repos.files.create({ userId,
  sha256, originalFilename, mimeType: stored.storedMimeType, size:
  stored.sizeBytes, width: ?? null, height: ?? null, linkedTo, source: ??
  'GENERATED', category: 'IMAGE', generationPrompt: ?? null, generationModel: ??
  null, generationRevisedPrompt: ?? null, generationKey: ?? null, description:
  ?? null, tags: ?? [], storageKey, projectId: ?? null, folderPath: ?? null },
  { id })`; **DEBUG `[GeneratedFileRow] Created image file row`** `{ context:
  'files.generated-file-row', fileId, source, storedMimeType, sizeBytes,
  linkedTo, hasGenerationKey }`. "In the job child the create is buffered and
  the returned row is synthetic, so callers rely on `input.id`."
- Callers at the pin: `character-avatar.ts:442`, `story-background.ts:869`,
  `image-generation-handler.ts:172`, `wardrobe/item-images.ts:216`,
  `app/api/v1/wardrobe/preview-avatar/route.ts:191` (ROUTES lane).
- Test `__tests__/unit/lib/files/generated-file-row.test.ts` (2: `records what
  the bridge stored, with no label`, `mints an id and takes the source it is
  given`).
- v5: no shared row helper — each site builds its `files` insert (`tools/
  generate_image.rs:1300-1360` `save_generated_image`; the avatar write;
  `story_background_job.rs` ~`:1020-1070`; `services/wardrobe_item_images/`).
  Rows byte-identical except the WARDROBE `description` drop (§3d). The DEBUG
  line is new at every site.

---

## 7. Bug 196 — Almanack image spend

- `lib/schemas/llm-log.types.ts:44-54`: `IMAGE_SPEND_LOG_TYPES =
  ['IMAGE_GENERATION', 'WARDROBE_ITEM_IMAGE'] as const satisfies readonly
  LLMLogType[]`; `ImageSpendLogType`. The enum itself unchanged.
- `lib/database/repositories/llm-logs.repository.ts:782-795` `getStatsByProfile(
  userId, groupBy, options: { type?: LLMLogType | readonly LLMLogType[] })`:
  `types = options.type === undefined ? [] : typeof options.type === 'string' ?
  [options.type] : options.type; typeClause = types.length > 0 ? \`AND "type" IN
  (${types.map(() => '?').join(', ')})\` : ''; params = [userId, ...types]`. So
  a SINGLE type now renders `AND "type" IN (?)` (was `AND "type" = ?`) — the
  SQL text changes even for one type; the same aggregate otherwise.
- `lib/tools/almanack/phase6-wire-records.ts:141`: `repos.getStatsByProfile(
  imageGroupBy, { type: IMAGE_SPEND_LOG_TYPES })` (was `{ type:
  'IMAGE_GENERATION' }`). The connection roll-up (L138, no type) and the two
  median queries unchanged. `getMedianDurationByProfile` is NOT widened —
  image medians still count every type? (it takes no type at all — unchanged.)
- Tests: `__tests__/unit/lib/database/llm-logs-repository.test.ts` + describe
  `getStatsByProfile — image spend (bug 196)` (3: `filters on every type it is
  given, wardrobe pictures included` — asserts `AND "type" IN (?, ?)` + binds
  `USER_ID, 'IMAGE_GENERATION', 'WARDROBE_ITEM_IMAGE'`; `still takes a single
  type` — `AND "type" IN (?)`; `applies no type filter when none is given`).
- v5: `almanack/phase6_wire_records.rs:158-200` `get_stats_by_profile(db,
  user_id, group_by, log_type: Option<&str>)` with `r#"AND "type" = ?"#` at
  L165 and the single bind L194-196; the call at L423 `Some("IMAGE_GENERATION")`;
  `services/llm_logging.rs:379/388` has both constants. Port: a slice argument
  (`&[&str]`) rendering `IN (?, ?)`, a `IMAGE_SPEND_LOG_TYPES` const beside the
  log-type consts, the call widened. Harness `almanack_tier2_equivalence.rs`
  (1,296 lines; oracle `almanack-routes.test.ts`): no `WARDROBE_ITEM_IMAGE`
  rows in its fixtures (grep: none) ⇒ **GREEN on the pin**; grow the fixture
  with a `WARDROBE_ITEM_IMAGE` row red-first (the per-image-profile roll-up
  gains the row; the medians do not change shape). `almanack_render_equivalence`
  — the render text names the roll-up (`render.rs:1971` mentions the Concierge
  reroute second row) — unchanged. No SQL-capture census pins `"type" = ?`
  (grep of harness: none).

---

## 8. Bug 197 — NEW `lib/llm/vision-request.ts` + the describe-fallback onto it

### v4 at the pin (382 lines)
- `textOnlyTokenCeiling(text) = Math.ceil(text.length / 2.5)` L55-57
  (`MIN_CHARS_PER_TOKEN = 2.5`). `verifyImageReachedModel(response,
  attachmentId, requestText)` L87-122 — MOVED from `file-attachment-fallback.ts`
  and GENERALISED: the ceiling is `textOnlyTokenCeiling(requestText)` (was the
  module constant over the instruction); the reasons: `the provider reported the
  attachment as not sent: ${mine.error || 'no reason given'}` (unchanged) and
  **`the model was billed for ${billedInput} prompt tokens, which is no more
  than the ${ceiling} the request's text costs on its own — the image was
  accepted and discarded before it reached the model, and any answer returned
  is invented`** (was `… the ${INSTRUCTION_TOKEN_CEILING} the instruction costs
  on its own — … and any description returned is invented`). For the
  describe-fallback `requestText = instruction` (no system prompt), so the
  NUMBER is the same 66; the WORDS differ.
- `resolveVisionProfile(repos, userId, { prefer: 'cheap' | 'capable',
  configured: 'honour' | 'skip-incapable' })` L159-205: configured
  `imageDescriptionProfileId` → `findById` → if `configured === 'honour' ||
  profileCanReceiveAttachment(configured, 'image/jpeg')` → DEBUG `[Vision]
  Using the configured image description profile` `{ profileId, provider, model
  }` → return; else DEBUG `[Vision] Configured image description profile
  cannot receive images; auto-picking`; then `findByUserId(userId).filter(p =>
  profileCanReceiveAttachment(p, 'image/jpeg'))`; none → DEBUG `[Vision] No
  profile can receive images` `{ userId }` → null; `preferred =
  visionProfiles.find(p => prefer === 'cheap' ? p.isCheap === true :
  !p.isCheap) ?? visionProfiles[0]`; DEBUG `[Vision] Auto-picked a vision
  profile` `{ profileId, provider, model, isCheap, prefer }`. (Logger is
  `logger.child({ module: 'vision-request' })` — the capture-blind
  `logger.child` shape the memory note warns about.)
- `sendVisionRequest(input)` L248-382 — refusals as `ok: false`: `'model-cannot-read'`
  (`!profileSupportsMimeType(profile, mime)`, detail `${provider} ${modelName}
  does not read ${mime} files`), `'plugin-cannot-send'` (`image/*` &&
  `!providerCanTransportImages(provider)`, detail `the ${provider} plugin does
  not forward image attachments`), `'missing-api-key'` (only when
  `requireApiKey` and not OLLAMA, detail `API key not found for provider
  ${provider}`), `'image-not-received'`. The shrink L272-282:
  `shrinkImageForLlmTransport({ buffer, mimeType, provider, filename })` when
  `attachment.data`; `sentAttachment = shrunk.wasShrunk ? { ...attachment, data,
  mimeType: shrunk.mimeType, size: shrunk.finalSize } : attachment`.
  `messageParams = { model, messages: [system?, { role: 'user', content:
  instruction, attachments: [sentAttachment] }], temperature?, maxTokens? (>0),
  topP?, profileParameters? }` L284-295. `loggedRequest` L297-308 `{ messages:
  [system?, { role: 'user', content: instruction, attachments: [{ filename,
  mimeType: sentAttachment.mimeType }] }], temperature, maxTokens }` — **used for
  the FAILURE row too** (L347): pre-span the fallback's failed row was
  `{ messages: [{ role: 'user', content: INSTRUCTION }] }` only — a
  row-visible change in `llm_logs.request` on the timeout/throw arm. `record`
  L309-326 → `logLLMCall({ userId, type: input.logType, provider, modelName,
  request: loggedRequest, ...call, durationMs })`, catch → WARN `[Vision] Failed
  to record the vision call in llm_logs` `{ logType, error }`. DEBUG `[Vision]
  Sending image to vision model` `{ logType, profileId, provider, model,
  originalBytes, sentBytes, shrunk }` L328-336. `withTimeout(provider.sendMessage(
  messageParams, apiKeyValue), timeoutMs, \`Vision request timed out after
  ${timeoutMs}ms\`)` L341-345 (was `Image description timed out after …ms` —
  the message lands in the logged `response.error` and in the catch's
  `Failed to generate image description: <msg>` result). Arrival check against
  `${systemPrompt ?? ''}${instruction}` L357-361; not arrived → WARN `[Vision]
  Model answered without the image; discarding its answer` `{ logType,
  profileId, provider, model, reason, promptTokens, contentLength }` → `{ ok:
  false, refusal: 'image-not-received', detail, response }`; else DEBUG
  `[Vision] Vision answer received` `{ logType, profileId, contentLength,
  durationMs }`.

### `lib/chat/file-attachment-fallback.ts` (−313)
- `getImageDescriptionProfile` L39-44 → `resolveVisionProfile(repos, userId, {
  prefer: 'cheap', configured: 'honour' })` — same pick as before (configured
  honoured even if incapable; cheap first; `[0]`), plus the four `[Vision]`
  DEBUG lines.
- `describeImageWithProfile` L245-395: the sampling (reasoning-model 4000 cap,
  WARN `[Image Fallback] Reasoning model detected, increasing maxTokens from
  ${maxTokens} to 4000` kept at ~L258-262), then `sendVisionRequest({ profile,
  repos, userId, attachment: file, instruction: IMAGE_DESCRIPTION_INSTRUCTION,
  sampling: { temperature, maxTokens, topP }, timeoutMs: 60_000, logType:
  'IMAGE_DESCRIPTION' })` — NO `requireApiKey` (keyless still sent, as before).
  The refusal → result mapping L286-313, sentences UNCHANGED: `Image description
  profile (${provider} ${modelName}) does not support image files`; `… cannot
  send images — the ${provider} plugin does not forward image attachments. Pick
  a describer on a provider that does (OpenAI, Anthropic, Google, Grok,
  OpenRouter, NanoGPT, Z.AI).`; `… did not process the image — ${sent.detail}.
  Pick a describer on a model that genuinely reads images; a gateway may accept
  an image and route to a model that ignores it.` (the DETAIL text changed, see
  above). **GONE:** the `resizeImageForProvider`/`canResizeImage` block and its
  WARN `[Image Fallback] Resize for description provider failed; sending
  original` (the shrink never throws), the WARN `[Image Fallback] Describer
  answered without the image; discarding its description` (→ `[Vision] Model
  answered without the image; discarding its answer`), the WARN `[Image Fallback]
  Failed to record IMAGE_DESCRIPTION llm log` (→ `[Vision] Failed to record the
  vision call in llm_logs`), the module's `MIN_CHARS_PER_TOKEN` /
  `INSTRUCTION_TOKEN_CEILING` / exported `verifyImageReachedModel`. The catch
  L388-395 keeps `logger.error('[Image Fallback] Error generating description:')`
  + `Failed to generate image description: ${message}` and no longer logs the
  failed call itself ("sendVisionRequest has already logged").
- **Transport change:** the describer's bytes go through bug 151's
  `shrinkImageForLlmTransport` (1024 px edge, 500 KB base64 target, the quality
  ladder) instead of the provider-ceiling `resizeImageForProvider` — DIFFERENT
  bytes reach the describer for any image over those bounds.

### `lib/wardrobe/image-analysis.ts` (−180) — **SURVEYED, NO-PORT**
`runAnalyzeImageForWardrobeItems` L250-345: `resolveVisionProfile(repos, userId,
{ prefer: 'capable', configured: 'skip-incapable' })` (was its own
`findVisionProfile` over `profileSupportsMimeType` + `connections.findAll()`
with WARN `No vision-capable profiles found`); `NO_VISION_PROFILE_MESSAGE`
L245-248 (text unchanged); `sendVisionRequest({ …, attachment: { id:
'wardrobe-analysis-image', filename: \`analysis.${ext}\`, mimeType, size:
Math.ceil(image.length * 0.75), data }, systemPrompt: SYSTEM_PROMPT,
instruction: userContent, sampling: { temperature: 0.5, maxTokens: 4000 },
timeoutMs: 120_000 (NEW — no timeout before), logType:
'WARDROBE_IMAGE_ANALYSIS', requireApiKey: true })`; throw → ERROR `[Wardrobe
Image Analysis] LLM call failed` + `Image analysis failed: ${message}. Please
try again or use a different provider.`; `!sent.ok` → WARN `[Wardrobe Image
Analysis] Vision request refused` `{ provider, model, refusal, detail }` then
`'missing-api-key'` → `API key not found for provider ${provider}. Check your
connection profile settings.`; `'image-not-received'` → **`The AI returned an
answer without seeing the image (${sent.detail}). Pick a vision profile on a
model that genuinely reads images.`**; default → `NO_VISION_PROFILE_MESSAGE`.
INFO `Analysis complete` loses `durationMs`. v5's `analyze-image` is
REFUSAL-ARMED (`api/wardrobe.rs:1348-1360`, tier 3 §4; `phase-4.md:120` D4
lists it among the binary routes) — the standing deferral stands; this lane
ports NOTHING of it. Tests `__tests__/unit/lib/wardrobe/image-analysis.test.ts`
(12, incl. `sends the bytes shrunk for transport, not the upload`, `refuses an
answer the model gave without the image`, `passes over a configured profile
that cannot receive images`).

### v5 main
`services/file_fallback.rs` (1,970 lines) is the PRE-span shape whole:
`instruction_token_ceiling()` L75-77 + the reason text L180-187 ("the
instruction costs on its own"); `get_image_description_profile` L493-550 (the
inline pick); `describe_image_with_profile` L606-934 — mime L639-647, transport
L659-668, `try_downsize` L970-995 over `files::image_processing::
resize_image_for_provider` (the bug-151 shrink EXISTS at
`files/llm_image_budget.rs:168` `shrink_image_for_llm_transport`, used by
`services/chat_files.rs` + `message_context.rs` — not here); the WARN `Describer
answered without the image; discarding its description` L800; the two log
shapes `log_description_success` L996-1049 / `log_description_failure`
L1065-1113 (the failure row's request = instruction only, `attachments: None`,
no temperature — exactly the pre-span v4). `files/image_transport.rs` has
`provider_can_transport_images` + `profile_can_receive_attachment`. Port:
`llm::vision_request` { `text_only_token_ceiling`, `verify_image_reached_model(
response, attachment_id, request_text)`, `resolve_vision_profile(prefer,
configured)`, `send_vision_request` } with `file_fallback.rs` consuming it
(the analyze-image consumer stays refused). Harness
`file_attachment_tier3_equivalence.rs` (1,321 lines; oracle
`file-attachment-tier3.test.ts` — `logLLMCall` is a NO-OP there, so the
request-row change is invisible; the canned provider is keyed by `(provider,
model, attachment filename)` so the shrunk-vs-resized BYTES are invisible too;
the result TEXT is compared) ⇒ **RED on the pin only for an arrival-failure
case** (the reason wording) — grep the corpus for `promptTokens` ≤ 66 rows;
otherwise GREEN. Grow it: a bug-116 row, a timeout row (the new `Vision request
timed out` message in the result text), and run the llm-logs tier-2 projection
for the failed-call request shape. `chat_upload_auto_describe.rs` rides the
same path — check its pins on the WARN names.

---

## 9. Also in `lib/image-gen/profile-resolution.ts` — NEW `resolveAvatarImageProfile` (L146-210)

`resolveAvatarImageProfile(userId, repos, { override?, chat? })`: `usable =
profile && profile.userId === userId && !!profile.apiKeyId`; override →
`findById` → usable → return, else DEBUG `[ProfileResolution] Avatar override
profile unusable; falling back` `{ context: 'image-gen.profile-resolution',
override, found }`; `chat.imageProfileId` → `findById` → usable → return, else
DEBUG `… Chat image profile unusable for avatars; falling back` `{ chatProfileId,
found }`; `findDefault(userId)` with `apiKeyId` → return; else DEBUG `… No
usable avatar image profile` `{ userId }` → null. **Behaviour change at its
callers:** `lib/wardrobe/avatar-generation.ts` (−24) now takes override → chat
→ default EACH CHECKED FOR A KEY (pre-span: override `findById` presence only,
WARN `Avatar generation override profile not found, falling back`; chat
`findById` presence; `findAll().find(isDefault)` presence) + a new DEBUG `Avatar
image profile resolved` `{ context: callerContext, chatId, characterId,
imageProfileId, overrideRequested, overrideHonoured }`; `toggle-avatar-generation.ts:56`
and `chats/route.ts` (the initial avatar: DEBUG `No usable image profile;
skipping initial avatar generation`) and `preview-avatar/route.ts:77` call it.
Tests: `profile-resolution.test.ts` +3 (`takes the override first, then the
chat, then the default`, `passes over a keyless, foreign or missing candidate`,
`answers null when even the default has no key`); `avatar-generation.test.ts`
(4) adjusted. v5: `services/image_profile_resolution.rs` (`resolve_image_profile_for_chat`
L19, `resolve_wardrobe_image_profile` L129-185 — the sibling's exact shape, incl.
`usable`); `services/avatar_generation.rs:113-160` carries the pre-span three
reads + the WARN at L134 (pinned by `quilltap-harness/tests/chat_cast_routes_equivalence.rs:1935`
— that pin REDDENS when ported); `services/chat_create.rs:2404` calls
`trigger_avatar_generation_if_enabled`. **Ownership (§M4):** the function's
HOME is this lane's path; its callers are the ROUTES lane's. Recommend this lane
lands `resolve_avatar_image_profile` beside its sibling with a tier-1/2 family
and the `avatar_generation.rs` rewire (it is `lib/wardrobe/`, 49 lines, the
one caller v5 has today), and HANDS the route callers' rewire to the routes
lane — or the planner assigns all four to one lane; either way NAME it.

---

## M. Cross-lane meeting points

- **M1 — the POOL (phases a–c lane).** `item-image-generation.ts:118-127`
  `loadWearablePool` + `componentGraph`; `avatar-prompt.ts:165-169`
  `loadWearablePool`; `appearance-resolution.ts:108-111`;
  `wardrobe-announcement.ts:72-75`. v5: `wardrobe_item_image_generation.rs:
  156-224` `resolve_component_leaves` (the `shared_wardrobe_tiers_for_character`
  + `hydrate_component_graph` body), `avatar_prompt.rs`'s resolver call,
  `appearance_resolution.rs` `equipped_wardrobe_items_for_appearance`. Whoever
  lands the pool rewrites those bodies; THIS lane's hunks in the same files are
  the Concierge wiring + the `Outfit expansion stopped short` line + the pick.
  Order: pool first or this lane first — but the item-image tier-3 family's
  `[hydrateComponentGraph]` LOG_PREFIX leaves with the pool, and the pool lane's
  `[WearablePool]` lines may need adding to it. Agree the prefix list.
- **M2 — the KEYSTONE (what this lane needs from it): NOTHING schema-wise.**
  The span carries NO DDL / migration / index (ledger). Bug 189 writes to
  EXISTING tables (`moderation_refusals`, the Concierge announcement rows,
  `llm_logs.chatId`); bug 196 is a read. If the round has a keystone lane for
  the pool / location types, this lane depends on it only through M1.
- **M3 — the IMAGES lane (P4.D263's `services/wardrobe_item_images/`):**
  `item-images.ts:216` drops `files.description` for wardrobe pictures and adds
  `linkWardrobeItemImage`; the `wardrobe_item_images_tier2_equivalence` +
  `wardrobe_item_images_routes` families redden on `description`. Decide
  whether this lane's `create_generated_file_row` helper lands FIRST so the
  images lane consumes it (recommended: this lane ships the helper + the four
  job/tool sites; the images lane re-points `add_wardrobe_item_image` to it).
- **M4 — the ROUTES lane:** `preview-avatar/route.ts` (227 lines changed; v5's
  renderer seam `quilltap-host/src/avatar_preview.rs` + `api/wardrobe.rs:1079`)
  consumes `makeLoggedImageAttempt` / `decodeProviderImage` /
  `createGeneratedFileRow` / `resolveAvatarImageProfile`; `chats/route.ts` +
  `toggle-avatar-generation.ts` consume `resolveAvatarImageProfile`. This lane
  must export the four helpers with the exact option surfaces above (`prebuilt`,
  `characterId`-only attempts, `index`) so the routes lane needs no second
  shape.
- **M5 — the tools lane:** `tool-executor.ts` (−205) is the `WARDROBE_TOOL_TABLE`;
  nothing image-related in its hunk (grep: no `image`/`picture` lines).
  `tool-image-generation.ts` unchanged.
- **M6 — the docs-vendor lane:** `IMAGE_GENERATION.md`,
  `features/avatar-configuration-cache.md` (exist in `docs/v4/developer/`),
  `features/{ => complete}/wardrobe-refactor.md` (NOT yet mirrored — the span's
  planning doc), `help/wardrobe.md`'s analyst-profile paragraph (bug 197 —
  v5's refused route makes that paragraph describe an absent feature; the
  help lane should note it).
- **M7 — the unported scene-state job** (§5): record the `scene` order for
  `scene-state-tracking.ts:214` in the work order's deferrals.

---

## H. Harness — predicted state under the pin regen (summary)

RED (certain): `wardrobe_item_image_generation_tier3_equivalence` (the new
`Concierge policy resolved` DEBUG on all 23; `llm_logs.chatId` on the seven
`job_*`; the `[hydrateComponentGraph]` lines once the pool lands);
`lora_log_anchor_guard` (a source census — flips when the port retitles, not on
the regen); `chat_cast_routes_equivalence:1935` (flips when `avatar_generation.rs`
is ported, §9); the in-crate `character_avatar_job.rs:1776-1849` line tests
(flip on the port). RED (conditional on corpus contents — measure first):
`story_background_job_tier3` + `appearance_sanitize_gate_tier3` (a cast member
with only `complete`/`full`/`headAndShoulders` variants), `system_prompt_equivalence`
(whitespace in a variant), `file_attachment_tier3` (an arrival-failure or
timeout row), `image_generation_tier3` (an empty-`data` image).
GREEN (predicted): `avatar_job_tier3`, `avatar_cache_key`,
`wardrobe_item_image_prompt`, `tool_image_generation`, `image_failover_tier3`
(the chokepoint is unchanged), `almanack_tier2` + `almanack_render`
(no `WARDROBE_ITEM_IMAGE` rows planted), `headshoulders_backfill_tier3` /
`_enqueue`, `external_prompt_tier3`, `characters_physical_tier2`,
`llm_logs_tier2`. Every GREEN family in this list should GROW a red-first row
for the behaviour it now hides (named per section).

---

## T. v4 jest coverage at the pin (names + case counts)

- `__tests__/unit/lib/wardrobe/item-image-generation.test.ts` — 13 (incl. the
  bug-189 pair `resolves the Concierge against the chat a tool-queued picture
  came from (bug 189)`, `routes an Unmoderated chat direct to the uncensored
  desk`; `calls the failover chokepoint with no chat and purpose "wardrobe"`).
- `__tests__/unit/lib/background-jobs/handlers/wardrobe-item-image.test.ts` — 4
  (`draws the item from its owner's wardrobe, under the chat's Concierge state`
  asserts `chatId: 'chat-1'` in the args).
- `__tests__/unit/lib/image-gen/image-attempt.test.ts` — 7 (NEW).
- `__tests__/unit/lib/files/generated-file-row.test.ts` — 2 (NEW).
- `__tests__/unit/lib/characters/physical-description.test.ts` — 3 (NEW).
- `__tests__/unit/lib/database/llm-logs-repository.test.ts` — 57 (+3 bug 196).
- `__tests__/unit/lib/image-gen/profile-resolution.test.ts` — 16 (+3).
- `__tests__/unit/lib/chat/file-attachment-fallback.test.ts` — 30 (the five
  `verifyImageReachedModel` cases now import from `@/lib/llm/vision-request`
  and pass `INSTRUCTION`).
- `__tests__/unit/lib/wardrobe/image-analysis.test.ts` — 12 (NO-PORT).
- `__tests__/unit/lib/background-jobs/handlers/character-avatar-sha256.test.ts`
  — 6; `story-background-sha256.test.ts` — 4; `story-background-uncensored-target`,
  `-present-participants`, `-backfill-absent` unchanged; `scene-state-tracking.test.ts`
  — 19 (unchanged, handler unported in v5); `character-headshoulders-backfill.test.ts`
  — 6 (unchanged).
- `__tests__/unit/lib/tools/image-generation-concierge-failover.test.ts` — 3
  (unchanged); `image-generation-orientation`, `-tool-defaults` unchanged.
- The three MOVED: `lib/image-gen/__tests__/avatar-cache.test.ts` — 19;
  `avatar-prompt-aesthetic.test.ts` — 4; `avatar-prompt-gender.test.ts` — 3;
  plus `__tests__/unit/lib/image-gen/avatar-prompt.test.ts` — 11 (moved, pool mock).
- `__tests__/unit/lib/wardrobe/avatar-generation.test.ts` — 4 (adjusted);
  `__tests__/unit/lib/wardrobe/item-images.test.ts` — 24 (adjusted, M3);
  `item-images.link.test.ts` — NEW (M3 / routes).

---

## P. Deltas against the ledger's row prose (`drift-ledger.md:460-461`)

1. The (d) paragraph says the full-length pick's new fallthrough is the ONLY
   model-visible change and "the system prompt's `self-image` order is
   unchanged" — the ORDER is unchanged, but `pickPhysicalDescription` TRIMS and
   skips whitespace-only variants at EVERY caller (ten, not the three named),
   and `story-background.ts` has TWO `scene` sites whose pre-span chains were
   SHORTER (`buildBasicEnumeration` lacked `completePrompt`; the per-character
   fallback had only `medium ‖ short`) — model-visible on more inputs than the
   row records. `scene-state-tracking.ts:214` and `image-scene-tasks.ts:916`
   also widen from `medium ‖ short`.
2. The (e) paragraph says the `llm_logs` row shape is unchanged — true for
   `content`, but the WARDROBE row gains `chatId` on the job path (bug 189), the
   `generate_image` row is now AWAITED (ordering), and the describe-fallback's
   FAILED row gains the attachment + sampling keys (bug 197).
3. Not in the row: `files.description` dropped for wardrobe pictures
   (`item-images.ts:216`); NEW `resolveAvatarImageProfile` with its key-checked
   fallthrough (§9); the Lantern's missing-image WARN that v5 never carried.
4. The `[WardrobeItemImage] Chat unreadable…` WARN is a dead arm in v4 (safeQuery
   beneath it) — port as the fallback home, not a reachable branch.
