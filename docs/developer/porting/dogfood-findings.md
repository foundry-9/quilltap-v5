# Dogfood findings — the Friday smoke (started 2026-07-10)

The running log of findings from browsing a COPY of the real Friday instance
through `quilltap-web` + the SPA. Each finding is either fixed-in-place (with
its commit) or promoted into the next work order. The common thread so far:
**fresh-`generateDDL` schema vs the migration-accumulated schema of a real
instance** — the one divergence class synthetic fixtures structurally cannot
catch, since every fixture is built fresh.

| # | Finding | Class | Status |
|---|---|---|---|
| 1 | Salon list: `Invalid column type Integer … isSilentMessage` | Migration affinity — the `add-silent-message-field` migration declared INTEGER where fresh DDL says TEXT; the strictly-`String` read refused integer cells | **FIXED** `bcaa744` — `put_is_silent` reads the raw sql value (Integer/Real/Text coerced uniformly); regression tests over both table shapes; migrations audit found no other read-breaking affinity divergence |
| 2 | Chat GET: `no such column: timezone` | Never-migrated column — v4 added `chat_settings.timezone` with NO migration; its `SELECT *` reads tolerate the absence, the port's explicit column list errored | **FIXED** `bb71652` — `db::tolerant_select_list` (PRAGMA table_info → missing columns substituted `NULL AS "col"`), applied to `chat_settings::find_by_user_id`; `sidebarWidth` extraction NULL-tolerant; `settings_routes_equivalence` re-verified |
| 3 | A large Salon chat renders for 10+ s and lands stuck at the top (console: `'setTimeout' handler took 10196ms`, no errors) | TWO distinct causes — see #3a/#3b | split |
| 3a | NO chat could scroll at all (an 80-message chat reproduced it) | The scroll chain was broken for every chat: the v5 shell dropped v4 `app-layout.tsx`'s inner `flex-1 min-h-0 overflow-y-auto` scroller wrapper around the page content, and two unstyled Angular component hosts (`qt-salon-conversation`, `qt-message-list`) broke the flex/height chain React never has — `.qt-chat-messages`' own `overflow-y-auto` never got a bounded height. Fixture chats FIT the viewport and the e2e never scrolls, so it slipped through | **FIXED** — the shell scroller wrapper restored + `host:` classes on both components (`block h-full` / `flex flex-col flex-1 min-h-0`); a real scroll e2e beat lands with the long-chat fixture the virtualization deliverable needs anyway |
| 3b | The 10+ s synchronous render on a LARGE chat | No virtualization — every message renders through the full markdown pipeline in one task | **FIXED** (P4.6h) — the message list is virtualized with `@tanstack/angular-virtual` (a port of v4's own tanstack-virtual + `useAutoScroll` architecture) and markdown is memoized per `(content, renderingPatterns, dialogueDetection)`, so only the viewport + overscan rows pay the render cost. A separate ~300-message `salon-long-*.db` fixture backs the new `e2e/salon-scroll.spec.ts` (interactive < 3s, lands at bottom, windowed DOM, jump-button round-trip) |
| 4 | Clicking a character card on `/characters` does nothing unless the click hits the name/avatar exactly | Port divergence — v4's `AuroraView` card is clickable ANYWHERE (`cursor-pointer` + `handleCardClick`, which ignores clicks landing on inner buttons/links); the v5 card only linked the avatar+name row. The e2e never caught it because it clicked the name link directly | **FIXED** — the card div carries v4's whole-card click (the `closest('button')`/`closest('a')` guard preserved); a unit test proves navigate-from-body / no-navigate-from-star, and the e2e's detail-open beat now clicks the card BODY |
| 5 | The System Prompts view tab renders a prompt containing the character's name as scattered fragments with huge gaps — each name chip floats alone mid-screen | Port divergence (Angular-mechanics class, not schema) — v4 renders the body via a shared `TemplateDisplay` component inside `<pre><code>`; v5 had INLINED that markup into the tab template, and Angular preserves a template's literal whitespace inside `<pre>` elements, so every highlight segment rendered wrapped in the template's own newlines + ~20-space indentation. Fixture prompts are short one-liners, so nobody eyeballed it | **FIXED** — v4's `TemplateDisplay` ported as the shared `qt-template-display` (its own template compiles OUTSIDE any `<pre>`, so default whitespace stripping applies); both the System Prompts and Details tabs now use it (the Details tab's `div` + CSS `pre-wrap` variant wasn't affected but shared the duplicated markup); a unit test asserts the rendered `<pre><code>` textContent is BYTE-EXACT to the prompt content |
| 6 | The Default Settings tab "doesn't seem to accept edits" on Friday data | TWO port divergences, both Angular-mechanics class. (a) Error surfacing: v4 wraps every defaults save in try/catch + `showErrorToast`; v5's `save()` had `try/finally` with NO catch — a failed save would revert silently. (b) The actual cause: `<select [value]>` with async-loaded options — the value binding fires BEFORE the profiles/partner options render, the assignment finds no matching option and silently resets to `""`, and it never re-fires when options arrive (React re-renders `<select value>` after options change; Angular doesn't). So saves were in fact SUCCEEDING all along — the select just never displayed the stored value, so every render read as "the edit didn't take". Diagnosed live against the Friday copy: the character's stored `defaultConnectionProfileId` was among 36 rendered options while `select.value` was `""`; fixture characters have NO stored profile, so the fixture never exercised a non-empty value + async options | **FIXED** — (a) tab-level `qt-alert-error` with v4's fallback microcopy per control (`ba216ec`); (b) the profile/partner/prompt/scenario selects bind `[selected]` per option instead of `[value]` on the select (re-applies when options render); regression tests set the options/id inputs AFTER first render and assert the stored value displays. Verified live on the Friday copy: stored values display, an edit round-trips, no alert. ~8 more `[value]`+dynamic-options sites exist (settings modals, wizard, create screen) — audit filed in the standing notes |

## Standing notes for the next orders

- Finding #3 made **virtualization + post-render scroll-to-bottom** the first
  deliverable of the next Salon slice — it blocked dogfooding long-running
  chats outright. (Closed: P4.6h — `p4.6h-salon-virtualization.md`.)
- If findings of class #1/#2 keep appearing, the systematic close-out is a
  **migration-vintage fixture**: a test DB built by replaying v4's actual
  migration chain (instead of fresh `generateDDL`) so the differential harness
  can exercise real-instance shapes. Write it as its own small order if a
  third schema-divergence finding lands.
- `db::tolerant_select_list` is the reusable fix for any further
  `no such column` hits — apply it to the failing reader + add the
  migration-vintage regression test (the `chat_settings` pattern).
- **Audit the remaining `<select [value]>` + dynamic-options sites**
  (finding #6's class): a select-level `[value]` binding fires before
  async-loaded `@for` options render and silently resets to `""`; the fix is
  `[selected]` per option. Sites found by the finding-#6 sweep (regex:
  `<select` … `[value]` with `@for` in the block): `characters/edit/
  details-tab.ts:163`, `characters/new/new-character.ts:271`,
  `characters/view/tabs/details-tab.ts:160`, `settings/providers/
  api-key-modal.ts:53`, `settings/providers/cheap-llm-card.ts:94,121`,
  `settings/providers/profile-modal.ts:114,214,475`,
  `settings/wizard/steps/model-selection-step.ts:141`. Many render only
  after their data resolves (modals) so they may be safe TODAY — the audit
  is to prove it per site or convert to `[selected]`; the static-options
  selects are fine as-is. Small enough to ride any SPA order.
