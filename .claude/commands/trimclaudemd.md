---
description: Trim CLAUDE.md back to a reasonable per-turn size by archiving its older Status round bullets VERBATIM (diff-verified) into claude-md-status-history.md, leaving one compressed arc bullet in their place — the same move the 2026-07-10 and 2026-08-13 splits made
---

# Trim CLAUDE.md — archive the older round bullets

CLAUDE.md is loaded into every turn of every session and every lane agent,
against its own "stays short" rule. Its **Status** section accretes one long
bullet per round and per dogfood pass, so it grows back to hundreds of KB.
This command repeats what commits `d04b6dc5` (2026-07-10, the journal →
`status-log.md`) and `595fb678` (2026-08-13, the round bullets →
`claude-md-status-history.md`) did, as a repeatable procedure.

**What it is:** a VERBATIM move of the oldest round bullets out of CLAUDE.md
into `docs/developer/porting/claude-md-status-history.md`, plus ONE new
compressed arc bullet in CLAUDE.md standing in for the moved span.
**What it is not:** an edit of any round's record. The record of authority for
every round is `docs/developer/porting/status-log.md`; the history file only
preserves the exact phase-level text CLAUDE.md used to carry. Nothing is
summarized away — the summary is *additional* to the verbatim archive.

Run from a main-checkout session with a clean tree (never inside a lane —
CLAUDE.md is shared by every lane and a moving Status section fights every
in-flight `/unify`). If lanes are open, ask before proceeding.

Optional argument: `$ARGUMENTS`
- empty → keep the most recent **8** round bullets (roughly the last two weeks
  of rounds and dogfood passes) plus everything structural (below);
- an integer `N` → keep the most recent N round bullets;
- `through YYYY-MM-DD` → archive every round bullet whose unification /
  pass date is on or before that date.

## 1. Measure, then decide the cut

1. `wc -lc CLAUDE.md` and `wc -l docs/developer/porting/claude-md-status-history.md`.
   Record both — the report needs before/after. If CLAUDE.md is already
   under ~60 KB, stop and say so; there is nothing to trim.
2. Map the Status section: `grep -n '^## \|^- \*\*' CLAUDE.md`. Everything
   before `## Status` is standing rules and stays untouched. Inside Status,
   the bullets fall into four kinds:
   - **Structural — never archived:** the `Phase 0`…`Phase 4` bullets, every
     existing `**Rounds … — ARCHIVED.**` arc bullet, the current
     `**Oracle baseline: …**` paragraph, and the closing
     `**Standing deferrals + gotchas:**` bullet.
   - **Round bullets** (`**The … round … UNIFIED on main (DATE) …**`) and
     **dogfood-pass bullets** (`**The … dogfood pass RAN (DATE, …**`) and
     their same-day riders (a follow-up that "found a SECOND finding", a
     solo stacked lane such as `**P4.50 — …**`). These are what gets
     archived, always as a contiguous chronological span starting right
     after the last existing ARCHIVED arc bullet.
   - **Superseded baseline paragraphs**, if any linger — `/unify` archives
     these at each baseline move under their own `## Superseded baseline
     paragraph …` headers in the history file; if one is still in
     CLAUDE.md, archive it the same way (its own header, verbatim).
3. Choose the span: from the first non-structural bullet after the last
   ARCHIVED bullet, through the bullet that leaves N (default 8) most
   recent round/pass bullets in place. Never split a round from its
   same-day rider or a round from the dogfood pass that walked it if that
   would leave the kept set starting mid-story — move the cut one bullet
   earlier instead. Record the span's first/last line numbers, its first
   and last round names, and its date range.

## 2. Extract the span verbatim

Extract by line number into the scratchpad, never by pattern-editing:

```bash
gsed -n '<FIRST>,<LAST>p' CLAUDE.md > "$SCRATCH/archived-span.md"
```

Sanity-check the file starts with `- **The` and ends at the end of a bullet
(the next line in CLAUDE.md is `- **`). `md5 "$SCRATCH/archived-span.md"` —
keep the hash; it is the proof at step 5.

## 3. Append to the history file

In `docs/developer/porting/claude-md-status-history.md`:

1. Add a new numbered section **after the last `## <N>. Archived round
   bullets …` section and before `## 2. Superseded oracle-baseline
   paragraphs`** (renumber nothing — use the next free integer, and if the
   file's numbering has drifted, add the section at the end of the
   round-bullet sections with a header of the form
   `## <N>. Archived round bullets (<START> → <END>)`), with a two-line
   preface naming the archive date, the span (first round → last round),
   and that the text is verbatim — then the span file's contents, untouched.
2. Update the header's "Contents:" list with one line for the new section,
   and its "moved VERBATIM … on <date>" sentence so it reads as a list of
   archive dates rather than a single one.

Use `cat >>` / a Python script that reads the whole file into a variable
FIRST and writes once — never `open(p,'w')` before reading `p` (that
truncates the file), and never BSD `sed -i` (use `gsed`).

## 4. Replace the span in CLAUDE.md with ONE arc bullet

Delete lines FIRST..LAST and put a single bullet in their place, in the
existing arc bullet's shape:

```
- **Rounds <START-DATE> → <END-DATE> — ARCHIVED.** The verbatim round bullets
  for that span (the <first round name> through the <last round name>) now
  live in `docs/developer/porting/claude-md-status-history.md` §<N>; the
  full round records were always in `status-log.md`. The arc, compressed:
  <8–20 lines>. Deferred items from that era are tracked in the work orders,
  the drift ledger, and later bullets, not here.
```

Writing the arc is the one act of judgment in this command. Read every
archived bullet's opening sentence and its bolded catches, then compress:
what landed (features, verticals, v4 bugs absorbed by number), what the
port itself found (v4 filings by number, the dogfood findings fixed), where
the baseline moved (first pin → last pin), and any ruling made. Keep every
proper name that a later grep might look for (`P4.Dnn` order ids, bug
numbers, finding numbers, the v4 pins). Do not carry gate numbers,
versions, or 💸 queue items — those live in the archived text and the
status log. Keep the spelling rule: the project name is Quilltap.

Anything in the archived span that is still **live** — an OPEN order, an
owed 💸 item, a standing "PIN REQUIRED", a rule the human ratified — must
already appear in a kept bullet, the drift ledger, `phase-4.md`, or a work
order. Grep for it before you archive; if it lives only in the span, say so
in the report and carry a one-line pointer into the arc bullet rather than
losing it.

## 5. Verify — the move is byte-exact or it is not done

```bash
# the lines removed from CLAUDE.md, minus the ONE arc bullet added, are exactly the span file
# (bullet lines start with "- ", so a removed one reads "-- **…" — filter on the file header, not on "^-[^-]")
git diff -U0 -- CLAUDE.md | grep '^-' | grep -v '^--- a/' | cut -c2- > "$SCRATCH/removed.md"
diff "$SCRATCH/removed.md" "$SCRATCH/archived-span.md" && echo REMOVED-OK
git diff -U0 -- CLAUDE.md | grep '^+' | grep -v '^+++ b/' | cut -c2-        # prints ONLY the new arc bullet
# the lines added to the history file are the span file plus ONLY the new header/preface/contents lines
git diff -U0 -- docs/developer/porting/claude-md-status-history.md | grep '^+' | grep -v '^+++ b/' | cut -c2- > "$SCRATCH/added.md"
grep -vxF -f "$SCRATCH/archived-span.md" "$SCRATCH/added.md"               # inspect: header/preface/contents lines and nothing else
md5 "$SCRATCH/archived-span.md"   # unchanged from step 2
```

Then:

- `git diff --stat` shows exactly two files (CLAUDE.md shrank by ~the span;
  the history file grew by the span + header lines) — plus the CHANGELOG
  once step 6 runs.
- `python3 harness/tools/check_spelling.py` is clean (the history file is in
  its scan; a misspelling never enters the repo, even quoted).
- `wc -lc CLAUDE.md` again. If it is still over ~60 KB, say so and either
  widen the span (repeat from step 1 with a smaller N) or stop and report
  why not (e.g. the kept bullets are themselves oversized — that is a
  finding for the human, not something this command edits).
- The kept Status section still reads in order: Phase bullets → ARCHIVED
  arc bullet(s) → kept rounds → the Oracle baseline paragraph → Standing
  deferrals. Nothing outside Status changed.

## 6. Commit and report

Commit per `.claude/commands/commit.md` — docs-only: a CHANGELOG entry
(`_Docs-only change._`), no version bumps; the subject in the shape of the
precedent, e.g. `docs: trim CLAUDE.md — archive the <START>→<END> round
bullets to claude-md-status-history.md §<N> (verbatim, diff-verified)`.

Report to the human:

- Before/after: CLAUDE.md lines + bytes; how many bullets moved and the span
  (first round → last round, dates).
- The verification results (REMOVED-OK; the md5; the spelling guard).
- Anything live that had lived only in the span and where it now points.
- Whether CLAUDE.md is under the target, and if not, why.
