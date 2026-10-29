# Survey — WCAG tokens / CSS / themes (P4.D357), WCAG semantics (P4.D358), SPA riders (P4.D359)

## 0. Header

- **Date:** 2026-10-29. **Pin:** v4 `236c04e35` (code-identical to `7789ebed6`, read through `git show` and the detached worktree `/tmp/qt-v4-pin-setupphase-7789ebed6`). **v5 main:** `f6cdceedd` (SPA 0.5.907). Read-only: nothing was built, no test was run; every v5 "measured" claim is a grep / read of main at that commit.
- **Commits in scope** (the four a11y / legibility sweeps + the riders the planner assigned here):
  - `cce44e098` Fix: legibility pass over the qt-* surfaces added since 4.9
  - `86cc25482` Fix: legible dialogue in user messages; Rains user text; Great Estate sidebar
  - `58120a7fe` Fix: WCAG AA contrast, focus and reduced-motion pass across all themes
  - `dff76ebef` Fix: dialog semantics and keyboard access for every modal and clickable card
  - `eed72d60c` Fix: accessible names for every form control
  - `68340008b` Fix: name the Salon composer and each character-field editor
  - `10e09b7c3` Fix: remaining WCAG items from release checklist 13
  - `2041bb875` Fix: Automation settings checkbox uses qt-checkbox (#111)
  - `99463fc56` Feature: always-on floating Brahma Console (SPA only)
  - `a4945d8d9` Docs: release checklist 14 documentation pass (the ONE code hunk: About credits)
  - `3b8b6f42b` SPA half (two settings cache writes); `9a88f862f` SPA half (Salon name chips); `8cb584eff` SPA half (keystone UI). The wire for the last two is the context survey's / the keystone survey's.
- **Lanes informed:** P4.D357, P4.D358, P4.D359 directly; P4.D344 / P4.D346 / P4.D352 / P4.D353 (file-ownership HANDOFFs, §7); P4.D360 (help pages, theme-pack question); P4.D349 (one conditional collision, §7.4).

**Headline facts a planner must not miss**

1. **Every one of the six bundled theme packs in v5 is a byte copy of the ORACLE BASELINE's (`322a5488e`), not the pin's** — versions art-deco 1.0.11 / earl-grey 1.3.7 / great-estate 1.0.6 / madmans-box 1.1.7 / old-school 1.0.12 / rains 1.3.10 against the pin's 1.0.13 / 1.3.10 / 1.0.10 / 1.1.9 / 1.0.14 / 1.3.14. The span's pack deltas are EXACTLY the four legibility / WCAG commits (measured with `git log 322a5488e..236c04e35 -- themes/bundled`: 18 files, +241 / −142). Three packs are NOT pure byte copies: `art-deco/styles.css`, `great-estate/styles.css`, `madmans-box/styles.css` carry v5's asset-URL rewrite (`/api/themes/assets/bundle:<id>/…` → `/themes/<id>/…`) and Madman's Box carries an icon-override block + two comment rewrites (§2.6). The other 15 files (all `tokens.json`, all `theme.json`, earl-grey / old-school / rains `styles.css`) are byte-identical to the baseline — so **tokens.json and theme.json can be re-vendored by `cp`; the three adapted `styles.css` must take v4's hunks on top of the adaptation**. Recommend ONE owner for `apps/web/public/themes/**` this round: P4.D357 (the contrast test it writes reads them). P4.D360 gets none of the packs.
2. **The ledger row for `a4945d8d9` is wrong about v5's About page:** it says v5's credits "name v5's own stack". Measured: `apps/web/src/app/screens/about/about-page.ts:435-459` carries v4's acknowledgments **verbatim** ("v4 `:362-369`, verbatim" — React, Next.js, Lexical, `pdf-parse`…). The About hunk is therefore a plain byte port, not a judgment call (one ruling rider: the "Diagrams" line names libraries v5 adopts only if P4.D353's dependency ruling passes).
3. **The `58120a7fe` toast colour move lands on a RECORDED v5 divergence.** v5 never used v4's Tailwind palette: `apps/web/src/styles/qt-components/_toast.css:11-17` styles toasts with the tinted `--qt-status-*` triples "per the order". v4 now converges onto theme tokens too, but onto the SOLID fills (`qt-bg-success qt-text-success-foreground qt-border-success`). Whether v5 follows (solid fill) or keeps the tint is a **ruling** (§10 #3; recommend converge — the divergence's reason, "v4 is raw Tailwind", is gone, and the contrast test then covers the toast by construction).
4. **`qt-modal` (`apps/web/src/app/ui/modal.ts`, 77 hosts) covers most of `dff76ebef` in ONE edit** — and that edit collides with a standing v5 convention: `modal.ts:22-24` "renders IN PLACE and handles no Escape (the standing convention)", while v4's `BaseModal` has had `closeOnEscape` (default true) all along and `dff76ebef`'s help text says every dialog "closes at a touch of Escape". **Ruling owed** (§10 #1; recommend converge with a `closeOnEscape` input, default true).
5. **The `99463fc56` floating console cannot be ported onto v5's console as it stands.** v5's console is a scrim modal by ruling ("No `FloatingDialog`", `status-log.md:41020`, `brahma-console-dialog.ts:14`); v4's whole point is a NON-modal topmost panel usable over an open form. Porting the launcher onto a scrim modal would ship a button that opens a modal over a modal. **Ruling owed** (§10 #2): build a scoped `qt-floating-dialog` primitive (the CSS already exists in v5 — `_surfaces.css:279-323`, unused), used by the console only this round; `dff76ebef`'s FloatingDialog semantics (`role="dialog" aria-modal="false"`, no trap, Escape rules) then become a HANDOFF from P4.D358 to P4.D359.
6. **The toast role change breaks 40 e2e selector lines in 20 spec files** (`[role="toast-container"]` → the pin's `[data-toast-container]`) plus 3 assertions in `src/app/ui/toast.spec.ts:105-143`; exact list in §7.6. Recommend P4.D358 lands a `e2e/support/toasts.ts` helper + the mechanical rewrite as its FIRST commit so any sibling touching one of those specs cherry-picks it.
7. **The ownership problem is real: nine v5 SPA files are wanted by two or more lanes this round**, two of them by three or four (`screens/salon/salon-conversation.ts`: P4.D344 / P4.D352 / P4.D358 / P4.D359; `chat/message-row.ts` + `chat/message-content.ts`: P4.D353 / P4.D358 / P4.D359). §7.3 gives the owner per file and the rules R1–R12 the owner applies; P4.D357 and P4.D358 are provably disjoint (CSS files + `public/themes/**` vs `src/app/**/*.ts` + `e2e/**`; the one meeting point is the literal class `qt-skip-link`, §7.2).
8. **v4 shipped NO tests for `eed72d60c` / `dff76ebef` / `10e09b7c3`** (only `99463fc56`'s two jest files). There is no v4 code to drive for the a11y work; the "oracle" is the attribute set in the hunks, so the verification is (a) a token-contrast vitest over `public/themes/*/tokens.json` + `src/styles.css` (NOT via `screens/settings/appearance/contrast.ts`, whose `getLuminance` is v4's HSL-lightness shortcut by design — §6.3), (b) a static accessible-name census script modelled on `scripts/check-qt-classes.mjs` (my approximate run: **143 unnamed `<input|select|textarea>` in 73 files** of 676), and (c) an optional axe pass as a second Playwright project (`@axe-core/playwright` is a NEW dev dep — ruling, §10 #4; the SPA has no eslint at all: no `eslint.config.*`, no `@angular-eslint`, no axe in `node_modules`).
9. **`v5` already has several of the sweep's items** — count them before writing the red: the Salon sidebar switches are named (`chat/sidebar/visibility-section.ts:66-72, :89-95, :121-127`), `Allow Any Character` is a named switch (`screens/prospero/cards/project-characters-card.ts:87-89`), the message edit box is named (`chat/message-row.ts:207`), the composer carries `ariaLabel="Message"` (`chat/chat-composer.ts:521`) and `aria-label="Message (markdown source)"` (`:498`, one byte off v4's capital M), the LLM-log row is already a `<button class="qt-card … w-full text-left">` (`screens/settings/system/llm-logs-card.ts:88`), and the slide-over already saves / restores focus + traps Tab (`ui/slide-over-panel.ts:131-199`). The hand-rolled overlay census (§2.2) is the real gap: of 34 overlay components, **13 have no `role="dialog"` at all** and only 2 use `aria-labelledby`.
10. **`_intervention.css` and `_storyteller.css` do not exist in v5** — the legibility hunks on them (`cce44e098`) ride INTO the files P4.D346 / P4.D344 create from the pin's bytes; nothing to port in P4.D357 except to say so. Likewise the Art Deco / Great Estate `.qt-intervention-row` exemptions and the Madman's Box `--qt-diagram-*` tokens ride in the theme packs (P4.D357) and target classes that exist only after P4.D346 / P4.D353 land — harmless CSS until then.

---

## 1. Per commit, the shipped hunks

All paths are v4's at the pin. Every byte below was read from `git show <sha>`.

### 1.1 `cce44e098` — legibility pass (27 files, +165 / −51)

| v4 file | Hunk | What it does |
|---|---|---|
| `app/styles/qt-components/_chat.css:164-170` | `.qt-chat-regenerating-plate-text` | adds `color: var(--qt-chat-regenerating-plate-fg, var(--qt-chat-assistant-fg, var(--color-foreground)));` (NEW hook `--qt-chat-regenerating-plate-fg`) + a three-line why-comment |
| `_chat.css:2978-2994` | `.qt-danger-badge` / `-muted` | NEW `--qt-concierge-badge-fg: var(--qt-alert-error-fg, var(--qt-concierge-badge-color));` and `color: var(--qt-concierge-badge-fg);`; the muted variant sets `--qt-concierge-badge-fg: var(--color-foreground);` |
| `_chat.css:3004` | `.qt-concierge-mark` | `--qt-concierge-mark-color: var(--qt-alert-error-fg, var(--color-destructive, #dc2626));` |
| `_intervention.css:80-86, :116-127` | note ink + the hidden-field hand | `color: var(--qt-intervention-note-fg, var(--qt-alert-warning-fg));`; `.qt-intervene-toggle--off` replaces `opacity: 0.5` with a diagonal `linear-gradient(to top right, transparent calc(50% - 0.75px), currentColor calc(50% - 0.75px), currentColor calc(50% + 0.75px), transparent calc(50% + 0.75px))` strike |
| `_storyteller.css:179-183` | `.qt-storyteller-warning` | `color: var(--qt-storyteller-warning-fg, var(--qt-alert-error-fg));` |
| `_variables.css:413` | light `--qt-response-status-streaming-fg` | `hsl(152 55% 38%)` → `hsl(152 55% 29%)` |
| `themes/bundled/art-deco/styles.css` | alert tokens `:81-98`, streaming `:225`, NEW `.qt-intervention-row` exemption `:726-731` | success-fg 30%→22%, warning-fg `hsl(43 80% 32%)`→`hsl(38 85% 22%)`, error-fg `hsl(0 84% 35%)`→`hsl(0 80% 30%)`, info-fg 32%→28%, streaming `hsl(142 70% 21%)`; the exemption: `letter-spacing: normal; text-transform: none; font-size: inherit;` |
| `earl-grey/styles.css` | `:791-792` dark `--qt-status-danger-fg: hsl(var(--danger-000));`; `:980-985` light `--qt-status-warning-fg: hsl(38 92% 28%); --qt-status-danger-fg: hsl(0 70% 40%); --qt-status-success-fg: hsl(142 70% 26%); --qt-response-status-streaming-fg: hsl(142 70% 27%);` | |
| `great-estate/styles.css` | `:221` streaming 38%→27%; dark `:498` NEW `--qt-alert-error-fg: hsl(0 65% 70%);`, `:502` NEW `--qt-alert-info-fg: hsl(210 70% 72%);`; NEW `.qt-intervention-row` exemption `:754-759` | |
| `great-estate/tokens.json` | light `mutedForeground` 45%→`hsl(20 14% 39%)`, `destructive` `hsl(0 62% 43%)`; dark `mutedForeground` 52%→60% | |
| `madmans-box/styles.css` | `:474-482` NEW `--qt-diagram-bg: var(--qt-terminal-bg); --qt-diagram-border: color-mix(in srgb, var(--rotor-cyan-dim) 70%, var(--box-walnut-rim)); --qt-diagram-toolbar-hover-bg: var(--box-walnut-mid);`; `:676-681` `[data-theme="madmans-box"] .qt-diagram { box-shadow: inset 0 0 28px hsl(184 78% 50% / 0.07); }` | targets P4.D353's class |
| `old-school/styles.css:183` streaming →`hsl(142 70% 27%)`; `old-school/tokens.json` light `destructive` →`hsl(0 70% 44%)` | | |
| `rains/styles.css` | NEW dark `--oncolor-brand: 30 25% 9%;` (`:160`), light `--oncolor-brand: 0 0% 100%;` (`:282`); light accent `24 80% 40%` / `24 80% 35%` / `24 80% 40%` / `24 78% 32%` (`:220-223`); `--qt-button-primary-fg`, `--qt-badge-primary-fg`, `--qt-filter-chip-active-fg` and `.qt-button-primary` `color` → `hsl(var(--oncolor-brand))`; streaming →`hsl(142 70% 27%)`; light status ink block `:1046-1050` (same three values as Earl Grey) | |
| `rains/tokens.json` | light `primary` `hsl(11 55% 45%)`, `mutedForeground` `hsl(30 4% 40%)`; dark `primaryForeground` `hsl(30 25% 9%)` | |
| six `theme.json` version bumps | art-deco 1.0.12, earl-grey 1.3.8, great-estate 1.0.7, madmans-box 1.1.8, old-school 1.0.13, rains 1.3.11 | |
| `help/pascals-workbench.md:39` | one sentence | "(it greys)" → "(the hand is struck through, a line drawn smartly across its palm)" — rides intervention |
| `packages/theme-storybook/*` | 1.0.87 mirror | NO-PORT (v5 has no storybook) |

### 1.2 `86cc25482` — user dialogue / Rains user text / Great Estate sidebar (12 files, +113 / −9)

- `_chat.css:1848-1857` NEW rule after the `.qt-chat-message-content .qt-chat-dialogue` block: `.qt-chat-message-user .qt-chat-message-content .qt-chat-dialogue { --qt-rp-fg: var(--qt-rp-user-dialogue-color, var(--qt-chat-user-fg)); }` with its six-line why-comment (NEW theme hook `--qt-rp-user-dialogue-color`).
- `rains/styles.css:449` `--qt-chat-user-fg: hsl(40 6% 98%)` → `hsl(var(--text-000))`.
- `great-estate/styles.css:714-743` NEW light-mode left-sidebar block (`[data-theme="great-estate"]:not(.dark) .qt-left-sidebar { --qt-sidebar-item-hover-bg: hsl(38 25% 95% / 0.1); --qt-sidebar-item-active-bg: hsl(43 75% 55% / 0.18); color: var(--estate-cream); }` + three `:is(...)` rules — icons/labels cream, toggle/view-all/profile-email `hsl(38 20% 78%)`, the active item `--estate-gold-light`).
- `theme.json` bumps: great-estate 1.0.8, rains 1.3.12. Storybook 1.0.88 NO-PORT.

### 1.3 `58120a7fe` — WCAG AA contrast / focus / reduced motion (56 files, +362 / −245)

**Default palette** (`app/globals.css`, mirrored in `lib/themes/default-tokens.ts` — v5 has ONE home, `src/styles.css`):

| mode | token | old | new |
|---|---|---|---|
| light | `--theme-success` | `hsl(152 60% 38%)` | `hsl(152 60% 31%)` |
| light | `--theme-warning` | `hsl(38 90% 46%)` | `hsl(38 90% 32%)` |
| light | `--theme-warning-foreground` | `hsl(0 0% 10%)` | `hsl(0 0% 100%)` |
| light | `--theme-info` | `hsl(210 75% 48%)` | `hsl(210 75% 44.5%)` |
| light | `--theme-sidebar-muted` | `hsl(225 8% 50%)` | `hsl(225 8% 44.5%)` |
| dark | `--theme-muted-foreground` | `hsl(225 8% 56%)` | `hsl(225 8% 58%)` |
| dark | `--theme-destructive` | `hsl(0 60% 54%)` | `hsl(0 60% 62%)` |
| dark | `--theme-destructive-foreground` | `hsl(0 0% 100%)` | `hsl(225 18% 9%)` |
| dark | `--theme-success-foreground` | `hsl(0 0% 100%)` | `hsl(225 18% 9%)` |
| dark | `--theme-info-foreground` | `hsl(0 0% 100%)` | `hsl(225 18% 9%)` |
| dark | `--theme-sidebar-muted` | `hsl(225 8% 48%)` | `hsl(225 8% 51%)` |

**`_variables.css`:** `:575` light `--qt-queue-badge-summary-bg: hsl(152 55% 38%)` → `33.5%`; `:933` dark `hsl(152 50% 42%)` → `34.5%`; `:988-996` the reduced-motion rule's selector `[data-theme="default"] *` → `*, *::before, *::after` with the comment `/* Every theme, not just the default: a bundle's own animations (and the infinite pulses in qt-components) must honour the OS setting too. */`.

**`_utilities.css:581-598`:** NEW `.qt-text-success-foreground { color: var(--color-success-foreground); }`, `.qt-text-warning-foreground`, `.qt-text-info-foreground`, `.qt-text-destructive-foreground` (each `var(--color-<x>-foreground)`), inserted after `.qt-text-destructive`, under the comment `Text ON a status fill (qt-bg-success / -warning / -info / -destructive) — each theme pairs these with its status colour at 4.5:1 or better.`

**`lib/toast.tsx`:** the container gets `role="status"`, `aria-live="polite"`, `data-toast-container=""` (comment `// Polite live region: screen readers announce each toast as it lands.`); `getToastStyles` returns one string per type — success `qt-bg-success qt-text-success-foreground qt-border-success`, error `qt-bg-destructive qt-text-destructive-foreground qt-border-destructive`, warning `qt-bg-warning qt-text-warning-foreground qt-border-warning`, info `qt-bg-info qt-text-info-foreground qt-border-info`; each toast div gets `role={toast.type === 'error' ? 'alert' : undefined}` (comment `// Errors interrupt; everything else waits its turn in the polite region.`); `text-white` dropped. The container's `zIndex` stays `'1000'`.

**`components/ui/BaseModal.tsx:96-123`:** `const titleId = useId()`; `const handleFocusKeyDown = useDialogFocus(modalRef, isOpen)`; the dialog card gets `role="dialog" aria-modal="true" aria-labelledby={titleId} tabIndex={-1} onKeyDown={handleFocusKeyDown}`; `<h2 id={titleId} className="qt-dialog-title">`.

**NEW `hooks/useDialogFocus.ts`** (65 lines): `FOCUSABLE = 'button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])'`; on open records `document.activeElement`, focuses the dialog in a `requestAnimationFrame` UNLESS the dialog already contains the active element (an `autoFocus` field); the cleanup cancels the frame and refocuses the previous element if `isConnected`; the returned keydown handler: Tab only; if no focusables → `preventDefault()` and return; `Shift+Tab` from the first focusable OR FROM THE DIALOG ITSELF wraps to last; `Tab` from last wraps to first.

**`components/ui/SlideOverPanel.tsx`:** the inline save/restore effect (`:39-50`) and the trap (`:75-97`, selector WITHOUT `:not([disabled])`) are deleted; `const handleKeyDown = useDialogFocus(panelRef, isOpen)`.

**16 `aria-label`s on icon-only buttons** (byte-exact): `"Close"` on AIWizardModal, AddCharacterDialog, BulkCharacterReplaceModal, CreateNPCDialog, GenerateImageDialog, MergeConversationModal, ReattributeMessageDialog, StandaloneGenerateImageDialog, memory-editor, TaskDetails, wardrobe import-from-image-modal, wardrobe-item-editor; `"Close gallery"` (DescriptionSourceStep's Select Image gallery); `"Cancel"` (HelpEntityPicker); `"Dismiss message"` (ThemeSelector + ThemeBrowser install-status banners).

**Theme packs** (styles.css + tokens.json + theme.json each; versions art-deco 1.0.13, earl-grey 1.3.9, great-estate 1.0.9, madmans-box 1.1.9, old-school 1.0.14, rains 1.3.13): Art Deco NEW `--deco-bronze: hsl(43 80% 20%)` for `--qt-link-color` and `--qt-tab-active-fg` (light), dark `--qt-tab-active-fg: var(--deco-gold-dark)`, queue badges darkened; Great Estate NEW `--estate-bronze: hsl(43 70% 31%)` (links + active tab light), dark `--qt-tab-active-fg: var(--estate-gold)`; Earl Grey / Rains status `--theme-*` lines repointed (dark success/destructive/info foregrounds → `hsl(var(--bg-100))`, dark `--theme-destructive: hsl(0 67% 68.1%)`, Rains dark `--theme-info: hsl(210 50% 61%)`), `--text-400` / `--text-300` lifted, Earl Grey `--qt-link-color` per mode (`hsl(var(--accent-secondary-000))` dark, `hsl(211 100% 46%)` light); queue badge fills darkened in core + five themes (`summary-bg` → 30% in four packs, `danger-bg` 55%→49.5%, `memory-bg` 55%→53% in dark); the tokens.json contrast moves per pack are in the diff (24 / 22 / 18 / 12 / 22 / 28 changed lines). The exact values must be byte-copied, not retyped — §9.

**`help/appearance-settings.md:274-281`** NEW "### Legibility and Motion" section (three paragraphs; P4.D360). NO-PORT: `README.md`, `knip.json`, `package.json` (the `create-quilltap-theme` dependency drop), `packages/create-quilltap-theme/*`, `packages/theme-storybook/*`, `__tests__/unit/lib/toast-stacking.test.ts` (the selector flip — v5's twin is `e2e/toast-stacking-flow.spec.ts`, which already selects `.qt-toast-container` by class and is unaffected).

### 1.4 `dff76ebef` — dialog semantics (70 prod files, +1,133 / −213; `plugins/dist` NO-PORT)

The pattern, applied to 54 dialog files (full per-file list in the scratch census — the lane regenerates it with `git show dff76ebef | gawk …`): on the dialog CARD (not the overlay) `role="dialog"` (`"alertdialog"` on: `alert-dialog.tsx`, `character-delete-dialog`, `ArchiveCharacterDialog`, `DeleteProjectDialog`, `DeleteDocumentStoreDialog`, `WorkbenchEditor`'s conflict dialog "The file has moved under your hand", `MemoryCascadeDialog` "Delete Message", `WardrobeModeChangePrompt` "Switch to a single garment?", AuroraView's "Reset Built-in Characters"), `aria-modal="true"`, `aria-labelledby={titleId}` with `id={titleId}` on the visible title (`aria-label` instead where there is no title: `ImageDetailModal` `aria-label={`Image viewer: ${image.filename}`}`, `search-dialog` `aria-label="Search"`), `tabIndex={-1}`, `onKeyDown={useDialogFocus(ref, open)}`, and `useEscapeKey(onClose, <open && !busy>)` where Escape was missing (busy gates kept: `!working`, `!saving`, `!isGenerating`, `!moving`, `!isProcessing`, `!resetBuiltinsInProgress`, `!showKeepResetPrompt`, `alreadyQueued || !creating`).

Other hunks:
- **`hooks/useDialogFocus.ts:22-47`:** the opener is now captured DURING RENDER (`useState({open, opener})` flipped when `isOpen` changes) so an `autoFocus` field cannot steal it before the effect runs; the cleanup restores `opener` if `isConnected`.
- **`components/ui/FloatingDialog.tsx`:** `role="dialog" aria-modal="false" aria-labelledby={titleId} tabIndex={-1}`, `useDialogFocus(dialogRef, isOpen)` (focus in / restore only — the handler is NOT attached, so no Tab trap), the title div gets `id={titleId}`, the close button `aria-label="Close"`; Escape: a keydown listener ON the element (`if (topmost) e.stopPropagation(); onClose()`) plus a document listener that closes only when the target is outside the dialog AND `!document.querySelector('[aria-modal="true"]')`; both skip `e.defaultPrevented`.
- **NEW `components/ui/activate-on-key.ts`** (20 lines): `if (e.key !== 'Enter' && e.key !== ' ') return; if (e.target !== e.currentTarget) return; e.preventDefault(); e.currentTarget.click()`.
- **Clickable rows / tiles → `role="button" tabIndex={0} onKeyDown={activateOnKey}`:** `DocumentStoreCard` title `<span>` inside the `<h2>`; `FileTable` file-name `<div>` (+ `aria-expanded={expanded}`); `FileBrowserList` ".." cell (`<span … aria-label="Up one folder">..</span>`) and folder-name `<span>`; `llm-logs-card` row div; `image-gallery` tile's `aspect-square` div (only when `onSelectImage && !missing`, + `aria-pressed={selectedImageId === image.id}`); `DocumentPane` title `<span className="qt-doc-title cursor-pointer">` (rename).
- **`AuroraView.tsx:528-536`:** the character card's name becomes a `<Link href={`/aurora/${id}/view`}>` (in-tab: `preventDefault` + `setSelectedCharacterId`).
- **`CharacterBasicInfo.tsx:235`** alias remove button `aria-label={`Remove alias ${alias}`}`; **`nav-user-menu-quick-hide.tsx:81`** tag toggles `aria-pressed={isHidden}`.
- `help/appearance-settings.md:282-285` two paragraphs (P4.D360).

### 1.5 `eed72d60c` — accessible names (95 prod files, +488 / −172)

Shape, byte-exact where it matters (the complete per-file attribute list is in the hunks; the lane carries the strings from there):

- Visible `<label>` ↔ control: `const id = useId()` (or `fieldId`) and `htmlFor={`${id}-<slug>`}` / `id={`${id}-<slug>`}` — ~130 sites.
- Heading captions → `aria-labelledby` on the control (ProfilesTab ×9: the h2/h3 captions incl. `Self-Dressing`, `Outfit Creation`, `Inventory Management`; ImageGenerationCard ×4: `Avatar Generation`, `Default Image Profile`, `Announce Lantern Images to Characters`, `Story Backgrounds`; ModelBehaviorCard ×3: `Agent Mode`, `Answer Confirmation`, `Default Roleplay Template`; FieldSelectionStep `background-heading`; SuggestionCard `Amend the Proposed Wording`; ContextCompressionSettings' switch `aria-labelledby` the `Enable Context Compression` label).
- `aria-label` strings (verbatim): `"Search characters"` (AuroraView? no — GenerateImageView, AddCharacterDialog, EntitySearchDropdown, GenerateImageDialog, CharacterPickerPanel), `"Search conversations"` (character-conversations-tab, MergeConversationModal), `"Search the table"` (CustomToolRunDialog), `"Search tools"` (RunToolModal), `"Search memories"` / `"Sort memories by"` / `"Filter by source"` (memory-list), `"Search themes"`, `"Search npm plugins"`, `"Search outfits"`, `"Search items to add as components"`, `` `Search ${slotLabel} items` `` (equipped-slot-row), `"Search"` (search-bar, search-dialog, SearchInput fallback `placeholder || 'Search'`), `"Filter list"` (HelpEntityPicker), `"Filter files"` + `"File description or transcript"` (FileTable), `"Message (Markdown source)"` (ChatComposer source textarea), `"Edit message"` (MessageRow), `"Message"` (HelpChatComposer), `"Document title"` / `"Document source"` / `"Document content"` (DocumentPane), `"Up one folder"` + `"New folder name"` (DocumentPickerModal), `"Directory path"`, `"Folder"` + `"New folder path"` (FolderPicker), `"Project name"` / `"Project description"`, `"Group color (hex value)"`, `"Document store to link"`, `"Character to add"`, `"User-controlled character"`, `"Add alias"`, `"Scenario title"`, `"Number of rolls"` / `"Sides per die"` (RngDropdown), `"Image profile"`, `"Freeform text"` / `"Connection profile"` (AIImportWizard), `"Show theme selector in navigation"`, `"Sample text input"`, `"Daily token budget"` / `"Freshness window in hours"`, `"Log retention in days"`, `"Try smart typography"`, `"Try text replacement"` / `"Replace this text"` / `"With this text"`, `"Embedding model"` / `"Image model"`, `"Add a tag"`, `"Type DELETE to confirm"`, `"Character"` (ScopeSelectionStep), `"Item title"` (import-from-image), `"Delimiter"` / `"Opening delimiter"` / `"Closing delimiter"` / `"Closing bracket"` (roleplay templates), `` `Whisper to ${targetName}` `` (WhisperDialog), `"Toggle model list"` (ModelSelector chevron, + `aria-expanded={isOpen}`), `` `Enable ${plugin.name}` `` (plugins-tab switches), JsonSchemaForm `aria-label={required.has(key) ? undefined : key}` / `aria-label={propKey}`.
- Switches: `SettingsTab.tsx:61-64` Allow Any Character `type="button" role="switch" aria-checked={!!project.allowAnyCharacter} aria-label="Allow Any Character"`; ChatSidebar `aria-label="All Whispers"` / `"Shared Vaults"` / `"Turn Skipping"`; the Core whisper `Offering` / `Cadence` and the `Thinking` / `Answer Confirmation` captions become `<label htmlFor>`s.
- `ModelSelector` gains `id?: string`; `ariaLabel = id ? undefined : placeholder` on the select and the search input.
- `ParticipantCard`: both `Talkativeness` captions become `<label htmlFor={`${talkId}-active` | `-na`}>` and the ranges take the ids.
- `connection-profiles/ProfileModal.tsx` passes `id="modelName"` (its `<label htmlFor="modelName">`).

### 1.6 `68340008b` — composer / editor names (6 files, +10 / −5)

- `LexicalComposerWrapper.tsx:149` the rich `ContentEditable` gets `aria-label="Message"`.
- `MarkdownLexicalEditor.tsx`: `EditorBody` takes `ariaLabel?: string` and puts `aria-label={ariaLabel ?? 'Markdown editor'}` on the editable; the outer frame `<div className={frameClassName} role="group" aria-label={ariaLabel}>`.

### 1.7 `10e09b7c3` — remaining items (29 files, +300 / −21)

- `components/layout/app-layout.tsx:113-116`: `<a href="#main-content" className="qt-skip-link">Skip to content</a>` as the first child of `.qt-app-layout`; `<main id="main-content" tabIndex={-1} className="… focus:outline-none">`.
- `_interactive.css:416-437` NEW `.qt-skip-link { position: absolute; left: 0.5rem; top: -10rem; z-index: 60; padding: 0.5rem 1rem; border-radius: 0.375rem; background: var(--color-primary); color: var(--color-primary-foreground); font-size: 0.875rem; font-weight: 500; text-decoration: none; }` and `.qt-skip-link:focus { top: 0.5rem; outline: 2px solid var(--color-ring); outline-offset: 2px; }`.
- `SalonView.tsx:1341-1344` `announcerNameFor(participantId)` → `chat?.participants.find(p => p.id === participantId)?.character?.name`; `:1582-1583` `<h1 className="sr-only">{chat?.title || 'The Salon'}</h1>` and `<ReplyAnnouncer messages={renderMessages} nameFor={announcerNameFor} />` as the first children of `.qt-chat-main`.
- NEW `components/chat/ReplyAnnouncer.tsx` (70 lines): `MAX_SPOKEN_CHARS = 300`; `spokenText` = whitespace-collapsed, trimmed, cut to 300 + `…`; `latest` = the LAST message with `role === 'ASSISTANT' && !systemSender`; a ref `undefined` until the first render seeds it (history is never spoken); thereafter a NEW latest id sets `` `${name} replied: ${spokenText(content)}` `` with `nameFor(participantId) ?? 'A character'`; renders `<div className="sr-only" role="status" aria-live="polite" aria-atomic="true">{announcement}</div>`.
- `DocumentPane.tsx:755-760`: `<span className="sr-only" role="status" aria-live="polite">{editingIndicator ?? (saveStatus.label === 'Saved' ? 'Saved' : '')}</span>` beside the status items.
- `MessageRow.tsx:388` `{isOverheardWhisper && ' (overheard)'}` appended inside the whisper label.
- `ParticipantCard.tsx:288-289` `{isDangerousChat && <span className="sr-only">Unmoderated chat.</span>}` as the card's first child.
- `_workspace.css:223-226` `.qt-tab-close` `width/height: 1.15rem; margin-left: 0.125rem` → `width: 1.5rem; height: 1.5rem; margin: -0.175rem -0.25rem -0.175rem 0;` with the comment `/* 24px: the WCAG 2.5.8 minimum target. The negative margin keeps the tab's footprint. */`.
- `CopyChatIdButton.tsx:51` adds `min-w-6 min-h-6` to the inline variant's class.
- `Terminal.tsx:100-108` `term.attachCustomKeyEventHandler(event => { if (event.key !== 'F6' || event.type !== 'keydown') return true; event.preventDefault(); moveFocusOutOf(containerRef.current, event.shiftKey ? -1 : 1); return false; })`; NEW module-level `FOCUSABLE` (same selector as useDialogFocus) and `moveFocusOutOf(container, direction)` (`:233-251`): all focusables not inside the container with `offsetParent !== null`, the first FOLLOWING (or last PRECEDING) by `compareDocumentPosition`, else `blur()`.
- `_icons.css:52-60` (and the generator) `@media (forced-colors: active) { [data-icon] { forced-color-adjust: none; } }` with a four-line why-comment.
- `_layout.css:738-750` `@media (max-width: 639px) { .qt-page-toolbar { flex-wrap: wrap; row-gap: 0.5rem; } .qt-page-toolbar-right { flex-basis: 100%; justify-content: center; } }` (comment `/* Phone widths: the queue badges take a row of their own rather than sliding under the search button (WCAG 1.4.10 reflow). */`).
- `footer-wrapper.tsx:52` `.qt-footer-container` gains `max-sm:pl-14` — **NO-COUNTERPART**: v5 has no `qt-footer` / `<footer>` in the shell (grep: the only `<footer>` is `screens/photos/photos-page.ts:335`).
- Themes: `earl-grey/styles.css:1358-1363` `:focus-visible` outline `hsl(var(--border-300) / 0.3)` → `var(--color-ring)` (1.3.10); `great-estate/styles.css:693-695` light h1–h3 `color: var(--estate-bronze)` (1.0.10); `rains/tokens.json` dark `mutedForeground` `hsl(48 4.8% 59.2%)` → `hsl(48 4.8% 62%)` (1.3.14).
- `help/appearance-settings.md:286` + `help/ariel.md:67` (F6 paragraph) — P4.D360. `scripts/generate-icon-css.ts` NO-PORT (v5's `_icons.css` is hand-maintained, `_icons.css:1-8`).

### 1.8 `99463fc56` — always-on floating Brahma Console (13 prod files, +785 / −394)

- NEW `components/brahma-console/BrahmaConsoleLauncher.tsx`: a `createPortal`ed `<button type="button" className="qt-brahma-launcher[ qt-brahma-launcher-active]" title={label} aria-label={label} aria-pressed={isOpen} data-qt-floating-layer>` with `label = isOpen ? 'Hide the Brahma Console' : 'Open the Brahma Console'`, `<Icon name="brahma-console" className="qt-brahma-launcher-icon" />`; never disabled.
- NEW `components/brahma-console/BrahmaConsolePanel.tsx` (render-prop `{headerActions, body}`): the launcher view + transcript + composer moved out of the dialog; `loadMessages` now treats a 404 as "deleted elsewhere" → `setCurrentChatId(null)`; `handleShowLauncher` (clears chat id + model, refetches past chats, refocuses the composer after 50 ms); `pickerActiveId = activeConnectionProfileId ?? (currentChatId ? null : profiles.find(p => p.isDefault)?.id ?? null)`; header: the `ModelPicker` ALWAYS when eligible, plus — with a chat open — a `title="Past conversations"` clock button and a `title="New conversation"` plus button (both call `handleShowLauncher`); the launcher-view composer now takes `inputRef`. User-visible strings unchanged (`The Console wants for an engine. Establish a connection profile first, then return to confer.`, `Recent Console Conversations`, `Untitled`, `No prior conversations. Pose a question below to open a fresh line to the engine.`, placeholders `What shall we put to the engine?` / `Speak to the engine…`). `console.error` lines: `Failed to load Brahma Console messages:`, `Failed to create Brahma Console chat:`, `Failed to delete Brahma Console chat:`, `Failed to switch Brahma Console model:` (browser console only; no capture pin).
- NEW `hooks/useBrahmaConsoleSession.ts`: per-surface `{currentChatId, setCurrentChatId, activeConnectionProfileId, setActiveConnectionProfileId, setModel}`, localStorage key per surface — floating `'quilltap:brahma-floating-last-id'`, tab `'quilltap:brahma-console-last-id'`; `setModel` PATCHes `?action=set-model` only with a chat open.
- `BrahmaConsoleDialog.tsx` → `<BrahmaConsolePanel session active={isOpen}>` inside `<FloatingDialog … storageKey="quilltap:brahma-console-geometry" minWidth={480} minHeight={400} initialGeometry={{width: 560}} topmost>`; stays MOUNTED while hidden. `BrahmaConsoleView.tsx` renders the panel bare with the tab session.
- `brahma-console-provider.tsx`: drops `currentChatId` / model state; keeps `isOpen`, `openConsole` (NO longer resets the chat id), `closeConsole`, NEW `toggleConsole`, `profiles` (+ `isDefault: boolean` on `BrahmaConnectionProfile`), `isEligible`.
- `FloatingDialog.tsx`: `topmost?: boolean` → class `qt-floating-dialog-topmost` + `data-qt-floating-layer`; Escape listener on the element (swallowed) instead of the document when topmost.
- NEW `components/ui/floating-layer.ts`: `FLOATING_LAYER_ATTR = 'data-qt-floating-layer'`, `floatingLayerProps`, `isInFloatingLayer(target)`.
- `hooks/useClickOutside.ts`: returns early for a mousedown or Escape whose target `isInFloatingLayer`.
- `_variables.css:279-291` `--qt-floating-layer-z: 1000;` + nine `--qt-brahma-launcher-*` tokens (`size 2.75rem`, `offset 0.75rem`, `bg var(--color-primary)`, `fg var(--color-primary-foreground)`, `hover-bg color-mix(in srgb, var(--color-primary) 88%, black)`, `active-bg var(--color-background)`, `active-fg var(--color-primary)`, `ring var(--color-primary)`, `shadow 0 6px 18px -4px rgb(0 0 0 / 0.35)`); `_surfaces.css:291-298` `.qt-floating-dialog-topmost { z-index: var(--qt-floating-layer-z); }`, `:332-375` the `.qt-brahma-launcher` family (`position: fixed; left: calc(var(--qt-left-sidebar-collapsed-width) + var(--qt-brahma-launcher-offset)); bottom: var(--qt-brahma-launcher-offset); z-index: var(--qt-floating-layer-z); …`, `:hover` scale 1.06, `:focus-visible` outline, `-active` inverted, `-icon` 60%).
- `app-layout.tsx:141` mounts `<BrahmaConsoleLauncher />` after the dialog.
- `help/brahma-console.md` rewritten (two doors; the HelpChat direction line) — P4.D360. Tests: `__tests__/unit/components/ui/FloatingDialog-topmost.test.tsx` (4 cases) and `useClickOutside.test.ts` (+3) — the v5 twins are component specs (§6.2).

### 1.9 `2041bb875`, `a4945d8d9`, `3b8b6f42b` (SPA), `9a88f862f` (SPA), `8cb584eff` (SPA)

- **`2041bb875`:** `AutomationSettings.tsx:78` checkbox class `mt-1 h-4 w-4 rounded border-input text-primary focus:ring-primary` → `qt-checkbox mt-1`.
- **`a4945d8d9` `AboutView.tsx:377-381`:** "Markdown & Documents" body becomes `unified, remark-parse, remark-gfm, remark-math, remark-rehype, rehype-stringify, rehype-highlight, rehype-katex, remark-breaks, remark-smartypants, KaTeX, react-markdown, react-syntax-highlighter, mammoth, PDF.js, yaml, jsonrepair, MessagePack` (drops `pdf-parse`); NEW `Diagrams: Mermaid, Graphviz (@hpcc-js/wasm-graphviz), svg2roughjs, DOMPurify` after it; NEW `Terminals & Realtime: xterm.js, node-pty, ws, croner` after "UI & Interaction". NOTE the pin's "Markdown & Documents" line ALREADY differs from v5's copy by `remark-math, remark-rehype… rehype-katex` — v5's `about-page.ts:449-451` body lacks `remark-math`, `rehype-katex` and `KaTeX` (it was copied at an older v4), so the port replaces the whole string.
- **`3b8b6f42b` SPA:** `AutoLockSettingsCard.tsx:88` and `useRoleplayTemplates.ts:247` write the PUT response into the `settings.chat` query (`queryClient.setQueryData(queryKeys.settings.chat, await res.json())`).
- **`9a88f862f` SPA:** NEW `components/chat/MentionChip.tsx` (`AbsentMentionChip {characterId, text, card: {name, aliases, pronouns, body}}`, `MentionChipsContext`, `MentionCard` with `Also known as` / `Pronouns` / the quote / hint `Not in this conversation · click to pin`, `<span className="qt-mention-chip" tabIndex={0} role="button" aria-label={`${name}, not in this conversation`}>` inside a `Tooltip interactive pinnable delay={250}`, `withMentionChips(text)` keyed on the private-use marker); NEW `hooks/useAbsentMentions.ts` (one `?action=absent-mentions` read per chat, `staleTime 30_000`, `useRealtimeRefetchInterval(60_000)`, filtered per `messageId`); `MessageContent.tsx` threads `mentions` through `segmentsToReactNodes` / `processChildren` and adds `[rehypeMentionMarkers, mentions]` after `rehypeKatex`; `LazyMessageContent.tsx` forces the full render when `mentions.length > 0`; `MessageRow.tsx` calls the hook and `assignMentionsToRuns` across tool-split runs; `SalonView.tsx:409` filters `!isOffSceneMentionIntroduction(msg)` out of the feed; `_chat.css:700-716` `.qt-mention-chip` (`color: inherit; padding: 0 0.25em; border-radius: 0.375rem; background: color-mix(in srgb, var(--color-primary) 14%, transparent); border-bottom: 1px dotted color-mix(in srgb, var(--color-primary) 70%, transparent); cursor: help; box-decoration-break: clone`) + hover/focus 26%.
- **`8cb584eff` SPA:** `memory-card.tsx:79-86` badge `<span className="text-xs px-2 py-0.5 rounded-full qt-bg-primary/10 qt-text-favorite" title="Keystone: a standing fact, always remembered in full and never tidied away">Keystone</span>` before the Archived badge; `memory-editor.tsx:206-225` a `qt-checkbox` labelled `Keystone — always remembered` with the help sentence `A standing fact, set in stone until you yourself chip it out. Whenever its subject is in the room, in the character's own circle, or is the character themself, it is recalled in full, and no tidying of the Book will ever fold, digest or shelve it. Five to a subject, at most.`, sent as `keystone` on save; `memory-list.tsx:315-323` a `Keystones only` checkbox (`title="Show only the standing facts you have set in stone"`) adding `keystone=true` to the list params.

---

## 2. The v5 surface map

### 2.1 CSS / tokens / themes (P4.D357's area)

| v4 file | v5 counterpart | v5 CURRENT shape (the red) |
|---|---|---|
| `app/globals.css` + `lib/themes/default-tokens.ts` | `apps/web/src/styles.css` (ONE home — v5 has no `default-tokens.ts`; grep `DEFAULT_LIGHT_COLORS` → 0) | light `:87` success 38%, `:90` warning 46%, `:91` warning-fg 10%, `:93` info 48%, `:106` sidebar-muted 50%; dark `:140` muted-fg 56%, `:145` destructive 54%, `:146` destructive-fg white, `:149` success-fg white, `:155` info-fg white, `:167` sidebar-muted 48% — all OLD |
| `_variables.css` | `apps/web/src/styles/qt-components/_variables.css` | `:389` streaming-fg `hsl(152 55% 38%)` (old); `:551` light summary-bg 38%, `:888` dark 42% (old); `:946-947` `[data-theme="default"] *` (old). The alert tokens exist (`:110` warning-fg, `:114` error-fg; dark `:806`, `:810`) |
| `_utilities.css` | `…/_utilities.css` | NO `qt-text-*-foreground` classes; `:593-595` `.qt-text-destructive`, `:597-606` the `qt-text-danger` alias; `:640-660` a comment that calls the `-foreground` spelling a dead class and names `qt-text-on-{accent,primary,success,destructive}` (`:662`ff) as the real ones — no `-on-warning` / `-on-info` exist |
| `_chat.css` | `…/_chat.css` | `:164-169` plate text has no `color:` line; `:1948-1951` `.qt-chat-message-content .qt-chat-dialogue { --qt-rp-fg: var(--qt-rp-dialogue-color) … }` with no user-bubble rule after it; `:3101-3106` badge `color: var(--qt-concierge-badge-color)`, `:3109-3111` muted, `:3119-3123` mark `--qt-concierge-mark-color: var(--color-destructive, #dc2626)` — all OLD |
| `_intervention.css`, `_storyteller.css` | ABSENT (P4.D346 / P4.D344 create them from the pin) | — |
| `_interactive.css` | `…/_interactive.css` | no `.qt-skip-link`; the `@layer` block ends after `.qt-tab-icon` (file tail) |
| `_layout.css` | `…/_layout.css` | `.qt-page-toolbar` `:705-711`, `-right` `:724-729`, the `(min-width: 768px)` block `:732-736`; no `max-width: 639px` rule |
| `_workspace.css` | `…/_workspace.css` | `:220-228` `.qt-tab-close` `1.15rem / 1.15rem / margin-left: 0.125rem` (old) |
| `_icons.css` | `…/_icons.css` | `[data-icon]` paint block `:45-50`; no `forced-colors` rule; hand-maintained (no generator) |
| `_surfaces.css` (`99463fc56`) | `…/_surfaces.css` | `.qt-floating-dialog` family EXISTS `:279-323` (z-[58]); no `-topmost`, no launcher family |
| `lib/toast.tsx` colours | `…/_toast.css` | tinted `--qt-status-*` triples (`:84-104`); `z-index: 1000` (`:36`); recorded divergence in its header `:11-17` |
| `themes/bundled/<id>/{styles.css,tokens.json,theme.json}` | `apps/web/public/themes/<id>/…` | at the BASELINE's bytes (see §0 #1); `theme.service.ts:221` loads `/themes/${id}/styles.css`; `src/app/theme/bundled-themes.ts` carries `theme.json`'s id / name / description / previewColors / fonts (NO version — no regen needed for a version bump) |
| `packages/theme-storybook/*`, `packages/create-quilltap-theme/*` | NO-COUNTERPART | standing NO-PORT (v5 has no storybook; `status-log.md` P4.d17 et seq.) |

### 2.2 Dialogs — the overlay census (P4.D358's area)

`qt-modal` (`ui/modal.ts`) is hosted by **77** files (`grep -rl '<qt-modal' src/app`); it already has `role="dialog" aria-modal="true"` and a close button `aria-label="Close"` (`:35-36, :44`) but NO `aria-labelledby`, NO `tabindex`, NO focus move / trap / restore, NO Escape. One edit there covers every host (v4's BaseModal hosts AND most of v4's hand-rolled dialogs, which v5 had already moved onto `qt-modal`: e.g. `memory/memory-editor.ts`, `images/generate-image-dialog.ts`, `images/standalone-generate-image-dialog.ts`, `screens/settings/system/task-details-modal.ts`, `screens/characters/generators/wizard/description-source-step.ts` are all `qt-modal` hosts — so their `58120a7fe` close `aria-label`s are ALREADY covered).

Hand-rolled overlays in v5 (every non-spec file under `src/app` carrying `qt-dialog-overlay` or `fixed inset-0`), measured 2026-10-29:

| v5 file | role=dialog | aria-labelledby / aria-label | Escape | v4 twin (dff76ebef role) |
|---|---|---|---|---|
| `almanack/almanack-report-dialog.ts` | yes | label ×2 | yes | capabilities-report-dialog (dialog) |
| `brahma/brahma-console-dialog.ts` | yes (`aria-label="Brahma Console"`) | — | NO | FloatingDialog (dialog, aria-modal=false) → **P4.D359** |
| `chat/bulk-character-replace-modal.ts:66` | yes | title not linked | NO | dialog |
| `chat/cast/add-character-dialog.ts:82` | yes | label | yes | dialog |
| `chat/cast/create-npc-dialog.ts:60` | yes | label | NO | dialog |
| `chat/memory-cascade-dialog.ts` | **NO** | NO | NO | **alertdialog** "Delete Message" |
| `chat/merge-conversation-modal.ts:77` | yes | not linked | NO | dialog |
| `chat/post-office/whisper-dialog.ts:41` | yes | NO | yes | dialog `Whisper to {name}` |
| `chat/reattribute-message-dialog.ts:50` | yes | not linked | yes | dialog |
| `chat/sidebar/chat-sidebar.ts` | n/a (a mobile drawer, not a dialog) | — | yes | — (P4.D344's file) |
| `help/help-dialog.ts:82` | yes | label | NO | FloatingDialog semantics (HelpChatDialog is a FloatingDialog in v4) |
| `images/full-screen-image-viewer.ts:95` | yes | label | yes | — (v5's own frame) |
| `images/image-detail-modal.ts` | **NO** | NO | yes | dialog `aria-label="Image viewer: <filename>"` |
| `images/image-modal.ts:35` | yes | NO | yes | — |
| `images/photo-gallery-modal.ts` | **NO** | labels ×4 (not the dialog) | yes | dialog (title) |
| `memory/maintenance-dialog.ts` | yes | not linked | yes | dialog |
| `memory/memory-digest-viewer.ts` | yes | NO | yes | dialog "The Digest" |
| `memory/memory-list.ts:214` | yes | not linked | NO | dialog "The Digest" → **P4.D359** owns the file |
| `progressions/progression-editor-modal.ts` | **NO** | labels ×3 | NO | dialog |
| `screens/characters/generators/ai-import/ai-import-wizard.ts` | **NO** | 1 | yes | dialog "Summon From Lore" |
| `…/external-prompt/external-prompt-dialog.ts` | **NO** | NO | yes | dialog |
| `…/external-prompt/external-prompt-result-dialog.ts` | **NO** | NO | yes | dialog |
| `…/optimizer/character-optimizer-modal.ts` | **NO** | 1 | yes | dialog |
| `…/wizard/ai-wizard-modal.ts` | **NO** | NO | yes | dialog; close button lacks `aria-label="Close"` |
| `screens/custom-tools/destination-picker.ts:44` | yes (on the OVERLAY) | NO | NO | dialog "Where shall Pascal keep this contrivance?" |
| `screens/custom-tools/workbench-editor.ts:298` | yes (on the OVERLAY) | not linked | NO | **alertdialog** "The file has moved under your hand" |
| `screens/files/file-preview-modal.ts:52` | yes | label | yes | — |
| `screens/new-chat/green-room-dialog.ts:24` | yes | NO | yes | NewChatModal (dialog) |
| `screens/photos/photos-page.ts:244` | yes | `aria-labelledby="photo-detail-title"` | yes | already v4-shaped |
| `screens/profile/avatar-picker.ts:42` | yes | not linked | NO | avatar-selector (dialog "Select Avatar") |
| `screens/startup/startup-screen.ts:19` | alertdialog + labelledby | — | — | already shaped |
| `search/search-dialog.ts` | **NO** | 2 (inputs) | yes | dialog `aria-label="Search"` |
| `ui/alert-modal.service.ts:67` | alertdialog | NO labelledby | yes | alert-dialog (`aria-labelledby={messageId}` on the `<p>`) + prompt-dialog |
| `wardrobe/item-editor/wardrobe-mode-change-prompt.ts` | **NO** | 1 | NO | **alertdialog** "Switch to a single garment?" |
| `wardrobe/wardrobe-control-dialog.ts:159` | yes | label | yes | dialog |
| `wardrobe/wardrobe-item-editor.ts` | **NO** | labels ×4 | NO | dialog; close has `aria-label="Close"` already |

Not found by name in v5 (the lane locates or records NO-COUNTERPART): wardrobe `import-from-image-modal` (no `*import-from-image*` under `src/app`), `ImportKeysDialog`, `StateEditorModal`, v4's `components/import/import-wizard.tsx` + `memory-creation-dialog.tsx` (SillyTavern chat import — v5's twin may be under `screens/settings/system/`). `aria-labelledby` appears 7× in the whole SPA (`inform-dialog.ts:92`, `insert-announcement-dialog.ts:264`, `startup-screen.ts:20`, `photos-page.ts:245`, `wear-history-section.ts:46` + 2 specs); `role="button"` 0×; `alertdialog` 2×.

### 2.3 Clickable rows / tiles (dff76ebef) → v5

| v4 | v5 | current |
|---|---|---|
| `DocumentStoreCard` title | `screens/scriptorium/store-card.ts:30` `<h2 class="qt-section-title truncate">{{ store().name }}</h2>` inside a card with `(click)="open.emit()"` `:22` | no inner `role="button"` |
| `FileTable` name cell | `screens/scriptorium/file-table.ts:169-179` `<tr (click)="toggleExpand(file)">` + name `<td>` | no `role="button"` / `aria-expanded` |
| `FileBrowserList` ".." + folder rows | `files/file-manager/file-manager.ts` (v5's bespoke D18 manager — the lane confirms it still has a ".." row) | — |
| `llm-logs-card` row | `screens/settings/system/llm-logs-card.ts:88-89` `class="qt-card p-3 w-full text-left …" (click)="openLog(log)"` — looks like a real `<button>` already (verify the tag) | likely CONVERGED |
| `image-gallery` tile | `images/image-gallery.ts:54-56` tile `(click)="onSelectImage()?.(image)"`, `aspect-square` div | no role / tabindex / `aria-pressed` |
| `DocumentPane` title | `documents/document-pane.ts:116` `<span class="qt-doc-title cursor-pointer" title="Click to rename" (click)="startTitleEdit()">` | no role / tabindex |
| `AuroraView` card name → Link | `screens/characters/*` list card (the lane locates the name `<h2>`) | — |
| alias "×" | `screens/characters/edit/details-tab.ts:152` `&times;` | no `aria-label` |
| quick-hide `aria-pressed` | `quick-hide/quick-hide-menu-section.ts` | lane measures |

### 2.4 Names sweep (eed72d60c) → v5

A whole-SPA sweep: the approximate census (`scratchpad/name-census.py`, a regex over inline templates: a control is "named" if its tag carries `aria-label` / `[attr.aria-label]` / `aria-labelledby` / an `id`, or sits inside an open `<label>`) found **676 controls, 143 unnamed in 73 files**. Top: `screens/characters/view/tabs/defaults-tab.ts` 12 (the ProfilesTab twin — the heading-captioned selects), `screens/settings/templates/template-delimiter-editor.ts` 11, `settings/memory/embedding-profiles/embedding-profile-modal.ts` 7, `settings/images/image-profile-modal.ts` 6, `screens/new-chat/timestamp-config-card.ts` 5, `screens/characters/edit/details-tab.ts` 5 (CharacterBasicInfo: pronoun `Subject` / `Object` / `Possessive` labels at `:205`ff, the alias input), `…/generators/rename/rename-replace-tab.ts` 4, `…/ai-import/ai-import-wizard.ts` 4, `ui/model-selector.ts` 3 (`:28` select, `:39` search input — no `id` input, no `aria-label`, no `Toggle model list` / `aria-expanded` on the chevron), `memory/memory-list.ts` 3 (only `aria-label="Filter by tier"` at `:119` exists), `documents/document-pane.ts` 3 (title input `:109`, the two textareas), `autonomous/autonomous-room-defaults.ts` 3, `files/file-manager/file-manager.ts` 3, then 2s and 1s (`search/search-bar.ts`, `search/search-dialog.ts`, `brahma/help-composer.ts` — the v4 `aria-label="Message"` site, `help/help-entity-picker.ts`, `chat/bulk-character-replace-modal.ts`, `screens/custom-tools/builder-form.ts` ×2 — P4.D346's file, `screens/settings/chat/text-replacement-settings.ts` ×2, `screens/settings/wizard/steps/api-key-entry-step.ts` ×2, `wardrobe/equipped-slot-row.ts`, `wardrobe/outfit-quick-pick.ts`, `images/image-profile-picker.ts`, …). Already converged: `visibility-section.ts` switches (`:66-72, :89-95, :121-127` `role="switch" [attr.aria-checked] aria-label="All Whispers|Shared Vaults|Turn Skipping"`), but its `Offering` / `Cadence` / `Thinking` / `Answer Confirmation` captions are still `<span>`s (`:148, :162, :181, :198`); `project-characters-card.ts:87-89` `role="switch" … aria-label="Allow Any Character"`; `chat/message-row.ts:207` `aria-label="Edit message"`; `chat/chat-composer.ts:498` `"Message (markdown source)"` (lower-case m — one byte off); `chat/sidebar/participant-card.ts:264 / :281` `<span>Talkativeness</span>` with the range named `'Talkativeness for ' + name()` (`:273`) — named differently, so v4's `<label for>` is a shape change, not a defect fix.

### 2.5 `68340008b`, `10e09b7c3`, `2041bb875`, `99463fc56`, riders → v5

| v4 | v5 | current |
|---|---|---|
| composer `aria-label="Message"` | `chat/chat-composer.ts:521` passes `ariaLabel="Message"`; `editor/rich-editor.ts:255-257` puts `aria-label` + `aria-multiline` on the ProseMirror editable | CONVERGED |
| editor frame `role="group"` + default `'Markdown editor'` | `editor/markdown-field.ts:107` threads `ariaLabel` (default `'Editor'` `:118`; `rich-editor.ts:79` default `'Editor'`); no `role="group"` on the frame | residual: the frame role; the default string (`'Editor'` vs `'Markdown editor'`) |
| skip link + `<main id>` | `shell/shell.ts:199` `<main class="flex flex-col flex-1 min-h-0 overflow-hidden">` — no id, no tabindex, no skip link | ABSENT |
| Salon `<h1>` + `ReplyAnnouncer` | `screens/salon/salon-conversation.ts` (the SalonView twin; the chat main container) — no `sr-only` h1, no live region | ABSENT; NEW `chat/reply-announcer.ts` |
| DocumentPane live "Saved" | `documents/document-pane.ts:261-273` status bar (`saveLabel()` `:356-360`: `'Saving...'` / `'Unsaved'` / `'Saved'`; the AI indicator is the literal `AI editing...` at `:271`) | ABSENT |
| `(overheard)` | `chat/message-row.ts:183` `<div class="qt-chat-whisper-label">whispered to {{ whisperTargets() }}</div>`; `isOverheardWhisper()` exists (`:178`) | ABSENT |
| `Unmoderated chat.` | `chat/sidebar/participant-card.ts` (`isDangerousChat` input `:429`, class at `:500-510`) | ABSENT |
| tab close 24px | `_workspace.css:223-225` (CSS); `workspace/chrome/tab-strip.ts:59` the button | CSS only |
| copy-id `min-w-6 min-h-6` | `ui/copy-chat-id-button.ts:32` inline variant class | ABSENT |
| F6 out of xterm | `terminal/terminal.ts:103-128` (`term.open(el)` at `:117`); no `attachCustomKeyEventHandler` | ABSENT |
| footer `max-sm:pl-14` | NO-COUNTERPART (no shell footer) | record |
| `qt-checkbox` (2041bb875) | `screens/settings/chat/automation-settings.ts:65` `class="mt-1 h-4 w-4 rounded border-input text-primary focus:ring-primary"` | OLD — P4.D346's file |
| Brahma launcher / panel / session / floating layer | `brahma/brahma-entry.ts` (rail button, mounts `<qt-brahma-console-dialog />` under `@defer (when isConsoleOpen())` `:50-52`), `brahma/brahma-console-dialog.ts` (a `qt-dialog-overlay` scrim modal, `asTab` mode `:60-70`), `brahma/brahma-console-view.ts` (`[asTab]="true"`), `brahma/brahma-console.service.ts` (ONE `currentChatId` + `STORAGE_KEY_LAST_CHAT = 'quilltap:brahma-console-last-id'` `:51`; `openConsole()` RESETS the chat id `:95-98`; no `toggleConsole`; `isDefault` not on the picker profile type — verify `mapBrahmaConnectionProfiles`), `brahma/model-picker.ts`, `brahma/help-composer.ts`; `ui/icon.ts:37` has `'brahma-console'` | the v4 structure is absent end to end |
| About credits | `screens/about/about-page.ts:435-459` (verbatim v4 list, older vintage) | port the three strings |
| auto-lock cache write | `screens/settings/system/auto-lock-settings-card.ts:161-170` `persist()` → `ChatSettingsCard.save()` (base class) | the lane measures whether `save()` writes the response row into the `['chat-settings']` query; v4's bug shape may not exist |
| roleplay default-template cache write | `screens/settings/templates/roleplay-templates-card.ts:305-318` dispatches `chatSettingsUpdate` then `invalidateQueries(['chat-settings'])` `:312` | v5 invalidates (a refetch, not a stale read) — likely verify-only; if `chatSettingsUpdate` returns the row, `setQueryData` is the byte-faithful shape |
| name chips | `chat/message-row.ts`, `chat/message-content.ts`, `chat/render/markdown-renderer.ts` (unified pipeline — the rehype plugin's home), `screens/salon/salon-conversation.ts` (the feed filter), `ui/tooltip.ts` (exists; `interactive` / `pinnable` inputs to verify), `_chat.css` | ABSENT (`grep -rn 'mention' src/app` hits only the composer typeahead) |
| keystone UI | `memory/memory-card.ts` (`:165-166` the Archived badge), `memory/memory-editor.ts` (`:84-86` importance label), `memory/memory-list.ts` (`:117-125` the tier select), `memory/memory.api.ts:68-70` (`memoryList` params) | `grep keystone src/app/memory` → 0 |

### 2.6 The three adapted theme packs — the exact v5 deltas against the baseline

- `art-deco/styles.css:593` `url('/api/themes/assets/bundle:art-deco/textures/background-light.webp')` → `url('/themes/art-deco/textures/background-light.webp')`; `:603` likewise `background-dark.webp`.
- `great-estate/styles.css:587` `…bundle:great-estate/textures/background.webp` → `/themes/great-estate/textures/background.webp`; `:671` `carbon-fibre.png`.
- `madmans-box/styles.css:553` `home-bg.webp` URL; `:635` comment `<Link>s wearing .qt-button` → `links wearing .qt-button`; `:693-701` the `thinking` glyph comment rewritten (+ five lines explaining the unlayered re-declaration); `:710-713` NEW `[data-theme="madmans-box"] [data-icon="thinking"] { --_qt-icon-mask: url("/themes/madmans-box/icons/brand.svg"); --_qt-icon-bg: none; }` (recorded at `status-log.md:36716-36720`, P4.d17 unit 5).

The rule for the lane: `cp` the 15 byte-identical files from the pin; for the three adapted `styles.css`, apply `git diff 322a5488e 236c04e35 -- themes/bundled/<id>/styles.css` as a patch onto v5's file (the hunks are far from the adapted lines; `patch -p3` applies cleanly or the lane re-applies by hand) and then prove `diff <(git show 236c04e35:themes/bundled/<id>/styles.css) apps/web/public/themes/<id>/styles.css` is EXACTLY the adaptation list above.

---

## 3. Schema / D23 shapes

None. Every commit in this survey is SPA-only. `8cb584eff`'s `memories.keystone` column and `9a88f862f`'s `?action=absent-mentions` are the keystone lane's (P4.D341) and the context lane's (P4.D347) respectively; P4.D359 consumes their wire (§6.4).

## 4. Log lines

No server log lines. Browser-console lines (no capture pins in v5's SPA convention): `99463fc56`'s four `console.error` texts (§1.8) — v5's `brahma-console-dialog.ts` already carries v4's pre-refactor set; the lane keeps the bytes when it moves them into the panel.

## 5. Harness / oracle impact

- `grep -rln 'default-tokens\|lib/themes\|lib/toast' harness/oracle` → nothing. No oracle case imports any file these commits touch; no family reddens, nothing is IMPORT-DEAD or GARBLED in this area.
- The only harness-adjacent surface is `help/`: `crates/quilltap-harness/tests/help_doc_ensure_equivalence.rs` compares the vendored `help/` tree through a committed fixture (`harness/oracle/fixtures/help-ensure/help/…`). The four help pages in scope (`appearance-settings.md` +14 lines over three commits, `ariel.md` +2, `brahma-console.md` rewritten, `pascals-workbench.md` one sentence) are all SAME-AS-BASELINE in v5 today; **P4.D360 re-vendors them and rebuilds the help-ensure fixture** (its recipe is in that test's header `:55-62`).
- The import-dead census in the brief lists nothing in this area; confirmed — none of the nine known-dead cases touch SPA / theme code.

## 6. SPA: component map, specs, verification design, wire

### 6.1 Component map — see §2 (every row names the v5 file:line). New v5 files this survey expects:

- P4.D358: `apps/web/src/app/ui/dialog-focus.ts` (+ spec) — the `useDialogFocus` twin (a directive `qtDialogFocus` taking an `open` input, or a small class the overlays instantiate); `ui/activate-on-key.ts` (+ spec) — `activateOnKey(event: KeyboardEvent)` + a `[qtActivateOnKey]` directive for templates; `chat/reply-announcer.ts` (+ spec); `e2e/support/toasts.ts`; `scripts/check-a11y-names.mjs` (+ `--self-test`) if the static census is adopted (§6.3).
- P4.D359: `ui/floating-dialog.ts` (+ spec) if §10 #2 is ruled YES; `ui/floating-layer.ts`; `brahma/brahma-console-launcher.ts`; `brahma/brahma-console-panel.ts`; `brahma/brahma-console-session.ts`; `chat/mention-chip.ts`; `chat/render/rehype-mention-markers.ts` (the plugin twin — its matcher is P4.D347's `name-mentions` port; the SPA needs a TS twin of `splitMentionMarkers` / `assignMentionsToRuns`, which are v4 `lib/chat/mention-markers.ts` — confirm with the context survey whether that module is ported to Rust only or also needed in TS; the private-use marker bytes must match the server's).
- P4.D357: `src/app/theme/theme-contrast.spec.ts` (data-only test), optionally `src/app/theme/contrast-ratio.ts` (a REAL WCAG contrast helper — keep it out of `screens/settings/appearance/contrast.ts`, whose shortcut is pinned by `theme-preview.spec.ts:30-35`).

### 6.2 Component specs touched / added

- `ui/modal.spec.ts` (currently only the `overlayClass` cases `:30-41`) gains: `aria-labelledby` resolves to the `<h2>`'s text; `tabindex="-1"`; focus moves to the dialog after a frame unless a descendant already has focus; Tab from the last focusable wraps to the first, Shift+Tab from the first OR from the dialog element wraps to the last; `:not([disabled])` excludes a disabled button; zero focusables → `preventDefault`; destroy restores focus to the element focused at construction (if connected); Escape (if ruled) emits `close` unless `closeOnEscape` is false.
- `ui/slide-over-panel.spec.ts` (`:79-113` already pin role/aria-modal/inert) gains the three behavioural deltas (disabled skipped; Shift+Tab from the panel itself; empty → preventDefault).
- `ui/toast.spec.ts:105-143` — three assertions select `[role="toast-container"]`; they move to `[data-toast-container]` and gain `role="status"` + `aria-live="polite"` + error `role="alert"` cases.
- Per-overlay specs (every file in §2.2 that P4.D358 owns and that has a spec) gain one case: "announces as a dialog named by its title" (`role`, `aria-modal`, `aria-labelledby` → the title's text; `alertdialog` where v4 says so) and one Escape case where Escape is newly added.
- `chat/reply-announcer.spec.ts` (transcribe v4's semantics — there is no v4 test; write the four cases from the hunk: silent on first render with history; speaks a new ASSISTANT row; ignores `systemSender` rows and USER rows; 300-char cut + `…`; `A character` fallback).
- `shell.spec.ts`: the skip link is the first focusable; `#main-content` has `tabindex="-1"`. `documents/document-pane.spec.ts`: the sr-only status text. `chat/sidebar/participant-card.spec.ts`: `Unmoderated chat.` present iff `isDangerousChat`. `chat/message-row.spec.ts`: ` (overheard)`. `terminal/terminal.spec.ts`: drive the fake term's `attachCustomKeyEventHandler` with F6 / Shift+F6 (the spec already fakes the term, `terminal.ts:183-190`).
- P4.D359: `ui/floating-dialog.spec.ts` — transcribe `FloatingDialog-topmost.test.tsx`'s four cases (class + layer marker; mousedown inside leaves the modal open; Escape inside closes only the dialog; Escape in the modal closes only the modal) and `useClickOutside.test.ts`'s three; `brahma/*.spec.ts` for the launcher (`aria-pressed`, label flip), the per-surface sessions (two storage keys), the 404 fallback, the default-profile picker; `memory/memory-card.spec.ts` / `memory-editor.spec.ts` / `memory-list.spec.ts` for the keystone badge / checkbox / filter param; `chat/mention-chip.spec.ts` (transcribe v4's `components/chat/__tests__/MentionChip.test.tsx`, 82 lines).

### 6.3 Verification design (the brief's three asks)

**(a) The contrast values as a token test.** `src/app/theme/theme-contrast.spec.ts` (vitest, Node `fs`): load `public/themes/*/tokens.json` (both modes) and parse `src/styles.css`'s `:root` / `.dark` `--theme-*` declarations into the same token names (the default theme — v4 measures it from `app/globals.css` too); convert `hsl(h s% l%)` → sRGB → relative luminance → WCAG contrast (write it fresh — do NOT reuse `screens/settings/appearance/contrast.ts`, whose `getLuminance` returns the HSL lightness for `hsl()` inputs by v4's design). Assert v4's checklist pairs (`.claude/commands/release-checklist-13-ui.md:19-23` at the pin): every `X`/`XForeground` pair (`primary`, `secondary`, `muted`, `accent`, `destructive`, `card`, `popover`, `success`, `warning`, `info`, `chatUser`) ≥ 4.5; `foreground` and `mutedForeground` on `background` / `card` / `muted` / `popover` ≥ 4.5; `border` / `input` on `background` / `card` ≥ 3 and `ring` on `background` / `card` / `input` ≥ 3; `destructive` / `success` / `warning` / `info` as text on `background` ≥ 4.5. Expect a FEW pairs to fail even at the pin (v4's own commit message says only the listed classes were fixed; e.g. `chatUser: transparent` in Earl Grey has no contrast) — the test takes a committed per-theme allowlist of known residues with the measured ratio, so a NEW regression fails and the residues are visible. Add a second block that pins each pack's `theme.json` `version` to the pin's six values (a re-vendoring tripwire). Tier-1 "exact" for the data itself is the byte copy (§9).

**(b) The axe pass as a Playwright project.** Add a project `{ name: 'a11y', testMatch: '**/a11y/*.spec.ts', use: devices['Desktop Chrome'] }` to `playwright.config.ts:27` and `e2e/a11y/axe-sweep.spec.ts` using `@axe-core/playwright` (NEW dev dep, §10 #4): unlock the shared global-setup server (the `maybeUnlock` pattern of `e2e/settings-brahma-console-flow.spec.ts:31-40`), then for each of a fixed page list (home, the fixture chat, Settings × tabs, the characters list, one open `qt-modal`) run `new AxeBuilder({page}).withTags(['wcag2a','wcag2aa','wcag21aa','wcag22aa'])`, once per theme × mode by setting `data-theme` + `.dark` on `<html>` (`aa-foundation.spec.ts:57-67` shows the theme flip), and diff the violations against a committed `e2e/a11y/known-violations.json` keyed by `rule + target selector`. Fail on anything not in the file; the lane records the file's starting size and drives it down. If the dep is refused, (c) alone is the gate.

**(c) The static accessible-name census.** `apps/web/scripts/check-a11y-names.mjs`, modelled line-for-line on `scripts/check-qt-classes.mjs` (header rationale, `--self-test`, runs from `npm run lint`): for every `<input|select|textarea>` in an inline template, require one of `aria-label` / `[attr.aria-label]` / `[attr.aria-labelledby]` / `aria-labelledby` / an `id` (static or bound) with a matching `for=` in the same template / an enclosing `<label>`; for `role="switch"` / `role="button"` elements require a name; exempt `type="hidden"`. My approximate run gives the starting red: **143 / 676 in 73 files**. The script takes an allowlist for files OWNED BY A SIBLING this round (the HANDOFF files in §7.3), which the unifier empties.

**(d) Dialog semantics.** Component specs as in §6.2 plus ONE committed table `apps/web/src/app/ui/dialog-roles.table.ts` (`{ v5: 'chat/memory-cascade-dialog', v4: 'components/ui/MemoryCascadeDialog.tsx', role: 'alertdialog', name: 'title' }` …) that the lane fills from the dff76ebef hunks; a vitest reads the table and the source files and asserts each listed file's template contains the role and a name binding (a cheap static tripwire; the behavioural cases stay per-spec).

### 6.4 Wire contract (for P4.D359, consumed not built)

- Name chips: the context survey maps v4 `GET /api/v1/chats/[id]?action=absent-mentions` → `{ mentions: Array<{ messageId, characterId, text, card: { name, aliases, pronouns, body } }> }` and the realtime invalidation of `['chats', id, 'absent-mentions']`. P4.D359 needs the v5 names: proposed `Request::ChatAbsentMentions { chat_id }` → `Response::ChatAbsentMentions { mentions }` (`crates/quilltap-core/src/api/types.rs`), SPA `type: 'chatAbsentMentions'` in `core/core-contract.ts` beside the `chatInventory*` verbs (`:9773`ff), and the realtime topic → query key `['chats', id, 'absent-mentions']`. **P4.D347 builds it; P4.D359 consumes it** — the exact names are the context survey's to fix; if P4.D359 lands first the chips are gated behind a named constant (the `BRAHMA_CONSOLE_SERVER_LANDED` pattern, `e2e/settings-brahma-console-flow.spec.ts:30`).
- Keystone: `memoryList` (`core-contract.ts:6069`) gains `keystone?: boolean`; memory rows gain `keystone: boolean`; `memoryCreate` (`:6087`) / `memoryUpdate` (`:6091`) accept `keystone`, with the two refusal texts (`An archived memory cannot be a keystone; it belongs to the digest that replaced it.` / `A character may keep at most 5 keystones about one subject; unmark one first.`) surfacing as a 400 — P4.D341 builds, P4.D359 consumes; the keystone survey fixes the names.
- `3b8b6f42b`: nothing new on the wire for the SPA half; the server half (the `coreWhisper` arm + the 34-key allow-list) is P4.D356's — note for P4.D359's verify step that after P4.D356 lands, any v5 SPA `chatSettingsUpdate` caller that echoes a read-back row would 400.

---

## 7. The lane split, by file ownership

### 7.1 Confirmed lanes

P4.D357 = CSS / tokens / themes; P4.D358 = templates / TS semantics + e2e; P4.D359 = riders. Confirmed, with the ownership rules below.

### 7.2 P4.D357 and P4.D358 are disjoint — proof

- **P4.D357 edits ONLY:** `apps/web/src/styles.css`; `apps/web/src/styles/qt-components/{_variables,_utilities,_chat,_interactive,_layout,_workspace,_icons,_toast}.css`; `apps/web/public/themes/**` (18 files); NEW `apps/web/src/app/theme/theme-contrast.spec.ts` (+ optionally `theme/contrast-ratio.ts`). No file under `src/app/**` other than those two new ones; no `e2e/**`.
- **P4.D358 edits ONLY:** files under `apps/web/src/app/**/*.ts` (templates, components, specs — none in `src/app/theme/`), `apps/web/e2e/**`, `apps/web/scripts/check-a11y-names.mjs`, `apps/web/playwright.config.ts` (if the axe project is adopted — P4.D358 owns it; P4.D357 does not touch it). No `.css` file, nothing under `public/`.
- **Meeting points (shared contracts, by literal name):** (1) the class `qt-skip-link` — P4.D357 defines it in `_interactive.css`, P4.D358 emits it in `shell/shell.ts`; `scripts/check-qt-classes.mjs` does not police a bare component class on an `<a>` (its scope note `:26-33`), so neither lane breaks `npm run lint` alone. (2) `.qt-tab-close` / `[data-icon]` / `.qt-page-toolbar` are CSS-only changes on classes v5 templates already emit — no template edit. (3) The toast: P4.D358 changes attributes in `ui/toast-container.ts`; P4.D357 changes colours in `_toast.css`; the class names `qt-toast-{success,error,warning,info}` are unchanged on both sides. (4) `qt-text-*-foreground`: P4.D357 adds the classes; NO v5 template uses them (the toast uses `qt-toast-*`), so no consumer this round.

### 7.3 Every v5 file the four WCAG / legibility commits would touch, with its owner this round

Rule for a file wanted by two lanes: **the lane with the larger or structural hunk owns the file; the a11y lane records a HANDOFF naming the rule(s) R1–R12 and v4's bytes; the owner applies them in its own commit; the unifier checks the handoff list.** Ties go to the feature lane (the a11y edits are mechanical).

| v5 file | wanted by | owner | HANDOFF content |
|---|---|---|---|
| `screens/salon/salon-conversation.ts` | P4.D344 (StorytellerBoard mount, turn-mode wiring, +41), P4.D352 (prose mode +7/+2), P4.D358 (`<h1 class="sr-only">{{ chat?.title \|\| 'The Salon' }}</h1>` + `<qt-reply-announcer [messages] [nameFor]>` as the first children of `.qt-chat-main`), P4.D359 (the `isOffSceneMentionIntroduction` feed filter, 1 line) | **P4.D344** | P4.D352 / P4.D358 / P4.D359 each record their exact template hunk; P4.D358 ships `chat/reply-announcer.ts` standalone with its spec |
| `chat/message-row.ts` | P4.D353 (`diagramStyle` threading +12), P4.D359 (mentions hook + `assignMentionsToRuns` +15), P4.D358 (` (overheard)` at `:183`; `Edit message` already converged) | **P4.D353** (see stacking, §7.5) | P4.D358: R10 one line; P4.D359: stacks |
| `chat/message-content.ts` | P4.D353 (the code-block → DiagramBlock rewrite, +129/−), P4.D359 (rehype marker plugin + chip rendering) | **P4.D353** | P4.D359 stacks |
| `chat/sidebar/participant-card.ts` | P4.D353 (+3), P4.D358 (R10 `Unmoderated chat.`; R4 Talkativeness `<label for>`) | **P4.D358** | P4.D353 hands off its 3 lines (the lane names them) |
| `chat/chat-composer.ts` | P4.D352 (prose mode +4/+2), P4.D358 (R6: `:498` `Message (markdown source)` → `Message (Markdown source)`) | **P4.D352** | one byte |
| `editor/rich-editor.ts` | P4.D352 (the punctuation-lint ProseMirror plugin registration — likely), P4.D358 (68340008b: default label `'Editor'` → `'Markdown editor'`?) | **P4.D352** | R6: whether to flip the default is a judgment (v5 chose `'Editor'` deliberately in two files); recommend converge to v4's byte |
| `editor/markdown-field.ts` | P4.D358 only (R: `role="group"` + `aria-label` on the frame) | **P4.D358** — unless P4.D352's survey claims it | — |
| `screens/settings/chat/automation-settings.ts` | P4.D346 (+8 intervention rows), P4.D358 (`2041bb875` class) | **P4.D346** | `class="qt-checkbox mt-1"` at `:65` |
| `screens/settings/chat/smart-typography-settings.ts` | P4.D352 (+20), P4.D358 (R6 `aria-label="Try smart typography"`) | **P4.D352** | one attribute |
| `screens/settings/templates/{templates-tab,roleplay-templates-card,template-form-modal,template-delimiter-editor}.ts` | P4.D352 (Novel template SPA, 778b97b2b +263 on `index.tsx`), P4.D358 (R1 on the edit + preview dialogs; R4/R6 ~20 labels incl. `Delimiter` / `Opening delimiter` / `Closing delimiter` / `Closing bracket`, `Style (CSS class)`, `Kind` / `Name` / `Button Label` / `Line marker` / `Brackets:` / `Token pattern (regex, Unicode)`), P4.D359 (3b8b6f42b — verify-only, see §2.5) | **P4.D352** | P4.D358's rule list with the strings; P4.D359 verify-only |
| `screens/custom-tools/{builder-form,outcomes-section,side-effects-section,workbench-library}.ts` | P4.D346 / P4.D344 | **P4.D346** (library: P4.D344 ∥ P4.D346 — their surveys) | P4.D358: `builder-form.ts` ×2 unnamed controls (R4) |
| `screens/custom-tools/{workbench-editor,destination-picker}.ts` | P4.D358 only (R1: the conflict ALERTDIALOG; the picker dialog — move `role` from the overlay to the card, add `aria-labelledby`) | **P4.D358** | — |
| `screens/settings/system/llm-logs-card.ts` | P4.D344 (+1 storyteller round label), P4.D358 (R8 — likely already a button) | **P4.D358** | P4.D344 hands off its one line |
| `chat/sidebar/chat-sidebar.ts` | P4.D344 (+76) | **P4.D344** | none from P4.D358 (the switches live in `visibility-section.ts`) |
| `chat/sidebar/visibility-section.ts` | P4.D358 (R4: `Offering` / `Cadence` / `Thinking` / `Answer Confirmation` `<span>` → `<label for>` + ids on the selects, `:148-198`) | **P4.D358** — **conditional:** if P4.D344's turn-mode control lands here, P4.D344 owns and takes the handoff | — |
| `memory/memory-list.ts` | P4.D359 (keystone filter), P4.D358 (R1 the digest dialog `:214` `aria-labelledby`; R6 `Search memories` / `Sort memories by` / `Filter by source`) | **P4.D359** | the four items |
| `memory/memory-editor.ts`, `memory/memory-card.ts` | P4.D359 only (qt-modal host — R1 inherited) | **P4.D359** | — |
| `brahma/brahma-console-dialog.ts`, `brahma/brahma-entry.ts`, `brahma/brahma-console.service.ts`, `brahma/brahma-console-view.ts`, `brahma/model-picker.ts` | P4.D359 | **P4.D359** | P4.D358 hands off R2 (FloatingDialog semantics) + R3 (`aria-label="Close"` on the floating close) |
| `brahma/help-composer.ts` | P4.D358 (R6 `aria-label="Message"`) | **P4.D358** | — |
| `shell/shell.ts` | P4.D358 (skip link + `<main id="main-content" tabindex="-1" class="… focus:outline-none">`) | **P4.D358** | P4.D359 mounts the launcher INSIDE `brahma-entry.ts` (already in the shell) so it does not touch `shell.ts` |
| `screens/characters/edit/details-tab.ts` | P4.D358 (R3 `Remove alias ${alias}`; R4 pronoun labels `Subject` / `Object` / `Possessive` `:205`ff; R6 `Add alias`) | **P4.D358** — **conditional:** `eb90d9ed8` (pronounGuidance) also edits `CharacterBasicInfo.tsx` (−4/+3); if P4.D349 or another lane ports that SPA hunk, it owns the file | — |
| `screens/characters/edit/character-edit.ts`, `screens/characters/view/character-detail.ts` | P4.D353 (CanDrawDiagramsToggle, +38 / +37) | **P4.D353** | nothing from P4.D358 (its hunks are in `details-tab.ts` / `view/tabs/defaults-tab.ts`) |
| `screens/characters/view/tabs/defaults-tab.ts` (ProfilesTab twin, 12 unnamed) | P4.D358 (R5 ×9 heading `aria-labelledby`) | **P4.D358** | — |
| `wardrobe/wardrobe-item-editor.ts`, `wardrobe/equipped-slot-row.ts`, `wardrobe/wardrobe-control-dialog.ts`, `wardrobe/outfit-quick-pick.ts`, `wardrobe/item-editor/wardrobe-mode-change-prompt.ts` | P4.D358 (R1 on the editor + the ALERTDIALOG prompt; R6 search labels) — **conditional:** `eb90d9ed8`'s SPA half edits all of `wardrobe-item-editor.tsx`, `equipped-slot-row.tsx`, `wardrobe-control-dialog.tsx`, `outfit-selector`, `WardrobeBrowser`; whichever lane ports it owns these files | **P4.D358 unless a pronoun-guidance SPA lane exists** | — |
| `documents/document-pane.ts` | P4.D358 only (R6 ×3 names, R8 the title span, R11 the sr-only status) | **P4.D358** | — |
| `terminal/terminal.ts` | P4.D358 only (`85c002822` is server-only — measured: `app/api/v1/terminals/[id]/route.ts` alone) | **P4.D358** | — |
| `ui/modal.ts`, `ui/slide-over-panel.ts`, `ui/toast-container.ts`, `ui/toast.service.ts` (comment), `ui/model-selector.ts`, `ui/copy-chat-id-button.ts`, `ui/alert-modal.service.ts`, `ui/tooltip.ts` (no edit) | P4.D358 | **P4.D358** | `ui/tooltip.ts`: P4.D359 may need `interactive` / `pinnable` inputs for the chip — if absent, P4.D359 owns the tooltip edit |
| every other overlay in §2.2 and every other names-sweep file in §2.4 not listed above | P4.D358 | **P4.D358** | — |
| `screens/about/about-page.ts`, `screens/settings/system/auto-lock-settings-card.ts` | P4.D359 (About; auto-lock verify-only), P4.D358 (R4 `Lock after` `<label for>` on the auto-lock card) | About: **P4.D359**; auto-lock card: **P4.D358** (P4.D359 verify-only, no edit) | — |
| `_variables.css` | P4.D357 (`:389`, `:551`, `:888`, `:946-947`), P4.D352 (+12 new vars; v4 inserts at the status-tokens region, v5 ≈ `:100-115`), P4.D353 (+28 new vars; v4 inserts after the code-block vars, v5 ≈ `:580-600`), P4.D359 (+10 at `:277`, after `--qt-dialog-backdrop`) | **P4.D357 owns the file; siblings append FENCED blocks at the named anchors** — the contract: a sibling adds ONLY new declarations inside a `/* ==== <lane> ==== */` fence at its anchor and never edits an existing line; P4.D357 edits ONLY existing lines. The four hunks cannot overlap; `git merge` is clean | — |
| `_chat.css` | P4.D357 (`:164`, `:1955` insert, `:3101-3123`), P4.D352 (punctuation highlights, v4 anchor `:1035-1075` → v5's composer section), P4.D359 (`.qt-mention-chip`, v4 anchor `:697` → v5 before the `MARKDOWN ELEMENT SPACING` banner) | **P4.D357 owns; same fenced-append contract** | — |
| `_content.css` | P4.D353 only | **P4.D353** | — |
| `_surfaces.css` | P4.D359 only (`-topmost` + launcher family) | **P4.D359** | — |
| `_index.css` | P4.D344 (+`_storyteller`), P4.D346 (+`_intervention`) | their surveys (two adjacent one-line imports — union) | — |
| `_intervention.css`, `_storyteller.css` (NEW) | P4.D346 / P4.D344 | theirs — port from the PIN's bytes, which already contain `cce44e098`'s ink changes | P4.D357 records: nothing to do |
| `apps/web/public/themes/**` | P4.D357 (18 files) vs P4.D360 (vendoring) | **P4.D357** | P4.D360 takes none; records the rule in §2.6 |
| `help/appearance-settings.md`, `help/ariel.md`, `help/brahma-console.md`, `help/pascals-workbench.md` + the help-ensure fixture | P4.D360 | **P4.D360** | — |
| `apps/web/e2e/**` (the 20 toast spec files + new specs) | P4.D358 (the toast rewrite; new a11y specs); sibling lanes add NEW spec files only | **P4.D358** for the 20 files' toast lines (first commit, cherry-pickable) | any sibling editing one of the 20 files cherry-picks P4.D358's first commit |

### 7.4 The rules R1–R12 a sibling applies on a handed-off file (byte-exact where v4 has a byte)

- **R1 Modal dialog surface** (dff76ebef): the dialog CARD (never the overlay) carries `role="dialog"` — `role="alertdialog"` where the table in §2.2 says so — `aria-modal="true"`, `aria-labelledby="<titleId>"` with `id="<titleId>"` on the visible title element (v4's `useId()` → in v5 a per-instance id: `inject(UniqueId)` if the SPA has one, else a module counter `qt-dialog-<n>`); `tabindex="-1"`; the shared focus directive (`qtDialogFocus`, P4.D358's `ui/dialog-focus.ts`) for focus-in / Tab-trap / restore; Escape closes unless the host's busy gate says otherwise (keep v4's gate expression). For a `qt-modal` host: nothing — the primitive does it.
- **R2 Non-modal floating surface** (dff76ebef FloatingDialog): `role="dialog" aria-modal="false" aria-labelledby`, `tabindex="-1"`, focus in on open and back to the opener on close, NO Tab trap, close button `aria-label="Close"`, Escape inside closes it (topmost: `stopPropagation`), Escape elsewhere closes it only while `!document.querySelector('[aria-modal="true"]')`, both skipping `defaultPrevented`.
- **R3 Icon-only buttons** carry v4's `aria-label` byte: `Close`, `Cancel`, `Dismiss message`, `Close gallery`, `` Remove alias ${alias} ``, `Up one folder`, `Toggle model list` (+ `aria-expanded`).
- **R4 Visible label ↔ control:** `<label for="<id>">` + `id="<id>"` on the control; never a bare `<span>` caption beside a control.
- **R5 Heading-captioned control:** `aria-labelledby="<headingId>"` with `id` on the heading (ProfilesTab / ImageGenerationCard / ModelBehaviorCard / FieldSelectionStep / SuggestionCard / ContextCompression switch).
- **R6 Placeholder-only / caption-less control:** `aria-label` with v4's string from §1.5 (never invent a string; where v4 has none, do not add one).
- **R7 Switch:** `role="switch"` + `aria-checked` + a name (`aria-label` or `aria-labelledby`); the Prospero one is `type="button"` too.
- **R8 Clickable non-button:** `role="button" tabindex="0" (keydown)="activateOnKey($event)"` (P4.D358's `ui/activate-on-key.ts`) with `aria-expanded` (FileTable) / `aria-pressed` (gallery tile, quick-hide tags) where v4 sets them. Prefer a real `<button>` where the layout allows (v4 kept spans only inside `<tr>`/heading contexts and under the uppercase-button themes).
- **R9 Live region for notices:** the toast container `role="status" aria-live="polite" data-toast-container`; an error toast `role="alert"` (P4.D358 only).
- **R10 Colour-only state gets words:** `<span class="sr-only">Unmoderated chat.</span>`; ` (overheard)` appended to the whisper label.
- **R11 Async result announced:** `<span class="sr-only" role="status" aria-live="polite">…</span>` (DocumentPane: `editingIndicator ?? (saveLabel === 'Saved' ? 'Saved' : '')`).
- **R12 Pointer targets ≥ 24 px** — CSS, P4.D357 (`.qt-tab-close`); templates: `min-w-6 min-h-6` on the inline copy-id button (P4.D358).

### 7.5 Stacking

- **P4.D359 stacks on P4.D353's Tier-1 commit** if the name chips stay in P4.D359: both rewrite `chat/message-content.ts` / `chat/message-row.ts`, and the chips are a rehype plugin over the FINAL pipeline (v4 adds `rehypeMentionMarkers` after `rehypeKatex` in the same `rehypePlugins` array the diagrams work also touches). Alternative: split "Salon name chips" out of P4.D359 into a wave-1 sub-order and keep the rest of P4.D359 in wave 0. Planner's call; either way P4.D359's chips also depend on P4.D347's server action (gate behind a named constant until both land).
- P4.D357 and P4.D358 are wave 0, independent of each other and of the keystone.
- P4.D358's `salon-conversation.ts` / `message-row.ts` / `participant-card.ts` lines are HANDOFFs, not a stack.

### 7.6 The toast role change — every line that breaks, and who fixes it

Selector `[role="toast-container"]` → `[data-toast-container]` (the pin's handle). **Owner: P4.D358**, first commit, with `e2e/support/toasts.ts` (`export const toastStack = (page: Page) => page.locator('[data-toast-container] > div')` and `toastByClass(page, 'qt-toast-error')`). The 40 lines (20 files):

`e2e/toast-flow.spec.ts:14, :37, :39, :55`; `e2e/salon-dialogs-flow.spec.ts:116, :256, :872`; `e2e/salon-send-failure-restore-flow.spec.ts:220`; `e2e/memory-maintenance-flow.spec.ts:199`; `e2e/wardrobe-flow.spec.ts:1125, :1437, :1516, :1923, :2118`; `e2e/salon-interrupt-flow.spec.ts:331, :384, :448`; `e2e/scriptorium-flow.spec.ts:269`; `e2e/wardrobe-folders-flow.spec.ts:201`; `e2e/ai-import-flow.spec.ts:262`; `e2e/wardrobe-items-editor-flow.spec.ts:470`; `e2e/salon-cast-flow.spec.ts:130, :144, :227`; `e2e/salon-chain-pause-toast-flow.spec.ts:205, :224, :348`; `e2e/projects-flow.spec.ts:297, :377`; `e2e/profile-flow.spec.ts:94, :106`; `e2e/settings-memory-consolidation-flow.spec.ts:172`; `e2e/toast-open-rows-flow.spec.ts:25, :97`; `e2e/salon-attachment-ledger-flow.spec.ts:130`; `e2e/wardrobe-copy-to-folder-flow.spec.ts:193`; `e2e/salon-paused-hold-flow.spec.ts:212`. (`e2e/toast-stacking-flow.spec.ts:29, :130, :139` select `.qt-toast-container` by class — unaffected.) Component spec: `src/app/ui/toast.spec.ts:105, :107, :120, :139, :143`. Source comments: `src/app/ui/toast.service.ts:30, :37`, `src/app/ui/toast-container.ts:20`.

No sibling lane of this round is known to edit any of the 20 spec files (no wardrobe / project / profile lane this round); if one does, it cherry-picks the first commit.

---

## 8. Tiering

**P4.D357**
- Tier 1 (must land): the default palette (11 values, `styles.css`); `_variables.css` ×4; the four `qt-text-*-foreground` classes + the retired warning comment; `_chat.css` ×3 rules + the user-dialogue rule; `_workspace.css` tab-close; `_interactive.css` skip link; `_layout.css` phone rule; `_icons.css` forced-colors; all 18 theme-pack files at the pin's bytes (three via patch-on-adaptation) with the §2.6 proof; the token-contrast spec with its residue allowlist and the version tripwire; `_toast.css` per the ruling (§10 #3).
- Tier 2: a one-page `docs/developer/porting/theme-pack-vendoring.md` note? No — keep it in the lane record (the §2.6 rule) rather than a new doc.
- Tier 3 (loud refusals): none applicable (CSS has no refusal shape). Record the NO-PORTs: storybook, create-quilltap-theme, `generate-icon-css.ts`, the footer rule.

**P4.D358**
- Tier 1: `ui/modal.ts` semantics (+ Escape if ruled); `ui/slide-over-panel.ts` deltas; the toast role/live region + the 40-line e2e rewrite + `toast.spec.ts`; `ui/dialog-focus.ts` + `ui/activate-on-key.ts` (+ specs); the 13 role-less overlays and the role-on-overlay fixes (`destination-picker.ts`, `workbench-editor.ts`); the shell skip link / `main-content`; `chat/reply-announcer.ts` + the HANDOFF hunks; `(overheard)` / `Unmoderated chat.` / DocumentPane live status / F6; the names sweep over every P4.D358-owned file in §2.4 driving the census to zero outside the allowlist; the static census script in `npm run lint`.
- Tier 2: the axe Playwright project (dep ruling); `role="group"` on the editor frame; `model-selector.ts` `id` input + chevron name; the AuroraView name-as-link change; `copy-chat-id-button.ts` target size.
- Tier 3: nothing refuses here; record NO-COUNTERPARTs (footer; the dialogs not found by name in §2.2) explicitly in the lane record.

**P4.D359**
- Tier 1: the About credits (three strings); the keystone UI (badge / checkbox / filter) against the keystone lane's contract (gated spec cases until it lands); the Brahma console per the ruling (§10 #2): launcher (`qt-brahma-launcher`, labels, `aria-pressed`), per-surface sessions (two storage keys), the panel extraction, `openConsole` no longer resetting the chat, the default-profile picker on the launcher, the 404 fallback, the "Past conversations" button, `isDefault` on the profile type; if the floating primitive is ruled YES: `ui/floating-dialog.ts` with R2 + geometry under `quilltap:brahma-console-geometry` + `data-qt-floating-layer`, and the click-outside guards (`isInFloatingLayer`) on v5's document-level listeners that can dismiss a surface (`search/search-dialog.ts`, `chat/sidebar/chat-sidebar.ts:527`, `workspace/chrome/workspace-host.ts:210`, `images/standalone-generate-image-dialog.ts:256`, `screens/generate-image/generate-image-page.ts:329`, the row kebabs) — the lane censuses `document.addEventListener('mousedown'|'pointerdown'|'click'`.
- Tier 2: the name chips (stacked / gated as §7.5); the two cache-write verifications (`auto-lock`, `roleplay-templates-card`) — convert to `setQueryData` only if `chatSettingsUpdate` returns the row and v5's path is measurably stale.
- Tier 3: if the floating primitive is ruled NO, the launcher is NOT ported (a launcher that opens a scrim modal over a scrim modal is a regression) — record the whole of `99463fc56` as a ruled divergence with the two-door help text left to P4.D360's vendoring note.

## 9. Fixtures

- **Theme packs:** the 18 files under `apps/web/public/themes/**` are the committed "fixture"; delivered by P4.D357 as byte copies of the pin (15 files) + patched adaptations (3 files). Nobody regenerates anything from them (`bundled-themes.ts` carries no version).
- **e2e:** no fixture change; the shared global-setup server and the committed chat fixture serve the new specs. The axe sweep (if adopted) adds `e2e/a11y/known-violations.json` (P4.D358 delivers; the unifier re-runs it on the union, since siblings' new UI adds nodes).
- **Help-ensure fixture:** P4.D360 rebuilds it after re-vendoring the four help pages (`help_doc_ensure_equivalence.rs:55-62`).
- **Oracles:** none to regenerate in this area.

## 10. Rulings owed to the human

1. **`qt-modal` Escape.** v5's `modal.ts:22-24` convention ("handles no Escape") vs v4's `BaseModal` (`closeOnEscape` default true) and `dff76ebef`'s help text ("closes at a touch of Escape"). Options: (A) converge — add `closeOnEscape = input(true)`, a document `keydown` listener while mounted, hosts with busy states pass `[closeOnEscape]="!busy()"`; (B) keep the convention and record the divergence a third time. **Recommend A** — it is now documented user-facing behaviour in `help/appearance-settings.md`, and the census of hosts that ported `closeOnEscape` locally (`images/save-image-dialog.ts`, `status-log.md:177492`, `:178642`) shows the convention is already leaking.
2. **A `FloatingDialog` primitive for the Brahma console** (overturning the scoped "No `FloatingDialog`" ruling at `status-log.md:41020` / `brahma-console-dialog.ts:14`). v4's `99463fc56` + `dff76ebef` make the console a NON-modal topmost panel; v5's scrim modal cannot carry that. **Recommend YES, scoped to the console** (the CSS exists at `_surfaces.css:279-323`; draggable header + CSS `resize: both` + geometry persisted under v4's key; `aria-modal="false"`, no trap). Inform / compose-mail / insert-announcement / Help stay `qt-modal` this round.
3. **`_toast.css` colours:** converge to v4's solid status fills (`var(--color-<x>)` background, `var(--color-<x>-foreground)` text, `var(--color-<x>)` border — the pin's `qt-bg-* qt-text-*-foreground qt-border-*` triple) or keep v5's tinted `--qt-status-*` set. **Recommend converge** and retire the `_toast.css:11-17` divergence note.
4. **`@axe-core/playwright` as a dev dependency** (Deque, MPL-2.0; the SPA has no eslint / angular-eslint — adding those would be a larger decision). **Recommend yes**; otherwise the static census alone is the gate.
5. **About credits:** keep v5's "verbatim v4" policy and port the three strings (including a "Diagrams" line that is true only once P4.D353's four client deps are ruled in), or re-author the acknowledgments for v5's actual stack (Angular, ProseMirror, markdown-it, …). **Recommend verbatim now** (one port, one source of truth), with a standing note that the About page is v4's text by design.
6. **Theme-pack ownership:** P4.D357 owns all 18 files (recommended) vs splitting the 15 pure byte copies to P4.D360.
7. **The three-/four-lane files' owners** (§7.3): `salon-conversation.ts` → P4.D344; `message-row.ts` / `message-content.ts` → P4.D353 with P4.D359 stacked (or the chips split out). Confirm.
8. **The editor default name** `'Editor'` → `'Markdown editor'` (two v5 files chose `'Editor'`; v4's byte is `'Markdown editor'`). Recommend converge (byte rule).
9. **`qt-text-*-foreground`:** add the four as REAL classes (v4 did) and keep `qt-text-on-*` as v5 aliases, retiring the `_utilities.css:640-660` warning. Recommend yes — no ruling strictly needed, but it reverses a documented v5 stance.

## 11. Dogfood rows the lanes should queue

- Switch each of the six packs × light / dark on the Friday copy; raise a success, warning, error and info toast (a rename, a failed save) and read each against its fill; look at the header queue badges, links and the active tab under Art Deco / Great Estate light; the Concierge badge and the regenerating plate.
- OS reduced-motion ON under a bundled theme: the Salon's streaming quill and the toast slide must settle.
- VoiceOver pass: Tab once from a fresh load → "Skip to content"; the Salon `<h1>` reads the chat title; 💸 send one real turn and hear "<name> replied: …" (≤ 300 chars); a toast is announced; open five dialogs (a `qt-modal`, the Whisper dialog, the wardrobe item editor, the Pascal destination picker, the delete-message confirm) — each announces its title and the right role; Escape closes; focus returns to the opener.
- Keyboard-only: Tab into Ariel's terminal, F6 out, Shift+F6 back; Enter/Space on a Scriptorium store title, a file row, a gallery tile, the Document Mode title.
- 320 px viewport: the page toolbar wraps; the workspace tab close buttons hit at 24 px.
- 💸 The floating Brahma console (if ruled): open it over the connection-profile modal, ask the engine one question, confirm the modal stays and Escape in each surface closes only that surface; the tab and the floating window on two different conversations; hide mid-stream and reopen.
- Keystone UI on a real character: mark, see the badge, filter "Keystones only", hit the five-per-subject refusal text (needs P4.D341).
- 💸 Name chips: one turn that names an absent workspace character; the chip's hover card and pin (needs P4.D347).
- Automation settings: the per-row checkbox renders as `qt-checkbox` under Madman's Box (the theme restyles it).
