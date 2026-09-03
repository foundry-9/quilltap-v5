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

- **Oracle baseline:** `6d2a50382` — "docs(update): version bump for
  Concierge state view changes" (v4 main, 2026-09-02, v4 `4.9.0-dev.115`),
  adopted at the `6d2a50382` drift catch-up round unification (P4.D143 ∥
  P4.D144 ∥ P4.D145 ∥ P4.D146 ∥ P4.D147, 2026-09-02).
- **Checked:** 2026-09-02 (late evening — a standalone `/driftcheck` run, the
  first since the follow-ups round unified).
- **v4 `main` HEAD at check:** `b448eddd7` — "docs(bugs): file bugs 116-118
  from one mis-described image upload" (2026-09-02 16:59 -0500) — **FOUR
  commits past the baseline** (`303288fb4`, `02d4efa1b`, `c9faa2c74`,
  `b448eddd7`). `origin/main` is level with `main` (both `b448eddd7`).
- **v4 `bugfix` tip at check:** `3a76b17df` — **unmoved** (the bare 4.8.4 fork
  marker; its `main..bugfix` content list is the squash-topology lie, §4
  step 2 — re-measured by content this check, nothing genuinely unabsorbed).
- **v4 `release` tip at check:** `8736d7042` ("release: 4.8.4") — unchanged.
- **Checkout at check:** branch `main`, **tree CLEAN.** The previous §1's
  docs dirt is **DISCHARGED** — the human committed the three new bug
  filings as `b448eddd7`, so `docs/CHANGELOG.md`, `docs/developer/bugs.md`
  and `docs/developer/bugs/bug-11{6,7,8}-*.md` are now tracked and the tree
  carries nothing uncommitted. Regens are still pinned anyway (HEAD is past
  the baseline).
- **Verdict: DRIFT PENDING — 4 commits past the baseline; §3 holds FOUR
  UNPROCESSED rows** (one PORT-NEW, one PORT, one PORT log-only, one
  NO-PORT?). **None is a convergence** — `b448eddd7` only FILES bugs 116–118
  as open; v4's `bugs.md` status line still reads "Bugs **1–115** are
  **fixed in v4**", and nothing this port filed came back fixed this check.
- **Regen rule in force: PIN REQUIRED** at **`6d2a50382`** (§5.1 — a
  lane-unique detached worktree, the sweep driver's `--v4 "$PIN"`).
  **Unchanged from the previous check** — HEAD has been past the baseline
  throughout; the clean tree does not lift the pin.
- **⚠ Three OPEN v4 bugs, filed 2026-09-02 and now COMMITTED at
  `b448eddd7`** — all three from one uploaded warship screenshot that
  Quilltap described as a tabby kitten in 3,175 characters. Every one is
  marked **"Applies"** to v5 by its own `v5 status` row, so each is a future
  §3 row the moment v4 fixes it, and each carries a **v5-side measurement
  owed NOW** (a v5 measurement does not wait on v4's fix):
  **bug 116** (High) the describer's answer is believed without checking the
  image ever arrived — `describeImageWithProfile` holds two disproofs and
  reads neither (`response.usage`, logged then dropped — the offending call
  reported `promptTokens: 38` — and `LLMResponse.attachmentResults`, which
  the plugin populates for exactly this); the only check greps the text for
  refusal words, so it catches a model that admits it cannot see and never
  one that answers confidently, and the result lands on `files.description`
  where it short-circuits every later reader permanently. v4's own lesson:
  **bug 91's gate asks whether we CAN send an image, never whether one
  ARRIVED.** v5's twin is `services/file_fallback.rs` (the P4.D106/P4.D108
  describe tier).
  **bug 117** (Medium) a chat upload's `FileEntry.sha256` is the
  PRE-transcode hash (`chat-files-v2.ts` hashes the input buffer; the
  storage bridge then converts to WebP and returns the stored bytes' hash,
  used for `mimeType`/`size` and discarded for `sha256`), so every
  files ↔ document-store join fails for converted uploads — descriptions
  never reach `extractedText`, chunks or embeddings, and
  `describe_image`/`attach_image` cannot resolve a mount-link uuid;
  `images-v2` orders the same two operations correctly. Live on Friday:
  **118 of 239 uploads affected, 0 of 2,541 generated**; v4 says it needs a
  backfill migration. Measure which bytes v5's chat-upload path hashes
  (`api/chat_media.rs` / the upload bridge).
  **bug 118** (Low) the NanoGPT `manifest.json` still declares
  `attachmentSupport.supported: false` eleven versions after bug 91 —
  no runtime effect (nothing reads the field) but it is the only one of
  eleven bundled manifests that disagrees with its own code, and the only
  declaration `image-transport.test.ts` does not gate. v5's generated
  NanoGPT manifest is regenerated FROM that file, so it very likely carries
  the same stale block (measure; the generator's augmentation table is the
  fix site — memory note `manifest-generator-augmentation-rot`).
- **Release shape:** still no `release: 4.9.0` squash and no 4.9 bugfix
  fork; v4 develops on `main` alone. Keep probing BOTH branches.
- _The follow-ups round (P4.67 ∥ P4.68 ∥ P4.69 ∥ P4.70 ∥ P4.71) UNIFIED
  2026-09-02, before this check — a non-drift round, the baseline STAYED
  `6d2a50382`, no §3 row moved. Every lane and the unification regenerated
  from a pinned worktree (the lanes' own lane-unique pins, then
  `/tmp/qt-v4-pin-unify-6d2a50382` at the gate: 43 + 62 families zero SKIP).
  **The four UNPROCESSED rows are the next `/setupphase`'s first lane** —
  the drift catch-up is the recommended next move._
- _Superseded (the 2026-09-02 night verdict): DRIFT PENDING — 3 commits
  past `6d2a50382`, PIN REQUIRED, checkout DIRTY (docs only — the bug
  116–118 filings, since committed as `b448eddd7`, which is what this
  check's fourth row records)._
- _Superseded (the 2026-09-02 evening verdict): DRIFT PENDING — 1 commit
  past `6d2a50382` (`303288fb4`), PIN REQUIRED, checkout clean. The five
  follow-ups lanes resumed against that §1 and every one regenerated from
  the pin (their lane records say so, verified by drift-marker greps)._

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

| `303288fb4` | 2026-09-02 | Choose the Concierge state on the New Chat form | **PORT-NEW** | **Server:** `app/api/v1/chats/route.ts` (244 lines changed) + the NEW `__tests__/unit/app/api/v1/chats/route.concierge-state.test.ts` (444 lines): `createChatSchema` gains `conciergeState: z.enum(['monitored','flagged','vouched','uncensored']).optional()`; NEW `applyRequestedConciergeState` (no-op on absent/`'monitored'`; else `progress.status('Briefing the Concierge…')` → the P4.D141 `applyConciergeFlip` chokepoint + a `[Chats v1] Applied Concierge state at creation` debug) called at ALL THREE create branches immediately after `writeSystemPromptMessage` and before scenario/staff/greeting; the `createInitialMessages` wrapper DELETED (both call sites now write the prompt themselves); `autoGenerateFirstMessage` reads the fresh chat row and gains "attempt 0" — `shouldUseUncensoredRoute(chatRow)` → the NEW `generateViaUncensoredDesk(trigger)` closure (resolver asked WITH the chat: Vouched → `OFF` never reroutes, Uncensored reroutes under a global `OFF`); the content-filter attempt 3 now reuses the closure and SKIPS when attempt 0 ran (`uncensoredDeskTried`); five log lines (two new `trigger` fields, one new "unavailable or empty" info, one new "attempt failed" warn). `orchestrator.service.ts`: comment only. → v5 `services/chat_create.rs` (`write_system_prompt_message` `:989`, `create_initial_messages_scenario_and_staff` `:1011`, `auto_generate_first_message` `:1390` — the P4.4 unit-2 chat-create port + the P4.D44 flatten seam), `api/types.rs` `ChatCreate` (`:381`), `services/dangerous_content/manual_flip.rs` `apply_concierge_flip` (P4.D141 — today's ONLY production caller is the chat PUT), the danger resolver's chat-aware arm (P4.D141/P4.D143). Families: `chat_create_capstone_equivalence` (19 cases), `initial_greeting_equivalence`, `first_message_context_equivalence`, the danger-resolver/manual-flip families. **SPA:** `NewChatForm.tsx` (the dropdown above Starting Scenario — two optgroups, `Monitored (default)`, helper `detail` from `CONCIERGE_STATE_PRESENTATION`, the `hint` deliberately NOT shown), `useNewChat.ts` (`conciergeState` OMITTED from the POST body when `'monitored'` — a plain create stays byte-identical), `types.ts` (`conciergeState: ConciergeState`, default `'monitored'`), `NewChatModal.tsx` + `new-chat-provider.tsx` (`initialConciergeState` prop), `SalonView.tsx:1730` (Continue Elsewhere seeds it from `getConciergeState(chat)`), two client test files (the scenario test now reaches its select by id because the form has two). → v5 `screens/new-chat/{new-chat-form, new-chat.state, new-chat.logic, new-chat.types}.ts` (the P4.9 New-Chat vertical + the P4.D44 template picker), the Continue Elsewhere dialog (P4.9E3), `chat/concierge-state-presentation.ts` (P4.D144's ONE table — the `detail` sentences already there). **Help:** `help/chats.md` (+15, "A Word With the Concierge, Before the Doors Open"), `help/dangerous-content.md` (two sentences) → the `p4.9i2` bank. **Docs/infra (NO-PORT):** `API.md`, the feature design doc `docs/developer/features/complete/concierge-default-at-creation.md` (read it — §3 "Wire contract", §4 "Applying the state", §5 "Greeting routing"), `releases/4.9.0.md`, `README.md`, `scripts/concierge-four-state-test.sh` CT-3, `.claude/commands/update-documentation.md`, `package-lock.json`. No schema / migration / export-schema / backup change (the commit's own claim, CONFIRMED by the file list — no `generateDDL`, no D23 re-dump). | UNPROCESSED |
| `02d4efa1b` | 2026-09-02 | fix(memory): the turn-blocking distillation asks for the interactive budget (bug 115) | **PORT** | v4's own filing (bug 115 — fixed same day, `docs/developer/bugs/fixed/bug-115-interactive-distill-background-budget.md`; NOT a convergence). **Hunks:** `lib/memory/cheap-llm-tasks/memory-tasks.ts` — `extractMemorySearchKeywords` gains a trailing `latency: CheapLLMLatencyClass = 'background'` parameter threaded to `executeCheapLLMTask` as `{ latency }`; `lib/chat/context-manager.ts:1398` — the dynamic-head FALLBACK distill (the branch that runs when no proactive pre-compute pass has a query ready) passes `'interactive'` (45 s, no retry) with a nine-line why-comment; the proactive pass (`pre-compute.service.ts`) and the `recall-replay` diagnostic keep the default. Two new tests (`dynamic-head-distill-latency.test.ts` asserts the 8th argument is `'interactive'`; four `task-deadline.test.ts` cases pin the budget/retry arithmetic bug 107 shipped untested). "Completes bug 107" — the third inline cheap call 107's task-type histogram could not see. → v5: `services/memory_recap/distill.rs:340 distill_memory_search` (no latency parameter — the executor's `execute(…)` takes the budget seam P4.D136 added; today every distill caller passes the same default), its THREE call sites `services/build_context.rs:2339` (the fallback — must become interactive), `services/pre_compute.rs:288` + `services/recall_replay.rs:275` (stay background). The P4.D136 lane record threaded the latency class from 45 call sites and named the recap + compression legs; this is the one v4 itself missed, so v5 reproduces bug 115 today. Families: `build_context_tier3`, `precompute_equivalence`, `recall_replay_equivalence` — a deadline class is differential-INVISIBLE on a canned executor, so the pin is P4.D136's compile-pin/unit idiom (a mutation-proven per-site assertion that the fallback site's class is `Interactive`), not a corpus row. | UNPROCESSED |
| `c9faa2c74` | 2026-09-02 | fix(context): restore the inter-character memory timing log | **PORT (log-only)** | ONE hunk in `lib/chat/context-manager.ts:1755`: the empty `if (isMultiCharacter) { }` block (emptied by the `96bf74b5b` debug-strip chore) regains `logger.debug('[ContextManager] Inter-character memory retrieval complete', {chatId, characterId, durationMs: Math.round(performance.now() - tInterStart), loadedCount: interCharacterLoadedCount, includedCount: interCharacterMemoriesIncluded})`. Rest is version bumps (`4.9.0-dev.117` → `.118`), README, CHANGELOG. → v5 `services/build_context.rs` has NO such line (grep `Inter-character memory retrieval complete` → nothing) — a log-only port with a thread-scoped capture pin (memory notes `differential-blind-to-a-log-only-fix`, `a-process-global-test-seam-must-be-thread-scoped`); the three fields need v5's inter-character load count + a duration stamp at the site. Joins the handler-logging sweep's row list; small enough to ride the bug-115 catch-up. | UNPROCESSED |
| `b448eddd7` | 2026-09-02 | docs(bugs): file bugs 116-118 from one mis-described image upload | **NO-PORT?** | **Docs only — ZERO `lib/`, `app/`, `packages/`, `plugins/`.** Five files: `docs/CHANGELOG.md` (+30), `docs/developer/bugs.md` (the Status paragraph rewritten — "Bugs **1–115** are **fixed in v4**. **116–118** are **open**"), and the three NEW filings `docs/developer/bugs/bug-116-describer-answer-never-verified.md` (198), `bug-117-file-entry-sha-pre-transcode.md` (207), `bug-118-nanogpt-manifest-attachment-drift.md` (121). No code hunk to port, so this row ratifies as NO-PORT at the next baseline move — **but it is not inert**: all three bugs are marked **"Applies"** to v5 by their own `v5 status` rows, and the v5-side MEASUREMENTS are owed now (§1's ⚠ bullet carries the shapes and the v5 surfaces: `services/file_fallback.rs` for 116, `api/chat_media.rs` / the upload bridge for 117, the manifest generator's augmentation table for 118). This commit is also what DISCHARGES the previous §1's dirty-tree note — the same three files were the untracked dirt recorded there. **Not a convergence:** it files v4's own new bugs as OPEN; nothing this port filed came back fixed. | UNPROCESSED |

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

- **The `6d2a50382` drift catch-up round (2026-09-02, baseline `4622411fd` →
  `6d2a50382`):** `70505745a` ABSORBED(p4.d146 — the presence gate at the
  three story-background sites incl. the reworded 400, the
  `backgroundDisplayMode` normalizer at the overlay parse + the narrowed
  update schema + the GET's dead arms deleted, the SPA card; the committed
  `cost-background` pair and the story builder widened so the gates can be
  seen), `a00e18f0d` ABSORBED(p4.d147 — v5 had NO folder picker; v4's
  post-fix `FolderPicker` built fresh over the existing verbs with the live
  beat), `a5df98b3f` ABSORBED(p4.d145 — the unique-constraint predicate,
  `ensure_by_path` over the seven create sites with the two private lookups
  deleted, the collapse-then-index boot ensure in the index-guarded idiom
  with NO ledger row, the restore quiet-drop arm + a new committed archive;
  the order's provisioning-hook suggestion REFUTED by measurement),
  `f3351d54f` NO-PORT-RATIFIED(p4.d143 — three files, zero lib/app),
  `c43d3b1b4` ABSORBED(p4.d143 server ∥ p4.d144 SPA — the derived
  `conciergeState`/`dangerCategories` pair on all four list payloads, the
  predicate delegation, the per-turn enqueue guard, the `has-dangerous`
  probe v5 never had; the presentation table once in the SPA, the mark, the
  pill, `shouldHideChat` as the one rule), `6d2a50382`
  NO-PORT-RATIFIED(p4.d143 — four version files). Round record:
  `status-log.md` → "Round record — the `6d2a50382` drift catch-up round
  unification".

- **The P4.D138 follow-up (2026-09-01, baseline unchanged at `4622411fd`;
  the LoRA train's three PARTIAL rows completed):** `84f33ce94`
  ABSORBED(p4.d138 units 1–4 + unit 6 ∥ p4.d139 — the read side landed:
  `list-models` `loraSupport`, the `options-schema` action, the NanoGPT
  detailed-catalog cache with the augmentation arm the unit-1 narrowing had
  named; the tripwire fired and was deleted; the two SPA beats live),
  `648d5c8aa` ABSORBED(p4.d138 unit 5 — bug 110's family-first `apply_loras`
  with the corpus re-recorded at the tip, exactly the two predicted rows
  moving; bug 111's error-level request log + v4's debug line, both
  capture-pinned), `2ece98c90` ABSORBED(p4.d138 unit 7 ∥ p4.d139 — the
  HuggingFace lookup + repo-id twins with a 57-row differential over v4's
  real modules, the `lora-metadata` action live behind an engine gate, the
  host transport; one recorded divergence, V8's own `SyntaxError` wording).
  Round record: `status-log.md` → "Round record — the P4.D138 follow-up
  unification".

- **The round-2 drift catch-up (2026-09-01, baseline `7fb668263` →
  `4622411fd`):** `5f56f7a7d` ABSORBED(p4.d142 — `.qt-range` + tokens
  byte-identical, all twelve v5 range hosts adopted, dogfood #107's
  `qt-markdown-field` rule, the host-class guard at the ordered NARROW
  scope + the `--self-test` landed at unification), `735d9408c`
  ABSORBED(p4.d140 — bug 112 whole: the `chat_activity` chokepoint with
  its tier-1 family, both write sites red-first, the six readers, the
  restore re-derive, the ai-import twin NO-COUNTERPART, the boot recompute
  heal in the P4.D97 ledger shape with its own family, the four SPA
  display flips, the e2e seed landmine repaired; plus the out-of-mandate
  `allowCheapFallback` P4.D135 remainder), `e41fcb12e`
  NO-PORT-RATIFIED(p4.d138 — CHANGELOG + two help files, +34, zero
  lib/app/packages/plugins; help hunks banked to `p4.9i2`), `4622411fd`
  NO-PORT-RATIFIED(the unify — `docs/releases/4.9.0.md` alone, +70/−6),
  `60e3c4a0a` ABSORBED(p4.d141 — the Concierge four-state whole: the
  predicate family reshape at every call site, the resolver's operator
  arms, the flips + writer sentences byte-exact, the `conciergeState` PUT
  arm closing v5's long-named deferral, the classifier-gate corpora that
  can finally see the gate, the SPA control + single-pill badge + client
  twin, the four-state walk live; the §3 review's sidebar-latch fix and
  the broken `post_office_writers_tier3` family repaired at unification).
  The LoRA train (`84f33ce94` → `648d5c8aa` → `2ece98c90`) is PARTIAL —
  the whole client half (p4.d139) + p4.d138 units 1–4; units 5–7 stay in
  §3 as PARTIAL rows and in the order's resume list. The §3 unification
  review (five parallel readers over six lanes) fixed six finding groups
  on the unify branch — headline: the Concierge select's permanent
  optimistic latch, the optimistic-bubble echo scoped across two clocks,
  and a harness family the kind rename had silently broken. Round record:
  `status-log.md` → "Round record — the drift catch-up round 2 of 2
  unification".

- **The round-1 drift catch-up (2026-09-01, baseline `b121ac77f` →
  `7fb668263`):** `1560bd43b` ABSORBED(p4.d134 — the Lima/WSL2 retirement
  whole: env/lock/CLI, the data-dir `isVM` wire deletion with two renamed
  deletion pins, the host-rewrite two-strategy collapse, self-inventory/
  almanack retirements, the SPA About/footer/profile mirrors; the grep
  census; the unported host gateway resolver named as a follow-up order),
  `7819afb1d` + `3c3432ae9` NO-PORT-RATIFIED(p4.d134, file lists in the
  lane record), `65f5021c8` ABSORBED(p4.d135 — provider/model fallback
  chains whole: the D23 re-dump with the generateDDL-position correction,
  the pure engine tier-1 at 155→158 cases, the Salon spine both
  entrances, cheap-LLM + image-description sites, both id-remap paths,
  the delete-nulls cascade with the updatedAt stamp, the SPA mirrors +
  the live understudy round-trip beat), `97ebfb9fc`
  NO-PORT-RATIFIED(p4.d136 — both bug files' v5-relevance columns quoted
  in the lane record), `a1d88aa3a` ABSORBED(p4.d136 — bug 106 proven
  red-first in both halves + the three-spellings consolidation; bug 107's
  budget rewrite with the latency class threaded from 45 call sites, the
  timeout-only retry, five of six handler guards [scene-state deferred
  loud], the C4 dogfood row superseded per §5.5; the outfit-consult
  bound inversion measured as v4's own and reproduced), `487ae16b1`
  ABSORBED(p4.d137 — bugs 108/109 red-first: the argument guards
  byte-exact, the fold module + rebuilt per-UTF-16-unit diacritics map,
  the 5/25 replay split executable), `7fb668263` ABSORBED(p4.d134's About
  rider; README/CHANGELOG_V4 remainder NO-PORT). The §3 unification
  review fixed four finding groups on the unify branch — headline: the
  `[CheapLLM] Task failed` warn fired AFTER the chain instead of before
  it (v4 warns first; the counter bug 107 was measured from would have
  under-counted). Round record: `status-log.md` → "The drift catch-up
  round 1 of 2 unification".

- **The P4.D131 round (2026-08-27, baseline `aec86a613` → `b121ac77f`):**
  `679e450e3` ABSORBED(p4.d131 — the bug-105 CONVERGENCE retired by
  measurement per §5.4: FULL convergence, no residue; the arm — code name
  `execute_bug105_seed_abort`, the row's `import_aborts_on_non_string_
  provider` was its class description — is now a plain state-compared
  regression guard; the retirement measurably WIDENED coverage, the
  formerly-subtracted `main.image_profiles` table now discriminating),
  `0bd841394` + `1b0ce9eba` ABSORBED(p4.d132 — the Tooltip primitive +
  nine-button adoption + the ConfirmationBadge net-new + the deletion
  rider; help rows banked to `p4.9i2`; theme-storybook NO-PORT),
  `b121ac77f` ABSORBED(p4.d133 — `instances restore-key` whole, Tier R
  188 → 212; the row's ⚠ scope question resolved at ordering — the write
  proved in-sandbox via `reset_live`, the real-pepper walk banked 💸; the
  NO-PORT remainder RATIFIED: README/docs/help/package files + v4's two
  new test files, their behavior carried by the Tier R arms + unit pins).
  Round record: `status-log.md` → "The P4.D131 ∥ P4.D132 ∥ P4.D133 ∥
  P4.65 round unification".

- **The P4.D130 round (2026-08-27, baseline `8872d7efc` → `aec86a613`):**
  `aec86a613` ABSORBED(p4.d130 — the outfit pull-down + garments-only slot
  pickers, SPA-only; the composed-outfits selectors' recorded-vector corpus
  pinned at the drift commit, re-proven byte-identical after mid-lane
  drift), `b6c6d7793` NO-PORT-RATIFIED(this round — docs-only, this port's
  own bug-105 filing; file list verified `docs/developer/bugs.md` + the bug
  file, zero lib/app/packages/plugins content). Round record:
  `status-log.md` → "The P4.D130 ∥ P4.62 ∥ P4.63 ∥ P4.64 round".

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
