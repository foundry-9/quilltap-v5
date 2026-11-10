# Survey D — the Angular SPA (`apps/web`) and its Playwright e2e

Survey of v5 `main` @ `8ad1a0640`, against v4 at the pin `624d0d25c`. Read-only: no build, no test, no Playwright run. Every "probable" verdict on a flaky beat below is from reading the spec and the round records, NOT from a trace — the lane that takes it must capture a full-suite trace first.

Orders covered: P4.D344, D346, D352, D353, D357, D358, D359, D383, D384, D385, D386, D387, D388, P4.173. Named items: the `ThemeService` tokens gap, D387 Tier 2 item 23, D386 items 8 / 9 / 17, the lazy-`queryFn` audit, the four red-in-suite beats + the Salon-streaming cluster, the union axe re-baseline, W10 `R4_GUARD_BEYOND_V4`, the Core-whisper card, D352 Tier 2 items 1 / 2 + H-1 / H-4, `salon-custom-tools-flow:46`.

EXCLUDED (another survey owns it whole): the Storyteller live run fold. Files it will touch, so lanes stay disjoint: `apps/web/src/app/screens/salon/salon-conversation.ts` (+ `.spec.ts`), `core/chat-stream.reducer.ts`, `chat/message-list.ts`, `chat/streaming-message.ts`, `chat/storyteller-board.ts`, `e2e/salon-storyteller-flow.spec.ts`. **No lane proposed here edits any of them** (four latent lazy-`queryFn` sites in `salon-conversation.ts` are deliberately left out — §A, rows marked).

**Counts (by heading):**

| Status | Count |
|---|---|
| OPEN | 16 headings (A-1…A-4, B-1, B-3, B-5, C-1, C-2, C-3, D-2, D-4, D-7, E-2, E-3, E-6); four are optional, one (C-3) is recommended OUT |
| CLOSED (by measurement) | 5 headings (B-2, C-4, D-1, D-9, E-7) + 2 rows of §F |
| NO-ACTION | 6 headings (D-3, D-5, D-6, D-8, E-4, E-5) + 7 rows of §F |
| NEEDS-RULING | 1 (B-4), plus one planner decision (A-1 on a rewritten surface) |
| DEFER-TO-CATCH-UP | 1 heading (B-6) + 2 groups of latent rows (§G) |

Post-pin churn measured with `git diff --stat 624d0d25c HEAD -- <path>` on the v4 repo (history only; no checkout):

| v4 surface | Post-pin diff | Consequence |
|---|---|---|
| `lib/themes/`, `components/providers/theme/`, `theme-style-injector.tsx` | none | the `ThemeService` fix is safe |
| `components/document-browser/`, `components/files/FilePreview/` | none | D387 item 23 is safe |
| `components/item-browser/` | 1 file, +4 / −1 (`ItemActionMenu.tsx`) | D386 items 8 / 9 are not rewritten |
| `components/wardrobe/wardrobe-control-dialog.tsx` | +37 / −2 over 3 commits (`0d73a4aee`, `aefbdde05`, `fee1bd63d`) | D386 item 8 would be re-touched at the catch-up |
| `app/aurora/` | 60 files, +4,130 / −5,268 (`7624332c0`) | character view / edit tabs are rewritten |
| `app/aurora/AuroraView.tsx` (the list) | +7 / −7 | the character-card fix is safe |
| `components/settings/` | `f1386cebd` + 4 more | the Core-whisper card's home (`TemplatesPromptsTabContent.tsx`) is deleted |
| `components/settings/roleplay-templates/`, `core-whisper/` | none | the components themselves stand |

---

## A. The lazy-`queryFn` audit (the unchecked follow-up of `5ff2d0f7f`)

**Method.** A script walked every `injectQuery` / `injectInfiniteQuery` / `queryOptions` / `fetchQuery` / `ensureQueryData` call in `apps/web/src` (non-spec): 172 option blocks in 123 files. For each it extracted the `queryKey` and `queryFn` expressions and listed every zero-argument call (`this.x()`, `x()`) inside `queryFn`. 68 blocks carry one. The other 104 read nothing lazily (constants, or ids captured in the options factory as `chat-composer.ts:937,945` and the two `5ff2d0f7f` files do). The three raw `QueryObserver` sites (`editor/mentions/mention-source.ts:52`, `editor/slash/slash-command-host.ts:113`, `brahma/brahma-console.service.ts:68`) take a caller's closure over constants — clean.

**No site reads a signal that is absent from its key** once derived keys are resolved (`memory-list.ts:312-319` `filters()`, `custom-tool-presets.ts:199` `presetsKey()`, `chat-gallery.api.ts:78`, `subprompts.api.ts:153`, `character-progressions.api.ts:99`, `chat-scenario-control.ts:270` `characterIdsKey()` all derive from the same signals the `queryFn` reads).

**The `5ff2d0f7f` mechanism, restated so the verdicts are checkable.** A TanStack `Query` keeps the last `options` an observer gave it. An invalidation refetch (`query.fetch()` with no options) runs that stored `queryFn`. So when observer A sits on key K1, then its signal moves it to K2 while the component stays alive, K1 still holds A's closure — and a lazy `this.x()` inside it now reads K2's value. If another observer B keeps K1 active, the next invalidation fills K1 with K2's data. Three conditions: (1) the read is lazy, (2) the signal can move in a LIVE instance, (3) another observer shares the old key. Workspace tabs are keyed by `tab.id` with immutable payloads and kept alive (`workspace/chrome/workspace-host.ts:127`), which makes (3) common for global keys and makes (2) false for per-tab ids.

**Verdict classes.**
- **HIT (shared key + movable signal)** — all three conditions hold. Fix: capture in the options factory, as `5ff2d0f7f` did.
- **HIT (seed in `queryFn`)** — the `linkedSignal` class (unify-236 E1).
- **LATENT-MOVABLE** — (1) and (2) hold; (3) is false today. One new observer on the key makes it a HIT. Cheap to harden.
- **LATENT-FIXED** — only (1) holds. Harmless; no edit recommended (40 sites, most on surfaces v4 rewrote past the pin).
- **NOT-A-SIGNAL** — the script's zero-arg match is a method call.

### A-1 — three forms seeded inside a `queryFn`   [OPEN — one is a data-loss class]

- **`screens/characters/edit/appearance-tab.ts:333-340`** and **`screens/characters/view/tabs/character-appearance-tab.ts:153-160`**: both run `this.guidelinesDraft.set(content)` / `this.draft.set(content)` inside the `queryFn`, and both use the SAME key `characterKeys.depiction(id)`. `staleTime` is 5 s (`app.config.ts:16`). Open one tab, then the other (or leave and re-enter the same tab) inside 5 s: the cached entry is fresh, the `queryFn` never runs, the draft stays `''`, the editor mounts EMPTY (`edit/appearance-tab.ts:256-263` gates only on `isPending()`), and Save is enabled with no dirty gate (`:266-269`; `saveGuidelines` `:413-421` writes `content: this.guidelinesDraft()`). **A Save then writes blank depiction guidelines over the stored file.** This is exactly unify-236's E1, which fixed the physical-description form 40 lines above (`:303-306`, `linkedSignal`) and left this one.
- **`ui/aesthetic-editor-field.ts:112-119`**: `this.content.set(loaded)` inside the `queryFn`, deliberately (class doc `:28-39`). Same stale-cache hole: a re-mount inside 5 s shows an empty editor. Lower severity — Save is gated on `dirty()` (`:79`) — but a user who then types saves their text over the stored file. Consumers: `screens/prospero/cards/project-aesthetic-field.ts:26`, `screens/settings/images/default-aesthetics-card.ts:32,39`.
- v4 sites @`624d0d25c`: `components/settings/AestheticEditorField.tsx:54-76` (a fetch in an effect, re-run per mount — v4 has no cache between mounts, so it always re-seeds); `app/aurora/[id]/edit/CharacterEditView.tsx:338-347`.
- Done: each draft is a `linkedSignal` over the query's `data()` (the `:303-306` shape); a red-first spec case per file — pre-fill the query cache, mount, assert the draft equals the cached text. The editor-must-not-mount-empty ordering (the class doc's reason) still holds: `linkedSignal` is set the moment `data()` exists.
- Differential: unit specs `appearance-tab.spec.ts`, `character-appearance-tab.spec.ts`, `aesthetic-editor-field.spec.ts` (the E1 case in `appearance-tab.spec.ts` is the template).
- Size: S (three files).
- ⚠ Surface: the two character tabs are on `7624332c0`'s rewritten surface (ledger §3 row `7624332c0`). **Recommend fixing now anyway** — ten lines each, and the defect writes blank user data. The planner may instead mark the two tabs DEFER and take only `aesthetic-editor-field.ts`.
- Trap: `linkedSignal` re-seeds when `data()` changes identity. TanStack's structural sharing keeps an equal string stable, so a post-save refetch that returns the saved text does not clobber a newer draft; a refetch that returns DIFFERENT text does (v4 does the same on each `fetchCharacter`).

### A-2 — six lazy reads on a shared key with a movable signal   [OPEN]

- **`screens/characters/list/characters-list.ts:357-360`** — `listFilter()` is `undefined` or `{archived:'include'}` (`:353-355`). With Show Archived off the key is `characterKeys.list()` = `['characters','list']` (`characters.api.ts:48-49`), shared with seven other observers (`chat/chat-composer.ts:711`, `chat/cast/add-character-dialog.ts:422`, `chat/post-office/compose-mail-dialog.ts:220`, `chat/post-office/insert-announcement-dialog.ts:468`, `chat/tools/search-replace-modal.ts:510`, `screens/prospero/cards/project-characters-card.ts:268`, `scenario-builder/save-scenario-dialog.ts:274`). Turn Show Archived on in a Characters tab mounted after a Salon tab, then save any character (`characters.list()` is invalidated): the unfiltered entry is refetched WITH archived characters, and the composer's mention list and the add-character dialog offer them.
- **`screens/salon/salon-list.ts:137-148`** — `effectiveIncludeAutonomous()`; the `false` key equals `chat/merge-conversation-modal.ts:288`'s. `chatKeys.all` is invalidated by every chat event (`core/realtime-topic-map.ts:58,68`).
- **`chat/sidebar/chat-scenario-control.ts:272-296`** — four tiers, all on `showArchived()`; every mounted Salon tab's sidebar shares `scenarioKeys.general(false)`.
- v4: no counterpart defect — React Query's `useQuery` re-registers `queryFn` per render with the render's own captured values (`hooks/useCharacters.ts`, `components/chat/ChatScenarioControl.tsx`).
- Done: capture each signal in the options factory (`const filter = this.listFilter(); return { queryKey: …(filter), queryFn: () => fetch(core, filter) }`), with one red-first spec per component in `5ff2d0f7f`'s shape (`document-browser/document-dialogs.spec.ts:275`, "after a cross-store pick, the SOURCE store's listing query still fetches the source" — two observers on one key, move one, invalidate, assert the old key's data).
- Size: S (three files + specs).
- Trap: `salon-list.ts` and `chat-scenario-control.ts` are Salon-adjacent but are NOT in the Storyteller lane's file list.

### A-3 — thirteen LATENT-MOVABLE sites   [OPEN — optional hardening]

Listed in the table. Same one-line capture. Recommend taking the ones in files a lane already owns (`document-viewer.ts` with D387-23; `custom-tool-presets.ts`, `merge-conversation-modal.ts`, `compose-mail-dialog.ts`, `memory-list.ts`, `scenario-browser-dialog.ts`, `document-links-dialog.ts`, `save-scenario-dialog.ts`, `project-store-browser.ts` with A-2) and leaving `conversations-tab.ts`, `description-source-step.ts` (Aurora, `7624332c0`) and `profile-tag-editor.ts` (Foundry, `f1386cebd`) for the catch-up.

### A-4 — a census spec so the shape cannot return   [OPEN — optional, Tier 2]

- Shape: a source census in the `ui/floating-layer-guards.spec.ts` pattern (`node:fs` walk, an allowlist table, a self-test): every `queryFn` body that contains a zero-arg `this.<signal>()` read must be in an allowlist that names the reason (LATENT-FIXED, per-instance id). New sites fail until captured or listed.
- New file: `apps/web/src/app/core/query-fn-capture.census.spec.ts`. No `package.json` edit (it is a vitest spec).
- Size: S–M. Trap: the allowlist starts at 40 rows; the matcher must not count `this.core.x()` method calls (six sites).

### The 68 sites

| # | Site (`apps/web/src/app/…`) | Lazy reads inside `queryFn` | Verdict | Surface rewritten past the pin |
|---|---|---|---|---|
| 1 | `chat/chat-cost-summary.ts:70` | `this.chatId()` | LATENT-FIXED — the id is a per-instance input (workspace tabs are keyed by `tab.id`, payload immutable) |  |
| 2 | `chat/chat-gallery.api.ts:80` | `chatId()` | LATENT-FIXED — the id is a per-instance input (workspace tabs are keyed by `tab.id`, payload immutable) |  |
| 3 | `chat/custom-tool-intervention-dialog.ts:950` | `this.chatId()` | LATENT-FIXED — the id is a per-instance input (workspace tabs are keyed by `tab.id`, payload immutable) |  |
| 4 | `chat/custom-tool-presets.ts:207` | `this.asCharacterId()`, `this.chatId()`, `this.toolName()` | LATENT-MOVABLE — `asCharacterId` / `toolName` (in the key via `presetsKey()`); sole observer of its key today |  |
| 5 | `chat/custom-tools-popup.ts:528` | `this.chatId()` | LATENT-FIXED — the id is a per-instance input (workspace tabs are keyed by `tab.id`, payload immutable) |  |
| 6 | `chat/library-picker/library-file-picker-modal.ts:338` | `this.chatId()` | LATENT-FIXED — the id is a per-instance input (workspace tabs are keyed by `tab.id`, payload immutable) |  |
| 7 | `chat/library-picker/library-file-picker-modal.ts:342` | `this.chatId()` | LATENT-FIXED — the id is a per-instance input (workspace tabs are keyed by `tab.id`, payload immutable) |  |
| 8 | `chat/merge-conversation-modal.ts:296` | `this.sourceId()` | LATENT-MOVABLE — `sourceId` picker; sole observer of its key today |  |
| 9 | `chat/pending-inform-chips.ts:89` | `this.chatId()` | LATENT-FIXED — the id is a per-instance input (workspace tabs are keyed by `tab.id`, payload immutable) |  |
| 10 | `chat/post-office/compose-mail-dialog.ts:245` | `this.chatId()`, `this.fromCharacterId()` | LATENT-MOVABLE — `fromCharacterId` picker; sole observer of its key today |  |
| 11 | `chat/sidebar/chat-scenario-control.ts:272` | `this.showArchived()` | **HIT (shared key + movable signal)** — `showArchived` toggle; `scenarioKeys.general(false)` is shared by every mounted Salon tab (keep-alive) |  |
| 12 | `chat/sidebar/chat-scenario-control.ts:279` | `this.projectId()`, `this.showArchived()` | **HIT (shared key + movable signal)** — same toggle; `scenarioKeys.project(id,false)` shared by Salon tabs of one project |  |
| 13 | `chat/sidebar/chat-scenario-control.ts:286` | `this.llmCharacterIds()`, `this.showArchived()` | **HIT (shared key + movable signal)** — same toggle; group tier |  |
| 14 | `chat/sidebar/chat-scenario-control.ts:293` | `this.showArchived()`, `this.singleLlmCharacterId()` | **HIT (shared key + movable signal)** — same toggle; character tier |  |
| 15 | `chat/sidebar/chat-scenario-control.ts:307` | `this.chatId()` | LATENT-FIXED — the id is a per-instance input (workspace tabs are keyed by `tab.id`, payload immutable) |  |
| 16 | `chat/storyteller-board.ts:294` | `this.chatId()` | LATENT-FIXED — the id is a per-instance input (workspace tabs are keyed by `tab.id`, payload immutable) |  |
| 17 | `chat/tools/chat-tool-settings-modal.ts:120` | `this.chatId()` | LATENT-FIXED — the id is a per-instance input (workspace tabs are keyed by `tab.id`, payload immutable) |  |
| 18 | `chat/tools/run-tool-modal.ts:293` | `this.chatId()` | LATENT-FIXED — the id is a per-instance input (workspace tabs are keyed by `tab.id`, payload immutable) |  |
| 19 | `document-browser/document-links-dialog.ts:77` | `this.path()`, `this.storeId()` | LATENT-MOVABLE — `path()`; sole observer of its key today |  |
| 20 | `document-browser/document-viewer.ts:240` | `this.entry()` | LATENT-MOVABLE — `entry()` changes as the viewer is re-aimed; sole observer of its key today |  |
| 21 | `document-browser/document-viewer.ts:247` | `this.entry()` | LATENT-MOVABLE — `entry()`; sole observer of its key today |  |
| 22 | `document-browser/project-store-browser.ts:27` | `projectId()` | LATENT-MOVABLE — `projectId()`; sole observer of its key today |  |
| 23 | `help/help.service.ts:57` | — | NOT-A-SIGNAL — a `CoreClient` / api method call, no lazy state |  |
| 24 | `memory/memory-list.ts:321` | `this.characterId()`, `this.keystoneParam()`, `this.search()`, `this.sortBy()`, `this.sortOrder()`, `this.sourceParam()`, `this.tierParam()` | LATENT-MOVABLE — filters/search/sort (all in the key via `filters()`); sole observer of its key today |  |
| 25 | `progressions/character-progressions.api.ts:102` | `characterId()` | LATENT-FIXED — the id is a per-instance input (workspace tabs are keyed by `tab.id`, payload immutable) |  |
| 26 | `scenario-builder/save-scenario-dialog.ts:258` | `this.castKey()` | LATENT-MOVABLE — `castKey()`; sole observer of its key today |  |
| 27 | `scenario/scenario-browser-dialog.ts:270` | `this.showArchived()` | LATENT-MOVABLE — `showArchived`; sole observer of its key today |  |
| 28 | `screens/characters/edit/appearance-tab.ts:296` | `this.characterId()` | LATENT-FIXED — the id is a per-instance input (workspace tabs are keyed by `tab.id`, payload immutable) | `7624332c0` (Aurora one page) |
| 29 | `screens/characters/edit/appearance-tab.ts:333` | `this.characterId()` | **HIT (seed in `queryFn`)** | `7624332c0` (Aurora one page) |
| 30 | `screens/characters/edit/avatar-picker-modal.ts:82` | `this.characterId()` | LATENT-FIXED — the id is a per-instance input (workspace tabs are keyed by `tab.id`, payload immutable) | `7624332c0` (Aurora one page) |
| 31 | `screens/characters/edit/character-edit.ts:308` | `this.characterId()` | LATENT-FIXED — the id is a per-instance input (workspace tabs are keyed by `tab.id`, payload immutable) | `7624332c0` (Aurora one page) |
| 32 | `screens/characters/edit/llm-logs-section.ts:112` | `this.characterId()` | LATENT-FIXED — the id is a per-instance input (workspace tabs are keyed by `tab.id`, payload immutable) | `7624332c0` (Aurora one page) |
| 33 | `screens/characters/edit/system-prompts-tab.ts:199` | `this.characterId()` | LATENT-FIXED — the id is a per-instance input (workspace tabs are keyed by `tab.id`, payload immutable) | `7624332c0` (Aurora one page) |
| 34 | `screens/characters/generators/wizard/description-source-step.ts:340` | `this.characterId()` | LATENT-MOVABLE — `characterId` (wizard target); sole observer of its key today | `7624332c0` (Aurora one page) |
| 35 | `screens/characters/list/character-delete-dialog.ts:148` | `this.characterId()` | LATENT-FIXED — the id is a per-instance input (workspace tabs are keyed by `tab.id`, payload immutable) | `7624332c0` (Aurora one page) |
| 36 | `screens/characters/list/characters-list.ts:357` | `this.listFilter()` | **HIT (shared key + movable signal)** — `showArchived` toggle; leaves `characterKeys.list()` (8 other observers: composer mentions, add-character, compose-mail, insert-announcement, search-replace, project-characters-card, save-scenario) | `7624332c0` (Aurora one page) |
| 37 | `screens/characters/view/character-detail.ts:446` | `this.id()` | LATENT-FIXED — the id is a per-instance input (workspace tabs are keyed by `tab.id`, payload immutable) | `7624332c0` (Aurora one page) |
| 38 | `screens/characters/view/character-detail.ts:464` | `this.id()` | LATENT-FIXED — the id is a per-instance input (workspace tabs are keyed by `tab.id`, payload immutable) | `7624332c0` (Aurora one page) |
| 39 | `screens/characters/view/character-detail.ts:469` | `this.id()` | LATENT-FIXED — the id is a per-instance input (workspace tabs are keyed by `tab.id`, payload immutable) | `7624332c0` (Aurora one page) |
| 40 | `screens/characters/view/tabs/avatar-rolls-section.ts:246` | `this.characterId()` | LATENT-FIXED — the id is a per-instance input (workspace tabs are keyed by `tab.id`, payload immutable) | `7624332c0` (Aurora one page) |
| 41 | `screens/characters/view/tabs/character-appearance-tab.ts:153` | `this.characterId()` | **HIT (seed in `queryFn`)** | `7624332c0` (Aurora one page) |
| 42 | `screens/characters/view/tabs/conversations-tab.ts:144` | `this.characterId()`, `this.search()` | LATENT-MOVABLE — `search`; sole observer of its key today | `7624332c0` (Aurora one page) |
| 43 | `screens/characters/view/tabs/gallery-tab.ts:290` | `this.characterId()` | LATENT-FIXED — the id is a per-instance input (workspace tabs are keyed by `tab.id`, payload immutable) | `7624332c0` (Aurora one page) |
| 44 | `screens/characters/view/tabs/gallery-tab.ts:297` | `this.characterId()` | LATENT-FIXED — the id is a per-instance input (workspace tabs are keyed by `tab.id`, payload immutable) | `7624332c0` (Aurora one page) |
| 45 | `screens/characters/view/tabs/tags-tab.ts:138` | `this.characterId()` | LATENT-FIXED — the id is a per-instance input (workspace tabs are keyed by `tab.id`, payload immutable) | `7624332c0` (Aurora one page) |
| 46 | `screens/characters/view/tabs/wardrobe-pictures-section.ts:204` | `this.characterId()` | LATENT-FIXED — the id is a per-instance input (workspace tabs are keyed by `tab.id`, payload immutable) | `7624332c0` (Aurora one page) |
| 47 | `screens/files/files-browser.ts:273` | — | NOT-A-SIGNAL — a `CoreClient` / api method call, no lazy state |  |
| 48 | `screens/groups/group-editor.ts:262` | `this.id()` | LATENT-FIXED — the id is a per-instance input (workspace tabs are keyed by `tab.id`, payload immutable) |  |
| 49 | `screens/groups/group-editor.ts:267` | `this.id()` | LATENT-FIXED — the id is a per-instance input (workspace tabs are keyed by `tab.id`, payload immutable) |  |
| 50 | `screens/groups/group-editor.ts:272` | `this.id()` | LATENT-FIXED — the id is a per-instance input (workspace tabs are keyed by `tab.id`, payload immutable) |  |
| 51 | `screens/prospero/cards/project-files-card.ts:119` | `this.projectId()` | LATENT-FIXED — the id is a per-instance input (workspace tabs are keyed by `tab.id`, payload immutable) |  |
| 52 | `screens/prospero/project-detail.ts:224` | `this.id()` | LATENT-FIXED — the id is a per-instance input (workspace tabs are keyed by `tab.id`, payload immutable) |  |
| 53 | `screens/prospero/project-detail.ts:229` | `this.id()` | LATENT-FIXED — the id is a per-instance input (workspace tabs are keyed by `tab.id`, payload immutable) |  |
| 54 | `screens/prospero/project-detail.ts:257` | `this.id()` | LATENT-FIXED — the id is a per-instance input (workspace tabs are keyed by `tab.id`, payload immutable) |  |
| 55 | `screens/salon/salon-conversation.ts:1473` | `this.chatId()` | LATENT-FIXED — the id is a per-instance input (workspace tabs are keyed by `tab.id`, payload immutable) |  |
| 56 | `screens/salon/salon-conversation.ts:1490` | `this.chatId()` | LATENT-FIXED — the id is a per-instance input (workspace tabs are keyed by `tab.id`, payload immutable) |  |
| 57 | `screens/salon/salon-conversation.ts:1590` | `this.chatId()` | LATENT-FIXED — the id is a per-instance input (workspace tabs are keyed by `tab.id`, payload immutable) |  |
| 58 | `screens/salon/salon-conversation.ts:1812` | `this.chatId()` | LATENT-FIXED — the id is a per-instance input (workspace tabs are keyed by `tab.id`, payload immutable) |  |
| 59 | `screens/salon/salon-list.ts:137` | `this.effectiveIncludeAutonomous()` | **HIT (shared key + movable signal)** — the Autonomous toggle; leaves `[...chatKeys.all,{includeAutonomous:false}]` (shared with `chat/merge-conversation-modal.ts:288`; `chatKeys.all` is invalidated by every chat event) |  |
| 60 | `screens/salon/salon-list.ts:155` | — | NOT-A-SIGNAL — a `CoreClient` / api method call, no lazy state |  |
| 61 | `screens/scriptorium/store-detail.ts:243` | `this.storeId()` | LATENT-FIXED — the id is a per-instance input (workspace tabs are keyed by `tab.id`, payload immutable) |  |
| 62 | `screens/settings/memory/conversation-summary-regenerate-card.ts:71` | — | NOT-A-SIGNAL — a `CoreClient` / api method call, no lazy state | `f1386cebd` (Foundry rebuilt) |
| 63 | `screens/settings/memory/embedding-profiles/embedding-profiles-card.ts:81` | — | NOT-A-SIGNAL — a `CoreClient` / api method call, no lazy state | `f1386cebd` (Foundry rebuilt) |
| 64 | `screens/settings/memory/embedding-profiles/embedding-profiles-card.ts:87` | — | NOT-A-SIGNAL — a `CoreClient` / api method call, no lazy state | `f1386cebd` (Foundry rebuilt) |
| 65 | `screens/settings/providers/profile-tag-editor.ts:144` | `this.profileId()` | LATENT-MOVABLE — `profileId` input (one editor re-aimed per profile); sole observer of its key today | `f1386cebd` (Foundry rebuilt) |
| 66 | `subprompts/subprompts.api.ts:155` | `characterId()` | LATENT-FIXED — the id is a per-instance input (workspace tabs are keyed by `tab.id`, payload immutable) |  |
| 67 | `ui/aesthetic-editor-field.ts:112` | `this.load()` | **HIT (seed in `queryFn`)** |  |
| 68 | `wardrobe/outfit-history-tab.ts:166` | `this.characterId()`, `this.chatId()` | LATENT-FIXED — the id is a per-instance input (workspace tabs are keyed by `tab.id`, payload immutable) |  |

The four `screens/salon/salon-conversation.ts` rows (`:1473`, `:1490`, `:1590`, `:1812`) are LATENT-FIXED and stay untouched: that file belongs to the Storyteller lane.

---

## B. Themes and accessibility

### B-1 — `ThemeService` never applies a pack's `tokens.json`   [OPEN]
- Source: P4.D383's header ("Survivor, explained") and the round record's follow-ups.
- v5 site: `apps/web/src/app/theme/theme.service.ts:191-211` (`applyToDom`) sets `data-theme`, the mode class, the pack's `styles.css` `<link>` (`:213-231`) and the `@font-face` rules (`:233-252`). It emits no `--theme-*` variable. The only v5 code that turns tokens into CSS is the preview's `screens/settings/appearance/scoped-theme-css.ts:147` (`generateScopedThemeCSS`, with the four var maps at `:31-118`), fed by a `fetch('/themes/<id>/tokens.json')` at `theme-preview-modal.ts:249`.
- v4 site @`624d0d25c`: `components/providers/theme-style-injector.tsx:113-118` (`themeTokensToCSS(tokens)`), assembled at `:121-147` in the order fonts → variables → icon overrides → component overrides, in one `<style id="quilltap-theme-variables">` (`:154-161`). `lib/themes/utils.ts:210-250` is `themeTokensToCSS`: light colours + typography + spacing + effects under `:root`, dark colours under `.dark`.
- Measured: earl-grey's `tokens.json` dark `mutedForeground` is `hsl(0 0% 70.6%)`; its `styles.css` never sets `--theme-muted-foreground`; so v5 paints the default sheet's `hsl(225 8% 58%)` (#8b909c on #2f2f2f, 4.18:1). 23 rows of `e2e/a11y/known-violations.json` are owned `FOLLOW-UP: v5 ThemeService never applies…` for exactly this.
- Done: the service injects v4's variable sheet for the active pack, BEFORE the pack's stylesheet in cascade order (v4 puts variables ahead of overrides), and removes it for the default theme. A tier-1 corpus pins `themeTokensToCSS` bytes for the six packs against v4's real function (recorder under `apps/web/oracle/`, the `item-browser.recorder.ts` recipe). Then one `--project=a11y` run with `QT_AXE_REBASELINE=1`: expect the 23 earl-grey rows to clear; every NEW row is a finding to own.
- Differential: `theme/theme.service.spec.ts` (7 cases today), `theme/theme-contrast.spec.ts` + `theme-contrast.residues.json`, `e2e/a11y/axe-sweep.spec.ts`.
- Files: `theme/theme.service.ts` (+ spec), `theme/bundled-themes.ts` (if the tokens are embedded at build time rather than fetched), a new `theme/theme-tokens-css.ts` holding the var maps, `screens/settings/appearance/scoped-theme-css.ts` (re-export the maps from the new module — one source), `theme/theme-contrast.residues.json`, `e2e/a11y/known-violations.json`.
- Size: M.
- Traps:
  - **Fetch vs embed.** v4 server-renders the sheet; a runtime `fetch` of `tokens.json` paints one frame in default colours on every boot. Embedding the six token sets in `bundled-themes.ts` avoids the flash; its header says the file is generated, and the generator lives only in the P4.5 commit.
  - **It moves every pack, not one.** Typography / spacing / effects tokens start applying too (font sizes, radii). That is v4's behaviour, but `theme-contrast.spec.ts`'s 52 cases and its residues file were measured without them — re-measure, do not hand-edit.
  - Madman's Box is dark by decree (`theme.service.ts:199-202`): its `.dark` palette must apply although `supportsDarkMode` is false. v4 emits both blocks unconditionally; do the same.
  - v4's `iconOverridesCss` arm (`theme-style-injector.tsx:104-110`) has no v5 twin either; madmans-box carries its overrides in `styles.css` (`:713-727`), so nothing is missing today. Record, do not port.

### B-2 — the union axe re-baseline (P4.D358 H-out-14's last step)   [CLOSED by measurement; re-opens inside B-1]
- `known-violations.json` holds 509 rows (420 queue-badge `RULE v4-faithful` + 66 ParticipantCard + 23 `FOLLOW-UP`), the count P4.D383's lane wrote in `88daa9ef2`. The union's final full run reported the axe sweep red ONLY after the destructive specs and green by file alone — so the 509-row baseline holds on the union tree with no new row. Nothing to re-baseline until B-1 changes the paint.

### B-3 — the axe sweep runs after the destructive specs   [OPEN — an ordering defect with a cheap fix]
- `playwright.config.ts:28-31` declares two projects; `package.json:13` runs `playwright test` with no `--project`, so a full run executes `a11y` after all of `chromium`, on the SAME global-setup server, after `zz-bulk-replace-destructive`, `zz-delete-all-destructive` and `zzz-restore-destructive` have rewritten the instance. The sweep's keys are `pack|mode|page|rule|target`; a changed page yields new targets. The config comment (`:26-27`) says the `a11y` project is optional and "the default `npm run e2e` set is unchanged" — the config does not do what its comment says.
- Done (pick one, both S): (a) `"e2e": "playwright test --project=chromium"` plus `"e2e:a11y": "playwright test --project=a11y"`, run separately on a fresh instance; or (b) keep one invocation and list `a11y` FIRST in `projects`. (a) matches the comment and the gate records ("`--project=a11y` 2 passed").
- Files: `apps/web/playwright.config.ts`, `apps/web/package.json`, `apps/web/README.md` (the e2e section).
- Size: S. Trap: `/unify`'s gate text says "full Playwright"; the round record must then name both invocations.

### B-4 — W10 `R4_GUARD_BEYOND_V4`   [NEEDS-RULING]
- State: the header's "no divergence record exists in the tree" is no longer true. `ui/floating-layer-guards.spec.ts:185-200` records it as "a RULED divergence (the round's R-4: guard EVERY document-level dismiss site) … revisit if the human rules R-4 back to v4's exact set". P4.D386 removed `scenario-row.ts` from the list (it is now guarded as v4's `useClickOutside` is). **Twelve hosts remain:**

  | # | v5 host | v4 listener (unguarded) |
  |---|---|---|
  | 1 | `ui/tooltip.ts` | `components/ui/Tooltip.tsx:164-172` |
  | 2 | `workspace/chrome/workspace-host.ts` | `WorkspaceLinkInterceptor` |
  | 3 | `chat/sidebar/chat-sidebar.ts` | the sidebar's keydown |
  | 4 | `images/image-navigation.ts` | image arrows |
  | 5 | `images/image-modal.ts` | the image modal's Escape |
  | 6 | `almanack/almanack-report-dialog.ts` | the report dialog |
  | 7 | `ui/alert-modal.service.ts` | the alert modal |
  | 8 | `chat/absent-mentions/mention-chip-host.ts` | the chip card |
  | 9 | `chat/diagrams/diagram-lightbox.ts` | `DiagramLightbox.tsx` |
  | 10 | `screens/files/file-preview-modal.ts` | `FilePreviewActions.tsx` |
  | 11 | `ui/slide-over-panel.ts` | `SlideOverPanel.tsx:41-50` |
  | 12 | the Salon inspector shortcut (`screens/salon/salon-conversation.ts`) | the inspector keydown |

- What the divergence does: with the floating Brahma Console open over one of these surfaces, an Escape or click INSIDE the console leaves the surface beneath open; in v4 it closes it.
- Options: **(A) ratify** — name the constant in the round record as a ruled divergence, no code change; **(B) remove** — delete twelve guards and twelve census rows, re-pin `GUARDED`'s count (`:228`), accept v4's "typing Escape in the console closes the lightbox behind it".
- Recommendation: **(A) ratify.** (B) is v4's behaviour but reads as a v4 oversight (v4 guards the same event in `useClickOutside` and `BaseModal`); row 12 lives in the Storyteller lane's file; and row 1 is about to be rewritten — `43623cca3` (ledger §3) replaces `Tooltip.tsx`'s listeners with a `pointermove` close-grace, so the tooltip guard must be re-measured at the catch-up either way. If ratified, consider filing the v4 gap (the human's go-ahead first).
- Files if (A): none in source; the round record + `phase-4.md`. If (B): the twelve hosts + `ui/floating-layer-guards.spec.ts`.

### B-5 — character cards are not keyboard-operable in the tabbed shell   [OPEN]
- Source: P4.D358 lane record, Tier 2.3 ("recorded, not fixed").
- v5 site: `screens/characters/list/character-card.ts:31` and `:151-152` bind `[routerLink]="inTab() ? null : […]"`. A null `routerLink` renders an `<a>` with no `href`: not a Tab stop, not activated by Enter. `inTab` is true in the workspace, the DEFAULT shell. The two other null-`routerLink` sites are in `screens/characters/list/characters-list.ts`.
- v4 site @`624d0d25c`: `app/aurora/AuroraView.tsx:530` (`href={`/aurora/${character.id}/view`}`) and `:615` (`…?action=chat`), with the tab-mode click handlers at `:509-521` calling `preventDefault`. v4 keeps the href.
- Done: keep the `routerLink` in both modes; in tab mode the existing `onDrillView` / `onDrillChat` already `preventDefault()` (`:233-236`). Spec: both anchors carry an `href` with `inTab` true, and Enter on the name anchor emits `view`.
- Differential: `character-card.spec.ts`; `e2e/characters-flow.spec.ts` (7 beats).
- Size: S. Trap: with an `href` present the workspace's link interceptor (`workspace/chrome/link-interceptor.ts`) may also claim the click — assert ONE drill, not two. Surface: the list page is +7 / −7 past the pin; not a DEFER.

### B-6 — the SPA Core-whisper card   [DEFER-TO-CATCH-UP]
- v5: `screens/settings/templates/templates-tab.ts:11` names the deferral; the server half landed (P4.D356, `coreWhisper` PUT arm).
- v4 @`624d0d25c`: `components/settings/core-whisper/CoreWhisperSection.tsx` (204 lines), mounted by `components/settings/tabs/TemplatesPromptsTabContent.tsx`.
- Why DEFER: `f1386cebd` (ledger §3) deletes the eight `*TabContent` files and re-homes every card through `lib/settings/registry.ts`. The section component is untouched past the pin, but its placement, deep link and help link are all rewritten. Port it with the Foundry catch-up.

---

## C. Documents and the item browser

### C-1 — P4.D387 Tier 2 item 23: Markdown in the store viewer   [OPEN]
- v5 site: `document-browser/document-viewer.ts:255-259` maps `markdown` onto the `text` arm, which renders `<pre class="qt-file-preview-code"><code>{{ text.data() }}</code></pre>` (`:189`). The class doc records the deferral (`:102-104`).
- v4 site @`624d0d25c`: `components/document-browser/DocumentViewer.tsx:104-107` hands `markdown` to `components/files/FilePreview/FilePreviewText.tsx` (627 lines: `react-markdown` + `remark-gfm` + `remark-math` + `rehype-katex`, Prism highlighting, frontmatter parsing `:346-356`, wikilinks `:275-304`).
- Done: the `markdown` arm renders through `chat/render/markdown-renderer.ts:339` `renderMarkdownToHtml` under Angular's sanitizer, as the `.docx` arm already does (`:157`, `[innerHTML]`). The deferral narrows to syntax highlighting, wikilinks and frontmatter display, each recorded by name. A small corpus (10–20 documents) records what still differs from v4's render.
- Differential: `document-browser/document-viewer.spec.ts`; `e2e/document-store-browser-flow.spec.ts`.
- Size: S–M.
- Traps:
  - **`renderMarkdownToHtml` cannot be asked for "no roleplay patterns" today**: `:349` reads `renderingPatterns.length > 0 ? renderingPatterns : DEFAULT_RENDERING_PATTERNS`, so passing `[]` paints the DEFAULT roleplay spans over a document. The order needs either a new option on the renderer (a shared chat-rendering file — a meeting point with any lane touching `chat/render/`) or a single no-op pattern. The dialogue pass has the same default (`:342`).
  - v4's viewer strips frontmatter before rendering (`FilePreviewText.tsx:346-356`); character vault files all carry it. Decide and pin: strip, or show raw.
  - Take A-3's capture at `:240-253` in the same change (the file is owned here).

### C-2 — P4.D386 item 9, the one behaviour gap: the Inventory keeps select mode after filing   [OPEN]
- v5 site: `inventory/inventory-folder-picker-dialog.ts:99-101` files and emits only `closed` (`:55`); its host `inventory/inventory-control-dialog.ts:194-200` binds only `(closed)`; select mode lives in the browser's `item-browser/folder-browser-state.ts:129-134` (read at `inventory/inventory-browser.ts:307`). Nothing clears it after a filing. The wardrobe does.
- v4 site @`624d0d25c`: `components/inventory/InventoryBrowser.tsx:269` `onFiled={() => setSelectMode(false)}` (the wardrobe's twin is `WardrobeBrowser.tsx:418`); `ItemFolderPickerDialog.tsx:76,111`.
- Done: the Inventory picker emits `filed`; the host forwards it; the browser clears select mode. One spec row each in `inventory-small-dialogs.spec.ts` and `inventory-browser.spec.ts`.
- Size: S. Trap: the picker is hosted by the control dialog, the select state by the browser inside it — the signal has to cross one component (the wardrobe routes it through its actions object; `inventory-item-actions.ts` is the twin).

### C-3 — P4.D386 items 8 and 9, the shared shells   [OPEN — recommend OUT of this round]
- Absent in v5: `item-browser/item-control-dialog-shell.ts` (v4 `ItemControlDialogShell.tsx`, 265 lines, with `createItemControlState`, the `KIND_COPY` table and `itemContainerLabel`), `item-browser/item-folder-picker-dialog.ts` (v4 142 lines), `item-browser/item-overlays.ts` (v4 `useItemOverlays.ts`, 79), `item-browser/item-image-viewer.ts` (v4 166). Still serving in their place: `wardrobe/wardrobe-control-dialog.ts`, `inventory/inventory-control-dialog.ts`, `wardrobe/wardrobe-folder-picker-dialog.ts`, `inventory/inventory-folder-picker-dialog.ts`, `wardrobe/wardrobe-item-overlays.ts`, `wardrobe/wardrobe-image-viewer.ts`, `inventory/inventory-image-viewer.ts`. Not transcribed: v4 `item-control-shared.test.tsx` (8 cases), `item-image-viewer.test.tsx` (3).
- This is a no-behaviour-change consolidation (the lane record: "none of these is a stub; the existing v5 code keeps serving"), size L, across two 600–1,400-line dialogs. `wardrobe-control-dialog.tsx` moves again past the pin (+37 / −2: `0d73a4aee`'s staged Wear again, `aefbdde05`'s pop-ups above the rail, `fee1bd63d`'s optimizer), so the wardrobe dialog would be re-opened at the catch-up.
- Recommendation: leave OUT; carry by name. If the planner wants it, it is one lane of its own and must keep `WARDROBE_VIEWER_ESCAPE_LAYERED` exactly (`wardrobe/wardrobe-item-editor.ts:590`, spec at `wardrobe-item-editor.spec.ts:891`, unedited) and `WardrobeDialogService.remountKey`.

### C-4 — P4.D386 Tier 2 item 17: the `aria-expanded` card-lift probe   [CLOSED]
- `item-browser/item-action-menu.spec.ts:170-183` asserts `aria-haspopup="menu"`, `aria-expanded="false"` closed and `"true"` open on the ⋮ button, and `item-folder-row.spec.ts:55` pins the `qt-card-interactive` row class. The `:has()` computed-style check stays a dogfood row (jsdom has no `:has()` layout), as the order says.

---

## D. Composer, templates, Pascal

### D-1 — P4.D352 H-1: the Novel patterns byte compare   [CLOSED by measurement]
- The 18 Novel rows of `chat/render/__fixtures__/markdown-fixtures.json` carry `renderingPatterns` with JSON md5 `e297b28f41310f17176ed100887c37f1` and `dialogueDetection` md5 `702bdceb2ca917efac602be6cebc5368` (compact separators, `ensure_ascii=False`). `crates/quilltap-core/src/services/builtin_templates.json`'s Novel row gives the same two md5s. Equal.

### D-2 — P4.D352 H-4: the SPA sentences against the three help pages   [OPEN — a measurement]
- The compare: the prose-mode toggle heading + body, the narration help, the `Unmarked Text` help, the asterisks label and the two wrap-checkbox labels in `screens/settings/templates/template-form.ts` and the composer settings card, against `help/chat-settings-composer.md`, `help/roleplay-templates.md`, `help/roleplay-templates-settings.md`.
- Both sides were transcribed from v4 at the pin, so this is a typo check. Done: record the compare in the lane record, or pin the sentences in a spec that reads the help files.
- Size: S. No v4 read needed beyond `components/settings/roleplay-templates/index.tsx:437-493`.

### D-3 — P4.D352 Tier 2 item 2: a narration row in the template preview   [NO-ACTION — v4 has none]
- The order said to measure first. Measured: v4's preview dialog at the pin (`components/settings/roleplay-templates/index.tsx`, the `previewTemplate &&` block) renders the template name, `Formatting Delimiters` and `LLM Prompt` — no narration row and no wrap flags. Adding one would be a v5-authored divergence. v5's `template-preview-modal.ts` has no narration row either. Close the item.

### D-4 — P4.D352 Tier 2 item 1: the own-instance Novel e2e   [OPEN — optional]
- Absent: `e2e/salon-novel-template-flow.spec.ts` (ports 4355 / 45353 still unused). It would prove the ASSISTANT bubble path and the streaming → settled hand-off for a Novel-marked model reply; today only the operator's bubble is proven live.
- Recipe: `e2e/salon-interrupt-flow.spec.ts:1-80` (own server), `startMockLlm(NOVEL_REPLY, 45353)`.
- Size: M. Trap (memory note `own-instance-e2e-fixture-choice`): the salon pair, not the pascal pair; UUID plants. Not on a rewritten surface.

### D-5 — P4.D352 Tier 2 item 3, the `Draft formatting instructions` button   [NO-ACTION — R-4 standing deferral]

### D-6 — P4.D352 C3 and the two rendering findings   [NO-ACTION here]
- C3 (`isProseTemplate` requires a literal `null`) is listed under another area ("Vault / templates / memory"). The two rendering findings are v4 filing candidates, not SPA work.

### D-7 — `salon-custom-tools-flow.spec.ts:46`   [OPEN]
- v5 site: `e2e/salon-custom-tools-flow.spec.ts:36-42` — `maybeUnlock` clicks Unlock and returns without waiting; `openChat`'s first line (`:46`) waits for the `Chats` heading with the default 5 s. A cold unlock takes longer.
- Done: `{ timeout: 15_000 }` on `:46` (the value its siblings use), and the same on the card wait at `:48`.
- Size: S. The identical shape is in `e2e/salon-thinking-indicator.spec.ts:36-42,52-54` — see E-3.

### D-8 — P4.D346 E3 (the hold dialog blocks Stop)   [NO-ACTION — v4-shared, a filing candidate owned by the Pascal area]

### D-9 — P4.D346's `PREEXISTING_PARAMETER_STRIP_DIVERGENCE`   [CLOSED]
- The unify-236 H4 note: "the grammar twin strips unknown parameter keys as v4".

---

## E. Playwright: the red-in-suite beats

None of these has a kept trace (`/tmp` holds no unify scratch; `apps/web/test-results/` is absent). Verdicts come from the specs and the round records. **A lane that takes E-2 / E-3 must first run the full suite ONCE with `E2E_KEEP_SERVER_LOG=<path>` (traces are already `retain-on-failure`) and read the failing step before editing.**

The common ground: 33 spec files drive the chat **"Group Expedition"** on the ONE shared global-setup server (`grep -l "Group Expedition" e2e/*.spec.ts`), among them `salon-cast-flow` (adds participants), `salon-storyteller-flow`, `salon-paused-hold-flow` and `salon-chain-pause-toast-flow` (move turn state). By the time the `salon-s*` / `salon-t*` files run, the chat has a longer transcript and possibly a longer cast than when the same file runs alone, so a turn chain takes longer and draws more `done` frames. `playwright.config.ts:17` gives every test 30 s.

| Beat | File | Verdict | Fix |
|---|---|---|---|
| The axe sweep | `e2e/a11y/axe-sweep.spec.ts` | **Real, cheap — an ordering defect** | B-3 |
| The waiting quill | `e2e/salon-thinking-indicator.spec.ts:45` | **Probably real, cheap — budget + settle waits** | E-3 |
| The dropped-attachment toast | `e2e/salon-attachment-ledger-flow.spec.ts:119` | **Probably real, cheap — same class; one extra hazard** | E-2 |
| Deus (k) | `e2e/salon-deus-ex-machina-flow.spec.ts:384` | **Suite-load artifact** | none proven; `test.slow()` is the only cheap mitigation |
| The streaming cluster: `salon-streaming-avatar-flow` ×2, `salon-transcript-subscribed-read` ×2 | shared server, "Group Expedition", no `test.slow()`, mock delay 350 ms | **Probably the same budget class as the quill** | E-3 |
| The streaming cluster: `salon-roleplay-template-flow` ×2, `salon-scroll` | already carry `test.slow()`; own chats / own server | **Suite-load artifacts** | none |
| `salon-regenerate-stream-flow` | already `test.slow()`; red 2/3 even BY FILE (records of 2026-10-05, -06, -27) | **Not a load artifact and not cheap** — an unexplained intermittent | needs its own diagnosis (E-4) |

### E-2 — the dropped-attachment toast   [OPEN — probable]
- `e2e/salon-attachment-ledger-flow.spec.ts:44-96` rewrites the FIRST `kind: 'done'` frame the page receives. Hazards in the full suite: (1) the 30 s budget (`:119`, no `test.slow()`, a group-chat chain on the grown chat, two 20 s waits at `:146`, `:151`); (2) `:146` asserts the warning list equals EXACTLY `[EXPECTED_TOAST]`, so any other `.qt-toast-warning` raised on that chat by state an earlier spec left (a chain-pause warning, for one) fails it; (3) the first `done` the page sees may belong to a chain an earlier spec left running, in which case the injected ledger is spent on a frame the Salon does not fold.
- Done: `test.slow()`; a settle wait before sending (no `.qt-thinking-indicator`, composer unlocked); assert the toast with `.filter({ hasText: … })` + `toHaveCount(1)` rather than list equality. If the trace shows (3), move the beat to a chat no other spec sends to (a dedicated seeded chat is the durable fix).
- Size: S.

### E-3 — the waiting quill and its two cluster siblings   [OPEN — probable]
- `e2e/salon-thinking-indicator.spec.ts`: `:36-42` returns from `maybeUnlock` without waiting for `Chats`; `:52`, `:54` use the default 5 s where every newer spec uses 15 s (a 2026-08 record caught this beat red at `:52`, "a chat card not yet rendered"); `:58` asserts no indicator IMMEDIATELY on open, which is false while a previous spec's chain is still streaming; the mock streams at 350 ms per delta (`:28`) through a group chain inside the 30 s budget, with two 30 s waits at `:77-78` (a wait cannot outlive the test).
- `salon-streaming-avatar-flow.spec.ts` and `salon-transcript-subscribed-read.spec.ts`: same server, same chat, no `test.slow()`.
- Done: per file — `test.slow()`; 15 s on the unlock / card / list waits; a settle wait before the first in-flight assertion. No assertion is weakened.
- Size: S (three files + D-7's). Trap (memory `unify-review-finding-shapes`: "e2e is serial-only", "a drawn chain length makes the timeout intermittent — fix with `test.slow()`").

### E-4 — `salon-regenerate-stream-flow`   [NO-ACTION this round — carry]
- Red by file about one run in three across four rounds, with a different beat each time (the `2/3` swipe counter at `:188` among them). That is not suite order. It needs a server log of a red run; the unify-236 note measured one "swipe hang" as the 30 s budget, but the file already carries `test.slow()`. Leave the watch counter; do not spend a smalls lane guessing.

### E-5 — `maybeUnlock` is copied into ~128 spec files   [NO-ACTION — recommend OUT]
- A shared `e2e/support/unlock.ts` would remove the class behind D-7 and E-3, but it edits nearly every spec, and several rewritten surfaces' specs will be redone at the catch-up.

### E-6 — P4.173 Tier 3: `workspace-help-ask-flow`'s fifth beat   [OPEN — optional]
- `e2e/workspace-help-ask-flow.spec.ts:28` records it: a `help_navigate` turn needs a one-shot `toolCall` on the mock (`e2e/support/mock-llm.ts` `MockLlmOptions.toolCall`), which on the shared mock would land on an arbitrary earlier send. It needs a spec-owned instance + `e2e/support/seed-help-fixture.ts`.
- Size: M. P4.D309 item 21 (the outfit-asks beat) is in the same position; both optional.

### E-7 — `salon-courier-images-flow:175` (the lightbox red P4.D385 recorded)   [CLOSED]
- Not among the union's five final reds (549 / 5 / 9); the vault-less fixture character no longer 500s the chat read on the union server.

---

## F. Order headers with nothing left for this round

| Order | Header's OPEN line | Status |
|---|---|---|
| P4.D344 | "All at once" / beat (K) | EXCLUDED — the Storyteller lane |
| P4.D344 | Tier 3 items 18–28 | NO-ACTION — recorded NO-PORTs / shapes; `CustomToolLibraryEntry.effect` was made required at unify-236 (H1) |
| P4.D353 | Tier 2 item 20 (WebKit) | NO-ACTION — a dogfood row |
| P4.D353 | C4 / C5 / C7 | NO-ACTION here — v4 filing candidates, owned by the filing list |
| P4.D353 | `diagram-lightbox` guarded beyond v4 | B-4, row 9 |
| P4.D357 | nothing in scope | CLOSED |
| P4.D358 | H-out-14's last step | B-2 |
| P4.D359 | W10 | B-4 |
| P4.D359 | Tier 3 #30 | B-6 |
| P4.D383 | the `ThemeService` follow-up | B-1 |
| P4.D384 | none; H1b superseded by PL-4 | CLOSED (no `LANE-REMOVE` shim remains in `apps/web`: `grep -rn LANE-REMOVE apps/web/src apps/web/e2e` is empty) |
| P4.D385 | none; H5 (`chatKeys.inventory` for `inventory-control-dialog.ts:288`) was "recorded either way" | NO-ACTION |
| P4.D388 | none; Tier 3 item 34 (the fixed radio `name="diagram-style"`) | NO-ACTION — v4-faithful, a filing candidate; the file is on `7624332c0`'s surface |
| P4.D388 | Tier 3 items 30 / 32 (kind policy types, `useConnectionProfiles`) | NO-ACTION — recorded NO-PORTs |

No `_LANDED = false` gate remains in `apps/web/e2e` or `apps/web/src`.

---

## G. DEFER-TO-CATCH-UP, in one place

| Item | v4 commit past the pin (ledger §3) | Why |
|---|---|---|
| B-6 the Core-whisper card | `f1386cebd` | its tab file is deleted; cards re-homed through a registry |
| A-3's three sites in `screens/characters/**` and `screens/settings/providers/` | `7624332c0`, `f1386cebd` | latent only; the files are rewritten |
| The 40 LATENT-FIXED rows on character / settings / new-chat screens | `7624332c0`, `f1386cebd`, `348172b9c` + `309741661` + `dca8ff120` | no defect today; capture them when each screen is re-ported |

Checked and NOT deferred: nothing in this survey touches the Brahma console cards (`3cc12d346`), the message editor (`8d5eb8f0e`), the turn-mode control (`b941280d5`), the Markdown-editor typeaheads (`bb875c6f2`) or Wardrobe History's Wear again (`0d73a4aee`). The tooltip rewrite (`43623cca3`) bears only on B-4 row 1.

---

## H. Proposed lanes (owned files disjoint)

**Lane SPA-1 — "Theme tokens + the a11y project" (M).** B-1, B-3, B-5; B-4's record if ratified.
- Owns: `apps/web/src/app/theme/theme.service.ts`, `theme.service.spec.ts`, `theme/bundled-themes.ts`, NEW `theme/theme-tokens-css.ts` (+ spec + corpus fixture), `theme/theme-contrast.residues.json`, `screens/settings/appearance/scoped-theme-css.ts` (the maps become imports), NEW `apps/web/oracle/theme-tokens.recorder.ts`, `e2e/a11y/known-violations.json`, `apps/web/playwright.config.ts`, `apps/web/package.json`, `apps/web/README.md`, `screens/characters/list/character-card.ts` (+ spec).
- Gate needs: one `--project=a11y` run with `QT_AXE_REBASELINE=1`; a pin worktree for the recorder.

**Lane SPA-2 — "Query capture: the seeds, the shared keys, the census" (S–M).** A-1, A-2, A-3 (the in-lane subset), A-4.
- Owns: `ui/aesthetic-editor-field.ts`, `screens/characters/edit/appearance-tab.ts`, `screens/characters/view/tabs/character-appearance-tab.ts`, `screens/characters/list/characters-list.ts`, `screens/salon/salon-list.ts`, `chat/sidebar/chat-scenario-control.ts`, `memory/memory-list.ts`, `chat/custom-tool-presets.ts`, `chat/merge-conversation-modal.ts`, `chat/post-office/compose-mail-dialog.ts`, `scenario/scenario-browser-dialog.ts`, `scenario-builder/save-scenario-dialog.ts`, `document-browser/document-links-dialog.ts`, `document-browser/project-store-browser.ts` (each with its spec), NEW `core/query-fn-capture.census.spec.ts`.

**Lane SPA-3 — "Store viewer Markdown + the Inventory filed gap" (S–M).** C-1, C-2, and A-3's two `document-viewer.ts` rows.
- Owns: `document-browser/document-viewer.ts` (+ spec), `chat/render/markdown-renderer.ts` (ONE additive option; + spec), `inventory/inventory-browser.ts`, `inventory/inventory-folder-picker-dialog.ts`, `inventory/inventory-control-dialog.ts`, `inventory/inventory-item-actions.ts` (+ specs), `e2e/document-store-browser-flow.spec.ts`.

**Lane SPA-4 — "e2e settle waits and budgets" (S).** D-7, E-2, E-3; optionally items D-4 and E-6 (each M, each a NEW own-instance file).
- Owns: `e2e/salon-custom-tools-flow.spec.ts`, `e2e/salon-thinking-indicator.spec.ts`, `e2e/salon-attachment-ledger-flow.spec.ts`, `e2e/salon-streaming-avatar-flow.spec.ts`, `e2e/salon-transcript-subscribed-read.spec.ts`; optional NEW `e2e/salon-novel-template-flow.spec.ts`, `e2e/workspace-help-ask-flow.spec.ts`, `e2e/support/mock-llm.ts`.
- This lane runs Playwright; it should not overlap another lane's e2e run on the shared ports (4319 / 45301).

The measurement item D-2 (H-4) can ride any lane's record; it edits no source unless pinned as a spec.

### Meeting points with other areas
- `screens/salon/salon-conversation.ts`, `core/chat-stream.reducer.ts`, `chat/message-list.ts`, `chat/streaming-message.ts`, `e2e/salon-storyteller-flow.spec.ts` — the Storyteller lane's; nothing here edits them. B-4 option (B) WOULD edit `salon-conversation.ts` (row 12).
- `chat/render/markdown-renderer.ts` — Lane SPA-3 adds one option. Any other area's lane touching chat rendering (C3 `isProseTemplate`, the diagrams filing work) meets it here.
- `apps/web/package.json` — Lane SPA-1 only. Any lane that bumps the SPA version meets it (the usual version-file union).
- `e2e/support/mock-llm.ts` — Lane SPA-4's optional E-6 only; the Pascal area's scripted-resolver rows may want it too.
- `inventory/inventory-browser.ts` — Lane SPA-3; a wardrobe / inventory server lane with an SPA rider would meet it.
- `e2e/salon-custom-tools-flow.spec.ts` — Lane SPA-4; the Pascal area lists `:46` in its own carried list (`phase-4.md:7485`). ONE owner.

### Must land together
- B-1 and its axe re-baseline (`known-violations.json`) and `theme-contrast.residues.json` — one change, or the a11y gate is red between them.
- A-1's two character tabs — one key, two seeders; fixing one leaves the hole.
- C-1 and the `renderMarkdownToHtml` option.

## I. Recommended OUT of the round

- **C-3** (D386 items 8 / 9 shells): L, no behaviour change, and the wardrobe dialog moves again past the pin.
- **B-6** (Core-whisper card): `f1386cebd`.
- **E-4** (`salon-regenerate-stream-flow`): needs a diagnosis, not a smalls fix.
- **E-5** (a shared unlock helper): ~128 files.
- **The 40 LATENT-FIXED capture edits**: no defect; mostly rewritten surfaces. A-4's census allowlists them by name instead.
- **D-5** (`Draft formatting instructions`): R-4 stands.

## J. Rulings needed

1. **W10 `R4_GUARD_BEYOND_V4` (B-4).** (A) ratify the twelve guards as a ruled divergence, or (B) remove them to v4's exact set. Recommendation: (A); re-measure row 1 at the tooltip catch-up.
2. **A-1 on a DEFER surface.** Fix the two depiction-guideline tabs now (they are on `7624332c0`'s rewritten surface), or defer them and fix only `aesthetic-editor-field.ts`. Recommendation: fix now — a Save can write blank guidelines today, and the edit is ten lines per file.

Smaller choices a lane can take without the human: B-1 fetch-vs-embed (recommend embed), B-3 (a) vs (b) (recommend (a)), C-1 frontmatter (recommend strip, as v4).
