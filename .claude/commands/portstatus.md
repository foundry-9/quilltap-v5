---
description: Report where the v4→v5 port stands — phase-by-phase progress breakdown, the next steps, a start-to-finish completion estimate, and how far behind dogfood testing is. Use whenever the user asks how far along the port is, what percent is done, what's left, what to do next, where we are in the process, or how much dogfooding is owed.
argument-hint: [optional focus: progress | next | estimate | dogfood]
---

# Port progress report

Produce a read-only status report on the v4→v5 port. This command changes
nothing — no fixes, no planning, no work orders. Its output is a report the
human reads to decide what to run next (`/setupphase`, `/dogfood`, or a
specific order). If `$ARGUMENTS` names a focus, still gather everything
(the sections feed each other) but expand that section and compress the rest.

## 1. Gather

Read in this order. The order matters because each source corrects the one
before it: CLAUDE.md is the phase-level summary, the M6 checklist is the
distance-to-done yardstick, and the dogfood log is the reality check on what
"landed" actually means.

1. **CLAUDE.md, the Status section** (already in context every turn) — the
   round-by-round bullets, the oracle-baseline paragraph, and the version
   numbers. This is the spine of the whole report.
2. **`docs/developer/porting/m6-screen-parity.md`**, sections 4 and 5 — the
   16-row prioritized backlog (§4) and the v4 retirement criteria (§5,
   especially the §5.3 non-screen pools and the §5.4 short answer). This is
   the only document that frames "done" as a finish line rather than a next
   step, so the completion estimate leans on it hardest.
3. **`docs/developer/porting/dogfood-findings.md`** — whole file; it is
   short. The findings table plus the "Standing notes for the next orders"
   section, whose most recent "coverage so far" bullet enumerates WALKED vs
   NOT-yet-walked surfaces. This is the primary input for the dogfood-lag
   section.
4. **`docs/developer/porting/phase-4.md`** — the "Decomposition and
   ordering" section for the milestone table, and the most recent round
   sections at the tail for the current "next candidates" list. Skip the
   middle; it is 2,400 lines and the locked decisions don't change.
5. **Open work orders** —
   `grep -l "STATUS.*OPEN" docs/developer/porting/work-orders/*.md`
   (headers are `**STATUS: ...**` near the top; read the matching headers,
   not the whole orders). An order with an OPEN remainder is committed
   next-step scope, senior to any "next candidates" list.
6. **`docs/developer/porting/status-log.md`** — 29k+ lines; never read it
   whole. `grep -n '^## Round record\|lane record'` and read only the last
   one or two records, for anything the CLAUDE.md bullet compressed away
   (deferral lists, "live proof owed" notes).
7. **v4 drift check** (freshness, cheap): in `~/source/quilltap-server`,
   `git log <oracle-baseline>..HEAD --oneline` with the baseline SHA from
   CLAUDE.md's oracle-baseline paragraph. Un-absorbed drift is unfinished
   work and belongs in both the next-steps and estimate sections. If the
   checkout is unreachable, say so and report the drift debt CLAUDE.md
   already records.

## 2. Compute

### Progress breakdown

Summarize phases 0–3 in one line each (they are done; their value is
context, not detail). Spend the space on Phase 4: group the landed rounds
by vertical (transports/hosts, Salon, Characters, Scriptorium, Workbench,
workspace tabs, episodic campaign, …) rather than re-listing every round
bullet — the human has read those bullets before; what they want is the
shape. Distinguish three states honestly, because they get conflated:

- **LIVE** — ported, differential-verified, wired in the production host.
- **Refusal-armed / deferred loud** — reachable by a user but deliberately
  refusing; enumerate the big ones (they are product gaps, not bugs).
- **Ported but dormant** — landed and verified but awaiting a wire or a
  human walk ("live proof owed").

### Next steps

Rank, in this order of authority:

1. Open work orders and OPEN-remainder resume points (committed scope).
2. Owed drift catch-ups (the oracle-baseline paragraph and dogfood standing
   notes name them explicitly — e.g. a workspace corpus recapture).
3. The "next candidates" list from the most recent round record.
4. The unstarted M6 backlog rows, by their §4 priority order.
5. The owed dogfood pass (see below) — call it out as a next step in its
   own right when the lag is more than a round or two, because findings
   feed `/setupphase` and a long lag compounds silently.

### Completion estimate

Never emit a bare percentage — show the arithmetic, and give a range. The
honest yardsticks, from strongest to weakest:

- **The M6 backlog (§4):** count DONE vs unstarted rows, weighted by their
  size column (rider < lane < round; row 16's "largest" note calibrates the
  top). This measures screen parity — the retirement gate.
- **The §5.3 non-screen pools:** release/signing (D21), Windows/Linux
  re-checks, uniffi/mobile, the refusing service seams. These gate
  *retirement*, not *usability* — state which milestone your percentage is
  measuring, because "usable daily driver" and "v4 retired" are different
  finish lines, plausibly far apart.
- **The drift treadmill:** v4 is under active development, so the finish
  line moves. Report current drift debt and note the estimate assumes v4
  drift keeps being absorbed at the current cadence.

A defensible form: "X of 16 M6 rows done (weighted ~Y%); phases 0–3 and the
Phase-4 infrastructure represent the majority of total effort and are
complete; call it N–M% toward daily-driver, minus the §5.3 pools toward
full v4 retirement." Resist false precision — the spread between N and M
should widen when open orders or drift debt are large.

### Dogfood lag

Dogfooding (a human browsing a COPY of real Friday data) is the only check
that catches migration-vintage schemas, real data volumes, and gestures the
e2e never makes — so "landed but never dogfooded" is a distinct risk state,
not a detail. Compute the lag concretely:

1. **Anchor:** the date and coverage of the most recent pass, from the
   newest "coverage so far" bullet in dogfood-findings.md's standing notes
   (it lists WALKED and NOT-yet-walked explicitly).
2. **Accumulate:** every round unified on main *after* that anchor date
   (CLAUDE.md bullets) whose surfaces don't appear in any walked list —
   plus anything a round record marks "live proof owed" or
   "ACTIVATE-AT-UNIFY … awaiting walk".
3. **Flag real-spend items** (the notes mark them 💸 — consult entrances,
   imageProfileGenerate, recall-replay, …): these need the human to opt in,
   so they linger; list them separately with an "ask first" note.
4. **Express the lag three ways:** days since the last pass, rounds landed
   since whose surfaces are unwalked, and the enumerated walk backlog
   (carry forward NOT-yet-walked leftovers from *earlier* passes too —
   they compound across standing-notes bullets and are easy to lose).

End the section with the concrete recommendation: which orders a
`/dogfood <orders>` invocation should cover first, weighted toward surfaces
that write real data or spend real money.

## 3. Report shape

Lead with a 3–4 sentence TL;DR: where the port is in one phrase, the
completion range, the single most important next step, and the dogfood lag
in one number. Then the four sections above, in that order. Tables for the
phase breakdown and the walk backlog; prose for the estimate reasoning.
Keep it under a screen and a half — the detail lives in the documents, and
the report should cite them (file + section) rather than inline them.

## 4. Cautions

- A red `provisioning_equivalence` or stale differential after v4 moves is
  the drift tripwire firing as designed — report it as drift debt, never as
  a v5 bug (CLAUDE.md, "The differential port discipline").
- Round bullets in CLAUDE.md are written at unification and go stale as
  later rounds close their deferrals — the newest bullet and the status-log
  tail win any disagreement with an older bullet.
- "Deferred loud" lists shrink release-over-release; verify a deferral
  still stands (grep the newest round record) before reporting it open.
- Don't start fixing or planning anything you find — hand the human the
  report and the recommended command to run next.
