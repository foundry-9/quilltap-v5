---
description: Produce a hands-on dogfood test script for freshly unified work, then diagnose and fix what the human finds
argument-hint: <work order path(s) that just landed>
---

# Dogfood a freshly landed round

Dogfood the surfaces delivered by: **$ARGUMENTS**

Dogfooding is the human browsing a **COPY of real Friday data** through the
v5 server + SPA. It catches what synthetic fixtures structurally cannot:
migration-vintage schema shapes, real data volumes, and gestures the e2e
never makes. Findings log:
`docs/developer/porting/dogfood-findings.md` (numbered, classed,
fixed-in-place or promoted into an order).

## 1. Tell the human exactly what to test

Read each order's status header and status-log records to derive what is
newly LIVE vs still refusal-armed. Then produce a concrete test script:

- **Setup reminder**: run against a fresh COPY of Friday
  (`~/iCloud/Quilltap/Friday` — never the live directory; `brctl download`
  first if iCloud has evicted contents; never a writable open on the
  original). Include the launch command for the copy.
- **A numbered walk** of exact clicks/gestures, weighted toward what the
  e2e does NOT cover:
  - the **broadest gesture** for each action (click the card body, not the
    link; scroll with the wheel; resize; keyboard paths) — two findings
    (#3a, #4) came from surfaces whose narrow path worked while the broad
    one was broken;
  - real-data stress: the largest chat, characters with many
    tags/wardrobe/photos, legacy/broken-vault rows;
  - every surface the round newly un-refused;
- **What NOT to expect to work**: the still-refusal-armed actions, listed
  from the orders, so the human doesn't report them as bugs.

Then stop and wait for findings — do not fix hypothetical problems.

## 2. For each finding the human reports

1. **Reproduce and localize** before changing anything (console/server
   logs, the network tab, a curl against the dispatch API, the failing
   component's source).
2. **Diagnose against v4 as the oracle.** Survey the corresponding v4
   component and classify:
   - **Port divergence** — v4 behaves correctly, v5 dropped or narrowed
     something. Fix v5 to be v4-faithful.
   - **Migration-vintage schema divergence** — fresh-DDL fixtures vs a real
     migrated instance (the class of findings #1/#2). Reach for
     `db::tolerant_select_list` for missing columns; add a regression test
     over both table shapes. If this is the THIRD such finding, propose the
     migration-vintage fixture order.
   - **Faithfully ported v4 bug** — do NOT silently fix; flag to the human
     and record the decision.
3. **Fix in place**, with the full loop:
   - the v4-faithful code fix;
   - a unit test proving the fixed behavior AND the guard (e.g. navigate
     from the body, don't navigate from an inner button);
   - extend the e2e to use the **gesture that was actually broken**, not
     the one that already worked;
   - a numbered row in `dogfood-findings.md` (finding, class, status,
     commit).
4. **Commit per `.claude/commands/commit.md`** (changelog, version bumps,
   the relevant gate: SPA `ng test` + `ng build` + Playwright for SPA
   fixes; the workspace + differential gate for Rust fixes) and push to
   main — dogfood fixes are small and land directly, no lane ceremony.
5. Tell the human it's fixed, what the cause was, and to rebuild/restart
   their dogfood server — then continue the walk where they left off.

## Standing e2e gotchas (check before blaming the app)

Glyph-content buttons need `getByTitle` (content outranks `title` in
accname); wait ~450 ms for the auto-scroll strategies to drain before
scrolling away; use real `page.mouse.wheel`, never bare `scrollTop`; the
"Edit Character" link lives on the detail view's Details tab; specs that
mutate get their own server on a spec-private port (4321/4322 taken) with
`quilltap db --write` rewrites BEFORE launch.
