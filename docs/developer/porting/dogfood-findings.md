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
