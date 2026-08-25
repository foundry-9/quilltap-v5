---
description: Check v4 drift against the oracle baseline, delineate where it intersects already-ported work, and record the result in the drift ledger the other porting commands read
---

# Check and record v4 drift

Measure where v4 stands relative to the oracle baseline and write the result
into **`docs/developer/porting/drift-ledger.md`** — the single standard
record. The other porting commands (`/setupphase`, `/carryout`, `/unify`,
`/dogfood`) do not check drift themselves; they run the ledger's §2
freshness probe and read what this command recorded. The output of this
command is **an updated, committed ledger plus a report** — not ports, not
work orders.

Run this from a main-checkout session only (never from inside a lane —
parallel lanes must not race on the ledger).

## 1. Follow the ledger's own procedure

The full method lives in the ledger, **§4 "How a full drift check runs"**,
with its traps and recipes in §5 — follow it there rather than from this
file, so there is exactly one authoritative copy. In brief, it will have
you:

1. Read §1 for the baseline and prior state (and cross-check CLAUDE.md's
   Status — a disagreement between them is itself a finding).
2. Check **both** v4 branches (`main` by commit list, `bugfix` by CONTENT
   diff — the squash topology makes its commit list a lie), and record the
   checkout's branch + dirty state.
3. Classify every new commit from its **shipped hunks, never its message**
   (§4 step 4 / §5.3), including the CONVERGENCE check against v4's
   `docs/developer/bugs.md` (a bug this port filed coming back fixed means
   pins will trip by design — §5.4).
4. Delineate each commit's intersection with already-ported work by naming
   the v5 surface and the round/lane that ported it (grep `status-log.md`
   and CLAUDE.md's round bullets for the v4 path / feature / bug number).

## 2. Update the ledger

- Rewrite **§1 Current state** wholesale (it describes now, not history):
  baseline, check date, both branch tips, checkout posture, the verdict
  (`CLEAR` or `DRIFT PENDING — N commits`), and the **regen rule in force**
  (pin required whenever HEAD is past the baseline or the checkout is
  dirty/wrong-branch).
- Append new rows to **§3** as `UNPROCESSED`. Never touch existing
  `ORDERED`/`ABSORBED` dispositions except to append newer facts to a row
  (e.g. a later commit superseding part of an ordered one — note it, don't
  rewrite history).
- Do NOT move rows to §6 — only `/unify` does that, when the baseline moves.

## 3. Commit and report

Commit the ledger per `.claude/commands/commit.md` (docs-only: CHANGELOG
entry, no version bumps) and push, unless the working tree carries someone
else's in-flight changes — then leave it staged-ready and say so.

Report to the human, leading with the verdict:

- **CLEAR** (HEAD at baseline, clean checkout) or **DRIFT PENDING** with the
  commit count and the one-line-per-commit table.
- Which already-ported surfaces are hit, and which rows look like
  convergences vs new ports vs NO-PORT candidates.
- Whether the regen rule changed (pins now required / no longer required).
- The recommended next move (usually: `/setupphase` for a catch-up round
  when PORT/PORT-NEW rows exist; nothing, when CLEAR).
