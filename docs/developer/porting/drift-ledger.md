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

- **Oracle baseline:** `b220999da` — "feat(search): a Documents chip that
  searches every document store" (v4 main, 2026-08-25), adopted at the
  `b220999d`-round unification (2026-08-26, the P4.D119→P4.D120 ∥ P4.D121 ∥
  P4.D122 drift catch-up).
- **Checked:** 2026-08-26 (`/driftcheck`, main-checkout session, evening —
  after the `b220999d`-round dogfood pass).
- **v4 `main` HEAD at check:** `f3892158d` — **TWO commits past the
  baseline** (`664cfca84`, `f3892158d`, both 2026-08-26).
- **v4 `bugfix` tip at check:** `3a76b17df` — "bugfix: started 4.8.4 bug
  branch" (**unmoved** since the last check; the fork marker). No unabsorbed
  bugfix-side content: the content diff against main is main-forward only
  (`git diff --stat main bugfix -- lib/ app/ packages/ plugins/` = 2,711
  insertions / 32,517 deletions, i.e. bugfix *lacking* main's newer work).
- **Checkout at check:** branch `main`, tree **clean**.
- **Verdict: DRIFT PENDING — 2 commits.** Both land on already-ported
  surfaces; both are substantial (a jobs/activity-accounting rework and a
  whole new realtime subsystem). See §3.
- **Regen rule in force: PIN REQUIRED.** v4 HEAD is past the baseline —
  every oracle regeneration must run from a lane-unique detached worktree
  pinned at `b220999da` (recipe in §5.1; `recipe_sweep.py --v4 "$PIN"`),
  until a catch-up round moves the baseline.

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
| `664cfca84` | 2026-08-26 | fix(jobs): toolbar chips count whole operations, not just job rows | **PORT** | the jobs verb + `background_jobs` repo (Phase 2/3, P4.9G1 unit 1) — `GET /api/v1/system/jobs` changes wire shape; the SPA toolbar chips (P4.9P); the memory family, embeddings, the memory gate, cheap-LLM tasks, the Concierge gatekeeper, the image-generation tool handler, the wardrobe image analyzer + preview-avatar route, the character wizard, and the describe-fallback (P4.D106) each gain a span; `qt-queue-badge` CSS (P4.9P) | ORDERED(p4.d123 server ∥ p4.d125 client) |
| `f3892158d` | 2026-08-26 | feat(realtime): push interface updates over a WebSocket, tick clocks locally | **PORT-NEW** | a whole new `lib/realtime/**` subsystem with publish points inside ported code (queue-service enqueue/cancel/claim/markCompleted/markFailed, the job dispatcher's post-commit `dispatchInvalidations`, the activity registry, autonomous-room run-state transitions); the terminal WS handler's auth (`lib/terminal/ws.ts` → `quilltap-web::terminal_routes`); `lib/format-time.ts` (v5 twin in `tasks-queue.api.ts`, P4.9G1); and roughly a dozen SPA polling sites across the Salon, characters, chat cards, the tasks queue, autonomous rooms, the memory cards, and story background | ORDERED(p4.d124 server ∥ p4.d125 client) — the round decides: the hints ride v5's EXISTING Event channel (SSE `/api/events` + the Tauri pump), no second WebSocket; the orders' §Shared contract §B is the binding shape |

**Row notes** (delineation detail for `/setupphase`; classified from the
shipped hunks, not the messages):

- `664cfca84` is **not just a chip fix** — it reworks how work is counted.
  `JOB_TYPE_ACTIVITY` becomes a total `Record<BackgroundJobType,
  ActivityKind | null>` (nine previously unassigned job types gain chips), a
  new in-flight **activity registry** counts non-job work (the three inline
  image paths, `classifyContent`, `generateEmbeddingForUser`, `runMemoryGate`,
  `executeCheapLLMTask`, and four vision call sites), counting is re-entrant
  by kind, and the route's response changes: `activeByKind` +
  `startedByKind` are always present while **`activeByType` becomes opt-in**
  behind `?includeByType=true`. v5's `system_data.rs` jobs verb answers
  `activeByType` unconditionally and the SPA badges read exactly that key, so
  both ends move. Also in scope: `getStats` and the active-count query become
  indexed `COUNT(*)`s (a v5 repo change, since v5 mirrors v4's queries), the
  child→parent IPC activity mirror (v5's job runner is **in-process** — the
  IPC leg is a standing non-port, the *accounting* is not), and a poll that
  becomes a 1.5 s/8 s heartbeat instead of firing only after
  `notifyQueueChange()`. `help/system-tasks-queue.md` banks to `p4.9i2`.
- `f3892158d` is architectural. v4 adds a multiplexed WebSocket at
  `/api/v1/system/realtime/stream` carrying ~40-byte invalidation hints
  (`{v, topic, id?, at}`), never data, with a 250 ms trailing-edge debounce
  per `topic+id`; polling becomes the **fallback** everywhere, gated on
  socket health. v5 already owns a transport-agnostic `Event` channel (SSE in
  `quilltap-web`, Tauri events in the shell) — the port question is whether
  the hints ride that existing channel rather than a second socket, which is
  a round-planning decision, not one this ledger makes. Two further legs:
  (a) **auth hardening** — `lib/realtime/upgrade-auth.ts` becomes the single
  gate for BOTH WS handlers (live session, not locked, same origin) and
  replaces the terminal handler's worthless cookie test; v5 has **no session
  auth by design (D2)**, so the session leg is a likely non-port while the
  **same-origin** leg still applies to `terminal_routes.rs`, which today
  gates only on readiness; (b) **`useNow`** — a shared boundary-aligned
  ticker, with `formatRelativeDate`/`formatChatListDate` taking an optional
  `nowMs` and `formatRelativeAge` moving into `lib/format-time.ts` (v5's
  twins live in `tasks-queue.api.ts`). `server.ts`,
  `build-standalone-overlay.mjs`, and the Next HMR upgrade branch are
  v4-infra with no v5 analog. `docs/developer/features/complete/
  realtime-updates.md` (230 lines) ships inside this same commit and is the
  feature spec to read first.
- **No convergence rows:** `docs/developer/bugs.md` is unchanged since the
  baseline, so neither commit is v4 adopting a fix this port filed; no
  both-directions pin should trip on these by design.

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
