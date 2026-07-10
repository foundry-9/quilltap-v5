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
| 3 | A large Salon chat renders for 10+ s and lands stuck at the top (console: `'setTimeout' handler took 10196ms`, no errors) | No virtualization — the conversation renders EVERY message through the full markdown pipeline synchronously; scroll-to-bottom fires before the layout settles | **OPEN — promoted**: virtualization (already a named full-Salon deferral) moves to the TOP of the next Salon order; interim option (declined for now): a newest-~150 window with load-earlier |

## Standing notes for the next orders

- Finding #3 makes **virtualization + post-render scroll-to-bottom** the first
  deliverable of the next Salon slice — it blocks dogfooding long-running
  chats outright.
- If findings of class #1/#2 keep appearing, the systematic close-out is a
  **migration-vintage fixture**: a test DB built by replaying v4's actual
  migration chain (instead of fresh `generateDDL`) so the differential harness
  can exercise real-instance shapes. Write it as its own small order if a
  third schema-divergence finding lands.
- `db::tolerant_select_list` is the reusable fix for any further
  `no such column` hits — apply it to the failing reader + add the
  migration-vintage regression test (the `chat_settings` pattern).
