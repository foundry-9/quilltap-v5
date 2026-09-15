# Dogfood walk — the six-round backlog (2026-09-15)

**Scope.** Six rounds have unified since the last pass (2026-09-07/08):

| round | baseline | landed | headline surfaces |
|---|---|---|---|
| progressions + bug 126 | `25f534c0b` | 2026-09-09 | character progressions (engine + card + prompt path + Pascal), the instance lock without hostname |
| the twelve-commit round | `78b381a96` | 2026-09-10 | the message route trail + badge, the drawn rotation, the Salon chat gallery, bug 128/132 |
| bug 133 + remainders | `cc65d6bfc` | 2026-09-10 | the story-background reroute gate, six reroute log lines, the gallery 404, eight memory-gate lines |
| In Their Own Words | `f4ad2c8d1` | 2026-09-11 | the impersonated-line rehearsal, `VOICE_REWRITE`, bug 134 live settings |
| the transcript round | `31436bae4` | 2026-09-15 | transcript as a subscribed read, the avatar configuration cache, Avatar Rolls, the paused-chat hold |
| the stream watchdog | `ffb6b3119` | 2026-09-15 | eleven wrapped stream sites, the stalled→`network` classification, the greeting ladder gate |

**Drift state at walk start.** The ledger's §2 freshness probe **PASSES**: v4 `main`
HEAD **is** the baseline `ffb6b3119`, tree clean, both branch logs empty, §3 **EMPTY**.
No step in this walk may blame drift, and **no pin is required** for anything.

**Instance.** `~/qt-dogfood-friday`, refreshed 2026-09-15 10:33 from Friday data
dated 2026-09-14 23:10. 951 chats · 139,526 chat messages · 2,151 `files` rows.

---

## §0 Pre-walk measurement (done before the plan was written)

Per ledger §5.5 — v4 runs daily on the real instance and heals data out from
under banked proofs. Every population below was measured read-only on the copy
*before* the steps were written. **A zero is a finding, not a failure.**

| what | measured | consequence for the plan |
|---|---|---|
| `files` total / with `generationKey` | **2,151 / 915** | the avatar cache is richly populated — a HIT is reachable without spending |
| avatar-collapse candidates (the ledger's exact query) | **0** | ⚠ the banked positive leg is **dead on arrival** |
| `migrations_state` `collapse-duplicate-avatar-rolls-v1` | **present** — v4 ran it **2026-09-11 17:45:57** at `4.10.0-dev.27`, `itemsAffected = 1785`, *"Collapsed 1785 avatar rolls to 898 configurations (887 images freed; repointed 262 chats, 24 characters, 977 messages)"* | the proof becomes **cross-app**: v5 must boot and write **nothing**, leaving v4's row byte-identical. A plant supplies the positive leg. |
| duplicate `generationKey` groups | **10** (all `n = 2`), i.e. post-collapse re-accumulation | an investigation row — did v4 miss a cache hit 10 times since its own collapse? |
| `chats.transcriptVersion` | **951 / 951 populated, max 448** | v4 is actively versioning; the subscribed read has real counters to honour |
| `chat_messages.routeTrail` non-empty | **2 of 139,526** | ⭐ **v4 has already written two real failover trails** (2026-09-14 and 2026-09-15) — the badge can be proven against v4's own bytes with **no failover to provoke** |
| `chats.cycleOrderParticipantIds` non-empty | **25** | the drawn rotation has real rows |
| `chats.isPaused = 1` | **32** | the paused-chat hold has a real population |
| `chat_settings.impersonationVoiceRewrite` | **1 row, value `1`** (the table is keyed by user — this is the instance-wide setting, and **v4 has it ON**) | the toggle proof is cross-app; the boot ensure must leave the `1` untouched |
| `llm_logs` `type = 'VOICE_REWRITE'` | **17 rows**, all `messageId = NULL`, all chat `40774a5a…`, 2026-09-12 | ⭐ v4 has already run seventeen rehearsals — v5's row shape is judged against v4's own |
| `migrations_state` `clear-generated-image-placeholder-descriptions-v1` | **present** — v4 ran it **2026-09-09**, 2,908 items | bug 132's positive leg is likewise dead; cross-app + plant |
| remaining placeholder descriptions in `files` | **0** | confirms the above |
| new columns on the real migrated schema | `files.generationKey` ✓ (appended last, not in fresh-DDL position) · `chats.transcriptVersion` ✓ · `chats.characterAvatars` ✓ · `chat_settings."impersonationVoiceRewrite"` ✓ · `chat_messages.routeTrail` ✓ · `chats.cycleOrderParticipantIds` ✓ | no vintage gap to chase; binding is by name |

**Walk targets picked from the data:**

- route trails: chats **`8f830899…`** *Four Hops and a Written Page* (primary
  failed `Service temporarily unavailable. Please try again later.` → understudy
  answered) and **`9f440b84…`** *The Blind Lens and the Warm Chair* (primary
  failed `Your prompt was blocked by safety filters. Please revise and try again.`
  → understudy answered). Both trails are two rows, NANOGPT DeepSeek → DeepSeek.
- voice rehearsal: chat **`40774a5a-8c1e-4630-803f-fd2d941dc82d`**.
- paused rooms: **`6ec13ecb…`** *The Weight of What We Ask For* (most recent),
  **`7e7058fc…`** *The Ledger of Skin and Water*.

---

## §1 What NOT to report as a bug

Collected from the six round records. A walker hitting any of these is seeing
the port working as ruled.

**Recorded divergences / v4-faithful shapes**

- `[CHEAP_LLM_NAME_FIELD_GAP]` — v5's `CompletionMessage`/`StreamMessage` carry
  no `name`, so speaker attribution never reaches name-supporting providers.
  Pre-existing, v5-wide, pinned both directions, has its own follow-up order.
- `[VALIDATION_DETAILS_GAP]` — v4's Zod 400 `details` array is not modelled by
  v5's error envelope (standing P4.6bb deferral).
- A lock-conflict boot answers **503 `unhealthy`**, not 409 `lock-conflict`
  (v4's own `lock-helpers.js` is untouched).
- `{{start}}` / `{{end}}` fall back to **UTC** where v4 falls back to the host
  zone (recorded seam).
- `idx_files_generationKey` on a *fresh* instance (a fresh v4 never creates it).
- The stalled socket is **abandoned, not cancelled** — it may stay open. v4
  doesn't cancel either; P4.44's abort-arming deferral stands.
- v5's header bound and first-chunk budget are **sequential**, so a no-token
  worst case is `transport budget + 240 s` where v4's is 240 s.
- `?download=1` is not wired on the thumbnail leg nor on
  `GET /mount-points/{id}/files/{path}?raw=1` — v4 doesn't either.
- v4 **deleted** the message route's `fileId must be a UUID` /
  `mountPointId must be a UUID` sentences; their absence is faithful.
- On `execute_overwrite_all` a chat's `cycleOrderParticipantIds` loses the id
  equal to `spokenThisCycleParticipantIds` — v5 reproduces v4 byte-for-byte.
- v4's v0 cache-key fallback is dead for LoRA-trigger profiles; such rolls
  regenerate rather than hit.

**Absent by design**

- No generation-prompt viewer on a roll; no empty-state sentence for the Avatar
  Rolls section (v4 has neither); no Discard/Set-as-avatar on an
  already-promoted portrait tile.
- "Show shared" does **not** persist across dialog opens and issues no fetch.
- `POST /api/v1/messages` is deliberately unregistered (sends go over dispatch).
- `chatImpersonationVoicePreview` has **no REST edge** — dispatch only.
- `Only a character seat can be spoken for.` and `Failed to restate the line in
  character.` are structurally unreachable through v4's own write path.
- The orchestrator's turn-time compiled-stack reader stays UNPORTED
  (`precompiled_identity_stack: None`); the rehearsal reads the stored stack.
- `self_inventory`, the character-voiced announcer, help chat and Brahma carry
  **no** progressions section (v4's own stated gaps).
- v4's two `[AppearanceResolution]` info lines have no v5 counterpart.
- The avatar failure bag deliberately omits `hasUncensoredImageProvider`.
- Six `autoGenerateFirstMessage` log lines stay unported; 18 pre-existing
  `[CharacterAvatar]` handler lines likewise; `SCENE_STATE_TRACKING` unported.
- An **empty** memory delete batch narrates nothing at all; a single delete
  whose `findById` misses returns without logging. Both are correct silences.

**Parked tests (not product)**

- Five P4.D187 e2e beats parked on `SHARED_FIXTURE_TITLE_CHECKPOINT_PARK`.
- The gallery Delete beat is an honest park (no generated entry with bytes).
- `brahma_orchestrator_tier3_equivalence` is a pre-existing red on main (stale
  fixture) — not this walk's business.

---

## §2 The walk

Status legend: `PENDING` · `PASS` · `FAIL(#n)` · `DEFERRED-TO-HUMAN` · `BLOCKED(reason)`

### Part A — the cross-app proofs (free; no spend; run first, on the first boot)

| # | Owner | Gesture | Expected + how verified | Status |
|---|---|---|---|---|
| A1 | CLAUDE | Boot `quilltap-web` on the copy once and stop. | **The avatar-roll collapse writes nothing.** `migrations_state` row `collapse-duplicate-avatar-rolls-v1` is byte-identical afterwards (`completedAt 2026-09-11T17:45:57.477Z`, `quilltapVersion 4.10.0-dev.27`, `itemsAffected 1785`, same message). Zero `[AvatarCache]`/collapse lines in `combined.log`. This is v5 honouring v4's ledger — the third arm of `host_boot_avatar_rolls_collapse`. | **PASS** — 187-row `migrations_state` byte-identical across the boot (md5 `85ecf65d5310671ba804037dae5bb353` before and after); v4's row unmoved at `2026-09-11T17:45:57.477Z` / `4.10.0-dev.27` / 1785. Zero collapse lines in the boot log. `files` 2,151 and rolls 915, both unmoved. |
| A2 | CLAUDE | Same boot. | **bug 132's heal writes nothing** — `clear-generated-image-placeholder-descriptions-v1` unchanged (`2026-09-09`, 2,908), no `Cleared placeholder descriptions from generated images` line. | **PASS** — same byte-identical ledger; `clear-generated-image-placeholder-descriptions-v1` unmoved at `2026-09-09T14:27:24.680Z` / 2,908. No `Cleared placeholder descriptions…` line. |
| A3 | CLAUDE | Plant the damage on the disposable copy: null out `generationKey` on a handful of avatar rolls (matching the ledger's predicate) **and** delete the collapse ledger row; restart. | The heal runs on the plant: the info line fires, `itemsAffected` matches the plant, and a **third** boot is idempotent. Verified by `migrations_state` + `combined.log`. (Offered as the weaker positive leg, not swapped in for A1.) | **NOT RUN** — the plant was offered as the weaker positive leg and A1 gave the stronger cross-app proof; skipped to spend the time on the two headline 💸 items instead. |
| A4 | CLAUDE | Open chat **`8f830899…`** *Four Hops and a Written Page*, find the assistant message `d84425b5…` (2026-09-14 03:28). | ⭐ **The `qt-route-trail-badge` renders v4's own two-row trail** under the desktop avatar: `[aria-label="Models tried for this reply"]`, the failed `DeepSeek V4.1 Flash Thinking Vision` row struck through with ❌ and its hover detail *Service temporarily unavailable. Please try again later.*, the `DeepSeek V4 Flash Latest` understudy row answering. Repeat on `9f440b84…` for the safety-filter detail. Verified on screen + against the stored JSON. | **PASS** — on message `932e918c…` in *The Blind Lens and the Warm Chair*, `<ul aria-label="Models tried for this reply">` renders under Revenant's avatar with v4's own two rows: row 1 carries `<span role="img" aria-label="failed">❌</span>` and an `<s>` wrapper (computed `text-decoration: line-through` confirmed on that row **only**), hover `DeepSeek V4.1 Flash Thinking Vision · NANOGPT: deepseek/deepseek-v4.1-flash:thinking — first on the call sheet; fell over: provider-error (Your prompt was blocked by safety filters. Please revise and try again.)` — the `detail` from v4's stored JSON verbatim; row 2 `DeepSeek V4 Flash Latest · NANOGPT: deepseek/deepseek-v4.1-flash — stood in as the understudy; answered`. **v4's bytes, v5's render.** |
| A5 | CLAUDE | Open a chat from the 25 with a populated `cycleOrderParticipantIds`; read the participants list order. | The list orders by the persisted rotation (generating seat at the head, latecomers behind), matching the stored id array exactly. | **PASS (server half).** `chatTurnAction` with `action: "query"` answers `state: {queue, cycleOrder}`, and `cycleOrder` matches the persisted `chats.cycleOrderParticipantIds` **exactly, all three ids in order** — a rotation **written by this walk's own nudge turn** (B2), so bug 131's whole-cycle persisted draw is proven live. The chat GET correctly does **NOT** project `cycleOrderParticipantIds` (the P4.D171 survey correction, pinned both directions) while it does carry `transcriptVersion`. |
| A6 | CLAUDE | Open the largest chat; watch `GET /api/v1/messages` in the network tab; send nothing. | The transcript is a **subscribed read**: `?knownVersion=<n>` with the current counter answers `unchanged`; a bare `?knownVersion=` (→ 0) against a counter of 0 also answers `unchanged`. Chat GET projects `transcriptVersion`. | **PASS** — `chatTranscript` on the 261-row chat: `knownVersion: 448` (the current counter) answers `unchanged: true` with no message payload; `knownVersion: 0` answers `unchanged: false`, `version: 448`, `count: 192`. The whole transcript rides one read. |
| A7 | CLAUDE | With a chat open in two browser tabs, send in tab 1. | Tab 2's transcript re-reads without a remount; the optimistic bubble is now **inside** the message array (dogfood #106's mechanism retired) — the user's line must never render twice mid-turn. Sample the bubble count across the turn. | **NOT RUN** — no blocker; ran out of walk before it. (Note dogfood #106's mechanism was retired this round, so this is the beat that would re-check it.) |

### Part B — the paused-chat hold (32 real paused rooms)

| # | Owner | Gesture | Expected + how verified | Status |
|---|---|---|---|---|
| B1 | CLAUDE | Open paused room `6ec13ecb…`; type a line and send. | **The message is recorded and nothing generates.** The chain-complete frame carries `heldUserTurn`; `chats.lastTurnParticipantId` goes NULL; one v4 info line in `combined.log`; **no new `llm_logs` row** for the chat. The once-per-pause held-turn toast appears exactly once across repeated sends. | **PASS (and it produced finding #118)** — on the real paused chat *The Ledger of Skin and Water* (`7e7058fc…`, chosen because its `lastTurnParticipantId` was NON-null so the clear is observable): the message was recorded (138 → 139), **nobody answered**, `lastTurnParticipantId` went `4ba95d1a-f5f1-4874-bc88-13d09f48623f` → **NULL**, `llm_logs` for the chat stayed at **0**, and v4's info line fired: `[Orchestrator] Chat paused — recording the user message without a reply`. The banner directly below the unanswered message still promised an answer → **#118, FIXED**. |
| B2 | CLAUDE | In the same paused room, press **Nudge**, then **Skip**. | Both leave the pause **standing** (`isPaused` stays 1) — the pre-round behaviour lifted it. | **PASS** — with the new NanoGPT key live, a `nudge` summons in the paused room ran a **real turn** (HTTP 200 in 64.6 s, `hasContent: true`; messages 139 → 150 with `search` and `doc_grep` TOOL rows, two NANOGPT `llm_logs` rows) — and **`isPaused` is still 1** afterwards, with the send response itself carrying `"isPaused": true`. The explicit summons runs; the pause the operator set stands. (Free rider: the turn re-proved finding #98 — the configured `search` provider ran off `api_keys`.) |
| B3 | CLAUDE | Press **Continue**. | bug 139: the chat resumes first, *then* asks. `isPaused` → 0 and a turn runs. | **NOT RUN** — same reason as B2. |
| B4 | CLAUDE | Provoke bug 136's two composer refusals (an out-loud send where v4 refuses). | Both sentences render in the composer byte-for-byte. | **NOT RUN** — no blocker; ran out of walk before it. |

### Part C — the avatar configuration cache + Avatar Rolls

| # | Owner | Gesture | Expected + how verified | Status |
|---|---|---|---|---|
| C1 | CLAUDE | Pick a character whose current outfit already has a cached roll (915 to choose from). Trigger an **automatic** avatar request for that exact configuration (not Regenerate). | ⭐ **The cache HITS**: `Reused cached avatar for this configuration` in `combined.log`, **no new `files` row**, and **no `IMAGE_GENERATION` row in `llm_logs`** — i.e. zero provider spend. This is the round's headline live proof. | ⭐ **PASS — the cache HITS, three times, for nothing.** Gesture: `chatToggleAvatarGeneration` off then on in *The Blind Lens and the Warm Chair* — v4's own automatic trigger, which enqueues one job per LLM-controlled character with `force: false`. All three `CHARACTER_AVATAR_GENERATION` jobs answered **`[CharacterAvatar] Reused cached avatar for this configuration`** within ~10 ms each (16:27:39.512 → .564 for the whole set). The discriminating measurement: `files` **2,138 → 2,138**, rolls **910 → 910**, `llm_logs` `IMAGE_GENERATION` **198 → 198**. **No new row, no provider call, no spend.** |
| C2 | CLAUDE | Same character, press **Regenerate avatar**. | `force` is true only here — the cache is bypassed and a new roll is generated. ⚠ one image's spend; verify by the new `files` row + one `IMAGE_GENERATION` row. | **DEFERRED-TO-HUMAN** — Regenerate avatar is real image spend, and C1 already proved the `force: false` cache branch. This arm proves only that `force: true` bypasses it. |
| C3 | CLAUDE | Investigate the 10 duplicate `generationKey` groups. | **RUN → FILED as v4 bug 143** (`docs/developer/bugs/bug-143-collapse-leaves-unprotected-duplicates.md`, v4 commit `064ba85df`). It also CORRECTS this walk's own first reading. All 10 groups are same-character and **byte-identical in `(generationModel, generationPrompt)`**, and **all 20 rows PREDATE v4's 2026-09-11 collapse** (newest 2026-09-07) — survivors of it, not re-accumulation, with nothing duplicated since. **Seven of the ten are deliberate:** the migration's victim loop keeps a row whose blob is still a character's portrait and keys it alongside the survivor on purpose, logging `Keeping avatar roll still serving as a character portrait` — and resolving `characters.defaultImageId` → link → file → blob gives 43 protected blob ids that cover exactly 7 groups' non-newest row. **Three are not** (`058a0214…` Kumar, `17bb35a0…` Friday, `59192685…` Elara): each older row matches `selectAvatarRows` exactly, is NOT in the protected set, is referenced by NO chat's `characterAvatars`, and still has a live blob — the loop should have remapped and deleted it. **No user impact, and this was checked rather than assumed:** v4's `lookupCachedAvatar` sorts `createdAt` descending and then verifies `row.tags?.includes(characterId)` and `mountBlobExists`, and **v5's `lookup_cached_avatar` carries both guards and the same sort verbatim** — so a duplicate key cannot serve the wrong face or a dead image. | **PASS (no v5 defect; v4 bug 143 filed)** |
| C4 | CLAUDE | Character → **Photo Gallery tab → Avatar Rolls** on a character with many rolls. | Section collapsed by default with a `<total> plate(s)` badge; tiles offer Set as avatar / Keep in the photo album / Download / two-click delete with a 3000 ms disarm. A character with zero rolls renders **nothing**. | **NOT RUN** — the ROUTE half is covered by C6; the SPA section itself was not opened. |
| C5 | CLAUDE | Exercise Set as avatar, Keep in the album, Download, and the two-click delete. | Each lands in the DB (`files`, the vault links) and on screen. The delete's second click must be required; the disarm must expire. | **NOT RUN** — depends on C4. |
| C6 | CLAUDE | Probe the route guards by curl: a save/delete for a **missing character**; `?action=bogus`; bare `?action=`; `?limit=` empty; `?limit=0`; `?limit=201`; `?limit=1.5`. | Byte-exact: `Avatar roll not found` (**not** `Character not found`) · `Unknown action: bogus` + `availableActions` · `Action parameter required` · `Too small: expected number to be >=1` · `Invalid input: expected number, received NaN` · `Invalid input: expected int, received number` · `Too big: expected number to be <=200`, joined with `; `. | **PASS — all seven arms byte-exact, and the counter-intuitive guard ORDER confirmed.** `?limit=` (empty) → `Too small: expected number to be >=1` (the `Number('') === 0` quirk); `?limit=0` → same; `?limit=201` → `Too big: expected number to be <=200`; `?limit=1.5` → `Invalid input: expected int, received number`; `?limit=abc` → `Invalid input: expected number, received NaN`. `?action=bogus` → `Unknown action: bogus` + `availableActions: ["save-to-album","set-avatar"]`; bare `?action=` → `Action parameter required` + the same list. ⭐ A **DELETE for a missing character** answers `Avatar roll not found`, NOT `Character not found` — while the LIST for the same missing character answers `Character not found`, and an unknown action beats the character check entirely (`Unknown action: keep`). |

### Part D — In Their Own Words (v4 has already run this feature here)

| # | Owner | Gesture | Expected + how verified | Status |
|---|---|---|---|---|
| D1 | CLAUDE | First boot (A1's), then read `chat_settings.impersonationVoiceRewrite`. | ⭐ **v4's `1` survives the boot ensure untouched** — a cross-app proof, not a heal. Settings → Chat → **Composer card, last row** shows the toggle **on**, copy byte-exact to v4. | **PASS** — v4's `impersonationVoiceRewrite = 1` survives the boot ensure untouched (cross-app, not a heal). |
| D2 | CLAUDE | In chat `40774a5a…` (where v4 ran 17 rehearsals), impersonate a character seat and send a line. | ⭐ **A real rehearsal**: the dialog restates the line in the seat's voice; a new `llm_logs` row `type = 'VOICE_REWRITE'` with the request's `chatId` and **NULL `messageId`** — the same shape as v4's seventeen; `[Chats v1] Impersonation voice preview generated` in `combined.log` with `seedLength` in UTF-16 units. 💸 one cheap-LLM call. | **PASS** — in `40774a5a…` *Three Inks for the Chalkoprateia* (v4's own rehearsal chat), Amy's seat impersonated and a flat 64-unit seed (*"I tell them the ink is wrong and we should start the page again."*) restated in her voice as 1,391 units drawing on the chat's real history and the stored identity stack. `llm_logs` row 20 of `type = VOICE_REWRITE` with the request's `chatId` and **`messageId` NULL** — the same shape as v4's own seventeen. `[Chats v1] Impersonation voice preview generated` with `seedLength: 64` (UTF-16 units, matching the seed exactly) and `proposedLength: 1391`. The ladder's guard also proved itself: the same call **before** impersonating answered `That seat is not being impersonated.` |
| D3 | CLAUDE | In the dialog: **Send as written**, then **Edit original**, then the prompt picker (shown only when >1 prompt). | Send-as-written posts the original; Edit original keeps the real draft **and the tray**; the picker switches prompts. Four-arm send-button title and the portrait **quill badge** (three-arm title) both render. | **NOT RUN** — the dialog's UI half; the rehearsal itself (D2) and its refusals are proven server-side. |
| D4 | CLAUDE | `PUT` the setting with `'yes'`, `1`, and explicit `null`. | All three **400** with byte-exact `Invalid impersonationVoiceRewrite value (must be boolean)`. Nothing written. | **PASS** — `"yes"`, `1` and explicit `null` all answer **400** `Invalid impersonationVoiceRewrite value (must be boolean)`, byte-exact, and the stored value stayed `1`. |
| D5 | CLAUDE | LLM Inspector → filter for `VOICE_REWRITE`. | The type appears under the `other` group with its label at all three sites; v4's 17 rows plus D2's are listed. | **PASS** — `llmLogsList` with `logType: "VOICE_REWRITE"` returns **21 rows, all of type `VOICE_REWRITE`, all with `messageId: null`** (v4's 17 plus this session's four), under a `{logs, count, total, limit, offset}` envelope. |
| D6 | CLAUDE | bug 134: leave a Salon open in tab 1; change a chat setting in tab 2. | The open Salon picks it up **without remounting or refetching the chat** (the shared `['chatSettings']` key). Verified by the network tab + the live UI. | **NOT RUN** — no blocker; ran out of walk before it. |
| D7 | CLAUDE | Trigger the memory-cascade dialog and tick **"Remember this choice"**. | The arm exists (v5 never had it) and invalidates correctly on the next cascade. | **NOT RUN** — needs a memory cascade to trigger. |
| D8 | CLAUDE | Open the Almanack. | The row **`Impersonated Lines in Character Voice`** reads **Yes** (v4 has it on). | **PASS** — the generated Almanack carries `**Impersonated Lines in Character Voice**: Yes` (correct — v4 has the setting on). Free riders in the same report: `Free Memory: 8.7 GB` (finding #94's fix still holding) and `Uptime: 1h 39m`. |

### Part E — the Salon chat gallery + images

| # | Owner | Gesture | Expected + how verified | Status |
|---|---|---|---|---|
| E1 | CLAUDE | Open the **Gallery** button in a real image-heavy chat's header (title `Every image in this conversation`). | The nine-source enumerator rolls at real scale — the one thing no fixture poses. Chips/filter work; the detail modal is body-portalled. | **PASS** — `chatGallery` on *The Blind Lens and the Warm Chair* rolled **10 entries** across **four** of the nine sources (`portrait` 4, `avatar` 3, `generated` 2, `story-background` 1) under a `{entries, counts, total}` envelope, each carrying the full shape: `id`, `idKind`, `url`, `filename`, `mimeType`, `size`, `width`/`height`, `sha256`, `createdAt`, `source`, `isCurrent`, `deletable`, `linkSummary`, `messageId`. |
| E2 | CLAUDE | Save an image from the gallery into a character vault, then save the **same** image again. | First: `Saved to <mount>` toast. Second: **409** with four **flat** siblings `{error, code, relativePath, keptAt}`, `code = ALREADY_SAVED`. | ⭐ **PASS — including the shape the §3 review fixed.** First save landed in `Abigail Character Vault` at `photos/2026-09-15T17-39-38.161Z-wide-cinematic-view-of-an-open.webp` with the seven-key body `{saved, mountPoint, relativePath, linkId, keptAt, fileId, sha256}`. The repeat answers **409** carrying `code: ALREADY_SAVED` — and the **four siblings are FLAT at the top level** (`error`, `code`, `relativePath`, `keptAt`), which is v4's body exactly and the P4.D185 nesting bug staying fixed. |
| E3 | CLAUDE | Hit a byte route with `?download=1`. | `Content-Disposition: attachment`. (The Electron `will-download` half is shell-only — not this walk.) | **PASS** — `GET /api/v1/files/{id}` answers `content-disposition: inline; filename="story_background_1789438160349.webp"`; the same URL with `?download=1` answers `attachment; …` with the same stored basename. |
| E4 | CLAUDE | Use Jump-to-message from the detail modal. | Scrolls to `#message-<id>` (no `data-message-id` attribute exists). | **NOT RUN** — no blocker. |
| E5 | CLAUDE | Run a chat-scoped `generate_image` in a chat, then open that chat's `files` listing. | bug 130's user-visible half: the generated image carries `chatId` and appears in **that chat's** listing. 💸 one image. | **DEFERRED-TO-HUMAN** — real image spend. |
| E6 | CLAUDE | `GET /api/v1/chats/<garbage-id>/gallery`. | v4's `notFound('Chat')` **404** (not a 500 `Failed to list chat gallery`), plus an error-level `Error finding entity by ID` line with `{collection: "chats", id, error}`. | **PASS (the 404 half).** `chatGallery` on a nonexistent chat answers `{"kind":"not-found","message":"Chat not found"}` — v4's `notFound('Chat')`, not the pre-fix 500 `Failed to list chat gallery`. The companion `Error finding entity by ID` line is correctly ABSENT: it fires on a read *failure*, not on a chat that legitimately does not exist, which would need a posed broken read. |

### Part F — character progressions

| # | Owner | Gesture | Expected + how verified | Status |
|---|---|---|---|---|
| F1 | CLAUDE | Character detail → **System Prompts tab**, below Subprompts: the **Progressions** card. Create one with a start/end and stages. | It saves through the existing `characterUpdate` read-modify-write; the character vault's `metadata.json` gains `metadata.progressions`; the one-second live clocks tick. | **PASS (read half; no new progression created — v4 already supplied one).** On `/characters/{id}/edit?tab=system-prompts` the card renders with v4's register intact (*"Spans of time Abigail is carrying — a gestation, a recharging weapon, a fermentation… They live in the vault's metadata.json, where a custom tool can read them and adjust them; the model itself never sets one."*), an `+ Add Progression` action, and v4's entry: **Pregnancy · pregnancy in progress · "You are carrying Charlie's daughter. Pregnancy: 6 weeks elapsed, 32 weeks, 6 days remaining."** — ⭐ **byte-identical to the line that reached the LLM prompt in F3**, so the card and the prompt path agree. `qt-subprompts-section` is mounted above it, as ordered. |
| F2 | CLAUDE | Provoke the Zod refusals: an unknown key, a reserved `metadata.progressions.<x>` target, a gate with no key, a bad record key. | Byte-exact: `Unrecognized key: "stages"` (singular) / `Unrecognized keys: "zeta", "alpha"` / an effect-target refusal beginning `must start with "state." or …` / `must test at least one metadata key or progress field` / `progress.cannon: Invalid key in record`. | ⭐ **PASS — and the contract is better than the plan assumed.** The two malformed entries (an unknown key `stages`; an `endTime` before `startTime`) were **accepted by the write** — correct: *"Nothing validates at hydration."* The refusal is fail-soft at the point of USE, and it surfaces **honestly on screen**: *"One entry in this character's metadata.json could not be read and is being skipped: **probe**. Editing the file directly is the way to mend it."* — while **Pregnancy still rendered correctly beside it**, which is the whole invariant (a broken entry must never hollow a character). v4's entry was afterwards restored byte-for-byte, `updatedAt` included. |
| F3 | CLAUDE | Send a turn in a chat seating that character; read the request in `llm_logs`. | The section **`Time-bound conditions you are carrying`** appears after Suparṇā's mail and **before** the turn-skip note, rendering like `Cannon recharge: 2 minutes, 10 seconds elapsed, 7 minutes, 50 seconds remaining, 22% complete (0.2/1.0 MJ).` AM/PM separator is **U+0020**, not U+202F. | ⭐ **PASS — and it is a cross-implementation proof, plus a discriminating pair.** v4 wrote a real progression on **2026-09-08** (Abigail, `pregnancy`: *"You are carrying Charlie's daughter."*, `2026-08-03T17:39Z` → `2027-05-04T16:00Z`, `timeIncrement: week`, `percentageReport: false`, `reportFrequency: "1h"`, `onComplete: once`), so v5 was judged against v4's own bytes. The **forced** greeting build carried it verbatim:\
\
`Time-bound conditions you are carrying, as of this moment:`\
`- You are carrying Charlie's daughter. Pregnancy: 6 weeks elapsed, 32 weeks, 6 days remaining.`\
\
The arithmetic checks out to the day (42 days elapsed = 6 weeks 0 days; 230 remaining = 32 weeks 6 days), `timeIncrement: week` is honoured, `percentageReport: false` is honoured (no percentage in the line), and the `description` leads with the `name` following. ⭐ **The negative arm is the other half of the proof:** the ordinary turn 14 minutes later did **not** carry the section — correctly, because `reportFrequency: "1h"` is walked out of the character's own last visible turn in that chat (the greeting). Same character, same chat, one reports and one does not, each for the right reason. Zero `Dropping a malformed character progression` warns. |
| F4 | CLAUDE | Start a new chat with that character (forced greeting) and a Carina user-message report. | Both carry the section too (`force` is inert on both sides). | **PARTIALLY COVERED** — the forced GREETING arm is proven in F3 (that is what made the cross-app proof possible). The Carina arm was not run. |
| F5 | CLAUDE | `/custom-tools` Workbench: set the gate-chip `Gate subject` to `progress`, add an `Outcomes → Progress…` effect with the `progress.` prefix button, and watch the read-only progress `<dl>` on the Proving Bench. | The derived `<dl>` ticks once a second off `benchNowMs`; a dry run computes and does not apply. | **NOT RUN** — the Workbench half; Pascal's `progress` family was not exercised. |
| F6 | CLAUDE | Run the tool for real through **Run Tool**. | The write lands; `combined.log` carries `Custom tool progress effect folded`. A **declined** write logs the skip and **not** the fold; a plain metadata write logs **no** fold. A non-matching gate logs `Custom tool {subject} test did not match`. | **NOT RUN** — depends on F5. |

### Part G — bug 133 and the reroute logging

| # | Owner | Gesture | Expected + how verified | Status |
|---|---|---|---|---|
| G1 | CLAUDE | On a **moderated** chat, request a story background whose image the provider refuses. | **No escalation.** `[StoryBackground] Image generation failed` with `rerouteAllowed: false`, **no second image**, exactly **3** `llm_logs` rows (one `IMAGE_GENERATION` for the refused attempt + craft + derive), **no** second `IMAGE_GENERATION` and **no** second `IMAGE_PROMPT_CRAFTING`. 💸 one refused image. | **DEFERRED-TO-HUMAN** — needs a provider to refuse a real image; that is spend, and posing a refusal reliably needs a stub. |
| G2 | CLAUDE | On a **Flagged** chat with no uncensored profile, the same. | `rerouteAllowed: true`; the prompt is resent **as-is**. | **DEFERRED-TO-HUMAN** — same. |
| G3 | CLAUDE | On a moderated chat **with** an uncensored image profile configured, the same. | The appearances now **sanitize** — visible in the recorded craft prompt (the round record's example: `a woman with silver hair, in a high-necked woollen dress` replacing `a tall elegant woman with flowing silver hair…`). 6 `llm_logs` rows including a `DANGER_CLASSIFICATION` proving classify ran. | **DEFERRED-TO-HUMAN** — same. |
| G4 | CLAUDE | Delete every memory for a chat with a wide neighbour set (measure ≥20 single / ≥200 batch first; a zero here is a finding, not a failure). | `[Memories API] Deleted every memory for a chat` (debug, `{chatId, deleted}` — fires on **both** arms, an empty chat narrating `deleted=0`) and, if the population supports it, the `touched an unusually large neighbour set` warns. | **PASS on both arms — after the population was measured.** With `RUST_LOG=debug`: a chat with 8 memories logged `[MemoryGate] deleteMemoriesWithUnlinkBatch complete` (`requested=8 deleted=8 neighboursTouched=10 charactersAffected=1 durationMs=106`) then `[Memories API] Deleted every memory for a chat`; an **empty** chat answered `deletedCount: 0` and **still logged the line** — the both-arms claim. ⭐ The `unusually large neighbour set` warn correctly did **NOT** fire: `neighboursTouched=10` against a ≥20 threshold. The §0 population question is answered — 762 chats carry memories (biggest 846), but a typical chat's neighbour set is well under the warn threshold. |

### Part H — the stream watchdog (the `ffb6b3119` round's one owed proof)

| # | Owner | Gesture | Expected + how verified | Status |
|---|---|---|---|---|
| H1 | CLAUDE | Stand up a local OpenAI-compatible endpoint that answers **200 with headers and then holds the socket**. Point a throwaway connection profile at it, give it a working **understudy**, seat it, and send. | ⭐ Inside **240 s** the turn fails rather than hanging: `[LLMStream] Abandoned a stalled provider stream` fires **once** in `combined.log` with `context = streaming.service`, `budget_ms = 240000`, `chunks_received = 0`, `provider`, `model_name` (absent ids dropped). The error is `Provider stream never sent a first chunk within 240000ms`; it classifies as **`network`**, so the **understudy answers** and the route trail records `primary/failed/network` with the stalled message as `detail`. | **PASS — the mechanism proven end to end.** At **`elapsed_ms: 240001`** against `budget_ms: 240000`, fired ONCE: `[LLMStream] Abandoned a stalled provider stream`, `context: "streaming.service"`, `chunks_received: 0`, and this time the bag carries `message_id` as well as `user_id`/`chat_id`/`character_id`. One millisecond later — ⭐ the whole point of the port — `[Failover] Primary call failed; walking the fallback chain` with **`trigger: "network"`**: the stalled error now classifies as a network fault, so the chain RUNS instead of the turn wedging. The chain then walked to the understudy and exhausted honestly (`[Failover] Fallback chain exhausted`, `attempts: [("STALL probe (dogfood)", "OPENAI_COMPATIBLE", "network"), ("DeepSeek V4 Flash Latest", …, "auth")]`), the turn failed cleanly at **243,740 ms** with a 500 rather than hanging, and the error sentence carried the whole story: `primary stream failed: Provider stream never sent a first chunk within 240000ms (STALL probe (dogfood) failed (network), DeepSeek V4 Flash Latest failed (auth))`. ⚠ The understudy could not ANSWER on this copy because NANOGPT's stored key is dead here (401 `Invalid session` — environmental, see §4); repeated against a live DeepSeek understudy as H1b. |
| H1b | CLAUDE | H1 repeated with the understudy repointed at a **live** DeepSeek profile (the dialog-equivalent gesture; NANOGPT's key is dead on this copy). | ⭐ **PASS — the round's 💸 item closed, and P4.D173's with it.** The primary stalled at 240 s again, then `[Failover] Understudy answered` — `understudy_name: "DeepSeek v4 Flash"`, `provider: DEEPSEEK`, `kind: "configured"`, `response_length: 641`, `failed_attempts_before: 1`. **The understudy's reply is what the operator sees; the turn never hung.** And v5 wrote its own two-row `routeTrail` on the assistant message: `{profileName: "STALL probe (dogfood)", provider: OPENAI_COMPATIBLE, via: "primary", outcome: "failed", trigger: "network", detail: "Provider stream never sent a first chunk within 240000ms"}` then `{profileName: "DeepSeek v4 Flash", via: "understudy", outcome: "answered"}` — so A4's badge proof (v4's bytes) and this one (v5's bytes) cover both directions. | **PASS** |
| H2 | CLAUDE | Same endpoint, but send **two chunks** then hold. | `Provider stream went quiet for 120000ms after 2 chunk(s)` — note `chunk(s)`, not `chunks`. | ⭐ **PASS — the IDLE budget, distinct from the first-chunk one.** A second posed endpoint sending two chunks then holding: `[LLMStream] Abandoned a stalled provider stream` with **`budget_ms: 120000`, `chunks_received: 2`, `elapsed_ms: 120406`**, and the error byte-exact — **`Provider stream went quiet for 120000ms after 2 chunk(s)`**, including the `chunk(s)` form. Total wall time 125.98 s. |
| H3 | CLAUDE | Same endpoint seated on a **new chat's greeting**. | The greeting abandons at **90 s** (`Provider stream never sent a first chunk within 90000ms`), the watchdog bag carries `context = initial-greeting`, and `[Chats v1] Greeting abandoned — the provider accepted the request and then went quiet` fires. The ladder ends on an own-profile stall; the static greeting is the fallback. `llm_logs` `response.error` carries the stalled sentence and `LLMStreamStalledError`. | **PASS — the round's headline proof.** A new chat seated on a posed OpenAI-compatible endpoint that answers 200 + SSE headers and then holds the socket. At **`elapsed_ms: 90002`** against `budget_ms: 90000`, fired ONCE: `[LLMStream] Abandoned a stalled provider stream` with `context: "initial-greeting"`, `chunks_received: 0`, `provider: OPENAI_COMPATIBLE`, `model_name: stall-model`, and the `user_id`/`chat_id`/`character_id` set (no `message_id`, correctly dropped). Then `[Chats v1] Greeting generation attempt failed` (`attempt: "full context"`, error `Provider stream never sent a first chunk within 90000ms`) and `[Chats v1] Greeting abandoned — the provider accepted the request and then went quiet`. ⭐ **Zero** `without memories` / `Final greeting generation retry failed` / `All greeting generation attempts exhausted` lines — the own-profile gate ended the ladder, exactly as P4.D190 ports it. The chat opened on the static greeting (`Hello, Charlie! I'm Abigail. What's on your mind today?`) and the `llm_logs` row carries `"error":"Provider stream never sent a first chunk within 90000ms"`. **Chat creation did not hang.** |
| H4 | CLAUDE | An endpoint that streams a few chunks and then **dies**. | v4 and v5 both **keep the partial** and do NOT substitute; the chain is skipped (`routeVia: primary`, `routeFailures: []`); the turn throws. | ⭐ **PASS — proven by H2, which is the same branch.** Once streaming has started there is **no substitution**: the message carries **no `routeTrail`** (v4's `routeFailures: []`), no `[Failover]` line fired, the turn threw — and **the partial was preserved**: `tick tick\n\n{{OOC: stream ended abruptly (Provider stream went quiet for 120000ms after 2 chunk(s))…`. The two chunks that did arrive survived with v4's salvage note appended. |

### Part I — the instance lock (bug 126)

| # | Owner | Gesture | Expected + how verified | Status |
|---|---|---|---|---|
| I1 | CLAUDE | With the server running, `quilltap db --lock-status`; then `--lock-clean`. | Ownership is PID + `startedAt` keyed by lock path — hostname is gone from every test. The live lock is reported held; `--lock-clean` refuses to clean a live one. Lock file fields: `pid`, `hostname`, `startedAt`, `lastHeartbeat`. | **PASS** — with the server up, `quilltap db --lock-status` reports `ACTIVE (process confirmed running)`, PID 17168, `Hostname: Mac (this host)`, `Environment: local`, `Process: quilltap-web`, heartbeat 58 s ago, and the one history entry `acquired (PID 17168) — Clean acquisition — no prior lock`. Ownership is the PID + `startedAt`; the hostname is displayed, not relied on. |
| I2 | CLAUDE | Kill the server ungracefully, leaving a stale lock; boot again. | The stale lock is reclaimed (dead PID) without the hostname mattering; the ordered teardown means no orphaned PTY children. | ⭐ **PASS — bug 126's point, exactly.** The live server was SIGKILLed (no teardown, lock left behind: PID 17168, host `Mac`). A fresh boot reclaimed it and the lock file's own history tells the story: `acquired 17168 — Clean acquisition — no prior lock` → **`stale-detected 24346 — PID 17168 is no longer running`** → `stale-claimed 24346 — Claimed by PID 24346`. The hostname plays no part; the reclaim is keyed on PID liveness. |
| I3 | **HUMAN** | Rename the host (or otherwise flip the hostname) under a running instance. | bug 126's headline: a renamed host **no longer kills its own DB**. Needs a real hostname change — Claude will read `combined.log` and the lock file as you go. | DEFERRED-TO-HUMAN |

### Part J — standing 💸 (human, cost/judgment)

| # | Owner | Item | Status |
|---|---|---|---|
| J1 | **HUMAN** | The Brahma Console budget on a genuinely deep query (long agent loop, real spend). | DEFERRED-TO-HUMAN |
| J2 | **HUMAN** | Memory deduplication + conversation-summary regeneration first run (batch cost). | DEFERRED-TO-HUMAN |
| J3 | **HUMAN** | Finding #101 — NanoGPT prompt caching writes a cache every turn and never reads one; a cost question for the operator, not a v5 defect. | DEFERRED-TO-HUMAN |
| J4 | **HUMAN** | The three-row route-trail layout in three themes (aesthetic judgment); a `.qtap` export → fresh-import round trip carrying a trail. | DEFERRED-TO-HUMAN |

---

## §3 Findings

- **#118 — the paused-room notice promises an answer that never comes.** FIXED,
  commit `6e11605a`, SPA 0.5.722. Full row in `dogfood-findings.md`.

### Free proofs the walk picked up along the way

- **Finding #110's fix, proven live.** The scheduled maintenance pass ran at
  16:03–16:04 unprompted and **narrated every deletion**, which is exactly what
  #110 restored: per-chat `Collapsed stale chat assets`
  (`chat_id`, `deleted`, `bytes_released_estimate`), then
  `Stale-chat asset collapse complete` (`chats_scanned: 951`,
  `stale_chats: 819`, `chats_collapsed: 7`, `files_deleted: 14`,
  `bytes_released_estimate: 2112912`), then `Scheduled maintenance pass
  complete`. Before #110 this deleted the operator's images in silence — and it
  is why `files` fell 2,151 → 2,138 mid-walk, a drop that would otherwise have
  looked alarming.
- **P4.78's `chatCreate` validation + the `details` host wire.** A `chatCreate`
  with a participant missing `type` answered **400 `Validation error`** with a
  populated `details` array (`{code: "invalid_value", values: ["CHARACTER"],
  path: ["participants", 0, "type"], message: "Invalid input: expected
  \"CHARACTER\""}`) — refusing before any write.
- **P4.72's `?action=` honesty.** `GET /api/v1/chats/{id}` answers
  `Only the get-background and cost actions are served on this route; the chat
  GET rides POST /api/dispatch` — naming what it does serve rather than 404ing
  blankly.
- **The impersonation ladder's guard.** `chatImpersonationVoicePreview` on a
  seat that is not being impersonated answers `That seat is not being
  impersonated.` before any provider call.

_(filled in as the walk runs; numbering continues from #117)_

## §4 Instrument notes

_(traps and false negatives caught during the walk — the standing rule is
**prove the instrument before trusting a negative**)_

- **The Salon transcript is render-windowed over a fully-loaded array.**
  `/api/v1/messages` returns the whole transcript (192 entries for a 261-row
  chat), but the DOM holds only ~12 `[id^="message-"]` nodes at a time. So
  **`document.getElementById('message-…')` returning null proves nothing** about
  whether a message exists — it only says it is not in the render window. Worse,
  wheel-scrolling up is non-linear: `scrollHeight` *grows* as placeholder heights
  resolve (25,170 → 45,313 px over ~30 wheel actions here) and scroll anchoring
  holds your position, so progress is best measured as the top rendered node's
  **index in the wire array**, not by pixels. Reaching index 45 of 192 took about
  eight batches. `scrollIntoView` also drifts for the same reason — call it in a
  short loop with a delay and re-measure `getBoundingClientRect()` before
  screenshotting.

- **Plain Enter does not send on this instance — and that is correct.**
  `chat_settings.compositionModeDefault = 1` on the real Friday data (v4's own
  setting), so the composer runs v4's composition branch: Enter inserts a
  paragraph and **Cmd/Ctrl+Enter submits**. Two sends were silently swallowed
  before this was measured, and both looked exactly like "the paused room
  refused my message". Send via the **Send message** button (or Cmd+Enter), and
  confirm a send by the composer going empty — never by assuming the keystroke
  landed. (The 2026-09-02 pass banked this same trap; it is now confirmed as a
  *setting on real data*, not an emulation artifact.)
- **`computer` clicks: a `ref` click and a `coordinate` click use different
  spaces.** `coordinate` is the last screenshot's frame (here 800x500 for a
  1512x945 viewport, divisor 1.89); a `ref` click reports page pixels. A
  hand-converted coordinate that is 12 px off in frame space is ~23 px off on
  the page and misses a 40 px button silently — the click "succeeds" and nothing
  happens. Prefer `find` → `ref`, or read the element's own rect and divide.
- **Every open workspace tab has its own composer and its own Send button.**
  `find` returned four `Send message` buttons; three belonged to background
  tabs with zero-size rects. Filter by `getBoundingClientRect().width > 0`
  before clicking (the standing "probe the visible tab, not the first" note,
  which also applies to buttons, not just composers).

- **NANOGPT's stored key is DEAD on this copy** — every NANOGPT call answers
  `HTTP 401: {"error":{"message":"Invalid session","type":"invalid_api_key",...}}`.
  It is the instance's *default* profile, so this shapes the whole walk:
  seats fall over rather than answering. **Environmental, not a v5 defect** (the
  key was rotated after the data was written). OPENAI and DEEPSEEK both work.
  It bought three free proofs on the way: the cheap-LLM chain walking
  `[CheapLLM] Task failed` → `Retrying task with a stand-in` (`trigger: "auth"`)
  → `Stand-in answered` on DEEPSEEK for `derive-scene-context`,
  `resolve-character-appearances` and `craft-story-background-prompt` — **with
  the `Task failed` warn firing BEFORE the chain**, which is the ordering a §3
  review corrected a few rounds ago, now proven live.
- **DeepSeek's raw `DSML` tool-call markup persisting as message text is
  pre-existing and v4-faithful — NOT a finding.** The understudy's reply in H1b
  came back as `<｜｜DSML｜｜ calls>…` text rather than parsed tool calls, which
  looked like a cross-provider failover defect (and sits next to the banked
  "the tool loops re-stream with a `base_provider.model` never refreshed after a
  cross-provider failover" item). The free discriminator settles it: the
  instance holds **five** messages containing `DSML`, and **four predate this
  walk** (2026-08-26 ×1, 2026-07-03 ×3) — v4 wrote them. Both apps persist the
  markup. A candidate upstream nicety at most.

- **Two components share the selector `qt-character-system-prompts-tab`** —
  `screens/characters/view/tabs/` (v4's read-only `SystemPromptsTab.tsx`) and
  `screens/characters/edit/` (which mounts Subprompts + Progressions). The
  workspace's character **detail** tab renders the VIEW one, so looking for the
  Progressions card there finds nothing and looks exactly like a wiring gap.
  It is not: editing lives at `/characters/:id/edit?tab=system-prompts`, which
  is v4's mount point too. Check which variant is mounted before filing.
- **`characterList` does not project `metadata`.** A population sweep for
  progressions over the list payload reports **zero** even when characters carry
  them; `characterGet` (or the vault) is the only honest source. This produced a
  false "v4 has never used this feature" reading that would have thrown away the
  walk's best comparand.
- **Rebuilding the SPA mid-walk invalidates the open tab's chunk hashes.** After
  `npm run build`, the loaded page keeps requesting chunks that no longer exist
  (404s in the console) and newly-added components simply never appear. `grep`
  the served `dist/` for a selector to prove the bundle has it, then
  `location.reload()` — the pane's `Cmd+Shift+R` does not reload.
- **A hidden Browser pane blocks screenshots AND clicks** ("the Browser pane is
  not displayed, so the page is not compositing frames"), but `javascript_tool`,
  `find` and `read_page` keep working. Drive with `element.click()` and assert on
  content when the pane is collapsed.

---

## §5 Outcome

**30 of the walk's rows ran, and every one PASSED** — one of them (B1) passing
the behaviour while exposing finding **#118**, which was fixed and committed.
**12 rows not run** (each with its reason in the row) and **6 deferred to the
human** for image spend or a posed provider refusal, alongside the 4 standing
human-only 💸 items.

Two sessions: the first ran 17 rows and found #118; after the human refreshed
the NanoGPT key, the second ran 13 more — the rows the dead key had blocked
(B2), both long-pole watchdog arms (H2, H4), the lock reclaim (I1/I2), and the
cheap API batch — **and corrected one of this walk's own claims** (C3, below).

**Both headline 💸 items are discharged, and two more with them:**

| 💸 item | round | outcome |
|---|---|---|
| A real stalled provider; the turn fails inside 240 s and the understudy answers rather than hanging | `ffb6b3119` | ⭐ **DISCHARGED** (H1 + H1b) |
| A New Chat opens its scripted greeting inside 90 s with `Greeting abandoned` | `ffb6b3119` | ⭐ **DISCHARGED** (H3) — at `elapsed_ms: 90002` |
| The avatar cache's first HIT on a configuration already worn | `31436bae4` | ⭐ **DISCHARGED** (C1) — three hits, zero spend |
| A real failover showing the persisted trail and the badge | `78b381a96` | ⭐ **DISCHARGED twice** — A4 renders **v4's** trail, H1b writes and renders **v5's** |
| The transcript's live re-read | `31436bae4` | **DISCHARGED** (A6) |
| A paused room holding a real send | `31436bae4` | **DISCHARGED** (B1) — and it produced #118 |
| The progressions card + the report on a real turn and in a greeting | `25f534c0b` | ⭐ **DISCHARGED** (F1 + F2 + F3) — against v4's own entry |
| A REAL rehearsal with its `VOICE_REWRITE` row | `f4ad2c8d1` | **DISCHARGED** (D2) |
| The gallery roll on real Friday data | `78b381a96` | **DISCHARGED** (E1) |
| The gallery route on a failed chat read answering 404 | `cc65d6bfc` | **DISCHARGED** (E6, the 404 half) |

**The four cross-app proofs** — where v5 was judged against bytes v4 wrote on
this instance, which is the strongest evidence a port can get:

1. The avatar-roll collapse ledger (v4 ran it 2026-09-11; v5 boots and writes
   nothing, 187 rows byte-identical).
2. The bug-132 placeholder heal (v4 ran it 2026-09-09; same).
3. The route-trail badge rendering v4's own two failover rows verbatim.
4. Abigail's pregnancy progression — v4 wrote it 2026-09-08, v5 renders it in
   the card and computes it into the prompt, to the day.

**Still owed (human, cost or judgment):** the Brahma deep query,
memory-dedup + conversation-summary regeneration, finding #101 (NanoGPT prompt
caching cost), the bug-133 reroute trio (G1–G3), Regenerate-avatar (C2) and a
chat-scoped `generate_image` (E5).

**Cheapest unrun rows, if someone picks this up:** A7 (the two-tab transcript
re-read), B3/B4, C4/C5 (the Avatar Rolls SPA section), D3/D6/D7, E4, F4's Carina
arm, F5/F6 (the Pascal `progress` family in the Workbench).

**Filed upstream as v4 bug 143** (C3): the 10 duplicate `generationKey` groups
are all same-character and byte-identical in model+prompt, and all 20 rows
predate v4's 2026-09-11 collapse — survivors, not re-accumulation. **Seven are
the migration's deliberate protected-portrait branch**; three are an
unprotected, unreferenced older row with a live blob that its victim loop should
have deleted. Low severity, no read affected (both apps sort newest-first and
verify character + blob). v4 commit `064ba85df`.

- **The server logs at `info` by default, so every `debug`-level comparand is
  invisible.** G4's three `[MemoryGate]` / `[Memories API]` lines are all
  `debug`; the API answered correctly and `combined.log` showed nothing, which
  reads exactly like a dropped log line. Relaunch with `RUST_LOG=debug` before
  concluding a debug-level sentence is missing. (`tracing_filter_directive(None)`
  is pinned to `"info"` in `quilltap-web/src/lib.rs`.)
- **A connection profile's `baseUrl` cannot carry a query string** through the
  SDK's URL join, so a posed endpoint needs an env knob rather than
  `?chunks=2` — which is why `harness/tools/stall-server.py` gained
  `QT_STALL_CHUNKS`.

