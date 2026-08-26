# The v4 drift ledger

**The single standard record of where v4 stands relative to the oracle
baseline, what each drift commit touches, where it intersects work this port
has already landed, and what has been done about it.** Written by
`/driftcheck` (and by `/unify` when a round moves the baseline); read by
`/setupphase`, `/carryout`, `/unify`, and `/dogfood`. Those commands do NOT
re-derive drift — they run the §2 freshness probe and then trust this file.

Ownership: this file is written only from a main-checkout session
(`/driftcheck`, `/setupphase` marking rows ORDERED, `/unify` marking rows
ABSORBED, `/dogfood` when a walk changes a row's facts). **Lane agents never
write it** — a lane that finds the probe failing STOPs and reports instead.

---

## §1 Current state

_Updated only by `/driftcheck` and `/unify`. Every field here is what the §2
probe verifies against._

- **Oracle baseline:** `8f9101370` — "fix(ci): the zsh completion check no
  longer fails where zsh isn't installed" (v4 main, 2026-08-25), adopted at
  the `8f910137`-round unification (2026-08-25, the P4.D115–P4.D118 drift
  catch-up).
- **Checked:** 2026-08-26 (`/driftcheck`, main-checkout session).
- **v4 `main` HEAD at check:** `b220999da` — "feat(search): a Documents chip
  that searches every document store" (2026-08-25). **Five commits past the
  baseline.**
- **v4 `bugfix` tip at check:** `3a76b17df` — "bugfix: started 4.8.4 bug
  branch" (**unmoved** since the last check; the fork marker). No unabsorbed
  bugfix-side content: the tip is byte-identical to the previously measured
  state, and `git log main..bugfix --oneline -- lib/ app/ packages/` still
  lists only pre-4.8.4 history already squashed onto main.
- **Checkout at check:** branch `main`, tree **clean**.
- **Verdict: DRIFT PENDING — 5 commits** (2 feature commits + 1 combined
  feature commit + 2 docs-only specs; see §3). Three v4 features in one day,
  **all three landing on surfaces this port finished in the last two
  rounds** — the wardrobe tiers (P4.D39/P4.D112/P4.D113), the scenario
  feature (P4.D115/P4.D116, unified the previous day), and the `uiSearch`
  verb + search dialog (P4.9P). **No SQL DDL moved** — no D23 re-dump is
  implied (the one `schema.ts` hunk is the vault-overlay's TS constant
  re-export, not `generateDDL`).
- **Regen rule in force: PIN REQUIRED.** v4 `main` HEAD is past the
  baseline, so every oracle regeneration must run from a lane-unique
  detached worktree pinned at `8f9101370` (recipe in §5.1;
  `recipe_sweep.py --v4 "$PIN"`). Regenerating from the checkout would pull
  in the wardrobe-instructions / archived-entries / documents-search
  behavior and silently redden or (worse) greenwash unrelated families.

## §2 The freshness probe

What every consuming command runs before trusting §1 — four read-only
commands, no classification, no judgment:

```bash
git -C ~/source/quilltap-server branch --show-current
git -C ~/source/quilltap-server status --short
git -C ~/source/quilltap-server log --oneline <§1 main HEAD>..main
git -C ~/source/quilltap-server log --oneline <§1 bugfix tip>..bugfix
```

Pass = the branch and tree state match §1 and both logs are **empty**. Then
§1's verdict and regen rule stand; proceed on them. Any mismatch = the ledger
is stale: `/setupphase`, `/unify`, and `/dogfood` run `/driftcheck` (or its
§4 procedure) before continuing; a `/carryout` lane **STOPs and reports**.

A dirty tree matters even when HEAD hasn't moved: uncommitted `lib/`, `app/`,
`packages/`, or `plugins/` edits poison any regen run from the checkout
(§5.1's mid-lane note). Infra-only dirt (CI files, docs) still gets recorded
here so the next probe doesn't re-alarm on it.

## §3 The drift table

Classes: **PORT** (behavior change on a ported surface), **PORT-NEW** (new v4
feature to port), **CONVERGENCE** (v4 adopting a fix this port made first —
both-directions pins will trip at the baseline move *by design*; retire them
by measurement, §5.4), **NO-PORT?** (docs/infra/tests-only candidate —
needs ratification with evidence, never by subject line alone).

Dispositions: **UNPROCESSED** → **ORDERED(order-id)** (set by `/setupphase`)
→ **ABSORBED(round)** or **NO-PORT-RATIFIED(round)** (set by `/unify` when
the baseline moves past the commit). A row leaves this table for §6 only
when absorbed/ratified.

| sha | date | subject | class | intersects (already-ported work) | disposition |
|---|---|---|---|---|---|
| `b86bb1a58` | 2026-08-25 | feat(wardrobe): per-tier dressing instructions for characters who dress themselves | **PORT-NEW** (+ PORT) | New `lib/wardrobe/wardrobe-instructions.ts` (no v5 counterpart) feeding the outfit-selection prompt — hits the tri-tier dressing cascade (P4.D39), `outfit-selection.ts` + `apply-outfit-selections.ts` (services::outfit_selections), the vault projection sweep (**behavior change**: `projectArrayIntoVaultFolder` gains `preserveFileNames`, and the shared wardrobe reader now skips a file by name), `character-vault.ts`, the four wardrobe collection routes (`?action=instructions` GET/POST; P4.D112's `GroupWardrobe*` verbs + the wardrobe/project/character routes), the Almanack Scriptorium garment counts (P4.37/P4.D50), `field-hints.ts` (P4.D103's hints table), and the SPA wardrobe dialog + Aurora Wardrobe tab (P4.D113). Archived-character refusal rides the P4.D62–D65 tombstone. | ORDERED(p4.d119 server ∥ p4.d121 SPA) |
| `2417cbed1` | 2026-08-25 | docs(search): plan a Documents chip that searches all document stores | **NO-PORT?** | Docs-only (`docs/CHANGELOG.md` + a 402-line feature spec). Superseded in-place by `b220999da`, which moved the spec to `features/complete/`. Its "pre-existing search defects catalogued for separate filing" paragraph is worth reading when ordering the search lane. | ORDERED(p4.d122 — ratification at unify; the defects paragraph is quoted in that order's survey) |
| `a47d3e034` | 2026-08-25 | docs(wardrobe): plan archivable scenarios and wardrobe items | **NO-PORT?** | Docs-only (`docs/CHANGELOG.md` + a 126-line feature spec). Implemented by `d25dacc1d`. | ORDERED(p4.d120 — ratification at unify; the spec informed the order's split) |
| `d25dacc1d` | 2026-08-25 | feat(scenarios,wardrobe): archive entries instead of deleting them | **PORT-NEW** (+ PORT) | 84 files. An `archived: true` frontmatter key across all four scenario scopes + `archived: boolean` → `archivedAt` on the four wardrobe item routes + `?includeArchived=true` on the four collection routes. **Behavior changes on just-landed surfaces**: the scenario feature unified the day before (P4.D115/P4.D116 — `ScenarioSelect`, `ChatScenarioControl`, the `scenario_selection` resolver, default-conflict resolution, New Chat auto-select), `scenarios-common`/general/group/project (P4.6n), the character-vault scenario round-trip (**v4 bug fixed in passing**: `description` frontmatter was parsed but never written back), project/group wardrobe lists (**server-side filtering replaces the client-side pass**), the Green Room candidate pool + outfit-selection LLM (archived garments never audition, no override), `system-prompt-builder.ts`, the Almanack Scriptorium row, `qtap-export.schema.json` (export/import, P4.D46), and the New Chat group-scenarios optgroup (never connected in v4 until now). | ORDERED(p4.d120 server ∥ p4.d121 SPA) |
| `b220999da` | 2026-08-25 | feat(search): a Documents chip that searches every document store | **PORT** + **PORT-NEW** | New `lib/mount-index/document-text-search.ts` + two repo queries (`docMountFileLinks.searchByNameOrPath`, `docMountChunks.searchContent`) + a shared LIKE-escape helper, over the doc-mount repos (Phase 2 + P4.D41 link groups). **Behavior change on the ported `uiSearch` verb** (`GET /api/v1/ui/search`, P4.9P): a sixth `documents` type, and `ALL_SEARCH_TYPES` **reorders the chips/groups** to chats, characters, messages, documents, tags, memories. Also hits `qtap-uri.ts` + `uri-producers.ts` (the `docStoreAuthority` extraction) and the in-chat document-open choreography (Document Mode / P4.9L2), the archived-character vault exclusion (fails closed — P4.D62–D65), and the SPA search bar/dialog/results (P4.9P). No schema changes. | ORDERED(p4.d122) |

## §4 How a full drift check runs (the `/driftcheck` procedure)

1. **Read §1** for the baseline and the previously recorded state. Never
   take the baseline from memory — CLAUDE.md's Status and this file must
   agree; if they don't, that disagreement is itself a finding to report.
2. **Both branches, always.** Since v4's 4.8.0/4.8.1 releases (2026-08-12)
   v4 develops on TWO branches: `main` (next-dev) and `bugfix` (patch
   maintenance). Release flow: a `bugfix: started X.Y bug branch` commit
   forks the branch; fixes land there; `release: X.Y` squashes back onto
   main. The topology is squash-like — bugfix commits are NOT ancestors of
   main — so `git log <baseline>..main` misses nothing but shows the release
   squash, while `git log <baseline>..bugfix` shows the whole historical
   lineage and looks alarmingly long. **Measure bugfix by CONTENT, never the
   commit list:** `git log main..bugfix --oneline -- lib/ app/ packages/`
   for candidates, then `git diff main bugfix -- <paths>` to confirm what is
   genuinely unabsorbed.
3. **Record the checkout's posture:** `git branch --show-current` and
   `git status --short`. A checkout sitting on `bugfix`, or dirty in
   `lib/`/`app/`/`packages/`/`plugins/`, silently poisons regens (it has
   happened — see §5.1) and flips the §1 regen rule to pin-required.
4. **Classify every new commit** — `git show --stat` for the file list,
   then the hunks. ⚠ **Never classify (or later port) from the commit
   message.** A v4 commit message describes the bug's shape as its author
   understood it; the shipped hunks are often narrower. The spec is
   `git show <sha>:<path>` (the post-commit file) plus `git show <sha> --
   <path>` (the hunks). When the message asserts a behavior change, find
   the hunk that makes it; if there is no such hunk, the claim is about the
   bug, not the fix (P4.D110/bug 96 is the canonical case). Carry a note of
   what a commit did NOT do into the row when the prose misleads.
5. **Delineate the intersection with ported work.** For each commit's
   files, name the v5 surface and the round/lane that ported it — grep
   `status-log.md` and CLAUDE.md's round bullets for the v4 path, feature
   name, or bug number. Rules of thumb: `lib/**` and `app/api/**` are fully
   ported (Phases 2–4); `components/**`/`app/**` client code maps to the
   SPA verticals; `packages/quilltap/**` is the CLI Tier R;
   `help/**` banks to `p4.9i2`; `.github/`, `docker/`, release scripts,
   and tests-only changes are NO-PORT candidates. Check v4's
   `docs/developer/bugs.md` for the bug number: a bug this port filed
   coming back fixed is a **CONVERGENCE** row (pins will trip; §5.4).
6. **Update §1 and §3** — never touch existing ORDERED/ABSORBED
   dispositions except to append newer facts. Commit per
   `.claude/commands/commit.md` (docs-only) and report: the verdict, the
   new rows, which ported surfaces are affected, and whether the regen rule
   changed.

## §5 Standing drift machinery — recipes and traps

_Moved into the repo from the session-memory notes, 2026-08-25. This is the
durable home; the memory notes now just point here._

### §5.1 The pinned-worktree regen recipe

A fresh oracle is only as pinned as the tree it imports. Whenever v4 HEAD is
past the baseline OR the checkout is dirty or on the wrong branch, regen from
a detached worktree pinned at the baseline. v4 is the human's active repo —
**never stash or checkout-switch it.**

```bash
PIN=/tmp/qt-v4-pin-<order>-<sha>       # LANE-UNIQUE path — never share pins
git -C ~/source/quilltap-server worktree add --detach "$PIN" <baseline-sha>
# THREE symlink classes, all untracked and absent from a bare worktree:
ln -sfn ~/source/quilltap-server/node_modules "$PIN/node_modules"
ln -sfn ~/source/quilltap-server/packages/quilltap/node_modules \
        "$PIN/packages/quilltap/node_modules"
for d in ~/source/quilltap-server/plugins/dist/*/; do
  [ -d "$d/node_modules" ] && ln -sfn "$d/node_modules" "$PIN/plugins/dist/$(basename "$d")/node_modules"
done
cd "$PIN"   # run EVERY tsx/jest oracle + fixture builder from here
# cleanup: git -C ~/source/quilltap-server worktree remove --force "$PIN"
```

- The `@/` alias resolves through the cwd's tsconfig, so cwd = the pinned
  worktree imports pinned lib code. The second symlink feeds the real-DB
  jest cases' `requireActual` of the cipher driver; the third feeds any
  oracle that loads a provider plugin (`provider-registry.ts`, the
  stream/envelope recorders, the manifest generator) — without it they die
  on `Cannot find module '@anthropic-ai/sdk'`.
- **The empty-file trap:** that failure is loud on stderr, but if stdout was
  redirected to the oracle file, the redirect already truncated it to ZERO
  bytes — the next diff then reads "DIFFERS against an empty file" exactly
  like real drift. Check the file is non-empty before believing a diff.
- **Verify the pin** by grepping the fresh NDJSON for a marker only one tree
  has (a sentence the drift added must be ABSENT from a baseline-pinned
  oracle).
- **Lane-unique paths:** a pin shared between lanes was deleted mid-run by
  whoever finished first (P4.d26) while v4 HEAD was past the baseline — the
  later regen would silently have used the wrong tree. Cheap guard:
  `git -C ~/source/quilltap-server worktree list` before each regen batch.
- **The checkout can go dirty MID-LANE** (P4.D105): clean at lane start,
  dirty two units later. Do not reason about whether the dirty files "could"
  have mattered — build the pin, re-run every regen the lane already did
  from it, and expect the re-run to change nothing. That is a cheap, total
  proof; the alternative is an argument.
- The pin is also what makes an end-of-lane sweep honest:
  `harness/tools/recipe_sweep.py --run-all --v4 "$PIN"` rewrites every
  recipe's `cd ~/source/quilltap-server`.

### §5.2 The silent-stale-pass trap (regen verification)

A fixture builder that ERRORS leaves the old fixture in place, and the
differential then passes **green on stale data** — the green comes from the
old fixture + old oracle agreeing with each other as they always had.
Discipline for every regen:

```bash
rm -f /tmp/qt-<x>.db                      # a failed build can't hide behind a stale file
... build ... 2>&1 | tail -1              # read it; a stack trace invalidates the run
grep -c '<new-field-or-id>' /tmp/oracle-<x>.ndjson   # MUST be > 0
```

Corollaries: a green regen is not coverage — grep every regenerated NDJSON
for the CHANGED bytes; when appending to a pinned-id corpus, check the WHOLE
id set for collisions, not the tail; anchor jest `--` filters
(`"case\.test\.ts$"`) because substring matches let a sibling case clobber
the same `QT_ORACLE_OUT`.

### §5.3 Commit prose misdescribes diffs

(Also in §4 step 4, because it bites at classification AND at porting.) Port
from the shipped hunks, never the message. Re-verify even when the work
order already flagged the trap — the order's paragraph is the same kind of
prose. Carry the finding into a v5-side code comment naming the sha and what
it did NOT do.

### §5.4 Convergence rows: measure, don't assume

When v4 adopts a fix this port made first, the both-directions divergence
pins trip at the baseline move **by design** — that tells you v4 MOVED, not
HOW. Regenerate the oracle and dump v4's ACTUAL post-fix output before
retiring a pin to a plain equality: v4's adoption is often partial or
differs in wording/details the planner never checked (P4.D51's bug-8 message
heads and bug-12 phantom-row surprises are the canonical cases). A partial
convergence splits one carve-out into a converged half + a still-divergent
half — expect to reshape pins, not just delete them. When a converging arm
reddens, instrument the harness to dump the actual rows (a `QT_DEBUG_*`
-gated `eprintln!`), and for restore/import read the archive's own manifest
as ground truth.

### §5.5 Drift heals real data — 💸 proofs expire

A banked live proof that depends on **damaged rows existing** can expire
because v4 (running daily on the real instance) lands its own fix and heals
the data — the orphan-reaper and poisoned-base-URL proofs both died this
way. When a round banks such a proof, record the measurement date and count;
before building a walk step around it, **measure the population first** (one
read-only query; a zero is a finding, not a failure); when it has expired,
retire it explicitly rather than carrying it forward. Planting the damage on
the disposable copy proves the mechanism but is a weaker claim — offer it,
don't silently swap it in.

## §6 History

Absorbed drift blocks get one line each here when `/unify` moves the
baseline; the full story lives in the round record in `status-log.md`.

- **`8f910137`-round (2026-08-25, baseline `f6a10055d` → `8f9101370`):**
  `44a8137e9` ABSORBED(p4.d115 ∥ p4.d116 — the scenario-change feature whole),
  `8018c487a` ABSORBED(p4.d117 — bug 99, measured-then-ported),
  `309aaa97a` ABSORBED(p4.d117 — bugs 100/102 + the check-qt-classes guard),
  `6afacb187` ABSORBED(p4.d118 — bug 101, Tier R red-first),
  `8f9101370` NO-PORT-RATIFIED(p4.d118 — CI + tests-only; the +18 test lines
  absorbed by `completion_behavior.rs`). Round record: `status-log.md` →
  "The `8f910137` drift catch-up round".
- (This ledger was seeded 2026-08-25 with the baseline at `f6a10055d`. Drift
  older than that baseline is recorded in CLAUDE.md's round bullets and
  `claude-md-status-history.md`.)
