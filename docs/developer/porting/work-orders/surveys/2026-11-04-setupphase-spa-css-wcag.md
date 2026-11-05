# Survey — CSS / themes / WCAG (`29ba4d3f8`, `87c3860df`, `94f041764`, `705b954c2`), the Salon SPA rows (`3c41cf3da`, `4f5047241`, `671c59a53`, `27fb3ec77`, `df8d4d70a`), About credits, and an INVENTORY of every SPA half of the server rows

## 0. Header

- **Date:** 2026-11-04. **Baseline:** v4 `236c04e35`. **Pin:** v4 `624d0d25c` (main, `4.13.0-dev.8`). Every v4 byte was read with `git show <sha>` / `git show 624d0d25c:<path>` / `git diff 236c04e35 624d0d25c`. The v4 working tree has unrelated dirt and was never read. **v5 main:** `ff1651177` (SPA 0.5.971).
- **Read-only.** Nothing was built and no test was run. Every v5 "measured" claim is a grep, read or `patch --dry-run` over main at `ff1651177`. Scratch copies live in this session's scratchpad.
- **Method for the CSS:** the v4 files at the baseline and the pin were extracted, `diff -u base pin` was taken per file, and each diff was dry-run against v5's vendored file.
- **Inventory rows (§4):** two read-only sub-surveyors produced these. I spot-checked their v5 anchors (`storyteller-board.ts:261-268`, `core-contract.ts` gaps, `chat-composer.ts` hosts); treat any anchor I did not re-read as theirs.
- **Lanes informed:** the CSS/WCAG lane, the Salon-spine SPA lane, the Salon-composer SPA lane, the item-browser lane, the document-browser lane, the Document-Mode lane and the SPA-smalls lane proposed in §7. It also states the server contracts each SPA lane consumes (§6).

**Headline facts a planner must not miss**

1. **The whole qt-components re-vendor applies MECHANICALLY: 72 of 74 hunks dry-run clean onto v5's files.**
   - Per file: `_chat` 28/30, `_content` 4/4, `_icons` 1/1, `_intervention` 1/1, `_layout` 2/2, `_storyteller` 1/1, `_surfaces` 22/22, `_utilities` 1/1, `_workspace` 2/2. `_index`, `_interactive`, `_roleplay`, `_typography` and `_variables` have no base→pin delta.
   - The two `_chat` rejects are context misses, and v5 still carries the old bytes for both:
     - the message-content link colour at v5 `_chat.css:861/867`;
     - the Document Mode header block at v5 `:2412-2445`.
   - **Do not blind-patch, though.** v5's own `.qt-rich-editor-content` block (P4.6ag, the ProseMirror twin of the Lexical rules) duplicates the old Lexical bytes. The fuzz landed one Lexical hunk on the WRONG copy: after applying, v5 `:1189` stayed old.
   - Apply by hand. The ported checker (2) is the proof.
2. **`87c3860df` fixes v4 bugs v5 inherited verbatim, and v5 added more of the same.** About 105 `var()`s named properties that are defined nowhere:
   - shadcn-style `hsl(var(--primary))` / `hsl(var(--border))` / `hsl(var(--muted-foreground))`;
   - `var(--qt-text-primary)` / `--qt-border` / `--qt-primary` / `--qt-text-secondary` / `--qt-text-muted`.

   The help guide (`_surfaces.css:511-972`) and the Document Mode chrome (`_chat.css:2400-2445`) therefore painted with nothing. v5 vendors all of them (46 hits of the five shadcn shapes alone).

   v5-ONLY rules copy the defect too. Applying v4's patch leaves:
   - `_chat.css:1323` (`.qt-rich-editor-content blockquote`, `hsl(var(--border))` fallback);
   - `_chat.css:2444` (`.qt-doc-header-button-active`, `var(--qt-accent, var(--qt-text-primary))`; `--qt-accent` is undefined everywhere in v5);
   - v5's copy of the Lexical checklist at `:1189`.

   **Recommend porting v4's `scripts/check-css-vars.mjs` (NEW at `87c3860df`, 196 lines) as `apps/web/scripts/check-css-vars.mjs`**, with `--self-test`, wired into `npm run lint` and `npm test` beside `check-qt-classes.mjs` / `check-a11y-names.mjs`. Retarget it to v5's homes: `src/**/*.css`, `src/styles.css`, `public/themes/*/styles.css`, and `--name` string literals in `src/app/**/*.ts`. Its red is the list of every v5-only residue. **Ruling R1** (§9).
3. **`705b954c2` (`@theme inline` → `app/styles/theme-tokens.css`) is NO-PORT.**
   - Measured: the moved block is byte-identical to the baseline's `globals.css` block.
   - v5 does not vendor `globals.css`. Its ONE home is `apps/web/src/styles.css`, which already carries `@variant dark` (`:5`) and the `@theme inline` block (`:172-221`). That block is identical to v4's at the pin except for four section comments.
   - The rest of `705b954c2` is theme-storybook generation (v5 has no storybook).
   - The ONE `globals.css` hunk that IS a port lives in `4f5047241`: the `::highlight(quilltap-conversation-search)` rule (`--qt-highlight-bg` / `--qt-highlight-fg`). It goes into `src/styles.css`.
4. **The five bundled pack `styles.css` diffs dry-run clean on v5's packs**, including the two adapted packs (art-deco and great-estate differ from v4 only by the `/themes/<id>/…` asset URLs).
   - `tokens.json` does NOT move at the pin, so `theme-contrast.residues.json` (59 rows) does not move.
   - **`theme-contrast.spec.ts:44-51` `PACK_MANIFESTS` DOES move**, since all six `theme.json` versions bump:
     - art-deco 1.0.13→**1.0.14**
     - earl-grey 1.3.10→**1.3.11**
     - great-estate 1.0.10→**1.0.11**
     - madmans-box 1.1.9→**1.1.10**
     - old-school 1.0.14→**1.0.15**
     - rains 1.3.14→**1.3.15**
   - madmans-box `theme.json` also gains `"deus-ex-machina": "icons/deus-ex-machina.svg"`. v5's madmans-box `icons/` ships only 4 of v4's 88 files (brand, hand-raised, inventory, stop-sign). The precedent (P4.D336) is to vendor the referenced SVG.
5. **The axe baseline (`e2e/a11y/known-violations.json`, 521 rows) mostly does NOT move.**
   - 420 of its 443 `color-contrast` rows are the five `.qt-queue-badge-*` badges × 6 packs × 2 modes × pages. No pin hunk touches `qt-queue-badge`. They stay, and their owner tag `P4.D357` is now stale (that lane landed). Re-own them (§9 R6).
   - **Expected to clear:**
     - the 23 `earl-grey|dark` rows (`qt-meta` ×8, `qt-card-subtitle` ×7, `qt-card-description` ×5, three nav `a[routerlink]`), via `94f041764`'s Earl Grey dark `--text-400` 59%→61% ("incl. on tab/hover fills");
     - the **12 `FOLLOW-UP` `target-size` rows** on `character-card.ts:87`, via `94f041764`'s `inline-flex items-center justify-center min-w-6 min-h-6` on the favourite / Carina / play buttons (`AuroraView.tsx:563-588` at the pin).
   - The 66 `RULE v4-faithful ParticipantCard` rows stay.
   - The unifier re-baselines on the union with `QT_AXE_REBASELINE=1`.
6. **The toolbar breadcrumb (`3c41cf3da`) lands on a v5 DEFERRAL, not a v5 port.**
   - v5's `chat/conversation-header.ts:34` says "Character links remain a deferral". v5 also has no story-background thumbnail in the header.
   - v4's new `ChatHeaderBreadcrumb.tsx` (283 lines, NEW) consolidates both and adds three densities (full / abbrev / avatar), which it measures in invisible copies.
   - Porting it closes the deferral. Its two pure functions (`abbreviateName`, `chooseDensity`) carry a v4 jest test (`ChatHeaderBreadcrumb.test.tsx`, 58 lines), so they are tier-1 oracle candidates.
7. **`4f5047241` closes another named v5 deferral.** `search/search.logic.ts:20-23` reads: "`/salon/{id}?msg={messageId}` → unchanged (a named deferral: v5's Salon has no `?msg=` message-anchor affordance…)".
   - v4 now ships the affordance and, on top of it, a "This conversation" group in the header search bar.
   - `lib/chat/conversation-search.ts` (134 lines, NEW, pure) has a v4 jest test of 86 lines, so it is a tier-1 twin.
   - **No server contract.** It reads the transcript the Salon already holds.
8. **`671c59a53` is CONVERGED BY DESIGN in v5, except for one clause.**
   - v5 already has ONE predicate, `isAnnouncementChip` (`chat/chat-view-model.ts:293-298`), used for both grouping and row rendering, and it already carries v4's three carve-outs (carina, `suparna`/`mail-delivery`, `pascal`/`custom-tool-result`).
   - The only delta is v4's fourth arm, `isEstablishedFactAnnouncement`, which rides the Deus row (`db6190b44`) and goes in the composer lane.
9. **`df8d4d70a`'s non-Deus half replaces the native `title=` with Quilltap `<Tooltip>` across the composer.**
   - v4 title→Tooltip counts, base→pin:
     - `ChatComposer` 11→1 / 0→11;
     - `ComposerGutterTools` 6→0 / 0→6;
     - `FormattingToolbar` 9→2 / 0→7;
     - `RngDropdown` 3→0 / 0→3;
     - `PendingInformChips` 2→0 / 0→2;
     - `CustomToolsButton` 1→0 / 0→1;
     - `SpeakingAsAvatar` 1→0 / 0→2.
   - v5 has zero `<qt-tooltip>` in those homes today: `chat-composer.ts` (17 title bindings), `editor/formatting-toolbar.ts` (8), `rng-dropdown.ts` (3), `pending-inform-chips.ts` (2), `custom-tools-popup.ts` (5), `speaking-as-avatar.ts` (1).
   - **No e2e selects a composer control by title.** The `[title="Speak as …"]` selectors in `salon-dialogs-flow` / `salon-impersonation-voice-flow` are on participant cards, not the composer.
   - About 33 vitest lines across the six specs assert `title`.
10. **About credits are more than two lines.** Four commits move v4's `AboutView.tsx` in the span (`db4a76e39`, `856ed4f2b`, `c34986421`, `70d2859d8`). v5's `screens/about/about-page.ts` mirrors v4 verbatim (P4.D359: "v4 `AboutView.tsx:373-383` at `236c04e35`, verbatim"), so all ten deltas are byte ports (§3.4).
    - Five of them describe features this catch-up ports: Deus, conversation-first search, the shared folder browser, read-only viewers, and the Brahma lookups / engine.
    - **Ruling R5:** land the About text with the union (one owner), not per feature.

---

## 1. Per commit, the CSS / theme hunks (P = pin line)

| commit | v4 file:line @ pin | what | v5 file (applies?) |
|---|---|---|---|
| `29ba4d3f8` | `_surfaces.css:47-58` | `.qt-card:has([aria-haspopup][aria-expanded="true"]), .qt-card-interactive:has(…) { position: relative; z-index: 20; translate: none; }` + why-comment | `_surfaces.css` (clean) |
| `29ba4d3f8` | `_utilities.css:60-68` | NEW `.qt-bg-popover { background-color: var(--qt-popover-bg); }` + comment. `--qt-popover-bg` already defined in v5 `_variables.css:308` | `_utilities.css` (clean) |
| `87c3860df` | `_chat.css` 43/43, `_surfaces.css` 64/64, `_content.css` 1/1 | substitutions → `var(--color-primary)`, `--color-border`, `--color-foreground`, `--color-muted-foreground`, `--color-muted`, `--color-accent`, `--color-destructive`; help-guide hooks keep their `--qt-help-*` names with valid fallbacks | 28/30 `_chat` hunks clean, rest clean; see headline 1–2 |
| `87c3860df` | `scripts/check-css-vars.mjs:1-196` (NEW), `CLAUDE.md`, `help/help-chat.md` +2 | the guard (rules: defined somewhere / per-theme hook fallback resolves recursively / no `hsl(var(--color-*|--theme-*))` / `css-var-exception` escape); `help-chat.md` is `help/` vendoring (the help lane) | NEW `apps/web/scripts/check-css-vars.mjs` (R1) |
| `94f041764` | `_chat.css:317-322, :354-356, :421-426, :833-838` | `:focus-visible` rings on `.qt-chat-system-bar`, `.qt-chat-announcement-chip`, `.qt-mention-chip`; the system-bar timestamp loses `opacity: 0.75` | `_chat.css` (clean) |
| `94f041764` | `_content.css:~110-120` | inactive tab fill 85/15→90/10 (light), 78/22→84/16 (dark) | clean |
| `94f041764` | `_content.css:1487-1498` | `.qt-settings-toggle-row:hover` + `:where(*)` → `var(--qt-card-hover-fg, var(--color-accent-foreground))` | clean |
| `94f041764` | `_content.css:1732-1752` | `:is(.qt-alert…) :is(.qt-text-small, .qt-text-muted, .qt-text-secondary, .qt-text-xs) { color: inherit; }` + alert links underlined | clean |
| `94f041764` | `_layout.css` footer link (`:~976-986`) | `opacity: 0.7` → full strength; hover by colour `var(--qt-footer-link-hover)` | clean |
| `94f041764` | `_layout.css:~329` | dialog close cross `hover:text-accent-foreground` | clean |
| `94f041764` | `_workspace.css:214-235, :254` | NEW `.qt-tab-select` (+ `:focus-visible`, `.qt-tab-strip .qt-tab:has(.qt-tab-select:focus-visible)` ring); close-cross opacity 0.55→0.8 | clean |
| `94f041764` | `_intervention.css:+1`, `_storyteller.css:47`, `_surfaces.css:+1` | hover fg → accent-foreground | clean |
| `3c41cf3da` | `_layout.css:713-735` | `.qt-page-toolbar-left { flex: 1 1 0% }`, `.qt-page-toolbar-left:empty { flex: 0 1 auto }`, center `flex: 0 1 32rem`, right `flex-shrink: 0` | clean |
| `db6190b44` | `_chat.css:544-597` (54 lines) | `.qt-chat-established-fact*` (row, band, ribbon, accent) | clean (rides with the re-vendor; consumed by the composer lane — §6.1) |
| `df8d4d70a` | `_chat.css:594-634` (41 lines), `_icons.css:130` | `.qt-chat-established-fact-toggle` (+ hover, focus-visible, `[aria-pressed="true"]`, `:disabled`); `[data-icon="deus-ex-machina"]` mask | clean |
| `4f5047241` | `app/globals.css` (+14) | `::highlight(quilltap-conversation-search) { background-color: var(--qt-highlight-bg); color: var(--qt-highlight-fg); }` | → `apps/web/src/styles.css` (append after the `@layer base` block, as v4 does) |
| `705b954c2` | `app/globals.css` / `app/styles/theme-tokens.css` | relocation | **NO-PORT** (headline 3) |
| `cce44e098`-era packs | — | — | already at the baseline (P4.D357) |

**Theme packs** (`git diff 236c04e35 624d0d25c -- themes/bundled/`, 12 files, +55/−30, all `94f041764` except the madmans-box icon):

- **art-deco** `styles.css`:
  - `:84` `--qt-alert-success-fg` 22%→18%.
  - `:339-357` light `--qt-link-hover-color` / `--qt-footer-link-hover` → `var(--color-primary)`; `--qt-brand-fg` → `var(--deco-bronze)`.
  - `:473-478` dark restores gold.
- **earl-grey**:
  - `:170` dark `--text-400` 59%→61%.
  - `:273` light `--text-400` 45.5%→43%.
  - `:614`, `:626` alert fg darkened.
  - `:1272-1297` four status texts → `var(--qt-alert-*-fg)`.
- **great-estate**:
  - `:86-90` alert success/warning fg.
  - `:334-350` light hover / footer / brand → `hsl(43 70% 28%)`.
  - `:466-471` dark restores `--estate-gold`.
- **old-school** `:58` alert success fg.
- **rains**:
  - `:175` `--text-400` 59%→61%.
  - `:722` alert success fg.
  - `:1215-1240` four status texts → alert tokens.
- **madmans-box** `theme.json` (`+deus-ex-machina`) and NEW `icons/deus-ex-machina.svg` (5 lines).
- **All six** `theme.json` version bumps (headline 4).
- `help/themes.md` +12 ("Legibility under every lamp") and `help/tabbed-workspace.md` +6 go to the help-vendoring lane.

## 2. The Salon SPA rows

### 2.1 `3c41cf3da` — compact header breadcrumb (12 files, +433/−110)

- **v4:**
  - `components/chat/ChatHeaderBreadcrumb.tsx:1-283` (NEW):
    - `abbreviateName` `:44-57`: one word → its first 3 letters (or the word, if ≤4 letters); several words → initials with `MINOR_WORDS` dropped, max 4.
    - `chooseDensity` `:60-65`; `TITLE_MEASURE_MAX_PX = 256`.
    - The compact link wears a Tooltip (`qt-tooltip-title` + hint "Click to open project" / "Click to open character").
    - Compact `aria-label`s: `Project: ${name}` / `${name}`.
    - Story background button: `aria-label="Display story background"`, Tooltip "Click to display background".
    - The title is wrapped in a Tooltip with `anchorClassName="min-w-32 shrink-[1000]"`; `CopyChatIdButton variant="inline"`.
    - The measuring copies are `absolute … invisible pointer-events-none`, `aria-hidden`, `inert`.
    - `ResizeObserver` drives the recompute.
  - `SalonView.tsx:1218` hosts it; the commit is −153 / +… in `SalonView`.
  - `_layout.css` (§1).
  - `help/chats.md` ±6 (help lane).
  - Test: `__tests__/unit/components/chat/ChatHeaderBreadcrumb.test.tsx` (58).
- **v5 home:**
  - `apps/web/src/app/chat/conversation-header.ts:41-121` (template), hosted at `screens/salon/salon-conversation.ts:479`. Its first order was the P4.6a-series Salon; P4.9H1 moved its four entries to the sidebar.
  - v5 renders the header INLINE, not in the page toolbar (`layout/page-toolbar.service.ts:17`).
  - Today it holds the project link → title → Concierge pill → spacer → Inspector → cost summary → copy-id. It has **no character links and no story-background thumbnail** (the deferral at `:34`).
- **Port shape:**
  - A NEW `chat/conversation-breadcrumb.ts` (+ spec) with the three densities and a `ResizeObserver` (`afterNextRender` / `DestroyRef`).
  - A pure `chat/breadcrumb-density.ts` (`abbreviateName`, `chooseDensity`) with an oracle spec over a v4 jest-derived fixture.
  - `conversation-header.ts` hosts it, keeping the right-hand cluster.
  - Inputs needed: participant characters (`id`, `name`, `avatarUrl`), all already on `ChatDetail` / the cast, and the story-background URL, which `salon-conversation.ts` already resolves for the pane.
  - **No server contract.**
  - Route mapping: `/aurora/{id}/view?tab=conversations` → `/characters/{id}?tab=conversations` (the `mapResultUrl` precedent); `/projects/{id}` → `/prospero/{id}`.
- **Specs that move:** `chat/conversation-header.spec.ts`. e2e that read the header: `salon-concierge-three-state-flow`, `concierge-marks-flow`, `salon-autonomous-entry`, `salon-try-uncensored-flow`. Each selects by role or text; check the title link, which becomes tooltip-wrapped.
- **Ruling R3:** whether the inline header keeps the cost/Inspector cluster (v5 placement) beside the breadcrumb. Recommend YES: v4's toolbar keeps them in the right slot.

### 2.2 `4f5047241` — "This conversation" search + `?msg=` scroll (27 files, +943/−146)

- **v4:**
  - `lib/chat/conversation-search.ts:1-134` (NEW, pure):
    - `CONVERSATION_SEARCH_MIN_QUERY = 2`, `toSearchableText`, `escapeRegExp`, `conversationQueryPattern` (`'giu'`).
    - `findConversationMatches`: newest first, skips `TOOL` rows, `SNIPPET_LEAD 40` / `SNIPPET_LENGTH 140`, `…` ellipses, `occurrences`.
  - `lib/chat/message-navigation.ts`:
    - `PENDING_REVEAL_KEY='pendingMessageReveal'`, `MESSAGE_REVEAL_EVENT='quilltap:reveal-message'`.
    - `requestMessageReveal(chatId, messageId)` (sessionStorage + `CustomEvent`), `takePendingMessageReveal(chatId)`, `revealRequestFromSalonHref`.
    - The old `navigateToMessage` / `getPendingMessageNavigation` are deleted.
  - `components/search/conversation-matches.tsx` (NEW, 66):
    - `role="group" aria-label="Matches in this conversation"`; header "🔖 This conversation (n or n/total)".
    - Empty text: `Not a whisper of "{query}" in this conversation`.
    - Rows are `div role="button" tabIndex=0` + `activateOnKey`; "Unknown speaker"; an `Announcement` badge `qt-badge-info`; `×n`.
    - `formatWhen` = `toLocaleString(undefined,{month:'short',day:'numeric',hour:'numeric',minute:'2-digit'})`.
  - `components/search/search-bar.tsx`:
    - `:200` placeholder `conversation ? 'Search this chat & all… (⌘K)' : 'Search... (⌘K)'`; `pr-9`→`pr-8` plus a clear button.
    - `:167` two-stage Escape (close the dropdown, then clear the query and the highlight, then blur).
    - `:225` `<ConversationMatches>` above the global results; limit 5.
  - `components/search/search-dialog.tsx` (+52, the same group in the dialog).
  - `app/salon/[id]/hooks/useSearchHighlight.ts` (NEW, 87): the CSS Custom Highlight API (`CSS.highlights`, named `quilltap-conversation-search`).
  - `SalonView.tsx`:
    - `:1419` `revealMessage`: the whisper / not-on-page toasts, verbatim: "That line was a whisper meant for other ears. Choose “Show all whispers” in the sidebar to see it." / "That line isn’t on the page — it may belong to another version of a reply, or have been tidied away."
    - It expands a chip group, then settles by rAF (40 attempts, a 600-frame hidden wait, `-12px` offset) and flashes `qt-memory-source-highlight` for 3 s.
    - `:1471` event listener; `:1530` the `conversationSearchSource` object.
  - `useAutoScroll.ts`: `isInitialScrollDone`.
  - `page.tsx` takes `?msg`.
  - `components/providers/page-toolbar-provider.tsx`: a `search` slot.
  - `components/workspace/{WorkspaceIntent,WorkspaceLinkInterceptor,tab-toolbar,useWorkspaceNavigate}.tsx`: `?msg=` → `requestMessageReveal`.
  - Tests: `conversation-search.test.ts` (86), `message-navigation.test.ts` (96), `tab-toolbar.test.tsx` (+38).
- **v5 homes:**
  - `search/search-bar.ts:45-101` (P4.9P; `:66` also takes `qt-bg-popover` from `29ba4d3f8`), `search/search-dialog.ts` (`:64` `bg-background` → `qt-bg-popover`), `search/search.logic.ts:20-23` (delete the deferral line).
  - NEW `chat/conversation-search.ts` (+ `.oracle.spec.ts` against a v4-emitted fixture) and NEW `search/conversation-matches.ts`.
  - `layout/page-toolbar.service.ts` gains a `conversationSearch` signal slot.
  - NEW `chat/message-navigation.ts`; NEW `chat/search-highlight.ts` (Custom Highlight API; jsdom lacks `CSS.highlights`, so stub it in the spec).
  - `chat/auto-scroll.ts:55-96` (expose `isInitialScrollDone`; v5 has `isSettled`).
  - `screens/salon/salon-conversation.ts`: the reveal and source wiring; the `modalImage` / list host sits near `:513`.
  - `workspace/chrome/link-interceptor.ts` (P4.80 / P4.84), `workspace/core/route-to-intent.ts`, `workspace/workspace-redirect.guard.ts` (carries only `chatId` today, per `search.logic.ts:21-22`).
  - v5 already defines `.qt-memory-source-highlight` (`_chat.css:3445-3455`).
- **Specs that move:** `search/search-bar.spec.ts`, `search-dialog.spec.ts`, `search.logic.spec.ts`, `search-results.spec.ts`, `chat/auto-scroll.spec.ts`, `workspace/chrome/link-interceptor.spec.ts`, `workspace-redirect.guard.spec.ts`, `screens/salon/salon-conversation.spec.ts`. e2e that read the search bar: `page-toolbar-flow`, `workspace-search-documents-flow`, `new-chat-flow`, `generate-image-flow` (placeholder text). A new beat is owed: Salon search → This conversation → click → row at top, flashed; plus a `?msg=` deep link.
- **Server contract: NONE.** (The global Message result URL `/salon/{id}?msg={id}` is already on the wire.)

### 2.3 `671c59a53` — one never-collapsed predicate

- **v4:** `app/salon/[id]/announcement-render-items.ts:50-66` adds `isAlwaysExpandedAnnouncement` (carina / established fact / suparna `mail-delivery` / pascal `custom-tool-result`), with `isCollapsedAnnouncement` delegating to it. `VirtualizedMessageList.tsx:~327` uses the same rule. Test +48.
- **v5:** CONVERGED BY DESIGN. `chat/chat-view-model.ts:293-298` `isAnnouncementChip` is the one rule, and v5 keeps a chip mounted with its body beneath it, so a chip never leaves its group (`:288-291`). `chat/message-row.ts:139-150` documents the static expanded bar for exactly those senders.
- **Owed:** the `established-fact` arm (`systemSender==='host' && systemKind==='established-fact'`), with the Deus row in the composer lane (§4.3).
- **Specs:** `chat/chat-view-model.spec.ts`.

### 2.4 `27fb3ec77` — avatar opens full size; carried items under the title (15 files, +253/−19)

- **v4:**
  - `components/ui/Avatar.tsx:56-62, :172-182`: `onImageClick` wraps the picture alone in `button.block.cursor-zoom-in` with `aria-label` `` `View ${name}'s picture full size` `` (ignored when `onClick` is set or there is no picture).
  - `message-row/MessageDesktopAvatar.tsx:28-56`, `CarriedGlyphs`:
    - `ul.flex.flex-wrap.justify-center.gap-1.w-32.mt-1`, `aria-label="Carrying"`, `data-testid="salon-avatar-carried"`.
    - Each `li` is a Tooltip (bold name + description, `whitespace-pre-line`) over `span tabIndex=0 role="img" aria-label={name}`.
    - It renders between the title and the badge (`:79`).
  - `components/inventory/inventory-glyph.tsx:23-32`: `inventoryAbbreviation` (two words → two initials; one word → its first two letters/numbers, upper-cased; `?` when empty). It adds `fallback?: 'icon'|'abbreviation'` (`data-testid="inventory-glyph-abbreviation"`) and DROPS the emoji span's `title`.
  - `lib/hooks/use-inventory.ts:233-250`: `useChatCarriedItems` shares ONE query entry with `useChatInventory` (`select` → `inventory[characterId].equipped`).
  - `MessageRow.tsx` / `SalonView.tsx` thread `characterId` and `onImageClick`. Test: `message-desktop-avatar.test.tsx` (98).
  - `help/chats.md` +2, `help/inventory.md` +2 (help lane).
- **v5 homes:**
  - `chat/message-row.ts:96-108` (courier avatar) and `:121-132` (regular avatar), `:529` (the third `qt-avatar` site). The author model needs `characterId`.
  - `ui/avatar.ts:33` (NEW `imageClick` output + the button wrapper).
  - `inventory/inventory-glyph.ts:57-62` (the `fallback` input + the `inventoryAbbreviation` pure fn; P4.D330).
  - `inventory/chat-inventory-data.ts:26-48` (P4.D330): ONE per-chat read. Expose a per-character projection so N avatars cost one `chatInventoryGet`.
  - `screens/salon/salon-conversation.ts:513` already routes `(imageClick)` to `modalImage`; the avatar emits the same `ImageClickEvent` (`message-row.ts:619`).
- **Specs:** `chat/message-row.spec.ts`, `ui/avatar.spec.ts` (if present; otherwise add one), `inventory/inventory-glyph.spec.ts`, `inventory/inventory-data.spec.ts`. e2e: `inventory-flow` (a new beat: the glyph row under a Salon avatar).
- **Server contract:** none new (`chatInventoryGet` exists, `inventory/inventory.api.ts:350`). The realtime refetch is v5's existing `chats` / inventory topic.

### 2.5 `df8d4d70a` — Quilltap tooltips in the composer (non-Deus half)

- **v4:** title→`<Tooltip content=…>` across `ChatComposer.tsx` (`:418` "Dismiss", `:512` "Remove attachment", `:545` "Remove tool result", `:682` "Open terminal", …), `ComposerGutterTools.tsx` (six buttons: "Insert announcement", "Post a letter", "Attach file from library", "Generate image", "Attach file", "Inform the cast"; `aria-label` kept), `FormattingToolbar.tsx`, `RngDropdown.tsx`, `PendingInformChips.tsx`, `CustomToolsButton.tsx`, and `SlashCommandPlugin.tsx:82` (the icon map, Deus).
- **v5 homes:** `chat/chat-composer.ts` (the gutter tools are inline; "Insert announcement" lives here), `editor/formatting-toolbar.ts` (P4.6ax), `chat/rng-dropdown.ts` (P4.9e4b), `chat/pending-inform-chips.ts` (P4.D203+), `chat/custom-tools-popup.ts` (its trigger), `chat/speaking-as-avatar.ts` (P4.D58 / P4.D61), and `ui/tooltip.ts:72` (`qt-tooltip`).
- **Specs:** `chat-composer.spec.ts`, `chat-composer.toolbar.spec.ts`, `editor/formatting-toolbar.spec.ts`, `rng-dropdown.spec.ts`, `pending-inform-chips.spec.ts`, `speaking-as-avatar.spec.ts` (≈33 title assertions). No e2e moves.
- **Interaction with `94f041764`** (same files): `FormattingToolbar.tsx:128-139` `markdownButtonName` (the accessible name = the tooltip words, keeping the glyph when it is not in the words: "Heading 4 (H4)"); `:548-549` the code-block button `aria-label="Code block" aria-pressed`; `RngDropdown.tsx:249` `aria-expanded={isCustomOpen}` (v5 `rng-dropdown.ts:152`). One owner (§7).

## 3. `29ba4d3f8` / `94f041764` / About — the template half

### 3.1 `29ba4d3f8` sites (page-background → `qt-bg-popover`; `qt-dialog`)

| v4 @ pin | v5 site | owner (§7) |
|---|---|---|
| `components/brahma-console/ModelPicker.tsx:70` `qt-bg-surface`→`qt-bg-popover` | `brahma/model-picker.ts:52` | CSS/WCAG |
| `EntitySearchDropdown.tsx:40`, `GenerateImageDialog.tsx:254` `bg-background`→`qt-bg-popover` | `images/standalone-generate-image-dialog.ts:114` | CSS/WCAG |
| `search-bar.tsx:~165`, `search-dialog.tsx:~249` | `search/search-bar.ts:66`, `search/search-dialog.ts:64` | Salon-spine (same files as §2.2) |
| `outfit-quick-pick.tsx:84` `qt-bg-default`→`qt-bg-popover` | `wardrobe/outfit-quick-pick.ts:64` | item-browser |
| `wardrobe-control-dialog.tsx:604` | `wardrobe/wardrobe-control-dialog.ts:403` | item-browser |
| `wardrobe-item-row.tsx` (+ `useFixedMenu`, `role="menu"`/`menuitem`, `aria-haspopup`/`aria-expanded`) | `wardrobe/wardrobe-item-row.ts:210, :248` | item-browser |
| `inventory-item-row.tsx` (−48, onto `useFixedMenu`) | `inventory/inventory-item-row.ts:205` (already viewport-fixed) | item-browser |
| `item-browser/ItemFolderRow.tsx`, `scenarios/ScenarioRow.tsx` | `item-browser/item-folder-row.ts:73`, `screens/scenarios/shared/scenario-row.ts:93-200` | item-browser |
| `settings/roleplay-templates/index.tsx:363, :889`, `tools/tasks-queue/TaskDetails.tsx:61` → `qt-dialog` | v5 renders these through `ui/modal.ts` (`qt-modal`); measure, probably already opaque | CSS/WCAG (measure-only) |
| `hooks/useFixedMenu.ts:1-80` (NEW; `DEFAULT_ROOM 340`, `GAP 4`, flips up, closes on outside scroll/resize) | NEW `ui/fixed-menu.ts` | item-browser |

### 3.2 `94f041764` template sites

| v4 @ pin | v5 site | owner |
|---|---|---|
| `app/aurora/AuroraView.tsx:563-588`: favourite / Carina / play buttons `inline-flex items-center justify-center min-w-6 min-h-6`, `aria-label`s `` `Favorite ${name}` `` / `` `Carina answers (@-queries) for ${name}` `` / `` `You play ${name}` ``, `aria-pressed`, emoji in `<span aria-hidden>` | `screens/characters/list/character-card.ts:87-102` (the 12 axe target-size rows) | CSS/WCAG |
| `CharacterHeader.tsx:166-191` (same three, unparameterised names "Favorite" / "Carina answers (@-queries)" / "You play this character") | `screens/characters/view/character-header.ts:82-94` | CSS/WCAG — ⚠ the kind card (§4.6) adds to `character-detail.ts`, not this file |
| `app/prospero/[id]/components/FilesCard.tsx:86` `aria-expanded` | `screens/prospero/cards/project-files-card.ts` | Document-Mode lane (it rewrites "Browse All", §4.9) — HANDOFF rule W1 |
| `AnnouncementChip.tsx:~126`, `MessageRow.tsx:~339` DROP the `Expand/Collapse ${sender} ${kind} message` `aria-label`s (the visible text names them) | grep finds no such label in v5 (already absent; verify `chat/announcement-group.ts`) | composer lane (verify-only) |
| `app/setup/profile/page.tsx:135-160`: `<p id="setup-role-heading">` + `role="group" aria-labelledby`, buttons `type="button" aria-pressed`, error `role="alert"` | `screens/setup/setup-wizard.ts` (locate the role step) | CSS/WCAG |
| `ChatSidebar.tsx:731` `` aria-label={`${name}${statusLabel}${position ? ` ${position}` : ''}, expand sidebar`} `` | `chat/sidebar/chat-sidebar.ts:677` (`- click to expand sidebar`) | CSS/WCAG |
| `ProgressionsSection.tsx:165`, `PromptList.tsx:62`, `SubpromptsSection.tsx:68`, `LibraryFilePickerModal.tsx:559,622`, `ProfileCard.tsx:36`, `MemoryCascadeDialog.tsx:77-129`: `hover:bg-accent/50` → `hover:qt-bg-surface-hover` (ProfileCard also `hover:qt-text`) | `progressions/progressions-section.ts:141`, `subprompts/subprompts-section.ts:62`, `chat/library-picker/library-file-picker-modal.ts:95-197` (no hover class today), `screens/settings/providers/*` drag handle (locate), `chat/memory-cascade-dialog.ts:47,64` | CSS/WCAG except library-picker (Document-Mode lane, W1) and the provider card (SPA-smalls lane, W1) |
| `FormattingToolbar.tsx:128-139, :511, :548-549` | `editor/formatting-toolbar.ts:54` (format table) | composer lane |
| `InsertAnnouncementDialog.tsx:496` `aria-pressed={characterId === c.id}` | `chat/post-office/insert-announcement-dialog.ts:155-163` | composer lane (Deus edits this file too) |
| `RngDropdown.tsx:249` `aria-expanded` | `chat/rng-dropdown.ts:152` | composer lane |
| `FileBrowser.tsx:454-503`: emoji buttons get `aria-label`s ("Upload files" / `Uploading ${n} of ${t}`, "New folder", "Switch to grid/list view", `` `Clean up ${n} untracked file(s)` ``, "Sync filesystem", "Refresh file list") + `aria-hidden` glyphs | `screens/files/files-browser.ts:~95-125` (`title=` only) | Document-Mode lane (c17033f04 rewrites FileBrowser, −333) — W1 |
| `search-dialog.tsx:333-335` type chips `type="button" aria-pressed`; `search-results.tsx:408` `` aria-label={`Show all ${n} ${plural}`} `` | `search/search-dialog.ts:101-104`, `search/search-results.ts:71` | Salon-spine |
| `SlideOverPanel.tsx:84-88` `inert={!isOpen}` | `ui/slide-over-panel.ts:90`: **ALREADY PORTED** (P4.D358) | — |
| `WardrobeBrowser.tsx:319` `aria-pressed={slotFilter === slot}`; `wardrobe-control-dialog.tsx:425-448` tabs `aria-pressed` | `wardrobe/wardrobe-browser.ts:169-174`, `wardrobe/wardrobe-control-dialog.ts:307-321` | item-browser |
| `components/workspace/TabStrip.tsx:48-70, :101-158`: roving focus (Arrow/Home/End, **Delete closes**); `role="tab"` moves onto an inner `button.qt-tab-select` (`tabIndex` active 0 / −1, `aria-keyshortcuts="Delete"`); the outer div becomes `role="presentation"`; the close button `tabIndex=-1 aria-hidden`, losing its `aria-label` and the icon `title` | `workspace/chrome/tab-strip.ts:28-66` (outer `div role="tab"`, close `aria-label="'Close ' + title"`) | CSS/WCAG |

- **The TabStrip move breaks selectors.** `workspace/chrome/tab-strip.ts` has NO spec today, so add `tab-strip.spec.ts`. 14 e2e files select `.qt-tab` / `[data-tab-id]` (`workspace-flow:451` uses `.qt-tab-close`): `wardrobe-flow`, `memory-maintenance-flow`, `workspace-document-standalone-flow`, `settings-concierge-flow`, `workspace-help-ask-flow`, `workspace-brahma-console-flow`, `spa-riders-flow`, `settings-memory-consolidation-flow`, `workspace-tab-refresh-flow`, `workspace-search-documents-flow`, `workspace-flow`, `prompt-templates-flow`, `workspace-help-guide-flow`, `characters-flow`.
  - Keep `data-tab-id` and `.qt-tab` on the OUTER element so those survive.
  - Any `getByRole('tab')` now hits the inner button.
  - A `getByRole('button', {name: 'Close …'})` would fail; there are none today (grep).

### 3.3 `705b954c2`, `856ed4f2b`, `db4a76e39` (non-About halves)

All NO-PORT. `705b954c2` is the token relocation + storybook generator + CI. `856ed4f2b` removes v4's Playwright: `playwright.config.ts`, `__tests__/integration/*`, the dependency. v5's Playwright is its own and unaffected. `db4a76e39` is the chokidar watcher, NO-COUNTERPART per the ledger.

### 3.4 About credits (`screens/about/about-page.ts`, P4.D359; v4 `app/about/AboutView.tsx` @ pin)

| v4 @ pin | v5 line | change |
|---|---|---|
| `:150` Salon blurb | `:329-…` | + "Deus Ex Machina established facts that every character knows," … "indexed message search that looks in the open conversation first" (`70d2859d8`) |
| `:170` Pascal | `:349` | + "the Inventory of objects characters carry," |
| `:178` Scriptorium | `:357` | + ", browsed with one folder browser shared with the Wardrobe and the Inventory and read-only viewers for pictures, PDFs and Word files" (`c34986421`) |
| `:194` Saquel | `:373-374` | "SQLCipher-encrypted" → "ChaCha20-Poly1305-encrypted" |
| `:210` Brahma | `:389-…` | + "with lookup shortcuts for characters, memories and conversations," … "on an engine of its own choosing" |
| `:253` Language | `:423` | "TypeScript 5.9+" → "TypeScript 6+" |
| `:256` Database | `:424` | "SQLite + SQLCipher" → "SQLite + SQLite3 Multiple Ciphers" |
| (deleted) File Manager | `:427` | DELETE `{ label: 'File Manager', value: 'SVAR' }` (`c34986421`) |
| `:371` Core | `:438` | "(SQLCipher)" → "(SQLite3 Multiple Ciphers, ChaCha20-Poly1305)" |
| `:376` UI & Interaction | `:~452` | drop "@svar-ui/react-filemanager, " |
| `:378` Filesystem & Archives | `:458` | "chokidar, " dropped (`db4a76e39`) |
| `:380` Testing | `:460` | "Playwright, " dropped (`856ed4f2b`) |

Spec: `screens/about/about-page.spec.ts:60` (it lists labels; a deleted "File Manager" row moves its count).

## 4. INVENTORY — the SPA halves of the server rows (brief; full server shapes are the other surveys')

### 4.1 `604a082c4` — Salon stream frames become a `kind`-tagged union

- **v4:** `lib/chat/stream-frames.ts:132-300`.
  - `StreamFrameSchema` discriminates on `kind`: `content`, `reasoning`, `status`, `fileProcessing`, `toolsDetected`, `toolResult`, `turnStart`, `done{outcome: finished|skipped|yielded|stopped|empty}` (`:107`), `turnComplete`, `chainComplete`, `error`, `pendingExternalTurn`, `posted{what: carinaAnswer|hostAnnouncement|pascalResult}`, `storytellerRound`, `storytellerBid`, `roundOpen`, `handAsked`, `handAnswered`, `confirmationResult`, `swipeDone`.
  - `seat:{participantId,concurrent:true}` (`:59-63`) replaces the top-level `concurrent`.
  - `parseFrame` `:419` drops an unknown kind and logs once. `toLegacyBag` `:450` serves the help / Brahma / Scenario Builder / one-shot streams.
  - Consumers: `useSSEStreaming.ts:638-865` (three exhaustive switches) and `useRegeneration.ts`.
- **v5:**
  - `core/chat-stream.reducer.ts:284-560` (P4.5; P4.D336, P4.D343 / P4.D344) reads the bag flags: `:289-297` storyteller, `:301` concurrent, `:364-389` posted, `:398` done, `:456-503` `reduceConcurrent`, `:532-560` `reduceDone`.
  - `chat/regeneration.state.ts:163-193`.
  - `help/help-stream.ts` and the Brahma consumers stay on the bag.
- **Specs:** `core/chat-stream.reducer.spec.ts` (676 lines), `salon-conversation.spec.ts` (frame fixtures, e.g. `:5476-5488`), `salon-turn-controls.spec.ts`, regeneration specs. No e2e selector moves.
- **Contract:** `core-contract.ts:5369` `ChatStreamFrame` is still the untagged bag (`concurrent?` `:5459`, `handAnswered?` `:5412`). It must be REWRITTEN in lockstep with the server lane (**R7**).

### 4.2 `9eab8d2da` — the `handAsked` frame and the raised-hand chip

- **v4:** `app/salon/[id]/components/RaisedHandChip.tsx` (NEW, 76).
  - States at `:28-37`, verbatim:
    - raised: "Hand raised — the next speaker will be asked to yield"
    - asked: "`${name}` is weighing your raised hand…"
    - yielded: "`${name}` yields the floor — it is yours"
    - declined: "`${name}` carried on regardless; your hand is lowered"
    - unasked: "The room fell quiet before anyone was asked — the floor is yours"
  - Markup: `aria-live="polite"`; `.qt-chat-tool-result-chip[data-hand-state]`.
  - × button: "Lower your hand" / "Dismiss the raised-hand note".
  - `useSSEStreaming.ts:235-252, :643-650`; the decline TOAST is REMOVED.
- **v5:**
  - `screens/salon/salon-conversation.ts:1654` `handRaised`; the toast to delete is at `:4447-4454`; resets at `:4110` / `:4578`; `:4643` toggle.
  - `chat/chat-composer.ts:630` input; `:229` beside `<qt-pending-inform-chips>`.
  - `chat-stream.reducer.ts:374-382`.
  - Ported by P4.D336.
- **Specs:** `salon-conversation.spec.ts:5476-5488`, `chat-composer.spec.ts`, **`e2e/salon-interrupt-flow.spec.ts:431`** (asserts `carried on regardless` count 0; the chip now contains that text).
- **Contract:** a `handAsked{participantId, characterName}` frame. It is ABSENT; only `handAnswered` exists (`:5412`).

### 4.3 `db6190b44` + `df8d4d70a` (Deus parts) — Deus Ex Machina

- **v4 client:**
  - `lib/chat/established-fact.ts:32-84` (pure predicates).
  - `ChatComposer.tsx:248, :317` (`deus` slash action), `:472-488` (armed chip "Deus Ex Machina: this line will be established as fact", aria "Disarm Deus Ex Machina"), `:606-610`, `:642` (the title suffix "· Cmd/Ctrl+Shift+Enter sends as established fact"); the send title / aria "Send as established fact".
  - `SpeakingAsAvatar.tsx:126-146` (`button.qt-chat-established-fact-toggle aria-pressed aria-label="Deus Ex Machina"`; the two tooltips).
  - `KeyboardPlugin.tsx:59-61, :122-124`.
  - `lib/slash-commands/registry.ts:65-70` (`deus`; keywords fact / true / canon / dem).
  - `MessageRow.tsx` (ribbon "Deus Ex Machina — established as fact"; machine row `qt-chat-established-fact-row` / band, no avatar).
  - `system-message-labels.ts` (`'established-fact': 'Deus Ex Machina'`, importance `high`).
  - `announcement-render-items.ts:52`.
  - `InsertAnnouncementDialog.tsx` (a "Deus Ex Machina" tab, mode `fact`, "Establish" / "Established as fact").
  - `useImpersonationVoice.ts` (no rehearsal for a fact).
  - Icon `public/images/icons/deus-ex-machina.svg` (7 lines) + the madmans-box override.
- **v5:** ALL ABSENT (zero hits for `established` / `deus`). Targets:
  - `chat/chat-composer.ts` (`:319` speaking-as host, `:820` slash actions, `:993` title);
  - `chat/speaking-as-avatar.ts:70-81`;
  - `editor/rich-editor.ts:160, :421` (a `Mod-Shift-Enter` + submit options payload);
  - `chat/slash-commands/registry.ts:67` + `types.ts:36` (P4.D308);
  - `editor/slash/slash-command-plugin.ts:89`;
  - `chat/impersonation-voice/gate.ts:55`;
  - `chat/message-row.ts:~197`;
  - `chat/system-message-labels.ts:323` (+ `system-message-labels.oracle.spec.ts`);
  - `chat/chat-view-model.ts:293-298, :357`;
  - `chat/post-office/insert-announcement-dialog.ts:30, :568, :688`;
  - `ui/icon.ts` `IconName`.
- **CSS** is in the CSS lane (§1).
- **Contract (ABSENT):** `ChatSendRequest.isEstablishedFact?` (`core-contract.ts:42-56`), `MessageDto.isEstablishedFact?: boolean|null` (`:3473`), the `systemKind` value `'established-fact'`, `AnnouncerSenderWire {kind:'fact'}` (`:237-240`).

### 4.4 `b15fe9404` item 41 — re-enabling "All at once"

- **v5:** `chat/storyteller-board.ts:261-268` (bare `disabled`, no `(click)`, title "The whole company speaking at once is still being rehearsed — not yet on the bill."; the ruling comment `:253-260`).
- **v4 rule** (`components/chat/StorytellerBoard.tsx:408-416` @ pin): `onClick={() => run('simultaneous')}`, `disabled={disabled || chosenBids.length < 2 || destroyMixed}`, title "Simple and Lookup bids speak at once; Act and Destroy bids follow one at a time".
- **Moves:** `storyteller-board.spec.ts:253-257, :301, :305-316, :336+`; DELETE the skip at `e2e/salon-storyteller-flow.spec.ts:455-458` (beat (K) at `:447-458`, the gate note at `:51`).
- **Contract:** the server lifts the 422 `not_available: storyteller simultaneous` and emits seat-tagged frames. Under R7 these are `seat:{…}`, so the concurrent reducer `:298-301` / `:456-503` moves with §4.1. `StorytellerOrder` already has `'simultaneous'` (`:9521`).

### 4.5 `c539c82e0` — the inline Brahma engine

- **v4:** `components/brahma-console/InlineBrahmaProfileSelect.tsx` (NEW).
  - Inherit option `''`; "(a profile that has since left the building)"; options read `{name} ({provider} • {model})` with " — your default" / " — no tools".
  - `BrahmaConsoleSettings.tsx:24-120` (label "Engine for `@Brahma` in the Salon", "Use my default profile", toast "Inline Brahma engine saved").
  - `ModelBehaviorCard.tsx` (h4 "Inline Brahma Engine", "Inherit from global setting"; description "Agent mode, roleplay template, inline Brahma & tool defaults").
  - `useProjectDetail.ts` toasts.
- **v5:** `screens/settings/chat/brahma-console-settings.ts:46` (P4.D59), `screens/prospero/cards/project-model-behavior-card.ts:36` (P4.6l), `screens/prospero/project-detail.ts`; NEW shared `brahma/inline-brahma-profile-select.ts`.
- **Specs:** `brahma-console-settings.spec.ts`, `project-model-behavior-card.spec.ts`; e2e `settings-brahma-console-flow`.
- **Contract (ABSENT):** `inlineConnectionProfileId?: string|null` on the Brahma settings read and update (`:9214`); `brahmaConnectionProfileId?: string|null` on `ProjectUpdatePatch` (`:1974`) and the project DTO.

### 4.6 `b09ca46bb` — the "Kind of character" card

- **v4:** `components/characters/CharacterKindPicker.tsx` (NEW, 80).
  - Legend "Kind of character"; `data-testid="character-kind"`; `qt-radio`; hint via `aria-describedby`.
  - Two radio labels (verbatim in the inventory report).
  - `app/aurora/shared/save-character-kind.ts:16-23`: PUT `{kind}`; toasts and error "Failed to update the kind of character".
  - Placed BEFORE `CanDrawDiagramsToggle` at `view/CharacterDetailView.tsx:245` and `edit/CharacterEditView.tsx:260`. `resolveCharacterKind` defaults to `roleplay`.
- **v5:** `screens/characters/view/character-detail.ts:224`, `screens/characters/edit/character-edit.ts:163`; sibling pattern `screens/characters/can-draw-diagrams-card.ts` (P4.D353). NEW `character-kind-card.ts` + a `resolveCharacterKind` twin.
- **Contract (ABSENT):** `CharacterDetail.kind?` (`:3886`), the PUT `{kind}` arm.

### 4.7 `544c7ab1e` / `d439f6dc9` — the profile editor

- **v4:**
  - `ProfileModal.tsx:753-798`: no Temperature / Top P when `!supportsSamplingParameters`; the note "This provider chooses its own temperature and Top P; only the token ceiling is yours to set."; `grid-cols-1`.
  - `useProfileForm.ts:41, :53` (`outboundSamplingParameters` strips both fields for save and test-message).
  - `ProfileCard.tsx:161` shows "Max Tokens" only.
  - `ProfileModal.tsx:958-980`: `#maxImagesPerRequest` "Most images per request", min 1, max 50 (`MAX_IMAGES_PER_REQUEST_CEILING`), shown only with `supportsImageUpload`. Its hint (verbatim) is wired by `aria-describedby`; the default is `'4'`.
- **v5 (P4.6e):** `screens/settings/providers/profile-modal.ts:410-441, :571-578, :1192-1201`, `profile-form.ts:138, :279-281, :296, :324`, `connection-profiles-card.ts:130-132`.
- **Specs:** `profile-modal.spec.ts`, `profile-form.spec.ts`; e2e `settings-provider-options-flow`.
- **Contract (ABSENT):** `ProviderInfo.capabilities.samplingParameters?` (`:4251-4257`); `ConnectionProfileDto.maxImagesPerRequest?: number|null` (`:4343`) + the create / update bodies (`:1073` / `:1077`).

### 4.8 `fd165fce0` — Pascal Run-dialog presets across the tool walk

- **v4:** `components/chat/CustomToolRunDialog.tsx:637-692`.
  - Gate on `tool.asCharacterId`; `GET …/custom-tools?action=presets&tool=&asCharacterId=` → `{presets:[{tier,mountPointId,mountName,name,path,shadows[]}], stores[], unread[{…,reason}], chatId}`.
  - Options read `{name} — {mountName}`, with the title "Also in X, which this one outranks".
  - A "Save into" store select; copy "Consulting the stores…", "The stores would not list their presets.", the unread line, "Preset “n” filed in {mountName}."
  - Query key `['custom-tools', chatId, 'presets', asCharacterId??'', toolName]`.
- **v5:** `chat/custom-tool-presets.ts:127-261` (P4.D43; input `vaultMountPointId`, lists through `mountFilesList`), host `chat/custom-tools-popup.ts:187-196`, keys `chat/custom-tools.api.ts:37`.
- **Specs:** `custom-tool-presets.spec.ts`; e2e `salon-custom-tools-flow.spec.ts:295-340`.
- **Contract (ABSENT):** a presets dispatch (`chatCustomToolPresets` or an arm on `chatCustomToolsList` `:757`).

### 4.9 `b2b783b47` + `600c2906f` (+ `29ba4d3f8`'s hook) — the shared item folder browser

- **v4:** `components/item-browser/` holds the family that replaces per-kind code (about 6k v5 lines to refactor):
  - `FolderBrowserShell.tsx` (290, testids `${prefix}-folder-bar|-folder-row|-tier-row|-select-bar`)
  - `useFolderBrowserState.ts`
  - `ItemActionMenu.tsx` (184; `aria-haspopup="menu"`, roving arrows, `data-action`)
  - `ContainerSelect.tsx`
  - `ItemFolderField.tsx` ("New folder…")
  - `ItemControlDialogShell.tsx` (per-kind label table `:58-72`)
  - `ItemFolderPickerDialog.tsx`
  - `DestinationPickerDialog.tsx` (`role="listbox" aria-label="Folders"`, "Already here")
  - `ItemTransferFooter.tsx`
  - `ItemImageViewer.tsx`
  - `useItemOverlays.ts`
  - `item-dialog-provider.tsx`

  Other client moves: the wardrobe Duplicate → `?action=duplicate {itemId}`; `case 'wardrobe'` in `topic-map.ts`; `refresh-unless-realtime.ts`.
- **v5:** `item-browser/` (only folder-bar, folder-picker-dialog, folder-row, select-bar, tier-row); `wardrobe/*` (P4.D285, P4.D277); `inventory/*` (P4.D330); `screens/scenarios/shared/scenario-row.ts`.
- **e2e:** `wardrobe-flow`, `wardrobe-folders-flow`, `wardrobe-copy-to-folder-flow`, `wardrobe-items-editor-flow`, `inventory-flow`, `character-wardrobe-pictures-flow`, `workbench-wardrobe-picker-flow`.
- **Contract (ABSENT):** `POST <tier>/wardrobe?action=duplicate {itemId}` → `{item}` (the inventory twin exists, `:10134`); the realtime topic `'wardrobe'` (`core/realtime.types.ts:40-70`; P4.D330 R-2 precedent: `onTopic`, return `[]`).

### 4.10 `c34986421` + `aec626eff` — the Scriptorium store browser (SVAR removed)

- **v4:** NEW `components/document-browser/`:
  - `DocumentStoreBrowser.tsx` (456; "Search this store", "Upload here", "Move to…" / "Copy to…", the empty texts)
  - `DocumentRow`, `DocumentFileDialogs`, `DocumentDestinationDialog`
  - `DocumentViewer.tsx` (image / pdf via lazy pdf.js `:32` / markdown / text / docx via `?action=render-html` `:85`)
  - `useDocumentStoreSource.ts`
  - `FilePreviewDocx.tsx:25-31` `sanitizeDocxHtml` (DOMPurify `USE_PROFILES html`, `FORBID_TAGS`, `ALLOWED_URI_REGEXP`)
  - `lib/folder-browser/{document-actions,preview-kind}.ts`

  `components/files/svar/**` and `FileTable.tsx` are DELETED.
- **v5 (P4.6z / P4.6aa — D18):** `screens/scriptorium/store-detail.ts:17-60`, `file-table.ts`, `file-detail-row.ts`; `files/{event-wire-map, file-manager-adapter, file-manager-transport, listing-to-tree, node-id, error-translation, reindex-after-copy}.ts` (`event-wire-map.ts:25` records "no copy-folder verb"); `screens/files/file-preview-modal.ts:41,177` (PDF = a download prompt).
- **e2e:** `scriptorium-file-manager-flow`, `scriptorium-flow`.
- **Contract (mostly ABSENT):**
  - the listing `{files: DocMountBrowserEntry[] (+linkGroupId, linkCount), folders}` (`DocMountFileDto` `:7461` is minimal);
  - `?action=move-folder|copy-folder` across mounts; `delete-folder {path, recursive}`; `link-file`, `unlink-file`;
  - the item route `PATCH {rename|description}`, `DELETE`, `PUT` multipart (v5 uses `?action=write-file`);
  - `GET ?action=links` → `{linkGroupId, locations[…]}`;
  - `GET ?action=render-html` → `{html}` / 400 "Only .docx files can be rendered to HTML";
  - the per-store `mountPoints` realtime scope (v5 `core/realtime-topic-map.ts:132-141` returns `[]`).
  - `capabilities` is PRESENT (`:6163-6198`).
- **Rulings R8 (D18 reversal), R9 (the sanitizer), R10 (PDF viewer).**

### 4.11 `5efe53ea7` + `c17033f04` — the Document Mode picker, read-only viewers, General library

- **v4:**
  - `DocumentPickerModal.tsx`: −383; the pick step is `DocumentStoreBrowser`; the NEW "General library" button "Quilltap General, the shelf every chat shares", `:320-330`.
  - `DocumentViewerPane.tsx` (NEW, 74); `useDocumentMode.ts` (`viewer`); `documentModeApi.ts`.
  - **`components/ui/BaseModal.tsx` `anotherDialogOwns`:** only the topmost dialog answers Escape / an outside click.
  - c17033f04: `ProjectStoreBrowser.tsx` (NEW); `FilesCard.tsx` "Browse All"; `LibraryFilePickerModal.tsx` (projects / stores via `attachMountFile`); `FileBrowser.tsx` −333.
- **v5:** `documents/document-picker.ts:69-151, :357-413`, `document-pane.ts`, `document-mode.ts`, `document-api.ts:31,103`, `standalone-document-view.ts` (P4.6ag / P4.D122 / P4.9l2); `chat/library-picker/*` (P4.9e4b); `screens/prospero/cards/project-files-card.ts` (P4.6r); `screens/files/files-browser.ts` (P4.29).
  - **`ui/modal.ts:33-43, :113`: two stacked `qt-modal`s BOTH close on one Escape today.**
- **e2e:** `salon-documents-flow`, `workspace-document-standalone-flow`, `workspace-search-documents-flow`, `general-files-flow`, `projects-flow`.
- **Contract (ABSENT):**
  - `viewer?:{mountPointId,relativePath,fileName,fileType,fileSizeBytes,originalMimeType|null,updatedAt}` on open / read (`OpenDocumentResultDto` `:7410`);
  - a 400 on an editor write to a viewed file;
  - `generalLibrary:{mountPointId,name,mountType}|null` on accessible-stores (`:7269`);
  - `GET /projects/{id}/mount-points` → `{mountPoints, officialMountPointId}` (`projects.api.ts:161`).
- **Ruling R11:** port `anotherDialogOwns` into `ui/modal.ts`. It flips a v5 convention and interacts with the ruled `WARDROBE_VIEWER_ESCAPE_LAYERED` divergence, which v4 now converges on.

## 5. Verification design

- **(a) Pure twins, tier-1 in the SPA** (the `*.oracle.spec.ts` + committed `.v4.json` precedent, e.g. `chat/concierge-state.v4.json`). A small `harness/oracle/cases/spa-*.ts` imports v4's REAL module at the pin and emits the fixture:
  - `conversation-search` (`toSearchableText`, `conversationQueryPattern`, `findConversationMatches` over a corpus incl. markdown emphasis, links, a `TOOL` row, `systemSender`, a `limit` < total, astral / `u`-flag text);
  - `abbreviateName` / `chooseDensity`;
  - `inventoryAbbreviation`;
  - `markdownButtonName`;
  - `established-fact` predicates;
  - `revealRequestFromSalonHref`.
  - The two jest suites (`conversation-search.test.ts`, `ChatHeaderBreadcrumb.test.tsx`) seed the corpus.
- **(b) CSS:** the ported `check-css-vars.mjs` is the gate (zero unresolved). `theme-contrast.spec.ts` `PACK_MANIFESTS` → the pin versions; the residues are unchanged (`tokens.json` unmoved). A `check-qt-classes.mjs` run proves the new classes resolve (`qt-bg-popover`, `qt-tab-select`, `qt-chat-established-fact-*`).
- **(c) axe:** a re-baseline on the union. The expected deltas are 23 earl-grey-dark rows and 12 target-size rows retiring; the queue-badge rows stay (§9 R6).
- **(d) e2e beats:**
  - a Salon search hit scrolling to and flashing a row, and a `?msg=` deep link;
  - the breadcrumb at three widths (resize the viewport);
  - the avatar click opening the image viewer, and the glyph row;
  - the tab strip's Arrow / Delete keys;
  - Deus via `/deus`, the toggle and Cmd/Ctrl+Shift+Enter;
  - the raised-hand chip states;
  - item 41's beat (K) un-skipped.

## 6. Server contracts the SPA lanes consume (by literal name)

1. **CSS lane → every SPA lane:**
   - classes `qt-bg-popover`, `qt-tab-select`, `qt-chat-established-fact`, `qt-chat-established-fact-row`, `-band`, `-toggle`, `qt-chat-message-established-fact`, `qt-chat-composer-established-fact`;
   - `[data-icon="deus-ex-machina"]` + `public/images/icons/deus-ex-machina.svg`;
   - `::highlight(quilltap-conversation-search)`;
   - `.qt-page-toolbar-left:empty`.

   `check-qt-classes.mjs` reds a lane that emits one before the CSS lands, so **stage the CSS lane first** (§7.2).
2. **Salon stream frames (server lane ↔ Salon-spine SPA):** the `ChatStreamFrame` rewrite (§4.1): `kind`, `seat`, `done.outcome`, `posted.what`, `handAsked`, `swipeDone`. The SAME block goes in both orders (R7).
3. **Deus (server ↔ composer + spine SPA):** `ChatSendRequest.isEstablishedFact`, `MessageDto.isEstablishedFact`, `systemKind 'established-fact'`, `AnnouncerSenderWire {kind:'fact'}`, the job type `established-fact-memories` (server-only).
4. **Item 41:** `storytellerOrder: 'simultaneous'` no longer 422s; seat-tagged frames.
5. **Brahma:** `inlineConnectionProfileId`, `brahmaConnectionProfileId`.
6. **Kind:** `CharacterDetail.kind`, the PUT `{kind}`.
7. **Profiles:** `capabilities.samplingParameters`, `maxImagesPerRequest`.
8. **Pascal presets:** the presets dispatch + response shape.
9. **Item browser:** `?action=duplicate`, the topic `'wardrobe'`.
10. **Document stores:** the listing shape, the folder / file actions (`copy-folder`, `link-file`, `unlink-file`, `delete-folder`), `?action=links`, `?action=render-html`, the per-store `mountPoints` scope.
11. **Document Mode:** `viewer`, `generalLibrary`, `{mountPoints, officialMountPointId}`.
12. **None** for: §2.1 breadcrumb, §2.2 search, §2.3, §2.4 avatar, §2.5 tooltips, §3.

## 7. Lane cuts (file-disjoint in `apps/web/**`)

### 7.1 Proposed SPA lanes (8; the last round had 7)

| lane | rows | OWNS (exclusive) | size |
|---|---|---|---|
| **S1 CSS / themes / WCAG pass** | `29ba4d3f8` (CSS + the 3 unclaimed class edits), `87c3860df`, `94f041764` (CSS, packs, and the template edits in files no other lane owns), the `db6190b44` / `df8d4d70a` CSS + icon, `3c41cf3da` / `4f5047241` CSS | `src/styles/**`, `src/styles.css`, `public/themes/**`, `public/images/icons/deus-ex-machina.svg`, `src/app/theme/**`, NEW `scripts/check-css-vars.mjs` (+ the `package.json` `lint` / `test` lines), `e2e/a11y/**`; templates: `screens/characters/list/character-card.ts`, `screens/characters/view/character-header.ts`, `screens/setup/setup-wizard.ts`, `chat/sidebar/chat-sidebar.ts`, `progressions/progressions-section.ts`, `subprompts/subprompts-section.ts`, `chat/memory-cascade-dialog.ts`, `workspace/chrome/tab-strip.ts` (+ NEW spec), `brahma/model-picker.ts`, `images/standalone-generate-image-dialog.ts`; the e2e tab-strip selector fixes | M |
| **S2 Salon spine** | `604a082c4` SPA, `9eab8d2da` status machine, `4f5047241`, `3c41cf3da`, item-41 reducer half | `core/chat-stream.reducer.ts` (+ spec), `chat/regeneration.state.ts`, `screens/salon/salon-conversation.ts` (+ spec), `chat/conversation-header.ts`, NEW `chat/conversation-breadcrumb.ts` / `breadcrumb-density.ts` / `conversation-search.ts` / `message-navigation.ts` / `search-highlight.ts`, `search/**`, `layout/page-toolbar.service.ts`, `chat/auto-scroll.ts`, `workspace/chrome/link-interceptor.ts`, `workspace/core/route-to-intent.ts`, `workspace/workspace-redirect.guard.ts`, `core/core-contract.ts` **frame block only**, `e2e/salon-interrupt-flow.spec.ts` | L |
| **S3 Salon composer & transcript** | Deus SPA, `df8d4d70a` tooltips, the raised-hand CHIP component, `27fb3ec77`, the `671c59a53` clause, the `94f041764` composer-family edits | `chat/chat-composer.ts`, `chat/speaking-as-avatar.ts`, `editor/rich-editor.ts`, `editor/formatting-toolbar.ts`, `editor/slash/**`, `chat/slash-commands/**`, `chat/rng-dropdown.ts`, `chat/pending-inform-chips.ts`, `chat/custom-tools-popup.ts` (the trigger only), `chat/message-row.ts`, `chat/chat-view-model.ts`, `chat/announcement-group.ts`, `chat/system-message-labels.ts`, `chat/impersonation-voice/gate.ts`, `chat/post-office/insert-announcement-dialog.ts`, NEW `chat/raised-hand-chip.ts` / `chat/established-fact.ts`, `ui/avatar.ts`, `ui/icon.ts`, `inventory/inventory-glyph.ts`, `inventory/chat-inventory-data.ts` | L |
| **S4 Storyteller item 41** | `b15fe9404` item 41 (SPA) | `chat/storyteller-board.ts` (+ spec, `.pure.ts`), `e2e/salon-storyteller-flow.spec.ts` | S (or fold into S2: same reducer) |
| **S5 Item browser** | `b2b783b47`, `600c2906f`, `29ba4d3f8` rows + `useFixedMenu`, the `94f041764` wardrobe edits | `item-browser/**`, `wardrobe/**`, `inventory/**` **except** `inventory-glyph.ts` / `chat-inventory-data.ts` (S3), `screens/scenarios/shared/scenario-row.ts`, NEW `ui/fixed-menu.ts`, `core/realtime*.ts` ('wardrobe' topic), the seven e2e files | L |
| **S6 Document-store browser** | `c34986421`, `aec626eff` (SPA) | NEW `document-browser/**`, `screens/scriptorium/**`, `files/**` (retired under R8), `screens/files/file-preview-modal.ts`, `e2e/scriptorium-*` | L (gated on R8) |
| **S7 Document Mode + libraries** | `5efe53ea7`, `c17033f04`, the `94f041764` FilesCard / LibraryPicker / FileBrowser edits | `documents/**`, `chat/library-picker/**`, `screens/prospero/cards/project-files-card.ts`, `screens/files/files-browser.ts`, `ui/modal.ts` (R11), the e2e list in §4.11 | M (depends on S6's `document-browser/` for pick mode → stage after S6, or S6 exports the shell first) |
| **S8 SPA smalls** | `b09ca46bb` card, `544c7ab1e`, `d439f6dc9`, `c539c82e0`, `fd165fce0` SPA, About (`db4a76e39` / `856ed4f2b` / `c34986421` / `70d2859d8`) | `screens/characters/view/character-detail.ts`, `screens/characters/edit/character-edit.ts`, NEW `character-kind-card.ts`, `screens/settings/providers/**`, `screens/settings/chat/brahma-console-settings.ts`, `screens/prospero/cards/project-model-behavior-card.ts`, `screens/prospero/project-detail.ts`, NEW `brahma/inline-brahma-profile-select.ts`, `chat/custom-tool-presets.ts`, `chat/custom-tools.api.ts`, `screens/about/**` | M |

### 7.2 Stacking and collisions

- **S1 lands first** (stage 0): every new class and icon is there before a template emits it. S1 is mechanical and needs no server.
- **S2 / S3 depend on the frame and Deus contracts.** The contract block is written ONCE, in S2's order, and copied verbatim into the server order. S3 consumes `isEstablishedFact` through its own `chat-composer` output (`send` gains `{establishedFact?: true}`). **S2 wires it into `chatSend` in `salon-conversation.ts`** (HANDOFF H1).
- **The raised-hand chip** is split the same way: S3 builds `qt-raised-hand-chip` (input `status: RaisedHandStatus|null`, output `dismiss`) and hosts it in `chat-composer.ts`; S2 computes the status from frames and deletes the toast. `RaisedHandStatus` is a shared type in NEW `chat/raised-hand.ts`, owned by S3 and imported by S2 (H2).
- **Avatar full size:** S3 emits `imageClick` from `message-row.ts`; `salon-conversation.ts:513` already consumes it. No S2 edit is needed.
- **`core-contract.ts`** is shared by every SPA lane plus the server lanes. Rule: each lane appends ONLY its own named block, never reorders; the unifier resolves.
- **W1 handed-off `94f041764` edits** (byte-exact from the hunk, applied by the file's owner): FilesCard `aria-expanded` → S7; LibraryFilePicker `hover:qt-bg-surface-hover` → S7; FileBrowser `aria-label`s → S7; ProfileCard drag handle → S8; search-dialog / search-results → S2; wardrobe `aria-pressed` → S5; formatting-toolbar / rng-dropdown / insert-announcement → S3.
- **`ui/modal.ts`** (R11) is S7's. S5's item viewer and the wardrobe viewer's ruled divergence must be re-checked on the union.
- **`ui/icon.ts`** (S3) and `public/images/icons/` (S1): the icon file is S1's, the `IconName` union entry S3's.
- **Help pages** (`help/chats.md`, `inventory.md`, `search.md`, `themes.md`, `tabbed-workspace.md`, `deus-ex-machina.md`, `help-chat.md`) belong to the help-vendoring lane, never an SPA lane.

## 8. Tiering

| row | tier | why |
|---|---|---|
| CSS re-vendor, packs | byte (vendored = pin, modulo the recorded v5 adaptations) + `check-css-vars` | pure data |
| `conversation-search`, `abbreviateName` / `chooseDensity`, `inventoryAbbreviation`, `markdownButtonName`, established-fact predicates | tier-1 SPA oracle (`.v4.json`) | pure, v4-tested |
| breadcrumb densities, reveal settle, Custom Highlight | component spec + e2e | layout / DOM |
| frames reducer | component spec over the shared contract fixtures; tier-3 on the server side | wire |
| WCAG attributes | the static census (`check-a11y-names.mjs`) + component specs + the axe re-baseline | no v4 tests |

## 9. Rulings owed to the human

- **R1** — Port `check-css-vars.mjs` into `npm run lint` / `test`? **Recommend YES.** It is the only proof the v5-only residues (`_chat.css:1323`, `:2444`, …) are gone, and it stops the class from recurring.
- **R2** — For the v5-only rules it reds: fix them in place (a v5 defect, not drift). **Recommend FIX.** They are the same defect v4 just fixed, in v5-authored twins.
- **R3** — Breadcrumb host: keep v5's inline header with the Inspector / cost cluster on the right. **Recommend YES** (v4's right slot holds them too).
- **R4** — Vendor only the referenced madmans-box `deus-ex-machina.svg` (the P4.D336 precedent), not all 88 icons. **Recommend YES.**
- **R5** — The About text lands whole in S8. The five feature blurbs are true only on the union, so the unifier's gate checks them. **Recommend YES.**
- **R6** — Re-own the 420 `qt-queue-badge-*` axe rows from `P4.D357` (landed) to a measured class. First measure v4 at the pin with the same axe sweep: if v4 fails them too, mark them `RULE v4-faithful`; otherwise they are a FOLLOW-UP. **Recommend MEASURE, then RULE.**
- **R7** — The Salon frame wire: port v4's `kind`-tagged union in lockstep (server + S2), with non-Salon streams on the legacy bag. **Recommend YES** (contract frames stay "v4 payloads verbatim").
- **R8** — D18 (`phase-4.md:263`, the bespoke SVAR-twin file manager) is reversed upstream. **Recommend RETIRE** `files/*`, `file-table.ts`, `file-detail-row.ts` and the toggle, and port `document-browser/`.
- **R9** — The `render-html` client sanitizer with v4's exact DOMPurify config (`FilePreviewDocx.tsx:25-31`) on a private instance (the `chat/diagrams/sanitize-svg.ts` precedent; `dompurify` 3.4.17 is already a dependency). **Recommend YES**, whatever the server's docx crate.
- **R10** — PDF viewing: v4 lazy-loads pdf.js. Choose between adding `pdfjs-dist` to the SPA and a native `<object type="application/pdf">` as a ruled divergence. **Recommend NATIVE + ruled divergence.**
- **R11** — Port `anotherDialogOwns` into `ui/modal.ts` (only the topmost dialog answers Escape / an outside click). This retires v5's double-close and re-checks `WARDROBE_VIEWER_ESCAPE_LAYERED`. **Recommend YES.**
- **R12** — The TabStrip `role` move: keep `data-tab-id` / `.qt-tab` on the outer element so the 14 e2e files keep their selectors. **Recommend YES.**

## 10. Dogfood rows the lanes should queue

- Every pack × mode: open a wardrobe kebab over a scrolling list (opaque, above later rows), the help guide and Document Mode chrome (colours now resolve), an alert with a link (underlined, inherits ink), the tab strip by keyboard.
- The Salon with 6+ characters at 1024 px: the breadcrumb densities and tooltips.
- Header search inside a long chat: "This conversation" hits, a whisper hit (the toast), a collapsed announcement hit (expands), a second Escape clears the highlight; a global Message result opening `?msg=` in a fresh tab.
- An avatar click → the viewer; an equipped inventory item's glyph under the title.
- Deus via all three gestures; the raised-hand chip through raised → asked → yielded / declined / unasked.
- "All at once" with two Simple bids.
