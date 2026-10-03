# Survey — P4.D247: `9753d0eb2`'s SPA half (the project Characters card: the Add-character picker, visible remove buttons, the reworded strings and toasts) → v5 `screens/prospero/cards/project-characters-card.ts`

**Date:** 2026-10-03 · **v4:** `e5c6bd0c0` (the hunks are `9753d0eb2`'s; `e5c6bd0c0` touches none of these paths) · **v5 main:** `538fcbf65` · **Kind:** read-only measurement — `git show 9753d0eb2:<path>` / `git show 9753d0eb2 -- <path>` of the four v4 SPA/route paths; reads of every v5 file named below on `main`; `ggrep` of the SPA + e2e trees for every string, key and verb the card touches. Nothing was built or run in either repo.

## The finding in one line

**v5's Characters card has NEVER shown a roster** — it reads `project().roster` (`project-characters-card.ts:119`), a key no server path has ever emitted (the GET enriches **`characterRoster`** in place, `api/projects.rs:350`, exactly as v4 does), so every v5 project detail says "No characters in the roster yet." whatever the roster holds; the vitest specs pass only because their fixtures populate the phantom key (`projects.spec.ts:56`, `quick-hide-consumers.spec.ts:228`). The port therefore closes a standing v5 defect (since P4.6l, 2026-07-11) **in the same hunk** that absorbs v4's picker. Everything else is additive and SPA-local: the add verb (`projectCharacterAdd`) exists end to end, v5 never swaps a PUT body into state (it refetches), so **P4.D247 lands before, after, or without P4.D246 with zero breakage** — but one more Playwright file (`toast-open-rows-flow.spec.ts:187-221`) pins the two OLD toggle toasts and goes red by construction.

---

## §A The v4 hunks (`9753d0eb2`, quoted from the shipped code, never the commit prose)

### A1 `app/prospero/[id]/components/CharactersCard.tsx` — post-commit, whole (291 lines)

Props (`:39-46`): `project`, **`onAddCharacter: (characterId: string) => Promise<void>`** (new), `onRemoveCharacter`, `onToggleAllowAnyCharacter`, `expanded`, `onToggle`. State (`:57-59`): `pickerOpen` (false), `search` (`''`), `addingId` (`string | null`). `rosterEditable = !project.allowAnyCharacter` (`:61`).

`visibleCharacters` (`:64-71`, unchanged): dedupe by `id` (falsy id dropped), then `!shouldHideByIds(char.tags || [])` over `project.characterRoster` (the ENRICHED entries — v4 `app/prospero/[id]/types.ts:96` `characterRoster: ProjectCharacter[]`).

**The picker query** (`:74-78`):

```ts
const { data: charactersData, isLoading: charactersLoading } = useQuery({
  queryKey: queryKeys.characters.list(),
  queryFn: ({ signal }) => apiFetch<{ characters: CharacterOption[] }>('/api/v1/characters', { signal }),
  enabled: expanded && rosterEditable && pickerOpen,
})
```

— the SHARED unfiltered characters-list key (`lib/query/keys.ts:28` `['characters','list',{}]`); `/api/v1/characters` with no query string excludes archived (`handlers/get.ts:28-33`) and does NOT exclude NPCs or user-controlled characters (`:35-49` filter only on an explicit param).

**Candidates** (`:80-88`):

```ts
const onRoster = new Set(project.characterRoster.map(c => c.id))
const term = search.trim().toLowerCase()
return (charactersData?.characters ?? [])
  .filter(c => !onRoster.has(c.id))
  .filter(c => !shouldHideByIds(c.tags || []))
  .filter(c => !term || c.name.toLowerCase().includes(term) || (c.title ?? '').toLowerCase().includes(term))
  .sort((a, b) => a.name.localeCompare(b.name))
```

Note `onRoster` is built from the RAW roster (pre-dedupe, pre-quick-hide) — a hidden roster member is not offered again.

`handleAdd` (`:90-97`): `setAddingId(id)`; `try { await onAddCharacter(id) } finally { setAddingId(null) }`. `closePicker` (`:99-102`): `setPickerOpen(false)` AND `setSearch('')`. The picker is NOT closed by a successful add.

**Every user-facing string, byte-for-byte (line → bytes):**

| line | where | bytes |
|---|---|---|
| `:114` | header title | `Characters` |
| `:116-118` | header subtitle | allowAny → `Open to every character`; else `` `${n} character${n !== 1 ? 's' : ''} in roster` `` over `visibleCharacters.length` |
| `:131` | toggle label | `Allow Any Character` |
| `:133-135` | toggle description | allowAny → `Every character may use the project files and shared wardrobe.`; else `Only roster characters may use the project files and shared wardrobe.` |
| `:140-145` | switch | class gains `shrink-0`; `role="switch"`, `aria-checked`, **`aria-label="Allow Any Character"`** (new — v5 already has it) |
| `:155-161` | allowAny body (replaces the WHOLE roster region) | `<p className="qt-text-small">` `Any character in a project chat may read and edit its files and borrow from its wardrobe.` + JSX newline-whitespace + `Turn this off to choose who may.` — the two source lines render with ONE space between them (JSX collapses the newline+indent to a single space) |
| `:167-173` | closed picker | `<button type="button" className="qt-button qt-button-secondary qt-button-sm w-full">Add character</button>` |
| `:177-185` | search input | `placeholder="Search characters…"` (U+2026), `className="qt-input flex-1"`, `autoFocus`, `aria-label="Search characters to add"` |
| `:186-192` | close | `<button type="button" className="qt-button qt-button-ghost qt-button-sm">Done</button>` → `closePicker` |
| `:194` | list frame | `mt-2 max-h-56 overflow-y-auto` |
| `:196` | loading | `Loading characters…` (U+2026) |
| `:198-200` | empty | `search.trim()` ? `No characters match.` : `Every character is already on the roster.` |
| `:206-220` | candidate row | `<button type="button" disabled={addingId !== null} className="w-full flex items-center gap-2 p-2 rounded-md text-left hover:qt-bg-muted transition-colors disabled:opacity-50">`, `Avatar size="xs"`, name (`block text-sm text-foreground truncate`), title only when truthy (`block qt-text-xs qt-text-secondary truncate`), trailing `addingId === char.id ? 'Adding…' : 'Add'` (U+2026) |
| `:233` | roster empty / all hidden | `project.characterRoster.length === 0` ? `No characters in the roster yet.` : `No visible characters (some may be hidden).` |
| `:234-238` | roster empty hint | `Until someone is added, no character may use the project files or shared wardrobe.` (shown only when the RAW roster is empty) |
| `:255` | remove button class | `absolute top-1 right-1 p-1 rounded-full opacity-60 group-hover:opacity-100 focus:opacity-100 qt-text-secondary hover:qt-text-destructive hover:qt-bg-destructive/10 transition-all` (was `opacity-0 group-hover:opacity-100`) |
| `:256-257` | remove button | `title="Remove from roster"`, **`aria-label={`Remove ${char.name \|\| 'character'} from roster`}`** (new) |
| `:273` | tile name | `char.name \|\| 'Unknown Character'` |
| `:276` | tile count | `` {char.chatCount || 0} chat{char.chatCount !== 1 ? 's' : ''} `` — the plural test reads the RAW `chatCount` (undefined → `chats`; v5's ported `n !== 1` over `?? 0` agrees on every value the server sends — `chatCount` is always a number) |

Removed by the commit (`git show 9753d0eb2 -- …CharactersCard.tsx`): `Any character can join project chats.` / `Only roster characters can participate.` (toggle descriptions) and `Characters are added when chats are associated.` (empty hint). The roster grid is now rendered ONLY when `rosterEditable` (allowAny OFF) — with allowAny ON the explainer replaces it even over a non-empty roster.

### A2 `app/prospero/[id]/hooks/useProjectDetail.ts` (post-commit)

- **`handleToggleAllowAnyCharacter`** (`:88-108`): PUT `{ allowAnyCharacter: !project.allowAnyCharacter }`; `if (!res.ok) throw new Error('Failed to update project')`; `setProject(data.project)` (**the swap** — v4's bug: pre-commit the PUT answered the un-enriched row, so the card read string ids as entries and showed an empty roster; the fix is server-side, §A3); success toast `data.project.allowAnyCharacter ? 'Every character may now use the project files and wardrobe' : 'Only roster characters may use the project files and wardrobe'` (was `Any character can now participate` / `Only roster characters can participate`); catch → `showErrorToast(err instanceof Error ? err.message : 'Failed to update setting')` — so on a non-OK response the toast is the FIXED `Failed to update project`, never the server's body.
- **`handleAddCharacter`** (NEW, `:271-290`): POST `?action=add-character` `{ characterId }`; on `!res.ok` → `const data = await res.json().catch(() => null); throw new Error(data?.error || 'Failed to add character')` (**the server's own `error` sentence reaches the toast** — e.g. the archived refusal); then `await fetchProject()` (a full GET re-read, NOT a swap); then `showSuccessToast('Character added to the roster')`; catch → `showErrorToast(err instanceof Error ? err.message : 'Failed to add character')`. Never throws to the card (so the card's `finally` alone clears `addingId`).
- **`handleRemoveCharacter`** (`:292-308`): DELETE `?action=remove-character`; `if (!res.ok) throw new Error('Failed to remove character')` (FIXED — body never read); `await fetchProject()`; toast `Character removed from the roster` (was `Character removed from project`).
- Every other `handleSave*` still swaps `data.project` (`:78,120,144,168,192,213,234,258`) — enriched since §A3.

### A3 The server contract the card relies on (`9753d0eb2`, owned by P4.D246 — quoted for §H)

- `app/api/v1/projects/[id]/actions/project-crud.ts`: NEW `enrichProject(project, repos)` (`:23-68` post-commit) — `{...project, characterRoster: <entries {id, name, defaultImageId, defaultImage, tags: char.tags || [], chatCount}, nulls filtered>, _count: {chats, files, characters: project.characterRoster.length}}`; the GET answers it (`:84`) and **the PUT now answers it too** (`handlePutDefault` `:108-115`, with a new `if (!project) return notFound('Project')`).
- `actions/roster.ts` (UNCHANGED by the commit): add → `notFound('Project')` / `addCharacterSchema.parse` / `notFound('Character')` / the archived 400 `That character is archived; rehydrate them before adding them to a roster.` / idempotent append / `{ success: true }` (`:56-90`); remove → filter + always write / `{ success: true }` (`:95-115`).
- `schemas.ts:16`: `allowAnyCharacter: z.boolean().prefault(true)` (was `false`) — a NEW project opens the card in "Open to every character" mode.

### A4 `app/prospero/[id]/ProjectDetailView.tsx`

`:55` destructures `handleAddCharacter`; `:198` passes `onAddCharacter={handleAddCharacter}`. Nothing else.

### A5 What the commit did NOT do

- No CSS change (no `app/styles/**` in the file list) — every class the card uses already exists (`hover:qt-bg-muted` v4 `_utilities.css:809`).
- No `lib/help/**` / help-categories change — only four `help/*.md` bodies (P4.D246 vendors them); no frontmatter `title`/`url` moved (`git show 9753d0eb2 -- help/… | grep '^[-+]title'` empty), so the SPA's `help-guide-tables.json` is untouched.
- No change to `roster.ts`, the characters list route, `queryKeys`, the toast lib, or `Avatar`.
- No new test file for the card (v4 has no jest spec of `CharactersCard`; the picker logic is inline in the component — there is NO exported pure function, so no tier-1 recorder path exists; memory `a-v4-service-that-exports-only-its-runner-has-no-tier-1-path`).
- The project detail's OTHER save handlers keep their own error handling (out of the commit).

---

## §B v5's twins (`main` `538fcbf65`)

### B1 `apps/web/src/app/screens/prospero/cards/project-characters-card.ts` (167 lines) — what renders today

| v5 | v4 post-commit | status |
|---|---|---|
| `:15-20` doc: "There is NO add picker — 'Characters are added when chats are associated' (v4 copy)" | the picker exists | **frozen false claim** — rewrite |
| `:26-31` `qt-collapsible-card` (uncontrolled, `[defaultOpen]`), `[description]="subtitle()"` | custom header + `expanded` prop | structural twin; `expanded` is not observable to the card (see §D3) |
| `:32` toggle row `flex items-center justify-between px-2 py-3 qt-bg-muted rounded-lg mb-3` | `… gap-3 px-4 py-3 qt-bg-muted` | layout idiom of v5's card body; add `gap-3` |
| `:36-40` descriptions `Any character can join project chats.` / `Only roster characters can participate.` | §A1 `:133-135` | **OLD strings** |
| `:45` switch class (no `shrink-0`) | `shrink-0` added | add |
| `:50` `aria-label="Allow Any Character"` | same | ✓ (already present) |
| `:61-65` empty: `roster().length === 0` → `No characters in the roster yet.` + `Characters are added when chats are associated.` | §A1 `:231-239`: branch on the FILTERED list, text on the RAW length (`No visible characters (some may be hidden).`), new hint | **two divergences**: the all-hidden sentence was never ported (pre-existing, it is in v4 pre-commit too); the hint is OLD |
| `:66-98` grid ALWAYS rendered | grid only when allowAny OFF; explainer otherwise | **missing explainer branch** |
| `:75` remove class `opacity-0 group-hover:opacity-100` | `opacity-60 … focus:opacity-100` | OLD |
| `:77` `aria-label="Remove from roster"` (static) | `` `Remove ${name \|\| 'character'} from roster` `` | divergent |
| `:83` `[routerLink]="['/characters', char.id]"` | `/characters/${id}/view` | v5 route idiom — keep |
| `:117-124` `roster` computed over **`this.project().roster ?? []`** | `project.characterRoster` | **THE DEFECT (§D1)** |
| `:126-129` subtitle always the count | `Open to every character` when allowAny | OLD |
| `:141-155` toggle: `updateProject` → `invalidateQueries(projectKeys.detail(id))` → toast `updated.allowAnyCharacter ? 'Any character can now participate' : 'Only roster characters can participate'`; catch → `err.message` | §A2 | OLD strings; **error toast leaks the server message** where v4 shows the fixed `Failed to update project` (§D4) |
| `:158-166` remove: `removeProjectCharacter` → invalidate → `Character removed from project`; catch → `err.message` | §A2 | OLD string; leaks the server message where v4 shows `Failed to remove character` |
| — | `handleAdd` / picker / `addingId` / candidates | **absent** |

### B2 The data layer

- `screens/prospero/projects.api.ts:79-87` `updateProject` → `dispatchData({type:'projectUpdate', projectId, project: patch})` → `data['project']`; `:89-95` `removeProjectCharacter` → `projectCharacterRemove`. **No add helper** — the verb is otherwise fully wired: `core-contract.ts:1794-1797` `ProjectCharacterAddRequest {type:'projectCharacterAdd'; projectId; characterId}` (in the `CoreRequest` union at `:2777`), `api/types.rs:1346-1352` `ProjectCharacterAdd { project_id, character_id }` (camelCase), `api/engine.rs:3270-3278` → `api/projects.rs:599-638` `project_character_add` (v4's four arms incl. the archived 400 sentence byte-identical, `:615-619`; `{success:true}`). **No dispatch verb is needed; `api/types.rs` stays frozen (census 441).**
- `projectKeys.detail(id)` = `['projects','detail',id]` (`projects.api.ts:22`); the detail screen's query (`project-detail.ts:211-214`) → `fetchProject` → `projectGet` (`projects.api.ts:73-76`).
- **No site in the SPA swaps a PUT body into the cache** (`ggrep setQueryData screens/prospero` → only `prospero-list.ts:183,191`, the list's optimistic delete). Every per-field save — `project-detail.ts:369-378`, `project-model-behavior-card.ts:258`, `project-image-generation-card.ts:267`, and this card `:143-146` — awaits `invalidateQueries(projectKeys.detail(id))`, i.e. a `projectGet` refetch. **So v4's bug (the swap of a non-enriched PUT) never existed in v5** (and is invisible on v5 regardless because of §D1); P4.D246's enriched PUT changes nothing the SPA must do.

### B3 The contract type (`core-contract.ts`)

- `ProjectRosterCharacter` (`:4159-4166`): `{id, name, defaultImageId: string|null, defaultImage: EnrichedImage|null, tags: string[], chatCount: number}` — **already the v4 entry shape** (`defaultImageId` is absent on the wire when null — `api/projects.rs:333-338` — so the TS `string|null` reads `undefined`; harmless, `characterAvatarSrc` takes `null|undefined`).
- `ProjectDetail` (`:4173-4204`): **`characterRoster: string[]`** (`:4181`, WRONG for the GET projection — v4's own type says entries) and **`roster?: ProjectRosterCharacter[]`** (`:4182`, a key the server never sends). Consumers of `ProjectDetail` (ggrep): `projects.api.ts` (create/get/update return types), `project-detail.ts`, the six cards, five specs. **No production reader of `.characterRoster` exists** (ggrep `\.characterRoster` in `src/` → spec fixtures only), so retyping is compile-safe; `createProject`'s only consumer reads `.id` (`project-create-dialog.ts:94-96`).

### B4 The characters list, avatar, quick-hide, toasts, focus

- List: `screens/characters/characters.api.ts:110-123` `fetchCharacterList(core, filter?)` → `characterList` (archived excluded by default, `api/characters.rs:126-150` — v4's chokepoint); key `characterKeys.list()` = `['characters','list']` (`:46-47` — v5's omit-when-absent convention; only DISTINCTNESS from the filtered keys is contractual, `:33-42`). Five dialogs already read that exact `{key, fn}` pair (`chat/cast/add-character-dialog.ts:405-408`, `chat/post-office/compose-mail-dialog.ts:220`, `insert-announcement-dialog.ts:456`, `chat/tools/search-replace-modal.ts:509`, `scenario-builder/save-scenario-dialog.ts:274`) and the mention source shares the key (`editor/mentions/mention-source.ts:6-10,46-66` — its own `enabled` toggling over a `QueryObserver`). `CharacterListItem` (`core-contract.ts:3533-3570`) carries `id, name, title, defaultImageId, defaultImage, tags, archivedAt?` — everything the picker needs.
- Avatar: `ui/avatar.ts:5,14` `AvatarSize` includes `'xs'` (32 px); src via `characterAvatarSrc(defaultImage, defaultImageId)` (`characters.api.ts:95`).
- Quick-hide: `quick-hide/quick-hide.service.ts:108` `shouldHideByIds(tagIds?)`.
- Toasts: `ui/toast.service.ts:73,78` `showSuccess` / `showError`.
- `autoFocus` twin: plain `autofocus` does NOT fire on a node inserted after load; the SPA's precedent is `wardrobe/outfit-quick-pick.ts:127-132` (a `viewChild` + `effect` calling `.focus()` — "React focuses it as it mounts; the signal `viewChild` resolves on the render that creates it").
- Error envelope: `CoreClient.dispatchData` throws `CoreDispatchError(resp.data)` whose `message` is the server's error sentence (`core-client.ts:133-139`, `core-contract.ts:4560-4575`) — the twin of v4's `data?.error`.
- `qt-*` classes the port adds all resolve: `qt-button-secondary`/`qt-button-ghost`/`qt-button-sm` (`styles/qt-components/_interactive.css:21-22,64,77,135`), `qt-input` (`:150`), `hover:qt-bg-muted` (`_utilities.css:827`); the rest are Tailwind (`opacity-60`, `focus:opacity-100`, `disabled:opacity-50`, `max-h-56`, `shrink-0`).

### B5 The specs that pin today's (wrong) shape

- `screens/prospero/projects.spec.ts`: the `project()` builder sets `characterRoster: []` AND `roster: []` (`:55-56`); `:476-484` pins `Characters are added when chats are associated`; `:486-500` pins `Any character can now participate`; `:454-474` pins the leaked `boom` on a toggle failure; `:502-535` builds `project({ roster })` and pins `Character removed from project` + the leaked `cannot remove`.
- `quick-hide/quick-hide-consumers.spec.ts:223-236` builds `characterRoster: roster.map(r => r.id)` + `roster` — **the string-id shape the server never sends on a GET** — and `:570-592` pins dedupe/hide + `1 character in roster` through the phantom key.
- `apps/web/e2e/toast-open-rows-flow.spec.ts:187-221` (P4.29, port 4323, the groups-projects pair) pins both OLD toggle toasts in BOTH directions — **red by construction once the strings move**.
- `apps/web/e2e/projects-flow.spec.ts:122-167` (port 4325) toggles Allow Any on the FIRST list card and renames it; asserts no roster/toast text — **neutral** to this port (the switch's `role`/`aria-label` are unchanged).

### B6 Help / tooltip copy

None for this card: `ggrep -rn -i 'roster characters\|Allow Any\|can participate\|join project chats'` outside the card hits only `project-detail.ts:27,148` (the import/use) and the help category slug `project-characters` (`help/help-categories.ts:81`, `help/__fixtures__/help-guide-tables.json:57`) — a slug, unchanged by the commit. The help BODY (`help/project-characters.md`) is served from the server's vendored tree (P4.D246's).

---

## §C Differential families touched

- **No harness differential reads `apps/web/**`.** There is no SPA oracle recorder for the project cards (`apps/web/oracle/` holds 13 recorders, none prospero), and v4's picker logic is component-inline — **no tier-1 recorder path** (memory `a-v4-service-that-exports-only-its-runner-has-no-tier-1-path`). The proof is: vitest arms written RED-FIRST against the quoted v4 bytes (each new string and each behaviour asserted, run against the unported card, recorded red, then ported), plus ONE live Playwright beat over the real server.
- **Server families this lane depends on but does not edit** (P4.D246's): `projects_routes_equivalence` (`get_iota`, `put_*`, `add_character`, `add_character_archived`, `remove_character`), `projects_tier2_equivalence`. Their wire bytes for `projectGet` (`characterRoster` enriched) and the two roster verbs are UNCHANGED by `9753d0eb2` except the PUT body — which the SPA does not consume (§B2).
- **Neutral SPA specs to re-run by name** (they touch `ProjectDetail` through the retype): `project-model-behavior-card.spec.ts` (`characterRoster: []` at `:20` — type-compatible), `project-settings-card.spec.ts`, `prompt-field-migration.spec.ts:254-265`, `project-chats-section.spec.ts`, `project-tool-settings-modal.spec.ts`.

---

## §D Traps and rulings

### D1 The phantom `roster` key — a standing v5 defect, closed by this lane (NOT drift)

`project-characters-card.ts:119` reads `this.project().roster`; `api/projects.rs:290-360` (`project_get`) writes the enriched list to **`characterRoster`** (`:350`, `obj.insert("characterRoster".into(), …)`), and `git log -S'"roster"' -- crates/quilltap-core/src/api/projects.rs` is EMPTY — no server revision ever emitted `roster`. The SPA type invented it (`core-contract.ts:4182`, since P4.6l `fc07efcc3`, 2026-07-11). Consequence on every instance: the roster grid never renders, the subtitle always reads `0 characters in roster`, the remove button is unreachable from the UI. The specs and the quick-hide consumer spec populate the phantom key, so nothing failed. The fix is the retype (`characterRoster: ProjectRosterCharacter[]`, `roster` deleted) plus the card reading `characterRoster` — and the fixtures rebuilt to the WIRE shape (red-first: a spec whose fixture carries ONLY `characterRoster` entries reds on unported main with `0 characters in roster`). Record it in the lane record as a v5 defect found by the survey (a dogfood-class miss: the P4.6l walk never asserted a roster name). **No ruling needed** — it is v4's shape on both sides.

### D2 The ledger row is right about the SPA half

Row `9753d0eb2` item (9) — "`CharactersCard.tsx` (an Add-character picker, visible remove buttons when Allow Any is off) and `useProjectDetail.ts` (`handleAddCharacter` over the existing `?action=add-character`; three reworded toasts)". Verified: the toasts reworded are FOUR sentences across three handlers (two toggle directions + remove; add is new) — "three reworded toasts" counts handlers. Nothing refuted. Not in the row: the remove button's dynamic `aria-label`, the switch's `shrink-0`/`gap-3`, the explainer branch replacing the grid when allowAny is ON.

### D3 `expanded` in the picker's `enabled`

v4 gates the fetch on `expanded && rosterEditable && pickerOpen`. v5's card owns no `expanded` (uncontrolled `qt-collapsible-card`, `collapsible-card.ts:87-90,120-131`), but projected content is instantiated by the parent even while `@if (isOpen())` hides it — the card's signals (incl. `pickerOpen`) persist across a collapse, as React's card state does. To carry the third conjunct the card switches to the CONTROLLED mode (`[isOpen]="expanded()"`, `(openChange)="expanded.set($event)"`, `expanded` seeded from `defaultOpen` — the `chat/sidebar/chat-sidebar.ts:238-303` precedent). Observable difference otherwise: a collapsed card with the picker left open would keep refetching the list on invalidation. Cheap and faithful — Tier 1.

### D4 Error-toast sentences: v5 leaks server messages on toggle and remove

v4 throws FIXED sentences on a non-OK response for the toggle (`Failed to update project`) and remove (`Failed to remove character`) — the body is never read — but reads the body's `error` for ADD (`data?.error || 'Failed to add character'`). v5's card toasts `err.message` on all paths (`:153`, `:164`), i.e. the server's sentence. This is P4.29's own class ("v5 was leaking server messages", restored at that unification for the files-browser arms). The twin: a `CoreDispatchError` (the non-OK analogue) → the fixed sentence; any other `Error` → its message (v4's fetch-reject path); a non-`Error` → the catch fallback (`Failed to update setting` / `Failed to remove character` / `Failed to add character`). For ADD a `CoreDispatchError` → `err.message || 'Failed to add character'`. The existing spec arms `:454-474` (`boom`) and `:524-534` (`cannot remove`) pin the leak and flip red-first. The SAME leak on the other project-detail handlers (`project-detail.ts:378`, the model-behavior / image-generation cards) is OUT of this commit's scope — a named Tier 3 deferral, not silently widened.

### D5 The picker's focus

`autoFocus` → a `viewChild` + `effect` focus (§B4 precedent). Plain `autofocus` passes a jsdom spec and does nothing in Chromium on a node inserted after load — pin it in the Playwright beat (`expect(search).toBeFocused()`), not in vitest.

### D6 `localeCompare`, `toLowerCase`, `includes`

The SPA runs in a JS engine — `a.name.localeCompare(b.name)` (default locale, no options) and the lower-case `includes` are byte-identical semantics by construction. Do NOT route the sort through ICU4X or any Rust-side twin. In vitest (jsdom on Node) and Chromium the default locale may differ (`en-US` both in practice) — pick candidate names whose order is locale-stable (ASCII letters, distinct first letters) in the specs.

### D7 The cache key is shared — do not invent a flag-carrying key

`characterKeys.list()` is read by five dialogs + the mention source with the SAME `fetchCharacterList(core)` function, so the picker reuses the warm cache (v4's intent — one key). A different key shape (e.g. `list({archived: …})`) would create a second cache entry and a second fetch; the P4.D64 note (`characters.api.ts:33-42`) makes distinctness contractual only for the FILTERED variants. P4.115's fix (one raw `['connection-profiles']` entry where two flag-less shapes had diverged) is the same lesson: one key ⇒ one `queryFn`.

### D8 The add refetch timing

v4: `await fetchProject()` BEFORE the success toast and BEFORE `addingId` clears (the card's `finally` runs after the hook resolves) — so when the rows re-enable, the added character has already left the candidate list. v5's twin: `await queryClient.invalidateQueries({queryKey: projectKeys.detail(id)})` (which awaits the active refetch) before the toast, inside the `try` whose `finally` clears `addingId`. A vitest arm must hold the refetch and assert every row `disabled` + the clicked row's `Adding…` between the click and the refetch's release (memory `a-rollback-is-invisible-once-the-refetch-lands` — assert BETWEEN).

### D9 No ruling needed before launch

Nothing here needs the human: the defect fix is v4's own shape; the error-sentence alignment is P4.29's precedent; the refetch-vs-swap is v5's established idiom with an identical end state (recorded as a retained idiom, not a divergence).

---

## §E Ownership

**Edits (all under `apps/web/**`, plus the two docs):**
- `apps/web/src/app/screens/prospero/cards/project-characters-card.ts` (the whole card)
- `apps/web/src/app/screens/prospero/projects.api.ts` (NEW `addProjectCharacter`)
- `apps/web/src/app/core/core-contract.ts` (`ProjectDetail.characterRoster` retyped; `roster?` deleted — the ONLY hunk in this file)
- `apps/web/src/app/screens/prospero/projects.spec.ts` (the `ProjectCharactersCard` describe + the `project()` builder)
- `apps/web/src/app/quick-hide/quick-hide-consumers.spec.ts` (`project()` builder `:223-236` + the card arm)
- `apps/web/e2e/projects-flow.spec.ts` (ONE new beat)
- `apps/web/e2e/toast-open-rows-flow.spec.ts` (`:187-221`, the two toggle sentences only)
- `apps/web/package.json` (SPA version bump)
- `docs/CHANGELOG.md`, `docs/developer/porting/status-log.md` (append)

- `apps/web/src/app/core/core-transport.ts` (`interpretHealth`'s ONE new degraded-503 arm) + `core-transport.spec.ts` (NEW if none) — **added by the planner 2026-10-03** after P4.D248's survey (its §D1 R1): the Shared contract P4.D248 ↔ P4.D247 in the order's §S.

**Reads, must NOT edit:** `crates/quilltap-core/src/api/{types,engine,projects,characters}.rs`, `db/projects.rs`, `apps/web/src/app/ui/collapsible-card.ts`, `screens/characters/characters.api.ts`, `wardrobe/outfit-quick-pick.ts`, `screens/prospero/project-detail.ts` (no hunk needed — the card owns its handlers in v5's P4.29 shape), the help tree.

**Sibling files, never touched:** every `crates/**` file (P4.D245/P4.D246/P4.D248), `help/**` + `docs/v4/**` (P4.D246 whole), `harness/**`.

---

## §F The §R.5 designed-reds sentence

**P4.D247: no harness family goes red at `e5c6bd0c0` on unported main for this lane (no differential reads `apps/web/**`); its designed reds are its own — the rewritten `ProjectCharactersCard` vitest arms (red-first on the unported card) and `e2e/toast-open-rows-flow.spec.ts`'s Allow-Any beat, which pins the two pre-`9753d0eb2` toggle sentences and reds the moment the card moves — both fixed inside the lane.**

---

## §G 💸 for the dogfood pass

1. **The roster renders at all** — open a real Friday project whose roster v4 auto-populated (every pre-`9753d0eb2` chat-in-project added its cast): the grid shows those names with chat counts; before this lane v5 showed "No characters in the roster yet." on every project (§D1).
2. Allow Any ON → the subtitle `Open to every character`, the explainer, no grid; OFF → `N characters in roster`, the picker; both toggle toasts in v4's new bytes.
3. Add character → the picker lists live characters only (an archived character absent; a quick-hidden one absent; NPCs present), the search by TITLE finds a character whose name does not match, `Adding…` disables every row, `Character added to the roster`, the added name leaves the list and joins the grid with `Remove from roster` visible at rest (`opacity-60`).
4. Remove → `Character removed from the roster`; the grid shrinks; `Every character is already on the roster.` when nothing is left to add; `No characters match.` on a junk search; `Done` clears the search (reopen → empty box).
5. **The cross-lane proof (with P4.D245 landed):** remove a character from a roster with Allow Any OFF, then in a project chat ask that character to `doc_list_files` / `search_scriptorium` the project — refused; re-add — admitted. The card is the human's only lever for the gate.
6. A new project (P4.D246's `true` default) opens in "Open to every character" mode.

---

## §H The SPA's half of the Shared contract (proposed, name for name)

1. **`projectGet { projectId }`** → `{ project }` where `project.characterRoster` is the ENRICHED list `Array<{ id: string; name: string; defaultImageId?: string /* absent when null */; defaultImage: EnrichedImage | null; tags: string[]; chatCount: number }>` (missing characters dropped), plus `_count: { chats, files, characters }` — **unchanged by `9753d0eb2`; P4.D246 must not rename `characterRoster` or reorder its role** (the SPA reads it by name). The SPA retypes `ProjectDetail.characterRoster` to `ProjectRosterCharacter[]` and deletes `roster?`.
2. **`projectUpdate { projectId, project: { allowAnyCharacter } }`** → `{ project }`. The SPA reads exactly ONE field from the body — `project.allowAnyCharacter` (the success toast's branch) — and **never swaps the body into the query cache**: it awaits `invalidateQueries(projectKeys.detail(id))` and re-reads through `projectGet`. So the SPA is shape-agnostic across P4.D246 (un-enriched today, enriched after); P4.D246 must keep `allowAnyCharacter` present and boolean in the PUT body, and its new 404 (`Project not found`) reaches the SPA as a `CoreDispatchError` → the fixed `Failed to update project` toast.
3. **`projectCharacterAdd { projectId, characterId }`** (EXISTING — `api/types.rs:1346-1352`, `core-contract.ts:1794-1797`) → `{ success: true }`; errors as `CoreDispatchError` whose `message` the SPA toasts VERBATIM (v4 `data?.error`): `Project not found` (404), `Character not found` (404), `That character is archived; rehydrate them before adding them to a roster.` (400). New SPA helper: `addProjectCharacter(core, projectId, characterId): Promise<void>` in `projects.api.ts`.
4. **`projectCharacterRemove { projectId, characterId }`** (EXISTING) → `{ success: true }`; any error → the fixed `Failed to remove character`.
5. **`characterList {}`** (EXISTING, no filter) → `{ characters: CharacterListItem[] }`, archived EXCLUDED by default, NPCs and user-controlled INCLUDED; cache key `characterKeys.list()` with `queryFn: () => fetchCharacterList(core)` — the shared pair.
6. **No new dispatch verb; `api/types.rs` and `dispatch_wrong_type_census` (441) unmoved.** No Event-channel change. The card can merge before P4.D246 (it depends on nothing P4.D246 changes); the unifier's pick order is free.
