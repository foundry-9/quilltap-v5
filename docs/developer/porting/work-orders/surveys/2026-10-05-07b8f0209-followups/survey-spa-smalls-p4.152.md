# Survey — P4.152, the Angular SPA smalls (`apps/web/**` only)

Surveyed 2026-10-05 on v5 `main` `6c3a3c635` against v4 `07b8f0209` (clean).
Read-only: no build, no test, no Playwright. Every v4 byte below is from
`~/source/quilltap-server` at `07b8f0209`; every v5 line from `main`.

**Headline:** item 1 is real and slightly bigger than the order named — SEVEN
handlers across three files toast the server's message where v4 toasts a fixed
sentence, and FOUR of v5's fallback strings are v4's *catch* fallback (which v4
never reaches on a refusal), not the sentence v4 actually throws. The refocus
item is real (the projected input survives the collapse, so the `viewChild`
effect never re-fires). The `tags` item is really a SERVER divergence (v4
writes `char.tags || []`), out of this lane. Item 2 is real and has a
two-line fix. P4.D252 is clean. No other SPA item from the five newest
rounds' NEXT lists; three older dogfood standing notes are phantoms.

---

## 1. P4.D247's OPEN items

Source: `work-orders/p4.d247-project-roster-characters-card-spa.md:3`
(Unification) + Tier 3 items 10–13 (`:279-293`) + the lane record
(`status-log.md:163986`, "### Tier 3 — deferred loudly (P4.D247)").

### 1a. The server-message leak on the OTHER project-detail handlers (Tier 3 item 10)

**Quoted:** "The same server-message leak on the OTHER project-detail handlers
(`project-detail.ts:378` header save; `project-model-behavior-card.ts`;
`project-image-generation-card.ts`) — v4 throws fixed sentences there too;
out of `9753d0eb2`'s scope; named for the next smalls round."

**v4 at `07b8f0209`** — every handler is in
`app/prospero/[id]/hooks/useProjectDetail.ts` (except chat remove,
`useProjectChats.ts`). Each does `if (!res.ok) throw new Error('<THROWN>')`
and catches `err instanceof Error ? err.message : '<CATCH FALLBACK>'` →
`showErrorToast`. On a refusal the toast is therefore ALWAYS `<THROWN>` (the
body is never read); on a fetch reject it is the reject's message (Chromium:
`Failed to fetch`); `<CATCH FALLBACK>` is reachable only by a non-`Error`
throw (practically unreachable). All REACHABLE.

**Census (every project-detail toast-on-failure handler, v5 vs v4):**

| # | v5 site (main) | v5 toasts on a refusal (`CoreDispatchError`) | v5 fallback literal | v4 handler (lines) | v4 THROWN = what v4 toasts on a refusal | v4 catch fallback |
|---|---|---|---|---|---|---|
| H1 | `screens/prospero/project-detail.ts:364-380` `save()` (header save) | **server message** (leak) | `Failed to update project` | `handleSave` `:64-86` | `Failed to update project` | `Failed to update project` |
| H2 | `cards/project-model-behavior-card.ts:205-217` `onAgentMode` → `save()` `:252-264` | **server message** | `Failed to update agent mode` | `handleSaveAgentMode` `:110-132` | `Failed to update agent mode setting` | `Failed to update agent mode` |
| H3 | `project-model-behavior-card.ts:220-232` `onAnswerConfirmation` | **server message** | `Failed to update answer confirmation` | `handleSaveAnswerConfirmationOverride` `:134-156` | `Failed to update answer confirmation setting` | `Failed to update answer confirmation` |
| H4 | `project-model-behavior-card.ts:235-250` `onRoleplayTemplate` | **server message** | `Failed to update default roleplay template` | `handleSaveDefaultRoleplayTemplate` `:203-222` | `Failed to update default roleplay template` | `Failed to update roleplay template` |
| H5 | `cards/project-image-generation-card.ts:199-212` `onAvatar` → `save()` `:261-273` | **server message** | `Failed to update avatar generation` | `handleSaveAvatarGeneration` `:158-180` | `Failed to update avatar generation setting` | `Failed to update avatar generation` |
| H6 | `project-image-generation-card.ts:215-227` `onAnnounce` | **server message** | `Failed to update Lantern image announcement setting` | `handleSaveAlertCharactersOfLanternImages` `:224-246` | `Failed to update Lantern image announcement setting` | (same) |
| H7 | `project-image-generation-card.ts:230-244` `onBackground` | **server message** | `Failed to update background mode` | `handleSaveBackgroundDisplayMode` `:248-269` | `Failed to update background display mode` | `Failed to update background mode` |
| H8 | `project-image-generation-card.ts:247-259` `onImageProfile` | **server message** | `Failed to update default image profile` | `handleSaveDefaultImageProfile` `:182-201` | `Failed to update default image profile` | `Failed to update image profile` |
| H9 | `cards/project-chats-section.ts:185-195` `onRemove` | **server message** | `Failed to remove chat` | `useProjectChats.ts:88-105` `handleRemoveChat` | `Failed to remove chat` | `Failed to remove chat` |
| — | `project-characters-card.ts:352-372` toggle | `Failed to update project` ✔ | `Failed to update setting` | `:88-108` | `Failed to update project` | `Failed to update setting` |
| — | `project-characters-card.ts:383-400` add | server message ✔ (v4 reads `data?.error`) | `Failed to add character` | `:271-290` | `data?.error \|\| 'Failed to add character'` | same |
| — | `project-characters-card.ts:406-420` remove | `Failed to remove character` ✔ | same | `:292-308` | `Failed to remove character` | same |
| — | `project-tool-settings-modal.ts:139-147` | server message — **CONVERGED** | `Failed to save tool settings` | `components/tools/tool-settings/ProjectToolSettingsModal.tsx:71-78` | `errorData.error \|\| 'Failed to save tool settings'` (reads the body) | same |
| — | `project-detail.ts:388-399` `onUnlinkStore` | server message | `Failed to unlink store` | `useProjectDocumentStores.ts:82-99` `unlinkStore` | **no toast** — v4 returns `false`, logs `useProjectDocumentStores: unlinkStore error` | — |

Other cards measured with NO failure handler: `project-settings-card.ts`,
`project-scenarios-card.ts`, `project-files-card.ts`,
`project-wardrobe-card.ts`, `project-header.ts` (zero `catch`). The wardrobe
manager reads the server body as v4's `useProjectWardrobe.ts:100-153` does
(`body?.error || 'Failed to create (${status})'`) — converged, not in scope.
The chats section's two LOAD paths (`project-chats-section.ts:157-158`,
`:178-179`) set an inline `qt-error-alert`, where v4 (`useProjectChats.ts:46-
86`) is SILENT on both a non-OK (`if (res.ok)`) and a reject (console only) —
a v5 invention, **not** in item 10's scope; name it as a ruling question
(§Open questions) rather than fold it in.

**Divergence (one sentence):** nine handlers (H1–H9) toast the server's
refusal message where v4 toasts its fixed thrown sentence, and in four of them
(H2, H3, H5, H7) the fixed sentence v5 would fall back to is v4's catch
fallback, not the sentence v4 shows.

**Predicted hunks** (the P4.29 / P4.D247 class, copied from
`project-characters-card.ts:364-371`):
- Each `save()` helper (`project-model-behavior-card.ts:252-264`,
  `project-image-generation-card.ts:261-273`) gains a `refusal` argument (v4's
  THROWN) beside `fallback` (v4's catch fallback); the catch becomes
  `err instanceof CoreDispatchError ? refusal : err instanceof Error ?
  err.message : fallback`. Seven call sites pass both strings from the table
  (H2–H8). Import `CoreDispatchError` from `../../../core/core-contract`.
- `project-detail.ts:377-379` and `project-chats-section.ts:192-194`: the same
  three-arm shape with `Failed to update project` / `Failed to remove chat`.
- Doc comments re-cited to v4's real lines (v5's comments cite stale ranges:
  e.g. `:107-129` for agent mode, now `:110-132`).
- `onUnlinkStore` stays (a documented v5 addition, `project-detail.ts:382-
  387`); leave it.
- Blast radius: 4 source files, 9 handlers; `CoreDispatchError` is already
  imported by the characters card only.

**Transport caveat (affects the wording of the order):** a v5 transport
failure is NOT a plain `Error` — `core-transport.ts:96` /
`tauri-core-transport.ts:54` return `syntheticError('Connection lost. The
server may still be starting.', err)` → `CoreClient.dispatchData`
(`core-client.ts:133-137`) throws `CoreDispatchError` (kind `internal`).
So after the fix a network failure toasts v4's FIXED sentence, while v4 shows
`Failed to fetch`. The "plain `Error` → message" arm models a path
`CoreClient` never takes (the specs' `stubClient` throws raw `Error`s).
Same as the characters card today — see 1e.

**The proof** (no harness family reads `apps/web/**`; vitest is the proof):
- Copy `projects.spec.ts:908-929` ("V10 toggle errors") — a cases table
  `[new CoreDispatchError({kind:'not-found', message:'boom'}), '<THROWN>']`,
  `[new Error('network down'), 'network down']`, `['not an error',
  '<CATCH FALLBACK>']` per handler. RED-FIRST on main for every
  `CoreDispatchError` row (main toasts `boom`).
- Existing specs that stay green but whose framing is wrong:
  `project-chats-section.spec.ts:89` ("toasts the server message on a failed
  remove", throws a plain `Error`) — rename/add the refusal row;
  `projects.spec.ts:1103` (header save, plain `Error('save failed')`) and
  `:1203-1211` (background, `Error('bg fail')`) — add `CoreDispatchError`
  rows. `project-model-behavior-card.spec.ts` has NO failure arm today (its
  `describe`s at `:60`, `:128`); add one. The image-generation card has no
  spec file of its own — its arms live in `projects.spec.ts:1172+`
  (`describe('ProjectImageGenerationCard')`).
- Mutation: swap one `refusal` for its `fallback` (e.g. H7 `…display mode` →
  `…background mode`) → that row RED.
- No e2e change needed: `grep` finds none of these failure sentences in
  `e2e/*.ts`.

**Fixtures:** none.

**Risk:** none cross-lane (SPA-only files). The two `save()` helpers are
shared by 4 + 3 call sites — change the signature once per file.

### 1b. Collapse + re-expand with the picker open does not refocus the search

**Quoted (Unification):** "collapsing and re-expanding the card with the
picker open does not refocus the search (v4 remounts and `autoFocus`es; the
projected input survives the collapse)".

**v5 today:** `cards/project-characters-card.ts:259`
`searchInput = viewChild<ElementRef<HTMLInputElement>>('searchInput')`;
`:273-281` `effect(() => { this.searchInput()?.nativeElement.focus(); })`.
The card is `<qt-collapsible-card [isOpen]="expanded()"
(openChange)="expanded.set($event)">` (`:68-69`) and the collapsible renders
`@if (isOpen()) { … <ng-content></ng-content> }` (`ui/collapsible-card.ts:57-
60`). Projected content is instantiated by the PARENT and only detached by the
child's `@if`, so the `<input #searchInput>` element (and the `viewChild`
signal's value) survive a collapse → the effect has no changed dependency on
re-expand → no focus. `pickerOpen` and `search` are signals and also survive
(as v4's `useState` does).

**v4:** `app/prospero/[id]/components/CharactersCard.tsx:126`
`{expanded && (` — CharactersCard renders its own header and conditionally
mounts the body; the input at `:177-185` carries `autoFocus` and
`aria-label="Search characters to add"`. Collapse unmounts it; re-expand
remounts → React's `autoFocus` fires. `pickerOpen` (`:57`) is CharactersCard
state and survives, so re-expand with the picker open lands focus in the box.
REACHABLE.

**Divergence:** v5 leaves focus on the header after a collapse/re-expand with
the picker open; v4 focuses the search box.

**Predicted hunk:** replace the `effect` with `afterRenderEffect` (already used
in-tree: `ui/tooltip.ts`, `chat/message-list.ts`, …) reading `expanded()`,
`pickerOpen()` and `searchInput()`, focusing when all hold. A plain `effect`
reading `expanded()` is the WRONG fix: a component effect runs before the
child collapsible re-inserts the projected nodes, so `.focus()` would hit a
detached node (NOT MEASURED — measure it as a mutation, the P4.D247 M9
precedent). One file.

**Proof:** vitest cannot see real focus reliably (jsdom-ish); the live beat
is the proof. In `e2e/projects-flow.spec.ts`, after the existing REOPEN
assertion (`:382-386`, `await expect(search).toBeFocused()`), add: click the
header (collapse), click it again (expand), `await
expect(search).toBeFocused()`. RED on main by construction (focus stays on the
header button). Mutation: revert to the plain `effect` → red there.

### 1c. `ProjectRosterCharacter.tags` typed `string[]` though the server passes a stored `null`

**Quoted:** "`ProjectRosterCharacter.tags` typed `string[]` though the server
passes a stored `null` (the card's `?? []` covers it)".

**v5 today:** `core/core-contract.ts:4177-4185` `tags: string[]`; reads at
`project-characters-card.ts:300` (`char.tags ?? []`). Server:
`crates/quilltap-core/src/api/projects.rs:385-388`
`entry.insert("tags", char.get("tags").cloned().unwrap_or(json!([])))` — a
present JSON `null` passes through as `null`.

**v4:** `app/api/v1/projects/[id]/actions/project-crud.ts:48-55` returns
`tags: char.tags || []` — v4 NEVER emits `null`. v4's client type
`app/prospero/[id]/types.ts:10-22` `ProjectCharacter.tags?: string[]`.

**Divergence:** it is the SERVER that diverges (v5 can emit `tags: null`; v4
coerces every falsy to `[]`); the SPA type `string[]` matches v4's wire.
Whether any v5 character read actually yields a `null` tags cell is NOT
MEASURED (a vault-backed or legacy row would be the candidate).

**Recommendation:** NO SPA hunk. Hand the `|| []` coercion to a Rust smalls
lane (`api/projects.rs` `enrich_project`, a `projects_routes` /
`projects_tier2` row with a planted `tags: null`). The SPA keeps `?? []` as a
belt-and-braces read. If the order insists on an SPA hunk, the honest type is
`tags?: string[] | null` with a doc line — but that would document the
divergence rather than close it.

### 1d. The live beat's `toHaveCSS('opacity','0.6')` watch item

**Quoted:** "the live beat's `toHaveCSS('opacity','0.6')` would read 1 under a
hovering pointer (watch item)".

**v5 today:** `e2e/projects-flow.spec.ts:356-359`. The button class
(`project-characters-card.ts:206`) is `… opacity-60 group-hover:opacity-100
focus:opacity-100 …`. The last pointer action before the assert is the
conditional header click (`:347-349`) or none (after `page.goto`); the pointer
then sits on the header, not on a `.group` tile, so today it passes; a layout
shift putting a tile under the pointer would read `1`.

**Predicted hunk (deterministic, one line):** `await page.mouse.move(0, 0);`
immediately before `:359` (and the button is never keyboard-focused there, so
`focus:` cannot fire). The P4.D247 M6 mutation (`opacity-60` → `opacity-0`)
still reds at `:359`. Never seen red; this is a hardening, not a fix.

### 1e. Network failure / unreadable body toast v5's transport sentence (document; NO-PORT)

**Quoted:** "a network failure or unreadable body toasts v5's transport
sentence where v4 shows `Failed to fetch` / its fixed sentence (v5's transport
convention — the card's 'plain `Error` → message' arms model a path
`CoreClient` never takes)".

**Measured:** v5's transports synthesize three sentences —
`Connection lost. The server may still be starting.` (+ ` (<cause>)`),
`The server returned an unreadable response (HTTP <n>).`,
`The server returned an unexpected response (HTTP <n>).` / `(IPC).`
(`core-transport.ts:96,103,109`; `tauri-core-transport.ts:54,59`) — all as a
`CoreDispatchError` of kind `internal`. Consequences after 1a lands: toggle /
remove / H1–H9 show v4's FIXED sentence on a network failure (v4: `Failed to
fetch`); ADD (`project-characters-card.ts:389-396`) shows the transport
sentence (v4: `Failed to fetch` on reject; `Failed to add character` on an
unreadable non-OK body, `res.json().catch(() => null)` at
`useProjectDetail.ts:280-281`).

**Ruling:** NO-PORT by v5's transport convention (one synthetic sentence per
transport failure, app-wide; `Failed to fetch` is a browser string v5 never
surfaces). Record it in the order's Tier 3 so nobody "ports" it. The only
change worth making: say so in the doc comment at
`project-characters-card.ts:346-351` (its "a thrown `Error` (v4's fetch
reject) its own message" sentence describes a path `CoreClient` never takes).

### 1f. Items 11–13 + the `structure` banner + `waitForHealth`

11 (console lines) NO-PORT by convention; 12 (character link route) v5 idiom;
13 (no recorder) — re-confirmed: v4's picker logic is inline in
`CharactersCard.tsx` with no export. The `structure` banner: none in v4.
`waitForHealth` accepting only 200/423 lives in each spec's own helper
(`projects-flow.spec.ts`, `wardrobe-flow.spec.ts`, … ten files) and in
`global-setup.ts` — **out of this lane** (global-setup is forbidden; no beat
yet boots a damaged fixture). Leave as recorded.

---

## 2. P4.D250's OPEN item — the standing-inform beat's greeting wait

**Quoted** (`work-orders/p4.d250-standing-informs-spa-checkbox-chip.md:5`):
"the greeting wait (`toHaveCount(1)` on the streamed text) could resolve
before the greeting turn settles — it fails LOUDLY if the send meets the
still-speaking refusal, so it cannot pass falsely, but a `chatGet` poll for
the saved greeting row would close it." (Also `phase-4.md:7343` "the beat's
greeting wait on the saved row".)

**v5 today:** `e2e/salon-inform-flow.spec.ts:318-322`:
```ts
await page.goto(`/salon/${chatId}`);
await expect(page.locator('.qt-chat-messages-list')).toBeVisible({ timeout: 15_000 });
// The greeting turn the create drew must settle before anything else, or
// the send below meets the "still speaking" refusal.
await expect(page.getByText(MOCK_LLM_REPLY)).toHaveCount(1, { timeout: 30_000 });
```
The beat already owns a raw `dispatch()` helper (`:266-279`, the
`new-chat-flow` idiom) and already polls `chatGet` for a saved row
(`:368-387`). The "still speaking" refusal is CLIENT-side: `busy()` at
`screens/salon/salon-conversation.ts:3576-3580` (send door) and
`chat/chat-composer.ts:970-975` (Enter), sentence `One moment — the room is
still speaking. Your remark waits in the composer.` While `busy()` the
composer renders `Stop generating` INSTEAD of `Send message`
(`chat-composer.ts:507-526`).

**Sibling precedent:** `e2e/salon-attachment-only-send-flow.spec.ts:121-125`
waits on the greeting text then `expect(sendButton).toBeDisabled({ timeout:
15_000 })` — which only resolves once `Send message` is rendered, i.e. once
`busy()` is false. That is the settle proof the inform beat lacks.

**Proposed replacement** for `:320-322` (server truth, then client truth):
```ts
// The greeting turn the create drew must settle before anything else: its
// row SAVED (server) and the composer back to Send (client `busy()` false),
// or the send below meets the "still speaking" refusal.
await expect
  .poll(
    async () => {
      const chat = (await dispatch({ type: 'chatGet', chatId }))['chat'] as
        { messages?: Array<{ role: string; content?: string }> } | undefined;
      return (chat?.messages ?? []).some(
        (m) => m.role === 'ASSISTANT' && (m.content ?? '').includes(MOCK_LLM_REPLY),
      );
    },
    { timeout: 30_000 },
  )
  .toBe(true);
await expect(page.getByRole('button', { name: 'Send message' })).toBeVisible({ timeout: 15_000 });
```
Trap to note in the order: a `toHaveCount(0)` on `Stop generating` would
resolve on its first poll (memory `e2e-tohavecount-resolves-on-first-matching-
poll`); assert the PRESENCE of `Send message` instead. Whether the greeting
drives the client `busy()` at all (vs arriving only by the realtime refetch)
is NOT MEASURED — the two waits are correct either way. Whether a
no-user-seat room chains a second turn after the GREETING (it chains after a
user send, `:353-358`) is NOT MEASURED; the poll's `.some` tolerates either.
Optional sibling: none — `salon-attachment-only-send-flow` already has the
client half; adding the server half there is not asked.

**Proof:** run the file alone (one Playwright run, port 4319). No mutation can
red it on a fast machine (the race window is timing); record it as a
hardening.

---

## 3. P4.D252 — confirm "Still OPEN: nothing"

`work-orders/p4.d252-impersonation-voice-mode-spa-draft-stage-radios.md:5`:
"**Still OPEN:** nothing." CONFIRMED. Lane record (`status-log.md:165663-
165846`) carries no watch item; one recorded v4-faithful quirk (a picker
change with no open rehearsal still sets `stage` to `draft`, v4's
unconditional `dropStaleProposal`; nothing renders it) and one refuted
contract premise (`chat-settings.api.ts:80-93` appends no server sentence).
Its full Playwright run read 349 / 7 / 10 — the seven the Salon-streaming
cluster, each green alone (see §4b). The unification run read 360 / 0 / 6,
"no intermittent fired". Nothing for P4.152.

---

## 4. Other SPA items in the newest NEXT lists / standing notes

### 4a. NEXT lists, "Deliberately left out", dogfood standing notes

- `phase-4.md:7158-7178` (`07b8f0209`) NEXT 2: `repository-zod-messages.ts`
  row, TEXT/REAL ensure cells, `RefusingTextExtractor` wording, a PDF wizard
  corpus — all `crates/**`/harness. No SPA item.
- `phase-4.md:7315-7344` (`52d6e7ecd`) NEXT 3: restore/import/db items +
  "the beat's greeting wait on the saved row" (= item 2 above). Only SPA item.
- `phase-4.md:7446-7465` (`e5c6bd0c0`) NEXT 2: "the error-toast leak on the
  other project-detail handlers, the picker refocus on re-expand (P4.D247)"
  (= 1a, 1b). Rest crates.
- `phase-4.md:7609-7630` (`f6426e196` recorded-divergences) NEXT 3: all
  crates. P4.145 (the SPA lane of that round) left Tier 3 items 9–10 OPEN
  (`work-orders/p4.145-*.md:305-310`): 9 "No UI for `fileProcessing`" (v4
  has none — NO-PORT, recorded); 10 an injected-frame beat "Named, not
  written" (adds nothing visible). Neither belongs in P4.152.
- "Deliberately left out" paragraphs (`:7279-7284`, `:7412-7419`,
  `:7578-7584`, `:7812-7820`): the `structure` banner (none in v4) is the
  only SPA mention.
- `dogfood-findings.md` standing notes from `:625`: #141/#142 restore order
  and #140 import-warning text — both `crates/**`. Note: standing note 3's
  "B4's `Every character is already on the roster.` (e2e-asserted)" is
  WRONG — the sentence is asserted only in vitest (`projects.spec.ts:761`,
  `:772`), never in `e2e/`. Optional: one line in the 1b beat extension
  (the fixture roster [Aria, Cleo] + Bram/Diana/… means it cannot render
  there without adding every character; NOT worth it — record the
  correction instead).
- Older standing notes that look SPA-open but are PHANTOMS on `main`:
  #50 (Project library e2e) — table row says FIXED; #107 (`qt-markdown-field`
  inline host) — `styles/qt-components/_surfaces.css:1245` now carries the
  rule (the table row is stale); #106 (duplicate optimistic bubble) — closed
  by P4.66 (`status-log.md:125263`, the `salon-optimistic-bubble-reconcile`
  beat). None for P4.152.

### 4b. The two Playwright intermittents — watch counters

- **`salon-regenerate-stream-flow.spec.ts`** (3 beats): first named at the
  P4.113 round (`status-log.md:148305-148325` — main itself measured 3/3,
  2/3, 2/3 alone; the `2/3` swipe counter at `:101` the named watch item).
  P4.127–132 round (`:157448-157466`): 2 reds in the full suite, 2/3 four
  times by file then 3/3 twice — "its counter is now 4 of 6 by file on this
  tree". Since: `52d6e7ecd` unification 3/3 TWICE alone (`:165110`); P4.D252
  lane 3/3 alone (`:165824`); `07b8f0209` unification full suite 360/0/6,
  no intermittent. Cause recorded as the spec's own luck windows (the 15 s
  plate poll it documents, the strip's 30 s `toHaveCount(0)`), no server
  stream error.
- **P4.66 optimistic bubble** (`salon-optimistic-bubble-reconcile.spec.ts`):
  red once per full suite in several rounds (`:141655`, `:149850`,
  `:156377`, `:159886`, `:165110`), green alone every time (2/2).

**Recommendation:** both STAY WATCH ITEMS. A deflake needs a cause, and the
records attribute both to beat-internal timing windows under full-suite load
with no server defect; a P4.152 lane cannot reproduce them deterministically
alone, and the newest unified run was clean. If the order wants anything, it
is a counter line in the round record, not a code change.

---

## 5. Mechanics the order must cite

- **Gated beats:** a named `const P4D249_SERVER_LANDED = true;`
  (`salon-inform-flow.spec.ts:48`, with `P4D205_SERVER_LANDED` at `:38`) and
  `test.skip(!CONST, '<reason>')` at the top of the beat (`:259-262`). The
  doc block (`:19-29`) explains why a constant beats a capability probe (the
  `P49K2_SERVER_LANDED` precedent). P4.152's beats need NO gate (every
  server path exists on main).
- **Playwright:** `playwright.config.ts:15-18` `fullyParallel: false`,
  `workers: 1`, `timeout: 30_000`; the shared server is port **4319**
  (`e2e/support/env.ts:44`), repo-wide — **one Playwright run at a time on
  the machine**. `projects-flow.spec.ts` boots its OWN server on **4325**
  (`:33-34`); check 4319/4325 free before running. Run by FILE
  (`npx playwright test e2e/projects-flow.spec.ts`), never `--grep` on a
  serial spec.
- **SPA gates:** `npm run lint` (qt-class guard + self-test), `npm run build`
  (the only real type-check — `tsc` checks nothing), `npm test` (lint +
  `ng-run.mjs test --watch=false`). Never bare `ng test` (watch mode never
  exits). `package.json:6-14`.
- **SPA oracle recorders** live outside `src/` in `apps/web/oracle/`
  (`*.recorder.ts` — `mention-typeahead`, `openai-image-options`,
  `progressions-schema`, `carina-parser`, `download-utils`, `pascal-progress`,
  `scenario-builder`, `transcript-reconcile`). **None applies here**: every
  item is component-inline in v4 (no exported function), so vitest arms
  quoting v4's bytes + the live beats are the proof (the P4.D247 "differential
  requirement, restated").
- **Prettier:** file by file on touched hunks only (memory
  `prettier-write-a-directory-is-never-a-noop`).
- **Version bump:** SPA `package.json` version (main 0.5.805 per the
  `07b8f0209` round).

---

## §Ownership proposal

**Edit:**
- `apps/web/src/app/screens/prospero/project-detail.ts` (H1)
- `apps/web/src/app/screens/prospero/cards/project-model-behavior-card.ts` (H2–H4)
- `apps/web/src/app/screens/prospero/cards/project-image-generation-card.ts` (H5–H8)
- `apps/web/src/app/screens/prospero/cards/project-chats-section.ts` (H9)
- `apps/web/src/app/screens/prospero/cards/project-characters-card.ts` (1b refocus; 1e doc comment)
- specs: `apps/web/src/app/screens/prospero/projects.spec.ts`,
  `cards/project-model-behavior-card.spec.ts`,
  `cards/project-chats-section.spec.ts`
- e2e: `apps/web/e2e/projects-flow.spec.ts` (1b re-expand focus, 1d
  `mouse.move`), `apps/web/e2e/salon-inform-flow.spec.ts` (item 2)
- `apps/web/package.json` (version only)

**Must NOT touch:** `apps/web/e2e/global-setup.ts`, `apps/web/e2e/support/**`
(incl. `mock-llm.ts`, `env.ts`), `apps/web/src/app/core/core-transport.ts` /
`tauri-core-transport.ts` / `core-client.ts` (the transport convention, 1e),
`core-contract.ts` (no type change proposed — 1c), any `crates/**` path
(1c's `api/projects.rs` coercion belongs to a Rust lane), any harness family
or fixture (none read `apps/web/**`).

**Harness families / oracle cases / fixtures owned:** none.

## §Open questions

1. **1c:** hand `tags: char.tags || []` to a Rust smalls lane
   (`api/projects.rs:385-388`)? (Recommended; the SPA then needs nothing.)
2. **The chats section's inline load error** (`project-chats-section.ts:157-
   158`, `:178-179`) — v4 is silent on both paths. Keep v5's banner (a v5
   addition, recorded) or retire it to v4's silence? Not in item 10's scope.
3. **`onUnlinkStore`'s toast** (`project-detail.ts:382-399`) — v4's
   `unlinkStore` never toasts. The comment records it as a deliberate v5
   addition; confirm it stays.
4. **1e** recorded NO-PORT — confirm the human is content that a network
   failure on a per-field save now reads v4's fixed sentence (not
   `Failed to fetch`, not v5's `Connection lost…`).
5. Should the two intermittents get a counter line only (recommended), or is
   a dedicated deflake lane wanted later?
