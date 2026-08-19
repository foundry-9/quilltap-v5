---
description: Run a hands-on dogfood pass over freshly unified work — research the walk, pause for the human's build + data refresh, then drive the browser yourself, deferring only the genuinely human steps
argument-hint: <work order path(s) that just landed, or "the standing 💸 queue">
---

# Dogfood a freshly landed round

Dogfood the surfaces delivered by: **$ARGUMENTS**

Dogfooding is browsing a **COPY of real Friday data** through the v5 server +
SPA. It catches what synthetic fixtures structurally cannot: migration-vintage
schema shapes, real data volumes, and gestures the e2e never makes. Findings
log: `docs/developer/porting/dogfood-findings.md` (numbered, classed,
fixed-in-place or promoted into an order).

**The division of labor: Claude drives the walk.** The human builds, refreshes
the data copy, and unlocks the instance; Claude then executes every test it
can through the Browser pane, and hands the human only the short remainder
that genuinely needs them. This is a disposable test instance — it is never
rsynced back to production — so be bold: create test characters, spin up new
chats, reuse existing ones, wear things, delete things. Nothing here can break
anything that can't be restored by the next rsync.

## 1. Research, then write the walk plan as a living document

Read each order's status header, the status-log round records, and the
standing 💸 live-proof queue (CLAUDE.md status bullets) to derive what is
newly LIVE vs still refusal-armed. Then write the walk plan to
**`docs/developer/porting/dogfood-walks/YYYY-MM-DD-<slug>.md`** — this file is
updated in place as tests run, so structure every step as a checklist row:

- **Owner: `CLAUDE` or `HUMAN`.** Default to `CLAUDE`. Mark `HUMAN` only for:
  steps that are expensive by nature (long autonomous runs, image-generation
  batches, anything burning real money beyond a few LLM calls); steps needing
  human aesthetic/acceptance judgment ("does this look right"); and steps
  bound to credentials or actions Claude must not perform (typing the
  passphrase, entering API keys). A few live LLM calls that can be verified
  from logs, the wire, or the screen are `CLAUDE` work.
- **Status:** `PENDING` → `PASS` / `FAIL(#finding)` / `DEFERRED-TO-HUMAN` /
  `BLOCKED(reason)`.
- **The gesture**, weighted toward what the e2e does NOT cover: the broadest
  gesture for each action (click the card body, not the link; scroll with the
  wheel; resize; keyboard paths — findings #3a/#4 came from broad paths broken
  while narrow ones worked); real-data stress (the largest chat, characters
  with many tags/wardrobe/photos, legacy/broken-vault rows); every surface the
  round newly un-refused.
- **The expected result AND how to verify it** — the exact log query
  (`quilltap db logs --tail N`, a server-log grep, the network tab), not just
  "it works". `llm_logs` lives in the llm-logs partition (`quilltap db
  --llm-logs`, call-type column is `type`); `memories` is on main.
- **What NOT to expect to work**: the still-refusal-armed actions, listed
  from the orders, so nobody reports them as bugs.

## 2. Pause for the build + data refresh (the human's turn)

Print these instructions verbatim (adjusted only if the round changed them),
then **STOP and wait for the human to confirm** both are done:

```bash
cargo build --release
```

```bash
cd apps/web && npx ng build
```

Data refresh — a fresh copy of Friday into the standing dogfood instance
(`~/qt-dogfood-friday` — the instance dir itself: it holds `data/`, `files/`,
`logs/`):

```bash
rsync -a ~/iCloud/Quilltap/Friday/data/*.db ~/qt-dogfood-friday/data/ && rsync -a ~/iCloud/Quilltap/Friday/data/quilltap.dbkey ~/qt-dogfood-friday/data/ && rm -f ~/qt-dogfood-friday/data/*.db-journal ~/qt-dogfood-friday/data/quilltap.lock
```

Remind them: **never reverse the rsync direction** (the copy IS the safety
layer; nothing writable ever points at live `~/iCloud/Quilltap/Friday`), and
if the copy opens with 0 tables, iCloud evicted the source —
`brctl download ~/iCloud/Quilltap/Friday/data` first, then re-rsync.

## 3. Launch the server and get unlocked

Once the human confirms, Claude launches (backend serves the SPA — one
process):

```bash
RUST_BACKTRACE=1 ./target/release/quilltap-web --data-dir ~/qt-dogfood-friday --spa-dir apps/web/dist/quilltap/browser
```

- Run it via `run_in_background` with output captured to a log file in the
  scratchpad — that log is a first-class diagnostic during the walk.
  **`RUST_BACKTRACE=1` is standing policy**: the panics that matter are
  data-dependent and don't reproduce on retry; the first `quilltap_core::`
  frame is the diagnosis.
- Default bind is `127.0.0.1:3000`. Open the Browser pane there
  (`preview_start` with the URL).
- **The unlock is the human's step.** Claude never types the passphrase. Ask
  the human to enter it — in the Browser pane directly, or in their own tab
  against the same server (the unlock is server-side, so any session's unlock
  unlocks the engine). Confirm the unlocked state before proceeding.

## 4. Drive the walk (the big change: Claude does the testing)

Execute every `CLAUDE` step yourself through the Browser pane, in walk order,
updating the walk doc's status column **after each step or coherent suite —
never batch the recording to the end**.

- **Verify, don't vibe.** Each step's PASS cites its evidence: the on-screen
  state (screenshot/read_page), the log row, the network response, or the DB
  row (`quilltap db` against the copy — read-only queries freely; writes only
  the deliberate test kind, this copy is disposable).
- **Improvise test material freely.** Take existing characters/chats when a
  good candidate exists; otherwise create throwaway ones and play the
  scenario out. When picking a good candidate matters (e.g. "a
  pre-hair-vintage wardrobe row", "the largest chat"), find it by query
  first, and ask the human only if the data can't tell you.
- **LLM spend judgment:** a few completions on the configured models to prove
  a wire or a flow is fine — watch them in `llm_logs` or the wire. Long
  loops, batch jobs, image batches, or anything that would rack up real cost
  → mark the step `DEFERRED-TO-HUMAN` with a one-line reason instead.
- **After an SPA fix mid-walk:** rebuild (`npx ng build`), restart the
  server, and **hard-reload the tab (`Cmd+Shift+R`)** — a server restart
  alone does nothing; the tab keeps its loaded chunks. If a fix "isn't
  taking", rule the tab out FIRST: curl a chunk from the running server, grep
  it for a string only the fix introduces, `cmp` against
  `apps/web/dist/quilltap/browser/`.

### For each failure found (yours or the human's)

1. **Reproduce and localize** before changing anything (console/server logs,
   the network tab, a curl against the dispatch API, the failing component's
   source).
2. **Diagnose against v4 as the oracle.** Survey the corresponding v4
   component and classify:
   - **Port divergence** — v4 behaves correctly, v5 dropped or narrowed
     something. Fix v5 to be v4-faithful.
   - **Migration-vintage schema divergence** — fresh-DDL fixtures vs a real
     migrated instance (the class of findings #1/#2). Reach for
     `db::tolerant_select_list` for missing columns; add a regression test
     over both table shapes.
   - **Faithfully ported v4 bug** — do NOT silently fix; flag to the human
     and record the decision (candidate for the v4 bugs doc).
3. **Fix in place**, with the full loop: the v4-faithful code fix; a unit
   test proving the fixed behavior AND the guard; extend the e2e to use the
   **gesture that was actually broken**, not the one that already worked; a
   numbered row in `dogfood-findings.md` (finding, class, status, commit).
4. **Commit per `.claude/commands/commit.md`** (changelog, version bumps, the
   relevant gate: SPA `ng test` + `ng build` + Playwright for SPA fixes; the
   workspace + differential gate for Rust fixes) and push to main — dogfood
   fixes are small and land directly, no lane ceremony.
5. Rebuild/restart/hard-reload as above, re-run the failed step to PASS, and
   continue the walk where it left off.

## 5. The human remainder

When every `CLAUDE` step is PASS, FAIL-with-finding, or honestly BLOCKED,
present the human with the (much shorter) `HUMAN`/`DEFERRED-TO-HUMAN` list —
each item with its setup already done where possible, the exact gesture, the
expected result, and how you'll verify it from your side (logs/DB) as they
go. Walk them through interactively, recording results in the same walk doc.

## 6. Close out: record, report, next steps

- Finalize the walk doc (every row a terminal status) and commit it with the
  pass's fixes.
- Findings rows in `dogfood-findings.md`; the walk's record in
  `status-log.md` (the established "Dogfood pass — …" form); update the
  standing 💸 queue in CLAUDE.md's status bullet if this pass discharged
  items from it.
- A finding that can't be fixed in place lands somewhere durable the next
  `/setupphase` reads — the "Standing notes for the next orders" section of
  `dogfood-findings.md`, the owning order's OPEN list, or a proposed new
  order. A finding that reveals a landed order was incomplete → correct that
  order's status header now.
- New reusable gotchas → the round's memory note + MEMORY.md.
- Report to the human: what passed, what was found and fixed, what's filed
  upstream, what remains deferred, and the concrete next steps.

## Standing browser-walk gotchas (check before blaming the app)

Glyph-content buttons need `getByTitle` semantics (content outranks `title`
in accname); wait ~450 ms for the auto-scroll strategies to drain before
scrolling away; scroll with real wheel events, never bare `scrollTop`; the
"Edit Character" link lives on the detail view's Details tab; store-overlay
properties (projects/groups columns) cannot be SQL-seeded — seed through the
API/UI or the change is silently invisible.
