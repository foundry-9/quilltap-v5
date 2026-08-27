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

- **Oracle baseline:** `8872d7efc` — "perf(cheap-llm): give compression
  its own budget, and log cheap-task failures" (v4 main, 2026-08-26, the
  last commit of the 4.9.0 release push), adopted at the 4.9.0-push round
  unification (2026-08-27, P4.D126 ∥ P4.D127 ∥ P4.D128 ∥ P4.D129 + the
  unification wires).
- **Checked:** 2026-08-27 (a full `/driftcheck` — both branches by the §4
  procedure, every new commit classified from its shipped hunks).
- **v4 `main` HEAD at check:** `aec86a613` — **TWO commits past the
  baseline**, one of them portable:
  - `b6c6d7793` `docs(bugs): file bug 105` — this port's own upstream
    filing, docs-only (`docs/developer/bugs.md` + the bug file), zero
    lib/app/packages/plugins content. Carried in §3 as a NO-PORT? row so
    the next baseline move ratifies it explicitly.
  - `aec86a613` `feat(wardrobe): an outfit pull-down above the slot rows,
    garments only in the slot pickers` — a **PORT-NEW** landing on two
    already-ported SPA components plus a new pure module. Row in §3.
- **v4 `bugfix` tip at check:** `3a76b17df` — **unmoved** since the last
  check (the bare `bugfix: started 4.8.4 bug branch` fork marker,
  2026-08-13); nothing new can have landed there. Its `main..bugfix`
  commit list still shows the whole 4.7/4.8 lineage — that is the
  documented squash-topology lie (§4 step 2), not unabsorbed content.
- **Checkout at check:** branch `main`, tree **clean**.
- **Verdict: DRIFT PENDING — 1 portable commit** (2 past the baseline;
  `b6c6d7793` is docs-only). The fourteen-commit 4.9.0 block remains fully
  absorbed/ratified (§6).
- **Regen rule in force: PIN REQUIRED** — v4 HEAD is past the baseline, so
  a regen run from the checkout would import `aec86a613`'s tree. Build a
  lane-unique detached worktree at `8872d7efc` per §5.1 for **every**
  oracle regen and fixture build until the baseline moves. (The checkout
  itself is clean and on `main`, so the pin is the only precaution
  needed.)
- **Release shape:** still no `release: 4.9.0` squash and no
  `bugfix: started 4.9.0 bug branch` fork — v4 is at `4.9.0-dev.89` and
  developing on `main` alone. The standing expectation from the last two
  checks holds: re-probe BOTH branches next time rather than assuming
  `bugfix` stays inert.

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
| `b6c6d7793` | 2026-08-27 | docs(bugs): file bug 105 — the legacy-field seeding sits outside the per-item try | NO-PORT? | none — `docs/developer/bugs.md` + the bug file only, and it is **this port's own filing** (raised by P4.D126 while porting `e000d6bfc`) | UNPROCESSED |
| `aec86a613` | 2026-08-27 | feat(wardrobe): an outfit pull-down above the slot rows, garments only in the slot pickers | PORT-NEW | the SPA composer surface — `apps/web/src/app/wardrobe/outfit-composer.ts` + `equipped-slot-row.ts` (ported P4.9f2 unit 3, 2026-07-19; since touched by the hair slot P4.D87 and the container rounds P4.D112/P4.D113), the client bundle rule `apps/web/src/app/wardrobe/dissolve-bundles.ts` (P4.D72 unit 3) it builds on, and the composer's two hosts — the wardrobe dialog and the chat-start Starting Outfit panel (`screens/new-chat/outfit-selector.ts`). `help/wardrobe.md` banks to `p4.9i2`. | UNPROCESSED |

**`aec86a613` — what the hunks actually ship** (§5.3: classified from the
diff, not the message). Client-only; no server verb, no schema, no wire
change. Five shipped pieces:

1. **NEW `lib/wardrobe/composed-outfits.ts`** (41 lines) — a pure pool
   split on the *existing* `isBundle` from `lib/wardrobe/dissolve-bundles.ts`:
   `selectComposedOutfits` (filter `isBundle`, then
   `a.title.localeCompare(b.title)`) and `selectGarments` (the complement,
   caller's order). Archived items are already gone from the pool the
   composer is handed — nothing is re-filtered. Shipped with
   `__tests__/unit/lib/wardrobe/composed-outfits.test.ts` (69 lines).
   v5 has `crates/quilltap-core/src/dissolve_bundles.rs` and the SPA
   client twin, so the natural home is the SPA twin; nothing server-side
   consumes this.
2. **NEW `components/wardrobe/outfit-quick-pick.tsx`** (138 lines) — the
   `Wear an outfit…` pull-down: a `qt-button-secondary qt-button-sm` full-
   width toggle with a rotating `chevron-down`, a `role="listbox"` panel
   with an autofocused `type="search"` box, per-row
   `WARDROBE_SLOT_META[t].label` joins plus a ` · replaces` suffix when
   `outfit.replace`, `No matching outfits.` on an empty filter, outside-
   click close, and an Escape handler registered **in the capture phase
   with `stopPropagation`** so the enclosing dialog is not dismissed along
   with the menu. **Renders nothing when the pool holds no composites.**
   No v5 counterpart exists.
3. **`outfit-composer.tsx`** — mounts `<OutfitQuickPick>` *above* the
   bundle cards and slot rows, wiring `onWear` as
   `onAddToSlot(outfit.types[0]!, outfit.id)`. **No new equip path:** the
   existing callback already applies `wearItemIntoSlots` across every slot
   an item covers, so the `slot` argument names where the gesture started,
   not where the item lands. Additive bundles layer; a `replace` bundle
   sweeps its slots first; either way it dissolves into components as it
   lands.
4. **`equipped-slot-row.tsx`** — the picker candidates become
   `selectGarments(allItems)` before the existing slot/equipped/search
   filters, and the now-dead `· composite` suffix is deleted. `allItems`
   is still passed whole so equipped composite *chips* keep their labels.
   ⚠ **v5 measurably has the pre-fix shape**: `equipped-slot-row.ts:100`
   still renders `' · composite'` and `:144`'s `candidates` computed still
   offers composites.
5. **Infra/docs riders** — `help/wardrobe.md` (Composite Items + chat-start
   Manual mode), `docs/CHANGELOG.md`, and the `4.9.0-dev.87 → .89` version
   bump across `README.md` / `package.json` / `packages/quilltap/package.json`
   / `package-lock.json`.

Note for the porting lane: the prose's "a multi-slot *leaf* (a dress typed
`["top","bottom"]`) is not a composite and stays in the slot pickers" is
not a new rule — it falls out of `isBundle` testing `componentItemIds`,
which v5 already ports. Nothing in this commit changes what reaches the
server; the differential surface is the SPA specs plus a tier-1 port of the
two selectors.

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

- **The 4.9.0-push round (2026-08-27, baseline `f3892158d` → `8872d7efc`):**
  `914b59e13` + `805ef12bf` + `e000d6bfc` ABSORBED(p4.d126 — the full-wipe
  chokepoint, the variable-limit chunking, bug 103's legacy profile-column
  seeding; a v4 REGRESSION found in `e000d6bfc` itself — the helper sits
  outside the per-item try, one malformed profile aborts a whole v4 import —
  pinned v5-side, filed upstream), `964ffb959` + `8872d7efc` + `21f573039`
  ABSORBED(p4.d127 — bug 104, the per-task cheap-LLM budgets + failure warn,
  the coalesce-trace silence pin; the §1-predicted 💸 expiry executed: the
  Z.AI refusal-sentence proof retired, replaced by the glm-5.3 wire proof),
  `97d0b8f8e` + `57e7b1bc2` ABSORBED(p4.d128 — the qt-* utilities sweep,
  the four completion flags Tier R red-first), `8440b6391` ABSORBED(p4.d128,
  the AboutView hunk) + NO-PORT-RATIFIED(p4.d129, the docs remainder),
  `487ae57fe` `561466cfe` `7509c5cfb` `c0352fdba` NO-PORT-RATIFIED(p4.d129,
  each with evidence; `561466cfe`'s rider removed one vestigial v5 wardrobe
  twin), and `dcab791c2` NO-PORT-RATIFIED(p4.d129 — 410-family neutrality
  sweep + hunk measurements) **EXCEPT its title-cleaner second-trim collapse,
  which was measured NON-NEUTRAL (10/76 vectors) and ABSORBED at the
  unification wires** (both v5 cleaners + the `regen_title_quoted_padded_
  inside` tier-3 arm). Round record: `status-log.md` → "The 4.9.0-push
  drift catch-up round".

- **`f3892158d`-round (2026-08-26, baseline `b220999da` → `f3892158d`):**
  `664cfca84` ABSORBED(p4.d123 server ∥ p4.d125 client — the jobs/activity
  accounting whole), `f3892158d` ABSORBED(p4.d124 server ∥ p4.d125 client —
  the realtime subsystem whole, the hints riding v5's existing Event
  channel per the round's §Shared contract §B rather than a second
  WebSocket). Round record: `status-log.md` → "The `f3892158d` drift
  catch-up round".

- **`b220999d`-round (2026-08-26, baseline `8f9101370` → `b220999da`):**
  `b86bb1a58` ABSORBED(p4.d119 server ∥ p4.d121 SPA — the per-tier
  dressing instructions whole), `d25dacc1d` ABSORBED(p4.d120 ∥ p4.d121 —
  archive-instead-of-delete whole), `b220999da` ABSORBED(p4.d122 — the
  Documents-search vertical whole), `a47d3e034` + `2417cbed1`
  NO-PORT-RATIFIED(the two feature specs — docs-only, confirmed by the
  implementing lanes' hunk surveys; the search spec's defects paragraph is
  quoted in the p4.d122 order). Round record: `status-log.md` → "The
  `b220999d` drift catch-up round".

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
